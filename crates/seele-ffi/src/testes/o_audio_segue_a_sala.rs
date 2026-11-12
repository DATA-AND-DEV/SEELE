//! A pendência 52: o áudio abre quando a pessoa passa a estar numa sala de
//! voz, e fecha quando ela deixa de estar.
//!
//! **A prova é da decisão, e nenhum aparelho abre aqui.** A voz é de
//! mentira e o abridor é injetado, no molde dos testes de
//! `reabrir_voz_na_reconexao`: `PortaDaVoz` e `VozDaSala` são genéricas na
//! voz justamente para isso, e `drive` usa as mesmas com a [`Voice`] de
//! verdade.
//!
//! [`Voice`]: seele_core::Voice

use super::{
    ganhos_a_aplicar, readotar_na_volta, sala_do_audio, Aberta, AberturaDoAudio,
    ControlesGuardados, Futuro, Origem, PortaDaVoz, VozDaSala, VozViva,
};
use seele_core::retrato::voice_rooms_of;
use seele_core::{
    AlertReason, AlertSeverity, DeviceChoice, PersonId, PersonProfile, Room, ServerMessage,
    SessionId, SessionInfo, Ssrc, VoiceMode, VoiceRoomId, VoiceRoomInfo,
};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

const SALA: VoiceRoomId = VoiceRoomId(1);

/// Uma voz de mentira, que lembra o que vestiu ao abrir, a tecla de falar e
/// o ganho que cada número recebeu.
#[derive(Debug, Default)]
struct Voz {
    ssrc: u32,
    vestida: Mutex<Option<ControlesGuardados>>,
    /// A tecla de falar, como o átomo `key_held` dos controles do núcleo.
    tecla: Mutex<bool>,
    ganhos: Mutex<HashMap<u32, f32>>,
}

impl VozViva for Voz {
    fn vestir(&self, escolhida: &ControlesGuardados) {
        *self.vestida.lock().expect("o que a voz vestiu") = Some(escolhida.clone());
    }

    fn segurar(&self, segura: bool) {
        *self.tecla.lock().expect("a tecla da voz") = segura;
    }

    fn ganhar(&self, ssrc: Ssrc, ganho: f32) {
        self.ganhos
            .lock()
            .expect("os ganhos da voz")
            .insert(ssrc.get(), ganho);
    }
}

fn voz_de(ssrc: u32) -> Voz {
    Voz {
        ssrc,
        vestida: Mutex::new(None),
        tecla: Mutex::new(false),
        ganhos: Mutex::new(HashMap::new()),
    }
}

/// A voz que `Voice::reopen` devolve na volta: nova, no `ssrc` de agora, e
/// com os controles da velha — o que ela vestiu, a tecla e os ganhos por
/// número —, como `carregar_controles` os leva no núcleo.
fn reaberta(velha: &Voz, ssrc: u32) -> Voz {
    Voz {
        ssrc,
        vestida: Mutex::new(velha.vestida.lock().expect("o que a velha vestiu").clone()),
        tecla: Mutex::new(*velha.tecla.lock().expect("a tecla da velha")),
        ganhos: Mutex::new(velha.ganhos.lock().expect("os ganhos da velha").clone()),
    }
}

/// A tecla de falar da voz viva, se há uma.
fn tecla_de(voz: &VozDaSala<Voz>) -> Option<bool> {
    voz.lock()
        .expect("a voz")
        .as_ref()
        .map(|viva| *viva.tecla.lock().expect("a tecla da voz"))
}

fn entregue(aberta: Aberta<Voz>) -> Futuro<Voz> {
    Box::pin(async move { aberta })
}

fn nada_abre(_: &DeviceChoice) -> Futuro<Voz> {
    panic!("o áudio abriu num ponto em que nada devia abrir")
}

/// O que `drive` faz no fim de cada volta do laço: olha o assento, e, se
/// uma abertura começou, espera a chegada dela como o braço do `select!`.
///
/// Devolve quantas vezes o abridor foi chamado.
async fn olhar_e_esperar(
    porta: &mut PortaDaVoz<Voz>,
    voz: &VozDaSala<Voz>,
    room: &Room,
    ssrc: u32,
) -> u32 {
    let mut aberturas = 0;
    // O assento pelo qual o áudio abre, como `sala_de` o lê em `drive`.
    porta.olhar(voz, sala_do_audio(room), |_| {
        aberturas += 1;
        entregue(Ok(Ok(voz_de(ssrc))))
    });
    if voz.abertura() == AberturaDoAudio::Abrindo {
        let aberta = tokio::time::timeout(Duration::from_secs(5), porta.chegada())
            .await
            .expect("a abertura começou e não chegou");
        porta.chegou(voz, aberta, nada_abre);
    }
    aberturas
}

fn ha_voz(voz: &VozDaSala<Voz>) -> bool {
    voz.lock().expect("a voz").is_some()
}

/// O servidor decidiu o pedido de entrada que estava no ar: o `Pong` do
/// `Ping` marcado que o motor manda atrás de cada pedido.
fn decidida(room: &mut Room) {
    room.apply(&ServerMessage::Pong {
        // O `Pong` de um pedido mais novo que todos os destes testes: fecha
        // o que está no ar, como o `Pong` do pedido que acabou de sair.
        timestamp: seele_core::assento::carimbo_da_entrada(u64::from(u32::MAX)),
    });
}

fn recusa(reason: AlertReason) -> ServerMessage {
    ServerMessage::Alert {
        severity: AlertSeverity::Warning,
        reason,
        operator_text: None,
    }
}

/// (a) Fora de sala, nenhum aparelho aberto; entrar numa sala abre, sair
/// fecha.
///
/// O primeiro olhar é o de `drive` logo depois do aperto de mão: o servidor
/// adotado e ninguém sentado. Antes da pendência 52, era ali que a voz abria,
/// para a sessão inteira.
#[tokio::test]
async fn fora_de_sala_nenhum_aparelho_abre_e_a_sala_abre_e_fecha() {
    let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
    let mut porta = PortaDaVoz::nova();
    let mut room = Room::new();
    room.adopt(&sessao(700), "marcela");
    room.apply(&lia_na_sala());

    let aberturas = olhar_e_esperar(&mut porta, &voz, &room, 700).await;
    assert_eq!(
        aberturas, 0,
        "entrar no servidor abriu o áudio: quem fica só pela conversa por \
             escrito segura o microfone e o repouso da máquina a sessão inteira"
    );
    assert!(!ha_voz(&voz), "há voz de pé fora de sala");
    assert_eq!(voz.abertura(), AberturaDoAudio::ForaDeSala);

    room.enter_voice_room(SALA);
    let aberturas = olhar_e_esperar(&mut porta, &voz, &room, 700).await;
    assert_eq!(
        aberturas, 0,
        "o áudio abriu com a entrada ainda no ar: o microfone abre numa sala que \
             ainda pode recusar (M-d)"
    );
    assert_eq!(voz.abertura(), AberturaDoAudio::ForaDeSala);

    decidida(&mut room);
    let aberturas = olhar_e_esperar(&mut porta, &voz, &room, 700).await;
    assert_eq!(aberturas, 1, "entrar numa sala não abriu o áudio");
    assert!(ha_voz(&voz), "a voz que abriu não ficou no lugar");
    assert_eq!(voz.abertura(), AberturaDoAudio::Aberto);

    room.leave_voice_room();
    let aberturas = olhar_e_esperar(&mut porta, &voz, &room, 700).await;
    assert_eq!(aberturas, 0);
    assert!(
        !ha_voz(&voz),
        "sair da sala não fechou o áudio: o microfone continua aberto para \
             sala nenhuma"
    );
    assert_eq!(voz.abertura(), AberturaDoAudio::ForaDeSala);
}

/// Os cinco caminhos para dentro e para fora de uma sala, um depois do outro.
///
/// Todos são aplicados no `seele-core` — `Room::apply` e os dois métodos de
/// assento, pela porta única de `seele_core::assento` —, e a porta da voz só
/// vê o efeito no assento confirmado. É o que este teste prende: um caminho
/// novo que esquecesse o assento seria pego aqui, e não na tela de alguém com
/// o microfone aberto fora de sala.
///
/// Cada pedido de entrada vem com a resposta que o servidor dá a ele, na
/// ordem em que ela chega: a recusa, quando há, e o `Pong` do `Ping`
/// marcado. E entram as três recusas que **não** tiram ninguém da sala
/// (C1 da rodada 1): a troca recusada, a troca para uma sala lotada e a
/// recusa sem pedido no ar, que é o apelido tomado.
#[tokio::test]
async fn o_audio_abre_e_fecha_pelos_cinco_caminhos_da_sala() {
    const OUTRA: VoiceRoomId = VoiceRoomId(2);
    let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
    let mut porta = PortaDaVoz::nova();
    let mut room = Room::new();
    room.adopt(&sessao(700), "marcela");

    /// O nome do caminho, o que ele faz com o assento, e se a voz fica
    /// aberta depois dele.
    type Caminho = (&'static str, Box<dyn Fn(&mut Room)>, bool);
    let caminhos: Vec<Caminho> = vec![
        (
            "a entrada pedida e aceita",
            Box::new(|room| {
                room.enter_voice_room(SALA);
                decidida(room);
            }),
            true,
        ),
        ("a saída pedida", Box::new(Room::leave_voice_room), false),
        (
            "o operador que move para a sala",
            Box::new(|room| {
                room.apply(&ServerMessage::MovedToVoiceRoom { voice_room: SALA });
            }),
            true,
        ),
        (
            "a troca para outra sala, recusada",
            Box::new(|room| {
                room.enter_voice_room(OUTRA);
                room.apply(&recusa(AlertReason::VoiceRoomEntryRefused));
                decidida(room);
            }),
            true,
        ),
        (
            "a troca para outra sala, lotada",
            Box::new(|room| {
                room.enter_voice_room(OUTRA);
                room.apply(&recusa(AlertReason::VoiceRoomFull));
                decidida(room);
            }),
            true,
        ),
        (
            "uma recusa sem pedido no ar (o apelido tomado)",
            Box::new(|room| {
                room.apply(&recusa(AlertReason::VoiceRoomEntryRefused));
            }),
            true,
        ),
        (
            "a saída pedida de novo",
            Box::new(Room::leave_voice_room),
            false,
        ),
        (
            "a entrada recusada de quem estava fora",
            Box::new(|room| {
                room.enter_voice_room(SALA);
                room.apply(&recusa(AlertReason::VoiceRoomEntryRefused));
                decidida(room);
            }),
            false,
        ),
        (
            "a entrada lotada de quem estava fora",
            Box::new(|room| {
                room.enter_voice_room(SALA);
                room.apply(&recusa(AlertReason::VoiceRoomFull));
                decidida(room);
            }),
            false,
        ),
        (
            "a entrada pedida e aceita mais uma vez",
            Box::new(|room| {
                room.enter_voice_room(SALA);
                decidida(room);
            }),
            true,
        ),
        (
            "a sala apagada",
            Box::new(|room| {
                room.apply(&ServerMessage::VoiceRoomDeleted { voice_room: SALA });
            }),
            false,
        ),
    ];

    for (caminho, andar, dentro) in caminhos {
        andar(&mut room);
        olhar_e_esperar(&mut porta, &voz, &room, 700).await;
        assert_eq!(
            ha_voz(&voz),
            dentro,
            "depois de «{caminho}» a voz está {}: o áudio não segue a sala por \
                 este caminho",
            if ha_voz(&voz) { "aberta" } else { "fechada" }
        );
    }
}

/// (b) O que se escolhe fora de sala vale ao entrar: a voz abre muda.
///
/// `escolher` é o que `Connection::set_muted` e os irmãos chamam — ver
/// `fora_de_sala_a_conexao_guarda_os_controles_e_o_retrato_os_mostra`.
#[tokio::test]
async fn os_controles_escolhidos_fora_de_sala_valem_ao_entrar() {
    let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
    let mut porta = PortaDaVoz::nova();
    let mut room = Room::new();
    room.adopt(&sessao(700), "marcela");
    olhar_e_esperar(&mut porta, &voz, &room, 700).await;

    voz.escolher(|escolhida| escolhida.mudo = true);
    voz.escolher(|escolhida| escolhida.isolamento = true);
    voz.escolher(|escolhida| escolhida.modo = VoiceMode::VoiceActivated);
    voz.escolher(|escolhida| escolhida.supressao = 0.25);
    voz.escolher(|escolhida| escolhida.abertura_dbfs = Some(-48.0));

    room.enter_voice_room(SALA);
    decidida(&mut room);
    olhar_e_esperar(&mut porta, &voz, &room, 700).await;

    let viva = voz.lock().expect("a voz");
    let vestida = viva
        .as_ref()
        .and_then(|viva| viva.vestida.lock().expect("o que a voz vestiu").clone());
    let Some(vestida) = vestida else {
        panic!(
            "a voz abriu sem vestir o que a pessoa escolheu fora de sala: ela se \
                 mutou antes de entrar, o roster a mostra muda, e o microfone está aberto"
        );
    };
    assert!(
        vestida.mudo,
        "a voz abriu com o microfone aberto para quem se mutou fora de sala, \
             enquanto o roster a mostra muda"
    );
    assert!(
        vestida.isolamento,
        "o Isolamento total pedido fora de sala se perdeu"
    );
    assert_eq!(vestida.modo, VoiceMode::VoiceActivated);
    assert!((vestida.supressao - 0.25).abs() < f32::EPSILON);
    assert_eq!(vestida.abertura_dbfs, Some(-48.0));
}

/// (c) A volta de quem está numa sala reabre a voz, e a de quem está fora
/// não abre nada.
///
/// Na ordem do braço `Aviso::Reconectado` de `drive`: o `Room` volta com o
/// assento que o motor restabeleceu, a porta reabre a voz viva, e o olhar do
/// fim da volta não pode desfazer nada disso.
#[tokio::test]
async fn a_volta_de_quem_esta_na_sala_reabre_a_voz_e_a_de_quem_esta_fora_nao_abre() {
    // Na sala, com a voz de pé.
    let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
    let mut porta = PortaDaVoz::nova();
    let mut room = Room::new();
    room.adopt(&sessao(700), "marcela");
    room.enter_voice_room(SALA);
    decidida(&mut room);
    olhar_e_esperar(&mut porta, &voz, &room, 700).await;
    assert!(ha_voz(&voz));

    // A volta dentro da carência: o servidor devolve o assento e o `ssrc`
    // (o mesmo 700, numa sessão nova), e o motor refaz a entrada. O canal de mídia é novo, e
    // a voz reabre nele.
    readotar_na_volta(&mut room, &sessao_numero(700, 701), "marcela", Some(SALA));
    porta.na_volta(&voz, |_velha| entregue(Ok(Ok(voz_de(701)))), nada_abre);
    olhar_e_esperar(&mut porta, &voz, &room, 701).await;

    assert_eq!(
        voz.lock().expect("a voz").as_ref().map(|viva| viva.ssrc),
        Some(701),
        "quem caiu e voltou numa sala ficou sem a voz da conexão nova: o \
             motor a sentou de novo, e o áudio fechou como se ela tivesse saído"
    );
    assert_eq!(voz.abertura(), AberturaDoAudio::Aberto);

    // M3: a sala tem senha, e a entrada refeita sem ela é recusada. O
    // servidor manteve a pessoa sentada, e a voz continua.
    room.apply(&recusa(AlertReason::VoiceRoomEntryRefused));
    decidida(&mut room);
    olhar_e_esperar(&mut porta, &voz, &room, 701).await;
    assert!(
        ha_voz(&voz),
        "M3: a entrada refeita numa sala com senha foi recusada, o servidor manteve a \
             pessoa sentada, e o áudio fechou: surda e muda numa sala em que todos a veem"
    );
    assert_eq!(room.current_voice_room, Some(SALA));

    // Fora de sala, sem voz: a volta não abre aparelho nenhum.
    let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
    let mut porta = PortaDaVoz::nova();
    let mut room = Room::new();
    room.adopt(&sessao(700), "marcela");
    olhar_e_esperar(&mut porta, &voz, &room, 700).await;

    readotar_na_volta(&mut room, &sessao(701), "marcela", None);
    porta.na_volta(&voz, |_| panic!("não havia voz para reabrir"), nada_abre);
    let aberturas = olhar_e_esperar(&mut porta, &voz, &room, 701).await;

    assert_eq!(
        aberturas, 0,
        "a volta de quem estava fora de sala abriu o áudio"
    );
    assert!(!ha_voz(&voz));
    assert_eq!(voz.abertura(), AberturaDoAudio::ForaDeSala);
}

/// Item 1 do lote V: soltar a tecla de falar durante a reabertura da volta
/// fecha a fala na voz nova.
///
/// A volta tira a voz do lugar enquanto ela reabre (`PortaDaVoz::na_volta`,
/// até o prazo do áudio), e `Voice::reopen` carrega a tecla da velha. A
/// janela manda a soltura uma vez só: perdida ali, a voz nova entrava com a
/// tecla segura e transmitia até a próxima pressão. `segurar_a_tecla` é o
/// que `Connection::set_talking` chama.
#[tokio::test]
async fn soltar_a_tecla_durante_a_reabertura_da_volta_fecha_a_fala() {
    let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
    let mut porta = PortaDaVoz::nova();
    let mut room = Room::new();
    room.adopt(&sessao(700), "marcela");
    room.enter_voice_room(SALA);
    decidida(&mut room);
    olhar_e_esperar(&mut porta, &voz, &room, 700).await;

    // Ela fala segurando a tecla.
    voz.segurar_a_tecla(true);
    assert_eq!(
        tecla_de(&voz),
        Some(true),
        "a tecla apertada com a voz viva não chegou a ela: a pessoa segura a tecla \
             de falar e ninguém a ouve"
    );

    // A rede cai e volta: a voz sai do lugar e reabre na conexão nova.
    readotar_na_volta(&mut room, &sessao_numero(700, 701), "marcela", Some(SALA));
    porta.na_volta(
        &voz,
        |velha| entregue(Ok(Ok(reaberta(&velha, 701)))),
        nada_abre,
    );
    assert!(
        !ha_voz(&voz),
        "a voz não saiu do lugar durante a reabertura, e o teste mediu outra coisa"
    );

    // E solta a tecla no meio da reabertura.
    voz.segurar_a_tecla(false);
    let aberta = tokio::time::timeout(Duration::from_secs(5), porta.chegada())
        .await
        .expect("a reabertura não chegou");
    porta.chegou(&voz, aberta, nada_abre);

    assert_eq!(
        voz.lock().expect("a voz").as_ref().map(|viva| viva.ssrc),
        Some(701),
        "a voz reaberta na conexão nova não entrou no lugar: quem caiu e voltou fica \
             sem voz, e a tecla abaixo é medida noutra"
    );
    assert_eq!(
        tecla_de(&voz),
        Some(false),
        "a pessoa soltou a tecla de falar durante a reabertura da volta e a voz nova \
             entrou com ela segura: o microfone transmite até a próxima pressão, com o \
             roster dizendo que ela fala"
    );

    // Quem segura a tecla pela volta inteira continua falando na voz nova.
    voz.segurar_a_tecla(true);
    porta.na_volta(
        &voz,
        |velha| entregue(Ok(Ok(reaberta(&velha, 702)))),
        nada_abre,
    );
    let aberta = tokio::time::timeout(Duration::from_secs(5), porta.chegada())
        .await
        .expect("a segunda reabertura não chegou");
    porta.chegou(&voz, aberta, nada_abre);
    assert_eq!(
        tecla_de(&voz),
        Some(true),
        "quem segurava a tecla pela volta inteira voltou calado: a voz nova soltou \
             uma tecla que a pessoa não soltou"
    );
}

/// Rodada 1 do lote V, achado 1: na volta, a voz velha sai do lugar calada e
/// com a tecla solta.
///
/// Ela segue viva até a reabertura voltar — a vez do driver e a abertura do
/// aparelho —, mandando pelo canal da conexão que caiu, que ninguém fecha e
/// que fica de pé até o prazo de ociosidade. Fora do lugar, nenhuma
/// escolha a alcança: a soltura e o mudo pedidos durante a reabertura vão
/// só para a guardada. É a medida do c6 da revisão: a reaberta chegava a
/// `instalar` com a tecla segura, carregada da velha.
#[tokio::test]
async fn na_volta_a_voz_velha_sai_do_lugar_calada_e_com_a_tecla_solta() {
    use std::sync::Arc;
    let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
    let mut porta = PortaDaVoz::nova();
    let mut room = Room::new();
    room.adopt(&sessao(700), "marcela");
    room.enter_voice_room(SALA);
    decidida(&mut room);
    olhar_e_esperar(&mut porta, &voz, &room, 700).await;
    voz.segurar_a_tecla(true);

    // A volta: a velha vai para a reabertura, e o teste a guarda para olhar.
    let velha_aqui: Arc<Mutex<Option<Voz>>> = Arc::default();
    let guardar = Arc::clone(&velha_aqui);
    readotar_na_volta(&mut room, &sessao_numero(700, 701), "marcela", Some(SALA));
    porta.na_volta(
        &voz,
        move |velha| {
            let nova = reaberta(&velha, 701);
            *guardar.lock().expect("a velha") = Some(velha);
            entregue(Ok(Ok(nova)))
        },
        nada_abre,
    );

    // Durante a reabertura, a pessoa solta a tecla e pede o mudo.
    voz.segurar_a_tecla(false);
    voz.escolher(|escolhida| escolhida.mudo = true);

    let velha = velha_aqui.lock().expect("a velha").take();
    let Some(velha) = velha else {
        panic!("a volta não entregou a velha à reabertura, e o teste mediu outra coisa");
    };
    assert!(
        !*velha.tecla.lock().expect("a tecla da velha"),
        "a voz velha seguiu com a tecla segura durante a reabertura: ela transmite pela \
             conexão que caiu depois de a pessoa soltar a tecla"
    );
    assert!(
        velha
            .vestida
            .lock()
            .expect("o que a velha vestiu")
            .as_ref()
            .is_some_and(|vestida| vestida.mudo),
        "a voz velha seguiu sem mudo durante a reabertura: quem se mutou enquanto ela \
             reabria continua no ar pela conexão que caiu"
    );

    let aberta = tokio::time::timeout(Duration::from_secs(5), porta.chegada())
        .await
        .expect("a reabertura não chegou");
    if let Ok(Ok(nova)) = &aberta {
        assert!(
            !*nova.tecla.lock().expect("a tecla da reaberta"),
            "a reaberta chegou a `instalar` com a tecla segura, carregada da velha (c6 \
                 da revisão)"
        );
    }
    porta.chegou(&voz, aberta, nada_abre);
    assert_eq!(
        tecla_de(&voz),
        Some(false),
        "a voz que entrou no lugar está com a tecla segura que a pessoa soltou"
    );
    let mudo = voz.lock().expect("a voz").as_ref().and_then(|viva| {
        viva.vestida
            .lock()
            .expect("o que a voz vestiu")
            .as_ref()
            .map(|vestida| vestida.mudo)
    });
    assert_eq!(
        mudo,
        Some(true),
        "o mudo pedido durante a reabertura não chegou à voz que entrou no lugar"
    );
}

/// Rodada 1 do lote V, achado 2: a tecla guardada não atravessa a saída da
/// sala.
///
/// A janela perde soltura — numa recarga com a tecla segura, ou num
/// `invoke` que falha, porque `segurarFala` grava `falando` e engole o erro.
/// Antes do lote, toda voz aberta do zero nascia com a tecla solta; com a
/// tecla guardada, a soltura perdida entrava na sala seguinte e o microfone
/// abria transmitindo nela. É o c7 da revisão.
#[tokio::test]
async fn a_tecla_segura_nao_atravessa_a_saida_da_sala() {
    let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
    let mut porta = PortaDaVoz::nova();
    let mut room = Room::new();
    room.adopt(&sessao(700), "marcela");
    room.enter_voice_room(SALA);
    decidida(&mut room);
    olhar_e_esperar(&mut porta, &voz, &room, 700).await;

    // Segura a tecla, e a soltura nunca chega.
    voz.segurar_a_tecla(true);
    room.leave_voice_room();
    olhar_e_esperar(&mut porta, &voz, &room, 700).await;
    assert!(!ha_voz(&voz), "sair da sala não fechou a voz");

    room.enter_voice_room(SALA);
    decidida(&mut room);
    olhar_e_esperar(&mut porta, &voz, &room, 701).await;
    assert_eq!(
        tecla_de(&voz),
        Some(false),
        "a soltura que a janela perdeu numa sala entrou na seguinte: o microfone abre \
             transmitindo, sem a pessoa ter apertado nada nesta sala"
    );

    // E a pressão no «ABRINDO» da entrada seguinte vem depois de entrar:
    // continua valendo.
    room.leave_voice_room();
    olhar_e_esperar(&mut porta, &voz, &room, 701).await;
    room.enter_voice_room(SALA);
    decidida(&mut room);
    porta.olhar(&voz, sala_do_audio(&room), |_| {
        entregue(Ok(Ok(voz_de(702))))
    });
    assert_eq!(
        voz.abertura(),
        AberturaDoAudio::Abrindo,
        "entrar de novo na sala não começou a abrir o áudio: a pessoa volta sem voz"
    );
    voz.segurar_a_tecla(true);
    let aberta = tokio::time::timeout(Duration::from_secs(5), porta.chegada())
        .await
        .expect("a abertura não chegou");
    porta.chegou(&voz, aberta, nada_abre);
    assert_eq!(
        tecla_de(&voz),
        Some(true),
        "a tecla apertada no «ABRINDO» da sala seguinte foi esquecida: a pessoa segura a \
             tecla e ninguém a ouve"
    );
}

/// Item 1 do lote V, a variante: apertar a tecla de falar enquanto a
/// primeira abertura está no «ABRINDO» vale quando a voz chega.
#[tokio::test]
async fn apertar_a_tecla_durante_a_primeira_abertura_vale_quando_a_voz_chega() {
    let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
    let mut porta = PortaDaVoz::nova();
    porta.olhar(&voz, Some(SALA), |_| entregue(Ok(Ok(voz_de(700)))));
    assert_eq!(
        voz.abertura(),
        AberturaDoAudio::Abrindo,
        "entrar na sala não começou a abrir o áudio, e a pressão abaixo não cai no \
             «ABRINDO»"
    );

    voz.segurar_a_tecla(true);
    let aberta = porta.chegada().await;
    porta.chegou(&voz, aberta, nada_abre);

    assert_eq!(
        tecla_de(&voz),
        Some(true),
        "a pessoa apertou a tecla de falar no «ABRINDO» e a voz abriu com ela solta: \
             ela segura a tecla e ninguém a ouve até soltar e apertar de novo"
    );
}

/// Item 1 do lote V, pela conexão: sem voz viva, `set_talking` guarda a
/// tecla em vez de jogá-la fora.
#[test]
fn sem_voz_viva_a_conexao_guarda_a_tecla_de_falar() {
    let (commands, _fila) = tokio::sync::mpsc::unbounded_channel();
    let connection = super::Connection {
        commands,
        shared: compartilhado(true),
        encerrada: Mutex::new(None),
        ligacao: None,
    };

    connection.set_talking(true);
    assert!(
        connection.shared.voice.escolhida().tecla_segura,
        "`set_talking(true)` sem voz viva foi jogado fora: quem aperta a tecla \
             enquanto a voz abre fala para ninguém"
    );
    connection.set_talking(false);
    assert!(
        !connection.shared.voice.escolhida().tecla_segura,
        "`set_talking(false)` sem voz viva foi jogado fora: a voz que abrir \
             transmite com a tecla solta"
    );
}

/// A voz de verdade veste cada controle guardado, e segura a tecla de falar.
///
/// Os testes acima provam, com a voz de mentira, **quando** a voz veste; o
/// que ela veste mora em `impl VozViva for Voice`, que pede placa de som.
/// Lido do fonte, campo por campo de `ControlesGuardados`: um campo que o
/// `vestir` de verdade esquecesse seria guardado, mostrado na tela e nunca
/// posto na voz. Os aparelhos ficam de fora, porque valem ao abrir e não ao
/// vestir.
#[test]
fn a_voz_de_verdade_veste_cada_controle_guardado() {
    let fonte = include_str!("../lib.rs");
    let programa = fonte.split("#[cfg(test)]").next().unwrap_or(fonte);
    let bloco = |abertura: &str| {
        programa
            .split(abertura)
            .nth(1)
            .and_then(|resto| resto.split("\n}\n").next())
            .unwrap_or_else(|| panic!("`{abertura}` sumiu de `lib.rs`"))
            .to_owned()
    };
    let campos: Vec<String> = bloco("struct ControlesGuardados {")
        .lines()
        .map(str::trim)
        .filter(|linha| !linha.starts_with("//"))
        .filter_map(|linha| linha.split_once(':').map(|(nome, _)| nome.to_owned()))
        .collect();
    assert!(
        campos.len() >= 6,
        "o guarda não achou os campos de `ControlesGuardados`, e um verde dele não diz \
             nada: {campos:?}"
    );
    let voz = bloco("impl VozViva for Voice {");
    for campo in campos
        .iter()
        .filter(|campo| !["aparelhos", "tecla_segura"].contains(&campo.as_str()))
    {
        assert!(
            voz.contains(&format!("escolhida.{campo}")),
            "a voz de verdade não veste `{campo}`: a escolha é guardada, a tela a \
                 mostra, e a voz que abre fica no padrão"
        );
    }
    assert!(
        voz.contains("self.set_key_held(segura)"),
        "a voz de verdade não segura a tecla de falar que a porta manda: a tecla \
             guardada nunca chega ao microfone"
    );
}

/// M-2 do lote V: o volume e o mudo do som da tela atravessam a saída e a
/// entrada na sala.
///
/// Moravam só na voz, e com a pendência 52 a voz nasce a cada entrada: sair
/// e entrar de novo devolvia o som da transmissão a 100 e audível a quem o
/// tinha calado. `escolher_o_som_da_tela` é o que
/// `Connection::ajustar_volume_da_tela` e `Connection::calar_a_tela` chamam.
#[tokio::test]
async fn o_som_da_tela_atravessa_a_saida_e_a_entrada_na_sala() {
    let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
    let mut porta = PortaDaVoz::nova();
    let mut room = Room::new();
    room.adopt(&sessao(700), "marcela");
    room.enter_voice_room(SALA);
    decidida(&mut room);
    olhar_e_esperar(&mut porta, &voz, &room, 700).await;

    voz.escolher_o_som_da_tela(|escolhida| escolhida.volume_da_tela = 0.3)
        .expect("o volume da tela, com a voz de pé");
    voz.escolher_o_som_da_tela(|escolhida| escolhida.tela_calada = true)
        .expect("o mudo da tela, com a voz de pé");

    room.leave_voice_room();
    olhar_e_esperar(&mut porta, &voz, &room, 700).await;
    assert!(!ha_voz(&voz), "sair da sala não fechou a voz");
    room.enter_voice_room(SALA);
    decidida(&mut room);
    olhar_e_esperar(&mut porta, &voz, &room, 701).await;

    let vestida = voz
        .lock()
        .expect("a voz")
        .as_ref()
        .and_then(|viva| viva.vestida.lock().expect("o que a voz vestiu").clone());
    let Some(vestida) = vestida else {
        panic!("a voz da sala não abriu, e este teste mediu outra coisa");
    };
    assert!(
        (vestida.volume_da_tela - 0.3).abs() < f32::EPSILON,
        "saiu da sala e entrou de novo, e o som da transmissão voltou a {} com a \
             pessoa o tendo deixado em 0.3",
        vestida.volume_da_tela
    );
    assert!(
        vestida.tela_calada,
        "saiu da sala e entrou de novo, e o som da transmissão voltou audível a quem o \
             tinha calado"
    );
}

/// M-2 do lote V, pela conexão: sem voz viva — a volta reabrindo, a sala
/// abrindo —, o som da tela é guardado, e só a sessão sem áudio o recusa.
#[test]
fn sem_voz_viva_a_conexao_guarda_o_som_da_tela() {
    let (commands, _fila) = tokio::sync::mpsc::unbounded_channel();
    let connection = super::Connection {
        commands,
        shared: compartilhado(true),
        encerrada: Mutex::new(None),
        ligacao: None,
    };
    assert_eq!(
        connection.ajustar_volume_da_tela(0.3),
        Ok(()),
        "sem voz viva, o volume da tela foi recusado e jogado fora: arrastado durante \
             a volta, ele se perde"
    );
    assert_eq!(
        connection.calar_a_tela(true),
        Ok(()),
        "sem voz viva, o mudo da tela foi recusado e jogado fora"
    );
    let escolhida = connection.shared.voice.escolhida();
    assert!(
        (escolhida.volume_da_tela - 0.3).abs() < f32::EPSILON && escolhida.tela_calada,
        "o som da tela aceito sem voz viva não ficou guardado: a voz que abrir toca a \
             transmissão a 100 e audível"
    );

    let (commands, _fila) = tokio::sync::mpsc::unbounded_channel();
    let sem_audio = super::Connection {
        commands,
        shared: compartilhado(false),
        encerrada: Mutex::new(None),
        ligacao: None,
    };
    assert_eq!(
        sem_audio.ajustar_volume_da_tela(0.3),
        Err(crate::ConnectionError::NotConnected),
        "numa sessão sem áudio o volume da tela foi aceito, e nenhuma voz vai tocá-lo"
    );
}

/// Achado 6 da revisão do lote V: sem voz viva, `som_da_tela` mostra o que
/// ficou guardado.
///
/// O M-2 passou a gravar o som da tela sem voz, e `som_da_tela` continuava
/// `None` ali: a janela relê depois de SILENCIAR (`tela-chamada.js`,
/// `desenharSomDoPalco`), não acha nada, e o botão não muda — enquanto a voz
/// que abrir depois abre calada. A sessão sem áudio continua `None`, como
/// em `controles_da_voz`.
#[test]
fn sem_voz_viva_o_som_da_tela_mostra_o_que_ficou_guardado() {
    let (commands, _fila) = tokio::sync::mpsc::unbounded_channel();
    let connection = super::Connection {
        commands,
        shared: compartilhado(true),
        encerrada: Mutex::new(None),
        ligacao: None,
    };
    connection
        .calar_a_tela(true)
        .expect("o mudo da tela, sem voz");
    assert_eq!(
        connection.som_da_tela(),
        Some(super::SomDaTela {
            volume: 1.0,
            calada: true
        }),
        "SILENCIAR sem voz viva gravou o mudo da tela, e o botão relê nada: ele segue \
             dizendo SILENCIAR, e a voz que abrir depois abre calada"
    );
    connection
        .ajustar_volume_da_tela(0.3)
        .expect("o volume da tela, sem voz");
    assert_eq!(
        connection.som_da_tela(),
        Some(super::SomDaTela {
            volume: 0.3,
            calada: true
        }),
        "o volume da tela guardado sem voz viva não chega à janela: o controle volta \
             ao que era, e a voz abre no guardado"
    );

    let (commands, _fila) = tokio::sync::mpsc::unbounded_channel();
    let sem_audio = super::Connection {
        commands,
        shared: compartilhado(false),
        encerrada: Mutex::new(None),
        ligacao: None,
    };
    assert_eq!(
        sem_audio.som_da_tela(),
        None,
        "numa sessão sem áudio o som da tela aparece para ajustar, e nenhuma voz vai \
             tocá-lo"
    );
}

/// Achado 6, os dois de acordo: o botão que a janela desenha sem voz e a voz
/// que abre depois dizem o mesmo som da tela.
#[tokio::test]
async fn o_botao_sem_voz_e_a_voz_que_abre_dizem_o_mesmo_som_da_tela() {
    let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
    let mut porta = PortaDaVoz::nova();
    let mut room = Room::new();
    room.adopt(&sessao(700), "marcela");
    olhar_e_esperar(&mut porta, &voz, &room, 700).await;

    voz.escolher_o_som_da_tela(|escolhida| escolhida.tela_calada = true)
        .expect("o mudo da tela, fora de sala");
    voz.escolher_o_som_da_tela(|escolhida| escolhida.volume_da_tela = 0.3)
        .expect("o volume da tela, fora de sala");
    let botao = voz.som_da_tela_escolhido();

    room.enter_voice_room(SALA);
    decidida(&mut room);
    olhar_e_esperar(&mut porta, &voz, &room, 700).await;
    let vestida = voz.lock().expect("a voz").as_ref().and_then(|viva| {
        viva.vestida
            .lock()
            .expect("o que a voz vestiu")
            .as_ref()
            .map(|vestida| super::SomDaTela {
                volume: vestida.volume_da_tela,
                calada: vestida.tela_calada,
            })
    });
    assert!(
        vestida.is_some(),
        "a voz da sala não abriu, e este teste mediu outra coisa"
    );
    assert_eq!(
        botao, vestida,
        "o botão que a janela desenha sem voz e a voz que abre depois discordam do som \
             da tela: a pessoa vê SILENCIAR e a transmissão abre calada"
    );
}

/// Uma abertura em curso na hora da volta abria sobre a conexão que caiu, e
/// recomeça na nova.
#[tokio::test]
async fn a_abertura_em_curso_na_volta_recomeca_na_conexao_nova() {
    let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
    let mut porta = PortaDaVoz::nova();
    porta.olhar(&voz, Some(SALA), |_| {
        Box::pin(std::future::pending::<Aberta<Voz>>())
    });
    assert_eq!(voz.abertura(), AberturaDoAudio::Abrindo);

    let mut recomecou = false;
    porta.na_volta(
        &voz,
        |_| panic!("não havia voz viva para reabrir"),
        |_| {
            recomecou = true;
            entregue(Ok(Ok(voz_de(701))))
        },
    );
    assert!(
        recomecou,
        "a abertura que corria sobre a conexão que caiu não recomeçou na nova"
    );
    let aberta = tokio::time::timeout(Duration::from_secs(5), porta.chegada())
        .await
        .expect("a abertura recomeçada não chegou");
    porta.chegou(&voz, aberta, nada_abre);
    assert_eq!(
        voz.lock().expect("a voz").as_ref().map(|viva| viva.ssrc),
        Some(701)
    );
}

/// Um aparelho escolhido enquanto a voz abria vale: a abertura que chega com
/// o pedido velho recomeça com o novo.
#[tokio::test]
async fn o_aparelho_escolhido_enquanto_a_voz_abre_nao_se_perde() {
    let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
    let mut porta = PortaDaVoz::nova();
    porta.olhar(&voz, Some(SALA), |_| entregue(Ok(Ok(voz_de(1)))));

    voz.guardar_aparelho(super::Lado::Entrada, Some("fone".into()));

    let aberta = porta.chegada().await;
    let mut pedido = None;
    porta.chegou(&voz, aberta, |pedidos| {
        pedido = Some(pedidos.clone());
        entregue(Ok(Ok(voz_de(2))))
    });
    assert_eq!(
        pedido.and_then(|pedidos| pedidos.capture),
        Some("fone".to_owned()),
        "a voz que abriu no microfone de antes ficou no lugar: a tela diz \
             ESCOLHIDO num e EM USO noutro"
    );
    assert!(!ha_voz(&voz), "a voz do pedido velho ficou no lugar");
    let aberta = porta.chegada().await;
    porta.chegou(&voz, aberta, nada_abre);
    assert_eq!(
        voz.lock().expect("a voz").as_ref().map(|viva| viva.ssrc),
        Some(2)
    );
}

/// M-b: quem confere o aparelho é `instalar`, com o cadeado da voz preso.
///
/// A corrida em si — uma troca entre a conferência e o cadeado — não tem
/// como ser forçada num teste sem uma costura no meio. O que se prende aqui
/// é que a conferência mora dentro de `instalar`: uma voz aberta com os
/// aparelhos de antes volta no `Err`, e o lugar fica vazio.
#[test]
fn instalar_recusa_a_voz_aberta_com_os_aparelhos_de_antes() {
    let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
    let pedidos = voz.escolhida().aparelhos;
    voz.guardar_aparelho(super::Lado::Entrada, Some("fone".into()));

    let devolvida = voz.instalar(voz_de(1), &pedidos, Origem::Nova);

    assert!(
        devolvida.is_err(),
        "`instalar` pôs no lugar uma voz aberta no microfone de antes da escolha"
    );
    assert!(!ha_voz(&voz));
    assert_eq!(
        voz.instalar(voz_de(2), &voz.escolhida().aparelhos, Origem::Nova)
            .map_err(|_| ()),
        Ok(()),
        "a voz aberta com os aparelhos de agora não entrou no lugar"
    );
    assert!(ha_voz(&voz));
}

/// O prazo vencido e o aparelho recusado têm estados próprios, e a sala
/// segue.
#[tokio::test]
async fn o_audio_que_nao_abre_deixa_o_estado_dito() {
    for (aberta, estado) in [
        (Err(super::Atraso), AberturaDoAudio::NaoAbriuNoPrazo),
        (
            Ok(Err("sem microfone".to_owned())),
            AberturaDoAudio::SemAparelho,
        ),
    ] {
        let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
        let mut porta = PortaDaVoz::nova();
        porta.olhar(&voz, Some(SALA), move |_| entregue(aberta));
        let chegou = porta.chegada().await;
        porta.chegou(&voz, chegou, nada_abre);

        assert!(!ha_voz(&voz));
        assert_eq!(
            voz.abertura(),
            estado,
            "o áudio não abriu e o estado não diz por quê: a tela fica sem a frase"
        );

        // E sair da sala volta a «fora de sala», sem frase nenhuma.
        porta.olhar(&voz, None, nada_abre);
        assert_eq!(voz.abertura(), AberturaDoAudio::ForaDeSala);
    }
}

/// Uma sessão que entrou sem áudio não abre aparelho em sala nenhuma.
#[test]
fn a_sessao_sem_audio_nao_abre_aparelho_na_sala() {
    let voz: VozDaSala<Voz> = VozDaSala::nova(false, DeviceChoice::default());
    let mut porta = PortaDaVoz::nova();
    porta.olhar(&voz, Some(SALA), nada_abre);
    assert_eq!(voz.abertura(), AberturaDoAudio::SemAparelho);
}

/// Um `Shared` sem voz de pé, de uma sessão que pediu áudio ou não.
fn compartilhado(quer_audio: bool) -> std::sync::Arc<super::Shared> {
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8};
    std::sync::Arc::new(super::Shared {
        caminho_medido: AtomicU32::new(0),
        link_battery: AtomicBool::new(false),
        link_seconds: AtomicU64::new(0),
        link_attempts: AtomicU32::new(0),
        messages_revision: AtomicU64::new(0),
        fora_do_ar_revision: AtomicU64::new(0),
        icon_revision: AtomicU64::new(0),
        person_icons_revision: AtomicU64::new(0),
        perfis: crate::perfis::DoPerfil::negociado(seele_core::PROTOCOL_VERSION),
        room: Mutex::new(Room::new()),
        listeners: Mutex::new(Vec::new()),
        sessao: super::SessaoId::nova(),
        geracao: std::sync::atomic::AtomicU64::new(1),
        voice: VozDaSala::nova(quer_audio, DeviceChoice::default()),
        media: Mutex::new(None),
        nickname: Mutex::new("marcela".into()),
        link_state: AtomicU8::new(0),
        rtt_micros: AtomicU64::new(0),
        jitter_de_chegada_micros: AtomicU64::new(0),
        caminho: Mutex::new(None),
        impressao_do_servidor: Mutex::new(None),
        signal: AtomicU8::new(0),
        running: AtomicBool::new(true),
        pending_weights: Mutex::new(Vec::new()),
        tuneis: Mutex::new(crate::TabelaDeTuneis::default()),
        limites_da_tela: Mutex::new(None),
        tela_escolhida: Mutex::new(super::EscolhaDeTela::NinguemEscolheu),
        tela_ligada: Mutex::new(seele_core::retrato::TelaLigada::default()),
        fontes_de_tela: Mutex::new(Vec::new()),
        camera: Mutex::new(None),
    })
}

/// M-e: numa sessão que entrou sem áudio, `controles_da_voz` não responde,
/// e a casca lê os controles do disco, como sempre leu.
///
/// Os guardados da sessão são os padrões — nada os aplica a voz nenhuma, e
/// nenhuma tela os muda —, e devolvê-los escondia da configuração a escolha
/// gravada (`controles_da_voz`, em `main.rs`, cai no disco só com `None`).
#[test]
fn a_sessao_sem_audio_deixa_os_controles_da_voz_para_o_disco() {
    let (commands, _fila) = tokio::sync::mpsc::unbounded_channel();
    let connection = super::Connection {
        commands,
        shared: compartilhado(false),
        encerrada: Mutex::new(None),
        ligacao: None,
    };
    assert_eq!(
        connection.controles_da_voz(),
        None,
        "a sessão sem áudio respondeu os controles com os padrões dela, e a tela de \
             configuração deixou de mostrar o que está gravado no disco"
    );
}

/// Fora de sala, cada controle da conexão é guardado, nenhum é recusado, e
/// o retrato mostra o escolhido.
///
/// Antes da pendência 52 a voz estava sempre de pé, e estes comandos falavam
/// com ela. Sem ela: o mudo, o isolamento, o modo, a supressão e a
/// sensibilidade pulavam calados; `controles_da_voz` devolvia `None`; e
/// trocar o microfone devolvia `NoAudioDevice` — que a casca escreve como
/// «o microfone sumiu» a quem só ainda não entrou numa sala.
#[test]
fn fora_de_sala_a_conexao_guarda_os_controles_e_o_retrato_os_mostra() {
    let (commands, _fila) = tokio::sync::mpsc::unbounded_channel();
    let connection = super::Connection {
        commands,
        shared: compartilhado(true),
        encerrada: Mutex::new(None),
        ligacao: None,
    };

    connection.set_muted(true).expect("o mudo fora de sala");
    connection
        .set_total_isolation(true)
        .expect("o isolamento fora de sala");
    connection.set_voice_mode(crate::VoiceMode::VoiceActivated);
    connection.set_supressao_de_ruido(0.25);
    connection.set_abertura_da_voz_dbfs(Some(-48.0));
    assert_eq!(
        connection.set_capture_device(Some("fone".into())),
        Ok(()),
        "escolher o microfone fora de sala foi recusado: a casca diz que o \
             aparelho sumiu a quem só ainda não entrou numa sala"
    );
    assert_eq!(connection.set_playback_device(Some("caixa".into())), Ok(()));

    let retrato = connection.snapshot();
    assert!(
        retrato.muted && retrato.total_isolation,
        "o retrato esqueceu o mudo e o isolamento escolhidos fora de sala: o \
             botão volta solto na tela, e a voz abre sem eles"
    );
    assert_eq!(retrato.voice_mode, crate::VoiceMode::VoiceActivated);
    assert!(!retrato.audio_available);
    assert_eq!(retrato.abertura_do_audio, AberturaDoAudio::ForaDeSala);

    let Some(controles) = connection.controles_da_voz() else {
        panic!("fora de sala não há controles de voz para a tela desenhar");
    };
    assert!((controles.supressao - 0.25).abs() < f32::EPSILON);
    assert!((controles.abertura_dbfs + 48.0).abs() < f32::EPSILON);
    assert!(controles.abertura_escolhida);

    let escolhida = connection.shared.voice.escolhida();
    assert_eq!(escolhida.aparelhos.capture.as_deref(), Some("fone"));
    assert_eq!(escolhida.aparelhos.playback.as_deref(), Some("caixa"));
}

/// F1-microfone-macos: o silêncio digital atravessa a ponte na telemetria,
/// pelo nome que a tela lê, e falso sem voz.
///
/// A tela o lê como `snapshot.telemetry.microfone_entrega_silencio_digital`
/// (`desenharAberturaDoAudio`). Um nome que mudasse deste lado calaria a
/// frase do macOS sem erro em lugar nenhum: o campo chegaria `undefined`.
#[test]
fn o_silencio_digital_chega_na_telemetria_pelo_nome_que_a_tela_le() {
    let (commands, _fila) = tokio::sync::mpsc::unbounded_channel();
    let connection = super::Connection {
        commands,
        shared: compartilhado(true),
        encerrada: Mutex::new(None),
        ligacao: None,
    };
    let retrato = serde_json::to_value(connection.snapshot()).expect("o retrato vira JSON");
    assert_eq!(
        retrato
            .get("telemetry")
            .and_then(|telemetria| telemetria.get("microfone_entrega_silencio_digital")),
        Some(&serde_json::Value::Bool(false)),
        "o retrato não leva `telemetry.microfone_entrega_silencio_digital`, ou o leva \
             verdadeiro sem voz nenhuma: a frase do macOS nunca acende, ou acende fora de \
             sala"
    );
}

/// E com a voz de pé, o retrato lê o silêncio digital da telemetria dela.
///
/// Lido do fonte porque o caminho com voz pede uma [`Voice`] de verdade, e
/// ela pede placa de som. Sem esta linha, um `false` escrito no lugar da
/// leitura deixava o teste de cima verde e a frase do macOS apagada para
/// sempre.
///
/// [`Voice`]: seele_core::Voice
#[test]
fn o_retrato_com_voz_le_o_silencio_digital_da_telemetria_dela() {
    let fonte = include_str!("../lib.rs");
    let programa = fonte.split("#[cfg(test)]").next().unwrap_or(fonte);
    let Some(corpo) = programa
        .split("fn audio_state(&self) -> AudioState {")
        .nth(1)
        .and_then(|resto| resto.split("\n    }\n").next())
    else {
        panic!("`Connection::audio_state` sumiu, e com ela o retrato da voz");
    };
    assert!(
        corpo.contains("silencio_digital: telemetry.local.microfone_entrega_silencio_digital"),
        "com a voz de pé, o retrato não lê o silêncio digital que o laço de voz \
             mediu: o produto sabe que o microfone entrega zero e não conta"
    );
}

/// Acréscimo A do lote I-b: a supressão e a sensibilidade gravadas em disco
/// valem na primeira abertura do áudio.
///
/// A casca aplicava só o modo logo depois do `connect`; a supressão e a
/// sensibilidade do disco ficavam de fora, e a sessão começava nos padrões
/// até a pessoa mexer — com a tela de configuração, que lê o disco fora de
/// sessão, mostrando outra coisa. `vestir_com_o_disco` é o que `main.rs`
/// chama ali (por [`super::Connection::vestir_as_escolhas_gravadas`]), e o
/// guarda `a_casca_veste_a_sessao_com_o_disco_logo_depois_do_connect`, em
/// `frontend.rs`, prende a chamada.
#[tokio::test]
async fn a_supressao_e_a_sensibilidade_gravadas_valem_na_primeira_abertura() {
    use seele_core::preferences::Preferences;

    let pasta = std::env::temp_dir().join(format!(
        "seele-ffi-escolhas-gravadas-{}",
        std::process::id()
    ));
    let arquivo = pasta.join("preferencias");
    {
        let mut ajustes = Preferences::open(arquivo.clone()).expect("as preferências");
        ajustes
            .set_supressao_de_ruido(Some(0.25))
            .expect("grava a supressão");
        ajustes
            .set_abertura_da_voz_dbfs(Some(-48.0))
            .expect("grava a sensibilidade");
        ajustes
            .set_voice_mode(Some(VoiceMode::VoiceActivated))
            .expect("grava o modo");
    }
    // Lidas de novo do disco, como a casca as lê a cada conexão.
    let do_disco = Preferences::open(arquivo).expect("relê as preferências");
    let _ = std::fs::remove_dir_all(&pasta);

    let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
    let mut porta = PortaDaVoz::nova();
    let mut room = Room::new();
    room.adopt(&sessao(700), "marcela");
    // O aperto de mão, e logo depois o que a casca faz com o disco.
    olhar_e_esperar(&mut porta, &voz, &room, 700).await;
    super::vestir_com_o_disco(&voz, &do_disco);

    room.enter_voice_room(SALA);
    decidida(&mut room);
    olhar_e_esperar(&mut porta, &voz, &room, 700).await;

    let vestida = voz
        .lock()
        .expect("a voz")
        .as_ref()
        .and_then(|viva| viva.vestida.lock().expect("o que a voz vestiu").clone());
    let Some(vestida) = vestida else {
        panic!("a voz da sala não abriu, e este teste mediu outra coisa");
    };
    assert!(
        (vestida.supressao - 0.25).abs() < f32::EPSILON,
        "a supressão gravada em disco não valeu na primeira abertura: a pessoa a \
             baixou para 25% e a sessão abre com a redução inteira (vestiu {})",
        vestida.supressao
    );
    assert_eq!(
        vestida.abertura_dbfs,
        Some(-48.0),
        "a sensibilidade gravada em disco não valeu na primeira abertura: a ativação \
             por voz abre no padrão, e não onde a pessoa a deixou"
    );
    assert_eq!(
        vestida.modo,
        VoiceMode::VoiceActivated,
        "o modo gravado em disco deixou de valer no connect"
    );
}

// ---- o volume de cada pessoa (F1-vazamento, passo 3 (2), e acréscimo B) ----

/// A Rafa sentada na sala, com o `ssrc` que o servidor deu a ela nesta
/// conexão dela.
fn rafa_na_sala(ssrc: u32) -> ServerMessage {
    ServerMessage::PersonJoined {
        voice_room: SALA,
        profile: PersonProfile {
            id: PersonId(11),
            nickname: "rafa".into(),
            roles: Vec::new(),
            material_e2ee: None,
        },
        ssrc: Ssrc(ssrc),
    }
}

/// O ganho que a voz viva tem para este número, se ela existe e tem um.
fn ganho_de(voz: &VozDaSala<Voz>, ssrc: u32) -> Option<f32> {
    voz.lock().expect("a voz").as_ref().and_then(|viva| {
        viva.ganhos
            .lock()
            .expect("os ganhos da voz")
            .get(&ssrc)
            .copied()
    })
}

/// Uma volta do laço de `drive`, na ordem dele: o olhar do fim da volta, a
/// chegada de uma abertura quando ela começou — o braço do `select!` da
/// volta seguinte —, e os volumes, no fim de cada volta.
async fn volta_do_laco(porta: &mut PortaDaVoz<Voz>, voz: &VozDaSala<Voz>, room: &Mutex<Room>) {
    let sala = sala_do_audio(&room.lock().expect("a sala"));
    porta.olhar(voz, sala, |_| entregue(Ok(Ok(voz_de(700)))));
    if voz.abertura() == AberturaDoAudio::Abrindo {
        let aberta = tokio::time::timeout(Duration::from_secs(5), porta.chegada())
            .await
            .expect("a abertura começou e não chegou");
        porta.chegou(voz, aberta, nada_abre);
    }
    voz.seguir_os_volumes(room);
}

/// A sala da Marcela, com a Rafa sentada nela pelo número `ssrc`.
fn sala_com_a_rafa(ssrc: u32) -> Mutex<Room> {
    let mut room = Room::new();
    room.adopt(&sessao(700), "marcela");
    room.apply(&rafa_na_sala(ssrc));
    Mutex::new(room)
}

/// (b) do passo 3: o volume é por apelido, e vale pelo `ssrc` de agora.
///
/// A Rafa caiu e voltou, ou trocou de rede: o servidor a reapresenta com um
/// número novo. A voz só conhece números, e o volume de 150 que a Marcela
/// escolheu para ela tem de ir para o número novo.
#[test]
fn o_volume_volta_quando_o_ssrc_muda() {
    let room = sala_com_a_rafa(900);
    let volumes = HashMap::from([("rafa".to_owned(), 150_u16)]);
    assert_eq!(
        ganhos_a_aplicar(&volumes, &room.lock().expect("a sala")),
        vec![(Ssrc(900), 1.5)],
        "o volume de 150 da Rafa não virou o ganho 1.5 no número dela"
    );

    room.lock().expect("a sala").apply(&rafa_na_sala(901));
    assert_eq!(
        ganhos_a_aplicar(&volumes, &room.lock().expect("a sala")),
        vec![(Ssrc(901), 1.5)],
        "a Rafa voltou com outro `ssrc` e o volume dela ficou no número velho: a \
             janela mostra 150 e ela toca a 100"
    );

    // Quem não está aqui não ganha número nenhum, e o nome não depende de
    // maiúscula, como em `Room::ssrc_of`.
    let volumes = HashMap::from([("RAFA".to_owned(), 50_u16), ("lia".to_owned(), 200)]);
    assert_eq!(
        ganhos_a_aplicar(&volumes, &room.lock().expect("a sala")),
        vec![(Ssrc(901), 0.5)],
        "o volume de quem não está aqui virou ganho de alguém, ou o apelido passou a \
             depender de maiúscula"
    );
}

/// O segundo teste do (b): o laço aplica os volumes **na abertura** da voz
/// e **na troca** de `ssrc`, e só quando algo mudou.
///
/// `volta_do_laco` é a volta de `drive`; o guarda
/// `os_volumes_sao_seguidos_no_fim_de_cada_volta_de_drive` prende que
/// `drive` faz o mesmo.
#[tokio::test]
async fn o_laco_aplica_os_volumes_na_abertura_e_na_troca_de_ssrc() {
    let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
    let mut porta = PortaDaVoz::nova();
    let room = sala_com_a_rafa(900);
    volta_do_laco(&mut porta, &voz, &room).await;

    // Fora de sala: só grava.
    voz.guardar_volume("rafa", 150);
    assert!(
        !voz.seguir_os_volumes(&room),
        "fora de sala não há voz, e mesmo assim algum ganho foi posto"
    );

    {
        let mut sala = room.lock().expect("a sala");
        sala.enter_voice_room(SALA);
        decidida(&mut sala);
    }
    volta_do_laco(&mut porta, &voz, &room).await;
    assert_eq!(
        ganho_de(&voz, 900),
        Some(1.5),
        "a voz abriu e o volume escolhido fora de sala não foi posto nela: a janela \
             diz 150 e a Rafa toca a 100"
    );
    assert!(
        !voz.seguir_os_volumes(&room),
        "nada mudou e os ganhos foram postos de novo: o laço reescreve a voz a cada \
             volta"
    );

    room.lock().expect("a sala").apply(&rafa_na_sala(901));
    volta_do_laco(&mut porta, &voz, &room).await;
    assert_eq!(
        ganho_de(&voz, 901),
        Some(1.5),
        "a Rafa voltou com outro `ssrc` e o laço não levou o volume dela para o \
             número novo"
    );
    assert_eq!(
        ganho_de(&voz, 900),
        Some(1.0),
        "o número velho da Rafa ficou com o ganho dela: o volume é da pessoa, e um \
             servidor que reusa o número o daria a outra"
    );
}

/// Acréscimo B do lote I-b (o M-a da revisão do I-a): os volumes voltam ao
/// reabrir a voz.
///
/// Com a pendência 52, quem sai da sala e entra de novo recebe uma voz nova,
/// com ganho 1.0, e a janela continuava mostrando o valor de antes.
#[tokio::test]
async fn o_volume_volta_quando_se_sai_da_sala_e_entra_de_novo() {
    let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
    let mut porta = PortaDaVoz::nova();
    let room = sala_com_a_rafa(900);
    volta_do_laco(&mut porta, &voz, &room).await;
    {
        let mut sala = room.lock().expect("a sala");
        sala.enter_voice_room(SALA);
        decidida(&mut sala);
    }
    volta_do_laco(&mut porta, &voz, &room).await;

    // Dentro da sala, como `Connection::set_volume` faz: grava e aplica.
    voz.guardar_volume("rafa", 150);
    voz.seguir_os_volumes(&room);
    assert_eq!(
        ganho_de(&voz, 900),
        Some(1.5),
        "dentro da sala, o volume escolhido não chegou à voz viva"
    );

    room.lock().expect("a sala").leave_voice_room();
    volta_do_laco(&mut porta, &voz, &room).await;
    assert!(!ha_voz(&voz), "sair da sala não fechou a voz");

    {
        let mut sala = room.lock().expect("a sala");
        sala.enter_voice_room(SALA);
        decidida(&mut sala);
    }
    volta_do_laco(&mut porta, &voz, &room).await;
    assert_eq!(
        ganho_de(&voz, 900),
        Some(1.5),
        "saiu da sala e entrou de novo, e a voz nova abriu com a Rafa a 100 enquanto \
             a janela mostra 150"
    );
}

/// Item 2 do lote V: depois de uma volta, o número que mudou de dono volta
/// a 1.0, e o volume vai com a pessoa para o número novo.
///
/// A volta com o assento resgatado reinstala a voz que `Voice::reopen`
/// abriu, com os ganhos por número da velha. Enquanto ela reabre, a
/// fotografia de ocupação chega: o 900, que era da Rafa, é da Bia, e a Rafa
/// está no 901. `instalar` zerava os aplicados, e `seguir_os_volumes` não
/// sabia que o 900 tinha 1.5: medido pela revisão, `900=Some(1.5)
/// 901=Some(1.5)` — a Bia a 150 com a janela dizendo 100.
///
/// O `seele-server` de hoje não dá um número a outra pessoa no mesmo
/// processo (`Registry::issue` só conta para a frente), e depois de um
/// reinício não há resgate: o olhar do fim da volta fecha a voz, e a que
/// abre é nova. O fio não promete nenhuma das duas coisas.
#[tokio::test]
async fn depois_da_volta_o_numero_que_mudou_de_dono_volta_a_um() {
    let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
    let mut porta = PortaDaVoz::nova();
    let room = sala_com_a_rafa(900);
    {
        let mut sala = room.lock().expect("a sala");
        sala.enter_voice_room(SALA);
        decidida(&mut sala);
    }
    volta_do_laco(&mut porta, &voz, &room).await;
    voz.guardar_volume("rafa", 150);
    voz.seguir_os_volumes(&room);
    assert_eq!(
        ganho_de(&voz, 900),
        Some(1.5),
        "antes da volta, o volume da Rafa não chegou à voz"
    );

    // O braço `Reconectado`, com o assento resgatado, e o fim da volta dele.
    readotar_na_volta(
        &mut room.lock().expect("a sala"),
        &sessao_numero(700, 701),
        "marcela",
        Some(SALA),
    );
    porta.na_volta(
        &voz,
        |velha| entregue(Ok(Ok(reaberta(&velha, 701)))),
        nada_abre,
    );
    let sala = sala_do_audio(&room.lock().expect("a sala"));
    porta.olhar(&voz, sala, nada_abre);
    voz.seguir_os_volumes(&room);

    // A fotografia de ocupação chega enquanto a voz reabre.
    {
        let mut sala = room.lock().expect("a sala");
        sala.apply(&ServerMessage::PersonJoined {
            voice_room: SALA,
            profile: PersonProfile {
                id: PersonId(12),
                nickname: "bia".into(),
                roles: Vec::new(),
                material_e2ee: None,
            },
            ssrc: Ssrc(900),
        });
        sala.apply(&rafa_na_sala(901));
    }
    voz.seguir_os_volumes(&room);

    // A reabertura chega, e o fim da volta segue os volumes.
    let aberta = tokio::time::timeout(Duration::from_secs(5), porta.chegada())
        .await
        .expect("a reabertura não chegou");
    porta.chegou(&voz, aberta, nada_abre);
    voz.seguir_os_volumes(&room);
    let ssrc = voz.lock().expect("a voz").as_ref().map(|viva| viva.ssrc);
    assert_eq!(
        ssrc,
        Some(701),
        "a voz no lugar não é a reaberta, e o teste mediu outra coisa"
    );

    assert_eq!(
        ganho_de(&voz, 901),
        Some(1.5),
        "a Rafa voltou no 901 e o volume dela não foi junto: a janela diz 150 e ela \
             toca a 100"
    );
    assert_eq!(
        ganho_de(&voz, 900),
        Some(1.0),
        "depois da volta, o 900 passou a ser da Bia e ficou com o volume que a Marcela \
             deu à Rafa: a Bia toca a 150 com a janela dizendo 100"
    );
}

/// M-4 da rodada 1 do lote I-b: quem troca de nome leva o volume junto.
///
/// O volume é por apelido, e é isso que o faz sobreviver à volta, quando a
/// pessoa ganha outra sessão e outro `ssrc`. Mas o apelido muda: o servidor
/// avisa com `PersonRenamed { person, nickname }`, e o `Room` sabe o nome de
/// antes pelo mesmo `PersonId`. Antes, a Rafa que virava Rafael voltava a
/// 1,0, e a entrada «rafa» ficava guardada para quem pegasse o apelido
/// depois. A troca passa por `dobrar_no_room`, que é o que `fold` chama.
#[tokio::test]
async fn quem_troca_de_nome_leva_o_volume_junto() {
    let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
    let mut porta = PortaDaVoz::nova();
    let room = sala_com_a_rafa(900);
    {
        let mut sala = room.lock().expect("a sala");
        sala.enter_voice_room(SALA);
        decidida(&mut sala);
    }
    volta_do_laco(&mut porta, &voz, &room).await;
    voz.guardar_volume("rafa", 150);
    voz.seguir_os_volumes(&room);
    assert_eq!(
        ganho_de(&voz, 900),
        Some(1.5),
        "antes da troca de nome, o volume da Rafa não chegou à voz"
    );

    super::dobrar_no_room(
        &room,
        &voz,
        &ServerMessage::PersonRenamed {
            person: PersonId(11),
            nickname: "rafael".into(),
        },
    );
    volta_do_laco(&mut porta, &voz, &room).await;

    assert_eq!(
        ganho_de(&voz, 900),
        Some(1.5),
        "a Rafa trocou de nome para Rafael e o volume dela voltou a 1,0, com a janela \
             mostrando 150"
    );
    assert_eq!(
        voz.volumes(),
        HashMap::from([("rafael".to_owned(), 150)]),
        "o volume não mudou de chave: «rafa» continua guardado, e vale para quem pegar \
             o apelido depois"
    );
}

/// Rodada 2 do lote I-b: quem troca para um nome que tem volume guardado de
/// outra pessoa não herda esse volume.
///
/// A Rafa, sem volume nenhum, vira «lia», e há `lia: 50` de uma Lia que saiu.
/// A troca não mexia em nada quando quem trocou não tinha volume, e o 50 da
/// outra Lia passava a valer para a Rafa — contra o que `renomear_volume`
/// diz de si: é da pessoa que trocou que o servidor está falando.
#[tokio::test]
async fn quem_troca_para_um_nome_com_volume_velho_nao_herda_o_volume() {
    let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
    let mut porta = PortaDaVoz::nova();
    let room = sala_com_a_rafa(900);
    {
        let mut sala = room.lock().expect("a sala");
        sala.enter_voice_room(SALA);
        decidida(&mut sala);
    }
    volta_do_laco(&mut porta, &voz, &room).await;
    // O volume de uma Lia que já saiu, e nenhum para a Rafa.
    voz.guardar_volume("lia", 50);

    super::dobrar_no_room(
        &room,
        &voz,
        &ServerMessage::PersonRenamed {
            person: PersonId(11),
            nickname: "lia".into(),
        },
    );
    volta_do_laco(&mut porta, &voz, &room).await;

    assert_eq!(
        ganho_de(&voz, 900).unwrap_or(1.0),
        1.0,
        "a Rafa trocou de nome para «lia» e passou a tocar com o volume que alguém deu \
             a outra Lia, que já saiu"
    );
    assert!(
        voz.volumes().is_empty(),
        "o volume da outra Lia continua guardado sob o nome que agora é da Rafa: {:?}",
        voz.volumes()
    );
}

/// E pelo caminho de verdade: `fold`, o que `drive` chama a cada mensagem.
#[test]
fn a_troca_de_nome_que_o_servidor_anuncia_move_o_volume_pela_conexao() {
    let (commands, _fila) = tokio::sync::mpsc::unbounded_channel();
    let connection = super::Connection {
        commands,
        shared: compartilhado(true),
        encerrada: Mutex::new(None),
        ligacao: None,
    };
    {
        let mut room = connection.shared.room.lock().expect("a sala");
        room.adopt(&sessao(700), "marcela");
        room.apply(&rafa_na_sala(900));
    }
    connection
        .set_volume("rafa".into(), 150)
        .expect("o volume da Rafa");

    super::fold(
        &connection.shared,
        &ServerMessage::PersonRenamed {
            person: PersonId(11),
            nickname: "rafael".into(),
        },
    );

    assert_eq!(
        connection.volumes_locais(),
        HashMap::from([("rafael".to_owned(), 150)]),
        "`fold` dobrou a troca de nome no `Room` e deixou o volume no nome velho: a \
             janela desenha o Rafael a 100"
    );
    let ganhos = ganhos_a_aplicar(
        &connection.volumes_locais(),
        &connection.shared.room.lock().expect("a sala"),
    );
    assert_eq!(
        ganhos,
        vec![(Ssrc(900), 1.5)],
        "depois da troca de nome, o volume não chega mais ao número da pessoa"
    );
}

/// F1-vazamento, passo 3 (2): o volume mora na conexão, e `volumes_locais`
/// o devolve.
///
/// Na conexão, e não na janela: uma conexão nova nasce sem volume nenhum, e
/// o 150 que alguém deu à Rafa de um servidor não passa à Rafa de outro.
/// Fora de sala o volume só grava, como o mudo e os aparelhos.
#[test]
fn o_volume_mora_na_conexao_e_volumes_locais_o_devolve() {
    let (commands, _fila) = tokio::sync::mpsc::unbounded_channel();
    let connection = super::Connection {
        commands,
        shared: compartilhado(true),
        encerrada: Mutex::new(None),
        ligacao: None,
    };
    {
        let mut room = connection.shared.room.lock().expect("a sala");
        room.adopt(&sessao(700), "marcela");
        room.apply(&rafa_na_sala(900));
    }
    assert!(
        connection.volumes_locais().is_empty(),
        "uma conexão nova já nasce com volumes: eles vieram de outra sessão"
    );

    assert_eq!(
        connection.set_volume("rafa".into(), 150),
        Ok(()),
        "o volume pedido fora de sala foi recusado: a pessoa escolhe antes de entrar \
             e a escolha some"
    );
    assert_eq!(
        connection.volumes_locais(),
        HashMap::from([("rafa".to_owned(), 150)]),
        "o volume escolhido não chega à janela por `volumes_locais`"
    );
    assert_eq!(
        connection.set_volume("ninguém".into(), 50),
        Err(crate::ConnectionError::UnknownPerson),
        "um apelido que não está aqui foi guardado"
    );

    // E nunca pelo retrato: ele vai inteiro aos MODs.
    fn chaves(valor: &serde_json::Value, todas: &mut Vec<String>) {
        match valor {
            serde_json::Value::Object(campos) => {
                for (chave, dentro) in campos {
                    todas.push(chave.clone());
                    chaves(dentro, todas);
                }
            }
            serde_json::Value::Array(itens) => {
                for item in itens {
                    chaves(item, todas);
                }
            }
            _ => {}
        }
    }
    let retrato = serde_json::to_value(connection.snapshot()).expect("o retrato vira JSON");
    let mut todas = Vec::new();
    chaves(&retrato, &mut todas);
    assert!(
        !todas.iter().any(|chave| chave.contains("volume")),
        "o volume de cada pessoa foi parar no retrato, que os MODs leem: quanto esta \
             máquina quer ouvir cada um não é deles ({todas:?})"
    );

    let (commands, _fila) = tokio::sync::mpsc::unbounded_channel();
    let sem_audio = super::Connection {
        commands,
        shared: compartilhado(false),
        encerrada: Mutex::new(None),
        ligacao: None,
    };
    sem_audio
        .shared
        .room
        .lock()
        .expect("a sala")
        .apply(&rafa_na_sala(900));
    // A ordem das duas recusas é a de antes: quem não está aqui é dito
    // primeiro, com ou sem áudio. A conformidade conta com ela
    // (`acceptance_m5::the_volume_of_a_stranger_is_refused_by_name`, que
    // conecta sem áudio).
    assert_eq!(
        sem_audio.set_volume("ninguém".into(), 50),
        Err(crate::ConnectionError::UnknownPerson),
        "numa sessão sem áudio, o volume de quem não está aqui deixou de ser \
             recusado pelo nome: a casca diz «sem áudio» sobre uma pessoa que não existe"
    );
    assert_eq!(
        sem_audio.set_volume("rafa".into(), 150),
        Err(crate::ConnectionError::NoAudioDevice),
        "numa sessão sem áudio o volume foi aceito, e nenhuma voz vai tocá-lo"
    );
}

/// O corpo de `drive`, sem as linhas de comentário, e os três pedaços que a
/// ligação da porta da voz atravessa: antes do laço, o `select!`, e o resto
/// da volta do laço depois dele.
fn pedacos_de_drive() -> (String, String, String, String) {
    let fonte = include_str!("../lib.rs");
    let sem_comentario: String = fonte
        .lines()
        .filter(|linha| !linha.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let inicio = sem_comentario
        .find(&format!("{} fn drive(", "async"))
        .expect("`drive` sumiu de `lib.rs`");
    let depois = &sem_comentario[inicio..];
    let corpo = &depois[..depois.find("\n}\n").expect("`drive` nunca fecha")];
    let (antes_do_laco, laco) = corpo
        .split_once("    while shared.running")
        .expect("o laço de `drive` mudou de forma, e este guarda tem de mudar com ele");
    let (_, select) = laco
        .split_once("tokio::select! {")
        .expect("o `select!` de `drive` sumiu");
    // O `select!` fecha na primeira linha com a indentação dele.
    let (select, resto_da_volta) = select
        .split_once("\n        }\n")
        .expect("o `select!` de `drive` não fecha na indentação de sempre");
    (
        corpo.to_owned(),
        antes_do_laco.to_owned(),
        select.to_owned(),
        resto_da_volta.to_owned(),
    )
}

/// I2 da rodada 1 do lote I-a: a porta da voz está ligada ao laço de
/// `drive`, nos três lugares, e só nos três.
///
/// Os testes acima provam a porta, e nenhum deles vê `drive`. A revisão
/// desfez duas ligações e ficou tudo verde: sem o olhar do fim da volta do
/// laço — o único ponto que abre o áudio ao entrar numa sala — e com um
/// `porta.comecar` logo depois do olhar do aperto de mão, que abre o áudio
/// ao conectar. Este guarda lê o texto de `drive` porque é ali que a ligação
/// mora, e nenhum valor a exibe sem um enlace de verdade.
#[test]
fn a_porta_da_voz_esta_ligada_ao_laco_de_drive() {
    let (corpo, antes_do_laco, select, resto_da_volta) = pedacos_de_drive();
    let olhar = "porta.olhar(&shared.voice, sala_de(&shared),";

    assert_eq!(
        antes_do_laco.matches("porta.").count(),
        1,
        "antes do laço, `drive` mexe na porta da voz mais de uma vez — ou nenhuma: o \
             aperto de mão é um olhar só, com o servidor adotado e ninguém sentado, e \
             qualquer outra coisa ali abre o áudio ao conectar"
    );
    assert!(
        antes_do_laco.contains(olhar),
        "o olhar do aperto de mão sumiu, ou deixou de ler o assento confirmado"
    );
    assert!(
        select.contains("aberta = porta.chegada() =>"),
        "o `select!` de `drive` não espera mais a abertura em curso: a voz abre e nunca \
             chega ao lugar"
    );
    assert!(
        resto_da_volta.contains(olhar),
        "o olhar do fim da volta do laço sumiu: é o único ponto que abre o áudio ao \
             entrar numa sala e o fecha ao sair"
    );
    for chamada in corpo
        .match_indices("porta.")
        .map(|(onde, _)| &corpo[onde..])
    {
        assert!(
            [
                "porta.olhar(",
                "porta.na_volta(",
                "porta.chegada()",
                "porta.chegou("
            ]
            .iter()
            .any(|permitida| chamada.starts_with(permitida)),
            "`drive` chama a porta da voz por fora das quatro portas dela: {}",
            chamada.lines().next().unwrap_or_default()
        );
    }
    for (onde, _) in corpo.match_indices("abrir_a_voz(") {
        let antes = corpo[..onde].trim_end().trim_end_matches('{').trim_end();
        assert!(
            antes.ends_with("|pedidos|"),
            "`drive` chama `abrir_a_voz` fora de uma closure dada à porta: a voz abre \
                 sem a porta decidir"
        );
    }
}

/// Os volumes são seguidos no fim de cada volta do laço de `drive`.
/// F1-vazamento e acréscimo B.
///
/// É dali que eles chegam à voz que acabou de abrir — a chegada da
/// abertura é um braço do `select!`, e a volta termina logo depois — e ao
/// `ssrc` novo de quem voltou, que entra no `Room` por uma mensagem dobrada
/// num braço do mesmo `select!`. Os testes acima provam a decisão com a
/// volta de `volta_do_laco`; este prende que `drive` dá a mesma volta.
#[test]
fn os_volumes_sao_seguidos_no_fim_de_cada_volta_de_drive() {
    let (_, _, _, resto_da_volta) = pedacos_de_drive();
    assert!(
        resto_da_volta.contains("shared.voice.seguir_os_volumes(&shared.room)"),
        "o fim da volta de `drive` não segue mais os volumes: a voz que abre fica \
             com todo mundo a 100, e quem volta com outro `ssrc` perde o volume dele"
    );
}

/// M-4 do lote V: o fim da sessão larga a voz sem esperar o cadeado dela.
///
/// `switch_device` segura o cadeado da voz enquanto o driver abre o aparelho
/// novo. Preso ali, ele segurava o fim de `drive` — e com ele a despedida —
/// até o teto da casca, que dizia «a despedida não terminou» sem dizer por
/// quê. A troca é uma thread que segura o cadeado e só solta quando o
/// teste manda, depois de medir quanto o fim levou.
#[test]
fn o_fim_da_sessao_larga_a_voz_sem_esperar_a_troca_de_aparelho() {
    use std::sync::Arc;
    let voz: Arc<VozDaSala<Voz>> = Arc::new(VozDaSala::nova(true, DeviceChoice::default()));

    *voz.lock().expect("a voz") = Some(voz_de(700));
    super::largar_sem_segurar(&voz, |voz| voz);
    assert!(
        !ha_voz(&voz),
        "com o cadeado livre, o fim da sessão não largou a voz: o microfone fica \
             aberto depois da despedida"
    );

    *voz.lock().expect("a voz") = Some(voz_de(701));
    let (preso, prendeu) = std::sync::mpsc::channel();
    let (solta, soltar) = std::sync::mpsc::channel::<()>();
    let troca = {
        let voz = Arc::clone(&voz);
        std::thread::spawn(move || {
            let mut viva = voz.lock().expect("a voz");
            preso.send(()).expect("a troca avisa que prendeu");
            // O driver demora, e a troca põe a voz nova antes de soltar.
            let _ = soltar.recv_timeout(Duration::from_secs(2));
            *viva = Some(voz_de(702));
        })
    };
    prendeu.recv().expect("a troca prendeu o cadeado");
    super::trilha_no_log::segundo_despachante();
    let espiao = super::trilha_no_log::Espiao::default();
    let antes = std::time::Instant::now();
    tracing::subscriber::with_default(espiao.clone(), || {
        super::largar_sem_segurar(&voz, |voz| voz);
    });
    let levou = antes.elapsed();
    let _ = solta.send(());
    troca.join().expect("a troca terminou");
    assert!(
        levou < Duration::from_millis(500),
        "o fim da sessão esperou {levou:?} pelo cadeado da voz, preso por uma troca de \
             aparelho no driver: a despedida sai depois do teto da casca, e quem ficou só \
             vê a saída pelo prazo de ociosidade"
    );

    let ate = std::time::Instant::now() + Duration::from_secs(5);
    while ha_voz(&voz) && std::time::Instant::now() < ate {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !ha_voz(&voz),
        "a troca de aparelho soltou o cadeado e a voz que ela pôs ficou de pé depois \
             do fim da sessão: o microfone fica aberto sem sessão nenhuma"
    );
    let escrito = espiao.escrito();
    let Some(linha) = escrito
        .iter()
        .find(|linha| linha.contains("o cadeado da voz estava ocupado no fim da sessão"))
    else {
        panic!(
            "o fim da sessão largou a voz por fora e o log não diz: quem lê o `seele.log` \
                 de uma saída lenta não acha o microfone que ficou aberto. Escrito: \
                 {escrito:?}"
        );
    };
    // Rodada 1, achado 3: o cadeado também fica preso por um instante quando
    // alguém só lê a voz — o retrato, o som da tela, os controles —, e a
    // linha não pode afirmar uma causa que ela não sabe.
    assert!(
        !linha.contains("troca de aparelho"),
        "o aviso do fim da sessão diz que uma troca de aparelho prendia a voz sem \
             saber quem a prendia: quem lê o log vai procurar um driver preso que não \
             houve. Linha: {linha}"
    );
}

/// Rodada 1 do lote V, achado 4: a thread que larga a voz também larga com
/// o cadeado envenenado.
///
/// Quem segurava o cadeado entrou em pânico com ele na mão. O `try_lock` já
/// tratava o veneno; a thread de `largar_sem_segurar` não, e a voz ficava de
/// pé depois do fim da sessão, com o microfone aberto.
#[test]
fn a_voz_cai_mesmo_com_o_cadeado_envenenado_na_thread_que_a_larga() {
    use std::sync::Arc;
    let voz: Arc<VozDaSala<Voz>> = Arc::new(VozDaSala::nova(true, DeviceChoice::default()));
    *voz.lock().expect("a voz") = Some(voz_de(700));

    let (preso, prendeu) = std::sync::mpsc::channel();
    let (solta, soltar) = std::sync::mpsc::channel::<()>();
    let quem_segura = {
        let voz = Arc::clone(&voz);
        std::thread::spawn(move || {
            let _viva = voz.lock().expect("a voz");
            preso.send(()).expect("avisa que prendeu");
            let _ = soltar.recv_timeout(Duration::from_secs(2));
            panic!("pânico com o cadeado da voz na mão, de propósito");
        })
    };
    prendeu.recv().expect("o cadeado foi preso");
    super::largar_sem_segurar(&voz, |voz| voz);
    let _ = solta.send(());
    assert!(
        quem_segura.join().is_err(),
        "quem segurava o cadeado não entrou em pânico, e o teste mediu outra coisa"
    );

    let de_pe = || {
        voz.viva
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
    };
    let ate = std::time::Instant::now() + Duration::from_secs(2);
    while de_pe() && std::time::Instant::now() < ate {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !de_pe(),
        "com o cadeado envenenado, a thread que larga a voz desistiu: a voz ficou de pé \
             depois do fim da sessão, com o microfone aberto"
    );
}

/// M-4 do lote V: o fim de `drive` larga a voz pelo caminho que não espera,
/// e antes da despedida.
///
/// O teste acima prova o caminho; este prende que `drive` o usa. Antes da
/// despedida, para o microfone fechar antes de o servidor saber da saída.
#[test]
fn o_fim_de_drive_larga_a_voz_antes_da_despedida_e_sem_esperar() {
    let (corpo, _, _, _) = pedacos_de_drive();
    let Some((_, fim)) = corpo.rsplit_once("shared.running.store(false") else {
        panic!("o fim de `drive` mudou de forma, e este guarda tem de mudar com ele");
    };
    let Some(larga) = fim.find("largar_sem_segurar(&shared,") else {
        panic!(
            "o fim de `drive` não larga mais a voz pelo caminho que não espera: uma \
                 troca de aparelho presa no driver segura a despedida até o teto da casca"
        );
    };
    let Some(sai) = fim.find("client.sair().await") else {
        panic!("a despedida sumiu do fim de `drive`");
    };
    assert!(
        larga < sai,
        "o fim de `drive` larga a voz depois da despedida: o microfone continua aberto \
             enquanto o servidor ainda ouve"
    );
    assert!(
        !fim.contains("shared.voice.lock()"),
        "o fim de `drive` voltou a esperar o cadeado da voz, que uma troca de aparelho \
             no driver segura até o teto da casca"
    );
}

/// Nenhum caminho abre a voz por fora da porta da sala.
///
/// Os testes acima provam a decisão da porta, e nenhum deles vê `drive`:
/// uma abertura escrita de volta logo depois do aperto de mão — onde ela
/// morou até a 0.15 — passaria por todos. Este guarda conta, no programa e
/// sem comentários, quem chama o núcleo para abrir uma voz: uma vez só, e
/// dentro de `abrir_a_voz`, que só a porta chama. A reabertura da volta é
/// `reopen`, que parte da voz que já estava aberta.
#[test]
fn a_voz_so_abre_pela_porta_da_sala() {
    let fonte = include_str!("../lib.rs");
    let programa = fonte.split("#[cfg(test)]").next().unwrap_or(fonte);
    let sem_comentarios: String = programa
        .lines()
        .filter(|linha| !linha.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let abertura = format!("Voice::{}(", "start_preferring");
    assert_eq!(
        sem_comentarios.matches(&abertura).count(),
        1,
        "a voz abre por fora de `abrir_a_voz`: uma abertura no aperto de mão \
             segura o microfone a sessão inteira, dentro de sala ou não"
    );
    for outra in [
        format!("Voice::{}(", "start"),
        format!("Voice::{}(", "start_on"),
    ] {
        assert!(
            !sem_comentarios.contains(&outra),
            "a FFI abre uma voz com `{outra}` por fora da porta da sala"
        );
    }
    let Some(abridor) = sem_comentarios
        .split(&format!("fn {}(", "abrir_a_voz"))
        .nth(1)
        .and_then(|resto| resto.split("\n}\n").next())
    else {
        panic!("`abrir_a_voz` sumiu, e com ela a única porta da voz");
    };
    assert!(
        abridor.contains(&abertura),
        "a única abertura da voz não está em `abrir_a_voz`"
    );
}

/// A sessão que o aperto de mão entrega. O `ssrc` muda a cada conexão.
fn sessao(ssrc: u32) -> SessionInfo {
    sessao_numero(ssrc, u64::from(ssrc))
}

/// A sessão com o número que o teste escolhe. O resgate de assento é o
/// mesmo `ssrc` numa sessão de número maior.
fn sessao_numero(ssrc: u32, numero: u64) -> SessionInfo {
    SessionInfo {
        id: SessionId(numero),
        person: PersonId(7),
        ssrc: Ssrc(ssrc),
        server: "Casa".into(),
        voice_rooms: vec![VoiceRoomInfo {
            id: SALA,
            name: "Sala".into(),
            limit: 0,
            password_required: false,
            channel: None,
        }],
        channels: Vec::new(),
        permissions: Vec::new(),
    }
}

/// A Lia sentada na sala, do jeito que o servidor a apresenta.
fn lia_na_sala() -> ServerMessage {
    ServerMessage::PersonJoined {
        voice_room: SALA,
        profile: PersonProfile {
            id: PersonId(9),
            nickname: "lia".into(),
            roles: Vec::new(),
            material_e2ee: None,
        },
        ssrc: Ssrc(900),
    }
}

/// (c), a medida: o que a FFI vê depois de uma volta de quem estava numa sala.
///
/// A volta é a do braço `Aviso::Reconectado` de `drive`: a sessão nova é
/// adotada, e o servidor reapresenta quem está sentado — e nunca a própria
/// pessoa (`seele-server`, `session.rs`, pula `session.person` na
/// fotografia de ocupação). Enquanto isso o motor do enlace **já** mandou
/// `EnterVoiceRoom` pela conexão nova (`restabelecer`, em `enlace.rs`), e o
/// servidor a sentou: a voz dela está na sala.
///
/// `Some(SALA)` é o que `Aviso::Reconectado` traz: o assento que o motor
/// restabeleceu.
#[test]
fn a_volta_mostra_a_pessoa_na_sala_em_que_o_motor_a_sentou_de_novo() {
    let mut room = Room::new();
    room.adopt(&sessao(700), "marcela");
    room.apply(&lia_na_sala());
    room.enter_voice_room(SALA);

    readotar_na_volta(&mut room, &sessao(701), "marcela", Some(SALA));
    room.apply(&lia_na_sala());

    let salas = voice_rooms_of(&room);
    assert_eq!(
        room.current_voice_room,
        Some(SALA),
        "depois da volta a FFI tira a pessoa da sala em que o motor a sentou de \
             novo: a tela a desenha FORA DE SALA enquanto a voz dela está lá, e um \
             gatilho do áudio lido deste espelho fecharia a voz de quem só caiu e \
             voltou. O que a tela recebe: {salas:?}"
    );
    assert!(
        salas
            .iter()
            .any(|sala| sala.occupied_by_us && sala.people.iter().any(|p| p.is_self)),
        "a pessoa voltou à sala e não aparece sentada nela na própria tela: {salas:?}"
    );
}

/// Quem estava fora de sala volta fora de sala.
#[test]
fn a_volta_de_quem_estava_fora_de_sala_continua_fora() {
    let mut room = Room::new();
    room.adopt(&sessao(700), "marcela");
    room.apply(&lia_na_sala());

    readotar_na_volta(&mut room, &sessao(701), "marcela", None);

    assert_eq!(
        room.current_voice_room, None,
        "a volta sentou numa sala quem estava fora de todas"
    );
}
