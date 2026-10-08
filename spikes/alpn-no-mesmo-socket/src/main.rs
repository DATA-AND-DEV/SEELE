//! Medida descartável: um quinn::Endpoint, dois ALPNs (seele/1 e h3), certificado
//! escolhido por ALPN no ClientHello, e o wtransport adotando a conexão h3.
//!
//! Desde 08/10, preparado para a medição 3 do ADR 0057 (o iPhone):
//! - **uma chave só** assina o longo e o curto, como o `seeled` faria;
//! - o endpoint A2 serve um curto de **chave própria**, como controle: se o
//!   iPhone recusar o A e aceitar o A2, a culpa é da chave repetida;
//! - o endpoint MUDO lê e joga fora (um servidor travado, um firewall que
//!   descarta), e a porta FECHADA não tem ninguém: os dois «fora do ar» que a
//!   página compara com a recusa;
//! - cada conexão sai no stderr como uma linha JSON, com a hora em que abriu, a
//!   hora em que fechou e o motivo. É assim que se vê o ocioso de 20 s.
//!
//! Uso: `alpn-demux [--lan] [--porta-base N]`. Sem `--lan`, escuta em
//! 127.0.0.1 em portas livres. Com `--lan`, escuta em 0.0.0.0, para o iPhone.

use std::any::Any;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use rcgen::PublicKeyData;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;
use sha2::{Digest, Sha256};

static INICIO: OnceLock<Instant> = OnceLock::new();

fn agora_ms() -> u128 {
    INICIO.get_or_init(Instant::now).elapsed().as_millis()
}

/// Texto para dentro de uma string JSON.
fn j(texto: &str) -> String {
    let mut s = String::with_capacity(texto.len() + 2);
    s.push('"');
    for c in texto.chars() {
        match c {
            '"' => s.push_str("\\\""),
            '\\' => s.push_str("\\\\"),
            c if (c as u32) < 0x20 => s.push_str(&format!("\\u{:04x}", c as u32)),
            c => s.push(c),
        }
    }
    s.push('"');
    s
}

/// Uma linha de registro: `{"t_ms": …, <campos>}` no stderr.
fn registro(campos: &str) {
    eprintln!("{{\"t_ms\":{},{campos}}}", agora_ms());
}

#[derive(Debug)]
struct PorAlpn {
    longo: Arc<CertifiedKey>,
    curto: Arc<CertifiedKey>,
}

impl ResolvesServerCert for PorAlpn {
    fn resolve(&self, hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        let h3 = hello.alpn().map_or(false, |mut it| it.any(|p| p == b"h3"));
        Some(if h3 { self.curto.clone() } else { self.longo.clone() })
    }
}

#[derive(Debug)]
struct SoLongo(Arc<CertifiedKey>);
impl ResolvesServerCert for SoLongo {
    fn resolve(&self, _: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(self.0.clone())
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// O longo, como o `seeled` o gera (`tls.rs:38-47`): a validade padrão do
/// `rcgen`, de 1975 a 4096. O curto, com no máximo 13 dias, como no ADR.
fn certificado(chave: &rcgen::KeyPair, curto: bool) -> Result<CertificateDer<'static>> {
    // O curto não nomeia endereço nenhum: mede se o navegador confere nome com hash.
    let nomes: Vec<String> = if curto { vec!["seele.invalid".into()] } else { vec!["localhost".into(), "127.0.0.1".into()] };
    let mut params = rcgen::CertificateParams::new(nomes)?;
    if curto {
        let agora = time::OffsetDateTime::now_utc();
        params.not_before = agora - time::Duration::days(1);
        params.not_after = agora + time::Duration::days(12);
    }
    Ok(params.self_signed(chave)?.der().clone())
}

fn chave_der(chave: &rcgen::KeyPair) -> PrivateKeyDer<'static> {
    PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(chave.serialize_der()))
}

fn config(resolvedor: Arc<dyn ResolvesServerCert>) -> Result<quinn::ServerConfig> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut tls = rustls::ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_no_client_auth()
        .with_cert_resolver(resolvedor);
    tls.alpn_protocols = vec![b"seele/1".to_vec(), b"h3".to_vec()];
    let quic = quinn::crypto::rustls::QuicServerConfig::try_from(tls)?;
    let mut cfg = quinn::ServerConfig::with_crypto(Arc::new(quic));
    // Os números do seeled: 20 s de ocioso e 5 s de keepalive
    // (seele-proto/src/transport.rs:33 e :39). A conexão que o wtransport adota
    // é esta, então herda os dois.
    let mut t = quinn::TransportConfig::default();
    t.max_idle_timeout(Some(Duration::from_secs(20).try_into()?));
    t.keep_alive_interval(Some(Duration::from_secs(5)));
    t.datagram_receive_buffer_size(Some(1024 * 1024));
    cfg.transport_config(Arc::new(t));
    Ok(cfg)
}

async fn atender(endpoint: quinn::Endpoint, rotulo: &'static str) {
    while let Some(chegando) = endpoint.accept().await {
        let de = chegando.remote_address();
        tokio::spawn(async move {
            let inicio = agora_ms();
            let r: Result<()> = async {
                let mut conectando = chegando.accept()?;
                let dados: Box<dyn Any> = conectando.handshake_data().await?;
                let alpn = dados
                    .downcast::<quinn::crypto::rustls::HandshakeData>()
                    .map_err(|_| anyhow!("sem HandshakeData"))?
                    .protocol;
                let alpn_texto = alpn.as_deref().map(String::from_utf8_lossy).unwrap_or_default().into_owned();
                registro(&format!("\"ev\":\"chegou\",\"ponto\":{},\"de\":{},\"alpn\":{}", j(rotulo), j(&de.to_string()), j(&alpn_texto)));
                match alpn.as_deref() {
                    Some(b"seele/1") => {
                        let conexao = conectando.await?;
                        let (mut envio, mut recebe) = conexao.accept_bi().await?;
                        let mut tam = [0u8; 4];
                        recebe.read_exact(&mut tam).await?;
                        let mut q = vec![0u8; u32::from_be_bytes(tam) as usize];
                        recebe.read_exact(&mut q).await?;
                        envio.write_all(&tam).await?;
                        envio.write_all(&q).await?;
                        envio.finish()?;
                        let _ = envio.stopped().await;
                    }
                    Some(b"h3") => {
                        let pedido = wtransport::endpoint::IncomingSessionFuture::with_quic_connecting(conectando).await?;
                        registro(&format!(
                            "\"ev\":\"pedido\",\"ponto\":{},\"de\":{},\"caminho\":{},\"origem\":{}",
                            j(rotulo),
                            j(&de.to_string()),
                            j(pedido.path()),
                            j(pedido.origin().unwrap_or("(nenhuma)"))
                        ));
                        let conexao = pedido.accept().await?;
                        registro(&format!(
                            "\"ev\":\"aberta\",\"ponto\":{},\"de\":{},\"max_datagram\":{}",
                            j(rotulo),
                            j(&de.to_string()),
                            conexao.max_datagram_size().unwrap_or(0)
                        ));
                        // Eco de datagrama: é o «ping» da página.
                        let c2 = conexao.clone();
                        tokio::spawn(async move {
                            while let Ok(d) = c2.receive_datagram().await {
                                let _ = c2.send_datagram(d.payload());
                            }
                        });
                        // Eco de quadro (4 bytes BE + corpo), em quantos fluxos a página abrir.
                        let c3 = conexao.clone();
                        tokio::spawn(async move {
                            while let Ok((mut envio, mut recebe)) = c3.accept_bi().await {
                                tokio::spawn(async move {
                                    let r = recebe.quic_stream_mut();
                                    let mut tam = [0u8; 4];
                                    if r.read_exact(&mut tam).await.is_err() {
                                        return;
                                    }
                                    let mut q = vec![0u8; u32::from_be_bytes(tam) as usize];
                                    if r.read_exact(&mut q).await.is_err() {
                                        return;
                                    }
                                    let s = envio.quic_stream_mut();
                                    let _ = s.write_all(&tam).await;
                                    let _ = s.write_all(&q).await;
                                    let _ = s.finish();
                                    // Até o FIN da página: soltar o fluxo antes
                                    // manda STOP_SENDING, e o `close()` dela
                                    // falharia por culpa deste eco.
                                    while let Ok(Some(_)) = r.read_chunk(64, true).await {}
                                });
                            }
                        });
                        let motivo = conexao.closed().await;
                        registro(&format!(
                            "\"ev\":\"fechou\",\"ponto\":{},\"de\":{},\"durou_ms\":{},\"motivo\":{}",
                            j(rotulo),
                            j(&de.to_string()),
                            agora_ms() - inicio,
                            j(&motivo.to_string())
                        ));
                    }
                    outro => return Err(anyhow!("ALPN inesperado {outro:?}")),
                }
                Ok(())
            }
            .await;
            if let Err(e) = r {
                // A recusa do navegador chega aqui, como alerta TLS.
                registro(&format!(
                    "\"ev\":\"falhou\",\"ponto\":{},\"de\":{},\"depois_de_ms\":{},\"erro\":{}",
                    j(rotulo),
                    j(&de.to_string()),
                    agora_ms() - inicio,
                    j(&format!("{e:#}"))
                ));
            }
        });
    }
}

/// O MUDO: lê tudo e não responde nada. Registra o primeiro pacote de cada
/// origem e, a cada 5 s, quantos chegaram.
async fn mudo(socket: tokio::net::UdpSocket) {
    let mut buf = vec![0u8; 65536];
    let mut vistos = std::collections::BTreeMap::<SocketAddr, u64>::new();
    let mut ultima_conta = Instant::now();
    loop {
        let Ok((_, de)) = socket.recv_from(&mut buf).await else { return };
        let n = vistos.entry(de).or_insert(0);
        *n += 1;
        if *n == 1 {
            registro(&format!("\"ev\":\"mudo_primeiro_pacote\",\"de\":{}", j(&de.to_string())));
        }
        if ultima_conta.elapsed() > Duration::from_secs(5) {
            ultima_conta = Instant::now();
            for (de, n) in &vistos {
                registro(&format!("\"ev\":\"mudo_conta\",\"de\":{},\"pacotes\":{n}", j(&de.to_string())));
            }
        }
    }
}

#[derive(Debug)]
struct Anota(Arc<std::sync::Mutex<Option<Vec<u8>>>>, Arc<rustls::crypto::CryptoProvider>);
impl rustls::client::danger::ServerCertVerifier for Anota {
    fn verify_server_cert(
        &self,
        ee: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        *self.0.lock().unwrap() = Some(ee.as_ref().to_vec());
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        d: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(m, c, d, &self.1.signature_verification_algorithms)
    }
    fn verify_tls13_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        d: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(m, c, d, &self.1.signature_verification_algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.1.signature_verification_algorithms.supported_schemes()
    }
}

/// Um cliente como o desktop: ALPN seele/1, anota o certificado que recebeu.
async fn cliente_seele(destino: SocketAddr) -> Result<Vec<u8>> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let visto = Arc::new(std::sync::Mutex::new(None));
    let mut tls = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Anota(visto.clone(), provider)))
        .with_no_client_auth();
    tls.alpn_protocols = vec![b"seele/1".to_vec()];
    let mut ponta = quinn::Endpoint::client("127.0.0.1:0".parse()?)?;
    ponta.set_default_client_config(quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls)?,
    )));
    let conexao = ponta.connect(destino, "localhost")?.await?;
    let (mut envio, mut recebe) = conexao.open_bi().await?;
    let corpo = b"\x08ola";
    envio.write_all(&(corpo.len() as u32).to_be_bytes()).await?;
    envio.write_all(corpo).await?;
    envio.finish()?;
    let volta = recebe.read_to_end(64).await?;
    anyhow::ensure!(volta.ends_with(corpo), "eco seele/1 errado: {volta:?}");
    let v = visto.lock().unwrap().clone().context("nenhum certificado visto")?;
    Ok(v)
}

fn contem(palheiro: &[u8], agulha: &[u8]) -> bool {
    palheiro.windows(agulha.len()).any(|w| w == agulha)
}

#[tokio::main]
async fn main() -> Result<()> {
    INICIO.get_or_init(Instant::now);
    let args: Vec<String> = std::env::args().collect();
    let lan = args.iter().any(|a| a == "--lan");
    let base: u16 = args
        .iter()
        .position(|a| a == "--porta-base")
        .and_then(|i| args.get(i + 1))
        .map(|p| p.parse())
        .transpose()?
        .unwrap_or(0);
    let ip: IpAddr = if lan { Ipv4Addr::UNSPECIFIED.into() } else { Ipv4Addr::LOCALHOST.into() };
    let porta = |n: u16| if base == 0 { 0 } else { base + n };

    let provider = rustls::crypto::ring::default_provider();

    // Uma chave só, P-256 (o padrão do rcgen, que é o que o
    // generate_simple_self_signed do seeled usa), para o longo e o curto.
    let chave = rcgen::KeyPair::generate()?;
    let spki = chave.subject_public_key_info();
    let longo_der = certificado(&chave, false)?;
    let curto_der = certificado(&chave, true)?;
    let mesma_chave = contem(&longo_der, &spki) && contem(&curto_der, &spki);
    anyhow::ensure!(mesma_chave, "o SPKI da chave não está nos dois certificados");

    // O controle: um curto com chave própria.
    let outra = rcgen::KeyPair::generate()?;
    let curto_outra_der = certificado(&outra, true)?;
    anyhow::ensure!(!contem(&curto_outra_der, &spki), "o controle saiu com a mesma chave");

    let longo = Arc::new(CertifiedKey::from_der(vec![longo_der.clone()], chave_der(&chave), &provider)?);
    let curto = Arc::new(CertifiedKey::from_der(vec![curto_der.clone()], chave_der(&chave), &provider)?);
    let curto_outra = Arc::new(CertifiedKey::from_der(vec![curto_outra_der.clone()], chave_der(&outra), &provider)?);

    let a = quinn::Endpoint::server(
        config(Arc::new(PorAlpn { longo: longo.clone(), curto }))?,
        SocketAddr::new(ip, porta(0)),
    )?;
    let a2 = quinn::Endpoint::server(
        config(Arc::new(PorAlpn { longo: longo.clone(), curto: curto_outra }))?,
        SocketAddr::new(ip, porta(1)),
    )?;
    let b = quinn::Endpoint::server(config(Arc::new(SoLongo(longo)))?, SocketAddr::new(ip, porta(2)))?;
    let surdo = tokio::net::UdpSocket::bind(SocketAddr::new(ip, porta(3))).await?;
    // A porta FECHADA: abre, anota o número, fecha. Ninguém escuta nela.
    let porta_fechada = {
        let s = std::net::UdpSocket::bind(SocketAddr::new(ip, porta(4)))?;
        s.local_addr()?.port()
    };

    let porta_a = a.local_addr()?.port();
    let porta_a2 = a2.local_addr()?.port();
    let porta_b = b.local_addr()?.port();
    let porta_mudo = surdo.local_addr()?.port();
    tokio::spawn(atender(a, "A"));
    tokio::spawn(atender(a2, "A2"));
    tokio::spawn(atender(b, "B"));
    tokio::spawn(mudo(surdo));

    let visto = cliente_seele(SocketAddr::from(([127, 0, 0, 1], porta_a))).await?;
    let h_longo = hex(&Sha256::digest(&longo_der));
    let h_curto = hex(&Sha256::digest(&curto_der));
    let h_curto_outra = hex(&Sha256::digest(&curto_outra_der));
    let h_visto = hex(&Sha256::digest(&visto));
    registro(&format!(
        "\"ev\":\"cliente_seele1\",\"recebeu\":{}",
        j(if h_visto == h_longo { "LONGO" } else if h_visto == h_curto { "CURTO" } else { "?" })
    ));

    println!(
        "{{\"porta_a\":{porta_a},\"porta_a2\":{porta_a2},\"porta_b\":{porta_b},\"porta_mudo\":{porta_mudo},\"porta_fechada\":{porta_fechada},\
\"curto\":\"{h_curto}\",\"curto_outra_chave\":\"{h_curto_outra}\",\"longo\":\"{h_longo}\",\
\"mesma_chave\":{mesma_chave},\"spki_sha256\":\"{}\",\"seele1_viu_longo\":{}}}",
        hex(&Sha256::digest(&spki)),
        h_visto == h_longo
    );
    // Fica de pé até ser morto: a medição do iPhone dura o quanto o dono quiser.
    std::future::pending::<()>().await;
    Ok(())
}
