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
//!
//! A máquina serve **depois do aceite** (`maquina_de_jogo`): a metade de
//! servidor pede, o núcleo mostra, e a máquina aceita a oferta do `eco-tcp`.

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
use seele_proto::tunel::ProtocoloDeTunel;
use seele_server::Daemon;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

mod maquina_de_jogo;
use maquina_de_jogo::{aceitar_como_maquina, MOD, SERVIDOR_DE_JOGO};

/// Quanto se espera por qualquer coisa antes de dar o teste por falho. Larga
/// porque o ping do canal de controle é de 2 s e uma máquina carregada é lenta.
const PACIENCIA: Duration = Duration::from_secs(30);

use maquina_de_jogo::Servidor;

/// Um servidor com o MOD `x/jogo` ligado, com a metade de servidor que pede a
/// máquina (ver `maquina_de_jogo::servidor`).
async fn servidor(malha_para_tuneis: bool) -> Result<Servidor> {
    maquina_de_jogo::servidor(malha_para_tuneis, None).await
}

// ------------------------------------------------------------ a máquina

/// A máquina do jogo: um `Enlace` que aceita ser a máquina e serve um eco TCP
/// como `eco-tcp`.
async fn maquina(daemon: &Daemon, endereco: SocketAddr, conjunto: &str) -> Result<Enlace> {
    let mut enlace = Enlace::conectar_entre(
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
    aceitar_como_maquina(
        daemon,
        &mut enlace,
        MOD,
        SERVIDOR_DE_JOGO,
        &[("eco-tcp", ProtocoloDeTunel::Tcp, jogo)],
    )
    .await?;
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
    let maquina = maquina(&daemon, endereco, &conjunto).await?;
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
    // As conexões vivas se dividem entre os dois caminhos, sem sobra.
    assert_eq!(
        medido.conexoes,
        medido.conexoes_diretas + medido.conexoes_pela_estrela,
        "{medido:?}"
    );

    // **Um segundo JOGAR com a porta aberta devolve a mesma porta e não mexe na
    // conexão do jogo.** Antes, o segundo pedido refazia o túnel: a porta velha
    // fechava (e levava a conexão viva) e a nova podia cair numa «ocupada».
    let mut viva = tokio::net::TcpStream::connect(("127.0.0.1", aberta.porta)).await?;
    viva.write_all(b"antes").await?;
    let mut ouvido = [0_u8; 5];
    tokio::time::timeout(Duration::from_secs(10), viva.read_exact(&mut ouvido)).await??;
    assert_eq!(&ouvido, b"antes");
    let de_novo = conexao
        .entrar_no_tunel(MOD.into(), "eco-tcp".into())
        .await
        .map_err(|r| anyhow::anyhow!("entrar de novo: {r}"))?;
    assert_eq!(
        (de_novo.porta, de_novo.preferida, de_novo.desejada),
        (aberta.porta, aberta.preferida, aberta.desejada),
        "o segundo JOGAR trouxe outra porta"
    );
    viva.write_all(b"depois").await?;
    let mut ouvido = [0_u8; 6];
    tokio::time::timeout(Duration::from_secs(10), viva.read_exact(&mut ouvido))
        .await
        .expect("a conexão do jogo morreu com o segundo JOGAR")?;
    assert_eq!(&ouvido, b"depois");
    conexao.disconnect();
    drop(maquina);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn fechar_os_tuneis_do_mod_fecha_a_porta_e_diz_o_motivo() -> Result<()> {
    let (endereco, daemon, conjunto, temp) = servidor(true).await?;
    let maquina = maquina(&daemon, endereco, &conjunto).await?;
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
    let maquina = maquina(&daemon, endereco, &conjunto).await?;
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
    let maquina = maquina(&daemon, endereco, &conjunto).await?;
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
