//! **O receptor que volta ao servidor e só senta na voz depois** (o P1 do CUA
//! nativo de 25/10, 20:42): a pessoa sai do servidor inteiro, sentada, e volta.
//! O servidor devolve o assento da graça (`Slots::reclaim`) e passa a encaminhar
//! a voz da sala à conexão nova **no instante em que ela entra**; o app só abre
//! a voz quando o foco volta a ela — no CUA, 12 s depois.
//!
//! Do lado de quem volta, a fila de voz do enlace (`seele_enlace::FILA_DE_VOZ`,
//! 256 datagramas, o mais novo descartado quando cheia) enche de voz que
//! ninguém lê. Quando a voz abre, o laço drena aquilo de uma vez: 5,12 s de voz
//! velha no buffer de jitter, que também tem teto de 256 quadros. O que se mediu
//! no CUA, nesta ordem:
//! - `prof_ms=4880` três segundos depois de abrir, com `perda=0`;
//! - depois, `perda` ~0,45–0,55 por dois minutos: o buraco de `seq` entre a voz
//!   velha e a viva é tocado quadro a quadro como perda, a fila cheia não aceita
//!   o que chega nesses tiques, e o buraco se replica a cada volta da fila;
//! - só então a drenagem de 10% do buffer (`JANELA_DE_RECUPERACAO_TICKS`) leva
//!   os 5 s de atraso a 20 ms: três minutos para normalizar.
//!
//! O que este teste fixa, com meios sintéticos (nenhum microfone nem saída da
//! máquina):
//! - quem fala é um par cru, um EMISSOR CONTÍNUO que nunca reconecta, com a
//!   `seq` e o carimbo de uma fala de ~22 min (por tempo simulado: os números
//!   partem de lá) e a `seq` dando a volta durante o teste;
//! - quem ouve é uma `Connection` da FFI, muda, com a saída de mentira, que sai
//!   do servidor sentada, volta com a mesma identidade e o mesmo foco (vazio,
//!   como no app) e só senta depois de [`ANTES_DE_SENTAR`];
//! - depois de sentar, o regime e não só o começo: a fração de blocos de saída
//!   com som numa janela de vários segundos, e quanto a saída continua tocando
//!   depois de o emissor parar — que é a profundidade do buffer, medida do lado
//!   de fora.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_audio::codec::VoiceEncoder;
use seele_audio::device::mentira;
use seele_audio::{FRAME_SAMPLES, SAMPLE_RATE_HZ};
use seele_core::enlace::{Destino, Enlace};
use seele_core::{MemoryPinStore, PinStore};
use seele_ffi::foco::FocoDeVoz;
use seele_ffi::{ConnectConfig, Connection, VoiceMode};
use seele_proto::MediaHeader;
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

mod vaga;

const PRAZO: Duration = Duration::from_secs(15);

/// Quanto quem voltou fica no servidor antes de sentar na voz. Mais que os
/// 5,12 s que a fila de voz do enlace guarda (256 quadros de 20 ms): no CUA
/// foram 12 s.
const ANTES_DE_SENTAR: Duration = Duration::from_secs(8);

/// O começo da janela de regime, depois de sentar. Folgado: o alvo do buffer
/// é 40 ms, e um receptor são toca em décimos.
const ASSENTAR: Duration = Duration::from_secs(1);

/// A janela de regime. Longa o bastante para passar dos 5,12 s de voz velha que
/// a fila guardava: é depois deles que a falha aparecia.
const JANELA: Duration = Duration::from_secs(9);

/// A fração mínima de blocos de saída com som na janela, com um tom contínuo
/// chegando sem perda nenhuma no fio. Com o defeito: ~0,5.
const MINIMO_COM_SOM: f64 = 0.9;

/// Quanto a saída pode continuar tocando depois de o emissor parar: o alvo do
/// buffer (40–200 ms), o anel do aparelho e a volta do laço. Com o defeito, a
/// cauda é a profundidade de ~5 s.
const TETO_DA_CAUDA: Duration = Duration::from_millis(800);

/// Onde a fala do emissor «está» quando o teste começa: ~22 min de quadros de
/// 20 ms, com a `seq` a 1 000 quadros (20 s) de dar a volta.
const SEQ_INICIAL: u16 = u16::MAX - 999;
const QUADROS_JA_FALADOS: u32 = 66_000;

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

/// O emissor contínuo: um par cru sentado, que manda um tom em Opus a cada
/// 20 ms, sem pausa e sem DTX, até `parar`. Nunca reconecta.
async fn emissor_continuo(
    endereco: SocketAddr,
    parar: Arc<AtomicBool>,
    mandados: Arc<AtomicU64>,
) -> Result<Enlace> {
    let enlace = Enlace::conectar(
        Destino {
            servidor: endereco,
            nome_tls: "localhost".into(),
            chave_do_pin: endereco.to_string(),
            apelido: "emissor".into(),
            segredo: None,
            esperada: None,
            o_pino_prova_o_servidor: true,
            aceito: None,
        },
        SigningKey::from_bytes(&[77; 32]),
        Arc::new(MemoryPinStore::new()) as Arc<dyn PinStore>,
    )
    .await?;
    let sala = enlace.sessao().voice_rooms[0].id;
    enlace.entrar_na_voice_room(sala, None).await?;
    let ssrc = enlace.sessao().ssrc.get();
    let canal = enlace.media();
    tokio::spawn(async move {
        let mut codificador = VoiceEncoder::with_defaults().expect("o Opus abriu");
        let mut seq = SEQ_INICIAL;
        let mut quadro = QUADROS_JA_FALADOS;
        let mut fase = 0.0_f32;
        let passo = 2.0 * std::f32::consts::PI * 440.0 / SAMPLE_RATE_HZ as f32;
        let mut datagrama = vec![0_u8; seele_proto::MAX_DATAGRAM_LEN];
        let mut cadencia = tokio::time::interval(Duration::from_millis(20));
        cadencia.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Burst);
        while !parar.load(Ordering::Relaxed) {
            cadencia.tick().await;
            let tom: Vec<f32> = (0..FRAME_SAMPLES)
                .map(|_| {
                    fase = (fase + passo) % (2.0 * std::f32::consts::PI);
                    0.3 * fase.sin()
                })
                .collect();
            let carga = codificador.encode(&tom).expect("o tom codificou");
            let cabecalho = MediaHeader {
                ssrc,
                seq,
                timestamp: quadro.wrapping_mul(FRAME_SAMPLES as u32),
            };
            let tamanho = cabecalho
                .encode_datagram(&carga, &mut datagrama)
                .expect("o datagrama montou");
            if canal.send(datagrama[..tamanho].to_vec()).is_ok() {
                mandados.fetch_add(1, Ordering::Relaxed);
            }
            seq = seq.wrapping_add(1);
            quadro = quadro.wrapping_add(1);
        }
    });
    Ok(enlace)
}

/// Quem ouve, pela FFI, com o aparelho de mentira e o foco dado.
async fn pela_ffi(
    endereco: SocketAddr,
    casa: &std::path::Path,
    aparelho: &str,
    foco: Arc<FocoDeVoz>,
) -> Arc<Connection> {
    let config = ConnectConfig {
        server: endereco.to_string(),
        alternate_servers: Vec::new(),
        nickname: "receptor".into(),
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
    .unwrap_or_else(|falha| panic!("o receptor não entrou: {falha:?}"));
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
    conexao.set_muted(true).expect("mudo");
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

/// A fração de blocos com som numa janela, e os dois deltas.
async fn fracao_com_som(aparelho: &str, janela: Duration) -> (f64, u64, u64) {
    let antes = mentira::contagem(aparelho);
    tokio::time::sleep(janela).await;
    let depois = mentira::contagem(aparelho);
    let blocos = depois.blocos - antes.blocos;
    let com_som = depois.blocos_com_som - antes.blocos_com_som;
    #[allow(clippy::cast_precision_loss, reason = "contagens de um teste")]
    let fracao = if blocos == 0 {
        0.0
    } else {
        com_som as f64 / blocos as f64
    };
    (fracao, com_som, blocos)
}

/// Quanto tempo a saída continua com som depois de agora: até 300 ms seguidos
/// sem nenhum bloco com som a mais.
async fn cauda(aparelho: &str, prazo: Duration) -> Duration {
    let comeco = Instant::now();
    let mut ultimo = mentira::contagem(aparelho).blocos_com_som;
    let mut ultimo_som = Duration::ZERO;
    while comeco.elapsed() < prazo {
        tokio::time::sleep(Duration::from_millis(10)).await;
        let agora = mentira::contagem(aparelho).blocos_com_som;
        if agora > ultimo {
            ultimo = agora;
            ultimo_som = comeco.elapsed();
        } else if comeco.elapsed() - ultimo_som >= Duration::from_millis(300) {
            break;
        }
    }
    ultimo_som
}

#[tokio::test(flavor = "multi_thread")]
async fn quem_volta_e_senta_depois_ouve_o_presente_e_sem_perda() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;
    let parar = Arc::new(AtomicBool::new(false));
    let mandados = Arc::new(AtomicU64::new(0));
    let _emissor = emissor_continuo(endereco, Arc::clone(&parar), Arc::clone(&mandados)).await?;

    // 1. Quem ouve entra, senta e ouve o emissor.
    let foco = FocoDeVoz::novo();
    let casa = tempfile::tempdir()?;
    let aparelho = "volta-e-senta-depois";
    let primeira = pela_ffi(endereco, casa.path(), aparelho, Arc::clone(&foco)).await;
    sentar_como_a_janela(&foco, &primeira).await;
    let tocados = || mentira::contagem(aparelho).blocos_com_som;
    let base = tocados();
    assert!(
        ate(PRAZO, || tocados() >= base + 100).await.is_some(),
        "antes de sair, quem ouve não tocou o emissor"
    );
    let (antes_fracao, antes_som, antes_blocos) = fracao_com_som(aparelho, ASSENTAR * 3).await;

    // 2. Sai do servidor inteiro, sentada. O emissor continua.
    primeira.disconnect();
    drop(primeira);
    assert!(
        ate(PRAZO, || mentira::contagem(aparelho).abertos == 0)
            .await
            .is_some(),
        "sair não fechou o aparelho de quem ouve"
    );
    tokio::time::sleep(Duration::from_secs(1)).await;

    // 3. Volta, com a mesma identidade e o mesmo foco — vazio, como o app o
    //    deixa: «o foco ficou vazio, e não passa sozinho». O servidor devolve o
    //    assento da graça; a voz não abre até sentar.
    let segunda = pela_ffi(endereco, casa.path(), aparelho, Arc::clone(&foco)).await;
    tokio::time::sleep(ANTES_DE_SENTAR).await;
    let aberturas = mentira::contagem(aparelho).aberturas;
    sentar_como_a_janela(&foco, &segunda).await;
    assert!(
        ate(PRAZO, || mentira::contagem(aparelho).aberturas > aberturas)
            .await
            .is_some(),
        "de volta e sentada, a voz de quem ouve não abriu"
    );

    // 4. O regime, e não o começo.
    tokio::time::sleep(ASSENTAR).await;
    let (fracao, com_som, blocos) = fracao_com_som(aparelho, JANELA).await;

    // 5. A profundidade, medida de fora: o emissor para, e a saída continua
    //    tocando o que o buffer guardava.
    parar.store(true, Ordering::Relaxed);
    let cauda = cauda(aparelho, Duration::from_secs(10)).await;

    println!(
        "antes de sair: {antes_som}/{antes_blocos} blocos com som ({antes_fracao:.3}); depois de \
         voltar e sentar {ANTES_DE_SENTAR:?} depois: {com_som}/{blocos} ({fracao:.3}) numa \
         janela de {JANELA:?}; a saída tocou {cauda:?} depois de o emissor parar; o emissor \
         mandou {} quadros",
        mandados.load(Ordering::Relaxed)
    );
    assert!(
        fracao >= MINIMO_COM_SOM,
        "de volta e sentada {ANTES_DE_SENTAR:?} depois, quem ouve tocou só {com_som} de {blocos} \
         blocos com som ({fracao:.3}) com um tom contínuo chegando: o mínimo é {MINIMO_COM_SOM}"
    );
    assert!(
        cauda <= TETO_DA_CAUDA,
        "a saída continuou tocando {cauda:?} depois de o emissor parar: é voz velha guardada \
         no buffer, e quem ouve está esse tanto atrás. O teto é {TETO_DA_CAUDA:?}"
    );

    segunda.disconnect();
    daemon.shutdown();
    Ok(())
}
