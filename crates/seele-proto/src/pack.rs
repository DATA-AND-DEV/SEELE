//! **O `.mrpack`** (API 8 parte 6; spec §8): o tipo, o teto e a conferência
//! pelos bytes.
//!
//! Um `.mrpack` é um zip com `modrinth.index.json` na raiz (o formato do
//! Modrinth). O produto o reconhece **pelos bytes**, como reconhece uma
//! imagem: um zip qualquer com a extensão trocada não passa, e um `.mrpack`
//! com outro nome passa. Só se lê o diretório central do zip — o índice não é
//! aberto aqui; quem o lê é o MOD, na máquina, depois de conferir o hash.

/// O tipo de mídia, como o Modrinth o registra.
pub const TIPO_DO_PACK: &str = "application/x-modrinth-modpack+zip";

/// O sufixo que um MOD declara em `volume.esperar` e no seletor.
pub const SUFIXO_DO_PACK: &str = "mrpack";

/// **O maior pack que se envia: 64 MiB** (spec §8). Um `.mrpack` só com a
/// lista de mods tem poucos KiB; o que pesa são os `overrides`. Acima disto, a
/// tela sugere o link do Modrinth, que não passa pelo envio.
pub const TETO_DO_PACK: u64 = 64 * 1024 * 1024;

/// O índice que faz de um zip um `.mrpack`.
pub const INDICE: &str = "modrinth.index.json";

/// Quantos bytes do fim bastam para achar o fim do diretório central: os 22
/// do registro mais o maior comentário (65 535).
pub const CAUDA: usize = 22 + 0xffff;

fn u16_em(b: &[u8], i: usize) -> Option<usize> {
    let p: [u8; 2] = b.get(i..i + 2)?.try_into().ok()?;
    Some(usize::from(u16::from_le_bytes(p)))
}

fn u32_em(b: &[u8], i: usize) -> Option<u64> {
    let p: [u8; 4] = b.get(i..i + 4)?.try_into().ok()?;
    Some(u64::from(u32::from_le_bytes(p)))
}

/// **Onde está o diretório central**, lido da cauda do arquivo (os últimos
/// até [`CAUDA`] bytes) e do tamanho total: `(início, tamanho)`. `None` se
/// não há fim de diretório, se ele é de zip64 ou se aponta para fora.
#[must_use]
pub fn diretorio_central(cauda: &[u8], total: u64) -> Option<(u64, u64)> {
    const ASSINATURA: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
    if cauda.len() < 22 {
        return None;
    }
    let fim = (0..=cauda.len() - 22)
        .rev()
        .find(|&i| cauda.get(i..i + 4) == Some(&ASSINATURA[..]))?;
    let tamanho = u32_em(cauda, fim + 12)?;
    let inicio = u32_em(cauda, fim + 16)?;
    // zip64 marca os campos com 0xffffffff: recusado, como no `desempacotar`.
    if tamanho == 0xffff_ffff || inicio == 0xffff_ffff {
        return None;
    }
    (inicio.checked_add(tamanho)? <= total).then_some((inicio, tamanho))
}

/// Se o diretório central tem uma entrada chamada exatamente [`INDICE`] —
/// na raiz, sem pasta.
#[must_use]
pub fn tem_o_indice_na_raiz(diretorio: &[u8]) -> bool {
    const ASSINATURA: [u8; 4] = [0x50, 0x4b, 0x01, 0x02];
    let mut i = 0_usize;
    while diretorio.get(i..i + 4) == Some(&ASSINATURA[..]) {
        let (Some(nome), Some(extra), Some(comentario)) = (
            u16_em(diretorio, i + 28),
            u16_em(diretorio, i + 30),
            u16_em(diretorio, i + 32),
        ) else {
            return false;
        };
        if diretorio.get(i + 46..i + 46 + nome) == Some(INDICE.as_bytes()) {
            return true;
        }
        i += 46 + nome + extra + comentario;
    }
    false
}

/// **Se estes bytes são um `.mrpack`**: um zip (cabeçalho local no começo)
/// com [`INDICE`] na raiz.
#[must_use]
pub fn e_pack(bytes: &[u8]) -> bool {
    if bytes.get(..4) != Some(&[0x50, 0x4b, 0x03, 0x04][..]) {
        return false;
    }
    let total = bytes.len() as u64;
    let cauda = bytes
        .get(bytes.len().saturating_sub(CAUDA)..)
        .unwrap_or_default();
    let Some((inicio, tamanho)) = diretorio_central(cauda, total) else {
        return false;
    };
    let (Ok(inicio), Ok(tamanho)) = (usize::try_from(inicio), usize::try_from(tamanho)) else {
        return false;
    };
    bytes
        .get(inicio..inicio + tamanho)
        .is_some_and(tem_o_indice_na_raiz)
}

#[cfg(test)]
mod testes {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]

    use super::*;

    /// Um zip «stored» mínimo, com estas entradas vazias.
    pub(crate) fn zip_com(nomes: &[&str]) -> Vec<u8> {
        let mut corpo = Vec::new();
        let mut diretorio = Vec::new();
        for nome in nomes {
            let deslocamento = corpo.len() as u32;
            corpo.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]);
            corpo.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            corpo.extend_from_slice(&[0; 12]);
            corpo.extend_from_slice(&(nome.len() as u16).to_le_bytes());
            corpo.extend_from_slice(&[0, 0]);
            corpo.extend_from_slice(nome.as_bytes());
            diretorio.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]);
            diretorio.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            diretorio.extend_from_slice(&[0; 12]);
            diretorio.extend_from_slice(&(nome.len() as u16).to_le_bytes());
            diretorio.extend_from_slice(&[0; 12]);
            diretorio.extend_from_slice(&deslocamento.to_le_bytes());
            diretorio.extend_from_slice(nome.as_bytes());
        }
        let inicio = corpo.len() as u32;
        let tamanho = diretorio.len() as u32;
        corpo.extend_from_slice(&diretorio);
        corpo.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0]);
        corpo.extend_from_slice(&(nomes.len() as u16).to_le_bytes());
        corpo.extend_from_slice(&(nomes.len() as u16).to_le_bytes());
        corpo.extend_from_slice(&tamanho.to_le_bytes());
        corpo.extend_from_slice(&inicio.to_le_bytes());
        corpo.extend_from_slice(&[0, 0]);
        corpo
    }

    #[test]
    fn um_zip_com_o_indice_na_raiz_e_um_pack() {
        assert!(e_pack(&zip_com(&[
            "overrides/a.txt",
            "modrinth.index.json"
        ])));
    }

    #[test]
    fn um_zip_sem_o_indice_ou_com_ele_numa_pasta_nao_e() {
        assert!(!e_pack(&zip_com(&["overrides/a.txt"])));
        assert!(!e_pack(&zip_com(&["pasta/modrinth.index.json"])));
        assert!(!e_pack(&zip_com(&["modrinth.index.json.bak"])));
    }

    #[test]
    fn bytes_que_nao_sao_zip_nao_sao_pack() {
        assert!(!e_pack(b"\x89PNG\r\n\x1a\n"));
        assert!(!e_pack(b""));
        let mut cortado = zip_com(&["modrinth.index.json"]);
        cortado.truncate(cortado.len() - 10);
        assert!(!e_pack(&cortado));
    }
}
