//! A `ui/` no navegador (F4 da 0.16, ADR 0057): o executor de MODs dele.
//!
//! O executor (`ui/mods-no-navegador.js` e `ui/executor-de-mod.html`) se prova
//! por comportamento na bancada `bancada/executor-no-navegador.cjs`, num
//! Chromium e com o motor de verdade. Estes guardas seguram o que a bancada de
//! um dia não vê mudar no dia seguinte: o texto das duas paredes. Um
//! `allow-same-origin` acrescentado, um `connect-src` na CSP do `iframe` ou um
//! script mudado sem o hash refeito são uma linha cada.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use base64::Engine as _;
use sha2::{Digest as _, Sha256};

fn app_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Um arquivo de `apps/seele-app/`, **com os bytes do disco**. O hash da CSP é
/// sobre os bytes que o navegador recebe, e o `.gitattributes` os mantém em LF
/// em todo sistema; normalizar aqui esconderia um checkout que os trocasse.
fn ler(relativo: &str) -> String {
    std::fs::read_to_string(app_dir().join(relativo))
        .unwrap_or_else(|erro| panic!("{relativo}: {erro}"))
}

/// O texto sem os comentários de HTML, de bloco e de linha. A mesma regra do
/// `without_comments` de `tests/frontend.rs`: a prosa ao lado de uma parede a
/// cita, e um guarda que lesse a prosa passaria pelo comentário.
fn sem_comentarios(texto: &str) -> String {
    let mut saida = String::with_capacity(texto.len());
    let mut resto = texto;
    while let Some(inicio) = resto.find("<!--") {
        saida.push_str(&resto[..inicio]);
        match resto[inicio..].find("-->") {
            Some(fim) => resto = &resto[inicio + fim + 3..],
            None => return saida,
        }
    }
    saida.push_str(resto);

    let mut sem_blocos = String::with_capacity(saida.len());
    let mut resto = saida.as_str();
    while let Some(inicio) = resto.find("/*") {
        sem_blocos.push_str(&resto[..inicio]);
        match resto[inicio..].find("*/") {
            Some(fim) => resto = &resto[inicio + fim + 2..],
            None => return sem_blocos,
        }
    }
    sem_blocos.push_str(resto);

    sem_blocos
        .lines()
        .map(|linha| match linha.find("//") {
            Some(em) if !linha[..em].contains(['"', '`', '\'']) => &linha[..em],
            _ => linha,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// O `iframe` dos executores: o script dele, inteiro, e a CSP, diretiva a
/// diretiva.
struct Iframe {
    script: String,
    csp: BTreeMap<String, BTreeSet<String>>,
}

fn iframe_dos_executores() -> Iframe {
    let html = ler("ui/executor-de-mod.html");
    assert_eq!(
        html.matches("<script").count(),
        1,
        "ui/executor-de-mod.html tem de ter um script só, o que o hash da CSP cobre"
    );
    let (antes, depois) = html
        .split_once("<script>")
        .expect("o script do `iframe` é `<script>`, sem `src` nem atributo");
    let script = depois
        .split_once("</script>")
        .expect("o script do `iframe` fecha")
        .0
        .to_owned();
    // A CSP de um `<meta>` só vale para o que vem depois dele.
    let conteudo = antes
        .split_once("<meta http-equiv=\"Content-Security-Policy\" content=\"")
        .expect("a CSP do `iframe` é um `<meta>` antes do script")
        .1
        .split_once('"')
        .expect("o `content` da CSP fecha")
        .0;
    let mut csp = BTreeMap::new();
    for diretiva in conteudo.split(';').map(str::trim).filter(|d| !d.is_empty()) {
        let mut partes = diretiva.split_whitespace();
        let nome = partes.next().expect("uma diretiva tem nome").to_owned();
        let fontes: BTreeSet<String> = partes.map(str::to_owned).collect();
        assert!(
            csp.insert(nome.clone(), fontes).is_none(),
            "a CSP do `iframe` repete `{nome}`, e o navegador fica com a primeira"
        );
    }
    Iframe { script, csp }
}

/// **O `iframe` só roda o próprio script**, e o hash na CSP é o dele.
///
/// O script é o anfitrião inteiro: o preâmbulo que tira do Worker o
/// armazenamento, o filtro do que o Worker diz e o teto da fila. Um script
/// mudado sem o hash refeito não roda, e todo MOD do celular para de subir.
/// Este guarda diz isso no `cargo test`, e não num telefone.
#[test]
fn o_hash_da_csp_do_iframe_e_o_do_script_dele() {
    let iframe = iframe_dos_executores();
    let hash = base64::engine::general_purpose::STANDARD.encode(Sha256::digest(&iframe.script));
    let esperado = BTreeSet::from([format!("'sha256-{hash}'"), "'wasm-unsafe-eval'".to_owned()]);
    assert_eq!(
        iframe.csp.get("script-src"),
        Some(&esperado),
        "o `script-src` do `iframe` tem de ser o hash do script dele e o \
         `'wasm-unsafe-eval'` do motor, e nada mais. O hash de hoje é \
         'sha256-{hash}'"
    );
}

/// **A CSP do `iframe` não deixa conectar a nada.**
///
/// É a segunda parede: se um MOD um dia sair do motor, ele cai num Worker deste
/// `iframe`, e o Worker herda esta CSP. Com `default-src 'none'` e sem
/// `connect-src`, ele não tem `fetch`, `WebSocket` nem `importScripts` que
/// alcancem nada. As diretivas são exatamente três: uma quarta é uma porta, e
/// quem a abrir tem de passar por aqui.
#[test]
fn a_csp_do_iframe_nao_deixa_conectar_a_nada() {
    let iframe = iframe_dos_executores();
    let nomes: Vec<&str> = iframe.csp.keys().map(String::as_str).collect();
    assert_eq!(
        nomes,
        ["default-src", "script-src", "worker-src"],
        "a CSP do `iframe` dos executores ganhou ou perdeu uma diretiva; um \
         `connect-src` ali deixa um MOD fugido falar com a rede"
    );
    assert_eq!(
        iframe.csp["default-src"],
        BTreeSet::from(["'none'".to_owned()]),
        "o `default-src` do `iframe` é `'none'`: é ele que fecha a rede, as \
         imagens e os quadros que nenhuma outra diretiva nomeia"
    );
    assert_eq!(
        iframe.csp["worker-src"],
        BTreeSet::from(["blob:".to_owned()]),
        "os Workers do `iframe` vêm do `blob:` que ele mesmo monta, e de mais nada"
    );
}

/// **O `iframe` dos executores nunca tem a origem da página.**
///
/// É a origem opaca que tira do MOD fugido o armazenamento da página, onde mora
/// a `CryptoKey` da pessoa. Com `allow-same-origin`, o `iframe` volta a ter a
/// origem dela, e um Worker dele também: foi esse o defeito que tirou o Worker
/// de `blob:` do desktop. O guarda lê o código de toda a `ui/`, e não só o
/// executor: um segundo `iframe` em outro arquivo seria outra porta.
#[test]
fn o_iframe_dos_executores_nunca_tem_a_origem_da_pagina() {
    let executor = sem_comentarios(&ler("ui/mods-no-navegador.js"));
    assert!(
        executor.contains("iframe.sandbox = \"allow-scripts\";"),
        "o `iframe` dos executores deixou de ser criado com \
         `sandbox = \"allow-scripts\"`, e só isso"
    );
    let mut lidos = 0;
    for entrada in std::fs::read_dir(app_dir().join("ui"))
        .expect("ui/")
        .filter_map(Result::ok)
    {
        let nome = entrada.file_name().to_string_lossy().into_owned();
        if ![".js", ".html"].iter().any(|e| nome.ends_with(e)) {
            continue;
        }
        lidos += 1;
        let codigo = sem_comentarios(&ler(&format!("ui/{nome}")));
        assert!(
            !codigo.contains("allow-same-origin"),
            "ui/{nome} dá a um `iframe` a origem da página (`allow-same-origin`)"
        );
        let cria_iframe =
            codigo.contains("createElement(\"iframe\")") || codigo.contains("<iframe");
        assert!(
            !cria_iframe || nome == "mods-no-navegador.js",
            "ui/{nome} cria um `iframe`, e o único da `ui/` é o dos executores, \
             em mods-no-navegador.js"
        );
    }
    assert!(
        lidos >= 30,
        "o guarda leu {lidos} arquivos da `ui/`: a leitura quebrou"
    );
}

/// **Cada lugar tem o seu executor, e quem escolhe é o `mods-runtime.js`.**
///
/// No desktop, o QuickJS nativo, do outro lado da ponte do Tauri. No navegador,
/// o mesmo motor num Worker do `iframe` opaco. A `base.js` pede o executor sem
/// saber qual vem, e a escolha é pela presença do `__TAURI_INTERNALS__`, que o
/// Tauri sempre injeta (o `__TAURI__` depende do `withGlobalTauri`). A escolha
/// funcionando se prova na bancada `ciclo-do-executor.cjs`; este guarda segura
/// que a `base.js` não volta a pedir um dos dois pelo nome.
#[test]
fn a_base_pede_o_executor_do_lugar_sem_saber_qual() {
    let base = sem_comentarios(&ler("ui/base.js"));
    assert!(
        base.contains("executorDoMod(mod.id, geracao, mod.hash)"),
        "`base.js` deixou de pedir o executor ao `executorDoMod`"
    );
    for nome in ["executorNativo(", "executorNoNavegador("] {
        assert!(
            !base.contains(nome),
            "`base.js` pede `{nome}` direto, e no outro lugar o MOD sobe no executor errado"
        );
    }
    let runtime = sem_comentarios(&ler("ui/mods-runtime.js"));
    let escolha = runtime
        .split_once("function executorDoMod(")
        .expect("`mods-runtime.js` declara o `executorDoMod`")
        .1
        .split_once("\n}\n")
        .expect("o `executorDoMod` fecha")
        .0;
    for exigido in [
        "\"__TAURI_INTERNALS__\" in globalThis",
        "executorNativo(id, geracao, hash)",
        "executorNoNavegador(id, geracao, hash)",
    ] {
        assert!(
            escolha.contains(exigido),
            "o `executorDoMod` deixou de ter `{exigido}`: {escolha}"
        );
    }
}
