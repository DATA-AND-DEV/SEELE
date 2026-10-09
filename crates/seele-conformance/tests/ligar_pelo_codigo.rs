//! Quem liga confere o código dentro do TLS, antes do `Hello` (tarefa 6b do
//! Plano 5, ADR 0060).
//!
//! Um `Destino` com `Esperada::Codigo` conecta pelo `ConfereCodigo`, e não pelo
//! TOFU por endereço:
//!
//! - a chave de quem atendeu tem de ser a do código, ou a conexão cai antes de
//!   qualquer quadro, com o motivo tipado;
//! - depois da primeira ligação completa, a chave inteira fica pinada sob
//!   `@CÓDIGO`, e dali em diante uma chave diferente com o mesmo código é
//!   «se passou por»;
//! - o SNI é sempre o nome da sala, e nunca o código.
//!
//! O caminho medido de ponta a ponta é o de quem recebe ligando para a própria
//! sala (`127.0.0.1:8385`, tarefa 7e): ele não toca, então não depende do
//! cliente que lê o `Tocando` (tarefa 6e).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use seele_core::confere_codigo::{chave_do_pino, FalhaDoCodigo};
use seele_core::enlace::{Destino, Enlace, Esperada};
use seele_core::{ConnectError, MemoryPinStore, PinStore};
use seele_proto::codigo::Codigo;
use seele_proto::transport::NOME_DA_SALA;
use seele_server::recepcao::Sala;
use seele_server::tls::Identity;
use seele_server::{Daemon, ServerConfig};

mod vaga;

/// A semente de quem recebe.
const DONO: u8 = 1;

fn chave(semente: u8) -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::from_bytes(&[semente; 32])
}

fn codigo_de(semente: u8) -> Codigo {
    Codigo::da_chave(&chave(semente).verifying_key().to_bytes())
}

/// A sala de `DONO`, em memória.
async fn sala() -> SocketAddr {
    let config = ServerConfig {
        identidade: Some(Identity::da_semente(&[DONO; 32]).unwrap()),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        voice_room_limit: 6,
        sala: Some(Sala::de(chave(DONO).verifying_key().to_bytes())),
        ..ServerConfig::default()
    };
    let servidor = Arc::new(Daemon::bind(config).await.unwrap());
    let endereco = servidor.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = servidor.run().await;
    });
    endereco
}

fn destino(endereco: SocketAddr, codigo: Codigo, nome_tls: &str) -> Destino {
    Destino {
        servidor: endereco,
        nome_tls: nome_tls.to_owned(),
        chave_do_pin: chave_do_pino(&codigo),
        apelido: "dona".into(),
        segredo: None,
        esperada: Some(Esperada::Codigo(codigo)),
        o_pino_prova_o_servidor: false,
        aceito: None,
    }
}

#[tokio::test]
async fn quem_recebe_liga_para_a_propria_sala_pelo_codigo_e_a_chave_fica_pinada() {
    let _vaga = vaga::minha();
    let endereco = sala().await;
    let pins = Arc::new(MemoryPinStore::new());
    let codigo = codigo_de(DONO);

    let enlace = Enlace::conectar(
        destino(endereco, codigo, NOME_DA_SALA),
        chave(DONO),
        Arc::clone(&pins) as Arc<dyn PinStore>,
    )
    .await
    .expect("quem recebe não entrou na própria sala pelo código");
    drop(enlace);

    assert_eq!(
        pins.pinned(&chave_do_pino(&codigo)),
        Some(seele_proto::transport::key_fingerprint(
            &chave(DONO).verifying_key().to_bytes()
        )),
        "depois de uma ligação completa, a chave inteira não ficou pinada sob @CÓDIGO"
    );
}

#[tokio::test]
async fn o_codigo_errado_e_recusado_dentro_do_tls_com_os_dois_codigos() {
    let _vaga = vaga::minha();
    let endereco = sala().await;
    let pins = Arc::new(MemoryPinStore::new());
    let esperado = codigo_de(7);

    let erro = Enlace::conectar(
        destino(endereco, esperado, NOME_DA_SALA),
        chave(DONO),
        Arc::clone(&pins) as Arc<dyn PinStore>,
    )
    .await
    .expect_err("o código errado entrou na sala de outra pessoa");
    assert_eq!(
        erro,
        ConnectError::CodigoNaoConfere(FalhaDoCodigo::CodigoNaoBate {
            esperado,
            veio: codigo_de(DONO),
        }),
        "o motivo não chegou tipado: quem liga leria uma falha genérica"
    );
    assert_eq!(
        pins.pinned(&chave_do_pino(&esperado)),
        None,
        "uma ligação recusada deixou pino"
    );
}

#[tokio::test]
async fn uma_chave_diferente_da_pinada_se_passou_pelo_dono_do_codigo() {
    let _vaga = vaga::minha();
    let endereco = sala().await;
    let pins = Arc::new(MemoryPinStore::new());
    let codigo = codigo_de(DONO);
    // O pino de outra chave com o mesmo código: a situação de quem achou uma
    // colisão nos 75 bits.
    pins.pin(
        &chave_do_pino(&codigo),
        seele_proto::transport::key_fingerprint(&chave(9).verifying_key().to_bytes()),
    );

    let erro = Enlace::conectar(
        destino(endereco, codigo, NOME_DA_SALA),
        chave(DONO),
        Arc::clone(&pins) as Arc<dyn PinStore>,
    )
    .await
    .expect_err("uma chave diferente da pinada passou só porque o código batia");
    assert_eq!(
        erro,
        ConnectError::CodigoNaoConfere(FalhaDoCodigo::SePassouPor { codigo })
    );
}

#[tokio::test]
async fn o_sni_e_o_nome_da_sala_mesmo_quando_o_destino_diz_o_codigo() {
    let _vaga = vaga::minha();
    // Um servidor QUIC cru com o certificado da sala, para ler o SNI que chegou.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let config =
        seele_server::tls::server_config(Identity::da_semente(&[DONO; 32]).unwrap()).unwrap();
    let ponta = quinn::Endpoint::server(config, SocketAddr::from(([127, 0, 0, 1], 0))).unwrap();
    let endereco = ponta.local_addr().unwrap();
    let sni = tokio::spawn(async move {
        let conexao = ponta.accept().await?.await.ok()?;
        conexao
            .handshake_data()?
            .downcast::<quinn::crypto::rustls::HandshakeData>()
            .ok()?
            .server_name
    });

    let codigo = codigo_de(DONO);
    // O destino pede o código como nome TLS: quem monta um destino errado não
    // pode fazer o código viajar legível no Initial do QUIC.
    let _ = tokio::time::timeout(
        Duration::from_secs(5),
        Enlace::conectar(
            destino(endereco, codigo, &codigo.to_string()),
            chave(DONO),
            Arc::new(MemoryPinStore::new()) as Arc<dyn PinStore>,
        ),
    )
    .await;

    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), sni)
            .await
            .unwrap()
            .unwrap()
            .as_deref(),
        Some(NOME_DA_SALA),
        "o SNI de uma ligação pelo código não foi o nome da sala"
    );
}
