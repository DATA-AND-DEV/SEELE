//! **O formato do quadro de tela é escrito num lugar só: o `seele_proto::screen`.**
//!
//! Até 09/10/2026, o cabeçalho de cinco bytes, o teto, os tipos e o
//! enquadramento que conta bytes estavam escritos duas vezes — no `seele-core` e
//! no `seele-server` —, com um comentário dizendo que a cópia era deliberada. O
//! celular precisava de uma terceira leitura, e o teto subiu de 512 KiB para
//! 4 MiB (D-fio-9): três cópias que concordam por escrito são três lugares para
//! uma delas ficar para trás. A P13 do plano F1b trocou as duas cópias pelo
//! módulo do `seele-proto`.
//!
//! Este guarda lê a fonte de todo crate e app do workspace, menos o
//! `seele-proto`, e reprova se alguém voltar a **declarar** uma dessas peças.
//! Reexportar (`pub use seele_proto::screen::…`) não é declarar, e passa.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::path::{Path, PathBuf};

/// O que só o `seele-proto` declara.
const SO_NO_PROTO: &[&str] = &[
    "const CABECALHO_DE_QUADRO_LEN",
    "const MAX_QUADRO_LEN",
    "const TIPO_CHAVE",
    "const TIPO_SOM",
    "enum TipoDeQuadro",
    "struct Enquadramento",
];

fn arquivos_rs(pasta: &Path, achados: &mut Vec<PathBuf>) {
    let Ok(entradas) = std::fs::read_dir(pasta) else {
        return;
    };
    for entrada in entradas.flatten() {
        let caminho = entrada.path();
        if caminho.is_dir() {
            arquivos_rs(&caminho, achados);
        } else if caminho.extension().is_some_and(|ext| ext == "rs") {
            achados.push(caminho);
        }
    }
}

#[test]
fn ninguem_alem_do_proto_declara_o_formato_do_quadro_de_tela() {
    let raiz = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut fontes = Vec::new();
    for grupo in ["crates", "apps"] {
        let Ok(pacotes) = std::fs::read_dir(raiz.join(grupo)) else {
            continue;
        };
        for pacote in pacotes.flatten() {
            if pacote.file_name() == "seele-proto" {
                continue;
            }
            arquivos_rs(&pacote.path().join("src"), &mut fontes);
        }
    }
    assert!(
        fontes.len() > 50,
        "o guarda leu só {} arquivos: o caminho do workspace mudou, e ele não \
         confere mais nada",
        fontes.len()
    );

    let mut copias = Vec::new();
    for arquivo in &fontes {
        let texto = std::fs::read_to_string(arquivo).unwrap();
        for (numero, linha) in texto.lines().enumerate() {
            let codigo = linha.trim_start();
            if codigo.starts_with("//") {
                continue;
            }
            for peca in SO_NO_PROTO {
                if codigo.contains(peca) {
                    copias.push(format!("{}:{}: {}", arquivo.display(), numero + 1, codigo));
                }
            }
        }
    }
    assert!(
        copias.is_empty(),
        "o formato do quadro de tela voltou a ser declarado fora do \
         `seele_proto::screen` — use-o de lá: {copias:#?}"
    );
}
