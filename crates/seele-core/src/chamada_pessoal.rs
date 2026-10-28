//! Assinatura pessoal restrita aos domínios tipados do protocolo de chamadas.
//! A casca nunca recebe semente nem oferece bytes arbitrários ao assinador.
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use seele_proto::chamada::{self, Assinatura, Convite, Operacao, Pedido};
use serde::{Deserialize, Serialize};

/// A única entrada de assinatura exposta à casca.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Intencao {
    /// Capacidade emitida pela própria identidade.
    Convite(Convite),
    /// Envelope com papel, destinatário, época e prazo explícitos.
    Pedido(Box<Pedido>),
}
/// Recusa local; nunca causa fallback para transporte menos autenticado.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Falha {
    /// Forma, prazo, assinatura ou contexto inválido.
    Invalido,
    /// O envelope tenta falar por outra identidade.
    OutraIdentidade,
    /// Receber ligações foi desligado pela pessoa.
    Desligado,
    /// A chave não corresponde ao código ou ao pino existente.
    ChaveNaoConfere,
    /// O contato está bloqueado.
    Bloqueado,
    /// Já existe uma conversa ou campainha pessoal em curso.
    Ocupado,
    /// O armazenamento local não abriu ou não gravou.
    Armazenamento,
}
fn verificar(convite: &Convite, agora: u64) -> bool {
    let Ok(chave) = VerifyingKey::from_bytes(&convite.dono) else {
        return false;
    };
    chamada::validar_convite(convite, agora).is_ok()
        && !chave.is_weak()
        && chave
            .verify_strict(
                &chamada::mensagem_do_convite(convite),
                &Signature::from_bytes(&convite.assinatura.bytes()),
            )
            .is_ok()
}
/// Assina somente intenções válidas da identidade indicada.
///
/// # Errors
/// Recusa outro remetente, contexto inválido e admissão quando desligada.
pub fn assinar(
    chave: &SigningKey,
    intencao: &Intencao,
    agora: u64,
    receber: bool,
) -> Result<Assinatura, Falha> {
    let publica = chave.verifying_key().to_bytes();
    let bytes = match intencao {
        Intencao::Convite(convite) => {
            if convite.dono != publica {
                return Err(Falha::OutraIdentidade);
            }
            if !receber {
                return Err(Falha::Desligado);
            }
            chamada::validar_convite(convite, agora).map_err(|_| Falha::Invalido)?;
            chamada::mensagem_do_convite(convite)
        }
        Intencao::Pedido(pedido) => {
            if pedido.remetente != publica {
                return Err(Falha::OutraIdentidade);
            }
            chamada::validar_pedido(pedido, agora).map_err(|_| Falha::Invalido)?;
            if matches!(
                pedido.operacao,
                Operacao::Registrar { .. } | Operacao::Atender
            ) && !receber
            {
                return Err(Falha::Desligado);
            }
            if let Operacao::Registrar { convite } | Operacao::Chamar { convite, .. } =
                &pedido.operacao
            {
                if !verificar(convite, agora) {
                    return Err(Falha::Invalido);
                }
            }
            chamada::mensagem_do_pedido(pedido).map_err(|_| Falha::Invalido)?
        }
    };
    Ok(Assinatura::de_bytes(chave.sign(&bytes).to_bytes()))
}
/// Confere o código e o pino completo, usando o mesmo hash da sala nativa.
///
/// # Errors
/// Um código ilegível, outra chave ou chave fraca são recusados.
pub fn conferir_par(codigo: &str, publica: &[u8; 32], pino: Option<&str>) -> Result<String, Falha> {
    let codigo = seele_proto::codigo::Codigo::ler(codigo).map_err(|_| Falha::Invalido)?;
    let chave = VerifyingKey::from_bytes(publica).map_err(|_| Falha::ChaveNaoConfere)?;
    let impressao = seele_proto::transport::key_fingerprint(publica);
    if chave.is_weak() || !codigo.confere_chave(publica) || pino.is_some_and(|p| p != impressao) {
        return Err(Falha::ChaveNaoConfere);
    }
    Ok(impressao)
}
/// Destinatário cujo bloqueio impede admissão ou continuidade de mídia.
/// Terminais continuam permitidos para liberar o par já bloqueado.
#[must_use]
pub fn sujeito_ao_bloqueio(intencao: &Intencao) -> Option<[u8; 32]> {
    match intencao {
        Intencao::Pedido(p)
            if matches!(
                p.operacao,
                Operacao::Chamar { .. } | Operacao::Atender | Operacao::Sinal(_)
            ) =>
        {
            Some(p.destinatario)
        }
        _ => None,
    }
}
/// Os tipos de protocolo usados pela fronteira, sem uma segunda codificação.
pub use seele_proto::chamada::{
    Assinatura as Prova, Convite as ConvitePessoal, Pedido as PedidoPessoal,
};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signer_rejeita_outro_dono_prazo_e_preferencia() {
        let chave = SigningKey::from_bytes(&[1; 32]);
        let mut c = Convite {
            id: [8; 32],
            dono: chave.verifying_key().to_bytes(),
            expira_em: 1100,
            assinatura: Assinatura::default(),
        };
        let sig = assinar(&chave, &Intencao::Convite(c.clone()), 1000, true).unwrap();
        c.assinatura = sig;
        assert!(verificar(&c, 1000));
        assert_eq!(
            assinar(&chave, &Intencao::Convite(c.clone()), 1100, true),
            Err(Falha::Invalido)
        );
        assert_eq!(
            assinar(&chave, &Intencao::Convite(c.clone()), 1000, false),
            Err(Falha::Desligado)
        );
        c.dono = [2; 32];
        assert_eq!(
            assinar(&chave, &Intencao::Convite(c), 1000, true),
            Err(Falha::OutraIdentidade)
        );
    }
    #[test]
    fn codigo_nao_substitui_pino_completo() {
        let publica = SigningKey::from_bytes(&[3; 32]).verifying_key().to_bytes();
        let codigo = seele_proto::codigo::Codigo::da_chave(&publica).to_string();
        let pin = conferir_par(&codigo, &publica, None).unwrap();
        assert!(conferir_par(&codigo, &publica, Some(&pin)).is_ok());
        assert_eq!(
            conferir_par(&codigo, &publica, Some("outra")),
            Err(Falha::ChaveNaoConfere)
        );
        assert_eq!(
            conferir_par(&codigo, &[4; 32], None),
            Err(Falha::ChaveNaoConfere)
        );
    }
}
