//! **O `seeled` atende `seele/1` e `h3` no mesmo socket, e escolhe o
//! certificado pelo ALPN** (Tarefa 2.4 da F4; ADR 0057, emenda ao 0003).
//!
//! O nativo recebe o certificado longo, como sempre. O navegador recebe o
//! **curto da janela vigente**: com no máximo 13 dias e a chave do longo, que
//! é o que a medição 3 no iPhone mostrou que serve
//! (`docs/evidencias/medicao-3-iphone/LEIA.md`). As janelas são o que o
//! `HashesDoNavegador` vai levar, e o link do navegador escreve no `wh=`.
//!
//! A sessão pelo `h3` é a de `tests/sessao_por_h3.rs`. Um navegador que
//! recusa o certificado é contado, e o registro diz a frase do ADR.
//!
//! O cliente daqui anota o certificado que o servidor apresentou, como o do
//! `spikes/alpn-no-mesmo-socket`.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use rustls::pki_types::CertificateDer;
use seele_proto::transport::{
    certificate_digest, certificate_fingerprint, ALPN, ALPN_DO_NAVEGADOR, ANTES_DA_JANELA,
    JANELAS_ANUNCIADAS, PASSO_DA_JANELA, VALIDADE_DO_CURTO,
};
use seele_server::persistence::Location;
use seele_server::tls::Identity;
use seele_server::{Daemon, ServerConfig};

/// Um verificador que anota o certificado apresentado, e aceita ou recusa.
#[derive(Debug)]
struct Anota {
    provider: Arc<rustls::crypto::CryptoProvider>,
    visto: Arc<Mutex<Option<CertificateDer<'static>>>>,
    recusa: bool,
}

impl rustls::client::danger::ServerCertVerifier for Anota {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        *self.visto.lock().unwrap() = Some(end_entity.clone().into_owned());
        if self.recusa {
            return Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::UnknownIssuer,
            ));
        }
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

async fn servidor(identidade: Option<Identity>) -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        identidade,
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

/// O que um cliente com `alpn` viu: o certificado apresentado, e a conexão, se
/// o aperto de mão acabou.
async fn apertar(
    endereco: SocketAddr,
    alpn: &[u8],
    recusa: bool,
) -> (
    Option<CertificateDer<'static>>,
    Option<quinn::Connection>,
    quinn::Endpoint,
) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let visto = Arc::new(Mutex::new(None));
    let mut tls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Anota {
            provider,
            visto: Arc::clone(&visto),
            recusa,
        }))
        .with_no_client_auth();
    tls.alpn_protocols = vec![alpn.to_vec()];
    let quic = quinn::crypto::rustls::QuicClientConfig::try_from(tls).unwrap();
    let mut ponta = quinn::Endpoint::client(SocketAddr::from(([127, 0, 0, 1], 0))).unwrap();
    ponta.set_default_client_config(quinn::ClientConfig::new(Arc::new(quic)));
    let conexao = tokio::time::timeout(
        Duration::from_secs(5),
        ponta.connect(endereco, "localhost").unwrap(),
    )
    .await
    .expect("o aperto de mão não acabou em 5 s")
    .ok();
    let visto = visto.lock().unwrap().clone();
    (visto, conexao, ponta)
}

fn agora() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// A chave pública P-256 de um certificado: o ponto de 65 bytes depois de
/// `03 42 00` no SPKI.
fn chave_publica(der: &[u8]) -> Vec<u8> {
    let em = der
        .windows(4)
        .position(|janela| janela == [0x03, 0x42, 0x00, 0x04])
        .expect("o certificado não tem uma chave P-256");
    der[em + 3..em + 3 + 65].to_vec()
}

#[tokio::test]
async fn o_nativo_recebe_o_longo() {
    let (endereco, daemon) = servidor(None).await.unwrap();
    let (visto, conexao, _ponta) = apertar(endereco, ALPN, false).await;
    let visto = visto.expect("o servidor não apresentou certificado ao `seele/1`");
    assert!(conexao.is_some(), "o `seele/1` não entrou");
    assert_eq!(
        certificate_fingerprint(visto.as_ref()),
        daemon.fingerprint(),
        "o `seele/1` não recebeu o certificado longo, o que o nativo fixa"
    );
}

#[tokio::test]
async fn o_navegador_recebe_o_curto_da_janela_vigente_com_a_chave_do_longo() {
    let (endereco, daemon) = servidor(None).await.unwrap();
    let (longo, _, _p1) = apertar(endereco, ALPN, false).await;
    let (curto, conexao, _p2) = apertar(endereco, ALPN_DO_NAVEGADOR, false).await;
    let (longo, curto) = (
        longo.unwrap(),
        curto.expect("o servidor não apresentou certificado ao `h3`"),
    );
    assert!(conexao.is_some(), "o aperto de mão do `h3` não acabou");
    assert_ne!(
        longo, curto,
        "o `h3` recebeu o longo, que o navegador recusa"
    );

    let hash = certificate_digest(curto.as_ref());
    let agora = agora();
    let janela = daemon
        .janelas_do_navegador()
        .iter()
        .find(|janela| janela.hash == hash)
        .expect("o curto apresentado não está entre as janelas anunciadas");
    assert!(
        janela.nao_antes <= agora && agora < janela.nao_depois,
        "o curto apresentado não vale agora: {janela:?}"
    );
    assert_eq!(
        seele_server::tls::janela_de(janela.nao_antes + ANTES_DA_JANELA.as_secs()),
        seele_server::tls::janela_de(agora),
        "o curto apresentado não é o da janela de agora (a seguinte, no último dia, \
         seria recusada por um link que a de agora ainda atende)"
    );
    assert_eq!(
        chave_publica(curto.as_ref()),
        chave_publica(longo.as_ref()),
        "o curto não tem a chave do longo (a medição 3 decidiu a mesma chave)"
    );
}

#[tokio::test]
async fn as_janelas_cobrem_um_ano_de_treze_em_treze_dias() {
    let (_, daemon) = servidor(None).await.unwrap();
    let janelas = daemon.janelas_do_navegador();
    assert_eq!(janelas.len(), JANELAS_ANUNCIADAS);
    let agora = agora();
    assert!(
        janelas[0].nao_antes <= agora && agora < janelas[0].nao_depois,
        "a primeira janela não é a vigente: {:?}",
        janelas[0]
    );
    for par in janelas.windows(2) {
        assert_eq!(
            par[1].nao_antes - par[0].nao_antes,
            PASSO_DA_JANELA.as_secs()
        );
    }
    for janela in janelas {
        assert_eq!(
            janela.nao_depois - janela.nao_antes,
            VALIDADE_DO_CURTO.as_secs()
        );
    }
    assert!(
        janelas.last().unwrap().nao_depois > agora + 360 * 86_400,
        "as janelas não chegam a um ano"
    );
}

#[tokio::test]
async fn o_navegador_que_recusa_o_certificado_e_contado() {
    let (endereco, daemon) = servidor(None).await.unwrap();
    assert_eq!(daemon.recusas_do_navegador(), 0);
    let (visto, conexao, _ponta) = apertar(endereco, ALPN_DO_NAVEGADOR, true).await;
    assert!(
        visto.is_some() && conexao.is_none(),
        "o cliente não recusou"
    );
    let comeco = tokio::time::Instant::now();
    while daemon.recusas_do_navegador() == 0 && comeco.elapsed() < Duration::from_secs(5) {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        daemon.recusas_do_navegador(),
        1,
        "a recusa do navegador não foi contada, e o registro dizia o aviso genérico"
    );
    // O nativo que recusa não é um navegador.
    let _ = apertar(endereco, ALPN, true).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(daemon.recusas_do_navegador(), 1);
}

#[tokio::test]
async fn a_sala_pessoal_nao_anuncia_o_h3() {
    let identidade = Identity::da_semente(&[7; 32]).unwrap();
    let (endereco, daemon) = servidor(Some(identidade)).await.unwrap();
    assert!(
        daemon.janelas_do_navegador().is_empty(),
        "a sala, com a chave Ed25519, anunciou janelas que o navegador não aceita"
    );
    let (_, conexao, _ponta) = apertar(endereco, ALPN_DO_NAVEGADOR, false).await;
    assert!(conexao.is_none(), "a sala aceitou o `h3`");
    let (_, conexao, _ponta) = apertar(endereco, ALPN, false).await;
    assert!(conexao.is_some(), "a sala deixou de atender o `seele/1`");
}

/// **Reabrir o `seeled` no mesmo banco anuncia os mesmos hashes** (Tarefa 2.5):
/// o link que o navegador levou continua abrindo.
#[tokio::test]
async fn reabrir_o_servidor_mantem_os_hashes_das_janelas() {
    let pasta = tempfile::tempdir().unwrap();
    let banco = pasta.path().join("seele.db");
    let abrir = |banco: std::path::PathBuf| async move {
        let config = ServerConfig {
            name: "Casa".into(),
            listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            database: Location::File(banco),
            ..ServerConfig::default()
        };
        Daemon::bind(config).await.unwrap()
    };
    let primeiro = abrir(banco.clone()).await;
    let antes = primeiro.janelas_do_navegador().to_vec();
    primeiro.shutdown();
    drop(primeiro);
    let segundo = abrir(banco).await;
    assert_eq!(antes.len(), JANELAS_ANUNCIADAS);
    assert_eq!(
        segundo.janelas_do_navegador(),
        antes.as_slice(),
        "reabrir o servidor trocou os hashes, e os links já mandados deixariam de abrir"
    );
}

/// **O link do navegador de quem hospeda abre** (D-web-6): o primeiro hash dele
/// é o do curto que o `h3` apresenta agora, num aperto de mão de verdade, e o
/// resto do link é o convite que a casca mostra — o mesmo endereço, a mesma
/// impressão e o mesmo token de uso único.
#[tokio::test]
async fn o_link_do_navegador_de_quem_hospeda_nomeia_o_curto_apresentado() {
    use seele_proto::uri::{
        analisar, analisar_no_navegador, LIMITE_DE_HASHES_DO_NAVEGADOR, ORIGEM_DO_NAVEGADOR,
    };
    use seele_server::hospedagem::{Hospedagem, SemLinkDoNavegador};

    let hospedagem = Hospedagem::iniciar(0, Location::Memory, "Casa", None, |_| Ok(()))
        .await
        .unwrap();
    let agora = agora();
    // O token de verdade, como o `criar_convite_do_server` da casca o cria.
    let token = {
        let banco = hospedagem.persistence();
        let mut persistence = banco.lock().await;
        seele_server::admissao::criar_convite(&mut persistence, "para o navegador").unwrap()
    };
    let convite = hospedagem.convite_com_token(&token);
    let link = hospedagem.link_do_navegador(&convite, agora).unwrap();
    let lido = analisar_no_navegador(&link.link, ORIGEM_DO_NAVEGADOR).unwrap();
    let original = analisar(&convite).unwrap();
    assert_eq!(lido.convite.alvo, original.alvo);
    assert_eq!(lido.convite.alternativos, original.alternativos);
    assert_eq!(lido.convite.impressao_digital, original.impressao_digital);
    assert_eq!(lido.convite.token, Some(token));
    assert_eq!(lido.hashes.len(), LIMITE_DE_HASHES_DO_NAVEGADOR);
    assert_eq!(lido.ate, Some(link.ate));
    assert!(agora < link.renova && link.renova < link.ate, "{link:?}");

    let porta = hospedagem.endereco().port();
    let (curto, conexao, _ponta) = apertar(
        SocketAddr::from(([127, 0, 0, 1], porta)),
        ALPN_DO_NAVEGADOR,
        false,
    )
    .await;
    assert!(conexao.is_some(), "o aperto de mão do `h3` não acabou");
    let apresentado: String = certificate_digest(curto.unwrap().as_ref())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(
        lido.hashes[0], apresentado,
        "o link não nomeia, primeiro, o curto que o servidor apresenta agora"
    );

    let de_outro = convite.replace(
        original.impressao_digital.as_deref().unwrap(),
        &"ab".repeat(32),
    );
    assert_eq!(
        hospedagem.link_do_navegador(&de_outro, agora),
        Err(SemLinkDoNavegador::OutroServidor)
    );
    assert_eq!(
        hospedagem.link_do_navegador("um texto qualquer", agora),
        Err(SemLinkDoNavegador::ConviteInvalido)
    );
    hospedagem.encerrar().await;
}

/// A sala pessoal não serve ao navegador (a chave Ed25519), e diz isso em vez
/// de dar um link que não abre.
#[tokio::test]
async fn a_sala_pessoal_nao_da_link_do_navegador() {
    use seele_server::hospedagem::{Hospedagem, SemLinkDoNavegador};

    let sala = Hospedagem::iniciar_sala(0, &[7; 32], "dona", None)
        .await
        .unwrap();
    assert_eq!(
        sala.link_do_navegador(&sala.convite(), agora()),
        Err(SemLinkDoNavegador::SemJanelas)
    );
    sala.encerrar().await;
}
