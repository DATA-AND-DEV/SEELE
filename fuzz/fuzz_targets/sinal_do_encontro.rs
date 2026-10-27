//! O fuzz do corpo do `POST /sinal/1` (`SEELE-SINAL/1`, ADR 0063).
//!
//! O ponto de encontro lê esse corpo de qualquer navegador da internet, antes
//! de qualquer taxa que não seja a do HTTP, e a página lê a resposta de quem
//! quer que tenha respondido. O analisador mora em `seele-proto`.
//!
//! Rode com:
//! cargo +nightly fuzz run sinal_do_encontro fuzz/corpus/sinal_do_encontro fuzz/sementes/sinal_do_encontro

#![no_main]

use libfuzzer_sys::fuzz_target;
use seele_proto::encontro::{
    ler_pedido_do_sinal, ler_resposta_do_sinal, pedido_do_sinal, resposta_do_sinal,
    serve_de_candidato, LIMITE_DE_CANDIDATOS, LIMITE_DO_SINAL,
};

fuzz_target!(|dados: &[u8]| {
    let Ok(texto) = std::str::from_utf8(dados) else {
        return;
    };
    let pedido = ler_pedido_do_sinal(texto);
    let resposta = ler_resposta_do_sinal(texto);

    // Um corpo acima do teto nunca é lido: o teto é o que impede o ponto de
    // alocar pelo que o navegador mandou.
    if pedido.is_some() || resposta.is_some() {
        assert!(dados.len() <= LIMITE_DO_SINAL);
    }
    // Um pedido nunca é lido como resposta, e o contrário.
    assert!(pedido.is_none() || resposta.is_none());

    if let Some(chama) = &pedido {
        assert!(chama.candidatos.len() <= LIMITE_DE_CANDIDATOS);
        assert!(chama.candidatos.iter().all(|c| serve_de_candidato(*c)));
        assert_eq!(ler_pedido_do_sinal(&pedido_do_sinal(chama)).as_ref(), Some(chama));
    }
    if let Some(dita) = &resposta {
        assert_eq!(ler_resposta_do_sinal(&resposta_do_sinal(dita)).as_ref(), Some(dita));
    }
});
