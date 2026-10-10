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
//!     <endereco> <sala> <segundos> [--calado] [--apelido <nome>] [--mensagens <n>]
//!     [--calados <n>]
//! ```
//!
//! **As opções da campanha da API 6** (tarefa 11 do plano da F3), sem mudar o
//! uso de antes: `--calado` entra e não fala (enche a sala e a lista no
//! cenário das capas); `--apelido` dá um nome por participante (o servidor
//! recusa o mesmo apelido com outra chave); `--mensagens <n>` posta `n`
//! mensagens no canal 1 ao entrar (a região `conversa.mensagem` precisa de
//! mensagens); a sala 0 é «não entrar em sala».
//!
//! **`--calados <n>`** (a `carga-cartoes`, 10/10): `n` participantes calados num
//! processo só, `<apelido>-000` em diante. Num processo por participante, os
//! 250 da lista pesariam na máquina medida. Eles entram no ritmo da portaria
//! (`taxa.rs`: 30 apertos de rajada por endereço, repostos a 30 por minuto; todos
//! saem de 127.0.0.1, e o app também): 28 de uma vez, e depois um a cada 2,05 s,
//! tentando de novo quando recusados. Cada um drena o que chega, e uma queda vai
//! ao stderr com o motivo. Prontos, eles dizem `M3 calados prontos=<n>`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::cast_precision_loss,
         clippy::cast_possible_truncation, clippy::indexing_slicing)]

use anyhow::{Context, Result};
use ed25519_dalek::SigningKey;
use seele_core::{client::ConnectError, Client, MemoryPinStore};
use seele_proto::ids::{ChannelId, ClientMessageId, VoiceRoomId};
use seele_proto::MediaHeader;
use std::{net::SocketAddr, sync::Arc, time::{Duration, Instant}};

const QUADRO: usize = 960; // 20 ms a 48 kHz, mono

async fn entrar(endereco: SocketAddr, chave: &SigningKey, apelido: &str) -> Result<Client> {
    let pins = Arc::new(MemoryPinStore::new());
    match Client::connect(endereco, "localhost", &endereco.to_string(), apelido, chave,
                          pins.clone(), None, None).await {
        Ok(cliente) => Ok(cliente),
        Err(ConnectError::ModsNaoAceitos { conjunto, .. }) => Ok(Client::connect(
            endereco, "localhost", &endereco.to_string(), apelido, chave, pins,
            None, Some(&conjunto)).await?),
        Err(outro) => Err(outro.into()),
    }
}

/// Os apertos que a portaria aceita de um endereço numa rajada (30), menos os
/// do app, que sai do mesmo 127.0.0.1; e o ritmo em que ela os repõe (30 por
/// minuto), com folga.
const RAJADA: usize = 28;
const RITMO: Duration = Duration::from_millis(2050);

/// `n` calados num processo só: entram no ritmo da portaria, drenam o que chega
/// até o prazo, e dizem quando estão todos dentro.
async fn muitos_calados(endereco: SocketAddr, n: usize, prefixo: &str, segundos: u64) -> Result<()> {
    let comeco = Instant::now();
    let fim = comeco + Duration::from_secs(segundos);
    let mut tarefas = Vec::with_capacity(n);
    for i in 0..n {
        if i >= RAJADA {
            tokio::time::sleep(RITMO).await;
        }
        let apelido = format!("{prefixo}-{i:03}");
        let chave = SigningKey::from_bytes(&rand::random::<[u8; 32]>());
        let mut tentativas = 0;
        let mut cliente = loop {
            match entrar(endereco, &chave, &apelido).await {
                Ok(cliente) => break cliente,
                Err(erro) if tentativas < 30 => {
                    tentativas += 1;
                    eprintln!("M3 calado {apelido} recusado ({erro}); de novo em {} ms", RITMO.as_millis());
                    tokio::time::sleep(RITMO).await;
                }
                Err(erro) => return Err(erro.context(format!("{apelido} não entrou"))),
            }
        };
        tarefas.push(tokio::spawn(async move {
            while Instant::now() < fim {
                if let Ok(Err(erro)) = tokio::time::timeout(Duration::from_millis(500), cliente.next_event()).await {
                    eprintln!("M3 calado {apelido} caiu: {erro}");
                    return;
                }
                while let Ok(Ok(_)) = tokio::time::timeout(Duration::ZERO, cliente.next_media()).await {}
            }
        }));
    }
    println!("M3 calados prontos={n} em {:.0} s", comeco.elapsed().as_secs_f64());
    for tarefa in tarefas {
        let _ = tarefa.await;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let endereco: SocketAddr = args.next().context("uso: <endereco> <sala> <segundos>")?.parse()?;
    let sala: u32 = args.next().context("falta a sala")?.parse()?;
    let segundos: u64 = args.next().context("faltam os segundos")?.parse()?;
    let (mut calado, mut apelido, mut mensagens) = (false, "falante-m3".to_owned(), 0_u64);
    let mut calados = 0_usize;
    while let Some(opcao) = args.next() {
        match opcao.as_str() {
            "--calado" => calado = true,
            "--apelido" => apelido = args.next().context("falta o apelido")?,
            "--mensagens" => mensagens = args.next().context("falta o número")?.parse()?,
            "--calados" => calados = args.next().context("falta o número")?.parse()?,
            outra => anyhow::bail!("opção desconhecida: {outra}"),
        }
    }
    if calados > 0 {
        return muitos_calados(endereco, calados, &apelido, segundos).await;
    }
    let chave = SigningKey::from_bytes(&rand::random::<[u8; 32]>());
    let mut cliente = entrar(endereco, &chave, &apelido).await?;
    for n in 0..mensagens {
        cliente.send_message(ChannelId(1), &format!("mensagem {n} da carga M3"), ClientMessageId(n + 1)).await?;
    }
    if sala != 0 {
        cliente.enter_voice_room(VoiceRoomId(sala), None).await?;
    }
    if calado {
        // Presente, e quieto: lê o que chega para o servidor não o achar morto.
        println!("M3 calado apelido={apelido} sala={sala}");
        let comeco = Instant::now();
        while comeco.elapsed() < Duration::from_secs(segundos) {
            while let Ok(Ok(_)) = tokio::time::timeout(Duration::from_millis(200), cliente.next_event()).await {}
            while let Ok(Ok(_)) = tokio::time::timeout(Duration::ZERO, cliente.next_media()).await {}
        }
        return Ok(());
    }
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
            // O número da voz da v0.15.1, onde ele é o do protocolo. Na 0.16 a
            // voz tem número próprio (`MEDIA_HEADER_VERSION`, desde o
            // `f3aaf9ce`), e a campanha da API 6 o troca na cópia
            // (`remendar-api6.py`, item 4): com o daqui, quem ouve na 0.16
            // jogava fora todo datagrama.
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
