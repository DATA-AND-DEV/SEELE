//! O lado nativo: imprime em hex o mesmo buffer que o `.wasm` devolve.

fn main() {
    match spike_proto_em_wasm::vetores() {
        Ok(b) => println!("{}", b.iter().map(|x| format!("{x:02x}")).collect::<String>()),
        Err(e) => {
            eprintln!("DEFEITO: {e}");
            std::process::exit(1);
        }
    }
}
