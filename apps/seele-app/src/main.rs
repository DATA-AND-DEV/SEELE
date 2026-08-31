//! The desktop client — a Tauri shell over [`seele_ffi`].
//!
//! `specs/06-clientes-gui.md` sets the shape of this file in one sentence:
//! "Nenhuma lógica de protocolo em JavaScript. Se o frontend precisa saber o
//! que é um `ssrc`, algo está errado." So the frontend gets a `Snapshot` and
//! sends back verbs — enter this voice room, say this, mute — and every one of them
//! is a call straight through to the FFI.
//!
//! Nothing here decides anything either. If a command in this file grows a
//! judgement, it belongs in `seele-core`, and the terminal client would have had
//! to grow the same one.
//!
//! # Threading
//!
//! [`seele_ffi::Connection::connect`] blocks, so it runs on a blocking thread. Events
//! arrive on the FFI's driver thread; [`Bridge`] is what marshals them onto the
//! webview, which is the "a casca marshala para sua thread de UI" the spec asks
//! for.

// A desktop shell with no window is not a desktop shell. The attribute keeps
// the console from opening behind the app on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// `specs/10-convencoes.md`: fora de teste não há `unwrap`/`expect`. Dentro, um
// `expect` com mensagem é mais legível que um `match` que só pode ir para um
// lado, e uma falha ali é falha do teste e não do produto. Mesma linha, mesma
// razão, que a de `seele-video`.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod icone;

use std::sync::{Arc, Mutex};

use seele_ffi::{
    ChannelWeight, ConnectConfig, ConnectFailure, Connection, ConnectionError, Event,
    EventListener, Preview, PreviewRules, Snapshot, VoiceMode,
};
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
    connection: Mutex<Option<Arc<Connection>>>,
    /// O servidor que este app está hospedando, quando está.
    ///
    /// Vive aqui e não numa variável local porque tem que sobreviver ao comando
    /// que o criou: o servidor fica de pé enquanto a janela estiver aberta.
    hospedagem: Mutex<Option<seele_server::hospedagem::Hospedagem>>,
    /// A busca corrente. O cursor é estado de sessão, e é o que impede a regra
    /// de dar-a-volta de ser reescrita em JavaScript.
    busca: Mutex<Option<seele_ffi::search::Search>>,
    /// O endereço com que esta sessão entrou, como a lista de visitados o
    /// conhece.
    ///
    /// Guardado porque a aparência do servidor — nome e imagem — só chega **depois**
    /// do aperto de mão, e quem a anota precisa saber sob qual chave. A
    /// alternativa era a tela devolver o endereço, e aí a chave da lista
    /// passaria a depender de o JavaScript normalizar a string do mesmo jeito
    /// que o Rust normalizou.
    ///
    /// `None` quando não há sessão, e também quando o servidor é hospedado aqui:
    /// esse não entra na lista de para-onde-voltar, e anotar sobre ele seria
    /// escrever numa entrada que não existe.
    alvo: Mutex<Option<String>>,
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
    /// O que mudou foi o outro lado: [`seele_ffi::Connection::connect`] devolve o
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
    fn connection(&self) -> Result<Arc<Connection>, ConnectionError> {
        self.connection
            .lock()
            .map_err(|_| ConnectionError::NotConnected)?
            .clone()
            .ok_or(ConnectionError::NotConnected)
    }
}

/// Carries FFI events onto the webview.
struct Bridge {
    app: AppHandle,
}

impl EventListener for Bridge {
    fn on_event(&self, event: Event) {
        // A failed emit means the window is gone, which is not worth a log channel
        // per event during shutdown.
        let _ = self.app.emit(EVENT_CHANNEL, &event);
    }
}

/// Where this client keeps its identity and its pins. ADR 0017.
///
/// The FFI takes a path because the shell knows where its platform keeps
/// configuration and the core knows how to persist an identity. `$SEELE_HOME`
/// comes first so the desktop app and `connection` can be told to be the same person —
/// which is what makes a session resumable between them.
fn config_dir(app: &AppHandle) -> String {
    if let Ok(home) = std::env::var("SEELE_HOME") {
        return home;
    }
    // The same `~/.config/seele` the terminal client uses, deliberately: two
    // clients on one machine should be one person unless told otherwise.
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
/// O veredito vem **junto** do `Snapshot`, e não por evento: `Connection::connect` só
/// devolve o `Arc<Connection>` depois de a identidade estar decidida, então uma casca
/// que se inscrevesse para ouvi-lo chegaria sempre tarde demais.
#[derive(Debug, serde::Serialize)]
struct Entrada {
    /// A tela inteira, como sempre.
    snapshot: Snapshot,
    /// O que a chave deste servidor acabou de ser.
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
) -> Result<Entrada, ConnectFailure> {
    if session.connection().is_ok() {
        return Err(ConnectionError::AlreadyConnected.into());
    }

    // O convite guardado vale para o servidor dele e para nenhum outro. Quem cola
    // um link e depois troca o endereço no campo deixaria para trás uma
    // confirmação de identidade que não é deste servidor — e agora que a FFI
    // sabe conferi-la, uma sobra dessas seria uma recusa que ninguém consegue
    // explicar.
    //
    // O que sobrevive ao descarte é o que vai conferir esta conexão.
    //
    // Os outros endereços do convite saem daqui pelo mesmo caminho e pela mesma
    // razão: eles pertencem ao servidor daquele link, e quem trocou o endereço no
    // campo está indo a outro lugar — tentar os alternativos do link anterior
    // seria bater à porta de um servidor que ninguém pediu.
    let (esperada, alternativos, bilhete) = match session.convite.lock() {
        Ok(mut slot) => {
            if slot.as_ref().is_some_and(|convite| convite.alvo != server) {
                *slot = None;
            }
            (
                slot.as_ref()
                    .and_then(|convite| convite.impressao_digital.clone()),
                slot.as_ref()
                    .map(|convite| convite.alternativos.clone())
                    .unwrap_or_default(),
                // Degrau 4 do ADR 0022. Sai daqui pela mesma porta e pela mesma
                // razão que os alternativos: o bilhete é do servidor **daquele**
                // link, e bater no ponto de encontro de outro seria apresentar
                // esta máquina a um anfitrião que ninguém pediu.
                slot.as_ref().and_then(|convite| convite.bilhete.clone()),
            )
        }
        // Sem o cadeado não há convite a ler. Entrar sem a confirmação é o
        // comportamento de quem digitou o endereço à mão, e é o pior que pode
        // acontecer aqui: nunca uma conferência contra um valor duvidoso.
        Err(_) => (None, Vec::new(), None),
    };

    let home = config_dir(&app);
    // Guardados antes de a configuração levar os originais para a outra thread:
    // a lista de visitados só é escrita lá embaixo, depois de a conexão existir.
    let alvo = server.clone();
    let apelido = nickname.clone();
    let casa = home.clone();
    let config = ConnectConfig {
        server,
        alternate_servers: alternativos,
        nickname,
        home,
        audio,
        join_secret: join_secret.filter(|s| !s.trim().is_empty()),
        expected_fingerprint: esperada,
        bilhete,
        // O microfone escolhido no Terminal servidor, lido do disco a cada
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
    //
    // O veredito volta daqui junto do handle, e continua sendo o único lugar de
    // onde ele pode vir: ele é decidido dentro do aperto de mão.
    //
    // **As etapas, não.** Elas acontecem durante esta linha, e é por isso que a
    // ponte entra por `connect_watching` em vez de por `subscribe` depois: o
    // comentário que estava aqui dizia que quem se inscreve só tem o
    // `Arc<Connection>` depois que esta linha termina, e isso continua verdade — o
    // que mudou é que a FFI passou a aceitar o ouvinte **antes** de bloquear.
    // Sem isso o `watch` da chegada não tinha um só leitor em produção.
    //
    // `Bridge` não depende do connection para nada: ele carrega o `AppHandle` e
    // reemite. Por isso um segundo, criado aqui, não é duplicação de estado —
    // é o mesmo destino, ligado mais cedo.
    let ponte = Arc::new(Bridge { app: app.clone() }) as Arc<dyn EventListener>;
    let atento = Arc::clone(&ponte);
    let (connection, veredito) =
        tauri::async_runtime::spawn_blocking(move || Connection::connect_watching(config, atento))
            .await
            .map_err(|_| ConnectFailure::from(ConnectionError::Unreachable))??;

    connection.subscribe(ponte);

    // O modo de microfone gravado, aplicado assim que há o que aplicar.
    //
    // Aqui e não num campo de `connect`: a sessão nasce no padrão da spec —
    // push-to-talk, microfone fechado — e só então abre para o que a pessoa
    // escolheu. A ordem é a segura; a inversa teria um instante em que um
    // microfone poderia estar aberto antes de alguém ter dito que podia.
    if let Some(modo) = preferencias(&app).and_then(|p| p.voice_mode()) {
        connection.set_voice_mode(VoiceMode::from(modo));
    }

    let snapshot = connection.snapshot();

    if let Ok(mut slot) = session.connection.lock() {
        *slot = Some(connection);
    }

    // A metade invisível da lista de visitados: sem isto a seção da tela de
    // entrada ficaria permanentemente vazia. A política é a mesma que o `connection`
    // já escreveu em `crates/seele-tui/src/main.rs`.
    //
    // Registrado só **depois** de dar certo — guardar antes encheria a lista de
    // endereços errados digitados uma vez, que é o oposto de uma lista de
    // atalhos. E um servidor hospedado aqui não entra: `127.0.0.1` não é lugar
    // aonde se volta, é o botão HOSPEDAR. O `connection` decide isso pela bandeira
    // `--hospedar`; aqui não há bandeira, e o endereço é o que sobrou para
    // dizer a mesma coisa.
    if !hospedado_aqui(&alvo) {
        if let Ok(mut guardado) = session.alvo.lock() {
            *guardado = Some(alvo.clone());
        }
        if let Ok(mut lista) = seele_ffi::conhecidos::Conhecidos::abrir(
            std::path::PathBuf::from(&casa).join("conhecidos"),
        ) {
            // A sala de voz que já estava anotado, preservado. `registrar` reescreve a
            // entrada inteira, e este arquivo é compartilhado com o `connection`, que
            // grava em qual sala de voz a pessoa entrou e o lê de volta como padrão na
            // sua tela de seleção. Passar `None` daqui apagaria, a cada visita
            // pelo app, o que o terminal anotou.
            let voice_room = lista
                .buscar(&alvo)
                .and_then(|conhecido| conhecido.voice_room);
            // Falhar em gravar um atalho não pode derrubar uma conversa que já
            // está de pé.
            if let Err(erro) = lista.registrar(&alvo, &apelido, voice_room) {
                tracing::warn!(%erro, "não guardei este servidor na lista de visitados");
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
    /// `PortaNoRoteador`, `FuroDeNat`, `Ipv6Direto`, `RedeLocalOuVpn` e
    /// `SoRedeLocal`, e os cinco já têm frase escrita lá.
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
    /// Por que o ponto de encontro não deu, quando ele chegou a ser tentado.
    ///
    /// Degrau 4 do ADR 0022, e o mesmo raciocínio do campo acima: é detalhe
    /// secundário, e é o que explica a quem hospeda por que sobrou um link de
    /// rede local numa casa em que o roteador também não abriu a porta. `None`
    /// quando ninguém pediu ponto de encontro nenhum — desligá-lo é uma escolha,
    /// não uma falha a explicar.
    encontro_recusado: Option<String>,
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
    /// Qualquer outro motivo para o servidor não subir.
    NaoSubiu,
}

/// Sobe um servidor dentro do app e devolve o link do convite.
///
/// Este comando é o item de UX que faltava: sem ele, hospedar exige abrir um
/// terminal, e num produto cujo argumento é "hospede você mesmo" isso exclui
/// justamente quem só quer clicar. O mesmo caminho do `connection --hospedar`, o
/// mesmo módulo, o mesmo server.
///
/// Não conecta. Quem conecta é o `connect` de sempre, com o endereço que este
/// comando devolve — um caminho só para entrar num servidor, hospedado aqui ou do
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

    let banco =
        seele_server::persistence::banco_do_cliente(std::path::Path::new(&config_dir(&app)));
    let server = seele_server::hospedagem::Hospedagem::iniciar(
        PORTA_PADRAO,
        seele_server::persistence::Location::File(banco),
        "Casa",
    )
    .await
    .map_err(|erro| classificar(&erro))?;

    // A porta se fecha no mesmo gesto de hospedar, antes do primeiro pacote —
    // ADR 0030. O `seeled` continua subindo aberto, porque quem digita `seeled`
    // aceitou cerimônia; quem apertou um botão não, e para ele o padrão é
    // perguntar.
    //
    // Semear, e não ligar: isto roda toda vez que a janela sobe um servidor, e um
    // interruptor que se rearma sozinho é um interruptor quebrado.
    {
        let banco = server.persistence();
        let mut persistence = banco.lock().await;
        if let Err(erro) = seele_server::portaria::semear_ligada(&mut persistence) {
            // Não impede de hospedar. Um servidor no ar com a portaria desligada é
            // o comportamento de antes deste ADR, e a tela diz em que estado a
            // porta está — que é a metade que faltava de verdade.
            tracing::warn!(%erro, "não consegui semear a portaria");
        }

        // E quem hospeda entra na própria casa.
        //
        // Sem isto a portaria tranca o dono para fora: o app conecta no servidor
        // que acabou de subir, o porteiro trata quem hospeda como desconhecido,
        // e o pedido fica esperando a decisão de alguém que não consegue entrar
        // para decidir. Foi o que aconteceu numa máquina de verdade, e é
        // deadlock no caminho principal do produto.
        //
        // Aqui e não na primeira conexão: se dependesse dela seria a mesma
        // corrida, porque a decisão precisa existir **antes** de haver alguém a
        // decidir.
        //
        // Falhar aqui é grave o bastante para não hospedar. Um servidor no ar em
        // que o dono não entra não é meio funcional — é uma janela travada com
        // uma frase pedindo que alguém decida.
        let minha = seele_ffi::impressao_desta_maquina(&config_dir(&app))
            .map_err(|_| FalhaAoHospedar::NaoSubiu)?;
        seele_server::portaria::admitir_o_dono(&mut persistence, &minha)
            .map_err(|_| FalhaAoHospedar::NaoSubiu)?;
    }

    let alcance = server.alcance();
    let anfitriao = Anfitriao {
        aqui: format!("127.0.0.1:{PORTA_PADRAO}"),
        convite: server.convite(),
        alcance: alcance.map_or("SoRedeLocal", |alcance| alcance.degrau().nome()),
        porta_recusada: alcance.and_then(|alcance| alcance.porta_recusada().map(str::to_owned)),
        encontro_recusado: alcance
            .and_then(|alcance| alcance.encontro_recusado().map(str::to_owned)),
    };

    session
        .hospedagem
        .lock()
        .map_err(|_| FalhaAoHospedar::NaoSubiu)?
        .replace(server);

    Ok(anfitriao)
}

/// A porta em que um servidor escuta por padrão.
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
    let connection = session
        .connection
        .lock()
        .ok()
        .and_then(|mut slot| slot.take());
    // Dropping the handle is what ends the session; taking it out of the slot
    // is what makes the next `connect` allowed.
    drop(connection);

    // O convite morre com a sessão que ele abriu. Enquanto nada era conferido
    // isto era inerte; deixou de ser no momento em que `expected_fingerprint`
    // passou a sair daqui — quem sai, digita outro endereço e entra de novo
    // levaria a impressão prometida por um link anterior para um servidor que
    // nunca a prometeu, e a recusa apareceria sem nada na tela que a explique.
    if let Ok(mut slot) = session.convite.lock() {
        *slot = None;
    }

    // Quem hospedava para de hospedar ao sair, e quem estava dentro é
    // derrubado. É o comportamento certo: o anfitrião fechou. `encerrar`
    // espera a porta voltar, para hospedar de novo em seguida funcionar.
    let server = session
        .hospedagem
        .lock()
        .ok()
        .and_then(|mut slot| slot.take());
    if let Some(server) = server {
        server.encerrar().await;
    }
    Ok(())
}

#[tauri::command]
fn snapshot(session: State<'_, Session>) -> Result<Snapshot, ConnectionError> {
    Ok(session.connection()?.snapshot())
}

/// A conversa da Linha aberta.
///
/// Fora do `snapshot` de propósito. Aquele é lido a cada quadro de interface, e
/// carregar a conversa junto fazia o custo crescer com a sessão — uma conversa
/// longa ficava lenta de escrever. A tela pede este quando o
/// `messages_revision` do snapshot muda, e só então.
#[tauri::command]
fn messages(session: State<'_, Session>) -> Result<Vec<seele_ffi::Message>, ConnectionError> {
    Ok(session.connection()?.messages())
}

#[tauri::command]
fn insert_plug(session: State<'_, Session>, voice_room: u32) -> Result<(), ConnectionError> {
    session.connection()?.insert_plug(voice_room)
}

#[tauri::command]
fn eject_plug(session: State<'_, Session>) -> Result<(), ConnectionError> {
    session.connection()?.eject_plug()
}

#[tauri::command]
fn open_channel(session: State<'_, Session>, channel: u32) -> Result<(), ConnectionError> {
    session.connection()?.open_channel(channel)
}

#[tauri::command]
fn send_message(
    session: State<'_, Session>,
    channel: u32,
    body: String,
) -> Result<(), ConnectionError> {
    session.connection()?.send_message(channel, body)
}

// ---------------------------------------------------------------- anexos
//
// ADR 0027. Quatro comandos, e o que **não** existe entre eles é a decisão: não
// há `abrir_anexo`. Nenhum cliente do SEELE abre arquivo, e este é o único
// ponto do desenho em que dá para ser estrito — então ele é estrito. Salvar é
// um ato de quem recebeu, num lugar que a pessoa escolheu, e o que acontece
// depois é com ela e com o sistema operacional dela.
//
// O quarto é o seletor, e ele entrou depois: o ADR 0027 tinha decidido que
// escolher um arquivo era arrastá-lo, e o primeiro dono a usar isto clicou no
// botão ARQUIVO esperando um seletor. A emenda do ADR conta o resto.

/// O que se sabe de um arquivo antes de mandá-lo.
#[derive(serde::Serialize)]
struct ArquivoEscolhido {
    /// O caminho nesta máquina. Nunca sai daqui: o servidor guarda o blob sob o
    /// hash do conteúdo, então nada deste caminho atravessa a rede.
    caminho: String,
    /// O nome que vai junto. **Não renomeado e sem extensão cortada** — o ADR
    /// 0027 é explícito: renomear `.exe` para parecer inofensivo faz o arquivo
    /// mentir, e mentir é a última coisa que ajuda aqui.
    nome: String,
    /// Quantos bytes.
    tamanho: u64,
    /// O tipo alegado, deduzido da extensão. **Alegação**, e é assim que ela
    /// atravessa: ninguém decide o que decodificar só por causa disto.
    tipo: String,
}

/// Lê nome, tamanho e tipo alegado de um arquivo que alguém escolheu.
///
/// Antes de mandar, para a tela poder mostrar o que vai e a pessoa poder
/// desistir. O tamanho é o que faz a barra ser barra: ele é sempre conhecido —
/// quem escolheu o arquivo sabe o tamanho dele —, então esta janela nunca
/// mostra um travessão no lugar de um andamento.
#[tauri::command]
fn descrever_arquivo(caminho: String) -> Result<ArquivoEscolhido, ConnectionError> {
    let alvo = std::path::PathBuf::from(&caminho);
    let meta = std::fs::metadata(&alvo).map_err(|_| ConnectionError::NotConnected)?;
    if !meta.is_file() {
        return Err(ConnectionError::NotConnected);
    }
    let nome = alvo
        .file_name()
        .map(|nome| nome.to_string_lossy().into_owned())
        .unwrap_or_else(|| "arquivo".to_owned());
    Ok(ArquivoEscolhido {
        tipo: tipo_alegado(&nome),
        caminho,
        nome,
        tamanho: meta.len(),
    })
}

/// Abre o seletor de arquivos do sistema e descreve o que a pessoa escolheu.
///
/// `Ok(None)` é desistir, e é o caso mais comum de todos: quem abre um seletor
/// fecha um seletor. Não é falha e não vira frase de erro em lugar nenhum.
///
/// **Fora da linha principal, e não por gosto.** O diálogo é modal e roda no
/// laço de eventos da janela; a versão bloqueante deste seletor trava esse laço
/// se for chamada de dentro dele, e é o que aconteceria num comando síncrono.
/// Então este é `async`, o seletor é o não-bloqueante, e a resposta volta por um
/// canal — a janela continua desenhando enquanto o diálogo está aberto.
///
/// Nenhum filtro de extensão. É a mesma decisão do `tipo_alegado` logo abaixo,
/// pelo mesmo motivo do ADR 0027: uma lista de extensões aqui esconderia
/// justamente o arquivo que alguém quer mandar, e um `rename` a contorna.
#[tauri::command]
async fn escolher_arquivo(app: AppHandle) -> Result<Option<ArquivoEscolhido>, ConnectionError> {
    use tauri_plugin_dialog::DialogExt as _;

    let (envia, mut recebe) = tauri::async_runtime::channel(1);
    app.dialog()
        .file()
        .set_title("Escolha um arquivo para anexar")
        .pick_file(move |escolha| {
            // `try_send` e não `blocking_send`: isto roda no laço de eventos da
            // janela, e o canal tem espaço para este único envio. Bloquear aqui
            // seria trocar um travamento por outro.
            let _ = envia.try_send(escolha);
        });

    let Some(Some(escolha)) = recebe.recv().await else {
        return Ok(None);
    };
    let Ok(caminho) = escolha.into_path() else {
        // Só o Android devolve `content://`, e este binário não roda lá. Um
        // caminho que não é caminho é recusado em vez de virar texto.
        return Err(ConnectionError::NotConnected);
    };
    descrever_arquivo(caminho.display().to_string()).map(Some)
}

/// O tipo que a extensão sugere.
///
/// Uma **alegação**, e o único uso dela é registro: o servidor a guarda como
/// alegação e nenhuma tela deste app decide o que desenhar por causa dela. Não
/// há lista de extensões proibidas, e não vai haver: o ADR 0027 explica por
/// que uma lista dessas é pior que lista nenhuma — contorna-se com um
/// `rename`, quebra mandar um build deste próprio projeto a um amigo, e faz o
/// que passou parecer conferido.
fn tipo_alegado(nome: &str) -> String {
    let extensao = nome
        .rsplit_once('.')
        .map(|(_, fim)| fim.to_ascii_lowercase());
    match extensao.as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("pdf") => "application/pdf",
        Some("txt" | "md") => "text/plain",
        Some("opus") => "audio/opus",
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("mp4") => "video/mp4",
        _ => "application/octet-stream",
    }
    .to_owned()
}

/// Manda um arquivo, num fluxo só dele.
///
/// Devolve a chave de idempotência da mensagem, na hora, porque a tela precisa
/// dela **agora**: é por ela que a barra encontra a própria subida entre os
/// eventos que chegam, e é ela que torna uma retentativa segura em vez de uma
/// segunda mensagem.
///
/// A mensagem só aparece na Linha quando os bytes chegam inteiros. Enquanto
/// sobe, quem a vê é só quem enviou — o custo que o ADR 0027 aceita para que
/// «ainda não chegou» e «expirou» nunca sejam duas ausências parecidas na
/// mesma tela.
#[tauri::command]
fn enviar_anexo(
    session: State<'_, Session>,
    channel: u32,
    body: String,
    caminho: String,
    nome: String,
    tipo: String,
) -> Result<u64, ConnectionError> {
    session
        .connection()?
        .send_attachment(channel, body, caminho, nome, tipo)
}

/// Salva um anexo onde quem recebeu escolheu.
///
/// O arquivo é marcado com a quarentena do próprio sistema ao ser gravado —
/// `com.apple.quarantine` no macOS, o fluxo `Zone.Identifier` no Windows —, que
/// é o que faz o Gatekeeper e o SmartScreen pararem o arquivo na frente de quem
/// for abri-lo. Não é antivírus: **este produto não varre vírus e não vai
/// varrer.** É a guarda que o sistema já tem, e que só funciona se quem grava a
/// acionar.
#[tauri::command]
fn salvar_anexo(
    session: State<'_, Session>,
    anexo: u64,
    destino: String,
) -> Result<(), ConnectionError> {
    session.connection()?.save_attachment(anexo, destino)
}

/// Baixa um anexo pequeno e diz se esta janela pode desenhá-lo.
///
/// A outra metade da regra do ADR 0027, e a que faltava: só uma lista curta de
/// tipos de imagem é desenhada embutida, e **só quando os bytes concordam com a
/// alegação**. O nome do arquivo e o tipo declarado são texto que a outra pessoa
/// escolheu; os primeiros bytes é que dizem o que a coisa é, e é deles que sai o
/// tipo de mídia que o `<img>` recebe.
///
/// **Prever não é abrir, e não é salvar.** Nada é gravado em disco em ponto
/// nenhum deste caminho — não há arquivo, não há caminho, não há marca de
/// quarentena a pôr, e nenhuma tela deste produto ganhou um botão que abre
/// arquivo. Salvar continua sendo o único verbo com destino, e continua tendo a
/// confirmação que diz em voz alta o que este produto não promete.
///
/// **Acontece ao apertar, nunca ao rolar.** O anexo está no servidor: ver é baixar.
/// Uma Linha que buscasse toda imagem enquanto a conversa rola transformaria o
/// teto de disco de quem hospeda em banda de todo mundo, uma vez por vez que
/// alguém abrisse a Linha.
#[tauri::command]
async fn prever_anexo(session: State<'_, Session>, anexo: u64) -> Result<Preview, ConnectionError> {
    let connection = session.connection()?;
    connection.preview_attachment(anexo).await
}

/// O que a tela precisa para decidir se oferece uma prévia.
///
/// Conveniência e não o limite: uma janela que pedisse assim mesmo receberia
/// `TooBig` ou `NotAPicture`, e a decisão continua sendo tomada no lado que
/// baixa e olha os bytes.
///
/// O número é do cliente e não do servidor de propósito. O teto por arquivo é uma
/// fração do teto de disco de quem hospeda e protege o disco **dele**; este
/// protege a memória de quem está lendo, que é outra máquina. E a lista dos
/// tipos vem daqui em vez de ser escrita de novo na página: duas cópias da
/// mesma lista discordam um dia, e a discordância seria a tela oferecer
/// desenhar o que a busca depois recusa.
#[tauri::command]
fn regras_de_previa() -> PreviewRules {
    Connection::preview_rules()
}

/// Onde os arquivos salvos vão parar, por padrão.
///
/// A pasta de downloads do sistema. Escrita inteira na tela antes de qualquer
/// botão de salvar, e essa é a parte que importa: sem um seletor de arquivos
/// nativo — que custaria um crate novo numa árvore que o ADR 0026 acabou de
/// contar — o lugar tem de estar **visível** em vez de suposto.
#[tauri::command]
fn pasta_de_downloads(app: AppHandle) -> String {
    use tauri::Manager as _;
    app.path()
        .download_dir()
        .or_else(|_| app.path().home_dir())
        .map(|pasta| pasta.display().to_string())
        .unwrap_or_default()
}

/// Pede ao servidor que faça uma sala de voz.
///
/// Devolve assim que o pedido entra na fila, e **não** quando a sala existe.
/// Quem responde isso é o servidor, e a resposta chega pela mesma porta que todo o
/// resto: `ChannelsChanged` se ele fez, `NoticeRaised` com `PermissionDenied`
/// se recusou. A tela redesenha a lista pelo evento, como já faz quando alguém
/// entra num sala de voz — não há caminho novo a aprender.
///
/// A tela pode consultar `Snapshot::may_manage_voice_rooms` para decidir se mostra o
/// botão. Isso é conveniência: mandar o pedido sem ter a permissão não cria
/// nada, e a `specs/08-seguranca.md` põe a segurança nessa recusa e não no
/// botão escondido.
#[tauri::command]
fn criar_voice_room(
    session: State<'_, Session>,
    name: String,
    limit: u16,
    channel: Option<u32>,
) -> Result<(), ConnectionError> {
    session
        .connection()?
        .create_voice_room(name, limit, channel)
}

/// Pede ao servidor que faça uma Linha.
#[tauri::command]
fn criar_linha(session: State<'_, Session>, name: String) -> Result<(), ConnectionError> {
    session.connection()?.create_channel(name)
}

/// Pede ao servidor que renomeie uma sala de voz.
#[tauri::command]
fn renomear_voice_room(
    session: State<'_, Session>,
    voice_room: u32,
    name: String,
) -> Result<(), ConnectionError> {
    session.connection()?.rename_voice_room(voice_room, name)
}

/// Pede ao servidor que renomeie uma Linha.
#[tauri::command]
fn renomear_linha(
    session: State<'_, Session>,
    channel: u32,
    name: String,
) -> Result<(), ConnectionError> {
    session.connection()?.rename_channel(channel, name)
}

// ------------------------------------------------- a cara e o nome do servidor
//
// O que quem hospeda personaliza: o nome que todo mundo lê no cabeçalho e a
// imagem ao lado dele. Cinco comandos, e nenhum deles decide nada — a
// permissão é conferida pelo PERMISSIONS no instante do verbo, e o que é uma
// imagem aceitável é conferido pelo próprio protocolo, dentro de
// `Connection::set_server_icon`.

/// Pede ao servidor que troque o próprio nome.
///
/// A tela pode consultar `Snapshot::may_customise_server` para decidir se
/// desenha o campo. Conveniência, como em `criar_voice_room`: quem pede sem a
/// permissão recebe `Alert`/`PermissionDenied` do servidor, e é lá que a
/// `specs/08-seguranca.md` põe a segurança — nunca no controle escondido.
#[tauri::command]
fn renomear_server(session: State<'_, Session>, name: String) -> Result<(), ConnectionError> {
    session.connection()?.rename_server(name)
}

/// O que a tela pode dizer sobre a imagem **antes** de alguém escolher uma.
///
/// Os dois números que o protocolo cobra, num lugar só, para que a frase da
/// tela não os traga escritos à mão. É a forma de `regras_de_previa`, pelo
/// mesmo motivo dela: duas cópias da mesma regra discordam um dia, e a
/// discordância aqui seria a tela prometendo aceitar o que o servidor recusa.
///
/// **Isto ainda é uma cópia**, e a nota é a dívida: os números de verdade são
/// `seele_proto::control::MAX_SERVER_ICON_LEN` e `MAX_SERVER_ICON_SIDE`, e o
/// ADR 0002 impede este binário de enxergá-los — ele vê `seele-ffi` e nada
/// além. `Connection::preview_rules()` existe justamente para não fazer isto com o
/// teto de prévia; falta o irmão dela, `Connection::server_icon_rules()`, e enquanto
/// ele não existe a cópia mora aqui, em Rust, onde uma linha a substitui.
///
/// O que a cópia **não** faz é julgar. Nenhum comando abaixo recusa uma imagem
/// por causa destes números: quem recusa é `Connection::set_server_icon`, com a
/// função do protocolo, e o número que a tela escreve no erro é o que o
/// `ConnectionError::IconTooBig` carrega. Se esta cópia envelhecer, a tela mostra
/// dois números diferentes — que é ruim, e ainda assim é melhor que uma casca
/// recusando em nome de uma regra que deixou de ser a regra.
#[tauri::command]
fn regras_do_icone_do_server() -> RegrasDoIcone {
    RegrasDoIcone {
        limite_bytes: TETO_DO_ICONE,
        lado: LADO_DO_ICONE,
    }
}

/// O que oferecer a quem clicou em compartilhar e não tem o módulo de vídeo.
///
/// `None` quando não há o que oferecer: o módulo já está lá, ou este sistema
/// não tem um publicado. A casca não mostra a caixa nos dois casos.
#[tauri::command]
fn modulo_de_video_a_baixar(app: AppHandle) -> Option<seele_ffi::ModuloAOferecer> {
    seele_ffi::modulo_de_video_a_baixar(&config_dir(&app))
}

/// Busca o módulo do Cisco e o instala, depois de a pessoa ter dito que sim.
///
/// **Só é chamado por um clique.** Nada aqui roda sozinho no arranque: baixar
/// um megabyte da rede é uma coisa que se pede, e o ADR 0026 já fixou essa
/// postura para o atualizador — que é, aliás, quem põe o `reqwest` nesta árvore,
/// então este comando não custa dependência nenhuma nova.
///
/// A conferência do hash não está aqui, e é de propósito: ela é do
/// `seele-video`, que é quem fixou o número, e assim ela é testável sem rede.
/// Esta função faz a única parte que não dá para testar sem rede — pedir os
/// bytes — e entrega o resto para quem sabe recusá-los.
#[tauri::command]
async fn baixar_modulo_de_video(app: AppHandle) -> Result<String, seele_ffi::ConnectionError> {
    let pasta = config_dir(&app);
    let Some(oferta) = seele_ffi::modulo_de_video_a_baixar(&pasta) else {
        // Já está instalado, ou não existe para este sistema. Nos dois casos não
        // há nada a buscar, e devolver erro seria pior: quem clicou duas vezes
        // veria uma falha por ter conseguido.
        return Ok(pasta);
    };

    let resposta = reqwest::get(&oferta.url).await.map_err(|erro| {
        tracing::warn!(%erro, url = %oferta.url, "não consegui pedir o módulo de vídeo");
        seele_ffi::ConnectionError::ScreenModuleRefused
    })?;
    if !resposta.status().is_success() {
        tracing::warn!(status = %resposta.status(), url = %oferta.url, "a origem do módulo recusou");
        return Err(seele_ffi::ConnectionError::ScreenModuleRefused);
    }
    let bytes = resposta.bytes().await.map_err(|erro| {
        tracing::warn!(%erro, "o download do módulo de vídeo não completou");
        seele_ffi::ConnectionError::ScreenModuleRefused
    })?;

    let caminho = seele_ffi::instalar_modulo_de_video(&pasta, &bytes)?;
    tracing::info!(caminho, "o módulo de vídeo está instalado");
    Ok(caminho)
}

/// Quanto pesa e quão grande é a imagem que o servidor aceita.
#[derive(Debug, Clone, Copy, serde::Serialize)]
struct RegrasDoIcone {
    /// O máximo de bytes.
    limite_bytes: u64,
    /// O máximo de pixels de cada lado.
    lado: u32,
}

/// Cópia do teto do protocolo. Veja `regras_do_icone_do_server`.
const TETO_DO_ICONE: u64 = 8 * 1024;

/// Cópia do lado máximo do protocolo. Veja `regras_do_icone_do_server`.
const LADO_DO_ICONE: u32 = 256;

/// Abre o seletor, lê o que a pessoa escolheu e o põe como imagem do servidor.
///
/// Um comando e não dois — escolher e aplicar — porque esta tela não tem
/// SALVAR: a escolha vale na hora, como a do microfone ao lado. `Ok(false)` é
/// desistir do seletor, que é o desfecho mais comum e não é falha nenhuma.
///
/// **Sem filtro de extensão**, pela razão do ADR 0027 que `escolher_arquivo` já
/// segue: um filtro esconde justamente o arquivo que a pessoa quer, e um
/// `rename` o contorna. O que separa uma imagem aceitável de outra é o
/// conteúdo, e quem o lê é o protocolo.
///
/// **Fora da linha principal**, como `escolher_arquivo` e pelo mesmo motivo: o
/// diálogo é modal e roda no laço de eventos da janela, e a versão bloqueante
/// trava esse laço se for chamada de dentro dele.
///
/// **A imagem escolhida é encolhida aqui**, e é o que faz este botão servir para
/// alguma coisa. O teto do protocolo é 8 KiB, e nenhuma imagem que uma pessoa
/// tem no computador tem 8 KiB: a versão anterior lia o arquivo, ouvia «não
/// cabe» do protocolo e devolvia isso à tela, o que na prática queria dizer
/// «arrume um PNG minúsculo por conta própria». Ver `icone::encolher`.
///
/// Lê no máximo `icone::TETO_DA_ORIGEM`, que é o teto do **arquivo escolhido** e
/// não o do ícone: sem um corte, escolher um vídeo de dois gigabytes seria
/// lê-lo inteiro para a memória antes de descobrir que não é imagem.
#[tauri::command]
async fn escolher_icone_do_server(
    app: AppHandle,
    session: State<'_, Session>,
) -> Result<bool, ConnectionError> {
    use std::io::Read as _;
    use tauri_plugin_dialog::DialogExt as _;

    let (envia, mut recebe) = tauri::async_runtime::channel(1);
    app.dialog()
        .file()
        .set_title("Escolha a imagem deste servidor")
        .pick_file(move |escolha| {
            let _ = envia.try_send(escolha);
        });

    let Some(Some(escolha)) = recebe.recv().await else {
        return Ok(false);
    };
    let Ok(caminho) = escolha.into_path() else {
        // Só o Android devolve `content://`, e este binário não roda lá.
        return Err(ConnectionError::IconNotAPicture);
    };

    let Ok(arquivo) = std::fs::File::open(&caminho) else {
        // Um arquivo que não abre não é uma imagem que este servidor possa usar, e
        // é a única frase honesta que esta casca tem: ela não sabe se o disco
        // sumiu ou se a permissão é de outra pessoa.
        return Err(ConnectionError::IconNotAPicture);
    };
    let mut bytes = Vec::new();
    if arquivo
        .take(icone::TETO_DA_ORIGEM.saturating_add(1))
        .read_to_end(&mut bytes)
        .is_err()
    {
        return Err(ConnectionError::IconNotAPicture);
    }

    // Fora da linha principal: uma foto de doze megapixels leva um tempo visível
    // para ser decodificada e reduzida, e fazer isso no laço de eventos congela
    // a janela inteira no meio de um gesto que parecia instantâneo.
    let Ok(Some(pronto)) =
        tauri::async_runtime::spawn_blocking(move || icone::encolher(&bytes)).await
    else {
        // Não é imagem, ou é uma que nem o último degrau fez caber. As duas
        // dizem a mesma coisa a quem escolheu: este arquivo não vira ícone.
        return Err(ConnectionError::IconNotAPicture);
    };

    session.connection()?.set_server_icon(Some(pronto))?;
    Ok(true)
}

/// Tira a imagem do servidor, deixando-o sem nenhuma.
///
/// Verbo próprio e não `escolher` com um argumento vazio: são duas coisas que
/// uma pessoa faz por motivos diferentes, e são dois botões na tela.
#[tauri::command]
fn tirar_icone_do_server(session: State<'_, Session>) -> Result<(), ConnectionError> {
    session.connection()?.set_server_icon(None)
}

/// Os bytes da imagem que está valendo, ou nada.
///
/// Fora do `Snapshot` de propósito, e a tela respeita o mesmo acordo: o
/// snapshot é lido duas vezes por segundo e atravessa a ponte em JSON, então
/// ele carrega `icon_revision` — um número — e a casca só vem buscar os bytes
/// quando o número anda. É o precedente de `messages_revision`.
#[tauri::command]
fn icone_do_server(session: State<'_, Session>) -> Result<Option<Vec<u8>>, ConnectionError> {
    Ok(session.connection()?.server_icon())
}

/// Pede ao servidor que acabe com a sessão de alguém — `expulsar`.
///
/// Devolve quando o pedido entra na fila, e não quando a pessoa saiu. Quem
/// responde isso é o servidor: `RosterChanged` quando ela sai de fato,
/// `NoticeRaised` com `PermissionDenied` quando ele recusa. É o mesmo caminho
/// dos verbos de sala, e não há porta nova a aprender.
///
/// A tela pode consultar `Snapshot::may_kick` para decidir se desenha o botão —
/// é o que faltava para o `EJETAR PLUG DO OPERADOR`, desenhado e desabilitado
/// desde o v2 porque não havia o que chamar. Isso é conveniência: mandar o
/// pedido sem a permissão não expulsa ninguém, e a `specs/08-seguranca.md` põe
/// a segurança nessa recusa e não no botão escondido.
#[tauri::command]
fn expulsar_pessoa(session: State<'_, Session>, person: u64) -> Result<(), ConnectionError> {
    session.connection()?.kick_person(person)
}

/// Pede ao servidor que impeça alguém de voltar — `banir`.
///
/// `expires_at` em segundos desde a época; `None` é para sempre. O `reason` é
/// para o registro de quem hospeda e nunca chega a quem foi banido.
#[tauri::command]
fn banir_pessoa(
    session: State<'_, Session>,
    person: u64,
    reason: Option<String>,
    expires_at: Option<i64>,
) -> Result<(), ConnectionError> {
    session.connection()?.ban_person(person, reason, expires_at)
}

/// Pede ao servidor que tire uma mensagem da Linha.
///
/// Sem permissão nenhuma quando a mensagem é de quem pede: a permissão do
/// `specs/04-servidor-seele.md` diz «de outra pessoa».
#[tauri::command]
fn remover_mensagem(session: State<'_, Session>, message: u64) -> Result<(), ConnectionError> {
    session.connection()?.remove_message(message)
}

/// Pede ao servidor que mova alguém para uma sala de voz — `mover_pessoa`.
#[tauri::command]
fn mover_pessoa(
    session: State<'_, Session>,
    person: u64,
    voice_room: u32,
) -> Result<(), ConnectionError> {
    session.connection()?.move_person(person, voice_room)
}

/// Pede ao servidor que destrua uma sala de voz — `apagar_voice_room`.
///
/// Quem estiver dentro é posto para fora e avisado; a Linha presa a ele, se
/// houver, fica onde está. O servidor recusa o último sala de voz e diz isso com
/// `LastVoiceRoom`, que é frase diferente da de entrada recusada.
///
/// A tela pode consultar `Snapshot::may_delete_rooms` para decidir se desenha o
/// controle — e é campo próprio, não `may_manage_voice_rooms`: fazer sala e destruir
/// sala são permissões diferentes na `specs/04-servidor-seele.md`, e é preciso
/// poder oferecer uma sem a outra. Isso é conveniência; quem nega é o servidor.
#[tauri::command]
fn apagar_voice_room(session: State<'_, Session>, voice_room: u32) -> Result<(), ConnectionError> {
    session.connection()?.delete_voice_room(voice_room)
}

/// Pede ao servidor que destrua uma Linha, e tudo que foi escrito nela —
/// `apagar_linha`.
#[tauri::command]
fn apagar_linha(session: State<'_, Session>, channel: u32) -> Result<(), ConnectionError> {
    session.connection()?.delete_channel(channel)
}

/// Pergunta quanto custaria destruir uma Linha. Não destrói nada.
///
/// O único comando desta janela que **espera** o servidor responder, e a razão é a
/// frase que ele alimenta: a caixa de confirmação promete um número exato de
/// mensagens, de gente e uma data, e os três são contados no banco no instante
/// de perguntar. A janela segura uma página de histórico e chutaria para baixo
/// por todo o passado da Linha — e um número quase certo numa caixa que promete
/// destruição é pior que nenhum.
///
/// Por isso `async`: um comando síncrono do Tauri roda na thread principal, e
/// esperar ali travaria a janela inteira enquanto a pergunta atravessa a rede.
///
/// Quem não conseguir resposta **não abre a caixa**. Não há versão honesta dela
/// sem os três números.
#[tauri::command]
async fn peso_da_linha(
    session: State<'_, Session>,
    channel: u32,
) -> Result<ChannelWeight, ConnectionError> {
    // O `Arc` sai do cadeado antes do `await`, e é de propósito: `Session::connection`
    // devolve um clone justamente para que nada segure o `Mutex` atravessando um
    // ponto de espera.
    let connection = session.connection()?;
    connection.weigh_channel(channel).await
}

#[tauri::command]
fn set_muted(session: State<'_, Session>, on: bool) -> Result<(), ConnectionError> {
    session.connection()?.set_muted(on)
}

#[tauri::command]
fn set_total_isolation(session: State<'_, Session>, on: bool) -> Result<(), ConnectionError> {
    session.connection()?.set_total_isolation(on)
}

/// Push-to-talk, reported as it happens.
///
/// Not fallible on purpose: a key coming *up* must never be refused. Returning
/// an error here would give the frontend a path where the microphone was opened
/// and the close was rejected.
#[tauri::command]
fn set_talking(session: State<'_, Session>, talking: bool) {
    if let Ok(connection) = session.connection() {
        connection.set_talking(talking);
    }
}

/// Escolhe como o microfone abre: grava no disco e, se houver sessão, aplica agora.
///
/// **As duas metades, nesta ordem**, pelo mesmo argumento que
/// [`escolher_microfone`] escreve por extenso: a escrita é o que faz a escolha
/// valer amanhã, a aplicação é o que faz valer agora.
///
/// Antes daqui só existia a segunda, e o preço tinha duas faces. Quem escolhia
/// voz achava push-to-talk de volta no dia seguinte — e o padrão é
/// push-to-talk *porque nunca dispara sozinho*, argumento que vale para quem
/// não escolheu e não para quem escolheu. E, por exigir sessão, não dava para
/// escolher o modo na tela de entrada; agora dá.
///
/// # Errors
///
/// [`FalhaAoEscolher::NaoGravei`] se o disco recusar. Sem sessão **não** é
/// falha: a escolha ficou gravada, que era tudo o que havia para fazer.
#[tauri::command]
fn set_voice_mode(
    app: AppHandle,
    session: State<'_, Session>,
    mode: VoiceMode,
) -> Result<(), FalhaAoEscolher> {
    let Some(mut ajustes) = preferencias(&app) else {
        return Err(FalhaAoEscolher::NaoGravei);
    };
    if let Err(erro) = ajustes.set_voice_mode(Some(mode.into())) {
        tracing::warn!(%erro, "não consegui gravar o modo de voz escolhido");
        return Err(FalhaAoEscolher::NaoGravei);
    }
    if let Ok(connection) = session.connection() {
        connection.set_voice_mode(mode);
    }
    Ok(())
}

/// Qual modo de microfone está escolhido, ou `None` para o padrão da spec.
///
/// Do disco e não do `Snapshot`, pela mesma razão que [`microfone_escolhido`]:
/// esta pergunta tem resposta sem sessão nenhuma, e é a que a tela de entrada
/// precisa fazer.
#[tauri::command]
fn modo_de_voz_escolhido(app: AppHandle) -> Option<VoiceMode> {
    preferencias(&app)
        .and_then(|p| p.voice_mode())
        .map(VoiceMode::from)
}

/// Qual tecla abre o microfone em push-to-talk, ou `None` para a barra de espaço.
///
/// O valor é um `KeyboardEvent.code` e **atravessa opaco**: este lado nunca
/// decide o que uma tecla significa, só lembra qual foi escolhida. Quem lê
/// teclado é a casca, e é o único lugar que pode nomeá-las.
#[tauri::command]
fn tecla_de_falar(app: AppHandle) -> Option<String> {
    preferencias(&app).and_then(|p| p.push_to_talk_key().map(str::to_owned))
}

/// Escolhe a tecla que abre o microfone. `None` volta para a barra de espaço.
///
/// Só grava: não há nada a aplicar numa sessão viva, porque quem lê a tecla é a
/// casca e ela relê esta preferência quando muda.
///
/// # Errors
///
/// [`FalhaAoEscolher::NaoGravei`] se o disco recusar.
#[tauri::command]
fn escolher_tecla_de_falar(app: AppHandle, tecla: Option<String>) -> Result<(), FalhaAoEscolher> {
    let Some(mut ajustes) = preferencias(&app) else {
        return Err(FalhaAoEscolher::NaoGravei);
    };
    ajustes
        .set_push_to_talk_key(tecla.as_deref())
        .map_err(|erro| {
            tracing::warn!(%erro, "não consegui gravar a tecla de falar");
            FalhaAoEscolher::NaoGravei
        })
}

#[tauri::command]
fn set_volume(
    session: State<'_, Session>,
    nickname: String,
    percent: u16,
) -> Result<(), ConnectionError> {
    session.connection()?.set_volume(nickname, percent)
}

// ------------------------------------------------------- compartilhar a tela
//
// Seis comandos e nenhuma decisão, como todo o resto deste arquivo. O teto de
// verdade é do core e tem de continuar lá: a decisão de 22/08 do
// `docs/superpowers/specs/2026-08-22-compartilhamento-de-tela-design.md` (§5.1)
// o escreve como
//
//     min(caminho de quem HOSPEDA × 60% ÷ N espectadores,
//         caminho de quem COMPARTILHA × 60%,
//         a escolha da pessoa)
//
// e as duas primeiras linhas são medidas que esta casca não tem e não deve
// tentar refazer. O que atravessa daqui é só a terceira — a escolha —, e ela é
// **teto**: `Snapshot::tela` volta dizendo o que de fato está saindo, e é isso
// que a tela desenha ao lado do que foi pedido.

/// As telas e janelas que esta máquina pode transmitir.
///
/// Uma lista vazia não é falha: é a resposta quando o sistema recusou, e
/// [`permissao_de_tela`] é quem diz qual foi a recusa. Por isso os dois são
/// comandos separados e a janela chama os dois — uma lista vazia sem motivo é
/// um beco.
#[tauri::command]
fn fontes_de_tela(
    session: State<'_, Session>,
) -> Result<Vec<seele_ffi::FonteDeTela>, ConnectionError> {
    session.connection()?.fontes_de_tela()
}

/// O que o sistema operacional respondeu sobre gravar a tela.
#[tauri::command]
fn permissao_de_tela(
    session: State<'_, Session>,
) -> Result<seele_ffi::PermissaoDeTela, ConnectionError> {
    Ok(session.connection()?.permissao_de_tela())
}

/// Pede a permissão ao sistema.
///
/// Só por aperto de botão, e nunca ao abrir a caixa: no macOS isto abre o
/// alerta do TCC, e um alerta de sistema que aparece sem ninguém ter pedido é
/// o que ensina a pessoa a recusar por reflexo — e o TCC não pergunta duas
/// vezes.
#[tauri::command]
fn pedir_permissao_de_tela(
    session: State<'_, Session>,
) -> Result<seele_ffi::PermissaoDeTela, ConnectionError> {
    Ok(session.connection()?.pedir_permissao_de_tela())
}

/// Começa a transmitir a fonte escolhida, com os limites escolhidos.
///
/// `ConnectionError::ScreenShareTaken` quando alguém já está compartilhando nesta
/// sala. Não é permissão que falta — a pessoa pode compartilhar assim que o
/// outro parar —, e é por isso que a frase dela em `ui/frases.js` manda esperar
/// em vez de mandar procurar um papel que ela já tem.
#[tauri::command]
fn compartilhar_tela(
    session: State<'_, Session>,
    fonte: u64,
    limites: seele_ffi::LimitesDeTela,
) -> Result<(), ConnectionError> {
    session.connection()?.compartilhar_tela(fonte, limites)
}

/// Para de transmitir. Idempotente do outro lado: parar sem estar
/// compartilhando não é erro.
#[tauri::command]
fn parar_de_compartilhar(session: State<'_, Session>) -> Result<(), ConnectionError> {
    session.connection()?.parar_de_compartilhar()
}

/// Muda os limites no meio da transmissão, sem cortá-la.
///
/// Comando próprio e não um `compartilhar_tela` de novo: recomeçar a
/// transmissão para trocar um teto piscaria a imagem de todo mundo que está
/// assistindo por causa de um controle mexido por uma pessoa só.
#[tauri::command]
fn ajustar_limites_da_tela(
    session: State<'_, Session>,
    limites: seele_ffi::LimitesDeTela,
) -> Result<(), ConnectionError> {
    session.connection()?.ajustar_limites_da_tela(limites)
}

/// Põe a janela em tela cheia, ou a tira de lá.
///
/// A janela do sistema **e** o corte da interface, e os dois são precisos: sem
/// a janela sobra a barra de título e a moldura do sistema em volta da imagem;
/// sem o corte, a interface inteira cresce junto e a tela compartilhada continua
/// dividindo espaço com o roster e os botões. Quem corta a interface é o CSS,
/// que responde ao `data-cinema`; esta metade é a janela.
///
/// Ignora a falta da janela em vez de reclamar: chamar isto durante o
/// fechamento não é erro de ninguém.
#[tauri::command]
fn tela_cheia(app: AppHandle, ligada: bool) {
    // Pelo rótulo, e por qualquer uma se o rótulo não achar. O `main` é o que o
    // Tauri dá quando o `tauri.conf.json` não nomeia a janela — e ele não a
    // nomeia. Depender de um padrão que não está escrito em lugar nenhum é o
    // tipo de coisa que some numa atualização do Tauri sem ninguém notar, e o
    // sintoma seria este botão parando de funcionar em silêncio.
    let janela = app
        .get_webview_window("main")
        .or_else(|| app.webview_windows().into_values().next());
    let Some(janela) = janela else {
        tracing::debug!(ligada, "não há janela para pôr em tela cheia");
        return;
    };
    if let Err(erro) = janela.set_fullscreen(ligada) {
        tracing::debug!(%erro, ligada, "a janela não trocou de modo");
    }
}

/// Anota o nome e a imagem do servidor na lista de visitados.
///
/// # Por que a tela chama isto, e não o Rust sozinho
///
/// Porque o momento é da tela. A aparência chega num quadro que o servidor manda
/// **depois** do aperto de mão, e `Connection::connect` já voltou quando ele chega —
/// é a mesma janela cega que faz o cabeçalho precisar sincronizar o ícone à mão
/// ao entrar. A tela é quem sabe que já sincronizou; daqui não dá para saber
/// sem inventar um prazo.
///
/// Sem argumentos de propósito: o endereço vem do que a sessão guardou, e o
/// nome e a imagem vêm da `Connection`. Uma tela que passasse os três poderia passar
/// os três de outro servidor.
#[tauri::command]
fn lembrar_aparencia_do_servidor(app: AppHandle, session: State<'_, Session>) {
    let Ok(Some(alvo)) = session.alvo.lock().map(|guardado| guardado.clone()) else {
        return;
    };
    let Ok(connection) = session.connection() else {
        return;
    };
    let nome = connection.snapshot().server;
    let icone = connection.server_icon();

    let Ok(mut lista) = seele_ffi::conhecidos::Conhecidos::abrir(caminho_dos_conhecidos(&app))
    else {
        return;
    };
    if let Err(erro) = lista.anotar_aparencia(&alvo, Some(&nome), icone.as_deref()) {
        // Um distintivo que não gravou é uma lista sem enfeite, e não uma falha
        // que valha interromper quem está entrando numa conversa.
        tracing::debug!(%erro, "não anotei a aparência deste servidor");
    }
}

/// O que o sistema deixa este app fazer com o microfone.
///
/// Sem sessão, de propósito: quem desconfia que está mudo quer a resposta antes
/// de entrar, e a tela de entrada não tem `Connection` nenhum para perguntar.
#[tauri::command]
fn permissao_de_microfone() -> seele_ffi::PermissaoDeMicrofone {
    seele_ffi::permissao_de_microfone()
}

/// Abre a página de privacidade do microfone do sistema.
///
/// # Por que abrir os Ajustes é o máximo que dá para fazer
///
/// Porque no Windows **não existe pedido a fazer**. Um app empacotado tem
/// prompt de consentimento e API; um app de área de trabalho — que é o que este
/// é, e o que o Discord também é — não tem nem um nem outro. O interruptor é da
/// pessoa, e o que este produto pode fazer é levá-la até ele em vez de deixá-la
/// procurar numa página longa.
///
/// `ms-settings:` é o esquema do próprio Windows, e não um caminho de arquivo:
/// ele abre a seção certa dos Ajustes.
///
/// Sem barulho quando falha: um botão de conveniência que não abriu não é
/// motivo para uma segunda mensagem de erro em cima da primeira.
#[tauri::command]
fn abrir_ajustes_do_microfone(app: AppHandle) {
    let _ = app;
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", "ms-settings:privacy-microphone"])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
            .spawn();
    }
}

/// Fecha o aviso que está na tela, de verdade.
///
/// A janela escondia a caixa por conta própria, e o redesenho — que roda duas
/// vezes por segundo a partir do `Snapshot` — a trazia de volta. Apagar uma sala
/// com alguém dentro deixava a janela coberta por um alerta que não fechava.
#[tauri::command]
fn dispensar_aviso(session: State<'_, Session>) -> Result<(), ConnectionError> {
    session.connection()?.dispensar_aviso();
    Ok(())
}

/// Os ajustes desta máquina, ou nada quando o disco não deixa lê-los.
///
/// `Option` e não `Result` porque nenhum chamador tem o que fazer com o motivo:
/// sem o arquivo, todo ajuste é o padrão, e é exatamente onde o app já estava
/// antes de o Terminal servidor existir. A mesma política da lista de visitados.
fn preferencias(app: &AppHandle) -> Option<seele_ffi::preferences::Preferences> {
    seele_ffi::preferences::Preferences::open(
        std::path::PathBuf::from(config_dir(app)).join("preferences"),
    )
    .ok()
}

/// Os microfones que esta máquina está oferecendo agora.
///
/// Respondível sem sessão: escolher microfone é coisa que se faz antes de
/// conectar tanto quanto durante, e pendurar a lista num `Connection` vivo poria o
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
/// outro. Nenhuma delas é `ConnectionError::IdentityUnavailable`, que era o que este
/// comando devolvia: a frase daquela fala de identidade em disco, e acusar a
/// chave do pessoa por causa de um arquivo de ajustes manda quem lê procurar no
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

    let Ok(connection) = session.connection() else {
        // Sem sessão a escolha está gravada, e era tudo o que havia para fazer.
        return Ok(());
    };
    connection.set_capture_device(dispositivo).map_err(|erro| {
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
/// uma troca que desmutasse calada poria um servidor dentro de uma sala que estava
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

    let Ok(connection) = session.connection() else {
        // Sem sessão a escolha está gravada, e era tudo o que havia para fazer.
        return Ok(());
    };
    connection.set_playback_device(dispositivo).map_err(|erro| {
        tracing::warn!(%erro, "não consegui trocar a saída de som da sessão");
        FalhaAoEscolher::DispositivoSumiu
    })
}

/// Onde fica a lista de servidores visitados.
fn caminho_dos_conhecidos(app: &AppHandle) -> std::path::PathBuf {
    std::path::PathBuf::from(config_dir(app)).join("conhecidos")
}

/// Os servidores onde este pessoa já esteve.
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

/// Tira um servidor da lista.
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
    /// O endereço do servidor, para o campo SERVER.
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
        // Degrau 4 do ADR 0022: o `enc` do convite veio pela metade ou com um
        // endereço que não é um. Nome próprio porque a frase é própria — meio
        // bilhete não leva a lugar nenhum, e o resto do link continua bom.
        Falha::BilheteInvalido => "BilheteInvalido",
        Falha::ImpressaoDigitalInvalida => "ImpressaoDigitalInvalida",
        Falha::TokenInvalido => "TokenInvalido",
        Falha::VoiceRoomInvalido => "VoiceRoomInvalido",
    }
}

// ---------------------------------------------------------------------------
// A portaria — ADR 0030. A porta do servidor que esta janela hospeda.
// ---------------------------------------------------------------------------
//
// Estes comandos falam **direto com o PERSISTENCE do servidor embutido**, e não pelo fio
// como toda a moderação faz. Não é atalho; é o ADR 0030:
//
// - fechar a porta não pode depender de estar dentro, senão a defesa depende do
//   canal que ela defende, e só fecha quem já tinha entrado enquanto estava
//   aberto;
// - a porta se fecha antes do primeiro pacote, no mesmo gesto de hospedar;
// - e nenhum verbo novo de protocolo é nenhuma superfície nova exposta à
//   internet para uma decisão que é, por definição, de quem está na máquina.
//
// O custo, que é real e está no ADR: isto não administra o servidor de outra
// pessoa. Quem está conectado ao servidor de um amigo não vê nenhuma destas telas,
// e é por isso que todas começam por `NaoEstaHospedando`.

/// Por que um comando da portaria não deu.
///
/// Enum e não frase, como as vizinhas: a fronteira erro→texto é do frontend.
#[derive(Debug, serde::Serialize)]
enum FalhaNaPortaria {
    /// Esta janela não está hospedando nada, então não há porta para mexer.
    NaoEstaHospedando,
    /// O banco do servidor não respondeu.
    BancoNaoRespondeu,
}

/// Em que estado está a porta do servidor que esta janela hospeda.
///
/// Uma leitura só, porque as quatro coisas são lidas juntas e mostradas juntas:
/// quem hospeda precisa ver **as três camadas ao mesmo tempo** para entender o
/// que está valendo. Ver `portaria` em `seele-server` e o ADR 0030.
#[derive(Debug, serde::Serialize)]
struct EstadoDaPorta {
    /// Se há um servidor no ar nesta janela.
    hospedando: bool,
    /// Sem senha e sem convites: qualquer um que alcance a porta passa a
    /// primeira camada. ADR 0021.
    aberto: bool,
    /// Se há senha do servidor configurada.
    tem_senha: bool,
    /// Se há pelo menos um convite emitido.
    aceita_convites: bool,
    /// Se a portaria pergunta antes de deixar entrar quem nunca entrou.
    portaria_ligada: bool,
    /// Quantos pedidos esperam decisão. O número que o cartão mostra sem
    /// desenhar a fila inteira.
    pendentes: i64,
    /// Até onde este servidor é alcançável, pelo degrau do ADR 0022.
    ///
    /// É o que transforma «está aberto» em «está aberto **para a internet**»,
    /// que são duas frases com urgências diferentes.
    alcance: &'static str,
}

/// Um pedido, como o cartão o desenha.
///
/// A ordem dos campos aqui não é a ordem da tela, e a da tela é que importa: a
/// impressão digital em primeiro lugar, o apelido abaixo e entre aspas. Ver o
/// ADR 0030 — título é do que a pessoa é, e quem bateu ainda não é nada.
#[derive(Debug, serde::Serialize)]
struct PedidoNaTela {
    /// SHA-256 da chave pública. **A identidade.**
    impressao: String,
    /// O apelido pedido. Texto que a pessoa digitou, e nada além disso.
    apelido: String,
    /// `aberto` | `senha` | `convite`.
    segredo: String,
    /// A observação que quem hospeda escreveu ao gerar o convite.
    observacao: String,
    /// Quando bateu pela primeira vez, em segundos.
    bateu_em: i64,
    /// Quantas vezes bateu.
    batidas: i64,
    /// `null` enquanto ninguém decidiu.
    decidido_em: Option<i64>,
    /// Se a decisão foi admitir.
    admitido: bool,
}

/// O `Arc` do PERSISTENCE do servidor hospedado, ou a recusa.
///
/// Clonado para fora do `Mutex` do app **antes** de qualquer `await`: segurar um
/// `std::sync::MutexGuard` atravessando um ponto de espera trava os dois
/// cadeados de uma vez e nem compila do lado do Tauri.
fn casper_hospedado(
    session: &State<'_, Session>,
) -> Result<seele_server::hospedagem::CasperCompartilhado, FalhaNaPortaria> {
    let aberto = session
        .hospedagem
        .lock()
        .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)?;
    let server = aberto.as_ref().ok_or(FalhaNaPortaria::NaoEstaHospedando)?;
    Ok(server.persistence())
}

/// O estado das três camadas da porta.
#[tauri::command]
async fn estado_da_porta(session: State<'_, Session>) -> Result<EstadoDaPorta, FalhaNaPortaria> {
    let (persistence, alcance) = {
        let aberto = session
            .hospedagem
            .lock()
            .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)?;
        let Some(server) = aberto.as_ref() else {
            // Não hospedar não é falha: é a resposta, e a tela desenha a partir
            // dela em vez de mostrar um erro a quem não pediu nada.
            return Ok(EstadoDaPorta {
                hospedando: false,
                aberto: false,
                tem_senha: false,
                aceita_convites: false,
                portaria_ligada: false,
                pendentes: 0,
                alcance: "SoRedeLocal",
            });
        };
        let alcance = server
            .alcance()
            .map_or("SoRedeLocal", |alcance| alcance.degrau().nome());
        (server.persistence(), alcance)
    };

    let persistence = persistence.lock().await;
    let politica = seele_server::admissao::Politica::carregar(&persistence)
        .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)?;
    let portaria_ligada = seele_server::portaria::ligada(&persistence)
        .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)?;
    let pendentes = seele_server::portaria::pendentes(&persistence)
        .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)?;

    Ok(EstadoDaPorta {
        hospedando: true,
        aberto: politica.aberto(),
        tem_senha: politica.tem_senha(),
        aceita_convites: politica.aceita_convites(),
        portaria_ligada,
        pendentes,
        alcance,
    })
}

/// Põe ou tira a senha do servidor. `None` tira. ADR 0021.
#[tauri::command]
async fn definir_senha_do_server(
    session: State<'_, Session>,
    senha: Option<String>,
) -> Result<(), FalhaNaPortaria> {
    let persistence = casper_hospedado(&session)?;
    let mut persistence = persistence.lock().await;
    seele_server::admissao::definir_senha(&mut persistence, senha.as_deref())
        .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)
}

/// Gera um convite de uso único e devolve o link inteiro para mandar.
///
/// O link, e não o token cru: o token sozinho obriga quem recebe a saber o
/// endereço por outro caminho, e é o link que a outra ponta já sabe colar.
#[tauri::command]
async fn criar_convite_do_server(
    session: State<'_, Session>,
    observacao: String,
) -> Result<String, FalhaNaPortaria> {
    let (persistence, ..) = {
        let aberto = session
            .hospedagem
            .lock()
            .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)?;
        let server = aberto.as_ref().ok_or(FalhaNaPortaria::NaoEstaHospedando)?;
        (server.persistence(), ())
    };

    let token = {
        let mut persistence = persistence.lock().await;
        seele_server::admissao::criar_convite(&mut persistence, &observacao)
            .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)?
    };

    let aberto = session
        .hospedagem
        .lock()
        .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)?;
    let server = aberto.as_ref().ok_or(FalhaNaPortaria::NaoEstaHospedando)?;
    Ok(server.convite_com_token(&token))
}

/// Liga ou desliga a portaria. ADR 0030.
#[tauri::command]
async fn ligar_portaria(session: State<'_, Session>, ligada: bool) -> Result<(), FalhaNaPortaria> {
    let persistence = casper_hospedado(&session)?;
    let mut persistence = persistence.lock().await;
    seele_server::portaria::ligar(&mut persistence, ligada)
        .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)
}

/// A fila e o histórico da portaria.
#[tauri::command]
async fn pedidos_da_portaria(
    session: State<'_, Session>,
) -> Result<Vec<PedidoNaTela>, FalhaNaPortaria> {
    let persistence = casper_hospedado(&session)?;
    let persistence = persistence.lock().await;
    let fila = seele_server::portaria::pedidos(&persistence)
        .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)?;

    Ok(fila
        .into_iter()
        .map(|pedido| PedidoNaTela {
            impressao: pedido.impressao,
            apelido: pedido.apelido,
            segredo: pedido.segredo,
            observacao: pedido.observacao,
            bateu_em: pedido.bateu_em,
            batidas: pedido.batidas,
            decidido_em: pedido.decidido_em,
            admitido: pedido.admitido,
        })
        .collect())
}

/// Quem hospeda decide sobre um pedido.
#[tauri::command]
async fn decidir_pedido(
    session: State<'_, Session>,
    impressao: String,
    admitir: bool,
) -> Result<(), FalhaNaPortaria> {
    let persistence = casper_hospedado(&session)?;
    let mut persistence = persistence.lock().await;
    seele_server::portaria::decidir(&mut persistence, &impressao, admitir)
        .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)
}

/// Desfaz uma decisão: a pessoa volta a ser desconhecida.
///
/// Não é banir e não derruba quem está dentro — ADR 0030. A frase que a tela
/// mostra antes de fazer isto é que precisa dizer as duas coisas.
#[tauri::command]
async fn revogar_admissao(
    session: State<'_, Session>,
    impressao: String,
) -> Result<(), FalhaNaPortaria> {
    let persistence = casper_hospedado(&session)?;
    let mut persistence = persistence.lock().await;
    seele_server::portaria::revogar(&mut persistence, &impressao)
        .map_err(|_| FalhaNaPortaria::BancoNaoRespondeu)
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
/// O `connection` faz o contrário — e está certo: `ui::wrap` já colapsa antes de
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
/// `Search::resume_at` é a mesma regra que o `connection` já usava em
/// `App::refazer_busca`, e mora no core justamente para não haver duas. O
/// cursor continua deste lado da ponte, em `Session::busca`: o JavaScript não
/// decide nada sobre busca (`specs/06-clientes-gui.md:19`).
#[tauri::command]
fn buscar(session: State<'_, Session>, termo: String) -> Result<BuscaEstado, ConnectionError> {
    // `messages()` e não `snapshot()`: a conversa saiu do snapshot, que agora
    // carrega só a revisão dela. Aqui a lista inteira é mesmo necessária — é o
    // que se busca —, e este comando roda quando alguém digita, não a cada
    // quadro.
    let mensagens = session.connection()?.messages();
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
    /// A página de releases respondeu, e não há release com manifesto nela.
    ///
    /// Separado de [`Self::NaoAlcancei`] porque a diferença é tudo para quem
    /// lê. Ali a rede falhou e tentar de novo faz sentido; aqui a rede
    /// funcionou perfeitamente e a resposta foi «não há nada publicado» —
    /// tentar de novo não muda nada, e mandar a pessoa conferir a conexão a
    /// manda procurar defeito onde não há.
    ///
    /// É o estado normal de um projeto que ainda não publicou release nenhum
    /// com `latest.json` ao lado, e foi assim que ele apareceu: o botão
    /// respondeu «a página de releases não respondeu» sobre uma página que
    /// tinha respondido.
    NadaPublicado,
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
        // `ReleaseNotFound` saiu daqui: ele é a resposta bem-sucedida «não há
        // release publicado», e não uma falha de rede. Ver `NadaPublicado`.
        Falha::ReleaseNotFound => FalhaAoAtualizar::NadaPublicado,
        Falha::Reqwest(_) | Falha::Network(_) | Falha::Serialization(_) => {
            FalhaAoAtualizar::NaoAlcancei
        }
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
/// antes que o SEELE vai fechar e abrir de novo** — e, se houver um servidor
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
    // Qual criptografia o `reqwest` usa, dito de fora.
    //
    // O `rustls` desta árvore é compilado sem provedor embutido de propósito
    // (ver `Cargo.toml`): a alternativa punha uma segunda pilha, em C, ao lado
    // do `ring` que já está aqui. O preço é esta linha, e ela precisa vir antes
    // do primeiro cliente HTTP existir.
    //
    // `Err` significa que alguém já instalou um — o atualizador, provavelmente —
    // e nesse caso está feito, que era o objetivo.
    let _ = rustls::crypto::ring::default_provider().install_default();

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
        // O seletor de arquivos. Registrado para que `escolher_arquivo` possa
        // chamá-lo daqui de dentro; a página **não** o alcança — não há
        // permissão de `dialog` em `capabilities/janela.json`, e é a decisão:
        // quem abre um diálogo neste app é um comando desta casca, com o título
        // escrito aqui, e não uma linha de JavaScript.
        .plugin(tauri_plugin_dialog::init())
        .manage(Session::default())
        .setup(move |app| {
            // O módulo de vídeo passa a morar ao lado do banco, e não dentro do
            // pacote.
            //
            // Dentro do pacote ele não sobrevive a uma instalação: cada versão
            // nova traz um pacote novo e vazio, e quem já tinha o módulo volta a
            // ver «falta o módulo de vídeo». Foi o que aconteceu duas vezes num
            // dia, no macOS e no Windows, e não é um acidente de empacotamento —
            // é onde o arquivo estava guardado. Ao lado do banco ele é do
            // computador, e não da versão instalada.
            //
            // Só quando ninguém apontou: `SEELE_OPENH264` continua sendo a
            // palavra final de quem quer testar outro módulo.
            if std::env::var_os("SEELE_OPENH264").is_none() {
                let pasta = config_dir(app.handle());
                // SAFETY-ish: estamos antes de qualquer thread do app tocar o
                // ambiente — o `setup` roda uma vez, antes da primeira janela.
                std::env::set_var("SEELE_OPENH264", &pasta);
                tracing::info!(%pasta, "onde o módulo de vídeo é procurado");
            }
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
            open_channel,
            send_message,
            criar_voice_room,
            criar_linha,
            renomear_voice_room,
            renomear_linha,
            renomear_server,
            regras_do_icone_do_server,
            modulo_de_video_a_baixar,
            baixar_modulo_de_video,
            escolher_icone_do_server,
            tirar_icone_do_server,
            icone_do_server,
            expulsar_pessoa,
            banir_pessoa,
            remover_mensagem,
            mover_pessoa,
            apagar_voice_room,
            apagar_linha,
            peso_da_linha,
            set_muted,
            set_total_isolation,
            set_talking,
            set_voice_mode,
            set_volume,
            fontes_de_tela,
            permissao_de_tela,
            pedir_permissao_de_tela,
            compartilhar_tela,
            parar_de_compartilhar,
            ajustar_limites_da_tela,
            dispensar_aviso,
            permissao_de_microfone,
            abrir_ajustes_do_microfone,
            lembrar_aparencia_do_servidor,
            tela_cheia,
            microfones,
            microfone_escolhido,
            modo_de_voz_escolhido,
            tecla_de_falar,
            escolher_tecla_de_falar,
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
            descrever_arquivo,
            escolher_arquivo,
            enviar_anexo,
            salvar_anexo,
            prever_anexo,
            regras_de_previa,
            pasta_de_downloads,
            estado_da_porta,
            definir_senha_do_server,
            criar_convite_do_server,
            ligar_portaria,
            pedidos_da_portaria,
            decidir_pedido,
            revogar_admissao,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = started {
        tracing::error!(%error, "the desktop shell could not start");
        std::process::exit(1);
    }
}
