//! Os cargos de ponta a ponta (ADR 0064): servidor de verdade, `Connection` de
//! verdade — o mesmo objeto que o comando Tauri segura —, pelo fio da linha 2.
//!
//! O que só se mede com as duas pontas de pé:
//!
//! - o dono declarado pela máquina (`ServerConfig::proprietario`) recebe o
//!   quadro e manda; quem chega primeiro não vira nada;
//! - um verbo aceito chega a TODAS as sessões (o quadro novo) e muda na hora o
//!   que a pessoa atingida pode (o `SuasPermissoes` dela);
//! - a revisão velha e a escalada são recusadas pelo SERVIDOR, com o motivo
//!   nomeado, e nada é gravado;
//! - o que foi gravado atravessa o reinício do servidor;
//! - dois servidores não se misturam, nem as duas sessões de uma mesma pessoa.

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
use seele_ffi::cargos::{PedidoDeCargo, Permissao, QuadroDeCargos, RecusaDeCargo};
use seele_ffi::{ConnectConfig, Connection, ConnectionError};
use seele_proto::control::Permission;
use seele_proto::ids::{PersonId, RoleId};
use seele_server::permissions::{COMMANDER_ROLE, OBSERVER_ROLE};
use seele_server::persistence::{Location, Persistence};
use seele_server::{Daemon, ServerConfig};

mod vaga;

const PRAZO: Duration = Duration::from_secs(10);

fn casa(nome: &str) -> String {
    let mut caminho = std::env::temp_dir();
    caminho.push(format!("seele-cargos-{nome}-{}", std::process::id()));
    caminho.to_string_lossy().into_owned()
}

fn nascer(nome: &str) -> String {
    let caminho = casa(nome);
    let _ = std::fs::remove_dir_all(&caminho);
    caminho
}

/// A impressão da chave de `nome` — a que a máquina que hospeda declara.
fn impressao_de(nome: &str) -> String {
    seele_ffi::impressao_desta_maquina(&casa(nome)).expect("a identidade")
}

async fn subir(
    arquivo: &std::path::Path,
    proprietario: Option<String>,
) -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::File(arquivo.to_path_buf()),
        proprietario,
        ..ServerConfig::default()
    };
    let servidor = Arc::new(Daemon::bind(config).await?);
    let endereco = servidor.local_addr()?;
    let aceitando = Arc::clone(&servidor);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });
    Ok((endereco, servidor))
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

fn quadro(connection: &Connection) -> Option<QuadroDeCargos> {
    connection.cargos_na_tela().quadro
}

fn revisao(connection: &Connection) -> u64 {
    quadro(connection).expect("o quadro chegou").revisao
}

fn cargo_chamado(connection: &Connection, nome: &str) -> Option<RoleId> {
    quadro(connection)?
        .cargos
        .into_iter()
        .find(|cargo| cargo.nome == nome)
        .map(|cargo| cargo.id)
}

fn tem_o_cargo(connection: &Connection, pessoa: PersonId, cargo: RoleId) -> bool {
    quadro(connection).is_some_and(|quadro| {
        quadro
            .atribuicoes
            .iter()
            .any(|atribuicao| atribuicao.pessoa == pessoa && atribuicao.cargos.contains(&cargo))
    })
}

fn pode(connection: &Connection, permissao: Permission) -> bool {
    connection
        .cargos_na_tela()
        .suas
        .is_some_and(|suas| suas.permissoes.contains(&permissao))
}

fn recusa(connection: &Connection) -> Option<RecusaDeCargo> {
    connection
        .cargos_na_tela()
        .ultima_recusa
        .map(|recusa| recusa.motivo)
}

fn eu(connection: &Connection) -> PersonId {
    PersonId(connection.snapshot().me.expect("identidade"))
}

#[tokio::test(flavor = "multi_thread")]
async fn os_cargos_pelo_fio_de_ponta_a_ponta() -> Result<()> {
    let _vaga = vaga::minha();
    let pasta = tempfile::tempdir()?;
    let arquivo = pasta.path().join("seele.db");
    for nome in ["dono", "gestora", "pessoa"] {
        let _ = nascer(nome);
    }
    // Declarado pela máquina, ANTES de qualquer conexão — como o app faz.
    let (endereco, servidor) = subir(&arquivo, Some(impressao_de("dono"))).await?;

    // Quem chega primeiro não é a dona: chega a pessoa comum antes de todos.
    let pessoa = voltar(endereco, "pessoa").await?;
    let dono = voltar(endereco, "dono").await?;
    let gestora = voltar(endereco, "gestora").await?;
    for conexao in [&pessoa, &dono, &gestora] {
        assert!(
            ate(conexao, |c| quadro(c).is_some()),
            "o quadro não chegou depois do Session"
        );
    }
    assert!(
        ate(&dono, |c| c
            .cargos_na_tela()
            .suas
            .is_some_and(|suas| suas.proprietario)),
        "a dona declarada não se viu dona"
    );
    assert!(
        !pessoa.cargos_na_tela().suas.unwrap().proprietario,
        "quem chegou primeiro virou dono"
    );
    assert!(!pode(&pessoa, Permission::ManageRoles));
    let (id_pessoa, id_gestora) = (eu(&pessoa), eu(&gestora));

    // A dona cria a Moderação, a põe acima do Pessoa e a dá à gestora.
    let concede = [Permission::ManageRoles, Permission::Kick, Permission::Speak]
        .into_iter()
        .map(Permissao::Produto)
        .collect();
    dono.pedido_de_cargo(
        revisao(&dono),
        PedidoDeCargo::Criar {
            nome: "Moderação".into(),
            concede,
            restringe: vec![],
        },
    )?;
    assert!(
        ate(&gestora, |c| cargo_chamado(c, "Moderação").is_some()),
        "o quadro novo não chegou às outras sessões"
    );
    let moderacao = cargo_chamado(&dono, "Moderação").unwrap();
    assert!(ate(&dono, |c| cargo_chamado(c, "Moderação").is_some()));
    dono.pedido_de_cargo(
        revisao(&dono),
        PedidoDeCargo::Mover {
            cargo: moderacao,
            posicao: 4,
        },
    )?;
    assert!(ate(&dono, |c| quadro(c)
        .unwrap()
        .cargos
        .iter()
        .any(|cargo| cargo.id == moderacao && cargo.posicao == 4)));
    dono.pedido_de_cargo(
        revisao(&dono),
        PedidoDeCargo::Atribuir {
            pessoa: id_gestora,
            cargo: moderacao,
        },
    )?;
    assert!(
        ate(&gestora, |c| pode(c, Permission::ManageRoles)),
        "a gestora não passou a gerenciar cargos na hora"
    );

    // Silenciar: a gestora dá o Observador à pessoa, e a pessoa perde Falar NA HORA.
    assert!(pode(&pessoa, Permission::Speak));
    let lida = revisao(&gestora);
    gestora.pedido_de_cargo(
        lida,
        PedidoDeCargo::Atribuir {
            pessoa: id_pessoa,
            cargo: OBSERVER_ROLE,
        },
    )?;
    assert!(
        ate(&pessoa, |c| !pode(c, Permission::Speak)),
        "a revogação não valeu na hora para quem foi atingida"
    );
    assert!(ate(&dono, |c| tem_o_cargo(c, id_pessoa, OBSERVER_ROLE)));

    // A revisão velha: o servidor recusa, nomeado, e não grava.
    gestora.pedido_de_cargo(
        lida,
        PedidoDeCargo::Revogar {
            pessoa: id_pessoa,
            cargo: OBSERVER_ROLE,
        },
    )?;
    assert!(
        ate(&gestora, |c| matches!(
            recusa(c),
            Some(RecusaDeCargo::RevisaoVelha { .. })
        )),
        "a edição velha não foi recusada: {:?}",
        recusa(&gestora)
    );
    assert!(
        tem_o_cargo(&dono, id_pessoa, OBSERVER_ROLE),
        "a edição velha desfez a mudança"
    );

    // A escalada: a pessoa tenta se dar o Comandante. O servidor recusa.
    pessoa.pedido_de_cargo(
        revisao(&pessoa),
        PedidoDeCargo::Atribuir {
            pessoa: id_pessoa,
            cargo: COMMANDER_ROLE,
        },
    )?;
    assert!(
        ate(&pessoa, |c| recusa(c) == Some(RecusaDeCargo::SemPermissao)),
        "a escalada não foi recusada pelo servidor: {:?}",
        recusa(&pessoa)
    );
    // E a gestora, tentando conceder o que não tem (Banir), também não passa.
    gestora.pedido_de_cargo(
        revisao(&gestora),
        PedidoDeCargo::Criar {
            nome: "Banidores".into(),
            concede: vec![Permissao::Produto(Permission::Ban)],
            restringe: vec![],
        },
    )?;
    assert!(
        ate(&gestora, |c| matches!(
            recusa(c),
            Some(RecusaDeCargo::NaoPodeConceder { .. })
        )),
        "a gestora concedeu o que não tem: {:?}",
        recusa(&gestora)
    );
    let quadro_antes = quadro(&dono).unwrap();
    assert!(quadro_antes
        .cargos
        .iter()
        .all(|cargo| cargo.nome != "Banidores"));

    // O reinício: o mesmo arquivo, o mesmo quadro, a mesma pessoa calada.
    for conexao in [pessoa, dono, gestora] {
        conexao.disconnect();
    }
    servidor.shutdown();
    servidor.wait_idle().await;
    let (endereco, servidor) = subir(&arquivo, Some(impressao_de("dono"))).await?;
    let pessoa = voltar(endereco, "pessoa").await?;
    assert!(ate(&pessoa, |c| quadro(c).is_some()));
    let depois = quadro(&pessoa).unwrap();
    assert_eq!(
        depois.revisao, quadro_antes.revisao,
        "a revisão não atravessou o reinício"
    );
    assert_eq!(depois.cargos, quadro_antes.cargos);
    assert!(tem_o_cargo(&pessoa, id_pessoa, OBSERVER_ROLE));
    assert!(ate(&pessoa, |c| c.cargos_na_tela().suas.is_some()));
    assert!(
        !pode(&pessoa, Permission::Speak),
        "o silêncio não atravessou o reinício"
    );
    // E no banco, a leitura mais crua: a pessoa não tem o Comandante.
    let banco = Persistence::open(&Location::File(arquivo.clone()))?;
    assert!(!seele_server::cargos::pode(
        &banco,
        id_pessoa,
        Permission::ManageRoles
    ));

    servidor.shutdown();
    Ok(())
}

/// **Sem laço de leituras** (pedido do G): a sessão que só escuta fica sabendo
/// da mudança de cargo pelo `Event::CargosChanged`, com a marca do retrato —
/// sem chamar `pedir_cargos` nem mandar nada ao servidor.
#[tokio::test(flavor = "multi_thread")]
async fn a_mudanca_de_cargo_chega_por_evento_a_quem_so_escuta() -> Result<()> {
    #[derive(Default)]
    struct Marcas(std::sync::Mutex<Vec<u64>>);
    impl seele_ffi::EventListener for Marcas {
        fn on_event(&self, event: seele_ffi::Event) {
            if let seele_ffi::Event::CargosChanged { marca } = event {
                self.0.lock().unwrap().push(marca);
            }
        }
    }

    let _vaga = vaga::minha();
    let pasta = tempfile::tempdir()?;
    for nome in ["dona-evento", "ouvinte"] {
        let _ = nascer(nome);
    }
    let (endereco, servidor) = subir(
        &pasta.path().join("e.db"),
        Some(impressao_de("dona-evento")),
    )
    .await?;
    let dona = voltar(endereco, "dona-evento").await?;
    let ouvinte = voltar(endereco, "ouvinte").await?;
    for conexao in [&dona, &ouvinte] {
        assert!(ate(conexao, |c| quadro(c).is_some()));
    }
    let marcas = Arc::new(Marcas::default());
    ouvinte.subscribe(Arc::clone(&marcas) as Arc<dyn seele_ffi::EventListener>);
    let antes = ouvinte.snapshot().cargos_revision;

    dona.pedido_de_cargo(
        revisao(&dona),
        PedidoDeCargo::Criar {
            nome: "Avisado".into(),
            concede: vec![],
            restringe: vec![],
        },
    )?;
    assert!(
        ate(&ouvinte, |_| !marcas.0.lock().unwrap().is_empty()),
        "quem só escuta não recebeu CargosChanged"
    );
    let ultima = *marcas.0.lock().unwrap().last().unwrap();
    assert!(
        ultima > antes,
        "a marca do evento não andou: {antes} → {ultima}"
    );
    assert!(ate(&ouvinte, |c| c.snapshot().cargos_revision == ultima));
    assert!(
        cargo_chamado(&ouvinte, "Avisado").is_some(),
        "o evento chegou sem o quadro novo"
    );

    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn dois_servidores_e_duas_sessoes_nao_se_misturam() -> Result<()> {
    let _vaga = vaga::minha();
    let pasta = tempfile::tempdir()?;
    for nome in ["dona-a", "mesma"] {
        let _ = nascer(nome);
    }
    let (endereco_a, servidor_a) =
        subir(&pasta.path().join("a.db"), Some(impressao_de("dona-a"))).await?;
    let (endereco_b, servidor_b) = subir(&pasta.path().join("b.db"), None).await?;

    let dona = voltar(endereco_a, "dona-a").await?;
    // A mesma pessoa em duas sessões, uma em cada servidor.
    let em_a = voltar(endereco_a, "mesma").await?;
    let em_b = voltar(endereco_b, "mesma").await?;
    for conexao in [&dona, &em_a, &em_b] {
        assert!(ate(conexao, |c| quadro(c).is_some()));
    }
    assert!(
        !quadro(&em_b).unwrap().dono_declarado,
        "o servidor B não declarou dono e diz que tem"
    );
    assert!(quadro(&em_a).unwrap().dono_declarado);

    dona.pedido_de_cargo(
        revisao(&dona),
        PedidoDeCargo::Criar {
            nome: "Só no A".into(),
            concede: vec![],
            restringe: vec![],
        },
    )?;
    assert!(
        ate(&em_a, |c| cargo_chamado(c, "Só no A").is_some()),
        "a sessão no A não recebeu o quadro do A"
    );
    // A sessão no B nunca vê o cargo do A, e a revisão do B não se mexe.
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        cargo_chamado(&em_b, "Só no A").is_none(),
        "o quadro do A vazou para a sessão do B"
    );
    assert_eq!(revisao(&em_b), 0);
    // E a dona do A não é dona de nada no B: lá ela nem declarada é.
    assert!(!em_b.cargos_na_tela().suas.unwrap().proprietario);

    servidor_a.shutdown();
    servidor_b.shutdown();
    Ok(())
}
