//! Guards for the untyped half of the desktop client.
//!
//! ADR 0019 chose no framework and no type checking on the frontend, and named
//! the mitigation: the boundary that matters is generated from Rust types, so a
//! `Snapshot` that changes shape stops compiling. What that argument does *not*
//! cover is the two things a plain-JS frontend gets wrong most often, both of
//! which fail silently at runtime:
//!
//! - calling a command name that no longer exists, and
//! - reading an element id that is not in the page.
//!
//! Neither shows up in a build. Both show up here.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use std::collections::BTreeSet;
use std::path::PathBuf;

fn app_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// A file from `apps/seele-app/`, with its channel endings normalised to `\n`.
///
/// The normalisation is not tidiness. Git on Windows checks out with CRLF by
/// default and this repository ships no `.gitattributes`, so the very same
/// commit reaches a Windows runner with every `\n` spelled `\r\n`. Several
/// guards below cut text at a channel boundary — `body_of` looks for `"\n}\n"` to
/// find where a function ends — and against CRLF that needle is simply never
/// found. `split` then returns the *whole remaining file* as the first piece,
/// so a guard scoped to one function silently widens to everything after it and
/// starts reporting its neighbours.
///
/// That is exactly how this was found: `a_person_card_…` passed on macOS and
/// failed on Windows, accusing the call screen of drawing a per-person waveform
/// out of `input_level` — a channel that lives in a different function further
/// down the same file.
///
/// Every guard here asks about **content**, and content does not change with
/// how a checkout spells its newlines. So the spelling is settled once, here,
/// rather than in each guard that happens to cut on a channel.
fn read(relative: &str) -> String {
    std::fs::read_to_string(app_dir().join(relative))
        .unwrap_or_else(|error| panic!("{relative}: {error}"))
        .replace("\r\n", "\n")
}

/// Every file directly in `ui/` with this extension, by name.
///
/// The frontend is six scripts and six stylesheets rather than one of each, so
/// every check below that used to read `ui/seele.js` has to read all of them —
/// and it has to find them by looking, not by a list somebody has to remember to
/// extend. A seventh screen that nobody adds here would otherwise arrive with
/// every guard in this file silently blind to it.
///
/// Sorted by name so the result is stable; the *load* order is asserted
/// separately, against `index.html`, which is the only place it is real.
fn ui_files(extension: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(app_dir().join("ui")) else {
        panic!("ui/ must exist: it is the whole frontend");
    };
    let mut found: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(extension))
        .collect();
    found.sort();
    assert!(!found.is_empty(), "ui/ ships no {extension} at all");
    found
}

/// Every script the window loads, concatenated.
fn scripts() -> String {
    ui_files(".js")
        .iter()
        .map(|name| read(&format!("ui/{name}")))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every stylesheet the window loads, concatenated in load order.
///
/// Load order, and not alphabetical: two of the checks below read a rule out of
/// this text, and one of them (`.veredito`) would find the wrong block if the
/// sheets were stitched in an order the browser never uses. `tokens.css` and
/// `fontes.css` are left out — they are declarations, not rules, and
/// `tests/tokens.rs` and `tests/fontes.rs` own them.
fn styles() -> String {
    let page = read("ui/index.html");
    linked_assets(&page, "href")
        .into_iter()
        .filter(|name| name.ends_with(".css") && name != "tokens.css" && name != "fontes.css")
        .map(|name| read(&format!("ui/{name}")))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The files an attribute loads, in the order the page names them.
///
/// Comments are stripped first, and that is the whole point rather than tidiness:
/// the block above the `<link>` list names half of these files in prose, so a
/// check that read the raw page could be satisfied by the paragraph explaining
/// the tag it was meant to guard.
fn linked_assets(page: &str, attribute: &str) -> Vec<String> {
    let page = without_comments(page);
    let needle = format!("{attribute}=\"");
    page.split(&needle)
        .skip(1)
        .filter_map(|piece| piece.split('"').next())
        .map(str::to_owned)
        .collect()
}

/// Every `invoke("name")` in the script.
///
/// Comments stripped first, for the reason `linked_assets` strips them: prose
/// about this boundary tends to quote the boundary. A doc comment explaining
/// that "every `invoke("…")` is tied to a registered command" made this helper
/// report a command literally named `…`, and the guard then accused the
/// frontend of calling something main.rs does not register — a true statement
/// about a call that does not exist.
fn invoked_commands(script: &str) -> BTreeSet<String> {
    let script = without_comments(script);
    let mut found = BTreeSet::new();
    for piece in script.split("invoke(\"").skip(1) {
        if let Some(name) = piece.split('"').next() {
            found.insert(name.to_owned());
        }
    }
    found
}

/// Every command registered in `generate_handler!`.
fn registered_commands(source: &str) -> BTreeSet<String> {
    let Some(block) = source.split("tauri::generate_handler![").nth(1) else {
        panic!("no generate_handler! block in main.rs");
    };
    let Some(block) = block.split(']').next() else {
        panic!("unterminated generate_handler! block");
    };
    block
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Os parâmetros de cada `#[tauri::command]`, sem os que o Tauri injeta.
///
/// `State`, `AppHandle` e `Window` chegam do runtime e nunca do JS; cobrá-los de
/// quem chama acusaria toda invocação do app.
fn command_parameters(source: &str) -> std::collections::BTreeMap<String, BTreeSet<String>> {
    let mut mapa = std::collections::BTreeMap::new();
    for bloco in source.split("#[tauri::command").skip(1) {
        let Some(inicio) = bloco.find("fn ") else {
            continue;
        };
        let resto = &bloco[inicio + 3..];
        let Some(abre) = resto.find('(') else {
            continue;
        };
        let nome = resto[..abre].trim().to_owned();
        let Some(fecha) = resto[abre..].find(')') else {
            continue;
        };
        let mut parametros = BTreeSet::new();
        for parte in resto[abre + 1..abre + fecha].split(',') {
            // **Os comentários saem antes de o nome ser lido.** Comentar um
            // parâmetro é Rust legítimo e é a norma deste repositório; sem esta
            // limpeza, a primeira linha de um `///` ou `//` acima do parâmetro
            // virava o nome dele, e este guarda acusava um defeito que era dele
            // mesmo — «o comando procura {"// Vazio é a ausência de escolha"}».
            //
            // Conservador de propósito: só linhas que **começam** com `//` são
            // descartadas. Um `//` no fim de uma linha que também traz o
            // parâmetro não acontece aqui e, se acontecesse, cortar por ele
            // arriscaria comer o que vem antes.
            let parte: String = parte
                .lines()
                .filter(|linha| !linha.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join(" ");
            let Some((chave, tipo)) = parte.split_once(':') else {
                continue;
            };
            let injetado = ["State<", "AppHandle", "Window", "Emitter", "Runtime"]
                .iter()
                .any(|marca| tipo.contains(marca));
            if !injetado {
                parametros.insert(chave.trim().trim_start_matches("mut ").to_owned());
            }
        }
        mapa.insert(nome, parametros);
    }
    mapa
}

/// O que o `#[tauri::command]` procura no objeto que o JS mandou.
///
/// `ArgumentCase::Camel` é o padrão do macro (conferido em
/// `tauri-macros/src/command/wrapper.rs`), então um parâmetro `voice_room` é
/// procurado como `voiceRoom`.
fn como_o_tauri_procura(parametro: &str) -> String {
    let mut saida = String::new();
    let mut proxima_maiuscula = false;
    for letra in parametro.chars() {
        if letra == '_' {
            proxima_maiuscula = true;
        } else if proxima_maiuscula {
            saida.extend(letra.to_uppercase());
            proxima_maiuscula = false;
        } else {
            saida.push(letra);
        }
    }
    saida
}

/// Cada `invoke("cmd", { … })` do frontend, com as chaves que ele manda.
fn invoked_with_arguments(script: &str) -> Vec<(String, usize, String)> {
    let script = without_comments(script);
    let mut achados = Vec::new();
    for pedaco in script.split("invoke(\"").skip(1) {
        let Some(nome) = pedaco.split('"').next().map(str::to_owned) else {
            continue;
        };
        let Some(abre) = pedaco.find('{') else {
            continue;
        };
        // Só o objeto literal imediato: um `invoke` cujo argumento é uma
        // variável não tem chave a conferir aqui.
        let antes = &pedaco[..abre];
        if antes.contains(';') || antes.contains("invoke(") || antes.len() > 60 {
            continue;
        }
        let Some(fecha) = pedaco[abre..].find('}') else {
            continue;
        };
        let corpo = &pedaco[abre + 1..abre + fecha];
        for item in corpo.split(',') {
            let chave = item.split(':').next().unwrap_or("").trim();
            if !chave.is_empty() && chave.chars().all(|c| c.is_alphanumeric() || c == '_') {
                achados.push((nome.clone(), 0, chave.to_owned()));
            }
        }
    }
    achados
}

/// Os argumentos de uma chamada, separados no nível de topo.
///
/// Escrito à mão em vez de um `split(',')` porque o que interessa está
/// frequentemente dentro de um ternário — `elemento("li", x ? "a" : "b")` — e
/// dividir por vírgula crua devolveria pedaços que não são argumentos. Ele
/// respeita parênteses, colchetes, chaves e aspas.
fn argumentos_da_chamada(a_partir_do_parenteses: &str) -> Vec<String> {
    let (mut prof, mut atual, mut args) = (0_i32, String::new(), Vec::new());
    let mut aspas: Option<char> = None;
    let mut escapado = false;
    for c in a_partir_do_parenteses.chars() {
        if let Some(fecha) = aspas {
            atual.push(c);
            if escapado {
                escapado = false;
            } else if c == '\\' {
                escapado = true;
            } else if c == fecha {
                aspas = None;
            }
        } else if c == '"' || c == '\'' || c == '`' {
            aspas = Some(c);
            atual.push(c);
        } else if c == '(' || c == '[' || c == '{' {
            prof += 1;
            if prof > 1 {
                atual.push(c);
            }
        } else if c == ')' || c == ']' || c == '}' {
            prof -= 1;
            if prof == 0 {
                args.push(atual);
                return args;
            }
            atual.push(c);
        } else if c == ',' && prof == 1 {
            args.push(std::mem::take(&mut atual));
        } else {
            atual.push(c);
        }
    }
    args
}

/// **Todo token que uma folha usa existe.**
///
/// `var(--seele-nao-existe)` não é erro em CSS: a propriedade some, e o
/// elemento fica com o valor herdado. Escrevi `min-height:
/// var(--seele-alvo-minimo)` num controle novo e o token não existia — a caixa
/// ficava com a altura do texto, e nada avisava. É o mesmo defeito de campo que
/// o guarda de classes existe para pegar, por uma porta que ele não olhava.
///
/// Um `var(--x, algo)` com reserva é outra coisa: ali a ausência tem resposta
/// escrita, e por isso passa.
#[test]
fn todo_token_que_uma_folha_usa_esta_declarado() {
    let folhas = styles() + &read("ui/tokens.css");
    let mut declarados: BTreeSet<String> = BTreeSet::new();
    for (posicao, _) in folhas.match_indices("--seele-") {
        let resto = &folhas[posicao..];
        let nome: String = resto
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '-')
            .collect();
        // Declaração é `--nome:`; uso é `var(--nome`. O que vem antes do nome
        // diz qual dos dois é.
        if resto[nome.len()..].starts_with(':') {
            declarados.insert(nome);
        }
    }

    let mut sem_declaracao: Vec<String> = Vec::new();
    for (posicao, _) in folhas.match_indices("var(--seele-") {
        let resto = &folhas[posicao + "var(".len()..];
        let nome: String = resto
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '-')
            .collect();
        // Com reserva, a ausência já tem resposta escrita.
        if resto[nome.len()..].trim_start().starts_with(',') {
            continue;
        }
        if !declarados.contains(&nome) {
            sem_declaracao.push(nome);
        }
    }
    sem_declaracao.sort();
    sem_declaracao.dedup();
    assert!(
        sem_declaracao.is_empty(),
        "uma folha usa token que ninguém declara — a propriedade some e o \
         elemento fica com o valor herdado, sem nada avisar: {sem_declaracao:?}"
    );
}

#[test]
fn toda_classe_que_o_script_aplica_tem_regra_de_css() {
    // **Nasceu de um defeito de campo, e o sintoma não parecia de código.**
    //
    // A renomeação de 2026-08-25 trocou `elemento("li", "cage")` por
    // `elemento("li", "voice room")` — a forma em **prosa**, com espaço. Em CSS
    // um espaço separa duas classes, então cada sala virava `voice` mais `room`,
    // nenhuma das duas com regra: o `<li>` perdia padding, borda e fundo, e a
    // lista aparecia grudada no canto esquerdo. Nada quebrou, nada avisou, e o
    // build ficou verde.
    //
    // O guarda lê o **segundo argumento** de cada `elemento(...)` — a classe — e
    // cobra que todo token dela tenha regra. Ler o argumento na posição certa é
    // o que separa isto de um teste inútil: uma varredura por qualquer literal
    // acusaria o **texto** do terceiro argumento, e uma que só olhasse literais
    // colados à vírgula perderia o ternário, que é exatamente onde o defeito
    // estava.
    let folhas = styles() + &read("ui/tokens.css");
    let mut regras: BTreeSet<String> = BTreeSet::new();
    for pedaco in folhas.split('.').skip(1) {
        let nome: String = pedaco
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if !nome.is_empty() {
            regras.insert(nome);
        }
    }

    let script = without_comments(&scripts());
    let mut sem_regra: Vec<String> = Vec::new();
    let mut chamadas = 0_usize;

    // **`className = "..."` também conta.** O guarda lia só `elemento(`, e um
    // arquivo novo que montasse por `createElement` punha classe sem regra na
    // tela sem ninguém notar — que é exatamente o defeito de campo descrito
    // acima, de volta por uma porta que o guarda não olhava. Aconteceu com
    // `mods-regiao.js` no dia em que ele nasceu.
    for (posicao, _) in script.match_indices(".className = \"") {
        let resto = &script[posicao + ".className = \"".len()..];
        let Some(fim) = resto.find('"') else { continue };
        chamadas += 1;
        for token in resto[..fim].split_whitespace() {
            if !regras.contains(token) {
                sem_regra.push(token.to_owned());
            }
        }
    }

    for (posicao, _) in script.match_indices("elemento(") {
        // Fatiado por **byte**, que é o que `match_indices` devolve. Indexar
        // um vetor de `char` com esse número trata acento como dois passos, e
        // o separador sai devorando o arquivo — foi o primeiro estado deste
        // guarda, e ele acusou texto de outra função inteiramente.
        let args = argumentos_da_chamada(&script[posicao + "elemento".len()..]);
        let Some(classe) = args.get(1) else { continue };
        chamadas += 1;
        for literal in classe.split('"').skip(1).step_by(2) {
            for token in literal.split_whitespace() {
                if !regras.contains(token) {
                    sem_regra.push(format!("`{token}` (de \"{literal}\")"));
                }
            }
        }
    }

    assert!(
        chamadas > 20,
        "só {chamadas} chamadas a `elemento(` com classe foram lidas, o que é \
         pouco demais — o separador de argumentos provavelmente parou de casar e \
         este teste está passando por não olhar nada"
    );
    assert!(
        sem_regra.is_empty(),
        "o script aplica classe que nenhuma folha estiliza — o elemento aparece \
         sem espaçamento nenhum e nada avisa:\n  {}",
        sem_regra.join("\n  ")
    );
}

#[test]
fn todo_argumento_que_o_frontend_manda_existe_no_comando() {
    // **O guarda irmão do de baixo, e ele nasceu de um defeito de campo.**
    //
    // Aquele confere o **nome do comando**; este confere os **argumentos**, e a
    // diferença custou uma sala em que ninguém conseguia entrar. A renomeação de
    // 2026-08-25 trocou o parâmetro `cage` por `voice_room`, e o JS continuou
    // mandando `voice_room` — mas o `#[tauri::command]` procura `voiceRoom`,
    // porque `ArgumentCase::Camel` é o padrão do macro.
    //
    // **Por que isso nunca tinha aparecido:** até aquele dia, todo argumento do
    // app era uma palavra só — `fonte`, `senha`, `person`, `channel` — e para
    // uma palavra só snake_case e camelCase são a mesma string. O primeiro
    // argumento de duas palavras foi o primeiro a poder quebrar.
    //
    // O sintoma é o pior tipo: build verde, testes verdes, e um `invoke` que
    // rejeita em silêncio com uma mensagem que ninguém lê.
    let script = scripts();
    let source = read("src/main.rs");
    let comandos = command_parameters(&source);

    let mut erros: Vec<String> = Vec::new();
    for (comando, _, chave) in invoked_with_arguments(&script) {
        let Some(parametros) = comandos.get(&comando) else {
            continue; // o guarda de baixo cuida de comando inexistente
        };
        let esperados: BTreeSet<String> =
            parametros.iter().map(|p| como_o_tauri_procura(p)).collect();
        if !esperados.contains(&chave) {
            erros.push(format!(
                "invoke(\"{comando}\") manda `{chave}`, e o comando procura {esperados:?}"
            ));
        }
    }

    assert!(
        erros.is_empty(),
        "o frontend manda argumento que o comando não procura — o `invoke` \
         rejeita em silêncio e a ação simplesmente não acontece:\n  {}",
        erros.join("\n  ")
    );
}

#[test]
fn every_command_the_frontend_calls_is_registered() {
    // A typo here is a promise rejected at runtime with a message nobody reads,
    // in a build that succeeded.
    let script = scripts();
    let source = read("src/main.rs");

    let called = invoked_commands(&script);
    let registered = registered_commands(&source);

    assert!(!called.is_empty(), "the frontend calls nothing at all");

    let missing: Vec<&String> = called.difference(&registered).collect();
    assert!(
        missing.is_empty(),
        "the frontend calls commands that main.rs does not register: {missing:?}\n\
         registered: {registered:?}"
    );
}

/// Commands whose Rust half is finished and whose screen is not drawn yet.
///
/// Empty is the resting state. An entry here is a promise that a screen is
/// coming, and it is meant to be deleted by whoever draws it — the assertions
/// below make sure it is: the moment the page calls one of these, this test
/// fails and says to take it off the list. Nothing rots quietly.
///
/// The seven of ADR 0030 were here for exactly one commit, and this is the note
/// they left: `camada-portaria.js` draws all of them, so the check below said so
/// by name and they came out. That is the list working the way it is meant to.
// **A lista está vazia, que é o estado de repouso dela.**
//
// `renomear_voice_room` saiu na 0.9.0: a comp faz do **nome da sala** o botão que
// renomeia, e o diálogo de nomear existe desde `bbe45b3`. A pilha inteira já
// estava construída — protocolo, servidor, núcleo, FFI e comando do app —, e o
// que faltava era exatamente o que esta lista existe para lembrar.
//
// `renomear_linha` saiu na 0.10.1, e vale registrar por que a espera acabou, já
// que as duas objeções escritas aqui eram específicas:
//
// A primeira dizia que renomear pertence **à fileira**, e que toda fileira de
// `#lista-linhas` é jogada fora e reconstruída duas vezes por segundo — um campo
// ali não segura cursor. Continua verdade, e por isso o controle não foi para a
// fileira: foi para a **barra do canal aberto**, onde o nome é título e não
// navegação. Na lista o nome já é o botão que abre o canal; na barra o alvo está
// livre. O preço é renomear só o canal em que se está.
//
// A segunda rejeitava o diálogo, por ser «pura distância entre a pessoa e a sala
// que ela renomeia». Essa objeção já tinha sido vencida pelos fatos: a 0.9.0
// renomeia sala de voz por esse mesmo diálogo, e ninguém reclamou da distância.
// O que a nota rejeitava era o diálogo **da moderação**, largo o bastante para
// soletrar uma consequência — e `abrirNomear` não é ele.
// `assistir` esteve aqui por dois commits — o servidor e o núcleo prontos, a
// tela não — e saiu quando `palco-imagem.js` passou a desenhar a escolha entre
// transmissões. A lista voltou ao estado de repouso, que é vazia.
// `habilitar_mod` e `desabilitar_mod` entraram com o ADR 0045: o núcleo, o
// FFI e o banco estão prontos, e a tela que liga e desliga MOD é de um plano
// posterior. `mods_instalados` **não** está aqui — o `base.js` já o chama para
// carregar os MODs habilitados, que é a metade que existe hoje.
// `aceite_de_mods`, `aceitar_mods` e `esquecer_aceite_de_mods` entram pelo outro
// lado do mesmo ADR 0045: os três de cima são de quem hospeda e decidem o que a
// sala exige; estes três são de quem entra e registram o que esta máquina
// respondeu. O protocolo, o servidor, o núcleo e o FFI estão prontos — a lista
// já chega à janela pelo `ConnectionError::ModsNaoAceitos`, e o `frases.js` a
// desenha —, e o que falta é a tela com o botão de aceitar, que é de um plano
// posterior junto com o download dos bytes.
//
// **Por que registrá-los antes da tela**, em vez de esperar por ela: sem eles a
// pergunta que a janela já mostra não tem resposta possível. A pessoa lê o que o
// servidor exige e não tem o que fazer com a leitura — que é meia funcionalidade
// entregue como se fosse uma.
// **A lista voltou a ser vazia em 2026-09-17**, e o estado de repouso dela é
// este. As cinco entradas que estavam aqui saíram juntas, com as duas telas que
// faltavam: a camada de aceite (`camada-mods.js`), que é de quem entra, e a
// seção MODS de CONFIGURAÇÕES, que é de quem hospeda mais a metade que vale
// para todo mundo — desfazer um sim já dado.
//
// `aceite_de_mods` foi a última a sair, e por pouco ela teria ficado com um
// comentário dizendo que o guarda de baixo a cobrava — o que é falso: estar
// nesta lista é justamente o que impede aquele guarda de vê-la. Ela ganhou o
// chamador que faltava, e ele é o aviso que o protótipo desenha: quando esta
// máquina já disse sim a **outra** lista deste mesmo servidor, a tela diz isso.
const AGUARDANDO_TELA: &[&str] = &[];

#[test]
fn no_command_is_registered_and_never_called() {
    // The other direction. A command nobody calls is either dead weight or a
    // feature that was wired on one side only — and the second is the one worth
    // catching.
    let called = invoked_commands(&scripts());
    let registered = registered_commands(&read("src/main.rs"));

    let unused: Vec<&String> = registered
        .difference(&called)
        .filter(|name| !AGUARDANDO_TELA.contains(&name.as_str()))
        .collect();
    assert!(
        unused.is_empty(),
        "main.rs registers commands the frontend never calls: {unused:?}\n\
         If one of these is deliberately waiting for a screen, say so in \
         AGUARDANDO_TELA rather than deleting this check."
    );
}

#[test]
fn nothing_waits_for_a_screen_that_already_draws_it() {
    // What keeps the exception above from becoming a hole. The list has to shrink
    // on its own, and the person who makes it shrink is the one who wires the
    // screen — they will be told here, by name, on the run where it starts
    // working.
    let called = invoked_commands(&scripts());
    let registered = registered_commands(&read("src/main.rs"));

    for esperando in AGUARDANDO_TELA {
        assert!(
            !called.contains(*esperando),
            "the page calls `{esperando}` now, so it is not waiting for a screen any more: \
             take it out of AGUARDANDO_TELA and let the check above cover it again"
        );
        assert!(
            registered.contains(*esperando),
            "AGUARDANDO_TELA names `{esperando}`, which main.rs does not register. \
             A waiting list that names nothing is a waiting list nobody is reading"
        );
    }
}

#[test]
fn every_element_the_script_reaches_for_exists_in_the_page() {
    // `$("nao-existe")` returns null, and the next channel throws. In a page with
    // no build step, nothing else would have said so.
    let script = scripts();
    let page = read("ui/index.html");

    let mut wanted = BTreeSet::new();
    for piece in script.split("$(\"").skip(1) {
        if let Some(id) = piece.split('"').next() {
            wanted.insert(id.to_owned());
        }
    }
    // Ids built at runtime from a list literal, which the split above misses.
    for piece in script.split("$(id)").skip(1) {
        let _ = piece;
    }
    for piece in script.split('"').filter(|piece| piece.starts_with("sub-")) {
        wanted.insert(piece.to_owned());
    }

    assert!(!wanted.is_empty(), "the script reaches for nothing at all");

    let missing: Vec<&String> = wanted
        .iter()
        .filter(|id| !page.contains(&format!("id=\"{id}\"")))
        .collect();
    assert!(
        missing.is_empty(),
        "the script reads element ids that index.html does not define: {missing:?}"
    );
}

/// **Escopo, desde o ADR 0045:** este guarda vale sobre `self` — os arquivos
/// embutidos em `ui/`. Bytes servidos sob `mod://` não passam por aqui e têm o
/// guarda deles em `apps/seele-app/src/mods.rs`: nada é servido que o manifesto
/// do MOD não declare, e nada sobe de diretório. A emenda é de alcance e não de
/// rigor — nenhum arquivo de MOD entra em `ui/`.
#[test]
fn the_page_loads_only_files_that_are_shipped() {
    // The CSP is `default-src 'self'`, so anything external is blocked at
    // runtime rather than at build time — a broken stylesheet reference would
    // simply render an unstyled window.
    let page = read("ui/index.html");

    // Both directions, and both matter now that the frontend is eleven files
    // instead of four.
    //
    // Outwards: everything the page names has to be on disk. `fontes.css` is
    // the one whose absence would be hardest to notice — the page would still
    // render, in Arial Narrow. `tests/fontes.rs` guards what is inside it.
    //
    // Inwards: every sheet and every script in `ui/` has to be *loaded*. This
    // is the half the split made necessary. A `tela-chamada.css` that nobody
    // links is a screen with no styling and no error anywhere, and the old
    // hard-coded list of four would have said nothing about it.
    //
    // The attribute, not the bare name, and on the comment-stripped page. A
    // bare `contains` was an implicit tag check only while each name appeared
    // exactly once in the file; the comment above the `<link>` block names
    // several of these files in prose, and with that the assertion became
    // satisfiable by the explanation of the tag it was meant to guard. Deleting
    // the `fontes.css` link once left all nineteen tests green. Same defect
    // class as the one `without_comments` exists for, one file over: a guard a
    // comment can satisfy is a guard that cannot fail.
    let stylesheets = linked_assets(&page, "href");
    let sources = linked_assets(&page, "src");

    for asset in stylesheets.iter().chain(sources.iter()) {
        assert!(
            app_dir().join("ui").join(asset).exists(),
            "index.html loads {asset}, which is not in ui/"
        );
    }

    for (extension, loaded) in [(".css", &stylesheets), (".js", &sources)] {
        for shipped in ui_files(extension) {
            assert!(
                loaded.contains(&shipped),
                "ui/ ships {shipped}, and index.html never loads it — so it is a \
                 file nobody sees and every guard in this suite reads"
            );
        }
    }

    assert!(
        !page.contains("http://") && !page.contains("https://"),
        "the page references something off the machine, which the CSP blocks"
    );
}

#[test]
fn the_shared_layer_loads_before_the_screens_and_accessibility_loads_last() {
    // Splitting one stylesheet into six reorders every rule in it, and order is
    // what decides a specificity tie. Two ties in this page are decided by
    // nothing else:
    //
    // - `acessibilidade.css` sets `.rotulo`, `.painel-titulo` and
    //   `.lista .pessoa` under `prefers-contrast: more`, against the very rules
    //   it is correcting, at the same specificity. Move that file up and the
    //   high-contrast mode stops working — silently, and only for the people who
    //   asked for it.
    // - every screen sheet refines a primitive from `base.css` at equal or
    //   higher specificity (`.compor input`, `.busca .botao-fantasma`,
    //   `.visitados-titulo` over `.rotulo`). Move `base.css` down and those
    //   refinements lose.
    //
    // The scripts have the same shape of hazard for a different reason: they
    // share one global scope, so what changes when they are split is not
    // visibility but *when* each name comes to exist. `base.js` is executed by
    // every other file's top level; a screen that loads before it registers a
    // listener on a function that is not there yet, and the page dies on load.
    let page = read("ui/index.html");
    let sheets = linked_assets(&page, "href");
    let sources = linked_assets(&page, "src");

    let position = |list: &[String], name: &str| {
        list.iter()
            .position(|entry| entry == name)
            .unwrap_or_else(|| panic!("index.html never loads {name}"))
    };

    let base = position(&sheets, "base.css");
    let accessibility = position(&sheets, "acessibilidade.css");
    // Every sheet between the two ends, and not only the ones named `tela-`.
    // The prefix was the rule while every sheet was a screen; the alert and the
    // battery are *layers* over the session rather than screens of their own,
    // so they are named `camada-` — and under the old filter they were the one
    // kind of file this check silently skipped. A guard that has to be told
    // about each new naming convention is a guard that stops holding on the
    // first convention somebody adds.
    for (at, sheet) in sheets.iter().enumerate() {
        // `href` also carries the favicon, which is an SVG and has no place in a
        // cascade order at all. `tokens.css` and `fontes.css` declare rather
        // than paint and belong *before* `base.css` on purpose — the same two
        // `styles()` leaves out, for the same reason.
        if !sheet.ends_with(".css")
            || matches!(
                sheet.as_str(),
                "base.css" | "acessibilidade.css" | "tokens.css" | "fontes.css"
            )
        {
            continue;
        }
        assert!(
            at > base,
            "{sheet} is loaded before base.css, so every rule it refines wins over it"
        );
        assert!(
            at < accessibility,
            "{sheet} is loaded after acessibilidade.css, which silently turns off \
             the high-contrast rules that only win by being last"
        );
    }
    let last_sheet = sheets
        .iter()
        .rposition(|entry| entry.ends_with(".css"))
        .unwrap_or_default();
    assert_eq!(
        accessibility, last_sheet,
        "acessibilidade.css is not the last stylesheet the page loads"
    );

    let base = position(&sources, "base.js");
    for (at, source) in sources.iter().enumerate() {
        // Os arquivos de MOD são as exceções, e elas são deliberadas: eles
        // declaram as classes e as funções que `base.js` **constrói e chama** —
        // um `class` está na zona morta até o script dele rodar, e construí-lo
        // antes disso lançaria. Nenhum deles chama nada de `base.js` ao
        // carregar; as chamadas que eles fazem acontecem dentro do ciclo de uma
        // instância, que é sempre depois de tudo estar de pé.
        //
        // Entre eles a ordem também importa, e está logo abaixo: o renderer usa
        // `aplicarEstiloDeMod`, e as superfícies constroem `RegiaoDeMod`.
        if !source.ends_with(".js")
            || source == "base.js"
            || source == "mods-runtime.js"
            || source == "mods-estilos.js"
            || source == "mods-regiao.js"
            || source == "mods-superficies.js"
            || source == "mods-contribuicoes.js"
        {
            continue;
        }
        assert!(
            at > base,
            "{source} is loaded before base.js, whose helpers it calls the moment \
             it registers a listener"
        );
    }

    // **A ordem entre os arquivos de MOD é ordem de verdade**, e não arrumação.
    //
    // `mods-regiao.js` chama `aplicarEstiloDeMod` e `classesDeMod`, que são
    // declaradas em `mods-estilos.js`; `mods-superficies.js` **constrói**
    // `RegiaoDeMod` e lê `PERFIS_DE_RENDER`, que estão em `mods-regiao.js`. Uma
    // `class` está na zona morta até o script dela rodar, e uma `const` de topo
    // também: inverter qualquer um destes pares lança na primeira superfície
    // que alguém abrir, e não no carregamento — que é onde ninguém procuraria.
    for (antes, depois, porque) in [
        (
            "mods-estilos.js",
            "mods-regiao.js",
            "o renderer chama `aplicarEstiloDeMod` e `classesDeMod`",
        ),
        (
            "mods-regiao.js",
            "mods-superficies.js",
            "uma superfície constrói `RegiaoDeMod` com `PERFIS_DE_RENDER`",
        ),
    ] {
        assert!(
            position(&sources, antes) < position(&sources, depois),
            "{depois} é carregado antes de {antes}, e {porque}"
        );
    }
}

/// Strips `//`, `/* */` and HTML comments.
///
/// The checks below are about what the code *does*. A comment explaining that
/// the frontend must not know what an `ssrc` is would otherwise fail the test
/// that enforces it — and, in the other direction, a doc comment naming a
/// verdict would satisfy the test that demands the verdict be *handled*. Block
/// comments are stripped for that second reason: every explanation in
/// `seele.js` is a `/** */`, and a guard a comment can satisfy is a guard that
/// cannot fail.
/// The markup with one `<section id="...">` taken out.
///
/// For a rule that holds over a screen **except** one panel, where the panel
/// has a written reason of its own. Cutting by the section keeps the rest of
/// the screen under the whole rule, rather than weakening the rule for all of
/// it.
fn sem_secao(markup: &str, id: &str) -> String {
    let marca = format!("id=\"{id}\"");
    let Some(dentro) = markup.find(&marca) else {
        return markup.to_owned();
    };
    // From the `<section` that carries the id to the `</section>` that closes it.
    let inicio = markup[..dentro].rfind('<').unwrap_or(dentro);
    let fim = markup[dentro..]
        .find("</section>")
        .map_or(markup.len(), |onde| dentro + onde + "</section>".len());
    format!("{}{}", &markup[..inicio], &markup[fim..])
}

fn without_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        match rest[start..].find("-->") {
            Some(end) => rest = &rest[start + end + 3..],
            None => return out,
        }
    }
    out.push_str(rest);

    let mut without_blocks = String::with_capacity(out.len());
    let mut rest = out.as_str();
    while let Some(start) = rest.find("/*") {
        without_blocks.push_str(&rest[..start]);
        match rest[start..].find("*/") {
            Some(end) => rest = &rest[start + end + 2..],
            None => return without_blocks,
        }
    }
    without_blocks.push_str(rest);
    let out = without_blocks;

    out.lines()
        .map(|channel| match channel.find("//") {
            // As três aspas, e não só a dupla. `mod://localhost` numa crase
            // escapava desta proteção: a linha era cortada em `` `mod: ``, o
            // literal ficava aberto, e `sem_texto` engolia tudo até a próxima
            // crase. O efeito era um guarda que parava de guardar no meio do
            // `base.js` — funções declaradas depois dali viravam fantasmas, e
            // um fantasma de verdade ali passaria sem ser visto.
            Some(at) if !channel[..at].contains(['"', '`', '\'']) => &channel[..at],
            _ => channel,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_highlight_ranges_land_on_the_term_inside_the_body_the_page_draws() {
    // Both halves of one invariant, and asserting one substring on one side of
    // it is how a highlight breaks with a green suite: read the wrong field
    // names in JS and the ranges arrive `undefined`, every `<mark>` comes out
    // empty, and a textual guard never notices.
    //
    // So this runs the arithmetic instead. `.mensagens .corpo` is
    // `white-space: pre-wrap`, so the page draws the body with its newlines
    // intact and `corpoComRealce` slices exactly that string by character.
    // Reintroduce `normalize` on either side and the slice stops being the
    // term.
    //
    // The body needs a *run* of whitespace, and this is the whole subtlety:
    // `normalize` rewrites `n` whitespace characters as one, so a single `\n`
    // between words collapses to a single space and every offset stays put. A
    // body like `"linha um\nlinha dois"` therefore passes this test whether or
    // not `normalize` is in the way — it is blind to exactly the defect being
    // guarded. Two spaces and two newlines is what makes the offsets move.
    let body = "linha  um\n\nlinha dois";
    let search = seele_ffi::search::Search::new([body], "linha");

    assert_eq!(
        search.matches().len(),
        2,
        "both occurrences should be found across the channel break"
    );

    for found in search.matches() {
        let sliced: String = body
            .chars()
            .skip(found.start)
            .take(found.end - found.start)
            .collect();
        assert_eq!(
            sliced, "linha",
            "the range {}..{} does not land on the term in the body the page draws; \
             one side of the bridge is collapsing whitespace and the other is not",
            found.start, found.end
        );
    }
}

#[test]
fn neither_side_of_the_bridge_collapses_whitespace() {
    // The test above proves the arithmetic on raw bodies. This is what keeps
    // both callers on raw bodies — the failure it guards is a silent one-per-run
    // offset drift, invisible until somebody pastes a message with two channels.
    let source = read("src/main.rs");
    let script = scripts();

    assert!(
        !source.contains("search::normalize"),
        "`buscar` normalises the bodies, but the page draws them raw"
    );
    assert!(
        !script.contains("split(/\\s+/)"),
        "the page collapses the body, but the ranges index the raw one"
    );
}

#[test]
fn the_script_reads_the_field_names_a_match_actually_serialises() {
    // `Match` carries no `serde(rename)`, so the payload says `message`,
    // `start`, `end`. Portuguese names would destructure to `undefined`, slice
    // to nothing, and paint empty marks — with no error anywhere.
    //
    // **Scoped to the screen that draws matches**, and not to every script.
    // The ban used to be on the bare substring `inicio` anywhere in the
    // frontend, and it caught an offset parameter in the MODs' file API — a
    // guard that is right about one screen and blind to which screen it is
    // will keep stopping work it was never about.
    let script = without_comments(&read("ui/tela-sessao.js"));

    for field in ["message", "start", "end"] {
        assert!(
            script.contains(field),
            "the script never names `{field}`, which is what a Match serialises to"
        );
    }
    for wrong in [".mensagem", ".inicio", ".fim"] {
        assert!(
            !script.contains(wrong),
            "the script reads `{wrong}` off a Match, which serialises no such field"
        );
    }
}

#[test]
fn a_message_arriving_does_not_throw_the_search_back_to_the_first_occurrence() {
    // What `buscar` does on every `MessagesChanged`, with the same types: build
    // the search again over the new history, then put the cursor back.
    //
    // Without the second half the counter snapped to `[1/12]` and the pane
    // jumped to match one every time anybody spoke, which is precisely the
    // conversation where searching is worth doing.
    let before_bodies = ["sync caiu", "o sync voltou", "o sync nem caiu"];
    let mut before = seele_ffi::search::Search::new(before_bodies, "sync");
    before.next_match();
    assert_eq!(
        before.position(),
        (2, 3),
        "the cursor should be on match two"
    );
    let Some(was_on) = before.current() else {
        panic!("a search with three matches has a current one");
    };

    // Somebody speaks. The list is rebuilt and every later index shifts.
    let after_bodies = [
        "sync caiu",
        "o sync voltou",
        "o sync nem caiu",
        "sync de novo",
    ];
    let mut after = seele_ffi::search::Search::new(after_bodies, "sync");
    assert_eq!(
        after.position(),
        (1, 4),
        "a freshly built search starts at one — this is the state being corrected"
    );

    after.resume_at(was_on);
    assert_eq!(
        after.position(),
        (2, 4),
        "the cursor did not stay on the occurrence the reader was on"
    );
    assert_eq!(
        after.current(),
        Some(was_on),
        "the cursor moved to a different occurrence than the one it was on"
    );
}

#[test]
fn the_search_command_puts_the_cursor_back_after_rebuilding() {
    // The test above proves the rule; this is what keeps `buscar` calling it.
    // The rule has to run on the Rust side — the cursor lives in
    // `Session::busca`, and `specs/06-clientes-gui.md:19` keeps decisions like
    // this one out of the frontend.
    let source = read("src/main.rs");
    let script = scripts();

    assert!(
        source.contains("resume_at"),
        "`buscar` rebuilds the search and never restores the cursor, so every \
         incoming message sends the reader back to occurrence one"
    );
    assert!(
        !script.contains("resume_at") && !script.contains("busca.cursor"),
        "the frontend is deciding where the search cursor goes, which is protocol \
         logic in JavaScript"
    );
}

#[test]
fn the_ordinal_indexes_the_same_list_the_page_groups_by_message() {
    // `desenharMensagens` groups the matches by message, in the order they
    // arrive, and lights the `ordinal`-th one of that group. So the ordinal has
    // to be an index into exactly that list — if the core counted any other way
    // the wrong word would be marked, silently and always by the same offset.
    let body = "sync caiu, o sync voltou, e o sync nem caiu";
    let mut search = seele_ffi::search::Search::new(["antes", body], "sync");

    let in_this_message: Vec<_> = search
        .matches()
        .iter()
        .filter(|found| found.message == 1)
        .copied()
        .collect();
    assert_eq!(in_this_message.len(), 3, "the body matches three times");

    for expected in 0..3 {
        assert_eq!(
            search.current().map(|found| found.message),
            Some(1),
            "the walk left the message under test"
        );
        let Some(ordinal) = search.ordinal_in_message() else {
            panic!("a search with a current match has an ordinal");
        };
        assert_eq!(
            ordinal, expected,
            "the ordinal is not counting in drawing order"
        );
        assert_eq!(
            search.current(),
            in_this_message.get(ordinal).copied(),
            "the ordinal does not index the list the page groups by message"
        );
        search.next_match();
    }
}

#[test]
fn the_current_occurrence_is_not_marked_out_by_colour_alone() {
    // `specs/06-clientes-gui.md:144`. The terminal separates the two states with
    // REVERSED against plain accent; the browser has weight and decoration, and
    // has to use one of them. A rule that only swaps hues would leave the
    // cursor invisible on a monochrome display — which is the same failure as
    // not marking it at all, for the people it happens to.
    let script = scripts();
    let css = styles();

    assert!(
        script.contains("realce-atual"),
        "nothing in the page marks the occurrence the cursor is on"
    );
    assert!(
        script.contains("estado.ordinal"),
        "the script never reads the ordinal, so it cannot know which match is the current one"
    );
    assert!(
        read("src/main.rs").contains("ordinal_in_message"),
        "the bridge never sends which occurrence inside its message the cursor is on"
    );

    let rule = css
        .split(".realce-atual")
        .nth(1)
        .and_then(|rest| rest.split('}').next())
        .unwrap_or_default();
    assert!(
        !rule.is_empty(),
        "the stylesheet has no rule for the current occurrence"
    );
    assert!(
        rule.contains("font-weight") || rule.contains("outline") || rule.contains("border"),
        "the current occurrence differs from the others only by colour:\n{rule}"
    );
}

#[test]
fn a_lista_de_servidores_vazia_diz_por_que_esta_vazia() {
    // **A lista de visitados saiu da entrada na 0.9.0** e virou o diálogo `ONDE
    // VOCÊ JÁ ESTEVE`. O guarda anterior cobrava que a seção nascesse `hidden`,
    // porque uma lista vazia piscando antes de carregar lê como defeito.
    //
    // No diálogo o problema é outro e maior: ele **abre** vazio para quem nunca
    // entrou em servidor nenhum, e uma caixa sem linhas e sem explicação é a
    // primeira coisa que essa pessoa vê do produto.
    //
    // Então o que se cobra deixou de ser o `hidden` e passou a ser a frase.
    let pagina = without_comments(&read("ui/index.html"));
    let script = without_comments(&scripts());

    assert!(
        pagina.contains("id=\"servidores-vazio\""),
        "sumiu a frase que explica a lista vazia; sem ela o diálogo lê como defeito"
    );
    assert!(
        script.contains("servidores-vazio"),
        "a frase existe e ninguém a mostra nem a esconde: ou ela fica sempre, \
         ou nunca, e as duas estão erradas"
    );
}

#[test]
fn the_event_name_the_script_branches_on_is_the_one_the_bridge_sends() {
    // The listener rebuilds a live search only when the message list changed —
    // an edit rewrites a body in place and a removal shortens the list, and
    // either one leaves cached ranges pointing at different text.
    //
    // That branch is a string compared against whatever serde makes of a unit
    // variant. Rename the variant and the branch simply stops firing: no error,
    // no warning, and a search that quietly goes stale again.
    let sent = serde_json::to_string(&seele_ffi::Event::MessagesChanged)
        .expect("the event must serialise to cross the bridge at all");
    assert_eq!(
        sent, "\"MessagesChanged\"",
        "the bridge no longer sends the string the script branches on"
    );

    assert!(
        scripts().contains("payload === \"MessagesChanged\""),
        "nothing in the script rebuilds the search when the message list changes"
    );
}

/// The body of a top-level `fn`, comments stripped.
///
/// Two of the checks below are about one statement being inside one function,
/// and `main.rs` as a whole says the word either way — the paragraph explaining
/// why the invite dies in `disconnect` would satisfy a search for `convite`
/// long after the channel itself was deleted. Scoping and stripping is what makes
/// those checks able to fail.
///
/// `\n}\n` terminates: every brace inside a body is indented.
fn body_of(source: &str, signature: &str) -> String {
    let Some(after) = source.split(signature).nth(1) else {
        panic!("main.rs no longer has `{signature}`");
    };
    let Some(body) = after.split("\n}\n").next() else {
        panic!("unterminated `{signature}`");
    };
    without_comments(body)
}

/// Does the text name `word` on its own, rather than inside a longer name?
///
/// The distinction is the whole point of the test below. `FirstContact` is a
/// prefix of `FirstContactVerified`, so a plain `contains` would call a verdict
/// handled when the only thing left in the file is the *other* one — which is
/// precisely the deletion this has to catch.
fn names(text: &str, word: &str) -> bool {
    let edge = |character: Option<char>| {
        character.is_none_or(|character| !character.is_alphanumeric() && character != '_')
    };
    text.match_indices(word).any(|(at, _)| {
        edge(text[..at].chars().next_back()) && edge(text[at + word.len()..].chars().next())
    })
}

/// Todo estado de aparelho que a ponte sabe mandar.
///
/// A lista é conferida por um `match` sem braço coringa logo abaixo dela: uma
/// variante nova **não compila** até entrar aqui, e é assim que o guarda deixa
/// de ser uma lista que alguém esquece de estender.
fn todos_os_estados_do_aparelho() -> Vec<seele_ffi::EstadoDoAparelho> {
    use seele_ffi::EstadoDoAparelho::{Funcionando, Perdido, Trocando};
    let todos = vec![Funcionando, Trocando, Perdido];
    for estado in &todos {
        match estado {
            Funcionando | Trocando | Perdido => {}
        }
    }
    todos
}

#[test]
fn todo_estado_do_aparelho_que_a_ponte_manda_chega_dito_na_tela() {
    // O defeito que esta tarefa conserta era o produto saber e não contar: o
    // aparelho mudava, o núcleo sabia, e a tela não dizia nada. Um estado novo
    // sem frase seria o mesmo silêncio de volta, por outro caminho.
    //
    // Os nomes saem da **serialização de verdade** do `EstadoDoAparelho`, e não
    // de uma lista de textos: renomear a variante em Rust quebra isto, em vez
    // de deixar um ramo morto no script. Comentários são retirados antes —
    // `body_of` faz isso — porque o comentário desta própria função nomeia os
    // três estados em prosa, e sem tirá-lo apagar o ramo deixaria o guarda
    // verde na força de uma explicação.
    let corpo = body_of(&scripts(), "function desenharAparelho(snapshot)");

    for estado in todos_os_estados_do_aparelho() {
        let Ok(json) = serde_json::to_string(&estado) else {
            panic!("EstadoDoAparelho não serializa: nenhuma casca consegue lê-lo");
        };
        let nome = json.trim_matches('"');

        if estado == seele_ffi::EstadoDoAparelho::default() {
            assert!(
                !names(&corpo, nome),
                "«{nome}» é o normal e não se anuncia: uma frase permanente na \
                 tela vira moldura e ninguém mais a lê"
            );
            continue;
        }
        assert!(
            names(&corpo, nome),
            "o estado «{nome}» não tem frase nenhuma na tela, então ele chega \
             como silêncio — que é exatamente o defeito de origem"
        );
    }
}

#[test]
fn a_tela_le_do_instantaneo_os_campos_que_o_rust_realmente_manda() {
    // `Snapshot` não carrega `serde(rename)`, então o nome do campo em Rust é
    // o nome que chega ao script. Um campo renomeado de um lado só faz o
    // script ler `undefined` — sem erro nenhum, e com a tela desenhando o
    // estado de sempre enquanto o aparelho muda por baixo.
    //
    // O fecho abaixo não é enfeite: ele **nomeia os campos em Rust**, então
    // renomear qualquer um deles impede este arquivo de compilar, e quem
    // renomeia é obrigado a passar aqui — e daqui, pelo script.
    // O estado e o contador dizem **que** o aparelho mudou; quem diz **para
    // onde** é o nome do aparelho aberto, e é ele que a frase da troca carrega.
    // Deixá-lo fora deste fecho era deixar metade do aviso solta: renomear
    // `playback`, `capture` ou o `name` de dentro deles apagaria o nome da
    // frase sem quebrar nada, e a tela voltaria a dizer só «o aparelho mudou».
    fn _liga_os_dois_lados(
        instantaneo: &seele_ffi::Snapshot,
    ) -> (seele_ffi::EstadoDoAparelho, u64, Option<&str>, Option<&str>) {
        (
            instantaneo.aparelho,
            instantaneo.trocas_de_aparelho,
            instantaneo.playback.as_ref().map(|onde| onde.name.as_str()),
            instantaneo.capture.as_ref().map(|onde| onde.name.as_str()),
        )
    }
    let corpo = body_of(&scripts(), "function desenharAparelho(snapshot)");

    for campo in [
        "aparelho",
        "trocas_de_aparelho",
        "playback",
        "capture",
        "name",
    ] {
        assert!(
            names(&corpo, campo),
            "a tela não lê `{campo}`, que é o que o instantâneo manda"
        );
    }
}

#[test]
fn o_aviso_do_aparelho_nao_fica_solto_do_desenho_da_tela() {
    // Os dois guardas acima conferem o **conteúdo** de `desenharAparelho`, e
    // nenhum deles confere que alguém a chama. Apagar a chamada deixava a
    // bateria inteira verde com o aviso fora da tela — o mesmo «o produto sabe
    // e não conta» que esta tarefa existe para consertar, de volta por outro
    // caminho.
    //
    // Por isso a corrente inteira, e não só o último elo: a volta do relógio
    // (`desenhar`) tem de chegar à telemetria, e a telemetria tem de chegar ao
    // aparelho. Cortar qualquer um dos dois apaga o aviso da tela.
    let elos = [
        ("function desenhar(snapshot)", "desenharTelemetria"),
        ("function desenharTelemetria(snapshot)", "desenharAparelho"),
    ];

    for (de, para) in elos {
        let corpo = body_of(&scripts(), de);
        assert!(
            names(&corpo, para),
            "`{de}` não chama `{para}`: o estado do aparelho pode ser desenhado \
             por ninguém, e a tela volta a calar sobre a troca"
        );
    }
}

#[test]
fn every_verdict_the_bridge_can_send_has_its_own_sentence_in_the_page() {
    // A variant with no sentence is a blank screen at the moment it most needs
    // to say something — and this shell spent its whole life pinning keys in
    // silence, so the failure is not hypothetical.
    //
    // It serialises the real `Trust` rather than listing strings, so renaming a
    // variant breaks this instead of leaving a dead branch behind. Comments are
    // stripped first, and that is load-bearing rather than tidy: the doc comment
    // on `fraseDoVeredito` says `Known` in prose, so deleting the branch that
    // handles it would leave this green on the strength of an explanation.
    //
    // `InviteRefused` is left out on purpose — the core drops the connection,
    // so it reaches the shell as a `ConnectionError`. The test below covers it.
    let script = without_comments(&scripts());

    for verdict in [
        seele_ffi::Trust::FirstContact {
            fingerprint: "a".into(),
        },
        seele_ffi::Trust::FirstContactVerified {
            fingerprint: "a".into(),
        },
        seele_ffi::Trust::Known,
        seele_ffi::Trust::InviteDisagrees {
            expected: "b".into(),
            offered: "a".into(),
        },
    ] {
        let Ok(json) = serde_json::to_string(&verdict) else {
            panic!("Trust does not serialise, so no shell can read it at all");
        };
        // The variant name, exactly as serde writes it on the wire.
        let Some(name) = json.trim_matches('"').split("\":").next() else {
            panic!("unexpected shape: {json}");
        };
        let name = name.trim_start_matches('{').trim_matches('"');

        assert!(
            names(&script, name),
            "the verdict {name} has no handling in the script, so it lands on a \
             screen that says nothing"
        );
    }
}

#[test]
fn the_refused_invite_reaches_the_screen_with_both_fingerprints() {
    // The fifth verdict never crosses as a `Trust`: the core drops the
    // connection, so it arrives as the error below and its sentence lives on
    // the `#boot-erro` path. Both prints have to be in it — an accusation that
    // shows one of the two is an accusation nobody can check.
    let refusal = seele_ffi::ConnectionError::InviteMismatch {
        expected: "bbbb".into(),
        offered: "aaaa".into(),
    };
    let Ok(json) = serde_json::to_string(&refusal) else {
        panic!("ConnectionError does not serialise, so no shell can read it at all");
    };
    let script = without_comments(&scripts());

    assert!(
        names(&script, "InviteMismatch"),
        "nothing in the script reads the refusal, so a link that names another \
         Server fails with a sentence about nothing: {json}"
    );
    for field in ["expected", "offered"] {
        assert!(
            json.contains(&format!("\"{field}\"")),
            "the refusal no longer carries `{field}`: {json}"
        );
        assert!(
            names(&script, field),
            "the script never names `{field}`, so the reader gets half a comparison"
        );
    }
}

#[test]
fn the_informative_verdicts_do_not_spend_the_alarm_reserved_for_a_key_change() {
    // `specs/08-seguranca.md` reserves the impossible-to-ignore treatment for a
    // key that changed, and `tokens.css:19` marks the red "EXCLUSIVO alerta e
    // queda". A first contact and a link that names another server stop nobody
    // from entering; dressing them as an alarm is what teaches people to
    // dismiss the alarm on the day it means the other thing.
    let page = without_comments(&read("ui/index.html"));
    let css = without_comments(&styles());

    let tag = page
        .split("id=\"veredito\"")
        .nth(1)
        .and_then(|rest| rest.split('>').next())
        .unwrap_or_default();
    assert!(
        tag.contains("role=\"status\""),
        "the verdict is announced as an alert, which is the treatment reserved \
         for what stops you entering: {tag}"
    );

    let rule = css
        .split(".veredito")
        .nth(1)
        .and_then(|rest| rest.split('}').next())
        .unwrap_or_default();
    assert!(
        !rule.is_empty(),
        "the stylesheet has no rule for the verdict band"
    );
    assert!(
        !rule.contains("vermelho"),
        "the verdict band is painted in the colour the tokens reserve for alarm \
         and collapse:\n{rule}"
    );
}

#[test]
fn the_comparison_stays_in_rust_and_only_its_verdict_crosses() {
    // `specs/06-clientes-gui.md:19`. The fingerprint crosses the bridge now,
    // which is a reversal — but it crosses as the *output* of a decision, to be
    // read by a person. What must never cross is the input: the value to check
    // against comes off `Session::convite` and goes straight into the FFI.
    let script = without_comments(&scripts());
    let connect = body_of(&read("src/main.rs"), "async fn connect");

    // Scoped to the body and blind to the variable that carries the value: what
    // matters is that the field is filled from the stored invite, not what the
    // local is called on the way.
    assert!(
        connect.contains("impressao_digital") && connect.contains("session.convite"),
        "`connect` never reads the invite's fingerprint, so a pasted link is \
         parsed and thrown away exactly as before"
    );
    assert!(
        connect.contains("expected_fingerprint") && !connect.contains("expected_fingerprint: None"),
        "`connect` hands the FFI no fingerprint to check against, so every \
         connection is as blind as a typed address"
    );
    assert!(
        !script.contains("expected_fingerprint") && !script.contains("expectedFingerprint"),
        "the frontend is feeding the comparison its input, which is the \
         comparison moving to JavaScript one argument at a time"
    );
    assert!(
        !script.contains("impressao_digital"),
        "the frontend reads the parsed invite's own field, so the whole `Convite` \
         crossed the bridge instead of the verdict"
    );
    assert!(
        !script.contains("conferencia_pendente"),
        "the script still branches on a pending check that no longer exists, so \
         that branch is dead and the screen it drew is gone"
    );
}

#[test]
fn leaving_forgets_the_invite_that_let_us_in() {
    // Inert while nothing was checked, and not inert any more: the fingerprint
    // that `connect` checks against comes from this slot. Left behind, the next
    // connection to a different server would be checked against the previous
    // link's promise and refused for a reason nobody could explain.
    // A limpeza mudou de casa com o A03 da auditoria: `disconnect` e
    // `desmontar_a_sessao` dividem `desmontar_o_cliente`, e é ele quem recolhe
    // a conexão e o convite. Cobrar no auxiliar é cobrar nas duas saídas de uma
    // vez — e o guarda continua falhando se alguém tirar a linha de lá.
    let source = read("src/main.rs");
    let body = body_of(&source, "fn desmontar_o_cliente");
    assert!(
        body.contains("session.convite"),
        "quem desmonta a sessão larga a conexão mas guarda o convite, e a impressão \
         prometida por um link anterior sobrevive à sessão a que ela pertencia"
    );
    // E as duas saídas passam por ele: uma delas que não passasse levaria o
    // convite adiante sem nada dizer.
    for saida in ["async fn disconnect", "async fn desmontar_a_sessao"] {
        assert!(
            body_of(&source, saida).contains("desmontar_o_cliente"),
            "`{saida}` não desmonta a sessão de cliente"
        );
    }
}

#[test]
fn the_page_never_draws_a_glyph_the_data_face_does_not_have() {
    // The embedded IBM Plex Mono has 1049 cmap entries and exactly one glyph in
    // U+25A0–U+25CF, so every one of these fell through to whatever monospace
    // the machine happens to have — SF Mono, Consolas, something else — putting
    // a second face in the middle of a channel, in an interface whose whole claim
    // is that every channel is a grid.
    //
    // `docs/marca.md` settled the same argument one layer up: the brand's three
    // katakana are outlines and not text, because as text the mark would be
    // Hiragino on macOS and Yu Gothic on Windows. `ui/glifos.js` is that answer
    // applied to the interface's own six.
    //
    // Comments are stripped first, and that is load-bearing rather than tidy:
    // the files that explain these characters have to be able to name them, and
    // a comment draws nothing. What this catches is a seventh screen typing one
    // back into a template string.
    let script = without_comments(&scripts());
    let page = without_comments(&read("ui/index.html"));

    for glyph in [
        '\u{25B8}', // ▸ small right-pointing triangle
        '\u{25C2}', // ◂ small left-pointing triangle
        '\u{25BC}', // ▼ down-pointing triangle
        '\u{25B6}', // ▶ right-pointing triangle
        '\u{25CF}', // ● black circle
        '\u{25CB}', // ○ white circle
        '\u{2318}', // ⌘ place of interest sign
        // The eight the v3 comp introduces. Every one measured against the
        // shipped face rather than assumed from its Unicode block: the `cmap`
        // of `fontes/ibm-plex-mono-400.woff2` has 499 codepoints and none of
        // these is among them. `ui/glifos.js` draws all eight.
        '\u{2315}', // ⌕ telephone recorder — drawn as the search lens
        '\u{2328}', // ⌨ keyboard
        '\u{23FB}', // ⏻ power on/off symbol
        '\u{25A4}', // ▤ square with horizontal fill
        '\u{25CD}', // ◍ circle with vertical fill
        '\u{25E7}', // ◧ square with left half black
        '\u{2699}', // ⚙ gear
        '\u{26BF}', // ⚿ squared key
    ] {
        for (name, text) in [("the scripts", &script), ("index.html", &page)] {
            assert!(
                !text.contains(glyph),
                "{name} draws U+{:04X} as a character, and the data face has no glyph \
                 for it — it falls through to the system monospace, mid-channel. \
                 `glifo()` in ui/glifos.js draws it instead.",
                u32::from(glyph)
            );
        }
    }
}

#[test]
fn the_microphone_list_reads_the_field_names_a_device_actually_serialises() {
    // Same defect class as the `Match` guard above, one screen over. The picker
    // is untyped JavaScript reading three names off the wire; a `serde(rename)`
    // or a Portuguese field name would draw every row `undefined`, send back
    // `undefined` when one is clicked, and fail nowhere.
    let device = seele_ffi::CaptureDevice {
        id: "coreaudio:alguma-coisa".into(),
        name: "Scarlett Solo".into(),
        default: true,
    };
    let Ok(json) = serde_json::to_string(&device) else {
        panic!("CaptureDevice does not serialise, so no shell can read it at all");
    };
    let script = without_comments(&scripts());

    for field in ["id", "name", "default"] {
        assert!(
            json.contains(&format!("\"{field}\"")),
            "a capture device no longer carries `{field}`: {json}"
        );
        assert!(
            names(&script, field),
            "the picker never names `{field}`, which is what a capture device \
             serialises to"
        );
    }
    for wrong in ["dispositivo.nome", "dispositivo.padrao"] {
        assert!(
            !script.contains(wrong),
            "the picker reads `{wrong}` off a capture device, which serialises no \
             such field"
        );
    }
}

#[test]
fn the_picker_sends_back_the_id_and_never_the_name() {
    // The one mistake this screen can make silently. Two microphones of the same
    // model report the same name, so a picker that sent the name back would work
    // on every machine with one interface and pick the wrong device on the
    // machines the feature exists for.
    //
    // Scoped to the function that builds a row, because the file as a whole says
    // both words either way — the paragraph explaining why the id is not shown
    // would otherwise satisfy a search for it.
    let body = body_of(&scripts(), "function linhaDeDispositivo");

    assert!(
        body.contains("dataset.dispositivo = id"),
        "a row no longer carries the id it stands for, so nothing can be sent back"
    );
    assert!(
        !body.contains("escolher(nome"),
        "the picker hands back the name, and two microphones of one model share it"
    );
}

#[test]
fn the_screen_says_which_microphone_is_open_and_not_only_which_was_chosen() {
    // The two diverge exactly when this screen is worth opening: an interface
    // chosen yesterday and unplugged today reads as chosen, while something else
    // is actually capturing. A screen that drew only the preference would call
    // the choice reality and leave somebody talking into the wrong microphone
    // while looking at a row that says it is the right one.
    // Scoped to the one function that marks a row. The file as a whole says both
    // words either way — the paragraph explaining the distinction would satisfy
    // a search for it — and `EM USO` in particular appears in an unrelated
    // sentence about port 8383 two files over, which is enough to make an
    // unscoped `contains` a guard that cannot fail. Found by breaking the code
    // on purpose and watching this pass.
    let body = body_of(&scripts(), "function marcarLinhas");

    assert!(
        body.contains("snapshot?.capture?.id") || body.contains("snapshot.capture.id"),
        "nothing in the picker reads which device actually opened, so it draws the \
         preference and calls it reality"
    );

    // As duas palavras desceram para a função que marca **uma** lista, quando a
    // saída de som passou a ser escolhível e as duas listas passaram a ser
    // desenhadas pelo mesmo código. O que o teste exige não mudou.
    let uma = body_of(&scripts(), "function marcarUmaLista");
    assert!(
        uma.contains("EM USO") && uma.contains("ESCOLHIDO"),
        "the picker has one word for both states, so it cannot show them apart:\n{uma}"
    );

    // And the bridge has to still answer the other half.
    assert!(
        read("src/main.rs").contains("microfone_escolhido"),
        "the bridge no longer answers which microphone was chosen"
    );
}

/// The variants of one `enum` declared in `main.rs`.
///
/// Serde writes a fieldless variant as its own name, so these are exactly the
/// strings the frontend receives when the command rejects.
fn variants_of(source: &str, name: &str) -> Vec<String> {
    let Some(after) = source.split(&format!("enum {name} {{")).nth(1) else {
        panic!("main.rs no longer declares `enum {name}`");
    };
    let Some(block) = after.split("\n}").next() else {
        panic!("unterminated `enum {name}`");
    };
    let found: Vec<String> = without_comments(block)
        .lines()
        .map(str::trim)
        .filter_map(|channel| channel.strip_suffix(','))
        .filter(|channel| {
            !channel.is_empty()
                && channel
                    .chars()
                    .all(|character| character.is_alphanumeric() || character == '_')
        })
        .map(str::to_owned)
        .collect();
    assert!(!found.is_empty(), "`enum {name}` came out with no variants");
    found
}

#[test]
fn every_refusal_the_bridge_writes_itself_has_a_sentence_in_the_page() {
    // Two of this shell's commands answer with an enum of their own rather than
    // a `ConnectionError` — hosting, and choosing a microphone — because their
    // failures are local ones the FFI has no business naming. That freedom costs
    // exactly this guard: a variant added here with no sentence over there lands
    // on a screen that says nothing, or worse, prints the name of a Rust variant
    // at somebody.
    //
    // The refusal most likely to be met is the one this was written for: the
    // list is drawn, an interface is unplugged, and then a row is clicked.
    //
    // Comments are stripped from both sides. The paragraph in `main.rs`
    // explaining why a variant exists must not count as declaring it, and the
    // paragraph in `frases.js` explaining a sentence must not count as writing
    // it.
    let source = read("src/main.rs");
    let script = without_comments(&scripts());

    // `FalhaNaPortaria` joined the two for the same reason: the doorkeeper's
    // commands talk to this machine's own server rather than across the bridge,
    // so the FFI has no name for either of their failures.
    for enumeration in ["FalhaAoHospedar", "FalhaAoEscolher", "FalhaNaPortaria"] {
        for variant in variants_of(&source, enumeration) {
            assert!(
                names(&script, &variant),
                "`{enumeration}::{variant}` reaches the page with no sentence written \
                 for it, so the screen either says nothing or says `{variant}`"
            );
        }
    }
}

#[test]
fn every_stage_of_an_arrival_has_a_sentence_in_the_page() {
    // The mute spinner, as a test. Four candidates were tried in a row behind
    // it, and when the two-house field test failed nobody could say at which
    // point — there was no point with a name. `ConnectStage` gave every instant
    // of that crossing a name; a name with no sentence over here puts the
    // silence back, one stage at a time.
    //
    // Two things are asserted, and the second is the one that would go quiet:
    // that every stage has an entry, and that the key the page files it under
    // is the name serde actually writes. A dictionary keyed by a name nothing
    // sends is a dictionary that is never read.
    let file = without_comments(&read("ui/frases.js"));
    let written: BTreeSet<String> = sentences_of(&file, "ETAPAS")
        .into_iter()
        .map(|(variant, _)| variant)
        .collect();
    assert!(
        !written.is_empty(),
        "`ETAPAS` came out empty, so the loop below is comparing against nothing"
    );

    // The list is `ConnectStage::todas()`, which is `Etapa::TODAS` put through
    // the one `From` every stage crosses by — not a third hand-written copy of
    // it. The copy was the bug: a variant added to the core reached this page
    // and fell through to "FALHA QUE ESTA TELA NÃO SABE NOMEAR" in the middle
    // of a connection that was going fine, and none of the 133 tests lit up,
    // because every list that could have noticed was written out by hand.
    let stages = seele_ffi::ConnectStage::todas();
    assert!(
        stages.len() >= 5,
        "`ConnectStage::todas` came back with {} stages, so this loop is \
         checking almost nothing",
        stages.len()
    );
    for stage in stages {
        let Ok(json) = serde_json::to_string(&stage) else {
            panic!("a stage does not serialise, so no shell can read it at all");
        };
        // The variant name, exactly as serde writes it on the wire.
        let Some(name) = json.trim_matches('"').split("\":").next() else {
            panic!("unexpected shape: {json}");
        };
        let name = name.trim_start_matches('{').trim_matches('"');

        assert_eq!(
            name,
            stage.nome(),
            "the stage crosses as `{name}` and calls itself `{}`, so the page \
             files the sentence under a name nothing sends",
            stage.nome()
        );
        assert!(
            written.contains(name),
            "the arrival can stop at `{name}` and no sentence says what that \
             means, so the screen shows the spinner this whole state machine \
             exists to replace"
        );
    }
}

#[test]
fn every_path_a_connection_can_take_has_a_sentence_in_the_page() {
    // The twin of the stage guard above, on the other list. `Snapshot.caminho`
    // crosses as one of four stable names, and a name with no entry here is a
    // metric that stays at the dash while the session knows perfectly well how
    // it got there.
    //
    // The list is `seele_ffi::caminhos()`, derived from the enum's own `match`,
    // and not a copy written out here. The copy was the bug this cycle already
    // paid for once: three parallel hand-written lists, none of them tied to
    // the enum by the compiler, and a new variant crossing all of them with
    // nothing lighting up.
    let file = without_comments(&read("ui/frases.js"));
    let written: BTreeSet<String> = sentences_of(&file, "CAMINHOS")
        .into_iter()
        .map(|(variant, _)| variant)
        .collect();
    assert!(
        !written.is_empty(),
        "`CAMINHOS` came out empty, so the loop below is comparing against nothing"
    );

    let paths = seele_ffi::caminhos();
    assert_eq!(
        paths.len(),
        4,
        "`seele_ffi::caminhos` came back with {} names and the table in §5 of the \
         spec has four rows, so this loop is checking something else: {paths:?}",
        paths.len()
    );
    for path in &paths {
        assert!(
            written.contains(*path),
            "a connection can arrive by `{path}` and no name says what that is, \
             so the footer keeps the dash it shows when nothing is known"
        );
    }
}

#[test]
fn every_reason_a_screen_share_can_stop_for_has_a_sentence() {
    // The twin of the two guards above, on the list that arrived with screen
    // sharing. `TelaEmCurso::parada` crosses as a stable name — not as a
    // sentence — for the reason the FFI writes beside it: a ready-made
    // Portuguese sentence coming over the bridge would be the one channel of this
    // window the vocabulary guard cannot see, and the one nobody could
    // translate.
    //
    // A name with no entry here draws «A TELA PAROU» and nothing else, which is
    // the shell knowing that something stopped and not why — and the two
    // reasons ask opposite things of the reader: one is the voice winning, and
    // the other is the path having nothing left.
    //
    // The list is `seele_ffi::motivos_de_parada_da_tela()`, tied to that
    // crate's own `match` by a test over there, and not a copy written out
    // here. The copy is the bug this cycle already paid for once.
    let file = without_comments(&read("ui/frases.js"));
    let written: BTreeSet<String> = sentences_of(&file, "PARADAS")
        .into_iter()
        .map(|(variant, _)| variant)
        .collect();
    assert!(
        !written.is_empty(),
        "`PARADAS` came out empty, so the loop below is comparing against nothing"
    );

    let reasons = seele_ffi::motivos_de_parada_da_tela();
    assert!(
        !reasons.is_empty(),
        "`seele_ffi::motivos_de_parada_da_tela` came back empty, so this guard is \
         asserting over nothing"
    );
    for reason in &reasons {
        assert!(
            written.contains(*reason),
            "a screen share can stop for `{reason}` and no sentence says what that \
             is, so the stage says it stopped and never why"
        );
    }
}

#[test]
fn the_screen_never_invents_a_word_for_a_path_it_does_not_know() {
    // «DIRECT» is not sayable, and this is the guard that keeps it unsaid. The
    // ladder has five rungs and the distinction that word would erase is the
    // one that matters: in `FuroDeNat` the conversation **is** direct, and
    // somebody knew it exists.
    //
    // Two halves. The dictionary must not grow the word, and `fraseDeCaminho`
    // must answer with nothing rather than fall through to a name — the
    // fallback every other lookup in this file has, deliberately not taken
    // here, because a metric that prints the name of a Rust variant next to
    // `RTT 41ms` is noise where a failure message would still be a lead.
    let file = without_comments(&read("ui/frases.js"));
    let escritos: BTreeSet<String> = sentences_of(&file, "CAMINHOS")
        .into_iter()
        .map(|(variant, _)| variant)
        .collect();
    let atravessam: BTreeSet<String> = seele_ffi::caminhos()
        .into_iter()
        .map(str::to_owned)
        .collect();
    // Both directions. The guard above catches a name that crosses with no
    // entry; this catches the entry with no name behind it, which is the shape
    // «DIRETO» would take — a fifth key nothing ever sends, filed beside four
    // that mean something, and read by a screen that had to decide what to do
    // when it knows nothing.
    let inventados: Vec<&String> = escritos.difference(&atravessam).collect();
    assert!(
        inventados.is_empty(),
        "`CAMINHOS` writes names the core never sends, and there is exactly one \
         reason to add one — to have something to say when nothing is known, \
         which is the confident lie ADR 0022 exists not to produce: {inventados:?}"
    );

    // And no single name may be the bare word either. `IPv6 DIRETO` is fine —
    // it is qualified, and it is one of the four the core distinguishes; a name
    // that is only «DIRETO» is the flattening itself, wearing one of the four
    // keys.
    for (variant, sentence) in sentences_of(&file, "CAMINHOS") {
        let baixa = sentence.trim().to_lowercase();
        assert!(
            baixa != "direto" && baixa != "direct",
            "`CAMINHOS.{variant}` is written as the bare word «direct», which \
             erases the distinction the four names exist to keep: in a NAT punch \
             the conversation **is** direct, and somebody knew it exists"
        );
    }

    let corpo = js_function(&read("ui/frases.js"), "function fraseDeCaminho(");
    assert!(
        corpo.contains("?? null"),
        "`fraseDeCaminho` no longer answers with nothing for a name it does not \
         know, so the footer prints whatever the core sends:\n{corpo}"
    );
    assert!(
        !corpo.contains("desconhecida("),
        "`fraseDeCaminho` falls through to the unknown-failure sentence, which \
         puts «FALHA QUE ESTA TELA NÃO SABE NOMEAR» beside the round trip:\n{corpo}"
    );
}

#[test]
fn the_path_is_written_beside_the_numbers_and_then_left_alone() {
    // Where it goes, and it is a product decision rather than a layout one: the
    // footer is numbers, and the rule that a sentence only exists if it changes
    // what somebody does is a rule about sentences. The path is a channel next to
    // them, written once when the session comes up and quiet afterwards.
    let page = read("ui/index.html");
    let Some(rodape) = page.split("<footer class=\"telemetria\">").nth(1) else {
        panic!("the telemetry footer is gone, and the path has nowhere to be");
    };
    let Some(rodape) = rodape.split("</footer>").next() else {
        panic!("the telemetry footer never closes");
    };
    assert!(
        rodape.contains("id=\"tel-caminho\""),
        "the path is not in the footer with the other measurements:\n{rodape}"
    );

    let corpo = js_function(&read("ui/tela-sessao.js"), "function desenharTelemetria(");
    assert!(
        corpo.contains("fraseDeCaminho(snapshot.caminho)"),
        "nothing draws the path, so `Snapshot.caminho` crosses the bridge twice \
         a second and is thrown away:\n{corpo}"
    );
    // Written once. Without the guard the footer rewrites the same two words on
    // every frame — twice a second — which `specs/07-estetica.md` calls a
    // design failure by name: movement that diagnoses nothing.
    assert!(
        corpo.contains("caminho !== null") && corpo.contains("!== caminho"),
        "the path is redrawn whether or not it changed, and it never changes:\n{corpo}"
    );
}

#[test]
fn the_arrival_stage_reaches_the_screen_while_the_arrival_is_happening() {
    // `fraseDeEtapa` was written in task 8 and had no caller at all: the FFI
    // published a stage per instant of the crossing and nothing in production
    // read one, because `Connection::connect` blocks and a listener subscribed after
    // it returns has the whole crossing already behind it.
    //
    // Three things have to channel up, and the middle one is the quiet one: the
    // command has to hand the FFI the listener before blocking, the event has
    // to cross under the name serde writes, and the script has to branch on
    // that name.
    let corpo = body_of(&read("src/main.rs"), "async fn connect(");
    assert!(
        corpo.contains("Connection::connect_watching"),
        "the app still enters by the door that blocks with nobody listening, so \
         the stages happen inside a channel that answers only at the end:\n{corpo}"
    );

    let sent = serde_json::to_string(&seele_ffi::Event::ConnectStageChanged {
        stage: seele_ffi::ConnectStage::Dentro,
    })
    .expect("the event must serialise to cross the bridge at all");
    assert!(
        sent.starts_with("{\"ConnectStageChanged\":"),
        "the bridge no longer sends the shape the script branches on: {sent}"
    );

    let script = without_comments(&scripts());
    assert!(
        script.contains("payload.ConnectStageChanged"),
        "nothing in the shell listens for the arrival's stages"
    );
    assert!(
        script.contains("fraseDeEtapa(payload.ConnectStageChanged.stage)"),
        "the stage arrives and is not turned into the sentence written for it, \
         which is `fraseDeEtapa` going back to being dead code"
    );
}

#[test]
fn the_failed_connection_hands_the_shell_the_trail_it_kept() {
    // Task 8 built the trail and `Connection::connect_with_trail` to carry it, and the
    // app went on entering by `Connection::connect`, which throws it away. «Tentei
    // quatro candidatos, o primeiro deu prazo esgotado em 4 s, o quarto
    // recusou» is the data that was missing when the two-house field test
    // failed, and a door nobody opens carries nothing.
    let source = read("src/main.rs");
    assert!(
        source.contains("Result<Entrada, ConnectFailure>"),
        "`connect` answers with the bare error again, and the trail dies at the \
         bridge"
    );

    // And the shell has to know the shape changed, or every failure sentence
    // reads a `ConnectFailure` as if it were a `ConnectionError` and falls through to
    // «FALHA QUE ESTA TELA NÃO SABE NOMEAR».
    let corpo = js_function(&read("ui/tela-boot.js"), "async function conectar(");
    assert!(
        corpo.contains("falha?.error ?? falha"),
        "the entry screen writes its sentence from the whole failure instead of \
         the error inside it, so every connection error reads as unknown:\n{corpo}"
    );
}

#[test]
fn the_stage_that_gives_up_names_no_cause_it_does_not_know() {
    // ADR 0003's alarm, on the screen. `Etapa::Desistiu` carries the whole
    // `ConnectError`, and `chegada.rs` says why in as many words: flattening
    // `PinChanged` and `InviteMismatch` would erase the alarm, because those
    // two are the errors that are *not* network errors.
    //
    // The stage sentence read "NENHUM ENDEREÇO DO CONVITE ATENDEU", which
    // asserts a network cause over both of them — sitting right next to a
    // `fraseDeErro` that builds the alarm out of the two fingerprints. Nobody
    // calls `fraseDeEtapa` yet, so this was a defect scheduled to become
    // visible in a later task rather than one anybody would have seen.
    let file = without_comments(&read("ui/frases.js"));
    let Some((_, frase)) = sentences_of(&file, "ETAPAS")
        .into_iter()
        .find(|(variant, _)| variant == "Desistiu")
    else {
        panic!(
            "`ETAPAS.Desistiu` is gone, and with it the sentence for the end of a failed arrival"
        );
    };

    let baixa = frase.to_lowercase();
    for causa in ["endere", "atende", "rede", "recus", "respond", "ningu"] {
        assert!(
            !baixa.contains(causa),
            "the sentence for `Desistiu` says `{causa}`, which claims a cause \
             the stage does not have: the same stage carries `PinChanged` and \
             `InviteMismatch`, and this channel would print over the ADR 0003 \
             alarm:\n{frase}"
        );
    }

    // And the cause stays with the half that has it. Without this the "fix"
    // for the channel above is to say nothing anywhere.
    assert!(
        file.contains("PinChanged") && file.contains("InviteMismatch"),
        "the two errors that are not network errors lost their sentence, so \
         nothing on the screen composes the ADR 0003 alarm any more"
    );
}

#[test]
fn the_frontend_never_names_a_protocol_concept() {
    // `specs/06-clientes-gui.md`, in one sentence: "Se o frontend precisa saber
    // o que é um `ssrc`, algo está errado." This is that sentence as a test.
    let script = without_comments(&scripts());
    let page = without_comments(&read("ui/index.html"));

    // **A transcrição de boot fica de fora, e é a única coisa que fica.**
    //
    // A regra da `06-clientes-gui.md` é sobre o que se lê **para agir**: «se
    // alguém precisa decifrar um rótulo para resolver um problema, a interface
    // falhou». `#boot-leitura` não é rótulo de nada — é um terminal escrito, e a
    // comp da 0.9.0 o desenha dizendo a porta, o transporte e a chave. Ninguém
    // precisa entender `quic/tls1.3` para apertar CONECTAR; quem entende ganha a
    // confirmação de que a caixa não é cenário.
    //
    // O recorte é por elemento e não por palavra, de propósito: `quic` continua
    // proibido em todo botão, rótulo, aviso e frase de erro desta janela.
    let pagina_sem_leitura = fora_da_leitura_de_boot(&page);

    for forbidden in ["ssrc", "opus_frame", "datagram", "quic", "postcard"] {
        for (name, text) in [
            ("the scripts", &script),
            (
                "index.html outside the boot transcript",
                &pagina_sem_leitura,
            ),
        ] {
            assert!(
                !text.to_lowercase().contains(forbidden),
                "{name} names `{forbidden}`, which is protocol knowledge in a shell"
            );
        }
    }
}

/// A página sem o bloco `#boot-leitura` — ver o guarda acima.
fn fora_da_leitura_de_boot(pagina: &str) -> String {
    let Some(de) = pagina.find("class=\"boot-leitura\"") else {
        panic!("`#boot-leitura` sumiu da entrada; o recorte deste guarda ficou sem assunto");
    };
    let Some(fim) = pagina[de..].find("</div>") else {
        panic!("o bloco de `boot-leitura` nunca fecha");
    };
    let mut sem = String::with_capacity(pagina.len());
    sem.push_str(&pagina[..de]);
    sem.push_str(&pagina[de + fim..]);
    sem
}

/// Every screen carries an id, and the app opens on exactly one of them.
///
/// Screens are swapped by assigning `.hidden` on the section directly, so the
/// *initial* state is in the markup and nowhere else — no script sets it on
/// load. A screen added without `hidden` therefore does not fail, does not
/// throw, and does not warn: it renders stacked below whichever screen is
/// supposed to be showing, and the window looks like boot with a second screen
/// nailed underneath. That is the failure this catches, and it is the one a
/// fifth screen is most likely to arrive with.
#[test]
fn the_app_opens_on_exactly_one_screen() {
    let page = read("ui/index.html");

    let mut screens = Vec::new();
    for rest in page.split("<section ").skip(1) {
        let Some(end) = rest.find('>') else { continue };
        let tag = &rest[..end];

        let classes = attribute(tag, "class").unwrap_or_default();
        if !classes.split_whitespace().any(|class| class == "tela") {
            continue;
        }

        let Some(id) = attribute(tag, "id") else {
            panic!("a `<section class=\"tela\">` has no id, so no script can reach it: <{tag}>");
        };
        screens.push((id, tag.contains("hidden")));
    }

    assert!(
        screens.len() >= 4,
        "found {} screens; boot, sessao, auth and fim are all supposed to be in the page",
        screens.len()
    );

    let open: Vec<&str> = screens
        .iter()
        .filter(|(_, hidden)| !hidden)
        .map(|(id, _)| id.as_str())
        .collect();

    assert_eq!(
        open,
        ["tela-boot"],
        "the app has to open on boot and on nothing else, but these screens start visible: {open:?}"
    );
}

/// The value of `name="…"` in an already-isolated opening tag.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let (_, after) = tag.split_once(&format!("{name}=\""))?;
    let (value, _) = after.split_once('"')?;
    Some(value.to_owned())
}

/// The opening tag of the element carrying `id`.
///
/// Comments are stripped first for the reason the rest of this file strips
/// them: several of these ids are named in the prose above the element they
/// belong to, and a check satisfied by an explanation is a check that cannot
/// fail.
fn tag_with_id(page: &str, id: &str) -> String {
    let page = without_comments(page);
    let Some(after) = page.split(&format!("id=\"{id}\"")).nth(1) else {
        panic!("index.html has no element with id `{id}`");
    };
    let Some(rest) = after.split('>').next() else {
        panic!("unterminated tag for id `{id}`");
    };
    rest.to_owned()
}

#[test]
fn the_alert_and_the_battery_are_layers_over_the_session_and_not_screens() {
    // The comp's inventory settles this on channel 281: `alerta` and `bateria` are
    // layers over `principal`, and `ehPrincipal` is true for all three. They are
    // not screens.
    //
    // Promoting either to a `<section class="tela">` is the tempting mistake —
    // both are full-window overlays now, and both would *look* right as
    // screens. What breaks is invisible in a screenshot: `specs/07` forbids this
    // client from replacing the conversation when the link drops ("a interface
    // esmaece … e o histórico continua ali para leitura"), and a screen replaces
    // it by definition. So the guard is structural: they have to live inside
    // `#tela-sessao`.
    //
    // The call screen pins the opposite decision in the same test, because the
    // two are decided together and drift apart otherwise: it *does* replace the
    // conversation, so it must not be nested in the session.
    let page = read("ui/index.html");
    let Some(after) = page.split("id=\"tela-sessao\"").nth(1) else {
        panic!("index.html no longer has the session screen");
    };
    let Some(session) = after.split("<section ").next() else {
        panic!("the session screen is never closed by another section");
    };

    for id in ["banner", "bateria"] {
        assert!(
            session.contains(&format!("id=\"{id}\"")),
            "`{id}` is drawn outside `#tela-sessao`, so it is a screen and not a \
             layer — and a screen replaces the history that specs/07 says has to \
             stay readable while the link is down"
        );
    }

    assert!(
        !session.contains("id=\"tela-chamada\""),
        "the call screen is nested inside the session, so it is a layer — but it \
         replaces the Channel's history instead of sitting over it, which is the one \
         thing a layer must not do"
    );
}

#[test]
fn the_severity_of_a_notice_reads_without_colour() {
    // `specs/06-clientes-gui.md` forbids information carried by colour alone, and
    // the severity of a `Notice` is information: the core decided between three
    // values and the shell has to show which one. The alert box is one orange
    // box for all three — the comp's own legend says orange for mention and
    // identity, red only for a dropped link — so nothing about the box's paint
    // separates them. The word has to.
    //
    // Built from the real enum, so renaming a variant fails here instead of
    // leaving a `data-para` that matches nothing and a chip that never shows.
    let page = without_comments(&read("ui/index.html"));
    let css = styles();

    let mut words = BTreeSet::new();
    for severity in [
        seele_ffi::Severity::Info,
        seele_ffi::Severity::Warning,
        seele_ffi::Severity::Critical,
    ] {
        let Ok(json) = serde_json::to_string(&severity) else {
            panic!("Severity does not serialise, so no shell can read it at all");
        };
        let name = json.trim_matches('"').to_owned();

        let marker = format!("data-para=\"{name}\">");
        let Some(word) = page
            .split(&marker)
            .nth(1)
            .and_then(|rest| rest.split('<').next())
        else {
            panic!(
                "the page has no chip for severity `{name}`, so that severity \
                 arrives with nothing but the same orange box as the other two"
            );
        };
        assert!(
            !word.trim().is_empty(),
            "the chip for severity `{name}` is empty"
        );
        assert!(
            css.contains(&format!("data-severidade=\"{name}\"")),
            "nothing in the stylesheet reveals the chip for severity `{name}`, so \
             the word is in the markup and never on the screen"
        );
        words.insert(word.trim().to_owned());
    }

    assert_eq!(
        words.len(),
        3,
        "two severities are written with the same word, so the box cannot tell \
         them apart at all: {words:?}"
    );
}

#[test]
fn the_button_with_no_command_behind_it_cannot_be_pressed_and_the_one_that_grew_one_asks_first() {
    // This guard used to cover two buttons and now covers one and a half, and
    // the rewrite *is* the record of what changed.
    //
    // `FORÇAR REINSERÇÃO DE PLUG` has not moved: there is still no "try now" in
    // this product. The core is already retrying — that is where the attempt
    // count above the button comes from — so a pressable button there would
    // promise to speed up something that is already running. The comp wires it
    // to a handler that closes the box, which is the worst of the available
    // readings: a button that looks like it acts, and does nothing.
    //
    // The other one moved. `EJETAR PLUG DO OPERADOR` was disabled because
    // `EndReason::Kicked` existed only for the person receiving one and nothing
    // could emit it; `expulsar_pessoa`, `banir_pessoa`, `remover_mensagem` and
    // `mover_pessoa` are what changed that. So the question about it is no
    // longer "is it disabled" — it is "does it stay honest now that it does
    // something", and that has three halves:
    //
    // - it still *starts* disabled in the markup. Before the first snapshot this
    //   window does not know which permissions it has, and a button born
    //   pressable promises what it may not be able to carry out;
    // - something turns it on from the snapshot, and from the moderation
    //   booleans rather than from `may_manage_voice_rooms` or from nothing at all;
    // - and it opens the choosing, rather than acting. A `Notice` carries a
    //   severity and a reason and never *whose* it is — the same gap that leaves
    //   the three cells of that box empty — so a button that ejected from there
    //   would have to guess who, and guessing who leaves a session is the last
    //   thing a product should do.
    let page = read("ui/index.html");
    let script = without_comments(&scripts());

    let forcar = tag_with_id(&page, "bateria-forcar");
    assert!(
        forcar.contains("disabled"),
        "`bateria-forcar` is pressable, and there is no command behind it — so it \
         is a button that looks like it acts and does nothing: <{forcar}>"
    );
    assert!(
        forcar.contains("title=\""),
        "`bateria-forcar` is disabled and says nothing about why, which reads as a \
         bug rather than as a gap: <{forcar}>"
    );
    assert!(
        !script.contains("$(\"bateria-forcar\")"),
        "a script reaches for `bateria-forcar`, so the disabled button grew a \
         listener — which is the comp's mistake, one layer down"
    );

    let ejetar = tag_with_id(&page, "alerta-ejetar");
    assert!(
        ejetar.contains("disabled"),
        "`alerta-ejetar` is born pressable. It is now a real control, but which \
         permissions this session has is not known until the first snapshot — so \
         between the window opening and that frame it would be promising \
         moderation nobody has: <{ejetar}>"
    );
    assert!(
        ejetar.contains("title=\""),
        "`alerta-ejetar` can still end up disabled — a Server that gave this person \
         no moderation at all — and a disabled control that says nothing about \
         why reads as a bug: <{ejetar}>"
    );

    // Scoped to the one function that owns the button's state, because the whole
    // script says all of these words either way — the paragraph explaining why
    // the alert box has no subject would satisfy an unscoped search for it.
    let porta = body_of(&scripts(), "function atualizarPortaDoAlerta");
    assert!(
        porta.contains("$(\"alerta-ejetar\")"),
        "nothing reaches for `alerta-ejetar`, so the button that finally has a \
         command behind it is never turned on:\n{porta}"
    );
    assert!(
        porta.contains("podeModerarPersonos") || porta.contains("may_kick"),
        "`alerta-ejetar` is enabled without asking whether this session may \
         moderate anybody, so it offers what the Server will refuse:\n{porta}"
    );
    assert!(
        porta.contains("title"),
        "`alerta-ejetar` can be left disabled without the reason being rewritten, \
         so the `title` in the markup outlives the state it explains:\n{porta}"
    );

    // And what the press does: open the choosing, never act.
    let Some(aperto) = script
        .split("$(\"alerta-ejetar\").addEventListener")
        .nth(1)
        .and_then(|rest| rest.split("\n});").next())
    else {
        panic!("nothing is listening on `alerta-ejetar` at all");
    };
    assert!(
        aperto.contains("abrirModeracao("),
        "`alerta-ejetar` does something other than open the moderation:{aperto}"
    );
    for verbo in [
        "invoke(\"expulsar_pessoa\"",
        "invoke(\"banir_pessoa\"",
        "invoke(\"mover_pessoa\"",
    ] {
        assert!(
            !aperto.contains(verbo),
            "`alerta-ejetar` calls `{verbo}…` straight from the alert box, which \
             has no subject — so it is acting on a person it guessed:{aperto}"
        );
    }
}

#[test]
fn the_alert_box_does_not_spend_the_red_reserved_for_a_dropped_link() {
    // `tokens.css:19` marks the red "EXCLUSIVO alerta e queda", and the comp's
    // own banner legend narrows it further: "laranja para menção e identidade;
    // vermelho apenas quando a conexão cai". The battery box is the red one.
    //
    // The tempting change is to escalate `Severity::Critical` to red, and it is
    // exactly what teaches people to read the battery's red as "some notice",
    // on the day it means the session is being held in memory.
    //
    // Comments are stripped from both sides, and that is load-bearing rather
    // than tidy: the alert sheet has to be able to write down *why* it is not
    // red, and the paragraph saying so names the token. A guard a comment can
    // trip is as broken as a guard a comment can satisfy.
    let sheet = without_comments(&read("ui/camada-alerta.css"));
    assert!(
        !sheet.contains("vermelho"),
        "the alert layer paints with the token reserved for alarm and collapse, \
         which is the battery's colour and nothing else's"
    );
    assert!(
        without_comments(&read("ui/camada-bateria.css")).contains("vermelho"),
        "the battery layer no longer uses the red that is the whole point of it"
    );
}

#[test]
fn a_person_card_passes_the_band_through_and_never_measures_anything_itself() {
    // Two failures in one function, both silent.
    //
    // The first is the one `crates/seele-ffi/src/types.rs:58-79` argues against
    // by name: the comp calls `corSync(media)` in the shell, and a shell that
    // knows "85 is nominal" is a shell that will disagree with the terminal the
    // day one of the two is updated. The band arrives decided; this card may
    // only pass it on.
    //
    // The second is drawing what nobody measured. `Telemetry.input_level` is a
    // scalar and it is *ours* — amplitude per person does not cross — and
    // `set_volume` writes with nothing reading back. Twenty-six bars driven by
    // our own microphone would animate convincingly under somebody else's name.
    //
    // Scoped to the two functions that make one card — the skeleton and the
    // values written over it — because the file as a whole says all of these
    // words either way: the paragraph explaining why the waveform is gone would
    // satisfy an unscoped search for it.
    let script = scripts();
    let body = format!(
        "{}\n{}",
        body_of(&script, "function cartaoDoPersono"),
        body_of(&script, "function pintarCartao")
    );

    assert!(
        body.contains("pessoa.sync_band"),
        "the card no longer reads the band the core decided"
    );
    for threshold in ["Nominal", "Degraded", "Critical"] {
        assert!(
            !body.contains(threshold),
            "the card names the band `{threshold}` itself, which is the shell \
             deciding a threshold the core already decided"
        );
    }

    // The v3 answer to the second failure is stronger than the v2 one, and this
    // is where the two differ. v2 drew the waveform and the per-person delay as
    // empty frames with a dash and a `title` saying what was missing; v3 drops
    // them, because on a screen whose whole point is being easy to read an
    // explained dash is noise — somebody entering a voice room wants to know who is
    // talking, not which fields this protocol does not carry yet. The record of
    // the gap lives in the inventory (§1.3, §7), which is where anybody looks
    // before trying to draw them again.
    //
    // So: neither the value nor the frame. Drawing them from what *does* cross
    // would be worse than either — `input_level` is ours and scalar, `rtt_ms` is
    // ours and one number, and both would animate convincingly under somebody
    // else's name.
    for ours in ["input_level", "rtt_ms"] {
        assert!(
            !body.contains(ours),
            "the card draws a per-person value out of `{ours}`, which is this \
             machine's own measurement and one number"
        );
    }
    for empty in ["naoMedido", "SEM_DADO", "SEM_MEDIDA"] {
        assert!(
            !body.contains(empty),
            "the card draws an empty field marked `{empty}`. On this screen the \
             decision is to omit what has no data rather than frame it: a dash \
             with an explanation is one more thing to read on the screen that \
             exists for not having to read much"
        );
    }
}

#[test]
fn the_state_of_a_person_is_a_word_and_never_only_a_colour() {
    // `specs/06-clientes-gui.md` forbids information carried by colour alone, and
    // who is transmitting is information: the comp marks it with an orange halo
    // around the card and nothing else. A halo is invisible to anybody who does
    // not see the hue, and this card carries three facts that way — the
    // microphone, the voice, and the ears.
    //
    // The v3 answer is two registers of the same fact, and this guard is that
    // both exist: the chip, in one word, and the plain sentence beside the name,
    // which is what `LEGENDAS SIMPLES` is for.
    let script = scripts();
    let paint = body_of(&script, "function pintarCartao");
    let sentence = body_of(&script, "function fraseDoEstado");

    // The chip, and what it says. Reaching for the element is not enough — an
    // empty chip is a card marked by paint alone, and it looks fine in a
    // screenshot taken by somebody who sees the orange. So the statement that
    // fills it has to branch on the microphone and on the voice, and every
    // branch has to be a word.
    let chips: Vec<&str> = paint
        .split(';')
        .filter(|statement| statement.contains("pastilha") && statement.contains("textContent"))
        .collect();
    assert!(
        !chips.is_empty(),
        "the card writes no state chip, so the halo is the only thing marking who \
         is speaking"
    );
    // A statement that tells the three states apart in words: it has to look at
    // the microphone and at the voice, and have a word for each way out.
    let tells_them_apart = |statement: &str| {
        statement.contains("muted")
            && statement.contains("speaking")
            && statement
                .split('"')
                .skip(1)
                .step_by(2)
                .filter(|word| !word.trim().is_empty())
                .count()
                >= 3
    };

    // Either the chip is written with the words in place, or it is written from
    // a local that carries them. Following the local matters: assigning `""` to
    // the chip while a perfectly good ternary sits unused above it is the exact
    // shape of "marked by paint alone", and it would read as fine in a diff.
    let straight = chips.iter().copied().any(tells_them_apart);
    let through_a_local = paint.split(';').any(|declaration| {
        tells_them_apart(declaration)
            && declaration
                .split('=')
                .next()
                .and_then(|left| left.split_whitespace().last())
                .is_some_and(|name| {
                    chips.iter().any(|chip| {
                        chip.split('=')
                            .nth(1)
                            .is_some_and(|value| value.contains(name))
                    })
                })
    });
    assert!(
        straight || through_a_local,
        "no state chip on this card is written from a value that branches on both \
         the microphone and the voice with a word for each. Either a state is \
         being told apart by paint alone, or two of them come out reading the \
         same:\n{}",
        chips.join("\n")
    );

    assert!(
        paint.contains("fraseDoEstado"),
        "the card no longer writes the state as a sentence beside the name, which \
         is the half a newcomer reads"
    );

    // All three facts, in the sentence. The colour version of any one of them
    // would be a fact only some readers get.
    for fact in ["muted", "speaking", "total_isolation"] {
        assert!(
            sentence.contains(fact),
            "the sentence beside the name never mentions `{fact}`, so that state \
             reaches the screen as paint and nothing else"
        );
    }
}

#[test]
fn the_volume_control_does_not_hide_behind_the_pointer() {
    // The defect this screen exists to fix, in one rule. `tela-sessao.css` gives
    // the per-person slider `opacity: 0` and reveals it on `:hover`, which is the
    // definition of a hidden control: it is not on the path of anybody using a
    // keyboard, it never appears under touch, and whoever did not sweep the
    // pointer across that row by accident never learned it was there.
    //
    // Two halves, because either alone passes with the defect present. The
    // control has to *exist* — minus, plus, and cells that are real buttons —
    // and no rule of this sheet may use the pointer to bring anything into
    // being. Highlighting on hover stays legal; that is what `background` and
    // `border-color` in those rules are.
    let script = scripts();
    // Comments stripped, and that is load-bearing rather than tidy: the sheet
    // has to be able to write down *why* nothing here hangs off `:hover`, and
    // the paragraph saying so names the word. A guard a comment can trip is as
    // broken as a guard a comment can satisfy.
    let sheet = without_comments(&read("ui/tela-chamada.css"));

    let build = body_of(&script, "function controleDeVolume");
    // U+2212, which the embedded face does have — the inventory measured it
    // (§5). The ASCII hyphen is a different character and a different width in a
    // monospaced face, beside a `+` that is ASCII.
    assert!(
        build.contains('\u{2212}') && build.contains('+'),
        "the volume control no longer offers a minus and a plus"
    );
    assert!(
        build.contains("CELULAS_DE_VOLUME"),
        "the volume control no longer draws a row of cells to click"
    );
    assert!(
        build
            .split(';')
            .any(|statement| statement.contains("elemento(\"button\"")
                && statement.contains("vol-cela")),
        "the volume cells are not buttons, so they are neither clickable nor \
         reachable by keyboard — which is the hidden control again, wearing the \
         shape of the fix"
    );
    assert!(
        scripts().contains("set_volume"),
        "nothing sends the chosen volume anywhere"
    );

    for rule in sheet.split('}') {
        let Some((selector, declarations)) = rule.split_once('{') else {
            continue;
        };
        if !selector.contains(":hover") {
            continue;
        }
        for reveal in ["opacity", "visibility", "display"] {
            assert!(
                !declarations.contains(reveal),
                "`{}` uses the pointer to decide whether something exists \
                 (`{reveal}`), which is the hidden control this screen was \
                 redrawn to fix — hover may highlight, never reveal",
                selector.trim()
            );
        }
    }
}

#[test]
fn as_duas_saidas_continuam_dizendo_qual_delas_larga_a_sala() {
    // A distinção que o v3 trouxe e que a comp da 0.9.0 mantém: **trocar de
    // vista não é sair da sala.** No protótipo os dois botões chamavam a mesma
    // coisa e só as palavras os separavam.
    //
    // O que mudou na 0.9.0 é onde eles moram. A tela de chamada deixou de
    // existir, então não há mais `VER CANAIS` nem `chamada-ejetar`: o par vive
    // na fileira do operador — `operador-vista`, que alterna o que a coluna do
    // meio mostra, e `operador-sair`, que larga a sala.
    //
    // O erro que este guarda impede é o mesmo de sempre: um dos dois passar a
    // fazer o que o outro faz, e a tela continuar prometendo dois caminhos.
    let script = without_comments(&scripts());
    let page = without_comments(&read("ui/index.html"));

    for id in ["operador-vista", "operador-sair"] {
        assert!(
            page.contains(id),
            "sumiu `{id}`, e com ele metade do par que separa trocar de vista de \
             sair da sala"
        );
    }

    // Alternar a vista não pode puxar a conexão nem largar a sala.
    let trocar = body_of(&script, "function fecharChamada");
    assert!(
        !trocar.contains("leave_voice_room") && !trocar.contains("leave_voice_room"),
        "voltar para a conversa está largando a sala, então os dois botões fazem \
         a mesma coisa e as palavras da tela ficaram erradas:\n{trocar}"
    );
}

#[test]
fn nada_finge_ter_um_passado_que_esta_janela_nao_viu() {
    // **A lista `EVENTOS` saiu com a tela de chamada na 0.9.0**, e este guarda
    // mudou de alvo em vez de sumir com ela.
    //
    // O que ele protegia não era a lista: era a regra de que **nada nesta
    // janela finge lembrar do que aconteceu antes de ela abrir**. O `EVENTOS`
    // nascia vazio toda vez e dizia isso por escrito; a tentação, sempre, é
    // preenchê-lo com algo plausível — «sessão iniciada», «entrou na sala» —
    // e inventar um passado que ninguém mediu.
    //
    // A comp não traz a lista. A regra continua, e o que se cobra agora é que
    // ninguém tenha reinventado um registro semeado: uma lista de eventos que
    // nasça com linhas dentro é a mesma mentira noutra marcação.
    let page = without_comments(&read("ui/index.html"));

    assert!(
        !page.contains("id=\"chamada-eventos\""),
        "a lista de eventos da chamada voltou; se ela precisa voltar, este guarda \
         precisa voltar a cobrar que ela nasça vazia"
    );

    // E a frase que a acompanhava não pode ter sobrevivido sozinha: ela promete
    // um registro, e prometer sem entregar é pior que não ter.
    assert!(
        !page.contains("Não há registro anterior"),
        "ficou a frase que explicava uma lista que não existe mais"
    );
}

#[test]
fn nothing_draws_a_battery_bar_out_of_a_denominator_the_wire_never_sent() {
    // **Este guarda substituiu um que cobrava o contrário**, e a troca é a
    // decisão e não um relaxamento.
    //
    // O anterior — `the_battery_bar_stays_empty_because_nothing_carries_its_total`
    // — exigia que a barra **existisse**, marcada como ausente, com o argumento
    // de que «a moldura é o que torna a lacuna visível». A moldura foi removida:
    // o `title` dela mesmo dizia que «a contagem ao lado já é a mesma
    // informação», e uma linha que só existe para dizer que não tem nada a dizer
    // é ruído com cara de dado.
    //
    // O defeito que aquele guarda protegia continua sendo real, e é este: o comp
    // divide por um literal `299`, que é a casca chutando a spec e ficando errada
    // no dia em que a spec mudar. `remaining_seconds` atravessa o fio; o
    // **total** não. Então o que se cobra aqui é a ausência do denominador
    // inventado, e não a presença de uma moldura vazia.
    let page = without_comments(&read("ui/index.html"));
    let script = without_comments(&scripts());

    assert!(
        !page.contains("bateria-barra") && !script.contains("bateria-barra"),
        "a barra da queda voltou; se ela voltou com denominador, ele foi inventado"
    );

    // O literal do comp, procurado onde ele faria estrago: junto do que desenha
    // a queda. Um `299` noutro lugar do script é outro número.
    for trecho in script.split("bateria").skip(1) {
        let vizinhanca: String = trecho.chars().take(400).collect();
        assert!(
            !vizinhanca.contains("299"),
            "algo perto da faixa da queda usa o literal 299 como total: \
             o protocolo nunca mandou de quanto a contagem partiu"
        );
    }
}

#[test]
fn ending_the_session_takes_the_call_screen_down_with_it() {
    // Every `.tela` is `height: 100vh`, so two visible ones do not overlap: they
    // stack, and the second sits below the fold where nobody finds it. A session
    // can end with the call screen open — that is precisely who gets kicked, the
    // person sitting in a voice room — and `mostrarFim` picks the next screen on its
    // own.
    //
    // Scoped to `mostrarFim`, because `tela-chamada.js` names the function in
    // its own declaration and in the paragraph above it either way.
    let body = body_of(&scripts(), "function mostrarFim");
    assert!(
        body.contains("abandonarChamada"),
        "the end-of-session screen does not take the call screen down, so the two \
         stack and the reason the session ended lands below the fold"
    );
}

/// The class names a stylesheet *defines* — the first class of every selector
/// that starts a channel.
///
/// The first class and not all of them, because that first one is the owner:
/// `.busca .botao-fantasma` is `tela-sessao.css` refining a primitive it did not
/// invent, and only `.busca` says whose rule it is.
fn classes_defined_in(css: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for channel in css.lines() {
        let Some(rest) = channel.strip_prefix('.') else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if !name.is_empty() {
            found.insert(name);
        }
    }
    found
}

#[test]
fn the_voice_room_says_who_is_inside_and_says_their_state_in_words() {
    // The v3 comp's biggest single gain, and it costs no protocol: `voice_rooms_of`
    // already fills `VoiceRoom.people` from `room.roster(voice_room.id)` for *every* voice room,
    // not only the occupied one. The app spent that on a block bar — twelve
    // characters standing in for the four names it had in hand.
    //
    // The second half is the part that would rot silently. The comp marks who
    // is talking with a coloured dot and nothing else, and
    // `specs/06-clientes-gui.md` forbids information carried by colour alone —
    // a dot is carried by shape alone too, which is the same failure wearing
    // the other hat. So the states that change something have to be a *word*.
    // Deleting the words leaves a screen that still looks right in a
    // screenshot and says nothing to anybody reading it without colour.
    //
    // Scoped to the two functions that build the rows, because the file as a
    // whole says all of these words either way: the paragraph explaining why
    // the word is there would satisfy an unscoped search for it.
    let lista = body_of(&scripts(), "function desenharCanais");
    let dentro = body_of(&scripts(), "function linhaDeQuemEstaDentro");

    assert!(
        lista.contains("voice_room.people") && lista.contains("linhaDeQuemEstaDentro"),
        "the VoiceRoom list no longer draws who is inside, so the one thing the v3 \
         added to this column is a block bar again"
    );
    assert!(
        dentro.contains("pessoa.speaking") && dentro.contains("pessoa.muted"),
        "the row inside a sala de voz reads neither who is talking nor who is muted, \
         which is everything it was drawn to say"
    );
    for word in ["fala", "mudo"] {
        assert!(
            dentro.contains(&format!("\"{word}\"")),
            "the state `{word}` is drawn without a word beside the glyph, so it \
             reaches a monochrome screen — and a colour-blind reader — as a dot \
             that means nothing:\n{dentro}"
        );
    }
}

#[test]
fn entering_and_leaving_a_voice_room_are_labelled_buttons_and_not_a_click_on_the_row() {
    // What the v2 shipped: a `<li>` with `cursor: pointer` and one listener on
    // the `<ul>`. Nothing about it said it could be pressed, no keyboard could
    // reach it, and no screen reader announced it as anything at all. The LAN
    // test found the same defect one column over, on the `+` that was the only
    // way out of a server — this is that finding applied here.
    //
    // Leaving is asserted beside entering on purpose. The comp writes
    // `VOCÊ ESTÁ AQUI` on the occupied sala de voz and wires it to nothing, and taking
    // that literally would trade a mute button for a dead one: this screen
    // would lose its only way out of a voice room, and gain a button that looks like
    // it acts.
    let handler = body_of(&scripts(), "async function alternarCanal");
    let lista = body_of(&scripts(), "function desenharCanais");

    assert!(
        handler.contains("button[data-voice_room]") && handler.contains("button[data-linha]"),
        "the channel handler is looking for something other than a button, so \
         whatever it finds is not focusable and announces as nothing:\n{handler}"
    );
    assert!(
        !handler.contains("closest(\"li\")"),
        "the channel handler is back to catching the row, which is the shape \
         that has no keyboard and no accessible name"
    );
    // **A saída saiu desta lista, e a exigência mudou de lugar com ela.**
    //
    // A fileira da sala teve um botão de desligar — o círculo com a haste, o
    // símbolo de um aparelho que se apaga — que chamava `leave_voice_room`. Ele saiu a
    // pedido de quem usa. O que sobrou na fileira é navegação, e só: entrar na
    // sala, ou voltar para a grade quando já se está nela.
    //
    // O que este guarda protege continua sendo o mesmo: os dois atos têm de ser
    // controles nomeados, e não um clique numa fileira que teclado nenhum
    // alcança. Mudou onde cada um mora.
    assert!(
        lista.contains("ENTRAR NA SALA") && lista.contains("VER A CHAMADA"),
        "a fileira da sala tem que oferecer os dois sentidos da navegação por \
         extenso: entrar, e voltar para a grade de onde já se está"
    );
    assert!(
        !lista.contains("leave_voice_room"),
        "a lista de salas voltou a largar a sala. Ela é navegação; quem sai é o \
         `SAIR DA SALA` do rodapé, que diz em palavras o que faz"
    );

    // E a saída existe, escrita, no rodapé do operador — que está na tela
    // enquanto se está numa sala, que é a condição em que ela faz sentido.
    let rodape = without_comments(&read("ui/tela-chamada.js"));
    assert!(
        rodape.contains("SAIR DA SALA") && rodape.contains("leave_voice_room"),
        "nada nesta janela larga a sala: o botão do rodapé perdeu o rótulo ou o \
         comando, e a fileira da sala não os tem mais"
    );
}

#[test]
fn the_session_screen_omits_what_nothing_measures_rather_than_a_dash_per_row() {
    // The v2 rule was: draw the frame, leave the value visibly unmeasured, put
    // the reason in a `title`. It is the right rule where the absence answers a
    // question the screen just asked — the average with no connection in, the battery
    // bar, the alert's three cells — and all of those stay.
    //
    // It is the wrong rule *per row*. A dash beside every Channel and two more
    // inside every person card is half a dozen explained em-dashes on a screen
    // whose entire purpose is being simple, each one asking to be read and none
    // of them answering anything. The v3 inverts it here: what has no data
    // leaves the screen.
    //
    // Guarded as "these two builders draw no dash", plus a control that the
    // helper is still in use elsewhere — otherwise deleting `naoMedido`
    // outright would satisfy this and quietly take the honest gaps with it.
    let script = scripts();
    let canais = body_of(&script, "function desenharCanais");
    let pessoa = body_of(&script, "function linhaNativaDoRoster");
    let media = body_of(&script, "function desenharMedia");

    for (name, body) in [("desenharCanais", &canais), ("linhaNativaDoRoster", &pessoa)] {
        assert!(
            !body.contains("naoMedido"),
            "`{name}` draws an unmeasured value once per row, which is the noise \
             the v3 took off this screen:\n{body}"
        );
    }
    assert!(
        media.contains("naoMedido"),
        "the Sync average no longer marks itself unmeasured when there is no \
         connection in — that gap is an answer to a question the panel just asked, and \
         it is the one this rule does not touch"
    );
}

#[test]
fn the_bound_name_is_stated_once_and_never_worn_as_a_badge() {
    // The v3 comp draws a `verif` seal per person and another per message. Both
    // are gone, and the reasoning is in §1.2 of its inventory: the PERSISTENCE binds
    // a nickname to the identity that claimed it first and the PERMISSIONS refuses
    // any other (ADR 0017), so the seal would be true on every channel forever — and
    // a badge everybody wears is a badge nobody learns to read, on the day one
    // of them is missing.
    //
    // What replaced it is one sentence. Two failure modes, and this catches
    // both: the sentence quietly disappearing in a later edit, and the seal
    // creeping back in per message.
    let page = without_comments(&read("ui/index.html"));
    let mensagens = body_of(&scripts(), "function desenharMensagens");

    // The sentence is gone too, and this is the third thing the screen stopped
    // saying about itself. It explained a property the product already
    // guarantees — the PERSISTENCE binds a nickname to the first identity that
    // claims it, and the PERMISSIONS refuses any other (ADR 0017) — to someone who
    // never doubted it. Whoever does doubt it is not reassured by a channel of
    // text next to the name; they are reassured by the pin alarm of ADR 0003,
    // which is loud and blocking and lives somewhere else.
    //
    // What this still guards is the part that was never about wording: the
    // per-channel seal must not creep back in.
    let sentence = "ninguém consegue usar o nome de outra pessoa";
    assert!(
        !page.contains(sentence),
        "the sentence about names being bound to keys is back on the page; it \
         explains a guarantee nobody asked about, next to a name nobody \
         doubted"
    );
    assert!(
        !mensagens.contains("selo"),
        "a per-message seal is back beside the author, and it will read `true` \
         on every message this product can ever draw:\n{mensagens}"
    );
}

#[test]
fn the_search_starts_closed_and_opens_from_something_that_says_buscar() {
    // The search bar used to live open, spending 40px of the Channel column on
    // every session for something done once an hour. The v3 puts a labelled
    // `BUSCAR` in the Channel's header instead.
    //
    // Which creates one way to get this exactly wrong, and it is silent:
    // `focus()` on an input inside a `hidden` element does nothing and reports
    // nothing, so pressing `/` would simply stop working — with the field still
    // in the page, the listener still bound, and no error anywhere.
    let page = read("ui/index.html");
    let script = without_comments(&scripts());

    let tag = tag_with_id(&page, "form-busca");
    assert!(
        tag.contains("hidden"),
        "the search bar starts open, so the header button that opens it is \
         either a no-op or a second way to do what is already done: <{tag}>"
    );
    assert!(
        script.contains("$(\"botao-buscar\")"),
        "nothing opens the search, so the bar the markup hides can never come back"
    );

    // Focusing the field is legal in exactly one place — after the bar has been
    // revealed — so the check is "nowhere else", with that one body cut out.
    let opener = body_of(&scripts(), "async function alternarBusca");
    let Some(revealed) = opener.find("barra.hidden =") else {
        panic!("`alternarBusca` no longer reveals the bar at all:\n{opener}");
    };
    let Some(focused) = opener.find(".focus()") else {
        panic!("`alternarBusca` reveals the bar and never puts the cursor in it");
    };
    assert!(
        revealed < focused,
        "`alternarBusca` focuses the field before revealing the bar, and \
         `focus()` inside a `hidden` element does nothing and reports nothing"
    );

    let elsewhere = script.replace(&opener, "");
    assert!(
        !elsewhere.contains("$(\"campo-busca\").focus()"),
        "something outside `alternarBusca` focuses the search field directly, so \
         it lands on a field inside a `hidden` element — which does nothing and \
         says nothing, and the `/` key just stops working"
    );
}

#[test]
fn the_add_server_button_carries_one_verb_and_not_the_two_the_v2_conflated() {
    // This guard used to demand the opposite, and the rewrite is the record of
    // what changed.
    //
    // It held the `+` *disabled*, on the reading that adding a server means two
    // `Connection` at once and this product has one. The premise did not change —
    // `Session` still holds one and `connect` still answers `AlreadyConnected` —
    // but the conclusion drawn from it did. Holding one connection at a time
    // does not forbid a second server on screen; it decides what pressing one
    // *means*: leave this one, enter that one. So the trilha lists the history,
    // and the `+` finally carries the half it never had.
    //
    // What did **not** change is the finding the v2 paid for on a two-machine
    // test: with "enter another" and "leave this" behind the same glyph, nobody
    // found how to leave. That is what this now holds.
    //
    // - the `+` names one thing, and it is not leaving. Leaving is what it
    //   costs, and the cost is what the confirmation says — **in words, with the
    //   server's name in them**, which is the half the v2 never had. A separate
    //   `DESCONECTAR` stood beside it for one version and the 0.9.0 comp does
    //   not draw it; what replaced it is not silence but the sentence;
    // - it asks before it spends a live session, through the one confirmation
    //   surface of this product rather than a second box of its own;
    // - and it is still a glyph, so it still needs an accessible name.
    let page = read("ui/index.html");
    let script = without_comments(&scripts());
    let sessao = read("ui/tela-sessao.js");

    let tag = tag_with_id(&page, "trilha-adicionar");
    assert!(
        !tag.contains("disabled"),
        "the `+` is drawn dead again. Switching servers is disconnect-and-connect \
         and this product can do it, so a disabled button here is a capability \
         hidden behind a moldura: <{tag}>"
    );
    assert!(
        tag.contains("aria-label=\""),
        "the `+` is a glyph with no accessible name, so it announces as `+`: <{tag}>"
    );
    assert!(
        tag.contains("title=\""),
        "the `+` says nothing about what it costs to the pointer, and what it \
         costs — a live session — is not in the word `+`: <{tag}>"
    );

    // The sentence is the mitigation, so the sentence is what is held: pressing
    // the `+` has to reach a confirmation whose deciding button spells out
    // leaving, naming the server being left. Without this the `+` is once again
    // a mute glyph with two consequences, which is the state the two-machine
    // test found and nobody escaped from.
    let pede = js_function(&sessao, "async function pedirAEntrada(");
    assert!(
        pede.contains("abrirConfirmacao"),
        "the `+` no longer asks before spending a live session: {pede}"
    );
    assert!(
        pede.contains("SAIR DE"),
        "the `+`'s confirmation does not say `SAIR DE` on the button that \
         decides, so leaving is unnamed again and the glyph carries two \
         consequences in silence: {pede}"
    );
    assert!(
        pede.contains("nomeDesteServidor()"),
        "the confirmation does not name the server being left, so it warns \
         about leaving somewhere without saying where: {pede}"
    );

    assert!(
        script.contains("$(\"trilha-adicionar\")"),
        "nothing listens on the `+`, so the button that is no longer disabled \
         does nothing at all — which is worse than the disabled one it replaced"
    );

    let pede = js_function(&sessao, "async function pedirAEntrada(");
    assert!(
        pede.contains("abrirConfirmacao("),
        "the `+` drops a live session without saying so first: {pede}"
    );
    for verbo in ["invoke(\"disconnect\"", "invoke(\"connect\""] {
        assert!(
            !pede.contains(verbo),
            "the `+` calls `{verbo}…` straight from the press, so the box that \
             would have said what it costs is decoration: {pede}"
        );
    }

    // And the act behind it goes to the entrance and stops there. A `connect`
    // in here would be the `+` choosing a server for somebody who pressed it to
    // choose one.
    // Pelo nome que só esta tela usa: `tela-fim.js` tem um `sairParaAEntrada`
    // que carrega depois e vencia este silenciosamente, o que fazia este guarda
    // conferir uma função que não rodava.
    let sai = js_function(&sessao, "async function sairDoServidorParaAEntrada(");
    assert!(
        sai.contains("ejetar("),
        "the `+` reaches the entrance without ending the session, and this \
         product cannot hold two: {sai}"
    );
    assert!(
        !sai.contains("conectar("),
        "the `+` connects somewhere on its own, so pressing «connect to another \
         server» picks the server for you: {sai}"
    );
}

#[test]
fn entering_another_server_leaves_this_one_and_says_so_with_both_names() {
    // The whole of "entrar num outro servidor desconecta você do anterior",
    // held where it can break: the order of the two halves, and the sentence
    // that has to run before either.
    //
    // Disconnect first is not style. `connect` answers `AlreadyConnected` while
    // a `Connection` is held, so a switch that connects first does not half-work — it
    // does nothing, and it does nothing while the person is looking at a screen
    // that says they are somewhere else.
    let sessao = read("ui/tela-sessao.js");
    let troca = js_function(&sessao, "async function trocarDeServidor(");

    let Some(saida) = troca.find("ejetar(") else {
        panic!("switching servers never ends the session it is switching away from: {troca}");
    };
    let Some(entrada) = troca.find("conectar(") else {
        panic!("switching servers never connects to the one it was asked for: {troca}");
    };
    assert!(
        saida < entrada,
        "the switch connects before it disconnects, and `connect` refuses with \
         `AlreadyConnected` while a Connection is held — so pressing a server in the \
         trilha does nothing at all: {troca}"
    );

    // The press asks first, and the question carries both names: where you are
    // and where you are going. A box that says «tem certeza?» is the box that
    // teaches people to press twice.
    let pede = js_function(&sessao, "async function pedirTrocaDeServidor(");
    assert!(
        pede.contains("abrirConfirmacao("),
        "pressing a server in the trilha drops the current session with no \
         question at all: {pede}"
    );
    let frase = js_function(&sessao, "function consequenciaDeTrocar(");
    assert!(
        frase.contains("${daqui}") && frase.contains("${ate}"),
        "the sentence does not name both servers, so it says something is about \
         to happen without saying to what: {frase}"
    );

    // The channel that only a host sees, and the only one that changes the answer:
    // for them, switching is not leaving a conversation, it is closing
    // everybody's.
    assert!(
        frase.contains("hospedando"),
        "the consequence is the same whether or not this computer is the one \
         hosting the server being left — and for the host it is not: {frase}"
    );
    // The call itself, and not the name: the `console.warn` in the same body
    // says `estado_da_porta` too, so a check on the word alone stays green with
    // the question deleted and only the failure branch left behind.
    let hospeda = js_function(&sessao, "async function hospedandoAqui(");
    assert!(
        hospeda.contains("invoke(\"estado_da_porta\")"),
        "the screen decides on its own whether this window is hosting, instead \
         of asking the side that knows: {hospeda}"
    );
}

#[test]
fn the_trilha_lists_the_history_and_never_offers_the_server_it_is_already_on() {
    // The column is the shortcut list of the entrance, seen from inside a
    // session — same command, same order. What it must not do is repeat the
    // server it is already on: pressing that entry would tear a session down to
    // build the same session again, and the sentence confirming it would name
    // the same server on both sides.
    let sessao = read("ui/tela-sessao.js");
    let desenha = js_function(&sessao, "function desenharTrilha(");

    assert!(
        desenha.contains("conhecidosDaTrilha"),
        "the trilha is drawn without the history, so the column only ever holds \
         the server somebody is already in: {desenha}"
    );
    assert!(
        desenha.contains("alvoDoServer"),
        "nothing in the trilha knows which server this session is on, so nothing \
         can be marked as current and nothing can be left out of the list: {desenha}"
    );

    // Read once per session, not once per frame: `desenharTopo` runs twice a
    // second and the shortcut list changes when somebody enters or forgets a
    // server.
    let recarrega = js_function(&sessao, "async function recarregarTrilha(");
    assert!(
        recarrega.contains("invoke(\"conhecidos\")"),
        "the history is not read from the side that keeps it: {recarrega}"
    );
    assert!(
        !desenha.contains("invoke("),
        "drawing the trilha asks Rust for something, and it is drawn twice a \
         second: {desenha}"
    );

    // And the abbreviation is never the only form of the name on screen.
    let veste = js_function(&sessao, "function vestirItemDaTrilha(");
    assert!(
        veste.contains("aria-label"),
        "a trilha button is a sigla or a picture and carries no accessible name, \
         so a screen reader announces three letters: {veste}"
    );
    assert!(
        veste.contains("alt"),
        "the server picture goes into the button with no `alt` decided, so it is \
         announced by its file name or read out beside the name it duplicates: {veste}"
    );

    // The sigla of an address is not the sigla of a name. `192.168.0.7` through
    // `sigla` is `110` — three digits shared by every machine on that network,
    // which is an abbreviation that abbreviates nothing.
    let siglas = js_function(&sessao, "function siglaDoAlvo(");
    assert!(
        siglas.contains("\\d{1,3}"),
        "the address is abbreviated as if it were a name, and every machine on \
         one network comes out with the same three characters: {siglas}"
    );
}

#[test]
fn no_two_screens_claim_the_same_class_name() {
    // The screens are one stylesheet each, loaded one after another into one
    // flat namespace — so two screens choosing the same name is not a clash the
    // browser reports, it is the later sheet quietly winning every tie.
    //
    // This is not hypothetical. `tela-boot.css` calls the polygon diagram
    // `.magi`; the session footer called its three lights `.magi` too, and it
    // loads second. The boot diagram — `position: relative`, absolutely placed
    // children — silently also got `display: flex; align-items: stretch`, and
    // worse, the footer's `.magi li` (one class, one type) outranked the
    // diagram's own `.magi-no` (one class) on the very `<li>`s it was meant to
    // paint. Nothing failed. The boot screen just came out wrong.
    //
    // A name that a *shared* sheet owns is a different thing and stays legal: a
    // screen writing `.ausente[title] { cursor: help }` is refining a primitive
    // from `base.css` on purpose, which is what the load order exists for.
    let shared: BTreeSet<String> = ["base.css", "acessibilidade.css"]
        .iter()
        .flat_map(|name| classes_defined_in(&read(&format!("ui/{name}"))))
        .collect();

    // Everything that is *not* one of the four sheets below, rather than
    // everything named `tela-*`. The prefix spelling had this guard blind to
    // `camada-alerta.css` and `camada-bateria.css` the day they landed — 464
    // channels of new CSS that could neither report a collision nor be reported
    // for one — and it was blind silently, which is the same failure the guard
    // exists to catch, one level up. A guard whose coverage depends on the next
    // author picking a blessed prefix is a guard with a hole in it.
    let not_a_screen = ["base.css", "acessibilidade.css", "tokens.css", "fontes.css"];
    let screens: Vec<String> = ui_files(".css")
        .into_iter()
        .filter(|name| !not_a_screen.contains(&name.as_str()))
        .collect();
    assert!(
        screens.len() >= 2,
        "there is nothing to collide: ui/ ships {} screen stylesheets",
        screens.len()
    );

    let mut owner: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut clashes: Vec<String> = Vec::new();
    for sheet in &screens {
        for class in classes_defined_in(&read(&format!("ui/{sheet}"))) {
            if shared.contains(&class) {
                continue;
            }
            match owner.get(&class) {
                Some(first) => clashes.push(format!(".{class} — {first} and {sheet}")),
                None => {
                    owner.insert(class, sheet.clone());
                }
            }
        }
    }

    assert!(
        clashes.is_empty(),
        "two screen stylesheets define the same class, and the one that loads \
         later wins every tie between them:\n{}",
        clashes.join("\n")
    );
}

// ---------------------------------------------------------------------------
// The Terminal server — the settings screen, rebuilt against the v3 comp.
// ---------------------------------------------------------------------------

/// The markup of one screen, cut out of the page by the id of the next one.
///
/// Cut by the *following* screen and not by `</section>`, because the settings
/// screen nests four `<section>`s of its own — one panel per section button —
/// and the first closing tag would end the slice a third of the way in.
fn screen_markup(page: &str, id: &str, next_id: &str) -> String {
    let page = without_comments(page);
    let Some(after) = page.split(&format!("id=\"{id}\"")).nth(1) else {
        panic!("index.html has no screen with id `{id}`");
    };
    let Some(body) = after.split(&format!("id=\"{next_id}\"")).next() else {
        panic!("`{next_id}` no longer follows `{id}`, so this slice has no end");
    };
    body.to_owned()
}

/// Every `data-glifo="…"` in the page, in document order.
fn drawn_glyphs(page: &str) -> Vec<String> {
    without_comments(page)
        .split("data-glifo=\"")
        .skip(1)
        .filter_map(|piece| piece.split('"').next())
        .map(str::to_owned)
        .collect()
}

#[test]
fn every_glyph_the_page_asks_for_is_one_glifos_js_can_draw() {
    // `glifo()` throws on an unknown name, and the loop at the bottom of
    // `glifos.js` runs over every `[data-glifo]` in the page — so one typo
    // anywhere takes the *whole* script file down with it, and every listener
    // registered after it is never attached. The window then renders, looks
    // right, and does nothing when clicked.
    //
    // This is the guard the v3 comp made worth having: the settings screen alone
    // asks for four of the eight new drawings by name, in markup, with nothing
    // between the attribute and the runtime.
    let page = read("ui/index.html");
    let script = read("ui/glifos.js");

    let Some(table) = script.split("const GLIFOS = {").nth(1) else {
        panic!("glifos.js no longer declares `GLIFOS`, and nothing can be drawn");
    };
    let Some(table) = table.split("\n};").next() else {
        panic!("`GLIFOS` is never closed");
    };
    let table = without_comments(table);

    let asked = drawn_glyphs(&page);
    assert!(!asked.is_empty(), "the page draws no glyph at all");

    for name in asked {
        assert!(
            table.contains(&format!("{name}: [")),
            "index.html asks for the glyph `{name}`, which `glifos.js` cannot \
             draw. `glifo()` throws, and it throws inside the loop at the bottom \
             of that file — so every listener registered after it is never \
             attached, and the window comes out looking right and doing nothing."
        );
    }
}

/// **A configuração se chama configuração, e as seções têm escopo** — U06 e U31.
///
/// «Configurações se chamam "TERMINAL SERVER", embora misturem preferências
/// locais e administração de um servidor.» O nome vinha da comp e descrevia
/// metade do que a tela faz — e a outra metade é justamente a que alguém não
/// encontraria procurando por ela.
///
/// E as seções ficavam numa lista plana. Trocar o microfone vale nesta
/// máquina; renomear o servidor vale para todo mundo que entra nele. A
/// diferença decide se alguém hesita antes de mexer, e uma lista plana a apaga.
/// **Os dois números da API não podem discordar** — U10.
///
/// A gestão de MODs passou a dizer, para um pacote que este build não executa,
/// se quem está para trás é o aplicativo ou o pacote. Essa frase é montada na
/// casca, a partir de um número escrito lá — e um número escrito duas vezes é
/// um número que discorda no dia em que só uma das cópias sobe.
///
/// A consequência seria específica e cara: a tela mandaria alguém atualizar o
/// aplicativo que já está atualizado, ou procurar uma versão nova de um pacote
/// que já é a mais nova.
#[test]
fn a_api_da_casca_bate_com_a_do_nucleo() {
    let rust = read("../../crates/seele-proto/src/mods.rs");
    let casca = without_comments(&read("ui/camada-mods.js"));

    let oferecida = rust
        .split_once("pub const MOD_API_VERSION: u32 = ")
        .and_then(|(_, resto)| resto.split_once(';'))
        .map(|(numero, _)| numero.trim().to_owned())
        .expect("`MOD_API_VERSION` sumiu de seele-proto");
    assert!(
        casca.contains(&format!("const API_DESTE_APLICATIVO = {oferecida};")),
        "a casca diz uma API e o núcleo diz {oferecida}: a gestão de MODs \
         mandaria alguém atualizar o que já está atualizado"
    );

    // E o conjunto aceito, na mesma ordem em que o Rust o escreve.
    let aceitas = rust
        .split_once("pub const APIS_ACEITAS: &[u32] = &[")
        .and_then(|(_, resto)| resto.split_once(']'))
        .map(|(lista, _)| lista.replace(char::is_whitespace, ""))
        .expect("`APIS_ACEITAS` sumiu de seele-proto");
    let esperado = format!(
        "const APIS_QUE_ESTE_APLICATIVO_ACEITA = [{}];",
        aceitas.trim_end_matches(',')
    );
    assert!(
        casca.contains(&esperado),
        "a casca lista outras APIs aceitas que o núcleo: esperava `{esperado}`"
    );
}

#[test]
fn a_configuracao_diz_o_nome_dela_e_o_escopo_de_cada_grupo() {
    let pagina = read("ui/index.html");
    let tela = screen_markup(&pagina, "tela-server", "tela-fim");

    assert!(
        tela.contains(">CONFIGURAÇÕES<"),
        "a tela de configuração voltou a se chamar outra coisa"
    );
    assert!(
        !tela.contains(">TERMINAL SERVER<"),
        "«TERMINAL SERVER» voltou ao título: ele descreve metade do que a tela \
         faz, e a outra metade é a que ninguém encontraria procurando por ela"
    );

    // Os quatro grupos, com nome e com a nota que diz o escopo. O nome sozinho
    // não basta: «este dispositivo» e «este servidor» são a mesma forma de
    // palavra com consequências opostas.
    for grupo in ["ESTE DISPOSITIVO", "MODS", "ESTE SERVIDOR", "APLICATIVO"] {
        assert!(
            tela.contains(&format!(">{grupo}<")),
            "o grupo `{grupo}` sumiu da coluna da configuração"
        );
    }
    assert!(
        tela.contains("Valem nesta máquina, e não no servidor."),
        "o grupo do dispositivo deixou de dizer que o escopo dele é a máquina"
    );
    assert!(
        tela.contains("Valem para todo mundo que entra nele."),
        "o grupo do servidor deixou de dizer que o escopo dele é todo mundo"
    );

    // **Cada `<ul>` é nomeado pelo grupo dele.** Sem isso, quem usa leitor de
    // tela ouve nove seções seguidas e não ouve nenhuma das divisões que a
    // pessoa vidente lê nos cabeçalhos.
    for chave in ["dispositivo", "mods", "servidor", "aplicativo"] {
        assert!(
            tela.contains(&format!("aria-labelledby=\"server-grupo-{chave}\"")),
            "a lista do grupo `{chave}` não é anunciada pelo nome do grupo"
        );
    }

    // **E a saída está no cabeçalho, e não no fim da coluna que rola** — U33.
    let (antes_do_corpo, _) = tela
        .split_once("class=\"server-corpo\"")
        .expect("o corpo da configuração");
    assert!(
        !antes_do_corpo.contains("id=\"server-fechar\""),
        "VOLTAR voltou para a coluna de navegação, que rola: numa janela baixa \
         ele sai da tela, e sair da configuração passa a depender de rolar até \
         o fim de uma lista de nove seções"
    );
    assert!(
        tela.contains("id=\"server-contexto\""),
        "a faixa de contexto sumiu: uma tela que cobre a janela inteira precisa \
         dizer que a voz continua atrás dela"
    );
}

#[test]
fn every_section_of_the_settings_screen_carries_the_panel_and_the_heading_it_opens() {
    // The four sections are buttons in markup and the heading strings travel
    // *with* them, on `data-titulo` and `data-sub`, rather than in a table in
    // JavaScript. That is the whole reason this can be checked at all — and the
    // failure it catches is the one a fifth section would arrive with: a button
    // whose `data-painel` names a panel that is not in the page, which reads as
    // a section that opens nothing and blanks the heading on the way.
    let page = read("ui/index.html");
    let server = screen_markup(&page, "tela-server", "tela-fim");

    let mut sections = Vec::new();
    for rest in server.split("<button ").skip(1) {
        let Some(end) = rest.find('>') else { continue };
        let tag = &rest[..end];
        if attribute(tag, "class").as_deref() != Some("server-secao") {
            continue;
        }
        let Some(id) = attribute(tag, "id") else {
            panic!("a section button has no id, so no script can mark it: <{tag}>");
        };
        for name in ["data-painel", "data-titulo", "data-sub"] {
            assert!(
                attribute(tag, name).is_some(),
                "the section `{id}` carries no `{name}`, so opening it leaves the \
                 heading of the panel empty: <{tag}>"
            );
        }
        let panel = attribute(tag, "data-painel").unwrap_or_default();
        assert!(
            page.contains(&format!("id=\"{panel}\"")),
            "the section `{id}` opens `{panel}`, which is not in the page"
        );
        sections.push(id);
    }

    // **A ordem passou a ser a dos grupos, e não a da comp** — U31 da
    // auditoria de 20/09/2026: «Microfone, Aparência, Identidade, MODs,
    // Versões, Malha e Atualização no mesmo nível, sem distinguir frequência e
    // escopo.» Sete seções numa lista plana fazem cada uma parecer o mesmo tipo
    // de coisa, e elas não são: trocar o microfone vale nesta máquina,
    // renomear o servidor vale para todo mundo que entra nele.
    //
    // Os quatro grupos são os que a auditoria nomeia. O que cada um contém
    // continua sendo uma decisão registrada — as razões de cada seção estar
    // onde está seguem escritas ao lado dela em `index.html` —, e o que esta
    // lista guarda é que nenhuma se perdeu no caminho.
    assert_eq!(
        sections,
        [
            // ---- ESTE DISPOSITIVO: vale nesta máquina ----
            //
            // Áudio abre porque é a razão de esta tela ser camada e não tela:
            // trocar de microfone no meio de uma conversa é o caso de uso.
            "secao-audio",
            "secao-atalhos",
            // **APARÊNCIA voltou na 0.9.0**, e o registro de por que ela tinha
            // saído fica, porque ele é a regra que a volta teve de respeitar:
            // omitir o que não se tem em vez de desenhá-lo morto. Ela tem o que
            // dizer — onde o contraste mora —, e por isso existe.
            "secao-aparencia",
            "secao-identidade",
            // ---- MODS: o que está no disco, e a quem se disse sim ----
            //
            // Grupo próprio e não dentro de «este dispositivo», embora seja
            // desta máquina: U12 mede que a gestão de MODs sozinha «junta
            // ativação, catálogo, cache, consentimentos e contadores numa
            // página longa». Uma coisa desse tamanho não é um item de lista.
            "secao-mods",
            // ---- ESTE SERVIDOR: vale para todo mundo que entra nele ----
            //
            // As duas só aparecem para quem tem a permissão, e por isso o grupo
            // inteiro costuma estar ausente — que é melhor que um buraco no meio
            // de uma lista plana, como era antes.
            "secao-porta",
            "secao-servidor",
            // ---- APLICATIVO: o SEELE em si ----
            //
            // VERSÕES continua vizinha de MODS na leitura — uma versão guardada
            // ao lado existe **por causa** de um MOD feito para ela —, mas o
            // grupo dela é o do aplicativo: o que se escolhe é qual SEELE roda.
            "secao-versoes",
            // MALHA: quanto da internet **deste computador** vai para servir
            // outras pessoas. A auditoria pediu que ela deixasse de se chamar
            // pelo nome da arquitetura (ver `data-titulo`), e o nome do grupo é
            // o que a põe onde alguém a procuraria.
            "secao-malha",
            "secao-atualizacao",
        ],
        "a configuração perdeu uma seção ou trocou a ordem dos grupos"
    );
}

/// The server section is offered from the snapshot, and never decided here.
///
/// The rule this screen shares with the channels column: hiding a control is
/// **not** what stops anybody. `AdministerServer` is checked by the PERMISSIONS at
/// the instant of the verb, and a rename asked without it comes back as
/// `Alert`/`PermissionDenied`. What the boolean buys is not offering what the
/// server was going to refuse.
///
/// Two halves, and the markup half is the one that fails silently: before the
/// first snapshot this window does not know which permissions it has, so a
/// section born visible promises for one frame what it may not be able to
/// carry out — and on the entry screen, where there is no session at all, it
/// would promise it for as long as somebody looked at it.
#[test]
fn the_server_section_is_offered_from_the_snapshot_and_never_judged_by_the_screen() {
    let page = read("ui/index.html");
    let item = tag_with_id(&page, "secao-servidor-item");
    assert!(
        item.contains("hidden"),
        "the server section is born visible, so between the window opening and \
         the first snapshot it offers a permission nobody has yet — and from the \
         entry screen, where there is no session, it never stops: <{item}>"
    );

    let desenha = js_function(&read("ui/tela-server.js"), "function desenharServidor(");
    assert!(
        desenha.contains("may_customise_server"),
        "the section is shown without asking whether this session may customise \
         anything, so it offers what the server will refuse: {desenha}"
    );
    assert!(
        desenha.contains("$(\"secao-servidor-item\")"),
        "nothing reaches for the section item, so the boolean is read and then \
         dropped: {desenha}"
    );

    // And nothing in this screen refuses on its own. A shell that returned early
    // on the boolean instead of sending would be a shell whose idea of the
    // permission is the one that counts — which is exactly the failure mode
    // `specs/08-seguranca.md` puts on the server side.
    for verbo in [
        "async function renomearServidor(",
        "async function escolherIcone(",
        "async function tirarIcone(",
    ] {
        let corpo = js_function(&read("ui/tela-server.js"), verbo);
        assert!(
            !corpo.contains("may_customise_server"),
            "`{verbo}…` checks the permission before asking, so the refusal that \
             matters stops being the server's: {corpo}"
        );
    }
}

/// The ceiling is on screen before the picker opens, and it is not written here.
///
/// Two failures, and they are different. The first is a person walking to the
/// folder with the photographs, picking one, and only then being told that a
/// server takes eight kilobytes — a trip spent to read a rule that could have
/// been on screen the whole time.
///
/// The second is subtler and is what the numbers coming from Rust prevents: two
/// copies of a protocol constant drift, and the drift shows up as a screen
/// promising to accept what the server refuses. `regras_de_previa` made the same
/// choice for the preview ceiling, and this follows it.
#[test]
fn the_ceiling_for_the_picture_is_said_before_the_picker_and_never_written_in_the_page() {
    let page = without_comments(&read("ui/index.html"));
    let server = read("ui/tela-server.js");

    let Some(regra) = page.find("id=\"server-icone-regra\"") else {
        panic!("the page has nowhere to write what a picture may weigh");
    };
    let Some(escolher) = page.find("id=\"server-icone-escolher\"") else {
        panic!("the page has no button that opens the picker");
    };
    assert!(
        regra < escolher,
        "the sentence with the ceiling is not above the button that opens the \
         picker, so it is read after the trip to the folder rather than before it"
    );

    let escreve = js_function(&server, "function desenharRegraDoIcone(");
    for campo in ["limite_bytes", "lado"] {
        assert!(
            escreve.contains(campo),
            "the sentence does not use `{campo}` from the rules Rust sent, so the \
             number on screen is one this page decided: {escreve}"
        );
    }

    // The refusal quotes the protocol's own number, and not the copy the shell
    // keeps to write the sentence above. They should agree; when they stop
    // agreeing, the one that decides is the one the person has to believe.
    let frase = js_function(&server, "function fraseDeIcone(");
    assert!(
        frase.contains("limit_bytes"),
        "the refusal does not carry the number the `ConnectionError` brought, so \
         somebody is told «too heavy» and never how heavy is not: {frase}"
    );
    assert!(
        frase.contains("IconNotAPicture") && frase.contains("IconTooBig"),
        "one of the two refusals has no sentence of its own, and they ask \
         different things of the person reading — a photograph can be shrunk \
         and a PDF cannot be made into a picture: {frase}"
    );
}

/// The bytes are fetched when the revision moves, and on no other frame.
///
/// The whole reason `icon_revision` exists. The snapshot crosses the bridge as
/// JSON twice a second; the picture is up to eight kilobytes that change when
/// somebody presses a button. A screen that read the bytes off every redraw
/// would be serialising them a hundred and twenty times a minute for a value
/// that moved once.
#[test]
fn the_picture_crosses_the_bridge_only_when_its_revision_moved() {
    let server = read("ui/tela-server.js");
    let sincroniza = js_function(&server, "async function sincronizarIcone(");

    let Some((antes, _)) = sincroniza.split_once("invoke(\"icone_do_server\")") else {
        panic!("nothing fetches the picture at all: {sincroniza}");
    };
    assert!(
        antes.contains("icon_revision"),
        "the bytes are fetched without the revision being compared first, so \
         every redraw pulls the whole picture across the bridge: {sincroniza}"
    );

    // And the drawing is separate from the fetching, which is what lets a redraw
    // be free: the panel and the header are painted from what is already here.
    let pinta = js_function(&server, "function pintarIcone(");
    assert!(
        !pinta.contains("invoke("),
        "painting the picture asks Rust for something, so a redraw is not free \
         after all: {pinta}"
    );
    // The picture is drawn in one place from here — the settings preview. It
    // used to be two, and the second was an `<img>` inside the 32px window bar,
    // where it came out sliced in half and shoved the rest of the bar off the
    // edge. The rail tile draws the same picture from the same
    // `iconeDesenhado.uri` on the next frame, so it needs no second write.
    assert!(
        pinta.contains("server-icone-previa"),
        "`pintarIcone` never touches the preview, so the place the picture is \
         drawn goes stale: {pinta}"
    );
    assert!(
        !pinta.contains("topo-server-icone"),
        "the picture is being drawn into the window bar again; it is 32px tall \
         and the picture comes out cut in half: {pinta}"
    );

    // A session that ends forgets it. The revision of a *new* session starts
    // counting from zero again, so without this the picture of the server
    // somebody just left would sit in the header of the one they just entered —
    // and a server with no picture sends nothing that would contradict it.
    let esquece = js_function(&server, "function esquecerIcone(");
    assert!(
        esquece.contains("null"),
        "leaving a server does not forget its picture: {esquece}"
    );
    let ouvinte = server
        .split("listen(\"seele://event\"")
        .nth(1)
        .unwrap_or_default();
    assert!(
        ouvinte.contains("ConnectStageChanged") && ouvinte.contains("esquecerIcone("),
        "nothing forgets the picture when a new session is being built, and that \
         is the only notice that arrives *before* there is a session to compare \
         revisions with: {ouvinte}"
    );
}

/// This screen still has no SALVAR, and the newest section did not bring one.
///
/// The rule of the whole window, and the usability review listed it under "what
/// is already right and was not touched": the choice takes effect at once, and
/// what is drawn is what is **in force**. The name field is not somebody's
/// intention held until they confirm it — it is the name the server is using.
///
/// The section that would most naturally have brought a save button is this one,
/// because it holds the only free-text field on the screen.
#[test]
fn the_server_section_confirms_by_state_and_not_by_a_save_button() {
    let page = read("ui/index.html");
    // Até `tela-fim`, que é a próxima coisa depois do painel, e **não** até
    // `ajuda-titulo`, que fica muito adiante: a fatia larga engolia todos os
    // diálogos entre os dois, e o `SALVAR` que o perfil ganhou — porque a comp o
    // desenha, e porque sem ele um nome digitado se perdia ao fechar — caía
    // dentro dela. Um guarda que acusa o vizinho é um guarda que se aprende a
    // ignorar.
    let painel = screen_markup(&page, "painel-servidor", "tela-fim");
    for palavra in ["SALVAR", "APLICAR", "DESCARTAR"] {
        assert!(
            !painel.contains(palavra),
            "the server section grew a `{palavra}` button. This screen confirms \
             by state: the field shows the name in force, and a button promising \
             that nothing changes until it is pressed would be false the moment \
             somebody else with the same permission renames the server"
        );
    }

    let server = read("ui/tela-server.js");
    let rodape = server
        .split("// ------------------------------------------------------------------- ligação")
        .nth(1)
        .unwrap_or_default();
    assert!(
        rodape.contains("\"change\""),
        "nothing sends the name when the field is left, so the only way to \
         rename is a button this screen does not have: {rodape}"
    );

    // And the field shows what is in force rather than what was typed — except
    // while somebody is typing into it, because a screen that overwrites the
    // caret twice a second is a screen nobody can write in.
    let desenha = js_function(&server, "function desenharServidor(");
    assert!(
        desenha.contains("activeElement"),
        "the name field is rewritten from the snapshot even while it has the \
         caret, so typing into it is undone twice a second: {desenha}"
    );
    assert!(
        desenha.contains("snapshot.server"),
        "the field is not filled from the name the server is using, so it shows \
         what was typed instead of what is in force: {desenha}"
    );
}

#[test]
fn every_key_the_shortcut_table_names_is_one_a_script_listens_for() {
    // The shortcuts section is a *list of what the keys are*. A list like that
    // has exactly one way to fail, and it fails silently — the key it names
    // stops being the key that acts, and the screen goes on documenting a
    // program that no longer exists. Nothing about that is visible from the
    // page, from the script, or from a running window.
    //
    // **One of them is now reboundable, and the sentence that used to be here
    // said the opposite**: «they are fixed: there is no editable table and
    // nowhere to save a rebinding». `preferences` gained a `push_to_talk_key`
    // line, so it does.
    //
    // The row did not grow a control somewhere else on the screen — it *became*
    // the control. The `<kbd>` for talking is a `<button>`, and the script
    // writes both its `data-tecla` and its label from what is on disk. Two
    // separate things could disagree; one thing cannot. What this test still
    // checks is the *defaults*, which is what the page ships with and what the
    // script falls back to.
    //
    // `data-tecla` carries the name the *browser* gives the key, not the word a
    // person reads, so this compares the row against the listener rather than
    // against another label.
    let page = read("ui/index.html");
    let script = without_comments(&scripts());

    let mut keys = Vec::new();
    for piece in without_comments(&page).split("data-tecla=\"").skip(1) {
        let Some(key) = piece.split('"').next() else {
            continue;
        };
        keys.push(key.to_owned());
    }

    assert!(
        keys.len() >= 4,
        "the shortcuts section lists {} keys; it is documentation, and an empty \
         list of shortcuts is the same as not having the section",
        keys.len()
    );

    for key in keys {
        assert!(
            script.contains(&format!("\"{key}\"")),
            "the shortcuts section says `{key}` does something, and no script \
             listens for it — so the screen documents a program this is not"
        );
    }

    // **A outra metade da tabela: os gestos que o sistema entrega prontos.**
    //
    // Ctrl+V não é uma tecla que esta janela escute. Nenhum script procura
    // aquela combinação, porque o sistema a traduz antes: ela chega como o
    // evento `paste`, com o conteúdo da área de transferência dentro. É também
    // por isso que ela funciona com o Cmd do Mac sem ninguém aqui saber disso.
    //
    // Uma linha assim escaparia da conferência acima por não ter `data-tecla`
    // — e uma exceção que não é conferida é uma linha livre para mentir. Então
    // ela declara `data-evento`, e o que se cobra é o mesmo por outro caminho:
    // que exista script escutando aquele evento.
    let mut eventos = Vec::new();
    for piece in without_comments(&page).split("data-evento=\"").skip(1) {
        let Some(evento) = piece.split('"').next() else {
            continue;
        };
        eventos.push(evento.to_owned());
    }

    for evento in eventos {
        assert!(
            script.contains(&format!("addEventListener(\"{evento}\"")),
            "a tabela diz que `{evento}` faz alguma coisa, e nenhum script o escuta"
        );
    }
}

#[test]
fn o_padrao_da_tecla_de_falar_e_o_mesmo_nos_dois_lugares() {
    // A tecla de falar é escrita em dois lugares que **têm** de concordar: o
    // `data-tecla` que o HTML traz — o que a lista mostra antes de qualquer
    // leitura de disco — e o `teclaDeFalar` de que o script parte quando não há
    // nada gravado, ou quando a leitura falha.
    //
    // Discordando, a janela abre dizendo que se fala numa tecla e falando
    // noutra, até alguém abrir a configuração. É o mesmo defeito que custou uma
    // versão em campo, na escala pequena: um lado declarando o que o outro
    // decide.
    //
    // Não dá para fundi-los num só — o HTML precisa de um valor antes de o
    // script rodar. Então eles ficam dois e são conferidos aqui.
    let page = read("ui/index.html");
    let script = without_comments(&scripts());

    let do_html = page
        .split("id=\"server-tecla-falar\"")
        .nth(1)
        .and_then(|resto| resto.split("data-tecla=\"").nth(1))
        .or_else(|| {
            // O atributo pode vir antes do `id`, e a ordem é do gosto de quem
            // escreveu o HTML, não uma regra. Então procura-se dos dois lados.
            page.split("server-atalho-tecla-troca")
                .nth(1)
                .and_then(|resto| resto.split("data-tecla=\"").nth(1))
        })
        .and_then(|resto| resto.split('"').next())
        .expect("o botão da tecla de falar tem de trazer um `data-tecla`");

    let do_script = script
        .split("let teclaDeFalar = \"")
        .nth(1)
        .and_then(|resto| resto.split('"').next())
        .expect("o script tem de partir de uma tecla");

    assert_eq!(
        do_html, do_script,
        "a lista de atalhos abre dizendo `{do_html}` e o script escuta `{do_script}`"
    );
}

#[test]
fn the_settings_screen_omits_what_the_product_lacks_instead_of_drawing_it_dead() {
    // This screen deliberately inverts the convention the auth screen follows.
    // There, the frame stays and the value is missing, because the gap is the
    // protocol's and worth showing. Here it does not: on a screen whose entire
    // purpose is being simple, half a dozen greyed-out controls with an
    // explanation beside each is noise, and every one of them is a promise.
    //
    // Each word below is a control the v3 comp draws and this product cannot
    // carry out. Two of them would take a written decision back:
    //
    // - `RUÍDO` — ADR 0007 kept C/C++ DSP out of v1 and made headphones a
    //   documented requirement. The control would exist to do nothing.
    // - `TEMA` — ADR 0014 freezes the palette, and a second theme is a second
    //   canonical palette.
    //
    // And `SALVAR`/`DESCARTAR` are the third: the choice applies now, so there
    // is nothing pending for a button to confirm (comp inventory §8.1). A
    // `SALVAR` on an audio panel promises that nothing changes until it is
    // pressed, which is false for sound — you have to hear the effect to know
    // you chose right.
    //
    // Comments are stripped, and that is the point: the paragraphs above the
    // markup have to be able to say *why* each of these is absent.
    let page = read("ui/index.html");
    let server = screen_markup(&page, "tela-server", "tela-fim");

    // **`SALVAR`/`DESCARTAR` deixaram de ser absolutos, e só no painel de MODs.**
    //
    // A razão escrita acima é sobre o painel de áudio, e continua valendo lá: a
    // escolha vale na hora, e um SALVAR prometeria que nada muda até ser
    // apertado — falso para som, que você precisa ouvir para saber se acertou.
    //
    // No painel de MODs o pendente existe de verdade. Aplicar ali não é
    // instantâneo nem local: muda o que o servidor exige e derruba todo mundo
    // que está dentro. Enquanto cada interruptor gravava na hora, ligar três
    // MODs custava três quedas ao operador e a quem estava com ele — relatado
    // no documento de ajustes de 18/09, que pede «no máximo um ciclo de
    // reconexão por aplicação, não um por switch». O rascunho é o que torna
    // isso possível, e um rascunho sem SALVAR não é um rascunho.
    //
    // O recorte é o painel, e não a tela: tudo o mais continua conferido
    // inteiro, e o áudio segue sem botão nenhum de confirmar.
    let fora_dos_mods = sem_secao(&server, "painel-mods");
    for absent in ["SALVAR", "DESCARTAR"] {
        assert!(
            !names(&fora_dos_mods, absent),
            "the settings screen draws `{absent}` outside the MODs panel. The \
             choice applies now everywhere else, so there is nothing pending \
             for a button to confirm."
        );
    }
    assert!(
        names(&server, "SALVAR"),
        "the MODs panel lost its `SALVAR`: without it every switch writes to \
         the server on the spot, and turning three MODs on drops everybody in \
         the session three times"
    );

    for absent in ["RUÍDO", "TEMA", "GANHO", "VOLUME", "GERAR", "COPIAR"] {
        assert!(
            !names(&server, absent),
            "the settings screen draws `{absent}`, which nothing in this product \
             can carry out. The rule here is to omit, not to draw it dead — and \
             two of these would take an ADR back."
        );
    }
}

#[test]
fn both_sides_of_the_audio_picker_are_drawn_by_the_same_code() {
    // The output row used to be one dead control with a `title` explaining that
    // the machine could not enumerate speakers. It can now, so that guard was
    // retired — a disabled row asserting a limitation that no longer exists is
    // a test that keeps a bug alive.
    //
    // What replaces it is the risk the wiring actually introduced. Input and
    // output are picked in exactly the same dance — list, read the choice,
    // choose, and show what *opened* rather than what was asked for — and the
    // cheapest way to add the second one is to copy the first. Two copies drift
    // on the first fix somebody makes to one side only, and the drift is
    // invisible: both lists keep drawing, one of them just stops telling the
    // truth about which device is open.
    let script = without_comments(&scripts());

    assert!(
        !script.contains("desenharMicrofones") && !script.contains("linhaDeMicrofone"),
        "a capture-only drawing function is back, which is how the two sides start \
         to differ"
    );

    let tabela = body_of(&scripts(), "const LADOS");
    for comando in [
        "microfones",
        "saidas",
        "microfone_escolhido",
        "saida_escolhida",
        "escolher_microfone",
        "escolher_saida",
    ] {
        assert!(
            tabela.contains(comando),
            "`{comando}` is not in the LADOS table, so one side of the picker is \
             wired somewhere else and can be changed without the other:\n{tabela}"
        );
    }

    // The whole point of the screen: what was chosen and what opened are two
    // different questions, and both sides have to answer the second one.
    let marcar = body_of(&scripts(), "function marcarLinhas");
    for campo in ["capture", "playback"] {
        assert!(
            marcar.contains(campo),
            "`marcarLinhas` never reads `snapshot.{campo}`, so that side draws the \
             preference and calls it reality:\n{marcar}"
        );
    }

    // And nothing may seed rows in the markup: a hard-coded device is one this
    // machine may not have.
    let page = without_comments(&read("ui/index.html"));
    let Some(after) = page.split("id=\"lista-saidas\"").nth(1) else {
        panic!("the output list is gone, and with it the only place the enumeration can be drawn");
    };
    let Some(lista) = after.split("</ul>").next() else {
        panic!("the output list is never closed");
    };
    assert!(
        !lista.contains("<li"),
        "the output list ships a row in the markup, which names a device before \
         anybody asked this machine what it has:{lista}"
    );
}

// `the_switch_that_hides_the_captions_never_hides_its_own_caption` stood here.
// It guarded one row of the APARÊNCIA section: the simple-captions switch had to
// write its own description outside the layer it turned off, or the way back
// vanished with it.
//
// The switch is gone, the section is gone, and the layer is not a mode any more —
// the note beside a control is part of the control and is always on screen. A
// guard for a row that cannot exist is a guard that passes for the wrong reason,
// so it went with them. What replaced it lives at the bottom of this file:
// `the_captions_mode_does_not_come_back_by_accident`.

/// The names a script declares at its top level.
///
/// Column zero and nothing else: anything indented is inside a function and
/// belongs to that function.
fn globals_declared_in(script: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for channel in without_comments(script).lines() {
        let Some(rest) = ["const ", "let ", "var ", "function ", "class "]
            .iter()
            .find_map(|keyword| channel.strip_prefix(keyword))
        else {
            continue;
        };
        // `const { invoke } = window.__TAURI__.core` binds through a link_state
        // rather than a name, and this does not try to read patterns.
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
            .collect();
        if !name.is_empty() {
            found.insert(name);
        }
    }
    found
}

#[test]
fn no_two_scripts_declare_the_same_top_level_name() {
    // The same hazard as the stylesheet check above, on the other half of the
    // split, and the failures are worse. ADR 0019 chose no modules, so the nine
    // scripts share one scope — and what changes when a file is split is not
    // visibility, it is what happens when two of them pick one name:
    //
    // - two top-level `const`s or `let`s with one name is a `SyntaxError`, and
    //   it kills the *whole* second script. Every listener it was going to
    //   register is never registered, so a screen's buttons simply do nothing.
    //   Nothing else in this suite would notice: the page loads, the markup is
    //   there, and only the console says why.
    // - two `function`s with one name is silent. The one that loads later wins
    //   every call, including the calls made from the file that declared the
    //   other one — which is `.magi` in `no_two_screens_claim_the_same_class_name`,
    //   one layer down and harder to see.
    //
    // Sharing on purpose stays legal and is how this frontend works: a screen
    // *reads* `medido`, `blocos`, `volumes` and `comecoDaSessao` from the file
    // that declares them. What it may not do is declare them again.
    let mut owner: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut clashes: Vec<String> = Vec::new();

    for name in ui_files(".js") {
        for global in globals_declared_in(&read(&format!("ui/{name}"))) {
            match owner.get(&global) {
                Some(first) => clashes.push(format!("`{global}` — {first} and {name}")),
                None => {
                    owner.insert(global, name.clone());
                }
            }
        }
    }

    assert!(!owner.is_empty(), "no script declares anything at all");
    assert!(
        clashes.is_empty(),
        "two scripts declare the same top-level name into the one scope they \
         share. For `const` and `let` that is a SyntaxError that takes the whole \
         second file down, listeners included; for `function` it is silent, and \
         the later file wins every call:\n{}",
        clashes.join("\n")
    );
}

#[test]
fn voltar_a_um_servidor_leva_o_apelido_daquela_vez() {
    // **O comportamento sobreviveu à tela; o guarda mudou de endereço.**
    //
    // Ele cobrava que apertar um visitado preenchesse o campo de apelido com o
    // nome daquela visita. A entrada da 0.9.0 não tem campo — e a razão de
    // aquilo existir continua: o apelido é único por servidor, e voltar com
    // outro nome é chegar como outra pessoa, perdendo o que aquela conta tinha.
    //
    // Agora quem carrega é o `camada-servidores.js`, que passa o apelido
    // gravado junto do endereço. E o que se cobra é isso: que a linha leve o
    // nome, e que quem conecta o receba.
    let script = without_comments(&read("ui/camada-servidores.js"));

    assert!(
        script.contains("conhecido.apelido"),
        "a linha de um servidor conhecido deixou de carregar o apelido daquela \
         visita; voltar com outro nome é chegar como outra pessoa"
    );
    // **A busca pelo apelido desta máquina mudou de casa, e o guarda com ela.**
    //
    // Ela morava aqui, e era a única porta que a fazia: quem apertava `HOSPEDAR
    // AQUI` não passava por este diálogo e conectava **sem nome nenhum** —
    // «coloquei meu nome na tela inicial e hospedei, o server não puxou meu
    // nome». Ela desceu para o `conectar`, que é por onde toda conexão passa:
    // hospedar, reconectar, a trilha e este diálogo.
    let boot = without_comments(&read("ui/tela-boot.js"));
    let conectar = js_function(&boot, "async function conectar(");
    assert!(
        conectar.contains("apelido_local"),
        "sem apelido gravado para aquele servidor, nada busca o desta \
         máquina:\n{conectar}"
    );
    // **E o recurso não mora mais aqui.** Ele era a palavra `pessoa`, igual
    // para todo mundo — e por ADR 0017 o apelido pertence a uma chave, então a
    // segunda pessoa sem nome de qualquer servidor colidia por construção e era
    // recusada na porta. Passou para o `main.rs`, que tem a impressão desta
    // máquina e monta um nome que não colide e é estável entre aberturas.
    //
    // Quem garante que o vazio não chega ao roster é o guarda do outro lado,
    // `o_recurso_do_apelido_sai_da_impressao_desta_maquina`, e não este.
    assert!(
        !conectar.contains("\"pessoa\""),
        "a casca voltou a inventar um recurso de apelido.\n\
         Uma constante compartilhada colide por construção com a segunda pessoa \
         que não se nomear; quem sabe montar um nome que não colide é o Rust, \
         porque só ele tem a impressão da chave desta máquina:\n{conectar}"
    );
}

#[test]
fn a_failure_this_screen_cannot_name_still_says_what_it_was() {
    // `FRASES` is a list of hand-written sentences, and the Rust side keeps
    // growing error variants — three landed today alone. Every variant that
    // arrives without a sentence used to reach a dead end that read «FALHA
    // DESCONHECIDA», which tells the person nothing and tells whoever has to fix
    // it less: somebody reporting «I cannot reconnect» had nothing else to pass
    // on.
    let script = without_comments(&scripts());

    assert!(
        !script.contains("\"FALHA DESCONHECIDA\""),
        "an unnamed failure is still drawn as a dead end, so the one screen that \
         sees the error keeps it to itself"
    );

    let body = body_of(&scripts(), "function desconhecida");
    assert!(
        body.contains("JSON.stringify") && body.contains("erro"),
        "the fallback names no detail, so it is the dead end under another \
         wording:\n{body}"
    );
}

#[test]
fn o_andamento_da_conexao_e_frase_e_nao_enfeite() {
    // Havia três marcas de subsistema que acendiam enquanto a conexão andava, e
    // este guarda cobrava que elas parecessem diferentes em carga — senão a
    // animação dizia «estou fazendo algo» sem dizer o quê.
    //
    // **A 0.9.0 as tirou.** A leitura de boot da comp é fixa e verdadeira: diz o
    // que o produto é, e não o que está acontecendo agora. O andamento continua
    // sendo dito, e por quem sempre soube dizê-lo em palavra — o `boot-etapa`,
    // que é `role="status"` e nomeia a etapa.
    //
    // O que se cobra é que o andamento não volte a ser só forma: se as marcas
    // voltarem, elas têm de vir com o guarda que as cobrava.
    let pagina = without_comments(&read("ui/index.html"));
    let script = without_comments(&scripts());

    assert!(
        pagina.contains("id=\"boot-etapa\""),
        "sumiu a frase que diz em que etapa a conexão está"
    );
    assert!(
        script.contains("mostrarEtapa"),
        "a frase existe e ninguém a escreve: o andamento voltou a ser silêncio"
    );
    assert!(
        !pagina.contains("boot-subsistema"),
        "as marcas de subsistema voltaram; elas dizem «algo está acontecendo» \
         sem dizer o quê, e o guarda que cobrava a diferença entre os estados \
         delas saiu junto com elas"
    );
}

#[test]
fn creating_a_room_is_offered_by_permission_and_sized_by_the_server() {
    let body = body_of(&scripts(), "function desenharCanais");

    // Offered, not enforced. The server refuses `CreateVoiceRoom` from anybody
    // without `ManageVoiceRooms`, and `seele-conformance` proves the refusal comes
    // from there — this is the shell not putting up a control that would fail.
    // The distinction matters because the opposite reading (hide it and call it
    // secured) is the one the `connection` walks straight through.
    assert!(
        body.contains("may_manage_voice_rooms"),
        "the screen offers the create forms without asking whether this person may \
         create, so it either hides them from the host or shows them to everybody"
    );

    // The size of a new room is the server's answer, not a number typed in here.
    // Whoever hosts already chose one when they set the server up, and repeating
    // their choice beats inventing a default in JavaScript.
    assert!(
        body.contains("voice_rooms[0].limit") || body.contains("limit"),
        "the default seat count no longer comes from a room that already exists, \
         so the shell is deciding how big a room should be:\n{body}"
    );

    // And the two commands have to be reached by their written names, or the
    // guard that ties calls to registered commands goes blind — which it did,
    // twice in one day, in this very file and in the settings screen.
    let script = without_comments(&scripts());
    for comando in ["invoke(\"criar_voice_room\"", "invoke(\"criar_linha\""] {
        assert!(
            script.contains(comando),
            "`{comando}…` is not written out anywhere, so the command name reaches \
             `invoke` through a variable and no static check can follow it"
        );
    }
}

// ---------------------------------------------------------------------------
// Reaching the bottom of a list.
// ---------------------------------------------------------------------------

/// Elements that carry no closing tag, so they never open a level.
const VOID_TAGS: &[&str] = &["meta", "link", "img", "input", "br", "hr", "source"];

/// Every `<ul>`/`<ol>` the page leaves **empty**, with the classes of every
/// element enclosing it — outermost first, the list's own classes last.
///
/// Emptiness is the whole test, and it is the narrow thing that makes this
/// guard true rather than merely broad. A list written out in `index.html` has
/// as many rows as the page has: `.server-atalhos` is four shortcuts, `.luzes` is
/// three subsystems, and no servidor can make either longer. A list the page leaves
/// empty is one a script fills from a `Snapshot`, and nothing in the protocol
/// caps how many voice_rooms, Linhas, people, messages, devices or visited servers come
/// back. Those are the ones that can outgrow the window.
///
/// So the distinction is not "long" against "short" — nobody can measure that
/// from the source — it is *who decides the length*. The page, or the server.
///
/// It also cannot be quietly silenced. Making this guard shut up means typing
/// rows into a list a script is about to replace wholesale, which is a change
/// the next reader of the diff would ask about.
fn empty_lists_with_their_ancestry(page: &str) -> Vec<(String, Vec<String>)> {
    let page = without_comments(page);
    let mut open: Vec<String> = Vec::new();
    let mut found: Vec<(String, Vec<String>)> = Vec::new();
    let mut rest = page.as_str();

    while let Some(lt) = rest.find('<') {
        let after = &rest[lt + 1..];
        let Some(gt) = after.find('>') else { break };
        let tag = &after[..gt];
        let body = &after[gt + 1..];
        rest = body;

        // `<!doctype …>` and its kind open nothing.
        if tag.starts_with('!') || tag.starts_with('?') {
            continue;
        }
        if tag.starts_with('/') {
            open.pop();
            continue;
        }

        let name: String = tag
            .split(|c: char| c.is_whitespace())
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let classes = attribute(tag, "class").unwrap_or_default();

        if name == "ul" || name == "ol" {
            let text = body.split('<').next().unwrap_or_default();
            let closes = body[text.len()..].starts_with(&format!("</{name}"));
            if text.trim().is_empty() && closes {
                let Some(id) = attribute(tag, "id") else {
                    panic!("an empty <{name}> carries no id, so nothing can fill it: <{tag}>");
                };
                let mut chain = open.clone();
                chain.push(classes.clone());
                found.push((id, chain));
            }
        }

        if VOID_TAGS.contains(&name.as_str()) || tag.ends_with('/') {
            continue;
        }
        open.push(classes);
    }

    // The walk above is only worth trusting if it comes out level. An unbalanced
    // page — or a `>` inside an attribute value — would leave a residue here,
    // and every ancestry this function reported would be wrong by that much.
    assert!(
        open.is_empty(),
        "the page does not close everything it opens, so this walk cannot say \
         what encloses what; {} level(s) left open",
        open.len()
    );
    found
}

/// Class names whose rule makes the element a scroll container.
fn classes_that_scroll(css: &str) -> BTreeSet<String> {
    let css = without_comments(css);
    let mut found = BTreeSet::new();
    for block in css.split('}') {
        let Some((selector, declarations)) = block.split_once('{') else {
            continue;
        };
        let scrolls = declarations.split(';').any(|declaration| {
            let Some((property, value)) = declaration.split_once(':') else {
                return false;
            };
            matches!(property.trim(), "overflow" | "overflow-y")
                && (value.contains("auto") || value.contains("scroll"))
        });
        if !scrolls {
            continue;
        }
        for piece in selector.split('.').skip(1) {
            let name: String = piece
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            if !name.is_empty() {
                found.insert(name);
            }
        }
    }
    found
}

#[test]
fn every_list_the_server_fills_lives_inside_something_that_scrolls() {
    // How this was found: somebody with more than a screenful of voice_rooms asked how
    // to see the rest. There was no way. `.canais` was `flex: 0 0 auto`, no panel
    // in the channel column declared `overflow-y`, and `base.css` puts
    // `overflow: hidden` on `body` — so the column grew past the window and the
    // window would not scroll behind it either. The rooms at the bottom did not
    // exist.
    //
    // The failure is silent twice over: nothing errors, and with the four rooms
    // a test server has, nothing looks wrong.
    let page = read("ui/index.html");
    let scrolls = classes_that_scroll(&styles());
    assert!(
        scrolls.len() >= 3,
        "no stylesheet declares a scroll container any more, so this guard is \
         asserting against an empty set: {scrolls:?}"
    );

    let lists = empty_lists_with_their_ancestry(&page);
    assert!(
        lists.len() >= 8,
        "found {} lists the Server fills; the session alone has VoiceRooms, Linhas, \
         the messages and the roster",
        lists.len()
    );

    let mut trapped: Vec<String> = Vec::new();
    for (id, ancestry) in lists {
        let inside_a_scroller = ancestry
            .iter()
            .flat_map(|classes| classes.split_whitespace())
            .any(|class| scrolls.contains(class));
        if !inside_a_scroller {
            trapped.push(format!("#{id} — inside: {}", ancestry.join(" / ")));
        }
    }

    assert!(
        trapped.is_empty(),
        "these lists are filled from the Server, so nothing caps how long they \
         get, and neither they nor anything enclosing them scrolls — past the \
         bottom of the window their rows stop existing:\n{}",
        trapped.join("\n")
    );
}

// ---------------------------------------------------------------------------
// Carrying the keyboard across a change of screen.
// ---------------------------------------------------------------------------

/// The screens, as (id, opening tag, markup up to the next screen).
///
/// Cut on `<section id="tela-` rather than on `</section>`, because the settings
/// screen nests four `<section>`s of its own and the first closing tag would end
/// that slice a third of the way in. `split` hands back exactly the text between
/// two screens, which is what "inside this screen" has to mean here.
fn screens_of(page: &str) -> Vec<(String, String, String)> {
    let page = without_comments(page);
    let mut found = Vec::new();
    for piece in page.split("<section id=\"tela-").skip(1) {
        let Some(name) = piece.split('"').next() else {
            continue;
        };
        let Some(tag) = piece.split('>').next() else {
            continue;
        };
        // The delimiter is put back so the tag reads as it does in the page —
        // an assertion that quotes half an opening tag sends the next reader
        // looking for a string that is not there.
        found.push((
            format!("tela-{name}"),
            format!("section id=\"tela-{tag}"),
            piece.to_owned(),
        ));
    }
    assert!(
        found.len() >= 4,
        "found {} screens; boot, sessao, auth and fim are all supposed to be in \
         the page — or they stopped being written `<section id=\"tela-…`",
        found.len()
    );
    found
}

/// A script cut into its top-level statements, by brace depth.
///
/// Coarser than "one function each", and that is deliberate: two of the nine
/// transitions in this frontend do not live in a named function at all — the
/// EJETAR of the end screen and the Escape handlers are arrow functions passed
/// straight to `addEventListener`. A cut that only knew about `function` would
/// have been blind to exactly the transitions nobody wrote a name for.
fn top_level_chunks(script: &str) -> Vec<String> {
    let script = without_comments(script);
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut depth: i32 = 0;
    for channel in script.lines() {
        current.push_str(channel);
        current.push('\n');
        depth += i32::try_from(channel.matches('{').count()).unwrap_or(0);
        depth -= i32::try_from(channel.matches('}').count()).unwrap_or(0);
        if depth > 0 {
            continue;
        }
        depth = 0;
        if !current.trim().is_empty() {
            chunks.push(std::mem::take(&mut current));
        }
        current.clear();
    }
    if !current.trim().is_empty() {
        chunks.push(current);
    }
    chunks
}

/// Whether this piece of script pulls a whole screen back into view.
///
/// `$("…")` with a screen id, or `$(…)` with a variable — `fecharServer` reveals
/// `$(volta)` and `abrirServer` hides `$(origem)`, and a check that only read
/// literals would miss the one transition that has two possible destinations.
/// A variable inside `$()` is only ever a screen here; everything else that
/// toggles `hidden` — the banner, the battery, the invite, an error channel —
/// holds its element in a variable and writes `erro.hidden`, never `$(erro)`.
fn reveals_a_screen(chunk: &str, screens: &BTreeSet<String>) -> bool {
    for piece in chunk.split("$(").skip(1) {
        let Some((argument, rest)) = piece.split_once(')') else {
            continue;
        };
        if !rest.trim_start().starts_with(".hidden = false") {
            continue;
        }
        let argument = argument.trim();
        match argument
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
        {
            Some(id) => {
                if screens.contains(id) {
                    return true;
                }
            }
            None => return true,
        }
    }
    false
}

#[test]
fn every_transition_that_puts_a_screen_on_hands_it_the_keyboard() {
    // Hiding the ancestor of the focused element drops focus on `<body>`. If
    // nothing then focuses anything on the screen that arrives, three things
    // break at once and none of them errors: somebody who opened the call from
    // the keyboard lands at the top of the document and has to tab back down to
    // the button they pressed; closing does not give that button back; and a
    // screen reader announces nothing at all, because from where it stands
    // nothing happened. WCAG 2.4.3.
    //
    // Before this, `.focus()` appeared exactly once in the whole frontend — the
    // `/` shortcut of the search — and every screen change was a bare `hidden`.
    let page = read("ui/index.html");
    let screens: BTreeSet<String> = screens_of(&page).into_iter().map(|(id, _, _)| id).collect();

    let mut deaf: Vec<String> = Vec::new();
    let mut transitions = 0;
    for name in ui_files(".js") {
        for chunk in top_level_chunks(&read(&format!("ui/{name}"))) {
            if !reveals_a_screen(&chunk, &screens) {
                continue;
            }
            transitions += 1;
            if chunk.contains("abrirTela(") || chunk.contains("voltarParaTela(") {
                continue;
            }
            let head = chunk.lines().next().unwrap_or_default().trim().to_owned();
            deaf.push(format!("ui/{name}: {head}"));
        }
    }

    // A count, because the way this guard goes quiet is not somebody deleting a
    // `abrirTela` — it is somebody writing a transition the cut above cannot
    // see, and then nothing is reported because nothing was read.
    assert!(
        transitions >= 8,
        "only {transitions} places in the frontend reveal a screen; there are \
         nine — into auth, into the session, into and out of the call, into and \
         out of the settings, into the end screen, and the two ways back to the \
         entrance"
    );

    assert!(
        deaf.is_empty(),
        "these transitions reveal a screen and leave the focus on <body>, so the \
         keyboard lands at the top of the document and a screen reader is told \
         nothing:\n{}",
        deaf.join("\n")
    );
}

#[test]
fn every_screen_says_where_the_focus_lands_and_what_to_announce() {
    let page = read("ui/index.html");

    // The live region first: it is the half of this that is not about the
    // keyboard, and it has to be *outside* every screen. Inside one, it would be
    // hidden away at the exact moment it had something to say — a `hidden`
    // ancestor takes the whole subtree out of the accessibility tree.
    let anuncio = tag_with_id(&page, "anuncio");
    assert!(
        anuncio.contains("role=\"status\""),
        "the announcement region is not a live region any more, so a change of \
         screen is announced to nobody: <{anuncio}>"
    );
    for (id, _, markup) in screens_of(&page) {
        assert!(
            !markup.contains("id=\"anuncio\""),
            "the announcement region moved inside #{id}, where a `hidden` on the \
             screen takes it out of the accessibility tree along with everything \
             else — and a screen being hidden is exactly when it has to speak"
        );
    }

    // And it has to stay readable while being invisible. `display: none` and
    // `visibility: hidden` remove an element from the accessibility tree, which
    // is the one thing this element cannot afford.
    let base = without_comments(&read("ui/base.css"));
    let Some((_, rule)) = base.split_once(".anuncio {") else {
        panic!("base.css no longer styles `.anuncio`, so it is a visible paragraph")
    };
    let rule = rule.split('}').next().unwrap_or_default();
    for banned in ["display: none", "visibility: hidden"] {
        assert!(
            !rule.contains(banned),
            "`.anuncio` hides itself with `{banned}`, which takes it out of the \
             accessibility tree — it would be invisible *and* silent:\n{rule}"
        );
    }

    for (id, tag, markup) in screens_of(&page) {
        // Focusable at all. Without this the fallback in `abrirTela` — focus the
        // screen itself when it names no better control — silently does nothing,
        // and the focus stays on `<body>` exactly as before.
        assert!(
            tag.contains("tabindex=\"-1\""),
            "#{id} cannot receive focus, so a transition into it has nowhere to \
             put the keyboard: <{tag}>"
        );

        assert!(
            attribute(&tag, "data-anuncio").is_some_and(|frase| !frase.trim().is_empty()),
            "#{id} does not say what to announce when it opens, so somebody who \
             cannot see it is told nothing: <{tag}>"
        );

        // `data-foco` is optional — the session leaves it out on purpose, and the
        // markup says why. What it may not be is a name of nothing, or a name of
        // something on another screen: `focus()` on either does nothing and says
        // nothing, which is the failure this whole pair of guards is about.
        let Some(alvo) = attribute(&tag, "data-foco") else {
            continue;
        };
        assert!(
            markup.contains(&format!("id=\"{alvo}\"")),
            "#{id} sends the focus to `#{alvo}`, which is not inside it — \
             `focus()` on something hidden or absent fails silently, and the \
             keyboard would stay on <body>"
        );
    }
}

// ---------------------------------------------------------------------------
// The update button — ADR 0026.
// ---------------------------------------------------------------------------

/// The sentence `ui/frases.js` writes for one enum variant.
///
/// Comments stripped first, for the reason the rest of this file strips them:
/// the block above these entries explains *why* there are six sentences and not
/// one, and a check satisfied by that explanation is a check that cannot fail.
///
/// An entry ends at `",` followed by a channel break, which is what closes the last
/// piece of a sentence whether it is written on one channel or concatenated over
/// four — the `+` ending every intermediate piece is what keeps this from
/// stopping early.
fn sentence_for(variant: &str) -> String {
    let file = without_comments(&read("ui/frases.js"));
    let Some(after) = file.split(&format!("\n    {variant}:")).nth(1) else {
        panic!(
            "frases.js writes no sentence for `{variant}`, so that failure reaches \
             a screen that either says nothing or says `{variant}`"
        );
    };
    let Some(sentence) = after.split("\",\n").next() else {
        panic!("the sentence for `{variant}` is never closed");
    };
    sentence.trim().to_owned()
}

#[test]
fn every_way_the_update_can_fail_asks_something_different_of_the_reader() {
    // Six variants, and ADR 0026 is explicit that they are six because they ask
    // six different things of whoever is in front of the screen: `NaoConfigurado`
    // means *this executable will never update* and there is nothing to retry,
    // `AssinaturaRecusada` means a package arrived signed by somebody else and
    // retrying is precisely the wrong move, and `NaoAlcancei` means try again in
    // a minute. One sentence for all six would send everybody back to the same
    // button, including the two cases where pressing it again is wrong.
    //
    // So this asserts distinctness and not merely presence. A copy-pasted
    // sentence is the failure mode: it looks written, it reads fine, and it
    // quietly collapses two different situations into one instruction.
    let source = read("src/main.rs");
    let script = without_comments(&scripts());

    // Seven, and it was six until the first person pressed the button.
    //
    // ADR 0026 wrote six deliberately, and one of them — `NaoAlcancei` — was
    // carrying two situations that ask opposite things of the reader. A network
    // that failed is worth retrying; a releases page that answered «nothing is
    // published» is not, and telling somebody to check their connection over it
    // sends them hunting for a fault that does not exist. The seventh,
    // `NadaPublicado`, is that split. See the closing section of ADR 0026.
    //
    // The number is asserted rather than merely counted so that the next
    // variant also has to come with the sentence and the reason, instead of
    // arriving silently and sharing somebody else's wording.
    let variants = variants_of(&source, "FalhaAoAtualizar");
    assert_eq!(
        variants.len(),
        7,
        "`FalhaAoAtualizar` has {} variants; ADR 0026 accounts for seven: {variants:?}",
        variants.len()
    );

    let mut written: BTreeSet<String> = BTreeSet::new();
    for variant in &variants {
        assert!(
            names(&script, variant),
            "`FalhaAoAtualizar::{variant}` reaches the page with no sentence \
             written for it, so the screen either says nothing or says `{variant}`"
        );
        let sentence = sentence_for(variant);
        assert!(
            !sentence.is_empty(),
            "the sentence for `{variant}` is empty"
        );
        assert!(
            written.insert(sentence.clone()),
            "two ways the update can fail are written with the same sentence, so \
             the screen cannot tell them apart — and two of these six must not \
             send the reader back to the same button:\n{sentence}"
        );
    }
}

/// The markup of one panel of the settings screen, cut out by its id.
///
/// The panels nest no `<section>` of their own, so the first closing tag is
/// theirs. Comments stripped: the paragraph above this panel explains, at
/// length, exactly what the panel has to say — and a guard an explanation can
/// satisfy is a guard that cannot fail.
fn panel_markup(page: &str, id: &str) -> String {
    let page = without_comments(page);
    let Some(after) = page.split(&format!("id=\"{id}\"")).nth(1) else {
        panic!("index.html has no panel with id `{id}`");
    };
    let Some(body) = after.split("</section>").next() else {
        panic!("the panel `{id}` is never closed");
    };
    body.to_owned()
}

#[test]
fn the_update_screen_says_the_window_closes_and_that_a_hosted_server_falls_with_it() {
    // `instalar_atualizacao` closes and reopens SEELE on all three systems — on
    // Windows there is no choice, because the NSIS installer will not run with
    // the program open. An action that closes somebody's window has to say so
    // *before* it is pressed.
    //
    // The second half is the one that is easy to leave out, and it is the one
    // that costs other people: this app can host a server inside the very window
    // that is about to be replaced (`hospedar`), and everybody inside it drops
    // when it goes. Whoever presses the button knows they are closing their own
    // window; nothing but this sentence tells them whose else.
    let page = read("ui/index.html");
    let painel = panel_markup(&page, "painel-atualizacao");

    for (what, needle) in [
        (
            "that the window closes and comes back",
            "fecha e abre de novo",
        ),
        (
            "that a server hosted here falls too",
            "hospedando um servidor",
        ),
        (
            "that a failure leaves no half installation",
            "meia instalação",
        ),
    ] {
        assert!(
            painel.contains(needle),
            "the update panel never says {what}. Installing is the one action in \
             this app that takes the window down, and on the day it takes a room \
             full of people with it, this paragraph is the only warning there was."
        );
    }

    // And it has to be above the button, not beside the failure afterwards.
    let Some(warning) = painel.find("ANTES DE INSTALAR") else {
        panic!("the update panel no longer heads its warning");
    };
    let Some(button) = painel.find("id=\"atualizacao-instalar\"") else {
        panic!("the update panel has no install button");
    };
    assert!(
        warning < button,
        "the warning about closing the window is drawn after the button that \
         closes it, so it is read by whoever already pressed"
    );
}

#[test]
fn the_download_bar_is_left_unmeasured_when_the_package_carries_no_total() {
    // `Andamento::total` is an `Option` because the server may send no
    // `Content-Length`. A bar drawn against a denominator nobody sent is a bar
    // that stalls somewhere and lies about how much is left — the same defect
    // the battery bar exists to avoid, and it gets the same answer here: the
    // frame stays, the value is marked absent, and the count beside it carries
    // the truth.
    //
    // Scoped to the one function that draws it, because the file says all of
    // these words either way — the paragraph explaining why there is no bar
    // would satisfy an unscoped search for it.
    let body = body_of(&scripts(), "function desenharAndamentoDoDownload");

    let Some(guard) = body.find("!andamento.total") else {
        panic!(
            "nothing branches on whether the package announced a total, so the \
             bar is drawn against a denominator that may not exist:\n{body}"
        );
    };
    let Some(unmeasured) = body.find("naoMedido(") else {
        panic!("the missing total is not marked absent anywhere:\n{body}");
    };
    let Some(bar) = body.find("blocos(") else {
        panic!("nothing draws the bar at all:\n{body}");
    };

    assert!(
        guard < unmeasured && unmeasured < bar,
        "the block bar is drawn before the missing total is ruled out, so a \
         package with no announced size still gets a percentage:\n{body}"
    );
    assert!(
        body[unmeasured..bar].contains("return"),
        "the unmeasured branch falls through into the bar, so one frame says \
         both «not measured» and «47%»:\n{body}"
    );
    assert!(
        body.contains("${parte}%"),
        "the bar has no number beside it, and `specs/06-clientes-gui.md` forbids \
         information carried by shape alone — a wall of blocks is shape:\n{body}"
    );
}

#[test]
fn the_update_is_only_ever_asked_for_by_a_press() {
    // ADR 0026: "nenhuma consulta automática ao abrir". In a product whose whole
    // argument is that the server is yours, an app that talks to github.com on
    // every launch contradicts the argument — and what was asked for was a
    // *button*.
    //
    // The tempting change is small and invisible: one call from the half-second
    // tick this screen already runs, or one from `abrirServer`, and the app
    // quietly phones home whenever anybody opens the settings. So the check is
    // that the search is reachable from exactly two places — where it is
    // declared, and where a click is bound to it — and from nowhere else.
    let file = read("ui/tela-server.js");

    let mut places = Vec::new();
    for chunk in top_level_chunks(&file) {
        if !chunk.contains("procurarAtualizacao") && !chunk.contains("procurar_atualizacao") {
            continue;
        }
        let head = chunk.lines().next().unwrap_or_default().trim().to_owned();
        places.push(head.clone());
        assert!(
            head.starts_with("async function procurarAtualizacao")
                || chunk.contains("addEventListener(\"click\""),
            "something other than a press reaches the update check: {head}"
        );
    }
    assert_eq!(
        places.len(),
        2,
        "the update check is reachable from {} places; there are two — the \
         declaration and the click: {places:?}",
        places.len()
    );

    // And the tick that keeps the input meter alive must not have grown a second
    // job. It runs twice a second for as long as this screen is open.
    let tick = body_of(&scripts(), "async function atualizarServer");
    assert!(
        !tick.contains("procurar"),
        "the half-second loop of the settings screen asks whether there is a new \
         version, which is an automatic check wearing a meter's clothes:\n{tick}"
    );
}

// ---------------------------------------------------------------------------
// Moderation — the four verbs of `specs/04-servidor-seele.md`.
// ---------------------------------------------------------------------------

/// The four commands that act on a person or on what they said.
const VERBOS_DE_MODERACAO: &[&str] = &[
    "expulsar_pessoa",
    "banir_pessoa",
    "remover_mensagem",
    "mover_pessoa",
    // Destroying a room goes through the same machine, and belongs on the same
    // list. It is the most consequential of the six — a kick lasts a session, a
    // ban is undone by whoever holds the server's file, and this ends what other
    // people wrote with nothing anywhere that brings it back.
    "apagar_voice_room",
    "apagar_linha",
];

#[test]
fn no_moderation_act_reaches_the_server_without_a_sentence_that_says_what_it_costs() {
    // The rule this whole layer exists for. Kicking and banning are
    // irreversible **for the person on the receiving end**, removing a message
    // takes it away from everybody, and moving somebody takes them out of where
    // they were without their asking. None of the four can be one press away.
    //
    // And a second press is not the answer either: «tem certeza?» adds no
    // information to somebody who already decided once, so it trains people to
    // press twice. What a confirmation is *for* is saying what will happen, with
    // the name of whoever it happens to inside the sentence — which is why
    // `armarAto` takes the sentence as an argument rather than a flag.
    //
    // So every one of the four has to reach the bridge from inside a chunk that
    // arms an act, and there must be no other path. Every chunk that names the
    // command is checked, not just the first — a second, unguarded call is
    // exactly the shape this would otherwise miss.
    let layer = read("ui/camada-moderar.js");
    let script = without_comments(&scripts());

    for verbo in VERBOS_DE_MODERACAO {
        let needle = format!("invoke(\"{verbo}\"");
        assert!(
            script.contains(&needle),
            "nothing calls `{verbo}`, so the verb is registered and unreachable"
        );

        let mut armed = 0;
        for chunk in top_level_chunks(&layer) {
            if !chunk.contains(&needle) {
                continue;
            }
            armed += 1;
            assert!(
                chunk.contains("armarAto(") || chunk.contains("abrirConfirmacao("),
                "`{verbo}` is sent without arming a confirmation first, so an \
                 irreversible act is one press away:\n{chunk}"
            );
        }
        assert!(
            armed > 0,
            "`{verbo}` is called from outside ui/camada-moderar.js, where the \
             confirmation lives — so whatever calls it is not going through one"
        );
    }

    // The other end of the same wire: the box's confirm button is the only thing
    // that runs an armed act, and it runs the one that was armed.
    let Some(confirmar) = script
        .split("$(\"moderar-confirmar\").addEventListener")
        .nth(1)
        .and_then(|rest| rest.split("\n});").next())
    else {
        panic!("nothing is listening on `moderar-confirmar` at all");
    };
    assert!(
        confirmar.contains("atoArmado") && confirmar.contains("executar()"),
        "the confirm button does not run the act that was armed, so the sentence \
         the reader agreed to and the command that goes out are two different \
         things:{confirmar}"
    );

    // And the message path goes through the same door.
    let mensagem = body_of(&scripts(), "function abrirConfirmacao");
    assert!(
        mensagem.contains("armarAto("),
        "`abrirConfirmacao` opens the box without arming anything, so a second \
         confirmation shape grew beside the first:\n{mensagem}"
    );

    assert!(
        !script.to_lowercase().contains("tem certeza"),
        "something in the page asks «tem certeza?», which is the confirmation \
         that adds nothing: it does not say what happens, so it teaches people \
         to press twice"
    );
}

#[test]
fn the_ban_says_that_nothing_in_this_product_undoes_it() {
    // The sharpest edge of the four, and the one a screen can hide by accident.
    // There is no `unban` verb in this protocol: a permanent ban is undone only
    // by somebody with the server's own file, by hand, on the machine hosting it.
    // A confirmation that says «bar this person?» and stops there is describing a
    // reversible act, and this one is not.
    //
    // The timed ban is in the same sentence for the same reason, from the other
    // side: it *is* the one that undoes itself, and that is exactly the fact
    // that makes it worth offering. Whoever is about to press has to be told
    // which of the two is about to happen.
    let body = body_of(&scripts(), "function consequenciaDeBanir");

    assert!(
        body.contains("ate === null") || body.contains("ate == null"),
        "the ban sentence does not branch on whether there is an expiry, so a \
         ban that lifts itself and a ban that never does read the same:\n{body}"
    );
    for (what, needle) in [
        ("that it is forever", "para sempre"),
        (
            "that no screen here undoes it",
            "Nenhuma tela deste produto desfaz",
        ),
        ("who can undo it, and how", "arquivo do servidor"),
        ("when a timed one lifts", "cai sozinha"),
    ] {
        assert!(
            body.contains(needle),
            "the ban confirmation never says {what}. Without it the sentence \
             describes a reversible act, and this is the one act in this product \
             that no screen of it can take back:\n{body}"
        );
    }

    // The name of the person, in the sentence. A consequence written about
    // nobody is a consequence about the wrong person just as easily.
    assert!(
        body.contains("quem.nome"),
        "the ban confirmation does not name who it is about:\n{body}"
    );
}

#[test]
fn moving_somebody_says_they_did_not_ask_and_that_both_rooms_watch_it_happen() {
    // The act that is easiest to underrate of the four, because nobody is
    // disconnected by it. It takes a person out of where they were without their
    // asking, they get a notice about it, and the two rooms watch them leave and
    // arrive. All three are things the person pressing does not experience, and
    // all three are the reason it deserves the same confirmation as the others.
    let body = body_of(&scripts(), "function consequenciaDeMover");

    for (what, needle) in [
        ("that the person did not ask for it", "sem ter pedido"),
        ("that they are told", "aviso"),
        ("which room they are taken from", "quem.voice_room"),
        ("which room they land in", "destino"),
    ] {
        assert!(
            body.contains(needle),
            "the move confirmation never says {what}:\n{body}"
        );
    }
    assert!(
        body.contains("continua na sessão"),
        "the move confirmation does not say the person stays connected, so it \
         reads like a kick with extra steps:\n{body}"
    );
}

#[test]
fn each_moderation_verb_is_offered_by_its_own_permission() {
    // `Snapshot` carries four booleans and not one, and the reason is on the
    // wire: `specs/04-servidor-seele.md` enumerates four permissions and a role
    // may hold any subset. A server can hand somebody `Kick` and nothing else.
    //
    // Gating the three on one boolean is the cheap version and it fails in both
    // directions at once — it offers `banir` to somebody who may only kick, and
    // hides `mover` from somebody who may only move. Neither shows up in a
    // build, and the first is only found by a refusal in front of a person.
    //
    // Scoped to the function that draws the box, because the file says all four
    // names either way: the paragraph explaining why there are four would
    // satisfy an unscoped search for them.
    let body = body_of(&scripts(), "function desenharModeracao");

    for (bloco, permissao) in [
        ("moderar-acao-expulsar", "may_kick"),
        ("moderar-acao-banir", "may_ban"),
        ("moderar-acao-mover", "may_move_person"),
    ] {
        assert!(
            body.split(';')
                .any(|statement| statement.contains(bloco) && statement.contains(permissao)),
            "`{bloco}` is not decided by `{permissao}`, so a role that carries \
             some of the four moderation permissions is offered the wrong ones:\n{body}"
        );
    }
    assert!(
        !body.contains("may_manage_voice_rooms"),
        "the moderation box is offered by the room-management permission, which \
         is a different permission for a different thing:\n{body}"
    );

    // The fourth lives on the message, and it is the one with an exception: your
    // own message needs no permission at all, because the permission in
    // `specs/04` reads «de outra pessoa».
    let mensagem = body_of(&scripts(), "function botaoDeRemoverMensagem");
    assert!(
        mensagem.contains("mensagem.own"),
        "removing a message consults only the permission, so somebody with no \
         moderation cannot take back what they themselves said:\n{mensagem}"
    );
    assert!(
        scripts().contains("may_remove_message"),
        "nothing reads `may_remove_message`, so the control is either shown to \
         everybody or to nobody"
    );

    // And never on yourself: kicking yourself is leaving the server, banning
    // yourself is not a thing, and moving yourself is ENTRAR NA SALA.
    //
    // The door used to be a `MODERAR` button drawn inside the voice-room list;
    // the 0.9.0 comp draws that list as names and nothing else, so the door
    // became the person's own name in the roster. The decision this guards —
    // who gets one — did not move with it.
    let porta = body_of(&scripts(), "function quemPodeSerModerado");
    assert!(
        porta.contains("pessoa.is_self"),
        "the moderation door is offered on one's own row too:\n{porta}"
    );
}

#[test]
fn the_moderation_is_a_layer_over_the_session_and_never_replaces_it() {
    // Same decision as the alert and the battery, and for the same written
    // reason: `specs/07-estetica.md` does not let this client replace the
    // conversation, and moderating is a decision taken while *looking* at the
    // room. A full screen would blank out who is talking at the exact moment
    // somebody is deciding what to do about a person.
    //
    // Structural, and not a screenshot: it has to live inside `#tela-sessao`.
    let page = read("ui/index.html");
    let Some(after) = page.split("id=\"tela-sessao\"").nth(1) else {
        panic!("index.html no longer has the session screen");
    };
    let Some(session) = after.split("<section ").next() else {
        panic!("the session screen is never closed by another section");
    };
    assert!(
        session.contains("id=\"moderar\""),
        "`moderar` is drawn outside `#tela-sessao`, so it is a screen and not a \
         layer — and a screen replaces the history that specs/07 says has to stay \
         readable"
    );

    let tag = tag_with_id(&page, "moderar");
    assert!(
        tag.contains("hidden"),
        "the moderation box is not hidden in the markup, so it is on the screen \
         from the first frame of every session: <{tag}>"
    );
    assert!(
        tag.contains("role=\"dialog\"") && tag.contains("aria-modal"),
        "the moderation box does not announce itself as a modal dialog, so a \
         screen reader reads it as more of the page it is covering: <{tag}>"
    );
}

#[test]
fn the_moderation_does_not_spend_the_red_reserved_for_alarm_and_collapse() {
    // `tokens.css:19` marks the red "EXCLUSIVO alerta e queda". Moderation is
    // the most serious thing this window offers and it is still not a dropped
    // link — and the red spent here is the red nobody reads on the day the
    // internal battery lights up. The accent is the institutional orange.
    //
    // Comments stripped, and that is load-bearing rather than tidy: the sheet
    // has to be able to write down *why* it is not red, and the paragraph saying
    // so names the token. A guard a comment can trip is as broken as a guard a
    // comment can satisfy.
    let sheet = without_comments(&read("ui/camada-moderar.css"));
    assert!(
        !sheet.contains("vermelho"),
        "the moderation layer paints with the token reserved for alarm and \
         collapse, which is the battery's colour and nothing else's"
    );

    // The one red it may show is the text of a failure, and that comes from
    // `.erro` in base.css — the same band of seriousness as a connection error,
    // and never the frame of the box.
    let page = without_comments(&read("ui/index.html"));
    assert!(
        page.contains("id=\"moderar-erro\" class=\"erro\""),
        "the moderation box writes its failures somewhere other than the shared \
         `.erro`, so it either invented a second red or says nothing when a \
         command is refused"
    );
}

#[test]
fn opening_the_moderation_carries_the_keyboard_and_closing_gives_it_back() {
    // A modal that appears without taking the focus leaves whoever navigates by
    // keyboard on the element behind it, pressing things that are no longer in
    // front — and a screen reader is told nothing at all, because from where it
    // stands nothing happened. WCAG 2.4.3, the same rule `abrirTela` exists for.
    //
    // Closing is the half that is easy to forget, and it has a trap of its own:
    // the button that opened the box is a row of `#lista-voice_rooms`, and that list is
    // thrown away and rebuilt twice a second — so by the time the box closes the
    // element may not be in the document any more. `focus()` on a node outside
    // the tree does nothing and reports nothing, which is the original defect
    // reintroduced from the inside.
    let abrir = body_of(&scripts(), "async function abrirModeracao");
    assert!(
        abrir.contains(".focus()"),
        "opening the moderation leaves the focus on whatever was behind it:\n{abrir}"
    );
    assert!(
        abrir.contains("anunciar("),
        "opening the moderation announces nothing, so somebody who cannot see it \
         is not told the box is there:\n{abrir}"
    );

    let fechar = body_of(&scripts(), "function fecharModeracao");
    assert!(
        fechar.contains("focoAntesDeModerar"),
        "closing the moderation does not give the keyboard back to whoever opened \
         it, so the focus lands on <body>:\n{fechar}"
    );
    assert!(
        fechar.contains("focavel("),
        "closing the moderation focuses the opener without checking it is still \
         in the page — and the row it lives on is rebuilt twice a second, so \
         `focus()` on it does nothing and says nothing:\n{fechar}"
    );

    // And a session that ends with the box open must not leave it armed for the
    // next one: the act inside it names somebody from a server already left.
    let fim = body_of(&scripts(), "function mostrarFim");
    assert!(
        fim.contains("abandonarModeracao"),
        "the end-of-session screen leaves the moderation box open, so it comes \
         back over the *next* session armed with an act on somebody from the \
         previous Server:\n{fim}"
    );
}

#[test]
fn every_failure_the_rust_side_can_name_has_a_sentence_here() {
    // `nome_da_falha` in `src/main.rs` turns each `ErroDeUri` variant into a
    // stable name, and `FRASES` in `ui/frases.js` turns that name into the
    // sentence somebody reads. The two lists are joined by nothing but
    // convention, and the seam is invisible: a new variant compiles, reaches the
    // screen, misses `FRASES`, and falls through to the `desconhecida()`
    // fallback — which prints a JSON blob of an English identifier.
    //
    // Not hypothetical. `EnderecoIpv6SemColchetes` arrived with step 2 of
    // ADR 0022 and did exactly that until this test existed.
    //
    // The fallback is still the right thing to have — it beats a dead end — but
    // it is a net, not a destination, and nothing was checking how often we
    // landed in it.
    let rust = read("src/main.rs");
    let Some(corpo) = rust.split("fn nome_da_falha").nth(1) else {
        panic!(
            "`nome_da_falha` is gone from src/main.rs, so the names the screen \
             keys off are now produced somewhere this test cannot see"
        );
    };
    let Some(corpo) = corpo.split("\n}").next() else {
        panic!("`nome_da_falha` is never closed");
    };

    // The names are the string literals on the right of each match arm.
    let nomes: Vec<&str> = corpo
        .split("=> \"")
        .skip(1)
        .filter_map(|resto| resto.split('"').next())
        .collect();
    assert!(
        nomes.len() >= 6,
        "found only {} failure names, so the match arms are no longer being read \
         correctly and this test has stopped guarding anything: {nomes:?}",
        nomes.len()
    );

    let frases = read("ui/frases.js");
    let sem_frase: Vec<&&str> = nomes
        .iter()
        .filter(|nome| !frases.contains(&format!("{nome}:")))
        .collect();
    assert!(
        sem_frase.is_empty(),
        "the Rust side can name these failures and `FRASES` has no sentence for \
         them, so each one reaches the person as a JSON blob of an English \
         identifier: {sem_frase:?}"
    );
}

#[test]
fn every_rung_of_the_reachability_ladder_has_a_sentence() {
    // `Anfitriao.alcance` crosses as one of four stable names from
    // `seele_server::alcance::Degrau`, and the sentence is here. Same seam as
    // `nome_da_falha` above, and the same failure if it drifts — except worse,
    // because this one is not an error: hosting succeeded, and the missing
    // sentence would be the one that says the link only works on the LAN.
    //
    // ADR 0022 asks for exactly this to be said out loud rather than left for
    // the person to discover as "it doesn't connect".
    let frases = read("ui/frases.js");
    // Scoped to `FRASES`, and that is load-bearing since the arrival paths
    // arrived: `CAMINHOS` writes `FuroDeNat` and `Ipv6Direto` too, about the
    // other side of the connection entirely, and a plain `contains` over the
    // file would let either ladder sentence be deleted while a path sentence
    // kept this green.
    let dicionario = without_comments(&frases);
    let escritas: BTreeSet<String> = sentences_of(&dicionario, "FRASES")
        .into_iter()
        .map(|(variante, _)| variante)
        .collect();
    // `FuroDeNat` and `EnderecoDireto` were missing from this list, each for the
    // same reason: the list is written by hand, and a rung added to `Degrau`
    // reaches the screen without anything here noticing. Deleting either
    // sentence from `frases.js` left this test green, which is the exact failure
    // it exists to prevent.
    for degrau in [
        "PortaNoRoteador",
        "FuroDeNat",
        "Ipv6Direto",
        "EnderecoDireto",
        "RedeLocalOuVpn",
        "SoRedeLocal",
    ] {
        assert!(
            escritas.contains(degrau),
            "the ladder can stop at `{degrau}` and no sentence says what that \
             means for the link the person is about to send"
        );
    }

    // The one that matters most is the bad one: it has to warn, and it has to
    // say what to do instead. A rung that only names itself leaves the host
    // exactly as stuck as no message at all.
    let Some(depois) = frases.split("SoRedeLocal:").nth(1) else {
        panic!("no sentence for SoRedeLocal");
    };
    let frase: String = depois.chars().take(400).collect();
    assert!(
        frase.to_lowercase().contains("rede"),
        "the LAN-only sentence never says the link is limited to the local \
         network:\n{frase}"
    );
    // The «way out» half was cut on 2026-08-20, by the product owner, across the
    // whole ladder. It used to be asserted here: the sentence had to name the
    // router or the VPN. The argument for it stands — a rung that only names
    // itself leaves the host as stuck as no message at all — and the trade was
    // made with that on the table: three paragraphs under a link is a warning
    // nobody reads, and `docs/alcance-pela-internet.md` already exists as the
    // long version precisely so the screen can be short.
    //
    // What is NOT allowed to come back is a pointer to that page on screen.
    assert!(
        !frase.contains("docs/") && !frase.contains(".md"),
        "the sentence sends the person to a documentation file, and a product \
         that answers «read the docs» on the screen where the link appears has \
         not answered:\n{frase}"
    );
}

#[test]
fn the_vpn_rung_names_the_vpn_that_is_why_the_link_stops_here() {
    // The rung that exists because of a field failure: a Windows host with
    // Cloudflare WARP had a global IPv6 — the tunnel's — and the ladder read it
    // as "reachable from anywhere", printed under a link that accepts no
    // inbound connection at all. The rung is only worth its own name if the
    // sentence says the thing the other three cannot: that a VPN is why, and
    // that turning it off is the way out.
    let frases = read("ui/frases.js");
    let Some(depois) = frases.split("RedeLocalOuVpn:").nth(1) else {
        panic!("no sentence for RedeLocalOuVpn");
    };
    // Bounded to this entry's own text, and comment-free: the block above it in
    // `frases.js` explains the field failure using every one of these words, so
    // an unscoped search would be satisfied by the justification instead of the
    // sentence.
    let frase: String = depois
        .split("SoRedeLocal:")
        .next()
        .unwrap_or_default()
        .lines()
        .filter(|linha| !linha.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        frase.to_lowercase().contains("vpn"),
        "the sentence never says a VPN is why the link reaches no further, which          is the only thing this rung knows that `SoRedeLocal` does not:\n{frase}"
    );
    // The fix half («desligue a VPN») was cut on 2026-08-20 with the rest of the
    // ladder's second channels. What this rung knows that `SoRedeLocal` does not —
    // that a VPN is why the link reaches no further — is asserted above and is
    // the reason the rung exists at all.
    assert!(
        !frase.to_lowercase().contains("alcança de qualquer lugar"),
        "the VPN rung repeats the promise that was the bug:\n{frase}"
    );
}

#[test]
fn the_ipv6_sentence_teaches_the_fix_with_an_address_that_would_work() {
    // This failure has a sentence of its own, rather than falling into
    // `EnderecoInvalido`, for one reason: it is fixable by the person reading
    // it. The address is fine; the punctuation is missing. So the sentence has
    // to actually show the bracketed form — naming a problem whose fix you
    // withhold is worse than the generic message it was split away from.
    //
    // And the example has to be an address that would work once bracketed. A
    // link-local (`fe80::`) example teaches the brackets and hands back an
    // address that still reaches nothing without a zone index, trading one dead
    // end for another.
    let frases = read("ui/frases.js");
    let Some(depois) = frases.split("EnderecoIpv6SemColchetes:").nth(1) else {
        panic!("no sentence for EnderecoIpv6SemColchetes");
    };
    let frase: String = depois.chars().take(400).collect();

    assert!(
        frase.contains('[') && frase.contains(']'),
        "the sentence never shows the bracketed form, so it names the problem and \
         withholds the fix:\n{frase}"
    );
    assert!(
        !frase.to_lowercase().contains("fe80"),
        "the example is a link-local address, which reaches nothing without a \
         zone index even once it is bracketed:\n{frase}"
    );
    assert!(
        !frase.contains('\u{2026}'),
        "a draft ellipsis leaked into a sentence somebody reads:\n{frase}"
    );
}

#[test]
fn the_host_is_told_how_far_the_link_they_are_about_to_send_reaches() {
    // The last unwired half of ADR 0022, and the half the whole ladder is for.
    // The Rust side climbs the rungs, the sentences exist in `FRASES`, and the
    // `alcance` field crosses the boundary — and none of that reaches anybody if
    // the page never draws it. A link that only works on the home network and a
    // link that works over the internet are the same text, so a host with no
    // sentence sends the first believing they sent the second, and the person
    // who finds out is their friend, on the other side, as "it won't connect".
    let page = read("ui/index.html");
    let script = without_comments(&scripts());

    let alcance = tag_with_id(&page, "convite-alcance");
    assert!(
        alcance.contains("hidden"),
        "`convite-alcance` is born visible, so it holds a reserved empty space \
         before `hospedar` has said which rung it stopped on: <{alcance}>"
    );

    // Scoped to the two functions that own this, because the page says all of
    // these words either way — the comment above the markup explaining why the
    // reach sits next to the link would satisfy an unscoped search for it.
    // On the comment-stripped source, like every other body read in this file:
    // `body_of` over the raw script would let a comment inside `hospedar`
    // mentioning `mostrarAlcance(` stand in for the call itself.
    let hospedar = body_of(&script, "async function hospedar");
    assert!(
        hospedar.contains("mostrarAlcance("),
        "`hospedar` shows the invite without ever saying how far it goes:\n{hospedar}"
    );
    assert!(
        hospedar.contains("alcance"),
        "`hospedar` never reads `alcance` off what the Rust side answered, so the \
         rung it climbed is thrown away at the boundary:\n{hospedar}"
    );

    let mostrar = body_of(&script, "function mostrarAlcance");
    assert!(
        mostrar.contains("fraseDeErro(") || mostrar.contains("FRASES"),
        "`mostrarAlcance` writes its own wording instead of reading the one in \
         `frases.js`, which is the file every other sentence lives in:\n{mostrar}"
    );
    assert!(
        mostrar.contains("SoRedeLocal"),
        "`mostrarAlcance` treats every rung alike, so the one that means `your \
         friends cannot reach this` looks exactly like the two that mean they \
         can:\n{mostrar}"
    );
    // And the rung that looks like good news and is not: a host on a browsing
    // VPN has an address that reads as global and accepts nobody. Drawing it
    // like the two rungs that do reach outside is the same lie the ladder used
    // to tell, moved one layer up.
    assert!(
        mostrar.contains("RedeLocalOuVpn"),
        "`mostrarAlcance` draws the VPN rung as if it reached the world, which \
         is exactly the promise this rung exists to stop making:\n{mostrar}"
    );

    // And the colour it may not spend. `tokens.css` reserves red for alarm and
    // collapse; a link that reaches only the home network is neither — it works,
    // and works less far than the host probably wanted.
    //
    // Two things this assertion got wrong on the way in, both worth keeping
    // written down because both produced a green test over a broken rule:
    //
    // - it read the sheet *with* comments, so the paragraph above the rule
    //   explaining why there is no red here was itself the match. The guard
    //   failed on its own justification. Same defect class `without_comments`
    //   exists for, one file over;
    // - it then read `split(…).nth(1)`, which is the text between the *first*
    //   and *second* occurrence of the selector — that is the base rule alone,
    //   and it stops exactly before `.convite-alcance-curto`, which is the one
    //   rule that actually carries a colour decision. Painting the short rung
    //   red passed. Every rule whose selector starts with the prefix has to be
    //   read, not the first one.
    let folha = without_comments(&styles());
    let mut regras = 0;
    for trecho in folha.split(".convite-alcance").skip(1) {
        let corpo = trecho.split('}').next().unwrap_or_default();
        regras += 1;
        assert!(
            !corpo.contains("vermelho"),
            "a `.convite-alcance…` rule spends the red that tokens.css reserves \
             for alarm and collapse:\n.convite-alcance{corpo}}}"
        );
    }
    assert!(
        regras >= 3,
        "found {regras} `.convite-alcance…` rules, and there are at least three: \
         the sentence itself and the two that tell the rungs apart. A count this \
         low means the selector was renamed and this whole check went quiet"
    );

    // Both directions of the same thing: the script must not invent a class the
    // sheet never styles, which renders as unstyled text and says nothing.
    let folha_bruta = styles();
    for classe in ["convite-alcance-curto", "convite-alcance-longe"] {
        assert!(
            !mostrar.contains(classe) || folha_bruta.contains(classe),
            "`mostrarAlcance` sets `{classe}`, which no stylesheet defines — so \
             the distinction it draws is invisible"
        );
    }
}

// ---------------------------------------------------------------------------
// Destroying a room — the confirmation that says the size of the damage.
// ---------------------------------------------------------------------------

/// O ouvinte que **apaga** um canal, achado pelo que ele mira e não pela ordem.
///
/// A lista de canais tem mais de um ouvinte, e este guarda pegava o primeiro:
/// `.split(...).nth(1)`. Isso valeu enquanto só houve um. Quando o lápis de
/// renomear entrou no arquivo — acima do apagar, que é onde ele fica na fileira
/// —, os dois guardas passaram a examinar o ouvinte errado e a acusar o de
/// apagar de não pesar o canal, apontando para um código que nunca pesou nada
/// porque não é ele que destrói.
///
/// Posição não é identidade. O que identifica este ouvinte é o atributo que ele
/// procura.
fn porta_de_apagar_o_canal(layer: &str) -> Option<String> {
    layer
        .split("$(\"lista-linhas\").addEventListener")
        .skip(1)
        .map(|resto| resto.split("\n});").next().unwrap_or_default().to_owned())
        .find(|corpo| corpo.contains("data-apagar-linha"))
}

#[test]
fn the_line_confirmation_counts_what_it_is_about_to_destroy() {
    // The requirement this whole path exists to satisfy, and the one a screen
    // can quietly fail: the box promises to destroy a specific number of
    // messages, written by a specific number of people, since a specific day.
    // All three have to be **counted**, in the server's database, at the moment
    // of asking.
    //
    // The tempting wrong version is right there and free: this window already
    // holds a page of history, so `mensagens.length` would compile, render, and
    // read as a real number. It would be low by whatever the Channel's whole past
    // is — and a number that is nearly right in a box promising to destroy 1.847
    // messages is worse than no number at all.
    //
    // So the sentence must be built out of the answer to `peso_da_linha` and
    // nothing else.
    let frase = body_of(&scripts(), "function consequenciaDeApagarLinha");

    for (what, needle) in [
        ("how many messages", "peso.messages"),
        ("how many people wrote them", "peso.authors"),
        ("since when", "peso.oldest_at_seconds"),
    ] {
        assert!(
            frase.contains(needle),
            "the Channel confirmation never says {what}, so it promises destruction \
             without saying how much:\n{frase}"
        );
    }
    assert!(
        !frase.contains("mensagens.length") && !frase.contains("messages.length"),
        "the Channel confirmation counts the page this window happens to be \
         holding, which is low by the whole of the Channel's past:\n{frase}"
    );

    // And the count reaches it from the server, through the one command that
    // waits for an answer. Scoped to the door that opens the box, because the
    // file explains the rule in prose as well and an unscoped search would be
    // satisfied by the paragraph.
    let layer = without_comments(&read("ui/camada-moderar.js"));
    let Some(porta) = porta_de_apagar_o_canal(&layer) else {
        panic!("nothing listens for a press on the Channel list in the moderation layer");
    };
    assert!(
        porta.contains("invoke(\"peso_da_linha\""),
        "the box about destroying o canal opens without asking the Server what is \
         in it, so its numbers came from somewhere this window guessed:\n{porta}"
    );

    // The order is the assertion: asked first, box second. A box opened before
    // the answer arrives is a box with a blank where the number goes.
    let pesa = porta
        .find("invoke(\"peso_da_linha\"")
        .expect("the weigh call was just asserted");
    let abre = porta
        .find("abrirConfirmacao(")
        .expect("the door opens no confirmation at all");
    assert!(
        pesa < abre,
        "the confirmation is opened before the count is asked for:\n{porta}"
    );
}

#[test]
fn a_count_that_did_not_arrive_stops_the_question_instead_of_rounding_it() {
    // The other half of "counted, never estimated", and the half a screen fails
    // by being helpful. When the server does not answer, there is no honest
    // version of this box: what is left is «apagar a Linha?», which is the
    // confirmation that adds nothing and teaches people to press twice.
    //
    // So the failure path must not reach `abrirConfirmacao`, and must not
    // invent a zero either — «isto destrói 0 mensagens» about o canal full of
    // them is the worst sentence this screen could produce.
    let layer = without_comments(&read("ui/camada-moderar.js"));
    let Some(porta) = porta_de_apagar_o_canal(&layer) else {
        panic!("nothing listens for a press on the Channel list in the moderation layer");
    };

    let Some(falhou) = porta
        .split("} catch (falha) {")
        .nth(1)
        .and_then(|resto| resto.split("\n  }").next())
    else {
        panic!(
            "the weigh call is not wrapped in a `catch`, so a Server that does not \
             answer leaves the press doing nothing at all:\n{porta}"
        );
    };
    assert!(
        !falhou.contains("abrirConfirmacao("),
        "o canal whose count never arrived is still offered for destruction, with \
         whatever number the sentence fell back to:\n{falhou}"
    );
    assert!(
        falhou.contains("abrirRecusa("),
        "the count failing says nothing to whoever pressed, so a press that was \
         refused looks exactly like one that was ignored:\n{falhou}"
    );

    // And the refusal arms nothing: no act, and no button that could run one.
    let recusa = body_of(&scripts(), "function abrirRecusa");
    assert!(
        recusa.contains("atoArmado = null"),
        "the box that refuses to ask leaves an act armed behind it:\n{recusa}"
    );
    assert!(
        recusa.contains("$(\"moderar-confirmar\").hidden = true"),
        "the box that refuses to ask still shows the button that confirms, which \
         is a button with nothing behind it in front of somebody who came here to \
         destroy something:\n{recusa}"
    );
    // Which means the confirm button has to come back for the boxes that do have
    // an act. `armarAto` is where every one of them passes.
    let armar = body_of(&scripts(), "function armarAto");
    assert!(
        armar.contains("$(\"moderar-confirmar\").hidden = false"),
        "nothing brings the confirm button back after a refusal hid it, so the \
         next act in this session is a sentence nobody can agree to:\n{armar}"
    );
}

#[test]
fn destroying_a_room_says_what_it_does_to_the_people_and_to_the_other_room() {
    // The two things the person pressing does not experience, and therefore the
    // two a confirmation has to say out loud.
    //
    // For a voice room: people are turned out of it in the middle of speaking, and
    // they are told. And the half that is easiest to get wrong from the other
    // direction — the Channel bound to it is **not** destroyed with it. Without
    // that channel, somebody who wanted a conversation gone destroys the voice room, sees
    // the Channel still there, and concludes the product did not do what it said.
    let voice_room = body_of(&scripts(), "function consequenciaDeApagarVoiceRoom");
    for (what, needle) in [
        ("how many people are inside", "voice_room.people.length"),
        (
            "that it happens mid-sentence",
            "no meio do que estiverem falando",
        ),
        ("that they are told", "aviso"),
        ("that the bound Channel survives", "não é apagado junto"),
        (
            "that nothing here brings it back",
            "Nenhuma tela deste produto",
        ),
    ] {
        assert!(
            voice_room.contains(needle),
            "the voice room confirmation never says {what}:\n{voice_room}"
        );
    }

    // For the channel: whoever is reading it loses it from the screen at that
    // instant, and any voice room bound to it comes out with no canal — a change
    // nobody asked for, which is exactly the kind this product names.
    let linha = body_of(&scripts(), "function consequenciaDeApagarLinha");
    for (what, needle) in [
        (
            "that no screen brings the writing back",
            "Nenhuma tela deste produto",
        ),
        ("what happens to whoever is reading it", "perde da tela"),
        ("which rooms come out without o canal", "sem canal"),
    ] {
        assert!(
            linha.contains(needle),
            "the Channel confirmation never says {what}:\n{linha}"
        );
    }

    // The empty Channel has a branch of its own: there is no "written since" when
    // nobody wrote, and a sentence that says "destroys 0 messages since
    // Invalid Date" is a sentence that was never read by its author.
    assert!(
        linha.contains("peso.messages === 0"),
        "the Channel confirmation has one sentence for o canal with a past and a \
         Channel with none, so the empty one reads as a date that does not \
         exist:\n{linha}"
    );
}

#[test]
fn the_last_voice_room_is_offered_disabled_with_the_reason_written_on_it() {
    // A server with na sala de voz has nowhere to speak. The server refuses the press,
    // and this window has to say why *before* it — a control that vanishes on
    // the last room teaches nothing, because an absence is not something anybody
    // reads. `moderar-acao-mover` hides instead, and the difference is real: it
    // hides when its **object** does not exist, and this room does.
    let porta = body_of(&scripts(), "function botaoDeApagarVoiceRoom");
    assert!(
        porta.contains("botao.disabled = ultimo"),
        "the last VoiceRoom is offered for destruction like any other, so the only \
         thing between a Server and having nowhere to speak is a refusal that \
         arrives after the press:\n{porta}"
    );
    assert!(
        porta.contains("botao.title = ultimo"),
        "the disabled control says nothing about why it is disabled, which is a \
         dead button and a shrug:\n{porta}"
    );
    assert!(
        porta.contains("única sala de voz"),
        "the reason the last VoiceRoom stays is not written anywhere a person \
         reads:\n{porta}"
    );

    // And the count comes from the server's list, not from anything this file
    // decides: one voice room left is one voice room in `snapshot.voice_rooms`.
    let desenho = body_of(&scripts(), "function desenharCanais");
    assert!(
        desenho.contains("snapshot.voice_rooms.length === 1"),
        "nothing tells the delete control which voice room is the last one:\n{desenho}"
    );
}

#[test]
fn destroying_a_room_is_offered_by_the_permission_that_destroys_it() {
    // The decision, asserted where a person meets it. Making a room and
    // renaming one are mistakes a server survives; destroying one ends what other
    // people wrote. `specs/04-servidor-seele.md` enumerates `gerenciar_voice_rooms`
    // and `administrar_server` separately, so a role that builds rooms without
    // being able to unmake them is a role somebody can actually write — and
    // gating both on one boolean makes it impossible to offer correctly.
    //
    // Scoped to the two functions that draw the controls, because the file
    // explains the distinction in prose too and an unscoped search for either
    // name would be satisfied by the paragraph that says why they differ.
    for porta in [
        "function botaoDeApagarVoiceRoom",
        "function botaoDeApagarLinha",
    ] {
        let corpo = body_of(&scripts(), porta);
        assert!(
            corpo.contains("may_delete_rooms"),
            "`{porta}` does not consult the permission that destroys rooms:\n{corpo}"
        );
        assert!(
            !corpo.contains("may_manage_voice_rooms"),
            "`{porta}` is offered by the permission to *make* rooms, which is a \
             different permission for a different thing:\n{corpo}"
        );
    }

    // And it is a permission of its own on the bridge, rather than a second
    // reading of one that was already there.
    let types = read("../../crates/seele-ffi/src/types.rs");
    assert!(
        types.contains("may_delete_rooms"),
        "the snapshot has no field for the permission that destroys rooms, so \
         whatever the screen is reading is somebody else's answer"
    );
}

#[test]
fn a_room_that_stopped_existing_has_a_sentence_and_not_a_shrug() {
    // Somebody is standing in the voice room, or reading the Channel, when it stops
    // existing. The connection is already out and the conversation is already off the
    // screen by the time this arrives — so without the sentence what is left is
    // a room that vanished on its own, which from where the reader sits is
    // indistinguishable from a window that lost track of where it was.
    //
    // The third is not about a room that went: it is the refusal, and the only
    // one of the three that has to teach what to do next.
    let frases = read("ui/frases.js");
    let Some(avisos) = frases
        .split("const AVISOS = {")
        .nth(1)
        .and_then(|resto| resto.split("\n};").next())
    else {
        panic!("`AVISOS` is gone from ui/frases.js");
    };
    let avisos = without_comments(avisos);

    for reason in ["VoiceRoomDeleted", "ChannelDeleted", "LastVoiceRoom"] {
        assert!(
            avisos.contains(&format!("{reason}:")),
            "the Server can raise `{reason}` and `AVISOS` has no sentence for it, \
             so it reaches the person as the word AVISO and nothing else"
        );
    }
    assert!(
        avisos.contains("Faça outra sala antes"),
        "the refusal of the last VoiceRoom does not say what to do about it, which \
         makes it a wall rather than an answer"
    );

    // Every one of the three has to be a reason the bridge can actually produce.
    let types = read("../../crates/seele-ffi/src/types.rs");
    for reason in ["VoiceRoomDeleted", "ChannelDeleted", "LastVoiceRoom"] {
        assert!(
            types.contains(reason),
            "`AVISOS` writes a sentence for `{reason}`, which `NoticeReason` \
             cannot be — a sentence nobody will ever read"
        );
    }
}

#[test]
fn o_palco_le_o_que_foi_pedido_do_snapshot_e_nao_da_memoria_da_janela() {
    // §5 da spec de compartilhamento de tela: a tela mostra o que está saindo
    // **ao lado** do que foi pedido, porque «escolher 1080p e receber 720p não é
    // defeito; esconder que aconteceu, é».
    //
    // A metade do que foi pedido morava numa variável desta janela — a caixa de
    // compartilhar guardava o que ela mesma tinha mandado. Uma recarga da janela
    // no meio de uma transmissão apagava essa metade enquanto a tela continuava
    // saindo, e o palco escrevia travessão sobre uma transmissão que estava
    // acontecendo. Agora ela atravessa em `Snapshot::tela.pedido`, guardada do
    // lado que sobrevive à janela.
    let numeros = body_of(&scripts(), "function desenharNumerosDoPalco");
    assert!(
        numeros.contains("tela.pedido"),
        "o palco não lê o que foi pedido do `Snapshot`, então a comparação que o \
         §5 obriga depende de esta janela ter estado aberta na hora da escolha:\n{numeros}"
    );
    assert!(
        !scripts().contains("limitesPedidos"),
        "a casca voltou a guardar em JavaScript o que ela mandou, e essa cópia \
         morre com a janela"
    );

    // E o campo existe do outro lado da ponte. Sem esta metade o `?? null` do
    // palco leria `undefined` para sempre e a coluna do pedido ficaria vazia
    // sem nada falhar.
    let types = read("../../crates/seele-ffi/src/types.rs");
    assert!(
        types.contains("pub pedido: Option<LimitesDeTela>"),
        "`TelaEmCurso` não carrega o que foi pedido, e o palco lê um campo que \
         não existe"
    );
}

#[test]
fn a_tela_parada_por_falta_de_subida_do_anfitriao_nao_acusa_quem_le() {
    // O servidor para a transmissão quando a sala cresce além da subida de quem
    // hospeda (§5.1: o teto é o caminho do anfitrião ÷ quem assiste). A razão
    // mais parecida que já existia é `SyncDegraded`, que esta casca escreve como
    // «SINAL EM QUEDA» — uma frase sobre a conexão de quem lê, na frente de
    // alguém cuja conexão está boa e cuja plateia cresceu. Sem frase própria,
    // quem foi parado sai procurando um defeito que não é dele.
    let frases = read("ui/frases.js");
    let Some(avisos) = frases
        .split("const AVISOS = {")
        .nth(1)
        .and_then(|resto| resto.split("\n};").next())
    else {
        panic!("`AVISOS` is gone from ui/frases.js");
    };
    let avisos = without_comments(avisos);

    let Some(frase) = avisos
        .split("ScreenShareOverHostUplink:")
        .nth(1)
        .and_then(|resto| resto.split(",\n").next())
    else {
        panic!(
            "o servidor pode parar uma transmissão por falta de subida do anfitrião \
             e `AVISOS` não tem frase para isso, então ela chega como a palavra \
             AVISO e mais nada"
        );
    };
    assert!(
        !frase.contains("SINAL"),
        "a frase da tela parada pelo anfitrião acusa o sinal de quem lê, que é a \
         confusão pela qual esta razão existe:{frase}"
    );

    let types = read("../../crates/seele-ffi/src/types.rs");
    assert!(
        types.contains("ScreenShareOverHostUplink"),
        "`AVISOS` escreve uma frase para uma razão que `NoticeReason` não pode \
         ser — uma frase que ninguém vai ler"
    );
}

#[test]
fn the_nat_punching_rung_names_its_cost_where_the_cost_is_paid() {
    // Degrau 4 do ADR 0022, added the day the rung was built. Two things are
    // being asserted, and both are product decisions rather than wording.
    //
    // First: it must not promise. Symmetric NAT on both ends does not punch,
    // and the ADR keeps relaying out of scope by decision — so the sentence has
    // to say what to do when it fails, exactly like the LAN-only one does.
    //
    // Second, and the one that would be quietest if it drifted: this rung is
    // the only one in the ladder with a third party in it. A product that sells
    // itself as «sem serviço no meio» just gained a service in the middle, and
    // the difference between saying so on the screen where the link appears and
    // letting somebody find out later is the difference between honesty and
    // advertising.
    // Read out of `FRASES` and not off the whole file. `CAMINHOS.FuroDeNat`
    // exists now — the path this connection actually took, which is a fact
    // about the other side of the wire — and it is a name rather than a
    // sentence. Splitting the file on `FuroDeNat:` finds that one first, and
    // every assertion below would then be checking a two-word metric for a
    // disclosure it was never meant to carry.
    let file = without_comments(&read("ui/frases.js"));
    let Some((_, frase)) = sentences_of(&file, "FRASES")
        .into_iter()
        .find(|(variante, _)| variante == "FuroDeNat")
    else {
        panic!("the ladder can stop at `FuroDeNat` and no sentence says what that means");
    };
    let baixa = frase.to_lowercase();

    // Two of the three assertions were cut on 2026-08-20, by the product owner,
    // when the ladder lost its second channels: «deve funcionar» (do not promise)
    // and «roteador» (name the way out). The third is not wording and did not
    // go — see below.
    //
    // What replaced them is narrower and still load-bearing: the sentence must
    // not send the person to a documentation file. It used to carry
    // `(docs/ponto-de-encontro.md)`, and that pointer went out with the rest.
    assert!(
        !frase.contains("docs/") && !frase.contains(".md"),
        "the sentence sends the person to a documentation file to learn what the \
         meeting point knows about them, which is the disclosure being made \
         somewhere other than where the cost is paid:\n{frase}"
    );
    assert!(
        baixa.contains("ponto de encontro") && baixa.contains("nunca o que foi dito"),
        "the sentence does not say what the meeting point learns. ADR 0022 accepts \
         this rung only if the metadata is said out loud rather than discovered \
         later:\n{frase}"
    );

    // «Opcional e trocável» is one of the four mitigations ADR 0022 accepted
    // this rung on, and a mitigation the person cannot see is a sentence in a
    // document rather than a property of the product. This used to be asserted
    // as `frase.contains("docs/ponto-de-encontro.md")` — the pointer WAS the
    // expression. The pointer went out on 2026-08-20 with every other
    // documentation reference on screen, and four words took its place.
    //
    // So the assertion moved rather than went: it no longer cares where the
    // person reads the detail, and it still cares that the screen says the
    // choice exists.
    assert!(
        baixa.contains("apontar para outro"),
        "the sentence never says the meeting point can be swapped, and «opcional \
         e trocável» stops being something the person paying the metadata can \
         act on:\n{frase}"
    );
}

// ---------------------------------------------------------------- anexos
//
// ADR 0027. Every check below reads the source **with the comments stripped**
// and scoped to one function's body, and that is not fussiness: this file has
// been fooled seven times in one day by an assertion satisfied by prose — a
// sentence in a doc comment, or a string in a `console.warn` sitting beside the
// `invoke` that had been deleted. A guard that a comment can satisfy guards a
// comment.

/// One JavaScript function's body, comments removed.
///
/// Braces are counted rather than split on, because every one of these bodies
/// contains nested blocks and object literals. Reading to the first `\n}` would
/// stop at the first `if`.
/// O corpo de um `Object.freeze({...})` ou `Object.freeze(new Set([...]))`.
///
/// Lê a tabela e não a linha: um guarda que procurasse o nome no arquivo
/// inteiro passaria com o nome escrito num comentário, que é o jeito mais
/// barato de um guarda deixar de medir.
fn bloco_congelado(source: &str, abertura: &str) -> String {
    let Some(at) = source.find(abertura) else {
        panic!("`{abertura}` is gone from the file that had it");
    };
    let after = &source[at + abertura.len()..];
    let mut depth = 1_i32;
    for (index, character) in after.char_indices() {
        match character {
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => {
                depth -= 1;
                if depth == 0 {
                    return after[..index].to_owned();
                }
            }
            _ => {}
        }
    }
    panic!("`{abertura}` never closes");
}

/// O corpo de uma `const` de objeto, pelo nome dela.
///
/// Igual a [`js_function`] e pelo mesmo motivo: uma tabela de tetos revisada
/// por `contains` no arquivo inteiro é uma asserção que o comentário ao lado
/// satisfaz. Recortando o objeto, o que ela lê é o objeto.
fn js_const(source: &str, name: &str) -> String {
    js_function(source, &format!("const {name} = Object.freeze("))
}

fn js_function(source: &str, signature: &str) -> String {
    let source = without_comments(source);
    let Some(at) = source.find(signature) else {
        panic!("`{signature}` is gone from the screen that had it");
    };
    let after = &source[at + signature.len()..];
    let Some(open) = after.find('{') else {
        panic!("`{signature}` has no body");
    };
    let mut depth = 0_i32;
    for (index, character) in after[open..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return after[open..=open + index].to_owned();
                }
            }
            _ => {}
        }
    }
    panic!("unterminated `{signature}`");
}

#[test]
fn a_file_goes_on_its_own_path_and_never_through_send_message() {
    // ADR 0027: the body travels **with** the file, on the transfer's own
    // stream, and the message is published only once the bytes have arrived
    // whole. A separate `send_message` would put the text on the Channel first and
    // let the picture turn up minutes later with nothing saying the two were
    // one thing.
    let corpo = js_function(&read("ui/tela-sessao.js"), "async function enviar(");
    assert!(
        corpo.contains("subirAnexo("),
        "sending no longer takes the attachment path at all: {corpo}"
    );
    let subir = js_function(&read("ui/tela-sessao.js"), "async function subirAnexo(");
    assert!(
        subir.contains("invoke(\"enviar_anexo\""),
        "`subirAnexo` no longer calls `enviar_anexo`, so nothing sends a file"
    );
    assert!(
        !subir.contains("send_message"),
        "the file path also posts the text separately, which publishes the \
         message before the file exists"
    );
}

#[test]
fn the_bar_measures_bytes_and_never_pretends() {
    // ADR 0026 fixed the shape and ADR 0027 inherits the easy half of it: here
    // the total is **always** known, because whoever chose the file knows how
    // big it is. So this path is always a bar with a percentage, and there is no
    // branch in it that falls back to a dash.
    let andou = js_function(&read("ui/tela-sessao.js"), "function transferenciaAndou(");
    assert!(
        andou.contains("anexo-barra") && andou.contains("transfer.done"),
        "the progress bar is no longer driven by bytes: {andou}"
    );
    assert!(
        andou.contains("100) / transfer.total") || andou.contains("* 100"),
        "the bar no longer computes a percentage from the real total"
    );

    // And the element really is a `<progress>` with a value, not a decorative
    // div somebody widens.
    let pagina = without_comments(&read("ui/index.html"));
    assert!(
        pagina.contains("<progress id=\"anexo-barra\""),
        "the upload bar stopped being a `<progress>`"
    );
}

#[test]
fn a_transfer_that_falls_says_that_trying_again_starts_from_zero() {
    // The sentence ADR 0027 requires and that nothing else in this product has
    // ever had to say: **there is no resumption.** A bar that simply returned to
    // the start would leave that for the person to work out, which is the
    // difference between a product that warns and one that surprises.
    let frases = without_comments(&read("ui/frases.js"));
    let Some(bloco) = frases
        .split("const TRANSFERENCIAS = {")
        .nth(1)
        .and_then(|resto| resto.split("\n};").next())
    else {
        panic!("`TRANSFERENCIAS` is gone from ui/frases.js");
    };
    assert!(
        bloco.contains("Fell:"),
        "a fallen transfer has no sentence, so it is a bar that stopped"
    );
    // Everything from `Fell:` to the next key. Not the first channel after it: the
    // sentence is several channels of concatenated string, and reading one channel
    // would let the half that matters live outside what is asserted about.
    let depois = bloco.split("Fell:").nth(1).unwrap_or_default();
    let caiu: String = depois
        .lines()
        .take_while(|channel| {
            let trimmed = channel.trim_start();
            !(channel.starts_with("  ")
                && trimmed.split(':').next().is_some_and(|word| {
                    !word.is_empty() && word.chars().all(char::is_alphanumeric)
                })
                && trimmed.contains(':'))
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        caiu.contains("do começo") || caiu.contains("inteiro outra vez"),
        "the sentence for a fallen transfer does not say that trying again \
         starts from zero: {caiu}"
    );

    // And the screen reaches for it on `Fell` and not on `Refused`: a refusal
    // has an explanation already travelling on the control stream, and a fall
    // has nothing coming ever.
    let andou = js_function(&read("ui/tela-sessao.js"), "function transferenciaAndou(");
    assert!(
        andou.contains("TRANSFERENCIAS.Fell"),
        "nothing on the screen writes the sentence for a fallen transfer"
    );
}

#[test]
fn every_refusal_the_server_can_send_has_a_sentence() {
    // A refusal reaching somebody as the word FALHA is a refusal that teaches
    // nothing. Read against the enum itself, so a variant added on the wire
    // without a sentence fails here rather than in front of a person.
    let control = read("../../crates/seele-proto/src/control.rs");
    let Some(enumeracao) = control
        .split("pub enum AttachmentRefusal {")
        .nth(1)
        .and_then(|resto| resto.split("\n}").next())
    else {
        panic!("`AttachmentRefusal` is gone from the protocol");
    };
    let variantes: Vec<String> = without_comments(enumeracao)
        .lines()
        .map(str::trim)
        .filter(|channel| channel.ends_with(',') || channel.ends_with('{'))
        .filter_map(|channel| {
            let name = channel.trim_end_matches([',', ' ', '{']);
            name.chars()
                .next()
                .filter(char::is_ascii_uppercase)
                .map(|_| name.to_owned())
        })
        .collect();
    assert!(
        variantes.len() >= 8,
        "the variant list came back too short to be the enum: {variantes:?}"
    );

    let frases = without_comments(&read("ui/frases.js"));
    let Some(bloco) = frases
        .split("const ANEXOS = {")
        .nth(1)
        .and_then(|resto| resto.split("\n};").next())
    else {
        panic!("`ANEXOS` is gone from ui/frases.js");
    };
    for variante in &variantes {
        assert!(
            bloco.contains(&format!("{variante}:")),
            "the Server can refuse with `{variante}` and `ANEXOS` has no \
             sentence for it"
        );
    }

    // The one that carries a number carries it into the sentence: "too big"
    // with no number sends somebody to try again with a file that is also too
    // big.
    let frase = js_function(&read("ui/frases.js"), "function fraseDeAnexo(");
    assert!(
        frase.contains("limit"),
        "the sentence for a file that is too large never mentions the limit"
    );
}

#[test]
fn no_screen_of_this_product_opens_a_file() {
    // The one point of ADR 0027 where it is possible to be strict, and it is
    // strict. Guarding for the absence of a thing, so it is read against the
    // whole frontend rather than one function: an "open" button added anywhere
    // is the failure.
    let mut fonte = String::new();
    for name in ui_files(".js") {
        fonte.push_str(&without_comments(&read(&format!("ui/{name}"))));
    }
    fonte.push_str(&without_comments(&read("ui/index.html")));

    for proibido in ["abrir_anexo", "openPath", "shell.open", "revealItemInDir"] {
        assert!(
            !fonte.contains(proibido),
            "the frontend reaches for `{proibido}`: no client of the SEELE opens \
             a file, and this is the only place in the whole design where being \
             strict is possible"
        );
    }

    // And the block a message draws offers exactly one verb.
    let bloco = js_function(&read("ui/tela-sessao.js"), "function blocoDeAnexo(");
    assert!(
        bloco.contains("anexo-salvar"),
        "an attachment offers no way to save it: {bloco}"
    );
    assert!(
        !bloco.to_lowercase().contains("abrir"),
        "the attachment block offers something other than saving: {bloco}"
    );
}

#[test]
fn saving_says_out_loud_what_this_product_does_not_promise() {
    // ADR 0027 has two things nobody may discover afterwards: **whoever hosts
    // the server could read this file**, and **the SEELE does not scan for
    // viruses**. Both are true, and both belong in front of the person before
    // they press, not on a help page.
    let salvar = js_function(&read("ui/tela-sessao.js"), "function salvarAnexo(");
    // `abrirConfirmacao(` and not `armarAto(`, and the difference is the whole
    // defect this channel used to hold in place. `armarAto` writes the sentence
    // into `#moderar` and focuses CANCELAR; it never *reveals* `#moderar`. So
    // this assertion passed, in green, while pressing SALVAR did nothing at all
    // — no box, no error, no console channel, because `focus()` inside a `hidden`
    // element is a silent no-op. Only the three doors of `camada-moderar.js`
    // open the box, and saving has to go through one of them.
    assert!(
        salvar.contains("abrirConfirmacao("),
        "saving arms an act without opening the box that would show it, so the \
         button is silent and the confirmation is never read: {salvar}"
    );
    assert!(
        salvar.contains("não varre vírus"),
        "the confirmation does not say that this product does not scan for \
         viruses, which is the thing it must not let anybody assume: {salvar}"
    );
    assert!(
        salvar.contains("quarentena"),
        "the confirmation does not mention the quarantine mark, which is the \
         one concrete guard this product can point at"
    );
    assert!(
        salvar.contains("chegou inteiro"),
        "the confirmation does not separate the question that has an answer \
         from the one that does not"
    );
    // And it says where the file lands. Choosing a file to send now opens a
    // system dialog; saving one that arrived still does not, so this side has
    // no dialog to read the destination off and has to write it out.
    assert!(
        salvar.contains("destino"),
        "the confirmation does not say where the file will be written"
    );
}

#[test]
fn there_is_no_blocklist_of_extensions_anywhere_in_the_frontend() {
    // ADR 0027 refuses one on purpose, and refusing it is a decision that has to
    // survive somebody adding one "just to be safe": a list is worked around
    // with a `rename`, it breaks sending a friend a build of this very project,
    // and — worse than both — it makes whatever got through look checked.
    let mut fonte = String::new();
    for name in ui_files(".js") {
        fonte.push_str(&without_comments(&read(&format!("ui/{name}"))));
    }
    fonte.push_str(&without_comments(&read("src/main.rs")));

    // **Uma lista, e não um nome.**
    //
    // Este guarda casava com qualquer ocorrência de `.exe"`, e em 05/09/2026 ele
    // reprovou por `join("desinstalar.exe")` — um caminho para **um** programa
    // desta casa, que não é uma lista de nada. Afrouxá-lo até «não cite `.exe`»
    // seria proibir o código de nomear os próprios arquivos.
    //
    // O que denuncia uma lista de proibidos é a forma: ou duas extensões
    // diferentes juntas, ou uma comparação de sufixo. Nomear um arquivo não é
    // nenhuma das duas.
    let suspeitos = [".exe\"", ".bat\"", ".scr\"", ".cmd\"", ".ps1\""];
    let quantos = suspeitos.iter().filter(|s| fonte.contains(**s)).count();
    assert!(
        quantos < 2,
        "a casca nomeia {quantos} extensões executáveis diferentes, que é como \
         uma lista de proibidos começa — e ADR 0027 explica por que uma é pior \
         que nenhuma"
    );

    for suspeito in [".exe", ".bat", ".scr", ".cmd", ".ps1"] {
        for forma in [
            format!("endsWith(\"{suspeito}\")"),
            format!("ends_with(\"{suspeito}\")"),
        ] {
            assert!(
                !fonte.contains(&forma),
                "a casca compara sufixo com `{suspeito}`, que é uma lista de \
                 proibidos com um item — e ADR 0027 recusa a forma, não o tamanho"
            );
        }
    }
}

#[test]
fn a_message_whose_file_expired_still_says_what_the_file_was() {
    // The whole reason the server keeps the attachment row after deleting the
    // bytes. Without this the message renders as a message with nothing in it,
    // and nobody learns there was ever a file.
    let bloco = js_function(&read("ui/tela-sessao.js"), "function blocoDeAnexo(");
    assert!(
        bloco.contains("EXPIROU"),
        "an expired attachment says nothing, so it is indistinguishable from a \
         message that never had one: {bloco}"
    );
    assert!(
        bloco.contains("anexo.file_name") && bloco.contains("anexo.byte_size"),
        "the name and the size are not drawn, so the sentence has nothing to be \
         about"
    );
    // The name and the size are drawn **before** the branch, so they survive it.
    let antes = bloco.split("anexo.expired").next().unwrap_or_default();
    assert!(
        antes.contains("anexo.file_name"),
        "the name is only drawn on the branch where the bytes are still there"
    );
}

#[test]
fn the_enumerated_reason_reaches_the_screen_and_is_not_only_carried() {
    // `ANEXOS` having a sentence for every variant is half of it. The half that
    // has been forgotten in this repository before is the other one: a
    // dictionary nobody looks up. `AttachmentRefusal` crossed the wire, reached
    // `Room`, reached the bridge — and if no screen called `fraseDeAnexo`, every
    // one of those sentences would be dead text.
    let sessao = read("ui/tela-sessao.js");
    let andou = js_function(&sessao, "function transferenciaAndou(");
    assert!(
        andou.contains("fraseDeAnexo("),
        "no screen turns the refusal into a sentence, so `ANEXOS` is a \
         dictionary nobody looks up: {andou}"
    );
    assert!(
        andou.contains("RefusedBecause") && andou.contains("Unavailable"),
        "the screen handles neither of the two shapes the reason arrives in"
    );

    // And the bridge really emits them, rather than collecting them in a queue
    // that nothing drains.
    let ponte = without_comments(&read("../../crates/seele-ffi/src/lib.rs"));
    assert!(
        ponte.contains("drain_transfers()"),
        "the bridge collects the reasons and never hands them on"
    );
    assert!(
        ponte.contains("Transfer::RefusedBecause") && ponte.contains("Transfer::Unavailable"),
        "the bridge has no way to carry an enumerated refusal to a shell"
    );
}

#[test]
fn the_bridge_maps_every_refusal_the_wire_can_carry() {
    // Mirrored rather than re-exported, so this is where the two lists can
    // drift. A variant added on the wire and forgotten here would compile —
    // `refusal_of` matches exhaustively, so it would not — but a variant added
    // to the bridge and never produced would be a sentence nobody can reach.
    let control = read("../../crates/seele-proto/src/control.rs");
    let Some(enumeracao) = control
        .split("pub enum AttachmentRefusal {")
        .nth(1)
        .and_then(|resto| resto.split("\n}").next())
    else {
        panic!("`AttachmentRefusal` is gone from the protocol");
    };
    let ponte = without_comments(&read("../../crates/seele-ffi/src/types.rs"));
    let Some(espelho) = ponte
        .split("pub enum AttachmentRefusal {")
        .nth(1)
        .and_then(|resto| resto.split("\n}").next())
    else {
        panic!("the bridge no longer mirrors `AttachmentRefusal`");
    };

    for channel in without_comments(enumeracao).lines() {
        let channel = channel.trim();
        let name = channel.trim_end_matches([',', ' ', '{']);
        if name.is_empty() || !name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
            continue;
        }
        assert!(
            espelho.contains(name),
            "the wire can refuse with `{name}` and the bridge has no variant \
             for it, so it would reach a shell as nothing"
        );
    }
}

#[test]
fn the_name_travels_as_it_was_and_the_type_is_carried_as_a_claim() {
    // ADR 0027: **não renomeia, não corta extensão.** Renaming an `.exe` to
    // look harmless makes the file lie, and lying is the last thing that helps
    // here. The declared type is registered as a claim and nothing downstream
    // decides what to decode from it alone.
    let main = without_comments(&read("src/main.rs"));
    let descrever = body_of(&main, "fn descrever_arquivo(caminho: String)");
    for suspeito in ["trim_end_matches", "replace(", "sanitiz", "strip_suffix"] {
        assert!(
            !descrever.contains(suspeito),
            "`descrever_arquivo` reaches for `{suspeito}`: the name a person \
             gave a file has to travel as it is"
        );
    }
    assert!(
        descrever.contains("file_name()"),
        "the name is no longer read off the path as it stands: {descrever}"
    );

    // And the type really is derived from the extension and used for nothing
    // but the record — the whole reason it is called a claim.
    let tipo = body_of(&main, "fn tipo_alegado(nome: &str) -> String");
    assert!(
        tipo.contains("application/octet-stream"),
        "an unknown extension no longer falls back to «bytes», which is the \
         only honest answer when nothing is known: {tipo}"
    );
}

/// Every element id a stretch of script writes to, in order.
fn ids_reached_for(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = source;
    while let Some(at) = rest.find("$(\"") {
        rest = &rest[at + 3..];
        let Some(end) = rest.find('"') else { break };
        out.push(rest[..end].to_owned());
        rest = &rest[end..];
    }
    out
}

/// Every element id that a stylesheet clips out of sight.
///
/// There is one such region and it is `#anuncio`: a one-pixel box with
/// `clip-path: inset(50%)`, which exists so a screen reader hears what changed.
/// Nothing written only there is visible to anybody looking at the screen —
/// which is the whole of the bug this pair of guards was written for.
///
/// Found by reading the stylesheets rather than by naming the id, because the
/// point is the *property*: a second hidden region added tomorrow has to be
/// caught by the same rule.
fn ids_clipped_off_screen() -> BTreeSet<String> {
    let mut classes = BTreeSet::new();
    for name in ui_files(".css") {
        let folha = without_comments(&read(&format!("ui/{name}")));
        for bloco in folha.split('}') {
            let Some((seletor, regras)) = bloco.split_once('{') else {
                continue;
            };
            if !regras.contains("clip-path: inset(50%)") {
                continue;
            }
            for parte in seletor.split([',', ' ', '>']) {
                if let Some(classe) = parte.trim().strip_prefix('.') {
                    classes.insert(classe.to_owned());
                }
            }
        }
    }
    assert!(
        !classes.is_empty(),
        "no stylesheet clips anything out of sight any more, so this guard is \
         measuring nothing — if the screen-reader-only region moved, follow it"
    );

    let pagina = without_comments(&read("ui/index.html"));
    let mut ids = BTreeSet::new();
    for tag in pagina.split('<') {
        let Some(tag) = tag.split('>').next() else {
            continue;
        };
        let Some(id) = atributo(tag, "id") else {
            continue;
        };
        let Some(class) = atributo(tag, "class") else {
            continue;
        };
        if class.split_whitespace().any(|c| classes.contains(c)) {
            ids.insert(id);
        }
    }
    ids
}

/// One attribute of one opening tag, if it has it.
fn atributo(tag: &str, nome: &str) -> Option<String> {
    let agulha = format!("{nome}=\"");
    let at = tag.find(&agulha)?;
    let rest = &tag[at + agulha.len()..];
    Some(rest[..rest.find('"')?].to_owned())
}

#[test]
fn the_attachment_button_opens_a_chooser() {
    // The decision this reverses is written down in ADR 0027 and was reverted by
    // the first person to use it: the button announced «arraste um arquivo» and
    // opened nothing, because dragging was the only way to choose a file. He
    // clicked it expecting a chooser, and dragging is not something anybody
    // discovers on their own.
    let sessao = read("ui/tela-sessao.js");
    let fonte = without_comments(&sessao);

    // What the click is wired to, read off the registration itself rather than
    // assumed.
    let Some(at) = fonte.find("$(\"botao-anexar\").addEventListener(\"click\"") else {
        panic!("nothing is listening for a press on the attachment button at all");
    };
    let registro = &fonte[at..at + fonte[at..].find(");").unwrap_or(0) + 1];

    assert!(
        registro.contains("abrirSeletorDeArquivo"),
        "the attachment button no longer reaches the chooser: {registro}"
    );

    // And the chooser is a real one: a command that opens the system dialog,
    // not a sentence describing how to drag.
    let abrir = js_function(&sessao, "async function abrirSeletorDeArquivo(");
    assert!(
        abrir.contains("invoke(\"escolher_arquivo\""),
        "the button opens nothing: whatever it calls does not ask the shell for \
         a file chooser: {abrir}"
    );
    assert!(
        abrir.contains("guardarAnexo("),
        "a file is chosen and never lands on the screen: {abrir}"
    );
}

#[test]
fn a_file_that_cannot_be_read_says_so_where_it_can_be_seen() {
    // The second half of the same report, and the subtler one. Both attachment
    // failures used to be reported through `anunciar(…)` alone — and `.anuncio`
    // is a one-pixel box clipped off the screen for screen readers. To the
    // person who was looking at the window, a refused file and nothing at all
    // are the same event.
    let sessao = read("ui/tela-sessao.js");
    let recusar = js_function(&sessao, "function recusarAnexo(");

    let escondidos = ids_clipped_off_screen();
    let escritos = ids_reached_for(&recusar);
    assert!(
        !escritos.is_empty(),
        "the refusal writes to no element at all, so it is invisible again: \
         {recusar}"
    );
    assert!(
        escritos.iter().any(|id| !escondidos.contains(id)),
        "every element the refusal writes to is clipped off the screen \
         ({escondidos:?}), so nothing changes for somebody looking at it"
    );
    // And the box it writes into is opened by the same function: a phrase
    // written into a `hidden` container is as invisible as the clipped one.
    assert!(
        recusar.contains(".hidden = false"),
        "the refusal writes into a box it never unhides: {recusar}"
    );

    // Both ways of choosing a file route their failure through it. The drag was
    // the one that had the bug; the chooser is new and would have grown the same
    // one by copying its neighbour.
    for entrada in [
        "async function escolherAnexo(",
        "async function abrirSeletorDeArquivo(",
    ] {
        let corpo = js_function(&sessao, entrada);
        assert!(
            corpo.contains("recusarAnexo("),
            "`{entrada}` reports its failure some other way, and the only other \
             way this file has is the clipped region: {corpo}"
        );
    }
}

#[test]
fn a_file_offered_with_no_line_open_is_refused_out_loud() {
    // Both doors give up when there is no canal to send to, and both used to give
    // up with a bare `return`. From the outside that is the same event as the
    // bug this whole file was written for: the person acts, and the window does
    // not move.
    let sessao = read("ui/tela-sessao.js");
    for entrada in [
        "listen(\"tauri://drag-drop\"",
        "async function abrirSeletorDeArquivo(",
    ] {
        let corpo = js_function(&sessao, entrada);
        let Some(depois) = corpo.split("linhaAberta === null").nth(1) else {
            panic!("`{entrada}` no longer checks whether o canal is open at all");
        };
        // Only as far as the end of that branch: a `recusarAnexo` further down,
        // on some other path, would say nothing about this one.
        let ramo = depois.split('}').next().unwrap_or_default();
        assert!(
            ramo.contains("recusarAnexo("),
            "`{entrada}` gives up in silence when no canal is open, which reads \
             exactly like a broken button: {ramo}"
        );
    }
}

#[test]
fn the_reason_a_rung_failed_is_not_prefixed_by_a_label_it_already_carries() {
    // From a real screen, on a real network: the detail channel under the invite
    // read «o roteador respondeu: o roteador respondeu, e o endereço dele
    // (100.65.128.5) não sai para a internet».
    //
    // The label was added here without reading the sentences it would sit in
    // front of. All four `FalhaAoAbrir` variants name the router, and every
    // `FalhaNoEncontro` names the rendezvous — they are written as whole
    // explanations, because that is what `specs` asks of a failure.
    //
    // So the rule is the absence: this function must not build the detail text
    // by concatenating anything in front of the reason it was handed.
    let mostrar = body_of(&without_comments(&scripts()), "function mostrarAlcance");

    for prefixo in ["o roteador respondeu:", "o ponto de encontro:"] {
        assert!(
            !mostrar.contains(prefixo),
            "`mostrarAlcance` glues `{prefixo}` in front of a sentence that already \
             says it, and the screen stutters:\n{mostrar}"
        );
    }

    // And the positive half, so the fix cannot be «delete the channel». The reason
    // still has to reach the page.
    assert!(
        mostrar.contains("portaRecusada") && mostrar.contains("encontroRecusado"),
        "one of the two reasons stopped being drawn at all:\n{mostrar}"
    );
    assert!(
        mostrar.contains("convite-alcance-detalhe"),
        "the reason is drawn without the class that makes it secondary:\n{mostrar}"
    );
}

#[test]
fn arriving_at_a_server_opens_a_line_and_does_not_put_anybody_in_a_voice_room() {
    // Both used to happen together on entry, with one good reason between them:
    // arriving at an empty screen is arriving without knowing what to do. The
    // reason still holds for one of the two and never held for the other.
    //
    // Reading text is passive — nobody hears you for having read something — so
    // opening the first Channel answers the empty screen and commits the person to
    // nothing.
    //
    // Entering a voice room is not passive. It takes one of fifteen seats, shows the
    // person as present, and puts a microphone at the disposal of a conversation
    // they did not pick. From the person who actually used it: «não dá para você
    // ficar fora de uma sala». They had never pressed anything.
    let entrar = body_of(&without_comments(&scripts()), "async function inserirPlug");

    assert!(
        entrar.contains("open_channel"),
        "arriving no longer opens o canal, so the screen is empty again and the \
         reason the automatic step existed is lost:\n{entrar}"
    );
    assert!(
        !entrar.contains("enter_voice_room"),
        "arriving puts the person inside a sala de voz without them pressing anything — \
         a seat taken and a microphone offered to a conversation nobody \
         chose:\n{entrar}"
    );
}

#[test]
fn the_button_that_stopped_inserting_the_plug_stopped_saying_it_does() {
    // The other half, and the one that rots quietly: behaviour moves, the label
    // stays. A button reading «INSERIR PLUG» that no longer inserts is worse
    // than an unnamed one, because this particular promise is about a microphone.
    let script = without_comments(&scripts());
    let segundo = body_of(&script, "async function verificarIdentidade");

    assert!(
        !segundo.contains("INSERIR PLUG"),
        "the second step still calls itself «INSERIR PLUG», and it no longer \
         inserts anything:\n{segundo}"
    );
    assert!(
        segundo.contains("botao.textContent"),
        "the second step stopped naming itself at all, so the button keeps \
         whatever the first step wrote:\n{segundo}"
    );
}
// ---------------------------------------------------------------------------
// The doorkeeper — ADR 0030.
// ---------------------------------------------------------------------------

/// The doorkeeper's acts, and which of them may not be one press away.
///
/// Three of the seven commands are here and four are not, and the split is the
/// rule rather than a sample. An act belongs on this list when it **opens** the
/// door or settles something about a person: refusing or admitting a knock
/// lasts until it is undone, revoking sends somebody back to being unknown,
/// and taking the password off or switching the doorkeeper off widens who may
/// come in.
///
/// Setting a password, minting an invite and switching the doorkeeper *on* are
/// deliberately absent. All three close the door, and demanding a confirmation
/// to close it is teaching people to press twice for the act that harms nobody
/// — the same reasoning that keeps «tem certeza?» out of this page.
const ATOS_DA_PORTARIA: &[&str] = &["decidir_pedido", "revogar_admissao"];

#[test]
fn no_act_of_the_doorkeeper_reaches_the_server_without_a_sentence_that_says_what_it_costs() {
    // The moderation rule, applied to the layer that decides who gets in. It is
    // a separate test rather than six more names on `VERBOS_DE_MODERACAO`
    // because that guard reads `ui/camada-moderar.js` and these live in their
    // own file — pointing it at two files would make it pass whenever either
    // one of them happened to arm something.
    //
    // Admitting is on the list beside refusing on purpose. An admission is the
    // whole promise of TOFU: it is never asked again. That makes it as durable
    // as the refusal, and a durable act with no sentence in front of it is the
    // shape this whole layer exists to prevent.
    let camada = read("ui/camada-portaria.js");
    let script = without_comments(&scripts());

    for verbo in ATOS_DA_PORTARIA {
        let needle = format!("invoke(\"{verbo}\"");
        assert!(
            script.contains(&needle),
            "nothing calls `{verbo}`, so the verb is registered and unreachable"
        );

        let mut armados = 0;
        for chunk in top_level_chunks(&camada) {
            if !chunk.contains(&needle) {
                continue;
            }
            armados += 1;
            let armado = chunk
                .find("armarAto(")
                .or_else(|| chunk.find("abrirConfirmacao("));
            let Some(armado) = armado else {
                panic!(
                    "`{verbo}` is sent without arming a confirmation first, so a \
                     decision about a person is one press away:\n{chunk}"
                );
            };

            // And the arming has to come *before* the call, which is what makes
            // this survive the trick it would otherwise fall for: a chunk that
            // sends the verb outright and mentions `abrirConfirmacao(` beside
            // it — in a `console.warn`, or in a second act further down —
            // satisfies a bare `contains` while confirming nothing. Comments are
            // already stripped by `top_level_chunks`; this is the half that
            // stripping cannot do.
            let Some(chamada) = chunk.find(&needle) else {
                unreachable!("the chunk was selected for containing it");
            };
            assert!(
                armado < chamada,
                "`{verbo}` is sent before anything arms a confirmation, so the \
                 sentence describes an act that already happened:\n{chunk}"
            );
        }
        assert!(
            armados > 0,
            "`{verbo}` is called from outside ui/camada-portaria.js, where the \
             confirmations are"
        );
    }

    // The two that open the door. They are not sent from a chunk that also arms
    // anything — the arming is in a named function the listener calls — so the
    // check is that the *function* which sends them is the one that arms.
    for (acao, funcao) in [
        ("senha: null", "function perguntarETirarSenha"),
        ("ligada: false", "function perguntarEDesligar"),
    ] {
        let corpo = body_of(&scripts(), funcao);
        assert!(
            corpo.contains("abrirConfirmacao("),
            "`{funcao}` widens who may come in without saying so first:\n{corpo}"
        );
        assert!(
            corpo.contains(acao),
            "`{funcao}` no longer sends `{acao}`, so the confirmation and the \
             act it describes have come apart:\n{corpo}"
        );
    }

    // And the ones that close it must NOT be behind a confirmation, or the page
    // is asking permission to be safer.
    for funcao in ["portaria-por-senha", "portaria-gerar"] {
        let script = without_comments(&scripts());
        let Some(depois) = script
            .split(&format!("$(\"{funcao}\").addEventListener"))
            .nth(1)
        else {
            panic!("`{funcao}` has no listener any more");
        };
        let chunk: String = depois.chars().take(600).collect();
        assert!(
            !chunk.contains("abrirConfirmacao("),
            "closing the door asks for confirmation, which trains people to \
             press twice for the act that harms nobody:\n{chunk}"
        );
    }
}

#[test]
fn a_knock_is_identified_by_its_fingerprint_and_never_by_the_name_it_claims() {
    // The question ADR 0030 turns on, and the one a card can get wrong while
    // looking fine. A nickname is text the person on the other side typed; the
    // fingerprint is the identity. If the card leads with the nickname, whoever
    // hosts approves a *name* — and `Rafae1` beside `Rafael` is a difference no
    // code catches and no eye catches either, unless the channel above it is the
    // one that carries the authority.
    //
    // Scoped to the function that builds a card and read without comments: the
    // paragraph above `cartao` explains exactly this, and a guard its own
    // rationale can satisfy is a guard that cannot fail.
    let corpo = body_of(&scripts(), "function cartao");

    // The order inside `append`, and not the order the two are *declared* in.
    //
    // This guard was written the wrong way round first and a mutation walked
    // straight through it: swapping the arguments of `append` — which is the
    // whole defect — leaves `const impressao = …` above `const apelido = …`
    // untouched, because declaring a variable is not putting it on a page. The
    // check has to read the channel that decides what the eye meets first.
    let Some(ordem) = corpo.split("linha.append(").nth(1) else {
        panic!("the card no longer appends its parts in one call:\n{corpo}");
    };
    let Some(ordem) = ordem.split(')').next() else {
        panic!("the append is never closed:\n{corpo}");
    };
    let Some(impressao) = ordem.find("impressao") else {
        panic!("the card no longer puts a fingerprint on the page:\n{ordem}");
    };
    let Some(apelido) = ordem.find("apelido") else {
        panic!("the card no longer puts the claimed name on the page:\n{ordem}");
    };
    assert!(
        impressao < apelido,
        "the card puts the claimed nickname above the fingerprint, so whoever \
         hosts is deciding about a name somebody typed:\n{ordem}"
    );

    // And the nickname is presented as a claim rather than as a fact.
    assert!(
        corpo.contains("diz chamar-se"),
        "the card states the nickname flatly, so it reads as identity rather \
         than as something the knocker asked to be called:\n{corpo}"
    );
    assert!(
        corpo.contains("«") && corpo.contains("»"),
        "the claimed nickname is not quoted, so nothing separates it from the \
         page's own words:\n{corpo}"
    );

    // The fingerprint reaches the eye in groups. Sixty-four unbroken characters
    // are not compared by a person: they lose their place halfway and conclude
    // it matches.
    let agrupar = body_of(&scripts(), "function agrupar");
    assert!(
        agrupar.contains("match("),
        "the fingerprint is handed over as one unbroken run, which is the shape \
         nobody actually checks:\n{agrupar}"
    );
}

#[test]
fn the_doorkeeper_spends_the_alarm_red_only_on_a_door_open_to_the_internet() {
    // Two guards in one, from both directions.
    //
    // Outwards: `tokens.css` marks the red "EXCLUSIVO alerta e queda", and the
    // moderation layer writes down that it does not spend it. This layer does,
    // once — a server with nothing closing it and an address reachable from
    // outside is the front door open to the street.
    //
    // Inwards, and this is the half that matters: it must not fire on a server
    // that is merely open on a home network. That is the ADR 0021 default,
    // defended there on purpose, and an alarm that goes off in the normal case
    // is an alarm people learn to dismiss — which is precisely what ADR 0003
    // says about the key-change warning it must never be confused with.
    let corpo = body_of(&scripts(), "function desenharAlarme");

    assert!(
        corpo.contains("estado.aberto") && corpo.contains("portaria_ligada"),
        "the alarm does not read whether anything is closing the door:\n{corpo}"
    );
    assert!(
        corpo.contains("ALCANCA_DE_FORA"),
        "the alarm fires without asking how far this Server is reachable, so it \
         goes off on the local-network default that ADR 0021 defends:\n{corpo}"
    );
    assert!(
        corpo.contains("!escancarada || !deFora") || corpo.contains("escancarada && deFora"),
        "the two conditions are not required together, so the alarm reports one \
         of them alone:\n{corpo}"
    );

    // `SoRedeLocal` is the rung that must never be in the list, and naming it
    // here is what keeps somebody from "fixing" the list by adding everything.
    let lista = body_of(&scripts(), "const ALCANCA_DE_FORA");
    assert!(
        !lista.contains("SoRedeLocal"),
        "the local-network rung counts as reachable from outside, so every \
         Server hosted on a laptop raises the alarm:\n{lista}"
    );

    // The red belongs to that band and to nothing else in the sheet.
    let folha = without_comments(&read("ui/camada-portaria.css"));
    let vermelhos = folha.matches("--seele-vermelho-alerta").count();
    assert_eq!(
        vermelhos, 1,
        "the doorkeeper spends the alarm red somewhere other than the band for \
         a door open to the internet, and that is the red nobody reads on the \
         day the internal battery lights up"
    );
}

#[test]
fn the_alarm_names_the_rungs_the_ladder_actually_reports() {
    // The alarm compares `estado.alcance` — which is `Degrau::nome()`, straight
    // off the Rust side — against a list written by hand in the layer. Nothing
    // joined the two, and they had drifted all the way apart: the list named
    // `EnderecoGlobal`, `PortaAberta` and `PontoDeEncontro`, and **no such name
    // exists** in `Degrau::nome()`. The comparison never matched, so the red
    // band saying this server is open and reachable from the internet never
    // appeared at all.
    //
    // That is worse than a missing sentence. A missing sentence is silence; an
    // alarm wired to names nobody produces is a guard everybody believes in and
    // nothing is behind. Read against the enum itself, so the next rung either
    // joins the list or fails here.
    let alcance = read("../../crates/seele-server/src/alcance.rs");

    let Some(corpo) = alcance
        .split("pub fn alcanca_de_fora")
        .nth(1)
        .and_then(|resto| resto.split("\n    }").next())
    else {
        panic!(
            "`alcanca_de_fora` is gone from the ladder, so the question this \
             alarm asks is now answered somewhere this test cannot see"
        );
    };
    let de_fora: Vec<String> = corpo
        .split("Self::")
        .skip(1)
        .filter_map(|resto| resto.split(|c: char| !c.is_alphanumeric()).next())
        .filter(|nome| !nome.is_empty())
        .map(str::to_owned)
        .collect();
    assert!(
        de_fora.len() >= 3,
        "found only {} rungs reachable from outside, so the `matches!` is no \
         longer being read correctly and this test guards nothing: {de_fora:?}",
        de_fora.len()
    );

    // Each one has to be a name `nome()` actually hands to the screen. Without
    // this, a variant renamed on one side of that `match` would still pass.
    let Some(nomes) = alcance
        .split("pub fn nome")
        .nth(1)
        .and_then(|resto| resto.split("\n    }").next())
    else {
        panic!("`Degrau::nome` is gone, so the names the screen keys off are gone with it");
    };
    for variante in &de_fora {
        assert!(
            nomes.contains(&format!("Self::{variante} => \"{variante}\"")),
            "`{variante}` reaches the person as some other string than its own \
             name, and the alarm compares against the name"
        );
    }

    let camada = without_comments(&read("ui/camada-portaria.js"));
    let Some(lista) = camada
        .split("const ALCANCA_DE_FORA")
        .nth(1)
        .and_then(|resto| resto.split(']').next())
    else {
        panic!("`ALCANCA_DE_FORA` is gone from ui/camada-portaria.js");
    };
    let anunciados: Vec<&str> = lista
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|nome| !nome.is_empty())
        .collect();

    let faltando: Vec<&String> = de_fora
        .iter()
        .filter(|nome| !anunciados.contains(&nome.as_str()))
        .collect();
    assert!(
        faltando.is_empty(),
        "the ladder reports these rungs as reachable from outside and the alarm \
         does not know them, so a Server open to the internet raises nothing: \
         {faltando:?}"
    );
    let inventados: Vec<&&str> = anunciados
        .iter()
        .filter(|nome| !de_fora.iter().any(|variante| variante == *nome))
        .collect();
    assert!(
        inventados.is_empty(),
        "the alarm compares against names the ladder never produces, which is \
         how it stayed silent for every rung at once: {inventados:?}"
    );
}

#[test]
fn every_reason_a_session_can_end_with_has_a_sentence_in_the_page() {
    // Written while adding two of them, and it found that nothing had been
    // guarding this at all: `EndReason` had twelve variants and `MOTIVOS`
    // twelve entries, and the two lists agreed only because one person kept
    // them in step by hand. A reason with no sentence reaches the end-of-session
    // screen and prints nothing, or prints the name of a Rust variant at
    // somebody who has just been disconnected and wants to know whether to try
    // again.
    //
    // Read from the bridge's own source rather than from a list here, so the
    // next variant fails this on the run it is added.
    let ponte = std::fs::read_to_string(app_dir().join("../../crates/seele-ffi/src/types.rs"))
        .expect("the bridge's types must be readable")
        .replace("\r\n", "\n");

    let script = without_comments(&scripts());
    let Some(motivos) = script.split("const MOTIVOS = {").nth(1) else {
        panic!("frases.js no longer declares MOTIVOS");
    };
    let Some(motivos) = motivos.split("\n};").next() else {
        panic!("MOTIVOS is never closed");
    };

    let variantes = variants_of(&ponte, "EndReason");
    assert!(
        variantes.len() >= 12,
        "`EndReason` came out with {} variants, so this guard is asserting \
         against almost nothing",
        variantes.len()
    );

    let mudas: Vec<&String> = variantes
        .iter()
        .filter(|variante| !motivos.contains(&format!("{variante}:")))
        .collect();
    assert!(
        mudas.is_empty(),
        "these ways a session can end reach the screen with no sentence written \
         for them, so it says nothing or says the name of a Rust variant: \
         {mudas:?}"
    );

    // The two the doorkeeper added are the ones worth naming here, because
    // folding them into each other — or into `CredentialRejected`, which is
    // where they landed before ADR 0030 — is the specific mistake this makes
    // impossible. They ask opposite things of whoever reads them.
    for (reason, needle) in [
        ("AdmissionPending", "AINDA NÃO DECIDIU"),
        ("AdmissionDenied", "RECUSOU"),
    ] {
        assert!(
            motivos.contains(&format!("{reason}:")),
            "`{reason}` has no sentence, so somebody who only had to wait is \
             sent away"
        );
        let Some(frase) = motivos.split(&format!("{reason}:")).nth(1) else {
            unreachable!("just asserted it is there");
        };
        let frase: String = frase.chars().take(400).collect();
        assert!(
            frase.contains(needle),
            "the sentence for `{reason}` does not say «{needle}», so it reads \
             like the other one:\n{frase}"
        );
    }
}

// ---------------------------------------------------------------------------
// What the owner asked for after installing the app and using it. Four guards,
// and three of them guard an **absence** — the hardest kind to keep, because
// nothing about a deleted thing coming back is visible in a diff that only adds
// channels.
// ---------------------------------------------------------------------------

#[test]
fn a_preview_is_asked_for_by_a_press_and_by_nothing_else() {
    // The attachment lives on the server, so looking at it is downloading it. A
    // Channel that previewed every picture as it scrolled would turn whoever
    // hosts' disk ceiling into everybody's uplink, once per person per time
    // anybody opened the Channel — and it would do it silently, because it would
    // look exactly like the feature working.
    let fonte = without_comments(&read("ui/tela-sessao.js"));
    let chamadas = fonte.matches("invoke(\"prever_anexo\"").count();
    assert_eq!(
        chamadas, 1,
        "`prever_anexo` is invoked from more than one place, and only one of \
         them can be the button"
    );

    let ver = js_function(&read("ui/tela-sessao.js"), "async function verPrevia(");
    assert!(
        ver.contains("invoke(\"prever_anexo\""),
        "the one call is not inside `verPrevia`, so it is somewhere a press \
         does not control: {ver}"
    );

    // And the only caller of `verPrevia` is a click handler. Redrawing the list
    // must not reach it: `desenharMensagens` runs on every snapshot tick.
    let desenhar = js_function(&read("ui/tela-sessao.js"), "function desenharMensagens(");
    assert!(
        !desenhar.contains("verPrevia"),
        "redrawing the conversation fetches previews, so scrolling downloads: \
         {desenhar}"
    );
    let bloco = js_function(&read("ui/tela-sessao.js"), "function blocoDeAnexo(");
    assert!(
        !bloco.contains("verPrevia") && !bloco.contains("invoke("),
        "drawing an attachment fetches its bytes, which is the whole thing this \
         must not do: {bloco}"
    );

    // A press, and a press is a click on a button. Not a hover, not focus.
    let chamadores: Vec<&str> = fonte
        .lines()
        .filter(|channel| channel.contains("verPrevia(") && !channel.contains("function verPrevia"))
        .collect();
    assert_eq!(
        chamadores.len(),
        1,
        "`verPrevia` is called from {} places: {chamadores:?}",
        chamadores.len()
    );
    // **Located by what it does, not by where it is.** This used to take the
    // first click listener on the conversation and read 900 characters from
    // it. The conversation grew a second one — the links in a message body —
    // and the guard started reading that one instead, failing on a file where
    // nothing it guards had changed. Matching by position is the same mistake
    // as a guard that matches its own comment: it agrees with the layout of
    // the file rather than with the property.
    // Each listener ends where the next `addEventListener` begins, capped at
    // 900 characters. A fixed window alone let neighbours bleed into one
    // another: two listeners closer than the window both appeared to handle
    // the same press, and the count meant nothing.
    const PADRAO: &str = "$(\"lista-mensagens\").addEventListener(\"click\"";
    let inicios: Vec<usize> = fonte.match_indices(PADRAO).map(|(at, _)| at).collect();
    let ouvintes: Vec<String> = inicios
        .iter()
        .map(|&at| {
            // A partir **depois** do `.addEventListener(` deste ouvinte: de
            // `at + 1` a busca acha o dele mesmo, vinte caracteres à frente, e
            // a janela fecha antes do corpo começar.
            let depois = at + PADRAO.len();
            let proximo = fonte[depois..]
                .find(".addEventListener(")
                .map_or(fonte.len(), |onde| depois + onde);
            // 900 **caracteres**, e não bytes: cortar por byte num arquivo com
            // acento encurta a janela sem avisar, e o guarda passa a não
            // enxergar o que estava lá.
            fonte[at..proximo].chars().take(900).collect()
        })
        .collect();
    assert!(
        !ouvintes.is_empty(),
        "the conversation no longer listens for a click at all"
    );
    let candidatos: Vec<&String> = ouvintes
        .iter()
        .filter(|um| um.contains("data-anexo-previa"))
        .collect();
    assert_eq!(
        candidatos.len(),
        1,
        "the attachment press is handled in {} click listeners on the \
         conversation; it has to be one, or two of them race for the same \
         press",
        candidatos.len()
    );
    let ouvinte = candidatos[0].clone();
    assert!(
        ouvinte.contains("verPrevia("),
        "the call to `verPrevia` is not inside the click handler on the \
         conversation, so a press is not what triggers it: {ouvinte}"
    );
    assert!(
        ouvinte.contains("data-anexo-previa"),
        "the click handler does not look for the preview button: {ouvinte}"
    );
}

#[test]
fn a_preview_already_fetched_is_redrawn_and_never_fetched_twice() {
    // The list is rebuilt whole on every update. Without somewhere to keep what
    // came back, every redraw would either lose the picture or pay for it
    // again — and paying again is somebody else's bandwidth.
    let bloco = js_function(&read("ui/tela-sessao.js"), "function blocoDeAnexo(");
    assert!(
        bloco.contains("previas.get("),
        "the attachment block does not read back a preview that was already \
         fetched, so redrawing loses it: {bloco}"
    );
    let ver = js_function(&read("ui/tela-sessao.js"), "async function verPrevia(");
    assert!(
        ver.contains("previas.set("),
        "nothing is kept after a fetch: {ver}"
    );
    // Refusals are kept too. Bytes that disagreed with a name do not disagree
    // less the second time, and asking again spends the host's uplink to reach
    // the same conclusion.
    let guardar = ver.split("previas.set(").nth(1).unwrap_or_default();
    let antes = ver.split("previas.set(").next().unwrap_or_default();
    assert!(
        !antes.contains("if (previa.image)") && !guardar.is_empty(),
        "the preview is only kept when it produced a picture, so a refusal is \
         re-fetched on every press: {ver}"
    );
}

#[test]
fn the_page_never_composes_the_media_type_a_picture_is_decoded_with() {
    // The crux of ADR 0027's rule. The `data:` URI arrives whole from Rust,
    // media type included, and that media type was written from what the bytes
    // turned out to be. A page that joined a type to some bytes would be a page
    // that could join the **sender's claim** to them, and the sender's claim is
    // text somebody else chose.
    let desenho = js_function(&read("ui/tela-sessao.js"), "function desenhoDaPrevia(");
    assert!(
        desenho.contains("previa.image"),
        "the drawing does not use the URI that came back: {desenho}"
    );
    assert!(
        !desenho.contains("data:"),
        "the page builds a `data:` URI of its own, which is the one way the \
         claim could reach a decoder: {desenho}"
    );
    assert!(
        !desenho.contains("declared_type"),
        "the drawing reads the type the sender declared: {desenho}"
    );
    assert!(
        !desenho.contains("base64"),
        "the page encodes or splices bytes itself: {desenho}"
    );

    // And nowhere in the frontend is a media type of one of the four written
    // out at all: the list a screen offers previews for comes from Rust, so
    // that two copies of it cannot drift into offering what the fetch refuses.
    //
    // ---- one exception, and why it is not a hole ----
    //
    // `tela-server.js` composes `data:image/png;base64,…` for the server's own
    // picture, and that is *not* the thing this test forbids. What it forbids is
    // a page joining a media type to bytes **whose type somebody else claimed**
    // — an attachment carries `declared_type`, chosen by the sender, and the
    // whole of ADR 0027 is that the claim must never reach a decoder.
    //
    // The server's picture carries no claim to disagree with. `SetServerIcon`
    // moves bytes and nothing else: the format is fixed by the message rather
    // than declared beside it, and both ends check the PNG signature and the
    // `IHDR` before the bytes travel any further. `image/png` there is not a
    // promise the page is making; it is what the bytes already proved on the way
    // in. The alternative was `seele-core::preview::data_uri`, which this crate
    // may not name — ADR 0002 gives the shell `seele-ffi` and nothing past it —
    // so the choice was this channel or a second base64 encoder in `main.rs`.
    //
    // The exception is held to exactly that shape below, rather than trusted.
    let excecao = "tela-server.js";
    let mut fonte = String::new();
    for name in ui_files(".js") {
        if name == excecao {
            continue;
        }
        fonte.push_str(&without_comments(&read(&format!("ui/{name}"))));
    }
    for tipo in ["image/png", "image/jpeg", "image/gif", "image/webp"] {
        assert!(
            !fonte.contains(tipo),
            "the frontend writes out `{tipo}`, which is a second copy of a list \
             that already lives in `seele-core::preview`"
        );
    }

    // The exception, kept to one type, one function, and no claim in sight.
    let servidor = read(&format!("ui/{excecao}"));
    let limpo = without_comments(&servidor);
    for tipo in ["image/jpeg", "image/gif", "image/webp"] {
        assert!(
            !limpo.contains(tipo),
            "`{excecao}` writes out `{tipo}`. The exemption above is for the one \
             format `SetServerIcon` fixes; a second one there is a page choosing \
             a decoder again"
        );
    }
    assert_eq!(
        limpo.matches("image/png").count(),
        1,
        "`{excecao}` names `image/png` more than once, so the exemption has \
         spread beyond the single channel that composes the server's picture"
    );
    let compoe = js_function(&servidor, "function uriDeIcone(");
    assert!(
        compoe.contains("image/png"),
        "the one `image/png` in `{excecao}` is no longer inside `uriDeIcone`, so \
         it is somewhere this test cannot see what it is being joined to: \
         {compoe}"
    );
    for reivindicado in ["declared_type", "claimed", "found", "anexo", "previa"] {
        assert!(
            !compoe.contains(reivindicado),
            "`uriDeIcone` reads `{reivindicado}`, so the type it writes is being \
             joined to bytes that came with somebody's claim about them — which \
             is the one thing ADR 0027 forbids: {compoe}"
        );
    }
    let pode = js_function(&read("ui/tela-sessao.js"), "function podeOferecerPrevia(");
    assert!(
        pode.contains("regrasDePrevia"),
        "the offer is decided from something other than the rules Rust sent: \
         {pode}"
    );
}

#[test]
fn a_file_that_is_not_what_it_says_it_is_gets_its_own_sentence() {
    // `NOTAS-DE-RELEASE.md` separates «did it arrive whole» from «is it what it
    // says it is». The hash answers the first. This is the second, and its
    // answer being no is not a transfer error, not something to retry, and not
    // something to leave as a silence that reads like a defect.
    let frase = js_function(&read("ui/frases.js"), "function fraseDePrevia(");
    assert!(
        frase.contains("Disagrees"),
        "there is no sentence for a file whose bytes disagree with its name: \
         {frase}"
    );
    assert!(
        frase.contains("chegou inteiro"),
        "the sentence does not separate the question that had an answer from \
         the one that has just been answered: {frase}"
    );
    assert!(
        frase.contains("previa.claimed") && frase.contains("previa.found"),
        "the sentence does not say what the file claimed to be and what it \
         turned out to be, which is the whole content of it: {frase}"
    );
    // And it says the file is still there. Not drawing is not hiding.
    assert!(
        frase.contains("salvar"),
        "the sentence leaves somebody thinking the file is gone: {frase}"
    );
}

#[test]
fn the_reason_a_picture_was_not_drawn_is_written_where_it_can_be_seen() {
    // ADR 0027 already paid once for a failure told only to `.anuncio`, which
    // is a one-pixel box clipped off the screen. To somebody looking at the
    // window, a refusal reported only there and nothing at all are the same
    // event.
    let desenho = js_function(&read("ui/tela-sessao.js"), "function desenhoDaPrevia(");
    assert!(
        desenho.contains("anexo-recusa") && desenho.contains("fraseDePrevia("),
        "a preview that produced no picture writes no sentence into the block: \
         {desenho}"
    );
    let ver = js_function(&read("ui/tela-sessao.js"), "async function verPrevia(");
    assert!(
        ver.contains("replaceWith(") || ver.contains("append("),
        "the drawing is never put into the page: {ver}"
    );

    // And the region it lands in is not one of the clipped ones. Read as a
    // property of the stylesheet, not by trusting the class name.
    let folha = without_comments(&read("ui/tela-sessao.css"));
    let bloco = folha
        .split('}')
        .find(|bloco| {
            bloco
                .split_once('{')
                .is_some_and(|(seletor, _)| seletor.contains(".anexo-recusa"))
        })
        .unwrap_or_else(|| panic!("`.anexo-recusa` has no rule of its own"));
    for escondido in ["clip-path", "display: none", "visibility: hidden"] {
        assert!(
            !bloco.contains(escondido),
            "the refusal lands in a region the stylesheet hides with \
             `{escondido}`: {bloco}"
        );
    }
}

#[test]
fn a_preview_is_not_an_open_and_is_not_a_save() {
    // The distance between drawing a picture and opening a file is where this
    // could go wrong worst, so the channel is written rather than assumed. A
    // preview holds bytes in this window's memory: no path, no file, nothing
    // handed to the operating system. Saving stays the one verb with a
    // destination, and it keeps its confirmation.
    let ver = js_function(&read("ui/tela-sessao.js"), "async function verPrevia(");
    let desenho = js_function(&read("ui/tela-sessao.js"), "function desenhoDaPrevia(");
    for corpo in [&ver, &desenho] {
        for proibido in [
            "salvar_anexo",
            "pastaDeDestino",
            "destino",
            "armarAto(",
            "download",
        ] {
            assert!(
                !corpo.contains(proibido),
                "the preview path reaches for `{proibido}`: previewing is not \
                 saving, and it must not become it by accident: {corpo}"
            );
        }
    }

    // The block still offers both, and still offers no third thing. The guard
    // for the absence of an open button reads the whole frontend; this one
    // reads the block, because that is where a preview button could have
    // replaced a save button rather than joined it.
    let bloco = js_function(&read("ui/tela-sessao.js"), "function blocoDeAnexo(");
    assert!(
        bloco.contains("anexo-salvar") && bloco.contains("anexo-previa"),
        "the attachment block lost one of its two verbs: {bloco}"
    );
    // And an expired attachment offers neither: there are no bytes to draw and
    // none to save.
    let expirado = bloco.split("if (anexo.expired)").nth(1).unwrap_or_default();
    let ramo = expirado.split("} else {").next().unwrap_or_default();
    assert!(
        !ramo.contains("anexo-previa") && !ramo.contains("anexo-salvar"),
        "an expired attachment is offered a button for bytes that are gone: \
         {ramo}"
    );
}

#[test]
fn the_content_security_policy_did_not_move_to_make_room_for_a_picture() {
    // ADR 0029 made this an explicit criterion: if drawing had needed the
    // policy loosened, the answer would have been no. It did not — `data:` was
    // already permitted for images and for nothing else — and this guard is
    // what keeps the next picture from being worth an entry.
    let config = read("tauri.conf.json");
    let Some(at) = config.find("\"csp\"") else {
        panic!("the window ships without a content security policy");
    };
    let rest = &config[at..];
    let Some(open) = rest.find(": \"") else {
        panic!("the csp entry has no value");
    };
    let value = &rest[open + 3..];
    let Some(end) = value.find('"') else {
        panic!("the csp value is not terminated");
    };
    let csp = &value[..end];

    assert!(
        csp.contains("default-src 'self'"),
        "the default source is no longer `self`: {csp}"
    );
    assert!(
        csp.contains("img-src 'self' data:"),
        "images are drawn from somewhere other than this bundle and `data:`: \
         {csp}"
    );
    for afrouxado in ["unsafe-inline", "unsafe-eval", "img-src *", "https:"] {
        assert!(
            !csp.contains(afrouxado),
            "the policy gained `{afrouxado}`, and no picture is worth that: \
             {csp}"
        );
    }

    // `blob:` era proibido em toda a política, e a frase era sobre imagem. O
    // ADR 0049 o admite numa diretiva só — `worker-src` —, porque é assim que
    // um `Worker` de mesma origem se constrói, e só ali. Numa imagem ou num
    // script ele continua valendo a mesma recusa de antes: seria um caminho
    // para bytes que ninguém conferiu virarem conteúdo da página.
    for diretiva in csp.split(';').map(str::trim) {
        assert!(
            !diretiva.contains("blob:") || diretiva.starts_with("worker-src"),
            "`blob:` entrou em `{diretiva}`, e fora de `worker-src` ele é um \
             caminho para conteúdo que ninguém conferiu: {csp}"
        );
    }
}

// ---------------------------------------------------------------------------
// Quem bate à porta, dos dois lados — ADR 0030, pendência 23.
//
// A portaria decidia e ninguém do outro lado ficava sabendo. Num teste entre
// duas casas o amigo bateu, leu a frase certa e ficou esperando; quem hospeda
// olhava uma tela onde nada indicava nada. Os cinco guardas abaixo são as cinco
// maneiras de reintroduzir metade daquilo sem que nada mais reclame.
// ---------------------------------------------------------------------------

/// The entry screen, cut out of the page.
///
/// `screens_of` hands back everything from a section to the end of the file,
/// which is fine for asking what a screen *has* and useless for asking what it
/// no longer has — every earlier screen would answer for the later ones. This
/// cuts at the closing tag, and `#tela-auth` nests no other `</section>`.
fn entry_screen() -> String {
    let page = without_comments(&read("ui/index.html"));
    let Some(after) = page.split("id=\"tela-auth\"").nth(1) else {
        panic!("index.html no longer has the entry screen");
    };
    let Some(screen) = after.split("</section>").next() else {
        panic!("#tela-auth is never closed");
    };
    screen.to_owned()
}

#[test]
fn the_knock_notice_never_takes_the_keyboard_from_somebody_mid_sentence() {
    // The half of this feature that is easiest to get wrong by being helpful.
    // Whoever hosts is very often *talking* — the button that hosts drops them
    // straight into a session, and the push-to-talk of the call screen is a key
    // that stops working the moment focus lands in a text field. A modal, an
    // `alert` role, or a bare `focus()` on a band that appears every time
    // somebody knocks is worse than the silence it replaces.
    let page = without_comments(&read("ui/index.html"));

    // Outside every screen, checked by position rather than by asking each
    // screen whether it contains it: `screens_of` runs each piece to the end of
    // the file, so "not inside the last screen" is a question it cannot answer.
    // Before the first `<section id="tela-` is exact.
    let Some(faixa) = page.find("id=\"portaria-batendo\"") else {
        panic!("the page no longer has the band that says somebody is knocking");
    };
    let Some(primeira) = page.find("<section id=\"tela-") else {
        panic!("index.html no longer draws screens");
    };
    assert!(
        faixa < primeira,
        "the knock band moved inside a screen, where a `hidden` on that section \
         takes it off the page along with everything else — and whoever hosts is \
         inside a voice_room or in the settings exactly when somebody knocks"
    );

    let tag = tag_with_id(&read("ui/index.html"), "portaria-batendo");
    for roubo in ["role=\"alert\"", "aria-modal", "autofocus"] {
        assert!(
            !tag.contains(roubo),
            "the knock band carries `{roubo}`, which interrupts whoever is \
             reading or talking: <{tag}>"
        );
    }

    // And the script that reveals it must not reach for the keyboard either.
    // Scoped to the function body, with comments already stripped by `body_of`:
    // the paragraph above it says `focus()` in prose, and a guard its own
    // rationale can satisfy is a guard that cannot fail.
    let corpo = body_of(&scripts(), "function avisarQueBatem");
    for roubo in ["focus(", "disabled", "abrirTela("] {
        assert!(
            !corpo.contains(roubo),
            "the knock band reaches for `{roubo}` when it appears, so it takes \
             over from whoever was in the middle of something:\n{corpo}"
        );
    }

    // It still has to appear, or the guard above is guarding nothing.
    assert!(
        corpo.contains("faixa.hidden"),
        "nothing shows or hides the band, so it is markup that never runs:\n{corpo}"
    );
    assert!(
        corpo.contains("anunciar("),
        "the band appears and says nothing to somebody who cannot see it, which \
         is the same silence this exists to end:\n{corpo}"
    );
}

#[test]
fn the_knock_is_read_out_once_per_appearance_and_not_once_per_poll() {
    // The door is read every five seconds. A live region written on every read
    // says "somebody is knocking" twelve times a minute at somebody wearing
    // headphones — which is how a person learns not to hear the thirteenth, and
    // it is the argument ADR 0003 makes about the key-change warning.
    //
    // So the sentence belongs to the *transition* into view: it is guarded by
    // the band having been hidden, and it happens before the channel that stops it
    // being hidden.
    let corpo = body_of(&scripts(), "function avisarQueBatem");

    let Some(fala) = corpo.find("anunciar(") else {
        panic!("the band no longer says anything out loud:\n{corpo}");
    };
    let Some(escreve) = corpo.find("faixa.hidden = !") else {
        panic!("the band's visibility is no longer decided in one place:\n{corpo}");
    };
    assert!(
        fala < escreve,
        "the announcement is written after the band is revealed, so it asks \
         whether the band was hidden about a band it has just shown — and the \
         sentence is never said:\n{corpo}"
    );

    // The condition itself, and not the words somewhere above it. The earlier
    // `faixa.hidden = true` of the empty-queue branch sits in the same body, so
    // a check for the name alone passes with the guard deleted.
    let Some(inicio) = corpo[..fala].rfind("if (") else {
        panic!("the announcement is behind no condition at all:\n{corpo}");
    };
    let guarda = &corpo[inicio..fala];
    assert!(
        guarda.contains("faixa.hidden"),
        "the announcement does not ask whether the band was already showing, so \
         the same knock is read out on every five-second poll:\n{guarda}"
    );
    assert!(
        guarda.contains("mostrar"),
        "the announcement does not ask whether the band is going to be shown, so \
         it speaks for knocks that DEPOIS already silenced:\n{guarda}"
    );

    // And reading the queue silences it. Without this the band comes straight
    // back the moment the layer closes, over exactly the people just read.
    let fila = body_of(&scripts(), "function desenharFila");
    assert!(
        fila.contains("calarBatidas("),
        "opening the door and reading the queue leaves the band armed, so it \
         reappears about the knocks whoever hosts has just looked at:\n{fila}"
    );
}

#[test]
fn a_espera_so_bate_enquanto_alguem_esta_olhando() {
    // Este guarda mudou de forma, e a mudança está registrada no ADR 0030.
    //
    // Ele exigia que **nada** batesse sozinho, e a razão citada era boa: uma
    // tela que repete no relógio bate no servidor de outra pessoa para sempre,
    // contra o balde por endereço do ADR 0025.
    //
    // Duas metades daquela razão não se sustentaram, e uma sustentou.
    //
    // Não se sustentou a leitura de que isto é o que o ADR 0030 recusou: lá o
    // que foi recusado é **segurar a conexão** enquanto quem hospeda decide, e
    // uma batida daqui conecta, é recusada e desconecta, sem segurar recurso
    // nenhum do outro lado. Não se sustentou a conta: quinze segundos são
    // quatro batidas por minuto, e o balde de antes de autenticar do ADR 0025
    // repõe trinta por minuto — a bateria de reconexão que já existe bate mais.
    //
    // Sustentou-se o **para sempre**. E o próprio ADR 0030 nomeia o caso, na
    // alternativa 2: «o caso que importa, o da janela minimizada». Uma janela
    // minimizada batendo por horas, com ninguém para ver a resposta, é gasto no
    // servidor de um estranho por uma espera que ninguém está esperando.
    //
    // Então a propriedade cobrada aqui deixou de ser «não bate sozinho» e
    // passou a ser **«só bate enquanto alguém está olhando»**.
    let tela = without_comments(&read("ui/tela-auth.js"));

    assert!(
        tela.contains("visibilityState"),
        "a espera automática não pergunta se alguém está olhando, então uma          janela minimizada bate na porta de um estranho por horas — o caso que          a alternativa 2 do ADR 0030 nomeia:\n{tela}"
    );
    assert!(
        tela.contains("visibilitychange"),
        "nada religa a espera quando a janela volta, então minimizar uma vez a          encerra em silêncio e quem volta fica olhando uma tela que desistiu"
    );

    // A pausa é a primeira coisa que a contagem decide, e não um `if` perdido
    // depois de já ter marcado o relógio. Um `setInterval` armado antes da
    // pergunta bate uma vez com a janela escondida, que é exatamente o que
    // isto existe para impedir.
    let contar = body_of(&scripts(), "function contarParaBater");
    let Some(pergunta) = contar.find("alguemOlhando()") else {
        panic!("a contagem não pergunta se alguém está olhando:\n{contar}");
    };
    let Some(arma) = contar.find("setInterval(") else {
        panic!("a contagem não arma relógio nenhum:\n{contar}");
    };
    assert!(
        pergunta < arma,
        "o relógio é armado antes de a contagem perguntar se alguém está \
         olhando, então a janela escondida ainda bate uma vez:\n{contar}"
    );

    // E sair pela porta encerra, em vez de pausar. Sem isto, voltar à janela
    // depois de desistir recomeçaria a bater numa porta que a pessoa já deixou.
    let voltar = body_of(&scripts(), "function voltarParaAEntrada");
    assert!(
        voltar.contains("esperando = false"),
        "sair da espera pela porta não encerra a espera, só para o relógio — e \
         voltar à janela a religa sobre um servidor que a pessoa abandonou:\n{voltar}"
    );
}

#[test]
fn the_waiting_screen_says_what_happened_what_to_do_and_what_is_useless() {
    // The three questions somebody who has just been dropped is holding, and
    // the third is the one a shell never writes down: the decision is durable
    // and does not expire, nothing on this side is waiting, and the approval
    // will not pull anybody back — so trying again later is the whole method,
    // and trying again now, over and over, is how you get rate-limited out.
    let bloco = {
        let tela = entry_screen();
        let Some(after) = tela.split("id=\"auth-espera\"").nth(1) else {
            panic!("the entry screen has no block for somebody waiting:\n{tela}");
        };
        let Some(bloco) = after.split("</div>").next() else {
            panic!("the waiting block is never closed");
        };
        bloco.to_owned()
    };

    for (needle, o_que) in [
        ("não vence", "that the request does not expire"),
        (
            "puxar de volta",
            "that nothing pulls them back when it is decided",
        ),
        ("tentativa por vez", "that knocking is one press at a time"),
        (
            "outro canal",
            "that asking whoever hosts directly is faster",
        ),
    ] {
        assert!(
            bloco.contains(needle),
            "the waiting screen never says {o_que}, which is the half of this \
             that a channel of red error text could not carry:\n{bloco}"
        );
    }

    // What happened comes from the enum, through the one place enums become
    // sentences. A second wording written here would be a second thing to keep
    // in step with `MOTIVOS`, which is what the end screen reads.
    let corpo = body_of(&scripts(), "function levarParaAEspera");
    assert!(
        corpo.contains("fraseDeErro(") && corpo.contains("auth-espera-frase"),
        "the waiting screen writes its own account of what happened instead of \
         the sentence the refusal already has:\n{corpo}"
    );
    assert!(
        corpo.contains("AdmissionPending"),
        "nothing routes a pending knock to this screen, so it is a screen \
         nobody arrives at:\n{corpo}"
    );

    // A decided refusal is a wall with the reason on it, and not a button that
    // keeps working. Same shape as the last voice room: drawn, disabled, explained —
    // a button that vanishes is a button somebody hunts for.
    assert!(
        corpo.contains("AdmissionDenied") && corpo.contains("disabled"),
        "a decided refusal leaves the try-again button live, so the screen \
         invites knocking on a door that has already answered:\n{corpo}"
    );
    assert!(
        corpo.contains("auth-parede"),
        "the refusal disables the button and says nothing about why, which \
         reads as the window being broken:\n{corpo}"
    );

    // Only a pending knock takes the entrance over. Everything else stays on
    // the boot screen, where another server can be chosen — and the boot screen
    // writes its own red channel only when this screen did not take the failure.
    let conectar = body_of(&scripts(), "async function conectar");
    let Some(desvio) = conectar.find("levarParaAEspera(") else {
        panic!("nothing sends a pending knock anywhere:\n{conectar}");
    };
    let Some(linha) = conectar.find("erro.textContent = fraseDeErro(") else {
        panic!("the entrance no longer writes why a connection failed:\n{conectar}");
    };
    assert!(
        desvio < linha,
        "the entrance writes its error channel before asking whether the failure \
         belongs to the waiting screen, so both say it at once:\n{conectar}"
    );
}

#[test]
fn the_entrance_screen_stopped_drawing_the_four_values_this_protocol_never_carried() {
    // The convention it drops was a good one applied too far: draw the frame,
    // write the value as missing, and the gap stays visible. That serves a gap
    // somebody means to close. These four never closed — there is no population
    // count and no "route" anywhere in the core, the codec has no value until a
    // connection is inside a voice room and nobody is yet, and the local key does not cross
    // the FFI — so what the screen showed was seven fields with four dashes,
    // which reads as a broken window rather than as an honest one.
    let tela = entry_screen();

    for morto in [
        "OPERADORES",
        "ROTA",
        "auth-chave",
        "auth-codec",
        "legenda-ausente",
    ] {
        assert!(
            !tela.contains(morto),
            "`{morto}` is back on the entry screen, a frame drawn around a value \
             this protocol does not carry"
        );
    }

    // The ones that are real stay, or this passes by deleting the panel.
    for vivo in ["auth-voice_rooms", "auth-linhas", "auth-server-nome"] {
        assert!(
            tela.contains(vivo),
            "`{vivo}` left the entry screen too, and that one comes straight out \
             of the snapshot"
        );
    }

    // And nothing writes into the ones that went. A script still filling an
    // element that is not in the page throws on the channel after `$()`.
    let script = without_comments(&scripts());
    for morto in ["auth-chave", "auth-codec"] {
        assert!(
            !script.contains(morto),
            "a script still fills `{morto}`, which is no longer in the page"
        );
    }
}

#[test]
fn the_note_beside_a_control_is_not_painted_in_the_colour_reserved_for_large_text() {
    // The note beside a control is, on several screens, the only thing that says
    // what the control does. Painting it in `osso-apagado` — which `tokens.css`
    // annotates in its own line as "4,11:1 só texto grande" — writes the
    // explanation in type the explained person cannot read.
    //
    // `docs/tokens-achados.md` settled this before the class existed: the
    // pending choice on that colour is to raise it *or* to make sure nothing
    // necessary depends on it alone. A note is necessary by definition.
    //
    // The check is on the token name and not on a computed ratio, deliberately.
    // A ratio computed here would have to be recomputed the day the palette
    // moves, and would then measure a number nobody had decided; the name is
    // what the decision was actually about.
    let sheet = read("ui/base.css");
    let Some(after) = sheet.split("\n.nota {").nth(1) else {
        panic!("base.css no longer declares `.nota`, so nothing owns the note beside a control");
    };
    let Some(rule) = after.split('}').next() else {
        panic!("the `.nota` rule is never closed");
    };

    for dim in ["osso-apagado", "rotulo-painel"] {
        assert!(
            !rule.contains(dim),
            "`.nota` is painted with `{dim}`, which tokens.css marks as large-text \
             only at 4,11:1. These are small prose lines nobody can turn off any \
             more — they are the one thing on screen that has to be legible to \
             somebody who does not already know the app.\n{rule}"
        );
    }
}

#[test]
fn the_note_beside_a_control_is_defined_once_and_never_by_a_screen() {
    // `.nota` is the short channel beside a control saying what pressing it causes.
    // On the call screen it is the *only* thing separating VER LINHAS from SAIR
    // DA JAULA; in the moderation box it is the only thing that says a ban has
    // no undo. One class, one rule in `base.css`, and it has to stay one.
    //
    // The failure this catches is a screen writing its own `.nota { … }`. That
    // screen's notes then answer to nobody: the day a rule here changes how
    // these channels are drawn — or whether they are drawn at all — every screen
    // follows except that one, and nothing about the divergence is visible
    // without opening each screen and comparing.
    //
    // This is not hypothetical for this class in particular. It *was* a mode
    // once, hidden behind a switch, and a screen claiming the class would have
    // silently opted out of the switch. The switch is gone and the argument
    // survived it, because it was never really about the switch.
    //
    // Refining is still allowed and is the point of the load order: a screen may
    // write `.painel .nota { margin-top: 4px }`, because the owner of that rule
    // is `.painel`. What it may not do is claim `.nota` itself.
    let shared = classes_defined_in(&read("ui/base.css"));
    assert!(
        shared.contains("nota"),
        "`.nota` is not defined in base.css, so the note beside a control has no \
         shared owner and each screen is free to invent one"
    );

    let not_a_screen = ["base.css", "acessibilidade.css", "tokens.css", "fontes.css"];
    for name in ui_files(".css") {
        if not_a_screen.contains(&name.as_str()) {
            continue;
        }
        let owned = classes_defined_in(&read(&format!("ui/{name}")));
        assert!(
            !owned.contains("nota"),
            "{name} claims `.nota`, which base.css owns. A screen that redefines \
             it decides on its own whether the explanation of a control appears, \
             and nothing about that failure is visible — the channel either keeps \
             showing or stops showing on exactly one screen."
        );
    }
}

#[test]
fn the_captions_mode_does_not_come_back_by_accident() {
    // `LEGENDAS SIMPLES` was a second copy of the interface: every explanatory
    // channel beside a control could be switched off from the settings screen, and
    // it was on by default, so nobody ever saw the other copy — least of all
    // whoever wrote a new channel and never read the app without it.
    //
    // On the call screen the two exits are told apart by nothing but those
    // channels; in the moderation box the only sentence saying a ban has no undo is
    // one of them. A mode that hides them is a mode in which this product lies
    // by omission to whoever turned it on.
    //
    // So the mode is gone and the channels stayed, always visible, as `.nota`. The
    // way this rots is somebody reintroducing "just a toggle" — and the failure
    // is silent by construction, because a hidden channel looks exactly like a channel
    // nobody ever wrote.
    //
    // Read with comments stripped, and that is not tidiness: the paragraph in
    // `base.js` explaining what was removed names every one of these words. A
    // check satisfied by the note explaining a removal is a check that can never
    // fail again.
    let mut source = String::new();
    for name in ui_files(".js") {
        source.push_str(&without_comments(&read(&format!("ui/{name}"))));
        source.push('\n');
    }
    for name in ui_files(".css") {
        source.push_str(&without_comments(&read(&format!("ui/{name}"))));
        source.push('\n');
    }
    source.push_str(&without_comments(&read("ui/index.html")));

    for gone in [
        "legendasSimples",
        "aplicarLegendas",
        "legendas-simples",
        "CHAVE_LEGENDAS",
        "server-legendas",
        "server-interruptor",
        "server-chave",
    ] {
        assert!(
            !source.contains(gone),
            "`{gone}` is back in the frontend, and the simple-captions mode comes \
             with it: the explanation of a control becomes something a person can \
             switch off, and nothing on screen ever says it was switched"
        );
    }

    // And the class it toggled. Written as the forms the class actually takes,
    // and not as the bare Portuguese noun: the point is the class, and a
    // sentence that happens to use the word «dica» is not the layer.
    for gone in [
        "class=\"dica",
        ".dica ",
        ".dica{",
        ".dica {",
        "dica-linha",
        "\"dica\")",
    ] {
        assert!(
            !source.contains(gone),
            "`{gone}` is back in the frontend: the class the captions mode toggled \
             has an owner again, and the toggle is one rule away from it"
        );
    }

    // The other half, and it is what makes this a guard rather than a grep: the
    // text those channels carried has to still be on screen. Removing the mode by
    // deleting the sentences would pass everything above, and it is the one
    // outcome worse than the mode.
    let page = without_comments(&read("ui/index.html"));
    let notes = page.matches("class=\"nota").count();
    assert!(
        notes >= 20,
        "the page draws {notes} notes beside controls. The mode was removed by \
         deleting the sentences instead of by making them permanent."
    );
}

#[test]
fn the_alert_box_has_one_way_out_and_the_keyboard_can_take_it() {
    // The alert box used to close two ways: a `RECONHECER` as wide as the box at
    // the bottom, whose entire job was `hidden = true`. Two controls for one act
    // in one box is somebody reading both to work out which is the real one —
    // and `RECONHECER` reads as if it records something, which it never did.
    //
    // What is left is the conventional `×` in the corner, and that raises the
    // bar rather than lowering it: a glyph is not a word, and a box covering the
    // whole window that only closes under the pointer is worse than the
    // redundancy it replaced. The four halves below are one invariant.
    let page = without_comments(&read("ui/index.html"));
    let Some(after) = page.split("id=\"banner\"").nth(1) else {
        panic!("index.html no longer has the alert box");
    };
    let Some(caixa) = after.split("id=\"veredito\"").next() else {
        panic!("`#veredito` no longer follows the alert box, so this slice has no end");
    };

    // Every button in the box, by id and by the words on it.
    let mut buttons: Vec<(String, String)> = Vec::new();
    for piece in caixa.split("<button ").skip(1) {
        let Some(end) = piece.find('>') else { continue };
        let tag = &piece[..end];
        let Some(id) = attribute(tag, "id") else {
            panic!("a button in the alert box has no id: <{tag}>");
        };
        let label = piece[end + 1..]
            .split("</button>")
            .next()
            .unwrap_or_default()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        buttons.push((id, label));
    }

    assert!(
        buttons.iter().any(|(id, _)| id == "banner-fechar"),
        "the alert box has no `#banner-fechar`, so the one thing that closed it \
         is gone: {buttons:?}"
    );

    // Nothing else in the box may be a second way out. The vocabulary is what a
    // dismissing button gets called in this window, and a new one arriving under
    // any of these words is the redundancy coming back under another label —
    // which is how it arrived the first time.
    for (id, label) in &buttons {
        if id == "banner-fechar" {
            continue;
        }
        let shouting = label.to_uppercase();
        for dismissal in ["FECHAR", "RECONHECER", "DISPENSAR", "ENTENDI", "CANCELAR"] {
            assert!(
                !shouting.contains(dismissal),
                "`{id}` is a second way to close the alert box — it says «{label}», \
                 and `#banner-fechar` already does that. One box, one way out."
            );
        }
    }

    // The `×` is a glyph, so the accessible name has to be written out.
    let fechar = tag_with_id(&read("ui/index.html"), "banner-fechar");
    let nome = attribute(&fechar, "aria-label").unwrap_or_default();
    assert!(
        nome.len() > 3,
        "`#banner-fechar` draws a glyph and carries no accessible name, so the \
         only way out of this box is announced as a multiplication sign: <{fechar}>"
    );

    // And it has to be visible when the keyboard lands on it.
    assert!(
        read("ui/camada-alerta.css").contains(".alerta-fechar:focus-visible"),
        "the only way out of the alert box has no focus ring, so somebody \
         tabbing to it cannot tell they are on it"
    );

    // Escape closes it, because the pointer must not be the only way. Scoped to
    // the branch and not to the word: `Escape` is named in the shortcuts section
    // and in four other handlers, and an unscoped search would be satisfied by
    // any of them.
    let script = without_comments(&scripts());
    let branches: Vec<&str> = script
        .split("evento.key === \"Escape\"")
        .skip(1)
        .map(|rest| rest.split("\n  }").next().unwrap_or_default())
        .collect();
    assert!(
        branches
            .iter()
            .any(|branch| branch.contains("$(\"banner\")")),
        "no Escape handler closes the alert box. It covers the whole window, it \
         has one small `×`, and without this the keyboard is stuck behind it."
    );
}

#[test]
fn nothing_arms_an_act_in_a_box_it_never_opened() {
    // The defect behind «o botão de salvar anexo não funciona», generalised.
    //
    // `armarAto` writes a consequence into `#moderar`, swaps its body for the
    // confirmation and focuses CANCELAR. It does **not** reveal `#moderar` — the
    // three doors of `camada-moderar.js` do that, and they also record the focus
    // to give back and whether CANCELAR closes the box or steps back to a list.
    //
    // `salvarAnexo` called the middle instead of a door, and the result was the
    // quietest failure available: pressing SALVAR did nothing. No error, no
    // console channel, no frame — `focus()` inside a `hidden` element is a silent
    // no-op — and the act sat armed in a box nobody would ever see. The guard
    // covering saving asserted that `armarAto(` was called, so it stayed green
    // for as long as the button was dead.
    //
    // The rule that would have caught it: only the file that owns the box may
    // reach for its middle. Everybody else goes through a door.
    for name in ui_files(".js") {
        if name == "camada-moderar.js" {
            continue;
        }
        let source = without_comments(&read(&format!("ui/{name}")));
        assert!(
            !source.contains("armarAto("),
            "{name} calls `armarAto(`, which arms a confirmation without opening \
             the box that would show it. The button goes silent and nothing \
             anywhere says why. Call `abrirConfirmacao` or `abrirRecusa`."
        );
    }

    // And the door saving goes through is one of the two that reveal the box.
    let salvar = js_function(&read("ui/tela-sessao.js"), "function salvarAnexo(");
    assert!(
        salvar.contains("abrirConfirmacao(") || salvar.contains("abrirRecusa("),
        "saving an attachment reaches no door of the moderation layer, so \
         whatever it does happens inside a box that never opens:\n{salvar}"
    );
}

#[test]
fn saving_never_writes_to_a_path_this_window_cannot_name() {
    // The destination comes from `pasta_de_downloads`, read once at load. On the
    // Rust side that is `download_dir()`, falling back to `home_dir()`, falling
    // back to `unwrap_or_default()` — an empty string. The shell held it in
    // `pastaDeDestino` and built `${pastaDeDestino}/${nome}`, so an empty folder
    // produced a bare file name: a **relative** path, written wherever the
    // process happened to be started from.
    //
    // That is the worst of the outcomes available here. The file is written for
    // real, so nothing fails; the sentence the person confirmed named a place
    // that is not where it went; and nobody finds it afterwards. A refusal that
    // says so is the only honest branch, and it has to come before the path is
    // ever assembled.
    let salvar = js_function(&read("ui/tela-sessao.js"), "function salvarAnexo(");

    let Some(guard) = salvar.find("pastaDeDestino === \"\"") else {
        panic!(
            "`salvarAnexo` never asks whether it knows the destination folder, so \
             an empty one becomes a relative path and the file lands somewhere \
             this window cannot name:\n{salvar}"
        );
    };
    let Some(path) = salvar.find("${pastaDeDestino}") else {
        panic!("`salvarAnexo` no longer builds the destination from the folder:\n{salvar}");
    };
    assert!(
        guard < path,
        "`salvarAnexo` builds the path before checking that it has a folder to \
         build it from, so the check cannot stop anything:\n{salvar}"
    );

    // And the empty case has to *say* so rather than return in silence, which is
    // the failure this area keeps producing.
    assert!(
        salvar.contains("abrirRecusa("),
        "with no destination folder, saving gives up without a word — the same \
         silence the broken button had:\n{salvar}"
    );
    assert!(
        salvar.contains("Nada foi gravado"),
        "the refusal does not say that nothing was written, which is the one \
         thing somebody who just pressed SALVAR needs to know:\n{salvar}"
    );
}

// ---------------------------------------------------------------------------
// The ruler: one sentence, and a second one only when it changes what somebody
// does. A third does not exist.
// ---------------------------------------------------------------------------

/// The longest a sentence in `ui/frases.js` may be, in characters.
///
/// Measured, not guessed. After the cut that produced this guard the eighty-one
/// sentences average 60 characters and the median is 44; the longest is 173 —
/// `FRASES.FuroDeNat`, the one rung of the ladder with a third party in it,
/// held at that length by
/// `the_nat_punching_rung_names_its_cost_where_the_cost_is_paid`,
/// which requires it to name the meeting point, what it learns, what it does
/// not, the way out, and where to switch it. 180 is that sentence plus the
/// slack of rewording one clause.
///
/// The number exists because prose comes back. The sentences this replaced ran
/// to 425 characters, and every one of them was written a clause at a time —
/// each addition true, each defensible on the day, and the sum three paragraphs
/// under a button nobody read to the end. A reviewer can be talked out of "this
/// is too long"; a number cannot.
const LIMITE_DE_FRASE: usize = 180;

/// The dictionaries of `ui/frases.js`, which is where sentences live.
///
/// Not every string this product shows is here. A confirmation that spends the
/// window on an irreversible act is written next to the act, in
/// `camada-moderar.js` and `camada-portaria.js`, and those were measured when
/// they were written — a ban, a deleted Linha, an update that takes a hosted
/// server down with the window. Nor the two `fraseDeErro` composes for a changed
/// key: those are two fingerprints with a channel either side, and a fingerprint is
/// as long as it is. What this guards is the prose.
const DICIONARIOS: [&str; 9] = [
    "MOTIVOS",
    "AVISOS",
    "ETAPAS",
    "CAMINHOS",
    "PARADAS",
    "FRASES",
    "ANEXOS",
    "TRANSFERENCIAS",
    "PREVIAS",
];

/// Every string literal on one channel, joined, with `\n` turned into a real break.
///
/// The entries are written both ways — one literal on one channel, and four joined
/// by `+` across four channels — so counting has to see the text somebody reads and
/// not the source that produces it. Counting the source would let a sentence
/// grow by being split into more pieces, which is exactly the move this exists
/// to stop.
fn literals_in(channel: &str) -> String {
    let mut out = String::new();
    let mut chars = channel.chars();
    while let Some(quote) = chars.next() {
        if quote != '"' {
            continue;
        }
        loop {
            match chars.next() {
                None | Some('"') => break,
                Some('\\') => match chars.next() {
                    Some('n') => out.push('\n'),
                    Some(other) => out.push(other),
                    None => break,
                },
                Some(other) => out.push(other),
            }
        }
    }
    out
}

/// Every sentence one dictionary of `ui/frases.js` writes, as (variant, text).
///
/// An entry opens at `Name:` at the start of a channel and runs until the next one
/// does, which is what holds a sentence together across the `+` that joins its
/// pieces. Feed it comment-free source: the blocks above these entries quote the
/// sentences they explain, and a length read off an explanation is a length read
/// off the wrong thing.
fn sentences_of(file: &str, dictionary: &str) -> Vec<(String, String)> {
    let Some(after) = file.split(&format!("const {dictionary} = {{")).nth(1) else {
        panic!("`{dictionary}` is gone from ui/frases.js");
    };
    let Some(block) = after.split("\n};").next() else {
        panic!("`{dictionary}` in ui/frases.js is never closed");
    };

    let mut out: Vec<(String, String)> = Vec::new();
    let mut current: Option<(String, String)> = None;
    for channel in block.lines() {
        let trimmed = channel.trim();
        if trimmed.is_empty() {
            continue;
        }
        // ASCII only, so the byte index below lands on a character boundary and
        // a channel opening with a word like `Não` is not read as a variant name.
        let name: String = trimmed
            .chars()
            .take_while(|letter| letter.is_ascii_alphanumeric() || *letter == '_')
            .collect();
        let opens = !name.is_empty() && trimmed[name.len()..].starts_with(':');
        let text = if opens {
            if let Some(done) = current.take() {
                out.push(done);
            }
            current = Some((name.clone(), String::new()));
            &trimmed[name.len() + 1..]
        } else {
            trimmed
        };
        if let Some((_, sentence)) = current.as_mut() {
            sentence.push_str(&literals_in(text));
        }
    }
    if let Some(done) = current.take() {
        out.push(done);
    }
    out
}

#[test]
fn no_sentence_the_screen_writes_runs_past_the_length_that_gets_read() {
    // The defect this guards is not a bug and never looks like one. Every
    // sentence it replaced was true, and several were expensive to find out —
    // what a meeting point learns, why two networks refuse to punch, the
    // difference between a releases page that did not answer and one that
    // answered «nothing published». Each arrived as one more clause, and the sum
    // was three paragraphs under a button.
    //
    // So this does not ask whether a sentence is good. It asks whether it is
    // short, which is the half of the question a machine can hold.
    let file = without_comments(&read("ui/frases.js"));

    let mut counted = 0usize;
    let mut longest = (String::new(), 0usize);
    for dictionary in DICIONARIOS {
        for (variant, sentence) in sentences_of(&file, dictionary) {
            counted += 1;
            let length = sentence.chars().count();
            assert!(
                length <= LIMITE_DE_FRASE,
                "`{dictionary}.{variant}` is {length} characters and the limit is \
                 {LIMITE_DE_FRASE}. The rule it broke: one sentence saying the \
                 state and, in the active voice, what to do — and a second only \
                 when it changes what the person does. If what is here is worth \
                 keeping, it goes to docs/ and the screen points at it:\n{sentence}"
            );
            if length > longest.1 {
                longest = (format!("{dictionary}.{variant}"), length);
            }
        }
    }

    // The parser has to have found the sentences, or the loop above asserted
    // nothing at all in perfect silence — the failure every guard in this file
    // that reads another file has to answer for.
    assert!(
        counted >= 70,
        "only {counted} sentences were read out of ui/frases.js, so the entries \
         are no longer being found and this guard is measuring an empty list"
    );

    // And the ceiling has to stay a measurement. If every sentence drops far
    // under it, the number stopped describing this file and became just a
    // number — and a number nobody re-measures is the room prose grows back
    // into.
    assert!(
        longest.1 * 2 > LIMITE_DE_FRASE,
        "the longest sentence is now {} characters ({}), less than half the \
         {LIMITE_DE_FRASE} this guard allows. Lower the limit to what the file \
         actually needs, or it is holding a door open",
        longest.1,
        longest.0
    );
}

#[test]
fn no_sentence_the_screen_writes_reaches_a_third_line() {
    // The shape the rule takes on screen: a channel in capitals that says the
    // state, and one under it that says what to do. Characters alone would let
    // three short channels through, and three channels is the thing itself — the
    // FuroDeNat sentence that started this was three channels and 425 characters,
    // and the third was the one nobody was ever going to read.
    //
    // The two composed sentences keep to this by construction. `fraseDeAnexo`
    // and `fraseDePrevia` fold the byte limit into the headline instead of
    // adding a channel, because the number is what qualifies the «too big»: it
    // belongs to the sentence that says it, not under it.
    let file = without_comments(&read("ui/frases.js"));

    for dictionary in DICIONARIOS {
        for (variant, sentence) in sentences_of(&file, dictionary) {
            let lines = sentence.lines().count();
            assert!(
                lines <= 2,
                "`{dictionary}.{variant}` is written on {lines} lines. The third \
                 is the one that justifies, contextualises, or answers the \
                 question nobody has asked yet — and it belongs in docs/:\n{sentence}"
            );
        }
    }
}

#[test]
fn a_ajuda_so_promete_teclas_que_a_janela_atende() {
    // Uma ajuda que lista um atalho inexistente é pior que ajuda nenhuma: ela
    // ensina errado e não falha em lugar nenhum. Isto lê os `<span
    // class="ajuda-tecla">` da página e cobra, para cada um, que exista um
    // `keydown` em algum script desta janela que o compare.
    //
    // Esta camada existe porque a explicação do vocabulário saiu das telas —
    // legenda permanente é texto que quem já sabe lê mil vezes. O que a
    // avaliação de usabilidade pediu continua valendo e passou a morar aqui, a
    // uma tecla de distância e a zero linhas da tela permanente.
    let page = read("ui/index.html");
    let scripts = scripts();

    // O que a tecla desenhada na ajuda vira num `KeyboardEvent`.
    let atalhos = [
        ("ESPAÇO", "\"Space\""),
        ("/", "\"/\""),
        ("ENTER", "\"Enter\""),
        ("?", "\"?\""),
        ("ESC", "\"Escape\""),
    ];

    let mut desenhadas = Vec::new();
    let mut resto = page.as_str();
    while let Some(inicio) = resto.find("class=\"ajuda-tecla\">") {
        let depois = &resto[inicio + "class=\"ajuda-tecla\">".len()..];
        let Some(fim) = depois.find('<') else { break };
        desenhadas.push(depois[..fim].to_owned());
        resto = &depois[fim..];
    }

    assert!(
        !desenhadas.is_empty(),
        "a ajuda não desenha tecla nenhuma; ou a seção sumiu ou a classe mudou \
         de nome e este teste virou decoração"
    );

    for tecla in &desenhadas {
        let Some((_, valor)) = atalhos.iter().find(|(desenhada, _)| desenhada == tecla) else {
            panic!(
                "a ajuda desenha a tecla «{tecla}», que este teste não sabe \
                 traduzir para um `KeyboardEvent`. Ou ela é nova — e então \
                 entra na tabela junto com o `keydown` que a atende — ou é \
                 promessa sem dono."
            );
        };
        assert!(
            scripts.contains(valor),
            "a ajuda promete «{tecla}» e nenhum script desta janela compara \
             {valor} num `keydown`: a ajuda está ensinando um atalho que não \
             existe"
        );
    }
}

#[test]
fn a_ajuda_nao_rouba_a_interrogacao_de_quem_esta_escrevendo() {
    // O mesmo guarda que a `/` da busca e o espaço da fala têm, e pelo mesmo
    // motivo: uma interrogação escrita numa mensagem é uma interrogação. Sem
    // isto a camada abriria no meio da frase, e o caractere se perderia.
    let ajuda = read("ui/camada-ajuda.js");
    let sem_comentario = without_comments(&ajuda);
    let abre = sem_comentario
        .lines()
        .find(|linha| linha.contains("\"?\""))
        .unwrap_or_default();

    assert!(
        abre.contains("!digitando()"),
        "a tecla `?` abre a ajuda sem perguntar se a pessoa está num campo de \
         texto; a interrogação que ela quis escrever vira uma caixa na frente \
         da tela:\n{abre}"
    );
}

/// A `<section id="tela-boot">` inteira, sem comentário.
///
/// O corte é na tela seguinte e não no primeiro `</section>`, e a diferença não
/// é gosto: a entrada tem uma `<section id="visitados">` dentro dela, então
/// cortar no primeiro fechamento devolve o cabeçalho da lista de visitados e
/// nada do formulário — um recorte que passa por tela inteira e não é.
fn tela_de_entrada() -> String {
    let pagina = without_comments(&read("ui/index.html"));
    let Some(depois) = pagina.split("id=\"tela-boot\"").nth(1) else {
        panic!("index.html não tem mais a tela de entrada");
    };
    let Some(tela) = depois.split("id=\"tela-sessao\"").next() else {
        panic!("a tela de operação sumiu, e com ela o fim da tela de entrada");
    };
    tela.to_owned()
}

#[test]
fn a_entrada_desenha_a_marca_nova_e_nenhuma_citacao_do_anime() {
    // Aqui morava a metade contrária deste argumento, escrita em três lugares:
    // a assinatura da entrada tinha de ser **desenho** e nunca texto, porque
    // como texto os três katakana virariam Hiragino no macOS e Yu Gothic no
    // Windows — e substituir a face japonesa era o que a folha de marca proibia.
    //
    // O argumento continua certo e ficou sem assunto. A direção 1c abandonou o
    // katakana: a marca é dois nós e uma ligação mais o nome em latim, e latim é
    // o que a Saira Condensed embarcada desenha. O símbolo continua imagem —
    // geometria com uma fonte só, `marca-simbolo.svg` — e o nome passou a ser
    // texto, que é o que deixa a assinatura escalar com a coluna e ser lida por
    // quem não vê a tela.
    //
    // Junto com ela saíram as outras duas citações diretas do anime que só esta
    // janela tinha: o `<title>` e a linha `FILE : ENTRY_PLUG.INIT` da cartela de
    // cenário. As três eram uma decisão só e caem por um motivo só, então o
    // guarda é um.
    let pagina = without_comments(&read("ui/index.html"));
    let tela = tela_de_entrada();

    assert!(
        !pagina.contains("marca-assinatura.svg"),
        "a assinatura em contorno voltou à janela, e com ela o katakana que a \
         direção 1c abandonou"
    );
    assert!(
        tela.contains("marca-simbolo.svg"),
        "a entrada não desenha o símbolo da marca; a coluna da esquerda é a \
         apresentação do produto e ela começa por ele"
    );
    assert!(
        tela.contains("boot-nome"),
        "o nome SEELE saiu da entrada, ou deixou de ser texto — e desenhado ele \
         volta a depender de um arquivo por tamanho"
    );

    // **A agulha é o termo aposentado, e não o novo.** Esta lista dizia
    // `["Entry Plug", "ENTRY_PLUG"]`, e a varredura de renomeação a reescreveu
    // para `["SEELE", …]` em 2026-08-25 — fazendo o guarda acusar o nome do
    // próprio produto, que obviamente está na página. É o terceiro guarda desta
    // sessão mordido do mesmo jeito, junto com as duas entradas de
    // `APOSENTADOS`: quem procura uma palavra que saiu não pode ser atualizado
    // para a que ficou.
    for citacao in ["Entry Plug", "ENTRY_PLUG"] {
        assert!(
            !pagina.contains(citacao),
            "«{citacao}» voltou à janela. É citação direta do anime, e as três \
             desta tela saíram juntas com o katakana da assinatura"
        );
    }

    // E o nome tem de sair na face embarcada, não numa qualquer: a folha diz
    // Saira Condensed 900, e `--seele-display` é o nome dela nesta casa.
    let folha = without_comments(&read("ui/tela-boot.css"));
    let Some(depois) = folha.split(".boot-nome {").nth(1) else {
        panic!("`tela-boot.css` não pinta mais `.boot-nome`");
    };
    let Some(regra) = depois.split('}').next() else {
        panic!("a regra de `.boot-nome` nunca fecha");
    };
    assert!(
        regra.contains("var(--seele-display)") && regra.contains("900"),
        "o nome da marca não sai na Saira Condensed 900:\n{regra}"
    );
}

#[test]
fn onde_se_escolhe_um_servidor_explica_o_que_se_digita() {
    // **Os três campos saíram da entrada na 0.9.0**, e este guarda mudou de
    // endereço com eles em vez de sumir.
    //
    // O que ele protegia continua sendo o assunto: um campo sem nota é um campo
    // que a pessoa preenche adivinhando. O que mudou é onde eles moram —
    // `campo-convite` e `campo-servidor` viraram um só, no diálogo `ONDE VOCÊ
    // JÁ ESTEVE`, que aceita as duas formas; e o apelido foi para o perfil.
    //
    // O `placeholder` conta como a explicação de um campo que é uma linha só. O
    // que não pode existir é um campo sem nenhuma das duas.
    let pagina = without_comments(&read("ui/index.html"));

    for campo in ["servidores-endereco", "perfil-apelido"] {
        let Some(depois) = pagina.split(&format!("id=\"{campo}\"")).nth(1) else {
            panic!("sumiu o campo `{campo}`");
        };
        let Some(resto) = depois.split("</label>").next().or(Some(depois)) else {
            continue;
        };
        let tem_nota = resto.contains("class=\"nota\"");
        // O `placeholder` do próprio campo vem **antes** do fim da tag, então
        // procurá-lo no resto do rótulo não bastaria.
        let tem_exemplo = depois
            .split('>')
            .next()
            .is_some_and(|tag| tag.contains("placeholder="));
        assert!(
            tem_nota || tem_exemplo,
            "o campo `{campo}` não diz o que se escreve nele, nem por nota nem por exemplo"
        );
    }
}

// ------------------------------------------ o vocabulário que saiu da tela

/// As palavras que o mapa v3 tirou da interface, e o que cada uma virou.
///
/// A decisão é `docs/adr/0033`: a camada de linguagem temática sai do texto que
/// a pessoa lê, e o desenho fica. Uma decisão dessas não se mantém sozinha —
/// ela se desfaz uma tela de cada vez, porque a palavra antiga continua sendo a
/// que quem escreve o código tem na cabeça, e porque ela ainda está viva e certa
/// em todo `id=`, toda classe, todo nome de tipo e todo comentário aqui do lado.
/// É por isso que o guarda é por palavra e não por tela: uma tela nova entra
/// coberta sem que ninguém se lembre de vir aqui.
///
/// A busca é por **palavra inteira** e sem caixa: `linha` acusa `Linhas` e
/// ignora `alinhado`.
///
/// `sincronização` sozinha não está aqui: `TEMPO ESGOTADO NA SINCRONIZAÇÃO
/// INICIAL` é o aperto de mão, não a taxa, e o mapa não o cobre.
///
/// Os três nomes MAGI **estavam** fora deste mapa, com a ressalva de que o 0033
/// tirou as três luzes do rodapé e não os nomes, que seguiam no diagrama do
/// arranque. Deixaram de precisar de ressalva em 2026-08-24: `Casper`,
/// `Melchior` e `Balthasar` foram renomeados para `Persistence`, `Permissions` e
/// `Media` no código e na interface, então não há mais nome a acusar nem a
/// isentar.
const APOSENTADOS: &[(&str, &str)] = &[
    ("server", "servidor"),
    // Mesma regra do aviso alguns itens abaixo, e a varredura de `Cage` as
    // mordeu do mesmo jeito em 2026-08-25: a esquerda é o nome **aposentado**,
    // e reescrevê-la para `voice room` fazia o guarda procurar a palavra nova.
    ("cage", "sala de voz"),
    ("cages", "salas de voz"),
    ("jaula", "sala de voz"),
    ("linha", "canal"),
    ("linhas", "canais"),
    // **Não reescrever estas duas para «pessoa».** Elas são o registro do termo
    // que saiu, e a coluna da esquerda tem de continuar dizendo o nome velho —
    // é ele que o guarda procura. Uma varredura de renomeação as transformou em
    // `("pessoa", "pessoa")` em 2026-08-24, e o guarda passou a acusar todo uso
    // legítimo da palavra nova contra ela mesma.
    ("piloto", "pessoa"),
    ("pilotos", "pessoas"),
    // `plug`, e não `connection`: pela terceira vez, a varredura trocou o nome
    // aposentado pelo novo nesta coluna. Ver o aviso mais abaixo.
    ("plug", "conectar / sair"),
    ("ejetar", "sair"),
    ("ejeção", "saída"),
    ("a.t. field", "mudo"),
    ("taxa de sincronização", "sinal"),
    ("sync", "sinal"),
    ("padrão: azul", "conexão segura"),
    ("padrão: laranja", "conexão não verificada"),
    ("padrão: desligado", "sem conexão"),
];

/// As frases que ainda carregam uma palavra aposentada, e por que ficam.
///
/// Cada uma sai do texto antes da busca, e cada uma é uma dívida com dono, não
/// uma licença: some daqui no dia em que a frase mudar. Vazia é o estado que se
/// quer.
const AINDA_NA_TELA: &[(&str, &str)] = &[
    (
        "Terminal server",
        "o nome da tela de ajustes locais. `SERVER → SERVIDOR` descreveria errado \
         uma tela cuja própria subtitulação diz «Ajustes deste computador»: aqui \
         `Server` é o nome do lugar, e não a palavra para servidor. A subtitulação \
         perdeu a segunda metade — «e não deste servidor» — quando a seção do \
         servidor entrou, e o argumento não depende dela: quatro das cinco seções \
         continuam sendo desta máquina, e a quinta se anuncia. O mapa não tem \
         linha para o composto e quem coordena ainda não decidiu.",
    ),
    (
        "Clicar numa linha preenche tudo e conecta.",
        "a nota sob a lista de visitados. Aqui `linha` é a fila da lista que se \
         clica, e não o canal de texto que o mapa aposentou — a mesma palavra \
         para duas coisas, e o guarda só sabe ler a palavra. A frase foi pedida \
         nestes termos para esta tela; some daqui no dia em que ela mudar.",
    ),
];

/// Os atributos que carregam frase, e não identificador.
///
/// `value` está aqui e não é engano: o que um `<input>` traz escrito é a
/// primeira palavra que a pessoa lê no campo, e a que ela manda adiante se não
/// mexer nele.
const ATRIBUTOS_VISIVEIS: &[&str] = &[
    "alt",
    "aria-description",
    "aria-label",
    "aria-roledescription",
    "aria-valuetext",
    "data-anuncio",
    "data-sub",
    "data-titulo",
    "placeholder",
    "title",
    "value",
];

/// O texto que uma marcação mostra: nó de texto e atributo de frase.
fn texto_de_marcacao(pagina: &str) -> Vec<String> {
    let pagina = without_comments(pagina);
    let mut achado = Vec::new();
    for atributo in ATRIBUTOS_VISIVEIS {
        // Com o espaço na frente, para que ` title="` não seja encontrado
        // dentro de um atributo cujo nome termina igual.
        let agulha = format!(" {atributo}=\"");
        for pedaco in pagina.split(&agulha).skip(1) {
            if let Some(valor) = pedaco.split('"').next() {
                achado.push(valor.to_owned());
            }
        }
    }
    for pedaco in pagina.split('>').skip(1) {
        let Some(texto) = pedaco.split('<').next() else {
            continue;
        };
        if !texto.trim().is_empty() {
            achado.push(texto.trim().to_owned());
        }
    }
    achado
}

/// Corta os argumentos de uma chamada, mantendo do `manter`-ésimo em diante.
///
/// Serve às duas posições deste frontend em que uma string com espaço dentro é
/// identificador e não frase, e onde nenhuma regra de forma as separa:
///
/// - `console.warn("leave_voice_room:", falha)` — nada que sai por aqui chega a uma
///   tela, então a chamada inteira cai (`manter` 0);
/// - `elemento(tag, classe, texto)` — a **lista de classes** é a única string de
///   identificador desta casa que traz espaço (`"voice_room aberto"`), e é o segundo
///   argumento; o terceiro é o texto e fica (`manter` 2).
///
/// O nome é casado com o `(` através dos caracteres de identificador que houver
/// entre os dois, que é o que faz `console.` alcançar `console.warn(` sem uma
/// lista de métodos aqui dentro. Um `elemento` que não seja chamada — a palavra
/// solta numa lista de argumentos — não tem `(` adiante e é copiado inteiro.
fn sem_argumentos(fonte: &str, nome: &str, manter: usize) -> String {
    let letras: Vec<char> = fonte.chars().collect();
    let alvo: Vec<char> = nome.chars().collect();
    let mut saida = String::with_capacity(fonte.len());
    let mut i = 0usize;

    while i < letras.len() {
        if !letras[i..].starts_with(alvo.as_slice()) {
            saida.push(letras[i]);
            i += 1;
            continue;
        }
        // Do fim do nome até o `(`, só pode haver mais identificador.
        let mut abre = i + alvo.len();
        while abre < letras.len() && (letras[abre].is_ascii_alphanumeric() || letras[abre] == '_') {
            abre += 1;
        }
        if abre >= letras.len() || letras[abre] != '(' {
            saida.push(letras[i]);
            i += 1;
            continue;
        }

        let mut j = abre + 1;
        let mut profundidade = 1usize;
        let mut virgulas = 0usize;
        let mut aspas: Option<char> = None;
        while j < letras.len() {
            let letra = letras[j];
            if let Some(fecha) = aspas {
                if letra == '\\' {
                    j += 2;
                    continue;
                }
                if letra == fecha {
                    aspas = None;
                }
                j += 1;
                continue;
            }
            match letra {
                '"' | '\'' | '`' => aspas = Some(letra),
                '(' | '[' | '{' => profundidade += 1,
                ')' | ']' | '}' => {
                    profundidade -= 1;
                    if profundidade == 0 {
                        j += 1;
                        break;
                    }
                }
                ',' if profundidade == 1 => {
                    // A `manter`-ésima vírgula de topo abre o argumento que
                    // sobrevive: o corte para nela, e o resto da chamada é
                    // copiado como qualquer outro trecho.
                    virgulas += 1;
                    if manter > 0 && virgulas >= manter {
                        j += 1;
                        break;
                    }
                }
                _ => {}
            }
            j += 1;
        }
        i = j;
    }
    saida
}

/// Uma string de código que é identificador, e não frase.
///
/// Todo `id=`, toda classe, todo nome de comando e todo nome de evento deste
/// frontend tem a mesma forma: ASCII minúsculo, sem espaço e sem acento, ligado
/// por `-`, `_` ou `.`. Um seletor traz `#`, `.` ou `[` junto. Frase deste
/// produto tem espaço, acento ou caixa alta, e nenhuma das formas abaixo tem
/// nenhum dos três.
///
/// O custo aceito, dito por extenso: uma palavra solta, ASCII e minúscula
/// escrita na tela por um script — `no.textContent = "pessoa"` — passa por
/// identificador e escapa daqui. Aceito porque a alternativa acusa todo
/// `invoke("apagar_voice_room")` da janela, e porque a marcação, onde essas palavras
/// de fato moram, é lida sem este filtro.
fn parece_identificador(literal: &str) -> bool {
    let texto = literal.trim();
    if texto.is_empty() {
        return true;
    }
    if texto.starts_with('.') || texto.starts_with('#') || texto.contains('[') {
        return true;
    }
    if texto
        .chars()
        .all(|letra| letra.is_ascii_lowercase() || letra.is_ascii_digit() || "-_.".contains(letra))
    {
        return true;
    }
    texto.chars().all(|letra| letra.is_ascii_alphanumeric())
        && texto.starts_with(|letra: char| letra.is_ascii_lowercase())
}

/// O texto que um script escreve na tela.
fn texto_de_script(script: &str) -> Vec<String> {
    let limpo = without_comments(script);
    let limpo = sem_argumentos(&limpo, "console.", 0);
    let limpo = sem_argumentos(&limpo, "elemento", 2);

    let letras: Vec<char> = limpo.chars().collect();
    let mut achado = Vec::new();
    let mut i = 0usize;
    while i < letras.len() {
        let abre = letras[i];
        if !matches!(abre, '"' | '\'' | '`') {
            i += 1;
            continue;
        }
        i += 1;
        let mut texto = String::new();
        while i < letras.len() {
            let letra = letras[i];
            if letra == '\\' {
                if let Some(fugida) = letras.get(i + 1) {
                    texto.push(if *fugida == 'n' { '\n' } else { *fugida });
                }
                i += 2;
                continue;
            }
            if letra == abre {
                i += 1;
                break;
            }
            // `${…}` é o buraco onde entra um nome, e não texto: some, deixando
            // o espaço para que as palavras dos dois lados não se colem numa só.
            if abre == '`' && letra == '$' && letras.get(i + 1) == Some(&'{') {
                let mut profundidade = 0usize;
                while i < letras.len() {
                    match letras[i] {
                        '{' => profundidade += 1,
                        '}' => {
                            profundidade -= 1;
                            if profundidade == 0 {
                                i += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
                texto.push(' ');
                continue;
            }
            texto.push(letra);
            i += 1;
        }
        if !parece_identificador(&texto) {
            achado.push(texto);
        }
    }
    achado
}

/// O texto que uma folha escreve: só o que `content:` desenha.
fn texto_de_folha(css: &str) -> Vec<String> {
    let limpo = without_comments(css);
    let mut achado = Vec::new();
    for pedaco in limpo.split("content:").skip(1) {
        let Some(depois) = pedaco.split(';').next() else {
            continue;
        };
        for (indice, entre_aspas) in depois.split('"').enumerate() {
            if indice % 2 == 1 && !entre_aspas.trim().is_empty() {
                achado.push(entre_aspas.to_owned());
            }
        }
    }
    achado
}

/// Tudo em `ui/` que uma pessoa lê, como (arquivo, texto).
fn textos_visiveis() -> Vec<(String, String)> {
    let mut achado = Vec::new();
    let mut colher = |arquivo: &str, textos: Vec<String>| {
        for texto in textos {
            achado.push((arquivo.to_owned(), texto));
        }
    };
    for nome in ui_files(".html") {
        colher(&nome, texto_de_marcacao(&read(&format!("ui/{nome}"))));
    }
    for nome in ui_files(".svg") {
        colher(&nome, texto_de_marcacao(&read(&format!("ui/{nome}"))));
    }
    for nome in ui_files(".js") {
        colher(&nome, texto_de_script(&read(&format!("ui/{nome}"))));
    }
    for nome in ui_files(".css") {
        colher(&nome, texto_de_folha(&read(&format!("ui/{nome}"))));
    }
    achado
}

/// Onde `palavra` aparece em `texto` como palavra inteira. Os dois em minúscula.
fn palavra_em(texto: &str, palavra: &str) -> bool {
    let fronteira =
        |letra: Option<char>| !letra.is_some_and(|letra| letra.is_alphanumeric() || letra == '_');
    let mut de = 0usize;
    while let Some(at) = texto[de..].find(palavra) {
        let inicio = de + at;
        let fim = inicio + palavra.len();
        if fronteira(texto[..inicio].chars().next_back()) && fronteira(texto[fim..].chars().next())
        {
            return true;
        }
        de = inicio + texto[inicio..].chars().next().map_or(1, char::len_utf8);
    }
    false
}

/// Japonês decorativo: kana e kanji.
fn japones(letra: char) -> bool {
    matches!(letra, '\u{3040}'..='\u{30FF}' | '\u{4E00}'..='\u{9FFF}')
}

#[test]
fn nenhum_termo_aposentado_volta_para_o_texto_da_interface() {
    let textos = textos_visiveis();

    // Um extrator que parou de achar texto fica verde para sempre, e um guarda
    // que não pode falhar não é guarda. Estas cinco são as palavras que o mapa
    // pôs no lugar das aposentadas: se nenhuma delas chega até aqui, quem está
    // verde é a leitura, e não a interface.
    for nova in ["servidor", "sala de voz", "canal", "apelido", "sinal"] {
        assert!(
            textos
                .iter()
                .any(|(_, texto)| texto.to_lowercase().contains(nova)),
            "nenhum texto de `ui/` diz «{nova}». Ou o vocabulário novo sumiu da \
             janela, ou este guarda parou de enxergar o texto dela — e a segunda \
             hipótese é a que o deixa verde calado"
        );
    }

    let mut achados: Vec<String> = Vec::new();
    for (arquivo, texto) in &textos {
        let mut procurado = texto.to_lowercase();
        for (frase, _) in AINDA_NA_TELA {
            procurado = procurado.replace(&frase.to_lowercase(), " ");
        }
        for (palavra, virou) in APOSENTADOS {
            if palavra_em(&procurado, palavra) {
                achados.push(format!(
                    "{arquivo}: «{texto}» ainda diz «{palavra}», que o mapa v3 \
                     trocou por «{virou}»"
                ));
            }
        }
        if texto.chars().any(japones) {
            achados.push(format!(
                "{arquivo}: «{texto}» ainda traz japonês decorativo, que o mapa \
                 v3 tirou da interface inteira"
            ));
        }
    }

    assert!(
        achados.is_empty(),
        "o vocabulário que o ADR 0033 tirou da interface voltou ao texto que se \
         lê nela:\n{}\n\nSe uma destas é `id=`, classe, nome de arquivo ou \
         comentário, o defeito é deste guarda e é aqui que se conserta. Se é \
         mesmo texto de tela e mesmo assim tem de ficar, ela entra em \
         AINDA_NA_TELA com o motivo escrito — e não some daqui calada.",
        achados.join("\n")
    );
}

#[test]
fn toda_camada_fecha_apertando_fora_dela() {
    // O `Escape` sozinho não bastava, e a razão é de quem usa: ele está longe
    // da mão que acabou de clicar, e quem nunca leu a documentação não tem por
    // que saber que ele fecha. Apertar fora é o gesto que todo mundo tenta
    // primeiro, e era o que não acontecia.
    //
    // Cobrado sobre **todas** as camadas e não sobre a lista de hoje: uma
    // quinta escrita amanhã tem de nascer com isto, e a forma de garantir é o
    // teste descobrir sozinho quais existem.
    let page = read("ui/index.html");
    let scripts = scripts();

    let mut camadas: Vec<String> = Vec::new();
    let mut resto = page.as_str();
    while let Some(inicio) = resto.find("role=\"dialog\"") {
        let antes = &resto[..inicio];
        let Some(marca) = antes.rfind("id=\"") else {
            break;
        };
        let depois = &antes[marca + 4..];
        let Some(fim) = depois.find('"') else { break };
        camadas.push(depois[..fim].to_owned());
        resto = &resto[inicio + 1..];
    }

    assert!(
        camadas.len() >= 4,
        "esperava pelo menos as quatro camadas conhecidas e achei {camadas:?}; \
         ou o `role=\"dialog\"` mudou de forma e este guarda ficou cego"
    );

    for camada in &camadas {
        let pedido = format!("fecharAoClicarFora(\"{camada}\"");
        assert!(
            scripts.contains(&pedido),
            "a camada «{camada}» não fecha apertando fora dela: quem abriu por \
             engano fica preso a uma tecla que ninguém lhe ensinou"
        );
    }
}

#[test]
fn nenhum_convite_sobrevive_a_troca_de_servidor() {
    // O guarda anterior cobrava um `limparConvite()` em cada caminho que
    // trocasse de servidor: o convite ficava guardado num campo da entrada, e
    // um convite velho aplicado a um servidor novo é uma credencial mandada
    // para quem não a emitiu.
    //
    // **A 0.9.0 tirou o problema em vez de o cobrir.** Não há mais campo de
    // convite nem convite guardado: o `seele://` é resolvido no instante em que
    // se aperta ENTRAR, no diálogo de servidores, e o token vive dentro daquela
    // chamada. Não há o que sobreviver a uma troca.
    //
    // O que se cobra passa a ser a ausência: se um convite voltar a ser
    // guardado entre conexões, este guarda reprova e quem o fizer lê por quê.
    let script = without_comments(&scripts());

    assert!(
        !script.contains("convitePendente"),
        "voltou a haver um convite guardado entre conexões; se ele precisa \
         existir, precisa também ser largado em todo caminho que troca de \
         servidor — que é o que este guarda cobrava antes de a 0.9.0 tirar o campo"
    );
}

#[test]
fn the_decoder_profile_is_read_from_the_stream_and_never_spelled_out() {
    // **This guard exists because the bug already happened, in the field.**
    //
    // `palco-imagem.js` used to configure the `VideoDecoder` with a literal
    // `avc1.42e0…` — `42` being `profile_idc` 66, Baseline — under a comment
    // that said, in so many words, «the profile is always baseline, because
    // `codec.rs` picks CAVLC precisely so OpenH264 does not go up to High».
    //
    // That was true when it was written. The commit that adopted CABAC made it
    // false the same day: **CABAC does not exist in Baseline**, so the encoder
    // moved to High (`profile_idc` 100) and this file was never told. The
    // `examples/perfil.rs` prints both, side by side.
    //
    // The failure was the worst kind. A hardware decoder accepts a config that
    // lies to it, draws for a while, and dies when it meets what it was not
    // armed to read. What reached the person sharing was «it worked and then
    // it stopped».
    //
    // The lesson is not «fix the number» — a corrected literal would rot on the
    // next encoder change exactly like the first one did. It is that **one side
    // must not declare what the other side decides.** The SPS rides in every
    // keyframe and already carries profile, constraints and level; reading them
    // is the only version of this that cannot age.
    let script = without_comments(&scripts());

    // Any `avc1.` followed by a hex digit is a profile someone wrote by hand.
    // Inside a template string the profile is the part *before* any `${`, so a
    // literal prefix is exactly what this catches.
    for trecho in script.split("avc1.").skip(1) {
        let seguinte = trecho.chars().next().unwrap_or(' ');
        assert!(
            !seguinte.is_ascii_hexdigit(),
            "a codec string voltou a trazer o perfil escrito à mão (`avc1.{seguinte}…`). \
             O perfil vem do SPS: ver `codecDoSps` e `examples/perfil.rs`."
        );
    }

    // And the reader has to still be there. Deleting it and going back to a
    // literal would pass the check above only if the literal were gone too —
    // this is what makes the pair a guard instead of half of one.
    assert!(
        script.contains("codecDoSps"),
        "sumiu quem lê o perfil do SPS; sem ele a configuração volta a ser palpite"
    );
}

#[test]
fn a_nota_que_promete_o_enter_tem_quem_a_cumpra() {
    // A barra de compor traz uma nota escrita: «Enter também envia». É uma
    // promessa, e até agosto de 2026 ela não tinha dono — quem a cumpria era o
    // *envio implícito* do navegador, o comportamento que manda um `<form>`
    // sozinho quando ele tem um campo de texto e um botão `type="submit"`.
    //
    // Aquilo funciona e tem pré-condições, e as pré-condições são sobre a forma
    // do formulário: exatamente um campo de texto, e um botão de submissão
    // habilitado ou nenhum. Um campo a mais nesta barra apaga a promessa sem
    // tocar na frase que a faz.
    //
    // **Nunca houve defeito aqui.** Uma versão anterior deste comentário dizia
    // que Enter não mandava em campo; era leitura errada de um pedido que era
    // outro — tirar o botão ENVIAR, que a 0.9.0 vai tirar. Fica escrito porque
    // quem ler este guarda tem de saber que ele não guarda uma cicatriz.
    //
    // O que ele prende é que a frase e o código que a cumpre existam **juntos**,
    // para que apagar um deles não deixe o outro mentindo sozinho na tela — e
    // isso passa a valer de verdade no dia em que o botão sair.
    let page = without_comments(&read("ui/index.html"));
    let script = without_comments(&scripts());

    // **A promessa mudou de lugar na 0.9.0**, junto com o botão que ela
    // explicava. Era uma nota ao lado do campo — `Enter também envia` —, e
    // virou parte do `placeholder`: `transmitir no canal — Enter envia, Ctrl+V
    // cola uma imagem`. O lugar é melhor, e é o único que quem está prestes a
    // digitar está olhando.
    let promete = page.contains("Enter envia");
    let cumpre = script.contains("campo-mensagem") && script.contains("evento.key !== \"Enter\"");

    assert_eq!(
        promete,
        cumpre,
        "a tela {} e o script {} — a promessa e quem a cumpre andam juntas",
        if promete {
            "promete Enter"
        } else {
            "não promete Enter"
        },
        if cumpre {
            "trata Enter"
        } else {
            "não trata Enter"
        }
    );
}

/// Uma classe desenhada duas vezes tem de estar declarada como refinamento.
///
/// **O defeito que este guarda registra custou uma versão em campo.** A marca
/// da entrada saiu empilhada e centrada na 0.9.0, e ninguém tinha escrito isso:
/// `base.css` definia `.boot-marca` com `flex-direction: column` para o cartão
/// que a entrada e o fim dividiam, e `tela-boot.css` acrescentou por cima um
/// `align-items: center` sem zerar a direção. Mesma especificidade, folhas
/// diferentes: o navegador não reporta nada, e o que vale é a **soma** — coluna
/// da primeira, centro da segunda. O desenho resultante não estava em folha
/// nenhuma.
///
/// A forma da falha é escrever a diferença em vez do valor. Uma regra que só
/// corrige a de cima depende dela para estar certa, e passa a quebrar quando a
/// de cima muda por outro motivo — que é o acoplamento mais caro que existe
/// numa folha de estilo, porque só aparece na tela de quem está usando.
///
/// Refinar continua sendo legítimo: `.rotulo` e `.ausente` ganham em
/// `acessibilidade.css` o que só vale sob `prefers-reduced-motion` e afins, e
/// `.erro` ganha na entrada a largura que só faz sentido dentro do cartão. O que
/// este guarda cobra é que o refinamento seja **declarado**, e não descoberto.
#[test]
fn no_class_is_drawn_by_two_stylesheets_without_being_a_declared_refinement() {
    // Refinamentos legítimos: a classe é definida em `base.css` e uma segunda
    // folha acrescenta o que só vale no contexto dela. Cada entrada carrega o
    // motivo, e acrescentar uma aqui é a decisão que este guarda quer forçar a
    // ser tomada de propósito.
    const REFINAMENTOS: &[(&str, &str)] = &[
        (".rotulo", "acessibilidade.css"),
        (".ausente", "acessibilidade.css"),
        (".erro", "tela-boot.css"),
    ];

    let folhas: Vec<(String, String)> = ui_files(".css")
        .into_iter()
        // `tokens.css` é cópia congelada do design (ADR 0014) e não define classe.
        .filter(|nome| nome != "tokens.css")
        .map(|nome| {
            let texto = without_comments(&read(&format!("ui/{nome}")));
            (nome, texto)
        })
        .collect();

    let mut onde: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for (nome, texto) in &folhas {
        // A profundidade de chaves, porque **só o topo conta**. Uma regra
        // dentro de um `@media` é a mesma classe dita de novo de propósito,
        // sob uma condição — é assim que `.boot-cursor` para de piscar sob
        // `prefers-reduced-motion`, e acusá-la seria acusar exatamente o que a
        // folha deve fazer.
        let mut fundura = 0i32;
        for linha in texto.lines() {
            let corte = linha.trim_start();
            let fundura_da_linha = fundura;
            fundura += i32::try_from(linha.matches('{').count()).unwrap_or(0);
            fundura -= i32::try_from(linha.matches('}').count()).unwrap_or(0);
            if fundura_da_linha != 0 {
                continue;
            }
            let Some(resto) = corte.strip_prefix('.') else {
                continue;
            };
            // Só a classe **sozinha** abrindo a regra: `.classe {`.
            //
            // Um seletor agrupado — `.voice_room, .linha { border-left: … }` —
            // fica de fora de propósito. Ele não é a mesma classe dita duas
            // vezes: é a folha dizendo que dois itens dividem um traço, uma vez
            // só, e desmembrá-lo em duas regras é que criaria a cópia. Um
            // seletor composto (`.boot-marca.fim`) também fica: a
            // especificidade maior é uma decisão explícita, não um empate.
            let fim = resto
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
                .unwrap_or(resto.len());
            let (classe, cauda) = resto.split_at(fim);
            if classe.is_empty() || !cauda.trim_start().starts_with('{') {
                continue;
            }
            // O nome da folha entra **uma vez por definição**, e não uma vez
            // por folha: a mesma classe escrita duas vezes no mesmo arquivo é o
            // mesmo defeito, e foi assim que `.operador-quem` apareceu — a
            // primeira regra dizia `flex-direction: column`, a segunda dizia
            // `align-items: center` e não zerava a direção, e o avatar subiu
            // para cima do nome que devia estar ao lado dele.
            onde.entry(format!(".{classe}"))
                .or_default()
                .push(nome.clone());
        }
    }

    let mut acusadas = Vec::new();
    for (classe, folhas_da_classe) in &onde {
        if folhas_da_classe.len() < 2 {
            continue;
        }
        // Uma folha citada duas vezes é uma classe definida duas vezes nela, e
        // isso não tem refinamento que valha: é o mesmo arquivo brigando
        // consigo mesmo, e a correção é sempre escrever uma regra só.
        let repetida_no_mesmo_arquivo = {
            let mut vistas = std::collections::BTreeSet::new();
            !folhas_da_classe.iter().all(|folha| vistas.insert(folha))
        };
        let declarado = !repetida_no_mesmo_arquivo
            && folhas_da_classe.iter().all(|folha| {
                folha == "base.css" || REFINAMENTOS.iter().any(|(c, f)| c == classe && f == folha)
            });
        if !declarado {
            acusadas.push(format!("{classe} em {folhas_da_classe:?}"));
        }
    }

    assert!(
        acusadas.is_empty(),
        "estas classes são desenhadas por mais de uma folha sem estarem \
         declaradas como refinamento, e o que vale na tela é a soma das regras \
         e não a intenção de nenhuma delas:\n  {}\n\nOu a segunda folha escreve \
         o valor inteiro e a primeira deixa de definir a classe, ou o par entra \
         em REFINAMENTOS com o motivo escrito.",
        acusadas.join("\n  ")
    );
}

/// Hospedar entra direto; conectar a outro servidor nunca entra direto.
///
/// A `#tela-auth` existe para o momento TOFU do ADR 0003: quem chega a um
/// servidor de outra pessoa olha a impressão digital antes de entrar, e é esse
/// olhar que detecta alguém no meio do caminho. Hospedando não há meio do
/// caminho — o certificado foi gerado por este mesmo processo, nesta máquina,
/// segundos antes.
///
/// O que este guarda protege é o **outro** lado da dispensa. Uma tentativa de
/// hospedar que falha não pode dispensar a conferência da conexão seguinte, que
/// essa vai a um servidor alheio de verdade.
///
/// **A dispensa deixou de ser um sim e passou a ser um endereço** — A05 da
/// auditoria de 17/09. A bandeira booleana era lida e apagada na primeira linha
/// de `conectar`, e isso confundia «a tentativa falhou» com «a mesma tentativa
/// está continuando»: uma entrada que para no aceite de MODs é retomada por
/// `aceitarOsMods`, e a intenção já tinha sido consumida — quem hospedava caía
/// na tela de conferir a própria chave.
///
/// Guardar o endereço resolve as duas de uma vez, e por construção: continuar
/// para o mesmo alvo reconhece a intenção, e **qualquer outro alvo não casa**,
/// então não há o que vazar para a conexão seguinte. É uma propriedade mais
/// forte que a de antes, e não um afrouxamento dela.
#[test]
fn the_flag_that_skips_the_key_check_is_cleared_before_the_attempt_and_not_after() {
    let boot = read("ui/tela-boot.js");
    let conectar = js_function(&boot, "async function conectar(");

    // A dispensa é uma comparação com o alvo desta conexão, e não a leitura de
    // um sim. Sem o `=== ultimoAlvo`, ela voltaria a valer em qualquer endereço.
    assert!(
        conectar.contains("nossoServidorEm === ultimoAlvo"),
        "`conectar` não compara a dispensa com o alvo desta conexão, então ela \
         valeria para qualquer endereço:\n{conectar}"
    );
    assert!(
        !boot.contains("nossoServidorEm = true"),
        "a dispensa voltou a ser um sim solto; ela tem de ser o endereço que \
         esta janela hospeda, ou volta a vazar para a conexão seguinte"
    );

    // E quem a liga é só o hospedar, com o endereço que acabou de subir.
    let ligam =
        boot.matches("nossoServidorEm = ").count() - boot.matches("nossoServidorEm = null").count();
    assert_eq!(
        ligam, 1,
        "a dispensa da conferência de chave é escrita em {ligam} lugares; só \
         `hospedar` pode escrevê-la"
    );
    let hospedar = js_function(&boot, "async function hospedar(");
    assert!(
        hospedar.contains("nossoServidorEm = anfitriao.aqui"),
        "`hospedar` não prende a dispensa ao endereço que acabou de subir: {hospedar}"
    );

    // **E ela morre com a hospedagem.** Um endereço de rede local volta a ser de
    // outra máquina no dia seguinte: mantê-la depois de o servidor cair seria
    // dispensar a conferência justamente onde ela passa a valer.
    let fim = read("ui/tela-fim.js");
    assert!(
        fim.contains("nossoServidorEm = null"),
        "nada zera a dispensa quando a hospedagem cai, e o mesmo endereço na mão \
         de outra máquina herdaria a dispensa"
    );

    // **E a bandeira tem de decidir alguma coisa do outro lado.**
    //
    // Ela chegava a `entrarNaAutenticacao`, era combinada num `direto` — e o
    // `if` continuava testando só o `liberado` de antes. O valor calculado e
    // nunca lido: nada quebra, nada avisa, e hospedar continuou pedindo para
    // conferir a própria chave por mais uma versão. Foi relatado em campo duas
    // vezes, a segunda depois de eu dizer que estava consertado.
    let auth = read("ui/tela-auth.js");
    let entrar = js_function(&auth, "function entrarNaAutenticacao(");
    assert!(
        entrar.contains("nossoServidor"),
        "`entrarNaAutenticacao` não recebe a bandeira: {entrar}"
    );
    let Some((antes_do_ramo, _)) = entrar.split_once("if (direto)") else {
        panic!(
            "nada em `entrarNaAutenticacao` ramifica por `direto`, então a \
             bandeira é calculada e nunca lida: {entrar}"
        );
    };
    assert!(
        antes_do_ramo.contains("const direto"),
        "`direto` é usado antes de ser definido: {entrar}"
    );
    assert!(
        antes_do_ramo.contains("nossoServidor"),
        "`direto` não leva a bandeira em conta, então hospedar continua \
         passando pela conferência: {entrar}"
    );
}

/// A frase de uma falha desconhecida lê `Error` antes de tentar `JSON.stringify`.
///
/// **Custou um ciclo inteiro de diagnóstico.** `JSON.stringify` de um `Error` ou
/// de uma `DOMException` devolve `{}`: `name` e `message` são propriedades do
/// protótipo, e o `stringify` só enumera as próprias. Toda falha de vídeo, de
/// área de transferência ou de mídia chegava à tela como duas chaves e nada
/// dentro — o pior detalhe possível, porque parece que o app tem a informação e
/// escolheu não a mostrar.
///
/// O relato de campo foi «o erro de compartilhar tela trouxe {}», e aquele `{}`
/// era exatamente a frase que diria por que o compartilhamento entre duas
/// máquinas Windows parou de funcionar.
#[test]
fn the_phrase_for_an_unknown_failure_reads_an_error_before_stringifying_it() {
    let frases = read("ui/frases.js");
    let desconhecida = js_function(&frases, "function desconhecida(");

    let Some((antes, _)) = desconhecida.split_once("JSON.stringify") else {
        panic!("`desconhecida` nem tenta serializar a falha: {desconhecida}");
    };
    assert!(
        antes.contains("instanceof Error") || antes.contains(".message"),
        "`desconhecida` chama `JSON.stringify` sem antes tratar `Error` e \
         `DOMException`, que serializam para `{{}}` — e a falha chega à tela \
         como duas chaves vazias:\n{desconhecida}"
    );
    assert!(
        desconhecida.contains("erro.name") || desconhecida.contains(".name,"),
        "o nome do erro não entra no detalhe, e ele é metade do diagnóstico: \
         `NotAllowedError` e `NotReadableError` mandam procurar coisas \
         diferentes:\n{desconhecida}"
    );
}

/// Dois scripts não declaram o mesmo nome no topo.
///
/// **Este guarda existe porque a sua ausência derrubou o aplicativo inteiro em
/// campo.** Os scripts desta janela são `<script src>` comuns, sem módulos: eles
/// dividem **um** escopo global. Um `let` num arquivo com o mesmo nome de uma
/// `function` de outro é `SyntaxError` — e não no ponto do conflito, mas no
/// arquivo inteiro, que deixa de carregar e leva junto tudo o que declarava.
///
/// Foi o que aconteceu com um `let hospedandoAqui` de `tela-boot.js` contra a
/// `async function hospedandoAqui()` de `tela-sessao.js`. O que a pessoa viu foi
/// «Can't find variable: desenhar» no Mac, «Cannot access 'comecoDaSessao'
/// before initialization» no Windows, o modal de perfil sem abrir e o botão
/// `CONECTAR` sem responder — quatro sintomas sem relação aparente, um nome
/// repetido. E os 154 guardas deste arquivo passaram, porque todos leem os
/// scripts como texto e nenhum os carregava junto.
///
/// Duas `function` homônimas não são erro para o navegador, e por isso são
/// piores: a que carrega depois vence, calada, e a outra vira código morto que
/// os testes continuam conferindo. `sairParaAEntrada` estava assim, e o `+` da
/// trilha rodava a função da tela de sessão encerrada — que não esconde a
/// sessão. Nome repetido é acusado do mesmo jeito, seja qual for a palavra-chave.
#[test]
fn no_two_scripts_declare_the_same_name_at_the_top_level() {
    let mut onde: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();

    for nome in ui_files(".js") {
        let texto = without_comments(&read(&format!("ui/{nome}")));
        // Profundidade zero: só o topo do arquivo é o escopo compartilhado.
        let mut fundura = 0i32;
        for linha in texto.lines() {
            let fundura_da_linha = fundura;
            fundura += i32::try_from(linha.matches('{').count()).unwrap_or(0);
            fundura -= i32::try_from(linha.matches('}').count()).unwrap_or(0);
            if fundura_da_linha != 0 {
                continue;
            }
            let corte = linha.trim_start_matches("async ");
            let Some(resto) = ["function ", "let ", "const ", "class ", "var "]
                .iter()
                .find_map(|chave| corte.strip_prefix(chave))
            else {
                continue;
            };
            // O identificador, e a linha tem de começar pela palavra-chave: um
            // `let` indentado está dentro de alguma coisa, e a fundura já o
            // pegaria — isto é o cinto do suspensório.
            if corte.len() != linha.len() && !linha.starts_with("async ") {
                continue;
            }
            let fim = resto
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '$')
                .unwrap_or(resto.len());
            let (identificador, _) = resto.split_at(fim);
            if identificador.is_empty() {
                continue;
            }
            let lista = onde.entry(identificador.to_owned()).or_default();
            if !lista.contains(&nome) {
                lista.push(nome.clone());
            }
        }
    }

    let repetidos: Vec<String> = onde
        .iter()
        .filter(|(_, arquivos)| arquivos.len() > 1)
        .map(|(identificador, arquivos)| format!("`{identificador}` em {arquivos:?}"))
        .collect();

    assert!(
        repetidos.is_empty(),
        "estes nomes são declarados no topo de mais de um script, e os scripts \
         desta janela dividem um escopo global só:\n  {}\n\nCom `let`, `const` \
         ou `class` de um lado isto é `SyntaxError` no arquivo inteiro, que \
         deixa de carregar. Com duas `function` é pior: o navegador aceita, a \
         que carrega depois vence em silêncio, e a outra vira código morto. \
         Renomeie a que for mais específica.",
        repetidos.join("\n  ")
    );
}

/// Ninguém chama uma função que nenhum script declara.
///
/// **Esta classe de defeito já entregou duas versões quebradas.** Quando uma
/// tela sai, as funções dela saem junto — e as chamadas ficam. Ninguém reclama:
/// o navegador só descobre o buraco quando a linha roda, e ela pode ser a linha
/// de um caminho que ninguém percorre no dia do teste.
///
/// Duas custaram caro:
///
/// - `desenharVisitados()` sobreviveu solto no **topo** de `tela-boot.js`. Uma
///   chamada de topo que estoura mata o resto do script: o `click` do
///   `CONECTAR` era registrado depois dela e nunca chegou a existir. O botão
///   ficava na tela sem responder a nada.
/// - `registrarEventoDaChamada()` sobreviveu **uma linha depois** de o
///   `compartilhar_tela` do Rust ter dado certo. A transmissão começava, o
///   `ReferenceError` caía no `catch` logo abaixo, e as duas linhas seguintes —
///   fechar a caixa e abrir a chamada — nunca rodavam. Na tela, «não funciona»,
///   com um `{}` de detalhe, porque é isso que `JSON.stringify` de um
///   `ReferenceError` devolve.
///
/// O guarda é grosseiro de propósito: ele não sabe escopo, então aceita como
/// declarado qualquer nome que apareça como declaração, parâmetro ou
/// desestruturação em **qualquer** script. Um falso negativo é o preço; o que
/// ele nunca deixa passar é um nome que não existe em lugar nenhum.
#[test]
fn no_script_calls_a_function_that_no_script_declares() {
    /// Fora comentários e o conteúdo de literais de texto: um `foo(` dentro de
    /// uma frase não é uma chamada, e era de onde vinha todo o falso positivo.
    fn sem_texto(bruto: &str) -> String {
        let sem_comentario = without_comments(bruto);
        let mut saida = String::with_capacity(sem_comentario.len());
        let mut aspas: Option<char> = None;
        let mut escapando = false;
        // O último caractere que não era espaço, para saber se uma `/` abre uma
        // expressão regular ou é divisão: depois de um valor ela divide, depois
        // de um operador ou de um abre-parêntese ela abre. Sem isto, o `Key(` de
        // `/^Key([A-Z])$/` conta como chamada — foi o que este guarda acusou na
        // primeira vez que rodou.
        let mut anterior = ' ';
        let mut em_classe = false;
        for c in sem_comentario.chars() {
            if aspas == Some('/') {
                if escapando {
                    escapando = false;
                } else if c == '\\' {
                    escapando = true;
                } else if c == '[' {
                    em_classe = true;
                } else if c == ']' {
                    em_classe = false;
                } else if c == '/' && !em_classe {
                    aspas = None;
                }
                saida.push(' ');
                continue;
            }
            if aspas.is_none()
                && c == '/'
                && matches!(
                    anterior,
                    '(' | ',' | '=' | ':' | '[' | '!' | '&' | '|' | '?' | '{' | ';' | ' '
                )
            {
                aspas = Some('/');
                em_classe = false;
                saida.push(' ');
                continue;
            }
            if !c.is_whitespace() {
                anterior = c;
            }
            match aspas {
                Some(fecha) => {
                    if escapando {
                        escapando = false;
                    } else if c == '\\' {
                        escapando = true;
                    } else if c == fecha {
                        aspas = None;
                        saida.push(' ');
                    }
                }
                None => {
                    if c == '"' || c == '\'' || c == '`' {
                        aspas = Some(c);
                        saida.push(' ');
                    } else {
                        saida.push(c);
                    }
                }
            }
        }
        saida
    }

    /// Todo identificador da linha, em ordem.
    fn nomes(linha: &str) -> Vec<String> {
        let mut achados = Vec::new();
        let mut atual = String::new();
        for c in linha.chars() {
            if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                atual.push(c);
            } else if !atual.is_empty() {
                achados.push(std::mem::take(&mut atual));
            }
        }
        if !atual.is_empty() {
            achados.push(atual);
        }
        achados
    }

    const PALAVRAS: &[&str] = &[
        "if",
        "for",
        "while",
        "switch",
        "catch",
        "return",
        "typeof",
        "await",
        "new",
        "delete",
        "void",
        "throw",
        "do",
        "else",
        "try",
        "finally",
        "of",
        "in",
        "instanceof",
        "yield",
        "async",
        "function",
        "super",
        "eval",
        "case",
        "default",
        "with",
    ];
    const GLOBAIS: &[&str] = &[
        "console",
        "window",
        "document",
        "navigator",
        "location",
        "fetch",
        "setTimeout",
        "setInterval",
        "clearTimeout",
        "clearInterval",
        "requestAnimationFrame",
        "cancelAnimationFrame",
        "queueMicrotask",
        "structuredClone",
        "btoa",
        "atob",
        "Object",
        "Array",
        "String",
        "Number",
        "Boolean",
        "Math",
        "JSON",
        "Date",
        "Promise",
        "Map",
        "Set",
        "WeakMap",
        "WeakSet",
        "Symbol",
        "Proxy",
        "Reflect",
        "BigInt",
        "RegExp",
        "Function",
        "Error",
        "TypeError",
        "RangeError",
        "URL",
        "URLSearchParams",
        "Blob",
        // ADR 0049: um MOD roda num `Worker`, e não na janela.
        "Worker",
        "File",
        "FileReader",
        "Image",
        "Uint8Array",
        "Uint8ClampedArray",
        "Int8Array",
        "Int16Array",
        "Uint16Array",
        "Int32Array",
        "Uint32Array",
        "Float32Array",
        "Float64Array",
        "ArrayBuffer",
        "DataView",
        "TextEncoder",
        "TextDecoder",
        "AbortController",
        "Event",
        "CustomEvent",
        "VideoDecoder",
        "VideoEncoder",
        "EncodedVideoChunk",
        "VideoFrame",
        "ImageData",
        "OffscreenCanvas",
        "MediaStream",
        "parseInt",
        "parseFloat",
        "isNaN",
        "isFinite",
        "encodeURIComponent",
        "decodeURIComponent",
        "encodeURI",
        "decodeURI",
        "alert",
        "confirm",
        "prompt",
        "Intl",
        "CSS",
        "matchMedia",
        "getComputedStyle",
        "DOMParser",
        "XMLHttpRequest",
        "WebSocket",
        "performance",
        "crypto",
        "localStorage",
        "sessionStorage",
        "IntersectionObserver",
        "ResizeObserver",
        "MutationObserver",
    ];

    let mut declarados: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut limpos: Vec<(String, String)> = Vec::new();

    for nome in ui_files(".js") {
        let texto = sem_texto(&read(&format!("ui/{nome}")));
        for linha in texto.lines() {
            let palavras = nomes(linha);
            for (i, palavra) in palavras.iter().enumerate() {
                // `function f`, `let x`, `const y`, `class C`, `var v`.
                if matches!(
                    palavra.as_str(),
                    "function" | "let" | "const" | "class" | "var"
                ) {
                    if let Some(seguinte) = palavras.get(i + 1) {
                        declarados.insert(seguinte.clone());
                    }
                }
            }
            // **Método**, de classe ou de objeto: `nome(args) {`, sem a
            // palavra `function` na frente. A primeira versão deste guarda não
            // os conhecia, e acusou `admite`, `encerrar` e `constructor` de não
            // existirem no dia em que a primeira classe entrou na interface.
            //
            // Distinguido de `if (...) {` pela lista de palavras de controle:
            // fora delas, um nome seguido de parênteses numa linha que abre
            // bloco é uma declaração.
            //
            // `async` e `static` vêm **antes** do nome, e o nome é o que
            // declara. Sem esta linha, `async iniciar(…) {` não era
            // reconhecido — e `iniciar` foi acusado de não existir no dia em
            // que o segundo executor entrou.
            {
                let cru = linha.trim();
                let cru = cru
                    .strip_prefix("async ")
                    .or_else(|| cru.strip_prefix("static "))
                    .unwrap_or(cru)
                    .trim();
                let primeira: String = cru
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                let controle = matches!(
                    primeira.as_str(),
                    "if" | "for" | "while" | "switch" | "catch" | "return" | "function" | "do"
                );
                if !primeira.is_empty()
                    && !controle
                    && cru.ends_with('{')
                    && cru[primeira.len()..].trim_start().starts_with('(')
                {
                    declarados.insert(primeira);
                }
            }
            // Parâmetros e desestruturação: tudo entre parênteses ou chaves
            // conta como nome que existe. Grosseiro, e é a troca aceita — o
            // guarda procura o nome que não existe em lugar nenhum.
            let mut dentro = false;
            let mut acumulado = String::new();
            for c in linha.chars() {
                match c {
                    '(' | '{' | '[' => {
                        dentro = true;
                        acumulado.clear();
                    }
                    ')' | '}' | ']' => {
                        if dentro {
                            for nome in nomes(&acumulado) {
                                declarados.insert(nome);
                            }
                        }
                        dentro = false;
                        acumulado.clear();
                    }
                    _ if dentro => acumulado.push(c),
                    _ => {}
                }
            }
            limpos.push((nome.clone(), linha.to_owned()));
        }
    }

    let mut fantasmas: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    for (arquivo, linha) in &limpos {
        let letras: Vec<char> = linha.chars().collect();
        let e_nome = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '$';
        let mut i = 0usize;
        while i < letras.len() {
            if !e_nome(letras[i]) {
                i += 1;
                continue;
            }
            // A corrida inteira de uma vez. Andar de um em um foi o defeito da
            // primeira versão deste guarda: ao pular o caractere depois de um
            // ponto, ela recomeçava **dentro** da palavra, e `console.warn(`
            // virava uma chamada a `arn()`.
            let de = i;
            while i < letras.len() && e_nome(letras[i]) {
                i += 1;
            }
            // Precedido de ponto é propriedade; precedido de dígito é número.
            let propriedade = de > 0 && (letras[de - 1] == '.' || letras[de - 1].is_ascii_digit());
            // Seguido de `(`, com espaços no meio ou não, é chamada.
            let mut j = i;
            while j < letras.len() && letras[j] == ' ' {
                j += 1;
            }
            if propriedade || j >= letras.len() || letras[j] != '(' {
                continue;
            }
            let nome: String = letras[de..i].iter().collect();
            let conhecido = declarados.contains(&nome)
                || PALAVRAS.contains(&nome.as_str())
                || GLOBAIS.contains(&nome.as_str());
            if !conhecido {
                fantasmas.entry(nome).or_default().insert(arquivo.clone());
            }
        }
    }

    let lista: Vec<String> = fantasmas
        .iter()
        .map(|(nome, arquivos)| format!("`{nome}()` chamada em {arquivos:?}"))
        .collect();
    assert!(
        lista.is_empty(),
        "estas funções são chamadas e nenhum script as declara:\n  {}\n\nUma \
         chamada assim estoura `ReferenceError` quando a linha roda — e se ela \
         estiver no topo de um script, leva junto tudo o que vinha depois. \
         Quando uma tela sai, as chamadas dela saem também.",
        lista.join("\n  ")
    );
}

/// O `remover` de uma mensagem some em repouso, e o teclado continua o achando.
///
/// A comp da 0.9.0 desenha a mensagem com avatar, autor, hora e corpo — e nada
/// mais. O `remover` sublinhado, repetido uma vez por mensagem, era uma coluna
/// de links descendo pelo lado direito de toda conversa, e o relato de campo o
/// pôs na mesma lista dos outros controles que a comp não desenha.
///
/// Ele não saiu: apagar a própria mensagem não pede permissão nenhuma, e apagar
/// a de outra pessoa é um verbo que a `specs/04` dá a quem tem a permissão.
/// Esconder um verbo é diferente de tirá-lo, e o que este guarda cobra é a
/// diferença.
///
/// **A metade que apodrece calada é a do teclado.** Um controle revelado só por
/// `:hover` é um controle que existe na ordem de tabulação e não aparece quando
/// o Tab chega nele — a pessoa aperta espaço num botão invisível. Sem o
/// `:focus-visible` isto seria acessibilidade trocada por estética, que é
/// exatamente o que a `specs/06-clientes-gui.md` recusa.
#[test]
fn the_remove_control_hides_at_rest_and_the_keyboard_still_finds_it() {
    let folha = without_comments(&read("ui/camada-moderar.css"));

    let Some(regra) = folha
        .split(".moderar-remover {")
        .nth(1)
        .and_then(|resto| resto.split('}').next())
    else {
        panic!("`camada-moderar.css` não desenha mais `.moderar-remover`");
    };
    assert!(
        regra.contains("visibility: hidden"),
        "o `remover` volta a ficar desenhado em toda mensagem, e a comp não o \
         desenha em nenhuma:\n{regra}"
    );
    // `visibility` e não `display`: o espaço fica reservado, e o texto da
    // mensagem não pula para o lado quando o controle aparece.
    assert!(
        !regra.contains("display: none"),
        "escondido por `display: none`, o controle tira o próprio espaço da \
         linha e o texto salta quando o ponteiro entra:\n{regra}"
    );

    // A regra que revela: procurada pelo que ela **faz**, e não pelo primeiro
    // `:focus-visible` da folha — a primeira versão deste guarda caiu no
    // `button:focus-visible` genérico que mora bem acima.
    let Some(revela) = folha
        .split('}')
        .map(|regra| regra.trim_start_matches(['\n', ' ']).to_owned())
        .find(|regra| regra.contains("visibility: visible") && regra.contains(".moderar-remover"))
    else {
        panic!("nada revela o `remover`, então ele está escondido para sempre:\n{folha}");
    };
    assert!(
        revela.contains(".moderar-remover:focus-visible"),
        "o `remover` é revelado por hover mas não pelo foco, então o Tab pára \
         num botão invisível:\n{revela}"
    );
    assert!(
        revela.contains(":hover .moderar-remover"),
        "o `remover` só aparece no foco do teclado, e o ponteiro não tem como \
         chegar nele:\n{revela}"
    );
}

/// O quadro-chave que arma o decodificador é entregue a ele, e não descartado.
///
/// **Este é o defeito que deixou o compartilhamento de tela sem imagem nos dois
/// sistemas, sem erro nenhum na tela.**
///
/// `armarPeloSps` lê o perfil do vídeo no primeiro quadro-chave — é dele que sai
/// o `avc1.PPCCLL` que o `VideoDecoder` precisa — e depois configurava o
/// decodificador deixando `esperandoChave = true`. O quadro que trouxe o SPS ia
/// embora, e a partir dali todo delta era pulado **até chegar outro
/// quadro-chave**.
///
/// Um segundo quadro-chave não vem sozinho: o codificador manda um no começo e
/// depois só quando alguém pede. Então o decodificador ficava armado, em
/// silêncio, esperando um quadro que não existia. Nada falhava — e é por isso
/// que a tela ficava preta sem uma frase explicando por quê: não havia falha a
/// explicar.
///
/// O quadro que carrega o SPS **é** um quadro-chave por definição, e é
/// exatamente o que um decodificador recém-configurado precisa receber
/// primeiro. Provado num navegador de verdade, com quadros que saíram do mesmo
/// `Codificador` que roda em produção: `tools/roteiros/palco.js`. Sem a entrega
/// o canvas fica escondido em 0×0; com ela ele desenha 960×540 com metade dos
/// pixels claros, que é o xadrez que entrou.
#[test]
fn the_key_frame_that_arms_the_decoder_is_handed_to_it() {
    let palco = read("ui/palco-imagem.js");
    let arma = js_function(&palco, "async function armarPeloSps(");

    let Some((_, depois)) = arma.split_once("configure(config)") else {
        panic!("`armarPeloSps` não configura mais o decodificador: {arma}");
    };
    assert!(
        depois.contains("entregarAoDecodificador"),
        "o quadro-chave que trouxe o SPS é lido e jogado fora, então o \
         decodificador fica armado esperando um segundo quadro-chave que o \
         codificador não manda sozinho — e a tela fica preta sem erro:\n{arma}"
    );
    assert!(
        !depois.contains("esperandoChave = true"),
        "`armarPeloSps` volta pedindo outro quadro-chave, o que descarta todo \
         delta que chegar até um que talvez nunca venha:\n{arma}"
    );

    // E a entrega é uma função à parte, para quem arma e quem recebe usarem o
    // mesmo caminho. Duas cópias divergem, e foi uma divergência assim que
    // deixou o assento devolvido sem anúncio no servidor.
    let entrega = js_function(&palco, "function entregarAoDecodificador(");
    assert!(
        entrega.contains("EncodedVideoChunk"),
        "`entregarAoDecodificador` não entrega nada ao decodificador: {entrega}"
    );
}

/// Quem descobre uma transmissão em curso pede um quadro-chave.
///
/// Um fluxo H.264 é um quadro-chave e uma corrente de diferenças que só fazem
/// sentido a partir dele, e o codificador manda um no começo e depois **só
/// quando alguém pede** — é medida e não descuido: um quadro-chave de 1080p
/// custa quatro vezes um quadro comum.
///
/// Quem estava na sala quando a transmissão começou recebe o fluxo desde o
/// primeiro byte, e o primeiro byte é aquele quadro-chave. Quem **entra depois**
/// pega a corrente no meio: só diferenças, de um quadro que nunca viu, que o
/// decodificador descarta uma a uma. A tela ficava vazia — «quando alguém entra
/// numa call que alguém tá compartilhando tela, a pessoa não consegue ver a
/// transmissão».
///
/// O pedido existia dos dois lados desde sempre — `Client::request_key_frame`
/// no cliente, `ClientMessage::RequestKeyFrame` no servidor, que já o traduz num
/// aviso a quem compartilha — e **ninguém chamava nenhum dos dois**.
#[test]
fn opening_a_stream_asks_for_a_key_frame() {
    let palco = read("ui/palco-imagem.js");
    let abre = js_function(&palco, "async function abrirImagemDaTela(");

    assert!(
        abre.contains("invoke(\"pedir_quadro_chave\""),
        "abrir uma transmissão não pede quadro-chave, então quem entra no meio \
         dela recebe só diferenças de um quadro que nunca viu:\n{abre}"
    );

    // E pedido **ao abrir**, não depois do primeiro quadro: esperar um
    // quadro-chave para poder pedir um quadro-chave é a espera que nunca acaba.
    let entrega = js_function(&palco, "function quadroDaTela(");
    assert!(
        !entrega.contains("pedir_quadro_chave"),
        "o pedido está no caminho de um quadro que chegou, e quem entra no meio \
         não recebe o quadro que dispararia o pedido:\n{entrega}"
    );
}

/// Todo nome de variante que a casca manda ao Rust é o nome do **fio**.
///
/// **Este guarda nasceu de um defeito que derrubou o compartilhamento de tela
/// por inteiro**, com esta frase na cara de quem tentou: «unknown variant
/// `Movimento`, expected `nitidez` or `movimento`».
///
/// Os quatro controles de limite saíram da caixa de compartilhar, e os valores
/// que eles escolhiam viraram constantes no JavaScript. O `<select>` mandava
/// `value="movimento"` — minúsculo, porque `Prioridade` carrega
/// `#[serde(rename_all = "lowercase")]`. Ao escrever a constante eu usei o nome
/// da **variante** em Rust, `Movimento`, que é o nome que se lê no código e não
/// o que atravessa a ponte. A ponte recusou, e não havia nada entre os dois
/// lados que soubesse compará-los.
///
/// Um `<select>` protegia por acidente: os valores estavam escritos ao lado do
/// rótulo, e trocá-los era mexer no HTML que a pessoa vê. Uma constante no meio
/// de um arquivo de script não tem esse acidente — então tem este guarda.
#[test]
fn the_variant_names_the_shell_sends_are_the_ones_the_wire_uses() {
    let ffi = std::fs::read_to_string(
        app_dir()
            .join("..")
            .join("..")
            .join("crates")
            .join("seele-ffi")
            .join("src")
            .join("types.rs"),
    )
    .expect("`seele-ffi/src/types.rs` é onde os tipos da ponte moram")
    // O mesmo `\r\n` que `read` normaliza, e que este teste não usava por ler
    // fora de `ui/`. No Windows ele reprovava aqui dizendo que `Prioridade`
    // deixou de ser minúscula, num arquivo em que ela é — porque a comparação é
    // um `ends_with` que termina em `\n`. Achado rodando a bateria numa máquina
    // Windows de verdade.
    .replace("\r\n", "\n");

    // As variantes de `Prioridade`, como o serde as escreve no fio.
    let Some(depois) = ffi.split("pub enum Prioridade {").nth(1) else {
        panic!("`Prioridade` mudou de forma; este guarda tem de mudar com ela");
    };
    let Some(corpo) = depois.split('}').next() else {
        panic!("`Prioridade` nunca fecha");
    };
    assert!(
        ffi.split("pub enum Prioridade {")
            .next()
            .is_some_and(|antes| antes.ends_with("#[serde(rename_all = \"lowercase\")]\n")),
        "`Prioridade` deixou de ser minúscula no fio, e a casca continua \
         mandando minúsculo"
    );
    let no_fio: Vec<String> = without_comments(corpo)
        .lines()
        .filter_map(|linha| {
            let nome = linha.trim().trim_end_matches(',');
            (!nome.is_empty() && nome.chars().next().is_some_and(char::is_uppercase))
                .then(|| nome.to_lowercase())
        })
        .collect();
    assert!(
        !no_fio.is_empty(),
        "não achei variante nenhuma em `Prioridade`"
    );

    // E o que a casca manda.
    let limites = js_function(&scripts(), "function limitesEscolhidos(");
    let Some((_, resto)) = limites.split_once("prioridade: \"") else {
        panic!("`limitesEscolhidos` não manda mais prioridade nenhuma: {limites}");
    };
    let Some((mandado, _)) = resto.split_once('"') else {
        panic!("a prioridade que a casca manda nunca fecha as aspas: {limites}");
    };

    assert!(
        no_fio.iter().any(|nome| nome == mandado),
        "a casca manda `{mandado}` e o fio aceita {no_fio:?}. É o nome da \
         variante em Rust em vez do nome que atravessa a ponte, e a ponte \
         recusa o comando inteiro — foi assim que compartilhar a tela parou de \
         funcionar de uma vez."
    );
}

/// As três superfícies que desenham uma pessoa vestem o retrato dela.
///
/// A comp da 0.9.0 desenha um avatar em três lugares — o bloco do operador, a
/// linha de cada mensagem, e o cartão de cada pessoa na grade da chamada. O
/// retrato passou a existir de verdade num commit, e **a grade ficou de fora**:
/// duas superfícies com foto e uma com letras, na mesma janela. Foi relatado
/// assim: «ícone do usuário não aparece na chamada».
///
/// O guarda é sobre a chamada a `vestirAvatar` e não sobre o desenho, porque é a
/// chamada que se esquece. Uma superfície nova entra por aqui.
#[test]
fn every_surface_that_draws_a_person_dresses_their_portrait() {
    for (arquivo, funcao, onde) in [
        (
            "ui/tela-sessao.js",
            "function desenharOperador(",
            "o bloco do operador",
        ),
        (
            "ui/tela-chamada.js",
            "function pintarCartao(",
            "o cartão da grade da chamada",
        ),
    ] {
        let corpo = js_function(&read(arquivo), funcao);
        assert!(
            corpo.contains("vestirAvatar("),
            "{onde} desenha uma pessoa e não veste o retrato dela, então ela \
             aparece com iniciais enquanto as outras superfícies mostram a \
             foto:\n{corpo}"
        );
    }

    // A linha da mensagem não é uma função inteira, e o avatar dela é desenhado
    // dentro do ramo de quem **não** repete o autor da linha anterior.
    let sessao = read("ui/tela-sessao.js");
    let Some((_, depois)) = sessao.split_once("\"mensagem-avatar\"") else {
        panic!("a linha da mensagem não desenha mais avatar nenhum");
    };
    let Some(perto) = depois.get(..400) else {
        panic!("o arquivo acaba logo depois do avatar da mensagem");
    };
    assert!(
        perto.contains("vestirAvatar("),
        "a linha da mensagem desenha um avatar e não veste o retrato:\n{perto}"
    );
}

#[test]
fn the_shell_never_remembers_a_nickname_nobody_chose() {
    // O relato: «pessoa não autorizada entra no link, eu autorizo, e ela não
    // consegue entrar porque o nome já existe» — e, decisivo, «mesmo trocando o
    // nick várias vezes continua dando o erro, num servidor com cinco nomes».
    //
    // Nada estava errado com os nomes digitados. `conectar` lê o apelido
    // lembrado desta execução primeiro e só pergunta ao perfil quando não há
    // nenhum; a última linha daquele bloco entregava ao lembrado o que o bloco
    // tivesse produzido, inclusive o recurso de quem nunca se nomeou. Da
    // primeira recusa em diante o cache estava cheio, o perfil nunca mais era
    // lido, e cada nome salvo ficava no disco sem sair da máquina.
    //
    // O recurso mudou de lado desde então — hoje quem o monta é o `main.rs`,
    // com a impressão da chave — e por isso este guarda deixou de olhar a ordem
    // entre duas linhas e passa a olhar a regra que sobrou, que é a que
    // importava: **só se lembra o que veio de escolha**.
    let corpo = without_comments(&js_function(
        &read("ui/tela-boot.js"),
        "async function conectar(",
    ));
    assert!(
        corpo.contains("if (apelido !== \"\") ultimoApelido = apelido;"),
        "a casca voltou a lembrar um apelido sem conferir se alguém o escolheu.\n\
         Lembrar o vazio, ou um recurso, faz o perfil parar de ser consultado \
         nas tentativas seguintes — e quem corrigir o nome no perfil vai vê-lo \
         gravado no disco sem nunca sair da máquina.\n{corpo}"
    );
}

#[test]
fn o_glifo_de_um_botao_nao_e_alvo_de_clique() {
    // Relato de campo, no Windows: «quando clico no meio dos botões de microfone
    // e de fone não funciona; precisa clicar no cantinho do botão».
    //
    // O centro de um botão de glifo é o `<svg>`; a borda é o `<button>`. Em
    // Chromium o clique no `<svg>` borbulha e o ouvinte roda — o laboratório de
    // `tools/carga-da-casca.py` confirmou isso, e é por isso que o defeito não
    // reproduz aqui. No WebView2 do Windows, não roda.
    //
    // `pointer-events: none` tira a pergunta do caminho: o alvo passa a ser
    // sempre o botão, em qualquer motor. Uma linha de CSS que some numa
    // reorganização de folha, e o sintoma que ela evita não aparece em nenhum
    // teste que rode fora do Windows — daí o guarda.
    let css = read("ui/base.css");
    let Some(regra) = css
        .split(".glifo {")
        .nth(1)
        .and_then(|d| d.split('}').next())
    else {
        panic!("a regra `.glifo` sumiu de `base.css`");
    };
    assert!(
        regra.contains("pointer-events: none"),
        "o glifo voltou a ser alvo de clique.\n\
         No WebView2 isso faz o meio dos botões de microfone e de fone parar de \
         responder, e só a borda funcionar."
    );
}

#[test]
fn o_isolamento_total_diz_que_cala_a_tela_tambem() {
    // O isolamento silencia todas as fontes de uma vez — é o `set_master` do
    // misturador, e `voice.rs` escreve que quem se isola «não ouve nem a voz nem
    // a tela». A decisão é boa. O que faltava era a interface contá-la: o botão
    // dizia «não ouvir ninguém», que fala de gente.
    //
    // O custo de não contar foi medido: «sem áudio na transmissão» levou a
    // cruzar dois logs, acrescentar telemetria dos dois lados e provar cadeia
    // por cadeia que o som saía do Mac, chegava ao Windows e era misturado — para
    // a resposta ser «tava com o áudio mutado, não sabia que mutava a live
    // também».
    //
    // Uma frase que some numa revisão de texto e leva o defeito de volta, sem
    // nada quebrar. Daí o guarda.
    let sessao = read("ui/tela-sessao.js");
    let corpo = without_comments(&sessao);
    assert!(
        corpo.contains("nem o som de uma tela compartilhada"),
        "o botão de isolamento voltou a dizer só que cala pessoas.\n\
         Ele cala a tela compartilhada junto, e quem não souber vai procurar \
         defeito no compartilhamento."
    );
    assert!(
        corpo.contains("não está ouvindo nada, nem a tela"),
        "a linha de estado voltou a dizer só «não está ouvindo».\n\
         Quem lê isso entende «ninguém está falando comigo», e não «o som da \
         tela também está calado»."
    );
}

#[test]
fn o_recurso_do_apelido_sai_da_impressao_desta_maquina() {
    // A outra metade do guarda da casca: ela manda vazio quando ninguém se
    // nomeou, e quem monta o nome é o `connect`.
    //
    // O recurso era a palavra `pessoa`, igual para todo mundo. Por ADR 0017 o
    // apelido pertence a uma chave, então a **segunda** pessoa sem nome de
    // qualquer servidor colidia por construção e era recusada na porta — com uma
    // frase mandando escolher outro apelido, um apelido que ela nunca escolheu.
    //
    // A impressão da chave resolve as duas metades de uma vez: não colide,
    // porque é da chave; e é **estável**, então quem volta amanhã volta na mesma
    // conta com o mesmo histórico, em vez de criar uma conta nova a cada
    // abertura — que é o que um sufixo aleatório faria.
    let fonte = read("src/main.rs");
    assert!(
        fonte.contains("impressao_desta_maquina"),
        "o `connect` deixou de derivar o apelido de recurso da impressão desta \
         máquina.\n\
         Sem isso, ou a conexão sai com o nome vazio — uma linha em branco no \
         roster de todo mundo — ou volta a uma constante que colide com a \
         segunda pessoa que não se nomear."
    );
    assert!(
        fonte.contains("nickname.trim().is_empty()"),
        "o `connect` deixou de tratar o apelido vazio.\n\
         A casca manda vazio de propósito quando ninguém se nomeou: é aqui que \
         isso vira um nome."
    );
}

#[test]
fn o_nome_do_canal_so_vira_botao_para_quem_administra() {
    // A mesma regra do nome da sala de voz, e pela mesma razão: um botão que
    // recusa é pior que a ausência dele, porque promete um caminho e o fecha
    // depois do clique. Quem não administra lê o nome como texto.
    let tela = read("ui/tela-sessao.js");
    let barra = tela
        .split("function desenharLinha(")
        .nth(1)
        .and_then(|resto| resto.split("\nfunction trocarONomeDaLinha(").next())
        .expect("a barra do canal aberto sumiu de tela-sessao.js");

    assert!(
        barra.contains("may_manage_voice_rooms"),
        "a barra do canal aberto deixou de conferir a permissão antes de \
         oferecer o renomear.\n\
         Sem isso o nome vira botão para todo mundo, e quem não administra \
         descobre que não pode depois de clicar e ler uma recusa."
    );
}

#[test]
fn o_renomear_do_canal_le_qual_canal_esta_aberto_na_hora_do_clique() {
    // O botão nasce uma vez — só quando a permissão muda — e vive enquanto a
    // tela viver. Entre o nascimento dele e o clique, quem usa pode ter aberto
    // outro canal.
    //
    // Se o canal fosse capturado na criação, o clique renomearia um canal que
    // ninguém está vendo, com o nome que está na tela pertencendo a outro. O
    // servidor aceitaria: é um pedido perfeitamente válido para o canal errado.
    let tela = read("ui/tela-sessao.js");
    let troca = tela
        .split("function trocarONomeDaLinha(")
        .nth(1)
        .expect("`trocarONomeDaLinha` sumiu de tela-sessao.js");
    let clique = troca
        .split("renomear_linha")
        .next()
        .expect("o renomear do canal sumiu da barra");

    assert!(
        clique.contains("channels") && clique.contains("open"),
        "o botão de renomear o canal deixou de descobrir qual canal está aberto \
         na hora do clique.\n\
         Capturar o canal quando o botão nasce renomeia o canal de antes — e o \
         servidor aceita, porque o pedido é válido para o canal errado."
    );
}

#[test]
fn o_canal_tem_um_lapis_grudado_no_x_e_nao_so_o_nome_da_barra_de_cima() {
    // **O relato foi uma pergunta:** «onde eu renomeio a sala? não devia ter um
    // botão de um lápis embaixo do x?».
    //
    // Renomear um canal existia e ficava no **título da barra de cima** — clicar
    // no nome do canal aberto, sem moldura, sem fundo, só cursor e sublinhado ao
    // passar. Um comentário deste repositório argumentava que «o alvo do gesto é
    // a coisa que se quer mudar, e não um lápis ao lado dela». O argumento é bom
    // e perdeu para o campo: quem é dono do produto foi procurar e não achou.
    //
    // O que este guarda prende não é o desenho, é a **descoberta**: o controle
    // fica na mesma fileira do canal, ao lado do que já se sabe apertar.
    let camada = without_comments(&read("ui/camada-moderar.js"));
    assert!(
        camada.contains("function botaoDeRenomearLinha"),
        "o canal perdeu o botão de renomear da fileira, e voltou a esconder o \
         verbo no título da barra de cima"
    );
    assert!(
        camada.contains("glifo(\"lapis\""),
        "o botão de renomear deixou de ser o lápis, que é o desenho que quem \
         procurou o verbo foi procurar"
    );
    assert!(
        camada.contains("data-renomear-linha") || camada.contains("renomearLinha"),
        "nada escuta o lápis, então ele é um botão que não faz nada"
    );

    // A permissão é a de administrar, e não a de destruir. Quem arruma nomes sem
    // poder derrubar canais existe na spec, e os dois botões aparecem e somem
    // independentes por causa disso.
    let renomear = camada
        .split("function botaoDeRenomearLinha")
        .nth(1)
        .and_then(|resto| resto.split("\n}\n").next())
        .unwrap_or_default()
        .to_owned();
    assert!(
        renomear.contains("may_manage_voice_rooms"),
        "o lápis está preso à permissão errada: trocar um nome se desfaz \
         digitando de novo, e destruir um canal não:\n{renomear}"
    );

    // E a fileira empilha os dois, em vez de pô-los lado a lado: a coluna tem
    // 268px e o que se lê nela é o nome do canal.
    let sessao = without_comments(&read("ui/tela-sessao.js"));
    assert!(
        sessao.contains("linha-controles"),
        "o `×` e o lápis deixaram de dividir uma coluna própria, e agora \
         disputam a largura com o nome do canal"
    );
    let css = read("ui/tela-sessao.css");
    assert!(
        css.contains("flex-direction: column"),
        "a coluna dos controles do canal não empilha, então o lápis não está \
         embaixo do `×` — que é onde ele foi procurado"
    );
}

#[test]
fn escolher_um_canal_leva_para_a_conversa_dele() {
    // Sem isto o clique não faz nada visível estando na grade: o canal troca por
    // baixo e a tela continua mostrando a chamada. É metade da navegação nova —
    // a outra é clicar numa sala para ver a grade —, e as duas juntas são o que
    // substituiu o alternador `CONVERSA`/`CHAMADA`.
    let sessao = without_comments(&read("ui/tela-sessao.js"));
    let trecho = sessao
        .split("async function alternarCanal")
        .nth(1)
        .and_then(|resto| resto.split("\n}\n").next())
        .unwrap_or_default()
        .to_owned();

    let abre = trecho.find("open_channel");
    let fecha = trecho.find("fecharChamada()");
    assert!(
        abre.is_some() && fecha.is_some(),
        "abrir um canal deixou de trocar a vista para a conversa dele:\n{trecho}"
    );
    assert!(
        abre < fecha,
        "a vista troca antes de o canal abrir, e o que aparece é a conversa \
         de antes:\n{trecho}"
    );
}

#[test]
fn o_rodape_do_operador_muda_de_oficio_com_a_sala() {
    // Quatro regras, e todas dizem a mesma coisa: um controle que nomeia um
    // lugar onde não se está é um controle que promete não fazer nada.
    //
    //   fora de sala   `SAIR DO SERVIDOR`, e o segundo botão escondido
    //   dentro        `SAIR DA SALA` e `COMPARTILHAR`
    let chamada = without_comments(&read("ui/tela-chamada.js"));
    let desenho = chamada
        .split("function desenharORodapeDoOperador")
        .nth(1)
        .and_then(|resto| resto.split("\n}\n").next())
        .unwrap_or_default()
        .to_owned();
    assert!(
        !desenho.is_empty(),
        "ninguém desenha o rodapé do operador, então os dois botões dizem para \
         sempre o que o HTML escreveu"
    );

    for frase in ["SAIR DA SALA", "SAIR DO SERVIDOR", "COMPARTILHAR"] {
        assert!(
            desenho.contains(frase),
            "«{frase}» sumiu do rodapé, e um dos dois estados ficou sem rótulo:\n{desenho}"
        );
    }
    assert!(
        desenho.contains("hidden"),
        "o botão de compartilhar deixou de sumir fora de sala, e compartilhar \
         fora de sala é uma promessa que o produto não cumpre:\n{desenho}"
    );

    // O alternador não voltou. Ele era um terceiro caminho para o que dois
    // cliques nas listas já fazem, e o pior dos três: não dizia de qual sala nem
    // de qual canal falava.
    assert!(
        !chamada.contains("naGrade ? \"CONVERSA\""),
        "o alternador `CONVERSA`/`CHAMADA` voltou ao rodapé"
    );
}

#[test]
fn sair_do_servidor_so_pergunta_quando_o_rotulo_nao_conta_tudo() {
    // A regra está escrita em `pedirTrocaDeServidor`, e este guarda a estende
    // para o rodapé: uma caixa que repete o rótulo do botão é a caixa que treina
    // a apertar duas vezes sem ler. `SAIR DO SERVIDOR` diz o que faz.
    //
    // O que ele **não** diz é que um servidor hospedado neste computador cai
    // junto, e todo mundo que estiver dentro sai. Essa é a parte que a caixa
    // existe para escrever — e é a única que a justifica aqui.
    let chamada = without_comments(&read("ui/tela-chamada.js"));
    let saida = chamada
        .split("$(\"operador-sair\").addEventListener")
        .nth(1)
        .and_then(|resto| resto.split("\n});").next())
        .unwrap_or_default()
        .to_owned();

    assert!(
        saida.contains("hospedandoAqui"),
        "a saída do servidor não pergunta se este computador hospeda, então ela \
         derruba o servidor de todo mundo sem avisar:\n{saida}"
    );
    let pergunta = saida.find("hospedandoAqui").unwrap_or(usize::MAX);
    let caixa = saida.find("abrirConfirmacao").unwrap_or(usize::MAX);
    assert!(
        pergunta < caixa,
        "a caixa abre antes de saber se há o que avisar:\n{saida}"
    );
    assert!(
        saida.contains("leave_voice_room"),
        "o mesmo botão dentro de uma sala tem que largar a sala, e não o \
         servidor:\n{saida}"
    );
}

#[test]
fn uma_atualizacao_que_nao_pegou_e_dita_e_nao_descoberta() {
    // **O atualizador do Tauri não sabe se o instalador abriu.** No Windows ele
    // entrega o arquivo ao shell e sai:
    //
    // ```rust
    // unsafe { ShellExecuteW(null, w!("open"), file, parameters, null, SW_SHOW) };
    // std::process::exit(0);
    // ```
    //
    // O retorno é descartado. UAC negado, elevação que falha, instalador que
    // aborta: o app fecha do mesmo jeito, e quem reabre está na versão de antes.
    // Foi relatado como pergunta — «por que não está persistindo?» —, e a
    // pergunta é a prova de que faltava a frase: a pessoa descobriu comparando
    // números de versão.
    let servidor = without_comments(&read("ui/tela-server.js"));
    assert!(
        servidor.contains("atualizacao_que_nao_pegou"),
        "ninguém pergunta se a atualização anterior pegou"
    );
    assert!(
        servidor.contains("contarAAtualizacaoQueNaoPegou()"),
        "a pergunta existe e ninguém a faz ao subir, então a frase espera a \
         pessoa abrir a configuração — depois de ela já ter descoberto sozinha"
    );

    // Na entrada, que é a tela da abertura. Escondida atrás da engrenagem ela
    // chegaria depois da pergunta.
    let conta = servidor
        .split("async function contarAAtualizacaoQueNaoPegou")
        .nth(1)
        .and_then(|resto| resto.split("\n}\n").next())
        .unwrap_or_default()
        .to_owned();
    assert!(
        conta.contains("boot-erro"),
        "a frase não aparece na entrada:\n{conta}"
    );
}

#[test]
fn o_bilhete_da_atualizacao_e_escrito_antes_de_o_processo_poder_morrer() {
    // No Windows o `install` não volta. Um bilhete escrito depois dele nunca
    // seria escrito, e a próxima abertura não teria o que comparar.
    let rust = without_comments(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"),
        )
        .expect("o main tem que ser legível"),
    );

    let instalar = rust
        .split("async fn instalar_atualizacao")
        .nth(1)
        .and_then(|resto| resto.split("\n}\n").next())
        .unwrap_or_default()
        .to_owned();

    let anota = instalar.find("anotar_a_tentativa").unwrap_or(usize::MAX);
    let instala = instalar.find("nova.install(").unwrap_or(usize::MAX);
    assert!(
        anota < instala,
        "o bilhete é escrito depois do `install`, que no Windows não volta:\n{instalar}"
    );

    // E lido uma vez: o Rust o apaga na leitura. Uma queixa que se repete a cada
    // abertura é uma queixa que se aprende a ignorar.
    let le = rust
        .split("fn atualizacao_que_nao_pegou")
        .nth(1)
        .and_then(|resto| resto.split("\n}\n").next())
        .unwrap_or_default()
        .to_owned();
    assert!(
        le.contains("remove_file"),
        "o bilhete sobrevive à leitura, e a frase passa a aparecer para sempre:\n{le}"
    );
}

#[test]
fn a_caixa_de_compartilhar_troca_de_janela_sem_parar_antes() {
    // Pedido: «quando estiver compartilhando o modal de compartilhamento deve
    // permitir a troca de janela. Hoje só permite parar a transmissão.»
    //
    // O botão sumia enquanto a transmissão era da própria pessoa. O comentário
    // que justificava isso dizia que «trocar de monitor no meio continua sendo
    // parar e começar de novo, que é o que sempre foi» — e era, mas isso
    // descrevia o que a pessoa tinha de fazer, não o que a caixa oferecia: o
    // único caminho era PARAR, fechar, reabrir e escolher de novo.
    let caixa = without_comments(&read("ui/camada-compartilhar.js"));

    assert!(
        !caixa.contains("comecar.hidden = minha"),
        "o botão voltou a sumir enquanto a transmissão é sua, e com ele o único \
         jeito de trocar de janela"
    );
    assert!(
        caixa.contains("\"TROCAR\""),
        "o botão não diz o que faz quando já há transmissão sua"
    );

    // **Para antes de começar, e aguardado.** O servidor recusa uma segunda
    // abertura da mesma pessoa — `tela_abriu` tem essa parede, e é ela que
    // impede alguém de ocupar duas vagas da sala. Começar antes de a parada
    // chegar lá é a mesma corrida por outro caminho.
    let aperto = caixa
        .split("$(\"compartilhar-comecar\").addEventListener")
        .nth(1)
        .and_then(|resto| resto.split("\n});").next())
        .unwrap_or_default()
        .to_owned();

    let para = aperto.find("await invoke(\"parar_de_compartilhar\")");
    let comeca = aperto.find("await invoke(\"compartilhar_tela\"");
    assert!(
        para.is_some() && comeca.is_some(),
        "trocar deixou de parar a transmissão de agora:\n{aperto}"
    );
    assert!(
        para < comeca,
        "a nova começa antes de a antiga parar, e o servidor recusa a segunda \
         abertura da mesma pessoa:\n{aperto}"
    );
}

#[test]
fn trocar_para_a_mesma_fonte_nao_corta_a_imagem_de_quem_assiste() {
    // Recomeçar corta a imagem de quem está vendo por um instante. Fazer isso
    // para chegar onde já se estava é o botão trabalhando contra quem o
    // apertou — e o retrato não ajuda: `TelaEmCurso` não diz **o quê** está
    // sendo mostrado, porque o servidor encaminha bytes e nunca soube que
    // monitor eles são. Quem sabe é a casca, que escolheu.
    let caixa = without_comments(&read("ui/camada-compartilhar.js"));

    assert!(
        caixa.contains("let fonteEmCurso"),
        "a casca deixou de lembrar qual fonte está no ar, e `TROCAR` passa a \
         recomeçar a transmissão para ela mesma"
    );
    assert!(
        caixa.contains("fonteArmada === fonteEmCurso"),
        "nada compara a fonte escolhida com a que já está saindo"
    );

    // E ela é esquecida quando a transmissão acaba por fora — o servidor a
    // encerrou, o teto estourou, a conexão caiu. Sem isso `TROCAR` continuaria
    // recusando a única fonte que a pessoa quer mandar.
    let desenho = caixa
        .split("function desenharBotoesDeTela")
        .nth(1)
        .and_then(|resto| resto.split("\n}\n").next())
        .unwrap_or_default()
        .to_owned();
    assert!(
        desenho.contains("fonteEmCurso = null"),
        "a fonte de agora sobrevive ao fim da transmissão:\n{desenho}"
    );
}

#[test]
fn a_escolha_da_transmissao_e_uma_intencao_e_nao_o_estado_do_fluxo() {
    // **As duas metades do relato saem daqui**: «tem hora que clico e ele não
    // vai, as vezes cliquei pra não ver ele fecha e depois abre novamente
    // sozinho».
    //
    // A fileira era desenhada a partir de `telaEmCurso`, que só é escrito
    // quando o fluxo **abre de verdade** — e abrir espera o próximo quadro-chave
    // de quem transmite, que são segundos. O intervalo inteiro entre o clique e
    // a imagem era indistinguível de nada ter acontecido, e clicar de novo
    // reiniciava a espera.
    let palco = without_comments(&read("ui/palco-imagem.js"));

    assert!(
        palco.contains("let telaQuerida"),
        "a intenção voltou a não ter variável própria, e a fileira volta a \
         mostrar o fluxo em vez do pedido"
    );
    assert!(
        !palco.contains("naoQueroVer"),
        "a bandeira antiga voltou: duas variáveis para um estado são duas \
         variáveis que discordam"
    );

    // Clicar duas vezes na mesma não pode refazer o pedido.
    let troca = palco
        .split("async function trocarDeTransmissao")
        .nth(1)
        .and_then(|resto| resto.split("\n}\n").next())
        .unwrap_or_default()
        .to_owned();
    assert!(
        troca.contains("if (tela === telaQuerida) return;"),
        "a comparação voltou a ser com o que está na tela; um segundo clique \
         refaz o pedido e reinicia a espera pelo quadro-chave:\n{troca}"
    );

    // E o desenho marca o pedido, com o «abrindo» enquanto ele não chega.
    let desenho = palco
        .split("function desenharTransmissoes")
        .nth(1)
        .and_then(|resto| resto.split("\n}\n").next())
        .unwrap_or_default()
        .to_owned();
    assert!(
        desenho.contains("const pedida =") && desenho.contains("ABRINDO"),
        "a fileira deixou de mostrar o pedido e o estado de espera:\n{desenho}"
    );
}

#[test]
fn um_retrato_transitorio_nao_apaga_a_escolha_de_quem_esta_na_sala() {
    // «Cliquei pra não ver, ele fecha e depois abre novamente sozinho.»
    //
    // A escolha era apagada sempre que um retrato não afirmasse que se está
    // numa sala — e um retrato transitório, chegando entre dois estados, afirma
    // isso sem querer. A escolha morria, o servidor religava, e a imagem voltava.
    //
    // Duas coisas consertam: só um retrato que **traz** a lista de salas conta,
    // e compara-se a sala em vez da presença.
    let palco = without_comments(&read("ui/palco-imagem.js"));
    let desenho = palco
        .split("function desenharTransmissoes")
        .nth(1)
        .and_then(|resto| resto.split("\n}\n").next())
        .unwrap_or_default()
        .to_owned();

    assert!(
        desenho.contains("Array.isArray(snapshot?.voice_rooms)"),
        "um retrato sem a lista de salas voltou a contar como «saiu da sala», e \
         não saber não é motivo para esquecer:\n{desenho}"
    );
    assert!(
        desenho.contains("salaDaEscolha"),
        "a escolha deixou de ser presa à sala em que foi feita:\n{desenho}"
    );
}

#[test]
fn o_servidor_nao_impoe_uma_transmissao_que_ninguem_pediu() {
    // Ele liga todo mundo na primeira transmissão de uma sala, sem pedido, e
    // isso é o certo para quem não disse nada. Para quem disse — não ver, ou ver
    // outra —, abrir tem de devolver a cópia.
    let palco = without_comments(&read("ui/palco-imagem.js"));
    let abrir = palco
        .split("async function abrirImagemDaTela")
        .nth(1)
        .and_then(|resto| resto.split("\n}\n").next())
        .unwrap_or_default()
        .to_owned();

    assert!(
        abrir.contains("telaQuerida !== undefined && telaQuerida !== tela"),
        "abrir a imagem deixou de conferir o pedido, e o servidor volta a impor \
         o que ninguém escolheu:\n{abrir}"
    );
    let confere = abrir.find("telaQuerida").unwrap_or(usize::MAX);
    let fecha = abrir.find("fecharImagemDaTela()").unwrap_or(usize::MAX);
    assert!(
        confere < fecha,
        "a conferência acontece depois de a imagem já ter sido montada:\n{abrir}"
    );
}

#[test]
fn os_pacotes_que_nao_voltam_nao_se_chamam_sincronizacao() {
    // **Duas falhas vestiam a mesma frase, e ela descrevia só uma.**
    //
    // `quinn::ConnectionError::TimedOut` é a conexão inteira expirando, e chega
    // **antes** de existir aperto de mão nenhum: os pacotes saíram e nada
    // voltou. Traduzi-lo como «tempo esgotado na sincronização inicial» manda
    // quem lê investigar protocolo, versão e servidor — três coisas do outro
    // lado — quando o que aconteceu foi um datagrama que não chegou lá.
    //
    // Perguntado assim: «o Mac só dá tempo esgotado na sincronização. Como
    // isso, se os PCs estão na mesma LAN e na mesma versão?». A pergunta é a
    // prova: as duas coisas que a frase mandou conferir estavam certas.
    let frases = without_comments(&read("ui/frases.js"));

    assert!(
        frases.contains("SemResposta:"),
        "a falha de «nada voltou» perdeu a frase dela, e volta a se chamar \
         sincronização"
    );
    assert!(
        frases.contains("OS PACOTES SAÍRAM E NADA VOLTOU"),
        "a frase deixou de dizer o que de fato aconteceu"
    );
    // **Duas máquinas podem calar este caminho, e a frase precisa das duas.**
    //
    // Esta parte do guarda pedia só o firewall de quem hospeda, e mandava
    // consertar a máquina certa metade das vezes. A outra metade apareceu no
    // rastro de campo: o candidato `192.168.…` gastava os quatro segundos em
    // silêncio enquanto o endereço público entrava no mesmo instante — o app
    // alcançava a máquina dando a volta pelo mundo e não pelo cabo ao lado.
    // Não era firewall nenhum: era o macOS negando, sem perguntar, o que sai
    // deste app para a própria casa.
    //
    // Nomear os dois numa frase só é o que `camada-compartilhar.js` já faz com
    // a recusa de gravar tela, e pela mesma razão: os sistemas negam coisas
    // diferentes, e quem lê está num deles.
    assert!(
        frases.contains("8383 UDP"),
        "a frase não diz o que fazer no lado de quem hospeda, e a porta \
         fechada continua sendo uma das duas causas"
    );
    assert!(
        frases.contains("rede local"),
        "a frase esqueceu a outra metade: no macOS o silêncio no caminho da LAN \
         é desta máquina, que recusa calada o que sai daqui para a própria casa \
         enquanto ninguém disser que sim nos Ajustes"
    );

    // E a de sincronização passa a descrever só o que ela é: o servidor
    // recebeu e demorou.
    // **São duas tabelas, e a chave se repete nas duas.** `MOTIVOS` traduz o
    // que o servidor manda pelo fio; `FRASES` traduz o que este lado conclui
    // sozinho. As duas tinham a mesma frase para coisas opostas — uma é «ele
    // recebeu e desistiu», a outra é «nada chegou lá».
    let motivos = frases
        .split("const MOTIVOS")
        .nth(1)
        .and_then(|resto| resto.split("};").next())
        .unwrap_or_default()
        .to_owned();
    assert!(
        motivos.contains("O SERVIDOR DESISTIU DE ESPERAR"),
        "o motivo que vem pelo fio voltou a se chamar sincronização, e some a \
         diferença entre «ele desistiu» e «nada chegou»:\n{motivos}"
    );

    let tabela = frases
        .split("const FRASES")
        .nth(1)
        .and_then(|resto| resto.split("};").next())
        .unwrap_or_default()
        .to_owned();
    assert!(
        tabela.contains("RECEBEU E NÃO RESPONDEU"),
        "a frase do aperto de mão não diz que o servidor recebeu, que é a \
         única coisa que a separa da outra:\n{tabela}"
    );
}

#[test]
fn a_parede_do_firewall_traz_o_conserto_junto() {
    // «Mas uai, o instalador já não faz isso?» Faz quando pedem — a caixa nasce
    // desmarcada no passo 02 —, e quem instala um app de conversa não está
    // decidindo sobre firewall naquele instante. Quem aperta HOSPEDAR AQUI está.
    //
    // A frase sozinha mandava reinstalar, que é um preço alto por uma linha no
    // firewall. O botão faz o Windows perguntar uma vez.
    let boot = without_comments(&read("ui/tela-boot.js"));

    assert!(
        boot.contains("abrir_a_porta_no_firewall"),
        "a parede do firewall voltou a ser só uma frase, e a frase manda \
         reinstalar por causa de uma regra que um clique cria"
    );
    assert!(
        boot.contains("ABRIR A PORTA AGORA"),
        "o botão perdeu o rótulo que diz o que ele faz"
    );

    // **O sucesso é lido, e não presumido.** Um botão que se declara vitorioso
    // sem confirmação é o mesmo defeito do atualizador do Tauri, que fechava o
    // app sem saber se o instalador tinha aberto.
    let ramo = boot
        .find("abrir_a_porta_no_firewall")
        .map(|onde| boot.get(onde..).unwrap_or_default().to_owned())
        .unwrap_or_default();
    assert!(
        ramo.contains("catch"),
        "o botão não trata a recusa, e recusar o UAC é o caminho mais comum \
         depois de apertar:\n{ramo}"
    );
}

/// Os três comandos de MOD existem no Rust com o nome que a página invoca.
///
/// ADR 0019 escolheu frontend sem checagem de tipo, e este arquivo é a
/// mitigação nomeada: um `invoke` com nome errado não aparece em build nenhum,
/// só em execução, e só quando alguém clica.
#[test]
fn the_mod_commands_the_page_invokes_exist_in_rust() {
    let rust = read("src/main.rs");
    for command in ["mods_instalados", "habilitar_mod", "desabilitar_mod"] {
        assert!(
            rust.contains(&format!("fn {command}(")),
            "a página invoca `{command}` e o Rust não o define"
        );
    }
}

/// A casca não nomeia o protocolo nem o núcleo para falar de MOD.
///
/// `xtask/src/check_deps.rs` já reprova a **aresta** de Cargo; este guarda cobra
/// o outro lado da mesma regra, que é o que custou uma reescrita durante a
/// execução do plano: os comandos passavam por `seele_core::mods` e a casca não
/// alcança aquele crate. Quem sabe de MOD aqui é `seele_ffi::mods`, e a regra
/// escreve o motivo — «reaching past it would put protocol knowledge in a Tauri
/// command».
#[test]
fn the_shell_reaches_mods_through_the_ffi_and_not_past_it() {
    let rust = read("src/main.rs");
    assert!(
        !rust.contains("seele_core::mods"),
        "a casca alcançou `seele_core::mods` por cima do `seele-ffi`"
    );
    assert!(
        !rust.contains("seele_proto::mods"),
        "a casca alcançou `seele_proto::mods`, que é conhecimento de protocolo numa Tauri command"
    );
    assert!(
        rust.contains("seele_ffi::mods"),
        "a casca deixou de falar de MOD pelo `seele-ffi`"
    );
}

/// A CSP admite um `Worker`, e o esquema dos MODs saiu dela — ADR 0049.
///
/// O ADR 0045 abriu `mod:` porque o código de um MOD era um `<script>` da
/// página, e `script-src 'self'` recusa script que não venha do pacote. O ADR
/// 0049 tirou o MOD da página: o código vai para um worker, pela ponte, e a
/// janela deixou de ter de onde carregá-lo como script.
///
/// **Uma porta sem ninguém do outro lado continua sendo uma porta.** Este
/// guarda é o que impede o esquema de voltar a ser admitido «porque já estava
/// lá»: se `mod:` voltar à CSP sem que nada o use, ele é alcance de disco que
/// um terceiro escolhe, oferecido à página de graça.
///
/// E o critério do 0045 continua valendo palavra por palavra: se rodar um MOD
/// exigisse `unsafe-inline` ou `unsafe-eval`, o desenho estaria errado.
#[test]
fn the_csp_admits_mods_and_never_loosens_further() {
    let conf = read("tauri.conf.json");
    let csp = conf
        .lines()
        .find(|line| line.contains("\"csp\""))
        .expect("a CSP sumiu do tauri.conf.json");

    assert!(
        csp.contains("worker-src blob:"),
        "sem `worker-src blob:` nenhum MOD sobe: o worker é construído de um \
         `Blob`, e a política o recusa antes da primeira linha dele"
    );
    for forma in ["mod:", "mod.localhost"] {
        assert!(
            !csp.contains(forma),
            "`{forma}` voltou à CSP, e nada na janela o usa desde o ADR 0049"
        );
    }
    assert!(
        csp.contains("script-src 'self'") && !csp.contains("script-src 'self' "),
        "o `script-src` deixou de ser só o pacote: {csp}"
    );
    assert!(!csp.contains("unsafe-inline"), "a CSP ganhou unsafe-inline");
    assert!(!csp.contains("unsafe-eval"), "a CSP ganhou unsafe-eval");
}

/// **O que um MOD desenha é montado pelo produto, e nunca interpretado.**
///
/// ADR 0049. A região é o único lugar da página onde um MOD aparece, e o
/// conteúdo dela vem de código de terceiro. Montá-lo como marcação — `innerHTML`
/// e família — seria devolver ao MOD a página inteira por outro caminho, que é
/// exatamente o que tirá-lo da janela existiu para acabar.
///
/// Os dois lados da mesma regra: o produto **monta** (`createElement`,
/// `createTextNode`, `textContent`) e **não interpreta**.
#[test]
fn o_que_um_mod_declara_e_montado_e_nunca_interpretado() {
    let regiao = without_comments(&read("ui/mods-regiao.js"));
    let criar = js_function(&regiao, "\n  criar(plano)");

    // `elemento` é o construtor desta casa — `createElement` mais `textContent`
    // — e é por ele que o produto monta. É também por ele que o guarda de CSS
    // enxerga as classes, e montar por fora dele põe classe sem regra na tela.
    assert!(
        criar.contains("elemento(FORMAS_DA_REGIAO[plano.forma]"),
        "`criar` deixou de montar por `elemento`: {criar}"
    );
    let base = without_comments(&read("ui/base.js"));
    let construtor = js_function(&base, "function elemento(");
    assert!(
        construtor.contains("createElement") && construtor.contains("textContent"),
        "`elemento` deixou de ser `createElement` + `textContent`, e é por ele \
         que o que um MOD declara vira nó: {construtor}"
    );
    let reconciliar = js_function(&regiao, "\n  reconciliar(pai, planos, fundura, orcamento)");
    assert!(
        reconciliar.contains("createTextNode"),
        "texto deixou de virar nó de texto: {reconciliar}"
    );
    for proibido in [
        "innerHTML",
        "outerHTML",
        "insertAdjacentHTML",
        "createContextualFragment",
        "document.write",
    ] {
        assert!(
            !regiao.contains(proibido),
            "a região passou a interpretar marcação por `{proibido}`, e o \
             conteúdo dela vem de um MOD"
        );
    }

    // Uma forma que a API não conhece não vira elemento por conveniência.
    let planejar = js_function(
        &regiao,
        "\n  planejar(no, fundura, orcamento, permitidas = null)",
    );
    assert!(
        planejar.contains("if (!forma) {") && planejar.contains("return [];"),
        "uma forma desconhecida deixou de ser recusada; a gramática passa a \
         crescer sem ninguém decidir: {planejar}"
    );
    // E num cartão, a recusa é **contada**: uma forma que a região conhece e o
    // cartão não é um `botao` que o MOD pôs e não vai aparecer, e ele precisa
    // saber disso em vez de ver o nó sumir.
    assert!(
        planejar.contains("if (permitidas) this.recusados += 1;"),
        "a forma recusada num cartão voltou a sumir calada: {planejar}"
    );

    // E há teto de fundura: sem ele, uma árvore que o MOD manda estoura a pilha
    // **da janela**, e não a dele. E teto de nós, que a fundura sozinha não
    // alcança: uma árvore rasa e larguíssima cabe em oito níveis.
    // O teto vem do perfil — região, cartão ou superfície —, e é por isso que
    // a asserção olha `this.perfil.fundura` e não mais um ternário: os três
    // lugares têm orçamentos diferentes, e `PERFIS_DE_RENDER` é onde cada um
    // deles está escrito com a razão dele.
    assert!(
        planejar.contains("this.perfil.fundura") && planejar.contains("fundura > teto"),
        "o teto de fundura saiu: uma árvore funda mandada por um MOD derruba a \
         janela do produto: {planejar}"
    );
    // E os três perfis continuam tendo teto: um perfil sem número é um teto
    // que não existe, e ele não apareceria em lugar nenhum além de uma janela
    // que trava.
    let perfis = js_const(&regiao, "PERFIS_DE_RENDER");
    for perfil in ["regiao:", "cartao:", "superficie:"] {
        assert!(
            perfis.contains(perfil),
            "`PERFIS_DE_RENDER` perdeu o perfil `{perfil}`, e quem renderiza \
             com ele fica sem teto nenhum: {perfis}"
        );
    }
    for teto in ["nos", "fundura", "campos", "midias", "bytesDeMidia"] {
        assert_eq!(
            perfis.matches(&format!("{teto}:")).count(),
            3,
            "o teto `{teto}` não está nos três perfis de renderização: {perfis}"
        );
    }
    assert!(
        planejar.contains("orcamento.nos <= 0"),
        "o teto de nós saiu: uma árvore rasa e larguíssima cabe na fundura e \
         custa layout do mesmo jeito: {planejar}"
    );
}

/// **Todo pedido de MOD recebe resposta, inclusive a recusa.**
///
/// `SeeleMods.request` é uma promessa, e uma promessa não rejeita sozinha: um
/// pedido que a janela decide não atender e não responde deixa o MOD esperando
/// para sempre — vivo, mudo, e sem nada no registro.
///
/// Medido no aplicativo nativo em 19/09, em três de seis execuções: o MOD
/// parava na segunda linha e ninguém sabia por quê. O caminho era o `return`
/// calado depois do `await` do retrato, quando a sessão tinha andado no meio.
///
/// Recusar **não é admitir efeito** — é o contrário dele, e é o que devolve o
/// controle a quem escreveu o MOD.
#[test]
fn um_pedido_de_mod_nunca_fica_sem_resposta() {
    let base = without_comments(&read("ui/base.js"));
    let atender = js_function(&base, "async function atenderOMod(");

    // Quando outra instância tomou o lugar desta, o bloco registra e retorna.
    // A instrumentação não muda a recusa; conferir o bloco permite o registro.
    let substituida = js_function(&atender, "if (!minhaInstancia())");
    assert!(
        substituida.contains("return;"),
        "a porta de entrada deixou de distinguir «não é minha instância» de \
         «a sessão andou», e as duas calavam o MOD: {atender}"
    );
    assert!(
        !atender.contains("if (!meu()) return;"),
        "voltou um `return` calado quando a sessão andou, e é exatamente ele \
         que prende o MOD esperando para sempre: {atender}"
    );

    // E a recusa sai pelo mesmo `responder`, para não haver dois caminhos.
    let recusa = atender
        .split_once("const responder = (ok, carga) => {")
        .expect("`responder`")
        .1;
    assert!(
        recusa.contains("ok: false") && recusa.contains("sessao-encerrada"),
        "`responder` deixou de recusar quando não pode entregar o valor, e \
         calar deixa o MOD preso: {recusa}"
    );

    // O retrato é o caso que custou a medição: depois do `await`, ele não pode
    // sair sem resposta nenhuma.
    let depois = atender
        .split_once("case \"snapshot\"")
        .expect("o caso do retrato")
        .1
        .split_once("break;")
        .expect("o fim do caso")
        .0;
    assert!(
        depois.contains("responder(") && !depois.contains("return;"),
        "o caso do retrato voltou a sair sem responder: {depois}"
    );
}

/// **Arredondamento e brilho são aplicados, e o que os limita é número.**
///
/// Eles já existiam como token — `--seele-raio` e `--seele-sombra`, em
/// `tokens.css`, valendo `0` e `none`, lidos por três folhas. O que faltava era
/// alguém poder escrevê-los, e o produto já guardava o lugar.
///
/// O que este guarda protege é a **forma** de escrevê-los, que é onde o risco
/// mora. Um raio livre aceita `9999px`; uma sombra livre aceita qualquer
/// `box-shadow`, inclusive um que desenhe fora do painel e cubra a tela. Por
/// isso um é inteiro com intervalo, o outro é sim ou não, e a sombra é montada
/// **pelo produto** a partir de um token do produto.
#[test]
fn o_arredondamento_e_o_brilho_sao_numero_e_bandeira_e_nunca_texto() {
    let base = without_comments(&read("ui/base.js"));

    // Os dois existem na API, e cada um na sua tabela.
    let numeros = bloco_congelado(&base, "const NUMEROS_DA_API = Object.freeze({");
    let bandeiras = bloco_congelado(&base, "const BANDEIRAS_DA_API = Object.freeze({");
    assert!(
        numeros.contains("arredondamento") && numeros.contains("--seele-raio"),
        "`arredondamento` deixou de escrever `--seele-raio`: {numeros}"
    );
    assert!(
        bandeiras.contains("brilho") && bandeiras.contains("--seele-sombra"),
        "`brilho` deixou de escrever `--seele-sombra`: {bandeiras}"
    );

    // **O intervalo é o que separa escolher de escrever CSS.** Sem teto, um
    // raio de 9999 vira um comprimido, e a API de tema vira uma porta.
    assert!(
        numeros.contains("minimo: 0") && numeros.contains("maximo: 24"),
        "o intervalo do arredondamento sumiu, e um número livre entra: {numeros}"
    );
    let aplicar = js_function(&base, "function aplicarOTemaDoMod(");
    assert!(
        aplicar.contains("Number.isInteger(valor)")
            && aplicar.contains("valor < regra.minimo")
            && aplicar.contains("valor > regra.maximo"),
        "o intervalo deixou de ser conferido na aplicação: {aplicar}"
    );
    assert!(
        aplicar.contains("typeof valor !== \"boolean\""),
        "a bandeira deixou de exigir booleano, e `\"false\"` passa a ser verdade: {aplicar}"
    );

    // **A sombra é do produto.** O que o MOD manda é `true`; o valor que chega
    // na folha é montado aqui, com um token daqui.
    assert!(
        bandeiras.contains("var(--seele-laranja-nerv)"),
        "o brilho deixou de ser feito de um token do produto: {bandeiras}"
    );
    assert!(
        !bandeiras.contains("valor") && !bandeiras.contains("cor"),
        "a bandeira passou a carregar valor de fora, e uma sombra livre entra: {bandeiras}"
    );

    // A unidade é escrita aqui, e não pelo MOD: o que atravessa a API é 8, e o
    // que vai para a folha é `8px`. Uma unidade vinda de fora é uma string, e
    // uma string é onde cabe o resto.
    let escrever = js_function(&base, "function escreverOTemaDaSessao(");
    assert!(
        escrever.contains("`${valor}${regra.unidade}`"),
        "a unidade deixou de ser escrita pelo produto: {escrever}"
    );
    assert!(
        escrever.contains("regra.ligado") && escrever.contains("regra.desligado"),
        "a bandeira deixou de escolher entre os dois valores do produto: {escrever}"
    );

    // E os tokens que os dois escrevem existem, e são lidos por alguma folha.
    let tokens = read("ui/tokens.css");
    for token in ["--seele-raio", "--seele-sombra"] {
        assert!(
            tokens.contains(&format!("{token}:")),
            "`{token}` não está em `tokens.css`, e o tema escreveria no vazio"
        );
    }
}

/// **A estética diz que o recuo é canto reto — e diz quem pode levantá-lo.**
///
/// A regra do produto continua a mesma: sem raio e sem sombra, por padrão. O
/// que mudou é que ela passou a ter uma exceção nomeada, e uma exceção que não
/// está escrita é uma deriva esperando para ser descoberta por quem vier
/// depois.
#[test]
fn a_excecao_do_tema_de_servidor_esta_escrita_na_estetica() {
    let estetica = read("../../specs/07-estetica.md");
    assert!(
        estetica.contains("canto reto") && estetica.contains("sombra"),
        "`specs/07-estetica.md` deixou de afirmar o canto reto e a ausência de sombra"
    );
    assert!(
        estetica.contains("--seele-raio") && estetica.contains("--seele-sombra"),
        "a exceção do tema de servidor não está escrita onde a regra está: um \
         produto cujo padrão é canto reto e que aceita raio por tema precisa \
         dizer as duas coisas no mesmo lugar"
    );
}

/// **O cartão de um MOD é desenhado pelo renderer do produto, e sai com ele.**
///
/// Esta é a única superfície de um MOD fora da região dele. O que a mantém
/// sendo do produto não é uma promessa: é o renderer ser o mesmo —
/// `planejar`, `reconciliar`, `elemento`, `createElement`, `textContent` — com
/// uma gramática menor e tetos próprios.
///
/// Os limites, a recusa e o descarte rodam em `bancada/regiao-do-mod.cjs`, no
/// DOM de mentira, contra este mesmo arquivo. O que este guarda prova é o que
/// um teste de comportamento não alcança: que a lista continua sendo montada
/// aqui, e que nada de fora escreve estilo nela.
#[test]
fn o_cartao_de_um_mod_e_declarado_e_quem_desenha_e_o_produto() {
    let regiao = without_comments(&read("ui/mods-regiao.js"));
    let base = without_comments(&read("ui/base.js"));
    let sessao = without_comments(&read("ui/tela-sessao.js"));

    // **O mesmo renderer.** Se `declararCartoes` parar de chamar `planejar` e
    // `reconciliar`, o cartão passou a ser montado por outra coisa — e essa
    // outra coisa é a que ninguém revisou.
    let declarar = js_function(&regiao, "\n  declararCartoes(cartoes)");
    assert!(
        declarar
            .contains("this.planejar(declaracao, 0, orcamento, PERFIS_DE_RENDER.cartao.formas)")
            && declarar.contains("this.reconciliar(raiz, planos, 0, orcamento)"),
        "o cartão deixou de passar pelo renderer da região: {declarar}"
    );
    assert!(
        declarar.contains("elemento(\"div\", \"pessoa-cartao\")"),
        "a raiz do cartão deixou de nascer por `elemento`, que é o construtor \
         desta casa e o que o guarda de CSS enxerga: {declarar}"
    );

    // **A gramática é menor, e o que fica de fora é tudo que recebe foco.**
    //
    // A lista cresceu com a API 4 — composição e apresentação de pessoa
    // entraram, porque U27 pediu que o cartão pudesse **substituir** a
    // identidade e não só acrescentar uma linha embaixo dela. O que não mudou
    // é a razão de a lista existir: a linha do roster já tem um botão do
    // produto, e um controle de MOD ao lado dele divide foco e área de toque
    // com ele.
    //
    // `formulario` e `abas` entram na lista proibida porque os dois contêm
    // controles; `link` porque é botão.
    let perfis = js_const(&regiao, "PERFIS_DE_RENDER");
    let formas = perfis
        .split_once("cartao:")
        .expect("o perfil do cartão")
        .1
        .split_once("superficie:")
        .map_or_else(|| perfis.clone(), |(antes, _)| antes.to_owned());
    for fora in [
        "campo", "escolha", "botao", "arquivo", "tela", "formulario", "abas",
        "textoLongo", "numero", "deslizante", "marca", "interruptor", "cor", "link",
    ] {
        assert!(
            !formas.contains(&format!("\"{fora}\"")),
            "`{fora}` entrou na gramática do cartão, e ele divide foco e área \
             de toque com um controle do produto: {formas}"
        );
    }
    for dentro in [
        "texto", "titulo", "linha", "lista", "item", "midia",
        "caixa", "pilha", "grade", "retrato", "distintivo",
    ] {
        assert!(
            formas.contains(&format!("\"{dentro}\"")),
            "`{dentro}` saiu da gramática do cartão: {formas}"
        );
    }

    // Tetos próprios, e menores que os da região: um cartão é por pessoa.
    let limites = bloco_congelado(&regiao, "const LIMITES_DO_CARTAO = Object.freeze({");
    for teto in ["cartoes:", "nos:", "fundura:", "midias:", "bytesDeMidia:"] {
        assert!(
            limites.contains(teto),
            "o teto `{teto}` sumiu dos limites do cartão: {limites}"
        );
    }

    // **A montagem acontece ao declarar, e não ao desenhar a linha.** É aqui
    // que o teto vira erro na mão de quem pediu; montar tarde recusaria
    // calado, no meio de um retrato, e o MOD nunca saberia.
    let dar = js_function(&base, "function darCartoesDoMod(");
    assert!(
        dar.contains("regiao.declararCartoes(cartoes)"),
        "`darCartoesDoMod` deixou de montar no momento da declaração: {dar}"
    );
    assert!(
        dar.contains("redesenharAsPessoas()"),
        "declarar um cartão deixou de repintar a lista, e ele só apareceria no \
         próximo retrato: {dar}"
    );
    // **A região é criada aqui se ainda não existe, e não exigida.**
    //
    // Exigir que o MOD desenhasse antes era uma armadilha de ordem que a API
    // não conta a ninguém: um MOD que monta os cartões no mesmo ciclo em que
    // pinta o painel — que é o caso do PERFIS — recebia «não tem região de pé»
    // e não tinha como saber que bastava inverter duas linhas. Foi a corrida
    // nativa que pegou, e não um teste.
    assert!(
        dar.contains("regiaoDoMod(mod, instancia)"),
        "dar cartão voltou a exigir uma região que já exista: {dar}"
    );
    let desenhar = js_function(&base, "function desenharARegiaoDoMod(");
    assert!(
        desenhar.contains("regiaoDoMod(mod, instancia)"),
        "os dois caminhos deixaram de criar a região pelo mesmo lugar, e um \
         deles vai esquecer de registrá-la para a saída: {desenhar}"
    );
    // E criar a região para um cartão **não abre uma faixa vazia**: quem tira
    // o palco de escondido é quem desenha.
    let criar_regiao = js_function(&base, "function regiaoDoMod(mod, instancia)");
    assert!(
        !criar_regiao.contains("hidden = false"),
        "criar a região passou a mostrar o palco, e um MOD que só dá cartão \
         abre uma faixa vazia na tela: {criar_regiao}"
    );

    // **O descarte é o da região**, e alcança o que não está sob a raiz dela.
    let soltar = js_function(&regiao, "\n  soltar()");
    assert!(
        soltar.contains("this.soltarCartao(pessoa, raiz)"),
        "soltar a região deixou de tirar os cartões da lista do produto: {soltar}"
    );
    let limpar = js_function(&base, "function limparARegiaoDoMod(");
    assert!(
        limpar.contains("cartoesDosMods.delete(id)"),
        "o MOD saiu e a lista continuou achando que ele tem cartões: {limpar}"
    );

    // **Quem desenha a linha é o produto.** O nó chega montado e é anexado; o
    // MOD não escolhe posição, e nada de fora escreve estilo nele.
    let linha = js_function(&sessao, "function linhaNativaDoRoster(");
    assert!(
        linha.contains("for (const cartao of pessoa.cartoes ?? []) item.append(cartao);"),
        "a linha deixou de anexar o cartão montado: {linha}"
    );

    // **E a substituição não desfaz nenhuma das três garantias.**
    //
    // Um MOD pode apresentar a identidade de alguém (U27), e a linha
    // substituída continua sendo montada pelo produto: a raiz é `<li>`, o alvo
    // de clique carrega o `id` real, e o diagnóstico nativo recolhe em vez de
    // sumir. Sem estas asserções, «apresentar» viraria «esconder», e o caminho
    // até a moderação de alguém dependeria do que um terceiro desenhou.
    let substituida = js_function(&sessao, "function linhaSubstituida(");
    assert!(
        substituida.contains("porta.dataset.pessoaDaAcao = String(pessoa.id)"),
        "o alvo de clique de uma apresentação deixou de carregar o ID real, e \
         passou a depender do que o MOD desenhou: {substituida}"
    );
    assert!(
        substituida.contains("linhaNativaDoRoster(pessoa, temAudio, true)"),
        "o diagnóstico nativo sumiu de uma linha apresentada por um MOD, em vez \
         de recolher: {substituida}"
    );
    assert!(
        substituida.contains("porta.setAttribute(")
            && substituida.contains("aria-label"),
        "a linha apresentada por um MOD ficou sem nome acessível do produto: \
         {substituida}"
    );
    for fora in [
        "innerHTML",
        "insertAdjacentHTML",
        "style.position",
        "style.height",
    ] {
        assert!(
            !linha.contains(fora),
            "a linha do roster passou a alcançar `{fora}` por causa do cartão"
        );
    }

    // E o tamanho do cartão é da folha do produto, e não do MOD.
    let css = read("ui/tela-sessao.css");
    assert!(
        css.contains(".pessoa-cartao") && css.contains("max-height"),
        "o cartão perdeu o teto de altura, e uma lista de vinte pessoas deixa \
         de ser uma lista de pessoas"
    );
}

/// **O que a região cria nasce com dono, teto e descarte.**/// **O que a região cria nasce com dono, teto e descarte.**
///
/// A diretriz de 19/09: «cada recurso deve nascer com dono, limites e
/// descarte». As três coisas são verificáveis na forma, e a que mais some numa
/// refatoração é a terceira — um recurso novo entra com o que o cria e sem o
/// que o solta, e ninguém percebe até alguém sair da sessão.
#[test]
fn todo_recurso_da_regiao_nasce_registrado_e_com_teto() {
    let regiao = without_comments(&read("ui/mods-regiao.js"));

    // Toda forma que cria recurso passa pelo contador, e o contador tem teto.
    let criar = js_function(&regiao, "\n  criar(plano)");
    assert!(
        criar.contains("FORMAS_COM_TETO[plano.forma]")
            && criar.contains("this.contagem[classe] >= limite")
            && criar.contains("return null"),
        "a criação deixou de conferir o teto da forma: {criar}"
    );

    // E cada montagem que cria recurso o registra. `guardar` é a única porta:
    // ela amarra ao elemento **e** à instância, e é isso que faz a saída
    // encontrar o que a árvore já não alcança.
    for montagem in [
        "montarCampo(elem, plano)",
        "montarBotao(elem, plano)",
        "montarTela(elem, plano)",
        "montarMidia(elem, plano)",
    ] {
        let corpo = js_function(&regiao, &format!("\n  {montagem}"));
        assert!(
            corpo.contains("this.guardar("),
            "`{montagem}` cria recurso e não o registra, então a saída não o \
             encontra: {corpo}"
        );
    }
    let guardar = js_function(&regiao, "\n  guardar(elem, porque, descartar)");
    assert!(
        guardar.contains("this.recursos.set(elem,")
            && guardar.contains("this.dono.instancia?.registrar("),
        "`guardar` deixou de amarrar o descarte aos dois donos: o elemento, \
         para sair com o nó, e a instância, para sair com a sessão: {guardar}"
    );

    // O descarte da mídia para o som **e** tira a fonte. Só remover o nó deixa
    // os bytes decodificados presos, e um que ainda não começou pode começar.
    let midia = js_function(&regiao, "\n  montarMidia(elem, plano)");
    assert!(
        midia.contains("tocador.pause?.()") && midia.contains(r#"tocador.removeAttribute("src")"#),
        "o descarte da mídia deixou de parar e de tirar a fonte: {midia}"
    );
    // E o quadro pendente do arraste também é recurso.
    let tela = js_function(&regiao, "\n  montarTela(elem, plano)");
    assert!(
        tela.contains("cancelAnimationFrame(quadro)"),
        "o quadro pendente do arraste não é cancelado no descarte, e ele roda \
         uma vez depois de a sessão ter acabado: {tela}"
    );
}

/// **Atualizar não pode custar o foco de quem está digitando.**
///
/// A região montava com `replaceChildren`, e isso tira **todo** nó do documento
/// e o devolve: um campo em edição perdia foco, cursor, seleção e o que um IME
/// estivesse compondo, a cada atualização que o MOD mandasse. Um MOD que
/// redesenha a cada tecla era, por construção, um MOD em que não se digita.
#[test]
fn a_atualizacao_da_regiao_e_incremental_e_nao_reescreve_quem_tem_foco() {
    let regiao = without_comments(&read("ui/mods-regiao.js"));

    // **Duas exceções, e as duas são mantidas na forma exata.**
    //
    // A lista de opções de uma escolha é refeita de uma vez — mas só quando ela
    // mudou, e as `<option>` não seguram foco nem cursor: quem tem foco é o
    // `<select>`, que não é tocado.
    //
    // A tira de abas segue a mesma regra e pelo mesmo desenho: refeita só
    // quando a lista de abas mudou (`tiras.dataset.abas !== assinatura`), e o
    // que recebe foco ali são os botões da própria tira — refazê-los a cada
    // desenho tiraria o foco de quem percorre as abas com as setas, que é
    // exatamente o que o padrão APG manda preservar.
    //
    // Refazer a árvore de nós, que é o que este guarda existe para proibir,
    // continua proibido em todo o resto do arquivo.
    assert_eq!(
        regiao.matches("replaceChildren").count(),
        2,
        "a região refaz filhos fora da lista de opções e da tira de abas, e \
         refazer é perder o foco"
    );
    let abas = js_function(&regiao, "\n  atualizarAbas(elem, plano)");
    assert!(
        abas.contains("tiras.dataset.abas !== assinatura"),
        "a tira de abas voltou a ser refeita sem conferir se mudou, e refazê-la \
         tira o foco de quem percorre com as setas: {abas}"
    );
    let escolha = js_function(&regiao, "\n  atualizarEscolha(elem, plano)");
    assert!(
        escolha.contains("replaceChildren")
            && escolha.contains("caixa.dataset.opcoes !== assinatura"),
        "a lista de opções voltou a ser refeita sem conferir se mudou, e \
         refazê-la fecha a caixa de quem está escolhendo: {escolha}"
    );
    assert!(
        escolha.contains("if (document.activeElement === caixa) return;"),
        "a escolha de quem está com a caixa aberta voltou a ser reescrita: {escolha}"
    );
    let reconciliar = js_function(&regiao, "\n  reconciliar(pai, planos, fundura, orcamento)");
    assert!(
        reconciliar.contains("if (atual !== no) pai.insertBefore(no, atual);"),
        "a reconciliação voltou a mover nós que já estavam no lugar, e mover é \
         remover: {reconciliar}"
    );
    // O valor de uma caixa com foco não é reescrito. Reescrevê-lo move o cursor
    // para o fim mesmo quando o texto é igual — e um MOD que ecoa o que grava
    // tornaria a digitação impossível a partir da segunda letra.
    let campo = js_function(&regiao, "\n  atualizarCampo(elem, plano)");
    assert!(
        campo.contains("if (document.activeElement === caixa) return;"),
        "o valor de um campo em edição voltou a ser reescrito: {campo}"
    );
}

/// **O tema de um MOD não sai da sessão, e não passa por folha de estilo.**/// **O tema de um MOD não sai da sessão, e não passa por folha de estilo.**
///
/// ADR 0049. Escrito em `document.documentElement`, o tema ia para a tela de
/// entrada, para o launcher e para a bateria, e só saía se o MOD cooperasse.
///
/// Escrito como texto de CSS, ele seria pior: o valor vem de terceiro, e um
/// valor que vira texto de regra é CSS arbitrário na página. Por isso a escrita
/// é pela CSSOM, propriedade a propriedade — e a CSP desta janela recusaria um
/// `<style>` montado aqui de qualquer forma.
#[test]
fn o_tema_de_um_mod_fica_dentro_da_sessao() {
    let base = without_comments(&read("ui/base.js"));
    let aplicar = js_function(&base, "function aplicarOTemaDoMod(");
    let escrever = js_function(&base, "function escreverOTemaDaSessao(");

    assert!(
        escrever.contains("$(\"tela-sessao\")"),
        "o tema deixou de ser escrito no contêiner da sessão: {escrever}"
    );
    assert!(
        escrever.contains("setProperty") && escrever.contains("removeProperty"),
        "o tema deixou de ser escrito e apagado pela CSSOM: {escrever}"
    );
    for fora in [
        "documentElement",
        "document.body",
        "createElement(\"style\")",
    ] {
        assert!(
            !escrever.contains(fora) && !aplicar.contains(fora),
            "o tema de um MOD voltou a alcançar `{fora}`, e dali ele não sai \
             quando a sessão sai"
        );
    }

    // Só cor, e só `#rrggbb`: um valor livre numa propriedade de cor é a porta
    // por onde um MOD escreveria o que quisesse.
    assert!(
        aplicar.contains("COR_DO_TEMA.test(valor)"),
        "o tema deixou de conferir a forma da cor: {aplicar}"
    );

    // Os quatro nomes da API são os quatro que `escreverOTemaDaSessao` escreve,
    // e eles são tokens que `tokens.css` define. Um nome a mais aqui sem um
    // token do outro lado é uma propriedade que ninguém lê.
    let tema = base
        .split_once("const TEMA_DA_API = Object.freeze({")
        .expect("`TEMA_DA_API`")
        .1;
    let tema = tema.split_once("});").expect("o fim de `TEMA_DA_API`").0;
    let tokens = read("ui/tokens.css");
    let mut quantos = 0;
    for token in tema.split("\"--seele-").skip(1) {
        let nome = token.split('"').next().unwrap_or_default();
        assert!(
            tokens.contains(&format!("--seele-{nome}:")),
            "`TEMA_DA_API` redefine `--seele-{nome}`, que `tokens.css` não define"
        );
        quantos += 1;
    }
    // Seis desde a emenda de 19/09 do ADR 0049: `painel` e `apagado` existiam no
    // produto e faltavam na API, e um MOD de tema guardava seis cores no
    // servidor conseguindo aplicar quatro — o que ele punha na tela era um
    // aviso de que as outras duas «aguardam suporte».
    assert_eq!(
        quantos, 6,
        "a API de tema mudou de tamanho sem ADR que a mude"
    );
    let adr = read("../../docs/adr/0049-um-mod-deixa-de-rodar-na-janela-do-produto.md");
    assert!(
        adr.contains("`painel`, `apagado` e `densidade`"),
        "o ADR 0049 não registra o tamanho que a API de tema tem hoje"
    );
}

/// **Dois MODs não disputam o mesmo token em silêncio, e o ilegível é recusado.**
///
/// As duas recusas são da mesma família, e é a família que este repositório
/// paga caro quando falta: o produto sabe e não conta.
///
/// Se dois MODs pedem o acento, o produto não tem como saber qual deles quem
/// usa quis — escolher calado é escolher errado metade das vezes. E se o texto
/// não alcança o fundo, quem paga é quem está lendo a conversa, que não pediu
/// tema nenhum.
#[test]
fn o_tema_recusa_pelo_nome_em_vez_de_escolher_sozinho() {
    let base = without_comments(&read("ui/base.js"));
    let aplicar = js_function(&base, "function aplicarOTemaDoMod(");
    let mede = js_function(&base, "function oTextoAlcancaOFundo(");

    assert!(
        aplicar.contains("já é do MOD"),
        "o segundo MOD a pedir um token volta a sobrescrever o primeiro sem \
         dizer nada: {aplicar}"
    );
    assert!(
        aplicar.contains("throw new Error(legivel)"),
        "um tema ilegível deixou de ser recusado: {aplicar}"
    );
    assert!(
        mede.contains("4.5"),
        "a medida de contraste saiu, ou deixou de ser 4,5:1 — que é o que \
         `tokens.css` afirma linha a linha: {mede}"
    );
    // Lido da raiz: é o único lugar onde a cor do produto continua sendo a do
    // produto com um tema de pé. Medir na sessão mediria o próprio tema.
    assert!(
        mede.contains("getComputedStyle(document.documentElement)"),
        "a cor do produto passou a ser lida de onde o tema já escreveu: {mede}"
    );
}

/// E a faixa não fica de pé vazia, nem some com região de outro MOD dentro.
#[test]
fn a_faixa_das_regioes_aparece_com_a_primeira_e_sai_com_a_ultima() {
    let base = without_comments(&read("ui/base.js"));
    let desenhar = js_function(&base, "function desenharARegiaoDoMod(");
    let limpar = js_function(&base, "function limparARegiaoDoMod(");

    assert!(
        desenhar.contains("palco.hidden = false"),
        "a faixa não é revelada quando a primeira região entra: {desenhar}"
    );
    assert!(
        limpar.contains("palco.childElementCount === 0"),
        "a faixa some sem olhar se ainda há região de outro MOD dentro, ou \
         fica de pé vazia — uma régua de 1px em cima de nada: {limpar}"
    );
    assert!(
        limpar.contains("temaDosMods.delete(id)"),
        "descarregar um MOD deixa o tema dele de pé: {limpar}"
    );
}

/// **A janela não tem mais um `SeeleMods` global, e ninguém o lê.**
///
/// ADR 0049. `globalThis.SeeleMods` era a API que um MOD via por rodar na
/// página; com o MOD dentro de um worker, ele deixou de existir aqui.
///
/// Este guarda existe porque a falta dele custou um defeito nesta mesma
/// mudança: o próprio produto lia `globalThis.SeeleMods.request` para buscar o
/// catálogo do servidor, e a leitura sobreviveu à remoção do objeto. Nada
/// reprovou — a chamada não é a de uma função declarada, é um acesso a
/// propriedade de algo que virou `undefined` — e o defeito só apareceria ao
/// ligar um MOD: a lista inteira ficaria parada, sem frase nenhuma.
#[test]
fn a_janela_nao_le_um_seele_mods_global_que_ela_nao_tem_mais() {
    // Sem comentários: os comentários **explicam** o objeto que saiu, e um
    // guarda que não distingue os dois acusa a própria explicação.
    let sobraram: Vec<&str> = ["ui/base.js", "ui/camada-mods.js", "ui/tela-sessao.js"]
        .into_iter()
        .filter(|arquivo| {
            let fonte = without_comments(&read(arquivo));
            fonte.contains("globalThis.SeeleMods") || fonte.contains("globalThis.SeeleUI")
        })
        .collect();
    assert!(
        sobraram.is_empty(),
        "estes arquivos leem um global que o ADR 0049 tirou da janela, e o que \
         eles alcançam é `undefined`: {sobraram:?}"
    );
}

/// **A manutenção do disco é um bloco à parte, e não configuração de MOD.**
///
/// Plano de isolamento de 18/09: «gerenciar espaço do cache sem chamar isso de
/// configuração de MOD», e «remoção de cache referenciado por servidor deve ser
/// impedida ou exigir desinstalação explícita naquele servidor».
///
/// O cache é endereçado pelo conteúdo, então cada versão que passou por aqui
/// deixou uma pasta e nada nunca a tirava. Isso não é defeito do
/// endereçamento: é o preço dele.
#[test]
fn a_manutencao_do_disco_e_um_bloco_proprio_e_nao_apaga_o_que_o_servidor_exige() {
    let pagina = read("ui/index.html");
    for peca in [
        "ESPAÇO EM DISCO",
        "lista-cache",
        "cache-total",
        "cache-erro",
    ] {
        assert!(
            pagina.contains(peca),
            "`{peca}` saiu da página: a manutenção local não tem onde aparecer"
        );
    }

    let camada = without_comments(&read("ui/camada-mods.js"));
    let linha = js_function(&camada, "function linhaDePacoteNoCache(");
    // Um pacote em uso não ganha botão morto: ele ganha a frase que diz por quê.
    let rotulo = linha
        .find("EM USO POR ESTE SERVIDOR")
        .expect("a linha deixou de dizer que o pacote está em uso");
    let botao = linha
        .find("APAGAR")
        .expect("a linha deixou de saber apagar o que ninguém exige");
    assert!(
        rotulo < botao,
        "o botão é montado antes da pergunta «este servidor o exige?», então \
         apagar volta a ser oferecido para o que está em uso: {linha}"
    );

    // E a recusa não é só da tela. A tela desenha o que a lista disse **quando
    // ela desenhou**; entre desenhar e apertar, outro operador pode ter ligado
    // aquele pacote, e quem decide de verdade é o Rust.
    let rust = read("src/main.rs");
    let comando = rust
        .split_once("async fn apagar_pacote_do_cache(")
        .expect("o comando de apagar")
        .1;
    let comando = comando.split_once("\n}\n").map_or(comando, |(a, _)| a);
    assert!(
        comando.contains("mods_ligados(&session)") && comando.contains("exigido-por-este-servidor"),
        "o backend apaga sem perguntar se o servidor exige aquele pacote, e a \
         tela sozinha não é guarda: {comando}"
    );
}

/// **O que já foi aceito e sumiu do disco é buscado de volta — e só ele.**
///
/// Matriz de aceite do plano de 18/09: «aceite salvo e cache removido → o fluxo
/// obtém o hash exato antes de ficar pronto; falhas são visíveis».
///
/// Isso valia só no caminho que passa pela tela de aceite. Quem já tinha dito
/// sim entrava direto, e se o pacote não estivesse mais no disco — apagado pela
/// manutenção local, ou numa máquina que trocou — a sessão começava sem ele e
/// ninguém dizia nada.
///
/// **A conferência de identidade é a regra inteira.** O aceite autoriza um
/// conjunto exato, o que estava escrito na tela. Buscar qualquer coisa que o
/// servidor anuncie depois ampliaria aquele consentimento para uma lista que
/// ninguém leu — então só há busca quando a identidade anunciada agora é igual
/// à gravada para este destino.
#[test]
fn o_conjunto_ja_aceito_volta_ao_disco_e_nenhum_outro() {
    let base = without_comments(&read("ui/base.js"));
    let buscar = js_function(&base, "async function buscarOQueFaltaSeJaFoiAceito(");

    assert!(
        buscar.contains("aceite_de_mods") && buscar.contains("aceito !== catalogo.conjunto"),
        "a busca deixou de conferir que o conjunto anunciado é o mesmo a que \
         esta pessoa disse sim; assim ela amplia um consentimento antigo para \
         uma lista nova: {buscar}"
    );
    assert!(
        buscar.contains("instalar_mod_do_catalogo"),
        "a busca não busca nada: {buscar}"
    );
    // Uma tentativa por conjunto. O laço roda a cada quatro segundos, e sem
    // isto um catálogo fora do ar vira uma batida a cada quatro segundos,
    // para sempre, contra o servidor de outra pessoa.
    assert!(
        buscar.contains("conjuntoJaBuscado === catalogo.conjunto"),
        "a busca voltou a ser por tique, e não por conjunto: {buscar}"
    );
    let encerrar = js_function(&base, "function encerrarOAmbienteDosMods(");
    assert!(
        encerrar.contains("conjuntoJaBuscado = \"\""),
        "uma busca que falhou fica falhada para sempre: entrar de novo é o \
         gesto que pede «tente outra vez»: {encerrar}"
    );

    // E o servidor manda a identidade junto do catálogo: sem ela a janela não
    // tem com o que comparar, e a conferência acima seria sempre falsa.
    let pedidos = read("../../crates/seele-server/src/mods/pedidos.rs");
    assert!(
        pedidos.contains("\"conjunto\": identidade"),
        "a resposta do catálogo deixou de dizer qual conjunto ela descreve"
    );

    // As duas fases novas têm frase: uma fase sem frase não é desenhada, e o
    // defeito volta a ser silencioso.
    let camada = read("ui/camada-mods.js");
    for fase in ["obtendo", "nao-obtive"] {
        assert!(
            camada.contains(&format!("\"{fase}\"")) || camada.contains(&format!("{fase}:")),
            "a fase `{fase}` não tem frase no painel"
        );
    }
}

/// **Nada da sessão anterior é admitido na seguinte** — etapa E2.
///
/// O contrato de API própria, §5: «revogar a geração: a partir desse ponto,
/// nenhum novo efeito daquela instância é admitido, mesmo que mensagens já
/// estejam na fila».
///
/// A conferência que existia era `modsCarregados.has(id)`, e o roteiro da etapa
/// nomeia por que ela não basta: «presença de um ID no Map não identifica uma
/// geração». Sair e entrar de novo no **mesmo** servidor repõe o mesmo
/// identificador no mesmo mapa, e a montagem atrasada da visita anterior
/// encontrava a chave dela lá.
#[test]
fn a_geracao_da_sessao_prende_todo_trabalho_atrasado() {
    let base = without_comments(&read("ui/base.js"));

    // A montagem confere a geração, e não só a presença no mapa.
    let montar = js_function(&base, "async function montarOMod(");
    assert!(
        montar.contains("const geracao = geracaoDaSessao")
            && montar.contains("daGeracaoDePe(geracao)"),
        "a montagem de um MOD voltou a se identificar pelo `id`; uma montagem \
         atrasada sobe um worker em cima da sessão seguinte: {montar}"
    );
    assert!(
        montar.contains("new InstanciaDeMod(mod.id, mod.hash, geracao"),
        "a instância não nasce com a geração dela, e quem atende as mensagens \
         não tem como saber de que sessão ela é: {montar}"
    );

    // O atendimento confere **antes** do efeito, e não só antes da resposta.
    let atender = js_function(&base, "async function atenderOMod(");
    let antes_do_switch = atender.split_once("switch (m.tipo)").map_or("", |(a, _)| a);
    assert!(
        antes_do_switch.contains("instancia.admite(geracaoDaSessao)"),
        "o atendimento deixou de perguntar se a instância admite efeito: entre \
         o pedido de parada e a confirmação ela fica em `encerrando`, e nesse \
         intervalo nada pode ser admitido: {atender}"
    );
    // E sair **antes** do `switch`, não depois: uma mensagem que chegue durante
    // a saída não pode desenhar, pedir nem escrever tema para só então
    // descobrir que não tinha com quem falar.
    //
    // A saída deixou de ser um `return` calado — ela recusa, e o porquê está em
    // `um_pedido_de_mod_nunca_fica_sem_resposta` —, mas continua sendo uma
    // saída, e é isso que esta linha guarda.
    // Lido até o fim do trecho, e não até a primeira `}`: a recusa carrega um
    // literal de objeto, e a chave dele fecharia a leitura antes do `return`.
    let depois_da_recusa = antes_do_switch
        .split_once("if (!meu()) {")
        .map_or("", |(_, resto)| resto);
    assert!(
        !depois_da_recusa.is_empty() && depois_da_recusa.contains("return;"),
        "a conferência de instância voltou para depois do efeito, ou deixou de \
         sair antes do `switch`: {atender}"
    );

    // O pedido carrega a geração nas duas pontas: no que sobe e no que volta.
    let pedir = js_function(&base, "async function pedirAoServidor(");
    assert!(
        pedir.contains("geracao,") && pedir.contains("daGeracaoDePe(geracao)"),
        "o pedido ao servidor deixou de carregar a geração: {pedir}"
    );
    assert!(
        base.contains("if (!daGeracaoDePe(pending.geracao)) return;"),
        "uma resposta atrasada volta a ser entregue a quem ocupou o número do \
         pedido na sessão seguinte"
    );

    // E o encerramento revoga antes de desmontar.
    let encerrar = js_function(&base, "function encerrarOAmbienteDosMods(");
    let primeira = encerrar
        .lines()
        .map(str::trim)
        .find(|linha| !linha.is_empty() && *linha != "{")
        .unwrap_or_default();
    assert_eq!(
        primeira, "geracaoDaSessao = 0;",
        "o encerramento deixou de revogar na primeira linha; entre ela e a \
         última há mensagens em voo: {encerrar}"
    );
}

/// **O ciclo de vida não sabe o que executa o código de um MOD** — etapa E2.
///
/// A diretriz de 18/09: «manter a implementação atrás de um contrato interno de
/// executor: iniciar, entregar evento, solicitar encerramento e confirmar
/// encerramento».
///
/// O Worker de Blob **não vai ser o executor** — a sonda releu, num aplicativo
/// reiniciado, uma marca que um MOD tinha gravado em IndexedDB. Se o ciclo de
/// vida estivesse escrito em cima dele, trocá-lo seria reescrever revogação,
/// corrida de saída e descarte junto, e a parte difícil voltaria ao começo.
///
/// Este guarda é o que mantém a separação: o ciclo mora num arquivo, o
/// executor de hoje mora noutro, e `base.js` não pode voltar a falar `Worker`.
#[test]
fn o_ciclo_de_vida_de_um_mod_nao_conhece_o_executor() {
    let runtime = without_comments(&read("ui/mods-runtime.js"));
    let base = without_comments(&read("ui/base.js"));

    // Os quatro verbos do contrato existem, e o executor de hoje os implementa.
    for verbo in ["iniciar(", "entregar(", "pedirEncerramento(", "encerrou("] {
        assert!(
            runtime.contains(verbo),
            "o contrato de executor perdeu `{verbo}`: {runtime}"
        );
    }

    // **`base.js` não constrói mais um `Worker`.** Ele pede um executor, e
    // trocar qual é uma linha só — que é a razão de tudo isto existir.
    assert!(
        !base.contains("new Worker("),
        "`base.js` voltou a construir um `Worker` direto, e o executor virou \
         dependência estrutural outra vez"
    );
    // **E não há mais escolha para fazer.** O Worker de `blob:` foi reprovado
    // por medição — ele herda a origem de quem o cria, e o que um MOD gravou
    // nela sobreviveu ao encerramento do aplicativo. Guardá-lo como reserva
    // seria guardar, como reserva, exatamente o que não isola.
    assert!(
        base.contains("executorNativo(mod.id, geracao, mod.hash)"),
        "`base.js` deixou de pedir o executor pelo nome: {base}"
    );
    for sumido in ["executorDeWorker", "PRELUDIO_DO_MOD", "executor_de_mods"] {
        assert!(
            !base.contains(sumido),
            "`{sumido}` voltou a `base.js`, e com ele o executor reprovado"
        );
    }
    assert!(
        !runtime.contains("executorDeWorker"),
        "o executor de Worker voltou ao ciclo de vida"
    );

    // Os quatro estados, e a ordem que o §5 do contrato exige.
    for estado in ["criando", "ativa", "encerrando", "encerrada"] {
        assert!(
            runtime.contains(&format!("{estado}:")),
            "o estado `{estado}` saiu da instância"
        );
    }
    let encerrar = runtime
        .split_once("encerrar() {")
        .expect("`InstanciaDeMod.encerrar`")
        .1;
    let ate_o_fim = encerrar.split_once("\n  }").map_or(encerrar, |(a, _)| a);
    let marca = ate_o_fim
        .find("ESTADOS_DE_MOD.encerrando")
        .expect("o encerramento não marca `encerrando`");
    let pede = ate_o_fim
        .find("pedirEncerramento()")
        .expect("o encerramento não pede ao executor que pare");
    assert!(
        marca < pede,
        "a instância pede a parada antes de sair de `ativa`; nesse intervalo um \
         efeito ainda seria admitido: {ate_o_fim}"
    );
    let confirma = ate_o_fim
        .find("await this.executor.encerrou(")
        .expect("o encerramento não espera o executor confirmar");
    let descarta = ate_o_fim
        .find("recurso.descartar()")
        .expect("o encerramento não descarta os recursos");
    assert!(
        confirma < descarta,
        "os recursos são descartados antes de o executor confirmar que parou — \
         e ele ainda pode estar usando: {ate_o_fim}"
    );
    let marca_fim = ate_o_fim
        .rfind("ESTADOS_DE_MOD.encerrada")
        .expect("o encerramento nunca chega a `encerrada`");
    assert!(
        descarta < marca_fim,
        "a instância se diz `encerrada` antes de descartar; «até confirmar o \
         descarte dos recursos, não anunciar que tudo foi limpo»: {ate_o_fim}"
    );
}

/// **Confirmar a parada não é esperar um prazo** — revisão de `6cc58c1`.
///
/// A primeira integração corria a promessa de confirmação contra um timeout que
/// **resolvia com sucesso**: a instância se dizia `encerrada` sem que ninguém
/// tivesse parado nada, o ouvinte era removido, e o contador mostrava zero com
/// o motor ainda parando.
///
/// Revogar continua imediato — nenhum efeito é admitido nem em `encerrando` nem
/// em `encerrada`. O que muda é o que o produto **afirma**.
#[test]
fn a_parada_confirmada_e_diferente_do_prazo_vencido() {
    let runtime = without_comments(&read("ui/mods-runtime.js"));

    // O prazo devolve `false`, e não resolve como se tivesse confirmado.
    let nativo = runtime
        .split_once("function executorNativo(")
        .expect("o executor nativo")
        .1;
    assert!(
        nativo.contains("resolve(false)"),
        "o prazo do encerramento voltou a resolver como sucesso: {nativo}"
    );
    assert!(
        nativo.contains("parada.then(() => true)"),
        "a confirmação de verdade deixou de ser distinguida do prazo: {nativo}"
    );
    // E a confirmação que chega **tarde** conclui: sem isto, uma instância que
    // parou depois do prazo ficava em `encerrando` para sempre, com o ouvinte
    // de pé e o diagnóstico acusando sobra de um MOD que já tinha parado.
    assert!(
        runtime.contains("concluirTardio()"),
        "a confirmação tardia voltou a só resolver a promessa, sem concluir"
    );

    // E a instância só se diz `encerrada` com confirmação **e** sem recurso que
    // tenha falhado ao sair.
    let encerrar = runtime
        .split_once("encerrar() {")
        .expect("`InstanciaDeMod.encerrar`")
        .1;
    let encerrar = encerrar.split_once("\n  }").map_or(encerrar, |(a, _)| a);
    assert!(
        encerrar.contains("confirmou && this.naoSairam.length === 0"),
        "a instância voltou a se dizer `encerrada` sem confirmação: {encerrar}"
    );
    assert!(
        encerrar.contains("this.naoSairam.push"),
        "um recurso que falha ao sair volta a não deixar rastro: {encerrar}"
    );

    // E o diagnóstico conta as três coisas, não só a primeira.
    let contagem = js_function(&runtime, "function recursosDePe(");
    for exigido in ["naoSairam.length", "ESTADOS_DE_MOD.encerrando"] {
        assert!(
            contagem.contains(exigido),
            "o diagnóstico deixou de ver `{exigido}`, e um encerramento sem \
             confirmação volta a aparecer como limpo: {contagem}"
        );
    }
}

/// **Uma instância nativa tem identidade própria, e ela não é o nome do MOD.**
///
/// Revisão de `6cc58c1`: «um pedido antigo pode atingir a nova instância com o
/// mesmo nome». Sair de um servidor e entrar noutro que exige o **mesmo MOD**
/// repõe o mesmo texto, e um encerramento a caminho matava a execução nova.
///
/// Recarregar um MOD é o mesmo problema dentro de uma geração só: duas
/// execuções, o mesmo nome, e nada para distingui-las.
#[test]
fn uma_instancia_nativa_e_identificada_pelo_numero_dela() {
    let runtime = without_comments(&read("ui/mods-runtime.js"));
    let nativo = runtime
        .split_once("function executorNativo(")
        .expect("o executor nativo")
        .1;

    assert!(
        nativo.contains("payload.instancia !== numero"),
        "o ouvinte voltou a casar as falas pelo nome do MOD: {nativo}"
    );
    for verbo in ["instancia: numero", "instancia: numero"] {
        assert!(
            nativo.contains(verbo),
            "um verbo do executor nativo voltou a falar por nome: {nativo}"
        );
    }
    assert!(
        !nativo.contains("mod_nativo_encerrar\", { id }"),
        "encerrar voltou a ser por nome, e mata a execução seguinte do mesmo MOD"
    );
    // **Sem recuo por nome durante a subida.** Era por ele que uma instância
    // anterior do mesmo MOD falava na janela em que a nova esperava o número.
    assert!(
        nativo.contains("if (numero === null || payload.instancia !== numero) return;"),
        "o recuo por nome voltou, e com ele a fala de uma instância antiga: {nativo}"
    );
    // E é a **colheita** que traz a mensagem: o aviso não carrega carga.
    assert!(
        nativo.contains("mod_nativo_colher"),
        "a janela voltou a receber a carga pelo evento, e o crédito volta a ser \
         devolvido antes de alguém ler: {nativo}"
    );

    // E o Rust guarda a identidade inteira: nome, geração e pacote.
    let rust = read("src/main.rs");
    let estrutura = rust
        .split_once("struct InstanciaNativa {")
        .expect("`InstanciaNativa`")
        .1;
    let estrutura = estrutura.split_once("\n}").map_or(estrutura, |(a, _)| a);
    for campo in [
        "id: String",
        "geracao: u64",
        "hash: String",
        "estado: EstadoNativo",
        "pendentes: std::collections::VecDeque<FalaPendente>",
    ] {
        assert!(
            estrutura.contains(campo),
            "a identidade da instância nativa perdeu `{campo}`: {estrutura}"
        );
    }
}

/// **Registrar vem antes de liberar a execução, e dentro do mesmo cadeado.**
///
/// Revisão de `6cc58c1`, duas metades. «A bomba pode emitir uma mensagem antes
/// de existir um destino registrado para a resposta» — e uma inserção podia
/// acontecer **depois** da saída, deixando um executor órfão na tabela de uma
/// sessão que já acabou.
///
/// Conferir a geração fora do cadeado não bastaria: entre a conferência e a
/// inserção cabe uma desmontagem inteira.
#[test]
fn o_executor_nativo_e_admitido_antes_de_rodar_e_junto_com_a_revogacao() {
    let rust = read("src/main.rs");
    // **Duas etapas**: reservar dá o número sem rodar nada, ativar libera a
    // execução. É o que fecha a corrida de identidade sem perder as primeiras
    // mensagens legítimas — entre as duas, não existe fala para ouvir.
    let reservar = rust
        .split_once("fn mod_nativo_reservar(")
        .expect("`mod_nativo_reservar`")
        .1;
    let reservar = reservar.split_once("\n/// ").map_or(reservar, |(a, _)| a);
    assert!(
        !reservar.contains(".iniciar(&codigo)"),
        "a reserva voltou a rodar o código, e o número deixa de chegar antes \
         da primeira fala: {reservar}"
    );
    assert!(
        reservar.contains("registrar_reserva("),
        "a reserva não registra a instância: {reservar}"
    );

    // A coordenação mora em `registrar_reserva` — extraída do comando para que
    // a corrida contra a revogação possa ser provada com threads de verdade,
    // sem `AppHandle`. É lá que a conferência de geração tem de estar
    // **dentro** do cadeado que insere.
    let registrar = rust
        .split_once("fn registrar_reserva(")
        .expect("`registrar_reserva`")
        .1;
    let registrar = registrar.split_once("\n/// ").map_or(registrar, |(a, _)| a);
    let trecho = registrar
        .split_once("mods.lock()")
        .expect("o cadeado da tabela")
        .1;
    let trecho = trecho
        .split_once("vivos.guardar(")
        .map_or(trecho, |(a, _)| a);
    assert!(
        trecho.contains("geracao_vale()"),
        "a geração voltou a ser conferida fora do cadeado, e entre a \
         conferência e a inserção cabe uma desmontagem inteira: {trecho}"
    );

    // E a ativação confere a revogação de novo: entre as duas etapas a sessão
    // pode ter acabado.
    let ativar = rust
        .split_once("fn liberar_reserva(")
        .expect("`liberar_reserva`")
        .1;
    let ativar = ativar.split_once("\n/// ").map_or(ativar, |(a, _)| a);
    assert!(
        ativar.contains("geracao_vale()") && ativar.contains(".iniciar_com_api(&codigo, api)"),
        "a ativação deixou de conferir a revogação antes de liberar a \
         execução: {ativar}"
    );
    // **E a API que sobe é a que o pacote declarou**, e não a deste build.
    // Aceitar a API 3 não é dar à API 3 o que a 4 tem: o prelúdio monta
    // `SeeleUI` a partir deste número, e trocá-lo por `MOD_API_VERSION` daria
    // superfícies e contribuições a um pacote que não as declarou.
    assert!(
        ativar.contains("alvo.api_declarada"),
        "a ativação passou a subir o MOD com uma API que não é a do manifesto \
         dele: {ativar}"
    );

    // E a revogação nativa não espera a janela pedir — e fecha a geração
    // **dentro** do cadeado. Fora dele, entre soltar o cadeado e incrementar
    // abre-se uma janela por onde entra uma reserva viva numa sessão que
    // acabou; é uma janela de poucas instruções, que nenhuma disputa por
    // thread alcança, e por isso a forma é o que a guarda.
    let revogar = rust.split_once("fn revogar_em(").expect("`revogar_em`").1;
    let revogar = revogar.split_once("\n/// ").map_or(revogar, |(a, _)| a);
    assert!(
        revogar.contains("revogar_geracao"),
        "a revogação nativa voltou a depender de a janela chamar o encerramento: {revogar}"
    );
    let sob_cadeado = revogar
        .split_once("let Ok(mut vivos) = mods.lock()")
        .expect("o cadeado da revogação")
        .1;
    assert!(
        sob_cadeado.contains("geracao.fetch_add(")
            && sob_cadeado.find("geracao.fetch_add(") < sob_cadeado.find("revogar_geracao"),
        "a geração voltou a ser fechada fora do cadeado que varre as \
         instâncias dela: {revogar}"
    );
}

/// **A saída local revoga antes de esperar o `disconnect`.**
///
/// `disconnect` é um `await` sobre a ponte: ele derruba a hospedagem e espera a
/// porta voltar. Enquanto ele corria, a sessão ainda era a de pé para esta
/// janela — e uma mensagem de worker que chegasse ali desenhava, pedia ao
/// servidor e escrevia tema, tudo depois de a pessoa ter apertado sair.
#[test]
fn sair_revoga_antes_de_esperar_a_ponte() {
    let sessao = without_comments(&read("ui/tela-sessao.js"));
    let ejetar = js_function(&sessao, "async function ejetar(");
    let encerrar = ejetar
        .find("encerrarOAmbienteDosMods()")
        .expect("a saída local deixou de encerrar o ambiente dos MODs");
    let desconectar = ejetar
        .find("invoke(\"disconnect\")")
        .expect("a saída local deixou de desconectar");
    assert!(
        encerrar < desconectar,
        "a saída local voltou a esperar a ponte antes de revogar, e a janela \
         continua admitindo efeito durante a espera: {ejetar}"
    );
}

/// **O Rust recusa o que vem de uma sessão que já acabou, e conta.**
///
/// A metade nativa da mesma regra. A janela confere o que nunca chega ao Rust —
/// um `await` que volta, um desenho — e o Rust confere o que chega: um comando
/// despachado antes da saída pode aterrissar depois dela.
///
/// O contador existe porque o contrato o pede: «sem essa observabilidade, "não
/// sobrou nada" vira inspeção visual insuficiente». Recusar em silêncio é o
/// certo a fazer com o comando e o errado a fazer com a informação.
#[test]
fn o_rust_recusa_comando_de_geracao_morta_e_conta() {
    let rust = read("src/main.rs");

    assert!(
        rust.contains("fn revogar(&self) -> u64"),
        "a sessão deixou de ter como revogar a geração"
    );
    // Revogar é a **primeira** coisa que a desmontagem faz.
    let desmontar = rust
        .split_once("fn desmontar_o_cliente(")
        .expect("`desmontar_o_cliente`")
        .1;
    let desmontar = desmontar.split_once("\n}\n").map_or(desmontar, |(a, _)| a);
    let revoga = desmontar
        .find("session.revogar()")
        .expect("a desmontagem deixou de revogar");
    let toma = desmontar
        .find(".and_then(|mut slot| slot.take())")
        .expect("a desmontagem deixou de tirar a conexão do slot");
    assert!(
        revoga < toma,
        "a desmontagem voltou a destruir a conexão antes de revogar, e entre \
         as duas coisas há efeito sendo admitido: {desmontar}"
    );

    // Os dois comandos que um MOD alcança conferem a geração.
    for (comando, marca) in [
        ("fn mod_request(", "session.confere_geracao(geracao)?"),
        ("fn codigo_do_mod(", "session.geracao_vale(geracao)"),
    ] {
        let corpo = rust.split_once(comando).expect(comando).1;
        let corpo = corpo.split_once("\n}\n").map_or(corpo, |(a, _)| a);
        assert!(
            corpo.contains(marca),
            "`{comando}` deixou de conferir a geração de quem chamou"
        );
    }

    // E a ponte de eventos não fala em nome de uma sessão morta.
    let ponte = rust
        .split_once("impl EventListener for Bridge")
        .expect("a ponte de eventos")
        .1;
    let ponte = ponte.split_once("\n}\n").map_or(ponte, |(a, _)| a);
    assert!(
        ponte.contains("self.geracao != self.de_pe.load"),
        "a ponte voltou a emitir todo evento para a janela, inclusive os de uma \
         conexão que já caiu: {ponte}"
    );
    assert!(
        ponte.contains("descartados"),
        "a ponte descarta em silêncio e não conta: {ponte}"
    );
}

/// E o esquema não ficou registrado no Rust depois de sair da CSP.
///
/// Os dois lados são uma regra só: um `register_uri_scheme_protocol("mod", …)`
/// sem `mod:` na política é código que a página não alcança hoje e alcança no
/// dia em que alguém puser a diretiva de volta «para destravar um MOD».
#[test]
fn o_esquema_dos_mods_nao_ficou_registrado_sem_ninguem_para_usa_lo() {
    let rust = read("src/main.rs");
    assert!(
        !rust.contains("register_uri_scheme_protocol(\"mod\""),
        "o esquema `mod://` continua registrado, e desde o ADR 0049 não há \
         nada na janela que o chame"
    );
}

/// A página carrega MOD, e só depois de haver página.
#[test]
fn the_page_loads_mods_after_the_first_draw() {
    let base = read("ui/base.js");
    assert!(base.contains("mods_instalados"), "base.js não lista MODs");
    // **O esquema `mod://` não existe mais** — o ADR 0049 o tirou junto com o
    // MOD que rodava na janela. Este guarda exigia o nome dele e passava por
    // causa de um comentário: um guarda que uma frase em prosa satisfaz guarda
    // uma frase. O que ele quer dizer é que os bytes vêm pela ponte.
    let montar = js_function(&without_comments(&base), "async function montarOMod(");
    assert!(
        montar.contains("invoke(\"codigo_do_mod\""),
        "os bytes de um MOD deixaram de vir pela ponte, onde a conferência de \
         hash mora: {montar}"
    );
    assert!(
        !without_comments(&base).contains("mod://"),
        "`mod://` voltou a `base.js`, e o ADR 0049 tirou o esquema"
    );

    let primeiro_desenho = base.find("desenhar(await invoke(\"snapshot\"))");
    let carga = base.find("carregarMods();");
    let (Some(desenho), Some(carga)) = (primeiro_desenho, carga) else {
        panic!("não achei o primeiro desenho ou a chamada de carregarMods");
    };
    assert!(
        carga > desenho,
        "os MODs carregam antes de a página existir, e um MOD que roda cedo \
         falha de um jeito que parece defeito dele"
    );
}

/// As duas listas de aparelho dizem que escolher pelo nome fixa.
///
/// # Por que esta frase precisa de um guarda
///
/// Ela é o **conserto inteiro** do achado B da auditoria de áudio. A tela
/// oferecia `PADRÃO DA MÁQUINA` e o nome de um aparelho lado a lado, como se
/// escolher qualquer um dos dois desse no mesmo — e no macOS não dá: um
/// aparelho vindo da lista abre por um AudioUnit fixo e perde o
/// reencaminhamento que o CoreAudio faz sozinho no padrão. A auditoria
/// classificou como gravidade alta com esta frase: *«quem usa a tela de
/// configuração fica pior do que quem não usa»*.
///
/// Escolheu-se dizer em vez de promover a escolha a seguidora, porque promover
/// trocaria um defeito de informação por um de comportamento — uma escolha
/// explícita viraria um seguidor calado. A escolha está justificada por escrito
/// em `crates/seele-conformance/tests/troca_de_aparelho.rs`, e os dois testes
/// de comportamento que a tornam verdade estão lá:
/// `com_um_aparelho_escolhido_a_troca_do_padrao_do_sistema_nao_move_a_voz` e
/// `o_aparelho_escolhido_que_some_desce_para_o_da_maquina_em_vez_de_emudecer`.
///
/// Sem este guarda, apagar a frase devolve a piora invisível sem nada ficar
/// vermelho — que é como ela viveu até aqui.
#[test]
fn as_listas_de_aparelho_avisam_que_escolher_pelo_nome_fixa() {
    let page = without_comments(&read("ui/index.html"));

    for id in ["nota-fixa-entrada", "nota-fixa-saida"] {
        assert!(
            page.contains(id),
            "sumiu a nota `{id}`: a tela voltou a oferecer `PADRÃO DA MÁQUINA` \
             e um nome de aparelho como se fossem a mesma coisa, e no macOS \
             escolher pelo nome desliga o reencaminhamento automático"
        );
    }
    // **Duas vezes o aviso, uma vez a explicação** — U30 da auditoria de
    // 20/09/2026: «Em Áudio, a explicação longa sobre acompanhar o dispositivo
    // padrão se repete para entrada e saída.»
    //
    // O que não podia sair era o **aviso**, que é o conserto do achado B acima.
    // Ele encurtou e continua ao lado das duas listas; o parágrafo que o
    // explica ficou uma vez, recolhido, no fim do painel. Recolher o aviso
    // junto teria escondido a informação que faz a escolha ser informada.
    assert_eq!(
        page.matches("fixa\n                  ele").count()
            + page.matches("fixa ele").count(),
        2,
        "o aviso de fixação existe em número diferente de dois: as duas listas \
         — entrada e saída — têm o mesmo comportamento e precisam do mesmo aviso"
    );
    assert_eq!(
        page.matches("<b>PADRÃO DA MÁQUINA</b> é a opção").count(),
        1,
        "a explicação longa voltou a existir duas vezes, ou sumiu: ela vale \
         igual para os dois lados e mora uma vez, recolhida"
    );
    assert!(
        page.contains("<b>PADRÃO DA MÁQUINA</b> é a opção"),
        "a nota deixou de nomear a opção que acompanha a troca. Dizer que \
         escolher fixa, sem dizer qual é a alternativa, é meio aviso: a pessoa \
         fica sabendo que perdeu algo e não como recuperá-lo"
    );
}

#[test]
fn nenhuma_tabela_de_frases_declara_a_mesma_chave_duas_vezes() {
    // **Como isto foi encontrado, e por que dói calado.**
    //
    // Um objeto literal em JavaScript aceita a mesma chave duas vezes sem
    // reclamar, e a última vence. Acrescentei uma frase para a assinatura do
    // catálogo de MODs chamada `AssinaturaRecusada`, e o nome já era de outra
    // coisa — o pacote do atualizador. Nada ficou vermelho. O que a pessoa lia
    // ao receber um catálogo adulterado era «O PACOTE BAIXADO NÃO FOI ASSINADO
    // POR ESTE PROJETO»: uma frase sobre um download que ela não tinha feito,
    // sem a única instrução que aquele momento exige, que é **parar**.
    //
    // É a pior forma do defeito que este repositório mais persegue — o produto
    // sabendo e contando outra coisa — e o custo de achá-lo foi um roteiro que
    // imprimiu a frase errada. Sem ele, ninguém veria.
    //
    // Cobrado sobre **todas** as tabelas de frases e não sobre a lista de hoje:
    // uma quarta escrita amanhã tem de nascer com isto.
    let fonte = read("ui/frases.js");

    for tabela in [
        "MOTIVOS",
        "FRASES",
        "NOMES_RECUSADOS",
        "MOTIVOS_DE_REVOGACAO",
    ] {
        let agulha = format!("const {tabela} = {{");
        let Some(inicio) = fonte.find(&agulha) else {
            panic!("a tabela `{tabela}` sumiu de frases.js, e este guarda ficou cego");
        };
        let corpo = &fonte[inicio + agulha.len()..];
        // Até a primeira linha que fecha na coluna zero: as tabelas são
        // literais de topo, e o `};` do fim delas é o único nessa coluna.
        let fim = corpo.find("\n};").unwrap_or(corpo.len());
        let corpo = &corpo[..fim];

        let mut vistas: BTreeSet<String> = BTreeSet::new();
        let mut repetidas: Vec<String> = Vec::new();
        for linha in corpo.lines() {
            let sem_espaco = linha.trim_start();
            // Só o que abre uma entrada: um identificador seguido de dois
            // pontos. Comentários e continuações de frase não entram — uma
            // continuação começa com aspas, e um comentário com barras.
            let Some((nome, _)) = sem_espaco.split_once(':') else {
                continue;
            };
            let nome = nome.trim().trim_matches('"');
            if nome.is_empty()
                || sem_espaco.starts_with("//")
                || sem_espaco.starts_with('*')
                || !nome
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            {
                continue;
            }
            if !vistas.insert(nome.to_owned()) {
                repetidas.push(nome.to_owned());
            }
        }
        assert!(
            repetidas.is_empty(),
            "`{tabela}` declara a mesma chave duas vezes: {repetidas:?}\n\
             A última vence, em silêncio, e quem lê a tela recebe a frase de \
             outra falha. Dê um nome próprio a cada uma."
        );
    }
}

/// Toda tela que desenha uma `.boot-caixa` sabe rolar sozinha.
///
/// **O defeito, medido e não deduzido.** `body` é `overflow: hidden` e `.tela`
/// nasce `overflow-y: visible`, com `height: calc(100vh - barra)`. Uma tela
/// mais alta que a janela não ganha barra: ela é **cortada em silêncio**, e
/// nada na tela diz que há mais embaixo.
///
/// A `.boot-caixa` é justamente o padrão que cresce — uma coluna de campos numa
/// largura fixa. Numa janela de 560px, a tela de entrada com os dois
/// `<details>` abertos (quem tem um DNS apontado e mais de uma versão) dá
/// 1074px de conteúdo: o botão PERFIL e a engrenagem de CONFIGURAÇÕES ficavam
/// fora do alcance, sem rolagem possível. A tela de preparar o servidor nasceu
/// com o mesmo problema e foi o que fez a medida acontecer.
///
/// O guarda é sobre a declaração porque é a declaração que falta: quem
/// acrescentar a terceira tela de cartão vai esquecer a mesma linha.
#[test]
fn toda_tela_de_cartao_declara_a_propria_rolagem() {
    let page = read("ui/index.html");
    let css = ui_files(".css")
        .iter()
        .map(|name| read(&format!("ui/{name}")))
        .collect::<Vec<_>>()
        .join("\n");

    // As telas que desenham o cartão, colhidas da página e não escritas aqui.
    let mut de_cartao: BTreeSet<String> = BTreeSet::new();
    let mut atual: Option<String> = None;
    for linha in page.lines() {
        if let Some(resto) = linha.split_once("<section id=\"tela-").map(|(_, r)| r) {
            atual = resto.split('"').next().map(|id| format!("tela-{id}"));
        }
        if linha.contains("boot-caixa") {
            if let Some(id) = &atual {
                de_cartao.insert(id.clone());
            }
        }
    }
    assert!(
        !de_cartao.is_empty(),
        "nenhuma tela desenha `.boot-caixa`; se o padrão saiu, este guarda sai junto"
    );

    // O seletor tem de ser o **identificador inteiro**, e não um prefixo dele.
    // A primeira versão casava por substring, e a prova por reversão passou
    // verde contra `#tela-preparar-DESLIGADO` — um guarda que erra para o lado
    // do «está tudo bem» é pior que guarda nenhum, porque ninguém vai conferir.
    let cita_o_id = |seletores: &str, id: &str| {
        let alvo = format!("#{id}");
        seletores.match_indices(&alvo).any(|(at, _)| {
            let depois = seletores[at + alvo.len()..].chars().next();
            !matches!(depois, Some(c) if c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
    };
    let declara_rolagem = |id: &str| {
        css.split('}').any(|bloco| {
            let Some((seletores, corpo)) = bloco.split_once('{') else {
                return false;
            };
            cita_o_id(seletores, id) && corpo.contains("overflow-y")
        })
    };
    let sem_rolagem: Vec<&String> = de_cartao.iter().filter(|id| !declara_rolagem(id)).collect();

    assert!(
        sem_rolagem.is_empty(),
        "estas telas desenham um cartão que cresce e não declaram `overflow-y`, \
         então o que passar da janela é cortado sem barra e sem aviso: {sem_rolagem:?}"
    );
}

/// A barra da janela arrasta pelo mecanismo que o macOS entende.
///
/// **Relatado do uso real:** «não consigo arrastar o SEELE pela barra superior
/// no Mac».
///
/// O CSS pede `-webkit-app-region: drag`, e isso é um recurso do Chromium: o
/// WebView2 do Windows o entende, o **WKWebView do macOS não**. Arrastar pela
/// nossa barra nunca funcionou lá — o que havia era a faixa de título do
/// sistema, e com `titleBarStyle: Overlay` o conteúdo da página fica por cima
/// dela.
///
/// O mecanismo do Tauri é `data-tauri-drag-region`, e ele **precisa das duas
/// metades**: o atributo na página e `core:window:allow-start-dragging` na
/// capacidade. Faltando a segunda, o arrasto é recusado em silêncio — que é a
/// forma exata do defeito que este repositório mais paga caro.
///
/// O guarda cobra as duas juntas porque é só juntas que elas valem.
#[test]
fn a_barra_da_janela_arrasta_nos_dois_mecanismos() {
    let page = read("ui/index.html");
    let css = ui_files(".css")
        .iter()
        .map(|name| read(&format!("ui/{name}")))
        .collect::<Vec<_>>()
        .join("\n");
    let capacidade = read("capabilities/janela.json");

    assert!(
        css.contains("-webkit-app-region: drag"),
        "sem isto a barra não arrasta no Windows, onde o WebView2 é Chromium"
    );
    assert!(
        page.contains("data-tauri-drag-region"),
        "sem isto a barra não arrasta no macOS, onde o WKWebView ignora `-webkit-app-region`"
    );
    assert!(
        capacidade.contains("core:window:allow-start-dragging"),
        "o atributo está na página e a permissão não foi concedida: o Tauri recusa o \
         arrasto sem dizer nada, e a barra fica parecendo que funciona"
    );

    // E o atributo alcança o que está **escrito** na barra, e não só a moldura
    // dela: o Tauri confere o alvo do clique, então um `<span>` sem o atributo
    // é um pedaço da barra que não arrasta. É quase toda a barra.
    for dentro in ["barra-marca", "barra-servidor-nome", "barra-relogio"] {
        let Some(at) = page.find(dentro) else {
            panic!("`{dentro}` saiu da barra; se foi de propósito, tire-o daqui também");
        };
        let linha = page[..at].rfind('<').map_or("", |de| {
            let fim = page[de..].find('>').map_or(page.len(), |f| de + f);
            page.get(de..fim).unwrap_or("")
        });
        assert!(
            linha.contains("data-tauri-drag-region"),
            "`{dentro}` é um pedaço da barra que não arrasta: {linha}"
        );
    }
}

/// Nenhum `id` aparece duas vezes na página.
///
/// **A01 da auditoria de 17/09, e o defeito é exatamente este guarda faltando.**
/// `#tela-servidores` nasceu com `servidores-titulo` e `servidores-erro`, que
/// já existiam no diálogo ONDE VOCÊ JÁ ESTEVE. `document.getElementById`
/// devolve o **primeiro**, e o primeiro passou a ser o da tela escondida: um
/// convite malformado escrevia o erro num elemento que ninguém vê, e o diálogo
/// ficava aberto sem dizer nada.
///
/// O mesmo empate fazia o leitor de tela anunciar aquele diálogo como
/// «HOSPEDAR», porque o `aria-labelledby` resolvia para o título errado.
///
/// Nada disso lança. É o «o produto sabe e não conta» na forma mais silenciosa
/// que existe: a mensagem foi escrita, e foi escrita no lugar errado.
#[test]
fn nenhum_id_aparece_duas_vezes_na_pagina() {
    let page = read("ui/index.html");

    let mut vistos: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut resto = page.as_str();
    while let Some(at) = resto.find("id=\"") {
        resto = &resto[at + 4..];
        let Some(fim) = resto.find('"') else { break };
        let Some(id) = resto.get(..fim) else { break };
        *vistos.entry(id.to_owned()).or_default() += 1;
        resto = &resto[fim..];
    }

    let repetidos: Vec<String> = vistos
        .iter()
        .filter(|(_, n)| **n > 1)
        .map(|(id, n)| format!("`{id}` ({n}×)"))
        .collect();

    assert!(
        repetidos.is_empty(),
        "estes `id` aparecem mais de uma vez, e `getElementById` devolve sempre \
         o primeiro — a segunda superfície escreve no elemento da primeira, sem \
         erro nenhum: {}",
        repetidos.join(", ")
    );
}

/// RECONECTAR não passa pelo verbo que derruba a hospedagem.
///
/// **A03 da auditoria de 17/09.** `reconectar` chamava `limparSessaoEncerrada`,
/// que chamava `disconnect` — e `disconnect` recolhe a conexão **e** a
/// hospedagem, com `encerrar().await`. Em seguida ele tentava entrar no
/// endereço que acabara de tirar do ar. Quem hospedava e teve a sessão local
/// encerrada, inclusive por uma troca de MODs, apertava um botão que promete
/// voltar e desligava o próprio servidor, levando junto quem estava dentro.
///
/// **O que este guarda não prova.** A aceitação que a auditoria pede é de duas
/// máquinas — anfitrião e convidado, encerrar só a sessão local, ver que a
/// escuta continua e o convidado não cai. Isso precisa de campo. O que se cobra
/// aqui é a única metade que um teste de texto alcança: que a intenção esteja
/// escrita na chamada, e que ninguém possa chamar a limpeza sem escolher.
#[test]
fn reconectar_desmonta_a_sessao_sem_fechar_o_servidor() {
    let fonte = without_comments(&read("ui/tela-fim.js"));

    let corpo = |nome: &str| -> String {
        let Some(de) = fonte.find(&format!("function {nome}(")) else {
            panic!("`{nome}` saiu de `tela-fim.js`; se foi de propósito, ajuste este guarda");
        };
        // Até a próxima declaração de topo, que é o fim prático do corpo. O
        // `+1` pula o `f` do próprio `function`, para que a busca pela próxima
        // não encontre esta — e a fatia começa em `de` para a mensagem de falha
        // sair com o nome inteiro.
        let resto = fonte.get(de..).unwrap_or("");
        let busca = resto.get(1..).unwrap_or("");
        let ate = busca
            .find("\nasync function ")
            .or_else(|| busca.find("\nfunction "))
            .map(|n| n + 1);
        resto
            .get(..ate.unwrap_or(resto.len()))
            .unwrap_or("")
            .to_owned()
    };

    let reconectar = corpo("reconectar");
    assert!(
        reconectar.contains("pararDeHospedar: false"),
        "RECONECTAR tem de dizer que a hospedagem fica de pé: {reconectar}"
    );

    let sair = corpo("sairParaAEntrada");
    assert!(
        sair.contains("pararDeHospedar: true"),
        "SAIR fecha o assunto, e o anfitrião fecha junto: {sair}"
    );

    // E ninguém chama a limpeza sem escolher: um `limparSessaoEncerrada()` sem
    // argumento estoura ao desestruturar, e um com o padrão errado seria o
    // defeito de volta em silêncio.
    for (at, _) in fonte.match_indices("limparSessaoEncerrada(") {
        // A própria declaração casa com o mesmo texto, e ela é a que **recebe**
        // o argumento em vez de passá-lo.
        if fonte
            .get(..at)
            .is_some_and(|antes| antes.ends_with("function "))
        {
            continue;
        }
        let depois = fonte
            .get(at + "limparSessaoEncerrada(".len()..)
            .unwrap_or("");
        assert!(
            depois.starts_with("{ pararDeHospedar:"),
            "esta chamada a `limparSessaoEncerrada` não diz o que fazer com a hospedagem"
        );
    }
}

/// O que falha ao carregar um MOD chega à tela, e a URL serve nas duas formas.
///
/// **A06 e A07 da auditoria de 17/09.**
///
/// O carregador distinguia pacote ausente, hash divergente, catálogo recusado e
/// script que não carregou — e comunicava os quatro por `console.warn` e
/// `console.error`. A gestão mostrava instalado e ligado, nunca «carregado» ou
/// «falhou». Num aplicativo empacotado o console não é lugar nenhum: quem usa
/// via o MOD na lista, nenhum botão dele na tela, e nenhum próximo passo.
///
/// E a URL era `mod://localhost/...` escrita à mão — a forma do macOS e do
/// Linux, servida no Windows e no Android como `http://mod.localhost/...`. **O
/// ADR 0049 tirou a URL do meio**: o código do MOD vem pela ponte, por
/// `codigo_do_mod`, e o que a janela monta é um `Blob`. As duas formas do
/// esquema deixaram de existir, e com elas o defeito de nascença de escolher
/// uma das duas à mão.
///
/// **O que este guarda não prova:** que o Mesa abre. Isso é um teste de fumaça
/// no binário nativo de cada sistema, e a auditoria o pede em separado. O que
/// se cobra aqui é que a falha tenha para onde ir e que a URL tenha de onde
/// vir.
#[test]
fn o_carregador_de_mods_diz_na_tela_e_monta_a_url_pelo_tauri() {
    // Sem comentários: o texto que **explica** a forma antiga cita a forma
    // antiga, e um guarda que não distingue os dois acusa a própria explicação.
    let base = without_comments(&read("ui/base.js"));
    let painel = read("ui/camada-mods.js");

    // O código vem pela ponte, com o hash junto: é lá que a conferência mora, e
    // é o que faz a janela não ter URL de MOD nenhuma para montar.
    assert!(
        base.contains("invoke(\"codigo_do_mod\""),
        "a janela deixou de pedir o código do MOD pela ponte"
    );
    for sumido in ["convertFileSrc", "mod://localhost/", "mod.localhost"] {
        assert!(
            !base.contains(sumido),
            "`{sumido}` voltou a `base.js`: desde o ADR 0049 o código de um MOD \
             não entra na janela por URL nenhuma"
        );
    }
    // E ele não volta a ser um `<script>` da página por outro caminho.
    assert!(
        !base.contains("createElement(\"script\")"),
        "alguém voltou a injetar o MOD como script da página"
    );

    // Toda fase que o carregador anota tem frase no painel: uma fase sem frase
    // não é desenhada, e o defeito volta a ser silencioso.
    let mut fases: BTreeSet<String> = BTreeSet::new();
    for (at, _) in base.match_indices("anotarEstadoDoMod(") {
        let Some(resto) = base.get(at + "anotarEstadoDoMod(".len()..) else {
            continue;
        };
        // Até o fecha-parênteses desta chamada, contando os que abrem no meio:
        // o segundo argumento pode ser um ternário com duas fases dentro, e
        // pegar só a primeira string depois da vírgula perde uma delas.
        let mut nivel = 1usize;
        let mut fim = resto.len();
        for (i, c) in resto.char_indices() {
            match c {
                '(' => nivel += 1,
                ')' => {
                    nivel -= 1;
                    if nivel == 0 {
                        fim = i;
                        break;
                    }
                }
                _ => {}
            }
        }
        let chamada = resto.get(..fim).unwrap_or("");
        // Toda string literal da chamada que tem cara de identificador de fase:
        // minúsculas e hífen. O detalhe é frase, e frase tem espaço ou acento.
        for pedaco in chamada.split('"').skip(1).step_by(2) {
            if !pedaco.is_empty() && pedaco.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
                fases.insert(pedaco.to_owned());
            }
        }
    }
    assert!(
        !fases.is_empty(),
        "o carregador não anota fase nenhuma; sem isso a tela não tem o que dizer"
    );
    let sem_frase: Vec<&String> = fases
        .iter()
        // `catalogo-recusado` não é de nenhum MOD: ele vira a frase do painel
        // inteiro, e não uma linha da lista.
        .filter(|f| *f != "catalogo-recusado")
        .filter(|f| !painel.contains(&format!("\"{f}\"")) && !painel.contains(&format!("{f}:")))
        .collect();
    assert!(
        sem_frase.is_empty(),
        "estas fases são anotadas pelo carregador e não têm frase no painel, \
         então elas não aparecem para ninguém: {sem_frase:?}"
    );
}

/// Todo `aria-labelledby` e `aria-describedby` aponta para um `id` que existe.
///
/// **A14 da auditoria de 17/09**, na mesma família do A01. Um `id` repetido
/// fazia o leitor de tela anunciar um diálogo pelo título de outro; um `id` que
/// **não existe** é pior e igualmente silencioso: o atributo é ignorado, o
/// elemento fica sem nome acessível, e nada na tela muda para quem enxerga.
///
/// Nenhum motor avisa, nenhum teste de renderização repara, e a pessoa que
/// depende disso é justamente a que não pode conferir sozinha.
#[test]
fn toda_referencia_de_acessibilidade_aponta_para_algo_que_existe() {
    let page = without_comments(&read("ui/index.html"));

    let ids: BTreeSet<String> = {
        let mut achados = BTreeSet::new();
        let mut resto = page.as_str();
        while let Some(at) = resto.find("id=\"") {
            resto = resto.get(at + 4..).unwrap_or("");
            let Some(fim) = resto.find('"') else { break };
            if let Some(id) = resto.get(..fim) {
                achados.insert(id.to_owned());
            }
            resto = resto.get(fim..).unwrap_or("");
        }
        achados
    };

    let mut orfas: Vec<String> = Vec::new();
    for atributo in ["aria-labelledby=\"", "aria-describedby=\"", "for=\""] {
        let mut resto = page.as_str();
        while let Some(at) = resto.find(atributo) {
            resto = resto.get(at + atributo.len()..).unwrap_or("");
            let Some(fim) = resto.find('"') else { break };
            let valor = resto.get(..fim).unwrap_or("");
            // Os dois `aria-` aceitam uma lista separada por espaço.
            for alvo in valor.split_whitespace() {
                if !ids.contains(alvo) {
                    orfas.push(format!("{atributo}{alvo}\""));
                }
            }
            resto = resto.get(fim..).unwrap_or("");
        }
    }

    assert!(
        orfas.is_empty(),
        "estas referências apontam para um `id` que não existe na página, então \
         o atributo é ignorado e o elemento fica sem nome acessível — e nada na \
         tela muda para quem enxerga: {orfas:?}"
    );
}

/// Um sim de quem hospeda é gravado para o endereço onde ele está.
///
/// **QA-02 do reteste de 17/09, e o defeito era do conserto entregue no mesmo
/// dia.** `aplicar_mod` prendia o consentimento a `session.alvo` — e `alvo` é
/// «onde voltar», preenchido **dentro** de `if !hospedado_aqui`, porque
/// `127.0.0.1` não entra na lista de atalhos. Num servidor hospedado aqui,
/// que é exatamente o caso de quem liga um MOD, o campo estava vazio: o aceite
/// não era gravado, e o diálogo que tinha acabado de prometer «você não vai ser
/// perguntado outra vez» perguntava de novo na reconexão seguinte.
///
/// Um campo servindo a duas perguntas responde mal a uma delas. São dois
/// campos, e este guarda cobra as duas metades: que `aplicar_mod` leia o certo,
/// e que o certo seja preenchido antes de qualquer condição sobre o endereço.
#[test]
fn o_aceite_de_quem_hospeda_e_gravado_para_o_endereco_onde_ele_esta() {
    let fonte = read("src/main.rs");

    let aplicar = body_of(&fonte, "async fn aplicar_mod");
    assert!(
        aplicar.contains("endereco_da_sessao"),
        "`aplicar_mod` não lê o endereço desta sessão: {aplicar}"
    );
    assert!(
        !aplicar.contains("session.alvo"),
        "`aplicar_mod` lê `alvo`, que é «onde voltar» e está vazio justamente \
         quando quem liga o MOD é quem hospeda:\n{aplicar}"
    );

    // E o preenchimento vem antes da condição que exclui o servidor local.
    let conectar = body_of(&fonte, "async fn connect");
    let (Some(onde), Some(condicao)) = (
        conectar.find("session.endereco_da_sessao"),
        conectar.find("if !hospedado_aqui"),
    ) else {
        panic!("`connect` deixou de preencher o endereço da sessão, ou de filtrar a lista de visitados");
    };
    assert!(
        onde < condicao,
        "o endereço da sessão é gravado **dentro** do filtro da lista de \
         visitados, então hospedar aqui volta a deixá-lo vazio — que é o \
         defeito inteiro"
    );
}

/// O caminho entre pares tem onde ser consentido, da tela até o núcleo.
///
/// **O defeito que este guarda existe para impedir já durou uma versão.** O
/// protocolo, o núcleo e o servidor traziam a malha inteira — discagem, furo de
/// NAT, conferência de impressão digital, volta para o servidor quando o par
/// cai — e **nada em `apps/` ou no `seele-ffi` declarava o consentimento**. O
/// `enlace.rs` registrava isso por escrito, o roteiro de duas máquinas marcava
/// o passo como bloqueado, e as notas da versão prometiam «com consentimento
/// dos dois lados» a quem não tinha onde consentir.
///
/// «Existir não é funcionar», e cinco mil linhas verdes e inalcançáveis são a
/// forma mais cara disso. O guarda cobra a corrente inteira, elo por elo,
/// porque foi um elo faltando no meio que a deixou inerte.
#[test]
fn a_malha_tem_onde_ser_consentida_da_tela_ate_o_nucleo() {
    let raiz = app_dir();
    let ffi = std::fs::read_to_string(raiz.join("../../crates/seele-ffi/src/lib.rs"))
        .unwrap_or_else(|erro| panic!("o `seele-ffi` tem de ser legível: {erro}"));
    let app = read("src/main.rs");
    let page = read("ui/index.html");
    let scripts = scripts();

    // 1 · O `seele-ffi` alcança o núcleo.
    assert!(
        ffi.contains("consentir_no_caminho_entre_pares"),
        "o `seele-ffi` não expõe o consentimento, e o núcleo fica inalcançável"
    );
    // 2 · O app alcança o `seele-ffi`, e por um comando que a página pode chamar.
    assert!(
        app.contains("fn consentir_no_caminho_entre_pares"),
        "o app não tem comando de consentimento"
    );
    // 3 · A página chama o comando.
    assert!(
        scripts.contains("\"consentir_no_caminho_entre_pares\""),
        "nenhum script chama o consentimento: o controle existe e não faz nada"
    );
    // 4 · E há onde clicar.
    for id in ["malha-assiste", "malha-empresta"] {
        assert!(
            page.contains(&format!("id=\"{id}\"")),
            "`{id}` não está na página: não há onde consentir"
        );
    }

    // **E o consentimento sai a cada conexão.** Ele viaja na declaração de
    // identidade, que o núcleo manda sozinho com «não empresto nada». Sem esta
    // linha em `connect`, a escolha guardada valeria só até a janela fechar —
    // que é uma forma mais sutil da mesma inércia.
    let conectar = body_of(&app, "async fn connect");
    assert!(
        conectar.contains("consentir_no_caminho_entre_pares"),
        "`connect` não declara o consentimento guardado, então ele vale só na \
         janela em que foi escolhido"
    );

    // **Dois controles, e não um.** `control.rs` é explícito: emprestar a
    // subida e aceitar ser servida são perguntas diferentes, feitas a pessoas
    // diferentes da mesma conversa. Juntá-las num interruptor responderia uma
    // delas por quem usa.
    assert!(
        page.contains("id=\"malha-assiste\"") && page.contains("id=\"malha-empresta\""),
        "as duas metades do consentimento viraram um controle só"
    );
}

/// A sonda é semeada em toda conexão, inclusive na de quem hospeda.
///
/// **Medido antes de consertar** (`tela.rs`, `medida_do_anfitriao`): com o
/// loopback por baixo e cinquenta megabits de subida do anfitrião, o primeiro
/// segundo da transmissão ainda é **540p** — porque a perna que aperta não é o
/// cano, é o que a sonda mediu, e ela parte de 2 Mbps enquanto não mediu nada.
///
/// A semente por servidor existia e era chamada, mas **dentro** do ramo
/// `if !hospedado_aqui(&alvo)` — o mesmo ramo que causou o QA-02. Quem hospeda
/// nunca a recebia, e os vinte e cinco segundos de subida que o comentário de
/// `camada-compartilhar.js` mediu em campo se repetiam em toda transmissão.
///
/// É o terceiro defeito com a mesma forma no mesmo dia: um `if` sobre o
/// endereço decidindo algo que não é sobre o endereço.
#[test]
fn a_sonda_e_semeada_fora_do_filtro_da_lista_de_visitados() {
    let fonte = read("src/main.rs");
    let conectar = without_comments(&body_of(&fonte, "async fn connect"));

    let (Some(semente), Some(filtro)) = (
        conectar.find("lembrar_o_caminho"),
        conectar.find("if !hospedado_aqui"),
    ) else {
        panic!("`connect` deixou de semear a sonda, ou de filtrar a lista de visitados");
    };
    assert!(
        semente < filtro,
        "a semente da sonda está **dentro** do filtro da lista de visitados, \
         então quem hospeda nunca a recebe e toda transmissão dele recomeça em \
         540p — que é o defeito inteiro"
    );

    // E ela tem as duas fontes, nesta ordem de especificidade: a medida deste
    // servidor manda, porque o caminho até cada um é diferente; a da máquina é
    // o ponto de partida de quem ainda não tem uma.
    assert!(
        conectar.contains("caminho_bps") && conectar.contains("caminho_da_maquina"),
        "a semente não tem as duas fontes; sem a da máquina, hospedar aqui e a \
         primeira visita a um servidor novo continuam partindo do palpite"
    );

    // E a máquina aprende ao sair, senão não haveria o que semear.
    let desmontar = without_comments(&body_of(&fonte, "fn desmontar_o_cliente"));
    assert!(
        desmontar.contains("anotar_caminho_da_maquina"),
        "nada guarda a subida desta máquina, então a semente dela nunca existe"
    );
}

/// **O aceite obtém o conjunto que ele autoriza, antes de entrar.**
///
/// Relato de campo, na v0.11.0: «entrei como convidado num servidor com MOD de
/// estilo e a cor do servidor não mudou». O aceite gravava o sim e reconectava,
/// e mais nada. O que era exigido e não estava na máquina virava a anotação
/// `sem-pacote`, desenhada **só** dentro da tela de MODs — que é justamente a
/// tela que quem entra não abriu. O convidado chegava à sessão sem o pacote e
/// sem ser avisado.
///
/// Este guarda prende as três metades do conserto:
///
///   1. aceitar chama a obtenção, e só entra depois dela;
///   2. a obtenção pede o pacote pelo `id` **e** pela versão anunciada;
///   3. e confere o **hash** exigido depois de obter — porque aceitar autoriza
///      aquele conteúdo, e não qualquer coisa que leve o mesmo nome.
#[test]
fn o_aceite_obtem_o_conjunto_antes_de_entrar() {
    let camada = without_comments(&read("ui/camada-mods.js"));

    let aceitar = camada
        .split_once("async function aceitarOsMods()")
        .expect("a função que grava o sim")
        .1;
    let corpo = aceitar
        .split_once("\nasync function ")
        .map_or(aceitar, |(antes, _)| antes);

    let obtem = corpo
        .find("obterOConjunto")
        .expect("aceitar tem de obter o conjunto que acabou de autorizar");
    let entra = corpo
        .find("conectar()")
        .expect("aceitar continua entrando no servidor");
    assert!(
        obtem < entra,
        "o aceite entra antes de obter os pacotes: o convidado chega à sessão \
         sem o que o servidor exige e descobre pela cor que não mudou"
    );

    let obter = camada
        .split_once("async function obterOConjunto(")
        .expect("a função que põe o conjunto nesta máquina")
        .1;
    assert!(
        obter.contains("instalar_mod_do_catalogo"),
        "a obtenção deixou de buscar o pacote: aceitar volta a ser só um sim \
         gravado, e o MOD exigido nunca chega"
    );
    assert!(
        obter.contains("versao: m.version"),
        "o pacote passou a ser pedido sem a versão anunciada: o servidor exige \
         um conteúdo exato, e pedir «a mais nova» é substituí-lo em silêncio"
    );
    assert!(
        obter.contains("i.hash === m.hash"),
        "a obtenção deixou de conferir o hash exigido: uma cópia local com o \
         mesmo nome passaria por autorizada, e aceitar viraria confiança em \
         qualquer versão futura"
    );
}

/// **Um endereço numa mensagem abre no navegador, e não nesta janela.**
///
/// O texto de uma mensagem é escrito por outra pessoa, e torná-lo clicável é
/// dar a ela um botão na máquina de quem lê. Duas coisas seguram isso, e este
/// guarda prende as duas:
///
///   - **sem `href`.** Com ele, um clique do meio ou um cmd-clique navegam
///     *esta* janela para o site. A casca não tem barra de endereço nem botão
///     de voltar: o produto seria substituído por uma página que outra pessoa
///     escolheu, sem caminho de volta;
///   - **o esquema é conferido no Rust.** `abrir_no_navegador` recusa tudo o
///     que não é `http`/`https` — `file:`, `javascript:`, `data:`, e qualquer
///     esquema que o sistema saiba abrir como programa. A tela não decide isso
///     sozinha, e a lista de permitidos tem teste próprio em `main.rs`.
#[test]
fn um_endereco_numa_mensagem_sai_pelo_navegador_e_nao_por_esta_janela() {
    let fonte = without_comments(&read("ui/tela-sessao.js"));

    let com_links = js_function(&fonte, "function comLinks(");
    assert!(
        !com_links.contains("href"),
        "o link de mensagem ganhou `href`: um clique do meio troca o SEELE por \
         um site escolhido por outra pessoa, e esta janela não tem como voltar"
    );
    assert!(
        com_links.contains("data.url = url") || com_links.contains("dataset.url"),
        "o endereço deixou de viajar num `data-url`, então o ouvinte não tem o \
         que abrir: {com_links}"
    );
    assert!(
        com_links.contains("setAttribute(\"title\""),
        "o link deixou de dizer para onde vai: quem clica num endereço escrito \
         por outra pessoa tem de poder ler o destino antes"
    );

    let abrir = js_function(&fonte, "function abrirLinkDaMensagem(");
    assert!(
        abrir.contains("abrir_no_navegador"),
        "o clique deixou de passar pelo comando que confere o esquema: {abrir}"
    );
    assert!(
        abrir.contains("preventDefault"),
        "o clique deixou de ser interceptado, então o navegador desta janela \
         decide o que fazer com ele: {abrir}"
    );

    // E o corpo da mensagem tem de passar por ali — senão a função existe e
    // nada a chama, que é o modo de falhar que este repositório mais conhece.
    let corpo = js_function(&fonte, "function corpoComRealce(");
    assert!(
        corpo.contains("comLinks("),
        "o corpo da mensagem voltou a ser texto cru: a função de links existe \
         e ninguém a chama: {corpo}"
    );
}

/// **Quem entrou não recebe interruptor para o que o servidor exige.**
///
/// A gestão mostrava a quem entrou um botão DESLIGAR desabilitado. Um botão
/// morto é uma pergunta sem resposta: sugere que desligar seria possível
/// noutro momento. E não é — o conjunto foi acordado na porta, e sair dele sem
/// sair do servidor é estar na sessão fingindo cumprir o que não cumpre.
///
/// O documento de ajustes de 18/09 pede o oposto de um controle: «Exigido por
/// este servidor», que é um fato, não uma oferta.
///
/// A lista do que é exigido tem de vir do **anúncio deste destino**, e não da
/// fase de cada MOD: a fase diz o que aconteceu com ele, e um MOD que o
/// servidor soltou continuaria com fase até a volta seguinte.
#[test]
fn quem_entrou_nao_recebe_interruptor_para_o_que_o_servidor_exige() {
    let camada = without_comments(&read("ui/camada-mods.js"));
    let linha = js_function(&camada, "function linhaDeModInstalado(");

    let rotulo = linha
        .find("modsExigidos")
        .expect("a linha tem de perguntar se este MOD é exigido por este servidor");
    let botao = linha
        .find("DESLIGAR")
        .expect("a linha continua sabendo desligar, para quem hospeda");
    assert!(
        rotulo < botao,
        "o interruptor é montado antes da pergunta «é exigido aqui?», então \
         quem entrou volta a receber um botão para desligar o que o servidor \
         exige: {linha}"
    );
    assert!(
        linha.contains("!hospedando && modsExigidos.has(mod.id)"),
        "a condição deixou de juntar as duas metades: para **quem hospeda** o \
         interruptor continua valendo, e é só para quem entrou que ele vira \
         rótulo: {linha}"
    );

    // E a lista tem de ser mantida onde o anúncio do servidor chega, e esvaziada
    // quando não há sessão — senão a gestão diz «exigido por este servidor»
    // depois de sair dele.
    let base = without_comments(&read("ui/base.js"));
    let carregar = js_function(&base, "async function carregarMods(");
    assert!(
        carregar.contains("modsExigidos.clear()") && carregar.contains("modsExigidos.set("),
        "a lista de exigidos deixou de ser refeita a cada anúncio: um MOD que \
         o servidor soltou continuaria sendo chamado de obrigatório: {carregar}"
    );
}

/// **O que o servidor exige e não está aqui aparece na lista.**
///
/// A gestão desenhava `mods_instalados`, e nada mais. O carregador anotava
/// `sem-pacote` para o que o servidor exigia e faltava, e aquela anotação não
/// tinha onde aparecer: a linha daquele MOD não existia.
///
/// Quem entrava num servidor que exige um MOD ausente via a lista sem ele e
/// nenhuma frase — o produto sabia e não contava. Esta é a linha que faltava, e
/// ela não traz interruptor: o conjunto foi acordado na porta.
#[test]
fn o_que_o_servidor_exige_e_nao_esta_aqui_tem_linha_na_gestao() {
    let camada = without_comments(&read("ui/camada-mods.js"));
    let desenhar = js_function(&camada, "async function desenharMods(");
    // **Montada e desenhada.** Cobrar só a chamada a `linhaDeModQueFalta` é
    // vacuoso: a primeira reversão deste guarda tirou as linhas do `repovoar` e
    // deixou a montagem de pé, e ele passou — as linhas eram construídas e
    // jogadas fora. O que prende é o `repovoar`.
    assert!(
        desenhar.contains("linhaDeModQueFalta("),
        "a lista deixou de montar a linha do que falta: {desenhar}"
    );
    // Toda chamada a `repovoar`, e não a primeira: há duas — a da lista vazia e
    // a da lista cheia —, e casar por posição é o erro que um guarda desta
    // suíte já cometeu hoje.
    let desenhada = desenhar.split("repovoar(lista, [").skip(1).any(|depois| {
        depois
            .split_once("]);")
            .map_or(depois, |(a, _)| a)
            .contains("...faltando")
    });
    assert!(
        desenhada,
        "as linhas do que falta são montadas e não entram na lista: {desenhar}"
    );

    let falta = js_function(&camada, "function linhaDeModQueFalta(");
    assert!(
        falta.contains("estadoDosMods.get(id)") && falta.contains("FASES_DO_MOD["),
        "a linha do que falta deixou de dizer em que pé está: {falta}"
    );
    assert!(
        falta.contains("EXIGIDO POR ESTE SERVIDOR"),
        "a linha do que falta não diz que ele é exigido: {falta}"
    );
    assert!(
        !falta.contains("botao-fantasma"),
        "a linha do que falta ganhou um botão; não há o que ligar, e um botão \
         morto é uma pergunta sem resposta: {falta}"
    );

    // E o hash exigido chega até ela: mandar instalar sem dizer **qual
    // conteúdo** é mandar instalar de novo a mesma versão errada.
    let base = without_comments(&read("ui/base.js"));
    let carregar = js_function(&base, "async function carregarMods(");
    assert!(
        carregar.contains("modsExigidos.set(m.id, m.hash)"),
        "a lista de exigidos voltou a guardar só o identificador, e a linha do \
         que falta não tem como dizer qual conteúdo o servidor exige: {carregar}"
    );
}

/// **Os interruptores editam um rascunho; SALVAR aplica o conjunto uma vez.**
///
/// Relatado no documento de ajustes de 18/09: «cada ativação expulsa o host da
/// sessão e exige nova entrada. Ativar vários MODs repete o processo para cada
/// um». A causa está no banco — cada `enable`/`disable` chama
/// `anotar_mudanca_nos_mods`, e acordar o anúncio encerra as sessões cujo
/// conjunto mudou. Três interruptores, três quedas.
///
/// O documento é explícito sobre o conserto: «Não implementar como um laço de
/// chamadas aos comandos atuais se cada chamada já notifica mudanças e encerra
/// sessões». Este guarda prende as quatro metades do que ficou:
///
///   1. o interruptor mexe no rascunho, e não no servidor;
///   2. SALVAR chama o comando atômico, e **não** `aplicar_mod` num laço;
///   3. sem diferença, SALVAR não fica aceso — salvar sem mudança ainda
///      escreveria, e escrever ainda derruba todo mundo, por nada;
///   4. um segundo envio não sai enquanto o primeiro não voltou.
#[test]
fn os_interruptores_de_mod_editam_um_rascunho_e_salvar_aplica_de_uma_vez() {
    let camada = without_comments(&read("ui/camada-mods.js"));

    let linha = js_function(&camada, "function linhaDeModInstalado(");
    // **Pelo par, e não pelo identificador.** Com o cache por conteúdo pode
    // haver dois pacotes do mesmo MOD no disco, e escolher um deles é a decisão
    // que esta tela toma — um rascunho por identificador diria «exija este MOD»
    // sem dizer quais bytes.
    assert!(
        linha.contains("rascunho.add(chaveDoPacote(mod))")
            && linha.contains("rascunho.delete(chaveDoPacote(mod))"),
        "o interruptor voltou a gravar no servidor em vez de mexer no rascunho, \
         ou voltou a marcar por identificador em vez do par: {linha}"
    );
    assert!(
        !linha.contains("perguntarETrocarOMod"),
        "o interruptor voltou a aplicar na hora: {linha}"
    );

    let salvar = js_function(&camada, "async function salvarAlteracoes(");
    assert!(
        salvar.contains("aplicar_conjunto_de_mods"),
        "SALVAR deixou de usar o comando atômico: {salvar}"
    );
    assert!(
        !salvar.contains("aplicar_mod"),
        "SALVAR virou um laço sobre o comando de um MOD só, que é exatamente o \
         que o documento manda não fazer — cada chamada acorda o anúncio: \
         {salvar}"
    );
    assert!(
        salvar.contains("base: baseDoConjunto"),
        "SALVAR deixou de mandar a base do rascunho, então uma mudança feita \
         por outra pessoa é sobrescrita em silêncio: {salvar}"
    );
    assert!(
        salvar.contains("salvando = true"),
        "o envio deixou de ser trancado: dois cliques mandam dois conjuntos, e \
         o segundo confere a base contra o que o primeiro acabou de gravar: \
         {salvar}"
    );

    let barra = js_function(&camada, "function desenharRascunho(");
    assert!(
        barra.contains("$(\"mods-salvar\").disabled = !mudou"),
        "SALVAR fica aceso sem diferença nenhuma: apertá-lo escreveria assim \
         mesmo, e escrever derruba todo mundo por nada: {barra}"
    );

    // E sair sem salvar avisa: a seleção pendente é uma decisão, e descartá-la
    // calado é «o produto sabe e não conta».
    let server = without_comments(&read("ui/tela-server.js"));
    let fechar = js_function(&server, "function fecharServer(");
    assert!(
        fechar.contains("haRascunhoPorSalvar()") && fechar.contains("abrirConfirmacao"),
        "fechar a tela descarta o rascunho sem perguntar: {fechar}"
    );
}

/// **Sair do servidor tem de encerrar o ambiente dos MODs, e sair é local.**
///
/// O descarregamento morava dentro do ouvinte de `Ended`, e `Ended` só chega do
/// outro lado: a sala terminou, o enlace caiu, alguém foi moderado. Sair por
/// vontade própria não emite nada — `Connection::disconnect` manda `Shutdown`,
/// e o laço da FFI responde `return false`.
///
/// O efeito era o que se via, e foi relatado assim: a cor verde do servidor
/// ficava na entrada e no launcher depois de sair. O ESTILO escrevia os tokens
/// em `document.documentElement`, e o `restore` dele só rodava no descarte
/// cooperativo — que nunca era pedido.
///
/// **O ADR 0049 trocou o pedido pela garantia.** O encerramento não dispara
/// mais `seele-mod-unload` e não espera que o MOD desmonte a si mesmo: ele
/// chama `terminate()`, e o contexto morre com temporizador, ouvinte, áudio e
/// promessa dentro. A região sai junto, e o tema que ela tinha sai com ela.
///
/// Este guarda prende as três metades: existe **um** encerramento, ele mata o
/// contexto em vez de pedir, e a saída local o chama **antes** de mostrar
/// qualquer tela fora do servidor.
#[test]
fn sair_do_servidor_encerra_o_ambiente_dos_mods_antes_de_trocar_de_tela() {
    let base = without_comments(&read("ui/base.js"));
    let encerrar = js_function(&base, "function encerrarOAmbienteDosMods(");
    for exigido in ["instancia?.encerrar()", "modsCarregados.clear()"] {
        assert!(
            encerrar.contains(exigido),
            "o encerramento deixou de fazer `{exigido}`: {encerrar}"
        );
    }
    // **Pelo ciclo, e não à mão.** O encerramento não pode voltar a mandar no
    // executor nem a limpar recurso por conta própria: quem sabe o que uma
    // instância criou é ela, e uma limpeza escrita aqui é uma lista que
    // envelhece sozinha a cada recurso novo. Ver `InstanciaDeMod.encerrar`.
    for proibido in ["terminate()", "limparARegiaoDoMod("] {
        assert!(
            !encerrar.contains(proibido),
            "o encerramento voltou a fazer `{proibido}` na mão, por fora do \
             ciclo da instância: {encerrar}"
        );
    }
    assert!(
        !encerrar.contains("seele-mod-unload"),
        "o encerramento voltou a **pedir** que o MOD se desmonte; um MOD que \
         não atenda deixa tudo de pé, e foi isso que o ADR 0049 tirou: \
         {encerrar}"
    );

    // E o ouvinte de `Ended` passa a ser um chamador, e não o dono: se ele
    // voltar a ter o corpo, o caminho local volta a não ter nenhum.
    //
    // Localizado pelo que ele faz — `payload?.Ended` —, e não por ser o
    // primeiro `listen("seele://event"` do arquivo: há dois, e o outro é o das
    // respostas de MOD. Casar por posição é o erro que um guarda desta suíte
    // já cometeu hoje.
    let ouvinte = base
        .split_once("if (!payload?.Ended) return;")
        .expect("o ouvinte do término da sessão")
        .1;
    let ate_o_fim = ouvinte.split_once("\n});").map_or(ouvinte, |(a, _)| a);
    assert!(
        ate_o_fim.contains("encerrarOAmbienteDosMods()"),
        "o término remoto deixou de passar pelo encerramento único: {ate_o_fim}"
    );

    let sessao = without_comments(&read("ui/tela-sessao.js"));
    let ejetar = js_function(&sessao, "async function ejetar(");
    let encerra = ejetar.find("encerrarOAmbienteDosMods()").expect(
        "sair tem de encerrar o ambiente dos MODs: sem isto o tema do \
                 servidor fica na entrada e no launcher, e só fechar o app o tira",
    );
    let esconde = ejetar
        .find("$(\"tela-sessao\").hidden = true")
        .expect("sair continua escondendo a sessão");
    assert!(
        encerra < esconde,
        "o encerramento acontece depois de a tela trocar: há um instante com a \
         tela de fora e o tema de dentro, e uma tarefa atrasada ainda alcança o \
         MOD: {ejetar}"
    );
}

/// **Um link de imagem vira imagem — e só um link de imagem é buscado.**
///
/// Desenhar um link é buscá-lo, e buscar é aparecer: quem controla o endereço
/// aprende o IP de quem está lendo a conversa, e a hora. Isso é o preço da
/// funcionalidade e foi aceito de olhos abertos.
///
/// O que **não** foi aceito é pagar esse preço por todo link. Buscar qualquer
/// endereço colado entregaria o IP de quem lê a todo domínio que aparecesse na
/// conversa — inclusive a um link posto ali só para colher endereços. Por isso
/// a busca só sai quando o caminho termina em extensão de imagem: o link
/// **direto** de mídia atravessa, e uma página que fala de um GIF continua
/// sendo um link clicável, que é o certo — ela não é uma imagem.
///
/// E a busca acontece **uma vez por endereço**: a conversa redesenha a cada
/// tique, e sem o registro cada desenho seria outra batida na porta de alguém.
#[test]
fn so_um_link_de_imagem_e_buscado_e_so_uma_vez() {
    let fonte = without_comments(&read("ui/tela-sessao.js"));

    let com_links = js_function(&fonte, "function comLinks(");
    assert!(
        com_links.contains("pareceImagem(url)"),
        "todo link passou a ser buscado: o IP de quem lê vai para qualquer \
         domínio que alguém cole na conversa: {com_links}"
    );

    let parece = js_function(&fonte, "function pareceImagem(");
    assert!(
        parece.contains("new URL(url).pathname"),
        "a extensão passou a ser procurada na URL inteira, e não no caminho: \
         `?x=.gif` na consulta faria qualquer página parecer imagem: {parece}"
    );

    let buscar = js_function(&fonte, "function buscarAPreviaDoLink(");
    assert!(
        buscar.contains("if (previasDeLink.has(url)) return;"),
        "a busca deixou de ser uma vez por endereço: a conversa redesenha a \
         cada tique, e cada desenho vira outra batida na porta: {buscar}"
    );
    assert!(
        buscar.contains("previasDeLink.set(url, false)"),
        "o que não deu deixou de ser guardado: um endereço que não responde \
         vira uma batida constante: {buscar}"
    );
    assert!(
        buscar.contains("previa_de_link"),
        "a busca deixou de passar pelo comando que confere esquema, tamanho e \
         bytes: {buscar}"
    );
}

/// **Um servidor exige um pacote por MOD, e o rascunho não deixa marcar dois.**
///
/// Com o cache por conteúdo, dois pacotes do mesmo MOD podem estar no disco —
/// um que este servidor exige, outro que outro servidor desta máquina baixou.
/// A tela lista os dois, e ligar o segundo tem de **substituir** o primeiro no
/// rascunho: um servidor não exige duas versões do mesmo MOD, e deixar as duas
/// marcadas diria que exige.
#[test]
fn ligar_outro_conteudo_do_mesmo_mod_substitui_no_rascunho() {
    let camada = without_comments(&read("ui/camada-mods.js"));
    let linha = js_function(&camada, "function linhaDeModInstalado(");
    assert!(
        linha.contains("if (chave.split(\"\\u0000\")[0] === mod.id) rascunho.delete(chave);"),
        "ligar outro conteúdo do mesmo MOD deixou os dois marcados: o SALVAR \
         mandaria um conjunto que exige duas versões do mesmo MOD: {linha}"
    );

    // E a chave é o par, em toda a tela: o rascunho, o conjunto de pé e o que
    // vai ao comando têm de falar a mesma língua, senão a comparação entre eles
    // acha diferença onde não há.
    let desenhar = js_function(&camada, "async function desenharMods(");
    assert!(
        desenhar.contains("map(chaveDoPacote)"),
        "o conjunto de pé deixou de ser lido por par: ele passaria a diferir do \
         rascunho por forma, e não por decisão: {desenhar}"
    );
}

/// **A gestão diz de qual servidor ela fala.**
///
/// Plano de isolamento de 18/09: «gestão indica servidor-alvo, sempre
/// identificando a instância selecionada… nunca inferir o destino apenas de
/// *estou hospedando*».
///
/// A tela dizia «ligar e desligar MOD é do servidor que **esta janela**
/// hospeda» e parava aí. Com a lista de servidores guardados, quem tem dois não
/// tinha como saber em qual estava mexendo — e o conjunto de MODs é de um
/// servidor, não da máquina.
#[test]
fn a_gestao_de_mods_nomeia_o_servidor_em_que_esta_mexendo() {
    let camada = without_comments(&read("ui/camada-mods.js"));
    let desenhar = js_function(&camada, "async function desenharMods(");

    assert!(
        desenhar.contains("servidor_hospedado"),
        "a tela voltou a inferir o destino de «estou hospedando», que responde \
         outra pergunta: {desenhar}"
    );
    assert!(
        desenhar.contains("mods-alvo-do-servidor"),
        "o alvo deixou de ser escrito na tela: {desenhar}"
    );
    // E o caso do legado é dito, em vez de aparecer como um nome vazio: a
    // máquina que já hospedava antes de haver lista não tem identificador.
    assert!(
        desenhar.contains("servidor de sempre desta máquina"),
        "o servidor legado deixou de ser nomeado, e a linha some ou fica vazia \
         justamente para quem hospeda há mais tempo: {desenhar}"
    );

    let pagina = read("ui/index.html");
    assert!(
        pagina.contains("id=\"mods-alvo-do-servidor\""),
        "o lugar onde o alvo é escrito saiu da página"
    );
}

/// Todo ponto de contribuição anunciado tem quem o aplique na interface.
///
/// # O defeito que este guarda fecha
///
/// A revisão de 20/09/2026, R3: «dos dez pontos anunciados, encontrei aplicação
/// visual para apenas dois. Os outros aceitam registro sem produzir o efeito
/// prometido». Um MOD registrava, recebia punho, e nada acontecia — a falha
/// silenciosa que o topo do `CLAUDE.md` chama de a mais cara deste repositório.
///
/// # Por que um guarda de texto basta aqui
///
/// Ele não prova que o nó aparece na tela: isso é a bancada e a observação
/// nativa. Ele prova o que **faltava**, que é mais simples e mais fácil de
/// regredir: um nome na tabela sem nenhum consumidor do outro lado. Um ponto
/// novo acrescentado à tabela e esquecido na interface reprova aqui, no commit
/// em que ele é acrescentado, e não numa revisão meses depois.
///
/// `mods-contribuicoes.js` é o registro e `camada-mods.js` é a gestão: nenhum
/// dos dois conta como aplicação. Aplicar é o que a tela faz com o que o
/// registro guarda.
#[test]
fn todo_ponto_de_contribuicao_anunciado_tem_quem_o_aplique() {
    let registro = read("ui/mods-contribuicoes.js");
    let tabela = registro
        .split_once("const PONTOS_DE_CONTRIBUICAO = Object.freeze({")
        .expect("a tabela de pontos de contribuição")
        .1;
    let tabela = tabela.split_once("});").expect("a tabela fecha").0;

    let pontos: Vec<String> = tabela
        .lines()
        .filter_map(|linha| {
            let linha = linha.trim();
            let nome = linha.strip_prefix('"')?;
            let (nome, _) = nome.split_once('"')?;
            Some(nome.to_owned())
        })
        .collect();
    assert!(
        pontos.len() >= 10,
        "a tabela de pontos encolheu para {}: se um ponto saiu de verdade, este \
         número acompanha — e se a leitura quebrou, ela precisa ser consertada \
         antes de o guarda voltar a dizer alguma coisa",
        pontos.len()
    );

    // Os arquivos onde aplicar acontece: a tela e o roteador. O registro e a
    // gestão ficam de fora de propósito — ver o comentário acima.
    let aplicadores: String = ui_files("js")
        .into_iter()
        .filter(|nome| nome != "mods-contribuicoes.js" && nome != "camada-mods.js")
        .map(|nome| read(&format!("ui/{nome}")))
        .collect();

    let orfaos: Vec<&String> = pontos
        .iter()
        .filter(|ponto| !aplicadores.contains(&format!("\"{ponto}\"")))
        .collect();
    assert!(
        orfaos.is_empty(),
        "estes pontos são anunciados pela API de MODs e nenhuma tela os aplica — \
         um MOD registra, recebe punho, e nada acontece: {orfaos:?}"
    );
}

/// Nenhum modo aceito pela tabela deixa de ter aplicação.
///
/// A outra metade de R3, e a que sobrou depois de ligar os oito pontos: um
/// ponto pode ter consumidor para `adicionar` e anunciar `substituir` sem
/// ninguém consultar a substituição. `escolherSubstituicao` é o único caminho
/// por onde uma substituição chega à tela, então todo ponto que a anuncia
/// precisa aparecer ao lado de uma chamada dela.
#[test]
fn todo_ponto_que_anuncia_substituir_e_consultado_por_escolher_substituicao() {
    let registro = read("ui/mods-contribuicoes.js");
    let tabela = registro
        .split_once("const PONTOS_DE_CONTRIBUICAO = Object.freeze({")
        .expect("a tabela de pontos de contribuição")
        .1;
    let tabela = tabela.split_once("});").expect("a tabela fecha").0;

    let substituiveis: Vec<String> = tabela
        .lines()
        .filter(|linha| linha.contains("\"substituir\""))
        .filter_map(|linha| {
            let nome = linha.trim().strip_prefix('"')?;
            let (nome, _) = nome.split_once('"')?;
            Some(nome.to_owned())
        })
        .collect();
    assert!(
        !substituiveis.is_empty(),
        "nenhum ponto aceita `substituir`: se a decisão foi essa, ela precisa \
         estar escrita — e este guarda sai junto"
    );

    let telas: String = ui_files("js")
        .into_iter()
        .filter(|nome| nome != "mods-contribuicoes.js")
        .map(|nome| read(&format!("ui/{nome}")))
        .collect();

    for ponto in &substituiveis {
        let citado = telas
            .match_indices(&format!("\"{ponto}\""))
            .any(|(onde, _)| {
                // A chamada e o nome do ponto na mesma vizinhança: a consulta é
                // escrita em várias linhas, com o alvo e o preferido entre elas.
                let inicio = onde.saturating_sub(400);
                let fim = (onde + 400).min(telas.len());
                telas[inicio..fim].contains("escolherSubstituicao")
            });
        assert!(
            citado,
            "«{ponto}» anuncia o modo `substituir` e nenhuma tela chama \
             `escolherSubstituicao` para ele: um MOD substitui, recebe punho, e \
             o produto continua desenhando o nativo"
        );
    }
}

/// `decorar` não volta à tabela sem alguém que o aplique.
///
/// Ele saiu por uma razão que está escrita no topo de `mods-contribuicoes.js`:
/// as propriedades validadas que um MOD já pode usar dentro da própria raiz —
/// `opacidade`, `escalar`, `mover` — somem com o nome que abre a moderação
/// quando aplicadas a um nó do produto. Repô-lo exige o subconjunto que impede
/// isso, e repô-lo sem ele é reabrir R3 pelo mesmo caminho.
#[test]
fn decorar_nao_volta_a_tabela_sem_quem_o_aplique() {
    let registro = read("ui/mods-contribuicoes.js");
    let tabela = registro
        .split_once("const PONTOS_DE_CONTRIBUICAO = Object.freeze({")
        .expect("a tabela de pontos de contribuição")
        .1;
    let tabela = tabela.split_once("});").expect("a tabela fecha").0;

    if !tabela.contains("\"decorar\"") {
        // O estado de hoje: suspenso, e recusado pelo nome.
        assert!(
            registro.contains("«decorar» não está na API 4"),
            "`decorar` saiu da tabela e a recusa deixou de dizer o nome dele: um \
             MOD que o peça lê «aceita adicionar, e veio decorar» e conclui que \
             errou o ponto, que não foi o que ele fez"
        );
        return;
    }

    let telas: String = ui_files("js")
        .into_iter()
        .filter(|nome| nome != "mods-contribuicoes.js")
        .map(|nome| read(&format!("ui/{nome}")))
        .collect();
    assert!(
        telas.contains("\"decorar\""),
        "`decorar` voltou à tabela e nenhuma tela o aplica: é o registro que \
         aceita sem produzir efeito, que é exatamente o R3 da revisão de \
         20/09/2026"
    );
}
