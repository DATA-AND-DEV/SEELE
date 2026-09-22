//! Isolamento entre canais e permissões, provado **direto no protocolo**.
//!
//! A revisão da v15 pede exatamente isto no critério de saída 6: «Isolamento
//! entre canais e permissões testado diretamente no protocolo; não apenas por
//! botões ocultos.» Os testes abaixo falam com o servidor pela [`Client`], sem
//! passar por nenhuma casca, e cada um corresponde a um defeito reproduzido:
//!
//! - **R01** — o histórico chegava a uma conta autenticada sem `ReadChannel`;
//! - **R04** — uma mensagem para um canal inexistente derrubava o lote, e
//!   ninguém era avisado de nada;
//! - **R08** — revogar a escrita não alcançava uma sessão já conectada.
//!
//! # Os oráculos são os inversos dos das sondas da revisão
//!
//! Nas sondas, «passou» queria dizer «reproduzi o defeito». Aqui passa quando o
//! defeito não acontece. É a inversão que a própria revisão pede para as sondas
//! virarem testes permanentes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_core::{Client, MemoryPinStore, PinStore};
use seele_proto::control::{MessageRefusal, ServerMessage};
use seele_proto::ids::{ChannelId, ClientMessageId};
use seele_server::permissions::{Permissions, PERSON_ROLE};
use seele_server::persistence::{Location, Persistence};
use seele_server::{Daemon, ServerConfig};

mod vaga;

/// O canal que [`seele_server::seed`] deixa em todo servidor novo.
const GERAL: ChannelId = ChannelId(1);

/// Quanto se espera por um quadro antes de dizer que ele não veio.
///
/// Generoso de propósito: o escritor agrupa por ~200 ms, e um `assert` que
/// vence antes disso testaria o relógio e não o servidor.
const ESPERA: Duration = Duration::from_secs(3);

async fn subir(caminho: &std::path::Path) -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::File(caminho.to_path_buf()),
        ..ServerConfig::default()
    };
    let server = Arc::new(Daemon::bind(config).await?);
    let address = server.local_addr()?;
    let aceitando = Arc::clone(&server);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });
    Ok((address, server))
}

async fn conectar(address: SocketAddr, apelido: &str, semente: u8) -> Result<Client> {
    let pins: Arc<dyn PinStore> = Arc::new(MemoryPinStore::new());
    Ok(Client::connect(
        address,
        "localhost",
        "server-de-teste",
        apelido,
        &SigningKey::from_bytes(&[semente; 32]),
        pins,
        None,
        None,
    )
    .await?)
}

/// Tira o papel `Pessoa` de quem tem este apelido, direto no banco.
///
/// É o que a revisão fez para reproduzir os dois defeitos, e é o que um operador
/// faz hoje — a edição de papéis ainda não é uma jornada da interface.
fn tirar_o_papel(caminho: &std::path::Path, apelido: &str, semente: u8) -> Result<()> {
    let banco = Persistence::open(&Location::File(caminho.to_path_buf()))?;
    let permissions = Permissions::new(&banco);
    // Pela chave, e não pelo apelido: a chave **é** a identidade (ADR 0004), e
    // `register_or_find` é o único caminho público até a conta. Encontra a que
    // já existe — não cria uma segunda —, porque é a mesma chave com que o
    // cliente do teste se conectou.
    let chave = SigningKey::from_bytes(&[semente; 32]);
    let pessoa = permissions.register_or_find(chave.verifying_key().as_bytes(), apelido)?;
    permissions.revoke_role(pessoa.id, PERSON_ROLE)?;
    Ok(())
}

/// Espera até um quadro casar, ou devolve `None` ao fim de [`ESPERA`].
async fn esperar<T>(
    cliente: &mut Client,
    mut casa: impl FnMut(&ServerMessage) -> Option<T>,
) -> Option<T> {
    let prazo = tokio::time::Instant::now() + ESPERA;
    loop {
        let restante = prazo.saturating_duration_since(tokio::time::Instant::now());
        if restante.is_zero() {
            return None;
        }
        match tokio::time::timeout(restante, cliente.next_event()).await {
            Ok(Ok(evento)) => {
                if let Some(achado) = casa(&evento) {
                    return Some(achado);
                }
            }
            // Fluxo fechado ou prazo esgotado: em nenhum dos dois o quadro vem.
            Ok(Err(_)) | Err(_) => return None,
        }
    }
}

/// R01: uma conta autenticada sem `ReadChannel` não recebe o histórico.
///
/// **A ordem importa e é a da reprodução da revisão**: a pessoa entra, escreve,
/// perde o papel, **reconecta com a mesma chave** e pede o histórico. Sem o
/// conserto, a página chegava inteira — a conta continua válida, a assinatura
/// confere, e nada no caminho do `FetchHistory` perguntava a PERMISSIONS.
#[tokio::test]
async fn sem_leitura_o_historico_nao_chega() -> Result<()> {
    let _vaga = vaga::minha();
    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");
    let (endereco, servidor) = subir(&banco).await?;

    // Quem hospeda entra primeiro e assume o comando — sem isto, a conta de
    // baixo seria a Comandante e passaria por qualquer guarda.
    let mut anfitriao = conectar(endereco, "quem-hospeda", 1).await?;
    anfitriao.join_channel(GERAL).await?;
    anfitriao
        .send_message(GERAL, "há conversa aqui", ClientMessageId(1))
        .await?;
    let gravada = esperar(&mut anfitriao, |evento| {
        matches!(evento, ServerMessage::MessageReceived { .. }).then_some(())
    })
    .await;
    assert!(gravada.is_some(), "a mensagem de partida não foi gravada");

    let mut visitante = conectar(endereco, "visitante", 2).await?;
    visitante.join_channel(GERAL).await?;
    drop(visitante);

    tirar_o_papel(&banco, "visitante", 2)?;

    let mut visitante = conectar(endereco, "visitante", 2).await?;
    assert!(
        !visitante
            .session()
            .permissions
            .contains(&seele_proto::control::Permission::ReadChannel),
        "a sessão ainda declara leitura: a reprodução não montou a condição"
    );

    visitante.fetch_history(GERAL, None, 50).await?;
    let recebeu = esperar(&mut visitante, |evento| match evento {
        ServerMessage::MessageReceived { body, .. } => Some(body.clone()),
        _ => None,
    })
    .await;
    assert_eq!(
        recebeu, None,
        "o histórico chegou a uma conta autenticada sem ReadChannel"
    );

    // E a recusa é **visível**: sem ela, quem pediu lê silêncio como travamento.
    let mut visitante2 = conectar(endereco, "visitante", 2).await?;
    visitante2.fetch_history(GERAL, None, 50).await?;
    let alerta = esperar(&mut visitante2, |evento| match evento {
        ServerMessage::Alert { reason, .. } => Some(*reason),
        _ => None,
    })
    .await;
    assert_eq!(
        alerta,
        Some(seele_proto::control::AlertReason::PermissionDenied),
        "a recusa não chegou: o produto sabe e não conta"
    );

    servidor.shutdown();
    Ok(())
}

/// R08: revogar a escrita alcança uma sessão que já está de pé.
///
/// A conexão **não** reconecta. É o ponto: `may_write` era calculado no aperto
/// de mão e consultado para sempre, então esta mesma sessão publicava depois de
/// perder o papel.
#[tokio::test]
async fn revogar_a_escrita_morde_a_sessao_viva() -> Result<()> {
    let _vaga = vaga::minha();
    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");
    let (endereco, servidor) = subir(&banco).await?;

    let _anfitriao = conectar(endereco, "quem-hospeda", 1).await?;

    let mut pessoa = conectar(endereco, "pessoa", 2).await?;
    pessoa.join_channel(GERAL).await?;
    pessoa
        .send_message(GERAL, "antes", ClientMessageId(1))
        .await?;
    let antes = esperar(&mut pessoa, |evento| match evento {
        ServerMessage::MessageReceived { body, .. } => Some(body.clone()),
        _ => None,
    })
    .await;
    assert_eq!(
        antes.as_deref(),
        Some("antes"),
        "a mensagem de partida não foi gravada"
    );

    tirar_o_papel(&banco, "pessoa", 2)?;

    pessoa
        .send_message(GERAL, "depois", ClientMessageId(2))
        .await?;
    let veredito = esperar(&mut pessoa, |evento| match evento {
        ServerMessage::MessageReceived { body, .. } if body == "depois" => Some(Ok(())),
        ServerMessage::MessageRejected { reason, .. } => Some(Err(*reason)),
        _ => None,
    })
    .await;
    assert_eq!(
        veredito,
        Some(Err(MessageRefusal::PermissionDenied)),
        "a sessão publicou com a escrita já revogada, ou a recusa não voltou \
         identificada"
    );

    servidor.shutdown();
    Ok(())
}

/// R04: um destino inválido de uma pessoa não apaga a mensagem de outra.
///
/// As duas conexões escrevem dentro da mesma janela de agrupamento, que é a
/// condição do defeito: um lote só, uma transação só, e a chave estrangeira
/// derrubando tudo.
///
/// Hoje o `SendMessage` recusa o canal inexistente **antes** de enfileirar, e o
/// `append_batch` isola o registro caso ele chegue de outro caminho. Este teste
/// prova a jornada inteira; o savepoint tem o teste dele em
/// `persistence::messages`.
#[tokio::test]
async fn um_destino_invalido_nao_leva_a_mensagem_de_outra_pessoa() -> Result<()> {
    let _vaga = vaga::minha();
    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");
    let (endereco, servidor) = subir(&banco).await?;

    let mut anfitriao = conectar(endereco, "quem-hospeda", 1).await?;
    anfitriao.join_channel(GERAL).await?;
    let mut outra = conectar(endereco, "outra", 2).await?;
    outra.join_channel(GERAL).await?;

    // O destino inventado primeiro, a mensagem boa em seguida, as duas dentro
    // da mesma janela de ~200 ms.
    outra
        .send_message(
            ChannelId(4_040),
            "para um canal que não existe",
            ClientMessageId(1),
        )
        .await?;
    anfitriao
        .send_message(GERAL, "esta tem de ficar", ClientMessageId(1))
        .await?;

    let boa = esperar(&mut anfitriao, |evento| match evento {
        ServerMessage::MessageReceived { body, .. } => Some(body.clone()),
        _ => None,
    })
    .await;
    assert_eq!(
        boa.as_deref(),
        Some("esta tem de ficar"),
        "a mensagem válida de uma pessoa se perdeu por causa do destino \
         inválido de outra"
    );

    // E quem mandou o destino inválido é avisado, com a chave dele.
    let recusa = esperar(&mut outra, |evento| match evento {
        ServerMessage::MessageRejected {
            channel,
            client_message_id,
            reason,
        } => Some((*channel, *client_message_id, *reason)),
        _ => None,
    })
    .await;
    assert_eq!(
        recusa,
        Some((
            ChannelId(4_040),
            ClientMessageId(1),
            MessageRefusal::NoSuchChannel
        )),
        "o remetente não foi avisado de qual mensagem não entrou"
    );

    servidor.shutdown();
    Ok(())
}

/// Assinar um canal que não se pode ler é recusado, e a recusa é visível.
///
/// `JoinChannel` aceitava qualquer número. Não era só permissivo: como a difusão
/// conferia apenas a lista de assinaturas, aceitar a assinatura **era** liberar
/// a difusão.
#[tokio::test]
async fn assinar_sem_leitura_e_recusado() -> Result<()> {
    let _vaga = vaga::minha();
    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");
    let (endereco, servidor) = subir(&banco).await?;

    let _anfitriao = conectar(endereco, "quem-hospeda", 1).await?;
    let mut visitante = conectar(endereco, "visitante", 2).await?;
    drop(visitante);
    tirar_o_papel(&banco, "visitante", 2)?;
    visitante = conectar(endereco, "visitante", 2).await?;

    visitante.join_channel(GERAL).await?;
    let alerta = esperar(&mut visitante, |evento| match evento {
        ServerMessage::Alert { reason, .. } => Some(*reason),
        _ => None,
    })
    .await;
    assert_eq!(
        alerta,
        Some(seele_proto::control::AlertReason::PermissionDenied),
        "assinar um canal fechado passou em silêncio"
    );

    servidor.shutdown();
    Ok(())
}

/// Um canal que a pessoa não pode ler não chega pela difusão, mesmo assinado.
///
/// A outra metade do teste acima: aqui a assinatura é feita **enquanto** a
/// permissão existe, e o papel é tirado depois. A lista de assinaturas continua
/// com o canal, e é a conferência por evento que tem de recusar — é a mesma
/// leitura viva que conserta o R08 para o lado da leitura.
#[tokio::test]
async fn a_difusao_para_quando_a_leitura_e_revogada() -> Result<()> {
    let _vaga = vaga::minha();
    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");
    let (endereco, servidor) = subir(&banco).await?;

    let mut anfitriao = conectar(endereco, "quem-hospeda", 1).await?;
    anfitriao.join_channel(GERAL).await?;
    let mut ouvinte = conectar(endereco, "ouvinte", 2).await?;
    ouvinte.join_channel(GERAL).await?;

    anfitriao
        .send_message(GERAL, "com permissão", ClientMessageId(1))
        .await?;
    let chegou = esperar(&mut ouvinte, |evento| match evento {
        ServerMessage::MessageReceived { body, .. } => Some(body.clone()),
        _ => None,
    })
    .await;
    assert_eq!(
        chegou.as_deref(),
        Some("com permissão"),
        "a difusão não alcançava quem tinha permissão: o guarda ficou fechado \
         para todo mundo"
    );

    tirar_o_papel(&banco, "ouvinte", 2)?;

    anfitriao
        .send_message(GERAL, "sem permissão", ClientMessageId(2))
        .await?;
    let vazou = esperar(&mut ouvinte, |evento| match evento {
        ServerMessage::MessageReceived { body, .. } if body == "sem permissão" => Some(()),
        _ => None,
    })
    .await;
    assert!(
        vazou.is_none(),
        "a difusão continuou entregando a um canal cuja leitura foi revogada"
    );

    servidor.shutdown();
    Ok(())
}
