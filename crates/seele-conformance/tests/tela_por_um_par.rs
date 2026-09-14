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
use seele_core::enlace::{Aviso, Destino, Enlace, PARES_QUE_ESTA_VERSAO_ATENDE};
use seele_core::{Link, MemoryPinStore, PinStore};
use seele_proto::control::{ClientMessage, ConsentimentoDePar, ServerMessage};
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
    servidor_em(0, database).await
}

/// O mesmo, numa porta escolhida — **a mesma na segunda vez**.
///
/// É o que `bateria_interna.rs` faz para que um reinício seja uma *reconexão* e
/// não um destino novo: o cliente volta ao endereço que atendeu, e o `Destino`
/// dele não muda. Com o banco em [`Location::File`], o servidor que sobe
/// apresenta **o mesmo certificado** — sem isso o TOFU do cliente veria troca de
/// chave, que é o alerta do ADR 0003 e o contrário de uma reconexão.
async fn servidor_em(porta: u16, database: Location) -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], porta)),
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
        aceito: None,
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

/// De quanto em quanto começa cada geração de mídia.
///
/// # Por que gerações, e não o `ScreenId`
///
/// Porque **o `ScreenId` não distingue as duas**. Ele é emitido por
/// `Registry::issue_screen`, um contador em memória do daemon: um servidor que
/// reinicia recomeça a numerar do mesmo lugar, e a transmissão que abre depois
/// da volta ganha, muito provavelmente, **o mesmo nome** que a de antes da
/// queda. Duas mídias diferentes com o mesmo rótulo é a receita exata do
/// falso-verde que este arquivo já pagou duas vezes: um quadro velho, entregue
/// por uma tarefa que sobreviveu à conexão que a criou, passaria por prova de
/// recuperação.
///
/// A marca, então, vai no **corpo** do quadro, que é a única coisa que quem
/// assiste confere byte a byte — ver [`corpo`]. Cada geração começa a numerar
/// um milhão acima da anterior, folga larga sobre os poucos milhares de quadros
/// que um teste destes chega a produzir, e [`geracao_de`] lê a marca de volta.
const PASSO_DA_GERACAO: u32 = 1_000_000;

/// O primeiro `seq` desta geração de mídia.
fn primeiro_seq_da(geracao: u32) -> u32 {
    geracao * PASSO_DA_GERACAO
}

/// De que geração de mídia este `seq` é.
fn geracao_de(seq: u32) -> u32 {
    seq / PASSO_DA_GERACAO
}

/// Põe este par cru a transmitir uma tela, e devolve o nome que o servidor deu
/// a ela.
///
/// A transmissão continua numa tarefa própria até a conexão morrer: quem
/// assiste precisa de quadros **novos** depois de cada mudança de caminho, e
/// uma rajada finita acabaria antes de a afirmação poder ser feita.
async fn compartilhar(par: &mut Par) -> Result<ScreenId> {
    compartilhar_desde(par, primeiro_seq_da(0)).await
}

/// O mesmo, numerando os quadros a partir de `primeiro`.
///
/// Existe para [`PASSO_DA_GERACAO`]: a segunda transmissão de um teste de
/// reinício precisa produzir imagem que **não se confunda** com a da primeira,
/// e o `ScreenId` não serve para isso.
async fn compartilhar_desde(par: &mut Par, primeiro: u32) -> Result<ScreenId> {
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
        let mut seq = primeiro;
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
    /// Passa a seguir o contador desta transmissão, a partir de uma assinatura
    /// **já feita** do barramento.
    ///
    /// **A assinatura é um argumento, e não uma conveniência que este construtor
    /// faça por conta própria.** Houve uma versão que assinava aqui dentro, e
    /// quem chamava assinava depois de abrir a transmissão: o servidor anuncia
    /// `ScreenViewers` uma vez ao ligar os espectadores no cano e não repete, e
    /// uma assinatura tardia perdia esse anúncio único — o contador ficava em
    /// `u32::MAX` para sempre e a espera de quem chamava esgotava a paciência
    /// inteira sem nada de errado ter acontecido. Medido em 2026-09-11: um
    /// atraso de trezentos milissegundos entre abrir a transmissão e assinar
    /// reproduzia o vermelho falso todas as vezes. Pedir a assinatura pronta
    /// obriga quem chama a fazê-la antes, que é o único momento em que ela é
    /// segura.
    ///
    /// Serve também a
    /// [`o_contador_de_copias_sobrevive_a_um_atraso_do_barramento`], que
    /// entrega um recebedor **já atrasado** — coisa que um daemon de verdade
    /// não sabe produzir sob encomenda.
    fn seguindo(mut eventos: tokio::sync::broadcast::Receiver<Event>, screen: ScreenId) -> Self {
        let quantos = Arc::new(std::sync::atomic::AtomicU32::new(u32::MAX));
        let escrevendo = Arc::clone(&quantos);
        tokio::spawn(async move {
            loop {
                // **`Lagged` não encerra a contagem.** O barramento do servidor
                // larga eventos quando quem lê fica para trás, e um `while let
                // Ok(..)` trataria essa perda como fim de barramento: o laço
                // sairia e `agora()` congelaria no último número visto. Um
                // contador congelado em `1` afirma para sempre a única coisa
                // que estes testes existem para provar. Perder eventos custa
                // precisão; parar de ler custa a prova.
                match eventos.recv().await {
                    Ok(evento) => {
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
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
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

/// Quem consente em **emprestar a conexão**: aceita subir cópias para outras
/// pessoas, até o teto desta versão do cliente.
const EMPRESTA: ConsentimentoDePar = ConsentimentoDePar {
    pares_que_atende: PARES_QUE_ESTA_VERSAO_ATENDE,
    assiste_por_par: false,
};

/// Quem consente em **assistir por par**: aceita que o próprio endereço seja
/// entregue a quem for servi-lo.
///
/// As duas metades do §5 são independentes de propósito, e estas duas
/// constantes existem separadas para que os testes digam qual delas estão
/// exercitando. Quem empresta aqui não assiste por par, e vice-versa: nenhum
/// teste deste arquivo depende de as duas virem juntas.
const ASSISTE_POR_PAR: ConsentimentoDePar = ConsentimentoDePar {
    pares_que_atende: 0,
    assiste_por_par: true,
};

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
    cenario_sobre(endereco, servidor, 0).await
}

/// O mesmo, sobre um servidor que quem chama já subiu, e com a mídia marcada.
///
/// Separado de [`cenario`] para o teste do reinício, que precisa das duas coisas
/// que aquele fixa: um servidor com **porta e banco escolhidos** (ver
/// [`servidor_em`]) e imagem de uma **geração** identificável (ver
/// [`PASSO_DA_GERACAO`]). Tudo o que vem depois desta linha é igual para os dois
/// — é o mesmo cenário, e não um parecido.
async fn cenario_sobre(
    endereco: SocketAddr,
    servidor: Arc<Daemon>,
    geracao: u32,
) -> Result<Cenario> {
    cenario_por(endereco, endereco, servidor, geracao).await
}

/// O mesmo, com quem assiste chegando **por outro endereço**.
///
/// Existe para [`Rele`]: a queda assimétrica precisa que só o caminho de quem
/// assiste até o servidor possa ser cortado, e quem compartilha e quem empresta
/// continuem falando com ele direto. Os dois endereços levam ao mesmo servidor;
/// o que muda é por onde os pacotes de uma das três conexões passam.
async fn cenario_por(
    endereco: SocketAddr,
    endereco_de_quem_assiste: SocketAddr,
    servidor: Arc<Daemon>,
    geracao: u32,
) -> Result<Cenario> {
    cenario_com(
        endereco,
        endereco_de_quem_assiste,
        servidor,
        geracao,
        ASSISTE_POR_PAR,
    )
    .await
}

/// O mesmo cenário, com quem assiste declarando **o consentimento que se
/// pede** em vez do de sempre.
///
/// Existe para o teste da recusa: o aceite diz que «recusa mantém caminho pelo
/// servidor», e provar isso precisa de uma pessoa que declare identidade de par
/// — para que a declaração exista e possa ser consultada — e mesmo assim
/// **negue** `assiste_por_par`. Passar [`EMPRESTA`] aqui é exatamente essa
/// pessoa: está na malha, empresta a própria subida, e não quer o próprio
/// endereço entregue a ninguém.
async fn cenario_com(
    endereco: SocketAddr,
    endereco_de_quem_assiste: SocketAddr,
    servidor: Arc<Daemon>,
    geracao: u32,
    consentimento_de_quem_assiste: ConsentimentoDePar,
) -> Result<Cenario> {
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
    empresta.entrar_na_voice_room(sala, None).await?;
    // **O opt-in de quem empresta, e é ele que põe esta máquina na malha.**
    // Sem esta linha `Pares::escolher` não tem candidato nenhum e o servidor
    // serve as duas cópias, como sempre serviu.
    empresta.consentir_no_caminho_entre_pares(EMPRESTA).await?;

    let assiste = cliente(endereco_de_quem_assiste, 3, "assiste").await?;
    assiste.entrar_na_voice_room(sala, None).await?;
    // **O opt-in de quem assiste, e ele é a metade nova do §5.** `SirvaTelaPara`
    // entrega o endereço de quem pediu a quem vai servi-lo: sem este
    // consentimento, o endereço de quem assiste seria publicado por uma decisão
    // que **outra pessoa** tomou — a de emprestar. Com esta linha comentada,
    // todo teste de repasse deste arquivo reprova, e é essa a prova de que o
    // consentimento precede a exposição do endereço.
    assiste
        .consentir_no_caminho_entre_pares(consentimento_de_quem_assiste)
        .await?;

    let mut empresta = empresta;
    let mut assiste = assiste;
    // **A assinatura do barramento vem antes de a transmissão existir.** O
    // servidor anuncia `ScreenViewers` **uma vez** quando liga os dois no cano,
    // e não repete: um contador que assinasse depois podia perder esse anúncio
    // único e ficar em `u32::MAX` para sempre, e a espera logo abaixo então
    // esgotava a paciência inteira sem nada de errado ter acontecido. Era um
    // vermelho falso, e um vermelho falso ensina a ignorar a suíte.
    let ouve = servidor.server().events.subscribe();
    let screen = compartilhar_desde(&mut compartilha, primeiro_seq_da(geracao)).await?;
    let copias = ContadorDeCopias::seguindo(ouve, screen);

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

/// Quanto se admite de intervalo entre quem empresta sair da sala e a imagem de
/// quem estava atrás dele voltar.
///
/// **Medido, e não escolhido.** Com o servidor reabrindo o cano na saída, o
/// primeiro quadro volta em dezenas de milissegundos. Sem essa linha, quem
/// ficou órfão espera o prazo do par do outro lado — `PRAZO_DO_PAR`, três
/// segundos em `seele_core::enlace` — antes de qualquer coisa acontecer. Um
/// segundo fica confortavelmente entre os dois: folgado o bastante para uma
/// máquina de integração carregada, curto o bastante para não caber o prazo que
/// esta entrega existe para evitar.
const GRACA_DA_SAIDA: Duration = Duration::from_secs(1);

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
            quem_empresta.consentimento.empresta_conexao(),
            "quem consentiu em emprestar a conexão chegou ao servidor como quem não empresta"
        );
        assert!(
            !quem_assiste.consentimento.empresta_conexao(),
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
///
/// # Qual mecanismo este teste prende, medido e não suposto
///
/// **Não é o braço `Err`.** A dispensa de fechamento mandava provar este teste
/// contra o `send(ParFalhou { CaiuNoMeio })` do braço `Err` de
/// `ler_a_tela_alheia`, e medindo se vê que aquele braço **nunca corre neste
/// cenário**: com ele removido e todo o resto de pé, o teste — já na forma
/// forte — passa. A máquina de quem empresta sumindo chega a quem assiste como
/// um fluxo que **termina limpo**, e não como erro de leitura: a conexão QUIC
/// fechada leva o fluxo a `Ok(None)`.
///
/// Quem carrega a recuperação, então, é o `send(ParFalhou { ParouDeMandar })`
/// do braço `Ok(None)` — o conserto do fim limpo. Removido ele, este teste
/// falha, e falha esperando o primeiro quadro acima do piso do corte:
/// «a paciência acabou esperando: um quadro chegar pelo servidor depois de o
/// par morrer, acima do piso do corte», **três vezes em três**.
///
/// É por isso que o comentário do braço `Ok(None)` em `seele-core/src/enlace.rs`
/// lista «quem empresta saindo da sala» entre os fins limpos: a morte da
/// máquina cai na mesma porta.
///
/// # A forma anterior afirmava e não prendia
///
/// O revisor removeu o `send(ParFalhou { CaiuNoMeio })` do braço `Err` de
/// `escoar_tela_alheia` e o teste **passou três vezes em três**, em 0,24 s
/// cada. Dois defeitos se somavam ali, e um escondia o outro: o braço removido
/// não era o que corre, **e** o teste não media recuperação nenhuma. O rastro
/// dizia por quê: o
/// «primeiro quadro pelo par» era o **seq 0**, o mais velho da fila FIFO,
/// entregue pelo servidor antes de `assistir()` ser chamado, quando as duas
/// cópias ainda subiam. O alvo virava `seq > 1`, e o quadro «que provava» já
/// estava enfileirado desde antes do corte. Era um teste verde que não media
/// nada — a forma de falha que este repositório paga mais caro.
///
/// O conserto é o que os irmãos deste arquivo já fazem, nas três pernas:
///
/// 1. **Antes do corte**, provar que quem assiste está mesmo atrás do par —
///    [`QUADROS_PARA_PROVAR`] quadros acima do piso com o servidor subindo uma
///    cópia só. Sem isso, o teste mediria a recuperação de um caminho que nunca
///    existiu.
/// 2. **No corte**, drenar a fila com [`maior_seq_ja_enfileirado`] e guardar o
///    maior `seq` já enfileirado. Só um `seq` estritamente maior que esse pode
///    ter atravessado depois de o par morrer.
/// 3. **Depois do corte**, sustentação: [`QUADROS_PARA_PROVAR`] quadros
///    seguidos, cada um acima do anterior. Um quadro isolado ainda caberia num
///    punhado em voo; um segundo inteiro de imagem só cabe vindo de um cano que
///    o servidor reabriu.
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

    let piso_inicial = maior_seq_ja_enfileirado(&mut assiste, screen).await;
    assiste.assistir(screen, true).await?;
    ate("o servidor parar de subir a cópia de quem assiste", || {
        copias.agora() == 1
    })
    .await?;

    // Perna 1: o par está servindo **de verdade**. Sem esta prova, o corte
    // aconteceria sobre um caminho que talvez nunca tivesse saído do servidor,
    // e a recuperação seguinte não diria nada sobre a malha.
    let mut piso = piso_inicial;
    for indice in 0..QUADROS_PARA_PROVAR {
        let (seq, _) = esperar(
            &mut assiste,
            "um quadro chegar pelo par, acima do piso, com o cano do servidor desligado",
            |aviso| match aviso {
                Aviso::TelaQuadro { tela, bytes, .. } if *tela == screen => seq_de(bytes)
                    .filter(|seq| piso.is_none_or(|p| *seq > p))
                    .map(|seq| (seq, ())),
                _ => None,
            },
        )
        .await?;
        assert_eq!(
            copias.agora(),
            1,
            "o servidor voltou a subir a cópia de quem assiste antes do corte (quadro {indice} \
             de {QUADROS_PARA_PROVAR}, seq {seq}): o par não estava servindo, e matá-lo não \
             provaria recuperação nenhuma"
        );
        piso = Some(seq);
    }
    println!("o par serviu até o quadro {piso:?}; agora quem empresta morre");

    // **À força, e não com `sair()`.** O que este teste cobre é a máquina de
    // alguém sumindo: `Enlace::drop` aborta a tarefa que fala com o servidor e
    // larga a conexão QUIC no meio de uma transmissão, e do lado de quem
    // assiste isso chega como um fluxo cortado — um erro de leitura.
    //
    // **A despedida limpa não é o caso fácil**, e este comentário já disse
    // que era. Um fluxo de par que termina direito é indistinguível, para
    // quem assiste, de uma transmissão que acabou — e quem assiste já saiu do
    // cano do servidor desde que o par foi apontado, então calar ali era tela
    // em branco permanente. O caso limpo tem prova própria, em
    // `o_fim_limpo_do_repasse_devolve_quem_assiste_ao_servidor`, e o conserto
    // dele está no `Ok(None)` de `escoar_tela_alheia`.
    drop(empresta);

    // Perna 2: **drenar antes de afirmar.** O canal de avisos é FIFO e o par
    // entregou até o instante em que morreu; o piso é o maior `seq` que já
    // estava enfileirado depois do corte, e nenhum quadro daqui para trás pode
    // provar coisa alguma sobre a recuperação. Drenar **depois** do corte, e
    // não antes, é o que fecha a janela entre a drenagem e a morte do par.
    let piso_do_corte = maior_seq_ja_enfileirado(&mut assiste, screen).await;
    println!("a fila esvaziou no quadro {piso_do_corte:?}; daqui para cima é o servidor");

    // Perna 3: sustentação. Um quadro isolado acima do piso ainda caberia num
    // punhado em voo — [`QUADROS_PARA_PROVAR`] seguidos, cada um acima do
    // anterior, são um segundo inteiro de imagem, e isso só chega por um cano
    // que o servidor reabriu por causa do relato.
    let mut piso = piso_do_corte;
    for indice in 0..QUADROS_PARA_PROVAR {
        let (seq, bytes) = esperar(
            &mut assiste,
            "um quadro chegar pelo servidor depois de o par morrer, acima do piso do corte",
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
            "o servidor assumiu e o que ele entregou não é o que saiu de quem compartilha \
             (quadro {indice} de {QUADROS_PARA_PROVAR}, seq {seq})"
        );
        piso = Some(seq);
    }
    println!(
        "{QUADROS_PARA_PROVAR} quadros seguidos chegaram pelo servidor depois de o par ter \
         morrido, até o {piso:?}"
    );

    // E a nomeação não sobreviveu ao par: um `ParFalhou` seguinte se resolveria
    // contra um par que já não serve ninguém.
    let apontado = servidor
        .server()
        .pares
        .lock()
        .await
        .quem_foi_apontado(screen, assiste.sessao().person);
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

    let mut empresta = cliente(endereco, 2, "empresta").await?;
    empresta.entrar_na_voice_room(sala, None).await?;
    empresta.consentir_no_caminho_entre_pares(EMPRESTA).await?;

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
            consentimento: ASSISTE_POR_PAR,
            impressao: "a".repeat(64),
            locais: Vec::new(),
        },
    )
    .await?;

    let screen = compartilhar(&mut compartilha).await?;
    let empresta_quem = empresta.sessao().person;
    let assiste_quem = assiste.pessoa;

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

    // **E tem de estar recebendo a transmissão**, que é a segunda
    // pré-condição e a que faltava aqui. Um par repassa o que ele mesmo
    // recebe, então `Pares::escolher` não o considera antes de o cano dele
    // estar ligado. A escolha acontece **uma vez**, no `WatchScreen`: pedir
    // cedo demais mede o caminho de sempre achando que mede a malha, e o
    // sintoma é este teste esgotando a paciência em «o servidor não apontou
    // quem empresta». `cenario_por` já espera por isto; este teste monta o
    // cenário à mão e não esperava.
    esperar(
        &mut empresta,
        "quem empresta ver a tela pelo servidor",
        |aviso| matches!(aviso, Aviso::TelaAbriu { tela, .. } if *tela == screen).then_some(()),
    )
    .await?;

    frame::write(&mut assiste.envio, &ClientMessage::WatchScreen { screen }).await?;
    let fim = Instant::now() + PACIENCIA;
    loop {
        let apontado = servidor
            .server()
            .pares
            .lock()
            .await
            .quem_foi_apontado(screen, assiste_quem);
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

/// **Um repasse que termina limpo devolve quem assiste ao servidor.**
///
/// O irmão de `quando_o_par_morre_o_servidor_assume_e_ninguem_perde_imagem`, e
/// o caso que ele **não** cobre. Lá o fluxo do par é cortado no meio e a
/// leitura devolve erro; aqui ele termina direito — `finish()` do outro lado,
/// `Ok(None)` desta — com a transmissão ainda no ar.
///
/// # Por que o caso limpo não é o caso fácil
///
/// Porque quem assiste **não tem como distinguir** «a tela acabou» de «o par
/// calou»: as duas chegam como um fluxo que termina sem erro. E o cano do
/// servidor para esta pessoa foi desligado quando o par foi apontado
/// (`TelaParouDeAssistir`), então calar aqui é tela em branco permanente, sem
/// ninguém saber.
///
/// # Como o fim limpo é produzido
///
/// Quem empresta para de assistir. O cano do servidor para ele fecha, a tarefa
/// que lê a tela alheia dele chega ao fim do fluxo, `RepasseDeTela::fechou`
/// desliga o destino, e `par::repassar` termina o fluxo do par direito — com a
/// transmissão de quem compartilha continuando no ar para todo mundo. É um dos
/// três caminhos do fim limpo (os outros são a contrapressão de
/// `PEDACOS_A_ESPERA_DO_PAR` e quem empresta sair da sala), e é o único que um
/// teste produz sem tocar em relógio nem em memória.
#[tokio::test(flavor = "multi_thread")]
async fn o_fim_limpo_do_repasse_devolve_quem_assiste_ao_servidor() -> Result<()> {
    let Cenario {
        servidor,
        compartilha,
        empresta,
        mut assiste,
        screen,
        copias,
    } = cenario().await?;

    // **O repasse tem de estar mesmo no ar antes de acabar.** Uma versão
    // anterior deste teste pedia «o primeiro quadro» e mandava quem empresta
    // parar de assistir logo depois: o quadro era o `seq` 0, que já estava na
    // fila desde antes da malha, e quem empresta parava **antes** de a ligação
    // com o par sequer fechar. O que o teste media então era o caminho de
    // `AssistaTelaPor` sem par nenhum do outro lado — não o fim limpo de um
    // repasse. A prova de que o par está servindo é a mesma do teste central:
    // um piso drenado, e um segundo de imagem acima dele com o servidor
    // subindo uma cópia só.
    let piso_inicial = maior_seq_ja_enfileirado(&mut assiste, screen).await;
    assiste.assistir(screen, true).await?;
    ate("o servidor parar de subir a cópia de quem assiste", || {
        copias.agora() == 1
    })
    .await?;
    let mut piso = piso_inicial;
    for indice in 0..QUADROS_PARA_PROVAR {
        let (seq, _) = esperar(
            &mut assiste,
            "um quadro chegar pelo par, acima do piso, com o cano do servidor desligado",
            |aviso| match aviso {
                Aviso::TelaQuadro { tela, bytes, .. } if *tela == screen => seq_de(bytes)
                    .filter(|seq| piso.is_none_or(|p| *seq > p))
                    .map(|seq| (seq, ())),
                _ => None,
            },
        )
        .await?;
        assert_eq!(
            copias.agora(),
            1,
            "o servidor voltou a subir a cópia de quem assiste antes de o repasse começar de \
             verdade (quadro {indice} de {QUADROS_PARA_PROVAR}, seq {seq})"
        );
        piso = Some(seq);
    }
    println!("o repasse pelo par está no ar até o quadro {piso:?}; agora ele termina limpo");

    // **Sem `drop`, e sem erro nenhum.** Quem empresta continua conectado, na
    // sala e vivo; só deixa de assistir. O repasse acaba pelo caminho educado.
    empresta.assistir(screen, false).await?;

    // O fim do fluxo do par, visto de dentro de quem assiste. Daqui para
    // frente, tudo o que chegar tem de ter vindo de outro lugar.
    esperar(&mut assiste, "o fluxo do par terminar", |aviso| {
        matches!(aviso, Aviso::TelaFechou { tela } if *tela == screen).then_some(())
    })
    .await?;

    // **Um piso novo, e é ele que faz esta prova valer.** O canal de avisos é
    // FIFO e o par entregou quadros até calar; pedir um `seq` acima do último
    // que se leu passaria com o defeito no lugar, servido pela fila. Drenar
    // até a fila esvaziar é o que separa «o servidor voltou a servir» de
    // «ainda havia imagem velha guardada».
    let depois_do_par = maior_seq_ja_enfileirado(&mut assiste, screen).await;
    println!("a fila de quem assiste esvaziou no quadro {depois_do_par:?}");

    // E sustentação, pela mesma razão do teste central: um quadro isolado
    // acima do piso ainda cabe num punhado em voo; um segundo inteiro de
    // imagem, cada quadro acima do anterior, não cabe.
    let mut anterior = depois_do_par;
    for indice in 0..QUADROS_PARA_PROVAR {
        let (seq, bytes) = esperar(
            &mut assiste,
            "um quadro chegar pelo servidor depois de o repasse ter terminado limpo",
            |aviso| match aviso {
                Aviso::TelaQuadro { tela, bytes, .. } if *tela == screen => seq_de(bytes)
                    .filter(|seq| anterior.is_none_or(|p| *seq > p))
                    .map(|seq| (seq, bytes.clone())),
                _ => None,
            },
        )
        .await?;
        assert_eq!(
            bytes,
            corpo(seq),
            "o quadro {seq} chegou e não é o que saiu de quem compartilha (quadro {indice} de \
             {QUADROS_PARA_PROVAR})"
        );
        anterior = Some(seq);
    }
    println!(
        "{QUADROS_PARA_PROVAR} quadros seguidos chegaram pelo servidor depois do fim limpo do \
         repasse, todos acima de {depois_do_par:?}"
    );

    drop(compartilha);
    drop(empresta);
    drop(assiste);
    servidor.shutdown();
    Ok(())
}

/// **Quem retira o consentimento de emprestar para de subir a cópia na hora, e
/// quem estava atrás dele não perde imagem.**
///
/// # O que um consentimento que só valesse adiante custaria
///
/// A pessoa desliga o interruptor e continua pagando. A declaração nova
/// entraria em `Pares` e mudaria a **escolha seguinte**, enquanto o repasse em
/// curso seguiria subindo pela conexão dela — sem prazo para acabar, porque
/// nada nele repara em consentimento. O único aviso seria a conta de internet
/// no fim do mês, e é a forma mais cara do defeito que esta casa mais comete:
/// o produto sabe e não conta.
///
/// # As três pernas
///
/// 1. **antes**: o par está servindo de verdade — `ate_o_par_estar_servindo`
///    prova o piso, a queda do contador para uma cópia e trinta quadros
///    sustentados. Sem esta perna o resto mediria o silêncio de um caminho que
///    nunca existiu;
/// 2. **no corte**: a vaga do par volta à fila — os dois lados da nomeação
///    desfeitos, e não só um;
/// 3. **depois**: trinta quadros estritamente crescentes acima do piso,
///    chegando pelo servidor. É o que separa «o repasse acabou» de «a tela de
///    quem assiste parou», que é o desfecho que a malha existe para nunca ter.
///
/// Esta prova não afirma que a contagem de espectadores volte a anunciar duas
/// cópias no corte — o produto hoje não reemite esse aviso ao reabrir o cano
/// (pendência registrada em `docs/pendencias.md`); só o repasse em si.
#[tokio::test(flavor = "multi_thread")]
async fn retirar_o_consentimento_de_emprestar_encerra_o_repasse_e_o_servidor_reassume() -> Result<()>
{
    let Cenario {
        servidor,
        compartilha,
        empresta,
        mut assiste,
        screen,
        copias,
    } = cenario().await?;

    let piso = ate_o_par_estar_servindo(
        &mut assiste,
        screen,
        &copias,
        "a retirada do consentimento de emprestar",
    )
    .await?;
    println!("o par serviu até o quadro {piso:?}; agora quem empresta retira o consentimento");

    empresta
        .consentir_no_caminho_entre_pares(ConsentimentoDePar::de_ninguem())
        .await?;

    // O fim do fluxo do par, visto de dentro de quem assiste. Daqui para
    // frente, tudo o que chegar tem de ter vindo de outro lugar — e o único
    // outro lugar é o servidor.
    esperar(
        &mut assiste,
        "o fluxo do par terminar depois de quem empresta retirar o consentimento",
        |aviso| matches!(aviso, Aviso::TelaFechou { tela } if *tela == screen).then_some(()),
    )
    .await?;

    // A vaga do par volta à fila: os dois lados da nomeação desfeitos, e não
    // só o do cliente. Sem isto o servidor continuaria contando quem retirou
    // como ocupado pelo resto da sessão do daemon.
    let fim = Instant::now() + PACIENCIA;
    loop {
        let ocupados = servidor.server().pares.lock().await.ja_servindo();
        if ocupados.is_empty() {
            break;
        }
        assert!(
            Instant::now() < fim,
            "quem retirou o consentimento continua contado como par ocupado ({ocupados:?}): a \
             vaga não voltou à fila"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // **Um piso novo, e é ele que faz esta prova valer.** O canal de avisos é
    // FIFO e o par entregou quadros até calar; pedir um `seq` acima do último
    // que se leu passaria com o defeito no lugar, servido pela fila.
    let depois = maior_seq_ja_enfileirado(&mut assiste, screen).await;
    println!("a fila esvaziou no quadro {depois:?}; daqui para cima é o servidor");
    let mut anterior = depois;
    for indice in 0..QUADROS_PARA_PROVAR {
        let (seq, bytes) = esperar(
            &mut assiste,
            "um quadro chegar pelo servidor depois de quem empresta retirar o consentimento",
            |aviso| match aviso {
                Aviso::TelaQuadro { tela, bytes, .. } if *tela == screen => seq_de(bytes)
                    .filter(|seq| anterior.is_none_or(|p| *seq > p))
                    .map(|seq| (seq, bytes.clone())),
                _ => None,
            },
        )
        .await?;
        assert_eq!(
            bytes,
            corpo(seq),
            "o quadro {seq} chegou e não é o que saiu de quem compartilha (quadro {indice} de \
             {QUADROS_PARA_PROVAR})"
        );
        anterior = Some(seq);
    }
    println!(
        "{QUADROS_PARA_PROVAR} quadros seguidos chegaram pelo servidor depois da retirada, todos \
         acima de {depois:?}"
    );

    drop(compartilha);
    drop(empresta);
    drop(assiste);
    servidor.shutdown();
    Ok(())
}

/// **Quem retira o consentimento de assistir por par volta a ser servido pelo
/// servidor, e não fica no escuro.**
///
/// O outro lado da retirada, e ele é o que **não tem relato a esperar**: quando
/// quem empresta desliga, o fluxo do par termina e o `ParFalhou` do fim limpo
/// reabre o cano. Aqui nada falhou — quem assiste desligou o próprio caminho de
/// propósito. Se o servidor não reabrisse o cano ao receber a declaração nova,
/// a tela desta pessoa simplesmente pararia, sem erro em lugar nenhum.
#[tokio::test(flavor = "multi_thread")]
async fn retirar_o_consentimento_de_assistir_por_par_devolve_a_tela_ao_servidor() -> Result<()> {
    let Cenario {
        servidor,
        compartilha,
        empresta,
        mut assiste,
        screen,
        copias,
    } = cenario().await?;

    let piso = ate_o_par_estar_servindo(
        &mut assiste,
        screen,
        &copias,
        "a retirada do consentimento de assistir por par",
    )
    .await?;
    println!("o par serviu até o quadro {piso:?}; agora quem assiste retira o consentimento");

    assiste
        .consentir_no_caminho_entre_pares(ConsentimentoDePar::de_ninguem())
        .await?;

    // **Não há fim de fluxo a esperar deste lado, e isso é medido e não
    // suposto.** Quando quem *empresta* retira, a conexão do par morre e quem
    // assiste vê `TelaFechou`. Aqui é esta máquina que cancela a própria
    // tarefa de leitura, e uma tarefa cancelada não emite fim nenhum — a
    // primeira versão deste teste esperou por `TelaFechou` e esgotou a
    // paciência. Fica registrado em `docs/pendencias.md`: a casca não é
    // avisada de que o caminho de par dela acabou, e o que a segura é a
    // imagem não parar.
    //
    // O que se afirma, então, são as duas coisas que **têm** de valer: o
    // servidor desfez a nomeação, e a imagem continua chegando.
    //
    // A vaga de quem emprestava volta à fila: o repasse acabou para os dois.
    let fim = Instant::now() + PACIENCIA;
    loop {
        let ocupados = servidor.server().pares.lock().await.ja_servindo();
        if ocupados.is_empty() {
            break;
        }
        assert!(
            Instant::now() < fim,
            "quem emprestava continua contado como ocupado ({ocupados:?}) depois de quem assiste \
             ter retirado o consentimento"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let depois = maior_seq_ja_enfileirado(&mut assiste, screen).await;
    println!("a fila esvaziou no quadro {depois:?}; daqui para cima é o servidor");
    let mut anterior = depois;
    for indice in 0..QUADROS_PARA_PROVAR {
        let (seq, bytes) = esperar(
            &mut assiste,
            "um quadro chegar pelo servidor depois de quem assiste retirar o consentimento",
            |aviso| match aviso {
                Aviso::TelaQuadro { tela, bytes, .. } if *tela == screen => seq_de(bytes)
                    .filter(|seq| anterior.is_none_or(|p| *seq > p))
                    .map(|seq| (seq, bytes.clone())),
                _ => None,
            },
        )
        .await?;
        assert_eq!(
            bytes,
            corpo(seq),
            "o quadro {seq} chegou e não é o que saiu de quem compartilha (quadro {indice} de \
             {QUADROS_PARA_PROVAR})"
        );
        anterior = Some(seq);
    }
    println!(
        "{QUADROS_PARA_PROVAR} quadros seguidos chegaram pelo servidor depois de quem assiste \
         retirar o consentimento"
    );

    drop(compartilha);
    drop(empresta);
    drop(assiste);
    servidor.shutdown();
    Ok(())
}

/// Se o servidor chegou a **entregar o endereço** de quem assiste a alguém.
///
/// `SirvaTelaPara` é a única mensagem desta casa que publica o endereço de um
/// espectador para outra máquina — é literalmente o que o §5 da spec de 05/09
/// chama de «espectadores passam a conhecer o endereço IP uns dos outros». O
/// aceite pede que o consentimento **preceda** essa exposição, e uma afirmação
/// sobre de onde os bytes vêm não diz nada sobre isso: um servidor que
/// entregasse o endereço e falhasse em montar o caminho serviria a tela do
/// mesmo jeito, com a privacidade já perdida.
///
/// Por isso esta escuta existe separada do [`ContadorDeCopias`]: uma mede o
/// alívio, e esta mede a exposição.
struct EnderecosEntregues {
    aconteceu: Arc<std::sync::atomic::AtomicBool>,
}

impl EnderecosEntregues {
    /// Passa a escutar as entregas de endereço desta transmissão.
    fn de(servidor: &Daemon, screen: ScreenId) -> Self {
        let mut eventos = servidor.server().events.subscribe();
        let aconteceu = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let escrevendo = Arc::clone(&aconteceu);
        tokio::spawn(async move {
            loop {
                match eventos.recv().await {
                    Ok(Event::SirvaTelaPara { screen: qual, .. }) if qual == screen => {
                        escrevendo.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    Ok(_) => {}
                    // Pela mesma razão do contador de cópias: parar de ler
                    // custa a prova. Aqui custa mais ainda, porque o valor
                    // seguro para esta afirmação é o `false`, e um laço morto
                    // devolve `false` para sempre.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        Self { aconteceu }
    }

    /// Se alguma entrega de endereço já saiu pelo barramento.
    fn houve(&self) -> bool {
        self.aconteceu.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// **Quem recusa assistir por par continua vendo a tela pelo servidor, e o
/// endereço dele não é entregue a ninguém.**
///
/// # A metade do aceite que não tinha guarda
///
/// O aceite tem duas frases: «consentimento explícito precede exposição do
/// endereço do espectador» e «recusa mantém caminho pelo servidor». A segunda
/// não era exercitada por teste nenhum — todo cenário deste arquivo passava por
/// [`cenario_por`], que sempre consente —, e a revisão mediu o que isso custava:
/// trocando a porta de `apontar_um_par` pela versão de antes desta onda
/// (`declaracao_de` no lugar de `quem_consentiu_assistir_por_par`), a suíte
/// inteira seguia verde. A regressão voltaria calada, que é o defeito que o
/// `CLAUDE.md` desta casa chama de «existir não é funcionar».
///
/// # Por que quem recusa aqui **declara** identidade
///
/// Uma pessoa que nunca declarou nada não prova nada: o servidor a recusaria
/// pela falta de declaração, muito antes de chegar ao consentimento. Quem
/// assiste aqui declara [`EMPRESTA`] — está na malha, empresta a própria
/// subida, tem endereço e impressão publicados — e só **não** quer o próprio
/// endereço entregue. É esse o caso que distingue as duas versões da porta, e
/// é o único que prende a regra.
///
/// # As três afirmações
///
/// O endereço não saiu (`SirvaTelaPara` nenhum pela transmissão), o servidor
/// continuou subindo as **duas** cópias do primeiro ao último quadro, e a
/// imagem chegou inteira — recusar não é ficar sem tela. Nenhum par foi
/// nomeado, e é o que se confere no fim.
#[tokio::test(flavor = "multi_thread")]
async fn quem_recusa_assistir_por_par_continua_sendo_servido_pelo_servidor() -> Result<()> {
    let (endereco, daemon) = servidor_com(Location::Memory).await?;
    let Cenario {
        servidor,
        compartilha,
        empresta,
        mut assiste,
        screen,
        copias,
    } = cenario_com(endereco, endereco, daemon, 0, EMPRESTA).await?;

    let enderecos = EnderecosEntregues::de(&servidor, screen);
    let piso = maior_seq_ja_enfileirado(&mut assiste, screen).await;
    println!("a fila esvaziou no quadro {piso:?}; agora quem recusou pede para assistir");
    assiste.assistir(screen, true).await?;

    let mut anterior = piso;
    for indice in 0..QUADROS_PARA_PROVAR {
        let chegou = esperar(
            &mut assiste,
            "um quadro chegar pelo servidor depois de quem recusou pedir para assistir",
            |aviso| match aviso {
                Aviso::TelaQuadro { tela, bytes, .. } if *tela == screen => seq_de(bytes)
                    .filter(|seq| anterior.is_none_or(|p| *seq > p))
                    .map(|seq| (seq, bytes.clone())),
                _ => None,
            },
        )
        .await;
        // **A falha desta espera é um achado, e não um relógio curto.** Quando
        // a porta do consentimento cai, o servidor desliga o cano desta pessoa
        // para dar lugar ao par — e a máquina dela recusa a discagem, porque o
        // cliente também sabe que ela não consentiu. O resultado é a tela
        // parando, e uma paciência esgotada não contaria ao leitor por quê.
        let (seq, bytes) = match chegou {
            Ok(quadro) => quadro,
            Err(erro) => panic!(
                "a imagem de quem recusou assistir por par parou no quadro {indice} de \
                 {QUADROS_PARA_PROVAR}: o endereço dele foi entregue? {}; cópias que o servidor \
                 ainda sobe: {} (esperava 2). A recusa tinha de manter o caminho pelo servidor. \
                 {erro}",
                enderecos.houve(),
                copias.agora()
            ),
        };
        assert!(
            !enderecos.houve(),
            "o servidor entregou o endereço de quem recusou assistir por par (quadro {indice} de \
             {QUADROS_PARA_PROVAR}, seq {seq}): o consentimento tem de preceder a exposição"
        );
        assert_eq!(
            copias.agora(),
            2,
            "o servidor parou de subir a cópia de quem recusou assistir por par (quadro {indice} \
             de {QUADROS_PARA_PROVAR}, seq {seq}): a recusa tinha de manter o caminho pelo \
             servidor"
        );
        assert_eq!(
            bytes,
            corpo(seq),
            "o quadro {seq} chegou e não é o que saiu de quem compartilha (quadro {indice} de \
             {QUADROS_PARA_PROVAR})"
        );
        anterior = Some(seq);
    }
    println!(
        "{QUADROS_PARA_PROVAR} quadros seguidos chegaram pelo servidor, com as duas cópias de pé"
    );

    let nomeados = servidor.server().pares.lock().await.ja_servindo();
    assert!(
        nomeados.is_empty(),
        "o servidor nomeou um par ({nomeados:?}) para servir quem recusou assistir por par"
    );

    drop(compartilha);
    drop(empresta);
    drop(assiste);
    servidor.shutdown();
    Ok(())
}

/// **Um repasse encerrado normalmente devolve o par à fila.**
///
/// A nomeação do servidor (`Pares::apontou`) só era desfeita por um
/// `ParFalhou`. Quando o repasse terminava **bem** — quem assiste fecha a
/// janela e manda `UnwatchScreen` —, ela ficava de pé, e `Pares::ja_servindo`
/// contava aquele par como ocupado pelo resto da sessão do daemon. Do lado do
/// cliente `atendendo_pares` já tinha sido devolvido: os dois lados
/// discordavam em silêncio, e a malha degradava para a estrela — um par por
/// transmissão encerrada — sem um único rastro dizendo por quê.
///
/// A afirmação é sobre o **estado do servidor**, e não sobre imagem: é lá que
/// a vaga era queimada, e é lá que o teste tem de olhar.
#[tokio::test(flavor = "multi_thread")]
async fn um_repasse_encerrado_normalmente_devolve_o_par_a_fila() -> Result<()> {
    let Cenario {
        servidor,
        compartilha,
        empresta,
        assiste,
        screen,
        copias,
    } = cenario().await?;

    let empresta_quem = empresta.sessao().person;

    assiste.assistir(screen, true).await?;
    ate("o servidor parar de subir a cópia de quem assiste", || {
        copias.agora() == 1
    })
    .await?;
    {
        let pares = servidor.server().pares.lock().await;
        assert!(
            pares.ja_servindo().contains(&empresta_quem),
            "o servidor apontou um par e não o contou como ocupado"
        );
    }

    // **Quem assiste sai da sala**, e é este o caminho que nenhuma mensagem do
    // cliente cobre: o cliente não relata nada, e se o servidor não soltar a
    // nomeação sozinho ela fica de pé para sempre.
    //
    // O `UnwatchScreen` é o caminho vizinho, e tem prova própria em
    // `um_unwatch_devolve_a_vaga_do_par_e_ele_volta_a_ser_escolhido`. (Este
    // comentário já disse que reverter a linha do `UnwatchScreen` não fazia
    // teste nenhum falhar, e era verdade enquanto o `ParFalhou` de rotina
    // saía depois e soltava a nomeação pelo braço de sempre. Agora que o
    // cliente derruba o caminho do par no próprio `UnwatchScreen`, relato
    // nenhum sai e aquele `desapontou` é o único mecanismo que resta.)
    assiste.sair_da_voice_room().await?;

    let fim = Instant::now() + PACIENCIA;
    loop {
        let ocupados = servidor.server().pares.lock().await.ja_servindo();
        if ocupados.is_empty() {
            break;
        }
        assert!(
            Instant::now() < fim,
            "o repasse terminou bem e o par continua contado como ocupado ({ocupados:?}) — \
             o servidor nunca mais vai escolhê-lo, e o cliente dele já devolveu a vaga"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    println!("o par voltou à fila depois de o repasse ter sido encerrado normalmente");

    drop(compartilha);
    drop(empresta);
    drop(assiste);
    servidor.shutdown();
    Ok(())
}

/// **Um `UnwatchScreen` devolve a vaga do par, e ele volta a ser escolhido.**
///
/// # O guarda que não tinha teste
///
/// `server.pares.desapontou(screen)`, no braço de `UnwatchScreen` de
/// `seele-server/src/session.rs`, é o **único** mecanismo que solta a vaga do
/// par ali. Ele entrou na onda anterior com a explicação de que o conserto do
/// fim limpo já cobria aquele caminho — e cobria pelo lado errado: quem soltava
/// a nomeação era o `ParFalhou` de rotina que saía **depois**, quando o fluxo do
/// par enfim terminava. Agora que `assistir(tela, false)` derruba o caminho do
/// par no cliente (ver
/// `um_unwatch_derruba_o_caminho_do_par_e_a_imagem_para`), relato nenhum sai
/// depois de um `UnwatchScreen`, e esta linha é tudo o que existe. Enquanto o
/// socorro estava lá, removê-la deixava a suíte inteira verde — medido.
///
/// # Por que a afirmação é dupla
///
/// «A nomeação sumiu» é estado interno, e sozinho não diz que a malha voltou a
/// funcionar. A segunda perna é a que vale: quem assiste pede a tela de novo, e
/// o **mesmo** par volta a servi-la — [`QUADROS_PARA_PROVAR`] quadros acima do
/// piso, com o servidor subindo uma cópia só do primeiro ao último. Sem a vaga
/// devolvida, `Pares::escolher` pula aquele candidato por `ja_servindo` e o
/// servidor reassume: a cópia volta a ser duas, e é assim que a reversão deste
/// guarda aparece.
#[tokio::test(flavor = "multi_thread")]
async fn um_unwatch_devolve_a_vaga_do_par_e_ele_volta_a_ser_escolhido() -> Result<()> {
    let Cenario {
        servidor,
        compartilha,
        empresta,
        mut assiste,
        screen,
        copias,
    } = cenario().await?;

    let empresta_quem = empresta.sessao().person;

    assiste.assistir(screen, true).await?;
    ate("o servidor parar de subir a cópia de quem assiste", || {
        copias.agora() == 1
    })
    .await?;
    {
        let pares = servidor.server().pares.lock().await;
        assert_eq!(
            pares.quem_foi_apontado(screen, assiste.sessao().person),
            Some(empresta_quem),
            "o servidor não apontou quem empresta para servir esta transmissão"
        );
        assert!(
            pares.ja_servindo().contains(&empresta_quem),
            "o servidor apontou um par e não o contou como ocupado"
        );
    }

    // A janela fecha. Nada mais será relatado por esta tela: o caminho do par
    // cai no cliente, e o `ParFalhou` que antes socorria este braço não sai.
    assiste.assistir(screen, false).await?;

    let fim = Instant::now() + PACIENCIA;
    loop {
        let ocupados = servidor.server().pares.lock().await.ja_servindo();
        if ocupados.is_empty() {
            break;
        }
        assert!(
            Instant::now() < fim,
            "o `UnwatchScreen` encerrou o repasse e o par continua contado como ocupado \
             ({ocupados:?}) — o servidor nunca mais vai escolhê-lo, e o cliente dele já \
             devolveu a vaga"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    println!("a vaga do par voltou à fila depois do `UnwatchScreen`");

    // E a prova que importa: com a vaga devolvida, o mesmo par volta a ser
    // escolhido e a servir de verdade.
    let piso_inicial = maior_seq_ja_enfileirado(&mut assiste, screen).await;
    assiste.assistir(screen, true).await?;
    {
        let fim = Instant::now() + PACIENCIA;
        loop {
            let apontado = servidor
                .server()
                .pares
                .lock()
                .await
                .quem_foi_apontado(screen, assiste.sessao().person);
            if apontado == Some(empresta_quem) {
                break;
            }
            assert!(
                Instant::now() < fim,
                "a vaga voltou à fila e o servidor não escolheu aquele par de novo (apontou \
                 {apontado:?}): a vaga foi devolvida no papel e não na prática"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    let mut piso = piso_inicial;
    for indice in 0..QUADROS_PARA_PROVAR {
        let (seq, _) = esperar(
            &mut assiste,
            "um quadro chegar pelo par de novo, acima do piso, com o cano do servidor desligado",
            |aviso| match aviso {
                Aviso::TelaQuadro { tela, bytes, .. } if *tela == screen => seq_de(bytes)
                    .filter(|seq| piso.is_none_or(|p| *seq > p))
                    .map(|seq| (seq, ())),
                _ => None,
            },
        )
        .await?;
        assert_eq!(
            copias.agora(),
            1,
            "o servidor voltou a subir a cópia de quem assiste (quadro {indice} de \
             {QUADROS_PARA_PROVAR}, seq {seq}): o par não reassumiu a transmissão, então a \
             vaga dele não tinha voltado à fila"
        );
        piso = Some(seq);
    }
    println!(
        "o mesmo par voltou a servir {QUADROS_PARA_PROVAR} quadros seguidos, até o {piso:?}, \
         com o servidor subindo uma cópia só"
    );

    drop(compartilha);
    drop(empresta);
    drop(assiste);
    servidor.shutdown();
    Ok(())
}

/// **O contador não pode calar quando o barramento atrasa.**
///
/// `ContadorDeCopias` é a metade negativa de toda prova deste arquivo: um
/// `copias.agora() == 1` afirmado trinta vezes seguidas. Se o laço que o
/// alimenta sair do ar num `Lagged` — e o barramento do servidor larga eventos
/// por desenho quando alguém não os lê a tempo —, `agora()` congela no último
/// número que viu. Congelado em `1`, ele afirma trinta vezes uma coisa que
/// parou de conferir, e o servidor pode ter voltado a subir a cópia sem que
/// asserção nenhuma acuse.
///
/// É o mesmo falso-verde que a Task 10 caçou por outra porta. Aqui ele é
/// provado direto: um recebedor que **já perdeu** eventos, e um número que
/// chega depois da perda.
#[tokio::test(flavor = "multi_thread")]
async fn o_contador_de_copias_sobrevive_a_um_atraso_do_barramento() -> Result<()> {
    let screen = ScreenId(7);
    let voice_room = VoiceRoomId(1);

    // Um barramento minúsculo, cheio **antes** de alguém ler: a primeira coisa
    // que o laço do contador encontra é o `Lagged`.
    let (fala, ouve) = tokio::sync::broadcast::channel::<Event>(2);
    for quantos in 0..4 {
        fala.send(Event::ScreenViewers {
            voice_room,
            screen,
            quantos,
        })
        .expect("o recebedor está vivo");
    }

    let copias = ContadorDeCopias::seguindo(ouve, screen);

    // O número que importa vem **depois** do atraso. Um contador que desistiu
    // no `Lagged` nunca o vê.
    fala.send(Event::ScreenViewers {
        voice_room,
        screen,
        quantos: 1,
    })
    .expect("o recebedor está vivo");

    ate(
        "o contador enxergar o número que veio depois do atraso do barramento",
        || copias.agora() == 1,
    )
    .await?;
    Ok(())
}

/// Quanto tempo de silêncio prova que o caminho parou de entregar imagem.
///
/// A trinta quadros por segundo (`INTERVALO`), um segundo é trinta quadros que
/// **não** chegaram. Um caminho vivo não fica um segundo calado com a
/// transmissão no ar.
const SILENCIO_PARA_PROVAR: Duration = Duration::from_secs(1);

/// **Quem para de assistir para de receber, e o caminho do par cai junto.**
///
/// # O que estava aberto
///
/// `Comando::Assistir { quero: false }` fazia uma coisa só: mandar
/// `UnwatchScreen` ao servidor. Nada derrubava o caminho do par — a tarefa de
/// `escoar_tela_alheia` e a conexão com o par seguiam vivas —, então quem
/// tinha acabado de fechar a janela **continuava recebendo a tela por baixo**,
/// pelo par, e continuava gastando a subida de quem empresta. O servidor
/// obedecia ao pedido (`TelaParouDeAssistir`) e a máquina de quem pediu não.
///
/// # A prova é o silêncio, e ela precisa das duas metades
///
/// Só «parou de chegar quadro» não basta: a transmissão podia ter acabado, e
/// aí o silêncio não diz nada sobre o `UnwatchScreen`. Por isso quem empresta
/// — que continua assistindo pelo servidor — é lido no mesmo intervalo: ele
/// tem de continuar recebendo enquanto quem parou não recebe nada.
#[tokio::test(flavor = "multi_thread")]
async fn um_unwatch_derruba_o_caminho_do_par_e_a_imagem_para() -> Result<()> {
    let Cenario {
        servidor,
        compartilha,
        mut empresta,
        mut assiste,
        screen,
        copias,
    } = cenario().await?;

    // O par tem de estar servindo **de verdade** antes de o `UnwatchScreen`
    // significar alguma coisa: um piso drenado e um segundo de imagem acima
    // dele, com o servidor subindo uma cópia só. É a mesma prova dos irmãos
    // deste arquivo, e sem ela este teste mediria o silêncio de um caminho que
    // nunca existiu.
    let piso_inicial = maior_seq_ja_enfileirado(&mut assiste, screen).await;
    assiste.assistir(screen, true).await?;
    ate("o servidor parar de subir a cópia de quem assiste", || {
        copias.agora() == 1
    })
    .await?;
    let mut piso = piso_inicial;
    for indice in 0..QUADROS_PARA_PROVAR {
        let (seq, _) = esperar(
            &mut assiste,
            "um quadro chegar pelo par, acima do piso, com o cano do servidor desligado",
            |aviso| match aviso {
                Aviso::TelaQuadro { tela, bytes, .. } if *tela == screen => seq_de(bytes)
                    .filter(|seq| piso.is_none_or(|p| *seq > p))
                    .map(|seq| (seq, ())),
                _ => None,
            },
        )
        .await?;
        assert_eq!(
            copias.agora(),
            1,
            "o servidor voltou a subir a cópia de quem assiste antes do `UnwatchScreen` \
             (quadro {indice} de {QUADROS_PARA_PROVAR}, seq {seq})"
        );
        piso = Some(seq);
    }
    println!("o par está servindo até o quadro {piso:?}; agora quem assiste fecha a janela");

    assiste.assistir(screen, false).await?;

    // **Drenar antes de afirmar.** O canal de avisos é FIFO e o par entregou
    // quadros até o instante do pedido; exigir silêncio sem esvaziar a fila
    // seria falhar por causa de imagem que já tinha chegado.
    let ultimo = maior_seq_ja_enfileirado(&mut assiste, screen).await;
    println!("a fila de quem parou de assistir esvaziou no quadro {ultimo:?}");

    // E agora o silêncio, com a transmissão comprovadamente no ar: quem
    // empresta continua recebendo pelo servidor no mesmo intervalo.
    let fim = Instant::now() + SILENCIO_PARA_PROVAR;
    while Instant::now() < fim {
        if let Ok(Aviso::TelaQuadro { tela, bytes, .. }) =
            &tokio::time::timeout(Duration::from_millis(50), assiste.proximo()).await
        {
            assert!(
                *tela != screen,
                "quem pediu para parar de assistir recebeu o quadro {:?} depois do \
                 `UnwatchScreen`: o caminho do par continuou vivo por baixo, gastando a \
                 subida de quem empresta e entregando imagem que ninguém pediu",
                seq_de(bytes)
            );
        }
    }
    let ainda_chega = maior_seq_ja_enfileirado(&mut empresta, screen).await;
    assert!(
        ainda_chega.is_some(),
        "quem empresta parou de receber junto: o silêncio de quem parou de assistir não prova \
         nada, porque a transmissão inteira pode ter morrido"
    );
    println!(
        "um segundo sem um único quadro para quem parou de assistir, com quem empresta \
         recebendo até o quadro {ainda_chega:?}"
    );

    drop(compartilha);
    drop(empresta);
    drop(assiste);
    servidor.shutdown();
    Ok(())
}

/// **Um `ParFalhou` que chega depois do `UnwatchScreen` não reabre o cano.**
///
/// # A regressão que a onda de consertos criou
///
/// Depois do conserto do fim limpo, **todo** fim de fluxo de par vira
/// `ParFalhou` — inclusive o de rotina: contrapressão, quem empresta
/// reconectando, quem empresta saindo da sala. E o braço de `ParFalhou` no
/// servidor mandava `TelaAssistir` para quem relatou sem conferir se essa
/// pessoa ainda queria a tela. Então: alguém fecha a janela; algum tempo
/// depois o fluxo do par termina; o relato sai; e **o servidor volta a subir a
/// cópia para quem tinha pedido para parar** — a subida que a malha existe
/// para aliviar, gasta com imagem que ninguém está olhando.
///
/// # Por que quem assiste é um par cru aqui
///
/// Porque a metade do cliente tem prova própria
/// (`um_unwatch_derruba_o_caminho_do_par_e_a_imagem_para`), e com ela no lugar
/// nenhum cliente de verdade manda `ParFalhou` depois de um `UnwatchScreen`.
/// Um teste que dependesse do cliente para produzir o relato estaria provando
/// o conserto do cliente uma segunda vez, e deixaria o braço do servidor sem
/// guarda nenhum — que é exatamente a forma de falha que este repositório paga
/// mais caro. Cru, o relato sai à mão, e o que está sob teste é só a decisão
/// do servidor.
///
/// # O que é observado, e por que não é o contador de cópias
///
/// `Event::ScreenViewers` só sai de `VoiceRoom::reconferir_o_teto`, e o braço
/// de `TelaAssistir` não o chama: o contador de cópias **não se mexe** quando
/// o servidor readmite alguém, e afirmar sobre ele aqui seria afirmar sobre um
/// número que não responde ao defeito. O que responde é o fio: readmitido, o
/// espectador entra em `esperando` e ganha um cano no próximo quadro-chave — e
/// um cano é um fluxo uni novo aberto para esta conexão. Nenhum fluxo novo,
/// nenhuma cópia.
#[tokio::test(flavor = "multi_thread")]
async fn um_parfalhou_depois_do_unwatch_nao_faz_o_servidor_voltar_a_mandar_a_tela() -> Result<()> {
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

    let mut empresta = cliente(endereco, 2, "empresta").await?;
    empresta.entrar_na_voice_room(sala, None).await?;
    empresta.consentir_no_caminho_entre_pares(EMPRESTA).await?;

    let mut assiste = abrir(endereco, 3).await?;
    frame::write(
        &mut assiste.envio,
        &ClientMessage::EnterVoiceRoom {
            voice_room: sala,
            password: None,
        },
    )
    .await?;
    // A identidade de quem só assiste, à mão: sem ela `apontar_um_par` não tem
    // impressão de quem pediu para apresentar a quem empresta, e nenhum par é
    // apontado.
    frame::write(
        &mut assiste.envio,
        &ClientMessage::EmprestarSubida {
            consentimento: ASSISTE_POR_PAR,
            impressao: "a".repeat(64),
            locais: Vec::new(),
        },
    )
    .await?;

    let screen = compartilhar(&mut compartilha).await?;
    let empresta_quem = empresta.sessao().person;

    // O cano do servidor de antes da malha: quem está na sala quando a única
    // transmissão abre entra ligado nela, e ligado é um fluxo uni.
    let _cano_do_servidor = tokio::time::timeout(PACIENCIA, assiste.conexao.accept_uni())
        .await
        .map_err(|_| {
            anyhow::anyhow!("o servidor nunca abriu o cano da tela para quem assiste")
        })??;

    // **E tem de estar recebendo a transmissão**, pela mesma razão do irmão
    // deste arquivo: um par repassa o que ele mesmo recebe, e a escolha
    // acontece uma vez só, no `WatchScreen`. Aqui o cano de quem assiste já
    // provou que `tela_abriu` correu — e ela liga todo mundo da sala no mesmo
    // instante —, mas a pré-condição fica escrita em vez de deduzida.
    esperar(
        &mut empresta,
        "quem empresta ver a tela pelo servidor",
        |aviso| matches!(aviso, Aviso::TelaAbriu { tela, .. } if *tela == screen).then_some(()),
    )
    .await?;

    // Quem empresta tem de estar declarado antes do pedido, senão
    // `Pares::escolher` não tem candidato e o teste mediria o caminho de
    // sempre pensando estar medindo a malha.
    {
        let fim = Instant::now() + PACIENCIA;
        loop {
            let pronto = servidor
                .server()
                .pares
                .lock()
                .await
                .declaracao_de(empresta_quem)
                .is_some();
            if pronto || Instant::now() >= fim {
                assert!(
                    pronto,
                    "a declaração de quem empresta não chegou ao servidor"
                );
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
            .quem_foi_apontado(screen, assiste.pessoa);
        if apontado == Some(empresta_quem) {
            break;
        }
        assert!(
            Instant::now() < fim,
            "o servidor não apontou quem empresta para servir esta transmissão (apontou \
             {apontado:?})"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // A janela fecha. Daqui para frente esta pessoa não quer mais a tela.
    frame::write(&mut assiste.envio, &ClientMessage::UnwatchScreen { screen }).await?;
    {
        let fim = Instant::now() + PACIENCIA;
        loop {
            let apontado = servidor
                .server()
                .pares
                .lock()
                .await
                .quem_foi_apontado(screen, assiste.pessoa);
            if apontado.is_none() || Instant::now() >= fim {
                assert_eq!(
                    apontado, None,
                    "o `UnwatchScreen` não soltou a nomeação do par"
                );
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    // E só agora o fluxo do par termina — o fim de rotina que o conserto do
    // fim limpo transformou em relato.
    frame::write(
        &mut assiste.envio,
        &ClientMessage::ParFalhou {
            screen,
            motivo: seele_proto::control::MotivoDeFalhaDePar::ParouDeMandar,
        },
    )
    .await?;

    // Nenhum cano novo. Dois segundos são sessenta quadros e vinte
    // quadros-chave (`A_CADA_QUANTOS_UMA_CHAVE`) — um espectador readmitido
    // teria ganho o cano dele em qualquer um deles.
    match tokio::time::timeout(Duration::from_secs(2), assiste.conexao.accept_uni()).await {
        Err(_) => {}
        Ok(_) => panic!(
            "o servidor abriu um cano de tela novo para quem tinha acabado de mandar \
             `UnwatchScreen`: um `ParFalhou` de rotina desfez o pedido de parar, e a subida \
             que a malha existe para aliviar voltou a sair daqui para uma janela fechada"
        ),
    }
    println!("dois segundos depois do relato, e nenhum cano novo para quem parou de assistir");

    drop(compartilha);
    drop(empresta);
    drop(assiste);
    servidor.shutdown();
    Ok(())
}

/// Quanto tempo se drena a fila de uma sessão que já acabou, antes de exigir
/// silêncio dela.
///
/// Prazo fixo, e **não** «até o canal ficar quieto» como
/// [`maior_seq_ja_enfileirado`] faz. Depois de `sair()` os dois desfechos do
/// canal de avisos são opostos e os dois têm de caber neste laço: com o
/// caminho do par derrubado não sobra remetente nenhum, e `Enlace::proximo`
/// passa a devolver `Encerrado` **na hora, para sempre** — um laço que só
/// desistisse por silêncio nunca desistiria. Com o caminho vivo, ao contrário,
/// os quadros continuam vindo. Um prazo cobre os dois.
const DRENAGEM_DEPOIS_DO_FIM: Duration = Duration::from_millis(300);

/// Drena os avisos de uma sessão que já acabou, e devolve o maior `seq` desta
/// tela que ainda estava na fila.
///
/// Irmão de [`maior_seq_ja_enfileirado`] para depois do fim — ver
/// [`DRENAGEM_DEPOIS_DO_FIM`] para por que o critério de parada tem de ser
/// outro. A pausa do braço `Ok` é o que impede este laço de virar espera
/// ocupada quando o canal responde `Encerrado` sem custo nenhum.
async fn drenar_depois_do_fim(enlace: &mut Enlace, screen: ScreenId) -> Option<u32> {
    let mut maior = None;
    let fim = Instant::now() + DRENAGEM_DEPOIS_DO_FIM;
    while Instant::now() < fim {
        match tokio::time::timeout(Duration::from_millis(20), enlace.proximo()).await {
            Ok(Aviso::TelaQuadro { tela, bytes, .. }) if tela == screen => {
                if let Some(seq) = seq_de(&bytes) {
                    maior = Some(maior.map_or(seq, |atual: u32| atual.max(seq)));
                }
            }
            Ok(_) => tokio::time::sleep(Duration::from_millis(5)).await,
            Err(_) => {}
        }
    }
    maior
}

/// Põe o par a servir esta tela de verdade, e devolve o maior `seq` provado.
///
/// As três pernas que os irmãos deste arquivo já usam, numa função só: o piso
/// drenado **antes** do pedido, o contador do servidor caindo para uma cópia, e
/// [`QUADROS_PARA_PROVAR`] quadros estritamente crescentes acima do piso com o
/// contador em uma cópia do primeiro ao último.
///
/// Existe porque os dois testes de fim de sessão precisam **da mesma** prova de
/// que havia um caminho de par vivo antes do corte, e sem ela mediriam o
/// silêncio de um caminho que nunca existiu.
async fn ate_o_par_estar_servindo(
    assiste: &mut Enlace,
    screen: ScreenId,
    copias: &ContadorDeCopias,
    onde: &str,
) -> Result<Option<u32>> {
    let piso_inicial = maior_seq_ja_enfileirado(assiste, screen).await;
    assiste.assistir(screen, true).await?;
    ate("o servidor parar de subir a cópia de quem assiste", || {
        copias.agora() == 1
    })
    .await?;

    let mut piso = piso_inicial;
    for indice in 0..QUADROS_PARA_PROVAR {
        let (seq, _) = esperar(
            assiste,
            "um quadro chegar pelo par, acima do piso, com o cano do servidor desligado",
            |aviso| match aviso {
                Aviso::TelaQuadro { tela, bytes, .. } if *tela == screen => seq_de(bytes)
                    .filter(|seq| piso.is_none_or(|p| *seq > p))
                    .map(|seq| (seq, ())),
                _ => None,
            },
        )
        .await?;
        assert_eq!(
            copias.agora(),
            1,
            "o servidor voltou a subir a cópia de quem assiste antes de {onde} (quadro {indice} \
             de {QUADROS_PARA_PROVAR}, seq {seq}): não havia caminho de par, e o que este teste \
             mede depois não diz nada sobre ele"
        );
        piso = Some(seq);
    }
    Ok(piso)
}

/// Um espectador **cru**: recebe a tela do servidor, declara identidade de par,
/// e nunca disca nem relata nada.
///
/// Existe para uma medida que nenhum cliente de verdade sabe dar. Quem assiste
/// por um `Enlace` conserta sozinho todo caminho de par que acaba: o fluxo do
/// par termina, ele manda `ParFalhou`, e o servidor reabre o cano dele na volta
/// do relato. Numa máquina só, essa volta cabe dentro de um quadro — e então
/// **toda** afirmação sobre imagem passa igual com ou sem o servidor ter feito
/// a própria parte. Esta ponta não relata nada, e o que sobra é só o que o
/// servidor decidiu sozinho.
struct EspectadorCru {
    par: Par,
    aberturas: Arc<std::sync::atomic::AtomicU32>,
}

impl EspectadorCru {
    /// Quantas vezes o servidor abriu um fluxo de tela para esta máquina.
    ///
    /// É a medida de «o cano do servidor está aberto para esta pessoa», e ela é
    /// de contagem e não de estado de propósito: reabrir é um evento, e um
    /// booleano que voltasse a `true` não diria se voltou por causa da saída ou
    /// se nunca chegou a cair.
    fn aberturas(&self) -> u32 {
        self.aberturas.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Sobe um [`EspectadorCru`] já sentado na sala e já declarado como par.
async fn espectador_cru(
    endereco: SocketAddr,
    semente: u8,
    sala: VoiceRoomId,
) -> Result<EspectadorCru> {
    let mut par = abrir(endereco, semente).await?;
    frame::write(
        &mut par.envio,
        &ClientMessage::EnterVoiceRoom {
            voice_room: sala,
            password: None,
        },
    )
    .await?;
    // A declaração de quem **assiste** por par, escrita no fio à mão: é ela que
    // faz o servidor ter endereço e impressão para pôr num `SirvaTelaPara`, e
    // sem ela a porta do consentimento recusaria a nomeação antes de o teste
    // chegar ao que ele mede.
    frame::write(
        &mut par.envio,
        &ClientMessage::EmprestarSubida {
            consentimento: ASSISTE_POR_PAR,
            impressao: "b".repeat(64),
            locais: Vec::new(),
        },
    )
    .await?;

    let conexao = par.conexao.clone();
    let aberturas = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let contando = Arc::clone(&aberturas);
    tokio::spawn(async move {
        while let Ok(mut fluxo) = conexao.accept_uni().await {
            let mut tipo = [0_u8; 1];
            if fluxo.read_exact(&mut tipo).await.is_err() {
                continue;
            }
            if tipo[0] != seele_proto::stream::StreamType::Screen.byte() {
                continue;
            }
            contando.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            // **Drenado, e não só contado.** Um fluxo que ninguém lê enche a
            // fila deste espectador, e a sala corta quem não acompanha — o cano
            // fecharia sozinho e o teste estaria medindo a própria negligência
            // em vez da saída de quem empresta.
            tokio::spawn(async move {
                while let Ok(Some(_)) = fluxo.read_chunk(64 * 1024, false).await {}
            });
        }
    });

    Ok(EspectadorCru { par, aberturas })
}

/// **A saída de quem empresta reabre o cano de quem ficou atrás dele, sem
/// esperar relato nenhum.**
///
/// # A terceira direção
///
/// Um par repassa o que ele mesmo recebe. Saindo da sala ele para de receber, e
/// quem estava atrás dele fica sem imagem — sem ninguém ter falhado e sem
/// ninguém ter pedido nada. O servidor é o único que sabe disso no instante em
/// que acontece: `Pares::quem_empresta_parou` devolve os órfãos justamente para
/// que `devolver_ao_servidor` reabra o cano de cada um ali, em vez de esperar a
/// outra ponta perceber.
///
/// # Por que quem fica órfão aqui é uma ponta crua
///
/// Medido, e é o que dá forma a este teste: com um `Enlace` de verdade no lugar
/// do órfão, **os dois mundos são indistinguíveis**. O fluxo do par termina
/// limpo quando quem empresta sai, o cliente manda `ParFalhou`, e numa máquina
/// só a volta desse relato cabe dentro de um intervalo de quadro — a imagem não
/// pisca nem com a linha nem sem ela. Um teste de imagem ali afirma uma coisa
/// verdadeira e não prende nada.
///
/// [`EspectadorCru`] tira o relato da conta: ele recebe a tela do servidor,
/// declara identidade de par para poder ser nomeado, e nunca disca nem relata.
/// Quem empresta também não relata — `ClientMessage::ParFalhou` é explícita que
/// só quem assiste a manda. Então o que reabre o cano deste espectador só pode
/// ser o servidor, por conta própria, na saída.
#[tokio::test(flavor = "multi_thread")]
async fn a_saida_de_quem_empresta_reabre_o_cano_de_quem_ficou_orfao_sem_esperar_relato(
) -> Result<()> {
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
    empresta.entrar_na_voice_room(sala, None).await?;
    empresta.consentir_no_caminho_entre_pares(EMPRESTA).await?;
    let quem_empresta = empresta.sessao().person;

    let mut orfao = espectador_cru(endereco, 3, sala).await?;

    // Assinado antes de a transmissão existir, pela razão escrita em
    // `cenario_com`: o anúncio de espectadores é único, e quem assina depois
    // dele pode nunca ver número nenhum.
    let ouve = servidor.server().events.subscribe();
    let screen = compartilhar(&mut compartilha).await?;
    let copias = ContadorDeCopias::seguindo(ouve, screen);
    ate(
        "o servidor contar as duas cópias que ele mesmo sobe",
        || copias.agora() == 2,
    )
    .await?;
    ate("o servidor abrir o fluxo de tela para a ponta crua", || {
        orfao.aberturas() == 1
    })
    .await?;

    // O pedido que põe um par no meio. `SirvaTelaPara` sai para quem empresta,
    // que vai discar para esta ponta e não vai conseguir — ela não atende
    // ligação nenhuma. Não faz diferença para o que se mede: quem empresta não
    // relata falha de par, e esta ponta não relata nada.
    frame::write(&mut orfao.par.envio, &ClientMessage::WatchScreen { screen }).await?;
    let prazo = Instant::now() + PACIENCIA;
    loop {
        if servidor
            .server()
            .pares
            .lock()
            .await
            .ja_servindo()
            .contains(&quem_empresta)
        {
            break;
        }
        assert!(
            Instant::now() < prazo,
            "o servidor não nomeou par nenhum para a ponta crua: o que este teste mede depois \
             não diria nada sobre a saída de quem empresta"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    ate("o servidor desligar o cano da ponta crua", || {
        copias.agora() == 1
    })
    .await?;

    let aberturas_antes = orfao.aberturas();
    assert_eq!(
        aberturas_antes, 1,
        "o servidor abriu {aberturas_antes} fluxos de tela para a ponta crua antes da saída, e \
         esperava-se um só"
    );

    println!("o par foi nomeado e o cano do servidor caiu; agora quem empresta sai da sala");
    let saiu_em = Instant::now();
    empresta.sair_da_voice_room().await?;

    let prazo = saiu_em + GRACA_DA_SAIDA;
    while orfao.aberturas() == aberturas_antes {
        assert!(
            Instant::now() < prazo,
            "quem empresta saiu da sala há {:?} e o servidor não reabriu o cano de quem ficava \
             atrás dele: essa pessoa está sem imagem, e ninguém vai relatar nada — nem ela, que \
             não sabe relatar, nem quem emprestava, que por protocolo nunca relata",
            saiu_em.elapsed()
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    println!(
        "o servidor reabriu o cano de quem ficou órfão em {:?}",
        saiu_em.elapsed()
    );

    drop(compartilha);
    drop(empresta);
    drop(orfao);
    servidor.shutdown();
    Ok(())
}

/// **Quem empresta sair da sala não deixa quem estava atrás dele no escuro.**
///
/// A promessa vista de fora, com clientes de verdade dos dois lados: a imagem
/// de quem ficou órfão não pisca. É a frase do aceite — «saída encerra o
/// repasse» — medida onde alguém a sentiria.
///
/// # O que este teste prova, e o que ele não prova
///
/// Ele **não** prende a linha que reabre o cano na saída. Medido: revertendo-a,
/// estes trinta quadros chegam igualzinho, porque numa máquina só o relato de
/// fim limpo do par volta dentro de um intervalo de quadro e reabre o cano por
/// outro caminho. Quem prende aquela linha é
/// [`a_saida_de_quem_empresta_reabre_o_cano_de_quem_ficou_orfao_sem_esperar_relato`],
/// que tira o relato da conta.
///
/// O que este prende são os **dois** caminhos de uma vez: se nem o servidor
/// reabrir na saída nem o relato chegar, a imagem para, e é aqui que isso
/// aparece. É a diferença entre medir um mecanismo e medir a promessa, e este
/// arquivo precisa dos dois.
#[tokio::test(flavor = "multi_thread")]
async fn quem_empresta_saindo_da_sala_nao_deixa_quem_estava_atras_dele_sem_imagem() -> Result<()> {
    let Cenario {
        servidor,
        compartilha,
        empresta,
        mut assiste,
        screen,
        copias,
    } = cenario().await?;

    let piso = ate_o_par_estar_servindo(
        &mut assiste,
        screen,
        &copias,
        "a saída de quem empresta da sala",
    )
    .await?;
    println!("o par serviu até o quadro {piso:?}; agora quem empresta sai da sala");

    let saiu_em = Instant::now();
    empresta.sair_da_voice_room().await?;

    // O piso é relido **depois** da saída: o que já estava na fila atravessou
    // enquanto o par ainda servia, e um quadro velho não diz nada sobre o cano
    // que o servidor tinha de reabrir.
    let depois = maior_seq_ja_enfileirado(&mut assiste, screen).await;
    let mut anterior = depois;
    for indice in 0..QUADROS_PARA_PROVAR {
        let chegou = esperar(
            &mut assiste,
            "um quadro chegar depois de quem emprestava sair da sala",
            |aviso| match aviso {
                Aviso::TelaQuadro { tela, bytes, .. } if *tela == screen => seq_de(bytes)
                    .filter(|seq| anterior.is_none_or(|p| *seq > p))
                    .map(|seq| (seq, bytes.clone())),
                _ => None,
            },
        )
        .await;
        let (seq, bytes) = match chegou {
            Ok(quadro) => quadro,
            Err(erro) => panic!(
                "a imagem de quem estava atrás do par parou no quadro {indice} de \
                 {QUADROS_PARA_PROVAR} depois de quem emprestava sair da sala: nem o servidor \
                 reabriu o cano na saída, nem o relato de fim do par chegou. {erro}"
            ),
        };
        if indice == 0 {
            let intervalo = saiu_em.elapsed();
            println!("o primeiro quadro depois da saída chegou em {intervalo:?}");
            assert!(
                intervalo < GRACA_DA_SAIDA,
                "a imagem de quem estava atrás do par voltou só depois de {intervalo:?}: para \
                 quem está olhando, a tela congelou"
            );
        }
        assert_eq!(
            bytes,
            corpo(seq),
            "o quadro {seq} chegou e não é o que saiu de quem compartilha (quadro {indice} de \
             {QUADROS_PARA_PROVAR})"
        );
        anterior = Some(seq);
    }
    println!("{QUADROS_PARA_PROVAR} quadros seguidos chegaram a quem ficou órfão depois da saída");

    drop(compartilha);
    drop(empresta);
    drop(assiste);
    servidor.shutdown();
    Ok(())
}

/// **Quem sai da sessão de propósito para de receber, e o caminho do par cai
/// junto.**
///
/// # O que sobrevivia à saída
///
/// `Comando::Sair` faz `Motor::encerrar` e devolve de `Motor::rodar`; o `Motor`
/// é solto ali, e com ele o mapa de `caminhos_de_par`. **Largar um `JoinHandle`
/// desprende a tarefa, não a cancela**: a tarefa que lia a tela no par
/// continuava viva, com a conexão QUIC de pé, gastando a subida de quem
/// empresta para uma sessão que já tinha acabado — e continuava escrevendo no
/// canal de avisos, que o `Enlace` ainda segura depois de `sair()`.
///
/// O sintoma é o que este teste mede: **quadro de tela chegando depois do
/// `Encerrado`**. Não é um detalhe interno — é a casca recebendo imagem de uma
/// sessão que ela já fechou.
///
/// # A prova é o silêncio, e ela precisa das duas metades
///
/// Só «parou de chegar quadro» não basta: a transmissão podia ter acabado, e aí
/// o silêncio não diz nada sobre a saída. Quem empresta — que continua na sala
/// assistindo pelo servidor — é lido no mesmo intervalo e tem de continuar
/// recebendo. É a mesma forma de
/// [`um_unwatch_derruba_o_caminho_do_par_e_a_imagem_para`], aplicada ao fim da
/// sessão inteira em vez de ao fim de uma tela.
#[tokio::test(flavor = "multi_thread")]
async fn uma_saida_voluntaria_derruba_o_caminho_do_par_e_a_tela_para() -> Result<()> {
    let Cenario {
        servidor,
        compartilha,
        mut empresta,
        mut assiste,
        screen,
        copias,
    } = cenario().await?;

    let piso = ate_o_par_estar_servindo(&mut assiste, screen, &copias, "a saída").await?;
    println!("o par está servindo até o quadro {piso:?}; agora quem assiste sai da sessão");

    assiste.sair().await;
    esperar(&mut assiste, "o fim da sessão chegar à casca", |aviso| {
        matches!(aviso, Aviso::Encerrado(_)).then_some(())
    })
    .await?;

    // **Drenar antes de afirmar.** O canal de avisos é FIFO e o par entregou
    // quadros até o instante do pedido; exigir silêncio sem esvaziar a fila
    // seria falhar por causa de imagem que já tinha chegado.
    let ultimo = drenar_depois_do_fim(&mut assiste, screen).await;
    println!("a fila de quem saiu esvaziou no quadro {ultimo:?}");

    // E agora o silêncio, com a transmissão comprovadamente no ar.
    let fim = Instant::now() + SILENCIO_PARA_PROVAR;
    while Instant::now() < fim {
        match tokio::time::timeout(Duration::from_millis(50), assiste.proximo()).await {
            Ok(Aviso::TelaQuadro { tela, bytes, .. }) => assert!(
                tela != screen,
                "quem saiu da sessão recebeu o quadro {:?} depois do `Encerrado`: a tarefa do \
                 caminho do par sobreviveu ao motor que a criou, e continua lendo do par e \
                 gastando a subida de quem empresta por uma sessão que já acabou",
                seq_de(&bytes)
            ),
            // A pausa é o que impede este laço de virar espera ocupada: com o
            // caminho do par derrubado não há remetente nenhum, e `proximo`
            // devolve `Encerrado` sem custo nenhum, sem parar.
            Ok(_) => tokio::time::sleep(Duration::from_millis(20)).await,
            Err(_) => {}
        }
    }
    let ainda_chega = maior_seq_ja_enfileirado(&mut empresta, screen).await;
    assert!(
        ainda_chega.is_some(),
        "quem empresta parou de receber junto: o silêncio de quem saiu não prova nada, porque a \
         transmissão inteira pode ter morrido"
    );
    println!(
        "um segundo sem um único quadro para quem saiu, com quem empresta recebendo até o quadro \
         {ainda_chega:?}"
    );

    drop(compartilha);
    drop(empresta);
    drop(assiste);
    servidor.shutdown();
    Ok(())
}

/// Quanto tempo o espectador novo pode passar sem imagem antes de a prova de
/// que a vaga voltou virar uma prova vazia.
///
/// # É tela preta, e não velocidade
///
/// Quando o servidor aponta um par, ele **desliga** o cano daquela pessoa: cada
/// milissegundo sem quadro daqui para frente é um milissegundo em que ela não vê
/// nada. Um teste que só esperasse «a imagem acabou chegando» daria por boa uma
/// máquina que serve com três segundos de tela preta no meio — e é exatamente
/// isso que a reversão dos guardas produz.
///
/// # Por que um segundo, e não um número redondo qualquer
///
/// Medido, três vezes, com os guardas de pé: o primeiro quadro pelo par chega em
/// **232, 235 e 232 ms**. A maior parte disso é [`seele_core`]`::enlace::
/// ESPERA_DO_FURO` — os 200 ms fixos que quem assiste espera entre avisar o
/// ponto de encontro e discar —, e não trabalho de processador, que é o que
/// torna o número estável numa máquina carregada.
///
/// Do outro lado: com a vaga de quem empresta presa, o pedido é recusado **em
/// silêncio** e a discagem gasta `PRAZO_DO_PAR` inteiro, três segundos, antes de
/// a imagem aparecer por qualquer outro caminho. Medido também: 2,9 s.
///
/// Um segundo é quatro vezes o caso bom e um terço do custo da recusa. Os dois
/// lados cabem folgados, e nenhum encosta no outro.
const SEM_IMAGEM_TOLERAVEL: Duration = Duration::from_secs(1);

/// **Destruir o `Enlace` encerra o caminho por par que ele criou — na rede, e
/// não só na posse.**
///
/// # A lacuna que este teste fecha
///
/// A prova de que `Enlace::drop` derruba as tarefas de par existia em duas
/// metades, e nenhuma delas era esta. `enlace::testes::
/// destruir_o_motor_aborta_as_tarefas_de_par_que_ele_guarda` solta o **`Motor`**
/// à mão e mede que o dono cancela o que guarda — que `Enlace::drop` solta esse
/// dono era, até aqui, propriedade da linguagem sem teste próprio. E
/// [`uma_saida_voluntaria_derruba_o_caminho_do_par_e_a_tela_para`] atravessa a
/// rede inteira, mas por `sair()`: ali o `Enlace` **sobrevive** ao corte, e é
/// justamente por sobreviver que aquele teste pode ler o silêncio no canal de
/// avisos dele.
///
/// Um `Enlace` destruído não tem canal para ler. É a razão de esta prova não
/// existir antes, e é o que decide a forma dela.
///
/// # De onde a prova sai, já que a ponta que sumiu não fala mais
///
/// De **quem emprestava**. A vaga de `atendendo_pares` é uma por máquina — no A1
/// quem empresta serve um par por vez —, e ela só volta quando a tarefa que
/// serve termina, o que só acontece quando a conexão QUIC com quem assistia
/// morre. Enquanto o caminho de par do `Enlace` destruído estiver de pé, quem
/// emprestava está **ocupado servindo um fantasma**: o servidor o aponta, porque
/// do lado dele a vaga voltou junto com a sessão que caiu, e o cliente recusa o
/// pedido em silêncio pela vaga que ficou.
///
/// Então a afirmação é a que se pode ver de fora: um espectador **novo**, um
/// `Enlace` de verdade que entra depois, é servido **pelo mesmo par**. Não é
/// estado interno de ninguém — é imagem atravessando um cano que o servidor não
/// subiu, por uma máquina que só pode servi-la se tiver largado a anterior.
///
/// # As quatro coisas que impedem esta prova de ser vazia
///
/// 1. **A transmissão continua no ar.** Quem empresta é lido logo depois da
///    destruição e tem de receber [`QUADROS_PARA_PROVAR`] quadros novos. Sem
///    isso, tudo o que vem depois poderia estar medindo um ambiente que acabou —
///    a aprovação por encerramento de todo o ambiente.
/// 2. **O piso é tirado depois da nomeação, e não antes.** Entre entrar na sala
///    e o par ser apontado, o servidor pode ter ligado o espectador novo ao cano
///    dele e enfileirado quadros. Drenar **depois** de o servidor ter apontado o
///    par e desligado esse cano é o que separa quadro enfileirado de atividade
///    nova; é a perna 2 de
///    [`quando_o_par_morre_o_servidor_assume_e_ninguem_perde_imagem`], pelo mesmo
///    motivo.
/// 3. **O contador do servidor fica em uma cópia do primeiro quadro ao último.**
///    Essa cópia é a de quem empresta. Um segundo inteiro de imagem chegando ao
///    espectador novo com o servidor subindo uma cópia só não cabe em voo
///    nenhum: só cabe vindo do par.
/// 4. **A imagem tem de chegar dentro de [`SEM_IMAGEM_TOLERAVEL`].** Esta é a
///    perna que a primeira versão deste teste não tinha, e sem ela ele **passava
///    com os guardas retirados**: uma vaga presa não impede a imagem de aparecer,
///    ela a atrasa em `PRAZO_DO_PAR` — três segundos de tela preta com o cano do
///    servidor já desligado. Medido, e não suposto: 230 ms com os guardas de pé,
///    2,9 s sem eles.
#[tokio::test(flavor = "multi_thread")]
async fn destruir_o_enlace_encerra_o_caminho_do_par_e_quem_emprestava_volta_a_servir() -> Result<()>
{
    let Cenario {
        servidor,
        compartilha,
        mut empresta,
        mut assiste,
        screen,
        copias,
    } = cenario().await?;
    let endereco = servidor.local_addr()?;
    let sala = compartilha.sala;
    let quem_empresta = empresta.sessao().person;

    let piso = ate_o_par_estar_servindo(&mut assiste, screen, &copias, "a destruição").await?;
    {
        let pares = servidor.server().pares.lock().await;
        assert!(
            pares.ja_servindo().contains(&quem_empresta),
            "o servidor apontou um par e não o contou como ocupado: o que for destruído agora não \
             tinha caminho de par nenhum"
        );
    }
    println!("o par serviu até o quadro {piso:?}; agora o `Enlace` de quem assiste é destruído");

    // **Destruído, e não `sair()`.** `Enlace::drop` aborta a tarefa do motor sem
    // passar por `Motor::encerrar`: não há `Comando::Sair`, não há
    // `Client::disconnect`, e não há `Aviso::Encerrado` — a casca simplesmente
    // some, que é o que acontece quando a janela fecha ou o processo cai. O
    // caminho de par foi criado por este `Enlace` e não é filho da tarefa dele;
    // se ninguém o derrubar, ele fica lendo do par com a conexão QUIC de pé.
    drop(assiste);

    // Perna 1: o ambiente **não** acabou. Quem empresta continua na sala, vendo
    // a tela pelo servidor, e recebe imagem nova depois da destruição.
    let mut piso_dele = maior_seq_ja_enfileirado(&mut empresta, screen).await;
    for _ in 0..QUADROS_PARA_PROVAR {
        let (seq, _) = esperar(
            &mut empresta,
            "quem empresta receber imagem nova depois de o outro `Enlace` ser destruído",
            |aviso| match aviso {
                Aviso::TelaQuadro { tela, bytes, .. } if *tela == screen => seq_de(bytes)
                    .filter(|seq| piso_dele.is_none_or(|p| *seq > p))
                    .map(|seq| (seq, ())),
                _ => None,
            },
        )
        .await?;
        piso_dele = Some(seq);
    }
    println!("a transmissão continua no ar: quem empresta recebeu até o quadro {piso_dele:?}");

    // O servidor solta a nomeação de quem sumiu — sem isto ele nunca escolheria
    // este par de novo, e o que vem abaixo mediria a recusa **dele**, não a do
    // cliente. É pré-condição do teste, e não a afirmação dele.
    let fim = Instant::now() + PACIENCIA;
    loop {
        let ocupados = servidor.server().pares.lock().await.ja_servindo();
        if ocupados.is_empty() {
            break;
        }
        assert!(
            Instant::now() < fim,
            "o `Enlace` que assistia foi destruído e o servidor continua contando o par como \
             ocupado ({ocupados:?})"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // A afirmação: um espectador novo, e o **mesmo** par o serve.
    let mut de_novo = cliente(endereco, 4, "assiste-de-novo").await?;
    de_novo.entrar_na_voice_room(sala, None).await?;
    // **Consentir precede ser apontado**, e sem esta linha o servidor recusa:
    // `SirvaTelaPara` entregaria o endereço deste espectador a quem empresta,
    // e ninguém tomou essa decisão por ele. Medido: sem ela, este teste falha
    // em «o servidor não apontou quem empresta (apontado: None)».
    de_novo
        .consentir_no_caminho_entre_pares(ASSISTE_POR_PAR)
        .await?;
    let pedido_em = Instant::now();
    de_novo.assistir(screen, true).await?;

    let fim = Instant::now() + PACIENCIA;
    loop {
        let apontado = servidor
            .server()
            .pares
            .lock()
            .await
            .quem_foi_apontado(screen, de_novo.sessao().person);
        if apontado == Some(quem_empresta) {
            break;
        }
        assert!(
            Instant::now() < fim,
            "o servidor não apontou quem empresta para servir o espectador novo (apontado: \
             {apontado:?})"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    ate(
        "o servidor desligar o cano do espectador novo, por ter apontado o par",
        || copias.agora() == 1,
    )
    .await?;

    // Perna 2: o piso sai **daqui**, depois de o cano do servidor estar
    // desligado. Nada que já estivesse na fila pode provar o que vem abaixo.
    let mut piso_novo = maior_seq_ja_enfileirado(&mut de_novo, screen).await;
    println!("a fila do espectador novo esvaziou no quadro {piso_novo:?}; daqui para cima é o par");

    // Perna 3: sustentação, com o contador do servidor em uma cópia do primeiro
    // quadro ao último.
    for indice in 0..QUADROS_PARA_PROVAR {
        let (seq, bytes) = esperar(
            &mut de_novo,
            "o espectador novo receber um quadro pelo par, acima do piso",
            |aviso| match aviso {
                Aviso::TelaQuadro { tela, bytes, .. } if *tela == screen => seq_de(bytes)
                    .filter(|seq| piso_novo.is_none_or(|p| *seq > p))
                    .map(|seq| (seq, bytes.clone())),
                _ => None,
            },
        )
        .await?;
        assert_eq!(
            copias.agora(),
            1,
            "o servidor voltou a subir a cópia do espectador novo (quadro {indice} de \
             {QUADROS_PARA_PROVAR}, seq {seq}): quem emprestava recusou o pedido em silêncio, \
             porque a vaga dele continua tomada pelo caminho de par do `Enlace` destruído — a \
             tarefa sobreviveu à destruição e esta máquina saiu da malha sem erro em lugar nenhum"
        );
        assert_eq!(
            bytes,
            corpo(seq),
            "o que chegou pelo par não é o que saiu de quem compartilha (quadro {indice} de \
             {QUADROS_PARA_PROVAR}, seq {seq})"
        );
        // **A vaga voltou, ou a imagem só apareceu depois de o silêncio custar
        // `PRAZO_DO_PAR`.** Ver [`SEM_IMAGEM_TOLERAVEL`]: sem este prazo, o
        // teste dá por boa uma máquina que recusa o pedido em silêncio e deixa
        // o espectador novo três segundos no escuro antes de a imagem enfim vir.
        if indice == 0 {
            let esperou = pedido_em.elapsed();
            assert!(
                esperou < SEM_IMAGEM_TOLERAVEL,
                "o espectador novo passou {esperou:?} sem um quadro depois de pedir a tela, com o \
                 cano do servidor já desligado: quem emprestava recusou o pedido em silêncio pela \
                 vaga que o caminho de par do `Enlace` destruído não devolveu, e a imagem só \
                 chegou depois de a discagem gastar o prazo inteiro"
            );
            println!("o primeiro quadro pelo par chegou em {esperou:?}");
        }
        piso_novo = Some(seq);
    }
    println!(
        "{QUADROS_PARA_PROVAR} quadros seguidos chegaram ao espectador novo pelo mesmo par, até o \
         {piso_novo:?}: o caminho de par do `Enlace` destruído tinha mesmo acabado"
    );

    drop(compartilha);
    drop(empresta);
    drop(de_novo);
    servidor.shutdown();
    Ok(())
}

/// A geração de mídia de antes da queda do servidor.
const ANTES_DA_QUEDA: u32 = 0;

/// A geração de mídia da transmissão que nasce depois da volta.
///
/// Um número diferente, e não «a mesma tela de novo»: com o `ScreenId`
/// recomeçando do mesmo lugar num daemon reiniciado (ver [`PASSO_DA_GERACAO`]),
/// é a marca no corpo do quadro que separa recuperação de fila velha.
const DEPOIS_DA_VOLTA: u32 = 1;

/// **A conexão que caiu não fala pela que a substitui, e o par volta a servir a
/// substituta.**
///
/// # A lacuna que este teste fecha
///
/// [`Motor::cair`](seele_core::enlace) é o **único** caminho em que o motor
/// sobrevive ao corte: a sessão entra na bateria interna, e volta noutra
/// conexão. Todos os outros fins de caminho de par deste arquivo matam o motor
/// junto — `sair()`, a destruição do `Enlace`, a morte do par —, e por isso
/// nenhum deles exercita `Motor::largar_o_caminho_entre_pares`, que é o guarda
/// escrito para **esta** substituição.
///
/// Ele existia provado por unidade, em duas metades, e nenhuma delas toca a
/// rede: `enlace::testes::cair_devolve_a_vaga_de_quem_estava_servindo_um_par`
/// monta a tarefa que serve à mão, e
/// `enlace::testes::cair_nao_deixa_a_fila_da_conexao_velha_alcancar_a_substituta`
/// escreve na fila à mão e a lê de volta. A costura — servidor de verdade caindo
/// e voltando, dois `Enlace` públicos atravessando a bateria, e o caminho por
/// par tendo de existir de novo do outro lado — é o que faltava, e é o §8 do
/// relatório da entrega anterior.
///
/// # Reconexão, e não destino novo
///
/// O servidor volta **na mesma porta e com o mesmo banco** — a receita de
/// `bateria_interna.rs`. As duas metades importam: a porta, porque o `Destino`
/// do cliente não muda e a reconexão volta ao endereço que atendeu; o banco em
/// arquivo, porque com ele o certificado é o mesmo, e um certificado novo seria
/// **troca de identidade** para o TOFU do cliente — o alerta do ADR 0003, e o
/// contrário de uma reconexão. Que os dois `Enlace` voltem a `Online` sem serem
/// recusados é, por si, a prova de que foi reconexão.
///
/// # Os comandos explícitos, e o que **não** volta sozinho
///
/// `Motor::cair` zera `tela_pedida` e para a captura de propósito: uma
/// transmissão que voltasse sozinha poria a tela de alguém no ar sem ninguém ter
/// apertado nada. Este teste respeita isso e **não** introduz retomada
/// automática nenhuma — quem compartilhava reaperta o botão (um
/// `StartScreenShare` novo, numa conexão nova), e quem assistia pede a tela de
/// novo (`assistir(tela, true)`). O que o contrato manda voltar sozinho volta
/// sozinho, e é afirmado: a sala de voz e a **declaração de quem empresta**, que
/// `Motor::tentar` redeclara com a escolha guardada.
///
/// # As quatro coisas que impedem esta prova de ser vazia
///
/// 1. **Havia caminho de par antes da queda.** [`ate_o_par_estar_servindo`]
///    exige um segundo de imagem pelo par com o servidor subindo uma cópia só;
///    sem isso, o que se mede depois não diz nada sobre substituição de caminho
///    nenhum.
/// 2. **A mídia nova é distinguível da velha.** Toda afirmação daqui para baixo
///    exige [`DEPOIS_DA_VOLTA`] no corpo do quadro. Um quadro da geração antiga
///    chegando depois da volta **derruba o teste**, e é assim que a interferência
///    da conexão anterior aparece: o `ScreenId` sozinho não a acusaria, porque o
///    daemon reiniciado reemite o mesmo.
/// 3. **O piso é tirado depois da nomeação.** O servidor liga quem está na sala
///    ao cano dele quando a tela abre; drenar só depois de ele ter apontado o par
///    e desligado esse cano é o que separa quadro enfileirado de imagem nova.
/// 4. **A imagem tem de chegar dentro de [`SEM_IMAGEM_TOLERAVEL`].** Uma vaga de
///    atendimento que não voltou não impede a imagem de aparecer — ela a atrasa
///    em `PRAZO_DO_PAR`, com o cano do servidor já desligado. É tela preta, e sem
///    este prazo o teste a daria por boa.
#[tokio::test(flavor = "multi_thread")]
async fn a_reconexao_ao_servidor_nao_deixa_a_conexao_velha_atrapalhar_o_par_novo() -> Result<()> {
    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");
    let (endereco, primeiro) = servidor_em(0, Location::File(banco.clone())).await?;
    let porta = endereco.port();

    let Cenario {
        servidor,
        compartilha,
        mut empresta,
        mut assiste,
        screen,
        copias,
    } = cenario_sobre(endereco, primeiro, ANTES_DA_QUEDA).await?;

    let quem_empresta = empresta.sessao().person;
    let quem_assiste = assiste.sessao().person;
    let conexao_velha_de_quem_empresta = empresta.sessao().id;
    let conexao_velha_de_quem_assiste = assiste.sessao().id;

    let piso =
        ate_o_par_estar_servindo(&mut assiste, screen, &copias, "a queda do servidor").await?;
    {
        let pares = servidor.server().pares.lock().await;
        assert!(
            pares.ja_servindo().contains(&quem_empresta),
            "o servidor apontou um par e não o contou como ocupado: não havia caminho de par \
             para a queda substituir"
        );
    }
    println!(
        "o par serviu a tela {screen} até o quadro {piso:?} (geração {ANTES_DA_QUEDA}); agora o \
         servidor cai"
    );

    // ---- a queda, do jeito que `bateria_interna.rs` a produz
    servidor.shutdown();
    servidor.wait_idle().await;
    drop(servidor);

    // **Os dois `Enlace` continuam vivos, e é isso que os distingue dos outros
    // testes deste arquivo.** Ninguém foi destruído nem saiu: o motor sobrevive
    // ao corte, entra na bateria, e é dele que a limpeza do caminho de par tem
    // de partir.
    esperar(&mut assiste, "quem assiste entrar na bateria", |aviso| {
        matches!(
            aviso,
            Aviso::Estado {
                estado: Link::InternalBattery { .. },
                ..
            }
        )
        .then_some(())
    })
    .await?;
    esperar(&mut empresta, "quem empresta entrar na bateria", |aviso| {
        matches!(
            aviso,
            Aviso::Estado {
                estado: Link::InternalBattery { .. },
                ..
            }
        )
        .then_some(())
    })
    .await?;
    println!("os dois `Enlace` viram a queda e entraram na bateria interna");

    // ---- e o servidor volta: mesma porta, mesmo banco, mesmo certificado
    let (_, servidor) = servidor_em(porta, Location::File(banco)).await?;

    esperar(&mut assiste, "quem assiste reconectar", |aviso| {
        matches!(aviso, Aviso::Reconectado { .. }).then_some(())
    })
    .await?;
    esperar(&mut empresta, "quem empresta reconectar", |aviso| {
        matches!(aviso, Aviso::Reconectado { .. }).then_some(())
    })
    .await?;
    assert_eq!(assiste.estado(), Link::Online);
    assert_eq!(empresta.estado(), Link::Online);

    // **A conexão é outra, e quem diz isso é o par de avisos — nunca o
    // `SessionId`.**
    //
    // Medido, e a primeira versão deste teste errava aqui: `SessionId` sai de
    // `Registry::issue`, o mesmo contador em memória que emite o `ScreenId`
    // (ver [`PASSO_DA_GERACAO`]). Um daemon reiniciado recomeça a numerar do
    // mesmo lugar, e a conexão nova pode receber **exatamente o id da que
    // caiu** — depende só da ordem em que as três reconexões chegam. Um
    // `assert_ne!` sobre ele reprovava uma volta em cada oito, e reprovava por
    // motivo errado: não havia defeito nenhum nas voltas que ele derrubou.
    //
    // O que prova a substituição é o caminho público que este teste já
    // atravessou: `InternalBattery` (a conexão morreu) seguido de
    // `Reconectado`, que `Motor::tentar` só emite depois de um `Client` novo ter
    // apertado a mão. Os ids ficam impressos porque ajudam a ler o rastro, e
    // não porque afirmem alguma coisa.
    assert_eq!(
        assiste.sessao().person,
        quem_assiste,
        "quem assiste voltou como outra pessoa: isto é troca de identidade, e não reconexão"
    );
    assert_eq!(
        empresta.sessao().person,
        quem_empresta,
        "quem empresta voltou como outra pessoa: isto é troca de identidade, e não reconexão"
    );
    println!(
        "reconectados: quem assiste {conexao_velha_de_quem_assiste} → {}, quem empresta \
         {conexao_velha_de_quem_empresta} → {} (as pessoas são as mesmas)",
        assiste.sessao().id,
        empresta.sessao().id
    );

    // **A declaração de quem empresta volta sozinha, e é contrato.**
    // `Motor::tentar` a refaz com a escolha guardada — sem isso, quem optou por
    // emprestar sairia da malha na primeira queda de rede sem nada na tela mudar,
    // e o resto deste teste mediria o caminho de sempre.
    {
        let fim = Instant::now() + PACIENCIA;
        loop {
            let declarado = {
                let pares = servidor.server().pares.lock().await;
                pares
                    .declaracao_de(quem_empresta)
                    .map(|declaracao| declaracao.consentimento.empresta_conexao())
            };
            if declarado == Some(true) {
                break;
            }
            assert!(
                Instant::now() < fim,
                "quem empresta reconectou e o servidor novo não o tem como quem empresta \
                 (declaração: {declarado:?}): a escolha não sobreviveu à substituição da conexão"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    // ---- a transmissão recomeça, e **por um comando explícito**
    //
    // A captura morre na queda por desenho (`Motor::cair` para a tela e esquece o
    // pedido), então quem compartilha reconecta e reaperta o botão. Nada aqui
    // retoma compartilhamento sozinho — seria construir a resposta que o teste
    // deveria estar exigindo.
    drop(compartilha);
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
    // Idem: antes da transmissão nova, e não depois dela.
    let ouve = servidor.server().events.subscribe();
    let nova = compartilhar_desde(&mut compartilha, primeiro_seq_da(DEPOIS_DA_VOLTA)).await?;
    let copias = ContadorDeCopias::seguindo(ouve, nova);
    println!(
        "a transmissão nova é a tela {nova} (a de antes era a {screen}), com mídia da geração \
         {DEPOIS_DA_VOLTA}"
    );

    esperar(&mut assiste, "quem assiste ver a tela nova", |aviso| {
        matches!(aviso, Aviso::TelaAbriu { tela, .. } if *tela == nova).then_some(())
    })
    .await?;
    esperar(&mut empresta, "quem empresta ver a tela nova", |aviso| {
        matches!(aviso, Aviso::TelaAbriu { tela, .. } if *tela == nova).then_some(())
    })
    .await?;
    ate(
        "o servidor contar as duas cópias que ele mesmo sobe",
        || copias.agora() == 2,
    )
    .await?;

    // O outro comando explícito: quem assistia pede a tela **de novo**. O pedido
    // anterior morreu com a conexão anterior, e é assim que tem de ser.
    let pedido_em = Instant::now();
    assiste.assistir(nova, true).await?;

    let fim = Instant::now() + PACIENCIA;
    loop {
        let apontado = servidor
            .server()
            .pares
            .lock()
            .await
            .quem_foi_apontado(nova, quem_assiste);
        if apontado == Some(quem_empresta) {
            break;
        }
        assert!(
            Instant::now() < fim,
            "o servidor não apontou quem empresta para servir a transmissão nova (apontou \
             {apontado:?}): a vaga de atendimento dele pode não ter voltado da conexão anterior"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    ate(
        "o servidor desligar o cano de quem assiste, por ter apontado o par",
        || copias.agora() == 1,
    )
    .await?;

    // Perna 3: o piso sai **daqui**, com o cano do servidor já desligado.
    let mut piso_novo = maior_seq_ja_enfileirado(&mut assiste, nova).await;
    println!("a fila de quem assiste esvaziou no quadro {piso_novo:?}; daqui para cima é o par");

    for indice in 0..QUADROS_PARA_PROVAR {
        let (seq, bytes) = esperar(
            &mut assiste,
            "um quadro da geração nova chegar pelo par depois da reconexão, acima do piso",
            |aviso| match aviso {
                Aviso::TelaQuadro { tela, bytes, .. } if *tela == nova => {
                    let seq = seq_de(bytes)?;
                    // **A interferência da conexão anterior morre aqui.** O
                    // `ScreenId` não a acusaria: o daemon reiniciado reemitiu o
                    // mesmo nome, e um quadro entregue por uma tarefa de par que
                    // sobrevivesse à queda entraria por este mesmo braço com o
                    // rótulo certo. A marca no corpo é o que os separa.
                    assert_eq!(
                        geracao_de(seq),
                        DEPOIS_DA_VOLTA,
                        "chegou o quadro {seq}, da geração {} — imagem da conexão que caiu, \
                         entregue depois da reconexão e com o nome da transmissão nova: uma \
                         tarefa de par sobreviveu à substituição da conexão e continua \
                         escrevendo no canal de avisos desta sessão",
                        geracao_de(seq)
                    );
                    (piso_novo.is_none_or(|p| seq > p)).then(|| (seq, bytes.clone()))
                }
                _ => None,
            },
        )
        .await?;
        assert_eq!(
            copias.agora(),
            1,
            "o servidor voltou a subir a cópia de quem assiste (quadro {indice} de \
             {QUADROS_PARA_PROVAR}, seq {seq}): depois da reconexão o caminho por par não se \
             refez, e a subida que a malha existe para aliviar voltou a sair de quem hospeda"
        );
        assert_eq!(
            bytes,
            corpo(seq),
            "o que chegou pelo par não é o que saiu de quem compartilha (quadro {indice} de \
             {QUADROS_PARA_PROVAR}, seq {seq})"
        );
        if indice == 0 {
            let esperou = pedido_em.elapsed();
            assert!(
                esperou < SEM_IMAGEM_TOLERAVEL,
                "o espectador passou {esperou:?} sem um quadro depois de pedir a tela de novo, \
                 com o cano do servidor já desligado: quem emprestava recusou o pedido em \
                 silêncio pela vaga que o caminho de par da conexão anterior não devolveu, e a \
                 imagem só chegou depois de a discagem gastar o prazo inteiro"
            );
            println!("o primeiro quadro pelo par chegou em {esperou:?}");
        }
        piso_novo = Some(seq);
    }
    println!(
        "{QUADROS_PARA_PROVAR} quadros seguidos da geração {DEPOIS_DA_VOLTA} chegaram pelo par \
         depois da reconexão, até o {piso_novo:?}, com o servidor subindo uma cópia só"
    );

    // E o servidor não guarda nomeação nenhuma da conexão que caiu: a única de
    // pé é a que ele fez para a substituta.
    {
        let pares = servidor.server().pares.lock().await;
        let ocupados = pares.ja_servindo();
        assert!(
            ocupados.len() == 1 && ocupados.contains(&quem_empresta),
            "o servidor conta como ocupado alguém que não é o par apontado agora ({ocupados:?})"
        );
    }

    drop(compartilha);
    drop(empresta);
    drop(assiste);
    servidor.shutdown();
    Ok(())
}

// ------------------------------------------------------- a queda assimétrica

/// Um relé UDP: um segundo endereço que leva ao mesmo servidor, e que se corta.
///
/// # Por que um relé, e não `Daemon::shutdown`
///
/// Derrubar o servidor mata **os dois lados** de todo caminho de par: o fluxo de
/// quem empresta termina, `RepasseDeTela::fechou` larga o destino, e a tarefa
/// que lia do par vê o fim do fluxo e morre sozinha. É por isso que o teste da
/// substituição da conexão não conseguiu prender
/// `Motor::largar_o_caminho_entre_pares` — naquele cenário ele é cinto sobre
/// suspensório, e o relatório de 10/09 mediu isso em vez de supor.
///
/// A queda que **precisa** do guarda é a assimétrica: a conexão com o servidor
/// morre e o par continua vivo do outro lado. Um relé produz exatamente isso —
/// quem assiste fala com o servidor por aqui, quem compartilha e quem empresta
/// falam direto, e cortar o relé derruba uma conexão e só ela. O caminho entre
/// pares não passa por aqui: ele é discado direto para o endereço que quem
/// empresta publicou.
struct Rele {
    endereco: SocketAddr,
    tarefa: tokio::task::JoinHandle<()>,
}

impl Rele {
    /// Abre um relé numa porta que o sistema escolhe.
    async fn abrir(destino: SocketAddr) -> Result<Self> {
        Self::na_porta(0, destino).await
    }

    /// Abre um relé numa porta escolhida — **a mesma na segunda vez**.
    ///
    /// Pelo motivo que [`servidor_em`] documenta: o `Destino` que o cliente
    /// guarda não muda, e a reconexão volta ao endereço que atendeu. Um relé que
    /// voltasse noutra porta seria destino novo, e não reconexão.
    async fn na_porta(porta: u16, destino: SocketAddr) -> Result<Self> {
        let socket = tokio::net::UdpSocket::bind(SocketAddr::from(([127, 0, 0, 1], porta))).await?;
        let endereco = socket.local_addr()?;
        let tarefa = tokio::spawn(async move {
            // Um socket só para as duas direções: o que vem do servidor vai
            // para o último cliente que falou, e o resto vai para o servidor.
            // Basta porque **uma** conexão passa por aqui, e é a única coisa
            // que este relé promete.
            let mut balde = vec![0_u8; 64 * 1024];
            let mut cliente: Option<SocketAddr> = None;
            loop {
                let Ok((quantos, de)) = socket.recv_from(&mut balde).await else {
                    return;
                };
                let Some(pedaco) = balde.get(..quantos) else {
                    continue;
                };
                if de == destino {
                    if let Some(para) = cliente {
                        let _ = socket.send_to(pedaco, para).await;
                    }
                } else {
                    // A porta de origem muda a cada reconexão — `Motor::tentar`
                    // sai de um socket novo —, então o cliente é sempre o
                    // último que falou, e não o primeiro.
                    cliente = Some(de);
                    let _ = socket.send_to(pedaco, destino).await;
                }
            }
        });
        Ok(Self { endereco, tarefa })
    }

    /// Corta o caminho e devolve a porta, para que ele possa voltar nela.
    ///
    /// Espera a tarefa terminar de verdade antes de devolver: sem isso a porta
    /// ainda estaria presa quando o relé novo tentasse tomá-la, e o teste
    /// falharia por corrida de bind em vez de por afirmação nenhuma.
    async fn cortar(self) -> u16 {
        let porta = self.endereco.port();
        let tarefa = self.tarefa;
        tarefa.abort();
        let _ = tarefa.await;
        porta
    }
}

/// Quanto silêncio se exige de quem assiste enquanto a bateria corre.
///
/// Dois segundos, e o número é escolhido contra o que ele tem de separar: a
/// transmissão manda um quadro a cada [`INTERVALO`], trinta por segundo, então
/// um caminho de par vivo entrega uns **sessenta** quadros nesta janela. Não é
/// um prazo apertado que alguma máquina lenta derrubaria; é a diferença entre
/// zero e sessenta.
const SILENCIO_DA_QUEDA: Duration = Duration::from_secs(2);

/// Quanto se drena antes de exigir o silêncio.
///
/// O canal de avisos é FIFO e a bateria começa **quatorze segundos** depois do
/// corte: o que estava em voo no instante do cancelamento ainda está na fila, e
/// exigir silêncio sem esvaziá-la mediria a fila e não o caminho. Fixo, e não
/// «até ficar quieto», pelo motivo que [`DRENAGEM_DEPOIS_DO_FIM`] documenta.
const DRENAGEM_ANTES_DO_SILENCIO: Duration = Duration::from_millis(500);

/// **A conexão de quem assiste morre sozinha, o par continua vivo, e o caminho
/// entre pares cai junto com ela.**
///
/// # A lacuna que este teste fecha, e por que ela sobreviveu a três entregas
///
/// `Motor::largar_o_caminho_entre_pares` foi escrito para a queda, e até aqui
/// nenhum teste de rede o prendeu. O relatório de 10/09 mediu por quê e deixou
/// escrito: naquele cenário — o **servidor** caindo — o guarda é cinto sobre
/// suspensório. A queda do servidor mata os dois lados de todo caminho de par,
/// a tarefa que lê do par vê o fim do fluxo e morre sozinha, e retirar o guarda
/// não muda nada que se possa medir.
///
/// A queda que precisa dele é a **assimétrica**: a conexão com o servidor morre
/// e o par continua vivo do outro lado. É a queda comum de verdade — a rota que
/// some, o NAT que reescreve, a máquina que dorme —, e é a que um [`Rele`]
/// produz: quem assiste fala com o servidor por um caminho que se pode cortar,
/// quem compartilha e quem empresta falam direto, e o caminho entre pares é
/// discado direto e não passa pelo relé.
///
/// # O defeito que este teste encontrou, e que ele existe para prender
///
/// Medido antes de consertado, com este mesmo cenário: **`Motor::cair` nunca
/// corria.** Há duas portas para a bateria interna e só uma passava por ele.
/// `cair` trata a queda que o transporte *avisa* — um fluxo que devolve erro. Uma
/// conexão que some sem avisar não devolve erro nenhum: ela produz silêncio, e
/// quem conta silêncio é o `Ping`. Três sem resposta e `Battery::poll_online`
/// põe a bateria de pé **por dentro**, devolvendo `Action::Wait`.
///
/// O resultado, medido: a tarefa de par da conexão morta atravessava a bateria e
/// a reconexão inteiras, e **1336 quadros da mídia velha** chegavam à casca
/// depois de `Reconectado`, por um caminho que o servidor novo não montou e não
/// conhece. O conserto é uma linha em `Motor::passo`: a porta dos pings passa
/// pela mesma soltura que a do erro.
///
/// # Por que a prova é a janela da bateria, e não uma marca na mídia
///
/// Entre `InternalBattery` e `Reconectado`, quem assiste **não tem conexão com o
/// servidor** — é o que a bateria significa. Nessa janela não existe cano do
/// servidor para confundir com caminho de par: qualquer quadro que chegue só
/// pode ter vindo do par. É um discriminador mais forte que uma geração de
/// mídia, que separa duas transmissões mas não duas origens.
///
/// A geração continua marcada ([`ANTES_DA_QUEDA`]) e continua servindo: ela é o
/// que faz a mensagem de falha dizer **de qual mídia** era o quadro que não
/// devia estar ali.
///
/// # As três pernas que impedem esta prova de ser vazia
///
/// 1. **Havia caminho de par antes do corte.** [`ate_o_par_estar_servindo`]
///    exige um segundo de imagem pelo par com o servidor subindo uma cópia só.
///    Sem isso o silêncio depois não diria nada.
/// 2. **O ambiente não acabou.** Quem empresta, que continua conectado direto,
///    recebe [`QUADROS_PARA_PROVAR`] quadros novos **na mesma janela** em que se
///    exige silêncio de quem assiste. Sem esta perna, um servidor morto ou uma
///    transmissão que parou passariam por conserto.
/// 3. **A sessão volta a funcionar.** Silêncio também é o que um cliente morto
///    produz. Depois de o relé voltar, quem assiste reconecta e volta a receber
///    imagem — e é aí que se sabe que o silêncio foi o caminho do par caindo, e
///    não o `Enlace` inteiro.
#[tokio::test(flavor = "multi_thread")]
async fn a_queda_de_uma_conexao_so_derruba_o_caminho_do_par_com_o_par_ainda_vivo() -> Result<()> {
    let (endereco, daemon) = servidor_em(0, Location::Memory).await?;
    let rele = Rele::abrir(endereco).await?;
    let endereco_do_rele = rele.endereco;

    let Cenario {
        servidor,
        compartilha,
        mut empresta,
        mut assiste,
        screen,
        copias,
    } = cenario_por(endereco, endereco_do_rele, daemon, ANTES_DA_QUEDA).await?;

    let piso =
        ate_o_par_estar_servindo(&mut assiste, screen, &copias, "a queda assimétrica").await?;
    println!(
        "o par serve quem assiste — piso {piso:?}, o servidor sobe {} cópia(s)",
        copias.agora()
    );

    // ---- o corte: uma conexão, e só ela ----
    //
    // Quem compartilha e quem empresta não passam por aqui, e o caminho entre
    // pares é discado direto para o endereço que quem empresta publicou. Nada
    // além da conexão de quem assiste com o servidor é tocado.
    let corte = Instant::now();
    let porta = rele.cortar().await;

    // Drenar até a bateria começar, e não esperar de braços cruzados: são uns
    // quatorze segundos de `Ping` sem resposta, e um canal FIFO que ninguém lê
    // acumula a transmissão inteira — quatrocentos quadros que depois seriam
    // lidos dentro da janela de silêncio, provando o contrário do que se quer.
    let mut na_bateria = false;
    while !na_bateria {
        anyhow::ensure!(
            corte.elapsed() < PACIENCIA,
            "a paciência acabou esperando a bateria interna começar depois do corte"
        );
        if let Ok(aviso) = tokio::time::timeout(Duration::from_millis(50), assiste.proximo()).await
        {
            na_bateria = matches!(
                aviso,
                Aviso::Estado {
                    estado: Link::InternalBattery { .. },
                    ..
                }
            );
        }
    }
    println!(
        "a bateria interna começou {:?} depois do corte",
        corte.elapsed()
    );

    // O que estava em voo no instante do cancelamento ainda está na fila.
    let fim_da_drenagem = Instant::now() + DRENAGEM_ANTES_DO_SILENCIO;
    while Instant::now() < fim_da_drenagem {
        let _ = tokio::time::timeout(Duration::from_millis(20), assiste.proximo()).await;
    }

    // ---- a afirmação: silêncio de um lado, imagem do outro, na mesma janela ----
    let mut de_quem_assiste: Vec<u32> = Vec::new();
    let mut de_quem_empresta = 0_u32;
    let fim_do_silencio = Instant::now() + SILENCIO_DA_QUEDA;
    while Instant::now() < fim_do_silencio {
        if let Ok(Aviso::TelaQuadro { tela, bytes, .. }) =
            tokio::time::timeout(Duration::from_millis(10), assiste.proximo()).await
        {
            if tela == screen {
                if let Some(seq) = seq_de(&bytes) {
                    de_quem_assiste.push(seq);
                }
            }
        }
        if let Ok(Aviso::TelaQuadro { tela, bytes, .. }) =
            tokio::time::timeout(Duration::from_millis(10), empresta.proximo()).await
        {
            if tela == screen && seq_de(&bytes).is_some() {
                de_quem_empresta += 1;
            }
        }
    }

    assert!(
        de_quem_empresta >= QUADROS_PARA_PROVAR,
        "quem empresta recebeu só {de_quem_empresta} quadro(s) nos {SILENCIO_DA_QUEDA:?} em que \
         se exigiu silêncio de quem assiste: o ambiente parou, e o silêncio do outro lado não \
         diz nada sobre caminho de par nenhum"
    );
    assert!(
        de_quem_assiste.is_empty(),
        "a conexão de quem assiste caiu e o caminho do par continuou entregando: {} quadro(s) \
         chegaram durante a bateria interna, o primeiro deles o seq {:?} da geração {:?}. \
         Nessa janela não há conexão com o servidor — a imagem só pode ter vindo da tarefa de \
         par da conexão que morreu",
        de_quem_assiste.len(),
        de_quem_assiste.first(),
        de_quem_assiste.first().copied().map(geracao_de),
    );
    println!(
        "{SILENCIO_DA_QUEDA:?} de silêncio para quem assiste, com {de_quem_empresta} quadros \
         novos para quem empresta na mesma janela"
    );

    // ---- a terceira perna: o silêncio não era um cliente morto ----
    let de_volta = Rele::na_porta(porta, endereco).await?;
    esperar(
        &mut assiste,
        "quem assiste reconectar ao servidor",
        |aviso| matches!(aviso, Aviso::Reconectado { .. }).then_some(()),
    )
    .await?;
    // Sem `assistir` nenhum: o servidor religa ao cano quem volta para uma sala
    // onde há transmissão, e é dele que esta imagem vem — `copias` volta a dois.
    // Aqui isso é exatamente o que se quer afirmar: **há um cliente vivo**.
    let (seq, _) = esperar(
        &mut assiste,
        "a imagem voltar a chegar depois da reconexão",
        |aviso| match aviso {
            Aviso::TelaQuadro { tela, bytes, .. } if *tela == screen => {
                seq_de(bytes).map(|seq| (seq, ()))
            }
            _ => None,
        },
    )
    .await?;
    println!("depois da volta, a imagem chega de novo — seq {seq}");

    let _ = de_volta.cortar().await;
    drop(compartilha);
    drop(empresta);
    drop(assiste);
    servidor.shutdown();
    Ok(())
}
