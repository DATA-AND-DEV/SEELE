//! **A mesa mínima de texto da 1.0, no fio** (protocolo 13): editar a própria
//! mensagem, a editada que volta marcada do histórico, desbanir e a lista de
//! banidos — e os dois sentidos da compatibilidade com um par 12.
//!
//! # O que cada guarda prende
//!
//! - **Editar**: só o autor edita; quem lê o canal recebe o `MessageEdited`;
//!   o que não existe, o que foi apagado e o que mora num canal que a pessoa
//!   não lê respondem igual, `NaoExiste`.
//! - **O histórico**: o `MessageReceived` não tem campo para «editada», e
//!   campo novo em mensagem que existe é proibido (ADR 0058). Atrás da página
//!   o servidor manda um `MessageEdited` por mensagem editada — e só por elas.
//! - **Banidos**: a lista é de quem pode banir, não mostra banimento expirado,
//!   e desbanir devolve a pessoa à lista de membros de todo mundo.
//! - **Um par 12** não recebe o `Banidos`, que ele não conhece, e continua
//!   recebendo o `MessageEdited`, que é do 9.
//!
//! O banco é um arquivo semeado antes de o daemon subir, como em
//! `autorizacao_por_canal.rs`: não há verbo de protocolo para dar um cargo.

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
use seele_proto::control::{
    AlertReason, Banido, CausaDaRecusa, ClientMessage, DisconnectReason, ServerMessage,
};
use seele_proto::ids::{ChannelId, ClientMessageId, MessageId, PersonId};
use seele_server::permissions::{Permissions, COMMANDER_ROLE, OPERATOR_ROLE, PERSON_ROLE};
use seele_server::persistence::{Location, Persistence};
use seele_server::{frame, Daemon, ServerConfig};

mod comum;

const COMANDANTE: u8 = 61;
const OPERADOR: u8 = 62;
const PESSOA: u8 = 63;
const OUTRA: u8 = 64;

/// O canal que existe desde o começo e que qualquer pessoa lê e escreve. O
/// outro, o 2, é semeado fechado para quem não é Comandante, na leitura e na
/// escrita.
const ABERTO: ChannelId = ChannelId(1);

struct Casa {
    endereco: SocketAddr,
    daemon: Arc<Daemon>,
    banco: std::path::PathBuf,
    comandante: PersonId,
    operador: PersonId,
    pessoa: PersonId,
    outra: PersonId,
    /// Da Pessoa, no canal fechado (gravada antes de o canal fechar).
    da_pessoa_no_fechado: MessageId,
    _pasta: tempfile::TempDir,
}

fn chave(semente: u8) -> SigningKey {
    SigningKey::from_bytes(&[semente; 32])
}

fn sql(banco: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open(banco).expect("abrir o arquivo")
}

/// Sobe um servidor com quatro contas e dois canais.
async fn casa() -> Result<Casa> {
    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");
    let (comandante, operador, pessoa, outra, da_pessoa_no_fechado) = {
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
        let comandante = conta(COMANDANTE, "comandante", COMMANDER_ROLE);
        let operador = conta(OPERADOR, "operador", OPERATOR_ROLE);
        let pessoa = conta(PESSOA, "pessoa", PERSON_ROLE);
        let outra = conta(OUTRA, "outra", PERSON_ROLE);

        let sql = sql(&banco);
        sql.execute_batch(
            "INSERT OR IGNORE INTO channels (id, name) VALUES (1, 'aberto');
             INSERT INTO channels (id, name) VALUES (2, 'fechado');",
        )?;
        sql.execute(
            "UPDATE channels SET minimum_read_role = ?1, minimum_write_role = ?1 WHERE id = 2",
            [i64::from(COMMANDER_ROLE.get())],
        )?;
        sql.execute(
            "INSERT INTO messages (channel_id, author_id, body, created_at)
             VALUES (2, ?1, 'da pessoa, no fechado', 1700000000)",
            [pessoa.get() as i64],
        )?;
        let no_fechado = MessageId(u64::try_from(sql.last_insert_rowid())?);
        (comandante, operador, pessoa, outra, no_fechado)
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
        comandante,
        operador,
        pessoa,
        outra,
        da_pessoa_no_fechado,
        _pasta: pasta,
    })
}

/// Um par cru que já entrou.
struct Par {
    _conexao: seele_enlace::Conexao,
    envio: seele_enlace::Envio,
    recebe: seele_enlace::Recebe,
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
    match frame::read::<ServerMessage>(&mut recebe).await? {
        ServerMessage::Session { .. } => {}
        outra => anyhow::bail!("o servidor não mandou Session, e sim {outra:?}"),
    }
    Ok(Par {
        _conexao: conexao,
        envio,
        recebe,
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

/// Manda `pedido` e devolve o primeiro quadro que `serve` aceitar.
async fn pedir<T>(
    par: &mut Par,
    pedido: ClientMessage,
    serve: impl FnMut(&ServerMessage) -> Option<T>,
) -> Option<T> {
    frame::write(&mut par.envio, &pedido).await.ok()?;
    esperar(par, serve).await
}

/// Tudo o que chega até o `Pong` de um `Ping` com `carimbo` mandado agora: o
/// servidor lê o fluxo de controle em ordem, então o que ele respondeu aos
/// pedidos de antes já chegou.
async fn ate_o_pong(par: &mut Par, carimbo: u64) -> Vec<ServerMessage> {
    frame::write(&mut par.envio, &ClientMessage::Ping { timestamp: carimbo })
        .await
        .expect("mandar o Ping");
    let mut chegou = Vec::new();
    let fim = esperar(par, |quadro| {
        if matches!(quadro, ServerMessage::Pong { timestamp } if *timestamp == carimbo) {
            return Some(());
        }
        chegou.push(quadro.clone());
        None
    })
    .await;
    assert!(fim.is_some(), "o `Pong` de {carimbo} não voltou");
    chegou
}

fn alerta(quadro: &ServerMessage) -> Option<AlertReason> {
    match quadro {
        ServerMessage::Alert { reason, .. } => Some(*reason),
        _ => None,
    }
}

fn ordinal_de(nome: &str) -> u32 {
    let nomes = seele_proto::vocabulario::variantes::<ClientMessage>().unwrap();
    u32::try_from(nomes.iter().position(|n| *n == nome).unwrap()).unwrap()
}

/// Assina o canal e espera a assinatura valer (o `Pong` vem depois dela).
async fn assinar(par: &mut Par, canal: ChannelId) {
    frame::write(
        &mut par.envio,
        &ClientMessage::JoinChannel { channel: canal },
    )
    .await
    .expect("assinar");
    ate_o_pong(par, 1).await;
}

/// Diz `corpo` no canal e devolve o id que o servidor deu.
async fn dizer(par: &mut Par, canal: ChannelId, chave: u64, corpo: &str) -> MessageId {
    pedir(
        par,
        ClientMessage::SendMessage {
            channel: canal,
            body: corpo.into(),
            replies_to: None,
            client_message_id: ClientMessageId(chave),
        },
        |quadro| match quadro {
            ServerMessage::MessageReceived {
                id,
                client_message_id: Some(eco),
                ..
            } if eco.get() == chave => Some(*id),
            _ => None,
        },
    )
    .await
    .expect("a mensagem não foi confirmada")
}

fn editada(quadro: &ServerMessage) -> Option<(ChannelId, MessageId, String)> {
    match quadro {
        ServerMessage::MessageEdited { channel, id, body } => Some((*channel, *id, body.clone())),
        _ => None,
    }
}

fn corpo_gravado(banco: &Path, mensagem: MessageId) -> (String, bool) {
    sql(banco)
        .query_row(
            "SELECT body, edited_at IS NOT NULL FROM messages WHERE id = ?1",
            [mensagem.get() as i64],
            |linha| Ok((linha.get(0)?, linha.get(1)?)),
        )
        .expect("ler a mensagem")
}

// ------------------------------------------------------------------ editar

/// **A autora edita, e quem lê o canal recebe o corpo novo.** A própria
/// autora também: é a confirmação dela.
#[tokio::test(flavor = "multi_thread")]
async fn a_autora_edita_e_quem_le_o_canal_recebe() -> Result<()> {
    let casa = casa().await?;
    let mut pessoa = entrar(casa.endereco, PESSOA).await?;
    let mut outra = entrar(casa.endereco, OUTRA).await?;
    assinar(&mut pessoa, ABERTO).await;
    assinar(&mut outra, ABERTO).await;
    let id = dizer(&mut pessoa, ABERTO, 1, "com erro de digitassão").await;

    let confirmada = pedir(
        &mut pessoa,
        ClientMessage::EditMessage {
            id,
            body: "com erro de digitação".into(),
        },
        editada,
    )
    .await;
    assert_eq!(
        confirmada,
        Some((ABERTO, id, "com erro de digitação".to_owned())),
        "a autora não recebeu a própria edição"
    );
    assert_eq!(
        esperar(&mut outra, editada).await,
        Some((ABERTO, id, "com erro de digitação".to_owned())),
        "quem lê o canal não recebeu a edição"
    );
    assert_eq!(
        corpo_gravado(&casa.banco, id),
        ("com erro de digitação".to_owned(), true)
    );

    casa.daemon.shutdown();
    Ok(())
}

/// **Só a autora edita.** Nem a Comandante, que pode apagar: editar não é
/// moderar, e ninguém põe palavras na boca de outra pessoa.
#[tokio::test(flavor = "multi_thread")]
async fn nem_a_comandante_edita_a_mensagem_de_outra_pessoa() -> Result<()> {
    let casa = casa().await?;
    let mut pessoa = entrar(casa.endereco, PESSOA).await?;
    let mut comandante = entrar(casa.endereco, COMANDANTE).await?;
    assinar(&mut pessoa, ABERTO).await;
    let id = dizer(&mut pessoa, ABERTO, 1, "o que eu disse").await;

    let resposta = pedir(
        &mut comandante,
        ClientMessage::EditMessage {
            id,
            body: "o que eu não disse".into(),
        },
        alerta,
    )
    .await;
    assert_eq!(resposta, Some(AlertReason::PermissionDenied));
    assert_eq!(
        corpo_gravado(&casa.banco, id),
        ("o que eu disse".to_owned(), false),
        "a mensagem foi reescrita por quem não a escreveu"
    );

    casa.daemon.shutdown();
    Ok(())
}

/// O que não existe, o que foi apagado e o que mora num canal que a pessoa
/// já não lê respondem **igual**: `NaoExiste`, com o pedido que o pediu.
#[tokio::test(flavor = "multi_thread")]
async fn editar_o_que_nao_existe_o_apagado_e_o_que_nao_se_le_responde_igual() -> Result<()> {
    let casa = casa().await?;
    let mut pessoa = entrar(casa.endereco, PESSOA).await?;
    assinar(&mut pessoa, ABERTO).await;
    let apagada = dizer(&mut pessoa, ABERTO, 1, "vou apagar").await;
    let some = pedir(
        &mut pessoa,
        ClientMessage::RemoveMessage { message: apagada },
        |quadro| matches!(quadro, ServerMessage::MessageRemoved { .. }).then_some(()),
    )
    .await;
    assert!(some.is_some(), "a remoção não foi anunciada");

    let nao_existe = Some(AlertReason::PedidoRecusado {
        pedido: ordinal_de("EditMessage"),
        causa: CausaDaRecusa::NaoExiste,
    });
    for (caso, id) in [
        ("nunca existiu", MessageId(9_999)),
        ("foi apagada", apagada),
        // A própria mensagem, num canal que fechou para ela depois.
        (
            "mora num canal que a pessoa não lê",
            casa.da_pessoa_no_fechado,
        ),
    ] {
        let resposta = pedir(
            &mut pessoa,
            ClientMessage::EditMessage {
                id,
                body: "outra coisa".into(),
            },
            alerta,
        )
        .await;
        assert_eq!(resposta, nao_existe, "{caso}");
    }
    assert_eq!(
        corpo_gravado(&casa.banco, casa.da_pessoa_no_fechado),
        ("da pessoa, no fechado".to_owned(), false),
        "a mensagem do canal fechado foi editada por quem não o lê"
    );

    casa.daemon.shutdown();
    Ok(())
}

/// **A editada volta do histórico marcada.** Quem chega depois da edição pede
/// a página e recebe, atrás dela, um `MessageEdited` para a mensagem editada —
/// e para nenhuma outra. Dobrados numa sala (`seele_proto` não tem uma; a
/// conta é feita aqui como `Room::apply` a faz), a editada sai `edited`.
#[tokio::test(flavor = "multi_thread")]
async fn a_editada_volta_do_historico_marcada() -> Result<()> {
    let casa = casa().await?;
    let mut pessoa = entrar(casa.endereco, PESSOA).await?;
    assinar(&mut pessoa, ABERTO).await;
    let intacta = dizer(&mut pessoa, ABERTO, 1, "nunca mexi").await;
    let mexida = dizer(&mut pessoa, ABERTO, 2, "antes").await;
    pedir(
        &mut pessoa,
        ClientMessage::EditMessage {
            id: mexida,
            body: "depois".into(),
        },
        editada,
    )
    .await
    .expect("a edição não foi anunciada");

    // Quem chega agora nunca viu a edição acontecer.
    let mut outra = entrar(casa.endereco, OUTRA).await?;
    assinar(&mut outra, ABERTO).await;
    frame::write(
        &mut outra.envio,
        &ClientMessage::FetchHistory {
            channel: ABERTO,
            cursor: None,
            limit: 50,
        },
    )
    .await?;
    let pagina = ate_o_pong(&mut outra, 2).await;

    // Como quem recebe dobra: a mensagem entra pelo `MessageReceived`, e um
    // `MessageEdited` de um id conhecido a marca.
    let mut conversa: Vec<(MessageId, String, bool)> = Vec::new();
    for quadro in &pagina {
        match quadro {
            ServerMessage::MessageReceived { id, body, .. } => {
                conversa.push((*id, body.clone(), false));
            }
            ServerMessage::MessageEdited { id, body, .. } => {
                let linha = conversa
                    .iter_mut()
                    .find(|(conhecida, ..)| conhecida == id)
                    .expect("o `MessageEdited` chegou antes da mensagem que ele marca");
                linha.1.clone_from(body);
                linha.2 = true;
            }
            _ => {}
        }
    }
    assert_eq!(
        conversa,
        vec![
            (intacta, "nunca mexi".to_owned(), false),
            (mexida, "depois".to_owned(), true),
        ],
        "o histórico não devolveu a editada marcada, ou marcou a que ninguém editou"
    );

    casa.daemon.shutdown();
    Ok(())
}

// ------------------------------------------------------------------ banidos

fn banidos(quadro: &ServerMessage) -> Option<(Vec<Banido>, bool)> {
    match quadro {
        ServerMessage::Banidos { banidos, ultima } => Some((banidos.clone(), *ultima)),
        _ => None,
    }
}

async fn banir(par: &mut Par, alvo: PersonId, motivo: Option<&str>, expira: Option<i64>) {
    frame::write(
        &mut par.envio,
        &ClientMessage::BanPerson {
            person: alvo,
            reason: motivo.map(str::to_owned),
            expires_at: expira,
        },
    )
    .await
    .expect("banir");
    ate_o_pong(par, 7).await;
}

/// O apelido como o banco o guarda: o alias, com a marca (`pessoa#XXXX`).
fn apelido_gravado(banco: &Path, pessoa: PersonId) -> String {
    sql(banco)
        .query_row(
            "SELECT nickname FROM people WHERE id = ?1",
            [pessoa.get() as i64],
            |linha| linha.get(0),
        )
        .expect("ler o apelido")
}

fn agora() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("relógio")
            .as_secs(),
    )
    .expect("segundos")
}

/// **A lista mostra o que vale agora, com o que quem baniu escreveu**, e um
/// banimento expirado não aparece.
#[tokio::test(flavor = "multi_thread")]
async fn a_lista_mostra_os_banimentos_que_valem_e_nao_os_expirados() -> Result<()> {
    let casa = casa().await?;
    let mut comandante = entrar(casa.endereco, COMANDANTE).await?;
    let daqui_a_uma_hora = agora() + 3_600;
    banir(
        &mut comandante,
        casa.pessoa,
        Some("spam"),
        Some(daqui_a_uma_hora),
    )
    .await;
    banir(&mut comandante, casa.outra, None, Some(agora() - 1)).await;

    let apelido = apelido_gravado(&casa.banco, casa.pessoa);
    assert!(apelido.starts_with("pessoa"), "{apelido}");
    frame::write(&mut comandante.envio, &ClientMessage::ListBans).await?;
    let resposta = ate_o_pong(&mut comandante, 8).await;
    // Atrás da lista vem a identidade apresentada de cada banido, como atrás
    // da lista de membros: é por ela que a tela mostra `pessoa`, e não o alias.
    assert!(
        resposta.iter().any(|quadro| matches!(
            quadro,
            ServerMessage::IdentidadeApresentada { pessoa, base, .. }
                if *pessoa == casa.pessoa && base == "pessoa"
        )),
        "a identidade apresentada da banida não veio atrás da lista: {resposta:?}"
    );
    let lista = resposta.iter().find_map(banidos);
    assert_eq!(
        lista,
        Some((
            vec![Banido {
                person: casa.pessoa,
                nickname: apelido,
                reason: Some("spam".into()),
                expires_at: Some(daqui_a_uma_hora),
                issued_by: Some(casa.comandante),
            }],
            true
        )),
        "a lista não é a dos banimentos que valem agora"
    );

    casa.daemon.shutdown();
    Ok(())
}

/// A lista é de quem pode banir: o motivo é registro de quem modera.
#[tokio::test(flavor = "multi_thread")]
async fn quem_nao_pode_banir_nao_le_a_lista_nem_desbane() -> Result<()> {
    let casa = casa().await?;
    let mut comandante = entrar(casa.endereco, COMANDANTE).await?;
    banir(
        &mut comandante,
        casa.outra,
        Some("segredo de quem modera"),
        None,
    )
    .await;

    let mut pessoa = entrar(casa.endereco, PESSOA).await?;
    assert_eq!(
        pedir(&mut pessoa, ClientMessage::ListBans, alerta).await,
        Some(AlertReason::PermissionDenied)
    );
    assert_eq!(
        pedir(
            &mut pessoa,
            ClientMessage::UnbanPerson { person: casa.outra },
            alerta
        )
        .await,
        Some(AlertReason::PermissionDenied)
    );
    // E nenhum `Banidos` escapou no caminho.
    let depois = ate_o_pong(&mut pessoa, 9).await;
    assert!(
        !depois.iter().any(|quadro| banidos(quadro).is_some()),
        "a lista de banidos chegou a quem não pode banir"
    );
    assert!(
        Permissions::new(&Persistence::open(&Location::File(casa.banco.clone()))?)
            .is_banned(casa.outra)?,
        "quem não pode banir desbaniu"
    );

    casa.daemon.shutdown();
    Ok(())
}

/// **A hierarquia do banimento vale para o desbanimento.** O Operador pode
/// banir, mas não quem está acima dele — e não desfaz o banimento que a
/// Comandante pôs em alguém que ele não poderia ter banido.
#[tokio::test(flavor = "multi_thread")]
async fn o_operador_nao_desbane_quem_esta_acima_dele() -> Result<()> {
    let casa = casa().await?;
    // Uma segunda Comandante, banida pela primeira direto no banco: pelo fio,
    // uma Comandante não bane a outra (as duas têm a mesma altura).
    let segunda = {
        let persistence = Persistence::open(&Location::File(casa.banco.clone()))?;
        let permissions = Permissions::new(&persistence);
        let segunda = permissions
            .register_or_find(&chave(70).verifying_key().to_bytes(), "segunda")?
            .id;
        permissions.revoke_role(segunda, PERSON_ROLE)?;
        permissions.grant_role(segunda, COMMANDER_ROLE)?;
        segunda
    };
    sql(&casa.banco).execute(
        "INSERT INTO bans (person_id, issued_by, reason, created_at, expires_at)
         VALUES (?1, ?2, NULL, 1700000000, NULL)",
        [segunda.get() as i64, casa.comandante.get() as i64],
    )?;

    let mut operador = entrar(casa.endereco, OPERADOR).await?;
    assert_eq!(
        pedir(
            &mut operador,
            ClientMessage::UnbanPerson { person: segunda },
            alerta
        )
        .await,
        Some(AlertReason::PermissionDenied)
    );
    assert!(
        Permissions::new(&Persistence::open(&Location::File(casa.banco.clone()))?)
            .is_banned(segunda)?,
        "o Operador desfez o banimento de quem está acima dele"
    );
    // E a lista ele lê: ele pode banir.
    let lista = pedir(&mut operador, ClientMessage::ListBans, banidos).await;
    assert_eq!(
        lista.map(|(lista, ultima)| (lista.len(), ultima)),
        Some((1, true))
    );
    let _ = casa.operador;

    casa.daemon.shutdown();
    Ok(())
}

/// **Desbanir devolve a pessoa.** Quem pediu recebe a lista como ficou; todo
/// mundo recebe o `MembroNovo`, que é o que corrige a lista de membros de
/// quem estava conectado; e a pessoa entra de novo.
#[tokio::test(flavor = "multi_thread")]
async fn desbanir_devolve_a_pessoa_a_lista_de_membros_e_a_porta() -> Result<()> {
    let casa = casa().await?;
    let mut comandante = entrar(casa.endereco, COMANDANTE).await?;
    let mut outra = entrar(casa.endereco, OUTRA).await?;
    banir(&mut comandante, casa.pessoa, Some("engano"), None).await;
    // Banida, a porta recusa.
    let recusada = entrar(casa.endereco, PESSOA).await;
    assert!(
        recusada.as_ref().err().is_some_and(|erro| erro
            .to_string()
            .contains(&format!("{:?}", DisconnectReason::Banned))),
        "a banida entrou, ou foi recusada por outro motivo: {:?}",
        recusada.err()
    );

    // A lista vem na resposta e o `MembroNovo` pela difusão: as duas chegam,
    // em qualquer ordem.
    let apelido = apelido_gravado(&casa.banco, casa.pessoa);
    frame::write(
        &mut comandante.envio,
        &ClientMessage::UnbanPerson {
            person: casa.pessoa,
        },
    )
    .await?;
    let (mut lista, mut membro) = (None, false);
    esperar(&mut comandante, |quadro| {
        if let Some(chegou) = banidos(quadro) {
            lista = Some(chegou);
        }
        if matches!(
            quadro,
            ServerMessage::MembroNovo { membro } if membro.id == casa.pessoa && membro.nickname == apelido
        ) {
            membro = true;
        }
        (lista.is_some() && membro).then_some(())
    })
    .await;
    assert!(membro, "quem pediu não recebeu o `MembroNovo`");
    assert_eq!(
        lista,
        Some((vec![], true)),
        "quem pediu não recebeu a lista como ficou"
    );
    let voltou = esperar(&mut outra, |quadro| match quadro {
        ServerMessage::MembroNovo { membro } if membro.id == casa.pessoa => Some(()),
        _ => None,
    })
    .await;
    assert!(
        voltou.is_some(),
        "quem estava conectado não soube que a pessoa voltou a ser membro"
    );
    // Quem só conversa não recebe a lista de banidos de carona.
    let de_carona = ate_o_pong(&mut outra, 4).await;
    assert!(!de_carona.iter().any(|quadro| banidos(quadro).is_some()));

    entrar(casa.endereco, PESSOA)
        .await
        .expect("a desbanida não conseguiu entrar");

    // E desbanir de novo responde: não há banimento a tirar.
    assert_eq!(
        pedir(
            &mut comandante,
            ClientMessage::UnbanPerson {
                person: casa.pessoa
            },
            alerta
        )
        .await,
        Some(AlertReason::PedidoRecusado {
            pedido: ordinal_de("UnbanPerson"),
            causa: CausaDaRecusa::NaoExiste,
        })
    );

    casa.daemon.shutdown();
    Ok(())
}

// ------------------------------------------------------- um par do protocolo 12

/// **Um par 12 não recebe o `Banidos`**, que ele não conhece: a v0.19.6
/// publicada pularia o quadro, e uma build sem a rede de segurança perderia o
/// fluxo de controle. O pedido chega aqui cru — um cliente 12 de verdade nem o
/// escreve — para provar que o portão é do servidor, e não a boa educação de
/// quem pede.
#[tokio::test(flavor = "multi_thread")]
async fn um_par_doze_nao_recebe_a_lista_de_banidos() -> Result<()> {
    let casa = casa().await?;
    let mut novo = entrar(casa.endereco, COMANDANTE).await?;
    banir(&mut novo, casa.pessoa, None, None).await;
    drop(novo);

    let mut doze = entrar_falando(casa.endereco, COMANDANTE, 12).await?;
    frame::write(&mut doze.envio, &ClientMessage::ListBans).await?;
    frame::write(
        &mut doze.envio,
        &ClientMessage::UnbanPerson {
            person: casa.pessoa,
        },
    )
    .await?;
    // O desbanimento valeu, e o que ele conhece chegou: o `MembroNovo` é do 9.
    // Tudo o que passa até ele, e até o `Pong` que vem atrás, é conferido.
    let mut chegou = Vec::new();
    let voltou = esperar(&mut doze, |quadro| {
        chegou.push(quadro.clone());
        matches!(
            quadro,
            ServerMessage::MembroNovo { membro } if membro.id == casa.pessoa
        )
        .then_some(())
    })
    .await;
    assert!(voltou.is_some(), "o par 12 não recebeu o `MembroNovo`");
    chegou.extend(ate_o_pong(&mut doze, 5).await);
    assert!(
        !chegou.iter().any(|quadro| banidos(quadro).is_some()),
        "um par que negociou 12 recebeu `Banidos`: {chegou:?}"
    );

    // E a mesma conta, falando 13, recebe.
    drop(doze);
    let mut treze = entrar(casa.endereco, COMANDANTE).await?;
    assert_eq!(
        pedir(&mut treze, ClientMessage::ListBans, banidos).await,
        Some((vec![], true))
    );

    casa.daemon.shutdown();
    Ok(())
}

/// **Um par 12 vê a editada marcada no histórico**: o `MessageEdited` é do 9,
/// e foi por isso que a marca não virou campo nem variante nova.
#[tokio::test(flavor = "multi_thread")]
async fn um_par_doze_recebe_a_editada_marcada_no_historico() -> Result<()> {
    let casa = casa().await?;
    let mut pessoa = entrar(casa.endereco, PESSOA).await?;
    assinar(&mut pessoa, ABERTO).await;
    let id = dizer(&mut pessoa, ABERTO, 1, "antes").await;
    pedir(
        &mut pessoa,
        ClientMessage::EditMessage {
            id,
            body: "depois".into(),
        },
        editada,
    )
    .await
    .expect("a edição não foi anunciada");

    let mut doze = entrar_falando(casa.endereco, OUTRA, 12).await?;
    frame::write(
        &mut doze.envio,
        &ClientMessage::FetchHistory {
            channel: ABERTO,
            cursor: None,
            limit: 50,
        },
    )
    .await?;
    let pagina = ate_o_pong(&mut doze, 6).await;
    assert_eq!(
        pagina.iter().find_map(editada),
        Some((ABERTO, id, "depois".to_owned())),
        "o par 12 não recebeu a marca de editada"
    );

    casa.daemon.shutdown();
    Ok(())
}
