//! `programa.empacotar`, `programa.oferecerCopia` e `programa.receberCopia`
//! (API 8 parte 7, a reserva): um mundo de verdade em disco, empacotado numa
//! máquina, servido em `127.0.0.1` e recebido noutra, com o sha256, a
//! retomada e as recusas.
//!
//! Aqui as duas pontas ligam direto na porta de quem oferece: o túnel entre
//! elas é provado em `seele-conformance` (`reserva.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use seele_core::programa::{
    Origem, OrigemHttps, PedidoDeCopia, PedidoDeDesempacotar, Politica, Programa, Recusa,
};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

struct Pasta(PathBuf);

impl Drop for Pasta {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn programa(nome: &str, teto: u64, portas: Vec<u16>) -> (Pasta, Programa) {
    static N: AtomicU64 = AtomicU64::new(0);
    let caminho = std::env::temp_dir().join(format!(
        "seele-programa-copia-{nome}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&caminho);
    let politica = Politica {
        dominios: vec![],
        artefatos: vec![],
        de_outros_sistemas: vec![],
        origens: vec![],
        comando: None,
        instalar: None,
        pack: None,
        teto_de_disco: teto,
        portas_de_copia: portas,
        raiz: caminho.join("jogo"),
    };
    let origem: Arc<dyn Origem> = Arc::new(OrigemHttps::nova().unwrap());
    (Pasta(caminho), Programa::novo(politica, origem).unwrap())
}

/// Um «mundo»: arquivos em pastas, um grande e com bytes que denunciam troca.
fn mundo(raiz: &Path) {
    let w = raiz.join("world");
    std::fs::create_dir_all(w.join("region")).unwrap();
    std::fs::create_dir_all(w.join("playerdata")).unwrap();
    std::fs::write(w.join("level.dat"), b"nivel").unwrap();
    std::fs::write(w.join("playerdata/alguem.dat"), b"inventario").unwrap();
    let grande: Vec<u8> = (0..3_000_000_u32)
        .map(|i| u8::try_from((i * 7 + i / 251) % 251).unwrap())
        .collect();
    std::fs::write(w.join("region/r.0.0.mca"), grande).unwrap();
}

/// Uma porta livre agora.
fn porta_livre() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn arvore(raiz: &Path) -> Vec<(String, Vec<u8>)> {
    let mut todos = Vec::new();
    let mut pendentes = vec![raiz.to_path_buf()];
    while let Some(p) = pendentes.pop() {
        for e in std::fs::read_dir(&p).unwrap().flatten() {
            let c = e.path();
            if c.is_dir() {
                pendentes.push(c);
            } else {
                let rel = c
                    .strip_prefix(raiz)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                todos.push((rel, std::fs::read(&c).unwrap()));
            }
        }
    }
    todos.sort();
    todos
}

#[tokio::test]
async fn o_mundo_vai_inteiro_de_uma_maquina_a_outra() {
    let porta = porta_livre();
    let (_a, principal) = programa("principal", 1 << 30, vec![porta]);
    let (_b, reserva) = programa("reserva", 1 << 30, vec![]);
    mundo(principal.raiz());

    let feito = principal.empacotar("world", "copias/a.tar.gz").unwrap();
    assert_eq!(feito.arquivos, 3);
    assert_eq!(feito.links_pulados, 0);
    assert_eq!(feito.sha256.len(), 64);
    let oferta = principal
        .oferecer_copia("copias/a.tar.gz", porta)
        .await
        .unwrap();
    assert_eq!(
        oferta.sha256, feito.sha256,
        "o hash da oferta é o do pacote"
    );
    assert_eq!(oferta.tamanho, feito.tamanho);

    let vistos = Arc::new(AtomicU64::new(0));
    let v = Arc::clone(&vistos);
    let chegou = reserva
        .receber_copia(
            PedidoDeCopia {
                porta_local: porta,
                codigo: oferta.codigo.clone(),
                sha256: oferta.sha256.clone(),
                tamanho: oferta.tamanho,
                destino: "copias/recebida.tar.gz".into(),
            },
            move |recebidos, _| v.store(recebidos, Ordering::SeqCst),
        )
        .await
        .unwrap();
    assert_eq!(chegou.sha256, feito.sha256);
    assert_eq!(chegou.retomadas, 0);
    assert_eq!(vistos.load(Ordering::SeqCst), oferta.tamanho);

    reserva
        .desempacotar(&PedidoDeDesempacotar {
            arquivo: "copias/recebida.tar.gz".into(),
            destino: "world".into(),
            raiz: "world".into(),
            mesclar: false,
        })
        .unwrap();
    assert_eq!(
        arvore(&reserva.raiz().join("world")),
        arvore(&principal.raiz().join("world")),
        "o mundo da reserva não é o da principal"
    );
}

#[tokio::test]
async fn sem_o_codigo_ou_com_outra_copia_nada_sai() {
    let porta = porta_livre();
    let (_a, principal) = programa("codigo", 1 << 30, vec![porta]);
    let (_b, reserva) = programa("codigo-r", 1 << 30, vec![]);
    mundo(principal.raiz());
    principal.empacotar("world", "a.tar.gz").unwrap();
    let oferta = principal.oferecer_copia("a.tar.gz", porta).await.unwrap();
    let pedido = |codigo: &str, sha: &str| PedidoDeCopia {
        porta_local: porta,
        codigo: codigo.into(),
        sha256: sha.into(),
        tamanho: oferta.tamanho,
        destino: "r.tar.gz".into(),
    };
    let errado = reserva
        .receber_copia(pedido(&"0".repeat(32), &oferta.sha256), |_, _| {})
        .await
        .unwrap_err();
    assert_eq!(
        errado,
        Recusa::CopiaRecusada {
            motivo: "codigo-errado".into()
        }
    );
    let outra = reserva
        .receber_copia(pedido(&oferta.codigo, &"a".repeat(64)), |_, _| {})
        .await
        .unwrap_err();
    assert_eq!(
        outra,
        Recusa::CopiaRecusada {
            motivo: "outra-copia".into()
        }
    );
    assert!(!reserva.raiz().join("r.tar.gz").exists());
    assert_eq!(reserva.recusas().get("copia-recusada"), Some(&2));
}

#[tokio::test]
async fn a_copia_so_sai_por_uma_porta_de_tunel_aceita() {
    let (_a, principal) = programa("porta", 1 << 30, vec![25_565]);
    mundo(principal.raiz());
    principal.empacotar("world", "a.tar.gz").unwrap();
    let recusa = principal
        .oferecer_copia("a.tar.gz", 4_000)
        .await
        .unwrap_err();
    assert_eq!(
        recusa,
        Recusa::PortaForaDoAceite {
            porta: 4_000,
            aceitas: vec![25_565]
        }
    );
}

#[test]
fn empacotar_nao_segue_link_nem_mora_na_origem() {
    let (_a, p) = programa("link", 1 << 30, vec![]);
    mundo(p.raiz());
    assert!(matches!(
        p.empacotar("world", "world/dentro.tar.gz").unwrap_err(),
        Recusa::ArquivoDentroDaOrigem { .. }
    ));
    assert!(matches!(
        p.empacotar("level.dat", "a.tar.gz").unwrap_err(),
        Recusa::NaoEPasta { .. }
    ));
    assert!(matches!(
        p.empacotar("../fora", "a.tar.gz").unwrap_err(),
        Recusa::CaminhoComPai { .. }
    ));
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/etc", p.raiz().join("world/etc")).unwrap();
        let feito = p.empacotar("world", "a.tar.gz").unwrap();
        assert_eq!(feito.links_pulados, 1, "o link entrou no pacote");
        assert_eq!(feito.arquivos, 3);
    }
}

#[test]
fn empacotar_para_no_teto_de_disco_e_nada_fica() {
    // O mundo ocupa ~3 MB, comprimido bem menos; o teto deixa só uns KiB.
    let (_a, p) = programa("teto", 3_000_100, vec![]);
    mundo(p.raiz());
    let recusa = p.empacotar("world", "a.tar.gz").unwrap_err();
    assert!(matches!(recusa, Recusa::DiscoCheio { .. }), "{recusa:?}");
    let sobrou: Vec<_> = std::fs::read_dir(p.raiz())
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert_eq!(
        sobrou,
        vec![std::ffi::OsString::from("world")],
        "o parcial ficou"
    );
}

/// **A retomada**: um servidor de mentira que corta a primeira ligação no
/// meio; a segunda pede `desde` o que já chegou, e a cópia fecha certa.
#[tokio::test]
async fn a_ligacao_que_cai_no_meio_continua_de_onde_parou() {
    let (_a, origem) = programa("retoma-o", 1 << 30, vec![]);
    let (_b, reserva) = programa("retoma", 1 << 30, vec![]);
    mundo(origem.raiz());
    let feito = origem.empacotar("world", "a.tar.gz").unwrap();
    let bytes = std::fs::read(&feito.arquivo).unwrap();
    let metade = bytes.len() / 2;
    let tamanho = bytes.len() as u64;
    let escuta = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let porta = escuta.local_addr().unwrap().port();
    let pedidos = Arc::new(std::sync::Mutex::new(Vec::new()));
    let anotados = Arc::clone(&pedidos);
    tokio::spawn(async move {
        let mut vez = 0;
        while let Ok((mut fluxo, _)) = escuta.accept().await {
            vez += 1;
            let mut linha = Vec::new();
            let mut b = [0_u8; 1];
            while fluxo.read_exact(&mut b).await.is_ok() && b[0] != b'\n' {
                linha.push(b[0]);
            }
            let linha = String::from_utf8(linha).unwrap();
            let desde: usize = linha.rsplit(' ').next().unwrap().parse().unwrap();
            anotados.lock().unwrap().push(desde);
            fluxo
                .write_all(format!("OK {tamanho}\n").as_bytes())
                .await
                .unwrap();
            let fim = if vez == 1 {
                bytes.len() / 2
            } else {
                bytes.len()
            };
            fluxo.write_all(&bytes[desde..fim]).await.unwrap();
            let _ = fluxo.shutdown().await;
        }
    });
    let chegou = reserva
        .receber_copia(
            PedidoDeCopia {
                porta_local: porta,
                codigo: "c".repeat(32),
                sha256: feito.sha256.clone(),
                tamanho,
                destino: "r.tar.gz".into(),
            },
            |_, _| {},
        )
        .await
        .unwrap();
    assert_eq!(chegou.retomadas, 1);
    assert_eq!(*pedidos.lock().unwrap(), vec![0, metade]);
    assert_eq!(
        std::fs::read(reserva.raiz().join("r.tar.gz")).unwrap(),
        std::fs::read(&feito.arquivo).unwrap()
    );
}

/// **A cópia que não bate não vira o destino**: os bytes chegam inteiros, mas
/// o sha256 anunciado é outro.
#[tokio::test]
async fn a_copia_que_nao_bate_e_apagada_e_o_destino_fica() {
    let (_b, reserva) = programa("nao-bate", 1 << 30, vec![]);
    std::fs::write(reserva.raiz().join("r.tar.gz"), b"a de antes").unwrap();
    let escuta = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let porta = escuta.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut fluxo, _)) = escuta.accept().await {
            let mut b = [0_u8; 1];
            while fluxo.read_exact(&mut b).await.is_ok() && b[0] != b'\n' {}
            fluxo.write_all(b"OK 5\nerrad").await.unwrap();
        }
    });
    let recusa = reserva
        .receber_copia(
            PedidoDeCopia {
                porta_local: porta,
                codigo: "c".repeat(32),
                sha256: "b".repeat(64),
                tamanho: 5,
                destino: "r.tar.gz".into(),
            },
            |_, _| {},
        )
        .await
        .unwrap_err();
    assert!(matches!(recusa, Recusa::CopiaNaoBate { .. }), "{recusa:?}");
    assert_eq!(
        std::fs::read(reserva.raiz().join("r.tar.gz")).unwrap(),
        b"a de antes"
    );
    let parciais = std::fs::read_dir(reserva.raiz())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().contains("recebendo"))
        .count();
    assert_eq!(parciais, 0, "o parcial que não bateu ficou");
}

/// **Sem ninguém do outro lado, a recusa diz quanto chegou e quantas vezes
/// se tentou** — e não fica girando.
#[tokio::test]
async fn sem_quem_ofereca_a_copia_cai_com_os_numeros() {
    let (_b, reserva) = programa("ninguem", 1 << 30, vec![]);
    let recusa = reserva
        .receber_copia(
            PedidoDeCopia {
                porta_local: porta_livre(),
                codigo: "c".repeat(32),
                sha256: "b".repeat(64),
                tamanho: 10,
                destino: "r.tar.gz".into(),
            },
            |_, _| {},
        )
        .await
        .unwrap_err();
    match recusa {
        Recusa::CopiaCaiu {
            recebidos,
            tamanho,
            tentativas,
            ..
        } => {
            assert_eq!((recebidos, tamanho), (0, 10));
            assert_eq!(tentativas, seele_core::programa::MAX_TENTATIVAS_DA_COPIA);
        }
        outra => panic!("{outra:?}"),
    }
}
