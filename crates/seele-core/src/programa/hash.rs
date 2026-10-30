//! O hash declarado de um artefato, e qual vale quando a origem dá vários.
//!
//! Spec da API 8, §5: valem `sha1`, `sha256` e `sha512`, porque é o que as
//! origens publicam, e **vale o mais forte que a origem der**. O `.mrpack` traz
//! `sha1` e `sha512` do mesmo arquivo; conferir só o `sha1` seria aceitar o
//! hash que já tem colisão construída quando o outro estava ali do lado.

use sha1::Digest as _;

/// Com que um hash foi calculado.
///
/// A ordem das variantes **é** a ordem de força, e é dela que
/// [`o_mais_forte`] depende: `Sha1 < Sha256 < Sha512`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AlgoritmoDeHash {
    /// 40 caracteres em hex. Tem colisão construída (SHAttered, 2017); vale só
    /// quando a origem não dá outro.
    Sha1,
    /// 64 caracteres em hex.
    Sha256,
    /// 128 caracteres em hex.
    Sha512,
}

impl AlgoritmoDeHash {
    /// Quantos caracteres tem o hash em hexadecimal.
    #[must_use]
    pub const fn tamanho_em_hex(self) -> usize {
        match self {
            Self::Sha1 => 40,
            Self::Sha256 => 64,
            Self::Sha512 => 128,
        }
    }

    /// O nome, como as origens o escrevem.
    #[must_use]
    pub const fn nome(self) -> &'static str {
        match self {
            Self::Sha1 => "sha1",
            Self::Sha256 => "sha256",
            Self::Sha512 => "sha512",
        }
    }
}

impl std::fmt::Display for AlgoritmoDeHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.nome())
    }
}

/// Um hash que o MOD declarou antes de baixar.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HashDeclarado {
    /// Com que se calculou.
    pub algoritmo: AlgoritmoDeHash,
    /// Hexadecimal. A caixa não importa na comparação; o tamanho e os dígitos,
    /// sim ([`HashDeclarado::e_valido`]).
    pub hex: String,
}

impl HashDeclarado {
    /// Se o hex tem o tamanho do algoritmo e só dígitos hexadecimais.
    #[must_use]
    pub fn e_valido(&self) -> bool {
        self.hex.len() == self.algoritmo.tamanho_em_hex()
            && self.hex.bytes().all(|b| b.is_ascii_hexdigit())
    }
}

/// O hash que vale entre os que a origem deu: o do algoritmo mais forte.
///
/// Dois do mesmo algoritmo: o primeiro. A lista vazia não tem nenhum, e quem
/// chama transforma isso em recusa ([`super::Recusa::SemHash`]).
#[must_use]
pub fn o_mais_forte(hashes: &[HashDeclarado]) -> Option<&HashDeclarado> {
    let mut melhor: Option<&HashDeclarado> = None;
    for hash in hashes {
        if melhor.is_none_or(|atual| hash.algoritmo > atual.algoritmo) {
            melhor = Some(hash);
        }
    }
    melhor
}

/// O cálculo incremental, pedaço a pedaço enquanto o corpo chega.
pub(crate) enum Calculo {
    Sha1(sha1::Sha1),
    Sha256(sha2::Sha256),
    Sha512(sha2::Sha512),
}

impl Calculo {
    pub(crate) fn novo(algoritmo: AlgoritmoDeHash) -> Self {
        match algoritmo {
            AlgoritmoDeHash::Sha1 => Self::Sha1(sha1::Sha1::new()),
            AlgoritmoDeHash::Sha256 => Self::Sha256(sha2::Sha256::new()),
            AlgoritmoDeHash::Sha512 => Self::Sha512(sha2::Sha512::new()),
        }
    }

    pub(crate) fn somar(&mut self, pedaco: &[u8]) {
        match self {
            Self::Sha1(h) => h.update(pedaco),
            Self::Sha256(h) => h.update(pedaco),
            Self::Sha512(h) => h.update(pedaco),
        }
    }

    /// O hash em hex minúsculo.
    pub(crate) fn hex(self) -> String {
        let bytes: Vec<u8> = match self {
            Self::Sha1(h) => h.finalize().to_vec(),
            Self::Sha256(h) => h.finalize().to_vec(),
            Self::Sha512(h) => h.finalize().to_vec(),
        };
        let mut texto = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            use std::fmt::Write as _;
            let _ = write!(texto, "{byte:02x}");
        }
        texto
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(algoritmo: AlgoritmoDeHash, c: char) -> HashDeclarado {
        HashDeclarado {
            algoritmo,
            hex: std::iter::repeat_n(c, algoritmo.tamanho_em_hex()).collect(),
        }
    }

    #[test]
    fn vale_o_mais_forte_que_a_origem_der() {
        let sha1 = h(AlgoritmoDeHash::Sha1, 'a');
        let sha256 = h(AlgoritmoDeHash::Sha256, 'b');
        let sha512 = h(AlgoritmoDeHash::Sha512, 'c');
        // O `.mrpack`: `sha1` e `sha512`, nesta ordem.
        let mrpack = [sha1.clone(), sha512.clone()];
        assert_eq!(o_mais_forte(&mrpack), Some(&sha512));
        // Ordem trocada não muda a escolha.
        let trocado = [sha512.clone(), sha256.clone(), sha1.clone()];
        assert_eq!(o_mais_forte(&trocado), Some(&sha512));
        assert_eq!(o_mais_forte(&[sha1.clone(), sha256.clone()]), Some(&sha256));
        assert_eq!(o_mais_forte(&[sha1.clone()]), Some(&sha1));
        assert_eq!(o_mais_forte(&[]), None);
    }

    #[test]
    fn os_vetores_conhecidos_de_abc() {
        let calcular = |a| {
            let mut c = Calculo::novo(a);
            c.somar(b"a");
            c.somar(b"bc");
            c.hex()
        };
        assert_eq!(
            calcular(AlgoritmoDeHash::Sha1),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            calcular(AlgoritmoDeHash::Sha256),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(calcular(AlgoritmoDeHash::Sha512).starts_with("ddaf35a193617aba"));
    }

    #[test]
    fn hex_de_tamanho_errado_ou_com_letra_fora_nao_vale() {
        assert!(h(AlgoritmoDeHash::Sha256, 'F').e_valido());
        let mut curto = h(AlgoritmoDeHash::Sha256, 'a');
        curto.hex.pop();
        assert!(!curto.e_valido());
        assert!(!h(AlgoritmoDeHash::Sha1, 'g').e_valido());
    }
}
