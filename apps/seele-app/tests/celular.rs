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
/// dela começa, e a primeira linha de todo script dela. A casca larga (D-web-3)
/// entra nesta tabela na Tarefa 17, junto com os arquivos dela.
const CASCAS: &[(&str, &str, &str)] = &[(
    "celular-",
    ":root[data-casca=\"celular\"]",
    "if (document.documentElement.dataset.casca !== \"celular\") return;",
)];

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
    #[expect(
        dead_code,
        reason = "o G6, da Tarefa 3, lê o corpo de cada regra do núcleo"
    )]
    corpo: String,
}

fn blocos(css: &str) -> Vec<Bloco> {
    fn ler_blocos(texto: &str, saida: &mut Vec<Bloco>) {
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
                ler_blocos(corpo, saida);
            } else {
                saida.push(Bloco {
                    preludio,
                    corpo: corpo.to_owned(),
                });
            }
            resto = &resto[fecha + 1..];
        }
    }
    let mut saida = Vec::new();
    ler_blocos(&sem_comentarios_css(css), &mut saida);
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
                    if !seletor.trim().starts_with(marca) {
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

/// G2. A marca é do celular e da ponte da F4, e de mais ninguém: uma folha do
/// desktop que a citasse passaria a mudar com ela.
#[test]
fn nenhum_arquivo_do_desktop_cita_a_marca() {
    let mut citam = Vec::new();
    for entrada in std::fs::read_dir(ui()).expect("ui/").filter_map(Result::ok) {
        let nome = entrada.file_name().to_string_lossy().into_owned();
        let do_desktop = !CASCAS
            .iter()
            .any(|(prefixo, _, _)| nome.starts_with(prefixo))
            && nome != "ponte-web.js";
        let de_texto = [".css", ".js", ".html"].iter().any(|e| nome.ends_with(e));
        if do_desktop && de_texto && ler(&nome).contains("data-casca") {
            citam.push(nome);
        }
    }
    assert!(
        citam.is_empty(),
        "o desktop cita `data-casca` em: {citam:?}"
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
