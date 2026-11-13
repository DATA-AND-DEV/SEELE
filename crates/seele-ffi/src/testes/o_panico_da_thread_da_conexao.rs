//! A thread da conexão que entra em pânico derruba a sessão **dizendo**.
//!
//! Era silêncio: `drive` desenrolava, e o `Shared` ficava como estava, com
//! `running` em `true`, o enlace `Verified` e nenhum `Event::Ended`. A tela
//! mostrava uma sessão de pé que não recebia nem mandava nada.
//!
//! O gancho de teste é o `corpo` de [`subir_a_thread_da_conexao`]: no produto
//! ele é o runtime com o `drive` dentro, e aqui é uma função que deixa o
//! `Shared` como `drive` o deixa depois do aperto de mão, e entra em pânico. A
//! thread, o amparo e o desmonte são os do produto.

use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use super::{
    link_state_byte, subir_a_thread_da_conexao, ConnectionError, EndReason, Event, EventListener,
    LinkTrust,
};

/// Quanto o teste espera pelo fim. A thread só tem o pânico para fazer; o
/// prazo é o que separa «caiu dizendo» de «ficou pendurada».
const PRAZO: Duration = Duration::from_secs(5);

/// Quem ouve a sessão, como a casca: cada evento vai para o canal do teste.
struct Ouvido(Mutex<mpsc::Sender<Event>>);

impl EventListener for Ouvido {
    fn on_event(&self, event: Event) {
        if let Ok(para) = self.0.lock() {
            let _ = para.send(event);
        }
    }
}

#[test]
fn a_bomba_que_entra_em_panico_derruba_a_sessao_dizendo_o_motivo() {
    let shared = super::sem_rede::shared("marcela");
    let (para, eventos) = mpsc::channel();
    shared
        .listeners
        .lock()
        .expect("o cadeado dos ouvintes de uma sessão nova")
        .push(Arc::new(Ouvido(Mutex::new(para))));
    let (ready, _pronta) = mpsc::channel();

    let da_thread = Arc::clone(&shared);
    subir_a_thread_da_conexao(Arc::clone(&shared), ready, move |_| {
        // Como `drive` deixa depois do aperto de mão: a sessão de pé.
        da_thread.running.store(true, Ordering::Relaxed);
        da_thread
            .link_state
            .store(link_state_byte(LinkTrust::Verified), Ordering::Relaxed);
        panic!("o gancho de teste: a bomba da sessão entrou em pânico");
    })
    .expect("a thread da conexão sobe");

    let fim = eventos.recv_timeout(PRAZO);
    assert_eq!(
        fim,
        Ok(Event::Ended {
            reason: EndReason::MotorCaiu
        }),
        "a thread da conexão entrou em pânico e a sessão ficou pendurada: nenhum \
         `Event::Ended {{ MotorCaiu }}` chegou a quem ouve em {PRAZO:?}"
    );
    // O evento é a última coisa que o amparo faz: quando ele chega, o resto já
    // está escrito.
    assert!(
        !shared.running.load(Ordering::Relaxed),
        "a sessão caiu e `running` segue `true`: a casca ainda a lê como de pé"
    );
    assert_eq!(
        shared.link_state.load(Ordering::Relaxed),
        link_state_byte(LinkTrust::Offline),
        "a sessão caiu e o enlace segue dito como verificado"
    );
}

#[test]
fn o_panico_antes_do_aperto_de_mao_responde_a_quem_espera() {
    let shared = super::sem_rede::shared("marcela");
    let (ready, pronta) = mpsc::channel();
    subir_a_thread_da_conexao(shared, ready, |_| {
        panic!("o gancho de teste: o pânico veio antes do aperto de mão");
    })
    .expect("a thread da conexão sobe");

    // `Ok(Err(..))`, e não o canal largado: quem ampara responde, com o erro.
    let resposta = pronta
        .recv_timeout(PRAZO)
        .map(|resposta| resposta.map(|_| ()).map_err(|falha| falha.error));
    assert_eq!(
        resposta,
        Ok(Err(ConnectionError::Unreachable)),
        "o pânico antes do aperto de mão não respondeu a quem esperava a conexão"
    );
}
