//! **O `seeled` encerra limpo quando recebe um sinal.**
//!
//! O defeito: o binário não ouvia sinal nenhum. Um `systemctl stop`, um
//! `docker stop` ou um Ctrl+C matavam o processo no meio: ninguém era avisado
//! de que o servidor ia cair, e o processo saía pelo sinal, e não por conta
//! própria. Este teste roda o binário de verdade, que é o único lugar onde a
//! escuta de sinal mora.

#![cfg(unix)]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::io::{BufRead as _, BufReader, Read as _};
use std::os::unix::process::ExitStatusExt as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn encerra_limpo_com(sinal: &str) {
    let pasta = tempfile::tempdir().expect("tempdir");
    let mut seeled = Command::new(env!("CARGO_BIN_EXE_seeled"))
        .arg("127.0.0.1:0")
        .env("SEELE_DB", pasta.path().join("seele.db"))
        // Um teste nunca fala com o ponto de encontro de produção.
        .env("SEELE_ENCONTRO", "127.0.0.1:9")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("subir o seeled");
    let mut saida = BufReader::new(seeled.stdout.take().expect("stdout"));

    // Espera ele dizer que está atendendo: antes disso não há o que encerrar.
    let mut linha = String::new();
    loop {
        linha.clear();
        assert!(
            saida.read_line(&mut linha).expect("ler") > 0,
            "o seeled saiu antes de atender"
        );
        if linha.starts_with("seeled listening on") {
            break;
        }
    }

    let matou = Command::new("kill")
        .args([&format!("-{sinal}"), &seeled.id().to_string()])
        .status()
        .expect("kill");
    assert!(matou.success(), "não consegui mandar o {sinal}");

    let desde = Instant::now();
    let estado = loop {
        if let Some(estado) = seeled.try_wait().expect("esperar") {
            break estado;
        }
        if desde.elapsed() > Duration::from_secs(15) {
            let _ = seeled.kill();
            panic!("o seeled não encerrou em 15 s depois do {sinal}");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let mut resto = String::new();
    saida.read_to_string(&mut resto).expect("ler o resto");

    assert_eq!(
        estado.signal(),
        None,
        "o seeled foi morto pelo {sinal} em vez de encerrar por conta própria"
    );
    assert!(estado.success(), "o seeled saiu com {estado:?}");
    assert!(
        resto.contains("seeled: encerrado"),
        "o seeled não disse que encerrou:\n{resto}"
    );
    // O banco fechou por conta própria: o SQLite apaga o `-wal` quando a
    // última conexão fecha limpa, e o deixa para trás quando o processo morre.
    assert!(
        !pasta.path().join("seele.db-wal").exists(),
        "o banco não foi fechado: o `-wal` ficou para trás"
    );
}

#[test]
fn o_seeled_encerra_limpo_no_sigterm() {
    encerra_limpo_com("TERM");
}

#[test]
fn o_seeled_encerra_limpo_no_sigint() {
    encerra_limpo_com("INT");
}
