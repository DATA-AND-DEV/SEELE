//! Pergunta a um ponto de encontro se ele está vivo, e o que ele vê de você.
//!
//!     cargo run -p seele-encontro --example sondar -- 203.0.113.7:8384
//!
//! Existe porque `docs/ponto-de-encontro.md` manda subir o seu e não oferecia
//! nenhuma forma de conferir. `systemctl status` diz que o processo está de pé,
//! que é uma pergunta diferente de «alguém de fora alcança esta porta» — e a
//! segunda é a única que importa. Entre as duas há um firewall de sistema, um
//! firewall de provedor e uma regra de porta, e cada um deles já quebrou este
//! produto em máquina de gente de verdade.
//!
//! Ele fala o protocolo mesmo (`ONDE` → `AQUI`), e não um `ping`: um serviço que
//! responde a ICMP e recusa datagrama de 96 bytes está tão quebrado quanto um
//! desligado, e só isto distingue os dois.
//!
//! O que ele imprime, quando dá certo, é **o seu endereço visto de fora** — que
//! é literalmente o serviço que este ponto de encontro presta.

use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::Duration;

use seele_proto::encontro::{ler_aqui, onde, Marca, TAMANHO};

fn main() -> std::process::ExitCode {
    let Some(alvo) = std::env::args().nth(1) else {
        eprintln!("uso: sondar <endereço:porta>");
        eprintln!();
        eprintln!("exemplo: cargo run -p seele-encontro --example sondar -- 203.0.113.7:8384");
        return std::process::ExitCode::FAILURE;
    };

    // Resolvido à mão, e as duas famílias tentadas em separado: um ponto de
    // encontro que atende IPv4 e não IPv6 apresenta mal justamente os pares que
    // mais precisam dele, e a resolução sozinha esconderia isso escolhendo uma
    // e calando sobre a outra.
    let enderecos: Vec<SocketAddr> = match alvo.to_socket_addrs() {
        Ok(achados) => achados.collect(),
        Err(erro) => {
            eprintln!("não consegui resolver «{alvo}»: {erro}");
            return std::process::ExitCode::FAILURE;
        }
    };

    if enderecos.is_empty() {
        eprintln!("«{alvo}» não resolveu para endereço nenhum");
        return std::process::ExitCode::FAILURE;
    }

    let mut alguem_respondeu = false;
    for destino in enderecos {
        let familia = if destino.is_ipv6() { "IPv6" } else { "IPv4" };
        match perguntar(destino) {
            Ok(visto) => {
                alguem_respondeu = true;
                println!("  {familia}  {destino}  respondeu");
                println!("          ele te vê como {visto}");
            }
            Err(motivo) => {
                println!("  {familia}  {destino}  {motivo}");
            }
        }
    }

    println!();
    if alguem_respondeu {
        println!("o ponto de encontro está no ar.");
        std::process::ExitCode::SUCCESS
    } else {
        println!("ninguém respondeu. Na ordem em que costuma quebrar:");
        println!("  1. o firewall do provedor (painel), que é o mais esquecido");
        println!("  2. o firewall da máquina — `ufw status` deve liberar 8384/udp");
        println!("  3. o serviço em si — `systemctl status seele-encontro`");
        println!("  4. a porta, se você a trocou com `--porta`");
        std::process::ExitCode::FAILURE
    }
}

/// Manda um `ONDE` e espera o `AQUI` que responde a ele.
fn perguntar(destino: SocketAddr) -> Result<SocketAddr, String> {
    // Ligado na mesma família do destino: um socket IPv4 não fala com um
    // endereço IPv6, e o erro que ele dá não parece ter nada a ver.
    let local = if destino.is_ipv6() {
        "[::]:0"
    } else {
        "0.0.0.0:0"
    };
    let socket = UdpSocket::bind(local).map_err(|erro| format!("socket local: {erro}"))?;
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|erro| format!("prazo: {erro}"))?;

    let marca = Marca::nova("sondagem").ok_or("marca inválida")?;
    socket
        .send_to(&onde(&marca), destino)
        .map_err(|erro| format!("não saiu daqui: {erro}"))?;

    let mut buraco = [0_u8; TAMANHO];
    let (lidos, _) = socket
        .recv_from(&mut buraco)
        .map_err(|_| "não respondeu em 2s".to_owned())?;

    let Some((devolvida, visto)) = ler_aqui(buraco.get(..lidos).unwrap_or_default()) else {
        return Err("respondeu algo que não é deste protocolo".to_owned());
    };
    if devolvida.texto() != marca.texto() {
        return Err("respondeu com outra marca".to_owned());
    }
    Ok(visto)
}
