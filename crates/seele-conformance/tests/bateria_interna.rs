//! A bateria interna, ponta a ponta.
//!
//! `specs/07-estetica.md` chama isto de "o melhor casamento entre tema e
//! engenharia no projeto" e manda proteger de simplificações. Estava protegido
//! de simplificações e desprotegido de outra coisa: as peças existiam, cada uma
//! testada, e **nada ligava uma na outra**. `Battery::new` não era chamado fora
//! do próprio módulo; ao cair, o cliente ia direto para "ENLACE PERDIDO".
//!
//! Testes de unidade não pegam isso — cada unidade passa. Este teste derruba o
//! servidor de verdade, confere que a sessão entra na bateria em vez de acabar,
//! sobe o servidor de novo na mesma porta, e confere que ela volta sozinha.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use seele_core::enlace::{Aviso, Destino, Enlace, Motivo};
use seele_core::{Link, MemoryPinStore};
use seele_proto::control::{DisconnectReason, ServerMessage};
use seele_proto::ids::{ChannelId, ClientMessageId, VoiceRoomId};
use seele_server::persistence::Location;
use seele_server::server::Event;
use seele_server::{Daemon, ServerConfig};

const VOICE_ROOM: u32 = 1;
const LINE: u32 = 1;

/// Sobe um servidor numa porta escolhida — a mesma na segunda vez.
async fn server(porta: u16, banco: Location) -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], porta)),
        database: banco,
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

fn destino(endereco: SocketAddr) -> Destino {
    Destino {
        servidor: endereco,
        nome_tls: "localhost".into(),
        chave_do_pin: endereco.to_string(),
        apelido: "marcela".into(),
        segredo: None,
        impressao_esperada: None,
    }
}

/// Espera um aviso que interesse, ou desiste.
async fn esperar<F>(enlace: &mut Enlace, prazo: Duration, mut aceita: F) -> Option<Aviso>
where
    F: FnMut(&Aviso) -> bool,
{
    let fim = tokio::time::Instant::now() + prazo;
    while tokio::time::Instant::now() < fim {
        match tokio::time::timeout(Duration::from_millis(300), enlace.proximo()).await {
            Ok(aviso) if aceita(&aviso) => return Some(aviso),
            Ok(_) => {}
            Err(_) => {}
        }
    }
    None
}

#[tokio::test(flavor = "multi_thread")]
async fn o_server_cai_e_a_sessao_entra_na_bateria_em_vez_de_acabar() -> Result<()> {
    // O banco em arquivo, e não em memória: o servidor que sobe depois tem que ser
    // o mesmo servidor, com o mesmo certificado. Um certificado novo pareceria uma
    // troca de chave ao cliente, que é o alerta do ADR 0003 e não uma
    // reconexão.
    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");

    let (endereco, servidor) = server(0, Location::File(banco.clone())).await?;
    let porta = endereco.port();

    let mut enlace = Enlace::conectar(
        destino(endereco),
        ed25519_dalek::SigningKey::from_bytes(&[42; 32]),
        Arc::new(MemoryPinStore::new()),
    )
    .await?;
    enlace.entrar_na_voice_room(VoiceRoomId(VOICE_ROOM), None).await?;
    enlace.abrir_linha(ChannelId(LINE)).await?;
    assert_eq!(enlace.estado(), Link::Online);

    // ---- o servidor cai
    servidor.shutdown();
    servidor.wait_idle().await;
    drop(servidor);
    tokio::time::sleep(Duration::from_millis(300)).await;

    let aviso = esperar(&mut enlace, Duration::from_secs(20), |aviso| {
        matches!(
            aviso,
            Aviso::Estado {
                estado: Link::InternalBattery { .. },
                ..
            }
        )
    })
    .await;

    let Some(Aviso::Estado { restante, .. }) = aviso else {
        panic!("a sessão não entrou na bateria interna — foi isto que ficou anos sem existir");
    };
    assert!(
        matches!(enlace.estado(), Link::InternalBattery { .. }),
        "o enlace não reporta a bateria"
    );
    let restante = restante.expect("sem contagem regressiva");
    assert!(
        restante > Duration::from_secs(4 * 60),
        "a contagem não começou perto de cinco minutos: {restante:?}"
    );

    // ---- o servidor volta, no mesmo endereço e com o mesmo banco
    let (_, de_novo) = server(porta, Location::File(banco)).await?;

    let voltou = esperar(&mut enlace, Duration::from_secs(30), |aviso| {
        matches!(aviso, Aviso::Reconectado { .. })
    })
    .await;
    assert!(
        voltou.is_some(),
        "a sessão não voltou sozinha depois que o servidor subiu"
    );
    assert_eq!(enlace.estado(), Link::Online);

    // E continua servindo: reconectar sem poder falar seria voltar para nada.
    enlace
        .dizer(ChannelId(LINE), "voltei".to_owned(), ClientMessageId(1))
        .await?;

    de_novo.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn o_que_a_pessoa_escolheu_volta_com_ela() -> Result<()> {
    // Reconectar no lugar errado é quase tão ruim quanto não reconectar: quem
    // estava numa sala de voz conversando volta calado noutro canto sem entender por
    // quê.
    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");
    let (endereco, servidor) = server(0, Location::File(banco.clone())).await?;
    let porta = endereco.port();

    let mut enlace = Enlace::conectar(
        destino(endereco),
        ed25519_dalek::SigningKey::from_bytes(&[43; 32]),
        Arc::new(MemoryPinStore::new()),
    )
    .await?;
    enlace.entrar_na_voice_room(VoiceRoomId(VOICE_ROOM), None).await?;
    enlace.abrir_linha(ChannelId(LINE)).await?;
    enlace.muted(true).await?;

    servidor.shutdown();
    servidor.wait_idle().await;
    drop(servidor);
    tokio::time::sleep(Duration::from_millis(300)).await;

    let (_, de_novo) = server(porta, Location::File(banco)).await?;

    let voltou = esperar(&mut enlace, Duration::from_secs(30), |aviso| {
        matches!(aviso, Aviso::Reconectado { .. })
    })
    .await;
    assert!(voltou.is_some(), "não reconectou");

    // O `ssrc` é novo — é por conexão (falha G1) — e é justamente por isso que
    // a casca precisa reabrir a voz com o canal que veio no aviso.
    assert_ne!(
        enlace.sessao().ssrc.0,
        0,
        "a sessão nova não trouxe um ssrc"
    );

    // Falar de novo prova que a sala de voz e a Linha foram refeitos: sem `join_channel`
    // o servidor aceita a mensagem e não a devolve para ninguém.
    enlace
        .dizer(ChannelId(LINE), "de volta".to_owned(), ClientMessageId(1))
        .await?;

    let ouviu = esperar(&mut enlace, Duration::from_secs(10), |aviso| {
        matches!(
            aviso,
            Aviso::Mensagem(mensagem)
                if matches!(&**mensagem, seele_proto::control::ServerMessage::MessageReceived { body, .. } if body == "de volta")
        )
    })
    .await;
    assert!(
        ouviu.is_some(),
        "a Linha não foi reaberta na reconexão: a mensagem não voltou"
    );

    de_novo.shutdown();
    Ok(())
}

/// Uma despedida **recuperável** chega pelo fio, e a sessão volta em vez de acabar.
///
/// # A lacuna que este teste fecha
///
/// `Enlace::a_sessao_acabou_aqui` decide, para cada `Disconnecting` do
/// protocolo, entre **acabar** com a sessão e **reconectar**. Metade dessa
/// fronteira tinha prova de **comportamento** e a outra metade não: expulsar e
/// banir são cobrados de ponta a ponta por `moderacao.rs`, mas nada exercia o
/// lado de reconectar com um servidor de verdade escrevendo a despedida no fio.
/// Uma despedida recuperável tratada como fim de sessão tira do ar exatamente
/// quem a bateria interna existe para segurar.
///
/// A lacuna é **comportamental**, e é só isso. O relatório de 10/09 registrou a
/// afirmação mais larga de que, com aquela função devolvendo `true` para tudo,
/// o workspace inteiro continuava verde — e a medida de 10/09 05:51 desmente
/// essa parte: sob essa mesma mutação, o guarda unitário
/// `toda_despedida_do_protocolo_escolhe_um_lado`, em `seele-core`, **falha**.
/// Ele não confere a tabela contra si mesma: escreve a decisão esperada
/// variante a variante, à parte da função, e compara. O que faltava não era um
/// guarda que percebesse a mutação — era um que a percebesse **por fora**, com
/// o servidor real no meio. É o que este teste faz: o `Enlace` público — o
/// mesmo objeto que a casca segura — tem de sobreviver à despedida que chega
/// pelo protocolo.
///
/// # Como a despedida é injetada
///
/// Pelo **mesmo caminho de servidor que a expulsão usa**: `Event::SessionEnded`
/// no barramento, que é como uma sessão alcança outra (ver a nota em
/// `server::Event`). O braço que o atende escreve `Disconnecting { reason }` e
/// se despede da conexão; só o motivo muda. É o que torna esta a medida da
/// fronteira e não de dois caminhos diferentes: a única diferença entre este
/// teste e `expulsar_acaba_com_a_sessao_e_deixa_voltar` é qual
/// [`DisconnectReason`] viajou.
///
/// [`DisconnectReason::FellBehind`] porque é a despedida recuperável que este
/// servidor realmente emite sozinho — quando o barramento passa à frente de uma
/// sessão — e o doc dela diz com todas as letras que reconectar e buscar o
/// histórico é o conserto, «que é o que a bateria interna faz sozinha».
#[tokio::test(flavor = "multi_thread")]
async fn uma_despedida_recuperavel_reconecta_em_vez_de_acabar_com_a_sessao() -> Result<()> {
    let (endereco, servidor) = server(0, Location::Memory).await?;

    let mut enlace = Enlace::conectar(
        destino(endereco),
        ed25519_dalek::SigningKey::from_bytes(&[46; 32]),
        Arc::new(MemoryPinStore::new()),
    )
    .await?;
    enlace.entrar_na_voice_room(VoiceRoomId(VOICE_ROOM)).await?;
    enlace.abrir_linha(ChannelId(LINE)).await?;
    assert_eq!(enlace.estado(), Link::Online);

    // ---- a barreira: a sessão está de pé **do lado de lá**, e não só aqui
    //
    // `abrir_linha` e `entrar_na_voice_room` não esperam resposta: põem o pedido na
    // fila do motor e voltam. O `assert_eq!` acima também é local — lê o estado
    // que este objeto guarda. Nenhum dos três prova que o servidor já processou
    // coisa alguma desta conexão, e é isso que a injeção logo abaixo precisa:
    // `Event::SessionEnded` vai para o barramento, que só entrega a quem já
    // assinou, e a sessão do servidor assina depois de responder ao handshake
    // (`seele-server/src/session.rs:1243`).
    //
    // Uma ida e volta pela Linha fecha essa classe inteira **por construção**:
    // a mensagem só volta se o laço da sessão a leu, e o laço começa depois da
    // assinatura. Custa um par de milissegundos e transforma três suposições em
    // uma medida. Ela também é o «antes» do passo Quatro: sem isto, uma Linha
    // que nunca chegou a abrir se pareceria, lá embaixo, com uma Linha que a
    // reconexão não reabriu.
    enlace
        .dizer(ChannelId(LINE), "estou aqui".to_owned(), ClientMessageId(0))
        .await?;
    let pronta = esperar(&mut enlace, Duration::from_secs(10), |aviso| {
        matches!(
            aviso,
            Aviso::Mensagem(mensagem)
                if matches!(
                    &**mensagem,
                    ServerMessage::MessageReceived { body, .. } if body == "estou aqui"
                )
        )
    })
    .await;
    assert!(
        pronta.is_some(),
        "a sessão não ficou de pé do lado do servidor: a Linha não devolveu o que foi dito nela, \
         e uma despedida injetada agora mediria o preparo em vez da fronteira"
    );

    let quem = enlace.sessao().person;
    let ssrc_de_antes = enlace.sessao().ssrc;

    // ---- a despedida recuperável, escrita no fio pelo servidor
    servidor
        .server()
        .events
        .send(Event::SessionEnded {
            person: quem,
            reason: DisconnectReason::FellBehind,
        })
        .expect("o barramento do servidor está de pé");

    // Um: ela **chegou pelo protocolo**. Sem esta asserção o teste passaria com
    // uma queda de transporte qualquer, que é outra coisa e já tem teste acima.
    let recebida = esperar(&mut enlace, Duration::from_secs(10), |aviso| {
        matches!(
            aviso,
            Aviso::Mensagem(mensagem)
                if matches!(
                    &**mensagem,
                    ServerMessage::Disconnecting {
                        reason: DisconnectReason::FellBehind
                    }
                )
        )
    })
    .await;
    // Quando ela **não** chega, duas coisas muito diferentes cabem no mesmo
    // vermelho, e um teste que não as separa manda quem lê adivinhar — que é o
    // defeito mais caro desta casa. Então ele mede o que o `Enlace` via no
    // instante da desistência e diz de qual das duas se trata:
    //
    // - **ainda `Online` com o mesmo `ssrc`**: o servidor não agiu sobre o
    //   evento. A despedida não foi escrita no fio, e isso é regressão do lado
    //   de lá — o braço de `Event::SessionEnded` deixou de responder.
    // - **fora de `Online`, ou com `ssrc` novo**: a conexão caiu e a sessão
    //   seguiu sem o motivo. O quadro se perdeu junto com a conexão que o
    //   carregava — o `Disconnecting` é escrito e a conexão é fechada logo
    //   atrás (`despedir`, em `seele-server/src/session.rs`) —, e o que se
    //   perdeu foi a **explicação**, não a decisão.
    //
    // Este teste ficou vermelho duas vezes em 10/09, as duas aqui, e **nenhuma
    // delas foi diagnosticada**: a distinção acima só existe a partir de agora.
    // Quatro hipóteses foram medidas e refutadas — relatório de 496ba8c5, §6.2.
    // Quem apanhar o próximo vermelho tem, nesta linha, a metade que faltava.
    if recebida.is_none() {
        let estado = enlace.estado();
        let ssrc_agora = enlace.sessao().ssrc;
        let leitura = if estado == Link::Online && ssrc_agora == ssrc_de_antes {
            "o Enlace seguiu Online na mesma conexão: o servidor não escreveu a despedida no fio"
        } else {
            "a conexão de antes não está mais de pé: a despedida se perdeu com ela, \
             e o que faltou foi o motivo — não a reconexão"
        };
        panic!(
            "a despedida recuperável não chegou ao Enlace em 10 s; não há fronteira a medir \
             ({estado:?}, ssrc {ssrc_de_antes:?} → {ssrc_agora:?}): {leitura}"
        );
    }

    // Dois: a sessão **não acaba**. `Encerrado` aqui é a regressão inteira —
    // é o que uma `a_sessao_acabou_aqui` alargada produz.
    let desfecho = esperar(&mut enlace, Duration::from_secs(30), |aviso| {
        matches!(aviso, Aviso::Reconectado { .. } | Aviso::Encerrado(_))
    })
    .await;
    match desfecho {
        Some(Aviso::Reconectado { .. }) => {}
        Some(Aviso::Encerrado(motivo)) => panic!(
            "uma despedida recuperável acabou com a sessão ({motivo:?}): quem a bateria \
             interna existe para segurar foi posto para fora"
        ),
        outro => panic!("a sessão não reconectou nem acabou: {outro:?}"),
    }

    // Três: a reconexão é **efetiva**, e não um aviso sobre nada. O `ssrc` é por
    // conexão (falha G1), então um número novo é a conexão nova.
    assert_eq!(enlace.estado(), Link::Online);
    assert_ne!(
        enlace.sessao().ssrc,
        ssrc_de_antes,
        "o aviso de reconexão saiu sem conexão nova por baixo"
    );

    // Quatro: e ela **serve**. Falar e ouvir de volta é o que prova que a Linha
    // foi reaberta do outro lado — sem `join_channel` o servidor aceita a
    // mensagem e não a devolve para ninguém. Voltar sem poder falar é voltar
    // para nada.
    enlace
        .dizer(
            ChannelId(LINE),
            "continuo aqui".to_owned(),
            ClientMessageId(1),
        )
        .await?;
    let ouviu = esperar(&mut enlace, Duration::from_secs(10), |aviso| {
        matches!(
            aviso,
            Aviso::Mensagem(mensagem)
                if matches!(
                    &**mensagem,
                    ServerMessage::MessageReceived { body, .. } if body == "continuo aqui"
                )
        )
    })
    .await;
    assert!(
        ouviu.is_some(),
        "a sessão voltou muda: a Linha não foi reaberta depois da despedida recuperável"
    );

    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn sair_encerra_sem_esperar_a_bateria() -> Result<()> {
    let (endereco, servidor) = server(0, Location::Memory).await?;
    let mut enlace = Enlace::conectar(
        destino(endereco),
        ed25519_dalek::SigningKey::from_bytes(&[44; 32]),
        Arc::new(MemoryPinStore::new()),
    )
    .await?;

    enlace.sair().await;

    let fim = esperar(&mut enlace, Duration::from_secs(5), |aviso| {
        matches!(aviso, Aviso::Encerrado(_))
    })
    .await;
    assert!(
        matches!(fim, Some(Aviso::Encerrado(Motivo::Pedido))),
        "sair deveria encerrar na hora, e não abrir uma contagem de cinco minutos"
    );

    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn as_tentativas_aparecem_enquanto_a_bateria_corre() -> Result<()> {
    // `specs/07-estetica.md` pede "tentativas de reconexão listadas". Um
    // contador que fica em zero enquanto o programa tenta é pior que não ter:
    // parece que ninguém está fazendo nada.
    let (endereco, servidor) = server(0, Location::Memory).await?;
    let mut enlace = Enlace::conectar(
        destino(endereco),
        ed25519_dalek::SigningKey::from_bytes(&[45; 32]),
        Arc::new(MemoryPinStore::new()),
    )
    .await?;

    servidor.shutdown();
    servidor.wait_idle().await;
    drop(servidor);

    // O prazo cobre várias tentativas: cada uma esbarra no tempo de handshake
    // antes de desistir, e o backoff cresce entre elas.
    let subiu = esperar(&mut enlace, Duration::from_secs(45), |aviso| {
        matches!(
            aviso,
            Aviso::Estado {
                estado: Link::InternalBattery { attempts },
                ..
            } if *attempts > 0
        )
    })
    .await;

    assert!(
        subiu.is_some(),
        "o contador de tentativas ficou em zero durante toda a bateria"
    );
    Ok(())
}
