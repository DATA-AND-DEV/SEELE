//! Os cargos para a casca do desktop (ADR 0064), pelo `seele_core`: o ffi não
//! vê o `seele-proto` (ADR 0002).
//!
//! O estado é POR SESSÃO ([`CargosPorSessao`], pelo `SessaoId`): cada
//! `Connection` aplica as mensagens de cargo dela, e o evento sai carimbado
//! com a sessão e a geração, como todo evento (fronteiras.md §4). A fiação no
//! `lib.rs` (os braços do `ServerMessage`, o comando que manda um verbo) é de
//! B, pedida em pedidos.md; este módulo é o tipo e o JSON que a casca fala.

pub use seele_core::cargos::{
    Altura, Atribuicao, Cargo, CargosDaSessao, CargosNaTela, CargosPorSessao, ChaveDeMod,
    EfeitoNaPessoa, EfeitoNaTela, EventoDeCargos, Explicacao, Motivo, PedidoDeCargo, Permissao,
    PermissaoDeModNoQuadro, PermissaoExplicada, Previa, QuadroDeCargos, RecusaDeAcaoDeMod,
    RecusaDeCargo, RecusaNumerada, SuasPermissoes, NOME_DO_TODOS, PERMISSOES_DO_PRODUTO, TODOS,
};

/// Um verbo de cargo como a casca o manda: a revisão que a tela viu e o
/// pedido. Em JSON: `{"revisao": 12, "pedido": {"Atribuir": {"pessoa": 7, "cargo": 2}}}`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VerboDeCargo {
    /// A revisão do último `QuadroDeCargos` desta sessão.
    pub revisao: u64,
    /// O verbo.
    pub pedido: PedidoDeCargo,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod testes {
    use super::*;

    /// O JSON que a casca (G) manda é o que este tipo lê — o contrato da tela.
    #[test]
    fn o_json_da_casca_e_o_do_verbo() {
        for (texto, esperado) in [
            (
                r#"{"revisao":3,"pedido":{"Atribuir":{"pessoa":7,"cargo":2}}}"#,
                PedidoDeCargo::Atribuir {
                    pessoa: 7_u64.into(),
                    cargo: 2_u32.into(),
                },
            ),
            (
                r#"{"revisao":3,"pedido":{"Mover":{"cargo":2,"posicao":1}}}"#,
                PedidoDeCargo::Mover {
                    cargo: 2_u32.into(),
                    posicao: 1,
                },
            ),
        ] {
            let lido: VerboDeCargo = serde_json::from_str(texto).unwrap();
            assert_eq!(
                lido,
                VerboDeCargo {
                    revisao: 3,
                    pedido: esperado
                }
            );
        }
        assert!(
            serde_json::from_str::<VerboDeCargo>(r#"{"revisao":3,"pedido":{"Promover":{}}}"#)
                .is_err()
        );
    }
}
