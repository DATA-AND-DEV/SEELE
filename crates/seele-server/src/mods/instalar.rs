//! Instalar um MOD a partir de uma pasta, para quem desenvolve.
//!
//! É o que o exemplo `servidor-com-mod.rs` usa, e o teste
//! `tests/exemplo_com_mod.rs` usa a mesma função: o exemplo e a prova dele não
//! podem instalar de dois jeitos.
//!
//! **Por hash, e não por identificador** (tarefa 15 da F3, medido na M3 em
//! 08/10). O exemplo instalava em `mundo/mods/<id>`, e o servidor procura o
//! pacote em [`crate::RaizesDosMods::pacote_de`], pelo hash do conteúdo: todo
//! pedido ao MOD morria com «No such file or directory».

use std::path::Path;

use anyhow::{Context, Result};
use seele_proto::mods::Manifest;

/// As raízes de MOD de um mundo de desenvolvimento, no arranjo da pasta do app
/// (`servidores::raizes_dos_mods`): os pacotes em `<mundo>/mod-packages/<hash>`,
/// e os dados de cada MOD em `<mundo>/mod-data/<id>`.
#[must_use]
pub fn raizes_de_um_mundo(mundo: &Path) -> crate::RaizesDosMods {
    crate::RaizesDosMods {
        pacotes: mundo.to_path_buf(),
        dados: mundo.join("mod-data"),
    }
}

/// Instala o pacote em `origem` onde o servidor o procura, e devolve o
/// manifesto e o hash do conteúdo.
///
/// O hash é o que o servidor confere antes de rodar o MOD, calculado sobre os
/// mesmos arquivos e na mesma ordem (`pedidos::package`). Um pacote com o mesmo
/// hash já instalado não é copiado de novo: os bytes são os mesmos.
///
/// # Errors
///
/// Falha se o manifesto não abre, se há atalho no pacote, ou se a cópia falha.
pub fn instalar_de_pasta(
    origem: &Path,
    raizes: &crate::RaizesDosMods,
) -> Result<(Manifest, String)> {
    let manifesto =
        seele_proto::mods::read_manifest(&std::fs::read_to_string(origem.join("mod.json"))?)?;
    let mut arquivos = Vec::new();
    super::pedidos::package(origem, origem, &mut arquivos)?;
    let hash = seele_proto::mods::hex(&seele_proto::mods::content_hash(&mut arquivos));
    let destino = raizes.pacote_de(&hash);
    if !destino.exists() {
        // Pela estufa, e não direto: uma cópia que falha no meio não pode
        // deixar no cache uma pasta cujo conteúdo não bate com o nome dela.
        let estufa = raizes.pacotes.join(".mods-em-obras").join(&hash);
        let _ = std::fs::remove_dir_all(&estufa);
        for (nome, bytes) in &arquivos {
            let caminho = estufa.join(nome);
            std::fs::create_dir_all(
                caminho
                    .parent()
                    .context("caminho sem pasta dentro do pacote")?,
            )?;
            std::fs::write(caminho, bytes)?;
        }
        std::fs::create_dir_all(destino.parent().context("o cache sem pasta")?)?;
        std::fs::rename(&estufa, &destino)?;
    }
    Ok((manifesto, hash))
}
