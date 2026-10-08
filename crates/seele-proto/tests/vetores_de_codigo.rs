//! Os vetores do código da sala pessoal, para quem reimplementar a derivação
//! fora daqui — o web app, um indexador, um teste de campo — provar que não
//! divergiu (spec de 23/09/2026, «O código»: «os vetores de teste ficam
//! congelados no molde de `vetores-de-hash.json`»).
//!
//! O teste **escreve** o arquivo e reprova se o comitado divergir. É essa
//! ordem que impede alguém de editar um código até o teste passar: quem os
//! produz é `Codigo::da_chave`, e mexer neles sem mexer nela reprova aqui. E
//! como o formato é congelado para sempre, um vetor que mude de valor é um
//! código distribuído que deixou de valer: o teste reprova, e a resposta certa
//! é desfazer a mudança, e não comitar o arquivo novo.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::path::PathBuf;

use seele_proto::codigo::Codigo;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Os casos, e por que cada um está aqui.
fn casos() -> Vec<(&'static str, [u8; 32])> {
    let mut contagem = [0_u8; 32];
    for (indice, byte) in contagem.iter_mut().enumerate() {
        *byte = u8::try_from(indice).expect("32 cabe num byte");
    }
    vec![
        // Os dois extremos de bytes.
        ("zeros", [0; 32]),
        ("ff", [0xFF; 32]),
        // Uma chave sem padrão nenhum de byte repetido.
        ("uns", [1; 32]),
        ("contagem", contagem),
    ]
}

fn montar_json() -> String {
    use std::fmt::Write as _;
    let mut texto =
        String::from("{\n  \"gerado_por\": \"seele-proto Codigo::da_chave\",\n  \"vetores\": [\n");
    let todos = casos();
    for (indice, (nome, chave)) in todos.iter().enumerate() {
        let codigo = Codigo::da_chave(chave);
        let virgula = if indice + 1 == todos.len() { "" } else { "," };
        let _ = writeln!(
            texto,
            "    {{ \"nome\": {nome:?}, \"chave\": \"{}\", \"codigo\": \"{codigo}\", \"dado\": \"{}\" }}{virgula}",
            hex(chave),
            codigo.dado()
        );
    }
    texto.push_str("  ]\n}\n");
    texto
}

fn caminho_do_arquivo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("vetores-de-codigo.json")
}

#[test]
fn os_vetores_comitados_batem_com_esta_implementacao() {
    let esperado = montar_json();
    let caminho = caminho_do_arquivo();

    let comitado = std::fs::read_to_string(&caminho).unwrap_or_default();
    if comitado != esperado {
        std::fs::write(&caminho, &esperado).expect("escrever os vetores");
        panic!(
            "vetores-de-codigo.json estava diferente e foi reescrito em {}.\n\
             Se o arquivo ainda não existia, comite-o. Se ele existia, o código \
             de alguma chave mudou: isso invalida todo código já distribuído, e \
             a mudança tem de ser desfeita (spec de 23/09, «formato congelado \
             para sempre»).",
            caminho.display()
        );
    }
}
