//! A fronteira com o JavaScript: o único arquivo deste crate com `unsafe`.
//!
//! Emenda de 08/10/2026 ao ADR 0057, no molde do `seele-video` (ADR 0041) e do
//! `seele-instalador` (ADR 0043). O `.wasm` não usa `wasm-bindgen`: são seis
//! funções que falam por ponteiro e tamanho, e a casca as chama com uma cola de
//! poucas linhas. A memória é de quem a pede: o JavaScript pede um bloco com
//! [`alocar`], escreve nele, chama, lê a [saída](saida_ptr) e devolve o bloco com
//! [`liberar`].
//!
//! # O contrato
//!
//! | função | devolve | na saída |
//! |---|---|---|
//! | `codificar(carimbo, json)` | 0 | os bytes do quadro |
//! | | 1 | `{"erro": "json" \| "fio", "texto": …}` |
//! | `decodificar(quadro)` | 0 | `{"mensagem": …}` ou `{"desconhecida": N}` |
//! | | 1 | `{"erro": …, "texto": …, "versao": N}`, com `versao` quando o carimbo é de outra versão |
//!
//! Uma chamada sobrescreve a saída da anterior, e o `.wasm` é de uma thread só.

#![allow(unsafe_code)]
#![deny(clippy::undocumented_unsafe_blocks)]

use std::cell::RefCell;

use seele_proto::control::ControlError;

use crate::{codificar_do_cliente, decodificar_do_servidor, DoServidor, ErroDaPonte};

thread_local! {
    static SAIDA: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn dizer(bytes: Vec<u8>, codigo: u32) -> u32 {
    SAIDA.with(|s| *s.borrow_mut() = bytes);
    codigo
}

fn erro(e: &ErroDaPonte) -> u32 {
    let corpo = match e {
        ErroDaPonte::Json(texto) => serde_json::json!({ "erro": "json", "texto": texto }),
        ErroDaPonte::Fio(ControlError::UnsupportedVersion { found, .. }) => {
            serde_json::json!({ "erro": "fio", "texto": format!("{e:?}"), "versao": found })
        }
        ErroDaPonte::Fio(_) => serde_json::json!({ "erro": "fio", "texto": format!("{e:?}") }),
    };
    dizer(corpo.to_string().into_bytes(), 1)
}

/// Os bytes que o JavaScript escreveu num bloco de [`alocar`].
///
/// # Safety
///
/// `ptr` e `len` têm de ser um bloco devolvido por [`alocar`] e ainda não
/// liberado, com `len` bytes escritos pelo JavaScript.
unsafe fn entrada<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if len == 0 {
        return &[];
    }
    // SAFETY: o contrato desta função: `ptr` é o começo de um bloco vivo de
    // pelo menos `len` bytes, alocado como `Vec<u8>` por `alocar`, e ninguém
    // mais o escreve durante a chamada (o `.wasm` tem uma thread só).
    unsafe { std::slice::from_raw_parts(ptr, len) }
}

/// Um bloco de `n` bytes para o JavaScript escrever.
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
    // foi esquecido sem mudar de capacidade; refazer o `Vec` com comprimento 0
    // e a mesma capacidade o devolve ao alocador sem ler o conteúdo.
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

/// [`codificar_do_cliente`] pela fronteira.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn codificar(carimbo: u32, ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let bytes = unsafe { entrada(ptr, len) };
    let Ok(json) = std::str::from_utf8(bytes) else {
        return erro(&ErroDaPonte::Json("o texto não é UTF-8".into()));
    };
    let Ok(carimbo) = u8::try_from(carimbo) else {
        return erro(&ErroDaPonte::Json(format!(
            "carimbo {carimbo} não cabe num byte"
        )));
    };
    match codificar_do_cliente(carimbo, json) {
        Ok(quadro) => dizer(quadro, 0),
        Err(e) => erro(&e),
    }
}

/// [`decodificar_do_servidor`] pela fronteira.
///
/// # Safety
///
/// `ptr` e `len` como em [`entrada`].
#[no_mangle]
pub unsafe extern "C" fn decodificar(ptr: *const u8, len: usize) -> u32 {
    // SAFETY: repassa o contrato desta função ao de `entrada`.
    let quadro = unsafe { entrada(ptr, len) };
    match decodificar_do_servidor(quadro) {
        // O JSON da mensagem já é JSON: entra no envelope sem ser relido.
        Ok(DoServidor::Mensagem(json)) => dizer(format!("{{\"mensagem\":{json}}}").into_bytes(), 0),
        Ok(DoServidor::Desconhecida { ordinal }) => {
            dizer(format!("{{\"desconhecida\":{ordinal}}}").into_bytes(), 0)
        }
        Err(e) => erro(&e),
    }
}

/// As duas funções só de teste: o `cargo xtask web` confere com elas os
/// bytes-ouro dentro do `.wasm`. O `.wasm` da página sai sem a feature.
#[cfg(feature = "prova")]
pub mod prova {
    use seele_proto::control::{decode, encode_em, ClientMessage, ServerMessage};

    use super::{dizer, entrada, erro};
    use crate::ErroDaPonte;

    /// O JSON de uma mensagem do cliente, a partir do quadro.
    ///
    /// # Safety
    ///
    /// `ptr` e `len` como em [`entrada`].
    #[no_mangle]
    pub unsafe extern "C" fn json_do_cliente(ptr: *const u8, len: usize) -> u32 {
        // SAFETY: repassa o contrato desta função ao de `entrada`.
        let quadro = unsafe { entrada(ptr, len) };
        match decode::<ClientMessage>(quadro) {
            Ok(m) => match serde_json::to_string(&m) {
                Ok(json) => dizer(json.into_bytes(), 0),
                Err(e) => erro(&ErroDaPonte::Json(e.to_string())),
            },
            Err(e) => erro(&ErroDaPonte::Fio(e)),
        }
    }

    /// O quadro de uma mensagem do servidor, a partir do JSON.
    ///
    /// # Safety
    ///
    /// `ptr` e `len` como em [`entrada`].
    #[no_mangle]
    pub unsafe extern "C" fn quadro_do_servidor(carimbo: u32, ptr: *const u8, len: usize) -> u32 {
        // SAFETY: repassa o contrato desta função ao de `entrada`.
        let bytes = unsafe { entrada(ptr, len) };
        let lida = std::str::from_utf8(bytes)
            .map_err(|e| ErroDaPonte::Json(e.to_string()))
            .and_then(|t| {
                serde_json::from_str::<ServerMessage>(t)
                    .map_err(|e| ErroDaPonte::Json(e.to_string()))
            });
        match lida {
            Ok(m) => match encode_em(u8::try_from(carimbo).unwrap_or(0), &m) {
                Ok(q) => dizer(q, 0),
                Err(e) => erro(&ErroDaPonte::Fio(e)),
            },
            Err(e) => erro(&e),
        }
    }
}
