//! **A prévia de uma fonte** (a miniatura da caixa de compartilhar): um quadro
//! da captura reduzido a uma imagem pequena em RGB.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "num teste, o pânico é o relatório"
)]

use seele_core::{previa_do_quadro, LADO_DA_PREVIA};
use seele_video::codec::QuadroI420;

/// Um quadro liso, de uma cor só (em YCbCr).
fn liso(largura: usize, altura: usize, y: u8, cb: u8, cr: u8) -> QuadroI420 {
    let croma = largura.div_ceil(2) * altura.div_ceil(2);
    QuadroI420::novo(
        largura,
        altura,
        vec![y; largura * altura],
        vec![cb; croma],
        vec![cr; croma],
    )
    .expect("planos certos")
}

/// A prévia cabe no lado pedido, com a proporção da fonte, e tem três bytes
/// por ponto.
#[test]
fn a_previa_cabe_no_lado_e_guarda_a_proporcao() {
    let previa = previa_do_quadro(&liso(960, 540, 128, 128, 128), LADO_DA_PREVIA).unwrap();
    assert_eq!((previa.largura, previa.altura), (320, 180));
    assert_eq!(previa.rgb.len(), 320 * 180 * 3);

    // Uma janela em pé cabe pela altura.
    let em_pe = previa_do_quadro(&liso(300, 900, 128, 128, 128), LADO_DA_PREVIA).unwrap();
    assert_eq!((em_pe.largura, em_pe.altura), (100, 300));

    // O que já é pequeno não cresce, e medidas ímpares não estouram os planos.
    let pequena = previa_do_quadro(&liso(101, 51, 128, 128, 128), LADO_DA_PREVIA).unwrap();
    assert_eq!((pequena.largura, pequena.altura), (101, 51));
}

/// **A cor é a do vídeo** (BT.709, faixa limitada): o preto e o branco do
/// vídeo saem preto e branco de verdade, e o vermelho sai vermelho — uma
/// prévia esverdeada ou lavada é a conversão errada.
#[test]
fn a_cor_da_previa_e_a_do_video() {
    let ponto = |y, cb, cr| {
        let previa = previa_do_quadro(&liso(64, 64, y, cb, cr), 64).unwrap();
        (previa.rgb[0], previa.rgb[1], previa.rgb[2])
    };
    assert_eq!(ponto(16, 128, 128), (0, 0, 0), "o preto do vídeo");
    let (r, g, b) = ponto(235, 128, 128);
    assert!(
        r >= 253 && g >= 253 && b >= 253,
        "o branco do vídeo: {r} {g} {b}"
    );
    // O vermelho puro em BT.709 de faixa limitada: Y 63, Cb 102, Cr 240.
    let (r, g, b) = ponto(63, 102, 240);
    assert!(r > 240 && g < 15 && b < 15, "o vermelho: {r} {g} {b}");
}

/// Um quadro sem lado, ou um lado pedido de zero, não tem prévia.
#[test]
fn sem_lado_nao_ha_previa() {
    assert!(previa_do_quadro(&liso(64, 64, 128, 128, 128), 0).is_none());
}
