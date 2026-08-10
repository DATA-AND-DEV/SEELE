//! Os pontos de segurança que valem para um Dogma exposto a outras pessoas.
//!
//! Cada teste aqui corresponde a algo que estava errado ou ausente até M5, e
//! todos foram escritos depois de verificar o comportamento antigo — não são
//! confirmações de que o código faz o que o código faz.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_core::{Client, MemoryPinStore, PinDecision, PinStore};
use seele_proto::ids::CageId;
use seele_server::casper::{Casper, Location};
use seele_server::{admissao, DogmaConfig, Server};

const CAGE: CageId = CageId(1);

async fn subir(caminho: &std::path::Path) -> Result<(SocketAddr, Arc<Server>)> {
    let config = DogmaConfig {
        name: "Terceira Tóquio".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::File(caminho.to_path_buf()),
        ..DogmaConfig::default()
    };
    let server = Arc::new(Server::bind(config).await?);
    let address = server.local_addr()?;
    let aceitando = Arc::clone(&server);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });
    Ok((address, server))
}

async fn conectar(
    address: SocketAddr,
    apelido: &str,
    semente: u8,
    pins: Arc<dyn PinStore>,
    segredo: Option<&str>,
) -> Result<Client, seele_core::ConnectError> {
    Client::connect(
        address,
        "localhost",
        "dogma-de-teste",
        apelido,
        &SigningKey::from_bytes(&[semente; 32]),
        pins,
        segredo,
    )
    .await
}

/// Reiniciar o Dogma não pode expulsar quem já se conectou.
///
/// O certificado era gerado a cada boot, então todo reinício trocava a chave e
/// todo cliente via o alerta bloqueante do ADR 0003 — o aviso reservado para
/// ataque, disparado por um reinício de rotina. Isso não só quebra a conexão:
/// ensina a ignorar o único aviso que não pode ser ignorado.
#[tokio::test]
async fn reiniciar_o_dogma_nao_troca_a_chave() -> Result<()> {
    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");

    let (endereco, servidor) = subir(&banco).await?;
    let impressao_inicial = servidor.fingerprint().to_owned();

    // Um piloto se conecta e fixa a chave.
    let pins: Arc<dyn PinStore> = Arc::new(MemoryPinStore::new());
    let cliente = conectar(endereco, "ayanami", 1, Arc::clone(&pins), None).await?;
    assert!(matches!(
        cliente.pin_decision(),
        PinDecision::FirstContact { .. }
    ));
    drop(cliente);
    servidor.shutdown();
    drop(servidor);

    // O operador reinicia. Mesmo banco, processo novo.
    let (endereco, servidor) = subir(&banco).await?;
    assert_eq!(
        servidor.fingerprint(),
        impressao_inicial,
        "o Dogma trocou de identidade ao reiniciar"
    );

    let cliente = conectar(endereco, "ayanami", 1, pins, None)
        .await
        .map_err(|erro| anyhow::anyhow!("o piloto foi recusado após um reinício: {erro:?}"))?;
    assert_eq!(cliente.pin_decision(), &PinDecision::Matches);

    servidor.shutdown();
    Ok(())
}

/// Um Dogma com senha recusa quem não a tem.
#[tokio::test]
async fn a_senha_do_dogma_fecha_a_porta() -> Result<()> {
    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");

    // O operador define a senha com o Dogma parado.
    {
        let mut casper = Casper::open(&Location::File(banco.clone()))?;
        admissao::definir_senha(&mut casper, Some("terceiro impacto"))?;
    }

    let (endereco, servidor) = subir(&banco).await?;

    let sem_segredo = conectar(
        endereco,
        "estranho",
        9,
        Arc::new(MemoryPinStore::new()),
        None,
    )
    .await;
    assert!(sem_segredo.is_err(), "entrou sem apresentar a senha");

    let errada = conectar(
        endereco,
        "estranho",
        9,
        Arc::new(MemoryPinStore::new()),
        Some("chute"),
    )
    .await;
    assert!(errada.is_err(), "entrou com a senha errada");

    let certa = conectar(
        endereco,
        "ayanami",
        1,
        Arc::new(MemoryPinStore::new()),
        Some("terceiro impacto"),
    )
    .await;
    assert!(
        certa.is_ok(),
        "quem sabe a senha não entrou: {:?}",
        certa.err()
    );

    servidor.shutdown();
    Ok(())
}

/// Um convite vale uma vez, e é isso que o torna seguro num link.
#[tokio::test]
async fn um_convite_serve_a_uma_pessoa_so() -> Result<()> {
    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");

    let token = {
        let mut casper = Casper::open(&Location::File(banco.clone()))?;
        admissao::criar_convite(&mut casper, "ayanami")?
    };

    let (endereco, servidor) = subir(&banco).await?;

    let primeiro = conectar(
        endereco,
        "ayanami",
        1,
        Arc::new(MemoryPinStore::new()),
        Some(&token),
    )
    .await;
    assert!(
        primeiro.is_ok(),
        "o convidado não entrou: {:?}",
        primeiro.err()
    );

    // O mesmo link, repassado adiante.
    let segundo = conectar(
        endereco,
        "penetra",
        2,
        Arc::new(MemoryPinStore::new()),
        Some(&token),
    )
    .await;
    assert!(segundo.is_err(), "o convite foi usado duas vezes");

    servidor.shutdown();
    Ok(())
}

/// A senha de um Cage é conferida, e não só anunciada.
#[tokio::test]
async fn a_senha_do_cage_e_conferida() -> Result<()> {
    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");

    {
        // O Dogma semeia o Cage ao subir, então sobe uma vez antes de trancar.
        let (_, servidor) = subir(&banco).await?;
        servidor.shutdown();
        let mut casper = Casper::open(&Location::File(banco.clone()))?;
        admissao::definir_senha_cage(&mut casper, CAGE, Some("geofront"))?;
    }

    let (endereco, servidor) = subir(&banco).await?;
    let mut cliente = conectar(
        endereco,
        "ayanami",
        1,
        Arc::new(MemoryPinStore::new()),
        None,
    )
    .await?;

    // A entrada sem senha é recusada com um alerta, não com uma queda: o Cage
    // é um cômodo, e errar a senha dele não derruba a sessão.
    cliente.insert_plug(CAGE).await?;
    let alerta = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        aguardar_recusa(&mut cliente),
    )
    .await;
    assert!(
        alerta.is_ok(),
        "entrar num Cage trancado sem senha não foi recusado"
    );

    servidor.shutdown();
    Ok(())
}

async fn aguardar_recusa(cliente: &mut Client) {
    while let Ok(mensagem) = cliente.next_event().await {
        if matches!(
            mensagem,
            seele_proto::ServerMessage::Alert {
                reason: seele_proto::control::AlertReason::CageEntryRefused,
                ..
            }
        ) {
            return;
        }
    }
}

/// Um Dogma sem configuração continua aberto — e isso é escolha, não descuido.
#[tokio::test]
async fn um_dogma_novo_aceita_qualquer_um() -> Result<()> {
    let pasta = tempfile::tempdir()?;
    let (endereco, servidor) = subir(&pasta.path().join("seele.db")).await?;

    let cliente = conectar(
        endereco,
        "qualquer",
        7,
        Arc::new(MemoryPinStore::new()),
        None,
    )
    .await;
    assert!(
        cliente.is_ok(),
        "o padrão deixou de ser aberto: {:?}",
        cliente.err()
    );

    servidor.shutdown();
    Ok(())
}
