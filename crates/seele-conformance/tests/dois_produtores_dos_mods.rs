//! Os dois produtores dos **MODs da sessão**: o desktop (`seele_ffi::mods`, que
//! lê os pacotes de uma pasta) e o navegador (`seele_web::mods`, que os lê do
//! armazém da página). Tarefa 2.6 da F4.
//!
//! As duas pontas guardam o pacote sob o hash do conteúdo e o conferem a cada
//! leitura pelas mesmas regras (`seele_sessao::mods`). Aqui os mesmos pacotes —
//! um bom, um mexido depois de guardado e um que este build recusa — dão a
//! mesma lista nas duas, campo a campo. O navegador acrescenta o que é de quem
//! hospeda, e não hospeda: `enabled` e `pinta` falsos, e a exigência na rede,
//! presa à do servidor.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use serde_json::Value;

/// Um pacote: os caminhos de dentro e os bytes.
fn pacote(versao: &str, api: u32, cliente: &[u8]) -> Vec<(String, Vec<u8>)> {
    let manifesto = format!(
        r#"{{"schema":1,"id":"ana/mesa","version":"{versao}","api":{api},"repo":"https://exemplo.org/mesa","client":"cliente/main.js","arquivos":["som/toque.wav"]}}"#
    );
    vec![
        ("mod.json".to_owned(), manifesto.into_bytes()),
        ("cliente/main.js".to_owned(), cliente.to_vec()),
        ("som/toque.wav".to_owned(), b"RIFF\0\0\0\0WAVE".to_vec()),
    ]
}

/// Guarda os pacotes como o desktop: `mod-packages/<lugar>/…`.
fn no_disco(guardados: &[seele_web::mods::Guardado]) -> std::path::PathBuf {
    let pasta = std::env::temp_dir().join(format!(
        "dois-produtores-dos-mods-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    for (lugar, arquivos) in guardados {
        for (caminho, bytes) in arquivos {
            let destino = pasta
                .join(seele_ffi::mods::PACOTES)
                .join(lugar)
                .join(caminho);
            std::fs::create_dir_all(destino.parent().unwrap()).unwrap();
            std::fs::write(destino, bytes).unwrap();
        }
    }
    pasta
}

#[test]
fn os_mesmos_pacotes_dao_a_mesma_lista_nas_duas_pontas() {
    let bom = pacote("1.0.0", 6, b"SeeleUI.pronto();");
    let lugar_do_bom = seele_ffi::mods::hash_do_conjunto(&mut bom.clone());
    // Mexido depois de guardado: o conteúdo não é o que o lugar promete.
    let mexido = pacote("2.0.0", 6, b"SeeleUI.outro();");
    let lugar_do_mexido = lugar_do_bom.replace(&lugar_do_bom[..2], "ff");
    // Uma API que este build não roda: recusado, e descrito pelo que diz ser.
    let velho = pacote("0.9.0", 2, b"antigo");
    let lugar_do_velho = seele_ffi::mods::hash_do_conjunto(&mut velho.clone());
    let guardados = vec![
        (lugar_do_bom, bom),
        (lugar_do_mexido, mexido),
        (lugar_do_velho, velho),
    ];

    let pasta = no_disco(&guardados);
    let do_desktop = serde_json::to_value(seele_ffi::mods::listar_por_conteudo(
        &pasta.to_string_lossy(),
    ))
    .unwrap();
    let _ = std::fs::remove_dir_all(&pasta);

    let mut do_navegador = seele_web::mods::na_tela(guardados);
    for linha in do_navegador.as_array_mut().unwrap() {
        let campos = linha.as_object_mut().unwrap();
        assert_eq!(campos.remove("enabled"), Some(Value::Bool(false)));
        assert_eq!(campos.remove("pinta"), Some(Value::Bool(false)));
        assert_eq!(
            campos.remove("exigencia_vale_na_rede"),
            Some(Value::Bool(seele_web::mods::EXIGENCIA_VALE_NA_REDE))
        );
    }
    // **Sem o desempate**: os dois ordenam pelo `id`, e dois pacotes do mesmo
    // MOD ficam, no desktop, na ordem em que o disco os lista (`read_dir`),
    // que não é fixa. Medido aqui na primeira vez: as mesmas linhas, com o
    // empate trocado.
    let em_ordem = |lista: &Value| {
        let mut linhas: Vec<String> = lista
            .as_array()
            .unwrap()
            .iter()
            .map(Value::to_string)
            .collect();
        linhas.sort();
        linhas
    };
    assert_eq!(em_ordem(&do_navegador), em_ordem(&do_desktop));
    // E a lista diz o que se esperava dela, para a igualdade não valer por
    // duas listas vazias.
    let recusas: Vec<Value> = do_desktop
        .as_array()
        .unwrap()
        .iter()
        .map(|linha| linha["refused"].clone())
        .collect();
    assert_eq!(recusas.len(), 3);
    assert!(recusas.contains(&Value::Null));
    assert!(recusas.contains(&Value::from("malformed-id")));
    assert!(recusas.contains(&Value::from("api-too-old")));
}

#[test]
fn a_exigencia_na_rede_do_navegador_e_a_do_servidor() {
    assert_eq!(
        seele_web::mods::EXIGENCIA_VALE_NA_REDE,
        seele_server::mods::anuncio::exigencia_vale_na_rede()
    );
}

#[test]
fn o_arquivo_servido_e_o_mesmo_nas_duas_pontas() {
    let mut arquivos = pacote("1.0.0", 6, b"SeeleUI.pronto();");
    let lugar = seele_ffi::mods::hash_do_conjunto(&mut arquivos.clone());
    let pasta = no_disco(&[(lugar.clone(), arquivos.clone())]);
    // O desktop lê o pacote da pasta; o navegador, dos bytes guardados.
    let do_desktop = seele_ffi::mods::ler_por_hash(&pasta.to_string_lossy(), &lugar).unwrap();
    let _ = std::fs::remove_dir_all(&pasta);
    let do_navegador = seele_web::mods::do_mod(&mut arquivos, "ana/mesa", &lugar).unwrap();
    assert_eq!(do_desktop, do_navegador);
    for pedido in [
        "/ana/mesa/cliente/main.js",
        "/ana/mesa/som/toque.wav",
        "/ana/mesa/mod.json",
        "/ana/mesa/../mesa/mod.json",
        "/ana/mesa/som//toque.wav",
        "/ana/outro/som/toque.wav",
    ] {
        assert_eq!(
            seele_ffi::mods::caminho_declarado(&do_desktop, pedido).is_some(),
            seele_web::mods::servir(&arquivos, &do_navegador, pedido).is_some(),
            "{pedido}: as duas pontas discordam do que se serve"
        );
    }
    assert_eq!(
        seele_web::mods::codigo(&mut arquivos, "ana/mesa", &lugar).unwrap()["fonte"],
        "SeeleUI.pronto();"
    );
}
