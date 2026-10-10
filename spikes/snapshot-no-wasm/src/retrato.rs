//! O retrato da sessão no formato do `Snapshot` do desktop
//! (`seele-ffi/src/types.rs`), com a projeção do `seele-ffi/src/lib.rs`
//! (`presentes_de`, `voice_rooms_of`, `lines_of`, `transmissoes_de`).
//!
//! **Os campos da sessão saem do `Room`**, como no desktop. Os do áudio, do
//! enlace e do caminho, que o navegador mede por conta própria, saem aqui com o
//! que o desktop diz sem voz (`AudioState::sem_voz`): o tamanho do JSON é o
//! mesmo, e é o que se mede.

use serde::Serialize;
use seele_proto::signal::SignalBand;

use crate::state::Room;

#[derive(Serialize)]
pub struct Pessoa {
    pub id: u64,
    pub nickname: String,
    pub speaking: bool,
    pub muted: bool,
    pub total_isolation: bool,
    pub signal: u8,
    pub sync_band: &'static str,
    pub is_self: bool,
}

#[derive(Serialize)]
pub struct Sincronia {
    pub ratio: u8,
    pub band: &'static str,
    pub people: u32,
}

#[derive(Serialize)]
pub struct Sala {
    pub id: u32,
    pub name: String,
    pub limit: u16,
    pub password_required: bool,
    pub occupied_by_us: bool,
    pub channel: Option<u32>,
    pub people: Vec<Pessoa>,
    pub sync: Option<Sincronia>,
}

#[derive(Serialize)]
pub struct Canal {
    pub id: u32,
    pub name: String,
    pub open: bool,
}

#[derive(Serialize)]
pub struct Telemetria {
    pub rtt_ms: f32,
    pub jitter_ms: f32,
    pub loss_fraction: f32,
    pub bitrate_bps: u32,
    pub signal: u8,
    pub sync_band: &'static str,
    pub input_level: f32,
    pub local_fault: bool,
    pub frames_refused: u64,
    pub microfone_entrega_silencio_digital: bool,
}

#[derive(Serialize)]
pub struct Transmissao {
    pub tela: u32,
    pub de: u64,
    pub e_minha: bool,
    pub assistida: bool,
    pub espectadores: u32,
}

#[derive(Serialize)]
pub struct Retrato {
    pub caminho: &'static str,
    pub link: &'static str,
    pub link_state: &'static str,
    pub server: String,
    pub icon_revision: u64,
    pub person_icons_revision: u64,
    pub me: Option<u64>,
    pub nickname: String,
    pub voice_rooms: Vec<Sala>,
    pub presentes: Vec<Pessoa>,
    pub channels: Vec<Canal>,
    pub messages_revision: u64,
    pub telemetry: Telemetria,
    pub notice: Option<String>,
    pub muted: bool,
    pub total_isolation: bool,
    pub speaking: bool,
    pub voice_mode: &'static str,
    pub audio_available: bool,
    pub abertura_do_audio: &'static str,
    pub capture: Option<String>,
    pub playback: Option<String>,
    pub aparelho: &'static str,
    pub trocas_de_aparelho: u64,
    pub may_create_rooms: bool,
    pub may_kick: bool,
    pub may_ban: bool,
    pub may_move: bool,
    pub may_remove_messages: bool,
    pub may_customise_server: bool,
    pub may_delete_rooms: bool,
    pub tela: Option<u32>,
    pub minha_transmissao: Option<u32>,
    pub transmissoes: Vec<Transmissao>,
    pub ended: Option<String>,
}

fn faixa(signal: u8) -> &'static str {
    match SignalBand::of(signal) {
        SignalBand::Nominal => "Nominal",
        SignalBand::Degraded => "Degraded",
        SignalBand::Critical => "Critical",
    }
}

fn pessoa(room: &Room, p: &crate::state::Person) -> Pessoa {
    Pessoa {
        id: p.id.0,
        nickname: p.nickname.clone(),
        speaking: p.speaking,
        muted: p.muted,
        total_isolation: p.total_isolation,
        signal: p.signal,
        sync_band: faixa(p.signal),
        is_self: room.me == Some(p.id),
    }
}

/// O retrato, como o `Connection::snapshot` do desktop o monta.
pub fn retratar(room: &Room) -> Retrato {
    let presentes = room
        .presentes
        .iter()
        .chain(room.me.iter().filter(|eu| !room.presentes.contains(eu)))
        .filter_map(|id| room.people.get(id))
        .map(|p| pessoa(room, p))
        .collect();
    let voice_rooms = room
        .voice_rooms
        .iter()
        .map(|sala| Sala {
            id: sala.id.0,
            name: sala.name.clone(),
            limit: sala.limit,
            password_required: sala.password_required,
            occupied_by_us: room.current_voice_room == Some(sala.id),
            channel: sala.channel.map(|c| c.0),
            people: room.roster(sala.id).map(|p| pessoa(room, p)).collect(),
            sync: room.voice_room_sync(sala.id).map(|s| Sincronia {
                ratio: s.ratio,
                band: faixa(s.ratio),
                people: u32::try_from(s.people).unwrap_or(u32::MAX),
            }),
        })
        .collect();
    let channels = room
        .channels
        .iter()
        .map(|c| Canal { id: c.id.0, name: c.name.clone(), open: room.current_channel == Some(c.id) })
        .collect();
    let mut transmissoes: Vec<Transmissao> = room
        .telas
        .values()
        .filter(|t| Some(t.voice_room) == room.current_voice_room)
        .map(|t| Transmissao {
            tela: t.screen.0,
            de: t.person.0,
            e_minha: room.me == Some(t.person),
            assistida: false,
            espectadores: t.espectadores,
        })
        .collect();
    transmissoes.sort_by_key(|t| t.tela);
    Retrato {
        caminho: "Direto",
        link: "Online",
        link_state: "Verified",
        server: room.server.clone(),
        icon_revision: 0,
        person_icons_revision: 0,
        me: room.me.map(|m| m.0),
        nickname: room.me.and_then(|m| room.people.get(&m)).map_or_else(String::new, |p| p.nickname.clone()),
        voice_rooms,
        presentes,
        channels,
        messages_revision: 0,
        telemetry: Telemetria {
            rtt_ms: 0.0,
            jitter_ms: 0.0,
            loss_fraction: room.telemetry.as_ref().map_or(0.0, |t| t.loss_fraction),
            bitrate_bps: 0,
            signal: 0,
            sync_band: "Critical",
            input_level: 0.0,
            local_fault: false,
            frames_refused: 0,
            microfone_entrega_silencio_digital: false,
        },
        notice: room.notice.as_ref().map(|n| format!("{:?}", n.reason)),
        muted: false,
        total_isolation: false,
        speaking: false,
        voice_mode: "PushToTalk",
        audio_available: false,
        abertura_do_audio: "sem_sala",
        capture: None,
        playback: None,
        aparelho: "fechado",
        trocas_de_aparelho: 0,
        may_create_rooms: false,
        may_kick: false,
        may_ban: false,
        may_move: false,
        may_remove_messages: false,
        may_customise_server: false,
        may_delete_rooms: false,
        tela: None,
        minha_transmissao: None,
        transmissoes,
        ended: room.ended.map(|e| format!("{:?}", e.reason)),
    }
}

/// O retrato em JSON, que é o que atravessa para a casca.
pub fn em_json(room: &Room) -> String {
    serde_json::to_string(&retratar(room)).unwrap_or_default()
}
