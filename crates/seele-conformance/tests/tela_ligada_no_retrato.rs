//! **A tela que o servidor liga sozinho aparece no retrato** — de ponta a
//! ponta, pelo servidor de verdade e pela FFI.
//!
//! O servidor liga todo mundo na primeira transmissão da sala. Quem não clicou
//! em VER recebia a imagem (`ScreenOpened`, `ScreenFrame`), e o retrato dizia
//! `tela: null` e `assistida: false` — a janela escrevia «NINGUÉM ESTÁ
//! COMPARTILHANDO A TELA» por cima da imagem. O braço `TelaAbriu` do laço da
//! FFI só emitia o evento, e o retrato lia a tela só da escolha da pessoa.
//!
//! Aqui: um par cru transmite uma tela sintética, a pessoa entra pela FFI e
//! **não chama `assistir`**. Mede-se o retrato depois da imagem chegar, depois
//! do NÃO VER, e depois do fim do fluxo.

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
use seele_ffi::{ConnectConfig, Connection, Event, EventListener, Snapshot};
use seele_proto::control::{ClientMessage, ServerMessage};
use seele_proto::screen::{ScreenCodec, ScreenHeader, ScreenSource, SCREEN_HEADER_LEN};
use seele_server::persistence::Location;
use seele_server::{frame, Daemon, ServerConfig};

mod par_cru;
mod vaga;

const PRAZO: Duration = Duration::from_secs(15);

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

/// O transmissor: um par cru sentado, mandando `quadros` quadros de uma tela
/// sintética e então fechando o fluxo (a transmissão continua anunciada).
async fn quem_compartilha(endereco: SocketAddr, quadros: u32) -> Result<(par_cru::Par, u32)> {
    let chave = SigningKey::from_bytes(&[1; 32]);
    let mut par = par_cru::ate_a_prova(endereco, &chave, "compartilha").await;
    let ServerMessage::Session { voice_rooms, .. } = par.proximo().await? else {
        anyhow::bail!("o servidor não mandou Session");
    };
    let sala = voice_rooms.first().expect("uma sala").id;
    frame::write(
        &mut par.envio,
        &ClientMessage::EnterVoiceRoom {
            voice_room: sala,
            password: None,
        },
    )
    .await?;
    frame::write(&mut par.envio, &ClientMessage::StartScreenShare).await?;
    let screen = loop {
        if let ServerMessage::ScreenShareStarted { screen, .. } = par.proximo().await? {
            break screen;
        }
    };
    let mut fluxo = par.conexao.open_uni().await?;
    fluxo
        .write_all(&[seele_proto::stream::StreamType::Screen.byte()])
        .await?;
    let mut abertura = [0_u8; SCREEN_HEADER_LEN];
    ScreenHeader {
        version: seele_proto::PROTOCOL_VERSION,
        screen,
        source: ScreenSource::Monitor,
        codec: ScreenCodec::H264Baseline,
        width: 640,
        height: 360,
    }
    .encode(&mut abertura)?;
    fluxo.write_all(&abertura).await?;
    tokio::spawn(async move {
        for seq in 0..quadros {
            let tipo = if seq.is_multiple_of(3) {
                seele_core::tela::TipoDeQuadro::Chave
            } else {
                seele_core::tela::TipoDeQuadro::Comum
            };
            let corpo = seq.to_be_bytes().repeat(16);
            let mut quadro = vec![tipo.byte()];
            quadro.extend_from_slice(&u32::try_from(corpo.len()).unwrap().to_be_bytes());
            quadro.extend_from_slice(&corpo);
            if fluxo.write_all(&quadro).await.is_err() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(33)).await;
        }
        let _ = fluxo.finish();
        // Segura o fluxo até o fim chegar ao servidor.
        tokio::time::sleep(Duration::from_secs(30)).await;
        drop(fluxo);
    });
    Ok((par, screen.0))
}

/// O que a janela receberia.
#[derive(Default)]
struct Olhos {
    abertas: AtomicUsize,
    quadros: AtomicUsize,
    fechadas: AtomicUsize,
}

impl EventListener for Olhos {
    fn on_event(&self, event: Event) {
        match event {
            Event::ScreenOpened { .. } => {
                self.abertas.fetch_add(1, Ordering::SeqCst);
            }
            Event::ScreenFrame { .. } => {
                self.quadros.fetch_add(1, Ordering::SeqCst);
            }
            Event::ScreenClosed { .. } => {
                self.fechadas.fetch_add(1, Ordering::SeqCst);
            }
            _ => {}
        }
    }
}

async fn quem_assiste(
    endereco: SocketAddr,
    casa: &std::path::Path,
    olhos: Arc<Olhos>,
) -> Arc<Connection> {
    let config = ConnectConfig {
        server: endereco.to_string(),
        alternate_servers: Vec::new(),
        nickname: "assiste".into(),
        home: casa.display().to_string(),
        join_secret: None,
        expected_fingerprint: None,
        codigo_esperado: None,
        bilhete: None,
        audio: false,
        capture_device: None,
        playback_device: None,
    };
    let (conexao, _) = tokio::task::spawn_blocking(move || Connection::connect(config))
        .await
        .expect("a thread que conecta caiu")
        .unwrap_or_else(|falha| panic!("a pessoa não entrou: {falha:?}"));
    conexao.subscribe(olhos as Arc<dyn EventListener>);
    let sala = conexao.snapshot().voice_rooms[0].id;
    conexao.enter_voice_room(sala, None).expect("sentar");
    conexao
}

async fn ate(prazo: Duration, mut pronto: impl FnMut() -> bool) -> bool {
    let comeco = Instant::now();
    while comeco.elapsed() < prazo {
        if pronto() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    pronto()
}

/// A tela do retrato e as que dizem `assistida`.
fn no_retrato(retrato: &Snapshot) -> (Option<u32>, Vec<u32>) {
    (
        retrato.tela.as_ref().map(|tela| tela.tela),
        retrato
            .transmissoes
            .iter()
            .filter(|transmissao| transmissao.assistida)
            .map(|transmissao| transmissao.tela)
            .collect(),
    )
}

/// Sentada, sem escolher nada, entra a primeira transmissão da sala: o
/// servidor a liga sozinho, a imagem chega, e o retrato diz qual é.
async fn ligada_pelo_servidor(
    quadros: u32,
) -> Result<(
    Arc<Daemon>,
    Arc<Connection>,
    Arc<Olhos>,
    par_cru::Par,
    u32,
    tempfile::TempDir,
)> {
    let (endereco, daemon) = servidor().await?;
    let casa = tempfile::tempdir()?;
    let olhos = Arc::new(Olhos::default());
    let conexao = quem_assiste(endereco, casa.path(), Arc::clone(&olhos)).await;
    assert!(
        ate(PRAZO, || conexao
            .snapshot()
            .voice_rooms
            .iter()
            .any(|sala| sala.occupied_by_us))
        .await,
        "a pessoa não sentou"
    );
    let (par, tela) = quem_compartilha(endereco, quadros).await?;
    assert!(
        ate(PRAZO, || olhos.quadros.load(Ordering::SeqCst) > 3).await,
        "o servidor não ligou a primeira transmissão para quem não escolheu nada"
    );
    assert_eq!(conexao.tela_escolhida(), None, "ninguém escolheu");
    Ok((daemon, conexao, olhos, par, tela, casa))
}

#[tokio::test(flavor = "multi_thread")]
async fn quem_nao_escolheu_ve_no_retrato_a_tela_que_recebe() -> Result<()> {
    let _vaga = vaga::minha();
    let (daemon, conexao, olhos, _par, tela, _casa) = ligada_pelo_servidor(10_000).await?;
    assert!(olhos.abertas.load(Ordering::SeqCst) >= 1);
    assert_eq!(
        no_retrato(&conexao.snapshot()),
        (Some(tela), vec![tela]),
        "a imagem chega e o retrato diz que ninguém está compartilhando"
    );

    // NÃO VER: a recusa vence a tela ligada, e o retrato não diz que assiste.
    conexao.assistir(tela, false).expect("não ver");
    assert_eq!(
        no_retrato(&conexao.snapshot()),
        (None, vec![]),
        "quem recusou aparece assistindo"
    );
    // E continua não dizendo depois que a imagem para de chegar.
    assert!(
        ate(PRAZO, || olhos.fechadas.load(Ordering::SeqCst) >= 1).await,
        "a recusa não fechou a tela"
    );
    assert_eq!(no_retrato(&conexao.snapshot()), (None, vec![]));

    conexao.disconnect();
    daemon.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn o_fim_do_fluxo_tira_a_tela_ligada_do_retrato() -> Result<()> {
    let _vaga = vaga::minha();
    let (daemon, conexao, olhos, _par, tela, _casa) = ligada_pelo_servidor(30).await?;
    // O fluxo pode já ter acabado quando o primeiro retrato é lido; antes do
    // fim, o retrato tem de mostrar a tela.
    if olhos.fechadas.load(Ordering::SeqCst) == 0 {
        let retrato = no_retrato(&conexao.snapshot());
        if olhos.fechadas.load(Ordering::SeqCst) == 0 {
            assert_eq!(retrato, (Some(tela), vec![tela]));
        }
    }
    assert!(
        ate(PRAZO, || olhos.fechadas.load(Ordering::SeqCst) >= 1).await,
        "o fim do fluxo não chegou como ScreenClosed"
    );
    // A transmissão continua anunciada na sala; só a imagem acabou.
    let retrato = conexao.snapshot();
    assert!(
        retrato
            .transmissoes
            .iter()
            .any(|transmissao| transmissao.tela == tela),
        "a transmissão saiu da sala, e o teste deixou de medir o TelaFechou"
    );
    assert_eq!(
        no_retrato(&retrato),
        (None, vec![]),
        "a imagem acabou e o retrato continua dizendo que se assiste"
    );

    conexao.disconnect();
    daemon.shutdown();
    Ok(())
}
