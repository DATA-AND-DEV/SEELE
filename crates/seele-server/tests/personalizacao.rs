//! Quem hospeda dá nome e cara ao Dogma, e todo mundo fica sabendo na hora.
//!
//! Pedido de quem paga a conta, nas palavras dele: «o host do servidor pode
//! personalizar algumas coisas, como: ícone do servidor (que fica à esquerda) e
//! nome do servidor». O ADR 0032 já dizia onde o nome mora — a tabela
//! `configuracao` — e por que ele precisa de um aviso quando muda com o Dogma
//! no ar: sem isso, a tela de quem renomeou mostra o nome novo e a de todo
//! mundo mostra o velho.
//!
//! O que esta suíte cobre é a metade que só existe **atravessada**: a permissão
//! conferida no instante do verbo, o aviso saindo para quem não pediu nada, e o
//! nome sobrevivendo a um reinício de verdade — banco fechado, processo do
//! servidor derrubado, banco reaberto.
//!
//! O **formato** e o **teto** do ícone não estão aqui, e a ausência é
//! deliberada: eles são conferidos por `seele_proto::control::decode`, que é a
//! mesma função que o `frame::read` deste servidor chama, e que já os cobra nas
//! duas direções com quadro montado à mão em `seele-proto`. Repetir aquilo aqui
//! custaria uma dependência de `postcard` neste crate para reafirmar o que já
//! está afirmado uma vez.
//!
//! # Por que um par cru, e não um `Client`
//!
//! Pelo primeiro dos dois motivos que `par_lento.rs` dá: `seele-core` não pode
//! ser dependência daqui (ADR 0002). O aperto de mão à mão é a cópia do que
//! está lá, encurtada — nem janela de fluxo escolhida, nem par que para de ler,
//! porque aqui nada disso é o assunto.

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
use seele_proto::control::{AlertReason, ClientMessage, ServerMessage, MAX_DOGMA_ICON_SIDE};
use seele_server::persistence::Location;
use seele_server::{frame, DogmaConfig, Server};

/// Quanto se espera por um quadro que tem de vir.
///
/// Generoso: o que está sob teste é se ele **vem**, e uma máquina de integração
/// contínua carregada não pode transformar isso num defeito intermitente.
const PRAZO: Duration = Duration::from_secs(5);

/// Um verificador que aceita qualquer certificado.
///
/// O que está sob teste não é o TOFU — que tem os testes dele em
/// `seele-core/src/tofu.rs`. O certificado aqui é o que este Dogma acabou de
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

/// Um Dogma já atendendo, no banco que se pedir.
async fn dogma(banco: Location) -> Result<(SocketAddr, Arc<Server>)> {
    let config = DogmaConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: banco,
        ..DogmaConfig::default()
    };
    let servidor = Arc::new(Server::bind(config).await?);
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
    /// O nome que o Dogma disse ter no aperto de mão.
    nome_do_dogma: String,
    /// O ícone que veio logo depois do `Session`, se veio algum.
    icone: Option<Vec<u8>>,
}

/// Abre uma conexão e faz o aperto de mão à mão.
///
/// `semente` é a chave: a mesma semente é a mesma pessoa, o que é o que permite
/// a um teste reconectar depois do reinício como quem hospeda, e não como um
/// estranho. A primeira conta criada num Dogma vira a Comandante — ver
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
    // A ponta é dona do socket; sem isto ela cai no fim desta função e leva a
    // conexão junto.
    std::mem::forget(ponta);

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
        anyhow::bail!("o Dogma não mandou Challenge");
    };
    frame::write(
        &mut envio,
        &ClientMessage::Response {
            proof: chave.sign(&nonce).to_bytes().to_vec(),
        },
    )
    .await?;
    let ServerMessage::Session {
        dogma: nome_do_dogma,
        ..
    } = frame::read::<ServerMessage>(&mut recebe).await?
    else {
        anyhow::bail!("o Dogma não mandou Session");
    };

    // O ícone vem logo depois, **num quadro próprio**, e só quando existe. Um
    // Dogma sem ícone não manda nada, e é por isso que esta espera é curta: o
    // silêncio é a resposta, não um tempo esgotado.
    let icone = match tokio::time::timeout(
        Duration::from_millis(500),
        frame::read::<ServerMessage>(&mut recebe),
    )
    .await
    {
        Ok(Ok(ServerMessage::DogmaIconChanged { icon })) => icon,
        _ => None,
    };

    Ok(Par {
        _conexao: conexao,
        envio,
        recebe,
        nome_do_dogma,
        icone,
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

/// Um PNG de verdade o bastante para a conferência do protocolo: assinatura,
/// `IHDR` e um lado que cabe.
fn png(lado: u32) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(&13_u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&lado.to_be_bytes());
    bytes.extend_from_slice(&lado.to_be_bytes());
    bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
    bytes
}

#[tokio::test(flavor = "multi_thread")]
async fn renomear_com_o_dogma_no_ar_chega_a_quem_nao_pediu_nada() -> Result<()> {
    // O defeito que o ADR 0032 manda não construir: o nome novo na tela de quem
    // renomeou e o velho na de todo mundo, até alguém reconectar.
    let (endereco, servidor) = dogma(Location::Memory).await?;

    let mut anfitria = abrir(endereco, 1).await?;
    let mut visita = abrir(endereco, 2).await?;
    assert_eq!(anfitria.nome_do_dogma, "Casa");
    assert_eq!(visita.nome_do_dogma, "Casa");

    frame::write(
        &mut anfitria.envio,
        &ClientMessage::RenameDogma {
            name: "Terceira Tóquio".into(),
        },
    )
    .await?;

    let na_visita = esperar(&mut visita, |quadro| match quadro {
        ServerMessage::DogmaRenamed { name } => Some(name.clone()),
        _ => None,
    })
    .await;
    assert_eq!(
        na_visita.as_deref(),
        Some("Terceira Tóquio"),
        "quem não pediu nada continuou lendo o nome velho"
    );

    // E de volta para quem pediu, que também tem um cabeçalho para redesenhar.
    let em_casa = esperar(&mut anfitria, |quadro| match quadro {
        ServerMessage::DogmaRenamed { name } => Some(name.clone()),
        _ => None,
    })
    .await;
    assert_eq!(em_casa.as_deref(), Some("Terceira Tóquio"));

    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn o_icone_chega_a_quem_esta_dentro_e_a_quem_entra_depois() -> Result<()> {
    let (endereco, servidor) = dogma(Location::Memory).await?;

    let mut anfitria = abrir(endereco, 1).await?;
    let mut visita = abrir(endereco, 2).await?;
    assert_eq!(visita.icone, None, "um Dogma novo não tem ícone");

    let imagem = png(128);
    frame::write(
        &mut anfitria.envio,
        &ClientMessage::SetDogmaIcon {
            icon: Some(imagem.clone()),
        },
    )
    .await?;

    let recebido = esperar(&mut visita, |quadro| match quadro {
        ServerMessage::DogmaIconChanged { icon } => Some(icon.clone()),
        _ => None,
    })
    .await;
    assert_eq!(recebido, Some(Some(imagem.clone())));

    // E quem chega depois recebe a mesma imagem logo atrás do `Session`, sem ter
    // de perguntar.
    let tarde = abrir(endereco, 3).await?;
    assert_eq!(tarde.icone, Some(imagem));

    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn tirar_o_icone_e_dito_a_quem_esta_dentro() -> Result<()> {
    // A metade que um `Option` esquecido deixa passar: pôr funciona e tirar não
    // chega a ninguém, então a imagem some do banco e continua na tela.
    let (endereco, servidor) = dogma(Location::Memory).await?;

    let mut anfitria = abrir(endereco, 1).await?;
    let mut visita = abrir(endereco, 2).await?;

    frame::write(
        &mut anfitria.envio,
        &ClientMessage::SetDogmaIcon {
            icon: Some(png(64)),
        },
    )
    .await?;
    esperar(&mut visita, |quadro| {
        matches!(quadro, ServerMessage::DogmaIconChanged { icon: Some(_) }).then_some(())
    })
    .await
    .expect("o ícone não chegou");

    frame::write(
        &mut anfitria.envio,
        &ClientMessage::SetDogmaIcon { icon: None },
    )
    .await?;
    esperar(&mut visita, |quadro| {
        matches!(quadro, ServerMessage::DogmaIconChanged { icon: None }).then_some(())
    })
    .await
    .expect("tirar o ícone não chegou a quem estava dentro");

    let tarde = abrir(endereco, 3).await?;
    assert_eq!(
        tarde.icone, None,
        "quem entrou depois ainda recebeu a imagem"
    );

    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn quem_nao_administra_e_recusado_e_nada_muda() -> Result<()> {
    // `specs/08-seguranca.md`: a interface esconder é conveniência; o servidor
    // negar é a segurança. A casca pergunta e obedece — e este par cru não
    // pergunta nada, que é exatamente a casca que não existe.
    let (endereco, servidor) = dogma(Location::Memory).await?;

    // A primeira conta vira a Comandante, então a segunda é uma Pessoa comum.
    let anfitria = abrir(endereco, 1).await?;
    let mut visita = abrir(endereco, 2).await?;

    frame::write(
        &mut visita.envio,
        &ClientMessage::RenameDogma {
            name: "Meu Agora".into(),
        },
    )
    .await?;
    frame::write(
        &mut visita.envio,
        &ClientMessage::SetDogmaIcon {
            icon: Some(png(64)),
        },
    )
    .await?;

    let mut recusas = 0;
    esperar(&mut visita, |quadro| {
        if matches!(
            quadro,
            ServerMessage::Alert {
                reason: AlertReason::PermissionDenied,
                ..
            }
        ) {
            recusas += 1;
        }
        (recusas == 2).then_some(())
    })
    .await
    .expect("o Dogma recusou calado, que é indistinguível de estar quebrado");

    // E a recusa foi de verdade: quem entra depois lê o nome de antes e não
    // recebe imagem nenhuma.
    let depois = abrir(endereco, 3).await?;
    assert_eq!(depois.nome_do_dogma, "Casa");
    assert_eq!(depois.icone, None);

    drop(anfitria);
    servidor.shutdown();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn o_nome_e_o_icone_sobrevivem_a_um_reinicio() -> Result<()> {
    // O critério com que a tabela `configuracao` foi criada, cobrado de ponta a
    // ponta: o processo do Dogma cai, o banco continua no disco, e quem volta
    // encontra o que quem hospeda escolheu — e não o `name` da `DogmaConfig`,
    // que é o que estava sendo mandado antes desta mudança.
    let diretorio = tempfile::tempdir()?;
    let arquivo = diretorio.path().join("dogma.db");
    let imagem = png(MAX_DOGMA_ICON_SIDE);

    {
        let (endereco, servidor) = dogma(Location::File(arquivo.clone())).await?;
        let mut anfitria = abrir(endereco, 1).await?;
        frame::write(
            &mut anfitria.envio,
            &ClientMessage::RenameDogma {
                name: "Terceira Tóquio".into(),
            },
        )
        .await?;
        frame::write(
            &mut anfitria.envio,
            &ClientMessage::SetDogmaIcon {
                icon: Some(imagem.clone()),
            },
        )
        .await?;
        // Esperar o próprio aviso é o que garante que a gravação aconteceu: ele
        // sai **depois** do banco ter aceitado.
        esperar(&mut anfitria, |quadro| {
            matches!(quadro, ServerMessage::DogmaIconChanged { icon: Some(_) }).then_some(())
        })
        .await
        .expect("o Dogma não confirmou o ícone");
        servidor.shutdown();
    }

    let (endereco, servidor) = dogma(Location::File(arquivo)).await?;
    let de_volta = abrir(endereco, 1).await?;
    assert_eq!(
        de_volta.nome_do_dogma, "Terceira Tóquio",
        "o reinício devolveu o nome do arranque por cima do que quem hospeda escolheu"
    );
    assert_eq!(de_volta.icone, Some(imagem));

    servidor.shutdown();
    Ok(())
}
