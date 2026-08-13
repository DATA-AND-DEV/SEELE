//! Uma recusa tem que chegar como recusa.
//!
//! `specs/02-protocolo.md`: a razão de uma falha de handshake é enumerada e
//! específica, "nunca genérica". Isto testa a ponta que importa — a que a
//! pessoa lê.
//!
//! Encontrado no uso, não na leitura: tentei entrar com um apelido que já
//! pertencia a outra identidade, o Dogma recusou dizendo exatamente isso no
//! log, e o cliente mostrou "NÃO FOI POSSÍVEL ALCANÇAR O DOGMA". Passei um bom
//! tempo procurando problema de rede que não existia.

#![allow(clippy::expect_used)]

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use seele_core::enlace::{Destino, Enlace};
use seele_core::{Client, ConnectError, MemoryPinStore, PinStore};
use seele_server::casper::Location;
use seele_server::{DogmaConfig, Server};

async fn dogma() -> Result<(SocketAddr, Arc<Server>)> {
    let config = DogmaConfig {
        name: "Terceira Tóquio".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        ..DogmaConfig::default()
    };
    let servidor = Arc::new(Server::bind(config).await?);
    let endereco = servidor.local_addr()?;
    let aceitando = Arc::clone(&servidor);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });
    Ok((endereco, servidor))
}

async fn entrar(endereco: SocketAddr, semente: u8) -> Result<Client, ConnectError> {
    Client::connect(
        endereco,
        "localhost",
        &endereco.to_string(),
        "ayanami",
        &ed25519_dalek::SigningKey::from_bytes(&[semente; 32]),
        Arc::new(MemoryPinStore::new()),
        None,
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn um_apelido_de_outro_dono_e_recusa_e_nao_rede() -> Result<()> {
    let (endereco, servidor) = dogma().await?;

    // O primeiro reivindica o apelido. ADR 0017: CASPER prende o apelido à
    // identidade que o pegou primeiro.
    let primeiro = entrar(endereco, 1).await.expect("o primeiro deve entrar");
    drop(primeiro);

    // O segundo tem outra chave e quer o mesmo nome.
    let Err(erro) = entrar(endereco, 2).await else {
        panic!("o segundo entrou com um apelido que não é dele");
    };

    assert!(
        matches!(erro, ConnectError::Refused { .. }),
        "o Dogma recusou com um motivo e o cliente entendeu «{erro:?}».\n\
         Quem lê isso vai procurar problema de rede que não existe."
    );

    servidor.shutdown();
    Ok(())
}

/// Um aperto de mão recusado não pode deixar a chave do servidor fixada.
///
/// O `TofuVerifier` fixa dentro do retorno de chamada do TLS, que termina bem
/// antes do aperto de mão — a recusa vem depois, com o pin já escrito. Deixá-lo
/// para trás desarma a conferência do ADR 0006 naquele endereço: na tentativa
/// seguinte a decisão seria `Matches`, e um `fp=` que **não** confere viraria um
/// aviso em vez de uma recusa, sem ninguém ter decidido isso.
#[tokio::test(flavor = "multi_thread")]
async fn uma_recusa_depois_do_tls_nao_deixa_a_chave_fixada() -> Result<()> {
    let (endereco, servidor) = dogma().await?;

    let primeiro = entrar(endereco, 1).await.expect("o primeiro deve entrar");
    drop(primeiro);

    let loja = Arc::new(MemoryPinStore::new());
    let chave_do_pin = endereco.to_string();
    let destino = Destino {
        servidor: endereco,
        nome_tls: "localhost".into(),
        chave_do_pin: chave_do_pin.clone(),
        apelido: "ayanami".into(),
        segredo: None,
        impressao_esperada: None,
    };

    // Mesma situação do teste acima: outra identidade pedindo um apelido que já
    // tem dono. O TLS passa, o aperto de mão não.
    let erro = Enlace::conectar(
        destino,
        ed25519_dalek::SigningKey::from_bytes(&[2; 32]),
        Arc::clone(&loja) as Arc<dyn PinStore>,
    )
    .await
    .expect_err("o segundo entrou com um apelido que não é dele");
    assert!(matches!(erro, ConnectError::Refused { .. }), "«{erro:?}»");

    assert_eq!(
        loja.pinned(&chave_do_pin),
        None,
        "o aperto de mão falhou e o pin ficou para trás"
    );

    servidor.shutdown();
    Ok(())
}
