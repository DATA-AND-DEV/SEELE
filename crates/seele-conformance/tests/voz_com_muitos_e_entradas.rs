//! Voz real (Opus, enlace, servidor, laço e saída sintética) sob fan-out e entradas.
//! Não homologa drivers, acústica nem capacidade de um link de Internet.
//! O caso funcional usa oito falantes. SEELE_CARGA_FALANTES=24/32/48/64 amplia
//! a bancada de capacidade; todos os clientes e o servidor disputam esta máquina.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_audio::{codec::VoiceEncoder, device::mentira, FRAME_SAMPLES};
use seele_core::{
    enlace::{Destino, Enlace},
    voice::{DeviceChoice, Voice, VoiceMode},
    MemoryPinStore,
};
use seele_proto::MediaHeader;
use seele_server::{persistence::Location, Daemon, ServerConfig};
use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

async fn entrar(addr: SocketAddr, numero: u8) -> Result<Enlace> {
    Ok(Enlace::conectar(
        Destino {
            servidor: addr,
            nome_tls: "localhost".into(),
            chave_do_pin: addr.to_string(),
            apelido: format!("pessoa{numero}"),
            segredo: None,
            esperada: None,
            o_pino_prova_o_servidor: true,
            aceito: None,
        },
        SigningKey::from_bytes(&[numero; 32]),
        Arc::new(MemoryPinStore::new()),
    )
    .await?)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn muitas_vozes_e_novas_entradas_nao_param_o_microfone_nem_a_saida() -> Result<()> {
    let n: usize = std::env::var("SEELE_CARGA_FALANTES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8);
    assert!((2..=64).contains(&n));
    let servidor = Arc::new(
        Daemon::bind(ServerConfig {
            listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            database: Location::Memory,
            voice_room_limit: 80,
            caminho_bps: Some(100_000_000),
            ..ServerConfig::default()
        })
        .await?,
    );
    let addr = servidor.local_addr()?;
    let srv = servidor.clone();
    tokio::spawn(async move {
        let _ = srv.run().await;
    });
    let ouvinte = entrar(addr, 1).await?;
    let sala = ouvinte.sessao().voice_rooms[0].id;
    ouvinte.entrar_na_voice_room(sala, None).await?;
    let mut pares = Vec::new();
    let mut canais = Vec::new();
    let mut drenos = Vec::new();
    let retornos = Arc::new(std::sync::Mutex::new(Vec::<Instant>::new()));
    for i in 0..n {
        // Todos vêm de loopback: respeita o balde de 30 apertos/minuto por IP.
        if i >= 24 {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        let p = entrar(addr, u8::try_from(i + 2)?).await?;
        p.entrar_na_voice_room(sala, None).await?;
        canais.push((p.media(), p.sessao().ssrc.get()));
        let media = p.media();
        let retornos = retornos.clone();
        let eu = ouvinte.sessao().ssrc.get();
        drenos.push(tokio::spawn(async move {
            while let Ok(b) = media.next().await {
                if i == 0 && MediaHeader::decode(&b).is_ok_and(|(h, _)| h.ssrc == eu) {
                    retornos.lock().unwrap().push(Instant::now());
                }
            }
        }));
        pares.push(p);
    }
    let nome = "muitos-e-entradas";
    let voz = Voice::start_on(
        &DeviceChoice {
            capture: Some(mentira::id(nome)),
            playback: Some(mentira::id(nome)),
        },
        ouvinte.media(),
        ouvinte.sessao().ssrc,
    )?;
    voz.set_mode(VoiceMode::Open);
    voz.set_supressao(0.0);
    let mut encoder = VoiceEncoder::with_defaults()?;
    let amostras: Vec<f32> = (0..FRAME_SAMPLES)
        .map(|k| (k as f32 * 440.0 * std::f32::consts::TAU / 48000.0).sin() * 0.2)
        .collect();
    let mut payloads = Vec::new();
    for _ in 0..600 {
        payloads.push(encoder.encode(&amostras)?);
    }
    let parar = Arc::new(AtomicBool::new(false));
    let fim = parar.clone();
    // Relógio de envio independente do executor: falantes escalonados ao longo de 20 ms.
    let emissor = std::thread::spawn(move || {
        let inicio = Instant::now();
        let mut seq = 0u16;
        while !fim.load(Ordering::Relaxed) {
            for (i, (canal, ssrc)) in canais.iter().enumerate() {
                let prazo = inicio
                    + Duration::from_micros(
                        u64::from(seq) * 20_000 + (i as u64) * 20_000 / n as u64,
                    );
                let espera = prazo.saturating_duration_since(Instant::now());
                if !espera.is_zero() {
                    std::thread::sleep(espera);
                }
                let header = MediaHeader {
                    ssrc: *ssrc,
                    seq,
                    timestamp: u32::from(seq) * 960,
                };
                let mut b = vec![0; seele_proto::MAX_DATAGRAM_LEN];
                let len = header
                    .encode_datagram(&payloads[usize::from(seq) % payloads.len()], &mut b)
                    .unwrap();
                b.truncate(len);
                let _ = canal.send(b);
            }
            seq = seq.wrapping_add(1);
        }
    });
    tokio::time::sleep(Duration::from_secs(3)).await;
    let antes = mentira::contagem(nome);
    let inicio = Instant::now();
    let mut tentativas = Vec::new();
    for i in 0..8 {
        tentativas.push(entrar(addr, 100 + i).await?);
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    tokio::time::sleep(Duration::from_secs(3)).await;
    let depois = mentira::contagem(nome);
    let duracao = inicio.elapsed();
    let telemetria = voz.telemetry();
    parar.store(true, Ordering::Relaxed);
    emissor.join().unwrap();
    let instantes = retornos.lock().unwrap();
    let intervalos: Vec<f64> = instantes
        .windows(2)
        .filter(|p| p[0] >= inicio)
        .map(|p| p[1].duration_since(p[0]).as_secs_f64() * 1000.0)
        .collect();
    let maior = intervalos.iter().copied().fold(0.0, f64::max);
    let tocados = depois.blocos_com_som - antes.blocos_com_som;
    let blocos = depois.blocos - antes.blocos;
    println!(
        "{}",
        serde_json::json!({"falantes":n,"entradas":8,"duracao_s":duracao.as_secs_f64(),"blocos":blocos,"blocos_com_som":tocados,"maior_intervalo_microfone_ms":maior,"pacotes_microfone":intervalos.len(),"fontes":telemetria.sources.len(),"pior_perda":telemetria.sources.iter().map(|s|s.loss_fraction).fold(0.0_f64,f64::max),"quadros_ocultados":telemetria.sources.iter().map(|s|s.concealed_frames).sum::<u64>(),"envios_recusados":voz.quadros_recusados(),"atraso_laco_ms":telemetria.local.playout_worst_lateness_ms,"reposicoes":telemetria.local.playout_catchup_frames,"reacertos":telemetria.local.playout_resyncs,"underruns":telemetria.local.playback_underruns})
    );
    drop(instantes);
    drop(voz);
    for d in drenos {
        d.abort();
    }
    servidor.shutdown();
    assert!(
        telemetria.sources.iter().all(|s| s.loss_fraction < 0.05),
        "a perda de voz passou de 5%; som de ocultação não conta como qualidade"
    );
    assert_eq!(telemetria.sources.len(), n, "todas as vozes têm de chegar");
    assert!(
        tocados * 100 >= blocos * 90,
        "a saída ficou sem som em mais de 10% dos blocos"
    );
    assert!(
        intervalos.len() as f64 >= duracao.as_secs_f64() * 35.0,
        "o microfone não manteve a cadência"
    );
    assert!(maior < 200.0, "o envio do microfone parou por {maior} ms");
    Ok(())
}
