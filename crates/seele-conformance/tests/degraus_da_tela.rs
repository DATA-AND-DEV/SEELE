//! **Os degraus da tela do navegador são os do desktop** (Tarefa 3.10 da F4).
//!
//! O `seele_video::codec` é nativo (carrega o módulo do Cisco) e não vai para o
//! `wasm`; o `seele_web::tela` tem cópias das três resoluções, das quatro
//! cadências, do teto de banda padrão e do `medidas_para`. Aqui as cópias são
//! comparadas com o original (pelo `seele-core`, que o reexporta), valor a
//! valor e numa grade de fontes.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use seele_core::{Cadencia, Resolucao};

#[test]
fn os_degraus_as_cadencias_e_o_teto_sao_os_do_desktop() {
    let do_desktop: Vec<(u32, u32)> = Resolucao::TODAS
        .iter()
        .map(|degrau| {
            (
                u32::try_from(degrau.largura()).unwrap(),
                u32::try_from(degrau.altura()).unwrap(),
            )
        })
        .collect();
    assert_eq!(seele_web::tela::DEGRAUS.to_vec(), do_desktop);
    let cadencias: Vec<u32> = Cadencia::TODAS.iter().map(|c| c.hz()).collect();
    assert_eq!(seele_web::tela::CADENCIAS.to_vec(), cadencias);
    assert_eq!(
        seele_web::tela::TETO_PADRAO_BPS,
        seele_core::TETO_DA_PROVA_BPS
    );
}

#[test]
fn as_medidas_na_proporcao_da_fonte_sao_as_do_desktop() {
    for degrau in Resolucao::TODAS {
        for (largura, altura) in [
            (0, 0),
            (1920, 1080),
            (3024, 1964),
            (2560, 1600),
            (1080, 1920),
            (800, 600),
            (5120, 1440),
            (1, 1),
        ] {
            let do_desktop = seele_core::medidas_para(degrau, largura, altura);
            let do_navegador = seele_web::tela::medidas_para(
                (
                    u32::try_from(degrau.largura()).unwrap(),
                    u32::try_from(degrau.altura()).unwrap(),
                ),
                u32::try_from(largura).unwrap(),
                u32::try_from(altura).unwrap(),
            );
            assert_eq!(
                (
                    usize::try_from(do_navegador.0).unwrap(),
                    usize::try_from(do_navegador.1).unwrap()
                ),
                do_desktop,
                "o degrau {degrau:?} numa fonte {largura}×{altura}"
            );
        }
    }
}
