//! O verificador da medição 1: as linhas de `crates/seele-server/src/session.rs`
//! que conferem a prova do aperto de mão, sem mudança.
//!
//! Lê do `stdin` uma assinatura por linha, `chave_hex mensagem_hex assinatura_hex`,
//! e escreve no `stdout` uma linha JSON por entrada:
//!
//! - `verify`: o veredito do `seeled` de hoje (`session.rs:550-599`);
//! - `estrito`: o do `verify_strict`, caso a F1 troque por ele;
//! - `erro`: o texto do erro do `seeled`, quando há.

use std::io::{BufRead, Write};

use ed25519_dalek::{Signature, Verifier, VerifyingKey};

fn hex(texto: &str) -> Option<Vec<u8>> {
    if texto.len() % 2 != 0 {
        return None;
    }
    (0..texto.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&texto[i..i + 2], 16).ok())
        .collect()
}

/// O que o `seeled` faz, na mesma ordem e com as mesmas recusas.
fn como_o_seeled(public_key: &[u8], nonce: &[u8], proof: &[u8]) -> Result<VerifyingKey, String> {
    // session.rs:550-557
    let key: [u8; 32] = public_key
        .try_into()
        .map_err(|_| "public key was not 32 bytes".to_owned())?;
    let verifying =
        VerifyingKey::from_bytes(&key).map_err(|_| "public key is not a valid Ed25519 point".to_owned())?;
    // session.rs:587-590
    let signature: [u8; 64] = proof
        .try_into()
        .map_err(|_| "proof was not a 64-byte signature".to_owned())?;
    // session.rs:594-599
    verifying
        .verify(nonce, &Signature::from_bytes(&signature))
        .map_err(|_| "signature did not verify".to_owned())?;
    Ok(verifying)
}

fn main() {
    let entrada = std::io::stdin();
    let mut saida = std::io::stdout().lock();
    for linha in entrada.lock().lines() {
        let Ok(linha) = linha else { break };
        let partes: Vec<&str> = linha.split_whitespace().collect();
        let [k, m, s] = partes[..] else {
            let _ = writeln!(saida, "{{\"verify\":false,\"estrito\":false,\"erro\":\"linha torta\"}}");
            continue;
        };
        // Uma mensagem vazia chega como "-".
        let m = if m == "-" { "" } else { m };
        let (Some(k), Some(m), Some(s)) = (hex(k), hex(m), hex(s)) else {
            let _ = writeln!(saida, "{{\"verify\":false,\"estrito\":false,\"erro\":\"hex torto\"}}");
            continue;
        };
        let linha = match como_o_seeled(&k, &m, &s) {
            Ok(chave) => {
                let assinatura: [u8; 64] = s.as_slice().try_into().unwrap_or([0; 64]);
                let estrito = chave
                    .verify_strict(&m, &Signature::from_bytes(&assinatura))
                    .is_ok();
                format!("{{\"verify\":true,\"estrito\":{estrito},\"erro\":null}}")
            }
            Err(e) => format!("{{\"verify\":false,\"estrito\":false,\"erro\":\"{e}\"}}"),
        };
        let _ = writeln!(saida, "{linha}");
    }
}
