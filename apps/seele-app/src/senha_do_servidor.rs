//! A senha do servidor, como quem hospeda a escolhe.
//!
//! Dois lugares a escolhem: a tela de preparar, na hora de hospedar um servidor
//! novo (`hospedar`), e o painel A PORTA, com o servidor já de pé
//! (`definir_senha_do_server`). Os dois passam por [`conferir`], e por isso a
//! regra é uma só.
//!
//! # A regra, e de onde ela vem
//!
//! Até aqui A PORTA aceitava qualquer coisa que não fosse o campo vazio, e três
//! dessas coisas eram senhas que ninguém conseguia usar, ou que não fechavam
//! nada:
//!
//! - **só espaços**: o `connect` deste mesmo app joga fora um segredo em branco
//!   antes de discar (`join_secret.filter(|s| !s.trim().is_empty())`). O
//!   servidor passava a exigir uma senha que nenhum app manda;
//! - **espaço na ponta**: quem recebe a senha por mensagem não vê o espaço, e a
//!   senha certa aos olhos de todo mundo é recusada;
//! - **curta**: a senha do servidor é conferida antes de qualquer
//!   autenticação, e o que segura quem adivinha é o balde por endereço
//!   (`seele_server::taxa`: trinta apertos por minuto). Quatro algarismos caem
//!   em menos de seis horas de um endereço só.
//!
//! O teto em bytes é o de um campo que alguém digita: o `Hello` leva o segredo
//! num quadro de controle, e uma senha de página inteira não cabe nele.
//!
//! # A senha não vira texto em lugar nenhum
//!
//! [`SenhaDoServidor`] não tem `Display`, e o `Debug` dela não imprime o que
//! guarda: um `tracing::info!(?senha)` escrito amanhã grava «oculta» no
//! `seele.log`, e não a senha. A recusa ([`SenhaRecusada`]) diz o motivo e
//! nunca o texto recusado. O guarda é `a_senha_escolhida_nao_vira_texto`.

/// O mínimo de caracteres de uma senha de servidor.
pub(crate) const MINIMO: usize = 8;

/// O máximo, em bytes, de uma senha de servidor.
pub(crate) const MAXIMO_EM_BYTES: usize = 128;

/// Uma senha de servidor que passou por [`conferir`].
///
/// O texto só sai por [`Self::texto`], que é o que `admissao::definir_senha`
/// recebe para calcular o Argon2id.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct SenhaDoServidor(String);

impl SenhaDoServidor {
    /// O texto da senha, para quem calcula o hash dela.
    pub(crate) fn texto(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SenhaDoServidor {
    fn fmt(&self, formatador: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatador.write_str("SenhaDoServidor(oculta)")
    }
}

/// Por que uma senha de servidor não serve.
///
/// O motivo, e nunca o texto recusado: esta enumeração atravessa a ponte e vai
/// ao registro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub(crate) enum SenhaRecusada {
    /// Vazia, ou só espaços.
    Vazia,
    /// Começa ou termina com espaço.
    ComEspacoNaPonta,
    /// Menos de [`MINIMO`] caracteres.
    Curta,
    /// Mais de [`MAXIMO_EM_BYTES`] bytes.
    Longa,
}

/// A regra, como a tela a escreve **antes** de alguém digitar.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub(crate) struct Regras {
    /// O mínimo de caracteres.
    minimo: usize,
    /// O máximo de bytes.
    maximo_em_bytes: usize,
}

/// A regra desta versão.
pub(crate) const fn regras() -> Regras {
    Regras {
        minimo: MINIMO,
        maximo_em_bytes: MAXIMO_EM_BYTES,
    }
}

/// Confere uma senha digitada, sem mexer nela.
///
/// **Nada é aparado**: uma senha gravada diferente da digitada seria uma senha
/// que quem hospeda não sabe qual é. O que não serve é recusado, e a tela diz
/// por quê.
///
/// # Errors
///
/// [`SenhaRecusada`], com o motivo.
pub(crate) fn conferir(digitada: &str) -> Result<SenhaDoServidor, SenhaRecusada> {
    if digitada.trim().is_empty() {
        return Err(SenhaRecusada::Vazia);
    }
    if digitada.trim() != digitada {
        return Err(SenhaRecusada::ComEspacoNaPonta);
    }
    if digitada.chars().count() < MINIMO {
        return Err(SenhaRecusada::Curta);
    }
    if digitada.len() > MAXIMO_EM_BYTES {
        return Err(SenhaRecusada::Longa);
    }
    Ok(SenhaDoServidor(digitada.to_owned()))
}

#[cfg(test)]
mod testes {
    #![allow(clippy::expect_used, reason = "num teste, o pânico é o relatório")]

    use super::*;

    #[test]
    fn a_senha_que_ninguem_consegue_usar_e_recusada_com_o_motivo() {
        for (digitada, motivo) in [
            ("", SenhaRecusada::Vazia),
            ("   ", SenhaRecusada::Vazia),
            ("\t\n", SenhaRecusada::Vazia),
            (" terceiro impacto", SenhaRecusada::ComEspacoNaPonta),
            ("terceiro impacto ", SenhaRecusada::ComEspacoNaPonta),
            ("1234", SenhaRecusada::Curta),
            ("sétimo7", SenhaRecusada::Curta),
        ] {
            assert_eq!(
                conferir(digitada),
                Err(motivo),
                "a senha de {} caracteres não foi recusada como {motivo:?}",
                digitada.chars().count()
            );
        }
        let longa = "é".repeat(MAXIMO_EM_BYTES / 2 + 1);
        assert_eq!(conferir(&longa), Err(SenhaRecusada::Longa));
    }

    #[test]
    fn a_senha_que_serve_sai_como_foi_digitada() {
        // O mínimo conta caracteres, e não bytes: oito letras acentuadas são
        // dezesseis bytes e continuam sendo oito caracteres.
        for digitada in ["12345678", "ççççéééé", "terceiro impacto", "a b c d e"] {
            let senha = conferir(digitada).expect("uma senha que serve foi recusada");
            assert_eq!(senha.texto(), digitada, "a senha foi mexida ao conferir");
        }
        let no_teto = "a".repeat(MAXIMO_EM_BYTES);
        assert!(
            conferir(&no_teto).is_ok(),
            "a senha do tamanho do teto foi recusada"
        );
    }

    #[test]
    fn a_senha_escolhida_nao_vira_texto() {
        // O `Debug` é o caminho por onde um valor chega ao `seele.log` sem
        // ninguém decidir: `?senha` num `tracing`, um `{:?}` num `panic!`, o
        // `derive(Debug)` de uma estrutura que a guarde.
        let senha = conferir("terceiro impacto").expect("a senha serve");
        let dita = format!("{senha:?} {:?}", Some(&senha));
        assert!(
            !dita.contains("terceiro") && !dita.contains("impacto"),
            "o `Debug` da senha escolhida imprime a senha: {dita}"
        );
        // E a recusa diz o motivo, sem o texto recusado.
        let recusa = conferir("xyz12").expect_err("cinco caracteres não servem");
        let dita = format!(
            "{recusa:?} {}",
            serde_json::to_string(&recusa).expect("a recusa serializa")
        );
        assert!(
            !dita.contains("xyz12"),
            "a recusa carrega a senha recusada: {dita}"
        );
    }
}
