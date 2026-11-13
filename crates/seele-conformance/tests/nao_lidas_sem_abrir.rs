//! **As não lidas e as menções de um canal que a pessoa nunca abriu**, pela
//! ponte, contra um servidor de verdade — e de novo depois de uma queda que o
//! núcleo reconecta.
//!
//! # O defeito
//!
//! O servidor só difundia a conversa de um canal a quem tivesse mandado
//! `JoinChannel`, e o cliente só o manda ao abrir o canal. A contagem por
//! canal da mesa de texto da 1.0 conta o que chega: num canal ainda não aberto
//! nesta conexão não chegava nada, a lista mostrava zero com a conversa
//! andando lá dentro, e ser chamado ali não avisava ninguém. Depois de uma
//! reconexão só o canal aberto era reassinado.
//!
//! # O conserto, e por que o teste mora aqui
//!
//! Desde o protocolo 13 o servidor entrega a quem **pode ler** o canal, sem
//! pedido (`ASSINA_O_QUE_LE_DESDE`, em `seele-server/src/session.rs`; o fio no
//! fio está em `seele-server/tests/assinatura_do_que_se_le.rs`). O que este
//! arquivo mede só existe com as duas pontas de pé: o `Room` do cliente
//! contando o que o servidor passou a mandar, e o `Connection` voltando de uma
//! queda sem reassinar nada.
//!
//! Provado contra a regressão: com `ASSINA_O_QUE_LE_DESDE` em 255, os dois
//! reprovam na primeira contagem.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use seele_ffi::types::LinkState;
use seele_ffi::{ConnectConfig, Connection, ConnectionError, Event, EventListener, NoticeReason};
use seele_proto::control::DisconnectReason;
use seele_proto::ids::PersonId;
use seele_server::permissions::{Permissions, COMMANDER_ROLE, PERSON_ROLE};
use seele_server::persistence::{Location, Persistence};
use seele_server::{Daemon, ServerConfig};

mod vaga;

const GERAL: u32 = 1;
const PRAZO: Duration = Duration::from_secs(15);

async fn server(marca: &str) -> Result<(SocketAddr, Arc<Daemon>, std::path::PathBuf)> {
    let mut arquivo = std::env::temp_dir();
    arquivo.push(format!("seele-sem-abrir-{marca}-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&arquivo);
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::File(arquivo.clone()),
        ..ServerConfig::default()
    };
    let servidor = Arc::new(Daemon::bind(config).await?);
    let endereco = servidor.local_addr()?;
    let aceitando = Arc::clone(&servidor);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });
    Ok((endereco, servidor, arquivo))
}

fn casa(nome: &str) -> String {
    let mut caminho = std::env::temp_dir();
    caminho.push(format!("seele-sem-abrir-{nome}-{}", std::process::id()));
    caminho.to_string_lossy().into_owned()
}

fn conectar(endereco: SocketAddr, apelido: &str) -> Result<Arc<Connection>, ConnectionError> {
    Connection::connect(ConnectConfig {
        server: endereco.to_string(),
        alternate_servers: Vec::new(),
        nickname: apelido.to_owned(),
        home: casa(apelido),
        join_secret: None,
        expected_fingerprint: None,
        codigo_esperado: None,
        bilhete: None,
        audio: false,
        capture_device: None,
        playback_device: None,
    })
    .map(|(connection, _confianca)| connection)
}

async fn entrar(endereco: SocketAddr, apelido: &str) -> Result<Arc<Connection>> {
    let _ = std::fs::remove_dir_all(casa(apelido));
    voltar(endereco, apelido).await
}

async fn voltar(endereco: SocketAddr, apelido: &str) -> Result<Arc<Connection>> {
    let apelido = apelido.to_owned();
    Ok(tokio::task::spawn_blocking(move || conectar(endereco, &apelido)).await??)
}

fn ate<F: Fn(&Connection) -> bool>(connection: &Connection, pronto: F) -> bool {
    let fim = Instant::now() + PRAZO;
    while Instant::now() < fim {
        if pronto(connection) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    pronto(connection)
}

/// Entra e recebe o Comandante pelo banco, como em `moderacao.rs`.
async fn entrar_como_comandante(
    endereco: SocketAddr,
    arquivo: &std::path::Path,
    apelido: &str,
) -> Result<Arc<Connection>> {
    let primeira = entrar(endereco, apelido).await?;
    let eu = PersonId(primeira.snapshot().me.expect("identidade"));
    {
        let persistence = Persistence::open(&Location::File(arquivo.to_path_buf()))?;
        let permissions = Permissions::new(&persistence);
        permissions.revoke_role(eu, PERSON_ROLE)?;
        permissions.grant_role(eu, COMMANDER_ROLE)?;
    }
    drop(primeira);
    voltar(endereco, apelido).await
}

/// O que a janela ouviria: quantas vezes foi chamada, e o enlace lido no
/// próprio aviso (a faixa da bateria dura milissegundos quando a primeira
/// volta dá certo — `queda_recuperavel.rs`).
#[derive(Default)]
struct Janela {
    chamadas: std::sync::Mutex<usize>,
    acabou: std::sync::Mutex<bool>,
    conexao: std::sync::OnceLock<std::sync::Weak<Connection>>,
    /// `true` para cada leitura na bateria, `false` para cada uma de pé.
    enlaces: std::sync::Mutex<Vec<bool>>,
}

impl EventListener for Janela {
    fn on_event(&self, event: Event) {
        match &event {
            Event::NoticeRaised { notice } if notice.reason == NoticeReason::Mentioned => {
                *self.chamadas.lock().unwrap() += 1;
            }
            Event::Ended { .. } => *self.acabou.lock().unwrap() = true,
            _ => {}
        }
        if !matches!(event, Event::TelemetryChanged | Event::Ended { .. }) {
            return;
        }
        let Some(conexao) = self.conexao.get().and_then(std::sync::Weak::upgrade) else {
            return;
        };
        let na_bateria = matches!(conexao.snapshot().link, LinkState::InternalBattery { .. });
        self.enlaces.lock().unwrap().push(na_bateria);
    }
}

impl Janela {
    fn de(conexao: &Arc<Connection>) -> Arc<Self> {
        let janela = Arc::new(Self::default());
        let _ = janela.conexao.set(Arc::downgrade(conexao));
        conexao.subscribe(Arc::clone(&janela) as Arc<dyn EventListener>);
        janela
    }

    fn chamadas(&self) -> usize {
        *self.chamadas.lock().unwrap()
    }

    fn marca(&self) -> usize {
        self.enlaces.lock().unwrap().len()
    }

    /// Se, desde `marca`, a janela leu a faixa da bateria e depois a volta.
    fn caiu_e_voltou_desde(&self, marca: usize) -> bool {
        let enlaces = self.enlaces.lock().unwrap();
        let depois = enlaces.get(marca..).unwrap_or_default();
        depois
            .iter()
            .position(|na_bateria| *na_bateria)
            .is_some_and(|faixa| depois.iter().skip(faixa).any(|na_bateria| !na_bateria))
    }
}

/// `(não lidas, menções)` de um canal, no retrato.
fn contagem(connection: &Connection, canal: u32) -> (u32, u32) {
    connection
        .snapshot()
        .channels
        .iter()
        .find(|linha| linha.id == canal)
        .map_or((0, 0), |linha| (linha.nao_lidas, linha.mencoes))
}

/// Um servidor com o `geral` e um segundo canal, a Ana comandando e com o
/// segundo canal aberto. Devolve o id dele.
async fn com_um_segundo_canal(
    endereco: SocketAddr,
    arquivo: &std::path::Path,
    apelido: &str,
) -> Result<(Arc<Connection>, u32)> {
    let ana = entrar_como_comandante(endereco, arquivo, apelido).await?;
    ana.create_channel("avisos".into())?;
    assert!(
        ate(&ana, |c| c.snapshot().channels.len() == 2),
        "o segundo canal não foi criado"
    );
    let avisos = ana
        .snapshot()
        .channels
        .iter()
        .find(|canal| canal.id != GERAL)
        .expect("o canal novo")
        .id;
    ana.open_channel(avisos)?;
    Ok((ana, avisos))
}

/// **Um canal que a pessoa nunca abriu conta o que chega nele, e ser chamada
/// ali avisa.** O Bruno entra, abre só o `geral`, e nunca manda nada sobre o
/// `avisos`.
#[tokio::test(flavor = "multi_thread")]
async fn o_canal_nunca_aberto_conta_as_nao_lidas_e_a_mencao_avisa() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor, arquivo) = server("conta").await?;
    let (ana, avisos) = com_um_segundo_canal(endereco, &arquivo, "ana-semabrir").await?;

    let bruno = entrar(endereco, "bruno-semabrir").await?;
    let janela = Janela::de(&bruno);
    bruno.open_channel(GERAL)?;
    assert_eq!(bruno.protocolo_negociado(), seele_ffi::PROTOCOL_VERSION);
    assert_eq!(contagem(&bruno, avisos), (0, 0));

    ana.send_message(avisos, "uma novidade".into())?;
    assert!(
        ate(&bruno, |c| contagem(c, avisos) == (1, 0)),
        "a mensagem num canal nunca aberto não contou como não lida: {:?}",
        contagem(&bruno, avisos)
    );
    assert_eq!(
        janela.chamadas(),
        0,
        "uma mensagem que não chama ninguém avisou"
    );

    ana.send_message(avisos, "@bruno-semabrir, olha aqui".into())?;
    assert!(
        ate(&bruno, |c| contagem(c, avisos) == (2, 1)),
        "a menção num canal nunca aberto não contou: {:?}",
        contagem(&bruno, avisos)
    );
    assert!(
        ate(&bruno, |_| janela.chamadas() == 1),
        "ser chamado num canal nunca aberto não levantou o aviso: {}",
        janela.chamadas()
    );
    // O canal aberto não ganhou nada com isso.
    assert_eq!(contagem(&bruno, GERAL), (0, 0));
    assert!(bruno.messages().is_empty(), "{:?}", bruno.messages());

    // Abrir o canal zera, e o que já tinha chegado não avisa de novo quando a
    // página do histórico o traz outra vez.
    bruno.open_channel(avisos)?;
    assert!(
        ate(&bruno, |c| contagem(c, avisos) == (0, 0)
            && c.messages().len() == 2),
        "abrir o canal não zerou a contagem ou não trouxe a conversa"
    );
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        janela.chamadas(),
        1,
        "a página do histórico repetiu o aviso"
    );

    servidor.shutdown();
    Ok(())
}

/// **Depois de uma queda que o núcleo reconecta**, o canal nunca aberto
/// continua contando e avisando — sem a pessoa reabrir nada.
#[tokio::test(flavor = "multi_thread")]
async fn depois_da_reconexao_o_canal_nunca_aberto_continua_contando() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor, arquivo) = server("volta").await?;
    let (ana, avisos) = com_um_segundo_canal(endereco, &arquivo, "ana-volta").await?;

    let bruno = entrar(endereco, "bruno-volta").await?;
    let janela = Janela::de(&bruno);
    bruno.open_channel(GERAL)?;
    let quem = PersonId(bruno.snapshot().me.expect("ele sabe quem é"));

    ana.send_message(avisos, "antes da queda".into())?;
    assert!(
        ate(&bruno, |c| contagem(c, avisos) == (1, 0)),
        "antes da queda o canal nunca aberto não contou: {:?}",
        contagem(&bruno, avisos)
    );

    // A queda recuperável de `queda_recuperavel.rs`: o servidor se despede com
    // `FellBehind`, e o núcleo reconecta sozinho.
    let marca = janela.marca();
    servidor
        .server()
        .events
        .send(seele_server::server::Event::SessionEnded {
            person: quem,
            reason: DisconnectReason::FellBehind,
        })
        .expect("o barramento do servidor está de pé");
    assert!(
        ate(&bruno, |_| janela.caiu_e_voltou_desde(marca)),
        "a janela não leu a faixa da bateria e depois a volta"
    );
    assert!(
        !*janela.acabou.lock().unwrap(),
        "a queda virou fim de sessão"
    );

    // Dito depois da volta, no canal que ele continua sem ter aberto. A volta
    // pode levar um instante a mais que a faixa: a Ana repete até contar.
    let contou = ate(&bruno, |c| {
        let _ = ana.send_message(avisos, "@bruno-volta, voltou?".into());
        std::thread::sleep(Duration::from_millis(400));
        contagem(c, avisos).1 >= 1
    });
    assert!(
        contou,
        "depois da reconexão o canal nunca aberto ficou mudo: {:?}",
        contagem(&bruno, avisos)
    );
    let (nao_lidas, mencoes) = contagem(&bruno, avisos);
    assert!(
        nao_lidas >= 2 && mencoes >= 1,
        "a contagem de antes da queda se perdeu, ou a de depois não somou: \
         {nao_lidas} não lidas, {mencoes} menções"
    );
    assert!(
        ate(&bruno, |_| janela.chamadas() >= 1),
        "ser chamado depois da reconexão não levantou o aviso"
    );

    ana.disconnect();
    bruno.disconnect();
    servidor.shutdown();
    Ok(())
}
