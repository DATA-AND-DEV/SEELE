//! **O sinal de quem só ouve segue a perda do que ele ouve** (protocolo 13).
//!
//! O buraco que o `seele_server::sinal` descrevia: de quem está calado e sem
//! vídeo o servidor não mede perda nenhuma, e o sinal dessa pessoa refletia só
//! o atraso do caminho. A perda do que ela **ouve** é medida no cliente dela,
//! e não atravessava o fio. Agora atravessa — o cliente relata a própria
//! recepção uma vez por segundo (`ClientMessage::RelatoDeRecepcao`) e o
//! servidor a põe na conta do sinal daquela pessoa.
//!
//! # O que este arquivo põe de pé
//!
//! O produto dos dois lados, sem mensagem escrita à mão:
//!
//! - um `seeled` em processo;
//! - **quem fala**: uma `Connection` da FFI com o aparelho de mentira (o tom
//!   contínuo do microfone de mentira), sentada, microfone aberto;
//! - **quem só ouve**: outra `Connection` da FFI, **muda e sem vídeo**,
//!   sentada na mesma sala, atrás de um [`Cano`] — o repassador UDP que perde
//!   pacotes de verdade num sentido só.
//!
//! O que se lê é o `signal` de quem só ouve **como o servidor o anuncia aos
//! outros**: no retrato de quem fala, que é onde a lista de pessoas o mostra.
//!
//! # As três cenas
//!
//! - perda só na **descida de quem ouve**: o sinal dela cai, o de quem fala
//!   não;
//! - sem perda: os dois ficam altos;
//! - perda só na **subida de quem fala**: o sinal de quem fala cai, e o de quem
//!   ouve **não** — o cliente dela relata a mesma perda (o buffer de jitter não
//!   sabe de quem é o buraco), e o servidor desconta a subida que ele mesmo
//!   mediu (`seele_server::sinal::descontar`). Antes do desconto esta cena
//!   reprovava com o sinal de quem ouve em 72.
//!
//! # Por que aqui, e não ao lado de `sinal_segue_a_rede.rs`
//!
//! O relato tem de vir do caminho real — o buffer de jitter, a janela, o
//! relator, o portão de versão, o motor do enlace —, e o ADR 0002 proíbe o
//! `seele-server` de ver o cliente. Este é o único crate onde as duas pontas
//! cabem juntas. A metade do servidor, com o relato escrito à mão, está em
//! `seele-server/tests/relato_de_recepcao.rs`.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use seele_audio::device::mentira;
use seele_enlace::teste::{Cano, ConfigDoCano, Sentido};
use seele_ffi::foco::FocoDeVoz;
use seele_ffi::{ConnectConfig, Connection, VoiceMode};
use seele_proto::signal::NOMINAL_FLOOR;
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

mod vaga;

const PRAZO: Duration = Duration::from_secs(15);

/// Quanto cada cena observa depois de a voz chegar a quem ouve.
///
/// O relato sai uma vez por segundo e só tem medida depois de dois segundos
/// de fala na janela; o servidor anuncia o sinal uma vez por segundo, e a 10%
/// de perda a suavização cruza o piso nominal no quarto anúncio. Vinte
/// segundos deixam o último terço (o que as asserções olham) começar bem
/// depois disso, com folga para uma máquina carregada.
const OBSERVACAO: Duration = Duration::from_secs(20);

async fn servidor() -> Result<(SocketAddr, Arc<Daemon>)> {
    let daemon = Arc::new(
        Daemon::bind(ServerConfig {
            name: "Casa".into(),
            listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            database: Location::Memory,
            caminho_bps: Some(10_000_000),
            ..ServerConfig::default()
        })
        .await?,
    );
    let endereco = daemon.local_addr()?;
    let servindo = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = servindo.run().await;
    });
    Ok((endereco, daemon))
}

/// Uma pessoa pela FFI, com o aparelho de mentira `aparelho` e um foco de voz
/// só dela (as duas vozes ficam abertas ao mesmo tempo neste processo).
async fn pela_ffi(
    endereco: SocketAddr,
    casa: &std::path::Path,
    apelido: &str,
    aparelho: &str,
) -> Arc<Connection> {
    let config = ConnectConfig {
        server: endereco.to_string(),
        alternate_servers: Vec::new(),
        nickname: apelido.into(),
        home: casa.display().to_string(),
        join_secret: None,
        expected_fingerprint: None,
        codigo_esperado: None,
        bilhete: None,
        audio: true,
        capture_device: Some(mentira::id(aparelho)),
        playback_device: Some(mentira::id(aparelho)),
    };
    let (conexao, _) = tokio::task::spawn_blocking(move || {
        Connection::connect_com_foco(config, None, None, Some(FocoDeVoz::novo()), None)
    })
    .await
    .expect("a thread que conecta caiu")
    .unwrap_or_else(|falha| panic!("{apelido} não entrou: {falha:?}"));
    conexao
}

async fn ate(prazo: Duration, mut pronto: impl FnMut() -> bool) -> bool {
    let comeco = Instant::now();
    while comeco.elapsed() < prazo {
        if pronto() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    pronto()
}

/// O sinal de `pessoa` como `de_quem_olha` o vê na lista da sala.
fn sinal_de(de_quem_olha: &Connection, pessoa: u64) -> Option<u8> {
    de_quem_olha
        .snapshot()
        .voice_rooms
        .iter()
        .flat_map(|sala| sala.people.iter())
        .find(|alguem| alguem.id == pessoa)
        .map(|alguem| alguem.signal)
}

/// O que uma cena mediu.
struct Cena {
    /// O sinal de quem só ouve, como o servidor o anunciou a quem fala: uma
    /// amostra por segundo.
    de_quem_ouve: Vec<u8>,
    /// O de quem fala, visto por quem ouve.
    de_quem_fala: Vec<u8>,
    /// Do servidor para quem ouve: quantos datagramas o cano repassou e
    /// quantos tirou.
    descida: (u64, u64),
    /// De quem fala para o servidor: o mesmo.
    subida: (u64, u64),
    /// Blocos com som que a saída de quem ouve tocou durante a observação.
    tocados: u64,
}

/// A cena: cada pessoa atrás de um cano. O de quem só ouve perde
/// `perda_na_descida` no sentido servidor→cliente e nada no outro; o de quem
/// fala perde `perda_na_subida` no sentido cliente→servidor e nada no outro.
async fn cena(perda_na_descida: f64, perda_na_subida: f64, semente: u64) -> Result<Cena> {
    let (endereco, daemon) = servidor().await?;
    let cano = Cano::abrir(
        endereco,
        ConfigDoCano {
            ida: Sentido::limpo(),
            volta: Sentido {
                perda: perda_na_descida,
                ..Sentido::limpo()
            },
            semente,
        },
    )
    .await?;
    let cano_de_quem_fala = Cano::abrir(
        endereco,
        ConfigDoCano {
            ida: Sentido {
                perda: perda_na_subida,
                ..Sentido::limpo()
            },
            volta: Sentido::limpo(),
            semente: semente + 1000,
        },
    )
    .await?;

    // Quem só ouve: atrás do cano, muda, sem vídeo.
    let casa_de_quem_ouve = tempfile::tempdir()?;
    let aparelho_de_quem_ouve = format!("so-ouve-{semente}");
    let ouve = pela_ffi(
        cano.endereco(),
        casa_de_quem_ouve.path(),
        "ouvinte",
        &aparelho_de_quem_ouve,
    )
    .await;
    ouve.set_muted(true).expect("mudo");
    let sala = ouve.snapshot().voice_rooms[0].id;
    ouve.enter_voice_room(sala, None).expect("quem ouve sentou");
    let eu_que_ouco = ouve.snapshot().me.expect("quem ouve sabe quem é");

    // Quem fala: atrás do cano dela, microfone aberto.
    let casa_de_quem_fala = tempfile::tempdir()?;
    let aparelho_de_quem_fala = format!("fala-{semente}");
    let fala = pela_ffi(
        cano_de_quem_fala.endereco(),
        casa_de_quem_fala.path(),
        "falante",
        &aparelho_de_quem_fala,
    )
    .await;
    fala.set_voice_mode(VoiceMode::Open);
    fala.enter_voice_room(sala, None).expect("quem fala sentou");
    let eu_que_falo = fala.snapshot().me.expect("quem fala sabe quem é");

    // A voz tem de estar chegando a quem ouve antes de a observação começar:
    // sem ela não há recepção a medir, e o teste não provaria nada.
    let tocados = || mentira::contagem(&aparelho_de_quem_ouve).blocos_com_som;
    assert!(
        ate(PRAZO, || tocados() >= 50).await,
        "a voz de quem fala não chegou à saída de quem só ouve"
    );
    assert!(
        ate(PRAZO, || sinal_de(&fala, eu_que_ouco).is_some()
            && sinal_de(&ouve, eu_que_falo).is_some())
        .await,
        "as duas pessoas não se veem na sala"
    );

    let antes = tocados();
    let fim = Instant::now() + OBSERVACAO;
    let mut de_quem_ouve = Vec::new();
    let mut de_quem_fala = Vec::new();
    while Instant::now() < fim {
        tokio::time::sleep(Duration::from_secs(1)).await;
        de_quem_ouve.push(sinal_de(&fala, eu_que_ouco).expect("quem ouve saiu da sala"));
        de_quem_fala.push(sinal_de(&ouve, eu_que_falo).expect("quem fala saiu da sala"));
    }
    let descida = cano.volta();
    let subida = cano_de_quem_fala.ida();
    let cena = Cena {
        de_quem_ouve,
        de_quem_fala,
        descida: (descida.repassados, descida.perdidos_no_sorteio),
        subida: (subida.repassados, subida.perdidos_no_sorteio),
        tocados: tocados() - antes,
    };
    println!(
        "perda na descida de quem ouve {:.0}%, na subida de quem fala {:.0}%: sinal de quem só \
         ouve {:?}; de quem fala {:?}; na descida o cano repassou {} e tirou {}; na subida, {} e \
         {}; a saída de quem ouve tocou {} blocos com som",
        perda_na_descida * 100.0,
        perda_na_subida * 100.0,
        cena.de_quem_ouve,
        cena.de_quem_fala,
        cena.descida.0,
        cena.descida.1,
        cena.subida.0,
        cena.subida.1,
        cena.tocados,
    );

    fala.disconnect();
    ouve.disconnect();
    daemon.shutdown();
    Ok(cena)
}

/// O último terço das amostras: o regime, depois de a média móvel assentar.
fn regime(sinais: &[u8]) -> &[u8] {
    sinais.get(sinais.len() * 2 / 3..).unwrap_or(sinais)
}

/// **O buraco, fechado, pelo caminho de verdade.** Dez por cento de perda só
/// no que o servidor manda a quem ouve: ela está calada e sem vídeo, o
/// servidor não mede perda nenhuma dela, e o sinal dela cai porque **ela**
/// relata o que ouve. Quem fala, num caminho limpo, fica alto.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn com_perda_so_na_descida_o_sinal_de_quem_esta_calado_cai() -> Result<()> {
    let _vaga = vaga::minha();
    let cena = cena(0.10, 0.0, 31).await?;

    // A cena aconteceu como descrita: a voz passou pelo cano, o cano tirou
    // pacotes, e ainda assim quem ouve ouviu.
    assert!(
        cena.descida.0 > 500 && cena.descida.1 > 20,
        "a voz não passou pelo cano, ou o cano não tirou nada: {:?}",
        cena.descida
    );
    assert!(
        cena.tocados > 500,
        "quem ouve quase não ouviu: {}",
        cena.tocados
    );

    assert!(
        regime(&cena.de_quem_ouve)
            .iter()
            .all(|s| *s < NOMINAL_FLOOR),
        "10% de perda no que ela ouve e o sinal de quem está calado continuou nominal: {:?}",
        cena.de_quem_ouve
    );
    assert!(
        regime(&cena.de_quem_fala)
            .iter()
            .all(|s| *s >= NOMINAL_FLOOR),
        "a perda na descida de quem ouve derrubou o sinal de quem fala: {:?}",
        cena.de_quem_fala
    );
    Ok(())
}

/// A mesma cena sem perda: o relato existe e diz que está tudo bem, e o
/// sinal de quem só ouve fica alto. É o que separa «o relato derruba o sinal»
/// de «a perda relatada derruba o sinal».
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sem_perda_o_sinal_de_quem_esta_calado_fica_alto() -> Result<()> {
    let _vaga = vaga::minha();
    let cena = cena(0.0, 0.0, 32).await?;
    assert!(
        cena.tocados > 500,
        "quem ouve quase não ouviu: {}",
        cena.tocados
    );
    assert!(
        regime(&cena.de_quem_ouve)
            .iter()
            .all(|s| *s >= NOMINAL_FLOOR),
        "sem perda nenhuma o sinal de quem só ouve não ficou alto: {:?}",
        cena.de_quem_ouve
    );
    assert!(
        regime(&cena.de_quem_fala)
            .iter()
            .all(|s| *s >= NOMINAL_FLOOR),
        "sem perda nenhuma o sinal de quem fala não ficou alto: {:?}",
        cena.de_quem_fala
    );
    Ok(())
}

/// **A perda de quem fala é de quem fala.** Dez por cento de perda só na
/// **subida** de quem fala: quem ouve recebe a voz com os mesmos buracos, e o
/// buffer de jitter dela não tem como saber de quem eles são. O sinal de quem
/// fala cai (o servidor mede a subida da voz dela); o de quem só ouve, num
/// caminho limpo, **não** pode cair junto — senão uma pessoa com a subida ruim
/// pintaria a sala inteira, e o indicador deixaria de dizer quem é.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_perda_na_subida_de_quem_fala_nao_cai_no_sinal_de_quem_ouve() -> Result<()> {
    let _vaga = vaga::minha();
    let cena = cena(0.0, 0.10, 33).await?;
    assert!(
        cena.subida.0 > 500 && cena.subida.1 > 20,
        "a voz não subiu pelo cano, ou o cano não tirou nada: {:?}",
        cena.subida
    );
    assert!(
        cena.tocados > 500,
        "quem ouve quase não ouviu: {}",
        cena.tocados
    );

    assert!(
        regime(&cena.de_quem_fala)
            .iter()
            .all(|s| *s < NOMINAL_FLOOR),
        "10% de perda na subida de quem fala e o sinal dela continuou nominal: {:?}",
        cena.de_quem_fala
    );
    assert!(
        regime(&cena.de_quem_ouve)
            .iter()
            .all(|s| *s >= NOMINAL_FLOOR),
        "a perda na subida de quem fala caiu no sinal de quem só ouve, num caminho limpo: {:?}",
        cena.de_quem_ouve
    );
    Ok(())
}
