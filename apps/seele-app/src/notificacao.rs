//! A notificação do sistema: o toque no ombro com a janela fora da frente.
//!
//! Era «o que sobrou da pendência 23, item 1», e a falta de mesa mínima do mapa
//! de 09/11 (`docs/analise-para-a-1.0-2026-11-09.md`, seção 8): o app só avisava
//! dentro da janela, e quem hospedava com a janela atrás de um jogo não ficava
//! sabendo de quem batia à porta.
//!
//! # Quem decide o quê
//!
//! **A regra é da casca** (`ui/notificacao-do-sistema.js`): o que notifica (a
//! menção, a batida na porta, a ligação que chega), a repetição, e o texto, que
//! é de `frases.js` como todo texto do produto. Ela é uma só para o app e para
//! a página de navegador, e é em Node que os testes dela rodam.
//!
//! **Este lado é a porta, e confere o que só ele sabe** antes de abrir:
//!
//! - a **preferência no disco** ([`seele_ffi::preferences::Preferences`]):
//!   desligada, nada sai, peça a página o que pedir;
//! - a **janela na frente**, lida do sistema e não da página: com ela na
//!   frente o aviso de dentro basta, e uma notificação por cima da própria
//!   janela é barulho.
//!
//! A página não alcança o plugin por conta própria: não há
//! `notification:allow-notify` em `capabilities/janela.json`, e
//! `tests/permissoes.rs` segura isso. Uma linha de JavaScript — de um MOD que
//! achasse um caminho, ou de um defeito da casca — não escreve na tela
//! bloqueada de ninguém sem passar por [`pedir`].
//!
//! # O que o plugin não conta, e este módulo não finge
//!
//! Lido em `tauri-plugin-notification` 2.4.0, `src/desktop.rs`:
//!
//! - **A permissão do sistema não é legível no desktop.** `permission_state()`
//!   e `request_permission()` devolvem `Granted` fixo. O [`Retrato`] diz
//!   [`Permissao::NaoSeSabe`], e a tela diz onde olhar nos ajustes do sistema em
//!   vez de afirmar «permitida» ou «negada».
//! - **A falha de entrega é engolida.** O `show()` do plugin entrega a
//!   notificação a uma tarefa solta e descarta o resultado
//!   (`let _ = notification.show()`). O `Ok` de [`pedir`] quer dizer «o pedido
//!   foi entregue ao sistema», e **não** «a notificação apareceu»: a tela não
//!   afirma que ela saiu.
//! - **Não há retorno do clique.** No macOS o sistema traz o app à frente, e é
//!   só; levar ao canal ou à portaria pelo clique existe na página de
//!   navegador (`Notification.onclick`), e não aqui.
//! - **No macOS o caminho é o `NSUserNotificationCenter`**, que não mostra
//!   faixa com o app na frente. Não atrapalha a regra, que já é «só com a
//!   janela fora da frente», e é o motivo de não haver «notificação de teste»
//!   no gesto de ligar.

use tauri::{AppHandle, Manager, Runtime};

/// A janela do produto. O mesmo rótulo de `tecla_global.rs`.
const JANELA_PRINCIPAL: &str = "main";

/// O teto do título, em caracteres. Uma linha de sistema, e não um parágrafo.
pub(crate) const TETO_DO_TITULO: usize = 80;

/// O teto do corpo, em caracteres.
///
/// O nome de um servidor e o apelido de quem liga são de outra pessoa, e uma
/// notificação aparece na tela bloqueada: o que vem de fora entra com teto.
pub(crate) const TETO_DO_CORPO: usize = 200;

/// **O que o produto notifica**, lista fechada. Um motivo que não está aqui
/// não desserializa, e o comando recusa antes de qualquer conferência.
///
/// Mensagem comum de canal não está, de propósito: seria barulho. Mensagem de
/// conversa pessoal também não: o produto não tem um evento que diga «chegou
/// uma mensagem de outra pessoa» (o `MessagesChanged` é o mesmo para a
/// própria, para a edição e para o histórico).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub(crate) enum Motivo {
    /// Alguém chamou quem está logado (`Mentioned`).
    Mencao,
    /// Alguém bate à porta do servidor que esta máquina hospeda.
    Porta,
    /// Uma ligação de conversa pessoal está chegando.
    Ligacao,
}

/// Por que uma notificação não saiu. A casca recebe o nome, e o deixa no
/// console: quem investiga um «não fui avisado» lê qual das portas fechou.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub(crate) enum RecusaDeNotificar {
    /// A preferência está desligada, ou não pôde ser lida.
    Desligadas,
    /// A janela está na frente: o aviso de dentro basta.
    JanelaNaFrente,
    /// Sem título depois da limpeza: não há o que mostrar.
    SemTexto,
    /// O plugin recusou o pedido, ou não está registrado.
    NaoSaiu,
}

/// O que se sabe da permissão do sistema.
///
/// No desktop, uma variante só, e é a honesta: ver o topo deste arquivo. A
/// página de navegador responde as outras (`Concedida`, `Negada`, `PorPedir`,
/// `NaoHa`) pela ponte, que lê `Notification.permission`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub(crate) enum Permissao {
    /// O plugin não deixa ler o que o sistema decidiu.
    NaoSeSabe,
}

/// O que a tela de CONFIGURAÇÕES lê: a escolha, e o que se sabe do sistema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Retrato {
    /// A preferência desta máquina.
    pub(crate) ligadas: bool,
    /// O que se sabe da permissão do sistema.
    pub(crate) permissao: Permissao,
}

impl Retrato {
    /// O retrato do desktop para esta escolha.
    pub(crate) fn do_desktop(ligadas: bool) -> Self {
        Self {
            ligadas,
            permissao: Permissao::NaoSeSabe,
        }
    }
}

/// **As duas conferências deste lado**, na ordem em que recusam: a
/// preferência primeiro (quem desligou não quer nem a pergunta pelo foco), a
/// janela depois.
///
/// # Errors
///
/// [`RecusaDeNotificar::Desligadas`] ou [`RecusaDeNotificar::JanelaNaFrente`].
pub(crate) fn conferir(ligadas: bool, janela_na_frente: bool) -> Result<(), RecusaDeNotificar> {
    if !ligadas {
        return Err(RecusaDeNotificar::Desligadas);
    }
    if janela_na_frente {
        return Err(RecusaDeNotificar::JanelaNaFrente);
    }
    Ok(())
}

/// Um texto de notificação: uma linha, sem caractere de controle, com teto.
///
/// O que entra aqui tem pedaços de outra pessoa (o nome do servidor, o apelido
/// de quem liga). Uma quebra de linha no meio deles desenharia uma segunda
/// «frase» que o produto não escreveu, e um nome de mil caracteres ocuparia a
/// tela bloqueada inteira.
pub(crate) fn limpar(texto: &str, teto: usize) -> String {
    let mut limpo = String::new();
    let mut ultimo_foi_espaco = true;
    for letra in texto.chars() {
        let letra = if letra.is_control() || letra.is_whitespace() {
            ' '
        } else {
            letra
        };
        if letra == ' ' {
            if ultimo_foi_espaco {
                continue;
            }
            ultimo_foi_espaco = true;
        } else {
            ultimo_foi_espaco = false;
        }
        limpo.push(letra);
    }
    let limpo = limpo.trim_end();
    if limpo.chars().count() <= teto {
        return limpo.to_owned();
    }
    let mut cortado: String = limpo.chars().take(teto.saturating_sub(1)).collect();
    cortado.push('…');
    cortado
}

/// **Um pedido de notificação, do começo ao fim**: confere, limpa o texto e
/// entrega a `emitir`. `emitir` é o plugin no produto e um anotador nos testes.
///
/// # Errors
///
/// A recusa de [`conferir`]; [`RecusaDeNotificar::SemTexto`] sem título; e o que
/// `emitir` devolver.
pub(crate) fn pedir(
    ligadas: bool,
    janela_na_frente: bool,
    titulo: &str,
    corpo: &str,
    emitir: impl FnOnce(&str, &str) -> Result<(), RecusaDeNotificar>,
) -> Result<(), RecusaDeNotificar> {
    conferir(ligadas, janela_na_frente)?;
    let titulo = limpar(titulo, TETO_DO_TITULO);
    if titulo.is_empty() {
        return Err(RecusaDeNotificar::SemTexto);
    }
    let corpo = limpar(corpo, TETO_DO_CORPO);
    emitir(&titulo, &corpo)
}

/// Se a janela do produto está na frente, pelo sistema.
///
/// Sem janela, ou sem saber, «não está»: é a resposta que deixa o aviso sair,
/// e um aviso a mais é melhor que o silêncio de quem batia à porta.
pub(crate) fn janela_na_frente<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.get_webview_window(JANELA_PRINCIPAL)
        .is_some_and(|janela| {
            janela.is_focused().unwrap_or_else(|erro| {
                tracing::warn!(%erro, "não consegui saber se a janela está na frente");
                false
            })
        })
}

/// Entrega o pedido ao plugin.
///
/// **`Ok` não é «apareceu»**: ver o topo deste arquivo. O texto não vai ao
/// registro, nem na falha: ele tem o nome de um servidor ou de uma pessoa.
///
/// # Errors
///
/// [`RecusaDeNotificar::NaoSaiu`] se o plugin não está registrado ou recusou.
pub(crate) fn emitir<R: Runtime>(
    app: &AppHandle<R>,
    titulo: &str,
    corpo: &str,
) -> Result<(), RecusaDeNotificar> {
    let Some(plugin) = app.try_state::<tauri_plugin_notification::Notification<R>>() else {
        tracing::warn!("o plugin de notificação não está registrado: nada saiu");
        return Err(RecusaDeNotificar::NaoSaiu);
    };
    let mut pedido = plugin.builder().title(titulo);
    if !corpo.is_empty() {
        pedido = pedido.body(corpo);
    }
    pedido.show().map_err(|erro| {
        tracing::warn!(%erro, "o sistema não aceitou a notificação");
        RecusaDeNotificar::NaoSaiu
    })
}

#[cfg(test)]
mod testes {
    use super::{conferir, limpar, pedir, RecusaDeNotificar, TETO_DO_CORPO, TETO_DO_TITULO};
    use std::cell::RefCell;

    /// Um emissor que anota o que recebeu.
    fn anotador(
        saidas: &RefCell<Vec<(String, String)>>,
    ) -> impl FnOnce(&str, &str) -> Result<(), RecusaDeNotificar> + '_ {
        |titulo, corpo| {
            saidas
                .borrow_mut()
                .push((titulo.to_owned(), corpo.to_owned()));
            Ok(())
        }
    }

    #[test]
    fn so_sai_com_a_preferencia_ligada_e_a_janela_fora_da_frente() {
        assert_eq!(conferir(true, false), Ok(()));
        assert_eq!(conferir(true, true), Err(RecusaDeNotificar::JanelaNaFrente));
        assert_eq!(conferir(false, false), Err(RecusaDeNotificar::Desligadas));
        // Desligada vem primeiro: quem desligou lê «desligadas», e não «a
        // janela está na frente», que mandaria procurar no lugar errado.
        assert_eq!(conferir(false, true), Err(RecusaDeNotificar::Desligadas));
    }

    #[test]
    fn a_preferencia_desligada_nao_chega_ao_plugin() {
        let saidas = RefCell::new(Vec::new());
        let recusa = pedir(
            false,
            false,
            "Você foi chamado",
            "em Casa",
            anotador(&saidas),
        );
        assert_eq!(recusa, Err(RecusaDeNotificar::Desligadas));
        assert!(
            saidas.borrow().is_empty(),
            "com a preferência desligada o pedido chegou ao sistema"
        );
    }

    #[test]
    fn a_janela_na_frente_nao_chega_ao_plugin() {
        let saidas = RefCell::new(Vec::new());
        let recusa = pedir(true, true, "Você foi chamado", "em Casa", anotador(&saidas));
        assert_eq!(recusa, Err(RecusaDeNotificar::JanelaNaFrente));
        assert!(
            saidas.borrow().is_empty(),
            "com a janela na frente o pedido chegou ao sistema"
        );
    }

    #[test]
    fn o_pedido_que_passa_chega_limpo_e_uma_vez() {
        let saidas = RefCell::new(Vec::new());
        let feito = pedir(
            true,
            false,
            "  Alguém está\nbatendo à porta ",
            "em Casa\u{0007}\tdo  Rafa",
            anotador(&saidas),
        );
        assert_eq!(feito, Ok(()));
        assert_eq!(
            *saidas.borrow(),
            vec![(
                "Alguém está batendo à porta".to_owned(),
                "em Casa do Rafa".to_owned()
            )]
        );
    }

    #[test]
    fn sem_titulo_nao_ha_o_que_mostrar() {
        let saidas = RefCell::new(Vec::new());
        let recusa = pedir(true, false, " \n\t ", "corpo", anotador(&saidas));
        assert_eq!(recusa, Err(RecusaDeNotificar::SemTexto));
        assert!(saidas.borrow().is_empty());
    }

    #[test]
    fn a_falha_do_plugin_volta_com_o_nome() {
        let recusa = pedir(true, false, "título", "", |_, _| {
            Err(RecusaDeNotificar::NaoSaiu)
        });
        assert_eq!(recusa, Err(RecusaDeNotificar::NaoSaiu));
    }

    #[test]
    fn o_texto_de_fora_entra_com_teto() {
        let nome = "n".repeat(5000);
        let titulo = limpar(&nome, TETO_DO_TITULO);
        assert_eq!(titulo.chars().count(), TETO_DO_TITULO);
        assert!(titulo.ends_with('…'), "o corte não é dito");
        assert_eq!(limpar(&nome, TETO_DO_CORPO).chars().count(), TETO_DO_CORPO);
        // No teto exato, nada é cortado.
        let justo = "é".repeat(TETO_DO_TITULO);
        assert_eq!(limpar(&justo, TETO_DO_TITULO), justo);
    }
}
