//! A candidata A: o motor do executor de MODs do desktop, em `wasm32-wasip1`.
//!
//! O que é igual ao `apps/seele-app/src/executor.rs`:
//! - o motor: `rquickjs` 0.12.2 (quickjs-ng 0.15.1);
//! - o prelúdio: o mesmo texto, extraído do fonte pelo `build.rs`;
//! - a ponte de duas funções que o prelúdio espera, `seele.postar` e
//!   `seele.console`, mais `__seeleCapacidades` e `__seeleTetoDaMensagem`;
//! - os limites: 8 MiB de memória, 500 ms e 200 000 consultas por volta;
//! - a volta: avaliar, escoar os jobs, recolher os pedidos de relógio.
//!
//! O que muda: o canal para fora. No desktop é um `mpsc`; aqui são três funções
//! importadas do módulo `seele`, que o Worker fornece. **Elas são tudo o que o
//! código do MOD alcança fora do motor**, mesmo que o motor tenha um defeito de
//! memória: um `.wasm` só chama o que foi importado.
//!
//! Descartável. Um MOD por instância do módulo, uma thread.

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use rquickjs::{Context, Ctx, Function, Runtime};

const PRELUDIO: &str = include_str!(concat!(env!("OUT_DIR"), "/preludio.js"));
const TETO_DE_MEMORIA: usize = 8 * 1024 * 1024;
const TETO_DA_MENSAGEM: u32 = 12 * 1024;
const PRAZO_POR_VOLTA_MS: f64 = 500.0;
const CONSULTAS_POR_VOLTA: usize = 200_000;

#[link(wasm_import_module = "seele")]
extern "C" {
    /// Põe uma mensagem do MOD na fila da janela. 1 se coube, 0 se não.
    fn postar(ptr: *const u8, len: usize) -> u32;
    /// Uma linha do `console` do MOD.
    fn console(metodo_ptr: *const u8, metodo_len: usize, texto_ptr: *const u8, texto_len: usize);
    /// O relógio, para o prazo da volta.
    fn agora_ms() -> f64;
}

struct Motor {
    // A ordem importa: o contexto cai antes do runtime.
    contexto: Context,
    runtime: Runtime,
    comeco: Arc<AtomicU64>,
    consultas: Arc<AtomicUsize>,
}

thread_local! {
    static MOTOR: RefCell<Option<Motor>> = const { RefCell::new(None) };
    static SAIDA: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn dizer(texto: impl Into<Vec<u8>>) -> u32 {
    let bytes = texto.into();
    let n = bytes.len() as u32;
    SAIDA.with(|s| *s.borrow_mut() = bytes);
    n
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

/// Começa uma volta: zera o relógio e as consultas.
fn comecar(m: &Motor) {
    m.comeco.store(unsafe { agora_ms() }.to_bits(), Ordering::Relaxed);
    m.consultas.store(0, Ordering::Relaxed);
}

/// Escoa os jobs (as promessas) e devolve o primeiro erro, se houver.
fn escoar(m: &Motor) -> Option<String> {
    for _ in 0..10_000 {
        match m.runtime.execute_pending_job() {
            Ok(true) => continue,
            Ok(false) => return None,
            Err(e) => return Some(format!("um job lançou: {e:?}")),
        }
    }
    Some("jobs demais numa volta".into())
}

/// # Safety
/// Chamado pelo Worker, uma vez, com memória que ele próprio alocou.
#[no_mangle]
pub unsafe extern "C" fn alocar(n: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(n.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `p` e `n` vêm de `alocar`.
#[no_mangle]
pub unsafe extern "C" fn liberar(p: *mut u8, n: usize) {
    drop(Vec::from_raw_parts(p, 0, n.max(1)));
}

#[no_mangle]
pub extern "C" fn saida_ptr() -> *const u8 {
    SAIDA.with(|s| s.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn saida_len() -> usize {
    SAIDA.with(|s| s.borrow().len())
}

unsafe fn texto<'a>(p: *const u8, n: usize) -> &'a str {
    std::str::from_utf8(std::slice::from_raw_parts(p, n)).unwrap_or("")
}

/// Sobe o motor, a ponte, as capacidades, o prelúdio e o código do MOD.
/// Devolve 0 se subiu; senão o texto do defeito fica na saída e devolve 1.
///
/// # Safety
/// Os ponteiros vêm de `alocar`.
#[no_mangle]
pub unsafe extern "C" fn iniciar(caps_ptr: *const u8, caps_len: usize, fonte_ptr: *const u8, fonte_len: usize) -> u32 {
    let capacidades: Vec<String> = texto(caps_ptr, caps_len)
        .split(',')
        .filter(|c| !c.is_empty())
        .map(str::to_owned)
        .collect();
    let fonte = texto(fonte_ptr, fonte_len).to_owned();

    // **Não `Runtime::new()`**: com o alocador padrão, o teto abaixo não vale no
    // `wasi` (ver o Cargo.toml). Medido: 128 MiB alocados com o teto de 8 MiB.
    let Ok(runtime) = Runtime::new_with_alloc(rquickjs::allocator::RustAllocator) else {
        dizer("o motor não subiu");
        return 1;
    };
    runtime.set_memory_limit(TETO_DE_MEMORIA);
    let comeco = Arc::new(AtomicU64::new(0));
    let consultas = Arc::new(AtomicUsize::new(0));
    {
        let comeco = Arc::clone(&comeco);
        let consultas = Arc::clone(&consultas);
        // O `deve_parar` do desktop (`executor.rs:338-362`), sem a revogação:
        // aqui quem revoga é o Worker, que é encerrado inteiro.
        runtime.set_interrupt_handler(Some(Box::new(move || {
            let n = consultas.fetch_add(1, Ordering::Relaxed);
            if n > CONSULTAS_POR_VOLTA {
                return true;
            }
            if n % 1024 != 0 {
                return false;
            }
            let agora = unsafe { agora_ms() };
            agora - f64::from_bits(comeco.load(Ordering::Relaxed)) > PRAZO_POR_VOLTA_MS
        })));
    }
    let Ok(contexto) = Context::full(&runtime) else {
        dizer("o contexto não subiu");
        return 1;
    };
    let m = Motor { contexto, runtime, comeco, consultas };
    comecar(&m);

    let resultado = m.contexto.with(|ctx| -> Result<(), String> {
        let montar = (|| -> rquickjs::Result<()> {
            let seele = rquickjs::Object::new(ctx.clone())?;
            seele.set(
                "postar",
                Function::new(ctx.clone(), |json: String| -> bool {
                    if json.len() > TETO_DA_MENSAGEM as usize {
                        return false;
                    }
                    unsafe { postar(json.as_ptr(), json.len()) == 1 }
                })?,
            )?;
            seele.set(
                "console",
                Function::new(ctx.clone(), |metodo: String, texto: String| unsafe {
                    console(metodo.as_ptr(), metodo.len(), texto.as_ptr(), texto.len());
                })?,
            )?;
            ctx.globals().set("seele", seele)?;
            let lista = rquickjs::Array::new(ctx.clone())?;
            for (i, c) in capacidades.iter().enumerate() {
                lista.set(i, c.as_str())?;
            }
            ctx.globals().set("__seeleCapacidades", lista)?;
            ctx.globals().set("__seeleTetoDaMensagem", TETO_DA_MENSAGEM)?;
            ctx.eval::<(), _>(PRELUDIO.as_bytes())?;
            ctx.eval::<(), _>(fonte.as_bytes())
        })();
        montar.map_err(|e| texto_do_erro(&ctx, e))
    });
    let jobs = escoar(&m);
    MOTOR.with(|g| *g.borrow_mut() = Some(m));
    match (resultado, jobs) {
        (Err(e), _) | (Ok(()), Some(e)) => {
            dizer(e);
            1
        }
        (Ok(()), None) => 0,
    }
}

/// Uma volta com uma função global do prelúdio (`aoResponder` ou
/// `__seeleRelogio`). 0 se correu bem; 1 com o defeito na saída.
fn volta(nome: &str, argumento: VoltaArg) -> u32 {
    MOTOR.with(|g| {
        let g = g.borrow();
        let Some(m) = g.as_ref() else {
            dizer("o motor não subiu");
            return 1;
        };
        comecar(m);
        let r = m.contexto.with(|ctx| {
            let chamar = (|| -> rquickjs::Result<()> {
                let Ok(f) = ctx.globals().get::<_, Function<'_>>(nome) else { return Ok(()) };
                match &argumento {
                    VoltaArg::Texto(t) => f.call::<_, ()>((t.as_str(),)),
                    VoltaArg::Numero(n) => f.call::<_, ()>((*n,)),
                }
            })();
            chamar.map_err(|e| texto_do_erro(&ctx, e))
        });
        let jobs = escoar(m);
        match (r, jobs) {
            (Err(e), _) | (Ok(()), Some(e)) => {
                dizer(e);
                1
            }
            (Ok(()), None) => 0,
        }
    })
}

enum VoltaArg {
    Texto(String),
    Numero(u32),
}

/// # Safety
/// Os ponteiros vêm de `alocar`.
#[no_mangle]
pub unsafe extern "C" fn entregar(json_ptr: *const u8, json_len: usize) -> u32 {
    volta("aoResponder", VoltaArg::Texto(texto(json_ptr, json_len).to_owned()))
}

#[no_mangle]
pub extern "C" fn relogio(id: u32) -> u32 {
    volta("__seeleRelogio", VoltaArg::Numero(id))
}

/// Os pedidos de relógio da última volta, em JSON, na saída. Devolve o tamanho.
#[no_mangle]
pub extern "C" fn pedidos_de_relogio() -> u32 {
    MOTOR.with(|g| {
        let g = g.borrow();
        let Some(m) = g.as_ref() else { return dizer("[]") };
        let texto = m.contexto.with(|ctx| -> String {
            ctx.globals()
                .get::<_, Function<'_>>("__seelePedidosDeRelogio")
                .and_then(|f| f.call::<_, String>(()))
                .unwrap_or_else(|_| "[]".into())
        });
        dizer(texto)
    })
}

/// Quanto o motor alocou, em bytes (o `malloc_size` do QuickJS).
#[no_mangle]
pub extern "C" fn memoria_do_motor() -> f64 {
    MOTOR.with(|g| g.borrow().as_ref().map_or(0.0, |m| m.runtime.memory_usage().malloc_size as f64))
}
