//! HTTP real da caixa pessoal; o ENC/3 continua nos testes `enc3`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "fixtures sintéticas e asserções de teste"
)]
use ed25519_dalek::{Signer, SigningKey};
use seele_encontro::{sinal::Sinal, Quarto};
use seele_proto::chamada::{self, Assinatura, Convite, Motivo, Operacao, Papel, Pedido, Resposta};
use seele_proto::encontro::Vizinhanca;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::Duration;
const ORIGEM: &str = "https://exemplo.test";
fn servidor() -> SocketAddr {
    let s = Sinal::abrir(
        "127.0.0.1:0".parse().unwrap(),
        Arc::new(Quarto::novo()),
        Vizinhanca::TambemAqui,
        vec![ORIGEM.into()],
    )
    .unwrap();
    let addr = s.endereco().unwrap();
    std::thread::spawn(move || {
        let _ = s.servir();
    });
    addr
}
fn http(addr: SocketAddr, req: &str) -> (String, String) {
    let mut s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(6))).unwrap();
    s.write_all(req.as_bytes()).unwrap();
    let mut texto = String::new();
    s.read_to_string(&mut texto).unwrap();
    let (h, b) = texto.split_once("\r\n\r\n").unwrap();
    (h.into(), b.into())
}
fn get(addr: SocketAddr) -> ([u8; 32], u64) {
    let (h, b) = http(
        addr,
        &format!("GET /chamada/1 HTTP/1.1\r\nHost: x\r\nOrigin: {ORIGEM}\r\n\r\n"),
    );
    assert!(h.starts_with("HTTP/1.1 200"));
    assert!(h.contains("Cache-Control: no-store"));
    assert!(h.contains(&format!("Access-Control-Allow-Origin: {ORIGEM}")));
    let Resposta::Desafio { epoca, agora } = chamada::ler_resposta(&b).unwrap() else {
        panic!()
    };
    (epoca, agora)
}
fn post(addr: SocketAddr, p: &Pedido) -> Resposta {
    let b = chamada::escrever_pedido(p).unwrap();
    let (h, b) = http(
        addr,
        &format!(
            "POST /chamada/1 HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\n\r\n{b}",
            b.len()
        ),
    );
    assert!(h.starts_with("HTTP/1.1 200"), "{h}");
    chamada::ler_resposta(&b).unwrap()
}
fn pedido(
    epoca: [u8; 32],
    agora: u64,
    sign: &SigningKey,
    destino: [u8; 32],
    n: u8,
    operacao: Operacao,
    papel: Papel,
) -> Pedido {
    let chamada = if matches!(operacao, Operacao::Registrar { .. }) {
        [0; 32]
    } else {
        [8; 32]
    };
    let mut p = Pedido {
        epoca,
        nonce: [n; 16],
        convite: [9; 32],
        chamada,
        remetente: sign.verifying_key().to_bytes(),
        destinatario: destino,
        geracao: 1,
        sequencia: u64::from(n),
        valido_ate: agora + 60,
        papel,
        operacao,
        assinatura: Assinatura::default(),
    };
    assinar(&mut p, sign);
    p
}
fn assinar(p: &mut Pedido, k: &SigningKey) {
    p.assinatura =
        Assinatura::de_bytes(k.sign(&chamada::mensagem_do_pedido(p).unwrap()).to_bytes());
}
#[test]
fn http_entrega_assinada_retry_cursor_e_ack_entre_duas_pontas() {
    let addr = servidor();
    let (epoca, agora) = get(addr);
    let dono = SigningKey::from_bytes(&[1; 32]);
    let guest = SigningKey::from_bytes(&[2; 32]);
    let dk = dono.verifying_key().to_bytes();
    let gk = guest.verifying_key().to_bytes();
    let mut c = Convite {
        id: [9; 32],
        dono: dk,
        expira_em: agora + 600,
        assinatura: Assinatura::default(),
    };
    c.assinatura = Assinatura::de_bytes(dono.sign(&chamada::mensagem_do_convite(&c)).to_bytes());
    let registrar = pedido(
        epoca,
        agora,
        &dono,
        dk,
        1,
        Operacao::Registrar { convite: c.clone() },
        Papel::QuemRecebe,
    );
    assert_eq!(post(addr, &registrar), Resposta::Ok);
    let chamar = pedido(
        epoca,
        agora,
        &guest,
        dk,
        2,
        Operacao::Chamar {
            convite: c,
            apelido: "Pessoa".into(),
        },
        Papel::QuemLiga,
    );
    assert_eq!(post(addr, &chamar), Resposta::Ok);
    assert_eq!(post(addr, &chamar), Resposta::Ok);
    let mut buscar = pedido(
        epoca,
        agora,
        &dono,
        dk,
        3,
        Operacao::Buscar {
            cursor: 0,
            confirmado: 0,
        },
        Papel::QuemRecebe,
    );
    buscar.chamada = [0; 32];
    assinar(&mut buscar, &dono);
    let primeira = post(addr, &buscar);
    assert_eq!(post(addr, &buscar), primeira);
    let Resposta::Eventos { eventos, cursor } = primeira else {
        panic!()
    };
    assert_eq!(eventos.len(), 1);
    assert_eq!(eventos[0].pedido, chamar);
    buscar.nonce = [4; 16];
    buscar.operacao = Operacao::Buscar {
        cursor,
        confirmado: cursor,
    };
    assinar(&mut buscar, &dono);
    assert!(matches!(post(addr,&buscar),Resposta::Eventos{eventos,..} if eventos.is_empty()));
    let atender = pedido(
        epoca,
        agora,
        &dono,
        gk,
        5,
        Operacao::Atender,
        Papel::QuemRecebe,
    );
    assert_eq!(post(addr, &atender), Resposta::Ok);
    let buscar_guest = pedido(
        epoca,
        agora,
        &guest,
        dk,
        6,
        Operacao::Buscar {
            cursor: 0,
            confirmado: 0,
        },
        Papel::QuemLiga,
    );
    assert!(
        matches!(post(addr,&buscar_guest),Resposta::Eventos{eventos,..} if eventos[0].pedido == atender)
    );
    let mut forjado = buscar_guest;
    forjado.nonce = [7; 16];
    assert_eq!(
        post(addr, &forjado),
        Resposta::Recusa {
            motivo: Motivo::Assinatura
        }
    );
}
#[test]
fn http_limites_e_cors_preservam_sinal_1() {
    let addr = servidor();
    let (h, _) = http(
        addr,
        "GET /chamada/1 HTTP/1.1\r\nHost: x\r\nOrigin: https://nao.test\r\n\r\n",
    );
    assert!(!h.contains("Access-Control-Allow-Origin"));
    let (h, _) = http(addr, "OPTIONS /chamada/1 HTTP/1.1\r\nHost: x\r\n\r\n");
    assert!(h.starts_with("HTTP/1.1 204"));
    for (caminho, tamanho) in [("/sinal/1", 513), ("/chamada/1", chamada::MAX_PEDIDO + 1)] {
        let (h, _) = http(
            addr,
            &format!("POST {caminho} HTTP/1.1\r\nHost: x\r\nContent-Length: {tamanho}\r\n\r\n"),
        );
        assert!(h.starts_with("HTTP/1.1 413"), "{h}");
    }
    for extra in [
        "Content-Length: 0\r\nContent-Length: 0",
        "Transfer-Encoding: chunked",
    ] {
        let (h, _) = http(
            addr,
            &format!("POST /chamada/1 HTTP/1.1\r\nHost: x\r\n{extra}\r\n\r\n"),
        );
        assert!(h.starts_with("HTTP/1.1 400"), "{h}");
    }
    let (h, _) = http(
        addr,
        "POST /chamada/1 HTTP/1.1\r\nHost: x\r\nContent-Length: 1\r\n\r\nx",
    );
    assert!(h.starts_with("HTTP/1.1 400"));
}
