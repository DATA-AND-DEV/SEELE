//! `magid` — the MAGI daemon.
//!
//! `specs/04-servidor-magi.md`: one instance is a **Dogma Central**. This binary
//! is a thin wrapper; everything it does lives in the library beside it, so the
//! integration tests can drive a server in process (`specs/10-convencoes.md`).

use std::net::SocketAddr;

use anyhow::{Context, Result};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "magi_server=info,magid=info".into()),
        )
        .init();

    let listen: SocketAddr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| format!("0.0.0.0:{}", magi_proto::transport::DEFAULT_PORT))
        .parse()
        .context("could not parse the listen address")?;

    let dogma = magi_server::DogmaConfig {
        name: "Terceira Tóquio".into(),
        listen,
        ..magi_server::DogmaConfig::default()
    };

    let server = magi_server::Server::bind(dogma).await?;
    println!("magid listening on {}", server.local_addr()?);
    println!("certificate fingerprint: {}", server.fingerprint());
    println!();
    println!("TOFU (ADR 0003): a client pins this on first contact and refuses");
    println!("to connect silently if it ever changes. Read it out over another");
    println!("channel if somebody asks whether a change was real.");

    server.run().await
}
