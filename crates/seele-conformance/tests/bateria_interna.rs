//! A bateria interna, ponta a ponta.
//!
//! `specs/07-tema-evangelion.md` chama isto de "o melhor casamento entre tema e
//! engenharia no projeto" e manda proteger de simplificações. Estava protegido
//! de simplificações e desprotegido de outra coisa: as peças existiam, cada uma
//! testada, e **nada ligava uma na outra**. `Battery::new` não era chamado fora
//! do próprio módulo; ao cair, o cliente ia direto para "ENLACE PERDIDO".
//!
//! Testes de unidade não pegam isso — cada unidade passa. Este teste derruba o
//! servidor de verdade, confere que a sessão entra na bateria em vez de acabar,
//! sobe o servidor de novo na mesma porta, e confere que ela volta sozinha.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use seele_core::enlace::{Aviso, Destino, Enlace, Motivo};
use seele_core::{Link, MemoryPinStore};
use seele_proto::ids::{ChannelId, ClientMessageId, VoiceRoomId};
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

const VOICE_ROOM: u32 = 1;
const LINE: u32 = 1;

/// Sobe um servidor numa porta escolhida — a mesma na segunda vez.
async fn server(porta: u16, banco: Location) -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Terceira Tóquio".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], porta)),
        database: banco,
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

fn destino(endereco: SocketAddr) -> Destino {
    Destino {
        servidor: endereco,
        nome_tls: "localhost".into(),
        chave_do_pin: endereco.to_string(),
        apelido: "ayanami".into(),
        segredo: None,
        impressao_esperada: None,
    }
}

/// Espera um aviso que interesse, ou desiste.
async fn esperar<F>(enlace: &mut Enlace, prazo: Duration, mut aceita: F) -> Option<Aviso>
where
    F: FnMut(&Aviso) -> bool,
{
    let fim = tokio::time::Instant::now() + prazo;
    while tokio::time::Instant::now() < fim {
        match tokio::time::timeout(Duration::from_millis(300), enlace.proximo()).await {
            Ok(aviso) if aceita(&aviso) => return Some(aviso),
            Ok(_) => {}
            Err(_) => {}
        }
    }
    None
}

#[tokio::test(flavor = "multi_thread")]
async fn o_server_cai_e_a_sessao_entra_na_bateria_em_vez_de_acabar() -> Result<()> {
    // O banco em arquivo, e não em memória: o servidor que sobe depois tem que ser
    // o mesmo servidor, com o mesmo certificado. Um certificado novo pareceria uma
    // troca de chave ao cliente, que é o alerta do ADR 0003 e não uma
    // reconexão.
    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");

    let (endereco, servidor) = server(0, Location::File(banco.clone())).await?;
    let porta = endereco.port();

    let mut enlace = Enlace::conectar(
        destino(endereco),
        ed25519_dalek::SigningKey::from_bytes(&[42; 32]),
        Arc::new(MemoryPinStore::new()),
    )
    .await?;
    enlace.inserir_plug(VoiceRoomId(VOICE_ROOM)).await?;
    enlace.abrir_linha(ChannelId(LINE)).await?;
    assert_eq!(enlace.estado(), Link::Online);

    // ---- o servidor cai
    servidor.shutdown();
    servidor.wait_idle().await;
    drop(servidor);
    tokio::time::sleep(Duration::from_millis(300)).await;

    let aviso = esperar(&mut enlace, Duration::from_secs(20), |aviso| {
        matches!(
            aviso,
            Aviso::Estado {
                estado: Link::InternalBattery { .. },
                ..
            }
        )
    })
    .await;

    let Some(Aviso::Estado { restante, .. }) = aviso else {
        panic!("a sessão não entrou na bateria interna — foi isto que ficou anos sem existir");
    };
    assert!(
        matches!(enlace.estado(), Link::InternalBattery { .. }),
        "o enlace não reporta a bateria"
    );
    let restante = restante.expect("sem contagem regressiva");
    assert!(
        restante > Duration::from_secs(4 * 60),
        "a contagem não começou perto de cinco minutos: {restante:?}"
    );

    // ---- o servidor volta, no mesmo endereço e com o mesmo banco
    let (_, de_novo) = server(porta, Location::File(banco)).await?;

    let voltou = esperar(&mut enlace, Duration::from_secs(30), |aviso| {
        matches!(aviso, Aviso::Reconectado { .. })
    })
    .await;
    assert!(
        voltou.is_some(),
        "a sessão não voltou sozinha depois que o servidor subiu"
    );
    assert_eq!(enlace.estado(), Link::Online);

    // E continua servindo: reconectar sem poder falar seria voltar para nada.
    enlace
        .dizer(ChannelId(LINE), "voltei".to_owned(), ClientMessageId(1))
        .await?;

    de_novo.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn o_que_a_pessoa_escolheu_volta_com_ela() -> Result<()> {
    // Reconectar no lugar errado é quase tão ruim quanto não reconectar: quem
    // estava num sala de voz conversando volta calado noutro canto sem entender por
    // quê.
    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");
    let (endereco, servidor) = server(0, Location::File(banco.clone())).await?;
    let porta = endereco.port();

    let mut enlace = Enlace::conectar(
        destino(endereco),
        ed25519_dalek::SigningKey::from_bytes(&[43; 32]),
        Arc::new(MemoryPinStore::new()),
    )
    .await?;
    enlace.inserir_plug(VoiceRoomId(VOICE_ROOM)).await?;
    enlace.abrir_linha(ChannelId(LINE)).await?;
    enlace.muted(true).await?;

    servidor.shutdown();
    servidor.wait_idle().await;
    drop(servidor);
    tokio::time::sleep(Duration::from_millis(300)).await;

    let (_, de_novo) = server(porta, Location::File(banco)).await?;

    let voltou = esperar(&mut enlace, Duration::from_secs(30), |aviso| {
        matches!(aviso, Aviso::Reconectado { .. })
    })
    .await;
    assert!(voltou.is_some(), "não reconectou");

    // O `ssrc` é novo — é por conexão (falha G1) — e é justamente por isso que
    // a casca precisa reabrir a voz com o canal que veio no aviso.
    assert_ne!(
        enlace.sessao().ssrc.0,
        0,
        "a sessão nova não trouxe um ssrc"
    );

    // Falar de novo prova que a sala de voz e a Linha foram refeitos: sem `join_channel`
    // o servidor aceita a mensagem e não a devolve para ninguém.
    enlace
        .dizer(ChannelId(LINE), "de volta".to_owned(), ClientMessageId(1))
        .await?;

    let ouviu = esperar(&mut enlace, Duration::from_secs(10), |aviso| {
        matches!(
            aviso,
            Aviso::Mensagem(mensagem)
                if matches!(&**mensagem, seele_proto::control::ServerMessage::MessageReceived { body, .. } if body == "de volta")
        )
    })
    .await;
    assert!(
        ouviu.is_some(),
        "a Linha não foi reaberta na reconexão: a mensagem não voltou"
    );

    de_novo.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn sair_encerra_sem_esperar_a_bateria() -> Result<()> {
    let (endereco, servidor) = server(0, Location::Memory).await?;
    let mut enlace = Enlace::conectar(
        destino(endereco),
        ed25519_dalek::SigningKey::from_bytes(&[44; 32]),
        Arc::new(MemoryPinStore::new()),
    )
    .await?;

    enlace.sair().await;

    let fim = esperar(&mut enlace, Duration::from_secs(5), |aviso| {
        matches!(aviso, Aviso::Encerrado(_))
    })
    .await;
    assert!(
        matches!(fim, Some(Aviso::Encerrado(Motivo::Pedido))),
        "sair deveria encerrar na hora, e não abrir uma contagem de cinco minutos"
    );

    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn as_tentativas_aparecem_enquanto_a_bateria_corre() -> Result<()> {
    // `specs/07-tema-evangelion.md` pede "tentativas de reconexão listadas". Um
    // contador que fica em zero enquanto o programa tenta é pior que não ter:
    // parece que ninguém está fazendo nada.
    let (endereco, servidor) = server(0, Location::Memory).await?;
    let mut enlace = Enlace::conectar(
        destino(endereco),
        ed25519_dalek::SigningKey::from_bytes(&[45; 32]),
        Arc::new(MemoryPinStore::new()),
    )
    .await?;

    servidor.shutdown();
    servidor.wait_idle().await;
    drop(servidor);

    // O prazo cobre várias tentativas: cada uma esbarra no tempo de handshake
    // antes de desistir, e o backoff cresce entre elas.
    let subiu = esperar(&mut enlace, Duration::from_secs(45), |aviso| {
        matches!(
            aviso,
            Aviso::Estado {
                estado: Link::InternalBattery { attempts },
                ..
            } if *attempts > 0
        )
    })
    .await;

    assert!(
        subiu.is_some(),
        "o contador de tentativas ficou em zero durante toda a bateria"
    );
    Ok(())
}
