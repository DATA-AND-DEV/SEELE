//! Which MODs this server requires, and whether each is on.
//!
//! ADR 0045. The rows say what a joining client has to fetch and verify; the
//! bytes never live here.
//!
//! Free functions taking `&Persistence`, and not methods over a `Connection`,
//! for the reason the neighbours already follow — `admissao::criar_convite`,
//! `icone_da_pessoa`: `Persistence::connection` is `pub(crate)`, so a signature
//! over a raw connection cannot be called from the desktop shell, which is
//! exactly who calls these.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::BTreeMap;

use anyhow::Result;
use rusqlite::params;

use super::Persistence;

/// One MOD this server currently requires.
///
/// The first three fields are **identity** — who, which version, which bytes.
/// The last three are what the acceptance screen has to show before a single
/// byte is downloaded (ADR 0045), and they are stored rather than read off disk
/// at announce time: written in the same act that wrote the hash, they describe
/// the bytes the hash covers. See migration 12.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnabledMod {
    /// `author/name`.
    pub id: String,
    /// The author's version string, as published.
    pub version: String,
    /// Hex of the content hash, so a person can compare it by eye against what
    /// the indexer publishes — the same reason `pins` is plain text (ADR 0017).
    pub hash: String,
    /// The public repository the manifest declares.
    pub repo: String,
    /// What the manifest declares this MOD reaches.
    pub reach: Vec<String>,
    /// Whether this MOD has a server half, and therefore runs on the machine of
    /// whoever hosts.
    ///
    /// The one field a person cannot infer from the others, and the one the
    /// acceptance screen may not omit: a MOD with a server half reaches the
    /// `world` block of `api/v1.json` — outbound network, clock and log — from
    /// the host's machine.
    pub server_half: bool,
}

/// Turns a MOD on, or updates everything about one already known.
///
/// Takes the whole row rather than a field per argument: every field describes
/// the same bytes, and a signature that let a caller update the hash without the
/// `reach` beside it would let the two drift within one statement.
///
/// # Errors
///
/// Fails if the row cannot be written.
pub fn enable(persistence: &Persistence, ligado: &EnabledMod) -> Result<()> {
    let reach = serde_json::to_string(&ligado.reach).unwrap_or_else(|_| "[]".to_owned());
    persistence.connection().execute(
        "INSERT INTO mods (id, version, hash, repo, reach, server_half, enabled)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)
         ON CONFLICT(id) DO UPDATE SET
            version = ?2, hash = ?3, repo = ?4, reach = ?5, server_half = ?6, enabled = 1",
        params![
            ligado.id,
            ligado.version,
            ligado.hash,
            ligado.repo,
            reach,
            i64::from(ligado.server_half)
        ],
    )?;
    // Depois de gravado, e não antes: quem acorda vai ao banco, e acordar antes
    // da escrita é acordar para ler o estado velho.
    persistence.anotar_mudanca_nos_mods();
    Ok(())
}

/// Turns a MOD off, keeping its row and its data.
///
/// # Errors
///
/// Fails if the row cannot be written.
pub fn disable(persistence: &Persistence, id: &str) -> Result<()> {
    persistence
        .connection()
        .execute("UPDATE mods SET enabled = 0 WHERE id = ?1", [id])?;
    persistence.anotar_mudanca_nos_mods();
    Ok(())
}

/// Every MOD currently on, by identifier.
///
/// # Errors
///
/// Fails if the table cannot be read.
pub fn enabled(persistence: &Persistence) -> Result<Vec<EnabledMod>> {
    let connection = persistence.connection();
    let mut statement = connection.prepare(
        "SELECT id, version, hash, repo, reach, server_half
         FROM mods WHERE enabled = 1 ORDER BY id",
    )?;
    let rows = statement.query_map([], |row| {
        let reach: String = row.get(4)?;
        Ok(EnabledMod {
            id: row.get(0)?,
            version: row.get(1)?,
            hash: row.get(2)?,
            repo: row.get(3)?,
            // Um `reach` ilegível vira nenhum alcance declarado, e não uma
            // leitura que falha: a lista existe para a tela de aceite mostrar o
            // que o MOD pede, e uma linha estragada não pode ser o motivo de
            // ninguém mais conseguir entrar. O que ela custa é a tela dizer
            // «nada declarado», que é verdade sobre o que o produto sabe.
            reach: serde_json::from_str(&reach).unwrap_or_default(),
            server_half: row.get::<_, i64>(5)? != 0,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Everything one MOD has in its yard.
///
/// The whole yard at once, and not key by key, because that is the shape
/// `mods::Anfitriao` wants: it loads before a call and stores after, which is
/// what makes a call transactional without this module knowing what a call is.
///
/// A yard that does not fit in memory is a yard past `TETO_DO_QUINTAL`, and
/// that ceiling is enforced where the writing happens.
///
/// Values are UTF-8 out of a `BLOB` column. Non-UTF-8 rows are skipped rather
/// than failing the read: a yard that refuses to load because of one bad row
/// takes the whole MOD down with it, and the MOD cannot fix a row it cannot
/// read.
///
/// # Errors
///
/// Fails if the table cannot be read.
pub fn ler_quintal(persistence: &Persistence, mod_id: &str) -> Result<BTreeMap<String, String>> {
    let connection = persistence.connection();
    let mut statement =
        connection.prepare("SELECT key, value FROM mod_data WHERE mod_id = ?1 ORDER BY key")?;
    let linhas = statement.query_map([mod_id], |row| {
        let chave: String = row.get(0)?;
        let valor: Vec<u8> = row.get(1)?;
        Ok((chave, valor))
    })?;

    let mut quintal = BTreeMap::new();
    for linha in linhas {
        let (chave, valor) = linha?;
        if let Ok(texto) = String::from_utf8(valor) {
            quintal.insert(chave, texto);
        }
    }
    Ok(quintal)
}

/// Replaces one MOD's yard with what it holds now.
///
/// Replace and not merge, inside one transaction: the map that comes back from
/// a call **is** the yard, so a key the MOD deleted has to disappear here too.
/// Merging would make `delete` impossible to express from JavaScript.
///
/// # Errors
///
/// Fails if the transaction cannot be committed. Nothing is written when it
/// fails, which is what keeps a call transactional end to end.
pub fn gravar_quintal(
    persistence: &mut Persistence,
    mod_id: &str,
    quintal: &BTreeMap<String, String>,
) -> Result<()> {
    let transacao = persistence.connection_mut().transaction()?;
    transacao.execute("DELETE FROM mod_data WHERE mod_id = ?1", [mod_id])?;
    {
        let mut inserir =
            transacao.prepare("INSERT INTO mod_data (mod_id, key, value) VALUES (?1, ?2, ?3)")?;
        for (chave, valor) in quintal {
            inserir.execute(params![mod_id, chave, valor.as_bytes()])?;
        }
    }
    transacao.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::Location;

    #[test]
    fn o_quintal_ida_e_volta() {
        let mut persistence = banco();
        enable(&persistence, &ligado("seele/x", "1.0.0", "h")).expect("habilitar");

        let quintal = BTreeMap::from([
            ("pontos".to_owned(), "7".to_owned()),
            ("nome".to_owned(), "coelho".to_owned()),
        ]);
        gravar_quintal(&mut persistence, "seele/x", &quintal).expect("gravar");

        assert_eq!(ler_quintal(&persistence, "seele/x").expect("ler"), quintal);
    }

    /// Gravar substitui em vez de misturar, senão apagar uma chave seria
    /// impossível de expressar do JavaScript.
    #[test]
    fn gravar_substitui_e_uma_chave_apagada_some() {
        let mut persistence = banco();
        enable(&persistence, &ligado("seele/x", "1.0.0", "h")).expect("habilitar");

        gravar_quintal(
            &mut persistence,
            "seele/x",
            &BTreeMap::from([
                ("a".to_owned(), "1".to_owned()),
                ("b".to_owned(), "2".to_owned()),
            ]),
        )
        .expect("primeira");
        gravar_quintal(
            &mut persistence,
            "seele/x",
            &BTreeMap::from([("a".to_owned(), "1".to_owned())]),
        )
        .expect("segunda");

        let lido = ler_quintal(&persistence, "seele/x").expect("ler");
        assert_eq!(lido.len(), 1);
        assert!(!lido.contains_key("b"), "a chave apagada sobreviveu");
    }

    /// Dois MODs não leem o quintal um do outro.
    #[test]
    fn o_quintal_de_um_mod_nao_aparece_no_do_outro() {
        let mut persistence = banco();
        enable(&persistence, &ligado("seele/um", "1.0.0", "h")).expect("um");
        enable(&persistence, &ligado("seele/dois", "1.0.0", "h")).expect("dois");

        gravar_quintal(
            &mut persistence,
            "seele/um",
            &BTreeMap::from([("segredo".to_owned(), "meu".to_owned())]),
        )
        .expect("gravar");

        assert!(ler_quintal(&persistence, "seele/dois")
            .expect("ler")
            .is_empty());
    }

    fn banco() -> Persistence {
        Persistence::open(&Location::Memory).expect("abrir memória")
    }

    /// Uma linha completa, para os testes que não são sobre os campos dela.
    fn ligado(id: &str, version: &str, hash: &str) -> EnabledMod {
        EnabledMod {
            id: id.to_owned(),
            version: version.to_owned(),
            hash: hash.to_owned(),
            repo: "https://github.com/seele/exemplo".to_owned(),
            reach: vec!["dom".to_owned()],
            server_half: false,
        }
    }

    #[test]
    fn habilitar_e_depois_listar_devolve_o_mod() {
        let persistence = banco();
        enable(&persistence, &ligado("seele/exemplo", "1.0.0", "abc123")).expect("habilitar");

        let lista = enabled(&persistence).expect("listar");
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].id, "seele/exemplo");
        assert_eq!(lista[0].hash, "abc123");
    }

    /// ADR 0045: desabilitar preserva. Se a linha sumisse, desabilitar seria
    /// destrutivo e ninguém desabilitaria para testar.
    #[test]
    fn desabilitar_tira_da_lista_e_preserva_a_linha() {
        let persistence = banco();
        enable(&persistence, &ligado("seele/exemplo", "1.0.0", "abc123")).expect("habilitar");
        disable(&persistence, "seele/exemplo").expect("desabilitar");

        assert!(enabled(&persistence).expect("listar").is_empty());

        let sobrou: i64 = persistence
            .connection()
            .query_row(
                "SELECT count(*) FROM mods WHERE id = ?1",
                ["seele/exemplo"],
                |row| row.get(0),
            )
            .expect("contar");
        assert_eq!(sobrou, 1, "desabilitar apagou a linha");
    }

    /// Um MOD que sobe de versão não vira uma segunda linha disputando o mesmo
    /// identificador.
    #[test]
    fn habilitar_de_novo_atualiza_em_vez_de_duplicar() {
        let persistence = banco();
        enable(&persistence, &ligado("seele/exemplo", "1.0.0", "abc123")).expect("primeira");
        enable(&persistence, &ligado("seele/exemplo", "2.0.0", "def456")).expect("segunda");

        let lista = enabled(&persistence).expect("listar");
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].version, "2.0.0");
        assert_eq!(lista[0].hash, "def456");
    }

    /// O quintal de dados sobrevive a desabilitar, porque é o que faz religar
    /// um MOD devolver a sala como ela estava.
    #[test]
    fn o_dado_do_mod_sobrevive_a_desabilitar() {
        let persistence = banco();
        enable(&persistence, &ligado("seele/exemplo", "1.0.0", "abc123")).expect("habilitar");
        persistence
            .connection()
            .execute(
                "INSERT INTO mod_data (mod_id, key, value) VALUES (?1, ?2, ?3)",
                params!["seele/exemplo", "placar", b"7".as_slice()],
            )
            .expect("escrever dado");

        disable(&persistence, "seele/exemplo").expect("desabilitar");

        let linhas: i64 = persistence
            .connection()
            .query_row("SELECT count(*) FROM mod_data", [], |row| row.get(0))
            .expect("contar");
        assert_eq!(linhas, 1);
    }
    /// O que a tela de aceite precisa dizer vai e volta do banco.
    ///
    /// Sem estas três colunas, o anúncio teria de ler o disco a cada entrada —
    /// e descreveria o que estiver lá **agora**, e não os bytes que o hash
    /// cobre. Ver a migração 12.
    #[test]
    fn o_repositorio_o_alcance_e_a_metade_de_servidor_voltam_do_banco() {
        let persistence = banco();
        enable(
            &persistence,
            &EnabledMod {
                id: "seele/bot".to_owned(),
                version: "1.0.0".to_owned(),
                hash: "a1".repeat(32),
                repo: "https://github.com/seele/bot".to_owned(),
                reach: vec!["ler".to_owned(), "escrever".to_owned()],
                server_half: true,
            },
        )
        .expect("habilitar");

        let lista = enabled(&persistence).expect("listar");
        assert_eq!(lista[0].repo, "https://github.com/seele/bot");
        assert_eq!(lista[0].reach, vec!["ler", "escrever"]);
        assert!(lista[0].server_half);
    }

    /// Uma linha escrita antes da migração 12 continua legível, e o que ela não
    /// sabe fica vazio em vez de derrubar a leitura de todas as outras.
    #[test]
    fn uma_linha_de_antes_das_colunas_novas_continua_legivel() {
        let persistence = banco();
        persistence
            .connection()
            .execute(
                "INSERT INTO mods (id, version, hash, enabled) VALUES (?1, ?2, ?3, 1)",
                params!["seele/antigo", "1.0.0", "a1".repeat(32)],
            )
            .expect("escrever à mão");

        let lista = enabled(&persistence).expect("listar");
        assert_eq!(lista.len(), 1);
        assert!(lista[0].repo.is_empty());
        assert!(lista[0].reach.is_empty());
        assert!(!lista[0].server_half);
    }

    /// Um `reach` que não é JSON não pode ser o motivo de ninguém mais entrar.
    #[test]
    fn um_alcance_ilegivel_vira_nenhum_alcance_e_nao_uma_leitura_que_falha() {
        let persistence = banco();
        persistence
            .connection()
            .execute(
                "INSERT INTO mods (id, version, hash, reach, enabled)
                 VALUES (?1, ?2, ?3, ?4, 1)",
                params!["seele/torto", "1.0.0", "a1".repeat(32), "{não é json"],
            )
            .expect("escrever à mão");

        let lista = enabled(&persistence).expect("listar");
        assert_eq!(lista.len(), 1);
        assert!(lista[0].reach.is_empty());
    }

    /// **Quem está dentro precisa saber que a lista mudou.** Um aceite vale
    /// para o conjunto que foi lido, e habilitar ou desabilitar troca o
    /// conjunto — inclusive quando quem desabilita é o próprio despachante,
    /// tirando do ar um MOD que lançou exceção.
    #[test]
    fn habilitar_e_desabilitar_avisam_quem_espera() {
        let persistence = banco();
        let mut aviso = persistence.mods_mudaram();
        assert!(!aviso.has_changed().expect("o canal está vivo"));

        enable(&persistence, &ligado("seele/x", "1.0.0", "a1")).expect("habilitar");
        assert!(
            aviso.has_changed().expect("vivo"),
            "habilitar não avisou quem está dentro"
        );
        let _ = aviso.borrow_and_update();

        disable(&persistence, "seele/x").expect("desabilitar");
        assert!(
            aviso.has_changed().expect("vivo"),
            "desabilitar não avisou quem está dentro"
        );
    }

    /// Ler o quintal e gravar nele não são mudanças no que o servidor exige, e
    /// não podem derrubar sessão de ninguém.
    #[test]
    fn escrever_no_quintal_nao_avisa_mudanca_de_conjunto() {
        let mut persistence = banco();
        enable(&persistence, &ligado("seele/x", "1.0.0", "a1")).expect("habilitar");
        let mut aviso = persistence.mods_mudaram();
        let _ = aviso.borrow_and_update();

        gravar_quintal(
            &mut persistence,
            "seele/x",
            &BTreeMap::from([("a".to_owned(), "1".to_owned())]),
        )
        .expect("gravar");

        assert!(
            !aviso.has_changed().expect("vivo"),
            "escrever no quintal de um MOD passou por troca de conjunto"
        );
    }
}
