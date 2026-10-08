//! M3 da 0.16 — o servidor da medição de custo dos MODs (ADR 0052:189-193).
//!
//! Não é código de produto. O roteiro (`m3.sh`) copia este arquivo para
//! `crates/seele-server/examples/` de uma cópia por `git archive` da
//! v0.15.1 (`54f542f`), e só lá ele compila. O repositório não muda.
//!
//! Faz o que `crates/seele-server/examples/servidor-com-mod.rs` pretende, com
//! três diferenças: instala o pacote por hash (onde o servidor o procura), liga **N** pacotes (0, 1 ou 3) e escuta na porta pedida, para
//! nunca disputar a 8383 (a do app) nem a 8384 (a da bateria de testes).
//!
//! ```text
//! cargo run --release -p seele-server --example m3-servidor -- \
//!     <porta> <mundo-novo> [pacote …]
//! ```
//!
//! Imprime, numa linha só, o endereço, a impressão digital e o hash de cada
//! pacote ligado — o roteiro monta o `seele://` a partir dela.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use anyhow::{Context, Result};
use seele_server::{
    persistence::{
        mods::{enable, EnabledMod},
        Location,
    },
    Daemon, ServerConfig,
};
use std::{net::SocketAddr, path::Path, path::PathBuf};

fn pacote(raiz: &Path, pasta: &Path, arquivos: &mut Vec<(String, Vec<u8>)>) -> Result<()> {
    for entrada in std::fs::read_dir(pasta)? {
        let entrada = entrada?;
        if pasta == raiz && entrada.file_name() == "dados" {
            continue;
        }
        let caminho = entrada.path();
        if caminho.is_dir() {
            pacote(raiz, &caminho, arquivos)?;
        } else {
            arquivos.push((
                caminho.strip_prefix(raiz)?.to_string_lossy().replace('\\', "/"),
                std::fs::read(&caminho)?,
            ));
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // O registro do servidor vai para o stderr, com o filtro de `RUST_LOG`:
    // sem ele, uma metade de servidor que falha some sem dizer nada.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
    let mut args = std::env::args().skip(1);
    let porta: u16 = args.next().context("uso: <porta> <mundo> [pacote …]")?.parse()?;
    let mundo = PathBuf::from(args.next().context("falta o mundo")?);
    anyhow::ensure!(!mundo.exists(), "{} já existe: o mundo é novo a cada cenário", mundo.display());
    let mut ligados = Vec::new();
    for origem in args.map(PathBuf::from) {
        let manifesto =
            seele_proto::mods::read_manifest(&std::fs::read_to_string(origem.join("mod.json"))?)?;
        let mut arquivos = Vec::new();
        pacote(&origem, &origem, &mut arquivos)?;
        let hash = seele_proto::mods::hex(&seele_proto::mods::content_hash(&mut arquivos));
        // Por hash, onde `RaizesDosMods::pacote_de` procura — e não por
        // identificador, como `servidor-com-mod.rs` ainda faz (na v0.15.1 a
        // metade de servidor daquele exemplo recusa todo pedido com
        // «No such file or directory»).
        let instalado = mundo.join("mods").join("mod-packages").join(&hash);
        for (nome, bytes) in &arquivos {
            let destino = instalado.join(nome);
            std::fs::create_dir_all(destino.parent().context("sem pasta")?)?;
            std::fs::write(destino, bytes)?;
        }
        ligados.push((manifesto, hash));
    }
    std::fs::create_dir_all(&mundo)?;
    let daemon = Daemon::bind(ServerConfig {
        name: format!("M3 · {} MOD(s)", ligados.len()),
        listen: SocketAddr::from(([127, 0, 0, 1], porta)),
        database: Location::File(mundo.join("seele.db")),
        mods_dir: Some(seele_server::RaizesDosMods {
            pacotes: mundo.join("mods"),
            dados: mundo.join("mod-data"),
        }),
        ..ServerConfig::default()
    })
    .await?;
    {
        let db = daemon.server().persistence.lock().await;
        for (manifesto, hash) in &ligados {
            enable(
                &db,
                &EnabledMod {
                    id: manifesto.id.clone(),
                    version: manifesto.version.clone(),
                    hash: hash.clone(),
                    repo: manifesto.repo.clone(),
                    reach: manifesto.reach.clone(),
                    server_half: manifesto.server.is_some(),
                },
            )?;
        }
    }
    let lista: Vec<String> = ligados
        .iter()
        .map(|(m, h)| format!("{}@{}#{}", m.id, m.version, h))
        .collect();
    println!(
        "M3 endereco={} fp={} mods={}",
        daemon.local_addr()?,
        daemon.fingerprint(),
        lista.join(",")
    );
    daemon.run().await
}
