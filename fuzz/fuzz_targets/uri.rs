//! O fuzz do convite `seele://`.
//!
//! O texto chega colado de uma conversa, de quem quer que tenha mandado o
//! link, e o que sai dele termina num `connect` e num `send_to`
//! (`seele_proto::uri`, «Tudo é validado antes de virar `Convite`»). É entrada
//! que ninguém confere antes de chegar aqui.
//!
//! Rode com: cargo +nightly fuzz run uri fuzz/corpus/uri fuzz/sementes/uri
//!
//! A pasta de corpus vem primeiro de propósito: o libFuzzer grava o que acha
//! na primeira pasta que recebe, e `fuzz/sementes/` é versionada e conferida
//! byte a byte por `crates/seele-proto/tests/sementes_do_fuzz.rs`.

#![no_main]

use libfuzzer_sys::fuzz_target;
use seele_proto::uri::{
    analisar, analisar_no_navegador, codigo_do_link, LIMITE_DE_ALVOS,
    LIMITE_DE_HASHES_DO_NAVEGADOR,
};

/// A origem dos testes, de mentira, e não a do produto
/// (`ORIGEM_DO_NAVEGADOR`). Precisa ser a mesma de `fuzz/sementes/uri/navegador`.
const ORIGEM: &str = "origem.invalid";

fuzz_target!(|dados: &[u8]| {
    // O convite é texto. Bytes que não são UTF-8 não chegam a `analisar`: quem
    // cola um link entrega uma `&str`.
    let Ok(texto) = std::str::from_utf8(dados) else {
        return;
    };

    // O link da sala: só não pode entrar em pânico.
    let _ = codigo_do_link(texto);

    // A forma do navegador (ADR 0057): o que ela aceita respeita os tetos, e o
    // que o SEELE escreve dela é lido de volta igual.
    if let Ok(lido) = analisar_no_navegador(texto, ORIGEM) {
        assert!(lido.hashes.len() <= LIMITE_DE_HASHES_DO_NAVEGADOR);
        assert!(lido.convite.bilhete.is_none());
        let escrito = lido
            .convite
            .no_navegador(ORIGEM, "/e", &lido.hashes, lido.ate)
            .expect("o link do navegador aceito tem fp e hashes");
        let relido = analisar_no_navegador(&escrito, ORIGEM)
            .expect("o link do navegador que o SEELE escreve tem de ser lido de volta");
        assert_eq!(relido, lido);
    }

    let Ok(convite) = analisar(texto) else {
        // Recusar é sempre aceitável. O que não é aceitável é entrar em pânico,
        // e chegar aqui prova que não houve.
        return;
    };

    // O teto de endereços vale para o que entra, e não só para o que se monta.
    assert!(convite.alternativos.len() < LIMITE_DE_ALVOS);

    // O link que o SEELE escreve a partir de um convite aceito é lido de volta
    // como o mesmo convite. Uma diferença aqui é o mesmo link levando duas
    // pessoas a dois lugares diferentes, conforme quem o reescreveu no meio.
    let escrito = convite.to_string();
    let relido =
        analisar(&escrito).expect("o convite que o SEELE escreve tem de ser lido de volta");
    assert_eq!(relido, convite);
});
