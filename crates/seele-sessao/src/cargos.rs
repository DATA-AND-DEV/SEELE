//! Os cargos do lado de quem conversa (ADR 0064): o que cada SESSÃO sabe dos
//! cargos do servidor dela, e o que a tela precisa para explicar e pedir.
//!
//! # Por sessão, nunca global
//!
//! Um `RoleId` só tem sentido no servidor que o emitiu (E6, isolamento entre
//! servidores e sessões). Por isso o estado é um [`CargosDaSessao`] por sessão,
//! guardado com ela, e o [`CargosPorSessao`] é só um mapa — nenhum quadro de
//! uma sessão se aplica a outra, e descartar uma sessão descarta os cargos
//! dela.
//!
//! # Explicar não é decidir
//!
//! [`CargosDaSessao::explicar`] e [`CargosDaSessao::previa`] usam a MESMA regra
//! do servidor (`seele_proto::cargos`), para a tela dizer «por quê» antes de o
//! servidor responder. Quem decide é o servidor, e ele revalida tudo; a prévia
//! é informativa.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use seele_proto::cargos::Estado;
pub use seele_proto::cargos::{
    Altura, Atribuicao, Cargo, ChaveDeMod, EfeitoNaPessoa, Explicacao, Motivo, PedidoDeCargo,
    Permissao, PermissaoDeModNoQuadro, PermissaoExplicada, QuadroDeCargos, RecusaDeAcaoDeMod,
    RecusaDeCargo, SuasPermissoes, NOME_DO_TODOS, PERMISSOES_DO_PRODUTO, TODOS,
};
use seele_proto::control::{ClientMessage, ServerMessage};
use seele_proto::ids::PersonId;

/// O que uma mensagem de cargo do servidor diz à tela. Vai no evento da
/// sessão, sempre com o carimbo dela (`{sessao, geracao, evento}`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum EventoDeCargos {
    /// O quadro novo (resposta ao `PedirCargos`, logo depois do `Session`, ou
    /// difusão de uma mudança).
    Cargos {
        /// O quadro.
        quadro: QuadroDeCargos,
    },
    /// O que esta pessoa pode agora.
    SuasPermissoes {
        /// As permissões.
        suas: SuasPermissoes,
    },
    /// Um verbo de cargo desta sessão foi recusado. Nunca calado.
    CargoRecusado {
        /// O número desta recusa na sessão: sobe a cada uma, inclusive quando
        /// o motivo se repete — é por ele que a tela distingue «a mesma recusa
        /// de novo» de «nada aconteceu».
        seq: u64,
        /// O motivo. `RevisaoVelha` vem seguido do quadro de agora.
        motivo: RecusaDeCargo,
    },
}

/// Uma recusa de cargo com o seu número de ordem na sessão.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecusaNumerada {
    /// O número: 1 na primeira recusa desta sessão, e sobe sempre.
    pub seq: u64,
    /// O motivo.
    pub motivo: RecusaDeCargo,
}

/// O que uma sessão sabe dos cargos do servidor dela.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CargosDaSessao {
    quadro: Option<QuadroDeCargos>,
    suas: Option<SuasPermissoes>,
    ultima_recusa: Option<RecusaNumerada>,
    /// Sobe a cada mensagem de cargo aplicada (quadro, suas ou recusa). É o
    /// número que o retrato da sessão leva, para a tela reler os cargos só
    /// quando ele anda, em vez de perguntar em laço.
    marca: u64,
}

impl CargosDaSessao {
    /// Aplica uma mensagem do servidor. `Some` quando ela é de cargo, com o
    /// evento para a tela.
    pub fn aplicar(&mut self, mensagem: &ServerMessage) -> Option<EventoDeCargos> {
        let evento = match mensagem {
            ServerMessage::QuadroDeCargos(quadro) => {
                self.quadro = Some(quadro.clone());
                EventoDeCargos::Cargos {
                    quadro: quadro.clone(),
                }
            }
            ServerMessage::SuasPermissoes(suas) => {
                self.suas = Some(suas.clone());
                EventoDeCargos::SuasPermissoes { suas: suas.clone() }
            }
            ServerMessage::CargoRecusado { motivo } => {
                let seq = self
                    .ultima_recusa
                    .as_ref()
                    .map_or(0, |r| r.seq)
                    .saturating_add(1);
                self.ultima_recusa = Some(RecusaNumerada {
                    seq,
                    motivo: motivo.clone(),
                });
                EventoDeCargos::CargoRecusado {
                    seq,
                    motivo: motivo.clone(),
                }
            }
            _ => return None,
        };
        self.marca = self.marca.saturating_add(1);
        Some(evento)
    }

    /// O número que anda a cada mensagem de cargo: o `cargos_revision` do
    /// retrato da sessão.
    #[must_use]
    pub fn marca(&self) -> u64 {
        self.marca
    }

    /// O último quadro recebido.
    #[must_use]
    pub fn quadro(&self) -> Option<&QuadroDeCargos> {
        self.quadro.as_ref()
    }

    /// O que esta pessoa pode, como o servidor disse por último.
    #[must_use]
    pub fn suas(&self) -> Option<&SuasPermissoes> {
        self.suas.as_ref()
    }

    /// A última recusa de cargo desta sessão, com o número dela.
    #[must_use]
    pub fn ultima_recusa(&self) -> Option<&RecusaNumerada> {
        self.ultima_recusa.as_ref()
    }

    /// A revisão do último quadro: a que todo verbo de gestão manda.
    #[must_use]
    pub fn revisao(&self) -> Option<u64> {
        self.quadro.as_ref().map(|quadro| quadro.revisao)
    }

    /// Por que `pessoa` tem, ou não tem, `permissao` — pela regra do servidor,
    /// sobre o último quadro. `None` antes do primeiro quadro.
    #[must_use]
    pub fn explicar(&self, pessoa: PersonId, permissao: &Permissao) -> Option<Explicacao> {
        self.quadro
            .as_ref()
            .map(|quadro| Estado::do_quadro(quadro).explicar(pessoa, permissao))
    }

    /// A prévia de um verbo: o que o servidor provavelmente diria. Só
    /// informativa — a confirmação é a do servidor, sobre o estado de agora.
    /// `None` antes do primeiro quadro.
    #[must_use]
    pub fn previa(
        &self,
        autor: PersonId,
        pedido: &PedidoDeCargo,
    ) -> Option<Result<(), RecusaDeCargo>> {
        self.quadro.as_ref().map(|quadro| {
            Estado::do_quadro(quadro)
                .decidir(autor, quadro.revisao, pedido)
                .map(|_| ())
        })
    }

    /// A mensagem que pede este verbo, com a revisão do último quadro. `None`
    /// antes do primeiro quadro: sem revisão, não há o que mandar.
    #[must_use]
    pub fn comando(&self, pedido: PedidoDeCargo) -> Option<ClientMessage> {
        self.revisao()
            .map(|revisao| seele_proto::cargos::mensagem_do_pedido(revisao, pedido))
    }

    /// O estado do último quadro, sabendo de todos os `membros` deste servidor
    /// (o FORA DO AR do `Room`): sem eles, quem só tem o `@everyone` não
    /// aparece numa prévia.
    fn estado(&self, membros: &BTreeMap<PersonId, String>) -> Option<Estado> {
        self.quadro
            .as_ref()
            .map(|quadro| Estado::do_quadro(quadro).com_pessoas(membros.keys().copied()))
    }

    /// A prévia de um verbo com a revisão `revisao` que a tela viu: a recusa
    /// que o servidor daria, ou o ganho e a perda por pessoa. `None` antes do
    /// primeiro quadro. Só informativa.
    #[must_use]
    pub fn prever(
        &self,
        membros: &BTreeMap<PersonId, String>,
        autor: PersonId,
        revisao: u64,
        pedido: &PedidoDeCargo,
    ) -> Option<Result<Previa, RecusaDeCargo>> {
        self.estado(membros).map(|estado| {
            estado.prever(autor, revisao, pedido).map(|efeitos| Previa {
                pessoas: efeitos
                    .into_iter()
                    .map(|efeito| EfeitoNaTela {
                        pessoa: efeito.pessoa.get(),
                        apelido: membros.get(&efeito.pessoa).cloned(),
                        ganha: efeito.ganha,
                        perde: efeito.perde,
                        alem_do_autor: efeito.alem_do_autor,
                    })
                    .collect(),
            })
        })
    }

    /// Por que `pessoa` tem, ou não, cada permissão. `None` antes do primeiro
    /// quadro.
    #[must_use]
    pub fn explicar_tudo(
        &self,
        membros: &BTreeMap<PersonId, String>,
        pessoa: PersonId,
    ) -> Option<Vec<PermissaoExplicada>> {
        self.estado(membros)
            .map(|estado| estado.explicar_tudo(pessoa))
    }

    /// O que a tela de cargos lê: o quadro, as suas permissões e o apelido de
    /// cada pessoa que aparece no quadro (os do FORA DO AR, mais o roster).
    #[must_use]
    pub fn na_tela(&self, apelidos_conhecidos: &BTreeMap<PersonId, String>) -> CargosNaTela {
        let mut apelidos = BTreeMap::new();
        if let Some(quadro) = &self.quadro {
            let citadas = quadro
                .atribuicoes
                .iter()
                .map(|atribuicao| atribuicao.pessoa)
                .chain(quadro.proprietario);
            for pessoa in citadas {
                if let Some(apelido) = apelidos_conhecidos.get(&pessoa) {
                    apelidos.insert(pessoa.get(), apelido.clone());
                }
            }
        }
        CargosNaTela {
            quadro: self.quadro.clone(),
            suas: self.suas.clone(),
            apelidos,
            ultima_recusa: self.ultima_recusa.clone(),
            marca: self.marca,
        }
    }
}

/// A prévia de um verbo para a tela: quem ganha e quem perde o quê.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Previa {
    /// Só as pessoas cujo efetivo mudaria.
    pub pessoas: Vec<EfeitoNaTela>,
}

/// O efeito de um verbo numa pessoa, com o apelido para a tela.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EfeitoNaTela {
    /// A conta.
    pub pessoa: u64,
    /// O apelido, se esta sessão o conhece.
    pub apelido: Option<String>,
    /// O que ela passaria a poder.
    pub ganha: Vec<Permissao>,
    /// O que ela deixaria de poder.
    pub perde: Vec<Permissao>,
    /// Do que ganharia, o que o autor não tem — o que o servidor recusa.
    pub alem_do_autor: Vec<Permissao>,
}

/// O que `pedir_cargos` devolve à tela, por sessão.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargosNaTela {
    /// O último quadro, ou `None` antes do primeiro.
    pub quadro: Option<QuadroDeCargos>,
    /// O que esta pessoa pode, como o servidor disse por último.
    pub suas: Option<SuasPermissoes>,
    /// O apelido de cada pessoa citada no quadro (pelo id em número). Quem não
    /// aparece aqui a tela chama pelo número.
    pub apelidos: BTreeMap<u64, String>,
    /// A última recusa de cargo desta sessão (`RevisaoVelha` inclusive), com o
    /// número dela: a mesma recusa repetida chega com `seq` maior.
    pub ultima_recusa: Option<RecusaNumerada>,
    /// O `cargos_revision` do retrato no instante desta leitura.
    pub marca: u64,
}

/// Os cargos de cada sessão, pelo `SessaoId` (u64) do registro de sessões.
#[derive(Debug, Clone, Default)]
pub struct CargosPorSessao {
    sessoes: BTreeMap<u64, CargosDaSessao>,
}

impl CargosPorSessao {
    /// Aplica uma mensagem do servidor DESTA sessão.
    pub fn aplicar(&mut self, sessao: u64, mensagem: &ServerMessage) -> Option<EventoDeCargos> {
        if !matches!(
            mensagem,
            ServerMessage::QuadroDeCargos(_)
                | ServerMessage::SuasPermissoes(_)
                | ServerMessage::CargoRecusado { .. }
        ) {
            return None;
        }
        self.sessoes.entry(sessao).or_default().aplicar(mensagem)
    }

    /// Os cargos de uma sessão.
    #[must_use]
    pub fn da(&self, sessao: u64) -> Option<&CargosDaSessao> {
        self.sessoes.get(&sessao)
    }

    /// Esquece uma sessão que saiu.
    pub fn descartar(&mut self, sessao: u64) {
        self.sessoes.remove(&sessao);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod testes {
    use super::*;
    use seele_proto::control::Permission;
    use seele_proto::ids::RoleId;

    fn quadro(revisao: u64, nome: &str) -> QuadroDeCargos {
        QuadroDeCargos {
            cargos: vec![
                Cargo {
                    id: RoleId(1),
                    nome: nome.into(),
                    posicao: 2,
                    concede: vec![
                        Permissao::Produto(Permission::ManageRoles),
                        Permissao::Produto(Permission::Speak),
                    ],
                    restringe: vec![],
                },
                Cargo {
                    id: RoleId(2),
                    nome: "Silêncio".into(),
                    posicao: 1,
                    concede: vec![],
                    restringe: vec![Permissao::Produto(Permission::Speak)],
                },
                Cargo {
                    id: TODOS,
                    nome: NOME_DO_TODOS.into(),
                    posicao: 0,
                    concede: vec![Permissao::Produto(Permission::ReadChannel)],
                    restringe: vec![],
                },
            ],
            atribuicoes: vec![
                Atribuicao {
                    pessoa: PersonId(7),
                    cargos: vec![RoleId(1)],
                },
                Atribuicao {
                    pessoa: PersonId(8),
                    cargos: vec![RoleId(2)],
                },
            ],
            proprietario: None,
            de_mods: vec![],
            revisao,
            dono_declarado: false,
        }
    }

    #[test]
    fn o_quadro_de_uma_sessao_nunca_vale_para_outra() {
        let mut todas = CargosPorSessao::default();
        todas.aplicar(1, &ServerMessage::QuadroDeCargos(quadro(5, "Gestão A")));
        todas.aplicar(2, &ServerMessage::QuadroDeCargos(quadro(9, "Gestão B")));
        assert_eq!(todas.da(1).unwrap().revisao(), Some(5));
        assert_eq!(todas.da(2).unwrap().revisao(), Some(9));
        assert_eq!(
            todas.da(1).unwrap().quadro().unwrap().cargos[0].nome,
            "Gestão A"
        );
        // O comando de cada uma leva a revisão DELA.
        let pedido = PedidoDeCargo::Apagar { cargo: RoleId(2) };
        assert_eq!(
            todas.da(2).unwrap().comando(pedido.clone()),
            Some(ClientMessage::ApagarCargo {
                revisao: 9,
                cargo: RoleId(2)
            })
        );
        todas.descartar(1);
        assert!(todas.da(1).is_none());
        assert!(todas.da(2).is_some(), "descartar uma sessão levou a outra");
        // Uma mensagem que não é de cargo não cria sessão nenhuma.
        assert!(todas
            .aplicar(3, &ServerMessage::Pong { timestamp: 0 })
            .is_none());
        assert!(todas.da(3).is_none());
    }

    #[test]
    fn sem_quadro_nao_ha_comando_nem_explicacao() {
        let cargos = CargosDaSessao::default();
        assert!(cargos
            .comando(PedidoDeCargo::Apagar { cargo: RoleId(2) })
            .is_none());
        assert!(cargos
            .explicar(PersonId(7), &Permissao::Produto(Permission::Speak))
            .is_none());
    }

    #[test]
    fn a_explicacao_e_a_previa_seguem_a_regra_do_servidor() {
        let mut cargos = CargosDaSessao::default();
        cargos.aplicar(&ServerMessage::QuadroDeCargos(quadro(3, "Gestão")));
        let fala = Permissao::Produto(Permission::Speak);
        assert_eq!(
            cargos.explicar(PersonId(8), &fala).unwrap().motivo,
            Motivo::Restringida {
                por: vec![RoleId(2)]
            }
        );
        assert!(cargos.explicar(PersonId(7), &fala).unwrap().tem);
        // A pessoa 7 não pode conceder Banir; a prévia diz por quê.
        assert_eq!(
            cargos.previa(
                PersonId(7),
                &PedidoDeCargo::Criar {
                    nome: "x".into(),
                    concede: vec![Permissao::Produto(Permission::Ban)],
                    restringe: vec![]
                }
            ),
            Some(Err(RecusaDeCargo::NaoPodeConceder {
                permissao: Permissao::Produto(Permission::Ban)
            }))
        );
    }

    #[test]
    fn a_previa_diz_quem_ganha_e_perde_e_ve_quem_esta_fora_do_ar() {
        let mut cargos = CargosDaSessao::default();
        cargos.aplicar(&ServerMessage::QuadroDeCargos(quadro(3, "Gestão")));
        // A pessoa 9 não tem cargo nenhum: só o FORA DO AR a conhece.
        let membros: BTreeMap<PersonId, String> = [
            (PersonId(7), "ana".to_owned()),
            (PersonId(8), "bia".to_owned()),
            (PersonId(9), "caio".to_owned()),
        ]
        .into();
        // A gestora (7) atribui o Silêncio à 9: ela perde Falar? Não tinha. Nada muda.
        let efeitos = cargos
            .prever(
                &membros,
                PersonId(7),
                3,
                &PedidoDeCargo::Atribuir {
                    pessoa: PersonId(9),
                    cargo: RoleId(2),
                },
            )
            .unwrap()
            .unwrap();
        assert!(efeitos.pessoas.is_empty(), "{efeitos:?}");
        // A gestora (7) põe o Silêncio em si mesma: ela perde Falar, e é a
        // única atingida — a prévia diz quem e o quê, com o apelido.
        let efeitos = cargos
            .prever(
                &membros,
                PersonId(7),
                3,
                &PedidoDeCargo::Atribuir {
                    pessoa: PersonId(7),
                    cargo: RoleId(2),
                },
            )
            .unwrap()
            .unwrap()
            .pessoas;
        assert_eq!(efeitos.len(), 1, "{efeitos:?}");
        assert_eq!(efeitos[0].pessoa, 7);
        assert_eq!(efeitos[0].apelido.as_deref(), Some("ana"));
        assert_eq!(
            efeitos[0].perde,
            vec![Permissao::Produto(Permission::Speak)]
        );
        assert!(efeitos[0].ganha.is_empty() && efeitos[0].alem_do_autor.is_empty());
        // Revisão velha: a prévia diz o mesmo que o servidor diria.
        assert_eq!(
            cargos.prever(
                &membros,
                PersonId(7),
                2,
                &PedidoDeCargo::Revogar {
                    pessoa: PersonId(8),
                    cargo: RoleId(2)
                }
            ),
            Some(Err(RecusaDeCargo::RevisaoVelha { atual: 3 }))
        );
        // A explicação por pessoa traz todas as permissões do produto.
        assert_eq!(
            cargos.explicar_tudo(&membros, PersonId(9)).unwrap().len(),
            PERMISSOES_DO_PRODUTO.len()
        );
        // E os apelidos de quem aparece no quadro, inclusive fora do ar.
        let tela = cargos.na_tela(&membros);
        assert_eq!(tela.apelidos.get(&8), Some(&"bia".to_owned()));
        assert!(!tela.apelidos.contains_key(&9), "a 9 não aparece no quadro");
    }

    #[test]
    fn a_recusa_vira_evento_nomeado_e_fica_guardada() {
        let mut cargos = CargosDaSessao::default();
        let evento = cargos
            .aplicar(&ServerMessage::CargoRecusado {
                motivo: RecusaDeCargo::RevisaoVelha { atual: 4 },
            })
            .unwrap();
        let json = serde_json::to_value(&evento).unwrap();
        assert_eq!(json["tipo"], "cargo_recusado");
        assert_eq!(json["seq"], 1);
        assert_eq!(
            cargos.ultima_recusa().map(|r| (r.seq, r.motivo.clone())),
            Some((1, RecusaDeCargo::RevisaoVelha { atual: 4 }))
        );
    }

    /// **A mesma recusa repetida é outra recusa** (pedido do G): sem o número,
    /// duas `SemPermissao` seguidas deixavam a tela esperando por algo que já
    /// tinha chegado. E a marca anda a cada mensagem de cargo, nunca nas outras.
    #[test]
    fn a_mesma_recusa_repetida_tem_numero_novo_e_a_marca_anda_so_com_cargos() {
        let mut cargos = CargosDaSessao::default();
        assert_eq!(cargos.marca(), 0);
        let recusa = ServerMessage::CargoRecusado {
            motivo: RecusaDeCargo::SemPermissao,
        };
        cargos.aplicar(&recusa);
        let primeira = cargos.ultima_recusa().unwrap().seq;
        cargos.aplicar(&recusa);
        let segunda = cargos.ultima_recusa().unwrap().seq;
        assert!(
            segunda > primeira,
            "a recusa repetida não se distingue da anterior"
        );
        assert_eq!(cargos.marca(), 2);
        cargos.aplicar(&ServerMessage::Pong { timestamp: 0 });
        assert_eq!(
            cargos.marca(),
            2,
            "uma mensagem que não é de cargo mexeu na marca"
        );
        cargos.aplicar(&ServerMessage::QuadroDeCargos(quadro(1, "Gestão")));
        assert_eq!(cargos.marca(), 3);
        assert_eq!(cargos.na_tela(&BTreeMap::new()).marca, 3);
    }
}
