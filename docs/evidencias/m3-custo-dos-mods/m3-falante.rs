//! M3 da 0.16 — um falante sintético, para medir o jitter da voz sem
//! microfone.
//!
//! Não é código de produto: o roteiro o compila numa cópia da v0.15.1, como o
//! `m3-servidor.rs`. Ele entra no servidor como uma pessoa, aceita o conjunto
//! de MODs que o servidor exigir (o mesmo sim que a tela de aceite grava),
//! entra na sala de voz pedida e manda um quadro Opus de 20 ms a cada 20 ms —
//! um tom de 440 Hz a −20 dBFS, codificado pelo `VoiceEncoder` do produto.
//!
//! O jitter que importa é o que o app medido calcula ao **receber**: este
//! lado só precisa de um ritmo estável e igual em todos os cenários. A
//! cadência real de envio é impressa a cada 10 s, para que um desvio dele não
//! seja lido como custo dos MODs.
//!
//! ```text
//! cargo run --release -p seele-conformance --example m3-falante -- \
//!     <endereco> <sala> <segundos>
//! ```
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::cast_precision_loss,
         clippy::cast_possible_truncation, clippy::indexing_slicing)]

use anyhow::{Context, Result};
use ed25519_dalek::SigningKey;
use seele_core::{client::ConnectError, Client, MemoryPinStore};
use seele_proto::ids::VoiceRoomId;
use seele_proto::MediaHeader;
use std::{net::SocketAddr, sync::Arc, time::{Duration, Instant}};

const QUADRO: usize = 960; // 20 ms a 48 kHz, mono

async fn entrar(endereco: SocketAddr, chave: &SigningKey) -> Result<Client> {
    let pins = Arc::new(MemoryPinStore::new());
    match Client::connect(endereco, "localhost", &endereco.to_string(), "falante-m3", chave,
                          pins.clone(), None, None).await {
        Ok(cliente) => Ok(cliente),
        Err(ConnectError::ModsNaoAceitos { conjunto, .. }) => Ok(Client::connect(
            endereco, "localhost", &endereco.to_string(), "falante-m3", chave, pins,
            None, Some(&conjunto)).await?),
        Err(outro) => Err(outro.into()),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let endereco: SocketAddr = args.next().context("uso: <endereco> <sala> <segundos>")?.parse()?;
    let sala: u32 = args.next().context("falta a sala")?.parse()?;
    let segundos: u64 = args.next().context("faltam os segundos")?.parse()?;
    let chave = SigningKey::from_bytes(&rand::random::<[u8; 32]>());
    let mut cliente = entrar(endereco, &chave).await?;
    cliente.enter_voice_room(VoiceRoomId(sala), None).await?;
    let ssrc = cliente.session().ssrc.get();
    let mut codificador = seele_audio::codec::VoiceEncoder::new(32_000)?;
    let mut amostras = vec![0.0_f32; QUADRO];
    let mut fase = 0.0_f32;
    let passo = 2.0 * std::f32::consts::PI * 440.0 / 48_000.0;
    let mut saida = vec![0_u8; seele_proto::MAX_DATAGRAM_LEN];
    let mut relogio = tokio::time::interval(Duration::from_millis(20));
    relogio.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Burst);
    let comeco = Instant::now();
    let mut seq: u16 = 0;
    let mut anterior = Instant::now();
    let mut intervalos: Vec<f64> = Vec::new();
    let mut ultimo_relato = Instant::now();
    println!("M3 falante ssrc={ssrc} sala={sala}");
    while comeco.elapsed() < Duration::from_secs(segundos) {
        relogio.tick().await;
        let agora = Instant::now();
        intervalos.push(agora.duration_since(anterior).as_secs_f64() * 1000.0);
        anterior = agora;
        for amostra in &mut amostras {
            *amostra = 0.1 * fase.sin();
            fase = (fase + passo) % (2.0 * std::f32::consts::PI);
        }
        let opus = codificador.encode(&amostras)?;
        seq = seq.wrapping_add(1);
        let cabecalho = MediaHeader {
            version: seele_proto::PROTOCOL_VERSION,
            ssrc,
            seq,
            timestamp: u32::from(seq).wrapping_mul(QUADRO as u32),
        };
        let n = cabecalho.encode_datagram(&opus, &mut saida)?;
        cliente.send_media(saida[..n].to_vec())?;
        while let Ok(Ok(_)) = tokio::time::timeout(Duration::ZERO, cliente.next_event()).await {}
        while let Ok(Ok(_)) = tokio::time::timeout(Duration::ZERO, cliente.next_media()).await {}
        if ultimo_relato.elapsed() >= Duration::from_secs(10) {
            intervalos.sort_by(f64::total_cmp);
            let p = |q: f64| intervalos[((intervalos.len() - 1) as f64 * q) as usize];
            println!("M3 falante cadencia_ms p50={:.2} p99={:.2} max={:.2} n={}",
                     p(0.5), p(0.99), intervalos[intervalos.len() - 1], intervalos.len());
            intervalos.clear();
            ultimo_relato = Instant::now();
        }
    }
    Ok(())
}
