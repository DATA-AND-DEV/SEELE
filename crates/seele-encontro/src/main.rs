//! `seele-encontro` — sobe um ponto de encontro. Degrau 4 do ADR 0022.
//!
//! ```text
//! seele-encontro                 # 8384, IPv4 e IPv6
//! seele-encontro --porta 9000
//! seele-encontro --rede-local    # para experimentar dentro de uma rede só
//! ```
//!
//! Não tem arquivo de configuração, não tem estado em disco e não precisa de
//! banco. Ver `docs/ponto-de-encontro.md` para como deixar um no ar.

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::process::ExitCode;

use seele_encontro::Ponto;
use seele_proto::encontro::{Vizinhanca, PORTA_PADRAO};

fn main() -> ExitCode {
    let mut porta = PORTA_PADRAO;
    let mut vizinhanca = Vizinhanca::Internet;
    let mut barulhento = false;

    let mut argumentos = std::env::args().skip(1);
    while let Some(argumento) = argumentos.next() {
        match argumento.as_str() {
            "--porta" | "-p" => {
                let Some(valor) = argumentos.next().and_then(|texto| texto.parse().ok()) else {
                    eprintln!("--porta pede um número");
                    return ExitCode::FAILURE;
                };
                porta = valor;
            }
            // A frouxidão que permite experimentar o mecanismo inteiro numa
            // rede só, antes de apontar o mundo para ele. Num ponto público
            // isto é um refletor apontado para dentro da rede de quem hospeda.
            "--rede-local" => vizinhanca = Vizinhanca::TambemAqui,
            "--barulhento" => barulhento = true,
            "--ajuda" | "-h" | "--help" => {
                ajuda();
                return ExitCode::SUCCESS;
            }
            outro => {
                eprintln!("não conheço {outro}");
                ajuda();
                return ExitCode::FAILURE;
            }
        }
    }

    if barulhento {
        eprintln!(
            "--barulhento: daqui em diante este processo imprime que endereço \
             falou com que endereço. É exatamente o metadado que ele existe \
             para não guardar; desligue quando acabar de investigar."
        );
    }

    // Uma escuta por família, e não um socket de pilha dupla: o padrão do
    // `IPV6_V6ONLY` muda de sistema para sistema, e o degrau 2 deste mesmo ADR
    // apanhou disso. Duas linhas de execução, nenhuma dependência.
    let mut linhas = Vec::new();
    // **Um quarto para as duas.** Um anfitrião registra por uma família e quem
    // procura pode perguntar pela outra; dois quartos fariam esse par nunca se
    // encontrar, e a resposta seria o silêncio — indistinguível de «esse
    // servidor não existe».
    let quarto = std::sync::Arc::new(seele_encontro::Quarto::novo());
    for escuta in [
        SocketAddr::from((Ipv4Addr::UNSPECIFIED, porta)),
        SocketAddr::from((Ipv6Addr::UNSPECIFIED, porta)),
    ] {
        match Ponto::abrir_com_quarto(escuta, vizinhanca, std::sync::Arc::clone(&quarto))
            .map(|ponto| ponto.barulhento(barulhento))
        {
            Ok(ponto) => {
                eprintln!("ponto de encontro atendendo em {escuta}");
                linhas.push(std::thread::spawn(move || {
                    // `servir` só volta com erro: o `Ok` dele é `Infallible`.
                    let Err(erro) = ponto.servir();
                    eprintln!("a escuta {escuta} parou: {erro}");
                }));
            }
            // Uma VPS sem IPv6 é o caso comum, e ficar de pé só com IPv4 é o
            // comportamento certo. Ficar de pé com nenhuma das duas não é, e é
            // por isso que a conta é feita depois do laço.
            Err(erro) => eprintln!("não deu para escutar em {escuta}: {erro}"),
        }
    }

    if linhas.is_empty() {
        eprintln!("nenhuma das duas famílias abriu a porta {porta}; não há o que servir");
        return ExitCode::FAILURE;
    }

    for linha in linhas {
        let _ = linha.join();
    }
    ExitCode::SUCCESS
}

fn ajuda() {
    eprintln!("seele-encontro — o ponto de encontro do degrau 4 do ADR 0022");
    eprintln!();
    eprintln!("  --porta N       em que porta atender (padrão {PORTA_PADRAO})");
    eprintln!("  --rede-local    também apresentar endereços de rede local (só para experimentar)");
    eprintln!("  --barulhento    imprimir quem falou com quem (é metadado; desligue depois)");
    eprintln!();
    eprintln!("Ele não guarda nada e não vê nada do que é dito: o TLS é ponta a ponta.");
}
