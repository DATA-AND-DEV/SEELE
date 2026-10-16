//! **Do lado de quem liga** (tarefas 28 e 30 da F7; spec de 23/09, «Do lado de
//! quem liga»).
//!
//! A `sala.rs` é o lado de quem recebe: a sala que sobe, toca e atende, e que
//! mexe nas hospedagens deste app. Quem liga não hospeda nada: lê o texto colado
//! em CONECTAR e disca, pela ponte, o alvo `@CÓDIGO`. Por isso mora à parte: a
//! ponte do navegador separa os comandos que mexem na hospedagem local, e uma
//! leitura de texto não é um deles.

/// O que um texto colado em CONECTAR é, para a casca (tarefa 30 da F7, a 6g;
/// spec de 23/09, «Como um texto colado é classificado» e «Do lado de quem
/// liga», 1).
///
/// A leitura é a do núcleo (`seele_ffi::entrada::ler_entrada`): a casca manda
/// **todo** texto, e o `startsWith("seele://")` dela sai. A pergunta «é o meu
/// código?» é respondida aqui, com a chave desta máquina, e não na tela.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) enum EntradaColada {
    /// Um endereço, como foi escrito.
    Endereco(String),
    /// Um convite `seele://…`, como foi escrito.
    Convite(String),
    /// O código de outra pessoa.
    Codigo {
        /// O código, como a tela o mostra.
        codigo: String,
        /// O alvo canônico de quem liga, `@CÓDIGO`
        /// (`seele_ffi::alvo_do_codigo`): ele se basta sozinho, e sobrevive ao
        /// `ejetar`.
        alvo: String,
    },
    /// Tem a forma de um código, e não fecha: «ESSE CÓDIGO TEM UM ERRO DE
    /// DIGITAÇÃO», dito sem consultar a rede.
    CodigoComErro,
    /// O código desta máquina: «ESSE É O SEU CÓDIGO».
    SeuCodigo,
}

/// [`EntradaColada`] de um texto, com o código desta máquina, quando ele se
/// lê. Sem ele (a identidade não abriu), nenhum código é «o seu».
pub(crate) fn ler_entrada_em(texto: &str, meu: Option<&seele_ffi::Codigo>) -> EntradaColada {
    match seele_ffi::entrada::ler_entrada(texto) {
        seele_ffi::entrada::Entrada::Endereco(endereco) => EntradaColada::Endereco(endereco),
        seele_ffi::entrada::Entrada::Convite(convite) => EntradaColada::Convite(convite),
        seele_ffi::entrada::Entrada::CodigoComErro => EntradaColada::CodigoComErro,
        seele_ffi::entrada::Entrada::Codigo(codigo) if meu == Some(&codigo) => {
            EntradaColada::SeuCodigo
        }
        seele_ffi::entrada::Entrada::Codigo(codigo) => EntradaColada::Codigo {
            codigo: codigo.to_string(),
            alvo: seele_ffi::alvo_do_codigo(&codigo),
        },
    }
}

/// A entrada colada em CONECTAR (tarefa 30 da F7): a leitura do núcleo, com «é o
/// seu código?» respondido pela chave desta máquina.
#[cfg(test)]
mod a_entrada_colada {
    use super::{ler_entrada_em, EntradaColada};
    use seele_ffi::Codigo;

    /// O código da chave de zeros, o mesmo dos vetores do `seele-proto`.
    const DE_ZEROS: &str = "CSM7-NBFR-CAYQ-EV4Y";

    fn zeros() -> Codigo {
        Codigo::da_chave(&[0; 32])
    }

    #[test]
    fn enderecos_e_convites_passam_como_foram_escritos() {
        for endereco in ["casa.exemplo:8383", "10.0.0.1:8383", "[::1]:8383"] {
            assert_eq!(
                ler_entrada_em(endereco, None),
                EntradaColada::Endereco(endereco.to_owned())
            );
        }
        assert_eq!(
            ler_entrada_em("seele://casa.exemplo:8383", None),
            EntradaColada::Convite("seele://casa.exemplo:8383".to_owned())
        );
    }

    #[test]
    fn o_codigo_de_outra_pessoa_vem_com_o_alvo_de_quem_liga() {
        let outro = Codigo::da_chave(&[1; 32]);
        assert_eq!(
            ler_entrada_em(DE_ZEROS, Some(&outro)),
            EntradaColada::Codigo {
                codigo: DE_ZEROS.to_owned(),
                alvo: seele_ffi::alvo_do_codigo(&zeros()),
            }
        );
        // O link da sala também é código, e o alvo é o mesmo.
        assert_eq!(
            ler_entrada_em(&format!("seele://@{DE_ZEROS}"), None),
            ler_entrada_em(DE_ZEROS, None)
        );
    }

    #[test]
    fn o_codigo_desta_maquina_e_o_seu_como_quer_que_seja_escrito() {
        for escrito in [
            DE_ZEROS,
            "csm7 nbfr cayq ev4y",
            &format!("seele://@{DE_ZEROS}"),
        ] {
            assert_eq!(
                ler_entrada_em(escrito, Some(&zeros())),
                EntradaColada::SeuCodigo,
                "`{escrito}`"
            );
        }
        // Sem a chave desta máquina, nenhum código é o seu.
        assert!(matches!(
            ler_entrada_em(DE_ZEROS, None),
            EntradaColada::Codigo { .. }
        ));
    }

    #[test]
    fn um_codigo_que_nao_fecha_e_erro_de_digitacao() {
        assert_eq!(
            ler_entrada_em("CSM7-NBFR-CAYQ-EV4Z", Some(&zeros())),
            EntradaColada::CodigoComErro
        );
    }

    #[test]
    fn a_casca_le_a_entrada_pelo_nome_da_variante() {
        // A tela escolhe a frase e o caminho por estes nomes.
        assert_eq!(
            serde_json::to_value(ler_entrada_em(DE_ZEROS, None)).ok(),
            Some(serde_json::json!({
                "Codigo": { "codigo": DE_ZEROS, "alvo": format!("@{DE_ZEROS}") }
            }))
        );
        assert_eq!(
            serde_json::to_value(EntradaColada::SeuCodigo).ok(),
            Some(serde_json::json!("SeuCodigo"))
        );
        assert_eq!(
            serde_json::to_value(EntradaColada::CodigoComErro).ok(),
            Some(serde_json::json!("CodigoComErro"))
        );
    }
}
