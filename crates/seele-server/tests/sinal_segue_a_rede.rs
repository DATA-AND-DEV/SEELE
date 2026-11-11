//! **O sinal que o servidor anuncia segue a rede de quem ele descreve.**
//!
//! O `signal` do `PersonState` é o número da faixa de sinal de cada pessoa na
//! lista, e é dele que o motor do cliente tira a faixa que corta o teto do
//! vídeo. Ele ficou parado em 100 da 0.17 em diante: a sessão só alimentava a
//! conta quando tinha ida e volta **e** a perda de saída do RTCP, e essa perda
//! só existe para quem está **recebendo** mídia RTP do servidor. Quem só fala,
//! quem só ouve e quem transmite ficavam sem medida, e «sem medida» valia 100.
//!
//! Este arquivo põe um caminho ruim de verdade entre um par e o `seeled` em
//! processo (o [`Cano`]) e lê o `PersonState` que o servidor devolve:
//!
//! - caminho limpo: o sinal fica alto;
//! - atraso no caminho, par **calado e sem mídia nenhuma** (o caso do
//!   navegador): o sinal cai pela ida e volta que o ICE mede;
//! - perda no caminho, par falando: o sinal cai pela perda de subida da voz,
//!   que a sala já contava e só dizia a quem falava.
//!
//! A conta em si (qual medida entra, e quando ela envelhece) está provada sem
//! rede em `seele_server::sinal`.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::print_stdout,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use ed25519_dalek::{Signer, SigningKey};
use seele_enlace::teste::{Cano, ConfigDoCano, Sentido};
use seele_proto::control::{ClientMessage, ServerMessage};
use seele_proto::ids::{PersonId, Ssrc, VoiceRoomId};
use seele_proto::media::MediaHeader;
use seele_proto::signal::{DEGRADED_FLOOR, NOMINAL_FLOOR};
use seele_server::persistence::Location;
use seele_server::{frame, Daemon, ServerConfig};

mod comum;

/// Um quadro de voz do tamanho que 32 kbps produz. O servidor não o decodifica.
const PAYLOAD: [u8; 80] = [7; 80];

/// Quanto tempo cada cenário observa. O sinal sai uma vez por segundo, a janela
/// da perda de subida tem cinco, e a primeira medida do ICE leva até dois.
// Doze e não nove: a 10% de perda a suavização cruza o piso nominal no sexto
// anúncio, e com nove segundos uma rodada que perdia um anúncio (máquina
// carregada) ficava com o sexto dentro do regime e reprovava: 1 em 7 na
// bateria de 11/11/2026.
const OBSERVACAO: Duration = Duration::from_secs(12);

async fn server() -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
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

struct Par {
    conexao: seele_enlace::Conexao,
    envio: seele_enlace::Envio,
    recebe: seele_enlace::Recebe,
    eu: PersonId,
    ssrc: Ssrc,
    sala: VoiceRoomId,
}

/// Disca `endereco` (o do cano) e faz o aperto de mão à mão.
async fn abrir(endereco: SocketAddr, semente: u8) -> Result<Par> {
    let comum::Ligado {
        ponta,
        conexao,
        mut envio,
        mut recebe,
    } = comum::ligar(endereco).await?;
    // A ponta é dona do socket: tem de viver enquanto a conexão for usada.
    std::mem::forget(ponta);
    let chave = SigningKey::from_bytes(&[semente; 32]);
    frame::write(
        &mut envio,
        &ClientMessage::Hello {
            faixa: seele_proto::FAIXA,
            client: "par-cru".into(),
            nickname: format!("pessoa{semente:03}"),
            public_key: chave.verifying_key().to_bytes().to_vec(),
            join_secret: None,
            material_e2ee: None,
            apis_de_mod: seele_proto::mods::APIS_ACEITAS.to_vec(),
        },
    )
    .await?;
    let ServerMessage::Challenge { nonce } = frame::read::<ServerMessage>(&mut recebe).await?
    else {
        anyhow::bail!("o servidor não mandou Challenge");
    };
    frame::write(
        &mut envio,
        &ClientMessage::Response {
            proof: chave
                .sign(&comum::prova_amarrada(&conexao, &nonce))
                .to_bytes()
                .to_vec(),
        },
    )
    .await?;
    let ServerMessage::Session {
        person,
        ssrc,
        voice_rooms,
        ..
    } = frame::read::<ServerMessage>(&mut recebe).await?
    else {
        anyhow::bail!("o servidor não mandou Session");
    };
    let sala = voice_rooms
        .first()
        .map(|info| info.id)
        .ok_or_else(|| anyhow::anyhow!("o servidor não ofereceu sala de voz nenhuma"))?;
    Ok(Par {
        conexao,
        envio,
        recebe,
        eu: person,
        ssrc,
        sala,
    })
}

fn quadro(ssrc: Ssrc, seq: u16) -> Vec<u8> {
    let cabecalho = MediaHeader {
        ssrc: ssrc.get(),
        seq,
        timestamp: u32::from(seq) * 960,
    };
    let mut fora = vec![0_u8; seele_proto::MAX_DATAGRAM_LEN];
    let tamanho = cabecalho
        .encode_datagram(&PAYLOAD, &mut fora)
        .expect("o quadro cabe num datagrama");
    fora.truncate(tamanho);
    fora
}

/// Um par atrás de um caminho `sentido` (nos dois lados), numa sala, falando
/// ou calado: os sinais que o servidor anunciou **sobre ele**, na ordem.
async fn sinais_sob(sentido: Sentido, falando: bool, semente: u8) -> Result<Vec<u8>> {
    let (endereco, servidor) = server().await?;
    let cano = Cano::abrir(
        endereco,
        ConfigDoCano::simetrico(sentido, u64::from(semente)),
    )
    .await?;
    let mut par = abrir(cano.endereco(), semente).await?;
    frame::write(
        &mut par.envio,
        &ClientMessage::EnterVoiceRoom {
            voice_room: par.sala,
            password: None,
        },
    )
    .await?;

    let fim = tokio::time::Instant::now() + OBSERVACAO;
    let voz = falando.then(|| {
        let conexao = par.conexao.clone();
        let ssrc = par.ssrc;
        tokio::spawn(async move {
            // O ritmo de um microfone: um quadro a cada 20 ms, `seq` corrido.
            // O que falta do outro lado é o que o cano tirou.
            let mut relogio = tokio::time::interval(Duration::from_millis(20));
            let mut seq = 0_u16;
            while tokio::time::Instant::now() < fim {
                relogio.tick().await;
                let _ = conexao.send_datagram(quadro(ssrc, seq));
                seq = seq.wrapping_add(1);
            }
        })
    });

    let mut sinais = Vec::new();
    while tokio::time::Instant::now() < fim {
        match tokio::time::timeout(
            Duration::from_millis(500),
            frame::read::<ServerMessage>(&mut par.recebe),
        )
        .await
        {
            Ok(Ok(ServerMessage::PersonState(estado))) if estado.person == par.eu => {
                sinais.push(estado.signal);
            }
            Ok(Ok(_)) | Err(_) => {}
            Ok(Err(_)) => break,
        }
    }
    if let Some(voz) = voz {
        let _ = voz.await;
    }
    println!(
        "perda {:.0}% atraso {} ms falando={falando}: sinais {sinais:?} (cano: ida perdeu {}, volta perdeu {})",
        sentido.perda * 100.0,
        sentido.atraso.as_millis(),
        cano.ida().perdidos_no_sorteio,
        cano.volta().perdidos_no_sorteio,
    );
    servidor.shutdown();
    Ok(sinais)
}

/// O último terço das amostras: o regime, depois de a média móvel assentar.
fn regime(sinais: &[u8]) -> &[u8] {
    sinais.get(sinais.len() * 2 / 3..).unwrap_or(sinais)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn num_caminho_limpo_o_sinal_fica_alto() -> Result<()> {
    let sinais = sinais_sob(Sentido::limpo(), true, 11).await?;
    assert!(sinais.len() >= 5, "poucos anúncios: {sinais:?}");
    assert!(
        regime(&sinais).iter().all(|s| *s >= 95),
        "caminho limpo em loopback e o sinal não ficou alto: {sinais:?}"
    );
    Ok(())
}

/// O caso de quem não tem mídia RTP nenhuma e está calado — o navegador que só
/// ouve. A única medida que o servidor tem dele é a ida e volta do ICE, e ela
/// basta para o número deixar de ser 100: 100 ms em cada sentido são 200 ms de
/// ida e volta, ~18 pontos de penalidade.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn com_atraso_no_caminho_o_sinal_de_quem_esta_calado_cai() -> Result<()> {
    let sentido = Sentido {
        atraso: Duration::from_millis(100),
        ..Sentido::limpo()
    };
    let sinais = sinais_sob(sentido, false, 12).await?;
    assert!(sinais.len() >= 5, "poucos anúncios: {sinais:?}");
    let regime = regime(&sinais);
    assert!(
        regime
            .iter()
            .all(|s| *s < NOMINAL_FLOOR && *s >= DEGRADED_FLOOR),
        "200 ms de ida e volta deveriam pôr o sinal na faixa degradada (60..85): {sinais:?}"
    );
    Ok(())
}

/// Dez por cento de perda são os 30 pontos inteiros da parcela de perda.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn com_perda_no_caminho_o_sinal_de_quem_fala_cai() -> Result<()> {
    let sentido = Sentido {
        perda: 0.10,
        ..Sentido::limpo()
    };
    let sinais = sinais_sob(sentido, true, 13).await?;
    assert!(sinais.len() >= 5, "poucos anúncios: {sinais:?}");
    assert!(
        regime(&sinais).iter().all(|s| *s < NOMINAL_FLOOR),
        "10% de perda na voz de quem fala e o sinal continuou nominal: {sinais:?}"
    );
    Ok(())
}
