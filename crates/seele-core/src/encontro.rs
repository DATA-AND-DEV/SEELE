//! Degrau 4 do ADR 0022, do lado de quem entra.
//!
//! O trabalho pesado é de quem hospeda — ver `alcance::encontro`, no
//! `seele-server`. Deste lado o degrau 4 é **um datagrama**: "ponto de encontro,
//! diga ao anfitrião de onde este pacote veio". O anfitrião recebe aquele
//! endereço, manda alguns pacotes para cá, e o roteador de lá passa a deixar
//! entrar o aperto de mão que vem em seguida.
//!
//! # Por que o socket importa mais que o datagrama
//!
//! O NAT mapeia por porta interna. Se este aviso sair de um socket e o QUIC sair
//! de outro, o anfitrião fura o caminho para a porta errada e o aperto de mão
//! continua batendo numa porta fechada — em quase todo roteador doméstico, que
//! filtra por endereço **e** porta.
//!
//! É por isso que [`bater`] devolve o socket por onde bateu, em vez de só mandar
//! o pacote: quem conecta em seguida tem de conectar por ele. É o mesmo motivo
//! pelo qual o outro lado precisou de um espelho do socket do Dogma.
//!
//! # O que este lado nunca faz
//!
//! **Não lê resposta nenhuma do ponto de encontro.** Nem precisa: os endereços
//! que serão tentados vieram todos do `seele://`, e a impressão digital contra a
//! qual o Dogma é conferido também. Um ponto de encontro hostil consegue não
//! avisar o anfitrião — e é só. Ele não tem por onde mandar ninguém para outro
//! lugar, porque ninguém deste lado escuta o que ele diz.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use seele_proto::encontro::{self, Marca};
use seele_proto::uri::Bilhete;

/// Quanto tempo se gasta batendo antes de desistir e conectar assim mesmo.
///
/// Curto porque **não há o que esperar**: o aviso é de mão única, não há
/// resposta a aguardar, e a única coisa que este prazo cobre é a resolução do
/// nome do ponto de encontro. Um ponto de encontro fora do ar não pode atrasar
/// uma conexão que talvez nem precisasse dele — o primeiro endereço do convite é
/// o da rede de casa, e quem está na sala ao lado entra por ele.
const PRAZO: Duration = Duration::from_millis(600);

/// Quantas vezes o aviso é mandado, e de quanto em quanto tempo.
///
/// Dois, porque um datagrama se perde e este não tem confirmação nenhuma. Não
/// mais que isso: quem espera do outro lado tem uma janela de furos, e gastá-la
/// com repetição nossa seria gastá-la contra nós mesmos.
const AVISOS: u8 = 2;
/// O intervalo entre eles.
const INTERVALO: Duration = Duration::from_millis(80);

/// Bate no ponto de encontro do convite, e devolve o socket por onde bateu.
///
/// `None` quando não deu para bater — nome que não resolve, ponto de encontro
/// inalcançável, convite sem impressão digital. Nesse caso quem chama conecta
/// como sempre conectou, pelos endereços do convite: o degrau 4 é o único que se
/// perde, e ele é o de cima.
///
/// # Por que a impressão digital é obrigatória aqui
///
/// A marca do aviso são os primeiros dígitos dela, e é o que diz ao anfitrião
/// que quem bateu tem o link dele. Sem impressão digital o aviso chegaria com
/// uma marca que o anfitrião não reconhece, e ele o ignoraria — mandar o pacote
/// assim mesmo seria gastar rede para produzir silêncio.
pub async fn bater(
    bilhete: &Bilhete,
    impressao_digital: Option<&str>,
) -> Option<std::net::UdpSocket> {
    let marca = impressao_digital
        .and_then(|impressao| impressao.get(..16))
        .and_then(Marca::nova)?;
    let aviso = bilhete.aviso().ok()?;
    let ponto = tokio::time::timeout(PRAZO, resolver(bilhete))
        .await
        .ok()??;

    let socket = abrir_socket_local()?;
    let socket = tokio::net::UdpSocket::from_std(socket).ok()?;
    let datagrama = encontro::leve(aviso, &marca);
    for tentativa in 0..AVISOS {
        if tentativa > 0 {
            tokio::time::sleep(INTERVALO).await;
        }
        if let Err(erro) = socket.send_to(&datagrama, mapear(ponto, &socket)).await {
            tracing::info!(%erro, %ponto, "não deu para avisar o ponto de encontro");
            return None;
        }
    }
    tracing::info!(%ponto, %aviso, "degrau 4: avisamos o ponto de encontro de que estamos chegando");

    // De volta a um socket da `std`, porque é isso que o quinn adota. E sem
    // esperar por nada: o primeiro endereço do convite é o da rede de casa, e o
    // tempo que ele leva para falhar é tempo de sobra para o furo abrir.
    socket.into_std().ok()
}

/// Onde o ponto de encontro atende.
async fn resolver(bilhete: &Bilhete) -> Option<SocketAddr> {
    let alvo = bilhete.ponto().ok()?;
    tokio::net::lookup_host((alvo.maquina, alvo.porta))
        .await
        .ok()?
        .next()
}

/// Um socket local que alcança as duas famílias, como o do QUIC.
///
/// Mesmo raciocínio de `local_endpoint` em [`crate::client`], e pelo mesmo
/// motivo: um socket IPv4 não manda para destino IPv6 de jeito nenhum, e um
/// socket IPv6 só manda para IPv4 com o `IPV6_V6ONLY` desligado — cujo padrão
/// muda de sistema para sistema (o degrau 2 do ADR 0022 mediu isso e apanhou).
///
/// A diferença é que aqui a opção é escrita à mão em vez de herdada do quinn,
/// porque é este código que abre o socket. Uma máquina sem IPv6 cai para IPv4, e
/// continua exatamente como estava.
fn abrir_socket_local() -> Option<std::net::UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};

    let seis = || -> Option<std::net::UdpSocket> {
        let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP)).ok()?;
        socket.set_only_v6(false).ok()?;
        socket
            .bind(&SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, 0)).into())
            .ok()?;
        Some(socket.into())
    };
    let quatro = || std::net::UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], 0))).ok();

    let socket = seis().or_else(quatro)?;
    socket.set_nonblocking(true).ok()?;
    Some(socket)
}

/// Um destino IPv4 escrito como o socket local o entende.
///
/// Num socket IPv6 de pilha dupla, um endereço IPv4 precisa ir na forma mapeada
/// (`::ffff:a.b.c.d`) — é o que o quinn faz por dentro, e aqui é à mão porque o
/// socket é nosso.
fn mapear(destino: SocketAddr, socket: &tokio::net::UdpSocket) -> SocketAddr {
    let local_e_seis = socket.local_addr().is_ok_and(|local| local.is_ipv6());
    match (local_e_seis, destino.ip()) {
        (true, IpAddr::V4(quatro)) => {
            SocketAddr::new(quatro.to_ipv6_mapped().into(), destino.port())
        }
        _ => destino,
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn bilhete(ponto: &str) -> Bilhete {
        Bilhete::novo(ponto, "45.33.32.156:41234").expect("bilhete de teste")
    }

    const FP: &str = "3cbcfb0212da738f89c156de86eb280adee30fd6b907523b898fedcb2b1de5b9";

    #[tokio::test]
    async fn sem_impressao_digital_nao_se_bate_em_ponto_nenhum() {
        // A marca sai da impressão digital, e sem ela o aviso chegaria com uma
        // etiqueta que o anfitrião não reconhece — rede gasta para produzir
        // silêncio. E é a asserção que garante que nenhum pacote sai daqui por
        // um link que não prometeu identidade nenhuma.
        assert!(bater(&bilhete("192.0.2.1:8384"), None).await.is_none());
        assert!(bater(&bilhete("192.0.2.1:8384"), Some("curto"))
            .await
            .is_none());
    }

    #[tokio::test]
    async fn um_ponto_de_encontro_que_nao_resolve_nao_segura_a_conexao() {
        // O requisito do ADR 0022 deste lado: o degrau 4 não pode virar ponto
        // único de falha. Um nome que não existe custa o que a resolução custa,
        // e nunca mais que o prazo.
        let comeco = std::time::Instant::now();
        let batido = bater(&bilhete("nao-existe-mesmo.invalid:8384"), Some(FP)).await;
        assert!(batido.is_none(), "um nome inexistente devolveu um socket");
        assert!(
            comeco.elapsed() < PRAZO * 2,
            "a conexão ficou presa {:?} num ponto de encontro que não existe",
            comeco.elapsed()
        );
    }

    #[tokio::test]
    async fn bater_devolve_o_socket_por_onde_bateu() {
        // A propriedade que faz o furo funcionar: quem conecta em seguida tem de
        // conectar **por este socket**, ou o anfitrião fura o caminho para a
        // porta errada. O ponto de encontro aqui é um socket qualquer no
        // loopback: nada precisa responder, porque este lado não lê resposta.
        let fingido = std::net::UdpSocket::bind("127.0.0.1:0").expect("loopback");
        let onde = fingido.local_addr().expect("endereço");

        let socket = bater(&bilhete(&onde.to_string()), Some(FP))
            .await
            .expect("bater num ponto de encontro que existe");
        let local = socket.local_addr().expect("o socket não diz onde ligou");
        assert_ne!(
            local.port(),
            0,
            "o socket não ficou ligado em porta nenhuma"
        );

        // E o que chegou lá é um `LEVE` com a marca do convite, apontando para o
        // endereço de avisos do anfitrião.
        fingido
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("prazo");
        let mut balde = [0_u8; encontro::TAMANHO];
        let (lidos, de) = fingido.recv_from(&mut balde).expect("nada chegou ao ponto");
        assert_eq!(
            de.port(),
            local.port(),
            "o aviso saiu de outro socket que não o que vai conectar"
        );
        let Some(seele_proto::encontro::Pedido::Leve { destino, marca }) =
            balde.get(..lidos).and_then(seele_proto::encontro::analisar)
        else {
            panic!("chegou outra coisa ao ponto de encontro");
        };
        assert_eq!(destino.to_string(), "45.33.32.156:41234");
        assert_eq!(marca.texto(), &FP[..16]);
    }
}
