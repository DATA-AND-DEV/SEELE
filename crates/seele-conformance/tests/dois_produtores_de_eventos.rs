//! Os dois produtores de **eventos**, com as mesmas mensagens: o desktop
//! (`seele-ffi`, pelo `fold` da sessão viva) e o navegador (`seele-web`, pelo
//! `Sessao::dobrar_com_eventos`). Tarefa 2.6 da F4.
//!
//! O retrato igual (`dois_produtores_de_snapshot.rs`) não basta: a casca só
//! relê o retrato **quando um evento chega**. Um navegador que dobrasse certo e
//! não avisasse desenharia a sala da mensagem anterior — o produto sabendo e
//! não contando. Por isso a comparação é a dos eventos, na ordem em que saem,
//! e a dos quatro números que quem dobra conta (`icon_revision`,
//! `person_icons_revision`, `messages_revision`, `fora_do_ar_revision`), lidos
//! logo depois de cada mensagem.
//!
//! A sequência passa por cada evento que o `fold` manda, e o teste confere que
//! passou: dois produtores mudos seriam iguais.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::collections::BTreeSet;

use seele_proto::control::{
    AlertReason, AlertSeverity, AttachmentInfo, AttachmentRefusal, AttachmentState, ChannelInfo,
    DisconnectReason, Membro, Permission, PersonProfile, ServerMessage, Telemetry, TemaDoServidor,
    VoiceRoomInfo,
};
use seele_proto::ids::{
    AttachmentId, ChannelId, ClientMessageId, MessageId, PersonId, ScreenId, SessionId, Ssrc,
    VoiceRoomId,
};
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
        voice_rooms: vec![
            VoiceRoomInfo {
                id: SALA,
                name: "SALA-02".into(),
                limit: 0,
                password_required: false,
                channel: Some(ChannelId(1)),
            },
            VoiceRoomInfo {
                id: VoiceRoomId(3),
                name: "SALA-03".into(),
                limit: 0,
                password_required: false,
                channel: None,
            },
        ],
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
        ServerMessage::Membros {
            membros: vec![
                Membro {
                    id: PersonId(8),
                    nickname: "ana".into(),
                },
                Membro {
                    id: PersonId(11),
                    nickname: "caio".into(),
                },
            ],
            fim: true,
        },
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
        ServerMessage::MessageReceived {
            channel: ChannelId(1),
            id: MessageId(51),
            author: PersonId(8),
            at_seconds: 1_790_000_000,
            author_nickname: "ana".into(),
            body: "oi".into(),
            replies_to: None,
            client_message_id: None,
            attachment: Some(AttachmentInfo {
                id: AttachmentId(951),
                file_name: "foto.png".into(),
                declared_type: "image/png".into(),
                byte_size: 1234,
                state: AttachmentState::Available,
            }),
        },
        // A mesma mensagem outra vez, como a de um histórico que se sobrepõe:
        // nenhum evento nos dois.
        ServerMessage::MessageReceived {
            channel: ChannelId(1),
            id: MessageId(51),
            author: PersonId(8),
            at_seconds: 1_790_000_000,
            author_nickname: "ana".into(),
            body: "oi".into(),
            replies_to: None,
            client_message_id: None,
            attachment: Some(AttachmentInfo {
                id: AttachmentId(951),
                file_name: "foto.png".into(),
                declared_type: "image/png".into(),
                byte_size: 1234,
                state: AttachmentState::Available,
            }),
        },
        ServerMessage::ChannelCreated {
            channel: ChannelInfo {
                id: ChannelId(2),
                name: "avisos".into(),
            },
        },
        ServerMessage::ServerRenamed {
            name: "Casa Nova".into(),
        },
        ServerMessage::ServerIconChanged {
            icon: Some(vec![0x89, 0x50, 0x4e, 0x47]),
        },
        ServerMessage::PersonIconChanged {
            person: PersonId(8),
            icon: Some(vec![0x89, 0x50, 0x4e, 0x47]),
        },
        ServerMessage::Telemetry(Telemetry {
            // `Option` desde a linha 2: `None` é «não medido», e nunca 0.0.
            rtt_ms: Some(31.0),
            jitter_ms: Some(2.5),
            loss_fraction: Some(0.0),
            subsystems: Vec::new(),
        }),
        ServerMessage::UplinkLoss { fraction: 0.04 },
        // As recusas de transferência: o laço da conexão as drena depois da
        // mensagem, e elas saem como `TransferChanged`. O anexo vencido da
        // mensagem de antes também muda a conversa.
        ServerMessage::AttachmentRefused {
            client_message_id: ClientMessageId(77),
            reason: AttachmentRefusal::TooLarge { limit: 1024 },
        },
        ServerMessage::AttachmentUnavailable {
            attachment: AttachmentId(951),
            reason: AttachmentRefusal::Expired,
        },
        ServerMessage::VolumeRecusado {
            token: "t-1".into(),
            motivo: seele_proto::volume::VolumeRefusal::TipoRecusado,
        },
        ServerMessage::ModReply {
            request: 3,
            part: 0,
            total: 1,
            payload: r#"{"ok":true}"#.into(),
        },
        ServerMessage::ModAviso {
            mod_id: "seele/enquete".into(),
            canal: Some(ChannelId(1)),
            carga: r#"{"votos":2}"#.into(),
        },
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
        // Com uma transmissão na sala, mexer em quem está nela é mexer na tela.
        ServerMessage::PersonRenamed {
            person: PersonId(9),
            nickname: "bianca".into(),
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
        ServerMessage::PersonGone {
            person: PersonId(8),
        },
        // As duas que mexem em duas coisas de uma vez: a ordem dos eventos é a
        // do `fold` (a lista de gente, a conversa, os canais).
        ServerMessage::VoiceRoomDeleted {
            voice_room: VoiceRoomId(3),
        },
        ServerMessage::ChannelDeleted {
            channel: ChannelId(1),
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

/// O nome do evento, para a cobertura: a variante, sem os campos.
fn nome(evento: &serde_json::Value) -> String {
    match evento {
        serde_json::Value::String(nome) => nome.clone(),
        serde_json::Value::Object(campos) => campos.keys().next().cloned().unwrap_or_default(),
        outro => format!("{outro}"),
    }
}

/// Os eventos que a sequência tem de fazer sair. Os outros do `Event` (a etapa
/// da chegada, a transferência, a tela que abre ou não se lê) não vêm do
/// `fold`: saem de quem liga e de quem recebe os fluxos.
const DO_FOLD: &[&str] = &[
    "ModReply",
    "ModAviso",
    "RosterChanged",
    "MessagesChanged",
    "ChannelsChanged",
    "ServerChanged",
    "TelemetryChanged",
    "NoticeRaised",
    "ScreenChanged",
    "TransferChanged",
    "Ended",
];

#[test]
fn os_dois_produtores_dao_os_mesmos_eventos_depois_de_cada_mensagem() {
    let mensagens = mensagens();
    let do_desktop = seele_ffi::sem_rede::eventos(&sessao(), APELIDO, &mensagens);
    assert_eq!(do_desktop.len(), mensagens.len());

    let mut navegador = Sessao::adotar(&sessao(), APELIDO);
    let mut vistos = BTreeSet::new();
    for (mensagem, desktop) in mensagens.iter().zip(&do_desktop) {
        let eventos_do_navegador = navegador.dobrar_com_eventos(mensagem);
        let eventos_do_desktop: Vec<serde_json::Value> = desktop
            .eventos
            .iter()
            .map(|evento| serde_json::to_value(evento).expect("o evento do desktop em JSON"))
            .collect();
        assert_eq!(
            eventos_do_navegador, eventos_do_desktop,
            "depois de {mensagem:?}, os eventos do navegador não são os do desktop"
        );
        let contados = navegador.contados();
        assert_eq!(
            (
                contados.icon_revision,
                contados.person_icons_revision,
                contados.messages_revision,
                contados.fora_do_ar_revision,
            ),
            (
                desktop.icon_revision,
                desktop.person_icons_revision,
                desktop.messages_revision,
                desktop.fora_do_ar_revision,
            ),
            "depois de {mensagem:?}, os números que quem dobra conta não são os do desktop"
        );
        vistos.extend(eventos_do_navegador.iter().map(nome));
    }

    let faltam: Vec<_> = DO_FOLD
        .iter()
        .filter(|evento| !vistos.contains(**evento))
        .collect();
    assert!(
        faltam.is_empty(),
        "a sequência não fez sair {faltam:?}: dois produtores mudos passariam por iguais"
    );
    let fim = navegador.contados();
    assert!(
        fim.icon_revision == 1
            && fim.person_icons_revision == 1
            && fim.messages_revision == 3
            && fim.fora_do_ar_revision >= 2,
        "algum número não subiu como a sequência pede: {fim:?}"
    );
}

/// O `Contados` em JSON tem os nomes de campo do `Snapshot` da FFI: é por eles
/// que a ponte do navegador monta o retrato que a casca lê.
#[test]
fn os_contados_tem_os_nomes_do_snapshot() {
    let retrato = seele_ffi::sem_rede::retratos(&sessao(), APELIDO, &[])
        .pop()
        .expect("o retrato da entrada");
    let retrato = serde_json::to_value(retrato).expect("o retrato em JSON");
    let contados = Sessao::adotar(&sessao(), APELIDO).contados().em_json();
    let campos = contados.as_object().expect("um objeto");
    assert_eq!(campos.len(), 4);
    for (campo, valor) in campos {
        assert_eq!(
            retrato.get(campo),
            Some(valor),
            "o `{campo}` do navegador não é um campo do retrato do desktop com o mesmo valor"
        );
    }
}
