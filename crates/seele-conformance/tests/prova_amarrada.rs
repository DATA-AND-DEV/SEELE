//! **A prova de identidade vale só para o servidor e para a conexão que o
//! cliente conferiu.**
//!
//! Até a 0.15, o cliente assinava o nonce cru que o servidor mandasse, e o
//! servidor conferia a assinatura sobre esse nonce. Uma prova não dizia **para
//! quem** ela era (análise de 22/09, §3.1, o S2). No Servidor 1.0, a mensagem
//! passou a ser o rótulo, a impressão do certificado do servidor e o nonce.
//!
//! **Desde o Servidor 2.0 (0.17, D5 do contrato da fase 2)**, a mensagem é
//! `prova_de_cliente_2`: o rótulo `SEELE/2 prova-de-cliente\0`, a impressão do
//! servidor, **a impressão do certificado DTLS de quem liga nesta conexão**, e
//! o nonce. O servidor confere a impressão DTLS com a que o enlace viu, e não
//! com algo que o cliente diga. Este arquivo prova os dois lados, cada um por
//! conta própria:
//!
//! - **o servidor** só aceita a prova amarrada à impressão **dele** e ao DTLS
//!   **desta** conexão. Recusa, com o mesmo `CredentialRejected` uniforme, a
//!   prova amarrada a outra impressão de servidor, a amarrada a outro DTLS (uma
//!   prova repassada por quem está no meio), a da linha 1 (o rótulo `SEELE/1`)
//!   e o nonce cru da 0.15;
//! - **o cliente de verdade** (`seele_core::Client::connect`) assina a impressão
//!   do certificado DTLS que o enlace daquela conexão conferiu e o DTLS dele, e
//!   nenhuma outra combinação.
//!
//! Até a 0.16 o par cru era um QUIC com um verificador que aceitava qualquer
//! certificado; desde a 0.17 é o `par_cru` do enlace, com `Pino::Qualquer`.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing)]

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use ed25519_dalek::{Signer, SigningKey};
use seele_proto::assinatura::{prova_de_cliente, prova_de_cliente_2};
use seele_proto::control::{encode, ClientMessage, DisconnectReason, ServerMessage};
use seele_server::persistence::Location;
use seele_server::{Daemon, ServerConfig};

mod par_cru;
mod vaga;

async fn servidor() -> Result<(SocketAddr, Arc<Daemon>)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        ..ServerConfig::default()
    };
    let daemon = Arc::new(Daemon::bind(config).await?);
    let endereco = daemon.local_addr()?;
    let atendendo = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = atendendo.run().await;
    });
    Ok((endereco, daemon))
}

/// O que o servidor respondeu à prova: `Session` ou o quadro que veio no
/// lugar dele.
#[derive(Debug)]
enum Resposta {
    Entrou,
    Recusou(ServerMessage),
}

/// O que a assinatura cobre, a partir do que esta conexão viu de verdade: a
/// impressão do servidor, a do DTLS desta ponta e o nonce.
struct Vista {
    servidor: [u8; 32],
    dtls_local: [u8; 32],
    nonce: [u8; 32],
}

/// Faz o aperto de mão à mão e assina **a mensagem que `assinar` montar** a
/// partir do que a conexão viu de verdade.
async fn bater_com(
    endereco: SocketAddr,
    semente: u8,
    assinar: impl FnOnce(&Vista) -> Vec<u8>,
) -> Result<Resposta> {
    let (ponta, conexao) = par_cru::discar(endereco).await;
    let (envio, recebe) = conexao.controle().expect("o ctl, uma vez");
    let mut par = par_cru::Par {
        conexao,
        envio,
        recebe,
        ponta,
    };
    let chave = SigningKey::from_bytes(&[semente; 32]);
    par.mandar(&par_cru::hello(
        &chave,
        &format!("pessoa{semente:03}"),
        None,
    ))
    .await?;

    let primeiro = par.proximo().await?;
    let ServerMessage::Challenge { nonce } = primeiro else {
        return Ok(Resposta::Recusou(primeiro));
    };
    let vista = Vista {
        servidor: par_cru::impressao_da_prova(&par.conexao),
        dtls_local: par.conexao.impressao_dtls_local(),
        nonce: nonce.as_slice().try_into()?,
    };
    par.mandar(&ClientMessage::Response {
        proof: chave.sign(&assinar(&vista)).to_bytes().to_vec(),
    })
    .await?;

    let resposta = par.proximo().await?;
    Ok(match resposta {
        ServerMessage::Session { .. } => Resposta::Entrou,
        outro => Resposta::Recusou(outro),
    })
}

fn recusada(resposta: &Resposta) -> bool {
    matches!(
        resposta,
        Resposta::Recusou(ServerMessage::Disconnecting {
            reason: DisconnectReason::CredentialRejected
        })
    )
}

/// O caminho certo: a prova D5 amarrada a este servidor e a este DTLS entra.
#[tokio::test(flavor = "multi_thread")]
async fn a_prova_amarrada_a_este_servidor_e_a_este_dtls_entra() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;

    let resposta = bater_com(endereco, 61, |v| {
        prova_de_cliente_2(&v.servidor, &v.dtls_local, &v.nonce)
    })
    .await?;
    assert!(
        matches!(resposta, Resposta::Entrou),
        "a prova amarrada a este servidor e a este DTLS foi recusada: {resposta:?}"
    );

    daemon.shutdown();
    Ok(())
}

/// **O lado do servidor:** uma prova feita para outro servidor — amarrada a
/// outra impressão — é recusada, com a mesma razão uniforme de toda assinatura
/// que não confere (`specs/08-seguranca.md`).
#[tokio::test(flavor = "multi_thread")]
async fn uma_prova_amarrada_a_outra_impressao_de_servidor_e_recusada() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;

    let resposta = bater_com(endereco, 62, |v| {
        let mut outra = v.servidor;
        outra[0] ^= 0xFF;
        prova_de_cliente_2(&outra, &v.dtls_local, &v.nonce)
    })
    .await?;
    assert!(
        recusada(&resposta),
        "uma prova amarrada a outra impressão de servidor não foi recusada: {resposta:?}"
    );

    daemon.shutdown();
    Ok(())
}

/// **A D5:** uma prova amarrada a **outro** certificado DTLS de cliente — a de
/// uma conexão que não é esta, como a que alguém no meio repassaria com o
/// DTLS dele — é recusada. O servidor confere contra o DTLS que o enlace viu.
#[tokio::test(flavor = "multi_thread")]
async fn uma_prova_amarrada_a_outro_dtls_de_cliente_e_recusada() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;

    let resposta = bater_com(endereco, 65, |v| {
        let mut outro = v.dtls_local;
        outro[31] ^= 0x01;
        prova_de_cliente_2(&v.servidor, &outro, &v.nonce)
    })
    .await?;
    assert!(
        recusada(&resposta),
        "uma prova amarrada a outro DTLS de cliente entrou: {resposta:?}"
    );

    daemon.shutdown();
    Ok(())
}

/// **A prova da linha 1 não entra na linha 2:** o rótulo `SEELE/1`, com a
/// impressão certa do servidor e o nonce certo, sem o DTLS do cliente.
#[tokio::test(flavor = "multi_thread")]
async fn a_prova_da_linha_1_e_recusada() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;

    let resposta = bater_com(endereco, 66, |v| prova_de_cliente(&v.servidor, &v.nonce)).await?;
    assert!(
        recusada(&resposta),
        "a prova da linha 1 (SEELE/1) entrou num servidor da linha 2: {resposta:?}"
    );

    daemon.shutdown();
    Ok(())
}

/// E o nonce cru, como a 0.15 assinava, também é recusado: sem rótulo e sem
/// impressão, a assinatura não confere.
#[tokio::test(flavor = "multi_thread")]
async fn a_prova_da_0_15_sobre_o_nonce_cru_e_recusada() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;

    let resposta = bater_com(endereco, 63, |v| v.nonce.to_vec()).await?;
    assert!(
        recusada(&resposta),
        "a prova sobre o nonce cru ainda entra: {resposta:?}"
    );

    daemon.shutdown();
    Ok(())
}

/// **Uma chave de ordem pequena é recusada no registro**, antes do desafio.
///
/// Uma chave dessas não prova posse de nada: a conferência comum do
/// `ed25519-dalek` aceita com ela assinaturas que não exigem chave privada
/// nenhuma. O servidor a recusa ao ler o `Hello`, com a mesma razão uniforme de
/// toda credencial que não serve, e confere a prova com `verify_strict`. Achado
/// da F2, pedido pela orquestradora em 08/10/2026.
#[tokio::test(flavor = "multi_thread")]
async fn uma_chave_de_ordem_pequena_e_recusada_antes_do_desafio() -> Result<()> {
    let _vaga = vaga::minha();
    let (endereco, daemon) = servidor().await?;

    // O elemento neutro da curva, comprimido: y = 1, x positivo.
    let mut identidade = [0u8; 32];
    identidade[0] = 1;
    assert!(
        ed25519_dalek::VerifyingKey::from_bytes(&identidade)?.is_weak(),
        "a chave do teste deixou de ser de ordem pequena"
    );

    let (ponta, conexao) = par_cru::discar(endereco).await;
    let (envio, recebe) = conexao.controle().expect("o ctl, uma vez");
    let mut par = par_cru::Par {
        conexao,
        envio,
        recebe,
        ponta,
    };
    par.mandar(&ClientMessage::Hello {
        faixa: seele_proto::FAIXA,
        client: "par-cru".into(),
        nickname: "pessoa064".into(),
        public_key: identidade.to_vec(),
        join_secret: None,
        material_e2ee: None,
        apis_de_mod: seele_proto::mods::APIS_ACEITAS.to_vec(),
    })
    .await?;
    let resposta = par.proximo().await?;
    assert_eq!(
        resposta,
        ServerMessage::Disconnecting {
            reason: DisconnectReason::CredentialRejected
        },
        "uma chave de ordem pequena chegou ao desafio"
    );

    daemon.shutdown();
    Ok(())
}

/// **O lado do cliente:** o `Client::connect` de verdade assina a impressão do
/// certificado DTLS que o enlace daquela conexão conferiu, com o DTLS dele, e
/// nenhuma outra combinação.
///
/// Quem confere é um anfitrião cru (`par_cru::anfitriao`), com um certificado
/// efêmero próprio: ele lê o `Hello`, manda um desafio conhecido e confere a
/// assinatura que volta, com a chave do `Hello`, contra a mensagem montada com
/// **o certificado que ele mesmo apresentou** e **o DTLS que ele viu do
/// cliente**. O conteúdo assinado não precisa de gancho no cliente: ele sai
/// inteiro na assinatura, e quem tem a chave pública o confere.
#[tokio::test(flavor = "multi_thread")]
async fn o_cliente_assina_a_impressao_que_o_dtls_conferiu_e_o_dtls_dele() -> Result<()> {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};

    let _vaga = vaga::minha();
    let ponta = par_cru::anfitriao().await;
    let endereco = ponta.local_addr();

    let nonce = [0x5C_u8; 32];
    let conferindo = tokio::spawn(async move {
        let (conexao, mut envio, mut recebe) = par_cru::primeira_chegada(&ponta).await?;
        let apresentada = conexao.impressao_dtls_local();
        let do_cliente = conexao.impressao_dtls_do_par();
        let ClientMessage::Hello { public_key, .. } =
            seele_server::frame::read::<ClientMessage>(&mut recebe).await?
        else {
            anyhow::bail!("o primeiro quadro não foi Hello");
        };
        seele_server::frame::write(
            &mut envio,
            &ServerMessage::Challenge {
                nonce: nonce.to_vec(),
            },
        )
        .await?;
        let ClientMessage::Response { proof } =
            seele_server::frame::read::<ClientMessage>(&mut recebe).await?
        else {
            anyhow::bail!("o segundo quadro não foi Response");
        };
        // Recusa, para o cliente sair do aperto de mão; o que importa já
        // chegou.
        let _ = par_cru::escrever(
            &mut envio,
            &seele_proto::quadro::enquadrar(&encode(&ServerMessage::Disconnecting {
                reason: DisconnectReason::CredentialRejected,
            })?)?,
        )
        .await;
        let _ = envio.finish();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(1), envio.entregue()).await;

        let chave = VerifyingKey::from_bytes(&public_key.as_slice().try_into()?)?;
        let assinatura = Signature::from_slice(&proof)?;
        let confere = |mensagem: &[u8]| chave.verify(mensagem, &assinatura).is_ok();
        let resultado = [
            confere(&prova_de_cliente_2(&apresentada, &do_cliente, &nonce)),
            confere(&prova_de_cliente_2(&[0; 32], &do_cliente, &nonce)),
            confere(&prova_de_cliente_2(&apresentada, &[0; 32], &nonce)),
            confere(&prova_de_cliente(&apresentada, &nonce)),
            confere(&nonce),
        ];
        drop(conexao);
        drop(ponta);
        Ok(resultado)
    });

    let resultado = seele_core::Client::connect(
        endereco,
        "localhost",
        &endereco.to_string(),
        "marcela",
        &SigningKey::from_bytes(&[64; 32]),
        Arc::new(seele_core::MemoryPinStore::new()),
        None,
        None,
    )
    .await;
    assert!(
        resultado.is_err(),
        "o servidor de teste recusa sempre, e o cliente entrou"
    );

    let [certa, outro_servidor, outro_dtls, da_linha_1, nonce_cru] = conferindo.await??;
    assert!(
        certa,
        "a assinatura do cliente não confere com o certificado que o DTLS apresentou e o \
         DTLS dele: ele assinou outra coisa"
    );
    assert!(
        !outro_servidor,
        "a assinatura confere com uma impressão de servidor que ninguém apresentou"
    );
    assert!(
        !outro_dtls,
        "a assinatura confere sem o DTLS do cliente: a prova não está amarrada à conexão"
    );
    assert!(!da_linha_1, "o cliente voltou a assinar a prova da linha 1");
    assert!(
        !nonce_cru,
        "o cliente voltou a assinar o nonce cru, como a 0.15"
    );
    Ok(())
}
