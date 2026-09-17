//! O teto declarado de uma sala de voz e a permissão de entrar nela, contra um
//! servidor de verdade.
//!
//! # Os dois defeitos, e por que são o mesmo defeito
//!
//! `VoiceRoomInfo::limit` atravessa o fio desde sempre, a casca o desenha, e
//! **ninguém o conferia**. `Permission::EnterVoiceRoom` existe, é resolvida no
//! aperto de mão e viaja em `SessionInfo::permissions` — cuja própria
//! documentação diz, com todas as letras, *«Convenience, never enforcement —
//! `specs/08-seguranca.md` puts the security in the server refusing, and it
//! refuses again whatever this list says»*. Para entrar numa sala, o servidor
//! **não** recusava de novo: o ramo `EnterVoiceRoom` conferia a existência da
//! sala e a senha, e sentava.
//!
//! Os dois são a mesma família: um número e uma permissão que o produto anuncia
//! e não cumpre. É a fechadura que se anuncia trancada e não está — a frase que
//! o conserto da senha da sala já usou neste repositório.
//!
//! O [ADR 0038](../../../docs/adr/0038-o-teto-da-sala-e-contado-nao-declarado.md)
//! decidiu que a estimativa de subida **avisa e nunca recusa**, e na mesma
//! decisão escreveu o outro lado: *«o `limit` que quem hospeda escreveu
//! continua sendo o único que barra alguém»*. Este arquivo é esse «único»
//! passando a existir.
//!
//! # Por que «em nenhuma das pontas»
//!
//! O cliente senta a si mesmo ao mandar o pedido, sem esperar confirmação —
//! `specs/02-protocolo.md` faz o servidor confirmar pelo silêncio. Uma recusa
//! que o cliente não entenda como recusa de entrada deixa a pessoa **se vendo
//! dentro** de uma sala onde ninguém a vê: exatamente o sintoma que
//! `senha_da_sala.rs` já documenta. Por isso os dois testes afirmam as duas
//! pontas, e não só a do servidor.

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
use seele_proto::control::{AlertReason, Permission};
use seele_proto::ids::{PersonId, RoleId, VoiceRoomId};
use seele_server::permissions::Permissions;
use seele_server::persistence::{channels::Channels, Location};
use seele_server::{Daemon, ServerConfig};

mod vaga;

async fn servidor() -> Result<(SocketAddr, Arc<Daemon>)> {
    let daemon = Arc::new(
        Daemon::bind(ServerConfig {
            name: "Casa".into(),
            listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            database: Location::Memory,
            ..ServerConfig::default()
        })
        .await?,
    );
    let endereco = daemon.local_addr()?;
    let servindo = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = servindo.run().await;
    });
    Ok((endereco, daemon))
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

fn sala_local(enlace: &Enlace) -> Room {
    let mut room = Room::new();
    room.adopt(enlace.sessao(), "eu");
    room
}

/// Dobra os avisos até a condição valer, ou até o prazo acabar.
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

/// Absorve por um tempo fixo, sem condição de parada.
///
/// Serve para deixar o servidor acabar de falar quando o que se quer afirmar é
/// uma **ausência** — que ninguém sentou. Uma ausência conferida cedo demais é
/// ausência do relógio, não do produto.
async fn absorver(enlace: &mut Enlace, room: &mut Room, por: Duration) {
    let fim = tokio::time::Instant::now() + por;
    while tokio::time::Instant::now() < fim {
        if let Ok(Aviso::Mensagem(mensagem)) =
            tokio::time::timeout(Duration::from_millis(50), enlace.proximo()).await
        {
            room.apply(&mensagem);
        }
    }
}

/// Quantas pessoas o servidor tem sentadas nesta sala.
async fn sentados_no_servidor(daemon: &Daemon, sala: VoiceRoomId) -> usize {
    daemon.server().occupancy.lock().await.quantos(sala)
}

/// Cria uma sala com o teto pedido e devolve o identificador dela.
async fn sala_com_teto(daemon: &Daemon, teto: u16) -> VoiceRoomId {
    let db = daemon.server().persistence.lock().await;
    Channels::new(&db)
        .create_voice_room("Estreita", teto, None)
        .expect("criar a sala estreita")
        .id
}

#[tokio::test(flavor = "multi_thread")]
async fn quem_passa_do_teto_da_sala_e_recusado_e_nao_senta_em_nenhuma_das_pontas() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;
    let estreita = sala_com_teto(&daemon, 1).await;

    // A primeira pessoa cabe, e é o que separa «o teto barra o excedente» de
    // «o teto barra todo mundo». Sem esta metade, um guarda que recusasse
    // sempre passaria no resto do teste e trancaria a sala inteira.
    let mut primeira = conectar(endereco, 60, "primeira").await?;
    let mut sala_da_primeira = sala_local(&primeira);
    primeira.entrar_na_voice_room(estreita, None).await?;
    absorver(
        &mut primeira,
        &mut sala_da_primeira,
        Duration::from_millis(200),
    )
    .await;
    let entrou = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if sentados_no_servidor(&daemon, estreita).await == 1 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    assert!(
        entrou.is_ok(),
        "a primeira pessoa não sentou numa sala de teto 1: o teto passou a \
         barrar quem cabe"
    );

    // A segunda não cabe.
    let mut segunda = conectar(endereco, 61, "segunda").await?;
    let mut sala_da_segunda = sala_local(&segunda);
    segunda.entrar_na_voice_room(estreita, None).await?;

    let recusou = absorver_ate(
        &mut segunda,
        &mut sala_da_segunda,
        Duration::from_secs(5),
        |room| {
            room.notice
                .as_ref()
                .is_some_and(|aviso| aviso.reason == AlertReason::VoiceRoomFull)
        },
    )
    .await;
    assert!(
        recusou,
        "o servidor sentou a segunda pessoa numa sala de teto 1, ou recusou sem \
         dizer que a sala está cheia. O teto que a casca desenha é o número que \
         ninguém confere."
    );

    // Deixa o servidor acabar de falar antes de afirmar a ausência.
    absorver(
        &mut segunda,
        &mut sala_da_segunda,
        Duration::from_millis(400),
    )
    .await;

    assert_eq!(
        sentados_no_servidor(&daemon, estreita).await,
        1,
        "o servidor sentou a segunda pessoa apesar do teto"
    );
    assert_eq!(
        sala_da_segunda.current_voice_room, None,
        "a segunda pessoa continua se vendo dentro da sala em que não coube. O \
         servidor recusou e ela não soube: é a divergência que o conserto da \
         senha da sala já tinha fechado, voltando por um motivo novo."
    );

    daemon.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn quem_nao_tem_a_permissao_e_recusado_pelo_servidor_mesmo_forjando_a_mensagem() -> Result<()>
{
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;

    let mut sem_permissao = conectar(endereco, 62, "sem-permissao").await?;
    let pessoa: PersonId = sem_permissao.sessao().person;
    let sala = VoiceRoomId(sem_permissao.sessao().voice_rooms[0].id.get());

    // Tirados **todos** os papéis, e não só o esperado: um papel a mais numa
    // migração futura devolveria a permissão calada, e este teste passaria a
    // medir outra coisa sem que nada nele mudasse de cor.
    {
        let db = daemon.server().persistence.lock().await;
        let permissoes = Permissions::new(&db);
        for papel in 1..=4 {
            permissoes
                .revoke_role(pessoa, RoleId(papel))
                .expect("revogar o papel");
        }
        assert!(
            !permissoes
                .may(pessoa, Permission::EnterVoiceRoom)
                .expect("consultar a permissão"),
            "a pessoa continua podendo entrar depois de perder todos os papéis: \
             a encenação deste teste não chegou a acontecer"
        );
    }

    // **Forjado de propósito.** `Enlace::entrar_na_voice_room` manda a mensagem
    // sem consultar `SessionInfo::permissions`, que é o que uma casca hostil
    // faria de graça. O que este teste cobra é o servidor recusar de novo, e
    // não a interface esconder o botão.
    let mut sala_local_dela = sala_local(&sem_permissao);
    sem_permissao.entrar_na_voice_room(sala, None).await?;

    let recusou = absorver_ate(
        &mut sem_permissao,
        &mut sala_local_dela,
        Duration::from_secs(5),
        |room| {
            room.notice
                .as_ref()
                .is_some_and(|aviso| aviso.reason == AlertReason::VoiceRoomEntryRefused)
        },
    )
    .await;
    assert!(
        recusou,
        "o servidor aceitou numa sala de voz quem não tem `EnterVoiceRoom`. A \
         permissão é resolvida no aperto de mão, viaja em \
         `SessionInfo::permissions`, e a documentação daquele campo promete que \
         o servidor recusa de novo seja qual for a lista — para entrar numa \
         sala, não recusava."
    );

    absorver(
        &mut sem_permissao,
        &mut sala_local_dela,
        Duration::from_millis(400),
    )
    .await;

    assert_eq!(
        sentados_no_servidor(&daemon, sala).await,
        0,
        "o servidor sentou quem não tem a permissão de entrar"
    );
    assert_eq!(
        sala_local_dela.current_voice_room, None,
        "quem foi recusado continua se vendo dentro da sala"
    );

    daemon.shutdown();
    Ok(())
}
