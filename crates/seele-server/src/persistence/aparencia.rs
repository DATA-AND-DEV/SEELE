//! O nome e o ícone que o servidor diz sobre si mesmo.
//!
//! Duas linhas na tabela `configuracao`, pelo critério que a própria migração 2
//! escreveu ao criá-la — «configuração do servidor que não cabe num arquivo,
//! porque muda em tempo de execução e precisa sobreviver a reinício» — e que o
//! teto de anexos do ADR 0027 já reusou sem emenda. Trocar o nome com o servidor
//! no ar é o caso normal, e não o excepcional.
//!
//! # Por que a ausência é o padrão, e não uma linha escrita no primeiro arranque
//!
//! O mesmo desenho de [`super::attachments::quota`]: linha ausente quer dizer «o
//! padrão», e escolher é o que escreve. Duas consequências, e as duas são
//! desejadas. Um servidor que existia antes disto sobe com o nome que a
//! `ServerConfig` sempre lhe deu, sem migração nenhuma tocar nele. E quem nunca
//! escolheu nome segue o que estiver no arranque — trocar o `--nome` da linha de
//! comando volta a valer, em vez de ser silenciosamente vencido por uma linha
//! que ninguém pediu.
//!
//! # O que aqui **não** se confere
//!
//! Permissão. Este módulo escreve; quem pergunta se pode é [`crate::session`],
//! no instante em que o verbo é usado, pelo PERMISSIONS — a mesma divisão que
//! [`super::channels`] explica no cabeçalho dele, e pelo mesmo motivo: o
//! arranque e os testes também escrevem aqui, e nenhum dos dois tem pessoa para
//! conferir.
//!
//! O **formato** do ícone, esse sim, é conferido — mas não aqui. Ele é conferido
//! onde os bytes entram, em `seele_proto::control`, que os recusa nas duas
//! direções: PNG de verdade, lado declarado até
//! `MAX_SERVER_ICON_SIDE`, e no máximo `MAX_SERVER_ICON_LEN` bytes. Repetir a
//! conferência aqui seria uma segunda regra para ficar para trás da primeira.

use anyhow::Result;
use rusqlite::{params, OptionalExtension};

use super::Persistence;

/// Onde o nome escolhido mora na tabela `configuracao`.
pub const CHAVE_NOME: &str = "server_nome";

/// Onde o ícone mora.
///
/// Os bytes ficam no banco, e não em arquivo ao lado dele como os anexos do ADR
/// 0027. A diferença que justifica o tratamento diferente é o tamanho: um anexo
/// é medido em megabytes e um banco que os guardasse cresceria sem devolver
/// espaço ao apagá-los, enquanto o ícone tem teto de 8 KiB e é **um só**. Um
/// arquivo à parte custaria um diretório, uma varredura de órfãos e um caso a
/// mais de «a linha existe e os bytes sumiram» — três coisas que os anexos
/// pagam porque não têm escolha, e que uma linha de 8 KiB não paga.
pub const CHAVE_ICONE: &str = "server_icone";

/// Como este servidor se chama agora.
///
/// `padrao` é o que a [`crate::ServerConfig`] trouxe, e é o que vale enquanto
/// ninguém tiver escolhido. Ver o cabeçalho do módulo.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn nome(persistence: &Persistence, padrao: &str) -> Result<String> {
    let escolhido: Option<String> = persistence
        .connection()
        .query_row(
            "SELECT valor FROM configuracao WHERE chave = ?1",
            params![CHAVE_NOME],
            |linha| linha.get(0),
        )
        .optional()?;
    // Um nome em branco no banco é tratado como ausência em vez de ser
    // desenhado: nada aqui pode ter escrito um — `definir_nome` recusa —, mas
    // quem tem o arquivo na mão tem um `sqlite3`, e um cabeçalho vazio é pior
    // que o padrão.
    Ok(escolhido
        .filter(|guardado| !guardado.trim().is_empty())
        .unwrap_or_else(|| padrao.to_owned()))
}

/// Escreve o nome escolhido. Devolve o nome aparado, que é o que ficou gravado.
///
/// Apara antes de gravar, como [`super::channels::Channels::rename_voice_room`] apara
/// o nome de uma sala de voz, e recusa o que sobrar em branco. A recusa é aqui **além**
/// de ser no protocolo porque o arranque e a janela de quem hospeda também
/// chamam por aqui, sem passar por quadro nenhum.
///
/// # Errors
///
/// Falha se o nome for branco, ou se o banco não responder.
pub fn definir_nome(persistence: &Persistence, nome: &str) -> Result<String> {
    let nome = nome.trim();
    anyhow::ensure!(!nome.is_empty(), "o nome do servidor não pode ser branco");
    persistence.connection().execute(
        "INSERT INTO configuracao (chave, valor) VALUES (?1, ?2)
         ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor",
        params![CHAVE_NOME, nome],
    )?;
    Ok(nome.to_owned())
}

/// O ícone deste servidor, se ele tiver um.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn icone(persistence: &Persistence) -> Result<Option<Vec<u8>>> {
    let guardado: Option<Vec<u8>> = persistence
        .connection()
        .query_row(
            "SELECT valor FROM configuracao WHERE chave = ?1",
            params![CHAVE_ICONE],
            |linha| linha.get(0),
        )
        .optional()?;
    // Zero byte é ausência, e não um ícone de tamanho zero: `definir_icone`
    // apaga a linha em vez de escrever vazio, então isto só acontece com quem
    // mexeu no banco à mão — e um quadrado de nada na trilha é pior que
    // nenhum.
    Ok(guardado.filter(|bytes| !bytes.is_empty()))
}

/// Grava o ícone, ou o tira.
///
/// `None` **apaga a linha** em vez de gravar zero byte. A tabela então volta a
/// dizer exatamente o que dizia antes de alguém ter escolhido uma imagem, e não
/// fica um estado a mais — «existe e está vazio» — para todo leitor ter de
/// distinguir de «não existe».
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn definir_icone(persistence: &Persistence, icone: Option<&[u8]>) -> Result<()> {
    let conexao = persistence.connection();
    match icone {
        Some(bytes) => {
            conexao.execute(
                "INSERT INTO configuracao (chave, valor) VALUES (?1, ?2)
                 ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor",
                params![CHAVE_ICONE, bytes],
            )?;
        }
        None => {
            conexao.execute(
                "DELETE FROM configuracao WHERE chave = ?1",
                params![CHAVE_ICONE],
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "um teste que trata o caso impossível deixa de ser uma afirmação sobre o código"
)]
mod testes {
    use super::*;
    use crate::persistence::Location;

    fn memoria() -> Persistence {
        Persistence::open(&Location::Memory).expect("banco em memória")
    }

    #[test]
    fn sem_escolha_vale_o_nome_do_arranque() {
        let persistence = memoria();
        assert_eq!(nome(&persistence, "Casa").unwrap(), "Casa");
    }

    #[test]
    fn o_nome_escolhido_vence_o_do_arranque() {
        let persistence = memoria();
        definir_nome(&persistence, "Terceira Tóquio").unwrap();
        assert_eq!(nome(&persistence, "Casa").unwrap(), "Terceira Tóquio");
    }

    #[test]
    fn o_nome_e_aparado_antes_de_ser_gravado() {
        let persistence = memoria();
        assert_eq!(
            definir_nome(&persistence, "  Terceira Tóquio \n").unwrap(),
            "Terceira Tóquio"
        );
        assert_eq!(nome(&persistence, "Casa").unwrap(), "Terceira Tóquio");
    }

    #[test]
    fn um_nome_branco_e_recusado() {
        // Um cabeçalho com nada dentro é uma coisa que ninguém consegue citar em
        // voz alta — a mesma razão que `check_name` dá sobre uma sala de voz.
        let persistence = memoria();
        for branco in ["", "   ", "\t\n"] {
            assert!(
                definir_nome(&persistence, branco).is_err(),
                "aceitou o nome branco {branco:?}"
            );
        }
        assert_eq!(nome(&persistence, "Casa").unwrap(), "Casa");
    }

    #[test]
    fn um_nome_branco_escrito_a_mao_no_banco_nao_chega_a_uma_tela() {
        // `definir_nome` recusa, mas quem tem o arquivo tem um `sqlite3`. O
        // padrão é melhor que um cabeçalho vazio.
        let persistence = memoria();
        persistence
            .connection()
            .execute(
                "INSERT INTO configuracao (chave, valor) VALUES (?1, '   ')",
                params![CHAVE_NOME],
            )
            .unwrap();
        assert_eq!(nome(&persistence, "Casa").unwrap(), "Casa");
    }

    #[test]
    fn um_server_sem_icone_diz_que_nao_tem() {
        let persistence = memoria();
        assert_eq!(icone(&persistence).unwrap(), None);
    }

    #[test]
    fn o_icone_gravado_volta_byte_a_byte() {
        let persistence = memoria();
        let bytes = vec![0x89, b'P', b'N', b'G', 1, 2, 3, 4, 5];
        definir_icone(&persistence, Some(&bytes)).unwrap();
        assert_eq!(icone(&persistence).unwrap(), Some(bytes));
    }

    #[test]
    fn trocar_o_icone_nao_deixa_o_anterior_para_tras() {
        let persistence = memoria();
        definir_icone(&persistence, Some(&[1, 2, 3])).unwrap();
        definir_icone(&persistence, Some(&[9, 9])).unwrap();
        assert_eq!(icone(&persistence).unwrap(), Some(vec![9, 9]));
        let linhas: i64 = persistence
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM configuracao WHERE chave = ?1",
                params![CHAVE_ICONE],
                |linha| linha.get(0),
            )
            .unwrap();
        assert_eq!(linhas, 1);
    }

    #[test]
    fn tirar_o_icone_apaga_a_linha_em_vez_de_esvazia_la() {
        // Senão a tabela ganha um terceiro estado — «existe e está vazio» — que
        // todo leitor teria de distinguir de «não existe».
        let persistence = memoria();
        definir_icone(&persistence, Some(&[1, 2, 3])).unwrap();
        definir_icone(&persistence, None).unwrap();

        assert_eq!(icone(&persistence).unwrap(), None);
        let linhas: i64 = persistence
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM configuracao WHERE chave = ?1",
                params![CHAVE_ICONE],
                |linha| linha.get(0),
            )
            .unwrap();
        assert_eq!(linhas, 0, "sobrou linha de ícone depois de tirá-lo");
    }

    #[test]
    fn o_nome_e_o_icone_sobrevivem_a_um_reinicio() {
        // O critério com que a tabela `configuracao` foi criada, cobrado: fechar
        // o banco e abri-lo de novo é o que um reinício faz.
        let diretorio = tempfile::tempdir().unwrap();
        let arquivo = Location::File(diretorio.path().join("seele.db"));
        let bytes = vec![0x89, b'P', b'N', b'G', 7, 7, 7];

        {
            let persistence = Persistence::open(&arquivo).unwrap();
            definir_nome(&persistence, "Terceira Tóquio").unwrap();
            definir_icone(&persistence, Some(&bytes)).unwrap();
        }

        let persistence = Persistence::open(&arquivo).unwrap();
        assert_eq!(nome(&persistence, "Casa").unwrap(), "Terceira Tóquio");
        assert_eq!(icone(&persistence).unwrap(), Some(bytes));
    }
}
