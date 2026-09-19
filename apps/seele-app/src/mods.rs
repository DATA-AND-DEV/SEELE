//! Reading a MOD's own files, for the bridge that hands them to a worker.
//!
//! ADR 0045 put these files behind a scheme of our own, `mod://`, because
//! `script-src 'self'` refuses any script not compiled into the binary.
//! **ADR 0049 took the scheme away**: a MOD no longer runs in the product's
//! window, so there is nothing in the page that loads a MOD's bytes as script,
//! and a scheme that served them would be a door with nobody on the other side.
//!
//! What is left is the reading, and it now answers one caller — `codigo_do_mod`,
//! which hands the text to a worker built from a `blob:`. The two guards that
//! made the scheme safe are the reason this module still exists, and neither is
//! politeness:
//!
//! - **only what the manifest declares.** A file sitting in the directory that
//!   `mod.json` never names is not read. A MOD is what it declared it was.
//! - **nothing climbs out.** The path is rebuilt from components, so a `..`
//!   anywhere is refused rather than resolved. Without this, reading would be
//!   arbitrary disk reads from a path a third party chooses.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::Path;

/// O manifesto que os testes deste módulo usam, com a API vinda da constante.
///
/// Escrita à mão ela era `1`, e passou a recusar no dia em que o ADR 0049 fez a
/// API ser uma só. Nenhum destes testes é sobre a versão: eles são sobre ler o
/// que o manifesto declara, copiar árvore, publicar por conteúdo e não deixar
/// estufa — e um `1` cravado os faria reprovar por um motivo que não é o deles.
#[cfg(test)]
fn manifesto_de_teste(id: &str) -> String {
    let api = seele_ffi::mods::MOD_API_VERSION;
    format!(
        r#"{{"schema":1,"id":"{id}","version":"1.0.0","api":{api},
            "repo":"https://example.invalid/x","reach":["dom"],
            "client":"cliente/main.js"}}"#
    )
}

/// Reads one path inside a MOD's package, or nothing.
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
pub(crate) fn serve(config_dir: &Path, url_path: &str, hash: &str) -> Option<Vec<u8>> {
    let mut parts = url_path.trim_start_matches('/').split('/');
    let author = parts.next()?;
    let name = parts.next()?;
    let inside: Vec<&str> = parts.collect();
    if inside.is_empty() || author.is_empty() || name.is_empty() {
        return None;
    }

    let relative = seele_ffi::mods::caminho_interno(&inside)?;

    // **O mesmo pacote que a conferência acima aprovou**, achado pelo mesmo
    // hash. Achá-lo de outro jeito aqui seria conferir uma coisa e ler outra —
    // a forma clássica de uma checagem valer para um arquivo e a leitura
    // acontecer sobre outro.
    let id = format!("{author}/{name}");
    let pacote = seele_ffi::mods::ler_por_hash(&config_dir.to_string_lossy(), hash).ok()?;
    if pacote.id != id {
        return None;
    }
    let declared = pacote.client?;
    // Only what the manifest declares. Today that is the client script; when a
    // MOD may ship more than one file, this list grows and this guard does not
    // change shape.
    if Path::new(&declared) != relative.as_path() {
        return None;
    }

    std::fs::read(
        config_dir
            .join(seele_ffi::mods::PACOTES)
            .join(hash)
            .join(&relative),
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn semear(raiz: &Path, id: &str) -> (PathBuf, String) {
        let dir = raiz.join("em-obras");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("cliente")).expect("criar diretório");
        std::fs::write(dir.join("mod.json"), manifesto_de_teste(id)).expect("manifesto");
        std::fs::write(dir.join("cliente/main.js"), "globalThis.MOD_RODOU = true;")
            .expect("script");
        // Publicado pelo conteúdo, como o instalador faz: o nome da pasta é o
        // hash do que está dentro, e é por ele que o `mod://` resolve.
        let hash = seele_ffi::mods::ler_pasta(&dir.to_string_lossy())
            .expect("ler")
            .hash;
        let destino = raiz.join(seele_ffi::mods::PACOTES).join(&hash);
        std::fs::create_dir_all(destino.parent().expect("pai")).expect("raiz");
        std::fs::rename(&dir, &destino).expect("publicar");
        (destino, hash)
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
        let (_, hash) = semear(&raiz, "seele/exemplo");
        let bytes = serve(&raiz, "/seele/exemplo/cliente/main.js", &hash).expect("devia servir");
        assert_eq!(bytes, b"globalThis.MOD_RODOU = true;");
    }

    /// O guarda que substitui o que a CSP deixa de garantir: um arquivo que o
    /// manifesto não declara não é servido, mesmo estando no diretório.
    #[test]
    fn um_arquivo_que_o_manifesto_nao_declara_nao_e_servido() {
        let raiz = temporario("nao-declarado");
        let (dir, hash) = semear(&raiz, "seele/exemplo");
        std::fs::write(dir.join("cliente/extra.js"), "// clandestino").expect("extra");
        assert_eq!(serve(&raiz, "/seele/exemplo/cliente/extra.js", &hash), None);
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
        let (_, hash) = semear(&raiz, "seele/exemplo");
        for caminho in [
            "/seele/exemplo/../../../etc/passwd",
            "/seele/exemplo/cliente/../../mod.json",
            "/../mods/seele/exemplo/cliente/main.js",
        ] {
            assert_eq!(serve(&raiz, caminho, &hash), None, "subiu com `{caminho}`");
        }
    }

    /// **O hash de um MOD com o nome de outro não serve nenhum dos dois.**
    ///
    /// Com o cache por conteúdo, a URL diz **quais bytes** quer. Servir só pelo
    /// hash entregaria o conteúdo de um MOD sob o caminho de outro — e o
    /// caminho é o que a janela usa para decidir de quem é o script. Servir só
    /// pelo identificador entregaria «o que estiver instalado», que pode ser o
    /// pacote que outro servidor desta máquina baixou depois.
    ///
    /// As duas conferências, então: os bytes existem sob aquele hash, **e**
    /// dizem ser deste MOD.
    #[test]
    fn o_hash_de_um_mod_com_o_nome_de_outro_e_recusado() {
        let raiz = temporario("trocado");
        let (_, um) = semear(&raiz, "seele/exemplo");
        let (_, outro) = semear(&raiz, "seele/outro");
        assert_ne!(um, outro);

        assert!(
            serve(&raiz, "/seele/exemplo/cliente/main.js", &um).is_some(),
            "o caminho certo com o hash certo tem de passar"
        );
        assert_eq!(
            serve(&raiz, "/seele/outro/cliente/main.js", &um),
            None,
            "o conteúdo de um MOD foi servido sob o caminho de outro"
        );
    }

    #[test]
    fn um_mod_que_nao_existe_nao_e_servido() {
        let raiz = temporario("ausente");
        assert_eq!(
            serve(&raiz, "/seele/fantasma/cliente/main.js", &"0".repeat(64)),
            None
        );
    }

    #[test]
    fn nenhum_caminho_causa_panico() {
        let raiz = temporario("panico");
        let (_, hash) = semear(&raiz, "seele/exemplo");
        for caminho in ["", "/", "//", "/a", "/a/b", "\u{0}", "/seele/exemplo/"] {
            let _ = serve(&raiz, caminho, &hash);
            let _ = serve(&raiz, caminho, "");
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
    /// O disco recusou, e o que ele disse.
    ///
    /// **Aqui havia um `JaInstalado`**, e ele saiu com o endereçamento por
    /// conteúdo. A recusa existia porque havia um lugar por identificador, e
    /// escrever por cima de uma instalação boa podia quebrá-la. Pastas
    /// endereçadas pelo conteúdo não se sobrescrevem: bytes iguais são a mesma
    /// pasta, e bytes diferentes são outra.
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
/// Leva os pacotes do layout antigo para o cache por conteúdo, uma vez.
///
/// Até a v0.11.x o pacote morava em `mods/<autor>/<nome>/`. Quem já tem MODs
/// instalados tem os três oficiais lá, e a leitura passou a ser em
/// `mod-packages/<hash>/`: sem esta mudança eles sumiriam da lista, e quem
/// usasse leria isso como o produto ter perdido o que estava instalado.
///
/// **Copia e não move**, ao contrário da mudança dos dados. Os dois motivos:
/// o pacote é imutável, então uma segunda cópia não diverge da primeira; e o
/// layout antigo ainda é lido por `listar`, que a tela de gestão usa enquanto
/// a migração da interface não fecha. Apagar aqui deixaria a tela mostrando o
/// que já não existe.
///
/// Silenciosa quando não há o que levar, que é toda máquina depois da primeira
/// vez — o destino é o hash, e um pacote já levado já está lá.
pub(crate) fn levar_pacotes_antigos_para_o_cache(config_dir: &Path) {
    let antigos = config_dir.join("mods");
    let Ok(autores) = std::fs::read_dir(&antigos) else {
        return;
    };
    for autor in autores.flatten() {
        let Ok(nomes) = std::fs::read_dir(autor.path()) else {
            continue;
        };
        for nome in nomes.flatten() {
            let dir = nome.path();
            if !dir.join("mod.json").is_file() {
                continue;
            }
            let Ok(lido) = seele_ffi::mods::ler_pasta(&dir.to_string_lossy()) else {
                continue;
            };
            let destino = config_dir.join(seele_ffi::mods::PACOTES).join(&lido.hash);
            if destino.exists() {
                continue;
            }
            // Pela estufa, e não direto: uma cópia que falha no meio não pode
            // deixar no cache uma pasta cujo conteúdo não bate com o nome dela
            // — a listagem a recusaria, e a recusa apontaria para um defeito
            // que não existe.
            let estufa = config_dir.join(".mods-em-obras").join(&lido.hash);
            let _ = std::fs::remove_dir_all(&estufa);
            if copiar_arvore(&dir, &estufa).is_err() {
                let _ = std::fs::remove_dir_all(&estufa);
                continue;
            }
            if let Some(pai) = destino.parent() {
                if std::fs::create_dir_all(pai).is_err() {
                    let _ = std::fs::remove_dir_all(&estufa);
                    continue;
                }
            }
            match std::fs::rename(&estufa, &destino) {
                Ok(()) => tracing::info!(
                    mod_id = %lido.id,
                    hash = %lido.hash,
                    "pacote levado do layout antigo para o cache por conteúdo"
                ),
                Err(erro) => {
                    tracing::warn!(mod_id = %lido.id, %erro, "não consegui levar este pacote");
                    let _ = std::fs::remove_dir_all(&estufa);
                }
            }
        }
    }
}

/// O que uma instalação publicou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Publicado {
    /// `autor/nome`, lido do manifesto.
    pub(crate) id: String,
    /// O hash do conteúdo, que é o nome da pasta onde ele ficou.
    pub(crate) hash: String,
}

/// Põe um pacote no cache endereçado pelo conteúdo.
///
/// # Por que isto encolheu
///
/// Antes havia um lugar por identificador, e daí vinha tudo o mais: um sinal
/// de `substituir`, uma recusa `JaInstalado`, a troca em dois `rename` com
/// volta atrás, e a mudança do `dados/` de um lado para o outro para que
/// atualizar não apagasse a mesa de ninguém.
///
/// Endereçado pelo conteúdo, nada disso existe:
///
///   - **não há substituição.** Bytes iguais dão a mesma pasta; bytes
///     diferentes dão outra. Instalar por cima é uma operação que não tem como
///     acontecer;
///   - **não há «já instalado» como recusa.** Se a pasta existe, o trabalho
///     está feito — e responder erro a quem pediu exatamente o que já está lá
///     seria inventar um problema;
///   - **não há dados a preservar.** Eles saíram do pacote na separação de
///     raízes: vivem em `servidores/<id>/mod-data`, que é da instância.
///
/// A montagem ao lado fica, e pelo motivo de sempre (A10 da auditoria): uma
/// cópia que falha no meio não pode deixar uma pasta com parte do MOD onde a
/// leitura seguinte a encontre.
///
/// # Errors
///
/// [`FalhaAoInstalarMod`] quando falta manifesto, quando ele é recusado, ou
/// quando a cópia não termina.
pub(crate) fn instalar_de(
    config_dir: &Path,
    origem: &Path,
) -> Result<Publicado, FalhaAoInstalarMod> {
    if !origem.join("mod.json").is_file() {
        return Err(FalhaAoInstalarMod::SemManifesto);
    }
    let lido = seele_ffi::mods::ler_pasta(&origem.to_string_lossy())
        .map_err(FalhaAoInstalarMod::ManifestoRecusado)?;
    let id = lido.id;

    // A estufa fica **fora** da raiz dos pacotes, e não é detalhe: a listagem
    // confere o nome da pasta contra o conteúdo, e uma pasta de obras lá dentro
    // apareceria como um pacote recusado enquanto a cópia acontece.
    let estufa = config_dir.join(".mods-em-obras").join(&id);
    let _ = std::fs::remove_dir_all(&estufa);
    if let Err(erro) = copiar_arvore(origem, &estufa) {
        let _ = std::fs::remove_dir_all(&estufa);
        return Err(FalhaAoInstalarMod::NaoCopiei(erro.to_string()));
    }

    // **O hash sai do que foi copiado, e não do que foi lido.** São a mesma
    // coisa quando a cópia deu certo — e quando não deu, é o que está em disco
    // que vai ser servido, então é dele que o nome tem de sair.
    let copiado = match seele_ffi::mods::ler_pasta(&estufa.to_string_lossy()) {
        Ok(copiado) => copiado,
        Err(motivo) => {
            let _ = std::fs::remove_dir_all(&estufa);
            return Err(FalhaAoInstalarMod::ManifestoRecusado(motivo));
        }
    };
    let hash = copiado.hash;

    let destino = config_dir.join(seele_ffi::mods::PACOTES).join(&hash);
    if destino.exists() {
        // Já está aqui, com estes bytes. Não é erro, e não há o que fazer.
        let _ = std::fs::remove_dir_all(&estufa);
        return Ok(Publicado { id, hash });
    }
    if let Some(pai) = destino.parent() {
        if let Err(erro) = std::fs::create_dir_all(pai) {
            let _ = std::fs::remove_dir_all(&estufa);
            return Err(FalhaAoInstalarMod::NaoCopiei(erro.to_string()));
        }
    }
    if let Err(erro) = std::fs::rename(&estufa, &destino) {
        let _ = std::fs::remove_dir_all(&estufa);
        return Err(FalhaAoInstalarMod::NaoCopiei(erro.to_string()));
    }
    Ok(Publicado { id, hash })
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
    use super::{instalar_de, manifesto_de_teste, FalhaAoInstalarMod};

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

    /// Um pacote de MOD de mentira, válido. `extra` muda os bytes sem mudar o
    /// identificador — que é como dois pacotes do mesmo MOD existem.
    fn pacote(raiz: &std::path::Path, id: &str, extra: Option<&str>) -> std::path::PathBuf {
        let origem = raiz.join("origem");
        std::fs::create_dir_all(origem.join("cliente")).unwrap();
        std::fs::write(origem.join("mod.json"), manifesto_de_teste(id)).unwrap();
        std::fs::write(origem.join("cliente/main.js"), b"globalThis.x = 1;").unwrap();
        if let Some(conteudo) = extra {
            std::fs::write(origem.join("extra.txt"), conteudo).unwrap();
        }
        origem
    }

    #[test]
    fn um_pacote_bom_chega_inteiro_e_vai_para_a_pasta_do_conteudo() {
        let raiz = pasta("bom");
        let origem = pacote(&raiz, "autor/exemplo", None);
        let config = raiz.join("config");

        let publicado = instalar_de(&config, &origem).expect("um pacote válido tem de instalar");

        assert_eq!(
            publicado.id, "autor/exemplo",
            "o identificador não saiu do manifesto"
        );
        assert_eq!(publicado.hash.len(), 64, "{}", publicado.hash);
        let destino = config.join(seele_ffi::mods::PACOTES).join(&publicado.hash);
        assert!(
            destino.join("cliente/main.js").is_file(),
            "o pacote não chegou inteiro à pasta do conteúdo"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **Instalar o mesmo pacote de novo não é erro, e não duplica nada.**
    ///
    /// Antes havia um lugar por identificador, e instalar por cima era uma
    /// recusa — `JaInstalado`. Endereçado pelo conteúdo, a pergunta desaparece:
    /// bytes iguais são a mesma pasta, e responder erro a quem pediu
    /// exatamente o que já está lá seria inventar um problema.
    #[test]
    fn instalar_o_mesmo_pacote_de_novo_e_o_mesmo_lugar_e_nao_e_erro() {
        let raiz = pasta("de-novo");
        let origem = pacote(&raiz, "autor/exemplo", None);
        let config = raiz.join("config");

        let um = instalar_de(&config, &origem).expect("primeira");
        let outro = instalar_de(&config, &origem).expect("segunda");

        assert_eq!(um, outro, "o mesmo conteúdo foi para dois lugares");
        let quantos = std::fs::read_dir(config.join(seele_ffi::mods::PACOTES))
            .unwrap()
            .count();
        assert_eq!(quantos, 1, "o mesmo pacote foi guardado duas vezes");
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **Dois conteúdos do mesmo MOD convivem**, que é o P1 que este layout
    /// conserta: por identificador, instalar a versão que um servidor exige
    /// apagava a que o outro exigia.
    #[test]
    fn dois_conteudos_do_mesmo_mod_ficam_lado_a_lado() {
        let raiz = pasta("dois");
        let config = raiz.join("config");

        let um = instalar_de(&config, &pacote(&raiz, "autor/exemplo", None)).expect("primeiro");
        let _ = std::fs::remove_dir_all(raiz.join("origem"));
        let outro = instalar_de(
            &config,
            &pacote(&raiz, "autor/exemplo", Some("outra coisa")),
        )
        .expect("segundo");

        assert_eq!(um.id, outro.id);
        assert_ne!(um.hash, outro.hash, "bytes diferentes deram o mesmo hash");
        assert!(config
            .join(seele_ffi::mods::PACOTES)
            .join(&um.hash)
            .is_dir());
        assert!(config
            .join(seele_ffi::mods::PACOTES)
            .join(&outro.hash)
            .is_dir());
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn cada_recusa_diz_qual_foi_e_nada_e_copiado() {
        let raiz = pasta("recusas");
        let config = raiz.join("config");

        let vazia = raiz.join("vazia");
        std::fs::create_dir_all(&vazia).unwrap();
        assert!(matches!(
            instalar_de(&config, &vazia),
            Err(FalhaAoInstalarMod::SemManifesto)
        ));
        assert!(
            !config.join(seele_ffi::mods::PACOTES).exists(),
            "uma recusa deixou pacote no cache"
        );

        let torto = raiz.join("torto");
        std::fs::create_dir_all(&torto).unwrap();
        std::fs::write(torto.join("mod.json"), b"{isto nao e json}").unwrap();
        assert!(matches!(
            instalar_de(&config, &torto),
            Err(FalhaAoInstalarMod::ManifestoRecusado(_))
        ));
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn um_atalho_dentro_do_pacote_nao_e_instalado() {
        // Um link simbólico faria o `mod://` servir à janela um arquivo de
        // outro lugar do disco, como se fosse do MOD.
        #[cfg(unix)]
        {
            let raiz = pasta("atalho");
            let origem = pacote(&raiz, "autor/exemplo", None);
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

    /// **A10 da auditoria: uma cópia que falha não deixa nada pela metade.**
    ///
    /// A falha é injetada com um atalho dentro do pacote, que `copiar_arvore`
    /// recusa **depois** de já ter criado diretório e copiado arquivo.
    // **Só onde há atalho.** O Windows não tem `std::os::unix::fs::symlink`.
    #[cfg(unix)]
    #[test]
    fn uma_copia_que_falha_no_meio_nao_deixa_pacote_pela_metade() {
        let raiz = pasta("meia-copia");
        let origem = pacote(&raiz, "autor/exemplo", None);
        std::fs::create_dir_all(origem.join("extra")).unwrap();
        std::os::unix::fs::symlink("/etc/hosts", origem.join("extra/atalho")).unwrap();
        let config = raiz.join("config");

        assert!(matches!(
            instalar_de(&config, &origem),
            Err(FalhaAoInstalarMod::NaoCopiei(_))
        ));
        assert!(
            !config.join(seele_ffi::mods::PACOTES).exists()
                || std::fs::read_dir(config.join(seele_ffi::mods::PACOTES))
                    .unwrap()
                    .count()
                    == 0,
            "a cópia que falhou publicou um pacote assim mesmo"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **E uma cópia que falha não alcança o que já estava publicado.**
    ///
    /// Antes isto era uma dança de dois `rename` com volta atrás, porque o
    /// destino era o mesmo lugar. Agora é uma propriedade do layout: pacotes
    /// diferentes são pastas diferentes, e uma instalação que falha não tem
    /// como tocar na de outro.
    #[cfg(unix)]
    #[test]
    fn uma_copia_que_falha_nao_toca_no_que_ja_estava_publicado() {
        let raiz = pasta("nao-toca");
        let config = raiz.join("config");
        let bom = instalar_de(&config, &pacote(&raiz, "autor/exemplo", None)).expect("o bom");
        let _ = std::fs::remove_dir_all(raiz.join("origem"));

        let torto = pacote(&raiz, "autor/exemplo", Some("outro"));
        std::fs::create_dir_all(torto.join("extra")).unwrap();
        std::os::unix::fs::symlink("/etc/hosts", torto.join("extra/atalho")).unwrap();
        assert!(instalar_de(&config, &torto).is_err());

        assert!(
            config
                .join(seele_ffi::mods::PACOTES)
                .join(&bom.hash)
                .join("cliente/main.js")
                .is_file(),
            "a instalação que falhou levou junto o pacote que já estava lá"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// E o caminho feliz não deixa rastro: uma estufa esquecida é disco ocupado
    /// que ninguém sabe explicar.
    #[test]
    fn uma_instalacao_que_deu_certo_nao_deixa_estufa() {
        let raiz = pasta("limpo");
        let origem = pacote(&raiz, "autor/exemplo", None);
        let config = raiz.join("config");

        instalar_de(&config, &origem).expect("instala");

        assert!(!config.join(".mods-em-obras/autor/exemplo").exists());
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **Quem já tinha MOD instalado não o perde.**
    ///
    /// Até a v0.11.x o pacote morava em `mods/<autor>/<nome>/`, e a leitura
    /// passou a ser por conteúdo. Sem a mudança, os três MODs oficiais de uma
    /// máquina em uso sumiriam da lista — e quem usa leria isso como o produto
    /// ter perdido o que estava instalado, que é o pior jeito de mudar um
    /// layout.
    #[test]
    fn um_pacote_do_layout_antigo_e_levado_para_o_cache() {
        let raiz = pasta("migracao");
        let config = raiz.join("config");
        let antigo = config.join("mods").join("seele").join("exemplo");
        std::fs::create_dir_all(antigo.join("cliente")).unwrap();
        std::fs::write(
            antigo.join("mod.json"),
            manifesto_de_teste("seele/exemplo").as_bytes(),
        )
        .unwrap();
        std::fs::write(antigo.join("cliente/main.js"), b"globalThis.x = 1;").unwrap();
        let hash = seele_ffi::mods::ler_pasta(&antigo.to_string_lossy())
            .expect("ler")
            .hash;

        super::levar_pacotes_antigos_para_o_cache(&config);

        let destino = config.join(seele_ffi::mods::PACOTES).join(&hash);
        assert!(
            destino.join("cliente/main.js").is_file(),
            "o pacote do layout antigo não chegou ao cache"
        );
        // **Copiado, e não movido.** O pacote é imutável, então a segunda cópia
        // não diverge; e o layout antigo ainda é lido pela tela de gestão
        // enquanto a migração da interface não fecha. Apagar aqui a deixaria
        // mostrando o que já não existe.
        assert!(
            antigo.join("cliente/main.js").is_file(),
            "o layout antigo foi apagado antes de deixar de ser lido"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// E a segunda passagem não faz nada: o destino é o hash, e um pacote já
    /// levado já está lá.
    #[test]
    fn levar_os_pacotes_duas_vezes_nao_duplica_nada() {
        let raiz = pasta("migracao-dupla");
        let config = raiz.join("config");
        let antigo = config.join("mods").join("seele").join("exemplo");
        std::fs::create_dir_all(antigo.join("cliente")).unwrap();
        std::fs::write(
            antigo.join("mod.json"),
            manifesto_de_teste("seele/exemplo").as_bytes(),
        )
        .unwrap();
        std::fs::write(antigo.join("cliente/main.js"), b"globalThis.x = 1;").unwrap();

        super::levar_pacotes_antigos_para_o_cache(&config);
        super::levar_pacotes_antigos_para_o_cache(&config);

        let quantos = std::fs::read_dir(config.join(seele_ffi::mods::PACOTES))
            .unwrap()
            .count();
        assert_eq!(quantos, 1, "o mesmo pacote foi levado duas vezes");
        assert!(
            !config.join(".mods-em-obras").exists()
                || std::fs::read_dir(config.join(".mods-em-obras"))
                    .unwrap()
                    .count()
                    == 0,
            "a migração deixou estufa para trás"
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }
}
