//! Pergunta a um ponto de encontro se ele está vivo, e o que ele vê de você.
//!
//!     cargo run -p seele-encontro --example sondar -- 203.0.113.7:8384
//!     cargo run -p seele-encontro --example sondar -- rendzvous.seele.sh
//!     cargo run -p seele-encontro --example sondar -- --exige-enc2 rendzvous.seele.sh
//!
//! Sem porta, a do ponto de encontro (8384), como o link escreve o ponto.
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
//!
//! # O ENC/2, e o portão da 0.16
//!
//! Depois do quarto, ele sonda o ENC/2 (ADR 0062) com uma chave de sondagem
//! nova a cada rodada: um `MORO2` sem cookie, que um ponto com o ENC/2 responde
//! com o `DESAFIO`; o mesmo `MORO2`, com o cookie e o carimbo de uma hora atrás,
//! que ele recusa dizendo a hora dele (é assim que o relógio do ponto é
//! medido); um `MORO2` com o cookie, o carimbo de agora e uma dica sintética de
//! rede local, que ele guarda; um `QUEM2` da mesma marca, que devolve o
//! endereço e a dica; e um `QUEM2` de uma marca que ninguém registrou, que diz
//! `NINGUEM`. Cada pedido sai até três vezes.
//!
//! Um ponto anterior ao ENC/2 cala para os quatro, e é o caso que o portão
//! existe para pegar: a sala de toda pessoa ficaria sem registro, e quem liga
//! pelo código não acharia ninguém. Foi o ADR 0047 com o quarto: dez dias de
//! recurso morto por um binário velho na VPS. Com `--exige-enc2`, este exemplo
//! sai com falha quando nenhuma família fala o ENC/2, ou quando alguma família
//! que respondeu ao `ONDE` não fala, e é isso que se roda antes de publicar uma
//! versão que depende dele (`docs/ponto-de-encontro.md`).

use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Signer, SigningKey};
use seele_proto::codigo::Codigo;
use seele_proto::encontro::{
    atende, chama, ler_aqui, ler_resposta2, ler_resposta3, ler_resposta_do_sinal,
    mensagem_do_moro3, moro, moro2, moro3, onde, pedido_do_sinal, quem, quem2, quem3, Atende,
    Chama, Credencial, Marca, Marcas, Moro2, Motivo, Resposta2, Resposta3, RespostaDoSinal,
    TAMANHO2,
};
use seele_proto::uri::separar_ponto;

fn main() -> std::process::ExitCode {
    let argumentos: Vec<String> = std::env::args().skip(1).collect();
    // O sinal do navegador é HTTPS, e o TLS não é deste exemplo (sem dependência de TLS): quem
    // fala HTTPS é o `curl` do `publicar.sh`, que confere o certificado. Daqui saem o corpo do
    // `POST /sinal/1` e a conferência da resposta, com os tipos do protocolo, e não por texto.
    if argumentos.first().map(String::as_str) == Some("--corpo-do-sinal") {
        let Some(corpo) = corpo_do_sinal() else {
            eprintln!("não deu para montar uma credencial de sondagem");
            return std::process::ExitCode::FAILURE;
        };
        println!("{corpo}");
        return std::process::ExitCode::SUCCESS;
    }
    if argumentos.first().map(String::as_str) == Some("--resposta-do-sinal") {
        let mut corpo = String::new();
        let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut corpo);
        return match conferir_resposta_do_sinal(&corpo) {
            Ok(()) => {
                println!(
                    "o sinal respondeu SEELE-SINAL/1 NINGUEM a uma marca que ninguém registrou"
                );
                std::process::ExitCode::SUCCESS
            }
            Err(motivo) => {
                println!("o sinal NÃO respondeu o que o protocolo promete: {motivo}");
                std::process::ExitCode::FAILURE
            }
        };
    }
    let Some((alvo, exige_enc2, exige_enc3)) = ler_argumentos(&argumentos) else {
        eprintln!(
            "uso: sondar [--exige-enc2] [--exige-enc3] <endereço[:porta]>   (sem porta, a do ponto de encontro)"
        );
        eprintln!(
            "      sondar --corpo-do-sinal          (o corpo de um POST /sinal/1, para o curl)"
        );
        eprintln!("      sondar --resposta-do-sinal < corpo  (confere a resposta do sinal)");
        eprintln!();
        eprintln!("exemplo: cargo run -p seele-encontro --example sondar -- 203.0.113.7:8384");
        eprintln!(
            "--exige-enc2: sai com falha se o ponto não falar o ENC/2 em toda família que responde"
        );
        eprintln!(
            "--exige-enc3: também o ENC/3 e o STUN Binding (a 0.17) em toda família que responde"
        );
        return std::process::ExitCode::FAILURE;
    };

    // Resolvido à mão, e as duas famílias tentadas em separado: um ponto de
    // encontro que atende IPv4 e não IPv6 apresenta mal justamente os pares que
    // mais precisam dele, e a resolução sozinha esconderia isso escolhendo uma
    // e calando sobre a outra.
    let enderecos = match enderecos_do_ponto(&alvo) {
        Ok(achados) => achados,
        Err(motivo) => {
            eprintln!("{motivo}");
            return std::process::ExitCode::FAILURE;
        }
    };

    if enderecos.is_empty() {
        eprintln!("«{alvo}» não resolveu para endereço nenhum");
        return std::process::ExitCode::FAILURE;
    }

    let mut resultados = Vec::new();
    for destino in enderecos {
        let familia = if destino.is_ipv6() { "IPv6" } else { "IPv4" };
        let resultado = perguntar(destino);
        match &resultado {
            Ok(sondagem) => {
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
                match &sondagem.enc2 {
                    EstadoDoEnc2::Fala { relogio_do_ponto } => println!(
                        "          o ENC/2: fala — MORO2, QUEM2 e NINGUEM responderam, e o relógio \
                         do ponto difere do daqui em {relogio_do_ponto} s"
                    ),
                    EstadoDoEnc2::Mudo => println!(
                        "          o ENC/2: mudo — é um ponto anterior ao ENC/2, e quem liga pelo \
                         código não acha ninguém nele"
                    ),
                    EstadoDoEnc2::Torto(motivo) => println!("          o ENC/2: torto — {motivo}"),
                }
                match &sondagem.enc3 {
                    EstadoDoEnc3::Fala => println!(
                        "          o ENC/3: fala — MORO3 morou, QUEM3 achou, e o CHAMA foi atendido pela escuta"
                    ),
                    EstadoDoEnc3::Mudo => println!(
                        "          o ENC/3: mudo — é um ponto anterior à 0.17, e uma sala da 0.17 \
                         não se registra nele"
                    ),
                    EstadoDoEnc3::Torto(motivo) => println!("          o ENC/3: torto — {motivo}"),
                }
                match &sondagem.stun {
                    Ok(visto) => println!(
                        "          o STUN: responde — o Binding devolveu {visto} (XOR-MAPPED-ADDRESS)"
                    ),
                    Err(motivo) => println!(
                        "          o STUN: {motivo} — o navegador não aprende o próprio endereço \
                         público por este ponto"
                    ),
                }
            }
            Err(motivo) => {
                println!("  {familia}  {destino}  {motivo}");
            }
        }
        resultados.push(resultado);
    }

    println!();
    let alguem_respondeu = resultados.iter().any(Result::is_ok);
    if passou_no_portao(&resultados, exige_enc2 || exige_enc3)
        && (!exige_enc3 || fala_a_017(&resultados))
    {
        println!("o ponto de encontro está no ar.");
        if exige_enc2 || exige_enc3 {
            println!("e fala o ENC/2 em toda família que respondeu.");
        }
        if exige_enc3 {
            println!("e fala o ENC/3 e responde STUN Binding em toda família que respondeu.");
        }
        std::process::ExitCode::SUCCESS
    } else if alguem_respondeu {
        println!(
            "o ponto de encontro está no ar, e NÃO fala o ENC/2{} em toda família que \
             respondeu, ou o relógio dele está a mais de {RELOGIO_DO_PORTAO} s do daqui.",
            if exige_enc3 { ", o ENC/3 e o STUN" } else { "" }
        );
        println!("uma versão que depende do ENC/2 não sai com este ponto: reimplante a VPS antes");
        println!("(docs/ponto-de-encontro.md, «Subir o seu»).");
        std::process::ExitCode::FAILURE
    } else {
        println!("ninguém respondeu. Na ordem em que costuma quebrar:");
        println!("  1. o firewall do provedor (painel), que é o mais esquecido");
        println!("  2. o firewall da máquina — `ufw status` deve liberar 8384/udp");
        println!("  3. o serviço em si — `systemctl status seele-encontro`");
        println!("  4. a porta, se você a trocou com `--porta`");
        std::process::ExitCode::FAILURE
    }
}

/// Os endereços do ponto de encontro, a partir do texto que se passou.
///
/// **Com a porta padrão quando ela falta**, pela regra de
/// [`seele_proto::uri::separar_ponto`], a mesma com que o cliente pergunta ao
/// quarto. O texto ia cru para `to_socket_addrs`, que recusa nome sem porta, e o
/// link escreve o ponto sem porta (`enc=encontro.seele.app.br/…`): esta
/// ferramenta, que é a da medida de campo, recusava exatamente o ponto que o
/// link carrega.
fn enderecos_do_ponto(alvo: &str) -> Result<Vec<SocketAddr>, String> {
    let ponto =
        separar_ponto(alvo).map_err(|erro| format!("«{alvo}» não é um endereço: {erro}"))?;
    (ponto.maquina, ponto.porta)
        .to_socket_addrs()
        .map(Iterator::collect)
        .map_err(|erro| format!("não consegui resolver «{alvo}»: {erro}"))
}

/// O alvo e a bandeira `--exige-enc2`, em qualquer ordem. `None` sem alvo, ou
/// com uma bandeira que este exemplo não conhece.
fn ler_argumentos(argumentos: &[String]) -> Option<(String, bool, bool)> {
    let mut alvo = None;
    let mut exige_enc2 = false;
    let mut exige_enc3 = false;
    for argumento in argumentos {
        match argumento.as_str() {
            "--exige-enc2" => exige_enc2 = true,
            "--exige-enc3" => exige_enc3 = true,
            outro if outro.starts_with("--") => return None,
            outro if alvo.is_none() => alvo = Some(outro.to_owned()),
            _ => return None,
        }
    }
    alvo.map(|alvo| (alvo, exige_enc2, exige_enc3))
}

/// O portão da 0.17, por cima do do ENC/2: toda família que respondeu ao `ONDE` fala o ENC/3 e
/// responde STUN Binding. A sala da 0.17 se registra pelo ENC/3, e o navegador aprende o próprio
/// endereço pelo STUN deste ponto e de nenhum outro (D8): um ponto da 0.16 passa no ENC/2 e
/// deixa as duas coisas mortas.
fn fala_a_017(resultados: &[Result<Sondagem, String>]) -> bool {
    let responderam: Vec<&Sondagem> = resultados.iter().filter_map(|r| r.as_ref().ok()).collect();
    !responderam.is_empty()
        && responderam
            .iter()
            .all(|sondagem| sondagem.enc3 == EstadoDoEnc3::Fala && sondagem.stun.is_ok())
}

/// Quanto o relógio do ponto pode se afastar do daqui para o portão passar.
///
/// A tolerância do carimbo é de 300 s, e o relógio de quem registra erra
/// também: um ponto 240 s adiantado recusa uma sala 61 s atrasada. Um minuto
/// deixa o resto da tolerância para as máquinas de quem usa.
const RELOGIO_DO_PORTAO: i64 = 60;

/// Se a sondagem passa.
///
/// Sem `--exige-enc2`, como sempre foi: alguma família respondeu ao `ONDE`.
/// Com ela, alguma família fala o ENC/2, e **nenhuma** que respondeu ao `ONDE`
/// deixa de falar, ou tem o relógio a mais de [`RELOGIO_DO_PORTAO`] segundos do
/// daqui. Uma família que não respondeu a nada não reprova: a máquina que sonda
/// pode não ter rota IPv6, e isso não diz nada do ponto.
fn passou_no_portao(resultados: &[Result<Sondagem, String>], exige_enc2: bool) -> bool {
    let responderam: Vec<&Sondagem> = resultados.iter().filter_map(|r| r.as_ref().ok()).collect();
    if !exige_enc2 {
        return !responderam.is_empty();
    }
    !responderam.is_empty() && responderam.iter().all(|sondagem| {
        matches!(
            sondagem.enc2,
            EstadoDoEnc2::Fala { relogio_do_ponto } if relogio_do_ponto.abs() <= RELOGIO_DO_PORTAO
        )
    })
}

/// O que uma sondagem completa aprendeu: o degrau 3 (`ONDE`), o quarto, e o
/// ENC/2.
#[derive(Debug)]
struct Sondagem {
    /// O endereço que o `ONDE` devolveu — o seu, visto de fora.
    visto: SocketAddr,
    /// Se o `MORO`/`QUEM` que vieram depois encontraram o quarto vivo.
    quarto: EstadoDoQuarto,
    /// Se o ponto fala o ENC/2.
    enc2: EstadoDoEnc2,
    /// Se o ponto fala o ENC/3 (a 0.17).
    enc3: EstadoDoEnc3,
    /// O endereço que o STUN Binding devolveu, ou por que não.
    stun: Result<SocketAddr, String>,
}

/// O que a sondagem do ENC/3 achou.
#[derive(Debug, Clone, PartialEq, Eq)]
enum EstadoDoEnc3 {
    /// O `QUEM3` de uma marca que ninguém registrou ouviu `NINGUEM` dela.
    Fala,
    /// Calou para o ENC/3: um binário anterior à 0.17.
    Mudo,
    /// Respondeu, mas não o que o ENC/3 promete.
    Torto(String),
}

/// O que a sondagem do ENC/2 achou.
#[derive(Debug, Clone, PartialEq, Eq)]
enum EstadoDoEnc2 {
    /// O `MORO2` assinado mora (`MOROU`), o `QUEM2` devolve o endereço e a
    /// dica que ele registrou, e o `QUEM2` de uma marca vazia diz `NINGUEM`.
    Fala {
        /// O relógio do ponto menos o daqui, em segundos, medido pela `RECUSA`
        /// de um carimbo de propósito uma hora atrás.
        relogio_do_ponto: i64,
    },
    /// Calou para o ENC/2: um binário anterior a ele.
    Mudo,
    /// Respondeu, mas não o que o ENC/2 promete. O texto diz o quê.
    Torto(String),
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

    // Prazo ABSOLUTO e só o que vem do ponto: lixo ou outro remetente não renovam a espera.
    let Some(visto) = receber_ate(
        &socket,
        destino,
        Instant::now() + Duration::from_secs(2),
        |dados| {
            ler_aqui(dados)
                .filter(|(devolvida, _)| devolvida.texto() == marca.texto())
                .map(|(_, visto)| visto)
        },
    ) else {
        return Err("não respondeu em 2s um AQUI desta marca".to_owned());
    };

    // O `ONDE` respondeu — o degrau 3 está de pé. O quarto é um degrau
    // depois dele, e mudo aqui não é erro de sondagem: é a resposta que se
    // está medindo. O ENC/2, a mesma coisa.
    let quarto = sondar_quarto(&socket, destino, &marca);
    let enc2 = sondar_enc2(&socket, destino);
    let enc3 = sondar_enc3(&socket, destino);
    let stun = sondar_stun(&socket, destino);

    Ok(Sondagem {
        visto,
        quarto,
        enc2,
        enc3,
        stun,
    })
}

/// Segundos Unix no relógio daqui.
fn agora_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |desde| desde.as_secs())
}

/// Uma chave de sondagem nova a cada rodada, sem `rand`: os nanossegundos de
/// agora e o número do processo. Ela não protege nada — só não pode repetir a
/// de outra sondagem, que o ponto recusaria como «velho» se viesse de outro
/// endereço no mesmo segundo.
fn chave_de_sondagem(variante: u8) -> SigningKey {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |desde| desde.as_nanos());
    let mut semente = [variante; 32];
    for (destino, byte) in semente.iter_mut().zip(
        nanos
            .to_le_bytes()
            .into_iter()
            .chain(std::process::id().to_le_bytes()),
    ) {
        *destino ^= byte;
    }
    SigningKey::from_bytes(&semente)
}

/// Um `MORO2` assinado por esta chave, para a marca do servidor da sala dela.
fn moro2_de(chave: &SigningKey, carimbo: u64, dicas: Vec<SocketAddr>) -> Moro2 {
    let publica = chave.verifying_key().to_bytes();
    let marca = Marcas::da_sala(&Codigo::da_chave(&publica))
        .servidor()
        .clone();
    let mensagem = seele_proto::assinatura::moro2(&marca, &publica, carimbo, &dicas);
    Moro2 {
        marca,
        chave: publica,
        carimbo,
        dicas,
        assinatura: chave.sign(&mensagem).to_bytes(),
        cookie: None,
    }
}

/// Os quatro passos do ENC/2 (ver o cabeçalho). Silêncio no primeiro é
/// [`EstadoDoEnc2::Mudo`]; qualquer outra coisa fora do prometido é
/// [`EstadoDoEnc2::Torto`], com o que veio.
fn sondar_enc2(socket: &UdpSocket, destino: SocketAddr) -> EstadoDoEnc2 {
    let chave = chave_de_sondagem(0x5E);
    let uma_hora_atras = agora_unix().saturating_sub(3600);
    let mut torto = moro2_de(&chave, uma_hora_atras, Vec::new());

    // Sem cookie, o DESAFIO (D-enc-4). Silêncio aqui é um ponto de antes do
    // ENC/2.
    let cookie = match pedir2(socket, destino, &moro2(&torto), &torto.marca) {
        None => return EstadoDoEnc2::Mudo,
        Some(Resposta2::Desafio { cookie, .. }) => cookie,
        Some(outra) => {
            return EstadoDoEnc2::Torto(format!(
                "um MORO2 sem cookie não ouviu o DESAFIO, e sim {outra:?}: o ponto fala um \
                 ENC/2 de antes do cookie"
            ))
        }
    };
    torto.cookie = Some(cookie);

    let relogio_do_ponto = match pedir2(socket, destino, &moro2(&torto), &torto.marca) {
        None => {
            return EstadoDoEnc2::Torto(
                "o MORO2 com o cookie do DESAFIO ficou sem resposta em três tentativas".to_owned(),
            )
        }
        Some(Resposta2::Recusa {
            motivo: Motivo::Relogio,
            agora,
            ..
        }) => i64::try_from(i128::from(agora) - i128::from(agora_unix())).unwrap_or(i64::MAX),
        Some(Resposta2::Desafio { .. }) => {
            return EstadoDoEnc2::Torto(
                "o cookie que o próprio ponto deu não valeu no MORO2 seguinte".to_owned(),
            )
        }
        Some(Resposta2::Morou { .. }) => {
            return EstadoDoEnc2::Torto(
                "o ponto aceitou um carimbo de uma hora atrás: o relógio dele está atrasado \
                     em cerca de uma hora, e o de toda sala que registrar vai ser recusado"
                    .to_owned(),
            )
        }
        Some(outra) => {
            return EstadoDoEnc2::Torto(format!("ao carimbo torto, respondeu {outra:?}"))
        }
    };

    let dica: SocketAddr = SocketAddr::from(([10, 255, 255, 254], 8385));
    let mut certo = moro2_de(&chave, agora_unix(), vec![dica]);
    certo.cookie = Some(cookie);
    let registrado = match pedir2(socket, destino, &moro2(&certo), &certo.marca) {
        Some(Resposta2::Morou { endereco, .. }) => endereco,
        Some(Resposta2::Recusa {
            motivo: Motivo::Relogio,
            ..
        }) => {
            return EstadoDoEnc2::Torto(format!(
                "o relógio do ponto difere do daqui em {relogio_do_ponto} s, além dos 300 de \
                 tolerância: o registro de toda sala vai ser recusado"
            ))
        }
        Some(outra) => {
            return EstadoDoEnc2::Torto(format!("ao MORO2 de agora, respondeu {outra:?}"))
        }
        None => {
            return EstadoDoEnc2::Torto(
                "o MORO2 de agora ficou sem resposta em três tentativas".to_owned(),
            )
        }
    };

    match pedir2(socket, destino, &quem2(&certo.marca), &certo.marca) {
        Some(Resposta2::Aqui2 {
            endereco, dicas, ..
        }) if endereco == registrado && dicas == [dica] => {}
        outra => {
            return EstadoDoEnc2::Torto(format!(
                "o QUEM2 não devolveu o endereço ({registrado}) e a dica que o MORO2 registrou: \
                 {outra:?}"
            ))
        }
    }

    let ninguem = moro2_de(&chave_de_sondagem(0xA1), agora_unix(), Vec::new()).marca;
    match pedir2(socket, destino, &quem2(&ninguem), &ninguem) {
        Some(Resposta2::Ninguem { .. }) => EstadoDoEnc2::Fala { relogio_do_ponto },
        outra => EstadoDoEnc2::Torto(format!(
            "o QUEM2 de uma marca que ninguém registrou não disse NINGUEM: {outra:?}"
        )),
    }
}

/// Quantas vezes um pedido do ENC/2 sai antes de a sondagem dar o silêncio
/// por resposta.
const TENTATIVAS: u32 = 3;

/// Quanto se espera pela resposta de cada tentativa.
const ESPERA_POR_TENTATIVA: Duration = Duration::from_millis(700);

/// Manda um pedido do ENC/2 e espera a resposta desta marca, até
/// [`TENTATIVAS`] vezes. `None` quando nenhuma veio, ou quando nada saiu daqui.
///
/// Repetido porque um pacote perdido virava «mudo», com a frase «é um ponto
/// anterior ao ENC/2»: o portão fechava, e o texto enganava. Os pedidos se
/// repetem sem estrago: o mesmo `MORO2` do mesmo socket é uma retransmissão
/// para o ponto (ADR 0062), e o `QUEM2` não guarda nada.
fn pedir2(
    socket: &UdpSocket,
    destino: SocketAddr,
    datagrama: &[u8],
    marca: &Marca,
) -> Option<Resposta2> {
    pedir(socket, destino, datagrama, |dados| {
        ler_resposta2(dados).filter(|resposta| resposta.marca() == marca)
    })
}

/// Manda e espera, até [`TENTATIVAS`] vezes, cada uma com o prazo ABSOLUTO de
/// [`ESPERA_POR_TENTATIVA`]. `None` quando nenhuma resposta aceita veio.
fn pedir<T>(
    socket: &UdpSocket,
    destino: SocketAddr,
    datagrama: &[u8],
    mut aceita: impl FnMut(&[u8]) -> Option<T>,
) -> Option<T> {
    for _ in 0..TENTATIVAS {
        if socket.send_to(datagrama, destino).is_err() {
            return None;
        }
        let prazo = Instant::now() + ESPERA_POR_TENTATIVA;
        if let Some(veio) = receber_ate(socket, destino, prazo, &mut aceita) {
            return Some(veio);
        }
    }
    None
}

/// Lê até `aceita` reconhecer a resposta, ou até o `prazo` (absoluto). Só conta
/// o que vem do `destino` (o IP e a porta do ponto): um datagrama de outro
/// remetente, ou um que `aceita` recusa, é pulado **sem renovar o prazo** — um
/// fluxo de lixo não segura a sondagem para sempre, e um terceiro não responde
/// por ele.
fn receber_ate<T>(
    socket: &UdpSocket,
    destino: SocketAddr,
    prazo: Instant,
    mut aceita: impl FnMut(&[u8]) -> Option<T>,
) -> Option<T> {
    let antes = socket.read_timeout().ok().flatten();
    let mut balde = [0_u8; TAMANHO2 + 1];
    let veio = loop {
        let resta = prazo.saturating_duration_since(Instant::now());
        if resta.is_zero() || socket.set_read_timeout(Some(resta)).is_err() {
            break None;
        }
        let Ok((lidos, de)) = socket.recv_from(&mut balde) else {
            break None;
        };
        if de.ip().to_canonical() != destino.ip().to_canonical() || de.port() != destino.port() {
            continue;
        }
        if let Some(resposta) = aceita(balde.get(..lidos).unwrap_or_default()) {
            break Some(resposta);
        }
    };
    let _ = socket.set_read_timeout(antes);
    veio
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
    if ler_aqui_desta_marca(socket, destino, marca).is_none() {
        return EstadoDoQuarto::Velho;
    }

    if socket.send_to(&quem(marca), destino).is_err() {
        return EstadoDoQuarto::Velho;
    }
    match ler_aqui_desta_marca(socket, destino, marca) {
        Some(endereco) => EstadoDoQuarto::Vivo(endereco),
        None => EstadoDoQuarto::Velho,
    }
}

/// Lê um `AQUI` desta marca, ou `None` — prazo vencido, lixo, ou outra marca.
fn ler_aqui_desta_marca(
    socket: &UdpSocket,
    destino: SocketAddr,
    marca: &Marca,
) -> Option<SocketAddr> {
    receber_ate(
        socket,
        destino,
        Instant::now() + Duration::from_secs(2),
        |dados| {
            ler_aqui(dados)
                .filter(|(devolvida, _)| devolvida.texto() == marca.texto())
                .map(|(_, visto)| visto)
        },
    )
}

/// Um `MORO3` assinado pela chave de sondagem, para uma das marcas da sala dela.
fn moro3_de(chave: &SigningKey, marca: &Marca, cookie: Option<[u8; 16]>) -> Moro2 {
    let publica = chave.verifying_key().to_bytes();
    let carimbo = agora_unix();
    let mensagem = mensagem_do_moro3(marca, &publica, carimbo, &[]);
    Moro2 {
        marca: marca.clone(),
        chave: publica,
        carimbo,
        dicas: Vec::new(),
        assinatura: chave.sign(&mensagem).to_bytes(),
        cookie,
    }
}

/// Uma resposta do ENC/3 que diz respeito a esta marca (a recusa também).
fn resposta3_de(dados: &[u8], marca: &Marca) -> Option<Resposta3> {
    let resposta = ler_resposta3(dados)?;
    let dela = match &resposta {
        Resposta3::Ninguem { marca: dela }
        | Resposta3::Aqui { marca: dela, .. }
        | Resposta3::Desafio { marca: dela, .. }
        | Resposta3::Morou { marca: dela, .. }
        | Resposta3::Recusa { marca: dela, .. } => dela == marca,
        Resposta3::Calado { .. } | Resposta3::Chamado(_) => false,
    };
    dela.then_some(resposta)
}

/// O ENC/3 da 0.17 de ponta a ponta, como uma sala e quem liga para ela, numa
/// sala EFÊMERA (uma chave nova a cada rodada; o registro expira sozinho no
/// prazo do quarto):
///
/// 1. o anfitrião (um socket próprio) manda o `MORO3` sem cookie → `DESAFIO`.
///    Silêncio aqui é um ponto anterior à 0.17;
/// 2. o `MORO3` assinado com o cookie, da marca do servidor → `MOROU`, e o
///    endereço que o ponto viu; o mesmo para a marca da escuta;
/// 3. quem liga (o socket da sondagem) pergunta `QUEM3` → `AQUI` com o
///    endereço registrado; e de uma marca vazia → `NINGUEM`;
/// 4. quem liga manda o `CHAMA`; a escuta do anfitrião recebe o `CHAMADO`,
///    responde o `ATENDE` com o `id` dele, e quem liga ouve o `AQUI` do servidor.
///
/// Um ponto que responde `NINGUEM` e nunca aceita um `MORO3` reprova no passo 2.
fn sondar_enc3(chamador: &UdpSocket, destino: SocketAddr) -> EstadoDoEnc3 {
    let local = if destino.is_ipv6() {
        "[::]:0"
    } else {
        "0.0.0.0:0"
    };
    let Ok(anfitriao) = UdpSocket::bind(local) else {
        return EstadoDoEnc3::Torto("o socket do anfitrião de sondagem não abriu".to_owned());
    };
    let chave = chave_de_sondagem(0x3C);
    let marcas = Marcas::da_sala(&Codigo::da_chave(&chave.verifying_key().to_bytes()));
    let (servidor, escuta) = (marcas.servidor().clone(), marcas.escuta().clone());

    let cookie = match pedir(
        &anfitriao,
        destino,
        &moro3(&moro3_de(&chave, &servidor, None)),
        |d| resposta3_de(d, &servidor),
    ) {
        None => return EstadoDoEnc3::Mudo,
        Some(Resposta3::Desafio { cookie, .. }) => cookie,
        Some(outra) => {
            return EstadoDoEnc3::Torto(format!(
                "um MORO3 sem cookie não ouviu o DESAFIO, e sim {outra:?}"
            ))
        }
    };
    let mut registrado = None;
    for marca in [&servidor, &escuta] {
        match pedir(&anfitriao, destino, &moro3(&moro3_de(&chave, marca, Some(cookie))), |d| {
            resposta3_de(d, marca)
        }) {
            Some(Resposta3::Morou { endereco, .. }) => {
                if registrado.is_none() {
                    registrado = Some(endereco);
                }
            }
            outra => {
                return EstadoDoEnc3::Torto(format!(
                    "o MORO3 assinado, com o cookie do DESAFIO, não morou ({marca}): {outra:?} — o ponto \
                     responde ao ENC/3 e não registra a sala da 0.17"
                ))
            }
        }
    }
    let Some(registrado) = registrado else {
        return EstadoDoEnc3::Torto("nenhum MOROU".to_owned());
    };

    match pedir(chamador, destino, &quem3(&servidor), |d| {
        resposta3_de(d, &servidor)
    }) {
        Some(Resposta3::Aqui { endereco, .. }) if endereco == registrado => {}
        outra => {
            return EstadoDoEnc3::Torto(format!(
                "o QUEM3 não devolveu o endereço que o MORO3 registrou ({registrado}): {outra:?}"
            ))
        }
    }
    let vazia = moro2_de(&chave_de_sondagem(0x3D), agora_unix(), Vec::new()).marca;
    match pedir(chamador, destino, &quem3(&vazia), |d| {
        resposta3_de(d, &vazia)
    }) {
        Some(Resposta3::Ninguem { .. }) => {}
        outra => {
            return EstadoDoEnc3::Torto(format!(
                "o QUEM3 de uma marca que ninguém registrou não disse NINGUEM: {outra:?}"
            ))
        }
    }

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |desde| desde.as_nanos());
    let Some(credencial) = Credencial::nova(&format!("{nanos:032x}")) else {
        return EstadoDoEnc3::Torto("credencial de sondagem inválida".to_owned());
    };
    let pedido = chama(&Chama {
        servidor: servidor.clone(),
        escuta: escuta.clone(),
        credencial,
        candidatos: Vec::new(),
    });
    for _ in 0..TENTATIVAS {
        if chamador.send_to(&pedido, destino).is_err() {
            break;
        }
        let Some(chamado) = receber_ate(
            &anfitriao,
            destino,
            Instant::now() + ESPERA_POR_TENTATIVA,
            |d| match ler_resposta3(d) {
                Some(Resposta3::Chamado(chamado)) if chamado.escuta == escuta => Some(chamado),
                _ => None,
            },
        ) else {
            continue;
        };
        let resposta = atende(&Atende {
            escuta: escuta.clone(),
            id: chamado.id,
            endereco: registrado,
            dicas: Vec::new(),
        });
        if anfitriao.send_to(&resposta, destino).is_err() {
            break;
        }
        let prazo = Instant::now() + seele_encontro::enc3::PRAZO_DA_CHAMADA + ESPERA_POR_TENTATIVA;
        return match receber_ate(chamador, destino, prazo, |d| resposta3_de(d, &servidor)) {
            Some(Resposta3::Aqui { endereco, .. }) if endereco == registrado => EstadoDoEnc3::Fala,
            outra => EstadoDoEnc3::Torto(format!(
                "a escuta atendeu o CHAMADO, e quem ligou não ouviu o AQUI do servidor ({registrado}): {outra:?}"
            )),
        };
    }
    EstadoDoEnc3::Torto(
        "o CHAMA de quem liga não chegou como CHAMADO à escuta registrada, em três tentativas"
            .to_owned(),
    )
}

/// O cookie mágico da RFC 5389.
const COOKIE_DO_STUN: u32 = 0x2112_A442;

/// Um Binding Request (RFC 8489) com o número de transação deste processo e desta hora.
fn binding_request() -> ([u8; 20], [u8; 12]) {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |desde| desde.as_nanos());
    let mut transacao = [0_u8; 12];
    for (destino, byte) in transacao.iter_mut().zip(
        nanos
            .to_le_bytes()
            .into_iter()
            .chain(std::process::id().to_le_bytes()),
    ) {
        *destino = byte;
    }
    let mut pedido = [0_u8; 20];
    // O tipo (Binding Request), o comprimento 0, o cookie mágico e a transação, nessa ordem.
    let campos = 1_u16
        .to_be_bytes()
        .into_iter()
        .chain(0_u16.to_be_bytes())
        .chain(COOKIE_DO_STUN.to_be_bytes())
        .chain(transacao);
    for (destino, byte) in pedido.iter_mut().zip(campos) {
        *destino = byte;
    }
    (pedido, transacao)
}

/// O `XOR-MAPPED-ADDRESS` de um Binding Success Response a esta transação, ou por que não.
fn ler_binding_success(resposta: &[u8], transacao: &[u8; 12]) -> Result<SocketAddr, String> {
    let campo = |de: usize, ate: usize| resposta.get(de..ate).ok_or("resposta curta demais");
    let tipo = u16::from_be_bytes(campo(0, 2)?.try_into().map_err(|_| "tipo")?);
    let comprimento = usize::from(u16::from_be_bytes(
        campo(2, 4)?.try_into().map_err(|_| "tamanho")?,
    ));
    if tipo != 0x0101 {
        return Err(format!(
            "respondeu o tipo {tipo:#06x}, e não o Binding Success (0x0101)"
        ));
    }
    if campo(4, 8)? != COOKIE_DO_STUN.to_be_bytes() || campo(8, 20)? != transacao.as_slice() {
        return Err("respondeu com outro cookie ou outra transação".to_owned());
    }
    let mut atributos = campo(20, 20 + comprimento)?;
    while atributos.len() >= 4 {
        let [n0, n1, t0, t1] = *atributos.first_chunk::<4>().ok_or("atributo curto")?;
        let nome = u16::from_be_bytes([n0, n1]);
        let tamanho = usize::from(u16::from_be_bytes([t0, t1]));
        let valor = atributos.get(4..4 + tamanho).ok_or("atributo cortado")?;
        if nome == 0x0020 {
            let familia = *valor.get(1).ok_or("XOR-MAPPED-ADDRESS curto")?;
            let porta =
                u16::from_be_bytes([*valor.get(2).ok_or("curto")?, *valor.get(3).ok_or("curto")?])
                    ^ 0x2112;
            let mut chave = COOKIE_DO_STUN.to_be_bytes().to_vec();
            chave.extend_from_slice(transacao);
            let ip: std::net::IpAddr = match familia {
                1 => {
                    let mut bytes: [u8; 4] = valor
                        .get(4..8)
                        .ok_or("IPv4 curto")?
                        .try_into()
                        .map_err(|_| "IPv4")?;
                    bytes.iter_mut().zip(&chave).for_each(|(b, k)| *b ^= k);
                    bytes.into()
                }
                2 => {
                    let mut bytes: [u8; 16] = valor
                        .get(4..20)
                        .ok_or("IPv6 curto")?
                        .try_into()
                        .map_err(|_| "IPv6")?;
                    bytes.iter_mut().zip(&chave).for_each(|(b, k)| *b ^= k);
                    bytes.into()
                }
                outra => return Err(format!("família {outra} no XOR-MAPPED-ADDRESS")),
            };
            return Ok(SocketAddr::new(ip, porta));
        }
        let passo = 4 + tamanho.div_ceil(4) * 4;
        atributos = atributos.get(passo..).unwrap_or_default();
    }
    Err("o Binding Success veio sem XOR-MAPPED-ADDRESS".to_owned())
}

/// O STUN Binding, na mesma porta UDP do ponto, como o navegador o manda.
fn sondar_stun(socket: &UdpSocket, destino: SocketAddr) -> Result<SocketAddr, String> {
    let (pedido, transacao) = binding_request();
    // Só o que parece STUN e é desta transação (um atrasado do ENC/3 é pulado).
    pedir(socket, destino, &pedido, |dados| {
        (dados.first().is_some_and(|primeiro| *primeiro <= 3)
            && dados.get(8..20) == Some(transacao.as_slice()))
        .then(|| ler_binding_success(dados, &transacao))
    })
    .unwrap_or_else(|| {
        Err("mudo — o Binding Request não teve resposta em três tentativas".to_owned())
    })
}

/// O corpo de um `POST /sinal/1` que não acha ninguém: as marcas e a credencial são novas a
/// cada rodada, e o ponto responde `NINGUEM` sem chamar escuta nenhuma.
fn corpo_do_sinal() -> Option<String> {
    let servidor = moro2_de(&chave_de_sondagem(0x51), agora_unix(), Vec::new()).marca;
    let escuta = moro2_de(&chave_de_sondagem(0x52), agora_unix(), Vec::new()).marca;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |desde| desde.as_nanos());
    let credencial = Credencial::nova(&format!("{nanos:032x}{:08x}", std::process::id()))?;
    Some(pedido_do_sinal(&Chama {
        servidor,
        escuta,
        credencial,
        candidatos: Vec::new(),
    }))
}

/// A resposta do sinal a [`corpo_do_sinal`] tem de ser a linha `NINGUEM` do protocolo. Um `200`
/// de uma página qualquer (a SPA do Pages, um proxy que não repassa) não passa daqui.
fn conferir_resposta_do_sinal(corpo: &str) -> Result<(), String> {
    match ler_resposta_do_sinal(corpo.trim_end_matches(['\r', '\n'])) {
        Some(RespostaDoSinal::Ninguem) => Ok(()),
        Some(RespostaDoSinal::Recusa { motivo, agora }) => Err(format!(
            "o ponto recusou ({}; relógio dele {agora})",
            motivo.texto()
        )),
        Some(outra) => Err(format!("a uma marca nova, respondeu {outra:?}")),
        None => {
            let comeco: String = corpo.chars().take(80).collect();
            Err(format!("o corpo não é uma linha SEELE-SINAL/1: «{comeco}»"))
        }
    }
}

#[cfg(test)]
mod testes {
    #![allow(
        clippy::expect_used,
        clippy::unwrap_used,
        reason = "num teste, o pânico é o relatório"
    )]

    use super::*;
    use seele_proto::encontro::TAMANHO;

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

    /// Um ponto de encontro de hoje, a v0.15.1: fala o ENC/1 inteiro, com o
    /// quarto, e nunca ouviu falar do ENC/2. Lê num balde de 96 bytes, como o
    /// binário de antes, e um datagrama do ENC/2 chega truncado e não forma
    /// pedido nenhum.
    fn ponto_sem_enc2() -> SocketAddr {
        use seele_proto::encontro::{analisar, aqui, Pedido};
        let socket = UdpSocket::bind("127.0.0.1:0").expect("o loopback não abriu");
        let endereco = socket.local_addr().expect("sem endereço");
        std::thread::spawn(move || {
            let mut balde = [0_u8; TAMANHO];
            let mut moradores = std::collections::HashMap::new();
            loop {
                let Ok((lidos, de)) = socket.recv_from(&mut balde) else {
                    return;
                };
                let resposta = match analisar(balde.get(..lidos).unwrap_or_default()) {
                    Some(Pedido::Onde { marca }) => Some(aqui(&marca, de)),
                    Some(Pedido::Moro { marca }) => {
                        moradores.insert(marca.texto().to_owned(), de);
                        Some(aqui(&marca, de))
                    }
                    Some(Pedido::Quem { marca }) => {
                        moradores.get(marca.texto()).map(|onde| aqui(&marca, *onde))
                    }
                    Some(Pedido::Leve { .. }) | None => None,
                };
                if let Some(resposta) = resposta {
                    let _ = socket.send_to(&resposta, de);
                }
            }
        });
        endereco
    }

    /// Um ponto de encontro com o ENC/2, o binário de agora.
    fn ponto_de_hoje() -> SocketAddr {
        let ponto = seele_encontro::Ponto::abrir(
            "127.0.0.1:0".parse().unwrap(),
            seele_proto::encontro::Vizinhanca::TambemAqui,
        )
        .unwrap();
        let endereco = ponto.endereco().unwrap();
        std::thread::spawn(move || {
            let _ = ponto.servir();
        });
        endereco
    }

    #[test]
    fn um_ponto_com_o_enc2_e_dito_assim_com_o_relogio_dele() {
        let sondagem = perguntar(ponto_de_hoje()).expect("o ONDE tinha que responder");
        assert!(
            matches!(sondagem.quarto, EstadoDoQuarto::Vivo(_)),
            "o quarto da versão 1 deixou de ser visto"
        );
        let EstadoDoEnc2::Fala { relogio_do_ponto } = sondagem.enc2 else {
            panic!(
                "um ponto com o ENC/2 não foi visto como tal: {:?}",
                sondagem.enc2
            );
        };
        assert!(
            relogio_do_ponto.abs() <= 2,
            "o relógio do ponto local veio com {relogio_do_ponto} s de diferença"
        );
    }

    #[test]
    fn um_ponto_sem_o_enc2_e_dito_assim_e_reprova_o_portao() {
        // O caso do ADR 0047, um degrau depois: o ponto em produção responde
        // ao ONDE, ao MORO e ao QUEM, e ninguém o reimplantou com o ENC/2.
        let sondagem = perguntar(ponto_sem_enc2()).expect("o ONDE tinha que responder");
        assert!(
            matches!(sondagem.quarto, EstadoDoQuarto::Vivo(_)),
            "o quarto da versão 1 deixou de ser visto"
        );
        assert_eq!(sondagem.enc2, EstadoDoEnc2::Mudo);
        assert!(
            !passou_no_portao(&[Ok(sondagem)], true),
            "um ponto sem o ENC/2 passou no portão"
        );
    }

    #[test]
    fn o_portao_pede_o_enc2_em_toda_familia_que_responde() {
        let fala = || Sondagem {
            visto: "127.0.0.1:1".parse().unwrap(),
            quarto: EstadoDoQuarto::Velho,
            enc2: EstadoDoEnc2::Fala {
                relogio_do_ponto: 0,
            },
            enc3: EstadoDoEnc3::Fala,
            stun: Ok("127.0.0.1:1".parse().unwrap()),
        };
        let mudo = || Sondagem {
            enc2: EstadoDoEnc2::Mudo,
            ..fala()
        };
        let torto = || Sondagem {
            enc2: EstadoDoEnc2::Torto("relógio".to_owned()),
            ..fala()
        };
        assert!(
            passou_no_portao(&[Ok(fala()), Err("não respondeu em 2s".to_owned())], true),
            "uma família sem rota daqui reprovou o portão"
        );
        assert!(
            !passou_no_portao(&[Ok(fala()), Ok(mudo())], true),
            "uma família muda no ENC/2 passou"
        );
        assert!(
            !passou_no_portao(&[Ok(fala()), Ok(torto())], true),
            "uma família torta no ENC/2 passou"
        );
        assert!(
            !passou_no_portao(&[Err("não respondeu".to_owned())], true),
            "ninguém respondeu, e passou"
        );
        assert!(
            passou_no_portao(&[Ok(mudo())], false),
            "sem a bandeira, o código de saída deixou de ser o de antes"
        );
        assert!(
            !passou_no_portao(&[Err("não respondeu".to_owned())], false),
            "sem a bandeira, ninguém respondeu e passou"
        );
    }

    /// Um ponto de hoje atrás de um intermediário que perde o primeiro
    /// datagrama do ENC/2 que passa por ele. O resto vai e volta.
    fn ponto_que_perde_o_primeiro_do_enc2() -> SocketAddr {
        let ponto = ponto_de_hoje();
        let meio = UdpSocket::bind("127.0.0.1:0").expect("o loopback não abriu");
        let endereco = meio.local_addr().expect("sem endereço");
        std::thread::spawn(move || {
            let mut balde = [0_u8; TAMANHO2 + 1];
            let mut quem_pediu: Option<SocketAddr> = None;
            let mut perdeu = false;
            loop {
                let Ok((lidos, de)) = meio.recv_from(&mut balde) else {
                    return;
                };
                let dados = balde.get(..lidos).unwrap_or_default();
                if de == ponto {
                    if let Some(volta) = quem_pediu {
                        let _ = meio.send_to(dados, volta);
                    }
                    continue;
                }
                quem_pediu = Some(de);
                if lidos == TAMANHO2 && !perdeu {
                    perdeu = true;
                    continue;
                }
                let _ = meio.send_to(dados, ponto);
            }
        });
        endereco
    }

    #[test]
    fn um_pacote_perdido_nao_vira_ponto_anterior_ao_enc2() {
        // Sem repetição, a perda de um pacote virava «mudo», com a frase «é um
        // ponto anterior ao ENC/2»: o portão fechava, e o texto enganava.
        let sondagem =
            perguntar(ponto_que_perde_o_primeiro_do_enc2()).expect("o ONDE tinha que responder");
        assert!(
            matches!(sondagem.enc2, EstadoDoEnc2::Fala { .. }),
            "um pacote perdido virou {:?}",
            sondagem.enc2
        );
    }

    #[test]
    fn o_portao_reprova_um_relogio_de_ponto_fora_de_um_minuto() {
        // A tolerância do carimbo é de 300 s, e o relógio de quem registra
        // erra também: um ponto 240 s adiantado recusa uma sala 61 s atrasada.
        let com_relogio = |segundos| Sondagem {
            visto: "127.0.0.1:1".parse().unwrap(),
            quarto: EstadoDoQuarto::Velho,
            enc2: EstadoDoEnc2::Fala {
                relogio_do_ponto: segundos,
            },
            enc3: EstadoDoEnc3::Fala,
            stun: Ok("127.0.0.1:1".parse().unwrap()),
        };
        assert!(passou_no_portao(&[Ok(com_relogio(60))], true));
        assert!(passou_no_portao(&[Ok(com_relogio(-60))], true));
        assert!(
            !passou_no_portao(&[Ok(com_relogio(61))], true),
            "um relógio de ponto 61 s adiantado passou no portão"
        );
        assert!(
            !passou_no_portao(&[Ok(com_relogio(-61))], true),
            "um relógio de ponto 61 s atrasado passou no portão"
        );
        assert!(
            passou_no_portao(&[Ok(com_relogio(600))], false),
            "sem a bandeira, o relógio passou a reprovar"
        );
    }

    #[test]
    fn a_bandeira_vale_em_qualquer_posicao() {
        let args = |lista: &[&str]| lista.iter().map(ToString::to_string).collect::<Vec<_>>();
        assert_eq!(
            ler_argumentos(&args(&["--exige-enc2", "encontro.seele.app.br"])),
            Some(("encontro.seele.app.br".to_owned(), true, false))
        );
        assert_eq!(
            ler_argumentos(&args(&["encontro.seele.app.br", "--exige-enc2"])),
            Some(("encontro.seele.app.br".to_owned(), true, false))
        );
        assert_eq!(
            ler_argumentos(&args(&["203.0.113.7:8384"])),
            Some(("203.0.113.7:8384".to_owned(), false, false))
        );
        assert_eq!(
            ler_argumentos(&args(&[
                "--exige-enc3",
                "rendzvous.seele.sh",
                "--exige-enc2"
            ])),
            Some(("rendzvous.seele.sh".to_owned(), true, true))
        );
        assert_eq!(ler_argumentos(&args(&["--exige-enc2"])), None);
        assert_eq!(ler_argumentos(&args(&["--outra", "x"])), None);
    }

    #[test]
    fn o_ponto_escrito_sem_porta_e_sondado_na_porta_do_ponto_de_encontro() {
        // O link escreve o ponto sem porta (`enc=encontro.seele.app.br/…`), e o
        // texto ia cru para `to_socket_addrs`, que recusa nome sem porta. Era o
        // defeito 1 do quarto, repetido na ferramenta que mede o portão G1: ela
        // recusava exatamente o ponto que o link carrega.
        let porta = seele_proto::encontro::PORTA_PADRAO;
        assert_eq!(
            enderecos_do_ponto("127.0.0.1"),
            Ok(vec![SocketAddr::from(([127, 0, 0, 1], porta))]),
            "o ponto escrito sem porta, como o link o escreve, não foi sondado na porta do ponto \
             de encontro"
        );
        assert_eq!(
            enderecos_do_ponto("[::1]"),
            Ok(vec![SocketAddr::from((
                std::net::Ipv6Addr::LOCALHOST,
                porta
            ))]),
            "um IPv6 entre colchetes sem porta não foi sondado na porta do ponto de encontro"
        );
        assert_eq!(
            enderecos_do_ponto("127.0.0.1:9000"),
            Ok(vec![SocketAddr::from(([127, 0, 0, 1], 9000))]),
            "a porta escrita deixou de mandar"
        );
        assert!(
            enderecos_do_ponto("tem espaço").is_err(),
            "um texto que não é endereço virou sondagem"
        );
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

    /// Um ponto da 0.16 diante do de hoje: o ENC/1 e o ENC/2 passam, e o ENC/3 e o STUN somem
    /// (um binário da 0.16 lê um e outro como lixo e cala). É o caso do achado P1: o portão do
    /// ENC/2 passava com ele, e a 0.17 saía sem o navegador alcançar ninguém.
    fn ponto_da_016() -> SocketAddr {
        let ponto = ponto_de_hoje();
        let meio = UdpSocket::bind("127.0.0.1:0").expect("o loopback não abriu");
        let endereco = meio.local_addr().expect("sem endereço");
        std::thread::spawn(move || {
            let mut balde = [0_u8; TAMANHO2 + 1];
            let mut quem_pediu: Option<SocketAddr> = None;
            loop {
                let Ok((lidos, de)) = meio.recv_from(&mut balde) else {
                    return;
                };
                let dados = balde.get(..lidos).unwrap_or_default();
                if de == ponto {
                    if let Some(volta) = quem_pediu {
                        let _ = meio.send_to(dados, volta);
                    }
                    continue;
                }
                quem_pediu = Some(de);
                let e_stun = dados.first().is_some_and(|primeiro| *primeiro <= 3);
                if e_stun || dados.starts_with(seele_proto::encontro::MAGIA3.as_bytes()) {
                    continue;
                }
                let _ = meio.send_to(dados, ponto);
            }
        });
        endereco
    }

    #[test]
    fn o_ponto_de_hoje_fala_o_enc3_e_o_stun_e_passa_no_portao_da_017() {
        let destino = ponto_de_hoje();
        let sondagem = perguntar(destino).expect("o ONDE tinha que responder");
        assert_eq!(sondagem.enc3, EstadoDoEnc3::Fala);
        let visto = sondagem.stun.clone().expect("o STUN tinha que responder");
        assert_eq!(
            visto, sondagem.visto,
            "o STUN e o ONDE viram endereços diferentes"
        );
        assert!(fala_a_017(&[Ok(sondagem)]));
    }

    #[test]
    fn um_ponto_da_016_passa_no_enc2_e_reprova_o_portao_da_017() {
        let sondagem = perguntar(ponto_da_016()).expect("o ONDE tinha que responder");
        assert!(
            matches!(sondagem.enc2, EstadoDoEnc2::Fala { .. }),
            "o meio de teste estragou o ENC/2: {:?}",
            sondagem.enc2
        );
        assert_eq!(sondagem.enc3, EstadoDoEnc3::Mudo);
        assert!(sondagem.stun.is_err());
        let resultados = [Ok(sondagem)];
        assert!(
            passou_no_portao(&resultados, true),
            "o portão do ENC/2 mudou"
        );
        assert!(
            !fala_a_017(&resultados),
            "um ponto da 0.16 passou no portão da 0.17"
        );
    }

    #[test]
    fn o_portao_da_017_pede_as_duas_coisas_em_toda_familia_que_responde() {
        let fala = || Sondagem {
            visto: "127.0.0.1:1".parse().unwrap(),
            quarto: EstadoDoQuarto::Velho,
            enc2: EstadoDoEnc2::Fala {
                relogio_do_ponto: 0,
            },
            enc3: EstadoDoEnc3::Fala,
            stun: Ok("127.0.0.1:1".parse().unwrap()),
        };
        let sem_stun = || Sondagem {
            stun: Err("mudo".to_owned()),
            ..fala()
        };
        let sem_enc3 = || Sondagem {
            enc3: EstadoDoEnc3::Mudo,
            ..fala()
        };
        assert!(fala_a_017(&[Ok(fala()), Err("sem rota".to_owned())]));
        assert!(!fala_a_017(&[Ok(fala()), Ok(sem_stun())]));
        assert!(!fala_a_017(&[Ok(sem_enc3()), Ok(fala())]));
        assert!(!fala_a_017(&[Err("sem rota".to_owned())]));
    }

    #[test]
    fn o_binding_success_e_lido_pelo_responder_do_ponto_nas_duas_familias() {
        for origem in ["203.0.113.9:40000", "[2001:db8::9]:40001"] {
            let origem: SocketAddr = origem.parse().unwrap();
            let (pedido, transacao) = binding_request();
            let resposta = seele_encontro::stun::responder(&pedido, origem)
                .expect("o ponto tinha que responder o Binding Request");
            assert_eq!(ler_binding_success(&resposta, &transacao), Ok(origem));
            let mut outra = transacao;
            outra[0] ^= 1;
            assert!(
                ler_binding_success(&resposta, &outra).is_err(),
                "aceitou outra transação"
            );
        }
    }

    /// Um `POST /sinal/1` em HTTP cru no loopback, como o proxy TLS o repassa. Devolve os
    /// cabeçalhos e o corpo.
    fn postar_sinal(sinal: SocketAddr, origem: &str, corpo: &str) -> (String, String) {
        use std::io::{Read, Write};
        let mut conexao = std::net::TcpStream::connect(sinal).expect("o sinal não aceitou");
        write!(
            conexao,
            "POST /sinal/1 HTTP/1.1\r\nHost: rendzvous.seele.sh\r\nOrigin: {origem}\r\n\
             Content-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corpo}",
            corpo.len()
        )
        .unwrap();
        let mut tudo = String::new();
        conexao.read_to_string(&mut tudo).unwrap();
        let (cabecalhos, corpo) = tudo.split_once("\r\n\r\n").unwrap_or((&tudo, ""));
        (cabecalhos.to_owned(), corpo.to_owned())
    }

    #[test]
    fn o_sinal_responde_ninguem_e_o_cors_so_da_origem_do_produto() {
        let ponto = seele_encontro::Ponto::abrir(
            "127.0.0.1:0".parse().unwrap(),
            seele_proto::encontro::Vizinhanca::TambemAqui,
        )
        .unwrap();
        let sinal = seele_encontro::sinal::Sinal::abrir(
            "127.0.0.1:0".parse().unwrap(),
            ponto.quarto(),
            seele_proto::encontro::Vizinhanca::TambemAqui,
            vec![seele_encontro::sinal::origem_do_produto()],
        )
        .unwrap();
        let escuta = sinal.endereco().unwrap();
        std::thread::spawn(move || {
            let _ = sinal.servir();
        });
        // A origem do web app, lida do lugar dela (`seele_proto::uri`), e não escrita aqui.
        let origem = format!("https://{}", seele_proto::uri::ORIGEM_DO_NAVEGADOR);
        assert_eq!(seele_encontro::sinal::origem_do_produto(), origem);

        let (cabecalhos, corpo) = postar_sinal(escuta, &origem, &corpo_do_sinal().unwrap());
        assert!(cabecalhos.starts_with("HTTP/1.1 200"), "{cabecalhos}");
        let permitida = format!("Access-Control-Allow-Origin: {origem}");
        assert!(
            cabecalhos
                .lines()
                .any(|linha| linha.eq_ignore_ascii_case(&permitida)),
            "sem o CORS da origem do produto: {cabecalhos}"
        );
        assert_eq!(conferir_resposta_do_sinal(&corpo), Ok(()));

        let (cabecalhos, _) = postar_sinal(
            escuta,
            "https://exemplo.invalid",
            &corpo_do_sinal().unwrap(),
        );
        assert!(
            !cabecalhos
                .to_ascii_lowercase()
                .contains("access-control-allow-origin"),
            "outra origem pôde ler a resposta: {cabecalhos}"
        );
    }

    #[test]
    fn uma_pagina_qualquer_com_200_nao_passa_por_resposta_do_sinal() {
        // O Pages devolve o index.html da SPA com 200 para qualquer caminho: um proxy que não
        // repassa o /sinal/1 parecia de pé por um código de status.
        assert!(
            conferir_resposta_do_sinal("<!doctype html><html><body>SEELE</body></html>").is_err()
        );
        assert!(conferir_resposta_do_sinal("").is_err());
        assert!(conferir_resposta_do_sinal("SEELE-SINAL/1 CALADO").is_err());
        assert!(conferir_resposta_do_sinal("SEELE-SINAL/1 NINGUEM\n").is_ok());
    }
}
