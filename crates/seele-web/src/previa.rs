//! **A prévia de um anexo**, como a casca a lê (Tarefa 2.6 da F4): o
//! `preview_of` da FFI, sobre o mesmo juiz (`seele_sessao::preview`).
//!
//! A imagem só é desenhada quando o tipo que quem mandou alegou é um dos que a
//! casca desenha **e** os bytes concordam com ele; o resto volta com o motivo.
//! A conformidade dos dois produtores (`dois_produtores_da_previa.rs`) confere
//! o JSON contra o da FFI, motivo a motivo.

use seele_proto::ids::AttachmentId;
use seele_sessao::preview::{data_uri, judge, Verdict, PREVIEW_LIMIT};
use seele_sessao::state::Room;
use serde_json::{json, Value};

/// O que veio do fluxo do anexo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chegada<'a> {
    /// O fluxo não veio, ou caiu, ou o hash não fechou.
    NaoVeio,
    /// O anexo passa do teto da prévia: nem foi lido.
    GrandeDemais,
    /// Os bytes, inteiros.
    Bytes(&'a [u8]),
}

/// O `Preview` da FFI, em JSON: `{attachment, image, claimed, found, refusal}`.
#[must_use]
pub fn previa_do_anexo(attachment: u64, claimed: &str, chegada: Chegada<'_>) -> Value {
    let recusada = |refusal: Value, found: Option<&str>| {
        json!({
            "attachment": attachment,
            "image": null,
            "claimed": claimed,
            "found": found,
            "refusal": refusal,
        })
    };
    match chegada {
        Chegada::NaoVeio => recusada(json!({ "kind": "DidNotArrive" }), None),
        Chegada::GrandeDemais => {
            recusada(json!({ "kind": "TooBig", "limit": PREVIEW_LIMIT }), None)
        }
        Chegada::Bytes(bytes) => match judge(claimed, bytes) {
            Verdict::Draw(formato) => json!({
                "attachment": attachment,
                "image": data_uri(formato, bytes),
                "claimed": claimed,
                "found": formato.media_type(),
                "refusal": null,
            }),
            Verdict::Disagrees { found, .. } => recusada(
                json!({ "kind": "Disagrees" }),
                found.map(|formato| formato.media_type()),
            ),
            Verdict::NotAPicture => recusada(json!({ "kind": "NotAPicture" }), None),
        },
    }
}

/// O tipo que quem mandou alegou para um anexo, procurado em todo o histórico
/// que esta sessão já viu: o `declared_type_of` da FFI. Vazio quando o anexo
/// não está em mensagem nenhuma.
#[must_use]
pub fn tipo_alegado(room: &Room, anexo: u64) -> String {
    anexo_no_historico(room, anexo)
        .map(|(_, tipo, _)| tipo)
        .unwrap_or_default()
}

/// O nome, o tipo alegado e o tamanho de um anexo do histórico.
#[must_use]
pub fn anexo_no_historico(room: &Room, anexo: u64) -> Option<(String, String, u64)> {
    let id = AttachmentId(anexo);
    room.mensagens.values().flatten().find_map(|mensagem| {
        mensagem
            .attachment
            .as_ref()
            .filter(|anexo| anexo.id == id)
            .map(|anexo| {
                (
                    anexo.file_name.clone(),
                    anexo.declared_type.clone(),
                    anexo.byte_size,
                )
            })
    })
}
