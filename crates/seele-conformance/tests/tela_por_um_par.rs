//! A costura do caminho entre pares, num servidor de verdade com clientes de
//! verdade.
//!
//! # A lacuna que estes testes fecham
//!
//! Nove tarefas construíram as peças, e cada uma tem prova isolada: `Pares`
//! escolhe, `par::ligar` liga, `par::repassar` escreve, `enlace` despacha. **Nenhuma
//! delas prova a costura** — que num servidor de verdade, com conexões de
//! verdade, um quadro sai da máquina de quem empresta a subida e chega à de
//! quem assiste sem passar pelo servidor, e que quando essa máquina morre a
//! imagem continua.
//!
//! É a diferença que o `CLAUDE.md` deste repositório chama de «existir não é
//! funcionar»: cada unidade passava, e a junção não tinha ninguém olhando.
//!
//! # A prova que importa é a negativa
//!
//! Que o quadro chegou, um teste ingênuo prova sem querer: **o servidor sabe
//! servir tela desde agosto**, e um caminho entre pares que não funcione é
//! indistinguível de um que funcione se ninguém olhar de onde os bytes vieram.
//! Por isso toda afirmação aqui é dupla — o quadro bate byte a byte **e** o
//! contador de cópias do servidor diz que aquela cópia não saiu dele. É a mesma
//! forma de prova que `seele-server/tests/subida_no_arranque.rs` usa.
//!
//! # Por que aqui, e não em `seele-server/tests`
//!
//! O plano pedia `crates/seele-server/tests/tela_por_um_par.rs`, e o ADR 0002
//! não permite: o daemon não pode depender do `seele-core`, nem como
//! dependência de desenvolvimento — um teste em `seele-server/tests` que
//! importasse `seele_core::par` faria o `seele-server` linkar o cliente. Este
//! crate existe exatamente para a exceção e lhe dá nome: *«the one crate
//! allowed to see both ends»*. Do plano vem o andaime — `AceitaQualquer`,
//! `servidor_com`, `abrir`, `Par`, copiados de `subida_no_arranque.rs` — e
//! dele vêm as duas pontas de cliente que só aqui cabem juntas.
//!
//! # Quem é quem
//!
//! **Quem compartilha é um par cru**, e é o único dos três que é. A imagem
//! precisa ser byte a byte previsível para a afirmação central existir, e um
//! cliente de verdade transmitindo tela traz captura de vídeo e codificador
//! H.264 junto — nenhum dos dois está sob teste aqui, e os dois produzem bytes
//! que dependem da máquina. **Quem empresta e quem assiste são clientes de
//! verdade**, `seele_core::enlace::Enlace`, porque são eles as duas pontas do
//! caminho que estes testes existem para provar.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use ed25519_dalek::{Signer, SigningKey};
use seele_core::enlace::{Aviso, Destino, Enlace};
use seele_core::{MemoryPinStore, PinStore};
use seele_proto::control::{ClientMessage, ServerMessage};
use seele_proto::ids::{PersonId, ScreenId, VoiceRoomId};
use seele_proto::screen::{ScreenCodec, ScreenHeader, ScreenSource, SCREEN_HEADER_LEN};
use seele_server::persistence::Location;
use seele_server::server::Event;
use seele_server::{frame, Daemon, ServerConfig};

/// Quanto tempo se espera por qualquer coisa antes de dar o teste por falho.
///
/// Generoso porque o caminho entre pares tem prazos próprios — três segundos
/// de discagem em `seele_core::enlace::PRAZO_DO_PAR` — e porque uma máquina de
/// integração carregada é lenta. Um teste que falha por relógio curto é um
/// teste que ensina a ignorá-lo.
const PACIENCIA: Duration = Duration::from_secs(30);

/// De quanto em quanto tempo quem compartilha manda um quadro.
///
/// Trinta e três milissegundos são trinta quadros por segundo, o teto que o §5
/// do desenho de compartilhamento de tela fixa.
const INTERVALO: Duration = Duration::from_millis(33);

/// Um quadro em cada três é quadro-chave.
///
/// **Não é enfeite de realismo: é o que `crate::par::repassar` espera.** Ele
/// descarta todo pedaço até achar o primeiro quadro-chave, porque ligar um par
/// no meio de um quadro é ligá-lo em lixo. Um fluxo só de quadros comuns
/// atravessaria a ligação inteira sem repassar um byte, e o teste passaria a
/// medir o silêncio.
const A_CADA_QUANTOS_UMA_CHAVE: u32 = 3;

/// Um verificador que aceita qualquer certificado.
///
/// O que está sob teste é o caminho entre pares, não o TOFU.
#[derive(Debug)]
struct AceitaQualquer(Arc<rustls::crypto::CryptoProvider>);

impl rustls::client::danger::ServerCertVerifier for AceitaQualquer {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// Um servidor em memória, já atendendo.
///
/// `caminho_bps` é dito, e não deixado em `None`, porque sem ele o teto do
/// §5.1 cai na hipótese de arranque e a admissão de espectadores passa a
/// depender de um número que este teste não escolheu. Dez megabits é folga
/// larga para três pessoas e 256 bytes por quadro, e tira o teto do caminho
/// destas afirmações.
async fn servidor_com(database: Location) -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database,
        caminho_bps: Some(10_000_000),
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

/// Um par cru: a conexão QUIC e o fluxo de controle, sem tarefa leitora.
struct Par {
    conexao: quinn::Connection,
    envio: quinn::SendStream,
    recebe: quinn::RecvStream,
    /// Quem o servidor disse que esta conexão é. Nunca escolhido pelo cliente.
    pessoa: PersonId,
    /// A primeira sala de voz que este par pode ver.
    sala: VoiceRoomId,
}

/// Abre uma conexão e faz o aperto de mão à mão.
async fn abrir(endereco: SocketAddr, semente: u8) -> Result<Par> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let provider = Arc::new(rustls::crypto::ring::default_provider());

    let mut tls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AceitaQualquer(provider)))
        .with_no_client_auth();
    tls.alpn_protocols = vec![seele_proto::transport::ALPN.to_vec()];

    let config = quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls).expect("config do QUIC"),
    ));

    let mut ponta = quinn::Endpoint::client(SocketAddr::from(([127, 0, 0, 1], 0)))?;
    ponta.set_default_client_config(config);
    let conexao = ponta.connect(endereco, "localhost")?.await?;
    // A ponta é dona do socket; sem isto ela cai no fim desta função.
    std::mem::forget(ponta);

    let (mut envio, mut recebe) = conexao.open_bi().await?;
    let chave = SigningKey::from_bytes(&[semente; 32]);
    frame::write(
        &mut envio,
        &ClientMessage::Hello {
            version: seele_proto::PROTOCOL_VERSION,
            client: "par-cru".into(),
            nickname: format!("pessoa{semente:03}"),
            public_key: chave.verifying_key().to_bytes().to_vec(),
            join_secret: None,
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
            proof: chave.sign(&nonce).to_bytes().to_vec(),
        },
    )
    .await?;
    let ServerMessage::Session {
        person,
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
        pessoa: person,
        sala,
    })
}

/// Para onde um cliente de verdade conecta.
fn destino(endereco: SocketAddr, apelido: &str) -> Destino {
    Destino {
        servidor: endereco,
        nome_tls: "localhost".into(),
        chave_do_pin: endereco.to_string(),
        apelido: apelido.to_owned(),
        segredo: None,
        impressao_esperada: None,
    }
}

/// Um cliente de verdade, conectado e sentado na sala.
async fn cliente(endereco: SocketAddr, semente: u8, apelido: &str) -> Result<Enlace> {
    let pins = Arc::new(MemoryPinStore::default());
    let enlace = Enlace::conectar_entre(
        vec![destino(endereco, apelido)],
        SigningKey::from_bytes(&[semente; 32]),
        Arc::clone(&pins) as Arc<dyn PinStore>,
    )
    .await?;
    Ok(enlace)
}

/// O corpo do quadro `seq`, e é a única coisa que quem assiste pode conferir.
///
/// **Cada quadro é diferente do anterior, e é isso que torna a afirmação
/// possível.** Um corpo constante provaria só que *algum* quadro chegou; com o
/// número da sequência dentro dele, quem assiste pode dizer **qual**, e o teste
/// pode exigir um que só existiu depois de o cano do servidor ter sido
/// desligado.
fn corpo(seq: u32) -> Vec<u8> {
    let mut bytes = vec![0_u8; 256];
    if let Some(cabeca) = bytes.get_mut(..4) {
        cabeca.copy_from_slice(&seq.to_be_bytes());
    }
    for (i, byte) in bytes.iter_mut().enumerate().skip(4) {
        // `as` truncando de propósito: o que importa é o padrão depender de
        // `seq`, não o valor em si.
        *byte = (seq as u8).wrapping_add(i as u8);
    }
    bytes
}

/// De que quadro este corpo é, se é de algum.
fn seq_de(bytes: &[u8]) -> Option<u32> {
    let cabeca: [u8; 4] = bytes.get(..4)?.try_into().ok()?;
    let seq = u32::from_be_bytes(cabeca);
    (bytes == corpo(seq)).then_some(seq)
}

/// Os bytes de um quadro no fio: tipo, tamanho, corpo.
///
/// O mesmo enquadramento que `seele_core::tela::Transmissao::enviar_quadro`
/// escreve e que `Recepcao::proximo_quadro` lê — escrito à mão aqui porque quem
/// compartilha, neste teste, é um par cru e não um cliente.
fn quadro(seq: u32) -> Vec<u8> {
    let corpo = corpo(seq);
    let tipo = if seq.is_multiple_of(A_CADA_QUANTOS_UMA_CHAVE) {
        seele_core::tela::TipoDeQuadro::Chave
    } else {
        seele_core::tela::TipoDeQuadro::Comum
    };
    let tamanho = u32::try_from(corpo.len()).expect("um quadro de teste cabe num u32");
    let mut fora = Vec::with_capacity(5 + corpo.len());
    fora.push(tipo.byte());
    fora.extend_from_slice(&tamanho.to_be_bytes());
    fora.extend_from_slice(&corpo);
    fora
}

/// Põe este par cru a transmitir uma tela, e devolve o nome que o servidor deu
/// a ela.
///
/// A transmissão continua numa tarefa própria até a conexão morrer: quem
/// assiste precisa de quadros **novos** depois de cada mudança de caminho, e
/// uma rajada finita acabaria antes de a afirmação poder ser feita.
async fn compartilhar(par: &mut Par) -> Result<ScreenId> {
    frame::write(&mut par.envio, &ClientMessage::StartScreenShare).await?;
    let screen = loop {
        match frame::read::<ServerMessage>(&mut par.recebe).await? {
            ServerMessage::ScreenShareStarted { person, screen, .. } if person == par.pessoa => {
                break screen;
            }
            _ => continue,
        }
    };

    let mut fluxo = par.conexao.open_uni().await?;
    fluxo
        .write_all(&[seele_proto::stream::StreamType::Screen.byte()])
        .await?;
    let mut abertura = [0_u8; SCREEN_HEADER_LEN];
    ScreenHeader {
        version: seele_proto::PROTOCOL_VERSION,
        screen,
        source: ScreenSource::Monitor,
        codec: ScreenCodec::H264Baseline,
        width: 640,
        height: 360,
    }
    .encode(&mut abertura)?;
    fluxo.write_all(&abertura).await?;

    tokio::spawn(async move {
        let mut seq = 0_u32;
        loop {
            if fluxo.write_all(&quadro(seq)).await.is_err() {
                return;
            }
            seq = seq.wrapping_add(1);
            tokio::time::sleep(INTERVALO).await;
        }
    });
    Ok(screen)
}

/// Espera até que a condição valha, ou até a paciência acabar.
///
/// A alternativa — dormir um tanto e afirmar — é a que produz o teste que
/// passa na máquina de quem o escreveu.
async fn ate<F: FnMut() -> bool>(o_que: &str, mut condicao: F) -> Result<()> {
    let fim = Instant::now() + PACIENCIA;
    while Instant::now() < fim {
        if condicao() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    anyhow::bail!("a paciência acabou esperando: {o_que}")
}

/// Lê avisos deste cliente até um deles satisfazer `quero`, e devolve o que ele
/// devolveu.
///
/// Drenar é obrigatório e não conveniência: os quadros chegam pelo mesmo canal
/// de avisos que todo o resto, e um cliente que ninguém lê acumula a
/// transmissão inteira na memória.
async fn esperar<T, F: FnMut(&Aviso) -> Option<T>>(
    enlace: &mut Enlace,
    o_que: &str,
    mut quero: F,
) -> Result<T> {
    let fim = Instant::now() + PACIENCIA;
    while Instant::now() < fim {
        let Ok(aviso) = tokio::time::timeout(Duration::from_millis(500), enlace.proximo()).await
        else {
            continue;
        };
        if let Some(achado) = quero(&aviso) {
            return Ok(achado);
        }
    }
    anyhow::bail!("a paciência acabou esperando: {o_que}")
}

/// Quantas cópias desta transmissão o servidor está subindo, segundo ele mesmo.
///
/// **É o contador que faz a prova ser negativa.** Ele conta os canos ligados —
/// `VoiceRoom::copias` —, e o servidor o anuncia a cada mudança pelo
/// [`Event::ScreenViewers`]. Uma cópia que o servidor não sobe é uma cópia que
/// só pode ter vindo de outro lugar.
struct ContadorDeCopias {
    quantos: Arc<std::sync::atomic::AtomicU32>,
}

impl ContadorDeCopias {
    /// Passa a seguir o contador desta transmissão.
    fn de(servidor: &Daemon, screen: ScreenId) -> Self {
        let quantos = Arc::new(std::sync::atomic::AtomicU32::new(u32::MAX));
        let escrevendo = Arc::clone(&quantos);
        let mut eventos = servidor.server().events.subscribe();
        tokio::spawn(async move {
            while let Ok(evento) = eventos.recv().await {
                if let Event::ScreenViewers {
                    screen: qual,
                    quantos,
                    ..
                } = evento
                {
                    if qual == screen {
                        escrevendo.store(quantos, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            }
        });
        Self { quantos }
    }

    /// O último número que o servidor anunciou. `u32::MAX` enquanto não houve
    /// nenhum — um valor que nenhuma asserção deste teste aceita por engano.
    fn agora(&self) -> u32 {
        self.quantos.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// O cenário inteiro montado: servidor, quem compartilha, quem empresta, quem
/// assiste, e a transmissão no ar chegando aos dois pelo servidor.
struct Cenario {
    servidor: Arc<Daemon>,
    compartilha: Par,
    empresta: Enlace,
    assiste: Enlace,
    screen: ScreenId,
    copias: ContadorDeCopias,
}

/// Monta o cenário até o ponto em que os dois clientes já veem a tela **pelo
/// servidor**, e ninguém pediu par nenhum ainda.
async fn cenario() -> Result<Cenario> {
    let (endereco, servidor) = servidor_com(Location::Memory).await?;

    let mut compartilha = abrir(endereco, 1).await?;
    let sala = compartilha.sala;
    frame::write(
        &mut compartilha.envio,
        &ClientMessage::EnterVoiceRoom {
            voice_room: sala,
            password: None,
        },
    )
    .await?;

    let empresta = cliente(endereco, 2, "empresta").await?;
    empresta.entrar_na_voice_room(sala).await?;
    // **O opt-in, e é ele que põe esta máquina na malha.** Sem esta linha
    // `Pares::escolher` não tem candidato nenhum e o servidor serve as duas
    // cópias, como sempre serviu.
    empresta.emprestar_subida(true).await?;

    let assiste = cliente(endereco, 3, "assiste").await?;
    assiste.entrar_na_voice_room(sala).await?;

    let mut empresta = empresta;
    let mut assiste = assiste;
    let screen = compartilhar(&mut compartilha).await?;
    let copias = ContadorDeCopias::de(&servidor, screen);

    // Os dois entram ligados no cano do servidor, porque é o que
    // `VoiceRoom::tela_abriu` faz com quem já está na sala quando há uma
    // transmissão só. É deste estado — o de antes da malha — que a afirmação
    // seguinte tem de sair.
    esperar(
        &mut empresta,
        "quem empresta ver a tela pelo servidor",
        |aviso| matches!(aviso, Aviso::TelaAbriu { tela, .. } if *tela == screen).then_some(()),
    )
    .await?;
    esperar(
        &mut assiste,
        "quem assiste ver a tela pelo servidor",
        |aviso| matches!(aviso, Aviso::TelaAbriu { tela, .. } if *tela == screen).then_some(()),
    )
    .await?;
    ate(
        "o servidor contar as duas cópias que ele mesmo sobe",
        || copias.agora() == 2,
    )
    .await?;

    Ok(Cenario {
        servidor,
        compartilha,
        empresta,
        assiste,
        screen,
        copias,
    })
}

/// O maior `seq` de tela já enfileirado nos avisos deste cliente, sem esperar
/// nenhum quadro novo.
///
/// **Frisar em tentativas curtas, e não varrer uma vez só.** O canal de
/// avisos é FIFO e o produtor (`compartilhar`) nunca para de mandar quadro
/// novo — a cada [`INTERVALO`]. Parar de drenar por causa de uma tentativa
/// que não achou nada dentro da janela é a única forma de saber que o canal
/// está momentaneamente vazio; ele nunca fica vazio *para sempre* enquanto a
/// transmissão durar.
async fn maior_seq_ja_enfileirado(enlace: &mut Enlace, screen: ScreenId) -> Option<u32> {
    let mut maior = None;
    loop {
        let Ok(aviso) = tokio::time::timeout(Duration::from_millis(20), enlace.proximo()).await
        else {
            return maior;
        };
        if let Aviso::TelaQuadro { tela, bytes, .. } = aviso {
            if tela == screen {
                if let Some(seq) = seq_de(&bytes) {
                    maior = Some(maior.map_or(seq, |atual: u32| atual.max(seq)));
                }
            }
        }
    }
}

/// Quantos quadros seguidos, todos pelo par, provam recepção sustentada — e
/// não um quadro isolado que ainda coubesse num punhado em voo.
///
/// A trinta quadros por segundo (`INTERVALO`), é um segundo de vídeo. Um cano
/// que o servidor desligou não entrega um segundo de imagem por acidente.
const QUADROS_PARA_PROVAR: u32 = 30;

/// O quadro chega ao segundo cliente **pelo primeiro**, e o servidor não o subiu.
///
/// # A prova que importa é a negativa
///
/// Que o quadro chegou, um teste ingênuo prova sem querer: o servidor sabe
/// servir tela desde agosto, e um caminho novo que não funcione é
/// indistinguível de um que funcione se ninguém olhar de onde os bytes vieram.
/// Então a asserção é dupla: o quadro bate byte a byte **e** o contador do
/// servidor diz que aquela cópia não saiu dele. É a mesma forma de prova que
/// `subida_no_arranque.rs` usa.
///
/// # Por que a prova de tempo não é "o quadro que só existiu depois"
///
/// Uma versão anterior deste teste median o tempo entre `assistir()` e o
/// quadro chegar, e chamava um quadro rápido demais de prova. Estava errada:
/// `esperar` lê um canal FIFO, e o canal já tinha o quadro 0 na fila — o
/// servidor o entregou muito antes de `assistir()` ser chamado, enquanto
/// ainda servia as duas cópias. Um `Instant::now()` medido depois de
/// `assistir()` limita **quanto se espera**, nunca **de onde o quadro veio**;
/// um teste que só olhasse esse relógio passava em microssegundos com um
/// `pop` de fila em memória, não com rede nenhuma atravessada.
///
/// A prova de verdade tem duas pernas. **Primeiro**, um piso: antes de pedir
/// para assistir, [`maior_seq_ja_enfileirado`] drena o que já chegou e guarda
/// o maior `seq` visto — só um `seq` estritamente maior que esse piso pode
/// ter atravessado depois do corte. **Segundo**, sustentação: um `seq` isolado
/// acima do piso ainda cabe num candidato a mais em voo (a discagem
/// simultânea dos dois lados admite isso, de propósito). [`QUADROS_PARA_PROVAR`]
/// quadros seguidos — um segundo inteiro —, cada um estritamente maior que o
/// anterior e com o contador do servidor em uma cópia o tempo todo, não cabem
/// em voo nenhum: só cabem vindo de um cano que o servidor não subiu.
#[tokio::test(flavor = "multi_thread")]
async fn o_quadro_chega_pelo_par_e_o_servidor_nao_o_subiu() -> Result<()> {
    let Cenario {
        servidor,
        compartilha,
        empresta,
        mut assiste,
        screen,
        copias,
    } = cenario().await?;

    // **O vazamento de privacidade, preso no ponto de chamada.** `enlace.rs`
    // decide o que publicar em `locais_a_publicar`, e a Task 8 provou a função
    // sem provar quem a chama. Aqui as duas declarações que existem no servidor
    // vieram de dois clientes de verdade que passaram por aquela linha: quem
    // empresta publicou endereços de rede local, e quem só assiste publicou
    // exatamente um endereço — o público que o servidor **viu**, e que
    // `Pares::declarou` acrescenta sozinho. Um `locais_a_publicar` chamado com
    // `true` fixo publicaria a topologia interna de quem nunca optou por nada.
    //
    // Os `PersonId` vêm de `.sessao().person`, e não escritos à mão: os dois
    // são clientes de verdade, e a ordem em que o servidor lhes atribuiu
    // identidade não é contrato — escrevê-los à mão prenderia esta afirmação
    // a essa ordem por acidente.
    {
        let empresta_quem = empresta.sessao().person;
        let assiste_quem = assiste.sessao().person;
        let pares = servidor.server().pares.lock().await;
        let quem_assiste = pares
            .declaracao_de(assiste_quem)
            .expect("quem assiste conectou e não declarou identidade nenhuma")
            .clone();
        let quem_empresta = pares
            .declaracao_de(empresta_quem)
            .expect("quem empresta optou por emprestar e não declarou identidade")
            .clone();
        assert!(
            quem_empresta.emprestando,
            "quem chamou `emprestar_subida(true)` chegou ao servidor como quem não empresta"
        );
        assert!(
            !quem_assiste.emprestando,
            "quem nunca optou por emprestar chegou ao servidor como quem empresta"
        );
        assert_eq!(
            quem_assiste.enderecos.len(),
            1,
            "quem só assiste publicou {:?} — o opt-in de `locais_a_publicar` não valeu no \
             ponto de chamada, e a topologia de rede interna desta máquina foi para o servidor \
             sem ninguém ter pedido",
            quem_assiste.enderecos
        );
    }

    // O piso: nenhum quadro daqui para trás prova nada sobre o par, porque
    // todos ainda podem ter vindo do servidor.
    let piso_inicial = maior_seq_ja_enfileirado(&mut assiste, screen).await;

    // Quem assiste pede para ver, e é aqui que o servidor escolhe o par.
    assiste.assistir(screen, true).await?;

    ate("o servidor parar de subir a cópia de quem assiste", || {
        copias.agora() == 1
    })
    .await?;

    // A prova de verdade: `QUADROS_PARA_PROVAR` seguidos, cada um acima do
    // piso anterior (o próprio piso avança a cada quadro aceito, então dois
    // quadros iguais ou fora de ordem não colam), e o servidor contando uma
    // cópia só do primeiro ao último.
    let mut piso = piso_inicial;
    for indice in 0..QUADROS_PARA_PROVAR {
        let (seq, bytes) = esperar(
            &mut assiste,
            "um quadro chegar pelo par, estritamente depois do piso, com o cano do servidor \
             desligado",
            |aviso| match aviso {
                Aviso::TelaQuadro { tela, bytes, .. } if *tela == screen => seq_de(bytes)
                    .filter(|seq| piso.is_none_or(|p| *seq > p))
                    .map(|seq| (seq, bytes.clone())),
                _ => None,
            },
        )
        .await?;
        assert_eq!(
            bytes,
            corpo(seq),
            "o quadro {seq} chegou e não é o que saiu: os bytes não batem"
        );
        assert_eq!(
            copias.agora(),
            1,
            "o servidor voltou a subir a cópia de quem assiste no meio da recepção sustentada \
             (quadro {indice} de {QUADROS_PARA_PROVAR}, seq {seq}): este quadro pode ter vindo \
             dele, e a malha não provou nada"
        );
        piso = Some(seq);
    }
    println!(
        "{QUADROS_PARA_PROVAR} quadros seguidos chegaram a quem assiste, todos com seq acima de \
         {piso_inicial:?}, com o servidor subindo uma cópia só do primeiro ao último"
    );

    // Sem isto os clientes caem antes do servidor e o desligamento vira corrida.
    drop(compartilha);
    drop(empresta);
    drop(assiste);
    servidor.shutdown();
    Ok(())
}

/// O par morre no meio, e quem estava atrás dele continua vendo.
///
/// # A propriedade de segurança, e por que ela não é adiável
///
/// *«A malha é alívio, nunca dependência»* — é o que quem desenhou o produto
/// escolheu, e é a única razão pela qual emprestar subida pode ser oferecido a
/// quem quer que seja: a máquina de outra pessoa caindo não pode custar a
/// imagem de ninguém. Uma propriedade de segurança provada depois é uma que
/// passou um tempo sem existir.
///
/// # Os dois pontos de chamada que este teste prende
///
/// O primeiro é o de quem assiste: `escoar_tela_alheia` tem de saber que um
/// fluxo de par cortado no meio é um `ParFalhou`, e não o fim normal de uma
/// transmissão. O segundo é o do servidor: o braço de `ParFalhou` tem de
/// **religar o cano** de quem relatou, e não só registrar o relato — quem
/// relata está sem cano nenhum desde que o par foi apontado.
#[tokio::test(flavor = "multi_thread")]
async fn quando_o_par_morre_o_servidor_assume_e_ninguem_perde_imagem() -> Result<()> {
    let Cenario {
        servidor,
        compartilha,
        empresta,
        mut assiste,
        screen,
        copias,
    } = cenario().await?;

    assiste.assistir(screen, true).await?;
    ate("o servidor parar de subir a cópia de quem assiste", || {
        copias.agora() == 1
    })
    .await?;
    let (pelo_par, _) = esperar(
        &mut assiste,
        "o primeiro quadro pelo par",
        |aviso| match aviso {
            Aviso::TelaQuadro { tela, bytes, .. } if *tela == screen => {
                seq_de(bytes).map(|seq| (seq, ()))
            }
            _ => None,
        },
    )
    .await?;
    println!("o quadro {pelo_par} chegou pelo par; agora quem empresta morre");

    // **À força, e não com `sair()`.** Uma despedida limpa é o caso fácil: o
    // servidor vê a sessão acabar e podia limpar sozinho. O que a propriedade
    // promete é o caso difícil — a máquina de alguém sumindo —, e é ele que
    // este `drop` produz: `Enlace::drop` aborta a tarefa que fala com o
    // servidor e larga a conexão QUIC no meio de uma transmissão.
    drop(empresta);

    // O quadro que prova a promessa é um **posterior** ao último que o par
    // entregou. Um que já estivesse na fila de avisos não provaria nada.
    let alvo = pelo_par + 1;
    let (seq, bytes) = esperar(
        &mut assiste,
        "um quadro chegar pelo servidor depois de o par morrer",
        |aviso| match aviso {
            Aviso::TelaQuadro { tela, bytes, .. } if *tela == screen => seq_de(bytes)
                .filter(|seq| *seq > alvo)
                .map(|seq| (seq, bytes.clone())),
            _ => None,
        },
    )
    .await?;
    println!("o quadro {seq} chegou pelo servidor depois de o par ter morrido");
    assert_eq!(
        bytes,
        corpo(seq),
        "o servidor assumiu e o que ele entregou não é o que saiu de quem compartilha"
    );

    // E a nomeação não sobreviveu ao par: um `ParFalhou` seguinte se resolveria
    // contra um par que já não serve ninguém.
    let apontado = servidor
        .server()
        .pares
        .lock()
        .await
        .quem_foi_apontado(screen);
    assert_eq!(
        apontado, None,
        "o par morreu e o servidor continua com a nomeação dele de pé"
    );

    drop(compartilha);
    drop(assiste);
    servidor.shutdown();
    Ok(())
}

/// Um `ParFalhou` por impressão que não bate desacredita **quem foi apontado**,
/// e nunca quem relatou.
///
/// # O ponto de chamada que faltava
///
/// A revisão da Task 8 achou três consertos cujos guardas estavam sobre a
/// função extraída e **nenhum sobre o ponto de chamada**. `quem_desacreditar`
/// é a função pura, e `seele-server/src/session.rs` é quem a chama: até esta
/// tarefa, nenhuma nomeação existia em produção, então o braço de `ParFalhou`
/// resolvia sempre contra `None` e o defeito que o round 2 consertou —
/// `saiu(session.person)`, que apaga a **vítima** — podia voltar sem nenhum
/// teste tropeçar.
///
/// Aqui a nomeação é de verdade: o servidor apontou um par por `WatchScreen`.
/// Quem relata é quem assiste, e quem tem de desaparecer de `Pares` é quem
/// empresta.
///
/// # Por que quem assiste é um par cru aqui
///
/// Porque `ImpressaoNaoBate` é o que um cliente de verdade manda quando alguém
/// responde no lugar do par apontado, e montar uma impostura de verdade contra
/// um par legítimo seria construir o ataque para testar a reação a ele. O que
/// está sob teste é o braço do servidor, e ele lê a mesma mensagem venha ela de
/// onde vier.
#[tokio::test(flavor = "multi_thread")]
async fn um_parfalhou_por_impressao_desacredita_o_par_apontado_e_nao_a_vitima() -> Result<()> {
    let (endereco, servidor) = servidor_com(Location::Memory).await?;

    let mut compartilha = abrir(endereco, 1).await?;
    let sala = compartilha.sala;
    frame::write(
        &mut compartilha.envio,
        &ClientMessage::EnterVoiceRoom {
            voice_room: sala,
            password: None,
        },
    )
    .await?;

    let empresta = cliente(endereco, 2, "empresta").await?;
    empresta.entrar_na_voice_room(sala).await?;
    empresta.emprestar_subida(true).await?;

    // Quem assiste é cru, e declara a própria identidade à mão — é o que
    // `Enlace` faz por dentro ao conectar, e sem ela o servidor não teria
    // impressão de quem pediu para apresentar a quem empresta.
    let mut assiste = abrir(endereco, 3).await?;
    frame::write(
        &mut assiste.envio,
        &ClientMessage::EnterVoiceRoom {
            voice_room: sala,
            password: None,
        },
    )
    .await?;
    frame::write(
        &mut assiste.envio,
        &ClientMessage::EmprestarSubida {
            emprestando: false,
            impressao: "a".repeat(64),
            locais: Vec::new(),
        },
    )
    .await?;

    let screen = compartilhar(&mut compartilha).await?;
    let empresta_quem = empresta.sessao().person;
    let assiste_quem = assiste.pessoa;

    ate("as duas declarações chegarem ao servidor", || true).await?;
    // Quem empresta tem de estar declarado antes do pedido: `Pares::escolher`
    // não tem candidato nenhum antes disso, e o teste mediria o caminho de
    // sempre pensando estar medindo a malha.
    {
        let fim = Instant::now() + PACIENCIA;
        loop {
            let pronto = {
                let pares = servidor.server().pares.lock().await;
                pares.declaracao_de(empresta_quem).is_some()
                    && pares.declaracao_de(assiste_quem).is_some()
            };
            if pronto || Instant::now() >= fim {
                assert!(pronto, "as declarações não chegaram ao servidor");
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    frame::write(&mut assiste.envio, &ClientMessage::WatchScreen { screen }).await?;
    let fim = Instant::now() + PACIENCIA;
    loop {
        let apontado = servidor
            .server()
            .pares
            .lock()
            .await
            .quem_foi_apontado(screen);
        if apontado == Some(empresta_quem) {
            break;
        }
        assert!(
            Instant::now() < fim,
            "o servidor não apontou quem empresta para servir esta transmissão (apontou {apontado:?})"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    frame::write(
        &mut assiste.envio,
        &ClientMessage::ParFalhou {
            screen,
            motivo: seele_proto::control::MotivoDeFalhaDePar::ImpressaoNaoBate,
        },
    )
    .await?;

    let fim = Instant::now() + PACIENCIA;
    loop {
        let (empresta_ainda, assiste_ainda) = {
            let pares = servidor.server().pares.lock().await;
            (
                pares.declaracao_de(empresta_quem).is_some(),
                pares.declaracao_de(assiste_quem).is_some(),
            )
        };
        assert!(
            assiste_ainda,
            "o servidor apagou a declaração de **quem relatou** — a vítima paga pelo que outro fez, \
             e é exatamente o defeito que o fix round 2 consertou e que nada em produção prendia"
        );
        if !empresta_ainda {
            break;
        }
        assert!(
            Instant::now() < fim,
            "o par apontado foi acusado de impostura e continua declarado no servidor"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    drop(compartilha);
    drop(empresta);
    drop(assiste);
    servidor.shutdown();
    Ok(())
}
