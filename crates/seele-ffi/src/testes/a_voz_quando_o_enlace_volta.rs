//! O braço da reconexão, exercitado pelo que ele **faz**.
//!
//! Complementa o guarda de texto-fonte de
//! `crates/seele-conformance/tests/voz_na_reconexao.rs`, que só conseguia
//! afirmar que a palavra `reopen` aparecia no braço. Três coisas que aquele
//! guarda deixaria passar são defeitos de verdade, e são estas:
//!
//! - reabrir **a partir** da voz viva, e não construir uma do zero — é o
//!   que carrega mudo, Isolamento total, modo e ganhos por cima da
//!   reabertura;
//! - abrir voz para quem estava em texto puro, que devolveria microfone a
//!   quem não tem nenhum;
//! - guardar uma voz que não reabriu, que deixaria a casca falando por uma
//!   conexão morta em vez de esmaecer para texto.
//!
//! Desde a pendência 52 a reabertura corre na thread de `esperar_o_audio`,
//! e a voz nova chega pelo braço da porta: os três testes passam por
//! `PortaDaVoz::na_volta` e `PortaDaVoz::chegou`, que é o caminho de `drive`.

use super::{Aberta, AberturaDoAudio, ControlesGuardados, Futuro, PortaDaVoz, VozDaSala, VozViva};
use seele_core::{DeviceChoice, VoiceRoomId};

/// Uma voz de mentira: o que interessa é o que atravessa a reabertura.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Voz {
    controles: Vec<&'static str>,
    ssrc: u32,
}

impl VozViva for Voz {
    fn vestir(&self, _: &ControlesGuardados) {}
    fn segurar(&self, _: bool) {}
    fn ganhar(&self, _: seele_core::Ssrc, _: f32) {}
}

fn entregue(aberta: Aberta<Voz>) -> Futuro<Voz> {
    Box::pin(async move { aberta })
}

fn nada_abre_do_zero(_: &DeviceChoice) -> Futuro<Voz> {
    panic!("a volta abriu uma voz do zero, sem partir da que estava viva")
}

/// Uma sala com esta voz de pé, como a volta a encontra.
fn na_sala_com(viva: Voz) -> (VozDaSala<Voz>, PortaDaVoz<Voz>) {
    let voz = VozDaSala::nova(true, DeviceChoice::default());
    *voz.lock().expect("a voz") = Some(viva);
    let mut porta = PortaDaVoz::nova();
    porta.olhar(&voz, Some(VoiceRoomId(1)), nada_abre_do_zero);
    (voz, porta)
}

async fn esperar(porta: &mut PortaDaVoz<Voz>, voz: &VozDaSala<Voz>) {
    let aberta = tokio::time::timeout(std::time::Duration::from_secs(5), porta.chegada())
        .await
        .expect("a reabertura não chegou");
    porta.chegou(voz, aberta, nada_abre_do_zero);
}

#[tokio::test]
async fn a_voz_viva_reabre_carregando_os_controles() {
    let (voz, mut porta) = na_sala_com(Voz {
        controles: vec!["mudo", "isolamento", "ganho de quem fala alto"],
        ssrc: 1,
    });

    porta.na_volta(
        &voz,
        |atual| {
            // A reabertura parte da voz que estava viva: é daí que os
            // controles vêm. Quem chama `start_on` não precisa deste `atual`.
            entregue(Ok(Ok(Voz {
                controles: atual.controles,
                ssrc: 2,
            })))
        },
        nada_abre_do_zero,
    );
    esperar(&mut porta, &voz).await;

    assert_eq!(
        voz.lock().expect("a voz").clone(),
        Some(Voz {
            controles: vec!["mudo", "isolamento", "ganho de quem fala alto"],
            ssrc: 2,
        }),
        "a reconexão devolveu uma voz nova sem os controles de agora: o \
             roster continua mostrando quem estava mudo como mudo, e o \
             microfone volta aberto"
    );
    assert_eq!(voz.abertura(), AberturaDoAudio::Aberto);
}

#[test]
fn quem_estava_em_texto_puro_continua_em_texto_puro() {
    let voz: VozDaSala<Voz> = VozDaSala::nova(true, DeviceChoice::default());
    let mut porta = PortaDaVoz::nova();
    porta.olhar(&voz, None, nada_abre_do_zero);
    let mut tentou = false;

    porta.na_volta(
        &voz,
        |_| {
            tentou = true;
            entregue(Ok(Ok(Voz {
                controles: vec![],
                ssrc: 2,
            })))
        },
        nada_abre_do_zero,
    );

    assert!(
        !tentou && voz.lock().expect("a voz").is_none(),
        "a reconexão abriu voz para quem não tinha nenhuma; sem placa de \
             som isso é abrir um microfone que não existe"
    );
    assert_eq!(voz.abertura(), AberturaDoAudio::ForaDeSala);
}

#[tokio::test]
async fn a_voz_que_nao_reabre_da_lugar_ao_texto() {
    let (voz, mut porta) = na_sala_com(Voz {
        controles: vec!["mudo"],
        ssrc: 1,
    });

    porta.na_volta(
        &voz,
        |_| entregue(Ok(Err("sem aparelho".to_owned()))),
        nada_abre_do_zero,
    );
    esperar(&mut porta, &voz).await;

    assert!(
        voz.lock().expect("a voz").is_none(),
        "a reconexão guardou a voz que não reabriu: ela sai por uma \
             conexão morta, com `ssrc` velho, e `audio_available` mente"
    );
    assert_eq!(voz.abertura(), AberturaDoAudio::SemAparelho);
}
