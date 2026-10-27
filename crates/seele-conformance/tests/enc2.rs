//! O registro assinado no ponto de encontro, numa máquina só, nas duas versões
//! que a 0.17 conhece (ADR 0062 e ADR 0063).
//!
//! - **O ENC/3, da 0.17:** a sala se registra assinada e com o cookie
//!   (`MORO3`), pela ponta WebRTC do servidor dela, e quem liga a acha pelo
//!   `QUEM3`.
//! - **O ENC/2, da 0.16, visto pela 0.17:** uma sala 0.16 registra pelo
//!   `MORO2`, e a 0.17 que procura a acha como «está no Servidor 1»
//!   (`LocalizacaoComLinha::NaLinhaUm`, R6 da revisão do coordenador). Não diz
//!   «ninguém mora», e não tenta discar uma linha que não conversa com ela.
//!
//! Até a 0.16, este arquivo provava o ENC/2 com um socket UDP cru no lugar do
//! servidor. Desde a 0.17, o servidor fala pela `Ponta` do enlace (STUN, DTLS
//! e o ENC/3 no mesmo socket), e é por ela que a sala registra. O caso da 0.16
//! continua aqui, montado à mão como a 0.16 o manda, porque é o que a 0.17
//! precisa reconhecer.
//!
//! Aqui, e não num crate só: o ADR 0002 não deixa o anfitrião, o ponto e quem
//! liga se verem. Todo ponto destes testes é `127.0.0.1`, e os guardas das duas
//! pontas reprovariam um teste que perguntasse à VPS de produção.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use ed25519_dalek::{Signer, SigningKey};
use seele_core::encontro::{
    localizar, localizar3, localizar_com_a_linha, onde_mora_hoje, Localizacao, LocalizacaoComLinha,
    PRAZO_DO_QUARTO,
};
use seele_enlace::Ponta;
use seele_proto::codigo::Codigo;
use seele_proto::encontro::{Marcas, Vizinhanca, TAMANHO, TAMANHO2};
use seele_server::alcance::encontro::{abrir, Convocacao, EstadoDoRegistro, PeloServidor};
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

/// O socket do servidor da sala, como o `seeled` da 0.17 o tem: uma `Ponta`
/// do enlace, com a caixa do ENC/3 que chega a ela.
async fn pelo_servidor() -> PeloServidor {
    let ponta = Ponta::abrir(SocketAddr::from(([127, 0, 0, 1], 0)), None)
        .await
        .expect("a ponta abre em 127.0.0.1");
    let enc = Arc::new(tokio::sync::Mutex::new(
        ponta.mensagens_enc().expect("a caixa do ENC/3"),
    ));
    PeloServidor { ponta, enc }
}

#[tokio::test]
async fn quem_liga_acha_a_sala_que_se_registrou_assinada_pelo_enc3() {
    let _vaga = vaga::minha();
    let ponto = subir_o_ponto();
    let chave = SigningKey::from_bytes(&[11; 32]);
    let marcas = marcas_de(&chave);
    let pelo = pelo_servidor().await;
    let servidor = pelo.ponta.local_addr();
    let porta = servidor.port();

    let convocacao = Convocacao::para_sala(
        pelo,
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

    let achado = localizar3(&ponto.to_string(), &marcas, PRAZO_DO_QUARTO).await;
    let Localizacao::Achada {
        servidor: Some(onde),
        escuta: Some(_),
        dicas,
    } = achado
    else {
        panic!("quem liga não achou a sala pelo QUEM3: {achado:?}");
    };
    assert_eq!(
        onde, servidor,
        "o QUEM3 deu outro endereço que o da ponta do servidor da sala"
    );
    assert!(
        dicas.iter().all(|dica| dica.port() == porta),
        "uma dica de rede local não está na porta do servidor da sala: {dicas:?}"
    );
    // E a 0.17 que pergunta as duas versões diz que é da linha 2.
    assert!(matches!(
        localizar_com_a_linha(&ponto.to_string(), &marcas, PRAZO_DO_QUARTO).await,
        LocalizacaoComLinha::Achada(_)
    ));
    // **A sala 0.17 não publica o MORO2** (ADR 0063): pelo QUEM2, uma 0.16
    // não a acha, e não recebe um endereço que não conversaria com ela.
    assert!(
        !matches!(
            localizar(&ponto.to_string(), &marcas, PRAZO_DO_QUARTO).await,
            Localizacao::Achada { .. }
        ),
        "a sala da 0.17 apareceu no ENC/2, onde a 0.16 a procura"
    );
    encontro.fechar();
}

/// **Uma sala da 0.16, registrada pelo `MORO2`, é dita «está no Servidor 1»**
/// pela 0.17 que a procura (R6), e não «ninguém mora».
///
/// O registro é montado como a 0.16 o manda: o `MORO2` assinado pela chave da
/// sala, primeiro sem cookie, e de novo com o cookie do `DESAFIO`.
#[tokio::test]
async fn uma_sala_da_0_16_e_dita_como_do_servidor_1() {
    let _vaga = vaga::minha();
    let ponto = subir_o_ponto();
    let chave = SigningKey::from_bytes(&[13; 32]);
    let marcas = marcas_de(&chave);
    let sala_016 = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let carimbo = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let assinar = |cookie| {
        let mensagem = seele_proto::assinatura::moro2(
            marcas.servidor(),
            &chave.verifying_key().to_bytes(),
            carimbo,
            &[],
        );
        seele_proto::encontro::moro2(&seele_proto::encontro::Moro2 {
            marca: marcas.servidor().clone(),
            chave: chave.verifying_key().to_bytes(),
            carimbo,
            dicas: vec![],
            assinatura: chave.sign(&mensagem).to_bytes(),
            cookie,
        })
    };
    sala_016.send_to(&assinar(None), ponto).await.unwrap();
    let mut balde = [0_u8; TAMANHO2];
    let (lidos, _) = tokio::time::timeout(Duration::from_secs(2), sala_016.recv_from(&mut balde))
        .await
        .expect("o ponto não respondeu ao MORO2")
        .unwrap();
    let Some(seele_proto::encontro::Resposta2::Desafio { cookie, .. }) =
        seele_proto::encontro::ler_resposta2(balde.get(..lidos).unwrap_or_default())
    else {
        panic!("o ponto não desafiou o primeiro MORO2");
    };
    sala_016
        .send_to(&assinar(Some(cookie)), ponto)
        .await
        .unwrap();
    let (lidos, _) = tokio::time::timeout(Duration::from_secs(2), sala_016.recv_from(&mut balde))
        .await
        .expect("o ponto não respondeu ao MORO2 com o cookie")
        .unwrap();
    assert!(
        matches!(
            seele_proto::encontro::ler_resposta2(balde.get(..lidos).unwrap_or_default()),
            Some(seele_proto::encontro::Resposta2::Morou { .. })
        ),
        "o ponto não guardou o MORO2 da 0.16"
    );

    assert_eq!(
        localizar_com_a_linha(&ponto.to_string(), &marcas, PRAZO_DO_QUARTO).await,
        LocalizacaoComLinha::NaLinhaUm,
        "a 0.17 não disse que a sala está no Servidor 1"
    );
}

#[tokio::test]
async fn uma_sala_que_nao_se_registrou_e_ninguem_mora_e_nao_ponto_mudo() {
    let _vaga = vaga::minha();
    let ponto = subir_o_ponto();
    let marcas = marcas_de(&SigningKey::from_bytes(&[12; 32]));
    assert_eq!(
        localizar3(&ponto.to_string(), &marcas, PRAZO_DO_QUARTO).await,
        Localizacao::NinguemMora
    );
    assert!(matches!(
        localizar_com_a_linha(&ponto.to_string(), &marcas, PRAZO_DO_QUARTO).await,
        LocalizacaoComLinha::NaoAchou(Localizacao::NinguemMora)
    ));
}

#[tokio::test]
async fn um_servidor_guardado_continua_achado_pelo_enc1_no_mesmo_ponto() {
    // A versão 1 ao lado das outras, contra o mesmo ponto: o `quarto.rs` inteiro
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
