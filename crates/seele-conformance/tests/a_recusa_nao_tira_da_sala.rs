//! Uma recusa do servidor não tira da sala quem o servidor manteve sentado.
//!
//! # O defeito
//!
//! O servidor manda `Alert { VoiceRoomEntryRefused | VoiceRoomFull }` também
//! quando **mantém** a pessoa onde estava, e o cliente lia todo alerta desses
//! como «a entrada que você acabou de pedir não valeu: saia da sala». Medido na
//! revisão do lote I-a da F5, em três casos, com o servidor tendo a pessoa em A
//! e os outros vendo-a em A:
//!
//! - **M1**: sentada em A, pede B com a senha errada;
//! - **M3**: cai e volta numa sala com senha. O servidor resgata o assento (o
//!   mesmo `ssrc`), e o motor refaz a entrada sem a senha, que é recusada;
//! - **M5**: sentada em A, escolhe um apelido que já é de outra pessoa. O
//!   servidor recusa com o mesmo motivo (`nao_deu`, em `seele-server`).
//!
//! Antes da pendência 52 isso só fazia a tela mentir. Com ela, o áudio segue o
//! assento, e a pessoa ficava surda e muda numa sala em que todos a viam — o
//! M3 a cada queda de rede numa sala com senha.
//!
//! # A regra
//!
//! Uma recusa desfaz só o pedido de entrada **pendente**, e devolve o assento de
//! antes dele. Sem pedido pendente, ela não move ninguém. O pedido deixa de estar
//! pendente quando volta o `Pong` do `Ping` marcado que o motor manda logo atrás
//! dele: o servidor lê o fluxo de controle em ordem, e se nenhuma recusa chegou
//! antes daquele `Pong`, a entrada valeu.
//!
//! # O que estes testes medem
//!
//! M1 e M5 pela FFI, como o app entra: o assento que o retrato desenha e o
//! estado da abertura do áudio. A sessão entra sem áudio (não há placa de som
//! numa máquina de integração), e então, numa sala, o estado é `SemAparelho`, e
//! fora dela `ForaDeSala`: é o estado que diz se o gatilho do áudio acha que a
//! pessoa está numa sala.
//!
//! O M3 é emulado com dois `Enlace` e o `Room` dobrado à mão, do jeito que a FFI
//! dobra: o motor de verdade só volta depois de três `Ping` sem resposta, e um
//! corte que o faça voltar leva mais de vinte segundos (o M4 da revisão levou
//! 26 s). A emulação refaz o que a volta faz: a mesma chave, o `ssrc` resgatado,
//! a sessão adotada e a entrada refeita sem senha.
//!
//! O de controle é o caso que a recusa sempre teve de desfazer: quem está fora
//! pede uma sala com a senha errada, e continua fora.

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
use seele_ffi::{AberturaDoAudio, ConnectConfig, Connection, NoticeReason};
use seele_proto::control::AlertReason;
use seele_proto::ids::{PersonId, VoiceRoomId};
use seele_server::persistence::{channels::Channels, Location};
use seele_server::{admissao, Daemon, ServerConfig};

mod vaga;

const SENHA: &str = "coelho-branco-42";

/// Quanto cada passo de rede tem para acontecer.
const PRAZO: Duration = Duration::from_secs(15);

/// Quanto se espera depois da recusa, antes de afirmar onde a pessoa ficou.
///
/// Uma ausência conferida cedo demais é ausência do relógio: o fim da volta do
/// laço da FFI, que é onde o gatilho do áudio olha o assento, vem depois da
/// mensagem.
const ASSENTAR: Duration = Duration::from_millis(500);

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

/// Uma sala nova, criada antes de quem entra conectar: a lista da sessão a traz.
async fn criar_sala(daemon: &Daemon, nome: &str) -> VoiceRoomId {
    let db = daemon.server().persistence.lock().await;
    Channels::new(&db)
        .create_voice_room(nome, 0, None)
        .expect("criar a sala")
        .id
}

async fn trancar(daemon: &Daemon, sala: VoiceRoomId) {
    let mut db = daemon.server().persistence.lock().await;
    admissao::definir_senha_voice_room(&mut db, sala, Some(SENHA)).expect("trancar a sala");
}

/// Onde o servidor tem esta pessoa sentada.
async fn no_servidor(daemon: &Daemon, quem: PersonId) -> Option<VoiceRoomId> {
    daemon.server().occupancy.lock().await.onde_esta(quem)
}

/// Espera a condição valer, olhando de pouco em pouco, até o prazo.
async fn esperar<F>(prazo: Duration, mut pronto: F) -> bool
where
    F: FnMut() -> bool,
{
    let fim = tokio::time::Instant::now() + prazo;
    while tokio::time::Instant::now() < fim {
        if pronto() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    pronto()
}

// ---- pela FFI

fn config(casa: &std::path::Path, endereco: SocketAddr, apelido: &str) -> ConnectConfig {
    ConnectConfig {
        server: endereco.to_string(),
        alternate_servers: Vec::new(),
        nickname: apelido.into(),
        home: casa.display().to_string(),
        join_secret: None,
        expected_fingerprint: None,
        bilhete: None,
        audio: false,
        capture_device: None,
        playback_device: None,
    }
}

async fn entrar_pela_ffi(config: ConnectConfig) -> Arc<Connection> {
    let entrou = tokio::time::timeout(
        PRAZO,
        tokio::task::spawn_blocking(move || Connection::connect(config)),
    )
    .await
    .expect("a FFI não entrou dentro do prazo")
    .expect("a thread que conecta caiu");
    match entrou {
        Ok((conexao, _)) => conexao,
        Err(erro) => panic!("a FFI não entrou: {erro:?}"),
    }
}

/// A sala que o retrato da FFI desenha como a desta pessoa.
fn sala_na_tela(conexao: &Connection) -> Option<u32> {
    conexao
        .snapshot()
        .voice_rooms
        .iter()
        .find(|sala| sala.occupied_by_us)
        .map(|sala| sala.id)
}

fn abertura(conexao: &Connection) -> AberturaDoAudio {
    conexao.snapshot().abertura_do_audio
}

fn recusada(conexao: &Connection) -> bool {
    conexao
        .snapshot()
        .notice
        .is_some_and(|aviso| aviso.reason == NoticeReason::VoiceRoomEntryRefused)
}

fn quem_e(conexao: &Connection) -> PersonId {
    PersonId(conexao.snapshot().me.expect("a sessão sabe quem é"))
}

/// Entra numa sala pela FFI e espera as três pontas concordarem: o servidor a
/// sentou, a tela a desenha lá, e o gatilho do áudio a dá por dentro.
async fn sentar_pela_ffi(conexao: &Connection, daemon: &Daemon, sala: VoiceRoomId) {
    conexao
        .enter_voice_room(sala.get(), None)
        .expect("a sessão acabou");
    let quem = quem_e(conexao);
    let fim = tokio::time::Instant::now() + PRAZO;
    loop {
        let no_servidor_agora = no_servidor(daemon, quem).await;
        if no_servidor_agora == Some(sala)
            && sala_na_tela(conexao) == Some(sala.get())
            && abertura(conexao) == AberturaDoAudio::SemAparelho
        {
            return;
        }
        assert!(
            tokio::time::Instant::now() < fim,
            "a entrada em {sala:?} não assentou: servidor {no_servidor_agora:?}, tela {:?}, \
             abertura {:?}",
            sala_na_tela(conexao),
            abertura(conexao)
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Depois da recusa: o servidor ainda a tem em `sala`, e a FFI também.
async fn continua_em(conexao: &Connection, daemon: &Daemon, sala: VoiceRoomId, caso: &str) {
    assert!(
        esperar(PRAZO, || recusada(conexao)).await,
        "{caso}: a recusa não chegou, e o teste mediu outra coisa"
    );
    tokio::time::sleep(ASSENTAR).await;
    assert_eq!(
        no_servidor(daemon, quem_e(conexao)).await,
        Some(sala),
        "{caso}: o servidor não manteve a pessoa sentada, e o teste mediu outra coisa"
    );
    assert_eq!(
        sala_na_tela(conexao),
        Some(sala.get()),
        "{caso}: o servidor manteve a pessoa em {sala:?} e a tela dela a tirou da sala: \
         todos a veem lá, e ela se vê fora"
    );
    assert_eq!(
        abertura(conexao),
        AberturaDoAudio::SemAparelho,
        "{caso}: o servidor manteve a pessoa em {sala:?} e o gatilho do áudio a deu por fora \
         da sala: surda e muda numa sala em que todos a veem"
    );
}

/// **M1**: sentada em A, pede B com a senha errada, e continua em A, com voz.
#[tokio::test(flavor = "multi_thread")]
async fn quem_e_recusado_noutra_sala_continua_na_sua_e_com_o_audio() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;
    let trancada = criar_sala(&daemon, "Trancada").await;
    trancar(&daemon, trancada).await;
    let casa = tempfile::tempdir()?;
    let p = entrar_pela_ffi(config(casa.path(), endereco, "p")).await;
    let a = VoiceRoomId(p.snapshot().voice_rooms[0].id);
    assert_ne!(a, trancada, "a sala padrão e a trancada são a mesma");

    sentar_pela_ffi(&p, &daemon, a).await;
    p.enter_voice_room(trancada.get(), Some("errada".into()))
        .expect("a sessão acabou");
    continua_em(&p, &daemon, a, "M1").await;

    p.disconnect();
    daemon.shutdown();
    Ok(())
}

/// **M5**: sentada em A, pede um apelido que já é de outra pessoa, e continua
/// em A, com voz. A recusa é do apelido, e chega com o motivo de entrada.
#[tokio::test(flavor = "multi_thread")]
async fn um_apelido_tomado_nao_tira_ninguem_da_sala() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;
    let _lia = Enlace::conectar(
        destino(endereco, "lia"),
        ed25519_dalek::SigningKey::from_bytes(&[72; 32]),
        Arc::new(MemoryPinStore::new()) as Arc<dyn PinStore>,
    )
    .await?;
    let casa = tempfile::tempdir()?;
    let p = entrar_pela_ffi(config(casa.path(), endereco, "p")).await;
    let a = VoiceRoomId(p.snapshot().voice_rooms[0].id);

    sentar_pela_ffi(&p, &daemon, a).await;
    p.set_nickname("lia".into()).expect("a sessão acabou");
    continua_em(&p, &daemon, a, "M5").await;

    p.disconnect();
    daemon.shutdown();
    Ok(())
}

/// **O de controle**: quem está fora pede uma sala com a senha errada, e
/// continua fora, sem áudio aberto.
///
/// É o caso que a recusa sempre teve de desfazer, e ele fica verde antes e
/// depois da regra nova: o que ele prova é que ela não desmontou o conserto
/// antigo (`senha_da_sala.rs`).
#[tokio::test(flavor = "multi_thread")]
async fn quem_esta_fora_e_e_recusado_continua_fora() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;
    let trancada = criar_sala(&daemon, "Trancada").await;
    trancar(&daemon, trancada).await;
    let casa = tempfile::tempdir()?;
    let p = entrar_pela_ffi(config(casa.path(), endereco, "p")).await;

    p.enter_voice_room(trancada.get(), Some("errada".into()))
        .expect("a sessão acabou");
    assert!(
        esperar(PRAZO, || recusada(&p)).await,
        "a recusa não chegou, e o teste mediu outra coisa"
    );
    tokio::time::sleep(ASSENTAR).await;

    assert_eq!(no_servidor(&daemon, quem_e(&p)).await, None);
    assert_eq!(
        sala_na_tela(&p),
        None,
        "quem foi recusado ao entrar se vê dentro da sala que o recusou"
    );
    assert_eq!(
        abertura(&p),
        AberturaDoAudio::ForaDeSala,
        "quem foi recusado ao entrar ficou com o áudio de quem está numa sala"
    );

    p.disconnect();
    daemon.shutdown();
    Ok(())
}

// ---- a volta, emulada

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

async fn conectar(endereco: SocketAddr, semente: u8, apelido: &str) -> Result<Enlace> {
    Ok(Enlace::conectar(
        destino(endereco, apelido),
        ed25519_dalek::SigningKey::from_bytes(&[semente; 32]),
        Arc::new(MemoryPinStore::new()) as Arc<dyn PinStore>,
    )
    .await?)
}

/// Dobra no `Room` o que o enlace ouve, por um tempo fixo, como a FFI dobra.
async fn dobrar(enlace: &mut Enlace, room: &mut Room, por: Duration) {
    let fim = tokio::time::Instant::now() + por;
    while tokio::time::Instant::now() < fim {
        if let Ok(Aviso::Mensagem(mensagem)) =
            tokio::time::timeout(Duration::from_millis(20), enlace.proximo()).await
        {
            room.apply(&mensagem);
        }
    }
}

/// **M3**: quem cai e volta numa sala com senha é sentado de novo pelo
/// servidor, e a entrada que a volta refaz sem a senha é recusada. A pessoa
/// continua na sala.
#[tokio::test(flavor = "multi_thread")]
async fn a_volta_numa_sala_com_senha_nao_tira_quem_o_servidor_resgatou() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;

    let mut a = conectar(endereco, 73, "p").await?;
    let sala = VoiceRoomId(a.sessao().voice_rooms[0].id.get());
    trancar(&daemon, sala).await;
    let quem = a.sessao().person;
    let mut room = Room::new();
    room.adopt(a.sessao(), "p");

    a.entrar_na_voice_room(sala, Some(SENHA.into())).await?;
    room.enter_voice_room(sala);
    let mut sentou = false;
    for _ in 0..50 {
        dobrar(&mut a, &mut room, Duration::from_millis(100)).await;
        if no_servidor(&daemon, quem).await == Some(sala) {
            sentou = true;
            break;
        }
    }
    assert!(sentou, "a entrada com a senha certa não sentou ninguém");
    // O que ainda vier pelo fio da entrada — a confirmação dela inclusive.
    dobrar(&mut a, &mut room, ASSENTAR).await;
    let ssrc_antes = a.sessao().ssrc;

    // A queda: o enlace some sem despedida, e o servidor guarda o assento.
    drop(a);
    let mut caiu = false;
    for _ in 0..100 {
        if no_servidor(&daemon, quem).await.is_none() {
            caiu = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        caiu,
        "o servidor não viu a queda, e o teste mediu outra coisa"
    );

    // A volta: a mesma chave, o assento e o `ssrc` resgatados.
    let mut a = conectar(endereco, 73, "p").await?;
    assert_eq!(
        a.sessao().ssrc,
        ssrc_antes,
        "o servidor não resgatou o assento na volta, e o teste mediu outra coisa"
    );
    // O que a FFI faz no braço da volta: adota a sessão nova e senta a pessoa
    // na sala que o motor refez — e o motor a refaz sem a senha.
    room.adopt(a.sessao(), "p");
    a.entrar_na_voice_room(sala, None).await?;
    room.enter_voice_room(sala);

    let mut recusou = false;
    for _ in 0..50 {
        dobrar(&mut a, &mut room, Duration::from_millis(100)).await;
        if room
            .notice
            .as_ref()
            .is_some_and(|aviso| aviso.reason == AlertReason::VoiceRoomEntryRefused)
        {
            recusou = true;
            break;
        }
    }
    assert!(
        recusou,
        "a entrada refeita sem a senha não foi recusada, e o teste mediu outra coisa"
    );
    dobrar(&mut a, &mut room, ASSENTAR).await;

    assert_eq!(
        no_servidor(&daemon, quem).await,
        Some(sala),
        "o servidor não manteve a pessoa sentada, e o teste mediu outra coisa"
    );
    assert_eq!(
        room.current_voice_room,
        Some(sala),
        "M3: a pessoa caiu e voltou numa sala com senha, o servidor a sentou de novo, e a \
         recusa da entrada refeita a tirou da sala na própria tela"
    );
    // E o assento pelo qual o áudio da pendência 52 abre: o confirmado. Esta
    // asserção entrou depois do vermelho no `fc6d0346`, que não tinha o método.
    assert_eq!(
        room.sala_confirmada(),
        Some(sala),
        "M3: o servidor manteve a pessoa sentada, e o assento pelo qual o áudio abre a dá \
         por fora da sala"
    );

    daemon.shutdown();
    Ok(())
}
