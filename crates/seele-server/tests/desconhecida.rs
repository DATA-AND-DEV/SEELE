//! **Uma mensagem de um vocabulário mais novo é pulada e contada, e a sessão
//! segue.**
//!
//! O postcard escreve a variante pelo ordinal, e até a 0.15 um ordinal que esta
//! build não conhece encerrava a leitura do fluxo de controle: o laço lia
//! `FimDoQuadro::NaoEntendi` e a sessão acabava. Foi o que a subida de 7 para 8
//! custou em campo — uma sessão que morre sem dizer por quê.
//!
//! Desde o Servidor 1.0 (adendo de 08/10/2026, §6), o quadro tem comprimento
//! próprio, e um ordinal acima dos conhecidos na lista de cima vira
//! `Lido::Desconhecida`: o laço pula, conta e segue. A contagem é lida: a
//! primeira vai ao registro com o ordinal e a pessoa, e o total fica no
//! servidor. A tarefa P2 do plano F1b.
//!
//! # Por que um par cru
//!
//! Nenhum cliente desta build manda um ordinal que ela não conhece.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use ed25519_dalek::{Signer, SigningKey};
use seele_proto::control::{ClientMessage, ServerMessage};
use seele_proto::version::PROTOCOL_VERSION;
use seele_server::persistence::Location;
use seele_server::{frame, Daemon, ServerConfig};

/// O ordinal 200, que nenhuma build conhece, como o postcard o escreve: um
/// varint de dois bytes.
const QUADRO_DESCONHECIDO: [u8; 3] = [PROTOCOL_VERSION, 0xC8, 0x01];

/// Um verificador que aceita qualquer certificado: o que está sob teste é o
/// laço de controle, não o TOFU.
#[derive(Debug)]
struct AceitaQualquer(Arc<rustls::crypto::CryptoProvider>);

impl rustls::client::danger::ServerCertVerifier for AceitaQualquer {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

async fn servidor() -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        ..ServerConfig::default()
    };
    let daemon = Arc::new(Daemon::bind(config).await?);
    let endereco = daemon.local_addr()?;
    let atendendo = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = atendendo.run().await;
    });
    Ok((endereco, daemon))
}

/// Um par cru que já entrou: a ponta e o fluxo de controle.
struct Par {
    _ponta: quinn::Endpoint,
    envio: quinn::SendStream,
    recebe: quinn::RecvStream,
}

async fn entrar(endereco: SocketAddr, semente: u8) -> Result<Par> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut tls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AceitaQualquer(provider)))
        .with_no_client_auth();
    tls.alpn_protocols = vec![seele_proto::transport::ALPN.to_vec()];
    let mut ponta = quinn::Endpoint::client(SocketAddr::from(([127, 0, 0, 1], 0)))?;
    ponta.set_default_client_config(quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls).expect("config do QUIC"),
    )));
    let conexao = ponta.connect(endereco, "localhost")?.await?;
    let (mut envio, mut recebe) = conexao.open_bi().await?;

    let chave = SigningKey::from_bytes(&[semente; 32]);
    frame::write(
        &mut envio,
        &ClientMessage::Hello {
            faixa: seele_proto::FAIXA,
            client: "par-cru".into(),
            nickname: format!("pessoa{semente:03}"),
            public_key: chave.verifying_key().to_bytes().to_vec(),
            join_secret: None,
            material_e2ee: None,
            apis_de_mod: Vec::new(),
        },
    )
    .await?;
    let ServerMessage::Challenge { nonce } = frame::read::<ServerMessage>(&mut recebe).await?
    else {
        anyhow::bail!("o servidor não mandou Challenge");
    };
    frame::write(
        &mut envio,
        &ClientMessage::Response {
            proof: chave
                .sign(&prova_amarrada(&conexao, &nonce))
                .to_bytes()
                .to_vec(),
        },
    )
    .await?;
    let ServerMessage::Session { .. } = frame::read::<ServerMessage>(&mut recebe).await? else {
        anyhow::bail!("o servidor não mandou Session");
    };
    Ok(Par {
        _ponta: ponta,
        envio,
        recebe,
    })
}

/// Escreve `quadro` com os quatro bytes de comprimento na frente, sem passar
/// por nenhum `encode`.
async fn escrever_cru(envio: &mut quinn::SendStream, quadro: &[u8]) -> Result<()> {
    envio
        .write_all(&seele_proto::quadro::enquadrar(quadro)?)
        .await?;
    Ok(())
}

/// Manda um `Ping` e espera o `Pong` dele, pulando o resto do que o servidor
/// escreve no caminho (a instância, as presenças).
async fn pingar(par: &mut Par, timestamp: u64) -> Result<bool> {
    frame::write(&mut par.envio, &ClientMessage::Ping { timestamp }).await?;
    let esperado = ServerMessage::Pong { timestamp };
    Ok(tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match frame::read::<ServerMessage>(&mut par.recebe).await {
                Ok(mensagem) if mensagem == esperado => return true,
                Ok(_) => {}
                Err(_) => return false,
            }
        }
    })
    .await
    .unwrap_or(false))
}

/// **O caso da P2:** um quadro com o ordinal 200, e depois um `Ping`. O `Pong`
/// volta — a sessão seguiu —, e o servidor contou uma. Uma segunda
/// desconhecida na mesma sessão é contada, e **não** vai ao registro de novo:
/// um par defeituoso não enche o `seele.log`. A linha que vai diz o ordinal e
/// a pessoa.
///
/// **Um caso só, de propósito, e o [`segundo_despachante`] antes da
/// captura.** O registro é capturado por um assinante deste fio
/// (`set_default`), e o `tracing` guarda por ponto de registro se alguém se
/// interessa por ele. Com um segundo teste rodando em paralelo, noutro fio e
/// sem assinante, a linha deixava de chegar aqui: medido em 08/10, seis de
/// seis reprovações em paralelo, e três de três aprovações em série. O
/// segundo despachante tira essa dependência (pendência 56), e o único outro
/// teste deste arquivo é a prova dele. `current_thread` pelo mesmo motivo do
/// assinante: o servidor e a sessão rodam neste fio, e é nele que o assinante
/// vale.
#[tokio::test]
async fn a_mensagem_desconhecida_e_pulada_contada_e_registrada_uma_vez() -> Result<()> {
    segundo_despachante();
    let rastro = Rastro::default();
    let escritor = rastro.clone();
    let _guarda = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_writer(move || escritor.clone())
            .with_max_level(tracing::Level::INFO)
            .with_ansi(false)
            .without_time()
            .finish(),
    );

    let (endereco, daemon) = servidor().await?;
    let mut par = entrar(endereco, 81).await?;
    let contagem = &daemon.server().desconhecidas;

    escrever_cru(&mut par.envio, &QUADRO_DESCONHECIDO).await?;
    assert!(
        pingar(&mut par, 4242).await?,
        "a sessão não respondeu ao Ping depois de uma mensagem desconhecida"
    );
    assert_eq!(contagem.mensagens(), 1);
    assert_eq!(contagem.sessoes(), 1);

    escrever_cru(&mut par.envio, &QUADRO_DESCONHECIDO).await?;
    assert!(pingar(&mut par, 4343).await?, "a sessão não seguiu");
    assert_eq!(contagem.mensagens(), 2);
    assert_eq!(
        contagem.sessoes(),
        1,
        "a mesma sessão foi contada duas vezes"
    );

    let linhas: Vec<String> = rastro
        .texto()
        .lines()
        .filter(|linha| linha.contains("mensagem de controle desconhecida"))
        .map(str::to_owned)
        .collect();
    assert_eq!(linhas.len(), 1, "o registro de desconhecidas: {linhas:?}");
    assert!(
        linhas[0].contains("ordinal=200"),
        "a linha não diz o ordinal: {}",
        linhas[0]
    );
    assert!(
        linhas[0].contains("person="),
        "a linha não diz a pessoa: {}",
        linhas[0]
    );

    daemon.shutdown();
    Ok(())
}

/// Um `Write` que guarda o que o `tracing` escreve, para o teste ler depois.
#[derive(Clone, Default)]
struct Rastro(Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Rastro {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("o rastro trancou")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Rastro {
    fn texto(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("o rastro trancou")).into_owned()
    }
}

/// **Um segundo despachante, vivo o processo inteiro, que não escreve nada.**
/// A cópia deste binário do auxiliar do crate (`src/rastro_de_teste.rs`), que
/// um teste de integração não alcança: o `#[cfg(test)]` dele só vale para os
/// testes da biblioteca. O mesmo do app (`apps/seele-app/src/main.rs`).
///
/// O `tracing` decide uma vez só, na primeira passagem de qualquer thread, se
/// uma linha tem quem a ouça. Com um despachante só registrado, a pergunta vai
/// só ao da **thread que passou** (`tracing-core` 0.1.36, `callsite.rs`:
/// `Rebuilder::JustOne`), e um teste sem captura que passasse primeiro pela
/// linha a deixava marcada como «nunca» para quem a captura. Com dois vivos, a
/// pergunta vai a todos, e cada evento pergunta de novo à thread em que
/// acontece.
fn segundo_despachante() {
    static SEMPRE: std::sync::OnceLock<tracing::Dispatch> = std::sync::OnceLock::new();
    SEMPRE.get_or_init(|| tracing::Dispatch::new(tracing::subscriber::NoSubscriber::new()));
}

/// Marca o processo filho da prova abaixo: com ela, a prova roda a
/// [`ordem_forcada`] em vez de chamar a si mesma.
const PROVA_NO_FILHO: &str = "PROVA56_FILHO";

/// **Uma linha que outra thread tocou primeiro, fora de qualquer captura,
/// ainda é capturada.** A prova de ordem forçada da pendência 56, para a cópia
/// de [`segundo_despachante`] deste binário, com a mesma captura do teste de
/// cima, rodada num processo filho.
///
/// **Por que num processo filho.** A ordem forçada só decide quando a captura
/// dela é o único despachante vivo: é aí que o `tracing` pergunta só à thread
/// que passou. Com o teste de cima de pé ao mesmo tempo, ela passava mesmo sem
/// o auxiliar (3 em 10 rodadas do binário, em 09/10). O filho roda só ela
/// (`--exact`, `--test-threads=1`), e ali a captura é a única. O pai exige
/// `1 passed`, porque um `--exact` que não acha o teste sai com sucesso e
/// `0 passed`, e a prova passaria sem ter rodado.
#[test]
fn uma_linha_que_outra_thread_tocou_primeiro_ainda_e_capturada() {
    if std::env::var_os(PROVA_NO_FILHO).is_some() {
        return ordem_forcada();
    }
    let saida =
        std::process::Command::new(std::env::current_exe().expect("o binário destes testes"))
            .args([
                "--exact",
                "uma_linha_que_outra_thread_tocou_primeiro_ainda_e_capturada",
                "--test-threads=1",
            ])
            .env(PROVA_NO_FILHO, "1")
            .output()
            .expect("o processo filho da prova não subiu");
    let fala = String::from_utf8_lossy(&saida.stdout);
    assert!(
        saida.status.success() && fala.contains("ok. 1 passed;"),
        "a ordem forçada reprovou no processo filho, onde a captura é o único despachante \
         vivo, ou o filtro não achou a prova e ela não rodou: {fala}{}",
        String::from_utf8_lossy(&saida.stderr)
    );
}

/// A outra thread toca a linha primeiro, com a captura já de pé, e a linha
/// ainda chega à captura. E a linha da outra thread continua fora: a captura
/// segue sendo só da thread corrente.
///
/// Só decide com a captura dela como único despachante vivo, e por isso roda no
/// processo filho da prova acima.
fn ordem_forcada() {
    fn diz() {
        tracing::warn!("linha tocada primeiro fora da captura");
    }
    segundo_despachante();
    let rastro = Rastro::default();
    let escritor = rastro.clone();
    let assinante = tracing_subscriber::fmt()
        .with_writer(move || escritor.clone())
        .with_ansi(false)
        .without_time()
        .finish();
    tracing::subscriber::with_default(assinante, || {
        std::thread::spawn(diz)
            .join()
            .expect("a outra thread terminou");
        diz();
    });

    let texto = rastro.texto();
    assert!(
        texto.contains("linha tocada primeiro fora da captura"),
        "uma linha que outra thread tocou primeiro sumiu do rastro, e o teste da mensagem \
         desconhecida passa ou falha conforme a ordem das threads: {texto}"
    );
    assert_eq!(
        texto.lines().count(),
        1,
        "a linha da outra thread entrou no rastro desta captura, e um teste passaria a ler o \
         que outro teste escreveu: {texto}"
    );
}

/// A mensagem que a prova assina: o rótulo, a impressão do certificado que o
/// TLS desta conexão conferiu e o nonce (o S2).
fn prova_amarrada(conexao: &quinn::Connection, nonce: &[u8]) -> Vec<u8> {
    let cadeia = conexao
        .peer_identity()
        .and_then(|identidade| {
            identidade
                .downcast::<Vec<rustls::pki_types::CertificateDer<'static>>>()
                .ok()
        })
        .expect("o TLS sempre entrega a cadeia do servidor");
    let impressao = seele_proto::transport::certificate_digest(
        cadeia
            .first()
            .expect("a cadeia tem um certificado")
            .as_ref(),
    );
    let nonce: [u8; 32] = nonce.try_into().expect("o desafio tem 32 bytes");
    seele_proto::assinatura::prova_de_cliente(&impressao, &nonce)
}
