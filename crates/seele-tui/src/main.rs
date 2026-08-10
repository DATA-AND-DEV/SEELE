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
use seele_core::conhecidos::Conhecidos;
use seele_core::enlace::{Aviso, Destino, Enlace, Motivo};
use seele_core::{
    identity, CageId, ClientMessageId, FilePinStore, LineId, PinDecision, Room, SyncInputs,
    SyncRatio, Voice, VoiceMode,
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
    nickname: String,
    cage: CageId,
    line: LineId,
    no_audio: bool,
    /// Convite ou senha, quando o Dogma pede.
    join_secret: Option<String>,
    /// Impressão digital esperada, quando veio num link.
    expected_fingerprint: Option<String>,
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
            nickname: escolha.apelido.clone(),
            cage: CageId(escolha.cage),
            line: LineId(1),
            no_audio: false,
            join_secret: escolha.convite.clone(),
            expected_fingerprint: escolha.impressao_digital.clone(),
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
    let mut hospedar = false;
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
                target = convite.alvo;
                expected_fingerprint = convite.impressao_digital;
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
        nickname,
        cage,
        line,
        no_audio,
        join_secret,
        expected_fingerprint,
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
fn resolve(target: &str) -> Result<(SocketAddr, String, String)> {
    let with_port = if target.contains(':') {
        target.to_owned()
    } else {
        format!("{target}:8383")
    };
    let host = with_port
        .rsplit_once(':')
        .map_or(with_port.clone(), |(host, _)| host.to_owned());
    let address = with_port
        .to_socket_addrs()
        .with_context(|| format!("não consegui resolver {target}"))?
        .next()
        .ok_or_else(|| anyhow!("{target} não resolveu para nenhum endereço"))?;

    // An IP is not a name the M2 server's certificate carries, so TLS is handed
    // the name it does carry. The pin, meanwhile, is filed under the address —
    // which is the thing that actually distinguishes one server from another.
    let server_name = if host.parse::<std::net::IpAddr>().is_ok() {
        "localhost".to_owned()
    } else {
        host
    };
    Ok((address, server_name, with_port))
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
/// Laço próprio, bloqueante, e de propósito: aqui ainda não existe conexão nem
/// áudio, então não há nada para o runtime fazer enquanto ninguém digita. O
/// laço assíncrono do `run` existe porque lá **há** — pacotes chegando enquanto
/// se digita — e copiá-lo para cá seria complexidade sem motivo.
///
/// `Ok(None)` é ter desistido, e sair calado é a resposta certa: quem apertou
/// `q` não precisa de mensagem de erro.
fn escolher(terminal: &mut Screen1, tema: Theme, home: &std::path::Path) -> Result<Option<Args>> {
    let mut lista = Conhecidos::abrir(home.join("conhecidos"))?;
    let mut tela = Selecao::nova(lista.listar());

    loop {
        terminal.draw(|frame| selecao::desenhar(frame, &tela, tema))?;

        let Event::Key(tecla) = crossterm::event::read()? else {
            continue;
        };
        // Sem isto uma tecla conta duas vezes no Windows, onde chegam o
        // pressionar e o soltar.
        if tecla.kind == KeyEventKind::Release {
            continue;
        }
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
struct Runtime {
    app: App,
    /// What is true about the session. Owned by `seele-core`, not by this file.
    room: Room,
    theme: Theme,
    voice: Option<Voice>,
    sync: SyncRatio,
    /// True when the terminal reports key releases, so the space bar can be
    /// held rather than latched.
    holds: bool,
    /// The latch, for terminals that cannot.
    latched: bool,
    next_message_id: u64,
}

#[allow(
    clippy::too_many_lines,
    reason = "one event loop; the branches are the interface's whole surface"
)]
async fn run(terminal: &mut Screen1, args: Option<Args>, holds: bool) -> Result<()> {
    let home = config_dir();
    let tema = Theme::detect();

    // Sem argumento nenhum, a tela decide. Com qualquer flag, ela nem aparece.
    let args = match args {
        Some(args) => args,
        None => match escolher(terminal, tema, &home)? {
            Some(args) => args,
            None => return Ok(()),
        },
    };

    let mut runtime = Runtime {
        app: App::new(),
        room: Room::new(),
        theme: tema,
        voice: None,
        sync: SyncRatio::new(),
        holds,
        latched: false,
        next_message_id: 1,
    };

    // The boot screen goes up before the connection is attempted and comes down
    // when it lands. specs/05-cliente-tui.md: it lasts the real time of the
    // connection, and nothing is added to make it look busier.
    runtime.app.screen = Screen::Boot;
    terminal.draw(|frame| ui::render(frame, &runtime.app, runtime.theme))?;

    // ADR 0004, and the reason it has to be on disk: CASPER binds a nickname to
    // the identity that first claimed it, so a client that generates a fresh key
    // each run can never come back under its own name.
    let key = identity::load_or_create(&home.join("identity.key"))?;
    // ADR 0003 is trust on *first* use, which needs the first use remembered.
    let pins = Arc::new(FilePinStore::open(home.join("pins"))?);

    // `--hospedar`: o Dogma sobe aqui dentro, antes de conectar. Fica vivo pelo
    // tempo desta função — quando o cliente sai, o Dogma vai junto, que é o
    // comportamento certo para "estou hospedando uma conversa".
    let hospedagem = if args.hospedar {
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
    let destino = Destino {
        servidor: args.server,
        nome_tls: args.server_name.clone(),
        chave_do_pin: args.pin_key.clone(),
        apelido: args.nickname.clone(),
        segredo: args.join_secret.clone(),
    };
    let mut client = match Enlace::conectar(destino, key, pins).await {
        Ok(client) => client,
        Err(error) => {
            runtime.app.screen = Screen::Lost {
                reason: format!("NÃO FOI POSSÍVEL ALCANÇAR O DOGMA: {error}"),
            };
            return wait_for_key(terminal, &runtime).await;
        }
    };

    // Quando o link trouxe a impressão digital, ela é conferida aqui — e é o
    // que transforma o primeiro contato de cego em verificado. Sem isso o ADR
    // 0003 depende de a pessoa conferir por outro canal, o que ninguém faz.
    if let Some(esperada) = &args.expected_fingerprint {
        let oferecida = match client.pin_decision() {
            PinDecision::FirstContact { fingerprint } => fingerprint.clone(),
            PinDecision::Matches => esperada.clone(),
            PinDecision::Changed { offered, .. } => offered.clone(),
        };
        if !oferecida.eq_ignore_ascii_case(esperada) {
            runtime.app.screen = Screen::Lost {
                reason: format!(
                    "ESTE NÃO É O DOGMA DO CONVITE.\n\nesperada:  {esperada}\nofertada:  {oferecida}"
                ),
            };
            return wait_for_key(terminal, &runtime).await;
        }
    }

    // ADR 0003 and specs/08-seguranca.md: first contact is stated, not accepted
    // in silence. A pin that establishes itself invisibly is a pin nobody knows
    // to check.
    if let PinDecision::FirstContact { fingerprint } = client.pin_decision() {
        runtime.app.alert = Some(Alert {
            text: format!("PRIMEIRO CONTATO — CHAVE FIXADA {fingerprint}"),
            blocking: false,
        });
    }

    let cage = args.cage;
    client.inserir_plug(cage).await?;
    client.abrir_linha(args.line).await?;
    client.historico(args.line, 50).await?;

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

    let (key_tx, mut key_rx) = tokio::sync::mpsc::channel::<Event>(64);
    std::thread::spawn(move || read_terminal_events(&key_tx));

    let mut next_tick = Instant::now();
    let mut last_draw = Instant::now() - FRAME;
    let mut dirty = true;

    loop {
        if dirty && last_draw.elapsed() >= FRAME {
            terminal.draw(|frame| ui::render(frame, &runtime.app, runtime.theme))?;
            last_draw = Instant::now();
            dirty = false;
        }
        if runtime.app.quit {
            return Ok(());
        }

        tokio::select! {
            event = key_rx.recv() => {
                let Some(event) = event else { return Ok(()) };
                dirty = true;
                match event {
                    Event::Key(key_event) => {
                        for key in key_source(&mut runtime, key_event) {
                            if let Some(action) = runtime.app.on_key(key) {
                                act(&mut runtime, &client, action, args.line).await?;
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
                        if runtime.room.apply(&message).any() {
                            view::project(&runtime.room, &mut runtime.app);
                            dirty = true;
                        }
                    }
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
                        if runtime.voice.is_some() {
                            runtime.voice = Voice::start(*media, sessao.ssrc).ok();
                        }
                        runtime.room.adopt(&sessao, &args.nickname);
                        view::project(&runtime.room, &mut runtime.app);
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
                        return wait_for_key(terminal, &runtime).await;
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

/// Refreshes everything that changes on its own: the clock, the audio meters,
/// and the Sync Ratio.
fn tick(runtime: &mut Runtime, client: &Enlace) {
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

/// Blocks until any key, so a final screen can be read.
///
/// A client that exits the instant it loses the link takes the reason with it.
async fn wait_for_key(terminal: &mut Screen1, runtime: &Runtime) -> Result<()> {
    terminal.draw(|frame| ui::render(frame, &runtime.app, runtime.theme))?;
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if crossterm::event::poll(Duration::from_millis(200))?
            && matches!(crossterm::event::read()?, Event::Key(_))
        {
            return Ok(());
        }
    }
    Ok(())
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

/// Turns a terminal key event into zero or more [`Key`]s.
///
/// Returns a list because the space bar in a terminal that cannot report
/// releases has to produce both halves of a press at once.
fn key_source(runtime: &mut Runtime, event: KeyEvent) -> Vec<Key> {
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
fn space(runtime: &mut Runtime, kind: KeyEventKind) -> Vec<Key> {
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
async fn act(runtime: &mut Runtime, client: &Enlace, action: Action, line: LineId) -> Result<()> {
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
async fn activate(runtime: &mut Runtime, client: &Enlace) -> Result<()> {
    let Some(node) = runtime.app.tree.get(runtime.app.selected).cloned() else {
        return Ok(());
    };
    match node {
        Node::Cage { name, .. } => {
            if let Some(id) = runtime.room.find_cage(&name) {
                client.inserir_plug(id).await?;
                runtime.room.enter_cage(id);
                view::project(&runtime.room, &mut runtime.app);
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
async fn open_line(runtime: &mut Runtime, client: &Enlace, line: LineId) -> Result<()> {
    client.abrir_linha(line).await?;
    runtime.room.open_line(line);
    client.historico(line, 50).await?;
    runtime.app.local.clear();
    view::project(&runtime.room, &mut runtime.app);
    Ok(())
}

async fn run_command(runtime: &mut Runtime, client: &Enlace, command: &Command) -> Result<()> {
    match command {
        Command::Quit => runtime.app.quit(),

        Command::Cage { which } => {
            if let Some(id) = runtime.room.find_cage(which) {
                client.inserir_plug(id).await?;
                runtime.room.enter_cage(id);
                view::project(&runtime.room, &mut runtime.app);
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
            if let Some(voice) = runtime.voice.as_ref() {
                let local = voice.telemetry().local;
                note(
                    runtime,
                    format!(
                        "LOCAL captura {} · saída {} · disp {} · agora {}",
                        local.capture_overruns,
                        local.playback_underruns,
                        local.device_errors,
                        if voice.falha_local() { "sim" } else { "não" }
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
            runtime.theme = Theme::with_palette(match which.as_deref() {
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
fn note(runtime: &mut Runtime, text: String) {
    runtime.app.local.push(ChatLine {
        at: clock_short(),
        author: "SEELE".to_owned(),
        body: text,
        own: false,
    });
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
