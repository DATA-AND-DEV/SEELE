//! A janela em que um estranho virava o Comandante de um servidor recém-criado.
//!
//! **O defeito, como foi lido em `4fc31e9c`.** `Hospedagem::iniciar` punha o
//! laço de aceitação no ar, e só **depois** o app semeava a portaria e admitia
//! quem hospeda. Num banco novo a portaria nasce desligada, e a primeira conta
//! criada num servidor vira a Comandante (`permissions::seat_the_arrival`).
//! Quem chegasse nesse intervalo entrava sem bater e ficava com o servidor.
//!
//! **O conserto** (D-seg-1 do spec da 0.16, a tarefa 4e do Plano 5, que entra
//! pelo Plano 2): `Hospedagem::iniciar` recebe o que fazer **antes de
//! aceitar**, e roda isso com o banco já aberto e o laço ainda parado. A ordem
//! deixou de depender de quem chama. É o desenho de `ba99007`, que a linha
//! 0.15 leva na 0.15.2.
//!
//! **As duas formas de banco.** Em arquivo é o servidor guardado; em memória é
//! a forma da sala pessoal, que sobe pela mesma porta (tarefa 4f). As duas
//! passam pelo mesmo `iniciar`, e a prova roda nas duas.
//!
//! **A prova tem duas metades.**
//!
//! 1. **A estrutural:** enquanto o gancho roda, um estranho tenta conectar, e
//!    a conexão **não** se estabelece. No quinn 0.11 o aperto de mão só anda
//!    quando o laço chama `accept`, então uma conexão estabelecida durante o
//!    gancho é a prova de que o laço já estava no ar. Esta é a metade que
//!    reprova com a ordem de antes.
//! 2. **A de quem usa:** depois, o estranho bate e fica de fora, e quem hospeda
//!    entra e é o Comandante.
//!
//! Por que a primeira metade não é «o estranho entrou durante a espera»: o
//! gancho roda com o banco travado, e a sessão do estranho precisa do banco
//! para criar a conta. A primeira tentativa desta prova, em `ba99007`, passou
//! revertida por isso. A conexão aceita não depende do banco, e é ela que diz
//! se o laço já estava no ar.
//!
//! # Por que um par cru, e não um `Client`
//!
//! O mesmo motivo de `personalizacao.rs`: `seele-core` não pode ser dependência
//! daqui (ADR 0002).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use ed25519_dalek::{Signer, SigningKey};
use seele_proto::control::{ClientMessage, DisconnectReason, Permission, ServerMessage};
use seele_server::frame;
use seele_server::hospedagem::Hospedagem;
use seele_server::permissions::Permissions;
use seele_server::persistence::Location;

/// A semente da chave de quem hospeda.
const DONO: u8 = 1;
/// A semente da chave de quem chega sem ter sido chamado.
const ESTRANHO: u8 = 7;

/// Um verificador que aceita qualquer certificado: o que está sob teste não é
/// o TOFU, e o certificado é o que este servidor acabou de gerar.
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

/// O que aconteceu com uma tentativa de entrar.
#[derive(Debug, PartialEq, Eq)]
enum Desfecho {
    /// O servidor mandou `Session`: a conta existe e está dentro.
    Entrou,
    /// O servidor encerrou o aperto de mão, com o motivo.
    Barrado(DisconnectReason),
}

/// A configuração de cliente de um par cru.
fn cliente() -> quinn::ClientConfig {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut tls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AceitaQualquer(provider)))
        .with_no_client_auth();
    tls.alpn_protocols = vec![seele_proto::transport::ALPN.to_vec()];
    quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls).expect("config do QUIC"),
    ))
}

/// A prova de identidade do S2: o rótulo, a impressão do certificado que o TLS
/// desta conexão aceitou e o nonce, montados por `seele_proto::assinatura`.
fn prova_amarrada(conexao: &quinn::Connection, nonce: &[u8]) -> Vec<u8> {
    let cadeia = conexao
        .peer_identity()
        .and_then(|identidade| {
            identidade
                .downcast::<Vec<rustls::pki_types::CertificateDer<'static>>>()
                .ok()
        })
        .expect("o TLS sempre entrega a cadeia do servidor");
    let impressao = seele_proto::transport::certificate_digest(
        cadeia
            .first()
            .expect("a cadeia tem um certificado")
            .as_ref(),
    );
    let nonce: [u8; 32] = nonce.try_into().expect("o desafio tem 32 bytes");
    seele_proto::assinatura::prova_de_cliente(&impressao, &nonce)
}

/// Um aperto de mão inteiro, sem convite, com a chave desta semente.
async fn tentar(endereco: SocketAddr, semente: u8) -> Result<Desfecho> {
    let mut ponta = quinn::Endpoint::client(SocketAddr::from(([127, 0, 0, 1], 0)))?;
    ponta.set_default_client_config(cliente());
    let conexao = ponta.connect(endereco, "localhost")?.await?;
    let (mut envio, mut recebe) = conexao.open_bi().await?;
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

    loop {
        match frame::read::<ServerMessage>(&mut recebe).await? {
            ServerMessage::Challenge { nonce } => {
                frame::write(
                    &mut envio,
                    &ClientMessage::Response {
                        proof: chave
                            .sign(&prova_amarrada(&conexao, &nonce))
                            .to_bytes()
                            .to_vec(),
                    },
                )
                .await?;
            }
            ServerMessage::Session { .. } => return Ok(Desfecho::Entrou),
            ServerMessage::Disconnecting { reason } => return Ok(Desfecho::Barrado(reason)),
            // Outro quadro do aperto de mão: segue lendo.
            _ => {}
        }
    }
}

/// Só o aperto de mão do QUIC, sem nenhuma mensagem do SEELE. Volta quando a
/// conexão se estabelece, o que só acontece com o laço de aceitação no ar.
async fn so_conectar(endereco: SocketAddr) -> Result<()> {
    let mut ponta = quinn::Endpoint::client(SocketAddr::from(([127, 0, 0, 1], 0)))?;
    ponta.set_default_client_config(cliente());
    ponta.connect(endereco, "localhost")?.await?;
    Ok(())
}

/// Desliga o degrau 4 para o binário inteiro: nenhum teste daqui fala com o
/// ponto de encontro de produção.
///
/// Sem `$SEELE_ENCONTRO`, `Hospedagem::iniciar` caía no ponto padrão, e numa
/// máquina sem IPv4 global cada rodada registrava um servidor de teste no
/// quarto da VPS de produção. Desde a decisão (d) do lote G-servidor ele não
/// convoca ponto nenhum sem a variável, e o `nao` daqui é a intenção escrita:
/// o que este arquivo mede é a janela do Comandante, e nenhum pacote precisa
/// sair desta máquina para isso.
///
/// **Uma vez, e sem devolver.** Os dois testes daqui correm ao mesmo tempo, e
/// os dois querem o mesmo valor. Um guarda que devolvesse a variável ao cair
/// a devolveria no meio do `iniciar` do outro, que então leria o valor de antes
/// da troca: hoje, sem a variável, «nenhum ponto», e o teste nem perceberia;
/// mas um `$SEELE_ENCONTRO` que viesse do terminal de quem roda a bateria
/// voltaria a valer no meio do teste. Todo teste deste binário sobe
/// hospedagem; quem um dia acrescentar um que precise de outro valor troca
/// isto por um guarda com trava, como o `ambiente_de_teste` da biblioteca.
///
/// O guarda da biblioteca (`alcance::encontro::recusar_a_producao`) alcança
/// este arquivo só quando ele roda junto com o `seele-conformance`
/// (`cargo test --workspace`): ver a feature `recusar-encontro-de-producao` no
/// `Cargo.toml` deste crate. Rodado sozinho, quem o protege é o padrão.
fn sem_degrau_4() {
    static UMA_VEZ: std::sync::Once = std::sync::Once::new();
    UMA_VEZ.call_once(|| std::env::set_var(seele_server::alcance::encontro::VARIAVEL, "nao"));
}

/// Uma porta livre nesta máquina, para o estranho saber onde bater antes de
/// `iniciar` devolver o servidor.
fn porta_livre() -> u16 {
    std::net::UdpSocket::bind("127.0.0.1:0")
        .and_then(|s| s.local_addr())
        .map(|e| e.port())
        .expect("uma porta livre")
}

fn chave_publica(semente: u8) -> [u8; 32] {
    SigningKey::from_bytes(&[semente; 32])
        .verifying_key()
        .to_bytes()
}

/// A janela, medida num banco desta forma.
async fn a_janela_fica_fechada(banco: Location) {
    sem_degrau_4();
    let porta = porta_livre();
    let endereco = SocketAddr::from(([127, 0, 0, 1], porta));

    let dono = seele_proto::transport::key_fingerprint(&chave_publica(DONO));
    let aceito_durante_o_gancho = Arc::new(AtomicBool::new(false));
    let anotado = Arc::clone(&aceito_durante_o_gancho);
    let hospedagem = Hospedagem::iniciar(porta, banco, "Casa", None, move |persistence| {
        // O servidor já tem porta. Um estranho bate agora, numa linha de
        // execução própria (o gancho é síncrono), e tem um segundo e meio
        // para ser aceito. Se o laço estivesse no ar, seria.
        let tentativa = std::thread::spawn(move || {
            let tempo = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("um runtime para o estranho");
            tempo.block_on(async {
                tokio::time::timeout(Duration::from_millis(1500), so_conectar(endereco)).await
            })
        });
        let aceito = matches!(tentativa.join(), Ok(Ok(Ok(()))));
        anotado.store(aceito, Ordering::SeqCst);

        seele_server::portaria::semear_ligada(persistence)?;
        seele_server::portaria::admitir_o_dono(persistence, &dono)
    })
    .await
    .expect("subir");

    // A metade estrutural.
    assert!(
        !aceito_durante_o_gancho.load(Ordering::SeqCst),
        "uma conexão foi aceita enquanto quem hospeda ainda semeava a portaria: \
         o laço de aceitação nasceu antes do gancho, e a janela do Comandante voltou"
    );

    // A metade de quem usa: o estranho bate agora e fica de fora.
    let desfecho_do_estranho =
        tokio::time::timeout(Duration::from_secs(20), tentar(endereco, ESTRANHO))
            .await
            .expect("o estranho teve resposta dentro do prazo")
            .expect("o aperto de mão do estranho terminou");
    assert!(
        matches!(desfecho_do_estranho, Desfecho::Barrado(_)),
        "um estranho entrou sem convite num servidor recém-criado: {desfecho_do_estranho:?}"
    );

    assert_eq!(
        tentar(endereco, DONO).await.expect("o dono conecta"),
        Desfecho::Entrou,
        "quem hospeda não entrou na própria casa"
    );

    let banco = hospedagem.persistence();
    let persistence = banco.lock().await;
    let permissoes = Permissions::new(&persistence);
    let do_dono = permissoes
        .register_or_find(&chave_publica(DONO), "pessoa001")
        .unwrap();
    assert!(
        permissoes
            .may(do_dono.id, Permission::AdministerServer)
            .unwrap(),
        "quem hospeda não ficou com o servidor"
    );
    let do_estranho = permissoes
        .register_or_find(&chave_publica(ESTRANHO), "pessoa007")
        .unwrap();
    assert!(
        !permissoes
            .may(do_estranho.id, Permission::AdministerServer)
            .unwrap(),
        "o estranho ficou com o servidor"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn quem_chega_enquanto_o_servidor_guardado_sobe_nao_vira_comandante() {
    let pasta = tempfile::tempdir().unwrap();
    a_janela_fica_fechada(Location::File(pasta.path().join("seele.db"))).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn quem_chega_enquanto_um_servidor_em_memoria_sobe_nao_vira_comandante() {
    a_janela_fica_fechada(Location::Memory).await;
}
