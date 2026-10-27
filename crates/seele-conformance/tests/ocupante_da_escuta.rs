//! Quem ocupa a marca da ESCUTA no quarto não recebe quem chega.
//!
//! O I1 da revisão final do Plano 1, na linha 2.
//!
//! # O que mudou na 0.17
//!
//! Até a 0.16 o risco morava em quem chega: a escuta que o quarto dava virava
//! o aviso do `LEVE`, e o ponto repassava a esse aviso um `AQUI` com o endereço
//! público de quem chegava, antes de qualquer TLS. A regra que fechava o caso
//! (`bilhete_desta_volta`) era do cliente: a escuta só valia no IP do servidor
//! da mesma resposta.
//!
//! A linha 2 não manda `LEVE`. Quem chega manda `CHAMA` **ao ponto**, e quem
//! decide a quem repassar é o ponto (`enc3.md`): ele procura as duas marcas
//! (servidor e escuta) e só manda o `CHAMADO` — que leva o endereço que ele
//! viu de quem chega e o `U` desta tentativa — se as duas estiverem lá **e no
//! mesmo IP**. Senão, `NINGUEM`. A ameaça é a mesma (a marca da escuta está em
//! todo link, e o `MORA3` de um servidor não é assinado); a guarda mudou de
//! lado, e é o ponto de verdade (`seele_encontro::Ponto`) que este arquivo
//! mede agora.
//!
//! # Por que o outro IP é o `::1`
//!
//! O macOS só tem o `127.0.0.1` no loopback IPv4. O outro IP desta máquina é
//! o `::1`, e o ponto abre uma escuta por família dividindo o quarto, como em
//! produção.
//!
//! # O que fica de resíduo, dito
//!
//! Quem ocupa a escuta **no mesmo IP** do servidor (atrás do mesmo NAT) recebe
//! o `CHAMADO`, como recebia o `AQUI` até a 0.16 — é o controle abaixo. E quem
//! ocupa **as duas** marcas recebe tudo e atende em nome do anfitrião; o que
//! impede isso de virar sessão com um impostor é a impressão do DTLS conferida
//! antes do `Hello`, e não o ponto.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::{Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use seele_proto::encontro::{
    chama, ler_resposta3, mora3, Chama, Credencial, Marca, Marcas, Resposta3, Vizinhanca,
};

mod vaga;

/// Quanto se espera um `CHAMADO` ou a resposta do ponto. O ponto espera o
/// `ATENDE` por 3 s antes do `CALADO`.
const ESPERA: Duration = Duration::from_secs(5);

/// A impressão de um servidor X qualquer: as marcas saem dela.
const IMPRESSAO_DE_X: &str = "5e1e0a0b0c0d0e0f00112233445566778899aabbccddeeff0011223344556677";

/// Um ponto de verdade com as duas escutas, IPv4 e IPv6, dividindo o quarto.
fn subir_o_ponto() -> (SocketAddr, SocketAddr) {
    let quarto = Arc::new(seele_encontro::Quarto::novo());
    let quatro = seele_encontro::Ponto::abrir_com_quarto(
        SocketAddr::from(([127, 0, 0, 1], 0)),
        Vizinhanca::TambemAqui,
        Arc::clone(&quarto),
    )
    .expect("a escuta IPv4 do ponto abre");
    let seis = seele_encontro::Ponto::abrir_com_quarto(
        SocketAddr::from((Ipv6Addr::LOCALHOST, 0)),
        Vizinhanca::TambemAqui,
        Arc::clone(&quarto),
    )
    .expect(
        "a escuta IPv6 do ponto abre em `::1`: este teste precisa do loopback IPv6, que é o \
         outro IP desta máquina",
    );
    let (em_quatro, em_seis) = (quatro.endereco().unwrap(), seis.endereco().unwrap());
    std::thread::spawn(move || {
        let _ = quatro.servir();
    });
    std::thread::spawn(move || {
        let _ = seis.servir();
    });
    (em_quatro, em_seis)
}

/// Registra `marca` pelo `MORA3`, com o cookie que o ponto pede.
async fn morar(socket: &tokio::net::UdpSocket, ponto: SocketAddr, marca: &Marca) {
    let mut cookie = None;
    let mut balde = [0_u8; 1500];
    for _ in 0..3 {
        socket
            .send_to(&mora3(marca, cookie.as_ref()), ponto)
            .await
            .unwrap();
        let (lidos, _) = tokio::time::timeout(Duration::from_secs(2), socket.recv_from(&mut balde))
            .await
            .expect("o ponto não respondeu ao MORA3")
            .unwrap();
        match ler_resposta3(balde.get(..lidos).unwrap_or_default()) {
            Some(Resposta3::Desafio { cookie: novo, .. }) => cookie = Some(novo),
            Some(Resposta3::Morou { .. }) => return,
            outra => panic!("o ponto não aceitou o MORA3 de {marca}: {outra:?}"),
        }
    }
    panic!("o ponto pediu o cookie três vezes");
}

/// O primeiro `CHAMADO` para `escuta` que chega a `socket`, se chegar.
async fn chamado_para(socket: &tokio::net::UdpSocket, escuta: &Marca) -> Option<SocketAddr> {
    let mut balde = [0_u8; 1500];
    let ate = tokio::time::Instant::now() + ESPERA;
    loop {
        let Ok(Ok((lidos, _))) = tokio::time::timeout_at(ate, socket.recv_from(&mut balde)).await
        else {
            return None;
        };
        if let Some(Resposta3::Chamado(chamado)) =
            ler_resposta3(balde.get(..lidos).unwrap_or_default())
        {
            if chamado.escuta == *escuta {
                return chamado.candidatos.first().copied();
            }
        }
    }
}

/// O que o ponto respondeu a quem chamou, se respondeu.
async fn resposta_a(socket: &tokio::net::UdpSocket, servidor: &Marca) -> Option<Resposta3> {
    let mut balde = [0_u8; 1500];
    let ate = tokio::time::Instant::now() + ESPERA;
    loop {
        let Ok(Ok((lidos, _))) = tokio::time::timeout_at(ate, socket.recv_from(&mut balde)).await
        else {
            return None;
        };
        let Some(resposta) = ler_resposta3(balde.get(..lidos).unwrap_or_default()) else {
            continue;
        };
        let marca = match &resposta {
            Resposta3::Ninguem { marca }
            | Resposta3::Calado { marca }
            | Resposta3::Aqui { marca, .. }
            | Resposta3::Recusa { marca, .. } => marca,
            _ => continue,
        };
        if marca == servidor {
            return Some(resposta);
        }
    }
}

/// O que cada lado viu de uma chamada a X com o ocupante na escuta.
struct Visto {
    /// O endereço de quem chega, se o `CHAMADO` chegou ao ocupante.
    ocupante: Option<SocketAddr>,
    /// O que o ponto respondeu a quem chamou.
    resposta: Option<Resposta3>,
}

/// O servidor de X registra a marca dele pela escuta IPv6 do ponto
/// (`servidor_pelo_ipv6`, o caso que a guarda corta) ou pela IPv4 (o controle:
/// o mesmo IP do ocupante). O ocupante registra a marca da escuta de X, que
/// está em todo link, pela IPv4. Quem chega chama X pelo ponto.
async fn chamar_com_o_ocupante(servidor_pelo_ipv6: bool) -> Visto {
    let (ponto, ponto_seis) = subir_o_ponto();
    let marcas = Marcas::do_servidor(IMPRESSAO_DE_X).expect("a impressão de X forma marcas");

    let ocupante = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    morar(&ocupante, ponto, marcas.escuta()).await;

    let servidor_de_x = if servidor_pelo_ipv6 {
        let socket = tokio::net::UdpSocket::bind((Ipv6Addr::LOCALHOST, 0))
            .await
            .expect("o loopback IPv6 abre");
        morar(&socket, ponto_seis, marcas.servidor()).await;
        socket
    } else {
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        morar(&socket, ponto, marcas.servidor()).await;
        socket
    };

    let quem_chega = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let pedido = chama(&Chama {
        servidor: marcas.servidor().clone(),
        escuta: marcas.escuta().clone(),
        credencial: Credencial::nova("Uabcdefghijklmnopqrstuvwxyz012345").unwrap(),
        candidatos: Vec::new(),
    });
    quem_chega.send_to(&pedido, ponto).await.unwrap();

    let (ocupante_viu, resposta) = tokio::join!(
        chamado_para(&ocupante, marcas.escuta()),
        resposta_a(&quem_chega, marcas.servidor()),
    );
    drop(servidor_de_x);
    Visto {
        ocupante: ocupante_viu,
        resposta,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn quem_ocupa_a_marca_da_escuta_de_outro_ip_nao_recebe_quem_chega() {
    let _vaga = vaga::minha();
    let visto = chamar_com_o_ocupante(true).await;

    assert_eq!(
        visto.ocupante, None,
        "quem ocupou a marca da escuta, noutro IP que o servidor de X, recebeu o CHAMADO com o \
         endereço de quem chega ({:?}): o ponto repassou sem conferir que a escuta e o servidor \
         moram no mesmo IP",
        visto.ocupante
    );
    assert!(
        matches!(visto.resposta, Some(Resposta3::Ninguem { .. })),
        "com a escuta noutro IP, o ponto tinha de dizer NINGUEM a quem chamou, e disse {:?}",
        visto.resposta
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn no_ip_do_servidor_a_escuta_do_quarto_recebe_quem_chega() {
    let _vaga = vaga::minha();
    // O controle, e o resíduo: no IP do servidor, a escuta do quarto recebe o
    // `CHAMADO`. É o anfitrião de verdade (escuta e servidor saem pelo mesmo
    // IP público), ou quem está atrás do mesmo NAT. Sem este lado, o de cima
    // passaria também com um ponto que nunca repassa nada.
    let visto = chamar_com_o_ocupante(false).await;

    let Some(quem_chega) = visto.ocupante else {
        panic!(
            "a escuta do quarto, no IP do servidor, não recebeu o CHAMADO: ou o ponto recusa o \
             anfitrião de verdade, ou este arranjo não faz o CHAMA chegar, e o outro teste deste \
             arquivo passa sem medir nada"
        );
    };
    assert_eq!(
        quem_chega.ip().to_canonical(),
        std::net::IpAddr::from([127, 0, 0, 1]),
        "o CHAMADO não levou o endereço que o ponto viu de quem chega"
    );
    // Ninguém atendeu (o ocupante não manda ATENDE): o ponto diz CALADO.
    assert!(
        matches!(visto.resposta, Some(Resposta3::Calado { .. })),
        "a escuta recebeu e não atendeu, e o ponto disse {:?} em vez de CALADO",
        visto.resposta
    );
}
