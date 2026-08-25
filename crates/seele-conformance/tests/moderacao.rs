//! Os quatro verbos de moderação, contra um Dogma de verdade.
//!
//! # Por que aqui, e conduzindo um `Plug`
//!
//! Duas coisas que só existem com as duas pontas de pé.
//!
//! **A recusa é do servidor.** É a que passaria por engano mais fácil. Nem o
//! `seele-core` nem o `seele-ffi` conferem permissão nenhuma — de propósito,
//! porque a `specs/08-seguranca.md` põe a decisão no servidor —, então o pedido
//! de um pessoa sem `Kick` **sai no fio**. Um teste que só olhasse o cliente não
//! distinguiria «a casca não mandou» de «o Dogma recusou», e as duas dão
//! exatamente a mesma tela. Aqui a diferença é medida por fora: a vítima
//! continua conectada e sentada, o que só é observável de outra sessão.
//!
//! **A ponte liga em algum lugar.** Um braço vazio no `executar` do `seele-ffi`
//! deixa a árvore inteira verde: o botão não faria nada, em silêncio, no app
//! publicado. Já aconteceu neste projeto. Por isso estes testes conduzem um
//! `Plug` de verdade, que é o mesmo objeto que o comando Tauri segura, e não um
//! `Enlace` — o `Enlace` pula justamente o trecho que some.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use seele_ffi::{ConnectConfig, EndReason, Event, EventListener, NoticeReason, Plug, PlugError};
use seele_server::persistence::{Persistence, Location};
use seele_server::permissions::{Permissions, COMMANDER_ROLE, OPERATOR_ROLE, PERSON_ROLE};
use seele_server::{DogmaConfig, Server};

const VOICE_ROOM: u32 = 1;
const LINE: u32 = 1;
const PRAZO: Duration = Duration::from_secs(10);

/// Sobe um Dogma numa porta que o sistema escolhe, com o banco num arquivo.
///
/// Num arquivo e não em memória porque alguns testes daqui precisam **ser** o
/// operador do Dogma para além do que o protocolo oferece: não há verbo para
/// conceder papel, e a metade da moderação que só aparece entre um Operador e
/// um Comandante ficaria sem teste. Abrir uma segunda conexão ao mesmo arquivo
/// e mexer no PERMISSIONS é encenar o que uma tela de papéis fará um dia.
async fn dogma(marca: &str) -> Result<(SocketAddr, Arc<Server>, std::path::PathBuf)> {
    let mut arquivo = std::env::temp_dir();
    arquivo.push(format!("seele-moderacao-{marca}-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&arquivo);

    let config = DogmaConfig {
        name: "Terceira Tóquio".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::File(arquivo.clone()),
        ..DogmaConfig::default()
    };
    let servidor = Arc::new(Server::bind(config).await?);
    let endereco = servidor.local_addr()?;
    let aceitando = Arc::clone(&servidor);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });
    Ok((endereco, servidor, arquivo))
}

/// Onde mora a identidade de um apelido, sem mexer no que já está lá.
///
/// **Sem apagar**, e isso é o ponto: o ADR 0004 faz da chave a identidade e o
/// ADR 0017 prende o apelido a ela, então uma casa limpa é uma chave nova, e uma
/// chave nova pedindo um apelido que já é de alguém é recusada com
/// `CredentialRejected`. Voltar depois de ser expulso tem que ser a **mesma
/// pessoa** voltando, ou o teste mediria outra coisa.
///
/// Uma casa por apelido e por processo, apagada uma vez em [`nascer`].
fn casa(nome: &str) -> String {
    let mut caminho = std::env::temp_dir();
    caminho.push(format!("seele-moderacao-{nome}-{}", std::process::id()));
    caminho.to_string_lossy().into_owned()
}

/// A primeira vinda de um apelido: casa limpa, identidade nova.
fn nascer(nome: &str) -> String {
    let caminho = casa(nome);
    let _ = std::fs::remove_dir_all(&caminho);
    caminho
}

fn conectar(endereco: SocketAddr, apelido: &str) -> Result<Arc<Plug>, PlugError> {
    Plug::connect(ConnectConfig {
        server: endereco.to_string(),
        alternate_servers: Vec::new(),
        nickname: apelido.to_owned(),
        home: casa(apelido),
        join_secret: None,
        expected_fingerprint: None,
        bilhete: None,
        // Não há placa de som numa máquina de integração contínua, e nada aqui
        // depende de áudio.
        audio: false,
        capture_device: None,
        playback_device: None,
    })
    .map(|(plug, _confianca)| plug)
}

/// Conecta fora da thread que desenha, como o comando do Tauri faz.
///
/// Estreia um apelido: a casa é apagada primeiro, então esta é uma identidade
/// nova contra um Dogma recém-nascido. Voltar usa [`voltar`], que guarda a
/// chave.
async fn entrar(endereco: SocketAddr, apelido: &str) -> Result<Arc<Plug>> {
    let _ = nascer(apelido);
    voltar(endereco, apelido).await
}

/// Reconecta com a **mesma** identidade.
async fn voltar(endereco: SocketAddr, apelido: &str) -> Result<Arc<Plug>> {
    let apelido = apelido.to_owned();
    Ok(tokio::task::spawn_blocking(move || conectar(endereco, &apelido)).await??)
}

/// Espera até o snapshot dizer o que o teste quer, ou desistir.
fn ate<F: Fn(&Plug) -> bool>(plug: &Plug, pronto: F) -> bool {
    let fim = Instant::now() + PRAZO;
    while Instant::now() < fim {
        if pronto(plug) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    pronto(plug)
}

/// Quantas pessoas o snapshot desenha num voice room.
fn sentados(plug: &Plug, voice_room: u32) -> usize {
    plug.snapshot()
        .voice_rooms
        .iter()
        .find(|desenhado| desenhado.id == voice_room)
        .map_or(0, |desenhado| desenhado.people.len())
}

/// O último aviso que a tela mostraria.
fn aviso(plug: &Plug) -> Option<NoticeReason> {
    plug.snapshot().notice.map(|notice| notice.reason)
}

/// Conta as recusas, em vez de olhar a última.
///
/// `Snapshot::notice` guarda **a última**, então quatro pedidos recusados em
/// seguida deixam o mesmo valor lá e três deles passariam de graça: o aviso do
/// primeiro responderia pelos outros. Contar é o que separa os quatro.
#[derive(Default)]
struct Recusas(std::sync::Mutex<usize>);

impl EventListener for Recusas {
    fn on_event(&self, event: Event) {
        if let Event::NoticeRaised { notice } = event {
            if notice.reason == NoticeReason::PermissionDenied {
                if let Ok(mut quantas) = self.0.lock() {
                    *quantas += 1;
                }
            }
        }
    }
}

impl Recusas {
    fn quantas(&self) -> usize {
        self.0.lock().map(|quantas| *quantas).unwrap_or(0)
    }
}

/// Dá um papel a uma conta, e tira os outros.
///
/// Encena a tela de papéis que ainda não existe. Numa segunda conexão ao mesmo
/// arquivo: o SQLite serializa escritores, e é assim que `permissions` já é testado
/// contra concorrência de verdade.
fn dar_papel(
    arquivo: &std::path::Path,
    person: seele_proto::ids::PersonId,
    papel: seele_proto::ids::RoleId,
) {
    let persistence = Persistence::open(&Location::File(arquivo.to_path_buf())).expect("abrir o banco");
    let permissions = Permissions::new(&persistence);
    for tinha in [COMMANDER_ROLE, OPERATOR_ROLE, PERSON_ROLE] {
        if tinha != papel {
            permissions.revoke_role(person, tinha).expect("revogar");
        }
    }
    permissions.grant_role(person, papel).expect("conceder");
}

#[tokio::test(flavor = "multi_thread")]
async fn um_pessoa_comum_e_recusado_pelo_dogma_e_nao_pela_casca() -> Result<()> {
    let (endereco, servidor, _arquivo) = dogma("recusa").await?;

    // O anfitrião conecta primeiro e vira Comandante. Aqui ele é a vítima e a
    // testemunha ao mesmo tempo: se qualquer verbo tivesse passado, a sessão
    // dele acabaria, ou o plug dele mudaria de sala.
    let anfitriao = entrar(endereco, "anfitriao-recusa").await?;
    anfitriao.insert_plug(VOICE_ROOM)?;
    anfitriao.open_line(LINE)?;
    anfitriao.send_message(LINE, "verificando harmônicos".into())?;
    assert!(
        ate(&anfitriao, |plug| !plug.messages().is_empty()),
        "a mensagem do anfitrião não chegou; não há o que tentar apagar"
    );
    let alvo = anfitriao.snapshot().me.expect("o anfitrião tem identidade");
    let mensagem = anfitriao.messages()[0].id;

    let intruso = entrar(endereco, "intruso-recusa").await?;
    let pode = intruso.snapshot();
    assert!(
        !pode.may_kick && !pode.may_ban && !pode.may_remove_message && !pode.may_move_person,
        "a segunda conta chegou podendo moderar, e este teste não mede mais nada"
    );

    // Os quatro pedidos **saem no fio**: nada no core nem na ponte confere
    // permissão. É essa a razão de este teste existir aqui.
    //
    // Contados um a um, e não lidos do `Snapshot`: aquele guarda o último
    // aviso, então a recusa do primeiro verbo responderia pelos outros três e
    // um `if` esquecido em qualquer um deles passaria despercebido. Cada verbo
    // tem a sua checagem, e um `if` esquecido é uma porta aberta.
    let recusas = Arc::new(Recusas::default());
    intruso.subscribe(Arc::clone(&recusas) as Arc<dyn EventListener>);

    let mut esperadas = 0;
    for (verbo, pedido) in [
        ("expulsar", intruso.kick_person(alvo)),
        ("banir", intruso.ban_person(alvo, None, None)),
        ("remover_mensagem", intruso.remove_message(mensagem)),
        ("mover_pessoa", intruso.move_person(alvo, VOICE_ROOM)),
    ] {
        pedido.unwrap_or_else(|erro| panic!("{verbo} não chegou a ser pedido: {erro}"));
        esperadas += 1;
        assert!(
            ate(&intruso, |_| recusas.quantas() >= esperadas),
            "{verbo}: o Dogma recusou em silêncio — {} recusas depois de {esperadas} \
             pedidos, e um silêncio não se distingue de um servidor quebrado",
            recusas.quantas()
        );
    }
    assert_eq!(
        recusas.quantas(),
        4,
        "quatro verbos, quatro recusas: {:?}",
        aviso(&intruso)
    );

    // E o que prova que a recusa é do **servidor**: o alvo continua de pé. Uma
    // casca que tivesse escondido os botões produziria exatamente a mesma tela
    // do lado do intruso, e nenhuma diferença aqui.
    let vitima = anfitriao.snapshot();
    assert_eq!(vitima.ended, None, "o intruso derrubou o Comandante");
    assert!(
        vitima
            .voice_rooms
            .iter()
            .any(|voice_room| voice_room.id == VOICE_ROOM && voice_room.occupied_by_us),
        "o plug do Comandante saiu da sala de voz por conta de um pedido recusado"
    );
    assert_eq!(
        anfitriao.messages().len(),
        1,
        "o intruso apagou a mensagem do Comandante"
    );

    // Nem no banco: uma reconexão é a leitura mais crua que existe.
    let de_volta = entrar(endereco, "testemunha-recusa").await?;
    assert!(
        de_volta.snapshot().ended.is_none(),
        "a testemunha nem conseguiu entrar"
    );

    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn expulsar_acaba_com_a_sessao_e_deixa_voltar() -> Result<()> {
    let (endereco, servidor, _arquivo) = dogma("expulsar").await?;

    let anfitriao = entrar(endereco, "anfitriao-expulsar").await?;
    assert!(
        anfitriao.snapshot().may_kick,
        "quem hospeda não recebeu Kick, e o verbo não tem quem o use"
    );

    let visita = entrar(endereco, "visita-expulsar").await?;
    visita.insert_plug(VOICE_ROOM)?;
    let quem = visita.snapshot().me.expect("a visita tem identidade");
    assert!(
        ate(&anfitriao, |plug| sentados(plug, VOICE_ROOM) == 1),
        "a visita não chegou a sentar; expulsar não mede nada"
    );

    anfitriao.kick_person(quem)?;

    // Um: a sessão acaba, e **com o motivo enumerado**. Uma queda sem motivo
    // manda a pessoa procurar problema de rede.
    assert!(
        ate(&visita, |plug| plug.snapshot().ended.is_some()),
        "a sessão da visita continuou de pé depois da expulsão"
    );
    assert_eq!(
        visita.snapshot().ended,
        Some(EndReason::Kicked),
        "a visita foi derrubada sem saber que foi expulsa"
    );

    // Dois: a sala esvazia para quem ficou.
    assert!(
        ate(&anfitriao, |plug| sentados(plug, VOICE_ROOM) == 0),
        "quem foi expulso continua desenhado na sala de voz"
    );

    // Três: expulsar é esta sessão e nada além dela — e o assento **não** fica
    // guardado. A janela de carência existe para um trem entrando num túnel;
    // aplicada aqui, devolveria a pessoa à sala de voz de onde ela foi tirada no
    // instante em que reconectasse, e o verbo estaria desfeito por um recurso
    // feito para outra coisa.
    let de_volta = voltar(endereco, "visita-expulsar").await?;
    assert_eq!(de_volta.snapshot().ended, None, "expulsar virou banimento");
    assert!(
        !de_volta
            .snapshot()
            .voice_rooms
            .iter()
            .any(|voice_room| voice_room.occupied_by_us),
        "quem voltou caiu de novo dentro da sala de voz de onde tinha sido expulso"
    );

    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn banir_acaba_com_a_sessao_e_impede_de_voltar() -> Result<()> {
    let (endereco, servidor, _arquivo) = dogma("banir").await?;

    let anfitriao = entrar(endereco, "anfitriao-banir").await?;
    let visita = entrar(endereco, "visita-banir").await?;
    let quem = visita.snapshot().me.expect("a visita tem identidade");

    anfitriao.ban_person(quem, Some("inundou a Linha".into()), None)?;

    assert!(
        ate(&visita, |plug| plug.snapshot().ended.is_some()),
        "a sessão de quem foi banido continuou de pé"
    );
    assert_eq!(
        visita.snapshot().ended,
        Some(EndReason::Banned),
        "quem foi banido foi derrubado sem saber por quê"
    );

    // A metade que separa banir de expulsar: a volta é recusada. Sem esta
    // asserção, `banir` implementado como `expulsar` passa o teste inteiro.
    let recusado = tokio::task::spawn_blocking(move || conectar(endereco, "visita-banir")).await?;
    assert!(
        recusado.is_err(),
        "quem foi banido reconectou; banir virou expulsar"
    );

    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn apagar_uma_mensagem_tira_ela_da_conversa_de_todo_mundo() -> Result<()> {
    let (endereco, servidor, _arquivo) = dogma("apagar").await?;

    let anfitriao = entrar(endereco, "anfitriao-apagar").await?;
    anfitriao.open_line(LINE)?;

    let visita = entrar(endereco, "visita-apagar").await?;
    visita.open_line(LINE)?;
    visita.send_message(LINE, "padrão azul".into())?;
    visita.send_message(LINE, "isto some".into())?;

    assert!(
        ate(&anfitriao, |plug| plug.messages().len() == 2),
        "as duas mensagens não chegaram ao Comandante"
    );
    let some = anfitriao.messages()[1].id;

    anfitriao.remove_message(some)?;

    // Some, e não fica marcada. As duas metades da decisão estão amarradas:
    // `Messages::remove` limpa o corpo e carimba `deleted_at`, o `history`
    // filtra o que está carimbado, e o `Room::apply` tira a linha da tela. O que
    // a linha sobrevivente serve é a não deixar resposta apontando para o vazio.
    assert!(
        ate(&anfitriao, |plug| plug.messages().len() == 1),
        "a mensagem apagada continua na conversa de quem apagou"
    );
    assert!(
        ate(&visita, |plug| plug.messages().len() == 1),
        "a mensagem apagada continua na conversa de quem a escreveu"
    );
    assert_eq!(anfitriao.messages()[0].body, "padrão azul");

    // E a outra metade da permissão: o autor apaga a **própria** sem ter
    // `RemoveMessage`. A permissão do specs/04 diz «de outra pessoa», e um
    // Dogma onde consertar o próprio erro de digitação precisa de um operador é
    // um Dogma onde se fala com o operador sobre erros de digitação.
    assert!(
        !visita.snapshot().may_remove_message,
        "a visita tem a permissão, e esta metade do teste não mede nada"
    );
    let propria = visita.messages()[0].id;
    visita.remove_message(propria)?;
    assert!(
        ate(&visita, |plug| plug.messages().is_empty()),
        "o autor não conseguiu apagar a própria mensagem: {:?}",
        aviso(&visita)
    );
    assert!(
        ate(&anfitriao, |plug| plug.messages().is_empty()),
        "a remoção pelo autor não chegou a quem estava lendo"
    );

    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn mover_leva_o_plug_e_conta_a_pessoa() -> Result<()> {
    let (endereco, servidor, _arquivo) = dogma("mover").await?;

    let anfitriao = entrar(endereco, "anfitriao-mover").await?;
    anfitriao.create_voice_room("VOICE_ROOM-02 SALA DOS FUNDOS".into(), 8, None)?;
    assert!(
        ate(&anfitriao, |plug| plug.snapshot().voice_rooms.len() == 2),
        "o segundo sala de voz não foi feito, e não há para onde mover ninguém"
    );
    let destino = anfitriao.snapshot().voice_rooms[1].id;

    let visita = entrar(endereco, "visita-mover").await?;
    visita.insert_plug(VOICE_ROOM)?;
    let quem = visita.snapshot().me.expect("a visita tem identidade");
    assert!(
        ate(&visita, |plug| plug
            .snapshot()
            .voice_rooms
            .iter()
            .any(|voice_room| voice_room.id == VOICE_ROOM && voice_room.occupied_by_us)),
        "a visita não entrou no primeiro sala de voz"
    );

    anfitriao.move_person(quem, destino)?;

    // Um: o plug muda de sala do lado de quem foi movido. Sem isto a pessoa
    // continuaria mandando voz para a sala que ela acha que ainda é a dela.
    assert!(
        ate(&visita, |plug| plug
            .snapshot()
            .voice_rooms
            .iter()
            .any(|voice_room| voice_room.id == destino && voice_room.occupied_by_us)),
        "quem foi movido continua desenhado na sala de voz antigo"
    );
    assert!(
        !visita
            .snapshot()
            .voice_rooms
            .iter()
            .any(|voice_room| voice_room.id == VOICE_ROOM && voice_room.occupied_by_us),
        "o plug ficou nos dois voice_rooms ao mesmo tempo"
    );

    // Dois: a pessoa é **contada**. Ser movido em silêncio é indistinguível de
    // um cliente que se perdeu de onde estava.
    assert!(
        ate(&visita, |plug| aviso(plug)
            == Some(NoticeReason::MovedByOperator)),
        "quem foi movido não recebeu aviso nenhum: {:?}",
        aviso(&visita)
    );

    // Três: todo mundo vê. Para quem assiste, um movimento é uma saída e uma
    // entrada, que é o que o roster já sabe desenhar.
    assert!(
        ate(&anfitriao, |plug| sentados(plug, destino) == 1
            && sentados(plug, VOICE_ROOM) == 0),
        "quem assiste vê a visita em {:?}",
        anfitriao
            .snapshot()
            .voice_rooms
            .iter()
            .map(|voice_room| (voice_room.id, voice_room.people.len()))
            .collect::<Vec<_>>()
    );

    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn um_operador_modera_pessoas_e_nao_o_comandante() -> Result<()> {
    let (endereco, servidor, arquivo) = dogma("hierarquia").await?;

    // O anfitrião é Comandante por ser o primeiro a chegar.
    let anfitriao = entrar(endereco, "anfitriao-hierarquia").await?;
    let comandante = anfitriao.snapshot().me.expect("identidade");

    // O segundo chega como Pessoa; é promovido a Operador pelo banco, que é o
    // que a tela de papéis fará um dia.
    let operador = entrar(endereco, "operador-hierarquia").await?;
    dar_papel(
        &arquivo,
        seele_proto::ids::PersonId(operador.snapshot().me.expect("identidade")),
        OPERATOR_ROLE,
    );
    drop(operador);
    let operador = voltar(endereco, "operador-hierarquia").await?;
    let poderes = operador.snapshot();
    assert!(
        poderes.may_kick && poderes.may_ban,
        "o Operador não recebeu a moderação que o specs/04 lhe dá: {poderes:?}"
    );

    // O verbo funciona: sem esta metade, «o Operador não bane o Comandante»
    // passaria com um Operador que não consegue banir ninguém.
    let visita = entrar(endereco, "visita-hierarquia").await?;
    let alguem = visita.snapshot().me.expect("identidade");
    operador.kick_person(alguem)?;
    assert!(
        ate(&visita, |plug| plug.snapshot().ended
            == Some(EndReason::Kicked)),
        "o Operador não conseguiu expulsar um Pessoa comum"
    );

    // E não funciona para cima. A `specs/04-servidor-seele.md` dá «moderação»
    // ao Operador e tudo ao Comandante, o que sozinho deixaria promover um
    // amigo por uma noite significar entregar-lhe a chave de trancar você fora
    // do seu próprio Dogma, para sempre, com um verbo que a spec diz que ele
    // deve ter.
    operador.ban_person(comandante, None, None)?;
    assert!(
        ate(&operador, |plug| aviso(plug)
            == Some(NoticeReason::PermissionDenied)),
        "o Dogma deixou o Operador banir o Comandante em silêncio"
    );
    assert_eq!(
        anfitriao.snapshot().ended,
        None,
        "o Operador baniu o Comandante do Dogma que ele hospeda"
    );
    let ainda =
        tokio::task::spawn_blocking(move || conectar(endereco, "anfitriao-hierarquia")).await?;
    assert!(
        ainda.is_ok(),
        "o Comandante ficou trancado para fora do próprio Dogma: {ainda:?}"
    );

    servidor.shutdown();
    Ok(())
}
