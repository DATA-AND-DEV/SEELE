//! **A redução de uma imagem de MOD ao tamanho em que ela aparece** (rascunho
//! da API 6, §4 e §12; tarefa 6 da F3, Step 5).
//!
//! # Por que reduzir aqui, e não deixar a janela mostrar o que veio
//!
//! O custo de uma imagem na janela é o **quadro decodificado**, e não o
//! arquivo: uma capa de 960 × 320 tem 1,23 MB por quadro, e um GIF de 60
//! quadros, 74 MB (a conta do PERFIS). Um MOD que mande uma foto de câmera como
//! capa faria a janela decodificar dezenas de megapixels para mostrar uma faixa
//! de 320 px de altura. Reduzir no Rust, antes de a imagem atravessar a ponte,
//! poupa a ponte, a memória da janela e a decodificação dela.
//!
//! # O uso é um nome, e não um tamanho
//!
//! A janela diz **para que** a imagem serve (`"capa"`), e o teto daquele uso
//! mora aqui. Nem a janela nem o MOD escolhem números: um uso desconhecido não
//! reduz nada, e o teto de cada um é deste arquivo.
//!
//! # Os números são de partida
//!
//! O lado, os quadros e a duração são a proposta do rascunho (§4), e a medida
//! que os fixa é a M3 (tarefa 11 do plano da F3). Até lá eles ficam aqui, num
//! lugar só.
//!
//! **Mora no `seele-sessao`** desde a Tarefa 2.6 da F4, atrás da feature
//! `reducao` (que traz o `image`): o navegador reduz a capa pelos mesmos
//! números e pelo mesmo código, e o desktop a lê pelo `seele-ffi`.

use std::io::Cursor;

use image::codecs::gif::{GifDecoder, GifEncoder, Repeat};
use image::codecs::webp::WebPDecoder;
use image::imageops::FilterType;
use image::{AnimationDecoder, Frame, ImageFormat};

/// O que uma imagem pode ser no máximo, para um uso.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Teto {
    /// A largura do maior lugar em que ela aparece, em pixels.
    pub largura: u32,
    /// A altura do maior lugar em que ela aparece, em pixels.
    pub altura: u32,
    /// Quantos quadros uma imagem animada mantém.
    pub quadros: usize,
    /// Quanto tempo uma imagem animada dura, somados os quadros mantidos.
    pub milissegundos: u32,
}

/// **A capa de uma pessoa** (`pessoa.capa`): o maior lugar em que ela aparece é
/// o cartão da chamada, e 960 × 320 é a conta do PERFIS; 60 quadros e 10 s, a
/// proposta do rascunho §4. A M3 mede.
pub const TETO_DA_CAPA: Teto = Teto {
    largura: 960,
    altura: 320,
    quadros: 60,
    milissegundos: 10_000,
};

/// O teto de um uso, pelo nome que a janela manda. `None` para um nome que
/// este produto não conhece: nada é reduzido, e a imagem segue como veio.
pub fn teto_do_uso(uso: &str) -> Option<Teto> {
    match uso {
        "capa" => Some(TETO_DA_CAPA),
        _ => None,
    }
}

/// Uma imagem reduzida, e quanto ela pesa decodificada.
#[derive(Debug)]
pub struct Reduzida {
    /// Os bytes que atravessam a ponte. Os de entrada, intactos, quando a
    /// imagem já cabia no teto.
    pub bytes: Vec<u8>,
    /// Largura × altura × 4 × quadros: o que a janela vai decodificar. É o que
    /// o bolso da capa conta (rascunho §4), e não o tamanho do arquivo.
    pub decodificados: u64,
}

/// **Reduz `bytes` ao teto**: as dimensões, cabendo no retângulo do teto com a
/// proporção mantida; numa imagem animada, também os quadros e a duração.
///
/// Uma imagem que já cabe sai com os mesmos bytes. Uma animação reduzida sai em
/// GIF — o único formato animado que este crate grava.
///
/// # Errors
///
/// `formato-desconhecido`, quando os bytes não são uma imagem que este produto
/// decodifica; é o nome que a janela já conhece (rascunho §7.1).
pub fn reduzir(bytes: &[u8], teto: &Teto) -> Result<Reduzida, String> {
    let formato = image::guess_format(bytes).map_err(|_| desconhecido())?;
    if let Some(quadros) = quadros_dentro_do_teto(bytes, formato, teto)? {
        return reduzir_animacao(quadros, teto);
    }
    let imagem = image::load_from_memory_with_format(bytes, formato).map_err(|_| desconhecido())?;
    let (largura, altura) = (imagem.width(), imagem.height());
    if largura <= teto.largura && altura <= teto.altura {
        return Ok(Reduzida {
            bytes: bytes.to_vec(),
            decodificados: decodificados(largura, altura, 1),
        });
    }
    let (nova_largura, nova_altura) = caber(largura, altura, teto);
    let reduzida = imagem.resize_exact(nova_largura, nova_altura, FilterType::Triangle);
    let mut saida = Vec::new();
    // **JPEG fica JPEG**: uma foto regravada em PNG cresce, e o que se queria
    // era o contrário. O resto vira PNG, sem perda.
    let gravou = if formato == ImageFormat::Jpeg {
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut saida, 85)
            .encode_image(&reduzida.to_rgb8())
    } else {
        reduzida.write_to(&mut Cursor::new(&mut saida), ImageFormat::Png)
    };
    gravou.map_err(|_| desconhecido())?;
    Ok(Reduzida {
        bytes: saida,
        decodificados: decodificados(nova_largura, nova_altura, 1),
    })
}

/// O nome com que uma imagem ilegível volta à janela.
fn desconhecido() -> String {
    "formato-desconhecido".to_owned()
}

/// Largura × altura × 4 × quadros, sem estourar.
fn decodificados(largura: u32, altura: u32, quadros: usize) -> u64 {
    u64::from(largura)
        .saturating_mul(u64::from(altura))
        .saturating_mul(4)
        .saturating_mul(u64::try_from(quadros).unwrap_or(u64::MAX))
}

/// O maior tamanho com a proporção de `largura × altura` que cabe no teto.
fn caber(largura: u32, altura: u32, teto: &Teto) -> (u32, u32) {
    let escala = (f64::from(teto.largura) / f64::from(largura.max(1)))
        .min(f64::from(teto.altura) / f64::from(altura.max(1)));
    let lado = |medida: u32, maximo: u32| {
        // O arredondamento de um `f64` positivo e limitado ao teto cabe num `u32`.
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "limitado logo abaixo"
        )]
        let medido = (f64::from(medida) * escala).round().max(1.0) as u32;
        medido.min(maximo)
    };
    (lado(largura, teto.largura), lado(altura, teto.altura))
}

/// **Os quadros de uma animação, lidos só até o teto** de quadros e de duração
/// — o resto do arquivo nem é decodificado. `None` para o que não é animação
/// (ou é uma de um quadro só), que segue o caminho da imagem parada.
fn quadros_dentro_do_teto(
    bytes: &[u8],
    formato: ImageFormat,
    teto: &Teto,
) -> Result<Option<Vec<Frame>>, String> {
    let quadros = match formato {
        ImageFormat::Gif => GifDecoder::new(Cursor::new(bytes))
            .map_err(|_| desconhecido())?
            .into_frames(),
        ImageFormat::WebP => {
            let decodificador = WebPDecoder::new(Cursor::new(bytes)).map_err(|_| desconhecido())?;
            if !decodificador.has_animation() {
                return Ok(None);
            }
            decodificador.into_frames()
        }
        _ => return Ok(None),
    };
    let mut mantidos = Vec::new();
    let mut duracao: u32 = 0;
    for quadro in quadros {
        let quadro = quadro.map_err(|_| desconhecido())?;
        let (numer, denom) = quadro.delay().numer_denom_ms();
        let deste = numer / denom.max(1);
        // O primeiro fica sempre: uma animação de um quadro longo é uma imagem.
        if !mantidos.is_empty()
            && (mantidos.len() >= teto.quadros
                || duracao.saturating_add(deste) > teto.milissegundos)
        {
            break;
        }
        duracao = duracao.saturating_add(deste);
        mantidos.push(quadro);
    }
    Ok((mantidos.len() > 1).then_some(mantidos))
}

/// Grava os quadros mantidos em GIF, reduzidos quadro a quadro quando a
/// animação é maior que o lugar.
fn reduzir_animacao(quadros: Vec<Frame>, teto: &Teto) -> Result<Reduzida, String> {
    let quantos = quadros.len();
    let (largura, altura) = quadros
        .first()
        .map(|quadro| quadro.buffer().dimensions())
        .ok_or_else(desconhecido)?;
    let (nova_largura, nova_altura) = if largura <= teto.largura && altura <= teto.altura {
        (largura, altura)
    } else {
        caber(largura, altura, teto)
    };
    let mut saida = Vec::new();
    {
        // A velocidade 10 da quantização é o meio da escala do crate: a paleta
        // de uma capa não precisa da melhor, e 60 quadros na mais lenta somam
        // segundos.
        let mut gravador = GifEncoder::new_with_speed(&mut saida, 10);
        gravador
            .set_repeat(Repeat::Infinite)
            .map_err(|_| desconhecido())?;
        for quadro in quadros {
            let atraso = quadro.delay();
            let mut imagem = quadro.into_buffer();
            if (nova_largura, nova_altura) != (largura, altura) {
                imagem = image::imageops::resize(
                    &imagem,
                    nova_largura,
                    nova_altura,
                    FilterType::Triangle,
                );
            }
            gravador
                .encode_frame(Frame::from_parts(imagem, 0, 0, atraso))
                .map_err(|_| desconhecido())?;
        }
    }
    Ok(Reduzida {
        bytes: saida,
        decodificados: decodificados(nova_largura, nova_altura, quantos),
    })
}

#[cfg(test)]
mod testes {
    #![allow(
        clippy::expect_used,
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "num teste, o pânico é o relatório"
    )]

    use super::*;
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Frame, ImageEncoder, RgbaImage};

    fn png(largura: u32, altura: u32) -> Vec<u8> {
        let imagem = RgbaImage::from_pixel(largura, altura, image::Rgba([40, 200, 120, 255]));
        let mut saida = Vec::new();
        image::codecs::png::PngEncoder::new(&mut saida)
            .write_image(&imagem, largura, altura, image::ExtendedColorType::Rgba8)
            .expect("gravar o PNG de teste");
        saida
    }

    fn gif(largura: u32, altura: u32, quadros: usize, milissegundos: u32) -> Vec<u8> {
        let mut saida = Vec::new();
        {
            let mut gravador = GifEncoder::new(&mut saida);
            gravador.set_repeat(Repeat::Infinite).expect("repetir");
            for i in 0..quadros {
                let cor = u8::try_from(i % 250).expect("cabe");
                let imagem = RgbaImage::from_pixel(largura, altura, image::Rgba([cor, 0, 0, 255]));
                let quadro =
                    Frame::from_parts(imagem, 0, 0, Delay::from_numer_denom_ms(milissegundos, 1));
                gravador.encode_frame(quadro).expect("gravar o quadro");
            }
        }
        saida
    }

    fn dimensoes(bytes: &[u8]) -> (u32, u32) {
        let imagem = image::load_from_memory(bytes).expect("decodificar a reduzida");
        (imagem.width(), imagem.height())
    }

    fn quadros_e_duracao(bytes: &[u8]) -> (usize, u32) {
        let quadros = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
            .expect("um GIF")
            .into_frames()
            .collect_frames()
            .expect("os quadros");
        let duracao = quadros
            .iter()
            .map(|quadro| {
                let (numer, denom) = quadro.delay().numer_denom_ms();
                numer / denom.max(1)
            })
            .sum();
        (quadros.len(), duracao)
    }

    /// Uma foto maior que o lugar cabe no retângulo, com a proporção mantida.
    #[test]
    fn uma_imagem_maior_que_o_lugar_cabe_nele_com_a_proporcao() {
        let reduzida = reduzir(&png(2000, 700), &TETO_DA_CAPA).expect("reduzir");
        let (largura, altura) = dimensoes(&reduzida.bytes);
        assert!(largura <= 960 && altura <= 320, "saiu {largura} × {altura}");
        assert!(
            altura == 320 || largura == 960,
            "não usou o lado que limita: {largura} × {altura}"
        );
        let proporcao = f64::from(largura) / f64::from(altura);
        assert!(
            (proporcao - 2000.0 / 700.0).abs() < 0.02,
            "a proporção mudou: {proporcao}"
        );
        assert_eq!(
            reduzida.decodificados,
            u64::from(largura) * u64::from(altura) * 4
        );
    }

    /// A que já cabe sai com os mesmos bytes.
    #[test]
    fn uma_imagem_que_ja_cabe_sai_intacta() {
        let original = png(300, 100);
        let reduzida = reduzir(&original, &TETO_DA_CAPA).expect("reduzir");
        assert_eq!(reduzida.bytes, original);
        assert_eq!(reduzida.decodificados, 300 * 100 * 4);
    }

    /// Uma animação longa perde os quadros que passam do teto de quadros.
    #[test]
    fn uma_animacao_com_quadros_demais_fica_no_teto_de_quadros() {
        let reduzida = reduzir(&gif(40, 20, 120, 50), &TETO_DA_CAPA).expect("reduzir");
        let (quadros, duracao) = quadros_e_duracao(&reduzida.bytes);
        assert_eq!(quadros, 60, "saiu com {quadros} quadros");
        assert!(duracao <= 10_000, "dura {duracao} ms");
        assert_eq!(reduzida.decodificados, 40 * 20 * 4 * 60);
    }

    /// E uma animação lenta perde os quadros que passam do teto de duração.
    #[test]
    fn uma_animacao_lenta_demais_fica_no_teto_de_duracao() {
        let reduzida = reduzir(&gif(40, 20, 40, 500), &TETO_DA_CAPA).expect("reduzir");
        let (quadros, duracao) = quadros_e_duracao(&reduzida.bytes);
        assert_eq!(quadros, 20, "saiu com {quadros} quadros e {duracao} ms");
        assert!(duracao <= 10_000, "dura {duracao} ms");
    }

    /// Uma animação maior que o lugar é reduzida quadro a quadro.
    #[test]
    fn uma_animacao_maior_que_o_lugar_e_reduzida_em_cada_quadro() {
        let reduzida = reduzir(&gif(1920, 640, 3, 100), &TETO_DA_CAPA).expect("reduzir");
        let (largura, altura) = dimensoes(&reduzida.bytes);
        assert!(largura <= 960 && altura <= 320, "saiu {largura} × {altura}");
        assert_eq!(quadros_e_duracao(&reduzida.bytes).0, 3);
    }

    /// Bytes que não são imagem são recusados pelo nome que a janela conhece.
    #[test]
    fn bytes_que_nao_sao_imagem_sao_formato_desconhecido() {
        assert_eq!(
            reduzir(b"nada disto", &TETO_DA_CAPA).unwrap_err(),
            "formato-desconhecido"
        );
    }

    /// O uso é um nome de lista fechada: um nome desconhecido não reduz.
    #[test]
    fn so_um_uso_conhecido_tem_teto() {
        assert_eq!(teto_do_uso("capa"), Some(TETO_DA_CAPA));
        assert_eq!(teto_do_uso("qualquer coisa"), None);
    }
}
