//! **O jogo que sobra de um app que morreu sem desligá-lo** (e2e de 31/10,
//! passo 14): o registro que cada processo deixa, e o app reaberto achando e
//! parando o órfão por ele.
//!
//! Só no Unix: no Windows a tarefa com `KILL_ON_JOB_CLOSE` (decisão W3) mata o
//! jogo quando o app cai, e não há órfão a achar. Este arquivo roda no Mac e no
//! Linux do CI, e não fica atrás de um `cfg` que nunca roda.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use seele_core::programa::{
    anotar_processo, inicio_do_processo, parar_sobras, ComandoAceito, DesfechoDaSobra,
    EventoDoProcesso, Origem, OrigemHttps, PedidoDeExecucao, Politica, Programa,
    RegistroDeProcesso,
};

struct Pasta(PathBuf);

impl Drop for Pasta {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn pasta(nome: &str) -> Pasta {
    static N: AtomicU64 = AtomicU64::new(0);
    let caminho = std::env::temp_dir().join(format!(
        "seele-sobras-{nome}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&caminho);
    std::fs::create_dir_all(&caminho).unwrap();
    Pasta(caminho)
}

/// Um jogo de mentira num grupo próprio, como o `programa` o roda; quem o
/// colhe é uma thread, para ele não ficar zumbi do teste.
fn orfao(script: &str) -> u32 {
    let mut filho = std::process::Command::new("sh")
        .args(["-c", script])
        .process_group(0)
        .spawn()
        .unwrap();
    let pid = filho.id();
    std::thread::spawn(move || {
        let _ = filho.wait();
    });
    // Até o `ps` vê-lo.
    for _ in 0..100 {
        if inicio_do_processo(pid).is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    pid
}

/// Um app dono que já morreu: um pid que saiu.
fn dono_morto() -> (u32, String) {
    let mut filho = std::process::Command::new("true").spawn().unwrap();
    let pid = filho.id();
    filho.wait().unwrap();
    (pid, "Thu Jan  1 00:00:00 1970".to_owned())
}

fn registro(pid: u32, dono: (u32, String)) -> RegistroDeProcesso {
    RegistroDeProcesso {
        pid,
        grupo: pid,
        inicio: inicio_do_processo(pid).expect("o processo de teste não está de pé"),
        dono: dono.0,
        dono_inicio: dono.1,
        etiqueta: r#"{"mod_id":"x/jogo"}"#.to_owned(),
    }
}

fn vivo(pid: u32) -> bool {
    inicio_do_processo(pid).is_some()
}

fn esperar_morrer(pid: u32) -> bool {
    for _ in 0..100 {
        if !vivo(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

fn matar(pid: u32) {
    let _ = std::process::Command::new("kill")
        .args(["-KILL", &format!("-{pid}")])
        .status();
}

const LACO: &str = "while :; do sleep 0.05; done";

/// O órfão de um app morto recebe o pedido de sair, sai, e o registro some.
/// Sem o `SIGTERM` de `parar_sobras`, ele segue vivo e este teste reprova.
#[test]
fn o_orfao_de_um_app_morto_sai_com_o_pedido() {
    let p = pasta("term");
    let pid = orfao(LACO);
    let arquivo = anotar_processo(&p.0, &registro(pid, dono_morto())).unwrap();

    let achadas = parar_sobras(&p.0, Duration::from_secs(5));

    assert_eq!(achadas.len(), 1, "{achadas:?}");
    assert!(
        matches!(achadas[0].desfecho, DesfechoDaSobra::SaiuAoPedido { .. }),
        "{achadas:?}"
    );
    assert_eq!(achadas[0].registro.etiqueta, r#"{"mod_id":"x/jogo"}"#);
    assert!(esperar_morrer(pid), "o órfão segue vivo");
    assert!(!arquivo.exists(), "o registro ficou");
}

/// Quem ignora o `SIGTERM` é forçado depois do prazo, e isso é dito.
#[test]
fn o_orfao_que_nao_sai_e_forcado() {
    let p = pasta("kill");
    let pid = orfao(&format!("trap '' TERM; {LACO}"));
    anotar_processo(&p.0, &registro(pid, dono_morto())).unwrap();

    let achadas = parar_sobras(&p.0, Duration::from_millis(600));

    assert_eq!(achadas.len(), 1, "{achadas:?}");
    assert!(
        matches!(achadas[0].desfecho, DesfechoDaSobra::Forcada { .. }),
        "{achadas:?}"
    );
    assert!(esperar_morrer(pid), "o órfão segue vivo depois do SIGKILL");
}

/// Um pid reusado não é o jogo: o começo não confere, nada é morto, e o
/// registro velho sai.
#[test]
fn um_pid_reusado_nao_e_morto() {
    let p = pasta("reusado");
    let pid = orfao(LACO);
    let mut velho = registro(pid, dono_morto());
    velho.inicio = "Mon Jan  1 00:00:00 2001".to_owned();
    let arquivo = anotar_processo(&p.0, &velho).unwrap();

    let achadas = parar_sobras(&p.0, Duration::from_secs(1));

    assert!(achadas.is_empty(), "{achadas:?}");
    assert!(vivo(pid), "um processo que não é o jogo foi morto");
    assert!(
        !arquivo.exists(),
        "o registro de um processo que já saiu ficou"
    );
    matar(pid);
}

/// O registro de outra janela viva é dela: nada é morto, e ele fica.
#[test]
fn o_jogo_de_uma_janela_viva_fica() {
    let p = pasta("viva");
    let pid = orfao(LACO);
    let outra_janela = orfao(LACO);
    let dono = (outra_janela, inicio_do_processo(outra_janela).unwrap());
    let arquivo = anotar_processo(&p.0, &registro(pid, dono)).unwrap();

    let achadas = parar_sobras(&p.0, Duration::from_secs(1));

    assert!(achadas.is_empty(), "{achadas:?}");
    assert!(vivo(pid), "o jogo de uma janela viva foi morto");
    assert!(arquivo.exists(), "o registro de uma janela viva sumiu");
    matar(pid);
    matar(outra_janela);
}

/// **Sem o começo do dono, um dono vivo segue dono** (revisão da 0.19, M2): um
/// app que não leu o próprio começo grava `dono_inicio` vazio, e a segunda
/// instância parava o jogo da primeira, viva. Com `dono_vive` exigindo o
/// começo não vazio, o jogo é morto e este teste reprova.
#[test]
fn sem_o_comeco_do_dono_um_dono_vivo_nao_perde_o_jogo() {
    /// Mata o grupo mesmo se o teste reprovar: um laço solto segura a saída
    /// do `cargo test` aberta para sempre.
    struct Matar(u32);
    impl Drop for Matar {
        fn drop(&mut self) {
            matar(self.0);
        }
    }
    let p = pasta("dono-sem-comeco");
    let pid = orfao(LACO);
    let outra_janela = orfao(LACO);
    let soltos = (Matar(pid), Matar(outra_janela));
    let arquivo = anotar_processo(&p.0, &registro(pid, (outra_janela, String::new()))).unwrap();

    let achadas = parar_sobras(&p.0, Duration::from_secs(1));

    assert!(achadas.is_empty(), "{achadas:?}");
    assert!(
        vivo(pid),
        "o jogo de uma janela viva foi parado por um registro sem o começo do dono"
    );
    assert!(arquivo.exists(), "o registro de uma janela viva sumiu");
    drop(soltos);

    // O dono que já saiu continua sendo dono morto: o órfão é parado.
    let pid = orfao(LACO);
    let _solto = Matar(pid);
    anotar_processo(&p.0, &registro(pid, (dono_morto().0, String::new()))).unwrap();
    let achadas = parar_sobras(&p.0, Duration::from_secs(5));
    assert_eq!(achadas.len(), 1, "{achadas:?}");
    assert!(esperar_morrer(pid), "o órfão de um dono morto segue vivo");
}

fn programa(raiz_de: &Path) -> Programa {
    let raiz = raiz_de.join("jogo");
    std::fs::create_dir_all(raiz.join("bin")).unwrap();
    std::fs::copy(
        env!("CARGO_BIN_EXE_seele-programa-de-teste"),
        raiz.join("bin").join("servidor"),
    )
    .unwrap();
    let politica = Politica {
        dominios: vec![],
        artefatos: vec![],
        de_outros_sistemas: vec![],
        origens: vec![],
        comando: Some(ComandoAceito {
            comando: "bin/servidor".to_owned(),
            argumentos: vec!["eco".to_owned()],
        }),
        instalar: None,
        pack: None,
        teto_de_disco: 1 << 30,
        portas_de_copia: vec![],
        raiz,
    };
    let origem: Arc<dyn Origem> = Arc::new(OrigemHttps::nova().unwrap());
    Programa::novo(politica, origem).unwrap()
}

/// **O `programa` deixa o registro enquanto o processo vive**, com o pid, o
/// grupo, o começo e este app como dono, e o tira quando ele termina. Sem a
/// anotação em `rodar`, o arquivo nunca aparece e este teste reprova.
#[tokio::test]
async fn o_programa_anota_cada_processo_enquanto_ele_vive() {
    let p = pasta("programa");
    let registros = p.0.join("registros");
    let programa = programa(&p.0);
    programa.anotar_processos_em(registros.clone(), "etiqueta-do-app".to_owned());
    let mut processo = programa
        .rodar(&PedidoDeExecucao {
            comando: "bin/servidor".to_owned(),
            argumentos: vec!["eco".to_owned()],
            pasta: String::new(),
        })
        .unwrap();
    let pid = processo.pid().unwrap();
    let arquivo = registros.join(format!("{pid}.json"));

    let mut lido = None;
    for _ in 0..250 {
        if let Ok(bytes) = std::fs::read(&arquivo) {
            lido = serde_json::from_slice::<RegistroDeProcesso>(&bytes).ok();
            if lido.is_some() {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let lido = lido.expect("o registro do processo não apareceu");
    assert_eq!(lido.pid, pid);
    assert_eq!(lido.grupo, pid, "o processo nasce líder do próprio grupo");
    assert_eq!(Some(lido.inicio.clone()), inicio_do_processo(pid));
    assert_eq!(lido.dono, std::process::id());
    assert!(!lido.dono_inicio.is_empty());
    assert_eq!(lido.etiqueta, "etiqueta-do-app");
    // A janela viva é a dona: abrir de novo não o toca.
    assert!(parar_sobras(&registros, Duration::from_secs(1)).is_empty());

    processo.matar();
    loop {
        let evento = tokio::time::timeout(Duration::from_secs(20), processo.proximo())
            .await
            .unwrap()
            .unwrap();
        if matches!(evento, EventoDoProcesso::Terminou(_)) {
            break;
        }
    }
    assert!(
        !arquivo.exists(),
        "o registro ficou depois de o processo terminar"
    );
}

/// **O registro existe quando `rodar` volta** (revisão da 0.19, M3): gravado
/// numa tarefa à parte, ele aparecia dezenas de ms depois de o processo já
/// rodar, e um app morto nesse meio deixava um órfão sem registro. Com a
/// gravação de volta na tarefa, o arquivo ainda não existe na linha seguinte ao
/// `rodar` e este teste reprova.
#[tokio::test]
async fn o_registro_existe_quando_rodar_volta() {
    let p = pasta("sincrono");
    let registros = p.0.join("registros");
    let programa = programa(&p.0);
    programa.anotar_processos_em(registros.clone(), "etiqueta-do-app".to_owned());
    let mut processo = programa
        .rodar(&PedidoDeExecucao {
            comando: "bin/servidor".to_owned(),
            argumentos: vec!["eco".to_owned()],
            pasta: String::new(),
        })
        .unwrap();
    let pid = processo.pid().unwrap();
    let arquivo = registros.join(format!("{pid}.json"));
    let lido = std::fs::read(&arquivo)
        .ok()
        .and_then(|b| serde_json::from_slice::<RegistroDeProcesso>(&b).ok());
    processo.matar();
    let lido = lido.expect("o registro não existia quando `rodar` voltou");
    assert_eq!(lido.pid, pid);
    assert!(
        !lido.inicio.is_empty(),
        "o registro saiu sem o começo do processo"
    );
    loop {
        let evento = tokio::time::timeout(Duration::from_secs(20), processo.proximo())
            .await
            .unwrap()
            .unwrap();
        if matches!(evento, EventoDoProcesso::Terminou(_)) {
            break;
        }
    }
    assert!(
        !arquivo.exists(),
        "o registro ficou depois de o processo terminar"
    );
}
