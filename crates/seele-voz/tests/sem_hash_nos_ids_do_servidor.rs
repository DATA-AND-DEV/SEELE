//! **Nenhum mapa de hash no `seele-voz`.**
//!
//! No navegador (`wasm32-unknown-unknown`) o `RandomState` não tem de onde tirar
//! aleatoriedade, e as chaves do hash são fixas (medido em 09/10/2026 em
//! `spikes/snapshot-no-wasm/`: o mesmo hash em duas instâncias do módulo e no
//! Chromium e no Safari). O mixer guarda os ganhos por SSRC, que o servidor dá, e com
//! chaves fixas um servidor hostil poderia mandar SSRCs que colidem e deixar a
//! aba quadrática. Por isso os mapas daqui são `BTreeMap`, e este guarda segura
//! que um `HashMap` ou um `HashSet` novo não entre sem alguém ler o porquê.

#![allow(clippy::expect_used)]

/// O código de um arquivo Rust, sem os comentários de linha.
fn codigo(texto: &str) -> String {
    texto
        .lines()
        .map(|linha| linha.split_once("//").map_or(linha, |(antes, _)| antes))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn nenhum_mapa_de_hash_no_crate() {
    let fontes = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut lidos = 0;
    for entrada in std::fs::read_dir(&fontes).expect("src/") {
        let caminho = entrada.expect("uma entrada de src/").path();
        if caminho.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        lidos += 1;
        let texto = codigo(&std::fs::read_to_string(&caminho).expect("o arquivo"));
        for proibido in ["HashMap", "HashSet", "RandomState"] {
            assert!(
                !texto.contains(proibido),
                "{} usa `{proibido}`: no navegador as chaves do hash são fixas, e estes \
                 mapas são indexados por ids que o servidor escolhe. Use `BTreeMap` ou \
                 `BTreeSet` (ver o comentário no topo do `mixer.rs`)",
                caminho.display()
            );
        }
    }
    assert!(
        lidos >= 3,
        "o guarda leu {lidos} arquivos de src/: a leitura quebrou"
    );
}
