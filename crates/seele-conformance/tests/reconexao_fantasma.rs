//! O participante fantasma na reconexão, contra um servidor de verdade.
//!
//! **O defeito, relatado no Órbita/Relatórios/Desenvolvimento/
//! 1881a3cb-e333-48a9-9fd4-185153456c71.md:** a fotografia que o servidor manda
//! ao reabrir uma sessão (`PersonPresent` e `PersonJoined`, uma vez para cada
//! sala) é puramente aditiva — ela diz quem está sentado agora, e não remove
//! ninguém. `Room::adopt` zerava ícone, imagens, telas, caminho e perda de
//! subida na reconexão, mas não `seats`, `presentes` nem
//! `current_voice_room`. Quem saiu da sala enquanto esta conexão estava fora
//! do ar ficava sentado no roster dela para sempre — visível na tela, mudo no
//! servidor, que já descarta a mídia dele como `not_a_member`.
//!
//! **A segunda metade do mesmo defeito:** `Room::adopt` (o caminho que a
//! casca roda) e o braço `ServerMessage::Session` de `Room::apply` (o caminho
//! que os testes de conformidade mediam) eram duas cópias que já haviam
//! divergido. Este arquivo agora exercita `adopt` — o caminho de verdade —
//! chamando-o de novo sobre um `Room` que já tem gente sentada, exatamente
//! como a FFI faz no braço de `Aviso::Reconectado`.
//!
//! # Como a queda é simulada
//!
//! Não há gancho de teste para forçar só a conexão de A a cair sem afetar B.
//! O que existe, e já é o padrão usado por
//! `ocupacao.rs::quem_volta_para_o_assento_guardado_reaparece_para_quem_ficou`,
//! é derrubar o `Enlace` sem despedida — o mesmo `drop` que uma queda de rede
//! silenciosa produz do lado do servidor — e reconectar com a mesma chave. O
//! servidor não distingue as duas causas: as duas chegam a ele como a mesma
//! conexão QUIC morrendo sem `CONNECTION_CLOSE`, e o assento fica guardado
//! pela mesma janela de carência que cobre uma queda de bateria de verdade.
//! `bateria_interna.rs` já mede o outro lado dessa moeda — a máquina de
//! estados `Link::InternalBattery` no cliente — e não precisa ser repetido
//! aqui.

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
use seele_server::{Daemon, ServerConfig};

mod vaga;

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

/// Dobra tudo o que já chegou, sem esperar por nada em particular.
async fn absorver(enlace: &mut Enlace, room: &mut Room, por: Duration) {
    let fim = tokio::time::Instant::now() + por;
    while tokio::time::Instant::now() < fim {
        if let Ok(Aviso::Mensagem(mensagem)) =
            tokio::time::timeout(Duration::from_millis(100), enlace.proximo()).await
        {
            room.apply(&mensagem);
        }
    }
}

fn sentados(room: &Room, voice_room: VoiceRoomId) -> Vec<String> {
    let mut nomes: Vec<String> = room
        .roster(voice_room)
        .map(|person| person.nickname.clone())
        .collect();
    nomes.sort();
    nomes
}

#[tokio::test(flavor = "multi_thread")]
async fn quem_saiu_durante_a_queda_do_host_nao_volta_como_fantasma() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, servidor) = server().await?;

    // A é quem cai e reconecta; o roster dele é o que este teste afirma.
    let mut a = conectar(endereco, 91, "a").await?;
    let voice_room = VoiceRoomId(a.sessao().voice_rooms[0].id.get());
    a.entrar_na_voice_room(voice_room, None).await?;

    // O `Room` de A, alimentado como a casca de verdade faz: `adopt` na
    // conexão, e o assento local próprio na hora em que o comando é mandado —
    // o servidor confirma entrada por silêncio, então essa metade é
    // contabilidade da casca (`Room::enter_voice_room`, `state.rs`).
    let mut sala_de_a = Room::new();
    sala_de_a.adopt(a.sessao(), "a");
    sala_de_a.enter_voice_room(voice_room);

    let b = conectar(endereco, 92, "b").await?;
    b.entrar_na_voice_room(voice_room, None).await?;

    // Uma testemunha que nunca cai, só para saber, sem adivinhar, que a saída
    // de B já foi processada pelo servidor antes de A reconectar.
    let mut testemunha = conectar(endereco, 93, "testemunha").await?;
    testemunha.entrar_na_voice_room(voice_room, None).await?;
    let mut sala_da_testemunha = Room::new();
    sala_da_testemunha.adopt(testemunha.sessao(), "testemunha");
    sala_da_testemunha.enter_voice_room(voice_room);

    // A vê B entrar, antes de qualquer coisa cair — do contrário a asserção do
    // fim passaria por um `PersonJoined` que nunca chegou a existir. Por
    // `contains` e não por igualdade de conjunto: a testemunha corre numa
    // conexão independente e pode já ter chegado ao roster de A quando esta
    // asserção é conferida, e isso não é o que este teste mede.
    assert!(
        absorver_ate(&mut a, &mut sala_de_a, Duration::from_secs(5), |sala| {
            sentados(sala, voice_room).contains(&"b".to_owned())
        })
        .await,
        "A não viu a entrada normal de B; roster de A: {:?}",
        sentados(&sala_de_a, voice_room)
    );
    assert!(
        absorver_ate(
            &mut testemunha,
            &mut sala_da_testemunha,
            Duration::from_secs(5),
            |sala| {
                let vistos = sentados(sala, voice_room);
                vistos.contains(&"a".to_owned()) && vistos.contains(&"b".to_owned())
            },
        )
        .await,
        "a testemunha não viu a e b sentados: {:?}",
        sentados(&sala_da_testemunha, voice_room)
    );

    // A cai: a conexão morre sem despedida, do mesmo jeito que uma queda de
    // rede silenciosa produz do lado do servidor — é a técnica que
    // `ocupacao.rs` já usa para simular "fechar o app" / cair. A própria
    // saída de A chega rápido a quem ficou (é o `PersonLeft` de sempre); o que
    // o servidor guarda por uma janela de carência não é a aparência de A no
    // roster dos outros, é a possibilidade de **A** reclamar o mesmo assento
    // ao voltar.
    drop(a);

    // B sai da sala enquanto A está fora do ar.
    b.sair_da_voice_room().await?;

    // A testemunha, que nunca caiu, prova que o servidor já processou a saída
    // de B antes de A voltar — sem essa prova, um A reconectando cedo demais
    // deixaria o teste medir uma corrida, não o defeito. Não se afirma nada
    // sobre A aqui: sua própria saída já pode ter chegado antes desta, e é
    // irrelevante para o que este teste mede.
    assert!(
        absorver_ate(
            &mut testemunha,
            &mut sala_da_testemunha,
            Duration::from_secs(5),
            |sala| !sentados(sala, voice_room).contains(&"b".to_owned()),
        )
        .await,
        "a saída de b não chegou à testemunha; roster da testemunha: {:?}",
        sentados(&sala_da_testemunha, voice_room)
    );

    // A reconecta, com a mesma chave — o servidor reconhece a volta e devolve
    // o assento guardado, exatamente como em
    // `ocupacao.rs::quem_volta_para_o_assento_guardado_reaparece_para_quem_ficou`.
    let mut a2 = conectar(endereco, 91, "a").await?;

    // O braço de `Aviso::Reconectado` na FFI (`seele-ffi/src/lib.rs`) chama só
    // `room.adopt(&sessao, &nickname)` sobre o **mesmo** `Room` que já tinha
    // b sentado. É a chamada que este teste teria que provar sozinha, sem
    // fundir as duas cópias em `state.rs`.
    sala_de_a.adopt(a2.sessao(), "a");

    // O que ainda chegar pelo fio depois da reconexão — não deveria haver
    // `PersonJoined` nenhum para b, porque b já saiu antes de A voltar.
    absorver(&mut a2, &mut sala_de_a, Duration::from_millis(500)).await;

    assert!(
        !sentados(&sala_de_a, voice_room).contains(&"b".to_owned()),
        "b ainda aparece no roster de A depois da reconexão, mas b saiu da \
         sala enquanto A estava fora do ar: {:?}. A fotografia de reabertura \
         é aditiva e não remove ninguém — quem tinha que limpar o assento \
         antigo era a reconexão, antes de aplicar a fotografia nova.",
        sentados(&sala_de_a, voice_room)
    );

    drop(a2);
    drop(b);
    drop(testemunha);
    servidor.shutdown();
    Ok(())
}
