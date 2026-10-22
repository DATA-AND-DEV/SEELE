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

/// **Uma imagem reduzida ao uso que a janela nomeou** (`reducao`; rascunho da
/// API 6, §4; tarefa 6, Step 5), e quanto ela pesa decodificada. Sem uso, ou
/// para um som, os bytes seguem como vieram.
///
/// # Errors
///
/// `formato-desconhecido`, quando a imagem não decodifica.
pub fn reduzir_ao_uso(
    bytes: Vec<u8>,
    teto: Option<&Teto>,
) -> Result<(Vec<u8>, Option<u64>), String> {
    let Some(teto) = teto else {
        return Ok((bytes, None));
    };
    if !ler_tipo(&bytes).is_some_and(|tipo| tipo.papel == "imagem") {
        return Ok((bytes, None));
    }
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
