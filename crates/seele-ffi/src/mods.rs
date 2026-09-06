//! A superfície de MODs que a casca enxerga.
//!
//! ADR 0044. Existe como camada e não como atalho: `xtask/src/check_deps.rs`
//! deixa a casca ver `seele-ffi` e nada além, e escreve o motivo —
//! «reaching past it would put protocol knowledge in a Tauri command»
//! (`specs/06-clientes-gui.md`). Sem este módulo, um comando do Tauri teria de
//! nomear `seele_core::mods::Found` e `seele_proto::mods::Refused`, que é
//! exatamente o vazamento que aquela regra transforma em build vermelho.
//!
//! O que ele faz, então, é o de sempre nesta fronteira: achatar em campos que
//! atravessam `serde` e substituir enum de domínio por **nome de recusa**, que a
//! casca vira frase (ADR 0012).

use seele_core::mods::{hex, refusal_name, Found};

/// Um MOD em disco, como a janela o desenha.
///
/// Um MOD recusado chega com `refused` preenchido em vez de ficar de fora da
/// lista: quem largou o diretório ali tem direito ao motivo, e o defeito que o
/// `CLAUDE.md` deste repositório nomeia como o mais caro é «o produto sabe e não
/// conta».
#[derive(Debug, Clone, serde::Serialize)]
pub struct ModInstalado {
    /// `autor/nome`.
    pub id: String,
    /// A versão que o autor declara, ou vazio quando o manifesto não foi lido.
    pub version: String,
    /// O hash do conteúdo em hexadecimal, ou vazio. É o que uma pessoa compara
    /// a olho com o que o indexador publica.
    pub hash: String,
    /// O caminho do script que a janela carrega, se há metade de cliente.
    pub client: Option<String>,
    /// O nome da recusa, de uma lista fechada, quando houve uma.
    pub refused: Option<String>,
}

/// Todo MOD instalado nesta máquina, válido ou não.
///
/// Uma pasta `mods/` que não existe é uma lista vazia, e não um erro: é o estado
/// de toda instalação que nunca teve MOD.
#[must_use]
pub fn listar(pasta: &str) -> Vec<ModInstalado> {
    seele_core::mods::list(std::path::Path::new(pasta))
        .into_iter()
        .map(achatar)
        .collect()
}

/// Lê um MOD pelo identificador, para quem vai habilitá-lo.
///
/// # Errors
///
/// Devolve o nome da recusa quando o manifesto falta ou não vale.
pub fn ler_um(pasta: &str, id: &str) -> Result<ModInstalado, String> {
    let dir = std::path::Path::new(pasta).join("mods").join(id);
    match seele_core::mods::read_one(&dir) {
        Ok(instalado) => Ok(ModInstalado {
            id: instalado.manifest.id,
            version: instalado.manifest.version,
            hash: hex(&instalado.hash),
            client: instalado.manifest.client,
            refused: None,
        }),
        Err(why) => Err(refusal_name(&why).to_owned()),
    }
}

/// Um achado do `seele-core` em campos que atravessam a fronteira.
fn achatar(found: Found) -> ModInstalado {
    match found {
        Found::Ok(instalado) => ModInstalado {
            id: instalado.manifest.id,
            version: instalado.manifest.version,
            hash: hex(&instalado.hash),
            client: instalado.manifest.client,
            refused: None,
        },
        Found::Refused { id, why } => ModInstalado {
            id,
            version: String::new(),
            hash: String::new(),
            client: None,
            refused: Some(refusal_name(&why).to_owned()),
        },
    }
}

/// O caminho dentro de um MOD, reconstruído por componentes, ou nada.
///
/// Republicado e não reimplementado: a regra é a mesma que o servidor usa para
/// limitar um MOD à pasta dele, e duas cópias seriam dois lugares para
/// consertar e um para esquecer. Mora no `seele-proto` porque é o único crate
/// que a casca **e** o servidor alcançam — cada um pelo caminho que
/// `check_deps` permite.
///
/// # Errors
///
/// Devolve `None` para `..`, para componente absoluto, para raiz e para pedaço
/// vazio. Recusar em vez de resolver, porque resolver é onde um caminho que
/// parece contido deixa de estar.
#[must_use]
pub fn caminho_interno(partes: &[&str]) -> Option<std::path::PathBuf> {
    seele_core::mods::inner_path(partes)
}
