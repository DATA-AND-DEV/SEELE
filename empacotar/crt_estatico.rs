// **O executável do Windows leva o runtime do Visual C++ dentro dele.**
//
// Incluído (`include!`) pelos `build.rs` do instalador e do `seeled`. Sem
// isto os dois importam `VCRUNTIME140.dll`, que não vem com o Windows: numa
// máquina sem o Visual C++ redistribuível o **instalador nem abre**, e a
// pessoa não tem como saber por quê. Medido na 1.0.0 publicada (13/11/2026):
// o instalador e o `seeled.exe` importavam a DLL; o `SEELE.exe` não, porque
// o `tauri build` já faz isto por ele (`tauri-build`, `static_vcruntime`).
//
// É a mesma técnica de lá (<https://github.com/ChrisDenton/static_vcruntime>):
// o runtime do compilador (`vcruntime`) entra estático, e o da linguagem C
// (`ucrt`) continua o do sistema, que todo Windows 10 e 11 tem. Baixar o
// redistribuível no instalador não serviria: é ele que não abre.
//
// Só no alvo `msvc`; fora dele não faz nada.

/// Emite os argumentos de ligação. Chamado do `main` do `build.rs`.
fn crt_estatico() {
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    // O Rust pede `msvcrt.lib` por conta própria (a versão dinâmica). Um
    // arquivo de mesmo nome, vazio, achado antes no caminho de bibliotecas,
    // tira esse pedido sem tocar no compilador.
    let maquina: &[u8] = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86_64") => &[0x64, 0x86],
        Ok("x86") => &[0x4C, 0x01],
        // Noutra arquitetura a biblioteca vazia teria o cabeçalho errado: fica
        // como estava, com a DLL, em vez de um executável que não liga.
        _ => return,
    };
    const VAZIA: &[u8] = &[
        1, 0, 94, 3, 96, 98, 60, 0, 0, 0, 1, 0, 0, 0, 0, 0, 132, 1, 46, 100, 114, 101, 99, 116,
        118, 101, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 60, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 10, 16, 0, 46, 100, 114, 101, 99, 116, 118, 101, 0, 0, 0, 0, 1, 0, 0, 0, 3, 0, 4, 0,
        0, 0,
    ];
    let Ok(saida) = std::env::var("OUT_DIR") else {
        panic!("o cargo não disse o OUT_DIR ao build.rs: o runtime do Visual C++ não foi embutido");
    };
    let mut bytes = maquina.to_vec();
    bytes.extend_from_slice(VAZIA);
    let caminho = std::path::Path::new(&saida).join("msvcrt.lib");
    if let Err(erro) = std::fs::write(&caminho, bytes) {
        panic!(
            "não escrevi {}: {erro}\nSem ela o executável volta a pedir o VCRUNTIME140.dll.",
            caminho.display()
        );
    }
    println!("cargo:rustc-link-search=native={saida}");
    for fora in [
        "libvcruntimed.lib",
        "vcruntime.lib",
        "vcruntimed.lib",
        "libcmtd.lib",
        "msvcrt.lib",
        "msvcrtd.lib",
        "libucrt.lib",
        "libucrtd.lib",
    ] {
        println!("cargo:rustc-link-arg=/NODEFAULTLIB:{fora}");
    }
    for dentro in ["libcmt.lib", "libvcruntime.lib", "ucrt.lib"] {
        println!("cargo:rustc-link-arg=/DEFAULTLIB:{dentro}");
    }
}
