//! A medida da subida, do enlace até o portão e até o disco — e, desde a
//! 0.17, a falta dela, que não vira número.
//!
//! # A lacuna que este teste fecha
//!
//! Três ondas puseram a subida medida para trabalhar, e cada peça tem prova
//! isolada: a `SondaDaSubida` aprende com a janela que o cano encheu, a
//! `VoiceRoom` reage ao número partilhado, e `persistence::subida` guarda e
//! devolve. **Nenhuma delas prova a costura** — que num servidor de verdade,
//! com conexões de verdade, os contadores do `quinn` viram uma medida, e que
//! essa medida sai do tique de telemetria e chega aos dois destinos.
//!
//! É a diferença que o CLAUDE.md deste repositório chama de «existir não é
//! funcionar»: cada unidade passava, e a junção não tinha ninguém olhando.
//!
//! # Por que a conversa, e não uma transmissão de tela
//!
//! Porque é o caso que a onda existe para servir. Antes dela, a sonda só
//! aprendia enquanto a tela transmitia: `permitido_bps` é zero sem
//! transmissão, `cheia` é falso, e toda janela morria. A primeira tela de uma
//! call abria na hipótese de 2 Mbps — e o portão de admissão, que divide esse
//! número por N, recusava a partir do sétimo espectador enquanto isso.
//!
//! Aqui **ninguém transmite tela nenhuma**. O que enche o cano é a sala
//! conversando, e o braço do piso demonstrado é o único caminho pelo qual a
//! estimativa pode sair da hipótese. Se ele não estiver ligado de ponta a
//! ponta, este teste reprova.

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
use seele_proto::control::{ClientMessage, ServerMessage};
use seele_proto::ids::{Ssrc, VoiceRoomId};
use seele_proto::media::MediaHeader;
use seele_server::persistence::Location;
use seele_server::{frame, Daemon, ServerConfig};

mod comum;

/// Um quadro do maior tamanho que o protocolo aceita.
///
/// **Grande de propósito, e não realista de propósito.** O que este teste
/// precisa é que a subida do servidor passe de `CAMINHO_DO_SERVER_BPS` dentro
/// de uma janela de um segundo, e o teto de `MAX_FRAMES_PER_SECOND` limita
/// quantos quadros cabem ali. Com o payload no máximo, quatro pares bastam;
/// com o payload de um Opus a 48 kbps seriam dezenas de conexões para provar a
/// mesma costura. O servidor não decodifica nada — `specs/08-seguranca.md`
/// proíbe tocar no payload —, então o conteúdo não importa e só o tamanho faz
/// diferença aqui.
const PAYLOAD: [u8; seele_proto::media::MAX_PAYLOAD_LEN] = [7; seele_proto::media::MAX_PAYLOAD_LEN];

/// Um servidor em memória, já atendendo.
async fn server() -> Result<(SocketAddr, Arc<Daemon>)> {
    servidor_com(Location::Memory).await
}

/// O mesmo servidor, sobre o banco que se pedir.
///
/// Um arquivo, e não memória, é o que permite o segundo arranque ler o que o
/// primeiro escreveu — que é a afirmação inteira de `partindo_de`.
async fn servidor_com(database: Location) -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database,
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
///
/// `recebe` não é lido aqui — este teste não espera resposta nenhuma do
/// servidor, ele olha o estado dele por dentro. A ponta fica no `struct` assim
/// mesmo porque largá-la fecharia o fluxo de controle, e um par sem controle é
/// um par que o servidor derruba no meio da conversa.
#[allow(dead_code, reason = "a ponta é segurada, não lida — ver o doc acima")]
struct Par {
    conexao: seele_enlace::Conexao,
    envio: seele_enlace::Envio,
    recebe: seele_enlace::Recebe,
    /// O que o servidor atribuiu a esta conexão. Nunca escolhido pelo cliente.
    ssrc: Ssrc,
    /// A primeira sala de voz que este par pode ver.
    sala: VoiceRoomId,
}

/// Abre uma conexão e faz o aperto de mão à mão.
///
/// `janela` é a janela de recepção de fluxo que este par anuncia. Uma janela
/// pequena não cria o defeito: só encurta o tempo até o par parar de aceitar
/// bytes, que é a condição do cenário.
async fn abrir(endereco: SocketAddr, semente: u8, _janela: u32) -> Result<Par> {
    // Na 0.17 não há janela de fluxo a anunciar: o enlace guarda até 1 MiB
    // por canal e corta quem estoura (`seele_enlace::CORTE_FILA`). O
    // parâmetro fica pela forma dos chamadores.
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
        ssrc, voice_rooms, ..
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
        ssrc,
        sala,
    })
}

/// Um quadro de voz pronto para o fio, com o ssrc que o servidor atribuiu.
fn quadro(ssrc: Ssrc, seq: u16) -> Vec<u8> {
    let cabecalho = MediaHeader {
        ssrc: ssrc.get(),
        seq,
        // 20 ms a 48 kHz são 960 amostras. O servidor não lê isto; quem lê é o
        // buffer de jitter do outro lado.
        timestamp: u32::from(seq) * 960,
    };
    let mut fora = vec![0_u8; seele_proto::MAX_DATAGRAM_LEN];
    let tamanho = cabecalho
        .encode_datagram(&PAYLOAD, &mut fora)
        .expect("o quadro cabe num datagrama");
    fora.truncate(tamanho);
    fora
}

/// Uma sala de quatro conversando por três segundos, como o teste de antes da
/// 0.17 a montava: o que sai desta máquina é doze vezes o que entra.
async fn conversar(endereco: SocketAddr) -> Result<Vec<Par>> {
    let mut pares = Vec::new();
    for semente in 1..=4_u8 {
        let mut par = abrir(endereco, semente, 1024 * 1024).await?;
        let sala = par.sala;
        frame::write(
            &mut par.envio,
            &ClientMessage::EnterVoiceRoom {
                voice_room: sala,
                password: None,
            },
        )
        .await?;
        pares.push(par);
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut falando = Vec::new();
    for par in pares {
        falando.push(tokio::spawn(async move {
            for seq in 0..150_u16 {
                let _ = par.conexao.send_datagram(quadro(par.ssrc, seq));
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            par
        }));
    }
    let mut pares = Vec::new();
    for quem in falando {
        pares.push(quem.await?);
    }
    Ok(pares)
}

/// **Na 0.17, a conversa enche o cano e o servidor não finge ter medido**
/// (revisão do Codex; `tela::LeituraDaConexao`).
///
/// Até a 0.16 este teste provava a costura inteira — os contadores do `quinn`
/// viravam uma medida, que chegava ao portão e ao disco. O enlace não mede
/// perda nem congestionamento, e a sonda decide subir pela ausência deles: com
/// o zero sintético, a conversa «provaria» uma subida que ninguém mediu.
///
/// A costura continua sendo provada, agora no sentido honesto: as janelas
/// chegaram à sonda (contadas em `janelas_sem_medida`), a estimativa ficou na
/// de partida, nada foi anunciado como medido e nada foi ao disco. Quando o
/// enlace medir perda (pedido ao A), a sonda volta a subir, e este teste volta
/// a ser o de antes.
#[tokio::test(flavor = "multi_thread")]
async fn a_conversa_sem_medida_de_perda_nao_vira_medida_nem_vai_ao_disco() -> Result<()> {
    let (endereco, servidor) = server().await?;
    let pares = conversar(endereco).await?;

    let (medida, estimativa, sem_medida) = {
        let subida = servidor.server().subida.lock().await;
        (
            subida.medida(),
            subida.estimativa(),
            subida.janelas_sem_medida(),
        )
    };
    println!("subida medida: {medida:?}, estimativa {estimativa}, janelas sem medida {sem_medida}");
    assert!(
        sem_medida > 0,
        "a sala conversou por três segundos e nenhuma janela chegou à sonda: a costura de \
         leitura do tique de telemetria está cortada"
    );
    assert_eq!(
        medida, None,
        "sem medida de perda, a subida foi anunciada como medida"
    );
    assert_eq!(estimativa, seele_server::tela::CAMINHO_DO_SERVER_BPS);
    let guardada = {
        let banco = servidor.server().persistence.lock().await;
        seele_server::persistence::subida::lembrada(&banco)?
    };
    assert_eq!(guardada, None, "uma subida não medida foi ao disco");

    drop(pares);
    servidor.shutdown();
    Ok(())
}

/// **O segundo arranque começa da memória que vale aqui**, e não de qualquer
/// número guardado.
///
/// A medida da mesma rede, recente e da linha 2 sobrevive ao reinício (era a
/// afirmação deste teste até a 0.16), grampeada no teto da memória. A que a
/// 0.16 deixou — só o número, sem dizer onde nem quando — não dá confiança
/// nenhuma ao caminho de agora: o servidor nasce na hipótese.
#[tokio::test(flavor = "multi_thread")]
async fn o_segundo_arranque_comeca_da_memoria_que_vale_aqui() -> Result<()> {
    use seele_server::persistence::subida::{
        agora, lembrar, rede_de_agora, Contexto, CHAVE, TETO_DA_MEMORIA_BPS,
    };
    let diretorio = tempfile::tempdir()?;

    // A memória desta rede, de agora: vale, com teto.
    let banco = diretorio.path().join("seele.db");
    {
        let persistence =
            seele_server::persistence::Persistence::open(&Location::File(banco.clone()))?;
        lembrar(
            &persistence,
            3_000_000,
            &Contexto {
                medida_em: agora(),
                rede: rede_de_agora(),
            },
        )?;
    }
    let (_, servidor) = servidor_com(Location::File(banco)).await?;
    let ao_nascer = servidor.server().subida.lock().await.medida();
    servidor.shutdown();
    servidor.wait_idle().await;
    if rede_de_agora().is_some() {
        assert_eq!(
            ao_nascer,
            Some(3_000_000_u32.min(TETO_DA_MEMORIA_BPS)),
            "o servidor reabriu o mesmo banco e ignorou a medida desta rede"
        );
    } else {
        // Sem rede de casa conhecida, nenhuma memória vale: o lado seguro.
        assert_eq!(ao_nascer, None);
    }

    // O que a 0.16 deixou: o número, sem contexto. Não vale.
    let da_016 = diretorio.path().join("da-0.16.db");
    drop(seele_server::persistence::Persistence::open(
        &Location::File(da_016.clone()),
    )?);
    rusqlite::Connection::open(&da_016)?.execute(
        "INSERT INTO configuracao (chave, valor) VALUES (?1, 90000000)",
        [CHAVE],
    )?;
    let (_, servidor) = servidor_com(Location::File(da_016)).await?;
    assert_eq!(
        servidor.server().subida.lock().await.medida(),
        None,
        "a memória sem contexto da 0.16 virou confiança no caminho de agora"
    );
    servidor.shutdown();
    Ok(())
}
