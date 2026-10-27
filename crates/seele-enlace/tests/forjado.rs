//! Critérios da revisão para o BLOQUEANTE 3: uma sessão autenticada ativa é dona
//! da sua 5-tupla. Nada que chegue da mesma origem — um Binding com U novo (o U
//! não é segredo), um registro DTLS atrasado ou forjado — a derruba, a interrompe
//! ou rouba o DTLS dela. Os pacotes forjados saem do MESMO socket da sessão
//! (`Ponta::enviar_bruto`), que é o caso mais forte de «mesma origem».

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    missing_docs
)]

use std::net::SocketAddr;
use std::time::Duration;

use hmac::Mac;
use seele_enlace::{CertificadoDtls, Conexao, ConfigDoAnfitriao, Discagem, Pino, Ponta};

fn hmac_sha1(chave: &[u8], partes: &[&[u8]]) -> [u8; 20] {
    let mut mac = <hmac::Hmac<sha1::Sha1> as Mac>::new_from_slice(chave).unwrap();
    for parte in partes {
        mac.update(parte);
    }
    mac.finalize().into_bytes().into()
}

fn binding(u: &str) -> Vec<u8> {
    let usuario = format!("{u}:{u}");
    let mensagem = str0m::ice::StunMessage::binding_request(
        &usuario,
        str0m::ice::TransId::new(),
        true,
        1,
        1000,
        true,
    );
    let mut buf = vec![0_u8; 512];
    let n = mensagem
        .to_bytes(Some(u.as_bytes()), &mut buf, hmac_sha1)
        .unwrap();
    buf.truncate(n);
    buf
}

/// Registros DTLS 1.2 com cabeçalho plausível e corpo de lixo: dados de aplicação
/// na época 1, um ClientHello novo na época 0, e um alerta fatal.
fn registros_dtls_forjados() -> Vec<Vec<u8>> {
    let registro = |tipo: u8, epoca: u16, corpo: &[u8]| {
        let mut r = vec![tipo, 0xfe, 0xfd];
        r.extend_from_slice(&epoca.to_be_bytes());
        r.extend_from_slice(&[0, 0, 0, 0, 0, 7]);
        r.extend_from_slice(&u16::try_from(corpo.len()).unwrap().to_be_bytes());
        r.extend_from_slice(corpo);
        r
    };
    vec![
        registro(23, 1, &[0xAB; 64]),
        registro(22, 0, &[1, 0, 0, 40, 0, 0, 0, 0, 0, 0, 0, 40, 0xfe, 0xfd]),
        registro(21, 1, &[2, 40]),
        registro(21, 0, &[2, 40]),
    ]
}

async fn anfitriao() -> (Ponta, CertificadoDtls) {
    let certificado = CertificadoDtls::efemero().unwrap();
    let ponta = Ponta::abrir(
        SocketAddr::from(([127, 0, 0, 1], 0)),
        Some(ConfigDoAnfitriao::novo(certificado.clone())),
    )
    .await
    .unwrap();
    (ponta, certificado)
}

async fn ligar(host: &Ponta, certificado: &CertificadoDtls, cliente: &Ponta) -> (Conexao, Conexao) {
    let discagem = Discagem::nova(
        vec![host.local_addr()],
        Pino::Impressao(certificado.impressao()),
    );
    let (a, b) = tokio::join!(cliente.discar(discagem), host.aceitar());
    (a.unwrap(), b.unwrap())
}

/// O ctl e a voz ainda andam nos dois sentidos.
async fn ainda_conversa(
    a: &Conexao,
    b: &Conexao,
    ctl: &mut (
        (seele_enlace::Envio, seele_enlace::Recebe),
        (seele_enlace::Envio, seele_enlace::Recebe),
    ),
    marca: u8,
) {
    let ((ae, ar), (be, br)) = ctl;
    ae.write_all(&[marca; 8]).await.unwrap();
    let mut buf = [0_u8; 8];
    tokio::time::timeout(Duration::from_secs(3), br.read_exact(&mut buf))
        .await
        .expect("o ctl parou de andar")
        .unwrap();
    assert_eq!(buf, [marca; 8]);
    be.write_all(&[marca; 4]).await.unwrap();
    let mut buf = [0_u8; 4];
    tokio::time::timeout(Duration::from_secs(3), ar.read_exact(&mut buf))
        .await
        .expect("o ctl de volta parou")
        .unwrap();
    a.send_datagram(vec![marca; 60]).unwrap();
    let voz = tokio::time::timeout(Duration::from_secs(3), b.read_datagram())
        .await
        .expect("a voz parou")
        .unwrap();
    assert_eq!(voz, vec![marca; 60]);
    assert!(a.acabou().is_none() && b.acabou().is_none());
}

/// Critério 1: Bindings com U novo forjados da mesma origem não derrubam, não
/// interrompem e não roubam o DTLS de uma sessão ativa.
#[tokio::test]
async fn bindings_com_u_novo_da_mesma_origem_nao_mexem_na_sessao_ativa() {
    let (host, certificado) = anfitriao().await;
    let cliente = Ponta::abrir(SocketAddr::from(([127, 0, 0, 1], 0)), None)
        .await
        .unwrap();
    let (a, b) = ligar(&host, &certificado, &cliente).await;
    let mut ctl = (a.controle().unwrap(), b.controle().unwrap());
    ainda_conversa(&a, &b, &mut ctl, 1).await;

    for _ in 0..20 {
        cliente
            .enviar_bruto(
                host.local_addr(),
                &binding(&seele_enlace::sortear_credencial()),
            )
            .unwrap();
        // E com a credencial da própria sessão, mas sem integridade certa.
        let mut ruim = binding(a.credencial());
        let n = ruim.len();
        ruim[n - 10] ^= 0xFF;
        cliente.enviar_bruto(host.local_addr(), &ruim).unwrap();
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    ainda_conversa(&a, &b, &mut ctl, 2).await;
    assert_eq!(
        host.conexoes_vivas(),
        1,
        "um U forjado criou um par ao lado da sessão ativa"
    );
    assert!(host.contadores().bindings_com_sessao_ativa >= 20);

    // Mesmo depois de dois segundos (o prazo de quem não começa o DTLS), nada muda.
    tokio::time::sleep(Duration::from_millis(2500)).await;
    ainda_conversa(&a, &b, &mut ctl, 3).await;
}

/// Critério 2: um registro DTLS atrasado ou forjado da mesma origem não chega a
/// outra sessão nem derruba a sessão ativa.
#[tokio::test]
async fn registros_dtls_forjados_da_mesma_origem_nao_derrubam_a_sessao() {
    let (host, certificado) = anfitriao().await;
    let cliente = Ponta::abrir(SocketAddr::from(([127, 0, 0, 1], 0)), None)
        .await
        .unwrap();
    let (a, b) = ligar(&host, &certificado, &cliente).await;
    let mut ctl = (a.controle().unwrap(), b.controle().unwrap());
    for _ in 0..10 {
        for registro in registros_dtls_forjados() {
            cliente.enviar_bruto(host.local_addr(), &registro).unwrap();
        }
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    ainda_conversa(&a, &b, &mut ctl, 7).await;
}

/// Critério 4 (R5): duas sessões de sockets diferentes no mesmo anfitrião. O que
/// chega pela origem de uma (forjado, ou o fechamento dela) não toca na outra.
#[tokio::test]
async fn sessoes_de_sockets_diferentes_nao_se_afetam() {
    let (host, certificado) = anfitriao().await;
    let um = Ponta::abrir(SocketAddr::from(([127, 0, 0, 1], 0)), None)
        .await
        .unwrap();
    let dois = Ponta::abrir(SocketAddr::from(([127, 0, 0, 1], 0)), None)
        .await
        .unwrap();
    let (a1, b1) = ligar(&host, &certificado, &um).await;
    let (a2, b2) = ligar(&host, &certificado, &dois).await;
    let mut ctl1 = (a1.controle().unwrap(), b1.controle().unwrap());
    let mut ctl2 = (a2.controle().unwrap(), b2.controle().unwrap());
    for _ in 0..10 {
        um.enviar_bruto(
            host.local_addr(),
            &binding(&seele_enlace::sortear_credencial()),
        )
        .unwrap();
        for registro in registros_dtls_forjados() {
            um.enviar_bruto(host.local_addr(), &registro).unwrap();
        }
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    ainda_conversa(&a1, &b1, &mut ctl1, 4).await;
    ainda_conversa(&a2, &b2, &mut ctl2, 5).await;
    a1.close(9, b"so a um");
    tokio::time::timeout(Duration::from_secs(5), b1.closed())
        .await
        .unwrap();
    ainda_conversa(&a2, &b2, &mut ctl2, 6).await;
    assert_eq!(host.conexoes_vivas(), 1);
}

/// A limitação documentada: com a sessão ainda viva, uma segunda discagem
/// legítima da MESMA porta não abre (o anfitrião a descarta e conta); depois que
/// a primeira fecha, ela abre.
#[tokio::test]
async fn a_segunda_discagem_da_mesma_porta_espera_a_primeira_fechar() {
    let (host, certificado) = anfitriao().await;
    let cliente = Ponta::abrir(SocketAddr::from(([127, 0, 0, 1], 0)), None)
        .await
        .unwrap();
    let (a, b) = ligar(&host, &certificado, &cliente).await;
    let _ctl = (a.controle().unwrap(), b.controle().unwrap());
    let mut segunda = Discagem::nova(
        vec![host.local_addr()],
        Pino::Impressao(certificado.impressao()),
    );
    segunda.prazo = Duration::from_secs(2);
    assert!(
        cliente.discar(segunda).await.is_err(),
        "duas sessões ativas na mesma 5-tupla"
    );
    assert!(host.contadores().bindings_com_sessao_ativa >= 1);
    assert!(
        a.acabou().is_none(),
        "a tentativa recusada derrubou a sessão ativa"
    );

    a.close(0, b"");
    tokio::time::timeout(Duration::from_secs(5), b.closed())
        .await
        .unwrap();
    let (c, d) = ligar(&host, &certificado, &cliente).await;
    assert!(c.acabou().is_none() && d.acabou().is_none());
}

/// Achado medido: um registro DTLS forjado de época 1 repetido, mandado a um par
/// ainda no aperto, faz o dimpl 0.7.6 entrar em pânico (`dtls12/engine.rs:381`).
/// Sem conter o pânico, a tarefa da ponta morria com TODAS as conexões dela.
#[tokio::test]
async fn um_panico_do_dtls_derruba_so_o_par_que_o_provocou() {
    let (host, certificado) = anfitriao().await;
    let legitimo = Ponta::abrir(SocketAddr::from(([127, 0, 0, 1], 0)), None)
        .await
        .unwrap();
    let (a, b) = ligar(&host, &certificado, &legitimo).await;
    let mut ctl = (a.controle().unwrap(), b.controle().unwrap());

    // Quem ataca abre um par pendente com um U qualquer e manda DTLS de lixo.
    let atacante = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    atacante
        .send_to(
            &binding(&seele_enlace::sortear_credencial()),
            host.local_addr(),
        )
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    // Um registro de aperto de época 0 primeiro (o filtro de época deixa passar,
    // como deixa um ClientHello), e depois o mesmo registro de época 1 repetido:
    // é a sequência que bate no `assert_eq!` do dimpl.
    let [dados_e1, aperto_e0, ..] = &registros_dtls_forjados()[..] else {
        unreachable!()
    };
    atacante.send_to(aperto_e0, host.local_addr()).unwrap();
    for _ in 0..3 {
        atacante.send_to(dados_e1, host.local_addr()).unwrap();
    }
    tokio::time::sleep(Duration::from_millis(300)).await;

    // A sessão legítima segue, e a ponta continua aceitando gente nova.
    ainda_conversa(&a, &b, &mut ctl, 8).await;
    let outro = Ponta::abrir(SocketAddr::from(([127, 0, 0, 1], 0)), None)
        .await
        .unwrap();
    let (c, _d) = ligar(&host, &certificado, &outro).await;
    assert!(c.acabou().is_none());
    assert!(
        host.contadores().panicos_contidos >= 1,
        "o pânico do dimpl não aconteceu: o teste não exercitou o que diz"
    );
}

/// Critério 3, e o (c) pela época: fechar e rediscar NA HORA, da mesma porta,
/// oito vezes seguidas. O `close_notify`
/// atrasado da sessão velha chega depois do Binding
/// da nova; ele é de época 1 e não pode fechar o aperto novo.
#[tokio::test]
async fn fechar_e_rediscar_da_mesma_porta_na_hora_sempre_abre() {
    let (host, certificado) = anfitriao().await;
    let cliente = Ponta::abrir(SocketAddr::from(([127, 0, 0, 1], 0)), None)
        .await
        .unwrap();
    // Oito vezes (o número do balde de criações por IP). Apertos que terminam
    // devolvem a vaga do balde, então ele não é mais o teto deste laço; o
    // número só ficou. `portoes.rs` prova o balde.
    for vez in 0..u8::try_from(seele_enlace::CRIACOES_POR_IP).unwrap() {
        let discagem = Discagem::nova(
            vec![host.local_addr()],
            Pino::Impressao(certificado.impressao()),
        );
        let (a, b) = tokio::time::timeout(Duration::from_secs(8), async {
            tokio::join!(cliente.discar(discagem), host.aceitar())
        })
        .await
        .unwrap_or_else(|_| panic!("a rediscagem {vez} não terminou"));
        let a = a.unwrap_or_else(|e| panic!("a rediscagem {vez} falhou: {e}"));
        let b = b.unwrap();
        let mut ctl = (a.controle().unwrap(), b.controle().unwrap());
        ainda_conversa(&a, &b, &mut ctl, vez).await;
        a.close(0, b"");
        tokio::time::timeout(Duration::from_secs(5), b.closed())
            .await
            .unwrap();
    }
}

/// Critério (c), direto: um registro DTLS de época ≥ 1 vindo da origem de um par
/// que ainda não viu o ClientHello desta sessão (o `close_notify` atrasado ou um
/// dado da sessão anterior) não é entregue a ele; um ClientHello de época 0 é.
#[tokio::test]
async fn um_registro_de_epoca_1_antes_do_client_hello_nao_e_entregue() {
    let (host, _) = anfitriao().await;
    let origem = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    origem
        .send_to(
            &binding(&seele_enlace::sortear_credencial()),
            host.local_addr(),
        )
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(host.conexoes_vivas(), 1, "o Binding criou o par pendente");
    let [dados_e1, _aperto_e0, alerta_e1, _alerta_e0] = &registros_dtls_forjados()[..] else {
        unreachable!()
    };
    origem.send_to(dados_e1, host.local_addr()).unwrap();
    origem.send_to(alerta_e1, host.local_addr()).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        host.contadores().dtls_de_outra_sessao,
        2,
        "um registro de época 1 chegou a um par que ainda espera o ClientHello"
    );
}
