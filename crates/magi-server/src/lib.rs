//! `magid` — the MAGI daemon. One instance is a **Dogma Central**.
//!
//! `specs/04-servidor-magi.md` names three subsystems, and they are real module
//! boundaries rather than decoration:
//!
//! | Subsystem | Responsibility | Module |
//! |---|---|---|
//! | **MELCHIOR** | Identity, authentication, sessions, roles | [`session`] |
//! | **BALTHASAR** | Media routing, forwarding, bandwidth | [`cage`] |
//! | **CASPER** | Persistent state, history, migrations | M3 |
//!
//! M2 builds MELCHIOR's handshake and BALTHASAR's forwarding, with one fixed
//! Cage and no persistence.
//!
//! # Why this is a library as well as a binary
//!
//! `specs/10-convencoes.md` puts protocol coverage under "integration tests with
//! an in-process server". The acceptance criterion for M2 — three clients in a
//! Cage — therefore runs in CI, on a machine with no sound card and no second
//! host.

#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)
)]

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use magi_proto::ids::CageId;

pub mod cage;
pub mod frame;
pub mod session;
pub mod tls;

/// Length of an Ed25519 public key, in bytes.
pub const PUBLIC_KEY_LEN: usize = magi_proto::control::PUBLIC_KEY_LEN;

/// How a Dogma is configured.
///
/// `specs/04-servidor-magi.md` describes a TOML file; M2 takes the same fields
/// as a struct and leaves parsing for M3, when there is persistent state worth
/// configuring.
#[derive(Debug, Clone)]
pub struct DogmaConfig {
    /// What this Dogma is called.
    pub name: String,
    /// Where to listen. UDP; QUIC needs no second port.
    pub listen: SocketAddr,
    /// The one Cage M2 offers.
    pub cage: CageId,
    /// Its display name.
    pub cage_name: String,
    /// How many pilots fit in it.
    pub cage_limit: u16,
    /// Nicknames that may enter but not transmit.
    ///
    /// A stand-in for the Observador role of `specs/04-servidor-magi.md` until
    /// M3 brings accounts. It exists so the M2 acceptance criterion — "a client
    /// without permission is refused" — can be exercised for real rather than
    /// asserted.
    pub observers: Vec<String>,
}

impl Default for DogmaConfig {
    fn default() -> Self {
        Self {
            name: "Dogma".into(),
            listen: SocketAddr::from(([0, 0, 0, 0], magi_proto::transport::DEFAULT_PORT)),
            cage: CageId(1),
            cage_name: "CAGE-01 CENTRAL".into(),
            cage_limit: 15,
            observers: Vec::new(),
        }
    }
}

/// A running Dogma.
pub struct Server {
    endpoint: quinn::Endpoint,
    config: Arc<DogmaConfig>,
    fingerprint: String,
    registry: Arc<session::Registry>,
    cage: tokio::sync::mpsc::Sender<cage::CageCommand>,
}

impl Server {
    /// Binds the endpoint and starts the Cage task.
    ///
    /// # Errors
    ///
    /// Fails if the certificate cannot be generated or the socket cannot bind.
    pub async fn bind(config: DogmaConfig) -> Result<Self> {
        // rustls 0.23 requires a crypto provider to be chosen explicitly. Doing
        // it here rather than in `main` means the integration tests get it too.
        let _ = rustls::crypto::ring::default_provider().install_default();

        let identity =
            tls::Identity::self_signed(vec!["localhost".into(), config.listen.ip().to_string()])?;
        let fingerprint = identity.fingerprint();
        let server_config = tls::server_config(identity)?;

        let endpoint = quinn::Endpoint::server(server_config, config.listen)
            .with_context(|| format!("could not bind {}", config.listen))?;

        let cage = cage::spawn(config.cage);

        Ok(Self {
            endpoint,
            config: Arc::new(config),
            fingerprint,
            registry: Arc::new(session::Registry::new()),
            cage,
        })
    }

    /// The address actually bound. Useful when the port was left as zero.
    ///
    /// # Errors
    ///
    /// Fails if the socket cannot report its address.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.endpoint.local_addr()?)
    }

    /// The certificate fingerprint a client pins. ADR 0003.
    #[must_use]
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Accepts connections until the endpoint closes.
    ///
    /// One task per connection, as `specs/04-servidor-magi.md` requires.
    ///
    /// # Errors
    ///
    /// Returns when the endpoint is closed.
    pub async fn run(self) -> Result<()> {
        while let Some(incoming) = self.endpoint.accept().await {
            let config = Arc::clone(&self.config);
            let registry = Arc::clone(&self.registry);
            let cage = self.cage.clone();

            tokio::spawn(async move {
                let connection = match incoming.await {
                    Ok(connection) => connection,
                    Err(error) => {
                        tracing::warn!(%error, "connection failed before the handshake");
                        return;
                    }
                };
                let peer = connection.remote_address();
                tracing::info!(%peer, "pattern orange");

                if let Err(error) = session::serve(connection, config, registry, cage).await {
                    tracing::info!(%peer, %error, "session closed");
                }
            });
        }
        Ok(())
    }

    /// Stops accepting and closes the endpoint.
    pub fn shutdown(&self) {
        // specs/04-servidor-magi.md wants clients told `ManutencaoProgramada`
        // and given 3 s. That needs the per-connection registry M3 brings; for
        // now the close is abrupt and honest about it.
        self.endpoint.close(0_u32.into(), b"shutting down");
    }
}
