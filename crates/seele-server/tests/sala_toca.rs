//! A sala pessoal segura a conexão enquanto toca: o aperto de mão em três fases.
//!
//! Spec de 23/09, «O aperto de mão em três fases», e o ADR 0059:
//!
//! 1. `Hello` → `Challenge` → `Response`, em 10 s, com o S2;
//! 2. **o toque**: o servidor manda `Tocando { prazo_s }` e espera a decisão de
//!    quem recebe, o prazo ou a conexão fechar, **sem segurar a trava do banco**;
//! 3. a conta e a sessão, em 10 s.
//!
//! Um servidor que não é sala não toca: ele não tem recepção.
//!
//! # Por que um par cru, e não um `Client`
//!
//! O mesmo motivo de `janela_do_comandante.rs`: `seele-core` não pode ser
//! dependência daqui (ADR 0002). O cliente que lê o `Tocando` é a tarefa 6e.

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
use seele_proto::codigo::Codigo;
use seele_proto::control::{ClientMessage, DisconnectReason, ServerMessage};
use seele_server::campainha::Desfecho;
use seele_server::frame;
use seele_server::recepcao::{Aviso, Sala};
use seele_server::{Daemon, ServerConfig};

mod comum;

/// A semente de quem recebe.
const DONO: u8 = 1;

fn chave(semente: u8) -> SigningKey {
    SigningKey::from_bytes(&[semente; 32])
}

fn publica(semente: u8) -> [u8; 32] {
    chave(semente).verifying_key().to_bytes()
}

/// Uma sala de `DONO`, em memória, com o prazo do toque dado.
async fn sala(prazo: Duration) -> Arc<Daemon> {
    let config = ServerConfig {
        identidade: Some(seele_server::tls::Identity::da_semente(&[DONO; 32]).unwrap()),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        voice_room_limit: 6,
        sala: Some(Sala {
            dono: publica(DONO),
            prazo_do_toque: prazo,
        }),
        ..ServerConfig::default()
    };
    let servidor = Arc::new(Daemon::bind(config).await.expect("subir a sala"));
    let rodando = Arc::clone(&servidor);
    tokio::spawn(async move { rodando.run().await });
    servidor
}

/// Uma ponta de quem liga, até a prova: o que vier depois do `Response` é o
/// assunto de cada teste.
struct Ligacao {
    conexao: seele_enlace::Conexao,
    envio: seele_enlace::Envio,
    recebe: seele_enlace::Recebe,
    /// A ponta é dona do socket, e sem ela viva a conexão cai.
    _ponta: (seele_enlace::Ponta, seele_enlace::Conexao),
}

impl Ligacao {
    async fn proximo(&mut self) -> Result<ServerMessage> {
        frame::read::<ServerMessage>(&mut self.recebe).await
    }
}

async fn ligar(endereco: SocketAddr, semente: u8) -> Result<Ligacao> {
    let comum::Ligado {
        ponta,
        conexao,
        mut envio,
        mut recebe,
    } = comum::ligar(endereco).await?;
    let pessoa = chave(semente);
    frame::write(
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
    .await?;
    let ServerMessage::Challenge { nonce } = frame::read::<ServerMessage>(&mut recebe).await?
    else {
        anyhow::bail!("a sala não mandou Challenge");
    };
    // Na sala, a impressão que a prova amarra é a da chave dela (D5), a que
    // a apresentação `0x03` traz nos 32 primeiros bytes.
    // Um servidor que não é sala se apresenta vazio, e a impressão é a do DTLS.
    let impressao = match conexao.apresentacao().get(..32) {
        Some(chave_da_sala) => seele_proto::transport::certificate_digest(chave_da_sala),
        None => conexao.impressao_dtls_do_par(),
    };
    frame::write(
        &mut envio,
        &ClientMessage::Response {
            proof: pessoa
                .sign(&comum::prova_para(&impressao, &conexao, &nonce))
                .to_bytes()
                .to_vec(),
        },
    )
    .await?;
    Ok(Ligacao {
        conexao: conexao.clone(),
        envio,
        recebe,
        _ponta: (ponta, conexao),
    })
}

/// Espera o `Tocou` de `semente` no canal do app, e devolve o toque.
async fn tocou(
    avisos: &mut tokio::sync::broadcast::Receiver<Aviso>,
    semente: u8,
) -> seele_server::campainha::IdDoToque {
    loop {
        let aviso = tokio::time::timeout(Duration::from_secs(5), avisos.recv())
            .await
            .expect("o app não ficou sabendo que tocava")
            .unwrap();
        if let Aviso::Tocou { toque, codigo, .. } = aviso {
            if codigo == Codigo::da_chave(&publica(semente)) {
                return toque;
            }
        }
    }
}

#[tokio::test]
async fn quem_liga_recebe_tocando_e_entra_quando_quem_recebe_atende() {
    let servidor = sala(Duration::from_secs(45)).await;
    let recepcao = servidor
        .server()
        .recepcao
        .clone()
        .expect("a sala tem recepção");
    let mut avisos = recepcao.assinar();
    let mut ligacao = ligar(servidor.local_addr().unwrap(), 2).await.unwrap();

    assert!(
        matches!(
            ligacao.proximo().await.unwrap(),
            ServerMessage::Tocando { prazo_s: 45 }
        ),
        "quem liga não soube que estava tocando"
    );
    let toque = tocou(&mut avisos, 2).await;
    assert!(recepcao.atender(toque));
    assert!(
        matches!(
            ligacao.proximo().await.unwrap(),
            ServerMessage::Session { .. }
        ),
        "quem foi atendido não entrou"
    );
}

#[tokio::test]
async fn recusar_manda_chamada_recusada() {
    let servidor = sala(Duration::from_secs(45)).await;
    let recepcao = servidor.server().recepcao.clone().unwrap();
    let mut avisos = recepcao.assinar();
    let mut ligacao = ligar(servidor.local_addr().unwrap(), 2).await.unwrap();
    assert!(matches!(
        ligacao.proximo().await.unwrap(),
        ServerMessage::Tocando { .. }
    ));
    assert!(recepcao.recusar(tocou(&mut avisos, 2).await));
    assert!(
        matches!(
            ligacao.proximo().await.unwrap(),
            ServerMessage::Disconnecting {
                reason: DisconnectReason::ChamadaRecusada
            }
        ),
        "a recusa não chegou a quem ligou como ChamadaRecusada"
    );
}

#[tokio::test]
async fn o_prazo_vencido_manda_ninguem_atendeu() {
    let servidor = sala(Duration::from_secs(1)).await;
    let mut ligacao = ligar(servidor.local_addr().unwrap(), 2).await.unwrap();
    assert!(matches!(
        ligacao.proximo().await.unwrap(),
        ServerMessage::Tocando { prazo_s: 1 }
    ));
    let veio = tokio::time::timeout(Duration::from_secs(5), ligacao.proximo())
        .await
        .expect("o prazo venceu e a sala continuou segurando a conexão")
        .unwrap();
    assert!(
        matches!(
            veio,
            ServerMessage::Disconnecting {
                reason: DisconnectReason::NinguemAtendeu
            }
        ),
        "veio {veio:?}"
    );
}

#[tokio::test]
async fn fechar_a_conexao_enquanto_toca_para_o_toque_no_app() {
    let servidor = sala(Duration::from_secs(45)).await;
    let recepcao = servidor.server().recepcao.clone().unwrap();
    let mut avisos = recepcao.assinar();
    let mut ligacao = ligar(servidor.local_addr().unwrap(), 2).await.unwrap();
    assert!(matches!(
        ligacao.proximo().await.unwrap(),
        ServerMessage::Tocando { .. }
    ));
    let toque = tocou(&mut avisos, 2).await;
    ligacao.conexao.close(0_u32, b"desisti");

    loop {
        let aviso = tokio::time::timeout(Duration::from_secs(5), avisos.recv())
            .await
            .expect("a faixa continuaria tocando para quem já desligou")
            .unwrap();
        if let Aviso::ParouDeTocar {
            toque: parou,
            desfecho,
            ..
        } = aviso
        {
            assert_eq!(parou, toque);
            assert_eq!(desfecho, Desfecho::Cancelada);
            break;
        }
    }
}

#[tokio::test]
async fn quem_recebe_entra_na_propria_sala_sem_tocar() {
    let servidor = sala(Duration::from_secs(45)).await;
    let mut dono = ligar(servidor.local_addr().unwrap(), DONO).await.unwrap();
    assert!(
        matches!(dono.proximo().await.unwrap(), ServerMessage::Session { .. }),
        "quem recebe tocou na própria sala"
    );
}

#[tokio::test]
async fn um_bloqueado_recebe_chamada_recusada_sem_tocar() {
    let servidor = sala(Duration::from_secs(45)).await;
    let recepcao = servidor.server().recepcao.clone().unwrap();
    recepcao.definir_bloqueados(vec![Codigo::da_chave(&publica(2))]);
    let mut ligacao = ligar(servidor.local_addr().unwrap(), 2).await.unwrap();
    assert!(
        matches!(
            ligacao.proximo().await.unwrap(),
            ServerMessage::Disconnecting {
                reason: DisconnectReason::ChamadaRecusada
            }
        ),
        "o bloqueado tocou, ou não recebeu a recusa"
    );
}

#[tokio::test]
async fn a_trava_do_banco_fica_solta_enquanto_toca() {
    let servidor = sala(Duration::from_secs(45)).await;
    let mut ligacao = ligar(servidor.local_addr().unwrap(), 2).await.unwrap();
    assert!(matches!(
        ligacao.proximo().await.unwrap(),
        ServerMessage::Tocando { .. }
    ));
    assert!(
        tokio::time::timeout(Duration::from_secs(1), servidor.server().persistence.lock())
            .await
            .is_ok(),
        "o toque segurou a trava do banco, e a sala inteira parou enquanto tocava"
    );
}

#[tokio::test]
async fn um_toque_atendido_tem_os_dez_segundos_da_conta_contados_depois_do_toque() {
    // Um toque que dura mais que os 10 s do aperto de mão não pode ser cortado
    // por eles: o prazo das fases 1 e 3 não conta o tempo em que tocou.
    let servidor = sala(Duration::from_secs(45)).await;
    let recepcao = servidor.server().recepcao.clone().unwrap();
    let mut avisos = recepcao.assinar();
    let mut ligacao = ligar(servidor.local_addr().unwrap(), 2).await.unwrap();
    assert!(matches!(
        ligacao.proximo().await.unwrap(),
        ServerMessage::Tocando { .. }
    ));
    let toque = tocou(&mut avisos, 2).await;
    tokio::time::sleep(seele_proto::transport::HANDSHAKE_TIMEOUT + Duration::from_secs(1)).await;
    assert!(recepcao.atender(toque));
    let veio = ligacao.proximo().await.unwrap();
    assert!(
        matches!(veio, ServerMessage::Session { .. }),
        "atendido depois de 11 s tocando, e veio {veio:?}"
    );
}

#[tokio::test]
async fn quando_quem_recebe_sai_quem_esta_dentro_recebe_chamada_encerrada() {
    let servidor = sala(Duration::from_secs(45)).await;
    let recepcao = servidor.server().recepcao.clone().unwrap();
    let mut avisos = recepcao.assinar();

    let mut dono = ligar(servidor.local_addr().unwrap(), DONO).await.unwrap();
    assert!(matches!(
        dono.proximo().await.unwrap(),
        ServerMessage::Session { .. }
    ));
    let mut convidado = ligar(servidor.local_addr().unwrap(), 2).await.unwrap();
    assert!(matches!(
        convidado.proximo().await.unwrap(),
        ServerMessage::Tocando { .. }
    ));
    assert!(recepcao.atender(tocou(&mut avisos, 2).await));
    assert!(matches!(
        convidado.proximo().await.unwrap(),
        ServerMessage::Session { .. }
    ));

    let _ = dono.envio.finish();
    dono.conexao.close(0_u32, b"desliguei");

    let fim = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match convidado.proximo().await {
                Ok(ServerMessage::Disconnecting { reason }) => return Some(reason),
                Ok(_) => {}
                Err(_) => return None,
            }
        }
    })
    .await
    .expect("quem estava na ligação ficou sozinho lá dentro");
    assert_eq!(
        fim,
        Some(DisconnectReason::ChamadaEncerrada),
        "quem estava na ligação não recebeu ChamadaEncerrada"
    );
}

#[tokio::test]
async fn um_servidor_que_nao_e_sala_nao_toca() {
    let config = ServerConfig {
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        ..ServerConfig::default()
    };
    let servidor = Arc::new(Daemon::bind(config).await.unwrap());
    assert!(servidor.server().recepcao.is_none());
    let rodando = Arc::clone(&servidor);
    tokio::spawn(async move { rodando.run().await });
    let mut ligacao = ligar(servidor.local_addr().unwrap(), 2).await.unwrap();
    assert!(
        matches!(
            ligacao.proximo().await.unwrap(),
            ServerMessage::Session { .. }
        ),
        "um servidor comum tocou"
    );
}
