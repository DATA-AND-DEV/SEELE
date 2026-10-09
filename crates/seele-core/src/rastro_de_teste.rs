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
//!
//! O que um rastro vê não depende de outro teste ter passado antes pela mesma
//! linha sem captura: [`Rastro::a_partir_de`] põe de pé o
//! [`segundo_despachante`] (pendência 56).

use std::sync::{Arc, Mutex};

/// **Um segundo despachante, vivo o processo inteiro, que não escreve nada.**
/// É ele que tira do [`Rastro`] a dependência da ordem das threads. O mesmo
/// auxiliar do app (`apps/seele-app/src/main.rs`, `rastro_de_teste`).
///
/// O `tracing` decide uma vez só, na primeira passagem de qualquer thread, se
/// uma linha que escreve (o *callsite*) tem quem a ouça, e guarda a resposta.
/// Se, no último registro de despachante, só havia um vivo, a pergunta vai só
/// ao despachante da **thread que passou** (`tracing-core` 0.1.36,
/// `callsite.rs`: `Dispatchers::rebuilder` devolve `Rebuilder::JustOne`, e ele
/// consulta `dispatcher::get_default`). Um teste sem captura que passasse
/// primeiro por uma linha, com o rastro de outro teste como único despachante
/// de pé, a deixava marcada como «nunca» — e a mesma linha, no teste que a
/// captura, não escrevia nada.
///
/// Com dois vivos, a pergunta vai a todos, e uma linha que o rastro ouve fica
/// em «às vezes»: cada evento pergunta de novo, à thread em que acontece. O
/// que um rastro vê não muda — só o que a thread dele escreveu.
pub(crate) fn segundo_despachante() {
    static SEMPRE: std::sync::OnceLock<tracing::Dispatch> = std::sync::OnceLock::new();
    SEMPRE.get_or_init(|| tracing::Dispatch::new(tracing::subscriber::NoSubscriber::new()));
}

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
    ///
    /// Põe de pé o [`segundo_despachante`] antes de o rastro existir: todo
    /// teste de log deste crate passa por aqui, e é aqui que ele deixa de
    /// depender da ordem das threads.
    pub(crate) fn a_partir_de(nivel: tracing::Level) -> Arc<Self> {
        segundo_despachante();
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

/// **Uma linha que outra thread tocou primeiro, fora de qualquer captura,
/// ainda é capturada.** A prova de ordem forçada da pendência 56, a mesma do
/// app (`apps/seele-app/src/main.rs`, `rastro_de_teste`), feita pelo
/// [`Rastro`].
///
/// É a ordem que fazia um teste deste crate passar ou falhar por sorte: um
/// teste sem captura passava por uma linha antes do teste que a captura, e a
/// linha sumia do rastro deste. Aqui a ordem é forçada — a outra thread toca a
/// linha primeiro, com o rastro já de pé —, e o resultado deixa de depender de
/// quem chega antes. E a linha da outra thread continua fora: o rastro segue
/// sendo só da thread corrente.
#[test]
fn uma_linha_que_outra_thread_tocou_primeiro_ainda_e_capturada() {
    fn diz() {
        tracing::warn!("linha tocada primeiro fora da captura");
    }
    let rastro = Rastro::a_partir_de(tracing::Level::WARN);
    let _guarda = tracing::subscriber::set_default(rastro.clone());
    std::thread::spawn(diz)
        .join()
        .expect("a outra thread terminou");
    diz();

    let linhas = rastro.linhas();
    assert!(
        linhas
            .iter()
            .any(|linha| linha.contains("linha tocada primeiro fora da captura")),
        "uma linha que outra thread tocou primeiro sumiu do rastro, e um guarda que a capture \
         passa ou falha conforme a ordem das threads: {linhas:?}"
    );
    assert_eq!(
        linhas.len(),
        1,
        "a linha da outra thread entrou neste rastro, e um teste passaria a ler o que outro \
         teste escreveu: {linhas:?}"
    );
}
