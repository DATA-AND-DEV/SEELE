//! Salas feitas por quem hospeda, contra um Dogma de verdade.
//!
//! Três coisas que só existem com um servidor do outro lado, e que nenhum teste
//! de unidade dos dois lados consegue enxergar sozinho.
//!
//! # 1. A recusa é do servidor, e não da casca
//!
//! Este é o que mais fácil passaria por engano. `seele-core` não confere
//! permissão nenhuma — de propósito, porque a `specs/08-seguranca.md` põe a
//! decisão no servidor —, então o pedido de um pessoa sem `ManageVoiceRooms`
//! **sai no fio**. Um teste que só olhasse o cliente não distinguiria «a casca
//! não mandou» de «o Dogma recusou», e as duas dão a mesma tela.
//!
//! Então este arquivo separa as duas com três asserções que só o servidor pode
//! produzir: o `Alert` com `PermissionDenied` volta; o **Comandante conectado
//! ao lado não recebe `VoiceRoomCreated` nenhum**, o que prova que a sala não foi
//! feita em lugar nenhum; e um aperto de mão novo não a encontra, o que prova
//! que ela não foi feita nem no PERSISTENCE.
//!
//! # 2. Quem hospeda vira Comandante, e só quem hospeda
//!
//! Medido no fio, em `Sessao::permissions`, e não no banco: é isso que a casca
//! recebe, e é aí que a regra vira produto. A segunda conta é metade do teste —
//! «a primeira vira Comandante» implementado como «toda conta vira Comandante»
//! passaria sem ela e entregaria o Dogma a quem entrasse.
//!
//! # 3. A sala aparece para quem já estava conectado
//!
//! Sem reconectar. Uma sala que só aparece no próximo aperto de mão é uma sala
//! cujo criador tem de pedir a todo mundo que reconecte antes de poderem entrar
//! nela, e isso não é uma funcionalidade — é uma demonstração.

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
use seele_core::{MemoryPinStore, PinStore};
use seele_proto::control::{AlertReason, Permission, ServerMessage};
use seele_proto::ids::{VoiceRoomId, LineId};
use seele_server::persistence::Location;
use seele_server::{DogmaConfig, Server};

/// Sobe um Dogma numa porta que o sistema escolhe.
///
/// Porta zero, nunca um número escrito à mão, que colidiria com o Dogma que a
/// pessoa deixou rodando na própria máquina. Cada teste sobe o seu, e isso
/// importa mais aqui do que nos vizinhos: **a primeira conta a chegar num Dogma
/// vira Comandante**, então dois testes dividindo um servidor dividiriam também
/// quem manda nele, e a ordem em que o `cargo test` os roda passaria a decidir
/// o resultado.
async fn dogma() -> Result<(SocketAddr, Arc<Server>)> {
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
    Ok((endereco, servidor))
}

/// O apelido entra por fora porque o ADR 0017 o prende à identidade: uma chave
/// nova com o apelido de outra pessoa é recusada com `CredentialRejected`.
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

/// Espera um aviso que interesse, ou desiste.
async fn esperar<F>(enlace: &mut Enlace, prazo: Duration, mut aceita: F) -> Option<Aviso>
where
    F: FnMut(&Aviso) -> bool,
{
    let fim = tokio::time::Instant::now() + prazo;
    while tokio::time::Instant::now() < fim {
        match tokio::time::timeout(Duration::from_millis(200), enlace.proximo()).await {
            Ok(aviso) if aceita(&aviso) => return Some(aviso),
            Ok(_) | Err(_) => {}
        }
    }
    None
}

/// Drena o que já chegou, sem esperar por nada.
///
/// Para as asserções negativas: «isto **não** chegou» só vale depois de dar ao
/// Dogma tempo de mandar, e um `proximo()` sem prazo esperaria para sempre.
async fn drenar(enlace: &mut Enlace, por: Duration) -> Vec<Aviso> {
    let mut vistos = Vec::new();
    let fim = tokio::time::Instant::now() + por;
    while tokio::time::Instant::now() < fim {
        if let Ok(aviso) = tokio::time::timeout(Duration::from_millis(100), enlace.proximo()).await
        {
            vistos.push(aviso);
        }
    }
    vistos
}

fn e_voice_room_criado(aviso: &Aviso, nome: &str) -> bool {
    matches!(
        aviso,
        Aviso::Mensagem(mensagem)
            if matches!(&**mensagem, ServerMessage::VoiceRoomCreated { voice_room } if voice_room.name == nome)
    )
}

fn e_recusa(aviso: &Aviso) -> bool {
    matches!(
        aviso,
        Aviso::Mensagem(mensagem)
            if matches!(
                &**mensagem,
                ServerMessage::Alert { reason: AlertReason::PermissionDenied, .. }
            )
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn quem_hospeda_vira_comandante_e_a_segunda_conta_nao() -> Result<()> {
    let (endereco, servidor) = dogma().await?;

    // Quem hospeda é quem conecta primeiro ao próprio Dogma. Medido em
    // `Sessao::permissions`, que é o que a casca recebe — no banco a regra
    // poderia estar certa e não chegar até aqui, que é o defeito que importa.
    let anfitriao = conectar(endereco, 46, "anfitriao").await?;
    assert!(
        anfitriao
            .sessao()
            .permissions
            .contains(&Permission::ManageVoiceRooms),
        "quem hospedou não recebeu ManageVoiceRooms: {:?}",
        anfitriao.sessao().permissions
    );

    let convidado = conectar(endereco, 47, "shinji").await?;
    assert!(
        !convidado
            .sessao()
            .permissions
            .contains(&Permission::ManageVoiceRooms),
        "a segunda conta chegou podendo gerenciar o Dogma dos outros: {:?}",
        convidado.sessao().permissions
    );
    // E não é que a segunda conta chegou sem nada: ela é um Pessoa inteiro.
    // Sem esta linha, «a segunda não vira Comandante» passaria com uma conta
    // que não pode fazer coisa nenhuma.
    for permissao in [
        Permission::Speak,
        Permission::WriteLine,
        Permission::ReadLine,
    ] {
        assert!(
            convidado.sessao().permissions.contains(&permissao),
            "a segunda conta chegou sem {permissao:?}"
        );
    }

    drop(anfitriao);
    drop(convidado);
    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn uma_sala_criada_aparece_para_quem_ja_estava_conectado() -> Result<()> {
    let (endereco, servidor) = dogma().await?;

    let anfitriao = conectar(endereco, 46, "anfitriao").await?;
    let mut testemunha = conectar(endereco, 47, "shinji").await?;

    // A testemunha entrou antes de a sala existir, e é isso que se mede: ela
    // conhece a lista que o aperto de mão dela trouxe, e mais nada.
    let antes = testemunha.sessao().voice_rooms.len();
    assert_eq!(antes, 1, "o Dogma não abriu com a sala de voz de sempre");

    anfitriao
        .criar_linha("planejamento".to_owned())
        .await
        .expect("a sessão do anfitrião acabou");
    anfitriao
        .criar_voice_room("VOICE_ROOM-02 SALA DOS FUNDOS".to_owned(), 8, None)
        .await
        .expect("a sessão do anfitrião acabou");

    // Drenado em vez de esperado um a um, e a diferença não é estilo: os dois
    // anúncios chegam na ordem em que o Dogma os fez, e um `esperar` pelo
    // segundo descartaria o primeiro pelo caminho. Isto reprovou assim na
    // primeira execução.
    let vistos = drenar(&mut testemunha, Duration::from_secs(3)).await;
    assert!(
        vistos
            .iter()
            .any(|aviso| e_voice_room_criado(aviso, "VOICE_ROOM-02 SALA DOS FUNDOS")),
        "a sala foi feita e quem já estava conectado não soube — teria de reconectar para vê-la"
    );
    assert!(
        vistos.iter().any(|aviso| matches!(
            aviso,
            Aviso::Mensagem(mensagem)
                if matches!(&**mensagem, ServerMessage::LineCreated { line } if line.name == "planejamento")
        )),
        "a Linha nova não chegou à testemunha"
    );

    // E ela está no PERSISTENCE, não só no fio: quem chegar depois a encontra sem
    // ninguém ter de repetir o anúncio.
    let depois = conectar(endereco, 48, "asuka").await?;
    let nomes: Vec<&str> = depois
        .sessao()
        .voice_rooms
        .iter()
        .map(|voice_room| voice_room.name.as_str())
        .collect();
    assert!(
        nomes.contains(&"VOICE_ROOM-02 SALA DOS FUNDOS"),
        "a sala não sobreviveu ao anúncio: {nomes:?}"
    );

    drop(anfitriao);
    drop(testemunha);
    drop(depois);
    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn um_pessoa_sem_manage_voice_rooms_e_recusado_pelo_dogma_e_nao_pela_casca() -> Result<()> {
    let (endereco, servidor) = dogma().await?;

    // O anfitrião conecta primeiro e fica de pé, calado, como testemunha. Ele é
    // a peça que separa «a casca não mandou» de «o Dogma recusou»: se a sala
    // tivesse sido feita, o anúncio chegaria a ele.
    let mut anfitriao = conectar(endereco, 46, "anfitriao").await?;
    let mut sem_permissao = conectar(endereco, 47, "shinji").await?;
    assert!(
        !sem_permissao
            .sessao()
            .permissions
            .contains(&Permission::ManageVoiceRooms),
        "o segundo pessoa tem a permissão, e este teste não mede mais nada"
    );

    // `seele-core` não confere nada — o pedido sai no fio. É essa a razão de
    // este teste existir aqui e não em `seele-core`.
    sem_permissao
        .criar_voice_room("VOICE_ROOM-DO-INTRUSO".to_owned(), 8, None)
        .await
        .expect("a sessão acabou antes de o pedido sair");

    // Um: a recusa volta, enumerada. Sem ela o pessoa olharia para uma lista
    // que não mudou sem saber por quê.
    let recusa = esperar(&mut sem_permissao, Duration::from_secs(15), e_recusa).await;
    assert!(
        recusa.is_some(),
        "o Dogma recusou em silêncio: nada distingue isso de um servidor quebrado"
    );

    // Dois: **ninguém** viu a sala nascer. Esta é a asserção que prova que a
    // recusa é do servidor: uma casca que tivesse escondido o botão produziria
    // exatamente a mesma tela do lado do intruso, e nenhuma diferença aqui.
    let no_anfitriao = drenar(&mut anfitriao, Duration::from_secs(2)).await;
    assert!(
        !no_anfitriao
            .iter()
            .any(|aviso| e_voice_room_criado(aviso, "VOICE_ROOM-DO-INTRUSO")),
        "a sala do intruso foi anunciada a quem estava conectado"
    );

    // Três: nem no PERSISTENCE. Um aperto de mão novo é a leitura mais crua que
    // existe da árvore de canais.
    let depois = conectar(endereco, 48, "asuka").await?;
    let nomes: Vec<&str> = depois
        .sessao()
        .voice_rooms
        .iter()
        .map(|voice_room| voice_room.name.as_str())
        .collect();
    assert!(
        !nomes.contains(&"VOICE_ROOM-DO-INTRUSO"),
        "a sala do intruso ficou gravada: {nomes:?}"
    );

    // E a mesma recusa vale para os outros três verbos, que compartilham a
    // permissão e não a checagem — cada um tem o seu `if`, e um `if` esquecido
    // é uma porta aberta que os outros três testes não veriam.
    let voice_room_de_sempre = VoiceRoomId(depois.sessao().voice_rooms[0].id.get());
    let linha_de_sempre = LineId(depois.sessao().lines[0].id.get());
    for (verbo, pedido) in [
        (
            "CreateLine",
            sem_permissao
                .criar_linha("linha-do-intruso".to_owned())
                .await,
        ),
        (
            "RenameVoiceRoom",
            sem_permissao
                .renomear_voice_room(voice_room_de_sempre, "TOMADO".to_owned())
                .await,
        ),
        (
            "RenameLine",
            sem_permissao
                .renomear_linha(linha_de_sempre, "tomada".to_owned())
                .await,
        ),
    ] {
        pedido.expect("a sessão acabou no meio");
        assert!(
            esperar(&mut sem_permissao, Duration::from_secs(15), e_recusa)
                .await
                .is_some(),
            "{verbo} não foi recusado"
        );
    }

    let ultimo = conectar(endereco, 49, "rei").await?;
    assert_eq!(
        ultimo.sessao().voice_rooms[0].name,
        depois.sessao().voice_rooms[0].name,
        "o intruso renomeou a sala de voz"
    );
    assert_eq!(
        ultimo.sessao().lines[0].name,
        depois.sessao().lines[0].name,
        "o intruso renomeou a Linha"
    );
    assert_eq!(
        ultimo.sessao().lines.len(),
        depois.sessao().lines.len(),
        "o intruso criou uma Linha"
    );

    drop(anfitriao);
    drop(sem_permissao);
    drop(depois);
    drop(ultimo);
    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn o_comandante_renomeia_e_todo_mundo_ve_o_nome_novo() -> Result<()> {
    let (endereco, servidor) = dogma().await?;

    let anfitriao = conectar(endereco, 46, "anfitriao").await?;
    let mut testemunha = conectar(endereco, 47, "shinji").await?;
    let voice_room = anfitriao.sessao().voice_rooms[0].id;

    anfitriao
        .renomear_voice_room(voice_room, "VOICE_ROOM-01 PONTE".to_owned())
        .await
        .expect("a sessão do anfitrião acabou");

    let chegou = esperar(&mut testemunha, Duration::from_secs(15), |aviso| {
        matches!(
            aviso,
            Aviso::Mensagem(mensagem)
                if matches!(
                    &**mensagem,
                    ServerMessage::VoiceRoomRenamed { name, .. } if name == "VOICE_ROOM-01 PONTE"
                )
        )
    })
    .await;
    assert!(chegou.is_some(), "o nome novo não chegou a quem estava lá");

    // E ele sobrevive: uma renomeação que só existisse no fio voltaria ao nome
    // velho na próxima conexão, e ninguém saberia qual dos dois é o certo.
    let depois = conectar(endereco, 48, "asuka").await?;
    assert_eq!(depois.sessao().voice_rooms[0].name, "VOICE_ROOM-01 PONTE");
    assert_eq!(
        depois.sessao().voice_rooms.len(),
        1,
        "a renomeação criou uma segunda sala"
    );

    drop(anfitriao);
    drop(testemunha);
    drop(depois);
    servidor.shutdown();
    Ok(())
}
