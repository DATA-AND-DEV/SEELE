//! **O exemplo `servidor-com-mod.rs` serve o MOD que ele instala** (tarefa 15
//! da F3).
//!
//! Medido na M3, em 08/10: o exemplo instalava o pacote por identificador
//! (`mundo/mods/<id>`), e o servidor o procura por hash
//! (`RaizesDosMods::pacote_de`). A metade de servidor recusava todo pedido com
//! «No such file or directory», e quem desenvolvia um MOD com o exemplo não via
//! nada funcionar. Este teste instala com a mesma função que o exemplo usa,
//! liga o MOD e sobe o servidor como o exemplo faz, e pede ao MOD.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório; e indexar um serde_json::Value devolve Null"
)]

use std::net::SocketAddr;

use seele_server::mods::instalar::{instalar_de_pasta, raizes_de_um_mundo};
use seele_server::permissions::Permissions;
use seele_server::persistence::mods::{enable, EnabledMod};
use seele_server::persistence::{Location, Persistence};
use seele_server::{Daemon, ServerConfig};

/// O MOD de referência do produto, que tem as duas metades.
const MOD_DE_REFERENCIA: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../apps/seele-app/testes/mod-de-referencia"
);

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn o_mod_que_o_exemplo_instala_responde_ao_pedido() {
    let mundo = tempfile::tempdir().unwrap();
    let raizes = raizes_de_um_mundo(mundo.path());
    let (manifesto, hash) =
        instalar_de_pasta(std::path::Path::new(MOD_DE_REFERENCIA), &raizes).unwrap();

    // Ligado antes de subir, como o exemplo: a subida carrega o que está
    // ligado, e um MOD ligado depois não recebe os momentos.
    let banco = Location::File(mundo.path().join("seele.db"));
    enable(
        &Persistence::open(&banco).unwrap(),
        &EnabledMod {
            id: manifesto.id.clone(),
            version: manifesto.version.clone(),
            hash: hash.clone(),
            repo: manifesto.repo.clone(),
            reach: manifesto.reach.clone(),
            server_half: manifesto.server.is_some(),
            api: manifesto.api,
            tema: false,
        },
    )
    .unwrap();

    let (_, queixas) =
        seele_server::mods::carregar_do_disco(&raizes, &[(manifesto.id.clone(), hash.clone())]);
    assert!(
        queixas.is_empty(),
        "a subida não acha o pacote que o exemplo instalou: {queixas:?}"
    );

    let servidor = Daemon::bind(ServerConfig {
        name: "Exemplo".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: banco,
        mods_dir: Some(raizes),
        ..ServerConfig::default()
    })
    .await
    .unwrap();
    let pessoa = {
        let db = servidor.server().persistence.lock().await;
        Permissions::new(&db)
            .register_or_find(&[7; 32], "quem-desenvolve")
            .unwrap()
            .id
    };
    let resposta = seele_server::mods::pedidos::executar(
        servidor.server(),
        pessoa,
        seele_proto::ids::ChannelId(0),
        &manifesto.id,
        r#"{"op":"contar"}"#,
    )
    .await;
    let lido: serde_json::Value = serde_json::from_str(&resposta).unwrap();
    assert_eq!(
        lido["vezes"], 1,
        "o MOD que o exemplo instalou não respondeu ao pedido: {resposta}"
    );
    assert_eq!(lido["canal"], serde_json::Value::Null, "{resposta}");
    servidor.shutdown();
}
