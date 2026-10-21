//! Os dois produtores da **prévia de um anexo**: o desktop (`preview_of` da
//! FFI) e o navegador (`seele_web::previa`). Tarefa 2.6 da F4.
//!
//! A casca desenha a imagem só quando o tipo alegado é de imagem e os bytes
//! concordam, e diz o motivo do resto. As duas pontas julgam pelo mesmo juiz
//! (`seele_sessao::preview`), e aqui o JSON de cada caso tem de ser o mesmo.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use seele_core::enlace::Previa;
use seele_web::previa::{previa_do_anexo, Chegada};

const PNG: &[u8] =
    b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00";
const JPEG: &[u8] = b"\xff\xd8\xff\xe0\x00\x10JFIF\x00";

#[test]
fn cada_caso_da_previa_e_o_mesmo_nas_duas_pontas() {
    let casos: Vec<(&str, Option<Previa>, Chegada<'_>)> = vec![
        ("image/png", None, Chegada::NaoVeio),
        ("image/png", Some(Previa::NaoVeio), Chegada::NaoVeio),
        (
            "image/png",
            Some(Previa::GrandeDemais { tamanho: 9_000_000 }),
            Chegada::GrandeDemais,
        ),
        (
            "image/png",
            Some(Previa::Bytes(PNG.to_vec())),
            Chegada::Bytes(PNG),
        ),
        (
            "IMAGE/PNG",
            Some(Previa::Bytes(PNG.to_vec())),
            Chegada::Bytes(PNG),
        ),
        (
            "image/jpeg",
            Some(Previa::Bytes(PNG.to_vec())),
            Chegada::Bytes(PNG),
        ),
        (
            "image/png",
            Some(Previa::Bytes(JPEG.to_vec())),
            Chegada::Bytes(JPEG),
        ),
        (
            "image/png",
            Some(Previa::Bytes(b"texto".to_vec())),
            Chegada::Bytes(b"texto"),
        ),
        (
            "application/pdf",
            Some(Previa::Bytes(PNG.to_vec())),
            Chegada::Bytes(PNG),
        ),
        ("", Some(Previa::Bytes(PNG.to_vec())), Chegada::Bytes(PNG)),
    ];
    let mut motivos = std::collections::BTreeSet::new();
    for (alegado, do_desktop, do_navegador) in casos {
        let desktop =
            serde_json::to_value(seele_ffi::sem_rede::previa(41, alegado, do_desktop)).unwrap();
        let navegador = previa_do_anexo(41, alegado, do_navegador);
        assert_eq!(navegador, desktop, "o caso {alegado:?} / {do_navegador:?}");
        motivos.insert(
            navegador["refusal"]["kind"]
                .as_str()
                .unwrap_or("desenha")
                .to_owned(),
        );
    }
    // Os casos passam por cada desfecho: dois produtores mudos seriam iguais.
    assert_eq!(
        motivos.into_iter().collect::<Vec<_>>(),
        [
            "DidNotArrive",
            "Disagrees",
            "NotAPicture",
            "TooBig",
            "desenha"
        ]
    );
}
