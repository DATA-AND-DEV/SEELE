//! A senha de uma sala de voz, contra um servidor de verdade.
//!
//! Dois defeitos somados produziam o mesmo sintoma relatado: a pessoa se vê
//! dentro da sala, o anfitrião não a vê e não a ouve.
//!
//! 1. `Client::enter_voice_room` mandava `password: None` no fio, sem sequer
//!    ter o parâmetro na assinatura — nenhuma sala com senha era entrável.
//! 2. O cliente senta a si mesmo por conta própria ao mandar o pedido, sem
//!    esperar confirmação (`specs/02-protocolo.md`: o servidor confirma pelo
//!    silêncio). Quando a senha está errada, o servidor recusa em
//!    `admissao::voice_room_liberado`, manda `Alert
//!    VoiceRoomEntryRefused` e nunca senta ninguém — mas o cliente já tinha
//!    se sentado sozinho, e ficava divergente do servidor para sempre.
//!
//! Este arquivo prova as duas coisas com um servidor real: a senha chega
//! (entrada com a senha certa senta nas duas pontas) e a recusa desfaz o
//! assento especulativo (entrada sem a senha certa não deixa o cliente
//! sentado nele mesmo).

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use seele_core::enlace::{Aviso, Destino, Enlace};
use seele_core::{MemoryPinStore, PinStore, Room};
use seele_proto::ids::VoiceRoomId;
use seele_server::persistence::Location;
use seele_server::{admissao, Daemon, ServerConfig};

mod vaga;

const SENHA_CERTA: &str = "coelho-branco-42";

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

async fn conectar(endereco: SocketAddr, semente: u8, apelido: &str) -> Result<Enlace> {
    Ok(Enlace::conectar(
        destino(endereco, apelido),
        ed25519_dalek::SigningKey::from_bytes(&[semente; 32]),
        Arc::new(MemoryPinStore::new()) as Arc<dyn PinStore>,
    )
    .await?)
}

fn sala(enlace: &Enlace) -> Room {
    let mut room = Room::new();
    room.adopt(enlace.sessao(), "eu");
    room
}

/// Dobra até a condição valer, ou até o prazo acabar.
async fn absorver_ate<F>(enlace: &mut Enlace, room: &mut Room, prazo: Duration, pronto: F) -> bool
where
    F: Fn(&Room) -> bool,
{
    let fim = tokio::time::Instant::now() + prazo;
    while tokio::time::Instant::now() < fim {
        if pronto(room) {
            return true;
        }
        if let Ok(Aviso::Mensagem(mensagem)) =
            tokio::time::timeout(Duration::from_millis(100), enlace.proximo()).await
        {
            room.apply(&mensagem);
        }
    }
    pronto(room)
}

/// Tranca a sala padrão do servidor com [`SENHA_CERTA`].
async fn trancar_sala_padrao(servidor: &Daemon, voice_room: VoiceRoomId) {
    let mut persistence = servidor.server().persistence.lock().await;
    admissao::definir_senha_voice_room(&mut persistence, voice_room, Some(SENHA_CERTA))
        .expect("gravar a senha da sala");
}

#[tokio::test(flavor = "multi_thread")]
async fn entrar_com_senha_errada_nao_deixa_o_cliente_sentado_nele_mesmo() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor) = server().await?;
    let anfitriao = conectar(endereco, 46, "anfitriao").await?;
    let voice_room = VoiceRoomId(anfitriao.sessao().voice_rooms[0].id.get());
    trancar_sala_padrao(&servidor, voice_room).await;

    let mut visita = conectar(endereco, 47, "visita").await?;
    let mut sala_visita = sala(&visita);

    // Pede a entrada sem senha, e senta a si mesmo por conta própria — é o
    // que a casca de verdade faz em `seele-ffi`, sem esperar confirmação: o
    // protocolo confirma pelo silêncio, e um alerta é a única coisa que
    // desfaz o que já foi sentado.
    visita
        .entrar_na_voice_room(voice_room, None)
        .await
        .expect("sessão acabou");
    sala_visita.enter_voice_room(voice_room);
    assert_eq!(
        sala_visita.current_voice_room,
        Some(voice_room),
        "sanidade: o assento especulativo tem de existir antes da recusa chegar"
    );

    let desfez = absorver_ate(
        &mut visita,
        &mut sala_visita,
        Duration::from_secs(15),
        |room| room.current_voice_room.is_none(),
    )
    .await;

    assert!(
        desfez,
        "a recusa do servidor chegou e o cliente continua se desenhando sentado \
         na sala: current_voice_room = {:?}",
        sala_visita.current_voice_room
    );
    assert!(
        sala_visita
            .seats
            .get(&voice_room)
            .is_none_or(|seats| seats.is_empty()),
        "a própria pessoa continua no assento da sala mesmo com a entrada recusada: {:?}",
        sala_visita.seats.get(&voice_room)
    );

    drop(anfitriao);
    drop(visita);
    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn entrar_com_a_senha_certa_senta_de_verdade_nas_duas_pontas() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor) = server().await?;
    let mut anfitriao = conectar(endereco, 46, "anfitriao").await?;
    let voice_room = VoiceRoomId(anfitriao.sessao().voice_rooms[0].id.get());
    trancar_sala_padrao(&servidor, voice_room).await;

    let mut sala_anfitriao = sala(&anfitriao);
    anfitriao
        .entrar_na_voice_room(voice_room, Some(SENHA_CERTA.to_owned()))
        .await
        .expect("sessão acabou");
    sala_anfitriao.enter_voice_room(voice_room);

    let visita = conectar(endereco, 47, "visita").await?;
    let mut sala_visita = sala(&visita);
    visita
        .entrar_na_voice_room(voice_room, Some(SENHA_CERTA.to_owned()))
        .await
        .expect("sessão acabou");
    sala_visita.enter_voice_room(voice_room);

    // O lado do cliente que entrou: continua sentado, porque a senha estava
    // certa e nenhuma recusa chega para desfazer o assento.
    assert_eq!(sala_visita.current_voice_room, Some(voice_room));

    // O lado do anfitrião: precisa realmente ver quem entrou, e não só supor
    // que a entrada deu certo porque `entrar_na_voice_room` devolveu `Ok`. É
    // exatamente o sintoma relatado — a pessoa se vê na sala e o anfitrião
    // não a vê — e aqui o anfitrião tem de vê-la.
    let viu_a_visita = absorver_ate(
        &mut anfitriao,
        &mut sala_anfitriao,
        Duration::from_secs(15),
        |room| {
            room.roster(voice_room)
                .any(|pessoa| pessoa.nickname == "visita")
        },
    )
    .await;

    assert!(
        viu_a_visita,
        "a visita entrou com a senha certa e o anfitrião não a vê na sala: {:?}",
        sala_anfitriao
            .roster(voice_room)
            .map(|p| p.nickname.clone())
            .collect::<Vec<_>>()
    );

    drop(anfitriao);
    drop(visita);
    servidor.shutdown();
    Ok(())
}
