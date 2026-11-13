//! O ATENDER (tarefa 27 da F7) entra na própria sala conferindo o próprio
//! código, e não uma impressão: a conferência é a do `ConfereCodigo` do
//! núcleo, e é este campo do `ConnectConfig` que a escolhe.

use super::{build_destino, quarto_de, ConnectConfig};
use seele_core::enlace::Esperada;
use seele_core::Codigo;

fn codigo() -> Codigo {
    Codigo::da_chave(&[7; 32])
}

fn config(impressao: Option<String>, codigo: Option<Codigo>) -> ConnectConfig {
    ConnectConfig {
        server: "127.0.0.1:8385".into(),
        alternate_servers: Vec::new(),
        nickname: "pessoa".into(),
        home: String::new(),
        join_secret: None,
        expected_fingerprint: impressao,
        codigo_esperado: codigo,
        bilhete: None,
        audio: false,
        capture_device: None,
        playback_device: None,
    }
}

fn esperada(config: &ConnectConfig) -> Option<Esperada> {
    build_destino(
        config,
        "127.0.0.1:8385".parse().unwrap(),
        "localhost",
        "127.0.0.1:8385",
        "127.0.0.1:8385",
    )
    .esperada
}

#[test]
fn o_codigo_esperado_vira_a_esperada_de_cada_destino() {
    assert_eq!(
        esperada(&config(None, Some(codigo()))),
        Some(Esperada::Codigo(codigo())),
        "o ATENDER entraria na própria sala sem conferir quem atende"
    );
}

#[test]
fn com_o_codigo_e_a_impressao_vale_o_codigo() {
    assert_eq!(
        esperada(&config(Some("ab".repeat(32)), Some(codigo()))),
        Some(Esperada::Codigo(codigo()))
    );
}

#[test]
fn sem_o_codigo_a_impressao_segue_como_era() {
    assert_eq!(
        esperada(&config(Some("ab".repeat(32)), None)),
        Some(Esperada::Certificado("ab".repeat(32)))
    );
    assert_eq!(esperada(&config(None, None)), None);
}

/// O quarto é perguntado pela impressão que a conexão confere. Com o
/// código valendo, não há impressão conferida, e um quarto perguntado por
/// ela apontaria um servidor que o TLS não aceita.
#[test]
fn o_quarto_so_e_perguntado_pela_impressao_que_a_conexao_confere() {
    assert!(quarto_de(&config(Some("ab".repeat(32)), None), "127.0.0.1:8385").is_some());
    assert!(
        quarto_de(
            &config(Some("ab".repeat(32)), Some(codigo())),
            "127.0.0.1:8385"
        )
        .is_none(),
        "com o código valendo, o quarto seria perguntado por uma impressão que ninguém confere"
    );
    assert!(quarto_de(&config(None, None), "127.0.0.1:8385").is_none());
}

/// O `Debug` é escrito à mão: um campo novo que ele esquecesse sumiria do
/// `seele.log` de quem investiga uma recusa.
#[test]
fn o_debug_mostra_o_codigo_esperado() {
    let impresso = format!("{:?}", config(None, Some(codigo())));
    assert!(
        impresso.contains(&format!("codigo_esperado: Some({:?})", codigo())),
        "o `Debug` de `ConnectConfig` não mostra o código esperado:\n{impresso}"
    );
}
