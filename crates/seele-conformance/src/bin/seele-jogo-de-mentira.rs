//! **O servidor de jogo de mentira das provas da reserva** (API 8 parte 7,
//! `tests/reserva.rs`): um processo de verdade, no Mac e no Windows, que o
//! `programa` roda como roda o Minecraft.
//!
//! `seele-jogo-de-mentira PORTA`: escuta um eco TCP em `127.0.0.1:PORTA` (o
//! jogo que o túnel serve), escreve um contador crescente em `batimento` (na
//! pasta de trabalho) a cada 20 ms — é como a prova vê se ele está vivo — e
//! sai quando lê `stop` na entrada. Não faz nada no produto e não é
//! empacotado.

use std::io::{BufRead as _, Read as _, Write as _};

fn main() {
    let Some(porta) = std::env::args().nth(1).and_then(|p| p.parse::<u16>().ok()) else {
        std::process::exit(2);
    };
    let Ok(escuta) = std::net::TcpListener::bind(("127.0.0.1", porta)) else {
        eprintln!("a porta {porta} não abriu");
        std::process::exit(3);
    };
    std::thread::spawn(move || {
        for fluxo in escuta.incoming().flatten() {
            std::thread::spawn(move || {
                let mut fluxo = fluxo;
                let mut buf = [0_u8; 16 * 1024];
                while let Ok(n) = fluxo.read(&mut buf) {
                    if n == 0 || fluxo.write_all(buf.get(..n).unwrap_or_default()).is_err() {
                        return;
                    }
                }
            });
        }
    });
    std::thread::spawn(|| {
        let mut n = 0_u64;
        loop {
            n += 1;
            let _ = std::fs::write("batimento", n.to_string());
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    });
    println!("pronto");
    let _ = std::io::stdout().flush();
    for linha in std::io::stdin().lock().lines() {
        match linha {
            Ok(l) if l.trim() == "stop" => {
                println!("parando");
                return;
            }
            Ok(_) => {}
            Err(_) => return,
        }
    }
}
