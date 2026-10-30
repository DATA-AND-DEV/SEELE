//! **O túnel de jogo no servidor** (protocolo 12): quem serve, quem entra, quem
//! perde a permissão e o que o anfitrião repassa pela estrela.
//!
//! Quadros crus sobre o enlace de verdade, no molde de `revogacao_de_cargos.rs`:
//! o que está sob teste é o servidor, e não um cliente.
//!
//! # O enquadramento que o Task 7 tem de falar
//!
//! A primeira mensagem de todo canal repassado é uma `AberturaDeTunel`:
//!
//! - no `t`, **um quadro de controle** (`frame::write`/`frame::read`: prefixo de
//!   comprimento mais o postcard carimbado), o mesmo dos canais `b`;
//! - no `j`, **o primeiro datagrama**, o mesmo quadro de `control::encode` (o
//!   carimbo e o postcard, sem prefixo de comprimento: o datagrama já é um
//!   limite).

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
use seele_enlace::{CanalDeTunel, Confiabilidade};
use seele_proto::cargos::{ChaveDeMod, Permissao, QuadroDeCargos, TODOS};
use seele_proto::control::{ClientMessage, ServerMessage};
use seele_proto::ids::PersonId;
use seele_proto::tunel::{
    AberturaDeTunel, MotivoDoFimDoTunel, ProtocoloDeTunel, RecusaDeTunel, TipoDeCanalDeTunel,
    ACAO_ENTRAR_NO_JOGO,
};
use seele_server::persistence::Location;
use seele_server::{frame, Daemon, ServerConfig};

mod comum;

const GESTORA: u8 = 1;
const MAQUINA: u8 = 2;
const JOGADOR: u8 = 3;
const VELHO: u8 = 5;

const MOD: &str = "x/jogo";
const NOME: &str = "eco-tcp";

const PRAZO: Duration = Duration::from_secs(5);

fn publica(semente: u8) -> [u8; 32] {
    SigningKey::from_bytes(&[semente; 32])
        .verifying_key()
        .to_bytes()
}

/// Um servidor com o MOD `x/jogo` ligado: poder `porta`, a ação
/// `entrar-no-jogo` declarada, e dada a todos só se `todos`.
async fn montar(todos: bool) -> Result<(SocketAddr, Arc<Daemon>, tempfile::TempDir)> {
    montar_com_malha(todos, true).await
}

/// Como [`montar`], com o interruptor da malha dos túneis na mão.
async fn montar_com_malha(
    todos: bool,
    malha_para_tuneis: bool,
) -> Result<(SocketAddr, Arc<Daemon>, tempfile::TempDir)> {
    montar_completo(todos, malha_para_tuneis, true).await
}

/// Como [`montar_com_malha`], com também o interruptor de servir túneis na mão
/// (`ServerConfig::servir_tuneis`, desligado no padrão do produto).
async fn montar_completo(
    todos: bool,
    malha_para_tuneis: bool,
    servir_tuneis: bool,
) -> Result<(SocketAddr, Arc<Daemon>, tempfile::TempDir)> {
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
        // A porta pede a metade de máquina, e ela a de servidor (API 8, parte 3).
        "maquina": "maquina.js", "server": "servidor.js",
        "permissoes": [{"acao": ACAO_ENTRAR_NO_JOGO, "nome": "Entrar no jogo", "todos": todos}],
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
        malha_para_tuneis,
        servir_tuneis,
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
                hash,
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
    Ok((endereco, daemon, temp))
}

struct Par {
    ligado: comum::Ligado,
    pessoa: PersonId,
}

/// O aperto de mão à mão, na versão `versao` (12 é a desta build).
async fn entrar(endereco: SocketAddr, semente: u8, versao: u8) -> Result<Par> {
    let mut ligado = comum::ligar(endereco).await?;
    let chave = SigningKey::from_bytes(&[semente; 32]);
    frame::write_em(
        &mut ligado.envio,
        versao,
        &ClientMessage::Hello {
            // A faixa do par é o que a negociação lê: um par 11 anuncia até 11.
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
        nome: NOME.into(),
        protocolo: ProtocoloDeTunel::Tcp,
        porta: 25565,
    }
}

fn entrar_no_tunel() -> ClientMessage {
    ClientMessage::EntrarNoTunel {
        mod_id: MOD.into(),
        nome: NOME.into(),
    }
}

/// A resposta ao pedido de túnel: `Ok(porta)` para o concedido, `Err` para a
/// recusa nomeada.
async fn resposta_de_tunel(par: &mut Par) -> Result<Result<u16, RecusaDeTunel>> {
    esperar(&mut par.ligado.recebe, |m| match m {
        ServerMessage::TunelConcedido { porta, .. } => Some(Ok(*porta)),
        ServerMessage::TunelRecusado { motivo, .. } => Some(Err(*motivo)),
        _ => None,
    })
    .await
    .ok_or_else(|| anyhow::anyhow!("sem resposta de túnel"))
}

fn abertura(de: Option<PersonId>) -> AberturaDeTunel {
    AberturaDeTunel {
        mod_id: MOD.into(),
        nome: NOME.into(),
        fluxo: 1,
        tipo: TipoDeCanalDeTunel::Tcp,
        de,
    }
}

/// A gestora lê o `ctl` numa tarefa e guarda o quadro de cargos mais novo.
fn acompanhar_a_gestora(mut recebe: seele_enlace::Recebe) -> Arc<Mutex<Option<QuadroDeCargos>>> {
    let quadro = Arc::new(Mutex::new(None));
    let guardado = Arc::clone(&quadro);
    tokio::spawn(async move {
        while let Ok(mensagem) = frame::read::<ServerMessage>(&mut recebe).await {
            if let ServerMessage::QuadroDeCargos(q) = mensagem {
                *guardado.lock().unwrap() = Some(q);
            }
        }
    });
    quadro
}

async fn quadro_mais_novo_que(
    quadro: &Arc<Mutex<Option<QuadroDeCargos>>>,
    revisao: Option<u64>,
) -> QuadroDeCargos {
    let ate = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(q) = quadro.lock().unwrap().clone() {
            if revisao.is_none_or(|r| q.revisao > r) {
                return q;
            }
        }
        assert!(Instant::now() < ate, "a gestora não recebeu o quadro");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// A gestora restringe `entrar-no-jogo` no `@everyone`.
async fn revogar_entrar_no_jogo(
    gestora: &mut seele_enlace::Envio,
    quadro: &Arc<Mutex<Option<QuadroDeCargos>>>,
) -> Result<()> {
    let antes = quadro_mais_novo_que(quadro, None).await;
    let todos = antes
        .cargos
        .iter()
        .find(|c| c.id == TODOS)
        .expect("o @everyone")
        .clone();
    let mut restringe = todos.restringe.clone();
    restringe.push(Permissao::DeMod(ChaveDeMod {
        mod_id: MOD.into(),
        acao: ACAO_ENTRAR_NO_JOGO.into(),
    }));
    frame::write(
        gestora,
        &ClientMessage::EditarCargo {
            revisao: antes.revisao,
            cargo: TODOS,
            nome: todos.nome.clone(),
            concede: todos.concede.clone(),
            restringe,
        },
    )
    .await?;
    let _ = quadro_mais_novo_que(quadro, Some(antes.revisao)).await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn entrar_sem_a_acao_e_recusado_com_o_nome() -> Result<()> {
    let (endereco, daemon, _temp) = montar(false).await?;
    let _gestora = entrar(endereco, GESTORA, 12).await?;
    let mut maquina = entrar(endereco, MAQUINA, 12).await?;
    let mut jogador = entrar(endereco, JOGADOR, 12).await?;

    frame::write(&mut maquina.ligado.envio, &servir()).await?;
    assert_eq!(resposta_de_tunel(&mut maquina).await?, Ok(25565));

    // Um nome que ninguém serve é `NaoExiste`, com ou sem permissão.
    frame::write(
        &mut jogador.ligado.envio,
        &ClientMessage::EntrarNoTunel {
            mod_id: MOD.into(),
            nome: "outro".into(),
        },
    )
    .await?;
    assert_eq!(
        resposta_de_tunel(&mut jogador).await?,
        Err(RecusaDeTunel::NaoExiste)
    );

    // O MOD declara a ação e a pessoa não a tem: a recusa tem o nome.
    frame::write(&mut jogador.ligado.envio, &entrar_no_tunel()).await?;
    assert_eq!(
        resposta_de_tunel(&mut jogador).await?,
        Err(RecusaDeTunel::SemPermissao)
    );
    daemon.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn quem_serve_sem_o_poder_porta_e_recusado() -> Result<()> {
    let (endereco, daemon, _temp) = montar(true).await?;
    let _gestora = entrar(endereco, GESTORA, 12).await?;
    let mut maquina = entrar(endereco, MAQUINA, 12).await?;
    // Um MOD que não está ligado não tem poder nenhum.
    frame::write(
        &mut maquina.ligado.envio,
        &ClientMessage::ServirTunel {
            mod_id: "x/outro".into(),
            nome: NOME.into(),
            protocolo: ProtocoloDeTunel::Tcp,
            porta: 1,
        },
    )
    .await?;
    assert_eq!(
        resposta_de_tunel(&mut maquina).await?,
        Err(RecusaDeTunel::ModSemPorta)
    );
    daemon.shutdown();
    Ok(())
}

/// Lê da máquina até um `TunelRetirado` deste túnel: quem foi retirado.
async fn retirada_lida(maquina: &mut Par) -> Option<PersonId> {
    esperar(&mut maquina.ligado.recebe, |m| match m {
        ServerMessage::TunelRetirado { mod_id, nome, quem } if mod_id == MOD && nome == NOME => {
            Some(*quem)
        }
        _ => None,
    })
    .await
}

/// **I1.** Quem deixa o túnel (`SairDoTunel`) e quem some (a conexão cai) são
/// ditos à máquina, que corta a ligação direta dele; o túnel segue de pé.
#[tokio::test(flavor = "multi_thread")]
async fn quem_sai_do_tunel_ou_cai_e_dito_a_maquina() -> Result<()> {
    let (endereco, daemon, _temp) = montar(true).await?;
    let _gestora = entrar(endereco, GESTORA, 12).await?;
    let mut maquina = entrar(endereco, MAQUINA, 12).await?;
    let mut jogador = entrar(endereco, JOGADOR, 12).await?;
    frame::write(&mut maquina.ligado.envio, &servir()).await?;
    assert_eq!(resposta_de_tunel(&mut maquina).await?, Ok(25565));

    // Deixar o túnel.
    frame::write(&mut jogador.ligado.envio, &entrar_no_tunel()).await?;
    assert_eq!(resposta_de_tunel(&mut jogador).await?, Ok(25565));
    frame::write(
        &mut jogador.ligado.envio,
        &ClientMessage::SairDoTunel {
            mod_id: MOD.into(),
            nome: NOME.into(),
        },
    )
    .await?;
    assert_eq!(retirada_lida(&mut maquina).await, Some(jogador.pessoa));

    // Cair com o túnel aberto.
    frame::write(&mut jogador.ligado.envio, &entrar_no_tunel()).await?;
    assert_eq!(resposta_de_tunel(&mut jogador).await?, Ok(25565));
    let pessoa = jogador.pessoa;
    drop(jogador);
    assert_eq!(
        retirada_lida(&mut maquina).await,
        Some(pessoa),
        "a máquina não soube que quem jogava saiu"
    );
    assert!(
        daemon.server().tuneis().existe(MOD, NOME),
        "a saída de quem joga derrubou o túnel da máquina"
    );
    daemon.shutdown();
    Ok(())
}

/// **Servir é provisório e vem desligado** (`ServerConfig::servir_tuneis`): com
/// ele desligado, uma sessão 12 qualquer não serve o nome de túnel de um MOD
/// com `porta`, mesmo com o MOD ligado e o poder declarado. O padrão do
/// produto é o desligado, e o teste o pega pelo `Default`.
#[tokio::test(flavor = "multi_thread")]
async fn sem_o_interruptor_ninguem_serve_um_tunel() -> Result<()> {
    assert!(
        !ServerConfig::default().servir_tuneis,
        "o padrão do produto tem de ser não servir túneis"
    );
    let (endereco, daemon, _temp) = montar_completo(true, true, false).await?;
    let _gestora = entrar(endereco, GESTORA, 12).await?;
    let mut maquina = entrar(endereco, MAQUINA, 12).await?;
    frame::write(&mut maquina.ligado.envio, &servir()).await?;
    // Sem o interruptor, quem decide é o aceite (`crate::maquinas`), e esta
    // sessão não aceitou máquina nenhuma.
    assert_eq!(
        resposta_de_tunel(&mut maquina).await?,
        Err(RecusaDeTunel::ForaDoAceite)
    );
    assert!(
        !daemon.server().tuneis().existe(MOD, NOME),
        "o túnel foi para o livro apesar da recusa"
    );
    daemon.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn revogar_entrar_no_jogo_fecha_o_tunel_aberto() -> Result<()> {
    let (endereco, daemon, _temp) = montar(true).await?;
    let gestora = entrar(endereco, GESTORA, 12).await?;
    let mut maquina = entrar(endereco, MAQUINA, 12).await?;
    let mut jogador = entrar(endereco, JOGADOR, 12).await?;
    let comum::Ligado {
        ponta: _pg,
        conexao: _cg,
        envio: mut envio_da_gestora,
        recebe: recebe_da_gestora,
    } = gestora.ligado;
    let quadro = acompanhar_a_gestora(recebe_da_gestora);

    frame::write(&mut maquina.ligado.envio, &servir()).await?;
    assert_eq!(resposta_de_tunel(&mut maquina).await?, Ok(25565));
    frame::write(&mut jogador.ligado.envio, &entrar_no_tunel()).await?;
    assert_eq!(resposta_de_tunel(&mut jogador).await?, Ok(25565));

    // **A prova de que o repasse anda antes da revogação**: sem ela, o «nada
    // chega depois» abaixo seria verdade de um repasse que nunca funcionou.
    let CanalDeTunel::Ordenado(mut envio, mut recebe) = jogador
        .ligado
        .conexao
        .abrir_tunel(Confiabilidade::Ordenada)
        .await?
    else {
        anyhow::bail!("o t veio sem ordem");
    };
    frame::write(&mut envio, &abertura(None)).await?;
    envio.write_all(b"ola").await?;
    let CanalDeTunel::Ordenado(mut m_envio, mut m_recebe) =
        tokio::time::timeout(PRAZO, maquina.ligado.conexao.aceitar_tunel()).await??
    else {
        anyhow::bail!("o t chegou sem ordem");
    };
    let chegou: AberturaDeTunel = frame::read(&mut m_recebe).await?;
    assert_eq!(chegou.de, Some(jogador.pessoa), "o anfitrião diz quem joga");
    assert_eq!(chegou.nome, NOME);
    let mut tres = [0_u8; 3];
    m_recebe.read_exact(&mut tres).await?;
    assert_eq!(&tres, b"ola");
    m_envio.write_all(b"eco").await?;
    let mut volta = [0_u8; 3];
    recebe.read_exact(&mut volta).await?;
    assert_eq!(&volta, b"eco");
    let antes = daemon.server().contador_de_repasse(MOD, NOME);
    assert!(antes >= 6, "o contador não contou o repasse: {antes}");

    // O dono revoga. A pessoa fica sabendo, e com o nome do motivo.
    revogar_entrar_no_jogo(&mut envio_da_gestora, &quadro).await?;
    let motivo = esperar(&mut jogador.ligado.recebe, |m| match m {
        ServerMessage::TunelEncerrado { motivo, .. } => Some(*motivo),
        _ => None,
    })
    .await;
    assert_eq!(motivo, Some(MotivoDoFimDoTunel::SemPermissao));
    // **A máquina também fica sabendo.** O servidor só confere os canais que
    // passam por ele; a ligação direta é dela, e sem este aviso ela só soube
    // do túnel inteiro acabar.
    assert_eq!(
        retirada_lida(&mut maquina).await,
        Some(jogador.pessoa),
        "a máquina não foi avisada de que a pessoa perdeu o túnel"
    );

    // O canal que já andava acaba, e um canal aberto agora é cortado sem tocar
    // na máquina.
    let fim = tokio::time::timeout(PRAZO, async {
        let mut sobra = [0_u8; 16];
        while let Ok(Some(_)) = recebe.read(&mut sobra).await {}
    })
    .await;
    assert!(
        fim.is_ok(),
        "o canal aberto seguiu de pé depois da revogação"
    );
    let depois_de_fechar = daemon.server().contador_de_repasse(MOD, NOME);

    let CanalDeTunel::Ordenado(mut novo, _novo_recebe) = jogador
        .ligado
        .conexao
        .abrir_tunel(Confiabilidade::Ordenada)
        .await?
    else {
        anyhow::bail!("o t veio sem ordem");
    };
    frame::write(&mut novo, &abertura(None)).await?;
    let _ = novo.write_all(b"de novo").await;
    let chegada = tokio::time::timeout(
        Duration::from_millis(1500),
        maquina.ligado.conexao.aceitar_tunel(),
    )
    .await;
    assert!(
        chegada.is_err(),
        "um canal sem concessão chegou à máquina do jogo"
    );
    assert_eq!(
        daemon.server().contador_de_repasse(MOD, NOME),
        depois_de_fechar,
        "o contador de repasse subiu para um canal sem concessão"
    );
    assert!(
        daemon.server().canais_sem_concessao() >= 1,
        "o canal sem concessão não foi contado"
    );
    daemon.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_maquina_que_sai_encerra_o_tunel_de_quem_estava_dentro() -> Result<()> {
    let (endereco, daemon, _temp) = montar(true).await?;
    let _gestora = entrar(endereco, GESTORA, 12).await?;
    let mut maquina = entrar(endereco, MAQUINA, 12).await?;
    let mut jogador = entrar(endereco, JOGADOR, 12).await?;
    frame::write(&mut maquina.ligado.envio, &servir()).await?;
    assert_eq!(resposta_de_tunel(&mut maquina).await?, Ok(25565));
    frame::write(&mut jogador.ligado.envio, &entrar_no_tunel()).await?;
    assert_eq!(resposta_de_tunel(&mut jogador).await?, Ok(25565));

    drop(maquina);
    let motivo = esperar(&mut jogador.ligado.recebe, |m| match m {
        ServerMessage::TunelEncerrado { motivo, .. } => Some(*motivo),
        _ => None,
    })
    .await;
    assert_eq!(motivo, Some(MotivoDoFimDoTunel::MaquinaSaiu));
    daemon.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn um_par_onze_nunca_recebe_mensagem_de_tunel() -> Result<()> {
    let (endereco, daemon, _temp) = montar(true).await?;
    let _gestora = entrar(endereco, GESTORA, 12).await?;
    let mut maquina = entrar(endereco, MAQUINA, 12).await?;
    let mut velho = entrar(endereco, VELHO, 11).await?;
    // O que o velho lê daqui em diante, de tudo que o servidor lhe mandar.
    let lidos: Arc<Mutex<Vec<ServerMessage>>> = Arc::new(Mutex::new(Vec::new()));
    {
        let lidos = Arc::clone(&lidos);
        let mut recebe = velho.ligado.recebe;
        tokio::spawn(async move {
            while let Ok(m) = frame::read::<ServerMessage>(&mut recebe).await {
                lidos.lock().unwrap().push(m);
            }
        });
    }
    frame::write(&mut maquina.ligado.envio, &servir()).await?;
    assert_eq!(resposta_de_tunel(&mut maquina).await?, Ok(25565));

    // O velho tenta entrar assim mesmo, com o verbo que não conhece: o servidor
    // não lhe responde com mensagem que nasceu no 12.
    let _ = frame::write(&mut velho.ligado.envio, &entrar_no_tunel()).await;
    frame::write(
        &mut maquina.ligado.envio,
        &ClientMessage::PararTunel {
            mod_id: MOD.into(),
            nome: NOME.into(),
        },
    )
    .await?;
    tokio::time::sleep(Duration::from_millis(800)).await;
    let tuneis: Vec<_> = lidos
        .lock()
        .unwrap()
        .iter()
        .filter(|m| {
            matches!(
                m,
                ServerMessage::TunelConcedido { .. }
                    | ServerMessage::TunelRecusado { .. }
                    | ServerMessage::TunelEncerrado { .. }
                    | ServerMessage::SirvaTunelPara { .. }
                    | ServerMessage::AlcanceTunelPor { .. }
            )
        })
        .cloned()
        .collect();
    assert!(tuneis.is_empty(), "o par 11 recebeu {tuneis:?}");
    daemon.shutdown();
    Ok(())
}

/// Desligar o MOD fecha os túneis dele **no servidor**, sem sessão nenhuma para
/// ajudar: o túnel é posto no livro à mão, com uma conexão que nenhuma sessão
/// tem, e só a tarefa que acompanha o conjunto de MODs pode tirá-lo. (Com
/// sessões de verdade a saída delas também fecharia o túnel, e o teste não
/// distinguiria uma coisa da outra: o servidor derruba quem está dentro quando
/// o conjunto de MODs muda, `ModsMudaram`.)
#[tokio::test(flavor = "multi_thread")]
async fn desligar_o_mod_fecha_os_tuneis_dele_no_servidor() -> Result<()> {
    let (_endereco, daemon, _temp) = montar(true).await?;
    daemon
        .server()
        .tuneis()
        .servir(
            PersonId(77),
            12345,
            seele_server::tuneis::PedidoDeTunel {
                mod_id: MOD,
                nome: NOME,
                protocolo: ProtocoloDeTunel::Tcp,
                porta: 1,
            },
            &[seele_proto::mods::Poder::Porta],
        )
        .expect("serve");
    // Um MOD ligado e com o poder não fecha nada.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(daemon.server().tuneis().existe(MOD, NOME));

    {
        let db = daemon.server().persistence.lock().await;
        seele_server::persistence::mods::disable(&db, MOD)?;
    }
    let ate = Instant::now() + PRAZO;
    while daemon.server().tuneis().existe(MOD, NOME) {
        assert!(
            Instant::now() < ate,
            "o MOD foi desligado e o túnel dele seguiu de pé"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    daemon.shutdown();
    Ok(())
}

/// Os datagramas de um canal `j` ao anfitrião: o tag na frente de tudo.
fn datagrama_de_dado(carga: &[u8]) -> Vec<u8> {
    let mut d = vec![seele_proto::tunel::TAG_DE_DADO];
    d.extend_from_slice(carga);
    d
}

/// Abre um túnel UDP servido pela máquina e deixa o jogador dentro dele.
async fn udp_pronto(
    endereco: SocketAddr,
) -> Result<(Par, Par, seele_enlace::Datagramas, AberturaDeTunel)> {
    let mut maquina = entrar(endereco, MAQUINA, 12).await?;
    let mut jogador = entrar(endereco, JOGADOR, 12).await?;
    frame::write(
        &mut maquina.ligado.envio,
        &ClientMessage::ServirTunel {
            mod_id: MOD.into(),
            nome: NOME.into(),
            protocolo: ProtocoloDeTunel::Udp,
            porta: 19132,
        },
    )
    .await?;
    assert_eq!(resposta_de_tunel(&mut maquina).await?, Ok(19132));
    frame::write(&mut jogador.ligado.envio, &entrar_no_tunel()).await?;
    assert_eq!(resposta_de_tunel(&mut jogador).await?, Ok(19132));
    let CanalDeTunel::SemOrdem(dele) = jogador
        .ligado
        .conexao
        .abrir_tunel(Confiabilidade::SemOrdem)
        .await?
    else {
        anyhow::bail!("o j veio ordenado");
    };
    let udp = AberturaDeTunel {
        tipo: TipoDeCanalDeTunel::Udp,
        fluxo: 7,
        ..abertura(None)
    };
    Ok((maquina, jogador, dele, udp))
}

/// O `j` pela estrela: a abertura vem marcada, a carga também, e os datagramas
/// andam nos dois sentidos.
#[tokio::test(flavor = "multi_thread")]
async fn o_udp_vai_pela_estrela_com_a_abertura_marcada_e_a_carga_marcada() -> Result<()> {
    let (endereco, daemon, _temp) = montar(true).await?;
    let _gestora = entrar(endereco, GESTORA, 12).await?;
    let (maquina, jogador, mut dele, udp) = udp_pronto(endereco).await?;
    dele.mandar(&seele_proto::tunel::datagrama_de_abertura(&udp)?)?;
    let CanalDeTunel::SemOrdem(mut da_maquina) =
        tokio::time::timeout(PRAZO, maquina.ligado.conexao.aceitar_tunel()).await??
    else {
        anyhow::bail!("a máquina recebeu um t");
    };
    let primeiro = tokio::time::timeout(PRAZO, da_maquina.receber())
        .await?
        .expect("o primeiro datagrama");
    let seele_proto::tunel::DatagramaJ::Abertura(corpo) =
        seele_proto::tunel::ler_datagrama_j(&primeiro)
    else {
        anyhow::bail!("o primeiro datagrama não é uma abertura marcada: {primeiro:?}");
    };
    let chegou: AberturaDeTunel = seele_proto::control::decode(corpo)?;
    assert_eq!(chegou.de, Some(jogador.pessoa));
    assert_eq!(chegou.fluxo, 7);
    // Os datagramas andam, sem garantia: manda-se até chegar um de cada lado.
    let ate = Instant::now() + PRAZO;
    let mut ida = false;
    let mut volta = false;
    while Instant::now() < ate && !(ida && volta) {
        let _ = dele.mandar(&datagrama_de_dado(b"ping"));
        let _ = da_maquina.mandar(&datagrama_de_dado(b"pong"));
        if let Ok(Some(b)) =
            tokio::time::timeout(Duration::from_millis(100), da_maquina.receber()).await
        {
            ida |= b == datagrama_de_dado(b"ping");
        }
        if let Ok(Some(b)) = tokio::time::timeout(Duration::from_millis(100), dele.receber()).await
        {
            volta |= b == datagrama_de_dado(b"pong");
        }
    }
    assert!(ida && volta, "ida {ida}, volta {volta}");
    assert!(daemon.server().contador_de_repasse(MOD, NOME) >= 8);
    daemon.shutdown();
    Ok(())
}

/// **I2.** O `j` não ordena nem retransmite: carga que chega **antes** da
/// abertura (ela se atrasou ou se perdeu) é descartada e contada, e o canal
/// **não** é cortado nem conta como falta de concessão. Quando a abertura chega
/// o repasse segue, e cada repetição dela chega à máquina reescrita pelo
/// anfitrião (`de: Some(quem abriu)`), nunca como quem joga a mandou.
#[tokio::test(flavor = "multi_thread")]
async fn carga_antes_da_abertura_nao_corta_o_canal_j_e_a_repeticao_chega_reescrita() -> Result<()> {
    let (endereco, daemon, _temp) = montar(true).await?;
    let _gestora = entrar(endereco, GESTORA, 12).await?;
    let (maquina, jogador, dele, udp) = udp_pronto(endereco).await?;

    // A abertura «se perdeu»: o primeiro que o anfitrião vê é carga.
    dele.mandar(&datagrama_de_dado(b"cedo demais"))?;
    dele.mandar(&datagrama_de_dado(b"outro cedo"))?;
    // E depois ela chega, repetida pelo jogador.
    let abertura_marcada = seele_proto::tunel::datagrama_de_abertura(&udp)?;
    dele.mandar(&abertura_marcada)?;
    let CanalDeTunel::SemOrdem(mut da_maquina) =
        tokio::time::timeout(PRAZO, maquina.ligado.conexao.aceitar_tunel()).await??
    else {
        anyhow::bail!("a máquina recebeu um t");
    };
    let primeiro = tokio::time::timeout(PRAZO, da_maquina.receber())
        .await?
        .expect("o primeiro datagrama");
    let seele_proto::tunel::DatagramaJ::Abertura(corpo) =
        seele_proto::tunel::ler_datagrama_j(&primeiro)
    else {
        anyhow::bail!("a máquina não leu a abertura primeiro, e sim: {primeiro:?}");
    };
    let chegou: AberturaDeTunel = seele_proto::control::decode(corpo)?;
    assert_eq!(chegou.de, Some(jogador.pessoa));
    assert_eq!(
        daemon.server().datagramas_j_antes_da_abertura(),
        2,
        "a carga que veio antes da abertura não foi contada"
    );
    assert_eq!(
        daemon.server().canais_sem_concessao(),
        0,
        "a carga antes da abertura foi tratada como falta de concessão"
    );

    // A repetição da abertura (a que a máquina pode ter perdido no trecho até
    // ela) chega reescrita, e a carga segue atrás dela.
    let ate = Instant::now() + PRAZO;
    let mut repetida = false;
    let mut carga = false;
    while Instant::now() < ate && !(repetida && carga) {
        let _ = dele.mandar(&abertura_marcada);
        let _ = dele.mandar(&datagrama_de_dado(b"ping"));
        while let Ok(Some(b)) =
            tokio::time::timeout(Duration::from_millis(100), da_maquina.receber()).await
        {
            match seele_proto::tunel::ler_datagrama_j(&b) {
                seele_proto::tunel::DatagramaJ::Abertura(corpo) => {
                    let de = seele_proto::control::decode::<AberturaDeTunel>(corpo)?.de;
                    assert_eq!(de, Some(jogador.pessoa), "a repetição foi sem reescrever");
                    repetida = true;
                }
                seele_proto::tunel::DatagramaJ::Dado(c) => carga |= c == b"ping",
                seele_proto::tunel::DatagramaJ::Ilegivel => {}
            }
        }
    }
    assert!(repetida && carga, "repetida {repetida}, carga {carga}");
    daemon.shutdown();
    Ok(())
}

/// Uma abertura **malformada** corta o canal: é o único motivo, além do prazo.
#[tokio::test(flavor = "multi_thread")]
async fn abertura_malformada_no_canal_j_corta_e_conta() -> Result<()> {
    let (endereco, daemon, _temp) = montar(true).await?;
    let _gestora = entrar(endereco, GESTORA, 12).await?;
    let (_maquina, _jogador, dele, _udp) = udp_pronto(endereco).await?;
    let mut lixo = vec![seele_proto::tunel::TAG_DE_ABERTURA];
    lixo.extend_from_slice(&[0xff; 12]);
    dele.mandar(&lixo)?;
    let ate = Instant::now() + PRAZO;
    while daemon.server().canais_sem_concessao() == 0 && Instant::now() < ate {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(daemon.server().canais_sem_concessao(), 1);
    assert_eq!(daemon.server().datagramas_j_antes_da_abertura(), 0);
    daemon.shutdown();
    Ok(())
}

/// **O corte do repasse chega a quem abriu o canal, com o nome.**
///
/// O túnel é posto no livro à mão, **sem a conexão da máquina registrada**, e o
/// jogador (de verdade, conectado) entra nele. O canal que ele abre é cortado
/// pelo anfitrião por `MaquinaSaiu`, e ele tem de ler `TunelRecusado`, e não só
/// ver o canal fechar. (O teto de canais da máquina, `CanaisEsgotados`, é o
/// mesmo caminho com outro motivo; o mapeamento dele está provado na unidade,
/// porque o teto é simétrico e um jogador só bate antes no da própria conexão.)
#[tokio::test(flavor = "multi_thread")]
async fn o_corte_do_repasse_chega_a_quem_abriu_o_canal_como_recusa_nomeada() -> Result<()> {
    let (endereco, daemon, _temp) = montar(true).await?;
    let _gestora = entrar(endereco, GESTORA, 12).await?;
    let mut jogador = entrar(endereco, JOGADOR, 12).await?;
    {
        let mut livro = daemon.server().tuneis();
        livro
            .servir(
                PersonId(77),
                12345,
                seele_server::tuneis::PedidoDeTunel {
                    mod_id: MOD,
                    nome: NOME,
                    protocolo: ProtocoloDeTunel::Tcp,
                    porta: 1,
                },
                &[seele_proto::mods::Poder::Porta],
            )
            .expect("serve");
        livro.entrar(jogador.pessoa, 1, MOD, NOME).expect("entra");
    }
    let CanalDeTunel::Ordenado(mut envio, _recebe) = jogador
        .ligado
        .conexao
        .abrir_tunel(Confiabilidade::Ordenada)
        .await?
    else {
        anyhow::bail!("o t veio sem ordem");
    };
    frame::write(&mut envio, &abertura(None)).await?;
    let recusa = esperar(&mut jogador.ligado.recebe, |m| match m {
        ServerMessage::TunelRecusado {
            mod_id,
            nome,
            motivo,
        } => Some((mod_id.clone(), nome.clone(), *motivo)),
        _ => None,
    })
    .await;
    assert_eq!(
        recusa,
        Some((MOD.to_owned(), NOME.to_owned(), RecusaDeTunel::MaquinaSaiu))
    );
    daemon.shutdown();
    Ok(())
}

/// Declara identidade para o caminho entre pares: sem ela, o servidor não tem o
/// que apresentar e o túnel vai pela estrela.
async fn declarar(par: &mut Par, letra: char) -> Result<()> {
    // Quem não empresta não publica endereço local; o público o servidor vê.
    frame::write(
        &mut par.ligado.envio,
        &ClientMessage::EmprestarSubida {
            consentimento: seele_proto::control::ConsentimentoDePar::de_ninguem(),
            impressao: letra.to_string().repeat(64),
            locais: Vec::new(),
        },
    )
    .await?;
    Ok(())
}

/// O que cada ponta lê do túnel, em ordem, até o prazo: o resumo das mensagens
/// de túnel de uma ponta.
async fn tuneis_lidos(par: &mut Par, ate_ver: usize) -> Vec<&'static str> {
    let mut vistos = Vec::new();
    let fim = Instant::now() + Duration::from_secs(2);
    while Instant::now() < fim && vistos.len() < ate_ver {
        let Ok(Ok(m)) = tokio::time::timeout(
            Duration::from_millis(200),
            frame::read::<ServerMessage>(&mut par.ligado.recebe),
        )
        .await
        else {
            continue;
        };
        match m {
            ServerMessage::TunelConcedido { .. } => vistos.push("concedido"),
            ServerMessage::SirvaTunelPara { .. } => vistos.push("sirva"),
            ServerMessage::AlcanceTunelPor { .. } => vistos.push("alcance"),
            _ => {}
        }
    }
    vistos
}

/// A apresentação sai depois do `TunelConcedido`: a máquina lê o dela e depois
/// o `SirvaTunelPara`, e quem joga lê o dele e depois o `AlcanceTunelPor`.
#[tokio::test(flavor = "multi_thread")]
async fn a_apresentacao_do_tunel_sai_depois_do_concedido() -> Result<()> {
    let (endereco, daemon, _temp) = montar(true).await?;
    let _gestora = entrar(endereco, GESTORA, 12).await?;
    let mut maquina = entrar(endereco, MAQUINA, 12).await?;
    let mut jogador = entrar(endereco, JOGADOR, 12).await?;
    declarar(&mut maquina, 'a').await?;
    declarar(&mut jogador, 'b').await?;
    // O servidor lê a declaração antes do verbo seguinte (mesmo fluxo, em ordem).
    frame::write(&mut maquina.ligado.envio, &servir()).await?;
    assert_eq!(resposta_de_tunel(&mut maquina).await?, Ok(25565));
    frame::write(&mut jogador.ligado.envio, &entrar_no_tunel()).await?;

    assert_eq!(
        tuneis_lidos(&mut jogador, 2).await,
        ["concedido", "alcance"]
    );
    assert_eq!(tuneis_lidos(&mut maquina, 1).await, ["sirva"]);
    daemon.shutdown();
    Ok(())
}

/// Com a malha desligada o servidor não apresenta ninguém: o jogador recebe o
/// `TunelConcedido` e nada mais, e o túnel vai pela estrela.
#[tokio::test(flavor = "multi_thread")]
async fn sem_malha_nao_ha_apresentacao() -> Result<()> {
    let (endereco, daemon, _temp) = montar_com_malha(true, false).await?;
    let _gestora = entrar(endereco, GESTORA, 12).await?;
    let mut maquina = entrar(endereco, MAQUINA, 12).await?;
    let mut jogador = entrar(endereco, JOGADOR, 12).await?;
    declarar(&mut maquina, 'a').await?;
    declarar(&mut jogador, 'b').await?;
    frame::write(&mut maquina.ligado.envio, &servir()).await?;
    assert_eq!(resposta_de_tunel(&mut maquina).await?, Ok(25565));
    frame::write(&mut jogador.ligado.envio, &entrar_no_tunel()).await?;

    assert_eq!(tuneis_lidos(&mut jogador, 2).await, ["concedido"]);
    assert!(tuneis_lidos(&mut maquina, 1).await.is_empty());
    daemon.shutdown();
    Ok(())
}

/// **O teto de canais por pessoa, de ponta a ponta.** A conexão da máquina tem
/// 64 canais para todos; uma pessoa que abre canais sem parar não pode
/// esgotá-los. O 17º canal vivo dela é cortado e ela lê `CanaisEsgotados` (a
/// prova que faltava de que o motivo chega a quem joga); outra pessoa ainda
/// chega à máquina; e fechado um dos 16 a vaga volta.
///
/// Os canais são `j` (UDP) de propósito: o teto vale para todo canal repassado,
/// e o `j` não passa pelo crédito de leitura do enlace
/// (`MAX_CREDITO_DA_CONEXAO`), que com 16 canais `t` parados abertos deixa o
/// 17º sem receber byte e faria o teste medir o enlace, e não o teto.
#[tokio::test(flavor = "multi_thread")]
async fn uma_pessoa_nao_esgota_os_canais_da_maquina() -> Result<()> {
    const OUTRO: u8 = 4;
    let teto = seele_server::tuneis::MAX_CANAIS_REPASSADOS_POR_PESSOA;
    let (endereco, daemon, _temp) = montar(true).await?;
    let _gestora = entrar(endereco, GESTORA, 12).await?;
    let (maquina, mut jogador, primeiro, udp) = udp_pronto(endereco).await?;
    let mut outro = entrar(endereco, OUTRO, 12).await?;
    frame::write(&mut outro.ligado.envio, &entrar_no_tunel()).await?;
    assert_eq!(resposta_de_tunel(&mut outro).await?, Ok(19132));

    // Abre um canal `j` de `quem` e manda a abertura (repete-se onde o `j`
    // pode perdê-la).
    async fn abrir(quem: &Par, udp: &AberturaDeTunel) -> Result<seele_enlace::Datagramas> {
        let CanalDeTunel::SemOrdem(canal) = quem
            .ligado
            .conexao
            .abrir_tunel(Confiabilidade::SemOrdem)
            .await?
        else {
            anyhow::bail!("o j veio ordenado");
        };
        canal.mandar(&seele_proto::tunel::datagrama_de_abertura(udp)?)?;
        Ok(canal)
    }
    // Espera o próximo canal chegar à máquina, repetindo a abertura de `canal`
    // enquanto não vem.
    async fn chega(
        maquina: &Par,
        canal: &mut seele_enlace::Datagramas,
        udp: &AberturaDeTunel,
    ) -> Result<Option<seele_enlace::Datagramas>> {
        let ate = Instant::now() + PRAZO;
        while Instant::now() < ate {
            let _ = canal.mandar(&seele_proto::tunel::datagrama_de_abertura(udp)?);
            if let Ok(chegou) = tokio::time::timeout(
                Duration::from_millis(300),
                maquina.ligado.conexao.aceitar_tunel(),
            )
            .await
            {
                let CanalDeTunel::SemOrdem(d) = chegou? else {
                    anyhow::bail!("a máquina recebeu um t");
                };
                return Ok(Some(d));
            }
        }
        Ok(None)
    }

    // O `_primeiro` de `udp_pronto` já é um canal vivo do jogador.
    let mut dele = vec![primeiro];
    let mut da_maquina = Vec::new();
    let mut primeiro_da_maquina = None;
    {
        // O primeiro canal ainda não foi anunciado: a abertura dele sai agora.
        let c = dele.first_mut().expect("o primeiro canal");
        primeiro_da_maquina = chega(&maquina, c, &udp).await?.or(primeiro_da_maquina);
        da_maquina.push(
            primeiro_da_maquina
                .take()
                .expect("o primeiro chegou à máquina"),
        );
    }
    while dele.len() < teto {
        let mut novo = abrir(&jogador, &udp).await?;
        da_maquina.push(
            chega(&maquina, &mut novo, &udp)
                .await?
                .expect("o canal dentro do teto chegou à máquina"),
        );
        dele.push(novo);
    }
    assert_eq!(daemon.server().canais_acima_do_teto(), 0);

    // O 17º é cortado, e quem joga lê o motivo.
    let corte = abrir(&jogador, &udp).await?;
    let recusa = {
        let ate = Instant::now() + PRAZO;
        let mut visto = None;
        while visto.is_none() && Instant::now() < ate {
            let _ = corte.mandar(&seele_proto::tunel::datagrama_de_abertura(&udp)?);
            visto = esperar_curto(&mut jogador.ligado.recebe).await;
        }
        visto
    };
    assert_eq!(recusa, Some(RecusaDeTunel::CanaisEsgotados));
    assert_eq!(daemon.server().canais_acima_do_teto(), 1);
    drop(corte);

    // Outra pessoa ainda chega à máquina: o teto é por pessoa.
    let mut do_outro = abrir(&outro, &udp).await?;
    let _chegou = chega(&maquina, &mut do_outro, &udp)
        .await?
        .expect("o canal de outra pessoa chegou à máquina");

    // Fechado um dos 16, a vaga volta e o próximo passa.
    drop(dele.remove(0));
    let mut novo = abrir(&jogador, &udp).await?;
    let passou = chega(&maquina, &mut novo, &udp).await?;
    assert!(
        passou.is_some(),
        "a vaga não voltou depois que um canal fechou"
    );
    assert_eq!(
        daemon.server().canais_acima_do_teto(),
        1,
        "só o 17º foi cortado"
    );
    drop(da_maquina);
    daemon.shutdown();
    Ok(())
}

/// Um `TunelRecusado` no controle, se vier em 300 ms.
async fn esperar_curto(recebe: &mut seele_enlace::Recebe) -> Option<RecusaDeTunel> {
    let Ok(Ok(ServerMessage::TunelRecusado { motivo, .. })) = tokio::time::timeout(
        Duration::from_millis(300),
        frame::read::<ServerMessage>(recebe),
    )
    .await
    else {
        return None;
    };
    Some(motivo)
}
