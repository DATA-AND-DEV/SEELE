//! `$SEELE_ENCONTRO` desligada enquanto um teste sobe hospedagem.
//!
//! # Por que existe
//!
//! Sem a variável, `Hospedagem::iniciar` caía no ponto de encontro de produção
//! (`seele_server::alcance::encontro::PONTO_PADRAO`), e numa máquina sem IPv4
//! global a escada tentava o degrau 4: cada rodada registrava um servidor de
//! teste no quarto da VPS de produção e deixava lá o IP público da máquina que
//! rodava a bateria. Desde a decisão (d) do lote G-servidor, sem a variável não
//! há ponto nenhum; o `nao` daqui é a intenção escrita, e vale também contra um
//! valor que outro teste deste binário tenha deixado. Nenhum teste daqui mede o
//! degrau 4 contra a produção, e o guarda da biblioteca (`recusar_a_producao`,
//! pela feature `recusar-encontro-de-producao` que o `Cargo.toml` deste crate
//! liga) é a segunda parede.
//!
//! # Como se usa
//!
//! Logo depois da vaga, e antes de qualquer `iniciar`:
//!
//! ```text
//! let _vaga = vaga::minha();
//! let _ambiente = ambiente::sem_degrau_4();
//! ```
//!
//! As variáveis locais caem na ordem inversa: a variável volta antes de a vaga
//! ser devolvida, e o próximo teste da fila a encontra como ela era. Quem
//! serializa é a vaga, e por isso este guarda não tem trava própria.
//!
//! O molde é o `AmbienteDoEncontro` de `quarto_pelo_link.rs`, que troca a
//! variável para um ponto local; este só a desliga. Não é um binário de teste:
//! quem usa faz `mod ambiente;`, como `vaga` e `porta`.

/// Guarda `$SEELE_ENCONTRO` e a devolve ao que era quando o teste acaba,
/// **inclusive em pânico**.
#[must_use = "a variável só volta quando o guarda cai; soltá-lo na hora a devolve cedo demais"]
pub(crate) struct SemDegrau4 {
    /// O que `$SEELE_ENCONTRO` era antes: `None` é «não estava definida».
    antes: Option<std::ffi::OsString>,
}

/// Desliga o degrau 4 (`nao`) até o guarda cair.
pub(crate) fn sem_degrau_4() -> SemDegrau4 {
    let antes = std::env::var_os(seele_server::alcance::encontro::VARIAVEL);
    std::env::set_var(seele_server::alcance::encontro::VARIAVEL, "nao");
    SemDegrau4 { antes }
}

impl Drop for SemDegrau4 {
    fn drop(&mut self) {
        match self.antes.take() {
            Some(valor) => std::env::set_var(seele_server::alcance::encontro::VARIAVEL, valor),
            None => std::env::remove_var(seele_server::alcance::encontro::VARIAVEL),
        }
    }
}
