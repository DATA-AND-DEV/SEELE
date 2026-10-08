//! F3, R5 do PERFIS — o escopo de servidor de um pedido de MOD, medido.
//!
//! Não é código de produto: compila numa cópia da v0.15.1, como o falante.
//! Entra no servidor, aceita o conjunto de MODs, e manda ao PERFIS um `view` e
//! um `save` com o canal 0 («nenhum canal») e com o canal 1, imprimindo cada
//! resposta. Responde à pergunta: o servidor e o PERFIS servem o pedido sem
//! canal, ou o que falta é do lado de quem pede?
#![allow(clippy::unwrap_used, clippy::expect_used)]

use anyhow::{Context, Result};
use ed25519_dalek::SigningKey;
use seele_core::{client::ConnectError, Client, MemoryPinStore};
use seele_proto::{control::ServerMessage, ids::ChannelId};
use std::{net::SocketAddr, sync::Arc, time::Duration};

async fn entrar(endereco: SocketAddr, chave: &SigningKey) -> Result<Client> {
    let pins = Arc::new(MemoryPinStore::new());
    match Client::connect(endereco, "localhost", &endereco.to_string(), "pedido-m3", chave,
                          pins.clone(), None, None).await {
        Ok(c) => Ok(c),
        Err(ConnectError::ModsNaoAceitos { conjunto, .. }) => Ok(Client::connect(
            endereco, "localhost", &endereco.to_string(), "pedido-m3", chave, pins, None,
            Some(&conjunto)).await?),
        Err(e) => Err(e.into()),
    }
}

async fn resposta(cliente: &mut Client, n: u32) -> Result<String> {
    let mut partes = Vec::new();
    loop {
        let evento = tokio::time::timeout(Duration::from_secs(10), cliente.next_event()).await??;
        if let ServerMessage::ModReply { request, part, total, payload } = evento {
            if request == n {
                partes.push((part, payload));
                if partes.len() as u32 == total {
                    partes.sort();
                    return Ok(partes.into_iter().map(|(_, p)| p).collect());
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let endereco: SocketAddr = std::env::args().nth(1).context("uso: <endereco>")?.parse()?;
    let chave = SigningKey::from_bytes(&rand::random::<[u8; 32]>());
    let mut cliente = entrar(endereco, &chave).await?;
    // A revisão base é a do perfil lido; o primeiro save parte da 0 e o
    // segundo da 1 (o PERFIS recusa um save sobre revisão velha).
    let salvar0 = r##"{"op":"save","revision":0,"profile":{"displayName":"Sem canal","pronouns":"","bio":"","status":"","accent":"#112233","effect":"none"}}"##;
    let salvar1 = r##"{"op":"save","revision":1,"profile":{"displayName":"Canal 1","pronouns":"","bio":"","status":"","accent":"#112233","effect":"none"}}"##;
    let casos = [(1, 0, r#"{"op":"view"}"#), (2, 0, salvar0), (3, 1, r#"{"op":"view"}"#), (4, 1, salvar1)];
    for (n, canal, corpo) in casos {
        cliente.mod_request(n, "seele/perfis".into(), ChannelId(canal), corpo.into()).await?;
        let r = resposta(&mut cliente, n).await;
        let op = if corpo.contains("save") { "save" } else { "view" };
        println!("canal {canal} · {op} → {}", r.map_or_else(|e| format!("ERRO {e}"), |t| t.chars().take(160).collect()));
    }
    Ok(())
}
