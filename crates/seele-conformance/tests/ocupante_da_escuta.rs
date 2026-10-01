//! Quem ocupa a marca da ESCUTA no quarto não recebe o endereço de quem chega.
//!
//! O I1 da revisão final do Plano 1. A escuta que o quarto dá vira o aviso do
//! `LEVE`, e o `LEVE` sai antes de qualquer aperto de mão: o ponto repassa a
//! esse aviso um `AQUI` com o endereço público de quem chega. A marca da escuta
//! (os 16 primeiros caracteres da impressão digital, e um `e`) está em todo
//! link, e no quarto fica quem escreveu primeiro. Um anfitrião 0.15.0 nunca a
//! registra: a marca dele está sempre livre. A revisão mediu um ocupante
//! recebendo o endereço de quem voltava pela lista em 0,36 s.
//!
//! A regra que fecha o caso barato é `seele_ffi::bilhete_desta_volta`: a escuta
//! só troca o aviso quando mora no IP do servidor que a mesma resposta do
//! quarto deu. A escuta e o servidor de um anfitrião moram na mesma máquina e
//! saem pelo mesmo IP público.
//!
//! # O que estes testes compõem
//!
//! Exatamente o que o `connect` do app faz com o quarto: `onde_mora_hoje` →
//! `bilhete_desta_volta` com o bilhete do link → `Connection::connect`. O
//! `connect` é um comando Tauri e não roda sem casca; que ele use a regra é
//! guardado por texto-fonte em `apps/seele-app/tests/frontend.rs`
//! (`a_escuta_do_quarto_so_vira_aviso_pela_regra_da_ffi`).
//!
//! O anfitrião X é o de um 0.15.0: o servidor dele se registra com a marca
//! `fp16s`, pelo próprio socket do QUIC (o espelho, como `registrar` faz), e a
//! escuta dele não registra a `fp16e`. Quem a registra é o ocupante.
//!
//! # Por que o outro IP é o `::1`
//!
//! O macOS só tem o `127.0.0.1` no loopback IPv4 (o `127.0.0.2` é recusado com
//! «Can't assign requested address»). O outro IP desta máquina é o `::1`. O
//! ponto abre uma escuta por família e as duas dividem o quarto, como em
//! produção (`seele_encontro::Ponto::abrir_com_quarto`). Quem chega, o ocupante
//! e a escuta de X falam com a escuta IPv4, e o servidor de X se registra:
//!
//! - **pela IPv6, de `::1`**, no caso que a regra corta: o ocupante mora noutro
//!   IP que o servidor da mesma resposta;
//! - **pela IPv4, de `127.0.0.1`**, no controle: o ocupante mora no IP do
//!   servidor, como alguém atrás do mesmo NAT, ou quem tomou as duas marcas. Ele
//!   recebe, e é o que prova que o caso de cima mede alguma coisa.
//!
//! # Por que `[::ffff:127.0.0.1]`
//!
//! Pelo mesmo motivo de `volta_pela_trilha.rs`: `enlace::e_publico` pergunta por
//! loopback na forma escrita, então a forma mapeada conta como pública e o
//! `LEVE` sai por ela, mas o pacote ainda chega a um socket desta máquina.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::{Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use seele_ffi::uri::Bilhete;
use seele_ffi::{ConnectConfig, Connection, Trust};
use seele_proto::encontro::{ler_aqui, moro, Marca, Marcas, Vizinhanca, TAMANHO};
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

mod vaga;

/// Quanto se espera um `AQUI` que o `LEVE` faria chegar.
///
/// O `LEVE` sai antes do aperto de mão, e quem o recebe o recebe em
/// milissegundos (a revisão mediu 0,36 s com o aperto de mão inteiro). Três
/// segundos é a espera do controle da revisão.
const ESPERA: Duration = Duration::from_secs(3);

/// Um ponto de verdade com as duas escutas, IPv4 e IPv6, dividindo o quarto.
fn subir_o_ponto() -> (SocketAddr, SocketAddr, Arc<seele_encontro::Quarto>) {
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
    (em_quatro, em_seis, quarto)
}

/// Espera o quarto chegar a `quantos` moradores, ou diz que não chegou.
async fn esperar_o_quarto(quarto: &seele_encontro::Quarto, quantos: usize, quem: &str) {
    let ate = tokio::time::Instant::now() + Duration::from_secs(1);
    while quarto.quantos() < quantos && tokio::time::Instant::now() < ate {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        quarto.quantos(),
        quantos,
        "o registro de {quem} não chegou ao quarto em um segundo: sem ele, este teste não mede \
         a regra"
    );
}

/// O primeiro `AQUI` com a marca do aviso de X que chega a `socket`, e de quem
/// ele fala: é o endereço público de quem chega.
async fn aqui_do_aviso(socket: &tokio::net::UdpSocket, aviso: &Marca) -> Option<SocketAddr> {
    let mut balde = [0_u8; TAMANHO];
    let ate = tokio::time::Instant::now() + ESPERA;
    loop {
        let Ok(Ok((lidos, _))) = tokio::time::timeout_at(ate, socket.recv_from(&mut balde)).await
        else {
            return None;
        };
        if let Some((marca, quem_chega)) = balde.get(..lidos).and_then(ler_aqui) {
            if marca == *aviso {
                return Some(quem_chega);
            }
        }
    }
}

/// O que cada um recebeu do `LEVE` de quem chegou.
struct Recebido {
    /// O que chegou à escuta de X, a do link.
    escuta_de_x: Option<SocketAddr>,
    /// O que chegou ao ocupante da marca da escuta.
    ocupante: Option<SocketAddr>,
}

/// A volta a X, composta como o `connect` do app a compõe, com o servidor de X
/// registrado pela escuta IPv6 do ponto (`servidor_pelo_ipv6`) ou pela IPv4.
async fn voltar_com_o_ocupante(servidor_pelo_ipv6: bool) -> Recebido {
    let (ponto, ponto_seis, quarto) = subir_o_ponto();

    // X, um servidor de verdade, em `[::]` como o de produção abre.
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0)),
        database: Location::Memory,
        ..ServerConfig::default()
    };
    let x = Arc::new(Daemon::bind(config).await.unwrap());
    let porta_de_x = x.local_addr().unwrap().port();
    let fp = x.fingerprint().to_owned();
    let rodando = Arc::clone(&x);
    tokio::spawn(async move {
        let _ = rodando.run().await;
    });
    let marcas = Marcas::do_servidor(&fp).expect("a impressão de X forma marcas");

    // A escuta de avisos de X: é ela que o link de X leva no bilhete.
    let escuta_de_x = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let do_link = Bilhete::novo(
        ponto.to_string(),
        escuta_de_x.local_addr().unwrap().to_string(),
    )
    .unwrap();

    // O ocupante: só precisa dos 16 primeiros caracteres da impressão, que
    // estão em todo link. A resposta ao `MORO` sai depois do registro.
    let ocupante = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let ocupante_em = ocupante.local_addr().unwrap();
    ocupante
        .send_to(&moro(&marcas.escuta), ponto)
        .await
        .unwrap();
    let mut balde = [0_u8; TAMANHO];
    tokio::time::timeout(Duration::from_secs(1), ocupante.recv_from(&mut balde))
        .await
        .expect("o ponto não respondeu ao MORO do ocupante")
        .unwrap();
    esperar_o_quarto(&quarto, 1, "do ocupante").await;

    // O servidor de X se registra pelo socket do QUIC, como `registrar` faz.
    // O espelho é `[::]`, então o IPv4 vai na forma mapeada.
    let espelho = x.espelho().expect("X tem o espelho do socket do servidor");
    let por_onde = if servidor_pelo_ipv6 {
        ponto_seis
    } else {
        let SocketAddr::V4(quatro) = ponto else {
            panic!("a escuta IPv4 do ponto não está em IPv4: {ponto}");
        };
        SocketAddr::from((quatro.ip().to_ipv6_mapped(), quatro.port()))
    };
    espelho
        .send_to(&moro(&marcas.servidor), por_onde)
        .expect("o MORO do servidor de X sai pelo espelho");
    esperar_o_quarto(&quarto, 2, "do servidor de X").await;

    // A volta, composta como o `connect` do app compõe. O bilhete guardado é o
    // do link, que é também o que a lista guarda dele.
    let achado = seele_ffi::onde_mora_hoje(&ponto.to_string(), &fp).await;
    assert_eq!(
        achado.escuta(),
        Some(ocupante_em),
        "o quarto não deu a escuta do ocupante ({achado:?}): este teste não mede a regra"
    );
    let ip_do_servidor = achado
        .servidor()
        .map(|servidor| servidor.ip().to_canonical());
    let esperado: std::net::IpAddr = if servidor_pelo_ipv6 {
        Ipv6Addr::LOCALHOST.into()
    } else {
        [127, 0, 0, 1].into()
    };
    assert_eq!(
        ip_do_servidor,
        Some(esperado),
        "o quarto não deu o servidor de X no IP em que ele se registrou ({achado:?}): este \
         teste não mede a regra"
    );
    let bilhete = seele_ffi::bilhete_desta_volta(&do_link, &achado);

    let casa = tempfile::tempdir().unwrap();
    let config = ConnectConfig {
        server: format!("[::ffff:127.0.0.1]:{porta_de_x}"),
        alternate_servers: Vec::new(),
        nickname: "pessoa".into(),
        home: casa.path().display().to_string(),
        join_secret: None,
        expected_fingerprint: Some(fp.clone()),
        bilhete: Some(bilhete),
        audio: false,
        capture_device: None,
        playback_device: None,
    };
    let entrada = tokio::task::spawn_blocking(move || Connection::connect(config))
        .await
        .unwrap();

    let (escuta_de_x, ocupante) = tokio::join!(
        aqui_do_aviso(&escuta_de_x, &marcas.aviso),
        aqui_do_aviso(&ocupante, &marcas.aviso),
    );
    let (conexao, confianca) = entrada.expect("a entrada em X deu certo");
    assert!(
        matches!(confianca, Trust::FirstContactVerified { .. }),
        "a entrada em X não conferiu a impressão do link ({confianca:?}): este teste não mede o \
         caso da volta"
    );
    conexao.disconnect();
    x.shutdown();
    Recebido {
        escuta_de_x,
        ocupante,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn quem_ocupa_a_marca_da_escuta_de_outro_ip_nao_recebe_quem_chega() {
    let _vaga = vaga::minha();
    let recebido = voltar_com_o_ocupante(true).await;

    assert_eq!(
        recebido.ocupante, None,
        "quem ocupou a marca da escuta, noutro IP que o servidor de X, recebeu do `LEVE` o \
         endereço de quem chega ({:?}), antes de qualquer TLS: a escuta do quarto virou aviso \
         sem o servidor da mesma resposta confirmá-la",
        recebido.ocupante
    );
    assert!(
        recebido.escuta_de_x.is_some(),
        "a escuta de X, a do link, não recebeu o `LEVE`: com a escuta do quarto deixada de \
         lado, o aviso tinha de ser o do bilhete guardado, e X não fura o NAT para quem chega"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn no_ip_do_servidor_a_escuta_do_quarto_recebe_quem_chega() {
    let _vaga = vaga::minha();
    // O controle, e o resíduo: no IP do servidor da mesma resposta, a escuta
    // do quarto vira o aviso. É o anfitrião que mudou de porta, ou quem tomou
    // as duas marcas (até o SEELE-ENC/2, Plano 4).
    let recebido = voltar_com_o_ocupante(false).await;

    assert!(
        recebido.ocupante.is_some(),
        "a escuta do quarto, no IP do servidor da mesma resposta, não recebeu o `LEVE`: ou a \
         regra recusa a escuta do anfitrião de verdade, ou este arranjo não faz o `LEVE` \
         chegar, e o outro teste deste arquivo passa sem medir nada"
    );
    assert_eq!(
        recebido.escuta_de_x, None,
        "a escuta do link recebeu o `LEVE` com a escuta do quarto confirmada: o aviso não foi \
         trocado pela escuta de hoje"
    );
}
