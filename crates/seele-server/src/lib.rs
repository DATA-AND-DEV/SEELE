//! `seeled` — the SEELE daemon. One instance is a **Dogma Central**.
//!
//! `specs/04-servidor-seele.md` names three subsystems, and they are real module
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
use seele_proto::ids::CageId;

pub mod admissao;
pub mod cage;
pub mod casper;
pub mod dogma;
pub mod frame;
pub mod hospedagem;
pub mod melchior;
pub mod session;
pub mod taxa;
pub mod tls;

/// Length of an Ed25519 public key, in bytes.
pub const PUBLIC_KEY_LEN: usize = seele_proto::control::PUBLIC_KEY_LEN;

/// How a Dogma is configured.
///
/// `specs/04-servidor-seele.md` describes a TOML file; M2 takes the same fields
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
    /// Nicknames that arrive as Observador rather than Piloto.
    ///
    /// M3 brought real accounts, so this is now only a bootstrap convenience:
    /// somebody has to be able to configure the first roles before there is an
    /// operator to do it. Authorisation itself is MELCHIOR's, always.
    pub observers: Vec<String>,
    /// Where CASPER keeps the database.
    pub database: crate::casper::Location,
}

impl Default for DogmaConfig {
    fn default() -> Self {
        Self {
            name: "Dogma".into(),
            listen: SocketAddr::from(([0, 0, 0, 0], seele_proto::transport::DEFAULT_PORT)),
            cage: CageId(1),
            cage_name: "CAGE-01 CENTRAL".into(),
            cage_limit: 15,
            observers: Vec::new(),
            database: crate::casper::Location::Memory,
        }
    }
}

/// Creates the configured Cage and its Line if they are not there yet.
///
/// M2 kept these in the config struct; M3 keeps them in CASPER so a restart
/// finds the same room rather than rebuilding it. Idempotent, because it runs
/// on every boot.
///
/// # A Dogma opens with somewhere to go
///
/// This is what stops a fresh Dogma being a screen that does not explain
/// itself. A room with no Cage offers nowhere to speak and no Line to write in;
/// the person who pressed **HOSPEDAR AQUI** looks at an empty list and has no
/// way to tell a working Dogma from a broken one. So the first boot writes a
/// Line called `geral` and one Cage bound to it, and there is somewhere to
/// stand from the first second.
///
/// # Why here, and not in the migration
///
/// A migration would look cheaper — one more `INSERT` in migration 1 — and it
/// is the wrong place, for two reasons that only show up later.
///
/// **A migration is irreversible and append-only** ([`casper::schema`]). The
/// name of a Cage is not: `ClientMessage::RenameCage` exists, and the whole
/// point of hosting your own Dogma is that the rooms are yours. Seeded content
/// baked into a schema version would be a name the operator can change and the
/// history of the schema still claims.
///
/// **The Cage's name and limit come from [`DogmaConfig`]**, which a migration
/// cannot see. Migrations run inside CASPER, before there is a config to
/// consult, and passing one in would make "which SQL is this database at" depend
/// on a runtime value.
///
/// `INSERT OR IGNORE` is what keeps it honest on the second boot: a Cage that
/// has since been renamed keeps its new name, because the conflicting insert is
/// skipped rather than applied.
fn seed(casper: &mut casper::Casper, config: &DogmaConfig) -> Result<()> {
    let connection = casper.connection();
    connection.execute(
        "INSERT OR IGNORE INTO lines (id, name) VALUES (1, 'geral')",
        [],
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO cages (id, name, member_limit, line_id)
         VALUES (?1, ?2, ?3, 1)",
        rusqlite::params![
            i64::from(config.cage.get()),
            config.cage_name,
            i64::from(config.cage_limit)
        ],
    )?;
    Ok(())
}

/// A running Dogma.
pub struct Server {
    endpoint: quinn::Endpoint,
    config: Arc<DogmaConfig>,
    fingerprint: String,
    registry: Arc<session::Registry>,
    dogma: Arc<dogma::Dogma>,
    cages: Arc<cage::Cages>,
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

        // CASPER first: the handshake needs accounts before it can answer
        // anybody, and migrations run at boot (specs/04-servidor-seele.md).
        // A identidade TLS também mora nele, então ele vem antes dela.
        let mut casper = casper::Casper::open(&config.database)?;
        seed(&mut casper, &config)?;

        // Lida do banco, não gerada a cada vez. Sem isto, reiniciar o Dogma
        // trocava a chave e todo cliente já conectado via `A CHAVE DO SERVIDOR
        // MUDOU` — o alerta bloqueante do ADR 0003 — e era recusado. O aviso
        // reservado para ataque disparando num reinício de rotina é como se
        // ensina a ignorá-lo.
        let identity = tls::Identity::load_or_create(
            &casper,
            vec!["localhost".into(), config.listen.ip().to_string()],
        )?;
        let fingerprint = identity.fingerprint();
        let server_config = tls::server_config(identity)?;

        let endpoint = quinn::Endpoint::server(server_config, config.listen)
            .with_context(|| format!("could not bind {}", config.listen))?;

        let casper = Arc::new(tokio::sync::Mutex::new(casper));

        let (events, _) = tokio::sync::broadcast::channel(1024);
        let writes = dogma::spawn_writer(Arc::clone(&casper), events.clone());
        let dogma = Arc::new(dogma::Dogma {
            casper,
            events,
            writes,
            slots: Arc::new(tokio::sync::Mutex::new(dogma::Slots::default())),
            occupancy: Arc::new(tokio::sync::Mutex::new(dogma::Occupancy::default())),
            portaria: Arc::new(tokio::sync::Mutex::new(taxa::Portaria::nova())),
        });

        // Held seats have to be released even if nobody reconnects, or a Dogma
        // slowly fills with places kept for people who left for good.
        let sweeper = Arc::clone(&dogma.slots);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                ticker.tick().await;
                let freed = sweeper.lock().await.sweep(std::time::Instant::now());
                if freed > 0 {
                    tracing::info!(freed, "expired seats released");
                }
            }
        });

        // Uma tarefa por Cage, criada quando alguém entra. Antes havia
        // exatamente uma, a do Cage do `DogmaConfig`, e toda sessão segurava
        // esse único remetente: correto enquanto um Dogma tinha uma sala,
        // silenciosamente errado no instante em que passou a poder ter duas.
        let cages = Arc::new(cage::Cages::new());

        Ok(Self {
            endpoint,
            config: Arc::new(config),
            fingerprint,
            registry: Arc::new(session::Registry::new()),
            dogma,
            cages,
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

    /// A impressão digital lida direto do banco, sem subir servidor.
    ///
    /// É o que `seeled convite` precisa: o operador está com o Dogma parado
    /// quando gera um convite, e o link tem de carregar a mesma impressão que
    /// o servidor vai apresentar depois.
    ///
    /// # Errors
    ///
    /// Falha se o banco não responder ou não tiver identidade ainda.
    pub fn fingerprint_do_banco(casper: &casper::Casper) -> Result<String> {
        let cert: Vec<u8> = casper.connection().query_row(
            "SELECT valor FROM configuracao WHERE chave = 'tls_cert'",
            [],
            |linha| linha.get(0),
        )?;
        Ok(seele_proto::transport::certificate_fingerprint(&cert))
    }

    /// A política de admissão deste Dogma.
    ///
    /// # Errors
    ///
    /// Falha se o banco não responder.
    pub fn politica_de_admissao(&self) -> Result<admissao::Politica> {
        let guard = self
            .dogma
            .casper
            .try_lock()
            .map_err(|_| anyhow::anyhow!("o banco está ocupado"))?;
        admissao::Politica::carregar(&guard)
    }

    /// Accepts connections until the endpoint closes.
    ///
    /// One task per connection, as `specs/04-servidor-seele.md` requires.
    ///
    /// # Errors
    ///
    /// Returns when the endpoint is closed.
    pub async fn run(&self) -> Result<()> {
        while let Some(incoming) = self.endpoint.accept().await {
            let config = Arc::clone(&self.config);
            let registry = Arc::clone(&self.registry);
            let dogma = Arc::clone(&self.dogma);
            let cages = Arc::clone(&self.cages);

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

                if let Err(error) = session::serve(connection, config, registry, dogma, cages).await
                {
                    tracing::info!(%peer, %error, "session closed");
                }
            });
        }
        Ok(())
    }

    /// Borrows the shared state, for tests and for tooling.
    #[must_use]
    pub fn dogma(&self) -> &Arc<dogma::Dogma> {
        &self.dogma
    }

    /// Stops accepting and closes the endpoint.
    pub fn shutdown(&self) {
        // specs/04-servidor-seele.md wants clients told `ManutencaoProgramada`
        // and given 3 s. That needs the per-connection registry M3 brings; for
        // now the close is abrupt and honest about it.
        self.endpoint.close(0_u32.into(), b"shutting down");
    }

    /// Espera o endpoint terminar de fechar.
    ///
    /// `shutdown` sinaliza e volta na hora; o socket UDP só é liberado quando o
    /// endpoint termina. Sem esperar, parar de hospedar e recomeçar em seguida
    /// falha com "endereço já em uso" — que é exatamente o que alguém faz ao
    /// fechar uma conversa e abrir outra.
    pub async fn wait_idle(&self) {
        self.endpoint.wait_idle().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::casper::channels::Channels;
    use crate::casper::{Casper, Location};

    fn born(config: &DogmaConfig) -> Casper {
        let mut casper = Casper::open(&config.database).expect("open");
        seed(&mut casper, config).expect("seed");
        casper
    }

    #[test]
    fn a_new_dogma_opens_with_somewhere_to_go() {
        // A Dogma with no Cage offers nowhere to speak and no Line to write in.
        // The person who pressed HOSPEDAR AQUI would look at an empty list with
        // no way to tell a working Dogma from a broken one — and "hospede você
        // mesmo" would be a claim rather than a thing that happens.
        let config = DogmaConfig::default();
        let casper = born(&config);
        let channels = Channels::new(&casper);

        let lines = channels.lines().expect("lines");
        assert_eq!(lines.len(), 1, "a Dogma opened with no Line: {lines:?}");
        assert_eq!(lines[0].name, "geral");

        let cages = channels.cages().expect("cages");
        assert_eq!(cages.len(), 1, "a Dogma opened with no Cage: {cages:?}");
        assert_eq!(cages[0].name, config.cage_name);
        assert_eq!(cages[0].limit, config.cage_limit);
        assert_eq!(
            cages[0].line,
            Some(lines[0].id),
            "the opening Cage has no Line attached to it"
        );
        assert!(
            !cages[0].password_required,
            "the opening Cage is locked, and nobody has the key"
        );
    }

    #[test]
    fn a_second_boot_does_not_add_a_second_set_of_rooms() {
        // `seed` runs on every boot. Not being idempotent would mean a Dogma
        // that has been restarted twenty times shows twenty `geral`s.
        let directory = tempfile::tempdir().expect("tempdir");
        let config = DogmaConfig {
            database: Location::File(directory.path().join("dogma.db")),
            ..DogmaConfig::default()
        };

        drop(born(&config));
        let casper = born(&config);

        let channels = Channels::new(&casper);
        assert_eq!(channels.lines().expect("lines").len(), 1);
        assert_eq!(channels.cages().expect("cages").len(), 1);
    }

    #[test]
    fn a_renamed_opening_cage_keeps_its_new_name_across_a_restart() {
        // The seed and `RenameCage` write the same row, and a seed that
        // overwrote would undo the rename on the next restart — silently, and
        // only for the room the Dogma came with, which is the hardest kind of
        // bug to believe when somebody reports it.
        let directory = tempfile::tempdir().expect("tempdir");
        let config = DogmaConfig {
            database: Location::File(directory.path().join("dogma.db")),
            ..DogmaConfig::default()
        };

        let casper = born(&config);
        let cage = Channels::new(&casper).cages().expect("cages")[0].id;
        Channels::new(&casper)
            .rename_cage(cage, "CAGE-01 PONTE")
            .expect("rename");
        drop(casper);

        let casper = born(&config);
        let cages = Channels::new(&casper).cages().expect("cages");
        assert_eq!(cages.len(), 1);
        assert_eq!(cages[0].name, "CAGE-01 PONTE");
    }
}
