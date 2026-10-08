//! O toque da sala pessoal abre só a saída.
//!
//! Spec de 23/09: o toque toca «sem abrir o microfone». No macOS, abrir a
//! entrada acende o indicador de microfone e pode pedir permissão: alguém que
//! recebe uma ligação veria o microfone ligar antes de atender.
//!
//! **Uma medida, e não um teste de CI.** Precisa de uma saída de áudio de
//! verdade, e é por isso que é `#[ignore]`. Roda à mão, neste Mac:
//!
//! ```text
//! cargo test -p seele-audio --test toque_so_de_saida -- --ignored --test-threads=1
//! ```
//!
//! O toque é aberto **mudo**: a propriedade medida é o que se abre, e não o
//! que soa.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use seele_audio::device::{fluxos_de_entrada_construidos, Tocador};

#[test]
#[ignore = "precisa de uma saída de áudio de verdade; roda à mão neste Mac"]
fn o_toque_toca_sem_abrir_nenhum_fluxo_de_entrada() {
    let antes = fluxos_de_entrada_construidos();
    let tocador = Tocador::tocar(None, 0.0).expect("a saída padrão desta máquina não abriu");
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        tocador.saida().is_some(),
        "o toque não diz em que saída abriu"
    );
    drop(tocador);
    assert_eq!(
        fluxos_de_entrada_construidos(),
        antes,
        "tocar o toque abriu um fluxo de entrada: o microfone de quem recebe acenderia"
    );
}

/// O controle negativo: sem ele, um contador que nunca sobe passaria no teste
/// de cima. Abrir o par inteiro (`device::open`) sobe o contador.
#[test]
#[ignore = "precisa de microfone e saída de verdade; roda à mão neste Mac"]
fn abrir_o_par_inteiro_conta_um_fluxo_de_entrada() {
    let antes = fluxos_de_entrada_construidos();
    let par = seele_audio::device::open_default(20).expect("o par padrão desta máquina não abriu");
    drop(par);
    assert_eq!(
        fluxos_de_entrada_construidos(),
        antes + 1,
        "o contador de fluxos de entrada não contou a entrada da voz"
    );
}
