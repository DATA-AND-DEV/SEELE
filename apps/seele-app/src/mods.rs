//! Serving a MOD's own files to the window, under `mod://`.
//!
//! ADR 0045. `script-src 'self'` refuses any script not compiled into the
//! binary, and loosening it to `unsafe-eval` would open the widest door in the
//! building to solve the narrowest problem. So the files stay on disk and a
//! scheme of our own serves them, with the CSP widened by exactly one scheme.
//!
//! # What replaces what the CSP stops guaranteeing
//!
//! Two guards, and neither is politeness:
//!
//! - **only what the manifest declares.** A file sitting in the directory that
//!   `mod.json` never names is not served. A MOD is what it declared it was.
//! - **nothing climbs out.** The path is rebuilt from components, so a `..`
//!   anywhere is refused rather than resolved. Without this, `mod://` would be
//!   arbitrary disk reads from a path a third party chooses.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::Path;

/// Checks the immutable package against the hash announced by the server.
pub(crate) fn hash_confere(config_dir: &Path, url_path: &str, expected: Option<&str>) -> bool {
    let mut parts = url_path.trim_start_matches('/').split('/');
    let (Some(author), Some(name), Some(expected)) = (parts.next(), parts.next(), expected) else {
        return false;
    };
    if seele_ffi::mods::caminho_interno(&[author, name]).is_none() {
        return false;
    }
    seele_ffi::mods::ler_um(&config_dir.to_string_lossy(), &format!("{author}/{name}"))
        .is_ok_and(|m| m.hash == expected)
}

/// Serves one path under `mod://`, or nothing.
///
/// `url_path` is the path component of the URL, e.g.
/// `/seele/exemplo/cliente/main.js` — author, name, then the file inside the
/// MOD.
///
/// Returns `None` for anything that is not a file this MOD declared, which is
/// deliberately the same answer for "does not exist", "not declared" and
/// "tried to climb out": a scheme that distinguishes them is a scheme that
/// answers questions about the disk of whoever is running it.
#[must_use]
pub(crate) fn serve(config_dir: &Path, url_path: &str) -> Option<Vec<u8>> {
    let mut parts = url_path.trim_start_matches('/').split('/');
    let author = parts.next()?;
    let name = parts.next()?;
    let inside: Vec<&str> = parts.collect();
    if inside.is_empty() || author.is_empty() || name.is_empty() {
        return None;
    }

    let relative = seele_ffi::mods::caminho_interno(&inside)?;

    let id = format!("{author}/{name}");
    let declared = seele_ffi::mods::ler_um(&config_dir.to_string_lossy(), &id)
        .ok()?
        .client?;
    // Only what the manifest declares. Today that is the client script; when a
    // MOD may ship more than one file, this list grows and this guard does not
    // change shape.
    if Path::new(&declared) != relative.as_path() {
        return None;
    }

    std::fs::read(
        config_dir
            .join("mods")
            .join(author)
            .join(name)
            .join(&relative),
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn semear(raiz: &Path, id: &str) -> PathBuf {
        let dir = raiz.join("mods").join(id);
        std::fs::create_dir_all(dir.join("cliente")).expect("criar diretório");
        std::fs::write(
            dir.join("mod.json"),
            format!(
                r#"{{"schema":1,"id":"{id}","version":"1.0.0","api":1,
                    "repo":"https://example.invalid/x","reach":["dom"],
                    "client":"cliente/main.js"}}"#
            ),
        )
        .expect("manifesto");
        std::fs::write(dir.join("cliente/main.js"), "globalThis.MOD_RODOU = true;")
            .expect("script");
        dir
    }

    fn temporario(nome: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("seele-modserve-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temporário");
        dir
    }

    #[test]
    fn um_arquivo_declarado_no_manifesto_e_servido() {
        let raiz = temporario("ok");
        semear(&raiz, "seele/exemplo");
        let bytes = serve(&raiz, "/seele/exemplo/cliente/main.js").expect("devia servir");
        assert_eq!(bytes, b"globalThis.MOD_RODOU = true;");
    }

    /// O guarda que substitui o que a CSP deixa de garantir: um arquivo que o
    /// manifesto não declara não é servido, mesmo estando no diretório.
    #[test]
    fn um_arquivo_que_o_manifesto_nao_declara_nao_e_servido() {
        let raiz = temporario("nao-declarado");
        let dir = semear(&raiz, "seele/exemplo");
        std::fs::write(dir.join("cliente/extra.js"), "// clandestino").expect("extra");
        assert_eq!(serve(&raiz, "/seele/exemplo/cliente/extra.js"), None);
    }

    /// Caminho inteiro por `serve`, e **este teste não é a prova da travessia**.
    ///
    /// Vale dizer, porque a suposição contrária já custou uma vez: com a regra
    /// de `seele_proto::mods::inner_path` inteiramente quebrada, este teste
    /// continua verde — a conferência do manifesto recusa estes caminhos por
    /// outro motivo. A prova de verdade é
    /// `nenhum_caminho_com_dois_pontos_vira_caminho_interno`, lá, com os dois
    /// lados. Aqui é integração: o `serve` recusa, seja qual for a razão.
    #[test]
    fn um_caminho_que_tenta_subir_nao_e_servido() {
        let raiz = temporario("subir");
        semear(&raiz, "seele/exemplo");
        for caminho in [
            "/seele/exemplo/../../../etc/passwd",
            "/seele/exemplo/cliente/../../mod.json",
            "/../mods/seele/exemplo/cliente/main.js",
        ] {
            assert_eq!(serve(&raiz, caminho), None, "subiu com `{caminho}`");
        }
    }

    #[test]
    fn um_mod_que_nao_existe_nao_e_servido() {
        let raiz = temporario("ausente");
        assert_eq!(serve(&raiz, "/seele/fantasma/cliente/main.js"), None);
    }

    #[test]
    fn nenhum_caminho_causa_panico() {
        let raiz = temporario("panico");
        semear(&raiz, "seele/exemplo");
        for caminho in ["", "/", "//", "/a", "/a/b", "\u{0}", "/seele/exemplo/"] {
            let _ = serve(&raiz, caminho);
        }
    }
}

/// Por que um pacote não pôde ser instalado.
///
/// Uma variante por recusa, com o motivo dentro — a mesma regra do resto desta
/// ponte: a frase mora no `FRASES` do JavaScript, e ela precisa saber **qual**
/// recusa foi para dizer o conserto.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) enum FalhaAoInstalarMod {
    /// A pasta escolhida não tem `mod.json`.
    ///
    /// O engano mais provável de todos: apontar para a pasta que **contém** o
    /// MOD em vez da pasta do MOD.
    SemManifesto,
    /// O `mod.json` existe e não vale, com o nome da recusa do `seele-core`.
    ManifestoRecusado(String),
    /// Já há um MOD instalado com este identificador.
    ///
    /// Recusado em vez de sobrescrito, e é a mesma decisão que o depósito de
    /// versões toma: escrever por cima de uma instalação boa é a operação que
    /// pode quebrá-la, e aqui ela ainda apagaria os `dados/` daquele MOD.
    JaInstalado {
        /// Qual.
        id: String,
    },
    /// O disco recusou, e o que ele disse.
    NaoCopiei(String),
}

/// Instala um MOD a partir de uma pasta desta máquina.
///
/// # O que ele confere antes de tocar no disco
///
/// O manifesto, inteiro, pelo mesmo `read_one` que o resto do produto usa. Um
/// pacote sem `mod.json`, com identificador malformado ou com esquema que este
/// build não lê é recusado **antes** de qualquer byte ser copiado — pela mesma
/// ordem que o depósito de versões segue, e pelo mesmo motivo: meia instalação
/// é o estado que ninguém sabe desfazer.
///
/// # O que ele não faz
///
/// **Não habilita.** Instalar põe os bytes no disco; ligar é outra decisão, de
/// quem hospeda, e tem o verbo dela. Separá-las é o que permite examinar um MOD
/// instalado antes de deixá-lo rodar.
///
/// **Não desce da rede.** A origem é uma pasta que já está aqui. Baixar do
/// catálogo é trabalho do indexador, que não está no ar.
///
/// # Errors
///
/// [`FalhaAoInstalarMod`], uma variante por motivo.
pub(crate) fn instalar_de(config_dir: &Path, origem: &Path) -> Result<String, FalhaAoInstalarMod> {
    if !origem.join("mod.json").is_file() {
        return Err(FalhaAoInstalarMod::SemManifesto);
    }
    let lido = seele_ffi::mods::ler_pasta(&origem.to_string_lossy())
        .map_err(FalhaAoInstalarMod::ManifestoRecusado)?;
    let id = lido.id;

    let destino = config_dir.join("mods").join(&id);
    if destino.exists() {
        return Err(FalhaAoInstalarMod::JaInstalado { id });
    }
    copiar_arvore(origem, &destino)
        .map_err(|erro| FalhaAoInstalarMod::NaoCopiei(erro.to_string()))?;
    Ok(id)
}

/// Copia uma árvore de arquivos, recusando atalhos.
///
/// **Atalho não passa**, e não é zelo: um link simbólico dentro do pacote faria
/// a instalação publicar como «arquivo deste MOD» algo que está noutro lugar do
/// disco — e o `mod://` serviria aquele conteúdo à janela. É a mesma recusa que
/// o `seele-lancador` faz ao ler um pacote de versão.
fn copiar_arvore(de: &Path, para: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(para)?;
    for entrada in std::fs::read_dir(de)? {
        let entrada = entrada?;
        let tipo = entrada.file_type()?;
        if tipo.is_symlink() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("atalho dentro do pacote: {}", entrada.path().display()),
            ));
        }
        let alvo = para.join(entrada.file_name());
        if tipo.is_dir() {
            copiar_arvore(&entrada.path(), &alvo)?;
        } else {
            std::fs::copy(entrada.path(), &alvo)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod instalar {
    use super::{instalar_de, FalhaAoInstalarMod};

    fn pasta(nome: &str) -> std::path::PathBuf {
        let caminho = std::env::temp_dir().join(format!(
            "seele-instalar-mod-{}-{}-{nome}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&caminho).unwrap();
        caminho
    }

    /// Um pacote de MOD de mentira, válido.
    fn pacote(raiz: &std::path::Path, id: &str) -> std::path::PathBuf {
        let origem = raiz.join("origem");
        std::fs::create_dir_all(origem.join("cliente")).unwrap();
        std::fs::write(
            origem.join("mod.json"),
            format!(
                r#"{{"schema":1,"id":"{id}","version":"1.0.0","api":1,
                    "repo":"https://example.invalid/m","reach":["dom"],
                    "client":"cliente/main.js"}}"#
            ),
        )
        .unwrap();
        std::fs::write(origem.join("cliente/main.js"), b"globalThis.x = 1;").unwrap();
        origem
    }

    #[test]
    fn um_pacote_bom_chega_inteiro_e_com_o_identificador_do_manifesto() {
        let raiz = pasta("bom");
        let origem = pacote(&raiz, "autor/exemplo");
        let config = raiz.join("config");

        let id = instalar_de(&config, &origem).expect("um pacote válido tem de instalar");
        assert_eq!(id, "autor/exemplo", "o identificador não saiu do manifesto");
        assert!(
            config.join("mods/autor/exemplo/cliente/main.js").is_file(),
            "a metade de cliente não foi copiada: o MOD instalado não é o que \
             a pessoa apontou"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn cada_recusa_diz_qual_foi_e_nada_e_copiado() {
        let raiz = pasta("recusas");
        let config = raiz.join("config");

        // Apontar para a pasta que contém o MOD, e não para a do MOD.
        let vazia = raiz.join("vazia");
        std::fs::create_dir_all(&vazia).unwrap();
        assert!(matches!(
            instalar_de(&config, &vazia),
            Err(FalhaAoInstalarMod::SemManifesto)
        ));
        assert!(
            !config.join("mods").exists(),
            "uma recusa criou a pasta de MODs: nada pode ser tocado antes de a \
             conferência passar"
        );

        // Um manifesto que existe e não vale.
        let torto = raiz.join("torto");
        std::fs::create_dir_all(&torto).unwrap();
        std::fs::write(torto.join("mod.json"), b"{ nao e json }").unwrap();
        assert!(matches!(
            instalar_de(&config, &torto),
            Err(FalhaAoInstalarMod::ManifestoRecusado(_))
        ));

        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn instalar_por_cima_de_um_ja_instalado_e_recusado() {
        // Sobrescrever apagaria os `dados/` daquele MOD junto, e essa é uma
        // perda que não se desfaz.
        let raiz = pasta("ja-instalado");
        let origem = pacote(&raiz, "autor/exemplo");
        let config = raiz.join("config");
        instalar_de(&config, &origem).expect("a primeira instala");
        std::fs::write(config.join("mods/autor/exemplo/marca-de-dados"), b"x").unwrap();

        assert!(matches!(
            instalar_de(&config, &origem),
            Err(FalhaAoInstalarMod::JaInstalado { .. })
        ));
        assert!(
            config.join("mods/autor/exemplo/marca-de-dados").is_file(),
            "a recusa mexeu na instalação que já estava lá"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn um_atalho_dentro_do_pacote_nao_e_instalado() {
        // Um link simbólico faria o `mod://` servir à janela um arquivo de
        // outro lugar do disco, como se fosse do MOD.
        #[cfg(unix)]
        {
            let raiz = pasta("atalho");
            let origem = pacote(&raiz, "autor/exemplo");
            let segredo = raiz.join("segredo.txt");
            std::fs::write(&segredo, b"nao sou deste mod").unwrap();
            std::os::unix::fs::symlink(&segredo, origem.join("atalho.txt")).unwrap();
            let config = raiz.join("config");

            assert!(matches!(
                instalar_de(&config, &origem),
                Err(FalhaAoInstalarMod::NaoCopiei(_))
            ));
            let _ = std::fs::remove_dir_all(&raiz);
        }
    }
}
