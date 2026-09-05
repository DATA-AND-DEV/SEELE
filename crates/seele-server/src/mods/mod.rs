//! Where a MOD's server half runs.
//!
//! ADR 0044. A MOD is third-party code with real access, by decision — the
//! product's rules protect the product and do not reach a MOD. What this module
//! does is not restrict it: it is to keep one bad MOD from taking the room down
//! with it.
//!
//! # The two ceilings, and why they are not politeness
//!
//! The server is an SFU budgeted at 1 vCPU / 512 MB — `xtask/src/check_deps.rs`
//! forbids two dependency edges over that number. A MOD in an infinite loop
//! cannot be the difference between the room working and not, so:
//!
//! - **memory**, per runtime, refused rather than swapped;
//! - **time**, by interrupt handler, counted in interpreter steps.
//!
//! Both measured in `spikes/mod-em-js/`, together with the property that makes
//! them useful: **the context survives either one**. A MOD that blows up is
//! disabled and the room continues.
//!
//! # One runtime per MOD, and not one shared
//!
//! Two MODs sharing a `globalThis` is two authors fighting over a name, and one
//! MOD reading what another wrote. Separate runtimes also mean the memory
//! ceiling is per MOD rather than per server, which is the number a host can
//! reason about.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::Result;
use rquickjs::{Context, Function, Runtime};

/// Memory one MOD may hold, in bytes.
///
/// 8 MiB against a 512 MB server: sixty MODs at the ceiling would still leave
/// the SFU its half. The spike measured 5,9 MB of RSS for **fifty** contexts
/// doing nothing, so this is room to work in and not a straitjacket.
const TETO_DE_MEMORIA: usize = 8 * 1024 * 1024;

/// How many times one call may be asked "keep going?" before it is cut.
///
/// **Not a step count, and the difference is a factor of five thousand.** The
/// interrupt handler is not called per bytecode operation: QuickJS polls it at
/// intervals. Measured here, on Apple Silicon, and the relationship is linear:
///
/// | operações JS | consultas | tempo |
/// |---|---|---|
/// | 100 000 | 20 | 8,2 ms |
/// | 1 000 000 | 200 | 102 ms |
/// | 10 000 000 | 2 000 | 909 ms |
///
/// So one poll ≈ 5 000 operations ≈ 0,45 ms. **500 is ≈ 2,5 milhões de
/// operações, ≈ 225 ms** — generous for a hook and bounded for a room.
///
/// The first number written here was 200 000, taken from the spike without
/// checking what it counted. That would have been **ninety seconds** of one MOD
/// holding an SFU budgeted at 1 vCPU.
///
/// Counting polls rather than milliseconds is still the right unit, and now
/// with the number to back it: the same MOD does the same work on a slow host
/// and simply takes longer, where a wall-clock ceiling would cut the slow
/// host's MOD and spare the fast one's.
const TETO_DE_CONSULTAS: usize = 500;

/// Why a MOD did not run.
///
/// **There is no `EstourouMemoria`, and the absence is deliberate.** The memory
/// ceiling works — `o_teto_de_memoria_e_aplicado` proves it — but QuickJS
/// reports it by throwing *inside the script*, so from out here it is
/// indistinguishable from a MOD that threw for any other reason. A variant that
/// is never emitted is the debt ADR 0021 already recorded once
/// (`DisconnectReason::RateLimited`, "existe e nunca é enviado"), and one is
/// enough.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Falha {
    /// The source did not compile.
    #[error("mod source did not compile")]
    NaoCarregou,
    /// It threw.
    #[error("mod threw")]
    Lancou,
    /// It went past the step ceiling.
    #[error("mod went past its step ceiling")]
    PassouDoTempo,
}

/// One MOD's runtime, and the step counter its interrupt handler reads.
struct Hospede {
    contexto: Context,
    passos: Arc<AtomicUsize>,
    /// Held so the runtime outlives its context.
    _runtime: Runtime,
}

/// Every MOD's server half, on this server.
pub struct Anfitriao {
    hospedes: BTreeMap<String, Hospede>,
    teto_de_memoria: usize,
    teto_de_consultas: usize,
}

impl Anfitriao {
    /// An empty host.
    ///
    /// # Errors
    ///
    /// Never today. It returns `Result` because loading does, and a caller that
    /// has to branch in one place and not the other invites the branch being
    /// forgotten.
    pub fn novo() -> Result<Self> {
        Self::com_tetos(TETO_DE_MEMORIA, TETO_DE_CONSULTAS)
    }

    /// A host with ceilings of its own.
    ///
    /// Exists because a ceiling nobody can vary is a ceiling nobody can prove:
    /// the test below shows the same allocation failing under a small one and
    /// passing under a large one, which is what says the number is applied
    /// rather than merely written down.
    ///
    /// It is also the shape configuration will take when a host wants to give a
    /// heavy MOD more room, and that is not accidental.
    ///
    /// # Errors
    ///
    /// Never today. See [`Self::novo`].
    pub fn com_tetos(memoria: usize, consultas: usize) -> Result<Self> {
        Ok(Self {
            hospedes: BTreeMap::new(),
            teto_de_memoria: memoria,
            teto_de_consultas: consultas,
        })
    }

    /// Compiles a MOD's server half into a runtime of its own.
    ///
    /// Loading the same identifier twice replaces it, which is what makes a MOD
    /// updatable without restarting the server.
    ///
    /// # Errors
    ///
    /// [`Falha::NaoCarregou`] when the source does not compile.
    pub fn carregar(&mut self, id: &str, fonte: &str) -> Result<(), Falha> {
        let runtime = Runtime::new().map_err(|_| Falha::NaoCarregou)?;
        runtime.set_memory_limit(self.teto_de_memoria);

        let passos = Arc::new(AtomicUsize::new(0));
        let contador = Arc::clone(&passos);
        let teto = self.teto_de_consultas;
        runtime.set_interrupt_handler(Some(Box::new(move || {
            contador.fetch_add(1, Ordering::Relaxed) > teto
        })));

        let contexto = Context::full(&runtime).map_err(|_| Falha::NaoCarregou)?;
        contexto
            .with(|ctx| ctx.eval::<(), _>(fonte))
            .map_err(|_| Falha::NaoCarregou)?;

        self.hospedes.insert(
            id.to_owned(),
            Hospede {
                contexto,
                passos,
                _runtime: runtime,
            },
        );
        Ok(())
    }

    /// Hands a MOD one moment.
    ///
    /// A MOD with no `aoAcontecer` is not a failure: it is a MOD whose server
    /// half exists for something else, and calling it is a no-op.
    ///
    /// # Errors
    ///
    /// [`Falha`] for a MOD that threw or went past a ceiling. A MOD that is not
    /// loaded is also `Lancou` — deliberately the same answer, because a caller
    /// that has to tell them apart is a caller inventing a recovery for a state
    /// it cannot fix.
    pub fn chamar(&mut self, id: &str, momento: &str, carga: &str) -> Result<(), Falha> {
        let hospede = self.hospedes.get(id).ok_or(Falha::Lancou)?;
        // Each call gets the whole budget: a MOD that was slow once is not a
        // MOD that is broken forever.
        hospede.passos.store(0, Ordering::Relaxed);

        hospede.contexto.with(|ctx| {
            let Ok(f) = ctx.globals().get::<_, Function<'_>>("aoAcontecer") else {
                return Ok(());
            };
            f.call::<_, ()>((momento, carga)).map_err(|_| {
                if hospede.passos.load(Ordering::Relaxed) > self.teto_de_consultas {
                    Falha::PassouDoTempo
                } else {
                    Falha::Lancou
                }
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn um_mod_que_carrega_responde_a_um_momento() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar(
                "seele/exemplo",
                "globalThis.aoAcontecer = (momento) => { globalThis.ultimo = momento; };",
            )
            .expect("carregar");
        anfitriao
            .chamar("seele/exemplo", "PersonJoined", "{}")
            .expect("chamar");
    }

    /// O teto de memória do ADR 0044, e a metade que importa: **a sala
    /// continua**. Um MOD que aloca sem parar é cortado, e o contexto dele
    /// segue respondendo — medido em `spikes/mod-em-js/src/bin/tetos.rs`.
    #[test]
    fn um_mod_que_aloca_sem_parar_e_cortado_e_o_contexto_sobrevive() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar(
                "seele/glutao",
                "globalThis.aoAcontecer = () => { const a = []; for (;;) a.push(new Array(1024)); };",
            )
            .expect("carregar");

        let falha = anfitriao.chamar("seele/glutao", "PersonJoined", "{}");
        // `Lancou` e não uma variante própria: QuickJS reporta o estouro
        // lançando dentro do script, e daqui de fora não há como distinguir.
        // Está escrito na doc de `Falha`, e o teto em si tem prova própria em
        // `o_teto_de_memoria_e_aplicado`.
        assert!(matches!(
            falha,
            Err(Falha::Lancou) | Err(Falha::PassouDoTempo)
        ));

        // E o contexto ainda responde.
        anfitriao
            .carregar("seele/glutao", "globalThis.aoAcontecer = () => {};")
            .expect("recarregar depois do estouro");
        anfitriao
            .chamar("seele/glutao", "PersonJoined", "{}")
            .expect("o contexto morreu junto com o estouro");
    }

    /// O teto de memória é **aplicado**, e não só escrito.
    ///
    /// A mesma alocação sob dois tetos: sob 256 KiB ela morre, sob 8 MiB ela
    /// passa. Sem os dois lados, este teste passaria com o `set_memory_limit`
    /// inteiramente apagado — que foi como o guarda de travessia de caminho do
    /// plano anterior passou sem guardar nada.
    #[test]
    fn o_teto_de_memoria_e_aplicado() {
        const ALOCA_UM_MIB: &str =
            "globalThis.aoAcontecer = () => { globalThis.g = new Array(262144).fill(7); };";

        let mut apertado = Anfitriao::com_tetos(256 * 1024, 10_000_000).expect("apertado");
        apertado
            .carregar("seele/x", ALOCA_UM_MIB)
            .expect("carregar");
        assert!(
            apertado.chamar("seele/x", "PersonJoined", "{}").is_err(),
            "1 MiB coube num teto de 256 KiB"
        );

        let mut folgado = Anfitriao::com_tetos(8 * 1024 * 1024, 10_000_000).expect("folgado");
        folgado.carregar("seele/x", ALOCA_UM_MIB).expect("carregar");
        folgado
            .chamar("seele/x", "PersonJoined", "{}")
            .expect("1 MiB não coube num teto de 8 MiB");
    }

    /// O teto de tempo é **aplicado**, e os números saem da medida.
    ///
    /// Um laço de 10 milhões de operações custa ≈ 2 000 consultas aqui. Sob um
    /// teto de 100 ele é cortado; sob 10 000 ele passa. Sem os dois lados este
    /// teste passaria com o `set_interrupt_handler` apagado.
    #[test]
    fn o_teto_de_tempo_e_aplicado() {
        const LACO_LONGO: &str =
            "globalThis.aoAcontecer = () => { let s = 0; for (let i = 0; i < 1e7; i++) s += i; };";

        let mut apertado = Anfitriao::com_tetos(64 * 1024 * 1024, 100).expect("apertado");
        apertado.carregar("seele/y", LACO_LONGO).expect("carregar");
        assert_eq!(
            apertado.chamar("seele/y", "PersonJoined", "{}"),
            Err(Falha::PassouDoTempo),
            "um laço de dez milhões de operações passou por um teto de 100 consultas"
        );

        let mut folgado = Anfitriao::com_tetos(64 * 1024 * 1024, 10_000).expect("folgado");
        folgado.carregar("seele/y", LACO_LONGO).expect("carregar");
        folgado
            .chamar("seele/y", "PersonJoined", "{}")
            .expect("o mesmo laço não coube em 10 000 consultas");
    }

    /// Um laço infinito num MOD não pode ser a diferença entre a sala funcionar
    /// e não funcionar. É o orçamento de 1 vCPU que cobra isto.
    #[test]
    fn um_laco_infinito_e_interrompido() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar(
                "seele/eterno",
                "globalThis.aoAcontecer = () => { while (true) {} };",
            )
            .expect("carregar");

        let inicio = std::time::Instant::now();
        let falha = anfitriao.chamar("seele/eterno", "PersonJoined", "{}");
        assert!(matches!(
            falha,
            Err(Falha::PassouDoTempo) | Err(Falha::Lancou)
        ));
        assert!(
            inicio.elapsed() < std::time::Duration::from_secs(5),
            "o laço infinito segurou a sala por {:?}",
            inicio.elapsed()
        );
    }

    /// Dois MODs não se enxergam. Um que escreve em `globalThis` não aparece no
    /// outro — senão dois MODs de autores diferentes brigariam por um nome.
    #[test]
    fn dois_mods_nao_dividem_o_mesmo_global() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar(
                "seele/um",
                "globalThis.marca = 1; globalThis.aoAcontecer = () => {};",
            )
            .expect("um");
        anfitriao
            .carregar(
                "seele/dois",
                "globalThis.aoAcontecer = () => { if (globalThis.marca) throw new Error('vazou'); };",
            )
            .expect("dois");
        anfitriao
            .chamar("seele/dois", "PersonJoined", "{}")
            .expect("o global de um MOD vazou para o outro");
    }

    /// Um MOD com erro de sintaxe é recusado ao carregar, e não na primeira vez
    /// que alguém entra na sala.
    #[test]
    fn um_mod_que_nao_compila_e_recusado_ao_carregar() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        assert!(matches!(
            anfitriao.carregar("seele/quebrado", "isto ( não é javascript"),
            Err(Falha::NaoCarregou)
        ));
    }

    /// Um MOD que não declara `aoAcontecer` não é um erro: é um MOD que só tem
    /// metade de cliente e cujo `servidor/` existe para outra coisa.
    #[test]
    fn um_mod_sem_ao_acontecer_nao_e_falha() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar("seele/mudo", "globalThis.nada = 1;")
            .expect("carregar");
        anfitriao
            .chamar("seele/mudo", "PersonJoined", "{}")
            .expect("um MOD calado virou falha");
    }
}
