//! O SEELE-ENC/2 pelo socket de verdade (ADR 0062): o registro assinado, o
//! `QUEM2` que sempre responde, e a versão 1 continuando igual ao lado.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "teste: uma expectativa quebrada é justamente o que se quer ver falhar"
)]

use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Signer, SigningKey};
use seele_encontro::Ponto;
use seele_proto::codigo::Codigo;
use seele_proto::encontro::{
    ler_aqui, ler_resposta2, moro, moro2, onde, quem2, Marca, Marcas, Moro2, Motivo, Resposta2,
    Vizinhanca, TAMANHO, TAMANHO2,
};

const PRAZO: Duration = Duration::from_secs(2);

fn socket() -> UdpSocket {
    let socket = UdpSocket::bind("127.0.0.1:0").expect("o loopback não abriu");
    socket.set_read_timeout(Some(PRAZO)).expect("sem prazo");
    socket
}

fn ponto_no_ar() -> SocketAddr {
    let ponto = Ponto::abrir("127.0.0.1:0".parse().unwrap(), Vizinhanca::TambemAqui)
        .expect("o ponto de encontro não subiu");
    let endereco = ponto.endereco().unwrap();
    std::thread::spawn(move || {
        let _ = ponto.servir();
    });
    endereco
}

fn agora() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn marcas_de(chave: &SigningKey) -> Marcas {
    Marcas::da_sala(&Codigo::da_chave(&chave.verifying_key().to_bytes()))
}

fn assinado(chave: &SigningKey, marca: Marca, carimbo: u64, dicas: Vec<SocketAddr>) -> Moro2 {
    let publica = chave.verifying_key().to_bytes();
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

/// O caminho de verdade de um registro: o `MORO2` sem cookie ouve o `DESAFIO`,
/// e o mesmo `MORO2`, com o cookie dele, recebe a resposta que se devolve.
fn morar(socket: &UdpSocket, ponto: SocketAddr, mut pedido: Moro2) -> Option<Resposta2> {
    pedido.cookie = Some(cookie_de(socket, ponto, &pedido));
    socket.send_to(&moro2(&pedido), ponto).unwrap();
    ler_resposta2(&receber(socket)?)
}

/// O cookie deste socket, pelo `DESAFIO` de um `MORO2` sem ele.
fn cookie_de(socket: &UdpSocket, ponto: SocketAddr, pedido: &Moro2) -> [u8; 16] {
    let sem = Moro2 {
        cookie: None,
        ..pedido.clone()
    };
    socket.send_to(&moro2(&sem), ponto).unwrap();
    let resposta = receber(socket).map(|d| ler_resposta2(&d));
    let Some(Some(Resposta2::Desafio { cookie, .. })) = resposta else {
        panic!("um MORO2 sem cookie não ouviu o DESAFIO: {resposta:?}");
    };
    cookie
}

fn receber(socket: &UdpSocket) -> Option<Vec<u8>> {
    let mut balde = [0_u8; TAMANHO2 + 1];
    let (lidos, _) = socket.recv_from(&mut balde).ok()?;
    Some(balde[..lidos].to_vec())
}

/// Que nada chegou, sem esperar os dois segundos: no loopback, o que ia
/// chegar já chegou.
fn nada_chega(socket: &UdpSocket) -> bool {
    socket
        .set_read_timeout(Some(Duration::from_millis(300)))
        .unwrap();
    let vazio = receber(socket).is_none();
    socket.set_read_timeout(Some(PRAZO)).unwrap();
    vazio
}

#[test]
fn um_moro2_assinado_mora_e_o_quem2_devolve_o_endereco_e_as_dicas() {
    let ponto = ponto_no_ar();
    let chave = SigningKey::from_bytes(&[1; 32]);
    let marca = marcas_de(&chave).servidor().clone();
    let dicas = vec!["192.168.0.10:8385".parse().unwrap()];
    let anfitriao = socket();
    let quem_liga = socket();

    let resposta = morar(
        &anfitriao,
        ponto,
        assinado(&chave, marca.clone(), agora(), dicas.clone()),
    );
    let Some(Resposta2::Morou {
        marca: dela,
        endereco,
        ..
    }) = resposta
    else {
        panic!("o MORO2 com o cookie não morou: {resposta:?}");
    };
    assert_eq!(
        (dela, endereco),
        (marca.clone(), anfitriao.local_addr().unwrap())
    );

    quem_liga.send_to(&quem2(&marca), ponto).unwrap();
    assert_eq!(
        ler_resposta2(&receber(&quem_liga).expect("o QUEM2 não foi respondido")),
        Some(Resposta2::Aqui2 {
            marca,
            endereco: anfitriao.local_addr().unwrap(),
            dicas,
        })
    );
}

#[test]
fn o_quem2_de_uma_marca_vazia_diz_ninguem_e_nao_cala() {
    // Calar aqui repetiria o ADR 0047: quem liga não separaria «ninguém mora»
    // de «o ponto não fala o ENC/2».
    let ponto = ponto_no_ar();
    let quem_liga = socket();
    let marca = marcas_de(&SigningKey::from_bytes(&[2; 32]))
        .servidor()
        .clone();
    quem_liga.send_to(&quem2(&marca), ponto).unwrap();
    assert_eq!(
        ler_resposta2(&receber(&quem_liga).expect("o QUEM2 de uma marca vazia foi calado")),
        Some(Resposta2::Ninguem { marca })
    );
}

#[test]
fn um_moro2_com_assinatura_errada_e_calado_e_nao_mora() {
    let ponto = ponto_no_ar();
    let chave = SigningKey::from_bytes(&[3; 32]);
    let marca = marcas_de(&chave).servidor().clone();
    let mut forjado = assinado(&chave, marca.clone(), agora(), Vec::new());
    forjado.assinatura[0] ^= 1;
    let impostor = socket();
    // Com o cookie do próprio impostor: aqui a barreira é a assinatura.
    forjado.cookie = Some(cookie_de(&impostor, ponto, &forjado));
    impostor.send_to(&moro2(&forjado), ponto).unwrap();
    assert!(
        nada_chega(&impostor),
        "um MORO2 com a assinatura errada foi respondido"
    );
    impostor.send_to(&quem2(&marca), ponto).unwrap();
    assert_eq!(
        ler_resposta2(&receber(&impostor).unwrap()),
        Some(Resposta2::Ninguem { marca }),
        "um MORO2 com a assinatura errada morou"
    );
}

#[test]
fn um_moro2_com_relogio_torto_ouve_a_recusa_com_a_hora_do_ponto() {
    let ponto = ponto_no_ar();
    let chave = SigningKey::from_bytes(&[4; 32]);
    let marca = marcas_de(&chave).servidor().clone();
    let anfitriao = socket();
    let resposta = morar(
        &anfitriao,
        ponto,
        assinado(&chave, marca.clone(), agora() - 1000, Vec::new()),
    );
    let Some(Resposta2::Recusa {
        marca: dela,
        motivo,
        agora: do_ponto,
    }) = resposta
    else {
        panic!("o relógio torto não virou RECUSA: {resposta:?}");
    };
    assert_eq!((dela, motivo), (marca, Motivo::Relogio));
    assert!(
        do_ponto.abs_diff(agora()) <= 2,
        "a hora do ponto veio errada: {do_ponto}"
    );
}

#[test]
fn um_moro_sem_assinatura_com_a_marca_da_sala_nao_aparece_no_quem2() {
    // O registro assinado vence o não assinado por construção: o MORO da
    // versão 1 não entra onde o QUEM2 lê.
    let ponto = ponto_no_ar();
    let marca = marcas_de(&SigningKey::from_bytes(&[5; 32]))
        .servidor()
        .clone();
    let impostor = socket();
    impostor.send_to(&moro(&marca), ponto).unwrap();
    assert!(
        ler_aqui(&receber(&impostor).unwrap()).is_some(),
        "o MORO da versão 1 deixou de responder como ONDE"
    );
    impostor.send_to(&quem2(&marca), ponto).unwrap();
    assert_eq!(
        ler_resposta2(&receber(&impostor).unwrap()),
        Some(Resposta2::Ninguem { marca })
    );
}

#[test]
fn as_duas_versoes_convivem_no_mesmo_ponto_e_no_mesmo_socket() {
    let ponto = ponto_no_ar();
    let alguem = socket();
    let marca = Marca::nova("abc").unwrap();

    alguem.send_to(&onde(&marca), ponto).unwrap();
    let v1 = receber(&alguem).unwrap();
    assert_eq!(v1.len(), TAMANHO);
    assert_eq!(
        ler_aqui(&v1),
        Some((marca.clone(), alguem.local_addr().unwrap()))
    );

    alguem.send_to(&quem2(&marca), ponto).unwrap();
    let v2 = receber(&alguem).unwrap();
    assert_eq!(v2.len(), TAMANHO2);
    assert_eq!(ler_resposta2(&v2), Some(Resposta2::Ninguem { marca }));
}

#[test]
fn um_datagrama_maior_que_o_do_protocolo_e_calado() {
    // Um byte a mais, nas duas versões. Num balde do tamanho exato, o sistema
    // trunca sem erro, e o pedaço truncado era um pedido válido.
    let ponto = ponto_no_ar();
    let alguem = socket();
    let marca = Marca::nova("abc").unwrap();
    let mut grande = quem2(&marca);
    grande.push(b'.');
    alguem.send_to(&grande, ponto).unwrap();
    assert!(
        nada_chega(&alguem),
        "um QUEM2 com um byte a mais foi respondido"
    );
    let mut maior_que_o_v1 = onde(&marca);
    maior_que_o_v1.push(b'.');
    alguem.send_to(&maior_que_o_v1, ponto).unwrap();
    assert!(
        nada_chega(&alguem),
        "um ONDE com um byte a mais foi respondido"
    );
}

/// Lê tudo o que chegar até o socket calar por 300 ms.
fn tudo_que_chega(socket: &UdpSocket) -> Vec<Vec<u8>> {
    socket
        .set_read_timeout(Some(Duration::from_millis(300)))
        .unwrap();
    let mut chegou = Vec::new();
    while let Some(datagrama) = receber(socket) {
        chegou.push(datagrama);
    }
    socket.set_read_timeout(Some(PRAZO)).unwrap();
    chegou
}

#[test]
fn uma_enxurrada_de_quem2_e_calada_e_a_recusa_e_dita_uma_vez() {
    // A fiação da taxa no ponto: o balde de 64 passa, a primeira recusa é
    // dita, e o resto da enxurrada é calado.
    let ponto = ponto_no_ar();
    let alguem = socket();
    let marca = marcas_de(&SigningKey::from_bytes(&[6; 32]))
        .servidor()
        .clone();
    for _ in 0..100 {
        alguem.send_to(&quem2(&marca), ponto).unwrap();
    }
    let chegou = tudo_que_chega(&alguem);
    let ninguem = chegou
        .iter()
        .filter(|d| matches!(ler_resposta2(d), Some(Resposta2::Ninguem { .. })))
        .count();
    let recusas = chegou
        .iter()
        .filter(|d| {
            matches!(
                ler_resposta2(d),
                Some(Resposta2::Recusa {
                    motivo: Motivo::Taxa,
                    ..
                })
            )
        })
        .count();
    assert!(
        (64..=80).contains(&ninguem),
        "{ninguem} QUEM2 passaram pelo limite de uma enxurrada de 100"
    );
    assert_eq!(recusas, 1, "a recusa por taxa foi dita {recusas} vezes");
}

#[test]
fn uma_enxurrada_de_quem2_nao_cala_o_registro_da_mesma_rede() {
    let ponto = ponto_no_ar();
    let alguem = socket();
    let chave = SigningKey::from_bytes(&[7; 32]);
    let marca = marcas_de(&chave).servidor().clone();
    let mut pedido = assinado(&chave, marca.clone(), agora(), Vec::new());
    pedido.cookie = Some(cookie_de(&alguem, ponto, &pedido));
    // Logo em seguida, com o balde das perguntas vazio: esperar o fim das
    // respostas daria tempo de ele encher de novo.
    for _ in 0..100 {
        alguem.send_to(&quem2(&marca), ponto).unwrap();
    }
    alguem.send_to(&moro2(&pedido), ponto).unwrap();
    let chegou = tudo_que_chega(&alguem);
    assert!(
        chegou
            .iter()
            .any(|d| matches!(ler_resposta2(d), Some(Resposta2::Morou { .. }))),
        "a enxurrada de QUEM2 esvaziou o balde do registro da mesma rede, e o MORO2 \
         logo depois dela foi calado"
    );
}

#[test]
fn um_moro2_sem_cookie_ouve_o_desafio_e_nao_mora() {
    let ponto = ponto_no_ar();
    let chave = SigningKey::from_bytes(&[8; 32]);
    let marca = marcas_de(&chave).servidor().clone();
    let anfitriao = socket();
    anfitriao
        .send_to(
            &moro2(&assinado(&chave, marca.clone(), agora(), Vec::new())),
            ponto,
        )
        .unwrap();
    let resposta = receber(&anfitriao).expect("um MORO2 sem cookie foi calado");
    assert_eq!(
        resposta.len(),
        TAMANHO2,
        "o DESAFIO não tem o tamanho do pedido"
    );
    assert!(
        matches!(ler_resposta2(&resposta), Some(Resposta2::Desafio { .. })),
        "um MORO2 sem cookie não ouviu o DESAFIO"
    );
    anfitriao.send_to(&quem2(&marca), ponto).unwrap();
    assert_eq!(
        ler_resposta2(&receber(&anfitriao).unwrap()),
        Some(Resposta2::Ninguem { marca }),
        "um MORO2 sem cookie morou"
    );
}

#[test]
fn cem_moro2_sem_cookie_nao_gastam_o_balde_do_registro() {
    // Quem forja a origem não recebe o cookie do IP que forja: o que ele manda
    // ouve DESAFIO, e não gasta o balde de quem mora naquele IP.
    let ponto = ponto_no_ar();
    let chave = SigningKey::from_bytes(&[9; 32]);
    let marca = marcas_de(&chave).servidor().clone();
    let alguem = socket();
    let mut pedido = assinado(&chave, marca, agora(), Vec::new());
    let cookie = cookie_de(&alguem, ponto, &pedido);
    for _ in 0..100 {
        alguem.send_to(&moro2(&pedido), ponto).unwrap();
    }
    pedido.cookie = Some(cookie);
    alguem.send_to(&moro2(&pedido), ponto).unwrap();
    let chegou = tudo_que_chega(&alguem);
    assert!(
        chegou
            .iter()
            .any(|d| matches!(ler_resposta2(d), Some(Resposta2::Morou { .. }))),
        "cem MORO2 sem cookie gastaram o balde do registro, e o com cookie foi calado"
    );
}

#[test]
fn o_cookie_de_um_ip_nao_vale_noutro() {
    // As duas famílias do loopback são dois IPs: o cookie do ::1 não vale no
    // 127.0.0.1, como o de quem forja a origem não vale no IP que ele forja.
    let quarto = std::sync::Arc::new(seele_encontro::Quarto::novo());
    let quatro = Ponto::abrir_com_quarto(
        "127.0.0.1:0".parse().unwrap(),
        Vizinhanca::TambemAqui,
        std::sync::Arc::clone(&quarto),
    )
    .unwrap();
    let Ok(seis) = Ponto::abrir_com_quarto(
        "[::1]:0".parse().unwrap(),
        Vizinhanca::TambemAqui,
        std::sync::Arc::clone(&quarto),
    ) else {
        eprintln!("PARCIAL: esta máquina não tem o ::1, e o teste dos dois IPs não roda");
        return;
    };
    let (onde_quatro, onde_seis) = (quatro.endereco().unwrap(), seis.endereco().unwrap());
    std::thread::spawn(move || {
        let _ = quatro.servir();
    });
    std::thread::spawn(move || {
        let _ = seis.servir();
    });

    let chave = SigningKey::from_bytes(&[10; 32]);
    let marca = marcas_de(&chave).servidor().clone();
    let mut pedido = assinado(&chave, marca, agora(), Vec::new());
    let pelo_seis = UdpSocket::bind("[::1]:0").unwrap();
    pelo_seis.set_read_timeout(Some(PRAZO)).unwrap();
    pedido.cookie = Some(cookie_de(&pelo_seis, onde_seis, &pedido));

    let pelo_quatro = socket();
    pelo_quatro.send_to(&moro2(&pedido), onde_quatro).unwrap();
    assert!(
        matches!(
            ler_resposta2(&receber(&pelo_quatro).unwrap()),
            Some(Resposta2::Desafio { .. })
        ),
        "o cookie do ::1 valeu no 127.0.0.1"
    );
}
