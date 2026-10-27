//! **O degrau 4 da 0.17 inteiro, na mesma máquina, com as peças de verdade**
//! (revisão do Codex; ADR 0063).
//!
//! Sobem um `seele-encontro` real (o `Ponto`, com o STUN e o ENC/3), um
//! `seeled` real que registra `MORA3` nele pelo socket do servidor, e quem liga
//! por uma `Ponta` real do enlace: `CHAMA` → o ponto repassa o `CHAMADO` → o
//! anfitrião fura e responde `ATENDE` → o ponto devolve `AQUI` → quem liga
//! disca o endereço com o `U` do `CHAMA`, confere a impressão no DTLS, passa a
//! prova D5 e troca controle e voz.
//!
//! No mesmo cenário, as recusas e os tetos: marca inexistente (`NINGUEM`),
//! escuta calada (`CALADO`), taxa (`RECUSA`), candidato inválido descartado,
//! o teto de Indications por `CHAMADO`, a impressão errada recusada e a lápide
//! do `U` que falhou.
//!
//! Duas famílias: IPv4 que chega mapeado ao `[::]` de pilha dupla do `seeled`, e
//! `::1`. **NAT real não é medido aqui**: tudo é laço local, sem tradução de
//! endereço nem filtro.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::{Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use ed25519_dalek::{Signer, SigningKey};
use seele_proto::control::{ClientMessage, ServerMessage};
use seele_proto::encontro::{self, Chama, Credencial, Marcas, Resposta3, Vizinhanca};
use seele_proto::ids::Ssrc;
use seele_proto::media::MediaHeader;
use seele_server::alcance::encontro::{abrir, Convocacao, PeloServidor};
use seele_server::alcance::Escuta;
use seele_server::persistence::Location;
use seele_server::transporte::{Conexao, Discagem, FalhaAoDiscar, Pino, Ponta};
use seele_server::{frame, Daemon, ServerConfig};

/// Um ponto de encontro de verdade, numa linha de execução própria.
fn ponto(local: SocketAddr) -> SocketAddr {
    let ponto = seele_encontro::Ponto::abrir(local, Vizinhanca::TambemAqui).expect("o ponto");
    let endereco = ponto.endereco().expect("o endereço do ponto");
    std::thread::spawn(move || {
        let _ = ponto.servir();
    });
    endereco
}

/// Um `seeled` de verdade em `[::]` de pilha dupla, registrado no ponto.
async fn anfitriao(ponto: SocketAddr) -> (Arc<Daemon>, seele_server::alcance::encontro::Encontro) {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0)),
        database: Location::Memory,
        ..ServerConfig::default()
    };
    let daemon = Arc::new(Daemon::bind(config).await.expect("o seeled"));
    let pelo = PeloServidor {
        ponta: daemon.ponta(),
        enc: daemon.mensagens_enc().expect("a caixa do ENC/3"),
    };
    let escuta = Escuta::nova(daemon.local_addr().unwrap().port(), daemon.pilha());
    let convocacao =
        Convocacao::para_servidor(pelo, daemon.fingerprint(), escuta, ponto.to_string())
            .expect("a convocação");
    let encontro = abrir(&convocacao).await.expect("o registro no ponto");
    let rodando = Arc::clone(&daemon);
    tokio::spawn(async move { rodando.run().await });
    (daemon, encontro)
}

fn impressao(hex: &str) -> [u8; 32] {
    let mut bytes = [0_u8; 32];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap();
    }
    bytes
}

fn credencial(u: &str) -> Credencial {
    Credencial::nova(u).expect("um U de 32 alfanuméricos")
}

/// Manda um `CHAMA` pela ponta e espera a resposta do ponto.
async fn chamar(
    ponta: &Ponta,
    caixa: &mut tokio::sync::mpsc::Receiver<(SocketAddr, Vec<u8>)>,
    ponto: SocketAddr,
    marcas: &Marcas,
    u: &str,
) -> Option<Resposta3> {
    let pedido = encontro::chama(&Chama {
        servidor: marcas.servidor().clone(),
        escuta: marcas.escuta().clone(),
        credencial: credencial(u),
        candidatos: Vec::new(),
    });
    ponta.enviar_bruto(ponto, &pedido).expect("o CHAMA saiu");
    let ate = tokio::time::Instant::now() + Duration::from_secs(6);
    while let Ok(Some((de, bytes))) = tokio::time::timeout_at(ate, caixa.recv()).await {
        if de.ip().to_canonical() != ponto.ip().to_canonical() {
            continue;
        }
        if let Some(resposta) = encontro::ler_resposta3(&bytes) {
            return Some(resposta);
        }
    }
    None
}

/// O aperto inteiro, com a prova D5, até o `Session`. Devolve o `ssrc` e a sala.
async fn entrar(
    conexao: &Conexao,
    semente: u8,
) -> (seele_enlace_ctl::Ctl, Ssrc, seele_proto::ids::VoiceRoomId) {
    let (mut envio, mut recebe) = conexao.controle().expect("o ctl");
    let chave = SigningKey::from_bytes(&[semente; 32]);
    frame::write(
        &mut envio,
        &ClientMessage::Hello {
            faixa: seele_proto::FAIXA,
            client: "par-cru".into(),
            nickname: format!("pessoa{semente:03}"),
            public_key: chave.verifying_key().to_bytes().to_vec(),
            join_secret: None,
            material_e2ee: None,
            apis_de_mod: seele_proto::mods::APIS_ACEITAS.to_vec(),
        },
    )
    .await
    .unwrap();
    let ServerMessage::Challenge { nonce } =
        frame::read::<ServerMessage>(&mut recebe).await.unwrap()
    else {
        panic!("sem Challenge");
    };
    let nonce: [u8; 32] = nonce.as_slice().try_into().unwrap();
    let prova = seele_proto::assinatura::prova_de_cliente_2(
        &conexao.impressao_dtls_do_par(),
        &conexao.impressao_dtls_local(),
        &nonce,
    );
    frame::write(
        &mut envio,
        &ClientMessage::Response {
            proof: chave.sign(&prova).to_bytes().to_vec(),
        },
    )
    .await
    .unwrap();
    loop {
        if let ServerMessage::Session {
            ssrc, voice_rooms, ..
        } = frame::read::<ServerMessage>(&mut recebe).await.unwrap()
        {
            let sala = voice_rooms.first().map(|v| v.id).expect("uma sala");
            return (seele_enlace_ctl::Ctl { envio, recebe }, ssrc, sala);
        }
    }
}

/// O `ctl` de uma sessão, segurado inteiro (largar um lado fecha a conexão).
mod seele_enlace_ctl {
    pub(crate) struct Ctl {
        pub(crate) envio: seele_server::transporte::Envio,
        pub(crate) recebe: seele_server::transporte::Recebe,
    }
}

fn quadro_de_voz(ssrc: Ssrc, seq: u16) -> Vec<u8> {
    let cabecalho = MediaHeader {
        ssrc: ssrc.get(),
        seq,
        timestamp: u32::from(seq) * 960,
    };
    let mut fora = vec![0_u8; seele_proto::MAX_DATAGRAM_LEN];
    let tamanho = cabecalho
        .encode_datagram(&[0xF8; 40], &mut fora)
        .expect("cabe");
    fora.truncate(tamanho);
    fora
}

/// O caminho feliz: ENC/3 até o `AQUI`, a discagem com o `U` do `CHAMA`, a
/// impressão conferida, a prova D5, o controle e a voz.
async fn o_caminho_inteiro(local: SocketAddr) {
    let ponto = ponto(local);
    let (daemon, _encontro) = anfitriao(ponto).await;
    let fp = impressao(daemon.fingerprint());
    let marcas = Marcas::do_servidor(daemon.fingerprint()).expect("as marcas");

    let quem_liga = Ponta::abrir(local, None)
        .await
        .expect("a ponta de quem liga");
    let mut caixa = quem_liga.mensagens_enc().expect("a caixa");
    let u = seele_server::transporte::sortear_credencial();
    let resposta = chamar(&quem_liga, &mut caixa, ponto, &marcas, &u).await;
    let Some(Resposta3::Aqui { endereco, .. }) = resposta else {
        panic!("o ponto não devolveu AQUI: {resposta:?}");
    };
    println!("[{local}] AQUI {endereco}");
    let mut discagem = Discagem::nova(vec![endereco], Pino::Impressao(fp));
    discagem.credencial = Some(u);
    let conexao = quem_liga
        .discar(discagem)
        .await
        .expect("a discagem pelo AQUI");
    assert_eq!(conexao.impressao_dtls_do_par(), fp);
    let (mut ctl, ssrc, sala) = entrar(&conexao, 41).await;

    // O controle.
    frame::write(&mut ctl.envio, &ClientMessage::Ping { timestamp: 77 })
        .await
        .unwrap();
    let voltou = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let ServerMessage::Pong { timestamp: 77 } =
                frame::read::<ServerMessage>(&mut ctl.recebe).await.unwrap()
            {
                return;
            }
        }
    })
    .await;
    assert!(voltou.is_ok(), "o Ping pelo caminho do ENC/3 não voltou");

    // A voz, até outra pessoa na mesma sala (direto, pelo endereço do AQUI).
    let outra = Ponta::abrir(local, None).await.unwrap();
    let ouvinte = outra
        .discar(Discagem::nova(vec![endereco], Pino::Impressao(fp)))
        .await
        .unwrap();
    let (mut ctl_ouvinte, _, _) = entrar(&ouvinte, 42).await;
    for c in [&mut ctl, &mut ctl_ouvinte] {
        frame::write(
            &mut c.envio,
            &ClientMessage::EnterVoiceRoom {
                voice_room: sala,
                password: None,
            },
        )
        .await
        .unwrap();
    }
    let mut recebe_o = ctl_ouvinte.recebe;
    tokio::spawn(async move { while frame::read::<ServerMessage>(&mut recebe_o).await.is_ok() {} });
    let mut recebe_f = ctl.recebe;
    tokio::spawn(async move { while frame::read::<ServerMessage>(&mut recebe_f).await.is_ok() {} });
    tokio::time::sleep(Duration::from_millis(400)).await;
    let ouvindo = {
        let ouvinte = ouvinte.clone();
        tokio::spawn(async move {
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let bytes = ouvinte.read_datagram().await.unwrap();
                    if MediaHeader::decode(&bytes).is_ok_and(|(c, _)| c.ssrc == ssrc.get()) {
                        return;
                    }
                }
            })
            .await
        })
    };
    for seq in 0..30_u16 {
        let _ = conexao.send_datagram(quadro_de_voz(ssrc, seq));
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        ouvindo.await.unwrap().is_ok(),
        "a voz de quem entrou pelo ENC/3 não chegou a quem está na sala"
    );
    drop(ctl.envio);
    drop(ctl_ouvinte.envio);
    daemon.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn o_enc3_inteiro_com_ipv4_mapeado_no_seeled_de_pilha_dupla() {
    o_caminho_inteiro(SocketAddr::from(([127, 0, 0, 1], 0))).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn o_enc3_inteiro_em_ipv6_loopback() {
    if std::net::UdpSocket::bind("[::1]:0").is_err() {
        // Uma máquina sem ::1 não tem o que este teste mede.
        return;
    }
    o_caminho_inteiro(SocketAddr::from((Ipv6Addr::LOCALHOST, 0))).await;
}

/// Um anfitrião de mentira, num socket cru, que se registra no ponto e
/// **nunca atende** um `CHAMADO`.
async fn escuta_calada(ponto: SocketAddr, marcas: &Marcas) -> tokio::net::UdpSocket {
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let mut balde = [0_u8; encontro::TAMANHO3 + 1];
    let mut cookie = None;
    for _ in 0..3 {
        for marca in [marcas.escuta(), marcas.servidor()] {
            socket
                .send_to(&encontro::mora3(marca, cookie.as_ref()), ponto)
                .await
                .unwrap();
        }
        let ate = tokio::time::Instant::now() + Duration::from_millis(500);
        while let Ok(Ok((n, _))) = tokio::time::timeout_at(ate, socket.recv_from(&mut balde)).await
        {
            match encontro::ler_resposta3(&balde[..n]) {
                Some(Resposta3::Desafio { cookie: c, .. }) => cookie = Some(c),
                Some(Resposta3::Morou { .. }) => return socket,
                _ => {}
            }
        }
    }
    panic!("a escuta de mentira não se registrou");
}

/// Conta as Binding Indications (20 B, tipo 0x0011) que chegam a um socket.
async fn indicacoes(socket: &tokio::net::UdpSocket, por: Duration) -> (usize, Vec<Resposta3>) {
    let mut contadas = 0;
    let mut respostas = Vec::new();
    let mut balde = [0_u8; encontro::TAMANHO3 + 1];
    let ate = tokio::time::Instant::now() + por;
    while let Ok(Ok((n, _))) = tokio::time::timeout_at(ate, socket.recv_from(&mut balde)).await {
        if n == 20 && balde[0] == 0x00 && balde[1] == 0x11 {
            contadas += 1;
        } else if let Some(r) = encontro::ler_resposta3(&balde[..n]) {
            respostas.push(r);
        }
    }
    (contadas, respostas)
}

/// **As recusas e os tetos**, no mesmo ponto e no mesmo `seeled`.
#[tokio::test(flavor = "multi_thread")]
async fn as_recusas_e_os_tetos_do_enc3() {
    let local = SocketAddr::from(([127, 0, 0, 1], 0));
    let ponto = ponto(local);
    let (daemon, _encontro) = anfitriao(ponto).await;
    let fp = impressao(daemon.fingerprint());
    let marcas = Marcas::do_servidor(daemon.fingerprint()).unwrap();
    let quem_liga = Ponta::abrir(local, None).await.unwrap();
    let mut caixa = quem_liga.mensagens_enc().unwrap();

    // Marca inexistente: NINGUEM.
    let ninguem = Marcas::do_servidor(&"ab".repeat(32)).unwrap();
    let resposta = chamar(
        &quem_liga,
        &mut caixa,
        ponto,
        &ninguem,
        &seele_server::transporte::sortear_credencial(),
    )
    .await;
    assert!(
        matches!(resposta, Some(Resposta3::Ninguem { .. })),
        "{resposta:?}"
    );

    // Escuta calada: CALADO.
    let caladas = Marcas::do_servidor(&"cd".repeat(32)).unwrap();
    let _calada = escuta_calada(ponto, &caladas).await;
    let resposta = chamar(
        &quem_liga,
        &mut caixa,
        ponto,
        &caladas,
        &seele_server::transporte::sortear_credencial(),
    )
    .await;
    assert!(
        matches!(resposta, Some(Resposta3::Calado { .. })),
        "{resposta:?}"
    );

    // O teto de Indications por CHAMADO: quem liga por um socket cru, e o
    // ponto junta o endereço que viu aos candidatos.
    let cru = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let chama = encontro::chama(&Chama {
        servidor: marcas.servidor().clone(),
        escuta: marcas.escuta().clone(),
        credencial: credencial(&seele_server::transporte::sortear_credencial()),
        candidatos: Vec::new(),
    });
    cru.send_to(&chama, ponto).await.unwrap();
    let (furos, respostas) = indicacoes(&cru, Duration::from_millis(2_000)).await;
    println!("furos {furos}, respostas {respostas:?}");
    assert!(
        (1..=seele_server::transporte::INDICACOES_POR_CANDIDATO).contains(&furos),
        "o anfitrião mandou {furos} Indications por um CHAMADO (o teto é {})",
        seele_server::transporte::INDICACOES_POR_CANDIDATO
    );
    assert!(respostas
        .iter()
        .any(|r| matches!(r, Resposta3::Aqui { .. })));

    // Candidato inválido (porta abaixo de 1024): fora da forma canônica, o
    // ponto cala, e ninguém fura nada para ele. (O descarte no anfitrião, para
    // um ponto que deixasse passar, está em `alcance::encontro`.)
    let invalido = encontro::chama(&Chama {
        servidor: marcas.servidor().clone(),
        escuta: marcas.escuta().clone(),
        credencial: credencial(&seele_server::transporte::sortear_credencial()),
        candidatos: vec![SocketAddr::from(([127, 0, 0, 1], 80))],
    });
    cru.send_to(&invalido, ponto).await.unwrap();
    let (furos, respostas) = indicacoes(&cru, Duration::from_millis(1_200)).await;
    assert_eq!(
        (furos, respostas.len()),
        (0, 0),
        "um CHAMA com candidato inválido foi atendido"
    );

    // Impressão errada: recusada no DTLS, antes de qualquer canal.
    let u = seele_server::transporte::sortear_credencial();
    let Some(Resposta3::Aqui { endereco, .. }) =
        chamar(&quem_liga, &mut caixa, ponto, &marcas, &u).await
    else {
        panic!("sem AQUI");
    };
    let mut errada = fp;
    errada[0] ^= 0xFF;
    let mut discagem = Discagem::nova(vec![endereco], Pino::Impressao(errada));
    discagem.credencial = Some(u.clone());
    discagem.prazo = Duration::from_secs(5);
    let falha = quem_liga
        .discar(discagem)
        .await
        .expect_err("a impressão errada entrou");
    assert!(
        matches!(falha, FalhaAoDiscar::ImpressaoDiferente { .. }),
        "{falha:?}"
    );

    // A lápide: o mesmo U, agora com a impressão certa, não cria par novo.
    tokio::time::sleep(seele_server::transporte::APERTO + Duration::from_secs(1)).await;
    let mut de_novo = Discagem::nova(vec![endereco], Pino::Impressao(fp));
    de_novo.credencial = Some(u);
    de_novo.prazo = Duration::from_secs(3);
    assert!(
        quem_liga.discar(de_novo).await.is_err(),
        "o U que falhou criou um par novo dentro da lápide"
    );
    assert!(daemon.ponta().contadores().bindings_em_lapide > 0);
    // E um U novo entra.
    let outro_u = seele_server::transporte::sortear_credencial();
    let Some(Resposta3::Aqui { .. }) =
        chamar(&quem_liga, &mut caixa, ponto, &marcas, &outro_u).await
    else {
        panic!("sem AQUI para o U novo");
    };
    let mut nova = Discagem::nova(vec![endereco], Pino::Impressao(fp));
    nova.credencial = Some(outro_u);
    assert!(
        quem_liga.discar(nova).await.is_ok(),
        "um U novo não entrou depois da lápide do outro"
    );

    // A taxa: uma enxurrada de CHAMA de um endereço só ouve RECUSA.
    let enxurrada = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    for _ in 0..200 {
        enxurrada.send_to(&chama, ponto).await.unwrap();
    }
    let (_, respostas) = indicacoes(&enxurrada, Duration::from_secs(4)).await;
    let recusas: Vec<_> = respostas
        .iter()
        .filter_map(|r| match r {
            Resposta3::Recusa { motivo, .. } => Some(*motivo),
            _ => None,
        })
        .collect();
    println!("recusas da enxurrada: {recusas:?}");
    assert!(
        recusas.contains(&encontro::Motivo::Taxa),
        "duzentos CHAMA seguidos e nenhuma RECUSA taxa: {respostas:?}"
    );
    daemon.shutdown();
}
