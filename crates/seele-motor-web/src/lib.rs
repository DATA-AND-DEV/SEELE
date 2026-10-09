//! O motor de MODs do celular (ADR 0057, D-web-1): **a volta do desktop**, num
//! Worker.
//!
//! O [`seele_motor::Motor`] é o mesmo que o `executor.rs` do desktop embrulha
//! numa thread: o mesmo QuickJS, o mesmo prelúdio, os mesmos tetos, o mesmo
//! balde do `console` e o mesmo rastreador de rejeições. Aqui ele é embrulhado
//! por uma [`Instancia`], que o Worker chama pela [`fronteira`] e que espera do
//! jeito do navegador: o Worker pergunta o [`Instancia::proximo_prazo_ms`] e
//! agenda um `setTimeout`, no lugar do `recv_timeout` da thread.
//!
//! **Onde roda:** um `.wasm` do `wasm32-wasip1`, num Worker que a página cria
//! de dentro de um `iframe` opaco: a forma AB do spike `mods-no-navegador`, que
//! a orquestradora aceitou em 08/10/2026. O `.wasm` importa só a WASI mínima
//! que o Worker dá, e o código do MOD só alcança o que o motor põe no contexto.
//!
//! **Onde mora o `unsafe`:** só em `fronteira.rs`, que existe só no `wasm`. A
//! [`Instancia`] é segura e roda no nativo também.

use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

use seele_motor::{Fila, Interrupcao, Limites, Motor, ParaOFora};

#[cfg(target_family = "wasm")]
pub mod fronteira;

/// Um MOD de pé no Worker: o motor, a saída dele e a fila que a página
/// esvazia.
///
/// **A fila é a do desktop, e o crédito volta do mesmo jeito**: quando a
/// janela lê. No desktop é a colheita (`mod_nativo_colher`); aqui é a página
/// dizer que atendeu a mensagem ([`Instancia::lido`]) ou o aviso
/// ([`Instancia::aviso_lido`]).
pub struct Instancia {
    motor: Option<Motor>,
    recebe: Receiver<ParaOFora>,
    fila: Arc<Fila>,
}

impl Instancia {
    /// Sobe o motor com os tetos do produto e entrega o código do MOD, com as
    /// capacidades da API dele.
    ///
    /// Um motor que não sobe já disse por quê nas [falas](Instancia::falas),
    /// com a parada confirmada: o Worker as leva à página como de qualquer
    /// outro.
    #[must_use]
    pub fn subir(capacidades: &[&str], fonte: &str) -> Self {
        let (manda, recebe) = channel();
        let limites = Limites::default();
        let interrupcao = Arc::new(Interrupcao::nova(limites));
        let fila = Arc::new(Fila::default());
        let mut motor = Motor::subir(&manda, &interrupcao, &fila, limites);
        if let Some(motor) = motor.as_mut() {
            motor.codigo(fonte, capacidades);
        }
        Self {
            motor,
            recebe,
            fila,
        }
    }

    /// Uma resposta da janela ao MOD ([`Motor::resposta`]).
    pub fn resposta(&mut self, json: &str) {
        if let Some(motor) = self.motor.as_mut() {
            motor.resposta(json);
        }
    }

    /// Os temporizadores na hora ([`Motor::vencer`]).
    pub fn vencer(&mut self) {
        if let Some(motor) = self.motor.as_mut() {
            motor.vencer();
        }
    }

    /// O que o anfitrião tem de pé ([`Motor::diagnostico`]).
    pub fn diagnostico(&mut self) {
        if let Some(motor) = self.motor.as_mut() {
            motor.diagnostico();
        }
    }

    /// Em quantos milissegundos o Worker chama [`Instancia::vencer`], ou `-1`
    /// para esperar só a próxima entrada ([`Motor::proximo_prazo`]).
    #[must_use]
    pub fn proximo_prazo_ms(&self) -> f64 {
        self.motor
            .as_ref()
            .and_then(Motor::proximo_prazo)
            .map_or(-1.0, |prazo| prazo.as_secs_f64() * 1000.0)
    }

    /// A página atendeu uma mensagem de `bytes` bytes: o lugar volta à fila.
    pub fn lido(&self, bytes: usize) {
        self.fila.tirar(bytes);
    }

    /// A página atendeu um aviso (`falhou` ou `interrompido`).
    pub fn aviso_lido(&self) {
        self.fila.tirar_aviso();
    }

    /// Para ([`Motor::parar`]): as rejeições e a conta do console que ficaram
    /// saem antes do `parou`.
    pub fn parar(&mut self) {
        if let Some(motor) = self.motor.take() {
            motor.parar();
        }
    }

    /// Quanto o motor alocou, em bytes, para quem mede o custo.
    #[must_use]
    pub fn memoria(&self) -> i64 {
        self.motor.as_ref().map_or(0, Motor::memoria)
    }

    /// O que o motor disse desde a última vez: quantas falas, e a lista delas
    /// em JSON ([`ParaOFora::em_json`]).
    #[must_use]
    pub fn falas(&self) -> (usize, String) {
        let falas: Vec<serde_json::Value> =
            self.recebe.try_iter().map(|fala| fala.em_json()).collect();
        (falas.len(), serde_json::Value::Array(falas).to_string())
    }
}
