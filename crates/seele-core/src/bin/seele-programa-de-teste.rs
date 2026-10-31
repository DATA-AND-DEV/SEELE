//! O programa de teste do poder `programa` (`tests/programa_processo.rs`).
//!
//! Existe porque os testes de processo rodam no Mac **e no Windows**, e `sh`
//! não existe no segundo: um binário deste crate, em Rust, faz o papel do
//! servidor de jogo nos dois. Não faz nada no produto e não é empacotado.
//!
//! - `eco`: diz `pronto` na saída e `aviso no erro` no erro, e responde a cada
//!   linha da entrada: `save-all` → `salvo`; `sair N` → sai com N;
//!   `muitas N` → N linhas; `longa N` → uma linha de N `x`; outra → `eco: …`.
//! - `argumentos …`: escreve cada argumento numa linha e sai com 0.
//! - `batimento ARQUIVO`: escreve um contador crescente no arquivo a cada
//!   20 ms, para sempre — é como o teste vê que o processo morreu.
//! - `neto ARQUIVO`: sobe um neto (`batimento ARQUIVO`), diz `neto PID` e
//!   espera a entrada fechar — o servidor de jogo que relança o `java`.
//! - `neto-e-sai ARQUIVO`: sobe o mesmo neto, espera ele bater e sai com 0 —
//!   o instalador que deixa um ajudante para trás.

use std::io::{BufRead as _, Write as _};

fn main() {
    let argumentos: Vec<String> = std::env::args().skip(1).collect();
    match argumentos.first().map(String::as_str) {
        Some("eco") => eco(),
        Some("argumentos") => {
            for argumento in argumentos.iter().skip(1) {
                println!("{argumento}");
            }
        }
        Some("batimento") => {
            let Some(arquivo) = argumentos.get(1) else {
                std::process::exit(2);
            };
            let mut n = 0_u64;
            loop {
                n += 1;
                let _ = std::fs::write(arquivo, n.to_string());
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
        Some(modo @ ("neto" | "neto-e-sai")) => {
            let Some(arquivo) = argumentos.get(1) else {
                std::process::exit(2);
            };
            let Ok(eu) = std::env::current_exe() else {
                std::process::exit(3);
            };
            let Ok(neto) = std::process::Command::new(eu)
                .args(["batimento", arquivo])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            else {
                std::process::exit(4);
            };
            println!("neto {}", neto.id());
            let _ = std::io::stdout().flush();
            // Sai só depois de o neto começar a bater: é o neto que fica
            // para trás que o teste procura.
            if modo == "neto-e-sai" {
                while std::fs::read_to_string(arquivo).map_or(true, |t| t.trim().is_empty()) {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
            if modo == "neto" {
                for linha in std::io::stdin().lock().lines() {
                    if linha.is_err() {
                        break;
                    }
                }
            }
        }
        _ => std::process::exit(2),
    }
}

fn eco() {
    let mut saida = std::io::stdout().lock();
    let _ = writeln!(saida, "pronto");
    let _ = saida.flush();
    eprintln!("aviso no erro");
    for linha in std::io::stdin().lock().lines() {
        let Ok(linha) = linha else { break };
        let mut partes = linha.splitn(2, ' ');
        let comando = partes.next().unwrap_or_default();
        let numero: u64 = partes.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        match comando {
            "save-all" => {
                let _ = writeln!(saida, "salvo");
            }
            "sair" => {
                let _ = saida.flush();
                std::process::exit(i32::try_from(numero).unwrap_or(1));
            }
            "muitas" => {
                for i in 0..numero {
                    let _ = writeln!(saida, "linha {i}");
                }
            }
            "longa" => {
                let _ = writeln!(
                    saida,
                    "{}",
                    "x".repeat(usize::try_from(numero).unwrap_or(0))
                );
            }
            _ => {
                let _ = writeln!(saida, "eco: {linha}");
            }
        }
        let _ = saida.flush();
    }
}
