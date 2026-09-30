//! Uma versão publicada da API de MODs não se edita.
//!
//! # O que aconteceu
//!
//! O `api/README.md` diz, desde a v1, que nenhum `api/vN.json` é editado
//! depois de publicado, e nada conferia. O mapeamento de 23/09 achou a v3 e a
//! v4 editadas depois de saírem — as duas no mesmo commit, o `970cb67`, horas
//! depois de a v4 sair na v0.13.0. Dizer não é guardar, que é a mesma frase do
//! vizinho `a_api_dos_mods_nao_diverge.rs` sobre a mesma API.
//!
//! # O que este arquivo confere
//!
//! `api/congeladas.sha256` lista o SHA-256 de cada versão, e o teste de árvore
//! reprova:
//!
//! - uma versão listada cujos bytes mudaram;
//! - uma `vN.json` que não está na lista: uma versão nova entra nela no commit
//!   que a cria, e não quando alguém lembrar;
//! - uma versão listada que sumiu de `api/`;
//! - um `.json` em `api/` que não se chama `vN.json`, que seria uma API
//!   escapando deste guarda pelo nome;
//! - uma lista sem a linha de alguma versão que este build oferece: apagar o
//!   arquivo **e** a linha passaria pelos três primeiros itens sem deixar
//!   rastro.
//!
//! Cada reprovação diz o que fazer: desfazer a edição e publicar a versão
//! seguinte, ou acrescentar a linha que falta.
//!
//! # Por que aqui, e não no `check-api`
//!
//! O `check-api` roda quando alguém o chama. Este teste roda em todo
//! `cargo test`, nos três sistemas, contra a árvore de verdade — e é o
//! `cargo test --workspace` do `validar` que segura a publicação. E o SHA-256
//! já está no `seele-proto` — é o que nomeia um anexo no disco —, então o
//! guarda não traz dependência nova para ninguém.
//!
//! # O que ele não pega
//!
//! Quem edita uma versão **e** a linha dela na lista passa. Isso fica para a
//! revisão, e de propósito: uma linha de `api/congeladas.sha256` mudando num
//! diff é a frase «editei uma API publicada» escrita por extenso.
//!
//! Não levanta servidor, então não toma a vaga da pendência 29 (`tests/vaga`),
//! como o vizinho `a_api_dos_mods_nao_diverge.rs`.

#![allow(clippy::expect_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use seele_proto::attachment::{hash, hex};
use seele_proto::mods::MOD_API_VERSION;

/// A lista, relativa à raiz do repositório.
const LISTA: &str = "api/congeladas.sha256";

/// A raiz do repositório: este crate mora em `crates/seele-conformance`.
fn raiz_do_repositorio() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("a raiz do repositório")
        .to_path_buf()
}

/// O número de uma versão pelo nome do arquivo: `v5.json` → 5.
///
/// `None` para qualquer outro nome, inclusive `v05.json` e `v+5.json`: o nome
/// de uma versão tem uma grafia só, e duas grafias seriam dois arquivos para a
/// mesma versão.
fn numero(nome: &str) -> Option<u32> {
    let digitos = nome.strip_prefix('v')?.strip_suffix(".json")?;
    if digitos.is_empty()
        || digitos.starts_with('0')
        || !digitos.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    digitos.parse().ok()
}

/// Lê a lista: nome do arquivo → SHA-256 esperado, em hexadecimal minúsculo.
///
/// Linha vazia e linha começando por `#` são comentário. Qualquer outra linha
/// fora do formato é erro, e não linha pulada: uma lista que pula o que não
/// entende deixa uma versão sem guarda sem dizer.
fn ler_lista(texto: &str) -> Result<BTreeMap<String, String>, String> {
    let mut lista = BTreeMap::new();
    for (indice, linha) in texto.lines().enumerate() {
        let n = indice + 1;
        let linha = linha.trim();
        if linha.is_empty() || linha.starts_with('#') {
            continue;
        }
        let Some((esperado, nome)) = linha.split_once("  ") else {
            return Err(format!(
                "linha {n}: esperava `<sha256>  vN.json`, com dois espaços, e veio `{linha}`"
            ));
        };
        let nome = nome.trim();
        if esperado.len() != 64
            || !esperado
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        {
            return Err(format!(
                "linha {n}: `{esperado}` não é um SHA-256 em hexadecimal minúsculo"
            ));
        }
        if numero(nome).is_none() {
            return Err(format!(
                "linha {n}: `{nome}` não é o nome de uma versão (`vN.json`)"
            ));
        }
        if lista.insert(nome.to_owned(), esperado.to_owned()).is_some() {
            return Err(format!("linha {n}: `{nome}` aparece duas vezes"));
        }
    }
    Ok(lista)
}

/// O que `api/` tem hoje: nome de cada `.json` → SHA-256 dos bytes.
fn ler_api(api: &Path) -> BTreeMap<String, String> {
    let mut presentes = BTreeMap::new();
    let entradas = std::fs::read_dir(api).expect("`api/` existe na raiz do repositório");
    for entrada in entradas.flatten() {
        let caminho = entrada.path();
        if !caminho.extension().is_some_and(|e| e == "json") {
            continue;
        }
        let nome = caminho
            .file_name()
            .and_then(|n| n.to_str())
            .expect("o nome de um arquivo de `api/` é UTF-8")
            .to_owned();
        let bytes = std::fs::read(&caminho).expect("uma versão da API se lê");
        presentes.insert(nome, hex(&hash(&bytes)));
    }
    presentes
}

/// Compara `api/` com a lista, e diz o que fazer em cada desacordo.
///
/// `oferecida` é a versão que este build oferece (`MOD_API_VERSION`). Até ela,
/// uma versão está publicada: o build a entrega, e um MOD pode ter sido escrito
/// contra ela. Acima dela, a versão está em construção, e a linha dela
/// acompanha as edições até o build passar a oferecê-la.
fn conferir(
    lista: &BTreeMap<String, String>,
    presentes: &BTreeMap<String, String>,
    oferecida: u32,
) -> Vec<String> {
    // A mudança vai na versão seguinte à que este build oferece: ou ela já está
    // em construção, ou é nova. Não é a seguinte à que foi editada: editar a v3
    // não se conserta na v4, que também está publicada.
    let proxima = oferecida.saturating_add(1);
    let destino = if presentes.contains_key(&format!("v{proxima}.json")) {
        format!("ponha a mudança na `api/v{proxima}.json`, que ainda está em construção")
    } else {
        format!("ponha a mudança numa `api/v{proxima}.json` nova, somada às aceitas")
    };

    let mut violacoes = Vec::new();
    for (nome, achado) in presentes {
        let Some(n) = numero(nome) else {
            violacoes.push(format!(
                "`api/{nome}` não é uma versão. `api/` guarda um arquivo por versão, com o \
                 nome `vN.json`, e um `.json` com outro nome escaparia deste guarda: \
                 renomeie-o ou tire-o de `api/`."
            ));
            continue;
        };
        match lista.get(nome) {
            None => violacoes.push(format!(
                "`api/{nome}` não está em `{LISTA}`. Toda versão entra na lista no commit \
                 que a cria: acrescente a linha `{achado}  {nome}`."
            )),
            Some(esperado) if esperado == achado => {}
            Some(esperado) if n <= oferecida => violacoes.push(format!(
                "`api/{nome}` foi publicada e mudou: a lista diz `{esperado}`, e os bytes de \
                 hoje dão `{achado}`. Uma versão publicada não se edita — um MOD escrito \
                 contra ela quebraria sem que nada no dele mudasse. Desfaça a edição \
                 (`git checkout -- api/{nome}`) e {destino}."
            )),
            Some(esperado) => violacoes.push(format!(
                "`api/{nome}` mudou, e a lista diz `{esperado}`. Este build ainda não a \
                 oferece (`MOD_API_VERSION` = {oferecida}), então ela está em construção: \
                 troque a linha dela por `{achado}  {nome}` no mesmo commit. Ela congela de \
                 vez quando `MOD_API_VERSION` chegar a {n}."
            )),
        }
    }
    for nome in lista.keys() {
        if presentes.contains_key(nome) {
            continue;
        }
        match numero(nome) {
            // Uma versão que o build ainda não oferece nunca saiu: não há o que
            // restaurar, e o `git checkout` falharia ou traria de volta um
            // rascunho descartado.
            Some(n) if n > oferecida => violacoes.push(format!(
                "`api/{nome}` está em `{LISTA}` e não existe em `api/`. Este build ainda \
                 não a oferece (`MOD_API_VERSION` = {oferecida}): tire a linha ou crie o \
                 arquivo."
            )),
            _ => violacoes.push(format!(
                "`api/{nome}` está em `{LISTA}` e sumiu de `api/`. Uma versão publicada \
                 continua no mundo enquanto houver MOD escrito contra ela: restaure-a \
                 (`git checkout -- api/{nome}`)."
            )),
        }
    }
    violacoes
}

/// **A árvore de verdade.** Nenhuma versão publicada mudou, toda versão está
/// na lista, e toda linha da lista tem o seu arquivo.
#[test]
fn nenhuma_versao_publicada_da_api_mudou() {
    let raiz = raiz_do_repositorio();
    let texto = std::fs::read_to_string(raiz.join(LISTA)).expect(
        "`api/congeladas.sha256` não existe, e sem ela nenhuma versão publicada da API de \
         MODs está protegida contra edição",
    );
    let lista = ler_lista(&texto).unwrap_or_else(|erro| panic!("`{LISTA}`, {erro}"));

    // A lista precisa ter a linha de toda versão que este build oferece. Sem
    // isto, apagar `api/v2.json` e a linha dela juntos deixaria a lista e a
    // pasta de acordo, e o guarda passaria sem ter conferido a v2.
    for n in 1..=MOD_API_VERSION {
        let publicada = format!("v{n}.json");
        assert!(
            lista.contains_key(&publicada),
            "`{LISTA}` não tem a linha de `{publicada}`, que este build oferece \
             (`MOD_API_VERSION` = {MOD_API_VERSION}). Sem ela a versão fica sem guarda \
             nenhum. Restaure a linha (`git checkout -- {LISTA}`)."
        );
    }

    let presentes = ler_api(&raiz.join("api"));
    let violacoes = conferir(&lista, &presentes, MOD_API_VERSION);
    assert!(violacoes.is_empty(), "{}", violacoes.join("\n"));
}

/// Uma lista de duas versões, escrita como a de verdade.
fn lista_de_fixture() -> BTreeMap<String, String> {
    let texto = format!(
        "# comentário, que o leitor pula\n\n{}  v1.json\n{}  v2.json\n",
        "a".repeat(64),
        "b".repeat(64)
    );
    ler_lista(&texto).expect("a lista de fixture está no formato")
}

#[test]
fn a_lista_que_casa_com_os_bytes_passa() {
    let lista = lista_de_fixture();
    assert!(
        conferir(&lista, &lista, 2).is_empty(),
        "bytes iguais aos da lista reprovaram: o guarda ficaria vermelho sem ninguém ter editado nada"
    );
}

#[test]
fn uma_versao_publicada_editada_reprova_e_manda_publicar_a_seguinte() {
    let lista = lista_de_fixture();
    let mut presentes = lista.clone();
    presentes.insert("v1.json".to_owned(), "c".repeat(64));
    let violacoes = conferir(&lista, &presentes, 2);
    assert_eq!(
        violacoes.len(),
        1,
        "esperava uma violação, vieram {violacoes:?}"
    );
    let primeira = violacoes.first().map_or("", String::as_str);
    assert!(
        primeira.contains("api/v1.json")
            && primeira.contains("api/v3.json")
            && primeira.contains("git checkout"),
        "a violação não diz o que fazer — desfazer a v1 e pôr a mudança na v3, a seguinte à \
         mais nova: {primeira}"
    );
}

#[test]
fn uma_versao_publicada_editada_manda_a_mudanca_para_a_seguinte_em_construcao() {
    // A v3 existe e está na lista, mas o build oferece a 2: ela está em
    // construção, e é nela que cabe a mudança que a v1 não pode receber.
    let mut lista = lista_de_fixture();
    lista.insert("v3.json".to_owned(), "c".repeat(64));
    let mut presentes = lista.clone();
    presentes.insert("v1.json".to_owned(), "d".repeat(64));
    let violacoes = conferir(&lista, &presentes, 2);
    assert_eq!(
        violacoes.len(),
        1,
        "esperava uma violação, vieram {violacoes:?}"
    );
    let primeira = violacoes.first().map_or("", String::as_str);
    assert!(
        primeira.contains("api/v1.json")
            && primeira.contains("git checkout")
            && primeira.contains("api/v3.json")
            && primeira.contains("em construção")
            && !primeira.contains("api/v4.json"),
        "com a v3 em construção, a mudança da v1 vai para ela, e não para uma v4 que ninguém \
         começou: {primeira}"
    );
}

#[test]
fn uma_versao_em_construcao_editada_pede_a_linha_nova() {
    let lista = lista_de_fixture();
    let mut presentes = lista.clone();
    let novo = "c".repeat(64);
    presentes.insert("v2.json".to_owned(), novo.clone());
    // O build oferece a 1: a 2 ainda não saiu.
    let violacoes = conferir(&lista, &presentes, 1);
    let primeira = violacoes.first().map_or("", String::as_str);
    assert!(
        primeira.contains(&format!("{novo}  v2.json")) && !primeira.contains("git checkout"),
        "uma versão que o build ainda não oferece pede a linha nova, e não a reversão: {primeira}"
    );
}

#[test]
fn uma_versao_fora_da_lista_reprova_e_da_a_linha() {
    let lista = lista_de_fixture();
    let mut presentes = lista.clone();
    let hash_da_nova = "d".repeat(64);
    presentes.insert("v3.json".to_owned(), hash_da_nova.clone());
    let violacoes = conferir(&lista, &presentes, 2);
    assert_eq!(
        violacoes.len(),
        1,
        "esperava uma violação, vieram {violacoes:?}"
    );
    assert!(
        violacoes
            .first()
            .is_some_and(|v| v.contains(&format!("{hash_da_nova}  v3.json"))),
        "a violação não dá a linha que falta na lista: {violacoes:?}"
    );
}

#[test]
fn uma_versao_listada_que_sumiu_reprova() {
    let lista = lista_de_fixture();
    let mut presentes = lista.clone();
    presentes.remove("v1.json");
    let violacoes = conferir(&lista, &presentes, 2);
    assert!(
        violacoes
            .first()
            .is_some_and(|v| v.contains("api/v1.json") && v.contains("sumiu")),
        "uma versão publicada apagada passou: os MODs dela continuam no mundo: {violacoes:?}"
    );
}

#[test]
fn uma_linha_sem_arquivo_de_versao_nao_oferecida_pede_tirar_a_linha_ou_criar_o_arquivo() {
    // A v3 está na lista e não em `api/`, e o build oferece a 2: ela nunca
    // saiu, então não há o que restaurar.
    let mut lista = lista_de_fixture();
    lista.insert("v3.json".to_owned(), "c".repeat(64));
    let presentes = lista_de_fixture();
    let violacoes = conferir(&lista, &presentes, 2);
    assert_eq!(
        violacoes.len(),
        1,
        "esperava uma violação, vieram {violacoes:?}"
    );
    let primeira = violacoes.first().map_or("", String::as_str);
    assert!(
        primeira.contains("api/v3.json")
            && primeira.contains("tire a linha ou crie o arquivo")
            && !primeira.contains("git checkout"),
        "uma linha sem arquivo de uma versão que o build não oferece manda restaurar o que \
         nunca foi publicado, em vez de tirar a linha ou criar o arquivo: {primeira}"
    );
}

#[test]
fn um_json_que_nao_se_chama_vn_reprova() {
    let lista = lista_de_fixture();
    let mut presentes = lista.clone();
    presentes.insert("rascunho.json".to_owned(), "e".repeat(64));
    presentes.insert("v06.json".to_owned(), "e".repeat(64));
    let violacoes = conferir(&lista, &presentes, 2);
    assert_eq!(
        violacoes.len(),
        2,
        "um `.json` com nome que não é `vN.json` escaparia do congelamento: {violacoes:?}"
    );
}

#[test]
fn uma_lista_fora_do_formato_reprova_em_vez_de_pular_a_linha() {
    let repetida = format!("{}  v1.json\n{}  v1.json\n", "a".repeat(64), "b".repeat(64));
    for (texto, porque) in [
        (format!("{} v1.json\n", "a".repeat(64)), "um espaço só"),
        (
            format!("{}  v1.json\n", "A".repeat(64)),
            "hexadecimal maiúsculo",
        ),
        (format!("{}  v1.json\n", "a".repeat(63)), "hash curto"),
        (
            format!("{}  um.json\n", "a".repeat(64)),
            "nome que não é versão",
        ),
        (repetida, "versão repetida"),
    ] {
        assert!(
            ler_lista(&texto).is_err(),
            "a lista aceitou {porque}: uma linha assim deixaria uma versão sem guarda"
        );
    }
}
