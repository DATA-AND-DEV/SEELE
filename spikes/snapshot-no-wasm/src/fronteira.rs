//! A fronteira com a página, sem `wasm-bindgen`: a página chama `montar` uma vez
//! e `retratar` quantas vezes quiser, e lê o JSON de `saida_ptr`/`saida_len`.

use std::cell::RefCell;

thread_local! {
    static SALA: RefCell<Option<crate::state::Room>> = const { RefCell::new(None) };
    static SAIDA: RefCell<String> = const { RefCell::new(String::new()) };
}

#[unsafe(no_mangle)]
pub extern "C" fn montar(pessoas: u32, salas: u32, canais: u32) {
    let room = crate::sala::sala_cheia(u64::from(pessoas), salas.max(1), canais);
    SALA.with(|s| *s.borrow_mut() = Some(room));
}

/// Monta o retrato e o deixa na saída; devolve o tamanho em bytes.
#[unsafe(no_mangle)]
pub extern "C" fn retratar() -> u32 {
    let json = SALA.with(|s| s.borrow().as_ref().map(crate::retrato::em_json).unwrap_or_default());
    let tamanho = u32::try_from(json.len()).unwrap_or(u32::MAX);
    SAIDA.with(|s| *s.borrow_mut() = json);
    tamanho
}

#[unsafe(no_mangle)]
pub extern "C" fn saida_ptr() -> *const u8 {
    SAIDA.with(|s| s.borrow().as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn saida_len() -> u32 {
    SAIDA.with(|s| u32::try_from(s.borrow().len()).unwrap_or(u32::MAX))
}

/// O hash de um mesmo valor por um `RandomState` novo. Se duas instâncias do
/// módulo derem o mesmo número, as chaves do `HashMap` são fixas neste alvo.
#[unsafe(no_mangle)]
pub extern "C" fn hash_de_prova() -> u32 {
    use std::hash::BuildHasher;
    let h = std::collections::hash_map::RandomState::new().hash_one(42_u64);
    (h & 0xffff_ffff) as u32
}
