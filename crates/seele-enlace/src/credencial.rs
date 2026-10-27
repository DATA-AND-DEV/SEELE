//! A credencial ICE de uma tentativa (contrato D2): `U`, com ufrag = pwd = U.
//!
//! As credenciais ICE não são segredo: o que protege a conexão é o DTLS com a
//! impressão conferida e a prova Ed25519. O `U` só separa uma tentativa da
//! outra no mesmo socket, e é o que o anfitrião vê no USERNAME `U:U`.

/// Quantos caracteres tem um `U`.
pub const CREDENCIAL_LEN: usize = 32;

const ALFABETO: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// Um `U` novo: 32 caracteres de `[A-Za-z0-9]`.
#[must_use]
pub fn sortear_credencial() -> String {
    (0..CREDENCIAL_LEN)
        .map(|_| {
            let indice = usize::from(rand::random::<u8>()) % ALFABETO.len();
            char::from(ALFABETO.get(indice).copied().unwrap_or(b'A'))
        })
        .collect()
}

/// Se o texto tem a forma de um `U`.
#[must_use]
pub fn credencial_valida(u: &str) -> bool {
    u.len() == CREDENCIAL_LEN && u.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// O `U` de um USERNAME `U:U`, se ele tem essa forma.
pub(crate) fn de_usuario(usuario: &str) -> Option<&str> {
    let (a, b) = usuario.split_once(':')?;
    (a == b && credencial_valida(a)).then_some(a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn um_u_sorteado_e_valido_e_unico() {
        let a = sortear_credencial();
        assert!(credencial_valida(&a));
        assert_ne!(a, sortear_credencial());
    }

    #[test]
    fn so_u_dois_pontos_u_vira_credencial() {
        let u = sortear_credencial();
        assert_eq!(de_usuario(&format!("{u}:{u}")), Some(u.as_str()));
        assert_eq!(de_usuario(&format!("{u}:{}", sortear_credencial())), None);
        assert_eq!(de_usuario("abc:abc"), None);
        assert_eq!(de_usuario(&u), None);
        let ruim = format!("{}!", &u[1..]);
        assert_eq!(de_usuario(&format!("{ruim}:{ruim}")), None);
    }
}
