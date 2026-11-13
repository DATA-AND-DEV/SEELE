use super::{pinta_o_servidor, ModNaTela};
use seele_ffi::mods::ModInstalado;
use std::collections::BTreeSet;

/// As chaves que um `ModNaTela` de verdade produz, e não as que parecem.
fn chaves_serializadas() -> BTreeSet<String> {
    let amostra = ModNaTela {
        instalado: ModInstalado {
            id: "autor/nome".to_owned(),
            version: "1.0.0".to_owned(),
            api: seele_ffi::mods::MOD_API_VERSION,
            hash: "0".repeat(64),
            client: Some("cliente/main.js".to_owned()),
            arquivos: vec!["som/toque.wav".to_owned()],
            repo: "https://exemplo".to_owned(),
            reach: vec!["dom".to_owned()],
            server: true,
            tema: true,
            plataformas: Vec::new(),
            poderes: Vec::new(),
            maquina: false,
            refused: None,
        },
        enabled: false,
        pinta: false,
        exigencia_vale_na_rede: true,
    };
    let Ok(serde_json::Value::Object(mapa)) = serde_json::to_value(amostra) else {
        panic!("um `ModNaTela` tem de serializar para objeto");
    };
    mapa.keys().cloned().collect()
}

/// **O defeito que este guarda existe para impedir, e que já aconteceu.**
///
/// `ModNaTela` embrulha um `ModInstalado` num campo `instalado`, e o campo
/// tem `#[serde(flatten)]` — então o invólucro **não existe** no JSON: os
/// campos saem no topo. A página lia `mod.instalado.id` em sete lugares.
/// `mod.instalado` é `undefined`, e `undefined.id` lança.
///
/// O que isso fazia na tela é a metade que dói: a contagem do catálogo é
/// escrita **antes** da lista, e o erro subia até um `console.warn`. Quem
/// usava lia «1 no catálogo» ao lado de lista nenhuma, sem uma palavra
/// sobre o motivo — o «o produto sabe e não conta» do `CLAUDE.md`, e ainda
/// por cima quebrando a gestão de MODs inteira junto.
///
/// Nenhum teste viu porque nenhum tinha um MOD instalado: com a lista
/// vazia o `some()` nunca chama o predicado, e o caminho quebrado não é
/// percorrido.
///
/// A regra é sobre a **forma** do defeito, e não sobre o nome `instalado`:
/// ler `x.<meio>.<folha>` em que `<meio>` não é chave e `<folha>` é, é
/// exatamente atravessar um invólucro que o `flatten` apagou.
/// **O `pinta` nos dois valores** (tarefa 17f.0 da F3): o pacote ligado do
/// escolhido pinta; o outro MOD ligado, o pacote desligado do mesmo MOD e
/// um servidor sem escolhido, não.
#[test]
fn so_o_pacote_ligado_do_escolhido_pinta_o_servidor() {
    assert!(
        pinta_o_servidor(true, Some("seele/estilo"), "seele/estilo"),
        "o pacote ligado do escolhido tem de pintar"
    );
    assert!(
        !pinta_o_servidor(true, Some("seele/estilo"), "seele/noite"),
        "outro MOD ligado não pinta"
    );
    assert!(
        !pinta_o_servidor(false, Some("seele/estilo"), "seele/estilo"),
        "o outro conteúdo do mesmo MOD, desligado, não pinta"
    );
    assert!(
        !pinta_o_servidor(true, None, "seele/estilo"),
        "sem escolhido, ninguém pinta"
    );
}

/// **`mods_instalados` diz os `poderes` de cada MOD**, como lista de nomes
/// no topo do objeto (o `flatten` apaga o invólucro): vazia sem a chave, e
/// `["porta"]` para quem a declara. É o que a casca lê para saber se o MOD
/// tem túnel, sem abrir o manifesto.
#[test]
fn mods_instalados_diz_os_poderes_de_cada_mod() {
    let tela = |poderes: Vec<seele_ffi::mods::Poder>| {
        let mut v = serde_json::to_value(ModNaTela {
            instalado: ModInstalado {
                id: "autor/nome".to_owned(),
                version: "1.0.0".to_owned(),
                api: seele_ffi::mods::MOD_API_VERSION,
                hash: "0".repeat(64),
                client: None,
                arquivos: Vec::new(),
                repo: String::new(),
                reach: Vec::new(),
                server: true,
                tema: false,
                plataformas: Vec::new(),
                poderes,
                maquina: false,
                refused: None,
            },
            enabled: false,
            pinta: false,
            exigencia_vale_na_rede: true,
        })
        .expect("serializa");
        v.get_mut("poderes")
            .map(serde_json::Value::take)
            .unwrap_or_default()
    };
    assert_eq!(tela(Vec::new()), serde_json::json!([]));
    assert_eq!(
        tela(vec![seele_ffi::mods::Poder::Porta]),
        serde_json::json!(["porta"])
    );
}

#[test]
fn a_pagina_nao_atravessa_um_involucro_que_o_flatten_apagou() {
    let chaves = chaves_serializadas();
    assert!(
        !chaves.contains("instalado"),
        "o `flatten` saiu do `ModNaTela`; este guarda foi escrito para quando ele estava lá"
    );

    let script = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/camada-mods.js"),
    )
    .unwrap_or_else(|erro| panic!("a camada de MODs tem de ser legível: {erro}"));

    let mut culpados = Vec::new();
    for palavra in script.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.')) {
        let partes: Vec<&str> = palavra.split('.').collect();
        for janela in partes.windows(3) {
            let [inicio, meio, folha] = janela else {
                continue;
            };
            if !chaves.contains(*meio) && chaves.contains(*folha) {
                culpados.push(format!("{inicio}.{meio}.{folha}"));
            }
        }
    }

    assert!(
        culpados.is_empty(),
        "a página lê {culpados:?}, e o meio dessas cadeias não é campo nenhum do que o \
             Rust serializa — quase sempre um invólucro que o `flatten` apagou. As chaves \
             que existem são {chaves:?}"
    );
}
