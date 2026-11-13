//! A tecla de falar com outra janela na frente.
//!
//! A tecla de falar da janela (`tela-sessao.js`) só existe enquanto a janela do
//! SEELE tem o teclado. Dentro de um jogo ela não chega, e quem joga falava
//! saindo do jogo. Este módulo pede a tecla ao sistema, pelo
//! `tauri-plugin-global-shortcut`, e só quando ela é útil.
//!
//! # Quando a tecla é do sistema
//!
//! Todas juntas, e nenhuma a menos ([`o_que_registrar`]):
//!
//! - a pessoa **ligou** a opção em CONFIGURAÇÕES · ATALHOS (nasce desligada);
//! - ela está **numa sala de voz**, na sessão que tem a voz desta máquina;
//! - o microfone abre **por tecla**;
//! - a janela do SEELE **não está na frente**.
//!
//! A última não estava no pedido, e é a que deixa a opção usável: um atalho do
//! sistema **tira a tecla de todos os programas** enquanto vale (é o que as
//! três APIs por baixo fazem: `RegisterEventHotKey`, `RegisterHotKey`,
//! `XGrabKey`), e isso inclui esta janela. Com ESPAÇO de tecla de falar,
//! registrar ao entrar na sala fazia a barra de espaço deixar de escrever na
//! conversa. Com a janela na frente quem ouve a tecla é a janela, como sempre.
//!
//! Faltando qualquer uma, a tecla é devolvida ao sistema na hora, e o
//! microfone que este módulo abriu fecha junto.
//!
//! # O que este módulo nunca faz calado
//!
//! O registro pode falhar: a tecla já é de outro programa, a tecla não serve de
//! atalho sozinha (SHIFT, CTRL, ALT), o sistema não tem o serviço. Cada um
//! desses é um [`Estado`] que a tela lê e diz por extenso (`frases.js`,
//! `FRASES_DA_TECLA_GLOBAL`). A frase mora lá e não aqui: a fronteira
//! erro→texto do produto é o frontend.
//!
//! # Como está dividido
//!
//! A decisão ([`Nucleo`], [`Porta`], [`o_que_registrar`], [`estado_de`]) não
//! conhece o Tauri nem o plugin: fala com o sistema por [`Sistema`], e os
//! testes do fim deste arquivo a exercitam com um sistema de mentira. A cola
//! ([`TeclaGlobal`], [`iniciar`], [`conferir`]) é o que sobra.

use std::str::FromStr as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

use tauri::{AppHandle, Emitter as _, Manager as _, Runtime};
use tauri_plugin_global_shortcut::{Code, GlobalShortcut, Shortcut, ShortcutState};

/// O canal por onde a janela fica sabendo que o estado mudou.
///
/// Separado do canal da conversa: isto é desta máquina, e não de sessão
/// nenhuma. A carga é um [`Retrato`].
pub(crate) const CANAL_DA_TECLA_GLOBAL: &str = "seele://tecla-global";

/// A tecla de quem nunca escolheu: a mesma do `teclaDeFalar` da janela.
const TECLA_PADRAO: &str = "Space";

/// De quanto em quanto tempo a situação é conferida mesmo sem aviso, enquanto a
/// opção está ligada.
///
/// A rede de segurança de um defeito caro: um aviso perdido deixaria a tecla
/// presa no sistema fora de sala, isto é, a barra de espaço sem escrever em
/// programa nenhum. Com a opção desligada este relógio não corre.
const REDE_DE_SEGURANCA: std::time::Duration = std::time::Duration::from_secs(2);

/// O que decide se a tecla é do sistema agora. Lida de uma vez, a cada conferida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Situacao {
    /// A pessoa ligou a opção.
    pub ligada: bool,
    /// O serviço de atalhos do sistema subiu nesta máquina.
    pub disponivel: bool,
    /// A janela principal existe (ela some antes de o app sair).
    pub janela_aberta: bool,
    /// A janela principal tem o teclado.
    pub janela_na_frente: bool,
    /// A sessão com a voz desta máquina está numa sala de voz.
    pub na_sala: bool,
    /// O microfone abre por tecla.
    pub modo_tecla: bool,
    /// O `KeyboardEvent.code` escolhido.
    pub tecla: String,
}

/// O que a tela diz sobre a tecla fora da janela.
///
/// Um por frase. Os três primeiros depois de `Desligada` são falhas, e são
/// ditos onde quer que a pessoa esteja; os outros dizem por que a tecla não é
/// do sistema neste instante, e o último que ela é.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub(crate) enum Estado {
    /// A opção está desligada.
    Desligada,
    /// O serviço de atalhos do sistema não subiu nesta máquina.
    Indisponivel,
    /// A tecla escolhida não serve de atalho do sistema.
    SemAtalho,
    /// O sistema recusou a tecla na última vez que ela foi pedida.
    Recusada,
    /// Ligada, e esperando uma sala de voz.
    ForaDeSala,
    /// Ligada, e o microfone não abre por tecla.
    OutroModo,
    /// Ligada, e a janela do SEELE está na frente: quem ouve a tecla é ela.
    NaJanela,
    /// A tecla é do sistema agora.
    Valendo,
}

/// O que a janela recebe: o estado, e a tecla de que ele fala.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Retrato {
    /// Se a opção está ligada.
    pub ligada: bool,
    /// O que dizer.
    pub estado: Estado,
    /// O `KeyboardEvent.code` da tecla; quem a nomeia é a janela.
    pub tecla: String,
}

impl Default for Retrato {
    fn default() -> Self {
        Self {
            ligada: false,
            estado: Estado::Desligada,
            tecla: TECLA_PADRAO.to_owned(),
        }
    }
}

/// Se um `KeyboardEvent.code` pode ser atalho do sistema sozinho.
///
/// Duas recusas, e as duas antes de pedir qualquer coisa ao sistema, para a
/// tela dizer isso ao ligar a opção e não só na primeira vez que a janela sair
/// da frente:
///
/// - um código que o plugin não conhece;
/// - um modificador sozinho (SHIFT, CTRL, ALT, a tecla do sistema): nenhuma das
///   três tabelas do `global-hotkey` 0.8 (`key_to_scancode`, `key_to_vk`,
///   `keycode_to_x11_keysym`) tem linha para eles.
pub(crate) fn serve_de_atalho(tecla: &str) -> bool {
    const MODIFICADORES: [&str; 5] = ["Shift", "Control", "Alt", "Meta", "OS"];
    if MODIFICADORES
        .iter()
        .any(|prefixo| tecla.starts_with(prefixo))
    {
        return false;
    }
    Code::from_str(tecla).is_ok()
}

/// A tecla que tem de estar registrada no sistema nesta situação, se alguma.
pub(crate) fn o_que_registrar(situacao: &Situacao) -> Option<&str> {
    let quer = situacao.ligada
        && situacao.disponivel
        && situacao.janela_aberta
        && !situacao.janela_na_frente
        && situacao.na_sala
        && situacao.modo_tecla
        && serve_de_atalho(&situacao.tecla);
    quer.then_some(situacao.tecla.as_str())
}

/// Se uma recusa do sistema ainda fala desta situação.
///
/// Voltar à janela **não** a apaga: é quando a pessoa a lê. O que a apaga é o
/// que a tornaria história: sair da sala, trocar de modo, trocar de tecla,
/// desligar a opção.
fn a_recusa_ainda_vale(situacao: &Situacao, recusada: &str) -> bool {
    situacao.ligada
        && situacao.disponivel
        && situacao.janela_aberta
        && situacao.na_sala
        && situacao.modo_tecla
        && situacao.tecla == recusada
}

/// O que dizer, dada a situação e o que este módulo conseguiu.
///
/// A ordem é a de quem lê: primeiro o que impede sempre (desligada, sem
/// serviço, tecla que não serve), depois o que está valendo, depois a recusa
/// lembrada, e só então o motivo de a tecla não ser do sistema agora.
pub(crate) fn estado_de(situacao: &Situacao, registrada: bool, recusada: bool) -> Estado {
    if !situacao.ligada {
        Estado::Desligada
    } else if !situacao.disponivel {
        Estado::Indisponivel
    } else if !serve_de_atalho(&situacao.tecla) {
        Estado::SemAtalho
    } else if registrada {
        Estado::Valendo
    } else if recusada {
        Estado::Recusada
    } else if !situacao.na_sala {
        Estado::ForaDeSala
    } else if !situacao.modo_tecla {
        Estado::OutroModo
    } else {
        Estado::NaJanela
    }
}

/// O sistema, visto daqui: pedir uma tecla, devolvê-la, fechar o microfone.
pub(crate) trait Sistema {
    /// Pede a tecla ao sistema.
    ///
    /// # Errors
    ///
    /// O que o sistema disse ao recusar. Vai ao registro, e não à tela.
    fn registrar(&self, tecla: &str) -> Result<(), String>;
    /// Devolve a tecla ao sistema.
    ///
    /// # Errors
    ///
    /// O que o sistema disse ao recusar.
    fn desregistrar(&self, tecla: &str) -> Result<(), String>;
    /// Fecha o microfone que a tecla abriu.
    fn fechar_o_microfone(&self);
}

/// A porta entre a tecla do sistema e o microfone.
///
/// Dois fatos, e os dois sob o mesmo cadeado ([`TeclaGlobal::porta`]): se a
/// tecla vale, e se foi ela que abriu o microfone. O segundo existe para este
/// módulo só fechar o que abriu: a tecla da janela também abre o microfone, e
/// uma soltura daqui não pode fechar o que ela segura.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Porta {
    valendo: bool,
    apertada: bool,
}

impl Porta {
    /// O que fazer com o microfone quando o sistema relata a tecla.
    ///
    /// `Some(true)` abre, `Some(false)` fecha, `None` não mexe.
    ///
    /// Apertar só abre com a tecla valendo: entre este módulo decidir devolver
    /// a tecla e o sistema devolvê-la, um aperto ainda pode chegar, e ele
    /// abriria o microfone de quem já saiu da sala. Soltar fecha sempre que foi
    /// daqui que abriu, valendo ou não: uma soltura nunca é recusada.
    pub(crate) fn ao_evento(&mut self, apertou: bool) -> Option<bool> {
        if apertou {
            if !self.valendo || self.apertada {
                return None;
            }
            self.apertada = true;
            Some(true)
        } else if self.apertada {
            self.apertada = false;
            Some(false)
        } else {
            None
        }
    }

    /// A tecla passou a ser do sistema.
    fn valer(&mut self) {
        self.valendo = true;
    }

    /// A tecla deixa de valer. Devolve se há microfone aberto por ela a fechar.
    fn deixar_de_valer(&mut self) -> bool {
        self.valendo = false;
        std::mem::take(&mut self.apertada)
    }
}

/// O que este módulo lembra entre uma conferida e a seguinte.
#[derive(Debug, Default)]
pub(crate) struct Nucleo {
    /// A tecla que está registrada no sistema, se alguma.
    registrada: Option<String>,
    /// A tecla que o sistema recusou, enquanto a recusa fala desta situação.
    recusada: Option<String>,
    /// A tecla já pedida nesta saída da janela.
    ///
    /// Uma recusa não é repetida a cada conferida: a mesma pergunta ao sistema
    /// daria a mesma resposta, dezenas de vezes por minuto. Ela é refeita
    /// quando a situação deixa de pedir a tecla e volta a pedir (a janela veio
    /// à frente e saiu de novo), que é quando o outro programa pode ter saído.
    tentada: Option<String>,
}

impl Nucleo {
    /// Põe o sistema de acordo com a situação, e devolve o que dizer.
    ///
    /// A ordem importa em dois lugares. A porta fecha **antes** de a tecla ser
    /// devolvida: depois dela nenhum aperto abre o microfone, chegue ou não. E
    /// a porta só abre **depois** de o sistema aceitar: uma tecla que não foi
    /// registrada não vale.
    pub(crate) fn conferir(
        &mut self,
        situacao: &Situacao,
        porta: &Mutex<Porta>,
        sistema: &impl Sistema,
    ) -> Estado {
        let quer = o_que_registrar(situacao);

        if let Some(antiga) = self
            .registrada
            .take_if(|registrada| quer != Some(registrada.as_str()))
        {
            let fechar = porta
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .deixar_de_valer();
            if fechar {
                sistema.fechar_o_microfone();
            }
            if let Err(erro) = sistema.desregistrar(&antiga) {
                // A porta já fechou: mesmo que a tecla siga presa, ela não abre
                // microfone. O registro diz o que o sistema disse.
                tracing::warn!(tecla = %antiga, %erro, "não consegui devolver a tecla de falar ao sistema");
            }
        }

        if self
            .recusada
            .as_deref()
            .is_some_and(|recusada| !a_recusa_ainda_vale(situacao, recusada))
        {
            self.recusada = None;
        }

        match quer {
            None => self.tentada = None,
            Some(tecla) if self.registrada.is_none() && self.tentada.as_deref() != Some(tecla) => {
                self.tentada = Some(tecla.to_owned());
                match sistema.registrar(tecla) {
                    Ok(()) => {
                        self.registrada = Some(tecla.to_owned());
                        self.recusada = None;
                        porta.lock().unwrap_or_else(PoisonError::into_inner).valer();
                    }
                    Err(erro) => {
                        tracing::warn!(%tecla, %erro, "o sistema recusou a tecla de falar fora da janela");
                        self.recusada = Some(tecla.to_owned());
                    }
                }
            }
            Some(_) => {}
        }

        estado_de(situacao, self.registrada.is_some(), self.recusada.is_some())
    }
}

// ------------------------------------------------------------------- a cola

/// O estado deste módulo, guardado no app.
#[derive(Default)]
pub(crate) struct TeclaGlobal {
    /// A opção, como está no disco. Em memória para o aviso ser barato: com ela
    /// desligada e nada registrado, um aviso não lê nada.
    ligada: AtomicBool,
    /// A tecla escolhida, como está no disco.
    tecla: Mutex<String>,
    /// O serviço de atalhos subiu.
    disponivel: AtomicBool,
    /// Ver [`Porta`]. Segurado por quem trata o evento do sistema **enquanto**
    /// mexe no microfone, e nunca durante uma chamada ao plugin.
    porta: Mutex<Porta>,
    /// Ver [`Nucleo`]. Segurado pela conferida inteira, inclusive pelas
    /// chamadas ao plugin (que esperam a thread principal): **nada que rode na
    /// thread principal pode esperá-lo**. Por isso a leitura da tela é o
    /// [`Self::retrato`], e o comando que confere é `async`.
    nucleo: Mutex<Nucleo>,
    /// O último estado, para a tela ler sem esperar ninguém.
    retrato: Mutex<Retrato>,
    /// Acorda quem confere.
    acordar: tokio::sync::Notify,
}

impl TeclaGlobal {
    /// Anota a opção e a tecla que estão no disco.
    pub(crate) fn anotar(&self, ligada: bool, tecla: Option<&str>) {
        self.ligada.store(ligada, Ordering::Release);
        *self.tecla.lock().unwrap_or_else(PoisonError::into_inner) =
            tecla.unwrap_or(TECLA_PADRAO).to_owned();
    }

    /// O último estado conferido.
    pub(crate) fn retrato(&self) -> Retrato {
        self.retrato
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// Avisa que a situação pode ter mudado. Barato, e de qualquer thread.
///
/// Não confere aqui: quem chama pode estar na thread principal (o evento de
/// foco da janela) ou com um cadeado da sessão na mão (a ponte de eventos), e
/// conferir lê a sessão e fala com a thread principal.
pub(crate) fn avisar<R: Runtime>(app: &AppHandle<R>) {
    if let Some(tecla) = app.try_state::<TeclaGlobal>() {
        tecla.acordar.notify_one();
    }
}

/// Sobe o serviço de atalhos e a tarefa que confere. Chamado uma vez, no `setup`.
///
/// O plugin sobe **aqui**, e não no `Builder` do app: no `Builder`, o serviço
/// de atalhos que não sobe derruba a abertura do app inteiro, por causa de uma
/// opção que nasce desligada. Aqui a falha vira [`Estado::Indisponivel`], dita
/// a quem ligar a opção.
pub(crate) fn iniciar<R: Runtime>(app: &AppHandle<R>, ligada: bool, tecla: Option<&str>) {
    let Some(estado) = app.try_state::<TeclaGlobal>() else {
        return;
    };
    estado.anotar(ligada, tecla);
    let plugin = tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _atalho, evento| {
            ao_evento_do_sistema(app, evento.state == ShortcutState::Pressed);
        })
        .build();
    match app.plugin(plugin) {
        Ok(()) => estado.disponivel.store(true, Ordering::Release),
        Err(erro) => {
            tracing::warn!(%erro, "o serviço de atalhos do sistema não subiu: a tecla de falar fora da janela fica indisponível");
        }
    }
    // A primeira conferida: o `Notify` guarda o aviso até a tarefa abaixo
    // começar a esperar.
    estado.acordar.notify_one();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let Some(estado) = app.try_state::<TeclaGlobal>() else {
                return;
            };
            let ligada = estado.ligada.load(Ordering::Acquire);
            tokio::select! {
                () = estado.acordar.notified() => {}
                () = tokio::time::sleep(REDE_DE_SEGURANCA), if ligada => {}
            }
            conferir_fora_da_thread(&app).await;
        }
    });
}

/// Confere numa thread que pode esperar, e devolve o que a tela diz.
pub(crate) async fn conferir_fora_da_thread<R: Runtime>(app: &AppHandle<R>) -> Retrato {
    let fora = app.clone();
    match tauri::async_runtime::spawn_blocking(move || conferir(&fora)).await {
        Ok(retrato) => retrato,
        Err(erro) => {
            tracing::warn!(%erro, "a conferida da tecla de falar fora da janela não terminou");
            app.try_state::<TeclaGlobal>()
                .map(|estado| estado.retrato())
                .unwrap_or_default()
        }
    }
}

/// Lê a situação, põe o sistema de acordo com ela, e conta à janela se mudou.
///
/// **Nunca na thread principal**: ver [`TeclaGlobal::nucleo`].
fn conferir<R: Runtime>(app: &AppHandle<R>) -> Retrato {
    let Some(estado) = app.try_state::<TeclaGlobal>() else {
        return Retrato::default();
    };
    let mut nucleo = estado.nucleo.lock().unwrap_or_else(PoisonError::into_inner);
    // Lida com o cadeado na mão: duas conferidas ao mesmo tempo aplicariam as
    // situações fora de ordem, e a mais velha ganharia.
    let situacao = situacao_de(app, &estado);
    let agora = nucleo.conferir(&situacao, &estado.porta, &DoSistema(app));
    let retrato = Retrato {
        ligada: situacao.ligada,
        estado: agora,
        tecla: situacao.tecla,
    };
    let mudou = {
        let mut guardado = estado
            .retrato
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mudou = *guardado != retrato;
        guardado.clone_from(&retrato);
        mudou
    };
    drop(nucleo);
    if mudou {
        tracing::info!(estado = ?retrato.estado, tecla = %retrato.tecla, "a tecla de falar fora da janela mudou de estado");
        // Sem janela não há a quem contar, e o estado fica no retrato.
        let _ = app.emit(CANAL_DA_TECLA_GLOBAL, &retrato);
    }
    retrato
}

/// A situação de agora, lida da sessão, da janela e do que está anotado.
fn situacao_de<R: Runtime>(app: &AppHandle<R>, estado: &TeclaGlobal) -> Situacao {
    let ligada = estado.ligada.load(Ordering::Acquire);
    let tecla = estado
        .tecla
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    // Só com a opção ligada: desligada, nada disto decide coisa alguma, e a
    // conferida de quem nunca ligou não lê a sessão nem acorda a thread principal.
    let (na_sala, modo_tecla) = if ligada {
        crate::voz_para_a_tecla_global(app)
    } else {
        (false, false)
    };
    let janela = app.get_webview_window(JANELA_PRINCIPAL);
    let janela_na_frente = ligada
        && janela.as_ref().is_some_and(|janela| {
            janela.is_focused().unwrap_or_else(|erro| {
                // Sem saber, «não está na frente»: é a resposta que faz a opção
                // funcionar, e a volta do foco confere de novo.
                tracing::warn!(%erro, "não consegui saber se a janela está na frente");
                false
            })
        });
    Situacao {
        ligada,
        disponivel: estado.disponivel.load(Ordering::Acquire),
        janela_aberta: janela.is_some(),
        janela_na_frente,
        na_sala,
        modo_tecla,
        tecla,
    }
}

/// O rótulo da janela do `tauri.conf.json` (o padrão do Tauri, o mesmo de
/// `capabilities/janela.json`).
const JANELA_PRINCIPAL: &str = "main";

/// O sistema relatou a tecla: aperto ou soltura.
///
/// O microfone é mexido **com a porta na mão**, e é o que impede uma conferida
/// de fechar a porta entre a decisão de abrir e a abertura.
fn ao_evento_do_sistema<R: Runtime>(app: &AppHandle<R>, apertou: bool) {
    let Some(estado) = app.try_state::<TeclaGlobal>() else {
        return;
    };
    let mut porta = estado.porta.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(falar) = porta.ao_evento(apertou) {
        if let Some(sessao) = app.try_state::<crate::Session>() {
            crate::segurar_a_fala(&sessao, falar);
        }
    }
}

/// O atalho do plugin para um `KeyboardEvent.code`: a tecla sozinha, sem
/// modificador.
fn atalho_de(tecla: &str) -> Result<Shortcut, String> {
    Code::from_str(tecla)
        .map(|codigo| Shortcut::new(None, codigo))
        .map_err(|erro| erro.to_string())
}

/// O [`Sistema`] de verdade: o plugin, e o microfone da sessão.
struct DoSistema<'a, R: Runtime>(&'a AppHandle<R>);

impl<R: Runtime> DoSistema<'_, R> {
    fn atalhos(&self) -> Result<tauri::State<'_, GlobalShortcut<R>>, String> {
        self.0
            .try_state::<GlobalShortcut<R>>()
            .ok_or_else(|| "o serviço de atalhos não subiu".to_owned())
    }
}

impl<R: Runtime> Sistema for DoSistema<'_, R> {
    fn registrar(&self, tecla: &str) -> Result<(), String> {
        self.atalhos()?
            .register(atalho_de(tecla)?)
            .map_err(|erro| erro.to_string())
    }

    fn desregistrar(&self, tecla: &str) -> Result<(), String> {
        self.atalhos()?
            .unregister(atalho_de(tecla)?)
            .map_err(|erro| erro.to_string())
    }

    fn fechar_o_microfone(&self) {
        if let Some(sessao) = self.0.try_state::<crate::Session>() {
            crate::segurar_a_fala(&sessao, false);
        }
    }
}

#[cfg(test)]
mod testes {
    use std::cell::RefCell;
    use std::sync::Mutex;

    use super::{
        estado_de, o_que_registrar, serve_de_atalho, Estado, Nucleo, Porta, Sistema, Situacao,
    };

    /// Um sistema de mentira: lembra o que lhe pediram, e recusa o que o teste
    /// mandar recusar.
    #[derive(Default)]
    struct DeMentira {
        /// Tudo o que foi pedido, em ordem: `+F`, `-F`, `mic`.
        pedidos: RefCell<Vec<String>>,
        /// As teclas que o sistema recusa.
        tomadas: RefCell<Vec<String>>,
    }

    impl DeMentira {
        fn pedidos(&self) -> Vec<String> {
            self.pedidos.borrow().clone()
        }
    }

    impl Sistema for DeMentira {
        fn registrar(&self, tecla: &str) -> Result<(), String> {
            self.pedidos.borrow_mut().push(format!("+{tecla}"));
            if self.tomadas.borrow().iter().any(|tomada| tomada == tecla) {
                return Err("HotKey already registered".to_owned());
            }
            Ok(())
        }

        fn desregistrar(&self, tecla: &str) -> Result<(), String> {
            self.pedidos.borrow_mut().push(format!("-{tecla}"));
            Ok(())
        }

        fn fechar_o_microfone(&self) {
            self.pedidos.borrow_mut().push("mic".to_owned());
        }
    }

    /// Uma mudança na situação, como o teste a escreve.
    type Mudanca = fn(&mut Situacao);
    /// O nome de um caso, a mudança, e o estado que ela deixa.
    type Caso = (&'static str, Mudanca, Estado);

    /// Tudo a favor: ligada, na sala, por tecla, a janela atrás, a tecla F.
    fn no_jogo() -> Situacao {
        Situacao {
            ligada: true,
            disponivel: true,
            janela_aberta: true,
            janela_na_frente: false,
            na_sala: true,
            modo_tecla: true,
            tecla: "KeyF".to_owned(),
        }
    }

    #[test]
    fn a_tecla_so_e_do_sistema_com_todas_as_condicoes() {
        assert_eq!(o_que_registrar(&no_jogo()), Some("KeyF"));
        let faltando: [(&str, Mudanca); 6] = [
            ("a opção desligada", |s| s.ligada = false),
            ("sem o serviço de atalhos", |s| s.disponivel = false),
            ("a janela fechada", |s| s.janela_aberta = false),
            ("a janela na frente", |s| s.janela_na_frente = true),
            ("fora de sala", |s| s.na_sala = false),
            ("o microfone por voz", |s| s.modo_tecla = false),
        ];
        for (nome, tirar) in faltando {
            let mut situacao = no_jogo();
            tirar(&mut situacao);
            assert_eq!(
                o_que_registrar(&situacao),
                None,
                "com {nome} a tecla continuou sendo pedida ao sistema"
            );
        }
    }

    #[test]
    fn modificador_sozinho_e_codigo_desconhecido_nao_servem_de_atalho() {
        for serve in ["Space", "KeyF", "F9", "Backquote", "CapsLock", "Numpad0"] {
            assert!(serve_de_atalho(serve), "{serve} foi recusada");
        }
        for nao_serve in [
            "ShiftLeft",
            "ShiftRight",
            "ControlLeft",
            "AltRight",
            "MetaLeft",
            "OSLeft",
            "",
            "TeclaQueNaoExiste",
        ] {
            assert!(!serve_de_atalho(nao_serve), "{nao_serve:?} foi aceita");
        }
        let mut situacao = no_jogo();
        situacao.tecla = "ShiftLeft".to_owned();
        assert_eq!(o_que_registrar(&situacao), None);
        // Dita em qualquer situação, e não só no jogo: é ao ligar a opção, com
        // a janela na frente e fora de sala, que a pessoa pode trocar a tecla.
        situacao.janela_na_frente = true;
        situacao.na_sala = false;
        assert_eq!(estado_de(&situacao, false, false), Estado::SemAtalho);
    }

    #[test]
    fn cada_situacao_tem_o_seu_estado() {
        let com = |mudar: fn(&mut Situacao)| {
            let mut situacao = no_jogo();
            mudar(&mut situacao);
            situacao
        };
        assert_eq!(
            estado_de(&com(|s| s.ligada = false), false, false),
            Estado::Desligada
        );
        assert_eq!(
            estado_de(&com(|s| s.disponivel = false), false, false),
            Estado::Indisponivel
        );
        assert_eq!(
            estado_de(&com(|s| s.na_sala = false), false, false),
            Estado::ForaDeSala
        );
        assert_eq!(
            estado_de(&com(|s| s.modo_tecla = false), false, false),
            Estado::OutroModo
        );
        assert_eq!(
            estado_de(&com(|s| s.janela_na_frente = true), false, false),
            Estado::NaJanela
        );
        assert_eq!(estado_de(&no_jogo(), true, false), Estado::Valendo);
        assert_eq!(estado_de(&no_jogo(), false, true), Estado::Recusada);
        // A recusa lembrada é dita com a janela na frente: é quando ela é lida.
        assert_eq!(
            estado_de(&com(|s| s.janela_na_frente = true), false, true),
            Estado::Recusada
        );
    }

    #[test]
    fn registra_ao_sair_da_janela_e_devolve_ao_voltar() {
        let (sistema, porta) = (DeMentira::default(), Mutex::new(Porta::default()));
        let mut nucleo = Nucleo::default();
        let mut situacao = no_jogo();
        situacao.janela_na_frente = true;
        assert_eq!(
            nucleo.conferir(&situacao, &porta, &sistema),
            Estado::NaJanela
        );
        assert!(
            sistema.pedidos().is_empty(),
            "com a janela na frente a tecla foi pedida ao sistema"
        );

        situacao.janela_na_frente = false;
        assert_eq!(
            nucleo.conferir(&situacao, &porta, &sistema),
            Estado::Valendo
        );
        // Conferir de novo sem nada mudar não pede de novo.
        assert_eq!(
            nucleo.conferir(&situacao, &porta, &sistema),
            Estado::Valendo
        );
        assert_eq!(sistema.pedidos(), ["+KeyF"]);

        situacao.janela_na_frente = true;
        assert_eq!(
            nucleo.conferir(&situacao, &porta, &sistema),
            Estado::NaJanela
        );
        assert_eq!(sistema.pedidos(), ["+KeyF", "-KeyF"]);
    }

    #[test]
    fn devolve_ao_sair_da_sala_ao_trocar_de_modo_ao_desligar_e_ao_fechar() {
        let saidas: [Caso; 4] = [
            ("sair da sala", |s| s.na_sala = false, Estado::ForaDeSala),
            (
                "trocar de modo",
                |s| s.modo_tecla = false,
                Estado::OutroModo,
            ),
            ("desligar a opção", |s| s.ligada = false, Estado::Desligada),
            // Fechar a janela: o estado é o de quem não tem janela para ler, e
            // o que importa é a tecla voltar ao sistema.
            (
                "fechar a janela",
                |s| s.janela_aberta = false,
                Estado::NaJanela,
            ),
        ];
        for (nome, sair, esperado) in saidas {
            let (sistema, porta) = (DeMentira::default(), Mutex::new(Porta::default()));
            let mut nucleo = Nucleo::default();
            let mut situacao = no_jogo();
            assert_eq!(
                nucleo.conferir(&situacao, &porta, &sistema),
                Estado::Valendo
            );
            sair(&mut situacao);
            assert_eq!(
                nucleo.conferir(&situacao, &porta, &sistema),
                esperado,
                "{nome}"
            );
            assert_eq!(
                sistema.pedidos(),
                ["+KeyF", "-KeyF"],
                "{nome} não devolveu a tecla ao sistema"
            );
        }
    }

    #[test]
    fn trocar_a_tecla_com_ela_valendo_troca_no_sistema() {
        let (sistema, porta) = (DeMentira::default(), Mutex::new(Porta::default()));
        let mut nucleo = Nucleo::default();
        let mut situacao = no_jogo();
        nucleo.conferir(&situacao, &porta, &sistema);
        situacao.tecla = "KeyG".to_owned();
        assert_eq!(
            nucleo.conferir(&situacao, &porta, &sistema),
            Estado::Valendo
        );
        assert_eq!(sistema.pedidos(), ["+KeyF", "-KeyF", "+KeyG"]);
    }

    #[test]
    fn a_recusa_do_sistema_e_dita_e_lembrada_ate_deixar_de_valer() {
        let (sistema, porta) = (DeMentira::default(), Mutex::new(Porta::default()));
        sistema.tomadas.borrow_mut().push("KeyF".to_owned());
        let mut nucleo = Nucleo::default();
        let mut situacao = no_jogo();
        assert_eq!(
            nucleo.conferir(&situacao, &porta, &sistema),
            Estado::Recusada,
            "o sistema recusou a tecla e o estado não disse"
        );
        // Recusada, a tecla não vale: um aperto que chegasse não abre nada.
        assert_eq!(
            porta.lock().expect("porta").ao_evento(true),
            None,
            "a tecla recusada abriu o microfone"
        );
        // A mesma pergunta não é repetida a cada conferida.
        nucleo.conferir(&situacao, &porta, &sistema);
        assert_eq!(sistema.pedidos(), ["+KeyF"]);

        // De volta à janela, a recusa continua dita: é quando a pessoa a lê.
        situacao.janela_na_frente = true;
        assert_eq!(
            nucleo.conferir(&situacao, &porta, &sistema),
            Estado::Recusada
        );

        // Saindo de novo, pergunta de novo: o outro programa pode ter saído.
        sistema.tomadas.borrow_mut().clear();
        situacao.janela_na_frente = false;
        assert_eq!(
            nucleo.conferir(&situacao, &porta, &sistema),
            Estado::Valendo
        );
        assert_eq!(sistema.pedidos(), ["+KeyF", "+KeyF"]);
    }

    #[test]
    fn a_recusa_some_com_o_que_a_tornaria_historia() {
        let esquecem: [Caso; 3] = [
            ("sair da sala", |s| s.na_sala = false, Estado::ForaDeSala),
            (
                "trocar de modo",
                |s| s.modo_tecla = false,
                Estado::OutroModo,
            ),
            (
                "trocar de tecla",
                |s| s.tecla = "KeyG".to_owned(),
                Estado::NaJanela,
            ),
        ];
        for (nome, mudar, esperado) in esquecem {
            let (sistema, porta) = (DeMentira::default(), Mutex::new(Porta::default()));
            sistema.tomadas.borrow_mut().push("KeyF".to_owned());
            let mut nucleo = Nucleo::default();
            let mut situacao = no_jogo();
            nucleo.conferir(&situacao, &porta, &sistema);
            situacao.janela_na_frente = true;
            mudar(&mut situacao);
            assert_eq!(
                nucleo.conferir(&situacao, &porta, &sistema),
                esperado,
                "{nome} não apagou a recusa"
            );
        }
    }

    #[test]
    fn apertar_abre_soltar_fecha_e_so_o_que_a_tecla_abriu() {
        let mut porta = Porta::default();
        // Sem valer, um aperto não abre, e a soltura dele não fecha nada: o
        // microfone pode estar seguro pela tecla da janela.
        assert_eq!(porta.ao_evento(true), None);
        assert_eq!(porta.ao_evento(false), None);

        porta.valer();
        assert_eq!(porta.ao_evento(true), Some(true));
        // A repetição do teclado não abre de novo.
        assert_eq!(porta.ao_evento(true), None);
        assert_eq!(porta.ao_evento(false), Some(false));
        assert_eq!(porta.ao_evento(false), None);
    }

    #[test]
    fn soltar_fecha_mesmo_depois_de_a_tecla_deixar_de_valer() {
        let mut porta = Porta::default();
        porta.valer();
        assert_eq!(porta.ao_evento(true), Some(true));
        // A conferida fecha a porta e diz que há microfone a fechar…
        assert!(porta.deixar_de_valer());
        // …e depois disso nem o aperto abre, nem a soltura fecha duas vezes.
        assert_eq!(porta.ao_evento(true), None);
        assert_eq!(porta.ao_evento(false), None);
    }

    #[test]
    fn devolver_a_tecla_com_ela_apertada_fecha_o_microfone_antes() {
        let (sistema, porta) = (DeMentira::default(), Mutex::new(Porta::default()));
        let mut nucleo = Nucleo::default();
        let mut situacao = no_jogo();
        nucleo.conferir(&situacao, &porta, &sistema);
        assert_eq!(porta.lock().expect("porta").ao_evento(true), Some(true));

        // Expulsa da sala com a tecla apertada.
        situacao.na_sala = false;
        nucleo.conferir(&situacao, &porta, &sistema);
        assert_eq!(
            sistema.pedidos(),
            ["+KeyF", "mic", "-KeyF"],
            "o microfone ficou aberto, ou fechou depois de a tecla ser devolvida"
        );
        // A soltura que ainda chegar não tem o que fechar.
        assert_eq!(porta.lock().expect("porta").ao_evento(false), None);
    }

    #[test]
    fn devolver_a_tecla_solta_nao_mexe_no_microfone() {
        let (sistema, porta) = (DeMentira::default(), Mutex::new(Porta::default()));
        let mut nucleo = Nucleo::default();
        let mut situacao = no_jogo();
        nucleo.conferir(&situacao, &porta, &sistema);
        situacao.janela_na_frente = true;
        nucleo.conferir(&situacao, &porta, &sistema);
        assert_eq!(
            sistema.pedidos(),
            ["+KeyF", "-KeyF"],
            "voltar à janela fechou um microfone que a tecla do sistema não abriu"
        );
    }
}
