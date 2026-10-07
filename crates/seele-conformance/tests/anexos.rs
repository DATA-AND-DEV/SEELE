//! Attachments, both ends, against a real server.
//!
//! `crates/seele-server/src/persistence/attachments.rs` proves the ceiling with time
//! and files under its own control, and no interleaved schedule can prove the
//! thing this file is for: that the mechanism is **wired up**. A ceiling nobody
//! consults is exactly the state `DisconnectReason::RateLimited` was in before
//! ADR 0025 — in the protocol and in no part of the server.
//!
//! So these go through the wire, with the real client, and ask the four
//! questions a person would notice:
//!
//! - a file sent arrives whole, and the message is only on the Channel afterwards;
//! - a file over the per-file limit is refused **with a reason**, not dropped;
//! - a server that fills stays under its ceiling, and the message whose file was
//!   evicted still says what the file was;
//! - a file can be fetched back, and one that expired says so instead.
//!
//! E uma quinta, que é de quem recebe: um arquivo salvo fica **dentro** da
//! pasta, com um nome que é só um nome, e nunca no lugar de um que já estava
//! lá. Essa passa pela ponte (`seele_ffi::Connection`), e não pelo cliente: em
//! produção o nome sai do histórico da ponte, e um teste que o passasse como
//! argumento provaria um caminho que o app não usa.
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
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use seele_core::client::{AttachmentRequest, Previewed, Sent};
use seele_core::preview::{data_uri, judge, ImageFormat, Verdict, PREVIEW_LIMIT};
use seele_core::{Client, MemoryPinStore};
use seele_ffi::{ConnectConfig, Connection, Event, EventListener, NotSavedReason, Transfer};
use seele_proto::control::{AttachmentRefusal, AttachmentState, ServerMessage};
use seele_proto::ids::{AttachmentId, ChannelId, ClientMessageId};
use seele_server::persistence::attachments::per_file_limit;
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

mod vaga;

/// How long a test waits for a file the server agreed to send.
const ESPERA: Duration = Duration::from_secs(5);

const LINHA: ChannelId = ChannelId(1);

/// A server with a real database file and a real `anexos/` beside it.
///
/// On disk rather than in memory, because the ceiling is a promise about a
/// directory and `Location::Memory` has none.
async fn server(teto: u64) -> Result<(SocketAddr, Arc<Daemon>, tempfile::TempDir)> {
    let casa = tempfile::tempdir()?;
    let banco = casa.path().join("seele.db");
    {
        let persistence =
            seele_server::persistence::Persistence::open(&Location::File(banco.clone()))?;
        seele_server::persistence::attachments::set_quota(&persistence, teto)?;
    }

    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::File(banco),
        ..ServerConfig::default()
    };
    let servidor = Arc::new(Daemon::bind(config).await?);
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
        if semente == 7 { "marcela" } else { "rafael" },
        &ed25519_dalek::SigningKey::from_bytes(&[semente; 32]),
        Arc::new(MemoryPinStore::new()),
        None,
        None,
    )
    .await?;
    cliente.join_channel(LINHA).await?;
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
        channel: LINHA,
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
    let _vaga = vaga::minha();
    let (endereco, servidor, casa) = server(64 * 1024).await?;
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
async fn baixar_num_caminho_que_ja_existe_nao_toca_no_que_estava_la() -> Result<()> {
    let _vaga = vaga::minha();
    // A porta da conformidade, `download_attachment`, recebe um caminho inteiro
    // e grava nele. Com `File::create`, um arquivo que já estava lá era truncado
    // e reescrito com os bytes de outra pessoa, sem pergunta nenhuma. Agora os
    // bytes chegam num parcial ao lado, e quem recusa este caminho é o atalho
    // de `download_attachment`, que vê que ele já existe antes de pedir byte
    // nenhum. O atalho perde uma corrida; quem garante, numa pasta com link
    // físico como esta, é o link que dá o nome final, que falha se o nome
    // existir, e o que estava lá fica como estava.
    let (endereco, _servidor, casa) = server(64 * 1024).await?;
    let quem_manda = entrar(endereco, 7).await?;
    let mut quem_espera = entrar(endereco, 9).await?;

    let caminho = arquivo(casa.path(), "foto.png", 1_000, 0x11);
    let anexo = mandar_e_receber(
        &quem_manda,
        &mut quem_espera,
        &pedido(&caminho, "foto.png", 1),
    )
    .await?;

    let ja_estava = casa.path().join("ja-estava.png");
    std::fs::write(&ja_estava, "original")?;
    let resultado = quem_espera
        .download_attachment(anexo.id, &ja_estava, ESPERA, |_, _| {})
        .await;
    assert!(
        resultado.is_err(),
        "baixar por cima de um arquivo que já existe deu certo, e só dá certo \
         substituindo o que estava lá"
    );
    assert_eq!(
        std::fs::read_to_string(&ja_estava)?,
        "original",
        "o arquivo que já estava no caminho foi substituído pelos bytes do anexo"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn um_arquivo_grande_demais_e_recusado_com_razao_e_nao_em_silencio() -> Result<()> {
    let _vaga = vaga::minha();
    // O teto por arquivo é derivado do total, e um arquivo acima dele é
    // recusado — não aceito para ser jogado fora depois. A razão carrega o
    // limite, senão a pessoa tenta de novo com um arquivo igualmente grande.
    let teto = 64 * 1024_u64;
    let (endereco, servidor, casa) = server(teto).await?;
    let mut cliente = entrar(endereco, 7).await?;

    let grande = usize::try_from(per_file_limit(teto)).unwrap() + 1;
    let caminho = arquivo(casa.path(), "grande.bin", grande, 1);

    let fim = cliente
        .transfers()
        .send_attachment(&pedido(&caminho, "grande.bin", 1), |_, _| {})
        .await?;
    assert!(
        matches!(fim, Sent::Stopped { .. }),
        "o servidor tem de cortar o fluxo em vez de engolir os bytes de um \
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
async fn o_server_enche_sem_passar_do_teto_e_a_mensagem_diz_que_o_arquivo_expirou() -> Result<()> {
    let _vaga = vaga::minha();
    // O teste que mais importa, pela porta da frente: o disco enchendo de
    // verdade, medido a cada arquivo, e o texto sobrevivendo ao anexo.
    let teto = 64 * 1024_u64;
    let (endereco, servidor, casa) = server(teto).await?;
    let mut cliente = entrar(endereco, 7).await?;

    let por_arquivo = usize::try_from(per_file_limit(teto)).unwrap();
    let anexos = casa.path().join("anexos");

    let mut primeiro = None;
    // Vinte e quatro arquivos do tamanho máximo num servidor que cabe dezesseis.
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
    // O parcial é criado antes de o pedido sair, e um pedido que não vem tem de
    // levá-lo embora: senão cada anexo expirado deixa um arquivo vazio na pasta.
    assert!(
        !destino.exists(),
        "um anexo que não veio deixou um arquivo vazio onde ia ser gravado"
    );
    let sobras: Vec<String> = nomes_em(casa.path())
        .into_iter()
        .filter(|nome| nome.contains("nao-vem.bin"))
        .collect();
    assert!(
        sobras.is_empty(),
        "um anexo que não veio deixou o parcial na pasta: {sobras:?}"
    );
    let razao = ate(&mut cliente, |evento| match evento {
        ServerMessage::AttachmentUnavailable { reason, .. } => Some(*reason),
        _ => None,
    })
    .await
    .expect("o servidor disse por que o arquivo não vem");
    assert_eq!(razao, AttachmentRefusal::Expired);

    // As mensagens todas continuam lá: expirar apaga bytes, não linhas.
    assert_eq!(servidor.quantas_mensagens(LINHA).await?, 24);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn a_mesma_foto_de_duas_pessoas_e_um_arquivo_so() -> Result<()> {
    let _vaga = vaga::minha();
    let teto = 64 * 1024_u64;
    let (endereco, _servidor, casa) = server(teto).await?;
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
    let _vaga = vaga::minha();
    // «Pode escrever» e «pode pôr um gigabyte no meu notebook» são perguntas
    // diferentes, e esta é a segunda. O Observador é negado explicitamente.
    let casa = tempfile::tempdir()?;
    let banco = casa.path().join("seele.db");
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::File(banco),
        observers: vec!["marcela".into()],
        ..ServerConfig::default()
    };
    let servidor = Arc::new(Daemon::bind(config).await?);
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
async fn um_server_que_nao_guarda_arquivo_diz_isso_em_vez_de_deixar_pendurado() -> Result<()> {
    let _vaga = vaga::minha();
    // Um servidor em memória não tem diretório para chamar de seu, e a resposta
    // certa é uma frase. Não aceitar o fluxo deixaria a barra do outro lado
    // parada em zero até o tempo ocioso do QUIC recolher a conexão — que é a
    // forma de falhar que este projeto recusa em toda outra porta.
    let casa = tempfile::tempdir()?;
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
    .expect("o servidor disse que não guarda arquivo, em vez de calar");
    assert_eq!(razao, AttachmentRefusal::Unavailable);
    assert_eq!(servidor.quantas_mensagens(LINHA).await?, 0);
    Ok(())
}

// ------------------------------------------------------ a prévia, ADR 0027
//
// A metade da regra do ADR 0027 que ficou escrita e não construída: só uma
// lista curta de tipos de imagem é desenhada embutida, e **só quando os bytes
// concordam com a alegação**. Estes quatro encenam a coisa inteira contra um
// servidor de verdade — um arquivo escrito em disco, subido pela rede, buscado de
// volta e julgado —, porque ler o código e concluir que ele decide certo é
// exatamente o que não prova nada aqui.

/// Escreve um arquivo que **começa** por `assinatura` e é preenchido depois.
///
/// O recheio é constante e não importa: quem decide o que este arquivo é são os
/// primeiros bytes, e é sobre isso que estes testes são.
fn arquivo_com(casa: &Path, nome: &str, assinatura: &[u8], tamanho: usize) -> std::path::PathBuf {
    let caminho = casa.join(nome);
    let mut bytes = assinatura.to_vec();
    bytes.resize(tamanho.max(assinatura.len()), 0x5A);
    std::fs::write(&caminho, bytes).expect("escrever o arquivo");
    caminho
}

/// Um pedido com o tipo alegado escolhido por quem manda — que é o ponto.
fn pedido_alegando<'a>(
    caminho: &'a Path,
    nome: &'a str,
    alegado: &'a str,
    chave: u64,
) -> AttachmentRequest<'a> {
    AttachmentRequest {
        declared_type: alegado,
        ..pedido(caminho, nome, chave)
    }
}

/// Manda um arquivo e devolve o anexo como quem recebe o vê.
async fn mandar_e_receber(
    quem_manda: &Client,
    quem_espera: &mut Client,
    pedido: &AttachmentRequest<'_>,
) -> Result<seele_proto::control::AttachmentInfo> {
    let fim = quem_manda
        .transfers()
        .send_attachment(pedido, |_, _| {})
        .await?;
    assert!(matches!(fim, Sent::Delivered { .. }), "{fim:?}");
    let corpo = pedido.body.to_owned();
    ate(quem_espera, |evento| match evento {
        ServerMessage::MessageReceived {
            body, attachment, ..
        } if *body == corpo => attachment.clone(),
        _ => None,
    })
    .await
    .ok_or_else(|| anyhow::anyhow!("a mensagem com anexo não chegou"))
}

/// Pede um anexo e o traz para a memória, como a prévia faz.
async fn prever(cliente: &mut Client, anexo: AttachmentId, limite: u64) -> Result<Previewed> {
    let transferencias = cliente.transfers();
    cliente.fetch_attachment(anexo).await?;
    transferencias
        .preview_attachment(anexo, limite, ESPERA)
        .await
}

#[tokio::test(flavor = "current_thread")]
async fn um_arquivo_cujos_bytes_discordam_do_nome_nao_e_desenhado() -> Result<()> {
    let _vaga = vaga::minha();
    // O teste que mais importa deste trabalho. O arquivo se chama `foto.png` e
    // é alegado `image/png` — as duas coisas escritas por quem mandou —, e os
    // bytes dele são de um JPEG.
    let (endereco, _servidor, casa) = server(64 * 1024).await?;
    let quem_manda = entrar(endereco, 7).await?;
    let mut quem_espera = entrar(endereco, 9).await?;

    let jpeg = arquivo_com(casa.path(), "foto.png", &[0xFF, 0xD8, 0xFF, 0xE0], 900);
    let anexo = mandar_e_receber(
        &quem_manda,
        &mut quem_espera,
        &pedido_alegando(&jpeg, "foto.png", "image/png", 1),
    )
    .await?;
    assert_eq!(anexo.declared_type, "image/png");

    let Previewed::Whole(bytes) = prever(&mut quem_espera, anexo.id, PREVIEW_LIMIT).await? else {
        panic!("um arquivo de 900 bytes não cabe numa prévia");
    };

    let veredito = judge(&anexo.declared_type, &bytes);
    assert_eq!(
        veredito,
        Verdict::Disagrees {
            claimed: ImageFormat::Png,
            found: Some(ImageFormat::Jpeg),
        },
        "os bytes discordam do nome e o veredito não disse isso"
    );

    // E a parte que uma asserção sobre a variante não cobre: nada foi desenhado
    // como se fosse o que o arquivo diz ser, **e nada foi desenhado como o que
    // ele por acaso é**. Desenhá-lo como JPEG seria concluir que o nome não
    // decide nada e o arquivo de quem mandou decide tudo.
    assert!(
        !matches!(veredito, Verdict::Draw(_)),
        "um arquivo que não é o que diz ser foi desenhado assim mesmo"
    );

    // E não desenhar não é esconder: os bytes continuam lá, para salvar.
    let destino = casa.path().join("salvo.png");
    quem_espera
        .download_attachment(anexo.id, &destino, ESPERA, |_, _| {})
        .await?;
    assert_eq!(std::fs::read(&destino)?.len(), 900);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn um_programa_com_nome_de_imagem_tambem_nao_e_desenhado() -> Result<()> {
    let _vaga = vaga::minha();
    // O caso mais afiado do mesmo defeito: os bytes não são de imagem nenhuma.
    // `MZ` é o começo de um executável do Windows, e o arquivo se chama
    // `gatinho.png`.
    let (endereco, _servidor, casa) = server(64 * 1024).await?;
    let quem_manda = entrar(endereco, 7).await?;
    let mut quem_espera = entrar(endereco, 9).await?;

    let programa = arquivo_com(casa.path(), "gatinho.png", b"MZ\x90\x00\x03\x00", 700);
    let anexo = mandar_e_receber(
        &quem_manda,
        &mut quem_espera,
        &pedido_alegando(&programa, "gatinho.png", "image/png", 1),
    )
    .await?;

    let Previewed::Whole(bytes) = prever(&mut quem_espera, anexo.id, PREVIEW_LIMIT).await? else {
        panic!("um arquivo de 700 bytes não cabe numa prévia");
    };
    assert_eq!(
        judge(&anexo.declared_type, &bytes),
        Verdict::Disagrees {
            claimed: ImageFormat::Png,
            found: None,
        }
    );

    // E o nome chegou como saiu. O ADR 0027 é explícito: não renomeia, não corta
    // extensão — um arquivo que mente é pior do que um arquivo que se apresenta.
    assert_eq!(anexo.file_name, "gatinho.png");
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn um_arquivo_que_e_o_que_diz_ser_vira_uma_figura() -> Result<()> {
    let _vaga = vaga::minha();
    // O outro ramo, pela mesma porta: os bytes concordam com a alegação, e o
    // tipo de mídia do `data:` sai do que foi **achado**.
    let (endereco, _servidor, casa) = server(64 * 1024).await?;
    let quem_manda = entrar(endereco, 7).await?;
    let mut quem_espera = entrar(endereco, 9).await?;

    let png = arquivo_com(
        casa.path(),
        "foto.png",
        &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A],
        900,
    );
    let anexo = mandar_e_receber(
        &quem_manda,
        &mut quem_espera,
        &pedido_alegando(&png, "foto.png", "image/png", 1),
    )
    .await?;

    let Previewed::Whole(bytes) = prever(&mut quem_espera, anexo.id, PREVIEW_LIMIT).await? else {
        panic!("um arquivo de 900 bytes não cabe numa prévia");
    };
    let Verdict::Draw(formato) = judge(&anexo.declared_type, &bytes) else {
        panic!("bytes que concordam com o nome não foram desenhados");
    };
    assert_eq!(formato, ImageFormat::Png);

    let uri = data_uri(formato, &bytes);
    assert!(
        uri.starts_with("data:image/png;base64,"),
        "o tipo de mídia do data: não saiu do que foi achado nos bytes"
    );
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn um_arquivo_maior_que_o_limite_da_previa_nao_e_baixado() -> Result<()> {
    let _vaga = vaga::minha();
    // O limite da prévia é decidido **separado** do limite por arquivo, porque
    // as duas coisas protegem máquinas diferentes: aquele é o disco de quem
    // hospeda, este é a memória de quem lê. Aqui o servidor aceita o arquivo de bom
    // grado — ele cabe no teto dele — e a prévia o recusa assim mesmo.
    let teto = 128 * 1024 * 1024_u64;
    let (endereco, _servidor, casa) = server(teto).await?;
    let quem_manda = entrar(endereco, 7).await?;
    let mut quem_espera = entrar(endereco, 9).await?;

    let tamanho = usize::try_from(PREVIEW_LIMIT).unwrap() + 4096;
    assert!(
        u64::try_from(tamanho).unwrap() < per_file_limit(teto),
        "o arquivo deste teste tem de caber no servidor para a recusa ser da prévia"
    );
    let enorme = arquivo_com(
        casa.path(),
        "panorama.png",
        &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A],
        tamanho,
    );
    let anexo = mandar_e_receber(
        &quem_manda,
        &mut quem_espera,
        &pedido_alegando(&enorme, "panorama.png", "image/png", 1),
    )
    .await?;
    assert_eq!(anexo.state, AttachmentState::Available);

    let previa = prever(&mut quem_espera, anexo.id, PREVIEW_LIMIT).await?;
    assert_eq!(
        previa,
        Previewed::TooBig {
            byte_size: u64::try_from(tamanho).unwrap()
        },
        "um arquivo acima do limite da prévia foi trazido assim mesmo"
    );

    // E a conexão sobrevive a ter cortado aquele fluxo: salvar o mesmo arquivo
    // continua funcionando, que é a diferença entre recusar uma prévia e perder
    // o anexo. Num caminho novo: `panorama.png` é o arquivo que subiu, e o
    // download não grava por cima do que já existe.
    let destino = casa.path().join("panorama-salvo.png");
    let baixados = quem_espera
        .download_attachment(anexo.id, &destino, ESPERA, |_, _| {})
        .await?;
    assert_eq!(baixados, u64::try_from(tamanho).unwrap());
    Ok(())
}

// --------------------------------------------- salvar, do lado de quem recebe
//
// O nome que veio com o arquivo é de quem mandou, e o fio o leva como veio. Do
// lado de quem recebe ele vira caminho, e é aí que a regra mora:
// `seele_core::anexo_no_disco`. Estes três passam pela ponte, como o app.

/// O que a ponte contou sobre transferências, na ordem.
#[derive(Default)]
struct Transferencias(Mutex<Vec<Transfer>>);

impl EventListener for Transferencias {
    fn on_event(&self, event: Event) {
        if let Event::TransferChanged { transfer } = event {
            if let Ok(mut vistas) = self.0.lock() {
                vistas.push(transfer);
            }
        }
    }
}

impl Transferencias {
    /// Como terminou o salvar deste anexo, se já terminou.
    fn fim_de(&self, anexo: u64) -> Option<Transfer> {
        self.0.lock().ok()?.iter().find_map(|vista| match vista {
            Transfer::Saved { attachment, .. } | Transfer::NotSaved { attachment, .. }
                if *attachment == anexo =>
            {
                Some(vista.clone())
            }
            _ => None,
        })
    }
}

/// Espera, sem segurar o executor, até a condição valer ou o prazo acabar.
///
/// Sem `std::thread::sleep`: o servidor roda neste mesmo executor de uma
/// thread, e dormir a thread seria parar o servidor que o teste espera.
async fn ate_que(mut pronto: impl FnMut() -> bool) -> bool {
    let prazo = tokio::time::Instant::now() + ESPERA;
    while tokio::time::Instant::now() < prazo {
        if pronto() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    pronto()
}

/// Quem recebe, pela ponte que o comando do Tauri segura, com a linha aberta.
async fn recebedor(
    endereco: SocketAddr,
    casa: &Path,
) -> Result<(Arc<Connection>, Arc<Transferencias>)> {
    let home = casa.join("carla").to_string_lossy().into_owned();
    let (ponte, _confianca) = tokio::task::spawn_blocking(move || {
        Connection::connect(ConnectConfig {
            server: endereco.to_string(),
            alternate_servers: Vec::new(),
            nickname: "carla".to_owned(),
            home,
            join_secret: None,
            expected_fingerprint: None,
            bilhete: None,
            // Não há placa de som numa máquina de integração contínua.
            audio: false,
            capture_device: None,
            playback_device: None,
        })
    })
    .await??;
    let vistas = Arc::new(Transferencias::default());
    ponte.subscribe(Arc::clone(&vistas) as Arc<dyn EventListener>);
    ponte.open_channel(LINHA.0)?;
    Ok((ponte, vistas))
}

/// Manda um arquivo com este nome e espera ele entrar no histórico da ponte.
async fn chegar_na_ponte(
    quem_manda: &Client,
    ponte: &Connection,
    caminho: &Path,
    nome: &str,
) -> Result<u64> {
    let fim = quem_manda
        .transfers()
        .send_attachment(&pedido(caminho, nome, 1), |_, _| {})
        .await?;
    assert!(
        matches!(fim, Sent::Delivered { .. }),
        "o arquivo de quem manda não subiu, e o teste não chega a salvar nada: {fim:?}"
    );
    let mut anexo = None;
    let chegou = ate_que(|| {
        anexo = ponte.messages().iter().find_map(|mensagem| {
            mensagem
                .attachment
                .as_ref()
                .filter(|anexo| anexo.file_name == nome)
                .map(|anexo| anexo.id)
        });
        anexo.is_some()
    })
    .await;
    assert!(
        chegou,
        "a mensagem com «{nome}» não chegou ao histórico da ponte, e é de lá que \
         o nome sai"
    );
    Ok(anexo.expect("o anexo chegou"))
}

#[tokio::test(flavor = "current_thread")]
async fn um_nome_com_caminho_nao_grava_fora_da_pasta() -> Result<()> {
    let _vaga = vaga::minha();
    // O remetente chama o arquivo de `../escapou.txt`. O fio leva o nome como
    // veio, e quem recebe salva em `casa/pasta`: se o nome virasse caminho, o
    // arquivo apareceria em `casa/escapou.txt`, fora da pasta que a pessoa vê
    // na frase de confirmação.
    let (endereco, _servidor, casa) = server(64 * 1024).await?;
    let quem_manda = entrar(endereco, 7).await?;
    let (ponte, vistas) = recebedor(endereco, casa.path()).await?;

    let origem = arquivo(casa.path(), "origem.txt", 500, 0x42);
    let anexo = chegar_na_ponte(&quem_manda, &ponte, &origem, "../escapou.txt").await?;

    let pasta = casa.path().join("pasta");
    std::fs::create_dir(&pasta)?;
    ponte.save_attachment(anexo, pasta.to_string_lossy().into_owned())?;

    let terminou = ate_que(|| vistas.fim_de(anexo).is_some()).await;
    assert!(
        terminou,
        "salvar um anexo com caminho no nome não terminou nem em salvo nem em não \
         salvo, e a tela ficaria esperando para sempre"
    );
    assert!(
        !casa.path().join("escapou.txt").exists(),
        "o nome `../escapou.txt` virou caminho e o arquivo foi gravado fora da \
         pasta escolhida"
    );
    assert_eq!(
        vistas.fim_de(anexo),
        Some(Transfer::NotSaved {
            attachment: anexo,
            reason: NotSavedReason::NomeRecusado,
        }),
        "um nome com caminho não chegou à tela como nome recusado, e ela mandaria \
         tentar de novo o que vai ser recusado de novo"
    );
    assert_eq!(
        std::fs::read_dir(&pasta)?.count(),
        0,
        "um nome recusado deixou alguma coisa dentro da pasta"
    );
    ponte.disconnect();
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn um_nome_repetido_nao_substitui_o_que_ja_estava_la() -> Result<()> {
    let _vaga = vaga::minha();
    // A pessoa já tem um `foto.png` na pasta de downloads, e chega outro com o
    // mesmo nome. Com `File::create`, o dela era truncado e reescrito com os
    // bytes de outra pessoa, sem pergunta. Agora o novo grava ao lado.
    let (endereco, _servidor, casa) = server(64 * 1024).await?;
    let quem_manda = entrar(endereco, 7).await?;
    let (ponte, vistas) = recebedor(endereco, casa.path()).await?;

    let origem = arquivo(casa.path(), "origem.png", 3_000, 0xAB);
    let anexo = chegar_na_ponte(&quem_manda, &ponte, &origem, "foto.png").await?;

    let pasta = casa.path().join("pasta");
    std::fs::create_dir(&pasta)?;
    std::fs::write(pasta.join("foto.png"), "original")?;
    ponte.save_attachment(anexo, pasta.to_string_lossy().into_owned())?;

    assert!(
        ate_que(|| vistas.fim_de(anexo).is_some()).await,
        "salvar o anexo não terminou dentro do prazo"
    );
    let ao_lado = pasta.join("foto (2).png");
    assert_eq!(
        vistas.fim_de(anexo),
        Some(Transfer::Saved {
            attachment: anexo,
            path: ao_lado.display().to_string(),
        }),
        "o anexo não foi salvo ao lado, ou a tela não recebeu o caminho real em \
         que ele ficou"
    );
    assert_eq!(
        std::fs::read_to_string(pasta.join("foto.png"))?,
        "original",
        "o `foto.png` que já estava na pasta foi substituído pelo anexo"
    );
    assert_eq!(
        std::fs::read(&ao_lado)?,
        vec![0xAB; 3_000],
        "o que foi gravado ao lado não são os bytes que subiram"
    );
    assert_eq!(
        nomes_em(&pasta),
        ["foto (2).png", "foto.png"],
        "depois de salvar, a pasta tem mais que o arquivo da pessoa e o salvo ao \
         lado: o parcial ficou para trás"
    );
    ponte.disconnect();
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn um_anexo_que_nao_fecha_com_o_hash_nao_apaga_o_que_ja_estava_la() -> Result<()> {
    let _vaga = vaga::minha();
    // A variante do apagar. Quando os bytes não fecham com o hash, o arquivo é
    // descartado — e com `File::create` o descartado era o `foto.png` da
    // pessoa: truncado, reescrito, e depois apagado. Só pode sair o que esta
    // gravação criou.
    let (endereco, _servidor, casa) = server(64 * 1024).await?;
    let quem_manda = entrar(endereco, 7).await?;
    let (ponte, vistas) = recebedor(endereco, casa.path()).await?;

    let origem = arquivo(casa.path(), "origem.png", 3_000, 0xCD);
    let anexo = chegar_na_ponte(&quem_manda, &ponte, &origem, "foto.png").await?;

    // Os bytes do servidor estragam no disco dele, com o mesmo tamanho: o
    // servidor manda o que tem, e o hash que ele anuncia é o de antes.
    let guardado = std::fs::read_dir(casa.path().join("anexos"))?
        .flatten()
        .map(|entrada| entrada.path())
        .find(|caminho| std::fs::read(caminho).is_ok_and(|bytes| bytes == vec![0xCD; 3_000]))
        .expect("o servidor guardou os bytes do anexo");
    std::fs::write(&guardado, vec![0xEE; 3_000])?;

    let pasta = casa.path().join("pasta");
    std::fs::create_dir(&pasta)?;
    std::fs::write(pasta.join("foto.png"), "original")?;
    ponte.save_attachment(anexo, pasta.to_string_lossy().into_owned())?;

    assert!(
        ate_que(|| vistas.fim_de(anexo).is_some()).await,
        "salvar o anexo estragado não terminou dentro do prazo"
    );
    assert_eq!(
        vistas.fim_de(anexo),
        Some(Transfer::NotSaved {
            attachment: anexo,
            reason: NotSavedReason::Falhou,
        }),
        "um anexo que não fecha com o hash não chegou à tela como falha"
    );
    assert_eq!(
        std::fs::read_to_string(pasta.join("foto.png"))
            .ok()
            .as_deref(),
        Some("original"),
        "o anexo que não fechou com o hash levou junto o `foto.png` que já estava \
         na pasta"
    );
    assert!(
        !pasta.join("foto (2).png").exists(),
        "o arquivo criado para o anexo estragado ficou na pasta pela metade"
    );
    assert_eq!(
        nomes_em(&pasta),
        ["foto.png"],
        "o anexo que não fechou com o hash deixou o parcial na pasta"
    );
    ponte.disconnect();
    Ok(())
}

/// Os nomes que estão na pasta agora, em ordem.
fn nomes_em(pasta: &Path) -> Vec<String> {
    let mut nomes: Vec<String> = std::fs::read_dir(pasta)
        .expect("ler a pasta do teste")
        .flatten()
        .map(|entrada| entrada.file_name().to_string_lossy().into_owned())
        .collect();
    nomes.sort();
    nomes
}

#[tokio::test(flavor = "current_thread")]
async fn um_download_pela_metade_nunca_tem_o_nome_final() -> Result<()> {
    let _vaga = vaga::minha();
    // Um processo que morre no meio do download não apaga nada: o que fica na
    // pasta é o que estava nela naquele instante. Com o arquivo criado já com
    // o nome final, era um «foto.png» truncado com cara de completo, que a
    // pessoa abre sem saber que faltam bytes.
    //
    // O andamento roda no meio da gravação, entre um bloco e o seguinte, e o
    // que a pasta tem ali é o que um processo morto ali deixaria. Pelo
    // `download_attachment`, e não pela ponte, porque é ele que chama o
    // andamento na mesma volta da gravação: um evento da ponte chega depois, e
    // olhar a pasta nele seria olhar outro instante.
    let teto = 8 * 1024 * 1024_u64;
    let (endereco, _servidor, casa) = server(teto).await?;
    let quem_manda = entrar(endereco, 7).await?;
    let mut quem_espera = entrar(endereco, 9).await?;

    // Maior que um bloco de leitura, para haver um meio de verdade.
    let tamanho = 300_000_usize;
    let origem = arquivo(casa.path(), "origem.png", tamanho, 0x3C);
    let anexo = mandar_e_receber(
        &quem_manda,
        &mut quem_espera,
        &pedido(&origem, "foto.png", 1),
    )
    .await?;

    let pasta = casa.path().join("pasta");
    std::fs::create_dir(&pasta)?;
    let destino = pasta.join("foto.png");
    let mut no_meio: Vec<(u64, Vec<String>)> = Vec::new();
    let baixados = quem_espera
        .download_attachment(anexo.id, &destino, ESPERA, |feito, total| {
            if feito < total {
                no_meio.push((feito, nomes_em(&pasta)));
            }
        })
        .await?;
    assert_eq!(
        baixados,
        u64::try_from(tamanho).unwrap(),
        "o anexo não chegou inteiro, e o teste não mede o meio de um download que terminou"
    );
    assert!(
        no_meio.iter().any(|(feito, _)| *feito > 0),
        "o andamento não foi chamado com nenhum byte gravado e o download por \
         terminar, então este teste não olhou a pasta no meio de nada: {no_meio:?}"
    );
    for (feito, nomes) in &no_meio {
        assert!(
            !nomes.iter().any(|nome| nome == "foto.png"),
            "com {feito} de {tamanho} bytes gravados, a pasta já tinha um \
             «foto.png»: se o processo morresse ali, ficaria um arquivo truncado \
             com o nome de um completo. Pasta: {nomes:?}"
        );
    }

    // E no fim fica só o nome final, com os bytes que subiram: o parcial saiu.
    assert_eq!(
        nomes_em(&pasta),
        ["foto.png"],
        "depois do download a pasta não tem só o arquivo salvo, e o parcial ficou \
         para trás"
    );
    assert_eq!(
        std::fs::read(&destino)?,
        vec![0x3C; tamanho],
        "o arquivo com o nome final não tem os bytes que subiram"
    );

    // A quarentena fica no arquivo com o nome final, que é o que alguém abre.
    #[cfg(target_os = "macos")]
    {
        let marca = std::process::Command::new("xattr")
            .args(["-p", "com.apple.quarantine"])
            .arg(&destino)
            .output()?;
        assert!(
            marca.status.success() && String::from_utf8_lossy(&marca.stdout).contains("SEELE"),
            "o arquivo salvo não tem a quarentena do SEELE, e o Gatekeeper não para \
             quem for abri-lo: {marca:?}"
        );
    }
    Ok(())
}
