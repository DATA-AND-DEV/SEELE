//! **O foco de voz entre três sessões, contado no fio** (E3 da 0.17, R5 do
//! coordenador).
//!
//! Três `seeled` de verdade, em processo. Três sessões desta máquina pela FFI
//! (`Connection::connect_com_foco`), uma em cada servidor, todas com o mesmo
//! [`seele_ffi::foco::FocoDeVoz`] e com o **aparelho de mentira** do
//! `seele-audio` (um tom de 440 Hz que abre qualquer portão), em modo aberto,
//! todas sentadas na sala padrão do seu servidor. Em cada servidor, um ouvinte
//! de verdade (`Enlace`) sentado na mesma sala conta **os datagramas de voz que
//! o servidor encaminha a ele**, com a hora de chegada.
//!
//! O foco passa de A para B, de B para C e de C para A. O que se exige:
//!
//! - **em nenhum instante dois servidores recebem voz desta máquina**: o último
//!   datagrama que chega pelo servidor que perdeu o foco chega **antes** do
//!   primeiro que chega pelo servidor que o ganhou;
//! - cada servidor recebe voz **enquanto** tem o foco (a troca não cala todo
//!   mundo);
//! - entre sessões, a troca espera o aparelho da antiga fechar: uma vigia lê
//!   as contagens dos aparelhos de mentira o tempo todo e não vê dois abertos.
//!   É um diagnóstico, e não o critério: reabrir o aparelho dentro da mesma
//!   sessão pode ter duas capturas por um instante. O critério é o datagrama
//!   recebido por cada servidor, acima;
//! - a sessão sentada sem o foco diz `EmOutraSessao`.
//!
//! **O que isto não mede**: uma placa de som de verdade (CoreAudio, WASAPI), e
//! o caminho do app (os comandos Tauri, que `apps/seele-app/src/
//! varias_sessoes_testes.rs` mede contra servidores reais, sem áudio).

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use seele_audio::device::mentira;
use seele_core::enlace::{Destino, Enlace};
use seele_core::{MemoryPinStore, PinStore};
use seele_ffi::foco::FocoDeVoz;
use seele_ffi::{AberturaDoAudio, ConnectConfig, Connection, VoiceMode};
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

mod vaga;

/// Quanto cada sessão fala com o foco antes da troca seguinte.
const FALANDO: Duration = Duration::from_millis(1500);

async fn servidor(nome: &str) -> Result<(SocketAddr, Arc<Daemon>)> {
    let daemon = Arc::new(
        Daemon::bind(ServerConfig {
            name: nome.into(),
            listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            database: Location::Memory,
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

/// O ouvinte de um servidor: uma pessoa de outra máquina, sentada na sala
/// padrão, contando a voz que chega a ela.
async fn ouvinte(endereco: SocketAddr, semente: u8) -> Result<Enlace> {
    let enlace = Enlace::conectar(
        Destino {
            servidor: endereco,
            nome_tls: "localhost".into(),
            chave_do_pin: endereco.to_string(),
            apelido: format!("ouvinte-{semente}"),
            segredo: None,
            esperada: None,
            o_pino_prova_o_servidor: true,
            aceito: None,
        },
        ed25519_dalek::SigningKey::from_bytes(&[semente; 32]),
        Arc::new(MemoryPinStore::new()) as Arc<dyn PinStore>,
    )
    .await?;
    let sala = enlace.sessao().voice_rooms[0].id;
    enlace.entrar_na_voice_room(sala, None).await?;
    Ok(enlace)
}

/// Uma sessão desta máquina, pela FFI, no foco da máquina, com o aparelho de
/// mentira `aparelho`.
async fn sessao(
    endereco: SocketAddr,
    casa: &std::path::Path,
    foco: &Arc<FocoDeVoz>,
    aparelho: &str,
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
        capture_device: Some(mentira::id(aparelho)),
        playback_device: Some(mentira::id(aparelho)),
    };
    let foco = Arc::clone(foco);
    let (conexao, _) = tokio::task::spawn_blocking(move || {
        Connection::connect_com_foco(config, None, None, Some(foco), None)
    })
    .await
    .expect("a thread que conecta caiu")
    .unwrap_or_else(|falha| panic!("a sessão não entrou em {endereco}: {falha:?}"));
    // Aberto: o tom de mentira sai sem tecla, e a troca do foco não depende de
    // alguém segurar nada.
    conexao.set_voice_mode(VoiceMode::Open);
    conexao
}

async fn esperar(o_que: &str, prazo: Duration, mut pronto: impl FnMut() -> bool) {
    let fim = tokio::time::Instant::now() + prazo;
    while tokio::time::Instant::now() < fim {
        if pronto() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(pronto(), "{o_que} não aconteceu em {prazo:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn nunca_dois_servidores_recebem_a_voz_desta_maquina_ao_mesmo_tempo() -> Result<()> {
    let _vaga = vaga::minha();
    let nomes = ["foco-a", "foco-b", "foco-c"];
    let mut servidores = Vec::new();
    for nome in ["A", "B", "C"] {
        servidores.push(servidor(nome).await?);
    }

    // Os ouvintes, um por servidor, cada um contando o que chega com a hora.
    let chegadas: Arc<Mutex<Vec<(Instant, usize)>>> = Arc::new(Mutex::new(Vec::new()));
    let mut ouvintes = Vec::new();
    for (indice, (endereco, _)) in servidores.iter().enumerate() {
        let enlace = ouvinte(*endereco, 90 + u8::try_from(indice)?).await?;
        let canal = enlace.media();
        let chegadas = Arc::clone(&chegadas);
        tokio::spawn(async move {
            while let Ok(_datagrama) = canal.next().await {
                chegadas
                    .lock()
                    .expect("as chegadas")
                    .push((Instant::now(), indice));
            }
        });
        ouvintes.push(enlace);
    }

    // As três sessões desta máquina, no mesmo foco, sentadas.
    let foco = FocoDeVoz::novo();
    let casa = tempfile::tempdir()?;
    let mut sessoes = Vec::new();
    for (indice, (endereco, _)) in servidores.iter().enumerate() {
        sessoes.push(sessao(*endereco, casa.path(), &foco, nomes[indice]).await);
    }
    for conexao in &sessoes {
        let sala = conexao.snapshot().voice_rooms[0].id;
        conexao.enter_voice_room(sala, None).expect("sentar");
    }
    assert_eq!(
        foco.dono(),
        sessoes[0].id_no_foco(),
        "a primeira sessão não ficou com a voz"
    );

    // A vigia dos aparelhos (diagnóstico): entre sessões, nunca dois abertos.
    let parar = Arc::new(AtomicBool::new(false));
    let vigia = {
        let parar = Arc::clone(&parar);
        std::thread::spawn(move || {
            let mut maximo = 0;
            while !parar.load(Ordering::SeqCst) {
                let abertos: usize = nomes
                    .iter()
                    .map(|nome| mentira::contagem(nome).abertos)
                    .sum();
                maximo = maximo.max(abertos);
                std::thread::sleep(Duration::from_millis(1));
            }
            maximo
        })
    };

    esperar("a voz de A abrir", Duration::from_secs(10), || {
        mentira::contagem(nomes[0]).abertos == 1
    })
    .await;
    esperar(
        "as outras duas dizerem que a voz está noutra sessão",
        Duration::from_secs(10),
        || {
            sessoes[1..].iter().all(|conexao| {
                conexao.snapshot().abertura_do_audio == AberturaDoAudio::EmOutraSessao
            })
        },
    )
    .await;
    // A primeira época começa com a voz de A aberta.
    let mut trocas = vec![(Instant::now(), 0_usize)];
    tokio::time::sleep(FALANDO).await;

    // As trocas, com a hora em que cada uma voltou.
    for proxima in [1_usize, 2, 0] {
        let alvo = sessoes[proxima].id_no_foco();
        let foco_da_troca = Arc::clone(&foco);
        let troca = tokio::task::spawn_blocking(move || foco_da_troca.focar(alvo))
            .await
            .expect("a troca caiu")
            .expect("a troca foi recusada");
        assert!(
            troca.confirmada,
            "a sessão que tinha a voz não confirmou a troca"
        );
        trocas.push((Instant::now(), proxima));
        esperar("a voz da nova abrir", Duration::from_secs(10), || {
            mentira::contagem(nomes[proxima]).abertos == 1
        })
        .await;
        tokio::time::sleep(FALANDO).await;
    }
    parar.store(true, Ordering::SeqCst);
    // Conferida depois dos datagramas: o que o servidor recebe é o sintoma que
    // importa, e o vermelho dele diz mais que o da vigia.
    let maximo = vigia.join().expect("a vigia");

    // A conta, por época de foco: a época k vai de `trocas[k]` a `trocas[k+1]`
    // (a última, até agora). Em cada troca, a antiga e a nova são comparadas só
    // dentro das duas épocas que a troca separa.
    let fim_de_tudo = Instant::now();
    let chegadas = chegadas.lock().expect("as chegadas").clone();
    let entre = |servidor: usize, de: Instant, ate: Instant| -> Vec<Instant> {
        chegadas
            .iter()
            .filter(|(quando, quem)| *quem == servidor && *quando >= de && *quando < ate)
            .map(|(quando, _)| *quando)
            .collect()
    };
    for k in 0..trocas.len() - 1 {
        let (comeco, antiga) = trocas[k];
        let (troca, nova) = trocas[k + 1];
        let fim_da_nova = trocas.get(k + 2).map_or(fim_de_tudo, |(quando, _)| *quando);
        let da_antiga = entre(antiga, comeco, fim_da_nova);
        let da_nova_depois = entre(nova, comeco, fim_da_nova);
        assert!(
            !da_antiga.is_empty(),
            "o servidor {antiga} não recebeu voz nenhuma enquanto tinha o foco"
        );
        let ultima_da_antiga = da_antiga.iter().max().copied().expect("há chegadas");
        let primeira_da_nova = da_nova_depois.iter().min().copied();
        let Some(primeira_da_nova) = primeira_da_nova else {
            panic!("o servidor {nova} não recebeu voz nenhuma depois de ganhar o foco");
        };
        assert!(
            ultima_da_antiga < primeira_da_nova,
            "o servidor {antiga} ainda recebia voz ({:?} depois da troca) quando o servidor \
             {nova} já recebia ({:?} depois da troca): o microfone foi a dois servidores",
            ultima_da_antiga.saturating_duration_since(troca),
            primeira_da_nova.saturating_duration_since(troca)
        );
        assert!(
            primeira_da_nova >= troca,
            "o servidor {nova} recebeu voz ({:?} antes da troca) enquanto o foco era do \
             servidor {antiga}",
            troca.saturating_duration_since(primeira_da_nova)
        );
    }
    let por_servidor: Vec<usize> = (0..3)
        .map(|indice| {
            chegadas
                .iter()
                .filter(|(_, servidor)| *servidor == indice)
                .count()
        })
        .collect();
    println!(
        "datagramas de voz por servidor: {por_servidor:?}; trocas: {}",
        trocas.len() - 1
    );
    assert!(
        maximo <= 1,
        "dois aparelhos de voz desta máquina estiveram abertos ao mesmo tempo ({maximo})"
    );

    for conexao in &sessoes {
        conexao.disconnect();
    }
    drop(ouvintes);
    for (_, daemon) in servidores {
        daemon.shutdown();
    }
    Ok(())
}
