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
use seele_proto::uri::{analisar, LIMITE_DE_ALVOS};

fuzz_target!(|dados: &[u8]| {
    // O convite é texto. Bytes que não são UTF-8 não chegam a `analisar`: quem
    // cola um link entrega uma `&str`.
    let Ok(texto) = std::str::from_utf8(dados) else {
        return;
    };

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
