//! A ocupação das salas de voz, contra um servidor de verdade.
//!
//! O defeito, relatado de uma sessão real: «o sistema de voice_rooms não está bem
//! implementado, mostra que as salas de voz estão vazias quando não deveriam estar».
//!
//! # Por que nenhum teste de unidade via isso
//!
//! Os dois lados estavam certos sozinhos. `seele_core::state::Room` sabe assentar
//! um pessoa em qualquer voice room e sempre soube — os testes dele passam entregando
//! `PersonJoined` à mão, para a sala de voz que o teste quiser. O servidor sabe manter a
//! ocupação e sempre soube. O que faltava era o **fio entre os dois**: o
//! `translate` da sessão descartava todo `PersonJoined` que não fosse da sala de voz em
//! que aquela conexão estava sentada, então o `seats` do cliente só ganhava
//! entrada para uma sala de voz e a tela desenhava os outros vazios para sempre.
//!
//! Uma falha de junção, com cada peça correta. Só um `Enlace` de verdade contra
//! um `Daemon` de verdade a enxerga, e é por isso que este arquivo existe aqui e
//! não em `seele-core`.
//!
//! # As três coisas medidas
//!
//! 1. **Quem já estava sentado aparece para quem chega.** Não só na sala de voz em que
//!    a pessoa entra: em todos.
//! 2. **Quem entra depois aparece para quem está noutro sala de voz.** É a metade que
//!    fica viva; sem ela a tela está certa ao abrir e erra um minuto depois.
//! 3. **Quem some é retirado.** A saída só era anunciada pelo ramo do
//!    `EjetarPlug`; uma conexão que caía deixava o pessoa no roster de todo
//!    mundo. Invisível enquanto o cliente desenhava uma sala de voz só, e um fantasma na
//!    tela agora que ele desenha todos.

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
use seele_proto::ids::{VoiceRoomId, LineId};
use seele_server::persistence::Location;
use seele_server::{ServerConfig, Daemon};

/// Sobe um servidor numa porta que o sistema escolhe, com dois salas de voz.
///
/// Dois, e esse é o ponto do arquivo inteiro: com um só, o filtro que este
/// teste existe para pegar não filtra nada e tudo passa.
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

async fn conectar(endereco: SocketAddr, semente: u8, apelido: &str) -> Result<Enlace> {
    Ok(Enlace::conectar(
        destino(endereco, apelido),
        ed25519_dalek::SigningKey::from_bytes(&[semente; 32]),
        Arc::new(MemoryPinStore::new()) as Arc<dyn PinStore>,
    )
    .await?)
}

/// Um `Room` alimentado pelo enlace, como toda casca faz.
///
/// Este é o valor que a tela lê: `seele_ffi::voice_rooms_of` percorre `room.voice_rooms` e
/// chama `room.roster(voice_room.id)` para cada um. Medir aqui é medir o que a pessoa
/// vê, e não o que o fio carregou.
fn sala(enlace: &Enlace) -> Room {
    let mut room = Room::new();
    room.adopt(enlace.sessao(), "eu");
    room
}

/// Dobra tudo o que já chegou no `Room`, sem esperar por nada em particular.
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

/// Dobra até a condição valer, ou até o prazo acabar.
///
/// Com prazo em vez de uma espera fixa: a asserção positiva não deve depender de
/// quanto a máquina demorou, e a negativa está sempre depois de uma positiva que
/// já provou que o servidor teve tempo de falar.
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

/// Os apelidos sentados num sala de voz, como a tela os desenharia.
fn sentados(room: &Room, voice_room: VoiceRoomId) -> Vec<String> {
    let mut nomes: Vec<String> = room
        .roster(voice_room)
        .map(|person| person.nickname.clone())
        .collect();
    nomes.sort();
    nomes
}

/// Abre um segundo sala de voz, que só quem hospeda pode fazer.
async fn segundo_voice_room(anfitriao: &Enlace, enlace: &mut Enlace, room: &mut Room) -> Result<VoiceRoomId> {
    anfitriao
        .criar_voice_room("VOICE_ROOM-02 SALA DOS FUNDOS".to_owned(), 8, None)
        .await
        .expect("a sessão do anfitrião acabou");

    let chegou = absorver_ate(enlace, room, Duration::from_secs(15), |room| {
        room.voice_rooms.len() >= 2
    })
    .await;
    assert!(chegou, "o segundo sala de voz não chegou a quem estava conectado");
    Ok(room.voice_rooms[1].id)
}

#[tokio::test(flavor = "multi_thread")]
async fn quem_chega_ve_todos_os_voice_rooms_ocupados_e_nao_so_o_seu() -> Result<()> {
    let (endereco, servidor) = server().await?;

    let anfitriao = conectar(endereco, 46, "anfitriao").await?;
    let mut asuka = conectar(endereco, 48, "asuka").await?;
    let mut sala_asuka = sala(&asuka);
    let voice_room_um = VoiceRoomId(anfitriao.sessao().voice_rooms[0].id.get());
    let voice_room_dois = segundo_voice_room(&anfitriao, &mut asuka, &mut sala_asuka).await?;

    // Duas pessoas sentam, uma em cada sala de voz, **antes** de a testemunha existir.
    let mut shinji = conectar(endereco, 47, "shinji").await?;
    shinji.inserir_plug(voice_room_um).await.expect("sessão acabou");
    asuka.inserir_plug(voice_room_dois).await.expect("sessão acabou");
    // Um instante para os dois assentos existirem no servidor antes do aperto de
    // mão seguinte, que é o que este teste mede.
    let mut sala_shinji = sala(&shinji);
    sala_shinji.enter_voice_room(voice_room_um);
    absorver(&mut shinji, &mut sala_shinji, Duration::from_secs(1)).await;

    // A testemunha chega agora, e não pediu nada a ninguém.
    let mut rei = conectar(endereco, 49, "rei").await?;
    let mut sala_rei = sala(&rei);
    let viu_os_dois = absorver_ate(&mut rei, &mut sala_rei, Duration::from_secs(15), |room| {
        room.roster(voice_room_um).count() == 1 && room.roster(voice_room_dois).count() == 1
    })
    .await;

    assert!(
        viu_os_dois,
        "quem chegou viu {:?} no VOICE_ROOM-01 e {:?} no VOICE_ROOM-02; \
         os dois estão ocupados, e uma sala desenhada vazia é o defeito relatado",
        sentados(&sala_rei, voice_room_um),
        sentados(&sala_rei, voice_room_dois),
    );
    assert_eq!(sentados(&sala_rei, voice_room_um), ["shinji"]);
    assert_eq!(sentados(&sala_rei, voice_room_dois), ["asuka"]);

    drop(anfitriao);
    drop(asuka);
    drop(shinji);
    drop(rei);
    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn entrar_num_voice_room_aparece_para_quem_esta_noutro() -> Result<()> {
    let (endereco, servidor) = server().await?;

    let anfitriao = conectar(endereco, 46, "anfitriao").await?;
    let mut rei = conectar(endereco, 49, "rei").await?;
    let mut sala_rei = sala(&rei);
    let voice_room_um = VoiceRoomId(anfitriao.sessao().voice_rooms[0].id.get());
    let voice_room_dois = segundo_voice_room(&anfitriao, &mut rei, &mut sala_rei).await?;

    // A testemunha senta no primeiro voice room e fica lá o teste inteiro.
    rei.inserir_plug(voice_room_um).await.expect("sessão acabou");
    sala_rei.enter_voice_room(voice_room_um);

    // Alguém entra **no outro**. Esta é a metade viva: um retrato pedido no
    // aperto de mão estaria certo até aqui e erraria a partir daqui.
    let shinji = conectar(endereco, 47, "shinji").await?;
    shinji.inserir_plug(voice_room_dois).await.expect("sessão acabou");

    let apareceu = absorver_ate(&mut rei, &mut sala_rei, Duration::from_secs(15), |room| {
        room.roster(voice_room_dois).count() == 1
    })
    .await;
    assert!(
        apareceu,
        "quem entrou na sala de voz-02 não apareceu para quem está na sala de voz-01: {:?}",
        sentados(&sala_rei, voice_room_dois)
    );
    assert_eq!(sentados(&sala_rei, voice_room_dois), ["shinji"]);

    // E sair do outro sala de voz também chega. Sem isto a tela só cresce.
    shinji.ejetar_plug().await.expect("sessão acabou");
    let sumiu = absorver_ate(&mut rei, &mut sala_rei, Duration::from_secs(15), |room| {
        room.roster(voice_room_dois).count() == 0
    })
    .await;
    assert!(
        sumiu,
        "quem ejetou o plug na sala de voz-02 continua desenhado lá: {:?}",
        sentados(&sala_rei, voice_room_dois)
    );

    drop(anfitriao);
    drop(rei);
    drop(shinji);
    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn uma_conexao_que_cai_sai_do_roster_de_todo_mundo() -> Result<()> {
    let (endereco, servidor) = server().await?;

    let anfitriao = conectar(endereco, 46, "anfitriao").await?;
    let mut rei = conectar(endereco, 49, "rei").await?;
    let mut sala_rei = sala(&rei);
    let voice_room_um = VoiceRoomId(anfitriao.sessao().voice_rooms[0].id.get());

    let shinji = conectar(endereco, 47, "shinji").await?;
    shinji.inserir_plug(voice_room_um).await.expect("sessão acabou");
    // Mais um comando atrás do primeiro, para que a entrada tenha sido
    // processada antes de a sessão morrer; sem isso o teste poderia medir uma
    // saída que nunca teve entrada e passar por engano.
    let _ = shinji.abrir_linha(LineId(1)).await;

    let entrou = absorver_ate(&mut rei, &mut sala_rei, Duration::from_secs(15), |room| {
        room.roster(voice_room_um).count() == 1
    })
    .await;
    assert!(entrou, "a entrada não chegou; a saída não mede nada");

    // Solta a alça: é o que uma janela fechada faz.
    drop(shinji);

    let saiu = absorver_ate(&mut rei, &mut sala_rei, Duration::from_secs(30), |room| {
        room.roster(voice_room_um).count() == 0
    })
    .await;
    assert!(
        saiu,
        "quem fechou o cliente ficou sentado no VOICE_ROOM-01 para todo mundo: {:?}\n\
         A saída só era anunciada pelo ramo do EjetarPlug, então toda queda \
         deixava um fantasma na tela.",
        sentados(&sala_rei, voice_room_um)
    );

    drop(anfitriao);
    drop(rei);
    servidor.shutdown();
    Ok(())
}
