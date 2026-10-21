//! Os dois produtores da **conversa** e do **FORA DO AR**: o desktop
//! (`messages_of` e `membros_fora_do_ar` da FFI) e o navegador
//! (`seele_web::sessao::conversa_de` e `fora_do_ar_de`). Tarefa 2.6 da F4.
//!
//! A conversa não está no retrato: a casca a pede pelo `messages` quando o
//! `messages_revision` sobe, e desenha cada linha pelo `estado`. O navegador
//! monta a lista pelo mesmo `Room`, e as duas têm de ser a mesma, campo a
//! campo: as confirmadas, o anexo vencido, o eco da própria, e as pendentes em
//! cada estado, com cada motivo de recusa.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use seele_core::state::Room;
use seele_proto::control::{
    AttachmentInfo, AttachmentState, ChannelInfo, Membro, MessageRefusal, Permission,
    PersonProfile, ServerMessage,
};
use seele_proto::ids::{
    AttachmentId, ChannelId, ClientMessageId, MessageId, PersonId, SessionId, Ssrc,
};

const EU: PersonId = PersonId(7);

fn sessao() -> seele_core::SessionInfo {
    seele_core::SessionInfo {
        id: SessionId(41),
        person: EU,
        ssrc: Ssrc(700),
        server: "Casa".into(),
        voice_rooms: Vec::new(),
        channels: vec![
            ChannelInfo {
                id: ChannelId(1),
                name: "geral".into(),
            },
            ChannelInfo {
                id: ChannelId(2),
                name: "avisos".into(),
            },
        ],
        permissions: vec![Permission::Speak],
    }
}

fn recebida(
    id: u64,
    channel: u32,
    author: u64,
    corpo: &str,
    eco: Option<u64>,
    anexo: Option<AttachmentState>,
) -> ServerMessage {
    ServerMessage::MessageReceived {
        channel: ChannelId(channel),
        id: MessageId(id),
        author: PersonId(author),
        at_seconds: 1_790_000_000 + i64::try_from(id).unwrap(),
        author_nickname: if author == EU.0 { "marcela" } else { "ana" }.into(),
        body: corpo.into(),
        replies_to: None,
        client_message_id: eco.map(ClientMessageId),
        attachment: anexo.map(|state| AttachmentInfo {
            id: AttachmentId(900 + id),
            file_name: format!("foto-{id}.png"),
            declared_type: "image/png".into(),
            byte_size: 1234,
            state,
        }),
    }
}

/// As duas listas de um `Room`, comparadas, e o tamanho delas.
fn conferir(room: &Room, passo: &str) -> usize {
    let desktop = serde_json::to_value(seele_ffi::sem_rede::conversa(room)).unwrap();
    let navegador = serde_json::Value::Array(seele_web::sessao::conversa_de(room));
    assert_eq!(navegador, desktop, "a conversa depois de {passo}");
    let desktop = serde_json::to_value(seele_ffi::sem_rede::fora_do_ar(room)).unwrap();
    let navegador = serde_json::Value::Array(seele_web::sessao::fora_do_ar_de(room));
    assert_eq!(navegador, desktop, "o FORA DO AR depois de {passo}");
    seele_web::sessao::conversa_de(room).len()
}

#[test]
fn a_conversa_e_o_fora_do_ar_sao_os_mesmos_nos_dois_produtores() {
    let mut room = Room::new();
    room.adopt(&sessao(), "marcela");
    assert_eq!(conferir(&room, "a entrada, sem canal aberto"), 0);

    room.apply(&ServerMessage::Membros {
        membros: vec![
            Membro {
                id: PersonId(8),
                nickname: "ana".into(),
            },
            Membro {
                id: PersonId(11),
                nickname: "Caio".into(),
            },
            Membro {
                id: PersonId(12),
                nickname: "bia".into(),
            },
        ],
        fim: true,
    });
    room.apply(&ServerMessage::PersonPresent {
        profile: PersonProfile {
            id: PersonId(8),
            nickname: "ana".into(),
            roles: Vec::new(),
            material_e2ee: None,
        },
        ssrc: Ssrc(800),
    });
    conferir(&room, "os membros");

    room.open_channel(ChannelId(1));
    for mensagem in [
        recebida(51, 1, 8, "oi", None, None),
        recebida(52, 1, 8, "a foto", None, Some(AttachmentState::Available)),
        recebida(53, 1, 8, "a velha", None, Some(AttachmentState::Expired)),
        recebida(54, 2, 8, "noutro canal", None, None),
    ] {
        room.apply(&mensagem);
    }
    assert_eq!(conferir(&room, "as confirmadas"), 3);

    // As pendentes desta ponta, uma em cada estado e com cada recusa.
    let motivos = [
        MessageRefusal::PermissionDenied,
        MessageRefusal::NoSuchChannel,
        MessageRefusal::TooLong { limit: 4096 },
        MessageRefusal::StorageFailed,
        MessageRefusal::Apagada,
    ];
    for chave in 1..=8_u64 {
        room.anotar_envio(
            ChannelId(1),
            ClientMessageId((7 << 32) | chave),
            format!("pendente {chave}"),
            1_790_000_100,
        );
    }
    for (chave, motivo) in (1..).zip(motivos) {
        assert!(room.recusar_envio(ClientMessageId((7 << 32) | chave), motivo));
    }
    assert_eq!(conferir(&room, "as pendentes recusadas"), 11);
    assert!(room.envios_sem_resposta());
    conferir(&room, "o enlace que caiu");

    // O eco da própria troca a pendente pela gravada, com a chave junto.
    room.apply(&recebida(
        55,
        1,
        EU.0,
        "pendente 8",
        Some((7 << 32) | 8),
        None,
    ));
    assert_eq!(conferir(&room, "o eco da própria"), 11);

    // E a conversa segue o canal aberto.
    room.open_channel(ChannelId(2));
    assert_eq!(conferir(&room, "trocar de canal"), 1);
}
