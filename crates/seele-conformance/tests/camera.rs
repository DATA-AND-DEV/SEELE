//! **A câmera, de ponta a ponta** (1.0, protocolo 13): pela `Connection` de
//! verdade, pelo servidor de verdade.
//!
//! O aplicativo não captura a câmera — o workspace proíbe `unsafe` —, então
//! quem codifica é a janela, e os quadros entram no núcleo já prontos, por
//! `Connection::quadro_da_camera`. Este arquivo põe um «codificador de
//! mentira» nesse lugar: bytes com forma de H.264, empurrados pela mesma
//! porta que a janela usa. O que se mede é tudo o que vem depois dela.
//!
//! - quem assiste pelo 13 recebe um `ScreenOpened` que diz **câmera**, e os
//!   quadros **byte por byte** como entraram, na ordem;
//! - quem assiste pelo 12 (um par cru) recebe a abertura com a fonte reescrita
//!   como monitor, e os mesmos quadros;
//! - o pedido de quadro-chave de quem assiste chega à casca de quem transmite
//!   como evento — é ela que tem o codificador;
//! - quem liga a câmera não recebe os próprios quadros de volta;
//! - diante de um servidor do 12, a câmera é recusada com o nome do erro e
//!   **nada** vai ao fio;
//! - ligar a câmera de novo, com uma no ar, troca uma pela outra: quem assiste
//!   vê a primeira fechar e a segunda chegar;
//! - o codec e as medidas que o aplicativo dá à janela são os que o `.wasm` dá
//!   à página.

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
use ed25519_dalek::SigningKey;
use seele_ffi::{
    ConnectConfig, Connection, ConnectionError, Event, EventListener, LimitesDeTela, ScreenKind,
};
use seele_proto::control::{ClientMessage, ServerMessage};
use seele_proto::screen::{ScreenHeader, ScreenSource, SCREEN_HEADER_LEN};
use seele_server::persistence::Location;
use seele_server::{frame, Daemon, ServerConfig};

mod par_cru;
mod vaga;

const PRAZO: Duration = Duration::from_secs(15);

async fn servidor() -> Result<(SocketAddr, Arc<Daemon>)> {
    let daemon = Arc::new(
        Daemon::bind(ServerConfig {
            name: "Casa".into(),
            listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            database: Location::Memory,
            caminho_bps: Some(50_000_000),
            ..ServerConfig::default()
        })
        .await?,
    );
    let endereco = daemon.local_addr()?;
    let servindo = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = servindo.run().await;
    });
    Ok((endereco, daemon))
}

/// O que a janela de uma pessoa receberia, na ordem.
#[derive(Default)]
struct Olhos {
    eventos: Mutex<Vec<Event>>,
}

impl EventListener for Olhos {
    fn on_event(&self, event: Event) {
        if matches!(
            event,
            Event::ScreenOpened { .. }
                | Event::ScreenFrame { .. }
                | Event::ScreenClosed { .. }
                | Event::ScreenUnreadable { .. }
                | Event::CameraKeyFrameRequested { .. }
                | Event::CameraBudget { .. }
                | Event::CameraPaused { .. }
        ) {
            self.eventos.lock().unwrap().push(event);
        }
    }
}

impl Olhos {
    fn vistos(&self) -> Vec<Event> {
        self.eventos.lock().unwrap().clone()
    }

    /// As aberturas vistas: (tela, largura, altura, tipo).
    fn aberturas(&self) -> Vec<(u32, u16, u16, ScreenKind)> {
        self.vistos()
            .into_iter()
            .filter_map(|evento| match evento {
                Event::ScreenOpened {
                    screen,
                    width,
                    height,
                    source,
                } => Some((screen, width, height, source)),
                _ => None,
            })
            .collect()
    }

    /// Os quadros desta tela: (é chave, os bytes).
    fn quadros(&self, tela: u32) -> Vec<(bool, Vec<u8>)> {
        self.vistos()
            .into_iter()
            .filter_map(|evento| match evento {
                Event::ScreenFrame { screen, key, data } if screen == tela => {
                    Some((key, seele_ffi::de_base64(&data).expect("base64 do quadro")))
                }
                _ => None,
            })
            .collect()
    }

    fn pedidos_de_chave(&self, tela: u32) -> usize {
        self.vistos()
            .iter()
            .filter(|evento| {
                matches!(evento, Event::CameraKeyFrameRequested { screen } if *screen == tela)
            })
            .count()
    }

    fn fechadas(&self) -> Vec<u32> {
        self.vistos()
            .into_iter()
            .filter_map(|evento| match evento {
                Event::ScreenClosed { screen } => Some(screen),
                _ => None,
            })
            .collect()
    }
}

async fn entrar(
    endereco: SocketAddr,
    casa: &std::path::Path,
    apelido: &str,
) -> (Arc<Connection>, Arc<Olhos>) {
    let config = ConnectConfig {
        server: endereco.to_string(),
        alternate_servers: Vec::new(),
        nickname: apelido.into(),
        home: casa.display().to_string(),
        join_secret: None,
        expected_fingerprint: None,
        codigo_esperado: None,
        bilhete: None,
        audio: false,
        capture_device: None,
        playback_device: None,
    };
    let (conexao, _) = tokio::task::spawn_blocking(move || Connection::connect(config))
        .await
        .expect("a thread que conecta caiu")
        .unwrap_or_else(|falha| panic!("a pessoa não entrou: {falha:?}"));
    let olhos = Arc::new(Olhos::default());
    conexao.subscribe(Arc::clone(&olhos) as Arc<dyn EventListener>);
    (conexao, olhos)
}

async fn sentar(conexao: &Arc<Connection>) {
    let sala = conexao.snapshot().voice_rooms[0].id;
    conexao.enter_voice_room(sala, None).expect("sentar");
    assert!(
        ate(PRAZO, || conexao
            .snapshot()
            .voice_rooms
            .iter()
            .any(|sala| sala.occupied_by_us))
        .await,
        "a pessoa não sentou na sala"
    );
}

async fn ate(prazo: Duration, mut pronto: impl FnMut() -> bool) -> bool {
    let comeco = Instant::now();
    while comeco.elapsed() < prazo {
        if pronto() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    pronto()
}

fn limites() -> LimitesDeTela {
    LimitesDeTela {
        banda_bps: Some(2_500_000),
        altura_maxima: 720,
        quadros_maximos: 30,
        prioridade: seele_core::retrato::Prioridade::Movimento,
        com_som: false,
    }
}

/// Um quadro com cara de H.264 em Annex-B: o código de início, o tipo da NAL,
/// e um corpo que só este quadro tem.
fn quadro_de_mentira(chave: bool, marca: u8, tamanho: usize) -> Vec<u8> {
    let mut bytes = vec![0, 0, 0, 1, if chave { 0x65 } else { 0x41 }];
    bytes.extend((0..tamanho).map(|i| marca.wrapping_add(u8::try_from(i % 251).unwrap())));
    bytes
}

/// Liga a câmera e espera o fluxo abrir do lado de quem transmite. Devolve o
/// nome que o servidor deu à transmissão.
async fn ligar_a_camera(conexao: &Arc<Connection>, olhos: &Olhos, ja_vistas: usize) -> u32 {
    conexao
        .compartilhar_camera(1280, 720, limites())
        .expect("o pedido da câmera não saiu");
    assert!(
        ate(PRAZO, || olhos.aberturas().len() > ja_vistas).await,
        "o fluxo da câmera não abriu do lado de quem transmite: {:?}",
        olhos.vistos()
    );
    let (tela, largura, altura, tipo) = olhos.aberturas()[ja_vistas];
    assert_eq!(
        (largura, altura, tipo),
        (1280, 720, ScreenKind::Camera),
        "a abertura da própria câmera não diz câmera, ou não traz as medidas de quem codifica"
    );
    tela
}

/// Empurra os quadros no passo de uma câmera a 30 por segundo, e devolve os
/// que entraram.
async fn empurrar(conexao: &Arc<Connection>, marca: u8, quantos: u8) -> Vec<(bool, Vec<u8>)> {
    let mut entraram = Vec::new();
    for n in 0..quantos {
        let chave = n == 0;
        let bytes = quadro_de_mentira(chave, marca.wrapping_add(n), if chave { 900 } else { 240 });
        conexao
            .quadro_da_camera(chave, bytes.clone())
            .unwrap_or_else(|falha| panic!("o quadro {n} não entrou: {falha:?}"));
        entraram.push((chave, bytes));
        // O quadro-chave sai em fatias, uma por intervalo: a janela de verdade
        // também não põe um quadro no meio delas.
        tokio::time::sleep(Duration::from_millis(if chave { 150 } else { 34 })).await;
    }
    entraram
}

/// **A câmera chega a quem assiste dizendo que é câmera, com os mesmos bytes;
/// o pedido de chave volta a quem a codifica; e ninguém recebe a própria.**
#[tokio::test(flavor = "multi_thread")]
async fn a_camera_de_fora_chega_inteira_e_o_pedido_de_chave_volta_a_quem_codifica() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;
    let casas = (tempfile::tempdir()?, tempfile::tempdir()?);
    let (assiste, olhos_de_quem_assiste) = entrar(endereco, casas.0.path(), "assiste").await;
    let (mostra, olhos_de_quem_mostra) = entrar(endereco, casas.1.path(), "mostra").await;
    sentar(&assiste).await;
    sentar(&mostra).await;

    // Sem câmera ligada, a porta dos quadros diz que não há câmera.
    assert_eq!(
        mostra.quadro_da_camera(true, quadro_de_mentira(true, 1, 16)),
        Err(ConnectionError::CameraNotLive)
    );

    let tela = ligar_a_camera(&mostra, &olhos_de_quem_mostra, 0).await;
    assert!(
        olhos_de_quem_mostra
            .vistos()
            .iter()
            .any(|evento| matches!(evento, Event::CameraBudget { screen, bps } if *screen == tela && *bps > 0)),
        "quem codifica não recebeu a banda do fluxo: {:?}",
        olhos_de_quem_mostra.vistos()
    );
    assert_eq!(
        mostra.snapshot().minha_transmissao.map(|minha| minha.tela),
        Some(tela),
        "o retrato de quem ligou a câmera não diz que ela está transmitindo"
    );

    let entraram = empurrar(&mostra, 0x10, 6).await;

    assert!(
        ate(PRAZO, || olhos_de_quem_assiste.quadros(tela).len()
            >= entraram.len())
        .await,
        "os quadros da câmera não chegaram a quem assiste: chegaram {} de {}; eventos {:?}",
        olhos_de_quem_assiste.quadros(tela).len(),
        entraram.len(),
        olhos_de_quem_assiste.aberturas()
    );
    assert_eq!(
        olhos_de_quem_assiste.aberturas(),
        vec![(tela, 1280, 720, ScreenKind::Camera)],
        "quem assiste não ficou sabendo que a transmissão é uma câmera"
    );
    assert_eq!(
        olhos_de_quem_assiste.quadros(tela),
        entraram,
        "os quadros não chegaram byte por byte como entraram, ou chegaram fora de ordem"
    );
    assert!(
        olhos_de_quem_mostra.quadros(tela).is_empty(),
        "quem ligou a câmera recebeu os próprios quadros de volta: a prévia é da janela, e \
         não uma volta pelo núcleo"
    );

    // O pedido de quem assiste chega à casca de quem transmite.
    let antes = olhos_de_quem_mostra.pedidos_de_chave(tela);
    assiste.pedir_quadro_chave(tela).expect("pedir a chave");
    assert!(
        ate(PRAZO, || olhos_de_quem_mostra.pedidos_de_chave(tela)
            > antes)
        .await,
        "o pedido de quadro-chave de quem assiste não chegou à casca de quem codifica a câmera"
    );

    // Parar fecha o fluxo de quem assiste, e a porta dos quadros.
    mostra.parar_de_compartilhar().expect("parar");
    assert!(
        ate(PRAZO, || olhos_de_quem_assiste.fechadas().contains(&tela)).await,
        "quem assiste não viu a câmera fechar"
    );
    assert_eq!(
        mostra.quadro_da_camera(true, quadro_de_mentira(true, 2, 16)),
        Err(ConnectionError::CameraNotLive),
        "um quadro entrou depois de a câmera ter sido desligada"
    );

    daemon.shutdown();
    Ok(())
}

/// Um par cru que entrou falando o protocolo 12 e está sentado na sala.
async fn par_do_doze(endereco: SocketAddr) -> Result<par_cru::Par> {
    let chave = SigningKey::from_bytes(&[12; 32]);
    let (ponta, conexao) = par_cru::discar(endereco).await;
    let (envio, recebe) = conexao.controle().expect("o enlace entrega o ctl uma vez");
    let mut par = par_cru::Par {
        conexao,
        envio,
        recebe,
        ponta,
    };
    let ClientMessage::Hello {
        client,
        nickname,
        public_key,
        join_secret,
        material_e2ee,
        apis_de_mod,
        ..
    } = par_cru::hello(&chave, "do-doze", None)
    else {
        unreachable!("o hello é um Hello");
    };
    frame::write_em(
        &mut par.envio,
        12,
        &ClientMessage::Hello {
            faixa: seele_proto::version::Faixa {
                minima: seele_proto::version::VERSAO_MINIMA,
                maxima: 12,
            },
            client,
            nickname,
            public_key,
            join_secret,
            material_e2ee,
            apis_de_mod,
        },
    )
    .await?;
    let ServerMessage::Challenge { nonce } = par.proximo().await? else {
        anyhow::bail!("o servidor não mandou Challenge");
    };
    let assinatura = par_cru::prova(&chave, &par.conexao, &nonce);
    frame::write_em(
        &mut par.envio,
        12,
        &ClientMessage::Response { proof: assinatura },
    )
    .await?;
    let sala = loop {
        if let ServerMessage::Session { voice_rooms, .. } = par.proximo().await? {
            break voice_rooms.first().expect("uma sala").id;
        }
    };
    frame::write_em(
        &mut par.envio,
        12,
        &ClientMessage::EnterVoiceRoom {
            voice_room: sala,
            password: None,
        },
    )
    .await?;
    Ok(par)
}

/// **Quem assiste pelo protocolo 12 recebe a câmera como monitor**, lê a
/// abertura com o leitor do protocolo, e recebe os mesmos quadros. Sem a
/// reescrita ele recusaria o código 2 e ficaria sem imagem.
#[tokio::test(flavor = "multi_thread")]
async fn quem_assiste_pelo_doze_recebe_a_camera_como_monitor_e_os_mesmos_quadros() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;
    let casa = tempfile::tempdir()?;
    let do_doze = par_do_doze(endereco).await?;
    // O `ctl` dele é lido até o fim numa tarefa: ninguém o lê aqui, e um par
    // que não lê é cortado.
    let par_cru::Par {
        conexao,
        envio: _envio,
        mut recebe,
        ponta: _ponta,
    } = do_doze;
    tokio::spawn(async move { while frame::read::<ServerMessage>(&mut recebe).await.is_ok() {} });

    let (mostra, olhos) = entrar(endereco, casa.path(), "mostra").await;
    sentar(&mostra).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let tela = ligar_a_camera(&mostra, &olhos, 0).await;
    let entraram = empurrar(&mostra, 0x40, 4).await;

    let mut fluxo = tokio::time::timeout(PRAZO, conexao.accept_uni()).await??;
    let mut inicio = vec![0_u8; 1 + SCREEN_HEADER_LEN];
    tokio::time::timeout(PRAZO, fluxo.read_exact(&mut inicio)).await??;
    let (cabecalho, _) = ScreenHeader::decode(&inicio[1..])
        .expect("quem negociou o 12 recebeu uma abertura que não se lê");
    assert_eq!(
        (
            cabecalho.screen.0,
            cabecalho.source,
            cabecalho.width,
            cabecalho.height
        ),
        (tela, ScreenSource::Monitor, 1280, 720),
        "a abertura que chegou ao 12 não é a da câmera com a fonte reescrita como monitor"
    );
    let mut leitor = seele_proto::screen::LeitorDeTela::novo();
    leitor.empurrar(&inicio[1..]).expect("a abertura");
    let mut chegaram = Vec::new();
    let mut pedaco = vec![0_u8; 16 * 1024];
    while chegaram.len() < entraram.len() {
        let lidos = tokio::time::timeout(PRAZO, fluxo.read(&mut pedaco))
            .await??
            .expect("o fluxo acabou antes dos quadros");
        leitor
            .empurrar(&pedaco[..lidos])
            .expect("um pedaço do fluxo");
        while let Some(lido) = leitor.proximo() {
            if let seele_proto::screen::LidoDaTela::Quadro { tipo, bytes } = lido {
                chegaram.push((tipo == seele_proto::screen::TipoDeQuadro::Chave, bytes));
            }
        }
    }
    assert_eq!(
        chegaram, entraram,
        "os quadros não chegaram iguais a quem negociou o 12"
    );

    daemon.shutdown();
    Ok(())
}

/// **Diante de um servidor do 12, a câmera é recusada com o nome do erro, e
/// nada vai ao fio.** Um servidor do 12 anunciaria a transmissão à sala e
/// recusaria o fluxo dela: ficaria desenhada uma tela que não existe.
///
/// O servidor é cru e carimba a sessão com 12. Depois da recusa ele lê o `ctl`
/// e espera um fluxo: nenhum `StartScreenShare` e nenhum fluxo podem chegar.
#[tokio::test(flavor = "multi_thread")]
async fn um_servidor_do_doze_recusa_a_camera_pelo_nome_e_nada_vai_ao_fio() -> Result<()> {
    use seele_proto::control::encode_em;

    let _vaga = vaga::minha();
    let ponta = par_cru::anfitriao().await;
    let endereco = ponta.local_addr();
    let (pronto, esperar_pronto) = tokio::sync::oneshot::channel::<()>();
    let (pode_medir, medir) = tokio::sync::oneshot::channel::<()>();
    let servidor = tokio::spawn(async move {
        let (conexao, mut envia, mut recebe) = par_cru::primeira_chegada(&ponta).await?;
        let doze = |mensagem: &ServerMessage| -> Result<Vec<u8>> {
            Ok(seele_proto::quadro::enquadrar(&encode_em(12, mensagem)?)?)
        };
        let _hello = par_cru::ler_quadro(&mut recebe).await?;
        par_cru::escrever(
            &mut envia,
            &doze(&ServerMessage::Challenge { nonce: vec![7; 32] })?,
        )
        .await?;
        let _prova = par_cru::ler_quadro(&mut recebe).await?;
        par_cru::escrever(
            &mut envia,
            &doze(&ServerMessage::Session {
                id: seele_proto::ids::SessionId(1),
                person: seele_proto::ids::PersonId(1),
                ssrc: seele_proto::ids::Ssrc(1),
                server: "Casa".into(),
                voice_rooms: Vec::new(),
                channels: Vec::new(),
                roles: Vec::new(),
                permissions: Vec::new(),
            })?,
        )
        .await?;
        let _ = pronto.send(());
        // Tudo o que o cliente escrever daqui em diante, até o teste mandar
        // parar e mais um instante.
        let mut escritos = Vec::new();
        let mut medir = medir;
        let mut fim: Option<tokio::time::Instant> = None;
        loop {
            let prazo = fim.map_or(Duration::from_secs(30), |fim| {
                fim.saturating_duration_since(tokio::time::Instant::now())
            });
            tokio::select! {
                quadro = tokio::time::timeout(prazo, par_cru::ler_quadro(&mut recebe)) => match quadro {
                    Ok(Ok(quadro)) => escritos.push(quadro),
                    _ => break,
                },
                _ = &mut medir, if fim.is_none() => {
                    fim = Some(tokio::time::Instant::now() + Duration::from_millis(900));
                }
            }
        }
        let abriu_fluxo = tokio::time::timeout(Duration::from_millis(200), conexao.accept_uni())
            .await
            .is_ok_and(|fluxo| fluxo.is_ok());
        drop(ponta);
        Ok::<_, anyhow::Error>((escritos, abriu_fluxo))
    });

    let casa = tempfile::tempdir()?;
    let (conexao, olhos) = entrar(endereco, casa.path(), "mostra").await;
    esperar_pronto.await?;
    assert!(
        ate(PRAZO, || conexao.protocolo_negociado() == 12).await,
        "a conexão não negociou o 12 com o servidor cru: {}",
        conexao.protocolo_negociado()
    );

    assert_eq!(
        conexao.compartilhar_camera(1280, 720, limites()),
        Err(ConnectionError::ServidorAntigo {
            negociado: 12,
            exige: 13
        }),
        "a câmera não foi recusada pelo nome diante de um servidor do 12"
    );
    assert_eq!(
        conexao.quadro_da_camera(true, quadro_de_mentira(true, 9, 64)),
        Err(ConnectionError::CameraNotLive),
        "depois da recusa ficou uma porta de quadros aberta"
    );
    assert_eq!(
        conexao.snapshot().minha_transmissao,
        None,
        "o retrato diz que há uma transmissão depois de a câmera ter sido recusada"
    );
    let _ = pode_medir.send(());

    let (escritos, abriu_fluxo) =
        tokio::time::timeout(Duration::from_secs(20), servidor).await???;
    let pedidos: Vec<ClientMessage> = escritos
        .iter()
        .filter_map(|quadro| seele_proto::control::decode::<ClientMessage>(quadro).ok())
        .filter(|mensagem| {
            matches!(
                mensagem,
                ClientMessage::StartScreenShare | ClientMessage::StopScreenShare
            )
        })
        .collect();
    assert!(
        pedidos.is_empty(),
        "um pedido de transmissão foi ao fio de um servidor do 12: {pedidos:?}"
    );
    assert!(!abriu_fluxo, "um fluxo foi aberto para um servidor do 12");
    assert!(
        olhos.aberturas().is_empty(),
        "a casca ouviu que a câmera abriu num servidor do 12"
    );
    Ok(())
}

/// **Ligar a câmera com uma no ar troca uma pela outra**, com quadros ainda
/// saindo da primeira: quem assiste vê a primeira fechar, a segunda abrir, e
/// os quadros da segunda — e não uma transmissão anunciada sem imagem.
#[tokio::test(flavor = "multi_thread")]
async fn ligar_de_novo_troca_a_transmissao_e_quem_assiste_recebe_a_nova() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;
    let casas = (tempfile::tempdir()?, tempfile::tempdir()?);
    let (assiste, olhos_de_quem_assiste) = entrar(endereco, casas.0.path(), "assiste").await;
    let (mostra, olhos_de_quem_mostra) = entrar(endereco, casas.1.path(), "mostra").await;
    sentar(&assiste).await;
    sentar(&mostra).await;

    let primeira = ligar_a_camera(&mostra, &olhos_de_quem_mostra, 0).await;
    let _ = empurrar(&mostra, 0x10, 5).await;
    assert!(
        ate(PRAZO, || !olhos_de_quem_assiste
            .quadros(primeira)
            .is_empty())
        .await,
        "a primeira transmissão não chegou a quem assiste"
    );

    let segunda = ligar_a_camera(&mostra, &olhos_de_quem_mostra, 1).await;
    assert_ne!(
        segunda, primeira,
        "a troca reaproveitou o nome da transmissão que parou"
    );
    let entraram = empurrar(&mostra, 0x80, 4).await;

    assert!(
        ate(PRAZO, || olhos_de_quem_assiste
            .fechadas()
            .contains(&primeira))
        .await,
        "quem assiste não viu a transmissão trocada fechar"
    );
    assert!(
        ate(PRAZO, || olhos_de_quem_assiste.quadros(segunda).len()
            >= entraram.len())
        .await,
        "a transmissão nova foi anunciada e não chegou: {} de {} quadros; aberturas {:?}",
        olhos_de_quem_assiste.quadros(segunda).len(),
        entraram.len(),
        olhos_de_quem_assiste.aberturas()
    );
    assert_eq!(olhos_de_quem_assiste.quadros(segunda), entraram);
    assert_eq!(
        mostra.snapshot().minha_transmissao.map(|minha| minha.tela),
        Some(segunda)
    );

    daemon.shutdown();
    Ok(())
}

/// **O que o aplicativo dá à janela é o que o `.wasm` dá à página**: o codec,
/// as medidas, a cadência e a banda de partida. A casca é a mesma nos dois, e
/// uma câmera codificada de um jeito no aplicativo e de outro no navegador
/// seria duas transmissões diferentes com o mesmo nome.
#[test]
fn a_codificacao_da_camera_e_a_mesma_no_aplicativo_e_no_navegador() {
    assert_eq!(
        seele_ffi::CODEC_DA_CAMERA,
        seele_web::tela::CODEC_DO_NAVEGADOR
    );
    for (largura, altura) in [(1280, 720), (1920, 1080), (640, 480), (720, 1280), (0, 0)] {
        for pedidos in [
            limites(),
            LimitesDeTela {
                banda_bps: None,
                altura_maxima: 1080,
                quadros_maximos: 60,
                ..limites()
            },
            LimitesDeTela {
                altura_maxima: 540,
                quadros_maximos: 15,
                ..limites()
            },
        ] {
            let do_aplicativo =
                serde_json::to_value(seele_ffi::codificacao_da_camera(largura, altura, pedidos))
                    .unwrap();
            let do_navegador = seele_web::tela::codificacao(
                &seele_web::tela::limites_da_camera(&pedidos),
                largura,
                altura,
            );
            assert_eq!(
                do_aplicativo, do_navegador,
                "a câmera de {largura}×{altura} com {pedidos:?} é codificada de um jeito no \
                 aplicativo e de outro no navegador"
            );
            assert!(
                do_aplicativo["altura"].as_u64().unwrap() <= 720
                    || largura == 720 && altura == 1280,
                "a câmera passou de 720p: {do_aplicativo}"
            );
            assert!(do_aplicativo["quadros"].as_u64().unwrap() <= 30);
        }
    }
}
