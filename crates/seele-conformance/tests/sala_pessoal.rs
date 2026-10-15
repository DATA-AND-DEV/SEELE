//! A hospedagem da sala pessoal (tarefa 4f do Plano 5): a segunda hospedagem do
//! app, em memória e sem MODs, com a identidade da pessoa, a campainha, e o
//! registro assinado no ponto de encontro.
//!
//! O que se prova aqui, de ponta a ponta:
//!
//! - a sala apresenta a chave da pessoa, e quem recebe entra nela pelo código;
//! - **um convidado atendido antes de quem recebe não fica com a sala**: na sala
//!   não há portaria, e o dono só conecta ao atender. Sem a conta do dono criada
//!   antes de aceitar, o primeiro atendido viraria a Comandante (o achado A2);
//! - a sala não manda a lista de membros (D-fio-4);
//! - a sala se registra assinada no ponto, e quem liga a acha pelo código.
//!
//! Todo ponto daqui é `127.0.0.1`, e os guardas das duas pontas reprovariam um
//! teste que perguntasse à VPS de produção.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use ed25519_dalek::{Signer, SigningKey};
use seele_core::confere_codigo::chave_do_pino;
use seele_core::encontro::{localizar, Localizacao, PRAZO_DO_QUARTO};
use seele_core::enlace::{Destino, Enlace, Esperada};
use seele_core::{MemoryPinStore, PinStore};
use seele_proto::codigo::Codigo;
use seele_proto::control::{ClientMessage, Permission, ServerMessage};
use seele_proto::encontro::{Marcas, Vizinhanca};
use seele_server::alcance::encontro::EstadoDoRegistro;
use seele_server::hospedagem::Hospedagem;
use seele_server::permissions::Permissions;
use seele_server::recepcao::Aviso;
use seele_server::tls::Identity;

mod vaga;

/// A semente de quem recebe.
const DONO: u8 = 1;
/// A semente de quem liga.
const CONVIDADO: u8 = 2;

fn chave(semente: u8) -> SigningKey {
    SigningKey::from_bytes(&[semente; 32])
}

fn publica(semente: u8) -> [u8; 32] {
    chave(semente).verifying_key().to_bytes()
}

fn codigo_de(semente: u8) -> Codigo {
    Codigo::da_chave(&publica(semente))
}

/// Um ponto de encontro de verdade, atendendo numa linha própria.
fn subir_o_ponto() -> SocketAddr {
    let ponto = seele_encontro::Ponto::abrir_com_quarto(
        SocketAddr::from(([127, 0, 0, 1], 0)),
        Vizinhanca::TambemAqui,
        Arc::new(seele_encontro::Quarto::novo()),
    )
    .expect("a escuta tem que abrir");
    let onde = ponto.endereco().expect("o socket sabe onde ligou");
    std::thread::spawn(move || {
        let _ = ponto.servir();
    });
    onde
}

async fn subir_a_sala(ponto: Option<String>) -> Hospedagem {
    Hospedagem::iniciar_sala(0, &[DONO; 32], "dona", ponto)
        .await
        .expect("a sala não subiu")
}

/// `127.0.0.1` na porta da sala: ela escuta em `[::]`.
fn local(sala: &Hospedagem) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], sala.endereco().port()))
}

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

/// Um par cru até a prova: quem liga ainda não lê o `Tocando` (tarefa 6e).
struct Par {
    _conexao: quinn::Connection,
    _envio: quinn::SendStream,
    recebe: quinn::RecvStream,
    _ponta: quinn::Endpoint,
}

impl Par {
    async fn proximo(&mut self) -> anyhow::Result<ServerMessage> {
        seele_server::frame::read::<ServerMessage>(&mut self.recebe).await
    }
}

async fn par_cru(endereco: SocketAddr, semente: u8) -> Par {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut tls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AceitaQualquer(provider)))
        .with_no_client_auth();
    tls.alpn_protocols = vec![seele_proto::transport::ALPN.to_vec()];
    let mut ponta = quinn::Endpoint::client(SocketAddr::from(([127, 0, 0, 1], 0))).unwrap();
    ponta.set_default_client_config(quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls).unwrap(),
    )));
    let conexao = ponta
        .connect(endereco, seele_proto::transport::NOME_DA_SALA)
        .unwrap()
        .await
        .unwrap();
    let (mut envio, mut recebe) = conexao.open_bi().await.unwrap();
    let pessoa = chave(semente);
    seele_server::frame::write(
        &mut envio,
        &ClientMessage::Hello {
            faixa: seele_proto::FAIXA,
            client: "par-cru".into(),
            nickname: format!("pessoa{semente:03}"),
            public_key: pessoa.verifying_key().to_bytes().to_vec(),
            join_secret: None,
            material_e2ee: None,
            apis_de_mod: seele_proto::mods::APIS_ACEITAS.to_vec(),
        },
    )
    .await
    .unwrap();
    let ServerMessage::Challenge { nonce } =
        seele_server::frame::read::<ServerMessage>(&mut recebe)
            .await
            .unwrap()
    else {
        panic!("a sala não mandou Challenge");
    };
    let cadeia = conexao
        .peer_identity()
        .unwrap()
        .downcast::<Vec<rustls::pki_types::CertificateDer<'static>>>()
        .unwrap();
    let impressao = seele_proto::transport::certificate_digest(cadeia.first().unwrap().as_ref());
    let nonce: [u8; 32] = nonce.as_slice().try_into().unwrap();
    seele_server::frame::write(
        &mut envio,
        &ClientMessage::Response {
            proof: pessoa
                .sign(&seele_proto::assinatura::prova_de_cliente(
                    &impressao, &nonce,
                ))
                .to_bytes()
                .to_vec(),
        },
    )
    .await
    .unwrap();
    Par {
        _conexao: conexao,
        _envio: envio,
        recebe,
        _ponta: ponta,
    }
}

#[tokio::test]
async fn a_sala_apresenta_a_chave_da_pessoa_e_quem_recebe_entra_pelo_codigo() {
    let _vaga = vaga::minha();
    let sala = subir_a_sala(None).await;
    assert_eq!(
        sala.impressao_digital(),
        Identity::da_semente(&[DONO; 32]).unwrap().fingerprint(),
        "a sala não apresentou a identidade da pessoa"
    );
    assert!(sala.recepcao().is_some(), "a sala subiu sem campainha");

    let codigo = codigo_de(DONO);
    Enlace::conectar(
        Destino {
            servidor: local(&sala),
            nome_tls: seele_proto::transport::NOME_DA_SALA.into(),
            chave_do_pin: chave_do_pino(&codigo),
            apelido: "dona".into(),
            segredo: None,
            esperada: Some(Esperada::Codigo(codigo)),
            o_pino_prova_o_servidor: false,
            aceito: None,
        },
        chave(DONO),
        Arc::new(MemoryPinStore::new()) as Arc<dyn PinStore>,
    )
    .await
    .expect("quem recebe não entrou na própria sala pelo código");
    sala.encerrar().await;
}

#[tokio::test]
async fn um_convidado_atendido_antes_de_quem_recebe_nao_fica_com_a_sala() {
    let _vaga = vaga::minha();
    let sala = subir_a_sala(None).await;
    let recepcao = sala.recepcao().unwrap();
    let mut avisos = recepcao.assinar();

    let mut convidado = par_cru(local(&sala), CONVIDADO).await;
    assert!(matches!(
        convidado.proximo().await.unwrap(),
        ServerMessage::Tocando { .. }
    ));
    let toque = loop {
        if let Aviso::Tocou { toque, .. } =
            tokio::time::timeout(Duration::from_secs(5), avisos.recv())
                .await
                .unwrap()
                .unwrap()
        {
            break toque;
        }
    };
    assert!(recepcao.atender(toque));
    assert!(matches!(
        convidado.proximo().await.unwrap(),
        ServerMessage::Session { .. }
    ));

    let banco = sala.persistence();
    let banco = banco.lock().await;
    let permissoes = Permissions::new(&banco);
    let do_convidado = permissoes
        .register_or_find(&publica(CONVIDADO), "pessoa002")
        .unwrap();
    assert!(
        !permissoes
            .may(do_convidado.id, Permission::AdministerServer)
            .unwrap(),
        "o primeiro atendido ficou com a sala de outra pessoa"
    );
    let do_dono = permissoes.register_or_find(&publica(DONO), "dona").unwrap();
    assert!(
        permissoes
            .may(do_dono.id, Permission::AdministerServer)
            .unwrap(),
        "quem recebe não é a Comandante da própria sala"
    );
}

/// Os quadros que chegam até `prazo` de silêncio.
async fn o_que_chega(par: &mut Par, prazo: Duration) -> Vec<ServerMessage> {
    let mut chegou = Vec::new();
    while let Ok(Ok(quadro)) = tokio::time::timeout(prazo, par.proximo()).await {
        chegou.push(quadro);
    }
    chegou
}

#[tokio::test]
async fn a_sala_nao_manda_a_lista_de_membros_e_um_servidor_guardado_manda() {
    let _vaga = vaga::minha();
    let sala = subir_a_sala(None).await;
    let mut dono = par_cru(local(&sala), DONO).await;
    let na_sala = o_que_chega(&mut dono, Duration::from_millis(800)).await;
    assert!(
        na_sala
            .iter()
            .any(|quadro| matches!(quadro, ServerMessage::Session { .. })),
        "quem recebe não entrou: {na_sala:?}"
    );
    assert!(
        !na_sala
            .iter()
            .any(|quadro| matches!(quadro, ServerMessage::Membros { .. })),
        "a sala mandou a lista de membros (D-fio-4): {na_sala:?}"
    );
    sala.encerrar().await;

    // O controle: um servidor guardado manda, e é o mesmo par cru que lê.
    let guardado = Hospedagem::iniciar(
        0,
        seele_server::persistence::Location::Memory,
        "Casa",
        None,
        |_| Ok(()),
    )
    .await
    .unwrap();
    let mut alguem = par_cru(
        SocketAddr::from(([127, 0, 0, 1], guardado.endereco().port())),
        DONO,
    )
    .await;
    let no_guardado = o_que_chega(&mut alguem, Duration::from_millis(800)).await;
    assert!(
        no_guardado
            .iter()
            .any(|quadro| matches!(quadro, ServerMessage::Membros { .. })),
        "o controle não viu a lista: o teste de cima não prova nada. {no_guardado:?}"
    );
    guardado.encerrar().await;
}

#[tokio::test]
async fn a_sala_se_registra_assinada_e_quem_liga_a_acha_pelo_codigo() {
    let _vaga = vaga::minha();
    let ponto = subir_o_ponto();
    let sala = subir_a_sala(Some(ponto.to_string())).await;

    let mut registro = sala
        .registro_no_quarto()
        .expect("a sala subiu sem registro no ponto");
    let registrou = tokio::time::timeout(
        Duration::from_secs(5),
        registro.wait_for(|estado| *estado == EstadoDoRegistro::Registrado),
    )
    .await
    .is_ok_and(|visto| visto.is_ok());
    assert!(
        registrou,
        "a sala não se registrou no ponto: {:?}",
        *registro.borrow()
    );

    let achado = localizar(
        &ponto.to_string(),
        &Marcas::da_sala(&codigo_de(DONO)),
        PRAZO_DO_QUARTO,
    )
    .await;
    let Localizacao::Achada {
        servidor: Some(onde),
        ..
    } = achado
    else {
        panic!("quem liga não achou a sala pelo código: {achado:?}");
    };
    assert_eq!(
        onde.port(),
        sala.endereco().port(),
        "o ponto deu outro endereço que o do servidor da sala"
    );
    sala.encerrar().await;
}

#[tokio::test]
async fn a_sala_que_fecha_diz_chamada_encerrada_e_nao_servidor_encerrando() {
    // O app que fecha se despede das duas hospedagens (tarefa 21). Para quem
    // está num servidor guardado, o servidor está encerrando; para quem está
    // numa ligação, a ligação acabou. A frase de cada um é outra, e a razão que
    // chega é o que a escolhe.
    let _vaga = vaga::minha();
    let sala = subir_a_sala(None).await;
    let mut dono = par_cru(local(&sala), DONO).await;
    let entrou = o_que_chega(&mut dono, Duration::from_millis(800)).await;
    assert!(
        entrou
            .iter()
            .any(|quadro| matches!(quadro, ServerMessage::Session { .. })),
        "quem recebe não entrou: {entrou:?}"
    );

    sala.despedir(Duration::from_secs(2)).await;

    let depois = o_que_chega(&mut dono, Duration::from_millis(800)).await;
    assert!(
        depois.iter().any(|quadro| matches!(
            quadro,
            ServerMessage::Disconnecting {
                reason: seele_proto::control::DisconnectReason::ChamadaEncerrada
            }
        )),
        "a sala se despediu sem dizer que a ligação acabou: {depois:?}"
    );
}
