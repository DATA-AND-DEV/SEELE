//! O rastro que um teste lê: um [`tracing::Subscriber`] escrito à mão.
//!
//! «O produto sabe e não conta» é a classe de defeito que mais custou neste
//! repositório, e o guarda dela é um teste que lê o `seele.log`. Este crate não
//! tem `tracing-subscriber` nas dependências de teste, e acrescentá-la só para
//! isto seria uma dependência nova; o `tracing` já expõe o necessário.
//!
//! Eram três cópias deste `Subscriber`, uma em cada módulo de teste que
//! precisou (`encontro.rs`, `par.rs` e `client.rs`), cada uma com o próprio
//! formato de linha. Agora é uma, com o nível mínimo escolhido por quem chama.
//!
//! # Como usar
//!
//! ```ignore
//! let rastro = Rastro::a_partir_de(tracing::Level::WARN);
//! let _guarda = tracing::subscriber::set_default(rastro.clone());
//! // … o que se mede …
//! let linhas = rastro.linhas();
//! ```
//!
//! `set_default` fixa o `Subscriber` só na thread corrente. Um
//! `#[tokio::test]` de thread única roda nela toda tarefa que o teste
//! dispara, e é o que faz o rastro das tarefas chegar aqui; numa runtime
//! `multi_thread`, uma tarefa que caia noutra thread some deste rastro.

use std::sync::{Arc, Mutex};

/// Guarda cada evento de um nível para cima como uma linha
/// `NÍVEL: mensagem campo=valor …`.
///
/// Os campos entram na linha porque o que um log de verdade precisa dizer
/// (qual endereço, qual impressão) costuma estar neles, e não na mensagem.
pub(crate) struct Rastro {
    nivel: tracing::Level,
    linhas: Mutex<Vec<String>>,
}

impl Rastro {
    /// Um rastro que guarda os eventos de `nivel` para cima.
    ///
    /// O nível existe porque o barulho é outro em cada teste: um que lê `INFO`
    /// (o nível que o `seele.log` grava) quer o que a pessoa leria no arquivo;
    /// um que só procura um `WARN` não quer as centenas de linhas do aperto de
    /// mão do `quinn` na mensagem da asserção que falhar.
    pub(crate) fn a_partir_de(nivel: tracing::Level) -> Arc<Self> {
        Arc::new(Self {
            nivel,
            linhas: Mutex::new(Vec::new()),
        })
    }

    /// As linhas guardadas até agora, na ordem em que chegaram.
    pub(crate) fn linhas(&self) -> Vec<String> {
        self.linhas
            .lock()
            .map(|linhas| linhas.clone())
            .unwrap_or_default()
    }
}

impl tracing::Subscriber for Rastro {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        *metadata.level() <= self.nivel
    }

    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }

    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        struct Campos(String);
        impl tracing::field::Visit for Campos {
            fn record_debug(&mut self, field: &tracing::field::Field, valor: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.0.push_str(&format!(" {valor:?}"));
                } else {
                    self.0.push_str(&format!(" {}={valor:?}", field.name()));
                }
            }
        }
        let mut campos = Campos(String::new());
        event.record(&mut campos);
        if let Ok(mut linhas) = self.linhas.lock() {
            linhas.push(format!("{}:{}", event.metadata().level(), campos.0));
        }
    }

    fn enter(&self, _span: &tracing::span::Id) {}

    fn exit(&self, _span: &tracing::span::Id) {}
}
