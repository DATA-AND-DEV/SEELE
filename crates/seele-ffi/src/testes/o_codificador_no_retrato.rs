//! O codificador de vídeo em uso chega ao retrato.
//!
//! O motor sabia e a tela não contava: o `armar` do `seele-video` escolhe entre
//! o codificador do sistema e a queda para software, e a escolha só ia ao
//! `seele.log`. O nome sobe pelo `Aviso::TelaAbriu` da própria transmissão, a
//! ponte o guarda com o número da tela, e o retrato o veste **só** na tela desta
//! pessoa e de mesmo número.

use std::sync::{Arc, Mutex};

use seele_core::{PersonId, ScreenId, ServerMessage, SessionId, Ssrc, VoiceRoomId, VoiceRoomInfo};

use super::{Connection, Shared};

/// Uma sessão (somos a pessoa 7) na sala 1, onde nós transmitimos a tela 9 e
/// a pessoa 3 transmite a tela 11.
fn sessao() -> (Connection, Arc<Shared>) {
    let shared = crate::sem_rede::shared("marcela");
    if let Ok(mut room) = shared.room.lock() {
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
        for (pessoa, tela) in [(7, 9), (3, 11)] {
            room.apply(&ServerMessage::ScreenShareStarted {
                voice_room: VoiceRoomId(1),
                person: PersonId(pessoa),
                screen: ScreenId(tela),
            });
        }
    }
    let (commands, _fila) = tokio::sync::mpsc::unbounded_channel();
    let conexao = Connection {
        commands,
        shared: Arc::clone(&shared),
        encerrada: Mutex::new(None),
        ligacao: None,
    };
    (conexao, shared)
}

#[test]
fn o_retrato_carrega_o_codificador_da_transmissao_desta_pessoa() {
    let (conexao, shared) = sessao();
    let minha = conexao
        .snapshot()
        .minha_transmissao
        .expect("a sessão de teste transmite a tela 9");
    assert_eq!(minha.tela, 9);
    assert_eq!(
        minha.codificador, None,
        "antes de o fluxo abrir ninguém disse o codificador, e o retrato inventou um"
    );

    // O que o `drive` faz ao receber o `Aviso::TelaAbriu` da própria tela.
    shared.gravar_codificador_da_tela(Some((ScreenId(9), "OpenH264 (software)".into())));
    let retrato = conexao.snapshot();
    assert_eq!(
        retrato
            .minha_transmissao
            .as_ref()
            .and_then(|tela| tela.codificador.as_deref()),
        Some("OpenH264 (software)"),
        "o motor disse qual codificador abriu a transmissão e o retrato não o carrega: a \
         tela de quem transmite não tem como mostrá-lo"
    );
    // E atravessa pelo nome que a janela lê.
    let json = serde_json::to_string(&retrato).expect("o retrato serializa");
    assert!(
        json.contains("\"codificador\":\"OpenH264 (software)\""),
        "o codificador não atravessa para a janela pelo nome `codificador`"
    );
}

#[test]
fn o_codificador_nao_veste_a_tela_de_outra_pessoa_nem_a_de_antes() {
    let (conexao, shared) = sessao();
    // Assistindo a tela 11, que é da pessoa 3.
    let _ = shared.gravar_tela_escolhida(super::EscolhaDeTela::Esta(ScreenId(11)));
    shared.gravar_codificador_da_tela(Some((ScreenId(9), "VideoToolbox".into())));
    let retrato = conexao.snapshot();
    assert_eq!(
        retrato
            .tela
            .as_ref()
            .map(|tela| (tela.tela, tela.codificador.clone())),
        Some((11, None)),
        "o codificador desta máquina apareceu ao lado da tela de outra pessoa"
    );

    // O nome de uma transmissão de antes (outra tela) não veste a de agora.
    shared.gravar_codificador_da_tela(Some((ScreenId(4), "VideoToolbox".into())));
    assert_eq!(
        conexao
            .snapshot()
            .minha_transmissao
            .and_then(|tela| tela.codificador),
        None,
        "o codificador de outra transmissão vestiu a tela 9"
    );
}

#[test]
fn o_codificador_morre_com_a_transmissao() {
    let (conexao, shared) = sessao();
    shared.gravar_codificador_da_tela(Some((ScreenId(9), "VideoToolbox".into())));
    // A transmissão acaba pelo caminho de verdade: o `fold` do `ScreenShareStopped`.
    super::fold(
        &shared,
        &ServerMessage::ScreenShareStopped {
            voice_room: VoiceRoomId(1),
            screen: ScreenId(9),
        },
    );
    assert!(conexao.snapshot().minha_transmissao.is_none());
    assert_eq!(
        shared
            .codificador_da_tela
            .lock()
            .expect("o cadeado do codificador")
            .clone(),
        None,
        "a transmissão acabou e o codificador dela ficou guardado: a próxima, com o mesmo \
         número, nasceria com o nome de antes"
    );
}
