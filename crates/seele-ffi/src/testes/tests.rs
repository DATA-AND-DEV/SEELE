// As funções da projeção que estes testes chamam sozinhas moram no
// `seele_core::retrato` desde a fase 2 da Tarefa 2.2 da F4.
use seele_core::retrato::{lines_of, tela_por_id, voice_rooms_of};
#[test]
fn a_chave_de_mensagem_nao_comeca_num_valor_fixo() {
    // Pendência 19, a metade do app. Aqui o contador é do processo, e era
    // *entre* processos que a repetição acontecia: reabrir o SEELE fazia a
    // primeira mensagem ser lida como reenvio da primeira mensagem da
    // abertura anterior, e ela não era gravada.
    //
    // Só dá para observar o sorteio uma vez por processo, então o que se
    // afirma é a propriedade que sobrevive a isso: o valor não é pequeno, e
    // o contador anda.
    let primeira = super::next_client_message_id();
    let segunda = super::next_client_message_id();

    assert!(
        primeira > u64::from(u32::MAX),
        "a metade alta não foi sorteada: {primeira}"
    );
    assert_eq!(segunda, primeira + 1, "o contador não anda de um em um");
}

use super::*;

#[test]
fn a_bare_host_gets_the_default_port() {
    let (address, name, _) = resolve("localhost").expect("resolve");
    assert_eq!(address.port(), seele_core::DEFAULT_PORT);
    assert_eq!(name, "localhost");
}

#[test]
fn an_address_is_pinned_under_a_name_a_certificate_can_carry() {
    // ADR 0003 pins per name. An IP is not a name a certificate is issued
    // for, so pinning under one would file the pin somewhere nothing ever
    // looks for it again.
    let (_, name, pin) = resolve("127.0.0.1:8383").expect("resolve");
    assert_eq!(
        name, "localhost",
        "TLS gets the name the certificate carries"
    );
    assert_eq!(pin, "127.0.0.1:8383", "the pin is filed under the address");
}

#[test]
fn every_expected_fingerprint_reaches_the_destino_it_promised() {
    // `drive` builds this immediately before a real QUIC connection, so
    // this is the only way to check the wiring without a live server —
    // and the only thing standing between an invite's `fp=` and the
    // refusal ADR 0006 exists to produce.
    let (address, name, pin) = resolve("localhost:8383").expect("resolve");

    let with_fingerprint = ConnectConfig {
        server: "localhost:8383".into(),
        alternate_servers: Vec::new(),
        nickname: "rafael".into(),
        home: "/tmp/does-not-matter".into(),
        join_secret: None,
        expected_fingerprint: Some("aaaa1111".into()),
        codigo_esperado: None,
        bilhete: None,
        audio: false,
        capture_device: None,
        playback_device: None,
    };
    let destino = build_destino(&with_fingerprint, address, &name, &pin, &pin);
    assert_eq!(destino.impressao_esperada(), Some("aaaa1111"));

    let without = ConnectConfig {
        expected_fingerprint: None,
        bilhete: None,
        ..with_fingerprint
    };
    let destino = build_destino(&without, address, &name, &pin, &pin);
    assert_eq!(destino.impressao_esperada(), None);
}

/// A configuração de uma conexão por link, com a impressão que ele promete.
fn config_com_esperada(alvo: &str) -> ConnectConfig {
    ConnectConfig {
        server: alvo.into(),
        alternate_servers: Vec::new(),
        nickname: "rafael".into(),
        home: "/tmp/does-not-matter".into(),
        join_secret: None,
        expected_fingerprint: Some(ESPERADA_DO_LINK.into()),
        codigo_esperado: None,
        bilhete: None,
        audio: false,
        capture_device: None,
        playback_device: None,
    }
}

/// O que o link promete nos testes do pino que prova o servidor.
const ESPERADA_DO_LINK: &str = "aaaa1111";

/// O destino que `drive` monta para o alvo: o primeiro, com a chave do
/// aceite igual à própria chave de pino.
fn destino_do_alvo(alvo: &str) -> seele_core::enlace::Destino {
    let (address, name, pin) = resolve(alvo).expect("um endereço literal resolve sem rede");
    build_destino(&config_com_esperada(alvo), address, &name, &pin, &pin)
}

#[test]
fn um_alternativo_nao_tem_o_pino_que_prova_o_servidor() {
    // `drive` dá a todos os candidatos a chave do destino como chave do
    // aceite, e a cada um a própria chave de pino: só o endereço que a
    // pessoa escolheu tem as duas iguais. Num alternativo, o pin pode ser de
    // outro servidor que esta máquina já fixou ali
    // (`seele_core::tofu::TofuVerifier::decide`). Os dois endereços daqui
    // são de escopo público, para a diferença ser só a de ser o alvo.
    let config = config_com_esperada("203.0.113.7:8383");
    let (address, name, pin) = resolve("203.0.113.7:8383").expect("resolve");
    assert!(
        build_destino(&config, address, &name, &pin, &pin).o_pino_prova_o_servidor,
        "o alvo público deixou de ter o pino que prova o servidor: um link velho contra \
             um servidor já fixado passaria a ser recusado, contra o ADR 0003"
    );

    let (outro, nome_do_outro, pin_do_outro) = resolve("203.0.113.9:9368").expect("resolve");
    assert!(
        !build_destino(&config, outro, &nome_do_outro, &pin_do_outro, &pin).o_pino_prova_o_servidor,
        "um alternativo ficou com o pino que prova o servidor: o pin de outro servidor \
             naquele endereço passaria por cima da impressão esperada, e o `Hello` sairia \
             para ele"
    );
}

#[test]
fn um_alvo_de_escopo_local_nao_tem_o_pino_que_prova_o_servidor() {
    // O alvo de um link de quem hospeda em casa é o endereço da rede local
    // do anfitrião (`seele_proto::uri`), e é também a chave da entrada que o
    // link deixa na lista. Para quem visita pela internet, esse endereço se
    // repete de uma casa para outra: na rede em que a pessoa está, outro
    // servidor pode atender nele, já fixado nesta máquina. Ali, um pin que
    // confere com a esperada discordando é, no caso comum, colisão, porque
    // um servidor que trocou de chave dá `Changed`.
    for alvo in [
        "192.168.0.20:8383",
        "10.0.0.5",
        "172.16.3.4:9000",
        "169.254.1.1:8383",
        "100.64.1.2:8383",
        "[fd00::5]:8383",
        "[fe80::1]:8383",
        "[::ffff:192.168.0.20]:8383",
    ] {
        assert!(
            !destino_do_alvo(alvo).o_pino_prova_o_servidor,
            "o alvo de escopo local {alvo} ficou com o pino que prova o servidor: o \
                 servidor que atende nesse endereço na rede desta pessoa passaria pelo pino \
                 dele, e o `Hello` sairia para ele com o convite, o apelido e a assinatura"
        );
    }
}

#[test]
fn um_alvo_publico_ou_de_loopback_tem_o_pino_que_prova_o_servidor() {
    // O guarda de não ir longe demais. Um endereço público é o mesmo em
    // qualquer rede, e o loopback é esta máquina em qualquer rede: nos dois,
    // o pin do alvo prova a continuidade do servidor, e vale o ADR 0003.
    for alvo in [
        "203.0.113.7:8383",
        "[2001:db8::7]:8383",
        "127.0.0.1:8383",
        "[::1]:8383",
    ] {
        assert!(
            destino_do_alvo(alvo).o_pino_prova_o_servidor,
            "o alvo {alvo} perdeu o pino que prova o servidor: um link velho contra um \
                 servidor já fixado ali passaria a ser recusado, contra o ADR 0003"
        );
    }
}

#[test]
fn um_nome_decide_pelo_endereco_resolvido() {
    // A decisão é sobre o endereço resolvido, o `SocketAddr` do destino, e
    // não sobre o texto: `casa.local` que resolve para a rede de casa é um
    // alvo de escopo local como `192.168.0.20`.
    let config = config_com_esperada("casa.local:8383");
    let na_rede_de_casa: SocketAddr = "192.168.0.20:8383".parse().expect("endereço");
    assert!(
        !build_destino(
            &config,
            na_rede_de_casa,
            "casa.local",
            "casa.local:8383",
            "casa.local:8383"
        )
        .o_pino_prova_o_servidor,
        "um nome que resolve para a rede de casa ficou com o pino que prova o servidor: \
             o escopo não saiu do endereço resolvido, e o nome passou por público"
    );
    let publico: SocketAddr = "203.0.113.7:8383".parse().expect("endereço");
    assert!(
        build_destino(
            &config,
            publico,
            "seele.exemplo.org",
            "seele.exemplo.org:8383",
            "seele.exemplo.org:8383"
        )
        .o_pino_prova_o_servidor,
        "um nome que resolve para um endereço público perdeu o pino que prova o servidor"
    );
}

/// O que o verificador decide com o destino que `drive` monta para `alvo`,
/// quando esta máquina já fixou naquele endereço a chave de quem atende e o
/// link promete outra. Devolve a decisão, a loja e a chave fixada.
fn a_volta_ao_alvo_ja_fixado(
    alvo: &str,
) -> (
    seele_core::PinDecision,
    Arc<seele_core::MemoryPinStore>,
    String,
) {
    use seele_core::tofu::TofuVerifier;
    use seele_core::{MemoryPinStore, PinDecision, PinStore};

    const DE_QUEM_ATENDE: &[u8] = b"o certificado de quem atende no alvo";
    let destino = destino_do_alvo(alvo);
    let loja = Arc::new(MemoryPinStore::new());

    // A visita de antes, sem link: a chave de quem atende ali fica fixada.
    let visita = TofuVerifier::new(
        Arc::clone(&loja) as Arc<dyn PinStore>,
        destino.chave_do_pin.clone(),
        None,
        true,
    );
    let PinDecision::FirstContact {
        fingerprint: fixada,
    } = visita.decide(&destino.chave_do_pin, DE_QUEM_ATENDE)
    else {
        panic!("a visita de antes não fixou a chave de quem atende em {alvo}");
    };

    let verificador = TofuVerifier::new(
        Arc::clone(&loja) as Arc<dyn PinStore>,
        destino.chave_do_pin.clone(),
        destino.impressao_esperada().map(str::to_owned),
        destino.o_pino_prova_o_servidor,
    );
    let decisao = verificador.decide(&destino.chave_do_pin, DE_QUEM_ATENDE);
    (decisao, loja, fixada)
}

#[test]
fn num_alvo_de_lan_o_pino_que_confere_nao_passa_por_cima_da_esperada() {
    // O que a regra de cima compra, visto no verificador. Na rede desta
    // pessoa, o servidor da casa dela atende no `192.168.0.20:8383` que o
    // link de um amigo tem como alvo, e esta máquina já o fixou ali. O pin
    // confere, e a esperada do link é a do amigo: é colisão, e o `Hello` não
    // pode sair para quem atendeu.
    use seele_core::{PinDecision, PinStore};

    for alvo in ["192.168.0.20:8383", "[fd00::5]:8383"] {
        let (decisao, loja, fixada) = a_volta_ao_alvo_ja_fixado(alvo);
        assert_eq!(
            decisao,
            PinDecision::InviteRefused {
                expected: ESPERADA_DO_LINK.into(),
                offered: fixada.clone(),
            },
            "no alvo de LAN {alvo}, o pino de quem atende passou por cima da impressão \
                 que o link promete: o `Hello` sairia para o servidor da rede desta pessoa \
                 com o convite, o apelido e a assinatura, e o do amigo ficaria inalcançável \
                 daqui"
        );
        let chave = chave_do_servidor(alvo).expect("chave");
        assert_eq!(
            loja.pinned(&chave),
            Some(fixada),
            "a recusa no alvo de LAN {alvo} mexeu no pino dali: ele continua sendo a \
                 prova de quem atende nesse endereço, e a recusa não fixa nem desfaz nada"
        );
    }
}

#[test]
fn num_alvo_publico_ou_de_loopback_o_pino_que_confere_passa_e_so_avisa() {
    // A metade que a recusa de cima não pode levar junto (ADR 0003): onde o
    // pino prova o servidor, quem discorda dele é o link.
    use seele_core::PinDecision;

    for alvo in ["203.0.113.7:8383", "127.0.0.1:8383"] {
        let (decisao, _, fixada) = a_volta_ao_alvo_ja_fixado(alvo);
        assert_eq!(
            decisao,
            PinDecision::Matches {
                fingerprint: fixada
            },
            "o alvo {alvo}, já fixado, passou a ser recusado pela impressão do link: um \
                 link velho trancaria a pessoa para fora do servidor que ela usa, contra o \
                 ADR 0003"
        );
    }
}

/// **O aceite acompanha o servidor, e não o caminho até ele.**
///
/// Encontrado em produção, e o laço era completo: a pessoa clicava em
/// ACEITAR, o sim era gravado sob o endereço que ela digitou, a conexão
/// entrava pelo candidato de retransmissão — cuja porta o NAT troca a cada
/// sessão — e o aceite, procurado sob *essa* porta, nunca existia. A tela
/// de aceite voltava. Aceitar de novo gravava no mesmo lugar e falhava do
/// mesmo jeito, para sempre.
///
/// Os dados que fecharam o diagnóstico: `aceites` tinha
/// `192.168.50.30:8383 → acff8e16…`, exatamente o conjunto exigido, e quem
/// respondia era `187.255.97.152:9368`. O arquivo de pins guardava nove
/// portas diferentes do mesmo endereço público — uma por sessão.
///
/// O pin continua por endereço, porque é isso que ele prova: *este*
/// endereço apresentou *esta* chave. O aceite é do **servidor**, e todos os
/// candidatos de uma chegada são o mesmo servidor — mesmo convite, mesma
/// impressão digital conferida.
#[test]
fn um_candidato_alternativo_encontra_o_aceite_do_destino() {
    let home =
        std::env::temp_dir().join(format!("seele-ffi-aceite-candidato-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("temporário");
    let casa = home.to_string_lossy().into_owned();
    let digitado = "192.168.50.30:8383";
    let conjunto = "acff8e1650272dcca3835f2a8a6dc0af6ce12fd081e9806ee1ced396f3e4339d";
    crate::mods::aceitar(&casa, digitado, conjunto).expect("gravar o sim");

    let config = ConnectConfig {
        home: casa.clone(),
        server: digitado.into(),
        alternate_servers: Vec::new(),
        nickname: "alguem".into(),
        join_secret: None,
        expected_fingerprint: None,
        codigo_esperado: None,
        bilhete: None,
        audio: false,
        capture_device: None,
        playback_device: None,
    };

    // O candidato que de fato responde: outro endereço, outra porta — a
    // porta que o NAT escolheu nesta sessão e vai trocar na próxima.
    let alternativo: SocketAddr = "187.255.97.152:9368".parse().expect("endereço");
    let destino = build_destino(
        &config,
        alternativo,
        "servidor",
        "187.255.97.152:9368",
        digitado,
    );
    assert_eq!(
        destino.aceito.as_deref(),
        Some(conjunto),
        "o candidato alternativo não encontrou o aceite do destino: a tela \
             de aceite volta, e aceitar de novo grava no mesmo lugar que já não \
             é consultado"
    );

    // E a outra metade: o aceite continua sendo **daquele** servidor. Um
    // destino que a pessoa nunca autorizou não herda o sim de ninguém.
    let outro = build_destino(
        &config,
        alternativo,
        "servidor",
        "187.255.97.152:9368",
        "outro.exemplo:8383",
    );
    assert_eq!(
        outro.aceito, None,
        "um servidor que nunca foi aceito herdou o consentimento de outro"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// E a chamada de verdade passa a chave do destino a **todos** os
/// candidatos, e não a de cada um.
///
/// Pelo mesmo motivo que `every_expected_fingerprint_reaches_the_destino_it_promised`
/// existe: `build_destino` pode estar certo e `drive` continuar chamando-o
/// com o argumento errado, e essa diferença só um aperto de mão de verdade
/// mostraria.
#[test]
fn a_chave_do_aceite_que_chega_aos_candidatos_e_a_do_destino() {
    let fonte = include_str!("../lib.rs");
    let sem_comentario: String = fonte
        .lines()
        .filter(|linha| !linha.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let chamada = sem_comentario
        .split_once("std::iter::once((address, server_name.clone(), pin_key.clone()))")
        .expect("a montagem dos destinos")
        .1;
    let ate_o_collect = chamada
        .split_once(".collect()")
        .map_or(chamada, |(antes, _)| antes);
    assert!(
        ate_o_collect.contains("&chave_do_aceite"),
        "os candidatos voltaram a receber a própria chave como chave do \
             aceite: quem entra pelo candidato de retransmissão cai no laço da \
             tela de aceite: {ate_o_collect}"
    );
}

/// O sim que a casca guarda é lido pela conexão seguinte.
///
/// **A trava que este teste põe.** São dois caminhos que nunca se
/// encontram: `mods::aceitar` grava sem socket nenhum, de dentro de um
/// comando da janela, e `build_destino` lê já com o endereço resolvido. Se
/// os dois derivarem a chave por conta própria, o dia em que uma das regras
/// mudar é um dia em que o aceite é escrito num lugar e procurado noutro —
/// e o sintoma é mudo: ninguém vê um erro, a pessoa só é perguntada de novo
/// em toda entrada, para sempre.
///
/// As três formas abaixo são as que já custaram caro em outro lugar: a
/// porta omitida (que a lista de visitados duplicava), o IPv6 literal (ADR
/// 0022, passo 2) e o endereço comum.
#[test]
fn o_aceite_e_guardado_sob_a_chave_que_a_conexao_vai_procurar() {
    let home =
        std::env::temp_dir().join(format!("seele-ffi-chave-do-aceite-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("temporário");
    let casa = home.to_string_lossy().into_owned();
    let conjunto = "b2".repeat(32);

    for digitado in ["localhost:8383", "localhost", "[::1]:8383"] {
        let (address, name, pin) = resolve(digitado).expect("resolve");

        crate::mods::aceitar(&casa, digitado, &conjunto).expect("guardar o sim");

        let config = ConnectConfig {
            server: digitado.into(),
            alternate_servers: Vec::new(),
            nickname: "rafael".into(),
            home: casa.clone(),
            join_secret: None,
            expected_fingerprint: None,
            codigo_esperado: None,
            bilhete: None,
            audio: false,
            capture_device: None,
            playback_device: None,
        };
        let destino = build_destino(&config, address, &name, &pin, &pin);
        assert_eq!(
            destino.aceito,
            Some(conjunto.clone()),
            "o aceite guardado para «{digitado}» não foi encontrado pela conexão"
        );

        crate::mods::esquecer_aceite(&casa, digitado).expect("esquecer");
        let destino = build_destino(&config, address, &name, &pin, &pin);
        assert_eq!(
            destino.aceito, None,
            "retirar o consentimento de «{digitado}» não chegou à conexão"
        );
    }

    let _ = std::fs::remove_dir_all(&home);
}

/// Um alvo que não é endereço é recusado por nome, e não gravado como veio.
///
/// Gravá-lo criaria a entrada que nada lê — ver o teste acima.
#[test]
fn um_alvo_que_nao_e_endereco_nao_vira_um_aceite_escrito_no_vazio() {
    let casa = std::env::temp_dir().to_string_lossy().into_owned();
    assert_eq!(
        crate::mods::aceitar(&casa, "::1", &"c3".repeat(32)),
        Err("EnderecoInvalido".to_owned()),
        "um IPv6 sem colchetes foi aceito como chave"
    );
    assert_eq!(crate::mods::aceite_de(&casa, "::1"), None);
}

#[test]
fn a_host_that_does_not_exist_is_an_enum_and_not_a_panic() {
    assert_eq!(
        resolve("nao-existe.invalid:8383"),
        Err(ConnectionError::UnresolvableHost)
    );
}

#[test]
fn the_pattern_survives_the_round_trip_through_an_atomic() {
    for link_state in [
        LinkTrust::Offline,
        LinkTrust::Unverified,
        LinkTrust::Verified,
    ] {
        assert_eq!(
            link_state_from_byte(link_state_byte(link_state)),
            link_state
        );
    }
}

#[test]
fn an_unknown_pattern_byte_reads_as_offline() {
    // Whatever goes wrong, it must not claim a verified session.
    assert_eq!(link_state_from_byte(200), LinkTrust::Offline);
}

#[test]
fn message_identifiers_never_repeat_within_a_process() {
    // Two handles in one process sending the same number would collide in
    // the server's idempotency check, and the second message would vanish
    // as a resend of the first.
    let ids: std::collections::HashSet<u64> = (0..1000).map(|_| next_client_message_id()).collect();
    assert_eq!(ids.len(), 1000);
}

#[test]
fn a_snapshot_of_an_empty_room_is_empty_and_not_a_panic() {
    let room = Room::new();
    assert!(voice_rooms_of(&room).is_empty());
    assert!(lines_of(&room).is_empty());
    assert!(messages_of(&room).is_empty());
}

/// **A confirmada própria chega com o eco da pendente**, e é o que deixa a
/// casca trocar uma pela outra no mesmo nó (achado da revisão do R63 da
/// F5). Sem ele, quem usa leitor de tela e lia a mensagem que acabou de
/// mandar perdia a posição, a seleção e o REMOVER a cada envio.
///
/// A de outra pessoa chega sem a chave, mesmo que o fio a traga. E a
/// própria relida do histórico chega com a chave gravada: o `FetchHistory`
/// do servidor responde com o `client_message_id` de cada linha.
#[test]
fn a_confirmada_propria_chega_com_a_chave_da_pendente() {
    use seele_core::{ChannelInfo, ClientMessageId, PersonId, ServerMessage, SessionId, Ssrc};

    let canal = ChannelId(1);
    let mut room = Room::new();
    room.apply(&ServerMessage::Session {
        id: SessionId(1),
        person: PersonId(7),
        ssrc: Ssrc(700),
        server: "Casa".into(),
        voice_rooms: Vec::new(),
        channels: vec![ChannelInfo {
            id: canal,
            name: "geral".into(),
        }],
        roles: Vec::new(),
        permissions: Vec::new(),
    });
    room.open_channel(canal);
    let recebida = |id: u64, autor: u64, chave: Option<u64>| ServerMessage::MessageReceived {
        channel: canal,
        id: MessageId(id),
        author: PersonId(autor),
        at_seconds: 0,
        author_nickname: format!("pessoa{autor}"),
        body: format!("mensagem {id}"),
        replies_to: None,
        client_message_id: chave.map(ClientMessageId),
        attachment: None,
    };

    // A pendente, e a chave com que a casca a desenha.
    room.anotar_envio(canal, ClientMessageId(77), "mensagem 5".into(), 0);
    let pendente = messages_of(&room);
    assert_eq!(pendente.len(), 1);
    assert_eq!(pendente[0].client_message_id.as_deref(), Some("77"));

    // Gravada: a pendente sai, e a confirmada leva a mesma chave.
    room.apply(&recebida(5, 7, Some(77)));
    // De outra pessoa, com uma chave no fio que não é desta máquina.
    room.apply(&recebida(6, 8, Some(78)));
    // A própria relida do histórico, com a chave que o servidor gravou.
    room.apply(&recebida(2, 7, Some(31)));

    let lista = messages_of(&room);
    let de = |id: u64| lista.iter().find(|mensagem| mensagem.id == id);
    assert_eq!(
        lista.len(),
        3,
        "a pendente não saiu, ou alguma não chegou: {lista:?}"
    );
    let gravada = de(5).expect("a gravada");
    assert_eq!(gravada.estado, EstadoDoEnvio::Confirmada);
    assert_eq!(
        gravada.client_message_id.as_deref(),
        Some("77"),
        "a confirmada própria chegou sem a chave da pendente, e a casca a desenha \
             como uma mensagem nova"
    );
    assert_eq!(
        de(6).expect("a de outra pessoa").client_message_id,
        None,
        "a chave de outra máquina chegou à casca"
    );
    assert_eq!(
        de(2).expect("a relida").client_message_id.as_deref(),
        Some("31"),
        "o histórico traz a chave gravada, e a própria antiga a perdeu no caminho"
    );
}

#[test]
fn the_snapshot_marks_which_person_is_us() {
    // Without this the shell has to compare ids, which means the shell has
    // to know what a `PersonId` is.
    use seele_core::{PersonId, PersonProfile, ServerMessage, SessionId, Ssrc, VoiceRoomInfo};

    let mut room = Room::new();
    room.apply(&ServerMessage::Session {
        id: SessionId(1),
        person: PersonId(7),
        ssrc: Ssrc(700),
        server: "Casa".into(),
        voice_rooms: vec![VoiceRoomInfo {
            id: VoiceRoomId(1),
            name: "SALA-01".into(),
            limit: 20,
            password_required: false,
            channel: None,
        }],
        channels: Vec::new(),
        roles: Vec::new(),
        permissions: Vec::new(),
    });
    room.enter_voice_room(VoiceRoomId(1));
    room.apply(&ServerMessage::PersonJoined {
        voice_room: VoiceRoomId(1),
        profile: PersonProfile {
            id: PersonId(3),
            nickname: "marcela".into(),
            roles: Vec::new(),
            material_e2ee: None,
        },
        ssrc: Ssrc(30),
    });

    let voice_rooms = voice_rooms_of(&room);
    let people = &voice_rooms[0].people;
    assert!(people.iter().any(|person| person.is_self));
    assert_eq!(
        people.iter().filter(|person| person.is_self).count(),
        1,
        "more than one person claims to be us"
    );
    assert!(voice_rooms[0].occupied_by_us);
}

#[test]
fn a_sync_band_travels_beside_its_number() {
    // specs/06-clientes-gui.md forbids carrying it by colour alone, and the
    // same applies here: a shell that only got a hue could not print a
    // number, and one that only got a number would have to know the
    // thresholds.
    use seele_core::{
        PersonId, PersonProfile, PersonState, Presence, ServerMessage, Ssrc, VoiceRoomInfo,
    };

    let mut room = Room::new();
    room.voice_rooms = vec![VoiceRoomInfo {
        id: VoiceRoomId(1),
        name: "SALA-01".into(),
        limit: 20,
        password_required: false,
        channel: None,
    }];
    room.apply(&ServerMessage::PersonJoined {
        voice_room: VoiceRoomId(1),
        profile: PersonProfile {
            id: PersonId(3),
            nickname: "marcela".into(),
            roles: Vec::new(),
            material_e2ee: None,
        },
        ssrc: Ssrc(30),
    });
    room.apply(&ServerMessage::PersonState(PersonState {
        person: PersonId(3),
        muted: false,
        total_isolation: false,
        speaking: false,
        presence: Presence::Available,
        signal: 72,
    }));

    // 72 rather than a critical number on purpose: `SignalBand::Critical` is
    // the `Default`, so a shell that received it could not tell a banded
    // ratio from a field nobody filled in.
    let person = &voice_rooms_of(&room)[0].people[0];
    assert_eq!(person.signal, 72);
    assert_eq!(person.sync_band, types::SignalBand::Degraded);
}

#[test]
fn the_voice_rooms_average_crosses_already_banded() {
    // MÉDIA DO VOICE_ROOM. The comp computes it in the shell; here the shell gets
    // the number, the band and the sample size and has nothing left to
    // decide. A voice room nobody is in carries `None`, not a critical zero.
    use seele_core::{
        PersonId, PersonProfile, PersonState, Presence, ServerMessage, Ssrc, VoiceRoomInfo,
    };

    let mut room = Room::new();
    room.voice_rooms = vec![VoiceRoomInfo {
        id: VoiceRoomId(1),
        name: "SALA-01".into(),
        limit: 20,
        password_required: false,
        channel: None,
    }];
    assert_eq!(voice_rooms_of(&room)[0].sync, None, "an empty voice room");

    for (id, sync) in [(3_u64, 84_u8), (4, 85)] {
        room.apply(&ServerMessage::PersonJoined {
            voice_room: VoiceRoomId(1),
            profile: PersonProfile {
                id: PersonId(id),
                nickname: format!("pessoa {id}"),
                roles: Vec::new(),
                material_e2ee: None,
            },
            ssrc: Ssrc(u32::try_from(id * 10).expect("ssrc")),
        });
        room.apply(&ServerMessage::PersonState(PersonState {
            person: PersonId(id),
            muted: false,
            total_isolation: false,
            speaking: false,
            presence: Presence::Available,
            signal: sync,
        }));
    }

    let average = voice_rooms_of(&room)[0]
        .sync
        .expect("two people are seated");
    assert_eq!(average.ratio, 85);
    assert_eq!(average.band, types::SignalBand::Nominal);
    assert_eq!(average.people, 2);
}

#[test]
fn a_listener_hears_what_the_room_folded() {
    use seele_core::{AlertReason, AlertSeverity, ServerMessage};

    #[derive(Default)]
    struct Recorder(Mutex<Vec<Event>>);
    impl EventListener for Recorder {
        fn on_event(&self, event: Event) {
            if let Ok(mut seen) = self.0.lock() {
                seen.push(event);
            }
        }
    }

    let shared = bare_shared();
    let recorder = Arc::new(Recorder::default());
    shared
        .listeners
        .lock()
        .unwrap()
        .push(Arc::clone(&recorder) as Arc<dyn EventListener>);

    fold(
        &shared,
        &ServerMessage::Alert {
            severity: AlertSeverity::Critical,
            reason: AlertReason::PermissionDenied,
            operator_text: None,
        },
    );

    let seen = recorder.0.lock().unwrap();
    assert_eq!(seen.len(), 1);
    let Event::NoticeRaised { notice } = &seen[0] else {
        panic!("expected a notice, got {:?}", seen[0]);
    };
    assert_eq!(notice.severity, Severity::Critical);
    assert_eq!(notice.reason, NoticeReason::PermissionDenied);
}

/// **O aviso da metade de servidor de um MOD chega à casca como evento**
/// (rascunho da API 6, §7.3; tarefa 8 do plano da F3), como a resposta de
/// um pedido: a casca o entrega às janelas daquele MOD. O núcleo não o
/// interpreta — a carga é JSON do MOD, e passa como veio.
#[test]
fn o_aviso_de_um_mod_chega_a_casca_como_evento() {
    use seele_core::ServerMessage;

    #[derive(Default)]
    struct Recorder(Mutex<Vec<Event>>);
    impl EventListener for Recorder {
        fn on_event(&self, event: Event) {
            if let Ok(mut seen) = self.0.lock() {
                seen.push(event);
            }
        }
    }

    let shared = bare_shared();
    let recorder = Arc::new(Recorder::default());
    shared
        .listeners
        .lock()
        .unwrap()
        .push(Arc::clone(&recorder) as Arc<dyn EventListener>);

    fold(
        &shared,
        &ServerMessage::ModAviso {
            mod_id: "seele/mesa".into(),
            canal: Some(ChannelId(7)),
            carga: r#"{"revisao":3}"#.into(),
        },
    );

    let seen = recorder.0.lock().unwrap();
    assert_eq!(
        seen.as_slice(),
        &[Event::ModAviso {
            mod_id: "seele/mesa".into(),
            canal: Some(7),
            carga: r#"{"revisao":3}"#.into(),
        }],
        "o aviso do MOD não chegou à casca como evento"
    );
}

/// Uma `Connection` montada à mão, com o fim da thread da conexão em
/// `encerrada`.
fn conexao_com_o_fim(encerrada: std::sync::mpsc::Receiver<()>) -> Connection {
    let (commands, _fila) = tokio::sync::mpsc::unbounded_channel();
    Connection {
        commands,
        shared: bare_shared(),
        encerrada: Mutex::new(Some(encerrada)),
        ligacao: None,
    }
}

/// **Sem o sinal da thread da conexão, `encerrar_e_esperar` espera o teto e
/// diz que não terminou.**
///
/// O `false` é o que a casca escreve no `seele.log`: quem ficou vai notar a
/// saída pelo prazo de ociosidade. Uma `encerrar_e_esperar` que voltasse
/// `true` sem esperar o sinal deixaria o processo sair com a despedida no
/// meio, e a linha que diz isso nunca sairia.
#[test]
fn encerrar_e_esperar_sem_o_sinal_espera_o_teto_e_diz_que_nao_terminou() {
    let (ainda_correndo, encerrada) = std::sync::mpsc::channel();
    let connection = conexao_com_o_fim(encerrada);
    let teto = Duration::from_millis(50);

    let comeco = Instant::now();
    let terminou = connection.encerrar_e_esperar(teto);
    let levou = comeco.elapsed();

    assert!(
        !terminou,
        "a thread da conexão não deu sinal nenhum, e `encerrar_e_esperar` disse que ela \
             terminou: a casca deixa o processo sair com a despedida no meio, calada"
    );
    assert!(
        levou >= teto,
        "`encerrar_e_esperar` voltou em {levou:?}, antes do teto de {teto:?}: não esperou o \
             sinal da thread da conexão"
    );
    drop(ainda_correndo);
}

/// **O sinal dado, ou a thread que terminou sem dá-lo, dizem que terminou.**
///
/// O segundo é a thread que caiu antes de chegar ao sinal — o runtime que não
/// subiu, um pânico —: o remetente largado diz o mesmo fim, e esperar o teto
/// por ela seria segurar a saída por uma thread que não existe mais.
#[test]
fn encerrar_e_esperar_volta_na_hora_com_o_sinal_dado_ou_a_thread_acabada() {
    let (sinal, encerrada) = std::sync::mpsc::channel();
    sinal.send(()).expect("o sinal");
    let deu_o_sinal = conexao_com_o_fim(encerrada);
    let (sem_sinal, acabou) = std::sync::mpsc::channel::<()>();
    drop(sem_sinal);
    let acabou_sem_sinal = conexao_com_o_fim(acabou);
    let teto = Duration::from_secs(5);

    let comeco = Instant::now();
    assert!(
        deu_o_sinal.encerrar_e_esperar(teto),
        "a thread da conexão deu o sinal, e `encerrar_e_esperar` disse que não terminou"
    );
    assert!(
        acabou_sem_sinal.encerrar_e_esperar(teto),
        "a thread da conexão acabou sem sinal, e `encerrar_e_esperar` disse que não terminou"
    );
    assert!(
        comeco.elapsed() < teto,
        "`encerrar_e_esperar` esperou o teto por uma thread que já tinha terminado"
    );
}

fn relatorio_de_tunel_de_teste(encerrado: Option<&'static str>) -> RelatorioDeTunel {
    RelatorioDeTunel {
        mod_id: "x/jogo".into(),
        nome: "eco".into(),
        porta: 25565,
        preferida: true,
        desejada: 25565,
        caminho: CaminhoDeTunel::Estrela,
        latencia_ms: None,
        bytes_por_segundo: 0,
        bytes_por_segundo_ida: 0,
        bytes_por_segundo_volta: 0,
        conexoes: 0,
        conexoes_diretas: 0,
        conexoes_pela_estrela: 0,
        quedas: Vec::new(),
        udp_descartados: 0,
        encerrado,
    }
}

/// **M6.** O relatório final de um túnel pode chegar **antes** de a espera
/// da entrada voltar a `guardar_tunel_aberto`. Sem a conferência, o
/// `or_insert` punha na tabela um túnel que já acabou, e
/// `tuneis_abertos` o devolvia para sempre.
#[test]
fn o_tunel_que_acabou_antes_de_a_entrada_voltar_nao_ressuscita_na_tabela() {
    let shared = bare_shared();
    let aberta = PortaDeTunel {
        porta: 25565,
        preferida: true,
        desejada: 25565,
    };
    // O caso de sempre: sem fim no meio, a porta entra na tabela.
    let antes = fins_do_tunel(&shared, "x/jogo", "eco");
    guardar_tunel_aberto(&shared, "x/jogo", "eco", aberta, antes);
    assert_eq!(shared.tuneis.lock().unwrap().abertos.len(), 1);
    guardar_relatorio_de_tunel(&shared, &relatorio_de_tunel_de_teste(Some("fechado")));
    assert!(shared.tuneis.lock().unwrap().abertos.is_empty());

    // A corrida: o pedido saiu (fins lidos), o relatório final chegou, e só
    // então a espera voltou.
    let antes = fins_do_tunel(&shared, "x/jogo", "eco");
    guardar_relatorio_de_tunel(&shared, &relatorio_de_tunel_de_teste(Some("maquina-saiu")));
    guardar_tunel_aberto(&shared, "x/jogo", "eco", aberta, antes);
    assert!(
        shared.tuneis.lock().unwrap().abertos.is_empty(),
        "o túnel que acabou no meio voltou à tabela como fantasma"
    );

    // E uma entrada nova, depois do fim, é um túnel novo e entra.
    let antes = fins_do_tunel(&shared, "x/jogo", "eco");
    guardar_tunel_aberto(&shared, "x/jogo", "eco", aberta, antes);
    assert_eq!(shared.tuneis.lock().unwrap().abertos.len(), 1);
}

/// A `Shared` with nothing in it, for the folds below. O mesmo da
/// conformidade dos dois produtores (`crate::sem_rede`).
fn bare_shared() -> Arc<Shared> {
    crate::sem_rede::shared("marcela")
}

#[test]
fn o_jitter_da_tela_nao_e_a_profundidade_do_anel() {
    // Duas grandezas com o mesmo nome, e mostrar a errada é pior que não
    // mostrar nada:
    //
    // - `worst_jitter_depth_ms` é **profundidade do anel de reprodução**, e o
    //   ADR 0028 acabou de dar um alvo a ele. Mostrá-lo como "jitter" exibiria a
    //   nossa própria reserva como ruído da rede — e uma reserva saudável
    //   apareceria na tela como problema;
    // - `SourceTelemetry::jitter_ms` é o de **chegada** (RFC 3550), medido aqui,
    //   e é o que a pessoa quer saber.
    //
    // E o que a tela lia antes deste conserto era um terceiro valor: o do
    // relatório do servidor, que é sempre `0.0` porque o servidor não tem como
    // saber — `session.rs` diz isso em comentário desde sempre.
    // Uma reserva de anel saudável (42 ms) ao lado de um jitter de rede baixo
    // (7,5 ms): mostrar a primeira faria uma conexão boa parecer ruim.
    assert!(
        (jitter_para_a_tela(7.5, 42.0) - 7.5).abs() < 0.01,
        "a tela mostra o jitter de chegada, não a profundidade do anel"
    );
    // E nunca o zero que o servidor manda de propósito, que era o que a tela lia.
    assert!(jitter_para_a_tela(7.5, 42.0) > 0.0);
}

#[test]
fn o_jitter_que_o_laco_de_voz_grava_e_o_que_a_casca_le() {
    // A travessia inteira do número, sem o laço de voz: se o campo do
    // `Shared` perdesse a escrita ou a leitura, a tela voltaria a mostrar um
    // zero — que é exatamente o defeito de antes, com outra causa.
    let compartilhado = bare_shared();
    assert_eq!(
        compartilhado.jitter_de_chegada_ms(),
        0.0,
        "sem medida nenhuma ainda não há jitter para mostrar"
    );

    compartilhado.gravar_jitter_de_chegada(jitter_para_a_tela(7.5, 42.0));

    assert!(
        (compartilhado.jitter_de_chegada_ms() - 7.5).abs() < 0.01,
        "a casca leu {}, e o laço de voz gravou 7,5",
        compartilhado.jitter_de_chegada_ms()
    );
}

#[test]
fn a_profundidade_do_anel_vai_para_o_sync_ratio_e_a_chegada_para_a_tela() {
    // A outra metade da fiação. `measure` em si precisa de um `Enlace` vivo
    // e de um dispositivo de áudio aberto, então o que se afirma é a
    // separação que ele faz, apartada nele para isto: qual das duas
    // grandezas sai por qual porta.
    //
    // Os números são escolhidos para não poderem ser confundidos, e são os
    // mesmos do teste da função pura: 42 ms de reserva de anel — saudável,
    // que é o alvo que o ADR 0028 deu a ele — ao lado de 7,5 ms de jitter de
    // rede. Trocar os dois de lugar em `measure` mostraria 42 na tela e uma
    // conexão boa pareceria ruim.
    let telemetria = seele_core::AudioTelemetry {
        local: seele_core::LocalTelemetry::default(),
        sources: vec![seele_core::SourceTelemetry {
            ssrc: 700,
            jitter_depth_ms: 42.0,
            jitter_ms: 7.5,
            ..Default::default()
        }],
    };

    let volta = jitter_da_volta(&telemetria);

    assert!(
        (volta.profundidade_do_anel_ms - 42.0).abs() < 0.01,
        "o Sync Ratio recebeu {}, e a reserva do anel era 42 ms",
        volta.profundidade_do_anel_ms
    );
    let Some(chegada) = volta.chegada_ms else {
        panic!("havia uma fonte sendo recebida e nenhum jitter de chegada saiu");
    };
    assert!(
        (chegada - 7.5).abs() < 0.01,
        "a tela recebeu {chegada}, e o jitter de chegada era 7,5 ms"
    );
}

#[test]
fn sem_fonte_nenhuma_a_tela_nao_recebe_um_zero_inventado() {
    // O pior de uma lista vazia seria zero, e zero é justamente o valor que
    // esta tarefa existe para tirar da tela. Sem ninguém falando não há
    // jitter de chegada nenhum a mostrar, e o último medido continua valendo
    // até haver outro.
    let telemetria = seele_core::AudioTelemetry {
        local: seele_core::LocalTelemetry::default(),
        sources: Vec::new(),
    };

    assert!(
        jitter_da_volta(&telemetria).chegada_ms.is_none(),
        "sem fonte nenhuma saiu um jitter de chegada, e ele só pode ser zero"
    );
}

#[test]
fn gravar_o_mesmo_jitter_duas_vezes_nao_e_uma_mudanca() {
    // O evento de telemetria existe para quem **não** puxa `snapshot` em
    // laço, e ele não falava do jitter: enquanto o número era o `0.0` fixo
    // do relatório do servidor isso não custava nada, porque ele nunca se
    // mexia. Agora ele varia sozinho, e quem depende do evento ficaria com o
    // número velho até outra coisa mudar.
    //
    // O limiar é meio milissegundo porque é a menor diferença que muda o
    // texto: as duas cascas arredondam para milissegundo inteiro. Sem ele o
    // evento sairia quatro vezes por segundo para redesenhar a mesma linha,
    // porque um jitter alisado muda no último bit sempre.
    let compartilhado = bare_shared();

    assert!(
        compartilhado.gravar_jitter_de_chegada(7.5),
        "a primeira medida veio de um zero e não contou como mudança"
    );
    assert!(
        !compartilhado.gravar_jitter_de_chegada(7.5),
        "o mesmo número contou como mudança, e a tela redesenharia por nada"
    );
    assert!(
        !compartilhado.gravar_jitter_de_chegada(7.6),
        "um décimo de milissegundo acordou a tela para escrever `8ms` de novo"
    );
    assert!(
        compartilhado.gravar_jitter_de_chegada(9.0),
        "um milissegundo e meio a mais não contou como mudança, e o rodapé \
             ficaria em `8ms` com a rede piorando"
    );
}

/// Uma volta de telemetria com as duas grandezas plantadas para não poderem
/// ser confundidas: 42 ms de reserva de anel contra `chegada` de jitter de
/// rede.
fn volta_com(chegada_ms: f64) -> seele_core::AudioTelemetry {
    seele_core::AudioTelemetry {
        local: seele_core::LocalTelemetry::default(),
        sources: vec![seele_core::SourceTelemetry {
            ssrc: 1,
            jitter_depth_ms: 42.0,
            jitter_ms: chegada_ms,
            ..seele_core::SourceTelemetry::default()
        }],
    }
}

#[test]
fn uma_volta_grava_o_jitter_de_chegada_e_nao_a_reserva_do_anel() {
    // A fiação de `measure`, afirmada onde ela é afirmável. A versão
    // anterior deste teste lia o **texto** de `measure` e passava verde com
    // a propriedade quebrada: um braço que gravasse o jitter e devolvesse
    // `false` satisfazia as duas asserções de texto que ela fazia.
    let compartilhado = bare_shared();
    let mut sync = Signal::new();

    let _ = medir_a_volta(&mut sync, &compartilhado, Some(&volta_com(7.5)), 41_000);

    assert!(
        (compartilhado.jitter_de_chegada_ms() - 7.5).abs() < 0.01,
        "a casca leria {}, e o jitter de chegada era 7,5 ms — 42,0 é a \
             reserva do anel, que é a grandeza errada",
        compartilhado.jitter_de_chegada_ms()
    );
}

#[test]
fn o_evento_de_telemetria_conta_a_variacao_do_jitter() {
    // A pendência do ledger, como comportamento. O evento existe para quem
    // **não** puxa `snapshot` em laço, e ele não falava do jitter: enquanto
    // o número era o `0.0` fixo do relatório do servidor isso não custava nada,
    // porque ele nunca se mexia.
    //
    // A parcela é isolada deixando o Sync Ratio assentar primeiro: ele é
    // uma média móvel, e com entradas constantes ele para de andar. A conta
    // dele não usa o jitter de **chegada** — usa a reserva do anel, o
    // ida-e-volta e a perda —, então depois de assentado o único motivo que
    // resta para responder «mudou» é o jitter da tela.
    let compartilhado = bare_shared();
    let mut sync = Signal::new();

    let mut assentou = false;
    for _ in 0..500 {
        if !medir_a_volta(&mut sync, &compartilhado, Some(&volta_com(7.5)), 41_000) {
            assentou = true;
            break;
        }
    }
    assert!(
        assentou,
        "o Sync Ratio não parou de andar com entradas constantes, então este \
             teste não consegue isolar a parcela do jitter"
    );

    assert!(
        medir_a_volta(&mut sync, &compartilhado, Some(&volta_com(12.0)), 41_000),
        "o jitter de chegada subiu de 7,5 ms para 12 ms e a volta disse que \
             não havia nada a contar à casca: quem depende do evento em vez de \
             puxar `snapshot` em laço desenha o número velho"
    );
    assert!(
        !medir_a_volta(&mut sync, &compartilhado, Some(&volta_com(12.0)), 41_000),
        "o mesmo jitter contou como mudança, e a tela redesenharia por nada"
    );
}

#[test]
fn o_laco_entrega_a_volta_de_audio_que_ele_leu() {
    // Um encosto, e só isso — as propriedades estão nos três testes de
    // comportamento em volta. O que este guarda é o **hop** que sobrou:
    // `measure` lê a telemetria de voz e a passa a `medir_a_volta`, e não há
    // como afirmar essa linha de outro jeito — ela pede um `Enlace` vivo e
    // um dispositivo de áudio aberto, que é justamente o motivo de tudo o
    // mais ter saído dela.
    //
    // O que ele pega é uma mutação real: `medir_a_volta(sync, shared, None,
    // rtt_micros)` tem o tipo certo, compila, e apaga a telemetria de áudio
    // da tela inteira sem que nenhum dos outros testes veja.
    let source = include_str!("../lib.rs");
    let Some(corpo) = source
        .split("fn measure(sync: &mut Signal, client: &Enlace, shared: &Arc<Shared>) -> bool {")
        .nth(1)
        .and_then(|resto| resto.split("\n}").next())
    else {
        panic!("`measure` mudou de assinatura, e este encosto tem de mudar com ela");
    };
    let corpo: String = corpo
        .lines()
        .map(|linha| match linha.split_once("//") {
            Some((antes, _)) => antes,
            None => linha,
        })
        .collect::<Vec<&str>>()
        .join("\n");

    assert!(
        corpo.contains("medir_a_volta(sync, shared, telemetria.as_ref(), rtt_micros)"),
        "`measure` deixou de entregar a `medir_a_volta` a telemetria que ele \
             acabou de ler, e o laço de voz para de chegar à tela:\n{corpo}"
    );
    assert!(
        corpo.contains("shared\n        .voice\n        .lock()"),
        "`measure` deixou de ler o laço de voz:\n{corpo}"
    );
}

#[test]
fn sem_volta_de_audio_nenhuma_o_ultimo_jitter_medido_fica() {
    // A contrapartida deliberada de não gravar zero, e o preço dela: numa
    // sessão viva em que a voz cai no meio, o campo congela no último valor
    // em vez de esvaziar. Gravar zero faria a tela afirmar «rede perfeita»
    // sobre uma sessão sem áudio nenhum, que é o defeito que este ciclo
    // tirou dali.
    let compartilhado = bare_shared();
    let mut sync = Signal::new();

    let _ = medir_a_volta(&mut sync, &compartilhado, Some(&volta_com(7.5)), 41_000);
    let _ = medir_a_volta(&mut sync, &compartilhado, None, 41_000);

    assert!(
        (compartilhado.jitter_de_chegada_ms() - 7.5).abs() < 0.01,
        "a voz caiu e a tela passou a mostrar {}, que é o zero que este \
             conserto existe para não mostrar",
        compartilhado.jitter_de_chegada_ms()
    );
}

#[test]
fn o_snapshot_le_o_jitter_que_o_laco_de_voz_gravou_e_nao_o_do_server() {
    // As duas linhas que consertam o defeito são de fiação, e fiação não se
    // afirma testando a função pura dos dois lados dela: `jitter_para_a_tela`
    // podia estar perfeita enquanto o `snapshot` continuava lendo o relatório
    // do servidor, e a suíte continuaria verde. Isto aqui é o teste que fica
    // vermelho quando alguém desfaz o conserto.
    //
    // Os dois números são plantados de propósito para não poderem ser
    // confundidos: 12,75 ms é o de chegada, medido neste receptor, e é o que
    // a tela tem de mostrar; 99,0 ms é o que o relatório do servidor diz, e é o
    // caminho errado. Na vida real esse campo é `0.0` fixo — o servidor não
    // tem como medir jitter, que só existe no receptor —, mas plantar um 99
    // aqui faz o teste distinguir "leu o certo" de "leu um zero que calhou de
    // bater", que um `0.0` de verdade não distinguiria.
    let compartilhado = bare_shared();
    if let Ok(mut room) = compartilhado.room.lock() {
        room.telemetry = Some(seele_core::Telemetry {
            rtt_ms: Some(20.0),
            jitter_ms: Some(99.0),
            loss_fraction: Some(0.0),
            subsystems: Vec::new(),
        });
    }
    compartilhado.gravar_jitter_de_chegada(jitter_para_a_tela(12.75, 42.0));

    let (commands, _fila) = tokio::sync::mpsc::unbounded_channel();
    let connection = Connection {
        commands,
        shared: Arc::clone(&compartilhado),
        encerrada: Mutex::new(None),
        ligacao: None,
    };
    let mostrado = connection.snapshot().telemetry.jitter_ms;

    assert!(
        (mostrado - 12.75).abs() < 0.01,
        "a tela mostrou {mostrado}, e o laço de voz mediu 12,75 ms de chegada"
    );
    assert!(
        (mostrado - 99.0).abs() > 0.01,
        "a tela voltou a ler o jitter do relatório do servidor, que é o defeito 3.3"
    );
}

#[test]
fn um_jitter_de_fracao_de_milissegundo_nao_e_arredondado_para_zero() {
    // O campo guarda microssegundos inteiros justamente para isto: num
    // enlace local o jitter fica abaixo de um milissegundo, e guardá-lo em
    // milissegundos inteiros faria a tela dizer zero num caminho excelente —
    // indistinguível do zero que o servidor manda.
    let compartilhado = bare_shared();
    compartilhado.gravar_jitter_de_chegada(0.25);

    assert!(
        (compartilhado.jitter_de_chegada_ms() - 0.25).abs() < 0.001,
        "0,25 ms virou {}",
        compartilhado.jitter_de_chegada_ms()
    );
}

#[test]
fn um_jitter_impossivel_nao_atravessa_a_ponte() {
    // `NaN` e negativo não têm como sair da RFC 3550, mas atravessariam a
    // ponte para uma casca que os desenha — e um `NaN` numa tela vira texto
    // que ninguém sabe ler. Zero é a resposta honesta para "não há medida".
    //
    // Quem garante isto é o `as u64` de `gravar_jitter_de_chegada`, que em
    // Rust satura, e não uma guarda escrita à mão — havia um `max(0.0)` ali
    // e ele foi tirado justamente porque nunca disparava. Este teste então
    // não vigia código nosso: ele **fixa a linguagem**, e reprovaria se
    // alguém trocasse aquele cast por uma conversão que embrulha ou por um
    // `unsafe` de arredondamento sem verificação.
    let compartilhado = bare_shared();

    compartilhado.gravar_jitter_de_chegada(f32::NAN);
    assert_eq!(compartilhado.jitter_de_chegada_ms(), 0.0);

    compartilhado.gravar_jitter_de_chegada(-3.0);
    assert_eq!(compartilhado.jitter_de_chegada_ms(), 0.0);
}

#[test]
fn a_room_made_now_reaches_the_shell_as_a_channel_change() {
    // The bridge invents nothing: the server says a voice room exists, the room
    // folds it in, and the shell is told to redraw the list it already
    // knows how to draw. If this stopped firing, the person who made the
    // room would be looking at the old list with no way to know.
    use seele_core::{ServerMessage, VoiceRoomInfo};

    #[derive(Default)]
    struct Recorder(Mutex<Vec<Event>>);
    impl EventListener for Recorder {
        fn on_event(&self, event: Event) {
            if let Ok(mut seen) = self.0.lock() {
                seen.push(event);
            }
        }
    }

    let shared = bare_shared();
    let recorder = Arc::new(Recorder::default());
    shared
        .listeners
        .lock()
        .unwrap()
        .push(Arc::clone(&recorder) as Arc<dyn EventListener>);

    fold(
        &shared,
        &ServerMessage::VoiceRoomCreated {
            voice_room: VoiceRoomInfo {
                id: VoiceRoomId(2),
                name: "SALA-02 SALA DOS FUNDOS".into(),
                limit: 8,
                password_required: false,
                channel: None,
            },
        },
    );

    assert_eq!(
        *recorder.0.lock().unwrap(),
        vec![Event::ChannelsChanged],
        "the shell was not told the channel list moved"
    );
    let room = shared.room.lock().unwrap();
    assert_eq!(voice_rooms_of(&room).len(), 1);
    assert_eq!(voice_rooms_of(&room)[0].name, "SALA-02 SALA DOS FUNDOS");
}

#[test]
fn the_snapshot_says_whether_this_person_may_make_rooms() {
    // What a screen asks before drawing the control. Convenience, never
    // enforcement — but a shell with no way to ask has only two options,
    // and both are bad: hide the feature from the host, or offer everybody
    // a button that mostly answers "no".
    use seele_core::{Permission, ServerMessage, SessionId, Ssrc};

    let shared = bare_shared();
    fold(
        &shared,
        &ServerMessage::Session {
            id: SessionId(1),
            person: seele_core::PersonId(7),
            ssrc: Ssrc(700),
            server: "Casa".into(),
            voice_rooms: Vec::new(),
            channels: Vec::new(),
            roles: Vec::new(),
            permissions: vec![Permission::Speak],
        },
    );
    assert!(
        !shared
            .room
            .lock()
            .unwrap()
            .permissions
            .contains(&Permission::ManageVoiceRooms),
        "a person who may only speak was told they may manage voice_rooms"
    );

    fold(
        &shared,
        &ServerMessage::Session {
            id: SessionId(1),
            person: seele_core::PersonId(7),
            ssrc: Ssrc(700),
            server: "Casa".into(),
            voice_rooms: Vec::new(),
            channels: Vec::new(),
            roles: Vec::new(),
            permissions: vec![Permission::Speak, Permission::ManageVoiceRooms],
        },
    );
    // And the field the screen actually reads, not only the list behind it.
    // Asserting on `room.permissions` alone would pass with
    // `may_manage_voice_rooms` hardcoded either way — measured, and it did.
    let (commands, _queue) = tokio::sync::mpsc::unbounded_channel();
    let connection = Connection {
        commands,
        shared: Arc::clone(&shared),
        encerrada: Mutex::new(None),
        ligacao: None,
    };
    assert!(connection.snapshot().may_manage_voice_rooms);

    fold(
        &shared,
        &ServerMessage::Session {
            id: SessionId(1),
            person: seele_core::PersonId(7),
            ssrc: Ssrc(700),
            server: "Casa".into(),
            voice_rooms: Vec::new(),
            channels: Vec::new(),
            roles: Vec::new(),
            permissions: vec![Permission::Speak],
        },
    );
    assert!(
        !connection.snapshot().may_manage_voice_rooms,
        "the snapshot went on offering the control after the permission went away"
    );
}

#[test]
fn the_snapshot_answers_each_moderation_verb_on_its_own() {
    // Four permissions, four questions, four booleans — and not one
    // `may_moderate`. `specs/04-servidor-seele.md` enumerates them
    // separately and a role may carry any subset, so collapsing them here
    // would draw the ban control for somebody who may only kick, and put a
    // shell in the position of teaching people that half its buttons say no.
    use seele_core::{Permission, ServerMessage, SessionId, Ssrc};

    let shared = bare_shared();
    let (commands, _queue) = tokio::sync::mpsc::unbounded_channel();
    let connection = Connection {
        commands,
        shared: Arc::clone(&shared),
        encerrada: Mutex::new(None),
        ligacao: None,
    };

    let sessao = |permissions: Vec<Permission>| ServerMessage::Session {
        id: SessionId(1),
        person: seele_core::PersonId(7),
        ssrc: Ssrc(700),
        server: "Casa".into(),
        voice_rooms: Vec::new(),
        channels: Vec::new(),
        roles: Vec::new(),
        permissions,
    };

    fold(&shared, &sessao(vec![Permission::Speak]));
    let nada = connection.snapshot();
    assert!(!nada.may_kick);
    assert!(!nada.may_ban);
    assert!(!nada.may_remove_message);
    assert!(!nada.may_move_person);

    // An Operador holding exactly one of the four. The assertion that
    // matters is the three `false`s beside the one `true`.
    fold(&shared, &sessao(vec![Permission::Speak, Permission::Kick]));
    let so_expulsa = connection.snapshot();
    assert!(so_expulsa.may_kick);
    assert!(
        !so_expulsa.may_ban && !so_expulsa.may_remove_message && !so_expulsa.may_move_person,
        "one moderation permission lit up the other three"
    );

    fold(
        &shared,
        &sessao(vec![
            Permission::Kick,
            Permission::Ban,
            Permission::RemoveMessage,
            Permission::MovePerson,
        ]),
    );
    let tudo = connection.snapshot();
    assert!(tudo.may_kick && tudo.may_ban && tudo.may_remove_message && tudo.may_move_person);

    // And they go away again. A snapshot that latched would go on offering
    // a control after a Comandante revoked it.
    fold(&shared, &sessao(Vec::new()));
    let depois = connection.snapshot();
    assert!(
        !depois.may_kick && !depois.may_ban && !depois.may_remove_message,
        "the snapshot went on offering the controls after the permissions went away"
    );
}

#[test]
fn destroying_a_room_is_a_permission_of_its_own_and_not_the_one_that_makes_them() {
    // The decision this whole path turns on, asserted where a shell reads
    // it. Making and renaming a room are mistakes a server survives;
    // destroying one ends what other people wrote. A role that may build
    // rooms without being able to unmake them is a role somebody can
    // actually write — `specs/04-servidor-seele.md` enumerates
    // `gerenciar_voice_rooms` and `administrar_server` separately — and a single
    // boolean for both would make it impossible to offer correctly.
    use seele_core::{Permission, ServerMessage, SessionId, Ssrc};

    let shared = bare_shared();
    let (commands, _queue) = tokio::sync::mpsc::unbounded_channel();
    let connection = Connection {
        commands,
        shared: Arc::clone(&shared),
        encerrada: Mutex::new(None),
        ligacao: None,
    };

    let sessao = |permissions: Vec<Permission>| ServerMessage::Session {
        id: SessionId(1),
        person: seele_core::PersonId(7),
        ssrc: Ssrc(700),
        server: "Casa".into(),
        voice_rooms: Vec::new(),
        channels: Vec::new(),
        roles: Vec::new(),
        permissions,
    };

    // The role that builds and does not destroy. This is the pair the
    // separation exists for, and the one a single boolean would get wrong.
    fold(&shared, &sessao(vec![Permission::ManageVoiceRooms]));
    let constroi = connection.snapshot();
    assert!(constroi.may_manage_voice_rooms);
    assert!(
        !constroi.may_delete_rooms,
        "the permission to make a room was read as the permission to destroy one"
    );

    // And the reverse, so the two are not simply the same field read twice.
    fold(&shared, &sessao(vec![Permission::AdministerServer]));
    let administra = connection.snapshot();
    assert!(administra.may_delete_rooms);
    assert!(!administra.may_manage_voice_rooms);

    // Never the moderation permissions either: somebody trusted to remove a
    // person for the evening is not thereby trusted with the server's past.
    fold(
        &shared,
        &sessao(vec![
            Permission::Kick,
            Permission::Ban,
            Permission::RemoveMessage,
            Permission::MovePerson,
        ]),
    );
    assert!(
        !connection.snapshot().may_delete_rooms,
        "a moderation permission lit up the one that destroys rooms"
    );

    // And it goes away again, like the five beside it.
    fold(&shared, &sessao(Vec::new()));
    assert!(!connection.snapshot().may_delete_rooms);
}

/// **Os cargos chegam à casca por evento, com a marca no retrato** (pedido
/// do G: a camada de cargos relia em laço). Um quadro e duas recusas iguais
/// seguidas: três `CargosChanged`, a marca subindo a cada um e igual à do
/// `Snapshot`, e a segunda recusa com número maior que a primeira — a mesma
/// recusa de novo é outra recusa. Uma mensagem que não é de cargo não mexe.
#[test]
fn os_cargos_chegam_por_evento_com_a_marca_no_retrato() {
    use seele_core::cargos::{QuadroDeCargos, RecusaDeCargo};
    use seele_core::ServerMessage;

    #[derive(Default)]
    struct Marcas(Mutex<Vec<u64>>);
    impl EventListener for Marcas {
        fn on_event(&self, event: Event) {
            if let Event::CargosChanged { marca } = event {
                if let Ok(mut vistas) = self.0.lock() {
                    vistas.push(marca);
                }
            }
        }
    }

    let shared = bare_shared();
    let marcas = Arc::new(Marcas::default());
    shared
        .listeners
        .lock()
        .unwrap()
        .push(Arc::clone(&marcas) as Arc<dyn EventListener>);
    let (commands, _queue) = tokio::sync::mpsc::unbounded_channel();
    let connection = Connection {
        commands,
        shared: Arc::clone(&shared),
        encerrada: Mutex::new(None),
        ligacao: None,
    };
    assert_eq!(connection.snapshot().cargos_revision, 0);

    let quadro = QuadroDeCargos {
        cargos: Vec::new(),
        atribuicoes: Vec::new(),
        proprietario: None,
        de_mods: Vec::new(),
        revisao: 4,
        dono_declarado: false,
    };
    fold(&shared, &ServerMessage::QuadroDeCargos(quadro));
    let recusa = ServerMessage::CargoRecusado {
        motivo: RecusaDeCargo::SemPermissao,
    };
    fold(&shared, &recusa);
    let primeira = connection.cargos_na_tela().ultima_recusa.unwrap().seq;
    fold(&shared, &recusa);
    let segunda = connection.cargos_na_tela().ultima_recusa.unwrap().seq;
    fold(&shared, &ServerMessage::Pong { timestamp: 1 });

    assert_eq!(
        *marcas.0.lock().unwrap(),
        vec![1, 2, 3],
        "um evento por mensagem de cargo, e só delas"
    );
    assert_eq!(
        connection.snapshot().cargos_revision,
        3,
        "o retrato não leva a marca do último evento"
    );
    assert!(
        segunda > primeira,
        "a mesma recusa repetida não se distingue da anterior"
    );
    assert_eq!(connection.cargos_na_tela().quadro.unwrap().revisao, 4);
}

/// A shell watching a server dress itself, from the outside.
///
/// The three properties a screen depends on and no type can state: the name
/// arrives whole, the picture does not, and the two do not move each
/// other\u{2019}s counter.
/// **O tema do servidor vai no retrato, e `None` o tira** (tarefa 17d da
/// F3): o núcleo guarda o `TemaMudou`, e a casca só o aplica se o retrato
/// o levar. Os dois lados, como o do ícone.
#[test]
fn o_tema_do_servidor_vai_no_retrato_e_none_o_tira() {
    use seele_core::{ServerMessage, SessionId, Ssrc, TemaDoServidor};

    let shared = bare_shared();
    let (commands, _queue) = tokio::sync::mpsc::unbounded_channel();
    let connection = Connection {
        commands,
        shared: Arc::clone(&shared),
        encerrada: Mutex::new(None),
        ligacao: None,
    };
    fold(
        &shared,
        &ServerMessage::Session {
            id: SessionId(1),
            person: seele_core::PersonId(7),
            ssrc: Ssrc(700),
            server: "Casa".into(),
            voice_rooms: Vec::new(),
            channels: Vec::new(),
            roles: Vec::new(),
            permissions: Vec::new(),
        },
    );
    assert_eq!(
        connection.snapshot().tema_do_servidor,
        None,
        "a sessão nasce sem tema"
    );

    fold(
        &shared,
        &ServerMessage::TemaMudou {
            tema: Some(TemaDoServidor {
                mod_id: "seele/estilo".into(),
                valores: r##"{"acento":"#c9a227"}"##.into(),
            }),
        },
    );
    assert_eq!(
        connection.snapshot().tema_do_servidor,
        Some(TemaDaSessao {
            mod_id: "seele/estilo".into(),
            valores: r##"{"acento":"#c9a227"}"##.into(),
        }),
        "o tema guardado no núcleo não foi ao retrato"
    );

    fold(&shared, &ServerMessage::TemaMudou { tema: None });
    assert_eq!(
        connection.snapshot().tema_do_servidor,
        None,
        "o `None` do servidor não tirou o tema do retrato"
    );
}

#[test]
fn the_server_name_rides_the_snapshot_and_the_picture_rides_a_revision() {
    use seele_core::{Permission, ServerMessage, SessionId, Ssrc};

    let shared = bare_shared();
    let (commands, _queue) = tokio::sync::mpsc::unbounded_channel();
    let connection = Connection {
        commands,
        shared: Arc::clone(&shared),
        encerrada: Mutex::new(None),
        ligacao: None,
    };

    fold(
        &shared,
        &ServerMessage::Session {
            id: SessionId(1),
            person: seele_core::PersonId(7),
            ssrc: Ssrc(700),
            server: "Casa".into(),
            voice_rooms: Vec::new(),
            channels: Vec::new(),
            roles: Vec::new(),
            permissions: vec![Permission::AdministerServer],
        },
    );
    let inicio = connection.snapshot();
    assert_eq!(inicio.server, "Casa");
    assert_eq!(inicio.icon_revision, 0);
    assert_eq!(connection.server_icon(), None);
    assert!(
        inicio.may_customise_server,
        "whoever administers the server was not offered the control"
    );

    // The name travels whole, because it is a string and it is cheap.
    fold(
        &shared,
        &ServerMessage::ServerRenamed {
            name: "Casa".into(),
        },
    );
    let renomeado = connection.snapshot();
    assert_eq!(renomeado.server, "Casa");
    assert_eq!(
        renomeado.icon_revision, 0,
        "a rename made every shell refetch a picture that did not change"
    );

    // The picture does not: the snapshot only says that it moved.
    let bytes = vec![0x89, b'P', b'N', b'G', 4, 5, 6];
    fold(
        &shared,
        &ServerMessage::ServerIconChanged {
            icon: Some(bytes.clone()),
        },
    );
    assert_eq!(connection.snapshot().icon_revision, 1);
    assert_eq!(connection.server_icon(), Some(bytes));

    // Including when it moves to nothing. A revision that only counted
    // arrivals would leave the old picture on screen after it was taken
    // down, because the shell would never be told to look again.
    fold(&shared, &ServerMessage::ServerIconChanged { icon: None });
    assert_eq!(connection.snapshot().icon_revision, 2);
    assert_eq!(connection.server_icon(), None);
}

#[test]
fn a_picture_that_will_not_do_never_leaves_this_crate() {
    // The failure this prevents is not a wrong error message: it is the app
    // starting its five-minute internal battery because somebody picked a
    // PDF. A frame the protocol will not build is a send that fails, and a
    // send that fails is how a dropped link looks from `enlace`.
    let shared = bare_shared();
    let (commands, mut fila) = tokio::sync::mpsc::unbounded_channel();
    let connection = Connection {
        commands,
        shared,
        encerrada: Mutex::new(None),
        ligacao: None,
    };

    assert_eq!(
        connection.set_server_icon(Some(b"%PDF-1.7".to_vec())),
        Err(ConnectionError::IconNotAPicture)
    );
    assert!(
        fila.try_recv().is_err(),
        "a picture the server would refuse was queued to be sent anyway"
    );

    // And the heavy one answers with the ceiling, because the sentence
    // about it needs the number.
    let mut gorda = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    gorda.extend_from_slice(&13_u32.to_be_bytes());
    gorda.extend_from_slice(b"IHDR");
    gorda.extend_from_slice(&128_u32.to_be_bytes());
    gorda.extend_from_slice(&128_u32.to_be_bytes());
    gorda.resize(64 * 1024, 0);
    let Err(ConnectionError::IconTooBig { limit_bytes }) = connection.set_server_icon(Some(gorda))
    else {
        panic!("a picture over the ceiling was queued");
    };
    assert!(limit_bytes > 0);
    assert!(fila.try_recv().is_err());

    // Taking the picture down is never refused: it carries no bytes to
    // refuse, and whoever put one up has to be able to take it away.
    assert_eq!(connection.set_server_icon(None), Ok(()));
    assert!(
        matches!(fila.try_recv(), Ok(Command::SetServerIcon { icon: None })),
        "taking the picture down was swallowed instead of sent"
    );
}

#[test]
fn dressing_the_server_is_a_permission_of_its_own_field() {
    // Same permission as `may_delete_rooms` today, separate question. A
    // shell reading the destroy flag to decide whether to draw a rename box
    // would be leaning on a coincidence between two verbs.
    use seele_core::{Permission, ServerMessage, SessionId, Ssrc};

    let shared = bare_shared();
    let (commands, _queue) = tokio::sync::mpsc::unbounded_channel();
    let connection = Connection {
        commands,
        shared: Arc::clone(&shared),
        encerrada: Mutex::new(None),
        ligacao: None,
    };

    let sessao = |permissions: Vec<Permission>| ServerMessage::Session {
        id: SessionId(1),
        person: seele_core::PersonId(7),
        ssrc: Ssrc(700),
        server: "Casa".into(),
        voice_rooms: Vec::new(),
        channels: Vec::new(),
        roles: Vec::new(),
        permissions,
    };

    fold(&shared, &sessao(vec![Permission::ManageVoiceRooms]));
    assert!(
        !connection.snapshot().may_customise_server,
        "the permission to make rooms was read as the permission to name the server"
    );

    fold(&shared, &sessao(vec![Permission::AdministerServer]));
    assert!(connection.snapshot().may_customise_server);

    fold(&shared, &sessao(Vec::new()));
    assert!(
        !connection.snapshot().may_customise_server,
        "the snapshot went on offering the control after the permission went away"
    );
}

#[test]
fn the_weight_of_a_line_reaches_the_caller_who_asked_for_it() {
    // The number in the confirmation, and the one call on this bridge that
    // waits for an answer. A shell that cannot get these three numbers must
    // not open the box at all, so the wiring that carries them is worth
    // pinning: the question registers a slot, the server's reply fills it,
    // and the caller wakes with the counts unrounded.
    let shared = bare_shared();
    let (commands, mut queue) = tokio::sync::mpsc::unbounded_channel();
    let connection = Connection {
        commands,
        shared: Arc::clone(&shared),
        encerrada: Mutex::new(None),
        ligacao: None,
    };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let pergunta = tokio::spawn({
            let shared = Arc::clone(&shared);
            async move {
                // The driver's half, by hand: take the command off the
                // queue, park the sender, then let the answer arrive.
                let Some(Command::WeighChannel { channel, answer }) = queue.recv().await else {
                    panic!("nothing asked the server to weigh anything");
                };
                shared
                    .pending_weights
                    .lock()
                    .expect("pending")
                    .push((channel, answer));
                shared.answer_weight(ChannelWeight {
                    channel: 7,
                    messages: 1_847,
                    authors: 6,
                    oldest_at_seconds: Some(1_678_600_000),
                });
            }
        });

        let peso = connection.weigh_channel(7).await.expect("weight");
        pergunta.await.expect("driver");

        assert_eq!(peso.messages, 1_847);
        assert_eq!(peso.authors, 6);
        assert_eq!(peso.oldest_at_seconds, Some(1_678_600_000));
    });

    // And nothing was kept: the weight's whole value is being fresh, so
    // asking for it left the room exactly as it was.
    assert!(shared.room.lock().expect("room").channels.is_empty());
    assert!(shared.pending_weights.lock().expect("pending").is_empty());
}

/// A recusa do servidor a uma pesagem, como o `nao_deu` a manda.
fn recusa_do_peso(causa: seele_core::CausaDaRecusa) -> seele_core::ServerMessage {
    let pedido = (0..256)
        .find(|n| seele_core::nome_do_pedido(*n) == Some("WeighChannel"))
        .expect("o ordinal do WeighChannel");
    seele_core::ServerMessage::Alert {
        severity: seele_core::AlertSeverity::Warning,
        reason: seele_core::AlertReason::PedidoRecusado { pedido, causa },
        operator_text: None,
    }
}

/// **Cada recusa de uma pesagem volta a uma espera, e nenhuma vira faixa.**
///
/// Era o APAGAR de um canal pendurado: o servidor recusa o `WeighChannel`
/// pelo `nao_deu`, e nada resolvia a espera — a caixa «Não consegui
/// contar…» nunca abria, e cada clique a mais empilhava outra espera, viva
/// até a sessão acabar. Dois cliques aqui: duas esperas, duas recusas, as
/// duas voltam com a causa, e nada fica. E nenhuma faixa, porque a caixa
/// diz. Uma terceira recusa, sem ninguém esperando, vira faixa como
/// qualquer aviso.
#[test]
fn cada_recusa_de_uma_pesagem_volta_a_quem_perguntou_e_sem_faixa() {
    let shared = bare_shared();
    let (primeira, mut primeira_volta) = tokio::sync::oneshot::channel();
    let (segunda, mut segunda_volta) = tokio::sync::oneshot::channel();
    {
        let mut pending = shared.pending_weights.lock().expect("pending");
        pending.push((seele_core::ChannelId(7), primeira));
        pending.push((seele_core::ChannelId(7), segunda));
    }

    receber(
        &shared,
        &recusa_do_peso(seele_core::CausaDaRecusa::FalhaDoServidor),
    );
    receber(
        &shared,
        &recusa_do_peso(seele_core::CausaDaRecusa::NaoExiste),
    );

    assert_eq!(
        primeira_volta.try_recv(),
        Ok(Err(types::CausaDaRecusa::FalhaDoServidor)),
        "a primeira espera não voltou com a primeira recusa, e o APAGAR fica pendurado"
    );
    assert_eq!(
        segunda_volta.try_recv(),
        Ok(Err(types::CausaDaRecusa::NaoExiste)),
        "a segunda espera não voltou com a segunda recusa"
    );
    assert!(shared.pending_weights.lock().expect("pending").is_empty());
    assert_eq!(
        shared.room.lock().expect("room").notice,
        None,
        "a recusa respondida virou faixa também, e a caixa e a faixa dizem a mesma coisa"
    );

    receber(
        &shared,
        &recusa_do_peso(seele_core::CausaDaRecusa::FalhaDoServidor),
    );
    assert!(
        shared.room.lock().expect("room").notice.is_some(),
        "sem ninguém esperando, a recusa sumiu calada em vez de virar faixa"
    );
}

/// E pela pergunta inteira: `weigh_channel` volta com a causa, e não fica
/// esperando um peso que não vem.
#[test]
fn uma_pesagem_recusada_devolve_a_causa_a_quem_perguntou() {
    let shared = bare_shared();
    let (commands, mut queue) = tokio::sync::mpsc::unbounded_channel();
    let connection = Connection {
        commands,
        shared: Arc::clone(&shared),
        encerrada: Mutex::new(None),
        ligacao: None,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime");
    let resposta = runtime.block_on(async {
        let motor = tokio::spawn({
            let shared = Arc::clone(&shared);
            async move {
                let Some(Command::WeighChannel { channel, answer }) = queue.recv().await else {
                    panic!("nada pediu a pesagem");
                };
                shared
                    .pending_weights
                    .lock()
                    .expect("pending")
                    .push((channel, answer));
                receber(
                    &shared,
                    &recusa_do_peso(seele_core::CausaDaRecusa::FalhaDoServidor),
                );
            }
        });
        // Com prazo: sem o conserto a pergunta espera para sempre, e um
        // teste que pendura não reprova, só não termina.
        let resposta = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            connection.weigh_channel(7),
        )
        .await
        .expect("a pesagem recusada ficou esperando um peso que não vem");
        motor.await.expect("motor");
        resposta
    });
    assert_eq!(
        resposta.map(|peso| peso.messages),
        Err(ConnectionError::PedidoRecusado {
            causa: types::CausaDaRecusa::FalhaDoServidor,
        })
    );
}

#[test]
fn a_weight_for_another_line_leaves_this_caller_waiting() {
    // Two boxes cannot be open at once in one window, but a reply landing
    // against the wrong slot would be the kind of mistake that fills a
    // confirmation with somebody else's numbers — which is the exact
    // failure the counted number exists to rule out.
    let shared = bare_shared();
    let (answer, mut resposta) = tokio::sync::oneshot::channel();
    shared
        .pending_weights
        .lock()
        .expect("pending")
        .push((seele_core::ChannelId(7), answer));

    shared.answer_weight(ChannelWeight {
        channel: 9,
        messages: 3,
        authors: 1,
        oldest_at_seconds: None,
    });

    assert!(
        resposta.try_recv().is_err(),
        "a count for another Channel was handed to the box asking about this one"
    );
    assert_eq!(
        shared.pending_weights.lock().expect("pending").len(),
        1,
        "the question was dropped without being answered"
    );
}

#[test]
fn asking_for_a_room_with_no_name_asks_for_nothing() {
    // The same swallow `send_message` does for an empty body. Somebody who
    // pressed the button with nothing typed has not asked for anything, and
    // answering that with a red message puts an error on a screen where
    // nothing went wrong. The check is here as well as in `seele-proto`
    // because the proto one would arrive as a dropped connection rather
    // than as nothing happening.
    let (commands, mut queue) = tokio::sync::mpsc::unbounded_channel();
    let connection = Connection {
        commands,
        shared: bare_shared(),
        encerrada: Mutex::new(None),
        ligacao: None,
    };

    for blank in ["", "   ", "\t\n"] {
        connection.create_voice_room(blank.into(), 8, None).unwrap();
        connection.create_channel(blank.into()).unwrap();
        connection.rename_voice_room(1, blank.into()).unwrap();
        connection.rename_channel(1, blank.into()).unwrap();
    }
    assert!(
        queue.try_recv().is_err(),
        "a blank name was sent to the server"
    );

    connection
        .create_voice_room("SALA-02".into(), 8, None)
        .unwrap();
    assert!(
        matches!(queue.try_recv(), Ok(Command::CreateVoiceRoom { .. })),
        "a real name was swallowed too"
    );
}

#[test]
fn a_capture_device_crosses_with_both_halves_of_its_identity() {
    // A shell shows the name and sends back the id, and the two are not
    // interchangeable: two microphones of the same model report the same
    // name, so a shell that stored the name would leave the second one
    // unpickable — and one that *showed* the id would put a backend string
    // on screen where a person expects "Scarlett Solo".
    //
    // Skips out loud rather than passing on a machine with no microphone. A
    // test that succeeds whether or not the feature works is not a test, and
    // CI has no sound card.
    let found = capture_devices();
    if found.is_empty() {
        eprintln!("skipped: this machine lists no capture device");
        return;
    }

    for device in &found {
        assert!(
            !device.id.is_empty(),
            "a device with no id cannot be chosen"
        );
        assert!(
            !device.name.is_empty(),
            "a device with no name cannot be labelled"
        );
    }
    assert!(
        found.iter().filter(|device| device.default).count() <= 1,
        "two devices both claim to be the machine's default"
    );
}

#[test]
fn a_playback_device_crosses_with_both_halves_of_its_identity() {
    // The twin of the check above, for the same reason and with the same
    // skip: CI has no sound card, and a test that passes on a machine with
    // no speakers passes on every machine.
    let found = playback_devices();
    if found.is_empty() {
        eprintln!("skipped: this machine lists no playback device");
        return;
    }

    for device in &found {
        assert!(
            !device.id.is_empty(),
            "a device with no id cannot be chosen"
        );
        assert!(
            !device.name.is_empty(),
            "a device with no name cannot be labelled"
        );
    }
    assert!(
        found.iter().filter(|device| device.default).count() <= 1,
        "two outputs both claim to be the machine's default"
    );
}

/// O pseudoaparelho que o ALSA lista em toda máquina, **nos dois sentidos**: o
/// `null`, que joga fora o que se toca nele e devolve silêncio a quem grava.
///
/// Não é microfone nem saída de ninguém, e num Linux sem placa de som é a
/// lista inteira das duas perguntas — foi o que o runner do `ci.yml` devolveu
/// em 12/11/2026: `["alsa:null"]` de um lado e do outro.
const NULO_DO_ALSA: &str = "alsa:null";

/// O que a comparação das duas listas de aparelhos concluiu.
#[derive(Debug, PartialEq, Eq)]
enum DuasListas {
    /// Falta aparelho de verdade de um dos lados, e não há o que comparar. A
    /// frase diz o que faltou; `sem_saida` diz se foi a saída.
    SemAparelho { motivo: String, sem_saida: bool },
    /// Há uma saída que não é microfone: são duas listas.
    Distintas,
    /// Toda saída listada é também um microfone listado: uma lista é a outra.
    AMesma,
}

/// Compara os ids das duas listas, **sem o pseudoaparelho do ALSA**.
///
/// O `alsa:null` sai antes da conferência de vazio, e não depois: com ele
/// dentro, uma máquina sem placa tem «um microfone e uma saída», os dois com o
/// mesmo id, e a comparação acusa o produto de devolver a mesma lista duas
/// vezes — quando o que a máquina não tem é aparelho.
fn comparar_as_duas_listas(microfones: &[&str], saidas: &[&str]) -> DuasListas {
    let de_verdade = |lista: &[&str]| -> Vec<String> {
        lista
            .iter()
            .filter(|id| **id != NULO_DO_ALSA)
            .map(|id| (*id).to_owned())
            .collect()
    };
    let (microfones, saidas) = (de_verdade(microfones), de_verdade(saidas));
    if microfones.is_empty() || saidas.is_empty() {
        return DuasListas::SemAparelho {
            motivo: format!(
                "esta máquina lista {} microfone(s) e {} saída(s) de verdade (fora o \
                 `{NULO_DO_ALSA}`), e distinguir as duas listas pede um de cada",
                microfones.len(),
                saidas.len()
            ),
            sem_saida: saidas.is_empty(),
        };
    }
    if saidas.iter().any(|id| !microfones.contains(id)) {
        DuasListas::Distintas
    } else {
        DuasListas::AMesma
    }
}

#[test]
fn the_two_device_lists_are_not_the_same_list() {
    // Both are `Vec<_>` of three strings, and the two functions differ by one
    // word. A `playback_devices` that forwarded to the capture side would
    // compile, serialise, and draw a list of microphones under SAÍDA DE SOM
    // — and every pick from it would be refused by a machine that has no
    // such output.
    let (microphones, speakers) = (capture_devices(), playback_devices());
    let ids_dos_microfones: Vec<&str> = microphones.iter().map(|d| d.id.as_str()).collect();
    let ids_das_saidas: Vec<&str> = speakers.iter().map(|d| d.id.as_str()).collect();

    match comparar_as_duas_listas(&ids_dos_microfones, &ids_das_saidas) {
        DuasListas::Distintas => {}
        DuasListas::AMesma => panic!(
            "every listed output is also a listed microphone, so one list is the other: \
             microphones {ids_dos_microfones:?}, outputs {ids_das_saidas:?}"
        ),
        DuasListas::SemAparelho { motivo, sem_saida } => {
            // A mesma regra de `seele-audio/src/laco.rs`: pular tem a cor de
            // passar, e quem liga `SEELE_EXIGE_SAIDA_DE_SOM` está dizendo que
            // nesta máquina a saída tem de existir.
            assert!(
                !sem_saida || std::env::var_os("SEELE_EXIGE_SAIDA_DE_SOM").is_none(),
                "SEELE_EXIGE_SAIDA_DE_SOM está ligado e {motivo}: aqui o teste tinha de \
                 rodar, e não rodou"
            );
            eprintln!(
                "PULADO: {motivo}.\n  O que este teste prova precisa de um microfone e de \
                 uma saída de som de verdade (microfones: {ids_dos_microfones:?}; saídas: \
                 {ids_das_saidas:?}).\n  Ligue SEELE_EXIGE_SAIDA_DE_SOM para que faltar a \
                 saída vire falha em vez de pulo."
            );
        }
    }
}

/// O guarda do pulo de cima, e da comparação que ele não pode desligar.
///
/// As listas são as do runner de Linux sem placa de 12/11/2026, e a régua tem
/// de dizer «sem aparelho» — antes ela dizia «a mesma lista», e o teste
/// reprovava uma máquina por não ter som. As outras linhas seguram o que o
/// teste existe para pegar: tirar o `alsa:null` não pode fazer duas listas de
/// verdade iguais passarem.
#[test]
fn o_nulo_do_alsa_nao_conta_como_aparelho_e_a_mesma_lista_continua_reprovando() {
    assert_eq!(
        comparar_as_duas_listas(&["alsa:null"], &["alsa:null"]),
        DuasListas::SemAparelho {
            motivo: "esta máquina lista 0 microfone(s) e 0 saída(s) de verdade (fora o \
                     `alsa:null`), e distinguir as duas listas pede um de cada"
                .to_owned(),
            sem_saida: true,
        },
        "o runner sem placa de som lista só o `alsa:null` dos dois lados: é pulo, e não a \
         mesma lista"
    );
    assert!(
        matches!(
            comparar_as_duas_listas(&["alsa:null", "alsa:hw:0,0"], &["alsa:null"]),
            DuasListas::SemAparelho {
                sem_saida: true,
                ..
            }
        ),
        "com microfone e sem saída de verdade ainda não há o que comparar"
    );
    assert!(
        matches!(
            comparar_as_duas_listas(&[], &["coreaudio:alto-falantes"]),
            DuasListas::SemAparelho {
                sem_saida: false,
                ..
            }
        ),
        "sem microfone a saída existe, e quem exige a saída não pode reprovar por isso"
    );
    assert_eq!(
        comparar_as_duas_listas(&["alsa:null", "alsa:hw:0,0"], &["alsa:null", "alsa:hw:0,0"]),
        DuasListas::AMesma,
        "duas listas de verdade iguais são o defeito que o teste procura, com ou sem o nulo"
    );
    assert_eq!(
        comparar_as_duas_listas(
            &["alsa:null", "alsa:hw:0,0"],
            &["alsa:null", "alsa:hw:0,0", "alsa:hdmi:0,3"]
        ),
        DuasListas::Distintas
    );
    assert_eq!(
        comparar_as_duas_listas(&["coreaudio:mic"], &["coreaudio:alto-falantes"]),
        DuasListas::Distintas
    );
}

#[test]
fn a_capture_device_serialises_the_field_names_a_shell_reads() {
    // The desktop shell is untyped JavaScript reading these three names off
    // the wire. A `serde(rename)` added here would leave every row drawn
    // `undefined`, with nothing failing anywhere — the same defect class
    // `apps/seele-app/tests/frontend.rs` guards for `Match`.
    let device = CaptureDevice {
        id: "coreaudio:alguma-coisa".into(),
        name: "Scarlett Solo".into(),
        default: true,
    };
    let Ok(json) = serde_json::to_string(&device) else {
        panic!("CaptureDevice does not serialise, so no shell can read it at all");
    };
    for field in ["\"id\"", "\"name\"", "\"default\""] {
        assert!(
            json.contains(field),
            "a capture device no longer carries {field}: {json}"
        );
    }
}

#[test]
fn a_playback_device_serialises_the_field_names_a_shell_reads() {
    // Its own test and not an addition to the one above, because the two
    // types are separate declarations: a `serde(rename)` on this one would
    // leave every row under SAÍDA DE SOM drawn `undefined` while the
    // microphone list next to it kept working, which is the version of this
    // failure that takes longest to believe.
    let device = PlaybackDevice {
        id: "coreaudio:alguma-saida".into(),
        name: "Fones de ouvido".into(),
        default: true,
    };
    let Ok(json) = serde_json::to_string(&device) else {
        panic!("PlaybackDevice does not serialise, so no shell can read it at all");
    };
    for field in ["\"id\"", "\"name\"", "\"default\""] {
        assert!(
            json.contains(field),
            "a playback device no longer carries {field}: {json}"
        );
    }
}

#[test]
fn a_snapshot_with_no_audio_names_no_microphone() {
    // The two have to agree, always: a screen that read a device off a
    // session with no voice path would draw a microphone that is not open.
    // `AudioState::sem_voz` is the one place that pairing is decided, and
    // it starts from what a person who chose nothing has chosen.
    let quiet = AudioState::sem_voz(
        &ControlesGuardados::de_partida(seele_core::DeviceChoice::default()),
        AberturaDoAudio::ForaDeSala,
    );
    assert!(!quiet.available);
    assert_eq!(quiet.capture, None);
    assert_eq!(
        quiet.playback, None,
        "a session with no audio must not name an output it is not playing through"
    );
    assert_eq!(
        quiet.mode,
        VoiceMode::PushToTalk,
        "a session with no audio must not read as an open microphone"
    );
}

#[test]
fn a_pong_wakes_nobody() {
    // Every event delivered is a redraw somewhere. The round-trip
    // measurement is consumed by the core and is not news.
    use seele_core::ServerMessage;

    struct Counter(std::sync::atomic::AtomicUsize);
    impl EventListener for Counter {
        fn on_event(&self, _: Event) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    let shared = Arc::new(Shared {
        caminho_medido: std::sync::atomic::AtomicU32::new(0),
        link_battery: AtomicBool::new(false),
        link_seconds: std::sync::atomic::AtomicU64::new(0),
        link_attempts: std::sync::atomic::AtomicU32::new(0),
        messages_revision: std::sync::atomic::AtomicU64::new(0),
        fora_do_ar_revision: std::sync::atomic::AtomicU64::new(0),
        icon_revision: std::sync::atomic::AtomicU64::new(0),
        person_icons_revision: std::sync::atomic::AtomicU64::new(0),
        perfis: perfis::DoPerfil::negociado(seele_core::PROTOCOL_VERSION),
        room: Mutex::new(Room::new()),
        listeners: Mutex::new(Vec::new()),
        sessao: SessaoId::nova(),
        geracao: std::sync::atomic::AtomicU64::new(1),
        voice: VozDaSala::nova(false, seele_core::DeviceChoice::default()),
        media: Mutex::new(None),
        nickname: Mutex::new("marcela".into()),
        link_state: AtomicU8::new(0),
        rtt_micros: std::sync::atomic::AtomicU64::new(0),
        jitter_de_chegada_micros: std::sync::atomic::AtomicU64::new(0),
        caminho: Mutex::new(None),
        impressao_do_servidor: Mutex::new(None),
        signal: AtomicU8::new(0),
        running: AtomicBool::new(true),
        pending_weights: Mutex::new(Vec::new()),
        tuneis: Mutex::new(TabelaDeTuneis::default()),
        limites_da_tela: Mutex::new(None),
        parada_da_tela: Mutex::new(Default::default()),
        codificador_da_tela: Mutex::new(None),
        tela_escolhida: Mutex::new(EscolhaDeTela::NinguemEscolheu),
        tela_ligada: Mutex::new(seele_core::retrato::TelaLigada::default()),
        fontes_de_tela: Mutex::new(Vec::new()),
        camera: Mutex::new(None),
    });
    let counter = Arc::new(Counter(std::sync::atomic::AtomicUsize::new(0)));
    shared
        .listeners
        .lock()
        .unwrap()
        .push(Arc::clone(&counter) as Arc<dyn EventListener>);

    fold(&shared, &ServerMessage::Pong { timestamp: 1 });

    assert_eq!(counter.0.load(Ordering::Relaxed), 0);
}

/// Uma sala com uma transmissão de tela em curso, e mais ninguém dentro.
///
/// Devolve o `Room` para o teste acrescentar gente: é a contagem de
/// espectadores que os testes abaixo mexem, e ela é o N do §5.1.
/// Um `Shared` vazio, para exercer o `fold` sem uma conexão.
/// **A lista de membros não vai no `Snapshot`** (P6, decisão da
/// orquestradora).
///
/// O `Snapshot` inteiro ainda vai a todo MOD da API 5 para baixo
/// (`base.js`, a mensagem `snapshot`), e a lista de membros do servidor não é
/// dele. O FORA DO AR sai por [`Connection::membros_fora_do_ar`]. Se alguém
/// puser os membros fora do ar no `Snapshot`, o apelido aparece no JSON que
/// a janela e os MODs leem, e este teste reprova.
#[test]
fn o_snapshot_serializado_nao_leva_nenhum_membro_fora_do_ar() {
    use seele_core::{ServerMessage, SessionId, Ssrc};
    let shared = compartilhado_de_teste();
    fold(
        &shared,
        &ServerMessage::Session {
            id: SessionId(1),
            person: seele_core::PersonId(7),
            ssrc: Ssrc(700),
            server: "Casa".into(),
            voice_rooms: Vec::new(),
            channels: Vec::new(),
            roles: Vec::new(),
            permissions: Vec::new(),
        },
    );
    let apelido = "membro-que-nenhum-mod-pode-ler";
    fold(
        &shared,
        &ServerMessage::Membros {
            membros: vec![seele_core::Membro {
                id: seele_core::PersonId(42),
                nickname: apelido.into(),
            }],
            fim: true,
        },
    );
    let (commands, _fila) = tokio::sync::mpsc::unbounded_channel();
    let connection = Connection {
        commands,
        shared: Arc::clone(&shared),
        encerrada: Mutex::new(None),
        ligacao: None,
    };
    // O controle: o membro chegou, e a janela o lê pelo acessor. Sem isto,
    // um `Snapshot` sem ele não provaria nada.
    assert_eq!(
        connection.membros_fora_do_ar(),
        vec![MembroForaDoAr {
            id: 42,
            nickname: apelido.into(),
        }],
        "o FORA DO AR não chegou ao acessor, e o teste mediu outra coisa"
    );
    let snapshot = serde_json::to_string(&connection.snapshot()).expect("o snapshot vira JSON");
    assert!(
        !snapshot.contains(apelido),
        "o `Snapshot` leva um membro fora do ar, e todo MOD da API 5 para baixo o lê"
    );
}

/// **O número do FORA DO AR já subiu quando o `RosterChanged` chega.**
///
/// A casca relê a lista quando o evento chega, e só se o número andou. Um
/// número que sobe depois do aviso é um que ela pode ler velho, e aí o
/// grupo fica errado até a próxima mudança de membros — que pode não vir.
/// O ouvinte lê o número no instante do aviso, que é o instante em que a
/// casca o leria.
#[test]
fn o_numero_do_fora_do_ar_ja_subiu_quando_o_aviso_chega() {
    use seele_core::ServerMessage;

    struct Leitor(std::sync::Weak<Shared>, Mutex<Vec<u64>>);
    impl EventListener for Leitor {
        fn on_event(&self, event: Event) {
            if !matches!(event, Event::RosterChanged) {
                return;
            }
            if let (Some(shared), Ok(mut lidos)) = (self.0.upgrade(), self.1.lock()) {
                lidos.push(
                    shared
                        .fora_do_ar_revision
                        .load(std::sync::atomic::Ordering::Relaxed),
                );
            }
        }
    }

    let shared = compartilhado_de_teste();
    let leitor = Arc::new(Leitor(Arc::downgrade(&shared), Mutex::new(Vec::new())));
    shared
        .listeners
        .lock()
        .unwrap()
        .push(Arc::clone(&leitor) as Arc<dyn EventListener>);

    fold(
        &shared,
        &ServerMessage::Membros {
            membros: vec![seele_core::Membro {
                id: seele_core::PersonId(42),
                nickname: "ritsuko".into(),
            }],
            fim: true,
        },
    );

    assert_eq!(
        leitor.1.lock().unwrap().as_slice(),
        &[1],
        "o `RosterChanged` saiu com o número do FORA DO AR ainda velho"
    );
}

pub(super) fn compartilhado_de_teste() -> Arc<Shared> {
    Arc::new(Shared {
        caminho_medido: std::sync::atomic::AtomicU32::new(0),
        link_battery: AtomicBool::new(false),
        link_seconds: std::sync::atomic::AtomicU64::new(0),
        link_attempts: std::sync::atomic::AtomicU32::new(0),
        messages_revision: std::sync::atomic::AtomicU64::new(0),
        fora_do_ar_revision: std::sync::atomic::AtomicU64::new(0),
        icon_revision: std::sync::atomic::AtomicU64::new(0),
        person_icons_revision: std::sync::atomic::AtomicU64::new(0),
        perfis: perfis::DoPerfil::negociado(seele_core::PROTOCOL_VERSION),
        room: Mutex::new(Room::new()),
        listeners: Mutex::new(Vec::new()),
        sessao: SessaoId::nova(),
        geracao: std::sync::atomic::AtomicU64::new(1),
        voice: VozDaSala::nova(false, seele_core::DeviceChoice::default()),
        media: Mutex::new(None),
        nickname: Mutex::new("marcela".into()),
        link_state: AtomicU8::new(0),
        rtt_micros: std::sync::atomic::AtomicU64::new(0),
        jitter_de_chegada_micros: std::sync::atomic::AtomicU64::new(0),
        caminho: Mutex::new(None),
        impressao_do_servidor: Mutex::new(None),
        signal: AtomicU8::new(0),
        running: AtomicBool::new(true),
        pending_weights: Mutex::new(Vec::new()),
        tuneis: Mutex::new(TabelaDeTuneis::default()),
        limites_da_tela: Mutex::new(None),
        parada_da_tela: Mutex::new(Default::default()),
        codificador_da_tela: Mutex::new(None),
        tela_escolhida: Mutex::new(EscolhaDeTela::NinguemEscolheu),
        tela_ligada: Mutex::new(seele_core::retrato::TelaLigada::default()),
        fontes_de_tela: Mutex::new(Vec::new()),
        camera: Mutex::new(None),
    })
}

/// Um retrato novo faz a revisão dos retratos andar.
///
/// **É a diferença deste número que manda a casca rebuscar as imagens.**
/// Parada, o cache dela nunca invalida — e o efeito era completo e
/// silencioso: quem trocava a própria imagem via a prévia do diálogo mudar,
/// porque ela lê o arquivo desta máquina, e mais nada mudava em lugar
/// nenhum. Nem o bloco do operador, nem a grade da chamada, nem para as
/// outras pessoas, cujo cliente tinha o mesmo defeito.
///
/// A causa foi a subida ter nascido no ramo errado: ao lado da revisão do
/// retrato **do servidor**, por semelhança, dentro de um `if changed.server`
/// — e `PersonIconChanged` marca `changed.roster`. Duas partes diferentes do
/// estado, e o ramo nunca rodava para ela.
#[test]
fn um_retrato_novo_faz_a_revisao_dos_retratos_andar() {
    use seele_core::{PersonId, ServerMessage};

    let shared = compartilhado_de_teste();
    let antes = shared.person_icons_revision.load(Ordering::Relaxed);

    fold(
        &shared,
        &ServerMessage::PersonIconChanged {
            person: PersonId(7),
            icon: Some(vec![1, 2, 3]),
        },
    );

    assert!(
        shared.person_icons_revision.load(Ordering::Relaxed) > antes,
        "a revisão dos retratos não andou com um `PersonIconChanged`, e o \
             cache da casca nunca vai rebuscar imagem nenhuma"
    );
}

/// E uma mensagem que não é de retrato **não** a faz andar.
///
/// Sem isto, a asserção acima passaria com a subida solta fora de qualquer
/// condição — e cada quadro do roster mandaria a casca rebuscar 8 KiB por
/// pessoa, que é exatamente o custo que a revisão existe para evitar.
#[test]
fn um_nome_novo_nao_faz_a_revisao_dos_retratos_andar() {
    use seele_core::{PersonId, ServerMessage};

    let shared = compartilhado_de_teste();
    let antes = shared.person_icons_revision.load(Ordering::Relaxed);

    fold(
        &shared,
        &ServerMessage::PersonRenamed {
            person: PersonId(7),
            nickname: "aleta".into(),
        },
    );

    assert_eq!(
        shared.person_icons_revision.load(Ordering::Relaxed),
        antes,
        "trocar de nome mandou a casca rebuscar todos os retratos"
    );
}

fn sala_com_tela(quem_compartilha: seele_core::PersonId) -> Room {
    use seele_core::{PersonId, ScreenId, ServerMessage, SessionId, Ssrc, VoiceRoomInfo};

    let mut room = Room::new();
    room.apply(&ServerMessage::Session {
        id: SessionId(1),
        person: PersonId(7),
        ssrc: Ssrc(700),
        server: "Casa".into(),
        voice_rooms: vec![VoiceRoomInfo {
            id: VoiceRoomId(1),
            name: "SALA-01".into(),
            limit: 20,
            password_required: false,
            channel: None,
        }],
        channels: Vec::new(),
        roles: Vec::new(),
        permissions: Vec::new(),
    });
    room.enter_voice_room(VoiceRoomId(1));
    room.apply(&ServerMessage::ScreenShareStarted {
        voice_room: VoiceRoomId(1),
        person: quem_compartilha,
        screen: ScreenId(9),
    });
    room
}

/// Senta mais uma pessoa na sala.
fn sentar(room: &mut Room, pessoa: u64) {
    use seele_core::{PersonId, PersonProfile, ServerMessage, Ssrc};

    room.apply(&ServerMessage::PersonJoined {
        voice_room: VoiceRoomId(1),
        profile: PersonProfile {
            id: PersonId(pessoa),
            nickname: format!("pessoa-{pessoa}"),
            roles: Vec::new(),
            material_e2ee: None,
        },
        ssrc: Ssrc(pessoa.wrapping_mul(10).try_into().unwrap_or(1)),
    });
}

/// **A contagem de quem assiste é a do servidor, e não a do roster daqui.**
///
/// # O que mudou no R20
///
/// Era «quem está na sala menos quem compartilha», contado neste lado. Isso
/// **inclui quem escolheu não assistir** — e desde que a escolha existe, isso
/// é gente de verdade. Duas contagens para a mesma pergunta, discordando, e o
/// servidor já mandava a certa: `ScreenViewers` conta assinaturas, e é o mesmo
/// N pelo qual ele divide o teto no §5.1.
#[test]
fn a_tela_conta_quem_o_servidor_disse_que_assiste() {
    use seele_core::{PersonId, ScreenId, ServerMessage};

    let mut room = sala_com_tela(PersonId(3));
    sentar(&mut room, 3);
    sentar(&mut room, 4);

    let tela = tela_por_id(&room, ScreenId(9), None).expect("a transmissão foi anunciada");
    assert_eq!(tela.tela, 9, "a transmissão não diz qual ela é");
    assert_eq!(tela.de, 3);
    assert!(
        !tela.e_minha,
        "quem compartilha é a pessoa 3, e nós somos a 7"
    );
    // **Zero até o servidor contar.** Zero é a resposta honesta: uma
    // transmissão que começou agora não tem ninguém assistindo ainda, e
    // deduzir do roster era contar quem recusou.
    assert_eq!(
        tela.espectadores, 0,
        "a contagem voltou a ser deduzida do roster desta máquina, e ela \
             inclui quem escolheu não assistir"
    );

    room.apply(&ServerMessage::ScreenViewers {
        tela: ScreenId(9),
        quantos: 2,
    });
    assert_eq!(
        tela_por_id(&room, ScreenId(9), None)
            .expect("a transmissão continua")
            .espectadores,
        2,
        "o `ScreenViewers` do servidor não chegou à ponte"
    );
}

/// E uma transmissão que não é a pedida não é devolvida no lugar dela.
///
/// R18: `tela_de` pegava a **primeira** do mapa da sala, então numa sala com
/// dois transmissores o painel podia falar de uma e a imagem ser de outra.
#[test]
fn a_tela_e_procurada_por_identidade() {
    use seele_core::{PersonId, ScreenId, ServerMessage};

    let mut room = sala_com_tela(PersonId(3));
    room.apply(&ServerMessage::ScreenShareStarted {
        voice_room: VoiceRoomId(1),
        person: PersonId(4),
        screen: ScreenId(11),
    });

    assert_eq!(
        tela_por_id(&room, ScreenId(11), None)
            .expect("a segunda transmissão existe")
            .de,
        4,
        "a busca por identidade devolveu outra transmissão"
    );
    assert_eq!(
        tela_por_id(&room, ScreenId(9), None)
            .expect("a primeira continua")
            .de,
        3
    );
    assert!(
        tela_por_id(&room, ScreenId(99), None).is_none(),
        "uma transmissão que não existe foi respondida com outra"
    );
}

#[test]
fn a_tela_de_quem_compartilha_se_reconhece() {
    use seele_core::{PersonId, ScreenId};

    // O pessoa 7 é quem esta sessão é — `sala_com_tela` o diz na `Session`.
    let room = sala_com_tela(PersonId(7));
    let tela = tela_por_id(&room, ScreenId(9), None).expect("a transmissão foi anunciada");
    assert!(
        tela.e_minha,
        "sem isto a casca desenha o painel de quem assiste para quem compartilha"
    );
    // E `minha_tela_de` a encontra sem que ninguém diga o `ScreenId`: é a
    // pergunta «qual é a minha», e ela é por identidade também — pela da
    // pessoa. ADR 0054.
    assert_eq!(
        minha_tela_de(&room, None).map(|minha| minha.tela),
        Some(9),
        "a própria transmissão não é encontrada por identidade"
    );
}

#[test]
fn nao_ha_tela_quando_ninguem_compartilha() {
    use seele_core::{PersonId, ScreenId, ServerMessage, SessionId, Ssrc, VoiceRoomInfo};

    let mut room = Room::new();
    room.apply(&ServerMessage::Session {
        id: SessionId(1),
        person: PersonId(7),
        ssrc: Ssrc(700),
        server: "Casa".into(),
        voice_rooms: vec![VoiceRoomInfo {
            id: VoiceRoomId(1),
            name: "SALA-01".into(),
            limit: 20,
            password_required: false,
            channel: None,
        }],
        channels: Vec::new(),
        roles: Vec::new(),
        permissions: Vec::new(),
    });
    room.enter_voice_room(VoiceRoomId(1));
    assert!(tela_por_id(&room, ScreenId(9), None).is_none());
    assert!(minha_tela_de(&room, None).is_none());
}

/// **A ponte não finge saber o que está saindo, e já não reserva lugar.**
///
/// # O que este teste era, e por que ele mudou
///
/// Ele prendia `medida == false` e os três zeros — «alguém passou a medir o
/// que sai: então preencha os três números e mude este teste». A regra estava
/// certa e os quatro campos nunca tiveram valor: a tela reservava três caixas
/// e escrevia «ainda não há medida» nas três, para sempre.
///
/// No R20 os quatro saíram da ponte, e a regra continua: nenhum campo de
/// medida existe enquanto não houver medida. O que este teste prende agora é
/// a **ausência** deles, e a mensagem diz o que fazer no dia em que houver.
#[test]
fn a_tela_nao_finge_saber_o_que_esta_saindo() {
    use seele_core::{PersonId, ScreenId};

    let tela = tela_por_id(&sala_com_tela(PersonId(3)), ScreenId(9), None)
        .expect("a transmissão foi anunciada");
    assert_eq!(
        tela.parada, None,
        "parar com motivo é decisão do teto, e nenhum teto roda deste lado"
    );
    // O guarda contra o campo que volta sem medida atrás dele mora em
    // `types::a_tela_em_curso_atravessa_pelo_nome_e_diz_de_qual_tela_fala`,
    // que é onde a forma do JSON é conferida. Aqui fica o que esta função
    // devolve, e ela devolve só o que se sabe.
    assert_eq!(tela.espectadores, 0, "a contagem foi inventada deste lado");
}

/// Um pedido qualquer, com os três controles do §5 preenchidos.
fn limites() -> LimitesDeTela {
    LimitesDeTela {
        banda_bps: Some(1_200_000),
        altura_maxima: 1080,
        quadros_maximos: 30,
        prioridade: crate::types::Prioridade::Nitidez,
        com_som: true,
    }
}

#[test]
fn o_pedido_so_aparece_ao_lado_da_propria_tela() {
    use seele_core::{PersonId, ScreenId};

    // O §5 manda pôr o que está saindo ao lado do que foi pedido. O que foi
    // pedido é escolha de quem transmite e **não viaja**: o `ScreenHeader`
    // carrega resolução e codec, nunca o teto. Então esta ponte só tem como
    // preencher a coluna da própria transmissão — e preenchê-la com a
    // escolha desta máquina ao lado da tela de outra pessoa seria mostrar o
    // teto de uma transmissão como se fosse o de outra.
    let minha = tela_por_id(&sala_com_tela(PersonId(7)), ScreenId(9), Some(limites()))
        .expect("a transmissão foi anunciada");
    assert!(minha.e_minha);
    assert_eq!(
        minha.pedido,
        Some(limites()),
        "quem compartilha perdeu a metade da comparação que o §5 obriga"
    );

    let alheia = tela_por_id(&sala_com_tela(PersonId(3)), ScreenId(9), Some(limites()))
        .expect("a transmissão foi anunciada");
    assert!(!alheia.e_minha);
    assert_eq!(
        alheia.pedido, None,
        "o teto escolhido nesta máquina foi mostrado como se fosse o de quem compartilha"
    );
}

#[test]
fn o_pedido_morre_com_a_transmissao_e_nao_com_o_roster() {
    use seele_core::{PersonId, ScreenId, ServerMessage};

    // A memória do que foi pedido saiu do JavaScript da casca — onde ela
    // morria com a janela — e passou a morar aqui. Isso troca um defeito por
    // outro se ela sobreviver à transmissão: a próxima vez que esta pessoa
    // compartilhasse mostraria o que está saindo agora ao lado de um teto de
    // outra vez, e nada na tela diria qual dos dois números vale.
    let shared = bare_shared();
    if let Ok(mut room) = shared.room.lock() {
        *room = sala_com_tela(PersonId(7));
    }
    shared.gravar_pedido_da_tela(Some(limites()));

    // Alguém entra na sala: o N do §5.1 anda, a transmissão continua, e o
    // que foi pedido continua valendo.
    fold(
        &shared,
        &ServerMessage::PersonJoined {
            voice_room: VoiceRoomId(1),
            profile: seele_core::PersonProfile {
                id: PersonId(4),
                nickname: "pessoa-4".into(),
                roles: Vec::new(),
                material_e2ee: None,
            },
            ssrc: Ssrc(40),
        },
    );
    assert_eq!(
        shared.pedido_da_tela(),
        Some(limites()),
        "uma pessoa entrando na sala apagou o teto de quem estava compartilhando"
    );

    // A transmissão acaba: o pedido acaba com ela.
    fold(
        &shared,
        &ServerMessage::ScreenShareStopped {
            voice_room: VoiceRoomId(1),
            screen: ScreenId(9),
        },
    );
    assert_eq!(
        shared.pedido_da_tela(),
        None,
        "o teto sobreviveu à transmissão a que ele pertencia"
    );
}

#[test]
fn um_pedido_recusado_nao_vira_memoria() {
    // `compartilhar_tela` guarda o que foi pedido **quando a transmissão
    // começa**. Enquanto ela não começa, guardar seria pôr na coluna do
    // pedido o teto de um botão que devolveu erro — e a coluna existe para
    // explicar a diferença entre o que se pediu e o que está saindo, não
    // para registrar tentativas.
    let fonte = include_str!("../lib.rs");
    let Some(corpo) = fonte
        .split("pub fn compartilhar_tela(")
        .nth(1)
        .and_then(|resto| resto.split("\n    }").next())
    else {
        panic!("`compartilhar_tela` mudou de forma; este guarda tem de mudar com ele");
    };
    assert!(
        corpo.contains("comecou.is_ok().then_some(limites)"),
        "`compartilhar_tela` guarda o pedido sem conferir se a transmissão começou:\n{corpo}"
    );
}

#[test]
fn o_roster_redesenha_a_tela_so_quando_ha_tela() {
    // §5.1: a contagem de espectadores é o N que divide o teto, e ela anda
    // quando alguém entra ou sai — sem nenhuma mensagem de tela chegar. Uma
    // casca que só redesenhasse em `ScreenShareStarted` escreveria
    // «4 pessoas assistindo» numa sala de seis.
    let so_roster = seele_core::Changed {
        roster: true,
        ..seele_core::Changed::default()
    };
    let so_tela = seele_core::Changed {
        telas: true,
        ..seele_core::Changed::default()
    };

    assert!(a_tela_mudou(so_roster, true));
    assert!(
        !a_tela_mudou(so_roster, false),
        "sem transmissão em curso, todo mundo que entra numa sala acenderia o painel de tela"
    );
    assert!(
        a_tela_mudou(so_tela, false),
        "uma transmissão que acabou de acabar tem de apagar o painel"
    );
    assert!(a_tela_mudou(so_tela, true));
    assert!(!a_tela_mudou(seele_core::Changed::default(), true));
}

#[test]
fn um_ouvinte_sabe_que_a_tela_comecou_e_que_a_sala_cresceu() {
    use seele_core::{
        PersonId, PersonProfile, ScreenId, ServerMessage, SessionId, Ssrc, VoiceRoomInfo,
    };

    #[derive(Default)]
    struct Recorder(Mutex<Vec<Event>>);
    impl EventListener for Recorder {
        fn on_event(&self, event: Event) {
            if let Ok(mut seen) = self.0.lock() {
                seen.push(event);
            }
        }
    }

    let shared = bare_shared();
    let recorder = Arc::new(Recorder::default());
    shared
        .listeners
        .lock()
        .unwrap()
        .push(Arc::clone(&recorder) as Arc<dyn EventListener>);

    fold(
        &shared,
        &ServerMessage::Session {
            id: SessionId(1),
            person: PersonId(7),
            ssrc: Ssrc(700),
            server: "Casa".into(),
            voice_rooms: vec![VoiceRoomInfo {
                id: VoiceRoomId(1),
                name: "SALA-01".into(),
                limit: 20,
                password_required: false,
                channel: None,
            }],
            channels: Vec::new(),
            roles: Vec::new(),
            permissions: Vec::new(),
        },
    );
    shared.room.lock().unwrap().enter_voice_room(VoiceRoomId(1));

    // Alguém entra antes de haver tela: nada de `ScreenChanged`.
    fold(
        &shared,
        &ServerMessage::PersonJoined {
            voice_room: VoiceRoomId(1),
            profile: PersonProfile {
                id: PersonId(3),
                nickname: "marcela".into(),
                roles: Vec::new(),
                material_e2ee: None,
            },
            ssrc: Ssrc(30),
        },
    );
    assert!(
        !recorder
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|event| matches!(event, Event::ScreenChanged)),
        "o painel de tela acordou numa sala sem transmissão nenhuma"
    );

    fold(
        &shared,
        &ServerMessage::ScreenShareStarted {
            voice_room: VoiceRoomId(1),
            person: PersonId(3),
            screen: ScreenId(9),
        },
    );
    let comecou = recorder
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|event| matches!(event, Event::ScreenChanged))
        .count();
    assert_eq!(
        comecou, 1,
        "a transmissão começou e a casca não foi avisada"
    );

    // E agora o roster sozinho, com a tela em curso: o N mudou.
    fold(
        &shared,
        &ServerMessage::PersonJoined {
            voice_room: VoiceRoomId(1),
            profile: PersonProfile {
                id: PersonId(4),
                nickname: "pires".into(),
                roles: Vec::new(),
                material_e2ee: None,
            },
            ssrc: Ssrc(40),
        },
    );
    let depois = recorder
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|event| matches!(event, Event::ScreenChanged))
        .count();
    assert_eq!(
        depois, 2,
        "entrou a quinta pessoa, o teto foi dividido por mais um, e a tela não soube"
    );
}

#[test]
fn todo_motivo_de_parada_tem_um_nome_na_lista() {
    // A lista é escrita à mão porque `seele_core::MotivoDeParada` não
    // publica um `TODOS` — ao contrário de `chegada::Caminho`, que
    // `caminhos()` deriva. Este teste é o que prende as duas: um motivo novo
    // reprova no `match` de `nome_da_parada`, e um motivo esquecido na lista
    // reprova aqui.
    let nomes = motivos_de_parada_da_tela();
    assert_eq!(
        nomes.len(),
        2,
        "a lista de motivos andou e `nome_da_parada` não, ou o contrário"
    );
    assert!(nomes.contains(&nome_da_parada(seele_core::MotivoDeParada::SinalCritico)));
    assert!(nomes.contains(&nome_da_parada(seele_core::MotivoDeParada::AbaixoDoPiso)));
    let distintos: std::collections::BTreeSet<&str> = nomes.iter().copied().collect();
    assert_eq!(
        distintos.len(),
        nomes.len(),
        "dois motivos com o mesmo nome: a casca escreveria a mesma frase para os dois"
    );
}

#[test]
fn todo_motivo_de_queda_tem_um_nome_e_o_evento_o_leva_a_casca() {
    // O irmão do teste de cima, para a transmissão que **cai** em vez de
    // parar. A lista sai de `MotivoDeQueda::TODOS`, e `nome_da_queda` é um
    // `match` exaustivo: um motivo novo reprova lá na compilação, e um motivo
    // esquecido em `TODOS` reprova aqui, na contagem.
    let nomes = motivos_de_queda_da_tela();
    assert_eq!(
        nomes,
        vec!["Codificador", "Captura", "Fluxo"],
        "a lista de motivos de queda andou: a casca cobra uma frase por nome"
    );
    let distintos: std::collections::BTreeSet<&str> = nomes.iter().copied().collect();
    assert_eq!(
        distintos.len(),
        nomes.len(),
        "dois motivos com o mesmo nome: a casca escreveria a mesma frase para os dois"
    );
    // Nenhum nome de queda é nome de parada: os dois dicionários da casca
    // dizem coisas opostas («volta sozinha» e «compartilhe de novo»).
    for parada in motivos_de_parada_da_tela() {
        assert!(
            !nomes.contains(&parada),
            "`{parada}` é parada e queda ao mesmo tempo"
        );
    }

    // E o evento atravessa com a forma que a casca lê: qual transmissão, o
    // que ela mostrava, e o nome do motivo.
    for (origem, tipo) in [
        (seele_core::ScreenSource::Monitor, "monitor"),
        (seele_core::ScreenSource::Window, "window"),
        (seele_core::ScreenSource::Camera, "camera"),
    ] {
        for motivo in seele_core::MotivoDeQueda::TODOS {
            let evento = evento_da_queda(seele_core::ScreenId(7), origem, motivo);
            assert_eq!(
                serde_json::to_value(&evento).unwrap(),
                serde_json::json!({
                    "ScreenShareFell": {
                        "screen": 7,
                        "source": tipo,
                        "reason": nome_da_queda(motivo),
                    }
                }),
                "a queda de uma transmissão chegaria à casca com outra forma"
            );
        }
    }
}

#[test]
fn esta_ponte_nao_decide_quem_perdeu_a_corrida_da_tela() {
    // `specs/08-seguranca.md`: a interface esconder é conveniência; o
    // servidor negar é a segurança. Uma transmissão por sala (§6.3) é regra
    // do servidor, e quem perde a corrida é avisado por ele —
    // `AlertReason::ScreenShareTaken`, que já atravessa como
    // `NoticeReason::ScreenShareTaken`.
    //
    // O guarda lê a fonte porque a propriedade é sobre o que o código **não**
    // faz, e nenhum valor a exibe: um `compartilhar_tela` que conferisse
    // `room.telas` por conta própria compila, passa em tudo o mais, e recusa
    // localmente o que o servidor teria aceitado.
    let fonte = include_str!("../lib.rs");
    let Some(corpo) = fonte
        .split("pub fn compartilhar_tela(")
        .nth(1)
        .and_then(|depois| depois.split("\n    }").next())
    else {
        panic!("`compartilhar_tela` mudou de forma; este guarda tem de mudar com ele");
    };
    assert!(
        !corpo.contains("ScreenShareTaken"),
        "`compartilhar_tela` julga a corrida por conta própria:\n{corpo}"
    );
    assert!(
            !corpo.contains("telas"),
            "`compartilhar_tela` lê o mapa de transmissões para decidir; quem decide é o servidor:\n{corpo}"
        );
}
