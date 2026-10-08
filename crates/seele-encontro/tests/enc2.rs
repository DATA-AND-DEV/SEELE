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
    }
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

    anfitriao
        .send_to(
            &moro2(&assinado(&chave, marca.clone(), agora(), dicas.clone())),
            ponto,
        )
        .unwrap();
    let resposta = receber(&anfitriao).expect("o MORO2 não foi respondido");
    assert_eq!(
        resposta.len(),
        TAMANHO2,
        "a resposta não tem o tamanho do pedido"
    );
    assert_eq!(
        ler_resposta2(&resposta),
        Some(Resposta2::Morou {
            marca: marca.clone(),
            endereco: anfitriao.local_addr().unwrap(),
        })
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
    anfitriao
        .send_to(
            &moro2(&assinado(&chave, marca.clone(), agora() - 1000, Vec::new())),
            ponto,
        )
        .unwrap();
    let Some(Resposta2::Recusa {
        marca: dela,
        motivo,
        agora: do_ponto,
    }) = ler_resposta2(&receber(&anfitriao).expect("o relógio torto foi calado"))
    else {
        panic!("o relógio torto não virou RECUSA");
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
