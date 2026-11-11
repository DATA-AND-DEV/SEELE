//! **O relato de recepção só vai a quem o conhece** (protocolo 13).
//!
//! O protocolo 12 está publicado (a v0.19.6): um servidor dele não conhece o
//! `ClientMessage::RelatoDeRecepcao`. Se um cliente do 13 o escrevesse, o
//! servidor do 12 pularia a variante calado, uma vez por segundo, pela sessão
//! inteira — e é a regra do ADR 0058 que quem manda não manda o que o par não
//! entende.
//!
//! # A cena
//!
//! Um **servidor cru** que carimba a sessão com o número pedido, senta o
//! cliente numa sala e lhe manda voz de verdade (Opus, vinte milissegundos por
//! quadro) **com um quadro faltando a cada dez**: o cliente — uma `Connection`
//! da FFI com o aparelho de mentira — ouve, e o buffer de jitter dele mede uns
//! dez por cento de perda. Tudo o que o cliente escreve no `ctl` é guardado.
//!
//! - carimbado com **13**, o relato chega, e diz perto de dez por cento: é o
//!   que prova que a cena produz relato, e que a outra metade não passa à toa;
//! - carimbado com **12**, com a mesma voz e a mesma perda, **nenhum** relato
//!   é escrito.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use seele_audio::device::mentira;
use seele_ffi::foco::FocoDeVoz;
use seele_ffi::{ConnectConfig, Connection};
use seele_proto::control::{decode, encode_em, ClientMessage, ServerMessage, VoiceRoomInfo};
use seele_proto::ids::{PersonId, SessionId, Ssrc, VoiceRoomId};
use seele_proto::signal::Recepcao;
use seele_proto::MediaHeader;

mod par_cru;
mod vaga;

const PRAZO: Duration = Duration::from_secs(15);

/// O `ssrc` de quem «fala» pelo servidor cru. O do cliente é 1.
const SSRC_DE_QUEM_FALA: u32 = 77;

/// De quantos em quantos quadros um falta: dez por cento de perda.
const UM_FALTA_A_CADA: u16 = 10;

/// O que o cliente escreveu no `ctl`, na ordem.
type Escritos = Arc<Mutex<Vec<ClientMessage>>>;

/// Um servidor cru que carimba tudo com `carimbo`: faz o aperto de mão,
/// oferece uma sala, responde aos `Ping`, e — depois de o cliente pedir a
/// sala — manda voz com um quadro faltando a cada [`UM_FALTA_A_CADA`].
async fn servidor_cru(
    carimbo: u8,
    escritos: Escritos,
) -> Result<(SocketAddr, tokio::task::JoinHandle<()>)> {
    let ponta = par_cru::anfitriao().await;
    let endereco = ponta.local_addr();
    let tarefa = tokio::spawn(async move {
        let Ok((conexao, mut envia, mut recebe)) = par_cru::primeira_chegada(&ponta).await else {
            return;
        };
        let quadro = |mensagem: &ServerMessage| -> Vec<u8> {
            seele_proto::quadro::enquadrar(&encode_em(carimbo, mensagem).expect("codifica"))
                .expect("enquadra")
        };
        let _hello = par_cru::ler_quadro(&mut recebe).await;
        let _ = par_cru::escrever(
            &mut envia,
            &quadro(&ServerMessage::Challenge { nonce: vec![7; 32] }),
        )
        .await;
        let _prova = par_cru::ler_quadro(&mut recebe).await;
        let _ = par_cru::escrever(
            &mut envia,
            &quadro(&ServerMessage::Session {
                id: SessionId(1),
                person: PersonId(1),
                ssrc: Ssrc(1),
                server: "Casa".into(),
                voice_rooms: vec![VoiceRoomInfo {
                    id: VoiceRoomId(1),
                    name: "SALA".into(),
                    limit: 8,
                    password_required: false,
                    channel: None,
                }],
                channels: Vec::new(),
                roles: Vec::new(),
                permissions: Vec::new(),
            }),
        )
        .await;

        // A voz: um tom, codificado como o produto codifica.
        let mut codificador = seele_audio::codec::VoiceEncoder::new(32_000).expect("o codificador");
        let mut fase = 0.0_f32;
        let mut seq = 0_u16;
        let mut sentado = false;
        let mut compasso = tokio::time::interval(Duration::from_millis(20));
        loop {
            tokio::select! {
                lido = par_cru::ler_quadro(&mut recebe) => {
                    let Ok(bytes) = lido else { break };
                    let Ok(mensagem) = decode::<ClientMessage>(&bytes) else { continue };
                    match &mensagem {
                        ClientMessage::Ping { timestamp } => {
                            let pong = quadro(&ServerMessage::Pong { timestamp: *timestamp });
                            let _ = par_cru::escrever(&mut envia, &pong).await;
                        }
                        ClientMessage::EnterVoiceRoom { .. } => sentado = true,
                        _ => {}
                    }
                    escritos.lock().unwrap().push(mensagem);
                }
                _ = compasso.tick(), if sentado => {
                    let pcm: Vec<f32> = (0..seele_audio::FRAME_SAMPLES)
                        .map(|_| {
                            fase += 2.0 * std::f32::consts::PI * 440.0 / 48_000.0;
                            fase.sin() * 0.3
                        })
                        .collect();
                    // Um quadro vazio é o DTX calando, e não um pacote: o `seq`
                    // não anda por ele.
                    let Ok(opus) = codificador.encode(&pcm) else { continue };
                    if opus.is_empty() {
                        continue;
                    }
                    seq = seq.wrapping_add(1);
                    // O quadro existe (o `seq` andou) e não é mandado: do lado
                    // de quem ouve, é um pacote que se perdeu.
                    if seq.is_multiple_of(UM_FALTA_A_CADA) {
                        continue;
                    }
                    let cabecalho = MediaHeader {
                        ssrc: SSRC_DE_QUEM_FALA,
                        seq,
                        timestamp: u32::from(seq) * 960,
                    };
                    let mut datagrama = vec![0_u8; seele_proto::MAX_DATAGRAM_LEN];
                    let Ok(tamanho) = cabecalho.encode_datagram(&opus, &mut datagrama) else {
                        continue;
                    };
                    datagrama.truncate(tamanho);
                    let _ = conexao.send_datagram(datagrama);
                }
            }
        }
        drop(ponta);
    });
    Ok((endereco, tarefa))
}

async fn entrar(endereco: SocketAddr, casa: &std::path::Path, aparelho: &str) -> Arc<Connection> {
    let config = ConnectConfig {
        server: endereco.to_string(),
        alternate_servers: Vec::new(),
        nickname: "ouvinte".into(),
        home: casa.display().to_string(),
        join_secret: None,
        expected_fingerprint: None,
        codigo_esperado: None,
        bilhete: None,
        audio: true,
        capture_device: Some(mentira::id(aparelho)),
        playback_device: Some(mentira::id(aparelho)),
    };
    let (conexao, _) = tokio::task::spawn_blocking(move || {
        Connection::connect_com_foco(config, None, None, Some(FocoDeVoz::novo()), None)
    })
    .await
    .expect("a thread que conecta caiu")
    .unwrap_or_else(|falha| panic!("o cliente não entrou: {falha:?}"));
    conexao
}

async fn ate(prazo: Duration, mut pronto: impl FnMut() -> bool) -> bool {
    let comeco = Instant::now();
    while comeco.elapsed() < prazo {
        if pronto() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    pronto()
}

fn relatos(escritos: &Escritos) -> Vec<Recepcao> {
    escritos
        .lock()
        .unwrap()
        .iter()
        .filter_map(|mensagem| match mensagem {
            ClientMessage::RelatoDeRecepcao(relato) => Some(*relato),
            _ => None,
        })
        .collect()
}

/// O cliente diante de um servidor cru de `carimbo`, sentado e ouvindo voz
/// com dez por cento de perda. Devolve a conexão, o que ela escreveu e quantos
/// blocos com som a saída dela tocou até a voz estar chegando.
async fn ouvindo(carimbo: u8, aparelho: &str) -> Result<(Arc<Connection>, Escritos)> {
    let escritos: Escritos = Arc::default();
    let (endereco, _servidor) = servidor_cru(carimbo, Arc::clone(&escritos)).await?;
    let casa = tempfile::tempdir()?;
    let conexao = entrar(endereco, casa.path(), aparelho).await;
    assert!(
        ate(PRAZO, || conexao.protocolo_negociado() == carimbo).await,
        "a conexão não negociou o {carimbo} com o servidor cru: {}",
        conexao.protocolo_negociado()
    );
    conexao.set_muted(true).expect("mudo");
    let sala = conexao.snapshot().voice_rooms[0].id;
    conexao.enter_voice_room(sala, None).expect("sentar");
    // A voz chega e toca: daqui em diante o cliente **tem** o que relatar.
    assert!(
        ate(PRAZO, || mentira::contagem(aparelho).blocos_com_som >= 100).await,
        "a voz do servidor cru não chegou à saída do cliente (protocolo {carimbo})"
    );
    // A casa só precisa viver até a conexão abrir.
    drop(casa);
    Ok((conexao, escritos))
}

/// **Diante de um servidor do 13, o relato chega, e diz o que foi perdido.**
/// É a metade que prova a cena: a mesma voz, a mesma perda, e o cliente
/// relata.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_um_servidor_do_treze_o_cliente_relata_o_que_ouve() -> Result<()> {
    let _vaga = vaga::minha();
    let (conexao, escritos) = ouvindo(13, "relato-treze").await?;

    // Três relatos: o primeiro pode pegar a janela ainda enchendo.
    assert!(
        ate(PRAZO, || relatos(&escritos).len() >= 3).await,
        "com voz chegando, o cliente não relatou a recepção a um servidor do 13: {:?}",
        relatos(&escritos)
    );
    let relatos = relatos(&escritos);
    println!("relatos a um servidor do 13: {relatos:?}");
    for relato in &relatos {
        assert_eq!(relato.confere(), Ok(()), "{relato:?}");
    }
    let ultimo = relatos.last().expect("há relatos");
    assert!(
        ultimo
            .perda
            .is_some_and(|perda| (0.05..=0.20).contains(&perda)),
        "um quadro faltando a cada dez, e o relato diz {ultimo:?}"
    );
    assert!(
        ultimo.jitter_ms.is_some(),
        "o relato não trouxe o jitter: {ultimo:?}"
    );

    conexao.disconnect();
    Ok(())
}

/// **Diante de um servidor do 12, nada é relatado** — com a mesma voz
/// chegando e a mesma perda sendo medida.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_um_servidor_do_doze_o_cliente_nao_manda_o_relato() -> Result<()> {
    let _vaga = vaga::minha();
    let aparelho = "relato-doze";
    let (conexao, escritos) = ouvindo(12, aparelho).await?;

    // Oito segundos de voz: diante do 13, o primeiro relato sai no terceiro.
    let antes = mentira::contagem(aparelho).blocos_com_som;
    tokio::time::sleep(Duration::from_secs(8)).await;
    let tocados = mentira::contagem(aparelho).blocos_com_som - antes;
    assert!(
        tocados > 400,
        "a voz parou de chegar durante a espera, e o silêncio não provaria nada: {tocados}"
    );
    assert_eq!(conexao.protocolo_negociado(), 12);

    let relatos = relatos(&escritos);
    assert!(
        relatos.is_empty(),
        "o relato de recepção foi escrito para um servidor do 12: {relatos:?}"
    );
    // E o `ctl` seguiu vivo e lido: o cliente escreveu outras coisas.
    let escritos = escritos.lock().unwrap();
    assert!(
        escritos
            .iter()
            .any(|mensagem| matches!(mensagem, ClientMessage::EnterVoiceRoom { .. })),
        "o servidor cru não leu o ctl do cliente: {escritos:?}"
    );

    conexao.disconnect();
    Ok(())
}
