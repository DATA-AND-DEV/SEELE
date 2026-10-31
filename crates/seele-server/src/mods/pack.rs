//! **O índice de um `.mrpack`, para a metade de servidor** (API 8 parte 6).
//!
//! A metade de servidor monta a oferta da máquina antes do aceite, e a oferta
//! precisa do que o pack pede: a versão do Minecraft, o loader e a versão
//! dele (o `dependencies` do `modrinth.index.json`). Ela não lê bytes do
//! volume, e o `mundo.buscar` só devolve texto. Então **o produto abre o pack
//! e devolve o texto do índice**, com o tamanho e o `sha512` dos bytes — o
//! hash que vai em `oferta.pack`, e que a máquina confere de novo. O MOD é
//! quem aplica as regras do índice (as chaves de `dependencies`, o
//! `env.server`, os domínios, os caminhos); o produto só o tira do zip.
//!
//! Duas portas, e o mesmo leitor: `volume.pack(caminho)`, para o pack que
//! quem configura enviou, e `mundo.buscarPack({ url, sha512, tamanho })`, para
//! o pack do link do Modrinth.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::io::{Read, Seek, SeekFrom};

use seele_proto::pack::{diretorio_central, CAUDA, INDICE, TETO_DO_PACK};

/// O maior índice que se abre: 4 MiB descomprimidos. Um pack de 300 mods tem
/// cerca de 250 KiB de índice; o teto é contra uma bomba de zip.
pub const TETO_DO_INDICE: u64 = 4 * 1024 * 1024;

/// O que o MOD recebe de um pack lido.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PackLido {
    /// Quantos bytes o pack tem.
    pub bytes: u64,
    /// O SHA-512 dos bytes, em hexadecimal minúsculo.
    pub sha512: String,
    /// O texto do `modrinth.index.json`.
    pub indice: String,
}

/// Por que o pack não se leu. O nome vai ao MOD em `erro`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NaoLido {
    /// Nada neste caminho.
    NaoExiste,
    /// Maior que o teto.
    GrandeDemais {
        /// O tamanho.
        bytes: u64,
        /// O teto.
        teto: u64,
    },
    /// Não é um zip com o índice na raiz.
    NaoEPack,
    /// O índice existe e não se abre (método de compressão, bomba, UTF-8).
    IndiceIlegivel(String),
    /// A busca falhou (rede, status, prazo).
    BuscaFalhou,
    /// Os bytes buscados não têm o hash declarado.
    HashNaoBate {
        /// O declarado.
        esperado: String,
        /// O obtido.
        obtido: String,
    },
    /// A URL não é `https://`.
    NaoEHttps,
}

impl NaoLido {
    /// A resposta ao MOD: `ok: false`, o nome e os campos.
    #[must_use]
    pub fn resposta(&self) -> serde_json::Value {
        let mut v = self.para_o_mod();
        if let Some(o) = v.as_object_mut() {
            o.insert("ok".to_owned(), serde_json::Value::Bool(false));
        }
        v
    }

    /// O nome, de lista fechada, e os campos.
    #[must_use]
    pub fn para_o_mod(&self) -> serde_json::Value {
        match self {
            Self::NaoExiste => serde_json::json!({ "erro": "nao-existe" }),
            Self::GrandeDemais { bytes, teto } => {
                serde_json::json!({ "erro": "grande-demais", "bytes": bytes, "teto": teto })
            }
            Self::NaoEPack => serde_json::json!({ "erro": "nao-e-pack" }),
            Self::IndiceIlegivel(motivo) => {
                serde_json::json!({ "erro": "indice-ilegivel", "motivo": motivo })
            }
            Self::BuscaFalhou => serde_json::json!({ "erro": "busca-falhou" }),
            Self::HashNaoBate { esperado, obtido } => serde_json::json!({
                "erro": "hash-nao-bate", "esperado": esperado, "obtido": obtido
            }),
            Self::NaoEHttps => serde_json::json!({ "erro": "nao-e-https" }),
        }
    }
}

/// Os nomes de [`NaoLido::para_o_mod`], para o contrato.
pub const NOMES: &[&str] = &[
    "nao-existe",
    "grande-demais",
    "nao-e-pack",
    "indice-ilegivel",
    "busca-falhou",
    "hash-nao-bate",
    "nao-e-https",
];

fn u16_em(b: &[u8], i: usize) -> Option<u64> {
    let p: [u8; 2] = b.get(i..i + 2)?.try_into().ok()?;
    Some(u64::from(u16::from_le_bytes(p)))
}

fn u32_em(b: &[u8], i: usize) -> Option<u64> {
    let p: [u8; 4] = b.get(i..i + 4)?.try_into().ok()?;
    Some(u64::from(u32::from_le_bytes(p)))
}

fn ilegivel(motivo: &str) -> NaoLido {
    NaoLido::IndiceIlegivel(motivo.to_owned())
}

/// **O texto do índice**, lido de um zip de `total` bytes: a cauda, o
/// diretório central, a entrada do índice, e só os bytes dela.
///
/// # Errors
///
/// [`NaoLido::NaoEPack`] e [`NaoLido::IndiceIlegivel`].
pub fn indice_de<R: Read + Seek>(zip: &mut R, total: u64) -> Result<String, NaoLido> {
    let io = |_| NaoLido::NaoEPack;
    let mut comeco = [0_u8; 4];
    zip.seek(SeekFrom::Start(0)).map_err(io)?;
    zip.read_exact(&mut comeco).map_err(io)?;
    if comeco != [0x50, 0x4b, 0x03, 0x04] {
        return Err(NaoLido::NaoEPack);
    }
    let tamanho_da_cauda = total.min(CAUDA as u64);
    let mut cauda = vec![0_u8; usize::try_from(tamanho_da_cauda).map_err(|_| NaoLido::NaoEPack)?];
    zip.seek(SeekFrom::Start(total - tamanho_da_cauda))
        .map_err(io)?;
    zip.read_exact(&mut cauda).map_err(io)?;
    let (inicio, tamanho) = diretorio_central(&cauda, total).ok_or(NaoLido::NaoEPack)?;
    if tamanho > TETO_DO_PACK {
        return Err(NaoLido::NaoEPack);
    }
    let mut diretorio = vec![0_u8; usize::try_from(tamanho).map_err(|_| NaoLido::NaoEPack)?];
    zip.seek(SeekFrom::Start(inicio)).map_err(io)?;
    zip.read_exact(&mut diretorio).map_err(io)?;

    // A entrada do índice, na raiz.
    let mut i = 0_usize;
    let entrada = loop {
        if diretorio.get(i..i + 4) != Some(&[0x50, 0x4b, 0x01, 0x02][..]) {
            return Err(NaoLido::NaoEPack);
        }
        let campo = |d| u16_em(&diretorio, i + d).ok_or(NaoLido::NaoEPack);
        let (nome, extra, comentario) = (campo(28)?, campo(30)?, campo(32)?);
        let n = usize::try_from(nome).map_err(|_| NaoLido::NaoEPack)?;
        if diretorio.get(i + 46..i + 46 + n) == Some(INDICE.as_bytes()) {
            break i;
        }
        i += 46 + n + usize::try_from(extra + comentario).map_err(|_| NaoLido::NaoEPack)?;
    };
    let campo16 = |d| u16_em(&diretorio, entrada + d).ok_or_else(|| ilegivel("entrada curta"));
    let campo32 = |d| u32_em(&diretorio, entrada + d).ok_or_else(|| ilegivel("entrada curta"));
    let bandeiras = campo16(8)?;
    let metodo = campo16(10)?;
    let comprimido = campo32(20)?;
    let descomprimido = campo32(24)?;
    let local = campo32(42)?;
    if bandeiras & 1 != 0 {
        return Err(ilegivel("criptografado"));
    }
    if descomprimido > TETO_DO_INDICE || comprimido > TETO_DO_PACK {
        return Err(ilegivel("o índice passa de 4 MiB"));
    }
    let mut cabecalho = [0_u8; 30];
    zip.seek(SeekFrom::Start(local))
        .map_err(|_| ilegivel("cabeçalho local"))?;
    zip.read_exact(&mut cabecalho)
        .map_err(|_| ilegivel("cabeçalho local"))?;
    if cabecalho[..4] != [0x50, 0x4b, 0x03, 0x04] {
        return Err(ilegivel("cabeçalho local"));
    }
    let pulo = u16_em(&cabecalho, 26).unwrap_or(0) + u16_em(&cabecalho, 28).unwrap_or(0);
    zip.seek(SeekFrom::Start(local + 30 + pulo))
        .map_err(|_| ilegivel("dados"))?;
    let dados = zip.take(comprimido);
    let mut texto = Vec::new();
    match metodo {
        0 => {
            let mut dados = dados;
            dados
                .read_to_end(&mut texto)
                .map_err(|_| ilegivel("dados"))?;
        }
        8 => {
            flate2::read::DeflateDecoder::new(dados)
                .take(TETO_DO_INDICE + 1)
                .read_to_end(&mut texto)
                .map_err(|_| ilegivel("deflate"))?;
        }
        _ => return Err(ilegivel("método de compressão desconhecido")),
    }
    if texto.len() as u64 > TETO_DO_INDICE || texto.len() as u64 != descomprimido {
        return Err(ilegivel("tamanho descomprimido"));
    }
    String::from_utf8(texto).map_err(|_| ilegivel("não é UTF-8"))
}

fn sha512_de<R: Read>(mut r: R) -> Option<String> {
    use sha2::Digest as _;
    let mut soma = sha2::Sha512::new();
    let mut bloco = vec![0_u8; 256 * 1024];
    loop {
        let n = r.read(&mut bloco).ok()?;
        if n == 0 {
            break;
        }
        soma.update(bloco.get(..n)?);
    }
    Some(
        soma.finalize()
            .iter()
            .fold(String::new(), |mut saida, byte| {
                use std::fmt::Write as _;
                let _ = write!(saida, "{byte:02x}");
                saida
            }),
    )
}

/// O pack de um arquivo no disco (o volume da metade de servidor).
///
/// # Errors
///
/// [`NaoLido::NaoExiste`], [`NaoLido::GrandeDemais`], [`NaoLido::NaoEPack`] e
/// [`NaoLido::IndiceIlegivel`].
pub fn ler_do_disco(caminho: &std::path::Path) -> Result<PackLido, NaoLido> {
    let meta = std::fs::metadata(caminho).map_err(|_| NaoLido::NaoExiste)?;
    if !meta.is_file() {
        return Err(NaoLido::NaoExiste);
    }
    let bytes = meta.len();
    if bytes > TETO_DO_PACK {
        return Err(NaoLido::GrandeDemais {
            bytes,
            teto: TETO_DO_PACK,
        });
    }
    let mut arquivo = std::fs::File::open(caminho).map_err(|_| NaoLido::NaoExiste)?;
    let indice = indice_de(&mut arquivo, bytes)?;
    arquivo
        .seek(SeekFrom::Start(0))
        .map_err(|_| NaoLido::NaoExiste)?;
    let sha512 = sha512_de(arquivo).ok_or(NaoLido::NaoExiste)?;
    Ok(PackLido {
        bytes,
        sha512,
        indice,
    })
}

/// O pack de bytes em memória (o que `mundo.buscarPack` trouxe).
///
/// # Errors
///
/// [`NaoLido::NaoEPack`] e [`NaoLido::IndiceIlegivel`].
pub fn ler_dos_bytes(bytes: &[u8]) -> Result<PackLido, NaoLido> {
    let total = bytes.len() as u64;
    let indice = indice_de(&mut std::io::Cursor::new(bytes), total)?;
    Ok(PackLido {
        bytes: total,
        sha512: sha512_de(bytes).ok_or(NaoLido::NaoEPack)?,
        indice,
    })
}

/// **`mundo.buscarPack`**: o pack de `url`, com o tamanho e o `sha512`
/// declarados, pelo mesmo caminho e com os mesmos tetos de `mundo.buscar`
/// (2 s, 4 MiB): o pack do link do Modrinth é a lista de mods, de poucos KiB.
/// Um pack maior vai pelo envio.
///
/// # Errors
///
/// [`NaoLido::NaoEHttps`], [`NaoLido::GrandeDemais`], [`NaoLido::BuscaFalhou`],
/// [`NaoLido::HashNaoBate`] e os de [`ler_dos_bytes`].
pub fn buscar(url: &str, sha512: &str, tamanho: u64) -> Result<PackLido, NaoLido> {
    if !url.starts_with("https://") {
        return Err(NaoLido::NaoEHttps);
    }
    let teto = super::mundo::TETO_DA_RESPOSTA as u64;
    if tamanho > teto {
        return Err(NaoLido::GrandeDemais {
            bytes: tamanho,
            teto,
        });
    }
    let bytes = super::mundo::buscar_bytes(url).ok_or(NaoLido::BuscaFalhou)?;
    let lido = ler_dos_bytes(&bytes)?;
    let esperado = sha512.to_ascii_lowercase();
    if lido.sha512 != esperado || lido.bytes != tamanho {
        return Err(NaoLido::HashNaoBate {
            esperado,
            obtido: lido.sha512,
        });
    }
    Ok(lido)
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::io::Write as _;

    /// Um zip de verdade, com o índice em deflate e um override «stored».
    fn zip(indice: &[u8], deflate: bool, nome_do_indice: &str) -> Vec<u8> {
        let comprimido = if deflate {
            let mut e =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
            e.write_all(indice).unwrap();
            e.finish().unwrap()
        } else {
            indice.to_vec()
        };
        let mut z = Vec::new();
        let mut d = Vec::new();
        let mut entrada =
            |z: &mut Vec<u8>, nome: &str, dados: &[u8], metodo: u16, tamanho: usize| {
                let local = z.len() as u32;
                z.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04, 20, 0, 0, 0]);
                z.extend_from_slice(&metodo.to_le_bytes());
                z.extend_from_slice(&[0; 8]);
                z.extend_from_slice(&(dados.len() as u32).to_le_bytes());
                z.extend_from_slice(&(tamanho as u32).to_le_bytes());
                z.extend_from_slice(&(nome.len() as u16).to_le_bytes());
                z.extend_from_slice(&[0, 0]);
                z.extend_from_slice(nome.as_bytes());
                z.extend_from_slice(dados);
                d.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02, 20, 0, 20, 0, 0, 0]);
                d.extend_from_slice(&metodo.to_le_bytes());
                d.extend_from_slice(&[0; 8]);
                d.extend_from_slice(&(dados.len() as u32).to_le_bytes());
                d.extend_from_slice(&(tamanho as u32).to_le_bytes());
                d.extend_from_slice(&(nome.len() as u16).to_le_bytes());
                d.extend_from_slice(&[0; 12]);
                d.extend_from_slice(&local.to_le_bytes());
                d.extend_from_slice(nome.as_bytes());
            };
        entrada(&mut z, "overrides/a.txt", b"ola", 0, 3);
        entrada(
            &mut z,
            nome_do_indice,
            &comprimido,
            if deflate { 8 } else { 0 },
            indice.len(),
        );
        let inicio = z.len() as u32;
        z.extend_from_slice(&d);
        z.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0, 2, 0, 2, 0]);
        z.extend_from_slice(&(d.len() as u32).to_le_bytes());
        z.extend_from_slice(&inicio.to_le_bytes());
        z.extend_from_slice(&[0, 0]);
        z
    }

    const INDICE_DE_EXEMPLO: &[u8] =
        br#"{"formatVersion":1,"game":"minecraft","dependencies":{"minecraft":"1.21.4","fabric-loader":"0.16.10"},"files":[]}"#;

    #[test]
    fn o_indice_sai_do_zip_em_deflate_e_stored() {
        for deflate in [true, false] {
            let bytes = zip(INDICE_DE_EXEMPLO, deflate, INDICE);
            let lido = ler_dos_bytes(&bytes).unwrap();
            assert_eq!(lido.indice.as_bytes(), INDICE_DE_EXEMPLO);
            assert_eq!(lido.bytes, bytes.len() as u64);
            assert_eq!(lido.sha512.len(), 128);
        }
    }

    #[test]
    fn sem_o_indice_na_raiz_nao_e_pack() {
        let bytes = zip(INDICE_DE_EXEMPLO, true, "pasta/modrinth.index.json");
        assert_eq!(ler_dos_bytes(&bytes), Err(NaoLido::NaoEPack));
        assert_eq!(ler_dos_bytes(b"nao e zip"), Err(NaoLido::NaoEPack));
    }

    #[test]
    fn o_pack_do_disco_tem_o_mesmo_hash_dos_bytes() {
        let bytes = zip(INDICE_DE_EXEMPLO, true, INDICE);
        let caminho =
            std::env::temp_dir().join(format!("seele-pack-{}.mrpack", std::process::id()));
        std::fs::write(&caminho, &bytes).unwrap();
        let do_disco = ler_do_disco(&caminho).unwrap();
        let _ = std::fs::remove_file(&caminho);
        assert_eq!(do_disco, ler_dos_bytes(&bytes).unwrap());
        assert_eq!(
            ler_do_disco(std::path::Path::new("/nao/existe.mrpack")),
            Err(NaoLido::NaoExiste)
        );
    }

    #[test]
    fn a_busca_confere_https_teto_antes_da_rede() {
        assert_eq!(
            buscar("http://cdn.modrinth.com/a.mrpack", "a", 1),
            Err(NaoLido::NaoEHttps)
        );
        assert!(matches!(
            buscar("https://cdn.modrinth.com/a.mrpack", "a", 5 * 1024 * 1024),
            Err(NaoLido::GrandeDemais { .. })
        ));
    }

    #[test]
    fn cada_nome_esta_na_lista() {
        for n in [
            NaoLido::NaoExiste,
            NaoLido::GrandeDemais { bytes: 0, teto: 0 },
            NaoLido::NaoEPack,
            NaoLido::IndiceIlegivel(String::new()),
            NaoLido::BuscaFalhou,
            NaoLido::HashNaoBate {
                esperado: String::new(),
                obtido: String::new(),
            },
            NaoLido::NaoEHttps,
        ] {
            let nome = n.para_o_mod()["erro"].as_str().unwrap().to_owned();
            assert!(NOMES.contains(&nome.as_str()), "{nome}");
        }
    }
}
