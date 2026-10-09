//! O que um texto colado em CONECTAR é: um endereço, um convite, ou o código de
//! uma pessoa.
//!
//! Spec de 23/09, «Como um texto colado é classificado»: **uma função só, no
//! núcleo**, e a casca manda todo texto a ela (o `startsWith` de
//! `camada-servidores.js` sai com a tarefa 6g). As quatro regras, nesta ordem:
//!
//! 1. `seele://@…` é código;
//! 2. `seele://…` é convite;
//! 3. um texto sem `.`, `:`, `[`, `]` nem `/` que normaliza para 16 símbolos de
//!    base32 Crockford é código, com o verificador certo, ou erro de digitação,
//!    com o verificador errado;
//! 4. o resto é endereço.
//!
//! A consequência da regra 3, dita na spec: um nome de máquina de uma palavra
//! só, com exatamente 16 símbolos válidos, passa a exigir `:porta`. Um nome
//! NetBIOS tem no máximo 15 caracteres, e o Bonjour exige `.local`.
//!
//! «ESSE É O SEU CÓDIGO» não é daqui: esta função não sabe quem é a máquina. Quem
//! chama compara o [`Entrada::Codigo`] com o código desta máquina.

use seele_proto::codigo::{Codigo, CodigoInvalido};

/// O que um texto colado é.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entrada {
    /// Um endereço: `host`, `host:porta` ou `[v6]:porta`, como foi escrito.
    Endereco(String),
    /// Um convite `seele://…`, como foi escrito.
    Convite(String),
    /// O código de uma pessoa, pelo link `seele://@…` ou digitado.
    Codigo(Codigo),
    /// Tem a forma de um código, e não fecha: «ESSE CÓDIGO TEM UM ERRO DE
    /// DIGITAÇÃO», dito sem consultar a rede.
    CodigoComErro,
}

/// Classifica um texto colado em CONECTAR.
#[must_use]
pub fn ler_entrada(texto: &str) -> Entrada {
    let texto = texto.trim();
    // 1. O link da sala: um link que não fecha também é «erro de digitação».
    if let Some(lido) = seele_proto::uri::codigo_do_link(texto) {
        return lido.map_or(Entrada::CodigoComErro, Entrada::Codigo);
    }
    // 2. Um convite.
    if texto.starts_with(seele_proto::uri::ESQUEMA) {
        return Entrada::Convite(texto.to_owned());
    }
    // 3. A forma de um código. A leitura do código só ignora hífen e espaço,
    //    então um texto com `.`, `:`, `[`, `]` ou `/` nunca chega aqui como
    //    código: ele tem um símbolo fora do alfabeto. Preso em
    //    `um_codigo_escrito_com_pontos_ou_barras_e_endereco`.
    match Codigo::ler(texto) {
        Ok(codigo) => return Entrada::Codigo(codigo),
        Err(CodigoInvalido::Verificador) => return Entrada::CodigoComErro,
        // Sem dezesseis símbolos do alfabeto, não tem a forma de um código.
        Err(CodigoInvalido::Tamanho { .. } | CodigoInvalido::Simbolo { .. }) => {}
    }
    // 4. O resto.
    Entrada::Endereco(texto.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O código da chave de zeros, o mesmo dos vetores do `seele-proto`.
    const DE_ZEROS: &str = "CSM7-NBFR-CAYQ-EV4Y";
    /// O mesmo, com o último símbolo trocado: o verificador não bate.
    const TORTO: &str = "CSM7-NBFR-CAYQ-EV4Z";

    fn zeros() -> Codigo {
        Codigo::da_chave(&[0; 32])
    }

    #[test]
    fn o_link_da_sala_e_codigo() {
        assert_eq!(
            ler_entrada(&format!("seele://@{DE_ZEROS}")),
            Entrada::Codigo(zeros())
        );
        assert_eq!(
            ler_entrada(&format!("seele://@{TORTO}")),
            Entrada::CodigoComErro,
            "um link da sala com o verificador errado não foi dito como erro de digitação"
        );
        assert_eq!(
            ler_entrada("seele://@curto"),
            Entrada::CodigoComErro,
            "um link da sala que não fecha virou outra coisa"
        );
    }

    #[test]
    fn um_link_seele_sem_arroba_e_convite() {
        let link = "seele://192.168.0.7:8383?fp=ab";
        assert_eq!(ler_entrada(link), Entrada::Convite(link.into()));
    }

    #[test]
    fn um_codigo_digitado_de_qualquer_jeito_e_codigo() {
        for escrito in [
            DE_ZEROS,
            "csm7-nbfr-cayq-ev4y",
            "csm7 nbfr cayq ev4y",
            "CSM7NBFRCAYQEV4Y",
            "  CSM7-NBFR-CAYQ-EV4Y\n",
        ] {
            assert_eq!(
                ler_entrada(escrito),
                Entrada::Codigo(zeros()),
                "{escrito:?} não foi lido como o código"
            );
        }
    }

    #[test]
    fn o_o_vale_zero_e_o_i_e_o_l_valem_um() {
        // Um código que tenha 0 e 1, escrito como uma pessoa escreveria.
        let (codigo, escrito) = (0..=u8::MAX)
            .map(|n| Codigo::da_chave(&[n; 32]))
            .map(|codigo| (codigo, codigo.to_string()))
            .find(|(_, escrito)| escrito.contains('0') && escrito.contains('1'))
            .expect("algum código com 0 e com 1");
        let trocado = escrito
            .replace('0', "O")
            .replacen('1', "I", 1)
            .replace('1', "l");
        assert_eq!(
            ler_entrada(&trocado),
            Entrada::Codigo(codigo),
            "{trocado:?} não foi lido como {escrito}"
        );
    }

    #[test]
    fn um_codigo_com_o_verificador_errado_e_erro_de_digitacao() {
        assert_eq!(ler_entrada(TORTO), Entrada::CodigoComErro);
        assert_eq!(ler_entrada("csm7nbfrcayqev4z"), Entrada::CodigoComErro);
    }

    #[test]
    fn enderecos_continuam_enderecos() {
        for escrito in [
            "casa.exemplo",
            "casa.exemplo:8383",
            "10.0.0.1:8383",
            "10.0.0.1",
            "[::1]:8383",
            "servidor",
            "meu-servidor",
            // Dezesseis caracteres com um U, que não é do alfabeto: não tem a
            // forma de um código.
            "UUUUUUUUUUUUUUUU",
            // Quinze símbolos válidos: um a menos que um código.
            "CSM7NBFRCAYQEV4",
        ] {
            assert_eq!(
                ler_entrada(escrito),
                Entrada::Endereco(escrito.into()),
                "{escrito:?} deixou de ser endereço"
            );
        }
    }

    #[test]
    fn um_nome_de_uma_palavra_com_dezesseis_simbolos_exige_a_porta() {
        // A regra da spec, dita num teste: sem `:porta` ele tem a forma de um
        // código, e com ela é um endereço.
        assert_eq!(ler_entrada("csm7nbfrcayqev4z"), Entrada::CodigoComErro);
        assert_eq!(
            ler_entrada("csm7nbfrcayqev4z:8383"),
            Entrada::Endereco("csm7nbfrcayqev4z:8383".into())
        );
    }

    #[test]
    fn um_codigo_escrito_com_pontos_ou_barras_e_endereco() {
        // A regra 3 da spec: só um texto sem `.`, `:`, `[`, `]` nem `/` tem a
        // forma de um código. Se a leitura do código passar a ignorar algum
        // deles, este teste reprova antes de um endereço virar ligação.
        for escrito in [
            "CSM7.NBFR.CAYQ.EV4Y",
            "CSM7/NBFR/CAYQ/EV4Y",
            "CSM7:NBFR:CAYQ:EV4Y",
            "[CSM7NBFRCAYQEV4Y]",
        ] {
            assert_eq!(
                ler_entrada(escrito),
                Entrada::Endereco(escrito.into()),
                "{escrito:?} virou código"
            );
        }
    }

    #[test]
    fn o_texto_vem_sem_os_espacos_das_pontas() {
        assert_eq!(
            ler_entrada("  casa.exemplo:8383 \n"),
            Entrada::Endereco("casa.exemplo:8383".into())
        );
    }
}
