//! **Onde um símbolo está definido**: o índice com que o `check-api` resolve,
//! a partir da API 6, um apontador pela definição, e não por uma ocorrência
//! qualquer (a orquestradora, 10/10; tarefa 12 da F3).
//!
//! O `evaluate` confere o texto depois do último `::` contra todas as fontes
//! juntas. «novo: Retrato::eu (de Snapshot::me)» passava porque `me)` está em
//! toda parte, e um `Tipo::membro` cujo membro só existia num comentário passava
//! igual: o guarda casava com o próprio comentário. Aqui:
//!
//! - um `nome` sozinho tem de estar **definido** em algum lugar: uma função, um
//!   tipo, uma constante, uma variante, uma classe ou uma função de JavaScript;
//! - um `Tipo::membro` tem de ter o `Tipo` (um `struct`, um `enum`, um `impl`,
//!   um módulo — o arquivo `tipo.rs` ou um `mod tipo { … }` — ou uma `class`) e
//!   o `membro` definido **nele**: um método do `impl` ou da classe, um campo do
//!   `struct`, uma variante do `enum`, ou um item do módulo.
//!
//! Comentários e o miolo das strings saem antes da leitura: um nome que só
//! aparece num `///` ou numa frase não define nada. Não é um parser: é o
//! casamento pela forma da definição, que basta para tirar a passagem pela
//! ocorrência.

use std::collections::{BTreeMap, BTreeSet};

/// O que as fontes definem.
#[derive(Debug, Default)]
pub(super) struct Definicoes {
    /// Os nomes definidos em algum lugar.
    livres: BTreeSet<String>,
    /// Os membros definidos em cada tipo, módulo ou classe, pelo nome dele.
    membros: BTreeMap<String, BTreeSet<String>>,
}

impl Definicoes {
    /// O índice de um conjunto de fontes, cada uma com o caminho dela: o nome
    /// do arquivo é o do módulo (`mundo.rs`, e `mod.rs` pela pasta).
    pub(super) fn de(fontes: &[(String, String)]) -> Self {
        let mut definicoes = Self::default();
        for (caminho, texto) in fontes {
            let js = caminho.ends_with(".js");
            let codigo = so_o_codigo(texto, js);
            if js {
                definicoes.ler_js(&codigo);
            } else {
                definicoes.ler_rust(caminho, &codigo);
            }
        }
        definicoes
    }

    /// Se o apontador resolve: `nome`, ou `Tipo::membro` (o caminho antes do
    /// tipo não conta: `crate::mods::mundo::agora` é `mundo::agora`).
    pub(super) fn resolve(&self, apontador: &str) -> bool {
        let partes: Vec<&str> = apontador.split("::").collect();
        match partes.as_slice() {
            [nome] => self.livres.contains(*nome),
            [.., tipo, membro] => self
                .membros
                .get(*tipo)
                .is_some_and(|membros| membros.contains(*membro)),
            [] => false,
        }
    }

    fn membro(&mut self, tipo: &str, membro: &str) {
        self.membros
            .entry(tipo.to_owned())
            .or_default()
            .insert(membro.to_owned());
    }

    fn ler_rust(&mut self, caminho: &str, codigo: &str) {
        let modulo = nome_do_modulo(caminho);
        for (palavra, nome, depois) in definicoes_rust(codigo) {
            self.livres.insert(nome.to_owned());
            if let Some(modulo) = &modulo {
                self.membro(modulo, nome);
            }
            match palavra {
                "struct" => {
                    if let Some(corpo) = corpo_depois(depois) {
                        for campo in campos(corpo) {
                            self.membro(nome, campo);
                        }
                    }
                }
                "enum" => {
                    if let Some(corpo) = corpo_depois(depois) {
                        for variante in variantes(corpo) {
                            self.livres.insert(variante.to_owned());
                            self.membro(nome, variante);
                        }
                    }
                }
                "mod" => {
                    if let Some(corpo) = corpo_depois(depois) {
                        for (_, item, _) in definicoes_rust(corpo) {
                            self.membro(nome, item);
                        }
                    }
                }
                _ => {}
            }
        }
        // Os `impl`: o tipo é o último antes do corpo (`impl Trait for Tipo`).
        for (indice, _) in codigo.match_indices("impl") {
            let antes = codigo[..indice].chars().next_back();
            let depois = codigo[indice + "impl".len()..].chars().next();
            if antes.is_some_and(|c| c.is_alphanumeric() || c == '_')
                || !depois.is_some_and(|c| c.is_whitespace() || c == '<')
            {
                continue;
            }
            let resto = &codigo[indice + "impl".len()..];
            let Some(chave) = resto.find('{') else {
                continue;
            };
            let Some(tipo) = tipo_do_impl(&resto[..chave]) else {
                continue;
            };
            if let Some(corpo) = corpo_depois(&resto[chave..]) {
                for (palavra, item, _) in definicoes_rust(corpo) {
                    if matches!(palavra, "fn" | "const" | "type") {
                        self.membro(&tipo, item);
                    }
                }
            }
        }
        // Os tipos que o macro `id_type!` define (`seele-proto/src/ids.rs`).
        for (indice, _) in codigo.match_indices("id_type!(") {
            if let Some(nome) = identificador(codigo[indice + "id_type!(".len()..].trim_start()) {
                self.livres.insert(nome.to_owned());
            }
        }
    }

    fn ler_js(&mut self, codigo: &str) {
        for palavra in ["function", "class", "const", "let", "var"] {
            for (indice, _) in codigo.match_indices(palavra) {
                if !fronteira(codigo, indice, palavra.len()) {
                    continue;
                }
                let depois = codigo[indice + palavra.len()..].trim_start();
                let depois = depois.strip_prefix('*').unwrap_or(depois).trim_start();
                let Some(nome) = identificador(depois) else {
                    continue;
                };
                self.livres.insert(nome.to_owned());
                if palavra == "class" {
                    if let Some(corpo) = corpo_depois(&depois[nome.len()..]) {
                        for metodo in metodos_js(corpo) {
                            self.membro(nome, metodo);
                        }
                    }
                }
            }
        }
    }
}

/// O módulo de um arquivo de Rust: o nome dele, e o da pasta para `mod.rs` e
/// `lib.rs`.
fn nome_do_modulo(caminho: &str) -> Option<String> {
    let caminho = std::path::Path::new(caminho);
    let nome = caminho.file_stem()?.to_str()?;
    if matches!(nome, "mod" | "lib" | "main") {
        return caminho.parent()?.file_name()?.to_str().map(str::to_owned);
    }
    Some(nome.to_owned())
}

/// Cada `palavra nome` de Rust que define alguma coisa, com o resto do código
/// depois do nome.
fn definicoes_rust(codigo: &str) -> Vec<(&'static str, &str, &str)> {
    let mut achadas = Vec::new();
    for palavra in [
        "fn", "struct", "enum", "type", "const", "static", "trait", "mod",
    ] {
        for (indice, _) in codigo.match_indices(palavra) {
            if !fronteira(codigo, indice, palavra.len()) {
                continue;
            }
            let depois = codigo[indice + palavra.len()..].trim_start();
            let depois = depois.strip_prefix("mut ").unwrap_or(depois);
            if let Some(nome) = identificador(depois) {
                achadas.push((palavra, nome, &depois[nome.len()..]));
            }
        }
    }
    achadas
}

/// O identificador no começo do texto.
fn identificador(texto: &str) -> Option<&str> {
    let fim = texto
        .char_indices()
        .find(|&(_, c)| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .map_or(texto.len(), |(i, _)| i);
    let nome = &texto[..fim];
    nome.chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
        .then_some(nome)
}

/// A palavra em `indice` é uma palavra, e não o meio de um identificador.
fn fronteira(codigo: &str, indice: usize, tamanho: usize) -> bool {
    let antes = codigo[..indice].chars().next_back();
    let depois = codigo[indice + tamanho..].chars().next();
    !antes.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$')
        && depois.is_some_and(char::is_whitespace)
}

/// O tipo de um cabeçalho de `impl`: o último caminho, sem os genéricos.
fn tipo_do_impl(cabecalho: &str) -> Option<String> {
    let alvo = cabecalho.rsplit(" for ").next().unwrap_or(cabecalho);
    let alvo = alvo.split(" where ").next().unwrap_or(alvo);
    let mut sem_genericos = String::new();
    let mut fundura = 0_u32;
    for c in alvo.chars() {
        match c {
            '<' => fundura += 1,
            '>' => fundura = fundura.saturating_sub(1),
            _ if fundura == 0 => sem_genericos.push(c),
            _ => {}
        }
    }
    let ultimo = sem_genericos.trim().rsplit("::").next()?.trim();
    identificador(ultimo).map(str::to_owned)
}

/// O corpo entre a primeira `{` do texto e a chave que a fecha, se a primeira
/// coisa depois do nome é o corpo (e não um `;` ou um `(`).
fn corpo_depois(texto: &str) -> Option<&str> {
    let abre = texto.find('{')?;
    if texto[..abre].contains(';') {
        return None;
    }
    let mut fundura = 0_u32;
    for (i, c) in texto[abre..].char_indices() {
        match c {
            '{' => fundura += 1,
            '}' => {
                fundura -= 1;
                if fundura == 0 {
                    return Some(&texto[abre + 1..abre + i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Os pedaços de um corpo no primeiro nível, separados por `,` ou `;`.
fn no_primeiro_nivel(corpo: &str) -> Vec<&str> {
    let mut pedacos = Vec::new();
    let (mut fundura, mut comeco) = (0_i32, 0_usize);
    for (i, c) in corpo.char_indices() {
        match c {
            '{' | '(' | '[' | '<' => fundura += 1,
            '}' | ')' | ']' | '>' => fundura -= 1,
            ',' | ';' if fundura == 0 => {
                pedacos.push(&corpo[comeco..i]);
                comeco = i + 1;
            }
            _ => {}
        }
    }
    pedacos.push(&corpo[comeco..]);
    pedacos
}

/// Sem os atributos `#[…]` e a visibilidade no começo de um item.
fn sem_prefixos(mut item: &str) -> &str {
    loop {
        item = item.trim_start();
        if let Some(resto) = item.strip_prefix("#[") {
            let mut fundura = 1_u32;
            let mut fim = resto.len();
            for (i, c) in resto.char_indices() {
                match c {
                    '[' => fundura += 1,
                    ']' => {
                        fundura -= 1;
                        if fundura == 0 {
                            fim = i + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            item = &resto[fim..];
        } else if let Some(resto) = item.strip_prefix("pub") {
            let resto = resto.trim_start();
            item = if resto.starts_with('(') {
                resto.find(')').map_or(resto, |i| &resto[i + 1..])
            } else {
                resto
            };
        } else {
            return item;
        }
    }
}

/// Os campos de um `struct`: `nome:` no primeiro nível.
fn campos(corpo: &str) -> Vec<&str> {
    no_primeiro_nivel(corpo)
        .into_iter()
        .filter_map(|pedaco| {
            let item = sem_prefixos(pedaco);
            let nome = identificador(item)?;
            item[nome.len()..]
                .trim_start()
                .starts_with(':')
                .then_some(nome)
        })
        .collect()
}

/// As variantes de um `enum`: o identificador no começo de cada item.
fn variantes(corpo: &str) -> Vec<&str> {
    no_primeiro_nivel(corpo)
        .into_iter()
        .filter_map(|pedaco| identificador(sem_prefixos(pedaco)))
        .collect()
}

/// Os métodos de uma `class` de JavaScript: `nome(` no primeiro nível do corpo,
/// depois de `async`, `static`, `get` ou `set`.
fn metodos_js(corpo: &str) -> Vec<&str> {
    let mut metodos = Vec::new();
    let mut fundura = 0_i32;
    let mut comeco_da_linha = true;
    for (i, c) in corpo.char_indices() {
        match c {
            '{' => fundura += 1,
            '}' => fundura -= 1,
            '\n' | ';' => comeco_da_linha = true,
            c if c.is_whitespace() => {}
            _ => {
                if fundura == 0 && comeco_da_linha {
                    let mut resto = &corpo[i..];
                    for prefixo in ["static ", "async ", "get ", "set "] {
                        resto = resto.strip_prefix(prefixo).unwrap_or(resto).trim_start();
                    }
                    let resto = resto.strip_prefix('#').unwrap_or(resto);
                    if let Some(nome) = identificador(resto) {
                        let depois = resto[nome.len()..].trim_start();
                        if depois.starts_with('(') || depois.starts_with('=') {
                            metodos.push(nome);
                        }
                    }
                }
                comeco_da_linha = false;
            }
        }
    }
    metodos
}

/// O código sem comentários e sem o miolo das strings: um nome que só aparece
/// num comentário, num doc ou numa frase não define nada, e uma chave dentro de
/// uma string não desequilibra um corpo.
fn so_o_codigo(texto: &str, js: bool) -> String {
    let c: Vec<char> = texto.chars().collect();
    let em = |k: usize| c.get(k).copied();
    let mut saida = String::with_capacity(texto.len());
    let mut i = 0;
    while let Some(atual) = em(i) {
        let proximo = em(i + 1);
        match atual {
            '/' if proximo == Some('/') => {
                while em(i).is_some_and(|x| x != '\n') {
                    i += 1;
                }
                continue;
            }
            '/' if proximo == Some('*') => {
                i += 2;
                while em(i).is_some() && !(em(i) == Some('*') && em(i + 1) == Some('/')) {
                    i += 1;
                }
                i += 2;
                saida.push(' ');
                continue;
            }
            'r' if !js
                && matches!(proximo, Some('"' | '#'))
                && !i
                    .checked_sub(1)
                    .and_then(em)
                    .is_some_and(|a| a.is_alphanumeric() || a == '_') =>
            {
                // r"…" ou r#"…"#, sem escape.
                let mut j = i + 1;
                let mut cerquilhas = 0;
                while em(j) == Some('#') {
                    cerquilhas += 1;
                    j += 1;
                }
                if em(j) == Some('"') {
                    j += 1;
                    while em(j).is_some() {
                        if em(j) == Some('"') && (0..cerquilhas).all(|k| em(j + 1 + k) == Some('#'))
                        {
                            j += 1 + cerquilhas;
                            break;
                        }
                        j += 1;
                    }
                    saida.push_str("\"\"");
                    i = j;
                    continue;
                }
                saida.push('r');
            }
            '"' | '`' => {
                i += 1;
                while em(i).is_some_and(|x| x != atual) {
                    if em(i) == Some('\\') {
                        i += 1;
                    }
                    i += 1;
                }
                saida.push_str("\"\"");
            }
            '\'' if js => {
                i += 1;
                while em(i).is_some_and(|x| x != '\'' && x != '\n') {
                    if em(i) == Some('\\') {
                        i += 1;
                    }
                    i += 1;
                }
                saida.push_str("''");
            }
            '\'' => {
                // Em Rust, um caractere ('{', '\n') ou um tempo de vida ('a).
                if proximo == Some('\\') {
                    i += 2;
                    while em(i).is_some_and(|x| x != '\'') {
                        i += 1;
                    }
                    saida.push_str("' '");
                } else if em(i + 2) == Some('\'') {
                    i += 2;
                    saida.push_str("' '");
                } else {
                    saida.push('\'');
                }
            }
            outro => saida.push(outro),
        }
        i += 1;
    }
    saida
}

#[cfg(test)]
mod tests {
    use super::Definicoes;

    fn indice(fontes: &[(&str, &str)]) -> Definicoes {
        Definicoes::de(
            &fontes
                .iter()
                .map(|(caminho, texto)| ((*caminho).to_owned(), (*texto).to_owned()))
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn o_rust_resolve_pela_definicao() {
        let d = indice(&[(
            "crates/x/src/server.rs",
            r#"
/// `Occupancy::sombra` só aparece aqui, num comentário.
pub struct Occupancy { pub salas: Vec<u32>, mapa: Map<u32, u32> }
impl Occupancy {
    pub fn onde_esta(&self) -> Option<u32> { None }
    // fn sombra() {}
    const LIMITE: u32 = 8;
}
pub enum ServerMessage { ModReply { request: u32 }, Session(u32), Ping }
id_type!(
    PersonId,
    u64
);
pub fn capacidades_da_api() {}
const FRASE: &str = "fn fantasma() {} { desequilibra";
"#,
        )]);
        for certo in [
            "Occupancy::onde_esta",
            "Occupancy::salas",
            "Occupancy::mapa",
            "Occupancy::LIMITE",
            "ServerMessage::ModReply",
            "ModReply",
            "PersonId",
            "capacidades_da_api",
            "server::capacidades_da_api",
        ] {
            assert!(d.resolve(certo), "`{certo}` não resolveu");
        }
        for errado in [
            "Occupancy::sombra",
            "Occupancy::nao_existe",
            "fantasma",
            "Snapshot::me",
            "nao_existe",
        ] {
            assert!(!d.resolve(errado), "`{errado}` resolveu");
        }
    }

    #[test]
    fn um_modulo_e_o_arquivo_dele() {
        let d = indice(&[
            (
                "crates/s/src/mods/mundo.rs",
                "pub fn agora() -> i64 { 0 }\n// fn sumiu() {}\n",
            ),
            ("crates/s/src/mods/volume/mod.rs", "pub fn esperar() {}\n"),
        ]);
        assert!(d.resolve("mundo::agora"));
        assert!(d.resolve("crate::mods::mundo::agora"));
        assert!(d.resolve("volume::esperar"));
        assert!(!d.resolve("mundo::sumiu"));
    }

    #[test]
    fn o_javascript_resolve_pela_definicao() {
        let d = indice(&[(
            "apps/x/ui/mods.js",
            r#"
// SuperficieDeMod::comentado() só num comentário
class SuperficieDeMod {
  constructor(id) { this.id = id; }
  atualizar(chave, arvore) { if (x) { return; } }
  async mostrar() {}
  static de(id) {}
  marcarErros = (erros) => {};
}
function projetarORetrato(snapshot) { return { eu: 1 }; }
const TETO = 128;
const texto = "class Fantasma { sumir() {} }";
"#,
        )]);
        for certo in [
            "SuperficieDeMod::atualizar",
            "SuperficieDeMod::mostrar",
            "SuperficieDeMod::de",
            "SuperficieDeMod::marcarErros",
            "projetarORetrato",
            "TETO",
        ] {
            assert!(d.resolve(certo), "`{certo}` não resolveu");
        }
        for errado in [
            "SuperficieDeMod::comentado",
            "Fantasma",
            "Fantasma::sumir",
            "SuperficieDeMod::x",
        ] {
            assert!(!d.resolve(errado), "`{errado}` resolveu");
        }
    }
}
