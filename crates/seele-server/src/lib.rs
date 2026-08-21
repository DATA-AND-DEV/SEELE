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
pub mod alcance;
pub mod cage;
pub mod casper;
pub mod dogma;
pub mod frame;
pub mod hospedagem;
pub mod melchior;
pub mod portaria;
pub mod session;
pub mod taxa;
pub mod tls;
pub mod transfer;

/// O vocabulário de fio de um ponto de encontro, e **só** ele.
///
/// Existe porque quem diagnostica a rede (`plug --rede`, em `seele-tui`) precisa
/// escrever e ler estes datagramas, e a regra de dependência do ADR 0002 não
/// deixa uma casca ver `seele-proto`.
///
/// # A lista é explícita, e o que ficou de fora é o ponto
///
/// `responder`, `responder_em`, `Vizinhanca` e `alcancavel` **não** saem daqui.
/// Eles são a política antiamplificação do ADR 0022 — até onde um `LEVE` pode
/// apontar, que portas são recusadas, o que faz o ponto de encontro calar — e
/// essa decisão é de quem opera um ponto de encontro, não de uma casca. Um
/// `pub use seele_proto::encontro;` inteiro os levava junto, e com eles a casca
/// passava a poder montar um refletor e escolher a política dele.
///
/// O que sai é o formato dos 96 bytes, que não tem decisão dentro.
///
/// # `check-deps` é cego para esta classe
///
/// `cargo xtask check-deps` lê o grafo do Cargo, e uma reexportação não é uma
/// aresta do Cargo. Alargar esta lista **não** faz o `check-deps` reprovar, e
/// quem contar com ele para pegar isto não vai pegar. A guarda é esta lista e a
/// revisão dela.
///
/// Note a diferença para [`alcance::encontro`], que é o **degrau 4** do lado do
/// anfitrião: aquele decide, este só escreve e lê datagramas.
pub mod encontro {
    /// O que a produção do diagnóstico usa: os construtores, o leitor da
    /// resposta, o tamanho fixo e a marca.
    pub use seele_proto::encontro::{furo, ler_aqui, leve, onde, Marca, TAMANHO};

    /// O que só o ponto de encontro de mentira do `mod testes` usa, para poder
    /// fazer o papel de um.
    ///
    /// `analisar` lê um pedido e **não julga o destino** — a frase é da
    /// documentação dele, e é por isso que ele pode sair e `responder_em` não:
    /// o julgamento é a política, e um duplo que herdasse a política do serviço
    /// público deixaria de ser um duplo, com a `Vizinhanca` escolhida por quem
    /// escreveu o teste. `aqui` monta a resposta, campo a campo, como o serviço
    /// a monta.
    pub use seele_proto::encontro::{analisar, aqui, Pedido};
}

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
    ///
    /// O padrão é `[::]`, e a diferença importa: um socket IPv6 de pilha dupla
    /// atende as duas famílias, e `0.0.0.0` atende só IPv4. Era `0.0.0.0` até o
    /// degrau 2 do ADR 0022 — e por isso um Dogma não atendia em IPv6 nem
    /// quando as duas pontas tinham. Ver [`alcance::abrir_escuta`].
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
    /// Where the attachment blobs live. ADR 0027.
    ///
    /// `None` derives it: `anexos/` beside the database file. Beside it, and
    /// **not inside it** — a blob in SQLite frees pages without shrinking the
    /// file, so the worst case on disk would become the historical high-water
    /// mark rather than the ceiling, which is the one property this whole
    /// decision was taken for.
    ///
    /// Nothing to derive from means no attachments: a Dogma in memory has no
    /// directory to own, and the honest answer to a transfer is
    /// `AttachmentRefusal::Unavailable` rather than a folder appearing in
    /// whatever directory the process happened to start in. Tests that want
    /// attachments say where.
    pub anexos: Option<std::path::PathBuf>,
}

impl Default for DogmaConfig {
    fn default() -> Self {
        Self {
            name: "Dogma".into(),
            listen: SocketAddr::from((
                std::net::Ipv6Addr::UNSPECIFIED,
                seele_proto::transport::DEFAULT_PORT,
            )),
            cage: CageId(1),
            cage_name: "CAGE-01 CENTRAL".into(),
            cage_limit: 15,
            observers: Vec::new(),
            database: crate::casper::Location::Memory,
            anexos: None,
        }
    }
}

/// Where the blobs go, given where the database is.
///
/// `anexos/` beside `seele.db`. A database with no file — the in-memory one
/// tests use — has nowhere to put them, and says so instead of guessing.
#[must_use]
pub fn diretorio_de_anexos(config: &DogmaConfig) -> Option<std::path::PathBuf> {
    if let Some(escolhido) = &config.anexos {
        return Some(escolhido.clone());
    }
    match &config.database {
        casper::Location::File(path) => Some(
            path.parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .join("anexos"),
        ),
        casper::Location::Memory => None,
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
///
/// # Why a flag, and not only `INSERT OR IGNORE`
///
/// Because the identifiers are written out by hand, `INSERT OR IGNORE` means
/// "unless row 1 is taken" — and once a Line can be **destroyed**, row 1 stops
/// being taken. A Dogma whose operator destroyed `geral` on purpose, after a
/// confirmation promising that nothing brings it back, would find it again at
/// the next restart: same name, same identifier, empty. That is the product
/// contradicting its own confirmation with a `systemctl restart`, and it is not
/// a case `INSERT OR IGNORE` can tell apart from a first boot.
///
/// So the question this asks changed from "is row 1 free" to "has this database
/// ever been seeded", which is the question it always meant. The flag survives
/// deletion because nothing deletes it.
///
/// A database seeded by an older build has no flag and still has its rooms:
/// the first boot on this code writes the flag and the two inserts do nothing,
/// which is what `INSERT OR IGNORE` was already for.
const SEMEADO: &str = "semeado";

fn seed(casper: &mut casper::Casper, config: &DogmaConfig) -> Result<()> {
    let connection = casper.connection();
    let ja: i64 = connection.query_row(
        "SELECT COUNT(*) FROM configuracao WHERE chave = ?1",
        [SEMEADO],
        |linha| linha.get(0),
    )?;
    if ja > 0 {
        return Ok(());
    }

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
    connection.execute(
        "INSERT INTO configuracao (chave, valor) VALUES (?1, '1')",
        [SEMEADO],
    )?;
    Ok(())
}

/// A running Dogma.
pub struct Server {
    endpoint: quinn::Endpoint,
    config: Arc<DogmaConfig>,
    pilha: alcance::Pilha,
    /// O mesmo socket do endpoint, para escrever. Ver [`Server::espelho`].
    espelho: Option<Arc<std::net::UdpSocket>>,
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

        // Não é `Endpoint::server`: aquele faz um `UdpSocket::bind` cru e herda
        // o padrão do sistema para `IPV6_V6ONLY`, que difere entre Linux, macOS
        // e Windows. A própria documentação dele avisa. Ver
        // [`alcance::abrir_escuta`], onde a opção é escrita e conferida.
        let (socket, pilha) = alcance::abrir_escuta(config.listen)?;
        // Um segundo descritor para o **mesmo** socket, antes de o quinn tomar
        // conta dele. É o que torna o degrau 4 do ADR 0022 possível: furar um
        // NAT exige mandar pacotes da porta em que o QUIC atende, e o quinn não
        // empresta o socket dele. Só para **escrever** — ler daqui roubaria
        // pacotes do QUIC, e é por isso que o degrau 4 abre uma escuta própria
        // para as respostas. Ver `alcance::encontro`.
        //
        // `try_clone` num socket UDP duplica o descritor, não o socket: as duas
        // pontas apontam para a mesma fila e para o mesmo mapeamento de NAT, que
        // é exatamente a propriedade de que o furo precisa.
        let espelho = match socket.try_clone() {
            Ok(copia) => Some(Arc::new(copia)),
            Err(erro) => {
                tracing::info!(%erro, "sem espelho do socket: o degrau 4 do ADR 0022 fica de fora");
                None
            }
        };
        let runtime = quinn::default_runtime()
            .ok_or_else(|| anyhow::anyhow!("não há runtime assíncrono para o QUIC"))?;
        let endpoint = quinn::Endpoint::new(
            quinn::EndpointConfig::default(),
            Some(server_config),
            socket,
            runtime,
        )
        .with_context(|| format!("could not bind {}", config.listen))?;
        tracing::info!(?pilha, escuta = %endpoint.local_addr()?, "o Dogma está atendendo");

        // Before the mutex, because opening it sweeps the directory against the
        // table and both are still ours alone at this point. ADR 0027 wants the
        // ceiling standing before the first connection, not after.
        let anexos = match diretorio_de_anexos(&config) {
            Some(root) => Some(Arc::new(transfer::Vault::open(root, &casper)?)),
            None => {
                tracing::info!("anexos: sem diretório, este Dogma não guarda arquivo");
                None
            }
        };

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
            atrasos: Arc::new(dogma::Atrasos::default()),
            anexos,
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
            pilha,
            espelho,
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

    /// O socket em que o Dogma atende, para **escrever**. Degrau 4 do ADR 0022.
    ///
    /// Furar um NAT exige mandar pacotes da porta em que o QUIC atende: o
    /// roteador abre caminho por porta interna, e um pacote saído de outro
    /// socket abriria caminho para o socket errado. Ver `alcance::encontro`.
    ///
    /// Só para escrever. Ler daqui roubaria pacotes do quinn, que é quem lê.
    #[must_use]
    pub fn espelho(&self) -> Option<Arc<std::net::UdpSocket>> {
        self.espelho.clone()
    }

    /// Que famílias de endereço esta escuta alcança. Degrau 2 do ADR 0022.
    ///
    /// Quem hospeda precisa poder dizer isto a quem está na frente da tela: um
    /// Dogma que caiu para IPv4 é um Dogma que ninguém só-IPv6 alcança, e essa
    /// é a diferença entre "não conecta" e "não conecta **porque**".
    #[must_use]
    pub fn pilha(&self) -> alcance::Pilha {
        self.pilha
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

    /// How many messages CASPER holds on a Line.
    ///
    /// The counterpart of [`Self::mensagens_da_linha`], and the one that answers
    /// "did anything get lost": a page is capped, a count is not. See
    /// [`casper::messages::Messages::count`].
    ///
    /// # Errors
    ///
    /// Fails if the database is busy or does not answer.
    pub async fn quantas_mensagens(&self, line: seele_proto::ids::LineId) -> Result<u64> {
        let mut guard = self.dogma.casper.lock().await;
        casper::messages::Messages::new(&mut guard).count(line)
    }

    /// What CASPER actually recorded on a Line, newest first.
    ///
    /// One page, capped at [`casper::messages::MAX_PAGE`] like every other
    /// reader of history — [`Self::quantas_mensagens`] is what counts.
    ///
    /// # Why this exists, and why it is not `Casper::connection()`
    ///
    /// `docs/pendencias.md` #1 is a delivery bug, and a delivery bug has two
    /// legs: what the Dogma **took in** and what it **handed out**. Measuring
    /// only the second leg cannot tell a message that was never stored from one
    /// that was stored and never sent, and those two have nothing in common but
    /// the symptom. So a test needs to see the first leg.
    ///
    /// The pendência recorded the obstacle as `Casper::connection()` being
    /// `pub(crate)`. Making it public would have worked and would have been the
    /// wrong trade: it hands out a `rusqlite::Connection`, and from that moment
    /// the schema is the contract. A test that writes its own `SELECT` goes on
    /// passing after a migration renames a column and starts asserting about a
    /// table nobody meant it to know.
    ///
    /// This asks the question instead of the storage: *what is on this Line?*
    /// It goes through [`casper::messages::Messages`] — the same reader the
    /// session uses for `FetchHistory` — so a test and a client see the same
    /// answer by construction, and the schema stays behind the crate wall.
    ///
    /// # Errors
    ///
    /// Fails if the database is busy or does not answer.
    pub async fn mensagens_da_linha(
        &self,
        line: seele_proto::ids::LineId,
        limite: u16,
    ) -> Result<Vec<casper::messages::StoredMessage>> {
        let mut guard = self.dogma.casper.lock().await;
        casper::messages::Messages::new(&mut guard).history(line, None, limite)
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

    #[test]
    fn a_destroyed_opening_line_does_not_come_back_at_the_next_boot() {
        // The confirmation in front of `apagar_linha` promises that no screen
        // of this product brings the Line back. A `systemctl restart` is not a
        // screen, and it was bringing this one back: the seed writes `lines`
        // row 1 by hand, `INSERT OR IGNORE` means "unless row 1 is taken", and
        // destroying the Line is precisely what makes row 1 free again. Same
        // name, same identifier, empty — the product contradicting its own
        // confirmation.
        let directory = tempfile::tempdir().expect("tempdir");
        let config = DogmaConfig {
            database: Location::File(directory.path().join("dogma.db")),
            ..DogmaConfig::default()
        };

        let casper = born(&config);
        let channels = Channels::new(&casper);
        let line = channels.lines().expect("lines")[0].id;
        // The Cage bound to it is unbound by the delete; nothing else here
        // depends on that, and the Cage is not what this test is about.
        channels.delete_line(line).expect("delete");
        drop(casper);

        let casper = born(&config);
        assert!(
            Channels::new(&casper).lines().expect("lines").is_empty(),
            "a Line destroyed on purpose was written back by the next boot"
        );
    }

    #[test]
    fn a_destroyed_opening_cage_does_not_come_back_at_the_next_boot() {
        // The same for the Cage, which the seed also writes by identifier. It
        // takes a second room to get here at all, because the last Cage is
        // refused — and that is the shape a real Dogma has when somebody
        // destroys the one it came with: they made theirs first.
        let directory = tempfile::tempdir().expect("tempdir");
        let config = DogmaConfig {
            database: Location::File(directory.path().join("dogma.db")),
            ..DogmaConfig::default()
        };

        let casper = born(&config);
        let channels = Channels::new(&casper);
        let inicial = channels.cages().expect("cages")[0].id;
        channels
            .create_cage("CAGE-02 PONTE", 8, None)
            .expect("cage");
        channels.delete_cage(inicial).expect("delete");
        drop(casper);

        let casper = born(&config);
        let cages = Channels::new(&casper).cages().expect("cages");
        assert_eq!(
            cages.len(),
            1,
            "the opening Cage was written back: {cages:?}"
        );
        assert_eq!(cages[0].name, "CAGE-02 PONTE");
    }

    #[test]
    fn a_dogma_seeded_by_an_older_build_is_not_seeded_twice() {
        // The upgrade path. A database from before the flag has its rooms and
        // no flag; the first boot on this code has to write the flag and leave
        // the rooms exactly as they are, which is what `INSERT OR IGNORE` was
        // already doing on its own.
        let directory = tempfile::tempdir().expect("tempdir");
        let config = DogmaConfig {
            database: Location::File(directory.path().join("dogma.db")),
            ..DogmaConfig::default()
        };

        let casper = born(&config);
        // Forget that this database was ever seeded, which is exactly the state
        // an older build leaves behind.
        casper
            .connection()
            .execute("DELETE FROM configuracao WHERE chave = ?1", [SEMEADO])
            .expect("forget");
        Channels::new(&casper)
            .rename_line(
                Channels::new(&casper).lines().expect("lines")[0].id,
                "avisos",
            )
            .expect("rename");
        drop(casper);

        let casper = born(&config);
        let lines = Channels::new(&casper).lines().expect("lines");
        assert_eq!(lines.len(), 1, "a second `geral` appeared: {lines:?}");
        assert_eq!(lines[0].name, "avisos");
    }
}
