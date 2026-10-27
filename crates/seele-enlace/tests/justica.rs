//! R4 da revisão: a voz não espera atrás de uma transferência.
//!
//! Mede o atraso de um datagrama de voz, de quem manda a quem lê no mesmo
//! processo (o mesmo relógio), em repouso e durante uma rajada de 32 MiB num
//! canal "u". O critério do coordenador: p95 durante a rajada abaixo de 100 ms
//! acima do p95 em repouso, em loopback.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    missing_docs
)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use seele_enlace::{CertificadoDtls, Conexao, ConfigDoAnfitriao, Discagem, Pino, Ponta};

async fn par() -> (Conexao, Conexao, Ponta, Ponta) {
    let certificado = CertificadoDtls::efemero().unwrap();
    let host = Ponta::abrir(
        SocketAddr::from(([127, 0, 0, 1], 0)),
        Some(ConfigDoAnfitriao::novo(certificado.clone())),
    )
    .await
    .unwrap();
    let cli = Ponta::abrir(SocketAddr::from(([127, 0, 0, 1], 0)), None)
        .await
        .unwrap();
    let discagem = Discagem::nova(
        vec![host.local_addr()],
        Pino::Impressao(certificado.impressao()),
    );
    let (a, b) = tokio::join!(cli.discar(discagem), host.aceitar());
    (a.unwrap(), b.unwrap(), host, cli)
}

/// Manda `quantos` datagramas, um a cada 20 ms, e devolve os atrasos vistos.
async fn medir_voz(a: &Conexao, b: &Conexao, inicio: Instant, quantos: usize) -> Vec<Duration> {
    let leitor = {
        let b = b.clone();
        tokio::spawn(async move {
            let mut atrasos = Vec::new();
            while atrasos.len() < quantos {
                let Ok(Ok(d)) =
                    tokio::time::timeout(Duration::from_secs(3), b.read_datagram()).await
                else {
                    break;
                };
                let enviado = u64::from_be_bytes(d[1..9].try_into().unwrap());
                let agora = u64::try_from(inicio.elapsed().as_micros()).unwrap();
                atrasos.push(Duration::from_micros(agora.saturating_sub(enviado)));
            }
            atrasos
        })
    };
    let mut tique = tokio::time::interval(Duration::from_millis(20));
    for _ in 0..quantos {
        tique.tick().await;
        let mut d = vec![1_u8; 120];
        let agora = u64::try_from(inicio.elapsed().as_micros()).unwrap();
        d[1..9].copy_from_slice(&agora.to_be_bytes());
        let _ = a.send_datagram(d);
    }
    leitor.await.unwrap()
}

fn p95(mut atrasos: Vec<Duration>) -> Duration {
    atrasos.sort_unstable();
    let i = (atrasos.len() * 95 / 100).min(atrasos.len().saturating_sub(1));
    atrasos[i]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_voz_p95_durante_uma_rajada_de_32_mib() {
    let (a, b, _host, _cli) = par().await;
    let inicio = Instant::now();

    let repouso = medir_voz(&a, &b, inicio, 50).await;
    assert!(
        repouso.len() >= 45,
        "a voz em repouso se perdeu: {}",
        repouso.len()
    );
    let p95_repouso = p95(repouso);

    // A rajada: 32 MiB num "u", lidos sem parar do outro lado.
    let mut envio = a.open_uni().await.unwrap();
    let total = 32 * 1024 * 1024;
    let acabou = Arc::new(AtomicBool::new(false));
    let escritor = {
        let acabou = Arc::clone(&acabou);
        tokio::spawn(async move {
            let pedaco = vec![0x5A_u8; 64 * 1024];
            let comeco = Instant::now();
            for _ in 0..(total / pedaco.len()) {
                envio.write_all(&pedaco).await.unwrap();
            }
            envio.finish().unwrap();
            acabou.store(true, Ordering::Relaxed);
            comeco.elapsed()
        })
    };
    let mut recebe = b.accept_uni().await.unwrap();
    let leitor = tokio::spawn(async move {
        let mut buf = vec![0_u8; 64 * 1024];
        let mut lidos = 0;
        while let Some(n) = recebe.read(&mut buf).await.unwrap() {
            lidos += n;
        }
        lidos
    });

    // Mede a voz enquanto a rajada corre (até 150 datagramas, ~3 s).
    let mut durante = Vec::new();
    while !acabou.load(Ordering::Relaxed) && durante.len() < 150 {
        durante.extend(medir_voz(&a, &b, inicio, 25).await);
    }
    let tempo_da_rajada = escritor.await.unwrap();
    assert_eq!(leitor.await.unwrap(), total);
    assert!(!durante.is_empty(), "a rajada acabou antes de medir a voz");
    let medidos = durante.len();
    let p95_durante = p95(durante);
    println!(
        "R4: voz p95 em repouso {p95_repouso:?}, durante a rajada {p95_durante:?} \
         ({medidos} datagramas); 32 MiB em {tempo_da_rajada:?}"
    );
    // O critério do coordenador: 100 ms acima do repouso. Medido com a cripto
    // otimizada também no perfil dev (o `[profile.dev.package.*]` da raiz):
    // 1,0–1,4 ms. Antes dele, a RustCrypto sem otimização prendia a tarefa da
    // ponta à CPU, e a medida em debug dependia da carga da máquina.
    let teto = Duration::from_millis(100);
    // A garantia que não depende da máquina: o escalonador nunca deixou mais que
    // o limiar (mais uma mensagem) de volume no SCTP na frente da voz. Em
    // loopback a CPU é o gargalo e o atraso sozinho não separa a regressão (sem o
    // escalonador e com a cripto otimizada, ele deu 1,9 ms); este número separa.
    let maior = a.contadores().maior_volume_no_sctp;
    let limite = (seele_enlace::LIMIAR_DE_VOLUME + seele_enlace::MAX_MENSAGEM) as u64;
    println!("R4: maior volume no SCTP na frente da voz: {maior} B (limite {limite} B)");
    assert!(
        maior > 0,
        "a rajada não passou pelo escalonador: o teste mediria outra coisa"
    );
    assert!(
        maior <= limite,
        "o escalonador deixou {maior} B de volume na frente da voz (limite {limite} B)"
    );
    assert!(
        p95_durante < p95_repouso + teto,
        "a voz esperou atrás da transferência: p95 {p95_durante:?} contra {p95_repouso:?} em repouso"
    );
}
