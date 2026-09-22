//! Text messages and history.
//!
//! `specs/04-servidor-seele.md`:
//!
//! > Histórico de mensagens com retenção configurável (padrão: ilimitado).
//! > Índice em `(linha_id, criado_em)` para paginação por cursor.
//! > Escritas de mensagem em lote com `flush` por tempo (~200 ms) para não fazer
//! > fsync por mensagem.
//!
//! # Batching and the guarantee that constrains it
//!
//! The same document also sets an acceptance criterion: **"Reinício não perde
//! mensagem confirmada ao cliente."**
//!
//! Those two pull against each other. Batching means a window where a message
//! is accepted but not yet durable, and if the client were told "sent" inside
//! that window, a crash would break the promise. So the order is fixed:
//!
//! 1. The message joins the pending batch.
//! 2. The batch commits — one `fsync` for all of it.
//! 3. **Only then** does anybody hear about it.
//!
//! Confirmation is therefore delayed by up to the flush interval. For chat that
//! is imperceptible; for the guarantee it is the whole difference between
//! keeping it and merely claiming it.
//!
//! # Pagination is by id, not by time
//!
//! `specs/02-protocolo.md`: "Paginação por cursor, nunca offset." The cursor is
//! a message id. Ids are monotonic; `created_at` is a wall clock that can tie
//! between two messages in the same millisecond, and can step backwards when
//! the host adjusts its clock. Paginating by time would silently drop or repeat
//! messages whenever either happened.

use anyhow::{Context, Result};
use rusqlite::{params, OptionalExtension};
use seele_proto::control::AttachmentInfo;
use seele_proto::ids::{ChannelId, ClientMessageId, MessageId, PersonId};

use super::attachments::Attachments;
use super::{now_seconds, Persistence};

/// Largest page a client may ask for.
///
/// `specs/02-protocolo.md` leaves the limit open. This bounds a single response
/// so one `FetchHistory` cannot ask the server to build a reply larger than the
/// control frame it has to fit in.
pub const MAX_PAGE: u16 = 200;

/// Default page when a client asks for none.
pub const DEFAULT_PAGE: u16 = 50;

/// A message waiting to be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingMessage {
    /// Which Channel.
    pub channel: ChannelId,
    /// Who wrote it.
    pub author: PersonId,
    /// What the author is called, stamped by their own connection.
    ///
    /// Carried through rather than looked up when the message is broadcast: the
    /// authoring session already knows its own name, and every other session
    /// receiving the broadcast would otherwise have to query for it.
    pub author_nickname: String,
    /// Body.
    pub body: String,
    /// What it replies to.
    pub replies_to: Option<MessageId>,
    /// The sender's idempotency key. Gap G9.
    pub client_message_id: Option<ClientMessageId>,
}

/// A message as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMessage {
    /// Server-assigned identifier, and the pagination cursor.
    pub id: MessageId,
    /// Which Channel.
    pub channel: ChannelId,
    /// Who wrote it.
    pub author: PersonId,
    /// What the author is called.
    ///
    /// Empty when read back from the database, where only the id is stored —
    /// the caller resolving history joins the names itself, in one query rather
    /// than one per row.
    pub author_nickname: String,
    /// Body. Empty when removed.
    pub body: String,
    /// When it was written, seconds since the epoch.
    pub created_at: i64,
    /// When it was last edited.
    pub edited_at: Option<i64>,
    /// What it replies to.
    pub replies_to: Option<MessageId>,
    /// Echo of the sender's idempotency key.
    pub client_message_id: Option<ClientMessageId>,
    /// The file hanging off it, if any. ADR 0027.
    ///
    /// Joined by the readers below rather than looked up per row, for the same
    /// reason `author_nickname` is: a screenful of history would otherwise be
    /// fifty round trips through SQLite to answer "does this one have a file".
    ///
    /// Still `Some` when the bytes have been evicted, carrying
    /// [`AttachmentState::Expired`]. **The text survives the file**, and this
    /// field is where that is true or not true.
    pub attachment: Option<AttachmentInfo>,
}

/// O que aconteceu com **cada** mensagem de um lote.
///
/// # Por que o lote deixou de ter um resultado só
///
/// Porque ele tinha, e o resultado era «tudo ou nada». Uma transação única com
/// um destino inválido dentro falhava inteira na chave estrangeira, e o escritor
/// — que já havia tirado o lote da fila — registrava o erro e voltava: as
/// mensagens válidas de todas as outras pessoas do mesmo intervalo de 200 ms
/// desapareciam sem que ninguém fosse avisado (R04 da revisão da v15).
///
/// Um cliente com defeito, um canal removido durante o envio, ou uma pessoa
/// autenticada mandando um número de canal inventado bastavam.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gravacao {
    /// Gravada, e **esta** é a que pode ser confirmada a quem escreveu.
    Feita(StoredMessage),
    /// Não gravada, e o suficiente para dizer a quem escreveu qual delas foi.
    Falhou {
        /// O canal a que ela se destinava.
        channel: ChannelId,
        /// Quem escreveu, para que o aviso chegue só a essa pessoa.
        author: PersonId,
        /// A chave que quem escreveu escolheu, e por onde a casca a encontra.
        ///
        /// `None` é uma mensagem sem chave de idempotência. Ela não pode ser
        /// identificada de volta, e é por isso que o cliente deste produto
        /// sempre manda uma — ver `ClientMessage::SendMessage`.
        client_message_id: Option<ClientMessageId>,
        /// Por quê, no vocabulário que vai para o fio.
        motivo: seele_proto::control::MessageRefusal,
    },
}

impl Gravacao {
    /// A linha gravada, ou `None` se esta mensagem foi recusada.
    ///
    /// Existe para quem só quer as que passaram — a difusão, o histórico que
    /// volta, e os testes. Quem precisa dizer a alguém que a mensagem dele não
    /// entrou tem de casar a variante, e é o certo: o motivo é a metade que não
    /// pode ser descartada por conveniência.
    #[must_use]
    pub const fn gravada(&self) -> Option<&StoredMessage> {
        match self {
            Self::Feita(stored) => Some(stored),
            Self::Falhou { .. } => None,
        }
    }
}

/// Uma recusa montada a partir da mensagem que não entrou.
fn recusa(message: &PendingMessage, motivo: seele_proto::control::MessageRefusal) -> Gravacao {
    Gravacao::Falhou {
        channel: message.channel,
        author: message.author,
        client_message_id: message.client_message_id,
        motivo,
    }
}

/// Por que uma gravação falhou, no vocabulário do fio.
///
/// **A distinção é a que decide se tentar de novo faz sentido.** Uma violação de
/// chave estrangeira é um destino que não existe, e repeti-la é bater na mesma
/// porta para sempre; qualquer outra falha do armazenamento pode passar no
/// próximo lote. O review pede exatamente isso: «distinguir falha transitória de
/// armazenamento de entrada inválida. Não repetir indefinidamente uma entrada
/// inválida.»
fn motivo_da_falha(erro: &rusqlite::Error) -> seele_proto::control::MessageRefusal {
    use rusqlite::ffi::ErrorCode;

    if let rusqlite::Error::SqliteFailure(falha, _) = erro {
        if falha.code == ErrorCode::ConstraintViolation {
            return seele_proto::control::MessageRefusal::NoSuchChannel;
        }
    }
    seele_proto::control::MessageRefusal::StorageFailed
}

/// Why a message operation was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MessageRefusal {
    /// No such message, or it was removed.
    #[error("no such message")]
    NotFound,
    /// Only the author may edit their own message.
    #[error("only the author may edit a message")]
    NotTheAuthor,
}

/// What the idempotency lookup reads back: `(id, channel_id, body, created_at,
/// edited_at, replies_to)`.
///
/// Named rather than written inline because the tuple is what an existing row
/// *is* here, and the answer to a retry is built out of it field by field.
///
/// **O canal entrou nesta tupla no conserto do R09.** Ele faltava, e a ausência
/// dele era o defeito: o resultado da repetição saía com o canal do pedido novo.
type StoredRow = (i64, i64, String, i64, Option<i64>, Option<i64>);

/// Message storage, over PERSISTENCE.
pub struct Messages<'a> {
    persistence: &'a mut Persistence,
}

impl<'a> Messages<'a> {
    /// Borrows a store.
    pub fn new(persistence: &'a mut Persistence) -> Self {
        Self { persistence }
    }

    /// Writes a whole batch in one transaction, and returns what was stored.
    ///
    /// One `fsync` for the batch rather than one per message
    /// (`specs/04-servidor-seele.md`). The returned messages carry their assigned
    /// ids, and **only these are safe to confirm to a client** — see the module
    /// docs.
    ///
    /// A message whose `client_message_id` has already been used by the same
    /// author is not written twice; the existing one comes back instead.
    /// `specs/02-protocolo.md` calls the send "idempotente por `client_msg_id`",
    /// and without this a retry after a lost acknowledgement posts twice.
    ///
    /// # Errors
    ///
    /// Fails if the transaction cannot commit.
    pub fn append_batch(&mut self, pending: &[PendingMessage]) -> Result<Vec<Gravacao>> {
        if pending.is_empty() {
            return Ok(Vec::new());
        }

        let now = now_seconds();
        let mut transaction = self
            .persistence
            .connection_mut()
            .transaction()
            .context("could not open the write batch")?;

        let mut stored = Vec::with_capacity(pending.len());
        for message in pending {
            // An existing idempotency key means this is a retry, not a new
            // message. Returning the original is what makes the send idempotent.
            //
            // **The original, and not the one that just arrived.** This used to
            // select the id alone and then build the answer out of the incoming
            // message — old id, new body, new timestamp. For an honest retry the
            // two bodies are equal and nothing showed. When they differ, the row
            // is never written and yet the new body is broadcast under the old
            // row's id: whoever has the window open reads the new text, whoever
            // fetches history afterwards reads the old one, and neither is told.
            // A message quietly changing content between two readers is worse
            // than a message that fails to send.
            //
            // Bodies differ under the defect below, which is real and open:
            // `client_message_id` restarts at 1 every session, or
            // every process (`seele-ffi`), while `author_id` is derived from the
            // key on disk and never changes. So after a reconnection a person's
            // messages 1, 2, 3… are all read as retries of the *previous*
            // session's. See pendency 19; the key itself is what has to change,
            // and this is only the half that is right either way.
            if let Some(key) = message.client_message_id {
                let existing: Option<StoredRow> = transaction
                    .query_row(
                        "SELECT id, channel_id, body, created_at, edited_at, replies_to
                         FROM messages
                         WHERE author_id = ?1 AND client_message_id = ?2",
                        params![message.author.get() as i64, key.get() as i64],
                        |row| {
                            Ok((
                                row.get(0)?,
                                row.get(1)?,
                                row.get(2)?,
                                row.get(3)?,
                                row.get(4)?,
                                row.get(5)?,
                            ))
                        },
                    )
                    .optional()?;
                if let Some((id, channel, body, created_at, edited_at, replies_to)) = existing {
                    stored.push(Gravacao::Feita(StoredMessage {
                        id: MessageId(id as u64),
                        // **O canal da linha original, e não o do pedido.**
                        //
                        // Este campo lia `message.channel`, o canal que acabou
                        // de chegar, junto com o id e o corpo da linha antiga. O
                        // resultado: repetir a chave de idempotência apontando
                        // para outro canal devolvia a mensagem original
                        // carimbada com o canal novo, e o escritor difundia isso
                        // — a mensagem aparecia num canal cujo histórico está
                        // vazio, e uma busca depois não a encontrava ali. R09 da
                        // revisão da v15, reproduzido.
                        //
                        // Devolver a linha inteira como ela está é a regra deste
                        // caminho, e o comentário acima já a enunciava para o
                        // corpo. O canal era a exceção que ninguém tinha visto.
                        channel: ChannelId(channel as u32),
                        author: message.author,
                        author_nickname: message.author_nickname.clone(),
                        body,
                        created_at,
                        edited_at,
                        replies_to: replies_to.map(|id| MessageId(id as u64)),
                        client_message_id: message.client_message_id,
                        // Left empty here even for a retry that had a file, and
                        // deliberately: the transaction above holds the only
                        // borrow of the connection, and the caller that
                        // publishes a transfer re-reads the row through
                        // [`Self::one`] anyway. That is also where a retry of an
                        // attachment is recognised — the row already having a
                        // file is what stops a second one being written for the
                        // same message.
                        attachment: None,
                    }));
                    continue;
                }
            }

            // **Um ponto de retorno por mensagem.**
            //
            // Sem ele, a `execute` abaixo que falha aborta a transação inteira e
            // leva consigo o que já estava dentro dela — as mensagens válidas de
            // outras pessoas, gravadas com sucesso momentos antes no mesmo laço.
            // Era o R04: a falha de uma entrada era a perda do lote.
            //
            // `SAVEPOINT` e não uma transação por mensagem: a promessa de um
            // `fsync` por lote é o que `specs/04-servidor-seele.md` pede, e uma
            // transação por mensagem a desfaz. O ponto de retorno custa nada em
            // disco — ele só marca onde desfazer dentro da transação que já
            // existe.
            let mut ponto = match transaction.savepoint() {
                Ok(ponto) => ponto,
                Err(erro) => {
                    // Não dá para isolar esta, então ela não é tentada. As
                    // anteriores continuam de pé.
                    tracing::error!(%erro, "não deu para abrir o ponto de retorno do lote");
                    stored.push(recusa(
                        message,
                        seele_proto::control::MessageRefusal::StorageFailed,
                    ));
                    continue;
                }
            };
            let escrita = ponto.execute(
                "INSERT INTO messages
                   (channel_id, author_id, body, created_at, replies_to, client_message_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    i64::from(message.channel.get()),
                    message.author.get() as i64,
                    message.body,
                    now,
                    message.replies_to.map(|id| id.get() as i64),
                    message.client_message_id.map(|id| id.get() as i64),
                ],
            );
            let id = match escrita {
                Ok(_) => ponto.last_insert_rowid(),
                Err(erro) => {
                    let motivo = motivo_da_falha(&erro);
                    tracing::warn!(
                        %erro, ?motivo, author = %message.author, channel = %message.channel,
                        "uma mensagem do lote não foi gravada; as outras seguem"
                    );
                    // Desfaz **só** esta. `rollback` num savepoint volta ao
                    // ponto e deixa a transação aberta, que é a diferença
                    // inteira em relação ao comportamento de antes.
                    if let Err(erro) = ponto.rollback() {
                        tracing::error!(%erro, "não deu para desfazer só esta mensagem");
                    }
                    stored.push(recusa(message, motivo));
                    continue;
                }
            };
            if let Err(erro) = ponto.commit() {
                tracing::warn!(%erro, "não deu para fechar o ponto de retorno desta mensagem");
                stored.push(recusa(
                    message,
                    seele_proto::control::MessageRefusal::StorageFailed,
                ));
                continue;
            }
            stored.push(Gravacao::Feita(StoredMessage {
                id: MessageId(id as u64),
                channel: message.channel,
                author: message.author,
                author_nickname: message.author_nickname.clone(),
                body: message.body.clone(),
                created_at: now,
                edited_at: None,
                replies_to: message.replies_to,
                client_message_id: message.client_message_id,
                // Nothing yet: the row for a file is written after the bytes
                // have landed, by the transfer that brought them.
                attachment: None,
            }));
        }

        transaction.commit().context("could not commit the batch")?;
        Ok(stored)
    }

    /// Reads a page of history, newest first.
    ///
    /// `cursor` is the id to continue **before**; `None` starts at the newest.
    /// Never an offset: a message arriving between two pages would shift every
    /// later one and the reader would see a duplicate.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn history(
        &self,
        channel: ChannelId,
        cursor: Option<MessageId>,
        limit: u16,
    ) -> Result<Vec<StoredMessage>> {
        let limit = i64::from(limit.clamp(1, MAX_PAGE));
        let before = cursor.map_or(i64::MAX, |id| id.get() as i64);

        // The nickname is joined here rather than resolved per row by the
        // caller: a client reading history has never seen most of these people
        // arrive and has no other way to learn their names, and a query per
        // message would be fifty round trips through SQLite for one page.
        let mut statement = self.persistence.connection().prepare(
            "SELECT m.id, m.channel_id, m.author_id, m.body, m.created_at, m.edited_at,
                    m.replies_to, m.client_message_id, p.nickname
             FROM messages m
             JOIN people p ON p.id = m.author_id
             WHERE m.channel_id = ?1 AND m.id < ?2 AND m.deleted_at IS NULL
             ORDER BY m.id DESC
             LIMIT ?3",
        )?;

        let rows =
            statement.query_map(params![i64::from(channel.get()), before, limit], |row| {
                Ok(StoredMessage {
                    attachment: None,
                    id: MessageId(row.get::<_, i64>(0)? as u64),
                    channel: ChannelId(row.get::<_, i64>(1)? as u32),
                    author: PersonId(row.get::<_, i64>(2)? as u64),
                    author_nickname: row.get(8)?,
                    body: row.get(3)?,
                    created_at: row.get(4)?,
                    edited_at: row.get(5)?,
                    replies_to: row.get::<_, Option<i64>>(6)?.map(|id| MessageId(id as u64)),
                    client_message_id: row
                        .get::<_, Option<i64>>(7)?
                        .map(|id| ClientMessageId(id as u64)),
                })
            })?;

        let mut page: Vec<StoredMessage> = rows.filter_map(Result::ok).collect();
        // One query for the page rather than one per row.
        let ids: Vec<MessageId> = page.iter().map(|message| message.id).collect();
        let found = Attachments::new(self.persistence).for_messages(&ids)?;
        for message in &mut page {
            message.attachment = found
                .get(&message.id)
                .map(super::attachments::StoredAttachment::info);
        }
        Ok(page)
    }

    /// How many messages o canal holds, removed ones excluded.
    ///
    /// Not a page: [`Self::history`] is capped at [`MAX_PAGE`], because a client
    /// scrolls and a screen holds a screenful. The question "how many are there"
    /// has no page in it, and answering it by counting the rows of a capped page
    /// gives the cap back as if it were the answer — which is how a test that
    /// checks nothing at all comes to look green.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn count(&self, channel: ChannelId) -> Result<u64> {
        let total: i64 = self.persistence.connection().query_row(
            "SELECT COUNT(*) FROM messages WHERE channel_id = ?1 AND deleted_at IS NULL",
            params![i64::from(channel.get())],
            |row| row.get(0),
        )?;
        Ok(total.unsigned_abs())
    }

    /// Rewrites a message's body.
    ///
    /// # Errors
    ///
    /// Returns [`MessageRefusal::NotTheAuthor`] if somebody else wrote it, or
    /// [`MessageRefusal::NotFound`].
    pub fn edit(&self, id: MessageId, editor: PersonId, body: &str) -> Result<StoredMessage> {
        let author: Option<i64> = self
            .persistence
            .connection()
            .query_row(
                "SELECT author_id FROM messages WHERE id = ?1 AND deleted_at IS NULL",
                [id.get() as i64],
                |row| row.get(0),
            )
            .optional()?;

        let author = author.ok_or(MessageRefusal::NotFound)?;
        // Editing is not moderation: even a Commander does not get to put words
        // in somebody else's mouth. Removal is the moderation tool, and it is
        // visible as removal.
        if author as u64 != editor.get() {
            return Err(MessageRefusal::NotTheAuthor.into());
        }

        let now = now_seconds();
        self.persistence.connection().execute(
            "UPDATE messages SET body = ?1, edited_at = ?2 WHERE id = ?3",
            params![body, now, id.get() as i64],
        )?;

        self.one(id)?.ok_or_else(|| MessageRefusal::NotFound.into())
    }

    /// Removes a message.
    ///
    /// Soft: the row stays, the body is cleared and `deleted_at` is stamped.
    /// A hard delete would break every reply pointing at it, and
    /// `specs/02-protocolo.md` has replies.
    ///
    /// # Errors
    ///
    /// Returns [`MessageRefusal::NotFound`] if there is no such message.
    pub fn remove(&self, id: MessageId) -> Result<()> {
        let affected = self.persistence.connection().execute(
            "UPDATE messages SET body = '', deleted_at = ?1
             WHERE id = ?2 AND deleted_at IS NULL",
            params![now_seconds(), id.get() as i64],
        )?;
        if affected == 0 {
            return Err(MessageRefusal::NotFound.into());
        }
        Ok(())
    }

    /// Loads one message, if it exists and was not removed.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn one(&self, id: MessageId) -> Result<Option<StoredMessage>> {
        let mut found = self
            .persistence
            .connection()
            .query_row(
                "SELECT m.id, m.channel_id, m.author_id, m.body, m.created_at, m.edited_at,
                        m.replies_to, m.client_message_id, p.nickname
                 FROM messages m
                 JOIN people p ON p.id = m.author_id
                 WHERE m.id = ?1 AND m.deleted_at IS NULL",
                [id.get() as i64],
                |row| {
                    Ok(StoredMessage {
                        id: MessageId(row.get::<_, i64>(0)? as u64),
                        channel: ChannelId(row.get::<_, i64>(1)? as u32),
                        author: PersonId(row.get::<_, i64>(2)? as u64),
                        author_nickname: row.get(8)?,
                        body: row.get(3)?,
                        created_at: row.get(4)?,
                        edited_at: row.get(5)?,
                        replies_to: row.get::<_, Option<i64>>(6)?.map(|id| MessageId(id as u64)),
                        client_message_id: row
                            .get::<_, Option<i64>>(7)?
                            .map(|id| ClientMessageId(id as u64)),
                        attachment: None,
                    })
                },
            )
            .optional()?;
        if let Some(message) = &mut found {
            message.attachment = Attachments::new(self.persistence)
                .of_message(message.id)?
                .map(|attachment| attachment.info());
        }
        Ok(found)
    }

    /// Deletes messages older than the retention window.
    ///
    /// `specs/04-servidor-seele.md` defaults retention to unlimited, so a
    /// `retention_days` of zero does nothing at all.
    ///
    /// # Errors
    ///
    /// Fails on a database error.
    pub fn prune(&self, retention_days: u32) -> Result<usize> {
        if retention_days == 0 {
            return Ok(0);
        }
        let cutoff = now_seconds() - i64::from(retention_days) * 86_400;
        Ok(self
            .persistence
            .connection()
            .execute("DELETE FROM messages WHERE created_at < ?1", [cutoff])?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::Location;

    fn store() -> Persistence {
        let persistence = Persistence::open(&Location::Memory).unwrap();
        persistence
            .connection()
            .execute_batch(
                "INSERT INTO channels (id, name) VALUES (1, 'geral'), (2, 'logs');
                 INSERT INTO people (id, nickname, public_key, created_at)
                   VALUES (1, 'marcela', X'01', 0), (2, 'rafael', X'02', 0);",
            )
            .unwrap();
        persistence
    }

    /// As linhas que o lote gravou, para os testes que só falam delas.
    ///
    /// Quem prova a recusa casa a variante — ver
    /// `uma_entrada_invalida_nao_leva_as_validas`.
    fn gravadas(lote: Vec<Gravacao>) -> Vec<StoredMessage> {
        lote.iter().filter_map(|g| g.gravada().cloned()).collect()
    }

    fn pending(body: &str) -> PendingMessage {
        PendingMessage {
            channel: ChannelId(1),
            author: PersonId(1),
            author_nickname: "pessoa".into(),
            body: body.into(),
            replies_to: None,
            client_message_id: None,
        }
    }

    #[test]
    fn a_batch_is_written_and_comes_back_with_ids() {
        let mut persistence = store();
        let mut messages = Messages::new(&mut persistence);
        let stored = messages
            .append_batch(&[pending("um"), pending("dois"), pending("três")])
            .unwrap();

        assert_eq!(stored.len(), 3);
        let stored = gravadas(stored);
        assert_eq!(stored.len(), 3, "uma mensagem válida do lote foi recusada");
        let ids: Vec<u64> = stored.iter().map(|m| m.id.get()).collect();
        assert_eq!(ids, vec![1, 2, 3], "ids must be assigned in order");
    }

    #[test]
    fn uma_entrada_invalida_nao_leva_as_validas() {
        // **R04, e o oráculo é o inverso do da sonda da revisão.** Lá «passou»
        // queria dizer «as duas se perderam». Aqui passa quando a válida fica.
        //
        // A prova de que o guarda é o savepoint e não a validação do
        // `session.rs`: este caminho é o do escritor, e ele não confere canal
        // nenhum. Tirar o savepoint faz este teste reprovar.
        let mut persistence = store();
        let mut messages = Messages::new(&mut persistence);

        let lote = messages
            .append_batch(&[
                pending("da marcela, para um canal que existe"),
                PendingMessage {
                    channel: ChannelId(404),
                    author: PersonId(2),
                    ..pending("do rafael, para um canal que não existe")
                },
                PendingMessage {
                    author: PersonId(2),
                    ..pending("do rafael, para um canal que existe")
                },
            ])
            .unwrap();

        assert_eq!(lote.len(), 3, "o lote deixou de responder por registro");
        let corpos: Vec<&str> = lote
            .iter()
            .filter_map(|g| g.gravada().map(|linha| linha.body.as_str()))
            .collect();
        assert_eq!(
            corpos,
            vec![
                "da marcela, para um canal que existe",
                "do rafael, para um canal que existe",
            ],
            "uma entrada inválida levou embora as mensagens válidas de outras              pessoas do mesmo lote"
        );

        // E a recusa é identificada: quem escreveu tem de poder encontrar
        // **qual** mensagem dele não entrou.
        let Some(Gravacao::Falhou {
            channel,
            author,
            motivo,
            ..
        }) = lote.get(1)
        else {
            panic!("a entrada inválida foi dada como gravada");
        };
        assert_eq!(*channel, ChannelId(404));
        assert_eq!(*author, PersonId(2));
        assert_eq!(
            *motivo,
            seele_proto::control::MessageRefusal::NoSuchChannel,
            "um destino que não existe foi classificado como falha transitória:              o cliente vai repetir para sempre uma entrada que nunca passa"
        );

        // O disco concorda com a resposta. Sem esta metade, um `stored` montado
        // certo sobre uma transação desfeita passaria.
        let page = messages.history(ChannelId(1), None, 50).unwrap();
        assert_eq!(page.len(), 2);
    }

    #[test]
    fn a_chave_repetida_devolve_o_canal_da_linha_original() {
        // **R09**, reproduzido no review: gravar no canal 1 e repetir a chave
        // apontando para o canal 2 devolvia a linha original carimbada com o
        // canal 2. O escritor difunde esse resultado, então a mensagem apareceria
        // num canal cujo histórico não a tem.
        let mut persistence = store();
        let mut messages = Messages::new(&mut persistence);
        let key = Some(ClientMessageId(11));

        let primeira = gravadas(
            messages
                .append_batch(&[PendingMessage {
                    channel: ChannelId(1),
                    client_message_id: key,
                    ..pending("no canal um")
                }])
                .unwrap(),
        );
        let original = primeira.first().expect("gravou").clone();

        let repetida = gravadas(
            messages
                .append_batch(&[PendingMessage {
                    channel: ChannelId(2),
                    client_message_id: key,
                    ..pending("no canal dois")
                }])
                .unwrap(),
        );
        let resposta = repetida.first().expect("a repetição respondeu");

        assert_eq!(
            resposta.id, original.id,
            "a repetição gravou uma segunda vez"
        );
        assert_eq!(
            resposta.channel,
            ChannelId(1),
            "a resposta saiu com o canal do pedido novo: a difusão poria a              mensagem num canal cujo histórico está vazio"
        );
        assert_eq!(resposta.body, "no canal um");
        assert!(
            messages.history(ChannelId(2), None, 50).unwrap().is_empty(),
            "a repetição gravou algo no canal dois"
        );
    }

    #[test]
    fn an_empty_batch_touches_nothing() {
        let mut persistence = store();
        let mut messages = Messages::new(&mut persistence);
        assert!(messages.append_batch(&[]).unwrap().is_empty());
    }

    #[test]
    fn a_retried_send_does_not_post_twice() {
        // specs/02-protocolo.md: "idempotente por client_msg_id". Gap G9 gave
        // the field a home; this is what it is for. Without it, a client that
        // resends after a lost acknowledgement duplicates the message.
        let mut persistence = store();
        let mut messages = Messages::new(&mut persistence);
        let with_key = PendingMessage {
            client_message_id: Some(ClientMessageId(42)),
            ..pending("verificando harmônicos")
        };

        let first = messages
            .append_batch(std::slice::from_ref(&with_key))
            .unwrap();
        let second = messages.append_batch(&[with_key]).unwrap();

        let first = gravadas(first);
        let second = gravadas(second);
        assert_eq!(first.first().map(|m| m.id), second.first().map(|m| m.id));
        assert_eq!(messages.history(ChannelId(1), None, 50).unwrap().len(), 1);
    }

    #[test]
    fn a_key_that_comes_back_with_another_body_answers_with_the_one_on_disk() {
        // The test above uses the same body twice, so it never sees this: what
        // came back was the *incoming* message wearing the stored row's id.
        // Equal bodies made the substitution invisible.
        //
        // Different bodies is not a hypothetical. `client_message_id` restarts
        // at 1 every session, and every process in `seele-ffi`,
        // while `author_id` is derived from the key on disk and never changes —
        // so after a reconnection message 1 of the new session is read as a
        // retry of message 1 of the old one, and the bodies have nothing to do
        // with each other (pendency 19).
        //
        // What must not happen is the halves diverging: the new body announced
        // live under an id whose row on disk holds the old text. Whoever has the
        // window open and whoever opens it a minute later would be reading two
        // different messages with the same id, and nothing anywhere would say so.
        let mut persistence = store();
        let mut messages = Messages::new(&mut persistence);
        let key = Some(ClientMessageId(7));

        let first = gravadas(
            messages
                .append_batch(&[PendingMessage {
                    client_message_id: key,
                    ..pending("padrão azul confirmado")
                }])
                .unwrap(),
        );

        // Edited before the collision, and this is what makes the check bite.
        // `created_at` cannot tell the two sources apart here — both writes land
        // in the same second, so `now` and the stored stamp are equal whichever
        // one the answer was built from, and a mutation swapping them survives.
        // `edited_at` has no such tie: on disk it is `Some` and on the incoming
        // message it is structurally `None`.
        let edited = messages
            .edit(
                first.first().expect("the first send stored something").id,
                PersonId(1),
                "padrão azul confirmado",
            )
            .unwrap();

        let again = gravadas(
            messages
                .append_batch(&[PendingMessage {
                    client_message_id: key,
                    ..pending("ISTO É DE OUTRA SESSÃO")
                }])
                .unwrap(),
        );

        let stored = again.first().expect("the retry answers with something");
        assert_eq!(
            stored.body, "padrão azul confirmado",
            "the answer carries a body that is on nobody's disk: live listeners \
             would read it and history would never show it"
        );
        assert_eq!(first.first().map(|m| m.id), Some(stored.id));
        assert_eq!(
            stored.edited_at, edited.edited_at,
            "the answer says the message was never edited, and the row on disk \
             says it was: the answer is still being built from what arrived"
        );

        // And the disk agrees with what was answered — the half that would still
        // be wrong if the answer were built from the incoming message.
        let page = messages.history(ChannelId(1), None, 50).unwrap();
        assert_eq!(page.len(), 1, "the second send must not have been written");
        assert_eq!(
            page.first().map(|m| m.body.as_str()),
            Some(stored.body.as_str())
        );
    }

    #[test]
    fn two_authors_may_use_the_same_key() {
        // The key is the client's, not the server's. Two clients choosing 1
        // independently is ordinary and must not collide.
        let mut persistence = store();
        let mut messages = Messages::new(&mut persistence);
        messages
            .append_batch(&[
                PendingMessage {
                    client_message_id: Some(ClientMessageId(1)),
                    ..pending("de marcela")
                },
                PendingMessage {
                    author: PersonId(2),
                    client_message_id: Some(ClientMessageId(1)),
                    ..pending("de rafael")
                },
            ])
            .unwrap();

        assert_eq!(messages.history(ChannelId(1), None, 50).unwrap().len(), 2);
    }

    #[test]
    fn history_comes_back_newest_first() {
        let mut persistence = store();
        let mut messages = Messages::new(&mut persistence);
        messages
            .append_batch(&[pending("um"), pending("dois"), pending("três")])
            .unwrap();

        let page = messages.history(ChannelId(1), None, 50).unwrap();
        let bodies: Vec<&str> = page.iter().map(|m| m.body.as_str()).collect();
        assert_eq!(bodies, vec!["três", "dois", "um"]);
    }

    #[test]
    fn the_cursor_walks_backwards_without_gaps_or_repeats() {
        let mut persistence = store();
        let mut messages = Messages::new(&mut persistence);
        let batch: Vec<PendingMessage> = (1..=10)
            .map(|index| pending(&format!("mensagem {index}")))
            .collect();
        messages.append_batch(&batch).unwrap();

        let mut seen = Vec::new();
        let mut cursor = None;
        loop {
            let page = messages.history(ChannelId(1), cursor, 3).unwrap();
            if page.is_empty() {
                break;
            }
            cursor = page.last().map(|m| m.id);
            seen.extend(page.into_iter().map(|m| m.body));
        }

        assert_eq!(seen.len(), 10, "the walk lost or repeated messages");
        let unique: std::collections::HashSet<&String> = seen.iter().collect();
        assert_eq!(unique.len(), 10);
    }

    #[test]
    fn a_message_arriving_mid_walk_does_not_shift_the_page() {
        // The reason specs/02-protocolo.md says "nunca offset". With an offset,
        // inserting at the head pushes everything down and the reader sees the
        // same message twice.
        let mut persistence = store();
        let mut messages = Messages::new(&mut persistence);
        messages
            .append_batch(
                &(1..=6)
                    .map(|i| pending(&format!("{i}")))
                    .collect::<Vec<_>>(),
            )
            .unwrap();

        let first_page = messages.history(ChannelId(1), None, 3).unwrap();
        let cursor = first_page.last().map(|m| m.id);

        // Somebody speaks while the reader is paging.
        messages.append_batch(&[pending("nova")]).unwrap();

        let second_page = messages.history(ChannelId(1), cursor, 3).unwrap();
        let bodies: Vec<&str> = second_page.iter().map(|m| m.body.as_str()).collect();
        assert_eq!(bodies, vec!["3", "2", "1"], "the page shifted");
        assert!(
            !second_page.iter().any(|m| m.body == "nova"),
            "a newer message appeared in an older page"
        );
    }

    #[test]
    fn lines_do_not_leak_into_each_other() {
        let mut persistence = store();
        let mut messages = Messages::new(&mut persistence);
        messages
            .append_batch(&[
                pending("geral"),
                PendingMessage {
                    channel: ChannelId(2),
                    ..pending("logs")
                },
            ])
            .unwrap();

        let geral = messages.history(ChannelId(1), None, 50).unwrap();
        assert_eq!(geral.len(), 1);
        assert_eq!(geral.first().map(|m| m.body.as_str()), Some("geral"));
    }

    #[test]
    fn a_page_is_bounded_however_much_is_asked_for() {
        // One FetchHistory must not be able to ask for a reply larger than the
        // frame it has to fit in.
        let mut persistence = store();
        let mut messages = Messages::new(&mut persistence);
        let batch: Vec<PendingMessage> = (0..300).map(|i| pending(&format!("{i}"))).collect();
        messages.append_batch(&batch).unwrap();

        let page = messages.history(ChannelId(1), None, u16::MAX).unwrap();
        assert_eq!(page.len(), MAX_PAGE as usize);
    }

    #[test]
    fn only_the_author_may_edit() {
        // Editing is not moderation. Even a Commander does not get to put words
        // in somebody else's mouth — removal is the moderation tool, and it is
        // visible as removal.
        let mut persistence = store();
        let mut messages = Messages::new(&mut persistence);
        let stored = gravadas(messages.append_batch(&[pending("original")]).unwrap());
        let id = stored.first().map(|m| m.id).unwrap();

        assert!(messages.edit(id, PersonId(2), "sequestrado").is_err());
        let edited = messages.edit(id, PersonId(1), "corrigido").unwrap();
        assert_eq!(edited.body, "corrigido");
        assert!(edited.edited_at.is_some());
    }

    #[test]
    fn a_removed_message_leaves_history_but_not_its_body() {
        // Soft delete: a hard one would break every reply pointing at it, and
        // specs/02-protocolo.md has replies.
        let mut persistence = store();
        let mut messages = Messages::new(&mut persistence);
        let stored = gravadas(
            messages
                .append_batch(&[pending("apagar"), pending("fica")])
                .unwrap(),
        );
        let id = stored.first().map(|m| m.id).unwrap();

        messages.remove(id).unwrap();

        assert_eq!(messages.history(ChannelId(1), None, 50).unwrap().len(), 1);
        assert!(messages.one(id).unwrap().is_none());
        // The row survives, so a reply pointing at it still resolves.
        let count: i64 = persistence
            .connection()
            .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn removing_twice_is_refused_rather_than_silently_fine() {
        let mut persistence = store();
        let mut messages = Messages::new(&mut persistence);
        let stored = gravadas(messages.append_batch(&[pending("um")]).unwrap());
        let id = stored.first().map(|m| m.id).unwrap();

        messages.remove(id).unwrap();
        assert!(messages.remove(id).is_err());
    }

    #[test]
    fn a_reply_keeps_pointing_at_its_parent() {
        let mut persistence = store();
        let mut messages = Messages::new(&mut persistence);
        let parent = gravadas(messages.append_batch(&[pending("pergunta")]).unwrap());
        let parent_id = parent.first().map(|m| m.id).unwrap();

        let reply = gravadas(
            messages
                .append_batch(&[PendingMessage {
                    replies_to: Some(parent_id),
                    ..pending("resposta")
                }])
                .unwrap(),
        );

        assert_eq!(reply.first().and_then(|m| m.replies_to), Some(parent_id));
    }

    #[test]
    fn a_page_of_history_carries_the_files_hanging_off_it() {
        // Joined once for the page rather than asked per row: a screenful is
        // fifty round trips through SQLite otherwise, and the shell has no way
        // to know a message has a file until it is told.
        use crate::persistence::attachments::Attachments;

        let mut persistence = store();
        let stored = gravadas(
            Messages::new(&mut persistence)
                .append_batch(&[pending("com foto"), pending("sem nada")])
                .unwrap(),
        );
        let com_foto = stored[0].id;
        Attachments::new(&persistence)
            .record(com_foto, &"a".repeat(64), "foto.png", "image/png", 2_048)
            .unwrap();

        let page = Messages::new(&mut persistence)
            .history(ChannelId(1), None, 50)
            .unwrap();
        let com = page.iter().find(|m| m.id == com_foto).expect("a mensagem");
        let anexo = com.attachment.as_ref().expect("o anexo veio junto");
        assert_eq!(anexo.file_name, "foto.png");
        assert_eq!(anexo.byte_size, 2_048);
        assert_eq!(
            anexo.state,
            seele_proto::control::AttachmentState::Available
        );
        assert!(
            page.iter()
                .find(|m| m.id != com_foto)
                .and_then(|m| m.attachment.as_ref())
                .is_none(),
            "uma mensagem sem arquivo ganhou um"
        );
    }

    #[test]
    fn the_text_survives_the_file_and_the_page_still_says_what_it_was() {
        // ADR 0027, and this is where a reader meets it: the bytes are gone,
        // the row is not, and the page carries the name and the size with a
        // state that says «expirou». A `None` here would draw as a message with
        // nothing in it, and nobody would learn a file had been there.
        use crate::persistence::attachments::Attachments;

        let mut persistence = store();
        let stored = gravadas(
            Messages::new(&mut persistence)
                .append_batch(&[pending("olha isto")])
                .unwrap(),
        );
        let id = stored[0].id;
        let anexo = Attachments::new(&persistence)
            .record(id, &"b".repeat(64), "recibo.pdf", "application/pdf", 900)
            .unwrap();
        Attachments::new(&persistence).expire(anexo.id).unwrap();

        let page = Messages::new(&mut persistence)
            .history(ChannelId(1), None, 50)
            .unwrap();
        let mensagem = page.first().expect("a mensagem continua no histórico");
        assert_eq!(
            mensagem.body, "olha isto",
            "o texto foi embora com o arquivo"
        );
        let carregado = mensagem.attachment.as_ref().expect("a linha sobreviveu");
        assert_eq!(
            carregado.state,
            seele_proto::control::AttachmentState::Expired
        );
        assert_eq!(carregado.file_name, "recibo.pdf");
        assert_eq!(carregado.byte_size, 900);
    }

    #[test]
    fn unlimited_retention_prunes_nothing() {
        // specs/04-servidor-seele.md defaults to unlimited. A sweep that deleted
        // anything at the default would be a data-loss bug in a config nobody
        // touched.
        let mut persistence = store();
        let mut messages = Messages::new(&mut persistence);
        messages.append_batch(&[pending("um")]).unwrap();
        assert_eq!(messages.prune(0).unwrap(), 0);
        assert_eq!(messages.history(ChannelId(1), None, 50).unwrap().len(), 1);
    }

    #[test]
    fn messages_survive_a_reopen() {
        // specs/04-servidor-seele.md acceptance: "Reinício não perde mensagem
        // confirmada ao cliente". `append_batch` returns only after the commit,
        // so anything it returned is durable by construction.
        let directory = tempfile::tempdir().unwrap();
        let location = Location::File(directory.path().join("seele.db"));

        {
            let mut persistence = Persistence::open(&location).unwrap();
            persistence
                .connection()
                .execute_batch(
                    "INSERT INTO channels (id, name) VALUES (1, 'geral');
                     INSERT INTO people (id, nickname, public_key, created_at)
                       VALUES (1, 'marcela', X'01', 0);",
                )
                .unwrap();
            let mut messages = Messages::new(&mut persistence);
            messages
                .append_batch(&[pending("sobrevive ao reinício")])
                .unwrap();
        }

        let mut persistence = Persistence::open(&location).unwrap();
        let messages = Messages::new(&mut persistence);
        let history = messages.history(ChannelId(1), None, 50).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(
            history.first().map(|m| m.body.as_str()),
            Some("sobrevive ao reinício")
        );
    }
}
