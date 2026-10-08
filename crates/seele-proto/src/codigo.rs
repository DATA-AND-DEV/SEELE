//! O código de uma pessoa: o «número de telefone» da sala pessoal.
//!
//! Spec de 23/09/2026, «O código» (`:136-191`). **Este módulo é o único lugar
//! que deriva, formata, lê e confere um código**, e mora no `seele-proto`
//! porque o servidor da sala também precisa dele (ADR 0002).
//!
//! # A forma, congelada para sempre
//!
//! - **Derivação:** `SHA-256(chave pública Ed25519, 32 bytes crus)`. Os
//!   primeiros **75 bits** viram 15 símbolos de base32 Crockford.
//! - **Verificador:** um 16º símbolo, por **Luhn mod 32** sobre os 15. Ele pega
//!   toda troca de um símbolo e toda inversão de dois vizinhos, exceto a do par
//!   de valores 0 e 31, que é a fraqueza conhecida do Luhn mod N. É ele que
//!   deixa dizer «erro de digitação» sem consultar a rede.
//! - **Exibição:** `XXXX-XXXX-XXXX-XXXX`.
//! - **Leitura:** maiúsculas ou minúsculas, ignorando hífen e espaço; O vale 0,
//!   I e L valem 1; U é recusado; exatamente 16 símbolos.
//!
//! Derivação, verificador, alfabeto e leitura viram contrato público: mudar
//! qualquer um invalida todo código já distribuído. Os vetores ficam em
//! `vetores-de-codigo.json`, na raiz, conferidos por
//! `tests/vetores_de_codigo.rs`, e os testes abaixo prendem quatro deles,
//! calculados por uma implementação de referência independente.
//!
//! As marcas do quarto (`p` + os 15 símbolos de dado, com `e` ou `s` no fim)
//! saem de [`Codigo::dado`], e o `Marcas::da_sala` que as monta mora em
//! `encontro.rs`.

use std::fmt;

/// Quantos símbolos tem um código, com o verificador.
pub const SIMBOLOS: usize = 16;

/// Quantos símbolos são de dado: os 75 bits da chave.
pub const SIMBOLOS_DE_DADO: usize = 15;

/// O alfabeto base32 Crockford: sem I, L, O e U.
const ALFABETO: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// A base do verificador.
const BASE: u32 = 32;

/// O código de uma pessoa, guardado como o valor (0 a 31) de cada símbolo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Codigo {
    valores: [u8; SIMBOLOS],
}

/// Por que um texto não é um código.
///
/// Uma casca decide a frase de cada caso (ADR 0012). [`Self::Verificador`] é o
/// «erro de digitação»: o texto tem a forma de um código, e um símbolo está
/// errado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CodigoInvalido {
    /// Sem hífen e sem espaço, o texto não tem 16 símbolos.
    #[error("um código tem 16 símbolos, e este tem {simbolos}")]
    Tamanho {
        /// Quantos símbolos o texto tem.
        simbolos: usize,
    },
    /// Um caractere que não é do alfabeto.
    #[error("o caractere {caractere:?} não é de código")]
    Simbolo {
        /// O primeiro que não é.
        caractere: char,
    },
    /// Os 16 símbolos são do alfabeto, e o verificador não bate.
    #[error("o verificador do código não bate: um símbolo foi digitado errado")]
    Verificador,
}

impl Codigo {
    /// O código da chave pública Ed25519 `chave`.
    #[must_use]
    pub fn da_chave(chave: &[u8; 32]) -> Self {
        let digest: [u8; 32] = {
            use sha2::{Digest, Sha256};
            Sha256::digest(chave).into()
        };
        let mut primeiros = [0_u8; 16];
        if let Some(dez) = digest.get(..10) {
            if let Some(destino) = primeiros.get_mut(6..) {
                destino.copy_from_slice(dez);
            }
        }
        // Os 80 primeiros bits, alinhados à direita; os 75 que importam são
        // os mais significativos deles.
        let bits = u128::from_be_bytes(primeiros) >> 5;
        let mut valores = [0_u8; SIMBOLOS];
        for (indice, valor) in valores.iter_mut().take(SIMBOLOS_DE_DADO).enumerate() {
            let deslocamento = 5 * (SIMBOLOS_DE_DADO - 1 - indice);
            *valor = u8::try_from((bits >> deslocamento) & 0x1F).unwrap_or(0);
        }
        let verificador = luhn_gerar(valores.get(..SIMBOLOS_DE_DADO).unwrap_or(&[]));
        if let Some(ultimo) = valores.last_mut() {
            *ultimo = verificador;
        }
        Self { valores }
    }

    /// Lê um código como uma pessoa o digita.
    ///
    /// # Errors
    ///
    /// [`CodigoInvalido`], que diz qual dos três casos é.
    pub fn ler(texto: &str) -> Result<Self, CodigoInvalido> {
        let mut valores = [0_u8; SIMBOLOS];
        let mut quantos = 0_usize;
        for caractere in texto.chars().filter(|c| *c != '-' && !c.is_whitespace()) {
            let valor = valor_do_simbolo(caractere).ok_or(CodigoInvalido::Simbolo { caractere })?;
            if let Some(lugar) = valores.get_mut(quantos) {
                *lugar = valor;
            }
            quantos += 1;
        }
        if quantos != SIMBOLOS {
            return Err(CodigoInvalido::Tamanho { simbolos: quantos });
        }
        if !luhn_confere(&valores) {
            return Err(CodigoInvalido::Verificador);
        }
        Ok(Self { valores })
    }

    /// Se `chave` é a chave deste código: se o código dela é este.
    ///
    /// São 75 bits, e não a chave inteira. A spec de 23/09 diz o que isso
    /// custa a quem tenta se passar por alguém, e o que o pino `@CÓDIGO` faz
    /// depois da primeira ligação.
    #[must_use]
    pub fn confere_chave(&self, chave: &[u8; 32]) -> bool {
        Self::da_chave(chave) == *self
    }

    /// Os 15 símbolos de dado, sem hífen e sem o verificador: é deles que as
    /// marcas do quarto saem.
    #[must_use]
    pub fn dado(&self) -> String {
        self.valores
            .iter()
            .take(SIMBOLOS_DE_DADO)
            .map(|valor| simbolo(*valor))
            .collect()
    }
}

impl fmt::Display for Codigo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (indice, valor) in self.valores.iter().enumerate() {
            if indice > 0 && indice % 4 == 0 {
                f.write_str("-")?;
            }
            write!(f, "{}", simbolo(*valor))?;
        }
        Ok(())
    }
}

/// O caractere de um valor de 0 a 31.
fn simbolo(valor: u8) -> char {
    ALFABETO
        .get(usize::from(valor))
        .map_or('?', |byte| char::from(*byte))
}

/// O valor de um caractere digitado, ou `None` se ele não é de código.
fn valor_do_simbolo(caractere: char) -> Option<u8> {
    let maiusculo = match caractere.to_ascii_uppercase() {
        'O' => '0',
        'I' | 'L' => '1',
        outro => outro,
    };
    let byte = u8::try_from(maiusculo).ok()?;
    ALFABETO
        .iter()
        .position(|letra| *letra == byte)
        .and_then(|indice| u8::try_from(indice).ok())
}

/// A soma do Luhn mod 32, da direita para a esquerda, com o fator começando
/// em `fator`.
fn luhn_soma(valores: &[u8], mut fator: u32) -> u32 {
    let mut soma = 0_u32;
    for valor in valores.iter().rev() {
        let parcela = fator * u32::from(*valor);
        fator = if fator == 2 { 1 } else { 2 };
        soma += parcela / BASE + parcela % BASE;
    }
    soma
}

/// O símbolo verificador de `dado`.
fn luhn_gerar(dado: &[u8]) -> u8 {
    let resto = luhn_soma(dado, 2) % BASE;
    u8::try_from((BASE - resto) % BASE).unwrap_or(0)
}

/// Se os 16 valores, com o verificador no fim, fecham.
fn luhn_confere(valores: &[u8; SIMBOLOS]) -> bool {
    luhn_soma(valores, 1).is_multiple_of(BASE)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Os vetores que uma implementação de referência **independente**, em
    /// Python, calculou em 08/10/2026 (SHA-256 da chave, 75 bits, Crockford,
    /// Luhn mod 32). Duas implementações concordando é o que separa um vetor de
    /// um espelho do código.
    const VETORES: [(&str, [u8; 32], &str); 4] = [
        ("zeros", [0; 32], "CSM7-NBFR-CAYQ-EV4Y"),
        ("uns", [1; 32], "EB6P-X112-RG3Z-PV8S"),
        (
            "contagem",
            [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
                23, 24, 25, 26, 27, 28, 29, 30, 31,
            ],
            "CC6W-TAB6-RGSP-D481",
        ),
        ("ff", [0xFF; 32], "NYB1-6XGF-E9HN-ZFD2"),
    ];

    #[test]
    fn o_codigo_de_cada_chave_e_o_da_referencia() {
        for (nome, chave, esperado) in VETORES {
            assert_eq!(
                Codigo::da_chave(&chave).to_string(),
                esperado,
                "o vetor {nome} divergiu da implementação de referência"
            );
        }
    }

    #[test]
    fn o_codigo_lido_e_o_mesmo_e_confere_a_chave() {
        for (nome, chave, esperado) in VETORES {
            let lido = Codigo::ler(esperado).unwrap();
            assert_eq!(lido, Codigo::da_chave(&chave), "{nome}");
            assert!(lido.confere_chave(&chave), "{nome}");
        }
        let zeros = Codigo::ler("CSM7-NBFR-CAYQ-EV4Y").unwrap();
        assert!(!zeros.confere_chave(&[1; 32]));
    }

    #[test]
    fn a_leitura_e_tolerante_ao_que_uma_pessoa_digita() {
        let certo = Codigo::ler("CSM7-NBFR-CAYQ-EV4Y").unwrap();
        for variante in [
            "csm7-nbfr-cayq-ev4y",
            "CSM7NBFRCAYQEV4Y",
            "CSM7 NBFR CAYQ EV4Y",
            " csm7 nbfr-cayq ev4y ",
        ] {
            assert_eq!(Codigo::ler(variante).unwrap(), certo, "{variante:?}");
        }
        // O → 0, e I ou L → 1.
        let com_zero_e_um = Codigo::ler("EB6P-X112-RG3Z-PV8S").unwrap();
        assert_eq!(Codigo::ler("EB6P-XIl2-RG3Z-PV8S").unwrap(), com_zero_e_um);
        assert_eq!(
            Codigo::ler("CC6W-TAB6-RGSP-D481").unwrap(),
            Codigo::ler("cc6w-tab6-rgsp-d48l").unwrap()
        );
    }

    /// Cada caractere, e o valor que ele tem: o alfabeto Crockford, com as
    /// letras que se confundem lidas como o número que parecem.
    #[test]
    fn o_valor_de_cada_simbolo() {
        for (indice, simbolo) in ALFABETO.iter().enumerate() {
            let valor = u8::try_from(indice).unwrap();
            assert_eq!(valor_do_simbolo(char::from(*simbolo)), Some(valor));
            assert_eq!(
                valor_do_simbolo(char::from(*simbolo).to_ascii_lowercase()),
                Some(valor)
            );
        }
        for (caractere, valor) in [('O', 0), ('o', 0), ('I', 1), ('i', 1), ('L', 1), ('l', 1)] {
            assert_eq!(valor_do_simbolo(caractere), Some(valor), "{caractere}");
        }
        for caractere in ['U', 'u', '-', ' ', 'Ç', '*'] {
            assert_eq!(valor_do_simbolo(caractere), None, "{caractere}");
        }
    }

    #[test]
    fn o_que_nao_e_codigo_e_recusado_com_o_motivo() {
        assert_eq!(
            Codigo::ler("CSM7-NBFR-CAYQ-EV4"),
            Err(CodigoInvalido::Tamanho { simbolos: 15 })
        );
        assert_eq!(
            Codigo::ler("CSM7-NBFR-CAYQ-EV4YY"),
            Err(CodigoInvalido::Tamanho { simbolos: 17 })
        );
        assert_eq!(
            Codigo::ler("CSM7-NBFR-CAYQ-EV4U"),
            Err(CodigoInvalido::Simbolo { caractere: 'U' })
        );
        assert_eq!(
            Codigo::ler("CSM7-NBFR-CAYQ-EV4Ç"),
            Err(CodigoInvalido::Simbolo { caractere: 'Ç' })
        );
        // O último símbolo trocado: o verificador não bate.
        assert_eq!(
            Codigo::ler("CSM7-NBFR-CAYQ-EV4Z"),
            Err(CodigoInvalido::Verificador)
        );
    }

    /// O verificador pega **toda** troca de um símbolo, em toda posição.
    #[test]
    fn toda_troca_de_um_simbolo_e_pega() {
        for (nome, chave, _) in VETORES {
            let certo = Codigo::da_chave(&chave);
            for posicao in 0..SIMBOLOS {
                for valor in 0..32_u8 {
                    if valor == certo.valores[posicao] {
                        continue;
                    }
                    let mut errado = certo.valores;
                    errado[posicao] = valor;
                    assert!(
                        !luhn_confere(&errado),
                        "{nome}: trocar a posição {posicao} por {valor} passou"
                    );
                }
            }
        }
    }

    /// E toda inversão de dois vizinhos, menos a do par de valores 0 e 31 —
    /// a fraqueza conhecida do Luhn mod N, que a spec de 23/09 registra.
    #[test]
    fn toda_inversao_de_vizinhos_e_pega_menos_a_de_0_e_31() {
        for a in 0..32_u8 {
            for b in 0..32_u8 {
                if a == b {
                    continue;
                }
                let mut valores = [0_u8; SIMBOLOS];
                valores[6] = a;
                valores[7] = b;
                valores[SIMBOLOS - 1] = 0;
                let verificador = luhn_gerar(&valores[..SIMBOLOS_DE_DADO]);
                valores[SIMBOLOS - 1] = verificador;
                let mut invertido = valores;
                invertido.swap(6, 7);
                let fraqueza = (a == 0 && b == 31) || (a == 31 && b == 0);
                assert_eq!(luhn_confere(&invertido), fraqueza, "inverter {a} e {b}");
            }
        }
    }

    #[test]
    fn o_dado_sao_os_quinze_primeiros_simbolos() {
        let zeros = Codigo::da_chave(&[0; 32]);
        assert_eq!(zeros.dado(), "CSM7NBFRCAYQEV4");
        assert_eq!(zeros.dado().len(), SIMBOLOS_DE_DADO);
    }
}
