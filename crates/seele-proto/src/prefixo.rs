//! O pedaço de um endereço IP que divide um balde de limite de taxa.
//!
//! Nasceu no `seele-encontro` (`taxa.rs`, D-enc-1) e veio para cá quando o
//! `seele-server` passou a precisar da mesma conta para o balde de apertos de
//! mão: duas normalizações da mesma coisa são duas regras esperando para
//! discordar, e este é o único crate que os dois têm em comum.

use std::net::IpAddr;

/// O pedaço do endereço que divide um balde.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Prefixo {
    /// O endereço IPv4 inteiro.
    V4(u32),
    /// Os 64 bits de cima de um IPv6: o que uma casa recebe.
    V6(u64),
}

/// O `/32` de um IPv4, ou o `/64` de um IPv6. Um IPv4 escrito como IPv6
/// mapeado é o IPv4 dele.
#[must_use]
pub fn prefixo(ip: IpAddr) -> Prefixo {
    match ip.to_canonical() {
        IpAddr::V4(quatro) => Prefixo::V4(u32::from(quatro)),
        IpAddr::V6(seis) => Prefixo::V6(u64::try_from(u128::from(seis) >> 64).unwrap_or(u64::MAX)),
    }
}
