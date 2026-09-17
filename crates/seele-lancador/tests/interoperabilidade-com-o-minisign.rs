//! O formato lido aqui é o formato que o `minisign` de verdade escreve?
//!
//! Os testes de unidade deste crate assinam eles mesmos, e é o que permite
//! provar que uma assinatura errada é recusada. O que eles **não** provam é
//! que a nossa ideia do formato é a certa: se ela estivesse errada dos dois
//! lados, eles passariam e o produto recusaria todo pacote publicado.
//!
//! Este arquivo fecha esse buraco com artefatos que saíram do `minisign`
//! instalado, que é o mesmo formato do `cargo tauri signer` e o mesmo que o
//! `tauri-plugin-updater` confere. Ver `testes/LEIA-ME.md`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use seele_lancador::revogacao::Envelope;
use seele_lancador::{Chave, ListaDeRevogacao};

/// A chave pública do artefato de teste, no formato do `tauri.conf.json`.
const CHAVE: &str = include_str!("../testes/chave-publica-de-teste.base64");
/// Os bytes exatos que o `minisign` assinou.
const DOCUMENTO: &[u8] = include_bytes!("../testes/lista-de-revogacao-de-teste.json");
/// O `.sig` daquele documento, em base64.
const ASSINATURA: &str = include_str!("../testes/assinatura-da-lista-de-teste.base64");

fn envelope(documento: &[u8]) -> Envelope {
    use base64::Engine as _;
    Envelope {
        document: base64::engine::general_purpose::STANDARD.encode(documento),
        signature: ASSINATURA.trim().to_owned(),
    }
}

#[test]
fn uma_lista_assinada_pelo_minisign_de_verdade_abre_por_este_caminho() {
    let chave = Chave::do_formato_do_atualizador(CHAVE.trim())
        .expect("a chave que o `minisign -G` escreveu tem de abrir");

    let lista = ListaDeRevogacao::abrir(&envelope(DOCUMENTO), &chave)
        .expect("a assinatura que o `minisign -S` escreveu tem de conferir");

    assert_eq!(lista.emitida_em(), "2026-09-10T00:00:00Z");
    let revogacao = lista
        .revogacao_de("0.0.0-artefato-de-teste")
        .expect("o artefato de teste revoga esta versão inventada");
    assert_eq!(
        revogacao.fixed_in.as_deref(),
        Some("0.0.1-artefato-de-teste")
    );
}

/// E a interoperabilidade não é «aceita qualquer coisa».
#[test]
fn um_byte_trocado_no_documento_derruba_a_assinatura_do_minisign() {
    let chave = Chave::do_formato_do_atualizador(CHAVE.trim()).unwrap();
    let mut adulterado = DOCUMENTO.to_vec();
    if let Some(ultimo) = adulterado.last_mut() {
        *ultimo = b' ';
    }
    assert!(
        ListaDeRevogacao::abrir(&envelope(&adulterado), &chave).is_err(),
        "editar o documento depois de assinado tem de ser recusado"
    );
}
