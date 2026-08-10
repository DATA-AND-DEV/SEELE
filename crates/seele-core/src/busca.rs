//! Finding a term in what a shell is showing.
//!
//! This lives here rather than in a shell because it would be identical in
//! both — the rule `seele-tui::view` already wrote down. How case and accent
//! fold, and the order `n` and `N` walk in, is one decision with one set of
//! tests.
//!
//! # The accent table is Portuguese, and only Portuguese
//!
//! Folding is one character to one character, which is what keeps the offsets
//! this module hands out aligned with the text the caller passed in. Pulling
//! `unicode-normalization` into the core for twelve characters is not a trade
//! this project makes. The cost is real and stated: an accent outside
//! Portuguese does not fold.
//!
//! # Callers pass normalised bodies
//!
//! [`normalizar`] collapses whitespace the same way `seele-tui::ui::wrap` does
//! and the way HTML does. Both shells already show text with runs of
//! whitespace collapsed, so searching the normalised body is searching exactly
//! what is on screen.

/// Where a term matched.
///
/// `Serialize` because the desktop shell sends these across the Tauri bridge:
/// with accent folding in the middle, a frontend cannot work out where a match
/// began on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Casamento {
    /// Index into the list of bodies, in the order the shell drew them.
    pub mensagem: usize,
    /// Character offset where the match starts.
    pub inicio: usize,
    /// Character offset one past the end.
    pub fim: usize,
}

/// Collapses runs of whitespace, the way both shells already display text.
#[must_use]
pub fn normalizar(texto: &str) -> String {
    texto.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Folds one character: lowercase first, then strip the Portuguese accent.
///
/// `to_lowercase` can yield more than one character for a few code points;
/// taking the first keeps this one-to-one, which is what the offsets depend on.
fn dobrar_char(character: char) -> char {
    let baixo = character.to_lowercase().next().unwrap_or(character);
    match baixo {
        'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ç' => 'c',
        'ñ' => 'n',
        outro => outro,
    }
}

fn dobrar(texto: &str) -> Vec<char> {
    texto.chars().map(dobrar_char).collect()
}

/// Every place `termo` occurs in `texto`, as character ranges, left to right.
///
/// Public because a shell that wraps text into segments needs to light the
/// matches inside a segment without redoing the folding rule.
#[must_use]
pub fn ocorrencias(texto: &str, termo: &str) -> Vec<(usize, usize)> {
    let alvo = dobrar(termo.trim());
    if alvo.is_empty() {
        return Vec::new();
    }
    let corpo = dobrar(texto);
    // `windows` yields nothing when the body is shorter than the term, and
    // panics only on a zero width — ruled out above.
    corpo
        .windows(alvo.len())
        .enumerate()
        .filter(|(_, janela)| *janela == alvo.as_slice())
        .map(|(inicio, _)| (inicio, inicio + alvo.len()))
        .collect()
}

/// A search over what a shell is showing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Busca {
    casamentos: Vec<Casamento>,
    cursor: usize,
}

impl Busca {
    /// Runs the term over the bodies, in drawing order.
    ///
    /// Rebuilt wholesale rather than patched, like `view::project`: an index
    /// that drifts out of step with the list is a bug that only shows up after
    /// an hour of use.
    #[must_use]
    pub fn nova<S: AsRef<str>>(corpos: impl IntoIterator<Item = S>, termo: &str) -> Self {
        let casamentos = corpos
            .into_iter()
            .enumerate()
            .flat_map(|(mensagem, corpo)| {
                ocorrencias(corpo.as_ref(), termo)
                    .into_iter()
                    .map(move |(inicio, fim)| Casamento {
                        mensagem,
                        inicio,
                        fim,
                    })
            })
            .collect();
        Self {
            casamentos,
            cursor: 0,
        }
    }

    /// Whether nothing matched.
    #[must_use]
    pub fn vazia(&self) -> bool {
        self.casamentos.is_empty()
    }

    /// Every match, in drawing order.
    ///
    /// A shell that paints the whole history at once — the desktop one does —
    /// needs all of them, not just the one the cursor is on.
    #[must_use]
    pub fn casamentos(&self) -> &[Casamento] {
        &self.casamentos
    }

    /// The match the cursor is on.
    #[must_use]
    pub fn atual(&self) -> Option<Casamento> {
        self.casamentos.get(self.cursor).copied()
    }

    /// `n` — the next match, wrapping past the end.
    pub fn proxima(&mut self) -> Option<Casamento> {
        self.andar(1)
    }

    /// `N` — the previous match, wrapping past the start.
    pub fn anterior(&mut self) -> Option<Casamento> {
        self.andar(-1)
    }

    fn andar(&mut self, passo: isize) -> Option<Casamento> {
        let total = self.casamentos.len();
        if total == 0 {
            return None;
        }
        // Wrapping on purpose: for somebody searching, the last occurrence and
        // the first are neighbours.
        let total_i = isize::try_from(total).unwrap_or(isize::MAX);
        let atual = isize::try_from(self.cursor).unwrap_or(0);
        let proximo = (atual + passo).rem_euclid(total_i);
        self.cursor = usize::try_from(proximo).unwrap_or(0);
        self.atual()
    }

    /// `(1, 3)` for drawing "[1/3]". `(0, 0)` when nothing matched.
    #[must_use]
    pub fn posicao(&self) -> (usize, usize) {
        if self.casamentos.is_empty() {
            return (0, 0);
        }
        (self.cursor + 1, self.casamentos.len())
    }

    /// Which occurrence *within its own message* the cursor is on, from zero.
    ///
    /// A shell that lights matches segment by segment counts them the same way
    /// and needs this to tell the current one from its neighbours.
    #[must_use]
    pub fn ordinal_na_mensagem(&self) -> Option<usize> {
        let atual = self.atual()?;
        Some(
            self.casamentos
                .iter()
                .take(self.cursor)
                .filter(|casamento| casamento.mensagem == atual.mensagem)
                .count(),
        )
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, reason = "test vectors are fixed and local")]
mod tests {
    use super::*;

    #[test]
    fn a_caixa_nao_importa() {
        let busca = Busca::nova(["o SYNC caiu"], "sync");
        assert_eq!(busca.posicao(), (1, 1));
        assert_eq!(
            busca.atual(),
            Some(Casamento {
                mensagem: 0,
                inicio: 2,
                fim: 6
            })
        );
    }

    #[test]
    fn o_acento_nao_importa_e_o_intervalo_continua_certo() {
        // A tabela é 1:1 por caractere, e é exatamente aqui que ela se
        // denunciaria se deixasse de ser: `não` tem três caracteres, e o
        // intervalo devolvido tem que ter três.
        let busca = Busca::nova(["não foi"], "nao");
        let casamento = busca.atual().unwrap_or(Casamento {
            mensagem: 9,
            inicio: 9,
            fim: 9,
        });
        assert_eq!(casamento.inicio, 0);
        assert_eq!(casamento.fim, 3);
    }

    #[test]
    fn o_acento_dobra_nos_dois_sentidos() {
        assert_eq!(Busca::nova(["nao foi"], "não").posicao(), (1, 1));
        assert_eq!(Busca::nova(["MAIÚSCULA"], "maiuscula").posicao(), (1, 1));
    }

    #[test]
    fn o_cursor_da_a_volta_nas_duas_pontas() {
        let mut busca = Busca::nova(["sync", "sync", "sync"], "sync");
        assert_eq!(busca.posicao(), (1, 3));
        busca.proxima();
        busca.proxima();
        assert_eq!(busca.posicao(), (3, 3));
        // Do fim, `n` volta ao começo: numa conversa a última ocorrência e a
        // primeira são vizinhas para quem está procurando.
        busca.proxima();
        assert_eq!(busca.posicao(), (1, 3));
        busca.anterior();
        assert_eq!(busca.posicao(), (3, 3));
    }

    #[test]
    fn varias_ocorrencias_na_mesma_mensagem_contam_separado() {
        let busca = Busca::nova(["sync e sync"], "sync");
        assert_eq!(busca.posicao(), (1, 2));
        assert_eq!(busca.ordinal_na_mensagem(), Some(0));
    }

    #[test]
    fn o_ordinal_conta_dentro_da_mensagem_e_nao_no_total() {
        let mut busca = Busca::nova(["sync", "sync e sync"], "sync");
        busca.proxima();
        assert_eq!(busca.atual().map(|c| c.mensagem), Some(1));
        assert_eq!(busca.ordinal_na_mensagem(), Some(0));
        busca.proxima();
        assert_eq!(busca.ordinal_na_mensagem(), Some(1));
    }

    #[test]
    fn termo_vazio_ou_so_espaco_nao_casa_nada() {
        assert!(Busca::nova(["sync"], "").vazia());
        assert!(Busca::nova(["sync"], "   ").vazia());
        assert_eq!(Busca::nova(["sync"], "").posicao(), (0, 0));
    }

    #[test]
    fn sem_casamento_nao_e_erro_nem_panico() {
        let mut busca = Busca::nova(["sync caiu"], "harmônicos");
        assert!(busca.vazia());
        assert_eq!(busca.atual(), None);
        assert_eq!(busca.proxima(), None);
        assert_eq!(busca.anterior(), None);
        assert_eq!(busca.posicao(), (0, 0));
    }

    #[test]
    fn um_termo_maior_que_o_corpo_nao_estoura() {
        assert!(Busca::nova(["oi"], "oi mesmo").vazia());
        assert!(Busca::nova([""], "oi").vazia());
    }

    #[test]
    fn normalizar_colapsa_o_espaco_como_o_wrap_da_tui_faz() {
        // `seele-tui::ui::wrap` usa `split_whitespace`, que colapsa. Sem esta
        // normalização os deslocamentos apontariam para o lugar errado em
        // qualquer corpo com espaço duplo.
        assert_eq!(normalizar("a  b\tc\nd "), "a b c d");
        assert_eq!(normalizar("   "), "");
    }

    #[test]
    fn ocorrencias_acha_todas_e_em_ordem() {
        assert_eq!(ocorrencias("sync e sync", "sync"), vec![(0, 4), (7, 11)]);
        assert_eq!(ocorrencias("nada", "sync"), Vec::new());
    }
}
