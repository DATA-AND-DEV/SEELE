//! **A câmera pelo servidor de verdade** (1.0, protocolo 13): pares crus sobre
//! o enlace, do `StartScreenShare` ao fluxo que chega a quem assiste.
//!
//! A câmera vai pelo caminho da tela — o mesmo pedido, o mesmo fluxo —, e o que
//! a distingue é um byte na abertura: a fonte 2. Três coisas se medem aqui:
//!
//! 1. quem negociou o 13 recebe a abertura como quem transmite a escreveu, com
//!    a fonte 2, e quem negociou o 12 a recebe com a fonte reescrita como
//!    monitor — ele recusaria o código que não conhece —, e os quadros são os
//!    mesmos bytes para os dois;
//! 2. trocar a tela pela câmera **anuncia o fim da tela** a quem assiste, e a
//!    câmera chega pelo fluxo novo;
//! 3. a troca vale também quando o pedido novo chega com o fluxo antigo ainda
//!    escrevendo, que é o que um cliente faz quando não manda parar antes.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use ed25519_dalek::{Signer, SigningKey};
use seele_proto::control::{ClientMessage, ServerMessage};
use seele_proto::ids::{ScreenId, VoiceRoomId};
use seele_proto::screen::{ScreenCodec, ScreenHeader, ScreenSource, SCREEN_HEADER_LEN};
use seele_proto::version::Faixa;
use seele_server::persistence::Location;
use seele_server::{frame, Daemon, ServerConfig};

mod comum;

const PRAZO: Duration = Duration::from_secs(10);

async fn servidor() -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        caminho_bps: Some(50_000_000),
        ..ServerConfig::default()
    };
    let daemon = Arc::new(Daemon::bind(config).await?);
    let endereco = daemon.local_addr()?;
    let atendendo = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = atendendo.run().await;
    });
    Ok((endereco, daemon))
}

struct Par {
    ligado: comum::Ligado,
    sala: VoiceRoomId,
    /// O protocolo com que este par escreve, e que ele negociou.
    versao: u8,
}

impl Par {
    async fn mandar(&mut self, mensagem: &ClientMessage) -> Result<()> {
        frame::write_em(&mut self.ligado.envio, self.versao, mensagem).await
    }

    async fn proximo(&mut self) -> Result<ServerMessage> {
        tokio::time::timeout(PRAZO, frame::read::<ServerMessage>(&mut self.ligado.recebe)).await?
    }

    /// Lê o `ctl` até a mensagem que `escolhe` reconhece.
    async fn ate<T>(&mut self, mut escolhe: impl FnMut(&ServerMessage) -> Option<T>) -> Result<T> {
        loop {
            let mensagem = self.proximo().await?;
            if let Some(achado) = escolhe(&mensagem) {
                return Ok(achado);
            }
        }
    }
}

/// Entra falando o protocolo `versao`: a faixa oferecida acaba nele, e é ele
/// que o servidor negocia.
async fn entrar(endereco: SocketAddr, semente: u8, versao: u8) -> Result<Par> {
    let mut ligado = comum::ligar(endereco).await?;
    let chave = SigningKey::from_bytes(&[semente; 32]);
    frame::write_em(
        &mut ligado.envio,
        versao,
        &ClientMessage::Hello {
            faixa: Faixa {
                minima: seele_proto::version::VERSAO_MINIMA,
                maxima: versao,
            },
            client: "par-cru".into(),
            nickname: format!("pessoa{semente:03}"),
            public_key: chave.verifying_key().to_bytes().to_vec(),
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
    let mut par = Par {
        ligado,
        sala: VoiceRoomId(0),
        versao,
    };
    par.sala = par
        .ate(|mensagem| match mensagem {
            ServerMessage::Session { voice_rooms, .. } => voice_rooms.first().map(|sala| sala.id),
            _ => None,
        })
        .await?;
    let sala = par.sala;
    par.mandar(&ClientMessage::EnterVoiceRoom {
        voice_room: sala,
        password: None,
    })
    .await?;
    Ok(par)
}

fn abertura(screen: ScreenId, fonte: ScreenSource) -> Vec<u8> {
    let mut bytes = vec![0_u8; SCREEN_HEADER_LEN];
    ScreenHeader {
        version: seele_proto::screen::SCREEN_HEADER_VERSION,
        screen,
        source: fonte,
        codec: ScreenCodec::H264Baseline,
        width: 1280,
        height: 720,
    }
    .encode(&mut bytes)
    .unwrap();
    bytes
}

/// Um quadro com o enquadramento na frente; o corpo carrega `marca`, para os
/// bytes de quem assiste poderem ser conferidos com os de quem transmitiu.
fn quadro(chave: bool, marca: u8, tamanho: usize) -> Vec<u8> {
    let mut bytes = vec![u8::from(chave)];
    bytes.extend_from_slice(&u32::try_from(tamanho).unwrap().to_be_bytes());
    bytes.extend(std::iter::repeat_n(marca, tamanho));
    bytes
}

/// Pede a transmissão e devolve o nome que o servidor deu a ela.
async fn pedir(par: &mut Par) -> Result<ScreenId> {
    par.mandar(&ClientMessage::StartScreenShare).await?;
    par.ate(|mensagem| match mensagem {
        ServerMessage::ScreenShareStarted { screen, .. } => Some(*screen),
        _ => None,
    })
    .await
}

/// Abre o fluxo de uma transmissão e escreve a abertura dela.
async fn abrir(par: &Par, screen: ScreenId, fonte: ScreenSource) -> Result<seele_enlace::Envio> {
    let mut fluxo = par.ligado.conexao.open_uni().await?;
    fluxo
        .write_all(&[seele_proto::stream::StreamType::Screen.byte()])
        .await?;
    fluxo.write_all(&abertura(screen, fonte)).await?;
    Ok(fluxo)
}

/// O próximo fluxo de tela que chega a este par: a abertura e os `corpo` bytes
/// que vêm depois dela.
async fn receber(par: &Par, corpo: usize) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut fluxo = tokio::time::timeout(PRAZO, par.ligado.conexao.accept_uni()).await??;
    let mut inicio = vec![0_u8; 1 + SCREEN_HEADER_LEN];
    tokio::time::timeout(PRAZO, fluxo.read_exact(&mut inicio)).await??;
    assert_eq!(
        inicio[0],
        seele_proto::stream::StreamType::Screen.byte(),
        "o fluxo que chegou não é de tela"
    );
    let mut resto = vec![0_u8; corpo];
    tokio::time::timeout(PRAZO, fluxo.read_exact(&mut resto)).await??;
    Ok((inicio[1..].to_vec(), resto))
}

/// **A câmera chega com a fonte 2 a quem negociou o 13, e como monitor a quem
/// negociou o 12** — pelo servidor inteiro, e com os mesmos quadros.
#[tokio::test(flavor = "multi_thread")]
async fn a_camera_chega_inteira_ao_treze_e_como_monitor_ao_doze() -> Result<()> {
    let (endereco, daemon) = servidor().await?;
    let mut mostra = entrar(endereco, 1, 13).await?;
    let do_treze = entrar(endereco, 2, 13).await?;
    let do_doze = entrar(endereco, 3, 12).await?;
    // Os dois sentados antes de a transmissão existir: entram no fluxo do
    // primeiro byte.
    tokio::time::sleep(Duration::from_millis(300)).await;

    let screen = pedir(&mut mostra).await?;
    let mut fluxo = abrir(&mostra, screen, ScreenSource::Camera).await?;
    let quadros = [quadro(true, 0xC1, 3_000), quadro(false, 0xC2, 700)].concat();
    fluxo.write_all(&quadros).await?;

    let (abertura_do_treze, corpo_do_treze) = receber(&do_treze, quadros.len()).await?;
    let (abertura_do_doze, corpo_do_doze) = receber(&do_doze, quadros.len()).await?;

    assert_eq!(
        abertura_do_treze,
        abertura(screen, ScreenSource::Camera),
        "quem negociou o 13 não recebeu a abertura da câmera como ela foi escrita"
    );
    let (lida, _) = ScreenHeader::decode(&abertura_do_doze)
        .expect("quem negociou o 12 recebeu uma abertura que um cliente do 12 não lê");
    assert_eq!(
        lida.source,
        ScreenSource::Monitor,
        "quem negociou o 12 recebeu a fonte da câmera, que ele recusa"
    );
    assert_eq!(
        abertura_do_doze,
        abertura(screen, ScreenSource::Monitor),
        "a reescrita para o 12 mexeu em mais que o byte da fonte"
    );
    assert_eq!(
        corpo_do_treze, quadros,
        "os quadros não chegaram iguais ao 13"
    );
    assert_eq!(
        corpo_do_doze, quadros,
        "os quadros não chegaram iguais ao 12"
    );

    daemon.shutdown();
    Ok(())
}

/// O `ctl` de quem assiste, do fim de uma transmissão ao começo da seguinte:
/// devolve a que começou depois de `velha` acabar.
async fn fim_e_recomeco(par: &mut Par, velha: ScreenId) -> Result<ScreenId> {
    par.ate(|mensagem| match mensagem {
        ServerMessage::ScreenShareStopped { screen, .. } if *screen == velha => Some(()),
        _ => None,
    })
    .await
    .map_err(|erro| anyhow::anyhow!("o fim da tela trocada não foi anunciado: {erro}"))?;
    par.ate(|mensagem| match mensagem {
        ServerMessage::ScreenShareStarted { screen, .. } if *screen != velha => Some(*screen),
        _ => None,
    })
    .await
}

/// **Trocar a tela pela câmera anuncia o fim da tela, e a câmera chega.** O
/// cliente para a tela, fecha o fluxo dela e pede a câmera — é o que o
/// aplicativo e a página fazem.
#[tokio::test(flavor = "multi_thread")]
async fn trocar_a_tela_pela_camera_anuncia_o_fim_da_tela_e_a_camera_chega() -> Result<()> {
    let (endereco, daemon) = servidor().await?;
    let mut mostra = entrar(endereco, 1, 13).await?;
    let mut assiste = entrar(endereco, 2, 13).await?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let tela = pedir(&mut mostra).await?;
    let mut fluxo_da_tela = abrir(&mostra, tela, ScreenSource::Monitor).await?;
    let da_tela = quadro(true, 0xA1, 2_000);
    fluxo_da_tela.write_all(&da_tela).await?;
    let (abertura_da_tela, _) = receber(&assiste, da_tela.len()).await?;
    assert_eq!(abertura_da_tela, abertura(tela, ScreenSource::Monitor));

    // A troca: parar, fechar, pedir de novo.
    mostra.mandar(&ClientMessage::StopScreenShare).await?;
    let _ = fluxo_da_tela.finish();
    let camera = pedir(&mut mostra).await?;
    assert_ne!(camera, tela, "a câmera herdou o nome da tela que parou");
    assert_eq!(
        fim_e_recomeco(&mut assiste, tela).await?,
        camera,
        "quem assiste não ouviu o fim da tela e o começo da câmera, nesta ordem"
    );

    let mut fluxo_da_camera = abrir(&mostra, camera, ScreenSource::Camera).await?;
    let da_camera = quadro(true, 0xC1, 2_000);
    fluxo_da_camera.write_all(&da_camera).await?;
    let (abertura_da_camera, corpo) = receber(&assiste, da_camera.len()).await?;
    assert_eq!(
        abertura_da_camera,
        abertura(camera, ScreenSource::Camera),
        "o fluxo que chegou depois da troca não é o da câmera"
    );
    assert_eq!(corpo, da_camera);

    daemon.shutdown();
    Ok(())
}

/// **A troca sem parar antes.** O pedido novo chega com o fluxo da tela ainda
/// escrevendo: o servidor troca a vaga, anuncia o fim da tela, e a câmera tem
/// de chegar assim mesmo — sem que o fluxo antigo, ao acabar, leve a câmera
/// junto, e sem que a sala fique com a tela velha presa no lugar dela.
#[tokio::test(flavor = "multi_thread")]
async fn a_troca_sem_parar_antes_tambem_entrega_a_camera() -> Result<()> {
    let (endereco, daemon) = servidor().await?;
    let mut mostra = entrar(endereco, 1, 13).await?;
    let mut assiste = entrar(endereco, 2, 13).await?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let tela = pedir(&mut mostra).await?;
    let mut fluxo_da_tela = abrir(&mostra, tela, ScreenSource::Monitor).await?;
    fluxo_da_tela.write_all(&quadro(true, 0xA1, 2_000)).await?;
    let _ = receber(&assiste, 2_005).await?;

    // A tela continua escrevendo enquanto o pedido novo é atendido, e por um
    // instante depois dele: é a janela em que o fim dela e o começo da câmera
    // se cruzam no servidor.
    let escrevendo = tokio::spawn(async move {
        for n in 0..25_u8 {
            if fluxo_da_tela
                .write_all(&quadro(n % 10 == 0, 0xA2, 1_500))
                .await
                .is_err()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let _ = fluxo_da_tela.finish();
    });
    tokio::time::sleep(Duration::from_millis(40)).await;
    let camera = pedir(&mut mostra).await?;
    assert_eq!(fim_e_recomeco(&mut assiste, tela).await?, camera);
    escrevendo.await?;

    let mut fluxo_da_camera = abrir(&mostra, camera, ScreenSource::Camera).await?;
    let da_camera = quadro(true, 0xC1, 2_000);
    // Mais de um quadro-chave, espaçados: quem assiste é ligado no próximo
    // quadro-chave, e a pergunta é se **algum** fluxo de câmera chega.
    let conexao = mostra.ligado.conexao.clone();
    let mandando = tokio::spawn(async move {
        let _conexao = conexao;
        for _ in 0..40 {
            if fluxo_da_camera.write_all(&da_camera).await.is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });
    let (abertura_da_camera, corpo) = receber(&assiste, 2_005)
        .await
        .map_err(|erro| anyhow::anyhow!("a câmera não chegou depois da troca sem parar: {erro}"))?;
    assert_eq!(abertura_da_camera, abertura(camera, ScreenSource::Camera));
    assert_eq!(corpo, quadro(true, 0xC1, 2_000));
    mandando.abort();

    daemon.shutdown();
    Ok(())
}

/// **Parar com o fluxo ainda escrevendo fecha o fluxo de quem assiste.**
///
/// O controle tira a vaga no `StopScreenShare`, e quem lê o fluxo de quem
/// transmite percebe na leitura seguinte que ele não vale mais. Voltar dali
/// calado deixava a transmissão registrada na sala: os canos de quem assistia
/// ficavam abertos para um fluxo que ninguém mais alimenta, e a cópia dela
/// seguia na conta do teto. Aqui quem assiste lê até o fluxo **acabar**.
#[tokio::test(flavor = "multi_thread")]
async fn parar_com_o_fluxo_ainda_escrevendo_fecha_o_fluxo_de_quem_assiste() -> Result<()> {
    let (endereco, daemon) = servidor().await?;
    let mut mostra = entrar(endereco, 1, 13).await?;
    let assiste = entrar(endereco, 2, 13).await?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let tela = pedir(&mut mostra).await?;
    let mut fluxo = abrir(&mostra, tela, ScreenSource::Monitor).await?;
    fluxo.write_all(&quadro(true, 0xA1, 2_000)).await?;
    let mut chegando = tokio::time::timeout(PRAZO, assiste.ligado.conexao.accept_uni()).await??;

    // Quem transmite segue escrevendo, e **não** fecha o fluxo: só o controle
    // diz que parou.
    let escrevendo = tokio::spawn(async move {
        for _ in 0..200 {
            if fluxo.write_all(&quadro(false, 0xA2, 1_000)).await.is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        fluxo
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    mostra.mandar(&ClientMessage::StopScreenShare).await?;

    let acabou = tokio::time::timeout(Duration::from_secs(5), async {
        let mut pedaco = vec![0_u8; 16 * 1024];
        while let Ok(Some(_)) = chegando.read(&mut pedaco).await {}
    })
    .await;
    assert!(
        acabou.is_ok(),
        "a transmissão parou no controle e o fluxo de quem assiste continuou aberto: a sala \
         ficou com a transmissão registrada"
    );
    let _fluxo = escrevendo.await?;

    daemon.shutdown();
    Ok(())
}
