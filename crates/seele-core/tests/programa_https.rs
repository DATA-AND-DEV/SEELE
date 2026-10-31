//! `programa.baixar` pelo `reqwest` de verdade, contra um servidor HTTPS em
//! `127.0.0.1` com certificado de teste.
//!
//! A origem de mentira dos testes de unidade prova a lógica; esta prova que a
//! [`OrigemHttps`] entrega o corpo **aos pedaços**, e não junto. Se o
//! `reqwest` juntasse o corpo antes de devolver, o corte no tamanho declarado
//! viraria um corte depois de um corpo sem fim — e é isso que o servidor daqui
//! manda.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use seele_core::programa::{
    AlgoritmoDeHash, ComandoAceito, HashDeclarado, Origem, OrigemHttps, PedidoDeDownload, Politica,
    Programa, Recusa,
};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio_rustls::rustls;

struct Pasta(PathBuf);

impl Drop for Pasta {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// O que o servidor de teste responde.
#[derive(Clone, Copy)]
enum Corpo {
    /// Estes bytes, com `Content-Length`.
    Exato(&'static [u8]),
    /// Sem `Content-Length`, pedaços de 64 KiB para sempre.
    SemFim,
}

/// Sobe um servidor HTTPS que responde `corpo` a todo pedido; devolve a porta,
/// o certificado (DER) e quantos bytes de corpo ele conseguiu escrever.
async fn servidor(corpo: Corpo) -> (u16, Vec<u8>, Arc<AtomicU64>) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let gerado = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
    let der = gerado.cert.der().to_vec();
    let chave =
        rustls::pki_types::PrivateKeyDer::try_from(gerado.signing_key.serialize_der()).unwrap();
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![gerado.cert.der().clone()], chave)
        .unwrap();
    let aceitador = tokio_rustls::TlsAcceptor::from(Arc::new(config));
    let ouvinte = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let porta = ouvinte.local_addr().unwrap().port();
    let escritos = Arc::new(AtomicU64::new(0));
    let contando = Arc::clone(&escritos);
    tokio::spawn(async move {
        while let Ok((fluxo, _)) = ouvinte.accept().await {
            let aceitador = aceitador.clone();
            let contando = Arc::clone(&contando);
            tokio::spawn(async move {
                let Ok(mut tls) = aceitador.accept(fluxo).await else {
                    return;
                };
                // Lê o pedido até a linha em branco.
                let mut pedido = Vec::new();
                let mut byte = [0_u8; 1];
                while !pedido.ends_with(b"\r\n\r\n") {
                    if tls.read(&mut byte).await.unwrap_or(0) == 0 {
                        return;
                    }
                    pedido.push(byte[0]);
                }
                match corpo {
                    Corpo::Exato(bytes) => {
                        let cabecalho = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            bytes.len()
                        );
                        let _ = tls.write_all(cabecalho.as_bytes()).await;
                        let _ = tls.write_all(bytes).await;
                        contando.fetch_add(bytes.len() as u64, Ordering::SeqCst);
                        let _ = tls.shutdown().await;
                    }
                    Corpo::SemFim => {
                        let _ = tls
                            .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n")
                            .await;
                        let pedaco = vec![b'x'; 64 * 1024];
                        while tls.write_all(&pedaco).await.is_ok() {
                            contando.fetch_add(pedaco.len() as u64, Ordering::SeqCst);
                        }
                    }
                }
            });
        }
    });
    (porta, der, escritos)
}

fn programa(der: &[u8], nome: &str) -> (Pasta, Programa) {
    let pasta = Pasta(std::env::temp_dir().join(format!(
        "seele-programa-https-{nome}-{}",
        std::process::id()
    )));
    let _ = std::fs::remove_dir_all(&pasta.0);
    let origem: Arc<dyn Origem> = Arc::new(OrigemHttps::confiando_so_em(der).unwrap());
    let programa = Programa::novo(
        Politica {
            dominios: vec!["localhost".to_owned()],
            artefatos: vec![],
            de_outros_sistemas: vec![],
            origens: vec!["localhost".to_owned()],
            comando: None::<ComandoAceito>,
            instalar: None,
            pack: None,
            teto_de_disco: 1 << 30,
            raiz: pasta.0.join("jogo"),
        },
        origem,
    )
    .unwrap();
    (pasta, programa)
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[tokio::test]
async fn baixa_pelo_https_de_verdade() {
    const BYTES: &[u8] = b"o server.jar de mentira";
    let (porta, der, _) = servidor(Corpo::Exato(BYTES)).await;
    let (_p, programa) = programa(&der, "exato");
    let feito = programa
        .baixar(PedidoDeDownload {
            url: format!("https://localhost:{porta}/server.jar"),
            hashes: vec![HashDeclarado {
                algoritmo: AlgoritmoDeHash::Sha256,
                hex: sha256(BYTES),
            }],
            tamanho: BYTES.len() as u64,
            destino: "server/server.jar".to_owned(),
        })
        .unwrap()
        .terminar()
        .await
        .unwrap();
    assert_eq!(std::fs::read(&feito.destino).unwrap(), BYTES);
}

/// O corpo sem fim é cortado logo depois do declarado.
///
/// Medido no Mac (30/10, três rodadas): o corte veio em 1 056 768 bytes (um
/// pedaço de 8 KiB depois do 1 MiB declarado), e o servidor escreveu entre
/// 1,69 e 1,75 MiB no total — o resto ficou nos buffers do TLS e do TCP. O teto
/// da asserção (declarado + 8 MiB) é folgado para os buffers de outro sistema,
/// e finito: sem o corte em `correr`, o servidor escreveria até o prazo.
#[tokio::test]
async fn corpo_sem_fim_e_cortado_pelo_https_de_verdade() {
    let (porta, der, escritos) = servidor(Corpo::SemFim).await;
    let (_p, programa) = programa(&der, "sem-fim");
    let declarado: u64 = 1024 * 1024;
    let fim = tokio::time::timeout(
        Duration::from_secs(30),
        programa
            .baixar(PedidoDeDownload {
                url: format!("https://localhost:{porta}/grande"),
                hashes: vec![HashDeclarado {
                    algoritmo: AlgoritmoDeHash::Sha256,
                    hex: "0".repeat(64),
                }],
                tamanho: declarado,
                destino: "grande.bin".to_owned(),
            })
            .unwrap()
            .terminar(),
    )
    .await
    .expect("o download não parou: o corpo sem fim foi lido sem corte");
    let Err(Recusa::MaiorQueODeclarado { recebido, .. }) = fim else {
        panic!("esperava o corte, veio {fim:?}");
    };
    assert!(recebido > declarado);
    assert!(recebido <= declarado + 64 * 1024, "{recebido}");
    // O servidor para de conseguir escrever pouco depois.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let total = escritos.load(Ordering::SeqCst);
    eprintln!("declarado {declarado}, recebido no corte {recebido}, escrito pelo servidor {total}");
    assert!(
        total < declarado + 8 * 1024 * 1024,
        "o servidor escreveu {total} bytes: a conexão não foi fechada no corte"
    );
    let parado = escritos.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        escritos.load(Ordering::SeqCst),
        parado,
        "o servidor ainda escreve"
    );
}
