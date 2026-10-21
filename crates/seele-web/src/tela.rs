//! **A tela que o PC compartilha pelo navegador** (Tarefa 3.10 da F4): o que
//! vai no fio, e com que medidas.
//!
//! Quem captura e codifica é o navegador (o `getDisplayMedia` e o
//! `VideoEncoder`, em `compartilhar-web.js`); quem diz o que vai no fluxo é
//! este módulo: o byte do tipo e a abertura da tela, o cabeçalho de cada quadro
//! (`seele_proto::screen`), e as medidas e a cadência que os limites pedidos
//! dão, pelos degraus do desktop.
//!
//! **Os degraus são cópias** dos do `seele_video::codec` (as três resoluções,
//! as quatro cadências e o `medidas_para`), porque aquele crate é nativo — ele
//! carrega o módulo do Cisco — e não vai para o `wasm`. A conformidade
//! (`degraus_da_tela.rs`) as prende ao original.

use seele_proto::ids::ScreenId;
use seele_proto::screen::{
    cabecalho_de_quadro, ScreenCodec, ScreenHeader, ScreenSource, TipoDeQuadro, SCREEN_HEADER_LEN,
};
use seele_proto::stream::StreamType;
use seele_sessao::retrato::LimitesDeTela;
use serde_json::{json, Value};

/// As três resoluções do §5 do desenho da tela, da menor para a maior: a
/// `Resolucao::TODAS` do `seele_video`, em (largura, altura).
pub const DEGRAUS: [(u32, u32); 3] = [(960, 540), (1280, 720), (1920, 1080)];

/// As quatro cadências, da menor para a maior: a `Cadencia::TODAS`.
pub const CADENCIAS: [u32; 4] = [8, 15, 30, 60];

/// **O H.264 que o navegador codifica**, na forma que o `VideoEncoder` pede:
/// Baseline (`42`, `profile_idc` 66), com as restrições que o fazem decodificar
/// em qualquer leitor de Baseline (`E0`), no nível 4.0 (`28`). É o
/// `ScreenCodec::H264Baseline` que a abertura anuncia, medido no Chromium e no
/// Safari, e o OpenH264 do desktop o decodifica.
///
/// **Mora aqui, e não na página**: quem decide o perfil é quem codifica, e a
/// casca não escreve perfil nenhum à mão (o guarda
/// `the_decoder_profile_is_read_from_the_stream_and_never_spelled_out`). Quem
/// decodifica lê o perfil do SPS, como sempre.
pub const CODEC_DO_NAVEGADOR: &str = "avc1.42E028";

/// O teto de banda quando quem compartilha não escolheu um: o
/// `TETO_DA_PROVA_BPS` do `seele_video`, o que a medida do §5 usou.
pub const TETO_PADRAO_BPS: u32 = 1_200_000;

/// As medidas que um degrau ganha na proporção da fonte, com o mesmo orçamento
/// de pixels, os dois lados pares: o `medidas_para` do `seele_video`.
#[must_use]
pub fn medidas_para(degrau: (u32, u32), fonte_largura: u32, fonte_altura: u32) -> (u32, u32) {
    if fonte_largura == 0 || fonte_altura == 0 {
        return degrau;
    }
    let area = f64::from(degrau.0) * f64::from(degrau.1);
    let proporcao = f64::from(fonte_largura) / f64::from(fonte_altura);
    let altura = (area / proporcao).sqrt();
    let largura = altura * proporcao;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "medidas de tela, positivas e muito abaixo do teto do u32"
    )]
    let par = |valor: f64| ((valor as u32) / 2 * 2).max(2);
    (par(largura), par(altura))
}

/// O que o codificador do navegador recebe: as medidas, a cadência e o teto de
/// banda, pelos degraus do desktop (`limites_do_nucleo` da FFI): o maior degrau
/// que cabe na altura pedida, a maior cadência que cabe nos quadros pedidos, e
/// nunca acima do que a fonte tem.
#[must_use]
pub fn codificacao(limites: &LimitesDeTela, fonte_largura: u32, fonte_altura: u32) -> Value {
    let degrau = DEGRAUS
        .into_iter()
        .rev()
        .find(|(_, altura)| *altura <= limites.altura_maxima)
        .unwrap_or(DEGRAUS[0]);
    let quadros = CADENCIAS
        .into_iter()
        .rev()
        .find(|hz| *hz <= limites.quadros_maximos)
        .unwrap_or(CADENCIAS[0]);
    let (largura, altura) = medidas_para(degrau, fonte_largura, fonte_altura);
    // Nunca ampliar: uma janela pequena vai do tamanho dela.
    let (largura, altura) = if fonte_largura > 0 && fonte_altura > 0 && fonte_altura < altura {
        medidas_para(
            (fonte_largura / 2 * 2, fonte_altura / 2 * 2),
            fonte_largura,
            fonte_altura,
        )
    } else {
        (largura, altura)
    };
    json!({
        "codec": CODEC_DO_NAVEGADOR,
        "largura": largura,
        "altura": altura,
        "quadros": quadros,
        "banda_bps": limites.banda_bps.unwrap_or(TETO_PADRAO_BPS),
    })
}

/// O começo do fluxo da tela: o byte do tipo e a abertura (o H.264 Baseline
/// que o navegador codifica).
///
/// # Errors
///
/// A abertura que o `seele-proto` recusa (uma medida fora da faixa).
pub fn abertura(tela: u32, monitor: bool, largura: u16, altura: u16) -> Result<Vec<u8>, String> {
    let mut abertura = [0_u8; SCREEN_HEADER_LEN];
    ScreenHeader {
        version: 1,
        screen: ScreenId(tela),
        source: if monitor {
            ScreenSource::Monitor
        } else {
            ScreenSource::Window
        },
        codec: ScreenCodec::H264Baseline,
        width: largura,
        height: altura,
    }
    .encode(&mut abertura)
    .map_err(|erro| erro.to_string())?;
    let mut fluxo = vec![StreamType::Screen.byte()];
    fluxo.extend(abertura);
    Ok(fluxo)
}

/// O cabeçalho de um quadro: o tipo (0 comum, 1 chave, 2 som) e o tamanho.
/// `None` para um tipo que não existe.
#[must_use]
pub fn cabecalho(tipo: u32, tamanho: u32) -> Option<[u8; 5]> {
    let tipo = match tipo {
        0 => TipoDeQuadro::Comum,
        1 => TipoDeQuadro::Chave,
        2 => TipoDeQuadro::Som,
        _ => return None,
    };
    Some(cabecalho_de_quadro(tipo, tamanho))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

    use super::*;
    use crate::fluxos::{FluxoQueChega, Passo};
    use seele_sessao::retrato::Prioridade;

    fn limites(altura: u32, quadros: u32, banda: Option<u32>) -> LimitesDeTela {
        LimitesDeTela {
            banda_bps: banda,
            altura_maxima: altura,
            quadros_maximos: quadros,
            prioridade: Prioridade::default(),
            com_som: true,
        }
    }

    #[test]
    fn os_limites_caem_no_degrau_de_baixo_e_nunca_ampliam() {
        assert_eq!(
            codificacao(&limites(1000, 25, None), 1920, 1080),
            json!({ "codec": CODEC_DO_NAVEGADOR, "largura": 1280, "altura": 720, "quadros": 15, "banda_bps": TETO_PADRAO_BPS })
        );
        assert_eq!(
            codificacao(&limites(100, 1, Some(500_000)), 1920, 1080)["altura"],
            540
        );
        assert_eq!(
            codificacao(&limites(1080, 60, None), 800, 600)["altura"],
            600
        );
        // A tela do MacBook: o orçamento de 720p, sem tarja.
        assert_eq!(medidas_para((1280, 720), 3024, 1964), (1190, 772));
    }

    #[test]
    fn o_fluxo_que_sai_e_o_que_o_leitor_le() {
        let mut fluxo = abertura(7, true, 1280, 720).unwrap();
        for (tipo, corpo) in [(1_u32, &b"chave"[..]), (2, b"som"), (0, b"comum")] {
            fluxo.extend(cabecalho(tipo, u32::try_from(corpo.len()).unwrap()).unwrap());
            fluxo.extend(corpo);
        }
        let mut leitor = FluxoQueChega::novo();
        let passos = leitor.chegou(&fluxo);
        assert!(
            matches!(&passos[0], Passo::Evento(evento) if evento["ScreenOpened"]["screen"] == 7)
        );
        assert!(matches!(&passos[2], Passo::SomDaTela { tela: 7, .. }));
        assert_eq!(passos.len(), 4);
        assert_eq!(cabecalho(9, 1), None);
    }
}
