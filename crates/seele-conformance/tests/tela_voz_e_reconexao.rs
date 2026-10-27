//! **TELA + VOZ + RECONEXÃO** (o relato de 25/10, inteiro): a pessoa ASSISTE
//! uma tela, a tela «desaparece», ela SAI do servidor inteiro, VOLTA, ENTRA na
//! sala, e fica sem áudio.
//!
//! Com meios sintéticos:
//! - um par cru transmite uma tela de verdade pelo fluxo de tela (quadros
//!   sintéticos numerados, nenhuma captura);
//! - quem fala é outra máquina pela FFI, com o tom de mentira;
//! - quem ouve conta, no fio, a voz que chega a ele;
//! - a pessoa é esta máquina pela FFI, com o foco da máquina, o aparelho de
//!   mentira (o tom no microfone; a saída conta os `blocos_com_som`) e um
//!   ouvinte de eventos que conta os `ScreenFrame` que a janela receberia.
//!
//! Duas falhas da tela, cada uma num teste: **o transmissor para de escrever
//! com o fluxo aberto** (a imagem para sem EOF) e **o transmissor fecha o
//! fluxo** (o fim da tela). Depois de cada uma, o ciclo do relato, e mede-se:
//! - se a saída fecha o aparelho da sessão velha, e em quanto tempo (a
//!   hipótese de a tela prender a sessão velha, e com ela o aparelho);
//! - depois de voltar e sentar: o foco, o ENVIO (datagramas acima da base de
//!   quem fala) e o RECEBIDO e tocado (`blocos_com_som`).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_audio::device::mentira;
use seele_core::enlace::{Destino, Enlace};
use seele_core::{MemoryPinStore, PinStore};
use seele_ffi::foco::FocoDeVoz;
use seele_ffi::{AberturaDoAudio, ConnectConfig, Connection, Event, EventListener, VoiceMode};
use seele_proto::control::{ClientMessage, ServerMessage};
use seele_proto::screen::{ScreenCodec, ScreenHeader, ScreenSource, SCREEN_HEADER_LEN};
use seele_server::persistence::Location;
use seele_server::{frame, Daemon, ServerConfig};

mod par_cru;
mod vaga;

const PRAZO: Duration = Duration::from_secs(15);

/// Como a tela «desaparece».
#[derive(Clone, Copy, Debug)]
enum Falha {
    /// O transmissor para de escrever, e o fluxo fica aberto: a imagem para sem EOF.
    Pausar,
    /// O transmissor fecha o fluxo da tela.
    Encerrar,
}

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

/// O transmissor: um par cru sentado, mandando uma tela sintética até `parar`
/// (e então, conforme a falha, segurando o fluxo aberto ou fechando-o).
async fn quem_compartilha(
    endereco: SocketAddr,
    parar: Arc<AtomicBool>,
    falha: Falha,
) -> Result<(par_cru::Par, u32)> {
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
        while !parar.load(Ordering::SeqCst) {
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
        match falha {
            // Segura o fluxo aberto, sem escrever, enquanto a tarefa viver.
            Falha::Pausar => loop {
                tokio::time::sleep(Duration::from_secs(3600)).await;
                let _ = &fluxo;
            },
            Falha::Encerrar => {
                let _ = fluxo.finish();
                drop(fluxo);
            }
        }
    });
    Ok((par, screen.0))
}

/// Quem ouve: sentado, contando a voz que chega no fio.
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

/// Os quadros de tela que a janela receberia (os eventos `ScreenFrame`).
#[derive(Default)]
struct Olhos {
    quadros: AtomicUsize,
    fechadas: AtomicUsize,
}

impl EventListener for Olhos {
    fn on_event(&self, event: Event) {
        match event {
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

/// Uma pessoa pela FFI, com um aparelho de mentira e, se dados, os olhos e o foco.
async fn pela_ffi(
    endereco: SocketAddr,
    casa: &std::path::Path,
    apelido: &str,
    aparelho: &str,
    olhos: Option<Arc<Olhos>>,
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
    // Como a casca: inscrita logo depois de entrar (`connection.subscribe`).
    if let Some(olhos) = olhos {
        conexao.subscribe(olhos as Arc<dyn EventListener>);
    }
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

async fn chegam_em(contados: &AtomicUsize, janela: Duration) -> usize {
    let antes = contados.load(Ordering::SeqCst);
    tokio::time::sleep(janela).await;
    contados.load(Ordering::SeqCst) - antes
}

async fn o_ciclo_do_relato(falha: Falha) -> Result<()> {
    let _vaga = vaga::minha();
    let aparelho = match falha {
        Falha::Pausar => "tela-pausar",
        Falha::Encerrar => "tela-encerrar",
    };
    let (endereco, daemon) = servidor().await?;
    let contados = Arc::new(AtomicUsize::new(0));
    let ouve = quem_ouve(endereco, Arc::clone(&contados)).await?;
    let parar = Arc::new(AtomicBool::new(false));
    let (_compartilha, tela) = quem_compartilha(endereco, Arc::clone(&parar), falha).await?;
    let casa_de_quem_fala = tempfile::tempdir()?;
    let fala = pela_ffi(
        endereco,
        casa_de_quem_fala.path(),
        "fala",
        &format!("{aparelho}-fala"),
        None,
        FocoDeVoz::novo(),
    )
    .await;
    let sala = fala.snapshot().voice_rooms[0].id;
    fala.enter_voice_room(sala, None).expect("quem fala sentou");

    let foco = FocoDeVoz::novo();
    let casa = tempfile::tempdir()?;
    let tocados = || mentira::contagem(aparelho).blocos_com_som;

    // 1. Entra, senta, assiste a tela.
    let olhos = Arc::new(Olhos::default());
    let primeira = pela_ffi(
        endereco,
        casa.path(),
        "dona",
        aparelho,
        Some(Arc::clone(&olhos)),
        Arc::clone(&foco),
    )
    .await;
    sentar_como_a_janela(&foco, &primeira).await;
    primeira.assistir(tela, true).expect("assistir a tela");
    assert!(
        ate(PRAZO, || olhos.quadros.load(Ordering::SeqCst) > 10)
            .await
            .is_some(),
        "a janela não recebeu os quadros da tela"
    );
    assert!(ate(PRAZO, || mentira::contagem(aparelho).abertos == 1)
        .await
        .is_some());
    let tocou = tocados();
    let antes = chegam_em(&contados, Duration::from_secs(2)).await;
    let ouviu_antes = tocados() - tocou;
    assert!(
        ouviu_antes > 0,
        "assistindo a tela, a pessoa não tocou a voz de quem fala"
    );

    // 2. A tela «desaparece».
    parar.store(true, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_secs(2)).await;
    let quadros_parados = olhos.quadros.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(500)).await;
    let ainda_chegam = olhos.quadros.load(Ordering::SeqCst) - quadros_parados;

    // 3. Sai do servidor inteiro, e mede se o aparelho da sessão velha fechou.
    primeira.disconnect();
    drop(primeira);
    let fechou_em = ate(Duration::from_secs(10), || {
        mentira::contagem(aparelho).abertos == 0
    })
    .await;
    let so_de_quem_fala = chegam_em(&contados, Duration::from_secs(2)).await;

    // 4. Volta, senta pela janela.
    let segunda = pela_ffi(
        endereco,
        casa.path(),
        "dona",
        aparelho,
        Some(Arc::new(Olhos::default())),
        Arc::clone(&foco),
    )
    .await;
    sentar_como_a_janela(&foco, &segunda).await;
    let abriu_em = ate(PRAZO, || mentira::contagem(aparelho).abertos == 1).await;
    let tocou = tocados();
    let depois = chegam_em(&contados, Duration::from_secs(5)).await;
    let ouviu_depois = tocados() - tocou;
    let base_5 = so_de_quem_fala * 5 / 2;
    println!(
        "falha {falha:?}: quadros depois de parar {ainda_chegam}, fechadas {}; antes: no fio {antes} em 2 s, \
         tocados {ouviu_antes}; aparelho velho fechou em {fechou_em:?}; base de quem fala {so_de_quem_fala} em 2 s; \
         de volta: aparelho abriu em {abriu_em:?}, no fio {depois} em 5 s (base {base_5}), tocados {ouviu_depois}, \
         abertura {:?}, foco {:?} (a nova é {:?}), aparelhos abertos {}",
        olhos.fechadas.load(Ordering::SeqCst),
        segunda.snapshot().abertura_do_audio,
        foco.dono(),
        segunda.id_no_foco(),
        mentira::contagem(aparelho).abertos
    );
    assert!(
        fechou_em.is_some(),
        "depois da tela «sumir» ({falha:?}), sair não fechou o aparelho da sessão velha"
    );
    assert_eq!(
        foco.dono(),
        segunda.id_no_foco(),
        "sentar de volta não deu a voz à sessão nova"
    );
    assert!(
        abriu_em.is_some(),
        "de volta e sentada, o aparelho da sessão nova não abriu"
    );
    assert!(
        depois.saturating_sub(base_5) > base_5 / 3,
        "de volta e sentada depois da tela «sumir» ({falha:?}), a voz desta máquina não chega a quem ouve"
    );
    assert!(
        ouviu_depois > 0,
        "de volta e sentada depois da tela «sumir» ({falha:?}), a pessoa não ouve"
    );
    assert_eq!(
        segunda.snapshot().abertura_do_audio,
        AberturaDoAudio::Aberto
    );

    segunda.disconnect();
    fala.disconnect();
    drop(ouve);
    daemon.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tela_que_para_sem_eof_e_a_volta_ao_servidor_devolvem_a_voz() -> Result<()> {
    o_ciclo_do_relato(Falha::Pausar).await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tela_que_fecha_e_a_volta_ao_servidor_devolvem_a_voz() -> Result<()> {
    o_ciclo_do_relato(Falha::Encerrar).await
}
