//! O motor de MODs do celular (ADR 0057, D-web-1).
//!
//! **O mesmo motor e o mesmo prelúdio do desktop**: o `rquickjs` 0.12.2, o
//! texto de `PRELUDIO` extraído de `apps/seele-app/src/executor.rs` no build, e
//! os limites de lá (8 MiB, 500 ms e 200 000 consultas por volta). Um MOD vê a
//! mesma API nos dois.
//!
//! **Onde roda:** um `.wasm` do `wasm32-wasip1`, num Worker que a página cria
//! de dentro de um `iframe` opaco: a forma AB do spike `mods-no-navegador`, que
//! a orquestradora aceitou em 08/10/2026. O código do MOD só alcança as três
//! funções do [`Anfitriao`] e a WASI mínima que o Worker dá.
//!
//! **Onde mora o `unsafe`:** só em `fronteira.rs`, que existe só no `wasm`. O
//! [`Motor`] é seguro e roda no nativo, onde os testes o exercitam.

mod motor;

pub use motor::{Anfitriao, Motor, PRELUDIO};

#[cfg(target_family = "wasm")]
pub mod fronteira;
