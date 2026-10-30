//! **A máquina de jogo no servidor** (protocolo 12, API 8 parte 3): servir um
//! túnel só com aceite, e só o que estava na oferta aceita.
//!
//! Quadros crus sobre o enlace de verdade, no molde de `tunel_no_servidor.rs`.
//! Os testes do aceite põem o pedido pelo livro direto
//! (`seele_server::maquinas::pedir`); os da metade de servidor (Task 5) fazem
//! o caminho inteiro: um `ModRequest` ao MOD de teste, cujo `aoPedir` chama
//! `mundo.maquinas.pedir` e cujo `aoAcontecer` grava no quintal os momentos da
//! máquina que vê. **`servir_tuneis` desligado em todos**: é o aceite, e não o
//! interruptor, que está sob teste.

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
use seele_proto::ids::{ChannelId, MessageId, PersonId};
use seele_proto::maquina::{
    MotivoDaDispensa, MotivoDaParadaDeMaquina, OfertaDeMaquina, TunelOferecido,
};
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

/// O segundo MOD, sem metade de máquina: o que ele vê dos momentos é o que
/// prova que os da máquina vão **só** ao MOD dela.
const ECO: &str = "x/eco";

/// A metade de servidor do MOD de teste.
///
/// `aoPedir`: um pedido da máquina (`contexto.maquina`) é gravado em
/// `dados.visto_da_maquina` e respondido; os outros fazem o que o pedido
/// manda — `pedir` (a `quem`, ou a quem pediu) e `avisar`. `aoAcontecer`:
/// cada momento da máquina, uma linha em `dados.momentos`. A oferta é a mesma
/// de [`oferta`], no JSON que um MOD escreve.
const METADE: &str = r#"
const OFERTA = { titulo: "Minecraft", receita: "minecraft-java", versao_da_receita: "1.0.0",
  versao_do_jogo: "1.21.4", tuneis: [{ nome: "mc", protocolo: "tcp", porta: 25565 }] };
globalThis.aoPedir = (contexto, pedido) => {
  const c = JSON.parse(contexto);
  const p = JSON.parse(pedido);
  if (c.maquina) {
    dados.visto_da_maquina = JSON.stringify({ servidor: c.maquina.servidor, pessoa: c.person, pedido: p });
    return JSON.stringify({ ok: true, servidor: c.maquina.servidor });
  }
  if (p.pedir) mundo.maquinas.pedir({ quem: p.quem === undefined ? c.person : p.quem, servidor: "mc-1", oferta: OFERTA });
  if (p.avisar !== undefined) mundo.maquinas.avisar({ servidor: "mc-1", carga: p.avisar });
  return JSON.stringify({ ok: true });
};
globalThis.aoAcontecer = (momento, carga) => {
  if (momento.indexOf("Maquina") === 0) dados.momentos = (dados.momentos || "") + momento + " " + carga + "\n";
};
"#;

/// A metade de servidor do [`ECO`]: grava **todo** momento que vê.
const METADE_DO_ECO: &str = r#"
globalThis.aoAcontecer = (momento, carga) => {
  dados.vistos = (dados.vistos || "") + momento + "\n";
};
"#;

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

/// Escreve um pacote na pasta do hash dele e o liga no banco; devolve o hash.
fn instalar(
    db: &seele_server::persistence::Persistence,
    raizes: &seele_server::RaizesDosMods,
    id: &str,
    manifesto: &serde_json::Value,
    metade: &str,
) -> Result<String> {
    let mut arquivos = vec![
        ("mod.json".to_owned(), manifesto.to_string().into_bytes()),
        ("cliente.js".to_owned(), b"void 0;".to_vec()),
        ("servidor.js".to_owned(), metade.as_bytes().to_vec()),
    ];
    if manifesto.get("maquina").is_some() {
        arquivos.push(("maquina.js".to_owned(), b"void 0;".to_vec()));
    }
    let hash = seele_proto::mods::hex(&seele_proto::mods::content_hash(&mut arquivos));
    let pacote = raizes.pacote_de(&hash);
    std::fs::create_dir_all(&pacote)?;
    for (nome, bytes) in arquivos {
        std::fs::write(pacote.join(nome), bytes)?;
    }
    seele_server::persistence::mods::enable(
        db,
        &seele_server::persistence::mods::EnabledMod {
            id: id.into(),
            version: "1.0.0".into(),
            hash: hash.clone(),
            repo: format!("https://github.com/{id}"),
            reach: vec![],
            server_half: true,
            api: seele_proto::mods::MOD_API_VERSION,
            tema: false,
            plataformas: Some(vec![seele_proto::mods::Plataforma::App]),
        },
    )?;
    Ok(hash)
}

/// Um servidor com o MOD `x/jogo` ligado (`poderes: ["porta"]`, `maquina`,
/// `server` = [`METADE`], `entrar-no-jogo` dada a todos) e **`servir_tuneis`
/// desligado**.
async fn montar() -> Result<Montado> {
    montar_com(false).await
}

/// [`montar`], e com `eco` também o [`ECO`] ligado.
///
/// O banco é um arquivo, e os MODs são ligados **antes** de o servidor subir:
/// o despachante dos momentos nasce no `Daemon::bind` só se há MOD ligado.
async fn montar_com(eco: bool) -> Result<Montado> {
    let temp = tempfile::tempdir()?;
    let raizes = seele_server::RaizesDosMods {
        pacotes: temp.path().join("cache"),
        dados: temp.path().join("dados"),
    };
    let banco = Location::File(temp.path().join("seele.db"));
    let hash = {
        let db = seele_server::persistence::Persistence::open(&banco)?;
        let manifesto = serde_json::json!({
            "schema": 1, "id": MOD, "version": "1.0.0",
            "api": seele_proto::mods::MOD_API_VERSION,
            "repo": "https://github.com/x/jogo", "reach": [],
            "client": "cliente.js",
            "plataformas": ["app"], "poderes": ["porta"],
            "maquina": "maquina.js", "server": "servidor.js",
            "permissoes": [{"acao": ACAO_ENTRAR_NO_JOGO, "nome": "Entrar no jogo", "todos": true}],
        });
        let hash = instalar(&db, &raizes, MOD, &manifesto, METADE)?;
        if eco {
            let manifesto = serde_json::json!({
                "schema": 1, "id": ECO, "version": "1.0.0",
                "api": seele_proto::mods::MOD_API_VERSION,
                "repo": "https://github.com/x/eco", "reach": [],
                "client": "cliente.js", "server": "servidor.js",
            });
            instalar(&db, &raizes, ECO, &manifesto, METADE_DO_ECO)?;
        }
        let queixas = seele_server::mods::gravar_permissoes_declaradas(&db, &raizes);
        assert!(queixas.is_empty(), "{queixas:?}");
        hash
    };
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: banco,
        proprietario: Some(seele_proto::transport::key_fingerprint(&publica(GESTORA))),
        mods_dir: Some(raizes.clone()),
        servir_tuneis: false,
        ..ServerConfig::default()
    };
    let daemon = Arc::new(Daemon::bind(config).await?);
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

// ---- a metade de servidor (Task 5) ----

/// Um `ModRequest` ao MOD de teste, e a resposta inteira.
async fn pedir_ao_mod(par: &mut Par, request: u32, payload: &serde_json::Value) -> Result<String> {
    frame::write(
        &mut par.ligado.envio,
        &ClientMessage::ModRequest {
            request,
            id: MOD.into(),
            channel: ChannelId(0),
            payload: payload.to_string(),
        },
    )
    .await?;
    resposta_do_mod(par, request).await
}

/// Junta as partes do `ModReply` de `request`.
async fn resposta_do_mod(par: &mut Par, request: u32) -> Result<String> {
    let mut partes = Vec::new();
    let fim = Instant::now() + PRAZO;
    while Instant::now() < fim {
        let veio = esperar(&mut par.ligado.recebe, |q| match q {
            ServerMessage::ModReply {
                request: r,
                part,
                total,
                payload,
            } if *r == request => Some((*part, *total, payload.clone())),
            _ => None,
        })
        .await;
        let Some((_, total, payload)) = veio else {
            break;
        };
        partes.push(payload);
        if partes.len() == total as usize {
            return Ok(partes.concat());
        }
    }
    anyhow::bail!("sem ModReply para o pedido {request}")
}

/// O quintal de `id`, lido do banco.
async fn quintal(m: &Montado, id: &str) -> std::collections::BTreeMap<String, String> {
    let db = m.daemon.server().persistence.lock().await;
    seele_server::persistence::mods::ler_quintal(&db, id).unwrap_or_default()
}

/// Espera a chave `chave` do quintal do MOD de teste conter `trecho`.
async fn esperar_no_quintal(m: &Montado, chave: &str, trecho: &str) -> String {
    let fim = Instant::now() + PRAZO;
    loop {
        let valor = quintal(m, MOD)
            .await
            .get(chave)
            .cloned()
            .unwrap_or_default();
        if valor.contains(trecho) {
            return valor;
        }
        assert!(
            Instant::now() < fim,
            "o quintal do MOD não chegou a ter `{trecho}` em `{chave}`: {valor:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// O par lê o `PedidoDeMaquina` que a metade de servidor mandou, confere-o, e
/// aceita com o resumo calculado do que **chegou** — o que o app faz.
async fn aceitar_o_que_chegou(m: &Montado, par: &mut Par, pedido_por: PersonId) -> Result<()> {
    let veio = esperar(&mut par.ligado.recebe, |q| match q {
        ServerMessage::PedidoDeMaquina {
            mod_id,
            hash,
            servidor,
            pedido_por,
            oferta,
        } => Some((
            mod_id.clone(),
            hash.clone(),
            servidor.clone(),
            *pedido_por,
            oferta.clone(),
        )),
        _ => None,
    })
    .await;
    let (mod_id, hash, servidor, por, oferta_que_veio) =
        veio.ok_or_else(|| anyhow::anyhow!("o PedidoDeMaquina não chegou"))?;
    assert_eq!(mod_id, MOD);
    assert_eq!(hash, m.hash, "o hash não é o do pacote ligado");
    assert_eq!(servidor, SERVIDOR);
    assert_eq!(por, pedido_por, "pedido_por não é quem pediu ao MOD");
    assert_eq!(
        oferta_que_veio,
        oferta(),
        "a oferta do JS não chegou como o MOD a escreveu"
    );
    let resumo = seele_proto::maquina::resumo_em_hex(&seele_proto::maquina::resumo_da_oferta(
        &mod_id,
        &hash,
        &servidor,
        &oferta_que_veio,
    ));
    frame::write(&mut par.ligado.envio, &aceite(&resumo)).await?;
    Ok(())
}

/// A gestora pede ao MOD que chame `quem`; `quem` aceita; o MOD ouve.
async fn chamar_e_aceitar(m: &Montado, gestora: &mut Par, quem: &mut Par) -> Result<()> {
    let r = pedir_ao_mod(
        gestora,
        1,
        &serde_json::json!({ "pedir": true, "quem": quem.pessoa.0 }),
    )
    .await?;
    assert_eq!(r, r#"{"ok":true}"#);
    aceitar_o_que_chegou(m, quem, gestora.pessoa).await?;
    esperar_no_quintal(
        m,
        "momentos",
        &format!(
            r#"MaquinaAceitou {{"servidor":"mc-1","pessoa":{}}}"#,
            quem.pessoa.0
        ),
    )
    .await;
    Ok(())
}

fn pedido_da_maquina(request: u32) -> ClientMessage {
    ClientMessage::PedidoDaMaquina {
        request,
        mod_id: MOD.into(),
        servidor: SERVIDOR.into(),
        payload: r#"{"estado":"pronto"}"#.into(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_metade_de_servidor_chama_e_ouve_o_aceite() -> Result<()> {
    let m = montar().await?;
    let mut gestora = entrar(m.endereco, GESTORA, 12).await?;
    let mut alice = entrar(m.endereco, ALICE, 12).await?;
    chamar_e_aceitar(&m, &mut gestora, &mut alice).await?;
    assert_eq!(
        m.daemon.server().maquinas().quem_opera(MOD, SERVIDOR),
        Some(alice.pessoa)
    );

    // A máquina fala com a metade de servidor, e a resposta volta como a de
    // qualquer pedido.
    frame::write(&mut alice.ligado.envio, &pedido_da_maquina(7)).await?;
    let resposta = resposta_do_mod(&mut alice, 7).await?;
    assert_eq!(resposta, r#"{"ok":true,"servidor":"mc-1"}"#);
    let visto: serde_json::Value =
        serde_json::from_str(&esperar_no_quintal(&m, "visto_da_maquina", "mc-1").await)?;
    assert_eq!(visto["servidor"], SERVIDOR);
    assert_eq!(
        visto["pessoa"],
        alice.pessoa.0.to_string(),
        "contexto.person não é a máquina"
    );
    assert_eq!(visto["pedido"]["estado"], "pronto");

    // E a parada também chega ao MOD, com o motivo pelo nome.
    frame::write(
        &mut alice.ligado.envio,
        &ClientMessage::MaquinaParada {
            mod_id: MOD.into(),
            servidor: SERVIDOR.into(),
            motivo: MotivoDaParadaDeMaquina::PeloDono,
        },
    )
    .await?;
    esperar_no_quintal(
        &m,
        "momentos",
        &format!(
            r#"MaquinaParou {{"servidor":"mc-1","pessoa":{},"motivo":"pelo-dono"}}"#,
            alice.pessoa.0
        ),
    )
    .await;
    m.daemon.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_recusa_de_quem_foi_chamado_chega_ao_mod() -> Result<()> {
    let m = montar().await?;
    let mut gestora = entrar(m.endereco, GESTORA, 12).await?;
    let mut alice = entrar(m.endereco, ALICE, 12).await?;
    pedir_ao_mod(
        &mut gestora,
        1,
        &serde_json::json!({ "pedir": true, "quem": alice.pessoa.0 }),
    )
    .await?;
    let veio = esperar(&mut alice.ligado.recebe, |q| {
        matches!(q, ServerMessage::PedidoDeMaquina { .. }).then_some(())
    })
    .await;
    assert!(veio.is_some(), "o pedido não chegou");
    frame::write(
        &mut alice.ligado.envio,
        &ClientMessage::RecusaDeMaquina {
            mod_id: MOD.into(),
            servidor: SERVIDOR.into(),
            motivo: seele_proto::maquina::MotivoDaRecusaDeMaquina::PeloDono,
        },
    )
    .await?;
    esperar_no_quintal(
        &m,
        "momentos",
        &format!(
            r#"MaquinaRecusou {{"servidor":"mc-1","pessoa":{},"motivo":"pelo-dono"}}"#,
            alice.pessoa.0
        ),
    )
    .await;
    m.daemon.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn quem_nao_e_a_maquina_nao_fala_como_maquina() -> Result<()> {
    let m = montar().await?;
    let mut gestora = entrar(m.endereco, GESTORA, 12).await?;
    let mut alice = entrar(m.endereco, ALICE, 12).await?;
    let mut bruno = entrar(m.endereco, BRUNO, 12).await?;
    chamar_e_aceitar(&m, &mut gestora, &mut alice).await?;

    // Bruno, no mesmo servidor e no 12, fala como se fosse a máquina de mc-1.
    frame::write(&mut bruno.ligado.envio, &pedido_da_maquina(9)).await?;
    let veio = esperar(&mut bruno.ligado.recebe, |q| match q {
        ServerMessage::MaquinaDispensada {
            mod_id,
            servidor,
            motivo,
        } => Some((mod_id.clone(), servidor.clone(), *motivo)),
        ServerMessage::ModReply { request: 9, .. } => {
            panic!("quem não é a máquina recebeu resposta da metade de servidor: {q:?}")
        }
        _ => None,
    })
    .await;
    assert_eq!(
        veio,
        Some((
            MOD.to_owned(),
            SERVIDOR.to_owned(),
            MotivoDaDispensa::SemAceite
        ))
    );
    // E o MOD não foi chamado: nada no quintal da máquina.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        quintal(&m, MOD).await.get("visto_da_maquina"),
        None,
        "o MOD foi chamado pelo pedido de quem não é a máquina"
    );
    m.daemon.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn o_aviso_vai_so_a_maquina_aceita() -> Result<()> {
    let m = montar().await?;
    let mut gestora = entrar(m.endereco, GESTORA, 12).await?;
    let mut alice = entrar(m.endereco, ALICE, 12).await?;
    let bruno = entrar(m.endereco, BRUNO, 12).await?;
    let lidos_por_bruno: Arc<Mutex<Vec<ServerMessage>>> = Arc::new(Mutex::new(Vec::new()));
    {
        let lidos = Arc::clone(&lidos_por_bruno);
        let mut recebe = bruno.ligado.recebe;
        tokio::spawn(async move {
            while let Ok(q) = frame::read::<ServerMessage>(&mut recebe).await {
                lidos.lock().unwrap().push(q);
            }
        });
    }

    // Sem máquina aceita, o aviso não sai, e é contado.
    assert_eq!(m.daemon.server().avisos_sem_maquina(MOD), 0);
    pedir_ao_mod(&mut gestora, 1, &serde_json::json!({ "avisar": "cedo" })).await?;
    let ate = Instant::now() + PRAZO;
    while m.daemon.server().avisos_sem_maquina(MOD) == 0 {
        assert!(Instant::now() < ate, "o aviso sem máquina não foi contado");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    chamar_e_aceitar(&m, &mut gestora, &mut alice).await?;
    pedir_ao_mod(
        &mut gestora,
        2,
        &serde_json::json!({ "avisar": { "fase": 2 } }),
    )
    .await?;
    let carga = esperar(&mut alice.ligado.recebe, |q| match q {
        ServerMessage::AvisoParaMaquina {
            mod_id,
            servidor,
            carga,
        } if mod_id == MOD && servidor == SERVIDOR => Some(carga.clone()),
        _ => None,
    })
    .await;
    assert_eq!(carga.as_deref(), Some(r#"{"fase":2}"#));
    assert_eq!(
        m.daemon.server().avisos_sem_maquina(MOD),
        1,
        "o aviso com máquina foi contado como sem máquina"
    );

    tokio::time::sleep(Duration::from_millis(500)).await;
    let avisos_a_bruno: Vec<_> = lidos_por_bruno
        .lock()
        .unwrap()
        .iter()
        .filter(|q| matches!(q, ServerMessage::AvisoParaMaquina { .. }))
        .cloned()
        .collect();
    assert!(
        avisos_a_bruno.is_empty(),
        "Bruno recebeu {avisos_a_bruno:?}"
    );
    m.daemon.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn pedir_a_quem_nao_esta_aqui_volta_como_recusa() -> Result<()> {
    let m = montar().await?;
    let mut gestora = entrar(m.endereco, GESTORA, 12).await?;
    let velho = entrar(m.endereco, VELHO, 11).await?;

    // Uma pessoa que não existe, e uma que está aqui mas no 11: nenhuma das
    // duas tem como ver o pedido.
    for (n, quem) in [(1, 999_u64), (2, velho.pessoa.0)] {
        pedir_ao_mod(
            &mut gestora,
            n,
            &serde_json::json!({ "pedir": true, "quem": quem }),
        )
        .await?;
        esperar_no_quintal(
            &m,
            "momentos",
            &format!(
                r#"MaquinaRecusou {{"servidor":"mc-1","pessoa":{quem},"motivo":"nao-esta-aqui"}}"#
            ),
        )
        .await;
    }
    assert_eq!(
        m.daemon.server().maquinas().hashes(),
        Vec::<(String, String)>::new(),
        "o pedido a quem não está aqui ficou pendente no livro"
    );
    m.daemon.shutdown();
    Ok(())
}

/// **Um pendente substituído diz a quem tinha sido chamado** (a decisão da
/// revisão do Task 3): a tela de aceite de Alice fecha quando o pedido passa a
/// Bruno, em vez de mostrar uma oferta que, aceita, não vale mais.
#[tokio::test(flavor = "multi_thread")]
async fn um_pendente_substituido_diz_a_quem_foi_chamado() -> Result<()> {
    let m = montar().await?;
    let gestora = entrar(m.endereco, GESTORA, 12).await?;
    let mut alice = entrar(m.endereco, ALICE, 12).await?;
    let mut bruno = entrar(m.endereco, BRUNO, 12).await?;
    let _ = pedir(&m, alice.pessoa, gestora.pessoa);
    let veio = esperar(&mut alice.ligado.recebe, |q| {
        matches!(q, ServerMessage::PedidoDeMaquina { .. }).then_some(())
    })
    .await;
    assert!(veio.is_some(), "o pedido a Alice não chegou");

    let _ = pedir(&m, bruno.pessoa, gestora.pessoa);
    let dispensa = esperar(&mut alice.ligado.recebe, |q| match q {
        ServerMessage::MaquinaDispensada {
            servidor, motivo, ..
        } => Some((servidor.clone(), *motivo)),
        _ => None,
    })
    .await;
    assert_eq!(
        dispensa,
        Some((SERVIDOR.to_owned(), MotivoDaDispensa::OutroPedido))
    );
    let veio = esperar(&mut bruno.ligado.recebe, |q| {
        matches!(q, ServerMessage::PedidoDeMaquina { .. }).then_some(())
    })
    .await;
    assert!(veio.is_some(), "o pedido a Bruno não chegou");
    m.daemon.shutdown();
    Ok(())
}

/// **Os momentos da máquina vão só ao MOD dela.** O [`ECO`] grava todo momento
/// que vê; ele vê o `MessageRemoved` posto no barramento (o controle: o
/// despachante chega a ele), e não vê o `MaquinaAceitou` de `x/jogo`.
#[tokio::test(flavor = "multi_thread")]
async fn os_momentos_da_maquina_vao_so_ao_mod_dela() -> Result<()> {
    let m = montar_com(true).await?;
    let mut gestora = entrar(m.endereco, GESTORA, 12).await?;
    let mut alice = entrar(m.endereco, ALICE, 12).await?;
    chamar_e_aceitar(&m, &mut gestora, &mut alice).await?;

    let _ = m
        .daemon
        .server()
        .events
        .send(seele_server::server::Event::MessageRemoved {
            channel: ChannelId(1),
            id: MessageId(1),
        });
    let fim = Instant::now() + PRAZO;
    let vistos = loop {
        let vistos = quintal(&m, ECO)
            .await
            .get("vistos")
            .cloned()
            .unwrap_or_default();
        if vistos.contains("MessageRemoved") {
            break vistos;
        }
        assert!(
            Instant::now() < fim,
            "o eco não viu nem o MessageRemoved: {vistos:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert!(
        !vistos.contains("Maquina"),
        "um momento da máquina de x/jogo chegou a x/eco: {vistos:?}"
    );
    m.daemon.shutdown();
    Ok(())
}
