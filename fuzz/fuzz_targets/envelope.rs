//! O fuzz do envelope dos canais de dados (contrato D4, `seele_enlace::envelope`).
//!
//! Toda mensagem de todo canal menos a voz passa por `ler` antes de qualquer
//! outra coisa, nos dois lados: o anfitrião a lê de quem liga antes da prova,
//! e quem liga a lê do anfitrião antes do `Hello` (a apresentação `0x03`).
//!
//! Rode com:
//! cargo +nightly fuzz run envelope fuzz/corpus/envelope fuzz/sementes/envelope

#![no_main]

use libfuzzer_sys::fuzz_target;
use seele_enlace::envelope::{apresentacao, corte, credito, dados, fim, ler, Mensagem};

fuzz_target!(|bytes: &[u8]| {
    // Total: todo byte tem resposta, e o que é lido se escreve de volta igual
    // pelos montadores. Uma mensagem que voltasse diferente seria uma segunda
    // escrita do mesmo envelope, e os dois lados discordariam.
    match ler(bytes) {
        Ok(Mensagem::Dados(corpo)) => {
            assert!(!corpo.is_empty() && corpo.len() <= seele_enlace::MAX_DADOS);
            assert_eq!(dados(corpo), bytes);
        }
        Ok(Mensagem::Fim) => assert_eq!(fim(), bytes),
        Ok(Mensagem::Corte(codigo)) => assert_eq!(corte(codigo), bytes),
        Ok(Mensagem::Apresentacao(corpo)) => {
            assert!(corpo.len() <= seele_enlace::MAX_APRESENTACAO);
            assert_eq!(apresentacao(corpo), bytes);
        }
        // O crédito (Δ8): 9 bytes exatos, e o limite volta igual.
        Ok(Mensagem::Credito(limite)) => {
            assert_eq!(bytes.len(), 9);
            assert_eq!(credito(limite), bytes);
        }
        Err(_) => {}
    }
});
