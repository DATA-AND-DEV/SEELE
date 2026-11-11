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

/// **A capa de um perfil oficial, para a janela.** A capa aparece cortada
/// (`background-size: cover`) numa faixa, e 1280 × 720 cobre a maior delas com
/// folga: uma capa 4K cabe nele com um nono dos pixels.
///
/// **O tamanho do arquivo também conta, e não só o dos pixels.** A imagem vai
/// à janela como `data:`, e a janela relê esse texto inteiro em cada elemento
/// que a mostra: medido em 10/11/2026, 37 ms por elemento com 10 MB, 3,7 ms
/// com 1 MB.
///
/// **Por que existe** (10/11/2026, relato do dono): uma capa em 4K deixou o
/// aplicativo inteiro lento. A imagem do perfil ia à janela do tamanho que
/// subiu, como um `data:` de megabytes, e a janela a decodificava em 33 MB por
/// lugar em que aparecia. As imagens de MOD já passavam por este arquivo; as
/// do perfil oficial, não.
pub const TETO_DA_CAPA_DO_PERFIL: Teto = Teto {
    largura: 1280,
    altura: 720,
    quadros: 60,
    milissegundos: 10_000,
};

/// **A foto de um perfil oficial, para a janela**: o maior lugar em que ela
/// aparece é o cartão do perfil, e 512 px cobrem uma tela de densidade dupla.
pub const TETO_DA_FOTO_DO_PERFIL: Teto = Teto {
    largura: 512,
    altura: 512,
    quadros: 60,
    milissegundos: 10_000,
};

/// **A imagem de um perfil oficial como ela vai à janela**: reduzida ao teto
/// do lado (`capa`, ou a foto). Uma que já cabe sai com os mesmos bytes.
///
/// # Errors
///
/// `formato-desconhecido`, quando os bytes não são uma imagem que este
/// produto decodifica.
pub fn imagem_do_perfil_para_a_janela(bytes: &[u8], capa: bool) -> Result<Vec<u8>, String> {
    let teto = if capa {
        TETO_DA_CAPA_DO_PERFIL
    } else {
        TETO_DA_FOTO_DO_PERFIL
    };
    reduzir(bytes, &teto).map(|reduzida| reduzida.bytes)
}

/// O teto de um uso, pelo nome que a janela manda. `None` para um nome que
/// este produto não conhece: nada é reduzido, e a imagem segue como veio.
pub fn teto_do_uso(uso: &str) -> Option<Teto> {
    match uso {
        "capa" => Some(TETO_DA_CAPA),
        // O avatar que um MOD põe numa pessoa: o mesmo lugar da foto do perfil
        // oficial, e o mesmo teto. Ia à janela do tamanho que o MOD mandou.
        "avatar" => Some(TETO_DA_FOTO_DO_PERFIL),
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
    // **As medidas, lidas do cabeçalho, antes de decodificar um pixel.** O
    // teto de bytes não limita o que a imagem ocupa aberta: um PNG de poucos
    // megabytes pode declarar 30 000 × 30 000, e decodificá-lo pediria 3,6 GB a
    // este processo. Quem manda a imagem é um par (o perfil de outra pessoa,
    // o MOD de um servidor).
    let (largura, altura) = image::ImageReader::with_format(Cursor::new(bytes), formato)
        .into_dimensions()
        .map_err(|_| desconhecido())?;
    if u64::from(largura) * u64::from(altura) > TETO_DE_PIXELS {
        return Err(GRANDE_DEMAIS.to_owned());
    }
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

/// O lado máximo, em pixels, de um ícone de pessoa: o do protocolo
/// (`MAX_SERVER_ICON_SIDE`).
const LADO_DO_ICONE: u32 = seele_proto::control::MAX_SERVER_ICON_SIDE;

/// O teto, em bytes, de um ícone de pessoa: o do protocolo
/// (`MAX_SERVER_ICON_LEN`, 8 KiB). Ele viaja num quadro de controle de 16 KiB e
/// quem hospeda o reenvia a cada pessoa conectada.
const TETO_DO_ICONE: usize = seele_proto::control::MAX_SERVER_ICON_LEN;

/// Os degraus do ícone, do que dói menos ao que dói mais: (lado máximo, bits
/// por canal de cor). Primeiro perde cor no tamanho cheio, e só quando isso não
/// basta é que começa a perder tamanho. O primeiro degrau é o tamanho cheio com
/// as cores todas, então um ícone que já cabia atravessa sem perder nada.
const DEGRAUS_DO_ICONE: [(u32, u8); 8] = [
    (LADO_DO_ICONE, 8),
    (LADO_DO_ICONE, 5),
    (LADO_DO_ICONE, 4),
    (192, 4),
    (160, 4),
    (128, 3),
    (96, 3),
    (64, 2),
];

/// **O ícone nativo de uma pessoa, feito da foto do perfil** (0.19, §9 Q1 da
/// especificação dos perfis): um PNG de até 256 px de lado e 8 KiB, do **primeiro
/// quadro** da imagem.
///
/// Existe para os clientes 0.18, que só conhecem o `SetPersonIcon`: ao salvar
/// uma foto nova, o cliente 0.19 manda também este ícone, e quem ainda está na
/// 0.18 vê o mesmo rosto. O desktop e o navegador chamam esta função, e o
/// `icone::encolher` do `seele-app` é uma chamada a ela — a escada mora num
/// lugar só.
///
/// PNG não tem botão de qualidade: o que o faz encolher é ter menos pixels e
/// menos cores. Um distintivo aguenta perder cor muito antes de aguentar perder
/// tamanho, e é nessa ordem que a escada desce ([`DEGRAUS_DO_ICONE`]). O último
/// degrau (64 px, quatro níveis por canal) dá uns 2 KiB no pior caso que se
/// consegue construir, então `None` é só «não é imagem».
///
/// Um GIF ou WebP animado entra pelo primeiro quadro (`load_from_memory`), e a
/// proporção é mantida: uma foto retangular espremida num quadrado fica com a
/// cara achatada.
///
/// # A paleta vem antes dos degraus (0.19.1)
///
/// Cortar bits por canal fazia de uma foto um cartaz: no ícone de quem
/// hospeda, medido, 128 × 127 em 30 cores, com franjas rosa e verdes num rosto
/// cinza — cada canal arredondava para um degrau diferente. Um PNG indexado,
/// com uma paleta tirada da própria imagem e o erro espalhado
/// (Floyd–Steinberg), guarda a mesma foto em 128 px e 256 cores com 6,9 KiB.
/// Depois do tamanho cheio com as cores todas (que já cabia atravessa sem
/// perder nada), a paleta desce de [`LADOS_DA_PALETA`] por [`CORES_DA_PALETA`];
/// os degraus de [`DEGRAUS_DO_ICONE`] ficam como último recurso, e são os
/// mesmos que o canvas do navegador ainda usa (`REGRAS_DO_ICONE`).
#[must_use]
pub fn icone_do_perfil(escolhida: &[u8]) -> Option<Vec<u8>> {
    let imagem = image::load_from_memory(escolhida).ok()?;
    let cheio = png_rgba(&reduzido(&imagem, LADO_DO_ICONE))?;
    if cheio.len() <= TETO_DO_ICONE {
        return Some(cheio);
    }
    // Cor antes de tamanho: um avatar aparece com 30 a 90 px, e 128 px em 256
    // cores é uma foto, enquanto 160 px em 16 cores é um cartaz. As paletas
    // ricas passam por todos os lados antes de as pobres serem tentadas.
    for paletas in [&CORES_DA_PALETA[..3], &CORES_DA_PALETA[3..]] {
        for lado in LADOS_DA_PALETA {
            let quadro = reduzido(&imagem, lado);
            for &cores in paletas {
                if let Some(saida) = png_em_paleta(&quadro, cores) {
                    if saida.len() <= TETO_DO_ICONE {
                        return Some(saida);
                    }
                }
            }
        }
    }
    for (lado, bits) in DEGRAUS_DO_ICONE.into_iter().skip(1) {
        let mut quadro = reduzido(&imagem, lado);
        if bits < 8 {
            achatar_cores(&mut quadro, bits);
        }
        let saida = png_rgba(&quadro)?;
        if saida.len() <= TETO_DO_ICONE {
            return Some(saida);
        }
    }
    None
}

/// Os lados que a paleta tenta, do maior ao menor.
const LADOS_DA_PALETA: [u32; 6] = [LADO_DO_ICONE, 224, 192, 160, 128, 96];

/// As paletas que cada lado tenta, da mais rica à mais pobre.
const CORES_DA_PALETA: [usize; 5] = [256, 128, 64, 32, 16];

/// A imagem com o lado maior em `lado`, se passar dele, e a proporção mantida.
fn reduzido(imagem: &image::DynamicImage, lado: u32) -> image::RgbaImage {
    if imagem.width() > lado || imagem.height() > lado {
        // `Lanczos3` e não `Nearest`: reduzir foto sem filtro é o que
        // produz aquele serrilhado que parece defeito da tela.
        imagem.resize(lado, lado, FilterType::Lanczos3).to_rgba8()
    } else {
        imagem.to_rgba8()
    }
}

/// Um PNG RGBA, com a compressão mais forte.
fn png_rgba(quadro: &image::RgbaImage) -> Option<Vec<u8>> {
    let mut saida = Vec::new();
    let escreveu = image::codecs::png::PngEncoder::new_with_quality(
        &mut saida,
        image::codecs::png::CompressionType::Best,
        image::codecs::png::FilterType::Adaptive,
    );
    image::ImageEncoder::write_image(
        escreveu,
        quadro.as_raw(),
        quadro.width(),
        quadro.height(),
        image::ExtendedColorType::Rgba8,
    )
    .ok()?;
    Some(saida)
}

/// Um PNG indexado em `cores` cores, com a paleta tirada da própria imagem
/// (NeuQuant) e o erro espalhado (Floyd–Steinberg). O alfa vai na paleta
/// (`tRNS`), então uma borda suave continua suave.
fn png_em_paleta(quadro: &image::RgbaImage, cores: usize) -> Option<Vec<u8>> {
    let mapa = color_quant::NeuQuant::new(10, cores, quadro.as_raw());
    let mut espalhado = quadro.clone();
    // O `dither` do `image` olha o vizinho da direita e o de baixo: numa
    // imagem de um pixel de largura ou de altura, ele sai do quadro.
    if espalhado.width() > 1 && espalhado.height() > 1 {
        image::imageops::dither(&mut espalhado, &mapa);
    }
    let indices = image::imageops::index_colors(&espalhado, &mapa);
    let rgba = mapa.color_map_rgba();
    let paleta: Vec<u8> = rgba
        .chunks_exact(4)
        .flat_map(|cor| cor.iter().take(3).copied())
        .collect();
    let alfa: Vec<u8> = rgba
        .chunks_exact(4)
        .filter_map(|cor| cor.get(3).copied())
        .collect();
    // Uma foto sem transparência não leva a tabela do alfa: a paleta do
    // NeuQuant arredonda o 255 de algumas cores para 254, e a tabela custava
    // uns 256 bytes a troco de nada.
    let opaca = quadro.pixels().all(|pixel| pixel.0[3] == u8::MAX);

    // Sem filtro costuma ser o melhor para um PNG indexado, e nem sempre é:
    // os dois são tentados, e fica o menor.
    let mut menor: Option<Vec<u8>> = None;
    for filtro in [png::FilterType::NoFilter, png::FilterType::Paeth] {
        let mut saida = Vec::new();
        {
            let mut gravador = png::Encoder::new(&mut saida, quadro.width(), quadro.height());
            gravador.set_color(png::ColorType::Indexed);
            gravador.set_depth(png::BitDepth::Eight);
            gravador.set_palette(paleta.clone());
            if !opaca {
                gravador.set_trns(alfa.clone());
            }
            gravador.set_compression(png::Compression::Best);
            gravador.set_filter(filtro);
            let mut escrita = gravador.write_header().ok()?;
            escrita.write_image_data(indices.as_raw()).ok()?;
        }
        if menor.as_ref().is_none_or(|atual| saida.len() < atual.len()) {
            menor = Some(saida);
        }
    }
    menor
}

/// Reduz cada canal a `bits` níveis, mantendo preto preto e branco branco: a
/// conta ingênua (apagar os bits de baixo) escurece a imagem inteira e nunca
/// chega ao branco. O alfa não é achatado: um distintivo com borda suave vira
/// um recorte de tesoura se for, e o alfa custa pouco no PNG.
fn achatar_cores(quadro: &mut image::RgbaImage, bits: u8) {
    let niveis = (1_u32 << bits).saturating_sub(1).max(1);
    #[allow(clippy::cast_precision_loss, reason = "no máximo 255")]
    let passo = 255.0 / niveis as f32;
    for pixel in quadro.pixels_mut() {
        for canal in 0..3 {
            let Some(valor) = pixel.0.get_mut(canal) else {
                continue;
            };
            let degrau = (f32::from(*valor) / passo).round();
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "limitado a 0..=255 na linha"
            )]
            {
                *valor = (degrau * passo).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// O nome com que uma imagem ilegível volta à janela.
/// O maior número de pixels (largura × altura) que este produto decodifica:
/// cabe uma imagem em 8K (33 megapixels), e a decodificação de uma só não
/// passa de 144 MB.
pub const TETO_DE_PIXELS: u64 = 36_000_000;

/// O nome da recusa de uma imagem acima de [`TETO_DE_PIXELS`]. É um nome que
/// a janela já conhece (o da mídia grande demais).
pub const GRANDE_DEMAIS: &str = "midia-grande-demais";

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
        // **Reduzido ao chegar, e não depois de juntar todos**: sessenta
        // quadros de uma animação em 4K, guardados inteiros, são 2 GB. Assim a
        // memória segura um quadro grande por vez.
        let (largura, altura) = quadro.buffer().dimensions();
        let quadro = if largura > teto.largura || altura > teto.altura {
            let (nova_largura, nova_altura) = caber(largura, altura, teto);
            let atraso = quadro.delay();
            let menor = image::imageops::resize(
                quadro.buffer(),
                nova_largura,
                nova_altura,
                FilterType::Triangle,
            );
            Frame::from_parts(menor, 0, 0, atraso)
        } else {
            quadro
        };
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

    /// Ruído: o pior caso do PNG, que não tem forma nem cor repetida.
    fn ruido(lado: u32) -> Vec<u8> {
        let mut quadro = RgbaImage::new(lado, lado);
        let mut semente = 0x2545_F491_4F6C_DD1D_u64;
        for pixel in quadro.pixels_mut() {
            semente ^= semente << 13;
            semente ^= semente >> 7;
            semente ^= semente << 17;
            let b = semente.to_le_bytes();
            pixel.0 = [b[0], b[1], b[2], 255];
        }
        let mut saida = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(quadro)
            .write_to(&mut saida, ImageFormat::Png)
            .expect("gravar o ruído");
        saida.into_inner()
    }

    /// **O ícone do perfil cabe no que o protocolo aceita, sempre**: PNG de até
    /// 256 px e 8 KiB, mesmo para o pior caso, e `check_server_icon` (o portão
    /// do servidor) o aceita.
    #[test]
    fn o_icone_do_perfil_cabe_no_que_o_protocolo_aceita() {
        let pronto = icone_do_perfil(&ruido(1024)).expect("o ruído tinha de caber");
        assert!(
            pronto.len() <= TETO_DO_ICONE,
            "saiu com {} bytes",
            pronto.len()
        );
        let (largura, altura) = dimensoes(&pronto);
        assert!(largura <= LADO_DO_ICONE && altura <= LADO_DO_ICONE);
        assert_eq!(
            seele_proto::control::check_server_icon(Some(&pronto)),
            Ok(()),
            "o servidor recusaria o ícone que o cliente gerou"
        );
    }

    /// Um GIF animado entra pelo **primeiro quadro**, e a proporção fica.
    #[test]
    fn o_icone_do_perfil_de_um_gif_e_o_primeiro_quadro() {
        // Dois quadros, 300 × 150: o primeiro verde e o segundo vermelho.
        let mut animado = Vec::new();
        {
            let mut gravador = GifEncoder::new(&mut animado);
            gravador.set_repeat(Repeat::Infinite).expect("repetir");
            for cor in [[0_u8, 200, 0, 255], [200, 0, 0, 255]] {
                let imagem = RgbaImage::from_pixel(300, 150, image::Rgba(cor));
                let quadro = Frame::from_parts(imagem, 0, 0, Delay::from_numer_denom_ms(100, 1));
                gravador.encode_frame(quadro).expect("gravar o quadro");
            }
        }
        let pronto = icone_do_perfil(&animado).expect("um GIF é imagem");
        let imagem = image::load_from_memory(&pronto).expect("é PNG").to_rgba8();
        assert_eq!(imagem.dimensions(), (256, 128), "a proporção mudou");
        let pixel = imagem.get_pixel(10, 10).0;
        assert!(
            pixel[0] < 40 && pixel[1] > 150,
            "não é o primeiro quadro (o verde): {pixel:?}"
        );
    }

    #[test]
    fn o_que_nao_e_imagem_nao_vira_icone() {
        assert!(icone_do_perfil(b"isto nao e uma imagem").is_none());
        assert!(icone_do_perfil(&[]).is_none());
    }

    /// Uma «foto» em tons de cinza com o ruído de cor de uma WebP com perda:
    /// formas suaves, textura fina e cada canal um pouco fora do outro.
    fn foto_cinza(lado: u32) -> Vec<u8> {
        let mut quadro = RgbaImage::new(lado, lado);
        let mut semente = 0x9E37_79B9_7F4A_7C15_u64;
        for (x, y, pixel) in quadro.enumerate_pixels_mut() {
            semente ^= semente << 13;
            semente ^= semente >> 7;
            semente ^= semente << 17;
            let b = semente.to_le_bytes();
            let (fx, fy) = (f64::from(x), f64::from(y));
            let forma = 128.0 + 70.0 * (fx / 37.0).sin() * (fy / 53.0).cos();
            let textura = f64::from(b[0] % 40) - 20.0;
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "limitado"
            )]
            let cinza = (forma + textura).clamp(0.0, 255.0) as i16;
            let canal = |desvio: u8| {
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "limitado"
                )]
                let valor = (cinza + i16::from(desvio % 7) - 3).clamp(0, 255) as u8;
                valor
            };
            pixel.0 = [canal(b[1]), canal(b[2]), canal(b[3]), 255];
        }
        let mut saida = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(quadro)
            .write_to(&mut saida, ImageFormat::Png)
            .expect("gravar a foto");
        saida.into_inner()
    }

    /// **O ícone de uma foto cinza continua cinza, e com tons** (0.19.1).
    ///
    /// A escada cortava bits por canal: num cinza com ruído de cor, cada canal
    /// arredondava para um degrau diferente, e o ícone saía com franjas rosa e
    /// verdes em 30 cores — medido no ícone de quem hospeda, 128 × 127. Com
    /// uma paleta, o mesmo teto de 8 KiB guarda o cinza como cinza.
    #[test]
    fn o_icone_de_uma_foto_cinza_nao_ganha_cor_e_guarda_os_tons() {
        let pronto = icone_do_perfil(&foto_cinza(873)).expect("a foto tinha de caber");
        assert!(
            pronto.len() <= TETO_DO_ICONE,
            "saiu com {} bytes",
            pronto.len()
        );
        assert_eq!(
            seele_proto::control::check_server_icon(Some(&pronto)),
            Ok(())
        );
        let imagem = image::load_from_memory(&pronto).expect("é PNG").to_rgba8();
        let mut tons = std::collections::BTreeSet::new();
        let mut pior_cor = 0_u8;
        for pixel in imagem.pixels() {
            let [r, g, b, _] = pixel.0;
            pior_cor = pior_cor.max(r.max(g).max(b) - r.min(g).min(b));
            tons.insert(g);
        }
        assert!(
            pior_cor <= 16,
            "o cinza ganhou cor: canais até {pior_cor} apart"
        );
        assert!(tons.len() >= 24, "só {} tons de cinza", tons.len());
        assert!(
            imagem.width() >= 96,
            "o ícone encolheu demais: {}px",
            imagem.width()
        );
    }

    /// Uma imagem de um pixel de largura não derruba a redução (o `dither`
    /// do `image` olha o vizinho da direita).
    #[test]
    fn uma_imagem_de_um_pixel_de_largura_vira_icone() {
        let mut saida = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            1,
            300,
            image::Rgba([10, 20, 30, 255]),
        ))
        .write_to(&mut saida, ImageFormat::Png)
        .expect("gravar");
        let pronto = icone_do_perfil(saida.get_ref()).expect("é imagem");
        assert!(pronto.len() <= TETO_DO_ICONE);
    }

    /// **Uma capa em 4K vai à janela com um quarto dos pixels, e uma foto
    /// grande vira 512 px** (o relato de 10/11/2026: a capa em 4K deixou o
    /// aplicativo inteiro lento). O que já cabe sai com os mesmos bytes.
    #[test]
    fn a_imagem_do_perfil_vai_a_janela_dentro_do_teto() {
        let medidas = |bytes: &[u8]| {
            let imagem = image::load_from_memory(bytes).expect("decodifica");
            (imagem.width(), imagem.height())
        };
        let capa_4k = png(3840, 2160);
        let reduzida = imagem_do_perfil_para_a_janela(&capa_4k, true).expect("reduz");
        assert_eq!(medidas(&reduzida), (1280, 720));

        // Uma faixa larga cabe pela largura, com a proporção mantida.
        let faixa = imagem_do_perfil_para_a_janela(&png(3840, 1280), true).expect("reduz");
        let (largura, altura) = medidas(&faixa);
        assert_eq!(largura, 1280);
        assert!((426..=427).contains(&altura), "{altura}");

        let foto = imagem_do_perfil_para_a_janela(&png(3000, 3000), false).expect("reduz");
        assert_eq!(medidas(&foto), (512, 512));

        // O que já cabe não é regravado.
        let pequena = png(1024, 576);
        assert_eq!(
            imagem_do_perfil_para_a_janela(&pequena, true).expect("passa"),
            pequena
        );
        // E o que não é imagem é dito, e não entregue.
        assert!(imagem_do_perfil_para_a_janela(b"nada disto", true).is_err());
    }

    /// Uma capa animada grande também é reduzida: é o caso mais caro (cada
    /// quadro é uma imagem inteira decodificada).
    #[test]
    fn a_capa_animada_do_perfil_tambem_cabe_no_teto() {
        let animada = gif(2560, 1440, 4, 100);
        let reduzida = imagem_do_perfil_para_a_janela(&animada, true).expect("reduz");
        let decodificador = GifDecoder::new(Cursor::new(&reduzida)).expect("continua um GIF");
        let quadros = decodificador
            .into_frames()
            .collect_frames()
            .expect("quadros");
        assert_eq!(quadros.len(), 4);
        let primeiro = quadros[0].buffer();
        assert_eq!((primeiro.width(), primeiro.height()), (1280, 720));
    }

    /// **Uma imagem que declara mais pixels que o teto é recusada pelo
    /// cabeçalho, sem ser decodificada.** O arquivo é pequeno (uma cor só
    /// comprime a quase nada) e aberto pediria gigabytes.
    #[test]
    fn a_imagem_com_pixels_demais_e_recusada_antes_de_decodificar() {
        // 9000 × 5000 = 45 megapixels, acima do teto; 8K (7680 × 4320) passa.
        let bomba = png(9000, 5000);
        assert!(bomba.len() < 1024 * 1024, "o arquivo de teste é pequeno");
        assert_eq!(reduzir(&bomba, &TETO_DA_CAPA).unwrap_err(), GRANDE_DEMAIS,);
        assert_eq!(
            imagem_do_perfil_para_a_janela(&bomba, true).unwrap_err(),
            GRANDE_DEMAIS,
        );
        assert!(reduzir(&png(7680, 4320), &TETO_DA_CAPA).is_ok());
    }
}
