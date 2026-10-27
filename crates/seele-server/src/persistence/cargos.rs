//! Os cargos no banco (ADR 0064): ler o [`Estado`] e gravar uma [`Mudanca`].
//!
//! A regra é de `seele_proto::cargos`; aqui só se traduz entre ela e as
//! tabelas `roles`, `person_roles`, `bans`, `config`, `mod_permissoes`,
//! `maiores_ids` e `revisao_dos_cargos`.
//!
//! # Nomes desconhecidos
//!
//! Uma entrada das listas JSON que esta build não reconhece é **descartada**
//! ao ler, e descartar é não conceder. Ela não é reescrita: um banco editado
//! à mão continua dizendo o que dizia, e só um verbo de gestão sobre aquele
//! cargo grava a lista de novo.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use seele_proto::cargos::{
    Cargo, ChaveDeMod, Estado, Mudanca, Permissao, PermissaoDeclarada, NOME_DO_TODOS, TODOS,
};
use seele_proto::ids::{PersonId, RoleId};

use super::now_seconds;

/// A chave de `config` onde mora a impressão do proprietário declarado.
pub const CHAVE_DO_PROPRIETARIO: &str = "proprietario";

/// As entradas de uma lista JSON gravada que esta build reconhece.
#[must_use]
pub fn lista_gravada(json: &str) -> Vec<Permissao> {
    serde_json::from_str::<Vec<String>>(json)
        .unwrap_or_default()
        .iter()
        .filter_map(|nome| Permissao::da_gravada(nome))
        .collect()
}

fn para_json(lista: &[Permissao]) -> String {
    serde_json::Value::from(lista.iter().map(Permissao::gravada).collect::<Vec<_>>()).to_string()
}

/// Se `impressao` é uma impressão de chave como a portaria a escreve: 64
/// hexadecimais minúsculos.
#[must_use]
pub fn impressao_valida(impressao: &str) -> bool {
    impressao.len() == 64
        && impressao
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A impressão do proprietário declarado, se há uma.
///
/// # Errors
///
/// Se o banco não responder.
pub fn impressao_do_proprietario(conexao: &Connection) -> Result<Option<String>> {
    Ok(conexao
        .query_row(
            "SELECT value FROM config WHERE key = ?1",
            [CHAVE_DO_PROPRIETARIO],
            |linha| linha.get::<_, String>(0),
        )
        .optional()?
        .filter(|impressao| impressao_valida(impressao)))
}

/// Grava a impressão do proprietário. Só a máquina que hospeda chama isto, e
/// nunca um verbo do fio.
///
/// # Errors
///
/// Uma impressão que não tem a forma de uma, ou o banco.
pub fn declarar_proprietario(conexao: &Connection, impressao: &str) -> Result<()> {
    let impressao = impressao.to_ascii_lowercase();
    anyhow::ensure!(
        impressao_valida(&impressao),
        "a impressão do proprietário tem de ter 64 hexadecimais"
    );
    // A mesma declaração de sempre (o app e o `seeled` declaram a cada
    // arranque) não muda nada, e não sobe a revisão: senão todo reinício
    // recusaria como velha a tela que estava aberta. Medido pela conformidade
    // (`cargos.rs`, o reinício).
    if impressao_do_proprietario(conexao)?.as_deref() == Some(impressao.as_str()) {
        return Ok(());
    }
    conexao.execute(
        "INSERT INTO config (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![CHAVE_DO_PROPRIETARIO, impressao],
    )?;
    subir_revisao(conexao)?;
    Ok(())
}

/// A conta cuja chave tem esta impressão, se existe.
fn conta_da_impressao(conexao: &Connection, impressao: &str) -> Result<Option<PersonId>> {
    let mut consulta = conexao.prepare("SELECT id, public_key FROM people")?;
    let linhas = consulta.query_map([], |linha| {
        Ok((linha.get::<_, i64>(0)?, linha.get::<_, Vec<u8>>(1)?))
    })?;
    for linha in linhas {
        let (id, chave) = linha?;
        if seele_proto::transport::key_fingerprint(&chave) == impressao {
            return Ok(Some(PersonId(id as u64)));
        }
    }
    Ok(None)
}

/// Se esta conta é a do proprietário declarado.
fn e_o_proprietario(
    conexao: &Connection,
    pessoa: PersonId,
    impressao: Option<&str>,
) -> Result<bool> {
    let Some(impressao) = impressao else {
        return Ok(false);
    };
    let chave: Option<Vec<u8>> = conexao
        .query_row(
            "SELECT public_key FROM people WHERE id = ?1",
            [pessoa.get() as i64],
            |linha| linha.get(0),
        )
        .optional()?;
    Ok(chave.is_some_and(|chave| seele_proto::transport::key_fingerprint(&chave) == impressao))
}

fn ler_cargos(conexao: &Connection) -> Result<BTreeMap<RoleId, Cargo>> {
    let mut consulta =
        conexao.prepare("SELECT id, name, position, permissions, denials FROM roles")?;
    let cargos = consulta
        .query_map([], |linha| {
            let id = RoleId(linha.get::<_, i64>(0)? as u32);
            let nome: String = linha.get(1)?;
            Ok(Cargo {
                id,
                nome: if id == TODOS {
                    NOME_DO_TODOS.to_owned()
                } else {
                    nome
                },
                posicao: linha.get::<_, i64>(2)?.max(0) as u32,
                concede: lista_gravada(&linha.get::<_, String>(3)?),
                restringe: lista_gravada(&linha.get::<_, String>(4)?),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(cargos.into_iter().map(|cargo| (cargo.id, cargo)).collect())
}

fn ler_declaradas(conexao: &Connection) -> Result<BTreeMap<ChaveDeMod, String>> {
    let mut consulta = conexao.prepare(
        "SELECT p.mod_id, p.acao, p.nome FROM mod_permissoes p
         JOIN mods m ON m.id = p.mod_id
         WHERE m.enabled = 1",
    )?;
    let declaradas = consulta
        .query_map([], |linha| {
            Ok((
                ChaveDeMod {
                    mod_id: linha.get(0)?,
                    acao: linha.get(1)?,
                },
                linha.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(declaradas
        .into_iter()
        .filter(|(chave, _)| chave.e_valida())
        .collect())
}

fn ler_revisao(conexao: &Connection) -> Result<u64> {
    Ok(conexao
        .query_row(
            "SELECT revisao FROM revisao_dos_cargos WHERE id = 1",
            [],
            |linha| linha.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or(0)
        .max(0) as u64)
}

fn subir_revisao(conexao: &Connection) -> Result<()> {
    conexao.execute(
        "UPDATE revisao_dos_cargos SET revisao = revisao + 1 WHERE id = 1",
        [],
    )?;
    Ok(())
}

fn ler_maior_id(conexao: &Connection) -> Result<u32> {
    let marcado: Option<i64> = conexao
        .query_row(
            "SELECT maior FROM maiores_ids WHERE tabela = 'roles'",
            [],
            |linha| linha.get(0),
        )
        .optional()?;
    let existente: i64 =
        conexao.query_row("SELECT COALESCE(MAX(id), 0) FROM roles", [], |linha| {
            linha.get(0)
        })?;
    Ok(marcado.unwrap_or(0).max(existente).max(0) as u32)
}

/// O estado inteiro deste servidor: para decidir um verbo de gestão e para o
/// quadro.
///
/// # Errors
///
/// Se o banco não responder.
pub fn ler_estado(conexao: &Connection) -> Result<Estado> {
    let impressao = impressao_do_proprietario(conexao)?;
    let pessoas: BTreeSet<PersonId> = {
        let mut consulta = conexao.prepare("SELECT id FROM people")?;
        let ids = consulta
            .query_map([], |linha| linha.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ids.into_iter().map(|id| PersonId(id as u64)).collect()
    };
    let banidos: BTreeSet<PersonId> = {
        let mut consulta = conexao.prepare(
            "SELECT DISTINCT person_id FROM bans WHERE expires_at IS NULL OR expires_at > ?1",
        )?;
        let ids = consulta
            .query_map([now_seconds()], |linha| linha.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ids.into_iter().map(|id| PersonId(id as u64)).collect()
    };
    let mut atribuicoes: BTreeMap<PersonId, BTreeSet<RoleId>> = BTreeMap::new();
    {
        let mut consulta = conexao.prepare("SELECT person_id, role_id FROM person_roles")?;
        let linhas = consulta
            .query_map([], |linha| {
                Ok((linha.get::<_, i64>(0)?, linha.get::<_, i64>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (pessoa, cargo) in linhas {
            if cargo != i64::from(TODOS.get()) {
                atribuicoes
                    .entry(PersonId(pessoa as u64))
                    .or_default()
                    .insert(RoleId(cargo as u32));
            }
        }
    }
    let proprietario = match impressao.as_deref() {
        Some(impressao) => conta_da_impressao(conexao, impressao)?,
        None => None,
    };
    Ok(Estado {
        cargos: ler_cargos(conexao)?,
        atribuicoes,
        pessoas,
        banidos,
        proprietario,
        declaradas: ler_declaradas(conexao)?,
        dono_declarado: impressao.is_some(),
        maior_id: ler_maior_id(conexao)?,
        revisao: ler_revisao(conexao)?,
    })
}

/// O estado que basta para responder sobre UMA pessoa — o caminho de toda
/// autorização, que não precisa das outras contas.
///
/// # Errors
///
/// Se o banco não responder.
pub fn ler_estado_de(conexao: &Connection, pessoa: PersonId) -> Result<Estado> {
    let impressao = impressao_do_proprietario(conexao)?;
    let existe: bool = conexao.query_row(
        "SELECT EXISTS(SELECT 1 FROM people WHERE id = ?1)",
        [pessoa.get() as i64],
        |linha| linha.get(0),
    )?;
    let banida: bool = conexao.query_row(
        "SELECT EXISTS(SELECT 1 FROM bans WHERE person_id = ?1
                        AND (expires_at IS NULL OR expires_at > ?2))",
        params![pessoa.get() as i64, now_seconds()],
        |linha| linha.get(0),
    )?;
    let cargos_dela: BTreeSet<RoleId> = {
        let mut consulta =
            conexao.prepare("SELECT role_id FROM person_roles WHERE person_id = ?1")?;
        let ids = consulta
            .query_map([pessoa.get() as i64], |linha| linha.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ids.into_iter()
            .map(|id| RoleId(id as u32))
            .filter(|id| *id != TODOS)
            .collect()
    };
    let dona = e_o_proprietario(conexao, pessoa, impressao.as_deref())?;
    let mut atribuicoes = BTreeMap::new();
    if !cargos_dela.is_empty() {
        atribuicoes.insert(pessoa, cargos_dela);
    }
    Ok(Estado {
        cargos: ler_cargos(conexao)?,
        atribuicoes,
        pessoas: if existe {
            [pessoa].into()
        } else {
            BTreeSet::new()
        },
        banidos: if banida {
            [pessoa].into()
        } else {
            BTreeSet::new()
        },
        proprietario: dona.then_some(pessoa),
        declaradas: ler_declaradas(conexao)?,
        dono_declarado: impressao.is_some(),
        maior_id: ler_maior_id(conexao)?,
        revisao: ler_revisao(conexao)?,
    })
}

/// Se algum canal ou sala exige este cargo. Um cargo exigido não se apaga:
/// apagar soltaria a exigência e abriria o canal a quem não podia lê-lo.
///
/// # Errors
///
/// Se o banco não responder.
pub fn cargo_exigido_por_canal(conexao: &Connection, cargo: RoleId) -> Result<bool> {
    let id = i64::from(cargo.get());
    Ok(conexao.query_row(
        "SELECT EXISTS(SELECT 1 FROM channels WHERE minimum_read_role = ?1 OR minimum_write_role = ?1)
             OR EXISTS(SELECT 1 FROM voice_rooms WHERE minimum_role = ?1)",
        [id],
        |linha| linha.get(0),
    )?)
}

fn gravar_posicoes(conexao: &Connection, posicoes: &[(RoleId, u32)]) -> Result<()> {
    let mut atualizar = conexao.prepare("UPDATE roles SET position = ?2 WHERE id = ?1")?;
    for (id, posicao) in posicoes {
        atualizar.execute(params![i64::from(id.get()), i64::from(*posicao)])?;
    }
    Ok(())
}

/// Grava uma mudança decidida e sobe a revisão. Chame dentro da MESMA
/// transação em que o estado foi lido e a decisão tomada.
///
/// # Errors
///
/// Se o banco recusar — uma chave estrangeira, um nome repetido.
pub fn gravar(conexao: &Connection, mudanca: &Mudanca) -> Result<()> {
    match mudanca {
        Mudanca::Criou { cargo, posicoes } => {
            conexao
                .execute(
                    "INSERT INTO roles (id, name, permissions, denials, position)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        i64::from(cargo.id.get()),
                        cargo.nome,
                        para_json(&cargo.concede),
                        para_json(&cargo.restringe),
                        i64::from(cargo.posicao)
                    ],
                )
                .context("não consegui criar o cargo")?;
            gravar_posicoes(conexao, posicoes)?;
        }
        Mudanca::Editou(cargo) => {
            let nome = if cargo.id == TODOS {
                NOME_DO_TODOS
            } else {
                cargo.nome.as_str()
            };
            conexao.execute(
                "UPDATE roles SET name = ?2, permissions = ?3, denials = ?4 WHERE id = ?1",
                params![
                    i64::from(cargo.id.get()),
                    nome,
                    para_json(&cargo.concede),
                    para_json(&cargo.restringe)
                ],
            )?;
        }
        Mudanca::Reordenou(posicoes) => gravar_posicoes(conexao, posicoes)?,
        Mudanca::Apagou { cargo, posicoes } => {
            // `person_roles` cai em cascata (ON DELETE CASCADE, migração 1).
            conexao.execute("DELETE FROM roles WHERE id = ?1", [i64::from(cargo.get())])?;
            gravar_posicoes(conexao, posicoes)?;
        }
        Mudanca::Atribuiu { pessoa, cargo } => {
            conexao.execute(
                "INSERT OR IGNORE INTO person_roles (person_id, role_id) VALUES (?1, ?2)",
                params![pessoa.get() as i64, i64::from(cargo.get())],
            )?;
        }
        Mudanca::Revogou { pessoa, cargo } => {
            conexao.execute(
                "DELETE FROM person_roles WHERE person_id = ?1 AND role_id = ?2",
                params![pessoa.get() as i64, i64::from(cargo.get())],
            )?;
        }
    }
    subir_revisao(conexao)
}

/// Grava as permissões que um MOD declara, no ato de habilitá-lo.
///
/// - As que deixaram de ser declaradas saem da tabela: uma chave guardada num
///   cargo deixa de conceder, e fica no cargo (ADR 0045: preservar).
/// - As NOVAS com `todos: true` vão para o `@everyone` — só na primeira vez em
///   que aparecem neste servidor, e visíveis e revogáveis como qualquer outra.
///
/// # Errors
///
/// Uma declaração inválida, ou o banco.
pub fn gravar_permissoes_do_mod(
    conexao: &Connection,
    mod_id: &str,
    declaradas: &[PermissaoDeclarada],
) -> Result<()> {
    seele_proto::cargos::validar_declaradas(declaradas).map_err(|recusa| {
        anyhow::anyhow!("o MOD {mod_id} declara permissões inválidas: {recusa}")
    })?;
    let ja: BTreeSet<String> = {
        let mut consulta = conexao.prepare("SELECT acao FROM mod_permissoes WHERE mod_id = ?1")?;
        let acoes = consulta
            .query_map([mod_id], |linha| linha.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        acoes.into_iter().collect()
    };
    conexao.execute("DELETE FROM mod_permissoes WHERE mod_id = ?1", [mod_id])?;
    let mut para_todos = Vec::new();
    for declarada in declaradas {
        conexao.execute(
            "INSERT INTO mod_permissoes (mod_id, acao, nome, todos) VALUES (?1, ?2, ?3, ?4)",
            params![
                mod_id,
                declarada.acao,
                declarada.nome,
                i64::from(declarada.todos)
            ],
        )?;
        if declarada.todos && !ja.contains(&declarada.acao) {
            para_todos.push(Permissao::DeMod(ChaveDeMod {
                mod_id: mod_id.to_owned(),
                acao: declarada.acao.clone(),
            }));
        }
    }
    if !para_todos.is_empty() {
        if let Some(todos) = ler_cargos(conexao)?.get(&TODOS) {
            let mut concede = todos.concede.clone();
            for permissao in para_todos {
                if !concede.contains(&permissao) {
                    concede.push(permissao);
                }
            }
            conexao.execute(
                "UPDATE roles SET permissions = ?1 WHERE id = ?2",
                params![para_json(&concede), i64::from(TODOS.get())],
            )?;
        }
    }
    subir_revisao(conexao)
}
