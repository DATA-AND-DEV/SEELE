//! **A mesa mínima de texto da 1.0, pela ponte, contra um servidor de
//! verdade**: editar, responder, mencionar, as não lidas por canal, e banir,
//! listar e desbanir.
//!
//! # Por que aqui, e conduzindo um `Connection`
//!
//! Pelo motivo de `moderacao.rs`: um braço vazio no `executar` do `seele-ffi`
//! deixa a árvore inteira verde, e o botão não faria nada, em silêncio. Estes
//! testes conduzem o mesmo objeto que o comando Tauri segura, e o que eles
//! medem só existe com as duas pontas de pé:
//!
//! - a edição que **outra** sessão vê, e a recusa que é do servidor;
//! - a marca de «editada» em quem **chegou depois** da edição — o caminho do
//!   histórico, que o `MessageReceived` não tem campo para dizer;
//! - a menção que vem do histórico e **não** levanta o aviso, e a que vem ao
//!   vivo e levanta: a cerca (`Room::esperar_historico`) é um `Ping` de
//!   verdade atrás de um `FetchHistory` de verdade;
//! - a lista de banidos que só responde a quem pode banir, e a pessoa
//!   desbanida que volta a entrar.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use seele_ffi::{
    ConnectConfig, Connection, ConnectionError, EndReason, Event, EventListener, Message,
    NoticeReason,
};
use seele_server::permissions::{Permissions, COMMANDER_ROLE, OPERATOR_ROLE, PERSON_ROLE};
use seele_server::persistence::{Location, Persistence};
use seele_server::{Daemon, ServerConfig};

mod vaga;

const LINE: u32 = 1;
const PRAZO: Duration = Duration::from_secs(10);

async fn server(marca: &str) -> Result<(SocketAddr, Arc<Daemon>, std::path::PathBuf)> {
    let mut arquivo = std::env::temp_dir();
    arquivo.push(format!("seele-mesa-{marca}-{}.db", std::process::id()));
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
    caminho.push(format!("seele-mesa-{nome}-{}", std::process::id()));
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

/// Estreia um apelido: casa limpa, identidade nova.
async fn entrar(endereco: SocketAddr, apelido: &str) -> Result<Arc<Connection>> {
    let _ = std::fs::remove_dir_all(casa(apelido));
    voltar(endereco, apelido).await
}

/// Reconecta com a **mesma** identidade.
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

fn dar_papel(
    arquivo: &std::path::Path,
    person: seele_proto::ids::PersonId,
    papel: seele_proto::ids::RoleId,
) {
    let persistence =
        Persistence::open(&Location::File(arquivo.to_path_buf())).expect("abrir o banco");
    let permissions = Permissions::new(&persistence);
    for tinha in [COMMANDER_ROLE, OPERATOR_ROLE, PERSON_ROLE] {
        if tinha != papel {
            permissions.revoke_role(person, tinha).expect("revogar");
        }
    }
    permissions.grant_role(person, papel).expect("conceder");
}

/// Entra e recebe o Comandante pelo banco, como em `moderacao.rs`.
async fn entrar_como_comandante(
    endereco: SocketAddr,
    arquivo: &std::path::Path,
    apelido: &str,
) -> Result<Arc<Connection>> {
    let primeira = entrar(endereco, apelido).await?;
    let eu = seele_proto::ids::PersonId(primeira.snapshot().me.expect("identidade"));
    dar_papel(arquivo, eu, COMMANDER_ROLE);
    drop(primeira);
    voltar(endereco, apelido).await
}

/// A mensagem confirmada de corpo `corpo`, quando ela chegar.
fn a_mensagem(connection: &Connection, corpo: &str) -> Option<Message> {
    let corpo = corpo.to_owned();
    let achou = |c: &Connection| {
        c.messages()
            .into_iter()
            .find(|mensagem| mensagem.id != 0 && mensagem.body == corpo)
    };
    ate(connection, |c| achou(c).is_some());
    achou(connection)
}

/// Conta os avisos de um motivo, em vez de olhar o último.
struct Avisos(NoticeReason, std::sync::Mutex<usize>);

impl EventListener for Avisos {
    fn on_event(&self, event: Event) {
        if let Event::NoticeRaised { notice } = event {
            if notice.reason == self.0 {
                if let Ok(mut quantos) = self.1.lock() {
                    *quantos += 1;
                }
            }
        }
    }
}

fn contando(connection: &Connection, motivo: NoticeReason) -> Arc<Avisos> {
    let avisos = Arc::new(Avisos(motivo, std::sync::Mutex::new(0)));
    connection.subscribe(Arc::clone(&avisos) as Arc<dyn EventListener>);
    avisos
}

impl Avisos {
    fn quantos(&self) -> usize {
        self.1.lock().map(|quantos| *quantos).unwrap_or(0)
    }
}

/// **Editar, responder e mencionar, de ponta a ponta.**
#[tokio::test(flavor = "multi_thread")]
async fn editar_responder_e_mencionar_pela_ponte() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor, _arquivo) = server("conversa").await?;
    let ana = entrar(endereco, "ana-mesa").await?;
    let bruno = entrar(endereco, "bruno-mesa").await?;
    assert_eq!(ana.protocolo_negociado(), seele_ffi::PROTOCOL_VERSION);
    ana.open_channel(LINE)?;
    bruno.open_channel(LINE)?;
    let chamada = contando(&ana, NoticeReason::Mentioned);
    let recusas = contando(&bruno, NoticeReason::PermissionDenied);

    // A primeira, e a resposta do Bruno a ela, chamando a Ana pelo apelido.
    ana.send_message(LINE, "primeira, com eror".into())?;
    let primeira = a_mensagem(&bruno, "primeira, com eror").expect("a primeira não chegou");
    assert_eq!((primeira.replies_to, primeira.edited), (None, false));
    bruno.send_reply(LINE, "@Ana-Mesa concordo".into(), primeira.id)?;
    let resposta = a_mensagem(&ana, "@Ana-Mesa concordo").expect("a resposta não chegou");
    assert_eq!(
        resposta.replies_to,
        Some(primeira.id),
        "a resposta chegou solta, sem dizer a quem responde"
    );
    assert!(
        resposta.mentions_me,
        "a Ana foi chamada pelo apelido e a mensagem não foi marcada"
    );
    assert!(
        ate(&ana, |_| chamada.quantos() == 1),
        "a menção ao vivo não levantou o aviso uma vez: {}",
        chamada.quantos()
    );
    // Para o Bruno, que escreveu, não é menção de ninguém a ele.
    assert!(
        !a_mensagem(&bruno, "@Ana-Mesa concordo")
            .unwrap()
            .mentions_me
    );

    // A Ana corrige a própria, e o Bruno vê o corpo novo, marcado.
    ana.edit_message(primeira.id, "primeira, sem erro".into())?;
    let corrigida = a_mensagem(&bruno, "primeira, sem erro").expect("a edição não chegou");
    assert_eq!((corrigida.id, corrigida.edited), (primeira.id, true));
    assert!(a_mensagem(&ana, "primeira, sem erro").unwrap().edited);

    // O Bruno não edita a mensagem da Ana: a recusa é do servidor.
    bruno.edit_message(primeira.id, "o que ela não disse".into())?;
    assert!(
        ate(&bruno, |_| recusas.quantos() == 1),
        "o servidor não recusou a edição da mensagem de outra pessoa"
    );
    // A recusa já voltou: se a edição tivesse passado, o corpo novo teria
    // sido difundido antes dela.
    for (quem, conexao) in [("a Ana", &ana), ("o Bruno", &bruno)] {
        assert!(
            conexao
                .messages()
                .iter()
                .any(|m| m.id == primeira.id && m.body == "primeira, sem erro"),
            "{quem} vê a mensagem da Ana reescrita por outra pessoa"
        );
    }

    servidor.shutdown();
    Ok(())
}

/// **Quem chega depois lê o histórico como ele é**: a editada vem marcada, a
/// resposta diz a quem responde, e a menção de antes marca a mensagem sem
/// levantar o aviso — que só sobe para o que é dito depois de a página chegar.
#[tokio::test(flavor = "multi_thread")]
async fn o_historico_traz_a_editada_marcada_e_a_mencao_antiga_nao_avisa() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor, _arquivo) = server("historico").await?;
    let ana = entrar(endereco, "ana-hist").await?;
    ana.open_channel(LINE)?;
    ana.send_message(LINE, "antes".into())?;
    let antes = a_mensagem(&ana, "antes").expect("a mensagem não foi gravada");
    ana.edit_message(antes.id, "depois".into())?;
    assert!(a_mensagem(&ana, "depois").is_some(), "a edição não voltou");
    ana.send_reply(LINE, "@carla-hist, quando chegar leia".into(), antes.id)?;
    assert!(a_mensagem(&ana, "@carla-hist, quando chegar leia").is_some());
    ana.send_message(LINE, "esta ninguém editou".into())?;
    assert!(a_mensagem(&ana, "esta ninguém editou").is_some());

    // A Carla nunca viu nada disso acontecer.
    let carla = entrar(endereco, "carla-hist").await?;
    let chamada = contando(&carla, NoticeReason::Mentioned);
    carla.open_channel(LINE)?;
    let intacta = a_mensagem(&carla, "esta ninguém editou").expect("o histórico não chegou");
    assert!(!intacta.edited, "o histórico marcou a que ninguém editou");
    assert!(
        ate(&carla, |c| c
            .messages()
            .iter()
            .any(|m| m.id == antes.id && m.body == "depois" && m.edited)),
        "a editada não voltou do histórico marcada: {:?}",
        carla.messages()
    );
    let chamando = a_mensagem(&carla, "@carla-hist, quando chegar leia").unwrap();
    assert_eq!(chamando.replies_to, Some(antes.id));
    assert!(
        chamando.mentions_me,
        "a menção do histórico não foi marcada"
    );

    // Uma mensagem ao vivo, depois da página: é a prova de que a cerca já
    // voltou, e a única menção que avisa.
    ana.send_message(LINE, "@carla-hist agora".into())?;
    assert!(a_mensagem(&carla, "@carla-hist agora").unwrap().mentions_me);
    assert!(
        ate(&carla, |_| chamada.quantos() >= 1),
        "a menção ao vivo não levantou o aviso"
    );
    assert_eq!(
        chamada.quantos(),
        1,
        "a menção que já estava no histórico levantou o aviso junto com a de agora"
    );

    servidor.shutdown();
    Ok(())
}

/// **As não lidas e as menções de cada canal**, no retrato: o que chega no
/// canal que a pessoa deixou conta, e abrir o canal zera.
#[tokio::test(flavor = "multi_thread")]
async fn as_nao_lidas_e_as_mencoes_do_canal_chegam_ao_retrato() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor, arquivo) = server("naolidas").await?;
    let ana = entrar_como_comandante(endereco, &arquivo, "ana-lidas").await?;
    let bruno = entrar(endereco, "bruno-lidas").await?;
    ana.create_channel("avisos".into())?;
    assert!(
        ate(&ana, |c| c.snapshot().channels.len() == 2),
        "o segundo canal não foi criado"
    );
    let avisos = ana
        .snapshot()
        .channels
        .iter()
        .find(|canal| canal.id != LINE)
        .expect("o canal novo")
        .id;

    // A Ana lê o `geral`, e depois vai para o `avisos`.
    ana.open_channel(LINE)?;
    bruno.open_channel(LINE)?;
    bruno.send_message(LINE, "lida na hora".into())?;
    assert!(a_mensagem(&ana, "lida na hora").is_some());
    ana.open_channel(avisos)?;
    assert!(ate(&ana, |c| c
        .snapshot()
        .channels
        .iter()
        .any(|canal| canal.id == avisos && canal.open)));
    let do_geral = |c: &Connection| {
        c.snapshot()
            .channels
            .iter()
            .find(|canal| canal.id == LINE)
            .map(|canal| (canal.nao_lidas, canal.mencoes))
    };
    assert_eq!(
        do_geral(&ana),
        Some((0, 0)),
        "o que a Ana leu com o canal aberto voltou como não lido"
    );

    bruno.send_message(LINE, "ninguém em especial".into())?;
    bruno.send_message(LINE, "@ana-lidas, volta aqui".into())?;
    assert!(
        ate(&ana, |c| do_geral(c) == Some((2, 1))),
        "as não lidas do canal fechado não chegaram ao retrato: {:?}",
        do_geral(&ana)
    );

    ana.open_channel(LINE)?;
    assert!(
        ate(&ana, |c| do_geral(c) == Some((0, 0))),
        "abrir o canal não o marcou como lido: {:?}",
        do_geral(&ana)
    );

    servidor.shutdown();
    Ok(())
}

/// **Banir, listar e desbanir, de ponta a ponta.**
#[tokio::test(flavor = "multi_thread")]
async fn banir_listar_e_desbanir_pela_ponte() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor, arquivo) = server("banidos").await?;
    let ana = entrar_como_comandante(endereco, &arquivo, "ana-ban").await?;
    let bruno = entrar(endereco, "bruno-ban").await?;
    let carla = entrar(endereco, "carla-ban").await?;
    let o_bruno = bruno.snapshot().me.expect("identidade");
    let a_ana = ana.snapshot().me.expect("identidade");

    ana.ban_person(o_bruno, Some("spam".into()), None)?;
    assert!(
        ate(&bruno, |c| c.snapshot().ended == Some(EndReason::Banned)),
        "o banido não foi posto para fora"
    );
    assert!(
        voltar(endereco, "bruno-ban").await.is_err(),
        "o banido voltou"
    );

    // Quem não pode banir não lê a lista: o motivo é registro de quem modera.
    let negadas = contando(&carla, NoticeReason::PermissionDenied);
    carla.list_bans()?;
    carla.unban_person(o_bruno)?;
    assert!(
        ate(&carla, |_| negadas.quantos() == 2),
        "o servidor não recusou a lista e o desbanimento a quem não pode banir: {}",
        negadas.quantos()
    );
    assert!(carla.banidos().is_empty());

    // A Comandante lê, com o que ela mesma escreveu.
    assert!(ana.banidos().is_empty(), "a lista veio sem ninguém pedir");
    ana.list_bans()?;
    assert!(
        ate(&ana, |c| c.banidos().len() == 1),
        "a lista de banidos não chegou"
    );
    let banido = ana.banidos().remove(0);
    assert_eq!(
        (
            banido.person,
            banido.reason.as_deref(),
            banido.expires_at,
            banido.issued_by
        ),
        (o_bruno, Some("spam"), None, Some(a_ana))
    );
    // O nome que a tela mostra sai da identidade apresentada, que vem atrás da
    // lista: a base, sem a marca.
    assert!(
        ate(&ana, |c| c
            .snapshot()
            .identidades_apresentadas
            .get(&o_bruno)
            .is_some_and(|(base, _)| base == "bruno-ban")),
        "a identidade apresentada do banido não veio com a lista"
    );
    assert!(
        !ana.membros_fora_do_ar().iter().any(|m| m.id == o_bruno),
        "o banido continua no FORA DO AR"
    );

    // Desbanir: a lista esvazia, o Bruno volta a ser membro para todo mundo,
    // e a porta abre.
    ana.unban_person(o_bruno)?;
    assert!(
        ate(&ana, |c| c.banidos().is_empty()),
        "a lista não voltou vazia depois de desbanir"
    );
    for (quem, conexao) in [("quem desbaniu", &ana), ("quem só estava lá", &carla)] {
        assert!(
            ate(conexao, |c| c
                .membros_fora_do_ar()
                .iter()
                .any(|m| m.id == o_bruno)),
            "{quem} não viu o desbanido voltar ao FORA DO AR"
        );
    }
    let de_volta = voltar(endereco, "bruno-ban").await?;
    assert_eq!(de_volta.snapshot().me, Some(o_bruno));

    servidor.shutdown();
    Ok(())
}
