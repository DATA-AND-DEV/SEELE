//! O que um MOD pode trazer no pacote dele para tocar ou mostrar.
//!
//! **O tipo vem dos bytes, e nunca do nome do arquivo nem do manifesto.** É a
//! mesma regra do ADR 0027 para anexos, pela mesma razão: o caminho e o
//! manifesto são texto que o autor do MOD escolheu, e um decodificador que
//! recebe a alegação de quem mandou é um decodificador escolhido por terceiro.
//! Aqui a alegação não existe — o que chega ao `data:` é o que os bytes
//! provaram ser.
//!
//! A lista é curta de propósito, e cada formato novo é uma decisão de API em
//! vez de um MOD descobrir que consegue.

/// O que um arquivo de MOD acabou sendo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoDeMidia {
    /// Uma imagem, num dos quatro formatos que o produto já decodifica.
    Imagem(crate::imagem::ImageFormat),
    /// Som sem compressão, num contêiner RIFF.
    Wav,
    /// Som num contêiner Ogg — Vorbis ou Opus.
    Ogg,
    /// Som em MPEG, camada III.
    Mpeg,
}

impl TipoDeMidia {
    /// O tipo de mídia, escrito por este produto.
    #[must_use]
    pub const fn media_type(self) -> &'static str {
        match self {
            Self::Imagem(formato) => formato.media_type(),
            Self::Wav => "audio/wav",
            Self::Ogg => "audio/ogg",
            Self::Mpeg => "audio/mpeg",
        }
    }

    /// É som, e não imagem — é o que decide qual elemento a janela monta.
    #[must_use]
    pub const fn e_som(self) -> bool {
        !matches!(self, Self::Imagem(_))
    }

    /// A palavra que a janela recebe, para não compor nenhuma.
    #[must_use]
    pub const fn papel(self) -> &'static str {
        if self.e_som() {
            "som"
        } else {
            "imagem"
        }
    }
}

/// **O maior arquivo que um MOD pode trazer**, por arquivo.
///
/// Dez MiB, o mesmo teto da seleção de imagens. A interface também limita
/// a soma de mídias simultâneas por superfície.
pub const TETO_DE_ARQUIVO: usize = 10 * 1024 * 1024;

/// O que estes bytes são, ou nada.
///
/// Nada é a resposta para «não reconheço», e é a mesma para um formato que não
/// está na lista e para um arquivo truncado: distinguir os dois responderia
/// perguntas sobre o disco de quem está rodando.
#[must_use]
pub fn sniff(bytes: &[u8]) -> Option<TipoDeMidia> {
    if let Some(formato) = crate::imagem::sniff(bytes) {
        return Some(TipoDeMidia::Imagem(formato));
    }
    // RIFF com forma WAVE. Os quatro bytes entre os dois são o tamanho, que
    // não diz nada sobre o formato — é o mesmo contêiner do WebP.
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE") {
        return Some(TipoDeMidia::Wav);
    }
    if bytes.starts_with(b"OggS") {
        return Some(TipoDeMidia::Ogg);
    }
    // Um MP3 começa com a etiqueta ID3 ou direto num quadro. O sincronismo são
    // onze bits em um, e os cinco seguintes dizem versão e camada: aceitar só
    // `0xFF 0xFB` deixaria de fora os arquivos sem CRC e os de MPEG-2.
    if bytes.starts_with(b"ID3") {
        return Some(TipoDeMidia::Mpeg);
    }
    if let (Some(&primeiro), Some(&segundo)) = (bytes.first(), bytes.get(1)) {
        let sincronismo = primeiro == 0xFF && (segundo & 0xE0) == 0xE0;
        // Camada reservada (`00`) e versão reservada (`01`) não são MP3.
        let camada = (segundo >> 1) & 0b11;
        let versao = (segundo >> 3) & 0b11;
        if sincronismo && camada != 0 && versao != 1 {
            return Some(TipoDeMidia::Mpeg);
        }
    }
    None
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn cada_formato_e_reconhecido_pelos_bytes_dele() {
        assert_eq!(
            sniff(b"RIFF\0\0\0\0WAVEfmt ").map(TipoDeMidia::media_type),
            Some("audio/wav")
        );
        assert_eq!(
            sniff(b"OggS\0\x02\0\0").map(TipoDeMidia::media_type),
            Some("audio/ogg")
        );
        assert_eq!(
            sniff(b"ID3\x03\0\0\0").map(TipoDeMidia::media_type),
            Some("audio/mpeg")
        );
        assert_eq!(
            sniff(&[0xFF, 0xFB, 0x90, 0x00]).map(TipoDeMidia::media_type),
            Some("audio/mpeg")
        );
        // Imagem continua respondendo pelo módulo que já a conhecia.
        assert_eq!(
            sniff(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]).map(TipoDeMidia::media_type),
            Some("image/png")
        );
    }

    #[test]
    fn o_que_nao_esta_na_lista_nao_vira_som_por_conveniencia() {
        // Um RIFF que não é WAVE nem WebP: o contêiner é o mesmo, e é por isso
        // que a forma é conferida em vez de presumida.
        assert_eq!(sniff(b"RIFF\0\0\0\0AVI LIST"), None);
        // Sincronismo com camada reservada: o byte parece, e não é.
        assert_eq!(sniff(&[0xFF, 0xE1, 0x00, 0x00]), None);
        assert_eq!(sniff(b"<html>"), None);
        assert_eq!(sniff(b""), None);
        assert_eq!(sniff(b"O"), None);
    }

    #[test]
    fn som_e_imagem_se_distinguem_sem_a_janela_escrever_o_tipo() {
        assert!(sniff(b"OggS\0").expect("ogg").e_som());
        assert_eq!(sniff(b"OggS\0").expect("ogg").papel(), "som");
        let png = sniff(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]).expect("png");
        assert!(!png.e_som());
        assert_eq!(png.papel(), "imagem");
    }
}
