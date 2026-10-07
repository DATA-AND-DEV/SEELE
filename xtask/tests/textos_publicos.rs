//! O que o SEELE diz em público não pode contradizer o código.
//!
//! # Os dois defeitos que este arquivo existe para não deixar voltar
//!
//! **Um programa que não existe.** O cliente de terminal saiu do repositório
//! com o ADR 0039 (`aa77ad8`), e o produto passou a ser o app e o `seeled`. O
//! README, a página de release, os docs que o README e o `install.ps1` apontam
//! e o próprio `seeled`, ao subir, continuaram mandando compilar e rodar o
//! cliente que saiu. Quem seguia a primeira linha ganhava «comando não
//! encontrado», e o `cargo build` do README falhava antes de compilar qualquer
//! coisa. Nada no repositório reprovava: um texto não compila.
//!
//! **Uma promessa de privacidade que o produto não cumpre.** A `specs/01`
//! dizia que o servidor nunca vê áudio em claro, e o README, que o TLS é
//! «ponta a ponta». O TLS vai de cada pessoa até o servidor de quem hospeda, e
//! ali a voz e o texto chegam em claro: a `specs/08` diz isso e manda escrever
//! (o operador do servidor lendo o áudio está fora de escopo na v1). Uma frase
//! que promete mais do que o produto entrega é a frase em que alguém confia.
//!
//! # O que fica de fora, e por quê
//!
//! Análises, planos e ADRs citam o cliente de terminal e o «ponta a ponta» de
//! antes, e ficam como estão: são o histórico do que se decidiu, e reescrevê-los
//! apagaria o motivo das decisões. O que este arquivo varre é o que alguém lê
//! para instalar, subir, testar ou confiar no SEELE.

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

/// O texto em minúsculas e com todo espaço, quebra de linha inclusive, virado
/// um espaço só.
///
/// Um parágrafo de Markdown é quebrado onde o editor quiser, e uma frase
/// partida em duas linhas continua sendo a mesma frase para quem lê.
fn normalizado(texto: &str) -> String {
    texto
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn nenhum_texto_promete_que_quem_hospeda_nao_ouve() {
    const PROMESSA: &str = "nunca vê áudio em claro";

    let mut lidos = vec![("README.md".to_owned(), ler("README.md"))];
    let pasta = raiz().join("specs");
    let mut specs: Vec<_> = std::fs::read_dir(&pasta)
        .expect("specs/ tem de ser legível")
        .map(|entrada| entrada.expect("entrada de specs/").path())
        .filter(|caminho| caminho.extension().is_some_and(|ext| ext == "md"))
        .collect();
    specs.sort();
    assert!(
        specs
            .iter()
            .any(|caminho| caminho.ends_with("01-arquitetura.md")),
        "a `specs/01-arquitetura.md` sumiu da pasta, e era ela que fazia a promessa: \
         este guarda passaria sem olhar para lugar nenhum"
    );
    for caminho in specs {
        let nome = format!(
            "specs/{}",
            caminho.file_name().expect("arquivo").to_string_lossy()
        );
        let texto = std::fs::read_to_string(&caminho).expect("spec legível");
        lidos.push((nome, texto));
    }

    for (nome, texto) in &lidos {
        assert!(
            !normalizado(texto).contains(PROMESSA),
            "`{nome}` diz que o servidor «{PROMESSA}». Ele recebe a voz e o texto em \
             claro, e a `specs/08` manda dizer isso: quem hospeda pode, em tese, \
             gravar a voz. Uma promessa de privacidade que o produto não cumpre é a \
             frase em que alguém confia"
        );
    }

    // «Ponta a ponta», sozinho, é o nome do que a v1 **não** tem: cifrado de
    // quem fala até quem ouve, com o servidor no meio sem ler. O TLS daqui vai
    // de cada pessoa até o servidor de quem hospeda, e a expressão só pode
    // ficar no README dizendo isso na mesma frase.
    let readme = normalizado(&ler("README.md"));
    let soltas: Vec<&str> = readme
        .split(['.', '!', '?'])
        .filter(|frase| frase.contains("ponta a ponta") && !frase.contains("até o servidor"))
        .map(str::trim)
        .collect();
    assert!(
        soltas.is_empty(),
        "o README diz «ponta a ponta» sem dizer até onde. O TLS vai de cada pessoa \
         até o servidor de quem hospeda, e quem hospeda recebe a voz e o texto em \
         claro (specs/08): sem o «até o servidor» na mesma frase, a expressão \
         promete a cifra de quem fala até quem ouve, que a v1 não tem.\n{}",
        soltas
            .iter()
            .map(|frase| format!("  «{frase}»"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
