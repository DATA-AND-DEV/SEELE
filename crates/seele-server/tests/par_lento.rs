//! Um par que parou de ler não pode custar mensagem de quem falou.
//!
//! Pendência nº 1 de `docs/pendencias.md`: rajada de mensagens grandes perde
//! entrega. O que esta suíte reproduz é a metade do defeito que ainda existe
//! depois de as duas tarefas leitoras dedicadas terem entrado — a do cliente
//! (`seele-core/src/client.rs`) e a da sessão (`session::run_session`).
//!
//! O caminho é este. A sessão escreve para o par de dentro do mesmo `select!`
//! em que lê o barramento de eventos. Quando o par para de ler, a janela de
//! controle de fluxo do QUIC fecha, a escrita bloqueia, e enquanto ela está
//! bloqueada **ninguém tira evento do barramento**. O barramento é um
//! `broadcast` de capacidade fixa: passado o tamanho do anel, ele descarta o
//! mais antigo e devolve `Lagged` na próxima leitura. Um `let Ok(event) = event
//! else { continue }` transformava isso em nada — a sessão seguia, calada, com
//! um buraco permanente no que aquela pessoa vê.
//!
//! É por isso que a pendência é a número 1: a mensagem foi gravada em PERSISTENCE,
//! quem escreveu viu a sua aparecer, e o outro lado nunca soube que faltou.
//!
//! # Por que um par cru, e não um `Client`
//!
//! `seele-core` não pode ser dependência daqui (ADR 0002), e — mais importante
//! — o `Client` **não consegue mais** encenar o cenário: a tarefa leitora dele
//! drena o fluxo para um canal sem limite, então um cliente que não chama
//! `next_event` continua esvaziando a janela do QUIC. O par que ainda existe no
//! mundo e para de ler é outro: um cliente de terceiro, ou uma casca cuja
//! tarefa travou. Este teste é esse par.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use ed25519_dalek::{Signer, SigningKey};
use seele_proto::control::{ClientMessage, DisconnectReason, ServerMessage};
use seele_proto::ids::{ChannelId, ClientMessageId, PersonId};
use seele_server::persistence::channels::Channels;
use seele_server::persistence::messages::{Messages, PendingMessage, MAX_PAGE};
use seele_server::persistence::Location;
use seele_server::server::Event;
use seele_server::{frame, Daemon, ServerConfig};

const LINE: u32 = 1;

/// O tamanho da pendência: grande o bastante para não caber num pacote.
const CORPO: usize = 3_900;

/// Quantas conexões falam ao mesmo tempo.
///
/// Vinte, e não uma, porque o [`seele_server::taxa::Vigia`] limita cada conexão
/// a sessenta quadros de rajada — e com razão. Vinte pessoas falando de uma vez
/// é um servidor cheio numa hora movimentada, que `specs/04-servidor-seele.md`
/// dimensiona em cinquenta pessoas.
const FALANTES: usize = 20;

/// Quantas mensagens cada um manda, dentro da rajada que o Vigia permite.
const CADA: usize = 58;

/// Um verificador que aceita qualquer certificado.
///
/// O que está sob teste é a entrega, não o TOFU — que tem os testes dele em
/// `seele-core/src/tofu.rs`. Aqui o certificado é o que o próprio servidor acabou
/// de gerar nesta função de teste.
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
async fn server() -> Result<(SocketAddr, Arc<Daemon>)> {
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

/// Um par cru: conexão QUIC e o fluxo de controle, sem nenhuma tarefa leitora.
struct Par {
    /// Segura a conexão viva enquanto o teste corre.
    _conexao: quinn::Connection,
    envio: quinn::SendStream,
    recebe: quinn::RecvStream,
    /// Quem o servidor disse, no `Session`, que este par é.
    pessoa: PersonId,
}

/// Abre uma conexão e faz o aperto de mão à mão.
///
/// `janela` é a janela de recepção de fluxo que este par anuncia. Passar um
/// valor pequeno não cria o defeito: só encurta o tempo até o par parar de
/// aceitar bytes, que é a condição do cenário. Qualquer par pode escolher a
/// dele, e um cliente de aparelho pequeno escolhe pequena.
async fn abrir(endereco: SocketAddr, semente: u8, janela: u32) -> Result<Par> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let provider = Arc::new(rustls::crypto::ring::default_provider());

    let mut tls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AceitaQualquer(provider)))
        .with_no_client_auth();
    tls.alpn_protocols = vec![seele_proto::transport::ALPN.to_vec()];

    let mut config = quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls).expect("config do QUIC"),
    ));
    let mut transporte = quinn::TransportConfig::default();
    transporte.stream_receive_window(janela.into());
    config.transport_config(Arc::new(transporte));

    let mut ponta = quinn::Endpoint::client(SocketAddr::from(([127, 0, 0, 1], 0)))?;
    ponta.set_default_client_config(config);
    let conexao = ponta.connect(endereco, "localhost")?.await?;
    // A ponta é dona do socket; sem isto ela cai no fim desta função e leva a
    // conexão junto.
    std::mem::forget(ponta);

    let (mut envio, mut recebe) = conexao.open_bi().await?;
    let chave = SigningKey::from_bytes(&[semente; 32]);
    frame::write(
        &mut envio,
        &ClientMessage::Hello {
            faixa: seele_proto::FAIXA,
            client: "par-cru".into(),
            nickname: format!("pessoa{semente:03}"),
            public_key: chave.verifying_key().to_bytes().to_vec(),
            join_secret: None,
            material_e2ee: None,
            apis_de_mod: Vec::new(),
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
            proof: chave
                .sign(&prova_amarrada(&conexao, &nonce))
                .to_bytes()
                .to_vec(),
        },
    )
    .await?;
    let ServerMessage::Session { person, .. } = frame::read::<ServerMessage>(&mut recebe).await?
    else {
        anyhow::bail!("o servidor não mandou Session");
    };

    Ok(Par {
        _conexao: conexao,
        envio,
        recebe,
        pessoa: person,
    })
}

/// O prazo de segurança de cada espera do teste.
///
/// Só para o teste não travar se o servidor travar: nenhuma espera daqui tem
/// prazo por leitura, cada uma acaba num sinal do servidor, e numa rodada verde
/// nenhuma chega perto disto. Quem decide são as asserções.
const SEGURANCA: Duration = Duration::from_secs(120);

/// O carimbo do `Ping` que cada falante manda atrás da rajada.
const FIM_DA_RAJADA: u64 = 0x5EE1E;

/// O que o servidor fez com um falante depois da rajada.
#[derive(Debug, PartialEq, Eq)]
enum Desfecho {
    /// O `Pong` do `Ping` de depois da rajada voltou: a sessão leu, e mandou
    /// ao escritor, cada mensagem de antes dele. O fluxo de controle é lido em
    /// ordem, e a sessão só lê o quadro seguinte depois de pôr o anterior na
    /// fila de gravação.
    LeuTudo,
    /// A sessão se despediu, com este motivo. O que ela não leu ficou com
    /// quem disse — é o `EstadoDoEnvio::SemResposta` do cliente de verdade, que
    /// pode repetir com a mesma chave quando a casca chama
    /// `reenviar_mensagem`; sozinho, ele não repete.
    Desligado(DisconnectReason),
    /// O fluxo acabou sem despedida.
    Caiu(String),
}

/// Manda o `Ping` atrás da rajada e lê o fluxo de volta até o `Pong` dele ou
/// a despedida — sem prazo por leitura.
///
/// Devolve também as chaves que o servidor recusou com `MessageRejected`, para
/// o teste exigir que não haja nenhuma: neste cenário a LINE existe e todo
/// falante pode escrever nela.
async fn desfecho(falante: &mut Par) -> (Desfecho, Vec<u64>) {
    // Se a sessão já se despediu, o fluxo de ida pode estar parado: a escrita
    // falha, e a despedida está no fluxo de volta.
    let _ = frame::write(
        &mut falante.envio,
        &ClientMessage::Ping {
            timestamp: FIM_DA_RAJADA,
        },
    )
    .await;
    let mut recusadas = Vec::new();
    loop {
        match frame::read::<ServerMessage>(&mut falante.recebe).await {
            Ok(ServerMessage::Pong { timestamp }) if timestamp == FIM_DA_RAJADA => {
                return (Desfecho::LeuTudo, recusadas);
            }
            Ok(ServerMessage::Disconnecting { reason }) => {
                return (Desfecho::Desligado(reason), recusadas);
            }
            Ok(ServerMessage::MessageRejected {
                client_message_id, ..
            }) => recusadas.push(client_message_id.0),
            Ok(_) => {}
            Err(erro) => return (Desfecho::Caiu(erro.to_string()), recusadas),
        }
    }
}

/// O canal só da testemunha, criado antes da rajada para o ouvinte poder
/// assiná-lo.
async fn criar_testemunha(servidor: &Daemon) -> Result<ChannelId> {
    let guarda = servidor.server().persistence.lock().await;
    Ok(Channels::new(&guarda).create_channel("testemunha")?.id)
}

/// Põe o ouvinte nos `canais` e espera o `Pong` de um `Ping` atrás dos
/// pedidos: a sessão lê em ordem, então o `Pong` diz que ela já o pôs em cada
/// um, ou que recusou algum.
async fn entrar(ouvinte: &mut Par, canais: &[ChannelId]) -> Result<()> {
    for &channel in canais {
        frame::write(&mut ouvinte.envio, &ClientMessage::JoinChannel { channel }).await?;
    }
    frame::write(
        &mut ouvinte.envio,
        &ClientMessage::Ping {
            timestamp: FIM_DA_RAJADA,
        },
    )
    .await?;
    loop {
        match frame::read::<ServerMessage>(&mut ouvinte.recebe).await? {
            ServerMessage::Pong { timestamp } if timestamp == FIM_DA_RAJADA => return Ok(()),
            ServerMessage::Alert { reason, .. } => {
                anyhow::bail!("o ouvinte não entrou num dos canais {canais:?}: {reason:?}")
            }
            _ => {}
        }
    }
}

/// Espera o escritor gravar tudo o que está na fila dele agora.
///
/// O escritor é uma fila só, gravada em ordem, e anuncia no barramento cada
/// mensagem depois do lote dela: o anúncio de uma testemunha posta agora, no
/// canal só dela, sai depois de gravado tudo o que entrou antes. Sem prazo e
/// sem contar com o relógio de 200 ms do lote.
async fn esperar_o_escritor(
    servidor: &Daemon,
    autor: PersonId,
    testemunha: ChannelId,
) -> Result<()> {
    let mut barramento = servidor.server().events.subscribe();
    servidor
        .server()
        .post(PendingMessage {
            channel: testemunha,
            author: autor,
            author_nickname: "testemunha".into(),
            body: "o escritor chegou até aqui".into(),
            replies_to: None,
            client_message_id: Some(ClientMessageId(u64::MAX)),
        })
        .await?;
    loop {
        match barramento.recv().await {
            Ok(Event::MessagePosted(mensagem)) if mensagem.channel == testemunha => return Ok(()),
            Ok(Event::MessageRejected { channel, .. }) if channel == testemunha => {
                anyhow::bail!("o escritor recusou a testemunha")
            }
            // Ficar para trás aqui só pula anúncios mais velhos que a testemunha.
            Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                anyhow::bail!("o barramento fechou antes da testemunha")
            }
        }
    }
}

/// O que PERSISTENCE gravou de cada falante: os números das mensagens dele.
async fn gravadas_por_falante(servidor: &Daemon) -> Result<Vec<Vec<usize>>> {
    let mut por_falante = vec![Vec::new(); FALANTES];
    let mut guarda = servidor.server().persistence.lock().await;
    let mensagens = Messages::new(&mut guarda);
    let mut cursor = None;
    loop {
        let pagina = mensagens.history(ChannelId(LINE), cursor, MAX_PAGE)?;
        let Some(ultima) = pagina.last() else {
            break;
        };
        cursor = Some(ultima.id);
        for mensagem in &pagina {
            // O corpo começa com o falante e o número, dois dígitos cada.
            let indice: usize = mensagem.body.get(0..2).expect("falante").parse()?;
            let numero: usize = mensagem.body.get(2..4).expect("número").parse()?;
            por_falante
                .get_mut(indice)
                .expect("um falante deste teste")
                .push(numero);
        }
    }
    for numeros in &mut por_falante {
        numeros.sort_unstable();
    }
    Ok(por_falante)
}

/// Um par que para de ler não pode custar mensagem de quem falou.
///
/// Sem asserção sobre *quantas* mensagens ele lê depois de voltar: a resposta
/// certa é que ou ele lê todas, ou ele é desligado com um motivo. O que não
/// pode acontecer é a terceira coisa — seguir conectado, calado, com um buraco
/// no meio da conversa que ninguém dos dois lados consegue nomear.
///
/// A mesma regra vale para quem fala: tudo o que o servidor leu de um falante
/// está gravado, em ordem, e o que ele não leu é de um falante que ele
/// desligou dizendo `FellBehind`. Pedir as 1 160 gravadas sempre era pedir que
/// nenhum falante ficasse para trás, e sob carga eles ficam (lote Y da F5).
#[tokio::test(flavor = "multi_thread")]
async fn o_server_nao_perde_mensagem_calado_quando_um_par_para_de_ler() -> Result<()> {
    let (endereco, servidor) = server().await?;
    let testemunha = criar_testemunha(&servidor).await?;

    // O ouvinte anuncia uma janela pequena e para de ler no instante seguinte.
    // Ele assina a LINE e o canal da testemunha: a testemunha é o último evento
    // do barramento, e é até ela — ou até a despedida — que ele lê depois.
    let mut ouvinte = abrir(endereco, 1, 16 * 1024).await?;
    tokio::time::timeout(
        SEGURANCA,
        entrar(&mut ouvinte, &[ChannelId(LINE), testemunha]),
    )
    .await
    .expect("o ouvinte não teve o Pong: o servidor travou")?;

    // E agora a sala inteira fala de uma vez.
    let mut falantes = Vec::new();
    for numero in 0..FALANTES {
        let semente = u8::try_from(numero).expect("cabe") + 10;
        falantes.push(abrir(endereco, semente, 1024 * 1024).await?);
    }
    for (indice, falante) in falantes.iter_mut().enumerate() {
        for numero in 0..CADA {
            let corpo = format!("{indice:02}{numero:02}").repeat(CORPO / 4);
            let escrita = frame::write(
                &mut falante.envio,
                &ClientMessage::SendMessage {
                    channel: ChannelId(LINE),
                    body: corpo,
                    replies_to: None,
                    client_message_id: ClientMessageId(u64::try_from(numero).expect("cabe") + 1),
                },
            )
            .await;
            // A escrita que falha no meio da rajada é a de um falante que o
            // servidor já desligou: o resto dele fica com ele, e é o desfecho,
            // lido logo abaixo, que diz se houve despedida. Sair com o erro
            // aqui reprovaria pelo transporte um desligamento que o teste aceita
            // (M4 da revisão do lote Y).
            if escrita.is_err() {
                break;
            }
        }
    }

    let ditas = FALANTES * CADA;

    // **O que o servidor fez com cada falante**, por sinal dele e não por
    // relógio. Era um `sleep` de 3 s e o contador lido uma vez, como se o que
    // faltasse fosse o lote que ainda não tinha gravado. Medido (lote Y da
    // F5): sob carga, o que falta nunca chega. O barramento passa à frente da
    // sessão de um falante também — dois lotes seguidos anunciam mais de 1 024
    // mensagens antes de ela voltar a ler o barramento —, ela se despede com
    // `FellBehind`, e o resto do que o falante mandou fica sem ler. Em cinco
    // rodadas reprovadas, toda mensagem faltando era a cauda de um falante
    // desligado assim, e a contagem não subia mais em 15 s.
    let desfechos = tokio::time::timeout(SEGURANCA, async {
        let mut desfechos = Vec::new();
        for falante in &mut falantes {
            desfechos.push(desfecho(falante).await);
        }
        desfechos
    })
    .await
    .expect("um falante não teve nem o Pong nem a despedida: o servidor travou");

    // E o escritor gravou tudo o que as sessões puseram na fila dele.
    let autor = falantes.first().expect("há falantes").pessoa;
    tokio::time::timeout(SEGURANCA, esperar_o_escritor(&servidor, autor, testemunha))
        .await
        .expect("o escritor não chegou à testemunha: o servidor travou")?;

    // **Nada some calado.** Para cada falante: o que o servidor gravou é o
    // começo do que ele disse, sem buraco no meio, sem repetir; quem teve o
    // `Pong` teve tudo gravado; e quem ficou com mensagem sem gravar foi
    // desligado dizendo por quê.
    let gravadas_de = gravadas_por_falante(&servidor).await?;
    for (indice, ((desfecho, recusadas), numeros)) in desfechos.iter().zip(&gravadas_de).enumerate()
    {
        // **Nenhuma recusa.** A LINE existe e todo falante pode escrever nela:
        // uma recusa aqui é o braço `SendMessage` errando, e aceitá-la deixaria
        // um servidor que recusa tudo passar com zero gravadas, sem a pendência
        // nº 1 nem acontecer (achado da revisão do lote Y).
        assert!(
            recusadas.is_empty(),
            "o servidor recusou {} mensagens do falante {indice:02} (chaves {recusadas:?}) \
             num canal que existe e em que ele pode escrever",
            recusadas.len()
        );
        let mut sem_repetir = numeros.clone();
        sem_repetir.dedup();
        assert_eq!(
            &sem_repetir, numeros,
            "o falante {indice:02} teve mensagem gravada duas vezes: {numeros:?}"
        );
        let ate = numeros.last().map_or(0, |ultimo| ultimo + 1);
        let buracos: Vec<usize> = (0..ate)
            .filter(|numero| !numeros.contains(numero))
            .collect();
        assert!(
            buracos.is_empty(),
            "o servidor pulou mensagens do falante {indice:02} e gravou as de depois: \
             faltam {buracos:?} — perder do meio não tem despedida que explique"
        );
        let faltam: Vec<usize> = (0..CADA)
            .filter(|numero| !numeros.contains(numero))
            .collect();
        match desfecho {
            Desfecho::LeuTudo => assert!(
                faltam.is_empty(),
                "o servidor perdeu mensagem antes de gravar, e aí reconectar não repõe nada: \
                 a sessão do falante {indice:02} leu todas e faltam {faltam:?}"
            ),
            Desfecho::Desligado(DisconnectReason::FellBehind) => {}
            outro => assert!(
                faltam.is_empty(),
                "o falante {indice:02} ficou sem {faltam:?} gravadas e o servidor não disse que \
                 ficou para trás: {outro:?}"
            ),
        }
    }

    // A contagem de PERSISTENCE é a soma do que se leu acima.
    let gravadas = servidor.quantas_mensagens(ChannelId(LINE)).await?;
    let somadas: usize = gravadas_de.iter().map(Vec::len).sum();
    assert_eq!(
        gravadas, somadas as u64,
        "a contagem do canal e as mensagens lidas dele discordam"
    );

    // O ouvinte volta a ler, até a testemunha ou a despedida. Sem prazo por
    // leitura: um silêncio não distingue «acabou» de «esta linha ficou fora de
    // cena». E sem esperar «tudo»: a sessão escreve na ordem do barramento, e a
    // testemunha é o evento mais novo dele, então ela chegar sem a despedida
    // quer dizer que o que faltou da LINE não vem mais. Era assim que a
    // despedida calada só reprovava pelo prazo de segurança, em 120 s e sem
    // número (achado da revisão do lote Y).
    let mut chegaram = 0_usize;
    let mut desligado = None;
    let mut fim = None;
    tokio::time::timeout(SEGURANCA, async {
        loop {
            match frame::read::<ServerMessage>(&mut ouvinte.recebe).await {
                Ok(ServerMessage::MessageReceived { channel, .. }) if channel == testemunha => {
                    fim = Some("a testemunha".to_owned());
                    break;
                }
                Ok(ServerMessage::MessageReceived { channel, body, .. })
                    if channel == ChannelId(LINE) && body.len() >= CORPO - 4 =>
                {
                    chegaram += 1;
                }
                Ok(ServerMessage::Disconnecting { reason }) => {
                    desligado = Some(reason);
                    fim = Some("a despedida".to_owned());
                    break;
                }
                Ok(_) => {}
                Err(erro) => {
                    fim = Some(format!("o fluxo terminou: {erro}"));
                    break;
                }
            }
        }
    })
    .await
    .expect("o ouvinte não teve nem a testemunha nem a despedida: o servidor travou");

    let perdidos = servidor.server().atrasos.eventos();
    let desligados = desfechos
        .iter()
        .filter(|(desfecho, _)| matches!(desfecho, Desfecho::Desligado(_)))
        .count();
    println!(
        "ditas {ditas}, gravadas {gravadas}, chegaram {chegaram}, perdidos no barramento \
         {perdidos}, ouvinte desligado {desligado:?}, falantes desligados {desligados}, \
         a leitura do ouvinte acabou em {fim:?}"
    );

    // Uma das duas, e nunca a terceira: ou chegou tudo, ou o servidor disse, com
    // esse nome, que ficou faltando. O que ninguém pode aceitar é o par voltar
    // a ler, seguir conectado, e faltar conversa no meio sem uma palavra de
    // nenhum dos dois lados.
    assert!(
        chegaram == somadas || desligado == Some(DisconnectReason::FellBehind),
        "pendência nº 1: {chegaram} de {somadas} chegaram e o servidor não disse nada — \
         {perdidos} eventos morreram no barramento (a leitura acabou em {fim:?})"
    );

    // E o que houve ficou contado. Desligamento sem número por trás seria a
    // mesma cegueira com outra roupa: ninguém sabe se aconteceu uma vez ou mil.
    if desligado.is_some() || desligados > 0 {
        assert!(
            perdidos > 0,
            "a sessão foi encerrada por ter ficado para trás e o contador marcou zero"
        );
        assert!(servidor.server().atrasos.sessoes() > 0);
    }

    servidor.shutdown();
    Ok(())
}

/// A prova de identidade do S2: o rótulo, a impressão do certificado que o TLS
/// desta conexão aceitou e o nonce, montados por `seele_proto::assinatura`.
/// Desde o Servidor 1.0, o nonce cru não entra mais.
fn prova_amarrada(conexao: &quinn::Connection, nonce: &[u8]) -> Vec<u8> {
    let cadeia = conexao
        .peer_identity()
        .and_then(|identidade| {
            identidade
                .downcast::<Vec<rustls::pki_types::CertificateDer<'static>>>()
                .ok()
        })
        .expect("o TLS sempre entrega a cadeia do servidor");
    let impressao = seele_proto::transport::certificate_digest(
        cadeia
            .first()
            .expect("a cadeia tem um certificado")
            .as_ref(),
    );
    let nonce: [u8; 32] = nonce.try_into().expect("o desafio tem 32 bytes");
    seele_proto::assinatura::prova_de_cliente(&impressao, &nonce)
}
