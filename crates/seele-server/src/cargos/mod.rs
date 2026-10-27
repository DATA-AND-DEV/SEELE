//! CARGOS — a autorização do servidor (ADR 0064).
//!
//! **Toda** pergunta «esta pessoa pode isto?» passa por aqui, no instante da
//! ação, lida do banco e nunca de um cache do aperto de mão
//! (`specs/08-seguranca.md`: o servidor negar é a segurança). A regra mora em
//! `seele_proto::cargos`; este módulo monta o estado do banco, decide e grava.
//!
//! # Falha de banco é negação
//!
//! As funções `bool` deste módulo leem um erro de banco como «não pode», e o
//! registram. Um servidor com o disco falhando não passa a liberar nada.
//!
//! # O contrato com quem chama
//!
//! A regra e as decisões estão no `docs/adr/0064-cargos-e-permissoes.md`. Os
//! ganchos ficam em `session.rs`, `mods/**` e `transfer.rs`.
//!
//! # Revalidar no ponto do efeito, e barato
//!
//! [`pode`] e [`autorizar_acao_de_mod`] leem só a pessoa perguntada
//! (`persistence::cargos::ler_estado_de`): os cargos, as atribuições dela, o
//! banimento e o dono — sem varrer as outras contas. Por isso o lugar certo
//! de chamá-los é **imediatamente antes do efeito**, e não antes de uma
//! espera: um `AcaoDeMod` que ficou na fila atrás de um MOD lento é
//! reautorizado depois de obter o turno, e uma tela em curso é reconferida no
//! [`EfeitoDeSeguranca`] do commit que tirou Transmitir tela.

use anyhow::Result;
use rusqlite::TransactionBehavior;
use seele_proto::cargos::{
    ChaveDeMod, Efetivas, Estado, PedidoDeCargo, PermissaoDeclarada, QuadroDeCargos,
    RecusaDeAcaoDeMod, RecusaDeCargo, SuasPermissoes,
};
use seele_proto::control::{ClientMessage, Permission, ServerMessage};
use seele_proto::ids::PersonId;

use crate::persistence::cargos as banco;
use crate::persistence::Persistence;

/// As permissões efetivas desta pessoa, agora.
///
/// # Errors
///
/// Se o banco não responder.
pub fn efetivas(persistence: &Persistence, pessoa: PersonId) -> Result<Efetivas> {
    Ok(banco::ler_estado_de(persistence.connection(), pessoa)?.efetivas(pessoa))
}

/// Se esta pessoa tem esta permissão do produto, agora. Erro de banco é não.
#[must_use]
pub fn pode(persistence: &Persistence, pessoa: PersonId, permissao: Permission) -> bool {
    match efetivas(persistence, pessoa) {
        Ok(efetivas) => efetivas.pode(permissao),
        Err(erro) => {
            tracing::warn!(%erro, pessoa = pessoa.get(), ?permissao, "cargos: o banco não respondeu; lido como negação");
            false
        }
    }
}

/// Se esta pessoa administra (Gerenciar servidor ou Administrador efetivos,
/// ou é a proprietária). É o que passa pelo cargo exigido de um canal.
#[must_use]
pub fn administra(persistence: &Persistence, pessoa: PersonId) -> bool {
    efetivas(persistence, pessoa).is_ok_and(|efetivas| efetivas.administra())
}

/// Se `quem` pode usar `permissao` (Kick, Ban, MovePerson, RemoveMessage
/// alheia) contra `alvo`, agora: a permissão E a hierarquia
/// (`Estado::pode_moderar`). Substitui o `moderavel` de `session.rs`.
#[must_use]
pub fn pode_moderar(
    persistence: &Persistence,
    quem: PersonId,
    alvo: PersonId,
    permissao: Permission,
) -> bool {
    match banco::ler_estado(persistence.connection()) {
        Ok(estado) => estado.pode_moderar(quem, alvo, permissao),
        Err(erro) => {
            tracing::warn!(%erro, "cargos: o banco não respondeu; moderação negada");
            false
        }
    }
}

/// O `SuasPermissoes` desta pessoa, para mandar logo depois do `Session` e a
/// cada mudança que a afete.
///
/// # Errors
///
/// Se o banco não responder.
pub fn suas(persistence: &Persistence, pessoa: PersonId) -> Result<SuasPermissoes> {
    Ok(efetivas(persistence, pessoa)?.suas())
}

/// O quadro de cargos deste servidor.
///
/// # Errors
///
/// Se o banco não responder.
pub fn quadro(persistence: &Persistence) -> Result<QuadroDeCargos> {
    Ok(banco::ler_estado(persistence.connection())?.quadro())
}

/// O que fazer depois de atender um verbo de cargo.
#[derive(Debug, Clone, PartialEq)]
pub struct Atendimento {
    /// O que vai SÓ para quem pediu: `CargoRecusado`, ou o `QuadroDeCargos`
    /// do `PedirCargos`. `None` quando o verbo foi aceito — aí a resposta é a
    /// difusão de baixo, que inclui quem pediu.
    pub resposta: Option<ServerMessage>,
    /// Numa recusa por revisão velha, o quadro de agora, para quem pediu
    /// recarregar. Mande depois da `resposta`.
    pub quadro_atual: Option<ServerMessage>,
    /// Num verbo aceito: o `QuadroDeCargos` novo, para TODAS as sessões
    /// deste servidor.
    pub difundir: Option<ServerMessage>,
    /// Num verbo aceito: as pessoas cujo efetivo mudou. Para cada sessão
    /// delas, mande `SuasPermissoes` de novo (`suas`) e recalcule o que estiver
    /// guardado (o `may_speak` da voz e da tela).
    pub afetadas: Vec<PersonId>,
}

impl Atendimento {
    fn so_para_quem_pediu(mensagem: ServerMessage) -> Self {
        Self {
            resposta: Some(mensagem),
            quadro_atual: None,
            difundir: None,
            afetadas: Vec::new(),
        }
    }
}

/// O que muda **na segurança** de uma pessoa atingida por um verbo de cargo —
/// o que o servidor tem de aplicar na hora, antes de avisar qualquer cliente.
///
/// É o GANCHO DE EFEITO DE SEGURANÇA (revisão do Codex): uma revogação de
/// Falar que só chegasse à voz depois de o `QuadroDeCargos` ser escrito no ctl
/// do cliente seria atrasada por um cliente que não lê o ctl. O gancho é
/// chamado logo depois do commit, com o efetivo NOVO, e quem o implementa (a
/// sessão e a sala de voz, no `session.rs`) cala a voz e para a tela dali
/// mesmo, independente das notificações.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EfeitoDeSeguranca {
    /// Quem.
    pub pessoa: PersonId,
    /// Se ela ainda pode falar (Speak). Falso: os datagramas dela param de ser
    /// encaminhados já.
    pub pode_falar: bool,
    /// Se ela ainda pode transmitir a tela (ShareScreen). Falso: a
    /// transmissão em curso para.
    pub pode_transmitir: bool,
    /// Se ela ainda pode estar numa sala de voz (EnterVoiceRoom).
    pub pode_entrar: bool,
    /// Se ela ainda pode escrever (WriteChannel).
    pub pode_escrever: bool,
}

/// Atende uma mensagem de cargo do fio. `None` se ela não é de cargo.
///
/// O mesmo de [`atender_com_efeito`], sem gancho de efeito de segurança. Quem
/// tem sessões vivas deve usar o outro.
pub fn atender(
    persistence: &mut Persistence,
    autor: PersonId,
    mensagem: &ClientMessage,
) -> Option<Atendimento> {
    atender_com_efeito(persistence, autor, mensagem, &mut |_| {})
}

/// Atende uma mensagem de cargo do fio. `None` se ela não é de cargo.
///
/// Os seis verbos de gestão rodam numa transação **`BEGIN IMMEDIATE`**: o
/// estado é lido, a autoridade de `autor` recalculada, a regra decidida e a
/// mudança gravada, tudo sobre o estado vigente e sem que outra escrita entre
/// no meio. Uma revisão velha, uma recusa da regra ou uma falha do banco não
/// gravam nada.
///
/// `ao_gravar` é chamado logo depois do commit, com o [`EfeitoDeSeguranca`] de
/// cada pessoa atingida, e ANTES de este `Atendimento` voltar — portanto antes
/// de qualquer quadro ir a qualquer cliente.
pub fn atender_com_efeito(
    persistence: &mut Persistence,
    autor: PersonId,
    mensagem: &ClientMessage,
    ao_gravar: &mut dyn FnMut(&[EfeitoDeSeguranca]),
) -> Option<Atendimento> {
    if matches!(mensagem, ClientMessage::PedirCargos) {
        return Some(match quadro(persistence) {
            Ok(quadro) => Atendimento::so_para_quem_pediu(ServerMessage::QuadroDeCargos(quadro)),
            Err(erro) => {
                tracing::warn!(%erro, "cargos: não consegui ler o quadro");
                Atendimento::so_para_quem_pediu(ServerMessage::CargoRecusado {
                    motivo: RecusaDeCargo::FalhaDoServidor,
                })
            }
        });
    }
    let (revisao, pedido) = seele_proto::cargos::pedido_do_fio(mensagem)?;
    Some(
        match executar_com_efeito(persistence, autor, revisao, &pedido, ao_gravar) {
            Ok((quadro, afetadas)) => Atendimento {
                resposta: None,
                quadro_atual: None,
                difundir: Some(ServerMessage::QuadroDeCargos(quadro)),
                afetadas,
            },
            Err(motivo) => {
                tracing::info!(autor = autor.get(), ?pedido, %motivo, "cargos: verbo recusado");
                let quadro_atual = matches!(motivo, RecusaDeCargo::RevisaoVelha { .. })
                    .then(|| quadro(persistence).ok().map(ServerMessage::QuadroDeCargos))
                    .flatten();
                Atendimento {
                    resposta: Some(ServerMessage::CargoRecusado { motivo }),
                    quadro_atual,
                    difundir: None,
                    afetadas: Vec::new(),
                }
            }
        },
    )
}

/// Decide e grava um verbo de gestão, numa transação imediata. Devolve o
/// quadro novo e quem teve o efetivo mudado.
///
/// # Errors
///
/// A [`RecusaDeCargo`]: a da regra, ou `FalhaDoServidor` quando o banco não
/// respondeu (e nada foi gravado).
pub fn executar(
    persistence: &mut Persistence,
    autor: PersonId,
    revisao: u64,
    pedido: &PedidoDeCargo,
) -> Result<(QuadroDeCargos, Vec<PersonId>), RecusaDeCargo> {
    executar_com_efeito(persistence, autor, revisao, pedido, &mut |_| {})
}

/// [`executar`], chamando `ao_gravar` logo depois do commit com o
/// [`EfeitoDeSeguranca`] de cada pessoa atingida (ver [`atender_com_efeito`]).
///
/// # Errors
///
/// Os mesmos de [`executar`]. Numa recusa, `ao_gravar` não é chamado.
pub fn executar_com_efeito(
    persistence: &mut Persistence,
    autor: PersonId,
    revisao: u64,
    pedido: &PedidoDeCargo,
    ao_gravar: &mut dyn FnMut(&[EfeitoDeSeguranca]),
) -> Result<(QuadroDeCargos, Vec<PersonId>), RecusaDeCargo> {
    let falha = |erro: anyhow::Error| {
        tracing::warn!(%erro, "cargos: o banco falhou no meio de um verbo; nada foi gravado");
        RecusaDeCargo::FalhaDoServidor
    };
    let transacao = persistence
        .connection_mut()
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|erro| falha(erro.into()))?;
    let antes = banco::ler_estado(&transacao).map_err(falha)?;
    // Um cargo que um canal exige não se apaga: soltaria a exigência.
    if let PedidoDeCargo::Apagar { cargo } = pedido {
        if banco::cargo_exigido_por_canal(&transacao, *cargo).map_err(falha)? {
            return Err(RecusaDeCargo::CargoProtegido);
        }
    }
    let mudanca = antes.decidir(autor, revisao, pedido)?;
    banco::gravar(&transacao, &mudanca).map_err(falha)?;
    let depois = banco::ler_estado(&transacao).map_err(falha)?;
    transacao.commit().map_err(|erro| falha(erro.into()))?;
    let afetadas = afetadas(&antes, &depois);
    // O efeito de segurança, já: antes de qualquer quadro sair.
    let efeitos: Vec<EfeitoDeSeguranca> = afetadas
        .iter()
        .map(|pessoa| {
            let agora = depois.efetivas(*pessoa);
            EfeitoDeSeguranca {
                pessoa: *pessoa,
                pode_falar: agora.pode(Permission::Speak),
                pode_transmitir: agora.pode(Permission::ShareScreen),
                pode_entrar: agora.pode(Permission::EnterVoiceRoom),
                pode_escrever: agora.pode(Permission::WriteChannel),
            }
        })
        .collect();
    if !efeitos.is_empty() {
        ao_gravar(&efeitos);
    }
    tracing::info!(
        autor = autor.get(),
        ?pedido,
        afetadas = afetadas.len(),
        revisao = depois.revisao,
        "cargos: verbo aceito"
    );
    Ok((depois.quadro(), afetadas))
}

/// Quem teve o efetivo (ou a altura) mudado entre dois estados.
fn afetadas(antes: &Estado, depois: &Estado) -> Vec<PersonId> {
    antes
        .pessoas
        .union(&depois.pessoas)
        .copied()
        .filter(|pessoa| {
            let (a, d) = (antes.efetivas(*pessoa), depois.efetivas(*pessoa));
            a.produto != d.produto || a.de_mods != d.de_mods || a.altura != d.altura
        })
        .collect()
}

/// O portão do despacho de `ClientMessage::AcaoDeMod`: só depois de `Ok` a
/// metade de servidor do MOD é chamada, com `contexto.acao = acao`.
///
/// Uma ação com forma inválida é «não declarada»: nada com essa forma pode ter
/// sido declarado.
///
/// # Errors
///
/// [`RecusaDeAcaoDeMod`]; erro de banco é `SemPermissao`.
pub fn autorizar_acao_de_mod(
    persistence: &Persistence,
    pessoa: PersonId,
    mod_id: &str,
    acao: &str,
) -> Result<(), RecusaDeAcaoDeMod> {
    let chave = ChaveDeMod {
        mod_id: mod_id.to_owned(),
        acao: acao.to_owned(),
    };
    if !chave.e_valida() {
        return Err(RecusaDeAcaoDeMod::NaoDeclarada);
    }
    match banco::ler_estado_de(persistence.connection(), pessoa) {
        Ok(estado) => estado.autorizar_acao_de_mod(pessoa, &chave),
        Err(erro) => {
            tracing::warn!(%erro, "cargos: o banco não respondeu; ação de MOD negada");
            Err(RecusaDeAcaoDeMod::SemPermissao)
        }
    }
}

/// As ações deste MOD que a pessoa tem agora — **informativo**, para o
/// contexto do MOD esconder botões. Não é o bloqueio: o bloqueio é
/// [`autorizar_acao_de_mod`].
#[must_use]
pub fn acoes_do_mod(persistence: &Persistence, pessoa: PersonId, mod_id: &str) -> Vec<String> {
    efetivas(persistence, pessoa)
        .map(|efetivas| efetivas.acoes_do_mod(mod_id))
        .unwrap_or_default()
}

/// Declara o proprietário pela impressão da chave dele (`key_fingerprint`).
/// Só a máquina que hospeda chama isto — o app, a sala pessoal, o `seeled`
/// pela config — e nunca um verbo do fio.
///
/// # Errors
///
/// Uma impressão inválida, ou o banco.
pub fn declarar_proprietario(persistence: &Persistence, impressao: &str) -> Result<()> {
    banco::declarar_proprietario(persistence.connection(), impressao)?;
    tracing::info!(
        impressao,
        "cargos: proprietário declarado pela máquina que hospeda"
    );
    Ok(())
}

/// O erro acionável de um servidor sem proprietário declarado, para o
/// `seeled` registrar no arranque. `None` quando há dono.
#[must_use]
pub fn aviso_sem_dono(persistence: &Persistence) -> Option<&'static str> {
    match banco::impressao_do_proprietario(persistence.connection()) {
        Ok(Some(_)) => None,
        _ => Some(
            // Só a flag: o `seeled` não lê arquivo de configuração nenhum
            // (`main.rs`), e mandar editar um seele.toml seria mandar a pessoa
            // a um lugar que não existe (achado 7 da conformidade).
            "nenhum proprietário declarado: ninguém tem a autoridade de dono neste servidor. \
             Suba de novo com `seeled --proprietario <impressão da sua chave>` \
             (64 hexadecimais: a impressão que a portaria mostra)",
        ),
    }
}

/// Grava as permissões que um MOD declara, no ato de habilitá-lo (o gancho de
/// `mods::enable`, do E).
///
/// # Errors
///
/// Uma declaração inválida, ou o banco.
pub fn gravar_permissoes_do_mod(
    persistence: &Persistence,
    mod_id: &str,
    declaradas: &[PermissaoDeclarada],
) -> Result<()> {
    banco::gravar_permissoes_do_mod(persistence.connection(), mod_id, declaradas)
}

#[cfg(test)]
mod testes;
