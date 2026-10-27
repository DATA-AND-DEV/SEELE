//! Quem hospeda fecha **avisando**: cada sessão lê que o servidor está
//! encerrando, e não um erro de transporte.
//!
//! # O defeito
//!
//! `Hospedagem::encerrar` (o SAIR de quem hospeda) e o `Drop` dela (fechar o
//! app) faziam `Daemon::shutdown`, que fecha o endpoint na hora. Quem estava
//! dentro lia a conexão caindo, e a casca dizia «não foi possível alcançar o
//! servidor», mandando a pessoa procurar problema de rede na casa dela. Pior:
//! a bateria interna entrava, e batia por cinco minutos numa porta que ninguém
//! ia atender. O protocolo tinha a frase certa desde sempre —
//! `DisconnectReason::ServerShuttingDown`, «O SERVIDOR ESTÁ ENCERRANDO» — e
//! ninguém a mandava ao fechar. É «o produto sabe e não conta».
//!
//! # O conserto, e o que cada teste daqui prova
//!
//! - `Daemon::encerrar_avisando` difunde `Event::ServidorEncerrando` pelo
//!   barramento, e toda sessão responde como a expulsão responde: escreve
//!   `Disconnecting { reason }`, espera a entrega e fecha. Só depois, com
//!   prazo, o endpoint fecha. O primeiro teste prova isso pelo `SAIR`
//!   (`Hospedagem::encerrar`), com clientes de verdade.
//! - `Hospedagem::despedir(teto)` é a mesma despedida para a saída do app, que
//!   o Plano 2 chama da casca: o runtime cai logo depois dela, e o que não
//!   tiver saído até ali não sai mais. O segundo teste prova que o aviso e o
//!   fechamento chegam antes de o runtime cair, com um par cru que mede o
//!   `closed()` do QUIC.
//!
//! O cliente trata `ServerShuttingDown` como fim (`a_sessao_acabou_aqui`, em
//! `seele-core/src/enlace.rs`), e é por isso que o primeiro teste também
//! confere que a sessão acabou com esse motivo, e não reconectou.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_core::enlace::{Aviso, Destino, Enlace, Motivo};
use seele_core::MemoryPinStore;
use seele_proto::control::{ClientMessage, DisconnectReason, ServerMessage};
use seele_proto::ids::{ChannelId, ClientMessageId};
use seele_server::hospedagem::Hospedagem;
use seele_server::persistence::Location;

mod ambiente;
mod par_cru;
mod vaga;

/// O canal que todo servidor novo tem.
const LINHA: u32 = 1;

fn destino(endereco: SocketAddr, apelido: &str) -> Destino {
    Destino {
        servidor: endereco,
        nome_tls: "localhost".into(),
        chave_do_pin: endereco.to_string(),
        apelido: apelido.to_owned(),
        segredo: None,
        esperada: None,
        o_pino_prova_o_servidor: true,
        aceito: None,
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
            Ok(_) | Err(_) => {}
        }
    }
    None
}

/// Conecta e prova que a sessão está de pé **do lado do servidor**.
///
/// A barreira é uma ida e volta pela Linha: a mensagem só volta se o laço da
/// sessão a leu, e o laço só começa depois de a sessão assinar o barramento.
/// Sem ela, a difusão do fechamento poderia sair antes de esta sessão
/// escutá-la, e o teste mediria o preparo em vez da despedida.
async fn de_pe(endereco: SocketAddr, semente: u8, apelido: &str) -> Result<Enlace> {
    let mut enlace = Enlace::conectar(
        destino(endereco, apelido),
        SigningKey::from_bytes(&[semente; 32]),
        Arc::new(MemoryPinStore::new()),
    )
    .await?;
    enlace.abrir_linha(ChannelId(LINHA)).await?;
    let dito = format!("{apelido} está aqui");
    enlace
        .dizer(
            ChannelId(LINHA),
            dito.clone(),
            ClientMessageId(u64::from(semente)),
        )
        .await?;
    let ouviu = esperar(&mut enlace, Duration::from_secs(10), |aviso| {
        matches!(
            aviso,
            Aviso::Mensagem(mensagem)
                if matches!(&**mensagem, ServerMessage::MessageReceived { body, .. } if *body == dito)
        )
    })
    .await;
    assert!(
        ouviu.is_some(),
        "a sessão de {apelido} não ficou de pé do lado do servidor: a Linha não devolveu o que \
         foi dito nela, e o fechamento mediria o preparo em vez da despedida"
    );
    Ok(enlace)
}

#[tokio::test(flavor = "multi_thread")]
async fn encerrar_avisa_cada_sessao_que_o_servidor_fechou() -> Result<()> {
    let _vaga = vaga::minha();
    let _ambiente = ambiente::sem_degrau_4();

    // Como em `ejetar.rs`: o servidor que o botão HOSPEDAR sobe, e não um
    // `Daemon` montado à mão. O que se mede é o SAIR de quem hospeda.
    let hospedagem = Hospedagem::iniciar(0, Location::Memory, "Casa", None, |_| Ok(())).await?;
    let endereco = SocketAddr::from(([127, 0, 0, 1], hospedagem.endereco().port()));

    // Duas sessões, e não uma: o aviso é difundido, e uma difusão que
    // chegasse só à primeira sessão a assinar o barramento passaria com uma.
    let mut asuka = de_pe(endereco, 51, "asuka").await?;
    let mut rei = de_pe(endereco, 52, "rei").await?;

    hospedagem.encerrar().await;

    for (apelido, enlace) in [("asuka", &mut asuka), ("rei", &mut rei)] {
        // Um: o motivo **chegou pelo protocolo**. Sem ele, a pessoa lê uma
        // queda de transporte, e a tela diz «não foi possível alcançar o
        // servidor» a quem precisa ler que ele fechou.
        let aviso = esperar(enlace, Duration::from_secs(5), |aviso| {
            matches!(
                aviso,
                Aviso::Mensagem(mensagem)
                    if matches!(
                        &**mensagem,
                        ServerMessage::Disconnecting {
                            reason: DisconnectReason::ServerShuttingDown
                        }
                    )
            )
        })
        .await;
        assert!(
            aviso.is_some(),
            "a sessão de {apelido} não leu `Disconnecting {{ ServerShuttingDown }}` quando quem \
             hospeda saiu: o servidor caiu sem avisar, e a tela dela diz «não foi possível \
             alcançar o servidor» em vez de «o servidor está encerrando»"
        );

        // Dois: a sessão **acaba** com esse motivo, e não reconecta.
        let fim = esperar(enlace, Duration::from_secs(5), |aviso| {
            matches!(aviso, Aviso::Encerrado(_) | Aviso::Reconectado { .. })
        })
        .await;
        assert!(
            matches!(
                fim,
                Some(Aviso::Encerrado(Motivo::Moderado(
                    DisconnectReason::ServerShuttingDown
                )))
            ),
            "a sessão de {apelido} leu que o servidor fechou e não acabou com esse motivo \
             ({fim:?}): a bateria bate cinco minutos numa porta que ninguém vai atender"
        );
    }
    Ok(())
}

// ------------------------------------------------- a despedida da saída do app

/// Um par cru: o `closed()` da conexão é o que este teste mede, e o `Enlace`
/// não o expõe — ele trata a queda e segue para a bateria.
///
/// Desde a 0.17 (ADR 0063), pelo `par_cru` do enlace, e não por um QUIC cru.
/// O fechamento que o servidor manda ao se despedir é o `corte` no `ctl`, e o
/// par o vê como `Motivo::Fechada { codigo }` — o que era o
/// `ApplicationClosed` do QUIC.
struct ParCru {
    /// Guardada para a ponta do cliente viver tanto quanto a conexão.
    _ponta: seele_enlace::Ponta,
    conexao: seele_enlace::Conexao,
    /// O envio fica aberto: fechá-lo seria o cliente se despedindo, e quem se
    /// despede aqui é o servidor.
    _envio: seele_enlace::Envio,
    recebe: seele_enlace::Recebe,
}

/// Só o aperto de mão do enlace (ICE, DTLS, os canais e a apresentação), sem
/// nenhuma mensagem do SEELE: é quem está entrando, e ainda não se apresentou.
async fn so_o_enlace(endereco: SocketAddr) -> (seele_enlace::Ponta, seele_enlace::Conexao) {
    par_cru::discar(endereco).await
}

/// Faz o aperto de mão inteiro e espera o laço da sessão estar de pé.
async fn par_cru(endereco: SocketAddr, semente: u8) -> Result<ParCru> {
    let chave = SigningKey::from_bytes(&[semente; 32]);
    let mut par = par_cru::ate_a_prova(endereco, &chave, &format!("pessoa{semente:03}")).await;
    loop {
        match par.proximo().await? {
            ServerMessage::Session { .. } => break,
            ServerMessage::Disconnecting { reason } => {
                anyhow::bail!("o servidor recusou o par cru: {reason:?}")
            }
            _ => {}
        }
    }

    // A barreira: o `Session` sai no aperto de mão, **antes** de a sessão
    // assinar o barramento. O `Pong` só sai do laço, e o laço só começa
    // depois da assinatura.
    par.mandar(&ClientMessage::Ping { timestamp: 1 }).await?;
    loop {
        if let ServerMessage::Pong { .. } = par.proximo().await? {
            break;
        }
    }
    Ok(ParCru {
        _ponta: par.ponta,
        conexao: par.conexao,
        _envio: par.envio,
        recebe: par.recebe,
    })
}

/// O teto que este teste passa a [`Hospedagem::despedir`].
const TETO_DA_DESPEDIDA: Duration = Duration::from_millis(800);

/// Quanto a mais a linha de quem hospeda pode levar depois do teto: o runtime
/// cair e a linha de execução terminar.
const FOLGA_DA_DESPEDIDA: Duration = Duration::from_millis(200);

/// A despedida da saída do app: o aviso e o fechamento chegam **antes** de o
/// runtime de quem hospeda cair, e `despedir` volta dentro do teto.
///
/// # A prova do `wait_idle` depende da máquina
///
/// Tirar o `wait_idle` do `despedir` reprova este teste **nesta** máquina,
/// medido no lote G-servidor: quem estava no meio do aperto de mão só vê a
/// conexão fechar pelo tempo ocioso, em 20,004 s. A reprovação depende de nada
/// mais deixar o runtime rodar entre o `shutdown` e a queda. O passo seguinte do
/// `despedir`, `escada.descer()`, devolve a porta do roteador quando o degrau 3
/// abriu uma: numa rede com UPnP que respondeu, ele faz E/S de rede, o runtime
/// roda o driver do endpoint enquanto espera, e o fechamento pode sair sem o
/// `wait_idle`. Ali a reversão pode passar verde sem que o `wait_idle` tenha
/// deixado de importar — ele é o que segura a ordem quando o roteador não tem
/// nada a devolver.
#[tokio::test(flavor = "multi_thread")]
async fn a_despedida_avisa_mesmo_com_o_runtime_caindo_em_seguida() -> Result<()> {
    let _vaga = vaga::minha();
    let _ambiente = ambiente::sem_degrau_4();

    // Quem hospeda numa linha de execução própria, com um runtime de uma
    // thread: é a forma da saída do app, em que o `ExitRequested` volta e o
    // runtime cai na linha seguinte. O que não saiu até ali não sai mais.
    let (porta_tx, porta_rx) = std::sync::mpsc::channel::<u16>();
    let (fechar_tx, fechar_rx) = tokio::sync::oneshot::channel::<()>();
    let anfitriao = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("o runtime de quem hospeda");
        runtime.block_on(async move {
            let hospedagem = Hospedagem::iniciar(0, Location::Memory, "Casa", None, |_| Ok(()))
                .await
                .expect("subir");
            porta_tx
                .send(hospedagem.endereco().port())
                .expect("o teste espera a porta");
            let _ = fechar_rx.await;
            hospedagem.despedir(TETO_DA_DESPEDIDA).await;
        });
        // A linha seguinte: o runtime cai, e com ele toda tarefa do servidor.
        drop(runtime);
        // Quando a linha de quem hospeda acabou, para o teste medir contra o
        // começo da despedida.
        Instant::now()
    });
    let porta = tokio::task::spawn_blocking(move || porta_rx.recv())
        .await?
        .expect("quem hospeda subiu e mandou a porta");
    let endereco = SocketAddr::from(([127, 0, 0, 1], porta));

    let ParCru {
        _ponta,
        conexao,
        _envio,
        mut recebe,
    } = par_cru(endereco, 61).await?;
    // E alguém no meio do aperto de mão no instante em que quem hospeda fecha:
    // conectado no enlace, sem `Hello`. Esta conexão não é sessão, não escuta o
    // barramento e não lê aviso nenhum. O que a alcança é o fechamento da
    // ponta no fim da despedida, e é ela que prova a espera depois dele —
    // a sessão de cima já fechou sozinha a essa altura, e passaria sem ela.
    let (_ponta_de_quem_entra, entrando) = so_o_enlace(endereco).await;

    let leitor = tokio::spawn(async move {
        loop {
            match seele_server::frame::read::<ServerMessage>(&mut recebe).await {
                Ok(ServerMessage::Disconnecting { reason }) => return Some(reason),
                Ok(_) => {}
                Err(_) => return None,
            }
        }
    });

    let comeco = Instant::now();
    fechar_tx
        .send(())
        .expect("quem hospeda espera o sinal de fechar");

    // O prazo de espera é mais longo que o tempo ocioso do transporte, de
    // propósito: sem a despedida, o `closed()` só chega por ele, e a mensagem
    // da reprovação diz quanto levou em vez de um prazo esgotado sem número.
    let ate_fechar = |conexao: seele_enlace::Conexao| async move {
        let fechou = tokio::time::timeout(
            seele_proto::transport::IDLE_TIMEOUT + Duration::from_secs(5),
            conexao.closed(),
        )
        .await;
        (fechou, comeco.elapsed())
    };
    let ((fechou, levou), (fechou_entrando, levou_entrando)) =
        tokio::join!(ate_fechar(conexao), ate_fechar(entrando));
    let aviso = tokio::time::timeout(Duration::from_secs(1), leitor)
        .await
        .ok()
        .and_then(Result::ok)
        .flatten();
    let acabou = anfitriao
        .join()
        .expect("a linha de quem hospeda terminou sem pânico");
    let a_despedida_levou = acabou.saturating_duration_since(comeco);
    // A medida, no registro da rodada: quanto do teto o fechamento usou.
    eprintln!(
        "quem_hospeda_fecha: aviso {aviso:?}; a sessão fechou em {levou:?} ({fechou:?}); quem \
         entrava fechou em {levou_entrando:?} ({fechou_entrando:?}); a despedida voltou e o \
         runtime caiu em {a_despedida_levou:?}"
    );

    assert_eq!(
        aviso,
        Some(DisconnectReason::ServerShuttingDown),
        "o cliente não leu que o servidor estava encerrando antes de o runtime de quem hospeda \
         cair (a conexão fechou em {levou:?}: {fechou:?}): a casca dele diz «não foi possível \
         alcançar o servidor» a quem precisa ler que quem hospeda fechou"
    );
    let Ok(motivo) = fechou else {
        panic!(
            "a conexão não fechou nem pelo tempo ocioso do transporte ({levou:?}): o teste não \
             mede nada"
        );
    };
    assert!(
        levou < Duration::from_secs(1),
        "o cliente levou {levou:?} para ver a conexão fechar ({motivo:?}): o fechamento não \
         saiu antes de o runtime cair, e quem estava dentro só descobre pelos {:?} do tempo \
         ocioso",
        seele_proto::transport::IDLE_TIMEOUT
    );
    assert!(
        matches!(motivo, seele_enlace::Motivo::Fechada { .. }),
        "a conexão fechou por outra coisa que não o servidor se despedindo: {motivo:?}"
    );

    // O teto é a promessa de `despedir` a quem fecha o app: a casca segura o
    // `ExitRequested` por ele, e uma despedida que passasse dele seguraria a
    // janela fechando.
    assert!(
        a_despedida_levou < TETO_DA_DESPEDIDA + FOLGA_DA_DESPEDIDA,
        "`despedir({TETO_DA_DESPEDIDA:?})` levou {a_despedida_levou:?} até o runtime cair: \
         passou do teto mais {FOLGA_DA_DESPEDIDA:?}, e a saída do app espera a despedida \
         inteira"
    );

    let Ok(motivo_de_quem_entrava) = fechou_entrando else {
        panic!(
            "a conexão de quem entrava não fechou nem pelo tempo ocioso do transporte \
             ({levou_entrando:?}): o teste não mede nada"
        );
    };
    assert!(
        levou_entrando < Duration::from_secs(1)
            && matches!(motivo_de_quem_entrava, seele_enlace::Motivo::Fechada { .. }),
        "quem estava no meio do aperto de mão levou {levou_entrando:?} para ver a conexão fechar \
         ({motivo_de_quem_entrava:?}): o fechamento da ponta ficou na fila quando o runtime caiu, \
         e só o tempo ocioso do transporte a derrubou. A despedida tem de esperar a ponta \
         terminar de fechar antes de devolver"
    );
    Ok(())
}
