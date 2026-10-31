//! **Um MOD ligado num servidor que subiu sem MOD nenhum passa a valer.**
//!
//! É o caminho normal do app: HOSPEDAR AQUI, e depois ligar o MOD em
//! CONFIGURAÇÕES · MODs. O `Daemon` só montava o despachante de eventos quando
//! já subia com algum MOD ligado, então nesse caminho ele não existia: as
//! permissões que o MOD declara nunca eram gravadas (o CRIAR do Multiplayer
//! desligado para a própria anfitriã) e nenhum momento chegava a ele. Medido no
//! app de verdade (e2e da API 8, 31/10); os outros testes ligam o MOD depois do
//! `bind` e gravam as permissões à mão, e por isso não viam.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;

use seele_server::persistence::Location;
use seele_server::{Daemon, RaizesDosMods, ServerConfig};

#[tokio::test(flavor = "multi_thread")]
async fn um_mod_ligado_depois_de_subir_ganha_as_permissoes_que_declara() {
    let temp = tempfile::tempdir().expect("a pasta");
    let raizes = RaizesDosMods {
        pacotes: temp.path().join("pacotes"),
        dados: temp.path().join("dados"),
    };
    let api = seele_proto::mods::MOD_API_VERSION;
    let manifesto = format!(
        r#"{{"schema":1,"id":"prova/tardio","version":"1.0.0","api":{api},
        "repo":"https://example.invalid/t","reach":["estado no servidor"],
        "server":"servidor.js",
        "permissoes":[{{"acao":"jogar","nome":"Jogar","todos":true}}]}}"#
    );
    let mut arquivos = vec![
        ("mod.json".to_owned(), manifesto.into_bytes()),
        (
            "servidor.js".to_owned(),
            b"globalThis.aoAcontecer = () => {};".to_vec(),
        ),
    ];
    let hash = seele_proto::mods::hex(&seele_proto::mods::content_hash(&mut arquivos));
    let pacote = raizes.pacote_de(&hash);
    std::fs::create_dir_all(&pacote).expect("o pacote");
    for (nome, bytes) in arquivos {
        std::fs::write(pacote.join(nome), bytes).expect("um arquivo");
    }

    // O servidor sobe com a pasta dos MODs e **nenhum** MOD ligado.
    let daemon = Arc::new(
        Daemon::bind(ServerConfig {
            name: "Casa".into(),
            listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            database: Location::Memory,
            mods_dir: Some(raizes.clone()),
            ..ServerConfig::default()
        })
        .await
        .expect("o servidor"),
    );
    let pessoa = {
        let db = daemon.server().persistence.lock().await;
        seele_server::permissions::Permissions::new(&db)
            .register_or_find(&[5; 32], "membra")
            .expect("a pessoa")
            .id
    };

    // E o MOD é ligado depois, pelo mesmo caminho do SALVAR da tela.
    {
        let db = daemon.server().persistence.lock().await;
        seele_server::persistence::mods::definir_conjunto(
            &db,
            &[seele_server::persistence::mods::EnabledMod {
                id: "prova/tardio".into(),
                version: "1.0.0".into(),
                hash,
                repo: "https://example.invalid/t".into(),
                reach: vec![],
                server_half: true,
                api,
                tema: false,
                plataformas: None,
            }],
        )
        .expect("ligar");
    }

    let mut acoes = Vec::new();
    for _ in 0..200 {
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        let db = daemon.server().persistence.lock().await;
        acoes = seele_server::cargos::acoes_do_mod(&db, pessoa, "prova/tardio");
        if !acoes.is_empty() {
            break;
        }
    }
    assert_eq!(
        acoes,
        vec!["jogar".to_owned()],
        "o MOD ligado depois de o servidor subir sem MOD nenhum ficou sem as permissões que \
         declara: o despachante de eventos não existia"
    );
}
