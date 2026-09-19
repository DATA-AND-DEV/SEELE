//! As duas constantes que precisam concordar entre dois repositórios.
//!
//! # O que aconteceu
//!
//! `seele_proto::mods::MOD_API_VERSION` foi a 2 com a ponte de pedidos. O
//! `VERSAO_DA_API` do `SEELE-MODS-INDEXER` ficou em 1. Nada ligava os dois, e
//! nenhum dos lados tinha teste sobre a própria constante — o cabeçalho do
//! `manifesto.py` de lá dizia, em texto, que elas «não podem divergir».
//!
//! A divergência só apareceu no primeiro MOD de verdade: o MESA declarava
//! `api: 2`, que é o que este build oferece, e o indexador o recusou com
//! `api-too-new`. Dizer não é guardar.
//!
//! # Por que aqui, e por que sobre o catálogo
//!
//! O catálogo assinado é o único arquivo que atravessa os dois repositórios, e
//! ele já é copiado para `apps/seele-app/testes/` pelo runbook de publicação.
//! O guarda não custa passo novo: ele lê o mesmo vetor que já estava lá.
//!
//! Não mora na casca do app porque ela não depende do `seele-proto` (ADR 0039)
//! e copia as constantes de que precisa. Copiar esta seria a terceira cópia do
//! número que acabou de divergir — e a terceira cópia diverge igual.
//!
//! O sentido também importa: o teste é sobre `api_oferecida`, que é o teto do
//! indexador, e não sobre a API de um MOD publicado. Um MOD que pede menos do
//! que este build oferece é o caso normal de todo MOD antigo.

#![allow(clippy::expect_used)]

use seele_proto::mods::MOD_API_VERSION;

/// O catálogo que o indexador assinou, como ele chegou.
///
/// **Histórico e intocado.** Ele é o que está publicado, e é isso que ele prova:
/// que este cliente entende o formato, e que nenhum MOD lá pede mais API do que
/// este build oferece. Regerá-lo exige a chave de produção, e amarrar a versão
/// em desenvolvimento a ele faria a única prova da versão nova ser um catálogo
/// que só existe depois de ela ser publicada — a ordem ao contrário.
const CATALOGO: &[u8] = include_bytes!("../../../apps/seele-app/testes/catalogo-do-indexador.json");

/// Que API o **código** do indexador oferece hoje.
///
/// Uma cópia de `VERSAO_DA_API`, e não do catálogo publicado. Os dois números
/// respondem perguntas diferentes: este diz o que o gerador aceita agora, e o
/// do catálogo diz o que foi gerado da última vez. Amarrar a constante deste
/// build ao segundo impediria qualquer versão nova de existir antes de ser
/// publicada.
const API_DO_INDEXADOR: &[u8] =
    include_bytes!("../../../apps/seele-app/testes/api-do-indexador.json");

#[test]
fn o_indexador_e_este_build_oferecem_a_mesma_api_de_mods() {
    let cru: serde_json::Value =
        serde_json::from_slice(CATALOGO).expect("o vetor do indexador é JSON");

    let publicada = cru
        .get("api_oferecida")
        .and_then(serde_json::Value::as_u64)
        .expect(
            "o catálogo deixou de dizer que API o indexador oferece; sem esse \
             campo a divergência volta a ser invisível",
        );

    let indexador: serde_json::Value =
        serde_json::from_slice(API_DO_INDEXADOR).expect("o vetor da API do indexador é JSON");
    let oferecida = indexador
        .get("api_oferecida")
        .and_then(serde_json::Value::as_u64)
        .expect("o vetor da API do indexador não diz `api_oferecida`");

    // **O que o gerador aceita hoje é o que este build oferece.** É a
    // divergência que este arquivo existe para pegar: as duas suítes verdes
    // enquanto os dois lados discordam.
    assert_eq!(
        u32::try_from(oferecida).expect("a API cabe num u32"),
        MOD_API_VERSION,
        "o indexador e este build discordam sobre a versão da API de MODs: \
         acerte `VERSAO_DA_API` lá ou `MOD_API_VERSION` aqui, e copie \
         `api-do-indexador.json` de novo"
    );

    // **E o catálogo publicado nunca vai à frente.** Ele pode ficar atrás — é o
    // estado normal entre implementar uma versão e publicá-la —, mas um
    // catálogo oferecendo mais do que este build entende é um cliente sendo
    // oferecido MODs que ele não roda.
    assert!(
        u32::try_from(publicada).expect("a API cabe num u32") <= MOD_API_VERSION,
        "o catálogo publicado oferece API {publicada} e este build entende \
         {MOD_API_VERSION}: um cliente destes seria oferecido MODs que ele não roda"
    );
}

#[test]
fn nenhum_mod_publicado_pede_mais_api_do_que_este_build_oferece() {
    // A outra ponta, e a que de fato machucaria quem instalou: o de cima
    // compara tetos, este olha o que já está publicado. Um MOD no catálogo
    // pedindo API 3 seria oferecido a gente que não consegue rodá-lo.
    let cru: serde_json::Value =
        serde_json::from_slice(CATALOGO).expect("o vetor do indexador é JSON");

    let mods = cru
        .get("mods")
        .and_then(serde_json::Value::as_array)
        .expect("o catálogo tem uma lista de MODs, ainda que vazia");

    for m in mods {
        let id = m
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("?");
        let versoes = m
            .get("versoes")
            .and_then(serde_json::Value::as_array)
            .unwrap_or_else(|| panic!("{id} está no catálogo sem versão nenhuma"));

        for v in versoes {
            let numero = v
                .get("versao")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("?");
            let api = v
                .get("api")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or_else(|| panic!("{id} {numero} não declara API"));

            assert!(
                u32::try_from(api).expect("a API cabe num u32") <= MOD_API_VERSION,
                "{id} {numero} pede API {api}, e este build oferece {MOD_API_VERSION}"
            );
        }
    }
}

/// **Nenhum manifesto de teste crava a versão da API.**
///
/// Três vezes a mesma coisa, em três lugares diferentes: um `"api": 2` escrito
/// à mão dentro de um manifesto de fixture. No dia em que a constante sobe, o
/// manifesto deixa de ser aceito — e cada um reprova com a frase do **seu**
/// teste, não com a do defeito:
///
/// - `ponte_de_pedidos` disse «bridge-refused», que fala da ponte;
/// - `despacho` disse «o MOD continuou mudo aos eventos», que fala do
///   despachante;
/// - e o terceiro seria encontrado na subida seguinte, do mesmo jeito.
///
/// Nenhuma das três frases menciona a versão, que é a causa. Este guarda
/// procura a forma — um manifesto de MOD com a API escrita — em vez de esperar
/// o próximo.
///
/// Uma **entrada de catálogo** não é um manifesto: ela lista versões
/// publicadas, e uma delas ser antiga é o caso normal. A diferença na forma é
/// que um manifesto traz `"schema"`, e a entrada de catálogo traz `"versao"`.
#[test]
fn nenhum_manifesto_de_teste_crava_a_versao_da_api() {
    let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("a raiz do repositório");

    let mut cravados = Vec::new();
    let mut vistos = 0_usize;
    percorrer(&raiz.join("crates"), &mut |caminho, fonte| {
        vistos += 1;
        procurar(caminho, fonte, &mut cravados);
    });
    percorrer(&raiz.join("apps"), &mut |caminho, fonte| {
        vistos += 1;
        procurar(caminho, fonte, &mut cravados);
    });

    assert!(vistos > 50, "a varredura não achou fonte nenhuma: {vistos}");
    assert!(
        cravados.is_empty(),
        "estes manifestos de teste escrevem a versão da API em vez de lê-la de \
         `MOD_API_VERSION`, e vão reprovar na próxima subida com uma frase que \
         fala de outra coisa: {cravados:?}"
    );
}

/// Chama `visitar` para cada `.rs` sob um diretório.
fn percorrer(pasta: &std::path::Path, visitar: &mut impl FnMut(&std::path::Path, &str)) {
    let Ok(entradas) = std::fs::read_dir(pasta) else {
        return;
    };
    for entrada in entradas.flatten() {
        let caminho = entrada.path();
        if caminho.is_dir() {
            if caminho.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            percorrer(&caminho, visitar);
        } else if caminho.extension().is_some_and(|e| e == "rs") {
            if let Ok(fonte) = std::fs::read_to_string(&caminho) {
                visitar(&caminho, &fonte);
            }
        }
    }
}

/// Anota cada `"api": <dígito>` que esteja num manifesto de MOD.
fn procurar(caminho: &std::path::Path, fonte: &str, achados: &mut Vec<String>) {
    for (n, linha) in fonte.lines().enumerate() {
        let Some(depois) = linha.split("\"api\"").nth(1) else {
            continue;
        };
        let Some(valor) = depois.trim_start().strip_prefix(':') else {
            continue;
        };
        if !valor.trim_start().starts_with(|c: char| c.is_ascii_digit()) {
            continue;
        }
        // Um manifesto traz `"schema"`; uma entrada de catálogo traz `"versao"`.
        // A janela é de dez linhas para cima, que é onde o começo do literal
        // está em todos os casos que existem hoje.
        let comeco = n.saturating_sub(10);
        let perto: String = fonte.lines().skip(comeco).take(n - comeco + 1).collect();
        if perto.contains("\"schema\"") {
            achados.push(format!("{}:{}", caminho.display(), n + 1));
        }
    }
}
