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
//!    `fronteira.rs` do `seele-web`, e o motor com cara de `.wasm`;
//! 6. **o `.wasm` da página com o perfil dela** (D-web-5, [`PERFIL_DA_PAGINA`]):
//!    sem a seção `name`, e abaixo de [`TETO_DO_WASM_DA_PAGINA`].
//!
//! Sai com 1 se o pacote não sai, e diz por quê. Desde a Tarefa 2.6 a ponte
//! atende todos os comandos, e ele sai. Nada aqui publica: quem leva o pacote
//! à origem é o dono
//! (`docs/superpowers/sdd/2026-10-08-0.16/F4/origem-para-o-dono.md`).

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// Só a cópia WEB recebe URLs por conteúdo. Uma página recém-carregada não
/// mistura o HTML novo com recursos ainda frescos no cache da versão anterior.
fn versionar_recursos(pasta: &Path) -> Result<String, String> {
    fn listar(pasta: &Path, prefixo: &str, arquivos: &mut Vec<String>) -> Result<(), String> {
        for entrada in std::fs::read_dir(pasta).map_err(|e| e.to_string())? {
            let entrada = entrada.map_err(|e| e.to_string())?;
            let nome = format!("{prefixo}{}", entrada.file_name().to_string_lossy());
            if entrada.path().is_dir() {
                listar(&entrada.path(), &format!("{nome}/"), arquivos)?;
            } else {
                arquivos.push(nome);
            }
        }
        Ok(())
    }
    let mut arquivos = Vec::new();
    listar(pasta, "", &mut arquivos)?;
    arquivos.sort();
    let mut hash = Sha256::new();
    for nome in &arquivos {
        let bytes = std::fs::read(pasta.join(nome)).map_err(|e| e.to_string())?;
        hash.update(nome.as_bytes());
        hash.update([0]);
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    let versao: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    for nome in &arquivos {
        if !["html", "js", "css"]
            .iter()
            .any(|ext| nome.ends_with(&format!(".{ext}")))
        {
            continue;
        }
        let caminho = pasta.join(nome);
        let mut texto = std::fs::read_to_string(&caminho).map_err(|e| e.to_string())?;
        // As referências do pacote são literais locais, inclusive as constantes
        // dos carregadores de WASM/iframe/worklet e os url("fontes/...").
        // Não tocamos URLs remotas, dados do usuário ou nomes sem arquivo local.
        for recurso in &arquivos {
            for aspas in ['"', '\'', '`'] {
                for prefixo in ["", "./", "/"] {
                    let antes = format!("{aspas}{prefixo}{recurso}{aspas}");
                    let depois = format!("{aspas}{prefixo}{recurso}?v={versao}{aspas}");
                    texto = texto.replace(&antes, &depois);
                }
            }
        }
        std::fs::write(caminho, texto).map_err(|e| e.to_string())?;
    }
    Ok(versao)
}

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
    // 6.
    if let Ok(bytes) = std::fs::read(pasta.join("seele_web.wasm")) {
        match secoes_proprias(&bytes) {
            Err(motivo) => motivos.push(format!("o `seele_web.wasm` não se lê: {motivo}")),
            Ok(nomes) if nomes.iter().any(|nome| nome == "name") => motivos.push(
                "o `seele_web.wasm` leva a seção `name` (os nomes das funções): ele não saiu com \
                 o perfil da página (D-web-5)"
                    .to_owned(),
            ),
            Ok(_) => {}
        }
        if bytes.len() > TETO_DO_WASM_DA_PAGINA {
            motivos.push(format!(
                "o `seele_web.wasm` tem {} bytes, acima do teto de {TETO_DO_WASM_DA_PAGINA} \
                 (D-web-5): meça de novo o que chega ao celular antes de subir o teto",
                bytes.len()
            ));
        }
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

/// **O perfil do `.wasm` da página** (D-web-5): o menor que o celular baixa.
/// Só nesta compilação, que tem o `target/` dela: o `.wasm` da `prova` e o
/// desktop não mudam. O custo fica escrito na decisão: sem a seção `name`, os
/// rastros de pilha perdem os nomes das funções, e a mensagem do pânico fica.
///
/// O `codegen-units=1` entrou na 0.17 (25/10/2026): medido no mesmo código,
/// ele tira 88 740 bytes do `.wasm` (2 092 679 → 2 003 939; brotli 11 de
/// 463 723 para 450 899), e custa só tempo de compilação desta construção.
pub(crate) const PERFIL_DA_PAGINA: [&str; 4] = [
    "profile.release.opt-level=\"z\"",
    "profile.release.lto=true",
    "profile.release.strip=\"symbols\"",
    "profile.release.codegen-units=1",
];

/// **O teto do `.wasm` da página**, em bytes.
///
/// - **21/10/2026 (0.16):** o do pacote com o [`PERFIL_DA_PAGINA`] tinha
///   1 714 052 (542 KB em gzip, 405 KB em brotli 11), e o teto de 1 900 000
///   deixava uns 10% de folga.
/// - **25/10/2026 (0.17), medido de novo antes de subir:** 2 092 679 com o
///   perfil de antes, e 2 003 939 com o `codegen-units=1`. Com ele, comprimido
///   nesta máquina, o gzip -9 dá 610 469 e o brotli 11 dá 450 899. São
///   compressões locais, comparáveis só entre si e com as de 21/10, e não uma
///   medida do que chega a um celular, que não foi medido. O Caddy planejado
///   serve com `encode zstd gzip`, sem brotli: o número planejado no fio é o
///   do gzip, e o do brotli fica como medida alternativa. Compilar e
///   instanciar no node 24, que compila sob demanda, leva 1,5 ms e 0,1 ms
///   (mediana de 7). O grafo do `.wasm` não ganhou crate: os
///   `Cargo.toml` do `seele-web`, do `seele-sessao`, do `seele-proto` e do
///   `seele-voz` são os da v0.16.0. Não entrou backend de cripto, `tracing`
///   nem `regex`. O que cresceu foi código do produto: os cargos (~200 KB de
///   código, com a leitura e a escrita em JSON dos tipos deles) e as
///   variantes novas de `ClientMessage` e `ServerMessage`, que se multiplicam
///   pelos quatro caminhos de serde da ponte (JSON lido, JSON escrito,
///   postcard lido, postcard escrito).
///   O teto novo deixa os mesmos ~10% sobre o medido.
///
/// - **27/10/2026 (0.17.3):** chamadas pessoais acrescentam contratos limitados
///   e verificação Ed25519 estrita (Dalek já usado pelo desktop, sem features
///   padrão no WASM). No mesmo perfil, 2 013 980 → 2 205 453 bytes (+191 473,
///   9,51%); gzip -9 local, 610 183 → 661 504 (+51 321, 8,41%). A prova inclui
///   convite/aceite/SDP autenticados em Chromium e WebKit; WebCrypto sozinho
///   aceitou a chave fraca de identidade e não substitui esse verificador.
///   O teto explícito de 2 250 000 deixa 44 547 bytes (2,02%) sobre a medida;
///   o limite continua bloqueando pacotes maiores. Não é medida de download
///   no aparelho nem autorização para crescimento posterior sem nova prova.
///
/// Passar dele é a hora de medir de novo o que chega ao celular, e não de
/// subir o número calado.
pub(crate) const TETO_DO_WASM_DA_PAGINA: usize = 2_250_000;

/// Os nomes das seções próprias (`custom`) de um `.wasm`, lidas do formato:
/// o cabeçalho, e cada seção com o id e o tamanho em LEB128.
fn secoes_proprias(bytes: &[u8]) -> Result<Vec<String>, String> {
    let leb = |em: &mut usize| -> Result<usize, String> {
        let mut valor = 0_usize;
        for deslocamento in (0..35).step_by(7) {
            let byte = *bytes.get(*em).ok_or("um tamanho cortado")?;
            *em += 1;
            valor |= usize::from(byte & 0x7f) << deslocamento;
            if byte & 0x80 == 0 {
                return Ok(valor);
            }
        }
        Err("um tamanho longo demais".to_owned())
    };
    if bytes.get(..8) != Some(b"\0asm\x01\0\0\0".as_slice()) {
        return Err("o cabeçalho não é o de um `.wasm`".to_owned());
    }
    let mut nomes = Vec::new();
    let mut em = 8;
    while em < bytes.len() {
        let id = *bytes.get(em).ok_or("uma seção cortada")?;
        em += 1;
        let tamanho = leb(&mut em)?;
        let fim = em
            .checked_add(tamanho)
            .filter(|fim| *fim <= bytes.len())
            .ok_or("uma seção passa do fim")?;
        if id == 0 {
            let mut dentro = em;
            let comprimento = leb(&mut dentro)?;
            let nome = bytes
                .get(dentro..dentro + comprimento)
                .filter(|_| dentro + comprimento <= fim)
                .ok_or("o nome de uma seção passa dela")?;
            nomes.push(String::from_utf8_lossy(nome).into_owned());
        }
        em = fim;
    }
    Ok(nomes)
}

fn compilar_wasm_da_pagina() -> Result<PathBuf, String> {
    let raiz = raiz();
    let construcao = raiz.join("target/pacote-web-build");
    rodar(
        cargo()
            .args([
                "build",
                "--locked",
                "--release",
                "-p",
                "seele-web",
                "--target",
                "wasm32-unknown-unknown",
            ])
            .args(
                PERFIL_DA_PAGINA
                    .iter()
                    .flat_map(|valor| ["--config", valor]),
            )
            .arg("--target-dir")
            .arg(&construcao),
    )
    .map_err(|e| format!("o seele-web sem a prova: {e}"))?;
    Ok(construcao.join("wasm32-unknown-unknown/release/seele_web.wasm"))
}

fn montar_e_conferir() -> Result<(PathBuf, Vec<String>), String> {
    let raiz = raiz();
    let sdk = crate::wasi_sdk::caminho_sem_baixar()?;
    let wasm = compilar_wasm_da_pagina()?;
    // O motor não tem `prova`: o `target/` de sempre, o mesmo do `xtask web`.
    rodar(
        cargo()
            .args([
                "build",
                "--locked",
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
        (wasm, "seele_web.wasm"),
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
    if motivos.is_empty() {
        versionar_recursos(&pasta)?;
    }
    Ok((pasta, motivos))
}

/// The desktop uses the same production module for personal WebRTC calls.
/// Build before invoking Tauri/Cargo; never recursively invoke Cargo in build.rs.
pub(crate) fn wasm_nativo() -> ExitCode {
    let preparar = || -> Result<(), String> {
        let raiz = raiz();
        let antes = crate::wasm_nativo::fontes(&raiz)?;
        let caminho = compilar_wasm_da_pagina()?;
        let bytes = std::fs::read(caminho).map_err(|e| e.to_string())?;
        let fronteira = std::fs::read_to_string(raiz.join("crates/seele-web/src/fronteira.rs"))
            .map_err(|e| e.to_string())?;
        let provas = exportacoes_da_prova(&fronteira);
        if !bytes.starts_with(b"\0asm")
            || provas.is_empty()
            || provas
                .iter()
                .any(|nome| bytes.windows(nome.len()).any(|b| b == nome.as_bytes()))
            || secoes_proprias(&bytes)?.iter().any(|nome| nome == "name")
        {
            return Err("o módulo nativo não é o WASM de produção".into());
        }
        if antes != crate::wasm_nativo::fontes(&raiz)? {
            return Err("as fontes mudaram durante a compilação; repita wasm-nativo".into());
        }
        let app = raiz.join("apps/seele-app");
        std::fs::write(app.join("ui/seele_web.wasm"), &bytes).map_err(|e| e.to_string())?;
        std::fs::write(
            app.join("wasm-nativo.sha256"),
            format!("{antes}\n{}\n", crate::wasm_nativo::soma(&bytes)),
        )
        .map_err(|e| e.to_string())?;
        crate::wasm_nativo::conferir(&raiz)?;
        eprintln!("wasm-nativo: {} bytes de produção conferidos", bytes.len());
        Ok(())
    };
    match preparar() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("wasm-nativo: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]
mod tests {
    use super::*;

    #[test]
    fn cache_de_uma_publicacao_nao_mistura_a_proxima() {
        let mut versoes = Vec::new();
        for versao in ["antiga", "nova"] {
            let pasta = pasta_nova(&format!("cache-{versao}"));
            std::fs::write(pasta.join("index.html"), "<!doctype html><link rel=\"stylesheet\" href=\"base.css\"><script src=\"app.js\"></script>").unwrap();
            std::fs::write(
                pasta.join("base.css"),
                format!("body {{ --versao: {versao}; }}"),
            )
            .unwrap();
            std::fs::write(pasta.join("app.js"), format!("window.versao = '{versao}'; window.recurso = fetch('motor.wasm').then(r => r.text()); window.worker = new Worker('worker.js'); window.worker.onmessage = e => window.trabalhador = e.data;")).unwrap();
            std::fs::write(pasta.join("worker.js"), format!("postMessage('{versao}')")).unwrap();
            std::fs::write(pasta.join("motor.wasm"), versao).unwrap();
            let hash = versionar_recursos(&pasta).unwrap();
            let html = std::fs::read_to_string(pasta.join("index.html")).unwrap();
            assert!(html.contains(&format!("app.js?v={hash}")));
            assert!(std::fs::read_to_string(pasta.join("app.js"))
                .unwrap()
                .contains(&format!("motor.wasm?v={hash}")));
            versoes.push(hash);
        }
        assert_eq!(versoes.len(), 2);
        assert_ne!(versoes.first(), versoes.get(1));
    }

    /// Uma pasta de teste nova, sob o `target/` deste crate.
    fn pasta_nova(nome: &str) -> PathBuf {
        let pasta = raiz().join("target/pacote-web-testes").join(nome);
        let _ = std::fs::remove_dir_all(&pasta);
        std::fs::create_dir_all(&pasta).unwrap();
        pasta
    }

    const CONGELADOS: &str = ":root { --cor: #000; }\n";

    /// Um `.wasm` bem formado, com uma seção própria `nome` levando `conteudo`.
    fn wasm_com(nome: &str, conteudo: &[u8]) -> Vec<u8> {
        fn leb(mut valor: usize, saida: &mut Vec<u8>) {
            loop {
                let byte = u8::try_from(valor & 0x7f).unwrap();
                valor >>= 7;
                if valor == 0 {
                    saida.push(byte);
                    return;
                }
                saida.push(byte | 0x80);
            }
        }
        let mut corpo = Vec::new();
        leb(nome.len(), &mut corpo);
        corpo.extend_from_slice(nome.as_bytes());
        corpo.extend_from_slice(conteudo);
        let mut wasm = b"\0asm\x01\0\0\0\0".to_vec();
        leb(corpo.len(), &mut wasm);
        wasm.extend(corpo);
        wasm
    }
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
            wasm_com("nada", b"analisar_link"),
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
        let casos: [Estrago<'_>; 8] = [
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
                    std::fs::write(
                        p.join("seele_web.wasm"),
                        wasm_com("nada", b"analisar_link_em"),
                    )
                    .unwrap()
                },
                "`analisar_link_em`, da `prova`",
            ),
            (
                "nomes",
                &|p| std::fs::write(p.join("seele_web.wasm"), wasm_com("name", b"\0")).unwrap(),
                "seção `name`",
            ),
            (
                "grande",
                &|p| {
                    let grande = vec![0_u8; TETO_DO_WASM_DA_PAGINA];
                    std::fs::write(p.join("seele_web.wasm"), wasm_com("nada", &grande)).unwrap()
                },
                "acima do teto",
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
        let hash = versionar_recursos(&pasta).unwrap();
        for (arquivo, recurso) in [
            ("index.html", "celular-entrada.js"),
            ("voz-web.js", "seele_web.wasm"),
            ("voz-web.js", "voz-worklet.js"),
            ("mods-no-navegador.js", "seele_motor_web.wasm"),
            ("mods-no-navegador.js", "executor-de-mod.html"),
            ("fontes.css", "fontes/saira-condensed-500.woff2"),
        ] {
            assert!(
                std::fs::read_to_string(pasta.join(arquivo))
                    .unwrap()
                    .contains(&format!("{recurso}?v={hash}")),
                "faltou versionar {arquivo}: {recurso}"
            );
        }
    }
}
