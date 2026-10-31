//! **O jogo que sobrou de um app que morreu sem se despedir** (e2e de 31/10,
//! passo 14).
//!
//! # O defeito
//!
//! Parar mata o grupo do processo (decisão R8) — mas só quem para é o app. Um
//! app que morre sem o `ExitRequested` (um `SIGTERM` de fora, uma queda, o
//! «Forçar encerramento») não para nada: o `java` nasceu num grupo próprio, o
//! sinal que matou o app não o alcança, e ele segue órfão, com o mundo e a
//! porta. O app reaberto não sabia que ele existia. No Windows o *Job Object*
//! com `KILL_ON_JOB_CLOSE` (decisão W3) já cobre a queda: o sistema fecha a
//! alça e mata a tarefa.
//!
//! # O que este módulo faz
//!
//! Cada processo que o `programa` roda deixa um **registro** numa pasta que o
//! app escolhe (fora da raiz do jogo: o MOD escreve na raiz, e um registro
//! escrito por ele mandaria o app matar um processo qualquer): o pid, o grupo,
//! **o instante em que o processo começou**, o app dono (pid e começo) e uma
//! etiqueta do app. O registro sai quando o processo termina.
//!
//! Ao abrir, o app chama [`parar_sobras`]: um registro cujo dono ainda vive é
//! de outra janela, e fica; um cujo processo já não é o mesmo (o pid foi
//! reusado: o começo não confere) é só apagado; o que sobra é o jogo órfão. Ele
//! recebe `SIGTERM` no grupo — o Java roda os ganchos de saída, e o servidor de
//! Minecraft salva o mundo neles —, tem [`prazo`] para sair, e então o grupo
//! recebe `SIGKILL`. O desfecho volta para o app dizer à pessoa.
//!
//! **O `aoParar` do MOD não roda aqui**: a metade de máquina que o chamaria
//! morreu com o app. O sinal de término é o pedido de salvar que sobra, e a
//! tela diz qual dos dois aconteceu.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// O registro de um processo de jogo que roda.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RegistroDeProcesso {
    /// O pid do processo.
    pub pid: u32,
    /// O grupo dele (no Unix, o próprio pid: ele nasce líder do grupo).
    pub grupo: u32,
    /// Quando ele começou, como o sistema diz: é o que separa este processo
    /// de outro que herdou o mesmo pid.
    pub inicio: String,
    /// O pid do app que o rodou.
    pub dono: u32,
    /// Quando o app dono começou.
    pub dono_inicio: String,
    /// O que o app quer reler depois (o MOD, o servidor de jogo, o título).
    pub etiqueta: String,
}

/// Como acabou um jogo que sobrou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesfechoDaSobra {
    /// Saiu com o `SIGTERM`, a tempo de salvar.
    SaiuAoPedido {
        /// Quanto levou.
        levou: Duration,
    },
    /// Não saiu no prazo e o grupo foi morto com `SIGKILL`.
    Forcada {
        /// Quanto se esperou antes de forçar.
        esperou: Duration,
    },
    /// Nem o `SIGKILL` o tirou (ou o sinal foi recusado): segue rodando.
    NaoParou {
        /// O que o sistema disse.
        motivo: String,
    },
}

/// Um jogo órfão encontrado, e o que se fez com ele.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SobraEncontrada {
    /// O registro que ele deixou.
    pub registro: RegistroDeProcesso,
    /// O desfecho.
    pub desfecho: DesfechoDaSobra,
}

/// Quanto um jogo órfão tem para salvar e sair depois do `SIGTERM`: o mesmo
/// teto que o `aoParar` tem com um processo vivo, no app.
#[must_use]
pub const fn prazo() -> Duration {
    Duration::from_secs(20)
}

/// O arquivo do registro de `pid` em `pasta`.
fn arquivo(pasta: &Path, pid: u32) -> PathBuf {
    pasta.join(format!("{pid}.json"))
}

/// O que o sistema diz de `pid`: o grupo e o começo. `None` se ele não
/// existe, já é um zumbi, ou o sistema não respondeu.
#[cfg(unix)]
fn como_esta(pid: u32) -> Option<(u32, String)> {
    let saida = std::process::Command::new("ps")
        .args(["-o", "pgid=,stat=,lstart=", "-p", &pid.to_string()])
        .env("LC_ALL", "C")
        .env("TZ", "UTC")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !saida.status.success() {
        return None;
    }
    let texto = String::from_utf8_lossy(&saida.stdout);
    let linha = texto.lines().find(|l| !l.trim().is_empty())?;
    // O grupo, o estado e o começo (que tem espaços: «Sat Oct 31 17:47:12 2026»).
    let partes: Vec<&str> = linha.split_whitespace().collect();
    let (grupo, stat, inicio) = match partes.as_slice() {
        [grupo, stat, inicio @ ..] if !inicio.is_empty() => {
            (grupo.parse().ok()?, *stat, inicio.join(" "))
        }
        _ => return None,
    };
    if stat.starts_with('Z') {
        return None;
    }
    Some((grupo, inicio))
}

#[cfg(not(unix))]
fn como_esta(_pid: u32) -> Option<(u32, String)> {
    None
}

/// Quando `pid` começou, como o sistema diz; `None` se ele não existe.
#[must_use]
pub fn inicio_do_processo(pid: u32) -> Option<String> {
    como_esta(pid).map(|(_, inicio)| inicio)
}

/// O começo deste processo, lido uma vez.
pub(crate) fn inicio_deste_app() -> String {
    static INICIO: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    INICIO
        .get_or_init(|| inicio_do_processo(std::process::id()).unwrap_or_default())
        .clone()
}

/// Grava o registro em `pasta` (criada se faltar), de uma vez: num nome
/// temporário, e então renomeado.
///
/// # Errors
///
/// O erro do disco.
pub fn anotar(pasta: &Path, registro: &RegistroDeProcesso) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(pasta)?;
    let destino = arquivo(pasta, registro.pid);
    let temporario = pasta.join(format!("{}.json.parcial", registro.pid));
    let texto = serde_json::to_vec(registro).map_err(std::io::Error::other)?;
    std::fs::write(&temporario, texto)?;
    std::fs::rename(&temporario, &destino)?;
    Ok(destino)
}

/// O registro de um processo que este app acabou de rodar, com o começo lido
/// do sistema agora.
pub(crate) fn registro_de(pid: u32, etiqueta: &str) -> RegistroDeProcesso {
    let (grupo, inicio) = como_esta(pid).unwrap_or((pid, String::new()));
    RegistroDeProcesso {
        pid,
        grupo,
        inicio,
        dono: std::process::id(),
        dono_inicio: inicio_deste_app(),
        etiqueta: etiqueta.to_owned(),
    }
}

/// O processo do registro ainda é o mesmo (o pid não foi reusado).
fn ainda_e_ele(registro: &RegistroDeProcesso) -> bool {
    !registro.inicio.is_empty()
        && como_esta(registro.pid)
            .is_some_and(|(grupo, inicio)| grupo == registro.grupo && inicio == registro.inicio)
}

/// O app dono do registro ainda vive: o registro é de outra janela.
fn dono_vive(registro: &RegistroDeProcesso) -> bool {
    !registro.dono_inicio.is_empty()
        && como_esta(registro.dono).is_some_and(|(_, inicio)| inicio == registro.dono_inicio)
}

#[cfg(unix)]
fn sinalizar(grupo: u32, sinal: nix::sys::signal::Signal) -> Result<(), String> {
    let Some(grupo) = i32::try_from(grupo).ok().filter(|g| *g > 1) else {
        return Err(format!("grupo inválido: {grupo}"));
    };
    match nix::sys::signal::killpg(nix::unistd::Pid::from_raw(grupo), sinal) {
        Ok(()) | Err(nix::errno::Errno::ESRCH) => Ok(()),
        Err(erro) => Err(erro.to_string()),
    }
}

/// Espera o processo do registro sumir, até `prazo`. Devolve se sumiu.
fn esperar_sumir(registro: &RegistroDeProcesso, prazo: Duration) -> bool {
    let fim = Instant::now() + prazo;
    loop {
        if !ainda_e_ele(registro) {
            return true;
        }
        if Instant::now() >= fim {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Para um órfão: `SIGTERM` ao grupo, `prazo` para sair, `SIGKILL` ao grupo.
#[cfg(unix)]
fn parar_um(registro: &RegistroDeProcesso, prazo: Duration) -> DesfechoDaSobra {
    use nix::sys::signal::Signal;
    let comeco = Instant::now();
    if let Err(motivo) = sinalizar(registro.grupo, Signal::SIGTERM) {
        return DesfechoDaSobra::NaoParou { motivo };
    }
    let saiu = esperar_sumir(registro, prazo);
    let esperou = comeco.elapsed();
    // O `SIGKILL` vai sempre: um neto que ficou no grupo sai junto.
    let forcar = sinalizar(registro.grupo, Signal::SIGKILL);
    if saiu {
        return DesfechoDaSobra::SaiuAoPedido { levou: esperou };
    }
    if let Err(motivo) = forcar {
        return DesfechoDaSobra::NaoParou { motivo };
    }
    if esperar_sumir(registro, Duration::from_secs(2)) {
        DesfechoDaSobra::Forcada { esperou }
    } else {
        DesfechoDaSobra::NaoParou {
            motivo: "o processo seguiu vivo depois do SIGKILL".to_owned(),
        }
    }
}

#[cfg(not(unix))]
fn parar_um(_registro: &RegistroDeProcesso, _prazo: Duration) -> DesfechoDaSobra {
    DesfechoDaSobra::NaoParou {
        motivo: "sem suporte neste sistema".to_owned(),
    }
}

/// **Acha e para os jogos que sobraram** de um app que morreu sem desligá-los.
///
/// Bloqueia até `prazo` por órfão: chame numa thread. Cada desfecho vai ao
/// log; a lista volta para a tela. O registro sai em todo desfecho, menos no
/// [`DesfechoDaSobra::NaoParou`] (para a próxima abertura tentar de novo) e
/// no de outra janela viva.
#[must_use]
pub fn parar_sobras(pasta: &Path, prazo: Duration) -> Vec<SobraEncontrada> {
    let Ok(entradas) = std::fs::read_dir(pasta) else {
        return Vec::new();
    };
    let mut achadas = Vec::new();
    for entrada in entradas.flatten() {
        let caminho = entrada.path();
        if caminho.extension().and_then(|e| e.to_str()) != Some("json") {
            // Um `.parcial` de uma gravação cortada no meio: não diz nada.
            if caminho.to_string_lossy().ends_with(".parcial") {
                let _ = std::fs::remove_file(&caminho);
            }
            continue;
        }
        let lido = std::fs::read(&caminho)
            .ok()
            .and_then(|b| serde_json::from_slice::<RegistroDeProcesso>(&b).ok());
        let Some(registro) = lido else {
            tracing::warn!(arquivo = %caminho.display(), "registro de processo de jogo ilegível: apagado");
            let _ = std::fs::remove_file(&caminho);
            continue;
        };
        if registro.dono == std::process::id() || dono_vive(&registro) {
            continue;
        }
        if !ainda_e_ele(&registro) {
            tracing::debug!(
                pid = registro.pid,
                "registro de processo de jogo de um processo que já saiu: apagado"
            );
            let _ = std::fs::remove_file(&caminho);
            continue;
        }
        tracing::warn!(
            pid = registro.pid,
            grupo = registro.grupo,
            etiqueta = %registro.etiqueta,
            "um jogo de uma execução anterior do app ainda roda: pedindo que salve e saia"
        );
        let desfecho = parar_um(&registro, prazo);
        match &desfecho {
            DesfechoDaSobra::SaiuAoPedido { levou } => {
                tracing::info!(pid = registro.pid, levou = ?levou, "o jogo que sobrou saiu com o SIGTERM");
            }
            DesfechoDaSobra::Forcada { esperou } => {
                tracing::warn!(pid = registro.pid, esperou = ?esperou, "o jogo que sobrou não saiu no prazo: grupo morto com SIGKILL");
            }
            DesfechoDaSobra::NaoParou { motivo } => {
                tracing::error!(pid = registro.pid, motivo, "o jogo que sobrou não parou");
            }
        }
        if !matches!(desfecho, DesfechoDaSobra::NaoParou { .. }) {
            let _ = std::fs::remove_file(&caminho);
        }
        achadas.push(SobraEncontrada { registro, desfecho });
    }
    achadas
}
