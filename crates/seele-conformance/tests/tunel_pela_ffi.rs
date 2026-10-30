//! **O túnel de jogo pela FFI** (API 8, parte 2, Task 9): quem joga é a
//! `seele_ffi::Connection` de verdade — o mesmo objeto que os comandos Tauri
//! `tunel_entrar`, `tunel_sair_do_mod` e `tuneis_abertos` seguram —, a máquina é
//! um `seele_core::enlace::Enlace` que serve um eco TCP em `127.0.0.1`, e o
//! servidor é o de verdade, com o MOD `x/jogo` (poder `porta`) ligado.
//!
//! O que cada teste prova, pela FFI e não pelo núcleo (o núcleo já tem a
//! prova dele em `tunel_de_jogo.rs`):
//!
//! - `a_porta_abre_pela_ffi_e_o_relatorio_chega_a_casca_com_o_apelido`: o eco
//!   passa pela porta que `Connection::entrar_no_tunel` devolve; o relatório
//!   chega como `Event::TunnelReport`, no caminho direto com o apelido da
//!   máquina (a frase «ligação direta com <apelido>» da faixa); e
//!   `tuneis_abertos` o lista;
//! - `fechar_os_tuneis_do_mod_fecha_a_porta_e_diz_o_motivo`: o descarregar de um
//!   MOD fecha a porta, e o último relatório leva o motivo (`mod-desligado`);
//! - `encerrar_a_sessao_fecha_a_porta`: é o que o `ExitRequested` do app faz
//!   (`despedir_da_sessao` → `encerrar_e_esperar`): a porta não sobrevive ao
//!   app;
//! - `a_recusa_chega_com_o_nome`: o nome em kebab-case, e nunca texto livre.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_core::enlace::{Destino, Enlace};
use seele_core::{MemoryPinStore, PinStore};
use seele_ffi::{
    CaminhoDeTunel, ConnectConfig, Connection, Event, EventListener, RelatorioDeTunel,
};
use seele_proto::tunel::{ProtocoloDeTunel, ACAO_ENTRAR_NO_JOGO};
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

mod porta;
use porta::ligar_insistindo;

const MOD: &str = "x/jogo";

/// Quanto se espera por qualquer coisa antes de dar o teste por falho. Larga
/// porque o ping do canal de controle é de 2 s e uma máquina carregada é lenta.
const PACIENCIA: Duration = Duration::from_secs(30);

/// O servidor de teste: onde ele escuta, o daemon, a identidade do conjunto de
/// MODs que quem entra tem de ter aceitado, e o diretório que o sustenta.
type Servidor = (SocketAddr, Arc<Daemon>, String, tempfile::TempDir);

/// Um servidor com o MOD `x/jogo` ligado: poder `porta`, plataforma `app` e a
/// ação `entrar-no-jogo` dada a todos. A mesma montagem de
/// `seele-server/tests/tunel_no_servidor.rs`.
async fn servidor(malha_para_tuneis: bool) -> Result<Servidor> {
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
        "permissoes": [{"acao": ACAO_ENTRAR_NO_JOGO, "nome": "Entrar no jogo", "todos": true}],
    })
    .to_string()
    .into_bytes();
    let mut arquivos = vec![
        ("mod.json".to_owned(), manifesto),
        ("cliente.js".to_owned(), b"void 0;".to_vec()),
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
        mods_dir: Some(raizes.clone()),
        malha_para_tuneis,
        servir_tuneis: true,
        ..ServerConfig::default()
    };
    let daemon = Arc::new(ligar_insistindo(config).await?);
    let conjunto = {
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
        seele_server::mods::anuncio::conjunto_exigido(&db)?.identidade
    };
    let endereco = daemon.local_addr()?;
    let atendendo = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = atendendo.run().await;
    });
    Ok((endereco, daemon, conjunto, temp))
}

// ------------------------------------------------------------ a máquina

/// A máquina do jogo: um `Enlace` que serve um eco TCP como `eco-tcp`.
async fn maquina(endereco: SocketAddr, conjunto: &str) -> Result<Enlace> {
    let enlace = Enlace::conectar_entre(
        vec![Destino {
            servidor: endereco,
            nome_tls: "localhost".into(),
            chave_do_pin: endereco.to_string(),
            apelido: "maquina".to_owned(),
            segredo: None,
            esperada: None,
            o_pino_prova_o_servidor: true,
            aceito: Some(conjunto.to_owned()),
        }],
        SigningKey::from_bytes(&[1; 32]),
        Arc::new(MemoryPinStore::default()) as Arc<dyn PinStore>,
    )
    .await?;
    let escuta = TcpListener::bind("127.0.0.1:0").await?;
    let jogo = escuta.local_addr()?.port();
    tokio::spawn(async move {
        loop {
            let Ok((mut fluxo, _)) = escuta.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut buf = vec![0_u8; 16 * 1024];
                while let Ok(n) = fluxo.read(&mut buf).await {
                    if n == 0 || fluxo.write_all(&buf[..n]).await.is_err() {
                        return;
                    }
                }
            });
        }
    });
    enlace
        .servir_tunel(MOD, "eco-tcp", ProtocoloDeTunel::Tcp, jogo)
        .await
        .map_err(|r| anyhow::anyhow!("servir eco-tcp: {r:?}"))?;
    Ok(enlace)
}

// ------------------------------------------------------------ quem joga

/// O que a casca ouviria: todo relatório de túnel, em ordem.
#[derive(Default)]
struct Relatorios(Mutex<Vec<RelatorioDeTunel>>);

impl EventListener for Relatorios {
    fn on_event(&self, event: Event) {
        if let Event::TunnelReport { report } = event {
            self.0.lock().unwrap().push(report);
        }
    }
}

impl Relatorios {
    /// Espera um relatório que `serve` aprove, dos que já chegaram ou dos que
    /// vierem.
    fn esperar(&self, o_que: &str, serve: impl Fn(&RelatorioDeTunel) -> bool) -> RelatorioDeTunel {
        let fim = Instant::now() + PACIENCIA;
        loop {
            if let Some(r) = self.0.lock().unwrap().iter().find(|r| serve(r)) {
                return r.clone();
            }
            assert!(
                Instant::now() < fim,
                "a paciência acabou esperando: {o_que}; relatórios vistos: {:#?}",
                self.0.lock().unwrap()
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

fn jogador(
    endereco: SocketAddr,
    home: &Path,
    conjunto: &str,
) -> Result<(Arc<Connection>, Arc<Relatorios>)> {
    std::fs::create_dir_all(home)?;
    // O sim aos MODs deste servidor, como a portaria o grava.
    let alvo = seele_ffi::chave_do_servidor(&endereco.to_string()).expect("o alvo");
    seele_core::aceites::Aceites::em(home).aceitar_agora(&alvo, conjunto)?;
    let (conexao, _) = Connection::connect(ConnectConfig {
        server: endereco.to_string(),
        alternate_servers: Vec::new(),
        nickname: "joga".to_owned(),
        home: home.to_string_lossy().into_owned(),
        join_secret: None,
        expected_fingerprint: None,
        codigo_esperado: None,
        bilhete: None,
        audio: false,
        capture_device: None,
        playback_device: None,
    })?;
    let ouve = Arc::new(Relatorios::default());
    conexao.subscribe(Arc::clone(&ouve) as Arc<dyn EventListener>);
    Ok((conexao, ouve))
}

/// Manda `ola` pela porta e confere o eco.
async fn ecoa(porta: u16) -> Result<()> {
    let mut aberta = TcpStream::connect(("127.0.0.1", porta)).await?;
    aberta.write_all(b"ola").await?;
    let mut volta = [0_u8; 3];
    tokio::time::timeout(PACIENCIA, aberta.read_exact(&mut volta)).await??;
    assert_eq!(&volta, b"ola");
    Ok(())
}

async fn a_porta_fecha(porta: u16) -> Result<()> {
    let fim = Instant::now() + PACIENCIA;
    while Instant::now() < fim {
        if TcpStream::connect(("127.0.0.1", porta)).await.is_err() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    anyhow::bail!("a porta {porta} continuou aceitando conexões")
}

/// Espera o servidor ter a identidade da máquina e de quem joga: sem isto o
/// pedido pode ir pela estrela sem nada estar errado, e o teste do caminho
/// direto reprovaria de vez em quando.
async fn esperar_as_identidades(daemon: &Daemon, maquina: &Enlace) -> Result<()> {
    let fim = Instant::now() + PACIENCIA;
    while Instant::now() < fim {
        let pronto = {
            let pares = daemon.server().pares.lock().await;
            // Quem joga é a outra pessoa presente além da máquina.
            let presentes = daemon.server().presentes.lock().await.todos();
            presentes.iter().any(|p| {
                p.person != maquina.sessao().person
                    && pares
                        .apresentar(maquina.sessao().person, p.person)
                        .is_some()
            })
        };
        if pronto {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    anyhow::bail!("o servidor não ficou sabendo da identidade das duas pontas")
}

// ------------------------------------------------------------ os testes

#[tokio::test(flavor = "multi_thread")]
async fn a_porta_abre_pela_ffi_e_o_relatorio_chega_a_casca_com_o_apelido() -> Result<()> {
    let (endereco, daemon, conjunto, temp) = servidor(true).await?;
    let maquina = maquina(endereco, &conjunto).await?;
    let home = temp.path().join("joga");
    let (conexao, relatorios) = jogador(endereco, &home, &conjunto)?;
    esperar_as_identidades(&daemon, &maquina).await?;

    let aberta = conexao
        .entrar_no_tunel(MOD.into(), "eco-tcp".into())
        .await
        .map_err(|r| anyhow::anyhow!("entrar eco-tcp: {r}"))?;
    assert!(aberta.preferida || aberta.desejada != 0, "{aberta:?}");
    ecoa(aberta.porta).await?;

    // O que está de pé já está na lista da faixa, antes de qualquer relatório.
    let abertos = conexao.tuneis_abertos();
    assert_eq!(abertos.len(), 1, "{abertos:?}");
    assert_eq!(abertos[0].nome, "eco-tcp");
    assert_eq!(abertos[0].porta, aberta.porta);

    // O relatório do caminho direto chega com o APELIDO da máquina: é o que a
    // faixa diz em «ligação direta com <apelido>».
    let direto = relatorios.esperar("o caminho direto com o apelido", |r| {
        r.nome == "eco-tcp"
            && matches!(&r.caminho, CaminhoDeTunel::Direto { apelido: Some(a), .. } if a == "maquina")
    });
    assert_eq!(direto.mod_id, MOD);
    assert_eq!(direto.porta, aberta.porta);
    // E a latência e a vazão chegam em números que a faixa mostra.
    let medido = relatorios.esperar("latência e vazão", |r| {
        r.nome == "eco-tcp" && r.latencia_ms.is_some() && r.bytes_por_segundo > 0
    });
    assert!(medido.bytes_por_segundo >= medido.bytes_por_segundo_ida);
    conexao.disconnect();
    drop(maquina);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn fechar_os_tuneis_do_mod_fecha_a_porta_e_diz_o_motivo() -> Result<()> {
    let (endereco, daemon, conjunto, temp) = servidor(true).await?;
    let maquina = maquina(endereco, &conjunto).await?;
    let (conexao, relatorios) = jogador(endereco, &temp.path().join("joga"), &conjunto)?;
    esperar_as_identidades(&daemon, &maquina).await?;

    let porta = conexao
        .entrar_no_tunel(MOD.into(), "eco-tcp".into())
        .await
        .map_err(|r| anyhow::anyhow!("entrar: {r}"))?
        .porta;
    ecoa(porta).await?;

    // O que o descarregar do MOD faz (`tunel_sair_do_mod`).
    conexao.sair_dos_tuneis_do_mod(MOD.into())?;
    assert!(
        conexao.tuneis_abertos().is_empty(),
        "a lista ainda tem o túnel que fechou"
    );
    a_porta_fecha(porta).await?;
    // O último relatório leva o motivo, em kebab-case: a faixa o diz em frase.
    // Quem fecha é quem joga (o MOD saiu da janela dele), e é `fechado`;
    // `mod-desligado` é o do servidor que desligou o MOD.
    let ultimo = relatorios.esperar("o relatório final", |r| {
        r.nome == "eco-tcp" && r.encerrado.is_some()
    });
    assert_eq!(ultimo.encerrado, Some("fechado"), "{ultimo:?}");
    conexao.disconnect();
    drop(maquina);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn encerrar_a_sessao_fecha_a_porta() -> Result<()> {
    let (endereco, daemon, conjunto, temp) = servidor(true).await?;
    let maquina = maquina(endereco, &conjunto).await?;
    let (conexao, _) = jogador(endereco, &temp.path().join("joga"), &conjunto)?;
    esperar_as_identidades(&daemon, &maquina).await?;

    let porta = conexao
        .entrar_no_tunel(MOD.into(), "eco-tcp".into())
        .await
        .map_err(|r| anyhow::anyhow!("entrar: {r}"))?
        .porta;
    ecoa(porta).await?;

    // A saída do app: `ExitRequested` → `despedir_da_sessao` →
    // `encerrar_e_esperar`. A porta de jogo não sobrevive ao app.
    let acabou = tokio::task::spawn_blocking({
        let conexao = Arc::clone(&conexao);
        move || conexao.encerrar_e_esperar(Duration::from_secs(10))
    })
    .await?;
    assert!(acabou, "a sessão não terminou dentro do teto");
    a_porta_fecha(porta).await?;
    drop(maquina);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_recusa_chega_com_o_nome() -> Result<()> {
    let (endereco, daemon, conjunto, temp) = servidor(true).await?;
    let maquina = maquina(endereco, &conjunto).await?;
    let (conexao, _) = jogador(endereco, &temp.path().join("joga"), &conjunto)?;
    esperar_as_identidades(&daemon, &maquina).await?;

    // Um nome que nenhuma máquina serve.
    let recusa = conexao
        .entrar_no_tunel(MOD.into(), "nao-existe-esse".into())
        .await
        .expect_err("ninguém serve esse nome");
    assert_eq!(recusa, "nao-existe");
    // Um MOD que o servidor não tem ligado.
    let recusa = conexao
        .entrar_no_tunel("x/desligado".into(), "eco-tcp".into())
        .await
        .expect_err("o MOD não está ligado");
    assert!(
        ["mod-sem-porta", "nao-existe"].contains(&recusa),
        "{recusa}"
    );
    // Depois de desconectar, o nome é o da sessão.
    conexao.disconnect();
    let recusa = conexao
        .entrar_no_tunel(MOD.into(), "eco-tcp".into())
        .await
        .expect_err("a sessão acabou");
    assert_eq!(recusa, "sessao-encerrada");
    drop(maquina);
    Ok(())
}
