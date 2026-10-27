//! Um anfitrião de eco, para medir navegadores reais contra o enlace de produto.
//!
//! ```text
//! cargo run -p seele-enlace --example eco -- [127.0.0.1:0] [--efemero] [--sem-eco-de-volume]
//! ```
//!
//! - O certificado é gerado como o do `seeled` (`rcgen::generate_simple_self_signed`,
//!   P-256, validade de 1975 a 4096), a não ser com `--efemero`.
//! - Imprime em stdout, uma linha cada: `ESCUTA <ip:porta> FP <hex64>` e
//!   `CERT <der em hex>`.
//! - Ecoa cada mensagem de dados do `ctl` e cada datagrama da voz, e os canais
//!   "u" que chegam voltam inteiros por um "u" novo, **de qualquer tamanho**. Cada
//!   conexão que abre e fecha sai numa linha `CONEXAO` / `FECHOU` em stdout.
//! - Com `--sem-eco-de-volume`, um "u" acima de 1 MiB é contado e descartado, sem
//!   voltar: é o que a bancada da subida (`bancada/subida.cjs`) usa, para a rajada
//!   que sobe não virar rajada que desce. **Sem a opção, tudo volta**: a bancada do
//!   navegador (`rtc-eco.cjs`) mede a descida com os 32 MiB de volta. (Medido em
//!   24/10: o corte de 1 MiB sem opção, que entrou com a bancada da subida, fez os
//!   32 MiB pararem de voltar, e pareceu um defeito do enlace.)

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    missing_docs
)]

use std::net::SocketAddr;

use seele_enlace::{CertificadoDtls, Conexao, ConfigDoAnfitriao, Ponta};

fn certificado_como_o_do_seeled() -> CertificadoDtls {
    let gerado = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
    CertificadoDtls::novo(
        gerado.cert.der().to_vec(),
        gerado.signing_key.serialize_der(),
    )
    .unwrap()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

async fn servir(conexao: Conexao, so_conta_o_volume: bool) {
    println!(
        "CONEXAO {} {}",
        conexao.stable_id(),
        conexao.remote_address()
    );
    let (mut envio, mut recebe) = conexao.controle().unwrap();
    let voz_recebida = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let voz = {
        let conexao = conexao.clone();
        let contador = std::sync::Arc::clone(&voz_recebida);
        tokio::spawn(async move {
            while let Ok(datagrama) = conexao.read_datagram().await {
                contador.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let _ = conexao.send_datagram(datagrama);
            }
        })
    };
    let volume_recebido = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let fluxos = {
        let conexao = conexao.clone();
        let volume = std::sync::Arc::clone(&volume_recebido);
        tokio::spawn(async move {
            while let Ok(mut entrada) = conexao.accept_uni().await {
                let conexao = conexao.clone();
                let volume = std::sync::Arc::clone(&volume);
                tokio::spawn(async move {
                    // Tudo volta, a não ser com `--sem-eco-de-volume`: aí, acima de
                    // 1 MiB, é rajada de subida, contada e descartada.
                    let teto = if so_conta_o_volume {
                        1 << 20
                    } else {
                        usize::MAX
                    };
                    let mut guardado = Vec::new();
                    let mut buf = vec![0_u8; 64 * 1024];
                    let mut total = 0_usize;
                    loop {
                        match entrada.read(&mut buf).await {
                            Ok(Some(n)) => {
                                total += n;
                                volume.fetch_add(n as u64, std::sync::atomic::Ordering::Relaxed);
                                if total <= teto {
                                    guardado.extend_from_slice(&buf[..n]);
                                }
                            }
                            Ok(None) => break,
                            Err(_) => return,
                        }
                    }
                    if total <= teto {
                        if let Ok(mut volta) = conexao.open_uni().await {
                            let _ = volta.write_all(&guardado).await;
                            let _ = volta.finish();
                            let _ = volta.entregue().await;
                        }
                    }
                });
            }
        })
    };
    let mut buf = vec![0_u8; 16 * 1024];
    while let Ok(Some(n)) = recebe.read(&mut buf).await {
        if envio.write_all(&buf[..n]).await.is_err() {
            break;
        }
    }
    let motivo = conexao.closed().await;
    voz.abort();
    fluxos.abort();
    let c = conexao.contadores();
    println!(
        "VOZ {} recebida={} descartada_na_fila={} volume_recebido={}",
        conexao.stable_id(),
        voz_recebida.load(std::sync::atomic::Ordering::Relaxed),
        c.voz_descartada,
        volume_recebido.load(std::sync::atomic::Ordering::Relaxed)
    );
    println!("FECHOU {} {motivo}", conexao.stable_id());
}

#[tokio::main]
async fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .try_init();
    let argumentos: Vec<String> = std::env::args().skip(1).collect();
    let escuta: SocketAddr = argumentos
        .iter()
        .find(|a| !a.starts_with("--"))
        .map_or_else(|| "127.0.0.1:0".parse().unwrap(), |a| a.parse().unwrap());
    let so_conta_o_volume = argumentos.iter().any(|a| a == "--sem-eco-de-volume");
    let certificado = if argumentos.iter().any(|a| a == "--efemero") {
        CertificadoDtls::efemero().unwrap()
    } else {
        certificado_como_o_do_seeled()
    };
    let ponta = Ponta::abrir(escuta, Some(ConfigDoAnfitriao::novo(certificado.clone())))
        .await
        .unwrap();
    println!(
        "ESCUTA {} FP {}",
        ponta.local_addr(),
        hex(&certificado.impressao())
    );
    println!("CERT {}", hex(certificado.der()));
    println!("BUFFER_DE_RECEPCAO {:?}", ponta.buffer_de_recepcao());
    while let Some(conexao) = ponta.aceitar().await {
        tokio::spawn(servir(conexao, so_conta_o_volume));
    }
}
