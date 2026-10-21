//! Os guardas da casca do celular (F8 da 0.16).
//!
//! O spec é `docs/superpowers/specs/2026-10-09-0.16-celular-design.md`, §9. A casca
//! do celular é a mesma `ui/` do desktop, reorganizada por `celular-*.css` e com a
//! moldura em `celular-*.js`. Estes guardas seguram as três promessas que tornam
//! isso seguro: o desktop não é alcançado (G1, G2, G4), nenhuma ação nova nasce
//! fora do caminho de sempre (G5), e o tema não chega ao núcleo (G6, G7).

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;

/// As cascas do navegador: o prefixo dos arquivos, a raiz que toda regra de CSS
/// dela começa, e a primeira linha de todo script dela.
const CASCAS: &[(&str, &str, &str)] = &[
    (
        "celular-",
        ":root[data-casca=\"celular\"]",
        "if (document.documentElement.dataset.casca !== \"celular\") return;",
    ),
    // A casca larga (D-web-3): o desktop no navegador de um computador.
    (
        "larga-",
        ":root[data-casca=\"larga\"]",
        "if (document.documentElement.dataset.casca !== \"larga\") return;",
    ),
];

fn ui() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui")
}

fn ler(nome: &str) -> String {
    std::fs::read_to_string(ui().join(nome))
        .unwrap_or_else(|erro| panic!("ui/{nome}: {erro}"))
        .replace("\r\n", "\n")
}

/// Os arquivos de uma casca com esta extensão, achados olhando o diretório, e não
/// por uma lista que alguém tem de lembrar de estender.
fn da_casca(prefixo: &str, extensao: &str) -> Vec<(String, String)> {
    let mut achados: Vec<(String, String)> = std::fs::read_dir(ui())
        .expect("ui/")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(prefixo) && n.ends_with(extensao))
        .map(|n| {
            let texto = ler(&n);
            (n, texto)
        })
        .collect();
    achados.sort();
    assert!(
        !achados.is_empty(),
        "ui/ não tem nenhum {prefixo}*{extensao}: um guarda sem arquivo passa sem ler nada"
    );
    achados
}

/// Todos os arquivos das cascas do navegador com esta extensão.
fn das_cascas(extensao: &str) -> Vec<(String, String)> {
    CASCAS
        .iter()
        .flat_map(|(prefixo, _, _)| da_casca(prefixo, extensao))
        .collect()
}

fn sem_comentarios_css(css: &str) -> String {
    let mut saida = String::with_capacity(css.len());
    let mut resto = css;
    while let Some(inicio) = resto.find("/*") {
        saida.push_str(&resto[..inicio]);
        match resto[inicio..].find("*/") {
            Some(fim) => resto = &resto[inicio + fim + 2..],
            None => return saida,
        }
    }
    saida.push_str(resto);
    saida
}

/// O mesmo corte de `tests/frontend.rs` (`without_comments`): blocos `/* */`, e
/// `//` até o fim da linha quando não há aspa antes dele na linha.
fn sem_comentarios_js(js: &str) -> String {
    sem_comentarios_css(js)
        .lines()
        .map(|linha| match linha.find("//") {
            Some(em) if !linha[..em].contains(['"', '`', '\'']) => &linha[..em],
            _ => linha,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Um bloco de regra: o prelúdio (seletor ou at-rule) e o corpo. `@media`,
/// `@supports` e `@container` abrem blocos de regras; o resto é devolvido inteiro.
struct Bloco {
    preludio: String,
    corpo: String,
}

fn blocos(css: &str) -> Vec<Bloco> {
    blocos_com_contexto(css)
        .into_iter()
        .map(|(_, bloco)| bloco)
        .collect()
}

/// Como `blocos`, e com as at-rules em volta de cada um, de fora para dentro.
fn blocos_com_contexto(css: &str) -> Vec<(Vec<String>, Bloco)> {
    fn ler_blocos(texto: &str, contexto: &[String], saida: &mut Vec<(Vec<String>, Bloco)>) {
        let mut resto = texto;
        while let Some(abre) = resto.find('{') {
            let preludio = resto[..abre].trim().to_owned();
            let mut nivel = 0usize;
            let mut fecha = None;
            for (i, c) in resto[abre..].char_indices() {
                match c {
                    '{' => nivel += 1,
                    '}' => {
                        nivel -= 1;
                        if nivel == 0 {
                            fecha = Some(abre + i);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let fecha = fecha.unwrap_or_else(|| panic!("chave sem par perto de «{preludio}»"));
            let corpo = &resto[abre + 1..fecha];
            if ["@media", "@supports", "@container"]
                .iter()
                .any(|a| preludio.starts_with(a))
            {
                let mut dentro = contexto.to_vec();
                dentro.push(preludio);
                ler_blocos(corpo, &dentro, saida);
            } else {
                saida.push((
                    contexto.to_vec(),
                    Bloco {
                        preludio,
                        corpo: corpo.to_owned(),
                    },
                ));
            }
            resto = &resto[fecha + 1..];
        }
    }
    let mut saida = Vec::new();
    ler_blocos(&sem_comentarios_css(css), &[], &mut saida);
    saida
}

/// Os seletores de uma lista, separados só pelas vírgulas de fora: dentro de
/// `:is(button, .botao)` a vírgula é do argumento, e partir ali reprovaria uma
/// regra que começa pela marca.
fn seletores(preludio: &str) -> Vec<&str> {
    let mut partes = Vec::new();
    let mut nivel = 0i32;
    let mut inicio = 0;
    for (i, c) in preludio.char_indices() {
        match c {
            '(' | '[' => nivel += 1,
            ')' | ']' => nivel -= 1,
            ',' if nivel == 0 => {
                partes.push(&preludio[inicio..i]);
                inicio = i + 1;
            }
            _ => {}
        }
    }
    partes.push(&preludio[inicio..]);
    partes
}

/// G1. Sem a marca, nada do celular casa com nada do desktop. Uma animação tem
/// nome global, e por isso o dela tem prefixo: um `@keyframes seele-varredura`
/// aqui trocaria a varredura do desktop sem casar seletor nenhum.
#[test]
fn toda_regra_das_cascas_do_navegador_comeca_pela_marca() {
    let mut fora = Vec::new();
    for (prefixo, marca, _) in CASCAS {
        for (nome, css) in da_casca(prefixo, ".css") {
            for bloco in blocos(&css) {
                if let Some(animacao) = bloco.preludio.strip_prefix("@keyframes") {
                    if !animacao.trim().starts_with("cel-") {
                        fora.push(format!("{nome}: @keyframes{animacao}"));
                    }
                    continue;
                }
                for seletor in seletores(&bloco.preludio) {
                    let seletor_ok = seletor.trim().starts_with(marca)
                        || seletor.trim().starts_with(&format!(":where({marca})"));
                    if !seletor_ok {
                        fora.push(format!(
                            "{nome}: {} (a raiz dela é {marca})",
                            seletor.trim()
                        ));
                    }
                }
            }
        }
    }
    assert!(
        fora.is_empty(),
        "estas regras não começam pela raiz da casca delas (ou animam com um nome sem `cel-`), \
         e alcançam o desktop:\n{}",
        fora.join("\n")
    );
}

/// As formas de ler a marca: o atributo (num seletor de CSS, num `[data-casca`
/// de JS, num `getAttribute`) e o `dataset`. O `?.` sai antes da comparação: a
/// leitura da oitava integração era `dataset?.casca`.
const LEITURAS_DA_MARCA: &[&str] = &[
    "data-casca",
    "dataset.casca",
    "dataset[\"casca\"]",
    "dataset['casca']",
    "dataset[`casca`]",
];

/// O código de um arquivo de texto da `ui/`, sem os comentários da língua dele.
fn codigo_de(nome: &str, texto: &str) -> String {
    if nome.ends_with(".js") {
        sem_comentarios_js(texto)
    } else if nome.ends_with(".css") {
        sem_comentarios_css(texto)
    } else {
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
        saida
    }
}

/// G2. A marca é do celular e da ponte da F4, e de mais ninguém: um arquivo do
/// desktop que a lesse passaria a mudar com ela. Lê o código sem os comentários,
/// e em toda forma de ler a marca: na oitava integração, o `cascaDoRetrato` da F3
/// lia `dataset?.casca`, e o guarda só reprovou porque o comentário ao lado citava
/// `data-casca`. Sem o comentário, o código passava; com o comentário e sem o
/// código, o guarda reprovava um texto.
#[test]
fn nenhum_arquivo_do_desktop_cita_a_marca() {
    let mut leem = Vec::new();
    let mut lidos = 0;
    for entrada in std::fs::read_dir(ui()).expect("ui/").filter_map(Result::ok) {
        let nome = entrada.file_name().to_string_lossy().into_owned();
        let do_desktop = !CASCAS
            .iter()
            .any(|(prefixo, _, _)| nome.starts_with(prefixo))
            && nome != "ponte-web.js";
        let de_texto = [".css", ".js", ".html"].iter().any(|e| nome.ends_with(e));
        if !(do_desktop && de_texto) {
            continue;
        }
        lidos += 1;
        let codigo = codigo_de(&nome, &ler(&nome))
            .replace("?.[", "[")
            .replace("?.", ".");
        for leitura in LEITURAS_DA_MARCA {
            if codigo.contains(leitura) {
                leem.push(format!("{nome}: {leitura}"));
            }
        }
    }
    assert!(
        lidos >= 20,
        "o guarda leu {lidos} arquivos do desktop: a leitura da ui/ quebrou"
    );
    assert!(
        leem.is_empty(),
        "o desktop lê a marca da casca, e passaria a mudar com ela:\n{}",
        leem.join("\n")
    );
}

/// G4. Um script do celular carregado no desktop não faz nada: é uma IIFE cuja
/// primeira linha volta sem a marca.
#[test]
fn todo_script_das_cascas_do_navegador_e_inerte_sem_a_marca() {
    for (prefixo, _, guarda) in CASCAS {
        for (nome, js) in da_casca(prefixo, ".js") {
            let codigo = sem_comentarios_js(&js);
            let codigo = codigo.trim().trim_start_matches("\"use strict\";").trim();
            assert!(
                codigo.starts_with("(() => {") && codigo.ends_with("})();"),
                "{nome}: o código mora inteiro numa IIFE, e nada roda fora dela"
            );
            let corpo = codigo["(() => {".len()..].trim_start();
            assert!(
                corpo.starts_with(guarda),
                "{nome}: a primeira linha da IIFE tem de ser `{guarda}`"
            );
        }
    }
}

/// G5. «Nenhum `invoke` novo» é promessa só enquanto nenhum script do celular
/// chama o Rust: as ações passam pelos controles e funções que já existem.
#[test]
fn nenhum_script_das_cascas_do_navegador_chama_o_invoke() {
    for (nome, js) in das_cascas(".js") {
        assert!(
            !sem_comentarios_js(&js).contains("invoke("),
            "{nome} chama `invoke(`: o celular aciona o caminho do desktop, e não abre outro"
        );
    }
}

/// G7. O tema escreve uma lista fechada de `--seele-*`, e o núcleo do celular é
/// `--nucleo-*` declarado na raiz. Se uma tabela do tema passasse a escrever um
/// `--nucleo-*`, o MUDO teria dono fora do produto.
#[test]
fn o_tema_nunca_escreve_o_nucleo() {
    let base = ler("base.js");
    for tabela in [
        "TEMA_DA_API",
        "NUMEROS_DA_API",
        "BANDEIRAS_DA_API",
        "MEDIDAS_DA_API",
    ] {
        let inicio = base
            .find(&format!("const {tabela} = Object.freeze("))
            .unwrap_or_else(|| {
                panic!("base.js não declara {tabela}: a leitura deste guarda quebrou")
            });
        let corpo = &base[inicio..];
        let corpo = &corpo[..corpo.find("\n});").expect("o fim da tabela")];
        let tokens: Vec<&str> = corpo.split('"').filter(|s| s.starts_with("--")).collect();
        assert!(
            !tokens.is_empty(),
            "{tabela} sem nenhum token: a leitura deste guarda quebrou"
        );
        for token in tokens {
            assert!(
                token.starts_with("--seele-"),
                "{tabela} escreve {token}, e o tema só escreve `--seele-*`"
            );
        }
    }
}

/// G6. O núcleo do celular lê só o núcleo: nenhuma cor, borda, fundo ou família de
/// letra dos seletores do núcleo vem de um `--seele-*`, que o tema reescreve.
const SELETORES_DO_NUCLEO: &[&str] = &[
    "#voz-mudo",
    "#voz-surdo",
    "#voz-sair",
    ".voz-nucleo",
    ".voz-controles",
    ".cel-selo-seguro",
    "#cel-aba-ajustes",
    "[data-nucleo]",
    ".cel-nucleo",
    ".cel-placa-de-alerta",
    // A mensagem que não foi gravada: a placa de alerta é do núcleo, e um tema
    // não apaga o aviso de que algo se perdeu.
    "[data-envio=\"falhou\"]",
    "[data-envio=\"sem-resposta\"]",
    // A linha MODS DESTE SERVIDOR em AJUSTES: a saída do tema, como a aba.
    "#secao-mods",
];
const PROPRIEDADES_DE_COR: &[&str] = &[
    "color",
    "background",
    "background-color",
    "border",
    "border-color",
    "border-top",
    "border-top-color",
    "border-right",
    "border-bottom",
    "border-left",
    "border-left-color",
    "outline",
    "outline-color",
    "font-family",
    "box-shadow",
];

#[test]
fn o_nucleo_do_celular_so_le_o_nucleo() {
    let mut lidos = 0;
    let mut fora = Vec::new();
    for (nome, css) in da_casca("celular-", ".css") {
        for bloco in blocos(&css) {
            if !SELETORES_DO_NUCLEO
                .iter()
                .any(|s| bloco.preludio.contains(s))
            {
                continue;
            }
            lidos += 1;
            for declaracao in bloco.corpo.split(';') {
                let Some((propriedade, valor)) = declaracao.split_once(':') else {
                    continue;
                };
                if PROPRIEDADES_DE_COR.contains(&propriedade.trim())
                    && valor.contains("var(--seele-")
                {
                    fora.push(format!(
                        "{nome}: {} {{ {}: {} }}",
                        bloco.preludio,
                        propriedade.trim(),
                        valor.trim()
                    ));
                }
            }
        }
    }
    assert!(lidos >= 3, "o guarda achou {lidos} regras do núcleo: a faixa de voz não está nos celular-*.css que ele lê");
    assert!(
        fora.is_empty(),
        "o núcleo lê o que o tema escreve:\n{}",
        fora.join("\n")
    );
}

// ------------------------------------------------------- o texto do sistema

/// O texto do sistema (spec, §5): a casca escala a escala de tipo inteira do
/// produto. O desktop declara o tamanho da letra em cerca de 270 regras, metade
/// em token e metade em px, e escalar à mão uma a uma deixava de fora o que
/// ninguém lembrou (medido no Safari do Simulador: as seções de AJUSTES, a
/// CHAMADA, o campo de mensagem e as folhas ficavam no tamanho do desktop, ao
/// lado de mensagens no dobro).
///
/// Por isso cada regra do desktop que declara tamanho de letra ganha um gêmeo
/// gerado, `:where(<marca>) <o mesmo seletor>`, na mesma ordem:
/// - o tamanho que escala vem vezes a escala (`calc(<tamanho> *
///   var(--cel-escala))`);
/// - o que segue o pai (`inherit`, `em`, `%`, `font: inherit`) vem igual, e
///   cresce com o pai;
/// - o de `NO_TAMANHO_DO_DESKTOP` vem igual, e fica.
///
/// O `:where()` não pesa: o gêmeo tem a especificidade da regra dele, e o
/// arquivo carrega depois do desktop. Como toda declaração de tamanho tem o seu
/// gêmeo na mesma ordem, o cascateamento do desktop se repete inteiro, escalado:
/// um gêmeo que faltasse deixaria um gêmeo anterior vencer a regra que antes o
/// vencia (medido: o `.linha-renomear { font-size: inherit }` perdia para o
/// gêmeo do `.botao-fantasma`, e o lápis do canal ia de 14 a 13 px a 1). As
/// regras escritas à mão nos `celular-*.css`, que pesam a marca, continuam
/// ganhando (os tetos das abas, do núcleo e da telemetria). A 1, todo nó tem o
/// mesmo tamanho com e sem o arquivo: a bancada confere (caso `texto-maior`).
const TEXTO_GERADO: &str = "celular-texto.css";

/// O que fica no tamanho do desktop com qualquer texto do sistema: o gêmeo
/// repete o tamanho, sem a escala. A bancada aceita os mesmos (`TEXTO_COM_TETO`,
/// caso `texto-maior`).
const NO_TAMANHO_DO_DESKTOP: &[(&str, &str)] = &[
    (
        ".trilha-item",
        "as iniciais de um servidor numa caixa de tamanho fixo: marca, e não texto de ler",
    ),
    (".trilha-mais", "o + da trilha, na mesma caixa"),
    (
        ".mensagem-avatar",
        "as iniciais de quem escreveu, numa caixa de tamanho fixo",
    ),
    (
        ".chamada-avatar",
        "as iniciais do cartão da CHAMADA, numa caixa de tamanho fixo",
    ),
    (
        ".barra",
        "a barra do sinal é desenho; o número e a palavra ao lado crescem",
    ),
    (".pessoa .barra", "a mesma barra, na lista de PESSOAS"),
    (".sync-marca", "o pedaço vazio da barra"),
    (
        ".chamada-cartao-barra",
        "a barra do sinal no cartão da CHAMADA",
    ),
    (".chamada-cartao-marca", "o pedaço vazio dela"),
    (
        ".server-atualizacao-barra",
        "a barra do progresso da atualização",
    ),
];

/// As folhas do desktop, na ordem em que o index.html as carrega.
fn folhas_do_desktop() -> Vec<String> {
    let html = ler("index.html");
    let folhas: Vec<String> = html
        .split("<link rel=\"stylesheet\" href=\"")
        .skip(1)
        .filter_map(|resto| resto.split('"').next())
        .filter(|folha| {
            !CASCAS
                .iter()
                .any(|(prefixo, _, _)| folha.starts_with(prefixo))
        })
        .map(str::to_owned)
        .collect();
    assert!(
        folhas.len() >= 10,
        "o index.html carrega só {folhas:?}: a leitura das folhas quebrou"
    );
    folhas
}

/// O tamanho de letra que uma regra declara, quando declara.
enum Tamanho {
    /// Em px, rem ou token: o gêmeo multiplica pela escala.
    Escala(String),
    /// `inherit`, `em` ou `%`: o gêmeo repete, e ele cresce com o pai.
    SegueOPai(String),
}

/// A última declaração de tamanho de uma regra, contando o `font: inherit`. Uma
/// forma que este guarda não conhece reprova, e não passa calada.
fn tamanho_da_letra(onde: &str, corpo: &str) -> Option<Tamanho> {
    let numero = |valor: &str, unidade: &str| {
        valor
            .strip_suffix(unidade)
            .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit() || c == '.'))
    };
    let mut tamanho = None;
    for declaracao in corpo.split(';') {
        let Some((propriedade, valor)) = declaracao.split_once(':') else {
            continue;
        };
        let valor = valor.trim();
        match propriedade.trim() {
            "font-size" => {
                tamanho = Some(
                    if numero(valor, "px")
                        || numero(valor, "rem")
                        || (valor.starts_with("var(--seele-t-") && valor.ends_with(')'))
                    {
                        Tamanho::Escala(valor.to_owned())
                    } else if valor == "inherit" || numero(valor, "em") || numero(valor, "%") {
                        Tamanho::SegueOPai(valor.to_owned())
                    } else {
                        panic!(
                            "{onde}: `font-size: {valor}` é uma forma que o gerador do texto do \
                             sistema não conhece; ensine `tamanho_da_letra` a lê-la"
                        )
                    },
                );
            }
            "font" if valor == "inherit" => {
                tamanho = Some(Tamanho::SegueOPai("inherit".to_owned()))
            }
            "font" => panic!(
                "{onde}: `font: {valor}` traz o tamanho no atalho; ensine `tamanho_da_letra` \
                 a lê-lo"
            ),
            _ => {}
        }
    }
    tamanho
}

/// O conteúdo de `celular-texto.css`: os gêmeos, folha a folha, na ordem do
/// index.html, com as at-rules de cada regra em volta dela.
fn texto_do_celular() -> String {
    let marca = CASCAS[0].1;
    let mut saida = String::from(
        "/* SEELE — o texto do sistema no celular (F8). GERADO: não edite à mão.\n \
         *\n \
         * Regere com `SEELE_REGERAR_TEXTO=1 cargo test -p seele-app --test celular`,\n \
         * que também reprova quando este arquivo fica atrás do CSS do desktop. O\n \
         * porquê e o que fica no tamanho estão em `tests/celular.rs` (`TEXTO_GERADO`,\n \
         * `NO_TAMANHO_DO_DESKTOP`) e no spec, §5. */\n",
    );
    for folha in folhas_do_desktop() {
        let mut da_folha = String::new();
        for (contexto, bloco) in blocos_com_contexto(&ler(&folha)) {
            if bloco.preludio.starts_with('@') {
                continue;
            }
            let onde = format!("{folha}: {}", bloco.preludio);
            let Some(tamanho) = tamanho_da_letra(&onde, &bloco.corpo) else {
                continue;
            };
            // Os seletores da regra, separados pelo valor do gêmeo: o tamanho vezes
            // a escala, ou o mesmo tamanho (o que segue o pai e o que fica).
            let mut escalados = Vec::new();
            let mut iguais = Vec::new();
            for seletor in seletores(&bloco.preludio) {
                let seletor = seletor.split_whitespace().collect::<Vec<_>>().join(" ");
                // A raiz não casa debaixo da marca; o corpo, a casca escala ela
                // mesma em celular-base.css.
                if seletor == "html" {
                    continue;
                }
                assert!(
                    !seletor.starts_with(":root") && !seletor.starts_with("html"),
                    "{onde}: um seletor preso à raiz não tem gêmeo debaixo da marca"
                );
                let gemeo = format!(":where({marca}) {seletor}");
                let fica = NO_TAMANHO_DO_DESKTOP.iter().any(|(s, _)| *s == seletor);
                match &tamanho {
                    Tamanho::Escala(_) if !fica => escalados.push(gemeo),
                    _ => iguais.push(gemeo),
                }
            }
            let valor = match &tamanho {
                Tamanho::Escala(v) | Tamanho::SegueOPai(v) => v,
            };
            let recuo = "  ".repeat(contexto.len());
            let mut regras = String::new();
            for (gemeos, declarado) in [
                (escalados, format!("calc({valor} * var(--cel-escala))")),
                (iguais, valor.clone()),
            ] {
                if !gemeos.is_empty() {
                    regras.push_str(&format!(
                        "{recuo}{} {{ font-size: {declarado}; }}\n",
                        gemeos.join(&format!(",\n{recuo}"))
                    ));
                }
            }
            if regras.is_empty() {
                continue;
            }
            for (nivel, at) in contexto.iter().enumerate().rev() {
                let fora = "  ".repeat(nivel);
                regras = format!("{fora}{at} {{\n{regras}{fora}}}\n");
            }
            da_folha.push_str(&regras);
        }
        if !da_folha.is_empty() {
            saida.push_str(&format!("\n/* {folha} */\n{da_folha}"));
        }
    }
    saida
}

/// Com `SEELE_REGERAR_TEXTO=1`, escreve o arquivo uma vez por processo, e de uma
/// vez: os guardas deste arquivo correm em paralelo, e o G1 lê os `celular-*.css`
/// enquanto este escreve. O nome provisório não termina em `.css`, e o G1 não o vê.
fn regerar_o_texto_se_pedido() {
    static FEITO: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    if std::env::var("SEELE_REGERAR_TEXTO").as_deref() != Ok("1") {
        return;
    }
    FEITO.get_or_init(|| {
        let provisorio = ui().join(format!("{TEXTO_GERADO}.novo"));
        std::fs::write(&provisorio, texto_do_celular()).expect("o texto gerado é gravável");
        std::fs::rename(&provisorio, ui().join(TEXTO_GERADO))
            .expect("o texto gerado troca de nome");
    });
}

#[test]
fn o_texto_do_sistema_alcanca_todo_tamanho_do_desktop() {
    regerar_o_texto_se_pedido();
    let esperado = texto_do_celular();
    let no_disco = ler(TEXTO_GERADO);
    assert!(
        no_disco == esperado,
        "ui/{TEXTO_GERADO} está atrás do CSS do desktop: uma regra nova ou mudada não cresce \
         com o texto do sistema no celular. Regere com SEELE_REGERAR_TEXTO=1 cargo test -p \
         seele-app --test celular"
    );
    // O gêmeo vence a regra dele por vir depois: o arquivo gerado carrega depois de
    // toda folha do desktop que tem gêmeo nele.
    let html = ler("index.html");
    let posicao = |folha: &str| {
        html.find(&format!("href=\"{folha}\""))
            .unwrap_or_else(|| panic!("o index.html não carrega {folha}"))
    };
    let gerado = posicao(TEXTO_GERADO);
    for folha in folhas_do_desktop() {
        if no_disco.contains(&format!("/* {folha} */")) {
            assert!(
                posicao(&folha) < gerado,
                "{folha} carrega depois de {TEXTO_GERADO}, e a regra dele vence o gêmeo"
            );
        }
    }
}

// ---------------------------------------------------------------- as frases

/// Os literais de texto de um código JS sem comentários, nas três aspas. Num
/// literal de crase, cada `${…}` vira um espaço: o que se lê é o texto em volta.
fn literais_js(codigo: &str) -> Vec<String> {
    let mut saida = Vec::new();
    let mut letras = codigo.chars().peekable();
    while let Some(c) = letras.next() {
        if !matches!(c, '"' | '\'' | '`') {
            continue;
        }
        let mut literal = String::new();
        let mut chaves = 0usize;
        while let Some(d) = letras.next() {
            match d {
                '\\' => {
                    if let Some(e) = letras.next() {
                        literal.push(e);
                    }
                }
                '$' if c == '`' && chaves == 0 && letras.peek() == Some(&'{') => {
                    letras.next();
                    chaves = 1;
                    literal.push(' ');
                }
                '{' if chaves > 0 => chaves += 1,
                '}' if chaves > 0 => chaves -= 1,
                _ if chaves > 0 => {}
                _ if d == c => break,
                _ => literal.push(d),
            }
        }
        saida.push(literal);
    }
    saida
}

/// Se um literal é uma frase de tela, e não um seletor, um nome de classe ou de
/// evento: um rótulo em caixa alta (uma palavra de quatro letras ou mais, toda
/// maiúscula), uma letra acentuada, ou uma das palavras curtas do português
/// entre espaços. Os nomes de etiqueta do DOM que o código compara são caixa
/// alta e não são frase.
fn e_frase(literal: &str) -> bool {
    const ETIQUETAS: &[&str] = &["INPUT", "TEXTAREA", "BUTTON", "SELECT"];
    const CURTAS: &[&str] = &[
        "de", "da", "do", "das", "dos", "o", "a", "os", "as", "é", "e", "em", "no", "na", "nos",
        "nas", "um", "uma", "para", "pelo", "pela", "com", "que", "se", "não", "ao",
    ];
    if ETIQUETAS.contains(&literal) {
        return false;
    }
    let palavras: Vec<&str> = literal
        .split(|c: char| !c.is_alphabetic())
        .filter(|p| !p.is_empty())
        .collect();
    let rotulo = palavras
        .iter()
        .any(|p| p.chars().count() >= 4 && p.chars().all(char::is_uppercase));
    let acento = literal
        .chars()
        .any(|c| "áéíóúâêôãõçàÁÉÍÓÚÂÊÔÃÕÇÀ".contains(c));
    let portugues = literal.contains(' ')
        && literal
            .split_whitespace()
            .any(|p| CURTAS.contains(&p.to_lowercase().as_str()));
    rotulo || acento || portugues
}

/// As frases das cascas do navegador moram no `frases.js` (P10): nenhum script
/// delas escreve uma frase de tela. É a fronteira de sempre do produto (o
/// cabeçalho do `frases.js`): uma tela nova que precise dizer algo acrescenta a
/// frase lá, e não um dicionário próprio.
#[test]
fn nenhum_script_das_cascas_do_navegador_escreve_frase_de_tela() {
    let mut escritas = Vec::new();
    let mut lidos = 0;
    for (nome, js) in das_cascas(".js") {
        for literal in literais_js(&sem_comentarios_js(&js)) {
            lidos += 1;
            if e_frase(&literal) {
                escritas.push(format!("{nome}: «{literal}»"));
            }
        }
    }
    assert!(
        lidos >= 100,
        "o guarda leu {lidos} literais: a leitura dos scripts quebrou"
    );
    assert!(
        escritas.is_empty(),
        "estas frases estão escritas nos scripts das cascas, e vão para FRASES_DO_CELULAR \
         ou FRASES_DA_LARGA, no frases.js:\n{}",
        escritas.join("\n")
    );
}

/// G8. O ponto da casca na coluna de salas (`CASCA.vestirASala`, aprovado pela
/// orquestradora em 16/10; pendência 63(b) no celular): **uma chamada só**, no
/// `desenharCanais`, sobre o `li` montado, **antes** do `reconciliarALista` da
/// coluna. Depois da reconciliação, o que a casca acrescenta sairia no quadro
/// seguinte, e o ENTRAR e o ENTRAR MUDO voltariam a ser outros nós a cada quadro
/// (o caso `salas-pelo-id` da `bancada-do-celular.py` mede isso no navegador).
/// Duas chamadas vestiriam a sala duas vezes. No desktop o ponto não faz nada: o
/// `CASCA` do `base.js` o declara vazio.
#[test]
fn a_casca_veste_a_sala_uma_vez_e_antes_da_reconciliacao() {
    let sessao = codigo_de("tela-sessao.js", &ler("tela-sessao.js"));
    let chamadas: Vec<usize> = sessao
        .match_indices("CASCA.vestirASala(")
        .map(|(onde, _)| onde)
        .collect();
    assert_eq!(
        chamadas.len(),
        1,
        "tela-sessao.js chama o CASCA.vestirASala {} vezes, e não uma",
        chamadas.len()
    );
    let desenhar = sessao
        .find("function desenharCanais(")
        .expect("tela-sessao.js sem o desenharCanais: a leitura deste guarda quebrou");
    let reconciliar = sessao[desenhar..]
        .find("reconciliarALista(")
        .map(|onde| desenhar + onde)
        .expect("o desenharCanais sem o reconciliarALista: a leitura deste guarda quebrou");
    assert!(
        desenhar < chamadas[0] && chamadas[0] < reconciliar,
        "o CASCA.vestirASala tem de estar no desenharCanais, antes do reconciliarALista da coluna de salas"
    );

    let base = codigo_de("base.js", &ler("base.js"));
    assert!(
        base.contains("vestirASala: (_item, _sala, _snapshot) => {}"),
        "o CASCA do base.js tem de declarar o vestirASala vazio: no desktop, o ponto não faz nada"
    );
}

/// G9. O `pessoa.detalhes` tem um dono por casca (aprovado pela orquestradora em
/// 16/10). A contribuição é um nó por pessoa (o cache de `montarContribuicao`),
/// e um nó não mora em dois lugares: no desktop e na larga, a linha da pessoa o
/// monta; no celular, só a folha. Quem decide é o `CASCA.detalhesNaLinha`,
/// **consultado num lugar só** do `tela-sessao.js`, onde as duas linhas (a
/// nativa e a apresentada) o acrescentam; e ninguém mais monta `pessoa.detalhes`.
/// Uma segunda consulta, ou um terceiro lugar que o monte, é a porta para o nó
/// voltar a ser disputado (a folha o tirava da linha, e a linha o tomava de volta
/// a cada quadro: o caso `detalhes-da-pessoa` da bancada mede isso).
#[test]
fn o_detalhe_da_pessoa_tem_um_dono_por_casca() {
    let sessao = codigo_de("tela-sessao.js", &ler("tela-sessao.js"));
    let consultas = sessao.matches("CASCA.detalhesNaLinha").count();
    assert_eq!(
        consultas, 1,
        "tela-sessao.js consulta o CASCA.detalhesNaLinha {consultas} vezes, e não uma"
    );

    let montagem = "conteudoDasContribuicoes(\"pessoa.detalhes\"";
    let mut onde = Vec::new();
    for entrada in std::fs::read_dir(ui()).expect("ui/ ilegível") {
        let nome = entrada
            .expect("ui/ ilegível")
            .file_name()
            .to_string_lossy()
            .into_owned();
        if !nome.ends_with(".js") {
            continue;
        }
        let vezes = codigo_de(&nome, &ler(&nome)).matches(montagem).count();
        if vezes > 0 {
            onde.push(format!("{nome} ×{vezes}"));
        }
    }
    onde.sort();
    assert_eq!(
        onde,
        ["celular-folhas.js ×1", "tela-sessao.js ×1"],
        "o pessoa.detalhes é montado fora dos dois lugares combinados (a linha, pelo ponto, e a folha do celular)"
    );

    let base = codigo_de("base.js", &ler("base.js"));
    assert!(
        base.contains("detalhesNaLinha: true"),
        "o CASCA do base.js tem de declarar o detalhesNaLinha verdadeiro: no desktop, a linha monta"
    );
    let folhas = codigo_de("celular-folhas.js", &ler("celular-folhas.js"));
    assert!(
        folhas.contains("CASCA.detalhesNaLinha = false"),
        "o celular tem de desligar o detalhesNaLinha: lá, só a folha monta"
    );
}

// ---------------------------------------------------------------------------
// G10. Um nome global, um script.
//
// Os scripts clássicos de uma página dividem um escopo global só: uma `function`
// declarada de novo toma o lugar da outra, calada, e um `let`, `const` ou
// `class` repetido derruba o script que chega depois. Achado pela F4 (21/10): a
// `instalarDoCatalogo` do `camada-mods.js`, que carrega depois da
// `ponte-web.js`, tomava o lugar da dela, e o comando respondia `false` sem
// buscar nada. As três cascas (desktop, celular e web) carregam o mesmo
// `index.html`, na mesma ordem (o pacote do web app copia a `ui/` como ela é);
// o `executor-de-mod.html` é outra página, com o escopo dele.

/// Os nomes que dois scripts da mesma página declaram **de propósito**, cada um
/// com a razão. Fechada: um nome repetido fora dela reprova, e um nome dela que
/// deixou de ser repetido também, para ela não envelhecer.
const GLOBAIS_REPETIDOS_DE_PROPOSITO: &[(&str, &str)] = &[];

#[derive(Debug, Clone, PartialEq)]
enum Ficha {
    Nome(String),
    Sinal(char),
    /// Uma cadeia, um modelo, uma expressão regular ou um número.
    Valor,
}

#[derive(Debug)]
struct Lida {
    ficha: Ficha,
    /// Quantas chaves, parênteses e colchetes estão abertos em volta dela.
    fundura: i32,
    /// Se uma quebra de linha veio antes dela (a inserção de `;` do JS).
    linha_nova_antes: bool,
}

fn comeca_nome(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '$'
}

fn segue_nome(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Depois desta ficha, uma `/` abre uma expressão regular, e não divide.
fn regex_depois(anterior: Option<&Lida>) -> bool {
    match anterior.map(|l| &l.ficha) {
        None => true,
        Some(Ficha::Sinal(c)) => !matches!(c, ')' | ']' | '}'),
        Some(Ficha::Nome(n)) => matches!(
            n.as_str(),
            "return"
                | "typeof"
                | "case"
                | "in"
                | "of"
                | "new"
                | "delete"
                | "void"
                | "throw"
                | "else"
                | "do"
                | "yield"
                | "await"
                | "instanceof"
        ),
        Some(Ficha::Valor) => false,
    }
}

fn pular_cadeia(c: &[char], mut i: usize) -> usize {
    let aspa = c[i];
    i += 1;
    while i < c.len() && c[i] != aspa && c[i] != '\n' {
        i += if c[i] == '\\' { 2 } else { 1 };
    }
    i + 1
}

fn pular_regex(c: &[char], mut i: usize) -> usize {
    let mut classe = false;
    i += 1;
    while i < c.len() && c[i] != '\n' {
        match c[i] {
            '\\' => i += 1,
            '[' => classe = true,
            ']' => classe = false,
            '/' if !classe => {
                i += 1;
                while i < c.len() && c[i].is_ascii_alphabetic() {
                    i += 1;
                }
                return i;
            }
            _ => {}
        }
        i += 1;
    }
    i
}

/// Um modelo (`` `…` ``), com as expressões `${…}` de dentro.
fn pular_modelo(c: &[char], mut i: usize) -> usize {
    i += 1;
    while i < c.len() {
        match c[i] {
            '\\' => i += 2,
            '`' => return i + 1,
            '$' if c.get(i + 1) == Some(&'{') => i = ler_fichas(c, i + 2, true).1,
            _ => i += 1,
        }
    }
    i
}

/// As fichas de um trecho de JS, sem os comentários. Com `ate_o_fecho`, para no
/// `}` que fecha uma expressão `${…}` de modelo e devolve o índice depois dele.
fn ler_fichas(c: &[char], mut i: usize, ate_o_fecho: bool) -> (Vec<Lida>, usize) {
    let mut saida: Vec<Lida> = Vec::new();
    let mut fundura = 0;
    let mut linha_nova = false;
    while i < c.len() {
        let ch = c[i];
        if ch == '\n' {
            linha_nova = true;
            i += 1;
            continue;
        }
        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        if ch == '/' && c.get(i + 1) == Some(&'/') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if ch == '/' && c.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < c.len() && !(c[i] == '*' && c[i + 1] == '/') {
                linha_nova |= c[i] == '\n';
                i += 1;
            }
            i += 2;
            continue;
        }
        let ficha = if ch == '\'' || ch == '"' {
            i = pular_cadeia(c, i);
            Ficha::Valor
        } else if ch == '`' {
            i = pular_modelo(c, i);
            Ficha::Valor
        } else if ch == '/' && regex_depois(saida.last()) {
            i = pular_regex(c, i);
            Ficha::Valor
        } else if comeca_nome(ch) {
            let inicio = i;
            while i < c.len() && segue_nome(c[i]) {
                i += 1;
            }
            Ficha::Nome(c[inicio..i].iter().collect())
        } else if ch.is_ascii_digit() {
            while i < c.len() && (segue_nome(c[i]) || c[i] == '.') {
                i += 1;
            }
            Ficha::Valor
        } else {
            i += 1;
            if matches!(ch, '}' | ')' | ']') {
                if ch == '}' && fundura == 0 && ate_o_fecho {
                    return (saida, i);
                }
                fundura -= 1;
            }
            saida.push(Lida {
                ficha: Ficha::Sinal(ch),
                fundura,
                linha_nova_antes: linha_nova,
            });
            linha_nova = false;
            if matches!(ch, '{' | '(' | '[') {
                fundura += 1;
            }
            continue;
        };
        saida.push(Lida {
            ficha,
            fundura,
            linha_nova_antes: linha_nova,
        });
        linha_nova = false;
    }
    (saida, i)
}

/// Uma ficha depois da qual uma quebra de linha pode terminar a instrução.
fn fecha_expressao(ficha: &Ficha) -> bool {
    matches!(
        ficha,
        Ficha::Nome(_) | Ficha::Valor | Ficha::Sinal(')' | ']' | '}')
    )
}

/// A ficha `k` começa uma instrução do nível de cima.
fn comeca_instrucao(f: &[Lida], k: usize) -> bool {
    k == 0
        || matches!(f[k - 1].ficha, Ficha::Sinal(';' | '}'))
        || (f[k].linha_nova_antes && fecha_expressao(&f[k - 1].ficha))
}

/// Os nomes de um padrão de desestruturação (`{ a, b: c, ...d }`, `[a, b]`) que
/// começa em `m`; devolve o índice depois dele.
fn nomes_do_padrao(f: &[Lida], m: usize, nomes: &mut Vec<String>) -> usize {
    let fundura = f[m].fundura;
    let mut k = m + 1;
    while k < f.len() && !(f[k].fundura == fundura && matches!(f[k].ficha, Ficha::Sinal('}' | ']')))
    {
        if let Ficha::Nome(nome) = &f[k].ficha {
            let depois = f.get(k + 1).map(|l| &l.ficha);
            let antes = &f[k - 1].ficha;
            if f[k].fundura == fundura + 1
                && matches!(depois, Some(Ficha::Sinal(',' | '}' | ']' | '=')))
                && *antes != Ficha::Sinal('=')
            {
                nomes.push(nome.clone());
            }
        }
        k += 1;
    }
    k + 1
}

/// Os declaradores de um `let`, `const` ou `var` que começam em `m`, até o fim
/// da instrução; devolve o índice depois dela.
fn nomes_declarados(f: &[Lida], mut m: usize, nomes: &mut Vec<String>) -> usize {
    loop {
        match f.get(m).map(|l| &l.ficha) {
            Some(Ficha::Nome(nome)) => {
                nomes.push(nome.clone());
                m += 1;
            }
            Some(Ficha::Sinal('{' | '[')) => m = nomes_do_padrao(f, m, nomes),
            _ => return m,
        }
        loop {
            match f.get(m) {
                None => return m,
                Some(l) if l.fundura == 0 && l.ficha == Ficha::Sinal(',') => {
                    m += 1;
                    break;
                }
                Some(l) if l.fundura == 0 && l.ficha == Ficha::Sinal(';') => return m + 1,
                Some(l)
                    if l.fundura == 0
                        && l.linha_nova_antes
                        && matches!(l.ficha, Ficha::Nome(_))
                        && fecha_expressao(&f[m - 1].ficha) =>
                {
                    return m;
                }
                _ => m += 1,
            }
        }
    }
}

/// Os nomes que um script clássico declara no escopo global: `function`
/// (também `async` e `function*`), `class`, `let`, `const` e `var` no nível de
/// cima, no começo de uma instrução. Dentro de uma IIFE, nada é global.
fn globais_declarados(js: &str) -> Vec<String> {
    let c: Vec<char> = js.chars().collect();
    let f = ler_fichas(&c, 0, false).0;
    let mut nomes = Vec::new();
    let mut k = 0;
    while k < f.len() {
        let palavra = match &f[k].ficha {
            Ficha::Nome(n) if f[k].fundura == 0 && comeca_instrucao(&f, k) => n.as_str(),
            _ => {
                k += 1;
                continue;
            }
        };
        let funcao_em = match palavra {
            "function" => Some(k),
            "async" if matches!(f.get(k + 1).map(|l| &l.ficha), Some(Ficha::Nome(n)) if n == "function") => {
                Some(k + 1)
            }
            _ => None,
        };
        if let Some(em) = funcao_em {
            let mut m = em + 1;
            if f.get(m).map(|l| &l.ficha) == Some(&Ficha::Sinal('*')) {
                m += 1;
            }
            if let Some(Ficha::Nome(nome)) = f.get(m).map(|l| &l.ficha) {
                nomes.push(nome.clone());
            }
            k = m + 1;
        } else if palavra == "class" {
            if let Some(Ficha::Nome(nome)) = f.get(k + 1).map(|l| &l.ficha) {
                nomes.push(nome.clone());
            }
            k += 2;
        } else if matches!(palavra, "let" | "const" | "var") {
            k = nomes_declarados(&f, k + 1, &mut nomes);
        } else {
            k += 1;
        }
    }
    nomes
}

/// Os scripts clássicos de uma página, na ordem dela: `(nome, código)`. Os de
/// `type="module"` têm escopo próprio e ficam de fora.
fn scripts_da_pagina(pagina: &str) -> Vec<(String, String)> {
    let html = codigo_de(pagina, &ler(pagina));
    let mut scripts = Vec::new();
    let mut resto = html.as_str();
    let mut inline = 0;
    while let Some(abre) = resto.find("<script") {
        let depois = &resto[abre..];
        let fim_da_tag = depois.find('>').expect("uma <script> sem `>`");
        let tag = &depois[..fim_da_tag];
        let corpo_e_resto = &depois[fim_da_tag + 1..];
        let fecha = corpo_e_resto
            .find("</script>")
            .expect("uma <script> sem </script>");
        if !tag.contains("type=\"module\"") {
            match tag.split("src=\"").nth(1).and_then(|s| s.split('"').next()) {
                Some(src) => scripts.push((src.to_string(), ler(src))),
                None => {
                    inline += 1;
                    scripts.push((
                        format!("{pagina} (script {inline} no HTML)"),
                        corpo_e_resto[..fecha].to_string(),
                    ));
                }
            }
        }
        resto = &corpo_e_resto[fecha..];
    }
    scripts
}

#[test]
fn nenhum_nome_global_e_declarado_por_dois_scripts_da_mesma_pagina() {
    let mut repetidos: Vec<String> = Vec::new();
    let mut de_proposito_vistos: Vec<&str> = Vec::new();
    for pagina in ["index.html", "executor-de-mod.html"] {
        let scripts = scripts_da_pagina(pagina);
        assert!(
            !scripts.is_empty(),
            "{pagina} sem script nenhum: a leitura deste guarda quebrou"
        );
        let mut donos: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        let mut total = 0;
        for (nome, codigo) in &scripts {
            let mut deste = globais_declarados(codigo);
            deste.sort();
            deste.dedup();
            total += deste.len();
            for global in deste {
                donos.entry(global).or_default().push(nome.clone());
            }
        }
        if pagina == "index.html" {
            assert!(
                total > 100 && donos.contains_key("desenharCanais") && donos.contains_key("CASCA"),
                "o index.html declara só {total} globais, sem o `desenharCanais` ou o `CASCA`: a leitura deste guarda quebrou"
            );
        }
        for (global, scripts) in donos.into_iter().filter(|(_, s)| s.len() > 1) {
            match GLOBAIS_REPETIDOS_DE_PROPOSITO
                .iter()
                .find(|(nome, _)| *nome == global)
            {
                Some((nome, _)) => de_proposito_vistos.push(nome),
                None => repetidos.push(format!("{pagina}: `{global}` em {}", scripts.join(", "))),
            }
        }
    }
    assert!(
        repetidos.is_empty(),
        "dois scripts da mesma página declaram o mesmo nome global (o de depois toma o lugar do de antes, \
         ou derruba o próprio script):\n{}",
        repetidos.join("\n")
    );
    let velhos: Vec<&str> = GLOBAIS_REPETIDOS_DE_PROPOSITO
        .iter()
        .map(|(nome, _)| *nome)
        .filter(|nome| !de_proposito_vistos.contains(nome))
        .collect();
    assert!(
        velhos.is_empty(),
        "estes nomes estão na lista dos repetidos de propósito e não se repetem mais; saem dela: {velhos:?}"
    );
}
