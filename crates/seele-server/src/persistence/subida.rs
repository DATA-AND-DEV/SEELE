//! A subida medida desta máquina, guardada entre arranques.
//!
//! Uma linha na tabela `configuracao`, pelo mesmo critério que a migração 2
//! escreveu para ela: uma configuração *«que não cabe num arquivo, porque muda
//! em tempo de execução e precisa sobreviver a reinício»*. É exatamente o que
//! este número é.
//!
//! (O comentário citado ainda diz `Dogma` onde o produto hoje diz **servidor**;
//! ele mora dentro do SQL de uma migração já aplicada, e o ADR 0035 preserva o
//! que registra ontem. A frase acima é a de hoje.)
//!
//! # Por que guardar
//!
//! A [`crate::tela::SondaDaSubida`] começa na hipótese de
//! [`crate::tela::CAMINHO_DO_SERVER_BPS`] e sobe por evidência. Sem memória,
//! **toda** vez que o servidor sobe ela recomeça dali — e o portão de admissão,
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
//! # Desde a 0.17: a memória diz onde e quando foi medida
//!
//! **Amarrada à rede, à idade e à linha** (revisão do Codex na 0.17). O número
//! vai junto de um contexto ([`CHAVE_DO_CONTEXTO`]): quando foi medido, em que
//! rede (o endereço desta máquina na rede de casa, [`Contexto::rede`]) e por
//! que linha do protocolo. [`lembrada_aqui`] só devolve a memória quando os três
//! batem — a mesma rede, até [`IDADE_MAXIMA`] e a linha 2 — e **grampeada em
//! [`TETO_DA_MEMORIA_BPS`]**: uma memória é de onde partir, nunca confiança
//! ilimitada no caminho de agora.
//!
//! A linha sem contexto, que a 0.16 deixou (só o número, medido pelo QUIC e
//! talvez noutra casa), **não é usada**: o servidor parte da hipótese, e diz
//! isso no registro.

use anyhow::Result;
use rusqlite::{params, OptionalExtension};

use super::Persistence;

/// Onde a subida medida mora na tabela `configuracao`.
pub const CHAVE: &str = "subida_medida_bps";

/// Onde mora o contexto da medida (JSON), desde a 0.17.
pub const CHAVE_DO_CONTEXTO: &str = "subida_medida_contexto";

/// A memória mais velha que ainda vale: uma semana.
pub const IDADE_MAXIMA: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 60 * 60);

/// O teto de uma memória: 8 Mbps, a mesma regra de lembrança do
/// `seele-core` (B). Partir de mais que isso pede medida desta sessão.
pub const TETO_DA_MEMORIA_BPS: u32 = 8_000_000;

/// A linha do protocolo cujas medidas valem aqui.
const LINHA: &str = "2";

/// Onde e quando uma subida foi medida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contexto {
    /// Segundos Unix UTC da medida.
    pub medida_em: u64,
    /// A rede: o endereço desta máquina na rede de casa, em texto, ou `None`
    /// quando não se soube (e aí a memória nunca vale).
    pub rede: Option<String>,
}

/// O número cru, como a 0.16 o guardava. Sem contexto, não diz se vale: quem
/// quer usar chama [`lembrada_aqui`].
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

/// Por que a memória não foi usada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoriaRecusada {
    /// Nada guardado.
    Nenhuma,
    /// Só o número, sem contexto: a 0.16, ou uma edição à mão.
    SemContexto,
    /// De outra linha do protocolo.
    OutraLinha,
    /// Mais velha que [`IDADE_MAXIMA`], ou do futuro.
    Velha,
    /// Medida noutra rede, ou sem saber a rede.
    OutraRede,
}

/// **A memória, se ela vale aqui e agora**, grampeada em
/// [`TETO_DA_MEMORIA_BPS`]; ou por que não.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn lembrada_aqui(
    persistence: &Persistence,
    agora: u64,
    rede_de_agora: Option<&str>,
) -> Result<std::result::Result<u32, MemoriaRecusada>> {
    let Some(bps) = lembrada(persistence)? else {
        return Ok(Err(MemoriaRecusada::Nenhuma));
    };
    let texto: Option<String> = persistence
        .connection()
        .query_row(
            "SELECT valor FROM configuracao WHERE chave = ?1",
            params![CHAVE_DO_CONTEXTO],
            |linha| linha.get(0),
        )
        .optional()?;
    let Some(contexto) = texto.and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
    else {
        return Ok(Err(MemoriaRecusada::SemContexto));
    };
    if contexto.get("linha").and_then(serde_json::Value::as_str) != Some(LINHA) {
        return Ok(Err(MemoriaRecusada::OutraLinha));
    }
    let Some(medida_em) = contexto
        .get("medida_em")
        .and_then(serde_json::Value::as_u64)
    else {
        return Ok(Err(MemoriaRecusada::SemContexto));
    };
    if medida_em > agora || agora - medida_em > IDADE_MAXIMA.as_secs() {
        return Ok(Err(MemoriaRecusada::Velha));
    }
    let rede = contexto.get("rede").and_then(serde_json::Value::as_str);
    if rede.is_none() || rede != rede_de_agora {
        return Ok(Err(MemoriaRecusada::OutraRede));
    }
    Ok(Ok(bps.min(TETO_DA_MEMORIA_BPS)))
}

/// Guarda o que se acabou de medir, com o contexto da medida.
///
/// Chamado quando a estimativa **anda**, que é a única vez em que há notícia.
///
/// # Errors
///
/// Falha se o banco não responder.
pub fn lembrar(persistence: &Persistence, bps: u32, contexto: &Contexto) -> Result<()> {
    let conexao = persistence.connection();
    conexao.execute(
        "INSERT INTO configuracao (chave, valor) VALUES (?1, ?2)
         ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor",
        params![CHAVE, i64::from(bps)],
    )?;
    let texto = serde_json::json!({
        "linha": LINHA,
        "medida_em": contexto.medida_em,
        "rede": contexto.rede,
    })
    .to_string();
    conexao.execute(
        "INSERT INTO configuracao (chave, valor) VALUES (?1, ?2)
         ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor",
        params![CHAVE_DO_CONTEXTO, texto],
    )?;
    Ok(())
}

/// A rede de agora, como o contexto a escreve.
#[must_use]
pub fn rede_de_agora() -> Option<String> {
    crate::alcance::endereco_de_rede_local().map(|ip| ip.to_string())
}

/// Segundos Unix UTC de agora.
#[must_use]
pub fn agora() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
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

    fn aqui(medida_em: u64) -> Contexto {
        Contexto {
            medida_em,
            rede: Some("192.168.0.10".into()),
        }
    }

    /// **A memória da 0.16 não vale** (só o número, sem contexto); a de outra
    /// rede, a velha e a do futuro também não; a que vale vem grampeada.
    #[test]
    fn a_memoria_so_vale_na_mesma_rede_na_idade_e_com_teto() {
        let p = memoria();
        assert_eq!(
            lembrada_aqui(&p, 10, Some("x")).unwrap(),
            Err(MemoriaRecusada::Nenhuma)
        );

        // O que a 0.16 deixou: o número, sem contexto.
        p.connection()
            .execute(
                "INSERT INTO configuracao (chave, valor) VALUES (?1, 90000000)",
                params![CHAVE],
            )
            .unwrap();
        assert_eq!(
            lembrada_aqui(&p, 10, Some("192.168.0.10")).unwrap(),
            Err(MemoriaRecusada::SemContexto)
        );

        let dia = 24 * 60 * 60;
        lembrar(&p, 90_000_000, &aqui(100 * dia)).unwrap();
        let rede = Some("192.168.0.10");
        assert_eq!(
            lembrada_aqui(&p, 100 * dia + 60, rede).unwrap(),
            Ok(TETO_DA_MEMORIA_BPS),
            "a memória alta não foi grampeada no teto"
        );
        assert_eq!(
            lembrada_aqui(&p, 100 * dia + 60, Some("10.0.0.7")).unwrap(),
            Err(MemoriaRecusada::OutraRede)
        );
        assert_eq!(
            lembrada_aqui(&p, 100 * dia + 60, None).unwrap(),
            Err(MemoriaRecusada::OutraRede)
        );
        assert_eq!(
            lembrada_aqui(&p, 108 * dia, rede).unwrap(),
            Err(MemoriaRecusada::Velha)
        );
        assert_eq!(
            lembrada_aqui(&p, 99 * dia, rede).unwrap(),
            Err(MemoriaRecusada::Velha),
            "uma memória do futuro foi aceita"
        );
        lembrar(&p, 3_000_000, &aqui(100 * dia)).unwrap();
        assert_eq!(
            lembrada_aqui(&p, 100 * dia + 1, rede).unwrap(),
            Ok(3_000_000)
        );

        // Uma memória sem rede conhecida nunca vale.
        lembrar(
            &p,
            3_000_000,
            &Contexto {
                medida_em: 100 * dia,
                rede: None,
            },
        )
        .unwrap();
        assert_eq!(
            lembrada_aqui(&p, 100 * dia + 1, None).unwrap(),
            Err(MemoriaRecusada::OutraRede)
        );
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
        // A razão inteira deste módulo: sem isto, o servidor recomeça na hipótese
        // de 2 Mbps toda vez que sobe, e o portão de admissão recomeça
        // recusando a partir do sétimo espectador.
        let persistence = memoria();
        lembrar(&persistence, 42_000_000, &aqui(1_000)).unwrap();
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
        lembrar(&persistence, 0, &aqui(1_000)).unwrap();
        assert_eq!(lembrada(&persistence).unwrap(), None);
    }

    #[test]
    fn medir_de_novo_substitui_e_nao_acumula() {
        // `configuracao` tem `chave` como chave primária, então o segundo
        // INSERT falharia sem o `ON CONFLICT`. Um erro aqui seria silencioso no
        // caminho de quem chama — o tique só ignora o `Result` — e a memória
        // ficaria congelada na primeira medida para sempre.
        let persistence = memoria();
        lembrar(&persistence, 10_000_000, &aqui(1_000)).unwrap();
        lembrar(&persistence, 30_000_000, &aqui(1_000)).unwrap();
        assert_eq!(lembrada(&persistence).unwrap(), Some(30_000_000));
    }
}
