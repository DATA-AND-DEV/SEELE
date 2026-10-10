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
#[derive(Default)]
struct Gravador {
    eventos: std::sync::Mutex<Vec<(Instant, Event)>>,
}

impl seele_ffi::EventListener for Gravador {
    fn on_event(&self, event: Event) {
        if let Ok(mut eventos) = self.eventos.lock() {
            eventos.push((Instant::now(), event));
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
        "MEDIDA-TOOLONG antes={estado_recusada:?} reenviou={reenviou:?} \
         | trajeto={trajeto:?} | fim={fim:?} | sentada_no_servidor={sentada:?} \
         | gravadas_no_canal={gravadas}",
    );
    ana.disconnect();
    servidor.shutdown();
    Ok(())
}
