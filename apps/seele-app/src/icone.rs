//! Encolher a imagem que a pessoa escolheu até ela caber no que o protocolo
//! aceita.
//!
//! # Por que encolher em vez de subir o teto
//!
//! O teto é `seele_proto::control::MAX_DOGMA_ICON_LEN`, 8 KiB, e ele não é
//! folgado por descuido: o ícone viaja num quadro de controle de 16 KiB e quem
//! hospeda o reenvia para cada pessoa conectada, umas cinquenta. Subir o teto
//! sobe as duas contas ao mesmo tempo, e a segunda é a de uma máquina que
//! alguém mantém na sala de casa.
//!
//! O que estava errado não era o número — era pedir a uma pessoa que arrumasse
//! um PNG de 8 KiB por conta própria. Nenhuma imagem que se escolhe no
//! computador tem esse tamanho, e a resposta «não coube» não dizia o que fazer.
//! Encolher aqui apaga a pergunta: escolhe-se qualquer foto, e o que sai é um
//! distintivo.
//!
//! # Por que uma escada e não uma qualidade
//!
//! PNG não tem botão de qualidade — é sem perda. O que faz um PNG encolher é
//! ter menos pixels e menos cores distintas, e as duas coisas doem de formas
//! diferentes: menos pixels borra a forma, menos cores chapa o degradê. Um
//! distintivo aguenta perder cor muito antes de aguentar perder tamanho, e é
//! nessa ordem que a escada desce.

/// O maior lado, em pixels, que o protocolo aceita.
///
/// Cópia de `seele_proto::control::MAX_DOGMA_ICON_SIDE`, pelo mesmo motivo que
/// `TETO_DO_ICONE` é cópia: esta casca não depende do `seele-proto`.
const LADO: u32 = 256;

/// O teto em bytes, cópia de `seele_proto::control::MAX_DOGMA_ICON_LEN`.
const TETO: usize = 8 * 1024;

/// Quanto ler do disco antes de desistir.
///
/// Não é o teto do ícone: é o teto do **arquivo de origem**, que é uma foto de
/// celular e não um distintivo. Doze megabytes cobrem qualquer câmera de
/// telefone com folga, e recusam o vídeo que alguém escolheu por engano antes
/// de ele virar doze gigabytes de memória.
pub(crate) const TETO_DA_ORIGEM: u64 = 12 * 1024 * 1024;

/// Os degraus, do que dói menos para o que dói mais.
///
/// Cada par é (lado máximo, bits por canal de cor). Ler de cima para baixo é
/// ler a ordem em que um distintivo se degrada bem: primeiro perde cor no
/// tamanho cheio, e só quando isso não basta é que começa a perder tamanho.
///
/// O primeiro degrau é o tamanho cheio com as cores todas, então uma imagem que
/// já era um distintivo atravessa esta função sem perder nada.
const DEGRAUS: [(u32, u8); 8] = [
    (LADO, 8),
    (LADO, 5),
    (LADO, 4),
    (192, 4),
    (160, 4),
    (128, 3),
    (96, 3),
    (64, 2),
];

/// A imagem escolhida, virada num ícone que cabe.
///
/// `None` quando não é imagem nenhuma, ou quando nem o último degrau coube —
/// que na prática não acontece: 64×64 com quatro níveis por canal dá uns 2 KiB
/// no pior caso que se consegue construir.
pub(crate) fn encolher(escolhida: &[u8]) -> Option<Vec<u8>> {
    let imagem = image::load_from_memory(escolhida).ok()?;

    for (lado, bits) in DEGRAUS {
        let mut quadro = if imagem.width() > lado || imagem.height() > lado {
            // `Lanczos3` e não `Nearest`: o que se reduz aqui é quase sempre uma
            // foto grande, e reduzir foto sem filtro é o que produz aquele
            // serrilhado que faz a imagem parecer defeito da tela.
            //
            // `resize` e não `resize_exact`: a proporção fica. Uma foto
            // retangular espremida num quadrado fica com a cara achatada, e
            // ninguém reconhece o próprio servidor assim.
            imagem
                .resize(lado, lado, image::imageops::FilterType::Lanczos3)
                .to_rgba8()
        } else {
            imagem.to_rgba8()
        };

        if bits < 8 {
            achatar_cores(&mut quadro, bits);
        }

        let mut saida = Vec::new();
        let escreveu = image::codecs::png::PngEncoder::new_with_quality(
            &mut saida,
            image::codecs::png::CompressionType::Best,
            image::codecs::png::FilterType::Adaptive,
        );
        if image::ImageEncoder::write_image(
            escreveu,
            quadro.as_raw(),
            quadro.width(),
            quadro.height(),
            image::ExtendedColorType::Rgba8,
        )
        .is_err()
        {
            return None;
        }

        if saida.len() <= TETO {
            return Some(saida);
        }
    }

    None
}

/// Reduz cada canal a `bits` níveis, mantendo preto preto e branco branco.
///
/// A conta ingênua — apagar os bits de baixo — escurece a imagem inteira e
/// nunca chega ao branco: 0xFF com três bits viraria 0xE0. Reescalar pela
/// razão dos extremos é o que preserva os dois, e num distintivo o branco do
/// fundo é justamente o que se nota faltando.
fn achatar_cores(quadro: &mut image::RgbaImage, bits: u8) {
    let niveis = (1_u32 << bits).saturating_sub(1).max(1);
    let passo = 255.0 / niveis as f32;
    for pixel in quadro.pixels_mut() {
        for canal in 0..3 {
            let Some(valor) = pixel.0.get_mut(canal) else {
                continue;
            };
            let degrau = (f32::from(*valor) / passo).round();
            *valor = (degrau * passo).round().clamp(0.0, 255.0) as u8;
        }
        // A transparência não é achatada: um distintivo com borda suave vira um
        // recorte de tesoura se ela for, e o alfa custa pouco no PNG porque
        // quase sempre é uma constante — ou tudo opaco, ou uma silhueta.
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Uma imagem que o teto recusaria: ruído, que é o pior caso do PNG.
    ///
    /// Ruído não tem forma nem cor repetida, então não há filtro nem dicionário
    /// que o comprima — é o que uma foto de verdade tem de mais parecido com o
    /// pior caso, e é por isso que o teste usa ele e não um quadrado colorido,
    /// que passaria no primeiro degrau e não provaria nada.
    fn ruido(lado: u32) -> Vec<u8> {
        let mut quadro = image::RgbaImage::new(lado, lado);
        let mut semente = 0x2545_F491_4F6C_DD1D_u64;
        for pixel in quadro.pixels_mut() {
            // Xorshift: os testes não podem sortear de verdade e não precisam —
            // o que se quer é sempre o mesmo ruído, para o teste medir a mesma
            // coisa em toda máquina que o rodar.
            semente ^= semente << 13;
            semente ^= semente >> 7;
            semente ^= semente << 17;
            let bytes = semente.to_le_bytes();
            pixel.0 = [bytes[0], bytes[1], bytes[2], 255];
        }
        let mut saida = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(quadro)
            .write_to(&mut saida, image::ImageFormat::Png)
            .expect("escrever o ruído");
        saida.into_inner()
    }

    #[test]
    fn a_pior_imagem_possivel_ainda_cabe() {
        // **A propriedade que este arquivo existe para dar.** Se um caso
        // qualquer pode não caber, a pessoa volta a ver «não cabe» sem saber o
        // que fazer — que é exatamente o defeito que se estava consertando.
        let pronto = encolher(&ruido(1024)).expect("o ruído tinha de caber em algum degrau");
        assert!(
            pronto.len() <= TETO,
            "saiu com {} bytes, e o teto é {TETO}",
            pronto.len()
        );
    }

    #[test]
    fn o_lado_nunca_passa_do_que_o_protocolo_aceita() {
        // O outro teto, o que é fácil esquecer: o protocolo recusa pelo lado
        // **declarado** no cabeçalho do PNG, e não pelos bytes. Um degrau que
        // coubesse em bytes e passasse de 256 px seria recusado do mesmo jeito,
        // do outro lado, com uma frase sobre tamanho que não bate com o número
        // que esta casca mostrou.
        let pronto = encolher(&ruido(2048)).expect("tinha de caber");
        let imagem = image::load_from_memory(&pronto).expect("o que sai daqui é PNG");
        assert!(
            imagem.width() <= LADO && imagem.height() <= LADO,
            "saiu {}x{}, e o lado máximo é {LADO}",
            imagem.width(),
            imagem.height()
        );
    }

    #[test]
    fn uma_imagem_que_ja_cabia_atravessa_sem_perder_nada() {
        // O primeiro degrau é o tamanho cheio com as cores todas, e isso é uma
        // promessa: quem já preparou um distintivo de 32×32 não vai encontrá-lo
        // reamostrado nem achatado do outro lado.
        let mut quadro = image::RgbaImage::new(32, 32);
        for (x, y, pixel) in quadro.enumerate_pixels_mut() {
            pixel.0 = [(x * 8) as u8, (y * 8) as u8, 0x1F, 255];
        }
        let mut original = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(quadro.clone())
            .write_to(&mut original, image::ImageFormat::Png)
            .expect("escrever");

        let pronto = encolher(original.get_ref()).expect("tinha de caber");
        let volta = image::load_from_memory(&pronto).expect("é PNG").to_rgba8();
        assert_eq!(volta.dimensions(), (32, 32));
        assert_eq!(volta.as_raw(), quadro.as_raw(), "os pixels mudaram à toa");
    }

    #[test]
    fn o_que_nao_e_imagem_nao_vira_icone() {
        assert!(encolher(b"isto nao e uma imagem").is_none());
        assert!(encolher(&[]).is_none());
    }
}
