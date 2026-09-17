//! O caminho do executável dentro de uma instalação, e por que ele também não
//! é uma `String`.
//!
//! O identificador de versão ganhou um tipo próprio ([`crate::versao`]) porque
//! ele vira pedaço de caminho em disco. O campo `executable` do manifesto vira
//! **exatamente a mesma coisa**, e por um caminho mais curto: ele é o último
//! `join` antes de o launcher iniciar um processo.
//!
//! Dois valores bastam para que a versão escolhida deixe de decidir o que roda:
//!
//! - `"/bin/sh"` — `Path::join` com caminho absoluto **descarta a base**, e a
//!   instalação inteira some da conta;
//! - `"../1.0.0/bin/seele"` — a versão pedida resolve para o binário de outra
//!   versão instalada.
//!
//! # A régua é a do sistema mais estrito, em todo sistema
//!
//! O manifesto é **um só** para as três máquinas, e o alvo (`darwin-aarch64`,
//! `windows-x86_64`) é escolhido por quem instala, não por quem publica: um
//! `executable` que só o Mac aceita passa aqui, é instalado, e falha ao abrir
//! na máquina de outra pessoa — que é o defeito mais caro deste repositório.
//!
//! Por isso a contrabarra é separador em todo sistema, e por isso os nomes que
//! o Windows não cria — `CON` e os outros dispositivos, `a<b>c|d`, um
//! componente terminado em ponto ou espaço — são recusados também no Mac. É a
//! mesma decisão, e a mesma lista, de [`crate::versao`].
//!
//! # Por que a conferência é aqui, e não na desserialização
//!
//! A [`crate::versao::Versao`] confere ao desserializar, e um identificador
//! inválido derruba o manifesto inteiro. Aqui não: um `executable` errado num
//! pacote de um sistema **não pode** trancar as outras versões, que é a promessa
//! do ADR 0046. Então a recusa é por pacote, com motivo próprio, e o campo cru é
//! privado — não há como chegar ao caminho sem passar por [`CaminhoDoExecutavel::novo`].
//!
//! # O que esta conferência não cobre
//!
//! O **conteúdo** do pacote. Uma ligação simbólica dentro da instalação aponta
//! para onde quem a criou quis, e quem a criou assinou o pacote — é a
//! assinatura do ADR 0026 que responde por isso. O que se confere aqui é o que
//! o manifesto diz, e o manifesto é, por decisão registrada em
//! `docs/versoes-lado-a-lado.md`, **não assinado**.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::fmt;
use std::path::PathBuf;

/// Quantos caracteres o caminho declarado pode ter.
///
/// `SEELE.app/Contents/MacOS/SEELE` tem trinta. O teto existe porque um caminho
/// longo demais falha na abertura, com a frase do sistema, em vez de aqui.
const TETO: usize = 256;

/// Os caracteres que não podem estar num nome de arquivo do Windows.
///
/// A barra e a contrabarra não estão aqui porque elas são **separadores**, e
/// separar é o que se espera delas. O que está aqui é o que nenhum nome pode
/// conter em lugar nenhum do caminho.
const PROIBIDOS: &[char] = &[':', '<', '>', '"', '|', '?', '*'];

/// Por que o executável declarado não serve.
///
/// Enum e não frase, pela regra de `specs/02-protocolo.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutavelInvalido {
    /// O manifesto não diz qual é o executável deste pacote.
    ///
    /// É o estado do manifesto publicado hoje — ver a lacuna 3 de
    /// `docs/versoes-lado-a-lado.md`. Não é caminho inválido: é caminho
    /// nenhum, e a coisa a fazer é publicar o campo.
    NaoDeclarado,
    /// Declarado, e vazio.
    Vazio,
    /// Passou de [`TETO`] caracteres.
    LongoDemais {
        /// Quantos vieram.
        caracteres: usize,
    },
    /// Começa na raiz de um volume, e `join` descartaria a instalação.
    Absoluto {
        /// O que veio.
        caminho: String,
    },
    /// Tem um componente `.` ou `..`: sai da pasta da versão escolhida.
    Travessia {
        /// O que veio.
        caminho: String,
    },
    /// Tem um componente vazio — `bin//seele`, ou termina em separador.
    ComponenteVazio {
        /// O que veio.
        caminho: String,
    },
    /// Tem um caractere que não pode estar num nome de arquivo.
    ///
    /// Os dois pontos estão aqui e não é exagero: no Windows eles separam o
    /// volume (`C:\`) e abrem fluxo alternativo de dados (`seele.exe:outro`),
    /// que é um segundo conteúdo dentro do mesmo nome. Os outros — `<`, `>`,
    /// `"`, `|`, `?` e `*` — são os que o Windows recusa em qualquer nome de
    /// arquivo, e são recusados aqui **em todo sistema**, pela mesma razão que
    /// a contrabarra é separador em todo sistema: o manifesto é um só para as
    /// três máquinas.
    CaractereProibido {
        /// O primeiro que não serviu.
        caractere: char,
    },
    /// Um componente bate com um nome de dispositivo reservado pelo Windows.
    ///
    /// `bin/aux` não é uma pasta com um programa dentro: `aux` é um
    /// dispositivo, e a instalação falha ao ser criada. A lista é a mesma de
    /// [`crate::versao`], e é uma só de propósito — ver
    /// [`crate::versao::nome_reservado`].
    NomeReservado {
        /// Qual deles.
        nome: String,
    },
    /// Um componente termina em ponto ou em espaço.
    ///
    /// O Windows corta os dois ao abrir: `seele.exe.` e `seele.exe ` abrem
    /// `seele.exe`, e um nome assim não pode sequer ser criado por lá. Um
    /// pacote que declarasse isso instalaria no Mac e abriria outro arquivo no
    /// Windows — ou nenhum.
    FinalInvalido {
        /// O componente que terminou assim.
        componente: String,
    },
}

/// O caminho do executável dentro da instalação, já conferido.
///
/// Relativo, sem travessia, e sem `Ord` — não há ordem entre caminhos que
/// signifique alguma coisa aqui.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CaminhoDoExecutavel(String);

impl CaminhoDoExecutavel {
    /// Confere o que o manifesto declarou, ou diz por que não serve.
    ///
    /// # Errors
    ///
    /// [`ExecutavelInvalido`], uma variante por motivo.
    pub fn novo(bruto: &str) -> Result<Self, ExecutavelInvalido> {
        if bruto.is_empty() {
            return Err(ExecutavelInvalido::Vazio);
        }
        let caracteres = bruto.chars().count();
        if caracteres > TETO {
            return Err(ExecutavelInvalido::LongoDemais { caracteres });
        }
        if let Some(caractere) = bruto
            .chars()
            .find(|c| PROIBIDOS.contains(c) || c.is_control())
        {
            return Err(ExecutavelInvalido::CaractereProibido { caractere });
        }
        // As duas barras, em todo sistema. Um `..\\x` no Mac é um nome de
        // arquivo e não uma travessia — mas o manifesto é o mesmo para as três
        // máquinas, e recusar só onde dói faz a falha aparecer na máquina de
        // outra pessoa. Mesmo argumento dos nomes reservados em `versao.rs`.
        if bruto.starts_with('/') || bruto.starts_with('\\') {
            return Err(ExecutavelInvalido::Absoluto {
                caminho: bruto.to_owned(),
            });
        }
        for componente in bruto.split(['/', '\\']) {
            if componente.is_empty() {
                return Err(ExecutavelInvalido::ComponenteVazio {
                    caminho: bruto.to_owned(),
                });
            }
            if componente == "." || componente == ".." {
                return Err(ExecutavelInvalido::Travessia {
                    caminho: bruto.to_owned(),
                });
            }
            // Depois da travessia, porque `.` e `..` também terminam em ponto
            // e o motivo certo para eles é o outro.
            if componente.ends_with('.') || componente.ends_with(' ') {
                return Err(ExecutavelInvalido::FinalInvalido {
                    componente: componente.to_owned(),
                });
            }
            if let Some(nome) = crate::versao::nome_reservado(componente) {
                return Err(ExecutavelInvalido::NomeReservado { nome });
            }
        }
        Ok(Self(bruto.to_owned()))
    }

    /// O caminho como o manifesto o escreveu.
    #[must_use]
    pub fn como_texto(&self) -> &str {
        &self.0
    }

    /// O caminho relativo, montado componente a componente.
    ///
    /// Componente a componente, e não `PathBuf::from`: é o que faz um
    /// `bin\seele` declarado para o Windows virar `bin/seele` no Mac em vez de
    /// um arquivo só com uma contrabarra no nome. Cada componente já passou
    /// pela conferência, então `raiz.join(isto)` fica dentro de `raiz`.
    #[must_use]
    pub fn como_caminho(&self) -> PathBuf {
        self.0
            .split(['/', '\\'])
            .fold(PathBuf::new(), |caminho, parte| caminho.join(parte))
    }
}

impl fmt::Display for CaminhoDoExecutavel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::path::{Component, Path};

    /// Os caminhos que os três sistemas usam de verdade.
    #[test]
    fn os_caminhos_que_os_tres_sistemas_declaram_passam() {
        for texto in [
            "SEELE.app/Contents/MacOS/SEELE",
            "SEELE.exe",
            "bin/seele",
            "bin\\seele.exe",
            "..oculto/seele",
        ] {
            assert!(
                CaminhoDoExecutavel::novo(texto).is_ok(),
                "recusou `{texto}`"
            );
        }
    }

    /// **O guarda que justifica o tipo existir.** Nenhum destes vira caminho —
    /// e os dois primeiros são os que faziam quem serve o manifesto, e não a
    /// versão selecionada, decidir o programa iniciado.
    #[test]
    fn nenhum_caminho_sai_da_pasta_da_versao() {
        for texto in [
            "/bin/sh",
            "/usr/bin/env",
            "\\Windows\\System32\\cmd.exe",
            "C:\\Windows\\System32\\cmd.exe",
            "../1.0.0-teste/bin/seele",
            "..\\1.0.0-teste\\bin\\seele",
            "bin/../../fuga",
            "./bin/seele",
            "..",
            ".",
            "bin/",
            "bin//seele",
            "seele.exe:fluxo",
            "",
        ] {
            assert!(
                CaminhoDoExecutavel::novo(texto).is_err(),
                "aceitou `{texto}`"
            );
        }
    }

    /// A propriedade, e não a lista: o que passa, montado sobre uma raiz,
    /// continua dentro dela e não tem componente de subida.
    #[test]
    fn todo_caminho_aceito_fica_dentro_da_instalacao() {
        let raiz = Path::new("/tmp/versoes/1.0.0-teste");
        for texto in [
            "SEELE.app/Contents/MacOS/SEELE",
            "SEELE.exe",
            "bin\\seele.exe",
            "a/b/c/d/e",
            "..oculto/seele",
            "-comeco-com-hifen",
        ] {
            let Ok(caminho) = CaminhoDoExecutavel::novo(texto) else {
                continue;
            };
            let montado = raiz.join(caminho.como_caminho());
            assert!(montado.starts_with(raiz), "`{texto}` saiu da raiz");
            assert!(
                !caminho
                    .como_caminho()
                    .components()
                    .any(|c| matches!(c, Component::ParentDir | Component::RootDir)),
                "`{texto}` tem componente de subida"
            );
        }
    }

    #[test]
    fn o_motivo_da_recusa_e_especifico_e_nao_generico() {
        assert_eq!(
            CaminhoDoExecutavel::novo("/bin/sh"),
            Err(ExecutavelInvalido::Absoluto {
                caminho: "/bin/sh".to_owned()
            })
        );
        assert_eq!(
            CaminhoDoExecutavel::novo("../outra/bin"),
            Err(ExecutavelInvalido::Travessia {
                caminho: "../outra/bin".to_owned()
            })
        );
        assert_eq!(
            CaminhoDoExecutavel::novo("bin//seele"),
            Err(ExecutavelInvalido::ComponenteVazio {
                caminho: "bin//seele".to_owned()
            })
        );
        assert_eq!(
            CaminhoDoExecutavel::novo("C:\\x"),
            Err(ExecutavelInvalido::CaractereProibido { caractere: ':' })
        );
        assert_eq!(
            CaminhoDoExecutavel::novo("bin/aux"),
            Err(ExecutavelInvalido::NomeReservado {
                nome: "aux".to_owned()
            })
        );
        assert_eq!(
            CaminhoDoExecutavel::novo("seele.exe."),
            Err(ExecutavelInvalido::FinalInvalido {
                componente: "seele.exe.".to_owned()
            })
        );
        assert_eq!(
            CaminhoDoExecutavel::novo(""),
            Err(ExecutavelInvalido::Vazio)
        );
        let comprido = "a".repeat(TETO + 1);
        assert_eq!(
            CaminhoDoExecutavel::novo(&comprido),
            Err(ExecutavelInvalido::LongoDemais {
                caracteres: TETO + 1
            })
        );
    }

    /// Uma contrabarra declarada para o Windows vira separador aqui também, em
    /// vez de virar um nome de arquivo com contrabarra dentro.
    #[test]
    fn a_contrabarra_do_windows_e_separador_em_todo_sistema() {
        let caminho = CaminhoDoExecutavel::novo("bin\\seele.exe").unwrap();
        assert_eq!(caminho.como_caminho().components().count(), 2);
        assert_eq!(caminho.como_texto(), "bin\\seele.exe");
    }

    /// Os nomes que o Windows reserva, recusados **também no Mac**. Aceitar
    /// aqui seria publicar uma instalação que só falha ao abrir por lá.
    #[test]
    fn nome_reservado_do_windows_e_recusado_em_todo_sistema() {
        for texto in ["CON", "con", "bin/aux", "NUL.exe", "bin/COM1/seele", "lpt9"] {
            assert!(
                matches!(
                    CaminhoDoExecutavel::novo(texto),
                    Err(ExecutavelInvalido::NomeReservado { .. })
                ),
                "aceitou `{texto}`"
            );
        }
    }

    /// O mesmo argumento, para os nomes que o Windows não consegue criar ou
    /// que ele abre como **outro** arquivo.
    #[test]
    fn nome_que_o_windows_nao_cria_e_recusado_em_todo_sistema() {
        for texto in ["a<b>c|d", "seele?.exe", "bin/*", "nome\"aspas\""] {
            assert!(
                matches!(
                    CaminhoDoExecutavel::novo(texto),
                    Err(ExecutavelInvalido::CaractereProibido { .. })
                ),
                "aceitou `{texto}`"
            );
        }
        // O Windows corta o ponto e o espaço do fim: estes abririam outro
        // arquivo, ou nenhum.
        for texto in ["seele.exe.", "seele.exe ", "bin /seele", "pasta./seele"] {
            assert!(
                matches!(
                    CaminhoDoExecutavel::novo(texto),
                    Err(ExecutavelInvalido::FinalInvalido { .. })
                ),
                "aceitou `{texto}`"
            );
        }
    }

    /// O identificador da versão e o caminho do executável respondem **a mesma
    /// coisa** sobre um nome reservado. Eram duas respostas diferentes, e a
    /// divergência era o defeito.
    #[test]
    fn a_versao_e_o_executavel_concordam_sobre_nome_reservado() {
        for texto in ["con", "CON", "aux", "nul.1", "com1", "LPT9"] {
            assert_eq!(
                crate::versao::Versao::nova(texto).is_err(),
                CaminhoDoExecutavel::novo(texto).is_err(),
                "`{texto}` foi recusado por um e aceito pelo outro"
            );
        }
    }

    #[test]
    fn nenhuma_entrada_causa_panico() {
        for texto in [
            "",
            ".",
            "..",
            "\u{0}",
            "🙂/seele",
            &"a/".repeat(500),
            "//",
            "CON",
            " ",
            "./.",
        ] {
            let _ = CaminhoDoExecutavel::novo(texto);
        }
    }
}
