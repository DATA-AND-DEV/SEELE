//! **A senha do servidor é conferida sem o banco na mão e fora do executor.**
//!
//! O defeito: no aperto de mão, antes de qualquer autenticação, o Argon2 da
//! senha do servidor rodava com o `Mutex` da persistência fechado e numa
//! thread do Tokio. Em servidor com senha, cada batida com um palpite parava o
//! banco de todo mundo, e uma thread de rede junto, pelo tempo de um Argon2.
//!
//! Os dois testes medem contra a própria máquina: o custo de um Argon2 aqui é
//! o de `definir_senha`, e o que se afirma é que nem o banco fica fechado nem
//! o executor fica parado por um pedaço comparável a ele.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use ed25519_dalek::SigningKey;
use seele_proto::control::{ClientMessage, ServerMessage};
use seele_server::persistence::Location;
use seele_server::{frame, Daemon, ServerConfig};

mod comum;

/// Um servidor com senha, e quanto custa um Argon2 nesta máquina.
async fn montar() -> Result<(SocketAddr, Arc<Daemon>, Duration)> {
    let config = ServerConfig {
        name: "Casa".into(),
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        database: Location::Memory,
        ..ServerConfig::default()
    };
    let daemon = Arc::new(Daemon::bind(config).await?);
    let um_argon2 = {
        let mut db = daemon.server().persistence.lock().await;
        let antes = Instant::now();
        seele_server::admissao::definir_senha(&mut db, Some("terceiro impacto"))?;
        antes.elapsed()
    };
    let endereco = daemon.local_addr()?;
    let atendendo = Arc::clone(&daemon);
    tokio::spawn(async move {
        let _ = atendendo.run().await;
    });
    Ok((endereco, daemon, um_argon2))
}

/// Bate à porta com a senha errada e espera o `Challenge`: a admissão já foi
/// avaliada quando ele chega (a recusa é adiada para depois da assinatura).
async fn bater_com_a_senha_errada(endereco: SocketAddr) -> Result<comum::Ligado> {
    let mut ligado = comum::ligar(endereco).await?;
    let chave = SigningKey::from_bytes(&[7; 32]);
    frame::write(
        &mut ligado.envio,
        &ClientMessage::Hello {
            faixa: seele_proto::version::FAIXA,
            client: "par-cru".into(),
            nickname: "pessoa".into(),
            public_key: chave.verifying_key().to_bytes().to_vec(),
            join_secret: Some("palpite errado".into()),
            material_e2ee: None,
            apis_de_mod: seele_proto::mods::APIS_ACEITAS.to_vec(),
        },
    )
    .await?;
    let ServerMessage::Challenge { .. } = frame::read::<ServerMessage>(&mut ligado.recebe).await?
    else {
        anyhow::bail!("sem Challenge");
    };
    Ok(ligado)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn o_banco_fica_livre_enquanto_a_senha_do_servidor_e_conferida() -> Result<()> {
    let (endereco, daemon, um_argon2) = montar().await?;
    let parar = Arc::new(AtomicBool::new(false));
    // Uma thread de fora do executor olha o `Mutex` da persistência o tempo
    // todo e guarda o maior trecho em que o achou fechado.
    let vigia = {
        let (daemon, parar) = (Arc::clone(&daemon), Arc::clone(&parar));
        std::thread::spawn(move || {
            let mut maior = Duration::ZERO;
            let mut fechado_desde: Option<Instant> = None;
            while !parar.load(Ordering::Relaxed) {
                if daemon.server().persistence.try_lock().is_ok() {
                    fechado_desde = None;
                } else {
                    let desde = *fechado_desde.get_or_insert_with(Instant::now);
                    maior = maior.max(desde.elapsed());
                }
                std::thread::sleep(Duration::from_micros(200));
            }
            maior
        })
    };
    let _ligado = bater_com_a_senha_errada(endereco).await?;
    parar.store(true, Ordering::Relaxed);
    let maior = vigia.join().expect("a vigia");
    assert!(
        maior < um_argon2 / 2,
        "o banco ficou fechado por {maior:?} durante o aperto de mão, e um Argon2 aqui custa \
         {um_argon2:?}: a senha foi conferida com o banco na mão"
    );
    daemon.shutdown();
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn o_executor_nao_para_enquanto_a_senha_do_servidor_e_conferida() -> Result<()> {
    let (endereco, daemon, um_argon2) = montar().await?;
    let parar = Arc::new(AtomicBool::new(false));
    // Com uma thread só, um Argon2 rodando no executor é um buraco do tamanho
    // dele entre duas voltas desta tarefa.
    let relogio = {
        let parar = Arc::clone(&parar);
        tokio::spawn(async move {
            let mut maior = Duration::ZERO;
            let mut ultima = Instant::now();
            while !parar.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_millis(1)).await;
                maior = maior.max(ultima.elapsed());
                ultima = Instant::now();
            }
            maior
        })
    };
    let _ligado = bater_com_a_senha_errada(endereco).await?;
    parar.store(true, Ordering::Relaxed);
    let maior = relogio.await?;
    assert!(
        maior < um_argon2 / 2,
        "o executor ficou {maior:?} sem dar a vez durante o aperto de mão, e um Argon2 aqui \
         custa {um_argon2:?}: a senha foi conferida numa thread do Tokio"
    );
    daemon.shutdown();
    Ok(())
}
