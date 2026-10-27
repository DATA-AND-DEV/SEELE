//! O anfitrião na configuração padrão do produto: `[::]` em pilha dupla, com o
//! `IPV6_V6ONLY` escrito à mão como `alcance::abrir_escuta` faz. Discado por
//! IPv4 (que chega como `::ffff:a.b.c.d`), por `::1`, pelo IP de LAN da máquina
//! e pelo IPv6 global, quando existem. Nada aqui mexe em rede ou firewall: só
//! sockets locais.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    missing_docs
)]

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use seele_enlace::{CertificadoDtls, ConfigDoAnfitriao, Discagem, Pino, Ponta};

/// O socket do anfitrião como o produto o abre: `[::]:0`, `V6ONLY = false`.
fn socket_de_pilha_dupla() -> std::net::UdpSocket {
    let socket = socket2::Socket::new(
        socket2::Domain::IPV6,
        socket2::Type::DGRAM,
        Some(socket2::Protocol::UDP),
    )
    .unwrap();
    socket.set_only_v6(false).unwrap();
    socket
        .bind(&SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0)).into())
        .unwrap();
    socket.into()
}

/// O IP que o sistema usaria para sair rumo a `destino`, sem mandar nada.
fn ip_de_saida(destino: SocketAddr) -> Option<IpAddr> {
    let quem = if destino.is_ipv4() {
        SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0))
    } else {
        SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0))
    };
    let s = std::net::UdpSocket::bind(quem).ok()?;
    s.connect(destino).ok()?;
    Some(s.local_addr().ok()?.ip())
}

async fn anfitriao() -> (Ponta, CertificadoDtls) {
    let certificado = CertificadoDtls::efemero().unwrap();
    let ponta = Ponta::de_socket(
        socket_de_pilha_dupla(),
        Some(ConfigDoAnfitriao::novo(certificado.clone())),
    )
    .unwrap();
    (ponta, certificado)
}

/// Disca `ip:porta-do-anfitrião` de uma ponta de cliente ligada em `local`, e
/// confere o eco do ctl e da voz e os endereços canônicos dos dois lados.
async fn discar_por(ip: IpAddr, cliente: Ponta) {
    let (host, certificado) = anfitriao().await;
    let alvo = SocketAddr::new(ip, host.local_addr().port());
    let mut discagem = Discagem::nova(vec![alvo], Pino::Impressao(certificado.impressao()));
    discagem.prazo = Duration::from_secs(5);
    let (a, b) = tokio::join!(cliente.discar(discagem), async {
        tokio::time::timeout(Duration::from_secs(6), host.aceitar()).await
    });
    let a = a.unwrap_or_else(|e| panic!("discar {alvo} falhou: {e}"));
    let b = b.unwrap().unwrap();
    // Os endereços saem canônicos: um IPv4 nunca aparece como ::ffff:a.b.c.d.
    assert_eq!(a.remote_address(), alvo, "o lado de quem liga");
    let visto_pelo_host = b.remote_address();
    assert_eq!(
        visto_pelo_host.ip(),
        ip_canonico_do_cliente(ip),
        "{visto_pelo_host}"
    );
    assert!(
        !matches!(visto_pelo_host, SocketAddr::V6(v6) if v6.ip().to_ipv4_mapped().is_some()),
        "IPv4 mapeado vazou: {visto_pelo_host}"
    );
    let (mut ae, _ar) = a.controle().unwrap();
    let (_be, mut br) = b.controle().unwrap();
    ae.write_all(b"pilha").await.unwrap();
    let mut buf = [0_u8; 5];
    br.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"pilha");
    a.send_datagram(vec![1; 80]).unwrap();
    let voz = tokio::time::timeout(Duration::from_secs(3), b.read_datagram())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(voz.len(), 80);
}

/// Em loopback e na mesma máquina, o par vê o cliente pelo mesmo IP que discou.
fn ip_canonico_do_cliente(alvo: IpAddr) -> IpAddr {
    alvo
}

#[tokio::test]
async fn pilha_dupla_discada_por_ipv4_de_loopback() {
    let cliente = Ponta::abrir(SocketAddr::from(([127, 0, 0, 1], 0)), None)
        .await
        .unwrap();
    discar_por(IpAddr::V4(Ipv4Addr::LOCALHOST), cliente).await;
}

#[tokio::test]
async fn pilha_dupla_discada_por_ipv4_de_um_cliente_tambem_de_pilha_dupla() {
    // O caso do app: o cliente em `[::]:0` também, mandando para um IPv4.
    let cliente = Ponta::abrir_cliente().await.unwrap();
    discar_por(IpAddr::V4(Ipv4Addr::LOCALHOST), cliente).await;
}

#[tokio::test]
async fn pilha_dupla_discada_por_ipv6_de_loopback() {
    let cliente = Ponta::abrir_cliente().await.unwrap();
    discar_por(IpAddr::V6(Ipv6Addr::LOCALHOST), cliente).await;
}

#[tokio::test]
async fn pilha_dupla_discada_pelo_ip_de_lan() {
    // A rota para um endereço de documentação dá o IP da interface de saída,
    // sem mandar pacote nenhum. Sem rede, o teste diz que não mediu.
    let Some(ip) = ip_de_saida("192.0.2.1:9".parse().unwrap()) else {
        eprintln!("NÃO MEDIDO: a máquina não tem rota IPv4 de saída");
        return;
    };
    if ip.is_loopback() || ip.is_unspecified() {
        eprintln!("NÃO MEDIDO: sem IPv4 de LAN ({ip})");
        return;
    }
    eprintln!("MEDIDO: pilha dupla discada pelo IPv4 de LAN {ip}");
    let cliente = Ponta::abrir_cliente().await.unwrap();
    discar_por(ip, cliente).await;
}

#[tokio::test]
async fn pilha_dupla_discada_pelo_ipv6_global() {
    let Some(ip) = ip_de_saida("[2001:db8::1]:9".parse().unwrap()) else {
        eprintln!("NÃO MEDIDO: a máquina não tem rota IPv6 de saída");
        return;
    };
    let IpAddr::V6(seis) = ip else {
        return;
    };
    // Global: nem loopback, nem link-local (fe80::/10), nem ULA (fc00::/7).
    let segmentos = seis.segments();
    let link_local = segmentos[0] & 0xffc0 == 0xfe80;
    let ula = segmentos[0] & 0xfe00 == 0xfc00;
    if seis.is_loopback() || link_local || ula || seis.is_unspecified() {
        eprintln!("NÃO MEDIDO: sem IPv6 global ({seis})");
        return;
    }
    eprintln!("MEDIDO: pilha dupla discada pelo IPv6 global {seis}");
    let cliente = Ponta::abrir_cliente().await.unwrap();
    discar_por(ip, cliente).await;
}

#[tokio::test]
async fn um_anfitriao_de_pilha_dupla_atende_v4_e_v6_ao_mesmo_tempo() {
    let (host, certificado) = anfitriao().await;
    let porta = host.local_addr().port();
    let cliente = Ponta::abrir_cliente().await.unwrap();
    let v4 = Discagem::nova(
        vec![SocketAddr::from((Ipv4Addr::LOCALHOST, porta))],
        Pino::Impressao(certificado.impressao()),
    );
    let v6 = Discagem::nova(
        vec![SocketAddr::from((Ipv6Addr::LOCALHOST, porta))],
        Pino::Impressao(certificado.impressao()),
    );
    let (a, b, c, d) = tokio::time::timeout(Duration::from_secs(15), async {
        tokio::join!(
            cliente.discar(v4),
            cliente.discar(v6),
            host.aceitar(),
            host.aceitar()
        )
    })
    .await
    .expect("as duas discagens não terminaram");
    let (a, b) = (a.unwrap(), b.unwrap());
    let (c, d) = (c.unwrap(), d.unwrap());
    let mut vistos = [c.remote_address().ip(), d.remote_address().ip()];
    vistos.sort();
    assert_eq!(
        vistos,
        [
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V6(Ipv6Addr::LOCALHOST)
        ]
    );
    assert!(a.acabou().is_none() && b.acabou().is_none());
}

/// Pedido do B (BLOQUEANTE 2): logo que o `discar` volta, `remote_address()` é o
/// candidato que venceu, e não o primeiro alvo da lista.
#[tokio::test]
async fn o_endereco_do_par_e_o_candidato_que_venceu_assim_que_o_discar_volta() {
    let (host, certificado) = anfitriao().await;
    let porta = host.local_addr().port();
    // Dois alvos antes do certo: um socket que nunca responde, e uma porta fechada.
    let mudo = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let fechada = std::net::UdpSocket::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let certo = SocketAddr::from((Ipv4Addr::LOCALHOST, porta));
    let cliente = Ponta::abrir_cliente().await.unwrap();
    let discagem = Discagem::nova(
        vec![mudo.local_addr().unwrap(), fechada, certo],
        Pino::Impressao(certificado.impressao()),
    );
    let (a, _b) = tokio::join!(cliente.discar(discagem), host.aceitar());
    let a = a.unwrap();
    assert_eq!(
        a.remote_address(),
        certo,
        "o primeiro alvo vazou como endereço do par"
    );
}

/// R4 na subida: a ponta pede o buffer de recepção grande ao sistema. Sem ele, o
/// navegador mandando 4 canais de volume perdia 1–5 % da voz (medido em 24/10,
/// `docs/evidencias/r4-subida-2026-10-24.md`). O teto é do sistema, então o teste
/// compara com o que o mesmo sistema dá a um socket cru que pede o mesmo número.
#[tokio::test]
async fn a_ponta_pede_o_buffer_de_recepcao_grande() {
    let cru = socket2::Socket::new(socket2::Domain::IPV4, socket2::Type::DGRAM, None).unwrap();
    let padrao = cru.recv_buffer_size().unwrap();
    cru.set_recv_buffer_size(seele_enlace::BUFFER_DE_RECEPCAO)
        .unwrap();
    let possivel = cru.recv_buffer_size().unwrap();
    let ponta = Ponta::abrir(SocketAddr::from(([127, 0, 0, 1], 0)), None)
        .await
        .unwrap();
    let dado = ponta.buffer_de_recepcao().unwrap();
    assert_eq!(
        dado, possivel,
        "a ponta não pediu o buffer (padrão do sistema: {padrao})"
    );
    eprintln!("buffer de recepção: padrão {padrao}, da ponta {dado}");
}

/// A corrida do core (`enlace::tests::a_recusa_de_um_candidato_perdedor…`): uma
/// ponta só disca quatro alvos, fica com quem respondeu, recusa pelo pino,
/// redisca sem ele, e assim até o verdadeiro, que só começa a responder depois.
#[tokio::test]
async fn rediscar_sem_quem_recusou_chega_ao_verdadeiro() {
    let (impostor, _) = anfitriao().await;
    let (trocado, _) = anfitriao().await;
    let (verdadeiro, certificado) = anfitriao().await;
    let porta = |p: &Ponta| SocketAddr::from((Ipv4Addr::LOCALHOST, p.local_addr().port()));
    let mudo = SocketAddr::from((Ipv4Addr::LOCALHOST, 9));
    // Os dois falsos aceitam e seguram; o verdadeiro só aceita.
    for falso in [impostor.clone(), trocado.clone()] {
        tokio::spawn(async move {
            let mut de_pe = Vec::new();
            while let Some(c) = falso.aceitar().await {
                de_pe.push(c);
            }
        });
    }
    let cliente = Ponta::abrir_cliente().await.unwrap();
    let mut restantes = vec![porta(&impostor), porta(&trocado), mudo, porta(&verdadeiro)];
    let alvo_verdadeiro = porta(&verdadeiro);
    let _aceitar = tokio::spawn(async move { verdadeiro.aceitar().await });
    let mut tentativas = 0;
    let vencedor = loop {
        tentativas += 1;
        assert!(tentativas <= 4, "a corrida não terminou: {restantes:?}");
        let mut discagem = Discagem::nova(restantes.clone(), Pino::Qualquer);
        discagem.prazo = Duration::from_secs(5);
        let conexao = cliente
            .discar(discagem)
            .await
            .unwrap_or_else(|e| panic!("tentativa {tentativas} com {restantes:?}: {e}"));
        let quem = conexao.remote_address();
        if conexao.impressao_dtls_do_par() == certificado.impressao() {
            break quem;
        }
        conexao.close(1, b"pino");
        restantes.retain(|r| *r != quem);
    };
    assert_eq!(vencedor, alvo_verdadeiro);
}

/// O teste que o B pediu (BLOQUEANTE 3): `[A, H]` com A atendendo e sendo fechado,
/// depois `[H]` da mesma ponta. O par pendente que o check da 1ª discagem criou em H
/// é aposentado pelo aperto novo, e o DTLS novo não cai nele.
#[tokio::test]
async fn a_rediscagem_da_mesma_porta_nao_cai_no_par_pendente_velho() {
    let (a, _) = anfitriao().await;
    let (h, certificado_de_h) = anfitriao().await;
    let alvo = |p: &Ponta| SocketAddr::from((Ipv4Addr::LOCALHOST, p.local_addr().port()));
    let (alvo_a, alvo_h) = (alvo(&a), alvo(&h));
    {
        let a = a.clone();
        tokio::spawn(async move {
            let mut de_pe = Vec::new();
            while let Some(c) = a.aceitar().await {
                de_pe.push(c);
            }
        });
    }
    let cliente = Ponta::abrir_cliente().await.unwrap();
    let primeira = cliente
        .discar(Discagem::nova(vec![alvo_a, alvo_h], Pino::Qualquer))
        .await
        .unwrap();
    assert_eq!(
        primeira.remote_address(),
        alvo_a,
        "A não atendeu primeiro: o teste mediria outra coisa"
    );
    primeira.close(1, b"pino");
    // O check que a 1ª discagem mandou a H criou um par pendente lá.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut segunda = Discagem::nova(vec![alvo_h], Pino::Impressao(certificado_de_h.impressao()));
    segunda.prazo = Duration::from_secs(5);
    let (discou, chegou) = tokio::join!(cliente.discar(segunda), async {
        tokio::time::timeout(Duration::from_secs(6), h.aceitar()).await
    });
    let discou = discou.expect("a rediscagem pela mesma porta não fechou o aperto");
    assert_eq!(discou.remote_address(), alvo_h);
    assert!(chegou.unwrap().is_some());
    let contadores = h.contadores();
    assert!(
        contadores.pares_criados < 2 || contadores.pendentes_superados >= 1,
        "H teve um par pendente da 1ª discagem e ele não foi aposentado: {contadores:?}"
    );
}
