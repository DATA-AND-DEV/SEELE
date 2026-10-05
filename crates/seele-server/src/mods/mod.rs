//! Where a MOD's server half runs.
//!
//! ADR 0045. A MOD is third-party code with real access, by decision — the
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

pub mod anuncio;
pub mod arquivos;
pub mod despacho;
pub mod mundo;
pub mod pedidos;
pub mod volume;

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

/// **O mais que o texto de um MOD ocupa na linha de quem hospeda**, contado
/// pelo tamanho que ele tem lá, já escapado.
///
/// O texto e o onde de [`Falha::Lancou`] e de [`Falha::NaoCarregou`] vêm do
/// MOD e vão em `%falha` e `%error` ao `seele.log` de quem hospeda. O `Debug`
/// os escapa, e o escape faz um caractere que não se imprime ocupar até dez
/// (`\u{100000}`): cortado antes do escape, um texto de 512 desses chegaria a
/// 5120 caracteres na linha. O erro do motor de [`Falha::FalhouSemLancar`] e
/// de [`Falha::NaoCarregouSemLancar`] passa pelo mesmo corte.
///
/// O número é o da janela (`TETO_DA_FRASE_NO_REGISTRO`, no app), e a regra do
/// corte é a de `ate_o_teto_do_registro` do executor, reescrita aqui porque o
/// servidor não depende do app.
const TETO_DO_LANCADO_NO_REGISTRO: usize = 512;

/// Why a MOD did not run.
///
/// **There is no `EstourouMemoria`, and the absence is deliberate.** The memory
/// ceiling works — `o_teto_de_memoria_e_aplicado` proves it — but QuickJS
/// reports it by throwing *inside the script*, so from out here it is
/// indistinguishable from a MOD that threw for any other reason. A variant that
/// is never emitted is the debt ADR 0021 already recorded once
/// (`DisconnectReason::RateLimited`, "existe e nunca é enviado"), and one is
/// enough.
///
/// **O que o MOD lançou vai junto, e escapado.** O `Display` de
/// [`Self::Lancou`] e de [`Self::NaoCarregou`] escreve o texto pelo `Debug`
/// (`mod threw: "ReferenceError: console is not defined" at "…"`), porque ele
/// é do MOD e vai cru ao `seele.log` de quem hospeda: sem as aspas e o
/// escape, um `\n` nele escreveria uma linha forjada. E cortado em
/// [`TETO_DO_LANCADO_NO_REGISTRO`] pelo tamanho escapado. Quando nada foi
/// lançado, o erro do motor vai do mesmo jeito, em [`Self::FalhouSemLancar`]
/// e [`Self::NaoCarregouSemLancar`] (`mod failed: "…"`). Não atravessa o fio:
/// quem pediu recebe `bridge-refused`, como antes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Falha {
    /// O código não carregou: não compilou, ou o topo dele lançou.
    ///
    /// O texto do QuickJS e o onde costumam separar os dois: o que não
    /// compilou sai como `SyntaxError: …`, com o onde no arquivo
    /// (`eval_script:1:1`), e um `throw` do topo sai com o onde dentro do
    /// `<eval>` (`<eval> (eval_script:1:10)`). Mas não é regra: um topo que
    /// compila e chama `JSON.parse('{')` lança um `SyntaxError` também, com o
    /// onde no texto que o JSON leu (`<input>:1:1`).
    #[error("mod threw while loading: {texto:?}{}", onde_dito(.onde.as_deref()))]
    NaoCarregou {
        /// `nome: mensagem` quando é um `Error`; o valor como o JavaScript o
        /// diria, quando não é.
        texto: String,
        /// A primeira linha da pilha, sem o `at` do começo, quando há.
        onde: Option<String>,
    },
    /// **Não carregou, e nada foi lançado**: o motor falhou ao montar o
    /// contexto ou ao rodar o topo, sem exceção do MOD por trás — um teto de
    /// memória menor que o próprio contexto, por exemplo.
    ///
    /// Distinta de [`Self::NaoCarregou`] porque «threw» mandaria quem escreveu
    /// o MOD procurar um `throw` que não existe.
    #[error("mod failed while loading: {texto:?}")]
    NaoCarregouSemLancar {
        /// O erro do motor.
        texto: String,
    },
    /// Lançou, numa chamada.
    #[error("mod threw: {texto:?}{}", onde_dito(.onde.as_deref()))]
    Lancou {
        /// `nome: mensagem` quando é um `Error`; o valor como o JavaScript o
        /// diria, quando não é.
        texto: String,
        /// A primeira linha da pilha, sem o `at` do começo, quando há.
        onde: Option<String>,
    },
    /// **Falhou numa chamada, e nada foi lançado**: o motor recusou uma
    /// conversão, sem exceção do MOD por trás — um `aoPedir` que falta, um
    /// que devolve outra coisa que texto, um valor que não é texto em `dados`.
    ///
    /// Distinta de [`Self::Lancou`] pela mesma razão de
    /// [`Self::NaoCarregouSemLancar`]. O caminho de eventos a trata como trata
    /// quem lançou: desliga o MOD.
    #[error("mod failed: {texto:?}")]
    FalhouSemLancar {
        /// O erro do motor.
        texto: String,
    },
    /// **Não está carregado aqui**, e isso não é defeito dele.
    ///
    /// O conjunto exigido vive no banco; o código carregado vive na thread dos
    /// MODs. Entre uma escrita e a recarga há um instante, e um evento que caia
    /// nele encontra um MOD exigido e ainda sem código.
    ///
    /// Distinta de [`Self::Lancou`] porque o caminho de eventos **desabilita**
    /// quem falha, e desabilitar por isto era o produto desligar sozinho o MOD
    /// que alguém acabara de ligar — sem ninguém pedir, e sem dizer por quê.
    #[error("mod is not loaded here")]
    NaoCarregadoAqui,
    /// It went past the step ceiling.
    #[error("mod went past its step ceiling")]
    PassouDoTempo,
    /// Its yard went past the size ceiling.
    ///
    /// Unlike the memory one, this refusal **is** distinguishable from a MOD
    /// that threw: the ceiling is ours and we count it, so telling the host
    /// which MOD filled its yard costs nothing and answers the question they
    /// will actually ask.
    #[error("mod yard went past its size ceiling")]
    QuintalCheio,
}

/// O ` at "…"` de uma falha, quando há onde.
fn onde_dito(onde: Option<&str>) -> String {
    onde.map(|onde| format!(" at {onde:?}")).unwrap_or_default()
}

/// **O que saiu de uma volta do MOD**, já cortado no teto da linha.
///
/// O molde é o `Lancado` do executor da janela (`apps/seele-app/src/executor.rs`),
/// copiado e não importado: o servidor não depende do app.
struct Lancado {
    /// `nome: mensagem`, o valor lançado como texto, ou o erro do motor.
    texto: String,
    /// A primeira linha da pilha, sem o `at` do começo.
    onde: Option<String>,
    /// Nada foi lançado: o texto é o erro do motor, e a falha não pode
    /// dizer «threw».
    do_motor: bool,
}

impl Lancado {
    /// O que fez `erro` acontecer, **lido ainda dentro do contexto**: a
    /// exceção pendente é dele, e é aqui que ela é tirada.
    ///
    /// Quando o erro não é uma exceção (uma conversão que o motor recusou),
    /// o texto é o do motor.
    fn da_volta(ctx: &rquickjs::Ctx<'_>, erro: &rquickjs::Error) -> Self {
        if !matches!(erro, rquickjs::Error::Exception) {
            return Self::do_motor(erro);
        }
        let (texto, onde) = o_que_o_mod_lancou(ctx);
        Self::no_teto(&texto, onde.as_deref(), false)
    }

    /// Um erro do motor, sem exceção do MOD por trás.
    fn do_motor(erro: &rquickjs::Error) -> Self {
        Self::no_teto(&erro.to_string(), None, true)
    }

    /// O texto e o onde cortados para caberem **juntos** em
    /// [`TETO_DO_LANCADO_NO_REGISTRO`], pelo tamanho escapado.
    ///
    /// O texto vem primeiro, e o onde fica com o que sobrar. Um texto que
    /// precisou ser cortado ocupou a linha inteira, e o onde sai: o pedaço
    /// que sobraria dele (`"<an…"`) não diria onde.
    fn no_teto(texto: &str, onde: Option<&str>, do_motor: bool) -> Self {
        let inteiro = tamanho_escapado(texto) <= TETO_DO_LANCADO_NO_REGISTRO;
        let texto = cortado_no_teto(texto, TETO_DO_LANCADO_NO_REGISTRO);
        let sobra = TETO_DO_LANCADO_NO_REGISTRO.saturating_sub(tamanho_escapado(&texto));
        let onde = onde
            .filter(|_| inteiro)
            .map(|onde| cortado_no_teto(onde, sobra))
            .filter(|onde| !onde.is_empty() && onde != MARCA_DO_CORTE);
        Self {
            texto,
            onde,
            do_motor,
        }
    }

    /// Como falha de uma chamada: [`Falha::Lancou`], ou
    /// [`Falha::FalhouSemLancar`] quando nada foi lançado.
    fn na_chamada(self) -> Falha {
        if self.do_motor {
            Falha::FalhouSemLancar { texto: self.texto }
        } else {
            Falha::Lancou {
                texto: self.texto,
                onde: self.onde,
            }
        }
    }

    /// Como falha ao carregar: [`Falha::NaoCarregou`], ou
    /// [`Falha::NaoCarregouSemLancar`] quando nada foi lançado.
    fn ao_carregar(self) -> Falha {
        if self.do_motor {
            Falha::NaoCarregouSemLancar { texto: self.texto }
        } else {
            Falha::NaoCarregou {
                texto: self.texto,
                onde: self.onde,
            }
        }
    }
}

/// **Por que uma volta do MOD falhou**, lido ainda dentro do contexto.
///
/// O teto é conferido **antes** da leitura: ler o `name` ou a `stack` do que
/// foi lançado pode rodar um getter do MOD, que conta consultas também, e um
/// MOD que lançou não pode virar um que passou do tempo por causa de quem o
/// leu. A leitura roda sob o mesmo teto da chamada, sem zerá-lo: um getter que
/// não termina é cortado por ele.
///
/// Quando quem parou a volta foi o teto, a exceção (`InternalError:
/// interrupted`) é tirada e a falha diz o teto, e não «o MOD lançou» — que
/// mandaria procurar um `throw` que não existe.
fn falha_da_volta(
    ctx: &rquickjs::Ctx<'_>,
    passos: &AtomicUsize,
    teto: usize,
    erro: &rquickjs::Error,
    como: fn(Lancado) -> Falha,
) -> Falha {
    if passos.load(Ordering::Relaxed) > teto {
        let _ = ctx.catch();
        return Falha::PassouDoTempo;
    }
    como(Lancado::da_volta(ctx, erro))
}

/// **O que o MOD lançou, em texto**: `nome: mensagem` e a primeira linha da
/// pilha, quando é um `Error`; o valor como o JavaScript o diria, quando não é.
///
/// O que não vira texto não derruba o resto: um nome que não vira texto sai
/// como `Error`, uma mensagem ou uma pilha sai vazia, e um valor lançado que
/// não vira texto (um `Symbol`) sai como um recuo que diz isso. A exceção que
/// essa leitura deixar pendente é tirada — a volta seguinte não pode herdar
/// uma exceção que não é dela.
fn o_que_o_mod_lancou(ctx: &rquickjs::Ctx<'_>) -> (String, Option<String>) {
    use rquickjs::convert::Coerced;
    let lancado = ctx.catch();
    let lido = match lancado.as_exception() {
        Some(excecao) => {
            let nome = excecao
                .get::<_, Coerced<String>>("name")
                .map_or_else(|_| "Error".to_owned(), |nome| nome.0);
            let mensagem = excecao.message().unwrap_or_default();
            let pilha = excecao.stack().unwrap_or_default();
            let onde = pilha
                .lines()
                .map(str::trim)
                .find(|linha| !linha.is_empty())
                .map(|linha| linha.strip_prefix("at ").unwrap_or(linha).to_owned());
            (format!("{nome}: {mensagem}"), onde)
        }
        None => (
            lancado.get::<Coerced<String>>().map_or_else(
                |_| "[valor que não virou texto]".to_owned(),
                |texto| texto.0,
            ),
            None,
        ),
    };
    if ctx.has_exception() {
        let _ = ctx.catch();
    }
    lido
}

/// O que fecha um texto cortado, para quem lê saber que ele continuava.
const MARCA_DO_CORTE: &str = "…";

/// `texto` inteiro, quando cabe em `teto` pelo tamanho escapado; o começo que
/// cabe e a [`MARCA_DO_CORTE`], quando não cabe.
fn cortado_no_teto(texto: &str, teto: usize) -> String {
    if tamanho_escapado(texto) <= teto {
        return texto.to_owned();
    }
    let Some(sem_a_marca) = teto.checked_sub(tamanho_escapado(MARCA_DO_CORTE)) else {
        return String::new();
    };
    let mut cortado = ate_o_teto(texto, sem_a_marca).to_owned();
    cortado.push_str(MARCA_DO_CORTE);
    cortado
}

/// **O começo de `texto` que cabe em `teto`**, contado pelo tamanho que ele
/// tem escapado ([`tamanho_no_registro`]).
///
/// A regra de `ate_o_teto_do_registro` do executor da janela, reescrita aqui.
/// Caractere a caractere, e não por bytes: parar antes do caractere que não
/// cabe respeita a fronteira dele por construção. E para no primeiro que não
/// cabe, sem medir o resto.
fn ate_o_teto(texto: &str, teto: usize) -> &str {
    let mut ocupado = 0_usize;
    for (onde, c) in texto.char_indices() {
        ocupado = ocupado.saturating_add(tamanho_no_registro(c));
        if ocupado > teto {
            return texto.get(..onde).unwrap_or_default();
        }
    }
    texto
}

/// Quantos caracteres `texto` ocupa no `Debug` de um `str`, sem as aspas.
fn tamanho_escapado(texto: &str) -> usize {
    texto.chars().fold(0_usize, |soma, c| {
        soma.saturating_add(tamanho_no_registro(c))
    })
}

/// Quantos caracteres `c` ocupa no `Debug` de um `str`.
///
/// É o `escape_debug` do caractere, com uma exceção, a mesma do executor da
/// janela: o `Debug` de um `char` escapa o apóstrofo, e o de um `str` não.
fn tamanho_no_registro(c: char) -> usize {
    if c == '\'' {
        1
    } else {
        c.escape_debug().count()
    }
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
    /// As autorizações de escrita por fluxo de volume — ADR 0048.
    ///
    /// Compartilhada porque quem a registra e quem a consome estão em lados
    /// opostos: o MOD registra de dentro do QuickJS, e quem confere é o
    /// tratador do fluxo que chega, que não tem nada a ver com esta árvore.
    esperas: Arc<std::sync::Mutex<volume::Esperas>>,
    /// Quem está sendo atendido agora, posto pelo servidor.
    ///
    /// **É o que impede um MOD de prender um token à pessoa errada.** Se
    /// `volume.esperar` recebesse a pessoa como argumento, um MOD com defeito
    /// registraria em nome de quem não pediu nada — e um mal-intencionado
    /// também. Ela vive aqui, no lado Rust, onde o MOD não alcança.
    ///
    /// Um valor só, e não um por MOD, porque `pedir` toma `&mut self`: há um
    /// pedido em curso por vez, e este campo vale exatamente enquanto ele dura.
    atendendo: Arc<std::sync::Mutex<Option<seele_proto::ids::PersonId>>>,
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
            esperas: Arc::new(std::sync::Mutex::new(volume::Esperas::default())),
            atendendo: Arc::new(std::sync::Mutex::new(None)),
        })
    }

    /// Compiles a MOD's server half into a runtime of its own.
    ///
    /// Loading the same identifier twice replaces it, which is what makes a MOD
    /// updatable without restarting the server.
    ///
    /// # Errors
    ///
    /// [`Falha::NaoCarregou`] when the source does not compile or its top
    /// level throws, with what it threw; [`Falha::PassouDoTempo`] when its top
    /// level goes past the step ceiling. [`Falha::NaoCarregouSemLancar`]
    /// quando o motor falha sem que nada tenha sido lançado (um contexto que
    /// não cabe no teto de memória).
    pub fn carregar(
        &mut self,
        id: &str,
        fonte: &str,
        pasta_de_dados: &std::path::Path,
    ) -> Result<(), Falha> {
        let runtime = Runtime::new().map_err(|erro| Lancado::do_motor(&erro).ao_carregar())?;
        runtime.set_memory_limit(self.teto_de_memoria);

        let passos = Arc::new(AtomicUsize::new(0));
        let contador = Arc::clone(&passos);
        let teto = self.teto_de_consultas;
        runtime.set_interrupt_handler(Some(Box::new(move || {
            contador.fetch_add(1, Ordering::Relaxed) > teto
        })));

        let contexto =
            Context::full(&runtime).map_err(|erro| Lancado::do_motor(&erro).ao_carregar())?;

        // A falha é lida **dentro** do `with`: a exceção pendente é do MOD, e
        // fora dele ela já não tem de onde ser lida. Ver `falha_da_volta`.
        contexto.with(|ctx| {
            self.montar(&ctx, id, pasta_de_dados, fonte)
                .map_err(|erro| falha_da_volta(&ctx, &passos, teto, &erro, Lancado::ao_carregar))
        })?;

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

    /// Monta o contexto de um MOD — `arquivos`, `volume` e `mundo` — e roda o
    /// topo do código dele.
    ///
    /// Separado de [`Self::carregar`] para a falha ser lida ainda dentro do
    /// `contexto.with`, onde a exceção pendente existe.
    fn montar(
        &self,
        ctx: &rquickjs::Ctx<'_>,
        id: &str,
        pasta: &std::path::Path,
        fonte: &str,
    ) -> rquickjs::Result<()> {
        // A pasta do MOD, e nada além dela. Ver `arquivos` para por que o disco
        // é a única exceção à liberdade total do ADR 0045.
        let arquivos_js = rquickjs::Object::new(ctx.clone())?;

        let p = pasta.to_path_buf();
        arquivos_js.set(
            "ler",
            Function::new(ctx.clone(), move |caminho: String| {
                arquivos::ler(&p, &caminho)
            })?,
        )?;

        let p = pasta.to_path_buf();
        arquivos_js.set(
            "escrever",
            Function::new(ctx.clone(), move |caminho: String, conteudo: String| {
                arquivos::escrever(&p, &caminho, &conteudo)
            })?,
        )?;

        let p = pasta.to_path_buf();
        arquivos_js.set(
            "listar",
            Function::new(ctx.clone(), move || arquivos::listar(&p))?,
        )?;

        let p = pasta.to_path_buf();
        arquivos_js.set(
            "apagar",
            Function::new(ctx.clone(), move |caminho: String| {
                arquivos::apagar(&p, &caminho)
            })?,
        )?;

        ctx.globals().set("arquivos", arquivos_js)?;

        // O bloco de volume — ADR 0048. Ele **não carrega bytes**: o
        // MOD autoriza, e os bytes vão por um fluxo próprio direto ao
        // disco, sem passar por aqui. É isso que torna 10 MiB possível.
        let volume_js = rquickjs::Object::new(ctx.clone())?;
        let esperas = Arc::clone(&self.esperas);
        let atendendo = Arc::clone(&self.atendendo);
        let quem_autoriza = id.to_owned();
        volume_js.set(
            "esperar",
            Function::new(
                ctx.clone(),
                move |token: String, caminho: String, tipos: Vec<String>, prazo: f64| {
                    // A pessoa vem daqui, e não do argumento. Ver o
                    // campo `atendendo`.
                    let Ok(quem) = atendendo.lock() else {
                        return false;
                    };
                    let Some(pessoa) = *quem else {
                        return false;
                    };
                    let Ok(mut esperas) = esperas.lock() else {
                        return false;
                    };
                    let segundos = if prazo.is_finite() && prazo > 0.0 {
                        prazo
                    } else {
                        0.0
                    };
                    esperas
                        .registrar(
                            volume::PedidoDeEspera {
                                mod_id: quem_autoriza.clone(),
                                pessoa,
                                token,
                                caminho,
                                tipos,
                                prazo: std::time::Duration::from_secs_f64(segundos),
                            },
                            std::time::Instant::now(),
                        )
                        .is_ok()
                },
            )?,
        )?;
        let p = pasta.join(volume::PASTA);
        volume_js.set(
            "tamanho",
            Function::new(ctx.clone(), move |nome: String| {
                arquivos::dentro(&p, &nome)
                    .and_then(|p| std::fs::metadata(p).ok())
                    .filter(|m| m.is_file())
                    .map(|m| m.len() as f64)
            })?,
        )?;
        let p = pasta.join(volume::PASTA);
        volume_js.set(
            "apagar",
            Function::new(ctx.clone(), move |nome: String| arquivos::apagar(&p, &nome))?,
        )?;
        let p = pasta.join(volume::PASTA);
        volume_js.set(
            "servir",
            Function::new(ctx.clone(), move |nome: String| {
                arquivos::dentro(&p, &nome)
                    .filter(|p| p.is_file())
                    .map(|_| nome)
            })?,
        )?;
        ctx.globals().set("volume", volume_js)?;

        // O bloco `world` do `api/v1.json`: rede, relógio e registro.
        // É onde a liberdade total do ADR 0045 mora, e é o que a tela
        // de aceite tem de dizer em voz alta.
        let mundo_js = rquickjs::Object::new(ctx.clone())?;
        mundo_js.set(
            "buscar",
            Function::new(ctx.clone(), |url: String| mundo::buscar(&url))?,
        )?;
        mundo_js.set("agora", Function::new(ctx.clone(), mundo::agora)?)?;
        let quem = id.to_owned();
        mundo_js.set(
            "registrar",
            Function::new(ctx.clone(), move |linha: String| {
                mundo::registrar(&quem, &linha);
            })?,
        )?;
        ctx.globals().set("mundo", mundo_js)?;

        ctx.eval::<(), _>(fonte)
    }

    /// The most a MOD's key→value yard may hold, in bytes.
    ///
    /// Counted over keys and values together, and refused on write rather than
    /// trimmed: a yard that silently drops the oldest entry is a yard whose MOD
    /// cannot tell whether it saved anything. The ADR 0027 attachment ceiling
    /// evicts because a file that arrives late is still a file; a counter that
    /// silently stops counting is a defect.
    pub const TETO_DO_QUINTAL: usize = 256 * 1024;

    /// Hands a MOD one moment.
    ///
    /// A MOD with no `aoAcontecer` is not a failure: it is a MOD whose server
    /// half exists for something else, and calling it is a no-op.
    ///
    /// # Errors
    ///
    /// [`Falha`] for a MOD that threw or went past a ceiling, and
    /// [`Falha::NaoCarregadoAqui`] for one that is not loaded here.
    /// [`Falha::FalhouSemLancar`] quando o motor recusou uma conversão sem que
    /// o MOD lançasse nada (um valor que não é texto em `dados`).
    pub fn chamar(
        &mut self,
        id: &str,
        momento: &str,
        carga: &str,
        quintal: &mut BTreeMap<String, String>,
    ) -> Result<(), Falha> {
        let hospede = self.hospedes.get(id).ok_or(Falha::NaoCarregadoAqui)?;
        // Each call gets the whole budget: a MOD that was slow once is not a
        // MOD that is broken forever.
        hospede.passos.store(0, Ordering::Relaxed);

        let resultado = hospede.contexto.with(|ctx| {
            // The yard goes in as a plain object and comes back as one.
            //
            // Loading and storing around the call, rather than host functions
            // that reach the database, is what keeps this module free of
            // persistence — `check_deps` would allow the dependency, and the
            // testability would not survive it. It also makes a call
            // transactional by construction: a MOD that throws half-way leaves
            // the yard as it was.
            //
            // Toda falha é lida aqui dentro, com o que o MOD lançou: ver
            // `falha_da_volta`.
            let falhou = |erro: rquickjs::Error| {
                falha_da_volta(
                    &ctx,
                    &hospede.passos,
                    self.teto_de_consultas,
                    &erro,
                    Lancado::na_chamada,
                )
            };
            let dados = rquickjs::Object::new(ctx.clone()).map_err(falhou)?;
            for (chave, valor) in quintal.iter() {
                dados.set(chave.as_str(), valor.as_str()).map_err(falhou)?;
            }
            ctx.globals().set("dados", dados).map_err(falhou)?;

            let Ok(f) = ctx.globals().get::<_, Function<'_>>("aoAcontecer") else {
                return Ok(());
            };
            f.call::<_, ()>((momento, carga)).map_err(falhou)
        });

        // Read the yard back **only if the call finished**. A MOD that threw or
        // was cut leaves the yard exactly as it was: half a write is worse than
        // none, and the MOD has no way to know which half landed.
        if resultado.is_ok() {
            self.recolher_quintal(id, quintal)?;
        }
        resultado
    }

    /// Calls API v2's authenticated request handler and commits its yard only on success.
    /// Returns a bounded JSON response, or a runtime failure.
    pub fn pedir(
        &mut self,
        id: &str,
        quem: seele_proto::ids::PersonId,
        contexto: &str,
        pedido: &str,
        quintal: &mut BTreeMap<String, String>,
    ) -> Result<String, Falha> {
        let hospede = self.hospedes.get(id).ok_or(Falha::NaoCarregadoAqui)?;
        hospede.passos.store(0, Ordering::Relaxed);

        // **Quem está sendo atendido, dito pelo servidor.** É contra isto que
        // `volume.esperar` prende a autorização; ver o campo `atendendo`.
        if let Ok(mut atendendo) = self.atendendo.lock() {
            *atendendo = Some(quem);
        }
        let resposta = hospede.contexto.with(|ctx| {
            // Como em `chamar`: a falha é lida aqui dentro, e um `aoPedir`
            // cortado pelo teto diz o teto, e não que lançou.
            let falhou = |erro: rquickjs::Error| {
                falha_da_volta(
                    &ctx,
                    &hospede.passos,
                    self.teto_de_consultas,
                    &erro,
                    Lancado::na_chamada,
                )
            };
            let dados = rquickjs::Object::new(ctx.clone()).map_err(falhou)?;
            for (k, v) in quintal.iter() {
                dados.set(k.as_str(), v.as_str()).map_err(falhou)?;
            }
            ctx.globals().set("dados", dados).map_err(falhou)?;
            let f = ctx
                .globals()
                .get::<_, Function<'_>>("aoPedir")
                .map_err(falhou)?;
            f.call::<_, String>((contexto, pedido)).map_err(falhou)
        });
        // Limpo **antes** de propagar a falha: um MOD que lança não pode deixar
        // a pessoa dele pendurada aqui, onde o pedido seguinte de outro MOD a
        // leria como se fosse a sua.
        if let Ok(mut atendendo) = self.atendendo.lock() {
            *atendendo = None;
        }
        let resposta = resposta?;
        if resposta.len() > 1024 * 1024 {
            return Err(Falha::QuintalCheio);
        }
        self.recolher_quintal(id, quintal)?;
        Ok(resposta)
    }

    /// As autorizações de escrita por fluxo — ADR 0048.
    ///
    /// Pública porque quem as registra e quem as consome estão em lados
    /// opostos: o MOD registra de dentro do QuickJS, e quem confere o
    /// cabeçalho do fluxo que chega é o tratador da conexão.
    #[must_use]
    pub fn esperas(&self) -> Arc<std::sync::Mutex<volume::Esperas>> {
        Arc::clone(&self.esperas)
    }

    /// Usa a lista de esperas de outra pessoa em vez da própria.
    ///
    /// **É isto que faz uma autorização sobreviver ao pedido que a emitiu.**
    /// `mods::pedidos` cria um `Anfitriao` por pedido e o descarta ao
    /// responder; uma espera registrada dentro do `aoPedir` morreria junto, e o
    /// fluxo que chegasse logo depois leria «não há espera com este token»
    /// para um token que o MOD tinha acabado de emitir.
    ///
    /// A lista de verdade mora no [`crate::server::Server`], que é o objeto que
    /// o QuickJS e o tratador do fluxo alcançam.
    ///
    /// Chamado **antes** de `carregar`: as ligações do QuickJS clonam o `Arc`
    /// no momento em que são montadas, e trocá-lo depois deixaria um MOD já
    /// carregado escrevendo na lista descartada.
    pub fn compartilhar_esperas(&mut self, esperas: Arc<std::sync::Mutex<volume::Esperas>>) {
        self.esperas = esperas;
    }

    /// Copies the JS `dados` object back into the map, refusing an oversized
    /// yard rather than trimming it.
    fn recolher_quintal(
        &self,
        id: &str,
        quintal: &mut BTreeMap<String, String>,
    ) -> Result<(), Falha> {
        let Some(hospede) = self.hospedes.get(id) else {
            return Ok(());
        };
        hospede.contexto.with(|ctx| {
            let Ok(dados) = ctx.globals().get::<_, rquickjs::Object<'_>>("dados") else {
                return Ok(());
            };
            let mut novo = BTreeMap::new();
            let mut bytes = 0_usize;
            for par in dados.props::<String, String>() {
                // Ler o quintal pode rodar um getter do MOD: a falha é lida
                // como a de uma chamada, sob o mesmo teto.
                let (chave, valor) = par.map_err(|erro| {
                    falha_da_volta(
                        &ctx,
                        &hospede.passos,
                        self.teto_de_consultas,
                        &erro,
                        Lancado::na_chamada,
                    )
                })?;
                bytes = bytes
                    .saturating_add(chave.len())
                    .saturating_add(valor.len());
                if bytes > Self::TETO_DO_QUINTAL {
                    return Err(Falha::QuintalCheio);
                }
                novo.insert(chave, valor);
            }
            *quintal = novo;
            Ok(())
        })
    }
}

/// Reads the server halves of the MODs this server has on.
///
/// Returns `(id, source, data folder)` per MOD, skipping the ones with no
/// server half — a MOD that is only appearance has nothing to run here, and
/// that is not a failure.
///
/// A MOD whose manifest is unreadable is skipped **and named** in the returned
/// list of complaints. Skipping in silence is the failure this repository pays
/// for most: whoever enabled it gets no answer.
#[must_use]
pub fn carregar_do_disco(
    raizes: &crate::RaizesDosMods,
    exigidos: &[(String, String)],
) -> (Vec<(String, String, std::path::PathBuf)>, Vec<String>) {
    let mut prontos = Vec::new();
    let mut queixas = Vec::new();

    // **Por hash, e é o hash que o banco guardou.** Carregar por identificador
    // era carregar «o que estiver naquela pasta», e o que estivesse lá podia
    // não ser o que este servidor exige — outro servidor desta máquina podia
    // tê-lo atualizado. O hash é o que a exigência diz, e é contra ele que o
    // `pedidos.rs` já confere a cada pedido; aqui a resolução passa a contar a
    // mesma história.
    for (id, hash) in exigidos {
        let dir = raizes.pacote_de(hash);
        let Ok(texto) = std::fs::read_to_string(dir.join("mod.json")) else {
            queixas.push(format!("{id}: sem mod.json em {}", dir.display()));
            continue;
        };
        let manifesto = match seele_proto::mods::read_manifest(&texto) {
            Ok(manifesto) => manifesto,
            Err(recusa) => {
                queixas.push(format!("{id}: {recusa}"));
                continue;
            }
        };
        let Some(metade) = manifesto.server else {
            continue;
        };
        let Ok(fonte) = std::fs::read_to_string(dir.join(&metade)) else {
            queixas.push(format!(
                "{id}: o manifesto aponta para {metade}, que não abre"
            ));
            continue;
        };
        // A pasta mutável é da **instância**, e não sai de dentro do pacote.
        prontos.push((id.clone(), fonte, raizes.dados_de(id)));
    }
    (prontos, queixas)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Uma pasta de dados de mentira, por teste.
    /// Uma pasta de dados de mentira, **única por chamada**.
    ///
    /// O contador não é zelo. Sem ele, dois testes que passem o mesmo nome
    /// dividem o diretório, e como `cargo test` roda em paralelo um apaga o do
    /// outro no meio da corrida. Foi o que aconteceu: testes **diferentes**
    /// falhando a cada execução, que é assinatura de colisão e não de lógica —
    /// e nenhum deles falhava sozinho.
    ///
    /// Único por construção, e não por todo chamador lembrar de um nome
    /// diferente: a versão que dependia disso é a que quebrou.
    fn pasta_de_teste(nome: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static QUAL: AtomicUsize = AtomicUsize::new(0);

        let n = QUAL.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("seele-anfitriao-{nome}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temporário");
        dir
    }

    /// **A pessoa de uma espera vem do servidor, e não do MOD** — ADR 0048.
    ///
    /// `volume.esperar` não recebe quem é. Se recebesse, um MOD com defeito
    /// registraria a autorização em nome de quem não pediu nada, e um MOD
    /// mal-intencionado faria isso de propósito: bastaria escrever o
    /// identificador de outra pessoa e esperar que ela — ou quem tivesse o
    /// token — escrevesse na pasta por ela.
    ///
    /// Este teste atende **duas pessoas diferentes** com o mesmo MOD e o mesmo
    /// código, e confere que cada espera saiu presa a quem estava sendo
    /// atendido naquele momento.
    #[test]
    fn a_espera_fica_presa_a_quem_o_servidor_disse_estar_atendendo() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar(
                "seele/perfis",
                // O MOD tenta prender a espera a quem ele quiser: ele escreve
                // o token e o caminho, e nada mais. Não há argumento de pessoa
                // para ele preencher — que é a propriedade sob teste.
                "globalThis.aoPedir = (c, r) => { \
                   const pedido = JSON.parse(r); \
                   const ok = volume.esperar(pedido.token, 'volume/x.bin', ['png'], 60); \
                   return JSON.stringify({ok}); \
                 };",
                &pasta_de_teste("espera-por-pessoa"),
            )
            .expect("carregar");

        let esperas = anfitriao.esperas();
        let mut quintal = BTreeMap::new();

        for (pessoa, token) in [(7_u64, "t-sete"), (9, "t-nove")] {
            let resposta = anfitriao
                .pedir(
                    "seele/perfis",
                    seele_proto::ids::PersonId(pessoa),
                    "{}",
                    &format!(r#"{{"token":"{token}"}}"#),
                    &mut quintal,
                )
                .expect("pedir");
            assert!(resposta.contains("true"), "o MOD não conseguiu registrar");
        }

        let agora = std::time::Instant::now();
        let mut esperas = esperas.lock().expect("esperas");
        assert!(
            esperas
                .tomar(
                    "t-sete",
                    "seele/perfis",
                    seele_proto::ids::PersonId(9),
                    agora
                )
                .is_none(),
            "a espera de uma pessoa foi consumida por outra"
        );
        assert!(
            esperas
                .tomar(
                    "t-sete",
                    "seele/perfis",
                    seele_proto::ids::PersonId(7),
                    agora
                )
                .is_some(),
            "a espera não ficou presa a quem o servidor estava atendendo"
        );
        assert!(esperas
            .tomar(
                "t-nove",
                "seele/perfis",
                seele_proto::ids::PersonId(9),
                agora
            )
            .is_some());
    }

    /// **Carregar um MOD não é atender ninguém.**
    ///
    /// O código de topo de um MOD roda no `carregar`, fora de qualquer pedido.
    /// Se a pessoa do último pedido continuasse de pé, um MOD instalado depois
    /// registraria uma autorização de escrita **em nome de quem foi atendido
    /// por último** — sem que essa pessoa tivesse pedido nada, e sem que o MOD
    /// dela tivesse qualquer relação com ele.
    ///
    /// É por isso que `pedir` limpa ao terminar, inclusive quando o MOD lança.
    ///
    /// **Este teste já nasceu errado uma vez:** a primeira versão carregava o
    /// segundo MOD e nunca o executava, então passava sem afirmar nada. Quem
    /// pegou foi a prova de reversão — tirar a limpeza não o fazia falhar.
    #[test]
    fn carregar_um_mod_nao_registra_em_nome_de_quem_foi_atendido_antes() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar(
                "seele/explode",
                "globalThis.aoPedir = () => { throw new Error('eu'); };",
                &pasta_de_teste("pendurada"),
            )
            .expect("carregar");
        let mut quintal = BTreeMap::new();
        let _ = anfitriao.pedir(
            "seele/explode",
            seele_proto::ids::PersonId(7),
            "{}",
            "{}",
            &mut quintal,
        );

        // O segundo MOD tenta registrar **no topo**, durante o `carregar` — o
        // único momento em que não há pedido em curso.
        anfitriao
            .carregar(
                "seele/tenta",
                "volume.esperar('t','volume/x.bin',['png'],60); \
                 globalThis.aoPedir = () => '{}';",
                &pasta_de_teste("pendurada2"),
            )
            .expect("carregar");

        assert_eq!(
            anfitriao.esperas().lock().expect("esperas").quantas(),
            0,
            "um MOD registrou autorização de escrita no carregamento, em nome \
             da pessoa que o servidor atendeu por último"
        );
    }

    #[test]
    fn um_mod_que_carrega_responde_a_um_momento() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar(
                "seele/exemplo",
                "globalThis.aoAcontecer = (momento) => { globalThis.ultimo = momento; };",
                &pasta_de_teste("t"),
            )
            .expect("carregar");
        anfitriao
            .chamar("seele/exemplo", "PersonJoined", "{}", &mut BTreeMap::new())
            .expect("chamar");
    }

    /// O teto de memória do ADR 0045, e a metade que importa: **a sala
    /// continua**. Um MOD que aloca sem parar é cortado, e o contexto dele
    /// segue respondendo — medido em `spikes/mod-em-js/src/bin/tetos.rs`.
    #[test]
    fn um_mod_que_aloca_sem_parar_e_cortado_e_o_contexto_sobrevive() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar(
                "seele/glutao",
                "globalThis.aoAcontecer = () => { const a = []; for (;;) a.push(new Array(1024)); };",
                &pasta_de_teste("t"),
            )
            .expect("carregar");

        let falha = anfitriao.chamar("seele/glutao", "PersonJoined", "{}", &mut BTreeMap::new());
        // `Lancou` e não uma variante própria: QuickJS reporta o estouro
        // lançando dentro do script, e daqui de fora não há como distinguir.
        // Está escrito na doc de `Falha`, e o teto em si tem prova própria em
        // `o_teto_de_memoria_e_aplicado`.
        assert!(matches!(
            falha,
            Err(Falha::Lancou { .. }) | Err(Falha::PassouDoTempo)
        ));

        // E o contexto ainda responde.
        anfitriao
            .carregar(
                "seele/glutao",
                "globalThis.aoAcontecer = () => {};",
                &pasta_de_teste("t"),
            )
            .expect("recarregar depois do estouro");
        anfitriao
            .chamar("seele/glutao", "PersonJoined", "{}", &mut BTreeMap::new())
            .expect("o contexto morreu junto com o estouro");
    }

    #[test]
    fn o_que_um_mod_guarda_no_quintal_volta() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar(
                "seele/placar",
                "globalThis.aoAcontecer = () => { dados.pontos = '7'; };",
                &pasta_de_teste("t"),
            )
            .expect("carregar");

        let mut quintal = BTreeMap::new();
        anfitriao
            .chamar("seele/placar", "PersonJoined", "{}", &mut quintal)
            .expect("chamar");

        assert_eq!(quintal.get("pontos").map(String::as_str), Some("7"));
    }

    #[test]
    fn o_que_estava_no_quintal_chega_ao_mod() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar(
                "seele/placar",
                "globalThis.aoAcontecer = () => { dados.eco = dados.pontos; };",
                &pasta_de_teste("t"),
            )
            .expect("carregar");

        let mut quintal = BTreeMap::from([("pontos".to_owned(), "7".to_owned())]);
        anfitriao
            .chamar("seele/placar", "PersonJoined", "{}", &mut quintal)
            .expect("chamar");

        assert_eq!(quintal.get("eco").map(String::as_str), Some("7"));
    }

    /// Uma chamada é transacional: meia escrita é pior que nenhuma, porque o
    /// MOD não tem como saber qual metade entrou.
    #[test]
    fn um_mod_que_lanca_no_meio_nao_deixa_meia_escrita() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar(
                "seele/meio",
                "globalThis.aoAcontecer = () => { dados.a = '1'; throw new Error('no meio'); };",
                &pasta_de_teste("t"),
            )
            .expect("carregar");

        let mut quintal = BTreeMap::from([("antes".to_owned(), "ok".to_owned())]);
        let chamada = anfitriao.chamar("seele/meio", "PersonJoined", "{}", &mut quintal);
        assert!(
            matches!(chamada, Err(Falha::Lancou { .. })),
            "um MOD que lança no meio não foi dito como um MOD que lançou: {chamada:?}"
        );

        assert_eq!(quintal.len(), 1, "a escrita de um MOD que lançou entrou");
        assert_eq!(quintal.get("antes").map(String::as_str), Some("ok"));
    }

    /// **A linha de quem hospeda diz o que o MOD lançou** (P51-27).
    ///
    /// A metade de servidor não tem `console`, e o guia diz que `console.log`
    /// lá lança `ReferenceError`. A falha dizia só «mod threw»: o erro mais
    /// comum de quem escreve a metade de servidor não deixava no `seele.log`
    /// nem o nome dele.
    #[test]
    fn um_aopedir_que_chama_console_diz_reference_error() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar(
                "seele/console",
                "globalThis.aoPedir = () => { console.log(1); return '{}'; };",
                &pasta_de_teste("console"),
            )
            .expect("carregar");

        let dito = anfitriao
            .pedir(
                "seele/console",
                seele_proto::ids::PersonId(7),
                "{}",
                "{}",
                &mut BTreeMap::new(),
            )
            .expect_err("um aoPedir que chama console.log respondeu como se nada fosse")
            .to_string();
        assert!(
            dito.contains("ReferenceError"),
            "a falha de um aoPedir que chama console.log não diz o ReferenceError que ele \
             lançou, e quem hospeda lê só que o MOD lançou: {dito}"
        );
    }

    /// **Um `aoPedir` que falta, ou que devolve outra coisa que texto, não
    /// lançou nada**, e a linha de quem hospeda não diz que lançou.
    ///
    /// O motor recusa a conversão (`Error converting from js 'undefined' into
    /// type 'function'`) sem exceção nenhuma do MOD. Dito como «mod threw», o
    /// erro mandava quem escreveu o MOD procurar um `throw` que não existe.
    #[test]
    fn um_aopedir_que_falta_ou_devolve_numero_diz_que_falhou_e_nao_que_lancou() {
        for (caso, fonte) in [
            ("sem aoPedir", "globalThis.nada = 1;"),
            (
                "aoPedir que devolve número",
                "globalThis.aoPedir = () => 1;",
            ),
        ] {
            let mut anfitriao = Anfitriao::novo().expect("anfitrião");
            anfitriao
                .carregar("seele/sem-texto", fonte, &pasta_de_teste("sem-texto"))
                .expect("carregar");

            let dito = anfitriao
                .pedir(
                    "seele/sem-texto",
                    seele_proto::ids::PersonId(7),
                    "{}",
                    "{}",
                    &mut BTreeMap::new(),
                )
                .expect_err("um aoPedir que o motor não converte respondeu como se nada fosse")
                .to_string();
            assert!(
                dito.starts_with("mod failed: \"Error converting from js")
                    && !dito.contains("threw"),
                "{caso}: a falha de um aoPedir que o motor não converteu diz que o MOD lançou, \
                 e manda procurar um throw que não existe: {dito}"
            );
        }
    }

    /// **Um motor que não consegue montar o contexto também não é um MOD que
    /// lançou.**
    ///
    /// Com um teto de memória menor que o próprio contexto, o QuickJS falha ao
    /// criar os objetos dele (`Allocation failed while creating object`), antes
    /// de uma linha do MOD rodar.
    #[test]
    fn um_contexto_que_nao_coube_na_memoria_diz_que_falhou_ao_carregar_e_nao_que_lancou() {
        let mut anfitriao = Anfitriao::com_tetos(16 * 1024, 100).expect("anfitrião");
        let dito = anfitriao
            .carregar(
                "seele/sem-memoria",
                "globalThis.x = 1;",
                &pasta_de_teste("sem-memoria"),
            )
            .expect_err("um contexto montado em 16 KiB foi carregado, e este teste não prova nada")
            .to_string();
        assert!(
            dito.starts_with("mod failed while loading: ") && !dito.contains("threw"),
            "a falha do motor ao montar o contexto de um MOD diz que o MOD lançou ao carregar, \
             antes de uma linha dele rodar: {dito}"
        );
    }

    /// Um `throw` no topo do MOD diz o que foi lançado, e diz que foi ao
    /// carregar.
    #[test]
    fn um_throw_no_topo_diz_o_que_foi_lancado_ao_carregar() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        let dito = anfitriao
            .carregar(
                "seele/topo",
                "throw new Error('topo');",
                &pasta_de_teste("topo"),
            )
            .expect_err("um MOD que lança no topo foi carregado")
            .to_string();
        assert!(
            dito.starts_with("mod threw while loading: ") && dito.contains("topo"),
            "a falha de um MOD que lança no topo não diz que foi ao carregar, ou não traz o \
             texto que ele lançou: {dito}"
        );
    }

    /// **O texto do MOD não quebra a linha de quem hospeda.**
    ///
    /// Ele vai em `%falha` e `%error` ao `seele.log`, e o `%` escreve cru. Sem o
    /// escape, um MOD que lança `"a\nWARN …"` escreveria uma segunda linha com
    /// a cara do produto — a injeção que o P51-26 fecha no id.
    #[test]
    fn o_texto_lancado_com_quebra_de_linha_fica_numa_linha_so() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar(
                "seele/quebra",
                "globalThis.aoAcontecer = () => { \
                   throw new Error('a\\nWARN seele_server: forjada'); \
                 };",
                &pasta_de_teste("quebra"),
            )
            .expect("carregar");

        let dito = anfitriao
            .chamar("seele/quebra", "PersonJoined", "{}", &mut BTreeMap::new())
            .expect_err("um MOD que lança foi chamado como se nada fosse")
            .to_string();
        assert!(
            dito.contains("forjada"),
            "a falha não traz o texto que o MOD lançou: {dito}"
        );
        assert!(
            !dito.contains('\n') && !dito.contains('\r'),
            "o texto que o MOD lançou quebra a linha do seele.log de quem hospeda, e a \
             segunda linha sai com a cara do produto: {dito:?}"
        );
    }

    /// **O texto lançado no topo também não quebra a linha.**
    ///
    /// [`Falha::NaoCarregou`] tem `Display` próprio, e o texto dele vai ao
    /// `seele.log` de quem hospeda por três portas: o «MOD recusado ao subir»
    /// do `lib.rs`, o «MOD recusado ao recarregar» do `despacho.rs` e o
    /// `%error` do carregar de cada pedido, em `pedidos.rs`. O teste de cima
    /// passa só por `chamar`, que dá [`Falha::Lancou`]: sem este, o escape de
    /// `NaoCarregou` voltaria ao texto cru e nada reclamaria.
    #[test]
    fn o_texto_lancado_no_topo_com_quebra_de_linha_fica_numa_linha_so() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        let falha = anfitriao
            .carregar(
                "seele/topo-quebra",
                "throw new Error('a\\nWARN seele_server: forjada');",
                &pasta_de_teste("topo-quebra"),
            )
            .expect_err("um MOD que lança no topo foi carregado");
        assert!(
            matches!(falha, Falha::NaoCarregou { .. }),
            "um MOD que lança no topo não foi dito como um MOD que não carregou, e este teste \
             deixou de guardar o Display de NaoCarregou: {falha:?}"
        );

        let dito = falha.to_string();
        assert!(
            dito.contains("forjada"),
            "a falha ao carregar não traz o texto que o MOD lançou no topo: {dito}"
        );
        assert!(
            !dito.contains('\n') && !dito.contains('\r'),
            "o texto que o MOD lançou no topo quebra a linha do seele.log de quem hospeda, e a \
             segunda linha sai com a cara do produto: {dito:?}"
        );
    }

    /// **O texto do MOD cabe no teto pelo tamanho que ele tem escapado.**
    ///
    /// O escape faz um caractere que não se imprime ocupar até dez
    /// (`\u{100000}`). Cortado antes do escape, um texto de 512 desses
    /// chegaria a 5120 caracteres na linha.
    #[test]
    fn o_texto_lancado_cabe_no_teto_depois_de_escapado() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar(
                "seele/comprido",
                "globalThis.aoAcontecer = () => { \
                   throw new Error('\\u{100000}'.repeat(2048)); \
                 };",
                &pasta_de_teste("comprido"),
            )
            .expect("carregar");

        let dito = anfitriao
            .chamar("seele/comprido", "PersonJoined", "{}", &mut BTreeMap::new())
            .expect_err("um MOD que lança foi chamado como se nada fosse")
            .to_string();
        assert!(
            dito.contains("\\u{100000}"),
            "a falha não traz o texto que o MOD lançou, escapado: {}",
            dito.chars().take(200).collect::<String>()
        );
        // A moldura deste caso, e só ela: o começo de `Lancou` e as duas aspas
        // do texto. O onde não entra, porque um texto cortado o leva embora
        // (a última asserção confere). Uma moldura mais larga deixaria passar
        // um corte que passasse do teto.
        let moldura = "mod threw: ".chars().count() + 2;
        assert!(
            dito.chars().count() <= TETO_DO_LANCADO_NO_REGISTRO + moldura,
            "a falha passou do teto da linha: {} caracteres, para um teto de {} mais a \
             moldura de {moldura}",
            dito.chars().count(),
            TETO_DO_LANCADO_NO_REGISTRO
        );
        assert!(
            dito.ends_with("…\"") && !dito.contains("\" at \""),
            "um texto cortado no teto não termina na marca do corte, ou leva junto o pedaço do \
             onde que sobrou, que não diz onde: {}",
            dito.chars()
                .skip(dito.chars().count().saturating_sub(40))
                .collect::<String>()
        );
    }

    /// Um `aoPedir` cortado pelo teto diz o teto, e não «o MOD lançou».
    ///
    /// O QuickJS faz da interrupção uma exceção (`InternalError:
    /// interrupted`), e dizê-la como lançada mandaria quem lê procurar um
    /// `throw` que não existe. `chamar` já separava os dois; `pedir`, não.
    #[test]
    fn um_aopedir_que_nao_termina_passa_do_tempo_e_nao_lanca() {
        let mut anfitriao = Anfitriao::com_tetos(64 * 1024 * 1024, 100).expect("anfitrião");
        anfitriao
            .carregar(
                "seele/eterno",
                "globalThis.aoPedir = () => { for (;;) {} };",
                &pasta_de_teste("pedido-eterno"),
            )
            .expect("carregar");

        assert_eq!(
            anfitriao.pedir(
                "seele/eterno",
                seele_proto::ids::PersonId(7),
                "{}",
                "{}",
                &mut BTreeMap::new(),
            ),
            Err(Falha::PassouDoTempo),
            "um aoPedir que não termina foi dito como um MOD que lançou"
        );
    }

    /// Um topo que não termina diz o teto ao carregar, e não que lançou.
    #[test]
    fn um_topo_que_nao_termina_passa_do_tempo_ao_carregar() {
        let mut anfitriao = Anfitriao::com_tetos(64 * 1024 * 1024, 100).expect("anfitrião");
        assert_eq!(
            anfitriao.carregar(
                "seele/topo-eterno",
                "for (;;) {}",
                &pasta_de_teste("topo-eterno")
            ),
            Err(Falha::PassouDoTempo),
            "um MOD cujo topo não termina foi dito como um MOD que lançou ao carregar"
        );
    }

    /// **O onde também é do MOD**, e também não quebra a linha.
    ///
    /// A pilha é uma propriedade que o MOD pode reescrever. A primeira linha
    /// dela não tem `\n` — é por ele que a pilha se divide —, mas pode ter um
    /// `\r` ou um U+2028, que um editor quebra do mesmo jeito.
    #[test]
    fn o_onde_que_o_mod_escreve_tambem_fica_numa_linha_so() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar(
                "seele/pilha",
                "globalThis.aoAcontecer = () => { \
                   const e = new Error('eu'); \
                   Object.defineProperty(e, 'stack', \
                     { value: 'at a\\rWARN seele_server: forjada\\u2028outra' }); \
                   throw e; \
                 };",
                &pasta_de_teste("pilha"),
            )
            .expect("carregar");

        let dito = anfitriao
            .chamar("seele/pilha", "PersonJoined", "{}", &mut BTreeMap::new())
            .expect_err("um MOD que lança foi chamado como se nada fosse")
            .to_string();
        assert!(
            dito.contains("forjada"),
            "a falha não traz o onde que o MOD escreveu: {dito}"
        );
        assert!(
            !dito.contains('\r') && !dito.contains('\u{2028}'),
            "o onde que o MOD escreveu quebra a linha do seele.log de quem hospeda: {dito:?}"
        );
    }

    /// **Ler o que o MOD lançou roda sob o teto da chamada.**
    ///
    /// O `name` de um `Error` pode ser um getter do MOD, e lê-lo roda código
    /// dele. Um getter que não termina é cortado pelo mesmo teto, e a falha
    /// continua sendo a de quem lançou, com o texto que deu para ler.
    #[test]
    fn um_getter_que_nao_termina_nao_segura_a_leitura_do_que_foi_lancado() {
        let mut anfitriao = Anfitriao::com_tetos(64 * 1024 * 1024, 100).expect("anfitrião");
        anfitriao
            .carregar(
                "seele/getter",
                "globalThis.aoAcontecer = () => { \
                   const e = new Error('lancei'); \
                   Object.defineProperty(e, 'name', { get() { for (;;) {} } }); \
                   throw e; \
                 };",
                &pasta_de_teste("getter"),
            )
            .expect("carregar");

        let inicio = std::time::Instant::now();
        let dito = anfitriao
            .chamar("seele/getter", "PersonJoined", "{}", &mut BTreeMap::new())
            .expect_err("um MOD que lança foi chamado como se nada fosse")
            .to_string();
        assert!(
            inicio.elapsed() < std::time::Duration::from_secs(5),
            "ler o que o MOD lançou segurou a sala por {:?}",
            inicio.elapsed()
        );
        assert!(
            dito.starts_with("mod threw: ") && dito.contains("lancei"),
            "um getter que não termina tirou da falha o texto que o MOD lançou: {dito}"
        );
    }

    /// O teto do quintal recusa em vez de aparar. Um quintal que descarta a
    /// entrada mais velha em silêncio é um quintal cujo MOD não sabe se gravou.
    #[test]
    fn um_quintal_grande_demais_e_recusado_e_nao_aparado() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar(
                "seele/glutao",
                "globalThis.aoAcontecer = () => { \
                   for (let i = 0; i < 5000; i++) dados['c' + i] = 'x'.repeat(100); \
                 };",
                &pasta_de_teste("t"),
            )
            .expect("carregar");

        let mut quintal = BTreeMap::from([("antes".to_owned(), "ok".to_owned())]);
        assert_eq!(
            anfitriao.chamar("seele/glutao", "PersonJoined", "{}", &mut quintal),
            Err(Falha::QuintalCheio)
        );
        assert_eq!(
            quintal.get("antes").map(String::as_str),
            Some("ok"),
            "o quintal foi aparado em vez de a escrita ser recusada"
        );
    }

    /// De ponta a ponta: um MOD **em JavaScript** escreve na pasta dele.
    ///
    /// Os testes de `arquivos` provam a regra; este prova a **ligação**. Sem
    /// ele, as quatro funções poderiam estar corretas e nunca registradas no
    /// contexto, e nada reclamaria.
    #[test]
    fn um_mod_escreve_um_arquivo_de_dentro_do_javascript() {
        let pasta = pasta_de_teste("js-escreve");
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar(
                "seele/rpg",
                "globalThis.aoAcontecer = () => { \
                   dados.gravou = arquivos.escrever('fichas/coelho.json', '{\"forca\":18}') \
                     ? 'sim' : 'nao'; \
                   dados.leu = arquivos.ler('fichas/coelho.json') ?? 'nada'; \
                 };",
                &pasta,
            )
            .expect("carregar");

        let mut quintal = BTreeMap::new();
        anfitriao
            .chamar("seele/rpg", "PersonJoined", "{}", &mut quintal)
            .expect("chamar");

        assert_eq!(quintal.get("gravou").map(String::as_str), Some("sim"));
        assert_eq!(
            quintal.get("leu").map(String::as_str),
            Some("{\"forca\":18}")
        );
        assert!(pasta.join("fichas/coelho.json").exists());
    }

    /// E o mesmo MOD, tentando sair, recebe `false` em vez da chave de
    /// identidade de quem hospeda.
    #[test]
    fn um_mod_que_tenta_sair_da_pasta_pelo_javascript_recebe_recusa() {
        let raiz = pasta_de_teste("js-fuga");
        let minha = raiz.join("dados");
        std::fs::create_dir_all(&minha).expect("pasta do mod");
        std::fs::write(raiz.join("identity.key"), "SEGREDO").expect("chave de mentira");

        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar(
                "seele/curioso",
                "globalThis.aoAcontecer = () => { \
                   dados.leu = arquivos.ler('../identity.key') ?? 'nada'; \
                   dados.escreveu = arquivos.escrever('../invadi.txt', 'oi') ? 'sim' : 'nao'; \
                 };",
                &minha,
            )
            .expect("carregar");

        let mut quintal = BTreeMap::new();
        anfitriao
            .chamar("seele/curioso", "PersonJoined", "{}", &mut quintal)
            .expect("chamar");

        assert_eq!(
            quintal.get("leu").map(String::as_str),
            Some("nada"),
            "um MOD leu o identity.key de quem hospeda"
        );
        assert_eq!(quintal.get("escreveu").map(String::as_str), Some("nao"));
        assert!(!raiz.join("invadi.txt").exists());
        assert_eq!(
            std::fs::read_to_string(raiz.join("identity.key")).expect("ler"),
            "SEGREDO"
        );
    }

    /// O `mundo` chega ao MOD, e o relógio dele é o nosso.
    #[test]
    fn um_mod_le_o_relogio_e_escreve_no_log() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar(
                "seele/relogio",
                "globalThis.aoAcontecer = () => { \
                   mundo.registrar('oi do MOD'); \
                   dados.quando = String(mundo.agora()); \
                 };",
                &pasta_de_teste("mundo"),
            )
            .expect("carregar");

        let mut quintal = BTreeMap::new();
        anfitriao
            .chamar("seele/relogio", "PersonJoined", "{}", &mut quintal)
            .expect("chamar");

        let quando: i64 = quintal
            .get("quando")
            .and_then(|q| q.parse().ok())
            .unwrap_or(0);
        assert!(
            quando > 1_700_000_000,
            "o MOD leu um relógio de antes de 2023"
        );
    }

    /// E um MOD **não** alcança `file://` pela rede, que seria fazer pelo
    /// `mundo.buscar` exatamente o que a pasta do MOD impede no disco.
    #[test]
    fn um_mod_nao_le_arquivo_pela_rede() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar(
                "seele/curioso",
                "globalThis.aoAcontecer = () => { \
                   dados.leu = mundo.buscar('file:///etc/passwd') ?? 'nada'; \
                 };",
                &pasta_de_teste("rede"),
            )
            .expect("carregar");

        let mut quintal = BTreeMap::new();
        anfitriao
            .chamar("seele/curioso", "PersonJoined", "{}", &mut quintal)
            .expect("chamar");

        assert_eq!(quintal.get("leu").map(String::as_str), Some("nada"));
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
            .carregar("seele/x", ALOCA_UM_MIB, &pasta_de_teste("t"))
            .expect("carregar");
        assert!(
            apertado
                .chamar("seele/x", "PersonJoined", "{}", &mut BTreeMap::new())
                .is_err(),
            "1 MiB coube num teto de 256 KiB"
        );

        let mut folgado = Anfitriao::com_tetos(8 * 1024 * 1024, 10_000_000).expect("folgado");
        folgado
            .carregar("seele/x", ALOCA_UM_MIB, &pasta_de_teste("t"))
            .expect("carregar");
        folgado
            .chamar("seele/x", "PersonJoined", "{}", &mut BTreeMap::new())
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
        apertado
            .carregar("seele/y", LACO_LONGO, &pasta_de_teste("t"))
            .expect("carregar");
        assert_eq!(
            apertado.chamar("seele/y", "PersonJoined", "{}", &mut BTreeMap::new()),
            Err(Falha::PassouDoTempo),
            "um laço de dez milhões de operações passou por um teto de 100 consultas"
        );

        let mut folgado = Anfitriao::com_tetos(64 * 1024 * 1024, 10_000).expect("folgado");
        folgado
            .carregar("seele/y", LACO_LONGO, &pasta_de_teste("t"))
            .expect("carregar");
        folgado
            .chamar("seele/y", "PersonJoined", "{}", &mut BTreeMap::new())
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
                &pasta_de_teste("t"),
            )
            .expect("carregar");

        let inicio = std::time::Instant::now();
        let falha = anfitriao.chamar("seele/eterno", "PersonJoined", "{}", &mut BTreeMap::new());
        assert!(matches!(
            falha,
            Err(Falha::PassouDoTempo) | Err(Falha::Lancou { .. })
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
                &pasta_de_teste("t"),
            )
            .expect("um");
        anfitriao
            .carregar(
                "seele/dois",
                "globalThis.aoAcontecer = () => { if (globalThis.marca) throw new Error('vazou'); };",
                &pasta_de_teste("t"),
            )
            .expect("dois");
        anfitriao
            .chamar("seele/dois", "PersonJoined", "{}", &mut BTreeMap::new())
            .expect("o global de um MOD vazou para o outro");
    }

    /// Um MOD com erro de sintaxe é recusado ao carregar, e não na primeira vez
    /// que alguém entra na sala.
    #[test]
    fn um_mod_que_nao_compila_e_recusado_ao_carregar() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        assert!(matches!(
            anfitriao.carregar(
                "seele/quebrado",
                "isto ( não é javascript",
                &pasta_de_teste("t")
            ),
            Err(Falha::NaoCarregou { .. })
        ));
    }

    /// Um MOD que não declara `aoAcontecer` não é um erro: é um MOD que só tem
    /// metade de cliente e cujo `servidor/` existe para outra coisa.
    #[test]
    fn um_mod_sem_ao_acontecer_nao_e_falha() {
        let mut anfitriao = Anfitriao::novo().expect("anfitrião");
        anfitriao
            .carregar("seele/mudo", "globalThis.nada = 1;", &pasta_de_teste("t"))
            .expect("carregar");
        anfitriao
            .chamar("seele/mudo", "PersonJoined", "{}", &mut BTreeMap::new())
            .expect("um MOD calado virou falha");
    }
}
