//! PERMISSIONS — identidade, cargos, permissões e banimentos.
//!
//! # Desde a 0.17: os cargos (ADR 0064)
//!
//! Os quatro papéis de `specs/04-servidor-seele.md` (Comandante, Operador,
//! Pessoa, Observador) viraram **cargos**, com posição, um `@everyone` e a regra
//! de `seele_proto::cargos`: a soma das concessões menos as restrições, o
//! Administrador, o proprietário e a hierarquia. Este módulo continua sendo a
//! porta de quem pergunta `may` — a assinatura não mudou —, e a resposta passou
//! a vir de `persistence::cargos` + `seele_proto::cargos`, que é uma regra num
//! lugar só.
//!
//! «Negadas vencem concedidas» continua valendo, com nome: é a **restrição de
//! cargo**. A migração 18 preserva o efeito de cada combinação dos quatro
//! papéis (provado em `cargos::testes`).
//!
//! # Chegar nunca dá autoridade
//!
//! Até a 0.16, a primeira conta criada num servidor virava Comandante, e uma
//! conta antiga que reconectava podia tomar o assento se ele «nunca tivesse
//! sido de ninguém». As duas coisas **saíram** (revisão do Codex, 24/10/2026):
//! eram autoridade dada pela ordem da rede, e havia uma escalada — com o
//! Comandante revogado de todos, a próxima conta NOVA o tomava, porque
//! `seat_the_arrival` não olhava a marca. O dono, agora, é DECLARADO pela
//! máquina que hospeda (`cargos::declarar_proprietario`), e toda conta nova
//! chega com o Pessoa e só.
//!
//! # Toda conferência acontece no servidor
//!
//! `specs/08-seguranca.md`: "Toda ação é verificada no servidor, sempre, mesmo
//! que o cliente já esconda o botão."

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use seele_proto::control::{Membro, Permission};
use seele_proto::ids::{PersonId, RoleId};

use crate::persistence::cargos as banco;
use crate::persistence::{now_seconds, Persistence};

/// O cargo Comandante, semeado pela migração 1.
pub const COMMANDER_ROLE: RoleId = RoleId(1);
/// O cargo Operador.
pub const OPERATOR_ROLE: RoleId = RoleId(2);
/// O cargo Pessoa: o cargo de chegada de toda conta nova.
pub const PERSON_ROLE: RoleId = RoleId(3);
/// O cargo Observador: ouvir e ler, e as restrições que silenciam.
pub const OBSERVER_ROLE: RoleId = RoleId(4);

/// Uma pessoa como PERMISSIONS a conhece.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    /// A conta.
    pub id: PersonId,
    /// O apelido.
    pub nickname: String,
    /// Os cargos atribuídos (fora o `@everyone`, que todos têm).
    pub roles: Vec<RoleId>,
}

/// Por que uma ação foi recusada.
///
/// Enumerado, por `specs/02-protocolo.md`. A casca casa com a variante.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    /// A pessoa não tem a permissão (ou a hierarquia não deixa).
    #[error("permission denied")]
    PermissionDenied,
    /// A pessoa está banida.
    #[error("banned")]
    Banned,
    /// Não há essa conta.
    #[error("no such person")]
    UnknownPerson,
    /// O apelido é de outra chave.
    ///
    /// `specs/08-seguranca.md` quer falhas de entrada uniformes, então isto só
    /// chega a quem opera — nunca ao par que a causou.
    #[error("nickname belongs to a different identity")]
    NicknameTaken,
}

/// As permissões do produto numa lista JSON gravada — as que esta build
/// conhece; o resto é descartado, e descartar é não conceder.
///
/// Pública porque o aperto de mão monta a lista de `Role` do `Session` a
/// partir da mesma coluna.
#[must_use]
pub fn permissions_from_json(json: &str) -> Vec<Permission> {
    banco::lista_gravada(json)
        .into_iter()
        .filter_map(|permissao| match permissao {
            seele_proto::cargos::Permissao::Produto(permissao) => Some(permissao),
            seele_proto::cargos::Permissao::DeMod(_) => None,
        })
        .collect()
}

/// Identidade e autorização, sobre a PERSISTENCE.
pub struct Permissions<'a> {
    connection: &'a Connection,
}

impl<'a> Permissions<'a> {
    /// Empresta um banco.
    #[must_use]
    pub fn new(persistence: &'a Persistence) -> Self {
        Self {
            connection: persistence.connection(),
        }
    }

    /// Acha a conta desta chave, criando-a na primeira vez.
    ///
    /// O ADR 0004 faz da chave a identidade, e do apelido um rótulo preso a
    /// ela. Quem volta mantém a **conta** qualquer que seja o nome pedido, e o
    /// nome pedido vira o rótulo. Uma chave **diferente** pedindo um nome
    /// tomado é recusada, e é essa recusa que impede renomear de ser o jeito de
    /// tomar o nome de alguém.
    ///
    /// # Errors
    ///
    /// Falha de banco ou esgotamento da reserva de tags.
    pub fn register_or_find(&self, public_key: &[u8], nickname: &str) -> Result<Person> {
        self.registrar_ou_achar(public_key, nickname)
            .map(|(person, _criou)| person)
    }

    /// O mesmo de [`Self::register_or_find`], dizendo se a conta **foi criada
    /// agora** — o FORA DO AR (D-fio-4) anuncia um membro novo só quando ele é
    /// novo.
    ///
    /// **Nenhum dos dois caminhos dá autoridade.** Quem volta não reassume nada
    /// (a reconexão não desfaz revogação), e quem chega recebe só o cargo de
    /// chegada. O dono é declarado pela máquina que hospeda.
    ///
    /// # Errors
    ///
    /// Os mesmos de [`Self::register_or_find`].
    pub fn registrar_ou_achar(&self, public_key: &[u8], nickname: &str) -> Result<(Person, bool)> {
        if let Some(id) = self
            .connection
            .query_row(
                "SELECT id FROM people WHERE public_key = ?1",
                [public_key],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
        {
            self.connection.execute(
                "UPDATE people SET last_seen_at = ?1 WHERE id = ?2",
                params![now_seconds(), id],
            )?;
            // O apelido pedido passa a valer (conserto da 0.9: a conta que
            // voltava mantinha o nome antigo em silêncio). O ADR 0017 continua:
            // a chave identifica a conta; `rename` preserva a tag dela.
            // **Menos o vazio**: uma volta sem apelido (medido em 26/10/2026, a
            // retomada pela lista de servidores do navegador) mantém o da conta,
            // em vez de apagá-lo para todo mundo.
            if !nickname.trim().is_empty() {
                self.rename(PersonId(id as u64), nickname)?;
            }
            return Ok((self.person(PersonId(id as u64))?, false));
        }

        let tag = nova_tag(self.connection)?;
        let alias = alias_do_nome(nickname, &tag);
        self.connection.execute(
            "INSERT INTO people (nickname, nickname_base, nickname_tag, public_key, created_at, last_seen_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![alias, nickname, tag, public_key, now_seconds()],
        )?;
        let id = self.connection.last_insert_rowid();
        self.seat_the_arrival(id)?;

        Ok((self.person(PersonId(id as u64))?, true))
    }

    /// Compatibilidade com a pré-checagem antiga da portaria: bases iguais
    /// são permitidas desde as tags. Não consulta nem altera a identidade;
    /// registro/rename preservam a tag e o alias únicos no banco.
    ///
    /// A assinatura Result permanece para os chamadores antigos; não falha.
    pub fn apelido_de_outra_chave(&self, public_key: &[u8], nickname: &str) -> Result<bool> {
        // A chave autentica a conta; bases iguais não reservam identidade.
        let _ = (public_key, nickname);
        Ok(false)
    }

    /// Troca o apelido de uma conta. As duas rotas (a entrada e o
    /// `SetNickname`) escrevem pela mesma função, e nenhuma esquece a
    /// preservação da tag e a unicidade do alias.
    ///
    /// # Errors
    ///
    /// Falha de banco; bases iguais não transferem identidade.
    pub fn rename(&self, person: PersonId, nickname: &str) -> Result<()> {
        let (atual, tag): (String, String) = self.connection.query_row(
            "SELECT nickname, nickname_tag FROM people WHERE id = ?1",
            [person.get() as i64],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        // Um cliente legado devolve o próprio alias no Hello. Não anexar tag
        // de novo, nem interpretar o sufixo arbitrário de outra pessoa.
        if atual == nickname {
            return Ok(());
        }
        self.connection.execute(
            "UPDATE people SET nickname = ?1, nickname_base = ?2 WHERE id = ?3",
            params![alias_do_nome(nickname, &tag), nickname, person.get() as i64],
        )?;
        tracing::info!(person = person.get(), "this account changed its name");
        Ok(())
    }

    /// O cargo de chegada de uma conta recém-criada: o Pessoa, se ele ainda
    /// existe — e só ele. Apagado o Pessoa, quem chega tem o `@everyone`.
    fn seat_the_arrival(&self, person_row: i64) -> Result<()> {
        self.connection.execute(
            "INSERT OR IGNORE INTO person_roles (person_id, role_id)
             SELECT ?1, id FROM roles WHERE id = ?2",
            params![person_row, i64::from(PERSON_ROLE.get())],
        )?;
        Ok(())
    }

    /// Apresentação completa, separada do alias compatível usado pela mídia.
    pub fn identidade_apresentada(
        &self,
        id: PersonId,
    ) -> Result<seele_proto::control::ServerMessage> {
        let (base, tag) = self.connection.query_row(
            "SELECT nickname_base, nickname_tag FROM people WHERE id = ?1",
            [id.get() as i64],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?;
        Ok(seele_proto::control::ServerMessage::IdentidadeApresentada {
            pessoa: id,
            base,
            tag,
        })
    }

    /// Carrega uma conta.
    ///
    /// # Errors
    ///
    /// [`Refusal::UnknownPerson`] se ela não existe.
    pub fn person(&self, id: PersonId) -> Result<Person> {
        let nickname: String = self
            .connection
            .query_row(
                "SELECT nickname FROM people WHERE id = ?1",
                [id.get() as i64],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(Refusal::UnknownPerson)?;

        let mut statement = self.connection.prepare(
            "SELECT role_id FROM person_roles WHERE person_id = ?1 AND role_id <> 0 ORDER BY role_id",
        )?;
        let roles = statement
            .query_map([id.get() as i64], |row| row.get::<_, i64>(0))?
            .filter_map(Result::ok)
            .map(|role| RoleId(role as u32))
            .collect();

        Ok(Person {
            id,
            nickname,
            roles,
        })
    }

    /// Dá um cargo, **sem regra nenhuma**: é o caminho da máquina que hospeda
    /// (a config do `seeled`, os testes). Pelo fio, quem atribui é
    /// `cargos::atender`, com a hierarquia e o ganho efetivo.
    ///
    /// # Errors
    ///
    /// O banco.
    pub fn grant_role(&self, person: PersonId, role: RoleId) -> Result<()> {
        self.connection.execute(
            "INSERT OR IGNORE INTO person_roles (person_id, role_id) VALUES (?1, ?2)",
            params![person.get() as i64, i64::from(role.get())],
        )?;
        Ok(())
    }

    /// Tira um cargo, sem regra nenhuma (como [`Self::grant_role`]).
    ///
    /// # Errors
    ///
    /// O banco.
    pub fn revoke_role(&self, person: PersonId, role: RoleId) -> Result<()> {
        self.connection.execute(
            "DELETE FROM person_roles WHERE person_id = ?1 AND role_id = ?2",
            params![person.get() as i64, i64::from(role.get())],
        )?;
        Ok(())
    }

    /// Se esta pessoa pode isto, AGORA (ADR 0064): o proprietário pode tudo;
    /// o banido, nada; o resto é a soma das concessões menos as restrições.
    ///
    /// # Errors
    ///
    /// O banco.
    pub fn may(&self, person: PersonId, permission: Permission) -> Result<bool> {
        Ok(banco::ler_estado_de(self.connection, person)?
            .efetivas(person)
            .pode(permission))
    }

    /// Todas as permissões do produto que esta pessoa tem agora.
    ///
    /// # Errors
    ///
    /// O banco.
    pub fn permissions(&self, person: PersonId) -> Result<Vec<Permission>> {
        Ok(banco::ler_estado_de(self.connection, person)?
            .efetivas(person)
            .produto
            .into_iter()
            .collect())
    }

    /// Bane uma pessoa. `expires_at` `None` é permanente.
    ///
    /// # Errors
    ///
    /// [`Refusal::PermissionDenied`] se quem bane não tem Banir **ou** se a
    /// hierarquia não deixa (o alvo não está abaixo, ou é o proprietário).
    /// A conferência mora aqui, e não em quem chama, para nenhum chamador
    /// futuro esquecê-la.
    pub fn ban(
        &self,
        person: PersonId,
        issued_by: PersonId,
        reason: Option<&str>,
        expires_at: Option<i64>,
    ) -> Result<()> {
        if !banco::ler_estado(self.connection)?.pode_moderar(issued_by, person, Permission::Ban) {
            return Err(Refusal::PermissionDenied.into());
        }
        self.connection
            .execute(
                "INSERT INTO bans (person_id, issued_by, reason, created_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    person.get() as i64,
                    issued_by.get() as i64,
                    reason,
                    now_seconds(),
                    expires_at
                ],
            )
            .context("could not record the ban")?;
        Ok(())
    }

    /// Tira todo banimento de uma pessoa.
    ///
    /// # Errors
    ///
    /// [`Refusal::PermissionDenied`] se quem tira não tem Banir.
    pub fn unban(&self, person: PersonId, issued_by: PersonId) -> Result<()> {
        if !self.may(issued_by, Permission::Ban)? {
            return Err(Refusal::PermissionDenied.into());
        }
        self.connection.execute(
            "DELETE FROM bans WHERE person_id = ?1",
            [person.get() as i64],
        )?;
        Ok(())
    }

    /// Se a pessoa está banida agora.
    ///
    /// # Errors
    ///
    /// O banco.
    pub fn is_banned(&self, person: PersonId) -> Result<bool> {
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM bans
             WHERE person_id = ?1 AND (expires_at IS NULL OR expires_at > ?2)",
            params![person.get() as i64, now_seconds()],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Os membros deste servidor: toda conta que não está banida agora, em
    /// ordem de id. É o grupo **FORA DO AR** (D-fio-4); só id e apelido.
    ///
    /// # Errors
    ///
    /// Se o banco não responder.
    pub fn membros(&self) -> Result<Vec<Membro>> {
        let mut statement = self.connection.prepare(
            "SELECT id, nickname FROM people
             WHERE id NOT IN (
                 SELECT person_id FROM bans
                 WHERE expires_at IS NULL OR expires_at > ?1
             )
             ORDER BY id",
        )?;
        let membros = statement
            .query_map([now_seconds()], |row| {
                Ok(Membro {
                    id: PersonId(row.get::<_, i64>(0)? as u64),
                    nickname: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(membros)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::persistence::Location;
    use seele_proto::cargos::PERMISSOES_DO_PRODUTO;

    const COMMANDER: RoleId = COMMANDER_ROLE;
    const OPERATOR: RoleId = OPERATOR_ROLE;
    const PERSON: RoleId = PERSON_ROLE;
    const OBSERVER: RoleId = OBSERVER_ROLE;

    /// As treze de `specs/04-servidor-seele.md` até a 0.16.
    const ALL: &[Permission] = &seele_proto::cargos::LEGADAS;

    fn store() -> Persistence {
        Persistence::open(&Location::Memory).unwrap()
    }

    /// Uma conta com exatamente um cargo, qualquer que tenha sido o de chegada.
    fn person_with(persistence: &Persistence, nickname: &str, key: u8, role: RoleId) -> PersonId {
        let permissions = Permissions::new(persistence);
        let person = permissions.register_or_find(&[key; 32], nickname).unwrap();
        for held in [COMMANDER, OPERATOR, PERSON, OBSERVER] {
            if held != role {
                permissions.revoke_role(person.id, held).unwrap();
            }
        }
        permissions.grant_role(person.id, role).unwrap();
        person.id
    }

    fn impressao(key: u8) -> String {
        seele_proto::transport::key_fingerprint(&[key; 32])
    }

    /// **Um apelido vazio na volta não apaga o da conta.** Medido em 26/10/2026:
    /// o iPhone voltou pela lista de servidores com o apelido em branco, e a
    /// chave já conhecida passou a se chamar `""` — a pessoa e o autor das
    /// mensagens apareceram vazios nos dois lados. Um apelido de verdade na volta
    /// continua valendo (o conserto da 0.9).
    #[test]
    fn um_apelido_vazio_na_volta_nao_apaga_o_da_conta() {
        let persistence = store();
        let permissions = Permissions::new(&persistence);
        let chave = [9_u8; 32];
        permissions.registrar_ou_achar(&chave, "iphone5g").unwrap();
        for vazio in ["", "   "] {
            let (pessoa, nova) = permissions.registrar_ou_achar(&chave, vazio).unwrap();
            assert!(!nova);
            assert_eq!(
                pessoa.nickname.split('#').next().unwrap(),
                "iphone5g",
                "o apelido {vazio:?} apagou o da conta"
            );
        }
        let (pessoa, _) = permissions.registrar_ou_achar(&chave, "iphone").unwrap();
        assert_eq!(
            pessoa.nickname.split('#').next().unwrap(),
            "iphone",
            "um apelido de verdade na volta continua valendo"
        );
    }

    #[test]
    fn os_membros_sao_as_contas_sem_os_banidos_em_ordem_de_id() {
        let persistence = store();
        let permissions = Permissions::new(&persistence);
        let anfitriao = person_with(&persistence, "anfitriao", 1, COMMANDER);
        let maya = permissions.register_or_find(&[2; 32], "maya").unwrap();
        let shinji = permissions.register_or_find(&[3; 32], "shinji").unwrap();
        let rei = permissions.register_or_find(&[4; 32], "rei").unwrap();
        permissions.ban(shinji.id, anfitriao, None, None).unwrap();
        permissions.ban(rei.id, anfitriao, None, Some(1)).unwrap();

        let membro = |id, nickname: &str| Membro {
            id,
            nickname: nickname.into(),
        };
        assert_eq!(
            permissions.membros().unwrap(),
            vec![
                membro(anfitriao, &permissions.person(anfitriao).unwrap().nickname),
                membro(maya.id, &maya.nickname),
                membro(rei.id, &rei.nickname),
            ]
        );
    }

    #[test]
    fn o_apelido_de_outra_chave_e_conferido_sem_escrever() {
        let persistence = store();
        let permissions = Permissions::new(&persistence);
        permissions.register_or_find(&[1; 32], "maya").unwrap();
        assert!(!permissions
            .apelido_de_outra_chave(&[2; 32], "maya")
            .unwrap());
        assert!(!permissions
            .apelido_de_outra_chave(&[1; 32], "maya")
            .unwrap());
        assert!(!permissions
            .apelido_de_outra_chave(&[2; 32], "shinji")
            .unwrap());
        assert!(permissions.register_or_find(&[2; 32], "shinji").is_ok());
    }

    #[test]
    fn a_key_gets_the_same_account_every_time() {
        let persistence = store();
        let permissions = Permissions::new(&persistence);
        let first = permissions.register_or_find(&[7; 32], "marcela").unwrap();
        let second = permissions.register_or_find(&[7; 32], "marcela").unwrap();
        assert_eq!(first.id, second.id);
    }

    #[test]
    fn duas_chaves_podem_ter_a_base_mas_nao_o_alias() {
        let persistence = store();
        let permissions = Permissions::new(&persistence);
        let a = permissions.register_or_find(&[7; 32], "marcela").unwrap();
        let b = permissions.register_or_find(&[8; 32], "marcela").unwrap();
        assert_ne!(a.nickname, b.nickname);
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn a_returning_key_takes_the_name_it_asks_for() {
        let persistence = store();
        let permissions = Permissions::new(&persistence);
        let antes = permissions.register_or_find(&[1; 32], "pessoa").unwrap();
        let depois = permissions.register_or_find(&[1; 32], "nunes").unwrap();
        assert_eq!(depois.id, antes.id);
        assert_eq!(
            depois.nickname,
            format!("nunes#{}", antes.nickname.rsplit_once('#').unwrap().1)
        );
    }

    #[test]
    fn renaming_is_not_a_way_to_take_somebody_elses_name() {
        let persistence = store();
        let permissions = Permissions::new(&persistence);
        permissions.register_or_find(&[1; 32], "nunes").unwrap();
        let outra = permissions.register_or_find(&[2; 32], "marcela").unwrap();
        let mesmo_nome = permissions.register_or_find(&[2; 32], "nunes").unwrap();
        assert_ne!(
            mesmo_nome.nickname,
            permissions.person(PersonId(1)).unwrap().nickname
        );
        let ainda = permissions.register_or_find(&[2; 32], "marcela").unwrap();
        assert_eq!(ainda.nickname, outra.nickname);
    }

    // ---- chegar nunca dá autoridade (revisão do Codex, 24/10/2026) ----

    #[test]
    fn a_primeira_conta_de_um_banco_virgem_nao_ganha_autoridade_nenhuma() {
        // O contrário do que valia até a 0.16. Sem dono declarado, ninguém
        // gerencia nada — e é o `seeled` quem avisa (`cargos::aviso_sem_dono`).
        let persistence = store();
        let permissions = Permissions::new(&persistence);
        let primeira = permissions.register_or_find(&[1; 32], "primeira").unwrap();
        assert_eq!(primeira.roles, vec![PERSON]);
        for permission in [
            Permission::ManageRoles,
            Permission::ManageVoiceRooms,
            Permission::AdministerServer,
            Permission::Administrator,
            Permission::Kick,
            Permission::Ban,
        ] {
            assert!(
                !permissions.may(primeira.id, permission).unwrap(),
                "{permission:?}"
            );
        }
        assert!(crate::cargos::aviso_sem_dono(&persistence).is_some());
    }

    #[test]
    fn duas_chegadas_concorrentes_num_banco_virgem_nenhuma_vira_dona_nem_comandante() {
        // Duas conexões de verdade no mesmo arquivo, como o teste antigo da
        // corrida — agora o resultado certo é «nenhuma».
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("seele.db");
        Persistence::open(&Location::File(file.clone())).unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let chegantes: Vec<_> = [(1_u8, "marcela"), (2_u8, "rafael")]
            .into_iter()
            .map(|(key, nickname)| {
                let file = file.clone();
                let barrier = std::sync::Arc::clone(&barrier);
                std::thread::spawn(move || {
                    let persistence = Persistence::open(&Location::File(file)).unwrap();
                    barrier.wait();
                    let pessoa = Permissions::new(&persistence)
                        .register_or_find(&[key; 32], nickname)
                        .unwrap();
                    let efetivas = crate::cargos::efetivas(&persistence, pessoa.id).unwrap();
                    (
                        pessoa.roles,
                        efetivas.proprietario,
                        efetivas.pode(Permission::ManageRoles),
                    )
                })
            })
            .collect();
        for chegante in chegantes {
            let (roles, dona, gerencia) = chegante.join().unwrap();
            assert_eq!(roles, vec![PERSON]);
            assert!(!dona && !gerencia, "uma chegada ganhou autoridade");
        }
    }

    #[test]
    fn reconectar_nao_devolve_um_comando_revogado() {
        let persistence = store();
        let permissions = Permissions::new(&persistence);
        let dono = person_with(&persistence, "anfitriao", 1, COMMANDER);
        permissions.revoke_role(dono, COMMANDER).unwrap();
        let de_novo = permissions.register_or_find(&[1; 32], "anfitriao").unwrap();
        assert!(!de_novo.roles.contains(&COMMANDER), "{de_novo:?}");
    }

    #[test]
    fn com_o_comando_revogado_de_todos_a_proxima_conta_nova_nao_o_toma() {
        // A escalada que existia: `seat_the_arrival` tomava o assento vago sem
        // olhar a marca `commandership_claimed`.
        let persistence = store();
        let permissions = Permissions::new(&persistence);
        let antigo = person_with(&persistence, "anfitriao", 1, COMMANDER);
        persistence
            .connection()
            .execute(
                "INSERT OR IGNORE INTO config (key, value) VALUES ('commandership_claimed', '1')",
                [],
            )
            .unwrap();
        permissions.revoke_role(antigo, COMMANDER).unwrap();
        let nova = permissions.register_or_find(&[9; 32], "intrusa").unwrap();
        assert_eq!(
            nova.roles,
            vec![PERSON],
            "a conta nova tomou o comando vago"
        );
    }

    #[test]
    fn um_banco_com_a_marca_e_sem_papeis_continua_sem_comando_na_reconexao() {
        // O banco legado de `an_account_older_than_the_commandership…`, nas
        // duas formas: com a marca e sem papéis, e sem a marca. Nenhuma
        // reconexão toma o comando.
        for com_marca in [true, false] {
            let persistence = store();
            persistence
                .connection()
                .execute(
                    "INSERT INTO people (nickname, public_key, created_at, last_seen_at) VALUES ('anfitriao', ?1, 0, 0)",
                    [&[1_u8; 32][..]],
                )
                .unwrap();
            if com_marca {
                persistence
                    .connection()
                    .execute(
                        "INSERT INTO config (key, value) VALUES ('commandership_claimed', '1')",
                        [],
                    )
                    .unwrap();
            }
            let de_volta = Permissions::new(&persistence)
                .register_or_find(&[1; 32], "anfitriao")
                .unwrap();
            assert!(
                de_volta.roles.is_empty(),
                "com_marca={com_marca}: {de_volta:?}"
            );
            assert!(!Permissions::new(&persistence)
                .may(de_volta.id, Permission::ManageRoles)
                .unwrap());
        }
    }

    #[test]
    fn um_atacante_remoto_nao_vira_dono_nem_com_o_apelido_do_dono() {
        // O dono é a IMPRESSÃO declarada, e não o nome: uma chave diferente
        // pedindo a mesma base ganha outra tag, sem autoridade do dono.
        let persistence = store();
        crate::cargos::declarar_proprietario(&persistence, &impressao(1)).unwrap();
        let permissions = Permissions::new(&persistence);
        let dono = permissions.register_or_find(&[1; 32], "dono").unwrap();
        assert!(
            crate::cargos::efetivas(&persistence, dono.id)
                .unwrap()
                .proprietario
        );
        let atacante = permissions.register_or_find(&[66; 32], "dono").unwrap();
        assert_ne!(atacante.nickname, dono.nickname);
        let dele = crate::cargos::efetivas(&persistence, atacante.id).unwrap();
        assert!(!dele.proprietario && !dele.pode(Permission::ManageRoles));
    }

    #[test]
    fn a_declaracao_local_faz_o_dono_e_o_dono_sempre_pode() {
        let persistence = store();
        assert!(crate::cargos::aviso_sem_dono(&persistence).is_some());
        crate::cargos::declarar_proprietario(&persistence, &impressao(5)).unwrap();
        assert!(crate::cargos::aviso_sem_dono(&persistence).is_none());
        let permissions = Permissions::new(&persistence);
        let dono = permissions.register_or_find(&[5; 32], "dona").unwrap();
        // Mesmo sem cargo e mesmo restrito pelo Observador.
        permissions.revoke_role(dono.id, PERSON).unwrap();
        permissions.grant_role(dono.id, OBSERVER).unwrap();
        for permission in PERMISSOES_DO_PRODUTO {
            assert!(
                permissions.may(dono.id, permission).unwrap(),
                "{permission:?}"
            );
        }
        assert!(crate::cargos::declarar_proprietario(&persistence, "nao-e-hex").is_err());
    }

    // ---- a matriz de permissões (specs/10: um teste por permissão negada) ----

    #[test]
    fn an_observer_is_denied_every_permission_it_should_not_have() {
        let persistence = store();
        let observer = person_with(&persistence, "observador", 4, OBSERVER);
        let permissions = Permissions::new(&persistence);
        let allowed = [
            Permission::ViewVoiceRoom,
            Permission::EnterVoiceRoom,
            Permission::ReadChannel,
        ];
        for permission in PERMISSOES_DO_PRODUTO {
            assert_eq!(
                permissions.may(observer, permission).unwrap(),
                allowed.contains(&permission),
                "observer and {permission:?}"
            );
        }
    }

    #[test]
    fn a_person_is_denied_every_moderation_permission() {
        let persistence = store();
        let person = person_with(&persistence, "marcela", 1, PERSON);
        let permissions = Permissions::new(&persistence);
        for permission in [
            Permission::RemoveMessage,
            Permission::MovePerson,
            Permission::Kick,
            Permission::Ban,
            Permission::ManageVoiceRooms,
            Permission::ManageRoles,
            Permission::AdministerServer,
            Permission::Administrator,
        ] {
            assert!(
                !permissions.may(person, permission).unwrap(),
                "{permission:?}"
            );
        }
        assert!(permissions.may(person, Permission::ShareScreen).unwrap());
    }

    #[test]
    fn an_operator_moderates_but_does_not_administer() {
        let persistence = store();
        let operator = person_with(&persistence, "operador", 2, OPERATOR);
        let permissions = Permissions::new(&persistence);
        assert!(permissions.may(operator, Permission::Kick).unwrap());
        assert!(permissions.may(operator, Permission::Ban).unwrap());
        for permission in [
            Permission::ManageVoiceRooms,
            Permission::ManageRoles,
            Permission::AdministerServer,
            Permission::Administrator,
        ] {
            assert!(
                !permissions.may(operator, permission).unwrap(),
                "{permission:?}"
            );
        }
    }

    #[test]
    fn a_commander_has_everything() {
        let persistence = store();
        let commander = person_with(&persistence, "comandante", 9, COMMANDER);
        let permissions = Permissions::new(&persistence);
        for permission in PERMISSOES_DO_PRODUTO {
            assert!(
                permissions.may(commander, permission).unwrap(),
                "{permission:?}"
            );
        }
    }

    #[test]
    fn denial_beats_a_grant_from_another_role() {
        let persistence = store();
        let person = person_with(&persistence, "silenciado", 3, PERSON);
        let permissions = Permissions::new(&persistence);
        assert!(permissions.may(person, Permission::Speak).unwrap());
        permissions.grant_role(person, OBSERVER).unwrap();
        assert!(!permissions.may(person, Permission::Speak).unwrap());
        assert!(!permissions.may(person, Permission::ShareScreen).unwrap());
        assert!(permissions.may(person, Permission::ReadChannel).unwrap());
    }

    #[test]
    fn a_ban_beats_every_permission() {
        let persistence = store();
        let commander = person_with(&persistence, "comandante", 9, COMMANDER);
        let target = person_with(&persistence, "marcela", 1, PERSON);
        let permissions = Permissions::new(&persistence);
        permissions
            .ban(target, commander, Some("flooding"), None)
            .unwrap();
        for permission in ALL {
            assert!(
                !permissions.may(target, *permission).unwrap(),
                "{permission:?}"
            );
        }
    }

    #[test]
    fn banning_needs_the_permission_to_ban() {
        let persistence = store();
        let ordinary = person_with(&persistence, "marcela", 1, PERSON);
        let target = person_with(&persistence, "rafael", 2, PERSON);
        let permissions = Permissions::new(&persistence);
        assert!(permissions.ban(target, ordinary, None, None).is_err());
        assert!(!permissions.is_banned(target).unwrap());
    }

    #[test]
    fn banning_respects_the_hierarchy() {
        // Um Operador não bane um Comandante, nem outro Operador, nem o dono.
        let persistence = store();
        crate::cargos::declarar_proprietario(&persistence, &impressao(1)).unwrap();
        let dono = Permissions::new(&persistence)
            .register_or_find(&[1; 32], "dono")
            .unwrap()
            .id;
        let operador = person_with(&persistence, "operador", 2, OPERATOR);
        let colega = person_with(&persistence, "colega", 3, OPERATOR);
        let comandante = person_with(&persistence, "comandante", 4, COMMANDER);
        let pessoa = person_with(&persistence, "pessoa", 5, PERSON);
        let permissions = Permissions::new(&persistence);
        for alvo in [dono, colega, comandante] {
            assert!(
                permissions.ban(alvo, operador, None, None).is_err(),
                "baniu {alvo:?}"
            );
            assert!(!permissions.is_banned(alvo).unwrap());
        }
        assert!(permissions.ban(pessoa, operador, None, None).is_ok());
        assert!(
            permissions.ban(comandante, dono, None, None).is_ok(),
            "o dono pode"
        );
    }

    #[test]
    fn an_expired_ban_stops_applying() {
        let persistence = store();
        let commander = person_with(&persistence, "comandante", 9, COMMANDER);
        let target = person_with(&persistence, "marcela", 1, PERSON);
        let permissions = Permissions::new(&persistence);
        permissions
            .ban(target, commander, None, Some(now_seconds() - 1))
            .unwrap();
        assert!(!permissions.is_banned(target).unwrap());
        assert!(permissions.may(target, Permission::Speak).unwrap());
    }

    #[test]
    fn a_ban_can_be_lifted() {
        let persistence = store();
        let commander = person_with(&persistence, "comandante", 9, COMMANDER);
        let target = person_with(&persistence, "marcela", 1, PERSON);
        let permissions = Permissions::new(&persistence);
        permissions.ban(target, commander, None, None).unwrap();
        permissions.unban(target, commander).unwrap();
        assert!(permissions.may(target, Permission::Speak).unwrap());
    }

    #[test]
    fn a_person_with_no_roles_can_do_nothing() {
        let persistence = store();
        let person = person_with(&persistence, "sem-papel", 5, PERSON);
        let permissions = Permissions::new(&persistence);
        permissions.revoke_role(person, PERSON).unwrap();
        for permission in PERMISSOES_DO_PRODUTO {
            assert!(
                !permissions.may(person, permission).unwrap(),
                "{permission:?}"
            );
        }
        assert!(permissions.permissions(person).unwrap().is_empty());
    }

    #[test]
    fn an_unknown_person_is_refused_rather_than_defaulted() {
        let persistence = store();
        let permissions = Permissions::new(&persistence);
        assert!(permissions.person(PersonId(9999)).is_err());
        assert!(!permissions.may(PersonId(9999), Permission::Speak).unwrap());
    }

    #[test]
    fn an_unknown_permission_name_in_the_database_never_grants() {
        let persistence = store();
        let person = person_with(&persistence, "marcela", 1, PERSON);
        persistence
            .connection()
            .execute(
                "UPDATE roles SET permissions = '[\"Superpoder\",\"mod:x/y#z\",\"ReadChannel\"]' WHERE id = 3",
                [],
            )
            .unwrap();
        let permissions = Permissions::new(&persistence);
        assert_eq!(
            permissions.permissions(person).unwrap(),
            vec![Permission::ReadChannel]
        );
    }
}

/// Alias legado, único por tag; a base integral permanece no banco.
fn alias_do_nome(base: &str, tag: &str) -> String {
    let mut prefixo = String::new();
    for c in base.chars() {
        let c = if c == '#' { ' ' } else { c };
        if prefixo.len() + c.len_utf8() > 27 {
            break;
        }
        prefixo.push(c);
    }
    format!("{prefixo}#{tag}")
}

fn nova_tag(connection: &rusqlite::Connection) -> Result<String> {
    nova_tag_com(connection, || rand::random_range(0..36u32.pow(4)))
}

fn nova_tag_com(
    connection: &rusqlite::Connection,
    mut numero: impl FnMut() -> u32,
) -> Result<String> {
    for _ in 0..1024 {
        let mut n = numero() % 36u32.pow(4);
        let mut bytes = [b'0'; 4];
        for b in bytes.iter_mut().rev() {
            *b = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ"[(n % 36) as usize];
            n /= 36;
        }
        let tag = String::from_utf8(bytes.to_vec())?;
        let existe: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM people WHERE nickname_tag = ?1)",
            [&tag],
            |r| r.get(0),
        )?;
        if !existe {
            return Ok(tag);
        }
    }
    anyhow::bail!("não foi possível reservar uma tag única; nenhuma conta foi criada")
}

pub(crate) fn migrar_tags(connection: &rusqlite::Connection) -> Result<()> {
    let pessoas = connection
        .prepare("SELECT id, nickname_base FROM people ORDER BY id")?
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    // Fora do limite de nome do protocolo: não colide com aliases existentes
    // enquanto a migração troca cada linha dentro da mesma transação.
    connection.execute("UPDATE people SET nickname = '________________________________________________migration19_' || id", [])?;
    for (id, base) in pessoas {
        let tag = nova_tag(connection)?;
        connection.execute(
            "UPDATE people SET nickname = ?1, nickname_tag = ?2 WHERE id = ?3",
            params![alias_do_nome(&base, &tag), tag, id],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tags_testes {
    use super::*;
    use crate::persistence::Location;

    #[test]
    fn tags_mesma_base_retorno_rename_e_spoof() {
        let db = Persistence::open(&Location::Memory).unwrap();
        let p = Permissions::new(&db);
        let a = p.register_or_find(&[61; 32], "teste").unwrap();
        let b = p.register_or_find(&[62; 32], "teste").unwrap();
        assert_ne!(a.id, b.id);
        assert_ne!(a.nickname, b.nickname);
        assert_eq!(a.nickname.len(), 10);
        assert_eq!(p.register_or_find(&[61; 32], &a.nickname).unwrap(), a);
        assert_eq!(p.register_or_find(&[61; 32], "teste").unwrap(), a);
        let tag = a.nickname.rsplit_once('#').unwrap().1;
        p.rename(a.id, "outra").unwrap();
        assert_eq!(p.person(a.id).unwrap().nickname, format!("outra#{tag}"));
        p.rename(a.id, &b.nickname).unwrap();
        assert_ne!(p.person(a.id).unwrap().nickname, b.nickname);
        assert!(p.person(a.id).unwrap().nickname.ends_with(tag));
        assert!(p
            .person(b.id)
            .unwrap()
            .nickname
            .ends_with(b.nickname.rsplit_once('#').unwrap().1));
    }

    #[test]
    fn tags_restart_e_colisao_controlada() {
        let pasta = tempfile::tempdir().unwrap();
        let local = Location::File(pasta.path().join("tags.db"));
        let db = Persistence::open(&local).unwrap();
        let a = Permissions::new(&db)
            .register_or_find(&[71; 32], "é".repeat(16).as_str())
            .unwrap();
        drop(db);
        let db = Persistence::open(&local).unwrap();
        assert_eq!(
            Permissions::new(&db)
                .register_or_find(&[71; 32], &a.nickname)
                .unwrap(),
            a
        );
        db.connection()
            .execute(
                "UPDATE people SET nickname_tag='0000' WHERE id=?1",
                [a.id.get() as i64],
            )
            .unwrap();
        let mut chamadas = 0;
        let tag = nova_tag_com(db.connection(), || {
            chamadas += 1;
            if chamadas <= 2 {
                0
            } else {
                1
            }
        })
        .unwrap();
        assert_eq!(tag, "0001");
        assert_eq!(chamadas, 3);
        assert!(nova_tag_com(db.connection(), || 0).is_err());
    }

    #[test]
    fn tags_migracao_preserva_base_unicode_ids_e_unicidade() {
        let mut c = rusqlite::Connection::open_in_memory().unwrap();
        for m in crate::persistence::schema::MIGRATIONS
            .iter()
            .filter(|m| m.version <= 18)
        {
            c.execute_batch(m.sql).unwrap();
        }
        let base = "é".repeat(16);
        c.execute(
            "INSERT INTO people(id,nickname,public_key,created_at) VALUES(123,?1,?2,0)",
            params![base, vec![1u8; 32]],
        )
        .unwrap();
        let t = c.transaction().unwrap();
        t.execute_batch(crate::persistence::schema::MIGRATIONS.last().unwrap().sql)
            .unwrap();
        migrar_tags(&t).unwrap();
        t.commit().unwrap();
        let (id, alias, completa, tag): (i64, String, String, String) = c
            .query_row(
                "SELECT id,nickname,nickname_base,nickname_tag FROM people",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(id, 123);
        assert_eq!(completa, base);
        assert!(alias.len() <= 32);
        assert_eq!(tag.len(), 4);
        assert!(c.execute("INSERT INTO people(nickname,nickname_base,nickname_tag,public_key,created_at) VALUES('x','x',?1,?2,0)", params![tag,vec![2u8;32]]).is_err());
        let again: String = c
            .query_row("SELECT nickname_tag FROM people WHERE id=123", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(again, tag);
    }
}
