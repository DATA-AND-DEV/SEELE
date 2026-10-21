//! **Um servidor para as bancadas do navegador** (Tarefa 2.6 da F4): o
//! `Daemon` de verdade, num endereço local, com o banco na memória.
//!
//!     cargo run -p seele-server --example servidor_da_bancada [pasta-de-um-mod]
//!
//! A primeira linha da saída é um JSON com o que a bancada precisa para entrar
//! pela página: `{"endereco", "impressao", "link"}`, em que `link` é o link do
//! navegador que o desktop daria (`Convite::link_do_navegador`), com os hashes
//! das janelas que valem agora. O servidor fica de pé até a entrada padrão
//! fechar: a bancada o encerra fechando o tubo, e um processo esquecido não
//! segura a porta.
//!
//! **Com um MOD** (a pasta do pacote, a que tem o `mod.json`): o servidor o
//! exige de quem entra, como o `servidor-com-mod` — instalado por hash
//! (`instalar_de_pasta`) e ligado antes da subida, num banco em arquivo numa
//! pasta temporária, para a subida carregar a metade de servidor. A saída
//! ganha `"mod": {"id", "version", "hash"}`.
//!
//! **Não é o `seeled`**: sem arquivo de configuração, sem ponto de encontro. É
//! o mesmo servidor, para a mesma sessão, com os anexos (e o MOD) numa pasta
//! temporária, e nada mais.

#![allow(clippy::print_stdout, reason = "a saída é o contrato com a bancada")]

use std::io::Read as _;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use seele_proto::uri::Convite;
use seele_server::mods::instalar::{instalar_de_pasta, raizes_de_um_mundo};
use seele_server::persistence::mods::{enable, EnabledMod};
use seele_server::persistence::{Location, Persistence};
use seele_server::{Daemon, ServerConfig};

#[tokio::main]
async fn main() -> Result<()> {
    // Os anexos numa pasta própria, apagada ao sair: sem ela, o servidor recusa
    // todo anexo com `Unavailable`, e a bancada não provaria a subida.
    let anexos = std::env::temp_dir().join(format!("servidor-da-bancada-{}", std::process::id()));
    std::fs::create_dir_all(&anexos)?;
    let mut config = ServerConfig {
        name: "Bancada".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        anexos: Some(anexos.clone()),
        ..ServerConfig::default()
    };
    let mut exigido = serde_json::Value::Null;
    if let Some(origem) = std::env::args_os().nth(1) {
        let mundo = anexos.join("mundo");
        let raizes = raizes_de_um_mundo(&mundo);
        let (manifesto, hash) = instalar_de_pasta(std::path::Path::new(&origem), &raizes)?;
        let banco = Location::File(mundo.join("seele.db"));
        enable(
            &Persistence::open(&banco)?,
            &EnabledMod {
                id: manifesto.id.clone(),
                version: manifesto.version.clone(),
                hash: hash.clone(),
                repo: manifesto.repo.clone(),
                reach: manifesto.reach.clone(),
                server_half: manifesto.server.is_some(),
                api: manifesto.api,
                tema: false,
            },
        )?;
        exigido =
            serde_json::json!({ "id": manifesto.id, "version": manifesto.version, "hash": hash });
        config.database = banco;
        config.mods_dir = Some(raizes);
    }
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
            "mod": exigido,
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
    let _ = std::fs::remove_dir_all(&anexos);
    Ok(())
}
