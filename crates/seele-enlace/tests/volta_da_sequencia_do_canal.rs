//! H1 da 0.17: o canal da tela é SCTP ordenado, uma mensagem por pedaço de até
//! 16 KiB, e o número de sequência do stream (16 bits) dá a volta em 65.536
//! mensagens. Este teste manda mais de duas voltas por um canal `u` igual ao da
//! tela e confere que o enlace entrega tudo, em ordem, sem duplicata e sem
//! vazar crédito, e diz onde parou se travar. Mede o enlace em processo, não a
//! rede real, o Windows nem o WebView2.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::panic,
    clippy::print_stdout,
    missing_docs
)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use seele_enlace::teste::{credito_em_aberto, par_em_loopback};

const MENSAGENS: u64 = 140_000;
const VOLTA: u64 = 65_536;
const CABECALHO: usize = 12;
/// Mensagens que podem estar a caminho de uma vez (além do crédito do canal).
const JANELA: u64 = 256;
const SEM_PROGRESSO: Duration = Duration::from_secs(20);

/// Tamanho da mensagem `n` (cabeçalho incluído): 12 B a 16 KiB, determinístico.
fn tamanho(n: u64) -> usize {
    let mistura = n.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40;
    match n % 4 {
        0 if n.is_multiple_of(16) => 16 * 1024,
        0 => CABECALHO + (mistura % 8192) as usize,
        1 => CABECALHO + (mistura % 1024) as usize,
        2 => CABECALHO + (mistura % 4096) as usize,
        _ => CABECALHO + (mistura % (16 * 1024 - CABECALHO as u64 + 1)) as usize,
    }
}

fn montar(n: u64) -> Vec<u8> {
    let total = tamanho(n);
    let mut m = Vec::with_capacity(total);
    m.extend_from_slice(&n.to_be_bytes());
    m.extend_from_slice(&(total as u32).to_be_bytes());
    m.extend((0..total - CABECALHO).map(|j| (n as u8).wrapping_add(j as u8)));
    m
}

/// Prazo da preparação (o par, o open e o accept), que roda antes do vigia.
const PREPARACAO: Duration = Duration::from_secs(30);
/// Prazo do teste inteiro: o normal leva ~20 s.
const TOTAL: Duration = Duration::from_secs(300);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn o_canal_ordenado_passa_de_duas_voltas_da_sequencia_sem_perder_ordem_nem_credito() {
    tokio::time::timeout(TOTAL, corpo())
        .await
        .expect("o teste inteiro passou de 5 min");
}

async fn corpo() {
    let par = tokio::time::timeout(PREPARACAO, par_em_loopback())
        .await
        .expect("parou no passo 1: par_em_loopback não ficou pronto em 30 s");
    let credito_inicial = (
        credito_em_aberto(&par.anfitriao),
        credito_em_aberto(&par.cliente),
    );
    let inicio = Instant::now();
    let enviadas = Arc::new(AtomicU64::new(0));
    let recebidas = Arc::new(AtomicU64::new(0));

    let mut envio = tokio::time::timeout(PREPARACAO, par.cliente.open_uni())
        .await
        .expect("parou no passo 2: open_uni não terminou em 30 s")
        .unwrap();
    let escritora = {
        let enviadas = Arc::clone(&enviadas);
        let recebidas = Arc::clone(&recebidas);
        tokio::spawn(async move {
            for n in 0..MENSAGENS {
                // Janela limitada: espera o receptor chegar perto antes de mandar mais.
                while n.saturating_sub(recebidas.load(Ordering::Relaxed)) > JANELA {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                envio.write_all(&montar(n)).await.unwrap();
                enviadas.store(n + 1, Ordering::Relaxed);
            }
            envio.finish().unwrap();
            envio.entregue().await.unwrap();
        })
    };
    // O canal só aparece no anfitrião com o primeiro byte: a escritora já escreveu.
    let mut recebe = tokio::time::timeout(PREPARACAO, par.anfitriao.accept_uni())
        .await
        .expect("parou no passo 3: accept_uni não terminou em 30 s")
        .unwrap();
    let leitora = {
        let recebidas = Arc::clone(&recebidas);
        tokio::spawn(async move {
            let mut bytes = 0_u64;
            for esperada in 0..MENSAGENS {
                let mut cab = [0_u8; CABECALHO];
                recebe
                    .read_exact(&mut cab)
                    .await
                    .map_err(|e| format!("mensagem {esperada}: cabeçalho: {e:?}"))?;
                let n = u64::from_be_bytes(cab[..8].try_into().unwrap());
                let total = u32::from_be_bytes(cab[8..].try_into().unwrap()) as usize;
                if n != esperada {
                    return Err(format!("fora de ordem: veio {n}, esperava {esperada}"));
                }
                if total != tamanho(n) {
                    return Err(format!("mensagem {n}: tamanho {total}"));
                }
                let mut corpo = vec![0_u8; total - CABECALHO];
                recebe
                    .read_exact(&mut corpo)
                    .await
                    .map_err(|e| format!("mensagem {n}: corpo: {e:?}"))?;
                if let Some(j) = corpo
                    .iter()
                    .enumerate()
                    .position(|(j, b)| *b != (n as u8).wrapping_add(j as u8))
                {
                    return Err(format!("mensagem {n}: byte {j} corrompido"));
                }
                bytes += total as u64;
                recebidas.store(n + 1, Ordering::Relaxed);
                // Antes, na e depois de cada volta: contagem, ordem e conteúdo já conferidos.
                if [VOLTA, 2 * VOLTA]
                    .iter()
                    .any(|v| (v - 2..=v + 1).contains(&n))
                {
                    println!("ponto {n}: ok (contagem {}, {bytes} B)", n + 1);
                }
            }
            // Depois da última, só o fim: nenhuma duplicata sobrando.
            match recebe.read_to_end(1).await {
                Ok(v) if v.is_empty() => Ok(bytes),
                outro => Err(format!("sobrou algo depois da última: {outro:?}")),
            }
        })
    };

    // Vigia de progresso: se nada mexe por SEM_PROGRESSO, diz onde parou.
    let mut visto = (0, 0);
    let mut desde = Instant::now();
    while !(escritora.is_finished() && leitora.is_finished()) {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let agora = (
            enviadas.load(Ordering::Relaxed),
            recebidas.load(Ordering::Relaxed),
        );
        if agora != visto {
            for volta in 1..=2 {
                if visto.1 < volta * VOLTA && agora.1 >= volta * VOLTA {
                    println!(
                        "passou da volta {volta}: {} recebidas, {:.1}s",
                        agora.1,
                        inicio.elapsed().as_secs_f64()
                    );
                }
            }
            visto = agora;
            desde = Instant::now();
        } else if desde.elapsed() > SEM_PROGRESSO {
            panic!(
                "TRAVOU: enviadas={} recebidas={} (volta em {VOLTA}) após {:.1}s; \
                 crédito anfitrião={} cliente={} (inicial {:?}); contadores anfitrião={:?}; \
                 acabou anfitrião={:?} cliente={:?}",
                agora.0,
                agora.1,
                inicio.elapsed().as_secs_f64(),
                credito_em_aberto(&par.anfitriao),
                credito_em_aberto(&par.cliente),
                credito_inicial,
                par.anfitriao.contadores(),
                par.anfitriao.acabou(),
                par.cliente.acabou(),
            );
        }
    }
    let bytes = leitora.await.unwrap().expect("a leitora achou defeito");
    escritora.await.unwrap();
    let tempo = inicio.elapsed();

    // O crédito devolvido é assíncrono: dá um instante para assentar.
    let mut final_ = (0, 0);
    for _ in 0..50 {
        final_ = (
            credito_em_aberto(&par.anfitriao),
            credito_em_aberto(&par.cliente),
        );
        if final_ == credito_inicial {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    println!(
        "{MENSAGENS} mensagens ({:.1} MiB) em {:.1}s; crédito inicial {:?} final {:?}; \
         contadores anfitrião {:?}",
        bytes as f64 / (1024.0 * 1024.0),
        tempo.as_secs_f64(),
        credito_inicial,
        final_,
        par.anfitriao.contadores(),
    );
    assert_eq!(final_, credito_inicial, "o crédito vazou");
    assert_eq!(par.anfitriao.contadores().cortes_por_fila, 0);
    assert!(par.anfitriao.acabou().is_none() && par.cliente.acabou().is_none());
}
