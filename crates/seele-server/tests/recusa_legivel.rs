//! **A recusa por versão sai num carimbo que o par lê.**
//!
//! O byte 0 de todo quadro é o número do protocolo, e uma build só decodifica
//! os números que ela fala. Até a 0.15, o servidor decodificava o primeiro
//! quadro inteiro antes de olhar o carimbo: um par de outra versão era recusado
//! como `ProtocolViolation`, carimbado com o número **do servidor** — e a
//! 0.15.1, que só lê o 8, via «não foi possível alcançar o servidor» em vez de
//! «versão incompatível».
//!
//! Desde o Servidor 1.0 (adendo de 08/10/2026, ADR 0058), o servidor lê o
//! primeiro quadro **bruto**, confere o byte 0 antes do `decode` e responde
//! `control::recusa_por_versao`: o carimbo do par quando ele é de antes da
//! 1.0, o do servidor quando é da linha 1 em diante. Depois do `Hello`, todo
//! quadro sai com o número negociado. A tarefa P1 do plano F1b.
//!
//! # Por que um par cru
//!
//! O que está sob teste é o byte 0 do primeiro quadro, e nenhum cliente desta
//! build manda um carimbo que não seja o dele. `tests/tipo_de_fluxo.rs` monta o
//! mesmo par pelo mesmo motivo.

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
use seele_server::{Daemon, ServerConfig};

/// Um verificador que aceita qualquer certificado: o que está sob teste é o
/// carimbo, não o TOFU.
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

/// Um par cru: a ponta, a conexão e o fluxo de controle aberto.
struct Par {
    _ponta: quinn::Endpoint,
    conexao: quinn::Connection,
    envio: quinn::SendStream,
    recebe: quinn::RecvStream,
}

async fn abrir(endereco: SocketAddr) -> Result<Par> {
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
    let (envio, recebe) = conexao.open_bi().await?;
    Ok(Par {
        _ponta: ponta,
        conexao,
        envio,
        recebe,
    })
}

/// Escreve `quadro` com os quatro bytes de comprimento na frente, sem passar
/// por nenhum `encode`: o carimbo é o que o teste puser no byte 0.
async fn escrever_cru(envio: &mut quinn::SendStream, quadro: &[u8]) -> Result<()> {
    envio
        .write_all(&seele_proto::quadro::enquadrar(quadro)?)
        .await?;
    Ok(())
}

/// Lê um quadro inteiro e devolve os bytes dele, carimbo incluído, sem
/// decodificar nada.
async fn ler_cru(recebe: &mut quinn::RecvStream) -> Result<Vec<u8>> {
    let mut prefixo = [0_u8; seele_proto::quadro::PREFIXO_LEN];
    recebe.read_exact(&mut prefixo).await?;
    let mut quadro = vec![0_u8; seele_proto::quadro::comprimento(prefixo)?];
    recebe.read_exact(&mut quadro).await?;
    Ok(quadro)
}

/// O que o servidor responde a um primeiro quadro carimbado `carimbo`, com um
/// corpo qualquer.
async fn resposta_ao_carimbo(carimbo: u8) -> Result<Vec<u8>> {
    let (endereco, daemon) = servidor().await?;
    let mut par = abrir(endereco).await?;
    escrever_cru(&mut par.envio, &[carimbo, 0, 1, 2, 3]).await?;
    let resposta = tokio::time::timeout(Duration::from_secs(5), ler_cru(&mut par.recebe)).await??;
    daemon.shutdown();
    Ok(resposta)
}

/// **Um par de antes da 1.0 recebe a recusa no carimbo dele.** A 0.15.1 fala
/// o 8, e `[8, 11, 0]` é o `Disconnecting { Incompatible }` que ela lê como
/// «versão incompatível».
#[tokio::test(flavor = "multi_thread")]
async fn um_par_de_antes_da_1_0_le_a_recusa_no_carimbo_dele() -> Result<()> {
    let resposta = resposta_ao_carimbo(8).await?;
    assert_eq!(
        resposta,
        seele_proto::control::recusa_por_versao(8),
        "a recusa ao carimbo 8 não saiu no carimbo 8"
    );
    assert_eq!(resposta, [8, 11, 0]);
    Ok(())
}

/// **Um par de um número que esta build não fala, da linha 1 em diante,
/// recebe a recusa no número do servidor** — é por ele que o par lê a linha de
/// quem recusou.
#[tokio::test(flavor = "multi_thread")]
async fn um_par_de_numero_maior_le_a_recusa_no_carimbo_do_servidor() -> Result<()> {
    let resposta = resposta_ao_carimbo(30).await?;
    assert_eq!(resposta, seele_proto::control::recusa_por_versao(30));
    assert_eq!(resposta, [PROTOCOL_VERSION, 11, 0]);
    Ok(())
}

/// **O caminho certo:** um `Hello` carimbado 9, com a faixa `[9, 9]`, entra, e
/// todo quadro que volta sai com o número negociado.
#[tokio::test(flavor = "multi_thread")]
async fn quem_fala_o_9_entra_e_todo_quadro_volta_carimbado_9() -> Result<()> {
    let (endereco, daemon) = servidor().await?;
    let mut par = abrir(endereco).await?;
    let chave = SigningKey::from_bytes(&[71; 32]);
    let hello = seele_proto::control::encode_em(
        PROTOCOL_VERSION,
        &ClientMessage::Hello {
            faixa: seele_proto::version::Faixa {
                minima: PROTOCOL_VERSION,
                maxima: PROTOCOL_VERSION,
            },
            client: "par-cru".into(),
            nickname: "pessoa071".into(),
            public_key: chave.verifying_key().to_bytes().to_vec(),
            join_secret: None,
            material_e2ee: None,
            apis_de_mod: Vec::new(),
        },
    )?;
    escrever_cru(&mut par.envio, &hello).await?;

    let mut carimbos = Vec::new();
    let desafio = ler_cru(&mut par.recebe).await?;
    carimbos.push(desafio[0]);
    let ServerMessage::Challenge { nonce } = seele_proto::control::decode(&desafio)? else {
        anyhow::bail!("o servidor não mandou Challenge");
    };
    let prova = chave
        .sign(&prova_amarrada(&par.conexao, &nonce))
        .to_bytes()
        .to_vec();
    let resposta = seele_proto::control::encode_em(
        PROTOCOL_VERSION,
        &ClientMessage::Response { proof: prova },
    )?;
    escrever_cru(&mut par.envio, &resposta).await?;

    // Até a `Session`, e mais o que vier logo depois dela: a instância, os
    // anúncios, as presenças.
    let mut entrou = false;
    while let Ok(Ok(quadro)) =
        tokio::time::timeout(Duration::from_millis(500), ler_cru(&mut par.recebe)).await
    {
        carimbos.push(quadro[0]);
        if let Ok(ServerMessage::Session { .. }) = seele_proto::control::decode(&quadro) {
            entrou = true;
        }
    }

    assert!(entrou, "quem fala o 9 não entrou");
    assert!(
        carimbos.iter().all(|&carimbo| carimbo == PROTOCOL_VERSION),
        "um quadro voltou com outro carimbo: {carimbos:?}"
    );
    daemon.shutdown();
    Ok(())
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
