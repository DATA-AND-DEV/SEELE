//! MODs installed on this machine.
//!
//! ADR 0045. This module does the I/O and nothing else: what a manifest *is*,
//! and what makes one invalid, lives in `seele_proto::mods`. Splitting them is
//! what lets the server share the validation without depending on this crate,
//! which `xtask/src/check_deps.rs` forbids.
//!
//! # Layout on disk
//!
//! Under the ADR 0017 directory, next to `identity.key`, `pins`, `conhecidos`
//! and `preferences`:
//!
//! ```text
//! mods/<author>/<name>/mod.json
//! mods/<author>/<name>/cliente/main.js
//! ```
//!
//! # A refused MOD stays in the list
//!
//! It would be less code to skip one that fails validation. It would also be
//! the failure this repository pays for most: whoever put the directory there
//! gets no answer, and asks days later with nothing to go on.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::{Path, PathBuf};

pub use seele_proto::mods::content_hash;
use seele_proto::mods::read_manifest;

// Republicados de propósito, e não por conveniência. `xtask/src/check_deps.rs`
// impede `seele-ffi` — e portanto a casca — de nomear `seele-proto`: «reaching
// past it would put protocol knowledge in a Tauri command». Quem consome MOD
// precisa destes dois tipos, então esta camada, que é a fronteira, os
// republica. É a mesma disciplina, não uma brecha nela.
pub use seele_proto::mods::{inner_path, Manifest, ModAnunciado, Refused, MOD_API_VERSION};

/// A MOD that read cleanly.
#[derive(Debug, Clone)]
pub struct Installed {
    /// What its `mod.json` declares.
    pub manifest: Manifest,
    /// The identity of its bytes, from [`seele_proto::mods::content_hash`].
    pub hash: [u8; 32],
    /// Where it lives, so the `mod://` handler can serve out of it.
    pub dir: PathBuf,
}

/// One entry of the installed list.
///
/// `Installed` is boxed because it carries a `Manifest` and a path while the
/// refusal is two small fields, and clippy's `large_enum_variant` is right
/// about the difference.
#[derive(Debug, Clone)]
pub enum Found {
    /// It read cleanly.
    Ok(Box<Installed>),
    /// It did not, and this is what to tell the person.
    Refused {
        /// The directory it was found under, which is the only name we have
        /// when the manifest itself is unreadable.
        id: String,
        /// Why.
        why: Refused,
    },
}

/// Every MOD under `<config_dir>/mods`, valid or not.
///
/// A missing `mods/` directory is an empty list and not an error: it is the
/// state of every installation that has never had a MOD.
#[must_use]
pub fn list(config_dir: &Path) -> Vec<Found> {
    let root = config_dir.join("mods");
    let mut found = Vec::new();

    let Ok(authors) = std::fs::read_dir(&root) else {
        return found;
    };
    for author in authors.flatten() {
        let Ok(names) = std::fs::read_dir(author.path()) else {
            continue;
        };
        for name in names.flatten() {
            let dir = name.path();
            let Some(id) = identifier_of(&root, &dir) else {
                continue;
            };
            match read_one(&dir) {
                Ok(installed) if installed.manifest.id == id => {
                    found.push(Found::Ok(Box::new(installed)));
                }
                // A manifest whose `id` disagrees with where it sits would let
                // two MODs claim one directory under `mod://`.
                Ok(_) => found.push(Found::Refused {
                    id,
                    why: Refused::MalformedId,
                }),
                Err(why) => found.push(Found::Refused { id, why }),
            }
        }
    }
    found.sort_by(|left, right| name_of(left).cmp(name_of(right)));
    found
}

/// Onde os pacotes moram, endereçados pelo conteúdo.
///
/// Plano de isolamento de 18/09, P1 «uma atualização substitui o pacote de
/// outros servidores».
///
/// Antes o pacote morava em `mods/<autor>/<nome>/`: **um por identificador**.
/// Dois servidores desta máquina podem exigir hashes diferentes do mesmo MOD —
/// um ficou na versão que revisou, o outro atualizou — e só um pacote cabia
/// naquele lugar. Atualizar para um deixava o outro exigindo bytes que saíram
/// do disco.
///
/// Endereçado pelo conteúdo, os dois cabem: são pastas diferentes porque são
/// bytes diferentes. E a identidade da pasta deixa de ser uma convenção que
/// alguém pode desrespeitar — ela é **conferível**, e
/// [`listar_por_conteudo`] a confere.
pub const PACOTES: &str = "mod-packages";

/// Todo pacote guardado, com o conteúdo conferido contra o nome da pasta.
///
/// **A conferência é o ponto.** No layout por identificador, o guarda era «o
/// `id` do manifesto tem de bater com o caminho» — uma convenção, e um
/// manifesto trocado depois da instalação a quebrava em silêncio. Aqui o nome
/// da pasta é o hash do que está dentro: mexer num byte muda o hash, e o
/// pacote deixa de ser o que a pasta diz que ele é.
///
/// Uma pasta cujo conteúdo não bate é **recusada e nomeada**, e não corrigida:
/// renomeá-la para o hash certo aceitaria bytes que ninguém revisou, e apagá-la
/// esconderia o que aconteceu de quem precisa saber.
#[must_use]
pub fn listar_por_conteudo(config_dir: &Path) -> Vec<Found> {
    let root = config_dir.join(PACOTES);
    let mut found = Vec::new();

    let Ok(pastas) = std::fs::read_dir(&root) else {
        return found;
    };
    for pasta in pastas.flatten() {
        let dir = pasta.path();
        if !dir.is_dir() {
            continue;
        }
        let Some(nome) = pasta.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        match read_one(&dir) {
            Ok(installed) if hex(&installed.hash) == nome => {
                found.push(Found::Ok(Box::new(installed)));
            }
            // O conteúdo não é o que o nome promete. Pode ser um pacote mexido
            // depois de instalado, uma cópia interrompida, ou uma pasta que
            // alguém criou à mão — e nenhum dos três pode ser servido a
            // ninguém como se fosse revisado.
            Ok(installed) => found.push(Found::Refused {
                id: installed.manifest.id,
                why: Refused::MalformedId,
            }),
            Err(why) => found.push(Found::Refused { id: nome, why }),
        }
    }
    found.sort_by(|left, right| name_of(left).cmp(name_of(right)));
    found
}

/// Quantos bytes um pacote ocupa no cache, ou zero quando ele não está lá.
///
/// **Existe por causa da manutenção local**, que o plano de 18/09 pede: o cache
/// endereçado por conteúdo guarda um pacote por versão, e nada nunca o
/// esvaziava. Sem um número ao lado de cada um, a tela ofereceria «apagar»
/// sobre coisas que quem lê não tem como comparar.
///
/// Soma o que está dentro **sem seguir atalho**: um atalho apontando para fora
/// somaria o disco de outra pessoa. O instalador já recusa um pacote que os
/// tenha, e isto é a segunda tranca.
#[must_use]
pub fn bytes_do_pacote(config_dir: &Path, hash: &str) -> u64 {
    fn somar(dir: &Path) -> u64 {
        let Ok(itens) = std::fs::read_dir(dir) else {
            return 0;
        };
        let mut total = 0;
        for item in itens.flatten() {
            let Ok(tipo) = item.file_type() else { continue };
            if tipo.is_symlink() {
                continue;
            }
            if tipo.is_dir() {
                total += somar(&item.path());
            } else if let Ok(meta) = item.metadata() {
                total += meta.len();
            }
        }
        total
    }
    let Some(dir) = caminho_do_pacote(config_dir, hash) else {
        return 0;
    };
    somar(&dir)
}

/// O diretório de um pacote no cache, se o hash tem a forma de um hash.
///
/// **A conferência de forma é a que impede a travessia.** O hash chega da
/// janela, e um `../..` ali seria um caminho para apagar o que alguém escolheu
/// em vez de um pacote. Sessenta e quatro hexadecimais minúsculos não contêm
/// barra, ponto, nem separador de nenhum sistema.
#[must_use]
pub fn caminho_do_pacote(config_dir: &Path, hash: &str) -> Option<std::path::PathBuf> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return None;
    }
    Some(config_dir.join(PACOTES).join(hash))
}

/// Reads one MOD directory.
///
/// # Errors
///
/// Returns [`Refused`] when the manifest is missing, unreadable, or invalid.
pub fn read_one(dir: &Path) -> Result<Installed, Refused> {
    // An unreadable manifest is reported at the position a reader would stop
    // at, which for "there is no file" is the beginning.
    let text = std::fs::read_to_string(dir.join("mod.json"))
        .map_err(|_| Refused::Malformed { line: 1, column: 1 })?;
    let manifest = read_manifest(&text)?;

    let mut files = collect(dir, dir);
    let hash = content_hash(&mut files);

    Ok(Installed {
        manifest,
        hash,
        dir: dir.to_path_buf(),
    })
}

/// Every file under `dir`, with paths relative to `root`, in whatever order the
/// filesystem gives them — [`content_hash`] sorts.
fn collect(root: &Path, dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Runtime data is not part of the immutable, consented package.
        if dir == root && entry.file_name() == "dados" {
            continue;
        }
        if entry.file_type().is_ok_and(|kind| kind.is_symlink()) {
            continue;
        }
        if path.is_dir() {
            files.extend(collect(root, &path));
        } else if let (Ok(relative), Ok(bytes)) = (path.strip_prefix(root), std::fs::read(&path)) {
            files.push((relative.to_string_lossy().replace('\\', "/"), bytes));
        }
    }
    files
}

/// `author/name` from the path, or `None` if it is not two levels under `root`.
fn identifier_of(root: &Path, dir: &Path) -> Option<String> {
    let relative = dir.strip_prefix(root).ok()?;
    let mut parts = relative.components();
    let author = parts.next()?.as_os_str().to_str()?;
    let name = parts.next()?.as_os_str().to_str()?;
    if parts.next().is_some() {
        return None;
    }
    Some(format!("{author}/{name}"))
}

/// Um arquivo de mídia de um MOD, lido e pronto para uma janela montar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiaDeMod {
    /// O `data:` inteiro, com o tipo que os bytes provaram ser.
    pub uri: String,
    /// `som` ou `imagem`, de uma lista fechada.
    pub papel: &'static str,
    /// Quantos bytes o arquivo tem.
    pub bytes: usize,
}

/// **O teto por arquivo de mídia de um MOD.**
pub use seele_proto::midia_de_mod::TETO_DE_ARQUIVO as TETO_DE_MIDIA;

/// O que estes bytes são, **sem montar um `data:` deles**.
///
/// [`ler_midia`] serve para o que vai virar um elemento na hora: ela devolve o
/// `data:` inteiro, e para isso codifica tudo em base64. Um arquivo que uma
/// pessoa acabou de escolher pode ter dez megabytes e vai sair em pedaços —
/// codificá-lo inteiro para descobrir que ele é um PNG seria pagar treze
/// megabytes de texto por uma pergunta de oito bytes.
///
/// O teto não é conferido aqui: quem escolhe o arquivo tem o próprio, e ele é
/// maior que o da mídia de pacote.
#[must_use]
pub fn ler_tipo(bytes: &[u8]) -> Option<TipoLido> {
    let tipo = seele_proto::midia_de_mod::sniff(bytes)?;
    Some(TipoLido {
        media_type: tipo.media_type(),
        papel: tipo.papel(),
    })
}

/// O que os bytes provaram ser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TipoLido {
    /// `image/png`, `audio/wav`, e assim por diante.
    pub media_type: &'static str,
    /// `som` ou `imagem`.
    pub papel: &'static str,
}

/// Lê um arquivo de mídia de MOD, ou diz que não.
///
/// **Aqui, e não na casca.** A composição do `data:` é a mesma decisão do ADR
/// 0027 para anexos: uma casca que junta um tipo a bytes é uma casca que pode
/// juntar a alegação de quem mandou a eles. O manifesto de um MOD é
/// exatamente essa alegação, e por isso o tipo sai dos bytes e o `data:` sai
/// daqui inteiro.
///
/// Nada para o que passa do teto e para o que não é um formato conhecido: são
/// recusas diferentes para quem está escrevendo o MOD, e quem as separa é a
/// casca, que já conhece o tamanho.
#[must_use]
pub fn ler_midia(bytes: &[u8]) -> Option<MidiaDeMod> {
    if bytes.len() > TETO_DE_MIDIA {
        return None;
    }
    let tipo = seele_proto::midia_de_mod::sniff(bytes)?;
    // O mesmo codificador do `data:` de um anexo. Um segundo aqui seria um
    // segundo alfabeto para o mesmo RFC.
    let mut uri = String::with_capacity(bytes.len().div_ceil(3) * 4 + 32);
    uri.push_str("data:");
    uri.push_str(tipo.media_type());
    uri.push_str(";base64,");
    crate::preview::encode_base64_publico(bytes, &mut uri);
    Some(MidiaDeMod {
        uri,
        papel: tipo.papel(),
        bytes: bytes.len(),
    })
}

/// The name of a refusal, from a closed list.
///
/// Exists here and not in the shell for the reason above: the shell cannot name
/// `Refused`'s variants. ADR 0012 keeps the wording in the shell — this is an
/// identifier, not a sentence, and `ui/frases.js` turns it into one.
#[must_use]
pub fn refusal_name(why: &Refused) -> &'static str {
    match why {
        Refused::Malformed { .. } => "malformed",
        Refused::SchemaTooNew { .. } => "schema-too-new",
        Refused::ApiTooNew { .. } => "api-too-new",
        Refused::ApiTooOld { .. } => "api-too-old",
        Refused::MalformedId => "malformed-id",
        Refused::Empty => "empty",
        Refused::ArquivoInvalido { .. } => "arquivo-invalido",
        Refused::ArquivosDemais { .. } => "arquivos-demais",
    }
}

/// Lowercase hex of a content hash.
///
/// Here rather than in the shell because it is what a person compares by eye
/// against what the indexer publishes, and two spellings of the same hash would
/// make that comparison fail for no reason.
#[must_use]
pub fn hex(hash: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    hash.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

/// The identifier of an entry, however it turned out.
fn name_of(found: &Found) -> &str {
    match found {
        Found::Ok(installed) => &installed.manifest.id,
        Found::Refused { id, .. } => id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Um MOD de mentira em disco, sob um diretório temporário do teste.
    fn semear(raiz: &Path, id: &str, manifesto: &str) -> PathBuf {
        let dir = raiz.join("mods").join(id);
        std::fs::create_dir_all(dir.join("cliente")).expect("criar diretório");
        std::fs::write(dir.join("mod.json"), manifesto).expect("escrever manifesto");
        std::fs::write(dir.join("cliente/main.js"), "/* nada */").expect("escrever script");
        dir
    }

    /// A API vem da constante, e não escrita à mão.
    ///
    /// Escrita à mão ela era `1`, e passou a recusar no dia em que o ADR 0049
    /// fez a API ser uma só — estes testes são sobre ler pasta, conferir hash e
    /// nomear recusa, e nenhum deles é sobre a versão.
    fn manifesto(id: &str) -> String {
        let api = seele_proto::mods::MOD_API_VERSION;
        format!(
            r#"{{"schema":1,"id":"{id}","version":"1.0.0","api":{api},
                "repo":"https://example.invalid/x","reach":["dom"],
                "client":"cliente/main.js"}}"#
        )
    }

    fn temporario(nome: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("seele-mods-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("criar temporário");
        dir
    }

    #[test]
    fn um_mod_em_disco_e_lido_com_hash() {
        let raiz = temporario("um");
        semear(&raiz, "seele/exemplo", &manifesto("seele/exemplo"));

        let achados = list(&raiz);
        assert_eq!(achados.len(), 1);
        let Found::Ok(instalado) = &achados[0] else {
            panic!("MOD válido veio recusado");
        };
        assert_eq!(instalado.manifest.id, "seele/exemplo");
        assert_ne!(instalado.hash, [0u8; 32]);
    }

    /// O defeito mais caro deste repositório, segundo o CLAUDE.md, é «o produto
    /// sabe e não conta». Um MOD que não passa na validação não pode
    /// desaparecer da lista: quem o pôs ali precisa ver por quê.
    #[test]
    fn um_mod_invalido_aparece_recusado_em_vez_de_sumir() {
        let raiz = temporario("invalido");
        semear(&raiz, "seele/quebrado", "{ isto não é json");
        semear(&raiz, "seele/bom", &manifesto("seele/bom"));

        let achados = list(&raiz);
        assert_eq!(achados.len(), 2, "um dos dois sumiu da lista");
        assert!(achados.iter().any(|f| matches!(f, Found::Refused { .. })));
        assert!(achados.iter().any(|f| matches!(f, Found::Ok(_))));
    }

    /// Um `id` que não corresponde ao lugar onde o MOD está em disco deixaria
    /// dois MODs disputarem o mesmo diretório no `mod://`.
    #[test]
    fn um_id_que_nao_bate_com_o_diretorio_e_recusado() {
        let raiz = temporario("mentiroso");
        semear(&raiz, "seele/pasta", &manifesto("outro/nome"));

        let achados = list(&raiz);
        assert!(matches!(
            &achados[0],
            Found::Refused {
                why: Refused::MalformedId,
                ..
            }
        ));
    }

    #[test]
    fn um_diretorio_de_mods_que_nao_existe_da_lista_vazia() {
        let raiz = temporario("vazio");
        assert!(list(&raiz).is_empty());
    }

    /// Semeia um pacote endereçado pelo conteúdo: grava, lê o hash, renomeia.
    ///
    /// É o que o instalador faz, e por isso o teste o faz assim — escrever o
    /// hash à mão seria testar contra um número inventado em vez de contra a
    /// função que o produz.
    fn semear_por_conteudo(raiz: &Path, id: &str, extra: Option<(&str, &str)>) -> String {
        let provisorio = raiz.join("em-obras");
        let _ = std::fs::remove_dir_all(&provisorio);
        std::fs::create_dir_all(provisorio.join("cliente")).expect("criar");
        std::fs::write(provisorio.join("mod.json"), manifesto(id)).expect("manifesto");
        std::fs::write(provisorio.join("cliente/main.js"), "/* nada */").expect("script");
        if let Some((caminho, conteudo)) = extra {
            std::fs::write(provisorio.join(caminho), conteudo).expect("extra");
        }
        let lido = read_one(&provisorio).expect("ler");
        let hash = hex(&lido.hash);
        let destino = raiz.join(PACOTES).join(&hash);
        std::fs::create_dir_all(destino.parent().expect("pai")).expect("raiz dos pacotes");
        std::fs::rename(&provisorio, &destino).expect("publicar");
        hash
    }

    /// **Dois hashes do mesmo MOD cabem lado a lado.**
    ///
    /// É o P1 que este layout conserta: por identificador havia um lugar só, e
    /// atualizar para um servidor deixava o outro exigindo bytes que saíram do
    /// disco.
    #[test]
    fn dois_pacotes_do_mesmo_mod_convivem() {
        let raiz = temporario("dois-pacotes");
        let um = semear_por_conteudo(&raiz, "seele/exemplo", None);
        let outro = semear_por_conteudo(&raiz, "seele/exemplo", Some(("extra.txt", "outra coisa")));
        assert_ne!(um, outro, "os dois pacotes deram o mesmo hash");

        let achados = listar_por_conteudo(&raiz);
        assert_eq!(achados.len(), 2, "um dos dois sumiu");
        for achado in &achados {
            assert!(
                matches!(achado, Found::Ok(_)),
                "um pacote válido foi recusado: {achado:?}"
            );
        }
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// **O nome da pasta é conferível, e é conferido.**
    ///
    /// No layout por identificador o guarda era uma convenção — «o `id` do
    /// manifesto tem de bater com o caminho» —, e um manifesto trocado depois
    /// da instalação a quebrava. Aqui mexer num byte muda o hash, e o pacote
    /// deixa de ser o que a pasta diz que ele é.
    #[test]
    fn um_pacote_mexido_depois_de_instalado_e_recusado_e_nomeado() {
        let raiz = temporario("mexido");
        let hash = semear_por_conteudo(&raiz, "seele/exemplo", None);
        std::fs::write(
            raiz.join(PACOTES).join(&hash).join("cliente/main.js"),
            "/* outra coisa */",
        )
        .expect("mexer");

        let achados = listar_por_conteudo(&raiz);
        assert_eq!(achados.len(), 1);
        assert!(
            matches!(&achados[0], Found::Refused { .. }),
            "um pacote cujo conteúdo não bate com o nome da pasta foi servido \
             como se fosse o revisado: {:?}",
            achados[0]
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// E uma pasta que alguém criou à mão, sem manifesto, é dita e não ignorada.
    #[test]
    fn uma_pasta_sem_manifesto_aparece_recusada() {
        let raiz = temporario("sem-manifesto");
        std::fs::create_dir_all(raiz.join(PACOTES).join("nao-e-um-hash")).expect("criar");

        let achados = listar_por_conteudo(&raiz);
        assert_eq!(achados.len(), 1);
        assert!(
            matches!(&achados[0], Found::Refused { .. }),
            "{:?}",
            achados[0]
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    /// Sem raiz de pacotes não há pacote, e isso não é erro: é toda máquina
    /// antes da primeira instalação.
    #[test]
    fn sem_a_raiz_de_pacotes_a_lista_vem_vazia() {
        let raiz = temporario("vazia");
        assert!(listar_por_conteudo(&raiz).is_empty());
        let _ = std::fs::remove_dir_all(&raiz);
    }
}
