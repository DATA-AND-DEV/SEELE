//! Imagens de MOD por QUIC real, incluindo autorização e substituição.
// Par cru porque seele-server não pode depender de seele-core (ADR 0002).
// Autorizações e persistência são reais; não há mock do transporte.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use ed25519_dalek::{Signer, SigningKey};
use seele_proto::control::{ClientMessage, ServerMessage};
use seele_server::persistence::Location;
use seele_server::{frame, Daemon, ServerConfig};

mod comum;

/// Quanto se espera por um quadro que tem de vir.
///
/// Generoso: o que está sob teste é se ele **vem**, e uma máquina de integração
/// contínua carregada não pode transformar isso num defeito intermitente.
const PRAZO: Duration = Duration::from_secs(5);

/// Um servidor já atendendo, no banco que se pedir.
async fn server(
    banco: Location,
    raizes: seele_server::RaizesDosMods,
    ligado: seele_server::persistence::mods::EnabledMod,
) -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: banco,
        mods_dir: Some(raizes),
        ..ServerConfig::default()
    };
    let servidor = Arc::new(Daemon::bind(config).await?);
    {
        let db = servidor.server().persistence.lock().await;
        seele_server::persistence::mods::enable(&db, &ligado)?;
    }
    let endereco = servidor.local_addr()?;
    let aceitando = Arc::clone(&servidor);
    tokio::spawn(async move {
        let _ = aceitando.run().await;
    });
    Ok((endereco, servidor))
}

/// Um par cru: a conexão e o fluxo de controle.
struct Par {
    /// Segura a conexão viva enquanto o teste corre.
    _conexao: seele_enlace::Conexao,
    envio: seele_enlace::Envio,
    recebe: seele_enlace::Recebe,
    _ponta: (seele_enlace::Ponta, seele_enlace::Conexao),
}

/// Abre uma conexão e faz o aperto de mão à mão.
///
/// `semente` é a chave: a mesma semente é a mesma pessoa, o que é o que permite
/// a um teste reconectar depois do reinício como quem hospeda, e não como um
/// estranho. A primeira conta criada num server vira a Comandante — ver
/// `permissions::seat_the_arrival` —, então a semente que abrir primeiro é a de
/// quem administra.
async fn abrir(endereco: SocketAddr, semente: u8) -> Result<Par> {
    let comum::Ligado {
        ponta,
        conexao,
        mut envio,
        mut recebe,
    } = comum::ligar(endereco).await?;
    let chave = SigningKey::from_bytes(&[semente; 32]);
    frame::write(
        &mut envio,
        &ClientMessage::Hello {
            faixa: seele_proto::FAIXA,
            client: "par-cru".into(),
            nickname: format!("pessoa{semente:03}"),
            public_key: chave.verifying_key().to_bytes().to_vec(),
            join_secret: None,
            material_e2ee: None,
            apis_de_mod: seele_proto::mods::APIS_ACEITAS.to_vec(),
        },
    )
    .await?;

    let ServerMessage::Challenge { nonce } = frame::read::<ServerMessage>(&mut recebe).await?
    else {
        anyhow::bail!("o servidor não mandou Challenge");
    };
    frame::write(
        &mut envio,
        &ClientMessage::Response {
            proof: chave
                .sign(&comum::prova_amarrada(&conexao, &nonce))
                .to_bytes()
                .to_vec(),
        },
    )
    .await?;
    let mut chegada = frame::read::<ServerMessage>(&mut recebe).await?;
    // Este par fala 12: as plataformas dos MODs vêm antes do anúncio, e aqui não
    // importam.
    if matches!(chegada, ServerMessage::PlataformasDosMods { .. }) {
        chegada = frame::read::<ServerMessage>(&mut recebe).await?;
    }
    if let ServerMessage::ModsExigidos { conjunto, .. } = chegada {
        frame::write(&mut envio, &ClientMessage::AceitarMods { conjunto }).await?;
        chegada = frame::read::<ServerMessage>(&mut recebe).await?;
    }
    anyhow::ensure!(
        matches!(chegada, ServerMessage::Session { .. }),
        "o servidor não mandou Session"
    );

    Ok(Par {
        _conexao: conexao.clone(),
        envio,
        recebe,
        _ponta: (ponta, conexao),
    })
}

/// Lê quadros até um deles servir, ou até o prazo.
async fn esperar<T>(
    par: &mut Par,
    mut serve: impl FnMut(&ServerMessage) -> Option<T>,
) -> Option<T> {
    let fim = tokio::time::Instant::now() + PRAZO;
    while tokio::time::Instant::now() < fim {
        let Ok(Ok(quadro)) = tokio::time::timeout(
            Duration::from_millis(500),
            frame::read::<ServerMessage>(&mut par.recebe),
        )
        .await
        else {
            continue;
        };
        if let Some(achado) = serve(&quadro) {
            return Some(achado);
        }
    }
    None
}

async fn pedido(par: &mut Par, n: u32, payload: serde_json::Value) -> Result<serde_json::Value> {
    frame::write(
        &mut par.envio,
        &ClientMessage::ModRequest {
            request: n,
            id: "prova/imagens".into(),
            channel: seele_proto::ids::ChannelId(0),
            payload: payload.to_string(),
        },
    )
    .await?;
    let texto = esperar(par, |m| {
        if let ServerMessage::ModReply {
            request, payload, ..
        } = m
        {
            (*request == n).then(|| payload.clone())
        } else {
            None
        }
    })
    .await
    .ok_or_else(|| anyhow::anyhow!("pedido sem resposta"))?;
    Ok(serde_json::from_str(&texto)?)
}

async fn transferir(
    conexao: &seele_enlace::Conexao,
    pedido: seele_proto::volume::PedidoDeImagem,
    bytes: &[u8],
) -> Result<Vec<u8>> {
    let (mut envia, mut recebe) = conexao.open_bi().await?;
    frame::write(&mut envia, &pedido).await?;
    for bloco in bytes.chunks(65536) {
        envia.write_all(bloco).await?;
    }
    envia.finish()?;
    let resposta: seele_proto::volume::RespostaDeImagem = frame::read(&mut recebe).await?;
    anyhow::ensure!(resposta.erro.is_none(), "{:?}", resposta.erro);
    let recebidos = recebe.read_to_end(10 * 1024 * 1024).await?;
    anyhow::ensure!(
        recebidos.len() as u64 == resposta.bytes,
        "resposta truncada"
    );
    Ok(recebidos)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dez_mib_em_fluxo_substituicao_e_duas_leituras_sem_fila_de_controle() -> Result<()> {
    use seele_proto::volume::{PedidoDeImagem, VolumeHeader};
    let temp = tempfile::tempdir()?;
    let raizes = seele_server::RaizesDosMods {
        pacotes: temp.path().join("cache"),
        dados: temp.path().join("dados"),
    };
    let manifest = serde_json::json!({"schema":1,"id":"prova/imagens","version":"1.0.0","api":seele_proto::mods::MOD_API_VERSION,"repo":"https://github.com/prova/imagens","reach":["imagens"],"client":"cliente.js","server":"servidor.js"}).to_string().into_bytes();
    // Só autorização e metadados passam pelo runtime; nenhum byte da imagem.
    let js = br#"globalThis.aoPedir=(c,p)=>{const ctx=JSON.parse(c),r=JSON.parse(p);
      if(r.op==='start'){const token=ctx.person+'-'+r.serial, nome='foto-'+token; dados.pending=nome;
        if(!volume.esperar(token,nome,['png'],60))throw Error('espera recusada');return JSON.stringify({ok:true,token});}
      if(r.op==='finish'){if(volume.tamanho(dados.pending)!==r.bytes)throw Error('incompleto');dados.foto=dados.pending;return '{"ok":true}';}
      if(r.op==='asset'){if(r.path!==dados.foto)return '{"ok":false}';return JSON.stringify({ok:true,volume:volume.servir(dados.foto)});}
      return '{"ok":false}';};"#;
    let mut files = vec![
        ("mod.json".into(), manifest.to_vec()),
        ("cliente.js".into(), b"void 0;".to_vec()),
        ("servidor.js".into(), js.to_vec()),
    ];
    let hash = seele_proto::mods::hex(&seele_proto::mods::content_hash(&mut files));
    let pacote = raizes.pacote_de(&hash);
    std::fs::create_dir_all(&pacote)?;
    for (nome, bytes) in files {
        std::fs::write(pacote.join(nome), bytes)?;
    }
    let ligado = seele_server::persistence::mods::EnabledMod {
        id: "prova/imagens".into(),
        version: "1.0.0".into(),
        hash,
        repo: "https://github.com/prova/imagens".into(),
        reach: vec!["imagens".into()],
        server_half: true,
        api: 6,
        tema: false,
        plataformas: None,
    };
    let (endereco, _daemon) = server(Location::Memory, raizes.clone(), ligado).await?;
    let mut par = abrir(endereco, 1).await?;
    let mut bytes = vec![42; 10 * 1024 * 1024];
    bytes
        .get_mut(..8)
        .unwrap()
        .copy_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut imagem_atual = String::new();
    for serial in 1..=2 {
        *bytes.get_mut(100).unwrap() = serial;
        let inicio = std::time::Instant::now();
        let autorizado = pedido(
            &mut par,
            serial as u32 * 10,
            serde_json::json!({"op":"start","serial":serial}),
        )
        .await?;
        let token = autorizado
            .get("token")
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned();
        imagem_atual = format!("foto-{token}");
        let header = VolumeHeader {
            mod_id: "prova/imagens".into(),
            token: token.clone(),
        };
        transferir(
            &par._conexao,
            PedidoDeImagem::Enviar {
                cabecalho: header.clone(),
                bytes: bytes.len() as u64,
            },
            &bytes,
        )
        .await?;
        assert_eq!(
            pedido(
                &mut par,
                serial as u32 * 10 + 1,
                serde_json::json!({"op":"finish","bytes":bytes.len()})
            )
            .await?
            .get("ok")
            .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        let envio_ms = inicio.elapsed().as_millis();
        let leitura = PedidoDeImagem::Ler {
            mod_id: "prova/imagens".into(),
            channel: seele_proto::ids::ChannelId(0),
            payload: serde_json::json!({"op":"asset","path":format!("foto-{token}")}).to_string(),
        };
        let (a, b) = tokio::try_join!(
            transferir(&par._conexao, leitura.clone(), &[]),
            transferir(&par._conexao, leitura, &[])
        )?;
        assert_eq!(a, bytes);
        assert_eq!(b, bytes);
        eprintln!(
            "10 MiB rodada {serial}: envio+commit {envio_ms} ms; com duas leituras {} ms",
            inicio.elapsed().as_millis()
        );
        // O teto não foi afrouxado no porte da 0.17. Medido em 24/10: em debug,
        // com a RustCrypto do DTLS sem otimização, 30 MiB levam ~8 s e este
        // teste reprova; o pedido de perfil otimizado para as dependências de
        // cripto foi pedido ao dono do `Cargo.toml` da raiz.
        assert!(
            inicio.elapsed() < Duration::from_secs(3),
            "a imagem sofreu demora artificial em loopback: {:?}",
            inicio.elapsed()
        );
        assert!(
            transferir(
                &par._conexao,
                PedidoDeImagem::Enviar {
                    cabecalho: header,
                    bytes: 1
                },
                &[0]
            )
            .await
            .is_err(),
            "token foi reutilizado"
        );
    }
    // Mais leitores que as quatro vagas não perdem imagens da lista.
    let leitura = PedidoDeImagem::Ler {
        mod_id: "prova/imagens".into(),
        channel: seele_proto::ids::ChannelId(0),
        payload: serde_json::json!({"op":"asset","path":imagem_atual}).to_string(),
    };
    let mut leituras = tokio::task::JoinSet::new();
    for _ in 0..8 {
        let conexao = par._conexao.clone();
        let leitura = leitura.clone();
        leituras.spawn(async move { transferir(&conexao, leitura, &[]).await });
    }
    while let Some(lida) = leituras.join_next().await {
        assert_eq!(lida??, bytes);
    }

    let outro = abrir(endereco, 2).await?;
    let autorizado = pedido(&mut par, 90, serde_json::json!({"op":"start","serial":90})).await?;
    let token = autorizado
        .get("token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    let cabecalho = VolumeHeader {
        mod_id: "prova/imagens".into(),
        token: token.clone(),
    };
    let upload = PedidoDeImagem::Enviar {
        cabecalho: cabecalho.clone(),
        bytes: 10,
    };
    assert!(
        transferir(&outro._conexao, upload.clone(), b"1234567890")
            .await
            .is_err(),
        "outra pessoa usou o token"
    );
    assert!(
        transferir(&par._conexao, upload, b"curto").await.is_err(),
        "aceitou imagem truncada"
    );
    let pasta = raizes.dados_de("prova/imagens").join("volume");
    assert!(!pasta.join(format!("foto-{token}")).exists());
    assert!(
        std::fs::read_dir(&pasta)?.all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".em-obras")),
        "fragmento abandonado no disco"
    );

    let fora = PedidoDeImagem::Ler {
        mod_id: "prova/imagens".into(),
        channel: seele_proto::ids::ChannelId(0),
        payload: r#"{"op":"asset","path":"foto-antiga"}"#.into(),
    };
    assert!(transferir(&par._conexao, fora, &[]).await.is_err());
    Ok(())
}

/// O `.mrpack` mínimo de verdade (zip «stored»): o índice na raiz, se
/// `com_indice`, e um override de `recheio` bytes.
fn mrpack(recheio: usize, com_indice: bool) -> Vec<u8> {
    let indice: &[u8] = br#"{"formatVersion":1,"game":"minecraft","versionId":"1","name":"prova","files":[],"dependencies":{"minecraft":"1.21.4","fabric-loader":"0.16.10"}}"#;
    let override_ = vec![7_u8; recheio];
    let mut entradas: Vec<(&str, &[u8])> = vec![("overrides/config/grande.bin", &override_)];
    if com_indice {
        entradas.push(("modrinth.index.json", indice));
    }
    let mut z = Vec::new();
    let mut d = Vec::new();
    for (nome, dados) in &entradas {
        let local = u32::try_from(z.len()).unwrap();
        let tam = u32::try_from(dados.len()).unwrap();
        let n = u16::try_from(nome.len()).unwrap();
        z.extend_from_slice(&[
            0x50, 0x4b, 0x03, 0x04, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ]);
        z.extend_from_slice(&tam.to_le_bytes());
        z.extend_from_slice(&tam.to_le_bytes());
        z.extend_from_slice(&n.to_le_bytes());
        z.extend_from_slice(&[0, 0]);
        z.extend_from_slice(nome.as_bytes());
        z.extend_from_slice(dados);
        d.extend_from_slice(&[
            0x50, 0x4b, 0x01, 0x02, 20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ]);
        d.extend_from_slice(&tam.to_le_bytes());
        d.extend_from_slice(&tam.to_le_bytes());
        d.extend_from_slice(&n.to_le_bytes());
        d.extend_from_slice(&[0; 12]);
        d.extend_from_slice(&local.to_le_bytes());
        d.extend_from_slice(nome.as_bytes());
    }
    let inicio = u32::try_from(z.len()).unwrap();
    let quantas = u16::try_from(entradas.len()).unwrap();
    z.extend_from_slice(&d);
    z.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0]);
    z.extend_from_slice(&quantas.to_le_bytes());
    z.extend_from_slice(&quantas.to_le_bytes());
    z.extend_from_slice(&u32::try_from(d.len()).unwrap().to_le_bytes());
    z.extend_from_slice(&inicio.to_le_bytes());
    z.extend_from_slice(&[0, 0]);
    z
}

/// Como `transferir`, mas devolve a recusa do servidor como texto.
async fn transferir_ou_recusa(
    conexao: &seele_enlace::Conexao,
    pedido: seele_proto::volume::PedidoDeImagem,
    bytes: &[u8],
) -> Result<Result<Vec<u8>, String>> {
    let (mut envia, mut recebe) = conexao.open_bi().await?;
    frame::write(&mut envia, &pedido).await?;
    // Os bytes num lado e a resposta no outro: uma recusa antes do primeiro
    // byte chega sem o servidor ler o resto, e escrever tudo antes de ler
    // esperaria crédito para sempre.
    let bytes = bytes.to_vec();
    let escrita = tokio::spawn(async move {
        for bloco in bytes.chunks(65536) {
            if envia.write_all(bloco).await.is_err() {
                break;
            }
        }
        let _ = envia.finish();
        envia
    });
    let resposta: seele_proto::volume::RespostaDeImagem =
        tokio::time::timeout(Duration::from_secs(60), frame::read(&mut recebe)).await??;
    if resposta.erro.is_some() {
        escrita.abort();
    } else {
        let _ = escrita.await;
    }
    if let Some(erro) = resposta.erro {
        return Ok(Err(erro));
    }
    Ok(Ok(recebe.read_to_end(80 * 1024 * 1024).await?))
}

/// **O `.mrpack` vai ao volume e volta, até 64 MiB** (API 8 parte 6; spec
/// §8): conferido pelos bytes (um zip com o índice na raiz), com o teto do
/// tipo que a espera aceita — a mesma espera de imagem não deixa passar 20
/// MiB, e diz o tamanho e o teto —, e lido de volta pela metade de servidor
/// (`volume.pack`: o tamanho, o `sha512` e o índice).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn o_mrpack_vai_ao_volume_e_volta_com_o_teto_do_tipo() -> Result<()> {
    use seele_proto::volume::{PedidoDeImagem, VolumeHeader};
    let temp = tempfile::tempdir()?;
    let raizes = seele_server::RaizesDosMods {
        pacotes: temp.path().join("cache"),
        dados: temp.path().join("dados"),
    };
    let manifest = serde_json::json!({"schema":1,"id":"prova/imagens","version":"1.0.0","api":seele_proto::mods::MOD_API_VERSION,"repo":"https://github.com/prova/imagens","reach":["packs"],"client":"cliente.js","server":"servidor.js","maquina":"maquina.js","plataformas":["app"],"poderes":["programa"]}).to_string().into_bytes();
    let js = br#"globalThis.aoPedir=(c,p)=>{const r=JSON.parse(p);
      if(r.op==='start'){if(!volume.esperar(r.token,'packs/'+r.token+'.mrpack',r.tipos,60))throw Error('espera recusada');return '{"ok":true}';}
      if(r.op==='pack'){return JSON.stringify(volume.pack('packs/'+r.token+'.mrpack'));}
      if(r.op==='ler'){return JSON.stringify({ok:true,volume:volume.servir('packs/'+r.token+'.mrpack')});}
      return '{"ok":false}';};"#;
    let mut files = vec![
        ("mod.json".into(), manifest.clone()),
        ("cliente.js".into(), b"void 0;".to_vec()),
        ("servidor.js".into(), js.to_vec()),
        ("maquina.js".into(), b"void 0;".to_vec()),
    ];
    let hash = seele_proto::mods::hex(&seele_proto::mods::content_hash(&mut files));
    let pacote = raizes.pacote_de(&hash);
    std::fs::create_dir_all(&pacote)?;
    for (nome, bytes) in files {
        std::fs::write(pacote.join(nome), bytes)?;
    }
    let ligado = seele_server::persistence::mods::EnabledMod {
        id: "prova/imagens".into(),
        version: "1.0.0".into(),
        hash,
        repo: "https://github.com/prova/imagens".into(),
        reach: vec!["packs".into()],
        server_half: true,
        api: seele_proto::mods::MOD_API_VERSION,
        tema: false,
        plataformas: Some(vec![seele_proto::mods::Plataforma::App]),
    };
    let (endereco, _daemon) = server(Location::Memory, raizes.clone(), ligado).await?;
    let mut par = abrir(endereco, 1).await?;
    let grande = mrpack(20 * 1024 * 1024, true);
    let enviar = |token: &str, bytes: u64| PedidoDeImagem::Enviar {
        cabecalho: VolumeHeader {
            mod_id: "prova/imagens".into(),
            token: token.into(),
        },
        bytes,
    };

    // 20 MiB de pack, numa espera de pack: vai, e volta igual.
    pedido(
        &mut par,
        1,
        serde_json::json!({"op":"start","token":"p1","tipos":["mrpack"]}),
    )
    .await?;
    transferir_ou_recusa(&par._conexao, enviar("p1", grande.len() as u64), &grande)
        .await?
        .map_err(|e| anyhow::anyhow!("o pack de 20 MiB foi recusado: {e}"))?;
    let lido = pedido(&mut par, 2, serde_json::json!({"op":"pack","token":"p1"})).await?;
    assert_eq!(lido["ok"], true, "{lido}");
    assert_eq!(lido["bytes"], grande.len() as u64);
    assert!(
        lido["indice"].as_str().unwrap().contains("fabric-loader"),
        "{lido}"
    );
    let sha512: String = {
        use sha2::Digest as _;
        sha2::Sha512::digest(&grande)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    };
    assert_eq!(lido["sha512"], sha512);
    let volta = transferir_ou_recusa(
        &par._conexao,
        PedidoDeImagem::Ler {
            mod_id: "prova/imagens".into(),
            channel: seele_proto::ids::ChannelId(0),
            payload: serde_json::json!({"op":"ler","token":"p1"}).to_string(),
        },
        &[],
    )
    .await?
    .map_err(|e| anyhow::anyhow!("a leitura do pack de 20 MiB foi recusada: {e}"))?;
    assert!(volta == grande, "o pack voltou diferente");

    // Os mesmos 20 MiB numa espera de imagem: o teto da imagem é 100 MiB
    // desde a 0.19, e o que os recusa é o tipo, lido nos primeiros bytes.
    pedido(
        &mut par,
        3,
        serde_json::json!({"op":"start","token":"p2","tipos":["png"]}),
    )
    .await?;
    let recusa = transferir_ou_recusa(&par._conexao, enviar("p2", grande.len() as u64), &grande)
        .await?
        .expect_err("um pack entrou numa espera de imagem");
    assert_eq!(recusa, "formato-desconhecido");

    // Um pack declarado acima do teto dele (64 MiB, abaixo do fio de 100) numa
    // espera só de `mrpack`: recusado antes do primeiro byte, com o tamanho e
    // o teto. (A imagem acima de 100 MiB nem chega: o fio a recusa antes.)
    pedido(
        &mut par,
        6,
        serde_json::json!({"op":"start","token":"p4","tipos":["mrpack"]}),
    )
    .await?;
    let teto = seele_proto::pack::TETO_DO_PACK;
    let recusa = transferir_ou_recusa(&par._conexao, enviar("p4", teto + 1), &[0x50, 0x4b, 0x03, 0x04])
        .await?
        .expect_err("um pack acima de 64 MiB passou");
    assert_eq!(recusa, format!("arquivo-grande-demais:{}:{teto}", teto + 1));

    // Um zip sem o índice na raiz não é pack.
    let sem = mrpack(1024, false);
    pedido(
        &mut par,
        4,
        serde_json::json!({"op":"start","token":"p3","tipos":["mrpack"]}),
    )
    .await?;
    let recusa = transferir_ou_recusa(&par._conexao, enviar("p3", sem.len() as u64), &sem)
        .await?
        .expect_err("um zip sem índice passou como pack");
    assert_eq!(recusa, "formato-desconhecido");
    let lido = pedido(&mut par, 5, serde_json::json!({"op":"pack","token":"p3"})).await?;
    assert_eq!(lido["erro"], "nao-existe", "{lido}");
    Ok(())
}
