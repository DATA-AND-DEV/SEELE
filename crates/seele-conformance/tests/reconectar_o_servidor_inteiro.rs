//! **A tela some, a pessoa desconecta e reconecta o servidor inteiro, e a voz
//! para** (relato do usuário, 25/10): o ciclo inteiro, com meios sintéticos.
//!
//! - Um `seeled` de verdade, em processo.
//! - **Quem compartilha** é um par cru (`par_cru`), sentado na sala padrão,
//!   transmitindo uma tela sintética numerada (nenhuma captura de verdade).
//! - **Quem ouve** é um `Enlace` de verdade, sentado na mesma sala, contando os
//!   datagramas de voz que o servidor encaminha a ele.
//! - **A pessoa** é esta máquina pela FFI (`Connection::connect_com_foco`), com
//!   o foco de voz da máquina ([`FocoDeVoz`]) e o **aparelho de mentira** do
//!   `seele-audio` (um tom que abre qualquer portão; nunca o microfone real).
//!
//! O ciclo, como a janela o faz:
//!
//! 1. a pessoa entra, senta na sala, assiste a tela, e a voz dela chega a quem
//!    ouve;
//! 2. ela **sai do servidor inteiro** (`disconnect`, a sessão cai);
//! 3. ela **entra de novo** no mesmo servidor, com o mesmo foco da máquina, e
//!    senta na sala **pelo mesmo caminho da janela**: sem o foco, foca antes de
//!    sentar (`entrar_na_sala_em`, no app);
//! 4. a voz dela tem de voltar a chegar a quem ouve, e o aparelho de mentira
//!    tem de estar aberto **uma vez só**.
//!
//! O que se mede é o datagrama que chega a quem ouve (o fio), e não o estado da
//! tela nem um indicador.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_audio::device::mentira;
use seele_core::enlace::{Destino, Enlace};
use seele_core::{MemoryPinStore, PinStore};
use seele_ffi::foco::FocoDeVoz;
use seele_ffi::{AberturaDoAudio, ConnectConfig, Connection, VoiceMode};
use seele_proto::control::{ClientMessage, ServerMessage};
use seele_proto::screen::{ScreenCodec, ScreenHeader, ScreenSource, SCREEN_HEADER_LEN};
use seele_server::persistence::Location;
use seele_server::{frame, Daemon, ServerConfig};

mod par_cru;
mod vaga;

const APARELHO: &str = "reconectar-o-servidor";
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

/// Quem compartilha: um par cru sentado, transmitindo uma tela sintética.
async fn quem_compartilha(endereco: SocketAddr) -> Result<(par_cru::Par, u32)> {
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
        let mut seq: u32 = 0;
        loop {
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
            seq = seq.wrapping_add(1);
            tokio::time::sleep(Duration::from_millis(33)).await;
        }
    });
    Ok((par, screen.0))
}

/// Quem ouve: sentado na sala padrão, contando a voz que chega.
async fn quem_ouve(endereco: SocketAddr, contados: Arc<AtomicUsize>) -> Result<Enlace> {
    let enlace = Enlace::conectar(
        Destino {
            servidor: endereco,
            nome_tls: "localhost".into(),
            chave_do_pin: endereco.to_string(),
            apelido: "ouve".into(),
            segredo: None,
            esperada: None,
            o_pino_prova_o_servidor: true,
            aceito: None,
        },
        SigningKey::from_bytes(&[90; 32]),
        Arc::new(MemoryPinStore::new()) as Arc<dyn PinStore>,
    )
    .await?;
    let sala = enlace.sessao().voice_rooms[0].id;
    enlace.entrar_na_voice_room(sala, None).await?;
    let canal = enlace.media();
    tokio::spawn(async move {
        while canal.next().await.is_ok() {
            contados.fetch_add(1, Ordering::SeqCst);
        }
    });
    Ok(enlace)
}

/// A pessoa, pela FFI, no foco da máquina, com o aparelho de mentira.
async fn a_pessoa(
    endereco: SocketAddr,
    casa: &std::path::Path,
    foco: &Arc<FocoDeVoz>,
) -> Arc<Connection> {
    let config = ConnectConfig {
        server: endereco.to_string(),
        alternate_servers: Vec::new(),
        nickname: "dona".into(),
        home: casa.display().to_string(),
        join_secret: None,
        expected_fingerprint: None,
        codigo_esperado: None,
        bilhete: None,
        audio: true,
        capture_device: Some(mentira::id(APARELHO)),
        playback_device: Some(mentira::id(APARELHO)),
    };
    let foco = Arc::clone(foco);
    let (conexao, _) = tokio::task::spawn_blocking(move || {
        Connection::connect_com_foco(config, None, None, Some(foco), None)
    })
    .await
    .expect("a thread que conecta caiu")
    .unwrap_or_else(|falha| panic!("a pessoa não entrou: {falha:?}"));
    conexao.set_voice_mode(VoiceMode::Open);
    conexao
}

/// **Sentar como a janela senta** (`entrar_na_sala_em`, no app): sem o foco,
/// foca antes; depois, senta.
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

async fn ate(o_que: &str, mut pronto: impl FnMut() -> bool) {
    let fim = tokio::time::Instant::now() + PRAZO;
    while tokio::time::Instant::now() < fim {
        if pronto() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(pronto(), "{o_que} não aconteceu em {PRAZO:?}");
}

/// Quantos datagramas chegam a quem ouve numa janela de tempo.
async fn chegam_em(contados: &AtomicUsize, janela: Duration) -> usize {
    let antes = contados.load(Ordering::SeqCst);
    tokio::time::sleep(janela).await;
    contados.load(Ordering::SeqCst) - antes
}

#[tokio::test(flavor = "multi_thread")]
async fn sair_do_servidor_inteiro_e_voltar_devolve_a_voz() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;
    let contados = Arc::new(AtomicUsize::new(0));
    let ouve = quem_ouve(endereco, Arc::clone(&contados)).await?;
    let (_compartilha, tela) = quem_compartilha(endereco).await?;

    let foco = FocoDeVoz::novo();
    let casa = tempfile::tempdir()?;

    // 1. Entra, senta, assiste a tela: a voz chega.
    let primeira = a_pessoa(endereco, casa.path(), &foco).await;
    sentar_como_a_janela(&foco, &primeira).await;
    primeira.assistir(tela, true).expect("assistir a tela");
    ate("a voz da primeira sessão abrir", || {
        mentira::contagem(APARELHO).abertos == 1
    })
    .await;
    let antes = chegam_em(&contados, Duration::from_millis(1500)).await;
    assert!(antes > 0, "a voz da primeira sessão não chegou a quem ouve");

    // 2. Sai do servidor inteiro.
    primeira.disconnect();
    drop(primeira);
    ate("o aparelho da sessão que saiu fechar", || {
        mentira::contagem(APARELHO).abertos == 0
    })
    .await;
    assert_eq!(foco.dono(), None, "a sessão saiu e o foco ficou com ela");

    // 3. Entra de novo e senta como a janela senta.
    let segunda = a_pessoa(endereco, casa.path(), &foco).await;
    sentar_como_a_janela(&foco, &segunda).await;
    assert_eq!(
        foco.dono(),
        segunda.id_no_foco(),
        "sentar pela janela não deu a voz à sessão nova"
    );

    // 4. A voz volta a chegar, por um aparelho só.
    ate("a voz da sessão nova abrir", || {
        segunda.snapshot().abertura_do_audio == AberturaDoAudio::Aberto
            || mentira::contagem(APARELHO).abertos == 1
    })
    .await;
    let depois = chegam_em(&contados, Duration::from_millis(1500)).await;
    println!(
        "datagramas de voz em 1,5 s: antes de sair {antes}, depois de voltar {depois}; \
         aparelhos abertos: {}; abertura: {:?}",
        mentira::contagem(APARELHO).abertos,
        segunda.snapshot().abertura_do_audio
    );
    assert!(
        depois > 0,
        "depois de sair do servidor inteiro e voltar, a voz não chega mais a quem ouve \
         (abertura {:?}, aparelhos abertos {})",
        segunda.snapshot().abertura_do_audio,
        mentira::contagem(APARELHO).abertos
    );
    assert_eq!(
        mentira::contagem(APARELHO).abertos,
        1,
        "dois aparelhos abertos"
    );

    segunda.disconnect();
    drop(ouve);
    daemon.shutdown();
    Ok(())
}
