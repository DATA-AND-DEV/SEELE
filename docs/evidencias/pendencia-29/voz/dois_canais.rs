use std::io::Write;
#[test]
fn passa() {
    eprintln!("CANAL_MACRO_EPRINTLN");
    let _ = writeln!(std::io::stderr(), "CANAL_STDERR_DIRETO");
    let _ = std::io::stderr().flush();
}
