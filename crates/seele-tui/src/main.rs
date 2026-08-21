//! `plug` — the Entry Plug.
//!
//! The terminal client, and the product's primary interface
//! (`specs/05-cliente-tui.md`: "O produto principal. Tudo o mais imita esta
//! interface.").
//!
//! This file is the shell around the shell: terminal setup, the event loop, and
//! the translation from crossterm events into [`seele_tui::app::Key`]. Every
//! decision it makes about *what* to do lives in [`seele_tui::app`]; every
//! decision about the session lives in `seele-core`. If this file grows a
//! judgement of its own, something has leaked.

use std::io::{self, Stdout};
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement, EnterAlternateScreen,
    LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use seele_core::chegada::Chegada;
use seele_core::conhecidos::Conhecidos;
use seele_core::enlace::{Aviso, Destino, Enlace, Fechado, Motivo};
use seele_core::{
    identity, CageId, ClientMessageId, ConnectError, FilePinStore, LineId, Room, SyncInputs,
    SyncRatio, Verdict, Voice, VoiceMode,
};
use seele_tui::app::{Action, Alert, App, ChatLine, Key, Mode, Node, Screen};
use seele_tui::command::{self, Command};
use seele_tui::selecao::{self, Resultado, Selecao};
use seele_tui::theme::{Palette, Theme};
use seele_tui::{ui, view};

/// The redraw floor. `specs/05-cliente-tui.md` asks for ~30 fps *when something
/// changed*, so this is a ceiling on how often a change is allowed to cost a
/// frame, not a blind loop.
const FRAME: Duration = Duration::from_millis(33);

/// How often the clock and the audio telemetry are re-read.
const TICK: Duration = Duration::from_millis(250);

struct Args {
    server: SocketAddr,
    server_name: String,
    pin_key: String,
    /// The invite's other addresses for the same Dogma, in try order.
    ///
    /// Text, not resolved addresses: an alternative that does not resolve is
    /// dropped when the attempt list is built, and dropping one is not a reason
    /// to refuse a link whose first address is fine.
    alternativos: Vec<String>,
    nickname: String,
    cage: CageId,
    line: LineId,
    no_audio: bool,
    /// Convite ou senha, quando o Dogma pede.
    join_secret: Option<String>,
    /// Impressão digital esperada, quando veio num link.
    expected_fingerprint: Option<String>,
    /// O bilhete de encontro do link, quando ele trouxe um.
    ///
    /// Degrau 4 do ADR 0022. Texto do `enc`, já lido: com ele, este cliente bate
    /// no ponto de encontro antes de tentar endereço nenhum, e o anfitrião fura o
    /// NAT para cá.
    bilhete: Option<seele_core::uri::Bilhete>,
    /// Subir um Dogma neste processo e conectar nele.
    hospedar: bool,
}

impl Args {
    /// Monta os argumentos a partir do que a tela de conexão devolveu.
    ///
    /// Mesmo caminho de quando vêm da linha de comando: o que a tela produz não
    /// é um atalho para dentro, é o mesmo `Args` que o `--server` produziria.
    fn da_escolha(escolha: &seele_tui::selecao::Escolha) -> Result<Self> {
        let (server, server_name, pin_key) = resolve(&escolha.alvo)?;
        Ok(Self {
            server,
            server_name,
            pin_key,
            // A known Dogma is one address, the one it was reached at. The
            // list is a property of an invite, not of a place already visited.
            alternativos: Vec::new(),
            nickname: escolha.apelido.clone(),
            cage: CageId(escolha.cage),
            line: LineId(1),
            no_audio: false,
            join_secret: escolha.convite.clone(),
            expected_fingerprint: escolha.impressao_digital.clone(),
            // Um Dogma conhecido é um endereço a que já se chegou, e o bilhete é
            // propriedade de um convite — ele envelhece com o mapeamento de NAT
            // que o produziu. Quem volta a um lugar volta pelo endereço.
            bilhete: None,
            hospedar: escolha.hospedar,
        })
    }
}

/// Lê a linha de comando, ou avisa que não veio nada.
///
/// `None` significa `plug` puro, sem flag nenhuma: aí a tela de conexão decide.
/// Qualquer argumento pula a tela — quem digitou aonde vai já disse aonde vai.
fn parse_args() -> Result<Option<Args>> {
    if std::env::args().len() <= 1 {
        return Ok(None);
    }

    let mut target = "127.0.0.1:8383".to_owned();
    let mut nickname = "piloto".to_owned();
    let mut cage = CageId(1);
    let mut line = LineId(1);
    let mut no_audio = false;
    let mut join_secret: Option<String> = None;
    let mut expected_fingerprint: Option<String> = None;
    let mut bilhete: Option<seele_core::uri::Bilhete> = None;
    let mut hospedar = false;
    let mut alternativos: Vec<String> = Vec::new();
    let mut argv = std::env::args().skip(1);

    while let Some(flag) = argv.next() {
        match flag.as_str() {
            "--server" | "-s" => target = argv.next().context("--server needs an address")?,
            // Um link colado de uma conversa. Traz endereço, e pode trazer a
            // impressão digital e um convite de uso único.
            "--url" | "-u" => {
                let texto = argv.next().context("--url needs a seele:// link")?;
                let convite = seele_core::uri::analisar(&texto)
                    .map_err(|erro| anyhow!("link inválido: {erro}"))?;
                alternativos = convite.alternativos;
                target = convite.alvo;
                expected_fingerprint = convite.impressao_digital;
                bilhete = convite.bilhete;
                join_secret = convite.token;
                if let Some(numero) = convite.cage {
                    cage = CageId(numero);
                }
            }
            "--convite" | "--senha" => {
                join_secret = Some(argv.next().context("--convite needs a value")?);
            }
            "--nick" | "-n" => nickname = argv.next().context("--nick needs a name")?,
            "--cage" => {
                cage = CageId(argv.next().context("--cage needs a number")?.parse()?);
            }
            "--linha" | "--line" => {
                line = LineId(argv.next().context("--linha needs a number")?.parse()?);
            }
            // A terminal on a headless box has no sound card, and the text half
            // of the product needs none.
            "--sem-audio" | "--no-audio" => no_audio = true,
            // Sobe o Dogma aqui dentro e entra nele. Um comando, um processo.
            "--hospedar" | "--host" => hospedar = true,
            "--ajuda" | "--help" | "-h" => {
                usage();
                std::process::exit(0);
            }
            other => return Err(anyhow!("opção desconhecida: {other}")),
        }
    }

    let (server, server_name, pin_key) = resolve(&target)?;
    Ok(Some(Args {
        server,
        server_name,
        pin_key,
        alternativos,
        nickname,
        cage,
        line,
        no_audio,
        join_secret,
        expected_fingerprint,
        bilhete,
        hospedar,
    }))
}

/// Resolves `host:port` into an address, a TLS label, and a pin key.
///
/// Three values because they are three things. The address is where the packets
/// go. The TLS label is what rustls is handed — it never gets checked, since
/// TOFU compares fingerprints. The **pin key** is the policy: what this server's
/// fingerprint is filed under, and it must be the target as typed.
///
/// Keying the pin by the TLS label instead was a real bug. Both shells mapped
/// every IP address to `localhost`, so two Dogmas on a LAN shared one entry and
/// the second one contacted looked like the first one's key had changed.
/// Até onde o link chega, dito em uma ou duas frases.
///
/// Um link que só funciona na rede de casa e um link que funciona pela internet
/// são o **mesmo texto**, e é por isso que isto existe: sem a frase o anfitrião
/// manda o primeiro achando que mandou o segundo, e a descoberta acontece do
/// outro lado, como "não conecta". O ADR 0022 pede que isto seja dito.
///
/// Nenhuma delas promete alcance: mesmo no melhor caso o firewall do outro lado
/// pode recusar. "Deve funcionar" é a promessa honesta.
fn frase_do_alcance(alcance: &seele_server::alcance::Alcance) -> String {
    use seele_server::alcance::Degrau;

    // Os dois motivos, quando houve dois. O degrau 4 tentado e falhado é
    // informação de quem hospeda: "o roteador não abriu **e** o ponto de
    // encontro não respondeu" explica um link limitado melhor que qualquer uma
    // das duas metades sozinha.
    let motivo: String = [alcance.porta_recusada(), alcance.encontro_recusado()]
        .into_iter()
        .flatten()
        .map(|motivo| format!("\n{motivo}."))
        .collect();

    match alcance.degrau() {
        Degrau::PortaNoRoteador => {
            "O roteador abriu a porta: este link deve funcionar pela internet.".to_owned()
        }
        // Degrau 4. "Deve funcionar" e não "funciona", e a diferença é a razão
        // de a escada continuar existindo: com NAT simétrico dos dois lados o
        // furo não abre, e a saída continua sendo encaminhar a porta à mão.
        Degrau::FuroDeNat => format!(
            "Um ponto de encontro apresentou esta máquina: o link deve \
             funcionar pela internet sem mexer no roteador.{motivo}\n\
             Se não funcionar, as duas redes são do tipo que não deixa furar — \
             aí encaminhe a porta {} no roteador à mão.\n\
             O ponto de encontro fica sabendo que endereço falou com o seu, e \
             quando; nunca o que foi dito. Para usar o seu, ou nenhum: \
             docs/ponto-de-encontro.md.",
            alcance.alvo().port()
        ),
        Degrau::Ipv6Direto => format!(
            "Este link é IPv6 e alcança de qualquer lugar — mas só quem também \
             tiver IPv6.{motivo}"
        ),
        // Degrau 1 com endereço próprio: uma VPS, um IP fixo, uma porta já
        // encaminhada à mão. O único degrau em que nada foi pedido a ninguém —
        // nem ao roteador, nem a um ponto de encontro —, e por isso o único sem
        // ressalva. Antes de ele existir esta máquina lia a frase do
        // `SoRedeLocal`, que manda encaminhar a porta num roteador que não
        // existe, embaixo de um link que alcança o mundo inteiro.
        Degrau::EnderecoDireto => format!(
            "Esta máquina tem endereço próprio: este link deve funcionar pela \
             internet, sem depender de ninguém.{motivo}"
        ),
        // O caso do relato de campo: a VPN é o único caminho que sai desta
        // máquina, e ela não aceita entrada. Frase própria porque o que se faz
        // a respeito é diferente — aqui a resposta é desligar a VPN.
        Degrau::RedeLocalOuVpn => format!(
            "ATENÇÃO: este link só funciona na sua rede, ou para quem estiver \
             na mesma VPN que você.{motivo}\n\
             O único endereço que sai desta máquina vem de uma VPN, e VPN de \
             navegação (WARP, Proton, Nord) não aceita conexão de entrada. \
             Para alcançar de fora: desligue a VPN e encaminhe a porta {} no \
             roteador à mão.",
            alcance.alvo().port()
        ),
        Degrau::SoRedeLocal => format!(
            "ATENÇÃO: este link só funciona para quem estiver na sua rede.{motivo}\n\
             Para alcançar de fora: encaminhe a porta {} no roteador à mão, ou \
             use uma VPN de rede (Tailscale, WireGuard) nos dois lados.",
            alcance.alvo().port()
        ),
    }
}

/// The split is `seele_core::uri::separar` and not `rsplit_once(':')`: the port
/// separator and an IPv6's own separator are the same character, and doing it by
/// hand here made `[2001:db8::1]:8383` resolve to nothing. ADR 0022, step 2.
fn resolve(target: &str) -> Result<(SocketAddr, String, String)> {
    let alvo = seele_core::uri::separar(target).map_err(|erro| match erro {
        seele_core::uri::ErroDeUri::EnderecoIpv6SemColchetes => anyhow!(
            "{target} é um IPv6 e precisa de colchetes: escreva [{target}] \
             (o `:` que separa a porta é o mesmo que separa o endereço)"
        ),
        outro => anyhow!("não consegui entender {target}: {outro}"),
    })?;
    let address = (alvo.maquina, alvo.porta)
        .to_socket_addrs()
        .with_context(|| format!("não consegui resolver {target}"))?
        .next()
        .ok_or_else(|| anyhow!("{target} não resolveu para nenhum endereço"))?;

    // An IP is not a name the M2 server's certificate carries, so TLS is handed
    // the name it does carry. The pin, meanwhile, is filed under the address —
    // which is the thing that actually distinguishes one server from another.
    let server_name = if alvo.maquina.parse::<std::net::IpAddr>().is_ok() {
        "localhost".to_owned()
    } else {
        alvo.maquina.to_owned()
    };
    // Canonical rather than as typed, so `[::1]` and `[::1]:8383` file the same
    // pin instead of two. The brackets go back on: without them the key would be
    // ambiguous with `host:port`.
    let pin_key = if address.is_ipv6() && alvo.maquina.contains(':') {
        format!("[{}]:{}", alvo.maquina, alvo.porta)
    } else {
        format!("{}:{}", alvo.maquina, alvo.porta)
    };
    Ok((address, server_name, pin_key))
}

/// Where this client keeps its identity and its pins.
///
/// `$SEELE_HOME` first, so a second client on one machine can be a second pilot
/// — which is exactly what testing two ends of a conversation needs.
fn config_dir() -> PathBuf {
    if let Ok(home) = std::env::var("SEELE_HOME") {
        return PathBuf::from(home);
    }
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("seele");
    }
    std::env::var("HOME").map_or_else(
        |_| PathBuf::from(".seele"),
        |home| PathBuf::from(home).join(".config").join("seele"),
    )
}

/// Roda a tela de conexão até alguém escolher ou desistir.
///
/// Laço próprio, e de propósito: aqui ainda não existe conexão nem áudio, então
/// não há nada para o runtime fazer enquanto ninguém digita. O laço do `sessao`
/// existe porque lá **há** — pacotes chegando enquanto se digita — e copiá-lo
/// para cá seria complexidade sem motivo.
///
/// As teclas vêm pelo mesmo canal da sessão, e não de um
/// `crossterm::event::read` próprio: dois leitores na mesma entrada disputam
/// cada tecla, e ao voltar de um `:ejetar` o perdedor da disputa seria esta
/// tela.
///
/// `Ok(None)` é ter desistido, e sair calado é a resposta certa: quem apertou
/// `q` não precisa de mensagem de erro.
async fn escolher(
    terminal: &mut Screen1,
    key_rx: &mut tokio::sync::mpsc::Receiver<Event>,
    tema: Theme,
    home: &std::path::Path,
) -> Result<Option<Args>> {
    let mut lista = Conhecidos::abrir(home.join("conhecidos"))?;
    let mut tela = Selecao::nova(lista.listar());

    loop {
        terminal.draw(|frame| selecao::desenhar(frame, &tela, tema))?;

        // Sem leitor não há como escolher nada, e insistir num canal fechado
        // seria um laço quente para sempre.
        let Some(evento) = key_rx.recv().await else {
            return Ok(None);
        };
        let Some(tecla) = tecla_apertada(&evento) else {
            continue;
        };
        if tecla.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(tecla.code, KeyCode::Char('c'))
        {
            return Ok(None);
        }

        match tela.tecla(tecla.code) {
            Resultado::Segue => {}
            Resultado::Sai => return Ok(None),
            // A tela não escreve em disco. Ela diz o que aconteceu e quem tem o
            // arquivo grava — a mesma separação do resto do cliente.
            Resultado::Esquecer(alvo) => lista.esquecer(&alvo)?,
            Resultado::Pronto(escolha) => return Args::da_escolha(&escolha).map(Some),
        }
    }
}

fn usage() {
    eprintln!("plug — Entry Plug, o cliente SEELE");
    eprintln!();
    eprintln!("  sem argumento nenhum, abre a tela de conexão: os Dogmas onde");
    eprintln!("  você já esteve, um endereço novo, um convite, ou hospedar aqui.");
    eprintln!();
    eprintln!("  -s, --server <host[:porta]>  Dogma ao qual se conectar (padrão 127.0.0.1:8383)");
    eprintln!("  -n, --nick <nome>            como aparecer no roster");
    eprintln!("      --cage <n>               Cage a entrar (padrão 1)");
    eprintln!("      --linha <n>              Linha a abrir (padrão 1)");
    eprintln!("      --sem-audio              só texto, sem placa de som");
    eprintln!(
        "  -u, --url <seele://…>        link de convite: endereço, impressão digital e convite"
    );
    eprintln!("      --convite <token>        convite de uso único, ou a senha do Dogma");
    eprintln!("      --hospedar               sobe um Dogma aqui e entra nele");
    eprintln!("  -h, --ajuda                  isto");
    eprintln!();
    eprintln!("  $SEELE_HOME  onde ficam a identidade e os pins (padrão ~/.config/seele)");
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = match parse_args() {
        Ok(args) => args,
        Err(error) => {
            eprintln!("{error}");
            usage();
            std::process::exit(2);
        }
    };

    // Asked once. `supports_keyboard_enhancement` writes a query and waits for
    // the terminal to answer, so calling it per use costs a stall per use — and
    // on a terminal that never answers, a stall of the full timeout each time.
    let holds = supports_keyboard_enhancement().unwrap_or(false);

    let mut terminal = enter_terminal(holds)?;
    let result = run(&mut terminal, args, holds).await;
    leave_terminal(&mut terminal, holds)?;

    // Printed after the terminal is restored, or it lands in the alternate
    // screen and disappears with it — which is how a crash becomes "it just
    // closed".
    if let Err(error) = &result {
        eprintln!("plug encerrou: {error:#}");
    }
    result
}

type Screen1 = Terminal<CrosstermBackend<Stdout>>;

fn enter_terminal(holds: bool) -> Result<Screen1> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen, EnableMouseCapture)?;

    // Key *release* events are what make hold-to-talk hold. Only terminals that
    // speak the Kitty keyboard protocol report them; the rest get a latch
    // instead, decided in `key_source`.
    if holds {
        let _ = execute!(
            out,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::REPORT_EVENT_TYPES)
        );
    }

    Ok(Terminal::new(CrosstermBackend::new(out))?)
}

fn leave_terminal(terminal: &mut Screen1, holds: bool) -> Result<()> {
    if holds {
        let _ = execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags);
    }
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    disable_raw_mode()?;
    terminal.show_cursor()?;
    Ok(())
}

/// Everything the loop needs to hold between keystrokes.
struct Runtime<'a> {
    app: App,
    /// What is true about the session. Owned by `seele-core`, not by this file.
    room: Room,
    /// Borrowed from [`run`], and borrowed rather than copied on purpose.
    ///
    /// `:tema` is an accessibility accommodation in a client whose spec forbids
    /// information carried by colour: somebody whose terminal renders the
    /// 256-colour palette illegibly types `:tema mono` because it is the only
    /// way to read the screen. A copy per session put them back on the detected
    /// palette at every eject, forever.
    theme: &'a mut Theme,
    voice: Option<Voice>,
    sync: SyncRatio,
    /// True when the terminal reports key releases, so the space bar can be
    /// held rather than latched.
    holds: bool,
    /// The latch, for terminals that cannot.
    latched: bool,
    next_message_id: u64,
}

/// The first idempotency key of a session, drawn rather than counted from one.
///
/// `Messages::append_batch` deduplicates on `(author_id, client_message_id)`.
/// The two halves used to have different lifetimes: `author_id` comes from the
/// Ed25519 key on disk (ADR 0004) and never changes, while this counter started
/// at 1 on every session. So after reconnecting, message 1 of the new session
/// was read as a retry of message 1 of the old one and **was never written** —
/// a pilot who dropped their wi-fi and came back could not speak, and neither
/// end was told (pendency 19).
///
/// The high half is random and the low half counts, which leaves four billion
/// messages in a session before the two could ever meet.
fn primeira_chave_de_mensagem() -> u64 {
    (u64::from(rand::random::<u32>()) << 32) | 1
}

/// The outer loop: choose, talk, eject, choose again.
///
/// The whole session lives inside one turn of this loop, and `Enlace` and
/// `Voice` are dropped at the end of it. This is **not** what issue #9
/// turned down — that was swapping the connection out from under a live
/// session; this is tearing everything down and going back to a screen that
/// has no roster, no telemetry, and no audio.
async fn run(terminal: &mut Screen1, args: Option<Args>, holds: bool) -> Result<()> {
    let home = config_dir();
    // Detectado uma vez, para o programa inteiro. Quem escolher outra paleta com
    // `:tema` continua nela ao trocar de Dogma: redetectar a cada sessão
    // devolveria a pessoa à paleta que ela acabou de recusar.
    let mut tema = Theme::detect();

    // Um leitor de teclado só, para o programa inteiro, e não um por sessão.
    // `crossterm::event::read` segura a entrada enquanto espera: dois leitores
    // vivos ao mesmo tempo roubam tecla um do outro, e ao ejetar era exatamente
    // isso que acontecia — o leitor da sessão que acabou engolia a primeira
    // tecla da tela de seleção.
    let (key_tx, mut key_rx) = tokio::sync::mpsc::channel::<Event>(64);
    std::thread::spawn(move || read_terminal_events(&key_tx));

    // Com uma flag, a tela de seleção não aparece na entrada — quem digitou
    // `--server` já disse aonde ia. Ao ejetar ela aparece, e é o certo: ejetar
    // é o pedido explícito de ir a outro lugar.
    let mut escolhidos = args;

    loop {
        let args = match escolhidos.take() {
            Some(args) => args,
            None => match escolher(terminal, &mut key_rx, tema, &home).await? {
                Some(args) => args,
                None => return Ok(()),
            },
        };

        if !sessao(terminal, &mut key_rx, &args, holds, &home, &mut tema).await? {
            return Ok(());
        }
    }
}

/// One session, from the first packet to the last.
///
/// Returns `true` to send [`run`]'s loop back to the selection screen, and
/// `false` only to leave the program. Every way a session can end returns
/// `true` — ejecting, a Dogma that never answered, an invite that pointed at
/// another Dogma, an internal battery that ran out — because none of those is
/// a request to close the client. Quitting is `:q`, `:quit`, `:sair` and
/// Ctrl-C, and nothing else.
#[allow(
    clippy::too_many_lines,
    reason = "one event loop; the branches are the interface's whole surface"
)]
async fn sessao(
    terminal: &mut Screen1,
    key_rx: &mut tokio::sync::mpsc::Receiver<Event>,
    args: &Args,
    holds: bool,
    home: &std::path::Path,
    tema: &mut Theme,
) -> Result<bool> {
    let mut runtime = Runtime {
        app: App::new(),
        room: Room::new(),
        theme: tema,
        voice: None,
        sync: SyncRatio::new(),
        holds,
        latched: false,
        next_message_id: primeira_chave_de_mensagem(),
    };

    // The boot screen goes up before the connection is attempted and comes down
    // when it lands. specs/05-cliente-tui.md: it lasts the real time of the
    // connection, and nothing is added to make it look busier.
    runtime.app.screen = Screen::Boot;
    terminal.draw(|frame| ui::render(frame, &runtime.app, *runtime.theme))?;

    // ADR 0004, and the reason it has to be on disk: CASPER binds a nickname to
    // the identity that first claimed it, so a client that generates a fresh key
    // each run can never come back under its own name.
    let key = identity::load_or_create(&home.join("identity.key"))?;
    // ADR 0003 is trust on *first* use, which needs the first use remembered.
    let pins = Arc::new(FilePinStore::open(home.join("pins"))?);

    // `--hospedar`: o Dogma sobe aqui dentro, antes de conectar. Fica vivo pelo
    // tempo desta função — quando o cliente sai, o Dogma vai junto, que é o
    // comportamento certo para "estou hospedando uma conversa".
    // `mut` e `take()` porque ejetar tem de **esperar** a porta voltar antes de
    // devolver o controle à tela de seleção; largar o handle no fim da função
    // não espera nada, e a próxima volta do laço acharia a porta ocupada.
    let mut hospedagem = if args.hospedar {
        let dogma = seele_server::hospedagem::Hospedagem::iniciar(
            args.server.port(),
            seele_server::casper::Location::File(home.join("dogma.db")),
            "Casa",
        )
        .await
        .context("não consegui subir o Dogma aqui")?;
        Some(dogma)
    } else {
        None
    };

    // `Enlace` e não `Client`: um `Client` é uma conexão e acaba quando ela
    // cai; o enlace é a **sessão**, que atravessa quedas. É ele que carrega a
    // bateria interna e a reconexão de `specs/07-tema-evangelion.md`.
    let destinos = construir_destinos(args);
    // Uma `Chegada`, que é quem dá nome a cada etapa desta travessia. Com
    // bilhete ela avisa o ponto de encontro pelos candidatos que precisam de
    // furo — degrau 4 do ADR 0022; sem bilhete, é o caminho de sempre.
    let chegada = Chegada::nova(destinos, args.bilhete.clone());
    let mut client = match chegada.chegar(key, pins).await {
        Ok(client) => client,
        Err(falha) => {
            // A trilha morre aqui, e de propósito. Este terminal está no ecrã
            // alternado quando isto acontece: escrever nele os passos da chegada
            // rabiscaria a tela de fim por cima, e escrevê-los fora dele seria
            // escrever onde ninguém está olhando. Quem pergunta «qual dos quatro
            // endereços deu o quê» num terminal pergunta ao `plug --rede`, que
            // existe para responder isso com o resto do diagnóstico junto.
            runtime.app.screen = Screen::Lost {
                reason: motivo_de_conexao_perdida(falha.motivo()),
            };
            // Um endereço que não atende é o caso mais comum de todos, e
            // justamente o que mais precisa da lista: escolher um Dogma morto
            // dos conhecidos jogava a pessoa para fora do cliente, longe da
            // lista de onde ela acabou de escolher.
            return fim_de_sessao(terminal, key_rx, &mut runtime, &mut hospedagem).await;
        }
    };

    // O core já decidiu o que este aperto de mão significa — `Enlace::veredito`
    // vem pronto, e esta casca só desenha o que ele diz. `Verdict::InviteRefused`
    // não aparece aqui: a recusa derruba a conexão antes de existir um `Enlace`
    // para perguntar, e o `Err` acima já ganhou frase em
    // [`motivo_de_conexao_perdida`].
    runtime.app.alert = alerta_do_veredito(client.veredito());

    let cage = args.cage;
    // Um enlace que fecha antes mesmo de a sessão subir é uma sessão que
    // acabou, e não um defeito do programa. Ver [`enlace_fechado`].
    if entrar(&client, cage, args.line).await.is_err() {
        enlace_fechado(&mut runtime);
        drop(client);
        return fim_de_sessao(terminal, key_rx, &mut runtime, &mut hospedagem).await;
    }

    // Registrado só **depois** de dar certo. Guardar antes encheria a lista de
    // endereços errados digitados uma vez, que é o oposto de uma lista de
    // atalhos. Um Dogma hospedado aqui não entra: `127.0.0.1` não é lugar aonde
    // se volta, é o botão "hospedar".
    if !args.hospedar {
        if let Ok(mut lista) = Conhecidos::abrir(home.join("conhecidos")) {
            // Falhar em gravar um atalho não pode derrubar uma conversa que já
            // está de pé.
            if let Err(erro) = lista.registrar(&args.pin_key, &args.nickname, Some(cage.0)) {
                note(
                    &mut runtime,
                    format!("não guardei este Dogma na lista: {erro}"),
                );
            }
        }
    }
    // O anfitrião precisa saber o que mandar para os amigos. Sem isto ele
    // hospeda e não tem como convidar ninguém, que é hospedar para nada.
    if let Some(dogma) = &hospedagem {
        let convite = dogma.convite();
        note(
            &mut runtime,
            "hospedando. `:convite` mostra o link.".to_owned(),
        );
        runtime.app.alcance = dogma.alcance().map(frase_do_alcance);
        runtime.app.convite = Some(convite);
        // Aberta de saída: a primeira coisa que o anfitrião precisa é o link.
        runtime.app.convite_visivel = true;
        note(
            &mut runtime,
            "o Dogma acaba quando você sair. `:convite` mostra o link de novo.".to_owned(),
        );
    }

    runtime.room.adopt(client.sessao(), &args.nickname);
    runtime.room.enter_cage(cage);
    runtime.room.open_line(args.line);

    if !args.no_audio {
        match Voice::start(client.media(), client.sessao().ssrc) {
            Ok(voice) => {
                voice.set_mode(VoiceMode::PushToTalk);
                runtime.voice = Some(voice);
            }
            Err(error) => {
                // No microphone is not a reason to have no client. The text half
                // works, and saying so beats exiting with a device error.
                runtime.app.alert = Some(Alert {
                    text: format!("SEM ÁUDIO: {error}"),
                    blocking: false,
                });
            }
        }
    }

    runtime.app.screen = Screen::PatternBlue;
    view::project(&runtime.room, &mut runtime.app);
    runtime.app.refazer_busca();

    let mut next_tick = Instant::now();
    let mut last_draw = Instant::now() - FRAME;
    let mut dirty = true;

    loop {
        if dirty && last_draw.elapsed() >= FRAME {
            terminal.draw(|frame| ui::render(frame, &runtime.app, *runtime.theme))?;
            last_draw = Instant::now();
            dirty = false;
        }
        if runtime.app.quit {
            return Ok(false);
        }
        if runtime.app.ejetou {
            // Primeiro o que está ligado, depois a porta: é a ordem do
            // `disconnect` do app.
            //
            // E soltar aqui não fecha nada de imediato: `Drop for Enlace` é um
            // `abort`, que é assíncrono, e `Drop for Voice` só levanta a
            // bandeira de parada de uma thread de áudio que ainda segura uma
            // cópia da conexão. Soltar cedo torna o fechamento provável e
            // imediato; quem o torna **suficiente** é o `encerrar`, que fecha
            // o endpoint do Dogma antes de esperar e ainda dá uma folga medida
            // no fim. Quem for afirmar em teste que a sessão fechou tem de
            // afirmar sobre o `encerrar`, e não sobre estes dois `drop`.
            drop(client);
            desligar(&mut runtime, &mut hospedagem).await;
            return Ok(true);
        }

        tokio::select! {
            event = key_rx.recv() => {
                let Some(event) = event else { return Ok(false) };
                dirty = true;
                match event {
                    Event::Key(key_event) => {
                        for key in key_source(&mut runtime, key_event) {
                            if let Some(action) = runtime.app.on_key(key) {
                                // Um `?` aqui matava o cliente: mandar uma
                                // mensagem por um enlace que acabou de fechar
                                // virava `plug encerrou: …` numa linha de
                                // stderr. Ver [`enlace_fechado`].
                                if act(&mut runtime, &client, action, args.line).await.is_err() {
                                    enlace_fechado(&mut runtime);
                                    drop(client);
                                    return fim_de_sessao(
                                        terminal,
                                        key_rx,
                                        &mut runtime,
                                        &mut hospedagem,
                                    )
                                    .await;
                                }
                            }
                        }
                    }
                    // A resize is the one event that must redraw even though
                    // nothing in the model changed.
                    Event::Resize(..) => {}
                    _ => dirty = false,
                }
            }

            aviso = client.proximo() => {
                match aviso {
                    Aviso::Mensagem(message) => {
                        // The core folds it in and says what moved; redrawing
                        // for a round-trip measurement nobody asked to see is
                        // thirty frames a second of nothing.
                        let mudou = runtime.room.apply(&message);
                        if mudou.any() {
                            view::project(&runtime.room, &mut runtime.app);
                            runtime.app.refazer_busca();
                            dirty = true;
                        }
                        // O Dogma avisou que está desconectando. O motivo é
                        // deste momento e de mais nenhum: quando a conexão
                        // cair de fato, o `Aviso::Estado` escreve a bateria
                        // interna por cima, e cinco minutos depois a pessoa
                        // saberia que a sessão acabou sem saber que foi
                        // expulsa, barrada ou que o Dogma estava lotado.
                        //
                        // `view::project` acabou de pôr na tela o texto que o
                        // `text::disconnect` dá para este `DisconnectReason`,
                        // então é só mostrar e sair.
                        if mudou.ended {
                            // Soltar o enlace aqui responde ao
                            // `view::worth_retrying`: sem enlace não há
                            // contagem de reconexão para começar, nem para um
                            // motivo que valeria a pena tentar de novo. Quem
                            // quiser tentar volta pela lista, que é o que a
                            // tela de seleção é.
                            drop(client);
                            return fim_de_sessao(
                                terminal,
                                key_rx,
                                &mut runtime,
                                &mut hospedagem,
                            )
                            .await;
                        }
                    }
                    // O terminal não manda arquivo — não há seletor de
                    // arquivos numa tela de texto, e inventar um caminho
                    // digitado à mão seria outra tela. Ele **desenha** o que
                    // chega: `view::project` põe o anexo na linha da mensagem,
                    // com o nome, o tamanho e a palavra que diz se o arquivo
                    // ainda está lá. Então não há transferência própria para
                    // acompanhar, e este ramo existe para dizer isso em vez de
                    // deixar um `_ =>` engolir uma variante nova amanhã.
                    Aviso::Transferencia(_) => {}
                    // A queda não encerra nada. `specs/07-tema-evangelion.md`:
                    // a interface esmaece e continua legível, a contagem desce,
                    // e o histórico fica ali para leitura.
                    Aviso::Estado { estado, restante } => {
                        view::project_link(
                            estado,
                            restante.map_or(0, |quanto| quanto.as_secs()),
                            &mut runtime.app,
                        );
                        dirty = true;
                    }
                    // Conexão nova, `ssrc` novo, canal de mídia novo: a voz
                    // precisa recomeçar por cima do canal certo, ou o áudio
                    // sairia por uma conexão que já morreu.
                    Aviso::Reconectado { media, sessao } => {
                        // `reopen` e não `start`: os controles têm que atravessar
                        // a reabertura. A lista está no `voice.rs` e mora lá
                        // justamente para que nenhuma casca esqueça um item — o
                        // A.T. Field é o que dói, porque `Enlace` restaura o
                        // silêncio no servidor e o roster continua mostrando
                        // esta pessoa muda enquanto o portão local volta aberto.
                        // Esta casca esquecia todos, e a gráfica também.
                        //
                        // `reopen` e não uma troca de um lado só: pede de volta
                        // os dois dispositivos que esta voz pediu, em vez do
                        // padrão da máquina. Hoje o terminal não escolhe nenhum
                        // dos dois, mas escrever o padrão aqui é deixar pronta
                        // uma reconexão que desfaz uma escolha assim que ela
                        // existir.
                        if let Some(atual) = runtime.voice.as_ref() {
                            runtime.voice = atual.reopen(*media, sessao.ssrc).ok();
                        }
                        runtime.room.adopt(&sessao, &args.nickname);
                        view::project(&runtime.room, &mut runtime.app);
                        // `note` below refreshes the search on its own, so no
                        // explicit `refazer_busca` belongs between this and it.
                        note(&mut runtime, "enlace restabelecido.".to_owned());
                        dirty = true;
                    }
                    Aviso::Encerrado(motivo) => {
                        runtime.app.screen = Screen::Lost {
                            reason: match motivo {
                                Motivo::Descarregou => {
                                    "BATERIA INTERNA ESGOTADA — a sessão não voltou em cinco minutos"
                                        .to_owned()
                                }
                                Motivo::Recusado(detalhe) => format!("O DOGMA RECUSOU: {detalhe}"),
                                Motivo::Pedido => "ENLACE ENCERRADO".to_owned(),
                            },
                        };
                        // O enlace já desistiu por conta própria, então não há
                        // conexão a soltar antes de esperar a porta — o que
                        // sobrou de pé é a voz e, se for o caso, o Dogma daqui.
                        return fim_de_sessao(terminal, key_rx, &mut runtime, &mut hospedagem).await;
                    }
                }
            }

            () = tokio::time::sleep_until(next_tick.into()) => {
                next_tick = Instant::now() + TICK;
                tick(&mut runtime, &client);
                dirty = true;
            }
        }
    }
}

/// Monta os `Destino` a partir do que a linha de comando ou a tela de conexão
/// já resolveram, na ordem em que se tenta.
///
/// Função à parte, e pura, para que o fio de `args.expected_fingerprint` até
/// `Destino::impressao_esperada` seja conferível sem Dogma do outro lado — o
/// mesmo motivo que fez `seele_core::enlace::conferir` sair de dentro de
/// `Enlace::conectar`.
///
/// Um convite pode trazer vários endereços do mesmo Dogma (ADR 0006), e todos
/// carregam a mesma credencial e a mesma impressão digital esperada: é o mesmo
/// Dogma, por caminhos diferentes. O que muda de um para o outro é a chave do
/// pin, que é o endereço — dois Dogmas distintos no mesmo endereço continuam
/// sendo dois arquivos diferentes de confiança, como sempre foram.
///
/// Um alternativo que não resolve é **descartado**, e não vira erro: o convite
/// pode ter sido gerado numa máquina com um IPv6 que a nossa não sabe alcançar,
/// e recusar o link inteiro por causa disso seria trocar um caminho a menos por
/// nenhum caminho.
fn construir_destinos(args: &Args) -> Vec<Destino> {
    let mut destinos = vec![Destino {
        servidor: args.server,
        nome_tls: args.server_name.clone(),
        chave_do_pin: args.pin_key.clone(),
        apelido: args.nickname.clone(),
        segredo: args.join_secret.clone(),
        impressao_esperada: args.expected_fingerprint.clone(),
    }];
    for alternativo in &args.alternativos {
        // Um endereço que não resolve some da lista, e some calado: esta casca
        // é um terminal em modo alternativo, e escrever nele por fora do
        // desenho suja a tela. O efeito que importa é o endereço não ser
        // tentado.
        if let Ok((servidor, nome_tls, chave_do_pin)) = resolve(alternativo) {
            destinos.push(Destino {
                servidor,
                nome_tls,
                chave_do_pin,
                apelido: args.nickname.clone(),
                segredo: args.join_secret.clone(),
                impressao_esperada: args.expected_fingerprint.clone(),
            });
        }
    }
    destinos
}

/// Os três pedidos que põem uma sessão de pé.
///
/// Juntos porque falham juntos e pelo mesmo motivo: se o enlace fechou, nenhum
/// dos três vai a lugar nenhum, e o que se faz com isso é um só.
async fn entrar(client: &Enlace, cage: CageId, line: LineId) -> Result<(), Fechado> {
    client.inserir_plug(cage).await?;
    client.abrir_linha(line).await?;
    client.historico(line, 50).await
}

/// O texto da tela de fim quando `Enlace::conectar` falha.
///
/// Função à parte, e pura, porque um dos erros merece frase própria: um
/// convite que não confere carrega duas impressões de 64 caracteres, e são
/// elas que a pessoa tem que comparar. `\n\n` e `\n` separam as duas em linhas
/// próprias — `render_lost` preserva quebra de linha — porque reflui-las no
/// mesmo parágrafo é a única forma em que ninguém consegue compará-las.
fn motivo_de_conexao_perdida(error: &ConnectError) -> String {
    match error {
        ConnectError::InviteMismatch { expected, offered } => {
            format!("ESTE NÃO É O DOGMA DO CONVITE.\n\nesperada:  {expected}\nofertada:  {offered}")
        }
        _ => format!("NÃO FOI POSSÍVEL ALCANÇAR O DOGMA: {error}"),
    }
}

/// O alerta a mostrar a partir do veredito de identidade — pura, e por isso
/// testável sem Dogma do outro lado.
///
/// `Verdict::InviteRefused` não produz alerta aqui: a recusa derruba a conexão
/// antes de existir um `Enlace` para perguntar `veredito()`, e o `ConnectError`
/// correspondente já ganhou frase em [`motivo_de_conexao_perdida`]. O braço
/// fica pela exaustão do `match` — se um dia a recusa parar de derrubar a
/// conexão, isto para de compilar sozinho, em vez de silenciosamente não
/// alertar ninguém.
fn alerta_do_veredito(veredito: &Verdict) -> Option<Alert> {
    let text = match veredito {
        // ADR 0003 and specs/08-seguranca.md: first contact is stated, not
        // accepted in silence. A pin that establishes itself invisibly is a
        // pin nobody knows to check.
        Verdict::FirstContact { fingerprint } => {
            format!("PRIMEIRO CONTATO — CHAVE FIXADA {fingerprint}")
        }
        // ADR 0006: é para produzir exatamente isto que o link existe.
        Verdict::FirstContactVerified { fingerprint } => {
            format!("PRIMEIRO CONTATO VERIFICADO — O CONVITE CONFIRMOU {fingerprint}")
        }
        // Uma linha por impressão, pela mesma razão de
        // [`motivo_de_conexao_perdida`]: são elas que a pessoa tem que comparar,
        // e lado a lado na mesma linha nenhuma das duas cabe inteira em 80
        // colunas. A faixa de alerta preserva o `\n` e cresce o quanto o texto
        // pedir — ver `ui::alert_rows`.
        Verdict::InviteDisagrees { expected, offered } => format!(
            "O CONVITE NÃO CORRESPONDE A ESTE DOGMA.\nesperada:  {expected}\nofertada:  {offered}"
        ),
        Verdict::Known | Verdict::InviteRefused { .. } => return None,
    };
    Some(Alert {
        text,
        blocking: false,
    })
}

/// Põe na tela o fim de uma sessão cujo enlace fechou no meio de um pedido.
///
/// `specs/05-cliente-tui.md`: só `:q`, `:quit`, `:sair` e Ctrl-C saem do
/// programa; toda outra forma de acabar mostra o motivo e volta à lista. Um
/// enlace fechado é uma sessão que acabou — o Dogma não está mais do outro lado
/// —, e não um defeito do cliente.
///
/// Era o buraco: `Enlace::dizer` devolve [`Fechado`] quando a sessão já morreu,
/// e o `?` levava isso até o `main`, que imprimia `plug encerrou: …` e saía com
/// código diferente de zero. Bastava o `select!` escolher a tecla em vez do
/// `Aviso::Encerrado` que estava pronto ao lado — e ele escolhe qualquer um dos
/// dois — para apertar Enter matar o cliente.
///
/// O que **continua** fatal é o que não tem tela onde caber: um terminal que
/// não se deixa dirigir, uma pasta de configuração onde não dá para escrever.
/// Isto aqui não é um `catch` geral, e é por isso que trata um tipo só.
fn enlace_fechado(runtime: &mut Runtime<'_>) {
    runtime.app.screen = Screen::Lost {
        reason: "O ENLACE FECHOU — o Dogma não está mais do outro lado".to_owned(),
    };
}

/// Refreshes everything that changes on its own: the clock, the audio meters,
/// and the Sync Ratio.
fn tick(runtime: &mut Runtime<'_>, client: &Enlace) {
    runtime.app.clock = clock();

    if let Some(rtt) = client.rtt() {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "a round trip in milliseconds fits an f32 long before it matters"
        )]
        {
            runtime.app.bar.rtt_ms = rtt.as_secs_f32() * 1000.0;
        }
    }

    if let Some(voice) = &runtime.voice {
        let telemetry = voice.telemetry();
        runtime.app.bar.bitrate = telemetry.local.bitrate_bps;
        runtime.app.speaking = telemetry.local.speaking;
        runtime.app.at_field = voice.at_field();
        runtime.app.total_isolation = voice.total_isolation();

        // Jitter and loss are only observable at the receiver, which is why the
        // server's own numbers are not the ones shown here.
        #[allow(
            clippy::cast_possible_truncation,
            reason = "milliseconds and a fraction; f32 is what the protocol carries"
        )]
        {
            runtime.app.bar.jitter_ms = telemetry.worst_jitter_depth_ms() as f32;
            runtime.app.bar.loss = telemetry.worst_loss_fraction() as f32;
        }
    }

    // The Sync Ratio is computed in the core from the same inputs the server
    // uses, so the number on screen means what the protocol says it means.
    runtime.app.bar.sync = runtime.sync.update(SyncInputs {
        rtt_ms: runtime.app.bar.rtt_ms,
        jitter_ms: runtime.app.bar.jitter_ms,
        loss_fraction: runtime.app.bar.loss,
    });
}

/// Mostra o motivo, espera ser lido, e volta à tela de seleção.
///
/// Um cliente que some no instante em que perde o enlace leva o motivo junto,
/// e por isso a pausa continua aqui. O que mudou é aonde a tecla leva: sair do
/// programa é `:q`, `:quit`, `:sair` e Ctrl-C, e mais nada. Toda outra forma de
/// uma sessão acabar — o Dogma não atendeu, o convite apontava para outro, a
/// bateria interna esgotou — volta para a lista, que é de onde se escolhe o
/// próximo. É o que o app já faz: a `tela-fim` dele tem um EJETAR que leva à
/// tela de entrada.
///
/// Devolve sempre `true` porque é sempre isso que significa: volte à seleção.
///
/// Lê do único leitor de teclado e não de um `crossterm::event::read` próprio:
/// com um segundo leitor vivo, quem estiver parado dentro do `read` fica com a
/// tecla, e esta tela cumpriria os trinta segundos inteiros com alguém
/// martelando o teclado nela.
async fn fim_de_sessao(
    terminal: &mut Screen1,
    key_rx: &mut tokio::sync::mpsc::Receiver<Event>,
    runtime: &mut Runtime<'_>,
    hospedagem: &mut Option<seele_server::hospedagem::Hospedagem>,
) -> Result<bool> {
    terminal.draw(|frame| ui::render(frame, &runtime.app, *runtime.theme))?;

    // Tecla que chegou **antes** desta tela não é resposta a ela.
    //
    // Entre escolher um Dogma na lista e o `conectar` devolver, ninguém lê o
    // canal: a identidade é carregada de disco, os pins abrem, o Dogma de casa
    // sobe, o handshake acontece — e as sessenta e quatro vagas do canal vão
    // enchendo com quem apertou Enter de novo, ou segurou (a repetição conta
    // como apertar). Se o Dogma estiver morto, a tela de motivo aparecia e era
    // dispensada no primeiro `recv()` por uma tecla de um segundo atrás: a
    // pessoa voltava à lista sem nunca ter lido `NÃO FOI POSSÍVEL ALCANÇAR O
    // DOGMA`, que é justamente por que este ramo parou de matar o processo.
    //
    // Esvaziar **depois** de desenhar é o que separa uma coisa da outra: o que
    // for apertado com a tela já no ar chega depois daqui e continua valendo.
    esvaziar(key_rx);

    let _ = tokio::time::timeout(Duration::from_secs(30), async {
        while let Some(event) = key_rx.recv().await {
            if tecla_apertada(&event).is_some() {
                return;
            }
        }
    })
    .await;

    desligar(runtime, hospedagem).await;
    Ok(true)
}

/// Joga fora o que já estava no canal, sem esperar por nada.
///
/// `try_recv` e não `recv`: o ponto é não bloquear. O laço acaba tanto no canal
/// vazio quanto no canal fechado, que são os dois `Err` que existem.
fn esvaziar(key_rx: &mut tokio::sync::mpsc::Receiver<Event>) {
    while key_rx.try_recv().is_ok() {}
}

/// Solta o que a sessão tinha de pé, e espera a porta voltar.
///
/// A espera é o ponto: `Hospedagem::encerrar` fecha o endpoint e só volta
/// quando o socket é devolvido, e sem isso a volta seguinte do laço tentaria
/// hospedar numa porta ainda ocupada. É o que o `disconnect` do app já fazia
/// (`apps/seele-app/src/main.rs`), e a razão de a hospedagem não poder
/// simplesmente cair no fim da função: `drop` não espera.
async fn desligar(
    runtime: &mut Runtime<'_>,
    hospedagem: &mut Option<seele_server::hospedagem::Hospedagem>,
) {
    runtime.voice = None;
    if let Some(dogma) = hospedagem.take() {
        dogma.encerrar().await;
    }
}

/// Reads terminal events on a thread of its own.
///
/// `crossterm::event::read` blocks, and blocking the runtime that also drives
/// QUIC would stall the connection every time nobody is typing.
fn read_terminal_events(sender: &tokio::sync::mpsc::Sender<Event>) {
    loop {
        match crossterm::event::read() {
            Ok(event) => {
                if sender.blocking_send(event).is_err() {
                    return;
                }
            }
            Err(_) => return,
        }
    }
}

/// A tecla que um evento representa, se representa alguma.
///
/// `None` para o que não é tecla e para as **solturas**. Um terminal que fala o
/// protocolo do Kitty manda o apertar e o soltar, e o Windows manda os dois de
/// qualquer jeito: sem este filtro, cada tecla conta duas vezes na tela de
/// seleção, e a soltura da tecla que levou alguém até a tela de motivo dispensa
/// a tela antes de a pessoa conseguir ler por que a sessão acabou.
///
/// Repetição conta como apertar: segurar uma tecla é continuar pedindo.
///
/// O laço da sessão **não** passa por aqui, e é de propósito: lá a soltura da
/// barra de espaço é justamente o que fecha o microfone, e quem cuida disso é o
/// [`key_source`].
fn tecla_apertada(evento: &Event) -> Option<KeyEvent> {
    match evento {
        Event::Key(tecla) if tecla.kind != KeyEventKind::Release => Some(*tecla),
        _ => None,
    }
}

/// Turns a terminal key event into zero or more [`Key`]s.
///
/// Returns a list because the space bar in a terminal that cannot report
/// releases has to produce both halves of a press at once.
fn key_source(runtime: &mut Runtime<'_>, event: KeyEvent) -> Vec<Key> {
    // Terminals that report releases send both; without filtering, every key
    // would register twice.
    if runtime.holds && event.kind == KeyEventKind::Repeat {
        return Vec::new();
    }

    // Ctrl-C leaves, from any mode. It is the one binding that does not go
    // through the modal handler, because a client you cannot leave is a client
    // people kill from another window.
    if event.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(event.code, KeyCode::Char('c') | KeyCode::Char('C'))
    {
        runtime.app.quit();
        return Vec::new();
    }

    if event.code == KeyCode::Char(' ') && runtime.app.mode == Mode::Normal {
        return space(runtime, event.kind);
    }

    if runtime.holds && event.kind == KeyEventKind::Release {
        return Vec::new();
    }

    let key = match event.code {
        KeyCode::Char(character) => Key::Char(character),
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Esc,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => Key::BackTab,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        _ => return Vec::new(),
    };
    vec![key]
}

/// Push-to-talk, and what to do when the terminal cannot say "released".
///
/// `specs/05-cliente-tui.md` wants the space bar held. Kitty-protocol terminals
/// report the release and get exactly that. The rest — Terminal.app, iTerm2,
/// most of what somebody connects over SSH with — never send one, and a held
/// key there would mean a microphone that never closes. So they get a latch:
/// press to open, press to close. Different feel, same two states, and the
/// telemetry bar says which one is live either way.
fn space(runtime: &mut Runtime<'_>, kind: KeyEventKind) -> Vec<Key> {
    if runtime.holds {
        return match kind {
            KeyEventKind::Press => vec![Key::SpaceDown],
            KeyEventKind::Release => vec![Key::SpaceUp],
            KeyEventKind::Repeat => Vec::new(),
        };
    }
    if kind != KeyEventKind::Press {
        return Vec::new();
    }
    runtime.latched = !runtime.latched;
    vec![if runtime.latched {
        Key::SpaceDown
    } else {
        Key::SpaceUp
    }]
}

/// Carries out what a keystroke asked for.
async fn act(
    runtime: &mut Runtime<'_>,
    client: &Enlace,
    action: Action,
    line: LineId,
) -> Result<(), Fechado> {
    match action {
        Action::Quit => runtime.app.quit(),

        Action::Send(body) => {
            // specs/02-protocolo.md: idempotent by client_msg_id, so a resend
            // after a lost acknowledgement does not post twice.
            let id = ClientMessageId(runtime.next_message_id);
            runtime.next_message_id += 1;
            let target = runtime.room.current_line.unwrap_or(line);
            client.dizer(target, body.trim().to_owned(), id).await?;
        }

        Action::Command(text) => run_command(runtime, client, &command::parse(&text)).await?,

        Action::StartTalking => {
            if let Some(voice) = &runtime.voice {
                voice.set_key_held(true);
            }
            runtime.app.speaking = true;
        }
        Action::StopTalking => {
            if let Some(voice) = &runtime.voice {
                voice.set_key_held(false);
            }
            runtime.app.speaking = false;
        }

        Action::ToggleAtField => {
            runtime.app.at_field = !runtime.app.at_field;
            if let Some(voice) = &runtime.voice {
                voice.set_at_field(runtime.app.at_field);
            }
        }
        Action::ToggleTotalIsolation => {
            runtime.app.total_isolation = !runtime.app.total_isolation;
            if let Some(voice) = &runtime.voice {
                voice.set_total_isolation(runtime.app.total_isolation);
            }
        }

        Action::Activate => activate(runtime, client).await?,
    }
    Ok(())
}

/// Enter on the selected row: enter a Cage, or open a Line.
async fn activate(runtime: &mut Runtime<'_>, client: &Enlace) -> Result<(), Fechado> {
    let Some(node) = runtime.app.tree.get(runtime.app.selected).cloned() else {
        return Ok(());
    };
    match node {
        Node::Cage { name, .. } => {
            if let Some(id) = runtime.room.find_cage(&name) {
                client.inserir_plug(id).await?;
                runtime.room.enter_cage(id);
                view::project(&runtime.room, &mut runtime.app);
                runtime.app.refazer_busca();
            }
        }
        Node::Line { name } => {
            if let Some(id) = runtime.room.find_line(&name) {
                open_line(runtime, client, id).await?;
            }
        }
        Node::Pilot(_) => {}
    }
    Ok(())
}

/// Opens a Line and asks for the page of history behind it.
///
/// The fetch is what makes `specs/06-clientes-gui.md`'s "sem perda de
/// histórico" true: a client arriving late reads what was already said instead
/// of an empty room.
async fn open_line(
    runtime: &mut Runtime<'_>,
    client: &Enlace,
    line: LineId,
) -> Result<(), Fechado> {
    client.abrir_linha(line).await?;
    runtime.room.open_line(line);
    client.historico(line, 50).await?;
    runtime.app.local.clear();
    view::project(&runtime.room, &mut runtime.app);
    runtime.app.refazer_busca();
    Ok(())
}

async fn run_command(
    runtime: &mut Runtime<'_>,
    client: &Enlace,
    command: &Command,
) -> Result<(), Fechado> {
    match command {
        Command::Quit => runtime.app.quit(),

        Command::Eject => runtime.app.ejetar(),

        Command::Cage { which } => {
            if let Some(id) = runtime.room.find_cage(which) {
                client.inserir_plug(id).await?;
                runtime.room.enter_cage(id);
                view::project(&runtime.room, &mut runtime.app);
                runtime.app.refazer_busca();
            } else {
                note(runtime, format!("nenhum Cage com «{which}»"));
            }
        }

        Command::Line { which } => {
            if let Some(id) = runtime.room.find_line(which) {
                open_line(runtime, client, id).await?;
            } else {
                note(runtime, format!("nenhuma Linha com «{which}»"));
            }
        }

        // Reconnecting means tearing down a live QUIC connection and a running
        // audio thread. Restarting the process does that correctly and this
        // would not, so it says so instead of half-doing it.
        Command::Connect { target } => note(
            runtime,
            format!("reconectar em execução ainda não existe — reinicie com --server {target}"),
        ),

        Command::Sync => {
            let bar = runtime.app.bar;
            note(
                runtime,
                format!(
                    "SYNC {}% · RTT {:.0}ms · JIT {:.0}ms · LOSS {:.2}% · OPUS {}k",
                    bar.sync,
                    bar.rtt_ms,
                    bar.jitter_ms,
                    bar.loss * 100.0,
                    bar.bitrate / 1000
                ),
            );
            // Os tropeços desta máquina, separados dos da rede. Sem isto, "ÁUDIO
            // LOCAL FALHANDO" é um aviso sem número atrás: dá para ver que
            // acendeu e não dá para ver o que subiu, nem se ainda está subindo.
            // Lido de uma vez, antes de qualquer `note`: cada `note` toma o
            // `runtime` emprestado por inteiro, e segurar a voz por cima disso
            // não compila.
            let medido = runtime.voice.as_ref().map(|voice| {
                (
                    voice.telemetry().local,
                    voice.falha_local(),
                    voice.quadros_recusados(),
                    voice.amostras_recusadas_pelo_anel(),
                )
            });
            if let Some((local, falhando, recusados, anel_cheio)) = medido {
                note(
                    runtime,
                    format!(
                        "LOCAL captura {} · saída {} · disp {} · agora {}",
                        local.capture_overruns,
                        local.playback_underruns,
                        local.device_errors,
                        if falhando { "sim" } else { "não" }
                    ),
                );
                // A linha que responde "é a rede ou é esta máquina?".
                //
                // `volta` é quanto tempo o laço de voz levou, no pior caso,
                // entre duas conferidas do prazo de reprodução. Abaixo de 20 ms
                // esta máquina acompanha o relógio sozinha; acima, ela só está
                // em dia porque a reposição a segura — e antes de haver
                // reposição, era aí que o áudio sumia, com o som de perda de
                // rede e sem nada na rede para achar.
                //
                // `recusa` e `anel` são as duas pontas onde áudio se perde
                // **dentro** desta máquina: o que não saiu para o transporte e
                // o que não chegou ao dispositivo. As duas eram `let _ =`.
                note(
                    runtime,
                    format!(
                        "LAÇO volta {:.1}ms · reposição {} · reacerto {} · recusa {} · anel {}",
                        local.playout_worst_lateness_ms,
                        local.playout_catchup_frames,
                        local.playout_resyncs,
                        recusados,
                        anel_cheio,
                    ),
                );
                // A outra metade da mesma pergunta, e a que o `anel` sozinho não
                // respondia: **por que** o anel estava cheio, ou vazio.
                //
                // `ppm` é a diferença medida entre o relógio desta máquina e o
                // cristal do dispositivo de saída — dezenas é o normal de dois
                // cristais quaisquer, e é o que a malha está cancelando. `anel`
                // contra o alvo diz se ela está dando conta; alguns
                // milissegundos de distância são o preço do controle lento, e
                // dezenas significam que não é deriva. `grampo` crescendo diz
                // isso com número: a razão pedida saiu da faixa em que cristal
                // vive, então a causa é outra — taxa diferente da anunciada,
                // dispositivo trocado. `reposição` é o anel tendo raspado o
                // fundo; uma no arranque é o normal, porque ele começa vazio.
                note(
                    runtime,
                    format!(
                        "RITMO {:+.0}ppm · anel {:.1}ms de {:.1}ms · grampo {} · reposição {}",
                        local.pacing_ppm,
                        local.pacing_depth_ms,
                        local.pacing_target_ms,
                        local.pacing_clamps,
                        local.pacing_primes,
                    ),
                );
            }
        }

        Command::Audio => {
            let text = runtime.voice.as_ref().map_or_else(
                || "sem áudio nesta sessão".to_owned(),
                |voice| {
                    let rates = voice.rates();
                    format!(
                        "captura {} Hz · reprodução {} Hz · modo {:?}",
                        rates.capture_hz,
                        rates.playback_hz,
                        voice.mode()
                    )
                },
            );
            note(runtime, text);
        }

        Command::Theme { which } => {
            // Escrito no tema do **programa**, e não numa cópia da sessão: quem
            // trocou de paleta para conseguir ler a tela continua lendo depois
            // de ejetar e entrar noutro Dogma.
            *runtime.theme = Theme::with_palette(match which.as_deref() {
                Some("mono" | "sem-cor") => Palette::Mono,
                Some("16") => Palette::Ansi16,
                Some("256") => Palette::Ansi256,
                Some("true" | "cor") => Palette::True,
                // Cycling downwards makes `:tema` a way to *check* the
                // degradation, which is the thing that needs checking.
                _ => match runtime.theme.palette {
                    Palette::True => Palette::Ansi256,
                    Palette::Ansi256 => Palette::Ansi16,
                    Palette::Ansi16 => Palette::Mono,
                    Palette::Mono => Palette::True,
                },
            });
            note(runtime, format!("tema {:?}", runtime.theme.palette));
        }

        Command::Convite => {
            if runtime.app.convite.is_some() {
                runtime.app.convite_visivel = true;
            } else {
                note(
                    runtime,
                    "este cliente não está hospedando; só o anfitrião tem convite a dar".to_owned(),
                );
            }
        }

        Command::About => note(
            runtime,
            format!(
                "SEELE · plug {} · protocolo v{}",
                env!("CARGO_PKG_VERSION"),
                seele_core::PROTOCOL_VERSION
            ),
        ),

        Command::AtField => {
            runtime.app.at_field = !runtime.app.at_field;
            if let Some(voice) = &runtime.voice {
                voice.set_at_field(runtime.app.at_field);
            }
        }

        Command::TotalIsolation => {
            runtime.app.total_isolation = !runtime.app.total_isolation;
            if let Some(voice) = &runtime.voice {
                voice.set_total_isolation(runtime.app.total_isolation);
            }
        }

        Command::VoiceMode { which } => {
            let mode = match which.as_str() {
                "vad" | "voz" | "automatico" | "automático" => Some(VoiceMode::VoiceActivated),
                "ptt" | "tecla" => Some(VoiceMode::PushToTalk),
                "aberto" | "open" => Some(VoiceMode::Open),
                _ => None,
            };
            match (mode, &runtime.voice) {
                (Some(mode), Some(voice)) => {
                    voice.set_mode(mode);
                    note(runtime, format!("voz: {mode:?}"));
                }
                (Some(_), None) => note(runtime, "sem áudio nesta sessão".to_owned()),
                (None, _) => note(runtime, format!("modo de voz desconhecido: «{which}»")),
            }
        }

        Command::Volume { who, percent } => {
            let ssrc = runtime.room.ssrc_of(who);
            match (ssrc, &runtime.voice) {
                (Some(ssrc), Some(voice)) => {
                    voice.set_gain(ssrc.get(), f32::from(*percent) / 100.0);
                    note(runtime, format!("volume de {who}: {percent}%"));
                }
                (None, _) => note(runtime, format!("não conheço «{who}»")),
                (_, None) => note(runtime, "sem áudio nesta sessão".to_owned()),
            }
        }

        Command::Unknown { typed } if typed.is_empty() => {}
        Command::Unknown { typed } => note(runtime, format!("comando desconhecido: «{typed}»")),
    }
    Ok(())
}

/// Answers a command in the message panel.
///
/// Not an alert: `:sync` is not a problem, and putting routine answers in the
/// alert banner is how the alert banner stops being read.
///
/// Refreshes the search itself rather than leaving it to callers: `local` is
/// in scope for `refazer_busca`, and a call at every one of this function's
/// dozen-odd call sites is a call that eventually gets left out somewhere.
fn note(runtime: &mut Runtime<'_>, text: String) {
    runtime.app.local.push(ChatLine {
        at: clock_short(),
        author: "SEELE".to_owned(),
        body: text,
        own: false,
    });
    runtime.app.refazer_busca();
}

/// Wall-clock time for the title bar.
///
/// Formatting lives here because the core deals in monotonic durations and has
/// no opinion about what time it is where the pilot is sitting.
fn clock() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

fn clock_short() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::primeira_chave_de_mensagem;

    #[test]
    fn a_chave_de_uma_sessao_nao_e_a_mesma_de_outra() {
        // Pendência 19. O servidor deduplica por `(author_id,
        // client_message_id)`, e o `author_id` vem da chave em disco — ele é o
        // mesmo em todas as sessões desta máquina, para sempre. Se a outra
        // metade recomeçar num valor fixo, a mensagem 1 de hoje é lida como
        // reenvio da mensagem 1 de ontem e **não é gravada**.
        //
        // Nada mais neste repositório cobre isto: o teste de conformidade que
        // encena reconexão fala pelo `Enlace` do core e passa a chave na mão,
        // então ele nunca chega aqui. Foi assim que a mutação «volta a começar
        // em 1» sobreviveu.
        let primeira = primeira_chave_de_mensagem();
        let segunda = primeira_chave_de_mensagem();

        assert_ne!(
            primeira, segunda,
            "duas sessões nasceriam com a mesma chave, e a segunda não gravaria nada"
        );
        assert!(
            primeira > u64::from(u16::MAX),
            "a chave nasceu num valor pequeno, que é o que um contador reiniciado dá: {primeira}"
        );

        // A metade baixa é o contador, e ela precisa de espaço para andar: uma
        // sessão que estoure a própria metade invade a de outra.
        assert_eq!(
            primeira & 0xFFFF_FFFF,
            1,
            "a metade baixa não começa em 1, então o espaço de contagem da sessão não é inteiro"
        );
    }

    use super::*;

    fn args_de_teste(expected_fingerprint: Option<String>) -> Args {
        Args {
            server: SocketAddr::from(([127, 0, 0, 1], 8383)),
            server_name: "localhost".to_owned(),
            pin_key: "127.0.0.1:8383".to_owned(),
            nickname: "piloto".to_owned(),
            cage: CageId(1),
            line: LineId(1),
            no_audio: true,
            join_secret: None,
            expected_fingerprint,
            bilhete: None,
            hospedar: false,
            alternativos: Vec::new(),
        }
    }

    fn args_com_alternativos(alternativos: Vec<String>) -> Args {
        Args {
            alternativos,
            ..args_de_teste(None)
        }
    }

    #[test]
    fn a_impressao_do_link_chega_ao_destino() {
        let args = args_de_teste(Some("aaaa1111".to_owned()));
        let destino = construir_destinos(&args).remove(0);
        assert_eq!(destino.impressao_esperada, Some("aaaa1111".to_owned()));
    }

    #[test]
    fn sem_link_o_destino_nao_tem_impressao_para_conferir() {
        let args = args_de_teste(None);
        let destino = construir_destinos(&args).remove(0);
        assert_eq!(destino.impressao_esperada, None);
    }

    #[test]
    fn os_outros_enderecos_do_convite_viram_destinos_com_a_mesma_credencial() {
        // Um convite com vários endereços é **um** Dogma por caminhos
        // diferentes: mesma impressão esperada, mesmo apelido, mesmo convite de
        // uso único. O que muda é a chave do pin, que é o endereço.
        let args = args_com_alternativos(vec![
            "192.168.0.30:8383".to_owned(),
            "[::1]:8383".to_owned(),
        ]);
        let destinos = construir_destinos(&args);
        assert_eq!(destinos.len(), 3, "os alternativos não viraram destinos");
        assert_eq!(
            destinos.first().map(|destino| destino.servidor),
            Some(args.server),
            "a ordem mudou"
        );
        assert!(
            destinos
                .iter()
                .all(|destino| destino.apelido == args.nickname),
            "um dos endereços entraria com outra credencial"
        );
        let chaves: std::collections::HashSet<&str> = destinos
            .iter()
            .map(|destino| destino.chave_do_pin.as_str())
            .collect();
        assert_eq!(
            chaves.len(),
            3,
            "dois endereços arquivam o pin no mesmo lugar"
        );
    }

    #[test]
    fn um_endereco_alternativo_que_nao_resolve_e_descartado_sem_derrubar_o_link() {
        // Perder um caminho não pode custar todos: o convite pode ter sido
        // gerado numa máquina cujo endereço esta aqui não sabe alcançar.
        let args = args_com_alternativos(vec!["nao-existe.invalid:8383".to_owned()]);
        let destinos = construir_destinos(&args);
        assert_eq!(
            destinos.len(),
            1,
            "um alternativo que não resolve entrou na lista de tentativas"
        );
        assert_eq!(
            destinos.first().map(|destino| destino.servidor),
            Some(args.server)
        );
    }

    #[test]
    fn a_recusa_do_convite_diz_as_duas_impressoes_em_linhas_proprias() {
        // Guarda o alinhamento de que `render_lost` depende: reflui-las no
        // mesmo parágrafo é a única forma em que ninguém consegue comparar
        // dois hexadecimais de 64 caracteres.
        let expected = "a".repeat(64);
        let offered = "b".repeat(64);
        let motivo = motivo_de_conexao_perdida(&ConnectError::InviteMismatch {
            expected: expected.clone(),
            offered: offered.clone(),
        });

        let linha_esperada = motivo.lines().find(|linha| linha.contains(&expected));
        let linha_ofertada = motivo.lines().find(|linha| linha.contains(&offered));
        let (Some(linha_esperada), Some(linha_ofertada)) = (linha_esperada, linha_ofertada) else {
            panic!("uma das duas impressões sumiu do motivo:\n{motivo}");
        };
        assert_ne!(
            linha_esperada, linha_ofertada,
            "as duas impressões caíram na mesma linha, a forma em que não dá para compará-las"
        );
    }

    #[test]
    fn qualquer_outro_erro_de_conexao_usa_a_frase_generica() {
        let motivo = motivo_de_conexao_perdida(&ConnectError::Unreachable);
        assert!(motivo.contains("NÃO FOI POSSÍVEL ALCANÇAR O DOGMA"));
    }

    #[test]
    fn primeiro_contato_alerta_com_a_impressao_fixada() {
        let veredito = Verdict::FirstContact {
            fingerprint: "aaaa1111".to_owned(),
        };
        let Some(alerta) = alerta_do_veredito(&veredito) else {
            panic!("primeiro contato ficou sem alerta");
        };
        assert!(alerta.text.contains("aaaa1111"));
        assert!(!alerta.blocking);
    }

    #[test]
    fn primeiro_contato_verificado_nao_e_o_mesmo_texto_do_primeiro_contato_cego() {
        // Um convite que confirmou a chave é notícia diferente de uma chave
        // fixada sem nada para conferir — trocar os dois ramos entre si é o
        // defeito que este teste existe para pegar.
        let cego = alerta_do_veredito(&Verdict::FirstContact {
            fingerprint: "aaaa1111".into(),
        });
        let verificado = alerta_do_veredito(&Verdict::FirstContactVerified {
            fingerprint: "aaaa1111".into(),
        });
        let (Some(cego), Some(verificado)) = (cego, verificado) else {
            panic!("um dos dois vereditos ficou sem alerta");
        };
        assert_ne!(cego.text, verificado.text);
    }

    #[test]
    fn um_dogma_ja_conhecido_nao_gera_alerta() {
        assert!(alerta_do_veredito(&Verdict::Known).is_none());
    }

    #[test]
    fn um_convite_recusado_tambem_nao_gera_alerta_aqui() {
        // Este veredito nunca chega até aqui em produção — a recusa derruba a
        // conexão antes de existir um `Enlace` para perguntar `veredito()` —,
        // mas o braço existe para o `match` continuar exaustivo, e por isso é
        // testado.
        let veredito = Verdict::InviteRefused {
            expected: "a".into(),
            offered: "b".into(),
        };
        assert!(alerta_do_veredito(&veredito).is_none());
    }

    #[test]
    fn um_link_que_discorda_de_um_dogma_conhecido_mostra_as_duas_impressoes() {
        // Com os 64 caracteres de verdade, que é o tamanho em que o alerta
        // deixa de caber numa linha só. `ui::alert_rows` é quem desenha isto em
        // mais de uma linha; aqui se guarda o que ele precisa receber.
        let expected = "a".repeat(64);
        let offered = "b".repeat(64);
        let veredito = Verdict::InviteDisagrees {
            expected: expected.clone(),
            offered: offered.clone(),
        };
        let Some(alerta) = alerta_do_veredito(&veredito) else {
            panic!("o desacordo do convite ficou sem alerta");
        };
        assert!(!alerta.blocking);

        let linha_esperada = alerta.text.lines().find(|linha| linha.contains(&expected));
        let linha_ofertada = alerta.text.lines().find(|linha| linha.contains(&offered));
        let (Some(linha_esperada), Some(linha_ofertada)) = (linha_esperada, linha_ofertada) else {
            panic!("uma das duas impressões sumiu do alerta:\n{}", alerta.text);
        };
        assert_ne!(
            linha_esperada, linha_ofertada,
            "as duas impressões caíram na mesma linha, a forma em que não dá para compará-las"
        );
    }

    fn evento(kind: KeyEventKind) -> Event {
        Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('a'),
            KeyModifiers::NONE,
            kind,
        ))
    }

    #[test]
    fn soltar_uma_tecla_nao_conta_como_apertar() {
        // Esta é a regra que só quebra no Windows e em terminal com o protocolo
        // do Kitty, onde chegam as duas metades de cada tecla. Quem desenvolve
        // no macOS nunca vê a regressão acontecer, e é por isso que ela precisa
        // de teste e não de um leitor atento: sem o filtro, a tela que diz por
        // que a sessão acabou some antes de ser lida, dispensada pela soltura
        // da tecla que levou até ela.
        assert!(tecla_apertada(&evento(KeyEventKind::Release)).is_none());
        assert!(tecla_apertada(&evento(KeyEventKind::Press)).is_some());
    }

    #[test]
    fn segurar_uma_tecla_continua_sendo_apertar() {
        assert!(tecla_apertada(&evento(KeyEventKind::Repeat)).is_some());
    }

    #[test]
    fn o_que_nao_e_tecla_nao_vira_tecla() {
        // Redimensionar a janela não é escolher um Dogma nem dispensar uma
        // tela, e com a captura de mouse ligada há evento chegando o tempo todo.
        assert!(tecla_apertada(&Event::Resize(80, 24)).is_none());
        assert!(tecla_apertada(&Event::FocusGained).is_none());
    }

    #[test]
    fn a_tela_de_motivo_nao_come_a_tecla_que_vem_depois_dela() {
        // As duas metades da regra, e a segunda importa tanto quanto a
        // primeira: esvaziar o canal não pode virar "ignore teclas por um
        // tempo". O que chegou antes da tela some; o que chegar depois vale.
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Event>(64);
        for _ in 0..5 {
            assert!(tx.try_send(evento(KeyEventKind::Press)).is_ok());
        }

        esvaziar(&mut rx);
        assert!(
            rx.try_recv().is_err(),
            "sobrou tecla de antes da tela, e ela dispensaria a tela antes de alguém ler"
        );

        assert!(tx.try_send(evento(KeyEventKind::Press)).is_ok());
        assert!(
            rx.try_recv().is_ok(),
            "a tecla apertada com a tela no ar foi engolida junto"
        );
    }

    #[test]
    fn um_enlace_fechado_vira_motivo_na_tela_e_nao_erro_do_programa() {
        // `specs/05-cliente-tui.md`: só `:q`, `:quit`, `:sair` e Ctrl-C saem do
        // programa. Mandar mensagem por um enlace que fechou devolvia `Fechado`,
        // o `?` levava até o `main`, e o cliente morria com uma linha de stderr
        // em vez de dizer o que houve.
        let mut tema = Theme::detect();
        let mut runtime = Runtime {
            app: App::new(),
            room: Room::new(),
            theme: &mut tema,
            voice: None,
            sync: SyncRatio::new(),
            holds: false,
            latched: false,
            next_message_id: 1,
        };

        enlace_fechado(&mut runtime);

        let Screen::Lost { reason } = &runtime.app.screen else {
            panic!("um enlace fechado não pôs na tela o fim da sessão");
        };
        assert!(
            !reason.trim().is_empty(),
            "a tela do fim está sem motivo, que é a única coisa que ela tem para dar"
        );
        assert!(
            !runtime.app.quit,
            "fechar o enlace não é pedido para fechar o cliente"
        );
    }

    #[test]
    fn a_tecla_atravessa_inteira() {
        // O `escolher` lê `code` e `modifiers` depois deste filtro, então o
        // filtro não pode devolver só "sim".
        let ctrl_c = Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            KeyEventKind::Press,
        ));
        let Some(tecla) = tecla_apertada(&ctrl_c) else {
            panic!("uma tecla apertada voltou como nenhuma");
        };
        assert_eq!(tecla.code, KeyCode::Char('c'));
        assert!(tecla.modifiers.contains(KeyModifiers::CONTROL));
    }
}
