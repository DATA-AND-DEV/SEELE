//! `seeled` — the SEELE daemon.
//!
//! `specs/04-servidor-seele.md`: one instance is a **Dogma Central**. This binary
//! is a thin wrapper; everything it does lives in the library beside it, so the
//! integration tests can drive a server in process (`specs/10-convencoes.md`).

use std::net::SocketAddr;

use anyhow::{Context, Result};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "seele_server=info,seeled=info".into()),
        )
        .init();

    let listen: SocketAddr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| format!("0.0.0.0:{}", seele_proto::transport::DEFAULT_PORT))
        .parse()
        .context("could not parse the listen address")?;

    let dogma = seele_server::DogmaConfig {
        name: "Terceira Tóquio".into(),
        listen,
        ..seele_server::DogmaConfig::default()
    };

    let server = seele_server::Server::bind(dogma).await?;
    let bound = server.local_addr()?;
    println!("seeled listening on {bound}");

    // What to type on the other machine. A server that only reports
    // `0.0.0.0:8383` has told the operator nothing they can use, and the first
    // thing anybody does with a self-hosted voice server is try it from a
    // second computer.
    if bound.ip().is_unspecified() {
        if let Some(lan) = lan_address() {
            println!();
            println!("na outra máquina:");
            println!("  plug --server {lan}:{}", bound.port());
        }
    }
    println!("certificate fingerprint: {}", server.fingerprint());
    println!();
    println!("TOFU (ADR 0003): a client pins this on first contact and refuses");
    println!("to connect silently if it ever changes. Read it out over another");
    println!("channel if somebody asks whether a change was real.");

    server.run().await
}

/// This machine's address on the network it would reach the world through.
///
/// No dependency and no interface enumeration: connecting a UDP socket picks a
/// route and binds a local address without sending a single packet, which is
/// exactly the question being asked — "which of my addresses would somebody
/// else see". The target is TEST-NET-3 (`203.0.113.0/24`, RFC 5737), reserved
/// for documentation, so nothing is implied about reaching a real host.
fn lan_address() -> Option<std::net::IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("203.0.113.1:80").ok()?;
    let local = socket.local_addr().ok()?.ip();
    (!local.is_loopback() && !local.is_unspecified()).then_some(local)
}
