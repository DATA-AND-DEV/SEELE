//! A corrida entre a tarefa do enlace, que soma uma mensagem na fila e a
//! entrega, e quem larga o `Recebe` (`stop` ou `drop`) ao mesmo tempo, numa
//! runtime de várias threads (revisão do crédito integrado, P1). Montada sem
//! sorte, com a barreira de `teste::pausar_a_proxima_entrega`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    missing_docs
)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use seele_enlace::teste::{credito_em_aberto, par_em_loopback, pausar_a_proxima_entrega};

#[derive(Clone, Copy, Debug)]
enum Larga {
    Parar,
    Soltar,
}

async fn largar_no_meio_da_entrega_nao_trava_o_agregado(larga: Larga) {
    let par = par_em_loopback().await;
    let mib = 1024 * 1024;
    let teto = seele_enlace::MAX_CREDITO_DA_CONEXAO as u64;

    // Oito reservas de 1 MiB enchem o agregado.
    let mut envios = Vec::new();
    let mut recebes = Vec::new();
    for _ in 0..8 {
        let mut envio = par.cliente.open_uni().await.unwrap();
        envio.write_all(&[1]).await.unwrap();
        let recebe = tokio::time::timeout(Duration::from_secs(5), par.anfitriao.accept_uni())
            .await
            .unwrap()
            .unwrap();
        envios.push(envio);
        recebes.push(recebe);
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(credito_em_aberto(&par.anfitriao), teto);

    // O nono espera vaga.
    let total = 2 * mib;
    let mut nono = par.cliente.open_uni().await.unwrap();
    let escritos = Arc::new(AtomicUsize::new(0));
    let escrita = {
        let escritos = Arc::clone(&escritos);
        tokio::spawn(async move {
            let pedaco = vec![9_u8; 16 * 1024];
            for _ in 0..(total / pedaco.len()) {
                nono.write_all(&pedaco).await.unwrap();
                escritos.fetch_add(pedaco.len(), Ordering::Relaxed);
            }
            nono.finish().unwrap();
            nono.entregue().await.unwrap();
        })
    };
    let mut recebe_nono = tokio::time::timeout(Duration::from_secs(5), par.anfitriao.accept_uni())
        .await
        .unwrap()
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(escritos.load(Ordering::Relaxed) <= seele_enlace::CREDITO_DE_SAIDA);

    // A corrida: a tarefa do anfitrião soma 16 KiB na fila do primeiro canal e
    // para antes de entregar; quem lê larga o `Recebe` inteiro; a tarefa segue.
    let pausa = pausar_a_proxima_entrega(&par.anfitriao);
    envios[0].write_all(&vec![2_u8; 16 * 1024]).await.unwrap();
    let pausa = tokio::task::spawn_blocking(move || {
        pausa.esperar_parar();
        pausa
    })
    .await
    .unwrap();
    let mut primeiro = recebes.remove(0);
    match larga {
        Larga::Parar => primeiro.stop(5).unwrap(),
        Larga::Soltar => drop(primeiro),
    }
    pausa.soltar();

    let recebido =
        tokio::time::timeout(Duration::from_secs(10), recebe_nono.read_to_end(total + 1))
            .await
            .unwrap_or_else(|_| {
                panic!(
                    "{larga:?}: o nono canal esperou para sempre (o agregado diz {} B)",
                    credito_em_aberto(&par.anfitriao)
                )
            })
            .unwrap();
    assert_eq!(recebido.len(), total);
    tokio::time::timeout(Duration::from_secs(5), escrita)
        .await
        .unwrap()
        .unwrap();
    let segurado = credito_em_aberto(&par.anfitriao);
    assert!(
        segurado <= teto,
        "{larga:?}: o agregado diz {segurado} B (teto {teto} B)"
    );
    drop((envios, recebes));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parar_no_meio_da_entrega_nao_trava_o_agregado() {
    largar_no_meio_da_entrega_nao_trava_o_agregado(Larga::Parar).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn largar_no_meio_da_entrega_nao_trava_o_agregado_ao_soltar() {
    largar_no_meio_da_entrega_nao_trava_o_agregado(Larga::Soltar).await;
}
