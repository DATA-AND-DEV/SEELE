//! As saídas de quem teve a gravação de tela negada pelo macOS.
//!
//! # O defeito que isto conserta
//!
//! Relato do dono em 13/11/2026, com a 1.0.0: «mesmo que eu permita o
//! compartilhamento de tela nos Ajustes, ele não funciona, não habilita nunca,
//! fica num loop infinito». A caixa de compartilhar mandava «marcar o SEELE
//! nos Ajustes», e ele já estava marcado: a permissão era a de uma versão
//! anterior, e para o binário novo ela não valia. A tela não tinha o que
//! oferecer além do mesmo pedido.
//!
//! # Por que a permissão fica presa à versão anterior
//!
//! O macOS guarda a permissão de gravar a tela junto do **requisito de
//! assinatura** do app que a recebeu. Os pacotes deste produto saem com
//! assinatura ad-hoc (`tauri.conf.json`, `"signingIdentity": "-"`), e nela o
//! requisito é o hash do próprio código: medido em 13/11/2026 com
//! `codesign -dr -`, a 0.17.0, a 0.18.1 e a 1.0.0 têm o mesmo identificador
//! (`tech.datadev.seele`) e três requisitos diferentes, um `cdhash` cada.
//! Depois de uma atualização, a entrada antiga continua nos Ajustes, ligada, e
//! não serve ao binário novo. (O que o `tccd` decide por dentro não foi medido:
//! o banco dele e o registro dele não se leem sem privilégio.)
//!
//! # O que este arquivo oferece, e só por aperto de botão
//!
//! 1. abrir a página certa dos Ajustes ([`abrir_os_ajustes`]);
//! 2. apagar a decisão guardada **deste app** ([`redefinir_a_permissao`]), que
//!    é o que o «−» daquela página faz: `tccutil reset ScreenCapture <id>`.
//!    Sem privilégio, e sem tocar em nenhuma outra permissão nem em nenhum
//!    outro app.
//!
//! Nada daqui roda sozinho: um app que apaga uma permissão sem ninguém pedir é
//! o que ensina a desconfiar dele.

/// A página dos Ajustes do Sistema em que o macOS lista quem pode gravar a
/// tela. A âncora `Privacy_ScreenCapture` está na extensão de Privacidade e
/// Segurança do macOS 27 (lida em 13/11/2026), e é a mesma desde o macOS 13.
pub(crate) const PAGINA_DOS_AJUSTES: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";

/// O nome que o TCC dá à gravação de tela (e do áudio do sistema).
const SERVICO_DO_TCC: &str = "ScreenCapture";

/// Por que uma destas saídas não aconteceu.
///
/// Enum e não frase: a fronteira erro→texto está em `ui/frases.js`, e o guarda
/// `every_refusal_the_bridge_writes_itself_has_a_sentence_in_the_page` cobra
/// uma frase por nome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub(crate) enum FalhaNaGravacaoDeTela {
    /// Este sistema não tem essa página nem essa permissão: só o macOS.
    SoNoMacos,
    /// O sistema não abriu a página dos Ajustes.
    AjustesNaoAbriram,
    /// O sistema não apagou a decisão guardada.
    PermissaoNaoRedefinida,
}

/// Um programa do sistema e os argumentos dele, prontos para rodar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Comando {
    /// O caminho inteiro: nada aqui depende do `PATH` de quem abriu o app.
    pub(crate) programa: &'static str,
    /// Os argumentos, um por posição. Nenhum passa por um shell.
    pub(crate) argumentos: Vec<String>,
}

/// O comando que abre a página dos Ajustes.
#[must_use]
pub(crate) fn comando_dos_ajustes() -> Comando {
    Comando {
        programa: "/usr/bin/open",
        argumentos: vec![PAGINA_DOS_AJUSTES.to_owned()],
    }
}

/// O comando que apaga a decisão guardada sobre **este** app gravar a tela.
///
/// # Errors
///
/// [`FalhaNaGravacaoDeTela::PermissaoNaoRedefinida`] quando o identificador
/// não tem a forma de um identificador de app. **É a guarda que importa
/// aqui:** `tccutil reset ScreenCapture` **sem** identificador apaga a
/// permissão de gravar a tela de **todos** os apps da máquina. Um
/// identificador vazio, ou um que o `tccutil` pudesse ler como outra coisa,
/// nunca chega a ele: sem identificador certo, não há comando.
pub(crate) fn comando_de_redefinir(identificador: &str) -> Result<Comando, FalhaNaGravacaoDeTela> {
    if !e_identificador_de_app(identificador) {
        return Err(FalhaNaGravacaoDeTela::PermissaoNaoRedefinida);
    }
    Ok(Comando {
        programa: "/usr/bin/tccutil",
        argumentos: vec![
            "reset".to_owned(),
            SERVICO_DO_TCC.to_owned(),
            identificador.to_owned(),
        ],
    })
}

/// Se o texto tem a forma de um identificador de app (`tech.datadev.seele`):
/// letras, algarismos, ponto e hífen, com um ponto no meio e nada nas pontas.
fn e_identificador_de_app(texto: &str) -> bool {
    let so_do_alfabeto = texto
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
    let partes_cheias = texto.split('.').all(|parte| !parte.is_empty());
    so_do_alfabeto
        && partes_cheias
        && texto.contains('.')
        && !texto.starts_with('-')
        && texto.len() <= 155
}

/// Roda o comando e diz se ele terminou bem. O que o sistema escreveu vai ao
/// registro: uma saída que não funcionou e não deixou rastro é um beco.
fn rodar(comando: &Comando) -> bool {
    let saida = std::process::Command::new(comando.programa)
        .args(&comando.argumentos)
        .stdin(std::process::Stdio::null())
        .output();
    match saida {
        Ok(saida) if saida.status.success() => true,
        Ok(saida) => {
            tracing::warn!(
                programa = comando.programa,
                codigo = ?saida.status.code(),
                dito = %String::from_utf8_lossy(&saida.stderr).trim(),
                "o sistema recusou uma saída da gravação de tela"
            );
            false
        }
        Err(erro) => {
            tracing::warn!(
                programa = comando.programa,
                %erro,
                "não consegui rodar uma saída da gravação de tela"
            );
            false
        }
    }
}

/// Abre a página dos Ajustes em que a gravação de tela é concedida.
///
/// # Errors
///
/// [`FalhaNaGravacaoDeTela::AjustesNaoAbriram`] quando o sistema não abriu, e
/// [`FalhaNaGravacaoDeTela::SoNoMacos`] fora do macOS. Com barulho, ao
/// contrário de `abrir_ajustes_do_microfone`: aqui o botão é a saída de quem
/// está preso, e uma saída que não abriu e não disse nada é o mesmo beco.
pub(crate) fn abrir_os_ajustes() -> Result<(), FalhaNaGravacaoDeTela> {
    if !cfg!(target_os = "macos") {
        return Err(FalhaNaGravacaoDeTela::SoNoMacos);
    }
    if rodar(&comando_dos_ajustes()) {
        Ok(())
    } else {
        Err(FalhaNaGravacaoDeTela::AjustesNaoAbriram)
    }
}

/// Apaga a decisão que o macOS guardou sobre este app gravar a tela.
///
/// Depois disto a entrada do app some da página dos Ajustes, e o próximo
/// pedido de uma execução nova faz o sistema perguntar de novo. O processo que
/// chamou continua com a resposta antiga: o macOS só a refaz quando o app
/// reabre.
///
/// # Errors
///
/// [`FalhaNaGravacaoDeTela::PermissaoNaoRedefinida`] quando o identificador
/// não serve ou o sistema não apagou, e [`FalhaNaGravacaoDeTela::SoNoMacos`]
/// fora do macOS.
pub(crate) fn redefinir_a_permissao(identificador: &str) -> Result<(), FalhaNaGravacaoDeTela> {
    if !cfg!(target_os = "macos") {
        return Err(FalhaNaGravacaoDeTela::SoNoMacos);
    }
    if !rodar(&comando_de_redefinir(identificador)?) {
        return Err(FalhaNaGravacaoDeTela::PermissaoNaoRedefinida);
    }
    tracing::info!(
        identificador,
        "a decisão guardada do macOS sobre gravar a tela foi apagada, a pedido"
    );
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O identificador de verdade, lido do `tauri.conf.json` deste app.
    fn identificador_do_app() -> String {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        conf.get("identifier")
            .and_then(serde_json::Value::as_str)
            .expect("o identificador do app")
            .to_owned()
    }

    /// **O `tccutil reset ScreenCapture` sem identificador apaga a permissão
    /// de todos os apps.** O comando sai com as três posições, e a terceira é o
    /// identificador deste app, inteiro.
    #[test]
    fn redefinir_leva_o_servico_e_o_identificador_deste_app_e_mais_nada() {
        let id = identificador_do_app();
        let comando = comando_de_redefinir(&id).expect("o identificador do app serve");
        assert_eq!(comando.programa, "/usr/bin/tccutil");
        assert_eq!(comando.argumentos, ["reset", "ScreenCapture", id.as_str()]);
    }

    /// Sem identificador certo não há comando: nem vazio, nem com espaço, nem
    /// uma opção no lugar dele, nem um que termine ou comece em ponto.
    #[test]
    fn redefinir_sem_identificador_certo_nao_vira_comando() {
        for ruim in [
            "",
            " ",
            "seele",
            "-b",
            "--all",
            ".tech.datadev.seele",
            "tech.datadev.seele.",
            "tech..seele",
            "tech.datadev.seele outro.app",
            "tech.datadev.seele\n",
            "tech/datadev",
            "*",
        ] {
            assert_eq!(
                comando_de_redefinir(ruim),
                Err(FalhaNaGravacaoDeTela::PermissaoNaoRedefinida),
                "«{ruim}» virou comando: o tccutil sem identificador certo apaga a \
                 gravação de tela de todos os apps"
            );
        }
    }

    /// A página é a da gravação de tela, pelo esquema que o macOS abre, e vai
    /// num argumento só.
    #[test]
    fn os_ajustes_abrem_na_pagina_da_gravacao_de_tela() {
        let comando = comando_dos_ajustes();
        assert_eq!(comando.programa, "/usr/bin/open");
        assert_eq!(
            comando.argumentos,
            ["x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"]
        );
    }

    /// **Reabrir passa pela despedida.** `AppHandle::restart`, chamado da linha
    /// principal (onde um comando síncrono roda), reinicia sem o
    /// `RunEvent::Exit`: a sessão e as hospedagens cairiam sem se despedir, e
    /// quem ficou só notaria pelo prazo de ociosidade. O comando usa o
    /// `request_restart`, que passa por ele (lido no `tauri` 2.11.5, `app.rs`).
    #[test]
    fn reabrir_o_app_pede_a_saida_em_vez_de_reiniciar_por_cima() {
        let main = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs"))
            .expect("o main.rs");
        let corpo = main
            .split("fn reabrir_o_app(")
            .nth(1)
            .and_then(|resto| resto.split("\n}\n").next())
            .expect("o comando `reabrir_o_app` saiu do main.rs");
        assert!(
            corpo.contains("app.request_restart();"),
            "`reabrir_o_app` não pede a saída pelo `request_restart`: {corpo}"
        );
        assert!(
            !corpo.contains(".restart()"),
            "`reabrir_o_app` chama o `restart`, que da linha principal sai sem o \
             `RunEvent::Exit` e sem despedir ninguém: {corpo}"
        );
    }

    /// As falhas atravessam com os nomes que `ui/frases.js` ramifica.
    #[test]
    fn as_falhas_atravessam_pelo_nome() {
        let nome = |falha: FalhaNaGravacaoDeTela| serde_json::to_string(&falha).expect("json");
        assert_eq!(nome(FalhaNaGravacaoDeTela::SoNoMacos), "\"SoNoMacos\"");
        assert_eq!(
            nome(FalhaNaGravacaoDeTela::AjustesNaoAbriram),
            "\"AjustesNaoAbriram\""
        );
        assert_eq!(
            nome(FalhaNaGravacaoDeTela::PermissaoNaoRedefinida),
            "\"PermissaoNaoRedefinida\""
        );
    }
}
