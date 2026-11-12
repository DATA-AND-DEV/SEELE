//! A mesa mínima de texto da 1.0, na ponte: o que a casca passa a ter, e o
//! portão de versão dos três verbos do protocolo 13.
use seele_core::{ChannelInfo, ServerMessage, SessionId};
use tokio::sync::mpsc::UnboundedReceiver;

use super::*;

const GERAL: ChannelId = ChannelId(1);
const AVISOS: ChannelId = ChannelId(2);

/// Uma conexão que não existe, no protocolo `negociado`, com a fila de
/// comandos na mão: o que a casca mandar e **sair** aparece nela. A pessoa
/// é a 7, `marcela`, com a marca `K7Q2`.
fn conexao(negociado: u8) -> (Connection, UnboundedReceiver<Command>) {
    let shared = crate::sem_rede::shared("marcela");
    shared.perfis.guardar_o_protocolo(negociado);
    let (commands, fila) = tokio::sync::mpsc::unbounded_channel();
    let conexao = Connection {
        commands,
        shared,
        encerrada: Mutex::new(None),
        ligacao: None,
    };
    for mensagem in [
        ServerMessage::Session {
            id: SessionId(1),
            person: PersonId(7),
            ssrc: Ssrc(700),
            server: "Casa".into(),
            voice_rooms: Vec::new(),
            channels: vec![
                ChannelInfo {
                    id: GERAL,
                    name: "geral".into(),
                },
                ChannelInfo {
                    id: AVISOS,
                    name: "avisos".into(),
                },
            ],
            roles: Vec::new(),
            permissions: Vec::new(),
        },
        ServerMessage::PersonRenamed {
            person: PersonId(7),
            nickname: "marcela#K7Q2".into(),
        },
        ServerMessage::IdentidadeApresentada {
            pessoa: PersonId(7),
            base: "marcela".into(),
            tag: "K7Q2".into(),
        },
    ] {
        receber(&conexao.shared, &mensagem);
    }
    (conexao, fila)
}

fn dita(id: u64, autor: u64, canal: ChannelId, corpo: &str) -> ServerMessage {
    ServerMessage::MessageReceived {
        channel: canal,
        id: MessageId(id),
        author: PersonId(autor),
        author_nickname: format!("pessoa {autor}"),
        at_seconds: 1_700_000_000,
        body: corpo.into(),
        replies_to: None,
        client_message_id: None,
        attachment: None,
    }
}

struct Anota(Mutex<Vec<Event>>);
impl EventListener for Anota {
    fn on_event(&self, event: Event) {
        if let Ok(mut anotados) = self.0.lock() {
            anotados.push(event);
        }
    }
}

fn ouvindo(conexao: &Connection) -> Arc<Anota> {
    let anota = Arc::new(Anota(Mutex::new(Vec::new())));
    conexao.subscribe(Arc::clone(&anota) as Arc<dyn EventListener>);
    anota
}

fn ouvidos(anota: &Anota) -> Vec<Event> {
    std::mem::take(&mut *anota.0.lock().unwrap())
}

/// **Nenhum verbo do protocolo 13 sai para um servidor 12, e a casca
/// recebe o erro com nome.** Tirar o portão de um dos três métodos faz
/// este teste falhar: o comando chega à fila, e um servidor da 0.19 o
/// pularia calado.
#[test]
fn um_servidor_doze_nao_recebe_os_verbos_do_treze() {
    let (conexao, mut fila) = conexao(12);
    let antigo = ConnectionError::ServidorAntigo {
        negociado: 12,
        exige: 13,
    };
    assert_eq!(
        conexao.edit_message(5, "outra coisa".into()),
        Err(antigo.clone())
    );
    assert_eq!(conexao.unban_person(9), Err(antigo.clone()));
    assert_eq!(conexao.list_bans(), Err(antigo.clone()));
    assert!(
        fila.try_recv().is_err(),
        "um verbo do protocolo 13 saiu para um servidor 12"
    );
    // E a frase que a casca escolhe sai do JSON com os dois números.
    assert_eq!(
        serde_json::to_value(&antigo).unwrap(),
        serde_json::json!({ "ServidorAntigo": { "negociado": 12, "exige": 13 } })
    );
    // O que é do 9 continua saindo: responder não precisou de versão nova.
    assert!(conexao.send_reply(1, "respondo".into(), 5).is_ok());
    assert!(matches!(
        fila.try_recv(),
        Ok(Command::Send {
            replies_to: Some(MessageId(5)),
            ..
        })
    ));

    // Os mesmos pedidos, num servidor 13, saem.
    let (conexao, mut fila) = conexao_13();
    assert_eq!(conexao.edit_message(5, "  outra coisa  ".into()), Ok(()));
    assert_eq!(conexao.unban_person(9), Ok(()));
    assert_eq!(conexao.list_bans(), Ok(()));
    assert!(matches!(
        fila.try_recv(),
        Ok(Command::EditMessage { message: MessageId(5), body }) if body == "outra coisa"
    ));
    assert!(matches!(
        fila.try_recv(),
        Ok(Command::UnbanPerson {
            person: PersonId(9)
        })
    ));
    assert!(matches!(fila.try_recv(), Ok(Command::ListBans)));
}

fn conexao_13() -> (Connection, UnboundedReceiver<Command>) {
    conexao(13)
}

/// Antes do aperto de mão o número negociado é zero, e a resposta é «sem
/// conexão», e não «servidor antigo»: não há servidor de que falar.
#[test]
fn antes_do_aperto_de_mao_o_portao_diz_que_nao_ha_conexao() {
    let (conexao, mut fila) = conexao(0);
    assert_eq!(
        conexao.edit_message(5, "x".into()),
        Err(ConnectionError::NotConnected)
    );
    assert_eq!(conexao.list_bans(), Err(ConnectionError::NotConnected));
    assert!(fila.try_recv().is_err());
}

/// O que a edição recusa **antes** de sair: a pendente, o corpo vazio e o
/// corpo acima do teto. Cada um com o nome dele, e nenhum na fila.
#[test]
fn a_edicao_recusa_aqui_o_que_nao_pode_sair() {
    let (conexao, mut fila) = conexao(13);
    assert_eq!(
        conexao.edit_message(0, "x".into()),
        Err(ConnectionError::UnknownMessage)
    );
    assert_eq!(
        conexao.edit_message(5, "   \n ".into()),
        Err(ConnectionError::MensagemVazia)
    );
    assert_eq!(
        conexao.edit_message(5, "á".repeat(3_000)),
        Err(ConnectionError::MensagemGrandeDemais {
            limite_bytes: Connection::limite_da_mensagem() as u64
        })
    );
    assert_eq!(
        conexao.send_reply(1, "x".into(), 0),
        Err(ConnectionError::UnknownMessage)
    );
    assert!(fila.try_recv().is_err());
}

/// **Responder**: a pendente já nasce com o `replies_to`, o comando o
/// leva, tentar de novo o mantém, e a confirmada o traz do servidor. O
/// `send_message` de sempre continua sem resposta.
#[test]
fn a_resposta_vai_no_comando_na_pendente_e_no_tentar_de_novo() {
    let (conexao, mut fila) = conexao(13);
    conexao.shared.room.lock().unwrap().open_channel(GERAL);

    let chave = conexao.send_reply(GERAL.0, "concordo".into(), 9).unwrap();
    assert!(matches!(
        fila.try_recv(),
        Ok(Command::Send { replies_to: Some(MessageId(9)), body, .. }) if body == "concordo"
    ));
    let pendente = conexao.messages().pop().unwrap();
    assert_eq!(
        (pendente.id, pendente.replies_to, pendente.estado),
        (0, Some(9), EstadoDoEnvio::Enviando)
    );

    assert_eq!(conexao.reenviar_mensagem(&chave), Ok(true));
    assert!(
        matches!(
            fila.try_recv(),
            Ok(Command::Send {
                replies_to: Some(MessageId(9)),
                ..
            })
        ),
        "tentar de novo mandou a resposta solta"
    );

    conexao.send_message(GERAL.0, "solta".into()).unwrap();
    assert!(matches!(
        fila.try_recv(),
        Ok(Command::Send {
            replies_to: None,
            ..
        })
    ));

    // E a que vem do servidor traz a quem responde, no JSON que a casca lê.
    receber(
        &conexao.shared,
        &ServerMessage::MessageReceived {
            channel: GERAL,
            id: MessageId(12),
            author: PersonId(3),
            author_nickname: "rafael".into(),
            at_seconds: 0,
            body: "também".into(),
            replies_to: Some(MessageId(9)),
            client_message_id: None,
            attachment: None,
        },
    );
    let json = serde_json::to_value(conexao.messages()).unwrap();
    assert_eq!(json[0]["id"], 12);
    assert_eq!(json[0]["replies_to"], 9);
    assert_eq!(json[0]["mentions_me"], false);
    assert_eq!(json[0]["edited"], false);
}

/// **A menção chega à casca**: a mensagem marcada, o aviso `Mentioned`, e
/// as contagens do canal no retrato — e abrir o canal as zera.
#[test]
fn a_mencao_marca_a_mensagem_avisa_e_conta_no_canal() {
    let (conexao, _fila) = conexao(13);
    conexao.shared.room.lock().unwrap().open_channel(GERAL);
    let anota = ouvindo(&conexao);

    receber(&conexao.shared, &dita(1, 3, AVISOS, "sem chamar ninguém"));
    receber(&conexao.shared, &dita(2, 3, AVISOS, "@Marcela, olha isto"));
    let eventos = ouvidos(&anota);
    assert_eq!(
        eventos
            .iter()
            .filter(|e| matches!(
                e,
                Event::NoticeRaised { notice } if notice.reason == NoticeReason::Mentioned
            ))
            .count(),
        1,
        "o aviso de menção não saiu uma vez: {eventos:?}"
    );
    assert!(
        eventos.contains(&Event::ChannelsChanged),
        "a contagem do canal mudou e a lista de canais não foi avisada"
    );

    let canais = serde_json::to_value(conexao.snapshot().channels).unwrap();
    assert_eq!(
        canais,
        serde_json::json!([
            { "id": 1, "name": "geral", "open": true, "nao_lidas": 0, "mencoes": 0 },
            { "id": 2, "name": "avisos", "open": false, "nao_lidas": 2, "mencoes": 1 },
        ])
    );

    conexao.shared.room.lock().unwrap().open_channel(AVISOS);
    let mensagens = conexao.messages();
    assert_eq!(
        mensagens.iter().map(|m| m.mentions_me).collect::<Vec<_>>(),
        vec![false, true]
    );
    assert!(conexao
        .snapshot()
        .channels
        .iter()
        .all(|canal| canal.nao_lidas == 0 && canal.mencoes == 0));
    // A regra pura que a casca usa no autocompletar é a mesma.
    assert!(menciona("@Marcela, olha isto", "marcela", Some("K7Q2")));
    assert!(!menciona("@marcelinha", "marcela", None));
}

/// **A lista de banidos chega à casca**: em partes, com o evento só quando
/// fecha, no JSON que a tela lê — e quem volta a ser membro sai dela.
#[test]
fn os_banidos_chegam_a_casca_pelo_fold() {
    let (conexao, _fila) = conexao(13);
    let anota = ouvindo(&conexao);
    let banido = |id: u64, apelido: &str| seele_core::Banido {
        person: PersonId(id),
        nickname: apelido.into(),
        reason: Some("spam".into()),
        expires_at: Some(1_800_000_000),
        issued_by: Some(PersonId(7)),
    };
    receber(
        &conexao.shared,
        &ServerMessage::Banidos {
            banidos: vec![banido(3, "kaworu#A001")],
            ultima: false,
        },
    );
    assert!(conexao.banidos().is_empty(), "uma parte sem `ultima` valeu");
    assert!(!ouvidos(&anota).contains(&Event::BanidosChanged));
    receber(
        &conexao.shared,
        &ServerMessage::Banidos {
            banidos: vec![banido(4, "gendo#B002")],
            ultima: true,
        },
    );
    assert!(ouvidos(&anota).contains(&Event::BanidosChanged));
    assert_eq!(
        serde_json::to_value(conexao.banidos()).unwrap(),
        serde_json::json!([
            { "person": 3, "nickname": "kaworu#A001", "reason": "spam",
              "expires_at": 1_800_000_000_i64, "issued_by": 7 },
            { "person": 4, "nickname": "gendo#B002", "reason": "spam",
              "expires_at": 1_800_000_000_i64, "issued_by": 7 },
        ])
    );

    receber(
        &conexao.shared,
        &ServerMessage::MembroNovo {
            membro: seele_core::Membro {
                id: PersonId(3),
                nickname: "kaworu#A001".into(),
            },
        },
    );
    assert!(ouvidos(&anota).contains(&Event::BanidosChanged));
    assert_eq!(
        conexao
            .banidos()
            .iter()
            .map(|b| b.person)
            .collect::<Vec<_>>(),
        vec![4]
    );
}
