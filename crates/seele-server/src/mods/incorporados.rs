//! Os MODs que viraram parte do produto, e por isso não se ligam mais.
//!
//! `docs/0.19/perfis-oficiais.md`, §4 «Desligar e recusar». O PERFIS virou o
//! perfil nativo na 0.19: os dados dele são migrados na subida
//! (`perfis::migracao`), e o MOD é desligado. Ligá-lo de novo faria dois donos
//! do mesmo perfil — o MOD gravando no quintal, o produto na tabela `perfis` —
//! e o que um gravasse o outro não veria.
//!
//! # Duas portas, uma lista
//!
//! - **na subida**, [`desligar_os_ligados`]: um incorporado ligado no banco (um
//!   banco antigo, ou alguém que o religou à mão) sai desligado, depois da
//!   migração;
//! - **em quem aplica um conjunto** (`aplicar_conjunto_de_mods`, no app), por
//!   [`conferir_conjunto`]: a recusa é o erro tipado [`ModIncorporado`], que a
//!   tela traduz em «Perfis agora faz parte do SEELE (0.19)».
//!
//! Não é revogação (§6): o catálogo continua servindo o MOD a quem ainda está
//! num servidor 0.18. Quem recusa é só o servidor 0.19.

/// Os MODs incorporados ao produto: `(id, desde que versão do SEELE)`.
pub const INCORPORADOS: &[(&str, &str)] = &[("seele/perfis", "0.19")];

/// Um MOD que agora é parte do produto, e por isso foi recusado (ou desligado).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{id} agora faz parte do SEELE ({desde}) e não se liga mais como MOD")]
pub struct ModIncorporado {
    /// `autor/nome`.
    pub id: String,
    /// A versão do SEELE em que ele virou parte do produto.
    pub desde: &'static str,
}

/// Se `id` é um MOD incorporado, qual e desde quando.
#[must_use]
pub fn incorporado(id: &str) -> Option<ModIncorporado> {
    INCORPORADOS
        .iter()
        .find(|(incorporado, _)| *incorporado == id)
        .map(|(id, desde)| ModIncorporado {
            id: (*id).to_owned(),
            desde,
        })
}

/// **A conferência de quem aplica um conjunto de MODs**: o primeiro
/// incorporado da lista, como recusa.
///
/// # Errors
///
/// [`ModIncorporado`] quando algum dos `ids` é parte do produto. Nada deve ser
/// gravado nesse caso: a conferência vem antes da primeira escrita.
pub fn conferir_conjunto<'a>(ids: impl IntoIterator<Item = &'a str>) -> Result<(), ModIncorporado> {
    ids.into_iter().find_map(incorporado).map_or(Ok(()), Err)
}

/// **Na subida**: desliga cada incorporado que estiver ligado e devolve quais.
///
/// Chamada por `perfis::migracao::na_subida`, depois da migração. Uma falha do
/// banco é dita no registro e não impede a subida — se o MOD for de uma API
/// que esta linha não roda, o portão seguinte o nomeia.
pub fn desligar_os_ligados(persistence: &crate::persistence::Persistence) -> Vec<ModIncorporado> {
    let ligados = match crate::persistence::mods::enabled(persistence) {
        Ok(ligados) => ligados,
        Err(erro) => {
            tracing::error!(%erro, "não deu para ler os MODs ligados para desligar os incorporados");
            return Vec::new();
        }
    };
    let mut desligados = Vec::new();
    for ligado in ligados {
        let Some(recusa) = incorporado(&ligado.id) else {
            continue;
        };
        match crate::persistence::mods::disable(persistence, &ligado.id) {
            Ok(()) => {
                tracing::warn!(
                    mod_id = %ligado.id,
                    versao = %ligado.version,
                    desde = recusa.desde,
                    "MOD incorporado ao produto estava ligado e foi desligado; os dados dele \
                     ficam guardados, e o produto faz o que ele fazia"
                );
                desligados.push(recusa);
            }
            Err(erro) => tracing::error!(
                %erro,
                mod_id = %ligado.id,
                "o MOD incorporado ao produto continua ligado: o banco não aceitou desligá-lo"
            ),
        }
    }
    desligados
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::persistence::mods::{enable, enabled, EnabledMod};
    use crate::persistence::{Location, Persistence};

    fn linha(id: &str) -> EnabledMod {
        EnabledMod {
            id: id.to_owned(),
            version: "4.0.0".to_owned(),
            hash: "00".repeat(32),
            repo: "https://github.com/DATA-AND-DEV/PERFIS".to_owned(),
            reach: vec![],
            server_half: true,
            api: 6,
            tema: false,
        }
    }

    #[test]
    fn o_perfis_e_recusado_com_a_versao_em_que_virou_produto() {
        assert_eq!(
            conferir_conjunto(["seele/tema", "seele/perfis"]),
            Err(ModIncorporado {
                id: "seele/perfis".to_owned(),
                desde: "0.19"
            })
        );
        assert_eq!(conferir_conjunto(["seele/tema"]), Ok(()));
        assert_eq!(conferir_conjunto([]), Ok(()));
    }

    /// Religado no banco, sai desligado na subida — e só ele.
    #[test]
    fn religado_no_banco_o_incorporado_sai_desligado_e_os_outros_ficam() {
        let banco = Persistence::open(&Location::Memory).unwrap();
        enable(&banco, &linha("seele/perfis")).unwrap();
        enable(&banco, &linha("seele/tema")).unwrap();
        let desligados = desligar_os_ligados(&banco);
        assert_eq!(desligados.len(), 1);
        let ids: Vec<String> = enabled(&banco).unwrap().into_iter().map(|m| m.id).collect();
        assert_eq!(ids, ["seele/tema"]);
        assert!(
            desligar_os_ligados(&banco).is_empty(),
            "desligar de novo mudou algo"
        );
    }
}
