//! **A revogação vale no servidor na hora, sem o cliente cooperar** (revisão
//! do Codex dos cargos, ADR 0064).
//!
//! Quem tem o Falar ou o Compartilhar a tela revogado continua mandando mídia
//! e **para de ler o `ctl`** — o cliente que não coopera. A revogação não pode
//! depender do aviso chegar a ele: quem gravou a mudança de cargos aplica o
//! efeito nas salas de voz e nas transmissões antes de difundir o quadro
//! (`session::aplicar_os_cargos`). Aqui se mede, com o servidor de verdade e o
//! enlace de verdade, que depois da revogação (e de uma folga curta para o que
//! já estava em voo) **nada** daquele par chega a quem ouve ou assiste, e que a
//! tela é anunciada como encerrada.

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
use seele_proto::cargos::{Permissao, QuadroDeCargos, TODOS};
use seele_proto::control::{ClientMessage, Permission, ServerMessage};
use seele_proto::ids::{ScreenId, Ssrc, VoiceRoomId};
use seele_proto::media::MediaHeader;
use seele_server::persistence::Location;
use seele_server::{frame, Daemon, ServerConfig};

mod comum;

const GESTORA: u8 = 1;
const FALANTE: u8 = 2;
const OUVINTE: u8 = 3;

/// A folga para o que já estava em voo quando a revogação foi gravada.
const FOLGA: Duration = Duration::from_millis(300);

fn publica(semente: u8) -> [u8; 32] {
    SigningKey::from_bytes(&[semente; 32])
        .verifying_key()
        .to_bytes()
}

async fn servidor() -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        proprietario: Some(seele_proto::transport::key_fingerprint(&publica(GESTORA))),
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
    ssrc: Ssrc,
    sala: VoiceRoomId,
}

async fn entrar(endereco: SocketAddr, semente: u8) -> Result<Par> {
    let mut ligado = comum::ligar(endereco).await?;
    let chave = SigningKey::from_bytes(&[semente; 32]);
    frame::write(
        &mut ligado.envio,
        &ClientMessage::Hello {
            faixa: seele_proto::FAIXA,
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
    frame::write(
        &mut ligado.envio,
        &ClientMessage::Response {
            proof: chave
                .sign(&comum::prova_amarrada(&ligado.conexao, &nonce))
                .to_bytes()
                .to_vec(),
        },
    )
    .await?;
    loop {
        if let ServerMessage::Session {
            ssrc, voice_rooms, ..
        } = frame::read::<ServerMessage>(&mut ligado.recebe).await?
        {
            let sala = voice_rooms.first().map(|v| v.id).expect("uma sala");
            return Ok(Par { ligado, ssrc, sala });
        }
    }
}

async fn entrar_na_sala(par: &mut Par) -> Result<()> {
    frame::write(
        &mut par.ligado.envio,
        &ClientMessage::EnterVoiceRoom {
            voice_room: par.sala,
            password: None,
        },
    )
    .await
}

fn quadro_de_voz(ssrc: Ssrc, seq: u16) -> Vec<u8> {
    let cabecalho = MediaHeader {
        ssrc: ssrc.get(),
        seq,
        timestamp: u32::from(seq) * 960,
    };
    let mut fora = vec![0_u8; seele_proto::MAX_DATAGRAM_LEN];
    let tamanho = cabecalho
        .encode_datagram(&[0xF8; 40], &mut fora)
        .expect("cabe");
    fora.truncate(tamanho);
    fora
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
        assert!(
            Instant::now() < ate,
            "a gestora não recebeu o quadro de cargos"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// A gestora restringe `permissao` no `@everyone`: vale para todo mundo
/// menos a proprietária. Volta quando o quadro novo chegou a ela, que é
/// depois de a mudança ter sido gravada e aplicada.
async fn revogar(
    gestora: &mut seele_enlace::Envio,
    quadro: &Arc<Mutex<Option<QuadroDeCargos>>>,
    permissao: Permission,
) -> Result<Instant> {
    let antes = quadro_mais_novo_que(quadro, None).await;
    let todos = antes
        .cargos
        .iter()
        .find(|c| c.id == TODOS)
        .expect("o @everyone")
        .clone();
    let mut restringe = todos.restringe.clone();
    restringe.push(Permissao::Produto(permissao));
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
    Ok(Instant::now())
}

/// **Falar revogado: a voz de quem não lê o `ctl` deixa de ser encaminhada.**
#[tokio::test(flavor = "multi_thread")]
async fn revogar_falar_cala_na_hora_quem_nao_le_o_ctl() -> Result<()> {
    let (endereco, daemon) = servidor().await?;
    let gestora = entrar(endereco, GESTORA).await?;
    let mut falante = entrar(endereco, FALANTE).await?;
    let mut ouvinte = entrar(endereco, OUVINTE).await?;
    entrar_na_sala(&mut falante).await?;
    entrar_na_sala(&mut ouvinte).await?;
    let Par {
        ligado: gestora, ..
    } = gestora;
    let comum::Ligado {
        ponta: _pg,
        conexao: _cg,
        envio: mut envio_da_gestora,
        recebe: recebe_da_gestora,
    } = gestora;
    let quadro = acompanhar_a_gestora(recebe_da_gestora);

    // A ouvinte drena o ctl e conta a voz da falante, com o instante.
    let Par { ligado: ouv, .. } = ouvinte;
    let comum::Ligado {
        ponta: _po,
        conexao: conexao_da_ouvinte,
        envio: _eo,
        recebe: mut recebe_da_ouvinte,
    } = ouv;
    tokio::spawn(async move {
        while frame::read::<ServerMessage>(&mut recebe_da_ouvinte)
            .await
            .is_ok()
        {}
    });
    let ssrc = falante.ssrc;
    let chegadas: Arc<Mutex<Vec<Instant>>> = Arc::new(Mutex::new(Vec::new()));
    {
        let chegadas = Arc::clone(&chegadas);
        tokio::spawn(async move {
            while let Ok(bytes) = conexao_da_ouvinte.read_datagram().await {
                if MediaHeader::decode(&bytes).is_ok_and(|(c, _)| c.ssrc == ssrc.get()) {
                    chegadas.lock().unwrap().push(Instant::now());
                }
            }
        });
    }
    tokio::time::sleep(Duration::from_millis(300)).await;

    // A falante fala sem parar e **nunca mais lê o ctl**.
    let conexao_da_falante = falante.ligado.conexao.clone();
    let falando = tokio::spawn(async move {
        for seq in 0..200_u16 {
            let _ = conexao_da_falante.send_datagram(quadro_de_voz(ssrc, seq));
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });
    tokio::time::sleep(Duration::from_secs(1)).await;
    let antes_da_revogacao = chegadas.lock().unwrap().len();
    assert!(
        antes_da_revogacao > 10,
        "a voz não chegava nem antes da revogação ({antes_da_revogacao}): o teste não mede nada"
    );

    let revogada = revogar(&mut envio_da_gestora, &quadro, Permission::Speak).await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let depois: Vec<_> = chegadas
        .lock()
        .unwrap()
        .iter()
        .filter(|quando| **quando > revogada + FOLGA)
        .copied()
        .collect();
    println!(
        "antes {antes_da_revogacao}, depois da revogação e da folga {}",
        depois.len()
    );
    assert!(
        depois.is_empty(),
        "{} datagramas da falante foram encaminhados depois da revogação do Falar (e da folga \
         de {FOLGA:?}): a revogação esperou o cliente ler o ctl",
        depois.len()
    );
    falando.abort();
    drop(falante);
    daemon.shutdown();
    Ok(())
}

fn abertura(screen: ScreenId) -> Vec<u8> {
    let cabecalho = seele_proto::screen::ScreenHeader {
        version: seele_proto::PROTOCOL_VERSION,
        screen,
        source: seele_proto::screen::ScreenSource::Monitor,
        codec: seele_proto::screen::ScreenCodec::H264Baseline,
        width: 1280,
        height: 720,
    };
    let mut bytes = vec![0_u8; seele_proto::screen::SCREEN_HEADER_LEN];
    let len = cabecalho.encode(&mut bytes).unwrap();
    bytes.truncate(len);
    bytes
}

fn quadro_de_tela(chave: bool, tamanho: usize) -> Vec<u8> {
    let mut bytes = vec![u8::from(chave)];
    bytes.extend_from_slice(&u32::try_from(tamanho).unwrap().to_be_bytes());
    bytes.extend(std::iter::repeat_n(0xAB, tamanho));
    bytes
}

/// **Compartilhar revogado: a tela para no servidor, é anunciada, e nada mais
/// dela chega a quem assiste** — com quem compartilha sem ler o `ctl`.
#[tokio::test(flavor = "multi_thread")]
async fn revogar_compartilhar_para_a_tela_na_hora_sem_o_cliente_ler() -> Result<()> {
    let (endereco, daemon) = servidor().await?;
    let gestora = entrar(endereco, GESTORA).await?;
    let mut falante = entrar(endereco, FALANTE).await?;
    let mut ouvinte = entrar(endereco, OUVINTE).await?;
    entrar_na_sala(&mut falante).await?;
    entrar_na_sala(&mut ouvinte).await?;
    let Par {
        ligado: gestora, ..
    } = gestora;
    let comum::Ligado {
        ponta: _pg,
        conexao: _cg,
        envio: mut envio_da_gestora,
        recebe: recebe_da_gestora,
    } = gestora;
    let quadro = acompanhar_a_gestora(recebe_da_gestora);

    // A falante pede a tela e lê o ctl só até saber o número dela.
    frame::write(&mut falante.ligado.envio, &ClientMessage::StartScreenShare).await?;
    let screen = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let ServerMessage::ScreenShareStarted { screen, .. } =
                frame::read::<ServerMessage>(&mut falante.ligado.recebe).await?
            {
                return anyhow::Ok(screen);
            }
        }
    })
    .await??;

    // A ouvinte assiste, e conta os bytes de tela que chegam, e o anúncio.
    frame::write(
        &mut ouvinte.ligado.envio,
        &ClientMessage::WatchScreen { screen },
    )
    .await?;
    let Par { ligado: ouv, .. } = ouvinte;
    let comum::Ligado {
        ponta: _po,
        conexao: conexao_da_ouvinte,
        envio: _eo,
        recebe: mut recebe_da_ouvinte,
    } = ouv;
    let anunciou_o_fim = Arc::new(Mutex::new(None::<Instant>));
    {
        let anunciou = Arc::clone(&anunciou_o_fim);
        tokio::spawn(async move {
            while let Ok(m) = frame::read::<ServerMessage>(&mut recebe_da_ouvinte).await {
                if let ServerMessage::ScreenShareStopped { screen: s, .. } = m {
                    if s == screen {
                        *anunciou.lock().unwrap() = Some(Instant::now());
                    }
                }
            }
        });
    }
    let bytes_de_tela: Arc<Mutex<Vec<(Instant, usize)>>> = Arc::new(Mutex::new(Vec::new()));
    {
        let bytes_de_tela = Arc::clone(&bytes_de_tela);
        tokio::spawn(async move {
            while let Ok(mut fluxo) = conexao_da_ouvinte.accept_uni().await {
                let bytes_de_tela = Arc::clone(&bytes_de_tela);
                tokio::spawn(async move {
                    let mut buf = vec![0_u8; 64 * 1024];
                    while let Ok(Some(n)) = fluxo.read(&mut buf).await {
                        bytes_de_tela.lock().unwrap().push((Instant::now(), n));
                    }
                });
            }
        });
    }

    // A falante transmite sem parar e não lê mais o ctl.
    let conexao_da_falante = falante.ligado.conexao.clone();
    let transmitindo = tokio::spawn(async move {
        let mut fluxo = conexao_da_falante.open_uni().await?;
        fluxo
            .write_all(&[seele_proto::stream::StreamType::Screen.byte()])
            .await?;
        fluxo.write_all(&abertura(screen)).await?;
        for n in 0..300 {
            fluxo.write_all(&quadro_de_tela(n % 30 == 0, 4_000)).await?;
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        anyhow::Ok(())
    });
    tokio::time::sleep(Duration::from_secs(1)).await;
    let antes: usize = bytes_de_tela.lock().unwrap().iter().map(|(_, n)| n).sum();
    assert!(
        antes > 10_000,
        "a tela não chegava nem antes da revogação ({antes} B): o teste não mede nada"
    );

    let revogada = revogar(&mut envio_da_gestora, &quadro, Permission::ShareScreen).await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let depois: usize = bytes_de_tela
        .lock()
        .unwrap()
        .iter()
        .filter(|(quando, _)| *quando > revogada + FOLGA)
        .map(|(_, n)| n)
        .sum();
    println!("tela: antes {antes} B, depois da revogação e da folga {depois} B");
    assert_eq!(
        depois, 0,
        "a tela continuou chegando a quem assiste depois da revogação do Compartilhar"
    );
    let anuncio = *anunciou_o_fim.lock().unwrap();
    assert!(
        anuncio.is_some_and(|quando| quando <= revogada + FOLGA),
        "o fim da tela não foi anunciado logo depois da revogação: {anuncio:?}"
    );
    transmitindo.abort();
    drop(falante);
    daemon.shutdown();
    Ok(())
}
