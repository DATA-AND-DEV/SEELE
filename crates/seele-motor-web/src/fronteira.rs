//! A fronteira com o Worker: o único arquivo deste crate com `unsafe`, e só no
//! `wasm`.
//!
//! Emenda de 08/10/2026 ao ADR 0057, no molde do `seele-video` (ADR 0041) e do
//! `seele-instalador` (ADR 0043).
//!
//! **Para dentro, nada além da WASI mínima** (relógio, aleatório, ambiente
//! vazio, escrita de registro). É tudo o que o `.wasm` importa, e portanto tudo o
//! que o código de um MOD alcança, mesmo com um defeito de memória no motor: um
//! `.wasm` só chama o que foi importado. A lista é fixa em `xtask/web/motor.mjs`.
//! Até 09/10/2026 havia também três funções do módulo `seele`; elas saíram
//! quando a volta passou a ser a do `seele-motor`, que fala por um canal.
//!
//! **Para fora**, as funções que o Worker chama. A memória é de quem a pede: o
//! Worker pede um bloco com [`alocar`], escreve nele, chama e devolve com
//! [`liberar`]. Cada chamada deixa na [saída](saida_ptr) as falas do motor
//! desde a anterior, numa lista em JSON (`ParaOFora::em_json`), e devolve
//! quantas são.

#![allow(unsafe_code)]
#![deny(clippy::undocumented_unsafe_blocks)]

use std::cell::RefCell;

use crate::Instancia;

thread_local! {
    static INSTANCIA: RefCell<Option<Instancia>> = const { RefCell::new(None) };
    static SAIDA: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// As falas desde a última chamada, na saída; devolve quantas são.
fn dizer_as_falas() -> u32 {
    let (quantas, falas) = INSTANCIA.with(|i| {
        i.borrow()
            .as_ref()
            .map_or_else(|| (0, "[]".to_owned()), Instancia::falas)
    });
    SAIDA.with(|s| *s.borrow_mut() = falas.into_bytes());
    u32::try_from(quantas).unwrap_or(u32::MAX)
}

fn com_a_instancia(f: impl FnOnce(&mut Instancia)) -> u32 {
    INSTANCIA.with(|i| {
        if let Some(instancia) = i.borrow_mut().as_mut() {
            f(instancia);
        }
    });
    dizer_as_falas()
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
    let capacidades: Vec<&str> = unsafe { texto(caps_ptr, caps_len) }
        .split(',')
        .filter(|c| !c.is_empty())
        .collect();
    // SAFETY: idem.
    let fonte = unsafe { texto(fonte_ptr, fonte_len) };
    let instancia = Instancia::subir(&capacidades, fonte);
    INSTANCIA.with(|i| *i.borrow_mut() = Some(instancia));
    dizer_as_falas()
}

/// Entrega uma resposta da janela ao MOD.
///
/// # Safety
///
/// `ptr` e `len` como em [`texto`].
#[no_mangle]
pub unsafe extern "C" fn entregar(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `texto`.
    let json = unsafe { texto(ptr, len) };
    com_a_instancia(|i| i.resposta(json))
}

/// Dispara os temporizadores na hora.
#[no_mangle]
pub extern "C" fn vencer() -> u32 {
    com_a_instancia(Instancia::vencer)
}

/// Pede ao motor que conte o que tem de pé.
#[no_mangle]
pub extern "C" fn diagnostico() -> u32 {
    com_a_instancia(Instancia::diagnostico)
}

/// Em quantos milissegundos chamar [`vencer`], ou `-1`.
#[no_mangle]
pub extern "C" fn proximo_prazo_ms() -> f64 {
    INSTANCIA.with(|i| {
        i.borrow()
            .as_ref()
            .map_or(-1.0, Instancia::proximo_prazo_ms)
    })
}

/// A página atendeu uma mensagem de `bytes` bytes.
#[no_mangle]
pub extern "C" fn lido(bytes: usize) {
    INSTANCIA.with(|i| {
        if let Some(instancia) = i.borrow().as_ref() {
            instancia.lido(bytes);
        }
    });
}

/// A página atendeu um aviso.
#[no_mangle]
pub extern "C" fn aviso_lido() {
    INSTANCIA.with(|i| {
        if let Some(instancia) = i.borrow().as_ref() {
            instancia.aviso_lido();
        }
    });
}

/// Para o motor: as falas que ficaram saem, e a última é o `parou`.
#[no_mangle]
pub extern "C" fn parar() -> u32 {
    com_a_instancia(Instancia::parar)
}

/// Quanto o motor alocou, em bytes.
#[no_mangle]
pub extern "C" fn memoria_do_motor() -> f64 {
    #[allow(
        clippy::cast_precision_loss,
        reason = "um número para o diagnóstico, e não uma conta"
    )]
    INSTANCIA.with(|i| i.borrow().as_ref().map_or(0.0, |m| m.memoria() as f64))
}
