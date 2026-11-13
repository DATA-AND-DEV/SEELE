use std::sync::{Arc, Mutex};

use seele_core::{PersonId, ScreenId, ServerMessage, SessionId, Ssrc, VoiceRoomId, VoiceRoomInfo};

use super::{Connection, Shared};

/// Uma sessão (somos a pessoa 7) sentada na sala 1, onde a pessoa 3
/// transmite a tela 9 e a pessoa 4 a tela 11.
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
        for (pessoa, tela) in [(3, 9), (4, 11)] {
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

/// A tela do retrato, e quais da lista dizem `assistida`.
fn no_retrato(conexao: &Connection) -> (Option<u32>, Vec<u32>) {
    let retrato = conexao.snapshot();
    let assistidas = retrato
        .transmissoes
        .iter()
        .filter(|transmissao| transmissao.assistida)
        .map(|transmissao| transmissao.tela)
        .collect();
    (retrato.tela.map(|tela| tela.tela), assistidas)
}

// Os números de leitura são os do `Aviso::TelaAbriu`/`TelaFechou`.

#[test]
fn a_abertura_automatica_aparece_no_retrato_e_como_assistida() {
    let (conexao, shared) = sessao();
    assert_eq!(no_retrato(&conexao), (None, vec![]));
    shared.tela_abriu(ScreenId(9), 1);
    assert_eq!(
        no_retrato(&conexao),
        (Some(9), vec![9]),
        "o servidor ligou a tela 9 e o retrato diz que ninguém está compartilhando"
    );
    // A escolha continua sendo de ninguém: ligar não é escolher.
    assert_eq!(conexao.tela_escolhida(), None);
}

#[test]
fn a_escolha_explicita_de_outra_tela_vence_a_automatica() {
    let (conexao, shared) = sessao();
    shared.tela_abriu(ScreenId(9), 1);
    let _ = conexao.assistir(11, true);
    assert_eq!(no_retrato(&conexao), (Some(11), vec![11]));
    // E a 9 chegando de novo (o servidor reenviando) não muda a escolha.
    shared.tela_abriu(ScreenId(9), 2);
    assert_eq!(no_retrato(&conexao), (Some(11), vec![11]));
}

#[test]
fn o_nao_ver_explicito_vence_a_automatica() {
    let (conexao, shared) = sessao();
    shared.tela_abriu(ScreenId(9), 1);
    let _ = conexao.assistir(9, false);
    assert_eq!(
        no_retrato(&conexao),
        (None, vec![]),
        "quem recusou aparece assistindo"
    );
    // O servidor religando não desfaz a recusa no retrato.
    shared.tela_abriu(ScreenId(9), 2);
    shared.tela_abriu(ScreenId(11), 3);
    assert_eq!(no_retrato(&conexao), (None, vec![]));
}

#[test]
fn o_fechamento_limpa() {
    let (conexao, shared) = sessao();
    shared.tela_abriu(ScreenId(9), 1);
    assert!(
        shared.tela_fechou(ScreenId(9), 1),
        "o fim da vigente não fechou"
    );
    assert_eq!(no_retrato(&conexao), (None, vec![]));
}

#[test]
fn o_fim_da_transmissao_no_servidor_limpa_mesmo_sem_tela_fechou() {
    // O `ScreenShareStopped` tira a tela da sala antes do fim da leitura.
    let (conexao, shared) = sessao();
    shared.tela_abriu(ScreenId(9), 1);
    crate::fold(
        &shared,
        &ServerMessage::ScreenShareStopped {
            voice_room: VoiceRoomId(1),
            screen: ScreenId(9),
        },
    );
    assert_eq!(no_retrato(&conexao), (None, vec![]));
    assert_eq!(
        shared
            .tela_ligada
            .lock()
            .ok()
            .and_then(|ligada| ligada.agora(1)),
        None,
        "a tela que saiu da sala ficou guardada"
    );
}

/// Astra: a troca de par aborta a leitura da mesma tela (o fim dela não
/// chegava), a nova abre e depois acaba — e o retrato não pode ficar com
/// um fantasma da primeira.
#[test]
fn a_leitura_substituida_nao_deixa_fantasma() {
    let (conexao, shared) = sessao();
    shared.tela_abriu(ScreenId(9), 1);
    shared.tela_abriu(ScreenId(9), 2);
    assert!(shared.tela_fechou(ScreenId(9), 2));
    assert_eq!(
        no_retrato(&conexao),
        (None, vec![]),
        "a leitura nova acabou e o retrato diz que se assiste"
    );
}

#[test]
fn na_alternancia_9_11_9_fica_a_9_nova_e_o_fim_velho_nao_a_apaga() {
    let (conexao, shared) = sessao();
    shared.tela_abriu(ScreenId(9), 1);
    shared.tela_abriu(ScreenId(11), 2);
    shared.tela_abriu(ScreenId(9), 3);
    assert_eq!(no_retrato(&conexao), (Some(9), vec![9]));
    // Os fins atrasados das duas primeiras leituras.
    assert!(
        !shared.tela_fechou(ScreenId(9), 1),
        "o fim de uma leitura substituída foi tratado como o da vigente"
    );
    let _ = shared.tela_fechou(ScreenId(11), 2);
    assert_eq!(
        no_retrato(&conexao),
        (Some(9), vec![9]),
        "um fim velho apagou a 9 nova"
    );
    assert!(shared.tela_fechou(ScreenId(9), 3));
    assert_eq!(no_retrato(&conexao), (None, vec![]));
}

#[test]
fn o_fim_da_tela_anterior_nao_apaga_a_nova() {
    let (conexao, shared) = sessao();
    shared.tela_abriu(ScreenId(9), 1);
    shared.tela_abriu(ScreenId(11), 2);
    let _ = shared.tela_fechou(ScreenId(9), 1);
    assert_eq!(
        no_retrato(&conexao),
        (Some(11), vec![11]),
        "o `TelaFechou` da tela anterior apagou a nova"
    );
}

#[test]
fn a_troca_de_geracao_nao_deixa_tela_velha_e_o_fim_atrasado_nao_apaga_a_nova() {
    let (conexao, shared) = sessao();
    shared.tela_abriu(ScreenId(9), 1);
    // O enlace voltou (o braço `Reconectado`).
    shared.nova_geracao();
    shared.tela_ligada_nova_geracao();
    assert_eq!(
        no_retrato(&conexao),
        (None, vec![]),
        "a tela ligada antes da queda ficou no retrato da volta"
    );
    shared.tela_abriu(ScreenId(9), 2);
    assert_eq!(no_retrato(&conexao), (Some(9), vec![9]));
    // O fim da leitura de antes da queda chega agora.
    assert!(!shared.tela_fechou(ScreenId(9), 1));
    assert_eq!(
        no_retrato(&conexao),
        (Some(9), vec![9]),
        "o fim atrasado da leitura da geração anterior apagou a da volta"
    );
}

#[test]
fn o_proprio_espelho_nao_e_tela_ligada() {
    let (conexao, shared) = sessao();
    if let Ok(mut room) = shared.room.lock() {
        room.apply(&ServerMessage::ScreenShareStarted {
            voice_room: VoiceRoomId(1),
            person: PersonId(7),
            screen: ScreenId(20),
        });
    }
    shared.tela_abriu(ScreenId(9), 1);
    // Esta pessoa começa a compartilhar: o espelho dela chega como abertura.
    shared.tela_abriu(ScreenId(20), 2);
    assert_eq!(no_retrato(&conexao), (Some(9), vec![9]));
    // E o fim do espelho é o vigente dele: o palco dele fecha.
    assert!(shared.tela_fechou(ScreenId(20), 2));
}

#[test]
fn cada_sessao_tem_a_sua() {
    let (uma, shared_uma) = sessao();
    let (outra, _) = sessao();
    shared_uma.tela_abriu(ScreenId(9), 1);
    assert_eq!(no_retrato(&uma), (Some(9), vec![9]));
    assert_eq!(no_retrato(&outra), (None, vec![]));
}

/// Astra P2: a leitura velha da 9 solta um quadro depois da abertura nova
/// da mesma 9, antes de o abort efetivar. Ele não chega à casca — que só
/// conhece a tela, e o daria ao decodificador da nova —, e os da nova sim.
#[test]
fn o_quadro_da_leitura_substituida_nao_chega_a_casca() {
    struct Anota(Mutex<Vec<Vec<u8>>>);
    impl crate::EventListener for Anota {
        fn on_event(&self, event: crate::Event) {
            if let crate::Event::ScreenFrame { data, .. } = event {
                if let Ok(mut anotados) = self.0.lock() {
                    anotados.push(data.into_bytes());
                }
            }
        }
    }
    let (conexao, shared) = sessao();
    let anota = Arc::new(Anota(Mutex::new(Vec::new())));
    conexao.subscribe(Arc::clone(&anota) as Arc<dyn crate::EventListener>);

    shared.tela_abriu(ScreenId(9), 1);
    shared.quadro_da_tela(ScreenId(9), 1, true, b"velha-1");
    shared.tela_abriu(ScreenId(9), 2);
    shared.quadro_da_tela(ScreenId(9), 1, false, b"velha-2");
    shared.quadro_da_tela(ScreenId(9), 2, true, b"nova-1");
    shared.quadro_da_tela(ScreenId(9), 2, false, b"nova-2");

    let chegaram = anota.0.lock().map(|a| a.clone()).unwrap_or_default();
    let esperados: Vec<Vec<u8>> = [&b"velha-1"[..], b"nova-1", b"nova-2"]
        .iter()
        .map(|bytes| crate::base64_de(bytes).into_bytes())
        .collect();
    assert_eq!(
        chegaram, esperados,
        "um quadro da leitura substituída chegou à casca, ou um da nova não chegou"
    );
}

/// O mesmo para o som: o pacote da leitura velha da 9, depois da abertura
/// nova da mesma 9, não vai à fila da mistura; o da nova vai. O braço
/// `TelaSom` do laço só chama `Voice::som_da_tela` quando
/// `som_da_leitura_vale` diz que sim.
#[test]
fn o_som_da_leitura_substituida_nao_entra_na_fila() {
    let (_conexao, shared) = sessao();
    shared.tela_abriu(ScreenId(9), 1);
    assert!(shared.som_da_leitura_vale(ScreenId(9), 1));
    shared.tela_abriu(ScreenId(9), 2);
    assert!(
        !shared.som_da_leitura_vale(ScreenId(9), 1),
        "o som da leitura substituída entraria na fila da nova"
    );
    assert!(
        shared.som_da_leitura_vale(ScreenId(9), 2),
        "o som da leitura nova não entraria na fila"
    );
}

/// **O retrato carrega a pausa quando o motor pausa a transmissão desta
/// pessoa, e a larga quando ela volta.**
///
/// `TelaEmCurso::parada` era sempre `None`: o teto parava o vídeo (o §3.2 —
/// quando o sinal aperta, quem cede é a imagem), quem escoava fechava o fluxo
/// e ninguém contava a quem transmitia. A casca já tinha a frase e o lugar;
/// faltava o estado chegar.
///
/// O que entra aqui é o que o `Aviso::TelaParou` e o `Aviso::TelaAbriu` fazem
/// na ponte, e o que sai é o retrato que a casca lê.
#[test]
fn o_retrato_diz_que_a_minha_tela_esta_pausada_e_deixa_de_dizer_quando_ela_volta() {
    use seele_core::MotivoDeParada;

    let (conexao, shared) = sessao();
    if let Ok(mut room) = shared.room.lock() {
        room.apply(&ServerMessage::ScreenShareStarted {
            voice_room: VoiceRoomId(1),
            person: PersonId(7),
            screen: ScreenId(13),
        });
    }
    let parada_da_minha = |conexao: &Connection| {
        conexao
            .snapshot()
            .minha_transmissao
            .expect("esta pessoa está transmitindo")
            .parada
    };
    assert_eq!(parada_da_minha(&conexao), None);

    // O motor pausa: o sinal ficou crítico.
    assert!(shared.a_tela_parou(ScreenId(13), MotivoDeParada::SinalCritico));
    assert_eq!(
        parada_da_minha(&conexao).as_deref(),
        Some("SinalCritico"),
        "o motor pausou a transmissão desta pessoa e o retrato não diz: ela seguiria vendo o \
         espelho congelado e o botão em PARAR DE COMPARTILHAR, como se a sala estivesse vendo"
    );
    assert!(
        crate::motivos_de_parada_da_tela().contains(&"SinalCritico"),
        "o nome que o retrato carrega saiu da lista de que a casca tira as frases"
    );
    // E não vaza para a tela de outra pessoa que esta esteja assistindo.
    shared.tela_abriu(ScreenId(9), 1);
    let retrato = conexao.snapshot();
    assert_eq!(retrato.tela.as_ref().map(|tela| tela.tela), Some(9));
    assert_eq!(
        retrato.tela.and_then(|tela| tela.parada),
        None,
        "a pausa do que esta máquina manda foi dita sobre a tela de outra pessoa"
    );

    // A abertura da tela de outra pessoa não é a volta da nossa.
    assert!(!shared.a_tela_voltou(ScreenId(9)));
    assert_eq!(parada_da_minha(&conexao).as_deref(), Some("SinalCritico"));
    // A da nossa é.
    assert!(shared.a_tela_voltou(ScreenId(13)));
    assert_eq!(
        parada_da_minha(&conexao),
        None,
        "a transmissão voltou e o retrato segue dizendo que ela está pausada"
    );

    // Parar de compartilhar esquece a pausa: a próxima transmissão não nasce
    // «pausada» por causa desta.
    assert!(shared.a_tela_parou(ScreenId(13), MotivoDeParada::AbaixoDoPiso));
    let _ = conexao.parar_de_compartilhar();
    assert_eq!(parada_da_minha(&conexao), None);
}
