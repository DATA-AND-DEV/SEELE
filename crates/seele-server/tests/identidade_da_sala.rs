//! A identidade da sala pessoal na 0.17: **a chave da pessoa na apresentação
//! `0x03`, sobre um DTLS efêmero** (contrato da fase 2, D3).
//!
//! Até a 0.16 a sala apresentava no TLS um certificado Ed25519 feito da chave
//! da pessoa, e quem liga conferia o código contra ele. O DTLS do WebRTC não
//! negocia Ed25519: na 0.17 o certificado DTLS da sala é P-256 e efêmero, e a
//! identidade vai na primeira mensagem do `ctl`, a `0x03`: a chave Ed25519
//! (32 B) e a assinatura dela sobre `apresentacao_da_sala(fp_dtls)`, que amarra
//! a identidade **a esta** conexão DTLS. Quem liga confere com o
//! `ConfereCodigo` antes do `Hello`.
//!
//! A prova do cliente (D5) amarra a impressão da sala, que é o SHA-256 dos 32
//! bytes da chave (o que quem liga por código calcula), e não a do DTLS
//! efêmero.
//!
//! O teste do Ed25519 no TLS do quinn (`o_ed25519_aperta_a_mao_no_quic…`) e
//! o do código conferido no TLS saíram junto com o QUIC: os equivalentes são
//! os de baixo, sobre o enlace de verdade.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;

use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use seele_proto::control::{ClientMessage, DisconnectReason, ServerMessage};
use seele_server::recepcao::Sala;
use seele_server::tls::Identity;
use seele_server::{frame, Daemon, ServerConfig};

mod comum;

/// A semente da dona da sala.
const DONA: [u8; 32] = [42; 32];

/// Uma sala de verdade, com a identidade da semente e a campainha.
async fn sala(semente: [u8; 32]) -> Arc<Daemon> {
    let pasta = tempfile::tempdir().unwrap();
    let dona = SigningKey::from_bytes(&semente).verifying_key().to_bytes();
    let config = ServerConfig {
        identidade: Some(Identity::da_semente(&semente).unwrap()),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: seele_server::persistence::Location::File(pasta.keep().join("seele.db")),
        sala: Some(Sala::de(dona)),
        ..ServerConfig::default()
    };
    let servidor = Arc::new(Daemon::bind(config).await.expect("subir a sala"));
    let rodando = Arc::clone(&servidor);
    tokio::spawn(async move { rodando.run().await });
    servidor
}

/// Confere a apresentação como o `ConfereCodigo` do `seele-core` faz: a chave
/// tem de ser a esperada, e a assinatura tem de cobrir a impressão DTLS **que
/// esta conexão viu**.
fn conferir(apresentacao: &[u8], fp_dtls: &[u8; 32], esperada: &VerifyingKey) -> bool {
    if apresentacao.len() != 96 || apresentacao[..32] != esperada.to_bytes() {
        return false;
    }
    let assinatura = ed25519_dalek::Signature::from_slice(&apresentacao[32..]).unwrap();
    esperada
        .verify_strict(
            &seele_proto::assinatura::apresentacao_da_sala(fp_dtls),
            &assinatura,
        )
        .is_ok()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sala_se_apresenta_com_a_chave_da_pessoa_sobre_o_dtls_desta_conexao() {
    let servidor = sala(DONA).await;
    let ligado = comum::ligar(servidor.local_addr().unwrap()).await.unwrap();
    let dona = SigningKey::from_bytes(&DONA).verifying_key();
    assert!(
        conferir(
            ligado.conexao.apresentacao(),
            &ligado.conexao.impressao_dtls_do_par(),
            &dona
        ),
        "a apresentação da sala não confere com a chave da dona sobre este DTLS"
    );
    // A mesma apresentação não vale para outro DTLS: é o que impede quem
    // está no meio de repassá-la com um certificado próprio.
    assert!(!conferir(ligado.conexao.apresentacao(), &[0; 32], &dona));
    // E outra chave não passa: um código errado é recusado antes do Hello.
    let outra = SigningKey::from_bytes(&[7; 32]).verifying_key();
    assert!(!conferir(
        ligado.conexao.apresentacao(),
        &ligado.conexao.impressao_dtls_do_par(),
        &outra
    ));
    // A impressão anunciada é a da chave, e não a do DTLS efêmero.
    let da_chave: String = seele_proto::transport::certificate_digest(&dona.to_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(servidor.fingerprint(), da_chave);
    servidor.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn o_dtls_da_sala_muda_a_cada_subida_e_a_identidade_nao() {
    let uma = sala(DONA).await;
    let outra = sala(DONA).await;
    let a = comum::ligar(uma.local_addr().unwrap()).await.unwrap();
    let b = comum::ligar(outra.local_addr().unwrap()).await.unwrap();
    assert_ne!(
        a.conexao.impressao_dtls_do_par(),
        b.conexao.impressao_dtls_do_par(),
        "o DTLS da sala não é efêmero"
    );
    assert_eq!(
        a.conexao.apresentacao()[..32],
        b.conexao.apresentacao()[..32]
    );
    assert_eq!(uma.fingerprint(), outra.fingerprint());
    uma.shutdown();
    outra.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn o_seeled_se_apresenta_vazio_e_o_dtls_dele_e_o_fp_do_banco() {
    let config = ServerConfig {
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        ..ServerConfig::default()
    };
    let servidor = Arc::new(Daemon::bind(config).await.unwrap());
    let rodando = Arc::clone(&servidor);
    tokio::spawn(async move { rodando.run().await });
    let ligado = comum::ligar(servidor.local_addr().unwrap()).await.unwrap();
    assert!(ligado.conexao.apresentacao().is_empty());
    let visto: String = ligado
        .conexao
        .impressao_dtls_do_par()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(
        visto,
        servidor.fingerprint(),
        "o a=fingerprint não é o fp do convite"
    );
    servidor.shutdown();
}

/// Entra na sala como a dona (que entra sem tocar), com a prova feita sobre
/// `impressao`. Devolve o que veio depois da `Response`.
async fn entrar_como_dona(servidor: &Daemon, impressao: [u8; 32]) -> ServerMessage {
    let comum::Ligado {
        ponta: _ponta,
        conexao,
        mut envio,
        mut recebe,
    } = comum::ligar(servidor.local_addr().unwrap()).await.unwrap();
    let dona = SigningKey::from_bytes(&DONA);
    frame::write(
        &mut envio,
        &ClientMessage::Hello {
            faixa: seele_proto::FAIXA,
            client: "par-cru".into(),
            nickname: "dona".into(),
            public_key: dona.verifying_key().to_bytes().to_vec(),
            join_secret: None,
            material_e2ee: None,
            apis_de_mod: seele_proto::mods::APIS_ACEITAS.to_vec(),
        },
    )
    .await
    .unwrap();
    let ServerMessage::Challenge { nonce } =
        frame::read::<ServerMessage>(&mut recebe).await.unwrap()
    else {
        panic!("a sala não mandou Challenge");
    };
    frame::write(
        &mut envio,
        &ClientMessage::Response {
            proof: dona
                .sign(&comum::prova_para(&impressao, &conexao, &nonce))
                .to_bytes()
                .to_vec(),
        },
    )
    .await
    .unwrap();
    frame::read::<ServerMessage>(&mut recebe).await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sala_aceita_a_prova_amarrada_a_impressao_da_chave_e_recusa_a_do_dtls() {
    let servidor = sala(DONA).await;
    let da_chave = seele_proto::transport::certificate_digest(
        &SigningKey::from_bytes(&DONA).verifying_key().to_bytes(),
    );
    assert!(
        matches!(
            entrar_como_dona(&servidor, da_chave).await,
            ServerMessage::Session { .. }
        ),
        "a prova amarrada à impressão da sala não foi aceita"
    );
    // A impressão do DTLS efêmero não é a identidade da sala: uma prova sobre
    // ela é recusada como qualquer assinatura que não confere.
    let ligado = comum::ligar(servidor.local_addr().unwrap()).await.unwrap();
    let do_dtls = ligado.conexao.impressao_dtls_do_par();
    drop(ligado);
    assert!(matches!(
        entrar_como_dona(&servidor, do_dtls).await,
        ServerMessage::Disconnecting {
            reason: DisconnectReason::CredentialRejected
        }
    ));
    {
        let banco = servidor.server().persistence.lock().await;
        assert!(
            seele_server::tls::identidade_guardada(&banco).is_none(),
            "a chave da pessoa foi gravada no banco do servidor"
        );
    }
    servidor.shutdown();
}

#[test]
fn a_mesma_semente_da_o_mesmo_der() {
    let um = Identity::da_semente(&[42; 32]).expect("a primeira identidade");
    let outro = Identity::da_semente(&[42; 32]).expect("a segunda identidade");
    assert_eq!(um.chain, outro.chain);
    assert_ne!(Identity::da_semente(&[43; 32]).unwrap().chain, um.chain);
}

#[test]
fn o_debug_da_configuracao_nao_mostra_a_chave() {
    // Bytes que não aparecem por acaso num texto de depuração.
    let semente: [u8; 32] = core::array::from_fn(|i| 0x9c ^ u8::try_from(i).unwrap());
    let config = ServerConfig {
        identidade: Some(Identity::da_semente(&semente).unwrap()),
        ..ServerConfig::default()
    };
    let texto = format!("{config:?}");
    let em_lista = semente[..6]
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let em_hex: String = semente[..6].iter().map(|b| format!("{b:02x}")).collect();
    assert!(
        !texto.contains(&em_lista)
            && !texto.contains(&em_hex)
            && !texto.contains(&em_hex.to_uppercase()),
        "o Debug da configuração mostrou a chave da pessoa: {texto}"
    );
}
