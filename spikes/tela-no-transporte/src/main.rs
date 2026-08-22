//! Prova de desenho: o que o compartilhamento de tela faz com a voz.
//!
//! **Descartável.** Fora do workspace, como os três spikes anteriores. Existe
//! para responder **uma** pergunta antes de a spec escolher um transporte para
//! o vídeo, e morre com a resposta. Nada pode depender dele.
//!
//! # A pergunta
//!
//! O produto já tem uma conexão QUIC por par: voz em datagramas, controle e
//! texto em fluxos (`specs/02-protocolo.md`). O vídeo cabe em qual dos dois —
//! e, quando a subida da casa não dá conta, **o que sobra da voz?**
//!
//! A spec não consegue responder sozinha porque a resposta não está no RFC:
//! está em como o `quinn` 0.11 ordena o que sai dentro da janela de
//! congestionamento, em quanto ele deixa enfileirar antes de descartar, e em
//! quanto de fila o gargalo do caminho acumula. Três detalhes de
//! implementação, e os três decidem se a voz engasga.
//!
//! # O que este binário monta
//!
//! Um par QUIC inteiro dentro de um processo, com um **cano estreito no meio**:
//!
//! ```text
//! cliente ──▶ [ cano: banda fixa, fila com teto, atraso ] ──▶ servidor
//!         ◀──────────── só atraso, sem estreitar ────────────
//! ```
//!
//! Só a **subida** é estreitada. Numa casa é ela que aperta, e o
//! compartilhamento de tela é subida quase pura.
//!
//! O cliente e o servidor moram no mesmo processo de propósito: assim os dois
//! leem o **mesmo relógio**, e o atraso de ponta a ponta de cada quadro de voz
//! é medido de verdade, não estimado por metade do RTT.
//!
//! # O que ele não faz, e por quê
//!
//! **Não codifica vídeo.** A pergunta é do transporte. A carga tem forma de
//! vídeo — 30 quadros por segundo, um quadro-chave a cada dois segundos com
//! cinco vezes o tamanho, bitrate alvo acima do que o cano aguenta — e é isso
//! que o QUIC vê. Um H.264 de verdade no meio acrescentaria uma variável que
//! não está sob prova e um número de CPU que não é o desta pergunta.
//!
//! # Uso
//!
//! ```text
//! cargo run --release                    # a matriz inteira
//! cargo run --release -- --modo fluxo    # um cenário só
//! cargo run --release -- --banda-kbps 1000 --fila-kib 128
//! ```

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use tokio::net::UdpSocket;
use tokio::time::Instant;

/// Tamanho de um datagrama de voz do produto.
///
/// 11 bytes de cabeçalho (`specs/02-protocolo.md`) mais ~80 de payload Opus a
/// 32 kbps. É o número que a `seele-proto::media` documenta.
const VOZ_BYTES: usize = 91;
/// Um quadro de 20 ms, que é a cadência de `specs/03-audio.md`.
const VOZ_INTERVALO: Duration = Duration::from_millis(20);

const VIDEO_FPS: u64 = 30;
/// Quadro-chave a cada dois segundos, como qualquer encoder de tela faz.
const QUADROS_ENTRE_CHAVES: u64 = 60;
/// Quanto um quadro-chave é maior que um quadro comum, quando o cenário não
/// diz outra coisa. É a rajada que estoura a fila do gargalo, e é ela que a
/// voz sente.
const CHAVE_VEZES: u64 = 5;

/// Primeiro byte de cada datagrama, para o receptor separar voz de vídeo.
const MARCA_VOZ: u8 = 1;
const MARCA_VIDEO: u8 = 2;

/// O que o modo `datagrama` corta de um quadro de vídeo em cada datagrama.
const PEDACO_VIDEO: usize = 1_000;

/// Onde o vídeo viaja.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Modo {
    /// Só voz. A linha de base contra a qual todo o resto é lido.
    Voz,
    /// Vídeo num fluxo unidirecional QUIC, na mesma conexão da voz.
    Fluxo,
    /// Vídeo em datagramas QUIC, na mesma conexão da voz.
    Datagrama,
    /// Vídeo num fluxo, numa **segunda** conexão QUIC pelo mesmo caminho.
    DuasConexoes,
}

/// Qual controle de congestionamento o `quinn` usa.
#[derive(Clone, Copy)]
enum Controle {
    /// O padrão do `quinn`, e portanto o do produto hoje.
    Cubic,
    Bbr,
}

#[derive(Clone, Copy)]
struct Cenario {
    nome: &'static str,
    modo: Modo,
    controle: Controle,
    video_kbps: u64,
    /// `TransportConfig::datagram_send_buffer_size`. O padrão do `quinn` é
    /// 1 MiB e o produto não o toca — este campo existe para medir o que esse
    /// padrão custa.
    buffer_datagrama_kib: usize,
    /// Quantas vezes o quadro-chave é maior que o comum. `1` desliga a rajada
    /// — é assim que se separa o que a rajada custa do que o bitrate custa.
    chave_vezes: u64,
}

/// O caminho entre as duas casas.
#[derive(Clone, Copy)]
struct Caminho {
    banda_kbps: u64,
    fila_kib: usize,
    /// Atraso de propagação em cada direção. 20 + 20 dá 40 ms de RTT, que é
    /// uma conversa entre duas casas do mesmo país.
    atraso_ms: u64,
    segundos: u64,
}

#[derive(Default)]
struct Coleta {
    voz_recebidas: u64,
    atrasos_us: Vec<u64>,
    video_bytes: u64,
}

struct Resultado {
    nome: &'static str,
    enviadas: u64,
    recebidas: u64,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    maximo_ms: f64,
    video_kbps: f64,
}

#[tokio::main]
async fn main() {
    let mut caminho = Caminho {
        banda_kbps: 2_000,
        fila_kib: 64,
        atraso_ms: 20,
        segundos: 10,
    };
    let mut so_este: Option<String> = None;

    let mut argv = std::env::args().skip(1);
    while let Some(bandeira) = argv.next() {
        match bandeira.as_str() {
            "--banda-kbps" => {
                caminho.banda_kbps = argv
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(caminho.banda_kbps)
            }
            "--fila-kib" => {
                caminho.fila_kib = argv
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(caminho.fila_kib)
            }
            "--atraso-ms" => {
                caminho.atraso_ms = argv
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(caminho.atraso_ms)
            }
            "--segundos" => {
                caminho.segundos = argv
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(caminho.segundos)
            }
            "--modo" => so_este = argv.next(),
            outra => {
                eprintln!("bandeira desconhecida: {outra}");
                std::process::exit(2);
            }
        }
    }

    // Alinhado à mão e mantido assim: a matriz é a pergunta do spike escrita
    // por extenso, e lida em coluna. O `rustfmt` a quebraria em sete blocos.
    #[rustfmt::skip]
    let matriz = [
        Cenario { nome: "voz sozinha",             modo: Modo::Voz,          controle: Controle::Cubic, video_kbps: 0,     buffer_datagrama_kib: 1024, chave_vezes: CHAVE_VEZES },
        Cenario { nome: "fluxo, cubic",            modo: Modo::Fluxo,        controle: Controle::Cubic, video_kbps: 4_000, buffer_datagrama_kib: 1024, chave_vezes: CHAVE_VEZES },
        Cenario { nome: "fluxo, bbr",              modo: Modo::Fluxo,        controle: Controle::Bbr,   video_kbps: 4_000, buffer_datagrama_kib: 1024, chave_vezes: CHAVE_VEZES },
        Cenario { nome: "folga 60%, cubic",        modo: Modo::Fluxo,        controle: Controle::Cubic, video_kbps: 1_200, buffer_datagrama_kib: 1024, chave_vezes: CHAVE_VEZES },
        Cenario { nome: "folga 60%, chave espalhada", modo: Modo::Fluxo,     controle: Controle::Cubic, video_kbps: 1_200, buffer_datagrama_kib: 1024, chave_vezes: 1 },
        Cenario { nome: "datagrama, buffer 1MiB",  modo: Modo::Datagrama,    controle: Controle::Cubic, video_kbps: 4_000, buffer_datagrama_kib: 1024, chave_vezes: CHAVE_VEZES },
        Cenario { nome: "datagrama, buffer 32KiB", modo: Modo::Datagrama,    controle: Controle::Cubic, video_kbps: 4_000, buffer_datagrama_kib: 32,   chave_vezes: CHAVE_VEZES },
        Cenario { nome: "segunda conexao, cubic",  modo: Modo::DuasConexoes, controle: Controle::Cubic, video_kbps: 4_000, buffer_datagrama_kib: 1024, chave_vezes: CHAVE_VEZES },
    ];

    println!(
        "caminho: subida {} kbps, fila {} KiB ({} ms de enfileiramento cheio), \
         atraso {} ms por sentido, {} s por cenario",
        caminho.banda_kbps,
        caminho.fila_kib,
        caminho.fila_kib as u64 * 1024 * 8 * 1000 / (caminho.banda_kbps * 1000),
        caminho.atraso_ms,
        caminho.segundos,
    );
    println!(
        "voz: {VOZ_BYTES} bytes a cada {} ms\n",
        VOZ_INTERVALO.as_millis()
    );

    let mut resultados = Vec::new();
    for cenario in matriz {
        if let Some(filtro) = &so_este {
            if !cenario.nome.contains(filtro.as_str()) {
                continue;
            }
        }
        eprintln!("... {}", cenario.nome);
        match executar(cenario, caminho).await {
            Ok(resultado) => resultados.push(resultado),
            Err(erro) => eprintln!("cenario {} falhou: {erro}", cenario.nome),
        }
    }

    println!(
        "\n{:<26} {:>6} {:>6} {:>7} {:>8} {:>8} {:>8} {:>9} {:>10}",
        "cenario", "env", "rec", "perda", "p50 ms", "p95 ms", "p99 ms", "pior ms", "video kbps"
    );
    for r in &resultados {
        let perda = if r.enviadas == 0 {
            0.0
        } else {
            (r.enviadas.saturating_sub(r.recebidas)) as f64 * 100.0 / r.enviadas as f64
        };
        println!(
            "{:<26} {:>6} {:>6} {:>6.2}% {:>8.1} {:>8.1} {:>8.1} {:>9.1} {:>10.0}",
            r.nome,
            r.enviadas,
            r.recebidas,
            perda,
            r.p50_ms,
            r.p95_ms,
            r.p99_ms,
            r.maximo_ms,
            r.video_kbps
        );
    }
}

async fn executar(cenario: Cenario, caminho: Caminho) -> Result<Resultado, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let gerado = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()])
        .map_err(|erro| format!("certificado: {erro}"))?;
    let cadeia = vec![rustls::pki_types::CertificateDer::from(
        gerado.cert.der().to_vec(),
    )];
    let chave = rustls::pki_types::PrivatePkcs8KeyDer::from(gerado.signing_key.serialize_der());

    let mut tls_servidor = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cadeia.clone(), chave.into())
        .map_err(|erro| format!("tls do servidor: {erro}"))?;
    tls_servidor.alpn_protocols = vec![b"spike-tela".to_vec()];
    let quic_servidor = quinn::crypto::rustls::QuicServerConfig::try_from(tls_servidor)
        .map_err(|erro| format!("quic do servidor: {erro}"))?;
    let mut config_servidor = quinn::ServerConfig::with_crypto(Arc::new(quic_servidor));
    config_servidor.transport_config(Arc::new(transporte(cenario)));

    let servidor = quinn::Endpoint::server(config_servidor, ([127, 0, 0, 1], 0).into())
        .map_err(|erro| format!("escutar: {erro}"))?;
    let endereco_servidor = servidor.local_addr().map_err(|erro| erro.to_string())?;

    // --- o cano ---
    let boca = Arc::new(
        UdpSocket::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .map_err(|erro| erro.to_string())?,
    );
    let cauda = Arc::new(
        UdpSocket::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .map_err(|erro| erro.to_string())?,
    );
    let endereco_boca = boca.local_addr().map_err(|erro| erro.to_string())?;
    let cliente_visto: Arc<Mutex<Option<SocketAddr>>> = Arc::new(Mutex::new(None));
    let descartes = Arc::new(AtomicU64::new(0));

    let subida = tokio::spawn(encanar(
        Arc::clone(&boca),
        Arc::clone(&cauda),
        Destino::Fixo(endereco_servidor),
        Some(Arc::clone(&cliente_visto)),
        Cano {
            banda_bps: Some(caminho.banda_kbps * 1_000),
            fila_max: caminho.fila_kib * 1024,
            atraso: Duration::from_millis(caminho.atraso_ms),
        },
        Arc::clone(&descartes),
    ));
    let descida = tokio::spawn(encanar(
        Arc::clone(&cauda),
        Arc::clone(&boca),
        Destino::Aprendido(Arc::clone(&cliente_visto)),
        None,
        Cano {
            // A descida não é estreitada: numa casa quem aperta é a subida, e
            // estreitar as duas mediria duas coisas ao mesmo tempo.
            banda_bps: None,
            fila_max: usize::MAX,
            atraso: Duration::from_millis(caminho.atraso_ms),
        },
        Arc::clone(&descartes),
    ));

    // --- servidor: recolhe ---
    let coleta = Arc::new(Mutex::new(Coleta::default()));
    let t0 = Instant::now();
    let recolhendo = {
        let coleta = Arc::clone(&coleta);
        tokio::spawn(async move {
            while let Some(chegando) = servidor.accept().await {
                let coleta = Arc::clone(&coleta);
                tokio::spawn(async move {
                    if let Ok(conexao) = chegando.await {
                        atender(conexao, coleta, t0).await;
                    }
                });
            }
        })
    };

    // --- cliente ---
    let mut raiz = rustls::RootCertStore::empty();
    if let Some(certificado) = cadeia.first() {
        raiz.add(certificado.clone())
            .map_err(|erro| format!("raiz: {erro}"))?;
    }
    let mut tls_cliente = rustls::ClientConfig::builder()
        .with_root_certificates(raiz)
        .with_no_client_auth();
    tls_cliente.alpn_protocols = vec![b"spike-tela".to_vec()];
    let quic_cliente = quinn::crypto::rustls::QuicClientConfig::try_from(tls_cliente)
        .map_err(|erro| format!("quic do cliente: {erro}"))?;
    let mut config_cliente = quinn::ClientConfig::new(Arc::new(quic_cliente));
    config_cliente.transport_config(Arc::new(transporte(cenario)));

    let mut cliente = quinn::Endpoint::client(SocketAddr::from(([127, 0, 0, 1], 0)))
        .map_err(|erro| format!("cliente: {erro}"))?;
    cliente.set_default_client_config(config_cliente);

    let conexao = cliente
        .connect(endereco_boca, "localhost")
        .map_err(|erro| format!("conectar: {erro}"))?
        .await
        .map_err(|erro| format!("aperto de mao: {erro}"))?;

    let ate = Instant::now() + Duration::from_secs(caminho.segundos);
    let enviadas = Arc::new(AtomicU64::new(0));

    let voz = tokio::spawn(manda_voz(conexao.clone(), t0, ate, Arc::clone(&enviadas)));

    let video = match cenario.modo {
        Modo::Voz => None,
        Modo::Fluxo => Some(tokio::spawn(manda_video_fluxo(
            conexao.clone(),
            cenario.video_kbps,
            cenario.chave_vezes,
            ate,
        ))),
        Modo::Datagrama => Some(tokio::spawn(manda_video_datagrama(
            conexao.clone(),
            cenario.video_kbps,
            cenario.chave_vezes,
            ate,
        ))),
        Modo::DuasConexoes => {
            let segunda = cliente
                .connect(endereco_boca, "localhost")
                .map_err(|erro| format!("segunda conexao: {erro}"))?
                .await
                .map_err(|erro| format!("segundo aperto de mao: {erro}"))?;
            Some(tokio::spawn(manda_video_fluxo(
                segunda,
                cenario.video_kbps,
                cenario.chave_vezes,
                ate,
            )))
        }
    };

    let _ = voz.await;
    if let Some(video) = video {
        let _ = video.await;
    }
    // Deixa o que já saiu chegar antes de contar perda. A fila do cano tem
    // teto, então o rabo é curto e limitado.
    tokio::time::sleep(Duration::from_millis(800)).await;

    recolhendo.abort();
    subida.abort();
    descida.abort();

    let enviadas = enviadas.load(Ordering::Relaxed);
    let mut coletado = coleta.lock().map_err(|_| "coleta envenenada".to_owned())?;
    coletado.atrasos_us.sort_unstable();

    Ok(Resultado {
        nome: cenario.nome,
        enviadas,
        recebidas: coletado.voz_recebidas,
        p50_ms: percentil(&coletado.atrasos_us, 0.50),
        p95_ms: percentil(&coletado.atrasos_us, 0.95),
        p99_ms: percentil(&coletado.atrasos_us, 0.99),
        maximo_ms: percentil(&coletado.atrasos_us, 1.0),
        video_kbps: coletado.video_bytes as f64 * 8.0 / 1_000.0 / caminho.segundos as f64,
    })
}

fn transporte(cenario: Cenario) -> quinn::TransportConfig {
    let mut config = quinn::TransportConfig::default();
    // O produto liga o recebimento de datagramas no servidor; sem isto o
    // `quinn` os negocia desligados e o cenário inteiro não roda.
    config.datagram_receive_buffer_size(Some(1024 * 1024));
    config.datagram_send_buffer_size(cenario.buffer_datagrama_kib * 1024);
    if let Ok(ocioso) = quinn::IdleTimeout::try_from(Duration::from_secs(20)) {
        config.max_idle_timeout(Some(ocioso));
    }
    match cenario.controle {
        Controle::Cubic => {
            config
                .congestion_controller_factory(Arc::new(quinn::congestion::CubicConfig::default()));
        }
        Controle::Bbr => {
            config.congestion_controller_factory(Arc::new(quinn::congestion::BbrConfig::default()));
        }
    }
    config
}

fn percentil(ordenados: &[u64], fracao: f64) -> f64 {
    if ordenados.is_empty() {
        return f64::NAN;
    }
    let ultimo = ordenados.len() - 1;
    let indice = ((ordenados.len() as f64 - 1.0) * fracao).round() as usize;
    ordenados
        .get(indice.min(ultimo))
        .map_or(f64::NAN, |valor| *valor as f64 / 1_000.0)
}

// --------------------------------------------------------------------------
// carga
// --------------------------------------------------------------------------

async fn manda_voz(
    conexao: quinn::Connection,
    t0: Instant,
    ate: Instant,
    enviadas: Arc<AtomicU64>,
) {
    let mut seq: u64 = 0;
    let mut proximo = Instant::now();
    while Instant::now() < ate {
        proximo += VOZ_INTERVALO;
        tokio::time::sleep_until(proximo).await;

        let mut quadro = vec![0_u8; VOZ_BYTES];
        if let Some(marca) = quadro.get_mut(0) {
            *marca = MARCA_VOZ;
        }
        if let Some(campo) = quadro.get_mut(1..9) {
            campo.copy_from_slice(&seq.to_be_bytes());
        }
        // O carimbo é lido pelo servidor no mesmo processo, logo no mesmo
        // relógio: o atraso medido é o de ponta a ponta, não metade do RTT.
        let carimbo = t0.elapsed().as_micros() as u64;
        if let Some(campo) = quadro.get_mut(9..17) {
            campo.copy_from_slice(&carimbo.to_be_bytes());
        }

        // É o que o produto chama: descarta o **mais velho** da fila quando
        // ela enche, nunca bloqueia.
        if conexao.send_datagram(Bytes::from(quadro)).is_ok() {
            enviadas.fetch_add(1, Ordering::Relaxed);
        }
        seq = seq.wrapping_add(1);
    }
}

/// Tamanhos de quadro de uma tela a `alvo_kbps`, com quadro-chave periódico.
fn quadro_de_video(alvo_kbps: u64, indice: u64, chave_vezes: u64) -> usize {
    let comum = alvo_kbps * 1_000 / 8 / VIDEO_FPS;
    let tamanho = if indice.is_multiple_of(QUADROS_ENTRE_CHAVES) {
        comum * chave_vezes
    } else {
        comum
    };
    tamanho as usize
}

async fn manda_video_fluxo(
    conexao: quinn::Connection,
    alvo_kbps: u64,
    chave_vezes: u64,
    ate: Instant,
) {
    let Ok(mut fluxo) = conexao.open_uni().await else {
        return;
    };
    let intervalo = Duration::from_nanos(1_000_000_000 / VIDEO_FPS);
    let mut indice: u64 = 0;
    let mut proximo = Instant::now();
    while Instant::now() < ate {
        proximo += intervalo;
        tokio::time::sleep_until(proximo).await;
        let quadro = vec![MARCA_VIDEO; quadro_de_video(alvo_kbps, indice, chave_vezes)];
        // `write_all` espera a janela abrir. É o encoder sendo segurado pelo
        // caminho, que é exatamente o caso sob prova.
        if fluxo.write_all(&quadro).await.is_err() {
            return;
        }
        indice = indice.wrapping_add(1);
    }
    let _ = fluxo.finish();
}

async fn manda_video_datagrama(
    conexao: quinn::Connection,
    alvo_kbps: u64,
    chave_vezes: u64,
    ate: Instant,
) {
    let teto = conexao
        .max_datagram_size()
        .unwrap_or(PEDACO_VIDEO)
        .min(PEDACO_VIDEO);
    let intervalo = Duration::from_nanos(1_000_000_000 / VIDEO_FPS);
    let mut indice: u64 = 0;
    let mut proximo = Instant::now();
    while Instant::now() < ate {
        proximo += intervalo;
        tokio::time::sleep_until(proximo).await;
        let mut restante = quadro_de_video(alvo_kbps, indice, chave_vezes);
        while restante > 0 {
            let pedaco = restante.min(teto);
            let mut dados = vec![MARCA_VIDEO; pedaco];
            if let Some(marca) = dados.get_mut(0) {
                *marca = MARCA_VIDEO;
            }
            // Mesma chamada que a voz usa, e portanto a **mesma fila**.
            let _ = conexao.send_datagram(Bytes::from(dados));
            restante -= pedaco;
        }
        indice = indice.wrapping_add(1);
    }
}

// --------------------------------------------------------------------------
// servidor
// --------------------------------------------------------------------------

async fn atender(conexao: quinn::Connection, coleta: Arc<Mutex<Coleta>>, t0: Instant) {
    {
        let conexao = conexao.clone();
        let coleta = Arc::clone(&coleta);
        tokio::spawn(async move {
            while let Ok(fluxo) = conexao.accept_uni().await {
                let coleta = Arc::clone(&coleta);
                tokio::spawn(async move {
                    let mut fluxo = fluxo;
                    let mut buffer = vec![0_u8; 64 * 1024];
                    while let Ok(Some(lidos)) = fluxo.read(&mut buffer).await {
                        if let Ok(mut guarda) = coleta.lock() {
                            guarda.video_bytes += lidos as u64;
                        }
                    }
                });
            }
        });
    }

    while let Ok(dados) = conexao.read_datagram().await {
        let Some(marca) = dados.first() else { continue };
        if *marca == MARCA_VIDEO {
            if let Ok(mut guarda) = coleta.lock() {
                guarda.video_bytes += dados.len() as u64;
            }
            continue;
        }
        let Some(campo) = dados.get(9..17) else {
            continue;
        };
        let mut carimbo = [0_u8; 8];
        carimbo.copy_from_slice(campo);
        let saiu_us = u64::from_be_bytes(carimbo);
        let chegou_us = t0.elapsed().as_micros() as u64;
        if let Ok(mut guarda) = coleta.lock() {
            guarda.voz_recebidas += 1;
            guarda.atrasos_us.push(chegou_us.saturating_sub(saiu_us));
        }
    }
}

// --------------------------------------------------------------------------
// o cano
// --------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Cano {
    /// `None` = sem limite de banda.
    banda_bps: Option<u64>,
    /// Teto da fila. Cheia, o pacote novo é descartado — cauda, como um
    /// roteador de casa.
    fila_max: usize,
    atraso: Duration,
}

enum Destino {
    Fixo(SocketAddr),
    Aprendido(Arc<Mutex<Option<SocketAddr>>>),
}

async fn encanar(
    entrada: Arc<UdpSocket>,
    saida: Arc<UdpSocket>,
    destino: Destino,
    aprender: Option<Arc<Mutex<Option<SocketAddr>>>>,
    cano: Cano,
    descartes: Arc<AtomicU64>,
) {
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut fila: VecDeque<(Instant, Vec<u8>)> = VecDeque::new();
    let mut bytes_na_fila: usize = 0;
    let mut livre_em = Instant::now();

    loop {
        let alvo = fila.front().map(|(pronto, dados)| {
            let serial = match cano.banda_bps {
                Some(bps) if bps > 0 => {
                    Duration::from_nanos(dados.len() as u64 * 8 * 1_000_000_000 / bps)
                }
                _ => Duration::ZERO,
            };
            livre_em.max(*pronto) + serial
        });

        // Despacha antes de voltar ao `select`. Sem esta conferência, um fluxo
        // de entrada sempre pronto pode fazer o `select` nunca escolher o
        // temporizador, e o cano deixaria de esvaziar — mediria o escalonador
        // do tokio, não o caminho.
        if let Some(quando) = alvo {
            if quando <= Instant::now() {
                if let Some((_, dados)) = fila.pop_front() {
                    bytes_na_fila = bytes_na_fila.saturating_sub(dados.len());
                    livre_em = quando;
                    if let Some(para) = para_onde(&destino) {
                        let _ = saida.send_to(&dados, para).await;
                    }
                }
                continue;
            }
        }

        tokio::select! {
            recebido = entrada.recv_from(&mut buffer) => {
                let Ok((tamanho, de)) = recebido else { continue };
                if let Some(mapa) = &aprender {
                    if let Ok(mut guarda) = mapa.lock() {
                        *guarda = Some(de);
                    }
                }
                let Some(bytes) = buffer.get(..tamanho) else { continue };
                if bytes_na_fila + tamanho > cano.fila_max {
                    descartes.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
                bytes_na_fila += tamanho;
                fila.push_back((Instant::now() + cano.atraso, bytes.to_vec()));
            }
            () = tokio::time::sleep_until(alvo.unwrap_or_else(Instant::now)), if alvo.is_some() => {}
        }
    }
}

fn para_onde(destino: &Destino) -> Option<SocketAddr> {
    match destino {
        Destino::Fixo(endereco) => Some(*endereco),
        Destino::Aprendido(mapa) => mapa.lock().ok().and_then(|guarda| *guarda),
    }
}
