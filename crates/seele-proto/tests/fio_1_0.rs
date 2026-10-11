//! Os bytes-ouro da linha 1: cada mensagem das duas listas de cima, cada
//! cabeçalho de fluxo e de datagrama, o enquadramento e a recusa, como o fio os
//! carrega.
//!
//! # Por que eles existem
//!
//! O ADR 0058 escreve que **1.x nunca quebra o fio**: variante nova só no fim,
//! campo novo nunca, nada que se remova ou reordene. Um guarda que só faz ida e
//! volta não pega nada disso — os dois lados são o mesmo codec, e qualquer
//! mudança volta igual. O que pega é comparar com os bytes de antes.
//!
//! # Rascunho e congelado
//!
//! O arquivo `tests/fio_1_0.txt` começa por `# estado: rascunho` ou
//! `# estado: congelado`, e cada linha seguinte é `nome<TAB>hex`.
//!
//! - **Rascunho** (até a 0.16 sair): o vocabulário da 1.0 ainda está entrando,
//!   e `SEELE_REGERAR_FIO=1` reescreve o arquivo inteiro. Quem regera lê o
//!   diff: é ele que diz o que mudou no fio.
//! - **Congelado** (no commit que publica a 0.16): a regeração só **acrescenta**
//!   nomes novos, e reprova se um nome que já existe mudou de bytes. É a regra
//!   do ADR 0058 escrita como teste.
//!
//! ```text
//! SEELE_REGERAR_FIO=1 cargo test -p seele-proto --test fio_1_0
//! ```

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::collections::BTreeSet;
use std::path::PathBuf;

use seele_proto::control::{
    encode, recusa_por_versao, AlertReason, AlertSeverity, AttachmentInfo, AttachmentRefusal,
    AttachmentState, CausaDaRecusa, ChannelInfo, ClientMessage, ConsentimentoDePar,
    DisconnectReason, ListaDeCima, Membro, MessageRefusal, MotivoDeFalhaDePar, Permission,
    PersonProfile, PersonState, Presence, Role, ServerMessage, Subsystem, SubsystemHealth,
    Telemetry, VoiceRoomInfo,
};
use seele_proto::ids::{
    AttachmentId, ChannelId, ClientMessageId, MessageId, PersonId, RoleId, ScreenId, SessionId,
    Ssrc, VoiceRoomId,
};
use seele_proto::mods::ModAnunciado;
use seele_proto::screen::{
    cabecalho_de_quadro, LeitorDeTela, LidoDaTela, ScreenCodec, ScreenHeader, ScreenSource,
    TipoDeQuadro, SCREEN_HEADER_VERSION,
};
use seele_proto::stream::StreamType;
use seele_proto::volume::{PedidoDeImagem, RespostaDeImagem, VolumeHeader, VolumeRefusal};
use seele_proto::{MediaHeader, FAIXA};

/// Onde os bytes-ouro moram, a partir da raiz do crate.
const ARQUIVO: &str = "tests/fio_1_0.txt";

/// Uma impressão digital no formato de verdade: 64 hexadecimais.
const IMPRESSAO: &str = "3cbcfb0212da738f89c156de86eb280adee30fd6b907523b898fedcb2b1de5b9";

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn perfil() -> PersonProfile {
    PersonProfile {
        id: PersonId(42),
        nickname: "maya".into(),
        roles: vec![RoleId(1)],
        material_e2ee: None,
    }
}

/// Um anúncio de um MOD só, com a identidade do conjunto calculada como o
/// servidor a calcula: `check_anuncio` recusa um conjunto que não bate.
fn mods_exigidos() -> ServerMessage {
    let mut mods = vec![ModAnunciado {
        id: "autor/nome".into(),
        version: "1.0.0".into(),
        hash: "ab".repeat(32),
        repo: "https://github.com/autor/nome".into(),
        reach: vec![],
        no_servidor: false,
        api: 6,
    }];
    let conjunto = seele_proto::mods::hex(&seele_proto::mods::identidade_do_conjunto(&mut mods));
    ServerMessage::ModsExigidos { mods, conjunto }
}

/// Um exemplar de cada variante de `ClientMessage`, **na ordem do código**, com
/// campos mínimos e válidos.
fn clientes() -> Vec<(&'static str, ClientMessage)> {
    vec![
        (
            "Hello",
            ClientMessage::Hello {
                join_secret: Some("7K4MNPQRSTVWXYZ23456".into()),
                faixa: FAIXA,
                client: "seele 0.16.0".into(),
                nickname: "ritsuko".into(),
                public_key: vec![0x11; 32],
                material_e2ee: None,
                apis_de_mod: vec![6],
            },
        ),
        (
            "Response",
            ClientMessage::Response {
                proof: vec![0x22; 64],
            },
        ),
        (
            "EnterVoiceRoom",
            ClientMessage::EnterVoiceRoom {
                voice_room: VoiceRoomId(1),
                password: None,
            },
        ),
        ("LeaveVoiceRoom", ClientMessage::LeaveVoiceRoom),
        (
            "JoinChannel",
            ClientMessage::JoinChannel {
                channel: ChannelId(1),
            },
        ),
        (
            "SendMessage",
            ClientMessage::SendMessage {
                channel: ChannelId(1),
                body: "olá, linha".into(),
                replies_to: Some(MessageId(3)),
                client_message_id: ClientMessageId(4),
            },
        ),
        (
            "FetchHistory",
            ClientMessage::FetchHistory {
                channel: ChannelId(1),
                cursor: None,
                limit: 50,
            },
        ),
        ("SetMuted", ClientMessage::SetMuted(true)),
        ("SetTotalIsolation", ClientMessage::SetTotalIsolation(false)),
        ("SetPresence", ClientMessage::SetPresence(Presence::Away)),
        ("Ping", ClientMessage::Ping { timestamp: 1_234 }),
        (
            "CreateVoiceRoom",
            ClientMessage::CreateVoiceRoom {
                name: "SALA-02".into(),
                limit: 8,
                channel: None,
            },
        ),
        (
            "CreateChannel",
            ClientMessage::CreateChannel {
                name: "geral".into(),
            },
        ),
        (
            "RenameVoiceRoom",
            ClientMessage::RenameVoiceRoom {
                voice_room: VoiceRoomId(1),
                name: "SALA-03".into(),
            },
        ),
        (
            "RenameChannel",
            ClientMessage::RenameChannel {
                channel: ChannelId(1),
                name: "avisos".into(),
            },
        ),
        (
            "KickPerson",
            ClientMessage::KickPerson {
                person: PersonId(7),
            },
        ),
        (
            "BanPerson",
            ClientMessage::BanPerson {
                person: PersonId(7),
                reason: None,
                expires_at: Some(1_800_000_000),
            },
        ),
        (
            "RemoveMessage",
            ClientMessage::RemoveMessage {
                message: MessageId(3),
            },
        ),
        (
            "MovePerson",
            ClientMessage::MovePerson {
                person: PersonId(7),
                voice_room: VoiceRoomId(2),
            },
        ),
        (
            "DeleteVoiceRoom",
            ClientMessage::DeleteVoiceRoom {
                voice_room: VoiceRoomId(2),
            },
        ),
        (
            "DeleteChannel",
            ClientMessage::DeleteChannel {
                channel: ChannelId(2),
            },
        ),
        (
            "WeighChannel",
            ClientMessage::WeighChannel {
                channel: ChannelId(2),
            },
        ),
        (
            "FetchAttachment",
            ClientMessage::FetchAttachment {
                attachment: AttachmentId(9),
            },
        ),
        (
            "RenameServer",
            ClientMessage::RenameServer {
                name: "Casa".into(),
            },
        ),
        ("SetServerIcon", ClientMessage::SetServerIcon { icon: None }),
        ("StartScreenShare", ClientMessage::StartScreenShare),
        ("StopScreenShare", ClientMessage::StopScreenShare),
        (
            "RequestKeyFrame",
            ClientMessage::RequestKeyFrame {
                screen: ScreenId(5),
            },
        ),
        ("SetPersonIcon", ClientMessage::SetPersonIcon { icon: None }),
        (
            "SetNickname",
            ClientMessage::SetNickname {
                name: "maya".into(),
            },
        ),
        (
            "WatchScreen",
            ClientMessage::WatchScreen {
                screen: ScreenId(5),
            },
        ),
        (
            "UnwatchScreen",
            ClientMessage::UnwatchScreen {
                screen: ScreenId(5),
            },
        ),
        (
            "EmprestarSubida",
            ClientMessage::EmprestarSubida {
                consentimento: ConsentimentoDePar::de_ninguem(),
                impressao: IMPRESSAO.into(),
                locais: vec![],
            },
        ),
        (
            "ParFalhou",
            ClientMessage::ParFalhou {
                screen: ScreenId(5),
                motivo: MotivoDeFalhaDePar::NaoAlcancou,
            },
        ),
        (
            "AceitarMods",
            ClientMessage::AceitarMods {
                conjunto: IMPRESSAO.into(),
            },
        ),
        ("RecusarMods", ClientMessage::RecusarMods),
        (
            "ModRequest",
            ClientMessage::ModRequest {
                request: 1,
                id: "autor/nome".into(),
                channel: ChannelId(1),
                payload: "{}".into(),
            },
        ),
    ]
}

/// Um exemplar de cada variante de `ServerMessage`, **na ordem do código**.
#[allow(clippy::too_many_lines, reason = "uma linha por variante, e são 41")]
fn servidores() -> Vec<(&'static str, ServerMessage)> {
    vec![
        (
            "Challenge",
            ServerMessage::Challenge {
                nonce: vec![0x33; 32],
            },
        ),
        (
            "Session",
            ServerMessage::Session {
                id: SessionId(7),
                person: PersonId(42),
                ssrc: Ssrc(0xABCD),
                server: "Casa".into(),
                voice_rooms: vec![VoiceRoomInfo {
                    id: VoiceRoomId(1),
                    name: "SALA-01 CENTRAL".into(),
                    limit: 15,
                    password_required: false,
                    channel: Some(ChannelId(1)),
                }],
                channels: vec![ChannelInfo {
                    id: ChannelId(1),
                    name: "geral".into(),
                }],
                roles: vec![Role {
                    id: RoleId(1),
                    name: "Person".into(),
                    permissions: vec![Permission::EnterVoiceRoom, Permission::Speak],
                }],
                permissions: vec![Permission::EnterVoiceRoom, Permission::Speak],
            },
        ),
        (
            "PersonJoined",
            ServerMessage::PersonJoined {
                voice_room: VoiceRoomId(1),
                profile: perfil(),
                ssrc: Ssrc(99),
            },
        ),
        (
            "PersonLeft",
            ServerMessage::PersonLeft {
                voice_room: VoiceRoomId(1),
                person: PersonId(42),
            },
        ),
        (
            "PersonState",
            ServerMessage::PersonState(PersonState {
                person: PersonId(42),
                muted: true,
                total_isolation: false,
                speaking: true,
                presence: Presence::Available,
                signal: 3,
            }),
        ),
        (
            "MessageReceived",
            ServerMessage::MessageReceived {
                channel: ChannelId(1),
                id: MessageId(3),
                author: PersonId(42),
                at_seconds: 1_790_000_000,
                author_nickname: "maya".into(),
                body: "olá".into(),
                replies_to: None,
                client_message_id: Some(ClientMessageId(4)),
                attachment: Some(AttachmentInfo {
                    id: AttachmentId(9),
                    file_name: "foto.png".into(),
                    declared_type: "image/png".into(),
                    byte_size: 1_024,
                    state: AttachmentState::Available,
                }),
            },
        ),
        (
            "MessageEdited",
            ServerMessage::MessageEdited {
                channel: ChannelId(1),
                id: MessageId(3),
                body: "olá de novo".into(),
            },
        ),
        (
            "MessageRemoved",
            ServerMessage::MessageRemoved {
                channel: ChannelId(1),
                id: MessageId(3),
            },
        ),
        (
            "Telemetry",
            ServerMessage::Telemetry(Telemetry {
                rtt_ms: 12.5,
                jitter_ms: 1.25,
                loss_fraction: 0.0,
                subsystems: vec![(Subsystem::Media, SubsystemHealth::Nominal)],
            }),
        ),
        (
            "Alert",
            ServerMessage::Alert {
                severity: AlertSeverity::Info,
                reason: AlertReason::Mentioned,
                operator_text: None,
            },
        ),
        // O motivo da 1.0 dentro de um quadro de sempre: o ordinal do pedido
        // (29, o `SetNickname`: `cliente:SetNickname` começa por `09 1d`) e a
        // causa.
        (
            "Alert.PedidoRecusado",
            ServerMessage::Alert {
                severity: AlertSeverity::Warning,
                reason: AlertReason::PedidoRecusado {
                    pedido: 29,
                    causa: CausaDaRecusa::NomeTomado,
                },
                operator_text: None,
            },
        ),
        ("Pong", ServerMessage::Pong { timestamp: 1_234 }),
        (
            "Disconnecting",
            ServerMessage::Disconnecting {
                reason: DisconnectReason::Incompatible,
            },
        ),
        (
            "VoiceRoomCreated",
            ServerMessage::VoiceRoomCreated {
                voice_room: VoiceRoomInfo {
                    id: VoiceRoomId(2),
                    name: "SALA-02".into(),
                    limit: 8,
                    password_required: true,
                    channel: None,
                },
            },
        ),
        (
            "ChannelCreated",
            ServerMessage::ChannelCreated {
                channel: ChannelInfo {
                    id: ChannelId(2),
                    name: "avisos".into(),
                },
            },
        ),
        (
            "VoiceRoomRenamed",
            ServerMessage::VoiceRoomRenamed {
                voice_room: VoiceRoomId(2),
                name: "SALA-03".into(),
            },
        ),
        (
            "ChannelRenamed",
            ServerMessage::ChannelRenamed {
                channel: ChannelId(2),
                name: "recados".into(),
            },
        ),
        (
            "MovedToVoiceRoom",
            ServerMessage::MovedToVoiceRoom {
                voice_room: VoiceRoomId(2),
            },
        ),
        (
            "VoiceRoomDeleted",
            ServerMessage::VoiceRoomDeleted {
                voice_room: VoiceRoomId(2),
            },
        ),
        (
            "ChannelDeleted",
            ServerMessage::ChannelDeleted {
                channel: ChannelId(2),
            },
        ),
        (
            "ChannelWeighed",
            ServerMessage::ChannelWeighed {
                channel: ChannelId(1),
                messages: 10,
                authors: 2,
                oldest_at_seconds: Some(1_790_000_000),
            },
        ),
        (
            "AttachmentRefused",
            ServerMessage::AttachmentRefused {
                client_message_id: ClientMessageId(4),
                reason: AttachmentRefusal::NoRoom,
            },
        ),
        (
            "AttachmentUnavailable",
            ServerMessage::AttachmentUnavailable {
                attachment: AttachmentId(9),
                reason: AttachmentRefusal::Expired,
            },
        ),
        (
            "ServerRenamed",
            ServerMessage::ServerRenamed {
                name: "Casa".into(),
            },
        ),
        (
            "ServerIconChanged",
            ServerMessage::ServerIconChanged { icon: None },
        ),
        (
            "ScreenShareStarted",
            ServerMessage::ScreenShareStarted {
                voice_room: VoiceRoomId(1),
                person: PersonId(42),
                screen: ScreenId(5),
            },
        ),
        (
            "ScreenShareStopped",
            ServerMessage::ScreenShareStopped {
                voice_room: VoiceRoomId(1),
                screen: ScreenId(5),
            },
        ),
        (
            "KeyFrameRequested",
            ServerMessage::KeyFrameRequested {
                screen: ScreenId(5),
                person: PersonId(42),
            },
        ),
        (
            "ScreenViewers",
            ServerMessage::ScreenViewers {
                tela: ScreenId(5),
                quantos: 2,
            },
        ),
        ("HostUplink", ServerMessage::HostUplink { bps: 2_000_000 }),
        (
            "PersonPresent",
            ServerMessage::PersonPresent {
                profile: perfil(),
                ssrc: Ssrc(99),
            },
        ),
        (
            "PersonGone",
            ServerMessage::PersonGone {
                person: PersonId(42),
            },
        ),
        ("UplinkLoss", ServerMessage::UplinkLoss { fraction: 0.25 }),
        (
            "PersonIconChanged",
            ServerMessage::PersonIconChanged {
                person: PersonId(42),
                icon: None,
            },
        ),
        (
            "PersonRenamed",
            ServerMessage::PersonRenamed {
                person: PersonId(42),
                nickname: "maya".into(),
            },
        ),
        (
            "SirvaTelaPara",
            ServerMessage::SirvaTelaPara {
                screen: ScreenId(5),
                enderecos: vec![],
                impressao: IMPRESSAO.into(),
            },
        ),
        (
            "AssistaTelaPor",
            ServerMessage::AssistaTelaPor {
                screen: ScreenId(5),
                enderecos: vec![],
                impressao: IMPRESSAO.into(),
            },
        ),
        ("ModsExigidos", mods_exigidos()),
        (
            "ModReply",
            ServerMessage::ModReply {
                request: 1,
                part: 0,
                total: 1,
                payload: "{}".into(),
            },
        ),
        (
            "Instancia",
            ServerMessage::Instancia {
                identidade: "0123456789abcdef0123456789abcdef".into(),
            },
        ),
        (
            "VolumeRecusado",
            ServerMessage::VolumeRecusado {
                token: "tok".into(),
                motivo: VolumeRefusal::SemEspera,
            },
        ),
        (
            "MessageRejected",
            ServerMessage::MessageRejected {
                channel: ChannelId(1),
                client_message_id: ClientMessageId(4),
                reason: MessageRefusal::NoSuchChannel,
            },
        ),
        ("Tocando", ServerMessage::Tocando { prazo_s: 45 }),
        (
            "Membros",
            ServerMessage::Membros {
                membros: vec![Membro {
                    id: PersonId(42),
                    nickname: "maya".into(),
                }],
                fim: true,
            },
        ),
        (
            "MembroNovo",
            ServerMessage::MembroNovo {
                membro: Membro {
                    id: PersonId(43),
                    nickname: "shinji".into(),
                },
            },
        ),
        (
            "MembroBanido",
            ServerMessage::MembroBanido {
                person: PersonId(43),
            },
        ),
        (
            "HashesDoNavegador",
            ServerMessage::HashesDoNavegador {
                janelas: vec![seele_proto::control::JanelaDoNavegador {
                    hash: [0xC3; 32],
                    nao_antes: 1_790_000_000,
                    nao_depois: 1_791_123_200,
                }],
            },
        ),
        (
            "ModAviso",
            ServerMessage::ModAviso {
                mod_id: "seele/mesa".into(),
                canal: Some(ChannelId(1)),
                carga: r#"{"rodada":2}"#.into(),
            },
        ),
        (
            "TemaMudou",
            ServerMessage::TemaMudou {
                tema: Some(seele_proto::control::TemaDoServidor {
                    mod_id: "seele/estilo".into(),
                    valores: r##"{"acento":"#c86b1f"}"##.into(),
                }),
            },
        ),
    ]
}

/// O que viaja fora das duas listas: o enquadramento, a recusa, o datagrama de
/// voz, a abertura de um fluxo de tela, o byte de tipo de cada fluxo e os
/// cabeçalhos de anexo e de volume.
fn cabecalhos() -> Vec<(String, Vec<u8>)> {
    let mut todos = vec![
        (
            "quadro:enquadramento".to_owned(),
            seele_proto::quadro::enquadrar(&[9, 11, 0]).unwrap(),
        ),
        ("recusa:8".to_owned(), recusa_por_versao(8).to_vec()),
        ("recusa:9".to_owned(), recusa_por_versao(9).to_vec()),
    ];

    // As razões de fim da linha 1, do ordinal 18 em diante: os bytes de um
    // `Disconnecting` com cada uma, porque é nele que elas viajam.
    for (nome, razao) in [
        ("ChamadaEncerrada", DisconnectReason::ChamadaEncerrada),
        ("NinguemAtendeu", DisconnectReason::NinguemAtendeu),
        ("ChamadaRecusada", DisconnectReason::ChamadaRecusada),
        (
            "ModsDeOutraApi",
            DisconnectReason::ModsDeOutraApi {
                apis: seele_proto::control::mascara_de_apis(&[7]),
            },
        ),
    ] {
        todos.push((
            format!("motivo:{nome}"),
            encode(&ServerMessage::Disconnecting { reason: razao }).unwrap(),
        ));
    }

    let mut voz = [0_u8; seele_proto::HEADER_LEN + 2];
    let len = MediaHeader {
        ssrc: 0xDEAD_BEEF,
        seq: 0x1234,
        timestamp: 0x0009_6000,
    }
    .encode_datagram(&[0xFC, 0xFF], &mut voz)
    .unwrap();
    todos.push(("datagrama:voz".to_owned(), voz[..len].to_vec()));

    let mut tela = [0_u8; seele_proto::screen::SCREEN_HEADER_LEN];
    ScreenHeader {
        version: SCREEN_HEADER_VERSION,
        screen: ScreenId(5),
        source: ScreenSource::Monitor,
        codec: ScreenCodec::H264Baseline,
        width: 1920,
        height: 1080,
    }
    .encode(&mut tela)
    .unwrap();
    todos.push(("fluxo:abertura-de-tela".to_owned(), tela.to_vec()));

    // Os quadros que seguem a abertura, um de cada tipo: o cabeçalho de cinco
    // bytes e um corpo curto (o começo de uma NAL H.264, ou dois bytes de
    // Opus). É o que o servidor, o desktop e o celular leem igual.
    for (nome, tipo, corpo) in [
        ("comum", TipoDeQuadro::Comum, &[0, 0, 0, 1, 0x41][..]),
        ("chave", TipoDeQuadro::Chave, &[0, 0, 0, 1, 0x65][..]),
        ("som", TipoDeQuadro::Som, &[0xFC, 0xFF][..]),
    ] {
        let mut quadro = cabecalho_de_quadro(tipo, u32::try_from(corpo.len()).unwrap()).to_vec();
        quadro.extend_from_slice(corpo);
        todos.push((format!("fluxo:quadro-de-tela:{nome}"), quadro));
    }

    for (nome, tipo) in [
        ("anexo", StreamType::Attachment),
        ("tela", StreamType::Screen),
        ("volume-de-mod", StreamType::ModVolume),
    ] {
        todos.push((format!("fluxo:tipo:{nome}"), vec![tipo.byte()]));
    }

    todos.push((
        "fluxo:cabecalho-de-anexo".to_owned(),
        encode(&seele_proto::attachment::AttachmentHeader {
            channel: ChannelId(1),
            client_message_id: ClientMessageId(4),
            body: "segue".into(),
            replies_to: None,
            file_name: "foto.png".into(),
            declared_type: "image/png".into(),
            declared_len: 1_024,
            content_hash: [0x5A; 32],
        })
        .unwrap(),
    ));
    todos.push((
        "fluxo:entrega-de-anexo".to_owned(),
        encode(&seele_proto::attachment::AttachmentDelivery {
            attachment: AttachmentId(9),
            byte_size: 1_024,
            content_hash: [0x5A; 32],
        })
        .unwrap(),
    ));
    todos.push((
        "fluxo:cabecalho-de-volume".to_owned(),
        encode(&VolumeHeader {
            mod_id: "autor/nome".into(),
            token: "tok".into(),
        })
        .unwrap(),
    ));
    todos.push((
        "fluxo:pedido-de-imagem".to_owned(),
        encode(&PedidoDeImagem::Ler {
            mod_id: "autor/nome".into(),
            channel: ChannelId(1),
            payload: "{}".into(),
        })
        .unwrap(),
    ));
    todos.push((
        "fluxo:resposta-de-imagem".to_owned(),
        encode(&RespostaDeImagem {
            erro: None,
            bytes: 1_024,
        })
        .unwrap(),
    ));
    todos
}

fn exemplares() -> Vec<(String, Vec<u8>)> {
    let mut todos = Vec::new();
    for (nome, mensagem) in clientes() {
        todos.push((format!("cliente:{nome}"), encode(&mensagem).unwrap()));
    }
    for (nome, mensagem) in servidores() {
        todos.push((format!("servidor:{nome}"), encode(&mensagem).unwrap()));
    }
    todos.extend(cabecalhos());
    todos
}

/// O ordinal que o postcard escreveu: o byte 1, porque toda lista de cima cabe
/// num byte de varint (menos de 128 variantes).
fn ordinais(quadros: &[(String, Vec<u8>)], prefixo: &str) -> BTreeSet<u32> {
    quadros
        .iter()
        .filter(|(nome, _)| nome.starts_with(prefixo))
        .map(|(_, bytes)| u32::from(bytes[1]))
        .collect()
}

/// **Toda variante das duas listas tem um exemplar**, e o nome dele é o da
/// variante. Uma variante nova sem exemplar reprova aqui, dizendo qual.
///
/// Um exemplar a mais para a mesma variante, que prende a forma de um motivo
/// aninhado, leva o nome da variante, um ponto e o nome do motivo
/// (`Alert.PedidoRecusado`): é a parte antes do ponto que tem de bater.
#[test]
fn toda_variante_das_listas_de_cima_tem_um_exemplar() {
    let todos = exemplares();
    let listas = [
        (
            "cliente:",
            ClientMessage::CONHECIDAS,
            seele_proto::vocabulario::variantes::<ClientMessage>().unwrap(),
        ),
        (
            "servidor:",
            ServerMessage::CONHECIDAS,
            seele_proto::vocabulario::variantes::<ServerMessage>().unwrap(),
        ),
    ];
    for (prefixo, conhecidas, nomes) in listas {
        let tem = ordinais(&todos, prefixo);
        let faltam: Vec<&str> = (0..conhecidas)
            .filter(|ordinal| !tem.contains(ordinal))
            .map(|ordinal| nomes[ordinal as usize])
            .collect();
        assert!(faltam.is_empty(), "sem exemplar em {prefixo} {faltam:?}");

        // E o nome de cada exemplar é o da variante no ordinal que ele ocupa:
        // um exemplar com o nome trocado prenderia os bytes certos sob o nome
        // errado.
        for (nome, bytes) in todos.iter().filter(|(n, _)| n.starts_with(prefixo)) {
            let variante = nomes[usize::from(bytes[1])];
            assert_eq!(
                nome[prefixo.len()..].split('.').next().unwrap_or_default(),
                variante,
                "o exemplar {nome} está no ordinal de {variante}"
            );
        }
    }
}

#[test]
fn o_fio_bate_com_os_bytes_ouro() {
    let caminho = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ARQUIVO);
    let gerado = exemplares();
    let lido = std::fs::read_to_string(&caminho).unwrap_or_default();
    let congelado = lido.lines().next() == Some("# estado: congelado");
    let guardado: Vec<(String, String)> = lido
        .lines()
        .filter(|linha| !linha.starts_with('#'))
        .filter_map(|linha| linha.split_once('\t'))
        .map(|(nome, bytes)| (nome.to_owned(), bytes.to_owned()))
        .collect();

    if std::env::var_os("SEELE_REGERAR_FIO").is_some() {
        let mudaram: Vec<&str> = guardado
            .iter()
            .filter(|(nome, bytes)| {
                gerado
                    .iter()
                    .any(|(outro, novos)| outro == nome && hex(novos) != *bytes)
            })
            .map(|(nome, _)| nome.as_str())
            .collect();
        assert!(
            !congelado || mudaram.is_empty(),
            "o fio está congelado (ADR 0058) e estes quadros mudaram de bytes: {mudaram:?}"
        );
        let sumiram: Vec<&str> = guardado
            .iter()
            .filter(|(nome, _)| !gerado.iter().any(|(outro, _)| outro == nome))
            .map(|(nome, _)| nome.as_str())
            .collect();
        assert!(
            !congelado || sumiram.is_empty(),
            "o fio está congelado (ADR 0058) e estes quadros sumiram: {sumiram:?}"
        );
        let cabeca = if congelado {
            "# estado: congelado"
        } else {
            "# estado: rascunho"
        };
        let corpo: String = gerado
            .iter()
            .map(|(nome, bytes)| format!("{nome}\t{}\n", hex(bytes)))
            .collect();
        std::fs::write(&caminho, format!("{cabeca}\n{corpo}")).unwrap();
        return;
    }

    let diferentes: Vec<&str> = gerado
        .iter()
        .filter(|(nome, bytes)| {
            guardado
                .iter()
                .find(|(outro, _)| outro == nome)
                .map(|(_, guardados)| guardados.as_str())
                != Some(hex(bytes).as_str())
        })
        .map(|(nome, _)| nome.as_str())
        .collect();
    let sumiram: Vec<&str> = guardado
        .iter()
        .filter(|(nome, _)| !gerado.iter().any(|(outro, _)| outro == nome))
        .map(|(nome, _)| nome.as_str())
        .collect();
    assert!(
        diferentes.is_empty() && sumiram.is_empty(),
        "estes quadros não batem com {ARQUIVO}: {diferentes:?}, e estes sumiram do código: \
         {sumiram:?}. Se o fio mudou de propósito e o estado é rascunho, rode \
         SEELE_REGERAR_FIO=1 cargo test -p seele-proto --test fio_1_0 e leia o diff. Se o \
         estado é congelado, a mudança quebra a linha 1 (ADR 0058)."
    );
}

/// **Os enums que viajam dentro das mensagens não mudam de ordem.**
///
/// Até a 0.15 nenhum deles tinha guarda (análise de 22/09, §3.2): o guarda dos
/// ordinais de `control.rs` só conta as duas listas de cima. Uma razão de
/// `Disconnecting` reordenada faria um par ler «banido» onde o servidor disse
/// «expulso», sem erro nenhum.
///
/// Acrescentar no fim é permitido numa 1.x, mas só com o portão de quem manda
/// (`session::entende_a_mensagem`): a variante nova só sai para quem a
/// entende. Por isso o acréscimo também reprova aqui — ele tem de ser decidido,
/// e não escorregar.
#[test]
fn os_enums_aninhados_nao_mudam_de_ordem() {
    use seele_proto::vocabulario::variantes;

    /// O nome do enum, a lista que o `serde` dá e a lista presa aqui.
    type Caso = (
        &'static str,
        Option<&'static [&'static str]>,
        &'static [&'static str],
    );

    let casos: [Caso; 13] = [
        (
            "DisconnectReason",
            variantes::<DisconnectReason>(),
            &[
                "Incompatible",
                "CredentialRejected",
                "HandshakeTimeout",
                "Kicked",
                "Banned",
                "ServerFull",
                "ScheduledMaintenance",
                "ServerShuttingDown",
                "Timeout",
                "ProtocolViolation",
                "RateLimited",
                "FellBehind",
                "AdmissionPending",
                "AdmissionDenied",
                "NicknameTaken",
                "ModsRecusados",
                "ModsMudaram",
                "ModsIndisponiveis",
                "ChamadaEncerrada",
                "NinguemAtendeu",
                "ChamadaRecusada",
                "ModsDeOutraApi",
            ],
        ),
        (
            "AlertReason",
            variantes::<AlertReason>(),
            &[
                "Mentioned",
                "SubsystemChanged",
                "SyncDegraded",
                "VoiceRoomEntryRefused",
                "PermissionDenied",
                "VoiceRoomFull",
                "OperatorNotice",
                "RateLimited",
                "MovedByOperator",
                "VoiceRoomDeleted",
                "ChannelDeleted",
                "LastVoiceRoom",
                "ScreenShareTaken",
                "ScreenShareOverHostUplink",
                "VoiceRoomOverHostUplink",
                "RetencaoApagouHistorico",
                "PedidoRecusado",
            ],
        ),
        (
            "CausaDaRecusa",
            variantes::<CausaDaRecusa>(),
            &["NaoExiste", "NomeTomado", "FalhaDoServidor"],
        ),
        (
            "AlertSeverity",
            variantes::<AlertSeverity>(),
            &["Info", "Warning", "Critical"],
        ),
        (
            "MessageRefusal",
            variantes::<MessageRefusal>(),
            &[
                "PermissionDenied",
                "NoSuchChannel",
                "TooLong",
                "StorageFailed",
            ],
        ),
        (
            "AttachmentRefusal",
            variantes::<AttachmentRefusal>(),
            &[
                "NotAllowed",
                "TooLarge",
                "NoRoom",
                "SizeMismatch",
                "HashDidNotMatch",
                "RateLimited",
                "Unavailable",
                "NotFound",
                "Expired",
                "Malformed",
            ],
        ),
        (
            "AttachmentState",
            variantes::<AttachmentState>(),
            &["Available", "Expired"],
        ),
        (
            "MotivoDeFalhaDePar",
            variantes::<MotivoDeFalhaDePar>(),
            &[
                "NaoAlcancou",
                "ImpressaoNaoBate",
                "CaiuNoMeio",
                "ParouDeMandar",
                "NaoFuiAceito",
            ],
        ),
        (
            "Permission",
            variantes::<Permission>(),
            &[
                "ViewVoiceRoom",
                "EnterVoiceRoom",
                "Speak",
                "ReadChannel",
                "WriteChannel",
                "RemoveMessage",
                "MovePerson",
                "Kick",
                "Ban",
                "ManageVoiceRooms",
                "ManageRoles",
                "AdministerServer",
                "AttachFile",
            ],
        ),
        (
            "Presence",
            variantes::<Presence>(),
            &["Available", "Away", "DoNotDisturb"],
        ),
        (
            "Subsystem",
            variantes::<Subsystem>(),
            &["Permissions", "Media", "Persistence"],
        ),
        (
            "SubsystemHealth",
            variantes::<SubsystemHealth>(),
            &["Nominal", "Degraded", "Failed"],
        ),
        (
            "VolumeRefusal",
            variantes::<VolumeRefusal>(),
            &["SemEspera", "TipoRecusado", "NaoGravei", "Incompleto"],
        ),
    ];
    let mut errados = Vec::new();
    for (nome, do_serde, preso) in casos {
        let do_serde = do_serde.unwrap_or_else(|| panic!("{nome} deixou de ser enum"));
        if do_serde != preso {
            errados.push(format!("{nome}: {do_serde:?}"));
        }
    }
    assert!(
        errados.is_empty(),
        "estes enums mudaram — leia o ADR 0058 antes de mexer:\n{}",
        errados.join("\n")
    );

    // O `Incompatible` no ordinal 0 é congelado para sempre (ADR 0058): é por
    // ele que toda build, desde a 0.11, lê a recusa de outra linha.
    assert_eq!(
        variantes::<DisconnectReason>().unwrap()[0],
        "Incompatible",
        "o Incompatible saiu do ordinal 0, congelado pelo ADR 0058"
    );
}

/// Os códigos de um byte da abertura de tela e do tipo de fluxo, que não
/// passam pelo `serde` e por isso não estão no teste acima.
#[test]
fn os_codigos_de_um_byte_nao_mudam() {
    assert_eq!(ScreenSource::Monitor.code(), 0);
    assert_eq!(ScreenSource::Window.code(), 1);
    assert_eq!(ScreenCodec::H264Baseline.code(), 0);
    assert_eq!(StreamType::Attachment.byte(), 1);
    assert_eq!(StreamType::Screen.byte(), 2);
    assert_eq!(StreamType::ModVolume.byte(), 3);
    assert_eq!(TipoDeQuadro::Comum.byte(), 0);
    assert_eq!(TipoDeQuadro::Chave.byte(), 1);
    assert_eq!(TipoDeQuadro::Som.byte(), 2);
}

/// **Os quadros de tela do ouro voltam inteiros pelo leitor.** O teste de cima
/// prende o que se escreve; este prende o que se lê, com os bytes do arquivo:
/// a abertura e os três quadros, emendados como num fluxo, saem na ordem e com
/// o corpo de cada um.
#[test]
fn os_quadros_de_tela_do_ouro_voltam_pelo_leitor() {
    let caminho = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ARQUIVO);
    let lido = std::fs::read_to_string(&caminho).unwrap();
    let de = |nome: &str| -> Vec<u8> {
        let hex = lido
            .lines()
            .find_map(|linha| linha.strip_prefix(&format!("{nome}\t")))
            .unwrap_or_else(|| panic!("o ouro não tem {nome}"));
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect()
    };
    let mut fluxo = de("fluxo:abertura-de-tela");
    for nome in ["comum", "chave", "som"] {
        fluxo.extend(de(&format!("fluxo:quadro-de-tela:{nome}")));
    }
    let mut leitor = LeitorDeTela::novo();
    leitor.empurrar(&fluxo).unwrap();
    let saiu: Vec<LidoDaTela> = std::iter::from_fn(|| leitor.proximo()).collect();
    assert!(matches!(saiu.first(), Some(LidoDaTela::Abertura(_))));
    let quadros: Vec<(TipoDeQuadro, Vec<u8>)> = saiu
        .into_iter()
        .filter_map(|l| match l {
            LidoDaTela::Quadro { tipo, bytes } => Some((tipo, bytes)),
            LidoDaTela::Abertura(_) => None,
        })
        .collect();
    assert_eq!(
        quadros,
        vec![
            (TipoDeQuadro::Comum, vec![0, 0, 0, 1, 0x41]),
            (TipoDeQuadro::Chave, vec![0, 0, 0, 1, 0x65]),
            (TipoDeQuadro::Som, vec![0xFC, 0xFF]),
        ]
    );
}
