//! **A mídia que um MOD recebe do servidor, pronta para a janela** (Tarefa 2.6
//! da F4): o texto do `midia_em_bytes` e os bytes que o volume traz
//! (`ler_imagem_mod`) passam pelo mesmo caminho no desktop e no navegador — o
//! piso do som, a redução ao uso que a janela nomeou, e o `data:` com o tipo
//! que os bytes provaram ser. Saiu do `main.rs` do app, com a feature
//! `reducao`.

use crate::mods::{ler_midia, ler_tipo, TETO_DE_MIDIA};
use crate::reducao::{self, Teto};

/// Uma mídia pronta para a janela: o `MidiaDoMod` do desktop com o `data:`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiaPronta {
    /// O `data:` inteiro, com o tipo que os bytes provaram ser.
    pub uri: String,
    /// `som` ou `imagem`.
    pub papel: &'static str,
    /// Quantos bytes o arquivo tem, depois da redução.
    pub bytes: usize,
    /// O que a janela vai decodificar, quando a imagem foi reduzida a um uso.
    pub decodificados: Option<u64>,
}

/// O nome com que a imagem de um MOD que declara pixels demais é recusada. Os
/// números vêm depois dele, no mesmo motivo, como no
/// `som-grande-demais-decodificado`: «grande demais» sem número não diz a quem
/// escreve o MOD se o que passou foi o arquivo ou a imagem aberta.
pub const IMAGEM_GRANDE_DEMAIS_DECODIFICADA: &str = "imagem-grande-demais-decodificada";

/// **Uma imagem de MOD reduzida ao lugar em que aparece** (`reducao`; rascunho
/// da API 6, §4; tarefa 6, Step 5): ao uso que a janela nomeou, com o que ela
/// pesa decodificada, ou, **sem uso conhecido, ao teto geral**
/// ([`reducao::TETO_DA_MIDIA_DE_MOD`]). Um som segue como veio.
///
/// **Toda imagem de MOD passa por aqui antes do `data:`** — a do pacote
/// (`midia_do_mod`) e as do servidor (`midia_em_bytes`, `ler_imagem_mod`), no
/// desktop e no navegador. Até 11/11/2026 só a que nomeava uso era reduzida, e
/// a do pacote nem chegava a esta função: o irmão do incidente da capa em 4K.
///
/// # Errors
///
/// `formato-desconhecido`, quando a imagem não decodifica, e
/// [`IMAGEM_GRANDE_DEMAIS_DECODIFICADA`] com as medidas e o teto, quando ela
/// declara mais pixels que [`reducao::TETO_DE_PIXELS`]: recusada pelo
/// cabeçalho, sem ser aberta aqui nem na janela.
pub fn reduzir_ao_uso(
    bytes: Vec<u8>,
    teto: Option<&Teto>,
) -> Result<(Vec<u8>, Option<u64>), String> {
    if !ler_tipo(&bytes).is_some_and(|tipo| tipo.papel == "imagem") {
        return Ok((bytes, None));
    }
    let (largura, altura) = reducao::medidas(&bytes).map_err(|motivo| {
        // O decodificador recusou a imagem por limite antes de dizer as
        // medidas: é a mesma recusa, sem os números que ninguém conseguiu ler.
        if motivo == reducao::GRANDE_DEMAIS {
            format!(
                "{IMAGEM_GRANDE_DEMAIS_DECODIFICADA}: a imagem declara mais pixels do que \
                 se consegue medir, e o teto é de {} pixels",
                reducao::TETO_DE_PIXELS
            )
        } else {
            motivo
        }
    })?;
    let pixels = u64::from(largura) * u64::from(altura);
    if pixels > reducao::TETO_DE_PIXELS {
        return Err(format!(
            "{IMAGEM_GRANDE_DEMAIS_DECODIFICADA}: a imagem tem {largura} × {altura} \
             ({pixels} pixels), e o teto é de {} pixels",
            reducao::TETO_DE_PIXELS
        ));
    }
    let Some(teto) = teto else {
        return reducao::para_a_janela(bytes, &reducao::TETO_DA_MIDIA_DE_MOD)
            .map(|reduzida| (reduzida, None));
    };
    let reduzida = reducao::reduzir(&bytes, teto)?;
    Ok((reduzida.bytes, Some(reduzida.decodificados)))
}

/// **Os bytes que vieram do servidor, prontos**: o som que não cabe
/// decodificado é recusado antes do `data:` (que seria o arquivo em base64 para
/// ninguém), a imagem é reduzida ao uso, e o `data:` sai com o tipo dos bytes.
///
/// # Errors
///
/// O motivo do desktop: `som-grande-demais-decodificado: …` e
/// `formato-desconhecido`.
pub fn pronta(bytes: Vec<u8>, teto: Option<&Teto>) -> Result<MidiaPronta, String> {
    if ler_tipo(&bytes).is_some_and(|tipo| tipo.papel == "som") {
        crate::som::som_cabe_decodificado(&bytes)?;
    }
    let (bytes, decodificados) = reduzir_ao_uso(bytes, teto)?;
    let lida = ler_midia(&bytes).ok_or_else(|| "formato-desconhecido".to_owned())?;
    Ok(MidiaPronta {
        uri: lida.uri,
        papel: lida.papel,
        bytes: lida.bytes,
        decodificados,
    })
}

/// **O texto do `midia_em_bytes`**: base64 cru, ou um `data:` cujo cabeçalho
/// só delimita o conteúdo (o tipo é provado pelos bytes), desfeito e pronto.
///
/// O teto do texto é conferido **antes** de decodificar: o texto cresce um
/// terço sobre os bytes, então o teto do texto é o teto dos bytes com folga, e
/// recusar aqui é recusar sem alocar.
///
/// # Errors
///
/// O motivo do desktop: `arquivo-grande-demais`, `nao-e-base64`, e os de
/// [`pronta`].
pub fn do_texto(base64: &str, teto: Option<&Teto>) -> Result<MidiaPronta, String> {
    if base64.len() > TETO_DE_MIDIA * 2 {
        return Err("arquivo-grande-demais".to_owned());
    }
    let conteudo = if let Some(uri) = base64.strip_prefix("data:") {
        let (cabecalho, conteudo) = uri
            .split_once(',')
            .ok_or_else(|| "nao-e-base64".to_owned())?;
        if cabecalho.len() > 128 || !cabecalho.ends_with(";base64") {
            return Err("nao-e-base64".to_owned());
        }
        conteudo
    } else {
        base64
    };
    let bytes = crate::base64::de_base64(conteudo).ok_or_else(|| "nao-e-base64".to_owned())?;
    if bytes.len() > TETO_DE_MIDIA {
        return Err("arquivo-grande-demais".to_owned());
    }
    pronta(bytes, teto)
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::io::Cursor;

    /// Um PNG liso: comprime a quase nada, com as medidas que se pedir.
    fn png(largura: u32, altura: u32) -> Vec<u8> {
        let mut saida = Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(largura, altura, image::Rgba([40, 80, 120, 255]))
            .write_to(&mut saida, image::ImageFormat::Png)
            .expect("grava");
        saida.into_inner()
    }

    fn medidas_do_uri(uri: &str) -> (u32, u32) {
        let (_, conteudo) = uri.split_once(',').expect("um data:");
        let bytes = crate::base64::de_base64(conteudo).expect("base64");
        reducao::medidas(&bytes).expect("imagem")
    }

    /// **A imagem de MOD que não nomeia uso vai à janela reduzida** (a
    /// varredura do incidente da capa em 4K, 11/11/2026): sem uso, ela ia do
    /// tamanho que veio. A que cabe continua com os bytes que tinha.
    #[test]
    fn a_imagem_sem_uso_vai_a_janela_dentro_do_teto_geral() {
        let grande = pronta(png(3840, 2160), None).expect("pronta");
        assert_eq!(grande.papel, "imagem");
        assert_eq!(medidas_do_uri(&grande.uri), (1920, 1080));
        assert!(
            grande.decodificados.is_none(),
            "só o uso nomeado conta o quadro"
        );

        let pequena = png(640, 360);
        let pronta_pequena = pronta(pequena.clone(), None).expect("pronta");
        assert_eq!(pronta_pequena.bytes, pequena.len());
        assert_eq!(medidas_do_uri(&pronta_pequena.uri), (640, 360));
    }

    /// O fundo de uma caixa nomeia o uso, e chega com o teto da capa.
    #[test]
    fn o_fundo_vai_a_janela_com_o_teto_dele() {
        let teto = reducao::teto_do_uso("fundo");
        let fundo = pronta(png(3840, 2160), teto.as_ref()).expect("pronta");
        assert_eq!(medidas_do_uri(&fundo.uri), (1280, 720));
    }

    /// **A imagem que declara pixels demais é recusada pelo nome, com os
    /// números**, com uso ou sem: o arquivo é pequeno (cabe no teto de bytes),
    /// e aberto pediria gigabytes à janela.
    #[test]
    fn a_imagem_com_pixels_demais_e_recusada_com_os_numeros() {
        let bomba = png(9000, 5000);
        assert!(bomba.len() < TETO_DE_MIDIA);
        for teto in [None, reducao::teto_do_uso("capa")] {
            let motivo = pronta(bomba.clone(), teto.as_ref()).unwrap_err();
            assert!(
                motivo.starts_with(IMAGEM_GRANDE_DEMAIS_DECODIFICADA),
                "{motivo}"
            );
            assert!(motivo.contains("9000 × 5000"), "{motivo}");
            assert!(motivo.contains("36000000"), "{motivo}");
        }
    }

    /// Bytes que começam como imagem e não têm cabeçalho legível não vão à
    /// janela: sem as medidas, não há como saber o que ela abriria.
    #[test]
    fn a_imagem_sem_cabecalho_legivel_e_formato_desconhecido() {
        let so_a_assinatura = b"\x89PNG\r\n\x1a\n".to_vec();
        assert_eq!(
            pronta(so_a_assinatura, None).unwrap_err(),
            "formato-desconhecido"
        );
    }
}
