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
//! # Por que ele não para no `ONDE`
//!
//! `ONDE` sozinho responde a uma pergunta menor do que a que este arquivo
//! promete. O quarto (`MORO`/`QUEM`) é um degrau depois dele, e um ponto de
//! encontro pode estar de pé, atender `ONDE` perfeitamente, e ainda assim ser
//! um binário anterior ao quarto — que fica mudo para `MORO` e `QUEM`, porque
//! nunca ouviu falar desses dois verbos. Um sondar que só mandasse `ONDE`
//! daria verde exatamente nesse caso, e foi assim que um ponto de encontro de
//! produção com binário de antes do quarto passou por «no ar» sem ressalva.
//!
//! Por isso, depois do `ONDE`, ele manda um `MORO` e em seguida um `QUEM` com a
//! mesma marca — o mesmo par que um anfitrião de verdade manda no pacote de
//! reavivamento, encolhido para uma sondagem só. Se o `QUEM` devolver o
//! endereço que o `MORO` acabou de registrar, o quarto está vivo. Se `MORO` e
//! `QUEM` ficarem mudos, o ponto de encontro é velho — e um link guardado
//! contra ele **não** vai voltar a servir, porque não há quarto nenhum do outro
//! lado para lembrar de ninguém.
//!
//! O que ele imprime, quando dá certo, é **o seu endereço visto de fora** — que
//! é literalmente o serviço que este ponto de encontro presta — e, em seguida,
//! qual dos dois casos é este.

use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::Duration;

use seele_proto::encontro::{ler_aqui, moro, onde, quem, Marca, TAMANHO};

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
            Ok(sondagem) => {
                alguem_respondeu = true;
                println!("  {familia}  {destino}  respondeu");
                println!("          ele te vê como {}", sondagem.visto);
                match sondagem.quarto {
                    EstadoDoQuarto::Vivo(endereco) => {
                        println!(
                            "          o quarto está vivo: QUEM devolveu {endereco}, o mesmo \
                             endereço que o MORO acabou de registrar"
                        );
                    }
                    EstadoDoQuarto::Velho => {
                        println!("          o quarto não respondeu: MORO e QUEM ficaram mudos");
                        println!(
                            "          é um ponto de encontro anterior ao quarto — um link \
                             guardado NÃO vai voltar a servir contra ele"
                        );
                    }
                }
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

/// O que uma sondagem completa aprendeu: o degrau 3 (`ONDE`) e o quarto.
struct Sondagem {
    /// O endereço que o `ONDE` devolveu — o seu, visto de fora.
    visto: SocketAddr,
    /// Se o `MORO`/`QUEM` que vieram depois encontraram o quarto vivo.
    quarto: EstadoDoQuarto,
}

/// O que a sondagem do quarto encontrou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EstadoDoQuarto {
    /// `QUEM` devolveu o endereço que o `MORO` anterior registrou.
    Vivo(SocketAddr),
    /// `MORO` ou `QUEM` ficaram mudos.
    ///
    /// É o que um binário anterior ao quarto faz: ele responde `ONDE`
    /// perfeitamente e nunca ouviu falar dos outros dois verbos, então ele
    /// simplesmente cala — que é a resposta deste protocolo para tudo o que
    /// não se sabe responder. Um link guardado contra um ponto de encontro
    /// neste estado não vai voltar a servir: não há memória nenhuma do outro
    /// lado para reencontrar.
    Velho,
}

/// Manda `ONDE`, depois `MORO` e `QUEM` com a mesma marca, e relata os dois
/// separadamente.
fn perguntar(destino: SocketAddr) -> Result<Sondagem, String> {
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

    // O `ONDE` respondeu — o degrau 3 está de pé. O quarto é um degrau
    // depois dele, e mudo aqui não é erro de sondagem: é a resposta que se
    // está medindo.
    let quarto = sondar_quarto(&socket, destino, &marca);

    Ok(Sondagem { visto, quarto })
}

/// Depois do `ONDE`, pergunta se o quarto conhece esta marca: um `MORO` para
/// registrar, e um `QUEM` logo em seguida para conferir se o registro pegou.
///
/// Mesmo socket e mesma marca do `ONDE` — é o par que um anfitrião de verdade
/// manda no pacote de reavivamento, encolhido para uma sondagem só. O prazo do
/// quarto é de 60 s, então um `QUEM` mandado logo depois do `MORO` sempre acha
/// a marca, se o quarto existir.
///
/// Aqui o silêncio não é falha da sondagem: é exatamente o que um ponto de
/// encontro anterior ao quarto faz. Ele nunca ouviu falar de `MORO` nem de
/// `QUEM`, então ele cala — a mesma resposta deste protocolo para qualquer
/// verbo que não conhece.
fn sondar_quarto(socket: &UdpSocket, destino: SocketAddr, marca: &Marca) -> EstadoDoQuarto {
    if socket.send_to(&moro(marca), destino).is_err() {
        return EstadoDoQuarto::Velho;
    }
    if ler_aqui_desta_marca(socket, marca).is_none() {
        return EstadoDoQuarto::Velho;
    }

    if socket.send_to(&quem(marca), destino).is_err() {
        return EstadoDoQuarto::Velho;
    }
    match ler_aqui_desta_marca(socket, marca) {
        Some(endereco) => EstadoDoQuarto::Vivo(endereco),
        None => EstadoDoQuarto::Velho,
    }
}

/// Lê um `AQUI` desta marca, ou `None` — prazo vencido, lixo, ou outra marca.
fn ler_aqui_desta_marca(socket: &UdpSocket, marca: &Marca) -> Option<SocketAddr> {
    let mut buraco = [0_u8; TAMANHO];
    let (lidos, _) = socket.recv_from(&mut buraco).ok()?;
    let (devolvida, visto) = ler_aqui(buraco.get(..lidos).unwrap_or_default())?;
    (devolvida.texto() == marca.texto()).then_some(visto)
}

#[cfg(test)]
mod testes {
    #![allow(
        clippy::expect_used,
        clippy::unwrap_used,
        reason = "num teste, o pânico é o relatório"
    )]

    use super::*;

    /// Um ponto de encontro que só fala `ONDE` — exatamente um binário
    /// anterior ao quarto, que nunca ouviu falar de `MORO` nem `QUEM`.
    fn ponto_de_encontro_velho() -> SocketAddr {
        let socket = UdpSocket::bind("127.0.0.1:0").expect("o loopback não abriu");
        let endereco = socket.local_addr().expect("sem endereço");
        std::thread::spawn(move || {
            let mut balde = [0_u8; TAMANHO];
            loop {
                let Ok((lidos, de)) = socket.recv_from(&mut balde) else {
                    return;
                };
                let pedido =
                    seele_proto::encontro::analisar(balde.get(..lidos).unwrap_or_default());
                if let Some(seele_proto::encontro::Pedido::Onde { marca }) = pedido {
                    let _ = socket.send_to(&seele_proto::encontro::aqui(&marca, de), de);
                }
                // `MORO` e `QUEM`: cala. Este ponto nunca ouviu falar deles.
            }
        });
        endereco
    }

    #[test]
    fn um_ponto_que_so_sabe_onde_e_classificado_como_velho_e_nao_como_no_ar() {
        // O achado que fez este arquivo crescer: um sondar que só manda `ONDE`
        // dá verde num ponto de encontro assim, e é exatamente o caso de
        // produção que ficou mudo em `MORO` e `QUEM` com um binário anterior
        // ao quarto. «No ar» sem ressalva é uma resposta boa demais para o que
        // aconteceu de verdade.
        let destino = ponto_de_encontro_velho();

        let sondagem = perguntar(destino).expect("o ONDE tinha que responder");

        assert_eq!(
            sondagem.quarto,
            EstadoDoQuarto::Velho,
            "um ponto de encontro que ignora MORO e QUEM foi classificado como \
             se o quarto estivesse vivo"
        );
    }
}
