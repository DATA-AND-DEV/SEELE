//! Os dois produtores de **recusa de entrada**: o desktop
//! (`classify_connect_failure` da FFI) e o navegador (`seele_web::ligacao`).
//! Tarefa 2.6 da F4.
//!
//! A casca diz uma frase por recusa (`frases.js`), lendo o JSON do
//! `ConnectionError`. Se o navegador montasse o JSON por conta própria, uma
//! variante nova, um campo renomeado ou uma regra do `OutraLinha` trocada
//! dariam ao celular a frase errada — ou nenhuma — sem que nada reprovasse.
//! Aqui a ligação do navegador recebe, pelo fio, o que faria o desktop
//! recusar, e os dois JSONs têm de ser o mesmo.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use seele_core::ConnectError;
use seele_proto::control::{encode_em, DisconnectReason, ServerMessage, MAX_NICKNAME_LEN};
use seele_proto::mods::ModAnunciado;
use seele_proto::quadro::enquadrar;
use seele_proto::version::PROTOCOL_VERSION;
use seele_web::ligacao::{Ligacao, Passo, Pedido};

fn pedido(apelido: &str) -> Pedido {
    Pedido {
        sala_pessoal: false,
        apelido: apelido.into(),
        chave_publica: [5; 32],
        impressao: [7; 32],
        // A impressão DTLS da página nesta conexão (D5). A recusa que se
        // compara aqui vem antes da prova, e não depende dela.
        impressao_dtls: [8; 32],
        convite: None,
        aceito: None,
    }
}

fn do_servidor(mensagem: &ServerMessage) -> Vec<u8> {
    enquadrar(&encode_em(PROTOCOL_VERSION, mensagem).unwrap()).unwrap()
}

/// A recusa que a ligação deu, ou o pânico que diz o que ela deu.
fn recusada(passos: Vec<Passo>) -> serde_json::Value {
    match passos.as_slice() {
        [Passo::Recusada(motivo)] => motivo.clone(),
        outro => panic!("a ligação não recusou: {outro:?}"),
    }
}

/// O desktop, para o mesmo erro.
fn do_desktop(erro: &ConnectError) -> serde_json::Value {
    serde_json::to_value(seele_ffi::sem_rede::recusa(erro)).unwrap()
}

/// Uma ligação depois da prova, esperando a sessão.
fn depois_da_prova() -> Ligacao {
    let mut ligacao = Ligacao::abrir(pedido("marcela")).unwrap();
    let passos = ligacao.chegou(&do_servidor(&ServerMessage::Challenge {
        nonce: vec![3; 32],
    }));
    assert!(matches!(passos.as_slice(), [Passo::Assinar(_)]));
    assert!(ligacao.assinado(&[9; 64]).is_empty());
    ligacao
}

fn todas_as_razoes() -> Vec<DisconnectReason> {
    vec![
        DisconnectReason::Incompatible,
        DisconnectReason::CredentialRejected,
        DisconnectReason::HandshakeTimeout,
        DisconnectReason::Kicked,
        DisconnectReason::Banned,
        DisconnectReason::ServerFull,
        DisconnectReason::ScheduledMaintenance,
        DisconnectReason::ServerShuttingDown,
        DisconnectReason::Timeout,
        DisconnectReason::ProtocolViolation,
        DisconnectReason::RateLimited,
        DisconnectReason::FellBehind,
        DisconnectReason::AdmissionPending,
        DisconnectReason::AdmissionDenied,
        DisconnectReason::NicknameTaken,
        DisconnectReason::ModsRecusados,
        DisconnectReason::ModsMudaram,
        DisconnectReason::ModsIndisponiveis,
        DisconnectReason::ChamadaEncerrada,
        DisconnectReason::NinguemAtendeu,
        DisconnectReason::ChamadaRecusada,
        DisconnectReason::ModsDeOutraApi { apis: 1 << 7 },
    ]
}

#[test]
fn a_recusa_do_servidor_e_a_mesma_antes_e_depois_da_prova() {
    for razao in todas_as_razoes() {
        let esperada = do_desktop(&ConnectError::Refused { reason: razao });

        let mut no_desafio = Ligacao::abrir(pedido("marcela")).unwrap();
        let antes = recusada(
            no_desafio.chegou(&do_servidor(&ServerMessage::Disconnecting {
                reason: razao,
            })),
        );
        assert_eq!(antes, esperada, "{razao:?} no lugar do desafio");

        let depois = recusada(depois_da_prova().chegou(&do_servidor(
            &ServerMessage::Disconnecting { reason: razao },
        )));
        assert_eq!(depois, esperada, "{razao:?} no lugar da sessão");
    }
}

#[test]
fn outra_linha_diz_a_mesma_versao_e_o_mesmo_mais_nova() {
    // Os números de antes da 1.0, o da linha 1 (9), que desde a 0.17 não é
    // este, e os depois da linha 2 (10, o de agora, fica de fora: ele entra).
    for numero in (0..=9).chain([11, 200, 255]) {
        let mut ligacao = Ligacao::abrir(pedido("marcela")).unwrap();
        let navegador = recusada(ligacao.chegou(&enquadrar(&[numero, 0, 0]).unwrap()));
        assert_eq!(
            navegador,
            do_desktop(&ConnectError::OutraLinha { numero }),
            "o carimbo {numero}"
        );
    }
}

#[test]
fn os_mods_nao_aceitos_levam_a_mesma_lista() {
    let mut mods = vec![
        ModAnunciado {
            id: "seele/enquete".into(),
            version: "1.2.0".into(),
            hash: "a".repeat(64),
            repo: "https://example.invalid/enquete".into(),
            reach: vec!["chat".into()],
            no_servidor: true,
            api: 6,
            tema: false,
        },
        ModAnunciado {
            id: "seele/estilo".into(),
            version: "0.3.1".into(),
            hash: "b".repeat(64),
            repo: "https://example.invalid/estilo".into(),
            reach: Vec::new(),
            no_servidor: false,
            api: 6,
            tema: true,
        },
    ];
    let conjunto = seele_proto::mods::hex(&seele_proto::mods::identidade_do_conjunto(&mut mods));
    let navegador = recusada(depois_da_prova().chegou(&do_servidor(
        &ServerMessage::ModsExigidos {
            mods: mods.clone(),
            conjunto: conjunto.clone(),
        },
    )));
    assert_eq!(
        navegador,
        do_desktop(&ConnectError::ModsNaoAceitos { mods, conjunto })
    );
}

#[test]
fn o_que_nao_e_do_aperto_e_violacao_e_o_fluxo_que_acaba_e_inalcancavel() {
    let violacao = do_desktop(&ConnectError::ProtocolViolation);
    let mut no_desafio = Ligacao::abrir(pedido("marcela")).unwrap();
    assert_eq!(
        recusada(
            no_desafio.chegou(&do_servidor(&ServerMessage::ServerRenamed {
                name: "x".into()
            }))
        ),
        violacao
    );
    assert_eq!(
        recusada(
            depois_da_prova().chegou(&do_servidor(&ServerMessage::ServerRenamed {
                name: "x".into()
            }))
        ),
        violacao
    );

    let inalcancavel = do_desktop(&ConnectError::Unreachable);
    assert_eq!(
        recusada(Ligacao::abrir(pedido("marcela")).unwrap().fluxo_acabou()),
        inalcancavel
    );
    assert_eq!(recusada(depois_da_prova().fluxo_acabou()), inalcancavel);
}

#[test]
fn o_apelido_grande_demais_e_o_do_desktop() {
    let navegador = Ligacao::abrir(pedido(&"x".repeat(MAX_NICKNAME_LEN + 1))).unwrap_err();
    let desktop = serde_json::to_value(seele_ffi::ConnectionError::ApelidoGrandeDemais {
        limite_bytes: MAX_NICKNAME_LEN as u64,
    })
    .unwrap();
    assert_eq!(navegador, desktop);
    // E o teto é o mesmo nos dois: um byte a menos passa.
    assert!(Ligacao::abrir(pedido(&"x".repeat(MAX_NICKNAME_LEN))).is_ok());
}
