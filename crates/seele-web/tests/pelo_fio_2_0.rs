//! A ponte do celular contra os bytes-ouro da linha 2 (a 0.17).
//!
//! A casca fala JSON e o fio fala `postcard`. Uma ida e volta pelo próprio
//! codec não pegaria nada: os dois lados mudariam juntos. O que pega é comparar
//! com os bytes que a linha 2 escreve em `seele-proto/tests/fio_2_0.txt`, gerados de
//! exemplares escritos à mão.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "num teste, o pânico é o relatório"
)]

use std::path::PathBuf;

use seele_proto::control::{
    decode, encode_em, ClientMessage, ControlError, ListaDeCima, ServerMessage,
};
use seele_web::{codificar_do_cliente, decodificar_do_servidor, DoServidor, ErroDaPonte};

/// As linhas `nome<TAB>hex` do arquivo da F1.
fn ouro(prefixo: &str) -> Vec<(String, Vec<u8>)> {
    let caminho =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../seele-proto/tests/fio_2_0.txt");
    let texto = std::fs::read_to_string(&caminho).expect("os bytes-ouro da F1");
    texto
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('\t'))
        .filter(|(nome, _)| nome.starts_with(prefixo))
        .map(|(nome, hex)| {
            let bytes = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).expect("hex"))
                .collect();
            (nome.to_owned(), bytes)
        })
        .collect()
}

/// Quantas mensagens diferentes os exemplares cobrem.
///
/// O arquivo da F1 pode trazer, além de um exemplar por variante, exemplares a
/// mais de uma mesma variante, com o nome dela seguido de ponto (como
/// `servidor:Alert.PedidoRecusado`): o guarda dos exemplares da F1 confere a
/// parte antes do ponto. Todos cruzam a ponte; a conta é de variantes.
fn variantes(quadros: &[(String, Vec<u8>)]) -> usize {
    quadros
        .iter()
        .map(|(nome, _)| nome.split('.').next().unwrap_or(nome))
        .collect::<std::collections::BTreeSet<_>>()
        .len()
}

#[test]
fn cada_mensagem_do_cliente_sai_pela_ponte_com_os_bytes_do_fio() {
    let quadros = ouro("cliente:");
    assert_eq!(
        variantes(&quadros),
        ClientMessage::CONHECIDAS as usize,
        "o arquivo devia trazer um exemplar de cada mensagem do cliente"
    );
    for (nome, quadro) in quadros {
        // O JSON é o que a casca mandaria: o `serde_json` da mensagem que o
        // fio carrega, e não algo que este crate produziu.
        let mensagem: ClientMessage = decode(&quadro).unwrap_or_else(|e| panic!("{nome}: {e:?}"));
        let json = serde_json::to_string(&mensagem).expect("json");
        let saiu =
            codificar_do_cliente(quadro[0], &json).unwrap_or_else(|e| panic!("{nome}: {e:?}"));
        assert_eq!(saiu, quadro, "{nome} saiu diferente do fio pela ponte");
    }
}

#[test]
fn cada_mensagem_do_servidor_chega_pela_ponte_sem_perder_nada() {
    let quadros = ouro("servidor:");
    assert_eq!(
        variantes(&quadros),
        ServerMessage::CONHECIDAS as usize,
        "o arquivo devia trazer um exemplar de cada mensagem do servidor"
    );
    for (nome, quadro) in quadros {
        let Ok(DoServidor::Mensagem(json)) = decodificar_do_servidor(&quadro) else {
            panic!("{nome} não chegou como mensagem");
        };
        // A volta: o JSON que a casca recebeu tem de carregar a mensagem
        // inteira, ou ela não codifica de novo nos mesmos bytes.
        let lida: ServerMessage =
            serde_json::from_str(&json).unwrap_or_else(|e| panic!("{nome}: {e}"));
        assert_eq!(
            encode_em(quadro[0], &lida).expect("encode"),
            quadro,
            "{nome} perdeu algo no JSON"
        );
    }
}

#[test]
fn o_carimbo_que_a_ponte_recebe_vai_no_byte_zero() {
    // `LeaveVoiceRoom` é o ordinal 3, sem corpo (`cliente:LeaveVoiceRoom 0903`).
    // Depois do `Hello`, quem manda carimba com o número negociado, e não com o
    // desta build.
    assert_eq!(
        codificar_do_cliente(0x7e, "\"LeaveVoiceRoom\"").expect("codificar"),
        vec![0x7e, 0x03]
    );
}

#[test]
fn uma_mensagem_de_vocabulario_mais_novo_chega_como_desconhecida() {
    // O ordinal 10 000 em varint: 0x90 0x4e. Nenhuma build vai conhecê-lo.
    assert_eq!(
        decodificar_do_servidor(&[seele_proto::version::PROTOCOL_VERSION, 0x90, 0x4e])
            .expect("pular, e não derrubar"),
        DoServidor::Desconhecida { ordinal: 10_000 }
    );
}

#[test]
fn um_quadro_do_servidor_0_15_diz_a_versao_que_trouxe() {
    // `[8, 11, 0]` é a recusa da 0.15.1 (`Disconnecting { Incompatible }`). É
    // pelo `found` que a página diz «Servidor 0.15».
    let Err(ErroDaPonte::Fio(ControlError::UnsupportedVersion { found, .. })) =
        decodificar_do_servidor(&[8, 11, 0])
    else {
        panic!("o carimbo 8 tinha de ser recusado com a versão que trouxe");
    };
    assert_eq!(found, 8);
}

#[test]
fn um_json_torto_volta_como_erro_e_nao_como_panico() {
    assert!(matches!(
        codificar_do_cliente(9, "{"),
        Err(ErroDaPonte::Json(_))
    ));
    assert!(matches!(
        codificar_do_cliente(9, r#"{"Hello":{}}"#),
        Err(ErroDaPonte::Json(_))
    ));
}
