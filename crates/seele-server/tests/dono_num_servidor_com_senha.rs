//! **Quem hospeda entra na própria casa sem a senha dela.**
//!
//! O defeito, achado ao pôr a escolha «com senha» na hora de hospedar: num
//! servidor **com senha e sem a portaria**, quem hospeda era recusado na
//! própria porta, com `CredentialRejected`. O app entra no servidor que acabou
//! de subir sem segredo nenhum (`conectar()`), a camada do segredo o recusa, e
//! o perdão que existia para isso — `portaria::ja_admitido` — só vale com a
//! portaria **ligada**. Com ela desligada, a linha que
//! `portaria::admitir_o_dono` grava não era lida por ninguém.
//!
//! Já dava para chegar lá na 1.1.0, pelo painel A PORTA: DEFINIR uma senha,
//! DESLIGAR a portaria, sair e entrar de novo.
//!
//! **O conserto:** o dono declarado pela máquina que hospeda
//! (`cargos::declarar_proprietario`, ADR 0064) não precisa do segredo. A
//! pergunta é feita depois da assinatura, com a impressão provada, no mesmo
//! ponto em que o perdão da portaria já era dado (`session::consultar_portaria`).
//!
//! **O que não muda**, e este arquivo prende: sem a senha ninguém mais passa,
//! e uma linha de `admitido` na portaria desligada continua não dispensando a
//! senha de quem não é o dono.
//!
//! # Por que um par cru, e não um `Client`
//!
//! O mesmo motivo de `janela_do_comandante.rs`: `seele-core` não pode ser
//! dependência daqui (ADR 0002).

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::time::Duration;

use anyhow::Result;
use ed25519_dalek::{Signer, SigningKey};
use seele_proto::control::{ClientMessage, DisconnectReason, ServerMessage};
use seele_server::frame;
use seele_server::hospedagem::Hospedagem;
use seele_server::persistence::Location;

mod comum;

/// A semente da chave de quem hospeda.
const DONO: u8 = 1;
/// A semente de quem foi admitido pela portaria, quando ela estava ligada.
const ADMITIDA: u8 = 5;
/// A semente da chave de quem chega sem ter sido chamado.
const ESTRANHO: u8 = 7;

const SENHA: &str = "terceiro impacto";

/// O que aconteceu com uma tentativa de entrar.
#[derive(Debug, PartialEq, Eq)]
enum Desfecho {
    /// O servidor mandou `Session`: a conta existe e está dentro.
    Entrou,
    /// O servidor encerrou o aperto de mão, com o motivo.
    Barrado(DisconnectReason),
}

fn impressao(semente: u8) -> String {
    seele_proto::transport::key_fingerprint(
        &SigningKey::from_bytes(&[semente; 32])
            .verifying_key()
            .to_bytes(),
    )
}

/// Um aperto de mão inteiro, com a chave desta semente e este segredo.
async fn tentar(endereco: SocketAddr, semente: u8, segredo: Option<&str>) -> Result<Desfecho> {
    let comum::Ligado {
        ponta: _ponta,
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
            join_secret: segredo.map(str::to_owned),
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
                            .sign(&comum::prova_amarrada(&conexao, &nonce))
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

async fn desfecho(endereco: SocketAddr, semente: u8, segredo: Option<&str>) -> Desfecho {
    tokio::time::timeout(Duration::from_secs(20), tentar(endereco, semente, segredo))
        .await
        .expect("a tentativa teve resposta dentro do prazo")
        .expect("o aperto de mão terminou")
}

/// Nenhum teste daqui fala com o ponto de encontro de produção. Ver
/// `janela_do_comandante.rs`.
fn sem_degrau_4() {
    static UMA_VEZ: std::sync::Once = std::sync::Once::new();
    UMA_VEZ.call_once(|| std::env::set_var(seele_server::alcance::encontro::VARIAVEL, "nao"));
}

/// Um servidor com senha e **sem a portaria**, como o app o sobe quando quem
/// hospeda escolhe «com senha»: o dono declarado e admitido antes de aceitar.
async fn com_senha_e_sem_portaria() -> Hospedagem {
    sem_degrau_4();
    Hospedagem::iniciar(0, Location::Memory, "Casa", None, |persistence| {
        seele_server::admissao::definir_senha(persistence, Some(SENHA))?;
        seele_server::portaria::ligar(persistence, false)?;
        seele_server::cargos::declarar_proprietario(persistence, &impressao(DONO))?;
        seele_server::portaria::admitir_o_dono(persistence, &impressao(DONO))?;
        // Alguém que a portaria admitiu quando estava ligada: a linha fica, e
        // com a portaria desligada ela não dispensa a senha.
        seele_server::portaria::admitir_o_dono(persistence, &impressao(ADMITIDA))
    })
    .await
    .expect("subir")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn quem_hospeda_entra_sem_a_senha_num_servidor_com_senha_e_sem_portaria() {
    let hospedagem = com_senha_e_sem_portaria().await;
    let endereco = SocketAddr::from(([127, 0, 0, 1], hospedagem.endereco().port()));

    assert_eq!(
        desfecho(endereco, DONO, None).await,
        Desfecho::Entrou,
        "quem hospeda foi recusado na própria casa por não digitar a senha dela"
    );

    hospedagem.encerrar().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn so_o_dono_e_dispensado_da_senha() {
    let hospedagem = com_senha_e_sem_portaria().await;
    let endereco = SocketAddr::from(([127, 0, 0, 1], hospedagem.endereco().port()));

    assert_eq!(
        desfecho(endereco, ESTRANHO, None).await,
        Desfecho::Barrado(DisconnectReason::CredentialRejected),
        "um estranho sem a senha passou"
    );
    assert_eq!(
        desfecho(endereco, ESTRANHO, Some("terceiro impact0")).await,
        Desfecho::Barrado(DisconnectReason::CredentialRejected),
        "a senha errada passou"
    );
    assert_eq!(
        desfecho(endereco, ADMITIDA, None).await,
        Desfecho::Barrado(DisconnectReason::CredentialRejected),
        "uma admissão antiga dispensou a senha com a portaria desligada: o perdão é só de \
         quem hospeda"
    );
    // E com a senha, entra direto: sem portaria, ninguém fica esperando.
    assert_eq!(
        desfecho(endereco, ESTRANHO, Some(SENHA)).await,
        Desfecho::Entrou,
        "quem tem a senha não entrou direto"
    );

    hospedagem.encerrar().await;
}
