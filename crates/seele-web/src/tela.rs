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
    let escala = (f64::from(seele_proto::screen::MAX_SCREEN_SIDE) / largura.max(altura)).min(1.0);
    let (largura, altura) = (largura * escala, altura * escala);
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

/// O nome com que a fonte de uma transmissão atravessa para a casca, no
/// `source` do `ScreenOpened`: o mesmo texto do `seele_ffi::ScreenKind`, que a
/// conformidade (`dois_produtores_da_tela.rs`) prende ao do desktop.
#[must_use]
pub const fn nome_da_fonte(fonte: ScreenSource) -> &'static str {
    match fonte {
        ScreenSource::Monitor => "monitor",
        ScreenSource::Window => "window",
        ScreenSource::Camera => "camera",
    }
}

/// A fonte que a página declara, pelo nome de [`nome_da_fonte`]. `None` para
/// um nome que não existe: a abertura não sai com uma fonte adivinhada.
#[must_use]
pub fn fonte_de(nome: &str) -> Option<ScreenSource> {
    [
        ScreenSource::Monitor,
        ScreenSource::Window,
        ScreenSource::Camera,
    ]
    .into_iter()
    .find(|fonte| nome_da_fonte(*fonte) == nome)
}

/// **O teto da câmera**: 720p a 30 quadros. Uma câmera é um rosto, e não um
/// texto a ler; acima disso o custo é de banda, e quem paga é a voz da sala.
pub const TETO_DA_CAMERA: (u32, u32) = (720, 30);

/// Os limites de uma câmera: os pedidos, nunca acima do [`TETO_DA_CAMERA`], e
/// sem som — a voz de quem fala já vai pela voz.
#[must_use]
pub fn limites_da_camera(limites: &LimitesDeTela) -> LimitesDeTela {
    LimitesDeTela {
        altura_maxima: limites.altura_maxima.min(TETO_DA_CAMERA.0),
        quadros_maximos: limites.quadros_maximos.min(TETO_DA_CAMERA.1),
        prioridade: seele_sessao::retrato::Prioridade::Movimento,
        com_som: false,
        banda_bps: limites.banda_bps,
    }
}

/// **O portão da câmera**, feito à mão: a fonte 2 nasceu no protocolo 13, e
/// as tabelas `desde` do `seele-proto` cobrem mensagens de controle, e não um
/// byte de cabeçalho de fluxo. Um servidor do 12 anunciaria a transmissão à
/// sala e recusaria a abertura dela; aqui o pedido não chega a sair, e o erro
/// é o mesmo `ServidorAntigo` dos verbos do 13.
///
/// # Errors
///
/// [`seele_sessao::perfil::ErroDoPerfil::ServidorAntigo`], com os dois números.
pub fn portao_da_fonte(
    fonte: ScreenSource,
    negociado: u8,
) -> Result<(), seele_sessao::perfil::ErroDoPerfil> {
    if negociado < fonte.desde() {
        return Err(seele_sessao::perfil::ErroDoPerfil::ServidorAntigo {
            negociado,
            exige: fonte.desde(),
        });
    }
    Ok(())
}

/// O começo do fluxo da tela: o byte do tipo e a abertura (o H.264 Baseline
/// que o navegador codifica).
///
/// # Errors
///
/// A abertura que o `seele-proto` recusa (uma medida fora da faixa).
pub fn abertura(
    tela: u32,
    fonte: ScreenSource,
    largura: u16,
    altura: u16,
) -> Result<Vec<u8>, String> {
    let mut abertura = [0_u8; SCREEN_HEADER_LEN];
    ScreenHeader {
        version: 1,
        screen: ScreenId(tela),
        source: fonte,
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
        let mut fluxo = abertura(7, ScreenSource::Monitor, 1280, 720).unwrap();
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

    /// **A câmera sai com a fonte 2 e é lida como câmera**, e o portão a
    /// recusa para um servidor do 12 — a tela e a janela, que ele conhece,
    /// continuam passando.
    #[test]
    fn a_camera_abre_com_a_fonte_dela_e_o_portao_a_recusa_no_doze() {
        let fluxo = abertura(7, ScreenSource::Camera, 1280, 720).unwrap();
        assert_eq!(fluxo[1 + 5], 2, "a câmera não saiu como a fonte 2");
        let passos = FluxoQueChega::novo().chegou(&fluxo);
        assert!(
            matches!(&passos[0], Passo::Evento(evento) if evento["ScreenOpened"]["source"] == "camera")
        );

        assert_eq!(
            portao_da_fonte(ScreenSource::Camera, 12),
            Err(seele_sessao::perfil::ErroDoPerfil::ServidorAntigo {
                negociado: 12,
                exige: 13
            })
        );
        assert_eq!(portao_da_fonte(ScreenSource::Camera, 13), Ok(()));
        assert_eq!(portao_da_fonte(ScreenSource::Window, 12), Ok(()));
        assert_eq!(portao_da_fonte(ScreenSource::Monitor, 10), Ok(()));
    }

    /// Os nomes vão e voltam, e um nome que não existe não vira fonte nenhuma.
    #[test]
    fn a_fonte_atravessa_pelo_nome_e_um_nome_estranho_nao_e_adivinhado() {
        for fonte in [
            ScreenSource::Monitor,
            ScreenSource::Window,
            ScreenSource::Camera,
        ] {
            assert_eq!(fonte_de(nome_da_fonte(fonte)), Some(fonte));
        }
        assert_eq!(fonte_de("tela"), None);
    }

    /// **A câmera nunca passa de 720p a 30, e vai sem som**, peça a casca o
    /// que pedir.
    #[test]
    fn os_limites_da_camera_tem_teto_e_nao_levam_som() {
        let pedidos = limites(1080, 60, Some(900_000));
        let da_camera = limites_da_camera(&pedidos);
        assert_eq!(
            (da_camera.altura_maxima, da_camera.quadros_maximos),
            TETO_DA_CAMERA
        );
        assert!(!da_camera.com_som);
        assert_eq!(da_camera.banda_bps, Some(900_000));
        assert_eq!(
            limites_da_camera(&limites(540, 15, None)).altura_maxima,
            540
        );
        assert_eq!(
            codificacao(&da_camera, 1280, 720)["quadros"],
            30,
            "a cadência da câmera não saiu pelos degraus"
        );
    }
}
