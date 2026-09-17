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

use seele_proto::mods::MOD_API_VERSION;

/// O catálogo que o indexador assinou, como ele chegou.
const CATALOGO: &[u8] = include_bytes!("../../../apps/seele-app/testes/catalogo-do-indexador.json");

#[test]
fn o_indexador_e_este_build_oferecem_a_mesma_api_de_mods() {
    let cru: serde_json::Value =
        serde_json::from_slice(CATALOGO).expect("o vetor do indexador é JSON");

    let oferecida = cru
        .get("api_oferecida")
        .and_then(serde_json::Value::as_u64)
        .expect(
            "o catálogo deixou de dizer que API o indexador oferece; sem esse \
             campo a divergência volta a ser invisível",
        );

    assert_eq!(
        u32::try_from(oferecida).expect("a API cabe num u32"),
        MOD_API_VERSION,
        "o indexador e este build discordam sobre a versão da API de MODs: \
         regere o catálogo lá, ou acerte a constante de um dos dois lados"
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
        let id = m.get("id").and_then(serde_json::Value::as_str).unwrap_or("?");
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
