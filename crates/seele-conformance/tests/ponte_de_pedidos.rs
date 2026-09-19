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
  if (pedido.op === 'esperar') {
    // ADR 0048: o MOD autoriza uma escrita de volume, de dentro do `aoPedir`.
    const ok = volume.esperar(pedido.token, pedido.caminho, pedido.tipos, 60);
    return JSON.stringify({ ok, contexto });
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
    pedir_em(client, request, ChannelId(1), body).await
}

/// O mesmo, dizendo em qual canal — ou em nenhum, com zero.
async fn pedir_em(
    client: &mut Client,
    request: u32,
    canal: ChannelId,
    body: serde_json::Value,
) -> Result<serde_json::Value> {
    client
        .mod_request(request, "prova/ponte".into(), canal, body.to_string())
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

/// **Um MOD de escopo de servidor lê sem haver canal aberto.**
///
/// O plano de 18/09 nomeia o defeito: «para MOD de escopo servidor, leitura não
/// deveria exigir que exista um canal de texto selecionado; hoje a interface
/// comum e a ponte associam pedidos a canal».
///
/// A ponte conferia que o canal do pedido existia, sempre. Um MOD que lê o tema
/// do servidor, a ficha de alguém ou a configuração da instância não é sobre
/// canal nenhum — e falhava com `unknown channel` enquanto a janela não tivesse
/// um aberto: um motivo que não tem nada a ver com ele, e que quem escreveu o
/// MOD não tem como consertar.
///
/// Zero passa a querer dizer **nenhum canal**. Ele nunca foi um canal de
/// verdade: `ChannelId` é a chave primária do SQLite, que começa em 1.
///
/// O que este teste prende são as três metades:
///
/// 1. com zero, o pedido é atendido;
/// 2. o MOD recebe `channel: null`, e não zero — comparar com um identificador
///    que não existe seria pior do que não receber nada;
/// 3. **um canal que não existe continua sendo recusado.** Sem isto, a mudança
///    teria trocado «exige canal sempre» por «não confere canal nunca», e um
///    pedido poderia nomear o canal de outra pessoa.
#[tokio::test(flavor = "multi_thread")]
async fn um_mod_de_escopo_de_servidor_le_sem_canal_aberto() -> Result<()> {
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

    let chave = SigningKey::from_bytes(&[202; 32]);
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

    let sem_canal = pedir_em(
        &mut anfitriao,
        1,
        ChannelId(0),
        serde_json::json!({"op":"quem"}),
    )
    .await?;
    assert_eq!(
        sem_canal["ok"],
        serde_json::Value::Bool(true),
        "um pedido de escopo de servidor foi recusado: {sem_canal}"
    );
    assert_eq!(
        sem_canal["contexto"]["channel"],
        serde_json::Value::Null,
        "o MOD recebeu um canal onde não havia canal nenhum: {sem_canal}"
    );

    // E o canal inventado continua recusado: a porta não ficou aberta.
    let inventado = pedir_em(
        &mut anfitriao,
        2,
        ChannelId(4242),
        serde_json::json!({"op":"quem"}),
    )
    .await?;
    assert_eq!(
        inventado["ok"],
        serde_json::Value::Bool(false),
        "um canal que não existe passou pela ponte: {inventado}"
    );

    serving.abort();
    std::fs::remove_dir_all(root)?;
    Ok(())
}

/// **O catálogo diz qual conjunto ele descreve.**
///
/// A janela precisa disso para uma coisa só, e ela é a regra inteira da
/// preparação de entrada: buscar de volta o que já foi aceito e sumiu do disco,
/// **e nada além disso**. Sem a identidade aqui, a janela veria quais MODs
/// faltam e não teria com o que comparar — e buscá-los assim ampliaria um
/// consentimento antigo para uma lista que ninguém leu.
///
/// A identidade é a mesma que o anúncio usa, e é contra ela que o aceite de
/// quem entra é conferido. Duas identidades diferentes para o mesmo conjunto
/// fariam a janela nunca buscar nada, calada.
#[tokio::test(flavor = "multi_thread")]
async fn o_catalogo_diz_qual_conjunto_ele_descreve() -> Result<()> {
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
                hash: hash.clone(),
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

    let chave = SigningKey::from_bytes(&[203; 32]);
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

    // Identificador vazio é o catálogo do próprio servidor.
    anfitriao
        .mod_request(1, String::new(), ChannelId(0), "{}".into())
        .await?;
    let catalogo = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if let ServerMessage::ModReply { payload, .. } = anfitriao.next_event().await? {
                return Ok::<serde_json::Value, anyhow::Error>(serde_json::from_str(&payload)?);
            }
        }
    })
    .await??;

    assert_eq!(
        catalogo["conjunto"].as_str().unwrap_or_default(),
        set,
        "o catálogo descreve um conjunto e diz que é outro: {catalogo}"
    );
    // E o hash de cada MOD continua lá: é por ele que a janela sabe **o quê**
    // buscar, e a identidade só diz se ela pode.
    assert_eq!(
        catalogo["mods"][0]["hash"].as_str().unwrap_or_default(),
        hash,
        "o catálogo deixou de dizer o conteúdo exigido: {catalogo}"
    );

    serving.abort();
    std::fs::remove_dir_all(root)?;
    Ok(())
}

/// **Uma autorização de volume sobrevive ao pedido que a emitiu.**
///
/// ADR 0048. O MOD chama `volume.esperar` de dentro do `aoPedir`, e os bytes
/// chegam depois, por um fluxo próprio. Entre as duas coisas há um abismo que
/// este teste existe para medir: `mods::pedidos` cria um `Anfitriao` por pedido
/// e o **descarta ao responder**.
///
/// Enquanto a lista de esperas morava no `Anfitriao`, ela era descartada junto.
/// O teste de unidade daquela lista passava — ele registra e consome no mesmo
/// objeto — e o produto não funcionava: o primeiro byte encontraria «não há
/// espera com este token» para um token que o MOD tinha acabado de emitir.
/// «Existir não é funcionar», e a diferença entre os dois é exatamente esta
/// função.
///
/// O que ele prende, então, é o caminho inteiro: pedido pela ponte de verdade,
/// espera registrada no servidor de verdade, e o fluxo de volume achando-a.
#[tokio::test(flavor = "multi_thread")]
async fn uma_espera_de_volume_sobrevive_ao_pedido_que_a_emitiu() -> Result<()> {
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

    let resposta = pedir(
        &mut anfitriao,
        1,
        serde_json::json!({
            "op": "esperar",
            "token": "t-da-ponte",
            "caminho": "retratos/um.png",
            "tipos": ["png"],
        }),
    )
    .await?;
    assert_eq!(
        resposta["ok"],
        serde_json::Value::Bool(true),
        "o MOD não conseguiu registrar a espera: {resposta}"
    );

    // **A prova.** O pedido já respondeu e o `Anfitriao` dele já morreu. Se a
    // lista fosse dele, aqui haveria zero.
    let quantas = daemon
        .server()
        .esperas
        .lock()
        .expect("as esperas do servidor")
        .quantas();
    assert_eq!(
        quantas, 1,
        "a espera morreu com o `Anfitriao` do pedido que a emitiu; nenhum byte \
         de volume encontraria o token"
    );

    serving.abort();
    std::fs::remove_dir_all(root)?;
    Ok(())
}

/// Um PNG mínimo, de verdade: assinatura, `IHDR` e `IEND`.
///
/// De verdade porque o servidor lê o tipo **dos bytes** — decisão 2 do ADR
/// 0048 —, e um vetor que só tivesse a assinatura provaria a conferência do
/// primeiro bloco e nada mais.
fn png_minimo() -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(&[0, 0, 0, 13]);
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0]);
    bytes.extend_from_slice(&[0x1F, 0x15, 0xC4, 0x89]);
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    bytes.extend_from_slice(b"IEND");
    bytes.extend_from_slice(&[0xAE, 0x42, 0x60, 0x82]);
    bytes
}

/// Sobe um servidor com o MOD de mentira ligado e entra nele.
///
/// Extraído porque os três testes de volume abaixo o repetiam inteiro, e um
/// arranque copiado três vezes é três lugares para divergirem.
async fn servidor_com_o_mod(
    semente: u8,
) -> Result<(
    Arc<Daemon>,
    std::path::PathBuf,
    Client,
    tokio::task::JoinHandle<()>,
)> {
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
    let serving = tokio::spawn(async move {
        let _ = service.run().await;
    });
    let chave = SigningKey::from_bytes(&[semente; 32]);
    let cliente = Client::connect(
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
    Ok((daemon, root, cliente, serving))
}

/// Registra uma espera pelo MOD e devolve o token.
async fn autorizar(cliente: &mut Client, pedido: u32, token: &str, tipos: &[&str]) -> Result<()> {
    let resposta = pedir(
        cliente,
        pedido,
        serde_json::json!({
            "op": "esperar",
            "token": token,
            "caminho": "retratos/um.png",
            "tipos": tipos,
        }),
    )
    .await?;
    anyhow::ensure!(
        resposta["ok"] == serde_json::Value::Bool(true),
        "{resposta}"
    );
    Ok(())
}

/// **Os bytes vão por um fluxo e chegam ao disco, inteiros — ADR 0048.**
///
/// O caminho inteiro numa função: o MOD autoriza pelo controle, a janela abre
/// um fluxo próprio, o servidor casa o token com a espera, confere o tipo **nos
/// bytes** e grava na pasta daquela instância.
///
/// O que ele prende, além de «funciona»: o arquivo fica **onde o MOD nomeou**,
/// dentro de `volume/`, e não onde quem enviou pediu — o cabeçalho não carrega
/// caminho, e é essa ausência que fecha a travessia por construção.
#[tokio::test(flavor = "multi_thread")]
async fn os_bytes_de_um_volume_chegam_ao_disco_pelo_fluxo() -> Result<()> {
    let _vaga = vaga::minha();
    let (_daemon, root, mut cliente, serving) = servidor_com_o_mod(205).await?;
    autorizar(&mut cliente, 1, "t-um", &["png"]).await?;

    let bytes = png_minimo();
    let enviado = cliente
        .transfers()
        .enviar_volume("prova/ponte", "t-um", &bytes, |_, _| {})
        .await?;
    assert!(
        matches!(enviado, seele_core::client::Sent::Delivered { .. }),
        "o fluxo de volume não foi entregue: {enviado:?}"
    );

    let destino = root
        .join("mod-data")
        .join("prova")
        .join("ponte")
        .join("volume")
        .join("retratos")
        .join("um.png");
    // O fluxo termina do lado de cá antes de o servidor fechar o arquivo: uma
    // espera curta, e não uma suposição sobre a ordem de duas tarefas.
    for _ in 0..50 {
        if destino.is_file() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        std::fs::read(&destino).ok(),
        Some(bytes),
        "o arquivo não chegou inteiro em {}",
        destino.display()
    );

    serving.abort();
    std::fs::remove_dir_all(root)?;
    Ok(())
}

/// **Um token que ninguém autorizou não escreve nada, e a recusa volta.**
///
/// É a propriedade que faz «autorizar» valer alguma coisa: sem ela, bastaria
/// abrir um fluxo com qualquer token inventado para escrever na pasta de um
/// MOD.
#[tokio::test(flavor = "multi_thread")]
async fn um_token_que_ninguem_autorizou_nao_escreve_nada() -> Result<()> {
    let _vaga = vaga::minha();
    let (_daemon, root, mut cliente, serving) = servidor_com_o_mod(206).await?;

    let _ = cliente
        .transfers()
        .enviar_volume("prova/ponte", "inventado", &png_minimo(), |_, _| {})
        .await?;

    let volume = root
        .join("mod-data")
        .join("prova")
        .join("ponte")
        .join("volume");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        !volume.exists(),
        "um token inventado criou {} — a autorização virou formalidade",
        volume.display()
    );

    // E a recusa chega pelo controle, com o token, para a tela saber qual
    // barra de progresso parar.
    let recusa = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let ServerMessage::VolumeRecusado { token, motivo } = cliente.next_event().await? {
                return Ok::<_, anyhow::Error>((token, motivo));
            }
        }
    })
    .await??;
    assert_eq!(recusa.0, "inventado");
    assert_eq!(recusa.1, seele_proto::volume::VolumeRefusal::SemEspera);

    serving.abort();
    std::fs::remove_dir_all(root)?;
    Ok(())
}

/// **O tipo é lido dos bytes, e não do que alguém declara.**
///
/// Decisão 2 do ADR 0048: quem confere é o servidor, porque o MOD deixou de ver
/// os bytes. O MOD autoriza só `png`; o que sobe é um GIF de verdade, com
/// assinatura de GIF. Nada fica no disco.
///
/// **E não sobra arquivo de obras.** Um fluxo recusado que deixasse o temporário
/// seria lixo com nome de token na pasta de quem hospeda, crescendo a cada
/// tentativa.
#[tokio::test(flavor = "multi_thread")]
async fn um_tipo_que_o_mod_nao_autorizou_nao_fica_no_disco() -> Result<()> {
    let _vaga = vaga::minha();
    let (_daemon, root, mut cliente, serving) = servidor_com_o_mod(207).await?;
    autorizar(&mut cliente, 1, "t-gif", &["png"]).await?;

    let mut gif = b"GIF89a".to_vec();
    gif.extend_from_slice(&[1, 0, 1, 0, 0, 0, 0, 0x3B]);
    let _ = cliente
        .transfers()
        .enviar_volume("prova/ponte", "t-gif", &gif, |_, _| {})
        .await?;

    let recusa = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let ServerMessage::VolumeRecusado { token, motivo } = cliente.next_event().await? {
                return Ok::<_, anyhow::Error>((token, motivo));
            }
        }
    })
    .await??;
    assert_eq!(recusa.0, "t-gif");
    assert_eq!(
        recusa.1,
        seele_proto::volume::VolumeRefusal::TipoRecusado,
        "um GIF passou por um MOD que só autorizou PNG"
    );

    let pasta = root
        .join("mod-data")
        .join("prova")
        .join("ponte")
        .join("volume")
        .join("retratos");
    let sobrou: Vec<String> = std::fs::read_dir(&pasta)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        sobrou.is_empty(),
        "sobrou arquivo depois de uma recusa: {sobrou:?}"
    );

    serving.abort();
    std::fs::remove_dir_all(root)?;
    Ok(())
}
