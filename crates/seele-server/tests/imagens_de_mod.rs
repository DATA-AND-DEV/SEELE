//! Imagens de MOD por QUIC real, incluindo autorização e substituição.
// Par cru porque seele-server não pode depender de seele-core (ADR 0002).
// Autorizações e persistência são reais; não há mock do transporte.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
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

/// Quanto se espera por um quadro que tem de vir.
///
/// Generoso: o que está sob teste é se ele **vem**, e uma máquina de integração
/// contínua carregada não pode transformar isso num defeito intermitente.
const PRAZO: Duration = Duration::from_secs(5);

/// Um verificador que aceita qualquer certificado.
///
/// O que está sob teste não é o TOFU — que tem os testes dele em
/// `seele-core/src/tofu.rs`. O certificado aqui é o que este servidor acabou de
/// gerar dentro desta função.
#[derive(Debug)]
struct AceitaQualquer(Arc<rustls::crypto::CryptoProvider>);

impl rustls::client::danger::ServerCertVerifier for AceitaQualquer {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

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
    _conexao: quinn::Connection,
    envio: quinn::SendStream,
    recebe: quinn::RecvStream,
    _ponta: quinn::Endpoint,
}

/// Abre uma conexão e faz o aperto de mão à mão.
///
/// `semente` é a chave: a mesma semente é a mesma pessoa, o que é o que permite
/// a um teste reconectar depois do reinício como quem hospeda, e não como um
/// estranho. A primeira conta criada num server vira a Comandante — ver
/// `permissions::seat_the_arrival` —, então a semente que abrir primeiro é a de
/// quem administra.
async fn abrir(endereco: SocketAddr, semente: u8) -> Result<Par> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let provider = Arc::new(rustls::crypto::ring::default_provider());

    let mut tls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AceitaQualquer(provider)))
        .with_no_client_auth();
    tls.alpn_protocols = vec![seele_proto::transport::ALPN.to_vec()];

    let config = quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls).expect("config do QUIC"),
    ));

    let mut ponta = quinn::Endpoint::client(SocketAddr::from(([127, 0, 0, 1], 0)))?;
    ponta.set_default_client_config(config);
    let conexao = ponta.connect(endereco, "localhost")?.await?;

    let (mut envio, mut recebe) = conexao.open_bi().await?;
    let chave = SigningKey::from_bytes(&[semente; 32]);
    frame::write(
        &mut envio,
        &ClientMessage::Hello {
            version: seele_proto::PROTOCOL_VERSION,
            client: "par-cru".into(),
            nickname: format!("pessoa{semente:03}"),
            public_key: chave.verifying_key().to_bytes().to_vec(),
            join_secret: None,
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
            proof: chave.sign(&nonce).to_bytes().to_vec(),
        },
    )
    .await?;
    let mut chegada = frame::read::<ServerMessage>(&mut recebe).await?;
    if let ServerMessage::ModsExigidos { conjunto, .. } = chegada {
        frame::write(&mut envio, &ClientMessage::AceitarMods { conjunto }).await?;
        chegada = frame::read::<ServerMessage>(&mut recebe).await?;
    }
    anyhow::ensure!(
        matches!(chegada, ServerMessage::Session { .. }),
        "o servidor não mandou Session"
    );

    Ok(Par {
        _conexao: conexao,
        envio,
        recebe,
        _ponta: ponta,
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
    conexao: &quinn::Connection,
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
        assert!(
            inicio.elapsed() < Duration::from_secs(3),
            "a imagem sofreu demora artificial em loopback"
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
