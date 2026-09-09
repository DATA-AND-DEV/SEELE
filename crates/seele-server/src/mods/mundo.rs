//! What a MOD reaches that is not the protocol: network, clock and log.
//!
//! ADR 0045, the `world` block. This is where "liberdade total" lives — a MOD
//! opens outbound connections from the machine of whoever hosts, which is what
//! the owner's tunnel MOD needs and what the acceptance screen has to say out
//! loud.
//!
//! # The ceiling that the interpreter's does not cover
//!
//! `set_interrupt_handler` fires while **JavaScript** runs. It does not fire
//! inside a native call, so a MOD sitting in `buscar()` is not interrupted by
//! the step ceiling at all — and the dispatcher is serial on purpose, so one
//! slow endpoint would stall every MOD and the whole event queue behind it.
//!
//! That is why the request carries a ceiling of its own, and why it is small.
//! It is not belt and braces: without it the step ceiling has a hole exactly
//! the size of the network.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

/// How long one request may take.
///
/// Small because of what it holds up: the MOD thread runs one moment at a time,
/// so this is the worst case a single MOD can delay every other MOD **and** the
/// events queued behind them. Two seconds is already long for a room where
/// people are talking; more would be a MOD deciding how responsive the server
/// is.
pub const TETO_DA_BUSCA: std::time::Duration = std::time::Duration::from_secs(2);

/// The most one response body a MOD receives may hold, in bytes.
///
/// A MOD that asks for a gigabyte would hold the ceiling above *and* the memory
/// of the whole server, which is not the MOD's to spend.
pub const TETO_DA_RESPOSTA: usize = 4 * 1024 * 1024;

/// Fetches one URL, or nothing.
///
/// Deliberately one answer for every failure — refused scheme, DNS, timeout,
/// oversized body. A MOD that could tell them apart could map the network of
/// whoever hosts it, and it has no use for the difference.
#[must_use]
pub fn buscar(url: &str) -> Option<String> {
    // `http` e `https` e mais nada. Sem isto, `file:///…/identity.key` faria
    // pela rede exatamente o que a pasta do MOD existe para impedir no disco.
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return None;
    }

    // O provedor de criptografia, pelo mesmo idioma que este crate já usa em
    // `lib.rs` e que o `seele-core` repete em quatro lugares: `install_default`
    // devolve erro se já houver um, e o `let _` é o que torna isso idempotente.
    // Sem ele, `rustls-no-provider` **entra em pânico** ao construir o cliente —
    // e um pânico aqui derrubaria a thread dos MODs inteira, que é o oposto do
    // que a falha isolada promete.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let cliente = reqwest::blocking::Client::builder()
        .timeout(TETO_DA_BUSCA)
        .build()
        .ok()?;
    let resposta = cliente.get(url).send().ok()?;

    let corpo = resposta.text().ok()?;
    if corpo.len() > TETO_DA_RESPOSTA {
        return None;
    }
    Some(corpo)
}

/// Seconds since the Unix epoch.
#[must_use]
pub fn agora() -> i64 {
    crate::persistence::now_seconds()
}

/// Writes one line to the host's log, from a MOD.
///
/// Always prefixed by the MOD's identifier, and never by the MOD's own choice:
/// a line whose origin a MOD could forge is a line that cannot be used to
/// decide which MOD to disable.
pub fn registrar(id: &str, linha: &str) {
    // Um teto no que um MOD escreve, para que ele não vire o log inteiro.
    let recortada: String = linha.chars().take(500).collect();
    tracing::info!(mod_id = %id, "{recortada}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn so_http_e_https_atravessam() {
        for url in [
            "file:///etc/passwd",
            "file:///Users/alguem/.config/seele/identity.key",
            "ftp://exemplo.invalido/x",
            "",
            "javascript:alert(1)",
        ] {
            assert_eq!(buscar(url), None, "`{url}` atravessou");
        }
    }

    /// **O teto que o interpretador não dá.** Um endereço que aceita a conexão
    /// e nunca responde não pode segurar a fila de MODs para sempre.
    #[test]
    fn um_endereco_que_nunca_responde_e_cortado_pelo_teto() {
        let ouvinte = std::net::TcpListener::bind("127.0.0.1:0").expect("ouvir");
        let porta = ouvinte.local_addr().expect("endereço").port();

        // Aceita e cala. É o pior caso: a conexão abre, então nem o TCP
        // desiste sozinho.
        std::thread::spawn(move || {
            let _mudo: Vec<_> = ouvinte.incoming().take(1).collect();
            std::thread::sleep(std::time::Duration::from_secs(30));
        });

        let inicio = std::time::Instant::now();
        assert_eq!(buscar(&format!("http://127.0.0.1:{porta}/")), None);
        assert!(
            inicio.elapsed() < TETO_DA_BUSCA * 3,
            "a busca segurou a fila por {:?}",
            inicio.elapsed()
        );
    }

    #[test]
    fn o_relogio_anda_para_a_frente() {
        assert!(agora() > 1_700_000_000, "o relógio veio antes de 2023");
    }
}
