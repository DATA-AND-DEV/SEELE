//! O ATENDER pela ponte (tarefas 22, 23 e 27 da F7): a semente da sala e o
//! código desta máquina saem da FFI, da mesma chave, e a conexão que atende
//! confere o código.
//!
//! O que se prova aqui, de ponta a ponta e pelo caminho do app:
//!
//! - a sala que sobe com `seele_ffi::semente_da_sala` é atendida por uma
//!   conexão da FFI que confere `seele_ffi::codigo_desta_maquina`
//!   (`ConnectConfig::codigo_esperado`): as duas leituras são da mesma chave;
//! - a mesma conexão, diante de uma sala de outra chave, cai dentro do TLS com
//!   «o código não bate», e não entra: o campo confere, e não só existe;
//! - a casca fica sabendo que o código conferiu: o primeiro contato com a
//!   própria sala chega como conferido, e não como um primeiro contato cego.
//!
//! Nenhum ponto de encontro: a sala sobe sem registro (`None`), como nos outros
//! testes da sala.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::time::Duration;

use seele_ffi::{ConnectConfig, Connection, ConnectionError, Trust};
use seele_server::hospedagem::Hospedagem;

mod vaga;

/// Uma pasta de dados com a chave desta «máquina» dentro, e nada mais.
fn casa_com_a_chave(semente: u8) -> tempfile::TempDir {
    let casa = tempfile::tempdir().expect("a pasta de teste não abriu");
    std::fs::write(casa.path().join("identity.key"), [semente; 32])
        .expect("a chave de teste não foi gravada");
    casa
}

/// `127.0.0.1` na porta da sala: ela escuta em `[::]`.
fn local(sala: &Hospedagem) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], sala.endereco().port()))
}

/// A configuração do ATENDER: a própria sala, conferida pelo próprio código.
fn atender(casa: &tempfile::TempDir, sala: SocketAddr) -> ConnectConfig {
    let home = casa.path().display().to_string();
    ConnectConfig {
        server: sala.to_string(),
        alternate_servers: Vec::new(),
        nickname: "dona".into(),
        codigo_esperado: Some(
            seele_ffi::codigo_desta_maquina(&home).expect("o código desta máquina não saiu"),
        ),
        home,
        join_secret: None,
        expected_fingerprint: None,
        bilhete: None,
        // Não há placa de som numa máquina de integração contínua.
        audio: false,
        capture_device: None,
        playback_device: None,
    }
}

#[tokio::test]
async fn quem_recebe_atende_a_propria_sala_pela_ponte_conferindo_o_proprio_codigo() {
    let _vaga = vaga::minha();
    let casa = casa_com_a_chave(1);
    let home = casa.path().display().to_string();
    let semente = seele_ffi::semente_da_sala(&home).expect("a semente da sala não saiu");
    let sala = Hospedagem::iniciar_sala(0, semente.bytes(), "dona", None)
        .await
        .expect("a sala não subiu");
    drop(semente);

    // Duas vezes: a primeira é o primeiro contato com a própria sala, e a
    // segunda já tem a chave inteira pinada sob `@CÓDIGO`.
    for (vez, esperado) in [(1, "conferido"), (2, "conhecido")] {
        let config = atender(&casa, local(&sala));
        let entrada = tokio::task::spawn_blocking(move || Connection::connect(config))
            .await
            .expect("a linha da conexão caiu");
        let (conexao, confianca) = entrada.unwrap_or_else(|erro| {
            panic!(
                "quem recebe não atendeu a própria sala pela ponte ({vez}ª vez): {erro:?}. A \
                 semente e o código saíram de chaves diferentes, ou a ponte não confere o código"
            )
        });
        // **O código conferiu, e a casca fica sabendo.** Um `FirstContact` aqui
        // faria a tela perguntar «confia nesta chave?» na própria sala.
        let como = match confianca {
            Trust::FirstContactVerified { .. } => "conferido",
            Trust::Known => "conhecido",
            outro => panic!("a {vez}ª entrada na própria sala chegou à casca como {outro:?}"),
        };
        assert_eq!(como, esperado, "a {vez}ª entrada na própria sala");
        let despediu =
            tokio::task::spawn_blocking(move || conexao.encerrar_e_esperar(Duration::from_secs(5)))
                .await
                .expect("a linha da despedida caiu");
        assert!(despediu, "a conexão que atendeu não se despediu em 5 s");
    }
    sala.encerrar().await;
}

#[tokio::test]
async fn o_atender_diante_de_uma_sala_de_outra_chave_nao_entra() {
    let _vaga = vaga::minha();
    let minha = casa_com_a_chave(1);
    let sala = Hospedagem::iniciar_sala(0, &[2; 32], "outra", None)
        .await
        .expect("a sala não subiu");

    let config = atender(&minha, local(&sala));
    let entrada = tokio::task::spawn_blocking(move || Connection::connect(config))
        .await
        .expect("a linha da conexão caiu");

    match entrada {
        Err(ConnectionError::CodigoNaoBate { esperado, veio }) => {
            assert_ne!(Some(esperado), veio, "a recusa não diz que código veio");
        }
        Err(outro) => panic!("a sala de outra chave caiu por outro motivo: {outro:?}"),
        Ok(_) => {
            panic!("o ATENDER entrou numa sala de outra chave: o código esperado não foi conferido")
        }
    }
    sala.encerrar().await;
}
