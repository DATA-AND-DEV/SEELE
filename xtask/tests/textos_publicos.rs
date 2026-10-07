//! O que o SEELE diz em público não pode contradizer o código.
//!
//! # O defeito que este arquivo existe para não deixar voltar
//!
//! **Um programa que não existe.** O cliente de terminal saiu do repositório
//! com o ADR 0039 (`aa77ad8`), e o produto passou a ser o app e o `seeled`. O
//! README, a página de release, os docs que o README e o `install.ps1` apontam
//! e o próprio `seeled`, ao subir, continuaram mandando compilar e rodar o
//! cliente que saiu. Quem seguia a primeira linha ganhava «comando não
//! encontrado», e o `cargo build` do README falhava antes de compilar qualquer
//! coisa. Nada no repositório reprovava: um texto não compila.
//!
//! # O que fica de fora, e por quê
//!
//! Análises, planos e ADRs citam o cliente de terminal, e ficam como estão: são
//! o histórico do que se decidiu, e reescrevê-los apagaria o motivo das
//! decisões. O que este arquivo varre é o que alguém lê para instalar, subir ou
//! testar o SEELE.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::path::PathBuf;

fn raiz() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/ sempre tem pai")
        .to_owned()
}

fn ler(relativo: &str) -> String {
    std::fs::read_to_string(raiz().join(relativo)).unwrap_or_else(|falha| {
        panic!(
            "não consegui ler `{relativo}` ({falha}): um texto publicado que some sem \
             este guarda saber é um texto que ninguém mais confere"
        )
    })
}

/// O programa aposentado. O nome é o do binário que o `seele-tui` produzia.
const APOSENTADO: &str = "connection";

/// O que alguém lê para instalar, subir ou testar o SEELE.
///
/// O README, a página de release (`NOTAS-DE-RELEASE.md` sai no corpo de toda
/// release), os dois instaladores de uma linha, o `seeled`, que imprime ao
/// subir o que fazer na outra máquina, e os docs para onde o README e o
/// `install.ps1` mandam quem quer ir além.
const PUBLICADOS: &[&str] = &[
    "README.md",
    ".github/NOTAS-DE-RELEASE.md",
    "install.sh",
    "install.ps1",
    "crates/seele-server/src/main.rs",
    "docs/windows.md",
    "docs/teste-duas-maquinas.md",
    "docs/ponto-de-encontro.md",
    "docs/como-testar.md",
    "docs/alcance-pela-internet.md",
];

/// Os binários que o workspace produz, lidos do próprio Cargo.
///
/// Do `cargo metadata`, e não de um `grep` por `[[bin]]`: um pacote com
/// `src/main.rs` e sem `[[bin]]` também produz um binário, com o nome do
/// pacote, e um guarda que lesse só os `[[bin]]` diria que ele não existe.
fn binarios_do_workspace() -> Vec<String> {
    let metadata = cargo_metadata::MetadataCommand::new()
        .manifest_path(raiz().join("Cargo.toml"))
        .no_deps()
        .exec()
        .expect("o `cargo metadata` do workspace tem de responder");
    metadata
        .workspace_packages()
        .into_iter()
        .flat_map(|pacote| pacote.targets.iter())
        .filter(|alvo| alvo.is_bin())
        .map(|alvo| alvo.name.clone())
        .collect()
}

/// A parte de um arquivo que alguém lê.
///
/// Num `.rs`, o que vem antes do módulo de testes: é ali que mora o que o
/// binário imprime, e o teste que confere a saída do `seeled` precisa escrever
/// o nome aposentado para dizer que ele não pode aparecer.
fn o_que_se_le(relativo: &str, texto: &str) -> String {
    if !relativo.ends_with(".rs") {
        return texto.to_owned();
    }
    texto
        .lines()
        .take_while(|linha| linha.trim() != "#[cfg(test)]")
        .collect::<Vec<_>>()
        .join("\n")
}

/// Se `palavra` aparece em `linha` como palavra inteira.
///
/// Inteira e com a caixa exata: pega `connection --server`, `connection.exe`,
/// `{connection,seeled}`, `./target/release/connection` e o nome entre crases,
/// e deixa passar o «Universal Connection and Play» que a
/// `docs/alcance-pela-internet.md` cita da tela de um roteador, e um
/// `connections` qualquer.
fn tem_a_palavra(linha: &str, palavra: &str) -> bool {
    let parte_de_palavra = |c: char| c.is_alphanumeric() || c == '_';
    linha.match_indices(palavra).any(|(onde, _)| {
        let antes = linha
            .get(..onde)
            .and_then(|resto| resto.chars().next_back());
        let depois = linha
            .get(onde + palavra.len()..)
            .and_then(|resto| resto.chars().next());
        !antes.is_some_and(parte_de_palavra) && !depois.is_some_and(parte_de_palavra)
    })
}

#[test]
fn nenhum_texto_publicado_manda_rodar_um_programa_que_nao_existe() {
    let binarios = binarios_do_workspace();
    // Sem isto, um `cargo metadata` que voltasse vazio diria que nada existe, e
    // o guarda passaria a valer por acaso.
    assert!(
        binarios.iter().any(|nome| nome == "seeled"),
        "o `cargo metadata` não achou o `seeled` entre os binários ({binarios:?}): a \
         leitura do workspace quebrou, e este guarda não sabe mais o que existe"
    );
    if binarios.iter().any(|nome| nome == APOSENTADO) {
        // Se ele voltar a existir, os textos voltam a poder citá-lo.
        return;
    }

    let mut achados = Vec::new();
    for relativo in PUBLICADOS {
        let texto = ler(relativo);
        for (numero, linha) in o_que_se_le(relativo, &texto).lines().enumerate() {
            if tem_a_palavra(linha, APOSENTADO) {
                achados.push(format!("  {relativo}:{}: {}", numero + 1, linha.trim()));
            }
        }
    }
    assert!(
        achados.is_empty(),
        "estes textos mandam compilar ou rodar o `{APOSENTADO}`, e nenhum pacote do \
         workspace produz esse binário (saiu com a TUI, ADR 0039). Quem seguir a \
         linha ganha «comando não encontrado», ou um `cargo build` que falha antes \
         de compilar. O cliente é o app, e o que se cola nele é o link que o \
         `seeled` imprime:\n{}",
        achados.join("\n")
    );
}
