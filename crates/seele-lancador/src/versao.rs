//! O identificador de uma versão, e por que ele não é uma `String`.
//!
//! Uma versão publicada vira **três coisas ao mesmo tempo**: um pedaço de
//! caminho em disco (`versoes/0.10.5-1/`), um pedaço de caminho de dados
//! (`dados/0.10.5-1/`) e um pedaço de URL. Os três vêm de um texto que o
//! manifesto trouxe da rede, e o manifesto é servido pelo GitHub — não é
//! entrada hostil no caso comum, e é exatamente o caso comum que faz alguém
//! esquecer que ela poderia ser.
//!
//! Um identificador `../../../etc` atravessaria os três usos. Por isso a
//! conferência acontece **uma vez, na construção**, e o resto do crate recebe
//! um tipo que já não pode ser aquilo.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::fmt;

/// Quantos caracteres um identificador de versão pode ter.
///
/// Não há versão legítima perto disso; o teto existe porque um nome de
/// diretório longo demais falha na criação em vez de na leitura, e falhar cedo
/// dá uma frase melhor.
const TETO: usize = 64;

/// Nomes que o Windows reserva para dispositivos, em qualquer pasta.
///
/// `CON`, `PRN` e companhia não são arquivos: uma pasta com esse nome não pode
/// ser criada, e a falha chega como «acesso negado» em vez de «nome inválido».
/// Recusar aqui é o que faz a mensagem dizer o que houve.
///
/// Vale em todos os sistemas de propósito. Um identificador que só serve no Mac
/// não é um identificador de versão: o manifesto é o mesmo para as três
/// máquinas, e uma versão que não pudesse ser instalada no Windows precisa ser
/// recusada no Mac também — senão a falha aparece na máquina de outra pessoa.
const RESERVADOS: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// O nome reservado com que este pedaço de nome casa, se casar.
///
/// Mora aqui e é usado também por [`crate::executavel`], de propósito. A razão
/// da lista — o manifesto é o mesmo para as três máquinas, e recusar só onde
/// dói faz a falha aparecer na máquina de outra pessoa — vale igual para o
/// identificador da versão e para **cada componente** do caminho do
/// executável. Duas listas seriam duas respostas para a mesma pergunta, e a
/// que ficasse para trás recusaria menos sem ninguém notar.
///
/// O Windows ignora a extensão ao casar: `con.txt` é o console. A comparação é
/// sobre o que vem antes do primeiro ponto.
pub(crate) fn nome_reservado(pedaco: &str) -> Option<String> {
    let raiz = pedaco
        .split('.')
        .next()
        .unwrap_or(pedaco)
        .to_ascii_lowercase();
    RESERVADOS.contains(&raiz.as_str()).then_some(raiz)
}

/// Por que um texto não serve como identificador de versão.
///
/// Enum e não frase, pela regra de `specs/02-protocolo.md`: nenhuma razão
/// genérica, e a casca decide o texto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentificadorInvalido {
    /// Texto vazio.
    Vazio,
    /// Passou de [`TETO`] caracteres.
    LongoDemais {
        /// Quantos vieram.
        caracteres: usize,
    },
    /// Um caractere que não é letra ASCII, dígito, ponto, hífen ou sublinhado.
    ///
    /// A barra e a contrabarra caem aqui, e é o ponto: é assim que um
    /// identificador deixa de poder virar caminho.
    CaractereProibido {
        /// O primeiro que não serviu.
        caractere: char,
    },
    /// Começa com algo que não é letra ou dígito.
    ///
    /// Um identificador que começa com ponto vira arquivo oculto; um que começa
    /// com hífen vira opção de linha de comando na primeira ferramenta que o
    /// receber.
    ComecoInvalido,
    /// É `.` ou `..`, que não nomeiam diretório nenhum: nomeiam a travessia.
    Travessia,
    /// Bate com um nome de dispositivo reservado pelo Windows.
    NomeReservado {
        /// Qual deles.
        nome: String,
    },
}

/// O identificador de uma versão publicada, já conferido.
///
/// Não há `Ord`, e a ausência é decisão. `0.10.5-1` é, para o semver, uma
/// pré-lançamento de `0.10.5` e portanto **anterior** a ela — e no
/// `SEELE-RELEASES` a `v0.10.5-1` saiu **depois** da `v0.10.5`. Ordenar por
/// conta própria daria a resposta errada sobre o que é «a mais nova», então
/// quem ordena é o manifesto, pela ordem em que lista as publicações. Ver
/// [`crate::manifesto::Manifesto::mais_nova`].
///
/// A ordem derivada seria a do texto, e ela erra o caso acima **e** o caso
/// banal: `"0.9.0"` viria depois de `"0.10.0"`. Quem só precisa listar ordena
/// pelo texto no próprio lugar, escrevendo que é isso que está fazendo —
/// [`crate::Deposito::instaladas`] e [`crate::Deposito::versoes_com_dados`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Versao(String);

impl Versao {
    /// Confere um texto e devolve o identificador, ou diz por que não.
    ///
    /// # Errors
    ///
    /// [`IdentificadorInvalido`], uma variante por motivo.
    pub fn nova(bruto: &str) -> Result<Self, IdentificadorInvalido> {
        if bruto.is_empty() {
            return Err(IdentificadorInvalido::Vazio);
        }
        if bruto.chars().count() > TETO {
            return Err(IdentificadorInvalido::LongoDemais {
                caracteres: bruto.chars().count(),
            });
        }
        if bruto == "." || bruto == ".." {
            return Err(IdentificadorInvalido::Travessia);
        }
        if let Some(caractere) = bruto
            .chars()
            .find(|c| !(c.is_ascii_alphanumeric() || *c == '.' || *c == '-' || *c == '_'))
        {
            return Err(IdentificadorInvalido::CaractereProibido { caractere });
        }
        if !bruto.starts_with(|c: char| c.is_ascii_alphanumeric()) {
            return Err(IdentificadorInvalido::ComecoInvalido);
        }
        if let Some(nome) = nome_reservado(bruto) {
            return Err(IdentificadorInvalido::NomeReservado { nome });
        }
        Ok(Self(bruto.to_owned()))
    }

    /// O texto, para compor caminho, URL ou tela.
    #[must_use]
    pub fn como_texto(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Versao {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for Versao {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let bruto = String::deserialize(d)?;
        Self::nova(&bruto).map_err(|erro| {
            serde::de::Error::custom(format!("identificador de versão recusado: {erro:?}"))
        })
    }
}

impl serde::Serialize for Versao {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn as_versoes_que_o_projeto_publicou_de_fato_sao_aceitas() {
        // Lidas da página de releases do `SEELE-RELEASES`, e não inventadas.
        for texto in ["0.10.5-1", "0.10.5", "0.10.4-4", "0.1.2", "1.0.0"] {
            assert!(Versao::nova(texto).is_ok(), "recusou `{texto}`");
        }
    }

    /// O guarda que justifica o tipo existir: nenhum desses vira caminho.
    #[test]
    fn nenhum_identificador_atravessa_diretorio() {
        for texto in [
            "..",
            ".",
            "../0.10.5",
            "..\\0.10.5",
            "/etc/passwd",
            "0.10.5/../../x",
            "0.10.5\\..\\x",
            "C:\\Windows",
            ".oculta",
        ] {
            assert!(Versao::nova(texto).is_err(), "aceitou `{texto}`");
        }
    }

    #[test]
    fn nome_reservado_do_windows_e_recusado_em_todo_sistema() {
        for texto in ["con", "CON", "nul.1", "com1", "LPT9.0.0"] {
            assert!(
                matches!(
                    Versao::nova(texto),
                    Err(IdentificadorInvalido::NomeReservado { .. })
                ),
                "aceitou `{texto}`"
            );
        }
    }

    #[test]
    fn o_motivo_da_recusa_e_especifico_e_nao_generico() {
        assert_eq!(Versao::nova(""), Err(IdentificadorInvalido::Vazio));
        assert_eq!(
            Versao::nova("0.10 5"),
            Err(IdentificadorInvalido::CaractereProibido { caractere: ' ' })
        );
        assert_eq!(
            Versao::nova("-1.0.0"),
            Err(IdentificadorInvalido::ComecoInvalido)
        );
        let comprido = "0".repeat(TETO + 1);
        assert_eq!(
            Versao::nova(&comprido),
            Err(IdentificadorInvalido::LongoDemais {
                caracteres: TETO + 1
            })
        );
    }

    #[test]
    fn nenhuma_entrada_causa_panico() {
        for texto in ["", ".", "..", "\u{0}", "🙂", &"a".repeat(1000), "0.\u{0}"] {
            let _ = Versao::nova(texto);
        }
    }

    #[test]
    fn a_serializacao_passa_pela_mesma_conferencia_da_construcao() {
        let recusada: Result<Versao, _> = serde_json::from_str("\"../fuga\"");
        assert!(
            recusada.is_err(),
            "um manifesto não devia poder trazer isso"
        );
        let aceita: Versao = serde_json::from_str("\"0.10.5-1\"").unwrap();
        assert_eq!(aceita.como_texto(), "0.10.5-1");
    }
}
