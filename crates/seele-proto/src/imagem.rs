//! Que imagem os **bytes** dizem ser — a tabela, e só ela.
//!
//! # Por que ela mora aqui, e não onde nasceu
//!
//! Nasceu em `seele_core::preview`, que é a casa da prévia de anexo: o que uma
//! janela faz com um arquivo. O ADR 0048 deu ao servidor o mesmo trabalho —
//! «o servidor confere, com a mesma tabela de quatro formatos que a prévia de
//! anexo já usa» — e o servidor não enxerga aquele crate: o ADR 0002 proíbe, e
//! por boa razão, que o daemon ligue o cliente.
//!
//! As saídas eram duas: uma segunda cópia da tabela do lado do servidor, ou uma
//! cópia só, no crate que os dois enxergam. Uma segunda cópia é uma segunda
//! cópia que pode discordar, e o jeito como ela discordaria é um arquivo aceito
//! por um lado e recusado pelo outro.
//!
//! `seele_core::preview` continua sendo a casa do **julgamento** — o que a
//! declaração de quem enviou vale, o `data:` que a janela desenha, o teto de
//! bytes que ela baixa. O que desceu para cá é o que não é opinião: a
//! assinatura de quatro formatos e o nome de cada um.
//!
//! # O que fica de fora, e por quê
//!
//! A lista não cresceu com a mudança de casa, e o motivo de cada ausência está
//! escrito em `seele_core::preview`: SVG é marcação, PDF é documento com
//! interpretador atrás, HEIC e AVIF se distinguem de vídeo por uma marca de
//! quatro bytes dentro do mesmo contêiner, e BMP/ICO/TIFF têm assinatura de
//! dois bytes ou menos.

/// Uma imagem que este produto se dispõe a desenhar.
///
/// Enumerado fechado de propósito. O tipo de mídia que uma janela vê sai daqui
/// e de lugar nenhum além, então não há caminho pelo qual um texto escolhido
/// por quem envia chegue ao decodificador.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    /// Uma captura de tela.
    Png,
    /// Uma fotografia.
    Jpeg,
    /// O que as pessoas colam.
    Gif,
    /// O que um navegador salva hoje.
    Webp,
}

impl ImageFormat {
    /// A lista inteira, num lugar só.
    ///
    /// Aqui para que uma tela decidindo se oferece prévia pergunte a este
    /// módulo em vez de guardar uma cópia dos quatro. Uma segunda cópia é uma
    /// segunda cópia que pode discordar, e o jeito como ela discordaria é uma
    /// janela oferecendo desenhar o que este módulo vai recusar.
    pub const ALL: [Self; 4] = [Self::Png, Self::Jpeg, Self::Gif, Self::Webp];

    /// O tipo de mídia, escrito por este produto.
    #[must_use]
    pub const fn media_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::Webp => "image/webp",
        }
    }

    /// O sufixo do tipo de mídia: `png`, `jpeg`, `gif`, `webp`.
    ///
    /// É como um MOD nomeia os tipos que aceita — ADR 0048 —, e como o `data:`
    /// da janela já os escrevia. Derivado de [`Self::media_type`] e não escrito
    /// à mão pelo mesmo motivo de sempre: dois lugares divergem.
    #[must_use]
    pub fn sufixo(self) -> &'static str {
        self.media_type().rsplit('/').next().unwrap_or_default()
    }
}

/// Quantos bytes do começo decidem. A assinatura mais longa é a do WebP, doze.
pub const SNIFF_LEN: usize = 12;

/// O que os primeiros bytes de um arquivo de fato são.
///
/// `None` não é falha: é todo arquivo que não é um dos quatro, que é a maioria
/// dos arquivos.
#[must_use]
pub fn sniff(bytes: &[u8]) -> Option<ImageFormat> {
    // Oito bytes, e os últimos cinco existem para pegar uma transferência que
    // mexeu em fim de linha. Nada mais começa assim.
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some(ImageFormat::Png);
    }
    // Start of Image, e o primeiro marcador.
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(ImageFormat::Jpeg);
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some(ImageFormat::Gif);
    }
    // Um contêiner RIFF cuja forma é WEBP. Os quatro bytes entre os dois são o
    // tamanho, que não diz nada sobre o formato.
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return Some(ImageFormat::Webp);
    }
    None
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_sufixo_e_o_que_um_mod_declara() {
        assert_eq!(ImageFormat::Png.sufixo(), "png");
        assert_eq!(ImageFormat::Jpeg.sufixo(), "jpeg");
        assert_eq!(ImageFormat::Gif.sufixo(), "gif");
        assert_eq!(ImageFormat::Webp.sufixo(), "webp");
    }

    /// **Doze bytes bastam, e menos que isso não engana.**
    ///
    /// O servidor lê o tipo do começo do fluxo — ADR 0048 —, então o que
    /// `SNIFF_LEN` promete tem de ser verdade: um arquivo cortado antes disso
    /// não pode ser reconhecido como nenhum dos quatro.
    #[test]
    fn nenhuma_assinatura_precisa_de_mais_que_o_que_sniff_len_promete() {
        for formato in ImageFormat::ALL {
            let inteiro: &[u8] = match formato {
                ImageFormat::Png => &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0],
                ImageFormat::Jpeg => &[0xFF, 0xD8, 0xFF, 0xE0, 0, 0, 0, 0, 0, 0, 0, 0],
                ImageFormat::Gif => b"GIF89a______",
                ImageFormat::Webp => b"RIFF....WEBP",
            };
            assert_eq!(
                sniff(inteiro.get(..SNIFF_LEN).unwrap_or(inteiro)),
                Some(formato),
                "{formato:?} não foi reconhecido nos {SNIFF_LEN} primeiros bytes"
            );
        }
    }

    #[test]
    fn o_que_nao_e_uma_das_quatro_nao_vira_nenhuma() {
        for nao in [
            &b""[..],
            b"BM",
            b"%PDF-1.7",
            b"<svg xmlns=",
            b"RIFF....WAVE",
            b"GIF88a",
        ] {
            assert_eq!(sniff(nao), None, "{nao:?} passou por imagem");
        }
    }
}
