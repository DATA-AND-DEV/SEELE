//! **O canal e a hierarquia valem em todo verbo que toca um canal**, e não só em
//! `SendMessage`.
//!
//! Achados da revisão de segurança da 0.19, todos do mesmo formato: o verbo
//! conferia a permissão **global** e nunca perguntava a qual canal o pedido se
//! referia, de modo que um canal com papel mínimo (`minimum_read_role`,
//! `minimum_write_role`) só era fechado para quem usava o caminho que o
//! conferia.
//!
//! - `AttachFile` publicava no canal do cabeçalho sem perguntar se a pessoa
//!   escreve nele, nem se o canal existe, nem se a mensagem respondida é
//!   daquele canal;
//! - `WeighChannel` contava as mensagens de qualquer canal, inclusive os que a
//!   pessoa não lê;
//! - `RemoveMessage` apagava a mensagem de qualquer pessoa, de qualquer canal,
//!   sem olhar a hierarquia dos cargos que `Kick` e `Ban` olham.
//!
//! # Por que um par cru, contra um banco que já nasce preparado
//!
//! O que está sob teste é a decisão do servidor, e o cliente de verdade não a
//! deixa ser tomada de outro jeito: ele só oferece o que a casca desenharia.
//! O banco é um arquivo, semeado **antes** de o daemon subir, porque não há
//! verbo de protocolo para fechar um canal nem para dar um cargo.

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
use seele_proto::attachment::AttachmentHeader;
use seele_proto::control::{
    AlertReason, AttachmentRefusal, CausaDaRecusa, ClientMessage, ServerMessage,
};
use seele_proto::ids::{ChannelId, ClientMessageId, MessageId, PersonId};
use seele_proto::stream::StreamType;
use seele_server::permissions::{Permissions, COMMANDER_ROLE, OPERATOR_ROLE, PERSON_ROLE};
use seele_server::persistence::{Location, Persistence};
use seele_server::{frame, Daemon, ServerConfig};

mod comum;

const COMANDANTE: u8 = 41;
const OPERADOR: u8 = 42;
const PESSOA: u8 = 43;

/// O canal que existe desde o começo e que qualquer pessoa lê e escreve.
const ABERTO: ChannelId = ChannelId(1);
/// Fechado para quem não é Comandante, na leitura e na escrita.
const FECHADO: ChannelId = ChannelId(2);
/// Outro canal aberto: serve para a resposta que atravessa de um canal a outro.
const OUTRO_ABERTO: ChannelId = ChannelId(3);
/// Um canal que não existe.
const FANTASMA: ChannelId = ChannelId(999);

/// O que o banco semeado guarda, para os testes apontarem.
struct Semente {
    /// Da Pessoa, no canal fechado.
    da_pessoa_no_fechado: MessageId,
    /// Do Comandante, no canal aberto.
    do_comandante_no_aberto: MessageId,
    /// Da Pessoa, no canal aberto.
    da_pessoa_no_aberto: MessageId,
    /// Do Comandante, no outro canal aberto.
    do_comandante_no_outro: MessageId,
}

struct Casa {
    endereco: SocketAddr,
    daemon: Arc<Daemon>,
    banco: std::path::PathBuf,
    semente: Semente,
    _pasta: tempfile::TempDir,
}

fn chave(semente: u8) -> SigningKey {
    SigningKey::from_bytes(&[semente; 32])
}

/// Uma conexão crua ao arquivo: `Persistence::connection` não é pública, e o
/// que estes testes fazem com o banco é semear e conferir.
fn sql(banco: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open(banco).expect("abrir o arquivo")
}

fn mensagem(
    sql: &rusqlite::Connection,
    canal: ChannelId,
    autor: PersonId,
    corpo: &str,
) -> MessageId {
    sql.execute(
        "INSERT INTO messages (channel_id, author_id, body, created_at)
             VALUES (?1, ?2, ?3, 1700000000)",
        rusqlite::params![i64::from(canal.get()), autor.get() as i64, corpo],
    )
    .expect("semear a mensagem");
    MessageId(u64::try_from(sql.last_insert_rowid()).expect("id"))
}

/// Sobe um servidor com um banco em arquivo, semeado antes do `bind`.
async fn casa() -> Result<Casa> {
    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");
    let semente = {
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
        let _operador = conta(OPERADOR, "operador", OPERATOR_ROLE);
        let pessoa = conta(PESSOA, "pessoa", PERSON_ROLE);

        let sql = sql(&banco);
        sql.execute_batch(
            "INSERT OR IGNORE INTO channels (id, name) VALUES (1, 'aberto');
             INSERT INTO channels (id, name) VALUES (2, 'fechado'), (3, 'outro');",
        )?;
        sql.execute(
            "UPDATE channels SET minimum_read_role = ?1, minimum_write_role = ?1 WHERE id = 2",
            [i64::from(COMMANDER_ROLE.get())],
        )?;

        Semente {
            da_pessoa_no_fechado: mensagem(&sql, FECHADO, pessoa, "segredo do fechado"),
            do_comandante_no_aberto: mensagem(&sql, ABERTO, comandante, "da comandante"),
            da_pessoa_no_aberto: mensagem(&sql, ABERTO, pessoa, "da pessoa"),
            do_comandante_no_outro: mensagem(&sql, OUTRO_ABERTO, comandante, "no outro"),
        }
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
        semente,
        _pasta: pasta,
    })
}

/// Um par cru que já entrou.
struct Par {
    conexao: seele_enlace::Conexao,
    envio: seele_enlace::Envio,
    recebe: seele_enlace::Recebe,
}

async fn entrar(endereco: SocketAddr, semente: u8, apelido: &str) -> Result<Par> {
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
            faixa: seele_proto::FAIXA,
            client: "par-cru".into(),
            nickname: apelido.into(),
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
    let ServerMessage::Session { .. } = frame::read::<ServerMessage>(&mut recebe).await? else {
        anyhow::bail!("o servidor não mandou Session");
    };
    Ok(Par {
        conexao,
        envio,
        recebe,
    })
}

/// Manda `pedido` e devolve o primeiro quadro que `serve` aceitar, em até 3 s.
async fn pedir<T>(
    par: &mut Par,
    pedido: ClientMessage,
    mut serve: impl FnMut(&ServerMessage) -> Option<T>,
) -> Option<T> {
    frame::write(&mut par.envio, &pedido).await.ok()?;
    tokio::time::timeout(Duration::from_secs(3), async {
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

fn apagada(banco: &Path, mensagem: MessageId) -> bool {
    sql(banco)
        .query_row(
            "SELECT deleted_at IS NOT NULL FROM messages WHERE id = ?1",
            [mensagem.get() as i64],
            |linha| linha.get(0),
        )
        .expect("a mensagem existe")
}

// ---------------------------------------------------------------------------
// 1. Anexo
// ---------------------------------------------------------------------------

/// Sobe um anexo de quatro bytes, com o hash certo, e devolve a recusa, se
/// houve uma em até 3 s.
async fn anexar(
    par: &mut Par,
    canal: ChannelId,
    chave: u64,
    responde_a: Option<MessageId>,
) -> Option<AttachmentRefusal> {
    let bytes = [1_u8, 2, 3, 4];
    let header = AttachmentHeader {
        channel: canal,
        client_message_id: ClientMessageId(chave),
        body: "olha isto".into(),
        replies_to: responde_a,
        file_name: "retrato.png".into(),
        declared_type: "image/png".into(),
        declared_len: bytes.len() as u64,
        content_hash: seele_proto::attachment::hash(&bytes),
    };
    let quadro = seele_proto::control::encode(&header).expect("codificar");
    let mut fluxo = par.conexao.open_uni().await.expect("abrir o fluxo");
    fluxo
        .write_all(&[StreamType::Attachment.byte()])
        .await
        .ok()?;
    fluxo
        .write_all(&u32::try_from(quadro.len()).unwrap().to_be_bytes())
        .await
        .ok()?;
    fluxo.write_all(&quadro).await.ok()?;
    fluxo.write_all(&bytes).await.ok()?;
    fluxo.finish().ok()?;

    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match frame::read::<ServerMessage>(&mut par.recebe).await {
                Ok(ServerMessage::AttachmentRefused {
                    client_message_id,
                    reason,
                }) if client_message_id == ClientMessageId(chave) => return Some(reason),
                Ok(_) => {}
                Err(_) => return None,
            }
        }
    })
    .await
    .ok()
    .flatten()
}

/// Quantas mensagens com esta chave de idempotência o banco guarda.
fn gravadas(banco: &Path, chave: u64) -> i64 {
    sql(banco)
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE client_message_id = ?1",
            [chave as i64],
            |linha| linha.get(0),
        )
        .expect("contar")
}

/// Espera a mensagem de um anexo aceito entrar no banco.
async fn espera_gravar(banco: &Path, chave: u64) -> bool {
    for _ in 0..50 {
        if gravadas(banco, chave) == 1 {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

/// **O ataque:** `AttachFile` é global, e o canal do cabeçalho era publicado
/// como veio. Uma Pessoa sem escrita no canal fechado escrevia nele por anexo.
#[tokio::test(flavor = "multi_thread")]
async fn anexo_num_canal_onde_a_pessoa_nao_escreve_e_recusado() -> Result<()> {
    let casa = casa().await?;
    let mut pessoa = entrar(casa.endereco, PESSOA, "pessoa").await?;

    // O controle: o mesmo anexo, no canal aberto, entra. Sem ele, a recusa
    // abaixo poderia ser de qualquer outra causa — hash, teto, vazão.
    assert_eq!(anexar(&mut pessoa, ABERTO, 1, None).await, None);
    assert!(
        espera_gravar(&casa.banco, 1).await,
        "o anexo legítimo no canal aberto não entrou: o controle não mede nada"
    );

    assert_eq!(
        anexar(&mut pessoa, FECHADO, 2, None).await,
        Some(AttachmentRefusal::NotAllowed),
        "o anexo entrou num canal onde a pessoa não escreve"
    );
    assert_eq!(
        gravadas(&casa.banco, 2),
        0,
        "a mensagem foi gravada mesmo assim"
    );

    casa.daemon.shutdown();
    Ok(())
}

/// Um canal que não existe responde **igual** a um que a pessoa não escreve:
/// outra palavra diria quais ids existem.
#[tokio::test(flavor = "multi_thread")]
async fn anexo_num_canal_que_nao_existe_responde_como_num_fechado() -> Result<()> {
    let casa = casa().await?;
    let mut pessoa = entrar(casa.endereco, PESSOA, "pessoa").await?;

    assert_eq!(
        anexar(&mut pessoa, FANTASMA, 3, None).await,
        Some(AttachmentRefusal::NotAllowed)
    );
    assert_eq!(gravadas(&casa.banco, 3), 0);

    casa.daemon.shutdown();
    Ok(())
}

/// **A resposta que atravessa:** `replies_to` apontando para uma mensagem de
/// outro canal pendura o anexo na conversa alheia. A resposta ao mesmo canal
/// continua passando.
#[tokio::test(flavor = "multi_thread")]
async fn anexo_que_responde_a_mensagem_de_outro_canal_e_recusado() -> Result<()> {
    let casa = casa().await?;
    let mut pessoa = entrar(casa.endereco, PESSOA, "pessoa").await?;

    // O controle: responder a uma mensagem do próprio canal entra.
    assert_eq!(
        anexar(
            &mut pessoa,
            ABERTO,
            4,
            Some(casa.semente.do_comandante_no_aberto)
        )
        .await,
        None
    );
    assert!(
        espera_gravar(&casa.banco, 4).await,
        "a resposta legítima não entrou"
    );

    assert_eq!(
        anexar(
            &mut pessoa,
            ABERTO,
            5,
            Some(casa.semente.do_comandante_no_outro)
        )
        .await,
        Some(AttachmentRefusal::NotAllowed),
        "o anexo respondeu a uma mensagem de outro canal"
    );
    assert_eq!(gravadas(&casa.banco, 5), 0);

    // E a que aponta para uma mensagem que não existe: a chave estrangeira a
    // derrubaria no meio da gravação, depois de os bytes terem subido.
    assert_eq!(
        anexar(&mut pessoa, ABERTO, 6, Some(MessageId(987_654))).await,
        Some(AttachmentRefusal::NotAllowed)
    );

    casa.daemon.shutdown();
    Ok(())
}

/// **O mesmo buraco na mensagem comum** (0.19): o `SendMessage` conferia o
/// canal e a escrita, e não a mensagem respondida. Uma resposta pendurada numa
/// mensagem de outro canal, ou numa que não existe, tem de ser recusada antes
/// do lote, como no anexo.
#[tokio::test(flavor = "multi_thread")]
async fn mensagem_que_responde_a_mensagem_de_outro_canal_e_recusada() -> Result<()> {
    let casa = casa().await?;
    let mut pessoa = entrar(casa.endereco, PESSOA, "pessoa").await?;

    let responder = |pai: MessageId, chave: u64| ClientMessage::SendMessage {
        channel: ABERTO,
        body: format!("resposta {chave}"),
        replies_to: Some(pai),
        client_message_id: ClientMessageId(chave),
    };
    let recusa_de = |chave: u64| {
        move |quadro: &ServerMessage| match quadro {
            ServerMessage::MessageRejected {
                client_message_id,
                reason,
                ..
            } if *client_message_id == ClientMessageId(chave) => Some(*reason),
            _ => None,
        }
    };

    // O controle: responder a uma mensagem do próprio canal entra.
    frame::write(
        &mut pessoa.envio,
        &responder(casa.semente.do_comandante_no_aberto, 14),
    )
    .await?;
    assert!(
        espera_gravar(&casa.banco, 14).await,
        "a resposta legítima não entrou"
    );

    let outro = pedir(
        &mut pessoa,
        responder(casa.semente.do_comandante_no_outro, 15),
        recusa_de(15),
    )
    .await;
    assert!(
        outro.is_some(),
        "a resposta a uma mensagem de outro canal não foi recusada"
    );
    assert_eq!(
        gravadas(&casa.banco, 15),
        0,
        "a resposta a outro canal foi gravada"
    );

    let fantasma = pedir(
        &mut pessoa,
        responder(MessageId(987_654), 16),
        recusa_de(16),
    )
    .await;
    assert_eq!(
        fantasma, outro,
        "responder a uma mensagem que não existe recusa diferente de responder a outro canal"
    );

    casa.daemon.shutdown();
    Ok(())
}

// ---------------------------------------------------------------------------
// 3. WeighChannel
// ---------------------------------------------------------------------------

/// **O ataque:** pesar o canal fechado devolvia quantas mensagens, de quantas
/// pessoas, desde quando. E a resposta tem de ser a **mesma** de um canal que
/// não existe, ou a diferença diz quais canais há.
#[tokio::test(flavor = "multi_thread")]
async fn pesar_um_canal_que_nao_se_le_responde_como_um_que_nao_existe() -> Result<()> {
    let casa = casa().await?;
    let mut pessoa = entrar(casa.endereco, PESSOA, "pessoa").await?;

    let pesou_ou_recusou = |quadro: &ServerMessage| match quadro {
        ServerMessage::ChannelWeighed { .. } | ServerMessage::Alert { .. } => Some(quadro.clone()),
        _ => None,
    };

    // O controle: o canal aberto é pesado.
    let aberto = pedir(
        &mut pessoa,
        ClientMessage::WeighChannel { channel: ABERTO },
        pesou_ou_recusou,
    )
    .await;
    assert!(
        matches!(
            aberto,
            Some(ServerMessage::ChannelWeighed { messages: 2, .. })
        ),
        "o canal aberto não foi pesado: {aberto:?}"
    );

    let fechado = pedir(
        &mut pessoa,
        ClientMessage::WeighChannel { channel: FECHADO },
        pesou_ou_recusou,
    )
    .await;
    let fantasma = pedir(
        &mut pessoa,
        ClientMessage::WeighChannel { channel: FANTASMA },
        pesou_ou_recusou,
    )
    .await;

    assert!(
        matches!(
            fantasma,
            Some(ServerMessage::Alert {
                reason: AlertReason::PedidoRecusado {
                    causa: CausaDaRecusa::NaoExiste,
                    ..
                },
                ..
            })
        ),
        "o canal inexistente deveria voltar como NaoExiste: {fantasma:?}"
    );
    assert_eq!(
        fechado, fantasma,
        "pesar o canal fechado respondeu diferente de pesar um que não existe"
    );

    casa.daemon.shutdown();
    Ok(())
}

// ---------------------------------------------------------------------------
// 4. RemoveMessage
// ---------------------------------------------------------------------------

/// Pede a remoção e espera o banco dizer que ela aconteceu.
///
/// Pelo banco, e não por `MessageRemoved`: o anúncio só chega a quem abriu o
/// canal, e o que se quer saber aqui é se a linha foi carimbada.
async fn apagar_e_esperar(par: &mut Par, banco: &Path, alvo: MessageId) {
    frame::write(
        &mut par.envio,
        &ClientMessage::RemoveMessage { message: alvo },
    )
    .await
    .expect("mandar o pedido");
    for _ in 0..50 {
        if apagada(banco, alvo) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("a remoção legítima não aconteceu: o controle não mede nada");
}

/// **O ataque do canal:** um Operador tem `RemoveMessage`, mas não lê o canal
/// fechado — e apagava a mensagem de lá, às cegas. Responde como se ela não
/// existisse.
#[tokio::test(flavor = "multi_thread")]
async fn apagar_a_mensagem_de_um_canal_que_nao_se_le_responde_como_inexistente() -> Result<()> {
    let casa = casa().await?;
    let mut operador = entrar(casa.endereco, OPERADOR, "operador").await?;

    let resposta = pedir(
        &mut operador,
        ClientMessage::RemoveMessage {
            message: casa.semente.da_pessoa_no_fechado,
        },
        alerta,
    )
    .await;
    assert_eq!(
        resposta,
        Some(AlertReason::PedidoRecusado {
            pedido: ordinal_de("RemoveMessage"),
            causa: CausaDaRecusa::NaoExiste,
        })
    );
    assert!(
        !apagada(&casa.banco, casa.semente.da_pessoa_no_fechado),
        "a mensagem do canal fechado foi apagada por quem não o lê"
    );

    casa.daemon.shutdown();
    Ok(())
}

/// **O ataque da hierarquia:** o Operador apagava a mensagem de quem está
/// acima dele, num canal que ele lê — a regra que `Kick` e `Ban` aplicam.
#[tokio::test(flavor = "multi_thread")]
async fn operador_nao_apaga_a_mensagem_de_quem_esta_acima_dele() -> Result<()> {
    let casa = casa().await?;
    let mut operador = entrar(casa.endereco, OPERADOR, "operador").await?;

    let resposta = pedir(
        &mut operador,
        ClientMessage::RemoveMessage {
            message: casa.semente.do_comandante_no_aberto,
        },
        alerta,
    )
    .await;
    assert_eq!(resposta, Some(AlertReason::PermissionDenied));
    assert!(
        !apagada(&casa.banco, casa.semente.do_comandante_no_aberto),
        "o Operador apagou a mensagem da Comandante"
    );

    // O controle: abaixo dele, num canal que ele lê, o Operador apaga. Sem isto
    // a recusa acima poderia ser o verbo inteiro quebrado.
    apagar_e_esperar(&mut operador, &casa.banco, casa.semente.da_pessoa_no_aberto).await;
    assert!(apagada(&casa.banco, casa.semente.da_pessoa_no_aberto));

    casa.daemon.shutdown();
    Ok(())
}

/// A autora apaga a **própria** mensagem sem permissão nenhuma — e continua
/// podendo, o que o conserto não pode levar junto.
#[tokio::test(flavor = "multi_thread")]
async fn a_autora_continua_apagando_a_propria_mensagem() -> Result<()> {
    let casa = casa().await?;
    let mut pessoa = entrar(casa.endereco, PESSOA, "pessoa").await?;

    apagar_e_esperar(&mut pessoa, &casa.banco, casa.semente.da_pessoa_no_aberto).await;

    casa.daemon.shutdown();
    Ok(())
}

// ---------------------------------------------------------------------------
// 2. Senha da sala de voz
// ---------------------------------------------------------------------------

const SENHA: &str = "coelho-branco-42";

/// Pede a entrada na sala e diz se o servidor a **recusou**.
///
/// Quem entra não recebe nada de volta, então «aceita» é o silêncio de um
/// segundo; a recusa é o alerta.
async fn recusada(par: &mut Par, senha: Option<&str>) -> bool {
    frame::write(
        &mut par.envio,
        &ClientMessage::EnterVoiceRoom {
            voice_room: seele_proto::ids::VoiceRoomId(1),
            password: senha.map(str::to_owned),
        },
    )
    .await
    .expect("mandar o pedido");
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if let Ok(ServerMessage::Alert {
                reason: AlertReason::VoiceRoomEntryRefused,
                ..
            }) = frame::read::<ServerMessage>(&mut par.recebe).await
            {
                return true;
            }
        }
    })
    .await
    .unwrap_or(false)
}

/// **O ataque:** a senha da sala se adivinhava na velocidade do Argon2, sem
/// teto. Depois de cinco erradas, a sala recusa **inclusive a certa**; e o
/// teto é de quem erra: outra pessoa, com a senha certa, entra.
#[tokio::test(flavor = "multi_thread")]
async fn senha_errada_em_laco_fecha_a_sala_ate_para_a_senha_certa() -> Result<()> {
    let casa = casa().await?;
    {
        let mut persistence = casa.daemon.server().persistence.lock().await;
        seele_server::admissao::definir_senha_voice_room(
            &mut persistence,
            seele_proto::ids::VoiceRoomId(1),
            Some(SENHA),
        )?;
    }
    let mut pessoa = entrar(casa.endereco, PESSOA, "pessoa").await?;
    let mut operador = entrar(casa.endereco, OPERADOR, "operador").await?;

    // O controle: a senha certa abre a sala, e uma errada é recusada. Sem ele,
    // a recusa final poderia ser uma sala que nunca abre.
    assert!(
        !recusada(&mut operador, Some(SENHA)).await,
        "a senha certa não abriu"
    );

    for tentativa in 0..5 {
        assert!(
            recusada(&mut pessoa, Some("errada")).await,
            "a senha errada #{tentativa} não foi recusada"
        );
    }
    assert!(
        recusada(&mut pessoa, Some(SENHA)).await,
        "depois de cinco erradas, a senha certa ainda abriu a sala"
    );

    casa.daemon.shutdown();
    Ok(())
}

/// **O Argon2 não roda com o banco fechado.** A conta custa dezenas de
/// milissegundos (ou mais, num build de teste); se o `Mutex` da persistência —
/// um só, de todas as sessões — ficar fechado durante ela, cada senha errada
/// para a escrita de todo mundo.
///
/// Mede-se sem relógio fixo: primeiro quanto uma verificação leva *nesta*
/// máquina, e depois quanto tempo o banco demora a ficar livre com uma
/// verificação em curso no servidor. Tem de ser uma fração dela.
#[tokio::test(flavor = "multi_thread")]
async fn conferir_a_senha_nao_segura_o_banco() -> Result<()> {
    let casa = casa().await?;
    {
        let mut persistence = casa.daemon.server().persistence.lock().await;
        seele_server::admissao::definir_senha_voice_room(
            &mut persistence,
            seele_proto::ids::VoiceRoomId(1),
            Some(SENHA),
        )?;
    }
    let fechadura = {
        let persistence = casa.daemon.server().persistence.lock().await;
        seele_server::admissao::fechadura_da_sala(&persistence, seele_proto::ids::VoiceRoomId(1))
    };
    let inicio = std::time::Instant::now();
    assert!(!seele_server::admissao::senha_abre(
        &fechadura,
        Some("errada")
    ));
    let conta = inicio.elapsed();
    assert!(
        conta > Duration::from_millis(100),
        "uma verificação levou só {conta:?} aqui; o teste não distingue segurar de soltar"
    );

    let mut pessoa = entrar(casa.endereco, PESSOA, "pessoa").await?;
    frame::write(
        &mut pessoa.envio,
        &ClientMessage::EnterVoiceRoom {
            voice_room: seele_proto::ids::VoiceRoomId(1),
            password: Some("errada".into()),
        },
    )
    .await?;
    // O pedido chega e a verificação começa; o resto da conta ainda vem.
    tokio::time::sleep(Duration::from_millis(40)).await;
    let inicio = std::time::Instant::now();
    drop(casa.daemon.server().persistence.lock().await);
    let esperou = inicio.elapsed();
    assert!(
        esperou < conta / 2,
        "o banco ficou fechado {esperou:?} de uma conta de {conta:?}: a senha \
         está sendo conferida com ele na mão"
    );

    casa.daemon.shutdown();
    Ok(())
}
