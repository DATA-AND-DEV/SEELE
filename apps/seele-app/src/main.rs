//! The desktop client — a Tauri shell over [`seele_ffi`].
//!
//! `specs/06-clientes-gui.md` sets the shape of this file in one sentence:
//! "Nenhuma lógica de protocolo em JavaScript. Se o frontend precisa saber o
//! que é um `ssrc`, algo está errado." So the frontend gets a `Snapshot` and
//! sends back verbs — enter this Cage, say this, mute — and every one of them
//! is a call straight through to the FFI.
//!
//! Nothing here decides anything either. If a command in this file grows a
//! judgement, it belongs in `seele-core`, and the terminal client would have had
//! to grow the same one.
//!
//! # Threading
//!
//! [`seele_ffi::Plug::connect`] blocks, so it runs on a blocking thread. Events
//! arrive on the FFI's driver thread; [`Bridge`] is what marshals them onto the
//! webview, which is the "a casca marshala para sua thread de UI" the spec asks
//! for.

// A desktop shell with no window is not a desktop shell. The attribute keeps
// the console from opening behind the app on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{Arc, Mutex};

use seele_ffi::{ConnectConfig, Event, EventListener, Plug, PlugError, Snapshot, VoiceMode};
use tauri::{AppHandle, Emitter, Manager, State};

/// The name the webview listens on.
///
/// One channel rather than one per variant: the payload already says which
/// [`Event`] it is, and a frontend subscribing to seven names would drift from
/// this list the first time one is added.
const EVENT_CHANNEL: &str = "seele://event";

/// O canal por onde o download de uma atualização diz por onde vai.
///
/// Separado do [`EVENT_CHANNEL`] de propósito: aquele carrega `Event`, que é o
/// que a FFI emite sobre a conversa, e um andamento de download não é um evento
/// de sessão. Quem estiver ouvindo a conversa não deve ter que peneirar bytes
/// baixados, e quem baixa não precisa estar em sessão nenhuma.
const CANAL_DE_ATUALIZACAO: &str = "seele://atualizacao";

/// Everything the commands share.
#[derive(Default)]
struct Session {
    plug: Mutex<Option<Arc<Plug>>>,
    /// O Dogma que este app está hospedando, quando está.
    ///
    /// Vive aqui e não numa variável local porque tem que sobreviver ao comando
    /// que o criou: o servidor fica de pé enquanto a janela estiver aberta.
    hospedagem: Mutex<Option<seele_server::hospedagem::Hospedagem>>,
    /// A busca corrente. O cursor é estado de sessão, e é o que impede a regra
    /// de dar-a-volta de ser reescrita em JavaScript.
    busca: Mutex<Option<seele_ffi::search::Search>>,
    /// O último `seele://` lido, **inteiro** — impressão digital inclusive.
    ///
    /// A impressão é o que `connect` entrega como
    /// [`seele_ffi::ConnectConfig::expected_fingerprint`]: é ela que transforma
    /// o primeiro contato de cego em verificado, e é a razão de o ADR 0006 ter
    /// fechado — `seele-proto/src/uri.rs` chama `fp` "o motivo principal de isto
    /// existir".
    ///
    /// # Uma decisão que se inverteu
    ///
    /// Este campo dizia que a impressão **não** atravessa a ponte. Era a
    /// resposta certa enquanto este app não sabia conferir nada: sem
    /// `expected_fingerprint` na FFI, mandar a string ao frontend não daria a
    /// ninguém nada para fazer com ela além de reescrever a comparação em
    /// JavaScript, que é exatamente o que `specs/06-clientes-gui.md:19` proíbe.
    ///
    /// O que mudou foi o outro lado: [`seele_ffi::Plug::connect`] devolve o
    /// veredito já decidido, em Rust, com a comparação feita. A string que
    /// atravessa agora não é uma entrada de decisão — é o que uma pessoa lê e
    /// confere por outro canal, do mesmo jeito que o `PinChanged` já mandava as
    /// duas impressões porque a coisa toda é um humano compará-las (ADR 0003).
    /// O frontend continua sem decidir nada.
    convite: Mutex<Option<seele_ffi::uri::Convite>>,
    /// A versão nova que a pessoa acabou de ver, esperando o «instalar».
    ///
    /// Guardada entre os dois comandos porque **quem decide é ela**: procurar
    /// não baixa nada, e instalar não procura de novo. Sem este campo, apertar
    /// «instalar» refaria a consulta, e o que seria instalado poderia não ser o
    /// que estava escrito na tela quando a pessoa leu e concordou.
    ///
    /// Some(_) é a única forma de [`instalar_atualizacao`] ter o que instalar, e
    /// é por isso que ele recusa com [`FalhaAoAtualizar::NadaEscolhido`] em vez
    /// de silenciosamente ir procurar.
    atualizacao: Mutex<Option<tauri_plugin_updater::Update>>,
}

impl Session {
    /// The live handle, or the reason there is none.
    fn plug(&self) -> Result<Arc<Plug>, PlugError> {
        self.plug
            .lock()
            .map_err(|_| PlugError::NotConnected)?
            .clone()
            .ok_or(PlugError::NotConnected)
    }
}

/// Carries FFI events onto the webview.
struct Bridge {
    app: AppHandle,
}

impl EventListener for Bridge {
    fn on_event(&self, event: Event) {
        // A failed emit means the window is gone, which is not worth a log line
        // per event during shutdown.
        let _ = self.app.emit(EVENT_CHANNEL, &event);
    }
}

/// Where this client keeps its identity and its pins. ADR 0017.
///
/// The FFI takes a path because the shell knows where its platform keeps
/// configuration and the core knows how to persist an identity. `$SEELE_HOME`
/// comes first so the desktop app and `plug` can be told to be the same pilot —
/// which is what makes a session resumable between them.
fn config_dir(app: &AppHandle) -> String {
    if let Ok(home) = std::env::var("SEELE_HOME") {
        return home;
    }
    // The same `~/.config/seele` the terminal client uses, deliberately: two
    // clients on one machine should be one pilot unless told otherwise.
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return format!("{xdg}/seele");
    }
    if let Ok(home) = std::env::var("HOME") {
        return format!("{home}/.config/seele");
    }
    app.path()
        .app_config_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| ".seele".to_owned())
}

/// O que uma conexão bem-sucedida entrega à tela.
///
/// O veredito vem **junto** do `Snapshot`, e não por evento: `Plug::connect` só
/// devolve o `Arc<Plug>` depois de a identidade estar decidida, então uma casca
/// que se inscrevesse para ouvi-lo chegaria sempre tarde demais.
#[derive(Debug, serde::Serialize)]
struct Entrada {
    /// A tela inteira, como sempre.
    snapshot: Snapshot,
    /// O que a chave deste Dogma acabou de ser.
    ///
    /// `crates/seele-core/src/tofu.rs` é explícito: a casca tem que dizer o que
    /// acabou de confiar. Um pin que se estabelece invisível é um pin que
    /// ninguém sabe que devia conferir.
    veredito: seele_ffi::Trust,
}

#[tauri::command]
async fn connect(
    app: AppHandle,
    session: State<'_, Session>,
    server: String,
    nickname: String,
    audio: bool,
    join_secret: Option<String>,
) -> Result<Entrada, PlugError> {
    if session.plug().is_ok() {
        return Err(PlugError::AlreadyConnected);
    }

    // O convite guardado vale para o Dogma dele e para nenhum outro. Quem cola
    // um link e depois troca o endereço no campo deixaria para trás uma
    // confirmação de identidade que não é deste servidor — e agora que a FFI
    // sabe conferi-la, uma sobra dessas seria uma recusa que ninguém consegue
    // explicar.
    //
    // O que sobrevive ao descarte é o que vai conferir esta conexão.
    let esperada = match session.convite.lock() {
        Ok(mut slot) => {
            if slot.as_ref().is_some_and(|convite| convite.alvo != server) {
                *slot = None;
            }
            slot.as_ref()
                .and_then(|convite| convite.impressao_digital.clone())
        }
        // Sem o cadeado não há convite a ler. Entrar sem a confirmação é o
        // comportamento de quem digitou o endereço à mão, e é o pior que pode
        // acontecer aqui: nunca uma conferência contra um valor duvidoso.
        Err(_) => None,
    };

    let home = config_dir(&app);
    // Guardados antes de a configuração levar os originais para a outra thread:
    // a lista de visitados só é escrita lá embaixo, depois de a conexão existir.
    let alvo = server.clone();
    let apelido = nickname.clone();
    let casa = home.clone();
    let config = ConnectConfig {
        server,
        nickname,
        home,
        audio,
        join_secret: join_secret.filter(|s| !s.trim().is_empty()),
        expected_fingerprint: esperada,
        // O microfone escolhido no Terminal Dogma, lido do disco a cada
        // conexão em vez de guardado em memória: quem escolheu ontem não
        // escolhe de novo hoje, e quem nunca escolheu continua no padrão da
        // máquina. Um dispositivo que sumiu não impede de entrar — a FFI cai
        // para o padrão e a tela mostra o que abriu de verdade.
        capture_device: preferencias(&app).and_then(|p| p.capture().map(str::to_owned)),
        // A saída de som, pela mesma política e lida do mesmo lugar. Os dois
        // ajustes são independentes: um fone que ficou na outra sala não pode
        // custar o microfone que a pessoa escolheu.
        playback_device: preferencias(&app).and_then(|p| p.playback().map(str::to_owned)),
    };

    // `connect` blocks on a QUIC handshake. Running it on the async runtime's
    // worker would stall every other command until it finished or timed out.
    // O veredito volta daqui junto do handle: é o único lugar de onde ele pode
    // vir, porque quem se inscreve para ouvir eventos só tem o `Arc<Plug>`
    // depois que esta linha termina.
    let (plug, veredito) = tauri::async_runtime::spawn_blocking(move || Plug::connect(config))
        .await
        .map_err(|_| PlugError::Unreachable)??;

    plug.subscribe(Arc::new(Bridge { app }) as Arc<dyn EventListener>);
    let snapshot = plug.snapshot();

    if let Ok(mut slot) = session.plug.lock() {
        *slot = Some(plug);
    }

    // A metade invisível da lista de visitados: sem isto a seção da tela de
    // entrada ficaria permanentemente vazia. A política é a mesma que o `plug`
    // já escreveu em `crates/seele-tui/src/main.rs`.
    //
    // Registrado só **depois** de dar certo — guardar antes encheria a lista de
    // endereços errados digitados uma vez, que é o oposto de uma lista de
    // atalhos. E um Dogma hospedado aqui não entra: `127.0.0.1` não é lugar
    // aonde se volta, é o botão HOSPEDAR. O `plug` decide isso pela bandeira
    // `--hospedar`; aqui não há bandeira, e o endereço é o que sobrou para
    // dizer a mesma coisa.
    if !hospedado_aqui(&alvo) {
        if let Ok(mut lista) = seele_ffi::conhecidos::Conhecidos::abrir(
            std::path::PathBuf::from(&casa).join("conhecidos"),
        ) {
            // O Cage que já estava anotado, preservado. `registrar` reescreve a
            // entrada inteira, e este arquivo é compartilhado com o `plug`, que
            // grava em qual Cage a pessoa entrou e o lê de volta como padrão na
            // sua tela de seleção. Passar `None` daqui apagaria, a cada visita
            // pelo app, o que o terminal anotou.
            let cage = lista.buscar(&alvo).and_then(|conhecido| conhecido.cage);
            // Falhar em gravar um atalho não pode derrubar uma conversa que já
            // está de pé.
            if let Err(erro) = lista.registrar(&alvo, &apelido, cage) {
                tracing::warn!(%erro, "não guardei este Dogma na lista de visitados");
            }
        }
    }

    Ok(Entrada { snapshot, veredito })
}

/// Este endereço é a própria máquina?
///
/// Só o começo do texto, e não uma resolução de nome: a pergunta é sobre o que
/// a pessoa escolheu, não sobre para onde o pacote foi. `127.0.0.1:8383` é o que
/// o botão HOSPEDAR escreve no campo, e é exatamente esse caso que não pertence
/// a uma lista de lugares aonde se volta.
fn hospedado_aqui(alvo: &str) -> bool {
    alvo.starts_with("127.0.0.1") || alvo.starts_with("localhost") || alvo.starts_with("[::1]")
}

/// O que o app precisa saber depois de virar anfitrião.
#[derive(Debug, serde::Serialize)]
struct Anfitriao {
    /// Onde este app se conecta — no próprio computador.
    aqui: String,
    /// O link para mandar aos amigos. ADR 0006.
    convite: String,
    /// Até onde esse link chega: em qual degrau da escada do ADR 0022 parou.
    ///
    /// Nome estável e não frase, igual a [`FalhaAoHospedar`] e pelo mesmo
    /// motivo: a frase mora no `FRASES` do JavaScript. Os nomes são
    /// `PortaNoRoteador`, `Ipv6Direto` e `SoRedeLocal`, e os três já têm frase
    /// escrita lá.
    ///
    /// Por que isto cruza a fronteira: um link que só funciona na rede de casa e
    /// um link que funciona pela internet **são o mesmo texto**. Sem este campo
    /// o anfitrião manda o primeiro achando que mandou o segundo, e a descoberta
    /// acontece do outro lado, como "não conecta". O ADR 0022 pede que seja dito
    /// em vez de deixar a pessoa descobrindo sozinha.
    alcance: &'static str,
    /// Por que a porta não abriu no roteador, quando não abriu.
    ///
    /// Esta **é** uma frase pronta, e é a exceção consciente à regra acima: o
    /// texto vem do roteador (`RoteadorRecusou` carrega o que ele respondeu) e
    /// nenhuma lista fechada de frases cobriria o que cada modelo inventa. Vai
    /// para a tela como detalhe secundário, embaixo da frase que o `alcance`
    /// escolheu.
    porta_recusada: Option<String>,
}

/// Por que não deu para hospedar.
///
/// Enum, e não frase: `specs` põe a fronteira erro→texto no frontend, e uma
/// mensagem escrita aqui seria uma frase que nenhum tradutor alcança. As três
/// pedem coisas diferentes de quem está na frente da tela, e é por isso que são
/// três e não uma.
#[derive(Debug, serde::Serialize)]
enum FalhaAoHospedar {
    /// Já se está hospedando nesta janela.
    JaHospedando,
    /// A porta 8383 está ocupada — quase sempre outro SEELE aberto.
    PortaOcupada,
    /// Qualquer outro motivo para o Dogma não subir.
    NaoSubiu,
}

/// Sobe um Dogma dentro do app e devolve o link do convite.
///
/// Este comando é o item de UX que faltava: sem ele, hospedar exige abrir um
/// terminal, e num produto cujo argumento é "hospede você mesmo" isso exclui
/// justamente quem só quer clicar. O mesmo caminho do `plug --hospedar`, o
/// mesmo módulo, o mesmo Dogma.
///
/// Não conecta. Quem conecta é o `connect` de sempre, com o endereço que este
/// comando devolve — um caminho só para entrar num Dogma, hospedado aqui ou do
/// outro lado do mundo.
#[tauri::command]
async fn hospedar(
    app: AppHandle,
    session: State<'_, Session>,
) -> Result<Anfitriao, FalhaAoHospedar> {
    {
        let aberto = session
            .hospedagem
            .lock()
            .map_err(|_| FalhaAoHospedar::NaoSubiu)?;
        if aberto.is_some() {
            return Err(FalhaAoHospedar::JaHospedando);
        }
    }

    let banco = std::path::Path::new(&config_dir(&app)).join("dogma.db");
    let dogma = seele_server::hospedagem::Hospedagem::iniciar(
        PORTA_PADRAO,
        seele_server::casper::Location::File(banco),
        "Casa",
    )
    .await
    .map_err(|erro| classificar(&erro))?;

    let alcance = dogma.alcance();
    let anfitriao = Anfitriao {
        aqui: format!("127.0.0.1:{PORTA_PADRAO}"),
        convite: dogma.convite(),
        alcance: alcance.map_or("SoRedeLocal", |alcance| alcance.degrau.nome()),
        porta_recusada: alcance.and_then(|alcance| alcance.porta_recusada.clone()),
    };

    session
        .hospedagem
        .lock()
        .map_err(|_| FalhaAoHospedar::NaoSubiu)?
        .replace(dogma);

    Ok(anfitriao)
}

/// A porta em que um Dogma escuta por padrão.
const PORTA_PADRAO: u16 = 8383;

/// Separa "já tem um SEELE aberto" de todo o resto.
///
/// A distinção não é cosmética: porta ocupada tem conserto óbvio — fechar a
/// outra janela — e todo o resto não tem. Dizer "não subiu" nos dois casos
/// esconde a única falha que a pessoa consegue resolver sozinha.
fn classificar(erro: &anyhow::Error) -> FalhaAoHospedar {
    for causa in erro.chain() {
        if let Some(io) = causa.downcast_ref::<std::io::Error>() {
            if io.kind() == std::io::ErrorKind::AddrInUse {
                return FalhaAoHospedar::PortaOcupada;
            }
        }
    }
    FalhaAoHospedar::NaoSubiu
}

#[tauri::command]
async fn disconnect(session: State<'_, Session>) -> Result<(), ()> {
    let plug = session.plug.lock().ok().and_then(|mut slot| slot.take());
    // Dropping the handle is what ends the session; taking it out of the slot
    // is what makes the next `connect` allowed.
    drop(plug);

    // O convite morre com a sessão que ele abriu. Enquanto nada era conferido
    // isto era inerte; deixou de ser no momento em que `expected_fingerprint`
    // passou a sair daqui — quem sai, digita outro endereço e entra de novo
    // levaria a impressão prometida por um link anterior para um Dogma que
    // nunca a prometeu, e a recusa apareceria sem nada na tela que a explique.
    if let Ok(mut slot) = session.convite.lock() {
        *slot = None;
    }

    // Quem hospedava para de hospedar ao sair, e quem estava dentro é
    // derrubado. É o comportamento certo: o anfitrião fechou. `encerrar`
    // espera a porta voltar, para hospedar de novo em seguida funcionar.
    let dogma = session
        .hospedagem
        .lock()
        .ok()
        .and_then(|mut slot| slot.take());
    if let Some(dogma) = dogma {
        dogma.encerrar().await;
    }
    Ok(())
}

#[tauri::command]
fn snapshot(session: State<'_, Session>) -> Result<Snapshot, PlugError> {
    Ok(session.plug()?.snapshot())
}

/// A conversa da Linha aberta.
///
/// Fora do `snapshot` de propósito. Aquele é lido a cada quadro de interface, e
/// carregar a conversa junto fazia o custo crescer com a sessão — uma conversa
/// longa ficava lenta de escrever. A tela pede este quando o
/// `messages_revision` do snapshot muda, e só então.
#[tauri::command]
fn messages(session: State<'_, Session>) -> Result<Vec<seele_ffi::Message>, PlugError> {
    Ok(session.plug()?.messages())
}

#[tauri::command]
fn insert_plug(session: State<'_, Session>, cage: u32) -> Result<(), PlugError> {
    session.plug()?.insert_plug(cage)
}

#[tauri::command]
fn eject_plug(session: State<'_, Session>) -> Result<(), PlugError> {
    session.plug()?.eject_plug()
}

#[tauri::command]
fn open_line(session: State<'_, Session>, line: u32) -> Result<(), PlugError> {
    session.plug()?.open_line(line)
}

#[tauri::command]
fn send_message(session: State<'_, Session>, line: u32, body: String) -> Result<(), PlugError> {
    session.plug()?.send_message(line, body)
}

/// Pede ao Dogma que faça um Cage.
///
/// Devolve assim que o pedido entra na fila, e **não** quando a sala existe.
/// Quem responde isso é o Dogma, e a resposta chega pela mesma porta que todo o
/// resto: `ChannelsChanged` se ele fez, `NoticeRaised` com `PermissionDenied`
/// se recusou. A tela redesenha a lista pelo evento, como já faz quando alguém
/// entra num Cage — não há caminho novo a aprender.
///
/// A tela pode consultar `Snapshot::may_manage_cages` para decidir se mostra o
/// botão. Isso é conveniência: mandar o pedido sem ter a permissão não cria
/// nada, e a `specs/08-seguranca.md` põe a segurança nessa recusa e não no
/// botão escondido.
#[tauri::command]
fn criar_cage(
    session: State<'_, Session>,
    name: String,
    limit: u16,
    line: Option<u32>,
) -> Result<(), PlugError> {
    session.plug()?.create_cage(name, limit, line)
}

/// Pede ao Dogma que faça uma Linha.
#[tauri::command]
fn criar_linha(session: State<'_, Session>, name: String) -> Result<(), PlugError> {
    session.plug()?.create_line(name)
}

/// Pede ao Dogma que renomeie um Cage.
#[tauri::command]
fn renomear_cage(session: State<'_, Session>, cage: u32, name: String) -> Result<(), PlugError> {
    session.plug()?.rename_cage(cage, name)
}

/// Pede ao Dogma que renomeie uma Linha.
#[tauri::command]
fn renomear_linha(session: State<'_, Session>, line: u32, name: String) -> Result<(), PlugError> {
    session.plug()?.rename_line(line, name)
}

/// Pede ao Dogma que acabe com a sessão de alguém — `expulsar`.
///
/// Devolve quando o pedido entra na fila, e não quando a pessoa saiu. Quem
/// responde isso é o Dogma: `RosterChanged` quando ela sai de fato,
/// `NoticeRaised` com `PermissionDenied` quando ele recusa. É o mesmo caminho
/// dos verbos de sala, e não há porta nova a aprender.
///
/// A tela pode consultar `Snapshot::may_kick` para decidir se desenha o botão —
/// é o que faltava para o `EJETAR PLUG DO OPERADOR`, desenhado e desabilitado
/// desde o v2 porque não havia o que chamar. Isso é conveniência: mandar o
/// pedido sem a permissão não expulsa ninguém, e a `specs/08-seguranca.md` põe
/// a segurança nessa recusa e não no botão escondido.
#[tauri::command]
fn expulsar_piloto(session: State<'_, Session>, pilot: u64) -> Result<(), PlugError> {
    session.plug()?.kick_pilot(pilot)
}

/// Pede ao Dogma que impeça alguém de voltar — `banir`.
///
/// `expires_at` em segundos desde a época; `None` é para sempre. O `reason` é
/// para o registro de quem hospeda e nunca chega a quem foi banido.
#[tauri::command]
fn banir_piloto(
    session: State<'_, Session>,
    pilot: u64,
    reason: Option<String>,
    expires_at: Option<i64>,
) -> Result<(), PlugError> {
    session.plug()?.ban_pilot(pilot, reason, expires_at)
}

/// Pede ao Dogma que tire uma mensagem da Linha.
///
/// Sem permissão nenhuma quando a mensagem é de quem pede: a permissão do
/// `specs/04-servidor-seele.md` diz «de outra pessoa».
#[tauri::command]
fn remover_mensagem(session: State<'_, Session>, message: u64) -> Result<(), PlugError> {
    session.plug()?.remove_message(message)
}

/// Pede ao Dogma que mova alguém para um Cage — `mover_piloto`.
#[tauri::command]
fn mover_piloto(session: State<'_, Session>, pilot: u64, cage: u32) -> Result<(), PlugError> {
    session.plug()?.move_pilot(pilot, cage)
}

#[tauri::command]
fn set_at_field(session: State<'_, Session>, on: bool) -> Result<(), PlugError> {
    session.plug()?.set_at_field(on)
}

#[tauri::command]
fn set_total_isolation(session: State<'_, Session>, on: bool) -> Result<(), PlugError> {
    session.plug()?.set_total_isolation(on)
}

/// Push-to-talk, reported as it happens.
///
/// Not fallible on purpose: a key coming *up* must never be refused. Returning
/// an error here would give the frontend a path where the microphone was opened
/// and the close was rejected.
#[tauri::command]
fn set_talking(session: State<'_, Session>, talking: bool) {
    if let Ok(plug) = session.plug() {
        plug.set_talking(talking);
    }
}

#[tauri::command]
fn set_voice_mode(session: State<'_, Session>, mode: VoiceMode) -> Result<(), PlugError> {
    session.plug()?.set_voice_mode(mode);
    Ok(())
}

#[tauri::command]
fn set_volume(
    session: State<'_, Session>,
    nickname: String,
    percent: u16,
) -> Result<(), PlugError> {
    session.plug()?.set_volume(nickname, percent)
}

/// Os ajustes desta máquina, ou nada quando o disco não deixa lê-los.
///
/// `Option` e não `Result` porque nenhum chamador tem o que fazer com o motivo:
/// sem o arquivo, todo ajuste é o padrão, e é exatamente onde o app já estava
/// antes de o Terminal Dogma existir. A mesma política da lista de visitados.
fn preferencias(app: &AppHandle) -> Option<seele_ffi::preferences::Preferences> {
    seele_ffi::preferences::Preferences::open(
        std::path::PathBuf::from(config_dir(app)).join("preferences"),
    )
    .ok()
}

/// Os microfones que esta máquina está oferecendo agora.
///
/// Respondível sem sessão: escolher microfone é coisa que se faz antes de
/// conectar tanto quanto durante, e pendurar a lista num `Plug` vivo poria o
/// controle atrás da porta que ele existe para abrir.
///
/// Lista vazia significa que a máquina não quis enumerar — **não** que não há
/// microfone. Quem desenha "sem áudio" a partir disto está escrevendo a frase
/// errada; `snapshot.audio_available` é a que quer dizer isso.
#[tauri::command]
fn microfones() -> Vec<seele_ffi::CaptureDevice> {
    seele_ffi::capture_devices()
}

/// Qual microfone está escolhido, ou `None` para o padrão da máquina.
///
/// Vem do disco e não do `Snapshot`: são duas perguntas diferentes. Esta é "o
/// que foi escolhido", que tem resposta sem sessão nenhuma; `snapshot.capture`
/// é "o que abriu de verdade", que só existe com áudio de pé. As duas divergem
/// justamente quando importa — um dispositivo escolhido e desconectado.
#[tauri::command]
fn microfone_escolhido(app: AppHandle) -> Option<String> {
    preferencias(&app).and_then(|p| p.capture().map(str::to_owned))
}

/// Por que não deu para escolher esse microfone.
///
/// Enum, e não frase, pela mesma razão que [`FalhaAoHospedar`]: a fronteira
/// erro→texto do produto está no frontend, e uma mensagem escrita aqui seria uma
/// frase que nenhum tradutor alcança.
///
/// Duas e não uma porque pedem coisas diferentes de quem está na frente da tela.
/// Uma não tem conserto ali — o disco recusou —, e a outra tem: a lista está
/// logo acima, e o que sumiu entre desenhá-la e clicar nela pode ser trocado por
/// outro. Nenhuma delas é `PlugError::IdentityUnavailable`, que era o que este
/// comando devolvia: a frase daquela fala de identidade em disco, e acusar a
/// chave do piloto por causa de um arquivo de ajustes manda quem lê procurar no
/// lugar errado.
#[derive(Debug, serde::Serialize)]
enum FalhaAoEscolher {
    /// O ajuste não pôde ser gravado nesta máquina.
    NaoGravei,
    /// O microfone não está mais aqui.
    DispositivoSumiu,
}

/// Escolhe o microfone: grava no disco e, se houver sessão, troca agora.
///
/// As duas metades, e nesta ordem, porque falham de jeitos diferentes. A
/// escrita é o que faz a escolha valer amanhã; a troca é o que faz valer agora.
/// Se a troca falhar — o dispositivo saiu do lugar entre desenhar a lista e
/// clicar nela — a escolha continua gravada, e é a certa: quem religar a
/// interface volta ao microfone que queria sem escolher de novo.
///
/// Sem sessão não é falha. Escolher microfone na tela de entrada é o caminho
/// normal, e é para ela que a próxima conexão vai olhar.
#[tauri::command]
fn escolher_microfone(
    app: AppHandle,
    session: State<'_, Session>,
    dispositivo: Option<String>,
) -> Result<(), FalhaAoEscolher> {
    let Some(mut ajustes) = preferencias(&app) else {
        return Err(FalhaAoEscolher::NaoGravei);
    };
    if let Err(erro) = ajustes.set_capture(dispositivo.as_deref()) {
        tracing::warn!(%erro, "não consegui gravar o microfone escolhido");
        return Err(FalhaAoEscolher::NaoGravei);
    }

    let Ok(plug) = session.plug() else {
        // Sem sessão a escolha está gravada, e era tudo o que havia para fazer.
        return Ok(());
    };
    plug.set_capture_device(dispositivo).map_err(|erro| {
        tracing::warn!(%erro, "não consegui trocar o microfone da sessão");
        FalhaAoEscolher::DispositivoSumiu
    })
}

/// Por onde esta máquina está oferecendo tocar som agora.
///
/// O irmão de [`microfones`], respondível sem sessão pelo mesmo motivo. E a
/// mesma advertência sobre lista vazia: significa que a máquina não quis
/// enumerar, **não** que não há onde tocar.
#[tauri::command]
fn saidas() -> Vec<seele_ffi::PlaybackDevice> {
    seele_ffi::playback_devices()
}

/// Qual saída está escolhida, ou `None` para o padrão da máquina.
///
/// Do disco e não do `Snapshot`, pela mesma razão que [`microfone_escolhido`], e
/// aqui a diferença entre as duas perguntas é a única coisa visível: cair para o
/// alto-falante da máquina não faz barulho nenhum próprio. Quem escolheu um fone
/// e não ouve nada tem `snapshot.playback` — o que abriu — para comparar com
/// isto, o que foi pedido.
#[tauri::command]
fn saida_escolhida(app: AppHandle) -> Option<String> {
    preferencias(&app).and_then(|p| p.playback().map(str::to_owned))
}

/// Escolhe a saída de som: grava no disco e, se houver sessão, troca agora.
///
/// As duas metades e nesta ordem, pelas mesmas razões que [`escolher_microfone`]
/// dá. Uma diferença de peso está do outro lado, no core: trocar de saída não
/// desliga o Isolamento total. Quem mexe neste controle costuma ser exatamente
/// quem não está ouvindo nada, e às vezes o motivo de não ouvir é que se mutou —
/// uma troca que desmutasse calada poria um Dogma dentro de uma sala que estava
/// em silêncio.
#[tauri::command]
fn escolher_saida(
    app: AppHandle,
    session: State<'_, Session>,
    dispositivo: Option<String>,
) -> Result<(), FalhaAoEscolher> {
    let Some(mut ajustes) = preferencias(&app) else {
        return Err(FalhaAoEscolher::NaoGravei);
    };
    if let Err(erro) = ajustes.set_playback(dispositivo.as_deref()) {
        tracing::warn!(%erro, "não consegui gravar a saída de som escolhida");
        return Err(FalhaAoEscolher::NaoGravei);
    }

    let Ok(plug) = session.plug() else {
        // Sem sessão a escolha está gravada, e era tudo o que havia para fazer.
        return Ok(());
    };
    plug.set_playback_device(dispositivo).map_err(|erro| {
        tracing::warn!(%erro, "não consegui trocar a saída de som da sessão");
        FalhaAoEscolher::DispositivoSumiu
    })
}

/// Onde fica a lista de Dogmas visitados.
fn caminho_dos_conhecidos(app: &AppHandle) -> std::path::PathBuf {
    std::path::PathBuf::from(config_dir(app)).join("conhecidos")
}

/// Os Dogmas onde este piloto já esteve.
///
/// Uma lista de atalhos corrompida não pode fechar a porta: `specs/05` diz que
/// este arquivo é conveniência e pode ser apagado sem consequência. Por isso
/// falha vira lista vazia, e nunca erro — a tela esconde a seção e o formulário
/// de sempre continua ali.
#[tauri::command]
fn conhecidos(app: AppHandle) -> Vec<seele_ffi::conhecidos::Conhecido> {
    seele_ffi::conhecidos::Conhecidos::abrir(caminho_dos_conhecidos(&app))
        .map(|lista| lista.listar())
        .unwrap_or_default()
}

/// Tira um Dogma da lista.
///
/// Um `Ok` quando a lista nem abriu é a resposta certa e não uma mentira: o que
/// se pediu foi que aquele endereço não estivesse mais lá, e ele não está.
#[tauri::command]
fn esquecer(app: AppHandle, alvo: String) -> Result<(), ()> {
    let Ok(mut lista) = seele_ffi::conhecidos::Conhecidos::abrir(caminho_dos_conhecidos(&app))
    else {
        return Ok(());
    };
    lista.esquecer(&alvo).map_err(|_| ())
}

/// O que um `seele://` colado diz à tela de entrada.
///
/// Deliberadamente **não** é o `Convite` inteiro do core: o que a tela de
/// entrada tem o que fazer com é o endereço a preencher e o token a devolver no
/// `connect`. A impressão digital do link fica em [`Session::convite`], que é de
/// onde `connect` a tira para conferir — mandá-la para cá seria mandar ao
/// frontend a **entrada** de uma comparação, e é isso que
/// `specs/06-clientes-gui.md:19` proíbe. O que ele recebe sobre identidade é o
/// veredito de depois, em [`Entrada::veredito`].
///
/// Não há mais campo dizendo que a conferência ficou pendente porque não fica:
/// o link que traz `fp` é conferido no `connect` seguinte, e o resultado tem
/// frase própria na tela.
#[derive(Debug, serde::Serialize)]
struct ConviteLido {
    /// O endereço do Dogma, para o campo DOGMA.
    alvo: String,
    /// O convite de uso único, quando o link trouxe um.
    token: Option<String>,
}

/// Lê um `seele://` colado.
///
/// Mora aqui e não no JavaScript porque `specs/06:19` é inegociável, e porque um
/// segundo analisador de URI seria um segundo conjunto de casos de borda para
/// discordar do primeiro.
#[tauri::command]
fn analisar_convite(session: State<'_, Session>, link: String) -> Result<ConviteLido, String> {
    let convite =
        seele_ffi::uri::analisar(&link).map_err(|erro| nome_da_falha(&erro).to_owned())?;

    let lido = ConviteLido {
        alvo: convite.alvo.clone(),
        token: convite.token.clone(),
    };

    // Guardado inteiro deste lado da ponte. Ver o campo em `Session`.
    if let Ok(mut slot) = session.convite.lock() {
        *slot = Some(convite);
    }

    Ok(lido)
}

/// O nome estável de uma falha ao ler um convite.
///
/// Um `match` e não o `Display` do core. O `Display` de `ErroDeUri` hoje escreve
/// o nome da variante, mas isso é escolha do core, e a frase que a pessoa lê
/// está no `FRASES` do JavaScript amarrada a este nome — igual a todo o resto
/// dos erros, porque nenhuma mensagem para gente é escrita em Rust. Escrito
/// aqui, a próxima variante para a compilação em vez de virar uma tela que diz
/// o nome de uma variante em inglês.
fn nome_da_falha(erro: &seele_ffi::uri::ErroDeUri) -> &'static str {
    use seele_ffi::uri::ErroDeUri as Falha;
    match erro {
        Falha::EsquemaDesconhecido => "EsquemaDesconhecido",
        Falha::SemEndereco => "SemEndereco",
        Falha::EnderecoInvalido => "EnderecoInvalido",
        // Falta a frase em `ui/frases.js`, e de propósito: outro agente está
        // naquele diretório agora. Até ela chegar, `desconhecida()` mostra o
        // nome em vez de um beco sem saída — que é o que aquele fallback existe
        // para fazer. A frase a escrever é sobre pôr o IPv6 entre colchetes.
        Falha::EnderecoIpv6SemColchetes => "EnderecoIpv6SemColchetes",
        Falha::ImpressaoDigitalInvalida => "ImpressaoDigitalInvalida",
        Falha::TokenInvalido => "TokenInvalido",
        Falha::CageInvalido => "CageInvalido",
    }
}

/// O que o frontend precisa saber sobre a busca corrente.
#[derive(Debug, Clone, Default, serde::Serialize)]
struct BuscaEstado {
    /// Onde o termo casou, na ordem em que a tela desenha.
    casamentos: Vec<seele_ffi::search::Match>,
    /// A ocorrência em que o cursor está.
    atual: Option<seele_ffi::search::Match>,
    /// Qual ocorrência **dentro da própria mensagem** é a do cursor, de zero.
    ///
    /// Sem isto o app desenhava todo casamento igual, e o cursor sumia dentro de
    /// uma mensagem: em "sync caiu, o sync voltou, e o sync nem caiu" com o
    /// contador em `[2/3]`, apertar a próxima duas vezes mudava o algarismo e
    /// mais nada na tela. `atual` sozinho só alcança **qual mensagem**, que é
    /// por onde a tela rola; é este número que alcança **qual trecho**.
    ///
    /// Vem do core, de `Search::ordinal_in_message`, que existe para isto: a
    /// casca que pinta o histórico inteiro de uma vez conta as ocorrências de
    /// cada mensagem do mesmo jeito, e precisa deste índice para distinguir a
    /// do cursor das vizinhas. Contar do outro lado da ponte seria reescrever
    /// em JavaScript uma contagem que o Rust já faz — e que ele tem de fazer,
    /// porque é ela que decide o `[n/m]`.
    ordinal: Option<u32>,
    /// Posição a partir de 1, para desenhar "[1/3]". Zero quando não casou nada.
    posicao: u32,
    /// Quantas ao todo.
    total: u32,
}

impl BuscaEstado {
    fn de(busca: &seele_ffi::search::Search) -> Self {
        let (posicao, total) = busca.position();
        Self {
            // Todos, e não só o corrente: o app pinta o histórico inteiro de uma
            // vez, e acender só a ocorrência do cursor esconderia as outras que
            // estão na mesma tela.
            casamentos: busca.matches().to_vec(),
            atual: busca.current(),
            ordinal: busca
                .ordinal_in_message()
                .and_then(|ordinal| u32::try_from(ordinal).ok()),
            posicao: u32::try_from(posicao).unwrap_or(0),
            total: u32::try_from(total).unwrap_or(0),
        }
    }
}

/// Roda o termo sobre o histórico desta sessão.
///
/// Os corpos vão **crus**, e é isso que amarra os deslocamentos ao texto que a
/// tela desenha: `.mensagens .corpo` é `white-space: pre-wrap`, então esta
/// janela mostra quebra de linha e espaço duplo como eles chegaram. Normalizar
/// aqui erraria o alvo em toda mensagem com espaço colapsado, e achatar o
/// desenho para consertar isso seria trocar o produto pela implementação.
///
/// O `plug` faz o contrário — e está certo: `ui::wrap` já colapsa antes de
/// desenhar, então lá o texto na tela é o normalizado. Cada casca busca o que
/// desenha.
///
/// O custo, dito: com `' '` e `'\n'` sendo caracteres diferentes, um termo com
/// espaço não casa por cima de uma quebra de linha.
///
/// # O cursor atravessa a reconstrução
///
/// Este comando é chamado de novo a cada mensagem que chega, porque os índices
/// andam e um realce guardado passaria a acender o trecho errado. Uma busca
/// recém-construída começa na ocorrência um — e era isso que, numa Linha
/// movimentada, jogava quem estava na 7 de 12 de volta para a 1 e puxava o
/// painel junto, toda vez que alguém falava. Numa conversa viva não dava para
/// segurar a posição, que é exatamente onde buscar serve para alguma coisa.
///
/// `Search::resume_at` é a mesma regra que o `plug` já usava em
/// `App::refazer_busca`, e mora no core justamente para não haver duas. O
/// cursor continua deste lado da ponte, em `Session::busca`: o JavaScript não
/// decide nada sobre busca (`specs/06-clientes-gui.md:19`).
#[tauri::command]
fn buscar(session: State<'_, Session>, termo: String) -> Result<BuscaEstado, PlugError> {
    // `messages()` e não `snapshot()`: a conversa saiu do snapshot, que agora
    // carrega só a revisão dela. Aqui a lista inteira é mesmo necessária — é o
    // que se busca —, e este comando roda quando alguém digita, não a cada
    // quadro.
    let mensagens = session.plug()?.messages();
    let mut busca =
        seele_ffi::search::Search::new(mensagens.iter().map(|mensagem| &mensagem.body), &termo);

    let Ok(mut slot) = session.busca.lock() else {
        // Sem o cadeado não há cursor anterior a carregar nem onde guardar o
        // novo. A busca ainda vale; o que se perde é a continuidade.
        return Ok(BuscaEstado::de(&busca));
    };
    if let Some(anterior) = slot.as_ref().and_then(seele_ffi::search::Search::current) {
        busca.resume_at(anterior);
    }
    let estado = BuscaEstado::de(&busca);
    *slot = Some(busca);
    Ok(estado)
}

/// Anda uma ocorrência, dando a volta nas pontas.
#[tauri::command]
fn busca_andar(session: State<'_, Session>, adiante: bool) -> BuscaEstado {
    let Ok(mut slot) = session.busca.lock() else {
        return BuscaEstado::default();
    };
    let Some(busca) = slot.as_mut() else {
        return BuscaEstado::default();
    };
    if adiante {
        busca.next_match();
    } else {
        busca.previous_match();
    }
    BuscaEstado::de(busca)
}

/// Apaga a busca. Não falha: esvaziar o campo nunca pode dar erro.
#[tauri::command]
fn busca_limpar(session: State<'_, Session>) {
    if let Ok(mut slot) = session.busca.lock() {
        *slot = None;
    }
}

// ------------------------------------------------------------- atualização
//
// Duas metades, e a divisão é a decisão: `procurar_atualizacao` só olha, e
// `instalar_atualizacao` só instala o que já foi olhado. Nada aqui roda sozinho.
//
// **Não há consulta automática ao abrir.** Num produto cujo argumento é que o
// servidor é seu, um app que fala com o github.com a cada arranque sem ninguém
// pedir contradiz o argumento — e o que o parceiro pediu foi um *botão*, não uma
// vigilância. Quem quiser saber aperta; quem não apertar nunca sai daqui.
//
// ADR 0026 registra as duas assinaturas em jogo e por que são duas.

/// O que uma versão nova diz de si, antes de baixar um byte.
#[derive(Debug, serde::Serialize)]
struct VersaoNova {
    /// A versão que está sendo oferecida.
    versao: String,
    /// A que está rodando agora, para a tela poder escrever «de X para Y».
    instalada: String,
    /// As notas do release, quando o manifesto trouxe alguma.
    notas: Option<String>,
}

/// Quanto do pacote já veio.
///
/// `total` é `Option` porque o servidor pode não mandar `Content-Length`. Uma
/// tela que assumir cem por cento conhecido desenha uma barra que trava; sem o
/// total, o desenho certo é o indeterminado.
#[derive(Debug, Clone, serde::Serialize)]
struct Andamento {
    /// Bytes recebidos até agora.
    baixados: u64,
    /// Bytes ao todo, quando o servidor disse quantos são.
    total: Option<u64>,
}

/// Por que não deu para atualizar.
///
/// Enum e não frase, pela mesma razão de [`FalhaAoHospedar`]: a fronteira
/// erro→texto está no frontend. Seis e não uma porque pedem seis coisas
/// diferentes de quem está na frente da tela — de «não há nada a fazer, este app
/// não foi empacotado com atualizador» a «tente de novo daqui a pouco».
#[derive(Debug, serde::Serialize)]
enum FalhaAoAtualizar {
    /// Este app saiu sem chave de atualização, e por isso não atualiza.
    ///
    /// Não é defeito nem erro de rede: é um build feito antes de a chave do
    /// projeto existir, ou um build de quem compilou do código-fonte. A tela
    /// certa aqui é a que manda baixar da página de releases — o caminho que
    /// sempre existiu — e **não** a que manda tentar de novo.
    NaoConfigurado,
    /// Não deu para alcançar o manifesto, ou ele não era o que se esperava.
    NaoAlcancei,
    /// Há versão nova, mas não para este sistema ou esta arquitetura.
    SemPacoteParaEsteSistema,
    /// O pacote baixado **não** foi assinado com a chave deste projeto.
    ///
    /// A falha mais séria da lista, e a única que não é para tentar de novo. O
    /// canal de atualização é onde comprometer um produto é mais barato, e este
    /// é o ponto em que a tentativa é recusada. O pacote é descartado sem
    /// tocar em nada instalado.
    AssinaturaRecusada,
    /// O pacote chegou inteiro e conferido, e a troca dos arquivos falhou.
    NaoInstalei,
    /// Pediram para instalar sem ter procurado antes.
    NadaEscolhido,
}

/// Traduz a falha do plugin para o que a tela tem que dizer.
///
/// Um `match` e não o `Display` do plugin: aquelas mensagens são em inglês,
/// escritas para quem desenvolve, e algumas dizem «Updater does not have any
/// endpoints set», que não é frase para ninguém ler. Escrito aqui, a próxima
/// variante do plugin cai no ramo final em vez de virar texto cru na tela.
fn classificar_atualizacao(erro: &tauri_plugin_updater::Error) -> FalhaAoAtualizar {
    use tauri_plugin_updater::Error as Falha;
    match erro {
        // Sem endpoint não há de onde buscar, e isso é configuração de
        // empacotamento, igual à chave ausente. Mesma tela.
        Falha::EmptyEndpoints => FalhaAoAtualizar::NaoConfigurado,
        Falha::Reqwest(_)
        | Falha::Network(_)
        | Falha::ReleaseNotFound
        | Falha::Serialization(_) => FalhaAoAtualizar::NaoAlcancei,
        Falha::TargetNotFound(_)
        | Falha::TargetsNotFound(_)
        | Falha::UnsupportedOs
        | Falha::UnsupportedArch => FalhaAoAtualizar::SemPacoteParaEsteSistema,
        Falha::Minisign(_) | Falha::Base64(_) | Falha::SignatureUtf8(_) => {
            FalhaAoAtualizar::AssinaturaRecusada
        }
        // Tudo o mais é a troca dos arquivos dando errado: descompactar, mover,
        // permissão negada, o instalador do sistema recusando. O que sobrou de
        // comum a esses casos é que o app continua sendo o de antes.
        _ => FalhaAoAtualizar::NaoInstalei,
    }
}

/// Este app foi empacotado com uma chave de atualização?
///
/// A pergunta é sobre a `pubkey` do `plugins.updater`, e ela existe porque o
/// plugin **não** a confere na hora de procurar: uma chave vazia atravessa o
/// `check()` inteira e só falha lá na frente, ao conferir o pacote já baixado.
/// Sem esta conferência, um app sem chave ofereceria a atualização, gastaria o
/// download e recusaria no fim — três minutos para chegar à resposta que se
/// tinha antes de começar.
fn tem_chave_de_atualizacao(app: &AppHandle) -> bool {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|updater| updater.get("pubkey"))
        .and_then(|chave| chave.as_str())
        .is_some_and(|chave| !chave.trim().is_empty())
}

/// Pergunta ao release se há versão mais nova. **Não baixa nada.**
///
/// `Ok(None)` é a resposta boa e comum: já se está na última. A tela distingue
/// isso de falha porque são coisas diferentes — uma pede um «você está em dia», a
/// outra pede um motivo.
///
/// O que atravessa a rede aqui é um GET de um JSON pequeno. Enquanto ele
/// acontece o app segue inteiro: a consulta roda no runtime assíncrono, sem
/// tocar na sessão nem na janela, e um erro volta como valor.
#[tauri::command]
async fn procurar_atualizacao(
    app: AppHandle,
    session: State<'_, Session>,
) -> Result<Option<VersaoNova>, FalhaAoAtualizar> {
    use tauri_plugin_updater::UpdaterExt;

    if !tem_chave_de_atualizacao(&app) {
        return Err(FalhaAoAtualizar::NaoConfigurado);
    }

    let atualizador = app
        .updater()
        .map_err(|erro| classificar_atualizacao(&erro))?;
    let achado = atualizador
        .check()
        .await
        .map_err(|erro| classificar_atualizacao(&erro))?;

    let Some(nova) = achado else {
        // Em dia. O que estava guardado de uma consulta anterior morre aqui: se
        // ainda estivesse lá, um «instalar» seguinte instalaria uma versão que
        // esta consulta acabou de dizer que não é a de agora.
        if let Ok(mut slot) = session.atualizacao.lock() {
            *slot = None;
        }
        return Ok(None);
    };

    let resposta = VersaoNova {
        versao: nova.version.clone(),
        instalada: nova.current_version.clone(),
        notas: nova.body.clone(),
    };

    if let Ok(mut slot) = session.atualizacao.lock() {
        *slot = Some(nova);
    }

    Ok(Some(resposta))
}

/// Baixa, confere a assinatura, instala e reabre o app.
///
/// # O que acontece se falhar no meio
///
/// A ordem é a resposta, e ela é do plugin: o pacote inteiro é baixado **para a
/// memória** e a assinatura é conferida **antes** de qualquer arquivo instalado
/// ser tocado. Então uma queda de rede, um download truncado ou um pacote
/// adulterado terminam com o app exatamente como estava — não há meia
/// instalação possível nesses caminhos. A troca em disco, que é a parte
/// destrutiva, só começa depois de o pacote estar completo e conferido, e o
/// próprio plugin guarda o app anterior num diretório temporário para devolvê-lo
/// se a troca falhar no meio.
///
/// O que a tela precisa saber: qualquer variante de [`FalhaAoAtualizar`] que
/// volte daqui deixa uma janela viva e um SEELE utilizável. Não há estado
/// «quebrado pela metade» a explicar.
///
/// # Isto fecha e reabre o app
///
/// No Windows não há escolha: o instalador do NSIS não roda com o programa
/// aberto, então o plugin o dispara e encerra este processo — o `/R` do modo
/// passivo é o que reabre. Nos outros dois o processo continua vivo depois da
/// troca, mas rodando o código antigo que já está na memória, e reabrir é a
/// única forma de a atualização valer.
///
/// Uniformizado de propósito: uma ação que às vezes fecha a janela e às vezes
/// não é uma ação que ninguém consegue avisar direito. **A tela tem que dizer
/// antes que o SEELE vai fechar e abrir de novo** — e, se houver um Dogma
/// hospedado nesta janela, que quem estiver dentro dele cai junto.
#[tauri::command]
async fn instalar_atualizacao(
    app: AppHandle,
    session: State<'_, Session>,
) -> Result<(), FalhaAoAtualizar> {
    // Tirado do lugar, e não emprestado: instalar é uma vez. Se falhar, quem
    // quiser tentar de novo procura de novo — e a consulta nova é justamente o
    // que confirma que a versão ainda é aquela.
    let nova = session
        .atualizacao
        .lock()
        .ok()
        .and_then(|mut slot| slot.take())
        .ok_or(FalhaAoAtualizar::NadaEscolhido)?;

    let relator = app.clone();
    let mut baixados: u64 = 0;
    let pacote = nova
        .download(
            move |pedaco, total| {
                baixados = baixados.saturating_add(pedaco as u64);
                // Uma emissão perdida é uma barra que para de andar, e não uma
                // instalação perdida. Não vale derrubar o download por causa
                // dela.
                let _ = relator.emit(CANAL_DE_ATUALIZACAO, Andamento { baixados, total });
            },
            || {},
        )
        .await
        .map_err(|erro| classificar_atualizacao(&erro))?;

    // Daqui para baixo é a parte que mexe em disco.
    nova.install(pacote)
        .map_err(|erro| classificar_atualizacao(&erro))?;

    // Só se chega aqui fora do Windows: lá o `install` acima já encerrou este
    // processo. `restart` não volta, e é por isso que não há `Ok(())` depois.
    app.restart()
}

fn main() {
    // Marca de arranque. `specs/06-clientes-gui.md` aceita M5 com inicialização
    // abaixo de 2 s, e um critério que ninguém mede é um critério que passa a
    // valer o que a lembrança de alguém sobre "pareceu rápido" valer.
    let arranque = std::time::Instant::now();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "seele_app=info,seele_ffi=info,seele_core=info".into()),
        )
        .init();

    // A window that cannot open is not a case with a graceful path: there is
    // nowhere left to show the reason. It goes to the log and to the exit code.
    let started = tauri::Builder::default()
        // O atualizador. Registrado sempre, inclusive num build sem chave: o
        // plugin não fala com a rede por conta própria — quem o aciona são os
        // dois comandos lá em cima, e os dois recusam antes de sair da máquina
        // se a `pubkey` do `tauri.conf.json` estiver vazia.
        //
        // A chave e os endereços vêm do arquivo de configuração e não daqui.
        // Fixá-los em código faria um build de quem clonou o repositório
        // apontar para o nosso release sem que nada no repositório dissesse
        // isso — e é justamente o que o `tauri.conf.json` diz por escrito.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(Session::default())
        .setup(move |_app| {
            tracing::info!(millis = arranque.elapsed().as_millis(), "janela pronta");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            connect,
            hospedar,
            disconnect,
            snapshot,
            messages,
            insert_plug,
            eject_plug,
            open_line,
            send_message,
            criar_cage,
            criar_linha,
            renomear_cage,
            renomear_linha,
            expulsar_piloto,
            banir_piloto,
            remover_mensagem,
            mover_piloto,
            set_at_field,
            set_total_isolation,
            set_talking,
            set_voice_mode,
            set_volume,
            microfones,
            microfone_escolhido,
            escolher_microfone,
            saidas,
            saida_escolhida,
            escolher_saida,
            conhecidos,
            esquecer,
            analisar_convite,
            buscar,
            busca_andar,
            busca_limpar,
            procurar_atualizacao,
            instalar_atualizacao,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = started {
        tracing::error!(%error, "the desktop shell could not start");
        std::process::exit(1);
    }
}
