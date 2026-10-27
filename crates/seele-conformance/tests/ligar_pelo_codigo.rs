//! Quem liga confere o código dentro do TLS, antes do `Hello` (tarefa 6b do
//! Plano 5, ADR 0060).
//!
//! Um `Destino` com `Esperada::Codigo` conecta pelo `ConfereCodigo`, e não pelo
//! TOFU por endereço:
//!
//! - a chave de quem atendeu tem de ser a do código, ou a conexão cai antes de
//!   qualquer quadro, com o motivo tipado;
//! - depois da primeira ligação completa, a chave inteira fica pinada sob
//!   `@CÓDIGO`, e dali em diante uma chave diferente com o mesmo código é
//!   «se passou por»;
//! - o código não viaja legível: até a 0.16, o SNI era sempre o nome da sala;
//!   desde a 0.17 (WebRTC, sem SNI), nenhum datagrama da ligação leva o código
//!   nem a chave da pessoa em claro.
//!
//! O caminho medido de ponta a ponta é o de quem recebe ligando para a própria
//! sala (`127.0.0.1:8385`, tarefa 7e): ele não toca, então não depende do
//! cliente que lê o `Tocando` (tarefa 6e).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use seele_core::confere_codigo::{chave_do_pino, FalhaDoCodigo};
use seele_core::enlace::{Destino, Enlace, Esperada};
use seele_core::{ConnectError, MemoryPinStore, PinStore};
use seele_proto::codigo::Codigo;
use seele_proto::transport::NOME_DA_SALA;
use seele_server::recepcao::Sala;
use seele_server::tls::Identity;
use seele_server::{Daemon, ServerConfig};

mod vaga;

/// A semente de quem recebe.
const DONO: u8 = 1;

fn chave(semente: u8) -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::from_bytes(&[semente; 32])
}

fn codigo_de(semente: u8) -> Codigo {
    Codigo::da_chave(&chave(semente).verifying_key().to_bytes())
}

/// A sala de `DONO`, em memória.
async fn sala() -> SocketAddr {
    let config = ServerConfig {
        identidade: Some(Identity::da_semente(&[DONO; 32]).unwrap()),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        voice_room_limit: 6,
        sala: Some(Sala::de(chave(DONO).verifying_key().to_bytes())),
        ..ServerConfig::default()
    };
    let servidor = Arc::new(Daemon::bind(config).await.unwrap());
    let endereco = servidor.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = servidor.run().await;
    });
    endereco
}

fn destino(endereco: SocketAddr, codigo: Codigo, nome_tls: &str) -> Destino {
    Destino {
        servidor: endereco,
        nome_tls: nome_tls.to_owned(),
        chave_do_pin: chave_do_pino(&codigo),
        apelido: "dona".into(),
        segredo: None,
        esperada: Some(Esperada::Codigo(codigo)),
        o_pino_prova_o_servidor: false,
        aceito: None,
    }
}

#[tokio::test]
async fn quem_recebe_liga_para_a_propria_sala_pelo_codigo_e_a_chave_fica_pinada() {
    let _vaga = vaga::minha();
    let endereco = sala().await;
    let pins = Arc::new(MemoryPinStore::new());
    let codigo = codigo_de(DONO);

    let enlace = Enlace::conectar(
        destino(endereco, codigo, NOME_DA_SALA),
        chave(DONO),
        Arc::clone(&pins) as Arc<dyn PinStore>,
    )
    .await
    .expect("quem recebe não entrou na própria sala pelo código");
    drop(enlace);

    assert_eq!(
        pins.pinned(&chave_do_pino(&codigo)),
        Some(seele_proto::transport::key_fingerprint(
            &chave(DONO).verifying_key().to_bytes()
        )),
        "depois de uma ligação completa, a chave inteira não ficou pinada sob @CÓDIGO"
    );
}

#[tokio::test]
async fn o_codigo_errado_e_recusado_dentro_do_tls_com_os_dois_codigos() {
    let _vaga = vaga::minha();
    let endereco = sala().await;
    let pins = Arc::new(MemoryPinStore::new());
    let esperado = codigo_de(7);

    let erro = Enlace::conectar(
        destino(endereco, esperado, NOME_DA_SALA),
        chave(DONO),
        Arc::clone(&pins) as Arc<dyn PinStore>,
    )
    .await
    .expect_err("o código errado entrou na sala de outra pessoa");
    assert_eq!(
        erro,
        ConnectError::CodigoNaoConfere(FalhaDoCodigo::CodigoNaoBate {
            esperado,
            veio: codigo_de(DONO),
        }),
        "o motivo não chegou tipado: quem liga leria uma falha genérica"
    );
    assert_eq!(
        pins.pinned(&chave_do_pino(&esperado)),
        None,
        "uma ligação recusada deixou pino"
    );
}

#[tokio::test]
async fn uma_chave_diferente_da_pinada_se_passou_pelo_dono_do_codigo() {
    let _vaga = vaga::minha();
    let endereco = sala().await;
    let pins = Arc::new(MemoryPinStore::new());
    let codigo = codigo_de(DONO);
    // O pino de outra chave com o mesmo código: a situação de quem achou uma
    // colisão nos 75 bits.
    pins.pin(
        &chave_do_pino(&codigo),
        seele_proto::transport::key_fingerprint(&chave(9).verifying_key().to_bytes()),
    );

    let erro = Enlace::conectar(
        destino(endereco, codigo, NOME_DA_SALA),
        chave(DONO),
        Arc::clone(&pins) as Arc<dyn PinStore>,
    )
    .await
    .expect_err("uma chave diferente da pinada passou só porque o código batia");
    assert_eq!(
        erro,
        ConnectError::CodigoNaoConfere(FalhaDoCodigo::SePassouPor { codigo })
    );
}

/// **O código não viaja legível** (ADR 0060), desde a 0.17 pelo WebRTC.
///
/// Até a 0.16 a pergunta era o SNI do QUIC: quem liga pelo código mandava
/// sempre o nome da sala (`NOME_DA_SALA`), e nunca o código, no Initial que
/// qualquer um na rede lê. O WebRTC não tem SNI. O que vai em claro antes de a
/// conexão segura fechar são os Bindings STUN (com o `U` sorteado por
/// tentativa) e o aperto do DTLS, com os certificados P-256 das duas pontas,
/// efêmeros, sem nome. A chave e a assinatura da sala só viajam dentro do
/// `ctl`, cifradas (a `0x03`, D3).
///
/// Este teste põe um retransmissor UDP entre quem liga e a sala, deixa a
/// ligação inteira passar por ele, e confere que **nenhum** datagrama, de
/// nenhum dos dois lados, leva o código escrito ou os bytes da chave da
/// pessoa. É mais forte que o guarda do SNI, que olhava um campo só.
#[tokio::test]
async fn o_codigo_e_a_chave_nao_viajam_legiveis_numa_ligacao_pelo_codigo() {
    let _vaga = vaga::minha();
    let endereco_da_sala = sala().await;

    // O retransmissor: quem liga fala com `frente`, e `tras` fala com a sala.
    let frente = Arc::new(tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let tras = Arc::new(tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let endereco_da_frente = frente.local_addr().unwrap();
    let vistos: Arc<std::sync::Mutex<Vec<Vec<u8>>>> = Arc::default();
    let quem_liga: Arc<std::sync::Mutex<Option<SocketAddr>>> = Arc::default();
    {
        let (frente, tras, vistos, quem_liga) = (
            Arc::clone(&frente),
            Arc::clone(&tras),
            Arc::clone(&vistos),
            Arc::clone(&quem_liga),
        );
        tokio::spawn(async move {
            let mut balde = vec![0_u8; 65_536];
            while let Ok((n, de)) = frente.recv_from(&mut balde).await {
                *quem_liga.lock().unwrap() = Some(de);
                let lido = balde.get(..n).unwrap_or_default();
                vistos.lock().unwrap().push(lido.to_vec());
                let _ = tras.send_to(lido, endereco_da_sala).await;
            }
        });
    }
    {
        let (frente, tras, vistos, quem_liga) = (
            Arc::clone(&frente),
            Arc::clone(&tras),
            Arc::clone(&vistos),
            Arc::clone(&quem_liga),
        );
        tokio::spawn(async move {
            let mut balde = vec![0_u8; 65_536];
            while let Ok((n, _)) = tras.recv_from(&mut balde).await {
                let lido = balde.get(..n).unwrap_or_default();
                vistos.lock().unwrap().push(lido.to_vec());
                let destino = *quem_liga.lock().unwrap();
                if let Some(destino) = destino {
                    let _ = frente.send_to(lido, destino).await;
                }
            }
        });
    }

    let codigo = codigo_de(DONO);
    let enlace = tokio::time::timeout(
        Duration::from_secs(15),
        Enlace::conectar(
            destino(endereco_da_frente, codigo, NOME_DA_SALA),
            chave(DONO),
            Arc::new(MemoryPinStore::new()) as Arc<dyn PinStore>,
        ),
    )
    .await
    .expect("a ligação pelo retransmissor passou do prazo")
    .expect("a ligação pelo código não passou pelo retransmissor");
    drop(enlace);

    let vistos = vistos.lock().unwrap().clone();
    assert!(
        vistos.len() > 4,
        "o retransmissor viu {} datagramas: a ligação não passou por ele",
        vistos.len()
    );
    let texto = codigo.to_string();
    let sem_tracos: String = texto.chars().filter(|c| *c != '-').collect();
    let chave_publica = chave(DONO).verifying_key().to_bytes();
    let contem = |palheiro: &[u8], agulha: &[u8]| {
        !agulha.is_empty()
            && palheiro
                .windows(agulha.len())
                .any(|janela| janela == agulha)
    };
    for (i, datagrama) in vistos.iter().enumerate() {
        assert!(
            !contem(datagrama, texto.as_bytes()) && !contem(datagrama, sem_tracos.as_bytes()),
            "o datagrama {i} leva o código legível"
        );
        assert!(
            !contem(datagrama, &chave_publica),
            "o datagrama {i} leva a chave da pessoa em claro"
        );
    }
}
