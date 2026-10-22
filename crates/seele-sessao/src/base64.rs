//! **O base64 do RFC 4648, de ida e de volta** (o par mora junto, e é regra:
//! ver a volta). Saiu do `seele-ffi` na Tarefa 2.6 da F4, para o navegador
//! desfazer a mídia que um MOD manda pelas mesmas vinte linhas; o `seele-ffi` o
//! reexporta na raiz, e quem os usava não mudou.

/// Base64 padrão, com o preenchimento que o `atob` da janela espera.
///
/// Sem o crate `base64`, pela mesma razão que `seele-video` escreveu quando
/// recusou o `hex`: ele entraria na árvore só para isto, e isto são vinte
/// linhas. A tabela é a do RFC 4648, que é a que o navegador decodifica.
///
/// **Público desde a imagem de perfil.** A casca desenha figuras que vêm da
/// rede, e uma `data:` URL é a única forma de pôr bytes num `<img>` sem um
/// arquivo no disco. O `seele-app` precisava exatamente disto, e escrever um
/// segundo codificador lá seria dois lugares para o mesmo alfabeto — a forma
/// de defeito que o `de_base64` logo abaixo existe para não repetir.
pub fn base64_de(bytes: &[u8]) -> String {
    const TABELA: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut texto = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for grupo in bytes.chunks(3) {
        // Três bytes viram quatro caracteres de seis bits. Os que faltam no
        // último grupo entram como zero e saem como `=`.
        let (a, b, c) = (
            u32::from(grupo.first().copied().unwrap_or(0)),
            u32::from(grupo.get(1).copied().unwrap_or(0)),
            u32::from(grupo.get(2).copied().unwrap_or(0)),
        );
        let junto = (a << 16) | (b << 8) | c;
        for casa in 0..4 {
            // Cada casa depois do que o grupo de fato trouxe vira `=`: dois
            // bytes dão três caracteres e um `=`, um byte dá dois e dois `=`.
            if casa > grupo.len() {
                texto.push('=');
                continue;
            }
            let indice = ((junto >> (18 - 6 * casa)) & 0x3F) as usize;
            // `get` e não `[]`: seis bits mascarados não podem passar de 63 e a
            // tabela tem 64, mas quem lê esta linha não tem por que refazer essa
            // conta para acreditar nela — e `indexing_slicing` é aviso aqui de
            // propósito, justamente para não haver a linha que só está certa
            // porque alguém conferiu.
            if let Some(letra) = TABELA.get(indice) {
                texto.push(char::from(*letra));
            }
        }
    }
    texto
}

/// A volta: base64 padrão para os bytes que ele descreve.
///
/// **Mora colada na ida, e é regra e não arrumação.** Um par de conversões
/// escrito em dois arquivos é um par que ganha um caso de um lado só — e este
/// projeto pagou por isso em campo no mesmo mês: o `palco-imagem.js` declarava
/// o perfil do vídeo que o `codec.rs` decidia, e os dois deixaram de concordar
/// sem que nada avisasse.
///
/// Existe porque uma imagem **colada** não tem caminho de arquivo: o que a
/// janela tem são bytes, e a ponte carrega texto. A janela codifica com o que
/// já tem — `FileReader`, nativo — e este lado desfaz.
///
/// `None` quando o texto não é base64: caractere fora da tabela, ou um resto de
/// um caractere só, que não completa byte nenhum. Devolver bytes truncados
/// daria um arquivo corrompido com cara de arquivo bom, e o defeito apareceria
/// como «esta imagem não abre» na máquina de quem recebeu.
#[must_use]
pub fn de_base64(texto: &str) -> Option<Vec<u8>> {
    /// O inverso da tabela do RFC 4648, resolvido por conta e não por tabela:
    /// são quatro faixas contíguas, e escrever 256 entradas à mão seria mais
    /// linhas e mais lugares para errar uma.
    const fn valor(byte: u8) -> Option<u32> {
        match byte {
            b'A'..=b'Z' => Some((byte - b'A') as u32),
            b'a'..=b'z' => Some((byte - b'a') as u32 + 26),
            b'0'..=b'9' => Some((byte - b'0') as u32 + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    // O preenchimento sai antes: ele marca o fim e não carrega bits. Espaço em
    // branco também — quem cola base64 de um e-mail cola as quebras de linha
    // junto, e recusar por causa delas seria recusar o caso comum.
    let uteis: Vec<u8> = texto
        .bytes()
        .filter(|b| !b.is_ascii_whitespace() && *b != b'=')
        .collect();

    let mut bytes = Vec::with_capacity(uteis.len() / 4 * 3);
    for grupo in uteis.chunks(4) {
        // Um caractere sozinho são seis bits: não fecha byte nenhum, e é a
        // marca de um texto cortado no meio.
        if grupo.len() < 2 {
            return None;
        }
        let mut junto = 0u32;
        for (casa, letra) in grupo.iter().enumerate() {
            junto |= valor(*letra)? << (18 - 6 * casa);
        }
        // Dois caracteres dão um byte, três dão dois, quatro dão três.
        for casa in 0..grupo.len() - 1 {
            bytes.push(((junto >> (16 - 8 * casa)) & 0xFF) as u8);
        }
    }
    Some(bytes)
}
