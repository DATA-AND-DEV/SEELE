//! **Só nos testes** (feature `costura-de-teste`, que só a `[dev-dependencies]`
//! do `seele-app` liga): o ponto de encontro de quem liga, por casa.
//!
//! # Por que por casa, e não global
//!
//! A variável de ambiente é do processo, e os testes do app rodam juntos num
//! processo só: pôr `$SEELE_ENCONTRO` num deles mudava o ponto de todos os
//! outros. Cada teste tem a sua casa (`ConnectConfig::home`, uma pasta
//! temporária), e o ponto dito para ela não alcança nenhuma outra.
//!
//! # Por que num arquivo próprio
//!
//! Os guardas de texto do `lib.rs` leem o programa como o que vem antes do
//! primeiro `cfg(test)` dele. Um módulo de teste aqui dentro, no topo daquele
//! arquivo, cortava o programa no meio, e cinco guardas reprovaram sem motivo.

use std::collections::BTreeMap;
use std::sync::Mutex;

static PONTOS: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

/// Enquanto o guarda viver, as ligações por código desta casa perguntam em
/// `ponto` (`host:porta`).
#[must_use = "o ponto vale só enquanto o guarda vive"]
pub fn usar_o_ponto_nesta_casa(home: &str, ponto: &str) -> PontoDaCasa {
    if let Ok(mut pontos) = PONTOS.lock() {
        pontos.insert(home.to_owned(), ponto.to_owned());
    }
    PontoDaCasa {
        home: home.to_owned(),
    }
}

/// O ponto dito para esta casa, se há um.
pub(crate) fn ponto_da_casa(home: &str) -> Option<String> {
    PONTOS.lock().ok()?.get(home).cloned()
}

/// Desfaz [`usar_o_ponto_nesta_casa`] ao cair.
#[derive(Debug)]
pub struct PontoDaCasa {
    home: String,
}

impl Drop for PontoDaCasa {
    fn drop(&mut self) {
        if let Ok(mut pontos) = PONTOS.lock() {
            pontos.remove(&self.home);
        }
    }
}

#[cfg(test)]
mod testes {
    use super::{ponto_da_casa, usar_o_ponto_nesta_casa};

    #[test]
    fn o_ponto_vale_so_para_a_casa_dita_e_so_enquanto_o_guarda_vive() {
        let guarda = usar_o_ponto_nesta_casa("/tmp/casa-a", "127.0.0.1:7000");
        assert_eq!(
            crate::ponto_de_quem_liga_desta_casa("/tmp/casa-a").as_deref(),
            Some("127.0.0.1:7000")
        );
        assert_eq!(
            ponto_da_casa("/tmp/casa-b"),
            None,
            "o ponto vazou para outra casa"
        );
        assert_eq!(
            crate::ponto_de_quem_liga_desta_casa("/tmp/casa-b"),
            crate::ponto_de_encontro(),
            "a outra casa não ficou com o ponto do produto"
        );
        drop(guarda);
        assert_eq!(
            ponto_da_casa("/tmp/casa-a"),
            None,
            "o ponto sobreviveu ao guarda"
        );
    }
}
