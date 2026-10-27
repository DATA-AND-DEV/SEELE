//! **Quem parou de ler e ficou para trás sai com o nome disso** (gate 10d,
//! revisão Astra do cliente lento, item 1).
//!
//! Com o crédito `0x04` do enlace, quem para de ler o `ctl` segura a escrita do
//! servidor. A sessão fica presa num `write` e não volta ao `events.recv()`.
//! Enquanto isso o barramento passa à frente dela, e os eventos descartados
//! eram também mensagens já gravadas. Antes da vigia
//! (`session::escrever_vigiando`), ninguém via o atraso enquanto a escrita
//! esperava: a sessão ficava presa pelo tempo que o par quisesse, sem palavra e
//! sem número. E quando o `Lagged` aparecia, o `Disconnecting{FellBehind}` não
//! tinha crédito para sair, e a conexão fechava com o código 0, igual a um
//! fechamento comum.
//!
//! Aqui o atraso é FORÇADO:
//! 1. o ouvinte entra na LINE e para de ler, sem largar o `Recebe`, porque largá-lo
//!    fecharia a conexão;
//! 2. o teste grava na LINE mais que o crédito de 1 MiB e espera os bytes pararem
//!    de chegar ao ouvinte: a sessão está presa na escrita, e ainda sem atraso;
//! 3. só então o teste grava, num canal que o ouvinte não assina, mais eventos que o
//!    anel do barramento.
//!
//! O que se exige é a conexão do ouvinte fechar, em prazo finito, com
//! `CODIGO_FICOU_PARA_TRAS`, com zero cortes por fila e o atraso contado. O
//! controle é a mesma escrita presa com o barramento dentro do anel: nada fecha,
//! e quem volta a ler recebe tudo.

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
use seele_enlace::Motivo;
use seele_proto::control::{ClientMessage, ServerMessage};
use seele_proto::ids::{ChannelId, ClientMessageId, PersonId, Ssrc, VoiceRoomId};
use seele_proto::transport::CODIGO_FICOU_PARA_TRAS;
use seele_server::persistence::channels::Channels;
use seele_server::persistence::messages::PendingMessage;
use seele_server::persistence::Location;
use seele_server::server::ANEL_DO_BARRAMENTO;
use seele_server::{frame, Daemon, ServerConfig};

mod comum;

const LINE: u32 = 1;

/// Um corpo grande o bastante para o crédito acabar em poucas centenas.
const CORPO: usize = 3_900;

/// Mais que o crédito de 1 MiB do `ctl`, mais os 64 KiB de saída, com folga.
const GRANDES: usize = 400;

/// O prazo para a conexão do ouvinte fechar. A vigia olha a cada 250 ms; o
/// resto é o escritor gravar e anunciar.
const PRAZO: Duration = Duration::from_secs(60);

struct Par {
    conexao: seele_enlace::Conexao,
    envio: seele_enlace::Envio,
    recebe: seele_enlace::Recebe,
    pessoa: PersonId,
    /// O `ssrc` que o `Session` deu: o mesmo na volta quer dizer assento resgatado.
    ssrc: Ssrc,
    /// A primeira sala de voz do servidor.
    sala: VoiceRoomId,
}

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

async fn abrir(endereco: SocketAddr, semente: u8) -> Result<Par> {
    let comum::Ligado {
        ponta,
        conexao,
        mut envio,
        mut recebe,
    } = comum::ligar(endereco).await?;
    // A ponta é dona do socket; sem isto ela cai no fim desta função e leva a
    // conexão junto.
    std::mem::forget(ponta);
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
            apis_de_mod: seele_proto::mods::APIS_ACEITAS.to_vec(),
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
                .sign(&comum::prova_amarrada(&conexao, &nonce))
                .to_bytes()
                .to_vec(),
        },
    )
    .await?;
    let ServerMessage::Session {
        person,
        ssrc,
        voice_rooms,
        ..
    } = frame::read::<ServerMessage>(&mut recebe).await?
    else {
        anyhow::bail!("o servidor não mandou Session");
    };
    Ok(Par {
        conexao,
        envio,
        recebe,
        pessoa: person,
        ssrc,
        sala: voice_rooms.first().map(|info| info.id).expect("uma sala"),
    })
}

/// Senta o par na sala de voz e espera a lotação do servidor mostrar.
async fn sentar(par: &mut Par, servidor: &Daemon) -> Result<()> {
    frame::write(
        &mut par.envio,
        &ClientMessage::EnterVoiceRoom {
            voice_room: par.sala,
            password: None,
        },
    )
    .await?;
    let pessoa = par.pessoa;
    let sala = par.sala;
    anyhow::ensure!(
        esperar(Duration::from_secs(10), || async {
            sentado(servidor, sala, pessoa).await
        })
        .await,
        "o par não foi sentado na sala"
    );
    Ok(())
}

async fn sentado(servidor: &Daemon, sala: VoiceRoomId, pessoa: PersonId) -> bool {
    servidor
        .server()
        .occupancy
        .lock()
        .await
        .in_voice_room(sala)
        .iter()
        .any(|quem| quem.person == pessoa)
}

async fn esperar<F, Fut>(prazo: Duration, mut condicao: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let fim = tokio::time::Instant::now() + prazo;
    while tokio::time::Instant::now() < fim {
        if condicao().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    condicao().await
}

/// **A queda é recuperável**: quem caiu volta pela mesma chave e é re-sentado na
/// mesma sala, com o mesmo `ssrc` (o assento da graça foi guardado e
/// resgatado). A conexão velha tem de ter sido desmontada antes, porque é no
/// desmonte que a reserva é gravada.
async fn volta_sentado(
    endereco: SocketAddr,
    servidor: &Daemon,
    semente: u8,
    antes: &Par,
) -> Result<Par> {
    let (sala, pessoa) = (antes.sala, antes.pessoa);
    anyhow::ensure!(
        esperar(Duration::from_secs(10), || async {
            !sentado(servidor, sala, pessoa).await
        })
        .await,
        "a sessão velha não foi desmontada no prazo"
    );
    let volta = abrir(endereco, semente).await?;
    assert_eq!(volta.pessoa, pessoa, "a volta não é a mesma conta");
    assert_eq!(
        volta.ssrc, antes.ssrc,
        "a volta ganhou outro ssrc: o assento da graça não foi guardado, e a queda deixou de \
         ser recuperável"
    );
    assert!(
        esperar(Duration::from_secs(10), || async {
            sentado(servidor, sala, pessoa).await
        })
        .await,
        "a volta não foi re-sentada na sala em que estava"
    );
    Ok(volta)
}

/// Entra na LINE e espera o `Pong` de trás do pedido.
async fn entrar_na_line(par: &mut Par) -> Result<()> {
    frame::write(
        &mut par.envio,
        &ClientMessage::JoinChannel {
            channel: ChannelId(LINE),
        },
    )
    .await?;
    frame::write(&mut par.envio, &ClientMessage::Ping { timestamp: 7 }).await?;
    loop {
        if let ServerMessage::Pong { timestamp: 7 } =
            frame::read::<ServerMessage>(&mut par.recebe).await?
        {
            return Ok(());
        }
    }
}

/// Grava `grandes` mensagens grandes na LINE e `pequenas` num canal à parte.
async fn encher(
    servidor: &Daemon,
    autor: PersonId,
    outro: ChannelId,
    grandes: usize,
    pequenas: usize,
) -> Result<()> {
    for numero in 0..grandes {
        servidor
            .server()
            .post(PendingMessage {
                channel: ChannelId(LINE),
                author: autor,
                author_nickname: "quem fala".into(),
                body: format!("{numero:04}").repeat(CORPO / 4),
                replies_to: None,
                client_message_id: Some(ClientMessageId(numero as u64 + 1)),
            })
            .await?;
    }
    for numero in 0..pequenas {
        servidor
            .server()
            .post(PendingMessage {
                channel: outro,
                author: autor,
                author_nickname: "quem fala".into(),
                body: format!("p{numero}"),
                replies_to: None,
                client_message_id: Some(ClientMessageId(1_000_000 + numero as u64)),
            })
            .await?;
    }
    Ok(())
}

/// Espera os bytes que chegam ao ouvinte pararem: o sinal de que o crédito
/// acabou e a escrita da sessão ficou presa. «Parar» é crescer menos de 4 KiB
/// num segundo, porque o STUN e o SCTP continuam a passar. Devolve `false`
/// se em 20 s não parar.
async fn esperar_a_escrita_presa(conexao: &seele_enlace::Conexao) -> bool {
    let mut antes = conexao.leitura().bytes_recebidos;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let agora = conexao.leitura().bytes_recebidos;
        if agora.saturating_sub(antes) < 4 * 1024 && agora > 1024 * 1024 {
            return true;
        }
        antes = agora;
    }
    false
}

#[tokio::test(flavor = "multi_thread")]
async fn quem_parou_de_ler_e_ficou_para_tras_fecha_com_o_codigo_tipado() -> Result<()> {
    let (endereco, servidor) = servidor().await?;
    let outro = {
        let guarda = servidor.server().persistence.lock().await;
        Channels::new(&guarda).create_channel("a-parte")?.id
    };
    let mut ouvinte = abrir(endereco, 1).await?;
    entrar_na_line(&mut ouvinte).await?;
    sentar(&mut ouvinte, &servidor).await?;
    let falante = abrir(endereco, 2).await?;

    // O ouvinte para de ler aqui, e o `Recebe` continua vivo.
    //
    // **Primeiro a escrita presa, e só depois o barramento atrás.** Numa fase
    // só, a sessão podia ficar para trás ANTES de o crédito acabar: no debug,
    // o escritor anuncia mais depressa do que ela consome. Aí o atraso era
    // visto no `recv`, e o teste passava sem a vigia (medido na reversão em
    // mutacao-E-apos-credito). Separadas, a segunda fase só pode ser vista
    // pela vigia.
    encher(&servidor, falante.pessoa, outro, GRANDES, 0).await?;
    let presa = esperar_a_escrita_presa(&ouvinte.conexao).await;
    assert!(
        presa,
        "o ouvinte não parou de receber bytes: a escrita da sessão não ficou presa no crédito"
    );
    assert_eq!(
        servidor.server().atrasos.sessoes(),
        0,
        "a sessão ficou para trás antes de o barramento passar à frente: o cenário não é o da escrita presa"
    );
    assert_eq!(
        ouvinte.conexao.acabou(),
        None,
        "a escrita presa, sozinha, fechou a conexão"
    );
    encher(&servidor, falante.pessoa, outro, 0, ANEL_DO_BARRAMENTO * 2).await?;

    let motivo = tokio::time::timeout(PRAZO, ouvinte.conexao.closed())
        .await
        .expect(
            "a conexão do ouvinte não fechou: a sessão ficou presa na escrita sem ver o atraso",
        );
    assert_eq!(
        motivo,
        Motivo::Fechada {
            codigo: CODIGO_FICOU_PARA_TRAS
        },
        "a conexão do ouvinte fechou sem dizer que ele ficou para trás"
    );
    assert_eq!(
        ouvinte.conexao.contadores().cortes_por_fila,
        0,
        "o ouvinte cortou por fila: o servidor escreveu sem crédito"
    );
    assert!(
        servidor.server().atrasos.eventos() > 0 && servidor.server().atrasos.sessoes() > 0,
        "a sessão foi encerrada por ter ficado para trás, e o contador marcou zero"
    );
    // A outra conexão segue de pé: o fechamento é de quem ficou para trás.
    assert_eq!(falante.conexao.acabou(), None, "o falante caiu junto");
    // E a queda é recuperável: a vigia sai pelo `?`, e o assento da graça tem de
    // ter sido guardado assim mesmo.
    let _volta = volta_sentado(endereco, &servidor, 1, &ouvinte).await?;
    drop(ouvinte.recebe);
    servidor.shutdown();
    Ok(())
}

/// O controle: **contrapressão sem perda não fecha**.
///
/// É o mesmo ouvinte parado, com a escrita da sessão presa sem crédito, mas
/// com o barramento dentro do anel: as mesmas grandes, e só meio anel de
/// pequenas. Ele fica parado por muitas voltas da vigia, depois volta a ler e
/// recebe tudo, com a conexão de pé e nenhum atraso contado. Uma vigia que
/// fechasse por tempo de escrita parada, e não por evento perdido, reprovaria
/// aqui.
#[tokio::test(flavor = "multi_thread")]
async fn a_contrapressao_sem_perda_nao_fecha() -> Result<()> {
    let (endereco, servidor) = servidor().await?;
    let outro = {
        let guarda = servidor.server().persistence.lock().await;
        Channels::new(&guarda).create_channel("a-parte")?.id
    };
    let mut ouvinte = abrir(endereco, 1).await?;
    entrar_na_line(&mut ouvinte).await?;
    let falante = abrir(endereco, 2).await?;

    // Parado, com a escrita presa: as grandes passam do crédito.
    encher(
        &servidor,
        falante.pessoa,
        outro,
        GRANDES,
        ANEL_DO_BARRAMENTO / 2,
    )
    .await?;
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(
        ouvinte.conexao.acabou(),
        None,
        "a sessão fechou um ouvinte só parado, sem evento perdido"
    );

    let mut recebe = ouvinte.recebe;
    let lendo = tokio::spawn(async move {
        let mut grandes = 0_usize;
        while grandes < GRANDES {
            match frame::read::<ServerMessage>(&mut recebe).await {
                Ok(ServerMessage::MessageReceived { channel, .. })
                    if channel == ChannelId(LINE) =>
                {
                    grandes += 1;
                }
                Ok(_) => {}
                Err(erro) => return Err(erro),
            }
        }
        Ok(recebe)
    });
    let _recebe = tokio::time::timeout(PRAZO, lendo)
        .await
        .expect("o ouvinte que voltou a ler não recebeu as grandes no prazo")??;
    assert_eq!(
        ouvinte.conexao.acabou(),
        None,
        "quem voltou a ler foi fechado"
    );
    assert_eq!(
        servidor.server().atrasos.sessoes(),
        0,
        "contrapressão sem perda contou como atraso"
    );
    assert_eq!(ouvinte.conexao.contadores().cortes_por_fila, 0);
    servidor.shutdown();
    Ok(())
}

/// **A queda por erro de escrita também guarda o assento.**
///
/// A sessão está presa numa escrita (o ouvinte parou de ler, e o crédito
/// acabou) quando a conexão cai. A escrita pendente falha, e o laço sai pelo
/// `?`, e não pelo `break`. Antes de `session::reservar_ao_sair`, esse caminho
/// não guardava a reserva da graça: a volta chegava com outro `ssrc` e fora da
/// sala, e a voz «parava depois de reconectar».
#[tokio::test(flavor = "multi_thread")]
async fn quem_cai_com_a_escrita_presa_volta_sentado() -> Result<()> {
    let (endereco, servidor) = servidor().await?;
    let mut ouvinte = abrir(endereco, 1).await?;
    entrar_na_line(&mut ouvinte).await?;
    sentar(&mut ouvinte, &servidor).await?;
    let falante = abrir(endereco, 2).await?;
    let outro = {
        let guarda = servidor.server().persistence.lock().await;
        Channels::new(&guarda).create_channel("a-parte")?.id
    };

    // A escrita da sessão fica presa no crédito, sem atraso nenhum.
    encher(&servidor, falante.pessoa, outro, GRANDES, 0).await?;
    assert!(
        esperar_a_escrita_presa(&ouvinte.conexao).await,
        "a escrita da sessão não ficou presa no crédito"
    );
    assert_eq!(servidor.server().atrasos.sessoes(), 0);

    // E a conexão cai com a escrita presa.
    ouvinte.conexao.close(0, b"queda");
    let _ = tokio::time::timeout(PRAZO, ouvinte.conexao.closed())
        .await
        .expect("a conexão do ouvinte não fechou");

    let _volta = volta_sentado(endereco, &servidor, 1, &ouvinte).await?;
    assert_eq!(
        servidor.server().atrasos.sessoes(),
        0,
        "a queda contou como atraso"
    );
    drop(ouvinte.recebe);
    servidor.shutdown();
    Ok(())
}
