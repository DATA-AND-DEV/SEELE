//! Os testes do download com uma origem de mentira, que conta quantos pedidos
//! chegaram e quantos pedaços entregou: é com esses dois números que «recusado
//! antes de qualquer pedido» e «cortado no pedaço que passa» são medidos, e
//! não supostos.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize};
use std::sync::Mutex;

use tokio::sync::Notify;

use super::super::caminho::tests::tempfile_simples::Pasta;
use super::super::{ComandoAceito, Politica};
use super::*;

#[derive(Clone)]
enum Roteiro {
    Corpo {
        alegado: Option<u64>,
        pedacos: Vec<Vec<u8>>,
        /// Repete o último pedaço para sempre.
        sem_fim: bool,
        /// Espera esta porta antes do primeiro pedaço.
        porta: Option<Arc<Notify>>,
    },
    Redireciona(String),
    Status(u16),
}

#[derive(Default)]
struct Mentira {
    rotas: Mutex<HashMap<String, Roteiro>>,
    pedidos: AtomicUsize,
    entregues: Arc<AtomicU64>,
    /// Quantos pedidos falham antes de qualquer resposta, como a conexão que
    /// o CDN da Mojang fecha no primeiro pedido.
    falhas_antes: AtomicUsize,
}

impl Mentira {
    fn com(self, url: &str, roteiro: Roteiro) -> Self {
        self.rotas.lock().unwrap().insert(url.to_owned(), roteiro);
        self
    }
}

struct CorpoDeMentira {
    pedacos: std::collections::VecDeque<Vec<u8>>,
    sem_fim: bool,
    porta: Option<Arc<Notify>>,
    entregues: Arc<AtomicU64>,
}

impl Corpo for CorpoDeMentira {
    fn pedaco(&mut self) -> FuturoDe<'_, Result<Option<Vec<u8>>, String>> {
        Box::pin(async move {
            if let Some(porta) = self.porta.take() {
                porta.notified().await;
            }
            let proximo = if self.sem_fim && self.pedacos.len() == 1 {
                self.pedacos.front().cloned()
            } else {
                self.pedacos.pop_front()
            };
            if proximo.is_some() {
                self.entregues.fetch_add(1, Ordering::SeqCst);
            }
            Ok(proximo)
        })
    }
}

impl Origem for Mentira {
    fn pedir<'a>(&'a self, url: &'a reqwest::Url) -> FuturoDe<'a, Result<Resposta, String>> {
        Box::pin(async move {
            self.pedidos.fetch_add(1, Ordering::SeqCst);
            if self
                .falhas_antes
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                .is_ok()
            {
                return Err("peer closed connection without sending TLS close_notify".to_owned());
            }
            let roteiro = self
                .rotas
                .lock()
                .unwrap()
                .get(url.as_str())
                .cloned()
                .ok_or_else(|| format!("sem rota para {url}"))?;
            Ok(match roteiro {
                Roteiro::Redireciona(para) => Resposta::Redireciona { para },
                Roteiro::Status(s) => Resposta::Status(s),
                Roteiro::Corpo {
                    alegado,
                    pedacos,
                    sem_fim,
                    porta,
                } => Resposta::Corpo {
                    alegado,
                    corpo: Box::new(CorpoDeMentira {
                        pedacos: pedacos.into(),
                        sem_fim,
                        porta,
                        entregues: Arc::clone(&self.entregues),
                    }),
                },
            })
        })
    }
}

const URL: &str = "https://piston-data.mojang.com/v1/server.jar";

fn corpo(bytes: &[u8]) -> Roteiro {
    Roteiro::Corpo {
        alegado: Some(bytes.len() as u64),
        pedacos: bytes.chunks(3).map(<[u8]>::to_vec).collect(),
        sem_fim: false,
        porta: None,
    }
}

fn sha256(bytes: &[u8]) -> HashDeclarado {
    let mut c = Calculo::novo(AlgoritmoDeHash::Sha256);
    c.somar(bytes);
    HashDeclarado {
        algoritmo: AlgoritmoDeHash::Sha256,
        hex: c.hex(),
    }
}

fn hash_de(algoritmo: AlgoritmoDeHash, bytes: &[u8]) -> HashDeclarado {
    let mut c = Calculo::novo(algoritmo);
    c.somar(bytes);
    HashDeclarado {
        algoritmo,
        hex: c.hex(),
    }
}

fn pedido(bytes: &[u8], destino: &str) -> PedidoDeDownload {
    PedidoDeDownload {
        url: URL.to_owned(),
        hashes: vec![sha256(bytes)],
        tamanho: bytes.len() as u64,
        destino: destino.to_owned(),
    }
}

struct Bancada {
    _pasta: Pasta,
    programa: Programa,
    origem: Arc<Mentira>,
}

fn bancada(origem: Mentira, teto: u64) -> Bancada {
    let pasta = Pasta::nova("baixar");
    let origem = Arc::new(origem);
    let programa = Programa::novo(
        Politica {
            dominios: vec!["mojang.com".to_owned()],
            artefatos: vec![],
            de_outros_sistemas: vec![],
            origens: vec!["mojang.com".to_owned()],
            comando: Some(ComandoAceito {
                comando: "java/bin/java".to_owned(),
                argumentos: vec![],
            }),
            instalar: None,
            pack: None,
            teto_de_disco: teto,
            raiz: pasta.caminho().join("jogo"),
        },
        Arc::clone(&origem) as Arc<dyn Origem>,
    )
    .unwrap();
    Bancada {
        _pasta: pasta,
        programa,
        origem,
    }
}

/// Nenhum parcial sobrou em lugar nenhum da raiz.
fn sem_parcial(raiz: &Path) -> bool {
    let mut pendentes = vec![raiz.to_path_buf()];
    while let Some(p) = pendentes.pop() {
        for e in std::fs::read_dir(&p).unwrap().flatten() {
            let c = e.path();
            if c.is_dir() {
                pendentes.push(c);
            } else if e_parcial(&c) {
                return false;
            }
        }
    }
    true
}

#[tokio::test]
async fn baixa_confere_e_renomeia_com_progresso() {
    let bytes = b"um servidor de jogo";
    let b = bancada(Mentira::default().com(URL, corpo(bytes)), 1 << 20);
    let mut download = b
        .programa
        .baixar(pedido(bytes, "server/server.jar"))
        .unwrap();
    let mut eventos = vec![];
    while let Some(e) = download.proximo().await {
        eventos.push(e);
    }
    let destino = b.programa.raiz().join("server").join("server.jar");
    assert_eq!(
        eventos.first(),
        Some(&EventoDoDownload::Progresso {
            recebidos: 0,
            total: 19
        })
    );
    assert!(eventos.contains(&EventoDoDownload::Progresso {
        recebidos: 19,
        total: 19
    }));
    assert_eq!(
        eventos.last(),
        Some(&EventoDoDownload::Pronto(Baixado {
            destino: destino.clone(),
            tamanho: 19,
            algoritmo: AlgoritmoDeHash::Sha256,
            hash: sha256(bytes).hex,
            ja_estava: false,
        }))
    );
    assert_eq!(std::fs::read(&destino).unwrap(), bytes);
    assert!(sem_parcial(b.programa.raiz()));
    assert!(b.programa.recusas().is_empty());
}

#[tokio::test]
async fn hash_que_nao_bate_apaga_e_conta() {
    let bytes = b"adulterado";
    let b = bancada(Mentira::default().com(URL, corpo(bytes)), 1 << 20);
    let mut p = pedido(bytes, "server/server.jar");
    p.hashes = vec![sha256(b"o original")];
    let recusa = b.programa.baixar(p).unwrap().terminar().await.unwrap_err();
    assert!(
        matches!(&recusa, Recusa::HashNaoBate { algoritmo: AlgoritmoDeHash::Sha256, esperado, obtido, .. }
            if *esperado == sha256(b"o original").hex && *obtido == sha256(bytes).hex),
        "{recusa:?}"
    );
    assert!(!b.programa.raiz().join("server/server.jar").exists());
    assert!(sem_parcial(b.programa.raiz()));
    assert_eq!(b.programa.recusas().get("hash-nao-bate"), Some(&1));
}

/// O `.mrpack` dá `sha1` e `sha512`: vale o `sha512`, nos dois sentidos.
#[tokio::test]
async fn confere_o_mais_forte_e_ignora_o_mais_fraco() {
    let bytes = b"pack";
    let errado_fraco = HashDeclarado {
        algoritmo: AlgoritmoDeHash::Sha1,
        hex: "0".repeat(40),
    };
    let b = bancada(Mentira::default().com(URL, corpo(bytes)), 1 << 20);
    let mut p = pedido(bytes, "pack.mrpack");
    p.hashes = vec![errado_fraco, hash_de(AlgoritmoDeHash::Sha512, bytes)];
    let feito = b.programa.baixar(p).unwrap().terminar().await.unwrap();
    assert_eq!(feito.algoritmo, AlgoritmoDeHash::Sha512);

    let mut p = pedido(bytes, "pack2.mrpack");
    p.hashes = vec![
        hash_de(AlgoritmoDeHash::Sha1, bytes),
        HashDeclarado {
            algoritmo: AlgoritmoDeHash::Sha512,
            hex: "F".repeat(128),
        },
    ];
    let recusa = b.programa.baixar(p).unwrap().terminar().await.unwrap_err();
    assert!(
        matches!(
            recusa,
            Recusa::HashNaoBate {
                algoritmo: AlgoritmoDeHash::Sha512,
                ..
            }
        ),
        "{recusa:?}"
    );
}

/// O corpo que não acaba é cortado no pedaço que passa do declarado.
///
/// A origem entrega pedaços de 1000 bytes para sempre; o declarado é 10 000.
/// O corte tem de acontecer no 11.º pedaço — e não nunca, que é o que um
/// «ler tudo e medir depois» faria aqui.
#[tokio::test]
async fn corpo_maior_e_cortado_no_pedaco_que_passa() {
    let b = bancada(
        Mentira::default().com(
            URL,
            Roteiro::Corpo {
                alegado: None,
                pedacos: vec![vec![7; 1000]],
                sem_fim: true,
                porta: None,
            },
        ),
        1 << 30,
    );
    let mut p = pedido(b"", "server/grande.bin");
    p.tamanho = 10_000;
    p.hashes = vec![sha256(b"qualquer")];
    let recusa = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        b.programa.baixar(p).unwrap().terminar(),
    )
    .await
    .expect("o corte não aconteceu: o download leu sem parar")
    .unwrap_err();
    assert_eq!(
        recusa,
        Recusa::MaiorQueODeclarado {
            url: URL.to_owned(),
            declarado: 10_000,
            recebido: 11_000,
        }
    );
    assert_eq!(b.origem.entregues.load(Ordering::SeqCst), 11);
    assert!(sem_parcial(b.programa.raiz()));
    assert!(!b.programa.raiz().join("server/grande.bin").exists());
    assert_eq!(b.programa.recusas().get("maior-que-o-declarado"), Some(&1));
}

#[tokio::test]
async fn corpo_menor_e_recusado_e_apagado() {
    let bytes = b"curto";
    let b = bancada(
        Mentira::default().com(
            URL,
            Roteiro::Corpo {
                alegado: None,
                pedacos: vec![bytes.to_vec()],
                sem_fim: false,
                porta: None,
            },
        ),
        1 << 20,
    );
    let mut p = pedido(bytes, "x.bin");
    p.tamanho = 100;
    let recusa = b.programa.baixar(p).unwrap().terminar().await.unwrap_err();
    assert_eq!(
        recusa,
        Recusa::MenorQueODeclarado {
            url: URL.to_owned(),
            declarado: 100,
            recebido: 5
        }
    );
    assert!(sem_parcial(b.programa.raiz()));
}

#[tokio::test]
async fn tamanho_alegado_diferente_recusa_antes_de_ler() {
    let b = bancada(
        Mentira::default().com(
            URL,
            Roteiro::Corpo {
                alegado: Some(5_000_000_000),
                pedacos: vec![vec![0; 10]],
                sem_fim: true,
                porta: None,
            },
        ),
        1 << 20,
    );
    let mut p = pedido(b"", "x.bin");
    p.tamanho = 10;
    p.hashes = vec![sha256(b"x")];
    let recusa = b.programa.baixar(p).unwrap().terminar().await.unwrap_err();
    assert_eq!(
        recusa,
        Recusa::TamanhoAlegadoDiferente {
            url: URL.to_owned(),
            declarado: 10,
            alegado: 5_000_000_000
        }
    );
    assert_eq!(b.origem.entregues.load(Ordering::SeqCst), 0);
}

/// Fora do aceite, nada toca a rede: a origem conta zero pedidos.
#[tokio::test]
async fn url_fora_do_aceite_e_recusada_antes_de_qualquer_pedido() {
    let b = bancada(Mentira::default(), 1 << 20);
    let casos = [
        ("https://evilmojang.com/x", "dominio-fora-do-aceito"),
        ("https://mojang.com.evil.net/x", "dominio-fora-do-aceito"),
        ("https://example.org/x", "dominio-fora-do-aceito"),
        ("http://piston-data.mojang.com/x", "nao-e-https"),
        ("file:///etc/passwd", "nao-e-https"),
        ("não é url", "url-invalida"),
    ];
    for (url, nome) in casos {
        let mut p = pedido(b"x", "x.bin");
        p.url = url.to_owned();
        let recusa = b.programa.baixar(p).unwrap_err();
        assert_eq!(recusa.nome(), nome, "{url}: {recusa:?}");
    }
    let recusa = {
        let mut p = pedido(b"x", "x.bin");
        p.url = "https://example.org/x".to_owned();
        b.programa.baixar(p).unwrap_err()
    };
    assert_eq!(
        recusa,
        Recusa::DominioForaDoAceito {
            host: "example.org".to_owned(),
            aceitos: vec!["mojang.com".to_owned()]
        }
    );
    assert_eq!(b.origem.pedidos.load(Ordering::SeqCst), 0);
    assert_eq!(b.programa.recusas().get("dominio-fora-do-aceito"), Some(&4));
}

#[tokio::test]
async fn sem_hash_ou_hash_mal_formado_e_recusado_antes() {
    let b = bancada(Mentira::default(), 1 << 20);
    let mut p = pedido(b"x", "x.bin");
    p.hashes.clear();
    assert_eq!(b.programa.baixar(p).unwrap_err().nome(), "sem-hash");
    let mut p = pedido(b"x", "x.bin");
    p.hashes = vec![HashDeclarado {
        algoritmo: AlgoritmoDeHash::Sha512,
        hex: "abc".to_owned(),
    }];
    assert_eq!(b.programa.baixar(p).unwrap_err().nome(), "hash-mal-formado");
    assert_eq!(b.origem.pedidos.load(Ordering::SeqCst), 0);
}

/// Spec, «Como provar»: `..` e caminho absoluto recusados em `baixar`.
#[tokio::test]
async fn destino_fora_da_raiz_e_recusado_antes() {
    let b = bancada(Mentira::default(), 1 << 20);
    for (destino, nome) in [
        ("../fora.jar", "caminho-com-pai"),
        ("server/../../fora.jar", "caminho-com-pai"),
        ("/etc/fora.jar", "caminho-absoluto"),
        ("C:\\fora.jar", "caminho-absoluto"),
        ("", "caminho-vazio"),
    ] {
        let recusa = b.programa.baixar(pedido(b"x", destino)).unwrap_err();
        assert_eq!(recusa.nome(), nome, "{destino}: {recusa:?}");
    }
    assert_eq!(b.origem.pedidos.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn redirecionamento_dentro_do_aceite_e_seguido() {
    let bytes = b"cdn";
    let b = bancada(
        Mentira::default()
            .com(
                URL,
                Roteiro::Redireciona("https://cdn.mojang.com/a".to_owned()),
            )
            // Relativo: resolvido contra o salto atual.
            .com(
                "https://cdn.mojang.com/a",
                Roteiro::Redireciona("/b".to_owned()),
            )
            .com("https://cdn.mojang.com/b", corpo(bytes)),
        1 << 20,
    );
    let feito = b
        .programa
        .baixar(pedido(bytes, "x.bin"))
        .unwrap()
        .terminar()
        .await
        .unwrap();
    assert_eq!(feito.tamanho, 3);
    assert_eq!(b.origem.pedidos.load(Ordering::SeqCst), 3);
}

/// Um salto para fora do aceite — outro domínio, ou `http` — é recusado, e o
/// destino de fora **não é pedido**.
#[tokio::test]
async fn redirecionamento_para_fora_e_recusado_sem_pedir_o_destino() {
    for para in ["https://evil.net/x", "http://piston-data.mojang.com/x"] {
        let b = bancada(
            Mentira::default().com(URL, Roteiro::Redireciona(para.to_owned())),
            1 << 20,
        );
        let recusa = b
            .programa
            .baixar(pedido(b"x", "x.bin"))
            .unwrap()
            .terminar()
            .await
            .unwrap_err();
        assert_eq!(
            recusa,
            Recusa::RedirecionouParaFora {
                de: URL.to_owned(),
                para: para.to_owned()
            }
        );
        assert_eq!(b.origem.pedidos.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn redirecionamentos_demais_sao_recusados() {
    let b = bancada(
        Mentira::default().com(URL, Roteiro::Redireciona(URL.to_owned())),
        1 << 20,
    );
    let recusa = b
        .programa
        .baixar(pedido(b"x", "x.bin"))
        .unwrap()
        .terminar()
        .await
        .unwrap_err();
    assert_eq!(
        recusa,
        Recusa::RedirecionamentosDemais {
            url: URL.to_owned(),
            saltos: MAX_REDIRECIONAMENTOS
        }
    );
    assert_eq!(
        b.origem.pedidos.load(Ordering::SeqCst),
        MAX_REDIRECIONAMENTOS + 1
    );
}

#[tokio::test]
async fn status_de_erro_e_dito_com_o_numero() {
    let b = bancada(Mentira::default().com(URL, Roteiro::Status(404)), 1 << 20);
    let recusa = b
        .programa
        .baixar(pedido(b"x", "x.bin"))
        .unwrap()
        .terminar()
        .await
        .unwrap_err();
    assert_eq!(
        recusa,
        Recusa::OrigemRespondeu {
            url: URL.to_owned(),
            status: 404
        }
    );
}

/// O teto de disco recusa com os números, antes do pedido.
#[tokio::test]
async fn disco_cheio_recusa_com_os_numeros_antes_do_pedido() {
    let b = bancada(Mentira::default().com(URL, corpo(&[1; 50])), 100);
    std::fs::write(b.programa.raiz().join("mundo.dat"), [0; 60]).unwrap();
    let recusa = b
        .programa
        .baixar(pedido(&[1; 50], "x.bin"))
        .unwrap()
        .terminar()
        .await
        .unwrap_err();
    assert_eq!(
        recusa,
        Recusa::DiscoCheio {
            usado: 60,
            pedido: 50,
            teto: 100
        }
    );
    assert_eq!(b.origem.pedidos.load(Ordering::SeqCst), 0);

    // Substituir o próprio arquivo cabe: o velho sai quando o novo entra.
    let feito = b
        .programa
        .baixar(pedido(&[1; 50], "mundo.dat"))
        .unwrap()
        .terminar()
        .await;
    assert!(feito.is_ok(), "{feito:?}");
}

/// Dois downloads ao mesmo tempo não cabem os dois só porque cada um olhou o
/// disco vazio: o primeiro reserva o tamanho dele.
#[tokio::test]
async fn downloads_simultaneos_reservam_o_disco() {
    let porta = Arc::new(Notify::new());
    let b = bancada(
        Mentira::default()
            .com(
                URL,
                Roteiro::Corpo {
                    alegado: Some(60),
                    pedacos: vec![vec![1; 60]],
                    sem_fim: false,
                    porta: Some(Arc::clone(&porta)),
                },
            )
            .com("https://mojang.com/b", corpo(&[2; 60])),
        100,
    );
    let primeiro = b.programa.baixar(pedido(&[1; 60], "a.bin")).unwrap();
    // Até o primeiro estar parado na porta, com a reserva feita.
    while b.origem.pedidos.load(Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
    }
    let mut p = pedido(&[2; 60], "b.bin");
    p.url = "https://mojang.com/b".to_owned();
    let recusa = b.programa.baixar(p).unwrap().terminar().await.unwrap_err();
    assert_eq!(
        recusa,
        Recusa::DiscoCheio {
            usado: 60,
            pedido: 60,
            teto: 100
        }
    );
    porta.notify_one();
    assert!(primeiro.terminar().await.is_ok());
}

/// Encerrar no meio apaga o parcial e solta a reserva.
#[tokio::test]
async fn encerrar_no_meio_apaga_o_parcial() {
    // Um corpo que nunca termina, para encerrar com o parcial no disco.
    let b = bancada(
        Mentira::default().com(
            URL,
            Roteiro::Corpo {
                alegado: None,
                pedacos: vec![vec![1; 10]],
                sem_fim: true,
                porta: None,
            },
        ),
        1 << 40,
    );
    let mut p = pedido(b"", "server/x.bin");
    p.tamanho = u64::from(u32::MAX);
    p.hashes = vec![sha256(b"x")];
    let mut download = b.programa.baixar(p).unwrap();
    // Espera o primeiro progresso: o parcial existe.
    let _ = download.proximo().await;
    while b.origem.entregues.load(Ordering::SeqCst) < 3 {
        tokio::task::yield_now().await;
    }
    assert!(!sem_parcial(b.programa.raiz()), "o parcial devia existir");
    b.programa.encerrar_tudo().await;
    let fim = download.terminar().await;
    assert_eq!(fim, Err(Recusa::Encerrado));
    // O abort solta a tarefa na próxima volta do runtime.
    for _ in 0..100 {
        if sem_parcial(b.programa.raiz()) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(sem_parcial(b.programa.raiz()));
    assert_eq!(*b.programa.interior.reservado.lock().unwrap(), 0);
    // E nada novo começa.
    assert_eq!(
        b.programa.baixar(pedido(b"x", "y.bin")).unwrap_err(),
        Recusa::Encerrado
    );
}

/// Uma bancada cuja política tem um artefato aceito (`URL`, com o hash e o
/// tamanho de `bytes`), um de outro sistema, e `origens` só com o CDN.
fn bancada_com_aceite(origem: Mentira, bytes: &[u8]) -> Bancada {
    let pasta = Pasta::nova("aceite");
    let origem = Arc::new(origem);
    let programa = Programa::novo(
        Politica {
            dominios: vec![
                "piston-data.mojang.com".to_owned(),
                "github.com".to_owned(),
                "cdn.modrinth.com".to_owned(),
            ],
            artefatos: vec![super::super::ArtefatoAceito {
                url: URL.to_owned(),
                hash: sha256(bytes),
                tamanho: bytes.len() as u64,
            }],
            de_outros_sistemas: vec!["https://github.com/win.zip".to_owned()],
            origens: vec!["cdn.modrinth.com".to_owned()],
            comando: None,
            instalar: None,
            pack: None,
            teto_de_disco: 1 << 20,
            raiz: pasta.caminho().join("jogo"),
        },
        Arc::clone(&origem) as Arc<dyn Origem>,
    )
    .unwrap();
    Bancada {
        _pasta: pasta,
        programa,
        origem,
    }
}

/// **O aceite prende o artefato mostrado** (spec §4, decisão Q1): a URL do
/// `server.jar` aceito só baixa com o hash e o tamanho que a tela mostrou;
/// outro hash, ou outro tamanho, é recusado antes de qualquer pedido.
#[tokio::test]
async fn o_artefato_aceito_so_baixa_como_foi_mostrado() {
    let bytes = b"o server.jar mostrado";
    let outro = b"um server.jar que ninguem viu";
    let b = bancada_com_aceite(Mentira::default().com(URL, corpo(bytes)), bytes);

    let trocado = PedidoDeDownload {
        hashes: vec![sha256(outro)],
        tamanho: outro.len() as u64,
        ..pedido(bytes, "server.jar")
    };
    match b.programa.baixar(trocado) {
        Err(Recusa::DiferenteDoAceito { campo: "hash", .. }) => {}
        outro => panic!("outro hash na URL aceita passou: {outro:?}"),
    }
    let maior = PedidoDeDownload {
        tamanho: bytes.len() as u64 + 1,
        ..pedido(bytes, "server.jar")
    };
    match b.programa.baixar(maior) {
        Err(Recusa::DiferenteDoAceito {
            campo: "tamanho", ..
        }) => {}
        outro => panic!("outro tamanho na URL aceita passou: {outro:?}"),
    }
    // O sha1 do mesmo arquivo não é o sha256 que a tela mostrou.
    let mais_fraco = PedidoDeDownload {
        hashes: vec![hash_de(AlgoritmoDeHash::Sha1, bytes)],
        ..pedido(bytes, "server.jar")
    };
    assert!(matches!(
        b.programa.baixar(mais_fraco),
        Err(Recusa::DiferenteDoAceito { campo: "hash", .. })
    ));
    assert_eq!(b.origem.pedidos.load(Ordering::SeqCst), 0, "pediu à origem");
    assert_eq!(b.programa.recusas().get("diferente-do-aceito"), Some(&3));

    let pronto = b
        .programa
        .baixar(pedido(bytes, "server.jar"))
        .unwrap()
        .terminar()
        .await
        .unwrap();
    assert_eq!(std::fs::read(pronto.destino).unwrap(), bytes);
}

/// **Uma URL fora dos artefatos só baixa de uma origem**: do host de um
/// artefato aceito (`piston-data.mojang.com`), uma URL que não é a dele é
/// recusada; da origem (`cdn.modrinth.com`), vale com o hash declarado — o que
/// a parte 6 precisa para os arquivos de um modpack.
#[tokio::test]
async fn fora_dos_artefatos_so_das_origens() {
    let bytes = b"um mod do pack";
    let do_cdn = "https://cdn.modrinth.com/data/x/mod.jar";
    let vizinho = "https://piston-data.mojang.com/v1/outro.jar";
    let b = bancada_com_aceite(
        Mentira::default()
            .com(do_cdn, corpo(bytes))
            .com(vizinho, corpo(bytes)),
        b"o server.jar mostrado",
    );
    let de = |url: &str| PedidoDeDownload {
        url: url.to_owned(),
        ..pedido(bytes, "mods/mod.jar")
    };
    match b.programa.baixar(de(vizinho)) {
        Err(Recusa::ForaDosArtefatosAceitos { origens, .. }) => {
            assert_eq!(origens, vec!["cdn.modrinth.com".to_owned()]);
        }
        outro => panic!("uma URL do host do artefato, sem ser ele, passou: {outro:?}"),
    }
    match b.programa.baixar(de("https://github.com/win.zip")) {
        Err(Recusa::ArtefatoDeOutroSistema { .. }) => {}
        outro => panic!("o Java de outro sistema passou: {outro:?}"),
    }
    assert_eq!(b.origem.pedidos.load(Ordering::SeqCst), 0);
    b.programa
        .baixar(de(do_cdn))
        .unwrap()
        .terminar()
        .await
        .unwrap();
    let recusas = b.programa.recusas();
    assert_eq!(recusas.get("fora-dos-artefatos-aceitos"), Some(&1));
    assert_eq!(recusas.get("artefato-de-outro-sistema"), Some(&1));
}

/// **Uma falha antes de qualquer resposta é tentada de novo** (decisão Q10):
/// o primeiro pedido ao CDN da Mojang morre às vezes no TLS, e o segundo passa.
#[tokio::test]
async fn a_conexao_que_cai_antes_da_resposta_e_tentada_de_novo() {
    let bytes = b"um servidor de jogo";
    let origem = Mentira::default().com(URL, corpo(bytes));
    origem.falhas_antes.store(2, Ordering::SeqCst);
    let b = bancada(origem, 1 << 20);
    let pronto = b
        .programa
        .baixar(pedido(bytes, "server.jar"))
        .unwrap()
        .terminar()
        .await;
    assert!(pronto.is_ok(), "{pronto:?}");
    assert_eq!(b.origem.pedidos.load(Ordering::SeqCst), 3);
    assert!(b.programa.recusas().is_empty());
}

/// E tem teto: depois de [`MAX_TENTATIVAS`], a recusa diz quantas foram e por
/// quê, e é contada uma vez.
#[tokio::test]
async fn as_tentativas_tem_teto_e_a_recusa_diz_quantas() {
    let bytes = b"um servidor de jogo";
    let origem = Mentira::default().com(URL, corpo(bytes));
    origem.falhas_antes.store(10, Ordering::SeqCst);
    let b = bancada(origem, 1 << 20);
    match b
        .programa
        .baixar(pedido(bytes, "server.jar"))
        .unwrap()
        .terminar()
        .await
    {
        Err(Recusa::OrigemFalhou { motivo, .. }) => {
            assert!(motivo.starts_with("3 tentativas: peer closed"), "{motivo}");
        }
        outro => panic!("{outro:?}"),
    }
    assert_eq!(
        b.origem.pedidos.load(Ordering::SeqCst) as u32,
        super::MAX_TENTATIVAS
    );
    assert_eq!(b.programa.recusas().get("origem-falhou"), Some(&1));
}

async fn ate_o_fim(mut download: Download) -> Vec<EventoDoDownload> {
    let mut eventos = vec![];
    while let Some(e) = download.proximo().await {
        eventos.push(e);
    }
    eventos
}

/// **O arquivo que já está, com o tamanho e o hash declarados, não é baixado
/// de novo** (API 8 parte 6): o MOD de multiplayer baixava ~105 MB a cada
/// partida do app. Medido pelo número de pedidos à origem, e não suposto.
#[tokio::test]
async fn o_arquivo_que_ja_esta_nao_e_baixado_de_novo() {
    let bytes = b"um servidor de jogo";
    let b = bancada(Mentira::default().com(URL, corpo(bytes)), 1 << 20);
    let primeiro = ate_o_fim(b.programa.baixar(pedido(bytes, "server.jar")).unwrap()).await;
    assert!(
        matches!(
            primeiro.last(),
            Some(EventoDoDownload::Pronto(Baixado {
                ja_estava: false,
                ..
            }))
        ),
        "{primeiro:?}"
    );
    assert_eq!(b.origem.pedidos.load(Ordering::SeqCst), 1);

    let segundo = ate_o_fim(b.programa.baixar(pedido(bytes, "server.jar")).unwrap()).await;
    assert!(
        matches!(
            segundo.last(),
            Some(EventoDoDownload::Pronto(Baixado {
                ja_estava: true,
                ..
            }))
        ),
        "{segundo:?}"
    );
    assert_eq!(
        b.origem.pedidos.load(Ordering::SeqCst),
        1,
        "o arquivo que já estava foi pedido à rede de novo"
    );
}

/// **O que está no disco e não bate é baixado**: o «já está» confere o hash,
/// e não só o nome e o tamanho.
#[tokio::test]
async fn o_arquivo_que_esta_mas_nao_bate_e_baixado() {
    let bytes = b"um servidor de jogo";
    let b = bancada(Mentira::default().com(URL, corpo(bytes)), 1 << 20);
    std::fs::write(b.programa.raiz().join("server.jar"), b"um servidor de JOGO").unwrap();
    let eventos = ate_o_fim(b.programa.baixar(pedido(bytes, "server.jar")).unwrap()).await;
    assert!(
        matches!(
            eventos.last(),
            Some(EventoDoDownload::Pronto(Baixado {
                ja_estava: false,
                ..
            }))
        ),
        "{eventos:?}"
    );
    assert_eq!(b.origem.pedidos.load(Ordering::SeqCst), 1);
    assert_eq!(
        std::fs::read(b.programa.raiz().join("server.jar")).unwrap(),
        bytes
    );
}
