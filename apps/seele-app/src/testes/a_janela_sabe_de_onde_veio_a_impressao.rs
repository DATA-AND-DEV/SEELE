use super::{DeOndeVeio, FalhaDaEntrada, OrigemDaImpressao};
use seele_ffi::{ConnectFailure, ConnectionError};

#[test]
fn a_origem_e_a_da_impressao_que_se_confere() {
    // As duas regras leem as mesmas entradas, e esta prende uma à outra: a
    // origem dita à janela tem de ser a da impressão que o TLS conferiu. Se
    // a ordem de `impressao_a_conferir` mudasse e esta não, a tela acusaria
    // o link de uma impressão que veio da lista, ou o contrário.
    let do_link = Some("bbbb2222");
    let guardada = Some("aaaa1111");
    for (link, lista) in [
        (do_link, guardada),
        (do_link, None),
        (None, guardada),
        (None, None),
    ] {
        let conferida = seele_ffi::impressao_a_conferir(link, lista);
        let origem = OrigemDaImpressao::da_conferida(link, lista);
        let de_onde_veio = match origem {
            Some(OrigemDaImpressao::Link) => link,
            Some(OrigemDaImpressao::Lista) => lista,
            None => None,
        };
        assert_eq!(
            de_onde_veio.map(str::to_owned),
            conferida,
            "com o link {link:?} e a guardada {lista:?}, a origem dita à janela \
                 ({origem:?}) não é a da impressão conferida ({conferida:?}): a frase acusaria \
                 quem não prometeu nada"
        );
    }
}

#[test]
fn a_falha_leva_o_erro_a_trilha_e_a_origem_no_topo() {
    // `tela-boot.js` lê `falha.error` e `falha.trail` desde que a trilha
    // existe, e agora também de onde veio a impressão. O `flatten` põe os
    // quatro no topo; sem ele, o erro ficaria dentro de um invólucro que a
    // janela não atravessa, e toda falha viraria «falha sem nome».
    let falha = FalhaDaEntrada {
        falha: ConnectFailure {
            error: ConnectionError::InviteMismatch {
                expected: "bbbb2222".into(),
                offered: "aaaa1111".into(),
            },
            trail: Vec::new(),
        },
        de_onde: DeOndeVeio {
            origem_da_impressao: Some(OrigemDaImpressao::Lista),
            a_lista_guardou: false,
        },
        substituida: false,
    };
    let Ok(serde_json::Value::Object(mapa)) = serde_json::to_value(&falha) else {
        panic!("a falha da entrada tem de serializar para objeto");
    };
    // A marca da tentativa substituída (lote E54) só aparece quando é
    // verdade: toda outra recusa chega à janela com a forma de antes.
    assert!(
        !mapa.contains_key("substituida"),
        "uma recusa que não é de tentativa substituída chegou à janela com a marca: {mapa:?}"
    );
    for chave in ["error", "trail", "origem_da_impressao", "a_lista_guardou"] {
        assert!(
            mapa.contains_key(chave),
            "a falha da entrada não traz `{chave}` no topo, e a janela o lê de lá: {mapa:?}"
        );
    }
    assert_eq!(
        mapa.get("origem_da_impressao"),
        Some(&serde_json::Value::String("Lista".into())),
        "a origem chegou à janela com outra forma que não o nome da variante"
    );
}

#[test]
fn a_janela_le_o_que_o_rust_manda() {
    // Os nomes saem da serialização de verdade: renomear um campo ou uma
    // variante de um lado só faria a janela ler `undefined`, sem erro nenhum,
    // e dizer a frase do link para uma impressão que veio da lista.
    let ler = |nome: &str| {
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("ui")
                .join(nome),
        )
        .unwrap_or_else(|erro| panic!("ui/{nome} tem de ser legível: {erro}"))
    };
    let entrada = ler("tela-boot.js");
    let Ok(serde_json::Value::Object(mapa)) = serde_json::to_value(DeOndeVeio::default()) else {
        panic!("`DeOndeVeio` tem de serializar para objeto");
    };
    for chave in mapa.keys() {
        assert!(
            entrada.contains(chave.as_str()),
            "`tela-boot.js` não lê `{chave}`, que o `connect` manda"
        );
    }
    let Ok(serde_json::Value::String(lista)) = serde_json::to_value(OrigemDaImpressao::Lista)
    else {
        panic!("a origem tem de serializar como texto");
    };
    let comparacao = format!("=== \"{lista}\"");
    for nome in ["frases.js", "tela-sessao.js", "tela-auth.js"] {
        assert!(
            ler(nome).contains(&comparacao),
            "`{nome}` não compara a origem com `\"{lista}\"`, que é como o Rust a manda: \
                 a frase da lista nunca aparece ali"
        );
    }
}
