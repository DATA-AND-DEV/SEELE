//! Um servidor local para desenvolver **um MOD qualquer**, sem publicar nada.
//!
//! # Por que ele não traz nenhum MOD dentro
//!
//! A primeira versão deste exemplo embutia a Mesa por `include_bytes!`, e com
//! isso o repositório do produto carregava um MOD específico só para conseguir
//! levantar um servidor de teste. É o que o [ADR
//! 0045](../../../docs/adr/0045-mods-o-produto-base-tem-regras-e-um-mod-nao.md)
//! nega — «o produto base tem regras, e um MOD não» —, e na prática impedia a
//! Mesa de mudar de casa sem quebrar a compilação do SEELE.
//!
//! Agora a pasta do MOD é argumento. O exemplo serve a Mesa, serve o MOD de
//! outra pessoa, e não conhece nenhum dos dois.
//!
//! # Uso
//!
//! ```text
//! cargo run -p seele-server --example servidor-com-mod -- \
//!     /caminho/absoluto/do/mod  /caminho/absoluto/do/mundo
//! ```
//!
//! A primeira pasta é o pacote — a que tem `mod.json` dentro. A segunda é o
//! mundo: banco, pacote instalado e `dados/`. Ela **não pode existir ainda**,
//! ou o comando recusa: reabrir um mundo com código diferente do que o gerou é
//! como se perde uma campanha, e essa é uma perda que não se desfaz.
use anyhow::{Context, Result};
use seele_server::{
    persistence::{
        mods::{enable, EnabledMod},
        Location,
    },
    Daemon, ServerConfig,
};
use std::{net::SocketAddr, path::Path, path::PathBuf};

/// Lê o pacote inteiro, menos `dados/`, na mesma ordem que o servidor usa para
/// conferir o hash. Fora do hash, `dados/` é do mundo e não do código.
fn pacote(raiz: &Path, pasta: &Path, arquivos: &mut Vec<(String, Vec<u8>)>) -> Result<()> {
    for entrada in std::fs::read_dir(pasta)? {
        let entrada = entrada?;
        if pasta == raiz && entrada.file_name() == "dados" {
            continue;
        }
        anyhow::ensure!(
            !entrada.file_type()?.is_symlink(),
            "atalho dentro do pacote: {}",
            entrada.path().display()
        );
        let caminho = entrada.path();
        if caminho.is_dir() {
            pacote(raiz, &caminho, arquivos)?;
        } else {
            arquivos.push((
                caminho
                    .strip_prefix(raiz)?
                    .to_string_lossy()
                    .replace('\\', "/"),
                std::fs::read(&caminho)?,
            ));
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let uso = "uso: cargo run -p seele-server --example servidor-com-mod -- \
               /caminho/do/mod /caminho/do/mundo";
    let origem = PathBuf::from(std::env::args().nth(1).context(uso)?);
    let mundo = PathBuf::from(std::env::args().nth(2).context(uso)?);
    anyhow::ensure!(
        origem.is_absolute() && mundo.is_absolute(),
        "use caminhos absolutos"
    );
    anyhow::ensure!(
        origem.join("mod.json").is_file(),
        "não achei mod.json em {} — aponte para a pasta do pacote",
        origem.display()
    );
    anyhow::ensure!(
        !mundo.exists(),
        "{} já existe. Escolha uma pasta nova: reabrir um mundo com código \
         diferente do que o gerou perde a campanha que está lá dentro.",
        mundo.display()
    );

    let manifesto =
        seele_proto::mods::read_manifest(&std::fs::read_to_string(origem.join("mod.json"))?)?;
    let instalado = mundo.join("mods").join(&manifesto.id);
    let mut arquivos = Vec::new();
    pacote(&origem, &origem, &mut arquivos)?;
    for (nome, bytes) in &arquivos {
        let destino = instalado.join(nome);
        std::fs::create_dir_all(
            destino
                .parent()
                .context("caminho sem pasta dentro do pacote")?,
        )?;
        std::fs::write(destino, bytes)?;
    }
    let hash = seele_proto::mods::hex(&seele_proto::mods::content_hash(&mut arquivos));

    let daemon = Daemon::bind(ServerConfig {
        name: format!("{} — desenvolvimento", manifesto.id),
        listen: SocketAddr::from(([127, 0, 0, 1], 8384)),
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
        enable(
            &db,
            &EnabledMod {
                id: manifesto.id.clone(),
                version: manifesto.version.clone(),
                hash,
                repo: manifesto.repo.clone(),
                reach: manifesto.reach.clone(),
                server_half: manifesto.server.is_some(),
            },
        )?;
    }
    println!(
        "MOD: {} {}\nServidor: {}\nImpressão digital: {}\nPacote instalado: {}",
        manifesto.id,
        manifesto.version,
        daemon.local_addr()?,
        daemon.fingerprint(),
        instalado.display()
    );
    daemon.run().await
}
