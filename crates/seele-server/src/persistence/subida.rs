//! A subida medida desta máquina, guardada entre arranques.
//!
//! Uma linha na tabela `configuracao`, pelo mesmo critério que a migração 2
//! escreveu para ela: *«configuração do Dogma que não cabe num arquivo, porque
//! muda em tempo de execução e precisa sobreviver a reinício»*. É exatamente o
//! que este número é.
//!
//! # Por que guardar
//!
//! A [`crate::tela::SondaDaSubida`] começa na hipótese de
//! [`crate::tela::CAMINHO_DO_SERVER_BPS`] e sobe por evidência. Sem memória,
//! **toda** vez que o Dogma sobe ela recomeça dali — e o portão de admissão,
//! que divide esse número por N, recomeça recusando a partir do sétimo
//! espectador até a primeira janela ensinar de novo o que já se sabia ontem.
//!
//! É o gêmeo do `caminho_bps` que `seele_core::conhecidos` guarda por servidor,
//! e existe pelo mesmo motivo que aquele: reaprender um fato que era verdade
//! desde o primeiro milissegundo custa imagem ruim enquanto se reaprende.
//!
//! # O que ele **não** garante, e a decisão está escrita
//!
//! Uma medida lembrada é de **outra rede**, potencialmente. A máquina pode ter
//! mudado de casa, de Wi-Fi, de operadora. Este módulo guarda um número e não
//! sabe nada sobre onde ele foi medido.
//!
//! O precedente é do cliente, e a razão dele vale aqui igual:
//!
//! > O valor é grampeado na faixa que a escada admite: memória não autoriza
//! > começar fora dela. E a sonda continua sendo uma sonda — a primeira janela
//! > que doer corrige este número como corrigiria qualquer outro.
//!
//! **A diferença que o servidor tem, e ela é real:** no cliente o número
//! governa o próprio vídeo de quem o mediu; aqui ele governa o portão de
//! admissão e o teto de **todo mundo** na sala. Uma memória alta demais numa
//! rede que encolheu custa a voz da sala inteira por uma ou duas janelas, até o
//! `doeu` derrubá-la. Fica registrado como o que é: um risco aceito, com
//! correção medida em segundos, contra um tateio que acontece em todo arranque.
//!
//! Amarrar a memória à rede em que ela foi medida é o que tiraria esse risco, e
//! não foi feito.

use anyhow::Result;
use rusqlite::{params, OptionalExtension};

use super::Persistence;

/// Onde a subida medida mora na tabela `configuracao`.
pub const CHAVE: &str = "subida_medida_bps";

/// A última subida que esta máquina mediu, se alguma.
///
/// `None` quando nunca se mediu nada aqui — e `None` é a resposta honesta, não
/// um zero: quem chama cai na ordem de sempre, declarado e depois hipótese.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn lembrada(persistence: &Persistence) -> Result<Option<u32>> {
    let guardado: Option<i64> = persistence
        .connection()
        .query_row(
            "SELECT valor FROM configuracao WHERE chave = ?1",
            params![CHAVE],
            |linha| linha.get(0),
        )
        .optional()?;
    // Zero é tratado como ausência, pela mesma regra que `caminho_do_server` já
    // aplica ao declarado: um caminho de zero bit por segundo não é uma medida,
    // é a falta de uma.
    Ok(guardado
        .filter(|bps| *bps > 0)
        .and_then(|bps| u32::try_from(bps).ok()))
}

/// Guarda o que se acabou de medir.
///
/// Chamado quando a estimativa **anda**, que é a única vez em que há notícia. Em
/// regime isso é raro; durante uma subida de escada são alguns segundos
/// seguidos, e uma linha de `configuracao` por segundo é ruído ao lado do que a
/// tabela de mensagens escreve na mesma conexão.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn lembrar(persistence: &Persistence, bps: u32) -> Result<()> {
    persistence.connection().execute(
        "INSERT INTO configuracao (chave, valor) VALUES (?1, ?2)
         ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor",
        params![CHAVE, i64::from(bps)],
    )?;
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
    fn sem_nada_medido_a_resposta_e_nao_sei() {
        // E `None` e não zero: quem chama distingue «nunca medi» de «medi zero»,
        // e a ordem de sempre — medido, declarado, hipótese — depende disso.
        let persistence = memoria();
        assert_eq!(lembrada(&persistence).unwrap(), None);
    }

    #[test]
    fn o_que_foi_medido_volta_no_proximo_arranque() {
        // A razão inteira deste módulo: sem isto, o Dogma recomeça na hipótese
        // de 2 Mbps toda vez que sobe, e o portão de admissão recomeça
        // recusando a partir do sétimo espectador.
        let persistence = memoria();
        lembrar(&persistence, 42_000_000).unwrap();
        assert_eq!(lembrada(&persistence).unwrap(), Some(42_000_000));
    }

    #[test]
    fn um_zero_guardado_conta_como_nada_medido() {
        // Nada neste código escreve zero, e o filtro existe assim mesmo: a
        // linha vive num banco que quem opera pode editar, e um caminho de zero
        // bit por segundo não é uma medida — é a falta de uma, e tem de sair
        // pela mesma porta. Sem isto, `caminho_do_arranque` receberia
        // `Some(0)`, e o segundo filtro dele seria a única parede.
        let persistence = memoria();
        lembrar(&persistence, 0).unwrap();
        assert_eq!(lembrada(&persistence).unwrap(), None);
    }

    #[test]
    fn medir_de_novo_substitui_e_nao_acumula() {
        // `configuracao` tem `chave` como chave primária, então o segundo
        // INSERT falharia sem o `ON CONFLICT`. Um erro aqui seria silencioso no
        // caminho de quem chama — o tique só ignora o `Result` — e a memória
        // ficaria congelada na primeira medida para sempre.
        let persistence = memoria();
        lembrar(&persistence, 10_000_000).unwrap();
        lembrar(&persistence, 30_000_000).unwrap();
        assert_eq!(lembrada(&persistence).unwrap(), Some(30_000_000));
    }
}
