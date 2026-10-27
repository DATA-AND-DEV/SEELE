//! **O registro da linha 1**: os bytes-ouro do Servidor 1.0, congelados no
//! que a v0.16.0 publicou, e que nada nesta árvore gera mais.
//!
//! # Por que um registro, e não um guarda que gera
//!
//! Até a 0.16 este arquivo gerava cada quadro da linha 1 com o código da
//! própria árvore e comparava com `tests/fio_1_0.txt` (ADR 0058, «1.x nunca
//! quebra o fio»). A 0.17 é a linha 2 (ADR 0063): o número do fio é 10, o
//! transporte é outro, e o código desta árvore deixou de escrever o 9. Gerar
//! os quadros daqui daria bytes da linha 2 comparados com os da linha 1 — um
//! guarda que reprova sempre, ou que alguém «conserta» regerando o registro.
//!
//! O que fica:
//!
//! - **o arquivo, inteiro e congelado**, com a impressão SHA-256 presa aqui. É
//!   o que um 0.16.x precisa ler do mesmo jeito, e o que uma investigação de
//!   campo sobre a 0.16 consulta sem precisar do commit dela;
//! - **a forma**: o cabeçalho `# estado: congelado`, e todo quadro de controle
//!   da linha 1 carimbado com o 9.
//!
//! O guarda que gera mora em `tests/fio_2_0.rs`, contra `tests/fio_2_0.txt`.
//!
//! **O cabeçalho só passou a `congelado` em 24/10/2026**, um dia depois de a
//! v0.16.0 sair do `8faa3379b` ainda em `rascunho`. Os bytes são os daquele
//! commit: a troca do cabeçalho foi a única mudança, e o guarda que gerava
//! passou com ela antes de o número subir para 10.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::path::PathBuf;

use sha2::{Digest, Sha256};

/// Onde o registro mora, a partir da raiz do crate.
const ARQUIVO: &str = "tests/fio_1_0.txt";

/// O SHA-256 do registro, em hexadecimal minúsculo.
const IMPRESSAO_DO_REGISTRO: &str =
    "7a9c1740384e49c929256865c497debdc5ba2a5edee9d4a44934ee9091fe4cf8";

fn registro() -> String {
    let caminho = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ARQUIVO);
    std::fs::read_to_string(caminho).expect("o registro da linha 1 sumiu")
}

#[test]
fn o_registro_da_linha_1_nao_muda() {
    let texto = registro();
    let impressao: String = Sha256::digest(texto.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(
        impressao, IMPRESSAO_DO_REGISTRO,
        "{ARQUIVO} mudou. Ele é o registro congelado da linha 1 (ADR 0058, ADR \
         0063): nada desta árvore o regera. Se a mudança é a linha 2, ela vai \
         em tests/fio_2_0.txt"
    );
}

#[test]
fn o_registro_da_linha_1_esta_congelado_e_fala_o_9() {
    let texto = registro();
    assert_eq!(texto.lines().next(), Some("# estado: congelado"));
    let quadros: Vec<(&str, &str)> = texto
        .lines()
        .filter(|linha| !linha.starts_with('#'))
        .map(|linha| linha.split_once('\t').expect("nome<TAB>hex"))
        .collect();
    assert!(quadros.len() > 100, "o registro perdeu quadros");
    for (nome, bytes) in &quadros {
        assert!(
            bytes.len() % 2 == 0 && bytes.bytes().all(|c| c.is_ascii_hexdigit()),
            "{nome} não é hexadecimal"
        );
        if nome.starts_with("cliente:") || nome.starts_with("servidor:") {
            assert!(
                bytes.starts_with("09"),
                "{nome} não está carimbado com o 9 da linha 1"
            );
        }
    }
    assert!(quadros.contains(&("recusa:9", "090b00")));
}
