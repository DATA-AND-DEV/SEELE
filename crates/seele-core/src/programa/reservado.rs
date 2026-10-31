//! **O que roda é o que foi aceito** (revisão final da API 8, I1; decisão R1
//! de `2026-10-31-api-8-revisao-final-decisoes.md`).
//!
//! A decisão Q1 deixa um host de `origens` baixar qualquer arquivo com o hash
//! que o MOD declarar, e `escrever`, `trazer`, `desempacotar` e a cópia do
//! mundo gravam o que a metade de máquina pedir. Nada disso podia cair **sobre
//! o que o aceite roda**: antes, `baixar({url: <qualquer um do github>,
//! destino: "java/bin/java"})` trocava o Java aceito, e o `rodar` seguinte
//! passava em todas as conferências (o comando e os argumentos são os
//! aceitos) e executava um binário que o dono nunca viu.
//!
//! # A regra
//!
//! A política diz, sem depender de nada que aconteceu antes nesta máquina,
//! que caminhos o aceite prende:
//!
//! - **o comando** de `rodar` e o de `instalar`: quando ele mora numa pasta
//!   (`java/bin/java`), **a pasta de cima inteira** (`java/`) — o runtime
//!   carrega bibliotecas nativas ao lado do executável, e trocar
//!   `java/lib/libjvm.dylib` é o mesmo que trocar `java/bin/java`; quando ele
//!   mora na raiz, o arquivo, com e sem `.exe`;
//! - **cada argumento que é um caminho** (`server.jar`, e o `@arquivo` sem a
//!   arroba), como arquivo.
//!
//! Uma escrita que **não** traz os bytes de um artefato aceito não pode cair
//! sobre um caminho preso, dentro de uma pasta presa, nem sobre uma pasta que
//! contém um caminho preso (trocá-la inteira o levaria junto). Os bytes de um
//! artefato aceito (o download com o hash e o tamanho que a tela mostrou, ou
//! o pacote com esses bytes aberto) escrevem ali: é assim que o Java aceito
//! chega a `java/`.
//!
//! Sem estado: a regra sai só da política, então vale na primeira escrita e
//! depois de o app reabrir, e um `server.jar` plantado antes do aceito é
//! recusado do mesmo jeito. E o `rodar` só roda na raiz (a `pasta` presa):
//! com a pasta livre, `-jar server.jar` numa pasta escolhida pelo MOD rodaria
//! o `server.jar` que ele escreveu lá.
//!
//! # Comparação
//!
//! Pelo caminho **real** (o maior pedaço que existe é resolvido, então um
//! link dentro da raiz não leva a escrita para dentro de `java/`), sem
//! diferença de caixa e sem os pontos e espaços do fim de cada nome: o disco
//! do Mac e o do Windows não distinguem `Java/BIN/java` de `java/bin/java`, e
//! o Windows ignora `java.` e `java `.

use std::ffi::OsStr;
use std::path::{Component, Path};

use super::{Politica, Recusa};

/// Um caminho que o aceite prende.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Preso {
    /// O caminho como a política o escreve, relativo à raiz.
    texto: String,
    /// Os pedaços, já na chave de comparação.
    partes: Vec<String>,
    /// Tudo o que fica dentro também é preso.
    arvore: bool,
}

/// Como a escrita cai no caminho.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Escrita {
    /// Um arquivo gravado, ou uma pasta trocada inteira: não pode cair sobre
    /// um preso, dentro de uma pasta presa, nem sobre uma pasta que contém um
    /// preso.
    Troca,
    /// Uma pasta criada por cima (o `mesclar`): não apaga o que já está, então
    /// só não pode ser um preso nem ficar dentro de uma pasta presa.
    Pasta,
}

/// Os caminhos que o aceite prende.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Presos {
    itens: Vec<Preso>,
}

/// A chave de comparação de um pedaço de caminho.
fn chave(parte: &OsStr) -> String {
    parte
        .to_string_lossy()
        .trim_end_matches(['.', ' '])
        .to_lowercase()
}

/// Os pedaços de um caminho relativo como a política o escreve, ou `None`
/// se ele não é um caminho dentro da raiz (absoluto, com `..`, vazio).
fn pedacos(texto: &str) -> Option<Vec<String>> {
    if texto.starts_with(['/', '\\']) || texto.as_bytes().get(1) == Some(&b':') {
        return None;
    }
    let partes: Vec<String> = texto
        .split(['/', '\\'])
        .filter(|p| !p.is_empty() && *p != ".")
        .map(|p| chave(OsStr::new(p)))
        .collect();
    (!partes.is_empty() && !partes.iter().any(|p| p == ".." || p.is_empty())).then_some(partes)
}

/// Os pedaços **reais** de `alvo` sob `raiz`: o maior prefixo que existe é
/// resolvido (links, nomes curtos do Windows), e o resto é acrescentado como
/// está. `None` se o caminho real cai fora da raiz.
fn reais(raiz: &Path, alvo: &Path) -> Option<Vec<String>> {
    let mut existe = alvo.to_path_buf();
    let mut resto = Vec::new();
    while std::fs::symlink_metadata(&existe).is_err() {
        let (Some(nome), Some(pai)) = (existe.file_name(), existe.parent()) else {
            break;
        };
        resto.push(nome.to_owned());
        existe = pai.to_path_buf();
    }
    let mut real = std::fs::canonicalize(&existe).unwrap_or(existe);
    for nome in resto.iter().rev() {
        real.push(nome);
    }
    let relativo = real.strip_prefix(raiz).ok()?;
    relativo
        .components()
        .map(|c| match c {
            Component::Normal(nome) => Some(chave(nome)),
            _ => None,
        })
        .collect()
}

impl Presos {
    /// Os caminhos que a política prende.
    pub(crate) fn da_politica(politica: &Politica) -> Self {
        let mut itens: Vec<Preso> = Vec::new();
        let mut prender = |texto: &str, arvore: bool| {
            if let Some(partes) = pedacos(texto) {
                let preso = Preso {
                    texto: texto.to_owned(),
                    partes,
                    arvore,
                };
                if !itens.iter().any(|i| i.partes == preso.partes) {
                    itens.push(preso);
                }
            }
        };
        for comando in politica.comando.iter().chain(politica.instalar.iter()) {
            match pedacos(&comando.comando) {
                Some(partes) if partes.len() > 1 => {
                    let pasta = comando
                        .comando
                        .split(['/', '\\'])
                        .find(|p| !p.is_empty() && *p != ".")
                        .unwrap_or_default();
                    prender(pasta, true);
                }
                Some(_) => {
                    prender(&comando.comando, false);
                    prender(&format!("{}.exe", comando.comando), false);
                }
                None => {}
            }
            for argumento in &comando.argumentos {
                let caminho = argumento.strip_prefix('@').unwrap_or(argumento);
                if caminho.starts_with('-') {
                    continue;
                }
                prender(caminho, false);
            }
        }
        Self { itens }
    }

    /// Confere uma escrita que **não** traz os bytes de um artefato aceito.
    ///
    /// `alvo` é o caminho já resolvido por [`super::dentro_da_raiz`]; `texto`
    /// é como o MOD o pediu (vai na recusa).
    ///
    /// # Errors
    ///
    /// [`Recusa::ReservadoAoAceite`].
    pub(crate) fn conferir(
        &self,
        raiz: &Path,
        alvo: &Path,
        texto: &str,
        escrita: Escrita,
    ) -> Result<(), Recusa> {
        if self.itens.is_empty() {
            return Ok(());
        }
        // Fora da raiz não acontece (o caminho já passou pela régua); se
        // acontecer, vale o lado seguro: é a raiz, que contém tudo.
        let alvo = reais(raiz, alvo).unwrap_or_default();
        for preso in &self.itens {
            let real = reais(raiz, &raiz.join(preso.partes.join("/")))
                .unwrap_or_else(|| preso.partes.clone());
            let sobre = alvo == real;
            let dentro = preso.arvore && alvo.starts_with(&real);
            let contem = escrita == Escrita::Troca && real.starts_with(&alvo);
            if sobre || dentro || contem {
                return Err(Recusa::ReservadoAoAceite {
                    caminho: texto.to_owned(),
                    reservado: preso.texto.clone(),
                });
            }
        }
        Ok(())
    }
}

/// **Os bytes de `arquivo` são os de um artefato aceito?** O tamanho e o hash
/// do mesmo algoritmo da tela. Só calcula o hash quando o tamanho bate com o
/// de algum.
pub(crate) fn e_artefato_aceito(politica: &Politica, arquivo: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(arquivo) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    politica
        .artefatos
        .iter()
        .filter(|a| a.tamanho == meta.len())
        .any(|a| {
            super::baixar::hash_do_arquivo(arquivo, a.hash.algoritmo).is_some_and(
                |(tamanho, hex)| tamanho == a.tamanho && hex == a.hash.hex.to_ascii_lowercase(),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::programa::caminho::tests::tempfile_simples::Pasta;
    use crate::programa::ComandoAceito;

    fn politica(comando: &str, argumentos: &[&str]) -> Politica {
        Politica {
            dominios: vec![],
            artefatos: vec![],
            de_outros_sistemas: vec![],
            origens: vec![],
            comando: Some(ComandoAceito {
                comando: comando.to_owned(),
                argumentos: argumentos.iter().map(|a| (*a).to_owned()).collect(),
            }),
            instalar: None,
            pack: None,
            teto_de_disco: 0,
            portas_de_copia: vec![],
            raiz: std::path::PathBuf::new(),
        }
    }

    fn raiz() -> (Pasta, std::path::PathBuf) {
        let pasta = Pasta::nova("presos");
        let canonica = std::fs::canonicalize(pasta.caminho()).unwrap();
        (pasta, canonica)
    }

    #[test]
    fn o_comando_numa_pasta_prende_a_pasta_e_os_argumentos_que_sao_caminho() {
        let presos = Presos::da_politica(&politica(
            "java/bin/java",
            &[
                "-Xmx2G",
                "-jar",
                "server.jar",
                "@libraries/x/unix_args.txt",
                "nogui",
            ],
        ));
        let textos: Vec<(&str, bool)> = presos
            .itens
            .iter()
            .map(|p| (p.texto.as_str(), p.arvore))
            .collect();
        assert_eq!(
            textos,
            vec![
                ("java", true),
                ("server.jar", false),
                ("libraries/x/unix_args.txt", false),
                ("nogui", false),
            ]
        );
    }

    #[test]
    fn caixa_pontos_e_links_nao_escapam() {
        let (_p, raiz) = raiz();
        let presos = Presos::da_politica(&politica("java/bin/java", &["-jar", "server.jar"]));
        let conferir =
            |texto: &str, escrita| presos.conferir(&raiz, &raiz.join(texto), texto, escrita);
        for texto in [
            "java/bin/java",
            "Java/BIN/java",
            "java./lib/libjvm.dylib",
            "java",
            "SERVER.JAR",
            "server.jar ",
        ] {
            assert!(
                matches!(
                    conferir(texto, Escrita::Troca),
                    Err(Recusa::ReservadoAoAceite { .. })
                ),
                "{texto}"
            );
        }
        // Uma pasta que contém um preso não é trocada; por cima, ela pode
        // ser criada (o `mesclar` não apaga o que está).
        std::fs::create_dir_all(raiz.join("libraries")).unwrap();
        let presos_args =
            Presos::da_politica(&politica("java/bin/java", &["@libraries/a/args.txt"]));
        assert!(presos_args
            .conferir(&raiz, &raiz.join("libraries"), "libraries", Escrita::Troca)
            .is_err());
        assert!(presos_args
            .conferir(&raiz, &raiz.join("libraries"), "libraries", Escrita::Pasta)
            .is_ok());
        // O que não é preso passa.
        for texto in [
            "mods/x.jar",
            "server.properties",
            "javax/a",
            "world/level.dat",
        ] {
            assert!(conferir(texto, Escrita::Troca).is_ok(), "{texto}");
        }
        // Um link dentro da raiz não leva a escrita para dentro de `java/`.
        std::fs::create_dir_all(raiz.join("java/bin")).unwrap();
        if crate::programa::caminho::tests::criar_link(&raiz.join("java"), &raiz.join("atalho")) {
            assert!(matches!(
                conferir("atalho/bin/java", Escrita::Troca),
                Err(Recusa::ReservadoAoAceite { .. })
            ));
        }
    }

    #[test]
    fn o_comando_na_raiz_prende_o_arquivo_com_e_sem_exe() {
        let (_p, raiz) = raiz();
        let presos = Presos::da_politica(&politica("bedrock_server", &[]));
        for texto in ["bedrock_server", "bedrock_server.exe", "BEDROCK_SERVER.EXE"] {
            assert!(presos
                .conferir(&raiz, &raiz.join(texto), texto, Escrita::Troca)
                .is_err());
        }
        assert!(presos
            .conferir(&raiz, &raiz.join("worlds/a"), "worlds/a", Escrita::Troca)
            .is_ok());
    }
}
