//! **Quem liga pela ponte disca o código de outra pessoa** (tarefa 30 da F7, a
//! 6g; spec de 23/09, «Do lado de quem liga»).
//!
//! O núcleo já discava um código desde a 14a (`Chegada::para_o_codigo`), e a
//! ponte não: ela só montava destinos a partir de um endereço, e um alvo
//! `@CÓDIGO` não resolvia. O alvo canônico de quem liga é `@CÓDIGO`, que se
//! basta sozinho: a ponte pergunta ao ponto de encontro desta máquina onde a
//! sala mora, corre os candidatos, e confere o código dentro do TLS.
//!
//! As provas sobem um ponto de encontro de verdade numa linha própria, a sala de
//! outra pessoa registrada nele, e apontam `$SEELE_ENCONTRO` para ele enquanto
//! o teste dura.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use seele_ffi::{ConnectConfig, ConnectStage, Connection, Event, EventListener, Trust};
use seele_proto::codigo::Codigo;
use seele_proto::encontro::Vizinhanca;
use seele_server::hospedagem::Hospedagem;
use seele_server::recepcao::Aviso;

mod vaga;

/// A semente de quem recebe.
const DONO: u8 = 1;

/// A semente de quem liga.
const QUEM_LIGA: u8 = 2;

fn codigo_de(semente: u8) -> Codigo {
    let chave = ed25519_dalek::SigningKey::from_bytes(&[semente; 32]);
    Codigo::da_chave(&chave.verifying_key().to_bytes())
}

/// Um ponto de encontro de verdade, atendendo numa linha própria.
fn subir_o_ponto() -> SocketAddr {
    let ponto = seele_encontro::Ponto::abrir_com_quarto(
        SocketAddr::from(([127, 0, 0, 1], 0)),
        Vizinhanca::TambemAqui,
        Arc::new(seele_encontro::Quarto::novo()),
    )
    .expect("a escuta tem que abrir");
    let onde = ponto.endereco().expect("o socket sabe onde ligou");
    std::thread::spawn(move || {
        let _ = ponto.servir();
    });
    onde
}

/// `$SEELE_ENCONTRO` apontado para o ponto do teste, e devolvido ao que era
/// quando o teste acaba, inclusive em pânico. Quem serializa é a vaga.
struct AmbienteDoEncontro {
    antes: Option<std::ffi::OsString>,
}

impl AmbienteDoEncontro {
    fn apontar(ponto: SocketAddr) -> Self {
        let antes = std::env::var_os(seele_server::alcance::encontro::VARIAVEL);
        std::env::set_var(seele_server::alcance::encontro::VARIAVEL, ponto.to_string());
        Self { antes }
    }
}

impl Drop for AmbienteDoEncontro {
    fn drop(&mut self) {
        match self.antes.take() {
            Some(valor) => std::env::set_var(seele_server::alcance::encontro::VARIAVEL, valor),
            None => std::env::remove_var(seele_server::alcance::encontro::VARIAVEL),
        }
    }
}

/// A casa de quem liga, com a identidade dele.
fn casa_de_quem_liga() -> tempfile::TempDir {
    let casa = tempfile::tempdir().unwrap();
    std::fs::write(casa.path().join("identity.key"), [QUEM_LIGA; 32]).unwrap();
    casa
}

fn config_para(server: String, casa: &tempfile::TempDir) -> ConnectConfig {
    ConnectConfig {
        server,
        alternate_servers: Vec::new(),
        nickname: "quem-liga".into(),
        codigo_esperado: None,
        home: casa.path().display().to_string(),
        join_secret: None,
        expected_fingerprint: None,
        bilhete: None,
        // Não há placa de som numa máquina de integração contínua.
        audio: false,
        capture_device: None,
        playback_device: None,
    }
}

/// As etapas que a ponte entrega, em ordem.
#[derive(Default)]
struct Etapas {
    vistas: Mutex<Vec<ConnectStage>>,
}

impl EventListener for Etapas {
    fn on_event(&self, event: Event) {
        if let Event::ConnectStageChanged { stage } = event {
            if let Ok(mut vistas) = self.vistas.lock() {
                vistas.push(stage);
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quem_liga_pela_ponte_disca_o_codigo_e_entra_quando_atendem() {
    let _vaga = vaga::minha();
    let ponto = subir_o_ponto();
    let _ambiente = AmbienteDoEncontro::apontar(ponto);
    let sala = Hospedagem::iniciar_sala(0, &[DONO; 32], "dona", Some(ponto.to_string()))
        .await
        .expect("a sala não subiu");
    let mut registro = sala
        .registro_no_quarto()
        .expect("a sala subiu sem registro no ponto");
    let registrou = tokio::time::timeout(
        Duration::from_secs(5),
        registro.wait_for(|estado| {
            *estado == seele_server::alcance::encontro::EstadoDoRegistro::Registrado
        }),
    )
    .await
    .is_ok_and(|visto| visto.is_ok());
    assert!(registrou, "a sala não se registrou no ponto");
    let recepcao = sala.recepcao().expect("a sala tem recepção");
    let mut avisos = recepcao.assinar();

    let casa = casa_de_quem_liga();
    let config = config_para(seele_ffi::alvo_do_codigo(&codigo_de(DONO)), &casa);
    let etapas = Arc::new(Etapas::default());
    let olhos = Arc::clone(&etapas) as Arc<dyn EventListener>;
    let ligando = tokio::task::spawn_blocking(move || Connection::connect_watching(config, olhos));

    // Quem recebe atende quando toca.
    let toque = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(Aviso::Tocou { toque, codigo, .. }) = avisos.recv().await {
                if codigo == codigo_de(QUEM_LIGA) {
                    return toque;
                }
            }
        }
    })
    .await;
    let Ok(toque) = toque else {
        let resultado = tokio::time::timeout(Duration::from_secs(20), ligando).await;
        panic!(
            "a ligação pela ponte não tocou na sala: {:?}; etapas {:?}",
            resultado.map(|feito| feito.map(|entrou| entrou.map(|_| ()))),
            etapas.vistas.lock().map(|vistas| vistas.clone())
        );
    };
    // Atende depois de quem liga ter visto que está chamando, como quem lê o
    // CHAMANDO antes de alguém atender. A ponte entrega as etapas por um
    // `watch`, que guarda a última: um atender no mesmo instante do toque
    // juntaria o `Tocando` e o `Dentro` numa entrega só.
    let viu_tocando = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let viu = etapas.vistas.lock().is_ok_and(|vistas| {
                vistas
                    .iter()
                    .any(|etapa| matches!(etapa, ConnectStage::Tocando { .. }))
            });
            if viu {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(
        viu_tocando.is_ok(),
        "a sala tocou, e quem liga pela ponte não viu `Tocando`: {:?}",
        etapas.vistas.lock().map(|vistas| vistas.clone())
    );
    assert!(recepcao.atender(toque), "o toque não estava mais tocando");

    let (conexao, confianca) = tokio::time::timeout(Duration::from_secs(10), ligando)
        .await
        .expect("quem foi atendido não entrou em 10 s")
        .unwrap()
        .unwrap_or_else(|falha| panic!("a ligação pela ponte não entrou: {falha:?}"));
    assert!(
        matches!(confianca, Trust::FirstContactVerified { .. }),
        "o código não foi conferido na primeira ligação: {confianca:?}"
    );
    let nomes: Vec<&str> = etapas
        .vistas
        .lock()
        .unwrap()
        .iter()
        .map(ConnectStage::nome)
        .collect();
    assert_eq!(
        nomes.first(),
        Some(&"Localizando"),
        "uma ligação começa perguntando ao ponto: {nomes:?}"
    );
    assert!(
        nomes.ends_with(&["Tocando", "Dentro"]),
        "a ligação não passou pelo toque até entrar: {nomes:?}"
    );
    // A casca decide pela origem o que anotar: a ligação vai só para a agenda,
    // e não para a lista de visitados (decisão de 16/10).
    assert_eq!(
        conexao.codigo_da_ligacao(),
        Some(codigo_de(DONO)),
        "a conexão discada pelo código não diz que foi uma ligação, nem para quem"
    );

    let despediu =
        tokio::task::spawn_blocking(move || conexao.encerrar_e_esperar(Duration::from_secs(5)))
            .await
            .expect("a linha da despedida caiu");
    assert!(despediu, "a ligação não se despediu em 5 s");
    sala.encerrar().await;
}

/// **Entrar na mesma sala pelo endereço não é ligar** (decisão de 16/10: um
/// `@CÓDIGO` não entra na lista de visitados, e o resto continua entrando).
///
/// O critério é a origem da tentativa, e quem a sabe é a ponte: a conexão
/// discada pelo código diz por qual código chegou, e a que veio de um endereço
/// não diz nada — mesmo com uma sala do outro lado, que tocou e foi atendida.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_mesma_sala_pelo_endereco_nao_e_uma_ligacao() {
    let _vaga = vaga::minha();
    let sala = Hospedagem::iniciar_sala(0, &[DONO; 32], "dona", None)
        .await
        .expect("a sala não subiu");
    let recepcao = sala.recepcao().expect("a sala tem recepção");
    let mut avisos = recepcao.assinar();

    let casa = casa_de_quem_liga();
    let endereco = SocketAddr::from(([127, 0, 0, 1], sala.endereco().port()));
    let config = config_para(endereco.to_string(), &casa);
    let olhos = Arc::new(Etapas::default()) as Arc<dyn EventListener>;
    let entrando = tokio::task::spawn_blocking(move || Connection::connect_watching(config, olhos));

    let toque = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(Aviso::Tocou { toque, .. }) = avisos.recv().await {
                return toque;
            }
        }
    })
    .await
    .expect("a sala não tocou para quem entrou pelo endereço");
    assert!(recepcao.atender(toque), "o toque não estava mais tocando");

    let (conexao, _) = tokio::time::timeout(Duration::from_secs(10), entrando)
        .await
        .expect("quem foi atendido não entrou em 10 s")
        .unwrap()
        .unwrap_or_else(|falha| panic!("a entrada pelo endereço não entrou: {falha:?}"));
    assert_eq!(
        conexao.codigo_da_ligacao(),
        None,
        "uma entrada pelo endereço da sala saiu como ligação"
    );

    let despediu =
        tokio::task::spawn_blocking(move || conexao.encerrar_e_esperar(Duration::from_secs(5)))
            .await
            .expect("a linha da despedida caiu");
    assert!(despediu, "a entrada não se despediu em 5 s");
    sala.encerrar().await;
}
