//! Os dois produtores de retrato, com as mesmas mensagens: o do desktop
//! (`seele-ffi`) e o do navegador (`seele-web`). Tarefa 2.2 da F4, fase 2.
//!
//! É o critério que o ADR 0057 deu ao «quanto do núcleo vai para o `wasm`»: a
//! mesma sequência de `ServerMessage` aos dois, e os dois retratos iguais campo
//! a campo. O desktop dobra pelo `fold` da sessão viva (`seele_ffi::sem_rede`),
//! inteiro; o navegador recebe cada mensagem **como quadro**, pelo mesmo
//! decodificador da página, e monta o retrato pela projeção comum. Os dois
//! começam pela mesma entrada: o `adopt` da `SessionInfo` com o apelido
//! digitado.
//!
//! **A parte comparada é a que sai do `Room`**, com os nomes de campo do
//! `Snapshot` (`seele_sessao::retrato::RetratoDaSala`): o que cada ponta mede
//! (o enlace, o áudio, o sinal) não vem de mensagem nenhuma. Cada campo do
//! retrato do navegador tem de existir no do desktop com o mesmo valor, depois
//! de cada mensagem e não só no fim.
//!
//! A sequência passa pelo que a casca desenha: gente presente e na sala, a
//! entrada desta pessoa, **o tema que chega e o que sai** (`TemaMudou`, a 17d
//! da F3), uma transmissão alheia e a própria, os espectadores, um aviso, a
//! troca do próprio apelido, a queda recuperável (`FellBehind`, que não é fim:
//! pendência 61, lote P61 da F5) e o fim.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use seele_proto::control::{
    AlertReason, AlertSeverity, ChannelInfo, DisconnectReason, Permission, PersonProfile,
    ServerMessage, TemaDoServidor, VoiceRoomInfo,
};
use seele_proto::ids::{ChannelId, PersonId, ScreenId, SessionId, Ssrc, VoiceRoomId};
use seele_proto::version::PROTOCOL_VERSION;
use seele_web::sessao::Sessao;

const EU: PersonId = PersonId(7);
const SALA: VoiceRoomId = VoiceRoomId(2);
const APELIDO: &str = "marcela";

fn sessao() -> seele_core::SessionInfo {
    seele_core::SessionInfo {
        id: SessionId(41),
        person: EU,
        ssrc: Ssrc(700),
        server: "Casa".into(),
        voice_rooms: vec![VoiceRoomInfo {
            id: SALA,
            name: "SALA-02".into(),
            limit: 0,
            password_required: false,
            channel: Some(ChannelId(1)),
        }],
        channels: vec![ChannelInfo {
            id: ChannelId(1),
            name: "geral".into(),
        }],
        permissions: vec![Permission::Speak, Permission::Kick],
    }
}

fn perfil(id: u64, apelido: &str) -> PersonProfile {
    PersonProfile {
        id: PersonId(id),
        nickname: apelido.into(),
        roles: Vec::new(),
        material_e2ee: None,
    }
}

fn mensagens() -> Vec<ServerMessage> {
    vec![
        ServerMessage::PersonPresent {
            profile: perfil(8, "ana"),
            ssrc: Ssrc(800),
        },
        ServerMessage::PersonJoined {
            voice_room: SALA,
            profile: perfil(9, "bia"),
            ssrc: Ssrc(900),
        },
        ServerMessage::MovedToVoiceRoom { voice_room: SALA },
        ServerMessage::TemaMudou {
            tema: Some(TemaDoServidor {
                mod_id: "seele/estilo".into(),
                valores: r##"{"acento":"#c86b1f"}"##.into(),
            }),
        },
        ServerMessage::ScreenShareStarted {
            voice_room: SALA,
            person: PersonId(9),
            screen: ScreenId(3),
        },
        ServerMessage::ScreenViewers {
            tela: ScreenId(3),
            quantos: 2,
        },
        ServerMessage::ScreenShareStarted {
            voice_room: SALA,
            person: EU,
            screen: ScreenId(4),
        },
        ServerMessage::Alert {
            severity: AlertSeverity::Warning,
            reason: AlertReason::VoiceRoomFull,
            operator_text: None,
        },
        ServerMessage::PersonRenamed {
            person: EU,
            nickname: "marcela2".into(),
        },
        ServerMessage::TemaMudou { tema: None },
        ServerMessage::Disconnecting {
            reason: DisconnectReason::FellBehind,
        },
        ServerMessage::Disconnecting {
            reason: DisconnectReason::Kicked,
        },
    ]
}

#[test]
fn os_dois_produtores_dao_o_mesmo_retrato_depois_de_cada_mensagem() {
    let mensagens = mensagens();
    let do_desktop = seele_ffi::sem_rede::retratos(&sessao(), APELIDO, &mensagens);

    let mut navegador = Sessao::adotar(&sessao(), APELIDO);
    let mut do_navegador = vec![navegador.retrato()];
    for mensagem in &mensagens {
        let quadro = seele_proto::control::encode_em(PROTOCOL_VERSION, mensagem)
            .expect("um quadro do servidor");
        navegador
            .dobrar_quadro(&quadro)
            .expect("o navegador leu o quadro");
        do_navegador.push(navegador.retrato());
    }
    assert_eq!(do_desktop.len(), mensagens.len() + 1);
    assert_eq!(do_navegador.len(), mensagens.len() + 1);

    for (passo, (desktop, navegador)) in do_desktop.iter().zip(&do_navegador).enumerate() {
        let depois = if passo == 0 {
            "a entrada".to_owned()
        } else {
            format!("{:?}", mensagens[passo - 1])
        };
        let desktop = serde_json::to_value(desktop).expect("o retrato do desktop em JSON");
        let navegador = serde_json::to_value(navegador).expect("o retrato do navegador em JSON");
        let campos = navegador.as_object().expect("um objeto");
        assert_eq!(
            campos.len(),
            19,
            "o retrato da sala tem {} campos, e a lista deste teste é de 19",
            campos.len()
        );
        for (campo, valor) in campos {
            assert_eq!(
                desktop.get(campo),
                Some(valor),
                "depois de {depois}, o campo `{campo}` do navegador não é o do desktop"
            );
        }
    }

    // A sequência mexe no que diz mexer: sem isto, dois retratos vazios
    // passariam por iguais.
    let tema = |passo: usize| {
        serde_json::to_value(&do_navegador[passo]).unwrap()["tema_do_servidor"].clone()
    };
    assert!(tema(3).is_null(), "havia tema antes do TemaMudou");
    assert_eq!(
        tema(4)["mod_id"],
        "seele/estilo",
        "o TemaMudou não chegou ao retrato"
    );
    assert!(tema(10).is_null(), "o TemaMudou sem tema não tirou o tema");
    // **A queda recuperável não é fim, nos dois produtores** (lote P61, o
    // pedido da F4). O `FellBehind` é a despedida que o núcleo trata como
    // queda e reconecta pela bateria; o `Room` marcava fim para toda
    // `Disconnecting`, e o desktop e o navegador mostravam a tela de fim por
    // cima de uma sessão que voltava. A regra é uma só:
    // `seele_sessao::despedida`.
    let depois_da_queda = 11;
    assert!(
        matches!(
            mensagens[depois_da_queda - 1],
            ServerMessage::Disconnecting {
                reason: DisconnectReason::FellBehind
            }
        ),
        "o passo da queda saiu do lugar"
    );
    for (lado, retrato) in [
        (
            "desktop",
            serde_json::to_value(&do_desktop[depois_da_queda]).unwrap(),
        ),
        (
            "navegador",
            serde_json::to_value(&do_navegador[depois_da_queda]).unwrap(),
        ),
    ] {
        assert!(
            retrato["ended"].is_null(),
            "o {lado} marcou fim numa queda recuperável: {}",
            retrato["ended"]
        );
    }
    let fim = &do_navegador[mensagens.len()];
    assert_eq!(
        fim.nickname, "marcela2",
        "a troca do próprio apelido não chegou"
    );
    assert!(
        fim.may_kick && !fim.may_ban,
        "as permissões da sessão não chegaram"
    );
    assert!(
        fim.minha_transmissao.is_some(),
        "a transmissão própria não chegou"
    );
    assert_eq!(
        fim.transmissoes.len(),
        2,
        "as duas transmissões da sala não chegaram"
    );
    assert!(
        fim.notice.is_some() && fim.ended.is_some(),
        "o aviso ou o fim não chegaram"
    );
}

/// O que cada ponta mede: não vem de mensagem nenhuma, e o navegador o mede do
/// lado dele (o WebTransport, o `voz-web.js`).
const MEDIDOS_POR_CADA_PONTA: &[(&str, &str)] = &[
    ("caminho", "o caminho de subida que a sonda desta ponta mediu"),
    ("link", "o enlace desta ponta"),
    ("link_state", "o estado do enlace desta ponta"),
    (
        "telemetry",
        "o rtt, o jitter e o áudio medidos aqui; a perda que o servidor contou vai junto, lida do `Room` pela `FotoDaSala`",
    ),
    ("muted", "o microfone desta ponta"),
    ("total_isolation", "a saída desta ponta"),
    ("speaking", "o portão desta ponta"),
    ("voice_mode", "o modo desta ponta"),
    ("audio_available", "o áudio desta ponta"),
    ("abertura_do_audio", "o áudio desta ponta"),
    ("capture", "o microfone desta ponta"),
    ("playback", "a saída desta ponta"),
    ("aparelho", "os aparelhos desta ponta"),
    ("trocas_de_aparelho", "os aparelhos desta ponta"),
];

/// As revisões que quem dobra conta, para a casca saber quando rebuscar uma
/// imagem ou a conversa. Saem das mensagens, mas como contadores do `fold` e
/// não do `Room`: o navegador as conta na ponte da sessão (Tarefa 2.6), pelas
/// mesmas mensagens.
const CONTADOS_POR_QUEM_DOBRA: &[(&str, &str)] = &[
    ("icon_revision", "sobe no `ServerIconChanged`"),
    ("person_icons_revision", "sobe no `PersonIconChanged`"),
    ("messages_revision", "sobe quando a conversa muda"),
];

/// **Cada campo do `Snapshot` tem um lado**, e um campo novo reprova até
/// alguém decidir qual: o que sai do `Room` entra no `RetratoDaSala` (e ganha
/// um passo no teste acima); o que cada ponta mede, ou conta ao dobrar, entra
/// numa das listas de cima, com o porquê. Pedido da orquestradora ao fechar a
/// fase 2 da 2.2: «os campos novos do `Snapshot` no lado certo».
#[test]
fn cada_campo_do_snapshot_tem_um_lado() {
    let desktop = serde_json::to_value(&seele_ffi::sem_rede::retratos(&sessao(), APELIDO, &[])[0])
        .expect("o retrato do desktop em JSON");
    let da_sala = serde_json::to_value(Sessao::adotar(&sessao(), APELIDO).retrato())
        .expect("o retrato da sala em JSON");
    let do_snapshot: Vec<&String> = desktop.as_object().expect("um objeto").keys().collect();
    let da_sala: Vec<&String> = da_sala.as_object().expect("um objeto").keys().collect();
    let medidos: Vec<&str> = MEDIDOS_POR_CADA_PONTA
        .iter()
        .map(|(campo, _)| *campo)
        .collect();
    let contados: Vec<&str> = CONTADOS_POR_QUEM_DOBRA
        .iter()
        .map(|(campo, _)| *campo)
        .collect();

    for campo in &do_snapshot {
        let lados = [
            da_sala.iter().any(|c| c == campo),
            medidos.contains(&campo.as_str()),
            contados.contains(&campo.as_str()),
        ];
        assert_eq!(
            lados.iter().filter(|lado| **lado).count(),
            1,
            "o campo `{campo}` do `Snapshot` não tem um lado só (da sala, medido, contado: \
             {lados:?}). O que sai do `Room` entra no `RetratoDaSala`, com um passo no teste \
             dos dois produtores; o que cada ponta mede ou conta entra numa lista deste \
             arquivo, com o porquê"
        );
    }
    for campo in medidos.iter().chain(&contados) {
        assert!(
            do_snapshot.iter().any(|c| c == campo),
            "`{campo}` está numa lista deste arquivo e não é mais campo do `Snapshot`: tire-o"
        );
    }
    assert_eq!(
        do_snapshot.len(),
        da_sala.len() + medidos.len() + contados.len(),
        "a conta dos lados não fecha"
    );
}
