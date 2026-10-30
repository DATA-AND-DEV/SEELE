//! A pasta de um servidor de jogo, e nada fora dela.
//!
//! O `destino` de um download, o comando e a pasta de trabalho de um processo
//! são todos **relativos à raiz** do servidor de jogo, dentro da área do MOD
//! (spec da API 8, §5: «caminho absoluto e `..` são recusados, como no
//! `dados`»). A regra de cada pedaço é a mesma do `dados` e da janela
//! ([`seele_proto::mods::inner_path`]); o que este módulo soma é o que o
//! `dados` não precisa: **o disco aqui é de um programa que roda**, e um
//! programa cria links. Um `server/mundo -> /Users/fulano` criado pelo jogo
//! faria um `destino: "server/mundo/.ssh/x"` sair da raiz sem `..` nenhum.
//!
//! # O que não fecha
//!
//! Conferir e depois abrir tem uma janela: um link criado entre as duas
//! passa. Quem cria links ali é o próprio processo do jogo, que roda com a
//! conta da pessoa e já lê o que ela lê (spec §5, «avisar e isolar depois»);
//! a conferência protege contra o caminho que o MOD escreve, e não contra o
//! programa que o MOD roda.

use std::path::{Path, PathBuf};

use super::Recusa;

/// Resolve um caminho relativo dentro da raiz, ou a recusa com o motivo.
///
/// `vazio_e_a_raiz` diz se `""` vale a própria raiz (a pasta de trabalho de
/// `rodar`) ou é recusado (o `destino` de `baixar`, que precisa de um nome).
///
/// `raiz` tem de ser canônica (`std::fs::canonicalize`): é contra ela que o
/// alvo real de um link é comparado.
///
/// # Errors
///
/// [`Recusa::CaminhoAbsoluto`], [`Recusa::CaminhoComPai`],
/// [`Recusa::CaminhoVazio`], [`Recusa::CaminhoInvalido`] e
/// [`Recusa::LinkSaiDaRaiz`].
pub fn dentro_da_raiz(raiz: &Path, caminho: &str, vazio_e_a_raiz: bool) -> Result<PathBuf, Recusa> {
    let recusa_absoluto = || Recusa::CaminhoAbsoluto {
        caminho: caminho.to_owned(),
    };
    // Absoluto em qualquer sistema, e não só no de quem roda: um MOD escrito no
    // Mac e rodado no Windows tem de ouvir a mesma recusa. `C:` é a letra de
    // unidade; `\\servidor\x` é UNC; `/x` e `\x` são a raiz do disco atual.
    let bytes = caminho.as_bytes();
    if caminho.starts_with('/')
        || caminho.starts_with('\\')
        || (bytes.len() >= 2 && bytes.get(1) == Some(&b':'))
        || Path::new(caminho).is_absolute()
    {
        return Err(recusa_absoluto());
    }
    // As duas barras separam, nos dois sistemas: `a\..\b` é `..` no Windows, e
    // recusá-lo também no Mac é o que faz a recusa ser a mesma nos dois.
    let partes: Vec<&str> = caminho
        .split(['/', '\\'])
        .filter(|p| !p.is_empty())
        .collect();
    if partes.contains(&"..") {
        return Err(Recusa::CaminhoComPai {
            caminho: caminho.to_owned(),
        });
    }
    if partes.is_empty() {
        return if vazio_e_a_raiz {
            Ok(raiz.to_path_buf())
        } else {
            Err(Recusa::CaminhoVazio)
        };
    }
    let relativo =
        seele_proto::mods::inner_path(&partes).ok_or_else(|| Recusa::CaminhoInvalido {
            caminho: caminho.to_owned(),
        })?;
    let alvo = raiz.join(relativo);
    conferir_links(raiz, &alvo, caminho)?;
    Ok(alvo)
}

/// Confere que nenhum pedaço já existente do caminho é um link que sai da raiz.
///
/// Cada prefixo que existe é olhado **sem seguir** (`symlink_metadata`); um que
/// é link é resolvido inteiro (`canonicalize`) e comparado com a raiz. Um link
/// que não resolve (o alvo não existe) é recusado também: não dá para dizer
/// onde ele cai, e criar o alvo depois o faria cair fora.
fn conferir_links(raiz: &Path, alvo: &Path, caminho: &str) -> Result<(), Recusa> {
    let Ok(resto) = alvo.strip_prefix(raiz) else {
        return Err(Recusa::CaminhoInvalido {
            caminho: caminho.to_owned(),
        });
    };
    let mut atual = raiz.to_path_buf();
    for pedaco in resto.components() {
        atual.push(pedaco);
        let Ok(meta) = std::fs::symlink_metadata(&atual) else {
            // Daqui para a frente nada existe, e o que não existe não é link.
            return Ok(());
        };
        if meta.file_type().is_symlink() {
            match std::fs::canonicalize(&atual) {
                Ok(real) if real.starts_with(raiz) => {}
                Ok(real) => {
                    return Err(Recusa::LinkSaiDaRaiz {
                        caminho: caminho.to_owned(),
                        aponta_para: real,
                    })
                }
                Err(_) => {
                    return Err(Recusa::LinkSaiDaRaiz {
                        caminho: caminho.to_owned(),
                        aponta_para: std::fs::read_link(&atual).unwrap_or_default(),
                    })
                }
            }
        }
    }
    Ok(())
}

/// Quanto a raiz ocupa, em bytes: a soma dos arquivos, sem seguir link.
///
/// Os arquivos parciais de downloads em andamento ficam fora (`parcial`
/// devolve `true` para eles): quem chama já os conta pelo tamanho declarado,
/// e contá-los duas vezes recusaria um download que cabe.
pub(crate) fn ocupado(raiz: &Path, parcial: &dyn Fn(&Path) -> bool) -> u64 {
    let mut total = 0_u64;
    let mut pendentes = vec![raiz.to_path_buf()];
    while let Some(pasta) = pendentes.pop() {
        let Ok(entradas) = std::fs::read_dir(&pasta) else {
            continue;
        };
        for entrada in entradas.flatten() {
            let caminho = entrada.path();
            let Ok(meta) = std::fs::symlink_metadata(&caminho) else {
                continue;
            };
            if meta.is_dir() {
                pendentes.push(caminho);
            } else if meta.is_file() && !parcial(&caminho) {
                total = total.saturating_add(meta.len());
            }
        }
    }
    total
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn raiz() -> (tempfile_simples::Pasta, PathBuf) {
        let pasta = tempfile_simples::Pasta::nova("caminho");
        let canonica = std::fs::canonicalize(pasta.caminho()).unwrap();
        (pasta, canonica)
    }

    /// Uma pasta temporária sem crate novo: o `tempfile` não está na árvore
    /// normal deste crate, e um teste não vale uma aresta.
    pub(crate) mod tempfile_simples {
        use std::path::{Path, PathBuf};
        use std::sync::atomic::{AtomicU64, Ordering};

        static PROXIMA: AtomicU64 = AtomicU64::new(0);

        pub(crate) struct Pasta(PathBuf);

        impl Pasta {
            pub(crate) fn nova(nome: &str) -> Self {
                let n = PROXIMA.fetch_add(1, Ordering::Relaxed);
                let caminho = std::env::temp_dir()
                    .join(format!("seele-programa-{nome}-{}-{n}", std::process::id()));
                let _ = std::fs::remove_dir_all(&caminho);
                std::fs::create_dir_all(&caminho).unwrap();
                Self(caminho)
            }
            pub(crate) fn caminho(&self) -> &Path {
                &self.0
            }
        }

        impl Drop for Pasta {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    #[test]
    fn relativo_fica_dentro() {
        let (_p, raiz) = raiz();
        assert_eq!(
            dentro_da_raiz(&raiz, "server/mods/x.jar", false).unwrap(),
            raiz.join("server").join("mods").join("x.jar")
        );
        assert_eq!(dentro_da_raiz(&raiz, "", true).unwrap(), raiz);
    }

    #[test]
    fn absoluto_e_recusado_em_qualquer_grafia() {
        let (_p, raiz) = raiz();
        for caminho in [
            "/etc/passwd",
            "\\Windows\\x",
            "C:\\x",
            "c:/x",
            "\\\\srv\\share",
        ] {
            assert!(
                matches!(
                    dentro_da_raiz(&raiz, caminho, true),
                    Err(Recusa::CaminhoAbsoluto { .. })
                ),
                "{caminho}"
            );
        }
    }

    #[test]
    fn pai_e_recusado_em_qualquer_posicao_e_com_qualquer_barra() {
        let (_p, raiz) = raiz();
        for caminho in [
            "..",
            "../fora",
            "server/../../fora",
            "server\\..\\..\\fora",
            "a/..",
        ] {
            assert!(
                matches!(
                    dentro_da_raiz(&raiz, caminho, true),
                    Err(Recusa::CaminhoComPai { .. })
                ),
                "{caminho}"
            );
        }
    }

    #[test]
    fn vazio_so_vale_quando_e_a_raiz() {
        let (_p, raiz) = raiz();
        assert!(matches!(
            dentro_da_raiz(&raiz, "", false),
            Err(Recusa::CaminhoVazio)
        ));
        // `//` começa por barra: é absoluto antes de ser vazio.
        assert!(matches!(
            dentro_da_raiz(&raiz, "//", false),
            Err(Recusa::CaminhoAbsoluto { .. })
        ));
    }

    #[test]
    fn ponto_sozinho_e_invalido() {
        let (_p, raiz) = raiz();
        assert!(matches!(
            dentro_da_raiz(&raiz, "server/./x", false),
            Err(Recusa::CaminhoInvalido { .. })
        ));
    }

    /// Um link para fora, criado dentro da raiz, não leva o caminho junto.
    ///
    /// Só onde criar link não pede privilégio: no Windows, `symlink_dir` exige
    /// o modo de desenvolvedor ou administrador, e o teste diz que pulou em
    /// vez de passar calado.
    #[test]
    fn link_que_sai_da_raiz_e_recusado() {
        let (_p, raiz) = raiz();
        let (_f, fora) = raiz_fora();
        let link = raiz.join("mundo");
        if !criar_link(&fora, &link) {
            eprintln!("PULADO: este sistema não deixou criar link sem privilégio");
            return;
        }
        let recusa = dentro_da_raiz(&raiz, "mundo/.ssh/id", false).unwrap_err();
        assert!(
            matches!(&recusa, Recusa::LinkSaiDaRaiz { aponta_para, .. } if aponta_para == &fora),
            "{recusa:?}"
        );
        // E um link que fica dentro passa.
        std::fs::create_dir_all(raiz.join("real")).unwrap();
        assert!(criar_link(&raiz.join("real"), &raiz.join("atalho")));
        assert!(dentro_da_raiz(&raiz, "atalho/x", false).is_ok());
    }

    fn raiz_fora() -> (tempfile_simples::Pasta, PathBuf) {
        raiz()
    }

    pub(crate) fn criar_link(alvo: &Path, link: &Path) -> bool {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(alvo, link).is_ok()
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_dir(alvo, link).is_ok()
        }
    }

    #[test]
    fn o_ocupado_soma_arquivos_e_deixa_os_parciais_fora() {
        let (_p, raiz) = raiz();
        std::fs::create_dir_all(raiz.join("a/b")).unwrap();
        std::fs::write(raiz.join("a/um"), [0; 10]).unwrap();
        std::fs::write(raiz.join("a/b/dois"), [0; 32]).unwrap();
        std::fs::write(raiz.join("a/b/tres.parcial"), [0; 1000]).unwrap();
        let parcial = |c: &Path| c.extension().is_some_and(|e| e == "parcial");
        assert_eq!(ocupado(&raiz, &parcial), 42);
    }
}
