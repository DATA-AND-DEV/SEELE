//! Recorta do `seele-core` o texto que o navegador precisaria, sem copiá-lo à
//! mão: o `state.rs` sem o `teto_de_video` (que puxa o vídeo) e sem os testes, e
//! o `SessionInfo` do `client.rs` (que puxa o quinn e o tokio). Cada corte tem
//! de casar uma vez só, ou o build para dizendo qual.

use std::path::PathBuf;

fn um(texto: &str, agulha: &str, de: &str) -> usize {
    let achados: Vec<usize> = texto.match_indices(agulha).map(|(i, _)| i).collect();
    assert_eq!(achados.len(), 1, "o corte «{agulha}» casou {} vezes em {de}", achados.len());
    achados[0]
}

fn main() {
    let core = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/seele-core/src");
    let saida = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    for arquivo in ["state.rs", "client.rs", "assento.rs"] {
        println!("cargo::rerun-if-changed={}", core.join(arquivo).display());
    }

    let estado = std::fs::read_to_string(core.join("state.rs")).unwrap();
    // Sem os testes, que usam o resto do `seele-core`.
    let fim = um(&estado, "\n#[cfg(test)]\nmod tests {", "state.rs");
    let estado = &estado[..fim];
    // Sem o `teto_de_video`: do comentário dele ao fim da função.
    let inicio = um(estado, "    /// O teto do vídeo com o que o fio já contou", "state.rs");
    let corpo = um(estado, "    pub fn teto_de_video(", "state.rs");
    let fecha = corpo + estado[corpo..].find("\n    }\n").expect("o fim do `teto_de_video`") + "\n    }\n".len();
    let estado = format!("{}{}", &estado[..inicio], &estado[fecha..]);
    // `include!` não aceita os `//!` do módulo.
    let estado = estado.replace("\n//!", "\n//").replacen("//!", "//", 1);
    std::fs::write(saida.join("state.rs"), estado).unwrap();

    let cliente = std::fs::read_to_string(core.join("client.rs")).unwrap();
    let inicio = um(&cliente, "/// Everything the server told us on the way in.", "client.rs");
    let fecha = inicio + cliente[inicio..].find("\n}\n").expect("o fim do `SessionInfo`") + "\n}\n".len();
    std::fs::write(saida.join("sessao.rs"), &cliente[inicio..fecha]).unwrap();
}
