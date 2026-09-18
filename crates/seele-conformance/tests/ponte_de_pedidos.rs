//! A ponte de pedidos da API v2, provada com QUIC, QuickJS e banco de verdade.
//!
//! # Por que o MOD desta prova mora aqui dentro
//!
//! A primeira versão desta bateria carregava a **Mesa** — o primeiro MOD
//! oficial — por `include_bytes!`, e afirmava sobre ela: ficha privada, cena
//! revelada, nota do mestre. Media o MOD, e o que precisa de guarda é a
//! **ponte**: o produto não pode depender de nenhum MOD, que é a frase inteira
//! do [ADR 0045](../../../docs/adr/0045-mods-o-produto-base-tem-regras-e-um-mod-nao.md)
//! — «o produto base tem regras, e um MOD não». Enquanto a Mesa estava aqui,
//! mudar uma regra de RPG reprovava a conformidade do produto, e mover a Mesa
//! para o repositório dela quebrava a compilação do SEELE.
//!
//! O MOD abaixo é de mentira e é mínimo de propósito: ele só devolve o que a
//! ponte lhe entregou. É o que permite afirmar sobre a ponte sem afirmar sobre
//! ninguém.
//!
//! # As quatro propriedades que a ponte promete, e que estão presas aqui
//!
//! 1. **A identidade vem da sessão, não do pedido.** `contexto.person`,
//!    `admin` e `write` são escritos pelo servidor a partir da conexão
//!    autenticada. Um cliente que os escreva dentro do próprio corpo não muda
//!    coisa nenhuma — e este teste forja os três para provar.
//! 2. **A resposta é privada.** Ela volta pela conexão que pediu, e não pelo
//!    barramento de eventos. Quem está na mesma sala não a vê.
//! 3. **O estado sobrevive à reconexão.** O quintal é gravado sob a trava do
//!    banco antes de a resposta sair.
//! 4. **A resposta grande atravessa inteira.** Acima de 10 KiB ela é repartida
//!    em partes numeradas, e o que chega do outro lado é byte a byte o que
//!    saiu.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_core::{Client, MemoryPinStore};
use seele_proto::{control::ServerMessage, ids::ChannelId};
use seele_server::{
    persistence::{
        mods::{enable, EnabledMod},
        Location,
    },
    Daemon, ServerConfig,
};
use std::{net::SocketAddr, sync::Arc, time::Duration};

mod vaga;

/// O MOD de mentira: devolve o contexto que recebeu, guarda o que mandarem
/// guardar, e sabe fabricar uma resposta grande sob encomenda.
///
/// Ele não valida nada e não decide nada — é justamente o que faz dele um
/// instrumento: qualquer coisa que este teste observe veio da ponte.
const MOD_DE_MENTIRA: &str = r#"
globalThis.aoPedir = (contextoJSON, pedidoJSON) => {
  const contexto = JSON.parse(contextoJSON);
  const pedido = JSON.parse(pedidoJSON);
  if (pedido.op === 'guardar') {
    dados[pedido.chave] = pedido.valor;
    return JSON.stringify({ ok: true, contexto });
  }
  if (pedido.op === 'ler') {
    return JSON.stringify({ ok: true, valor: dados[pedido.chave] || null, contexto });
  }
  if (pedido.op === 'grande') {
    return JSON.stringify({ ok: true, enchimento: 'á🧙'.repeat(pedido.vezes), contexto });
  }
  return JSON.stringify({ ok: true, contexto });
};
"#;

/// Faz um pedido e remonta a resposta a partir das partes numeradas.
///
/// Remontar aqui é de propósito: se a ponte repartir errado, é esta função que
/// falha, e não uma asserção distante que culparia o MOD.
async fn pedir(
    client: &mut Client,
    request: u32,
    body: serde_json::Value,
) -> Result<serde_json::Value> {
    client
        .mod_request(
            request,
            "prova/ponte".into(),
            ChannelId(1),
            body.to_string(),
        )
        .await?;
    tokio::time::timeout(Duration::from_secs(15), async {
        let mut pieces = std::collections::BTreeMap::new();
        loop {
            if let ServerMessage::ModReply {
                request: got,
                part,
                total,
                payload,
            } = client.next_event().await?
            {
                if got != request {
                    continue;
                }
                pieces.insert(part, payload);
                if pieces.len() == total as usize {
                    let all: String = pieces.into_values().collect();
                    return Ok::<_, anyhow::Error>(serde_json::from_str(&all)?);
                }
            }
        }
    })
    .await?
}

/// Escreve o pacote do MOD de mentira em disco e devolve a raiz e o hash.
fn instalar(id: &str) -> Result<(std::path::PathBuf, String)> {
    let root = std::env::temp_dir().join(format!(
        "seele-ponte-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    // **Endereçado pelo conteúdo.** O pacote é montado ao lado, tem o hash
    // calculado, e só então vai para `mod-packages/<hash>/` — que é o que o
    // instalador faz, e é onde o servidor o procura.
    let obras = root.join("em-obras");
    std::fs::create_dir_all(obras.join("servidor"))?;
    let package = obras;
    let manifest = serde_json::json!({
        "schema": 1,
        "id": id,
        "version": "1.0.0",
        "api": 2,
        "repo": "https://example.invalid/prova-da-ponte",
        "reach": ["estado no servidor"],
        "server": "servidor/main.js",
    })
    .to_string();
    let mut files = vec![
        ("mod.json".to_string(), manifest.clone().into_bytes()),
        (
            "servidor/main.js".to_string(),
            MOD_DE_MENTIRA.as_bytes().to_vec(),
        ),
    ];
    for (path, bytes) in &files {
        std::fs::write(package.join(path), bytes)?;
    }
    let hash = seele_proto::mods::hex(&seele_proto::mods::content_hash(&mut files));
    let destino = root.join(seele_core::mods::PACOTES).join(&hash);
    std::fs::create_dir_all(destino.parent().expect("pai"))?;
    std::fs::rename(&package, &destino)?;
    Ok((root, hash))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_ponte_entrega_a_identidade_da_sessao_e_ignora_a_que_o_pedido_afirma() -> Result<()> {
    let _vaga = vaga::minha();
    let (root, hash) = instalar("prova/ponte")?;
    let daemon = Arc::new(
        Daemon::bind(ServerConfig {
            name: "Ponte".into(),
            listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            database: Location::Memory,
            mods_dir: Some(seele_server::RaizesDosMods {
                pacotes: root.clone(),
                dados: root.join("mod-data"),
            }),
            ..ServerConfig::default()
        })
        .await?,
    );
    let set = {
        let db = daemon.server().persistence.lock().await;
        enable(
            &db,
            &EnabledMod {
                id: "prova/ponte".into(),
                version: "1.0.0".into(),
                hash,
                repo: "https://example.invalid/prova-da-ponte".into(),
                reach: vec!["estado no servidor".into()],
                server_half: true,
            },
        )?;
        seele_server::mods::anuncio::conjunto_exigido(&db)?.identidade
    };
    let addr = daemon.local_addr()?;
    let service = daemon.clone();
    let serving = tokio::spawn(async move { service.run().await });

    let chave = SigningKey::from_bytes(&[201; 32]);
    let mut anfitriao = Client::connect(
        addr,
        "localhost",
        &addr.to_string(),
        "Anfitriao",
        &chave,
        Arc::new(MemoryPinStore::new()),
        None,
        Some(&set),
    )
    .await?;

    // Quem sou eu, segundo a ponte.
    let honesto = pedir(&mut anfitriao, 1, serde_json::json!({"op":"quem"})).await?;
    let eu = honesto["contexto"]["person"].as_str().unwrap().to_owned();
    assert!(!eu.is_empty(), "a ponte não disse quem pediu: {honesto}");
    let admin_de_verdade = honesto["contexto"]["admin"].as_bool().unwrap();

    // Agora o mesmo pedido, mentindo os três campos que a ponte escreve.
    let forjado = pedir(
        &mut anfitriao,
        2,
        serde_json::json!({
            "op": "quem",
            "person": "999999999",
            "admin": !admin_de_verdade,
            "write": true,
            "channel": 4242,
        }),
    )
    .await?;
    assert_eq!(
        forjado["contexto"]["person"].as_str().unwrap(),
        eu,
        "o pedido reescreveu a identidade que a sessão tinha: {forjado}"
    );
    assert_eq!(
        forjado["contexto"]["admin"].as_bool().unwrap(),
        admin_de_verdade,
        "o pedido se deu a permissão de administrar: {forjado}"
    );
    assert_eq!(
        forjado["contexto"]["channel"], 1,
        "o pedido escolheu outro canal por dentro do corpo: {forjado}"
    );

    serving.abort();
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_resposta_volta_so_para_quem_pediu_e_sobrevive_a_reconexao() -> Result<()> {
    let _vaga = vaga::minha();
    let (root, hash) = instalar("prova/ponte")?;
    let daemon = Arc::new(
        Daemon::bind(ServerConfig {
            name: "Ponte".into(),
            listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            database: Location::Memory,
            mods_dir: Some(seele_server::RaizesDosMods {
                pacotes: root.clone(),
                dados: root.join("mod-data"),
            }),
            ..ServerConfig::default()
        })
        .await?,
    );
    let set = {
        let db = daemon.server().persistence.lock().await;
        enable(
            &db,
            &EnabledMod {
                id: "prova/ponte".into(),
                version: "1.0.0".into(),
                hash,
                repo: "https://example.invalid/prova-da-ponte".into(),
                reach: vec!["estado no servidor".into()],
                server_half: true,
            },
        )?;
        seele_server::mods::anuncio::conjunto_exigido(&db)?.identidade
    };
    let addr = daemon.local_addr()?;
    let service = daemon.clone();
    let serving = tokio::spawn(async move { service.run().await });

    let chave_a = SigningKey::from_bytes(&[202; 32]);
    let mut quem_pede = Client::connect(
        addr,
        "localhost",
        &addr.to_string(),
        "QuemPede",
        &chave_a,
        Arc::new(MemoryPinStore::new()),
        None,
        Some(&set),
    )
    .await?;
    let chave_b = SigningKey::from_bytes(&[203; 32]);
    let mut quem_so_assiste = Client::connect(
        addr,
        "localhost",
        &addr.to_string(),
        "QuemSoAssiste",
        &chave_b,
        Arc::new(MemoryPinStore::new()),
        None,
        Some(&set),
    )
    .await?;

    // Um segredo guardado por quem pede, com um número que só ele conhece.
    let guardado = pedir(
        &mut quem_pede,
        7,
        serde_json::json!({"op":"guardar","chave":"segredo","valor":"palavra-de-quem-pede"}),
    )
    .await?;
    assert_eq!(guardado["ok"], true, "{guardado}");

    // Quem está na mesma sala não recebe a resposta de número 7. A ausência
    // precisa de prazo: o outro lado faz o próprio pedido, e quando a resposta
    // DELE chega, a do 7 já teria chegado se fosse chegar.
    let dele = pedir(&mut quem_so_assiste, 8, serde_json::json!({"op":"quem"})).await?;
    assert_eq!(dele["ok"], true, "{dele}");
    let vazou = tokio::time::timeout(Duration::from_millis(600), async {
        loop {
            if let ServerMessage::ModReply { request: 7, .. } = quem_so_assiste.next_event().await?
            {
                return Ok::<bool, anyhow::Error>(true);
            }
        }
    })
    .await;
    assert!(
        vazou.is_err(),
        "a resposta privada de outra pessoa chegou nesta conexão"
    );

    // O estado sobrevive à queda da conexão que o gravou.
    drop(quem_pede);
    let mut voltou = Client::connect(
        addr,
        "localhost",
        &addr.to_string(),
        "QuemPede",
        &chave_a,
        Arc::new(MemoryPinStore::new()),
        None,
        Some(&set),
    )
    .await?;
    let lido = pedir(
        &mut voltou,
        9,
        serde_json::json!({"op":"ler","chave":"segredo"}),
    )
    .await?;
    assert_eq!(
        lido["valor"], "palavra-de-quem-pede",
        "o quintal não sobreviveu à reconexão: {lido}"
    );

    serving.abort();
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_resposta_grande_chega_inteira_em_partes_numeradas() -> Result<()> {
    let _vaga = vaga::minha();
    let (root, hash) = instalar("prova/ponte")?;
    let daemon = Arc::new(
        Daemon::bind(ServerConfig {
            name: "Ponte".into(),
            listen: SocketAddr::from(([127, 0, 0, 1], 0)),
            database: Location::Memory,
            mods_dir: Some(seele_server::RaizesDosMods {
                pacotes: root.clone(),
                dados: root.join("mod-data"),
            }),
            ..ServerConfig::default()
        })
        .await?,
    );
    let set = {
        let db = daemon.server().persistence.lock().await;
        enable(
            &db,
            &EnabledMod {
                id: "prova/ponte".into(),
                version: "1.0.0".into(),
                hash,
                repo: "https://example.invalid/prova-da-ponte".into(),
                reach: vec!["estado no servidor".into()],
                server_half: true,
            },
        )?;
        seele_server::mods::anuncio::conjunto_exigido(&db)?.identidade
    };
    let addr = daemon.local_addr()?;
    let service = daemon.clone();
    let serving = tokio::spawn(async move { service.run().await });

    let chave = SigningKey::from_bytes(&[204; 32]);
    let mut client = Client::connect(
        addr,
        "localhost",
        &addr.to_string(),
        "Grande",
        &chave,
        Arc::new(MemoryPinStore::new()),
        None,
        Some(&set),
    )
    .await?;

    // 'á🧙' são 6 bytes em UTF-8; 6 000 repetições passam de 35 KiB, o que
    // obriga pelo menos quatro partes de 10 KiB — e cai no meio de um par
    // substituto, que é onde um corte por bytes estragaria o texto.
    let vezes = 6_000;
    let grande = pedir(
        &mut client,
        11,
        serde_json::json!({"op":"grande","vezes":vezes}),
    )
    .await?;
    let enchimento = grande["enchimento"].as_str().unwrap();
    assert_eq!(
        enchimento,
        "á🧙".repeat(vezes),
        "a remontagem não devolveu o mesmo texto"
    );
    assert!(
        enchimento.len() > 10 * 1024,
        "o enchimento coube numa parte só, e o teste não provou o corte"
    );

    serving.abort();
    std::fs::remove_dir_all(root)?;
    Ok(())
}
