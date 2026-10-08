//! A linha de servidor: o que o launcher guarda, uma de cada.
//!
//! Spec 0.16, §1: a versão do **app** (0.16, 0.16.1…) é o que o atualizador
//! oferece, e a versão do **servidor** (Servidor 1.0, 1.1…) é o que o app e o
//! `seeled` falam. Um 1.x nunca quebra o fio, então a família 1.x inteira é
//! **uma linha**: o launcher guarda o app mais novo dela, e a 0.16.1 substitui a
//! 0.16.
//!
//! Antes das duas numerações, cada 0.N era uma linha própria, porque a 0.15 não
//! falava com a 0.14. Daí as duas formas do texto canônico: `0.N` e `M`.
//!
//! # Por que este tipo tem `Ord` e a `Versao` não
//!
//! A [`crate::Versao`] identifica uma publicação, e quem ordena publicações é
//! o manifesto: `0.10.5-1` saiu depois de `0.10.5`, e o semver diria o
//! contrário. A linha é um número nosso, sem sufixo de pré-lançamento, e a 1
//! vem depois da 0.15 em qualquer leitura. É essa ordem que põe a linha mais
//! nova primeiro no manifesto e no padrão da tela de hospedar.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::fmt;

/// A última linha de antes das duas numerações.
///
/// Até a 0.15, a versão do app e a do servidor eram uma coisa só: o app 0.15.1
/// falava o Servidor 0.15. O app 0.16 fala o Servidor 1.0, e daí em diante a
/// linha de uma versão de app só se sabe pelo manifesto (`unit.server`).
pub const ULTIMA_LINHA_SEM_NUMERACAO_PROPRIA: u32 = 15;

/// Por que um texto não serve como linha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinhaInvalida {
    /// Texto vazio.
    Vazia,
    /// Não tem a forma esperada: `0.N` ou `M` para uma linha, `M.N` para uma
    /// versão de servidor.
    Malformada {
        /// O que veio.
        texto: String,
    },
}

/// Uma linha de servidor, já conferida.
///
/// Guardada como números, e não como texto. É o que dá a ordem certa
/// (`0.9 < 0.15`, que o texto erraria) e o que impede `01` e `1` de virarem
/// duas pastas para os mesmos dados.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Linha {
    maior: u32,
    /// `Some` só na forma `0.N`.
    menor: Option<u32>,
}

impl Linha {
    /// Confere o texto canônico: `0.N` (N ≥ 1) ou `M` (M ≥ 1).
    ///
    /// # Errors
    ///
    /// [`LinhaInvalida`], quando o texto não tem nenhuma das duas formas.
    pub fn nova(bruto: &str) -> Result<Self, LinhaInvalida> {
        if bruto.is_empty() {
            return Err(LinhaInvalida::Vazia);
        }
        let malformada = || LinhaInvalida::Malformada {
            texto: bruto.to_owned(),
        };
        let partes: Vec<&str> = bruto.split('.').collect();
        match partes.as_slice() {
            ["0", menor] => match numero(menor) {
                Some(n) if n >= 1 => Ok(Self {
                    maior: 0,
                    menor: Some(n),
                }),
                _ => Err(malformada()),
            },
            [maior] => match numero(maior) {
                Some(m) if m >= 1 => Ok(Self {
                    maior: m,
                    menor: None,
                }),
                _ => Err(malformada()),
            },
            _ => Err(malformada()),
        }
    }

    /// A linha de uma versão de servidor. `1.0` e `1.3` são da linha `1`, e
    /// `0.15` é a linha `0.15`.
    ///
    /// # Errors
    ///
    /// [`LinhaInvalida`], quando o texto não é `M.N`.
    pub fn do_servidor(versao_do_servidor: &str) -> Result<Self, LinhaInvalida> {
        if versao_do_servidor.is_empty() {
            return Err(LinhaInvalida::Vazia);
        }
        let malformada = || LinhaInvalida::Malformada {
            texto: versao_do_servidor.to_owned(),
        };
        let mut partes = versao_do_servidor.split('.');
        let (Some(maior), Some(menor), None) = (partes.next(), partes.next(), partes.next()) else {
            return Err(malformada());
        };
        let (Some(maior), Some(menor)) = (numero(maior), numero(menor)) else {
            return Err(malformada());
        };
        match (maior, menor) {
            (0, 0) => Err(malformada()),
            (0, n) => Ok(Self {
                maior: 0,
                menor: Some(n),
            }),
            (m, _) => Ok(Self {
                maior: m,
                menor: None,
            }),
        }
    }

    /// A linha de uma versão de **app** publicada antes das duas numerações.
    ///
    /// `0.15.1` dá `0.15`. Uma versão de app da 0.16 em diante devolve `None`:
    /// a regra acabou ali, e inventar a linha de uma 0.17 seria pior que não
    /// saber. Quem sabe é o manifesto.
    #[must_use]
    pub fn da_versao_legada_do_app(versao: &str) -> Option<Self> {
        let mut partes = versao.split(['.', '-']);
        let maior = numero(partes.next()?)?;
        let menor = numero(partes.next()?)?;
        // A terceira parte tem de existir: `0.15` é versão de servidor.
        partes.next()?;
        (maior == 0 && (1..=ULTIMA_LINHA_SEM_NUMERACAO_PROPRIA).contains(&menor)).then_some(Self {
            maior: 0,
            menor: Some(menor),
        })
    }

    /// O texto canônico, para a tela, o manifesto e o disco.
    #[must_use]
    pub fn como_texto(&self) -> String {
        match self.menor {
            Some(n) => format!("{}.{n}", self.maior),
            None => self.maior.to_string(),
        }
    }

    /// O nome da pasta de dados desta linha, dentro de `dados/`.
    ///
    /// É por **linha**, e não por versão: um conserto da linha (a 0.15.3)
    /// herda os dados da 0.15.2 sem cópia nenhuma.
    #[must_use]
    pub fn pasta_de_dados(&self) -> String {
        format!("servidor-{}", self.como_texto())
    }

    fn chave(&self) -> (u32, u32) {
        (self.maior, self.menor.unwrap_or(0))
    }
}

/// Um número decimal sem zero à esquerda. `01` não vale como `1`: dois textos
/// para a mesma linha seriam duas pastas para os mesmos dados.
fn numero(texto: &str) -> Option<u32> {
    if texto.is_empty()
        || !texto.bytes().all(|b| b.is_ascii_digit())
        || (texto.len() > 1 && texto.starts_with('0'))
    {
        return None;
    }
    texto.parse().ok()
}

impl PartialOrd for Linha {
    fn partial_cmp(&self, outra: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(outra))
    }
}

impl Ord for Linha {
    fn cmp(&self, outra: &Self) -> std::cmp::Ordering {
        self.chave().cmp(&outra.chave())
    }
}

impl fmt::Display for Linha {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.como_texto())
    }
}

impl serde::Serialize for Linha {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.como_texto())
    }
}

impl<'de> serde::Deserialize<'de> for Linha {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let bruto = String::deserialize(d)?;
        Self::nova(&bruto)
            .map_err(|erro| serde::de::Error::custom(format!("linha recusada: {erro:?}")))
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn a_versao_do_servidor_vira_a_linha_dela() {
        for (servidor, linha) in [("1.0", "1"), ("1.3", "1"), ("2.0", "2"), ("0.15", "0.15")] {
            assert_eq!(
                Linha::do_servidor(servidor).unwrap().como_texto(),
                linha,
                "com {servidor}"
            );
        }
        for ruim in ["", "1", "0.0", "1.0.0", "um.zero", "01.0"] {
            assert!(Linha::do_servidor(ruim).is_err(), "aceitou {ruim:?}");
        }
    }

    #[test]
    fn as_versoes_de_app_de_antes_das_duas_numeracoes_dizem_a_linha() {
        let legada = Linha::nova("0.15").unwrap();
        assert_eq!(
            Linha::da_versao_legada_do_app("0.15.1"),
            Some(legada.clone())
        );
        assert_eq!(Linha::da_versao_legada_do_app("0.15.2"), Some(legada));
        assert_eq!(
            Linha::da_versao_legada_do_app("0.10.5-1"),
            Some(Linha::nova("0.10").unwrap())
        );
        // A 0.16 fala o Servidor 1.0: daqui em diante só o manifesto sabe.
        assert_eq!(Linha::da_versao_legada_do_app("0.16.0"), None);
        assert_eq!(Linha::da_versao_legada_do_app("1.0.0"), None);
        // Duas partes é versão de servidor, não de app.
        assert_eq!(Linha::da_versao_legada_do_app("0.15"), None);
    }

    #[test]
    fn so_o_texto_canonico_e_uma_linha() {
        for bom in ["0.15", "0.9", "1", "2", "10"] {
            assert!(Linha::nova(bom).is_ok(), "recusou {bom}");
        }
        for ruim in [
            "", "0", "0.0", "1.0", "01", "0.015", "../1", "1/2", "0.15.1", "x", "-1",
        ] {
            assert!(Linha::nova(ruim).is_err(), "aceitou {ruim:?}");
        }
    }

    /// As linhas têm ordem, e é por isso que este tipo tem `Ord` e a `Versao`
    /// não tem: o número da linha é nosso, sem sufixo de pré-lançamento.
    #[test]
    fn a_linha_mais_nova_vem_depois() {
        let l = |t: &str| Linha::nova(t).unwrap();
        assert!(l("0.9") < l("0.15"));
        assert!(l("0.15") < l("1"));
        assert!(l("1") < l("2"));
        assert!(l("2") < l("10"));
    }

    #[test]
    fn a_pasta_de_dados_e_por_linha() {
        assert_eq!(
            Linha::nova("0.15").unwrap().pasta_de_dados(),
            "servidor-0.15"
        );
        assert_eq!(Linha::nova("1").unwrap().pasta_de_dados(), "servidor-1");
    }

    #[test]
    fn a_linha_atravessa_o_json_como_texto() {
        let linha: Linha = serde_json::from_str("\"0.15\"").unwrap();
        assert_eq!(linha, Linha::nova("0.15").unwrap());
        assert_eq!(serde_json::to_string(&linha).unwrap(), "\"0.15\"");
        assert!(serde_json::from_str::<Linha>("\"../x\"").is_err());
    }
}
