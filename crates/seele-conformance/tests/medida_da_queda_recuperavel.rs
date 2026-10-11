//! A medida da fase A do lote P61 (pendência 61, o lado do cliente): **o que a
//! voz e o TENTAR DE NOVO fazem quando o enlace cai por baixo da tela.**
//!
//! Não é um guarda, e por isso fica fora da bateria (`#[ignore]`), como o
//! instrumento da F1 (`medida_do_barramento.rs`, no ramo `0.16/protocolo`). É o
//! instrumento que o lote pediu **antes** do conserto, e fica no repositório
//! para a medida ser refeita depois dele:
//!
//! ```text
//! cargo test -p seele-conformance --test medida_da_queda_recuperavel -- --ignored --nocapture --test-threads=1
//! ```
//!
//! # As duas medidas
//!
//! - **A voz debaixo da tela de fim** (defeito 4 da F1, que ela leu e não
//!   mediu). A pessoa entra pela FFI, como a janela, com a voz aberta num
//!   **aparelho de mentira** (`seele_audio::device::mentira`): o microfone e a
//!   saída desta máquina nunca abrem. O servidor passa à frente de todo mundo
//!   no barramento (o `FellBehind` encenado: o anel de 1 024 recebe 8 192
//!   eventos de uma vez, sem `await`, o que um lote do escritor maior que o anel
//!   faz), e o instrumento conta o que vem depois do `Ended`: se o aparelho
//!   fecha, se reabre, e se o servidor ouve a voz dela de novo — por uma
//!   segunda pessoa que entra depois e lê o `speaking` dela.
//! - **O TENTAR DE NOVO numa recusada por tamanho** (o achado não verificado da
//!   F1). A pessoa manda um corpo de 6 000 bytes (3 000 `á`), que a FFI recusa
//!   com `TooLong` sem mandar nada, e aperta TENTAR DE NOVO
//!   (`reenviar_mensagem`). O instrumento conta o enlace, o fim, o estado da
//!   pendente e o assento no servidor.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use seele_audio::device::mentira;
use seele_core::{Client, MemoryPinStore};
use seele_ffi::types::{EstadoDoEnvio, Event, LinkState};
use seele_ffi::{ConnectConfig, Connection, VoiceMode};
use seele_proto::control::ServerMessage;
use seele_proto::ids::{ChannelId, PersonId, VoiceRoomId};
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

mod vaga;

const LINHA: ChannelId = ChannelId(1);
const SALA: VoiceRoomId = VoiceRoomId(1);

/// Quanto cada passo de rede tem para acontecer.
const PRAZO: Duration = Duration::from_secs(15);

async fn servidor() -> Result<(SocketAddr, Arc<Daemon>)> {
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

/// O `FellBehind` encenado: o barramento recebe 8 vezes o anel, de uma vez.
///
/// Sem `await` entre os envios, como o escritor anuncia um lote: nenhuma sessão
/// tira evento do anel no meio, e toda sessão viva fica para trás. É o que a
/// F1 mediu com 18 pessoas despejando a rajada inteira, sem as 18 pessoas.
fn passar_todo_mundo_para_tras(servidor: &Daemon) {
    for _ in 0..8 * 1024 {
        let _ = servidor
            .server()
            .events
            .send(seele_server::server::Event::HostUplink { bps: 1 });
    }
}

/// O que a FFI entregou à janela, com a hora.
///
/// E o enlace **como a janela o lê** a cada aviso: a faixa da bateria dura
/// menos que uma amostra de 20 ms quando a primeira tentativa de volta dá
/// certo, e só lendo o retrato no próprio aviso ela aparece.
#[derive(Default)]
struct Gravador {
    eventos: std::sync::Mutex<Vec<(Instant, Event)>>,
    conexao: std::sync::OnceLock<std::sync::Weak<Connection>>,
    enlaces: std::sync::Mutex<Vec<(Instant, String)>>,
    /// Avisado uma vez, no instante em que a janela lê a volta depois da
    /// bateria. Ver `a_pagina_que_o_limitador_descarta`.
    na_volta: std::sync::Mutex<Option<std::sync::mpsc::Sender<()>>>,
}

impl seele_ffi::EventListener for Gravador {
    fn on_event(&self, event: Event) {
        let le_o_retrato = matches!(event, Event::TelemetryChanged | Event::Ended { .. });
        if let Ok(mut eventos) = self.eventos.lock() {
            eventos.push((Instant::now(), event));
        }
        if !le_o_retrato {
            return;
        }
        let Some(conexao) = self.conexao.get().and_then(std::sync::Weak::upgrade) else {
            return;
        };
        let agora = enlace(&conexao);
        if let Ok(mut enlaces) = self.enlaces.lock() {
            let voltou = agora == "Online"
                && enlaces
                    .last()
                    .is_some_and(|(_, ultimo)| ultimo.starts_with("Bateria"));
            if enlaces.last().is_none_or(|(_, ultimo)| *ultimo != agora) {
                enlaces.push((Instant::now(), agora));
            }
            if voltou {
                if let Some(aviso) = self
                    .na_volta
                    .lock()
                    .ok()
                    .and_then(|mut na_volta| na_volta.take())
                {
                    let _ = aviso.send(());
                }
            }
        }
    }
}

impl Gravador {
    fn eventos(&self) -> Vec<(Instant, Event)> {
        self.eventos
            .lock()
            .map(|eventos| eventos.clone())
            .unwrap_or_default()
    }

    /// O enlace como a janela o leu, a partir de `comeco`, em milissegundos.
    fn enlaces_desde(&self, comeco: Instant) -> Vec<(u128, String)> {
        self.enlaces
            .lock()
            .map(|enlaces| {
                enlaces
                    .iter()
                    .filter(|(quando, _)| *quando >= comeco)
                    .map(|(quando, enlace)| {
                        (quando.duration_since(comeco).as_millis(), enlace.clone())
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn config(endereco: SocketAddr, casa: &std::path::Path, aparelho: Option<&str>) -> ConnectConfig {
    ConnectConfig {
        server: endereco.to_string(),
        alternate_servers: Vec::new(),
        nickname: "ana".into(),
        home: casa.display().to_string(),
        join_secret: None,
        expected_fingerprint: None,
        codigo_esperado: None,
        bilhete: None,
        audio: aparelho.is_some(),
        capture_device: aparelho.map(mentira::id),
        playback_device: aparelho.map(mentira::id),
    }
}

async fn entrar_pela_ffi(config: ConnectConfig) -> Result<Arc<Connection>> {
    let (conexao, _) = tokio::task::spawn_blocking(move || Connection::connect(config))
        .await?
        .map_err(|falha| anyhow::anyhow!("a FFI não entrou: {falha:?}"))?;
    Ok(conexao)
}

async fn esperar(o_que: &str, mut feito: impl FnMut() -> bool) -> Result<()> {
    let prazo = Instant::now() + PRAZO;
    while !feito() {
        if Instant::now() > prazo {
            anyhow::bail!("{o_que} não aconteceu em {PRAZO:?}");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Ok(())
}

fn enlace(conexao: &Connection) -> String {
    match conexao.snapshot().link {
        LinkState::Online => "Online".to_owned(),
        LinkState::InternalBattery { attempts, .. } => format!("Bateria({attempts})"),
    }
}

/// Uma segunda pessoa, pelo cliente de verdade, sentada na sala.
async fn bia(endereco: SocketAddr) -> Result<Client> {
    let mut cliente = Client::connect(
        endereco,
        "localhost",
        &endereco.to_string(),
        "bia",
        &ed25519_dalek::SigningKey::from_bytes(&[7; 32]),
        Arc::new(MemoryPinStore::new()),
        None,
        None,
    )
    .await?;
    cliente.enter_voice_room(SALA, None).await?;
    Ok(cliente)
}

/// Quantos `PersonState` de `quem` com `speaking` ligado e desligado `cliente`
/// lê em `janela`.
async fn ouvir_quem_fala(cliente: &mut Client, quem: PersonId, janela: Duration) -> (u32, u32) {
    let fim = Instant::now() + janela;
    let (mut falando, mut calada) = (0, 0);
    while let Ok(Ok(mensagem)) = tokio::time::timeout(
        fim.saturating_duration_since(Instant::now()),
        cliente.next_event(),
    )
    .await
    {
        if let ServerMessage::PersonState(estado) = mensagem {
            if estado.person == quem {
                if estado.speaking {
                    falando += 1;
                } else {
                    calada += 1;
                }
            }
        }
    }
    (falando, calada)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "medida da fase A do lote P61, e não guarda: rodar com --ignored --nocapture"]
async fn a_voz_debaixo_da_tela_de_fim() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor) = servidor().await?;
    let aparelho = "p61-voz-debaixo-do-fim";
    let casa = tempfile::tempdir()?;
    let gravador = Arc::new(Gravador::default());
    let ana = entrar_pela_ffi(config(endereco, casa.path(), Some(aparelho))).await?;
    let _ = gravador.conexao.set(Arc::downgrade(&ana));
    ana.subscribe(gravador.clone());
    // Sempre aberto: o que se mede é se a voz **sai**, e não o portão.
    ana.set_voice_mode(VoiceMode::Open);
    ana.enter_voice_room(SALA.0, None)
        .map_err(|erro| anyhow::anyhow!("{erro:?}"))?;
    esperar("a voz abrir no aparelho de mentira", || {
        mentira::contagem(aparelho).abertos == 1
            && ana.snapshot().abertura_do_audio == seele_ffi::AberturaDoAudio::Aberto
    })
    .await?;
    let quem = PersonId(ana.snapshot().me.expect("ela sabe quem é"));
    // Antes da queda, o servidor ouve a voz dela: o instrumento mede o que
    // mede, e não um aparelho que nunca transmitiu.
    let mut antes = bia(endereco).await?;
    let ouvida_antes = ouvir_quem_fala(&mut antes, quem, Duration::from_millis(2500)).await;
    drop(antes);
    tokio::time::sleep(Duration::from_millis(500)).await;

    let aberturas_antes = mentira::contagem(aparelho).aberturas;
    let comeco = Instant::now();
    passar_todo_mundo_para_tras(&servidor);

    // De 20 em 20 ms, por 6 s: o enlace, a abertura do áudio e o aparelho.
    let mut trajeto: Vec<(u128, String)> = Vec::new();
    while comeco.elapsed() < Duration::from_secs(6) {
        let contagem = mentira::contagem(aparelho);
        let agora = format!(
            "{} {:?} aparelho(abertos={}, aberturas={})",
            enlace(&ana),
            ana.snapshot().abertura_do_audio,
            contagem.abertos,
            contagem.aberturas - aberturas_antes,
        );
        if trajeto.last().is_none_or(|(_, ultimo)| *ultimo != agora) {
            trajeto.push((comeco.elapsed().as_millis(), agora));
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let eventos = gravador.eventos();
    let fim = eventos
        .iter()
        .position(|(_, evento)| matches!(evento, Event::Ended { .. }));
    let qual_fim = fim
        .and_then(|indice| eventos.get(indice))
        .map(|(quando, evento)| format!("{evento:?} aos {:?}", quando.duration_since(comeco)));
    let depois_do_fim = fim.map_or(0, |indice| eventos.len() - indice - 1);
    let sentada = servidor.server().occupancy.lock().await.onde_esta(quem);

    // Quem entra agora ouve a voz dela? É a pergunta inteira: debaixo de uma
    // tela que diz «encerrado», o servidor recebe a voz desta pessoa.
    let mut depois = bia(endereco).await?;
    let ouvida_depois = ouvir_quem_fala(&mut depois, quem, Duration::from_millis(2500)).await;

    println!(
        "MEDIDA-VOZ enlace_lido_pela_janela={:?}",
        gravador.enlaces_desde(comeco)
    );
    println!(
        "MEDIDA-VOZ fim={qual_fim:?} eventos_depois_do_fim={depois_do_fim} \
         | antes_da_queda falando/calada={ouvida_antes:?} \
         | trajeto={trajeto:?} \
         | sentada_no_servidor={sentada:?} \
         | depois_do_fim falando/calada={ouvida_depois:?} \
         | aparelho_no_fim={:?}",
        mentira::contagem(aparelho),
    );
    ana.disconnect();
    drop(ana);
    tokio::time::sleep(Duration::from_millis(500)).await;
    println!(
        "MEDIDA-VOZ depois_do_disconnect aparelho={:?}",
        mentira::contagem(aparelho)
    );
    servidor.shutdown();
    Ok(())
}

fn estado_da(conexao: &Connection, chave: &str) -> Option<EstadoDoEnvio> {
    conexao
        .messages()
        .into_iter()
        .find(|mensagem| mensagem.client_message_id.as_deref() == Some(chave))
        .map(|mensagem| mensagem.estado)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "medida da fase A do lote P61, e não guarda: rodar com --ignored --nocapture"]
async fn tentar_de_novo_uma_recusada_por_tamanho() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor) = servidor().await?;
    let casa = tempfile::tempdir()?;
    let gravador = Arc::new(Gravador::default());
    let ana = entrar_pela_ffi(config(endereco, casa.path(), None)).await?;
    let _ = gravador.conexao.set(Arc::downgrade(&ana));
    ana.subscribe(gravador.clone());
    ana.enter_voice_room(SALA.0, None)
        .map_err(|erro| anyhow::anyhow!("{erro:?}"))?;
    ana.open_channel(LINHA.0)
        .map_err(|erro| anyhow::anyhow!("{erro:?}"))?;
    esperar("ela saber quem é", || ana.snapshot().me.is_some()).await?;
    let quem = PersonId(ana.snapshot().me.expect("ela sabe quem é"));
    tokio::time::sleep(Duration::from_millis(500)).await;

    // 3 000 `á` são 6 000 bytes: o campo da janela os deixa passar (conta
    // unidades de texto), e o teto do fio é de bytes.
    let corpo = "á".repeat(3000);
    let chave = ana
        .send_message(LINHA.0, corpo)
        .map_err(|erro| anyhow::anyhow!("{erro:?}"))?;
    let estado_recusada = estado_da(&ana, &chave);

    let comeco = Instant::now();
    let reenviou = ana
        .reenviar_mensagem(&chave)
        .map_err(|erro| anyhow::anyhow!("{erro:?}"));
    let mut trajeto: Vec<(u128, String)> = Vec::new();
    while comeco.elapsed() < Duration::from_secs(8) {
        let agora = format!("{} {:?}", enlace(&ana), estado_da(&ana, &chave));
        if trajeto.last().is_none_or(|(_, ultimo)| *ultimo != agora) {
            trajeto.push((comeco.elapsed().as_millis(), agora));
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let fim = gravador
        .eventos()
        .into_iter()
        .find(|(_, evento)| matches!(evento, Event::Ended { .. }))
        .map(|(quando, evento)| format!("{evento:?} aos {:?}", quando.duration_since(comeco)));
    let sentada = servidor.server().occupancy.lock().await.onde_esta(quem);
    let gravadas = servidor.quantas_mensagens(LINHA).await?;

    println!(
        "MEDIDA-TOOLONG enlace_lido_pela_janela={:?}",
        gravador.enlaces_desde(comeco)
    );
    println!(
        "MEDIDA-TOOLONG antes={estado_recusada:?} reenviou={reenviou:?} \
         | trajeto={trajeto:?} | fim={fim:?} | sentada_no_servidor={sentada:?} \
         | gravadas_no_canal={gravadas}",
    );
    ana.disconnect();
    servidor.shutdown();
    Ok(())
}

/// **A página que o limitador do servidor descarta** (I2 da revisão do P61): o
/// que acontece com a busca do buraco quando o balde da conexão nova acaba no
/// meio dela.
///
/// No instante em que a janela lê a volta, outra linha manda 80 `SetMuted`
/// pela FFI: o balde de 60 quadros se esgota, e o que a busca pede logo depois
/// — o `FetchHistory` e o `Ping` da cerca da página seguinte — cai no
/// `Descarta` do `Vigia`. Não dá para escolher qual dos dois cai: com o balde
/// vazio, os dois caem juntos, e é o caso da cerca perdida. O caso do pedido
/// descartado com a cerca passando está provado no crate
/// (`volta::tests::o_pedido_descartado_e_pedido_de_novo_e_o_buraco_fecha_inteiro`).
///
/// Mede quantas do buraco de 1 500 estão na tela ao longo de 12 s.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "medida da rodada 1 do lote P61, e não guarda: rodar com --ignored --nocapture"]
async fn a_pagina_que_o_limitador_descarta() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor) = servidor().await?;
    let casa = tempfile::tempdir()?;
    let gravador = Arc::new(Gravador::default());
    let ana = entrar_pela_ffi(config(endereco, casa.path(), None)).await?;
    let _ = gravador.conexao.set(Arc::downgrade(&ana));
    ana.subscribe(gravador.clone());
    ana.enter_voice_room(SALA.0, None)
        .map_err(|erro| anyhow::anyhow!("{erro:?}"))?;
    ana.open_channel(LINHA.0)
        .map_err(|erro| anyhow::anyhow!("{erro:?}"))?;
    ana.send_message(LINHA.0, "estou aqui".into())
        .map_err(|erro| anyhow::anyhow!("{erro:?}"))?;
    esperar("a primeira mensagem dela ser confirmada", || {
        ana.messages()
            .iter()
            .any(|m| m.body == "estou aqui" && m.estado == EstadoDoEnvio::Confirmada)
    })
    .await?;
    let quem = PersonId(ana.snapshot().me.expect("ela sabe quem é"));

    // O buraco: 1 500 de outra pessoa, gravadas direto no banco.
    let autor = bia(endereco).await?.session().person;
    {
        let mut banco = servidor.server().persistence.lock().await;
        let mut mensagens = seele_server::persistence::messages::Messages::new(&mut banco);
        let buraco: Vec<seele_server::persistence::messages::PendingMessage> = (1..=1_500)
            .map(
                |numero| seele_server::persistence::messages::PendingMessage {
                    channel: LINHA,
                    author: autor,
                    author_nickname: "bia".into(),
                    body: format!("buraco-{numero:04}"),
                    replies_to: None,
                    client_message_id: None,
                },
            )
            .collect();
        mensagens.append_batch(&buraco)?;
    }

    // A linha que esgota o balde, no instante da volta.
    let (aviso, na_volta) = std::sync::mpsc::channel();
    if let Ok(mut vaga) = gravador.na_volta.lock() {
        *vaga = Some(aviso);
    }
    let esgotando = {
        let ana = Arc::clone(&ana);
        std::thread::spawn(move || {
            if na_volta.recv_timeout(Duration::from_secs(20)).is_ok() {
                for numero in 0..80 {
                    let _ = ana.set_muted(numero % 2 == 0);
                }
            }
        })
    };

    let comeco = Instant::now();
    servidor
        .server()
        .events
        .send(seele_server::server::Event::SessionEnded {
            person: quem,
            reason: seele_proto::control::DisconnectReason::FellBehind,
        })
        .map_err(|_| anyhow::anyhow!("o barramento caiu"))?;
    let mut trajeto: Vec<(u128, usize)> = Vec::new();
    while comeco.elapsed() < Duration::from_secs(12) {
        let na_tela = ana
            .messages()
            .iter()
            .filter(|m| m.body.starts_with("buraco-"))
            .count();
        if trajeto.last().is_none_or(|(_, antes)| *antes != na_tela) {
            trajeto.push((comeco.elapsed().as_millis(), na_tela));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let _ = esgotando.join();
    println!(
        "MEDIDA-LIMITADOR enlace_lido_pela_janela={:?}",
        gravador.enlaces_desde(comeco)
    );
    println!(
        "MEDIDA-LIMITADOR buraco_na_tela(ms, quantas de 1500)={trajeto:?} | fim={:?}",
        gravador
            .eventos()
            .iter()
            .find(|(_, evento)| matches!(evento, Event::Ended { .. }))
            .map(|(_, evento)| evento.clone())
    );
    ana.disconnect();
    servidor.shutdown();
    Ok(())
}
