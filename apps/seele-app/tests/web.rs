//! A `ui/` no navegador (F4 da 0.16, ADR 0057): a ponte web e o executor de
//! MODs dele.
//!
//! **A ponte** (`ui/ponte-web.js`) põe no lugar do `window.__TAURI__` o que o
//! navegador sabe fazer, e recusa com nome o resto. Estes guardas seguram as
//! listas dela contra o que a casca chama e contra o `main.rs`; o
//! comportamento se prova nas bancadas `bancada/ponte-web.cjs` (Node) e
//! `bancada/ponte-sem-gesto.cjs` (Chromium).
//!
//! **O executor** (`ui/mods-no-navegador.js` e `ui/executor-de-mod.html`) se
//! prova por comportamento na bancada `bancada/executor-no-navegador.cjs`, num
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
/// saber qual vem, e a escolha é a decisão que a ponte tomou na carga
/// (`PONTE_NO_LUGAR_DO_TAURI`): o lugar se decide num arquivo só. A escolha
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
        "PONTE_NO_LUGAR_DO_TAURI",
        "executorNativo(id, geracao, hash)",
        "executorNoNavegador(id, geracao, hash)",
    ] {
        assert!(
            escolha.contains(exigido),
            "o `executorDoMod` deixou de ter `{exigido}`: {escolha}"
        );
    }
}

// ------------------------------------------------------------------- a ponte

/// Cada `ui/*.js`, com o nome, para o guarda dizer onde.
fn scripts_por_arquivo() -> Vec<(String, String)> {
    let mut todos: Vec<(String, String)> = std::fs::read_dir(app_dir().join("ui"))
        .expect("ui/")
        .filter_map(Result::ok)
        .map(|entrada| entrada.file_name().to_string_lossy().into_owned())
        .filter(|nome| nome.ends_with(".js"))
        .map(|nome| {
            let texto = ler(&format!("ui/{nome}"));
            (nome, texto)
        })
        .collect();
    todos.sort();
    assert!(
        todos.len() >= 30,
        "a `ui/` tem {} scripts: a leitura quebrou",
        todos.len()
    );
    todos
}

/// Todo `invoke("nome")` da casca, sem os comentários: a regra do
/// `invoked_commands` de `tests/frontend.rs`.
fn comandos_que_a_casca_chama() -> BTreeSet<String> {
    let mut achados = BTreeSet::new();
    for (_, fonte) in scripts_por_arquivo() {
        let codigo = sem_comentarios(&fonte);
        for pedaco in codigo.split("invoke(\"").skip(1) {
            if let Some(nome) = pedaco.split('"').next() {
                achados.insert(nome.to_owned());
            }
        }
    }
    assert!(
        achados.len() >= 100,
        "a casca chama {} comandos: a leitura quebrou",
        achados.len()
    );
    achados
}

fn e_nome_de_comando(nome: &str) -> bool {
    !nome.is_empty() && nome.chars().all(|c| c.is_ascii_lowercase() || c == '_')
}

/// Os nomes que uma lista da ponte declara, na forma em que ela as declara:
/// - `const X = new Set([ "a", "b" ]);`: todo nome entre aspas;
/// - `const X = new Map([ ["a", …], ["b", …] ]);`: o primeiro de cada entrada.
fn lista_da_ponte(ponte: &str, lista: &str) -> BTreeSet<String> {
    let codigo = sem_comentarios(ponte);
    let depois = codigo
        .split_once(&format!("const {lista} = new "))
        .unwrap_or_else(|| panic!("a ponte não declara `{lista}`"))
        .1;
    let (tipo, corpo) = depois.split_at(3);
    let corpo = corpo
        .strip_prefix("([")
        .unwrap_or_else(|| panic!("`{lista}` não abre com `new {tipo}([`"))
        .split_once("]);")
        .unwrap_or_else(|| panic!("`{lista}` não fecha com `]);`"))
        .0;
    let nomes: BTreeSet<String> = match tipo {
        "Set" => corpo
            .split('"')
            .skip(1)
            .step_by(2)
            .filter(|nome| e_nome_de_comando(nome))
            .map(str::to_owned)
            .collect(),
        "Map" => primeiros_das_entradas(corpo),
        outro => {
            panic!("`{lista}` é um `{outro}`, e a ponte declara as listas como `Set` ou `Map`")
        }
    };
    for nome in &nomes {
        assert!(
            e_nome_de_comando(nome),
            "`{lista}` tem `{nome}`, que não é nome de comando"
        );
    }
    nomes
}

/// O primeiro texto de cada entrada `[…]` de um `new Map([ … ])`, contando a
/// fundura e pulando o que está dentro de aspas: uma entrada escrita em várias
/// linhas, ou um valor com `[` dentro, não confunde a leitura.
fn primeiros_das_entradas(corpo: &str) -> BTreeSet<String> {
    let mut nomes = BTreeSet::new();
    let mut fundura = 0usize;
    let mut abriu_entrada = false;
    let mut aspas: Option<char> = None;
    let mut texto = String::new();
    let mut escapado = false;
    for c in corpo.chars() {
        if let Some(fecha) = aspas {
            if escapado {
                escapado = false;
                texto.push(c);
            } else if c == '\\' {
                escapado = true;
            } else if c == fecha {
                aspas = None;
                if abriu_entrada {
                    nomes.insert(std::mem::take(&mut texto));
                    abriu_entrada = false;
                }
                texto.clear();
            } else {
                texto.push(c);
            }
            continue;
        }
        match c {
            '"' | '\'' | '`' => aspas = Some(c),
            '[' | '{' | '(' => {
                abriu_entrada = fundura == 0 && c == '[';
                fundura += 1;
            }
            ']' | '}' | ')' => fundura = fundura.saturating_sub(1),
            c if c.is_whitespace() => {}
            _ => abriu_entrada = false,
        }
    }
    nomes
}

/// As seis listas da ponte.
struct Listas {
    atendidos: BTreeSet<String>,
    so_na_larga: BTreeSet<String>,
    recusados: BTreeSet<String>,
    neutros: BTreeSet<String>,
    apesar_da_hospedagem: BTreeSet<String>,
    a_fazer: BTreeSet<String>,
}

fn listas_da_ponte() -> Listas {
    let ponte = ler("ui/ponte-web.js");
    Listas {
        atendidos: lista_da_ponte(&ponte, "COMANDOS_DA_PONTE"),
        so_na_larga: lista_da_ponte(&ponte, "SO_NA_CASCA_LARGA"),
        recusados: lista_da_ponte(&ponte, "RECUSAS_NOMEADAS"),
        neutros: lista_da_ponte(&ponte, "NEUTROS_DE_QUEM_NAO_HOSPEDA"),
        apesar_da_hospedagem: lista_da_ponte(&ponte, "ATENDIDOS_APESAR_DA_HOSPEDAGEM"),
        a_fazer: lista_da_ponte(&ponte, "AINDA_A_FAZER"),
    }
}

/// **Todo comando que a casca chama está na ponte, nas duas cascas.**
///
/// No navegador, um `invoke` que a ponte não conhece é um botão que não faz
/// nada e não diz por quê. Cada comando cai numa lista, e numa só:
/// - `COMANDOS_DA_PONTE`: atendido nas duas cascas;
/// - `SO_NA_CASCA_LARGA`: atendido no PC (D-web-3) e recusado com a frase no
///   celular;
/// - `RECUSAS_NOMEADAS`: recusado com a frase nas duas;
/// - `NEUTROS_DE_QUEM_NAO_HOSPEDA` e `ATENDIDOS_APESAR_DA_HOSPEDAGEM`: os que
///   mexem na hospedagem e não são recusados (o guarda abaixo).
///
/// Então no celular atende-se `COMANDOS_DA_PONTE` mais os dois da hospedagem,
/// e recusa-se o resto; no PC, também os `SO_NA_CASCA_LARGA`. **E a lista não
/// guarda o que a casca não chama mais**: um nome velho é uma porta que
/// ninguém confere.
#[test]
fn todo_comando_que_a_casca_chama_esta_na_ponte_nas_duas_cascas() {
    let chamados = comandos_que_a_casca_chama();
    let l = listas_da_ponte();
    let nomeadas = [
        ("COMANDOS_DA_PONTE", &l.atendidos),
        ("SO_NA_CASCA_LARGA", &l.so_na_larga),
        ("RECUSAS_NOMEADAS", &l.recusados),
        ("NEUTROS_DE_QUEM_NAO_HOSPEDA", &l.neutros),
        ("ATENDIDOS_APESAR_DA_HOSPEDAGEM", &l.apesar_da_hospedagem),
    ];
    for (i, (nome_a, a)) in nomeadas.iter().enumerate() {
        for (nome_b, b) in nomeadas.iter().skip(i + 1) {
            let nas_duas: Vec<_> = a.intersection(b).collect();
            assert!(
                nas_duas.is_empty(),
                "{nas_duas:?} estão em `{nome_a}` e em `{nome_b}`: cada comando cai numa lista só"
            );
        }
    }
    let todos: BTreeSet<String> = nomeadas
        .iter()
        .flat_map(|(_, lista)| lista.iter().cloned())
        .collect();
    let sem_dono: Vec<_> = chamados.difference(&todos).collect();
    assert!(
        sem_dono.is_empty(),
        "a casca chama no navegador o que a ponte não atende nem recusa, em nenhuma das duas \
         cascas: {sem_dono:?}"
    );
    let velhos: Vec<_> = todos.difference(&chamados).collect();
    assert!(
        velhos.is_empty(),
        "a ponte lista o que a casca não chama mais: {velhos:?}. Tire-os da lista"
    );
    let a_fazer_fora: Vec<_> = l
        .a_fazer
        .iter()
        .filter(|nome| {
            !l.atendidos.contains(*nome)
                && !l.so_na_larga.contains(*nome)
                && !l.apesar_da_hospedagem.contains(*nome)
        })
        .collect();
    assert!(
        a_fazer_fora.is_empty(),
        "`AINDA_A_FAZER` só guarda o que a ponte atende e ainda não tem corpo: {a_fazer_fora:?}"
    );
}

/// O corpo da função de um comando no `main.rs`, sem os comentários.
fn corpo_do_comando(main: &str, comando: &str) -> Option<String> {
    let inicio = [
        format!("\nfn {comando}("),
        format!("\nasync fn {comando}("),
        format!("\nconst fn {comando}("),
    ]
    .iter()
    .find_map(|assinatura| main.find(assinatura.as_str()))?;
    let resto = &main[inicio..];
    let abre = resto.find('{')?;
    let mut fundura = 0usize;
    for (onde, c) in resto[abre..].char_indices() {
        match c {
            '{' => fundura += 1,
            '}' => {
                fundura -= 1;
                if fundura == 0 {
                    return Some(sem_comentarios(&resto[abre..=abre + onde]));
                }
            }
            _ => {}
        }
    }
    None
}

/// **O critério da hospedagem, comando a comando** (`F4/ponte-no-pc.md`, o
/// último item; pedido pela orquestradora em 09/10).
///
/// O navegador não hospeda. Todo comando que a casca chama e cujo corpo, no
/// `main.rs`, mexe na hospedagem local (`hospedagem`, `persistence_hospedada`,
/// `portaria::`, `persistence_do_mod`, `servidor_do_mod`, `sala::`) está numa
/// de três listas, e nunca solto
/// entre os atendidos:
/// 1. `RECUSAS_NOMEADAS`;
/// 2. `NEUTROS_DE_QUEM_NAO_HOSPEDA`, com a resposta (a bancada `ponte-web.cjs`
///    confere cada uma);
/// 3. `ATENDIDOS_APESAR_DA_HOSPEDAGEM`, com o motivo (o `disconnect`).
///
/// Um comando novo que mexa na hospedagem reprova até alguém decidir de que
/// lado ele fica. E o contrário também: um neutro que não mexe na hospedagem
/// está na lista errada.
#[test]
fn todo_comando_que_mexe_na_hospedagem_e_recusado_neutro_ou_atendido_com_motivo() {
    let main = ler("src/main.rs");
    let l = listas_da_ponte();
    let mut mexem = BTreeSet::new();
    for comando in comandos_que_a_casca_chama() {
        let Some(corpo) = corpo_do_comando(&main, &comando) else {
            continue;
        };
        let toca = [
            "hospedagem",
            "persistence_hospedada",
            "portaria::",
            "persistence_do_mod",
            // O ajudante da F3 que acha o servidor de um MOD pelas hospedagens
            // (`38421122`, na 0.16/api-6), e a sala pessoal da F7, que mexe nas
            // hospedagens de dentro do `sala.rs` (`sala::subir`,
            // `sala::desligar`, na 0.16/sala-viva). Um comando que só chame um
            // deles mexe na hospedagem sem escrever a palavra no corpo.
            "servidor_do_mod",
            "sala::",
        ]
        .iter()
        .any(|marca| corpo.contains(marca));
        if toca {
            mexem.insert(comando);
        }
    }
    assert!(
        mexem.len() >= 10,
        "o guarda achou {} comandos que mexem na hospedagem ({mexem:?}); em 09/10 eram 13, e a \
         leitura do `main.rs` quebrou",
        mexem.len()
    );
    for comando in &mexem {
        assert!(
            l.recusados.contains(comando)
                || l.neutros.contains(comando)
                || l.apesar_da_hospedagem.contains(comando),
            "`{comando}` mexe na hospedagem local (main.rs) e não está em `RECUSAS_NOMEADAS`, \
             `NEUTROS_DE_QUEM_NAO_HOSPEDA` nem `ATENDIDOS_APESAR_DA_HOSPEDAGEM`: decida de que \
             lado ele fica (F4/ponte-no-pc.md)"
        );
    }
    for comando in l.neutros.iter().chain(&l.apesar_da_hospedagem) {
        assert!(
            mexem.contains(comando),
            "`{comando}` está entre os da hospedagem na ponte, e o corpo dele no `main.rs` não \
             mexe nela: ele é um atendido comum"
        );
    }
}

/// Os números de um valor JSON como `f64`: `-60` da ponte e `-60.0` do Rust
/// são o mesmo número.
fn numeros_como_f64(valor: serde_json::Value) -> serde_json::Value {
    match valor {
        serde_json::Value::Number(n) => serde_json::json!(n.as_f64().expect("um número")),
        serde_json::Value::Array(itens) => {
            serde_json::Value::Array(itens.into_iter().map(numeros_como_f64).collect())
        }
        serde_json::Value::Object(campos) => serde_json::Value::Object(
            campos
                .into_iter()
                .map(|(chave, valor)| (chave, numeros_como_f64(valor)))
                .collect(),
        ),
        outro => outro,
    }
}

/// Uma constante `const NOME: T = <expressão>;` do `main.rs`, com a expressão
/// feita de inteiros multiplicados (`8 * 1024`).
fn inteiro_do_main(main: &str, nome: &str) -> u64 {
    let expressao = main
        .split_once(&format!("const {nome}: "))
        .unwrap_or_else(|| panic!("o `main.rs` não declara `{nome}`"))
        .1
        .split_once('=')
        .expect("a constante tem valor")
        .1
        .split_once(';')
        .expect("a constante fecha")
        .0;
    expressao
        .split('*')
        .map(|fator| {
            fator
                .trim()
                .replace('_', "")
                .parse::<u64>()
                .expect("um inteiro")
        })
        .product()
}

/// **O que a ponte responde sem perguntar a ninguém é o que o desktop
/// responde.** As constantes da casca (o teto da mensagem, as regras da
/// prévia e do ícone, as APIs de MOD aceitas, os domínios de GIF e os
/// controles da voz sem ajuste guardado) moram no Rust. Na ponte elas são uma
/// cópia, e este guarda é o que impede a cópia de discordar: quando o Rust
/// muda, ele reprova e diz o valor novo.
#[test]
fn as_constantes_da_ponte_sao_as_do_desktop() {
    let ponte = sem_comentarios(&ler("ui/ponte-web.js"));
    let json = ponte
        .split_once("const CONSTANTES_DO_DESKTOP = Object.freeze(")
        .expect("a ponte declara `CONSTANTES_DO_DESKTOP`")
        .1
        .split_once(");")
        .expect("`CONSTANTES_DO_DESKTOP` fecha")
        .0;
    let da_ponte: serde_json::Value = serde_json::from_str(json)
        .unwrap_or_else(|erro| panic!("`CONSTANTES_DO_DESKTOP` tem de ser JSON estrito: {erro}"));
    let main = ler("src/main.rs");
    let dominios: Vec<String> = main
        .split_once("const DOMINIOS_DE_GIF: &[&str] = &[")
        .expect("o `main.rs` declara `DOMINIOS_DE_GIF`")
        .1
        .split_once("];")
        .expect("`DOMINIOS_DE_GIF` fecha")
        .0
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect();
    let do_desktop = serde_json::json!({
        "limite_da_mensagem": seele_ffi::Connection::limite_da_mensagem(),
        "regras_de_previa": seele_ffi::Connection::preview_rules(),
        "regras_do_icone_do_server": {
            "limite_bytes": inteiro_do_main(&main, "TETO_DO_ICONE"),
            "lado": inteiro_do_main(&main, "LADO_DO_ICONE"),
        },
        "apis_de_mod_aceitas": seele_ffi::mods::APIS_ACEITAS,
        "dominios_de_gif": dominios,
        // O `controles_da_voz` do desktop sem sessão e sem ajuste guardado
        // (main.rs): a supressão em 1 e a abertura padrão dela.
        "controles_da_voz": seele_ffi::ControlesDaVoz {
            supressao: 1.0,
            abertura_dbfs: seele_ffi::ABERTURA_COM_SUPRESSAO_DBFS,
            abertura_escolhida: false,
            abertura_minima_dbfs: seele_ffi::ABERTURA_MINIMA_DBFS,
            abertura_maxima_dbfs: seele_ffi::ABERTURA_MAXIMA_DBFS,
        },
    });
    assert_eq!(
        numeros_como_f64(da_ponte),
        numeros_como_f64(do_desktop.clone()),
        "as `CONSTANTES_DO_DESKTOP` da ponte discordam do Rust. Os valores de hoje são:\n{}",
        serde_json::to_string_pretty(&do_desktop).expect("JSON")
    );
}

/// **A ponte é o primeiro script do `<head>`, e síncrono.**
///
/// A F8 escolhe a casca pela marca `data-casca` na raiz, antes da primeira
/// pintura (§9.2 do spec dela): o `invoke("plataforma")` é assíncrono, e a CSP
/// do pacote não deixa script inline. Então a marca sai da ponte, e a ponte
/// roda antes de tudo. E ela lê a escolha **uma vez**: girar ou redimensionar
/// não troca a casca no meio da sessão, então ela não ouve nada que mude.
#[test]
fn a_ponte_e_o_primeiro_script_do_head_e_le_a_casca_uma_vez() {
    let pagina = sem_comentarios(&ler("ui/index.html"));
    let cabeca = pagina
        .split_once("</head>")
        .expect("o `index.html` tem `</head>`")
        .0;
    let primeiro = pagina
        .split("<script")
        .nth(1)
        .and_then(|resto| resto.split_once('>'))
        .expect("a página tem script")
        .0;
    assert_eq!(
        primeiro.trim(),
        "src=\"ponte-web.js\"",
        "o primeiro script da página tem de ser a ponte, sem `async`, `defer` nem `type`: a \
         marca da casca tem de existir antes da primeira pintura"
    );
    assert!(
        cabeca.contains("<script src=\"ponte-web.js\"></script>"),
        "a ponte tem de carregar dentro do `<head>`"
    );
    let ponte = sem_comentarios(&ler("ui/ponte-web.js"));
    for exigido in [
        "localStorage.getItem(\"seele.casca\")",
        "matchMedia(\"(pointer: coarse)\").matches",
        "innerWidth < 900",
        "document.documentElement.dataset.casca = ",
        "\"web\"",
    ] {
        assert!(ponte.contains(exigido), "a ponte deixou de ter `{exigido}`");
    }
    for proibido in [
        "\"resize\"",
        "\"orientationchange\"",
        ".onchange",
        "addListener(",
    ] {
        assert!(
            !ponte.contains(proibido),
            "a ponte ouve `{proibido}`: a casca é lida uma vez, na carga, e não troca no meio da \
             sessão"
        );
    }
}

/// **O lugar se decide num arquivo só.** O Tauri sempre injeta o
/// `__TAURI_INTERNALS__`, e só a ponte o lê; quem mais precisa saber onde
/// está (o `executorDoMod`) lê a decisão dela, o `PONTE_NO_LUGAR_DO_TAURI`.
/// Dois critérios discordariam no primeiro duble de bancada.
#[test]
fn so_a_ponte_le_o_tauri_para_saber_o_lugar() {
    for (nome, fonte) in scripts_por_arquivo() {
        if nome == "ponte-web.js" {
            continue;
        }
        let codigo = sem_comentarios(&fonte);
        assert!(
            !codigo.contains("__TAURI_INTERNALS__"),
            "ui/{nome} decide o lugar pelo `__TAURI_INTERNALS__`; leia o `PONTE_NO_LUGAR_DO_TAURI`"
        );
    }
    let ponte = sem_comentarios(&ler("ui/ponte-web.js"));
    assert!(
        ponte.contains("const PONTE_NO_LUGAR_DO_TAURI ="),
        "a ponte não declara `PONTE_NO_LUGAR_DO_TAURI`"
    );
}

/// **Nenhum HTML cru na casca.** No navegador, um XSS alcança a chave da
/// pessoa, que mora no armazenamento da origem. A casca monta por
/// `createElement` e `textContent`, e este guarda vale para toda a `ui/`, e não
/// só para a região de MOD.
#[test]
fn nenhum_html_cru_na_casca() {
    let proibidos = [
        "innerHTML",
        "outerHTML",
        "insertAdjacentHTML",
        "createContextualFragment",
        "document.write",
    ];
    for (nome, fonte) in scripts_por_arquivo() {
        for (n, linha) in sem_comentarios(&fonte).lines().enumerate() {
            for proibido in proibidos {
                assert!(
                    !linha.contains(proibido),
                    "ui/{nome}:{} escreve HTML cru (`{proibido}`); no navegador um XSS alcança a \
                     chave da pessoa",
                    n + 1
                );
            }
        }
    }
}

/// Os pares `"a" => "b"` de um trecho de `match` do Rust, com as várias
/// chaves de um braço (`Some("jpg" | "jpeg")`) e o `_ =>` como chave vazia.
fn bracos_do_match(corpo: &str) -> BTreeMap<String, String> {
    let mut pares = BTreeMap::new();
    for linha in corpo.lines() {
        let Some((esquerda, direita)) = linha.split_once("=>") else {
            continue;
        };
        let Some(valor) = direita.split('"').nth(1) else {
            continue;
        };
        let chaves: Vec<&str> = esquerda.split('"').skip(1).step_by(2).collect();
        if chaves.is_empty() && esquerda.trim() == "_" {
            pares.insert(String::new(), valor.to_owned());
        }
        for chave in chaves {
            pares.insert(chave.to_owned(), valor.to_owned());
        }
    }
    pares
}

/// **O arquivo por alça segue as regras do desktop** (`F4/ponte-no-pc.md`, «O
/// arquivo sem caminho»). O tipo de um arquivo escolhido sai da extensão pela
/// tabela do `tipo_alegado`, e não do `File.type`; uma imagem colada ganha a
/// extensão do `guardar_colado` e o mesmo teto. Na ponte isso é uma cópia, e
/// este guarda a prende ao `main.rs`.
#[test]
fn as_regras_do_arquivo_da_ponte_sao_as_do_desktop() {
    let ponte = sem_comentarios(&ler("ui/ponte-web.js"));
    let json = ponte
        .split_once("const REGRAS_DO_ARQUIVO = Object.freeze(")
        .expect("a ponte declara `REGRAS_DO_ARQUIVO`")
        .1
        .split_once(");")
        .expect("`REGRAS_DO_ARQUIVO` fecha")
        .0;
    let da_ponte: serde_json::Value = serde_json::from_str(json)
        .unwrap_or_else(|erro| panic!("`REGRAS_DO_ARQUIVO` tem de ser JSON estrito: {erro}"));
    let main = ler("src/main.rs");
    let tipos =
        bracos_do_match(&corpo_do_comando(&main, "tipo_alegado").expect("o `tipo_alegado`"));
    let colagem: BTreeMap<String, String> =
        bracos_do_match(&corpo_do_comando(&main, "guardar_colado").expect("o `guardar_colado`"))
            .into_iter()
            .filter(|(tipo, _)| tipo.starts_with("image/"))
            .collect();
    let mut por_extensao = tipos.clone();
    let desconhecido = por_extensao
        .remove("")
        .expect("o `tipo_alegado` tem `_ =>`");
    let do_desktop = serde_json::json!({
        "tipos_por_extensao": por_extensao,
        "tipo_desconhecido": desconhecido,
        "extensao_da_colagem": colagem,
        "maior_colagem_bytes": inteiro_do_main(&main, "MAIOR_COLAGEM_BYTES"),
    });
    assert!(
        por_extensao.len() >= 10 && colagem.len() >= 4,
        "a leitura do `main.rs` achou {} extensões e {} tipos de colagem: ela quebrou",
        por_extensao.len(),
        colagem.len()
    );
    assert_eq!(
        numeros_como_f64(da_ponte),
        numeros_como_f64(do_desktop.clone()),
        "as `REGRAS_DO_ARQUIVO` da ponte discordam do `main.rs`. As de hoje são:\n{}",
        serde_json::to_string_pretty(&do_desktop).expect("JSON")
    );
}

/// **A regra da voz sem sessão é a do desktop.** Sem redução de ruído, o
/// `controles_da_voz` do desktop abre o portão em
/// `ABERTURA_SEM_SUPRESSAO_DBFS`, e não na abertura de com supressão. A ponte a
/// aplica com uma cópia, e este guarda a prende ao `seele_ffi`.
#[test]
fn as_regras_da_voz_da_ponte_sao_as_do_desktop() {
    let ponte = sem_comentarios(&ler("ui/ponte-web.js"));
    let json = ponte
        .split_once("const REGRAS_DA_VOZ = Object.freeze(")
        .expect("a ponte declara `REGRAS_DA_VOZ`")
        .1
        .split_once(");")
        .expect("`REGRAS_DA_VOZ` fecha")
        .0;
    let da_ponte: serde_json::Value = serde_json::from_str(json)
        .unwrap_or_else(|erro| panic!("`REGRAS_DA_VOZ` tem de ser JSON estrito: {erro}"));
    let do_desktop = serde_json::json!({
        "abertura_sem_supressao_dbfs": seele_ffi::ABERTURA_SEM_SUPRESSAO_DBFS,
    });
    assert_eq!(
        numeros_como_f64(da_ponte),
        numeros_como_f64(do_desktop.clone()),
        "as `REGRAS_DA_VOZ` da ponte discordam do `seele_ffi`. As de hoje são:\n{}",
        serde_json::to_string_pretty(&do_desktop).expect("JSON")
    );
}
