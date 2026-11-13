//! O que o alvo `control_frame` confere de cada entrada.
//!
//! Mora fora de `fuzz_targets/` porque dois lugares o compilam: o alvo
//! (`fuzz/fuzz_targets/control_frame.rs`, no nightly, com o libFuzzer) e o
//! teste `toda_semente_do_control_frame_passa_pela_propriedade_do_alvo`, em
//! `crates/seele-proto/tests/sementes_do_fuzz.rs`, que passa cada semente por
//! aqui no `cargo test` de sempre. É o mesmo código: uma propriedade que
//! reprovasse só no fuzz ficaria meses sem ninguém ver, que foi o que
//! aconteceu.
//!
//! # Por que os bytes, e não só as mensagens
//!
//! Até 12/11/2026 a conferência era `assert_eq!(again, message)`, e o fuzz do
//! `ci.yml` reprovou na primeira vez em que rodou (execução 43264412497), com
//! os dois lados impressos iguais:
//!
//! ```text
//!   left: RelatoDeRecepcao(Recepcao { perda: None, jitter_ms: Some(NaN) })
//!  right: RelatoDeRecepcao(Recepcao { perda: None, jitter_ms: Some(NaN) })
//! ```
//!
//! O `decode` aceita um relato com `NaN` de propósito: a faixa é conferida no
//! servidor por `Recepcao::confere`, que descarta o relato sem derrubar a
//! sessão. E `NaN != NaN`, então a mensagem não é igual nem a ela mesma. O
//! defeito era do arnês.
//!
//! A propriedade é «dois pares leem os mesmos bytes como a mesma mensagem», e
//! o que os pares trocam são bytes: o que foi lido, escrito de novo, tem de dar
//! o mesmo quadro. A comparação das mensagens continua onde ela quer dizer
//! alguma coisa — em toda mensagem que é igual a si mesma —, porque é ela que
//! pega um campo que o `encode` deixasse de escrever.

use seele_proto::control::{decode, encode, ClientMessage, ServerMessage, MAX_FRAME_LEN};

/// Confere uma entrada nos dois sentidos: um cliente lê quadros do servidor e
/// um servidor lê quadros do cliente, e só um dos dois está atrás da
/// autenticação. Entra em pânico se a propriedade não vale.
#[allow(
    clippy::eq_op,
    reason = "a mensagem comparada com ela mesma é a pergunta: com `NaN` dentro, não é igual"
)]
pub(crate) fn conferir(data: &[u8]) {
    if let Ok(message) = decode::<ClientMessage>(data) {
        assert!(data.len() <= MAX_FRAME_LEN);
        // O que foi aceito tem de se escrever de novo, e o que se escreve tem
        // de ser lido como os mesmos bytes.
        let frame = encode(&message).expect("an accepted message must re-encode");
        let again = decode::<ClientMessage>(&frame).expect("a re-encoded frame must decode");
        let frame_again = encode(&again).expect("a re-decoded message must re-encode");
        assert_eq!(
            frame_again, frame,
            "the same bytes read back as another message: {message:?} / {again:?}"
        );
        if message == message {
            assert_eq!(again, message);
        }
    }

    if let Ok(message) = decode::<ServerMessage>(data) {
        assert!(data.len() <= MAX_FRAME_LEN);
        let frame = encode(&message).expect("an accepted message must re-encode");
        let again = decode::<ServerMessage>(&frame).expect("a re-encoded frame must decode");
        let frame_again = encode(&again).expect("a re-decoded message must re-encode");
        assert_eq!(
            frame_again, frame,
            "the same bytes read back as another message: {message:?} / {again:?}"
        );
        if message == message {
            assert_eq!(again, message);
        }
    }
}
