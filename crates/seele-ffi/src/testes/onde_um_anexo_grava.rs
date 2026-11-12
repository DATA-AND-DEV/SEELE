//! A pergunta que a casca faz antes de salvar, e que a gravação refaz.
//!
//! `destino_de` é o lugar só onde o nome sai do histórico e passa pela regra
//! de `seele_core::anexo_no_disco`, e onde a pasta é conferida. A frase de
//! confirmação e a gravação perguntam aqui, e por isso não discordam.

use super::*;

const LINHA_ABERTA: ChannelId = ChannelId(1);
const OUTRA_LINHA: ChannelId = ChannelId(2);

/// Um histórico com um anexo deste nome, numa linha que não é a aberta.
fn com_anexo(nome: &str) -> Arc<Shared> {
    let shared = super::tests::compartilhado_de_teste();
    let mensagem = seele_core::Message {
        id: MessageId(1),
        channel: OUTRA_LINHA,
        author: PersonId(2),
        author_nickname: "rafael".into(),
        at_seconds: 0,
        body: String::new(),
        replies_to: None,
        own: false,
        client_message_id: None,
        edited: false,
        mentions_me: false,
        attachment: Some(seele_core::AttachmentInfo {
            id: seele_core::AttachmentId(7),
            file_name: nome.to_owned(),
            declared_type: "image/png".into(),
            byte_size: 10,
            state: seele_core::AttachmentState::Available,
        }),
    };
    shared
        .room
        .lock()
        .unwrap()
        .mensagens
        .entry(OUTRA_LINHA)
        .or_default()
        .push(mensagem);
    shared
}

fn pasta() -> String {
    std::env::temp_dir().to_string_lossy().into_owned()
}

#[test]
fn um_nome_que_e_so_um_nome_grava_na_pasta_dada() {
    let shared = com_anexo("foto.png");
    let (onde, nome) = destino_de(&shared, seele_core::AttachmentId(7), &pasta())
        .expect("um nome comum numa pasta absoluta é recusado");
    assert_eq!(
        onde,
        std::env::temp_dir(),
        "a pasta mudou entre a casca e a gravação"
    );
    assert_eq!(
        nome, "foto.png",
        "o nome que sai do histórico não é o que o remetente mandou"
    );
    // E em qualquer linha, não só na aberta: o download pode terminar depois
    // de a pessoa trocar de linha.
    assert!(
        !shared
            .room
            .lock()
            .unwrap()
            .mensagens
            .contains_key(&LINHA_ABERTA),
        "o anexo deste teste devia estar fora da linha aberta"
    );
}

#[test]
fn um_nome_que_nao_e_so_um_nome_e_recusado_e_citado_por_extenso() {
    for (alegado, citado) in [
        ("../.zshrc", "../.zshrc"),
        ("foto\u{202E}gnp.exe", "foto\\u{202E}gnp.exe"),
    ] {
        let shared = com_anexo(alegado);
        let recusa = destino_de(&shared, seele_core::AttachmentId(7), &pasta())
            .expect_err("um nome com caminho ou disfarce passou para a gravação");
        assert_eq!(
            recusa,
            SaveRefused {
                reason: NotSavedReason::NomeRecusado,
                claimed: citado.to_owned(),
                folder: pasta(),
            },
            "a recusa de {alegado:?} não diz o motivo, ou cita o nome de um \
                 jeito que a própria frase não consegue mostrar"
        );
    }
}

#[test]
fn sem_pasta_absoluta_nada_grava() {
    // A janela fazia esta conferência com `pastaDeDestino === ""`. Agora a
    // casca calcula a pasta, e uma vazia ou relativa gravaria onde quer que
    // o processo tenha sido iniciado — um lugar que a frase não nomeia.
    let shared = com_anexo("foto.png");
    for folder in ["", "Downloads", "./pasta"] {
        let recusa = destino_de(&shared, seele_core::AttachmentId(7), folder)
            .expect_err("uma pasta vazia ou relativa foi aceita para gravar");
        assert_eq!(
            recusa.reason,
            NotSavedReason::SemPasta,
            "a pasta {folder:?} não foi recusada como falta de pasta"
        );
    }
}

#[test]
fn um_anexo_fora_do_historico_nao_ganha_nome_de_fora() {
    let shared = com_anexo("foto.png");
    let recusa = destino_de(&shared, seele_core::AttachmentId(8), &pasta())
        .expect_err("um anexo que o histórico não tem ganhou um nome");
    assert_eq!(
        recusa,
        SaveRefused {
            reason: NotSavedReason::AnexoDesconhecido,
            claimed: String::new(),
            folder: pasta(),
        },
        "um anexo fora do histórico não foi recusado com motivo próprio"
    );
}
