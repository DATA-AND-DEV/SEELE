//! **A política de um `Programa` sai do aceite guardado, e só dele** (API 8
//! parte 4). A cola do app lê o [`AceiteDeMaquina`] que o dono deu nesta
//! máquina e monta daqui o que o `programa` deixa fazer; nada do que a metade
//! de máquina pede em tempo de execução entra na política.
//!
//! [`AceiteDeMaquina`]: crate::aceites_de_maquina::AceiteDeMaquina

use std::path::PathBuf;

use seele_proto::maquina::{artefatos_para, AlgoritmoDeHash as AlgoritmoDoFio, OfertaDeMaquina};

use super::{AlgoritmoDeHash, ArtefatoAceito, ComandoAceito, HashDeclarado, Politica};

/// **O sistema desta máquina**, no nome das receitas
/// ([`seele_proto::maquina::PLATAFORMAS_DE_MAQUINA`]), ou `None` num sistema
/// que nenhuma receita conhece.
///
/// Do alvo do build, e não perguntado ao sistema: o Java que o app baixa tem
/// de rodar ao lado dele, e um app Intel rodando pelo Rosetta num Mac ARM
/// ainda é um processo Intel (e o Java de Intel roda ali).
#[must_use]
pub const fn plataforma_desta_maquina() -> Option<&'static str> {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some("windows-x64")
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("mac-aarch64")
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some("mac-x64")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("linux-x64")
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some("linux-aarch64")
    } else {
        None
    }
}

const fn algoritmo(a: AlgoritmoDoFio) -> AlgoritmoDeHash {
    match a {
        AlgoritmoDoFio::Sha1 => AlgoritmoDeHash::Sha1,
        AlgoritmoDoFio::Sha256 => AlgoritmoDeHash::Sha256,
        AlgoritmoDoFio::Sha512 => AlgoritmoDeHash::Sha512,
    }
}

/// O host de uma URL, sem porta; `None` se ela não se lê.
fn host_de(url: &str) -> Option<String> {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
}

/// **A política do que foi aceito**, para uma máquina de `plataforma`:
///
/// - `dominios`: os hosts das URLs de **todos** os artefatos, mais as origens
///   (o que a tela mostrou como «de onde»);
/// - `artefatos`: os desta máquina ([`artefatos_para`]), cada um preso ao
///   hash e ao tamanho mostrados; os de outros sistemas, em
///   `de_outros_sistemas`, recusados pelo nome;
/// - `origens`: as da oferta, de onde vale uma URL que não é artefato;
/// - `comando`: o `rodar` da oferta, com os argumentos exatos;
/// - `teto_de_disco`: o `disco_maximo`;
/// - `raiz`: a pasta do servidor de jogo, que quem chama tira de
///   [`crate::aceites_de_maquina::pasta_do_servidor_de_jogo`].
#[must_use]
pub fn politica_do_aceite(oferta: &OfertaDeMaquina, raiz: PathBuf, plataforma: &str) -> Politica {
    let mut dominios: Vec<String> = Vec::new();
    for host in oferta
        .artefatos
        .iter()
        .filter_map(|a| host_de(&a.url))
        .chain(oferta.origens.iter().cloned())
    {
        if !dominios.contains(&host) {
            dominios.push(host);
        }
    }
    let (daqui, _) = artefatos_para(oferta, plataforma);
    let artefatos: Vec<ArtefatoAceito> = daqui
        .iter()
        .map(|a| ArtefatoAceito {
            url: a.url.clone(),
            hash: HashDeclarado {
                algoritmo: algoritmo(a.hash.algoritmo),
                hex: a.hash.hex.clone(),
            },
            tamanho: a.tamanho,
        })
        .collect();
    let de_outros_sistemas = oferta
        .artefatos
        .iter()
        .filter(|a| !a.serve_em(plataforma))
        .map(|a| a.url.clone())
        // A mesma URL nos dois lados (um artefato marcado para este sistema
        // e outro, igual, para outro) é deste: o aceito vale.
        .filter(|url| !artefatos.iter().any(|a| a.url == *url))
        .collect();
    Politica {
        dominios,
        artefatos,
        de_outros_sistemas,
        origens: oferta.origens.clone(),
        comando: oferta.rodar.as_ref().map(|r| ComandoAceito {
            comando: r.comando.clone(),
            argumentos: r.argumentos.clone(),
        }),
        teto_de_disco: oferta.disco_maximo,
        raiz,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use seele_proto::maquina::{ArtefatoOferecido, ComandoOferecido, HashDeArtefato};

    fn artefato(nome: &str, url: &str, plataformas: &[&str]) -> ArtefatoOferecido {
        ArtefatoOferecido {
            nome: nome.into(),
            url: url.into(),
            tamanho: 7,
            hash: HashDeArtefato {
                algoritmo: AlgoritmoDoFio::Sha256,
                hex: "c".repeat(64),
            },
            plataformas: plataformas.iter().map(|p| (*p).to_owned()).collect(),
        }
    }

    #[test]
    fn a_politica_sai_inteira_do_aceite() {
        let oferta = OfertaDeMaquina {
            titulo: "Minecraft".into(),
            receita: "minecraft".into(),
            versao_da_receita: "1".into(),
            versao_do_jogo: "1.21.4".into(),
            tuneis: vec![],
            artefatos: vec![
                artefato("server.jar", "https://piston-data.mojang.com/s.jar", &[]),
                artefato(
                    "java-mac",
                    "https://github.com/a/mac.tar.gz",
                    &["mac-aarch64"],
                ),
                artefato("java-win", "https://github.com/a/win.zip", &["windows-x64"]),
            ],
            pack: None,
            origens: vec!["release-assets.githubusercontent.com".into()],
            dominios_do_jogo: vec!["sessionserver.mojang.com".into()],
            rodar: Some(ComandoOferecido {
                comando: "java/bin/java".into(),
                argumentos: vec!["-jar".into(), "server.jar".into()],
            }),
            disco_maximo: 1234,
            termos: vec![],
        };
        let p = politica_do_aceite(&oferta, PathBuf::from("/r"), "mac-aarch64");
        assert_eq!(
            p.dominios,
            vec![
                "piston-data.mojang.com".to_owned(),
                "github.com".to_owned(),
                "release-assets.githubusercontent.com".to_owned()
            ]
        );
        assert_eq!(
            p.artefatos
                .iter()
                .map(|a| a.url.as_str())
                .collect::<Vec<_>>(),
            vec![
                "https://piston-data.mojang.com/s.jar",
                "https://github.com/a/mac.tar.gz"
            ]
        );
        assert_eq!(
            p.de_outros_sistemas,
            vec!["https://github.com/a/win.zip".to_owned()]
        );
        assert_eq!(
            p.origens,
            vec!["release-assets.githubusercontent.com".to_owned()]
        );
        assert_eq!(
            p.comando,
            Some(ComandoAceito {
                comando: "java/bin/java".into(),
                argumentos: vec!["-jar".into(), "server.jar".into()],
            })
        );
        assert_eq!(p.teto_de_disco, 1234);
        assert_eq!(p.raiz, PathBuf::from("/r"));
        // Os domínios do jogo são do jogo, e não do `programa`.
        assert!(!p.dominios.iter().any(|d| d.contains("sessionserver")));
    }

    #[test]
    fn esta_maquina_tem_nome_de_receita() {
        let nome = plataforma_desta_maquina().expect("o Mac e o Windows têm nome");
        assert!(seele_proto::maquina::PLATAFORMAS_DE_MAQUINA.contains(&nome));
    }
}
