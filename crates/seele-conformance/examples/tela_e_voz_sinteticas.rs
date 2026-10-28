//! **A outra ponta do ciclo do relato, para o Codex navegar por CUA** (25/10):
//! uma pessoa de outra máquina, num servidor local, que **transmite uma tela
//! sintética em H.264 de verdade** (um padrão colorido que anda a cada quadro e
//! um bloco que conta, codificado pelo codificador do produto, nenhuma captura)
//! e **fala** com o tom de mentira do `seele-audio` (nenhum microfone).
//!
//! O codificador é o do produto (`seele_video::codec`): no macOS, o do sistema
//! (`codec::macos::Codificador`, o VideoToolbox); com `SEELE_OPENH264=<caminho
//! da libopenh264>`, o de software (`codec::Codificador`, o OpenH264). Nenhum
//! codec novo.
//!
//! Uso, com um `seeled` local de pé (subido por um script local, não versionado):
//!
//! ```text
//! cargo run -p seele-conformance --example tela_e_voz_sinteticas -- 127.0.0.1:18401
//! ```
//!
//! Comandos pela entrada padrão, um por linha — **dois cenários distintos**:
//! - `pausar`: os quadros param e o fluxo fica aberto. O esperado na janela é
//!   a **última imagem congelada**, e não ela sumir; é o limite deste cenário;
//! - `encerrar`: a transmissão acaba (o fluxo da tela fecha e o `StopScreenShare`
//!   vai ao servidor). O esperado na janela é a **visualização sair**;
//! - `retomar`: depois de `pausar`, os quadros voltam (com um quadro-chave);
//! - `sair`: sai.
//!
//! A cada 5 s, uma linha com os quadros mandados e as contagens do aparelho de
//! mentira de quem fala (com `blocos_com_som`), e **uma segunda linha**
//! `telemetria` em chave=valor (`hora= rtt_ms= jitter_ms= loss_fraction=
//! bitrate_bps=`): o `snapshot().telemetry` da conexão `fala`, a que OUVE a voz
//! do app. `nd` é «não medido» (nunca zero).
//!
//! **Limites, explícitos:**
//! - a linha NÃO distingue PLC nem fila por fonte; sem conforto e sem
//!   atrasados também: a FFI não os expõe, e nenhuma API pública foi criada;
//! - o agregado é o «pior de todas as fontes»: só equivale à fonte quando há
//!   **uma** fonte (no soak, o app);
//! - não há contagem de datagramas por ssrc (fora do escopo desta rodada);
//! - esta rodada mede duração sem desaparecimento e retomada audível, não
//!   atraso físico nem DSP;
//! - a direção app-ouvindo (a voz do helper chegando ao app) não tem esta
//!   telemetria no log: só no retrato do app.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::indexing_slicing,
    reason = "um exemplo de bancada: o pânico e a linha são o relatório"
)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ed25519_dalek::SigningKey;
use seele_audio::device::mentira;
use seele_ffi::{ConnectConfig, Connection, VoiceMode};
use seele_proto::control::{ClientMessage, ServerMessage};
use seele_proto::screen::{ScreenCodec, ScreenHeader, ScreenSource, SCREEN_HEADER_LEN};
use seele_video::codec::{Cadencia, CodificaVideo, ConfigDoCodificador, QuadroI420, Resolucao};

#[path = "../tests/par_cru/mod.rs"]
mod par_cru;

/// O que a tela faz agora.
const TRANSMITE: u8 = 0;
const PAUSA: u8 = 1;
const ENCERRA: u8 = 2;

/// A resolução da tela sintética: a padrão do produto.
const RESOLUCAO: Resolucao = Resolucao::P720;

/// Um quadro-chave a cada tanto, para quem começa a assistir no meio começar
/// logo (além do primeiro e do da retomada).
const CHAVE_A_CADA: u64 = 30;

/// O padrão do quadro `n`: faixas coloridas que andam para a direita e um
/// bloco claro que desce, os dois a cada quadro — uma imagem que **muda**, e
/// em que se vê que mudou.
fn quadro(n: u64) -> QuadroI420 {
    let (largura, altura) = (RESOLUCAO.largura(), RESOLUCAO.altura());
    let passo = usize::try_from(n % 10_000).unwrap_or(0);
    let mut luma = vec![0_u8; largura * altura];
    for linha in 0..altura {
        for coluna in 0..largura {
            let faixa = ((coluna + passo * 4) / 80) % 8;
            luma[linha * largura + coluna] = 40 + u8::try_from(faixa * 25).unwrap_or(0);
        }
    }
    // O bloco que desce: 120×120, claro, na coluna do contador.
    let topo = (passo * 6) % (altura - 120);
    let esquerda = (passo * 2) % (largura - 120);
    for linha in topo..topo + 120 {
        for coluna in esquerda..esquerda + 120 {
            luma[linha * largura + coluna] = 235;
        }
    }
    let (cl, ca) = (largura.div_ceil(2), altura.div_ceil(2));
    let mut u = vec![128_u8; cl * ca];
    let mut v = vec![128_u8; cl * ca];
    for linha in 0..ca {
        for coluna in 0..cl {
            let faixa = ((coluna * 2 + passo * 4) / 80) % 8;
            u[linha * cl + coluna] = 64 + u8::try_from(faixa * 16).unwrap_or(0);
            v[linha * cl + coluna] = 192 - u8::try_from(faixa * 16).unwrap_or(0);
        }
    }
    QuadroI420::novo(largura, altura, luma, u, v).expect("um quadro do padrão")
}

/// O codificador do produto: o de software com `SEELE_OPENH264`, senão o do
/// sistema no macOS.
/// A hora do relógio do sistema, em UTC e na forma do `seele.log`
/// (`2026-10-25T06:39:24.145Z` sem a data: `06:39:24.145Z`), para cada linha
/// do relatório casar com o log do app. Sem ela, a conta do mudo era feita por
/// número de bloco e intervalo de poll, e não fechava (o CUA de 25/10).
fn hora() -> String {
    hora_de(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default(),
    )
}

fn hora_de(desde_a_epoca: Duration) -> String {
    let segundos = desde_a_epoca.as_secs() % 86_400;
    format!(
        "{:02}:{:02}:{:02}.{:03}Z",
        segundos / 3_600,
        segundos / 60 % 60,
        segundos % 60,
        desde_a_epoca.subsec_millis()
    )
}

fn codificador() -> anyhow::Result<Box<dyn CodificaVideo>> {
    let config = ConfigDoCodificador {
        medidas: None,
        resolucao: RESOLUCAO,
        cadencia: Cadencia::Q30,
        teto_bps: 1_500_000,
    };
    if let Some(caminho) = std::env::var_os("SEELE_OPENH264") {
        let biblioteca = seele_video::BibliotecaDeVideo::carregar(std::path::Path::new(&caminho))
            .map_err(|erro| anyhow::anyhow!("carregar o OpenH264: {erro}"))?;
        println!("codificador: OpenH264 (software)");
        return Ok(Box::new(
            seele_video::codec::Codificador::novo(&biblioteca, config)
                .map_err(|erro| anyhow::anyhow!("armar o OpenH264: {erro}"))?,
        ));
    }
    #[cfg(target_os = "macos")]
    {
        println!("codificador: o do sistema (VideoToolbox)");
        Ok(Box::new(
            seele_video::codec::macos::Codificador::novo(&config)
                .map_err(|erro| anyhow::anyhow!("armar o codificador do sistema: {erro}"))?,
        ))
    }
    #[cfg(not(target_os = "macos"))]
    {
        anyhow::bail!("fora do macOS, diga SEELE_OPENH264=<caminho da libopenh264>")
    }
}

/// A linha `telemetria`, em chave=valor, uma por tique de 5 s. `nd` é «não
/// medido». É o agregado do `snapshot`: não separa PLC nem fila por fonte.
fn linha_de_telemetria(hora: &str, t: &seele_ffi::Telemetry) -> String {
    fn medida(valor: Option<f32>) -> String {
        valor.map_or_else(|| "nd".into(), |v| format!("{v:.3}"))
    }
    format!(
        "{hora} telemetria fonte=fala rtt_ms={} jitter_ms={:.3} loss_fraction={} bitrate_bps={}",
        medida(t.rtt_ms),
        t.jitter_ms,
        medida(t.loss_fraction),
        t.bitrate_bps,
    )
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let alvo: SocketAddr = std::env::args()
        .nth(1)
        .expect("diga o servidor: 127.0.0.1:<porta>")
        .parse()?;
    let mut codificador = codificador()?;

    // Quem transmite a tela: um par cru sentado na sala padrão.
    let mut par = par_cru::ate_a_prova(alvo, &SigningKey::from_bytes(&[41; 32]), "tela").await;
    let ServerMessage::Session { voice_rooms, .. } = par.proximo().await? else {
        anyhow::bail!("o servidor não mandou Session");
    };
    let sala = voice_rooms.first().expect("uma sala").id;
    par.mandar(&ClientMessage::EnterVoiceRoom {
        voice_room: sala,
        password: None,
    })
    .await?;
    par.mandar(&ClientMessage::StartScreenShare).await?;
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
        width: u16::try_from(RESOLUCAO.largura()).unwrap(),
        height: u16::try_from(RESOLUCAO.altura()).unwrap(),
    }
    .encode(&mut abertura)?;
    fluxo.write_all(&abertura).await?;
    println!(
        "tela sintética {} transmitindo em {alvo} ({}x{})",
        screen.0,
        RESOLUCAO.largura(),
        RESOLUCAO.altura()
    );

    let estado = Arc::new(AtomicU8::new(TRANSMITE));
    let mandados = Arc::new(AtomicU64::new(0));
    let (quadros_tx, mut quadros_rx) = tokio::sync::mpsc::channel::<(bool, Vec<u8>)>(8);
    // O codificador numa linha própria, no ritmo de 30 por segundo.
    {
        let estado = Arc::clone(&estado);
        std::thread::spawn(move || {
            let mut n: u64 = 0;
            let mut chave_pendente = true;
            loop {
                match estado.load(Ordering::SeqCst) {
                    TRANSMITE => {
                        let pede = chave_pendente || n.is_multiple_of(CHAVE_A_CADA);
                        match codificador.codificar(&quadro(n), pede) {
                            Ok(Some(saiu)) => {
                                chave_pendente = false;
                                if quadros_tx.blocking_send((saiu.chave, saiu.bytes)).is_err() {
                                    return;
                                }
                            }
                            Ok(None) => {}
                            Err(erro) => println!("codificar o quadro {n}: {erro}"),
                        }
                        n += 1;
                    }
                    PAUSA => chave_pendente = true,
                    _ => return,
                }
                std::thread::sleep(Duration::from_millis(33));
            }
        });
    }
    // O fluxo: tipo, tamanho e bytes, o enquadramento de `seele_core::tela`.
    let parar_o_fluxo = Arc::new(tokio::sync::Notify::new());
    {
        let mandados = Arc::clone(&mandados);
        let parar_o_fluxo = Arc::clone(&parar_o_fluxo);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    quadro = quadros_rx.recv() => {
                        let Some((chave, bytes)) = quadro else { break };
                        let tipo = if chave {
                            seele_core::tela::TipoDeQuadro::Chave
                        } else {
                            seele_core::tela::TipoDeQuadro::Comum
                        };
                        let mut fora = vec![tipo.byte()];
                        fora.extend_from_slice(&u32::try_from(bytes.len()).unwrap().to_be_bytes());
                        fora.extend_from_slice(&bytes);
                        if fluxo.write_all(&fora).await.is_err() {
                            println!("a escrita da tela falhou: o fluxo acabou");
                            return;
                        }
                        mandados.fetch_add(1, Ordering::SeqCst);
                    }
                    () = parar_o_fluxo.notified() => break,
                }
            }
            let _ = fluxo.finish();
            println!("tela: fluxo fechado");
        });
    }

    // Quem fala: outra pessoa pela FFI, com o tom de mentira, sentada.
    let casa = tempfile::tempdir()?;
    let config = ConnectConfig {
        server: alvo.to_string(),
        alternate_servers: Vec::new(),
        nickname: "fala".into(),
        home: casa.path().display().to_string(),
        join_secret: None,
        expected_fingerprint: None,
        codigo_esperado: None,
        bilhete: None,
        audio: true,
        capture_device: Some(mentira::id("cua-fala")),
        playback_device: Some(mentira::id("cua-fala")),
    };
    let foco = seele_ffi::foco::FocoDeVoz::novo();
    let (fala, _) = tokio::task::spawn_blocking(move || {
        Connection::connect_com_foco(config, None, None, Some(foco), None)
    })
    .await?
    .map_err(|falha| anyhow::anyhow!("quem fala não entrou: {falha:?}"))?;
    fala.set_voice_mode(VoiceMode::Open);
    let sala = fala.snapshot().voice_rooms[0].id;
    fala.enter_voice_room(sala, None)
        .map_err(|erro| anyhow::anyhow!("quem fala não sentou: {erro:?}"))?;
    println!("quem fala está sentado (o tom de mentira)");

    {
        let mandados = Arc::clone(&mandados);
        let fala = fala.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                let c = mentira::contagem("cua-fala");
                println!(
                    "{} tela: {} quadros mandados; quem fala: aparelhos abertos {}, blocos {}, \
                     blocos tocados com som {}",
                    hora(),
                    mandados.load(Ordering::SeqCst),
                    c.abertos,
                    c.blocos,
                    c.blocos_com_som
                );
                println!(
                    "{}",
                    linha_de_telemetria(&hora(), &fala.snapshot().telemetry)
                );
            }
        });
    }

    // Os comandos.
    let (envia, mut recebe) = tokio::sync::mpsc::unbounded_channel::<String>();
    std::thread::spawn(move || {
        for linha in std::io::stdin().lines().map_while(Result::ok) {
            if envia.send(linha).is_err() {
                return;
            }
        }
    });
    while let Some(linha) = recebe.recv().await {
        match linha.trim() {
            "pausar" => {
                estado.store(PAUSA, Ordering::SeqCst);
                println!("{} tela: PAUSADA — os quadros pararam e o fluxo continua aberto (espera-se a imagem congelada)", hora());
            }
            "retomar" => {
                estado.store(TRANSMITE, Ordering::SeqCst);
                println!("tela: retomada, com um quadro-chave");
            }
            "encerrar" => {
                estado.store(ENCERRA, Ordering::SeqCst);
                parar_o_fluxo.notify_one();
                let _ = par.mandar(&ClientMessage::StopScreenShare).await;
                println!("{} tela: ENCERRADA — o fluxo fecha e o StopScreenShare foi (espera-se a visualização sair)", hora());
            }
            "sair" => break,
            outro => println!("comando desconhecido: {outro} (pausar | retomar | encerrar | sair)"),
        }
    }
    fala.disconnect();
    drop(par);
    Ok(())
}

#[cfg(test)]
mod testes {
    /// A linha nova traz todos os campos, em chave=valor; o não medido é `nd`, e nunca zero.
    #[test]
    fn a_linha_de_telemetria_traz_os_campos_por_fonte() {
        let t = seele_ffi::Telemetry {
            rtt_ms: Some(12.5),
            jitter_ms: 3.25,
            loss_fraction: Some(0.02),
            bitrate_bps: 32_000,
            ..seele_ffi::Telemetry::default()
        };
        let linha = super::linha_de_telemetria("06:39:24.145Z", &t);
        for campo in [
            "06:39:24.145Z telemetria",
            "fonte=fala",
            "rtt_ms=12.500",
            "jitter_ms=3.250",
            "loss_fraction=0.020",
            "bitrate_bps=32000",
        ] {
            assert!(linha.contains(campo), "falta `{campo}` em: {linha}");
        }
        let vazia = super::linha_de_telemetria("h", &seele_ffi::Telemetry::default());
        assert!(
            vazia.contains("rtt_ms=nd") && vazia.contains("loss_fraction=nd"),
            "{vazia}"
        );
    }

    /// O padrão muda a cada quadro: dois quadros seguidos não são iguais.
    #[test]
    fn o_padrao_muda_a_cada_quadro() {
        assert_ne!(super::quadro(0).luma(), super::quadro(1).luma());
        assert_ne!(super::quadro(1).luma(), super::quadro(2).luma());
    }

    /// A hora do relatório é a mesma do `seele.log`: 06:39:24.145 UTC de
    /// 25/10/2026, a saída de A no CUA.
    #[test]
    fn a_hora_do_relatorio_e_a_do_log_do_app() {
        let saida = std::time::Duration::from_millis(1_792_910_364_145);
        assert_eq!(super::hora_de(saida), "06:39:24.145Z");
    }

    /// Os tipos de NAL de um quadro Annex-B.
    fn nals(bytes: &[u8]) -> Vec<u8> {
        let mut tipos = Vec::new();
        let mut i = 0;
        while i + 3 < bytes.len() {
            let (tres, quatro) = (
                &bytes[i..i + 3] == [0, 0, 1],
                i + 4 <= bytes.len() && bytes[i..i + 4] == [0, 0, 0, 1],
            );
            if quatro || tres {
                let inicio = if quatro { i + 4 } else { i + 3 };
                if let Some(cabeca) = bytes.get(inicio) {
                    tipos.push(cabeca & 0x1f);
                }
                i = inicio;
            } else {
                i += 1;
            }
        }
        tipos
    }

    /// **O codificador do produto sai com H.264 de verdade**: o primeiro quadro
    /// é chave, em Annex-B, com SPS (7), PPS (8) e IDR (5); os seguintes, sem
    /// pedido de chave, vêm comuns (fatia 1). É a mesma saída que a transmissão
    /// do produto manda.
    #[test]
    fn o_codificador_do_produto_sai_com_h264_annex_b() {
        let mut codificador = super::codificador().expect("o codificador do produto");
        let mut saidas = Vec::new();
        for n in 0..10 {
            if let Some(saiu) = codificador
                .codificar(&super::quadro(n), n == 0)
                .expect("codificar")
            {
                saidas.push(saiu);
            }
        }
        let primeiro = saidas
            .first()
            .expect("o codificador não devolveu quadro nenhum");
        assert!(primeiro.chave, "o primeiro quadro não é chave");
        let tipos = nals(&primeiro.bytes);
        assert!(
            tipos.contains(&7) && tipos.contains(&8) && tipos.contains(&5),
            "o quadro-chave não traz SPS, PPS e IDR: {tipos:?}"
        );
        assert!(
            saidas
                .iter()
                .skip(1)
                .any(|saiu| !saiu.chave && nals(&saiu.bytes).contains(&1)),
            "nenhum quadro comum (fatia não-IDR) saiu depois do primeiro"
        );
    }
}
