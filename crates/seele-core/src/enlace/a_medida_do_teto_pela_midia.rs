use super::{da_medida_do_teto, faixa_para_o_teto, para_a_medida_do_teto};
use crate::tela::{Teto, TetoDeVideo, PISO_DE_BANDA_BPS};
use seele_proto::signal::SignalBand;

/// **Pela mídia, o sinal ruim não corta o vídeo de novo.** A estimativa
/// do RTP já cedeu à rede; a faixa do sinal cortando por cima era a mesma
/// perda cobrada duas vezes, e a Critical parava o vídeo. No canal a
/// faixa continua mandando. Devolver `do_sinal` nos dois casos reprova
/// aqui.
#[test]
fn pela_midia_a_faixa_do_sinal_nao_corta_o_teto() {
    let teto = TetoDeVideo::com_caminho(para_a_medida_do_teto(3_000_000));
    let cheio = teto.teto(SignalBand::Nominal);
    for do_sinal in [
        SignalBand::Nominal,
        SignalBand::Degraded,
        SignalBand::Critical,
    ] {
        assert_eq!(
            teto.teto(faixa_para_o_teto(do_sinal, true)),
            cheio,
            "pela mídia, a faixa {do_sinal:?} cortou o teto"
        );
        assert_eq!(faixa_para_o_teto(do_sinal, false), do_sinal);
    }
    // E no canal o corte continua existindo.
    assert_ne!(
        teto.teto(faixa_para_o_teto(SignalBand::Degraded, false)),
        cheio
    );
    assert!(matches!(
        teto.teto(faixa_para_o_teto(SignalBand::Critical, false)),
        Teto::Parado(_)
    ));
}

/// **A estimativa do RTP nunca para o vídeo.** Por baixa que ela esteja, o
/// teto fica no piso: com o vídeo parado a estimativa não teria de onde
/// subir. Tirar o piso de `para_a_medida_do_teto` reprova aqui.
#[test]
fn a_estimativa_baixa_deixa_o_video_no_piso_e_nao_parado() {
    for estimativa in [0_u64, 10_000, 100_000, 200_000] {
        let teto =
            TetoDeVideo::com_caminho(para_a_medida_do_teto(estimativa)).teto(SignalBand::Nominal);
        assert!(
            matches!(teto, Teto::Bps(bps) if bps >= PISO_DE_BANDA_BPS),
            "com a estimativa em {estimativa} bps o teto foi {teto:?}"
        );
    }
}

/// O teto gasta 60% do caminho, e a conversão faz esses 60% darem 85% da
/// estimativa; a ida e a volta se desfazem.
#[test]
fn o_teto_gasta_oitenta_e_cinco_por_cento_da_estimativa() {
    let Teto::Bps(bps) =
            // A perna de quem hospeda larga, para sobrar só a de quem transmite.
            TetoDeVideo::com_caminho(para_a_medida_do_teto(2_000_000))
                .com_caminho_de_quem_hospeda(100_000_000)
                .teto(SignalBand::Nominal)
        else {
            panic!("parado com 2 Mbps");
        };
    assert!((1_690_000..=1_710_000).contains(&bps), "{bps}");
    let ida = da_medida_do_teto(3_000_000);
    assert!(para_a_medida_do_teto(u64::from(ida)).abs_diff(3_000_000) < 10);
}
