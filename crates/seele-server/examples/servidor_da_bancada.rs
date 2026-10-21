//! **Um servidor para as bancadas do navegador** (Tarefa 2.6 da F4): o
//! `Daemon` de verdade, num endereço local, com o banco na memória.
//!
//!     cargo run -p seele-server --example servidor_da_bancada
//!
//! A primeira linha da saída é um JSON com o que a bancada precisa para entrar
//! pela página: `{"endereco", "impressao", "link"}`, em que `link` é o link do
//! navegador que o desktop daria (`Convite::link_do_navegador`), com os hashes
//! das janelas que valem agora. O servidor fica de pé até a entrada padrão
//! fechar: a bancada o encerra fechando o tubo, e um processo esquecido não
//! segura a porta.
//!
//! **Não é o `seeled`**: sem arquivo de configuração, sem banco em disco, sem
//! ponto de encontro. É o mesmo servidor, para a mesma sessão, e nada mais.

#![allow(clippy::print_stdout, reason = "a saída é o contrato com a bancada")]

use std::io::Read as _;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use seele_proto::uri::Convite;
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

#[tokio::main]
async fn main() -> Result<()> {
    let config = ServerConfig {
        name: "Bancada".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        ..ServerConfig::default()
    };
    let daemon = Arc::new(Daemon::bind(config).await?);
    let endereco = daemon.local_addr()?;
    let agora = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("o relógio desta máquina está antes de 1970")?
        .as_secs();
    let janelas: Vec<_> = daemon
        .janelas_do_navegador()
        .iter()
        .filter(|janela| janela.nao_antes <= agora && agora <= janela.nao_depois)
        .collect();
    let hashes: Vec<String> = janelas
        .iter()
        .map(|janela| {
            janela
                .hash
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect()
        })
        .collect();
    let ate = janelas.iter().map(|janela| janela.nao_depois).max();
    let mut convite = Convite::novo(endereco.to_string());
    convite.impressao_digital = Some(daemon.fingerprint().to_owned());
    let link = convite
        .link_do_navegador(&hashes, ate)
        .map_err(|erro| anyhow::anyhow!("o link do navegador não saiu: {erro:?}"))?;
    println!(
        "{}",
        serde_json::json!({
            "endereco": endereco.to_string(),
            "impressao": daemon.fingerprint(),
            "link": link,
        })
    );

    let atendendo = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = atendendo.run().await;
    });
    // Até a bancada fechar o tubo.
    tokio::task::spawn_blocking(|| {
        let mut resto = Vec::new();
        let _ = std::io::stdin().read_to_end(&mut resto);
    })
    .await?;
    Ok(())
}
