//! O retrato sem rede: uma conexão que não existe, com o mesmo `fold` e o mesmo
//! `snapshot` de uma que existe.
//!
//! Para os testes deste crate, que dobram mensagens à mão desde sempre, e para
//! a conformidade dos dois produtores de retrato
//! (`crates/seele-conformance/tests/dois_produtores_de_snapshot.rs`, fase 2 da
//! Tarefa 2.2 da F4). Lá, o `seele-web` recebe as mesmas mensagens, e os dois
//! retratos têm de ser o mesmo campo a campo.
//!
//! Fora dos testes, só existe com a feature `conformidade`, que só o
//! `seele-conformance` liga: o produto não leva uma conexão de mentira.

use std::sync::atomic::{AtomicBool, AtomicU8};
use std::sync::{Arc, Mutex};

use crate::{EscolhaDeTela, Room, Shared, VozDaSala};

/// O `Shared` de uma conexão que nunca abriu, com `apelido` digitado.
pub(crate) fn shared(apelido: &str) -> Arc<Shared> {
    Arc::new(Shared {
        caminho_medido: std::sync::atomic::AtomicU32::new(0),
        link_battery: AtomicBool::new(false),
        link_seconds: std::sync::atomic::AtomicU64::new(0),
        link_attempts: std::sync::atomic::AtomicU32::new(0),
        messages_revision: std::sync::atomic::AtomicU64::new(0),
        fora_do_ar_revision: std::sync::atomic::AtomicU64::new(0),
        icon_revision: std::sync::atomic::AtomicU64::new(0),
        person_icons_revision: std::sync::atomic::AtomicU64::new(0),
        room: Mutex::new(Room::new()),
        listeners: Mutex::new(Vec::new()),
        voice: VozDaSala::nova(false, seele_core::DeviceChoice::default()),
        media: Mutex::new(None),
        nickname: Mutex::new(apelido.into()),
        link_state: AtomicU8::new(0),
        rtt_micros: std::sync::atomic::AtomicU64::new(0),
        jitter_de_chegada_micros: std::sync::atomic::AtomicU64::new(0),
        caminho: Mutex::new(None),
        impressao_do_servidor: Mutex::new(None),
        signal: AtomicU8::new(0),
        running: AtomicBool::new(true),
        pending_weights: Mutex::new(Vec::new()),
        limites_da_tela: Mutex::new(None),
        tela_escolhida: Mutex::new(EscolhaDeTela::NinguemEscolheu),
        fontes_de_tela: Mutex::new(Vec::new()),
    })
}

/// Os retratos que o desktop desenharia: um logo depois da entrada, e um
/// depois de cada mensagem.
///
/// A entrada é o `room.adopt(client.sessao(), &config.nickname)` do `connect`,
/// e cada mensagem passa pelo `fold` da sessão viva, inteiro. O retrato é o
/// `Connection::snapshot`.
#[cfg(feature = "conformidade")]
#[must_use]
pub fn retratos(
    sessao: &seele_core::SessionInfo,
    apelido: &str,
    mensagens: &[seele_core::ServerMessage],
) -> Vec<crate::Snapshot> {
    let shared = shared(apelido);
    if let Ok(mut room) = shared.room.lock() {
        room.adopt(sessao, apelido);
    }
    let (commands, _fila) = tokio::sync::mpsc::unbounded_channel();
    let conexao = crate::Connection {
        commands,
        shared: Arc::clone(&shared),
        encerrada: Mutex::new(None),
        ligacao: None,
    };
    let mut retratos = vec![conexao.snapshot()];
    for mensagem in mensagens {
        crate::fold(&shared, mensagem);
        retratos.push(conexao.snapshot());
    }
    retratos
}

/// O que a casca do desktop ouviria de uma mensagem: os eventos, na ordem em
/// que saíram, e os quatro números que quem dobra conta, lidos logo depois.
#[cfg(feature = "conformidade")]
#[derive(Debug, Clone, PartialEq)]
pub struct Ouvido {
    /// Os eventos que o `fold` mandou aos ouvintes.
    pub eventos: Vec<crate::Event>,
    /// O `icon_revision` do retrato.
    pub icon_revision: u64,
    /// O `person_icons_revision` do retrato.
    pub person_icons_revision: u64,
    /// O `messages_revision` do retrato.
    pub messages_revision: u64,
    /// O `fora_do_ar_revision` do retrato.
    pub fora_do_ar_revision: u64,
}

/// Os eventos que o desktop ouviria, mensagem por mensagem: a mesma entrada de
/// [`retratos`], com um ouvinte inscrito pelo `Connection::subscribe` de
/// verdade, e o `fold` inteiro.
#[cfg(feature = "conformidade")]
#[must_use]
pub fn eventos(
    sessao: &seele_core::SessionInfo,
    apelido: &str,
    mensagens: &[seele_core::ServerMessage],
) -> Vec<Ouvido> {
    struct Anota(Mutex<Vec<crate::Event>>);
    impl crate::EventListener for Anota {
        fn on_event(&self, event: crate::Event) {
            if let Ok(mut anotados) = self.0.lock() {
                anotados.push(event);
            }
        }
    }

    let shared = shared(apelido);
    if let Ok(mut room) = shared.room.lock() {
        room.adopt(sessao, apelido);
    }
    let (commands, _fila) = tokio::sync::mpsc::unbounded_channel();
    let conexao = crate::Connection {
        commands,
        shared: Arc::clone(&shared),
        encerrada: Mutex::new(None),
        ligacao: None,
    };
    let anota = Arc::new(Anota(Mutex::new(Vec::new())));
    conexao.subscribe(Arc::clone(&anota) as Arc<dyn crate::EventListener>);
    mensagens
        .iter()
        .map(|mensagem| {
            crate::fold(&shared, mensagem);
            let eventos = anota
                .0
                .lock()
                .map(|mut anotados| std::mem::take(&mut *anotados))
                .unwrap_or_default();
            let retrato = conexao.snapshot();
            Ouvido {
                eventos,
                icon_revision: retrato.icon_revision,
                person_icons_revision: retrato.person_icons_revision,
                messages_revision: retrato.messages_revision,
                fora_do_ar_revision: retrato.fora_do_ar_revision,
            }
        })
        .collect()
}

/// O que a casca do desktop recebe de uma entrada que falhou: a regra do
/// `classify_connect_failure`, para a conformidade dos dois produtores de
/// recusa (`dois_produtores_de_recusa.rs`). O navegador recusa pela
/// `seele_web::ligacao`, e a frase na tela tem de ser a mesma.
#[cfg(feature = "conformidade")]
#[must_use]
pub fn recusa(erro: &seele_core::ConnectError) -> crate::ConnectionError {
    crate::classify_connect_failure(erro)
}
