//! O pacote do web app (Tarefa 3.6 da F4): o que vai para a origem
//! (a `ORIGEM_DO_NAVEGADOR`, D-web-2), montado e conferido.
//!
//!     cargo xtask pacote-web
//!
//! Monta em `target/pacote-web/`:
//! - os arquivos de `apps/seele-app/ui/`;
//! - o `seele_web.wasm` **sem a `prova`**, compilado num `target/` próprio
//!   (`target/pacote-web-build/`), para não trocar o que o `xtask web` e as
//!   bancadas usam, que é o com a `prova`;
//! - o `seele_motor_web.wasm`, com o wasi-sdk já conferido. Este passo **nunca
//!   baixa** o SDK ([`crate::wasi_sdk::caminho_sem_baixar`]);
//! - o `_headers` de `apps/seele-app/web/`.
//!
//! E confere ([`conferir`]) antes de dizer que ele sai:
//! 1. **os tokens da cópia são os congelados** (`design/seele-tokens.css`). O
//!    guarda `the_served_tokens_are_the_frozen_tokens` lê a `ui/`, e não a
//!    cópia;
//! 2. **a CSP do `_headers`**, que existe para a página, e nenhuma com
//!    `'unsafe-inline'` nem `'unsafe-eval'`;
//! 3. **nenhum HTML com `<script src="http`** (nem `//`): nenhum script de
//!    terceiro;
//! 4. **nenhum comando da ponte em `AINDA_A_FAZER`**: um atendido sem corpo é um
//!    botão que responde «ainda não» a quem usa;
//! 5. **o `.wasm` da página sem as exportações da `prova`**, lidas do
//!    `fronteira.rs` do `seele-web`, e o motor com cara de `.wasm`.
//!
//! Sai com 1 se o pacote não sai, e diz por quê. Desde a Tarefa 2.6 a ponte
//! atende todos os comandos, e ele sai. Nada aqui publica: quem leva o pacote
//! à origem é o dono
//! (`docs/superpowers/sdd/2026-10-08-0.16/F4/origem-para-o-dono.md`).

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn raiz() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

fn cargo() -> Command {
    Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
}

fn rodar(comando: &mut Command) -> Result<(), String> {
    let saida = comando
        .current_dir(raiz())
        .output()
        .map_err(|e| format!("não rodou: {e}"))?;
    if saida.status.success() {
        return Ok(());
    }
    let erro = String::from_utf8_lossy(&saida.stderr);
    let linhas: Vec<&str> = erro.lines().collect();
    Err(format!(
        "{}\n{}",
        saida.status,
        linhas
            .get(linhas.len().saturating_sub(25)..)
            .unwrap_or_default()
            .join("\n")
    ))
}

/// Copia `de` para `para`, com as subpastas.
fn copiar_pasta(de: &Path, para: &Path) -> Result<(), String> {
    std::fs::create_dir_all(para).map_err(|e| format!("{}: {e}", para.display()))?;
    for entrada in std::fs::read_dir(de).map_err(|e| format!("{}: {e}", de.display()))? {
        let entrada = entrada.map_err(|e| format!("{}: {e}", de.display()))?;
        let destino = para.join(entrada.file_name());
        if entrada.path().is_dir() {
            copiar_pasta(&entrada.path(), &destino)?;
        } else {
            std::fs::copy(entrada.path(), &destino)
                .map_err(|e| format!("{}: {e}", destino.display()))?;
        }
    }
    Ok(())
}

/// As exportações da `prova` do `seele-web`: as `fn` de `extern "C"` que vêm
/// depois do `#[cfg(feature = "prova")]` da fronteira. Lidas do Rust, e não
/// copiadas para cá: uma nova entra na conferência sozinha.
pub(crate) fn exportacoes_da_prova(fronteira: &str) -> Vec<String> {
    let Some(inicio) = fronteira.find("#[cfg(feature = \"prova\")]") else {
        return Vec::new();
    };
    fronteira
        .get(inicio..)
        .unwrap_or_default()
        .lines()
        .filter(|linha| linha.contains("extern \"C\" fn "))
        .filter_map(|linha| {
            let depois = linha.split("extern \"C\" fn ").nth(1)?;
            let nome: String = depois
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            (!nome.is_empty()).then_some(nome)
        })
        .collect()
}

/// O texto das constantes de uma linha (`const NOME = "texto";`) da ponte: as
/// tarefas da `AINDA_A_FAZER` são nomes delas.
fn constantes_de_texto(ponte: &str) -> std::collections::BTreeMap<String, String> {
    ponte
        .lines()
        .filter_map(|linha| {
            let resto = linha.trim().strip_prefix("const ")?;
            let (nome, valor) = resto.split_once(" = \"")?;
            let valor = valor.strip_suffix("\";")?;
            Some((nome.to_owned(), valor.to_owned()))
        })
        .collect()
}

/// Os comandos de `AINDA_A_FAZER` da ponte, com a tarefa de cada um (o texto
/// da constante, quando é uma), ou `None` se a lista não foi achada (e a
/// conferência ficou cega).
fn ainda_a_fazer(ponte: &str) -> Option<Vec<(String, String)>> {
    let constantes = constantes_de_texto(ponte);
    let inicio = ponte.find("const AINDA_A_FAZER = new Map([")?;
    let corpo = ponte.get(inicio..)?;
    let fim = corpo.find("]);")?;
    Some(
        corpo
            .get(..fim)?
            .lines()
            .filter_map(|linha| {
                let linha = linha.trim().strip_prefix("[\"")?;
                let (comando, resto) = linha.split_once('"')?;
                let tarefa = resto
                    .trim_start_matches([',', ' '])
                    .trim_end_matches([']', ',', ' ']);
                let tarefa = constantes.get(tarefa).map_or(tarefa, String::as_str);
                Some((comando.to_owned(), tarefa.to_owned()))
            })
            .collect(),
    )
}

/// As fontes de cada `Content-Security-Policy` do `_headers`.
fn politicas(cabecalhos: &str) -> Vec<String> {
    cabecalhos
        .lines()
        .filter_map(|linha| {
            let (nome, valor) = linha.trim().split_once(':')?;
            nome.trim()
                .eq_ignore_ascii_case("content-security-policy")
                .then(|| valor.trim().to_owned())
        })
        .collect()
}

/// Os HTML do pacote que carregam script de outra origem.
fn scripts_de_fora(pasta: &Path) -> Vec<String> {
    let mut achados = Vec::new();
    let Ok(entradas) = std::fs::read_dir(pasta) else {
        return achados;
    };
    for entrada in entradas.filter_map(Result::ok) {
        let nome = entrada.file_name().to_string_lossy().into_owned();
        if !nome.ends_with(".html") {
            continue;
        }
        let texto = std::fs::read_to_string(entrada.path())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .replace('\'', "\"");
        for (em, _) in texto.match_indices("<script") {
            let tag: String = texto
                .get(em..)
                .unwrap_or_default()
                .chars()
                .take_while(|c| *c != '>')
                .collect();
            let fonte = tag
                .split("src=\"")
                .nth(1)
                .and_then(|resto| resto.split('"').next())
                .unwrap_or_default();
            if fonte.starts_with("http:") || fonte.starts_with("https:") || fonte.starts_with("//")
            {
                achados.push(format!("{nome}: {fonte}"));
            }
        }
    }
    achados
}

/// **A conferência do pacote**, em `pasta`. Devolve o que impede o pacote de
/// sair, uma frase por motivo; vazio é «sai». Ver o doc do módulo.
pub(crate) fn conferir(pasta: &Path, congelados: &str, da_prova: &[String]) -> Vec<String> {
    let ler = |nome: &str| std::fs::read_to_string(pasta.join(nome));
    let mut motivos = Vec::new();

    if !pasta.join("index.html").is_file() {
        motivos.push("o pacote não tem `index.html`".to_owned());
    }

    // 1.
    match ler("tokens.css") {
        Ok(tokens) if tokens == congelados => {}
        Ok(_) => motivos.push(
            "os tokens da cópia (`tokens.css`) não são os congelados (`design/seele-tokens.css`)"
                .to_owned(),
        ),
        Err(_) => motivos.push("o pacote não tem `tokens.css`".to_owned()),
    }

    // 2.
    match ler("_headers") {
        Err(_) => motivos.push("o pacote não tem `_headers`".to_owned()),
        Ok(cabecalhos) => {
            let todas = politicas(&cabecalhos);
            if !todas.iter().any(|csp| csp.contains("default-src")) {
                motivos.push("o `_headers` não traz a CSP da página".to_owned());
            }
            for csp in &todas {
                for proibida in ["'unsafe-inline'", "'unsafe-eval'"] {
                    if csp.split([' ', ';']).any(|fonte| fonte == proibida) {
                        motivos.push(format!("uma CSP do `_headers` tem {proibida}: {csp}"));
                    }
                }
            }
        }
    }

    // 3.
    for achado in scripts_de_fora(pasta) {
        motivos.push(format!(
            "um HTML do pacote carrega script de fora: {achado}"
        ));
    }

    // 4.
    match ler("ponte-web.js").ok().as_deref().map(ainda_a_fazer) {
        None => motivos.push("o pacote não tem a `ponte-web.js`".to_owned()),
        Some(None) => motivos.push(
            "não achei o `AINDA_A_FAZER` na `ponte-web.js`: esta conferência ficou cega".to_owned(),
        ),
        Some(Some(pendentes)) if !pendentes.is_empty() => {
            let mut por_tarefa: std::collections::BTreeMap<&str, Vec<&str>> =
                std::collections::BTreeMap::new();
            for (comando, tarefa) in &pendentes {
                por_tarefa.entry(tarefa).or_default().push(comando);
            }
            let lista: Vec<String> = por_tarefa
                .iter()
                .map(|(tarefa, comandos)| format!("{tarefa}: {}", comandos.join(", ")))
                .collect();
            motivos.push(format!(
                "a ponte ainda tem {} comandos a fazer, e cada um é um botão que responde \
                 «ainda não» a quem usa. {}",
                pendentes.len(),
                lista.join("; ")
            ));
        }
        Some(Some(_)) => {}
    }

    // 5.
    match std::fs::read(pasta.join("seele_web.wasm")) {
        Err(_) => motivos.push("o pacote não tem o `seele_web.wasm`".to_owned()),
        Ok(bytes) => {
            if !bytes.starts_with(b"\0asm") {
                motivos.push("o `seele_web.wasm` não é um `.wasm`".to_owned());
            }
            for nome in da_prova {
                if bytes
                    .windows(nome.len())
                    .any(|janela| janela == nome.as_bytes())
                {
                    motivos.push(format!(
                        "o `seele_web.wasm` leva `{nome}`, da `prova`: ele foi compilado para a bateria, \
                         e não para a origem"
                    ));
                }
            }
        }
    }
    if da_prova.is_empty() {
        motivos.push(
            "não achei as exportações da `prova` no `fronteira.rs`: esta conferência ficou cega"
                .to_owned(),
        );
    }
    match std::fs::read(pasta.join("seele_motor_web.wasm")) {
        Ok(bytes) if bytes.starts_with(b"\0asm") => {}
        Ok(_) => motivos.push("o `seele_motor_web.wasm` não é um `.wasm`".to_owned()),
        Err(_) => motivos.push("o pacote não tem o `seele_motor_web.wasm`".to_owned()),
    }
    motivos
}

pub(crate) fn run() -> ExitCode {
    match montar_e_conferir() {
        Ok((pasta, motivos)) if motivos.is_empty() => {
            eprintln!("pacote-web: o pacote sai, em {}", pasta.display());
            ExitCode::SUCCESS
        }
        Ok((pasta, motivos)) => {
            eprintln!("pacote-web: montado em {}, e NÃO SAI:", pasta.display());
            for motivo in motivos {
                eprintln!("  - {motivo}");
            }
            ExitCode::FAILURE
        }
        Err(erro) => {
            eprintln!("pacote-web: FALHOU ao montar: {erro}");
            ExitCode::FAILURE
        }
    }
}

fn montar_e_conferir() -> Result<(PathBuf, Vec<String>), String> {
    let raiz = raiz();
    let sdk = crate::wasi_sdk::caminho_sem_baixar()?;
    let construcao = raiz.join("target/pacote-web-build");
    rodar(
        cargo()
            .args([
                "build",
                "--release",
                "-p",
                "seele-web",
                "--target",
                "wasm32-unknown-unknown",
            ])
            .arg("--target-dir")
            .arg(&construcao),
    )
    .map_err(|e| format!("o seele-web sem a prova: {e}"))?;
    // O motor não tem `prova`: o `target/` de sempre, o mesmo do `xtask web`.
    rodar(
        cargo()
            .args([
                "build",
                "--release",
                "-p",
                "seele-motor-web",
                "--target",
                "wasm32-wasip1",
            ])
            .env("WASI_SDK", &sdk),
    )
    .map_err(|e| format!("o motor: {e}"))?;

    let pasta = raiz.join("target/pacote-web");
    let _ = std::fs::remove_dir_all(&pasta);
    copiar_pasta(&raiz.join("apps/seele-app/ui"), &pasta)?;
    for (de, para) in [
        (
            construcao.join("wasm32-unknown-unknown/release/seele_web.wasm"),
            "seele_web.wasm",
        ),
        (
            raiz.join("target/wasm32-wasip1/release/seele_motor_web.wasm"),
            "seele_motor_web.wasm",
        ),
        (raiz.join("apps/seele-app/web/_headers"), "_headers"),
    ] {
        std::fs::copy(&de, pasta.join(para)).map_err(|e| format!("{}: {e}", de.display()))?;
    }

    let congelados = std::fs::read_to_string(raiz.join("design/seele-tokens.css"))
        .map_err(|e| format!("design/seele-tokens.css: {e}"))?;
    let fronteira = std::fs::read_to_string(raiz.join("crates/seele-web/src/fronteira.rs"))
        .map_err(|e| format!("crates/seele-web/src/fronteira.rs: {e}"))?;
    let motivos = conferir(&pasta, &congelados, &exportacoes_da_prova(&fronteira));
    Ok((pasta, motivos))
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]
mod tests {
    use super::*;

    /// Uma pasta de teste nova, sob o `target/` deste crate.
    fn pasta_nova(nome: &str) -> PathBuf {
        let pasta = raiz().join("target/pacote-web-testes").join(nome);
        let _ = std::fs::remove_dir_all(&pasta);
        std::fs::create_dir_all(&pasta).unwrap();
        pasta
    }

    const CONGELADOS: &str = ":root { --cor: #000; }\n";
    const DA_PROVA: &[&str] = &["analisar_link_em", "json_do_cliente"];

    fn da_prova() -> Vec<String> {
        DA_PROVA.iter().map(|&n| n.to_owned()).collect()
    }

    /// Um pacote que sai: cada teste estraga uma coisa só.
    fn pacote_bom(nome: &str) -> PathBuf {
        let pasta = pasta_nova(nome);
        std::fs::write(
            pasta.join("index.html"),
            "<!doctype html><script src=\"base.js\"></script>",
        )
        .unwrap();
        std::fs::write(pasta.join("tokens.css"), CONGELADOS).unwrap();
        std::fs::write(
            pasta.join("_headers"),
            "/\n  Content-Security-Policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval'\n",
        )
        .unwrap();
        std::fs::write(
            pasta.join("ponte-web.js"),
            "const AINDA_A_FAZER = new Map([\n]);\n",
        )
        .unwrap();
        std::fs::write(
            pasta.join("seele_web.wasm"),
            b"\0asm\x01\0\0\0analisar_link",
        )
        .unwrap();
        std::fs::write(pasta.join("seele_motor_web.wasm"), b"\0asm\x01\0\0\0").unwrap();
        pasta
    }

    #[test]
    fn um_pacote_bom_sai() {
        let pasta = pacote_bom("bom");
        assert_eq!(
            conferir(&pasta, CONGELADOS, &da_prova()),
            Vec::<String>::new()
        );
    }

    /// Um estrago: o nome do caso, o que ele faz ao pacote bom e a frase que
    /// o motivo tem de trazer.
    type Estrago<'a> = (&'a str, &'a dyn Fn(&Path), &'a str);

    /// Cada regra, estragada sozinha, reprova com a frase dela.
    #[test]
    fn cada_regra_reprova_o_que_ela_guarda() {
        let casos: [Estrago<'_>; 6] = [
            (
                "tokens",
                &|p| std::fs::write(p.join("tokens.css"), ":root { --cor: #fff; }\n").unwrap(),
                "não são os congelados",
            ),
            (
                "inline",
                &|p| {
                    std::fs::write(p.join("_headers"), "/\n  Content-Security-Policy: default-src 'self'; script-src 'self' 'unsafe-inline'\n").unwrap()
                },
                "'unsafe-inline'",
            ),
            (
                "de-fora",
                &|p| {
                    std::fs::write(
                        p.join("index.html"),
                        "<script src=\"https://cdn.exemplo/x.js\"></script>",
                    )
                    .unwrap()
                },
                "script de fora",
            ),
            (
                "a-fazer",
                &|p| {
                    std::fs::write(
                        p.join("ponte-web.js"),
                        "const AINDA_A_FAZER = new Map([\n  [\"connect\", TAREFA_34],\n]);\n",
                    )
                    .unwrap()
                },
                "1 comandos a fazer",
            ),
            (
                "prova",
                &|p| {
                    std::fs::write(p.join("seele_web.wasm"), b"\0asm\x01\0\0\0analisar_link_em")
                        .unwrap()
                },
                "`analisar_link_em`, da `prova`",
            ),
            (
                "sem-motor",
                &|p| std::fs::remove_file(p.join("seele_motor_web.wasm")).unwrap(),
                "seele_motor_web.wasm",
            ),
        ];
        for (nome, estragar, frase) in casos {
            let pasta = pacote_bom(nome);
            estragar(&pasta);
            let motivos = conferir(&pasta, CONGELADOS, &da_prova());
            assert!(
                motivos.len() == 1 && motivos.iter().all(|m| m.contains(frase)),
                "{nome}: esperava um motivo com «{frase}», veio {motivos:?}"
            );
        }
    }

    /// O `wasm-unsafe-eval` não é o `unsafe-eval`: a página compila o `.wasm`.
    #[test]
    fn o_wasm_unsafe_eval_nao_e_o_unsafe_eval() {
        let pasta = pacote_bom("wasm-unsafe-eval");
        assert!(conferir(&pasta, CONGELADOS, &da_prova()).is_empty());
    }

    /// As exportações da `prova` são lidas do `fronteira.rs` de verdade.
    #[test]
    fn as_exportacoes_da_prova_vem_da_fronteira() {
        let fronteira =
            std::fs::read_to_string(raiz().join("crates/seele-web/src/fronteira.rs")).unwrap();
        let lidas = exportacoes_da_prova(&fronteira);
        assert!(
            lidas.iter().any(|n| n == "analisar_link_em") && lidas.len() >= 5,
            "a leitura das exportações da `prova` ficou cega: {lidas:?}"
        );
        assert!(
            !lidas.iter().any(|n| n == "analisar_link"),
            "a função da página entrou na lista da `prova`: {lidas:?}"
        );
    }

    /// **O pacote de hoje, com a `ui/`, o `_headers` e os tokens de verdade**
    /// (e `.wasm` de mentira, que não se compilam num teste): ele sai. Até a
    /// Tarefa 2.6, só a ponte o segurava, com os comandos da sessão pelo
    /// navegador a fazer; hoje a ponte atende todos.
    #[test]
    fn hoje_o_pacote_sai() {
        let raiz = raiz();
        let pasta = pasta_nova("hoje");
        copiar_pasta(&raiz.join("apps/seele-app/ui"), &pasta).unwrap();
        std::fs::copy(
            raiz.join("apps/seele-app/web/_headers"),
            pasta.join("_headers"),
        )
        .unwrap();
        std::fs::write(pasta.join("seele_web.wasm"), b"\0asm\x01\0\0\0").unwrap();
        std::fs::write(pasta.join("seele_motor_web.wasm"), b"\0asm\x01\0\0\0").unwrap();
        let congelados = std::fs::read_to_string(raiz.join("design/seele-tokens.css")).unwrap();
        let fronteira =
            std::fs::read_to_string(raiz.join("crates/seele-web/src/fronteira.rs")).unwrap();
        let motivos = conferir(&pasta, &congelados, &exportacoes_da_prova(&fronteira));
        assert!(
            motivos.is_empty(),
            "o pacote de hoje devia sair, e não sai: {motivos:?}"
        );
    }
}
