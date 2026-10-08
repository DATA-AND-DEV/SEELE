//! A fronteira com o Worker: o único arquivo deste crate com `unsafe`, e só no
//! `wasm`.
//!
//! Emenda de 08/10/2026 ao ADR 0057, no molde do `seele-video` (ADR 0041) e do
//! `seele-instalador` (ADR 0043).
//!
//! **Para dentro**, três funções do módulo `seele`, que o Worker fornece:
//! `postar`, `console` e `agora_ms`. Elas e a WASI mínima (relógio, aleatório,
//! ambiente vazio, escrita de registro) são tudo o que o `.wasm` importa, e
//! portanto tudo o que o código de um MOD alcança, mesmo com um defeito de
//! memória no motor: um `.wasm` só chama o que foi importado.
//!
//! **Para fora**, as funções que o Worker chama. A memória é de quem a pede: o
//! Worker pede um bloco com [`alocar`], escreve nele, chama e devolve com
//! [`liberar`]. Uma chamada que falha deixa o texto do defeito na
//! [saída](saida_ptr) e devolve 1; a que dá certo devolve 0.

#![allow(unsafe_code)]
#![deny(clippy::undocumented_unsafe_blocks)]

use std::cell::RefCell;
use std::rc::Rc;

use crate::{Anfitriao, Motor};

#[link(wasm_import_module = "seele")]
extern "C" {
    fn postar(ptr: *const u8, len: usize) -> u32;
    fn console(metodo_ptr: *const u8, metodo_len: usize, texto_ptr: *const u8, texto_len: usize);
    fn agora_ms() -> f64;
}

/// O anfitrião do `.wasm`: as três importações.
struct Worker;

impl Anfitriao for Worker {
    fn postar(&self, json: &str) -> bool {
        // SAFETY: `postar` é a importação do Worker, que só lê `len` bytes a
        // partir de `ptr` durante a chamada; o `&str` vive até ela voltar.
        unsafe { postar(json.as_ptr(), json.len()) == 1 }
    }
    fn console(&self, metodo: &str, texto: &str) {
        // SAFETY: como em `postar`, com dois trechos vivos durante a chamada.
        unsafe { console(metodo.as_ptr(), metodo.len(), texto.as_ptr(), texto.len()) }
    }
    fn agora_ms(&self) -> f64 {
        // SAFETY: a importação não recebe ponteiro nem guarda estado nosso.
        unsafe { agora_ms() }
    }
}

thread_local! {
    static MOTOR: RefCell<Option<Motor>> = const { RefCell::new(None) };
    static SAIDA: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn dizer(texto: impl Into<Vec<u8>>, codigo: u32) -> u32 {
    SAIDA.with(|s| *s.borrow_mut() = texto.into());
    codigo
}

fn resultado(r: Result<(), String>) -> u32 {
    match r {
        Ok(()) => dizer(Vec::new(), 0),
        Err(e) => dizer(e, 1),
    }
}

/// O texto que o Worker escreveu num bloco de [`alocar`].
///
/// # Safety
///
/// `ptr` e `len` têm de ser um bloco de [`alocar`] ainda não liberado, com
/// `len` bytes escritos.
unsafe fn texto<'a>(ptr: *const u8, len: usize) -> &'a str {
    if len == 0 {
        return "";
    }
    // SAFETY: o contrato desta função: um bloco vivo de pelo menos `len` bytes,
    // alocado como `Vec<u8>` por `alocar`, que ninguém escreve durante a
    // chamada (o `.wasm` tem uma thread só).
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    std::str::from_utf8(bytes).unwrap_or("")
}

/// Um bloco de `n` bytes para o Worker escrever.
#[no_mangle]
pub extern "C" fn alocar(n: usize) -> *mut u8 {
    let mut bloco = Vec::<u8>::with_capacity(n.max(1));
    let ptr = bloco.as_mut_ptr();
    std::mem::forget(bloco);
    ptr
}

/// Devolve um bloco de [`alocar`].
///
/// # Safety
///
/// `ptr` e `n` têm de ser os de um [`alocar`] ainda não liberado.
#[no_mangle]
pub unsafe extern "C" fn liberar(ptr: *mut u8, n: usize) {
    // SAFETY: `ptr` veio de um `Vec::with_capacity(n.max(1))` em `alocar`, que
    // foi esquecido sem mudar de capacidade; refazê-lo com comprimento 0 e a
    // mesma capacidade o devolve ao alocador sem ler o conteúdo.
    drop(unsafe { Vec::from_raw_parts(ptr, 0, n.max(1)) });
}

/// Onde está a saída da última chamada.
#[no_mangle]
pub extern "C" fn saida_ptr() -> *const u8 {
    SAIDA.with(|s| s.borrow().as_ptr())
}

/// Quantos bytes tem a saída da última chamada.
#[no_mangle]
pub extern "C" fn saida_len() -> usize {
    SAIDA.with(|s| s.borrow().len())
}

/// Sobe o motor com as capacidades (separadas por vírgula) e o código do MOD.
///
/// # Safety
///
/// Os dois pares `ptr`/`len` como em [`texto`].
#[no_mangle]
pub unsafe extern "C" fn iniciar(
    caps_ptr: *const u8,
    caps_len: usize,
    fonte_ptr: *const u8,
    fonte_len: usize,
) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `texto`.
    let capacidades: Vec<String> = unsafe { texto(caps_ptr, caps_len) }
        .split(',')
        .filter(|c| !c.is_empty())
        .map(str::to_owned)
        .collect();
    // SAFETY: idem.
    let fonte = unsafe { texto(fonte_ptr, fonte_len) };
    match Motor::subir(&capacidades, fonte, Rc::new(Worker)) {
        Ok(motor) => {
            MOTOR.with(|m| *m.borrow_mut() = Some(motor));
            dizer(Vec::new(), 0)
        }
        Err(e) => dizer(e, 1),
    }
}

fn com_o_motor(f: impl FnOnce(&Motor) -> Result<(), String>) -> u32 {
    resultado(MOTOR.with(|m| {
        m.borrow()
            .as_ref()
            .map_or_else(|| Err("o motor não subiu".into()), f)
    }))
}

/// Entrega uma mensagem da janela ao MOD.
///
/// # Safety
///
/// `ptr` e `len` como em [`texto`].
#[no_mangle]
pub unsafe extern "C" fn entregar(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `texto`.
    let json = unsafe { texto(ptr, len) };
    com_o_motor(|m| m.entregar(json))
}

/// Dispara o temporizador `id`.
#[no_mangle]
pub extern "C" fn relogio(id: u32) -> u32 {
    com_o_motor(|m| m.relogio(id))
}

/// Os pedidos de relógio da última volta, em JSON, na saída. Devolve o tamanho.
#[no_mangle]
pub extern "C" fn pedidos_de_relogio() -> u32 {
    let json = MOTOR.with(|m| {
        m.borrow()
            .as_ref()
            .map_or_else(|| "[]".to_owned(), Motor::pedidos_de_relogio)
    });
    let n = u32::try_from(json.len()).unwrap_or(u32::MAX);
    dizer(json, n)
}

/// Quanto o motor alocou, em bytes.
#[no_mangle]
pub extern "C" fn memoria_do_motor() -> f64 {
    #[allow(
        clippy::cast_precision_loss,
        reason = "um número para o diagnóstico, e não uma conta"
    )]
    MOTOR.with(|m| {
        m.borrow()
            .as_ref()
            .map_or(0.0, |motor| motor.memoria() as f64)
    })
}
