//! Os vetores que o indexador de MODs usa para provar que a
//! reimplementação em Python de `content_hash` não divergiu desta.
//!
//! O teste **escreve** o arquivo e reprova se o comitado divergir. É essa
//! ordem que impede alguém de editar um hash até o teste passar: quem os
//! produz é esta função, e mexer neles sem mexer nela reprova aqui.
//!
//! O arquivo é comitado nos dois repositórios, e nenhum precisa do outro
//! para rodar os próprios testes.

#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use seele_proto::mods::content_hash;
use std::path::PathBuf;

fn hex(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn arquivo(caminho: &str, bytes: &[u8]) -> (String, Vec<u8>) {
    (caminho.to_owned(), bytes.to_vec())
}

/// Um caso: o nome que ele tem no arquivo e os arquivos que entram no digestor.
type Caso = (&'static str, Vec<(String, Vec<u8>)>);

/// Os casos, e por que cada um está aqui.
fn casos() -> Vec<Caso> {
    vec![
        // O conjunto vazio: só a contagem entra no digestor.
        ("vazio", vec![]),
        ("um-arquivo", vec![arquivo("mod.json", br#"{"schema":1}"#)]),
        // O par que prova que os prefixos de comprimento funcionam: sem
        // eles, estes dois teriam o mesmo hash.
        ("fronteira-ab-c", vec![arquivo("ab", b"c")]),
        ("fronteira-a-bc", vec![arquivo("a", b"bc")]),
        // O caso que o Rust acerta de graça e o Python erra fácil:
        // `path.len()` aqui é bytes, e `len(str)` lá é caracteres.
        ("utf8-no-caminho", vec![arquivo("ícone/ação.js", b"x")]),
        // A ordem de entrada não pode importar: a função ordena.
        (
            "ordem-invertida",
            vec![arquivo("z.js", b"Z"), arquivo("a.js", b"A")],
        ),
    ]
}

fn montar_json() -> String {
    use std::fmt::Write as _;
    let mut texto =
        String::from("{\n  \"gerado_por\": \"seele-proto content_hash\",\n  \"vetores\": [\n");
    let todos = casos();
    for (indice, (nome, arquivos)) in todos.iter().enumerate() {
        let mut copia = arquivos.clone();
        let digest = hex(&content_hash(&mut copia));
        let listados: Vec<String> = arquivos
            .iter()
            .map(|(caminho, bytes)| {
                format!(
                    "{{ \"caminho\": {caminho:?}, \"bytes_b64\": \"{}\" }}",
                    base64(bytes)
                )
            })
            .collect();
        let virgula = if indice + 1 == todos.len() { "" } else { "," };
        let _ = writeln!(
            texto,
            "    {{ \"nome\": {nome:?}, \"arquivos\": [{}], \"hash\": \"{digest}\" }}{virgula}",
            listados.join(", ")
        );
    }
    texto.push_str("  ]\n}\n");
    texto
}

/// Base64 padrão, escrito à mão porque `seele-proto` não depende de nada
/// (`specs/01-arquitetura.md`) e não vai passar a depender por um teste.
fn base64(bytes: &[u8]) -> String {
    const TABELA: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut saida = String::new();
    for bloco in bytes.chunks(3) {
        let b = [
            bloco[0],
            *bloco.get(1).unwrap_or(&0),
            *bloco.get(2).unwrap_or(&0),
        ];
        let junto = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for deslocamento in [18, 12, 6, 0] {
            saida.push(TABELA[((junto >> deslocamento) & 0x3F) as usize] as char);
        }
        let sobra = 3 - bloco.len();
        saida.truncate(saida.len() - sobra);
        saida.push_str(&"=".repeat(sobra));
    }
    saida
}

fn caminho_do_arquivo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("vetores-de-hash.json")
}

#[test]
fn os_vetores_comitados_batem_com_esta_implementacao() {
    let esperado = montar_json();
    let caminho = caminho_do_arquivo();

    let comitado = std::fs::read_to_string(&caminho).unwrap_or_default();
    if comitado != esperado {
        std::fs::write(&caminho, &esperado).expect("escrever os vetores");
        panic!(
            "vetores-de-hash.json estava desatualizado e foi reescrito em {}.\n\
             Comite o novo arquivo AQUI e no SEELE-MODS-INDEXER, e rode os \
             testes do indexador: se `hash_conteudo.py` divergir, ele reprova.",
            caminho.display()
        );
    }
}
