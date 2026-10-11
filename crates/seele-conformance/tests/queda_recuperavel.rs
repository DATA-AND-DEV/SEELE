//! **Uma queda recuperável não é fim** (lote P61 da F5, pendência 61): a prova
//! pela FFI, como a janela do app a vê.
//!
//! # O defeito, medido pela F1
//!
//! O motor do enlace tratava `Disconnecting { FellBehind }` como queda e
//! reconectava; o `Room` tratava toda `Disconnecting` como fim, a FFI emitia
//! `Ended` e o app revogava a geração. A janela mostrava «ENLACE ENCERRADO»
//! enquanto a pessoa seguia sentada na sala por baixo, com o que escrevia no
//! instante em `SemResposta` debaixo da tela, e nada buscava o que passou no
//! canal durante a queda.
//!
//! # O que estes testes provam
//!
//! - **A faixa, e não o fim** (fase B): com `FellBehind`, a janela não vê
//!   `Ended`, vê a faixa da bateria e vê a volta, e o servidor a tem sentada.
//! - **O buraco e a repetição** (fase C): o que passou no canal durante a queda
//!   aparece, e o que ela escreveu no instante e durante a bateria chega ao
//!   servidor uma vez cada, e à tela uma vez cada.
//! - **A voz** (fase D), pelo aparelho de mentira do `seele-audio`: numa razão
//!   que acaba a sessão, a voz fecha e não reabre; numa queda, ela volta.
//!
//! # Como a despedida é injetada
//!
//! Pelo `Event::SessionEnded` no barramento, o mesmo caminho que
//! `bateria_interna.rs` usa: o braço da sessão que o atende escreve
//! `Disconnecting { reason }` e se despede da conexão. Só a razão muda de um
//! teste para o outro, e só a pessoa da FFI cai.

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
use seele_proto::control::{DisconnectReason, ServerMessage};
use seele_proto::ids::{ChannelId, PersonId, VoiceRoomId};
use seele_server::persistence::messages::{Messages, PendingMessage};
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

mod vaga;

const LINHA: ChannelId = ChannelId(1);
const SALA: VoiceRoomId = VoiceRoomId(1);

/// Quanto cada passo de rede tem para acontecer.
const PRAZO: Duration = Duration::from_secs(20);

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

/// O que a FFI entregou à janela, e o enlace **como a janela o lê**.
///
/// O enlace é lido do retrato no próprio aviso, como a janela faz ao receber
/// `TelemetryChanged`. Medido na fase A: a faixa da bateria dura de 6 a 8 ms
/// quando a primeira volta dá certo, e uma amostra de 20 ms de fora nunca a vê.
#[derive(Default)]
struct Gravador {
    eventos: std::sync::Mutex<Vec<Event>>,
    conexao: std::sync::OnceLock<std::sync::Weak<Connection>>,
    /// `true` para cada leitura na bateria, `false` para cada uma de pé.
    enlaces: std::sync::Mutex<Vec<bool>>,
}

impl seele_ffi::EventListener for Gravador {
    fn on_event(&self, event: Event) {
        let le_o_retrato = matches!(event, Event::TelemetryChanged | Event::Ended { .. });
        if let Ok(mut eventos) = self.eventos.lock() {
            eventos.push(event);
        }
        if !le_o_retrato {
            return;
        }
        let Some(conexao) = self.conexao.get().and_then(std::sync::Weak::upgrade) else {
            return;
        };
        let na_bateria = matches!(conexao.snapshot().link, LinkState::InternalBattery { .. });
        if let Ok(mut enlaces) = self.enlaces.lock() {
            enlaces.push(na_bateria);
        }
    }
}

impl Gravador {
    fn fim(&self) -> Option<Event> {
        self.eventos
            .lock()
            .ok()?
            .iter()
            .find(|evento| matches!(evento, Event::Ended { .. }))
            .cloned()
    }

    /// Até onde o enlace já foi lido: o que vem depois é desta marca em diante.
    fn marca(&self) -> usize {
        self.enlaces.lock().map_or(0, |enlaces| enlaces.len())
    }

    /// Se, desde `marca`, a janela leu a faixa da bateria alguma vez.
    fn leu_a_bateria_desde(&self, marca: usize) -> bool {
        self.enlaces.lock().is_ok_and(|enlaces| {
            enlaces
                .get(marca..)
                .unwrap_or_default()
                .iter()
                .any(|na_bateria| *na_bateria)
        })
    }

    /// Se, desde `marca`, a janela leu a faixa da bateria e depois a volta.
    fn faixa_e_volta_desde(&self, marca: usize) -> bool {
        self.enlaces.lock().is_ok_and(|enlaces| {
            let depois = enlaces.get(marca..).unwrap_or_default();
            depois
                .iter()
                .position(|na_bateria| *na_bateria)
                .is_some_and(|faixa| depois[faixa..].iter().any(|na_bateria| !na_bateria))
        })
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

/// A pessoa da FFI, inscrita como a janela, na sala e no canal, e com a sessão
/// de pé **do lado de lá**: uma mensagem dela foi e voltou confirmada.
async fn ana(
    endereco: SocketAddr,
    casa: &std::path::Path,
    aparelho: Option<&str>,
) -> Result<(Arc<Connection>, Arc<Gravador>, PersonId)> {
    let config = config(endereco, casa, aparelho);
    let (ana, _) = tokio::task::spawn_blocking(move || Connection::connect(config))
        .await?
        .map_err(|falha| anyhow::anyhow!("a FFI não entrou: {falha:?}"))?;
    let gravador = Arc::new(Gravador::default());
    let _ = gravador.conexao.set(Arc::downgrade(&ana));
    ana.subscribe(gravador.clone());
    if aparelho.is_some() {
        ana.set_voice_mode(VoiceMode::Open);
    }
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
    Ok((ana, gravador, quem))
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

fn despedir(servidor: &Daemon, quem: PersonId, reason: DisconnectReason) {
    servidor
        .server()
        .events
        .send(seele_server::server::Event::SessionEnded {
            person: quem,
            reason,
        })
        .expect("o barramento do servidor está de pé");
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

/// Se `cliente` ouve `quem` falando em `janela`.
async fn ouve_falar(cliente: &mut Client, quem: PersonId, janela: Duration) -> bool {
    let fim = Instant::now() + janela;
    while let Ok(Ok(mensagem)) = tokio::time::timeout(
        fim.saturating_duration_since(Instant::now()),
        cliente.next_event(),
    )
    .await
    {
        if let ServerMessage::PersonState(estado) = mensagem {
            if estado.person == quem && estado.speaking {
                return true;
            }
        }
    }
    false
}

/// **Fase B**: ficar para trás no barramento é queda, e a janela vê a faixa e
/// a volta — não a tela de fim.
#[tokio::test(flavor = "multi_thread")]
async fn ficar_para_tras_mostra_a_faixa_e_a_volta_e_nao_o_fim() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor) = servidor().await?;
    let casa = tempfile::tempdir()?;
    let (ana, gravador, quem) = ana(endereco, casa.path(), None).await?;

    let marca = gravador.marca();
    despedir(&servidor, quem, DisconnectReason::FellBehind);
    esperar("a janela ler a faixa da bateria e depois a volta", || {
        gravador.faixa_e_volta_desde(marca)
    })
    .await?;
    // Uma volta inteira do laço depois da volta: o que viesse atrás dela, viria.
    tokio::time::sleep(Duration::from_millis(500)).await;

    assert_eq!(
        gravador.fim(),
        None,
        "a janela recebeu a tela de fim numa queda que o núcleo reconecta"
    );
    assert_eq!(
        ana.snapshot().ended,
        None,
        "o retrato diz que a sessão acabou"
    );
    assert_eq!(
        servidor.server().occupancy.lock().await.onde_esta(quem),
        Some(SALA),
        "ela não está sentada na sala depois da volta"
    );
    ana.disconnect();
    servidor.shutdown();
    Ok(())
}

/// **Fase C**: na volta, o buraco do canal aparece, e o que ela escreveu no
/// instante da queda e durante a bateria chega — uma vez cada.
#[tokio::test(flavor = "multi_thread")]
async fn a_volta_traz_o_buraco_e_o_que_ficou_sem_resposta_uma_vez_cada() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor) = servidor().await?;
    let casa = tempfile::tempdir()?;
    let (ana, gravador, quem) = ana(endereco, casa.path(), None).await?;

    // **O buraco**: 450 mensagens de outra pessoa que ela nunca viu passar.
    // Gravadas direto no banco, sem anúncio: só o histórico as alcança, e 450
    // passam das três páginas de 200 e muito das 50 que abrir o canal traz.
    let outra = bia(endereco).await?.session().person;
    {
        let mut banco = servidor.server().persistence.lock().await;
        let mut mensagens = Messages::new(&mut banco);
        let buraco: Vec<PendingMessage> = (1..=450)
            .map(|numero| PendingMessage {
                channel: LINHA,
                author: outra,
                author_nickname: "bia".into(),
                body: format!("buraco-{numero:03}"),
                replies_to: None,
                client_message_id: None,
            })
            .collect();
        mensagens.append_batch(&buraco)?;
    }

    // **No instante**: seis mensagens e a despedida logo atrás. Cada uma
    // depende da repetição: ou o servidor não chegou a lê-la, ou gravou e o
    // eco foi para uma sessão que já tinha acabado — e ela fica em
    // `SemResposta` na tela. (A bateria dura milissegundos, medido na fase A:
    // não há janela para escrever «durante» ela.)
    let marca = gravador.marca();
    for numero in 1..=6 {
        ana.send_message(LINHA.0, format!("no-instante-{numero}"))
            .map_err(|erro| anyhow::anyhow!("{erro:?}"))?;
    }
    despedir(&servidor, quem, DisconnectReason::FellBehind);
    esperar("a janela ler a faixa da bateria e depois a volta", || {
        gravador.faixa_e_volta_desde(marca)
    })
    .await?;

    let dela: Vec<String> = (1..=6)
        .map(|numero| format!("no-instante-{numero}"))
        .collect();
    esperar("o buraco e as seis dela aparecerem confirmadas", || {
        let mensagens = ana.messages();
        mensagens
            .iter()
            .filter(|m| m.body.starts_with("buraco-"))
            .count()
            >= 450
            && dela.iter().all(|corpo| {
                mensagens
                    .iter()
                    .any(|m| &m.body == corpo && m.estado == EstadoDoEnvio::Confirmada)
            })
    })
    .await?;
    // O eco duplo da repetição chega depois da confirmação: espera ele passar.
    tokio::time::sleep(Duration::from_millis(1000)).await;

    let mensagens = ana.messages();
    for corpo in &dela {
        let vezes = mensagens.iter().filter(|m| &m.body == corpo).count();
        assert_eq!(vezes, 1, "«{corpo}» aparece {vezes} vezes na tela");
    }
    for numero in 1..=450 {
        let corpo = format!("buraco-{numero:03}");
        let vezes = mensagens.iter().filter(|m| m.body == corpo).count();
        assert_eq!(
            vezes, 1,
            "«{corpo}», do buraco, aparece {vezes} vezes na tela"
        );
    }
    // Na ordem em que ela escreveu: o servidor numera na ordem em que grava.
    let ids: Vec<u64> = (1..=6)
        .map(|numero| {
            mensagens
                .iter()
                .find(|m| m.body == format!("no-instante-{numero}"))
                .map(|m| m.id)
                .expect("já conferida acima")
        })
        .collect();
    assert!(
        ids.windows(2).all(|par| par[0] < par[1]),
        "a repetição saiu fora da ordem em que ela escreveu: {ids:?}"
    );
    // E o servidor gravou cada uma uma vez: 1 de antes, 450 do buraco, 6 dela.
    assert_eq!(
        servidor.quantas_mensagens(LINHA).await?,
        1 + 450 + 6,
        "o servidor gravou alguma das dela duas vezes, ou perdeu alguma"
    );
    assert_eq!(gravador.fim(), None);
    ana.disconnect();
    servidor.shutdown();
    Ok(())
}

/// **Fase D**: numa razão que acaba a sessão, a voz fecha e não reabre; numa
/// queda, ela volta, e transmite — a tela não diz mais «encerrado».
#[tokio::test(flavor = "multi_thread")]
async fn a_voz_fecha_no_fim_e_volta_na_queda() -> Result<()> {
    let _vaga = vaga::minha();

    // A queda: a voz volta e o servidor a ouve.
    let (endereco, daemon) = servidor().await?;
    let aparelho = "p61-voz-na-queda";
    let casa = tempfile::tempdir()?;
    let (ana_na_queda, gravador, quem) = ana(endereco, casa.path(), Some(aparelho)).await?;
    esperar("a voz abrir", || mentira::contagem(aparelho).abertos == 1).await?;
    let antes = mentira::contagem(aparelho).aberturas;
    let marca = gravador.marca();
    despedir(&daemon, quem, DisconnectReason::FellBehind);
    esperar("a voz reabrir na volta", || {
        gravador.faixa_e_volta_desde(marca)
            && mentira::contagem(aparelho).aberturas > antes
            && mentira::contagem(aparelho).abertos == 1
    })
    .await?;
    let mut ouvinte = bia(endereco).await?;
    assert!(
        ouve_falar(&mut ouvinte, quem, Duration::from_secs(3)).await,
        "depois da volta, o servidor não ouve a voz dela"
    );
    assert_eq!(gravador.fim(), None);
    drop(ouvinte);
    ana_na_queda.disconnect();
    daemon.shutdown();

    // O fim: a voz fecha, e não reabre.
    let (endereco, daemon) = servidor().await?;
    let aparelho = "p61-voz-no-fim";
    let casa = tempfile::tempdir()?;
    let (ana_no_fim, gravador, quem) = ana(endereco, casa.path(), Some(aparelho)).await?;
    esperar("a voz abrir", || mentira::contagem(aparelho).abertos == 1).await?;
    let aberturas = mentira::contagem(aparelho).aberturas;
    despedir(&daemon, quem, DisconnectReason::Kicked);
    esperar("a tela de fim", || gravador.fim().is_some()).await?;
    esperar("a voz fechar", || mentira::contagem(aparelho).abertos == 0).await?;
    // Uma volta da bateria inteira, se houvesse uma: a primeira tentativa sai
    // 500 ms depois da queda.
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(
        mentira::contagem(aparelho),
        mentira::Contagem {
            abertos: 0,
            aberturas,
            ..mentira::contagem(aparelho)
        },
        "a voz reabriu depois de uma despedida que acaba a sessão"
    );
    ana_no_fim.disconnect();
    daemon.shutdown();
    Ok(())
}

/// **Fase D**: TENTAR DE NOVO numa recusada por tamanho é frase, e não queda.
///
/// Medido na fase A: `reenviar_mensagem` mandava os 6 000 bytes ao enlace, o
/// quadro se recusava a ser codificado e o núcleo derrubava a conexão; a
/// pendente passava de `Recusada(TooLong)` a `SemResposta`.
#[tokio::test(flavor = "multi_thread")]
async fn tentar_de_novo_uma_grande_demais_nao_derruba_o_enlace() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor) = servidor().await?;
    let casa = tempfile::tempdir()?;
    let (ana, gravador, quem) = ana(endereco, casa.path(), None).await?;

    let chave = ana
        .send_message(LINHA.0, "á".repeat(3_000))
        .map_err(|erro| anyhow::anyhow!("{erro:?}"))?;
    let marca = gravador.marca();
    assert_eq!(ana.reenviar_mensagem(&chave), Ok(true));
    // A queda medida voltava em menos de 22 ms; dois segundos cobrem a queda e
    // a volta com folga.
    tokio::time::sleep(Duration::from_secs(2)).await;

    assert!(
        !gravador.leu_a_bateria_desde(marca),
        "TENTAR DE NOVO numa mensagem grande demais derrubou o enlace"
    );
    let estado = ana
        .messages()
        .into_iter()
        .find(|m| m.client_message_id.as_deref() == Some(chave.as_str()))
        .map(|m| m.estado);
    assert_eq!(
        estado,
        Some(EstadoDoEnvio::Recusada(
            seele_ffi::types::MotivoDaRecusa::TooLong { limit: 4096 }
        )),
        "a pendente deixou de dizer por que não sai"
    );
    assert_eq!(servidor.quantas_mensagens(LINHA).await?, 1);
    assert_eq!(
        servidor.server().occupancy.lock().await.onde_esta(quem),
        Some(SALA)
    );
    ana.disconnect();
    servidor.shutdown();
    Ok(())
}
