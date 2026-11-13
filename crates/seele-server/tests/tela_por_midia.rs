//! **O vídeo por RTP pelo servidor de verdade** (ADR 0067): pares crus sobre o
//! enlace, com a sala no meio traduzindo entre a mídia e o canal.
//!
//! A sala repassa o fluxo enquadrado de sempre; a tradução é nas bordas. O
//! que se mede aqui é que **nenhuma combinação fica sem imagem**:
//!
//! 1. quem transmite por RTP chega por RTP a quem ligou a mídia e pelo canal
//!    `u` a quem não ligou, com os mesmos quadros;
//! 2. quem transmite pelo canal chega por RTP a quem ligou a mídia;
//! 3. o pedido de quadro-chave de quem assiste por RTP (PLI) chega a quem
//!    transmite como o `KeyFrameRequested` de sempre;
//! 4. um quadro comum que chega sem a cadeia inteira não vai à sala;
//! 5. o fim da transmissão chega dito a quem assiste por RTP.

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
use seele_enlace::{EventoDeMidia, Midia, RemetenteDeMidia, Trilha};
use seele_proto::control::{ClientMessage, ServerMessage};
use seele_proto::ids::{ScreenId, VoiceRoomId};
use seele_proto::screen::{
    LeitorDeTela, LidoDaTela, ScreenCodec, ScreenHeader, ScreenSource, TipoDeQuadro,
    SCREEN_HEADER_LEN,
};
use seele_proto::version::Faixa;
use seele_server::persistence::Location;
use seele_server::{frame, Daemon, ServerConfig};

mod comum;

const PRAZO: Duration = Duration::from_secs(10);
const FAIXA: u8 = 0;

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
    versao: u8,
    /// A mídia RTP deste par, se ele a ligou.
    midia: Option<Midia>,
}

impl Par {
    async fn mandar(&mut self, mensagem: &ClientMessage) -> Result<()> {
        frame::write_em(&mut self.ligado.envio, self.versao, mensagem).await
    }

    /// Lê o `ctl` até a mensagem que `escolhe` reconhece. Com prazo para o
    /// todo: a telemetria chega a cada segundo, e sem ele uma mensagem que
    /// nunca vem seria um teste parado em vez de um teste reprovado.
    async fn ate<T>(&mut self, mut escolhe: impl FnMut(&ServerMessage) -> Option<T>) -> Result<T> {
        let fim = tokio::time::Instant::now() + PRAZO;
        loop {
            let mensagem =
                tokio::time::timeout_at(fim, frame::read::<ServerMessage>(&mut self.ligado.recebe))
                    .await
                    .map_err(|_| {
                        anyhow::anyhow!("a mensagem esperada não chegou em {PRAZO:?}")
                    })??;
            if let Some(achado) = escolhe(&mensagem) {
                return Ok(achado);
            }
        }
    }

    fn remetente(&self) -> RemetenteDeMidia {
        self.midia.as_ref().expect("a mídia ligada").remetente()
    }

    /// O próximo quadro de vídeo que desce pela mídia.
    async fn quadro_da_midia(&mut self) -> Result<Vec<u8>> {
        let midia = self.midia.as_mut().expect("a mídia ligada");
        loop {
            match tokio::time::timeout(PRAZO, midia.receber()).await? {
                Some(EventoDeMidia::Quadro {
                    faixa: FAIXA,
                    trilha: Trilha::Video,
                    bytes,
                    ..
                }) => return Ok(bytes.to_vec()),
                Some(_) => {}
                None => anyhow::bail!("a mídia acabou"),
            }
        }
    }
}

/// Entra falando `versao`; com `midia`, liga a mídia RTP e diz ao servidor.
async fn entrar(endereco: SocketAddr, semente: u8, versao: u8, midia: bool) -> Result<Par> {
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
        midia: None,
    };
    par.sala = par
        .ate(|mensagem| match mensagem {
            ServerMessage::Session { voice_rooms, .. } => voice_rooms.first().map(|sala| sala.id),
            _ => None,
        })
        .await?;
    if midia {
        par.midia = Some(par.ligado.conexao.ligar_midia());
        par.mandar(&ClientMessage::MidiaLigada).await?;
    }
    let sala = par.sala;
    par.mandar(&ClientMessage::EnterVoiceRoom {
        voice_room: sala,
        password: None,
    })
    .await?;
    Ok(par)
}

fn abertura(screen: ScreenId) -> [u8; SCREEN_HEADER_LEN] {
    let mut bytes = [0_u8; SCREEN_HEADER_LEN];
    ScreenHeader {
        version: seele_proto::screen::SCREEN_HEADER_VERSION,
        screen,
        source: ScreenSource::Monitor,
        codec: ScreenCodec::H264Baseline,
        width: 1280,
        height: 720,
    }
    .encode(&mut bytes)
    .unwrap();
    bytes
}

/// Um quadro H.264 em Annex B: IDR ou fatia comum, com `marca` no corpo.
fn h264(chave: bool, marca: u8, tamanho: usize) -> Vec<u8> {
    let mut bytes = vec![0, 0, 0, 1, if chave { 0x65 } else { 0x41 }];
    bytes.resize(tamanho, marca);
    bytes
}

/// O mesmo quadro com o enquadramento do canal na frente.
fn enquadrado(chave: bool, corpo: &[u8]) -> Vec<u8> {
    let mut bytes = vec![u8::from(chave)];
    bytes.extend_from_slice(&u32::try_from(corpo.len()).unwrap().to_be_bytes());
    bytes.extend_from_slice(corpo);
    bytes
}

async fn pedir(par: &mut Par) -> Result<ScreenId> {
    par.mandar(&ClientMessage::StartScreenShare).await?;
    par.ate(|mensagem| match mensagem {
        ServerMessage::ScreenShareStarted { screen, .. } => Some(*screen),
        _ => None,
    })
    .await
}

/// Lê do próximo canal de tela até `quantos` quadros.
async fn quadros_do_canal(
    par: &Par,
    quantos: usize,
) -> Result<(ScreenHeader, Vec<(TipoDeQuadro, Vec<u8>)>)> {
    let mut fluxo = tokio::time::timeout(PRAZO, par.ligado.conexao.accept_uni()).await??;
    let mut marca = [0_u8; 1];
    tokio::time::timeout(PRAZO, fluxo.read_exact(&mut marca)).await??;
    assert_eq!(marca[0], seele_proto::stream::StreamType::Screen.byte());
    let mut leitor = LeitorDeTela::novo();
    let mut cabecalho = None;
    let mut quadros = Vec::new();
    let mut buffer = vec![0_u8; 8 * 1024];
    while quadros.len() < quantos {
        let lidos = tokio::time::timeout(PRAZO, fluxo.read(&mut buffer))
            .await??
            .expect("o canal acabou antes dos quadros");
        leitor.empurrar(&buffer[..lidos]).unwrap();
        while let Some(lido) = leitor.proximo() {
            match lido {
                LidoDaTela::Abertura(c) => cabecalho = Some(c),
                LidoDaTela::Quadro { tipo, bytes } => quadros.push((tipo, bytes)),
            }
        }
    }
    Ok((cabecalho.expect("a abertura"), quadros))
}

/// O corpo de um quadro que desceu pela mídia, sem o prefixo de início (o
/// `str0m` pode devolver três ou quatro bytes de prefixo).
fn corpo(annex_b: &[u8]) -> &[u8] {
    let inicio = annex_b
        .windows(3)
        .position(|j| j == [0, 0, 1])
        .expect("um prefixo de início")
        + 3;
    &annex_b[inicio..]
}

/// **Quem transmite por RTP chega por RTP a quem ligou a mídia, e pelo canal
/// a quem não ligou** — os mesmos quadros, pelo servidor inteiro.
#[tokio::test(flavor = "multi_thread")]
async fn a_tela_que_sobe_por_rtp_desce_por_rtp_e_pelo_canal() -> Result<()> {
    let (endereco, daemon) = servidor().await?;
    let mut mostra = entrar(endereco, 1, 13, true).await?;
    let mut pela_midia = entrar(endereco, 2, 13, true).await?;
    // Um par do 13 que não ligou a mídia (a página do navegador) e um do 12.
    let sem_midia = entrar(endereco, 3, 13, false).await?;
    let do_doze = entrar(endereco, 4, 12, false).await?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let screen = pedir(&mut mostra).await?;
    mostra
        .mandar(&ClientMessage::TelaPorMidia {
            abertura: abertura(screen),
        })
        .await?;
    let chave = h264(true, 0xC1, 3_000);
    let comum_ = h264(false, 0xC2, 700);
    let saida = mostra.remetente();
    // A entrada abre quando o servidor lê o `TelaPorMidia`: os quadros se
    // repetem até a sala os ver, como um codificador de verdade faria.
    let mandando = {
        let (chave, comum_) = (chave.clone(), comum_.clone());
        tokio::spawn(async move {
            let mut tempo = 0;
            loop {
                let _ = saida.mandar(FAIXA, Trilha::Video, tempo, chave.clone().into());
                tokio::time::sleep(Duration::from_millis(33)).await;
                let _ = saida.mandar(FAIXA, Trilha::Video, tempo + 3000, comum_.clone().into());
                tokio::time::sleep(Duration::from_millis(33)).await;
                tempo += 6000;
            }
        })
    };

    // Pela mídia: a abertura vem pelo controle, com a faixa, e depois os quadros.
    let (faixa, dita) = pela_midia
        .ate(|mensagem| match mensagem {
            ServerMessage::TelaPorMidia { faixa, abertura } => Some((*faixa, *abertura)),
            _ => None,
        })
        .await?;
    assert_eq!(faixa, FAIXA);
    assert_eq!(dita, abertura(screen));
    let primeiro = pela_midia.quadro_da_midia().await?;
    assert_eq!(
        corpo(&primeiro),
        &chave[4..],
        "o primeiro quadro pela mídia não é o quadro-chave"
    );
    let segundo = pela_midia.quadro_da_midia().await?;
    assert_eq!(corpo(&segundo), &comum_[4..]);

    // Pelo canal, para os dois que não ligaram a mídia: o mesmo conteúdo, no
    // enquadramento de sempre, começando na chave.
    for (nome, par) in [("13 sem mídia", &sem_midia), ("12", &do_doze)] {
        let (cabecalho, quadros) = quadros_do_canal(par, 2).await?;
        assert_eq!(cabecalho.screen, screen, "{nome}");
        assert_eq!(quadros[0].0, TipoDeQuadro::Chave, "{nome}");
        assert_eq!(corpo(&quadros[0].1), &chave[4..], "{nome}");
        assert_eq!(quadros[1].0, TipoDeQuadro::Comum, "{nome}");
        assert_eq!(corpo(&quadros[1].1), &comum_[4..], "{nome}");
    }

    mandando.abort();
    daemon.shutdown();
    Ok(())
}

/// **Quem transmite pelo canal chega por RTP a quem ligou a mídia**, e o
/// pedido de quadro-chave dele (PLI) chega a quem transmite pelo controle.
#[tokio::test(flavor = "multi_thread")]
async fn a_tela_que_sobe_pelo_canal_desce_por_rtp_e_o_pli_vira_pedido_de_chave() -> Result<()> {
    let (endereco, daemon) = servidor().await?;
    let mut mostra = entrar(endereco, 1, 12, false).await?;
    let mut pela_midia = entrar(endereco, 2, 13, true).await?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let screen = pedir(&mut mostra).await?;
    let mut fluxo = mostra.ligado.conexao.open_uni().await?;
    fluxo
        .write_all(&[seele_proto::stream::StreamType::Screen.byte()])
        .await?;
    fluxo.write_all(&abertura(screen)).await?;
    let chave = h264(true, 0xA1, 2_000);
    let comum_ = h264(false, 0xA2, 500);
    fluxo.write_all(&enquadrado(true, &chave)).await?;
    fluxo.write_all(&enquadrado(false, &comum_)).await?;

    let dita = pela_midia
        .ate(|mensagem| match mensagem {
            ServerMessage::TelaPorMidia { abertura, .. } => Some(*abertura),
            _ => None,
        })
        .await?;
    assert_eq!(dita, abertura(screen));
    assert_eq!(corpo(&pela_midia.quadro_da_midia().await?), &chave[4..]);
    assert_eq!(corpo(&pela_midia.quadro_da_midia().await?), &comum_[4..]);

    // Quem assiste pede um quadro-chave pela mídia: quem transmite ouve o
    // mesmo `KeyFrameRequested` de um `RequestKeyFrame`.
    pela_midia.remetente().pedir_chave(FAIXA);
    let pedida = mostra
        .ate(|mensagem| match mensagem {
            ServerMessage::KeyFrameRequested { screen, .. } => Some(*screen),
            _ => None,
        })
        .await?;
    assert_eq!(pedida, screen);

    // E o fim da transmissão vem dito a quem assiste pela mídia, como limpo:
    // no canal seria o `finish` do fluxo, e a mídia não tem fluxo para fechar.
    mostra.mandar(&ClientMessage::StopScreenShare).await?;
    let _ = fluxo.finish();
    let (faixa, limpo) = pela_midia
        .ate(|mensagem| match mensagem {
            ServerMessage::FimDaTelaPorMidia { faixa, limpo } => Some((*faixa, *limpo)),
            _ => None,
        })
        .await?;
    assert_eq!((faixa, limpo), (FAIXA, true));

    daemon.shutdown();
    Ok(())
}

/// **Um quadro comum sem a cadeia inteira não vai à sala.** A transmissão por
/// RTP que começa num quadro comum não entrega nada até a chave, e o servidor
/// pede a chave a quem transmite (PLI).
#[tokio::test(flavor = "multi_thread")]
async fn o_quadro_comum_antes_da_chave_nao_vai_a_sala() -> Result<()> {
    let (endereco, daemon) = servidor().await?;
    let mut mostra = entrar(endereco, 1, 13, true).await?;
    let do_doze = entrar(endereco, 2, 12, false).await?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let screen = pedir(&mut mostra).await?;
    mostra
        .mandar(&ClientMessage::TelaPorMidia {
            abertura: abertura(screen),
        })
        .await?;
    let saida = mostra.remetente();
    // Só quadros comuns, até o servidor pedir a chave.
    let orfao = h264(false, 0xB0, 600);
    let mut tempo = 0;
    let midia = mostra.midia.as_mut().unwrap();
    loop {
        saida
            .mandar(FAIXA, Trilha::Video, tempo, orfao.clone().into())
            .unwrap();
        tempo += 3000;
        match tokio::time::timeout(Duration::from_millis(50), midia.receber()).await {
            Ok(Some(EventoDeMidia::PediramChave { faixa: FAIXA })) => break,
            Ok(None) => anyhow::bail!("a mídia acabou"),
            _ => assert!(tempo < 3000 * 200, "o servidor nunca pediu a chave"),
        }
    }
    // A chave e um comum depois dela: é daí que quem assiste recebe.
    let chave = h264(true, 0xB1, 2_000);
    let depois = h264(false, 0xB2, 600);
    saida
        .mandar(FAIXA, Trilha::Video, tempo, chave.clone().into())
        .unwrap();
    saida
        .mandar(FAIXA, Trilha::Video, tempo + 3000, depois.clone().into())
        .unwrap();

    let (_, quadros) = quadros_do_canal(&do_doze, 2).await?;
    assert_eq!(quadros[0].0, TipoDeQuadro::Chave);
    assert_eq!(corpo(&quadros[0].1), &chave[4..]);
    assert_eq!(
        corpo(&quadros[1].1),
        &depois[4..],
        "um quadro órfão foi à sala"
    );

    daemon.shutdown();
    Ok(())
}

/// **Os quadros que chegam antes da abertura não se perdem.** A abertura vai
/// pelo controle e os quadros, pela mídia: os primeiros chegam antes dela, e o
/// primeiro é o quadro-chave. O servidor os guarda; quem assiste recebe a
/// transmissão desde aquele quadro-chave, e ninguém pede outro.
#[tokio::test(flavor = "multi_thread")]
async fn os_quadros_de_antes_da_abertura_sao_guardados() -> Result<()> {
    let (endereco, daemon) = servidor().await?;
    let mut mostra = entrar(endereco, 1, 13, true).await?;
    let do_doze = entrar(endereco, 2, 12, false).await?;
    tokio::time::sleep(Duration::from_millis(300)).await;

    let screen = pedir(&mut mostra).await?;
    let saida = mostra.remetente();
    let chave = h264(true, 0xD1, 2_500);
    let primeiro = h264(false, 0xD2, 600);
    let segundo = h264(false, 0xD3, 600);
    // Os três quadros saem **antes** de a abertura ser dita.
    for (n, quadro) in [&chave, &primeiro, &segundo].into_iter().enumerate() {
        saida
            .mandar(FAIXA, Trilha::Video, n as u64 * 3000, quadro.clone().into())
            .unwrap();
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    mostra
        .mandar(&ClientMessage::TelaPorMidia {
            abertura: abertura(screen),
        })
        .await?;
    // Mais um, depois da abertura: é ele que faz o servidor soltar os guardados.
    let terceiro = h264(false, 0xD4, 600);
    tokio::time::sleep(Duration::from_millis(200)).await;
    saida
        .mandar(FAIXA, Trilha::Video, 9000, terceiro.clone().into())
        .unwrap();

    let (_, quadros) = quadros_do_canal(&do_doze, 4).await?;
    assert_eq!(quadros[0].0, TipoDeQuadro::Chave);
    for (chegou, mandado) in quadros.iter().zip([&chave, &primeiro, &segundo, &terceiro]) {
        assert_eq!(
            corpo(&chegou.1),
            &mandado[4..],
            "a transmissão não começou do quadro-chave que saiu antes da abertura"
        );
    }
    // E o servidor não pediu outro quadro-chave a quem transmite.
    let midia = mostra.midia.as_mut().unwrap();
    let pediu = tokio::time::timeout(Duration::from_millis(1500), async {
        loop {
            if let Some(EventoDeMidia::PediramChave { .. }) = midia.receber().await {
                break;
            }
        }
    })
    .await
    .is_ok();
    assert!(!pediu, "o servidor pediu um quadro-chave que já tinha");

    daemon.shutdown();
    Ok(())
}
