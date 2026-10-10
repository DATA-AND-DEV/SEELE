//! **O teto do quadro de tela cabe o quadro cru da maior resolução do
//! codificador** (D-fio-9, 09/10/2026).
//!
//! O `MAX_QUADRO_LEN` mora no `seele-proto` e está congelado com a linha 1: uma
//! 1.x não pode mandar quadro maior para uma 1.0. As resoluções moram no
//! `seele-video`, que o `seele-proto` não alcança. Este crate alcança os dois,
//! e por isso o guarda mora aqui.
//!
//! O número é calculado das constantes do codificador, e não escrito à mão: no
//! dia em que aparecer uma quarta resolução maior, este teste reprova **antes**
//! de o teto passar a cortar quadros legítimos. O H.264 nunca precisa de mais
//! que o quadro I420 cru (o macrobloco PCM carrega o pixel cru), fora o
//! cabeçalho do quadro.

use seele_proto::screen::{CABECALHO_DE_QUADRO_LEN, MAX_QUADRO_LEN, MAX_SCREEN_PIXELS};
use seele_video::codec::{bytes_de_croma, bytes_de_luma, Resolucao};

#[test]
fn o_teto_do_quadro_cabe_o_quadro_cru_de_toda_resolucao_do_codificador() {
    for resolucao in Resolucao::TODAS {
        let (largura, altura) = (resolucao.largura(), resolucao.altura());
        let cru = bytes_de_luma(largura, altura) + 2 * bytes_de_croma(largura, altura);
        assert!(
            MAX_QUADRO_LEN >= cru + CABECALHO_DE_QUADRO_LEN,
            "o quadro cru de {largura}×{altura} tem {cru} bytes, e o teto do fio \
             (MAX_QUADRO_LEN, {MAX_QUADRO_LEN}) o cortaria: a linha 1 está congelada, \
             então a resolução nova não pode sair sem decidir o que fazer com isto"
        );
        // E a abertura do fluxo aceita a resolução: um codificador que mandasse
        // mais pixels que o fio declara seria recusado na abertura.
        assert!(
            u32::try_from(largura * altura).is_ok_and(|area| area <= MAX_SCREEN_PIXELS),
            "{largura}×{altura} passa da área que a abertura do fluxo aceita"
        );
    }
}
