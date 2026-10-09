//! O tema do servidor, como dado do servidor (rascunho da API 6, §9).
//!
//! A metade de servidor de um MOD com a porta do tema (`"tema": true` no
//! manifesto) o declara com `mundo.tema`; o servidor o guarda aqui e o manda a
//! quem entra, logo depois do `Session`, e a todos a cada troca.
//!
//! # Um tema por servidor
//!
//! Decisão do dono (09/10/2026): o servidor aceita o tema de **um** MOD, o
//! escolhido. Escolhido é o primeiro que declara, e continua sendo enquanto
//! estiver ligado; desligado, a porta fica livre para outro MOD de tema. A
//! troca explícita por quem administra, na gestão de MODs, é da casca (tarefa
//! 17f do plano da F3) e escreve na mesma coluna.
//!
//! **Tirar o tema não devolve a porta.** O escolhido que declara `null` deixa o
//! servidor sem tema e continua escolhido: um «voltar ao padrão» não pode
//! entregar a aparência do servidor a outro MOD sem ninguém decidir.
//!
//! # O que aqui não se confere
//!
//! A porta e os valores. Quem confere se o MOD tem a porta, e se o tema é um
//! objeto que cabe no fio, é o runtime (`crate::mods`), no instante da chamada,
//! que é onde a recusa pode voltar ao MOD como resposta. Os pares de contraste
//! entram com a tarefa 17e, com a mesma tabela da casca.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use seele_proto::control::TemaDoServidor;

use super::Persistence;

/// O que aconteceu com uma declaração.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Declaracao {
    /// Gravada: o tema é este, ou nenhum, e o MOD é o escolhido.
    Aceita,
    /// Outro MOD é o escolhido e está ligado. Nada foi gravado.
    ///
    /// O MOD que declarou recebe só o nome da recusa (`tema-de-outro-mod`);
    /// quem é o escolhido vai para o registro de quem hospeda.
    DeOutroMod {
        /// O MOD que pinta este servidor.
        escolhido: String,
    },
}

/// O tema em vigor: o do MOD escolhido, se ele estiver ligado e tiver um.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn em_vigor(persistence: &Persistence) -> Result<Option<TemaDoServidor>> {
    Ok(persistence
        .connection()
        .query_row(
            "SELECT t.mod_id, t.valores
             FROM temas_dos_mods t JOIN mods m ON m.id = t.mod_id
             WHERE t.escolhido = 1 AND m.enabled = 1 AND t.valores IS NOT NULL",
            [],
            |row| {
                Ok(TemaDoServidor {
                    mod_id: row.get(0)?,
                    valores: row.get(1)?,
                })
            },
        )
        .optional()?)
}

/// O MOD escolhido, se ele estiver ligado: o único que pode declarar agora.
///
/// `None` quando ninguém foi escolhido ou o escolhido está desligado — aí o
/// primeiro MOD com a porta que declarar passa a ser o escolhido.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn escolhido_ligado(persistence: &Persistence) -> Result<Option<String>> {
    Ok(persistence
        .connection()
        .query_row(
            "SELECT t.mod_id
             FROM temas_dos_mods t JOIN mods m ON m.id = t.mod_id
             WHERE t.escolhido = 1 AND m.enabled = 1",
            [],
            |row| row.get(0),
        )
        .optional()?)
}

/// Grava o tema que `mod_id` declarou, ou o tira com `None`.
///
/// Numa transação: a pergunta «quem é o escolhido» e a escrita não podem ser
/// separadas por outra declaração, ou dois MODs com a porta seriam os dois
/// escolhidos por um instante — o que o índice parcial do banco recusaria com
/// um erro, em vez da recusa pelo nome.
///
/// # Errors
///
/// Falha se o banco não responder. Uma recusa por outro MOD não é erro: é
/// [`Declaracao::DeOutroMod`], e nada é gravado.
pub fn declarar(
    persistence: &Persistence,
    mod_id: &str,
    valores: Option<&str>,
) -> Result<Declaracao> {
    // `unchecked_transaction` pela razão de `mods::definir_conjunto`: uma
    // conexão atrás de um mutex, e nada se aninha aqui.
    let transacao = persistence.connection().unchecked_transaction()?;
    let escolhido: Option<String> = transacao
        .query_row(
            "SELECT t.mod_id
             FROM temas_dos_mods t JOIN mods m ON m.id = t.mod_id
             WHERE t.escolhido = 1 AND m.enabled = 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(escolhido) = escolhido.filter(|escolhido| escolhido != mod_id) {
        return Ok(Declaracao::DeOutroMod { escolhido });
    }
    // O escolhido de antes, se houver, estava desligado: a porta passa.
    transacao.execute(
        "UPDATE temas_dos_mods SET escolhido = 0 WHERE escolhido = 1 AND mod_id <> ?1",
        [mod_id],
    )?;
    transacao.execute(
        "INSERT INTO temas_dos_mods (mod_id, valores, escolhido) VALUES (?1, ?2, 1)
         ON CONFLICT(mod_id) DO UPDATE SET valores = ?2, escolhido = 1",
        params![mod_id, valores],
    )?;
    transacao.commit()?;
    Ok(Declaracao::Aceita)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::mods::{disable, enable, EnabledMod};
    use crate::persistence::{Location, Persistence};

    fn banco() -> Persistence {
        Persistence::open(&Location::Memory).expect("abrir memória")
    }

    fn ligar(persistence: &Persistence, id: &str) {
        enable(
            persistence,
            &EnabledMod {
                id: id.to_owned(),
                version: "4.0.0".to_owned(),
                hash: format!("hash-de-{id}"),
                repo: "https://example.invalid/x".to_owned(),
                reach: Vec::new(),
                server_half: true,
                api: 6,
            },
        )
        .expect("habilitar");
    }

    const CLARO: &str = r##"{"acento":"#c86b1f"}"##;
    const ESCURO: &str = r##"{"acento":"#331704"}"##;

    #[test]
    fn um_servidor_sem_tema_diz_que_nao_tem() {
        let persistence = banco();
        assert_eq!(em_vigor(&persistence).expect("ler"), None);
    }

    #[test]
    fn o_tema_declarado_fica_em_vigor_com_o_mod_que_o_declarou() {
        let persistence = banco();
        ligar(&persistence, "seele/estilo");
        assert_eq!(
            declarar(&persistence, "seele/estilo", Some(CLARO)).expect("declarar"),
            Declaracao::Aceita
        );
        let tema = em_vigor(&persistence).expect("ler").expect("o tema sumiu");
        assert_eq!(tema.mod_id, "seele/estilo");
        assert_eq!(tema.valores, CLARO);

        // Declarar de novo troca, e não acumula.
        declarar(&persistence, "seele/estilo", Some(ESCURO)).expect("trocar");
        assert_eq!(em_vigor(&persistence).unwrap().unwrap().valores, ESCURO);
    }

    /// ADR 0045: desligar preserva os dados. O tema guardado fica, inativo, e
    /// religar o MOD o devolve sem ele precisar declarar de novo — a metade de
    /// servidor só roda dentro de um pedido ou de um evento, e esperar por um
    /// deles deixaria o servidor sem a aparência que tinha.
    #[test]
    fn desligar_o_mod_tira_o_tema_e_religar_devolve_o_guardado() {
        let persistence = banco();
        ligar(&persistence, "seele/estilo");
        declarar(&persistence, "seele/estilo", Some(CLARO)).expect("declarar");

        disable(&persistence, "seele/estilo").expect("desligar");
        assert_eq!(em_vigor(&persistence).expect("ler"), None);

        ligar(&persistence, "seele/estilo");
        assert_eq!(em_vigor(&persistence).unwrap().unwrap().valores, CLARO);
    }

    /// **Um tema por servidor** (decisão do dono, 09/10/2026). O segundo MOD
    /// com a porta recebe a recusa pelo nome enquanto o primeiro for o
    /// escolhido e estiver ligado, e o tema em vigor não muda.
    #[test]
    fn um_segundo_mod_nao_pinta_por_cima_do_escolhido() {
        let persistence = banco();
        ligar(&persistence, "seele/estilo");
        ligar(&persistence, "outro/tema");
        declarar(&persistence, "seele/estilo", Some(CLARO)).expect("declarar");

        assert_eq!(
            declarar(&persistence, "outro/tema", Some(ESCURO)).expect("declarar"),
            Declaracao::DeOutroMod {
                escolhido: "seele/estilo".to_owned()
            }
        );
        let tema = em_vigor(&persistence).unwrap().unwrap();
        assert_eq!(tema.mod_id, "seele/estilo");
        assert_eq!(tema.valores, CLARO);
        assert_eq!(
            escolhido_ligado(&persistence).expect("ler"),
            Some("seele/estilo".to_owned())
        );
    }

    /// O escolhido desligado não segura a porta: quem administra o tirou, e
    /// um servidor com outro MOD de tema ligado ficaria sem aparência até
    /// alguém reinstalar o primeiro.
    #[test]
    fn com_o_escolhido_desligado_outro_mod_pode_pintar() {
        let persistence = banco();
        ligar(&persistence, "seele/estilo");
        ligar(&persistence, "outro/tema");
        declarar(&persistence, "seele/estilo", Some(CLARO)).expect("declarar");
        disable(&persistence, "seele/estilo").expect("desligar");

        assert_eq!(escolhido_ligado(&persistence).expect("ler"), None);
        assert_eq!(
            declarar(&persistence, "outro/tema", Some(ESCURO)).expect("declarar"),
            Declaracao::Aceita
        );
        let tema = em_vigor(&persistence).unwrap().unwrap();
        assert_eq!(tema.mod_id, "outro/tema");

        // E o primeiro, religado, não volta por cima: a escolha agora é do
        // segundo, e trocá-la é de quem administra.
        ligar(&persistence, "seele/estilo");
        assert_eq!(
            em_vigor(&persistence).unwrap().unwrap().mod_id,
            "outro/tema"
        );
    }

    /// **Tirar o tema não devolve a porta.** Um «voltar ao padrão» do MOD
    /// escolhido não pode entregar a aparência do servidor a outro MOD sem
    /// ninguém decidir.
    #[test]
    fn tirar_o_tema_deixa_o_servidor_sem_tema_e_a_porta_com_o_escolhido() {
        let persistence = banco();
        ligar(&persistence, "seele/estilo");
        ligar(&persistence, "outro/tema");
        declarar(&persistence, "seele/estilo", Some(CLARO)).expect("declarar");

        assert_eq!(
            declarar(&persistence, "seele/estilo", None).expect("tirar"),
            Declaracao::Aceita
        );
        assert_eq!(em_vigor(&persistence).expect("ler"), None);
        assert_eq!(
            declarar(&persistence, "outro/tema", Some(ESCURO)).expect("declarar"),
            Declaracao::DeOutroMod {
                escolhido: "seele/estilo".to_owned()
            }
        );
    }
}
