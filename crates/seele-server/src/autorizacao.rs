//! Quem pode ler e escrever **em qual canal** — uma pergunta, um lugar.
//!
//! `specs/08-seguranca.md`: «Toda ação é verificada no servidor, sempre, mesmo
//! que o cliente já esconda o botão.»
//!
//! # Por que este módulo existe
//!
//! Porque a pergunta estava respondida em quatro lugares e em três deles a
//! resposta era «sim». Antes disto:
//!
//! - `FetchHistory` entregava a página sem conferir nada;
//! - `JoinChannel` aceitava a assinatura sem conferir nada;
//! - a difusão conferia só se **esta conexão assinou o canal** — e a assinatura
//!   era ela mesma sem conferência, então o guarda testava a intenção de quem
//!   pedia e não a permissão de quem pediu;
//! - `FetchAttachment` conferia [`Permission::ReadChannel`] global e não o
//!   canal de onde o arquivo pendura.
//!
//! Reproduzido no review da v15 (R01): tirei o papel de uma conta, reconectei
//! com a mesma chave, `SessionInfo.permissions` veio sem `ReadChannel`, e o
//! histórico chegou igual. Conta autenticada, sem a permissão.
//!
//! O conserto não é um `if` a mais em cada braço: é **um** caminho que os
//! quatro atravessam, porque quatro cópias de um guarda são três cópias que
//! alguém vai esquecer de atualizar.
//!
//! # «Papel mínimo», dito por extenso
//!
//! `specs/04-servidor-seele.md` dá à Linha um «papel mínimo de leitura/escrita»
//! e as colunas existem desde a migração 1 (`minimum_read_role`,
//! `minimum_write_role`). Nada as lia. O que a spec **não** diz é o que
//! «mínimo» ordena, e `permissions` é explícito sobre não haver herança em
//! árvore — então não há ordem para um mínimo percorrer.
//!
//! A semântica escolhida, e ela está aqui porque precisa estar escrita em algum
//! lugar: **o papel exigido é um papel que a pessoa tem de ter**, e quem tem
//! [`Permission::AdministerServer`] passa sempre. A segunda metade não é
//! conveniência — sem ela, um Comandante pode criar um canal cujo papel mínimo
//! ele não possui e perder o próprio servidor de vista, sem ninguém para
//! desfazer.
//!
//! Coluna vazia — que é o caso de **todo** canal existente hoje, porque nada
//! escreve nessas colunas — quer dizer «só a permissão global», que é o
//! comportamento de antes deste módulo para quem tem a permissão.
//!
//! # A resposta é a de agora, nunca a do aperto de mão
//!
//! Cada chamada toma o mutex do PERSISTENCE. É o ponto: R08 do mesmo review é
//! exatamente uma sessão publicando texto com um papel que já havia sido
//! revogado, porque `may_write` foi calculado uma vez, no aperto de mão, e
//! consultado para sempre. Um erro de banco lê como negação — um servidor com
//! disco falhando não pode passar a liberar leitura de canal privado.

use anyhow::Result;
use seele_proto::control::Permission;
use seele_proto::ids::{AttachmentId, ChannelId, PersonId};

use crate::permissions::Permissions;
use crate::persistence::Persistence;

/// O que se quer fazer num canal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acesso {
    /// Ler o histórico, receber a difusão, baixar um anexo.
    Leitura,
    /// Publicar, editar, apagar.
    Escrita,
}

impl Acesso {
    /// A permissão global que este acesso exige.
    const fn permissao(self) -> Permission {
        match self {
            Self::Leitura => Permission::ReadChannel,
            Self::Escrita => Permission::WriteChannel,
        }
    }

    /// A coluna de papel mínimo que este acesso consulta.
    const fn coluna(self) -> &'static str {
        match self {
            Self::Leitura => "minimum_read_role",
            Self::Escrita => "minimum_write_role",
        }
    }
}

/// Se esta pessoa pode fazer isto **neste canal**, agora.
///
/// Três perguntas, e as três têm de ser sim:
///
/// 1. o canal existe — um canal que não existe não é um canal sem papel mínimo,
///    é um destino inválido, e responder «sim» a ele é o que fazia um lote
///    inteiro de mensagens de outras pessoas cair na chave estrangeira (R04);
/// 2. a permissão global está de pé, lida agora;
/// 3. o papel mínimo do canal, se houver, é um papel que a pessoa tem — ou ela
///    administra o servidor.
///
/// # Errors
///
/// Falha de banco. Quem chama trata falha como negação; a falha é devolvida em
/// vez de engolida para que o log do operador saiba a diferença entre «não
/// pode» e «o disco não respondeu».
pub fn pode_no_canal(
    persistence: &Persistence,
    person: PersonId,
    channel: ChannelId,
    acesso: Acesso,
) -> Result<bool> {
    if !canal_existe(persistence, channel)? {
        return Ok(false);
    }

    let permissions = Permissions::new(persistence);
    if !permissions.may(person, acesso.permissao())? {
        return Ok(false);
    }

    let Some(exigido) = papel_minimo(persistence, channel, acesso)? else {
        return Ok(true);
    };

    if permissions.may(person, Permission::AdministerServer)? {
        return Ok(true);
    }

    let tem: bool = persistence.connection().query_row(
        "SELECT EXISTS(
             SELECT 1 FROM person_roles WHERE person_id = ?1 AND role_id = ?2
         )",
        rusqlite::params![person.get() as i64, exigido],
        |row| row.get(0),
    )?;
    Ok(tem)
}

/// Se este canal está no banco.
///
/// Público porque `SendMessage` precisa da resposta **antes** de enfileirar,
/// e ali a pergunta não é sobre papel: é sobre um destino existir. Ver R04.
///
/// # Errors
///
/// Falha de banco.
pub fn canal_existe(persistence: &Persistence, channel: ChannelId) -> Result<bool> {
    Ok(persistence.connection().query_row(
        "SELECT EXISTS(SELECT 1 FROM channels WHERE id = ?1)",
        [i64::from(channel.get())],
        |row| row.get(0),
    )?)
}

/// De qual canal este anexo pendura, se ele existe.
///
/// Um arquivo é parte da mensagem que o carrega, e a mensagem é de um canal.
/// Sem esta resolução, conferir «pode ler canal» em geral libera o anexo de um
/// canal que a pessoa não pode ler — que é a metade do R01 que sobra depois de
/// consertar o histórico.
///
/// # Errors
///
/// Falha de banco.
pub fn canal_do_anexo(
    persistence: &Persistence,
    attachment: AttachmentId,
) -> Result<Option<ChannelId>> {
    use rusqlite::OptionalExtension;

    Ok(persistence
        .connection()
        .query_row(
            "SELECT m.channel_id FROM attachments a
             JOIN messages m ON m.id = a.message_id
             WHERE a.id = ?1",
            [attachment.get() as i64],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .map(|id| ChannelId(id as u32)))
}

/// Filtra uma lista de canais pelos que esta pessoa pode ler agora.
///
/// Usada na difusão, que precisa da resposta por canal e não por conexão. Um
/// canal cuja consulta falha sai da lista: o lado fechado é o lado certo aqui.
pub fn canais_legiveis(
    persistence: &Persistence,
    person: PersonId,
    canais: &[ChannelId],
) -> Vec<ChannelId> {
    canais
        .iter()
        .copied()
        .filter(|channel| {
            pode_no_canal(persistence, person, *channel, Acesso::Leitura).unwrap_or(false)
        })
        .collect()
}

/// O papel mínimo que a coluna deste canal declara.
fn papel_minimo(
    persistence: &Persistence,
    channel: ChannelId,
    acesso: Acesso,
) -> Result<Option<i64>> {
    use rusqlite::OptionalExtension;

    // O nome da coluna é interpolado e **não** pode ser: ele vem de
    // `Acesso::coluna`, que é um `const fn` sobre duas variantes de um enum
    // deste módulo. Não há caminho de um byte de fora até aqui.
    let sql = format!("SELECT {} FROM channels WHERE id = ?1", acesso.coluna());
    Ok(persistence
        .connection()
        .query_row(&sql, [i64::from(channel.get())], |row| {
            row.get::<_, Option<i64>>(0)
        })
        .optional()?
        .flatten())
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::permissions::{OBSERVER_ROLE, PERSON_ROLE};

    /// Um banco com dois canais e **o comando já tomado**.
    ///
    /// A primeira conta de um servidor assume o Comandante — ver
    /// `Permissions::claim_never_held_commandership` — e um Comandante passa
    /// por todo papel mínimo. Sem esta conta de fachada, toda pessoa criada num
    /// teste seria Comandante e nenhum guarda daqui teria como recusar nada:
    /// os testes passariam dizendo o contrário do que provam.
    fn banco() -> Persistence {
        let persistence = Persistence::open(&crate::persistence::Location::Memory).expect("abrir");
        persistence
            .connection()
            .execute_batch("INSERT INTO channels (id, name) VALUES (1, 'geral'), (2, 'fechado');")
            .expect("canais");
        let _ = pessoa(&persistence, "quem-hospeda");
        persistence
    }

    fn pessoa(persistence: &Persistence, apelido: &str) -> PersonId {
        Permissions::new(persistence)
            .register_or_find(apelido.as_bytes(), apelido)
            .expect("conta")
            .id
    }

    #[test]
    fn sem_a_permissao_global_nao_se_le_canal_nenhum() {
        // R01, e o oráculo é o inverso do da sonda do review: lá «passou»
        // queria dizer «o histórico chegou». Aqui passa quando ele não chega.
        let banco = banco();
        let alguem = pessoa(&banco, "pessoa");
        let permissions = Permissions::new(&banco);
        permissions.revoke_role(alguem, PERSON_ROLE).expect("tirar");

        assert!(!pode_no_canal(&banco, alguem, ChannelId(1), Acesso::Leitura).expect("consulta"));
        assert!(!pode_no_canal(&banco, alguem, ChannelId(1), Acesso::Escrita).expect("consulta"));
    }

    #[test]
    fn com_a_permissao_global_e_sem_papel_minimo_se_le() {
        // O caso comum, que é todo canal que existe hoje: coluna vazia quer
        // dizer «só a permissão global».
        let banco = banco();
        let alguem = pessoa(&banco, "pessoa");
        assert!(pode_no_canal(&banco, alguem, ChannelId(1), Acesso::Leitura).expect("consulta"));
    }

    #[test]
    fn observador_le_e_nao_escreve() {
        let banco = banco();
        let alguem = pessoa(&banco, "observa");
        let permissions = Permissions::new(&banco);
        permissions.revoke_role(alguem, PERSON_ROLE).expect("tirar");
        permissions.grant_role(alguem, OBSERVER_ROLE).expect("dar");

        assert!(pode_no_canal(&banco, alguem, ChannelId(1), Acesso::Leitura).expect("consulta"));
        assert!(!pode_no_canal(&banco, alguem, ChannelId(1), Acesso::Escrita).expect("consulta"));
    }

    #[test]
    fn um_canal_que_nao_existe_nao_e_um_canal_sem_papel_minimo() {
        // R04: é esta resposta que impede o lote inteiro de morrer na chave
        // estrangeira por causa de um destino inventado.
        let banco = banco();
        let alguem = pessoa(&banco, "pessoa");
        assert!(!pode_no_canal(&banco, alguem, ChannelId(99), Acesso::Escrita).expect("consulta"));
        assert!(!canal_existe(&banco, ChannelId(99)).expect("consulta"));
    }

    #[test]
    fn o_papel_minimo_do_canal_e_aplicado() {
        let banco = banco();
        let alguem = pessoa(&banco, "pessoa");
        banco
            .connection()
            .execute(
                "UPDATE channels SET minimum_read_role = ?1 WHERE id = 2",
                [i64::from(OBSERVER_ROLE.get())],
            )
            .expect("papel mínimo");

        // Tem `ReadChannel` pelo papel Pessoa, e não tem o papel exigido.
        assert!(pode_no_canal(&banco, alguem, ChannelId(1), Acesso::Leitura).expect("consulta"));
        assert!(!pode_no_canal(&banco, alguem, ChannelId(2), Acesso::Leitura).expect("consulta"));

        Permissions::new(&banco)
            .grant_role(alguem, OBSERVER_ROLE)
            .expect("dar");
        assert!(pode_no_canal(&banco, alguem, ChannelId(2), Acesso::Leitura).expect("consulta"));
    }

    #[test]
    fn quem_administra_o_servidor_nao_se_tranca_fora() {
        // Sem isto, criar um canal com papel mínimo que o próprio Comandante
        // não tem é perder o canal para sempre, sem ninguém para desfazer.
        let banco = banco();
        let comandante = pessoa(&banco, "quem-hospeda");
        banco
            .connection()
            .execute(
                "UPDATE channels SET minimum_read_role = ?1 WHERE id = 2",
                [i64::from(OBSERVER_ROLE.get())],
            )
            .expect("papel mínimo");

        assert!(
            Permissions::new(&banco)
                .may(comandante, Permission::AdministerServer)
                .expect("consulta"),
            "a primeira conta deste banco assume o comando"
        );
        assert!(
            pode_no_canal(&banco, comandante, ChannelId(2), Acesso::Leitura).expect("consulta")
        );
    }

    #[test]
    fn a_difusao_filtra_por_canal() {
        let banco = banco();
        let alguem = pessoa(&banco, "pessoa");
        banco
            .connection()
            .execute(
                "UPDATE channels SET minimum_read_role = ?1 WHERE id = 2",
                [i64::from(OBSERVER_ROLE.get())],
            )
            .expect("papel mínimo");

        let legiveis = canais_legiveis(&banco, alguem, &[ChannelId(1), ChannelId(2)]);
        assert_eq!(legiveis, vec![ChannelId(1)]);
    }
}
