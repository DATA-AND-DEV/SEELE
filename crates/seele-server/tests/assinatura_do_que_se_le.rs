//! **Quem pode ler um canal recebe o que se diz nele, sem ter pedido** — desde
//! o protocolo 13, e só para quem o negociou.
//!
//! # O defeito
//!
//! A conversa de um canal só era difundida a quem tivesse mandado
//! `JoinChannel`, e o cliente só o manda ao abrir o canal. As não lidas e as
//! menções por canal contam o que chega: num canal que a pessoa ainda não
//! abriu nesta conexão não chegava nada, e depois de uma reconexão só o canal
//! aberto voltava a ser assinado.
//!
//! # O que cada guarda prende
//!
//! - **Sem assinar**: um par 13 que nunca mandou `JoinChannel` recebe a
//!   mensagem, a edição e a remoção de um canal que ele pode ler — e de novo
//!   numa conexão nova da mesma pessoa, que é o que uma reconexão é.
//! - **A permissão continua sendo do servidor, e de agora**: o canal que a
//!   pessoa não lê não chega; chega quando o cargo é dado com a sessão de pé,
//!   e para de chegar quando ele é tirado.
//! - **O canal criado depois da entrada** chega sem pedido nenhum.
//! - **Um par 12 não vê diferença**: só recebe o canal que assinou.
//! - **Abrir um canal que não se lê continua recusado, à vista** (R01): o
//!   `JoinChannel` responde `PermissionDenied`, e é por isso que o cliente não
//!   pode sair assinando a lista inteira.
//!
//! Provado contra a regressão: com `ASSINA_O_QUE_LE_DESDE` em 255 (ninguém
//! assina sem pedir), os quatro primeiros reprovam.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use ed25519_dalek::{Signer, SigningKey};
use seele_proto::control::{AlertReason, ClientMessage, ServerMessage};
use seele_proto::ids::{ChannelId, ClientMessageId, MessageId, PersonId};
use seele_server::permissions::{Permissions, COMMANDER_ROLE, PERSON_ROLE};
use seele_server::persistence::{Location, Persistence};
use seele_server::{frame, Daemon, ServerConfig};

mod comum;

const COMANDANTE: u8 = 71;
const PESSOA: u8 = 72;
const OUTRA: u8 = 73;

/// O canal que qualquer pessoa lê e escreve.
const ABERTO: ChannelId = ChannelId(1);
/// O canal semeado fechado para quem não é Comandante.
const FECHADO: ChannelId = ChannelId(2);

struct Casa {
    endereco: SocketAddr,
    daemon: Arc<Daemon>,
    banco: std::path::PathBuf,
    outra: PersonId,
    _pasta: tempfile::TempDir,
}

fn chave(semente: u8) -> SigningKey {
    SigningKey::from_bytes(&[semente; 32])
}

/// Sobe um servidor com três contas e dois canais, o segundo fechado.
async fn casa() -> Result<Casa> {
    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");
    let outra = {
        let persistence = Persistence::open(&Location::File(banco.clone()))?;
        let permissions = Permissions::new(&persistence);
        let conta = |semente: u8, apelido: &str, cargo| {
            let pessoa = permissions
                .register_or_find(&chave(semente).verifying_key().to_bytes(), apelido)
                .expect("conta")
                .id;
            if cargo != PERSON_ROLE {
                permissions.revoke_role(pessoa, PERSON_ROLE).expect("tirar");
                permissions.grant_role(pessoa, cargo).expect("dar");
            }
            pessoa
        };
        conta(COMANDANTE, "comandante", COMMANDER_ROLE);
        conta(PESSOA, "pessoa", PERSON_ROLE);
        let outra = conta(OUTRA, "outra", PERSON_ROLE);

        let sql = rusqlite::Connection::open(&banco)?;
        sql.execute_batch(
            "INSERT OR IGNORE INTO channels (id, name) VALUES (1, 'aberto');
             INSERT INTO channels (id, name) VALUES (2, 'fechado');",
        )?;
        sql.execute(
            "UPDATE channels SET minimum_read_role = ?1, minimum_write_role = ?1 WHERE id = 2",
            [i64::from(COMMANDER_ROLE.get())],
        )?;
        outra
    };

    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::File(banco.clone()),
        ..ServerConfig::default()
    };
    let daemon = Arc::new(Daemon::bind(config).await?);
    let endereco = daemon.local_addr()?;
    let atendendo = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = atendendo.run().await;
    });
    Ok(Casa {
        endereco,
        daemon,
        banco,
        outra,
        _pasta: pasta,
    })
}

/// Dá ou tira um cargo pelo banco, com o servidor de pé: não há verbo de
/// protocolo que escreva o papel mínimo de um canal, e é a leitura **de
/// agora** que se quer provar.
fn cargo(banco: &Path, pessoa: PersonId, dar: bool) {
    let persistence = Persistence::open(&Location::File(banco.to_path_buf())).expect("o banco");
    let permissions = Permissions::new(&persistence);
    if dar {
        permissions
            .grant_role(pessoa, COMMANDER_ROLE)
            .expect("dar o cargo");
    } else {
        permissions
            .revoke_role(pessoa, COMMANDER_ROLE)
            .expect("tirar o cargo");
    }
}

/// Um par cru que já entrou.
struct Par {
    _conexao: seele_enlace::Conexao,
    envio: seele_enlace::Envio,
    recebe: seele_enlace::Recebe,
    /// Os canais que o `Session` listou a este par.
    canais: Vec<ChannelId>,
}

/// Entra falando até `maxima`: 13 é esta build, 12 é a v0.19.6 publicada.
async fn entrar_falando(endereco: SocketAddr, semente: u8, maxima: u8) -> Result<Par> {
    let comum::Ligado {
        ponta,
        conexao,
        mut envio,
        mut recebe,
    } = comum::ligar(endereco).await?;
    std::mem::forget(ponta);
    let chave = chave(semente);
    frame::write(
        &mut envio,
        &ClientMessage::Hello {
            faixa: seele_proto::version::Faixa {
                minima: seele_proto::version::VERSAO_MINIMA,
                maxima,
            },
            client: "par-cru".into(),
            nickname: format!("par{semente}"),
            public_key: chave.verifying_key().to_bytes().to_vec(),
            join_secret: None,
            material_e2ee: None,
            apis_de_mod: seele_proto::mods::APIS_ACEITAS.to_vec(),
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
                .sign(&comum::prova_amarrada(&conexao, &nonce))
                .to_bytes()
                .to_vec(),
        },
    )
    .await?;
    let canais = match frame::read::<ServerMessage>(&mut recebe).await? {
        ServerMessage::Session { channels, .. } => channels.iter().map(|canal| canal.id).collect(),
        outra => anyhow::bail!("o servidor não mandou Session, e sim {outra:?}"),
    };
    Ok(Par {
        _conexao: conexao,
        envio,
        recebe,
        canais,
    })
}

async fn entrar(endereco: SocketAddr, semente: u8) -> Result<Par> {
    entrar_falando(endereco, semente, seele_proto::PROTOCOL_VERSION).await
}

/// O primeiro quadro que `serve` aceitar, em até 5 s.
async fn esperar<T>(
    par: &mut Par,
    mut serve: impl FnMut(&ServerMessage) -> Option<T>,
) -> Option<T> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match frame::read::<ServerMessage>(&mut par.recebe).await {
                Ok(quadro) => {
                    if let Some(achado) = serve(&quadro) {
                        return Some(achado);
                    }
                }
                Err(_) => return None,
            }
        }
    })
    .await
    .ok()
    .flatten()
}

/// Assina o canal e espera a assinatura valer (o `Pong` vem depois dela).
/// Devolve o canal e o corpo de cada mensagem que chegou **antes** do `Pong`:
/// quem lê até ele não pode jogar fora o que veio no caminho.
async fn assinar(par: &mut Par, canal: ChannelId) -> Vec<(ChannelId, String)> {
    frame::write(
        &mut par.envio,
        &ClientMessage::JoinChannel { channel: canal },
    )
    .await
    .expect("assinar");
    frame::write(&mut par.envio, &ClientMessage::Ping { timestamp: 1 })
        .await
        .expect("o Ping");
    let mut antes = Vec::new();
    let voltou = esperar(par, |quadro| {
        if let ServerMessage::MessageReceived { channel, body, .. } = quadro {
            antes.push((*channel, body.clone()));
        }
        matches!(quadro, ServerMessage::Pong { timestamp: 1 }).then_some(())
    })
    .await;
    assert!(voltou.is_some(), "o `Pong` da assinatura não voltou");
    antes
}

/// Diz `corpo` no canal e devolve o id que o servidor deu. Quem diz já
/// assinou o canal: a confirmação é o próprio eco.
async fn dizer(par: &mut Par, canal: ChannelId, chave: u64, corpo: &str) -> MessageId {
    frame::write(
        &mut par.envio,
        &ClientMessage::SendMessage {
            channel: canal,
            body: corpo.into(),
            replies_to: None,
            client_message_id: ClientMessageId(chave),
        },
    )
    .await
    .expect("mandar");
    esperar(par, |quadro| match quadro {
        ServerMessage::MessageReceived {
            id,
            client_message_id: Some(eco),
            ..
        } if eco.get() == chave => Some(*id),
        _ => None,
    })
    .await
    .expect("a mensagem não foi confirmada")
}

/// O canal e o corpo de cada `MessageReceived` que chegar até o de corpo
/// `ate`, inclusive. O barramento do servidor é um só e em ordem: o que foi
/// dito antes de `ate` e não está aqui, não foi mandado a este par.
async fn recebidas_ate(par: &mut Par, ate: &str) -> Vec<(ChannelId, String)> {
    let mut chegou = Vec::new();
    let fim = esperar(par, |quadro| {
        if let ServerMessage::MessageReceived { channel, body, .. } = quadro {
            chegou.push((*channel, body.clone()));
            if body == ate {
                return Some(());
            }
        }
        None
    })
    .await;
    assert!(
        fim.is_some(),
        "«{ate}» não chegou a este par; chegou: {chegou:?}"
    );
    chegou
}

/// **Um par 13 que nunca assinou nada recebe o que se diz no canal que ele
/// pode ler**: a mensagem, a edição e a remoção. E uma conexão nova da mesma
/// pessoa — o que uma reconexão é, para o servidor — nasce assinada.
#[tokio::test(flavor = "multi_thread")]
async fn quem_le_o_canal_recebe_sem_assinar_e_de_novo_na_conexao_seguinte() -> Result<()> {
    let casa = casa().await?;
    let mut pessoa = entrar(casa.endereco, PESSOA).await?;
    assinar(&mut pessoa, ABERTO).await;
    // A outra entra e não manda `JoinChannel` nenhum.
    let mut outra = entrar(casa.endereco, OUTRA).await?;

    let id = dizer(&mut pessoa, ABERTO, 1, "num canal que ela nunca abriu").await;
    assert_eq!(
        recebidas_ate(&mut outra, "num canal que ela nunca abriu").await,
        vec![(ABERTO, "num canal que ela nunca abriu".to_owned())],
    );

    frame::write(
        &mut pessoa.envio,
        &ClientMessage::EditMessage {
            id,
            body: "corrigida".into(),
        },
    )
    .await?;
    let editada = esperar(&mut outra, |quadro| match quadro {
        ServerMessage::MessageEdited { channel, id, body } => Some((*channel, *id, body.clone())),
        _ => None,
    })
    .await;
    assert_eq!(
        editada,
        Some((ABERTO, id, "corrigida".to_owned())),
        "a edição não chegou a quem lê o canal sem tê-lo aberto"
    );

    frame::write(
        &mut pessoa.envio,
        &ClientMessage::RemoveMessage { message: id },
    )
    .await?;
    let removida = esperar(&mut outra, |quadro| match quadro {
        ServerMessage::MessageRemoved { channel, id } => Some((*channel, *id)),
        _ => None,
    })
    .await;
    assert_eq!(removida, Some((ABERTO, id)), "a remoção não chegou");

    // **A reconexão**: a conexão cai, e a seguinte não reassina nada.
    drop(outra);
    let mut de_volta = entrar(casa.endereco, OUTRA).await?;
    dizer(&mut pessoa, ABERTO, 2, "depois da volta").await;
    assert_eq!(
        recebidas_ate(&mut de_volta, "depois da volta").await,
        vec![(ABERTO, "depois da volta".to_owned())],
        "a conexão nova da mesma pessoa não recebeu o canal sem reassinar"
    );

    casa.daemon.shutdown();
    Ok(())
}

/// **A permissão é do servidor, e é a de agora.** O canal fechado não chega a
/// quem não o lê; passa a chegar quando o cargo é dado com a sessão de pé, e
/// para quando ele é tirado. Nenhum pedido do cliente no meio.
#[tokio::test(flavor = "multi_thread")]
async fn o_canal_fechado_so_chega_enquanto_a_pessoa_pode_le_lo() -> Result<()> {
    let casa = casa().await?;
    let mut comandante = entrar(casa.endereco, COMANDANTE).await?;
    assinar(&mut comandante, ABERTO).await;
    assinar(&mut comandante, FECHADO).await;
    let mut outra = entrar(casa.endereco, OUTRA).await?;

    dizer(&mut comandante, FECHADO, 1, "só para quem comanda").await;
    dizer(&mut comandante, ABERTO, 2, "para todo mundo").await;
    assert_eq!(
        recebidas_ate(&mut outra, "para todo mundo").await,
        vec![(ABERTO, "para todo mundo".to_owned())],
        "o canal fechado chegou a quem não pode lê-lo"
    );

    cargo(&casa.banco, casa.outra, true);
    dizer(&mut comandante, FECHADO, 3, "agora ela lê").await;
    assert_eq!(
        recebidas_ate(&mut outra, "agora ela lê").await,
        vec![(FECHADO, "agora ela lê".to_owned())],
        "o cargo dado com a sessão de pé não abriu o canal"
    );

    cargo(&casa.banco, casa.outra, false);
    dizer(&mut comandante, FECHADO, 4, "e deixou de ler").await;
    dizer(&mut comandante, ABERTO, 5, "o aberto continua").await;
    assert_eq!(
        recebidas_ate(&mut outra, "o aberto continua").await,
        vec![(ABERTO, "o aberto continua".to_owned())],
        "o cargo tirado com a sessão de pé não fechou o canal"
    );

    casa.daemon.shutdown();
    Ok(())
}

/// **O canal criado depois da entrada** chega a quem já estava, sem pedido.
#[tokio::test(flavor = "multi_thread")]
async fn o_canal_criado_depois_da_entrada_chega_sem_pedido() -> Result<()> {
    let casa = casa().await?;
    let mut outra = entrar(casa.endereco, OUTRA).await?;
    let mut comandante = entrar(casa.endereco, COMANDANTE).await?;

    frame::write(
        &mut comandante.envio,
        &ClientMessage::CreateChannel {
            name: "novo".into(),
        },
    )
    .await?;
    let criado = |quadro: &ServerMessage| match quadro {
        ServerMessage::ChannelCreated { channel } => Some(channel.id),
        _ => None,
    };
    let novo = esperar(&mut comandante, criado)
        .await
        .expect("o canal não foi criado");
    assert_eq!(
        esperar(&mut outra, criado).await,
        Some(novo),
        "a outra não soube do canal novo"
    );

    assinar(&mut comandante, novo).await;
    dizer(&mut comandante, novo, 1, "no canal que nasceu agora").await;
    assert_eq!(
        recebidas_ate(&mut outra, "no canal que nasceu agora").await,
        vec![(novo, "no canal que nasceu agora".to_owned())],
    );

    casa.daemon.shutdown();
    Ok(())
}

/// **Um par 12 não vê diferença**: recebe o canal que assinou, e só depois de
/// assinar. É o que um cliente da 0.19 conta e o que a placa de «mensagem
/// nova» dele espera.
#[tokio::test(flavor = "multi_thread")]
async fn um_par_doze_so_recebe_o_canal_que_assinou() -> Result<()> {
    let casa = casa().await?;
    let mut pessoa = entrar(casa.endereco, PESSOA).await?;
    assinar(&mut pessoa, ABERTO).await;
    let mut antigo = entrar_falando(casa.endereco, OUTRA, 12).await?;

    dizer(&mut pessoa, ABERTO, 1, "antes de ele assinar").await;
    // O que foi dito antes da assinatura não chegou no caminho até ela.
    assert_eq!(
        assinar(&mut antigo, ABERTO).await,
        Vec::new(),
        "um par 12 recebeu um canal que ainda não tinha assinado"
    );
    dizer(&mut pessoa, ABERTO, 2, "depois de ele assinar").await;
    assert_eq!(
        recebidas_ate(&mut antigo, "depois de ele assinar").await,
        vec![(ABERTO, "depois de ele assinar".to_owned())],
        "um par 12 recebeu um canal que não assinou"
    );

    casa.daemon.shutdown();
    Ok(())
}

/// **Abrir um canal que não se lê continua recusado, e à vista** (R01). É a
/// razão de a assinatura morar no servidor: a lista de canais que o `Session`
/// manda é **inteira** — o canal fechado está nela, para quem não o lê —, e um
/// cliente que assinasse a lista ao entrar levantaria este aviso sem a pessoa
/// ter apertado nada. As duas medidas que decidiram o desenho, juntas.
#[tokio::test(flavor = "multi_thread")]
async fn assinar_o_canal_que_nao_se_le_continua_recusado_a_vista() -> Result<()> {
    let casa = casa().await?;
    let mut outra = entrar(casa.endereco, OUTRA).await?;
    assert_eq!(
        outra.canais,
        vec![ABERTO, FECHADO],
        "a lista de canais do `Session` deixou de trazer o canal que a pessoa \
         não lê: se ela passou a ser filtrada, o desenho da assinatura pode \
         ser revisto"
    );
    frame::write(
        &mut outra.envio,
        &ClientMessage::JoinChannel { channel: FECHADO },
    )
    .await?;
    let recusa = esperar(&mut outra, |quadro| match quadro {
        ServerMessage::Alert { reason, .. } => Some(*reason),
        _ => None,
    })
    .await;
    assert_eq!(recusa, Some(AlertReason::PermissionDenied));

    casa.daemon.shutdown();
    Ok(())
}
