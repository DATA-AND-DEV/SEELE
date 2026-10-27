//! O SEELE-ENC/3 e o sinal pelo socket de verdade (contrato da 0.17, D7): o
//! registro da 0.17, a chamada que vira `CHAMADO` e `AQUI`, o `CALADO`, o
//! `NINGUEM`, o STUN Binding, e o `POST /sinal/1`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "teste: uma expectativa quebrada é justamente o que se quer ver falhar"
)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::sync::Arc;
use std::time::{Duration, Instant};

use seele_encontro::sinal::Sinal;
use seele_encontro::{Ponto, Quarto};
use seele_proto::encontro::{
    analisar3, atende, chama, ler_resposta2, ler_resposta3, ler_resposta_do_sinal, mora3,
    pedido_do_sinal, quem2, quem3, Atende, Chama, Credencial, Marca, Marcas, Pedido3, Resposta3,
    RespostaDoSinal, Vizinhanca, TAMANHO3,
};

const PRAZO: Duration = Duration::from_secs(5);
const FP: &str = "3cbcfb0212da738f89c156de86eb280adee30fd6b907523b898fedcb2b1de5b9";

fn socket() -> UdpSocket {
    let socket = UdpSocket::bind("127.0.0.1:0").expect("o loopback não abriu");
    socket.set_read_timeout(Some(PRAZO)).expect("sem prazo");
    socket
}

fn ponto_no_ar() -> (SocketAddr, Arc<Quarto>) {
    let ponto = Ponto::abrir("127.0.0.1:0".parse().unwrap(), Vizinhanca::TambemAqui)
        .expect("o ponto de encontro não subiu");
    let endereco = ponto.endereco().unwrap();
    let quarto = ponto.quarto();
    std::thread::spawn(move || {
        let _ = ponto.servir();
    });
    (endereco, quarto)
}

fn sinal_no_ar(quarto: Arc<Quarto>) -> SocketAddr {
    let sinal = Sinal::abrir(
        "127.0.0.1:0".parse().unwrap(),
        quarto,
        Vizinhanca::TambemAqui,
        vec![seele_encontro::sinal::origem_do_produto()],
    )
    .expect("o sinal não subiu");
    let endereco = sinal.endereco().unwrap();
    std::thread::spawn(move || {
        let _ = sinal.servir();
    });
    endereco
}

fn ouvir(socket: &UdpSocket) -> Vec<u8> {
    let mut balde = [0_u8; TAMANHO3 + 1];
    let (n, _) = socket.recv_from(&mut balde).expect("nada chegou no prazo");
    balde[..n].to_vec()
}

fn resposta(socket: &UdpSocket) -> Resposta3 {
    let datagrama = ouvir(socket);
    assert_eq!(
        datagrama.len(),
        TAMANHO3,
        "toda resposta do ENC/3 tem o tamanho fixo"
    );
    ler_resposta3(&datagrama).expect("não é uma resposta do ENC/3")
}

/// Registra uma marca pelo `MORA3`, com a volta do `DESAFIO`.
fn registrar(socket: &UdpSocket, ponto: SocketAddr, marca: &Marca) {
    socket.send_to(&mora3(marca, None), ponto).unwrap();
    let Resposta3::Desafio { cookie, .. } = resposta(socket) else {
        panic!("sem cookie, o MORA3 ouve um DESAFIO");
    };
    socket.send_to(&mora3(marca, Some(&cookie)), ponto).unwrap();
    assert!(
        matches!(resposta(socket), Resposta3::Morou { .. }),
        "com o cookie, o MORA3 mora"
    );
}

fn credencial() -> Credencial {
    Credencial::nova("AbCdEfGhIjKlMnOpQrStUvWxYz012345").unwrap()
}

fn um_chama(marcas: &Marcas, candidatos: Vec<SocketAddr>) -> Chama {
    Chama {
        servidor: marcas.servidor().clone(),
        escuta: marcas.escuta().clone(),
        credencial: credencial(),
        candidatos,
    }
}

/// Um anfitrião da 0.17 de mentira: registra as duas marcas e devolve a escuta.
fn anfitriao(ponto: SocketAddr) -> (UdpSocket, UdpSocket, Marcas) {
    let marcas = Marcas::do_servidor(FP).unwrap();
    let escuta = socket();
    let servidor = socket();
    registrar(&escuta, ponto, marcas.escuta());
    registrar(&servidor, ponto, marcas.servidor());
    (escuta, servidor, marcas)
}

#[test]
fn a_chamada_vira_chamado_na_escuta_e_aqui_para_quem_liga() {
    let (ponto, _) = ponto_no_ar();
    let (escuta, servidor, marcas) = anfitriao(ponto);

    let quem_liga = socket();
    let candidato: SocketAddr = "127.0.0.1:50000".parse().unwrap();
    quem_liga
        .send_to(&chama(&um_chama(&marcas, vec![candidato])), ponto)
        .unwrap();

    let Resposta3::Chamado(chamado) = resposta(&escuta) else {
        panic!("a escuta tinha de ouvir o CHAMADO");
    };
    assert_eq!(&chamado.escuta, marcas.escuta());
    assert_eq!(chamado.credencial, credencial());
    assert!(
        chamado.candidatos.contains(&candidato),
        "o candidato de quem liga vai junto"
    );
    assert!(
        chamado
            .candidatos
            .contains(&quem_liga.local_addr().unwrap()),
        "e o endereço que o ponto viu também"
    );

    let dica: SocketAddr = "192.168.0.4:8383".parse().unwrap();
    escuta
        .send_to(
            &atende(&Atende {
                escuta: marcas.escuta().clone(),
                id: chamado.id,
                endereco: servidor.local_addr().unwrap(),
                dicas: vec![dica],
            }),
            ponto,
        )
        .unwrap();

    assert_eq!(
        resposta(&quem_liga),
        Resposta3::Aqui {
            marca: marcas.servidor().clone(),
            endereco: servidor.local_addr().unwrap(),
            dicas: vec![dica],
        }
    );
}

#[test]
fn um_atende_de_outro_ip_ou_com_outro_id_nao_vale() {
    let (ponto, quarto) = ponto_no_ar();
    let (escuta, servidor, marcas) = anfitriao(ponto);
    let quem_liga = socket();
    quem_liga
        .send_to(&chama(&um_chama(&marcas, Vec::new())), ponto)
        .unwrap();
    let Resposta3::Chamado(chamado) = resposta(&escuta) else {
        panic!("a escuta tinha de ouvir o CHAMADO");
    };
    // Outro id, da escuta mesmo: calado, e a chamada continua esperando.
    let mut outro = chamado.id;
    outro[0] ^= 1;
    escuta
        .send_to(
            &atende(&Atende {
                escuta: marcas.escuta().clone(),
                id: outro,
                endereco: servidor.local_addr().unwrap(),
                dicas: Vec::new(),
            }),
            ponto,
        )
        .unwrap();
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(quarto.chamadas().quantas(), 1);
    // Sem `ATENDE` certo, quem liga ouve `CALADO` no prazo.
    let antes = Instant::now();
    assert_eq!(
        resposta(&quem_liga),
        Resposta3::Calado {
            marca: marcas.servidor().clone()
        }
    );
    assert!(antes.elapsed() < Duration::from_secs(4));
    assert_eq!(quarto.chamadas().quantas(), 0);
}

#[test]
fn quem_liga_para_quem_nao_mora_ouve_ninguem() {
    let (ponto, _) = ponto_no_ar();
    let marcas = Marcas::do_servidor(FP).unwrap();
    let quem_liga = socket();
    quem_liga
        .send_to(&chama(&um_chama(&marcas, Vec::new())), ponto)
        .unwrap();
    assert_eq!(
        resposta(&quem_liga),
        Resposta3::Ninguem {
            marca: marcas.servidor().clone()
        }
    );
}

#[test]
fn o_anfitriao_da_017_nao_e_achado_pelo_enc2_nem_pelo_enc1() {
    // A recusa da linha 1 dita pelo ponto (D6): o registro do ENC/3 mora em
    // quartos próprios.
    let (ponto, _) = ponto_no_ar();
    let (_escuta, servidor, marcas) = anfitriao(ponto);
    let quem_procura = socket();

    quem_procura
        .send_to(&quem3(marcas.servidor()), ponto)
        .unwrap();
    assert_eq!(
        resposta(&quem_procura),
        Resposta3::Aqui {
            marca: marcas.servidor().clone(),
            endereco: servidor.local_addr().unwrap(),
            dicas: Vec::new(),
        }
    );

    quem_procura
        .send_to(&quem2(marcas.servidor()), ponto)
        .unwrap();
    let datagrama = ouvir(&quem_procura);
    assert!(matches!(
        ler_resposta2(&datagrama),
        Some(seele_proto::encontro::Resposta2::Ninguem { .. })
    ));

    quem_procura
        .send_to(&seele_proto::encontro::quem(marcas.servidor()), ponto)
        .unwrap();
    quem_procura
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    let mut balde = [0_u8; 512];
    assert!(
        quem_procura.recv_from(&mut balde).is_err(),
        "o QUEM do ENC/1 cala para um anfitrião da 0.17"
    );
}

#[test]
fn o_stun_binding_devolve_o_endereco_visto() {
    let (ponto, _) = ponto_no_ar();
    let navegador = socket();
    let mut pedido = vec![0x00, 0x01, 0x00, 0x00, 0x21, 0x12, 0xA4, 0x42];
    pedido.extend_from_slice(&[9; 12]);
    navegador.send_to(&pedido, ponto).unwrap();
    let mut balde = [0_u8; 128];
    let (n, _) = navegador.recv_from(&mut balde).unwrap();
    let resposta = &balde[..n];
    assert_eq!(n, 32, "IPv4: cabeçalho e um XOR-MAPPED-ADDRESS");
    assert_eq!(&resposta[0..2], &[0x01, 0x01]);
    let porta = u16::from_be_bytes([resposta[26], resposta[27]]) ^ 0x2112;
    assert_eq!(porta, navegador.local_addr().unwrap().port());
}

fn post(sinal: SocketAddr, corpo: &str, origem: &str) -> (String, String) {
    let mut conexao = TcpStream::connect(sinal).unwrap();
    conexao.set_read_timeout(Some(PRAZO)).unwrap();
    write!(
        conexao,
        "POST /sinal/1 HTTP/1.1\r\nHost: x\r\nOrigin: {origem}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{corpo}",
        corpo.len()
    )
    .unwrap();
    let mut texto = String::new();
    conexao.read_to_string(&mut texto).unwrap();
    let (cabecalho, corpo) = texto.split_once("\r\n\r\n").unwrap();
    (cabecalho.to_owned(), corpo.to_owned())
}

#[test]
fn o_sinal_do_navegador_chama_e_ouve_aqui_com_o_cors_exato() {
    let (ponto, quarto) = ponto_no_ar();
    let sinal = sinal_no_ar(quarto);
    let (escuta, servidor, marcas) = anfitriao(ponto);

    // A escuta atende em outra linha, como o anfitrião de verdade.
    let atendente = std::thread::spawn(move || {
        let Resposta3::Chamado(chamado) = resposta(&escuta) else {
            panic!("a escuta tinha de ouvir o CHAMADO do sinal");
        };
        assert_eq!(
            chamado.candidatos,
            vec!["127.0.0.1:51000".parse::<SocketAddr>().unwrap()],
            "pelo sinal vai só o candidato da página: o ponto não vê o UDP dela"
        );
        escuta
            .send_to(
                &atende(&Atende {
                    escuta: chamado.escuta,
                    id: chamado.id,
                    endereco: servidor.local_addr().unwrap(),
                    dicas: Vec::new(),
                }),
                ponto,
            )
            .unwrap();
        servidor.local_addr().unwrap()
    });

    let corpo = pedido_do_sinal(&um_chama(&marcas, vec!["127.0.0.1:51000".parse().unwrap()]));
    let (cabecalho, resposta) = post(sinal, &corpo, &seele_encontro::sinal::origem_do_produto());
    let do_servidor = atendente.join().unwrap();
    assert!(cabecalho.starts_with("HTTP/1.1 200"), "{cabecalho}");
    assert!(
        cabecalho.contains(&format!(
            "Access-Control-Allow-Origin: {}\r\n",
            seele_encontro::sinal::origem_do_produto()
        )),
        "o CORS devolve a origem exata: {cabecalho}"
    );
    assert_eq!(
        ler_resposta_do_sinal(&resposta),
        Some(RespostaDoSinal::Aqui {
            endereco: do_servidor,
            dicas: Vec::new()
        })
    );

    // Outra origem não lê a resposta.
    let (cabecalho, resposta) = post(sinal, &corpo, "https://outra.exemplo");
    assert!(
        !cabecalho.contains("Access-Control-Allow-Origin"),
        "{cabecalho}"
    );
    assert_eq!(
        ler_resposta_do_sinal(&resposta),
        Some(RespostaDoSinal::Calado)
    );
}

#[test]
fn o_sinal_recusa_o_corpo_torto_e_o_caminho_errado() {
    let (_, quarto) = ponto_no_ar();
    let sinal = sinal_no_ar(quarto);
    let (cabecalho, _) = post(
        sinal,
        "SEELE-SINAL/1 CHAMA a",
        &seele_encontro::sinal::origem_do_produto(),
    );
    assert!(cabecalho.starts_with("HTTP/1.1 400"), "{cabecalho}");
    let grande = "x".repeat(600);
    let (cabecalho, _) = post(sinal, &grande, &seele_encontro::sinal::origem_do_produto());
    assert!(cabecalho.starts_with("HTTP/1.1 413"), "{cabecalho}");
    let mut conexao = TcpStream::connect(sinal).unwrap();
    write!(conexao, "GET / HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
    let mut texto = String::new();
    conexao.read_to_string(&mut texto).unwrap();
    assert!(texto.starts_with("HTTP/1.1 404"), "{texto}");
}

#[test]
fn um_pedido_analisado_pelo_enc3_e_o_mesmo_que_o_sinal_leva() {
    let marcas = Marcas::do_servidor(FP).unwrap();
    let pedido = um_chama(&marcas, vec!["198.51.100.7:51000".parse().unwrap()]);
    assert_eq!(
        analisar3(&chama(&pedido)),
        Some(Pedido3::Chama(pedido.clone()))
    );
    assert_eq!(
        seele_proto::encontro::ler_pedido_do_sinal(&pedido_do_sinal(&pedido)),
        Some(pedido)
    );
}
