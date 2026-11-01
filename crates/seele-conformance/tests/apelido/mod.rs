//! O apelido como o servidor o devolve desde a 0.18 (`8284b642`): a base que a
//! pessoa escolheu e a marca da chave dela, `base#XXXX` (quatro dígitos de base
//! 36, em maiúsculas). Bases iguais de chaves diferentes são admitidas, e é a
//! marca que separa as duas (`permissions.rs`, `alias_do_nome`).
//!
//! Os testes que conferem *quem* aparece numa lista conferem a base; quem
//! confere a forma confere [`marca`].
#![allow(
    dead_code,
    reason = "cada teste que inclui este módulo usa só o que precisa"
)]

/// A marca de um apelido, se ele termina numa marca bem formada.
pub(crate) fn marca(apelido: &str) -> Option<&str> {
    let (_, marca) = apelido.rsplit_once('#')?;
    (marca.len() == 4
        && marca
            .bytes()
            .all(|c| c.is_ascii_digit() || c.is_ascii_uppercase()))
    .then_some(marca)
}

/// A base de um apelido: o que vem antes da marca, ou ele inteiro se não há
/// marca bem formada.
pub(crate) fn base(apelido: &str) -> &str {
    match marca(apelido) {
        Some(m) => &apelido[..apelido.len() - m.len() - 1],
        None => apelido,
    }
}
