//! Os cargos na página (ADR 0064): o núcleo seguro que a fronteira do `wasm`
//! exporta. Explicar, prever e montar o comando — com a mesma regra do
//! servidor (`seele_proto::cargos`), e nunca decidindo no lugar dele.
//!
//! A página já manda uma `ClientMessage` em JSON (`codificar_do_cliente`) e
//! recebe o servidor em JSON (`decodificar_do_servidor`), então os verbos de
//! cargo e as três mensagens novas atravessam sem código novo. O que este
//! módulo acrescenta é o que a página não pode calcular sozinha sem escrever o
//! protocolo em JavaScript (`specs/06-clientes-gui.md:19`): o «por quê» e a
//! prévia. O estado é por sessão — cada chamada recebe o quadro DAQUELA
//! sessão, e nada aqui guarda quadro nenhum.

use serde_json::{json, Value};

use seele_proto::cargos::{mensagem_do_pedido, Estado, PedidoDeCargo, Permissao, QuadroDeCargos};
use seele_proto::ids::PersonId;

/// `{"explicacao": Explicacao}` ou `{"erro": "…"}`.
#[must_use]
pub fn explicar(quadro_json: &str, pessoa: u64, permissao_json: &str) -> Value {
    let lidos = serde_json::from_str::<QuadroDeCargos>(quadro_json).and_then(|quadro| {
        serde_json::from_str::<Permissao>(permissao_json).map(|permissao| (quadro, permissao))
    });
    match lidos {
        Ok((quadro, permissao)) => json!({
            "explicacao": Estado::do_quadro(&quadro).explicar(PersonId(pessoa), &permissao)
        }),
        Err(erro) => json!({ "erro": erro.to_string() }),
    }
}

/// `{"previa": "ok"}`, `{"previa": {"recusa": RecusaDeCargo}}` ou `{"erro": …}`.
/// Só informativa: quem confirma é o servidor.
#[must_use]
pub fn previa(quadro_json: &str, autor: u64, pedido_json: &str) -> Value {
    let lidos = serde_json::from_str::<QuadroDeCargos>(quadro_json).and_then(|quadro| {
        serde_json::from_str::<PedidoDeCargo>(pedido_json).map(|pedido| (quadro, pedido))
    });
    match lidos {
        Ok((quadro, pedido)) => {
            match Estado::do_quadro(&quadro).decidir(PersonId(autor), quadro.revisao, &pedido) {
                Ok(_) => json!({ "previa": "ok" }),
                Err(recusa) => json!({ "previa": { "recusa": recusa } }),
            }
        }
        Err(erro) => json!({ "erro": erro.to_string() }),
    }
}

/// Os membros (o FORA DO AR da sessão) em JSON: `{"<id>": "apelido", …}`.
fn membros_de(
    membros_json: &str,
) -> Result<std::collections::BTreeMap<PersonId, String>, serde_json::Error> {
    let lidos: std::collections::BTreeMap<u64, String> = serde_json::from_str(membros_json)?;
    Ok(lidos
        .into_iter()
        .map(|(id, apelido)| (PersonId(id), apelido))
        .collect())
}

/// O `CargosDaSessao` daquele quadro — a mesma dobra do desktop.
fn sessao_de(quadro: QuadroDeCargos) -> seele_sessao::cargos::CargosDaSessao {
    let mut cargos = seele_sessao::cargos::CargosDaSessao::default();
    let _ = cargos.aplicar(&seele_proto::control::ServerMessage::QuadroDeCargos(quadro));
    cargos
}

/// `prever_cargo` da página: `{"previa": Previa}` (quem ganha e perde o quê,
/// com o apelido), `{"recusa": RecusaDeCargo}` (o que o servidor diria) ou
/// `{"erro": …}`. A mesma conta do desktop e do guarda do ganho efetivo.
#[must_use]
pub fn prever_na_sessao(
    quadro_json: &str,
    membros_json: &str,
    autor: u64,
    revisao: u64,
    pedido_json: &str,
) -> Value {
    let lidos = serde_json::from_str::<QuadroDeCargos>(quadro_json).and_then(|quadro| {
        let membros = membros_de(membros_json)?;
        let pedido = serde_json::from_str::<PedidoDeCargo>(pedido_json)?;
        Ok((quadro, membros, pedido))
    });
    match lidos {
        Ok((quadro, membros, pedido)) => {
            match sessao_de(quadro).prever(&membros, PersonId(autor), revisao, &pedido) {
                Some(Ok(previa)) => json!({ "previa": previa }),
                Some(Err(recusa)) => json!({ "recusa": recusa }),
                None => json!({ "erro": "sem-quadro" }),
            }
        }
        Err(erro) => json!({ "erro": erro.to_string() }),
    }
}

/// `explicar_permissoes` da página: `{"permissoes": [PermissaoExplicada]}` ou
/// `{"erro": …}`.
#[must_use]
pub fn explicar_na_sessao(quadro_json: &str, membros_json: &str, pessoa: u64) -> Value {
    let lidos = serde_json::from_str::<QuadroDeCargos>(quadro_json)
        .and_then(|quadro| Ok((quadro, membros_de(membros_json)?)));
    match lidos {
        Ok((quadro, membros)) => json!({
            "permissoes": sessao_de(quadro).explicar_tudo(&membros, PersonId(pessoa))
        }),
        Err(erro) => json!({ "erro": erro.to_string() }),
    }
}

/// `pedir_cargos` da página: o `CargosNaTela` (o quadro, as suas e os
/// apelidos de quem aparece no quadro) ou `{"erro": …}`. `suas_json` pode ser
/// `null`.
#[must_use]
pub fn na_tela(quadro_json: &str, suas_json: &str, membros_json: &str) -> Value {
    let lidos = serde_json::from_str::<QuadroDeCargos>(quadro_json).and_then(|quadro| {
        let suas = serde_json::from_str::<Option<seele_proto::cargos::SuasPermissoes>>(suas_json)?;
        Ok((quadro, suas, membros_de(membros_json)?))
    });
    match lidos {
        Ok((quadro, suas, membros)) => {
            let mut cargos = sessao_de(quadro);
            if let Some(suas) = suas {
                let _ = cargos.aplicar(&seele_proto::control::ServerMessage::SuasPermissoes(suas));
            }
            serde_json::to_value(cargos.na_tela(&membros))
                .unwrap_or_else(|erro| json!({ "erro": erro.to_string() }))
        }
        Err(erro) => json!({ "erro": erro.to_string() }),
    }
}

/// A `ClientMessage` (em JSON, para `codificar_do_cliente`) que pede este
/// verbo com esta revisão, ou `{"erro": …}`.
#[must_use]
pub fn comando(revisao: u64, pedido_json: &str) -> Value {
    match serde_json::from_str::<PedidoDeCargo>(pedido_json) {
        Ok(pedido) => serde_json::to_value(mensagem_do_pedido(revisao, pedido))
            .unwrap_or_else(|erro| json!({ "erro": erro.to_string() })),
        Err(erro) => json!({ "erro": erro.to_string() }),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod testes {
    use super::*;
    use seele_proto::cargos::{Atribuicao, Cargo, NOME_DO_TODOS, TODOS};
    use seele_proto::control::Permission;
    use seele_proto::ids::RoleId;

    fn quadro() -> String {
        serde_json::to_string(&QuadroDeCargos {
            cargos: vec![
                Cargo {
                    id: RoleId(1),
                    nome: "Gestão".into(),
                    posicao: 1,
                    concede: vec![Permissao::Produto(Permission::ManageRoles)],
                    restringe: vec![],
                },
                Cargo {
                    id: TODOS,
                    nome: NOME_DO_TODOS.into(),
                    posicao: 0,
                    concede: vec![],
                    restringe: vec![],
                },
            ],
            atribuicoes: vec![Atribuicao {
                pessoa: PersonId(7),
                cargos: vec![RoleId(1)],
            }],
            proprietario: None,
            de_mods: vec![],
            revisao: 12,
            dono_declarado: false,
        })
        .unwrap()
    }

    #[test]
    fn o_comando_sai_como_a_mensagem_que_a_pagina_codifica() {
        let saida = comando(12, r#"{"Atribuir":{"pessoa":8,"cargo":1}}"#);
        let mensagem: seele_proto::control::ClientMessage = serde_json::from_value(saida).unwrap();
        assert_eq!(
            mensagem,
            seele_proto::control::ClientMessage::AtribuirCargo {
                revisao: 12,
                pessoa: PersonId(8),
                cargo: RoleId(1)
            }
        );
        assert!(crate::codificar_do_cliente(
            seele_proto::PROTOCOL_VERSION,
            &serde_json::to_string(&mensagem).unwrap()
        )
        .is_ok());
    }

    #[test]
    fn a_pagina_tem_as_mesmas_tres_respostas_do_desktop() {
        let membros = r#"{"7":"ana","8":"bia"}"#;
        // A 7 (Gestão) atribui a Gestão à 8? É o cargo dela: recusa nomeada.
        let recusa = prever_na_sessao(
            &quadro(),
            membros,
            7,
            12,
            r#"{"Atribuir":{"pessoa":8,"cargo":1}}"#,
        );
        assert_eq!(recusa["recusa"], serde_json::json!("CargoAcimaDeVoce"));
        // Revisão velha: a mesma que o servidor diria.
        let velha = prever_na_sessao(&quadro(), membros, 7, 11, r#"{"Apagar":{"cargo":1}}"#);
        assert_eq!(velha["recusa"]["RevisaoVelha"]["atual"], 12);
        let explicadas = explicar_na_sessao(&quadro(), membros, 8);
        assert_eq!(
            explicadas["permissoes"].as_array().map(Vec::len),
            Some(seele_proto::cargos::PERMISSOES_DO_PRODUTO.len())
        );
        let tela = na_tela(&quadro(), "null", membros);
        assert_eq!(tela["apelidos"]["7"], "ana");
        assert_eq!(tela["quadro"]["revisao"], 12);
    }

    #[test]
    fn a_explicacao_e_a_previa_falam_json_e_recusam_entrada_torta() {
        let explicada = explicar(&quadro(), 7, r#"{"Produto":"Ban"}"#);
        assert_eq!(explicada["explicacao"]["tem"], false);
        assert_eq!(explicada["explicacao"]["motivo"], "NinguemConcede");
        let prevista = previa(
            &quadro(),
            7,
            r#"{"Criar":{"nome":"x","concede":[{"Produto":"Ban"}],"restringe":[]}}"#,
        );
        assert_eq!(
            prevista["previa"]["recusa"]["NaoPodeConceder"]["permissao"]["Produto"],
            "Ban"
        );
        assert!(explicar("{", 7, "{}")["erro"].is_string());
        assert!(comando(1, "nao e json")["erro"].is_string());
    }
}
