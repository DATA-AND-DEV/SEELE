//! Attachments, both ends, against a real Dogma.
//!
//! `crates/seele-server/src/casper/attachments.rs` proves the ceiling with time
//! and files under its own control, and no interleaved schedule can prove the
//! thing this file is for: that the mechanism is **wired up**. A ceiling nobody
//! consults is exactly the state `DisconnectReason::RateLimited` was in before
//! ADR 0025 — in the protocol and in no part of the server.
//!
//! So these go through the wire, with the real client, and ask the four
//! questions a person would notice:
//!
//! - a file sent arrives whole, and the message is only on the Line afterwards;
//! - a file over the per-file limit is refused **with a reason**, not dropped;
//! - a Dogma that fills stays under its ceiling, and the message whose file was
//!   evicted still says what the file was;
//! - a file can be fetched back, and one that expired says so instead.
//!
//! `current_thread` for the reason `limite_de_taxa.rs` gives: a `select!` over
//! several tasks on a multi-threaded executor hides defects, and this repository
//! has watched a test pass ten times out of ten with the function under test
//! replaced by a `drop`.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use seele_core::client::{AttachmentRequest, Sent};
use seele_core::{Client, MemoryPinStore};
use seele_proto::control::{AttachmentRefusal, AttachmentState, ServerMessage};
use seele_proto::ids::{ClientMessageId, LineId};
use seele_server::casper::attachments::per_file_limit;
use seele_server::casper::Location;
use seele_server::{DogmaConfig, Server};

/// How long a test waits for a file the Dogma agreed to send.
const ESPERA: Duration = Duration::from_secs(5);

const LINHA: LineId = LineId(1);

/// A Dogma with a real database file and a real `anexos/` beside it.
///
/// On disk rather than in memory, because the ceiling is a promise about a
/// directory and `Location::Memory` has none.
async fn dogma(teto: u64) -> Result<(SocketAddr, Arc<Server>, tempfile::TempDir)> {
    let casa = tempfile::tempdir()?;
    let banco = casa.path().join("dogma.db");
    {
        let casper = seele_server::casper::Casper::open(&Location::File(banco.clone()))?;
        seele_server::casper::attachments::set_quota(&casper, teto)?;
    }

    let config = DogmaConfig {
        name: "Terceira Tóquio".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::File(banco),
        ..DogmaConfig::default()
    };
    let servidor = Arc::new(Server::bind(config).await?);
    let endereco = servidor.local_addr()?;
    let aceitando = Arc::clone(&servidor);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });
    Ok((endereco, servidor, casa))
}

async fn entrar(endereco: SocketAddr, semente: u8) -> Result<Client> {
    let mut cliente = Client::connect(
        endereco,
        "localhost",
        &endereco.to_string(),
        if semente == 7 { "ayanami" } else { "shinji" },
        &ed25519_dalek::SigningKey::from_bytes(&[semente; 32]),
        Arc::new(MemoryPinStore::new()),
        None,
    )
    .await?;
    cliente.join_line(LINHA).await?;
    Ok(cliente)
}

/// Writes a file of `tamanho` bytes, filled with `semente`.
fn arquivo(casa: &Path, nome: &str, tamanho: usize, semente: u8) -> std::path::PathBuf {
    let caminho = casa.join(nome);
    std::fs::write(&caminho, vec![semente; tamanho]).expect("escrever o arquivo");
    caminho
}

fn pedido<'a>(caminho: &'a Path, nome: &'a str, chave: u64) -> AttachmentRequest<'a> {
    AttachmentRequest {
        line: LINHA,
        client_message_id: ClientMessageId(chave),
        body: "olha isto",
        replies_to: None,
        path: caminho,
        file_name: nome,
        declared_type: "image/png",
    }
}

/// Espera um quadro que o predicado aceite, ou desiste.
async fn ate<T>(
    cliente: &mut Client,
    mut quero: impl FnMut(&ServerMessage) -> Option<T>,
) -> Option<T> {
    let prazo = tokio::time::Instant::now() + ESPERA;
    loop {
        let restante = prazo.saturating_duration_since(tokio::time::Instant::now());
        if restante.is_zero() {
            return None;
        }
        let Ok(Ok(evento)) = tokio::time::timeout(restante, cliente.next_event()).await else {
            return None;
        };
        if let Some(achado) = quero(&evento) {
            return Some(achado);
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn um_arquivo_sobe_inteiro_e_a_mensagem_so_aparece_depois() -> Result<()> {
    let (endereco, servidor, casa) = dogma(64 * 1024).await?;
    let quem_manda = entrar(endereco, 7).await?;
    let mut quem_espera = entrar(endereco, 9).await?;

    let caminho = arquivo(casa.path(), "foto.png", 3_000, 0xAB);

    // Nada na Linha antes de o último byte chegar. É a metade do ADR 0027 que
    // custa alguma coisa: enquanto sobe, só quem enviou vê.
    assert_eq!(servidor.quantas_mensagens(LINHA).await?, 0);

    let mut andamento = Vec::new();
    let enviados = quem_manda
        .transfers()
        .send_attachment(&pedido(&caminho, "foto.png", 1), |feito, total| {
            andamento.push((feito, total));
        })
        .await?;
    assert_eq!(
        enviados,
        seele_core::client::Sent::Delivered { bytes: 3_000 }
    );
    assert!(
        andamento.iter().all(|(_, total)| *total == 3_000),
        "o total tem de ser conhecido em todo passo: quem escolheu o arquivo \
         sabe o tamanho dele, então é sempre barra e nunca travessão"
    );

    // E do outro lado a mensagem aparece com o anexo pendurado nela.
    let anexo = ate(&mut quem_espera, |evento| match evento {
        ServerMessage::MessageReceived {
            body, attachment, ..
        } if body == "olha isto" => attachment.clone(),
        _ => None,
    })
    .await
    .expect("a mensagem com anexo chegou a quem espera");

    assert_eq!(anexo.file_name, "foto.png");
    assert_eq!(anexo.byte_size, 3_000);
    assert_eq!(anexo.declared_type, "image/png");
    assert_eq!(anexo.state, AttachmentState::Available);
    assert_eq!(servidor.quantas_mensagens(LINHA).await?, 1);

    // E os bytes voltam, iguais aos que subiram.
    let destino = casa.path().join("baixado.png");
    let baixados = quem_espera
        .download_attachment(anexo.id, &destino, ESPERA, |_, _| {})
        .await?;
    assert_eq!(baixados, 3_000);
    assert_eq!(std::fs::read(&destino)?, vec![0xAB; 3_000]);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn um_arquivo_grande_demais_e_recusado_com_razao_e_nao_em_silencio() -> Result<()> {
    // O teto por arquivo é derivado do total, e um arquivo acima dele é
    // recusado — não aceito para ser jogado fora depois. A razão carrega o
    // limite, senão a pessoa tenta de novo com um arquivo igualmente grande.
    let teto = 64 * 1024_u64;
    let (endereco, servidor, casa) = dogma(teto).await?;
    let mut cliente = entrar(endereco, 7).await?;

    let grande = usize::try_from(per_file_limit(teto)).unwrap() + 1;
    let caminho = arquivo(casa.path(), "grande.bin", grande, 1);

    let fim = cliente
        .transfers()
        .send_attachment(&pedido(&caminho, "grande.bin", 1), |_, _| {})
        .await?;
    assert!(
        matches!(fim, Sent::Stopped { .. }),
        "o Dogma tem de cortar o fluxo em vez de engolir os bytes de um \
         arquivo que já recusou: {fim:?}"
    );

    let razao = ate(&mut cliente, |evento| match evento {
        ServerMessage::AttachmentRefused { reason, .. } => Some(*reason),
        _ => None,
    })
    .await
    .expect("a recusa chegou pelo fluxo de controle");

    assert_eq!(
        razao,
        AttachmentRefusal::TooLarge {
            limit: per_file_limit(teto)
        }
    );
    assert_eq!(
        servidor.quantas_mensagens(LINHA).await?,
        0,
        "uma transferência recusada publicou mensagem"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn o_dogma_enche_sem_passar_do_teto_e_a_mensagem_diz_que_o_arquivo_expirou() -> Result<()> {
    // O teste que mais importa, pela porta da frente: o disco enchendo de
    // verdade, medido a cada arquivo, e o texto sobrevivendo ao anexo.
    let teto = 64 * 1024_u64;
    let (endereco, servidor, casa) = dogma(teto).await?;
    let mut cliente = entrar(endereco, 7).await?;

    let por_arquivo = usize::try_from(per_file_limit(teto)).unwrap();
    let anexos = casa.path().join("anexos");

    let mut primeiro = None;
    // Vinte e quatro arquivos do tamanho máximo num Dogma que cabe dezesseis.
    for numero in 0..24_u64 {
        let nome = format!("f{numero}.bin");
        let caminho = arquivo(
            casa.path(),
            &nome,
            por_arquivo,
            u8::try_from(numero).unwrap(),
        );
        cliente
            .transfers()
            .send_attachment(&pedido(&caminho, &nome, numero + 1), |_, _| {})
            .await?;

        let anexo = ate(&mut cliente, |evento| match evento {
            ServerMessage::MessageReceived { attachment, .. } => attachment.clone(),
            _ => None,
        })
        .await
        .unwrap_or_else(|| panic!("o anexo {numero} não foi publicado"));
        if primeiro.is_none() {
            primeiro = Some(anexo.id);
        }

        // Medido **a cada passo**, e não no fim: o defeito que o ADR nomeia
        // vive no instante entre aceitar e despejar, e um teste que só olha o
        // fim nunca o visita.
        let em_disco: u64 = std::fs::read_dir(&anexos)?
            .flatten()
            .filter_map(|entrada| entrada.metadata().ok())
            .map(|meta| meta.len())
            .sum();
        assert!(
            em_disco <= teto,
            "depois do arquivo {numero} o diretório tem {em_disco} bytes, \
             acima do teto de {teto}"
        );
    }

    // E o mais velho saiu. A linha continua lá, com nome e tamanho, dizendo que
    // o arquivo expirou — que é a diferença entre «este arquivo expirou» e uma
    // mensagem com nada dentro.
    let expirado = primeiro.expect("o primeiro anexo existiu");
    let mut cliente = entrar(endereco, 9).await?;
    cliente.fetch_history(LINHA, None, 50).await?;
    let anexo = ate(&mut cliente, |evento| match evento {
        ServerMessage::MessageReceived { attachment, .. } => {
            attachment.clone().filter(|anexo| anexo.id == expirado)
        }
        _ => None,
    })
    .await
    .expect("a mensagem do primeiro arquivo continua no histórico");

    assert_eq!(anexo.state, AttachmentState::Expired);
    assert_eq!(anexo.file_name, "f0.bin", "o nome foi embora com os bytes");
    assert_eq!(anexo.byte_size as usize, por_arquivo);

    // E pedir os bytes responde com a razão, em vez de nada.
    let destino = casa.path().join("nao-vem.bin");
    let _ = cliente
        .download_attachment(anexo.id, &destino, Duration::from_millis(300), |_, _| {})
        .await;
    let razao = ate(&mut cliente, |evento| match evento {
        ServerMessage::AttachmentUnavailable { reason, .. } => Some(*reason),
        _ => None,
    })
    .await
    .expect("o Dogma disse por que o arquivo não vem");
    assert_eq!(razao, AttachmentRefusal::Expired);

    // As mensagens todas continuam lá: expirar apaga bytes, não linhas.
    assert_eq!(servidor.quantas_mensagens(LINHA).await?, 24);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn a_mesma_foto_de_duas_pessoas_e_um_arquivo_so() -> Result<()> {
    let teto = 64 * 1024_u64;
    let (endereco, _servidor, casa) = dogma(teto).await?;
    let caminho = arquivo(casa.path(), "igual.png", 2_000, 0x5A);
    let anexos = casa.path().join("anexos");

    for semente in [7_u8, 9] {
        let mut cliente = entrar(endereco, semente).await?;
        cliente
            .transfers()
            .send_attachment(&pedido(&caminho, "igual.png", 1), |_, _| {})
            .await?;
        ate(&mut cliente, |evento| match evento {
            ServerMessage::MessageReceived { attachment, .. } => attachment.clone(),
            _ => None,
        })
        .await
        .expect("publicado");
    }

    assert_eq!(
        std::fs::read_dir(&anexos)?.count(),
        1,
        "duas pessoas mandando a mesma foto guardaram duas cópias"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn quem_nao_pode_anexar_e_recusado_com_razao() -> Result<()> {
    // «Pode escrever» e «pode pôr um gigabyte no meu notebook» são perguntas
    // diferentes, e esta é a segunda. O Observador é negado explicitamente.
    let casa = tempfile::tempdir()?;
    let banco = casa.path().join("dogma.db");
    let config = DogmaConfig {
        name: "Terceira Tóquio".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::File(banco),
        observers: vec!["ayanami".into()],
        ..DogmaConfig::default()
    };
    let servidor = Arc::new(Server::bind(config).await?);
    let endereco = servidor.local_addr()?;
    let aceitando = Arc::clone(&servidor);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });

    let mut cliente = entrar(endereco, 7).await?;
    let caminho = arquivo(casa.path(), "foto.png", 500, 3);
    let fim = cliente
        .transfers()
        .send_attachment(&pedido(&caminho, "foto.png", 1), |_, _| {})
        .await?;
    assert!(matches!(fim, Sent::Stopped { .. }), "{fim:?}");

    let razao = ate(&mut cliente, |evento| match evento {
        ServerMessage::AttachmentRefused { reason, .. } => Some(*reason),
        _ => None,
    })
    .await
    .expect("a recusa chegou");
    assert_eq!(razao, AttachmentRefusal::NotAllowed);
    assert_eq!(servidor.quantas_mensagens(LINHA).await?, 0);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn um_dogma_que_nao_guarda_arquivo_diz_isso_em_vez_de_deixar_pendurado() -> Result<()> {
    // Um Dogma em memória não tem diretório para chamar de seu, e a resposta
    // certa é uma frase. Não aceitar o fluxo deixaria a barra do outro lado
    // parada em zero até o tempo ocioso do QUIC recolher a conexão — que é a
    // forma de falhar que este projeto recusa em toda outra porta.
    let casa = tempfile::tempdir()?;
    let config = DogmaConfig {
        name: "Terceira Tóquio".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        ..DogmaConfig::default()
    };
    let servidor = Arc::new(Server::bind(config).await?);
    let endereco = servidor.local_addr()?;
    let aceitando = Arc::clone(&servidor);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });

    let mut cliente = entrar(endereco, 7).await?;
    let caminho = arquivo(casa.path(), "foto.png", 500, 2);
    cliente
        .transfers()
        .send_attachment(&pedido(&caminho, "foto.png", 1), |_, _| {})
        .await?;

    let razao = ate(&mut cliente, |evento| match evento {
        ServerMessage::AttachmentRefused { reason, .. } => Some(*reason),
        _ => None,
    })
    .await
    .expect("o Dogma disse que não guarda arquivo, em vez de calar");
    assert_eq!(razao, AttachmentRefusal::Unavailable);
    assert_eq!(servidor.quantas_mensagens(LINHA).await?, 0);
    Ok(())
}
