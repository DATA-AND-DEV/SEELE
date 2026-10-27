//! Os portões do anfitrião (R3) e a prova amarrada (D5), contra o enlace de verdade.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    missing_docs
)]

use std::net::SocketAddr;
use std::time::Duration;

use ed25519_dalek::{Signer, SigningKey, Verifier};
use hmac::Mac;
use seele_enlace::{CertificadoDtls, ConfigDoAnfitriao, Discagem, Pino, Ponta};

fn hmac_sha1(chave: &[u8], partes: &[&[u8]]) -> [u8; 20] {
    let mut mac = <hmac::Hmac<sha1::Sha1> as Mac>::new_from_slice(chave).unwrap();
    for parte in partes {
        mac.update(parte);
    }
    mac.finalize().into_bytes().into()
}

/// Um Binding `U:U` com integridade certa, como um navegador mandaria, mas de
/// quem nunca vai seguir para o DTLS.
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

#[tokio::test]
async fn um_binding_sem_dtls_sai_em_dois_segundos_e_um_ip_tem_teto() {
    let (host, _) = anfitriao().await;
    // Vinte origens (portas) do mesmo IP. Da MESMA origem, cada U novo aposenta
    // o pendente anterior (BLOQUEANTE 3), e ali o teto é um.
    let falsarios: Vec<_> = (0..20)
        .map(|_| std::net::UdpSocket::bind("127.0.0.1:0").unwrap())
        .collect();
    for falsario in &falsarios {
        falsario
            .send_to(
                &binding(&seele_enlace::sortear_credencial()),
                host.local_addr(),
            )
            .unwrap();
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        host.conexoes_vivas(),
        seele_enlace::MAX_PENDENTES_POR_IP,
        "um IP só segura {} pendentes",
        seele_enlace::MAX_PENDENTES_POR_IP
    );
    assert!(host.contadores().bindings_recusados >= 12);

    tokio::time::sleep(seele_enlace::ESPERA_PELO_DTLS + Duration::from_millis(700)).await;
    assert_eq!(host.conexoes_vivas(), 0, "quem não começa o DTLS não fica");
    assert_eq!(
        host.contadores().pendentes_sem_dtls,
        seele_enlace::MAX_PENDENTES_POR_IP as u64
    );
}

/// O defeito do spike C: depois de uma falha DTLS, o próximo Binding de
/// consentimento com o mesmo U recriava o `Rtc`. Agora o U vira lápide.
#[tokio::test]
async fn depois_da_falha_dtls_o_mesmo_u_nao_recria_o_par() {
    let (host, _certificado) = anfitriao().await;
    let cli = Ponta::abrir(SocketAddr::from(([127, 0, 0, 1], 0)), None)
        .await
        .unwrap();
    let u = seele_enlace::sortear_credencial();
    let mut discagem = Discagem::nova(
        vec![host.local_addr()],
        Pino::Impressao(CertificadoDtls::efemero().unwrap().impressao()),
    );
    discagem.credencial = Some(u.clone());
    assert!(matches!(
        cli.discar(discagem).await,
        Err(seele_enlace::FalhaAoDiscar::ImpressaoDiferente { .. })
    ));
    // O navegador do spike C continua mandando consentimento com o mesmo U a
    // cada 2–3 s; aqui, a cada 500 ms, por 40 s (mais que a lápide de 30 s e
    // que os ~31 s do prazo do dimpl). O par do anfitrião morre pelo prazo do
    // aperto do enlace (10 s) e nunca mais volta.
    let consentimento = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let inicio = std::time::Instant::now();
    let mut morreu = None;
    while inicio.elapsed() < Duration::from_secs(40) {
        consentimento
            .send_to(&binding(&u), host.local_addr())
            .unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        let vivas = host.conexoes_vivas();
        match morreu {
            None if vivas == 0 => morreu = Some(inicio.elapsed()),
            None => assert!(
                inicio.elapsed() < seele_enlace::APERTO + Duration::from_secs(2),
                "o par sem DTLS passou do prazo do aperto"
            ),
            Some(quando) => assert_eq!(
                vivas, 0,
                "um U em lápide recriou o par {quando:?} depois de morrer"
            ),
        }
    }
    assert!(morreu.is_some());
    let contadores = host.contadores();
    assert_eq!(contadores.pares_criados, 1, "um Rtc só para o U");
    assert!(contadores.bindings_em_lapide >= 40);
}

#[tokio::test]
async fn o_u_de_uma_conexao_viva_nunca_cria_outra_nem_depois_de_fechada() {
    let (host, certificado) = anfitriao().await;
    let cli = Ponta::abrir(SocketAddr::from(([127, 0, 0, 1], 0)), None)
        .await
        .unwrap();
    let discagem = Discagem::nova(
        vec![host.local_addr()],
        Pino::Impressao(certificado.impressao()),
    );
    let (a, b) = tokio::join!(cli.discar(discagem), host.aceitar());
    let (a, b) = (a.unwrap(), b.unwrap());
    let u = a.credencial().to_owned();

    // Viva: um Binding com o U dela, de outra porta, não cria um segundo par.
    let intruso = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    intruso.send_to(&binding(&u), host.local_addr()).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(host.conexoes_vivas(), 1);

    // Fechada: o U fica em lápide.
    a.close(0, b"");
    let _ = tokio::time::timeout(Duration::from_secs(5), b.closed())
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    intruso.send_to(&binding(&u), host.local_addr()).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(host.conexoes_vivas(), 0);
    assert!(host.contadores().bindings_em_lapide >= 1);
}

#[tokio::test]
async fn um_ip_que_falha_seguido_espera_para_tentar_um_u_novo() {
    let (host, _) = anfitriao().await;
    let falsario = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    // Três pendentes que nunca fazem DTLS, de três portas do mesmo IP: três
    // falhas. (Da mesma porta, o U novo aposenta o anterior, e aposentar não é
    // falha: quem redisca da mesma porta não pode cair em espera por isso.)
    let portas: Vec<_> = (0..seele_enlace::FALHAS_ANTES_DA_ESPERA)
        .map(|_| std::net::UdpSocket::bind("127.0.0.1:0").unwrap())
        .collect();
    for origem in &portas {
        origem
            .send_to(
                &binding(&seele_enlace::sortear_credencial()),
                host.local_addr(),
            )
            .unwrap();
    }
    tokio::time::sleep(seele_enlace::ESPERA_PELO_DTLS + Duration::from_millis(600)).await;
    assert_eq!(host.conexoes_vivas(), 0);
    falsario
        .send_to(
            &binding(&seele_enlace::sortear_credencial()),
            host.local_addr(),
        )
        .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(host.conexoes_vivas(), 0, "o IP em espera criou um par");
    assert!(host.contadores().bindings_em_espera >= 1);
}

/// Um laço de U novos da mesma porta, sem DTLS: cada U aposenta o pendente
/// anterior (aposentar não é falha, e o teto por IP nunca enche), então só o
/// balde de criações por IP o para. É o ataque que o balde existe para conter.
#[tokio::test]
async fn um_laco_de_u_novos_sem_aperto_para_no_balde_de_criacoes() {
    let (host, _) = anfitriao().await;
    let falsario = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    for _ in 0..3 * seele_enlace::CRIACOES_POR_IP {
        falsario
            .send_to(
                &binding(&seele_enlace::sortear_credencial()),
                host.local_addr(),
            )
            .unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    let contadores = host.contadores();
    assert_eq!(
        contadores.pares_criados,
        seele_enlace::CRIACOES_POR_IP as u64,
        "o balde deixou passar mais criações que o teto: {contadores:?}"
    );
    assert_eq!(
        contadores.recusa_pelo_balde_de_criacao,
        2 * seele_enlace::CRIACOES_POR_IP as u64,
        "a recusa não foi do balde: {contadores:?}"
    );
    assert_eq!(contadores.recusa_pelo_teto_por_ip, 0, "{contadores:?}");
}

/// Uma casa, um escritório: muitas pessoas atrás do mesmo IP, entrando uma
/// depois da outra e ficando. Cada uma termina o aperto, e um aperto que
/// terminou não é o laço que o balde de criações para — quem limita conexões
/// honestas é quem hospeda (`admitir`, o balde do servidor por endereço).
#[tokio::test]
async fn muitas_pessoas_do_mesmo_ip_em_sequencia_entram_todas() {
    let (host, certificado) = anfitriao().await;
    let mut ligadas = Vec::new();
    for vez in 0..3 * seele_enlace::CRIACOES_POR_IP {
        let cliente = Ponta::abrir(SocketAddr::from(([127, 0, 0, 1], 0)), None)
            .await
            .unwrap();
        let discagem = Discagem::nova(
            vec![host.local_addr()],
            Pino::Impressao(certificado.impressao()),
        );
        let (a, b) = tokio::time::timeout(Duration::from_secs(8), async {
            tokio::join!(cliente.discar(discagem), host.aceitar())
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "a pessoa {vez} não terminou de entrar ({:?})",
                host.contadores()
            )
        });
        let a = a.unwrap_or_else(|e| {
            panic!("a pessoa {vez} foi recusada: {e} ({:?})", host.contadores())
        });
        ligadas.push((cliente, a, b.unwrap()));
    }
    let contadores = host.contadores();
    assert_eq!(contadores.recusa_pelo_balde_de_criacao, 0, "{contadores:?}");
    assert_eq!(host.conexoes_vivas(), 3 * seele_enlace::CRIACOES_POR_IP);
}

/// Um anfitrião que parou de aceitar: a fila de chegadas enche, e cada aperto
/// completo depois disso é encerrado sem chegar a ninguém (nem à portaria de
/// quem hospeda). Esse aperto **não** devolve a vaga do balde: senão o mesmo IP
/// giraria apertos completos sem taxa nenhuma.
#[tokio::test]
async fn com_a_fila_de_chegadas_cheia_apertos_completos_param_no_balde() {
    let (host, certificado) = anfitriao().await;
    let mut clientes = Vec::new();
    let tentativas = 40;
    for _ in 0..tentativas {
        let cliente = Ponta::abrir(SocketAddr::from(([127, 0, 0, 1], 0)), None)
            .await
            .unwrap();
        let discagem = Discagem::nova(
            vec![host.local_addr()],
            Pino::Impressao(certificado.impressao()),
        );
        // Ninguém chama `host.aceitar()`.
        let _ = tokio::time::timeout(Duration::from_millis(1500), cliente.discar(discagem)).await;
        clientes.push(cliente);
        if host.contadores().recusa_pelo_balde_de_criacao > 0 {
            break;
        }
    }
    let contadores = host.contadores();
    assert!(
        contadores.recusa_pelo_balde_de_criacao > 0,
        "{tentativas} apertos completos sem entrega e o balde nunca recusou: {contadores:?}"
    );
    assert!(contadores.pares_criados < tentativas, "{contadores:?}");
}

#[tokio::test]
async fn um_binding_com_integridade_errada_nao_cria_nada() {
    let (host, _) = anfitriao().await;
    let falsario = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let u = seele_enlace::sortear_credencial();
    let mut pacote = binding(&u);
    // Estraga um byte do MESSAGE-INTEGRITY (que fica antes do FINGERPRINT de 8 bytes).
    let n = pacote.len();
    pacote[n - 10] ^= 0xFF;
    falsario.send_to(&pacote, host.local_addr()).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(host.conexoes_vivas(), 0);
    assert_eq!(host.contadores().recusa_pela_integridade, 1);
}

#[tokio::test]
async fn a_prova_amarrada_vale_nesta_conexao_e_nao_em_outra() {
    let (host, certificado) = anfitriao().await;
    let cli = Ponta::abrir(SocketAddr::from(([127, 0, 0, 1], 0)), None)
        .await
        .unwrap();
    let discagem = Discagem::nova(
        vec![host.local_addr()],
        Pino::Impressao(certificado.impressao()),
    );
    let (a, b) = tokio::join!(cli.discar(discagem), host.aceitar());
    let (a, b) = (a.unwrap(), b.unwrap());

    let pessoa = SigningKey::from_bytes(&[7; 32]);
    let nonce = [9_u8; 32];
    // Quem liga assina com a impressão do seu próprio certificado DTLS.
    let assinada = seele_proto::assinatura::prova_de_cliente_2(
        &a.impressao_dtls_do_par(),
        &a.impressao_dtls_local(),
        &nonce,
    );
    let assinatura = pessoa.sign(&assinada);

    // O anfitrião confere com o que o DTLS dele viu: passa.
    let conferida = seele_proto::assinatura::prova_de_cliente_2(
        &certificado.impressao(),
        &b.impressao_dtls_do_par(),
        &nonce,
    );
    assert!(pessoa
        .verifying_key()
        .verify(&conferida, &assinatura)
        .is_ok());

    // A mesma prova repassada por outra conexão (outro certificado DTLS de quem
    // liga, como um intermediário teria) não confere.
    let outro = CertificadoDtls::efemero().unwrap();
    let repassada = seele_proto::assinatura::prova_de_cliente_2(
        &certificado.impressao(),
        &outro.impressao(),
        &nonce,
    );
    assert!(pessoa
        .verifying_key()
        .verify(&repassada, &assinatura)
        .is_err());
}
