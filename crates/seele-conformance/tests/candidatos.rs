//! An invite with several addresses, against a server that only answers at one.
//!
//! ADR 0006 grew a list of candidate addresses after a field failure: a Windows
//! host with a VPN announced an address nobody on its own network could reach,
//! and 0.5.0 had also stopped putting the local address in the invite at all.
//! One address is never enough — the one that works from outside is not the one
//! that works from inside, and vice versa.
//!
//! # What only a live server can prove
//!
//! The ordering and the sentence are decisions over values, and they are tested
//! where they are decided (`seele_server::alcance`, `seele_proto::uri`). What no
//! unit test reaches is the wiring: that `Enlace::conectar_entre` really moves
//! on to the next candidate after one fails, that it does so within the
//! deadline, and that the session it hands back is a session that **works**.
//! Each of those can break with every unit test still green.
//!
//! # Why the dead address is a closed port on loopback
//!
//! It has to fail the way a wrong address fails in the field, and it has to
//! fail on this machine, in CI, with no network. A closed UDP port on
//! `127.0.0.1` is the closest honest stand-in: nothing answers the handshake,
//! exactly as when an invite carries the LAN address of a house the client is
//! not in.

#![allow(clippy::expect_used, reason = "num teste, o pânico é o relatório")]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use seele_core::enlace::{Aviso, Destino, Enlace};
use seele_core::{MemoryPinStore, PinStore};
use seele_proto::control::ServerMessage;
use seele_proto::ids::{VoiceRoomId, ClientMessageId, ChannelId};
use seele_server::persistence::Location;
use seele_server::{ServerConfig, Daemon};

const VOICE_ROOM: u32 = 1;
const LINE: u32 = 1;

/// Starts a server on a port the system picks.
async fn server() -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Terceira Tóquio".into(),
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

/// An address where nothing is listening.
///
/// Bound and released, so the port is real and free: a number picked by hand
/// could be somebody else's server on the machine running the tests.
fn endereco_morto() -> SocketAddr {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind");
    let endereco = socket.local_addr().expect("local_addr");
    drop(socket);
    endereco
}

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

/// Proves the session serves, and not merely that the constructor said `Ok`.
async fn falar_e_ouvir(enlace: &mut Enlace, o_que: &str) -> Result<()> {
    enlace.inserir_plug(VoiceRoomId(VOICE_ROOM)).await?;
    enlace.abrir_linha(ChannelId(LINE)).await?;
    enlace
        .dizer(ChannelId(LINE), o_que.to_owned(), ClientMessageId(1))
        .await?;

    let fim = Instant::now() + Duration::from_secs(15);
    while Instant::now() < fim {
        if let Ok(Aviso::Mensagem(mensagem)) =
            tokio::time::timeout(Duration::from_millis(300), enlace.proximo()).await
        {
            if matches!(&*mensagem, ServerMessage::MessageReceived { body, .. } if body == o_que) {
                return Ok(());
            }
        }
    }
    anyhow::bail!("a sessão não devolveu o que foi dito nela")
}

#[tokio::test]
async fn a_conexao_cai_para_o_proximo_endereco_do_convite() -> Result<()> {
    // The field case, end to end: the invite's first address is the one the
    // host's own network uses, and the person opening the link is not on that
    // network. Before the candidate list this was the whole story — one address,
    // no second try, "it doesn't connect".
    let (vivo, _servidor) = server().await?;
    let pins = Arc::new(MemoryPinStore::default());

    let comeco = Instant::now();
    let mut enlace = Enlace::conectar_entre(
        vec![destino(endereco_morto(), "pessoa"), destino(vivo, "pessoa")],
        ed25519_dalek::SigningKey::from_bytes(&[7; 32]),
        Arc::clone(&pins) as Arc<dyn PinStore>,
    )
    .await
    .expect("o segundo endereço do convite tinha um servidor atendendo, e ninguém foi lá");
    let levou = comeco.elapsed();

    falar_e_ouvir(&mut enlace, "padrão azul").await?;

    // The pin is filed under the address that actually answered, and not under
    // the one that was tried first: two servers are told apart by where they
    // answer, and filing the dead address would hand the next visitor a key
    // belonging to nobody.
    assert!(
        pins.pinned(&vivo.to_string()).is_some(),
        "ninguém fixou a chave do servidor que atendeu"
    );
    assert!(
        pins.pinned(&endereco_morto().to_string()).is_none(),
        "sobrou pin de um endereço que nunca respondeu"
    );

    // The deadline is the product: a dead candidate cannot hold the queue for
    // longer than the person is willing to wait for a room to open.
    assert!(
        levou < Duration::from_secs(20),
        "o candidato morto segurou a fila por {levou:?}"
    );
    Ok(())
}

#[tokio::test]
async fn um_convite_de_um_endereco_so_continua_conectando_como_antes() -> Result<()> {
    // Compatibility, and it is not decorative: an invite written before ADR 0006
    // grew the list arrives as a one-item list, and it must take exactly the old
    // path — no new deadline, no extra attempt.
    let (vivo, _servidor) = server().await?;
    let pins = Arc::new(MemoryPinStore::default());

    let mut enlace = Enlace::conectar_entre(
        vec![destino(vivo, "pessoa")],
        ed25519_dalek::SigningKey::from_bytes(&[9; 32]),
        Arc::clone(&pins) as Arc<dyn PinStore>,
    )
    .await
    .expect("um convite de um endereço só deixou de conectar");

    falar_e_ouvir(&mut enlace, "padrão laranja").await?;
    Ok(())
}

#[tokio::test]
async fn nenhum_endereco_atendendo_falha_e_nao_trava() -> Result<()> {
    // Every candidate dead. What matters is that it comes back at all: a loop
    // that waits on each address without a deadline would leave whoever pasted
    // the link staring at a screen that never changes — the exact silence
    // ADR 0022 exists to remove.
    let comeco = Instant::now();
    let erro = Enlace::conectar_entre(
        vec![
            destino(endereco_morto(), "pessoa"),
            destino(endereco_morto(), "pessoa"),
            destino(endereco_morto(), "pessoa"),
        ],
        ed25519_dalek::SigningKey::from_bytes(&[11; 32]),
        Arc::new(MemoryPinStore::default()) as Arc<dyn PinStore>,
    )
    .await;
    let Err(erro) = erro else {
        panic!("conectou em três endereços onde não há servidor nenhum");
    };
    let levou = comeco.elapsed();

    eprintln!("três candidatos mortos levaram {levou:?} e devolveram {erro}");
    assert!(
        levou < Duration::from_secs(30),
        "três candidatos mortos seguraram a tela por {levou:?}"
    );
    Ok(())
}
