//! **A sessão pelo enlace da 0.17**: o que `tests/alpn.rs` e
//! `tests/sessao_por_h3.rs` provavam sobre o QUIC e a WebTransport, no
//! transporte que os substituiu.
//!
//! Os dois arquivos saíram com o QUIC (contrato da fase 2, D1): o ALPN, o
//! certificado curto do `h3`, as janelas, os hashes e a recusa contada do
//! navegador não existem mais. O que eles garantiam e continua valendo:
//!
//! - **o `a=fingerprint` do DTLS é o `fp=` do convite**, e quem liga confere
//!   antes do `Hello` (era «o nativo recebe o longo»);
//! - **uma impressão diferente é recusada no DTLS**, antes de qualquer canal
//!   (era «o navegador que recusa o certificado é contado»);
//! - **a voz atravessa entre duas sessões** pelo canal `voz` (era «a voz
//!   atravessa do navegador ao nativo»: os dois chegam agora pela mesma
//!   `Conexao`, e a página espelha o enlace pelo `rtc-web.js`);
//! - **o link do navegador é o convite**, sem `wh=` nem `ate=`, com o `v=2.0`.

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
use seele_enlace::{Discagem, FalhaAoDiscar, Pino};
use seele_proto::control::{ClientMessage, ServerMessage};
use seele_proto::ids::{Ssrc, VoiceRoomId};
use seele_proto::media::MediaHeader;
use seele_server::persistence::Location;
use seele_server::{frame, Daemon, ServerConfig};

mod comum;

async fn servidor() -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
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

fn impressao_de(hex: &str) -> [u8; 32] {
    let mut bytes = [0_u8; 32];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap();
    }
    bytes
}

/// O aperto de mão inteiro: devolve o `ssrc` e a primeira sala.
async fn apertar(ligado: &mut comum::Ligado, semente: u8) -> Result<(Ssrc, VoiceRoomId)> {
    let chave = SigningKey::from_bytes(&[semente; 32]);
    frame::write(
        &mut ligado.envio,
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
    let ServerMessage::Challenge { nonce } =
        frame::read::<ServerMessage>(&mut ligado.recebe).await?
    else {
        anyhow::bail!("o servidor não mandou Challenge");
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
    let ServerMessage::Session {
        ssrc, voice_rooms, ..
    } = frame::read::<ServerMessage>(&mut ligado.recebe).await?
    else {
        anyhow::bail!("o servidor não mandou Session");
    };
    Ok((
        ssrc,
        voice_rooms.first().map(|info| info.id).expect("uma sala"),
    ))
}

/// Um quadro de voz com o `ssrc` de quem fala.
fn quadro(ssrc: Ssrc, seq: u16) -> Vec<u8> {
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

/// **O `a=fingerprint` é o `fp=`**: quem liga com o pino do convite entra, e o
/// certificado que o DTLS mostrou é o da impressão anunciada.
#[tokio::test(flavor = "multi_thread")]
async fn quem_liga_com_o_fp_do_convite_entra_e_o_dtls_e_o_do_convite() -> Result<()> {
    let (endereco, daemon) = servidor().await?;
    let fp = impressao_de(daemon.fingerprint());
    let ponta = comum::ponta_para(endereco).await?;
    let conexao = ponta
        .discar(Discagem::nova(vec![endereco], Pino::Impressao(fp)))
        .await?;
    assert_eq!(conexao.impressao_dtls_do_par(), fp);
    let (envio, recebe) = conexao.controle().expect("ctl");
    let mut ligado = comum::Ligado {
        ponta,
        conexao,
        envio,
        recebe,
    };
    apertar(&mut ligado, 3).await?;
    frame::write(&mut ligado.envio, &ClientMessage::Ping { timestamp: 4242 }).await?;
    let voltou = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let ServerMessage::Pong { timestamp: 4242 } =
                frame::read::<ServerMessage>(&mut ligado.recebe).await?
            {
                return Ok::<_, anyhow::Error>(true);
            }
        }
    })
    .await??;
    assert!(voltou, "a sessão pelo enlace não respondeu ao Ping");
    daemon.shutdown();
    Ok(())
}

/// **Uma impressão diferente cai no DTLS**, antes de qualquer canal, e o
/// anfitrião não fica com par nenhum vivo dela.
#[tokio::test(flavor = "multi_thread")]
async fn uma_impressao_diferente_e_recusada_no_dtls_antes_de_qualquer_canal() -> Result<()> {
    let (endereco, daemon) = servidor().await?;
    let ponta = comum::ponta_para(endereco).await?;
    let mut errada = impressao_de(daemon.fingerprint());
    errada[0] ^= 0xFF;
    let mut discagem = Discagem::nova(vec![endereco], Pino::Impressao(errada));
    discagem.prazo = Duration::from_secs(5);
    let falha = ponta
        .discar(discagem)
        .await
        .expect_err("a impressão errada entrou");
    assert!(
        matches!(falha, FalhaAoDiscar::ImpressaoDiferente { .. }),
        "{falha:?}"
    );
    // O anfitrião larga o par dentro do prazo do aperto (o nativo que recusa
    // não manda alerta, e o par pendente sai pelo prazo do enlace), e nenhuma
    // sessão chegou a nascer dele.
    let ate = tokio::time::Instant::now() + seele_enlace::APERTO + Duration::from_secs(2);
    while daemon.ponta().conexoes_vivas() > 0 && tokio::time::Instant::now() < ate {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(
        daemon.ponta().conexoes_vivas(),
        0,
        "o anfitrião segurou o par da impressão errada além do prazo do aperto"
    );
    daemon.shutdown();
    Ok(())
}

/// **A voz atravessa entre duas sessões** pelo canal `voz` do enlace, nos dois
/// sentidos.
#[tokio::test(flavor = "multi_thread")]
async fn a_voz_atravessa_entre_duas_sessoes_nos_dois_sentidos() -> Result<()> {
    let (endereco, daemon) = servidor().await?;
    let mut um = comum::ligar(endereco).await?;
    let (ssrc_um, sala) = apertar(&mut um, 1).await?;
    let mut dois = comum::ligar(endereco).await?;
    let (ssrc_dois, sala_dois) = apertar(&mut dois, 2).await?;
    assert_eq!(sala, sala_dois);

    for ligado in [&mut um, &mut dois] {
        frame::write(
            &mut ligado.envio,
            &ClientMessage::EnterVoiceRoom {
                voice_room: sala,
                password: None,
            },
        )
        .await?;
    }
    // O controle dos dois é drenado: a sessão não pode travar no canal cheio.
    let comum::Ligado {
        ponta: _p1,
        conexao: c1,
        envio: _e1,
        recebe: mut r1,
    } = um;
    let comum::Ligado {
        ponta: _p2,
        conexao: c2,
        envio: _e2,
        recebe: mut r2,
    } = dois;
    tokio::spawn(async move { while frame::read::<ServerMessage>(&mut r1).await.is_ok() {} });
    tokio::spawn(async move { while frame::read::<ServerMessage>(&mut r2).await.is_ok() {} });
    tokio::time::sleep(Duration::from_millis(500)).await;

    for (de, para, ssrc) in [(&c1, &c2, ssrc_um), (&c2, &c1, ssrc_dois)] {
        let ouvinte = para.clone();
        let ouviu = tokio::spawn(async move {
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let bytes = ouvinte.read_datagram().await?;
                    if let Ok((cabecalho, _)) = MediaHeader::decode(&bytes) {
                        if cabecalho.ssrc == ssrc.get() {
                            return Ok::<_, anyhow::Error>(true);
                        }
                    }
                }
            })
            .await
        });
        for seq in 0..25_u16 {
            let _ = de.send_datagram(quadro(ssrc, seq));
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(
            matches!(ouviu.await?, Ok(Ok(true))),
            "a voz de {ssrc:?} não atravessou"
        );
    }
    daemon.shutdown();
    Ok(())
}

/// **O link do navegador é o convite**: o mesmo alvo, os mesmos
/// alternativos, a mesma impressão e o mesmo token, com o `v=2.0` e sem os
/// `wh=`/`ate=` da 0.16. E a impressão que ele leva é a que o DTLS mostra.
#[tokio::test]
async fn o_link_do_navegador_e_o_convite_sem_wh_nem_ate() {
    use seele_proto::uri::{analisar, analisar_no_navegador, ORIGEM_DO_NAVEGADOR};
    use seele_server::hospedagem::{Hospedagem, SemLinkDoNavegador};

    std::env::set_var(seele_server::alcance::encontro::VARIAVEL, "nao");
    let hospedagem = Hospedagem::iniciar(0, Location::Memory, "Casa", None, |_| Ok(()))
        .await
        .unwrap();
    let token = {
        let banco = hospedagem.persistence();
        let mut persistence = banco.lock().await;
        seele_server::admissao::criar_convite(&mut persistence, "para o navegador").unwrap()
    };
    let convite = hospedagem.convite_com_token(&token);
    let link = hospedagem.link_do_navegador(&convite).unwrap();
    assert!(
        !link.link.contains("wh=") && !link.link.contains("ate="),
        "{}",
        link.link
    );
    let lido = analisar_no_navegador(&link.link, ORIGEM_DO_NAVEGADOR).unwrap();
    let original = analisar(&convite).unwrap();
    assert_eq!(lido.convite.alvo, original.alvo);
    assert_eq!(lido.convite.alternativos, original.alternativos);
    assert_eq!(lido.convite.impressao_digital, original.impressao_digital);
    assert_eq!(lido.convite.token, Some(token));
    assert_eq!(
        lido.convite.versao.as_deref(),
        Some(seele_proto::version::VERSAO_DO_SERVIDOR)
    );

    // A impressão do link abre o DTLS do servidor.
    let fp = impressao_de(lido.convite.impressao_digital.as_deref().unwrap());
    let alvo = SocketAddr::from(([127, 0, 0, 1], hospedagem.endereco().port()));
    let ponta = comum::ponta_para(alvo).await.unwrap();
    let conexao = ponta
        .discar(Discagem::nova(vec![alvo], Pino::Impressao(fp)))
        .await
        .expect("o fp do link não abre o DTLS deste servidor");
    drop(conexao);

    let de_outro = convite.replace(
        original.impressao_digital.as_deref().unwrap(),
        &"ab".repeat(32),
    );
    assert_eq!(
        hospedagem.link_do_navegador(&de_outro),
        Err(SemLinkDoNavegador::OutroServidor)
    );
    assert_eq!(
        hospedagem.link_do_navegador("um texto qualquer"),
        Err(SemLinkDoNavegador::ConviteInvalido)
    );
    hospedagem.encerrar().await;
}

/// A sala pessoal é achada pelo código, e não dá link do navegador.
#[tokio::test]
async fn a_sala_pessoal_nao_da_link_do_navegador() {
    use seele_server::hospedagem::{Hospedagem, SemLinkDoNavegador};

    let sala = Hospedagem::iniciar_sala(0, &[7; 32], "dona", None)
        .await
        .unwrap();
    assert_eq!(
        sala.link_do_navegador(&sala.convite()),
        Err(SemLinkDoNavegador::SalaPessoal)
    );
    sala.encerrar().await;
}
