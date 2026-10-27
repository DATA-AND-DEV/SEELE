//! **A volta ao servidor com o mesmo `ssrc`, ouvida por quem ficou** (o achado
//! do CUA de 25/10, 03:39): depois de uma fala LONGA, a pessoa sai do servidor
//! inteiro (o SAIR da trilha: `disconnect`, sentada), volta com a mesma
//! identidade e senta. A ponta que ficou, um RECEPTOR PERSISTENTE que não se
//! recria, deixa de tocar a voz dela por um tempo da ordem da vida da voz
//! velha, e depois volta sozinha.
//!
//! O que se mede, com meios sintéticos (o aparelho de mentira dos dois lados,
//! nenhum microfone ou saída da máquina):
//! - no fio, por um par cru sentado na sala: o `ssrc`, a `seq` e o carimbo do
//!   último datagrama da pessoa antes de sair e do primeiro depois de voltar;
//! - no receptor (uma `Connection` da FFI, muda, com a saída de mentira): o
//!   tempo, depois de a pessoa voltar a falar no fio, até o primeiro bloco com
//!   som tocado, com a contagem zerada por fase.
//!
//! A causa, medida por ele: o servidor devolve o `ssrc` da reserva da graça
//! (`Slots::reclaim`) também a quem saiu de propósito, a `Voice` da conexão
//! nova recomeçava o relógio de mídia do zero, e o buffer de jitter do
//! receptor, que já tocou até o carimbo alto da voz velha, descartava como
//! atrasado (`JitterBuffer::push`, `ts_delta(..) < 0`) todo quadro novo até o
//! carimbo novo alcançar o velho. Sem o reparo: o mesmo `ssrc`, a `seq` e o
//! carimbo de volta a 1 e 960, e 21 s de mudo depois de 20 s de voz.
//!
//! O reparo é de quem manda (`seele_voz::relogio::partida`): a voz nova parte
//! do relógio da máquina, à frente da velha pelo tempo fora.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_audio::device::mentira;
use seele_core::enlace::{Destino, Enlace};
use seele_core::{MemoryPinStore, PinStore};
use seele_ffi::foco::FocoDeVoz;
use seele_ffi::{ConnectConfig, Connection, VoiceMode};
use seele_proto::MediaHeader;
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

mod vaga;

const PRAZO: Duration = Duration::from_secs(15);

/// Quanto a voz velha vive antes de a pessoa sair. O carimbo conta toda
/// amostra que passou desde que a voz abriu, falando ou não.
const VIDA_DA_VOZ_VELHA: Duration = Duration::from_secs(20);

/// Até quanto, depois de a voz nova chegar no fio, o receptor tem de tocá-la.
/// Folgado: o alvo do buffer é 40 ms, e um receptor são toca em décimos.
const TETO_DA_VOLTA: Duration = Duration::from_secs(3);

/// Blocos de 10 ms com som que contam como «tocou»: meio segundo.
const TOCOU: u64 = 50;

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

/// O que o fio mostrou de cada datagrama: quando, e o cabeçalho.
type Fio = Arc<Mutex<Vec<(Instant, MediaHeader)>>>;

/// Um par cru sentado, que guarda o cabeçalho de cada datagrama de voz.
async fn quem_le_o_fio(endereco: SocketAddr, fio: Fio) -> Result<Enlace> {
    let enlace = Enlace::conectar(
        Destino {
            servidor: endereco,
            nome_tls: "localhost".into(),
            chave_do_pin: endereco.to_string(),
            apelido: "fio".into(),
            segredo: None,
            esperada: None,
            o_pino_prova_o_servidor: true,
            aceito: None,
        },
        SigningKey::from_bytes(&[91; 32]),
        Arc::new(MemoryPinStore::new()) as Arc<dyn PinStore>,
    )
    .await?;
    let sala = enlace.sessao().voice_rooms[0].id;
    enlace.entrar_na_voice_room(sala, None).await?;
    let canal = enlace.media();
    tokio::spawn(async move {
        while let Ok(bytes) = canal.next().await {
            if let Ok((cabecalho, _)) = MediaHeader::decode(&bytes) {
                fio.lock().unwrap().push((Instant::now(), cabecalho));
            }
        }
    });
    Ok(enlace)
}

/// Uma pessoa pela FFI, com o aparelho de mentira `aparelho` e o foco dado.
async fn pela_ffi(
    endereco: SocketAddr,
    casa: &std::path::Path,
    apelido: &str,
    aparelho: &str,
    foco: Arc<FocoDeVoz>,
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
        Connection::connect_com_foco(config, None, None, Some(foco), None)
    })
    .await
    .expect("a thread que conecta caiu")
    .unwrap_or_else(|falha| panic!("a pessoa não entrou: {falha:?}"));
    conexao.set_voice_mode(VoiceMode::Open);
    conexao
}

/// Sentar como a janela senta: sem o foco, foca antes.
async fn sentar_como_a_janela(foco: &Arc<FocoDeVoz>, conexao: &Arc<Connection>) {
    if !conexao.tem_o_foco_de_voz() {
        let alvo = conexao.id_no_foco();
        let foco = Arc::clone(foco);
        tokio::task::spawn_blocking(move || foco.focar(alvo))
            .await
            .expect("a troca caiu")
            .expect("a troca do foco foi recusada");
    }
    let sala = conexao.snapshot().voice_rooms[0].id;
    conexao.enter_voice_room(sala, None).expect("sentar");
}

async fn ate(prazo: Duration, mut pronto: impl FnMut() -> bool) -> Option<Duration> {
    let comeco = Instant::now();
    while comeco.elapsed() < prazo {
        if pronto() {
            return Some(comeco.elapsed());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    pronto().then(|| comeco.elapsed())
}

/// O último cabeçalho do fio até agora, e quantos chegaram.
fn ultimo(fio: &Fio) -> (usize, Option<MediaHeader>) {
    let fio = fio.lock().unwrap();
    (fio.len(), fio.last().map(|(_, cabecalho)| *cabecalho))
}

#[tokio::test(flavor = "multi_thread")]
async fn quem_ficou_ouve_a_pessoa_que_saiu_e_voltou_depois_de_uma_fala_longa() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;
    let fio: Fio = Arc::default();
    let _le = quem_le_o_fio(endereco, Arc::clone(&fio)).await?;

    // O RECEPTOR PERSISTENTE: sentado, mudo (o fio só leva a pessoa), com a
    // saída de mentira contando o que toca. Nunca se recria.
    let casa_do_receptor = tempfile::tempdir()?;
    let receptor_aparelho = "mesmo-ssrc-receptor";
    let receptor = pela_ffi(
        endereco,
        casa_do_receptor.path(),
        "receptor",
        receptor_aparelho,
        FocoDeVoz::novo(),
    )
    .await;
    receptor.set_muted(true).expect("mudo");
    let sala = receptor.snapshot().voice_rooms[0].id;
    receptor
        .enter_voice_room(sala, None)
        .expect("o receptor sentou");
    let tocados = || mentira::contagem(receptor_aparelho).blocos_com_som;

    // 1. A pessoa entra, senta e fala, LONGO.
    let foco = FocoDeVoz::novo();
    let casa = tempfile::tempdir()?;
    let aparelho = "mesmo-ssrc-pessoa";
    let primeira = pela_ffi(endereco, casa.path(), "dona", aparelho, Arc::clone(&foco)).await;
    sentar_como_a_janela(&foco, &primeira).await;
    assert!(
        ate(PRAZO, || mentira::contagem(aparelho).abertos == 1)
            .await
            .is_some(),
        "a voz da pessoa não abriu"
    );
    let base = tocados();
    assert!(
        ate(PRAZO, || tocados() >= base + TOCOU).await.is_some(),
        "antes de sair, o receptor não tocou a voz da pessoa"
    );
    tokio::time::sleep(VIDA_DA_VOZ_VELHA).await;

    // 2. Sai do servidor inteiro, sentada.
    primeira.disconnect();
    drop(primeira);
    assert!(
        ate(PRAZO, || mentira::contagem(aparelho).abertos == 0)
            .await
            .is_some(),
        "sair não fechou o aparelho da pessoa"
    );
    tokio::time::sleep(Duration::from_secs(2)).await;
    let (no_fio_antes, velho) = ultimo(&fio);
    let velho = velho.expect("a voz velha nunca passou pelo fio");

    // 3. Volta, com a mesma identidade (a mesma casa), e senta.
    let segunda = pela_ffi(endereco, casa.path(), "dona", aparelho, Arc::clone(&foco)).await;
    sentar_como_a_janela(&foco, &segunda).await;
    let chegou_no_fio = ate(PRAZO, || ultimo(&fio).0 > no_fio_antes).await;
    let novo = {
        let fio = fio.lock().unwrap();
        fio.get(no_fio_antes).map(|(_, cabecalho)| *cabecalho)
    };
    // Contagem limpa desta fase: só o que tocou depois da voz nova no fio.
    let base = tocados();
    let comeco = Instant::now();
    let mut primeiro_em = None;
    let tocou_em = ate(VIDA_DA_VOZ_VELHA + PRAZO, || {
        let agora = tocados();
        if agora > base && primeiro_em.is_none() {
            primeiro_em = Some(comeco.elapsed());
        }
        agora >= base + TOCOU
    })
    .await;

    println!(
        "vida da voz velha {VIDA_DA_VOZ_VELHA:?}; último no fio antes de sair {velho:?}; primeiro \
         depois de voltar {novo:?} (no fio em {chegou_no_fio:?}); o receptor tocou o primeiro \
         bloco com som em {primeiro_em:?} e {TOCOU} em {tocou_em:?} depois de a voz nova chegar"
    );
    let novo = novo.expect("a voz nova não chegou ao fio");
    assert!(
        chegou_no_fio.is_some(),
        "de volta e sentada, a voz nova não chegou ao fio"
    );
    assert!(
        tocou_em.is_some_and(|em| em <= TETO_DA_VOLTA),
        "de volta com o ssrc {} (antes {}), a seq {} e o carimbo {} (antes {} e {}), o receptor \
         que ficou levou {tocou_em:?} para tocar a pessoa: o teto é {TETO_DA_VOLTA:?}",
        novo.ssrc,
        velho.ssrc,
        novo.seq,
        novo.timestamp,
        velho.seq,
        velho.timestamp
    );

    segunda.disconnect();
    receptor.disconnect();
    daemon.shutdown();
    Ok(())
}
