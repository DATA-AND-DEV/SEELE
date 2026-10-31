//! **O aceite da máquina de jogo, de ponta a ponta** (API 8, parte 3): o
//! servidor de verdade com o MOD `x/jogo` e a metade de servidor dele, a
//! máquina e quem joga como `seele_core::enlace::Enlace` de verdade, e o «jogo»
//! um eco TCP em `127.0.0.1`.
//!
//! O que cada teste prova:
//!
//! - `sem_aceite_a_maquina_nao_serve`: o portão. Sem aceite, servir é
//!   `ForaDoAceite`, e o jogo não recebe conexão nenhuma — a porta da máquina de
//!   alguém não se abre porque um MOD tem o poder `porta`.
//! - `com_aceite_o_eco_passa`: o caminho inteiro (a metade de servidor pede, o
//!   núcleo mostra, a máquina aceita, serve) e o eco anda.
//! - `um_enlace_sem_tela_recusa_e_a_metade_de_servidor_sabe`: sem tela não há
//!   quem leia a oferta; o núcleo recusa sozinho, e **o MOD fica sabendo** com o
//!   motivo — uma recusa silenciosa voltaria como pergunta de quem hospeda.
//! - `parar_a_maquina_fecha_a_porta_de_quem_joga_e_avisa_o_mod`.
//! - `a_maquina_que_cai_e_volta_nao_serve_sem_aceite_novo`: o aceite é da
//!   conexão que aceitou, e não da pessoa.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_core::enlace::{Aviso, Destino, Enlace};
use seele_core::tunel::{Queda, RelatorioDoTunel};
use seele_core::{MemoryPinStore, PinStore};
use seele_proto::maquina::MotivoDaParadaDeMaquina;
use seele_proto::tunel::{ProtocoloDeTunel, RecusaDeTunel};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

mod maquina_de_jogo;
use maquina_de_jogo::{
    aceitar_como_maquina, esperar_o_momento, pedir_a_maquina, servidor, MOD, SERVIDOR_DE_JOGO,
};

const PACIENCIA: Duration = Duration::from_secs(30);

/// O túnel que estas provas servem.
const TUNEL: &str = "eco-tcp";

const MAQUINA: u8 = 1;
const JOGA: u8 = 2;

async fn enlace(
    endereco: SocketAddr,
    conjunto: &str,
    semente: u8,
    apelido: &str,
) -> Result<Enlace> {
    Ok(Enlace::conectar_entre(
        vec![Destino {
            servidor: endereco,
            nome_tls: "localhost".into(),
            chave_do_pin: endereco.to_string(),
            apelido: apelido.to_owned(),
            segredo: None,
            esperada: None,
            o_pino_prova_o_servidor: true,
            aceito: Some(conjunto.to_owned()),
        }],
        SigningKey::from_bytes(&[semente; 32]),
        Arc::new(MemoryPinStore::default()) as Arc<dyn PinStore>,
    )
    .await?)
}

/// Um eco TCP fiel em `127.0.0.1`, que conta as conexões que aceitou: é o
/// contador que diz se alguém chegou ao jogo.
async fn eco_tcp() -> (u16, Arc<AtomicUsize>) {
    let escuta = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let porta = escuta.local_addr().unwrap().port();
    let aceitas = Arc::new(AtomicUsize::new(0));
    let contador = Arc::clone(&aceitas);
    tokio::spawn(async move {
        while let Ok((mut fluxo, _)) = escuta.accept().await {
            contador.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(async move {
                let mut buf = vec![0_u8; 16 * 1024];
                while let Ok(n) = fluxo.read(&mut buf).await {
                    if n == 0 || fluxo.write_all(&buf[..n]).await.is_err() {
                        return;
                    }
                }
            });
        }
    });
    (porta, aceitas)
}

/// Um padrão que denuncia byte fora do lugar.
fn padrao(n: usize) -> Vec<u8> {
    (0..n)
        .map(|i| u8::try_from((i * 7 + i / 251) % 251).unwrap())
        .collect()
}

/// Lê avisos de quem joga até um relatório do [`TUNEL`] que `serve` aprove.
async fn relatorio(
    joga: &mut Enlace,
    o_que: &str,
    serve: impl Fn(&RelatorioDoTunel) -> bool,
) -> Result<RelatorioDoTunel> {
    let mut vistos = Vec::new();
    let fim = Instant::now() + PACIENCIA;
    while Instant::now() < fim {
        let Ok(aviso) = tokio::time::timeout(Duration::from_millis(500), joga.proximo()).await
        else {
            continue;
        };
        if let Aviso::Tunel(r) = aviso {
            if r.nome == TUNEL && serve(&r) {
                return Ok(r);
            }
            vistos.push(r);
        }
    }
    anyhow::bail!("a paciência acabou esperando: {o_que}; relatórios vistos: {vistos:#?}")
}

async fn a_porta_fecha(porta: u16) -> Result<()> {
    let fim = Instant::now() + PACIENCIA;
    while Instant::now() < fim {
        if TcpStream::connect(("127.0.0.1", porta)).await.is_err() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    anyhow::bail!("a porta {porta} continuou aceitando conexões")
}

/// A linha que a metade de servidor grava para um momento da máquina.
fn momento(nome: &str, pessoa: u64, motivo: Option<&str>) -> String {
    match motivo {
        Some(m) => format!(
            r#"{nome} {{"servidor":"{SERVIDOR_DE_JOGO}","pessoa":{pessoa},"motivo":"{m}"}}"#
        ),
        None => format!(r#"{nome} {{"servidor":"{SERVIDOR_DE_JOGO}","pessoa":{pessoa}}}"#),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn sem_aceite_a_maquina_nao_serve() -> Result<()> {
    let (endereco, daemon, conjunto, _temp) = servidor(true, None).await?;
    let maquina = enlace(endereco, &conjunto, MAQUINA, "maquina").await?;
    let joga = enlace(endereco, &conjunto, JOGA, "joga").await?;
    let (jogo, aceitas) = eco_tcp().await;

    assert_eq!(
        maquina
            .servir_tunel(MOD, TUNEL, ProtocoloDeTunel::Tcp, jogo)
            .await,
        Err(RecusaDeTunel::ForaDoAceite),
        "a máquina serviu sem aceite"
    );
    assert!(
        !daemon.server().tuneis().existe(MOD, TUNEL),
        "o túnel foi para o livro apesar da recusa"
    );
    // Quem joga não acha o que entrar...
    assert_eq!(
        joga.entrar_no_tunel(MOD, TUNEL).await.map(|_| ()),
        Err(RecusaDeTunel::NaoExiste)
    );
    // ...e o jogo não viu ninguém: nenhuma porta do eco foi tocada.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(aceitas.load(Ordering::SeqCst), 0, "alguém chegou ao jogo");
    daemon.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn com_aceite_o_eco_passa() -> Result<()> {
    let (endereco, daemon, conjunto, _temp) = servidor(true, None).await?;
    let mut maquina = enlace(endereco, &conjunto, MAQUINA, "maquina").await?;
    let joga = enlace(endereco, &conjunto, JOGA, "joga").await?;
    let (jogo, _) = eco_tcp().await;

    aceitar_como_maquina(
        &daemon,
        &mut maquina,
        MOD,
        SERVIDOR_DE_JOGO,
        &[(TUNEL, ProtocoloDeTunel::Tcp, jogo)],
    )
    .await?;
    maquina
        .servir_tunel(MOD, TUNEL, ProtocoloDeTunel::Tcp, jogo)
        .await
        .map_err(|r| anyhow::anyhow!("servir com aceite: {r:?}"))?;
    let porta = joga
        .entrar_no_tunel(MOD, TUNEL)
        .await
        .map_err(|r| anyhow::anyhow!("entrar: {r:?}"))?
        .porta;

    // 64 KiB, escritos e lidos ao mesmo tempo.
    let corpo = padrao(64 * 1024);
    let mut fluxo = TcpStream::connect(("127.0.0.1", porta)).await?;
    let (mut le, mut escreve) = fluxo.split();
    let escrita = async { escreve.write_all(&corpo).await.unwrap() };
    let leitura = async {
        let mut volta = vec![0_u8; corpo.len()];
        le.read_exact(&mut volta).await.unwrap();
        volta
    };
    let (_, volta) = tokio::time::timeout(PACIENCIA, async { tokio::join!(escrita, leitura) })
        .await
        .expect("o eco de 64 KiB não voltou a tempo");
    assert!(volta == corpo, "o eco voltou diferente do que foi");
    daemon.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn um_enlace_sem_tela_recusa_e_a_metade_de_servidor_sabe() -> Result<()> {
    let (endereco, daemon, conjunto, _temp) = servidor(true, None).await?;
    // Sem `aceitar_pedidos_de_maquina`: um enlace sem tela, como o da TUI.
    let mut maquina = enlace(endereco, &conjunto, MAQUINA, "maquina").await?;
    let pessoa = maquina.sessao().person.0;

    pedir_a_maquina(
        &mut maquina,
        SERVIDOR_DE_JOGO,
        &[(TUNEL, ProtocoloDeTunel::Tcp, 25565)],
    )
    .await?;
    let vistos = esperar_o_momento(
        &mut maquina,
        &momento("MaquinaRecusou", pessoa, Some("sem-tela")),
    )
    .await?;
    assert!(
        !vistos.contains("MaquinaAceitou"),
        "um enlace sem tela aceitou: {vistos}"
    );
    assert_eq!(
        daemon.server().maquinas().quem_opera(MOD, SERVIDOR_DE_JOGO),
        None
    );
    daemon.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn parar_a_maquina_fecha_a_porta_de_quem_joga_e_avisa_o_mod() -> Result<()> {
    let (endereco, daemon, conjunto, _temp) = servidor(true, None).await?;
    let mut maquina = enlace(endereco, &conjunto, MAQUINA, "maquina").await?;
    let mut joga = enlace(endereco, &conjunto, JOGA, "joga").await?;
    let pessoa = maquina.sessao().person.0;
    let (jogo, _) = eco_tcp().await;

    aceitar_como_maquina(
        &daemon,
        &mut maquina,
        MOD,
        SERVIDOR_DE_JOGO,
        &[(TUNEL, ProtocoloDeTunel::Tcp, jogo)],
    )
    .await?;
    maquina
        .servir_tunel(MOD, TUNEL, ProtocoloDeTunel::Tcp, jogo)
        .await
        .map_err(|r| anyhow::anyhow!("servir: {r:?}"))?;
    let porta = joga
        .entrar_no_tunel(MOD, TUNEL)
        .await
        .map_err(|r| anyhow::anyhow!("entrar: {r:?}"))?
        .porta;
    // O túnel está de pé: a porta aceita.
    drop(TcpStream::connect(("127.0.0.1", porta)).await?);

    maquina
        .parar_maquina(MOD, SERVIDOR_DE_JOGO, MotivoDaParadaDeMaquina::PeloDono)
        .await
        .map_err(|_| anyhow::anyhow!("o enlace da máquina já tinha acabado"))?;

    // Quem joga vê o fim, com o motivo...
    let fim = relatorio(&mut joga, "o fim do túnel", |r| r.encerrado.is_some()).await?;
    assert_eq!(fim.encerrado, Some(Queda::MaquinaParou), "{fim:?}");
    a_porta_fecha(porta).await?;
    // ...e o MOD também.
    esperar_o_momento(
        &mut maquina,
        &momento("MaquinaParou", pessoa, Some("pelo-dono")),
    )
    .await?;
    assert_eq!(
        daemon.server().maquinas().quem_opera(MOD, SERVIDOR_DE_JOGO),
        None
    );
    daemon.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_maquina_que_cai_e_volta_nao_serve_sem_aceite_novo() -> Result<()> {
    let (endereco, daemon, conjunto, _temp) = servidor(true, None).await?;
    let mut maquina = enlace(endereco, &conjunto, MAQUINA, "maquina").await?;
    let pessoa = maquina.sessao().person;
    let (jogo, _) = eco_tcp().await;

    aceitar_como_maquina(
        &daemon,
        &mut maquina,
        MOD,
        SERVIDOR_DE_JOGO,
        &[(TUNEL, ProtocoloDeTunel::Tcp, jogo)],
    )
    .await?;
    maquina
        .servir_tunel(MOD, TUNEL, ProtocoloDeTunel::Tcp, jogo)
        .await
        .map_err(|r| anyhow::anyhow!("servir: {r:?}"))?;

    // A sessão da máquina cai.
    drop(maquina);
    // A mesma pessoa volta, por outra sessão, com a mesma oferta na mão: não
    // serve, porque o aceite era da conexão que caiu.
    let mut volta = enlace(endereco, &conjunto, MAQUINA, "maquina").await?;
    assert_eq!(
        volta.sessao().person,
        pessoa,
        "a volta não é a mesma pessoa"
    );
    assert_eq!(
        volta
            .servir_tunel(MOD, TUNEL, ProtocoloDeTunel::Tcp, jogo)
            .await,
        Err(RecusaDeTunel::ForaDoAceite),
        "a sessão que voltou serviu sem aceite novo"
    );
    // O túnel da sessão que caiu também não sobra (a queda chega ao servidor
    // no tempo dela, e por isso se espera).
    let ate = Instant::now() + PACIENCIA;
    while daemon.server().tuneis().existe(MOD, TUNEL) {
        assert!(
            Instant::now() < ate,
            "o túnel da máquina que caiu seguiu no livro"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // E o MOD soube que a máquina saiu.
    esperar_o_momento(&mut volta, &momento("MaquinaParou", pessoa.0, Some("saiu"))).await?;
    daemon.shutdown();
    Ok(())
}
