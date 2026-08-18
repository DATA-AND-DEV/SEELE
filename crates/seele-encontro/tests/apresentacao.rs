//! O degrau 4 do ADR 0022, ponta a ponta, numa máquina só.
//!
//! O que **não** dá para provar aqui é o furo em si: para isso são precisas
//! duas máquinas atrás de NAT diferentes, e o teste que precisa disso está em
//! `docs/teste-duas-maquinas.md`. O que dá para provar é tudo o que vem antes
//! dele e decide se ele acontece — que o anfitrião descobre o próprio endereço
//! público, que o aviso de quem quer entrar chega **com o endereço para onde
//! furar**, e que o ponto de encontro faz as duas coisas sem guardar nada.
//!
//! O loopback obriga a `Vizinhanca::TambemAqui`, e essa é a razão de aquele
//! modo existir: um mecanismo que só pode ser experimentado com a internet
//! inteira montada é um que ninguém confere antes de apontar o mundo para ele.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "teste: uma expectativa quebrada é justamente o que se quer ver falhar"
)]

use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

use seele_encontro::{Movimento, Ponto};
use seele_proto::encontro::{ler_aqui, leve, onde, Marca, Vizinhanca, TAMANHO};

/// Quanto um teste espera por um datagrama que atravessa o loopback.
///
/// Generoso de propósito: aqui um datagrama leva microssegundos, e o prazo
/// existe só para que uma falha apareça como uma asserção e não como um teste
/// pendurado para sempre.
const PRAZO: Duration = Duration::from_secs(2);

/// Um socket de teste no loopback, com prazo de leitura.
fn socket() -> UdpSocket {
    let socket = UdpSocket::bind("127.0.0.1:0").expect("o loopback não abriu");
    socket
        .set_read_timeout(Some(PRAZO))
        .expect("sem prazo de leitura");
    socket
}

/// Um ponto de encontro atendendo numa linha de execução própria.
fn ponto_no_ar() -> SocketAddr {
    let ponto = Ponto::abrir("127.0.0.1:0".parse().unwrap(), Vizinhanca::TambemAqui)
        .expect("o ponto de encontro não subiu");
    let endereco = ponto.endereco().expect("sem endereço");
    std::thread::spawn(move || {
        let _ = ponto.servir();
    });
    endereco
}

fn marca(texto: &str) -> Marca {
    Marca::nova(texto).expect("marca de teste inválida")
}

/// Que nada chegou, e sem esperar os dois segundos por isso.
///
/// Um prazo curto de propósito: aqui a espera é a asserção, e no loopback um
/// datagrama que ia chegar já chegou.
fn nada_chega(socket: &UdpSocket) -> bool {
    socket
        .set_read_timeout(Some(Duration::from_millis(300)))
        .expect("sem prazo de leitura");
    let vazio = receber(socket).is_none();
    socket.set_read_timeout(Some(PRAZO)).expect("sem prazo");
    vazio
}

fn receber(socket: &UdpSocket) -> Option<Vec<u8>> {
    let mut balde = [0_u8; TAMANHO];
    let (lidos, _) = socket.recv_from(&mut balde).ok()?;
    Some(balde[..lidos].to_vec())
}

#[test]
fn o_anfitriao_descobre_o_proprio_endereco_publico() {
    // A metade do degrau 4 que não é o furo: ninguém atrás de NAT sabe o
    // endereço que os outros veem, e sem essa resposta não há o que pôr no
    // convite. No loopback o endereço "público" é o próprio, e é justamente
    // isso que torna a asserção exata.
    let ponto = ponto_no_ar();
    let anfitriao = socket();
    let meu = anfitriao.local_addr().unwrap();

    anfitriao.send_to(&onde(&marca("umamarca")), ponto).unwrap();
    let resposta = receber(&anfitriao).expect("o ponto de encontro não respondeu ao ONDE");

    assert_eq!(ler_aqui(&resposta), Some((marca("umamarca"), meu)));
}

#[test]
fn o_aviso_chega_ao_anfitriao_com_o_endereco_para_onde_furar() {
    // A apresentação inteira, e é ela que faz o degrau 4 existir: quem tem o
    // convite bate no ponto de encontro, e o anfitrião recebe **o endereço de
    // quem bateu**. É para lá que ele manda os pacotes que abrem o caminho.
    let ponto = ponto_no_ar();
    let anfitriao = socket();
    let visitante = socket();
    let campainha = anfitriao.local_addr().unwrap();
    let de_onde = visitante.local_addr().unwrap();

    // Os primeiros dígitos da impressão digital do Dogma: está no `seele://` e
    // em nenhum outro lugar, e é assim que o anfitrião sabe que quem bateu tem
    // o link dele.
    let fp = marca("3cbcfb0212da738f");
    visitante.send_to(&leve(campainha, &fp), ponto).unwrap();

    let aviso = receber(&anfitriao).expect("o anfitrião não foi avisado de que alguém quer entrar");
    assert_eq!(
        ler_aqui(&aviso),
        Some((fp, de_onde)),
        "o aviso chegou sem o endereço para onde furar, que é a única coisa que ele carrega"
    );

    // E nada volta para quem bateu: o visitante não precisa de resposta
    // nenhuma, e é por isso que um ponto de encontro hostil não tem como
    // mandá-lo para lugar nenhum.
    assert!(
        nada_chega(&visitante),
        "o ponto de encontro respondeu a quem só pediu para avisar outro"
    );
}

#[test]
fn a_apresentacao_funciona_sem_ninguem_ter_se_cadastrado_antes() {
    // «Sem estado» cobrado de fora: este visitante bate num ponto de encontro
    // que nunca ouviu falar dele nem do anfitrião — não houve `ONDE` nenhum
    // antes —, e a apresentação acontece igual. Não há cadastro para consultar,
    // então não há como haver um cadastro faltando.
    let ponto = ponto_no_ar();
    let anfitriao = socket();
    let visitante = socket();

    visitante
        .send_to(&leve(anfitriao.local_addr().unwrap(), &marca("abc")), ponto)
        .unwrap();
    assert!(
        receber(&anfitriao).is_some(),
        "a apresentação exigiu um cadastro que este protocolo não tem"
    );
}

#[test]
fn um_ponto_de_encontro_reiniciado_responde_igual() {
    // A outra face do mesmo: se houvesse estado, o segundo processo responderia
    // diferente do primeiro. Dois `Ponto` distintos, um depois do outro, e a
    // mesma resposta byte a byte.
    let anfitriao = socket();
    let pergunta = onde(&marca("abc"));

    let mut respostas = Vec::new();
    for _ in 0..2 {
        let ponto = ponto_no_ar();
        anfitriao.send_to(&pergunta, ponto).unwrap();
        respostas.push(receber(&anfitriao).expect("sem resposta"));
    }
    assert_eq!(respostas[0], respostas[1]);
}

#[test]
fn lixo_e_calado_e_nao_recusado() {
    // Responder "não entendi" é dizer que há alguém ali, e dar a quem estiver
    // medindo um pacote de graça por pacote mandado.
    let ponto = ponto_no_ar();
    let alguem = socket();

    for lixo in [&b"oi"[..], &[0_u8; TAMANHO][..], &[b'A'; 400][..]] {
        alguem.send_to(lixo, ponto).unwrap();
        assert!(
            nada_chega(&alguem),
            "o ponto de encontro respondeu a {} bytes de lixo",
            lixo.len()
        );
    }
}

#[test]
fn um_datagrama_atendido_diz_quem_foi_apresentado_a_quem() {
    // O `Movimento` existe para o operador poder ver o serviço funcionando sem
    // ligar o registro de metadado. Aqui ele é a forma de afirmar as duas
    // saídas possíveis sem depender de tempo.
    let ponto = Ponto::abrir("127.0.0.1:0".parse().unwrap(), Vizinhanca::TambemAqui).unwrap();
    let onde_atende = ponto.endereco().unwrap();
    let anfitriao = socket();
    let visitante = socket();

    visitante
        .send_to(
            &leve(anfitriao.local_addr().unwrap(), &marca("abc")),
            onde_atende,
        )
        .unwrap();
    assert_eq!(
        ponto.atender().unwrap(),
        Movimento::Levado {
            de: visitante.local_addr().unwrap(),
            para: anfitriao.local_addr().unwrap(),
        }
    );

    visitante
        .send_to(b"nao sou desse protocolo", onde_atende)
        .unwrap();
    assert_eq!(
        ponto.atender().unwrap(),
        Movimento::Calado {
            de: visitante.local_addr().unwrap()
        }
    );
}
