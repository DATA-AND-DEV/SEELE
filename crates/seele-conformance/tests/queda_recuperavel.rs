//! **Uma queda recuperável não é fim**: o porte da pendência 61 da 0.16 para a
//! 0.15.2, provado pela FFI, como a janela do app a vê.
//!
//! # O defeito, na v0.15.1
//!
//! O motor do enlace só acaba a sessão em `Kicked`, `Banned` e `ModsMudaram`;
//! nas outras despedidas ele reconecta pela bateria. O `Room` marcava fim para
//! **toda** `Disconnecting`: a FFI emitia `Ended`, a janela dizia «encerrado»
//! e revogava a geração, e a reconexão por baixo reabria a voz no
//! `Aviso::Reconectado` (`reopen`), que voltava a transmitir sem a tela dizer.
//!
//! # O que estes testes provam
//!
//! Numa despedida recuperável, a janela não recebe `Ended`, lê a faixa da
//! bateria e depois a volta, o retrato não diz fim, o servidor a tem sentada na
//! sala de novo, e a sessão continua: o que ela escreve depois chega
//! confirmado.
//!
//! A voz não é aberta aqui (`audio: false`). Esta linha não tem o aparelho de
//! mentira da 0.16, e nenhum teste abre o microfone. O que fica provado é onde
//! a voz cai: ela reabre no `Aviso::Reconectado`, e é esse mesmo aviso que
//! leva a janela da faixa à volta. Agora ela reabre numa sessão que a tela
//! mostra viva, e não debaixo de «encerrado».
//!
//! E a fase D da 0.16, à parte: o TENTAR DE NOVO de uma mensagem recusada por
//! tamanho não derruba o enlace, e a recusa continua dizendo por quê.
//!
//! # Como a despedida é encenada
//!
//! - **A de verdade**: o barramento recebe oito vezes o anel de uma vez, como
//!   o escritor anunciava um lote sem teto antes da 0.15.2. A sessão fica para
//!   trás e o braço do `Lagged` escreve `FellBehind`, com o assento guardado
//!   pela janela de graça.
//! - **Cada razão recuperável**, pelo `Event::SessionEnded` no barramento, o
//!   caminho que `bateria_interna.rs` já usa: o braço que o atende escreve
//!   `Disconnecting { reason }` e se despede da conexão. Só a razão muda.
//!
//! Das seis, o servidor 0.15 só escreve `FellBehind` e `RateLimited` no meio de
//! uma sessão: `ServerShuttingDown` e `ProtocolViolation` são recusas do aperto
//! de mão, e `Timeout` e `ScheduledMaintenance` ele não escreve. O cliente é
//! provado contra todas, porque é o protocolo, e não este servidor, quem diz o
//! que pode chegar.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

use anyhow::Result;
use seele_ffi::types::{EstadoDoEnvio, Event, LinkState, MotivoDaRecusa};
use seele_ffi::{ConnectConfig, Connection, EventListener};
use seele_proto::control::DisconnectReason;
use seele_proto::ids::{ChannelId, PersonId, VoiceRoomId};
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

mod vaga;

const LINHA: ChannelId = ChannelId(1);
const SALA: VoiceRoomId = VoiceRoomId(1);

/// Quanto cada passo de rede tem para acontecer.
const PRAZO: Duration = Duration::from_secs(20);

/// As despedidas que o motor do enlace trata como queda, e reconecta.
const RECUPERAVEIS: [DisconnectReason; 6] = [
    DisconnectReason::FellBehind,
    DisconnectReason::RateLimited,
    DisconnectReason::Timeout,
    DisconnectReason::ScheduledMaintenance,
    DisconnectReason::ServerShuttingDown,
    DisconnectReason::ProtocolViolation,
];

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
/// `TelemetryChanged`: a 0.16 mediu a faixa da bateria durando de 6 a 8 ms
/// quando a primeira volta dá certo, e uma amostra de fora não a veria.
#[derive(Default)]
struct Gravador {
    eventos: Mutex<Vec<Event>>,
    conexao: OnceLock<Weak<Connection>>,
    /// `true` para cada leitura na bateria, `false` para cada uma de pé.
    enlaces: Mutex<Vec<bool>>,
}

impl EventListener for Gravador {
    fn on_event(&self, event: Event) {
        let le_o_retrato = matches!(event, Event::TelemetryChanged | Event::Ended { .. });
        if let Ok(mut eventos) = self.eventos.lock() {
            eventos.push(event);
        }
        if !le_o_retrato {
            return;
        }
        let Some(conexao) = self.conexao.get().and_then(Weak::upgrade) else {
            return;
        };
        let na_bateria = matches!(conexao.snapshot().link, LinkState::InternalBattery { .. });
        if let Ok(mut enlaces) = self.enlaces.lock() {
            enlaces.push(na_bateria);
        }
    }
}

/// Até onde o gravador já foi lido: o que vem depois é desta marca em diante.
#[derive(Clone, Copy)]
struct Marca {
    eventos: usize,
    enlaces: usize,
}

impl Gravador {
    fn marca(&self) -> Marca {
        Marca {
            eventos: self.eventos.lock().map_or(0, |eventos| eventos.len()),
            enlaces: self.enlaces.lock().map_or(0, |enlaces| enlaces.len()),
        }
    }

    /// Os `Ended` que a janela recebeu desde `marca`.
    fn fins_desde(&self, marca: Marca) -> Vec<Event> {
        self.eventos.lock().map_or_else(
            |_| Vec::new(),
            |eventos| {
                eventos
                    .get(marca.eventos..)
                    .unwrap_or_default()
                    .iter()
                    .filter(|evento| matches!(evento, Event::Ended { .. }))
                    .cloned()
                    .collect()
            },
        )
    }

    /// Se, desde `marca`, a janela leu a faixa da bateria alguma vez.
    fn leu_a_bateria_desde(&self, marca: Marca) -> bool {
        self.enlaces.lock().is_ok_and(|enlaces| {
            enlaces
                .get(marca.enlaces..)
                .unwrap_or_default()
                .contains(&true)
        })
    }

    /// Se, desde `marca`, a janela leu a faixa da bateria e depois a volta.
    fn faixa_e_volta_desde(&self, marca: Marca) -> bool {
        self.enlaces.lock().is_ok_and(|enlaces| {
            let depois = enlaces.get(marca.enlaces..).unwrap_or_default();
            depois
                .iter()
                .position(|na_bateria| *na_bateria)
                .is_some_and(|faixa| depois[faixa..].iter().any(|na_bateria| !na_bateria))
        })
    }
}

/// A pessoa da FFI, inscrita como a janela, na sala e no canal, e com a sessão
/// de pé **do lado de lá**: uma mensagem dela foi e voltou confirmada.
async fn ana(
    endereco: SocketAddr,
    casa: &std::path::Path,
) -> Result<(Arc<Connection>, Arc<Gravador>, PersonId)> {
    let config = ConnectConfig {
        server: endereco.to_string(),
        alternate_servers: Vec::new(),
        nickname: "ana".into(),
        home: casa.display().to_string(),
        join_secret: None,
        expected_fingerprint: None,
        bilhete: None,
        // Nenhum teste desta linha abre o microfone.
        audio: false,
        capture_device: None,
        playback_device: None,
    };
    let (ana, _) = tokio::task::spawn_blocking(move || Connection::connect(config))
        .await?
        .map_err(|falha| anyhow::anyhow!("a FFI não entrou: {falha:?}"))?;
    let gravador = Arc::new(Gravador::default());
    let _ = gravador.conexao.set(Arc::downgrade(&ana));
    ana.subscribe(gravador.clone());
    ana.enter_voice_room(SALA.0, None)
        .map_err(|erro| anyhow::anyhow!("{erro:?}"))?;
    ana.open_channel(LINHA.0)
        .map_err(|erro| anyhow::anyhow!("{erro:?}"))?;
    dizer_e_ver_confirmada(&ana, "estou aqui").await?;
    let quem = PersonId(ana.snapshot().me.expect("ela sabe quem é"));
    Ok((ana, gravador, quem))
}

/// Escreve no canal e espera a confirmação do servidor: a sessão está de pé.
async fn dizer_e_ver_confirmada(ana: &Connection, texto: &str) -> Result<()> {
    ana.send_message(LINHA.0, texto.to_owned())
        .map_err(|erro| anyhow::anyhow!("{erro:?}"))?;
    esperar(&format!("«{texto}» ser confirmada"), || {
        ana.messages()
            .iter()
            .any(|m| m.body == texto && m.estado == EstadoDoEnvio::Confirmada)
    })
    .await
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

/// Se o servidor a tem sentada na sala, esperando até o prazo.
async fn sentada(servidor: &Daemon, quem: PersonId) -> bool {
    let prazo = Instant::now() + PRAZO;
    loop {
        if servidor.server().occupancy.lock().await.onde_esta(quem) == Some(SALA) {
            return true;
        }
        if Instant::now() > prazo {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Depois da volta, o que a janela e o servidor dizem. Vazio quando a queda foi
/// queda: nenhum `Ended`, o retrato sem fim, e ela sentada na sala.
async fn o_que_disse_fim(
    servidor: &Daemon,
    ana: &Connection,
    gravador: &Gravador,
    quem: PersonId,
    marca: Marca,
    motivo: DisconnectReason,
) -> Vec<String> {
    // Uma volta inteira do laço depois da volta: o que viesse atrás, viria.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let mut achados = Vec::new();
    let fins = gravador.fins_desde(marca);
    if !fins.is_empty() {
        achados.push(format!(
            "{motivo:?}: a janela recebeu {fins:?} numa queda que o núcleo reconectou"
        ));
    }
    if let Some(fim) = ana.snapshot().ended {
        achados.push(format!(
            "{motivo:?}: o retrato diz que a sessão acabou ({fim:?}) depois da volta"
        ));
    }
    if !sentada(servidor, quem).await {
        achados.push(format!(
            "{motivo:?}: ela não está sentada na sala depois da volta"
        ));
    }
    achados
}

/// **O `FellBehind` de verdade**, pelo braço do `Lagged`: a janela vê a faixa e
/// a volta, e não a tela de fim.
///
/// É a despedida que a v0.15.1 mostrava a todo mundo de uma vez numa rajada,
/// como «ESTE ENLACE FICOU PARA TRÁS DO SERVIDOR», enquanto o núcleo reconectava
/// cada um por baixo.
#[tokio::test(flavor = "multi_thread")]
async fn ficar_para_tras_de_verdade_mostra_a_faixa_e_a_volta_e_nao_o_fim() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor) = servidor().await?;
    let casa = tempfile::tempdir()?;
    let (ana, gravador, quem) = ana(endereco, casa.path()).await?;

    let atrasadas_antes = servidor.server().atrasos.sessoes();
    let marca = gravador.marca();
    // Sem `await` entre os envios, como o escritor anunciava um lote: nenhuma
    // sessão tira evento do anel no meio, e a dela fica para trás.
    for _ in 0..8 * seele_server::server::ANEL_DO_BARRAMENTO {
        let _ = servidor
            .server()
            .events
            .send(seele_server::server::Event::HostUplink { bps: 1 });
    }
    esperar("a janela ler a faixa da bateria e depois a volta", || {
        gravador.faixa_e_volta_desde(marca)
    })
    .await?;
    assert!(
        servidor.server().atrasos.sessoes() > atrasadas_antes,
        "o servidor não registrou a sessão dela ficando para trás: a queda não foi o \
         `FellBehind`, e este teste mediria outra coisa"
    );

    let achados = o_que_disse_fim(
        &servidor,
        &ana,
        &gravador,
        quem,
        marca,
        DisconnectReason::FellBehind,
    )
    .await;
    assert!(achados.is_empty(), "{}", achados.join("\n"));
    dizer_e_ver_confirmada(&ana, "ainda aqui").await?;

    ana.disconnect();
    servidor.shutdown();
    Ok(())
}

/// **Cada despedida recuperável**, uma depois da outra na mesma sessão: a
/// janela vê a faixa e a volta, e não a tela de fim, e a sessão continua.
#[tokio::test(flavor = "multi_thread")]
async fn cada_despedida_recuperavel_mostra_a_faixa_e_a_volta_e_nao_o_fim() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor) = servidor().await?;
    let casa = tempfile::tempdir()?;
    let (ana, gravador, quem) = ana(endereco, casa.path()).await?;

    let mut achados = Vec::new();
    for motivo in RECUPERAVEIS {
        let marca = gravador.marca();
        servidor
            .server()
            .events
            .send(seele_server::server::Event::SessionEnded {
                person: quem,
                reason: motivo,
            })
            .expect("o barramento do servidor está de pé");
        esperar(
            &format!("{motivo:?}: a janela ler a faixa da bateria e depois a volta"),
            || gravador.faixa_e_volta_desde(marca),
        )
        .await?;
        achados.extend(o_que_disse_fim(&servidor, &ana, &gravador, quem, marca, motivo).await);
    }
    assert!(achados.is_empty(), "{}", achados.join("\n"));
    dizer_e_ver_confirmada(&ana, "ainda aqui").await?;

    ana.disconnect();
    servidor.shutdown();
    Ok(())
}

/// **O TENTAR DE NOVO de uma recusada por tamanho é frase, e não queda** (a
/// fase D da 0.16, à parte).
///
/// Medido na revisão do porte, pela FFI, sobre a v0.15.1: o `send_message`
/// confere o teto e recusa com `TooLong`, mas o `reenviar_mensagem` mandava o
/// corpo ao enlace. O quadro se recusava a ser codificado, o núcleo derrubava a
/// conexão (queda e volta em ~22 ms, a cada toque), e a pendente passava de
/// «não foi gravada: passa do teto» a «o enlace caiu antes da resposta — pode
/// ter sido gravada», que é falso: o corpo nunca saiu desta máquina.
///
/// Sem sono: a mensagem dita logo depois do TENTAR DE NOVO só sai confirmada
/// quando o motor já processou o comando anterior, que vem antes na mesma fila.
#[tokio::test(flavor = "multi_thread")]
async fn tentar_de_novo_uma_grande_demais_nao_derruba_o_enlace() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor) = servidor().await?;
    let casa = tempfile::tempdir()?;
    let (ana, gravador, quem) = ana(endereco, casa.path()).await?;

    // Um acento a mais que o teto, em bytes: o caso do doc de
    // `Connection::limite_da_mensagem`.
    let teto = Connection::limite_da_mensagem();
    let recusa = EstadoDoEnvio::Recusada(MotivoDaRecusa::TooLong {
        limit: u32::try_from(teto)?,
    });
    let estado_de = |chave: &str| {
        ana.messages()
            .into_iter()
            .find(|m| m.client_message_id.as_deref() == Some(chave))
            .map(|m| m.estado)
    };
    let chave = ana
        .send_message(LINHA.0, "á".repeat(teto / 2 + 1))
        .map_err(|erro| anyhow::anyhow!("{erro:?}"))?;
    assert_eq!(
        estado_de(&chave),
        Some(recusa),
        "o envio não recusou o corpo acima do teto"
    );

    let marca = gravador.marca();
    for toque in 1..=2 {
        assert_eq!(
            ana.reenviar_mensagem(&chave),
            Ok(true),
            "o TENTAR DE NOVO #{toque} não achou a pendente"
        );
    }
    let viva = dizer_e_ver_confirmada(&ana, "depois do tentar de novo").await;

    assert!(
        !gravador.leu_a_bateria_desde(marca),
        "o TENTAR DE NOVO numa mensagem grande demais derrubou o enlace: a janela leu a \
         faixa da bateria, e a pendente ficou em {:?}",
        estado_de(&chave)
    );
    assert_eq!(
        estado_de(&chave),
        Some(recusa),
        "a pendente deixou de dizer por que não sai"
    );
    viva?;
    assert_eq!(
        gravador.fins_desde(marca),
        Vec::new(),
        "a janela recebeu a tela de fim"
    );
    assert!(sentada(&servidor, quem).await, "ela saiu da sala");

    ana.disconnect();
    servidor.shutdown();
    Ok(())
}
