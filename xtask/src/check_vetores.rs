//! Confere que todo arquivo de vetor está **versionado**.
//!
//! # O defeito que ela existe para pegar
//!
//! O MOD de referência passou a trazer um som, e `*.wav` estava no
//! `.gitignore` — uma regra escrita para gravações de bancada, que são
//! grandes, produzidas por uma máquina, e não são fonte. O som do vetor é o
//! contrário das três coisas, mas a regra não distingue.
//!
//! **E a bateria passou.** Ela lê a cópia de trabalho, onde o arquivo estava;
//! quem reprovaria é um clone limpo, que ninguém faz antes de publicar. O
//! pacote de um MOD é endereçado pelo conteúdo, então um arquivo que não viaja
//! não é um arquivo que falta: é um hash que não fecha, numa máquina que não é
//! a de quem escreveu.
//!
//! # Por que aqui, e não em `cargo test`
//!
//! Porque precisa do `git`, e a bateria do produto não pode precisar — a mesma
//! razão de `check-runtime` precisar de Node e morar aqui. Em `xtask` a
//! exigência é explícita: quem chama esta ferramenta pediu por ela.

use std::process::ExitCode;

/// Onde moram os vetores que precisam viajar inteiros.
const VETORES: &str = "apps/seele-app/testes";

pub(crate) fn run() -> ExitCode {
    let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default();
    let pasta = raiz.join(VETORES);
    if !pasta.is_dir() {
        eprintln!("check-vetores: {VETORES} sumiu");
        return ExitCode::FAILURE;
    }

    let mut arquivos = Vec::new();
    if let Err(erro) = juntar(&raiz, &pasta, &mut arquivos) {
        eprintln!("check-vetores: não consegui ler {VETORES}: {erro}");
        return ExitCode::FAILURE;
    }
    if arquivos.is_empty() {
        eprintln!("check-vetores: não achei vetor nenhum, e isso não é aprovação");
        return ExitCode::FAILURE;
    }

    // **Contra o que o Git de fato tem**, e não contra o `.gitignore`.
    //
    // A primeira versão perguntava `check-ignore`, e ela respondia só metade:
    // um arquivo que alguém esqueceu de `git add` não é ignorado, e some do
    // clone do mesmo jeito. O que importa não é por que ele ficou de fora — é
    // que ficou.
    let saida = std::process::Command::new("git")
        .arg("-C")
        .arg(&raiz)
        .arg("ls-files")
        .arg("--")
        .arg(VETORES)
        .output();
    let saida = match saida {
        Ok(saida) => saida,
        // **Nomeado, e não engolido.** Sem `git` não dá para conferir, e dizer
        // «passou» seria a pior resposta: quem chamou acharia que os vetores
        // foram conferidos.
        Err(erro) => {
            eprintln!("check-vetores: não consegui rodar o `git`: {erro}");
            return ExitCode::FAILURE;
        }
    };
    if !saida.status.success() {
        eprintln!(
            "check-vetores: o `git ls-files` reprovou ({})",
            saida.status
        );
        return ExitCode::FAILURE;
    }
    let versionados: std::collections::BTreeSet<String> = String::from_utf8_lossy(&saida.stdout)
        .lines()
        .map(str::to_owned)
        .collect();

    let mut fora: Vec<&String> = arquivos
        .iter()
        .filter(|caminho| !versionados.contains(*caminho))
        .collect();
    fora.sort();
    if !fora.is_empty() {
        eprintln!("check-vetores: estes vetores não estão no Git, e um clone limpo não os teria:");
        for caminho in &fora {
            eprintln!("  {caminho}");
        }
        eprintln!("  um pacote de MOD é endereçado pelo conteúdo: o que não viaja vira hash que não fecha");
        return ExitCode::FAILURE;
    }
    println!(
        "check-vetores: os {} vetores estão versionados",
        arquivos.len()
    );
    ExitCode::SUCCESS
}

/// Todos os arquivos de uma árvore, em caminhos relativos à raiz do repo.
///
/// Relativos porque é assim que `git ls-files` os imprime, e comparar as duas
/// listas em grafias diferentes seria comparar nada.
fn juntar(
    raiz: &std::path::Path,
    pasta: &std::path::Path,
    achados: &mut Vec<String>,
) -> std::io::Result<()> {
    for entrada in std::fs::read_dir(pasta)? {
        let caminho = entrada?.path();
        if caminho.is_dir() {
            juntar(raiz, &caminho, achados)?;
        } else if let Ok(relativo) = caminho.strip_prefix(raiz) {
            achados.push(relativo.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}
