//! Spike `voz-no-navegador`: um navegador de celular fala voz com um servidor
//! por WebTransport? DESCARTÁVEL — ver o `README.md`.
//!
//! Um processo só, três portas:
//!
//! - **UDP `--porta`** (4433): o WebTransport. Uma sala, e cada sessão que
//!   entra nela recebe a voz das outras.
//! - **TCP `--porta`** (4433): a página, por HTTPS, com o **mesmo**
//!   certificado. Microfone e WebTransport só existem em contexto seguro, e um
//!   celular na LAN só chega a um contexto seguro por HTTPS — aceitando, uma
//!   vez, o aviso de certificado que o navegador mostra.
//! - **TCP `--http`** (8080), só em `127.0.0.1`: a mesma página sem TLS, porque
//!   `localhost` já é contexto seguro. É o que a prova automática usa.
//!
//! O certificado vale **13 dias**. O `serverCertificateHashes` do WebTransport
//! recusa qualquer um que valha mais de 14, e essa é a primeira coisa que
//! difere do `seeled`, cujo certificado não vence.
//!
//! O formato dos datagramas, que `pagina/pagina.js` espelha:
//!
//! | tipo | do navegador                      | do servidor                           |
//! |------|-----------------------------------|---------------------------------------|
//! | 0x01 | `seq u16 · t f64 · opus`          | `0x01 · de u16 · seq u16 · t f64 · opus` para os outros |
//! | 0x02 | `t f64` (ping)                    | o mesmo datagrama, de volta           |
//! | 0x03 | igual ao 0x01, pedindo eco        | o datagrama de volta, e 0x01 para os outros |

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use wtransport::endpoint::IncomingSession;
use wtransport::tls::Sha256DigestFmt;
use wtransport::{Connection, Endpoint, Identity, ServerConfig};

const PAGINA: &str = include_str!("../pagina/index.html");
const SCRIPT: &str = include_str!("../pagina/pagina.js");
const PROCESSADORES: &str = include_str!("../pagina/processadores.js");

/// Quem está na sala, pelo número que o servidor deu a cada sessão.
type Sala = Arc<Mutex<HashMap<u16, Connection>>>;

/// Onde os relatórios dos navegadores ficam. Um por linha, com a hora e o
/// endereço de quem mandou — é o dado que volta do celular, e sem ele a prova
/// num aparelho que não está na nossa mão vira uma conversa sem números.
struct Registro {
    caminho: std::path::PathBuf,
    trava: Mutex<()>,
}

impl Registro {
    fn gravar(&self, de: SocketAddr, via: &str, corpo: &str) {
        let relatorio: serde_json::Value = match serde_json::from_str(corpo) {
            Ok(valor) => valor,
            Err(erro) => {
                eprintln!("[relatório] {de} via {via}: JSON inválido ({erro})");
                return;
            }
        };
        let resumo = relatorio
            .get("resumo")
            .and_then(|r| r.as_str())
            .unwrap_or("(sem resumo)");
        println!("[relatório] {de} via {via}: {resumo}");
        let linha = serde_json::json!({
            "recebido_em_ms": agora_ms(),
            "de": de.to_string(),
            "via": via,
            "relatorio": relatorio,
        });
        let _guarda = self.trava.lock().unwrap_or_else(|e| e.into_inner());
        let gravou = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.caminho)
            .and_then(|mut arquivo| writeln!(arquivo, "{linha}"));
        if let Err(erro) = gravou {
            eprintln!("[relatório] não gravei em {}: {erro}", self.caminho.display());
        }
    }
}

fn agora_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// O endereço desta máquina na LAN. É o mesmo truque do ADR 0022: um socket
/// UDP com `connect` num endereço de documentação não manda nada, mas faz o
/// sistema escolher a interface de saída.
fn endereco_na_lan() -> Option<IpAddr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_unspecified()).then_some(ip)
}

struct Opcoes {
    porta: u16,
    http: u16,
    relatorios: std::path::PathBuf,
}

fn ler_opcoes() -> Result<Opcoes> {
    let mut opcoes = Opcoes {
        porta: 4433,
        http: 8080,
        relatorios: "relatorios.jsonl".into(),
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut valor = || args.next().with_context(|| format!("{arg} pede um valor"));
        match arg.as_str() {
            "--porta" => opcoes.porta = valor()?.parse()?,
            "--http" => opcoes.http = valor()?.parse()?,
            "--relatorios" => opcoes.relatorios = valor()?.into(),
            outro => bail!("não conheço {outro}; aceito --porta, --http e --relatorios"),
        }
    }
    Ok(opcoes)
}

#[tokio::main]
async fn main() -> Result<()> {
    let opcoes = ler_opcoes()?;
    let lan = endereco_na_lan();

    let mut nomes = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    if let Some(ip) = lan {
        nomes.push(ip.to_string());
    }
    let identidade = Identity::self_signed_builder()
        .subject_alt_names(&nomes)
        .from_now_utc()
        .validity_days(13)
        .build()
        .context("gerar o certificado")?;
    let certificado = &identidade.certificate_chain().as_slice()[0];
    let hash = certificado.hash();

    // A página recebe o hash como array de bytes, que é o que o
    // `serverCertificateHashes` pede, e a porta do WebTransport.
    let pagina = PAGINA.replace(
        "/*SPIKE*/",
        &format!(
            "window.SPIKE = {{ hash: {}, porta: {}, validadeDias: 13 }};",
            hash.fmt(Sha256DigestFmt::BytesArray),
            opcoes.porta
        ),
    );
    let pagina: &'static str = Box::leak(pagina.into_boxed_str());

    let registro = Arc::new(Registro {
        caminho: opcoes.relatorios.clone(),
        trava: Mutex::new(()),
    });

    // HTTPS com o mesmo certificado do WebTransport.
    let tls = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_no_client_auth()
    .with_single_cert(
        vec![certificado.der().to_vec().into()],
        rustls::pki_types::PrivateKeyDer::Pkcs8(
            identidade.private_key().secret_der().to_vec().into(),
        ),
    )
    .context("montar o TLS da página")?;
    let aceitador = tokio_rustls::TlsAcceptor::from(Arc::new(tls));

    let https = TcpListener::bind((Ipv4Addr::UNSPECIFIED, opcoes.porta))
        .await
        .with_context(|| format!("abrir TCP {} para a página", opcoes.porta))?;
    let http = TcpListener::bind((Ipv4Addr::LOCALHOST, opcoes.http))
        .await
        .with_context(|| format!("abrir TCP 127.0.0.1:{}", opcoes.http))?;

    let config = ServerConfig::builder()
        .with_bind_default(opcoes.porta)
        .with_identity(identidade.clone_identity())
        .keep_alive_interval(Some(Duration::from_secs(3)))
        .max_idle_timeout(Some(Duration::from_secs(30)))?
        .build();
    let servidor = Endpoint::server(config).context("abrir o WebTransport")?;

    println!("voz-no-navegador — spike descartável");
    println!("  certificado: P-256, 13 dias, sha-256 {}", hash.fmt(Sha256DigestFmt::DottedHex));
    if let Some(ip) = lan {
        println!("  no celular:  https://{ip}:{}/   (aceite o aviso de certificado uma vez)", opcoes.porta);
    }
    println!("  nesta máquina: http://localhost:{}/", opcoes.http);
    println!("  relatórios em {}", opcoes.relatorios.display());

    {
        let registro = registro.clone();
        tokio::spawn(async move {
            loop {
                let Ok((tcp, de)) = https.accept().await else { continue };
                let aceitador = aceitador.clone();
                let registro = registro.clone();
                tokio::spawn(async move {
                    // Um celular que recusa o certificado fecha o TLS aqui. É
                    // esperado na primeira visita, e o erro diz qual foi.
                    match aceitador.accept(tcp).await {
                        Ok(fluxo) => atender_http(fluxo, de, pagina, &registro).await,
                        Err(erro) => eprintln!("[https] {de}: TLS recusado ({erro})"),
                    }
                });
            }
        });
    }
    {
        let registro = registro.clone();
        tokio::spawn(async move {
            loop {
                let Ok((tcp, de)) = http.accept().await else { continue };
                let registro = registro.clone();
                tokio::spawn(async move { atender_http(tcp, de, pagina, &registro).await });
            }
        });
    }

    let sala: Sala = Arc::new(Mutex::new(HashMap::new()));
    let proximo = Arc::new(AtomicU16::new(1));
    loop {
        let chegando = servidor.accept().await;
        let sala = sala.clone();
        let registro = registro.clone();
        let id = proximo.fetch_add(1, Ordering::Relaxed);
        tokio::spawn(async move {
            if let Err(erro) = atender_sessao(chegando, id, sala, registro).await {
                eprintln!("[wt #{id}] {erro:#}");
            }
        });
    }
}

/// Contadores de uma sessão, impressos a cada cinco segundos. É por eles que
/// o servidor vê, do lado dele, o que a tela apagada fez: um celular que para
/// de mandar voz aparece aqui mesmo que o relatório dele nunca chegue.
#[derive(Default)]
struct Contas {
    entrada: AtomicU64,
    voz_entrada: AtomicU64,
    pings: AtomicU64,
    repassados: AtomicU64,
    repasse_falhou: AtomicU64,
    malformados: AtomicU64,
}

async fn atender_sessao(chegando: IncomingSession, id: u16, sala: Sala, registro: Arc<Registro>) -> Result<()> {
    let pedido = chegando.await.context("handshake")?;
    let de = pedido.remote_address();
    println!(
        "[wt #{id}] pedido de {de}: caminho {} · origem {}",
        pedido.path(),
        pedido.origin().unwrap_or("(nenhuma)")
    );
    let conexao = pedido.accept().await.context("aceitar a sessão")?;
    println!(
        "[wt #{id}] na sala · datagrama máximo {:?} bytes",
        conexao.max_datagram_size()
    );
    sala.lock().unwrap_or_else(|e| e.into_inner()).insert(id, conexao.clone());

    let contas = Arc::new(Contas::default());

    // Os relatórios chegam por um fluxo unidirecional, um JSON por linha.
    let leitor = {
        let conexao = conexao.clone();
        let registro = registro.clone();
        tokio::spawn(async move {
            while let Ok(mut fluxo) = conexao.accept_uni().await {
                let registro = registro.clone();
                tokio::spawn(async move {
                    let mut pendente = Vec::new();
                    let mut pedaco = vec![0u8; 16 * 1024];
                    while let Ok(Some(n)) = fluxo.read(&mut pedaco).await {
                        pendente.extend_from_slice(&pedaco[..n]);
                        while let Some(fim) = pendente.iter().position(|&b| b == b'\n') {
                            let linha: Vec<u8> = pendente.drain(..=fim).collect();
                            if let Ok(texto) = std::str::from_utf8(&linha[..linha.len() - 1]) {
                                registro.gravar(de, "webtransport", texto);
                            }
                        }
                        if pendente.len() > 1 << 20 {
                            eprintln!("[wt #{id}] relatório sem fim de linha passou de 1 MiB; descartado");
                            pendente.clear();
                        }
                    }
                });
            }
        })
    };

    let relogio = {
        let conexao = conexao.clone();
        let contas = contas.clone();
        tokio::spawn(async move {
            let mut antes = [0u64; 3];
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                let agora = [
                    contas.entrada.load(Ordering::Relaxed),
                    contas.voz_entrada.load(Ordering::Relaxed),
                    contas.pings.load(Ordering::Relaxed),
                ];
                println!(
                    "[wt #{id}] 5 s: {} datagramas ({} voz, {} ping) · repassados {} · falhas de repasse {} · malformados {} · rtt quic {} ms",
                    agora[0] - antes[0],
                    agora[1] - antes[1],
                    agora[2] - antes[2],
                    contas.repassados.load(Ordering::Relaxed),
                    contas.repasse_falhou.load(Ordering::Relaxed),
                    contas.malformados.load(Ordering::Relaxed),
                    conexao.rtt().as_millis()
                );
                antes = agora;
            }
        })
    };

    let motivo = loop {
        let datagrama = match conexao.receive_datagram().await {
            Ok(d) => d,
            Err(erro) => break erro,
        };
        let carga = datagrama.payload();
        contas.entrada.fetch_add(1, Ordering::Relaxed);
        match carga.first() {
            Some(0x02) if carga.len() == 9 => {
                contas.pings.fetch_add(1, Ordering::Relaxed);
                let _ = conexao.send_datagram(carga);
            }
            Some(tipo @ (0x01 | 0x03)) if carga.len() > 11 => {
                contas.voz_entrada.fetch_add(1, Ordering::Relaxed);
                if *tipo == 0x03 {
                    let _ = conexao.send_datagram(carga.clone());
                }
                let mut repasse = Vec::with_capacity(carga.len() + 2);
                repasse.push(0x01);
                repasse.extend_from_slice(&id.to_be_bytes());
                repasse.extend_from_slice(&carga[1..]);
                let outros: Vec<Connection> = sala
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .iter()
                    .filter(|(outro, _)| **outro != id)
                    .map(|(_, c)| c.clone())
                    .collect();
                for outro in outros {
                    match outro.send_datagram(repasse.clone()) {
                        Ok(()) => contas.repassados.fetch_add(1, Ordering::Relaxed),
                        Err(_) => contas.repasse_falhou.fetch_add(1, Ordering::Relaxed),
                    };
                }
            }
            _ => {
                contas.malformados.fetch_add(1, Ordering::Relaxed);
            }
        }
    };

    sala.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
    leitor.abort();
    relogio.abort();
    println!(
        "[wt #{id}] saiu: {motivo} · {} datagramas ao todo ({} voz, {} ping)",
        contas.entrada.load(Ordering::Relaxed),
        contas.voz_entrada.load(Ordering::Relaxed),
        contas.pings.load(Ordering::Relaxed)
    );
    Ok(())
}

/// HTTP/1.1 mínimo: três arquivos e o `POST /relatorio`. Uma requisição por
/// conexão. Não é servidor web, é o bastante para um celular carregar a página
/// e devolver o que viu quando o WebTransport nem chega a abrir.
async fn atender_http<S>(mut fluxo: S, de: SocketAddr, pagina: &str, registro: &Registro)
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut lido = Vec::new();
    let mut pedaco = [0u8; 4096];
    let fim_do_cabecalho = loop {
        match fluxo.read(&mut pedaco).await {
            Ok(0) | Err(_) => return,
            Ok(n) => lido.extend_from_slice(&pedaco[..n]),
        }
        if let Some(i) = lido.windows(4).position(|j| j == b"\r\n\r\n") {
            break i + 4;
        }
        if lido.len() > 16 * 1024 {
            return;
        }
    };
    let cabecalho = String::from_utf8_lossy(&lido[..fim_do_cabecalho]).to_string();
    let mut linhas = cabecalho.lines();
    let primeira = linhas.next().unwrap_or("");
    let mut partes = primeira.split_whitespace();
    let metodo = partes.next().unwrap_or("");
    let caminho = partes.next().unwrap_or("").split('?').next().unwrap_or("");
    let tamanho: usize = linhas
        .filter_map(|l| l.split_once(':'))
        .find(|(nome, _)| nome.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.trim().parse().ok())
        .unwrap_or(0);

    let (estado, tipo, corpo): (&str, &str, &[u8]) = match (metodo, caminho) {
        ("GET", "/") => ("200 OK", "text/html; charset=utf-8", pagina.as_bytes()),
        ("GET", "/pagina.js") => ("200 OK", "text/javascript; charset=utf-8", SCRIPT.as_bytes()),
        ("GET", "/processadores.js") => ("200 OK", "text/javascript; charset=utf-8", PROCESSADORES.as_bytes()),
        ("POST", "/relatorio") if tamanho <= 256 * 1024 => {
            let mut corpo = lido[fim_do_cabecalho..].to_vec();
            while corpo.len() < tamanho {
                match fluxo.read(&mut pedaco).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => corpo.extend_from_slice(&pedaco[..n]),
                }
            }
            registro.gravar(de, "https", &String::from_utf8_lossy(&corpo));
            ("204 No Content", "text/plain", b"")
        }
        _ => ("404 Not Found", "text/plain; charset=utf-8", b"nada aqui"),
    };
    let resposta = format!(
        "HTTP/1.1 {estado}\r\nContent-Type: {tipo}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        corpo.len()
    );
    let _ = fluxo.write_all(resposta.as_bytes()).await;
    let _ = fluxo.write_all(corpo).await;
    let _ = fluxo.shutdown().await;
}
