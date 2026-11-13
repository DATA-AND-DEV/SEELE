//! O `seeled.exe` do Windows leva o runtime do Visual C++ dentro dele.
//!
//! Só isto, e só no alvo `msvc`: ver `empacotar/crt_estatico.rs`, que o
//! instalador também inclui. Fora do Windows este `build.rs` não faz nada.

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../empacotar/crt_estatico.rs"
));

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../empacotar/crt_estatico.rs");
    crt_estatico();
}
