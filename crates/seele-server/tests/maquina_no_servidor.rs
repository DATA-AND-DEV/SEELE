//! **A máquina de jogo no servidor** (protocolo 12, API 8 parte 3): servir um
//! túnel só com aceite, e só o que estava na oferta aceita.
//!
//! Quadros crus sobre o enlace de verdade, no molde de `tunel_no_servidor.rs`.
//! O pedido é posto pelo livro direto (`seele_server::maquinas::pedir`): o
//! caminho da metade de servidor é do Task 5. **`servir_tuneis` desligado em
//! todos**: é o aceite, e não o interruptor, que está sob teste.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use ed25519_dalek::{Signer, SigningKey};
use seele_proto::control::{ClientMessage, ServerMessage};
use seele_proto::ids::PersonId;
use seele_proto::maquina::{MotivoDaParadaDeMaquina, OfertaDeMaquina, TunelOferecido};
use seele_proto::tunel::{
    MotivoDoFimDoTunel, ProtocoloDeTunel, RecusaDeTunel, ACAO_ENTRAR_NO_JOGO,
};
use seele_server::persistence::Location;
use seele_server::{frame, Daemon, ServerConfig};

mod comum;

const GESTORA: u8 = 1;
const ALICE: u8 = 2;
const BRUNO: u8 = 3;
const CARLA: u8 = 4;
const VELHO: u8 = 5;

const MOD: &str = "x/jogo";
const SERVIDOR: &str = "mc-1";
const TUNEL: &str = "mc";
const PORTA: u16 = 25565;

const PRAZO: Duration = Duration::from_secs(5);

fn publica(semente: u8) -> [u8; 32] {
    SigningKey::from_bytes(&[semente; 32])
        .verifying_key()
        .to_bytes()
}

fn oferta() -> OfertaDeMaquina {
    OfertaDeMaquina {
        titulo: "Minecraft".into(),
        receita: "minecraft-java".into(),
        versao_da_receita: "1.0.0".into(),
        versao_do_jogo: "1.21.4".into(),
        tuneis: vec![TunelOferecido {
            nome: TUNEL.into(),
            protocolo: ProtocoloDeTunel::Tcp,
            porta: PORTA,
        }],
        artefatos: vec![],
        pack: None,
        origens: vec![],
        dominios_do_jogo: vec![],
        rodar: None,
        disco_maximo: 0,
        termos: vec![],
    }
}

struct Montado {
    endereco: SocketAddr,
    daemon: Arc<Daemon>,
    hash: String,
    _temp: tempfile::TempDir,
}

/// Um servidor com o MOD `x/jogo` ligado (`poderes: ["porta"]`, `maquina`,
/// `server`, `entrar-no-jogo` dada a todos) e **`servir_tuneis` desligado**.
async fn montar() -> Result<Montado> {
    let temp = tempfile::tempdir()?;
    let raizes = seele_server::RaizesDosMods {
        pacotes: temp.path().join("cache"),
        dados: temp.path().join("dados"),
    };
    let manifesto = serde_json::json!({
        "schema": 1, "id": MOD, "version": "1.0.0",
        "api": seele_proto::mods::MOD_API_VERSION,
        "repo": "https://github.com/x/jogo", "reach": [],
        "client": "cliente.js",
        "plataformas": ["app"], "poderes": ["porta"],
        "maquina": "maquina.js", "server": "servidor.js",
        "permissoes": [{"acao": ACAO_ENTRAR_NO_JOGO, "nome": "Entrar no jogo", "todos": true}],
    })
    .to_string()
    .into_bytes();
    let mut arquivos = vec![
        ("mod.json".to_owned(), manifesto),
        ("cliente.js".to_owned(), b"void 0;".to_vec()),
        ("maquina.js".to_owned(), b"void 0;".to_vec()),
        ("servidor.js".to_owned(), b"void 0;".to_vec()),
    ];
    let hash = seele_proto::mods::hex(&seele_proto::mods::content_hash(&mut arquivos));
    let pacote = raizes.pacote_de(&hash);
    std::fs::create_dir_all(&pacote)?;
    for (nome, bytes) in arquivos {
        std::fs::write(pacote.join(nome), bytes)?;
    }
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        proprietario: Some(seele_proto::transport::key_fingerprint(&publica(GESTORA))),
        mods_dir: Some(raizes.clone()),
        servir_tuneis: false,
        ..ServerConfig::default()
    };
    let daemon = Arc::new(Daemon::bind(config).await?);
    {
        let db = daemon.server().persistence.lock().await;
        seele_server::persistence::mods::enable(
            &db,
            &seele_server::persistence::mods::EnabledMod {
                id: MOD.into(),
                version: "1.0.0".into(),
                hash: hash.clone(),
                repo: "https://github.com/x/jogo".into(),
                reach: vec![],
                server_half: false,
                api: seele_proto::mods::MOD_API_VERSION,
                tema: false,
                plataformas: Some(vec![seele_proto::mods::Plataforma::App]),
            },
        )?;
        let queixas = seele_server::mods::gravar_permissoes_declaradas(&db, &raizes);
        assert!(queixas.is_empty(), "{queixas:?}");
    }
    let endereco = daemon.local_addr()?;
    let atendendo = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = atendendo.run().await;
    });
    Ok(Montado {
        endereco,
        daemon,
        hash,
        _temp: temp,
    })
}

struct Par {
    ligado: comum::Ligado,
    pessoa: PersonId,
}

/// O aperto de mão à mão, na versão `versao`.
async fn entrar(endereco: SocketAddr, semente: u8, versao: u8) -> Result<Par> {
    let mut ligado = comum::ligar(endereco).await?;
    let chave = SigningKey::from_bytes(&[semente; 32]);
    frame::write_em(
        &mut ligado.envio,
        versao,
        &ClientMessage::Hello {
            faixa: seele_proto::version::Faixa {
                minima: seele_proto::version::VERSAO_MINIMA,
                maxima: versao,
            },
            client: "par-cru".into(),
            nickname: format!("pessoa{semente:03}"),
            public_key: publica(semente).to_vec(),
            join_secret: None,
            material_e2ee: None,
            apis_de_mod: seele_proto::mods::APIS_ACEITAS.to_vec(),
        },
    )
    .await?;
    let ServerMessage::Challenge { nonce } =
        frame::read::<ServerMessage>(&mut ligado.recebe).await?
    else {
        anyhow::bail!("sem Challenge");
    };
    frame::write_em(
        &mut ligado.envio,
        versao,
        &ClientMessage::Response {
            proof: chave
                .sign(&comum::prova_amarrada(&ligado.conexao, &nonce))
                .to_bytes()
                .to_vec(),
        },
    )
    .await?;
    loop {
        match frame::read::<ServerMessage>(&mut ligado.recebe).await? {
            ServerMessage::ModsExigidos { conjunto, .. } => {
                frame::write_em(
                    &mut ligado.envio,
                    versao,
                    &ClientMessage::AceitarMods { conjunto },
                )
                .await?;
            }
            ServerMessage::Session { person, .. } => {
                return Ok(Par {
                    ligado,
                    pessoa: person,
                });
            }
            _ => {}
        }
    }
}

/// Lê quadros até um servir, ou até o prazo.
async fn esperar<T>(
    recebe: &mut seele_enlace::Recebe,
    mut serve: impl FnMut(&ServerMessage) -> Option<T>,
) -> Option<T> {
    let fim = Instant::now() + PRAZO;
    while Instant::now() < fim {
        let Ok(Ok(quadro)) = tokio::time::timeout(
            Duration::from_millis(300),
            frame::read::<ServerMessage>(recebe),
        )
        .await
        else {
            continue;
        };
        if let Some(achado) = serve(&quadro) {
            return Some(achado);
        }
    }
    None
}

fn servir() -> ClientMessage {
    ClientMessage::ServirTunel {
        mod_id: MOD.into(),
        nome: TUNEL.into(),
        protocolo: ProtocoloDeTunel::Tcp,
        porta: PORTA,
    }
}

async fn resposta_de_tunel(par: &mut Par) -> Result<Result<u16, RecusaDeTunel>> {
    esperar(&mut par.ligado.recebe, |m| match m {
        ServerMessage::TunelConcedido { porta, .. } => Some(Ok(*porta)),
        ServerMessage::TunelRecusado { motivo, .. } => Some(Err(*motivo)),
        _ => None,
    })
    .await
    .ok_or_else(|| anyhow::anyhow!("sem resposta de túnel"))
}

/// O livro pede a `quem` a máquina de `mc-1`; devolve o resumo que o aceite tem
/// de trazer.
fn pedir(m: &Montado, quem: PersonId, pedido_por: PersonId) -> String {
    match seele_server::maquinas::pedir(
        m.daemon.server(),
        MOD,
        &m.hash,
        SERVIDOR,
        quem,
        pedido_por,
        &oferta(),
    ) {
        seele_server::maquinas::Pedido::Guardado { resumo_hex, .. } => resumo_hex,
        outro => panic!("o pedido não ficou pendente: {outro:?}"),
    }
}

fn aceite(resumo: &str) -> ClientMessage {
    ClientMessage::AceiteDeMaquina {
        mod_id: MOD.into(),
        servidor: SERVIDOR.into(),
        resumo: resumo.into(),
    }
}

/// O par lê o `PedidoDeMaquina` e manda o aceite; espera o livro registrá-lo
/// (o aceite não tem resposta no fio).
async fn aceitar(m: &Montado, par: &mut Par, resumo: &str) -> Result<()> {
    let veio = esperar(&mut par.ligado.recebe, |q| {
        matches!(q, ServerMessage::PedidoDeMaquina { .. }).then_some(())
    })
    .await;
    assert!(veio.is_some(), "quem foi chamado não recebeu o pedido");
    frame::write(&mut par.ligado.envio, &aceite(resumo)).await?;
    let ate = Instant::now() + PRAZO;
    while m.daemon.server().maquinas().quem_opera(MOD, SERVIDOR) != Some(par.pessoa) {
        assert!(Instant::now() < ate, "o aceite não entrou no livro");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn servir_sem_aceite_e_fora_do_aceite() -> Result<()> {
    assert!(
        !ServerConfig::default().servir_tuneis,
        "o padrão do produto tem de ser o aceite, e não o interruptor"
    );
    let m = montar().await?;
    let gestora = entrar(m.endereco, GESTORA, 12).await?;
    let mut alice = entrar(m.endereco, ALICE, 12).await?;

    // Sem pedido nenhum.
    frame::write(&mut alice.ligado.envio, &servir()).await?;
    assert_eq!(
        resposta_de_tunel(&mut alice).await?,
        Err(RecusaDeTunel::ForaDoAceite)
    );
    // Com o pedido mostrado, mas ainda sem o aceite.
    let _ = pedir(&m, alice.pessoa, gestora.pessoa);
    frame::write(&mut alice.ligado.envio, &servir()).await?;
    assert_eq!(
        resposta_de_tunel(&mut alice).await?,
        Err(RecusaDeTunel::ForaDoAceite)
    );
    assert!(
        !m.daemon.server().tuneis().existe(MOD, TUNEL),
        "o túnel foi para o livro apesar da recusa"
    );
    m.daemon.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn outra_pessoa_nao_toma_o_nome() -> Result<()> {
    let m = montar().await?;
    let gestora = entrar(m.endereco, GESTORA, 12).await?;
    let mut alice = entrar(m.endereco, ALICE, 12).await?;
    let mut bruno = entrar(m.endereco, BRUNO, 12).await?;
    let resumo = pedir(&m, alice.pessoa, gestora.pessoa);

    // Bruno, com 12 e no mesmo servidor, sabe o resumo (ele é calculável de
    // coisas públicas) e tenta aceitar no lugar de Alice, e depois servir o
    // nome antes dela.
    frame::write(&mut bruno.ligado.envio, &aceite(&resumo)).await?;
    frame::write(&mut bruno.ligado.envio, &servir()).await?;
    assert_eq!(
        resposta_de_tunel(&mut bruno).await?,
        Err(RecusaDeTunel::ForaDoAceite)
    );
    assert_ne!(
        m.daemon.server().maquinas().quem_opera(MOD, SERVIDOR),
        Some(bruno.pessoa),
        "o aceite de quem não foi chamado entrou no livro"
    );

    aceitar(&m, &mut alice, &resumo).await?;
    frame::write(&mut alice.ligado.envio, &servir()).await?;
    assert_eq!(resposta_de_tunel(&mut alice).await?, Ok(PORTA));
    m.daemon.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn parar_a_maquina_derruba_os_tuneis_dela() -> Result<()> {
    let m = montar().await?;
    let gestora = entrar(m.endereco, GESTORA, 12).await?;
    let mut alice = entrar(m.endereco, ALICE, 12).await?;
    let mut carla = entrar(m.endereco, CARLA, 12).await?;
    let resumo = pedir(&m, alice.pessoa, gestora.pessoa);
    aceitar(&m, &mut alice, &resumo).await?;
    frame::write(&mut alice.ligado.envio, &servir()).await?;
    assert_eq!(resposta_de_tunel(&mut alice).await?, Ok(PORTA));

    frame::write(
        &mut carla.ligado.envio,
        &ClientMessage::EntrarNoTunel {
            mod_id: MOD.into(),
            nome: TUNEL.into(),
        },
    )
    .await?;
    assert_eq!(resposta_de_tunel(&mut carla).await?, Ok(PORTA));

    frame::write(
        &mut alice.ligado.envio,
        &ClientMessage::MaquinaParada {
            mod_id: MOD.into(),
            servidor: SERVIDOR.into(),
            motivo: MotivoDaParadaDeMaquina::PeloDono,
        },
    )
    .await?;
    let motivo = esperar(&mut carla.ligado.recebe, |q| match q {
        ServerMessage::TunelEncerrado { motivo, .. } => Some(*motivo),
        _ => None,
    })
    .await;
    assert_eq!(motivo, Some(MotivoDoFimDoTunel::MaquinaParou));
    assert!(!m.daemon.server().tuneis().existe(MOD, TUNEL));
    assert_eq!(m.daemon.server().maquinas().quem_opera(MOD, SERVIDOR), None);

    // Parada, ela não serve mais: o aceite saiu junto.
    frame::write(&mut alice.ligado.envio, &servir()).await?;
    assert_eq!(
        resposta_de_tunel(&mut alice).await?,
        Err(RecusaDeTunel::ForaDoAceite)
    );
    m.daemon.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn um_par_onze_nunca_recebe_pedido_de_maquina() -> Result<()> {
    let m = montar().await?;
    let gestora = entrar(m.endereco, GESTORA, 12).await?;
    let mut alice = entrar(m.endereco, ALICE, 12).await?;
    let velho = entrar(m.endereco, VELHO, 11).await?;
    let lidos: Arc<Mutex<Vec<ServerMessage>>> = Arc::new(Mutex::new(Vec::new()));
    {
        let lidos = Arc::clone(&lidos);
        let mut recebe = velho.ligado.recebe;
        tokio::spawn(async move {
            while let Ok(q) = frame::read::<ServerMessage>(&mut recebe).await {
                lidos.lock().unwrap().push(q);
            }
        });
    }

    // O livro manda um pedido ao velho: nada chega.
    let _ = seele_server::maquinas::pedir(
        m.daemon.server(),
        MOD,
        &m.hash,
        "mc-2",
        velho.pessoa,
        gestora.pessoa,
        &oferta(),
    );
    // E a Alice, no 12, recebe o dela: o caminho funciona, e o silêncio do
    // velho é o corte por versão, e não um pedido que nunca saiu.
    let _ = pedir(&m, alice.pessoa, gestora.pessoa);
    let veio = esperar(&mut alice.ligado.recebe, |q| match q {
        ServerMessage::PedidoDeMaquina { servidor, .. } => Some(servidor.clone()),
        _ => None,
    })
    .await;
    assert_eq!(veio.as_deref(), Some(SERVIDOR));

    tokio::time::sleep(Duration::from_millis(800)).await;
    let de_maquina: Vec<_> = lidos
        .lock()
        .unwrap()
        .iter()
        .filter(|q| {
            matches!(
                q,
                ServerMessage::PedidoDeMaquina { .. }
                    | ServerMessage::AvisoParaMaquina { .. }
                    | ServerMessage::MaquinaDispensada { .. }
            )
        })
        .cloned()
        .collect();
    assert!(de_maquina.is_empty(), "o par 11 recebeu {de_maquina:?}");
    m.daemon.shutdown();
    Ok(())
}

/// Desligar o MOD dispensa a máquina dele **no servidor**, sem sessão nenhuma
/// para ajudar: o aceite é posto no livro à mão, com uma conexão que nenhuma
/// sessão tem, e só a vigia do conjunto de MODs pode tirá-lo (com sessões de
/// verdade, a queda delas por `ModsMudaram` também o tiraria, e o teste não
/// distinguiria uma coisa da outra).
#[tokio::test(flavor = "multi_thread")]
async fn desligar_o_mod_dispensa_a_maquina_dele() -> Result<()> {
    let m = montar().await?;
    let quem = PersonId(77);
    let resumo = pedir(&m, quem, PersonId(1));
    m.daemon
        .server()
        .maquinas()
        .aceitar(quem, 12345, MOD, SERVIDOR, &resumo)
        .expect("aceita");
    // Um MOD ligado, com o mesmo pacote, não dispensa nada.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        m.daemon.server().maquinas().quem_opera(MOD, SERVIDOR),
        Some(quem)
    );

    {
        let db = m.daemon.server().persistence.lock().await;
        seele_server::persistence::mods::disable(&db, MOD)?;
    }
    let ate = Instant::now() + PRAZO;
    while m
        .daemon
        .server()
        .maquinas()
        .quem_opera(MOD, SERVIDOR)
        .is_some()
    {
        assert!(
            Instant::now() < ate,
            "o MOD foi desligado e a máquina dele seguiu aceita"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    m.daemon.shutdown();
    Ok(())
}
