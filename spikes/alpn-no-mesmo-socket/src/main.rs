//! Medida descartável: um quinn::Endpoint, dois ALPNs (seele/1 e h3), certificado
//! escolhido por ALPN no ClientHello, e o wtransport adotando a conexão h3.

use std::any::Any;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;
use sha2::{Digest, Sha256};

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

fn certificado(curto: bool) -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>)> {
    let chave = rcgen::KeyPair::generate()?;
    // O curto não nomeia endereço nenhum: mede se o navegador confere nome com hash.
    let nomes: Vec<String> = if curto { vec!["seele.invalid".into()] } else { vec!["localhost".into(), "127.0.0.1".into()] };
    let mut params = rcgen::CertificateParams::new(nomes)?;
    if curto {
        let agora = time::OffsetDateTime::now_utc();
        params.not_before = agora - time::Duration::days(1);
        params.not_after = agora + time::Duration::days(12);
    }
    let cert = params.self_signed(&chave)?;
    Ok((
        cert.der().clone(),
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(chave.serialize_der())),
    ))
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
    // Os números do seeled (seele-proto/src/transport.rs e seele-server/src/tls.rs).
    let mut t = quinn::TransportConfig::default();
    t.max_idle_timeout(Some(Duration::from_secs(20).try_into()?));
    t.keep_alive_interval(Some(Duration::from_secs(5)));
    t.datagram_receive_buffer_size(Some(1024 * 1024));
    cfg.transport_config(Arc::new(t));
    Ok(cfg)
}

async fn atender(endpoint: quinn::Endpoint, rotulo: &'static str) {
    while let Some(chegando) = endpoint.accept().await {
        tokio::spawn(async move {
            let r: Result<()> = async {
                let mut conectando = chegando.accept()?;
                let dados: Box<dyn Any> = conectando.handshake_data().await?;
                let alpn = dados
                    .downcast::<quinn::crypto::rustls::HandshakeData>()
                    .map_err(|_| anyhow!("sem HandshakeData"))?
                    .protocol;
                eprintln!("[{rotulo}] ALPN {:?}", alpn.as_deref().map(String::from_utf8_lossy));
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
                        eprintln!("[{rotulo}] WT caminho {} origem {:?}", pedido.path(), pedido.origin());
                        let conexao = pedido.accept().await?;
                        eprintln!("[{rotulo}] WT max_datagram_size {:?}", conexao.max_datagram_size());
                        let c2 = conexao.clone();
                        tokio::spawn(async move {
                            while let Ok(d) = c2.receive_datagram().await {
                                let _ = c2.send_datagram(d.payload());
                            }
                        });
                        // Um quadro com o enquadramento do frame.rs (4 bytes BE + corpo),
                        // lido e escrito pelo quinn::RecvStream/SendStream de dentro.
                        let (mut envio, mut recebe) = conexao.accept_bi().await?;
                        let r = recebe.quic_stream_mut();
                        let mut tam = [0u8; 4];
                        r.read_exact(&mut tam).await?;
                        let mut q = vec![0u8; u32::from_be_bytes(tam) as usize];
                        r.read_exact(&mut q).await?;
                        let s = envio.quic_stream_mut();
                        s.write_all(&tam).await?;
                        s.write_all(&q).await?;
                        s.finish()?;
                        let _ = conexao.closed().await;
                    }
                    outro => return Err(anyhow!("ALPN inesperado {outro:?}")),
                }
                Ok(())
            }
            .await;
            if let Err(e) = r {
                eprintln!("[{rotulo}] conexão: {e:#}");
            }
        });
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

#[tokio::main]
async fn main() -> Result<()> {
    let provider = rustls::crypto::ring::default_provider();
    let (longo_der, longo_chave) = certificado(false)?;
    let (curto_der, curto_chave) = certificado(true)?;
    let longo = Arc::new(CertifiedKey::from_der(vec![longo_der.clone()], longo_chave, &provider)?);
    let curto = Arc::new(CertifiedKey::from_der(vec![curto_der.clone()], curto_chave, &provider)?);

    let a = quinn::Endpoint::server(
        config(Arc::new(PorAlpn { longo: longo.clone(), curto }))?,
        "127.0.0.1:0".parse()?,
    )?;
    let b = quinn::Endpoint::server(config(Arc::new(SoLongo(longo)))?, "127.0.0.1:0".parse()?)?;
    let porta_a = a.local_addr()?.port();
    let porta_b = b.local_addr()?.port();
    tokio::spawn(atender(a, "A"));
    tokio::spawn(atender(b, "B"));

    let visto = cliente_seele(SocketAddr::from(([127, 0, 0, 1], porta_a))).await?;
    let h_longo = hex(&Sha256::digest(&longo_der));
    let h_curto = hex(&Sha256::digest(&curto_der));
    let h_visto = hex(&Sha256::digest(&visto));
    eprintln!(
        "[cliente seele/1] recebeu o {} (sha256 {})",
        if h_visto == h_longo { "LONGO" } else if h_visto == h_curto { "CURTO" } else { "?" },
        &h_visto[..16]
    );

    println!(
        "{{\"porta_a\":{porta_a},\"porta_b\":{porta_b},\"curto\":\"{h_curto}\",\"longo\":\"{h_longo}\",\"seele1_viu_longo\":{}}}",
        h_visto == h_longo
    );
    tokio::time::sleep(Duration::from_secs(120)).await;
    Ok(())
}
