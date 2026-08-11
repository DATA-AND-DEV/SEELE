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
    /// A impressão não atravessa a ponte: `specs/06-clientes-gui.md:19` diz que
    /// se o frontend precisa saber o que é uma impressão digital, algo está
    /// errado. Mas jogá-la fora aqui seria pior. `seele-proto/src/uri.rs` chama
    /// `fp` "o motivo principal de isto existir": é o que transforma o primeiro
    /// contato de cego em verificado, e é a razão de o ADR 0006 ter fechado.
    ///
    /// Este app ainda não sabe conferir — [`seele_ffi::ConnectConfig`] não tem
    /// campo por onde ela passe, e o `Trust::FirstContact` sai antes de a casca
    /// se inscrever. Guardar aqui é o que faz o conserto ser ligar dois pontos
    /// em vez de descobrir onde o valor se perdeu; o `plug` já confere, e a tela
    /// diz que este não confere em vez de calar.
    convite: Mutex<Option<seele_ffi::uri::Convite>>,
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

#[tauri::command]
async fn connect(
    app: AppHandle,
    session: State<'_, Session>,
    server: String,
    nickname: String,
    audio: bool,
    join_secret: Option<String>,
) -> Result<Snapshot, PlugError> {
    if session.plug().is_ok() {
        return Err(PlugError::AlreadyConnected);
    }

    // O convite guardado vale para o Dogma dele e para nenhum outro. Quem cola
    // um link e depois troca o endereço no campo deixaria para trás uma
    // confirmação de identidade que não é deste servidor — e o dia em que a FFI
    // souber conferi-la seria o dia em que uma sobra dessas viraria uma recusa
    // que ninguém consegue explicar.
    if let Ok(mut slot) = session.convite.lock() {
        if slot.as_ref().is_some_and(|convite| convite.alvo != server) {
            *slot = None;
        }
    }

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
    };

    // `connect` blocks on a QUIC handshake. Running it on the async runtime's
    // worker would stall every other command until it finished or timed out.
    let plug = tauri::async_runtime::spawn_blocking(move || Plug::connect(config))
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

    Ok(snapshot)
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

    let anfitriao = Anfitriao {
        aqui: format!("127.0.0.1:{PORTA_PADRAO}"),
        convite: dogma.convite(),
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
/// Deliberadamente **não** é o `Convite` inteiro do core. Ele carrega a
/// impressão digital do certificado, e `specs/06-clientes-gui.md:19` é
/// inegociável: se o frontend precisa saber o que é uma impressão digital, algo
/// está errado. O que atravessa a ponte é o endereço a preencher e o token a
/// devolver no `connect` — as duas coisas que a tela tem o que fazer com.
#[derive(Debug, serde::Serialize)]
struct ConviteLido {
    /// O endereço do Dogma, para o campo DOGMA.
    alvo: String,
    /// O convite de uso único, quando o link trouxe um.
    token: Option<String>,
    /// O link trouxe uma confirmação de identidade que este app não confere.
    ///
    /// Só *que* existe, nunca *qual* — a segunda metade é a que o frontend não
    /// pode saber. Um booleano é o suficiente para a tela dizer o que tem que
    /// dizer, e é tudo o que ela recebe.
    ///
    /// Existe porque silêncio aqui é a falha, não a falta do recurso: quem cola
    /// um link que traz a confirmação supõe estar protegido **por causa dela**.
    /// A afordância é nova nesta versão; "antes também não conferia" não vale
    /// como resposta para algo que antes não dava para fazer.
    conferencia_pendente: bool,
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
        conferencia_pendente: convite.impressao_digital.is_some(),
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
    let snapshot = session.plug()?.snapshot();
    let mut busca = seele_ffi::search::Search::new(
        snapshot.messages.iter().map(|mensagem| &mensagem.body),
        &termo,
    );

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
            insert_plug,
            eject_plug,
            open_line,
            send_message,
            set_at_field,
            set_total_isolation,
            set_talking,
            set_voice_mode,
            set_volume,
            conhecidos,
            esquecer,
            analisar_convite,
            buscar,
            busca_andar,
            busca_limpar,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = started {
        tracing::error!(%error, "the desktop shell could not start");
        std::process::exit(1);
    }
}
