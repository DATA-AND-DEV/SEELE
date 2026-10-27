//! **Várias sessões ao mesmo tempo, contra servidores de verdade** (E3 da 0.17).
//!
//! O cenário mínimo do contrato, inteiro, pelos comandos da casca e pelo
//! runtime falso do Tauri: esta janela **hospeda** um servidor guardado (o de
//! sempre, `Hospedagem::iniciar`, com uma convidada de outra máquina dentro
//! dele) e mantém **três sessões cliente** em três `seeled` diferentes, todos
//! em processo e em porta zero.
//!
//! O que se mede, e o que reprovaria cada medida:
//!
//! - **as três ficam de pé ao mesmo tempo** e trocar a vista não derruba
//!   nenhuma (a 0.16 recusava a segunda com `AlreadyConnected`);
//! - **(1) o evento tardio**: o que acontece numa sessão do fundo chega à
//!   janela com o `SessaoId` dela e `vista: false`, e a placa dela marca;
//! - **(2) a promessa depois da troca**: uma resposta pedida para A e resolvida
//!   com B na vista vem carimbada com A e com o conteúdo de A;
//! - **(3) a reconexão em segundo plano**: o servidor de uma sessão sem foco e
//!   fora da vista cai e volta; ela reconecta com a geração nova, e a vista, o
//!   foco de voz e as outras duas não mudam, nem o servidor hospedado;
//! - **(4) o clique durante a troca**: um envio com o carimbo de C, disparado
//!   junto com a troca da vista para A, chega só ao servidor de C; um carimbo
//!   de uma sessão que saiu, ou de antes de uma reconexão, é recusado;
//! - **o foco de voz**: a primeira sessão o tem sozinha; entrar numa sala
//!   noutra sessão o traz para lá, e a anterior sai da sala dela; um
//!   `MovePerson` não o toma; a sessão sentada sem foco diz `EmOutraSessao`;
//! - **sair de uma** não toca nas outras nem no servidor hospedado.
//!
//! As sessões entram sem áudio (não há placa de som numa máquina de bateria):
//! numa sala, com o foco, o estado é `SemAparelho` («abriria, e não há
//! aparelho»), e sem o foco é `EmOutraSessao`. Os datagramas de voz contados
//! por servidor, com o aparelho de mentira, ficam na conformidade
//! (`crates/seele-conformance/tests/multi_sessao_foco_de_voz.rs`).

#![allow(
    clippy::too_many_lines,
    clippy::indexing_slicing,
    reason = "um cenário, contado do começo ao fim; num teste, o pânico de um índice é o relatório"
)]

use super::a_entrada_velha_que_chega_tarde::{app, runtime, vez};
use super::sessoes::CarimboDaJanela;
use super::{
    connect, disconnect, entrar_na_sala_em, focar_voz_em, sessoes_na_tela, ver_sessao_em, Entrada,
    Hospedado, Session, CANAL_DAS_SESSOES, EVENT_CHANNEL,
};
use seele_ffi::{AberturaDoAudio, ConnectConfig, Connection, ConnectionError};
use seele_server::hospedagem::Hospedagem;
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::test::MockRuntime;
use tauri::{AppHandle, Listener, Manager};

/// Quanto cada passo de rede tem para acontecer.
const PRAZO: Duration = Duration::from_secs(20);

/// Um `seeled` de verdade, nesta porta (zero é «o sistema escolhe») e com este
/// banco. Insiste um pouco se a porta ainda estiver presa: é o caso do
/// servidor que volta na mesma porta.
async fn seeled(nome: &str, porta: u16, banco: Location) -> (SocketAddr, Arc<Daemon>) {
    let mut tentativas = 0;
    let daemon = loop {
        let config = ServerConfig {
            name: nome.into(),
            listen: SocketAddr::from(([127, 0, 0, 1], porta)),
            database: banco.clone(),
            ..ServerConfig::default()
        };
        match Daemon::bind(config).await {
            Ok(daemon) => break Arc::new(daemon),
            Err(erro) if tentativas < 50 => {
                tentativas += 1;
                tracing::debug!(%erro, "a porta ainda não soltou");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(erro) => panic!("o servidor {nome} não subiu: {erro:#}"),
        }
    };
    let endereco = daemon.local_addr().expect("o endereço do servidor");
    let servindo = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = servindo.run().await;
    });
    (endereco, daemon)
}

/// Entra em `alvo` pelo `connect` da casca, sem áudio.
async fn entrar(app: &AppHandle<MockRuntime>, alvo: SocketAddr, apelido: &str) -> Entrada {
    tokio::time::timeout(
        PRAZO,
        connect(
            app.clone(),
            app.state::<Session>(),
            alvo.to_string(),
            apelido.to_owned(),
            false,
            None,
            None,
        ),
    )
    .await
    .unwrap_or_else(|_| panic!("o `connect` a {alvo} não voltou"))
    .unwrap_or_else(|falha| panic!("o `connect` a {alvo} não entrou: {falha:?}"))
}

/// Uma pessoa de outra máquina (outra casa, outra chave), pela FFI.
async fn de_outra_maquina(alvo: SocketAddr, apelido: &str) -> (Arc<Connection>, tempfile::TempDir) {
    let casa = tempfile::tempdir().expect("a casa da outra máquina");
    let config = ConnectConfig {
        server: alvo.to_string(),
        alternate_servers: Vec::new(),
        nickname: apelido.into(),
        home: casa.path().display().to_string(),
        join_secret: None,
        expected_fingerprint: None,
        codigo_esperado: None,
        bilhete: None,
        audio: false,
        capture_device: None,
        playback_device: None,
    };
    let conexao = tokio::time::timeout(
        PRAZO,
        tokio::task::spawn_blocking(move || Connection::connect(config)),
    )
    .await
    .expect("a outra máquina não entrou no prazo")
    .expect("a thread da outra máquina caiu")
    .map(|(conexao, _)| conexao)
    .unwrap_or_else(|erro| panic!("a outra máquina não entrou em {alvo}: {erro:?}"));
    (conexao, casa)
}

fn carimbo(entrada: &Entrada) -> CarimboDaJanela {
    CarimboDaJanela::da(entrada)
}

/// Espera a condição, olhando de pouco em pouco, até o prazo.
async fn ate(prazo: Duration, mut pronto: impl FnMut() -> bool) -> bool {
    let fim = tokio::time::Instant::now() + prazo;
    while tokio::time::Instant::now() < fim {
        if pronto() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    pronto()
}

/// Quem recebeu `corpo`, entre os servidores dados, esperando até `prazo`.
async fn quem_recebeu(
    ouvidos: &mut [(
        &str,
        tokio::sync::broadcast::Receiver<seele_server::server::Event>,
    )],
    corpo: &str,
    prazo: Duration,
) -> Vec<String> {
    let fim = tokio::time::Instant::now() + prazo;
    let mut receberam = Vec::new();
    while tokio::time::Instant::now() < fim {
        for (nome, ouvido) in ouvidos.iter_mut() {
            while let Ok(evento) = ouvido.try_recv() {
                if let seele_server::server::Event::MessagePosted(mensagem) = evento {
                    if mensagem.body == corpo && !receberam.contains(&(*nome).to_owned()) {
                        receberam.push((*nome).to_owned());
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    receberam
}

/// Quantas pessoas o servidor tem presentes.
async fn presentes(daemon: &Daemon) -> usize {
    daemon.server().presentes.lock().await.todos().len()
}

/// O primeiro canal de texto de uma entrada.
fn canal(entrada: &Entrada) -> u32 {
    entrada
        .snapshot
        .channels
        .first()
        .map(|canal| canal.id)
        .expect("o servidor não tem canal nenhum")
}

/// Abre o primeiro canal de texto da sessão, como a janela faz ao entrar: a
/// conversa que a FFI guarda é a do canal aberto.
fn abrir_o_canal(app: &AppHandle<MockRuntime>, entrada: &Entrada) {
    super::open_channel(app.state::<Session>(), carimbo(entrada), canal(entrada))
        .expect("abrir o canal");
}

/// O que a janela recebeu pelos dois canais, guardado para conferir.
#[derive(Default)]
struct Ouvido {
    eventos: Mutex<Vec<serde_json::Value>>,
    trilha: Mutex<Vec<serde_json::Value>>,
}

impl Ouvido {
    fn ligar(app: &AppHandle<MockRuntime>) -> Arc<Self> {
        let ouvido = Arc::new(Self::default());
        let dos_eventos = Arc::clone(&ouvido);
        app.listen_any(EVENT_CHANNEL, move |evento| {
            if let Ok(carga) = serde_json::from_str(evento.payload()) {
                dos_eventos.eventos.lock().expect("os eventos").push(carga);
            }
        });
        let da_trilha = Arc::clone(&ouvido);
        app.listen_any(CANAL_DAS_SESSOES, move |evento| {
            if let Ok(carga) = serde_json::from_str(evento.payload()) {
                da_trilha.trilha.lock().expect("a trilha").push(carga);
            }
        });
        ouvido
    }

    fn eventos(&self) -> Vec<serde_json::Value> {
        self.eventos.lock().expect("os eventos").clone()
    }

    fn trilha(&self) -> Vec<serde_json::Value> {
        self.trilha.lock().expect("a trilha").clone()
    }
}

/// Um envio de mensagem como a janela faz, pelo comando.
fn mandar(
    app: &AppHandle<MockRuntime>,
    carimbo: CarimboDaJanela,
    canal: u32,
    corpo: &str,
) -> Result<String, ConnectionError> {
    super::send_message(app.state::<Session>(), carimbo, canal, corpo.to_owned())
}

/// **O cenário inteiro, com hospedagem e três sessões**: (1), (2) e (4), as
/// não lidas, sair de uma, e o servidor hospedado de pé o tempo todo.
#[test]
fn hospedando_tres_sessoes_ficam_de_pe_e_isoladas() {
    let _vez = vez();
    runtime().block_on(async {
        let app = app();
        let session = app.state::<Session>();
        let ouvido = Ouvido::ligar(app);

        // O servidor que esta janela hospeda, com uma convidada dentro dele.
        let hospedado = Hospedagem::iniciar(0, Location::Memory, "Casa", None, |_| Ok(()))
            .await
            .expect("o servidor hospedado não subiu");
        let casa = SocketAddr::from(([127, 0, 0, 1], hospedado.endereco().port()));
        let servidor_da_casa = hospedado.servidor();
        session.hospedagens.lock().expect("as hospedagens").guardado =
            Some(Hospedado::novo(hospedado, None));
        let (convidada, _casa_dela) = de_outra_maquina(casa, "convidada").await;

        // Três servidores de outras pessoas, e uma sessão em cada.
        let (a, servidor_a) = seeled("A", 0, Location::Memory).await;
        let (b, servidor_b) = seeled("B", 0, Location::Memory).await;
        let (c, servidor_c) = seeled("C", 0, Location::Memory).await;
        let em_a = entrar(app, a, "dona").await;
        let em_b = entrar(app, b, "dona").await;
        let em_c = entrar(app, c, "dona").await;
        for entrada in [&em_a, &em_b, &em_c] {
            abrir_o_canal(app, entrada);
        }

        let ids = [
            em_a.carimbo.sessao,
            em_b.carimbo.sessao,
            em_c.carimbo.sessao,
        ];
        assert!(
            ids[0] != ids[1] && ids[1] != ids[2] && ids[0] != ids[2] && !ids.contains(&0),
            "as três sessões não têm SessionId próprio: {ids:?}"
        );
        let na_tela = sessoes_na_tela(&session);
        assert_eq!(
            na_tela.iter().map(|s| s.carimbo.sessao).collect::<Vec<_>>(),
            ids,
            "o registro não tem as três sessões, na ordem de entrada: {na_tela:?}"
        );
        assert!(
            na_tela.iter().all(|s| s.enlace == "online"),
            "entrar na terceira derrubou uma das outras: {na_tela:?}"
        );
        assert_eq!(
            na_tela
                .iter()
                .filter(|s| s.na_vista)
                .map(|s| s.carimbo.sessao)
                .collect::<Vec<_>>(),
            [em_c.carimbo.sessao],
            "a vista não é a última sessão em que a pessoa entrou"
        );
        assert!(
            na_tela.iter().all(|s| s.onde == super::OndeEstou::Outro),
            "uma sessão num servidor de outra pessoa se diz hospedada aqui: {na_tela:?}"
        );
        for (nome, servidor) in [("A", &servidor_a), ("B", &servidor_b), ("C", &servidor_c)] {
            assert_eq!(
                presentes(servidor).await,
                1,
                "o servidor {nome} não tem esta janela presente: entrar noutro a derrubou"
            );
        }

        // **(1) o evento tardio.** A vista vai para A; alguém fala em B.
        let vista_a = ver_sessao_em(app, &session, em_a.carimbo.sessao, None).expect("ver A");
        assert_eq!(vista_a.carimbo.sessao, em_a.carimbo.sessao);
        let (rafa, _casa_da_rafa) = de_outra_maquina(b, "rafa").await;
        let antes = ouvido.eventos().len();
        rafa.send_message(canal(&em_b), "chegou em B".into())
            .expect("a rafa não falou em B");
        let em_b_id = em_b.carimbo.sessao;
        assert!(
            ate(PRAZO, || ouvido.eventos()[antes..].iter().any(|evento| {
                evento["sessao"] == em_b_id && evento["evento"] == "MessagesChanged"
            }))
            .await,
            "a mensagem de B não chegou à janela como evento da sessão B"
        );
        for evento in &ouvido.eventos()[antes..] {
            assert!(
                evento.get("sessao").is_some() && evento.get("geracao").is_some(),
                "um evento chegou à janela sem SessionId e geração: {evento}"
            );
            if evento["sessao"] == em_b_id {
                assert_eq!(
                    evento["vista"], false,
                    "o evento de B saiu como se B estivesse na vista, com A na tela: {evento}"
                );
            }
        }
        assert!(
            ate(PRAZO, || {
                let placas = sessoes_na_tela(&session);
                placas
                    .iter()
                    .any(|s| s.carimbo.sessao == em_b_id && s.mudou && s.nao_lidas >= 1)
            })
            .await,
            "a placa de B não marcou a mensagem que chegou com A na tela: {:?}",
            sessoes_na_tela(&session)
        );
        let placa_a = sessoes_na_tela(&session)
            .into_iter()
            .find(|s| s.carimbo.sessao == em_a.carimbo.sessao)
            .expect("a placa de A");
        assert!(
            !placa_a.mudou && placa_a.nao_lidas == 0,
            "a mensagem de B foi contada em A: {placa_a:?}"
        );
        assert!(
            ouvido
                .trilha()
                .iter()
                .any(|sinal| sinal["sessao"] == em_b_id && sinal["motivo"] == "mensagens"),
            "a trilha não recebeu o sinal da mensagem de B"
        );

        // **(2) a promessa depois da troca.** As mensagens de B, pedidas com o
        // carimbo de B, com a vista indo para C no meio.
        let pedido = {
            let app = app.clone();
            let carimbo_b = carimbo(&em_b);
            std::thread::spawn(move || {
                super::messages(app.state::<Session>(), carimbo_b).expect("as mensagens de B")
            })
        };
        ver_sessao_em(app, &session, em_c.carimbo.sessao, None).expect("ver C");
        let resposta = pedido.join().expect("o pedido das mensagens");
        assert_eq!(
            resposta.carimbo.sessao, em_b_id,
            "a resposta pedida para B voltou carimbada com outra sessão"
        );
        assert!(
            resposta.valor.iter().any(|m| m.body == "chegou em B"),
            "a resposta pedida para B não tem a conversa de B: {:?}",
            resposta.valor
        );

        // **(4) o clique durante a troca.** Um envio com o carimbo de C sai
        // junto com a troca da vista para A.
        let mut ouvidos = [
            ("A", servidor_a.server().events.subscribe()),
            ("B", servidor_b.server().events.subscribe()),
            ("C", servidor_c.server().events.subscribe()),
        ];
        let clique = {
            let app = app.clone();
            let carimbo_c = carimbo(&em_c);
            let canal_c = canal(&em_c);
            std::thread::spawn(move || mandar(&app, carimbo_c, canal_c, "clicado em C"))
        };
        ver_sessao_em(app, &session, em_a.carimbo.sessao, None).expect("ver A");
        clique
            .join()
            .expect("o clique")
            .expect("o clique em C foi recusado sem motivo");
        let receberam = quem_recebeu(&mut ouvidos, "clicado em C", Duration::from_secs(2)).await;
        assert_eq!(
            receberam,
            ["C"],
            "o clique feito em C, durante a troca da vista para A, foi a {receberam:?}"
        );
        // Um carimbo de antes de uma reconexão (geração que a sessão não tem)
        // é recusado, e nada sai.
        let velho = CarimboDaJanela {
            sessao: em_c.carimbo.sessao,
            geracao: em_c.carimbo.geracao + 7,
        };
        match mandar(app, velho, canal(&em_c), "carimbo velho") {
            Err(ConnectionError::SessaoNaoConfere { .. }) => {}
            outro => panic!("um carimbo que não é o de agora não foi recusado: {outro:?}"),
        }
        let receberam = quem_recebeu(&mut ouvidos, "carimbo velho", Duration::from_secs(1)).await;
        assert!(
            receberam.is_empty(),
            "o carimbo recusado ainda mandou a {receberam:?}"
        );

        // **Sair de uma não toca nas outras nem no hospedado.**
        disconnect(app.clone(), app.state::<Session>(), carimbo(&em_a))
            .await
            .expect("o `disconnect` nunca falha");
        assert!(
            ate(PRAZO, || {
                sessoes_na_tela(&session)
                    .iter()
                    .all(|s| s.carimbo.sessao != em_a.carimbo.sessao)
            })
            .await,
            "a sessão que saiu continua no registro"
        );
        let fim = tokio::time::Instant::now() + PRAZO;
        while presentes(&servidor_a).await != 0 && tokio::time::Instant::now() < fim {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(
            presentes(&servidor_a).await,
            0,
            "o servidor A ainda vê a sessão que saiu"
        );
        match mandar(app, carimbo(&em_a), canal(&em_a), "depois de sair") {
            Err(ConnectionError::NotConnected) => {}
            outro => panic!("um comando para a sessão que saiu não foi recusado: {outro:?}"),
        }
        let mut ouvidos = [
            ("B", servidor_b.server().events.subscribe()),
            ("C", servidor_c.server().events.subscribe()),
        ];
        mandar(app, carimbo(&em_b), canal(&em_b), "B continua").expect("B caiu junto com A");
        assert_eq!(
            quem_recebeu(&mut ouvidos, "B continua", Duration::from_secs(2)).await,
            ["B"]
        );
        mandar(app, carimbo(&em_c), canal(&em_c), "C continua").expect("C caiu junto com A");
        assert_eq!(
            quem_recebeu(&mut ouvidos, "C continua", Duration::from_secs(2)).await,
            ["C"]
        );
        assert_eq!(
            sessoes_na_tela(&session)
                .iter()
                .filter(|s| s.enlace == "online")
                .count(),
            2,
            "sair de A derrubou outra sessão"
        );

        // O hospedado esteve de pé o tempo todo, com a convidada dentro.
        let mut da_casa = servidor_da_casa.events.subscribe();
        let canal_da_casa = convidada
            .snapshot()
            .channels
            .first()
            .map(|canal| canal.id)
            .expect("o canal da casa");
        convidada
            .send_message(canal_da_casa, "a casa continua".into())
            .expect("a convidada não fala mais no hospedado");
        let chegou = tokio::time::timeout(PRAZO, async {
            loop {
                if let Ok(seele_server::server::Event::MessagePosted(m)) = da_casa.recv().await {
                    if m.body == "a casa continua" {
                        break;
                    }
                }
            }
        })
        .await;
        assert!(
            chegou.is_ok(),
            "o servidor hospedado não recebeu a convidada depois das três sessões e da saída"
        );
        assert!(
            session
                .hospedagens
                .lock()
                .expect("as hospedagens")
                .guardado
                .is_some(),
            "sair de uma sessão derrubou o servidor hospedado"
        );

        for entrada in [&em_b, &em_c] {
            disconnect(app.clone(), app.state::<Session>(), carimbo(entrada))
                .await
                .expect("o `disconnect` nunca falha");
        }
        convidada.disconnect();
        rafa.disconnect();
        let _ = super::parar_de_hospedar_em(&session).await;
        for servidor in [servidor_a, servidor_b, servidor_c] {
            servidor.shutdown();
        }
    });
}

/// **O anexo de A em andamento durante a troca termina em A, e nunca vai para
/// B.** O envio sai com o carimbo de A; a vista vai para B logo em seguida,
/// com os bytes ainda subindo; o servidor de A publica a mensagem do anexo, o
/// de B não recebe nada, e o andamento do anexo chega à janela carimbado com A.
#[test]
fn o_anexo_de_a_durante_a_troca_termina_em_a() {
    let _vez = vez();
    runtime().block_on(async {
        let app = app();
        let session = app.state::<Session>();
        let ouvido = Ouvido::ligar(app);
        // Bancos em disco: um banco em memória não tem onde guardar anexos.
        let pasta_a = tempfile::tempdir().expect("a pasta de A");
        let pasta_b = tempfile::tempdir().expect("a pasta de B");
        let (a, servidor_a) = seeled("A", 0, Location::File(pasta_a.path().join("seele.db"))).await;
        let (b, servidor_b) = seeled("B", 0, Location::File(pasta_b.path().join("seele.db"))).await;
        let em_a = entrar(app, a, "dona").await;
        let em_b = entrar(app, b, "dona").await;
        ver_sessao_em(app, &session, em_a.carimbo.sessao, None).expect("ver A");

        // Um arquivo de verdade, grande o bastante para ainda estar subindo
        // quando a vista troca.
        let pasta = tempfile::tempdir().expect("a pasta do anexo");
        let arquivo = pasta.path().join("relatorio.txt");
        std::fs::write(&arquivo, vec![b'a'; 2 * 1024 * 1024]).expect("o arquivo do anexo");

        let mut ouvidos = [
            ("A", servidor_a.server().events.subscribe()),
            ("B", servidor_b.server().events.subscribe()),
        ];
        let antes = ouvido.eventos().len();
        let anexo = super::enviar_anexo(
            app.state::<Session>(),
            carimbo(&em_a),
            canal(&em_a),
            "o anexo de A".to_owned(),
            arquivo.display().to_string(),
            "relatorio.txt".to_owned(),
            "text/plain".to_owned(),
        )
        .expect("o anexo de A não começou");
        // A troca, com o anexo no ar.
        ver_sessao_em(app, &session, em_b.carimbo.sessao, None).expect("ver B");
        let receberam = quem_recebeu(&mut ouvidos, "o anexo de A", Duration::from_secs(6)).await;
        assert_eq!(
            receberam,
            ["A"],
            "o anexo {anexo} de A, com a vista trocada para B no meio, foi a {receberam:?}"
        );
        let do_anexo: Vec<_> = ouvido.eventos()[antes..]
            .iter()
            .filter(|evento| evento["evento"].get("TransferChanged").is_some())
            .cloned()
            .collect();
        assert!(
            !do_anexo.is_empty(),
            "o andamento do anexo não chegou à janela"
        );
        assert!(
            do_anexo
                .iter()
                .all(|evento| evento["sessao"] == em_a.carimbo.sessao),
            "um andamento do anexo de A chegou à janela com outra sessão: {do_anexo:?}"
        );

        for entrada in [&em_a, &em_b] {
            disconnect(app.clone(), app.state::<Session>(), carimbo(entrada))
                .await
                .expect("o `disconnect` nunca falha");
        }
        servidor_a.shutdown();
        servidor_b.shutdown();
    });
}

/// O MOD de referência do produto, que tem as duas metades.
const MOD_DE_REFERENCIA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testes/mod-de-referencia");

/// **O consentimento de MOD de A, respondido depois da troca, vai para A.**
///
/// A exige um MOD; a entrada nele pergunta (`ModsNaoAceitos`). A pessoa troca a
/// vista para B, que já estava conectada, e só então responde. O sim fica com
/// A — e B, que não exige nada, continua sem sim nenhum e de pé —, e a entrada
/// seguinte em A passa.
#[test]
fn o_sim_aos_mods_de_a_respondido_depois_da_troca_vai_para_a() {
    use seele_server::mods::instalar::{instalar_de_pasta, raizes_de_um_mundo};
    use seele_server::persistence::mods::{enable, EnabledMod};
    use seele_server::persistence::Persistence;

    let _vez = vez();
    runtime().block_on(async {
        let app = app();
        let session = app.state::<Session>();

        // A: um servidor com o MOD de referência ligado.
        let mundo = tempfile::tempdir().expect("a pasta de A");
        let raizes = raizes_de_um_mundo(mundo.path());
        let (manifesto, hash) = instalar_de_pasta(std::path::Path::new(MOD_DE_REFERENCIA), &raizes)
            .expect("o MOD de referência não instalou");
        let banco = Location::File(mundo.path().join("seele.db"));
        enable(
            &Persistence::open(&banco).expect("o banco de A"),
            &EnabledMod {
                id: manifesto.id.clone(),
                version: manifesto.version.clone(),
                hash,
                repo: manifesto.repo.clone(),
                reach: manifesto.reach.clone(),
                server_half: manifesto.server.is_some(),
                api: manifesto.api,
                tema: manifesto.tema,
            },
        )
        .expect("o MOD não ligou em A");
        let servidor_a = Hospedagem::iniciar(0, banco, "A", Some(raizes), |_| Ok(()))
            .await
            .expect("A não subiu");
        let a = format!("127.0.0.1:{}", servidor_a.endereco().port());

        // B: um servidor sem MOD, com a sessão de pé.
        let (b, servidor_b) = seeled("B", 0, Location::Memory).await;
        let em_b = entrar(app, b, "dona").await;

        // A pergunta.
        let pergunta = connect(
            app.clone(),
            app.state::<Session>(),
            a.clone(),
            "dona".to_owned(),
            false,
            None,
            None,
        )
        .await;
        let Err(falha) = pergunta else {
            panic!("A exige um MOD e a entrada não perguntou");
        };
        let dita = serde_json::to_value(&falha).expect("a falha em JSON");
        let conjunto = dita
            .pointer("/error/ModsNaoAceitos/conjunto")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("a entrada em A não perguntou pelos MODs: {dita}"))
            .to_owned();

        // A vista vai para B **antes** de a pessoa responder.
        ver_sessao_em(app, &session, em_b.carimbo.sessao, None).expect("ver B");
        // O corpo de `aceitar_mods` (o comando é só do runtime de verdade): o
        // sim vai ao alvo que a pergunta trouxe, e nunca à sessão da vista.
        let casa = super::config_dir(app);
        seele_ffi::mods::aceitar(&casa, &a, &conjunto).expect("o sim não gravou");
        assert_eq!(
            seele_ffi::mods::aceite_de(&casa, &a).as_deref(),
            Some(conjunto.as_str()),
            "o sim dado a A depois da troca não ficou com A"
        );
        assert_eq!(
            seele_ffi::mods::aceite_de(&casa, &b.to_string()),
            None,
            "o sim dado a A depois da troca foi parar em B, que está na vista"
        );
        // A entrada seguinte em A passa, e B segue de pé ao lado.
        let em_a = tokio::time::timeout(
            PRAZO,
            connect(
                app.clone(),
                app.state::<Session>(),
                a.clone(),
                "dona".to_owned(),
                false,
                None,
                None,
            ),
        )
        .await
        .expect("a segunda entrada em A não voltou")
        .unwrap_or_else(|falha| panic!("com o sim, A ainda não deixou entrar: {falha:?}"));
        let placas = sessoes_na_tela(&session);
        assert_eq!(placas.len(), 2, "A e B não estão as duas de pé: {placas:?}");
        assert!(placas.iter().all(|placa| placa.enlace == "online"));

        for entrada in [&em_a, &em_b] {
            disconnect(app.clone(), app.state::<Session>(), carimbo(entrada))
                .await
                .expect("o `disconnect` nunca falha");
        }
        servidor_a.encerrar().await;
        servidor_b.shutdown();
    });
}

/// **As três corridas da troca de vista** (revisão do Codex), contra servidores
/// de verdade:
///
/// 1. `ver_sessao` fora de ordem: o pedido 2 (para C) chega antes do pedido 1
///    (para B); o 1 é recusado, e a vista do backend fica em C;
/// 2. A→B→A: a época sobe a cada troca, a resposta da primeira A não vale na
///    segunda (um comando de MOD com aquela época é recusado), e uma resposta
///    pedida agora vem com a época de agora;
/// 3. o retry depois de `SessaoNaoConfere`, resolvido depois da troca: o
///    carimbo velho é recusado, a vista troca, e o retry com o carimbo novo
///    (lido da trilha) vai à sessão de origem, e não à da vista.
#[test]
fn a_vista_so_anda_para_a_frente_e_o_retry_vai_a_sessao_de_origem() {
    let _vez = vez();
    runtime().block_on(async {
        let app = app();
        let session = app.state::<Session>();
        let (a, servidor_a) = seeled("A", 0, Location::Memory).await;
        let (b, servidor_b) = seeled("B", 0, Location::Memory).await;
        let (c, servidor_c) = seeled("C", 0, Location::Memory).await;
        let em_a = entrar(app, a, "dona").await;
        let em_b = entrar(app, b, "dona").await;
        let em_c = entrar(app, c, "dona").await;

        // A instância desta carga da janela, como a janela a pede ao carregar.
        let instancia = super::instancia_da_janela(app.state::<Session>());
        let p = |n| Some(super::sessoes::PedidoDeVista { instancia, n });

        // 1. Fora de ordem: o pedido 2 vai antes do 1.
        let para_c =
            ver_sessao_em(app, &session, em_c.carimbo.sessao, p(2)).expect("o pedido 2, para C");
        match ver_sessao_em(app, &session, em_b.carimbo.sessao, p(1)) {
            Err(super::FalhaAoVer::PedidoVelho { pedido, ultimo })
                if pedido.n == 1 && ultimo.n == 2 => {}
            outro => panic!("o pedido 1, atrasado, não foi recusado: {outro:?}"),
        }
        assert_eq!(
            session.sessoes.vista(),
            Some(em_c.carimbo.sessao),
            "o pedido atrasado levou a vista do backend para trás"
        );
        assert_eq!(
            session.geracao(),
            para_c.geracao,
            "o pedido recusado mexeu na época da vista"
        );

        // 2. A→B→A: três épocas, e a da primeira A não vale na segunda.
        let a1 = ver_sessao_em(app, &session, em_a.carimbo.sessao, p(3)).expect("A");
        let b1 = ver_sessao_em(app, &session, em_b.carimbo.sessao, p(4)).expect("B");
        let a2 = ver_sessao_em(app, &session, em_a.carimbo.sessao, p(5)).expect("A de novo");
        assert!(
            para_c.geracao < a1.geracao && a1.geracao < b1.geracao && b1.geracao < a2.geracao,
            "a época da vista não subiu a cada troca: {} {} {} {}",
            para_c.geracao,
            a1.geracao,
            b1.geracao,
            a2.geracao
        );
        assert!(
            session.confere_geracao(a1.geracao).is_err(),
            "a época da primeira A ainda vale na segunda: um MOD daquela vista falaria nesta"
        );
        assert!(session.confere_geracao(a2.geracao).is_ok());
        let resposta =
            super::messages(app.state::<Session>(), carimbo(&em_a)).expect("as mensagens de A");
        assert_eq!(
            resposta.epoca, a2.geracao,
            "a resposta pedida agora não traz a época de agora"
        );
        assert_eq!(resposta.carimbo.sessao, em_a.carimbo.sessao);

        // 3. O retry depois de `SessaoNaoConfere`, resolvido depois da troca.
        let velho = CarimboDaJanela {
            sessao: em_b.carimbo.sessao,
            geracao: em_b.carimbo.geracao + 1,
        };
        let recusa = mandar(app, velho, canal(&em_b), "primeira tentativa em B");
        let Err(ConnectionError::SessaoNaoConfere { esperado, .. }) = recusa else {
            panic!("o carimbo velho de B não foi recusado: {recusa:?}");
        };
        assert_eq!(esperado.sessao, em_b.carimbo.sessao);
        // A vista vai para C antes do retry.
        ver_sessao_em(app, &session, em_c.carimbo.sessao, p(6)).expect("C");
        let novo = sessoes_na_tela(&session)
            .into_iter()
            .find(|placa| placa.carimbo.sessao == em_b.carimbo.sessao)
            .map(|placa| CarimboDaJanela::de(placa.carimbo))
            .expect("a placa de B");
        let mut ouvidos = [
            ("A", servidor_a.server().events.subscribe()),
            ("B", servidor_b.server().events.subscribe()),
            ("C", servidor_c.server().events.subscribe()),
        ];
        mandar(app, novo, canal(&em_b), "o retry de B").expect("o retry com o carimbo novo");
        assert_eq!(
            quem_recebeu(&mut ouvidos, "o retry de B", Duration::from_secs(2)).await,
            ["B"],
            "o retry de B, feito com C na vista, não foi só a B"
        );

        for entrada in [&em_a, &em_b, &em_c] {
            disconnect(app.clone(), app.state::<Session>(), carimbo(entrada))
                .await
                .expect("o `disconnect` nunca falha");
        }
        for servidor in [servidor_a, servidor_b, servidor_c] {
            servidor.shutdown();
        }
    });
}

/// A sala de voz em que o servidor tem esta pessoa.
async fn sala_no_servidor(daemon: &Daemon, quem: u64) -> Option<u32> {
    daemon
        .server()
        .occupancy
        .lock()
        .await
        .onde_esta(quem.into())
        .map(|sala| sala.get())
}

/// A abertura do áudio de uma sessão, pelo retrato dela.
fn abertura(session: &Session, entrada: &Entrada) -> Option<AberturaDoAudio> {
    session
        .sessoes
        .buscar(entrada.carimbo.sessao)
        .map(|sessao| sessao.conexao.snapshot().abertura_do_audio)
}

/// **O foco de voz entre três sessões**: a primeira o tem sozinha; entrar
/// numa sala de outra o traz, e a anterior sai da sala dela antes; um
/// `MovePerson` não o toma; trocar a vista não o toma; quem sai deixa o foco
/// vazio.
#[test]
fn o_foco_de_voz_e_um_so_e_so_muda_por_ato_da_pessoa() {
    let _vez = vez();
    runtime().block_on(async {
        let app = app();
        let session = app.state::<Session>();
        let (a, servidor_a) = seeled("A", 0, Location::Memory).await;
        let (b, servidor_b) = seeled("B", 0, Location::Memory).await;
        let (c, servidor_c) = seeled("C", 0, Location::Memory).await;
        let em_a = entrar(app, a, "dona").await;
        assert_eq!(
            super::foco_de_voz(app.state::<Session>()),
            Some(em_a.carimbo.sessao),
            "a única sessão não ficou com a voz"
        );
        let em_b = entrar(app, b, "dona").await;
        let em_c = entrar(app, c, "dona").await;
        assert_eq!(
            super::foco_de_voz(app.state::<Session>()),
            Some(em_a.carimbo.sessao),
            "conectar outras sessões tirou a voz da primeira: conectar não é falar"
        );

        // A entra numa sala: com o foco, «abriria» (sem placa de som aqui).
        let sala_a = em_a.snapshot.voice_rooms[0].id;
        entrar_na_sala_em(app, &session, carimbo(&em_a), sala_a, None)
            .await
            .expect("entrar na sala de A");
        let eu_em_a = em_a.snapshot.me.expect("quem sou em A");
        assert!(
            ate(PRAZO, || abertura(&session, &em_a)
                == Some(AberturaDoAudio::SemAparelho))
            .await,
            "A, com o foco e sentada, não chegou a abrir o áudio: {:?}",
            abertura(&session, &em_a)
        );

        // **C senta numa sala sem ato da pessoa na janela**: pela conexão, e não
        // pelo `enter_voice_room` da casca, que é o ato que traz a voz. É o que
        // um `MovePerson` de um operador de lá faz chegar à FFI. (Desde os
        // cargos do E6, a primeira conta não é mais Comandante, e esta janela
        // não pode mover a si mesma em C.)
        let eu_em_c = em_c.snapshot.me.expect("quem sou em C");
        let sala_c = em_c.snapshot.voice_rooms[0].id;
        session
            .sessoes
            .buscar(em_c.carimbo.sessao)
            .expect("a sessão C")
            .conexao
            .enter_voice_room(sala_c, None)
            .expect("sentar em C");
        let fim = tokio::time::Instant::now() + PRAZO;
        while sala_no_servidor(&servidor_c, eu_em_c).await != Some(sala_c)
            && tokio::time::Instant::now() < fim
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(sala_no_servidor(&servidor_c, eu_em_c).await, Some(sala_c));
        assert!(
            ate(PRAZO, || abertura(&session, &em_c)
                == Some(AberturaDoAudio::EmOutraSessao))
            .await,
            "C sentada sem o foco não diz que a voz está noutra sessão: {:?}",
            abertura(&session, &em_c)
        );
        assert_eq!(
            super::foco_de_voz(app.state::<Session>()),
            Some(em_a.carimbo.sessao),
            "ser movida para uma sala tomou o microfone para C"
        );

        // Trocar a vista não toma o foco.
        ver_sessao_em(app, &session, em_b.carimbo.sessao, None).expect("ver B");
        assert_eq!(
            super::foco_de_voz(app.state::<Session>()),
            Some(em_a.carimbo.sessao),
            "olhar B tomou o microfone para B"
        );

        // **B entra numa sala: o ato que traz a voz.** A sai da dela antes.
        let sala_b = em_b.snapshot.voice_rooms[0].id;
        entrar_na_sala_em(app, &session, carimbo(&em_b), sala_b, None)
            .await
            .expect("entrar na sala de B");
        assert_eq!(
            super::foco_de_voz(app.state::<Session>()),
            Some(em_b.carimbo.sessao),
            "entrar numa sala de B não trouxe a voz para B"
        );
        let fim = tokio::time::Instant::now() + PRAZO;
        while sala_no_servidor(&servidor_a, eu_em_a).await.is_some()
            && tokio::time::Instant::now() < fim
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(
            sala_no_servidor(&servidor_a, eu_em_a).await,
            None,
            "A perdeu a voz e continuou sentada na sala dela: os outros a veem presente, \
             sem ouvir nem falar"
        );
        assert!(
            ate(PRAZO, || abertura(&session, &em_b)
                == Some(AberturaDoAudio::SemAparelho))
            .await,
            "B, com o foco e sentada, não chegou a abrir o áudio: {:?}",
            abertura(&session, &em_b)
        );
        assert_eq!(
            abertura(&session, &em_a),
            Some(AberturaDoAudio::ForaDeSala),
            "A ficou com o estado de quem ainda tem áudio"
        );
        assert_eq!(
            abertura(&session, &em_c),
            Some(AberturaDoAudio::EmOutraSessao),
            "a troca de A para B mexeu em C"
        );
        let placas = sessoes_na_tela(&session);
        assert_eq!(
            placas
                .iter()
                .filter(|s| s.foco_de_voz)
                .map(|s| s.carimbo.sessao)
                .collect::<Vec<_>>(),
            [em_b.carimbo.sessao],
            "a trilha não mostra exatamente uma sessão com a voz: {placas:?}"
        );

        // `focar_voz` com o carimbo de C: explícito, e C (sentada) passa a
        // «abrir»; B sai da sala.
        let troca = focar_voz_em(app, &session, Some(carimbo(&em_c)))
            .await
            .expect("focar C");
        assert_eq!(troca.antes, Some(em_b.carimbo.sessao));
        assert_eq!(troca.agora, Some(em_c.carimbo.sessao));
        assert!(
            troca.saiu_da_sala.is_some(),
            "a troca não disse de que sala B saiu"
        );
        assert!(
            ate(PRAZO, || abertura(&session, &em_c)
                == Some(AberturaDoAudio::SemAparelho))
            .await,
            "C ganhou o foco sentada e não chegou a abrir: {:?}",
            abertura(&session, &em_c)
        );

        // Quem sai com o foco deixa o foco vazio: ele não passa a ninguém.
        disconnect(app.clone(), app.state::<Session>(), carimbo(&em_c))
            .await
            .expect("o `disconnect` nunca falha");
        assert!(
            ate(PRAZO, || super::foco_de_voz(app.state::<Session>())
                .is_none())
            .await,
            "a sessão com a voz saiu e o foco passou sozinho a {:?}",
            super::foco_de_voz(app.state::<Session>())
        );
        // E uma entrada nova, depois da saída do dono, não rouba o foco vazio
        // (a mesma semântica da web).
        let de_volta_em_c = entrar(app, c, "dona").await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            super::foco_de_voz(app.state::<Session>()),
            None,
            "uma conexão nova, depois de o dono sair, tomou o foco sozinha"
        );
        disconnect(app.clone(), app.state::<Session>(), carimbo(&de_volta_em_c))
            .await
            .expect("o `disconnect` nunca falha");

        for entrada in [&em_a, &em_b] {
            disconnect(app.clone(), app.state::<Session>(), carimbo(entrada))
                .await
                .expect("o `disconnect` nunca falha");
        }
        for servidor in [servidor_a, servidor_b, servidor_c] {
            servidor.shutdown();
        }
    });
}

/// **(3) A reconexão em segundo plano, e o isolamento da queda.** O servidor de
/// C (sem foco, fora da vista) cai e volta na mesma porta, com o mesmo banco:
/// durante a queda, A, B e o servidor hospedado seguem; na volta, C tem a
/// geração nova, e a vista e o foco de voz são os de antes.
#[test]
fn a_queda_e_a_volta_de_uma_sessao_no_fundo_nao_mexe_nas_outras() {
    let _vez = vez();
    runtime().block_on(async {
        let app = app();
        let session = app.state::<Session>();
        let ouvido = Ouvido::ligar(app);
        let hospedado = Hospedagem::iniciar(0, Location::Memory, "Casa", None, |_| Ok(()))
            .await
            .expect("o servidor hospedado não subiu");
        let casa = SocketAddr::from(([127, 0, 0, 1], hospedado.endereco().port()));
        session.hospedagens.lock().expect("as hospedagens").guardado =
            Some(Hospedado::novo(hospedado, None));
        let (convidada, _casa_dela) = de_outra_maquina(casa, "convidada").await;

        let pasta_de_c = tempfile::tempdir().expect("a pasta de C");
        let banco_de_c = Location::File(pasta_de_c.path().join("seele.db"));
        let (a, servidor_a) = seeled("A", 0, Location::Memory).await;
        let (b, servidor_b) = seeled("B", 0, Location::Memory).await;
        let (c, servidor_c) = seeled("C", 0, banco_de_c.clone()).await;
        let em_a = entrar(app, a, "dona").await;
        let em_b = entrar(app, b, "dona").await;
        let em_c = entrar(app, c, "dona").await;
        ver_sessao_em(app, &session, em_a.carimbo.sessao, None).expect("ver A");
        // Daqui em diante C está fora da vista: só os eventos dela depois disto
        // contam para «nenhum saiu como da vista».
        let desde_a_vista_em_a = ouvido.eventos().len();
        let foco_antes = super::foco_de_voz(app.state::<Session>());
        assert_eq!(foco_antes, Some(em_a.carimbo.sessao));
        let carimbos_antes: Vec<_> = sessoes_na_tela(&session)
            .iter()
            .map(|s| s.carimbo)
            .collect();

        // O servidor de C cai sem despedida.
        servidor_c.shutdown();
        servidor_c.wait_idle().await;
        drop(servidor_c);
        let id_c = em_c.carimbo.sessao;
        assert!(
            ate(Duration::from_secs(40), || sessoes_na_tela(&session)
                .iter()
                .any(
                    |s| s.carimbo.sessao == id_c && s.enlace == "reconectando"
                ))
            .await,
            "a sessão C não percebeu a queda do servidor dela: {:?}",
            sessoes_na_tela(&session)
        );
        // **Durante a queda**: A, B e o hospedado seguem.
        let mut ouvidos = [
            ("A", servidor_a.server().events.subscribe()),
            ("B", servidor_b.server().events.subscribe()),
        ];
        mandar(app, carimbo(&em_a), canal(&em_a), "A durante a queda de C").expect("A caiu com C");
        assert_eq!(
            quem_recebeu(
                &mut ouvidos,
                "A durante a queda de C",
                Duration::from_secs(2)
            )
            .await,
            ["A"]
        );
        mandar(app, carimbo(&em_b), canal(&em_b), "B durante a queda de C").expect("B caiu com C");
        assert_eq!(
            quem_recebeu(
                &mut ouvidos,
                "B durante a queda de C",
                Duration::from_secs(2)
            )
            .await,
            ["B"]
        );
        assert!(
            convidada.snapshot().link == seele_ffi::LinkState::Online,
            "a queda de C mexeu no servidor hospedado"
        );

        // C volta na mesma porta, com o mesmo banco (o mesmo certificado).
        let (_, de_novo) = seeled("C", c.port(), banco_de_c).await;
        assert!(
            ate(Duration::from_secs(60), || sessoes_na_tela(&session)
                .iter()
                .any(|s| s.carimbo.sessao == id_c && s.enlace == "online"))
            .await,
            "C não voltou depois de o servidor dela voltar: {:?}",
            sessoes_na_tela(&session)
        );
        let depois = sessoes_na_tela(&session);
        let c_agora = depois
            .iter()
            .find(|s| s.carimbo.sessao == id_c)
            .expect("a placa de C");
        assert!(
            c_agora.carimbo.geracao > em_c.carimbo.geracao,
            "C voltou sem a geração nova: {:?}",
            c_agora.carimbo
        );
        for antes in carimbos_antes
            .iter()
            .filter(|carimbo| carimbo.sessao != id_c)
        {
            let agora = depois
                .iter()
                .find(|s| s.carimbo.sessao == antes.sessao)
                .expect("uma sessão que não caiu sumiu");
            assert_eq!(
                agora.carimbo, *antes,
                "a volta de C mexeu no carimbo de outra sessão"
            );
            assert_eq!(agora.enlace, "online");
        }
        assert_eq!(
            session.sessoes.vista(),
            Some(em_a.carimbo.sessao),
            "a volta de C trocou a vista"
        );
        assert_eq!(
            super::foco_de_voz(app.state::<Session>()),
            foco_antes,
            "a volta de C mexeu no foco de voz"
        );
        assert!(
            ouvido.eventos()[desde_a_vista_em_a..]
                .iter()
                .filter(|evento| evento["sessao"] == id_c)
                .all(|evento| evento["vista"] == false),
            "um evento de C, fora da vista, saiu como da vista"
        );
        // O carimbo de antes da volta é recusado; o novo passa.
        match mandar(
            app,
            carimbo(&em_c),
            canal(&em_c),
            "carimbo de antes da volta",
        ) {
            Err(ConnectionError::SessaoNaoConfere { .. }) => {}
            outro => panic!("o carimbo de antes da volta de C não foi recusado: {outro:?}"),
        }
        let mut de_c = de_novo.server().events.subscribe();
        mandar(
            app,
            CarimboDaJanela::de(c_agora.carimbo),
            canal(&em_c),
            "C voltou",
        )
        .expect("C voltou e não fala");
        let chegou = tokio::time::timeout(PRAZO, async {
            loop {
                if let Ok(seele_server::server::Event::MessagePosted(m)) = de_c.recv().await {
                    if m.body == "C voltou" {
                        break;
                    }
                }
            }
        })
        .await;
        assert!(
            chegou.is_ok(),
            "C voltou e a mensagem dela não chegou ao servidor de C"
        );

        for entrada in [&em_a, &em_b, &em_c] {
            disconnect(app.clone(), app.state::<Session>(), carimbo(entrada))
                .await
                .expect("o `disconnect` nunca falha");
        }
        convidada.disconnect();
        let _ = super::parar_de_hospedar_em(&session).await;
        servidor_a.shutdown();
        servidor_b.shutdown();
        de_novo.shutdown();
    });
}

/// Quem ouve, de outra máquina: um `Enlace` de verdade sentado na sala padrão,
/// contando os datagramas de voz que o servidor encaminha a ele.
async fn quem_ouve(
    alvo: SocketAddr,
    contados: Arc<std::sync::atomic::AtomicUsize>,
) -> seele_core::enlace::Enlace {
    let enlace = seele_core::enlace::Enlace::conectar(
        seele_core::enlace::Destino {
            servidor: alvo,
            nome_tls: "localhost".into(),
            chave_do_pin: alvo.to_string(),
            apelido: "ouve".into(),
            segredo: None,
            esperada: None,
            o_pino_prova_o_servidor: true,
            aceito: None,
        },
        seele_core::SigningKey::from_bytes(&[90; 32]),
        Arc::new(seele_core::MemoryPinStore::new()) as Arc<dyn seele_core::PinStore>,
    )
    .await
    .expect("quem ouve não entrou");
    let sala = enlace.sessao().voice_rooms[0].id;
    enlace
        .entrar_na_voice_room(sala, None)
        .await
        .expect("quem ouve não sentou");
    let canal = enlace.media();
    tokio::spawn(async move {
        while canal.next().await.is_ok() {
            contados.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    });
    enlace
}

/// Os datagramas que chegam a quem ouve numa janela de tempo.
async fn chegam_em(contados: &std::sync::atomic::AtomicUsize, janela: Duration) -> usize {
    let antes = contados.load(std::sync::atomic::Ordering::SeqCst);
    tokio::time::sleep(janela).await;
    contados.load(std::sync::atomic::Ordering::SeqCst) - antes
}

/// **Sair do servidor inteiro e voltar devolve a voz** (o relato do usuário de
/// 25/10: a tela some, a pessoa desconecta e reconecta o servidor, e a voz
/// para), pelos comandos da janela: `connect` com áudio, `enter_voice_room`
/// (`entrar_na_sala_em`), `disconnect`, `connect` de novo, `enter_voice_room`.
/// O aparelho é o de mentira (um tom; nunca o microfone de verdade), gravado
/// nas preferências da casa do teste como o microfone e a saída escolhidos; o
/// que se mede é a voz que chega a quem ouve, no fio.
#[test]
fn sair_do_servidor_inteiro_e_voltar_devolve_a_voz_pelos_comandos_da_janela() {
    use seele_audio::device::mentira;
    const APARELHO: &str = "app-reconectar";
    let _vez = vez();
    runtime().block_on(async {
        let app = app();
        let session = app.state::<Session>();
        // O aparelho de mentira e a voz aberta, como a pessoa os escolheria.
        let caminho = std::path::PathBuf::from(super::config_dir(app)).join("preferences");
        {
            let mut preferencias = seele_ffi::preferences::Preferences::open(caminho.clone())
                .expect("as preferências");
            preferencias
                .set_capture(Some(&mentira::id(APARELHO)))
                .expect("o microfone de mentira");
            preferencias
                .set_playback(Some(&mentira::id(APARELHO)))
                .expect("a saída de mentira");
            preferencias
                .set_voice_mode(Some(seele_core::VoiceMode::Open))
                .expect("a voz aberta");
        }
        let (alvo, servidor) = seeled("Casa", 0, Location::Memory).await;
        let contados = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let ouve = quem_ouve(alvo, Arc::clone(&contados)).await;

        let com_audio = |app: &'static AppHandle<MockRuntime>| async move {
            tokio::time::timeout(
                PRAZO,
                connect(
                    app.clone(),
                    app.state::<Session>(),
                    alvo.to_string(),
                    "dona".to_owned(),
                    true,
                    None,
                    None,
                ),
            )
            .await
            .expect("o `connect` não voltou")
            .unwrap_or_else(|falha| panic!("o `connect` não entrou: {falha:?}"))
        };

        // 1. Entra com áudio e senta: a voz chega.
        let primeira = com_audio(app).await;
        let sala = primeira.snapshot.voice_rooms[0].id;
        entrar_na_sala_em(app, &session, carimbo(&primeira), sala, None)
            .await
            .expect("sentar");
        assert!(
            ate(PRAZO, || mentira::contagem(APARELHO).abertos == 1).await,
            "o aparelho da primeira sessão não abriu"
        );
        let antes = chegam_em(&contados, Duration::from_millis(1500)).await;
        assert!(antes > 0, "a voz da primeira sessão não chegou a quem ouve");

        // 2. Sai do servidor inteiro.
        disconnect(app.clone(), app.state::<Session>(), carimbo(&primeira))
            .await
            .expect("o `disconnect` nunca falha");
        assert!(
            ate(PRAZO, || mentira::contagem(APARELHO).abertos == 0).await,
            "o aparelho da sessão que saiu continuou aberto"
        );

        // 3. Entra de novo e senta pela janela.
        let segunda = com_audio(app).await;
        let sala = segunda.snapshot.voice_rooms[0].id;
        entrar_na_sala_em(app, &session, carimbo(&segunda), sala, None)
            .await
            .expect("sentar de novo");
        let estado = || abertura(&session, &segunda);
        let abriu = ate(PRAZO, || mentira::contagem(APARELHO).abertos == 1).await;
        let depois = chegam_em(&contados, Duration::from_millis(1500)).await;
        println!(
            "voz em 1,5 s: antes de sair {antes}, depois de voltar {depois}; abertura {:?}; \
             foco {:?} (a sessão nova é {})",
            estado(),
            super::foco_de_voz(app.state::<Session>()),
            segunda.carimbo.sessao
        );
        assert!(
            abriu && depois > 0,
            "depois de sair do servidor inteiro e voltar, a voz não chega a quem ouve \
             (abertura {:?}, foco {:?}, a sessão nova {})",
            estado(),
            super::foco_de_voz(app.state::<Session>()),
            segunda.carimbo.sessao
        );

        disconnect(app.clone(), app.state::<Session>(), carimbo(&segunda))
            .await
            .expect("o `disconnect` nunca falha");
        drop(ouve);
        servidor.shutdown();
        // A casa é de todos os testes deste módulo: as preferências voltam.
        let mut preferencias =
            seele_ffi::preferences::Preferences::open(caminho).expect("as preferências");
        let _ = preferencias.set_capture(None);
        let _ = preferencias.set_playback(None);
        let _ = preferencias.set_voice_mode(None);
    });
}

/// **Logo depois de voltar, a sessão nova já se vê sentada?** O botão da sala na
/// janela só chama `enter_voice_room` quando a pessoa NÃO está dentro
/// (`tela-sessao.js`, `alternarCanal`: `dataset.dentro === "sim"` é navegação).
/// Se o retrato da sessão nova já a mostra na sala (a vaga da sessão velha), o
/// clique não traz o foco, e a voz fica `EmOutraSessao`.
#[test]
fn voltar_ao_servidor_nao_deixa_a_pessoa_sentada_sem_a_voz() {
    use seele_audio::device::mentira;
    const APARELHO: &str = "app-reconectar-sentada";
    let _vez = vez();
    runtime().block_on(async {
        let app = app();
        let session = app.state::<Session>();
        let caminho = std::path::PathBuf::from(super::config_dir(app)).join("preferences");
        {
            let mut preferencias =
                seele_ffi::preferences::Preferences::open(caminho.clone()).expect("as preferências");
            preferencias.set_capture(Some(&mentira::id(APARELHO))).expect("o microfone de mentira");
            preferencias.set_playback(Some(&mentira::id(APARELHO))).expect("a saída de mentira");
            preferencias
                .set_voice_mode(Some(seele_core::VoiceMode::Open))
                .expect("a voz aberta");
        }
        let (alvo, servidor) = seeled("Casa", 0, Location::Memory).await;
        let contados = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let ouve = quem_ouve(alvo, Arc::clone(&contados)).await;
        let com_audio = |app: &'static AppHandle<MockRuntime>| async move {
            tokio::time::timeout(
                PRAZO,
                connect(app.clone(), app.state::<Session>(), alvo.to_string(), "dona".to_owned(), true, None, None),
            )
            .await
            .expect("o `connect` não voltou")
            .unwrap_or_else(|falha| panic!("o `connect` não entrou: {falha:?}"))
        };

        let primeira = com_audio(app).await;
        let sala = primeira.snapshot.voice_rooms[0].id;
        entrar_na_sala_em(app, &session, carimbo(&primeira), sala, None)
            .await
            .expect("sentar");
        let eu = primeira.snapshot.me.expect("quem sou");
        assert!(ate(PRAZO, || mentira::contagem(APARELHO).abertos == 1).await);
        disconnect(app.clone(), app.state::<Session>(), carimbo(&primeira))
            .await
            .expect("o `disconnect` nunca falha");

        let segunda = com_audio(app).await;
        let sentada_no_retrato = |retrato: &seele_ffi::Snapshot| {
            retrato
                .voice_rooms
                .iter()
                .any(|sala| sala.people.iter().any(|pessoa| Some(pessoa.id) == retrato.me))
        };
        let no_servidor = sala_no_servidor(&servidor, eu).await;
        let logo = sentada_no_retrato(&segunda.snapshot);
        tokio::time::sleep(Duration::from_millis(800)).await;
        let viva = session
            .sessoes
            .buscar(segunda.carimbo.sessao)
            .map(|sessao| sessao.conexao.snapshot())
            .expect("a sessão nova");
        println!(
            "depois de voltar: o servidor me tem na sala {no_servidor:?}; o retrato da entrada me \
             mostra sentada {logo}; 800 ms depois {}; abertura {:?}; foco {:?}; a sessão nova {}",
            sentada_no_retrato(&viva),
            viva.abertura_do_audio,
            super::foco_de_voz(app.state::<Session>()),
            segunda.carimbo.sessao
        );
        assert!(
            !(sentada_no_retrato(&viva) && viva.abertura_do_audio == AberturaDoAudio::EmOutraSessao),
            "depois de voltar ao servidor, a sessão nova se vê sentada e sem a voz: o botão da sala \
             é navegação para quem está dentro, e nada traz a voz de volta"
        );

        disconnect(app.clone(), app.state::<Session>(), carimbo(&segunda))
            .await
            .expect("o `disconnect` nunca falha");
        drop(ouve);
        servidor.shutdown();
        let mut preferencias =
            seele_ffi::preferences::Preferences::open(caminho).expect("as preferências");
        let _ = preferencias.set_capture(None);
        let _ = preferencias.set_playback(None);
        let _ = preferencias.set_voice_mode(None);
    });
}

/// Quem fala, de outra máquina, pela FFI: com o aparelho de mentira `fala` (o
/// tom), na voz aberta, sentado na sala padrão. É a voz que a pessoa desta
/// janela tem de OUVIR.
async fn quem_fala(alvo: SocketAddr) -> (Arc<Connection>, tempfile::TempDir) {
    use seele_audio::device::mentira;
    let casa = tempfile::tempdir().expect("a casa de quem fala");
    let config = ConnectConfig {
        server: alvo.to_string(),
        alternate_servers: Vec::new(),
        nickname: "fala".into(),
        home: casa.path().display().to_string(),
        join_secret: None,
        expected_fingerprint: None,
        codigo_esperado: None,
        bilhete: None,
        audio: true,
        capture_device: Some(mentira::id("app-quem-fala")),
        playback_device: Some(mentira::id("app-quem-fala")),
    };
    let foco = seele_ffi::foco::FocoDeVoz::novo();
    let (conexao, _) = tokio::task::spawn_blocking(move || {
        Connection::connect_com_foco(config, None, None, Some(foco), None)
    })
    .await
    .expect("a thread de quem fala caiu")
    .unwrap_or_else(|falha| panic!("quem fala não entrou: {falha:?}"));
    conexao.set_voice_mode(seele_ffi::VoiceMode::Open);
    let sala = conexao.snapshot().voice_rooms[0].id;
    conexao
        .enter_voice_room(sala, None)
        .expect("quem fala não sentou");
    (conexao, casa)
}

/// **O caso do usuário, pelo app** (25/10): SAIR do servidor inteiro pelo
/// `disconnect` da janela e VOLTAR por ela, dentro de segundos, e medir por 30 s
/// depois de voltar — o tempo em que a sessão velha ainda pode estar viva no
/// servidor (o ocioso do enlace é 20 s; a leitura do E em pedidos.md).
///
/// Mede, em janelas de 5 s: a sala (o servidor tem a pessoa sentada), o foco,
/// o ENVIO (os datagramas que chegam a quem ouve) e o RECEBIDO e tocado (os
/// blocos com som da saída de mentira desta janela, com a voz de quem fala). E
/// mede, primeiro, se a saída manual fecha a conexão de fato: quanto tempo o
/// servidor ainda tem a pessoa sentada pela sessão velha depois do `disconnect`.
#[test]
fn sair_e_voltar_ao_servidor_pela_janela_mantem_a_voz_nos_dois_sentidos_por_30_s() {
    use seele_audio::device::mentira;
    const APARELHO: &str = "app-reconectar-30s";
    let _vez = vez();
    runtime().block_on(async {
        let app = app();
        let session = app.state::<Session>();
        let caminho = std::path::PathBuf::from(super::config_dir(app)).join("preferences");
        {
            let mut preferencias =
                seele_ffi::preferences::Preferences::open(caminho.clone()).expect("as preferências");
            preferencias.set_capture(Some(&mentira::id(APARELHO))).expect("o microfone de mentira");
            preferencias.set_playback(Some(&mentira::id(APARELHO))).expect("a saída de mentira");
            preferencias
                .set_voice_mode(Some(seele_core::VoiceMode::Open))
                .expect("a voz aberta");
        }
        let (alvo, servidor) = seeled("Casa", 0, Location::Memory).await;
        let contados = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let ouve = quem_ouve(alvo, Arc::clone(&contados)).await;
        let (fala, _casa_de_quem_fala) = quem_fala(alvo).await;
        let com_audio = |app: &'static AppHandle<MockRuntime>| async move {
            tokio::time::timeout(
                PRAZO,
                connect(app.clone(), app.state::<Session>(), alvo.to_string(), "dona".to_owned(), true, None, None),
            )
            .await
            .expect("o `connect` não voltou")
            .unwrap_or_else(|falha| panic!("o `connect` não entrou: {falha:?}"))
        };
        let tocados = || mentira::contagem(APARELHO).blocos_com_som;

        // Antes: entra, senta pela janela, fala e ouve.
        let primeira = com_audio(app).await;
        let sala = primeira.snapshot.voice_rooms[0].id;
        let eu = primeira.snapshot.me.expect("quem sou");
        entrar_na_sala_em(app, &session, carimbo(&primeira), sala, None)
            .await
            .expect("sentar");
        assert!(ate(PRAZO, || mentira::contagem(APARELHO).abertos == 1).await);
        let tocou_antes = tocados();
        let enviou_antes = chegam_em(&contados, Duration::from_millis(2000)).await;
        let ouviu_antes = tocados() - tocou_antes;
        assert!(enviou_antes > 0, "antes de sair, a voz desta janela não chegou a quem ouve");
        assert!(ouviu_antes > 0, "antes de sair, esta janela não tocou a voz de quem fala");

        // Sai do servidor inteiro, pela janela; e mede se a sessão velha fechou.
        let saiu_em = tokio::time::Instant::now();
        disconnect(app.clone(), app.state::<Session>(), carimbo(&primeira))
            .await
            .expect("o `disconnect` nunca falha");
        let mut velha_sentada_ate = None;
        while tokio::time::Instant::now() < saiu_em + Duration::from_secs(3) {
            if sala_no_servidor(&servidor, eu).await.is_none() {
                velha_sentada_ate = Some(saiu_em.elapsed());
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        println!("depois do `disconnect`, o servidor soltou o assento velho em: {velha_sentada_ate:?}");
        // Quem ouve recebe também a voz de quem fala: a linha de base, com esta
        // janela fora, é o que chega só de quem fala. Daqui em diante, o envio
        // desta janela é o que passa dela.
        let so_de_quem_fala = chegam_em(&contados, Duration::from_secs(2)).await;
        let base_por_5_s = so_de_quem_fala * 5 / 2;
        println!("com esta janela fora, só de quem fala: {so_de_quem_fala} em 2 s");

        // Volta em seguida (dentro da vida da sessão velha, se ela ainda viver).
        let segunda = com_audio(app).await;
        let sala = segunda.snapshot.voice_rooms[0].id;
        entrar_na_sala_em(app, &session, carimbo(&segunda), sala, None)
            .await
            .expect("sentar de novo");

        // 30 s depois de voltar, em janelas de 5 s.
        let mut linhas = Vec::new();
        for janela in 0..6 {
            let tocou = tocados();
            let enviou = chegam_em(&contados, Duration::from_secs(5)).await;
            let ouviu = tocados() - tocou;
            let sentada = sala_no_servidor(&servidor, eu).await;
            let foco = super::foco_de_voz(app.state::<Session>());
            linhas.push((janela, enviou, ouviu, sentada, foco));
        }
        for (janela, enviou, ouviu, sentada, foco) in &linhas {
            println!(
                "depois de voltar, {}-{} s: enviados {enviou}, blocos tocados com som {ouviu}, \
                 sala no servidor {sentada:?}, foco {foco:?} (a sessão nova é {})",
                janela * 5,
                janela * 5 + 5,
                segunda.carimbo.sessao
            );
        }
        for (janela, enviou, ouviu, sentada, foco) in &linhas {
            // O envio desta janela é o que passa da base de quem fala, com folga
            // de um terço da base para a cadência.
            let desta_janela = enviou.saturating_sub(base_por_5_s);
            assert!(
                desta_janela > base_por_5_s / 3
                    && *ouviu > 0
                    && sentada.is_some()
                    && *foco == Some(segunda.carimbo.sessao),
                "depois de sair e voltar pela janela, entre {} e {} s a voz parou: enviados {enviou}, \
                 tocados {ouviu}, sala {sentada:?}, foco {foco:?}",
                janela * 5,
                janela * 5 + 5
            );
        }

        disconnect(app.clone(), app.state::<Session>(), carimbo(&segunda))
            .await
            .expect("o `disconnect` nunca falha");
        fala.disconnect();
        drop(ouve);
        servidor.shutdown();
        let mut preferencias =
            seele_ffi::preferences::Preferences::open(caminho).expect("as preferências");
        let _ = preferencias.set_capture(None);
        let _ = preferencias.set_playback(None);
        let _ = preferencias.set_voice_mode(None);
    });
}

/// **A variante MULTI-SESSÃO do relato** (o cenário da 0.17): esta janela
/// HOSPEDA e está também no servidor do amigo, sentada lá. Ela sai e volta SÓ
/// do servidor do amigo, pela janela. Com `mais_uma`, há ainda uma terceira
/// sessão aberta noutro servidor, fora de sala.
///
/// Em cada servidor há quem ouve (contando a voz no fio) e, no do amigo, quem
/// fala (o tom de mentira de outra máquina). Mede, depois do `disconnect` e do
/// `connect` do amigo: para onde vai o foco; o que a sessão nova mostra sem o
/// gesto de entrar na sala; se o `enter_voice_room` nela traz o foco; e se o
/// microfone (os datagramas) e o áudio tocado (os blocos com som da saída de
/// mentira) vão para o amigo, e não para o servidor hospedado.
async fn sair_e_voltar_so_do_amigo(mais_uma: bool) {
    use seele_audio::device::mentira;
    let aparelho = if mais_uma {
        "app-multi-3"
    } else {
        "app-multi-2"
    };
    let app = app();
    let session = app.state::<Session>();
    let caminho = std::path::PathBuf::from(super::config_dir(app)).join("preferences");
    {
        let mut preferencias =
            seele_ffi::preferences::Preferences::open(caminho.clone()).expect("as preferências");
        preferencias
            .set_capture(Some(&mentira::id(aparelho)))
            .expect("o microfone de mentira");
        preferencias
            .set_playback(Some(&mentira::id(aparelho)))
            .expect("a saída de mentira");
        preferencias
            .set_voice_mode(Some(seele_core::VoiceMode::Open))
            .expect("a voz aberta");
    }

    // O servidor hospedado por esta janela, com quem ouve lá.
    let hospedado = Hospedagem::iniciar(0, Location::Memory, "Casa", None, |_| Ok(()))
        .await
        .expect("o servidor hospedado não subiu");
    let casa = SocketAddr::from(([127, 0, 0, 1], hospedado.endereco().port()));
    session.hospedagens.lock().expect("as hospedagens").guardado =
        Some(Hospedado::novo(hospedado, None));
    let na_casa = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let ouve_na_casa = quem_ouve(casa, Arc::clone(&na_casa)).await;

    // O servidor do amigo, com quem ouve e quem fala.
    let (amigo, servidor_do_amigo) = seeled("Amigo", 0, Location::Memory).await;
    let no_amigo = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let ouve_no_amigo = quem_ouve(amigo, Arc::clone(&no_amigo)).await;
    let (fala, _casa_de_quem_fala) = quem_fala(amigo).await;

    let com_audio = |alvo: SocketAddr| async move {
        tokio::time::timeout(
            PRAZO,
            connect(
                app.clone(),
                app.state::<Session>(),
                alvo.to_string(),
                "dona".to_owned(),
                true,
                None,
                None,
            ),
        )
        .await
        .expect("o `connect` não voltou")
        .unwrap_or_else(|falha| panic!("o `connect` a {alvo} não entrou: {falha:?}"))
    };
    let tocados = || mentira::contagem(aparelho).blocos_com_som;
    let foco = || super::foco_de_voz(app.state::<Session>());

    // Esta janela: na casa, no amigo (e numa terceira, se pedida).
    let em_casa = com_audio(casa).await;
    let terceira = if mais_uma {
        let (outro, servidor_outro) = seeled("Outro", 0, Location::Memory).await;
        Some((com_audio(outro).await, servidor_outro))
    } else {
        None
    };
    let no_amigo_1 = com_audio(amigo).await;
    let sala = no_amigo_1.snapshot.voice_rooms[0].id;
    entrar_na_sala_em(app, &session, carimbo(&no_amigo_1), sala, None)
        .await
        .expect("sentar no amigo");
    assert_eq!(
        foco(),
        Some(no_amigo_1.carimbo.sessao),
        "sentar no amigo não trouxe a voz"
    );
    assert!(ate(PRAZO, || mentira::contagem(aparelho).abertos == 1).await);

    // A base de quem fala no amigo (quem ouve lá recebe a voz dele também).
    let base = |enviados: usize, base_5: usize| enviados.saturating_sub(base_5);
    let tocou = tocados();
    let casa_antes = chegam_em(&na_casa, Duration::from_secs(2)).await;
    let amigo_antes = chegam_em(&no_amigo, Duration::from_secs(2)).await;
    let ouviu_antes = tocados() - tocou;
    println!("sentada no amigo: na casa {casa_antes}, no amigo {amigo_antes} (com quem fala), tocados {ouviu_antes}");
    assert_eq!(
        casa_antes, 0,
        "com a voz no amigo, a casa recebeu voz desta janela"
    );
    assert!(
        ouviu_antes > 0,
        "sentada no amigo, esta janela não tocou a voz de quem fala lá"
    );

    // Sai SÓ do amigo.
    disconnect(app.clone(), app.state::<Session>(), carimbo(&no_amigo_1))
        .await
        .expect("o `disconnect` nunca falha");
    assert!(
        ate(PRAZO, || mentira::contagem(aparelho).abertos == 0).await,
        "saiu do amigo e o aparelho continuou aberto"
    );
    let foco_depois_de_sair = foco();
    let so_de_quem_fala = chegam_em(&no_amigo, Duration::from_secs(2)).await;
    let casa_fora = chegam_em(&na_casa, Duration::from_secs(1)).await;
    println!(
        "fora do amigo: foco {foco_depois_de_sair:?} (casa é {}), só de quem fala no amigo {so_de_quem_fala} em 2 s, \
         na casa {casa_fora}",
        em_casa.carimbo.sessao
    );
    assert_eq!(
        foco_depois_de_sair, None,
        "a voz saiu com o amigo e passou sozinha a outra sessão"
    );
    assert_eq!(
        casa_fora, 0,
        "fora do amigo, a voz desta janela foi para a casa"
    );

    // Volta ao amigo; SEM o gesto de entrar na sala.
    let no_amigo_2 = com_audio(amigo).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let sem_gesto = abertura(&session, &no_amigo_2);
    let foco_sem_gesto = foco();
    println!("de volta ao amigo, sem o gesto: abertura {sem_gesto:?}, foco {foco_sem_gesto:?}");
    assert_eq!(
        sem_gesto,
        Some(AberturaDoAudio::ForaDeSala),
        "sem o gesto de entrar na sala, a sessão nova não diz que está fora da sala"
    );
    assert_eq!(
        foco_sem_gesto, None,
        "conectar de novo tomou a voz sem o gesto"
    );

    // O gesto: entrar na sala pela janela.
    let sala = no_amigo_2.snapshot.voice_rooms[0].id;
    entrar_na_sala_em(app, &session, carimbo(&no_amigo_2), sala, None)
        .await
        .expect("sentar de novo no amigo");
    assert_eq!(
        foco(),
        Some(no_amigo_2.carimbo.sessao),
        "entrar na sala da sessão nova não trouxe a voz"
    );
    assert!(ate(PRAZO, || mentira::contagem(aparelho).abertos == 1).await);
    let base_5 = so_de_quem_fala * 5 / 2;
    for janela in 0..3 {
        let tocou = tocados();
        let casa = chegam_em(&na_casa, Duration::from_secs(5)).await;
        let ouviu = tocados() - tocou;
        println!(
            "de volta e sentada, {}-{} s: na casa {casa}, tocados {ouviu}",
            janela * 5,
            janela * 5 + 5
        );
        assert_eq!(
            casa, 0,
            "de volta ao amigo, a voz desta janela foi para a casa"
        );
        assert!(
            ouviu > 0,
            "de volta ao amigo, esta janela não toca a voz de quem fala lá"
        );
    }
    let amigo_depois = chegam_em(&no_amigo, Duration::from_secs(5)).await;
    let desta = base(amigo_depois, base_5);
    println!("de volta e sentada: no amigo {amigo_depois} em 5 s, base de quem fala {base_5}, desta janela {desta}");
    assert!(
        desta > base_5 / 3,
        "de volta ao amigo, a voz desta janela não chega a quem ouve lá"
    );
    assert_eq!(
        abertura(&session, &em_casa),
        Some(AberturaDoAudio::ForaDeSala),
        "a sessão da casa ficou com estado de áudio"
    );

    for entrada in [&em_casa, &no_amigo_2] {
        disconnect(app.clone(), app.state::<Session>(), carimbo(entrada))
            .await
            .expect("o `disconnect` nunca falha");
    }
    if let Some((entrada, servidor)) = terceira {
        disconnect(app.clone(), app.state::<Session>(), carimbo(&entrada))
            .await
            .expect("o `disconnect` nunca falha");
        servidor.shutdown();
    }
    fala.disconnect();
    drop(ouve_no_amigo);
    drop(ouve_na_casa);
    servidor_do_amigo.shutdown();
    let guardado = session
        .hospedagens
        .lock()
        .expect("as hospedagens")
        .guardado
        .take();
    if let Some(guardado) = guardado {
        guardado.hospedagem.encerrar().await;
    }
    let mut preferencias =
        seele_ffi::preferences::Preferences::open(caminho).expect("as preferências");
    let _ = preferencias.set_capture(None);
    let _ = preferencias.set_playback(None);
    let _ = preferencias.set_voice_mode(None);
}

#[test]
fn hospedando_sair_e_voltar_so_do_amigo_devolve_a_voz_ao_amigo_com_duas_sessoes() {
    let _vez = vez();
    runtime().block_on(sair_e_voltar_so_do_amigo(false));
}

#[test]
fn hospedando_sair_e_voltar_so_do_amigo_devolve_a_voz_ao_amigo_com_tres_sessoes() {
    let _vez = vez();
    runtime().block_on(sair_e_voltar_so_do_amigo(true));
}
