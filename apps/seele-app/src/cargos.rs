//! Os cargos na casca do desktop (ADR 0064): o corpo dos nove comandos de
//! cargos da tela (os envoltórios `#[tauri::command]` ficam no `main.rs`, onde
//! o guarda do frontend os lê), e o nome estável de cada recusa.
//!
//! Todo comando é de SESSÃO: a conexão vem pelo carimbo (`session.conexao`),
//! nunca pela vista, e as leituras voltam `NaSessao` (fronteiras.md §12). O
//! estado de cargos é o do `Room` daquela conexão — por sessão.
//!
//! Quem decide é o servidor. A prévia e a explicação usam a regra dele sobre o
//! último quadro, para a tela dizer «por quê» antes; a confirmação revalida
//! tudo do outro lado.

use seele_ffi::cargos::{
    CargosNaTela, PedidoDeCargo, Permissao, PermissaoExplicada, Previa, RecusaDeAcaoDeMod,
    RecusaDeCargo,
};
use seele_ffi::{Connection, ConnectionError};

/// Por que um comando de cargos não deu. Serializado como a tela espera:
/// `{"CargoRecusado": {"motivo": …}}`, `"SemQuadro"` ou `{"Conexao": …}`.
#[derive(Debug, serde::Serialize)]
pub(crate) enum FalhaDeCargo {
    /// A sessão não existe, saiu, ou o carimbo é de outra geração.
    Conexao(ConnectionError),
    /// A prévia antecipou a recusa que o servidor daria.
    CargoRecusado {
        /// O motivo, o mesmo do `ServerMessage::CargoRecusado`.
        motivo: RecusaDeCargo,
    },
    /// O quadro ainda não chegou: sem ele não há revisão nem explicação.
    SemQuadro,
}

impl From<ConnectionError> for FalhaDeCargo {
    fn from(erro: ConnectionError) -> Self {
        Self::Conexao(erro)
    }
}

/// `pedir_cargos`: devolve o que esta sessão sabe dos cargos.
///
/// **Só pergunta ao servidor enquanto não há quadro.** Depois do primeiro, o
/// servidor manda o novo sozinho a cada mudança, e a casca sabe pelo
/// `Event::CargosChanged { marca }` (e pelo `Snapshot::cargos_revision`) quando
/// reler. Perguntar a cada leitura era carga e corrida à toa (pedido do G: a
/// camada aberta relia a cada 4 s, e cada leitura ia ao servidor).
///
/// # Errors
///
/// A conexão.
pub(crate) fn pedir(conexao: &Connection) -> Result<CargosNaTela, ConnectionError> {
    let agora = conexao.cargos_na_tela();
    if agora.quadro.is_none() {
        conexao.pedir_cargos()?;
    }
    Ok(agora)
}

/// `explicar_permissoes`: por que `pessoa` tem, ou não, cada permissão.
///
/// # Errors
///
/// [`FalhaDeCargo::SemQuadro`] antes do primeiro quadro.
pub(crate) fn explicar(
    conexao: &Connection,
    pessoa: u64,
) -> Result<Vec<PermissaoExplicada>, FalhaDeCargo> {
    conexao
        .explicar_permissoes(pessoa)
        .ok_or(FalhaDeCargo::SemQuadro)
}

/// `prever_cargo`: quem ganha e quem perde o quê, ou a recusa que o servidor
/// daria — pela mesma conta do guarda do ganho efetivo.
///
/// # Errors
///
/// [`FalhaDeCargo::CargoRecusado`] com a recusa antecipada;
/// [`FalhaDeCargo::SemQuadro`] antes do primeiro quadro.
pub(crate) fn prever(
    conexao: &Connection,
    revisao: u64,
    pedido: &PedidoDeCargo,
) -> Result<Previa, FalhaDeCargo> {
    match conexao.prever_cargo(revisao, pedido) {
        Some(Ok(previa)) => Ok(previa),
        Some(Err(motivo)) => Err(FalhaDeCargo::CargoRecusado { motivo }),
        None => Err(FalhaDeCargo::SemQuadro),
    }
}

/// Os seis verbos de gestão: manda ao servidor com a revisão vista. A
/// resposta (o quadro novo, ou `CargoRecusado`) chega pelo evento da sessão.
///
/// # Errors
///
/// A conexão.
pub(crate) fn mandar(
    conexao: &Connection,
    revisao: u64,
    pedido: PedidoDeCargo,
) -> Result<(), FalhaDeCargo> {
    Ok(conexao.pedido_de_cargo(revisao, pedido)?)
}

/// Os pedidos dos comandos, montados dos argumentos da tela.
pub(crate) mod pedido {
    use super::{PedidoDeCargo, Permissao};

    pub(crate) fn criar(
        nome: String,
        concede: Vec<Permissao>,
        restringe: Vec<Permissao>,
    ) -> PedidoDeCargo {
        PedidoDeCargo::Criar {
            nome,
            concede,
            restringe,
        }
    }

    pub(crate) fn editar(
        cargo: u32,
        nome: String,
        concede: Vec<Permissao>,
        restringe: Vec<Permissao>,
    ) -> PedidoDeCargo {
        PedidoDeCargo::Editar {
            cargo: cargo.into(),
            nome,
            concede,
            restringe,
        }
    }

    pub(crate) fn mover(cargo: u32, posicao: u32) -> PedidoDeCargo {
        PedidoDeCargo::Mover {
            cargo: cargo.into(),
            posicao,
        }
    }

    pub(crate) fn apagar(cargo: u32) -> PedidoDeCargo {
        PedidoDeCargo::Apagar {
            cargo: cargo.into(),
        }
    }

    pub(crate) fn atribuir(pessoa: u64, cargo: u32) -> PedidoDeCargo {
        PedidoDeCargo::Atribuir {
            pessoa: pessoa.into(),
            cargo: cargo.into(),
        }
    }

    pub(crate) fn revogar(pessoa: u64, cargo: u32) -> PedidoDeCargo {
        PedidoDeCargo::Revogar {
            pessoa: pessoa.into(),
            cargo: cargo.into(),
        }
    }
}

/// O nome estável de cada recusa: a chave da frase em `frases.js`
/// (`CARGO_RECUSADO.<nome>`). Sem `_`: uma recusa nova não compila sem nome.
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "lido pelo guarda de frases do G (tests/cargos_ui.rs)"
    )
)]
pub(crate) const fn nome_da_recusa(recusa: &RecusaDeCargo) -> &'static str {
    match recusa {
        RecusaDeCargo::SemPermissao => "sem_permissao",
        RecusaDeCargo::CargoAcimaDeVoce => "cargo_acima_de_voce",
        RecusaDeCargo::PessoaAcimaDeVoce => "pessoa_acima_de_voce",
        RecusaDeCargo::Proprietario => "proprietario",
        RecusaDeCargo::NaoPodeConceder { .. } => "nao_pode_conceder",
        RecusaDeCargo::AtingeQuemEstaAcima { .. } => "atinge_quem_esta_acima",
        RecusaDeCargo::GanhoAlemDaSuaAutoridade { .. } => "ganho_alem_da_sua_autoridade",
        RecusaDeCargo::PermissaoDesconhecida { .. } => "permissao_desconhecida",
        RecusaDeCargo::CargoNaoExiste => "cargo_nao_existe",
        RecusaDeCargo::PessoaNaoExiste => "pessoa_nao_existe",
        RecusaDeCargo::CargoProtegido => "cargo_protegido",
        RecusaDeCargo::NomeInvalido => "nome_invalido",
        RecusaDeCargo::NomeTomado => "nome_tomado",
        RecusaDeCargo::LimiteDeCargos => "limite_de_cargos",
        RecusaDeCargo::PosicaoInvalida => "posicao_invalida",
        RecusaDeCargo::QuadroCheio => "quadro_cheio",
        RecusaDeCargo::FalhaDoServidor => "falha_do_servidor",
        RecusaDeCargo::RevisaoVelha { .. } => "revisao_velha",
    }
}

/// O nome estável de uma recusa de ação de MOD, o mesmo que o servidor escreve
/// no `ModReply` (`{"ok":false,"recusa":…}`).
#[cfg_attr(
    not(test),
    allow(dead_code, reason = "lido pelo executor de MOD da casca")
)]
pub(crate) const fn nome_da_recusa_de_mod(recusa: RecusaDeAcaoDeMod) -> &'static str {
    recusa.nome()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod testes {
    use super::*;

    #[test]
    fn a_falha_serializa_como_a_tela_espera() {
        let falha = FalhaDeCargo::CargoRecusado {
            motivo: RecusaDeCargo::RevisaoVelha { atual: 3 },
        };
        assert_eq!(
            serde_json::to_value(&falha).unwrap(),
            serde_json::json!({"CargoRecusado": {"motivo": {"RevisaoVelha": {"atual": 3}}}})
        );
        assert_eq!(
            serde_json::to_value(&FalhaDeCargo::SemQuadro).unwrap(),
            serde_json::json!("SemQuadro")
        );
    }

    #[test]
    fn os_pedidos_dos_comandos_sao_os_do_fio() {
        assert_eq!(
            pedido::atribuir(7, 2),
            PedidoDeCargo::Atribuir {
                pessoa: 7_u64.into(),
                cargo: 2_u32.into()
            }
        );
        assert_eq!(
            pedido::mover(2, 1),
            PedidoDeCargo::Mover {
                cargo: 2_u32.into(),
                posicao: 1
            }
        );
    }

    #[test]
    fn cada_recusa_tem_um_nome_proprio() {
        let todas = [
            RecusaDeCargo::SemPermissao,
            RecusaDeCargo::CargoAcimaDeVoce,
            RecusaDeCargo::PessoaAcimaDeVoce,
            RecusaDeCargo::Proprietario,
            RecusaDeCargo::CargoNaoExiste,
            RecusaDeCargo::PessoaNaoExiste,
            RecusaDeCargo::CargoProtegido,
            RecusaDeCargo::NomeInvalido,
            RecusaDeCargo::NomeTomado,
            RecusaDeCargo::LimiteDeCargos,
            RecusaDeCargo::PosicaoInvalida,
            RecusaDeCargo::QuadroCheio,
            RecusaDeCargo::FalhaDoServidor,
            RecusaDeCargo::RevisaoVelha { atual: 1 },
        ];
        let nomes: std::collections::BTreeSet<&str> = todas.iter().map(nome_da_recusa).collect();
        assert_eq!(nomes.len(), todas.len(), "duas recusas com a mesma frase");
        assert_eq!(
            nome_da_recusa_de_mod(RecusaDeAcaoDeMod::SemPermissao),
            "sem-permissao"
        );
    }
}
