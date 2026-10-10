//! A conexão diz à casca a impressão do servidor que ela conferiu?
//!
//! # Por que este arquivo existe
//!
//! Lote X da F5 da 0.16. A casca precisa responder «estou no meu servidor?»
//! (`onde_estou_em`, no `main.rs` do app) pela **chave**, e não pelo endereço:
//! `127.0.0.1`, `localhost`, o endereço da rede local e o nome público são a
//! mesma hospedagem, e um endereço velho que hoje é de outra máquina não é.
//!
//! O veredito que o `connect` devolve não basta. `Trust::Known`, o caso de toda
//! volta a um servidor já fixado, não leva a impressão, e é justamente o de
//! quem hospeda entrando no próprio servidor pela segunda vez. A impressão
//! existe na FFI em toda conexão que sobe (`PinDecision::FirstContact` ou
//! `Matches`); `Connection::impressao_do_servidor` é por onde ela chega à
//! casca.
//!
//! Medido contra o servidor que o botão HOSPEDAR sobe, e não um `Daemon`
//! montado à mão: a impressão que a casca compara é a de
//! `Hospedagem::impressao_digital`.

use std::sync::Arc;

use anyhow::Result;
use seele_ffi::{ConnectConfig, Connection, ConnectionError};
use seele_server::hospedagem::Hospedagem;
use seele_server::persistence::Location;

mod ambiente;
mod vaga;

fn conectar(alvo: String, casa: String) -> Result<Arc<Connection>, ConnectionError> {
    Connection::connect(ConnectConfig {
        server: alvo,
        alternate_servers: Vec::new(),
        nickname: "anfitria".to_owned(),
        home: casa,
        join_secret: None,
        expected_fingerprint: None,
        bilhete: None,
        audio: false,
        capture_device: None,
        playback_device: None,
    })
    .map(|(connection, _confianca)| connection)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_conexao_diz_a_impressao_do_servidor_que_conferiu() -> Result<()> {
    let _vaga = vaga::minha();
    let _ambiente = ambiente::sem_degrau_4();

    let hospedagem = Hospedagem::iniciar(0, Location::Memory, "Casa", None, |_| Ok(())).await?;
    let porta = hospedagem.endereco().port();
    let esperada = hospedagem.impressao_digital().to_ascii_lowercase();
    let pasta = std::env::temp_dir().join(format!("seele-impressao-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta)?;
    let casa = pasta.join("casa").to_string_lossy().into_owned();

    // Três entradas na mesma hospedagem, com a mesma casa (e o mesmo arquivo de
    // pins). A segunda é a que importa: o pin confere, o veredito é `Known`, e
    // ele não leva impressão nenhuma.
    let entradas = [
        ("no primeiro contato", format!("127.0.0.1:{porta}")),
        (
            "na volta com o pin que confere (`Known`)",
            format!("127.0.0.1:{porta}"),
        ),
        (
            "por outro nome da mesma máquina",
            format!("localhost:{porta}"),
        ),
    ];
    for (qual, alvo) in entradas {
        let connection = {
            let casa = casa.clone();
            tokio::task::spawn_blocking(move || conectar(alvo, casa)).await??
        };
        assert_eq!(
            connection
                .impressao_do_servidor()
                .map(|impressao| impressao.to_ascii_lowercase()),
            Some(esperada.clone()),
            "{qual}, a conexão não disse a impressão do servidor que conferiu: a casca não \
             tem como saber que a sessão é com a hospedagem desta janela, e o sim de quem \
             hospeda vai ao endereço, e não à chave (ADR 0045)"
        );
        let saiu = {
            let connection = Arc::clone(&connection);
            tokio::task::spawn_blocking(move || {
                connection.encerrar_e_esperar(std::time::Duration::from_secs(5))
            })
            .await?
        };
        assert!(
            saiu,
            "{qual}, a conexão não terminou a tempo da entrada seguinte"
        );
    }

    hospedagem.encerrar().await;
    let _ = std::fs::remove_dir_all(&pasta);
    Ok(())
}
