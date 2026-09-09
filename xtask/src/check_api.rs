//! Enforces the MOD API façade from `api/`.
//!
//! ADR 0045. The question this asks, and the direction is the whole point:
//!
//! > Does every name in `api/vN.json` still point at something?
//!
//! And never the reverse. A check that asked "did every type reach the API?"
//! would drag the API along behind the protocol, and renaming a field inside
//! would repaint the façade and break every published MOD — which is exactly
//! the defect this file exists to prevent.
//!
//! Same argument as `check_deps`: "a contract checked only by review is a
//! contract that erodes".
//!
//! # Textual, and what that costs
//!
//! It looks for the symbol in the concatenated sources. A real resolver would
//! need the compiler, and what this has to catch — a rename — changes the text.
//! The cost is that it finds a symbol anywhere in the repository rather than
//! exactly where the mapping claims: it guards against a name **disappearing**,
//! not against it moving.

use std::process::ExitCode;

/// One MOD-facing name that no longer resolves.
type Violation = String;

/// Pure rule evaluation, kept free of the filesystem so it can be tested.
fn evaluate(mapa: &[(&str, &str)], fonte: &str) -> Vec<Violation> {
    let mut violacoes = Vec::new();
    for (nome, interior) in mapa {
        let alvo = interior.rsplit("::").next().unwrap_or(interior);
        if !fonte.contains(alvo) {
            violacoes.push(format!(
                "`{nome}` aponta para `{interior}`, e `{alvo}` não existe mais. \
                 Conserte o mapeamento em `api/`, nunca o nome que o MOD escreve."
            ));
        }
    }
    violacoes
}

/// Reads `api/*.json` and every crate source, and reports orphans.
pub(crate) fn run() -> ExitCode {
    let raiz = match std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent() {
        Some(caminho) => caminho.to_path_buf(),
        None => {
            eprintln!("check-api: não achei a raiz do repositório");
            return ExitCode::FAILURE;
        }
    };

    let mut fonte = String::new();
    for pasta in ["crates", "apps"] {
        colher(&raiz.join(pasta), &mut fonte);
    }

    let mut houve = false;
    let Ok(entradas) = std::fs::read_dir(raiz.join("api")) else {
        eprintln!("check-api: não achei `api/`");
        return ExitCode::FAILURE;
    };
    let mut versoes = 0_usize;
    for entrada in entradas.flatten() {
        let caminho = entrada.path();
        if caminho.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        versoes += 1;
        let Ok(texto) = std::fs::read_to_string(&caminho) else {
            eprintln!("check-api: não deu para ler {}", caminho.display());
            return ExitCode::FAILURE;
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&texto) else {
            eprintln!("check-api: {} não é json", caminho.display());
            return ExitCode::FAILURE;
        };

        let mut mapa: Vec<(String, String)> = Vec::new();
        for bloco in ["reads", "actions", "own", "world"] {
            if let Some(obj) = json.get(bloco).and_then(serde_json::Value::as_object) {
                for (nome, interior) in obj {
                    if let Some(interior) = interior.as_str() {
                        mapa.push((nome.clone(), interior.to_owned()));
                    }
                }
            }
        }
        let emprestado: Vec<(&str, &str)> = mapa
            .iter()
            .map(|(nome, interior)| (nome.as_str(), interior.as_str()))
            .collect();
        for violacao in evaluate(&emprestado, &fonte) {
            eprintln!("check-api: {} — {violacao}", caminho.display());
            houve = true;
        }
    }

    if houve {
        ExitCode::FAILURE
    } else {
        println!(
            "check-api: toda a superfície de MOD ainda aponta para algo ({versoes} versão(ões))."
        );
        ExitCode::SUCCESS
    }
}

/// Concatenates every `.rs` under `dir`.
fn colher(dir: &std::path::Path, destino: &mut String) {
    let Ok(entradas) = std::fs::read_dir(dir) else {
        return;
    };
    for entrada in entradas.flatten() {
        let caminho = entrada.path();
        if caminho.is_dir() {
            if caminho.file_name().and_then(|n| n.to_str()) == Some("target") {
                continue;
            }
            colher(&caminho, destino);
        } else if caminho.extension().and_then(|e| e.to_str()) == Some("rs") {
            if let Ok(texto) = std::fs::read_to_string(&caminho) {
                destino.push_str(&texto);
                destino.push('\n');
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn um_nome_que_aponta_para_algo_que_existe_passa() {
        let fonte = "pub struct Person { pub nick: String }";
        assert!(evaluate(&[("pessoa.apelido", "Person::nick")], fonte).is_empty());
    }

    /// O guarda inteiro, numa frase: renomear por dentro fica vermelho **aqui**,
    /// e não na máquina de quem escreveu um MOD seis meses atrás.
    #[test]
    fn um_nome_que_aponta_para_o_que_sumiu_reprova() {
        let fonte = "pub struct Person { pub apelido: String }";
        let violacoes = evaluate(&[("pessoa.apelido", "Person::nick")], fonte);
        assert_eq!(violacoes.len(), 1);
        let primeira = violacoes.first().map_or("", String::as_str);
        assert!(primeira.contains("Person::nick"));
        assert!(
            primeira.contains("pessoa.apelido"),
            "a violação não diz qual nome de MOD ficou órfão"
        );
    }

    /// A direção importa, e é a correção que o dono achou no desenho:
    /// acrescentar coisa ao protocolo **não** reprova. Se reprovasse, todo tipo
    /// novo empurraria a API para a frente e a fachada viraria projeção de novo.
    #[test]
    fn um_tipo_novo_no_interior_que_a_api_nao_menciona_nao_reprova() {
        let fonte = "pub struct Person { pub nick: String, pub retrato: Vec<u8> }";
        assert!(evaluate(&[("pessoa.apelido", "Person::nick")], fonte).is_empty());
    }
}
