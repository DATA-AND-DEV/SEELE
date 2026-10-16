//! **A volta que aceita os MODs, pela FFI: com a credencial da tentativa ela
//! entra, sem ela é recusada.** Lote R da F5.
//!
//! # O caso
//!
//! Um servidor fechado por convite, com a portaria desligada e um MOD exigido.
//! A primeira batida para na pergunta dos MODs; a pessoa aceita, e o app entra
//! de novo. Essa volta saía sem o convite — `aceitarOsMods` terminava num
//! `conectar()` sem argumento —, e o servidor respondia `CredentialRejected`.
//!
//! O lado do servidor a F1 consertou em `d0b98b9c`: recusar os MODs deixa o
//! convite intacto, e ele é gasto só quando a pessoa de fato entra. O lado do
//! app o lote R consertou em `camada-mods.js`: a pergunta guarda a credencial
//! da tentativa enquanto está aberta, e a devolve à volta que aceita.
//!
//! # O que este arquivo prova, e o que não prova
//!
//! Ele prova **o contrato de que o conserto do app depende**, pelo mesmo
//! caminho que o app usa — `seele_ffi::Connection::connect`, o aceite gravado
//! por `seele_ffi::mods::aceitar`, um servidor de verdade:
//!
//! - a volta **sem** a credencial é recusada. É o defeito, medido pela FFI: sem
//!   isto, o reenvio do app seria um enfeite;
//! - a volta **com** a credencial entra. É o que o reenvio compra, e depende do
//!   `d0b98b9c`: sem ele, o convite já teria sido gasto pela batida que parou
//!   na pergunta.
//!
//! As duas credenciais que viajam no `join_secret` têm um teste cada: o convite
//! de uso único, que é o caso do relato, e a senha do servidor, que não é
//! gasta e prova que o caminho da volta é o mesmo para as duas.
//!
//! Ele **não** prova o app: quem confere que o segundo `connect` leva a
//! credencial é a cena do lote R em `apps/seele-app/bancada/ajustes-v013.cjs`,
//! e quem confere que ela só vai ao `connect` é o guarda
//! `a_credencial_da_tentativa_so_vai_ao_connect_e_morre_com_a_pergunta`, em
//! `apps/seele-app/tests/frontend.rs`.
//!
//! As credenciais daqui são geradas pelo servidor de teste (o convite) ou
//! escritas aqui (a senha), e não são de servidor nenhum.

#![allow(clippy::expect_used, reason = "num teste, o pânico é o relatório")]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use seele_ffi::{ConnectConfig, Connection, ConnectionError, EndReason};
use seele_server::persistence::mods::{enable, EnabledMod};
use seele_server::persistence::Location;
use seele_server::{admissao, portaria, Daemon, ServerConfig};

mod vaga;

/// A senha do servidor do segundo teste: só este arquivo a conhece.
const SENHA: &str = "senha-da-volta-do-aceite";

/// Quanto cada `connect` tem para responder.
const PRAZO: Duration = Duration::from_secs(15);

async fn servidor() -> Result<(SocketAddr, Arc<Daemon>)> {
    let daemon = Arc::new(
        Daemon::bind(ServerConfig {
            name: "Casa".into(),
            listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            database: Location::Memory,
            ..ServerConfig::default()
        })
        .await?,
    );
    let endereco = daemon.local_addr()?;
    let servindo = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = servindo.run().await;
    });
    Ok((endereco, daemon))
}

/// O MOD exigido. O que interessa é haver um: o portão dos MODs fica depois do
/// segredo, e é a recusa dele que faz a batida voltar.
fn mod_exigido() -> EnabledMod {
    EnabledMod {
        id: "fulano/exemplo".into(),
        version: "1.0.0".into(),
        hash: "ab".repeat(32),
        repo: "https://github.com/seele/exemplo".into(),
        reach: vec!["ler".into()],
        server_half: false,
        api: 0,
        tema: false,
    }
}

/// Um `connect` da FFI, como o app o faz: a mesma casa a cada chamada (a mesma
/// chave e o mesmo aceite gravado), sem áudio, e o segredo que esta tentativa
/// leva.
async fn conectar(
    casa: &std::path::Path,
    alvo: &str,
    segredo: Option<&str>,
) -> Result<Arc<Connection>, ConnectionError> {
    let config = ConnectConfig {
        server: alvo.to_owned(),
        alternate_servers: Vec::new(),
        nickname: "bia".into(),
        home: casa.display().to_string(),
        join_secret: segredo.map(str::to_owned),
        expected_fingerprint: None,
        codigo_esperado: None,
        bilhete: None,
        audio: false,
        capture_device: None,
        playback_device: None,
    };
    tokio::time::timeout(
        PRAZO,
        tokio::task::spawn_blocking(move || Connection::connect(config)),
    )
    .await
    .expect("a FFI não respondeu dentro do prazo")
    .expect("a thread que conecta caiu")
    .map(|(conexao, _)| conexao)
}

/// A batida, o sim, a volta sem o segredo e a volta com ele.
///
/// Cada passo diz o que quebra quando não acontece: este é o caminho inteiro
/// que o app faz, e um passo que dá outra resposta muda o que a tela mostra.
async fn bater_aceitar_e_voltar(alvo: &str, segredo: &str, qual: &str) {
    let casa = tempfile::tempdir().expect("uma casa para a identidade");

    // 1. A batida com o segredo para na pergunta dos MODs.
    let conjunto = match conectar(casa.path(), alvo, Some(segredo)).await {
        Err(ConnectionError::ModsNaoAceitos { conjunto, .. }) => conjunto,
        Err(outro) => panic!("a batida com {qual} não parou na pergunta dos MODs: {outro:?}"),
        Ok(_) => panic!("a batida com {qual} entrou sem aceitar o MOD exigido"),
    };

    // 2. O sim, gravado como a casca o grava (`aceitar_mods`, no `main.rs`).
    seele_ffi::mods::aceitar(&casa.path().display().to_string(), alvo, &conjunto)
        .expect("gravar o sim");

    // 3. A volta sem o segredo — o que o app fazia antes do lote R.
    match conectar(casa.path(), alvo, None).await {
        Err(ConnectionError::Refused {
            reason: EndReason::CredentialRejected,
        }) => {}
        Err(outro) => panic!(
            "a volta sem {qual} não foi recusada como credencial: {outro:?}. O reenvio \
             do app foi escrito para esta recusa, e ela mudou de nome"
        ),
        Ok(_) => panic!(
            "a volta sem {qual} entrou num servidor fechado sem portaria: ou o servidor \
             passou a lembrar de quem só faltava aceitar os MODs, ou ele está aberto e \
             este teste não mede nada"
        ),
    }

    // 4. A volta com o segredo entra.
    let conexao = conectar(casa.path(), alvo, Some(segredo))
        .await
        .unwrap_or_else(|erro| {
            panic!(
                "a volta que aceita os MODs, com {qual}, não entrou: {erro:?}. O app \
                 reenvia a credencial da tentativa justamente para isto; se {qual} foi \
                 gasto pela batida que parou na pergunta, o d0b98b9c da F1 regrediu"
            )
        });
    conexao.disconnect();
}

/// **O caso do relato:** convite de uso único, portaria desligada, um MOD.
#[tokio::test(flavor = "multi_thread")]
async fn a_volta_que_aceita_os_mods_entra_com_o_convite_e_e_recusada_sem_ele() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;
    let convite = {
        let mut banco = daemon.server().persistence.lock().await;
        enable(&banco, &mod_exigido())?;
        portaria::ligar(&mut banco, false)?;
        admissao::criar_convite(&mut banco, "para a volta do aceite")?
    };

    bater_aceitar_e_voltar(&endereco.to_string(), &convite, "o convite").await;

    daemon.shutdown();
    Ok(())
}

/// **A outra metade do `join_secret`:** a senha do servidor, que não é gasta.
#[tokio::test(flavor = "multi_thread")]
async fn a_volta_que_aceita_os_mods_entra_com_a_senha_e_e_recusada_sem_ela() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;
    {
        let mut banco = daemon.server().persistence.lock().await;
        enable(&banco, &mod_exigido())?;
        portaria::ligar(&mut banco, false)?;
        admissao::definir_senha(&mut banco, Some(SENHA))?;
    }

    bater_aceitar_e_voltar(&endereco.to_string(), SENHA, "a senha").await;

    daemon.shutdown();
    Ok(())
}
