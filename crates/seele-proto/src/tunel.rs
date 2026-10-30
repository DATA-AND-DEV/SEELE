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

/// O primeiro byte de todo datagrama de um canal `j`: este datagrama é a
/// [`AberturaDeTunel`] do canal.
///
/// O `j` anda num canal sem retransmissão e sem ordem. Antes do tag, a abertura
/// era «o primeiro datagrama»: se ele se perdesse ou chegasse depois de um
/// dado, quem recebia lia bytes do jogo como abertura, cortava o canal e
/// contava uma concessão que nunca faltou. Com o tag, ninguém adivinha pela
/// posição.
pub const TAG_DE_ABERTURA: u8 = 0;

/// O primeiro byte de um datagrama de um canal `j` que leva **carga do jogo**.
pub const TAG_DE_DADO: u8 = 1;

/// O intervalo máximo entre duas aberturas repetidas num canal `j` que só tem
/// ida (o jogo manda e nunca ouve): sem resposta não há como saber que a
/// abertura chegou, então ela se repete, a cada tanto, com custo limitado.
pub const REPETE_A_ABERTURA_A_CADA: std::time::Duration = std::time::Duration::from_secs(1);

/// O que um datagrama de um canal `j` é, pelo primeiro byte.
#[derive(Debug, PartialEq, Eq)]
pub enum DatagramaJ<'a> {
    /// A abertura do canal ([`TAG_DE_ABERTURA`]), ainda por decodificar com
    /// [`crate::control::decode`].
    Abertura(&'a [u8]),
    /// Carga do jogo ([`TAG_DE_DADO`]).
    Dado(&'a [u8]),
    /// Um tag que esta versão não conhece, ou um datagrama vazio.
    Ilegivel,
}

/// Separa o tag do resto de um datagrama de canal `j`.
#[must_use]
pub fn ler_datagrama_j(bytes: &[u8]) -> DatagramaJ<'_> {
    match bytes.split_first() {
        Some((&TAG_DE_ABERTURA, resto)) => DatagramaJ::Abertura(resto),
        Some((&TAG_DE_DADO, resto)) => DatagramaJ::Dado(resto),
        _ => DatagramaJ::Ilegivel,
    }
}

/// O datagrama de abertura de um canal `j`: o tag e a abertura codificada.
///
/// # Errors
///
/// Os de [`crate::control::encode`].
pub fn datagrama_de_abertura(abertura: &AberturaDeTunel) -> Result<Vec<u8>, ControlError> {
    let codificada = crate::control::encode(abertura)?;
    let mut datagrama = Vec::with_capacity(codificada.len() + 1);
    datagrama.push(TAG_DE_ABERTURA);
    datagrama.extend_from_slice(&codificada);
    Ok(datagrama)
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
    /// **Nunca atravessa o fio.** Nenhuma das 65 portas locais (a do jogo e as
    /// 64 seguintes) estava livre em `127.0.0.1`: a recusa é desta máquina.
    PortaLocalOcupada,
    /// **Nunca atravessa o fio.** Ninguém respondeu: a sessão desta pessoa já
    /// tinha acabado, o canal de comandos fechou, ou o prazo venceu sem
    /// `TunelConcedido` nem `TunelRecusado`.
    SemResposta,
    /// A máquina deste nome existe, mas a pessoa que a opera ainda não aceitou
    /// o que o servidor lhe mostrou (API 8 parte 3). **Atravessa o fio**: é o
    /// servidor quem a recusa, e por isso `so_na_ponta` não a inclui.
    ForaDoAceite,
}

impl RecusaDeTunel {
    /// Se a recusa nasce na ponta e **nunca** atravessa o fio.
    ///
    /// O `Validate` de `TunelRecusado` usa isto para recusar, ao decodificar,
    /// o que um servidor hostil não teria motivo de mandar.
    #[must_use]
    pub const fn so_na_ponta(self) -> bool {
        matches!(
            self,
            Self::ServidorAntigo | Self::PortaLocalOcupada | Self::SemResposta
        )
    }
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
    ///
    /// **Reservada: nenhum produtor hoje.** Quando o anfitrião cai, a conexão
    /// das duas pontas cai com ele, e quem joga vê `ServidorSaiu` (a sessão
    /// acabou) em vez de uma mensagem do servidor. A variante fica por causa da
    /// regra do ADR 0058 (nada se remove da lista) e porque a parte 3, com o
    /// servidor da máquina separado do anfitrião, pode precisar dela. O núcleo
    /// já a traduz (`queda_do_fim`), para o dia em que alguém a mandar.
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
    fn o_tag_separa_a_abertura_da_carga_e_nao_adivinha_pela_posicao() {
        let aberta = AberturaDeTunel {
            mod_id: "a/b".into(),
            nome: "mc-1".into(),
            fluxo: 7,
            tipo: TipoDeCanalDeTunel::Udp,
            de: None,
        };
        let datagrama = datagrama_de_abertura(&aberta).unwrap();
        let DatagramaJ::Abertura(corpo) = ler_datagrama_j(&datagrama) else {
            panic!("a abertura tem de ler-se como abertura")
        };
        assert_eq!(
            crate::control::decode::<AberturaDeTunel>(corpo).unwrap(),
            aberta
        );
        // Carga cujo primeiro byte de dado é o tag de dado: nunca abertura,
        // mesmo que o resto pareça uma.
        let mut carga = vec![TAG_DE_DADO];
        carga.extend_from_slice(corpo);
        assert!(matches!(ler_datagrama_j(&carga), DatagramaJ::Dado(c) if c == corpo));
        assert_eq!(ler_datagrama_j(&[]), DatagramaJ::Ilegivel);
        assert_eq!(ler_datagrama_j(&[9, 1, 2]), DatagramaJ::Ilegivel);
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
