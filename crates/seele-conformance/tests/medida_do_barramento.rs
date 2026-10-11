//! A medida da pendência 61: **o que é preciso, com o cliente de verdade, para
//! uma sessão cair com `FellBehind`.**
//!
//! Não é um guarda, e por isso fica fora da bateria (`#[ignore]`). É o
//! instrumento que a pendência pediu antes de alguém decidir entre defeito e
//! contrato, e fica no repositório para a medida poder ser refeita depois de
//! qualquer conserto:
//!
//! ```text
//! cargo test -p seele-conformance --test medida_do_barramento -- --ignored --nocapture
//! ```
//!
//! Cada rodada sobe um servidor próprio, conecta `N` falantes e um ouvinte pelo
//! `seele_core::Client`, solta as rajadas dos falantes juntas e conta: quanto o
//! servidor gravou, quantas sessões o barramento passou à frente, quem recebeu
//! `Disconnecting(FellBehind)`, e em que lotes o ouvinte viu as mensagens
//! chegarem (pela folga entre as chegadas: o escritor grava a cada 200 ms).

#![allow(clippy::expect_used, reason = "num teste, o pânico é o relatório")]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use seele_core::{Client, MemoryPinStore};
use seele_proto::control::{DisconnectReason, ServerMessage};
use seele_proto::ids::{ChannelId, ClientMessageId, VoiceRoomId};
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

mod vaga;

const LINHA: ChannelId = ChannelId(1);

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

async fn entrar(endereco: SocketAddr, numero: usize) -> Result<Client> {
    let semente = u8::try_from(numero + 1).expect("cabe num byte");
    let mut cliente = Client::connect(
        endereco,
        "localhost",
        &endereco.to_string(),
        &format!("pessoa{numero:02}"),
        &ed25519_dalek::SigningKey::from_bytes(&[semente; 32]),
        Arc::new(MemoryPinStore::new()),
        None,
        None,
    )
    .await?;
    cliente.enter_voice_room(VoiceRoomId(1), None).await?;
    cliente.join_channel(LINHA).await?;
    Ok(cliente)
}

/// O que uma ponta viu.
#[derive(Debug, Default)]
struct Visto {
    /// Quantas `MessageReceived` leu.
    lidas: usize,
    /// Se o servidor se despediu dela com `FellBehind`.
    ficou_para_tras: bool,
    /// Quando cada mensagem chegou, para ver os lotes.
    chegadas: Vec<Instant>,
}

/// Lê até ter `esperadas` mensagens, a despedida, o fluxo fechar ou o prazo.
async fn ler(cliente: &mut Client, esperadas: usize, prazo: Instant) -> Visto {
    let mut visto = Visto::default();
    while visto.lidas < esperadas {
        let resta = prazo.saturating_duration_since(Instant::now());
        match tokio::time::timeout(resta, cliente.next_event()).await {
            Ok(Ok(ServerMessage::MessageReceived { .. })) => {
                visto.lidas += 1;
                visto.chegadas.push(Instant::now());
            }
            Ok(Ok(ServerMessage::Disconnecting { reason })) => {
                visto.ficou_para_tras = reason == DisconnectReason::FellBehind;
                break;
            }
            Ok(Ok(_)) => {}
            Ok(Err(_)) | Err(_) => break,
        }
    }
    visto
}

/// Os tamanhos dos lotes, pela folga entre chegadas: mais de 100 ms entre duas
/// mensagens é lote novo (o escritor grava a cada 200 ms).
fn lotes(chegadas: &[Instant]) -> Vec<usize> {
    let mut lotes = Vec::new();
    let mut atual = usize::from(!chegadas.is_empty());
    for par in chegadas.windows(2) {
        if let [antes, depois] = par {
            if depois.duration_since(*antes) > Duration::from_millis(100) {
                lotes.push(atual);
                atual = 0;
            }
        }
        atual += 1;
    }
    if atual > 0 {
        lotes.push(atual);
    }
    lotes
}

/// Uma rodada: `falantes` pessoas mandam `por_falante` mensagens de
/// `tamanho` bytes, todas juntas, `rajadas` vezes, com `intervalo` entre as
/// rajadas de cada uma.
async fn rodada(
    falantes: usize,
    por_falante: usize,
    tamanho: usize,
    rajadas: usize,
    intervalo: Duration,
) -> Result<()> {
    let (endereco, servidor) = servidor().await?;
    let mut ouvinte = entrar(endereco, 0).await?;
    let mut clientes = Vec::new();
    for numero in 1..=falantes {
        clientes.push(entrar(endereco, numero).await?);
    }
    let total = falantes * por_falante * rajadas;
    let largada = Arc::new(tokio::sync::Barrier::new(falantes + 1));
    let prazo = Instant::now() + Duration::from_secs(8) + intervalo * u32::try_from(rajadas)?;

    let mut tarefas = Vec::new();
    for (indice, mut cliente) in clientes.into_iter().enumerate() {
        let largada = Arc::clone(&largada);
        tarefas.push(tokio::spawn(async move {
            largada.wait().await;
            let mut chave = 0_u64;
            for rajada in 0..rajadas {
                if rajada > 0 {
                    tokio::time::sleep(intervalo).await;
                }
                for _ in 0..por_falante {
                    chave += 1;
                    let corpo = format!("{indice:02}-{chave:04}-")
                        .chars()
                        .cycle()
                        .take(tamanho)
                        .collect::<String>();
                    if cliente
                        .send_message(LINHA, &corpo, ClientMessageId(chave))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
            ler(&mut cliente, total, prazo).await
        }));
    }
    largada.wait().await;
    let comeco = Instant::now();
    let do_ouvinte = ler(&mut ouvinte, total, prazo).await;
    let mut dos_falantes = Vec::new();
    for tarefa in tarefas {
        dos_falantes.push(tarefa.await?);
    }
    // O escritor pode ter um lote no meio quando o ouvinte desiste.
    tokio::time::sleep(Duration::from_millis(400)).await;

    let gravadas = servidor.quantas_mensagens(LINHA).await?;
    let atrasos = &servidor.server().atrasos;
    let falantes_atras = dos_falantes
        .iter()
        .filter(|visto| visto.ficou_para_tras)
        .count();
    let lotes_vistos = lotes(&do_ouvinte.chegadas);
    let ultimo = do_ouvinte
        .chegadas
        .last()
        .map_or(Duration::ZERO, |chegada| chegada.duration_since(comeco));
    println!(
        "MEDIDA falantes={falantes:>2} por_falante={por_falante:>2} corpo={tamanho:>4} \
         rajadas={rajadas} | gravadas={gravadas:>5}/{total:<5} | sessoes_atras={} eventos_perdidos={} \
         | falantes_FellBehind={falantes_atras:>2} ouvinte_FellBehind={} ouvinte_leu={:>5} \
         | lotes_vistos={lotes_vistos:?} em {ultimo:.0?}",
        atrasos.sessoes(),
        atrasos.eventos(),
        do_ouvinte.ficou_para_tras,
        do_ouvinte.lidas,
    );
    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "medida da pendência 61, e não guarda: rodar com --ignored --nocapture"]
async fn rajadas_juntas_de_varias_pessoas() -> Result<()> {
    let _vaga = vaga::minha();
    // A rajada inteira que o balde de quadros deixa (60, menos o que sobra),
    // de cada vez mais gente ao mesmo tempo.
    for falantes in [8, 12, 16, 17, 18, 20, 24] {
        rodada(falantes, 58, 20, 1, Duration::ZERO).await?;
    }
    // A mesma conta com corpos de 4 KiB, o teto do fio.
    for falantes in [16, 18, 24] {
        rodada(falantes, 58, 4000, 1, Duration::ZERO).await?;
    }
    Ok(())
}

/// O que a FFI entregou à janela, com a hora.
#[derive(Default)]
struct Gravador {
    eventos: std::sync::Mutex<Vec<(Instant, seele_ffi::types::Event)>>,
}

impl seele_ffi::EventListener for Gravador {
    fn on_event(&self, event: seele_ffi::types::Event) {
        if let Ok(mut eventos) = self.eventos.lock() {
            eventos.push((Instant::now(), event));
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "medida da pendência 61, e não guarda: rodar com --ignored --nocapture"]
async fn o_que_a_pessoa_ve_quando_o_barramento_passa_a_frente_dela() -> Result<()> {
    let _vaga = vaga::minha();
    // Junto da rajada: o que ela mandou já foi lido quando o lote passa.
    for falantes in [18, 24] {
        a_pessoa_pela_ffi(falantes, Duration::ZERO, 58).await?;
    }
    // Atrasada: o que ela manda está no ar quando o lote passa à frente dela.
    for atraso in [150, 200, 250] {
        a_pessoa_pela_ffi(24, Duration::from_millis(atraso), 20).await?;
    }
    Ok(())
}

fn config_da_ana(endereco: SocketAddr, casa: &std::path::Path) -> seele_ffi::ConnectConfig {
    seele_ffi::ConnectConfig {
        server: endereco.to_string(),
        alternate_servers: Vec::new(),
        nickname: "ana".into(),
        home: casa.display().to_string(),
        join_secret: None,
        expected_fingerprint: None,
        codigo_esperado: None,
        bilhete: None,
        audio: false,
        capture_device: None,
        playback_device: None,
    }
}

/// Os envios dela, pelo estado em que a janela os desenha.
fn estados_dela(ana: &seele_ffi::Connection) -> std::collections::BTreeMap<String, usize> {
    use seele_ffi::types::EstadoDoEnvio;
    let mut estados = std::collections::BTreeMap::<String, usize>::new();
    for mensagem in ana.messages().iter().filter(|mensagem| mensagem.own) {
        let estado = match &mensagem.estado {
            EstadoDoEnvio::Confirmada => "Confirmada".to_owned(),
            EstadoDoEnvio::Enviando => "Enviando".to_owned(),
            EstadoDoEnvio::SemResposta => "SemResposta".to_owned(),
            EstadoDoEnvio::Recusada(motivo) => format!("Recusada({motivo:?})"),
        };
        *estados.entry(estado).or_default() += 1;
    }
    estados
}

/// Uma pessoa pela FFI, no canal 1, mandando `quantas` mensagens `atraso`
/// depois de `falantes` mandarem a rajada inteira num canal onde ela não está.
/// Depois, o RECONECTAR da tela de fim: a conexão velha sai, e uma nova entra
/// com a mesma casa.
async fn a_pessoa_pela_ffi(falantes: usize, atraso: Duration, quantas: usize) -> Result<()> {
    use seele_ffi::types::{Event, LinkState};

    let (endereco, servidor) = servidor().await?;
    // A rajada é num canal **onde ela não está**: o barramento é um só.
    let outra = {
        let db = servidor.server().persistence.lock().await;
        seele_server::persistence::channels::Channels::new(&db)
            .create_channel("outra")?
            .id
    };

    let casa = tempfile::tempdir()?;
    let gravador = Arc::new(Gravador::default());
    let config = config_da_ana(endereco, casa.path());
    let (ana, _) = tokio::task::spawn_blocking(move || seele_ffi::Connection::connect(config))
        .await?
        .map_err(|falha| anyhow::anyhow!("a FFI não entrou: {falha:?}"))?;
    // Inscrito como a janela se inscreve (`Bridge`, em `main.rs`).
    ana.subscribe(gravador.clone());
    ana.enter_voice_room(1, None)
        .map_err(|erro| anyhow::anyhow!("{erro:?}"))?;
    ana.open_channel(LINHA.get())
        .map_err(|erro| anyhow::anyhow!("{erro:?}"))?;
    let quem = seele_proto::ids::PersonId(loop {
        if let Some(eu) = ana.snapshot().me {
            break eu;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    });
    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut clientes = Vec::new();
    for numero in 1..=falantes {
        let mut cliente = entrar(endereco, numero).await?;
        cliente.join_channel(outra).await?;
        clientes.push(cliente);
    }
    let largada = Arc::new(tokio::sync::Barrier::new(falantes + 2));
    let mut tarefas = Vec::new();
    for (indice, mut cliente) in clientes.into_iter().enumerate() {
        let largada = Arc::clone(&largada);
        tarefas.push(tokio::spawn(async move {
            largada.wait().await;
            for chave in 1..=58_u64 {
                let corpo = format!("{indice:02}-{chave:04}");
                if cliente
                    .send_message(outra, &corpo, ClientMessageId(chave))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            let prazo = Instant::now() + Duration::from_secs(4);
            ler(&mut cliente, usize::MAX, prazo).await
        }));
    }
    // Ela escreve pela mesma porta que a janela usa.
    let escrevendo = {
        let ana = Arc::clone(&ana);
        let largada = Arc::clone(&largada);
        tokio::spawn(async move {
            largada.wait().await;
            tokio::time::sleep(atraso).await;
            tokio::task::spawn_blocking(move || {
                (1..=quantas)
                    .filter(|numero| {
                        ana.send_message(LINHA.get(), format!("ana-{numero:04}"))
                            .is_ok()
                    })
                    .count()
            })
            .await
        })
    };
    largada.wait().await;
    let comeco = Instant::now();

    // O enlace, de 20 em 20 ms, por 4 s.
    let mut trajeto: Vec<(u128, String)> = Vec::new();
    while comeco.elapsed() < Duration::from_secs(4) {
        let enlace = match ana.snapshot().link {
            LinkState::Online => "Online".to_owned(),
            LinkState::InternalBattery { attempts, .. } => format!("Bateria({attempts})"),
        };
        if trajeto.last().is_none_or(|(_, ultimo)| *ultimo != enlace) {
            trajeto.push((comeco.elapsed().as_millis(), enlace));
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mandou = escrevendo.await??;
    let mut falantes_atras = 0;
    for tarefa in tarefas {
        falantes_atras += usize::from(tarefa.await?.ficou_para_tras);
    }
    let sentada_sem_janela = servidor.server().occupancy.lock().await.onde_esta(quem);

    let eventos = gravador
        .eventos
        .lock()
        .map(|eventos| eventos.clone())
        .unwrap_or_default();
    let fim = eventos
        .iter()
        .position(|(_, evento)| matches!(evento, Event::Ended { .. }));
    let depois_do_fim = fim.map_or(0, |indice| eventos.len() - indice - 1);
    let qual_fim = fim
        .and_then(|indice| eventos.get(indice))
        .map(|(quando, evento)| format!("{evento:?} aos {:?}", quando.duration_since(comeco)));
    let antes_de_reconectar = estados_dela(&ana);
    let gravadas_dela = servidor.quantas_mensagens(LINHA).await?;

    // O RECONECTAR da tela de fim: `desmontar_o_cliente` larga a conexão, e
    // uma nova entra com a mesma casa (a mesma chave, a mesma pessoa).
    ana.disconnect();
    drop(ana);
    let config = config_da_ana(endereco, casa.path());
    let (de_novo, _) = tokio::task::spawn_blocking(move || seele_ffi::Connection::connect(config))
        .await?
        .map_err(|falha| anyhow::anyhow!("a FFI não entrou de novo: {falha:?}"))?;
    de_novo
        .open_channel(LINHA.get())
        .map_err(|erro| anyhow::anyhow!("{erro:?}"))?;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let depois_de_reconectar = estados_dela(&de_novo);

    println!(
        "MEDIDA-FFI falantes_na_outra={falantes} (FellBehind={falantes_atras}) atraso_dela={atraso:?} \
         | ana mandou={mandou} gravadas_dela={gravadas_dela} | fim={qual_fim:?} \
         eventos_depois_do_fim={depois_do_fim} | enlace={trajeto:?} \
         | antes_de_reconectar={antes_de_reconectar:?} | sentada_com_a_tela_de_fim={sentada_sem_janela:?} \
         | depois_de_reconectar={depois_de_reconectar:?} | sessoes_atras={}",
        servidor.server().atrasos.sessoes(),
    );
    de_novo.disconnect();
    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "medida da pendência 61, e não guarda: rodar com --ignored --nocapture"]
async fn o_ritmo_do_barramento_com_gente_parada() -> Result<()> {
    // Quanto o barramento anda sozinho, com gente conectada e ninguém
    // escrevendo: é isso que decide em quanto tempo um par que parou de ler
    // (a rede travou, a máquina dormiu) é passado para trás.
    let _vaga = vaga::minha();
    let (endereco, servidor) = servidor().await?;
    let mut clientes = Vec::new();
    for numero in 0..24 {
        clientes.push(entrar(endereco, numero).await?);
    }
    tokio::time::sleep(Duration::from_secs(2)).await;
    let mut barramento = servidor.server().events.subscribe();
    let janela = Duration::from_secs(10);
    let fim = Instant::now() + janela;
    let mut por_tipo = std::collections::BTreeMap::<String, u64>::new();
    while let Ok(Ok(evento)) = tokio::time::timeout(
        fim.saturating_duration_since(Instant::now()),
        barramento.recv(),
    )
    .await
    {
        let nome = format!("{evento:?}");
        let nome = nome.split(['(', ' ', '{']).next().unwrap_or("?").to_owned();
        *por_tipo.entry(nome).or_default() += 1;
    }
    let total: u64 = por_tipo.values().sum();
    let por_segundo = total as f64 / janela.as_secs_f64();
    println!(
        "MEDIDA-RITMO 24 conectados, parados, 10 s: {total} eventos ({por_segundo:.1}/s) {por_tipo:?} \
         | um par que parou de ler é passado para trás em {:.0} s",
        1024.0 / por_segundo
    );
    drop(clientes);
    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "medida da pendência 61, e não guarda: rodar com --ignored --nocapture"]
async fn a_mesma_chave_duas_vezes_grava_uma() -> Result<()> {
    // O que torna repetir seguro: a mesma chave, da mesma pessoa, duas vezes.
    let _vaga = vaga::minha();
    let (endereco, servidor) = servidor().await?;
    let mut cliente = entrar(endereco, 0).await?;
    for _ in 0..2 {
        cliente
            .send_message(LINHA, "a mesma", ClientMessageId(7))
            .await?;
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
    let ecos = ler(&mut cliente, 2, Instant::now() + Duration::from_secs(2))
        .await
        .lidas;
    println!(
        "MEDIDA-CHAVE mandada 2 vezes com a chave 7: gravadas={} ecos_para_quem_mandou={ecos}",
        servidor.quantas_mensagens(LINHA).await?
    );
    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "medida da pendência 61, e não guarda: rodar com --ignored --nocapture"]
async fn uma_sala_cheia_falando_no_ritmo_que_o_balde_deixa() -> Result<()> {
    let _vaga = vaga::minha();
    // A taxa sustentada do balde é 20 quadros por segundo por conexão: quatro
    // por lote de 200 ms. Vinte e quatro pessoas no teto, por três segundos.
    rodada(24, 4, 20, 15, Duration::from_millis(200)).await?;
    Ok(())
}
