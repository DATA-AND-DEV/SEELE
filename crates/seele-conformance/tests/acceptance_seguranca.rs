//! Os pontos de segurança que valem para um Dogma exposto a outras pessoas.
//!
//! Cada teste aqui corresponde a algo que estava errado ou ausente até M5, e
//! todos foram escritos depois de verificar o comportamento antigo — não são
//! confirmações de que o código faz o que o código faz.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_core::{Client, MemoryPinStore, PinDecision, PinStore};
use seele_proto::ids::VoiceRoomId;
use seele_server::persistence::{Persistence, Location};
use seele_server::{admissao, DogmaConfig, Server};

const VOICE_ROOM: VoiceRoomId = VoiceRoomId(1);

async fn subir(caminho: &std::path::Path) -> Result<(SocketAddr, Arc<Server>)> {
    let config = DogmaConfig {
        name: "Terceira Tóquio".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::File(caminho.to_path_buf()),
        ..DogmaConfig::default()
    };
    let server = Arc::new(Server::bind(config).await?);
    let address = server.local_addr()?;
    let aceitando = Arc::clone(&server);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });
    Ok((address, server))
}

async fn conectar(
    address: SocketAddr,
    apelido: &str,
    semente: u8,
    pins: Arc<dyn PinStore>,
    segredo: Option<&str>,
) -> Result<Client, seele_core::ConnectError> {
    Client::connect(
        address,
        "localhost",
        "dogma-de-teste",
        apelido,
        &SigningKey::from_bytes(&[semente; 32]),
        pins,
        segredo,
    )
    .await
}

/// Reiniciar o Dogma não pode expulsar quem já se conectou.
///
/// O certificado era gerado a cada boot, então todo reinício trocava a chave e
/// todo cliente via o alerta bloqueante do ADR 0003 — o aviso reservado para
/// ataque, disparado por um reinício de rotina. Isso não só quebra a conexão:
/// ensina a ignorar o único aviso que não pode ser ignorado.
#[tokio::test]
async fn reiniciar_o_dogma_nao_troca_a_chave() -> Result<()> {
    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");

    let (endereco, servidor) = subir(&banco).await?;
    let impressao_inicial = servidor.fingerprint().to_owned();

    // Um pessoa se conecta e fixa a chave.
    let pins: Arc<dyn PinStore> = Arc::new(MemoryPinStore::new());
    let cliente = conectar(endereco, "ayanami", 1, Arc::clone(&pins), None).await?;
    assert!(matches!(
        cliente.pin_decision(),
        PinDecision::FirstContact { .. }
    ));
    drop(cliente);
    servidor.shutdown();
    drop(servidor);

    // O operador reinicia. Mesmo banco, processo novo.
    let (endereco, servidor) = subir(&banco).await?;
    assert_eq!(
        servidor.fingerprint(),
        impressao_inicial,
        "o Dogma trocou de identidade ao reiniciar"
    );

    let cliente = conectar(endereco, "ayanami", 1, pins, None)
        .await
        .map_err(|erro| anyhow::anyhow!("o pessoa foi recusado após um reinício: {erro:?}"))?;
    assert!(matches!(
        cliente.pin_decision(),
        PinDecision::Matches { .. }
    ));

    servidor.shutdown();
    Ok(())
}

/// Um Dogma com senha recusa quem não a tem.
#[tokio::test]
async fn a_senha_do_dogma_fecha_a_porta() -> Result<()> {
    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");

    // O operador define a senha com o Dogma parado.
    {
        let mut persistence = Persistence::open(&Location::File(banco.clone()))?;
        admissao::definir_senha(&mut persistence, Some("terceiro impacto"))?;
    }

    let (endereco, servidor) = subir(&banco).await?;

    let sem_segredo = conectar(
        endereco,
        "estranho",
        9,
        Arc::new(MemoryPinStore::new()),
        None,
    )
    .await;
    assert!(sem_segredo.is_err(), "entrou sem apresentar a senha");

    let errada = conectar(
        endereco,
        "estranho",
        9,
        Arc::new(MemoryPinStore::new()),
        Some("chute"),
    )
    .await;
    assert!(errada.is_err(), "entrou com a senha errada");

    let certa = conectar(
        endereco,
        "ayanami",
        1,
        Arc::new(MemoryPinStore::new()),
        Some("terceiro impacto"),
    )
    .await;
    assert!(
        certa.is_ok(),
        "quem sabe a senha não entrou: {:?}",
        certa.err()
    );

    servidor.shutdown();
    Ok(())
}

/// Um convite vale uma vez, e é isso que o torna seguro num link.
#[tokio::test]
async fn um_convite_serve_a_uma_pessoa_so() -> Result<()> {
    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");

    let token = {
        let mut persistence = Persistence::open(&Location::File(banco.clone()))?;
        admissao::criar_convite(&mut persistence, "ayanami")?
    };

    let (endereco, servidor) = subir(&banco).await?;

    let primeiro = conectar(
        endereco,
        "ayanami",
        1,
        Arc::new(MemoryPinStore::new()),
        Some(&token),
    )
    .await;
    assert!(
        primeiro.is_ok(),
        "o convidado não entrou: {:?}",
        primeiro.err()
    );

    // O mesmo link, repassado adiante.
    let segundo = conectar(
        endereco,
        "penetra",
        2,
        Arc::new(MemoryPinStore::new()),
        Some(&token),
    )
    .await;
    assert!(segundo.is_err(), "o convite foi usado duas vezes");

    servidor.shutdown();
    Ok(())
}

/// A senha de uma sala de voz é conferida, e não só anunciada.
#[tokio::test]
async fn a_senha_do_voice_room_e_conferida() -> Result<()> {
    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");

    {
        // O Dogma semeia a sala de voz ao subir, então sobe uma vez antes de trancar.
        let (_, servidor) = subir(&banco).await?;
        servidor.shutdown();
        let mut persistence = Persistence::open(&Location::File(banco.clone()))?;
        admissao::definir_senha_voice_room(&mut persistence, VOICE_ROOM, Some("geofront"))?;
    }

    let (endereco, servidor) = subir(&banco).await?;
    let mut cliente = conectar(
        endereco,
        "ayanami",
        1,
        Arc::new(MemoryPinStore::new()),
        None,
    )
    .await?;

    // A entrada sem senha é recusada com um alerta, não com uma queda: a sala de voz
    // é um cômodo, e errar a senha dele não derruba a sessão.
    cliente.insert_plug(VOICE_ROOM).await?;
    let alerta = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        aguardar_recusa(&mut cliente),
    )
    .await;
    assert!(
        alerta.is_ok(),
        "entrar num sala de voz trancado sem senha não foi recusado"
    );

    servidor.shutdown();
    Ok(())
}

async fn aguardar_recusa(cliente: &mut Client) {
    while let Ok(mensagem) = cliente.next_event().await {
        if matches!(
            mensagem,
            seele_proto::ServerMessage::Alert {
                reason: seele_proto::control::AlertReason::VoiceRoomEntryRefused,
                ..
            }
        ) {
            return;
        }
    }
}

/// Um Dogma sem configuração continua aberto — e isso é escolha, não descuido.
#[tokio::test]
async fn um_dogma_novo_aceita_qualquer_um() -> Result<()> {
    let pasta = tempfile::tempdir()?;
    let (endereco, servidor) = subir(&pasta.path().join("seele.db")).await?;

    let cliente = conectar(
        endereco,
        "qualquer",
        7,
        Arc::new(MemoryPinStore::new()),
        None,
    )
    .await;
    assert!(
        cliente.is_ok(),
        "o padrão deixou de ser aberto: {:?}",
        cliente.err()
    );

    servidor.shutdown();
    Ok(())
}

// ---------------------------------------------------------------------------
// A portaria — ADR 0030. TOFU aplicado a gente.
// ---------------------------------------------------------------------------

/// A razão que o Dogma deu, ou o pânico que diz o que ele deu no lugar.
///
/// Escrito assim, e não como `is_err()`, porque a portaria inteira existe para
/// distinguir três respostas. Um teste que só perguntasse se falhou passaria com
/// as três dobradas em `CredentialRejected`, que é exatamente o estado de antes
/// deste ADR.
fn razao(erro: Option<seele_core::ConnectError>) -> seele_proto::control::DisconnectReason {
    match erro {
        Some(seele_core::ConnectError::Refused { reason }) => reason,
        outra => panic!("esperava uma recusa enumerada do Dogma, veio {outra:?}"),
    }
}

/// A impressão digital de uma semente, como quem hospeda a vê na tela.
fn impressao_de(semente: u8) -> String {
    seele_proto::transport::key_fingerprint(
        SigningKey::from_bytes(&[semente; 32])
            .verifying_key()
            .as_bytes(),
    )
}

/// Um Dogma com portaria não deixa ninguém entrar por um caminho lateral.
///
/// O guarda central do ADR 0030, e ele encena cada tentativa em vez de ler o
/// código. Cada bloco abaixo é uma porta dos fundos que alguém tentaria de
/// verdade, e a mais perigosa é a terceira: um convite **válido e não gasto** é
/// a credencial mais forte que este produto emite, e ainda assim não atravessa a
/// portaria. Se atravessasse, a camada mais forte teria virado a mais fraca —
/// bastaria vazar um convite para pular a decisão de quem hospeda.
#[tokio::test]
async fn um_dogma_com_portaria_nao_admite_ninguem_por_um_caminho_lateral() -> Result<()> {
    use seele_proto::control::DisconnectReason;
    use seele_server::portaria;

    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");

    let token = {
        let mut persistence = Persistence::open(&Location::File(banco.clone()))?;
        portaria::ligar(&mut persistence, true)?;
        admissao::definir_senha(&mut persistence, Some("terceiro impacto"))?;
        admissao::criar_convite(&mut persistence, "para a Rei")?
    };

    let (endereco, servidor) = subir(&banco).await?;

    // 1. Sem segredo nenhum. Para antes da portaria, na camada do ADR 0021.
    let sem_nada = conectar(
        endereco,
        "estranho",
        9,
        Arc::new(MemoryPinStore::new()),
        None,
    )
    .await;
    assert_eq!(
        razao(sem_nada.err()),
        DisconnectReason::CredentialRejected,
        "quem não traz segredo tem que parar na camada do segredo, e a recusa \
         dela é uniforme de propósito"
    );

    // 2. Com a senha certa. Passa a primeira camada e para na segunda.
    let com_senha = conectar(
        endereco,
        "estranho",
        9,
        Arc::new(MemoryPinStore::new()),
        Some("terceiro impacto"),
    )
    .await;
    assert_eq!(
        razao(com_senha.err()),
        DisconnectReason::AdmissionPending,
        "saber a senha atravessou a portaria; as camadas são conjuntivas e \
         passar por uma não dispensa a outra"
    );

    // 3. Com um convite válido e não gasto. O caminho lateral mais perigoso.
    let com_convite = conectar(
        endereco,
        "convidado",
        8,
        Arc::new(MemoryPinStore::new()),
        Some(&token),
    )
    .await;
    assert_eq!(
        razao(com_convite.err()),
        DisconnectReason::AdmissionPending,
        "um convite válido aprovou sozinho quem quem hospeda nunca viu — o ADR \
         0030 recusa isso porque um link se encaminha"
    );

    // 4. Insistir. Uma porta que cede à repetição não é uma porta.
    for _ in 0..5 {
        let de_novo = conectar(
            endereco,
            "estranho",
            9,
            Arc::new(MemoryPinStore::new()),
            Some("terceiro impacto"),
        )
        .await;
        assert_eq!(razao(de_novo.err()), DisconnectReason::AdmissionPending);
    }

    // 5. Quem hospeda aprova **uma** chave, pelo banco, como a tela fará.
    {
        let mut persistence = Persistence::open(&Location::File(banco.clone()))?;
        // Sete batidas da mesma pessoa são um pedido, não sete.
        let fila = portaria::pedidos(&persistence)?;
        assert_eq!(fila.len(), 2, "a fila tem uma linha por pessoa: {fila:?}");
        portaria::decidir(&mut persistence, &impressao_de(9), true)?;
    }

    let entra = conectar(
        endereco,
        "estranho",
        9,
        Arc::new(MemoryPinStore::new()),
        Some("terceiro impacto"),
    )
    .await;
    assert!(
        entra.is_ok(),
        "a chave aprovada não entrou: {:?}",
        entra.err()
    );

    // 5b. E entra **de novo**, sem trazer segredo nenhum.
    //
    // O caso de campo, relatado assim: «dou permissão, ele tenta bater de novo
    // e dá credencial recusada; mesmo fechando o app dá credencial recusada».
    //
    // O que acontece do outro lado é isto: o app que já entrou uma vez guarda o
    // Dogma na lista de visitados e reconecta **sem** o convite — ele era de uso
    // único e já foi gasto — e sem a senha, que ninguém digitou. A política não
    // tem memória: com o Dogma fechado ela exige segredo de todo mundo, sempre.
    //
    // A portaria **é** a credencial durável de uma pessoa. Se aprovar não
    // dispensar o segredo na volta, aprovar não serviu para nada: a pessoa fica
    // dependendo de um convite novo a cada conexão, que é exatamente o que quem
    // hospeda achou que tinha resolvido ao apertar admitir.
    let sem_segredo_depois_de_aprovado = conectar(
        endereco,
        "estranho",
        9,
        Arc::new(MemoryPinStore::new()),
        None,
    )
    .await;
    assert!(
        sem_segredo_depois_de_aprovado.is_ok(),
        "quem foi admitido pela portaria precisou de segredo para voltar: {:?}",
        sem_segredo_depois_de_aprovado.err()
    );

    // 6. A aprovação não vazou para o vizinho da fila.
    let vizinho = conectar(
        endereco,
        "convidado",
        8,
        Arc::new(MemoryPinStore::new()),
        Some(&token),
    )
    .await;
    assert_eq!(
        razao(vizinho.err()),
        DisconnectReason::AdmissionPending,
        "aprovar uma chave abriu a porta para outra"
    );

    // 7. Nem para uma chave nova que se diz pelo mesmo apelido. O apelido é
    //    texto que a pessoa digitou; a impressão digital é a identidade.
    let homonimo = conectar(
        endereco,
        "estranho",
        3,
        Arc::new(MemoryPinStore::new()),
        Some("terceiro impacto"),
    )
    .await;
    assert_eq!(
        razao(homonimo.err()),
        DisconnectReason::AdmissionPending,
        "uma chave nova entrou por se dizer pelo nome de alguém já admitido"
    );

    // 8. E se quem hospeda aprovar **essa** chave nova, a recusa que sobra tem
    //    de dizer qual é.
    //
    // O relato de campo: «dou permissão e continua dando credencial recusada,
    // mesmo fechando o app». Quatro falhas diferentes vestiam essa frase, e uma
    // delas é esta — o apelido pertence a outra chave, e não passa com o tempo:
    // tentar de novo, ser aprovado de novo e reinstalar o app dão o mesmo
    // resultado. Enquanto ela se chamava «credencial recusada», o conselho que
    // vinha junto mandava conferir o convite, que é a única coisa que não era o
    // problema.
    {
        let mut persistence = Persistence::open(&Location::File(banco.clone()))?;
        portaria::decidir(&mut persistence, &impressao_de(3), true)?;
    }
    let homonimo_aprovado = conectar(
        endereco,
        "estranho",
        3,
        Arc::new(MemoryPinStore::new()),
        Some("terceiro impacto"),
    )
    .await;
    assert_eq!(
        razao(homonimo_aprovado.err()),
        DisconnectReason::NicknameTaken,
        "a portaria aprovou e a recusa que sobrou não disse que era o apelido;          quem lê «credencial recusada» vai mexer no convite para sempre"
    );

    // E com outro apelido a mesma chave entra, que é a prova de que a frase
    // acima aponta para a coisa certa de mexer.
    let com_outro_nome = conectar(
        endereco,
        "outro-nome",
        3,
        Arc::new(MemoryPinStore::new()),
        Some("terceiro impacto"),
    )
    .await;
    assert!(
        com_outro_nome.is_ok(),
        "trocar de apelido não resolveu: {:?}",
        com_outro_nome.err()
    );


    servidor.shutdown();
    Ok(())
}

/// Recusado e «ainda não decidiram» são coisas diferentes para quem bate.
///
/// A recusa é sempre a mesma na camada do segredo, e `specs/08-seguranca.md`
/// exige que seja: distinguir contaria a quem está adivinhando qual palpite
/// chegou mais perto. Na portaria não há palpite — a chave foi provada — e
/// dobrar as duas mandaria embora quem só precisava esperar.
#[tokio::test]
async fn quem_foi_recusado_ouve_outra_coisa_de_quem_so_espera() -> Result<()> {
    use seele_proto::control::DisconnectReason;
    use seele_server::portaria;

    let pasta = tempfile::tempdir()?;
    let banco = pasta.path().join("seele.db");

    {
        let mut persistence = Persistence::open(&Location::File(banco.clone()))?;
        portaria::ligar(&mut persistence, true)?;
    }

    let (endereco, servidor) = subir(&banco).await?;

    // Duas pessoas batem.
    for semente in [4_u8, 5_u8] {
        let batida = conectar(
            endereco,
            &format!("pessoa{semente}"),
            semente,
            Arc::new(MemoryPinStore::new()),
            None,
        )
        .await;
        assert_eq!(razao(batida.err()), DisconnectReason::AdmissionPending);
    }

    // Quem hospeda recusa uma e deixa a outra de pé.
    {
        let mut persistence = Persistence::open(&Location::File(banco.clone()))?;
        portaria::decidir(&mut persistence, &impressao_de(4), false)?;
    }

    let voltou = conectar(
        endereco,
        "pessoa4",
        4,
        Arc::new(MemoryPinStore::new()),
        None,
    )
    .await;
    assert_eq!(
        razao(voltou.err()),
        DisconnectReason::AdmissionDenied,
        "quem foi recusado ouviu a frase de quem só precisa esperar"
    );

    let esperando = conectar(
        endereco,
        "pessoa5",
        5,
        Arc::new(MemoryPinStore::new()),
        None,
    )
    .await;
    assert_eq!(
        razao(esperando.err()),
        DisconnectReason::AdmissionPending,
        "recusar uma pessoa recusou a outra junto"
    );

    // E recusar não é banir: revogar a decisão devolve a pessoa à fila em vez de
    // deixá-la barrada para sempre.
    {
        let mut persistence = Persistence::open(&Location::File(banco.clone()))?;
        portaria::revogar(&mut persistence, &impressao_de(4))?;
    }
    let de_novo = conectar(
        endereco,
        "pessoa4",
        4,
        Arc::new(MemoryPinStore::new()),
        None,
    )
    .await;
    assert_eq!(
        razao(de_novo.err()),
        DisconnectReason::AdmissionPending,
        "revogar uma recusa tem que voltar a perguntar, não continuar recusando"
    );

    servidor.shutdown();
    Ok(())
}
