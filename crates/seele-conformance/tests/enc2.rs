//! O SEELE-ENC/2 inteiro, numa máquina só (ADR 0062): a sala se registra
//! assinada e com o cookie, o ponto de verdade confere, e quem liga a acha pelo
//! `QUEM2`.
//!
//! Aqui, e não num crate só: o ADR 0002 não deixa o anfitrião, o ponto e quem
//! liga se verem. Todo ponto destes testes é `127.0.0.1`, e os guardas das duas
//! pontas reprovariam um teste que perguntasse à VPS de produção.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use ed25519_dalek::SigningKey;
use seele_core::encontro::{localizar, onde_mora_hoje, Localizacao, PRAZO_DO_QUARTO};
use seele_proto::codigo::Codigo;
use seele_proto::encontro::{Marcas, Vizinhanca, TAMANHO};
use seele_server::alcance::encontro::{abrir, Convocacao, EstadoDoRegistro};
use seele_server::alcance::{Escuta, Pilha};

mod vaga;

/// Um ponto de encontro de verdade, atendendo numa linha própria.
fn subir_o_ponto() -> SocketAddr {
    let ponto = seele_encontro::Ponto::abrir_com_quarto(
        SocketAddr::from(([127, 0, 0, 1], 0)),
        Vizinhanca::TambemAqui,
        Arc::new(seele_encontro::Quarto::novo()),
    )
    .expect("a escuta tem que abrir");
    let onde = ponto.endereco().expect("o socket sabe onde ligou");
    std::thread::spawn(move || {
        let _ = ponto.servir();
    });
    onde
}

fn marcas_de(chave: &SigningKey) -> Marcas {
    Marcas::da_sala(&Codigo::da_chave(&chave.verifying_key().to_bytes()))
}

#[tokio::test]
async fn quem_liga_acha_a_sala_que_se_registrou_assinada() {
    let _vaga = vaga::minha();
    let ponto = subir_o_ponto();
    let chave = SigningKey::from_bytes(&[11; 32]);
    let marcas = marcas_de(&chave);
    let servidor = Arc::new(std::net::UdpSocket::bind("127.0.0.1:0").unwrap());
    let porta = servidor.local_addr().unwrap().port();

    let convocacao = Convocacao::para_sala(
        Arc::clone(&servidor),
        Escuta::nova(porta, Pilha::SoIpv4),
        chave,
        ponto.to_string(),
    );
    let encontro = abrir(&convocacao)
        .await
        .expect("a sala não abriu o degrau 4 contra o ponto local");
    let mut estado = encontro
        .registro()
        .expect("a sala não tem estado de registro");
    let registrado = tokio::time::timeout(
        Duration::from_secs(3),
        estado.wait_for(|e| *e == EstadoDoRegistro::Registrado),
    )
    .await
    .is_ok_and(|visto| visto.is_ok());
    assert!(
        registrado,
        "a sala não se registrou no ponto: {:?}",
        *estado.borrow()
    );

    let achado = localizar(&ponto.to_string(), &marcas, PRAZO_DO_QUARTO).await;
    let Localizacao::Achada {
        servidor: Some(onde),
        escuta: Some(_),
        dicas,
    } = achado
    else {
        panic!("quem liga não achou a sala: {achado:?}");
    };
    assert_eq!(
        onde,
        servidor.local_addr().unwrap(),
        "o QUEM2 deu outro endereço que o do socket do servidor da sala"
    );
    assert!(
        dicas.iter().all(|dica| dica.port() == porta),
        "uma dica de rede local não está na porta do servidor da sala: {dicas:?}"
    );
    encontro.fechar();
}

#[tokio::test]
async fn uma_sala_que_nao_se_registrou_e_ninguem_mora_e_nao_ponto_mudo() {
    let _vaga = vaga::minha();
    let ponto = subir_o_ponto();
    let marcas = marcas_de(&SigningKey::from_bytes(&[12; 32]));
    assert_eq!(
        localizar(&ponto.to_string(), &marcas, PRAZO_DO_QUARTO).await,
        Localizacao::NinguemMora
    );
}

#[tokio::test]
async fn um_servidor_guardado_continua_achado_pelo_enc1_no_mesmo_ponto() {
    // A versão 1 ao lado da 2, contra o mesmo ponto: o `quarto.rs` inteiro
    // continua valendo, e este é o caso curto dele.
    let _vaga = vaga::minha();
    let ponto = subir_o_ponto();
    let marcas =
        Marcas::do_servidor("3cbcfb0212da738f89c156de86eb280adee30fd6b907523b898fedcb2b1de5b9")
            .unwrap();
    let anfitriao = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    anfitriao
        .send_to(&seele_proto::encontro::moro(marcas.servidor()), ponto)
        .await
        .unwrap();
    let mut balde = [0_u8; TAMANHO];
    anfitriao.recv_from(&mut balde).await.unwrap();
    let achado = onde_mora_hoje(&ponto.to_string(), &marcas, PRAZO_DO_QUARTO).await;
    assert_eq!(achado.servidor(), Some(anfitriao.local_addr().unwrap()));
}
