//! O motor, sem saber onde roda.
//!
//! É a volta do executor do desktop (`apps/seele-app/src/executor.rs`): avaliar,
//! escoar os jobs, recolher os pedidos de relógio. O que muda é o canal para
//! fora: no desktop é um `mpsc`; aqui é um [`Anfitriao`], que no `.wasm` são as
//! três funções que o Worker importa (`fronteira.rs`) e nos testes é um
//! anotador.

use std::cell::Cell;
use std::rc::Rc;

use rquickjs::{Context, Ctx, Function, Runtime};

/// O prelúdio do desktop, extraído do `executor.rs` pelo `build.rs`.
pub const PRELUDIO: &str = include_str!(concat!(env!("OUT_DIR"), "/preludio.js"));

/// Os limites do desktop (`executor.rs`: `TETO_DE_MEMORIA`, `TETO_DA_MENSAGEM` e
/// `Limites::default`). O rascunho da API 6 os quer iguais nos dois motores.
const TETO_DE_MEMORIA: usize = 8 * 1024 * 1024;
const TETO_DA_MENSAGEM: u32 = 12 * 1024;
const PRAZO_POR_VOLTA_MS: f64 = 500.0;
const CONSULTAS_POR_VOLTA: usize = 200_000;

/// O que o motor alcança fora de si. É tudo o que o código de um MOD alcança.
pub trait Anfitriao {
    /// Põe uma mensagem do MOD na fila da janela; `false` se não coube.
    fn postar(&self, json: &str) -> bool;
    /// Uma linha do `console` do MOD.
    fn console(&self, metodo: &str, texto: &str);
    /// O relógio, em milissegundos, para o prazo da volta.
    fn agora_ms(&self) -> f64;
}

/// O relógio e as consultas da volta corrente, que o interruptor lê.
struct Volta {
    comeco: Cell<f64>,
    consultas: Cell<usize>,
}

/// Uma instância do motor, com um MOD dentro.
pub struct Motor {
    // O contexto cai antes do runtime.
    contexto: Context,
    runtime: Runtime,
    volta: Rc<Volta>,
    anfitriao: Rc<dyn Anfitriao>,
}

fn texto_do_erro(ctx: &Ctx<'_>, erro: rquickjs::Error) -> String {
    if matches!(erro, rquickjs::Error::Exception) {
        let valor = ctx.catch();
        if let Some(excecao) = valor.as_exception() {
            return format!("{excecao}");
        }
        return format!("{valor:?}");
    }
    format!("{erro}")
}

impl Motor {
    /// Sobe o motor, a ponte, as capacidades, o prelúdio e o código do MOD.
    ///
    /// # Errors
    ///
    /// O texto do que não subiu: o motor, o contexto, o prelúdio, ou o MOD que
    /// lançou ao ser avaliado.
    pub fn subir(
        capacidades: &[String],
        fonte: &str,
        anfitriao: Rc<dyn Anfitriao>,
    ) -> Result<Self, String> {
        // **Não `Runtime::new()`**: no `wasm32-wasip1`, o teto abaixo só vale com
        // o `RustAllocator` (ver o `Cargo.toml`). O guarda está no
        // `cargo xtask web`, dentro do `.wasm`.
        let runtime = Runtime::new_with_alloc(rquickjs::allocator::RustAllocator)
            .map_err(|e| format!("o motor não subiu: {e}"))?;
        runtime.set_memory_limit(TETO_DE_MEMORIA);
        let volta = Rc::new(Volta {
            comeco: Cell::new(0.0),
            consultas: Cell::new(0),
        });
        {
            // O `deve_parar` do desktop (`executor.rs`), sem a revogação: aqui
            // quem revoga é a página, que encerra o Worker inteiro.
            //
            // Com `Rc`, e não `Arc`: o `rquickjs` sem a feature `parallel` não
            // pede `Send`. Se um dia algum crate do workspace a ligar, a
            // unificação de features faz este trecho parar de compilar, e é aí
            // que se decide.
            let volta = Rc::clone(&volta);
            let relogio = Rc::clone(&anfitriao);
            runtime.set_interrupt_handler(Some(Box::new(move || {
                let n = volta.consultas.get().saturating_add(1);
                volta.consultas.set(n);
                if n > CONSULTAS_POR_VOLTA {
                    return true;
                }
                if !n.is_multiple_of(1024) {
                    return false;
                }
                relogio.agora_ms() - volta.comeco.get() > PRAZO_POR_VOLTA_MS
            })));
        }
        let contexto = Context::full(&runtime).map_err(|e| format!("o contexto não subiu: {e}"))?;
        let motor = Self {
            contexto,
            runtime,
            volta,
            anfitriao,
        };
        motor.comecar();
        let montou = motor.contexto.with(|ctx| {
            motor
                .montar(&ctx, capacidades, fonte)
                .map_err(|e| texto_do_erro(&ctx, e))
        });
        montou?;
        motor.escoar()?;
        Ok(motor)
    }

    fn montar(&self, ctx: &Ctx<'_>, capacidades: &[String], fonte: &str) -> rquickjs::Result<()> {
        let seele = rquickjs::Object::new(ctx.clone())?;
        let saida = Rc::clone(&self.anfitriao);
        seele.set(
            "postar",
            Function::new(ctx.clone(), move |json: String| -> bool {
                // O primeiro teto do desktop: o de uma mensagem. O do acumulado
                // é da fila de quem recebe (a página).
                if json.len() > TETO_DA_MENSAGEM as usize {
                    return false;
                }
                saida.postar(&json)
            })?,
        )?;
        let linhas = Rc::clone(&self.anfitriao);
        seele.set(
            "console",
            Function::new(ctx.clone(), move |metodo: String, texto: String| {
                linhas.console(&metodo, &texto);
            })?,
        )?;
        ctx.globals().set("seele", seele)?;
        let lista = rquickjs::Array::new(ctx.clone())?;
        for (onde, nome) in capacidades.iter().enumerate() {
            lista.set(onde, nome.as_str())?;
        }
        ctx.globals().set("__seeleCapacidades", lista)?;
        ctx.globals()
            .set("__seeleTetoDaMensagem", TETO_DA_MENSAGEM)?;
        ctx.eval::<(), _>(PRELUDIO.as_bytes())?;
        ctx.eval::<(), _>(fonte.as_bytes())
    }

    /// Começa uma volta: zera o relógio e as consultas.
    fn comecar(&self) {
        self.volta.comeco.set(self.anfitriao.agora_ms());
        self.volta.consultas.set(0);
    }

    /// Escoa os jobs (as promessas) e devolve o primeiro erro.
    fn escoar(&self) -> Result<(), String> {
        for _ in 0..10_000 {
            match self.runtime.execute_pending_job() {
                Ok(true) => {}
                Ok(false) => return Ok(()),
                Err(e) => return Err(format!("um job lançou: {e:?}")),
            }
        }
        Err("jobs demais numa volta".into())
    }

    /// Uma volta com uma função global do prelúdio.
    fn volta_com(
        &self,
        nome: &str,
        chamar: impl FnOnce(&Function<'_>) -> rquickjs::Result<()>,
    ) -> Result<(), String> {
        self.comecar();
        self.contexto.with(|ctx| {
            let Ok(f) = ctx.globals().get::<_, Function<'_>>(nome) else {
                return Ok(());
            };
            chamar(&f).map_err(|e| texto_do_erro(&ctx, e))
        })?;
        self.escoar()
    }

    /// Entrega uma mensagem da janela ao MOD (`aoResponder` do prelúdio).
    ///
    /// # Errors
    ///
    /// O que o MOD lançou, ou `interrupted` quando a volta passou do prazo.
    pub fn entregar(&self, json: &str) -> Result<(), String> {
        self.volta_com("aoResponder", |f| f.call::<_, ()>((json,)))
    }

    /// Dispara o temporizador `id` (`__seeleRelogio` do prelúdio).
    ///
    /// # Errors
    ///
    /// Como [`Motor::entregar`].
    pub fn relogio(&self, id: u32) -> Result<(), String> {
        self.volta_com("__seeleRelogio", |f| f.call::<_, ()>((id,)))
    }

    /// Os pedidos de relógio desde a última vez, em JSON (`[]` se nenhum).
    #[must_use]
    pub fn pedidos_de_relogio(&self) -> String {
        self.contexto.with(|ctx| {
            ctx.globals()
                .get::<_, Function<'_>>("__seelePedidosDeRelogio")
                .and_then(|f| f.call::<_, String>(()))
                .unwrap_or_else(|_| "[]".into())
        })
    }

    /// Quanto o motor alocou, em bytes.
    #[must_use]
    pub fn memoria(&self) -> i64 {
        self.runtime.memory_usage().malloc_size
    }
}
