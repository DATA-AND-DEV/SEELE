//! `seeled` — the SEELE daemon. One instance is a **server Central**.
//!
//! `specs/04-servidor-seele.md` names three subsystems, and they are real module
//! boundaries rather than decoration:
//!
//! | Subsystem | Responsibility | Module |
//! |---|---|---|
//! | **PERMISSIONS** | Identity, authentication, sessions, roles | [`session`] |
//! | **MEDIA** | Media routing, forwarding, bandwidth | [`voice_room`] |
//! | **PERSISTENCE** | Persistent state, history, migrations | M3 |
//!
//! M2 builds PERMISSIONS's handshake and MEDIA's forwarding, with one fixed
//! voice room and no persistence.
//!
//! # Why this is a library as well as a binary
//!
//! `specs/10-convencoes.md` puts protocol coverage under "integration tests with
//! an in-process server". The acceptance criterion for M2 — three clients in a
//! voice room — therefore runs in CI, on a machine with no sound card and no second
//! host.

#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)
)]

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use seele_proto::ids::VoiceRoomId;

pub mod admissao;
pub mod alcance;
pub mod autorizacao;
pub mod campainha;
pub mod frame;
pub mod hospedagem;
pub mod mods;
pub mod pares;
pub mod perda_de_subida;
pub mod permissions;
pub mod persistence;
pub mod portaria;
#[cfg(test)]
pub(crate) mod rastro_de_teste;
pub mod recepcao;
pub mod server;
pub mod session;
pub mod taxa;
pub mod tela;
pub mod tls;
pub mod transfer;
pub mod transporte;
pub mod voice_room;

/// O vocabulário de fio de um ponto de encontro, e **só** ele.
///
/// Existe porque quem diagnostica a rede precisa
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

/// Onde moram o código de um MOD e os arquivos que ele escreve.
///
/// **Duas raízes, e não uma** — plano de isolamento de 18/09, P0 «arquivos
/// mutáveis de servidores diferentes compartilham caminho».
///
/// Antes havia um caminho só, e o `dados/` saía de dentro dele:
/// `config/mods/<autor>/<nome>/dados`. Toda instância hospedada nesta máquina
/// recebia a mesma raiz, então o mesmo MOD em dois servidores lia e escrevia
/// **nos mesmos arquivos** — enquanto o `dados` chave-valor já estava
/// separado, corretamente, em bancos diferentes. Um avatar gravado num
/// servidor aparecia no outro.
///
/// A divisão segue o que cada metade é:
///
/// - **o pacote é imutável e conferido por hash.** Dois servidores que exigem
///   os mesmos bytes podem compartilhá-lo, e compartilhar é o certo: são os
///   mesmos bytes;
/// - **os arquivos são da instância.** Eles nascem de quem usa aquele
///   servidor, e não têm nada a ver com outro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaizesDosMods {
    /// Onde os pacotes verificados moram, por identificador.
    pub pacotes: std::path::PathBuf,
    /// Onde os arquivos mutáveis **desta instância** moram, por identificador.
    pub dados: std::path::PathBuf,
}

impl RaizesDosMods {
    /// A pasta do pacote com **estes bytes**.
    ///
    /// Por hash e não por identificador — plano de isolamento de 18/09, P1.
    /// Dois servidores desta máquina podem exigir conteúdos diferentes do
    /// mesmo MOD, e por identificador só um cabia: atualizar para um deixava o
    /// outro exigindo bytes que saíram do disco.
    #[must_use]
    // O nome literal, e não a constante do `seele-core`: este crate não o
    // tem na árvore, e puxá-lo para cá por uma palavra seria pagar uma
    // dependência inteira por ela. Quem guarda a constante é
    // `seele_core::mods::PACOTES`, e o guarda abaixo prende as duas juntas.
    pub fn pacote_de(&self, hash: &str) -> std::path::PathBuf {
        self.pacotes.join("mod-packages").join(hash)
    }

    /// A pasta mutável deste MOD, nesta instância.
    #[must_use]
    pub fn dados_de(&self, id: &str) -> std::path::PathBuf {
        self.dados.join(id)
    }
}

/// How a server is configured.
///
/// `specs/04-servidor-seele.md` describes a TOML file; M2 takes the same fields
/// as a struct and leaves parsing for M3, when there is persistent state worth
/// configuring.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Onde os MODs deste servidor moram em disco, se houver.
    ///
    /// `None` desliga MODs inteiramente, e é o padrão: um servidor que nunca
    /// ouviu falar de MOD não deve ganhar uma thread de interpretador por
    /// existir. ADR 0045.
    pub mods_dir: Option<RaizesDosMods>,
    /// What this server is called.
    pub name: String,
    /// Where to listen. UDP; QUIC needs no second port.
    ///
    /// O padrão é `[::]`, e a diferença importa: um socket IPv6 de pilha dupla
    /// atende as duas famílias, e `0.0.0.0` atende só IPv4. Era `0.0.0.0` até o
    /// degrau 2 do ADR 0022 — e por isso um servidor não atendia em IPv6 nem
    /// quando as duas pontas tinham. Ver [`alcance::abrir_escuta`].
    pub listen: SocketAddr,
    /// The one voice room M2 offers.
    pub voice_room: VoiceRoomId,
    /// Its display name.
    pub voice_room_name: String,
    /// How many people fit in it.
    pub voice_room_limit: u16,
    /// Nicknames that arrive as Observador rather than Pessoa.
    ///
    /// M3 brought real accounts, so this is now only a bootstrap convenience:
    /// somebody has to be able to configure the first roles before there is an
    /// operator to do it. Authorisation itself is PERMISSIONS's, always.
    pub observers: Vec<String>,
    /// Where PERSISTENCE keeps the database.
    pub database: crate::persistence::Location,
    /// Where the attachment blobs live. ADR 0027.
    ///
    /// `None` derives it: `anexos/` beside the database file. Beside it, and
    /// **not inside it** — a blob in SQLite frees pages without shrinking the
    /// file, so the worst case on disk would become the historical high-water
    /// mark rather than the ceiling, which is the one property this whole
    /// decision was taken for.
    ///
    /// Nothing to derive from means no attachments: a server in memory has no
    /// directory to own, and the honest answer to a transfer is
    /// `AttachmentRefusal::Unavailable` rather than a folder appearing in
    /// whatever directory the process happened to start in. Tests that want
    /// attachments say where.
    pub anexos: Option<std::path::PathBuf>,
    /// Quanto a subida desta máquina carrega, em bits por segundo.
    ///
    /// O §5.1 do desenho de compartilhamento de tela pôs este número no meio do
    /// teto do vídeo — `caminho de quem hospeda × 60% ÷ N espectadores` —, e é
    /// a perna que estourava sem ser medida: com o servidor encaminhando, quem
    /// sobe N cópias é esta máquina.
    ///
    /// **Declarado, não medido**, e a diferença viaja no fio. `None` é o caso
    /// comum e quer dizer que ninguém sabe: a admissão daqui cai na hipótese de
    /// `crate::tela::CAMINHO_DO_SERVER_BPS` e o `HostUplink` sai **zero**, que
    /// pelo protocolo é «não medi» e não «zero bit por segundo». Ver
    /// `crate::tela::caminho_no_fio`.
    pub caminho_bps: Option<u32>,
    /// A partir de que versão do protocolo o anúncio de MODs sai.
    ///
    /// O padrão é [`seele_proto::mods::VERSAO_DO_ANUNCIO`] e **quase ninguém
    /// deve mexer nisto**. Nasceu por uma razão que tinha data de validade, e a
    /// data passou: enquanto a versão global do protocolo não alcançava a do
    /// anúncio, nenhum par conseguia aceitar coisa nenhuma, e sem esta porta o
    /// caminho verdadeiro — servidor de produção anunciando, cliente de
    /// produção aceitando — não rodava em lugar nenhum, nem em teste.
    ///
    /// **Desde 14/09/2026 o padrão já é alcançável** (`PROTOCOL_VERSION` subiu
    /// para 5 na integração conjunta com a malha), e os testes da costura em
    /// `crates/seele-conformance/tests/aceite_dos_mods.rs` correm sem tocar
    /// neste campo. O que sobrou para ele é o outro lado: **subir** o limiar,
    /// para exercitar o ramo dormente de propósito em vez de deixá-lo apodrecer
    /// — «existir não é funcionar».
    ///
    /// # O que baixar isto custa
    ///
    /// Mandar o anúncio a um par que não conhece a variante **mata o fluxo de
    /// controle dele**: o postcard indexa variante por posição, e um quadro
    /// desconhecido desloca a leitura para sempre. É a tela preta sem mensagem
    /// nenhuma. Por isso quem baixa o valor está declarando que as duas pontas
    /// são a mesma build — verdade num teste do workspace, e só nele. O
    /// arranque avisa no log de quem hospeda quando isto acontece.
    ///
    /// Não há como configurar isto de fora: [`ServerConfig`] não é
    /// desserializada de arquivo nenhum.
    pub versao_do_anuncio: u8,
    /// O que «mensagem confirmada» promete sobreviver. R14.
    ///
    /// O padrão é [`crate::persistence::Durabilidade::Normal`] — sobrevive à
    /// queda do processo, e não a uma queda de energia. Ver o doc daquele enum
    /// para a escolha e o que cada lado custa.
    pub durabilidade: crate::persistence::Durabilidade,
    /// Quantos dias de histórico este servidor guarda, se ele limita. R15.
    ///
    /// `None` é ilimitado, que é o padrão que `specs/04-servidor-seele.md`
    /// escreve. Um número liga a limpeza periódica — ver
    /// [`spawn_retencao`].
    ///
    /// # Por que ela é opcional, e por que ela precisa existir
    ///
    /// `Messages::prune` existia e **nada em produção a chamava**: a busca por
    /// chamadas encontrava só teste. Um servidor auto-hospedado crescia para
    /// sempre e a única saída era editar o banco à mão. R15 da revisão da v15.
    ///
    /// Opcional porque apagar conversa é decisão de quem hospeda, e o padrão do
    /// produto é guardar. Ligá-la é uma escolha com consequência, e a
    /// consequência é irreversível.
    pub retencao_em_dias: Option<u32>,
    /// A identidade TLS, quando quem sobe o servidor já tem uma.
    ///
    /// `None` é o caso de sempre: lida do banco, ou gerada e guardada nele
    /// ([`tls::Identity::load_or_create`]). A sala pessoal passa a dela, feita
    /// da chave da pessoa por [`tls::Identity::da_semente`]. Essa **nunca vai
    /// para o banco**: é a chave da pessoa, e não a do servidor, e um banco que a
    /// guardasse seria uma segunda cópia da `identity.key` fora do lugar que o
    /// ADR 0017 protege.
    ///
    /// [`Daemon::bind`] a tira daqui antes de a configuração ir para as
    /// sessões, então nenhuma sessão segura a chave.
    pub identidade: Option<tls::Identity>,
    /// Quando este servidor é a sala pessoal de alguém, de quem ela é.
    ///
    /// `None` é todo servidor de sempre. Com uma [`recepcao::Sala`], o aperto
    /// de mão ganha a fase do toque (ADR 0059): quem liga é segurado até quem
    /// recebe decidir, e quem recebe entra sem tocar. A recepção fica em
    /// [`server::Server::recepcao`].
    pub sala: Option<recepcao::Sala>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            // Neutro, como todo o resto do que a pessoa lê: quem sobe um
            // `seeled` sem configurar nome vê «Servidor» e não o vocabulário
            // que saiu da interface. Os dois clientes que hospedam de dentro
            // passam «Casa» e nunca chegam aqui.
            name: "Servidor".into(),
            listen: SocketAddr::from((
                std::net::Ipv6Addr::UNSPECIFIED,
                seele_proto::transport::DEFAULT_PORT,
            )),
            voice_room: VoiceRoomId(1),
            // A sala que todo servidor nasce com. Este nome **aparece na
            // tela** de quem entra, e ficou para trás na troca de vocabulário
            // porque ela varreu `apps/` e a TUI, e não o servidor.
            voice_room_name: "SALA 1".into(),
            voice_room_limit: 15,
            observers: Vec::new(),
            database: crate::persistence::Location::Memory,
            // Sem MODs por padrão: uma thread de interpretador é caro demais
            // para um servidor que nunca ouviu falar deles. ADR 0045.
            mods_dir: None,
            anexos: None,
            caminho_bps: None,
            versao_do_anuncio: seele_proto::mods::VERSAO_DO_ANUNCIO,
            durabilidade: crate::persistence::Durabilidade::default(),
            // Ilimitado, que é o padrão da spec. Ver o campo.
            retencao_em_dias: None,
            // A de sempre: a do banco.
            identidade: None,
            // Um servidor de sempre, e não uma sala pessoal.
            sala: None,
        }
    }
}

/// Quantas horas entre duas limpezas de retenção.
///
/// Seis, e não uma: a janela é medida em dias, então o instante da limpeza não
/// precisa de precisão nenhuma — e uma passada de hora em hora seria seis vezes
/// mais I/O para apagar as mesmas linhas.
const INTERVALO_DA_RETENCAO: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);

/// Liga a limpeza periódica do histórico. R15.
///
/// # Por que ela é uma tarefa, e não um passo do arranque
///
/// Porque um servidor fica de pé por semanas. Uma limpeza só no arranque
/// atenderia quem reinicia todo dia e ninguém mais — e quem hospeda em casa, que
/// é para quem este produto é, é justamente quem não reinicia.
///
/// A primeira passada acontece **ao ligar**, antes do primeiro intervalo: quem
/// acabou de escolher uma janela de retenção espera que ela valha para o que já
/// está lá, e não daqui a seis horas.
///
/// # O que ela apaga, e o que ela não
///
/// Mensagens mais velhas que a janela, por [`persistence::messages::Messages::prune`].
/// **Não** apaga anexos nem dados de MOD: os blobs têm o teto e a expiração
/// deles (ADR 0027), e o volume de MOD deliberadamente não tem quota (ADR 0048).
/// Uma limpeza que decidisse sobre os três seria uma limpeza escolhendo por quem
/// hospeda em duas coisas que ele já decidiu.
fn spawn_retencao(
    persistence: Arc<tokio::sync::Mutex<persistence::Persistence>>,
    dias: u32,
    events: tokio::sync::broadcast::Sender<server::Event>,
) {
    // Zero é «ilimitado» para o `prune`, e chegar aqui com zero seria uma tarefa
    // acordando de seis em seis horas para não fazer nada.
    if dias == 0 {
        tracing::warn!("retenção de zero dia é ilimitada; a limpeza não foi ligada");
        return;
    }
    tokio::spawn(async move {
        let mut relogio = tokio::time::interval(INTERVALO_DA_RETENCAO);
        relogio.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            relogio.tick().await;
            let apagadas = {
                let mut guarda = persistence.lock().await;
                let messages = persistence::messages::Messages::new(&mut guarda);
                match messages.prune(dias) {
                    Ok(quantas) => quantas,
                    Err(erro) => {
                        // **Dito, e não engolido.** Uma retenção que falha em
                        // silêncio é um disco que enche com quem hospeda achando
                        // que a limpeza está funcionando.
                        tracing::error!(%erro, dias, "a limpeza de histórico falhou");
                        continue;
                    }
                }
            };
            if apagadas > 0 {
                tracing::info!(
                    apagadas,
                    dias,
                    "histórico além da janela de retenção apagado"
                );
                // A sala fica sabendo: uma casca que tem a última página na tela
                // está mostrando mensagens que já não existem. Sem este aviso, a
                // próxima busca de histórico as perderia sem explicação.
                let _ = events.send(server::Event::HistoricoApagado { apagadas });
            }
        }
    });
}

/// Where the blobs go, given where the database is.
///
/// `anexos/` beside `seele.db`. A database with no file — the in-memory one
/// tests use — has nowhere to put them, and says so instead of guessing.
#[must_use]
pub fn diretorio_de_anexos(config: &ServerConfig) -> Option<std::path::PathBuf> {
    if let Some(escolhido) = &config.anexos {
        return Some(escolhido.clone());
    }
    match &config.database {
        persistence::Location::File(path) => Some(
            path.parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .join("anexos"),
        ),
        persistence::Location::Memory => None,
    }
}

/// Creates the configured voice room and its Channel if they are not there yet.
///
/// M2 kept these in the config struct; M3 keeps them in PERSISTENCE so a restart
/// finds the same room rather than rebuilding it. Idempotent, because it runs
/// on every boot.
///
/// # A server opens with somewhere to go
///
/// This is what stops a fresh server being a screen that does not explain
/// itself. A room with na sala de voz offers nowhere to speak and no canal to write in;
/// the person who pressed **HOSPEDAR AQUI** looks at an empty list and has no
/// way to tell a working server from a broken one. So the first boot writes a
/// Channel called `geral` and one voice room bound to it, and there is somewhere to
/// stand from the first second.
///
/// # Why here, and not in the migration
///
/// A migration would look cheaper — one more `INSERT` in migration 1 — and it
/// is the wrong place, for two reasons that only show up later.
///
/// **A migration is irreversible and append-only** ([`persistence::schema`]). The
/// name of a voice room is not: `ClientMessage::RenameVoiceRoom` exists, and the whole
/// point of hosting your own server is that the rooms are yours. Seeded content
/// baked into a schema version would be a name the operator can change and the
/// history of the schema still claims.
///
/// **The voice room's name and limit come from [`ServerConfig`]**, which a migration
/// cannot see. Migrations run inside PERSISTENCE, before there is a config to
/// consult, and passing one in would make "which SQL is this database at" depend
/// on a runtime value.
///
/// `INSERT OR IGNORE` is what keeps it honest on the second boot: a voice room that
/// has since been renamed keeps its new name, because the conflicting insert is
/// skipped rather than applied.
///
/// # Why a flag, and not only `INSERT OR IGNORE`
///
/// Because the identifiers are written out by hand, `INSERT OR IGNORE` means
/// "unless row 1 is taken" — and once a channel can be **destroyed**, row 1 stops
/// being taken. A server whose operator destroyed `geral` on purpose, after a
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

fn seed(persistence: &mut persistence::Persistence, config: &ServerConfig) -> Result<()> {
    let connection = persistence.connection();
    let ja: i64 = connection.query_row(
        "SELECT COUNT(*) FROM configuracao WHERE chave = ?1",
        [SEMEADO],
        |linha| linha.get(0),
    )?;
    if ja > 0 {
        return Ok(());
    }

    connection.execute(
        "INSERT OR IGNORE INTO channels (id, name) VALUES (1, 'geral')",
        [],
    )?;
    connection.execute(
        "INSERT OR IGNORE INTO voice_rooms (id, name, member_limit, channel_id)
         VALUES (?1, ?2, ?3, 1)",
        rusqlite::params![
            i64::from(config.voice_room.get()),
            config.voice_room_name,
            i64::from(config.voice_room_limit)
        ],
    )?;
    connection.execute(
        "INSERT INTO configuracao (chave, valor) VALUES (?1, '1')",
        [SEMEADO],
    )?;
    Ok(())
}

/// A running server.
pub struct Daemon {
    endpoint: quinn::Endpoint,
    config: Arc<ServerConfig>,
    pilha: alcance::Pilha,
    /// O mesmo socket do endpoint, para escrever. Ver [`Daemon::espelho`].
    espelho: Option<Arc<std::net::UdpSocket>>,
    fingerprint: String,
    /// As janelas dos certificados curtos do `h3` (ADR 0057), da vigente em
    /// diante. Vazias na sala pessoal. Ver [`Daemon::janelas_do_navegador`].
    janelas: Vec<seele_proto::control::JanelaDoNavegador>,
    /// Quantos apertos de mão do `h3` acabaram com o navegador recusando o
    /// certificado. Ver [`Daemon::recusas_do_navegador`].
    recusas_do_navegador: Arc<std::sync::atomic::AtomicU64>,
    registry: Arc<session::Registry>,
    server: Arc<server::Server>,
    voice_rooms: Arc<voice_room::VoiceRooms>,
}

/// Se um aperto de mão acabou porque o cliente recusou o certificado, o alerta
/// de TLS com que ele recusou: do 42 ao 46 (o certificado ruim, sem suporte,
/// revogado, vencido ou desconhecido) e o 48 (a autoridade desconhecida). O
/// Safari do iPhone recusa com o 46 (a medição 3).
///
/// **Pelo texto do erro**, e não pelo código: o `wtransport` embrulha o
/// fechamento do `quinn` sem expor o código, e o texto que o `quinn-proto`
/// escreve para um alerta de TLS é fixo (`the cryptographic handshake failed:
/// error N`, em `TransportErrorCode`). Uma versão que o mude reprova
/// `o_navegador_que_recusa_o_certificado_e_contado`, em `tests/alpn.rs`.
fn alerta_de_certificado(erro: &str) -> Option<u8> {
    let depois = erro
        .split("the cryptographic handshake failed: error ")
        .nth(1)?;
    let numero: String = depois.chars().take_while(char::is_ascii_digit).collect();
    let alerta = numero.parse::<u8>().ok()?;
    [42_u8, 43, 44, 45, 46, 48]
        .contains(&alerta)
        .then_some(alerta)
}

/// **A sessão do navegador**: o `wtransport` adota a conexão `h3` que este
/// endpoint aceitou e lê o pedido da WebTransport, e a sessão segue pelo mesmo
/// `session::serve` do nativo (Tarefa 2.6 da F4).
///
/// O caminho e a `Origin` do pedido vão ao registro. **Conferir a `Origin` está
/// em aberto no ADR 0057** («O que este ADR não decide»): até o dono decidir, o
/// pedido é aceito com qualquer uma, e o registro diz qual veio.
async fn aceitar_o_navegador(
    conectando: quinn::Connecting,
    recusas_do_navegador: &std::sync::atomic::AtomicU64,
) -> Option<transporte::Conexao> {
    let pedido = match wtransport::endpoint::IncomingSessionFuture::with_quic_connecting(conectando)
        .await
    {
        Ok(pedido) => pedido,
        Err(erro) if alerta_de_certificado(&erro.to_string()).is_some() => {
            recusas_do_navegador.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            tracing::warn!(
                %erro,
                "um navegador recusou o certificado: link velho, ou link de outro \
                 servidor que atende neste endereço noutra rede"
            );
            return None;
        }
        Err(erro) => {
            tracing::warn!(%erro, "a sessão do navegador não abriu: a conexão caiu antes do pedido");
            return None;
        }
    };
    tracing::info!(
        caminho = pedido.path(),
        origem = pedido.origin().unwrap_or("(nenhuma)"),
        "um navegador pediu a sessão pelo `h3`"
    );
    match pedido.accept().await {
        Ok(conexao) => Some(transporte::Conexao::Web(conexao)),
        Err(erro) => {
            tracing::warn!(%erro, "a sessão do navegador caiu ao ser aceita");
            None
        }
    }
}

impl Daemon {
    /// Binds the endpoint and starts the voice room task.
    ///
    /// # Errors
    ///
    /// Fails if the certificate cannot be generated or the socket cannot bind.
    pub async fn bind(mut config: ServerConfig) -> Result<Self> {
        // rustls 0.23 requires a crypto provider to be chosen explicitly. Doing
        // it here rather than in `main` means the integration tests get it too.
        let _ = rustls::crypto::ring::default_provider().install_default();

        // PERSISTENCE first: the handshake needs accounts before it can answer
        // anybody, and migrations run at boot (specs/04-servidor-seele.md).
        // A identidade TLS também mora nele, então ele vem antes dela.
        // **Com a durabilidade escolhida**, e não com um padrão escondido num
        // pragma. Ver `persistence::Durabilidade`: o que «confirmada» promete é
        // decisão de quem hospeda, e a frase que descreve a promessa mora no
        // mesmo lugar que a escolha.
        let mut persistence =
            persistence::Persistence::abrir_com(&config.database, config.durabilidade)?;
        tracing::info!(
            durabilidade = config.durabilidade.pragma(),
            "{}",
            config.durabilidade.promessa()
        );
        seed(&mut persistence, &config)?;

        // Lida do banco, não gerada a cada vez. Sem isto, reiniciar o servidor
        // trocava a chave e todo cliente já conectado via `A CHAVE DO SERVIDOR
        // MUDOU` — o alerta bloqueante do ADR 0003 — e era recusado. O aviso
        // reservado para ataque disparando num reinício de rotina é como se
        // ensina a ignorá-lo.
        //
        // A sala pessoal traz a dela, e ela sai da configuração aqui: as
        // sessões recebem a configuração, e não precisam da chave.
        let identity = match config.identidade.take() {
            Some(identidade) => identidade,
            None => tls::Identity::load_or_create(
                &persistence,
                vec!["localhost".into(), config.listen.ip().to_string()],
            )?,
        };
        let fingerprint = identity.fingerprint();
        let impressao = identity.digest();
        // Os curtos do `h3`: a janela vigente e as anunciadas depois dela, com
        // a chave do longo (a medição 3). A sala pessoal não tem: a chave dela é
        // Ed25519, e o navegador não a aceita pelo hash.
        let agora = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let curtos = tls::curtos_do_banco(&persistence, &identity, agora)?;
        let janelas = curtos.iter().map(tls::Curto::janela).collect();
        let server_config = tls::server_config_do_seeled(identity, &curtos)?;

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
        tracing::info!(?pilha, escuta = %endpoint.local_addr()?, "o servidor está atendendo");

        // Before the mutex, because opening it sweeps the directory against the
        // table and both are still ours alone at this point. ADR 0027 wants the
        // ceiling standing before the first connection, not after.
        let anexos = match diretorio_de_anexos(&config) {
            Some(root) => Some(Arc::new(transfer::Vault::open(root, &persistence)?)),
            None => {
                tracing::info!("anexos: sem diretório, este servidor não guarda arquivo");
                None
            }
        };

        // **Antes do mutex**, pela mesma razão que os anexos acima: o banco
        // ainda é nosso sozinho aqui, e o número tem de estar de pé antes da
        // primeira conexão. Um erro de leitura não impede o servidor de subir —
        // sem memória ele cai na ordem de sempre, que é o comportamento de
        // antes desta memória existir.
        let subida_lembrada = crate::persistence::subida::lembrada(&persistence)
            .inspect_err(|erro| {
                tracing::warn!(%erro, "não deu para ler a subida medida do arranque anterior");
            })
            .ok()
            .flatten();
        let caminho_do_arranque = tela::caminho_do_arranque(subida_lembrada, config.caminho_bps);
        match subida_lembrada {
            Some(bps) => tracing::info!(bps, "a sonda começa da subida medida da última vez"),
            None => tracing::info!(
                bps = caminho_do_arranque,
                "nenhuma subida medida guardada; o arranque usa o declarado ou a hipótese"
            ),
        }

        // **O aviso de quem baixou o limiar do anúncio.** Ver
        // `ServerConfig::versao_do_anuncio`: abaixo da versão global, o anúncio
        // sai para pares que podem não conhecer a variante, e o fluxo de
        // controle deles morre sem uma palavra. É verdade num teste do
        // workspace, onde as duas pontas são a mesma build, e é um defeito em
        // qualquer outro lugar — então ele aparece no log.
        if config.versao_do_anuncio < seele_proto::mods::VERSAO_DO_ANUNCIO {
            tracing::warn!(
                limiar = config.versao_do_anuncio,
                padrao = seele_proto::mods::VERSAO_DO_ANUNCIO,
                "este servidor anuncia MODs abaixo da versão em que o anúncio viaja; \
                 um par publicado que receba o quadro perde o fluxo de controle"
            );
        }

        let persistence = Arc::new(tokio::sync::Mutex::new(persistence));

        let (events, _) = tokio::sync::broadcast::channel(server::ANEL_DO_BARRAMENTO);
        let writes = server::spawn_writer(Arc::clone(&persistence), events.clone());
        // **A retenção, quando quem hospeda a escolheu.** R15: `Messages::prune`
        // existia e nada em produção a chamava, então um servidor
        // auto-hospedado crescia para sempre e a única saída era editar o banco
        // à mão.
        if let Some(dias) = config.retencao_em_dias {
            spawn_retencao(Arc::clone(&persistence), dias, events.clone());
        }
        // A recepção da sala pessoal, quando este servidor é uma, ligada ao
        // mesmo barramento das sessões: é por ele que o desligar chega a quem
        // está dentro.
        let recepcao = config
            .sala
            .map(|sala| Arc::new(recepcao::Recepcao::da_sala(sala, events.clone())));
        let server = Arc::new(server::Server {
            mods_dir: config.mods_dir.clone(),
            persistence,
            events,
            writes,
            slots: Arc::new(tokio::sync::Mutex::new(server::Slots::default())),
            occupancy: Arc::new(tokio::sync::Mutex::new(server::Occupancy::default())),
            presentes: Arc::new(tokio::sync::Mutex::new(server::Presentes::default())),
            subida: Arc::new(tokio::sync::Mutex::new(match subida_lembrada {
                Some(bps) => crate::tela::Subida::partindo_de(bps),
                None => crate::tela::Subida::nova(),
            })),
            portaria: Arc::new(tokio::sync::Mutex::new(taxa::Portaria::nova())),
            atrasos: Arc::new(server::Atrasos::default()),
            desconhecidas: Arc::new(server::MensagensDesconhecidas::default()),
            desassentamentos: Arc::new(server::Desassentamentos::default()),
            telas: Arc::new(tokio::sync::Mutex::new(server::Telas::default())),
            pares: Arc::new(tokio::sync::Mutex::new(pares::Pares::nova())),
            anexos,
            caminho_bps: config.caminho_bps,
            versao_do_anuncio: config.versao_do_anuncio,
            esperas: Arc::new(std::sync::Mutex::new(mods::volume::Esperas::default())),
            mods_em_curso: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            pacotes_conferidos: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            avisador: Arc::new(crate::mods::aviso::Avisador::default()),
            impressao,
            recepcao,
        });

        // Os MODs deste servidor, se houver pasta. ADR 0045.
        //
        // Aqui e não no `Daemon`: é o servidor montado que tem banco e
        // barramento, que são as duas coisas de que o despachante precisa. E
        // fica **depois** do `Server` existir, porque um MOD que reagisse a um
        // evento antes de haver sala reagiria a uma sala que não existe.
        if let Some(pasta_dos_mods) = config.mods_dir.clone() {
            let ligados = {
                let banco = server.persistence.lock().await;
                // A API e o tema das linhas habilitadas antes das migrações 14
                // e 16, lidos dos bytes do hash numa leitura só. Antes de montar
                // qualquer anúncio, e ditos no registro quando não se preenchem
                // (D-fio-7, D-api-8).
                for queixa in crate::mods::preencher_api_e_tema(&banco, &pasta_dos_mods) {
                    tracing::warn!("MOD habilitado sem API ou tema conhecidos — {queixa}");
                }
                crate::persistence::mods::enabled(&banco).unwrap_or_default()
            };
            // O par, e não só o identificador: é o hash que diz **quais bytes**
            // este servidor exige, e outro servidor desta máquina pode exigir
            // outros do mesmo MOD.
            let exigidos: Vec<(String, String)> = ligados
                .into_iter()
                .map(|ligado| (ligado.id, ligado.hash))
                .collect();

            if !exigidos.is_empty() {
                let (fontes, queixas) = crate::mods::carregar_do_disco(&pasta_dos_mods, &exigidos);
                for queixa in queixas {
                    // Um MOD habilitado que não abre é dito em voz alta. O
                    // silêncio aqui seria quem hospeda perguntando dias depois
                    // por que a sala não faz o que ele ligou.
                    tracing::error!("MOD habilitado e não carregado — {queixa}");
                }
                let (despachante, recusados) = crate::mods::despacho::Despachante::iniciar(fontes);
                for (id, falha) in recusados {
                    tracing::error!(mod_id = %id, %falha, "MOD recusado ao subir; a sala sobe sem ele");
                }
                tokio::spawn(crate::mods::despacho::acompanhar(
                    server.events.subscribe(),
                    server.events.downgrade(),
                    Arc::clone(&server.persistence),
                    despachante,
                    pasta_dos_mods.clone(),
                    Arc::clone(&server.avisador),
                ));
            }
        }

        // Held seats have to be released even if nobody reconnects, or a server
        // slowly fills with places kept for people who left for good.
        let sweeper = Arc::clone(&server.slots);
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

        // Uma tarefa por sala de voz, criada quando alguém entra. Antes havia
        // exatamente uma, a da sala de voz do `ServerConfig`, e toda sessão segurava
        // esse único remetente: correto enquanto um servidor tinha uma sala,
        // silenciosamente errado no instante em que passou a poder ter duas.
        let voice_rooms = Arc::new(voice_room::VoiceRooms::new(
            caminho_do_arranque,
            server.events.clone(),
        ));

        Ok(Self {
            endpoint,
            config: Arc::new(config),
            pilha,
            espelho,
            fingerprint,
            janelas,
            recusas_do_navegador: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            registry: Arc::new(session::Registry::new()),
            server,
            voice_rooms,
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

    /// **As janelas dos certificados curtos do `h3`** (ADR 0057): o hash e a
    /// validade de cada um, da vigente em diante, como o
    /// `ServerMessage::HashesDoNavegador` as leva. É o que o link do navegador
    /// escreve no `wh=`. Vazias quando o servidor não serve ao navegador (a
    /// sala pessoal).
    #[must_use]
    pub fn janelas_do_navegador(&self) -> &[seele_proto::control::JanelaDoNavegador] {
        &self.janelas
    }

    /// Quantas vezes um navegador recusou o certificado curto no aperto de mão
    /// do `h3`: um link velho, ou de outro servidor que atende neste endereço
    /// noutra rede. Cada uma vai ao registro com essa frase; a conta é para
    /// quem investiga, e para o teste que a prova.
    #[must_use]
    pub fn recusas_do_navegador(&self) -> u64 {
        self.recusas_do_navegador
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// O socket em que o servidor atende, para **escrever**. Degrau 4 do ADR 0022.
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
    /// servidor que caiu para IPv4 é um servidor que ninguém só-IPv6 alcança, e essa
    /// é a diferença entre "não conecta" e "não conecta **porque**".
    #[must_use]
    pub fn pilha(&self) -> alcance::Pilha {
        self.pilha
    }

    /// A impressão digital lida direto do banco, sem subir servidor.
    ///
    /// É o que `seeled convite` precisa: o operador está com o servidor parado
    /// quando gera um convite, e o link tem de carregar a mesma impressão que
    /// o servidor vai apresentar depois.
    ///
    /// # Errors
    ///
    /// Falha se o banco não responder ou não tiver identidade ainda.
    pub fn fingerprint_do_banco(persistence: &persistence::Persistence) -> Result<String> {
        let cert: Vec<u8> = persistence.connection().query_row(
            "SELECT valor FROM configuracao WHERE chave = 'tls_cert'",
            [],
            |linha| linha.get(0),
        )?;
        Ok(seele_proto::transport::certificate_fingerprint(&cert))
    }

    /// A política de admissão deste server.
    ///
    /// # Errors
    ///
    /// Falha se o banco não responder.
    pub fn politica_de_admissao(&self) -> Result<admissao::Politica> {
        let guard = self
            .server
            .persistence
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
            let server = Arc::clone(&self.server);
            let voice_rooms = Arc::clone(&self.voice_rooms);
            let recusas_do_navegador = Arc::clone(&self.recusas_do_navegador);

            tokio::spawn(async move {
                let mut conectando = match incoming.accept() {
                    Ok(conectando) => conectando,
                    Err(error) => {
                        tracing::warn!(%error, "connection failed before the handshake");
                        return;
                    }
                };
                // O ALPN que o aperto de mão escolheu: `seele/1` do nativo, ou o
                // `h3` do navegador (ADR 0057). Ele sai antes de o aperto acabar,
                // e é o que separa a recusa de um navegador da de um nativo.
                let alpn = conectando
                    .handshake_data()
                    .await
                    .ok()
                    .and_then(|dados| {
                        dados
                            .downcast::<quinn::crypto::rustls::HandshakeData>()
                            .ok()
                    })
                    .and_then(|dados| dados.protocol);
                let do_navegador =
                    alpn.as_deref() == Some(seele_proto::transport::ALPN_DO_NAVEGADOR);
                let conexao = if do_navegador {
                    match aceitar_o_navegador(conectando, &recusas_do_navegador).await {
                        Some(conexao) => conexao,
                        None => return,
                    }
                } else {
                    match conectando.await {
                        Ok(connection) => transporte::Conexao::Quic(connection),
                        Err(error) => {
                            tracing::warn!(%error, "connection failed before the handshake");
                            return;
                        }
                    }
                };
                let peer = conexao.remote_address();
                tracing::info!(%peer, navegador = conexao.do_navegador(), "link unverified");

                if let Err(error) =
                    session::serve(conexao, config, registry, server, voice_rooms).await
                {
                    tracing::info!(%peer, %error, "session closed");
                }
            });
        }
        Ok(())
    }

    /// Borrows the shared state, for tests and for tooling.
    #[must_use]
    pub fn server(&self) -> &Arc<server::Server> {
        &self.server
    }

    /// As salas de voz vivas, for tests and for tooling.
    ///
    /// Existe para que um teste de ponta a ponta possa ler os contadores de uma
    /// sala — em particular o do guarda de sessão, que é a única prova de que
    /// uma conexão velha morreu **e foi barrada**, e não de que ela simplesmente
    /// não chegou a morrer. Ver [`voice_room::VoiceRooms::contadores`].
    #[must_use]
    pub fn voice_rooms(&self) -> &Arc<voice_room::VoiceRooms> {
        &self.voice_rooms
    }

    /// How many messages PERSISTENCE holds on o canal.
    ///
    /// The counterpart of [`Self::mensagens_da_linha`], and the one that answers
    /// "did anything get lost": a page is capped, a count is not. See
    /// [`persistence::messages::Messages::count`].
    ///
    /// # Errors
    ///
    /// Fails if the database is busy or does not answer.
    pub async fn quantas_mensagens(&self, channel: seele_proto::ids::ChannelId) -> Result<u64> {
        let mut guard = self.server.persistence.lock().await;
        persistence::messages::Messages::new(&mut guard).count(channel)
    }

    /// What PERSISTENCE actually recorded on o canal, newest first.
    ///
    /// One page, capped at [`persistence::messages::MAX_PAGE`] like every other
    /// reader of history — [`Self::quantas_mensagens`] is what counts.
    ///
    /// # Why this exists, and why it is not `Persistence::connection()`
    ///
    /// `docs/pendencias.md` #1 is a delivery bug, and a delivery bug has two
    /// legs: what the server **took in** and what it **handed out**. Measuring
    /// only the second leg cannot tell a message that was never stored from one
    /// that was stored and never sent, and those two have nothing in common but
    /// the symptom. So a test needs to see the first leg.
    ///
    /// The pendência recorded the obstacle as `Persistence::connection()` being
    /// `pub(crate)`. Making it public would have worked and would have been the
    /// wrong trade: it hands out a `rusqlite::Connection`, and from that moment
    /// the schema is the contract. A test that writes its own `SELECT` goes on
    /// passing after a migration renames a column and starts asserting about a
    /// table nobody meant it to know.
    ///
    /// This asks the question instead of the storage: *what is on this Channel?*
    /// It goes through [`persistence::messages::Messages`] — the same reader the
    /// session uses for `FetchHistory` — so a test and a client see the same
    /// answer by construction, and the schema stays behind the crate wall.
    ///
    /// # Errors
    ///
    /// Fails if the database is busy or does not answer.
    pub async fn mensagens_da_linha(
        &self,
        channel: seele_proto::ids::ChannelId,
        limite: u16,
    ) -> Result<Vec<persistence::messages::StoredMessage>> {
        let mut guard = self.server.persistence.lock().await;
        persistence::messages::Messages::new(&mut guard).history(channel, None, limite)
    }

    /// Para de aceitar e fecha o endpoint, **na hora e sem avisar ninguém**.
    ///
    /// Síncrono de propósito: é o que o `Drop` de quem hospeda pode chamar, e
    /// um `drop` não espera nada. Quem estava dentro lê a conexão caindo, e não
    /// um motivo. Quem pode esperar chama [`Self::encerrar_avisando`], que avisa
    /// cada sessão antes e só então chega aqui.
    pub fn shutdown(&self) {
        self.endpoint.close(0_u32.into(), b"shutting down");
    }

    /// Avisa cada sessão de que o servidor vai cair, espera elas se
    /// despedirem até `prazo`, e só então fecha o endpoint.
    ///
    /// # O defeito que isto conserta
    ///
    /// Quem hospeda saía por [`Self::shutdown`], e quem estava dentro lia uma
    /// queda de transporte: a casca dizia «não foi possível alcançar o
    /// servidor», e a bateria batia cinco minutos numa porta que ninguém ia
    /// atender. O protocolo tinha a frase certa, `ServerShuttingDown`, e ninguém
    /// a mandava. O `specs/04-servidor-seele.md` pede o aviso; o que faltava era
    /// como alcançar cada sessão sem um registro delas, e a resposta é o
    /// barramento, que toda sessão já escuta.
    ///
    /// # Como
    ///
    /// Difunde [`server::Event::ServidorEncerrando`] com `reason`. Cada sessão o
    /// atende no braço da expulsão: escreve `Disconnecting { reason }`, espera
    /// até 1 s pela entrega (`session::despedir`) e fecha a conexão.
    ///
    /// A espera é pelo **endpoint**, com `wait_idle` sob `prazo`: ele volta
    /// quando não sobra conexão nenhuma, o que quer dizer que cada sessão se
    /// despediu e que o fechamento dela já saiu para o outro lado. Não há
    /// registro de sessões vivas para esperar por ele (`session::Registry` só
    /// conta), e o endpoint já é quem sabe quantas conexões há.
    ///
    /// O prazo é o teto, e não a espera de sempre: sem ninguém conectado o
    /// endpoint já está ocioso, e isto volta na hora. Ele vence em dois casos,
    /// e o endpoint não distingue um do outro:
    ///
    /// - **uma conexão que não se despede** — uma ainda no aperto de mão, que
    ///   não assinou o barramento, ou um par que não confirma a entrega. Essa
    ///   é fechada sem aviso pelo [`Self::shutdown`] do fim;
    /// - **uma sessão que já se despediu e ainda conta.** Uma conexão fechada
    ///   fica no endpoint pelo dreno do QUIC, três PTO, e um PTO é o RTT
    ///   suavizado, mais quatro vezes a variação, mais os 25 ms de atraso de
    ///   confirmação. No laço local isso são milissegundos; numa internet com
    ///   RTT de umas poucas dezenas de milissegundos (perto de 50 a 70 ms,
    ///   conforme a variação), passa de meio segundo. Essa sessão já leu o
    ///   aviso, o fechamento dela já saiu, e o `shutdown` não muda nada para
    ///   ela.
    ///
    /// # O que isto não faz
    ///
    /// Não espera o fechamento terminar depois do [`Self::shutdown`]: quem vai
    /// reabrir a porta, ou derrubar o runtime em seguida, chama
    /// [`Self::wait_idle`] depois.
    pub async fn encerrar_avisando(
        &self,
        reason: seele_proto::control::DisconnectReason,
        prazo: std::time::Duration,
    ) {
        // Sem sessão nenhuma escutando, o `send` falha, e não há a quem avisar.
        let _ = self
            .server
            .events
            .send(server::Event::ServidorEncerrando { reason });
        if tokio::time::timeout(prazo, self.endpoint.wait_idle())
            .await
            .is_err()
        {
            // O que se sabe, e só isso: o endpoint conta as conexões ainda
            // drenando junto com as que não se despediram. Ver o doc acima.
            tracing::info!(
                conexoes = self.endpoint.open_connections(),
                ?prazo,
                "o prazo da despedida acabou com conexões ainda no endpoint: fechando depois do \
                 aviso, ou sem ter se despedido (estas fecham agora, sem aviso)"
            );
        }
        self.shutdown();
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

/// `$SEELE_ENCONTRO` nos testes deste crate: trocada com trava, e devolvida
/// pelo `Drop`.
///
/// A variável é do processo, e o `libtest` roda os testes deste binário em
/// paralelo, no mesmo processo. Dois testes a trocam:
/// `um_ponto_de_encontro_fora_do_ar_nao_atrapalha_nada` (`hospedagem.rs`) e
/// `desligar_o_degrau_4_e_uma_variavel_de_ambiente` (`alcance/encontro.rs`).
/// Cada um a devolvia à mão, no fim: um pânico no meio deixava a troca para os
/// testes seguintes, e nada impedia os dois de trocá-la ao mesmo tempo.
///
/// O molde é o `AmbienteDoEncontro` de `quarto_pelo_link.rs`, com uma
/// diferença: lá a `vaga` já serializa o binário inteiro, e aqui quem
/// serializa é a trava que o guarda segura.
///
/// # Quem sobe hospedagem também a toma
///
/// Todo teste deste crate que chama `Hospedagem::iniciar` toma o guarda com o
/// degrau 4 desligado ([`AmbienteDoEncontro::sem_degrau_4`]). Antes eles só
/// liam a variável, sem trava, e sem ela `Convocacao::do_ambiente` caía no
/// ponto de encontro de produção: numa máquina sem IPv4 global, cada rodada
/// registrava um servidor de teste no quarto da VPS. Medido no lote
/// G-servidor da F5: rodados junto com
/// `um_ponto_de_encontro_fora_do_ar_nao_atrapalha_nada`, os oito passavam, e
/// passavam porque liam o `nao` ou o buraco negro que **aquele** teste tinha
/// posto; rodados sem ele, os oito reprovavam no guarda
/// (`alcance::encontro::recusar_a_producao`). Qual ponto cada um usava
/// dependia de quem subia primeiro.
///
/// A trava serializa os que sobem hospedagem. Medido na mesma máquina: os oito
/// em 1,08 s em fila, contra 0,52 s em paralelo.
///
/// # Depois do padrão seguro, por que o `nao` continua
///
/// Desde a decisão (d), `Hospedagem::iniciar` sem a variável já não convoca
/// ponto nenhum. O `nao` explícito e a trava ficam pelo que o padrão não cobre:
/// outros testes deste binário **trocam** a variável — para o buraco negro, e
/// para um IP da VPS no teste do guarda de `iniciar` —, e um teste de
/// hospedagem que subisse sem a trava nesse meio-tempo leria o valor do outro.
#[cfg(test)]
pub(crate) mod ambiente_de_teste {
    use std::sync::{Mutex, MutexGuard, PoisonError};

    use crate::alcance::encontro::VARIAVEL;

    /// A trava de quem troca `$SEELE_ENCONTRO` nos testes deste crate. Quem a
    /// toma é [`AmbienteDoEncontro::guardar`].
    static TRAVA: Mutex<()> = Mutex::new(());

    /// Guarda `$SEELE_ENCONTRO` e a devolve ao que era quando o teste acaba,
    /// **inclusive em pânico**, e segura a [`TRAVA`] enquanto vive.
    ///
    /// Quem devolve é o `Drop`, e nenhum caminho precisa lembrar de chamar nada
    /// à mão. Crie-o **antes** de qualquer troca, e troque só por ele
    /// ([`Self::pedir`], [`Self::tirar`]).
    ///
    /// # Por que a trava é um campo
    ///
    /// O teste de `hospedagem.rs` é `#[tokio::test] async`, e segura a troca
    /// através de `.await`. Um `let _t = TRAVA.lock()` solto no corpo do teste
    /// faz o clippy reprovar por `await_holding_lock`, e o CI roda o clippy com
    /// `-D warnings`. O campo não dispara o lint, e segura a trava pelo mesmo
    /// tempo que a troca. Ele torna o futuro do teste `!Send`, o que o
    /// `#[tokio::test]` de uma thread aceita.
    ///
    /// A ordem da queda é a que importa: o corpo do `drop` devolve a variável,
    /// e só depois os campos caem, a trava com eles. O próximo teste que a
    /// tomar acha a variável como ela era.
    #[must_use = "a variável só volta quando o guarda cai; soltá-lo na hora a devolve cedo demais"]
    pub(crate) struct AmbienteDoEncontro {
        /// O que `$SEELE_ENCONTRO` era antes: `None` é «não estava definida».
        antes: Option<std::ffi::OsString>,
        /// A trava, segura até o guarda cair.
        _trava: MutexGuard<'static, ()>,
    }

    impl AmbienteDoEncontro {
        /// Toma a [`TRAVA`] e guarda o valor de agora, sem trocar nada.
        ///
        /// Espera quem a tiver. Uma trava envenenada também serve: quem entrou
        /// em pânico com ela já devolveu a variável, pelo `Drop`.
        pub(crate) fn guardar() -> Self {
            Self::guardar_sob(&TRAVA)
        }

        /// [`Self::guardar`] com o degrau 4 desligado (`nao`) até o guarda
        /// cair.
        ///
        /// É o de quem sobe hospedagem e não mede o degrau 4: nenhum pacote
        /// sai para ponto de encontro nenhum, e o de produção nem é lido.
        pub(crate) fn sem_degrau_4() -> Self {
            let ambiente = Self::guardar();
            ambiente.pedir("nao");
            ambiente
        }

        /// [`Self::guardar`], sob a trava que quem chama escolhe.
        ///
        /// Existe para os testes deste módulo: eles seguram a [`TRAVA`] por
        /// fora, para nenhum outro teste trocar a variável no meio da medida, e
        /// o guarda que eles provam toma outra.
        fn guardar_sob(trava: &'static Mutex<()>) -> Self {
            let trava = trava.lock().unwrap_or_else(PoisonError::into_inner);
            Self {
                antes: std::env::var_os(VARIAVEL),
                _trava: trava,
            }
        }

        /// Troca `$SEELE_ENCONTRO` por `valor`, até este guarda cair.
        pub(crate) fn pedir(&self, valor: &str) {
            std::env::set_var(VARIAVEL, valor);
        }

        /// Tira `$SEELE_ENCONTRO` do ambiente, até este guarda cair.
        pub(crate) fn tirar(&self) {
            std::env::remove_var(VARIAVEL);
        }
    }

    impl Drop for AmbienteDoEncontro {
        fn drop(&mut self) {
            match self.antes.take() {
                Some(valor) => std::env::set_var(VARIAVEL, valor),
                None => std::env::remove_var(VARIAVEL),
            }
        }
    }

    #[cfg(test)]
    mod testes {
        use super::*;

        #[test]
        fn o_ambiente_do_encontro_volta_mesmo_quando_o_teste_entra_em_panico() {
            // A `TRAVA` por fora, segura do começo ao fim: nenhum outro teste
            // troca a variável entre o pânico e a conferência. O guarda que se
            // prova toma outra trava, só deste teste: tomar a `TRAVA` por
            // dentro, com ela já tomada aqui, travaria o teste.
            //
            // Sem `AmbienteDoEncontro` por fora: um segundo teria o mesmo
            // `Drop`, e quebraria junto com o guarda que este teste prova.
            static SO_DESTE_TESTE: Mutex<()> = Mutex::new(());
            let _ninguem_mais_troca = TRAVA.lock().unwrap_or_else(PoisonError::into_inner);
            let antes = std::env::var_os(VARIAVEL);

            let resultado = std::panic::catch_unwind(|| {
                let ambiente = AmbienteDoEncontro::guardar_sob(&SO_DESTE_TESTE);
                ambiente.pedir("valor-que-o-panico-nao-pode-deixar-para-tras");
                panic!("pânico provocado de propósito, com a variável já trocada");
            });

            assert!(
                resultado.is_err(),
                "o pânico provocado não aconteceu: este teste não mede nada"
            );
            assert_eq!(
                std::env::var_os(VARIAVEL),
                antes,
                "um teste que entrou em pânico com `$SEELE_ENCONTRO` trocada deixou a troca \
                 para trás: todo teste seguinte deste binário fala com o ponto de encontro \
                 errado"
            );
        }

        #[test]
        fn o_ambiente_do_encontro_segura_a_trava_enquanto_vive() {
            // A trava tem de durar o guarda inteiro, e não só o `guardar`. Uma
            // trava só deste teste, e não a `TRAVA`: a de outro teste em curso
            // deixaria esta conferência passar sem o guarda segurar nada.
            //
            // E a `TRAVA` por fora, como no teste de pânico: o guarda lê a
            // variável ao nascer e a escreve ao cair. Sem ela, este teste lia a
            // variável trocada pelo de pânico e a escrevia de volta depois de
            // o outro a devolver, e o de pânico reprovava com uma frase falsa.
            static SO_DESTE_TESTE: Mutex<()> = Mutex::new(());
            let _ninguem_mais_troca = TRAVA.lock().unwrap_or_else(PoisonError::into_inner);
            let ambiente = AmbienteDoEncontro::guardar_sob(&SO_DESTE_TESTE);

            let tomada = matches!(
                SO_DESTE_TESTE.try_lock(),
                Err(std::sync::TryLockError::WouldBlock)
            );
            drop(ambiente);

            assert!(
                tomada,
                "a trava estava livre com um `AmbienteDoEncontro` vivo: dois testes trocam \
                 `$SEELE_ENCONTRO` ao mesmo tempo, e um lê o valor que o outro pôs"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::channels::Channels;
    use crate::persistence::{Location, Persistence};

    fn born(config: &ServerConfig) -> Persistence {
        let mut persistence = Persistence::open(&config.database).expect("open");
        seed(&mut persistence, config).expect("seed");
        persistence
    }

    #[test]
    fn a_new_server_opens_with_somewhere_to_go() {
        // A server with na sala de voz offers nowhere to speak and no canal to write in.
        // The person who pressed HOSPEDAR AQUI would look at an empty list with
        // no way to tell a working server from a broken one — and "hospede você
        // mesmo" would be a claim rather than a thing that happens.
        let config = ServerConfig::default();
        let persistence = born(&config);
        let dao = Channels::new(&persistence);

        // `dao` e não `channels`: o local que lista os canais também se chama
        // `channels` depois do rename, e sombrear o DAO com o resultado dele
        // fazia a linha seguinte pedir `voice_rooms()` a um `Vec`.
        let channels = dao.channels().expect("channels");
        assert_eq!(
            channels.len(),
            1,
            "a server opened with no canal: {channels:?}"
        );
        assert_eq!(channels[0].name, "geral");

        let voice_rooms = dao.voice_rooms().expect("voice_rooms");
        assert_eq!(
            voice_rooms.len(),
            1,
            "a server opened with na sala de voz: {voice_rooms:?}"
        );
        assert_eq!(voice_rooms[0].name, config.voice_room_name);
        assert_eq!(voice_rooms[0].limit, config.voice_room_limit);
        assert_eq!(
            voice_rooms[0].channel,
            Some(channels[0].id),
            "the opening voice room has no canal attached to it"
        );
        assert!(
            !voice_rooms[0].password_required,
            "the opening voice room is locked, and nobody has the key"
        );
    }

    #[test]
    fn a_second_boot_does_not_add_a_second_set_of_rooms() {
        // `seed` runs on every boot. Not being idempotent would mean a server
        // that has been restarted twenty times shows twenty `geral`s.
        let directory = tempfile::tempdir().expect("tempdir");
        let config = ServerConfig {
            database: Location::File(directory.path().join("seele.db")),
            ..ServerConfig::default()
        };

        drop(born(&config));
        let persistence = born(&config);

        let channels = Channels::new(&persistence);
        assert_eq!(channels.channels().expect("channels").len(), 1);
        assert_eq!(channels.voice_rooms().expect("voice_rooms").len(), 1);
    }

    #[test]
    fn a_renamed_opening_voice_room_keeps_its_new_name_across_a_restart() {
        // The seed and `RenameVoiceRoom` write the same row, and a seed that
        // overwrote would undo the rename on the next restart — silently, and
        // only for the room the server came with, which is the hardest kind of
        // bug to believe when somebody reports it.
        let directory = tempfile::tempdir().expect("tempdir");
        let config = ServerConfig {
            database: Location::File(directory.path().join("seele.db")),
            ..ServerConfig::default()
        };

        let persistence = born(&config);
        let voice_room = Channels::new(&persistence)
            .voice_rooms()
            .expect("voice_rooms")[0]
            .id;
        Channels::new(&persistence)
            .rename_voice_room(voice_room, "SALA-01 PONTE")
            .expect("rename");
        drop(persistence);

        let persistence = born(&config);
        let voice_rooms = Channels::new(&persistence)
            .voice_rooms()
            .expect("voice_rooms");
        assert_eq!(voice_rooms.len(), 1);
        assert_eq!(voice_rooms[0].name, "SALA-01 PONTE");
    }

    #[test]
    fn a_destroyed_opening_line_does_not_come_back_at_the_next_boot() {
        // The confirmation in front of `apagar_linha` promises that no screen
        // of this product brings the Channel back. A `systemctl restart` is not a
        // screen, and it was bringing this one back: the seed writes `channels`
        // row 1 by hand, `INSERT OR IGNORE` means "unless row 1 is taken", and
        // destroying the Channel is precisely what makes row 1 free again. Same
        // name, same identifier, empty — the product contradicting its own
        // confirmation.
        let directory = tempfile::tempdir().expect("tempdir");
        let config = ServerConfig {
            database: Location::File(directory.path().join("seele.db")),
            ..ServerConfig::default()
        };

        let persistence = born(&config);
        let channels = Channels::new(&persistence);
        let channel = channels.channels().expect("channels")[0].id;
        // The voice room bound to it is unbound by the delete; nothing else here
        // depends on that, and the voice room is not what this test is about.
        channels.delete_channel(channel).expect("delete");
        drop(persistence);

        let persistence = born(&config);
        assert!(
            Channels::new(&persistence)
                .channels()
                .expect("channels")
                .is_empty(),
            "o canal destroyed on purpose was written back by the next boot"
        );
    }

    #[test]
    fn a_destroyed_opening_voice_room_does_not_come_back_at_the_next_boot() {
        // The same for the voice room, which the seed also writes by identifier. It
        // takes a second room to get here at all, because the last voice room is
        // refused — and that is the shape a real server has when somebody
        // destroys the one it came with: they made theirs first.
        let directory = tempfile::tempdir().expect("tempdir");
        let config = ServerConfig {
            database: Location::File(directory.path().join("seele.db")),
            ..ServerConfig::default()
        };

        let persistence = born(&config);
        let channels = Channels::new(&persistence);
        let inicial = channels.voice_rooms().expect("voice_rooms")[0].id;
        channels
            .create_voice_room("SALA-02 PONTE", 8, None)
            .expect("voice room");
        channels.delete_voice_room(inicial).expect("delete");
        drop(persistence);

        let persistence = born(&config);
        let voice_rooms = Channels::new(&persistence)
            .voice_rooms()
            .expect("voice_rooms");
        assert_eq!(
            voice_rooms.len(),
            1,
            "the opening voice room was written back: {voice_rooms:?}"
        );
        assert_eq!(voice_rooms[0].name, "SALA-02 PONTE");
    }

    #[test]
    fn a_server_seeded_by_an_older_build_is_not_seeded_twice() {
        // The upgrade path. A database from before the flag has its rooms and
        // no flag; the first boot on this code has to write the flag and leave
        // the rooms exactly as they are, which is what `INSERT OR IGNORE` was
        // already doing on its own.
        let directory = tempfile::tempdir().expect("tempdir");
        let config = ServerConfig {
            database: Location::File(directory.path().join("seele.db")),
            ..ServerConfig::default()
        };

        let persistence = born(&config);
        // Forget that this database was ever seeded, which is exactly the state
        // an older build leaves behind.
        persistence
            .connection()
            .execute("DELETE FROM configuracao WHERE chave = ?1", [SEMEADO])
            .expect("forget");
        Channels::new(&persistence)
            .rename_channel(
                Channels::new(&persistence).channels().expect("channels")[0].id,
                "avisos",
            )
            .expect("rename");
        drop(persistence);

        let persistence = born(&config);
        let channels = Channels::new(&persistence).channels().expect("channels");
        assert_eq!(channels.len(), 1, "a second `geral` appeared: {channels:?}");
        assert_eq!(channels[0].name, "avisos");
    }
}
