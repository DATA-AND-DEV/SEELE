//! O túnel de jogo (protocolo 12): o vocabulário que o servidor, o núcleo e a
//! casca compartilham para um MOD da API 8 com o poder `porta`.
//!
//! Aqui só mora o que atravessa o fio ou nomeia uma recusa: os tipos, os
//! rótulos e as regras de forma. Quem serve, quem alcança e quem repassa estão
//! nos crates que usam estes tipos; nenhum comportamento mora neste.

use serde::{Deserialize, Serialize};

use crate::control::{check_bounds, ControlError, Validate, MAX_ID_DO_MOD_NOS_PEDIDOS};
use crate::ids::PersonId;

/// A ação de MOD que dá a uma pessoa o direito de alcançar um túnel (ADR 0064).
///
/// Uma constante só, para o servidor, a casca e os MODs de referência não
/// poderem divergir na grafia.
pub const ACAO_ENTRAR_NO_JOGO: &str = "entrar-no-jogo";

/// O maior nome de túnel. `[a-z0-9-]`, como uma ação de MOD
/// ([`crate::cargos::acao_valida`]): o nome viaja em chaves e em logs.
pub const MAX_NOME_DE_TUNEL: usize = 32;

/// Quantos endereços uma apresentação de túnel leva. O quadro de controle já
/// tem teto de bytes; este é o teto de **candidatos**, para uma discagem não
/// virar uma rajada de tentativas.
pub const MAX_ENDERECOS_DE_TUNEL: usize = 16;

/// Se `nome` é um nome de túnel aceitável: `[a-z0-9-]`, de 1 a
/// [`MAX_NOME_DE_TUNEL`], sem `-` nas pontas.
#[must_use]
pub fn nome_valido(nome: &str) -> bool {
    !nome.is_empty()
        && nome.len() <= MAX_NOME_DE_TUNEL
        && !nome.starts_with('-')
        && !nome.ends_with('-')
        && nome
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// O que o túnel carrega.
///
/// **Sempre acrescente no fim** (ADR 0058).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProtocoloDeTunel {
    /// Uma conexão TCP por canal `t`.
    Tcp,
    /// Um fluxo UDP por canal `j`.
    Udp,
}

/// Por que um pedido de túnel foi recusado.
///
/// Enumerado e nunca texto livre (`specs/02-protocolo.md`): a casca decide a
/// frase. **Sempre acrescente no fim** (ADR 0058).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RecusaDeTunel {
    /// Nenhuma máquina serve esse nome.
    NaoExiste,
    /// A pessoa não tem a ação `entrar-no-jogo` do MOD.
    SemPermissao,
    /// O MOD não declara a ação `entrar-no-jogo`.
    AcaoNaoDeclarada,
    /// O MOD não está ligado ou não declara o poder `porta`.
    ModSemPorta,
    /// Já há um túnel com esse nome para esse MOD.
    NomeTomado,
    /// A máquina que serve saiu enquanto o pedido andava.
    MaquinaSaiu,
    /// O servidor já tem túneis demais.
    TuneisDemais,
    /// Não há canais de túnel livres para essa pessoa.
    CanaisEsgotados,
    /// **Nunca atravessa o fio.** É a recusa local de quem negociou 11 e não
    /// pode mandar verbo de túnel; a variante mora aqui para a casca ter um
    /// nome só para as recusas.
    ServidorAntigo,
}

/// Por que um túnel acabou.
///
/// **Sempre acrescente no fim** (ADR 0058).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MotivoDoFimDoTunel {
    /// A máquina parou de servir (`PararTunel`).
    MaquinaParou,
    /// A máquina saiu do servidor.
    MaquinaSaiu,
    /// O MOD foi desligado.
    ModDesligado,
    /// A pessoa perdeu a ação `entrar-no-jogo`.
    SemPermissao,
    /// A pessoa que jogava saiu.
    PessoaSaiu,
    /// O anfitrião da ligação caiu.
    AnfitriaoCaiu,
}

/// O que um canal `t`/`j` carrega.
///
/// **Sempre acrescente no fim** (ADR 0058).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TipoDeCanalDeTunel {
    /// Os bytes de uma conexão TCP.
    Tcp,
    /// Os datagramas de um fluxo UDP.
    Udp,
    /// O canal de sinais ([`SinalDeTunel`]) de um túnel UDP.
    Controle,
}

/// A primeira mensagem de todo canal `t`/`j`.
///
/// `de` é `None` quando quem joga abre o canal, e `Some(quem_joga)` quando o
/// anfitrião o repassa pela estrela: assim a máquina sabe de quem é cada fluxo
/// sem confiar no que a rede diz.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AberturaDeTunel {
    /// `autor/nome` do MOD.
    pub mod_id: String,
    /// O nome do túnel.
    pub nome: String,
    /// O fluxo: uma conexão TCP, ou um endereço de origem do cliente do jogo.
    pub fluxo: u32,
    /// O que o canal carrega.
    pub tipo: TipoDeCanalDeTunel,
    /// De quem é o fluxo, quando o anfitrião repassa.
    pub de: Option<PersonId>,
}

/// O que corre no canal [`TipoDeCanalDeTunel::Controle`].
///
/// **Sempre acrescente no fim** (ADR 0058).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SinalDeTunel {
    /// Pede que a outra ponta responda.
    Ping {
        /// Devolvido no `Pong`.
        n: u32,
    },
    /// A resposta a um `Ping`.
    Pong {
        /// O `n` do `Ping`.
        n: u32,
    },
}

/// O identificador e o nome de um túnel, com a mesma régua em todo lugar.
///
/// # Errors
///
/// [`ControlError::FieldOutOfRange`] quando o `mod_id` não é um `autor/nome` ou
/// o nome não passa em [`nome_valido`].
pub fn check_tunel(mod_id: &str, nome: &str) -> Result<(), ControlError> {
    check_bounds("mod_id", mod_id.len(), MAX_ID_DO_MOD_NOS_PEDIDOS)?;
    if !crate::cargos::id_de_mod_valido(mod_id) {
        return Err(ControlError::FieldOutOfRange { field: "mod_id" });
    }
    if !nome_valido(nome) {
        return Err(ControlError::FieldOutOfRange { field: "tunel" });
    }
    Ok(())
}

/// O teto de candidatos de uma apresentação.
///
/// # Errors
///
/// [`ControlError::FieldTooLong`] acima de [`MAX_ENDERECOS_DE_TUNEL`].
pub fn check_enderecos(len: usize) -> Result<(), ControlError> {
    check_bounds("enderecos", len, MAX_ENDERECOS_DE_TUNEL)
}

impl Validate for AberturaDeTunel {
    fn validate(&self) -> Result<(), ControlError> {
        check_tunel(&self.mod_id, &self.nome)
    }
}

impl Validate for SinalDeTunel {
    fn validate(&self) -> Result<(), ControlError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn o_nome_do_tunel_segue_a_regra_das_acoes() {
        for bom in ["mc-1", "a", "jogo", "0", &"a".repeat(MAX_NOME_DE_TUNEL)] {
            assert!(nome_valido(bom), "{bom}");
        }
        for ruim in [
            "",
            "-a",
            "a-",
            "A",
            "a b",
            "a/b",
            "á",
            &"a".repeat(MAX_NOME_DE_TUNEL + 1),
        ] {
            assert!(!nome_valido(ruim), "{ruim}");
        }
    }

    #[test]
    fn a_abertura_recusa_mod_ou_nome_que_nao_valem() {
        let boa = AberturaDeTunel {
            mod_id: "a/b".into(),
            nome: "mc-1".into(),
            fluxo: 1,
            tipo: TipoDeCanalDeTunel::Tcp,
            de: None,
        };
        assert!(boa.validate().is_ok());
        assert!(AberturaDeTunel {
            nome: "Mc".into(),
            ..boa.clone()
        }
        .validate()
        .is_err());
        assert!(AberturaDeTunel {
            mod_id: "sem-barra".into(),
            ..boa
        }
        .validate()
        .is_err());
    }
}
