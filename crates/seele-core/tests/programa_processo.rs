//! `programa.rodar` com um processo de verdade, no Mac e no Windows.
//!
//! O processo é o `seele-programa-de-teste` deste crate, e não `sh`: o
//! Windows não tem `sh`, e a spec pede estes testes nos dois sistemas, fora de
//! qualquer `#![cfg(windows)]`.
//!
//! «Morreu» é medido pelo batimento: o programa de teste escreve um contador
//! num arquivo a cada 20 ms, e um contador que parou de andar é um processo
//! que parou. Nenhum `kill -0`, nenhuma FFI.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use seele_core::programa::{
    ComandoAceito, EventoDoProcesso, FimDoProcesso, Origem, OrigemHttps, PedidoDeExecucao,
    Politica, Processo, Programa, Recusa, MAX_BYTES_POR_LINHA,
};

const COMANDO: &str = "bin/servidor";

struct Pasta(PathBuf);

impl Drop for Pasta {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn pasta(nome: &str) -> Pasta {
    static N: AtomicU64 = AtomicU64::new(0);
    let caminho = std::env::temp_dir().join(format!(
        "seele-programa-proc-{nome}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&caminho);
    std::fs::create_dir_all(&caminho).unwrap();
    Pasta(caminho)
}

/// Um `Programa` cuja raiz tem o programa de teste em `bin/servidor`, e cujo
/// aceite é esse comando com `argumentos`.
fn programa(nome: &str, argumentos: &[&str]) -> (Pasta, Programa) {
    let p = pasta(nome);
    let raiz = p.0.join("jogo");
    std::fs::create_dir_all(raiz.join("bin")).unwrap();
    std::fs::create_dir_all(raiz.join("server")).unwrap();
    let alvo = raiz
        .join("bin")
        .join(format!("servidor{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(env!("CARGO_BIN_EXE_seele-programa-de-teste"), &alvo).unwrap();
    let politica = Politica {
        dominios: vec![],
        artefatos: vec![],
        de_outros_sistemas: vec![],
        origens: vec![],
        comando: Some(ComandoAceito {
            comando: COMANDO.to_owned(),
            argumentos: argumentos.iter().map(|a| (*a).to_owned()).collect(),
        }),
        instalar: None,
        pack: None,
        teto_de_disco: 1 << 30,
        portas_de_copia: vec![],
        raiz,
    };
    let origem: Arc<dyn Origem> = Arc::new(OrigemHttps::nova().unwrap());
    (p, Programa::novo(politica, origem).unwrap())
}

fn pedido(argumentos: &[&str], pasta: &str) -> PedidoDeExecucao {
    PedidoDeExecucao {
        comando: COMANDO.to_owned(),
        argumentos: argumentos.iter().map(|a| (*a).to_owned()).collect(),
        pasta: pasta.to_owned(),
    }
}

async fn proximo(processo: &mut Processo) -> EventoDoProcesso {
    tokio::time::timeout(Duration::from_secs(20), processo.proximo())
        .await
        .expect("o processo ficou calado")
        .expect("os eventos acabaram antes do fim")
}

async fn esperar_o_fim(processo: &mut Processo) -> (Vec<EventoDoProcesso>, FimDoProcesso) {
    let mut antes = vec![];
    loop {
        match proximo(processo).await {
            EventoDoProcesso::Terminou(fim) => return (antes, fim),
            outro => antes.push(outro),
        }
    }
}

fn batimento(arquivo: &Path) -> u64 {
    // O programa de teste reescreve o arquivo (`fs::write` trunca antes de
    // escrever): uma leitura no meio vê o arquivo vazio, e isso não é «parou».
    for _ in 0..20 {
        if let Some(n) = std::fs::read_to_string(arquivo)
            .ok()
            .and_then(|t| t.trim().parse().ok())
        {
            return n;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    0
}

/// Espera o batimento começar, e devolve se ele anda.
async fn esta_vivo(arquivo: &Path) -> bool {
    let antes = batimento(arquivo);
    tokio::time::sleep(Duration::from_millis(300)).await;
    batimento(arquivo) > antes
}

async fn esperar_o_primeiro_batimento(arquivo: &Path) {
    for _ in 0..500 {
        if batimento(arquivo) > 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("o programa de teste não começou a bater");
}

#[tokio::test]
async fn le_a_saida_e_o_erro_escreve_na_entrada_e_diz_o_codigo() {
    let (_p, programa) = programa("eco", &["eco"]);
    let mut processo = programa.rodar(&pedido(&["eco"], "")).unwrap();
    assert!(processo.pid().is_some());

    let mut vistos = vec![proximo(&mut processo).await, proximo(&mut processo).await];
    vistos.sort_by_key(|e| format!("{e:?}"));
    assert_eq!(
        vistos,
        vec![
            EventoDoProcesso::Erro("aviso no erro".to_owned()),
            EventoDoProcesso::Saida("pronto".to_owned()),
        ]
    );

    processo.escrever("save-all").await.unwrap();
    assert_eq!(
        proximo(&mut processo).await,
        EventoDoProcesso::Saida("salvo".to_owned())
    );
    processo.escrever("sair 7\n").await.unwrap();
    let (resto, fim) = esperar_o_fim(&mut processo).await;
    assert!(resto.is_empty(), "{resto:?}");
    assert_eq!(
        fim,
        FimDoProcesso {
            codigo: Some(7),
            morto_a_pedido: false,
            linhas_perdidas: 0,
            linhas_cortadas: 0,
        }
    );
    assert_eq!(programa.processos_vivos(), 0);

    // A entrada de um processo que terminou recusa, com o motivo.
    let recusa = processo.escrever("save-all").await.unwrap_err();
    assert_eq!(recusa.nome(), "entrada-fechada");
}

/// Os argumentos vão como vetor, um a um, sem shell: o `;`, as aspas e o
/// espaço chegam como estão, e nada é executado.
#[tokio::test]
async fn os_argumentos_chegam_um_a_um_sem_shell() {
    let estranhos = [
        "argumentos",
        "a b; echo injetado",
        "\"aspas\"",
        "$HOME",
        "%PATH%",
    ];
    let (_p, programa) = programa("args", &estranhos);
    let mut processo = programa.rodar(&pedido(&estranhos, "")).unwrap();
    let (linhas, fim) = esperar_o_fim(&mut processo).await;
    assert_eq!(fim.codigo, Some(0));
    let esperado: Vec<EventoDoProcesso> = estranhos[1..]
        .iter()
        .map(|a| EventoDoProcesso::Saida((*a).to_owned()))
        .collect();
    assert_eq!(linhas, esperado);
}

#[tokio::test]
async fn comando_ou_argumentos_fora_do_aceite_sao_recusados_e_contados() {
    let (_p, programa) = programa("fora", &["eco"]);
    let outro = PedidoDeExecucao {
        comando: "bin/outro".to_owned(),
        ..pedido(&["eco"], "")
    };
    assert_eq!(
        programa.rodar(&outro).unwrap_err(),
        Recusa::ComandoForaDoAceito {
            pedido: "bin/outro".to_owned(),
            aceito: Some(COMANDO.to_owned()),
        }
    );
    let sh = PedidoDeExecucao {
        comando: "/bin/sh".to_owned(),
        ..pedido(&["eco"], "")
    };
    assert_eq!(
        programa.rodar(&sh).unwrap_err().nome(),
        "comando-fora-do-aceito"
    );
    assert_eq!(
        programa.rodar(&pedido(&["eco", "--mais"], "")).unwrap_err(),
        Recusa::ArgumentosForaDoAceito {
            pedidos: vec!["eco".to_owned(), "--mais".to_owned()],
            aceitos: vec!["eco".to_owned()],
        }
    );
    assert_eq!(programa.processos_vivos(), 0);
    let recusas = programa.recusas();
    assert_eq!(recusas.get("comando-fora-do-aceito"), Some(&2));
    assert_eq!(recusas.get("argumentos-fora-do-aceito"), Some(&1));
}

/// Spec, «Como provar»: `..` e caminho absoluto recusados em `rodar`.
#[tokio::test]
async fn pasta_fora_da_raiz_e_recusada() {
    let (_p, programa) = programa("pasta", &["eco"]);
    for (pasta, nome) in [
        ("..", "caminho-com-pai"),
        ("server/../..", "caminho-com-pai"),
        ("/tmp", "caminho-absoluto"),
        ("C:\\Windows", "caminho-absoluto"),
        // E uma pasta dentro da raiz também: o comando aceito roda na raiz
        // (revisão final, I1).
        ("server", "pasta-fora-do-aceite"),
        ("server/mundo/", "pasta-fora-do-aceite"),
    ] {
        let recusa = programa.rodar(&pedido(&["eco"], pasta)).unwrap_err();
        assert_eq!(recusa.nome(), nome, "{pasta}: {recusa:?}");
    }
    assert_eq!(programa.processos_vivos(), 0);
}

/// E um aceite que já veio com `..` no comando não escapa por isso.
#[tokio::test]
async fn comando_aceito_com_pai_e_recusado_mesmo_assim() {
    let p = pasta("comando-pai");
    let politica = Politica {
        dominios: vec![],
        artefatos: vec![],
        de_outros_sistemas: vec![],
        origens: vec![],
        comando: Some(ComandoAceito {
            comando: "../fora".to_owned(),
            argumentos: vec![],
        }),
        instalar: None,
        pack: None,
        teto_de_disco: 1,
        portas_de_copia: vec![],
        raiz: p.0.join("jogo"),
    };
    let programa = Programa::novo(politica, Arc::new(OrigemHttps::nova().unwrap())).unwrap();
    let recusa = programa
        .rodar(&PedidoDeExecucao {
            comando: "../fora".to_owned(),
            argumentos: vec![],
            pasta: String::new(),
        })
        .unwrap_err();
    assert_eq!(recusa.nome(), "caminho-com-pai");
}

#[tokio::test]
async fn matar_a_pedido_para_o_processo_e_diz_que_foi_a_pedido() {
    let p = pasta("matar");
    let arquivo = p.0.join("batimento");
    let argumento = arquivo.to_string_lossy().into_owned();
    let (_q, programa) = programa("matar", &["batimento", &argumento]);
    let mut processo = programa
        .rodar(&pedido(&["batimento", &argumento], ""))
        .unwrap();
    esperar_o_primeiro_batimento(&arquivo).await;
    assert!(esta_vivo(&arquivo).await);

    processo.matar();
    let (_, fim) = esperar_o_fim(&mut processo).await;
    assert!(fim.morto_a_pedido);
    assert!(!esta_vivo(&arquivo).await, "o processo continuou batendo");
    assert_eq!(programa.processos_vivos(), 0);
}

/// Soltar o `Programa` (o MOD desligado, a autorização revogada) mata o
/// processo, sem ninguém chamar `matar`.
#[tokio::test]
async fn soltar_o_programa_mata_o_processo() {
    let p = pasta("soltar");
    let arquivo = p.0.join("batimento");
    let argumento = arquivo.to_string_lossy().into_owned();
    let (_q, programa) = programa("soltar", &["batimento", &argumento]);
    let mut processo = programa
        .rodar(&pedido(&["batimento", &argumento], ""))
        .unwrap();
    esperar_o_primeiro_batimento(&arquivo).await;
    drop(programa);
    let (_, fim) = esperar_o_fim(&mut processo).await;
    assert!(fim.morto_a_pedido);
    assert!(!esta_vivo(&arquivo).await, "o processo ficou órfão");
}

/// Encerrar mata **e espera**: quando volta, o processo já parou, sem
/// precisar ler evento nenhum.
#[tokio::test]
async fn encerrar_tudo_mata_espera_e_fecha_a_porta() {
    let p = pasta("encerrar");
    let arquivo = p.0.join("batimento");
    let argumento = arquivo.to_string_lossy().into_owned();
    let (_q, programa) = programa("encerrar", &["batimento", &argumento]);
    let mut processo = programa
        .rodar(&pedido(&["batimento", &argumento], ""))
        .unwrap();
    esperar_o_primeiro_batimento(&arquivo).await;
    assert_eq!(programa.processos_vivos(), 1);
    assert_eq!(programa.encerrar_tudo().await, 1);
    assert_eq!(programa.processos_vivos(), 0);
    // «Espera» medido sem relógio: o `Terminou` já está na fila quando
    // `encerrar_tudo` volta. Um prazo zero só olha o que já chegou — o
    // `timeout` do Tokio consulta o futuro antes do prazo. A primeira versão
    // deste teste olhava só o batimento 300 ms depois, e passava com a espera
    // revertida (só mandar matar): o tempo da asserção dava tempo ao kill.
    let fim = tokio::time::timeout(Duration::ZERO, processo.proximo())
        .await
        .expect("encerrar_tudo voltou antes de o processo terminar");
    assert!(
        matches!(
            fim,
            Some(EventoDoProcesso::Terminou(FimDoProcesso {
                morto_a_pedido: true,
                ..
            }))
        ),
        "{fim:?}"
    );
    assert!(
        !esta_vivo(&arquivo).await,
        "encerrar voltou com o processo vivo"
    );
    assert_eq!(
        programa
            .rodar(&pedido(&["batimento", &argumento], ""))
            .unwrap_err(),
        Recusa::Encerrado
    );
}

/// O runtime caindo (o app fechando sem encerrar) também não deixa órfão: a
/// vigia é solta, e o filho nasceu com `kill_on_drop`.
#[test]
fn o_runtime_caindo_mata_o_processo() {
    let p = pasta("runtime");
    let arquivo = p.0.join("batimento");
    let argumento = arquivo.to_string_lossy().into_owned();
    let (_q, programa) = programa("runtime", &["batimento", &argumento]);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let processo = runtime.block_on(async {
        let processo = programa
            .rodar(&pedido(&["batimento", &argumento], ""))
            .unwrap();
        esperar_o_primeiro_batimento(&arquivo).await;
        processo
    });
    // O `Programa` fica vivo de propósito: só o runtime cai.
    drop(runtime);
    let antes = batimento(&arquivo);
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(
        batimento(&arquivo),
        antes,
        "o processo sobreviveu ao runtime"
    );
    drop(processo);
    drop(programa);
}

/// A fila tem teto e a linha também; o que passa é perdido ou cortado, e
/// **contado** — e o processo não para por isso.
#[tokio::test]
async fn saida_demais_e_perdida_e_contada_sem_parar_o_processo() {
    let (_p, programa) = programa("muitas", &["eco"]);
    let mut processo = programa.rodar(&pedido(&["eco"], "")).unwrap();
    let total: u64 = 20_000;
    processo.escrever(&format!("muitas {total}")).await.unwrap();
    processo
        .escrever(&format!("longa {}", MAX_BYTES_POR_LINHA * 3))
        .await
        .unwrap();
    processo.escrever("sair 0").await.unwrap();
    // Sem ler nada até aqui: a fila enche e o processo tem de terminar mesmo
    // assim.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let (eventos, fim) = esperar_o_fim(&mut processo).await;
    assert_eq!(fim.codigo, Some(0));
    let saidas = eventos
        .iter()
        .filter(|e| matches!(e, EventoDoProcesso::Saida(_)))
        .count() as u64;
    // `pronto` + as `total` + a longa; o `aviso no erro` vai pelo erro.
    let erros = eventos.len() as u64 - saidas;
    assert_eq!(
        saidas + erros + fim.linhas_perdidas,
        1 + total + 1 + 1,
        "cada linha ou chegou ou foi contada como perdida"
    );
    assert!(fim.linhas_perdidas > 0, "a fila devia ter enchido");
    assert_eq!(fim.linhas_cortadas, 1);
}

/// A linha longa chega cortada no teto, e a seguinte chega inteira.
#[tokio::test]
async fn linha_longa_chega_cortada_e_a_seguinte_inteira() {
    let (_p, programa) = programa("longa", &["eco"]);
    let mut processo = programa.rodar(&pedido(&["eco"], "")).unwrap();
    processo
        .escrever(&format!("longa {}", MAX_BYTES_POR_LINHA + 10))
        .await
        .unwrap();
    processo.escrever("save-all").await.unwrap();
    processo.escrever("sair 0").await.unwrap();
    let (eventos, fim) = esperar_o_fim(&mut processo).await;
    let saidas: Vec<&String> = eventos
        .iter()
        .filter_map(|e| match e {
            EventoDoProcesso::Saida(s) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(
        saidas.len(),
        3,
        "{:?}",
        saidas.iter().map(|s| s.len()).collect::<Vec<_>>()
    );
    assert_eq!(saidas[1].len(), MAX_BYTES_POR_LINHA);
    assert_eq!(saidas[2], "salvo");
    assert_eq!(fim.linhas_cortadas, 1);
}

/// **O passo de instalar é um segundo comando aceito** (API 8 parte 6): o
/// instalador do loader roda com os argumentos dele, o servidor com os dele, e
/// nada que misture os dois (o mesmo programa com outros argumentos) passa.
#[tokio::test]
async fn o_passo_de_instalar_e_um_segundo_comando_aceito_e_so_ele() {
    let (p, _) = programa("instalar", &["eco"]);
    let raiz = p.0.join("jogo");
    let politica = Politica {
        dominios: vec![],
        artefatos: vec![],
        de_outros_sistemas: vec![],
        origens: vec![],
        comando: Some(ComandoAceito {
            comando: COMANDO.to_owned(),
            argumentos: vec!["eco".to_owned()],
        }),
        instalar: Some(ComandoAceito {
            comando: COMANDO.to_owned(),
            argumentos: vec!["argumentos".to_owned(), "instalado".to_owned()],
        }),
        pack: None,
        teto_de_disco: 1 << 30,
        portas_de_copia: vec![],
        raiz,
    };
    let origem: Arc<dyn Origem> = Arc::new(OrigemHttps::nova().unwrap());
    let programa = Programa::novo(politica, origem).unwrap();

    let mut instalador = programa
        .rodar(&pedido(&["argumentos", "instalado"], ""))
        .unwrap();
    let (linhas, fim) = esperar_o_fim(&mut instalador).await;
    assert_eq!(fim.codigo, Some(0));
    assert_eq!(
        linhas,
        vec![EventoDoProcesso::Saida("instalado".to_owned())]
    );

    let mut servidor = programa.rodar(&pedido(&["eco"], "")).unwrap();
    servidor.escrever("sair 0").await.unwrap();
    let _ = esperar_o_fim(&mut servidor).await;

    assert_eq!(
        programa
            .rodar(&pedido(&["argumentos", "outra-coisa"], ""))
            .unwrap_err()
            .nome(),
        "argumentos-fora-do-aceito"
    );
    assert_eq!(
        programa
            .rodar(&PedidoDeExecucao {
                comando: "bin/outro".to_owned(),
                ..pedido(&["eco"], "")
            })
            .unwrap_err()
            .nome(),
        "comando-fora-do-aceito"
    );
    assert_eq!(programa.processos_vivos(), 0);
}

/// **Parar mata os netos** (revisão final, I9; spec §5, «o produto encerra
/// todo processo do MOD»). O filho sobe um neto que bate num arquivo e segue
/// rodando; `matar`, `encerrar_tudo` e soltar o `Programa` param o neto
/// também, e um filho que sai sozinho não deixa o neto para trás. Nos dois
/// sistemas: no Unix pelo grupo de processos (decisão R8), no Windows pela
/// tarefa (*Job Object*, decisão W3) — o programa de teste sobe o neto pela
/// `std`, sem `sh`.
#[tokio::test]
async fn parar_mata_os_netos_por_todos_os_caminhos() {
    for caminho in ["matar", "encerrar", "soltar", "sai-sozinho"] {
        let p = pasta(&format!("neto-{caminho}"));
        let arquivo = p.0.join("batimento-do-neto");
        let argumento = arquivo.to_string_lossy().into_owned();
        let modo = if caminho == "sai-sozinho" {
            "neto-e-sai"
        } else {
            "neto"
        };
        let (_q, programa) = programa(&format!("neto-{caminho}-jogo"), &[modo, &argumento]);
        let mut processo = programa.rodar(&pedido(&[modo, &argumento], "")).unwrap();
        // O neto começou (o filho que sai sozinho só sai depois disso).
        esperar_o_primeiro_batimento(&arquivo).await;
        if caminho != "sai-sozinho" {
            assert!(esta_vivo(&arquivo).await, "{caminho}: o neto não bate");
        }
        match caminho {
            "matar" => {
                processo.matar();
                let _ = esperar_o_fim(&mut processo).await;
            }
            "encerrar" => {
                assert_eq!(programa.encerrar_tudo().await, 1);
            }
            "soltar" => drop(programa),
            _ => {
                let (_, fim) = esperar_o_fim(&mut processo).await;
                assert_eq!(fim.codigo, Some(0), "{caminho}");
            }
        }
        let mut morreu = false;
        for _ in 0..50 {
            if !esta_vivo(&arquivo).await {
                morreu = true;
                break;
            }
        }
        assert!(
            morreu,
            "{caminho}: o neto seguiu batendo depois de o filho parar"
        );
    }
}
