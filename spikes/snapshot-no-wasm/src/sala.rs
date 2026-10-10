//! Uma sala cheia, montada pelas mensagens do servidor, como a sessão a monta.

use seele_proto::control::{ChannelInfo, PersonProfile, PersonState, Presence, ServerMessage, VoiceRoomInfo};
use seele_proto::ids::{ChannelId, PersonId, SessionId, Ssrc, VoiceRoomId};

use crate::client::SessionInfo;
use crate::state::Room;

/// O teto de pessoas de uma sala de voz (`MAX_VOICE_ROOM_LIMIT`).
pub const LIMITE_DA_SALA: u16 = seele_proto::control::MAX_VOICE_ROOM_LIMIT;

/// `pessoas` no servidor, enchendo as `salas` em ordem até o teto de cada uma,
/// com `canais` canais de texto. Esta pessoa entra pelo `adopt`, como no
/// desktop, e senta na primeira sala.
pub fn sala_cheia(pessoas: u64, salas: u32, canais: u32) -> Room {
    let info = SessionInfo {
        id: SessionId(1),
        person: PersonId(1),
        ssrc: Ssrc(1),
        server: "SERVIDOR DE MEDIDA DO RETRATO".to_owned(),
        voice_rooms: (1..=salas)
            .map(|i| VoiceRoomInfo {
                id: VoiceRoomId(i),
                name: format!("SALA DE VOZ NÚMERO {i}"),
                limit: LIMITE_DA_SALA,
                password_required: i % 3 == 0,
                channel: None,
            })
            .collect(),
        channels: (1..=canais)
            .map(|i| ChannelInfo { id: ChannelId(i), name: format!("canal-de-texto-{i}") })
            .collect(),
        permissions: Vec::new(),
    };
    let mut room = Room::new();
    room.adopt(&info, "esta pessoa aqui");
    room.enter_voice_room(VoiceRoomId(1));
    for pessoa in 1..=pessoas {
        let sala = 1 + u32::try_from((pessoa - 1) / u64::from(LIMITE_DA_SALA)).unwrap_or(0).min(salas - 1);
        let entrou = ServerMessage::PersonJoined {
            voice_room: VoiceRoomId(sala),
            profile: PersonProfile {
                id: PersonId(pessoa),
                nickname: if pessoa == 1 { "esta pessoa aqui".to_owned() } else { format!("pessoa de número {pessoa}") },
                roles: Vec::new(),
                material_e2ee: None,
            },
            ssrc: Ssrc(u32::try_from(pessoa).unwrap_or(u32::MAX)),
        };
        room.apply(&entrou);
        room.apply(&ServerMessage::PersonState(PersonState {
            person: PersonId(pessoa),
            muted: pessoa % 7 == 0,
            total_isolation: pessoa % 11 == 0,
            speaking: pessoa % 5 == 0,
            presence: Presence::Available,
            signal: u8::try_from(40 + pessoa % 60).unwrap_or(100),
        }));
    }
    room
}
