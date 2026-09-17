//! Prova que a versão do produto não ficou para trás em lugar nenhum.
//!
//! # A pergunta que este comando responde
//!
//! Não é «o número está certo» — este repositório não tem o número. A versão do
//! workspace é `0.0.0` **de propósito**: quem escolhe o número de verdade é a
//! tag, e o empacotamento o injeta. Um `version = "0.11.0"` no `Cargo.toml`
//! seria uma segunda fonte da mesma verdade, e duas fontes divergem.
//!
//! A pergunta é a outra: **todo lugar que carrega a versão a recebe?** E ela
//! tem dois lados, os dois já quebrados na prática:
//!
//! - **quem lê.** `seele-instalador` anunciava `CARGO_PKG_VERSION` e o painel do
//!   Windows registrava `DisplayVersion 0.0.0` — relatado em campo. E o `Hello`
//!   do cliente se apresentava como `connection/0.0.0`, de toda máquina e de
//!   toda release, com o servidor jogando a string fora sem ler.
//! - **quem entrega.** Achado ao fechar a v0.11.0: `SEELE_VERSAO` chegava só ao
//!   build do `seeled`. Os três passos que empacotam o **app** não a passavam, e
//!   o `empacotar/windows.ps1` a apagava de propósito, com um comentário
//!   dizendo que o app não a lia. Ele não lia; passou a ler.
//!
//! O segundo lado é o que nenhum teste de Rust alcança: ele está em YAML e em
//! dois scripts de concha. É por isso que este comando existe em vez de um
//! `#[test]`.

use std::path::Path;
use std::process::ExitCode;

/// Todo arquivo que compila algo que vai para a mão de alguém, e o que nele
/// precisa carregar a versão.
///
/// Uma tabela e não uma varredura: uma varredura por «cargo build» acharia
/// também os builds de teste e de ferramenta, que não empacotam nada e não
/// devem carimbar versão nenhuma. O que entra aqui entra por decisão.
const ENTREGAS: &[(&str, &str)] = &[
    (
        ".github/workflows/release.yml",
        "o workflow que publica as três plataformas",
    ),
    ("empacotar/macos.sh", "o empacotamento local do macOS"),
    ("empacotar/windows.ps1", "o empacotamento local do Windows"),
];

/// Quantas vezes `SEELE_VERSAO` precisa aparecer, no mínimo, em cada entrega.
///
/// O número sai de contar o que cada uma compila e entrega:
///
/// - o workflow tem cinco — `seeled` duas vezes (comum e universal do macOS) e
///   o app três (macOS, Windows, Linux);
/// - `macos.sh` tem duas: o `seeled` e o app;
/// - `windows.ps1` tem duas: o `seeled`, cuja variável agora **fica** para o
///   build do app logo abaixo, e o instalador próprio.
///
/// Mínimos e não exatos de propósito: acrescentar uma plataforma tem de passar
/// por aqui para ser contada, e não pode reprovar só por existir.
const MINIMO: &[(&str, usize)] = &[
    (".github/workflows/release.yml", 5),
    ("empacotar/macos.sh", 2),
    ("empacotar/windows.ps1", 2),
];

/// Onde a versão do produto **não** pode vir do `Cargo.toml`.
///
/// Cada um destes já disse `0.0.0` para alguém de verdade. Os guardas de
/// unidade que os prendem vivem nos próprios crates; esta lista é a que fica
/// junto do resto e não depende de alguém lembrar de rodar aquele crate.
const NAO_PODEM_LER_O_CARGO: &[&str] = &[
    "crates/seele-core/src/client.rs",
    "crates/seele-server/src/main.rs",
    "apps/seele-instalador/src/instalacao.rs",
    "apps/seele-instalador/src/janela.rs",
];

/// Roda a conferência.
pub(crate) fn run() -> ExitCode {
    let raiz = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask mora dentro do workspace");
    let mut faltas: Vec<String> = Vec::new();

    for (arquivo, o_que_e) in ENTREGAS {
        let caminho = raiz.join(arquivo);
        let Ok(texto) = std::fs::read_to_string(&caminho) else {
            faltas.push(format!("não consegui ler {arquivo} ({o_que_e})"));
            continue;
        };
        let quantas = texto.matches("SEELE_VERSAO").count();
        let minimo = MINIMO
            .iter()
            .find(|(nome, _)| nome == arquivo)
            .map_or(1, |(_, n)| *n);
        if quantas < minimo {
            faltas.push(format!(
                "{arquivo} ({o_que_e}) menciona SEELE_VERSAO {quantas} vez(es), \
                 e são precisas ao menos {minimo}. Um passo que compila algo \
                 entregável sem ela produz um binário que se anuncia como \
                 `local` — ou, pior, que não se anuncia."
            ));
        }
    }

    for arquivo in NAO_PODEM_LER_O_CARGO {
        let caminho = raiz.join(arquivo);
        let Ok(texto) = std::fs::read_to_string(&caminho) else {
            faltas.push(format!("não consegui ler {arquivo}"));
            continue;
        };
        if texto.contains("env!(\"CARGO_PKG_VERSION\")") {
            faltas.push(format!(
                "{arquivo} voltou a ler a versão do Cargo.toml, que é 0.0.0 no \
                 workspace inteiro e nunca foi a versão do produto"
            ));
        }
    }

    // E a outra ponta: que o `0.0.0` do workspace continue lá. Se alguém puser
    // um número nele, passam a existir duas fontes da mesma verdade — e a que
    // fica velha é sempre a que ninguém olha.
    let cargo = raiz.join("Cargo.toml");
    match std::fs::read_to_string(&cargo) {
        Ok(texto) if texto.contains("version = \"0.0.0\"") => {}
        Ok(_) => faltas.push(
            "o Cargo.toml do workspace deixou de dizer 0.0.0. O número do \
             produto vem da tag e é injetado no empacotamento; um número aqui é \
             uma segunda fonte da mesma verdade, e a que fica velha é a que \
             ninguém olha."
                .to_owned(),
        ),
        Err(_) => faltas.push("não consegui ler o Cargo.toml do workspace".to_owned()),
    }

    if faltas.is_empty() {
        println!(
            "check-versao: a versão alcança as {} entregas, e nenhum dos {} \
             lugares que a anunciam lê o Cargo.toml.",
            ENTREGAS.len(),
            NAO_PODEM_LER_O_CARGO.len()
        );
        return ExitCode::SUCCESS;
    }
    eprintln!(
        "check-versao: a versão ficou para trás em {} lugar(es):",
        faltas.len()
    );
    for falta in &faltas {
        eprintln!("  - {falta}");
    }
    ExitCode::FAILURE
}
