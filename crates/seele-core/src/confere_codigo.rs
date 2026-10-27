//! Quem liga confere o código **antes do `Hello`**.
//!
//! Spec de 23/09, «Quem liga confere o código», e o ADR 0060. Quem liga só sabe
//! o código; a sala é a chave Ed25519 da pessoa.
//!
//! # Desde a 0.17 (D3)
//!
//! O DTLS do WebRTC não negocia Ed25519, então a sala fecha o DTLS com um
//! certificado P-256 **efêmero** e, como primeira mensagem do `ctl` (a 0x03 do
//! enlace, a «apresentação»), manda a chave Ed25519 dela e uma assinatura sobre
//! a impressão desse certificado DTLS
//! (`seele_proto::assinatura::apresentacao_da_sala`). Este verificador:
//!
//! 1. lê a chave e confere a assinatura dela sobre a impressão que **o DTLS
//!    desta conexão** mostrou — nunca sobre uma que a sala diga: é isso que
//!    prova que quem tem a chave é quem está do outro lado deste DTLS, e que
//!    uma apresentação copiada de outra conexão não vale aqui;
//! 2. confere o código contra a chave;
//! 3. quando já houve uma ligação completa, confere o pino da chave inteira.
//!
//! Uma chave que não bate derruba a conexão antes do `Hello`: nada do que quem
//! liga diria chega a quem atendeu.
//!
//! # O que não é usado aqui
//!
//! O `PinStore` por `host:porta` (`crate::tofu`). Uma sala é achada pelo código,
//! em qualquer endereço, e o pino dela é a chave, sob `@CÓDIGO` — ver
//! [`chave_do_pino`].

use ed25519_dalek::{Signature, VerifyingKey};
use seele_proto::codigo::Codigo;

/// Os bytes da apresentação de uma sala: a chave Ed25519 e a assinatura.
pub const APRESENTACAO_DA_SALA_LEN: usize = 32 + 64;

/// Por que quem atendeu não é a sala deste código.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FalhaDoCodigo {
    /// A apresentação não traz uma chave Ed25519 que assine este DTLS: não é
    /// uma sala pessoal, ou é uma que não prova estar desta conexão.
    #[error("quem atendeu não apresentou a chave de uma pessoa; esperado {esperado}")]
    NaoEhChaveDePessoa {
        /// O código que quem liga digitou.
        esperado: Codigo,
    },
    /// A chave de quem atendeu tem outro código.
    #[error("quem atendeu não é o dono deste código: esperado {esperado}, veio {veio}")]
    CodigoNaoBate {
        /// O código que quem liga digitou.
        esperado: Codigo,
        /// O código da chave que quem atendeu apresentou.
        veio: Codigo,
    },
    /// O código bate, e a chave não é a que ficou pinada na primeira ligação.
    /// São 75 bits contra 256: é alguém que achou uma chave com o mesmo código.
    #[error("alguém tentou se passar pelo dono do código {codigo}")]
    SePassouPor {
        /// O código de quem se passou.
        codigo: Codigo,
    },
}

/// A chave sob a qual o pino de um código mora no arquivo de pins.
///
/// O `@` não aparece em `host:porta`, então um código nunca se confunde com um
/// endereço no mesmo arquivo.
#[must_use]
pub fn chave_do_pino(codigo: &Codigo) -> String {
    format!("@{codigo}")
}

/// Aceita só a sala deste código.
#[derive(Debug)]
pub struct ConfereCodigo {
    esperado: Codigo,
    /// A impressão da chave crua pinada sob [`chave_do_pino`], quando já houve
    /// uma ligação completa: `seele_proto::transport::key_fingerprint`.
    pino: Option<String>,
}

impl ConfereCodigo {
    /// Um verificador para a sala de `esperado`, com o pino lido por quem disca.
    #[must_use]
    pub fn novo(esperado: Codigo, pino: Option<String>) -> Self {
        Self { esperado, pino }
    }

    /// Confere a apresentação da sala contra a impressão DTLS **desta**
    /// conexão. Devolve a chave de quem atendeu, que é o que quem disca pina
    /// depois de uma ligação completa.
    ///
    /// # Errors
    ///
    /// [`FalhaDoCodigo`], dizendo qual dos três casos foi. Uma apresentação de
    /// tamanho errado, uma chave que não é ponto válido ou uma assinatura que
    /// não confere são todas [`FalhaDoCodigo::NaoEhChaveDePessoa`]: em nenhuma
    /// delas há uma pessoa provada do outro lado.
    pub fn confere_apresentacao(
        &self,
        apresentacao: &[u8],
        impressao_dtls: &[u8; 32],
    ) -> Result<[u8; 32], FalhaDoCodigo> {
        let nao_e = FalhaDoCodigo::NaoEhChaveDePessoa {
            esperado: self.esperado,
        };
        if apresentacao.len() != APRESENTACAO_DA_SALA_LEN {
            return Err(nao_e);
        }
        let (chave, assinatura) = apresentacao.split_at(32);
        let chave: [u8; 32] = chave.try_into().map_err(|_| nao_e.clone())?;
        let assinatura: [u8; 64] = assinatura.try_into().map_err(|_| nao_e.clone())?;
        let verificadora = VerifyingKey::from_bytes(&chave).map_err(|_| nao_e.clone())?;
        verificadora
            .verify_strict(
                &seele_proto::assinatura::apresentacao_da_sala(impressao_dtls),
                &Signature::from_bytes(&assinatura),
            )
            .map_err(|_| nao_e)?;
        self.confere_chave(chave)
    }

    /// O código e o pino contra uma chave já provada.
    fn confere_chave(&self, chave: [u8; 32]) -> Result<[u8; 32], FalhaDoCodigo> {
        let veio = Codigo::da_chave(&chave);
        if veio != self.esperado {
            return Err(FalhaDoCodigo::CodigoNaoBate {
                esperado: self.esperado,
                veio,
            });
        }
        // Depois da primeira ligação completa, a conferência é a chave
        // inteira, e não os 75 bits do código (ADR 0060, «Força»).
        if let Some(pino) = &self.pino {
            if *pino != seele_proto::transport::key_fingerprint(&chave) {
                return Err(FalhaDoCodigo::SePassouPor { codigo: veio });
            }
        }
        Ok(chave)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    const SEMENTE: [u8; 32] = [42; 32];

    /// A apresentação que a sala de `semente` manda para o DTLS `fp`.
    fn apresentacao(semente: [u8; 32], fp: &[u8; 32]) -> Vec<u8> {
        let chave = SigningKey::from_bytes(&semente);
        let assinatura = chave.sign(&seele_proto::assinatura::apresentacao_da_sala(fp));
        let mut bytes = chave.verifying_key().to_bytes().to_vec();
        bytes.extend_from_slice(&assinatura.to_bytes());
        bytes
    }

    fn codigo_de(semente: [u8; 32]) -> Codigo {
        Codigo::da_chave(&SigningKey::from_bytes(&semente).verifying_key().to_bytes())
    }

    #[test]
    fn a_sala_certa_sobre_este_dtls_passa() {
        let fp = [5; 32];
        assert_eq!(
            ConfereCodigo::novo(codigo_de(SEMENTE), None)
                .confere_apresentacao(&apresentacao(SEMENTE, &fp), &fp),
            Ok(SigningKey::from_bytes(&SEMENTE).verifying_key().to_bytes())
        );
    }

    #[test]
    fn a_apresentacao_de_outra_conexao_nao_vale_aqui() {
        // Quem está no meio repete a apresentação que a sala mandou numa
        // conexão dele com ela: a assinatura é sobre o DTLS de lá, e não deste.
        let de_la = [5; 32];
        let daqui = [6; 32];
        assert_eq!(
            ConfereCodigo::novo(codigo_de(SEMENTE), None)
                .confere_apresentacao(&apresentacao(SEMENTE, &de_la), &daqui),
            Err(FalhaDoCodigo::NaoEhChaveDePessoa {
                esperado: codigo_de(SEMENTE)
            }),
            "uma apresentação assinada para outro DTLS passou: quem está no meio se \
             passaria pela sala"
        );
    }

    #[test]
    fn o_codigo_errado_diz_os_dois() {
        let fp = [5; 32];
        let esperado = Codigo::da_chave(&[7; 32]);
        assert_eq!(
            ConfereCodigo::novo(esperado, None)
                .confere_apresentacao(&apresentacao(SEMENTE, &fp), &fp),
            Err(FalhaDoCodigo::CodigoNaoBate {
                esperado,
                veio: codigo_de(SEMENTE),
            })
        );
    }

    #[test]
    fn com_o_pino_da_mesma_chave_passa_e_com_o_de_outra_chave_se_passou_por() {
        let fp = [5; 32];
        let codigo = codigo_de(SEMENTE);
        let publica = SigningKey::from_bytes(&SEMENTE).verifying_key().to_bytes();
        let pino_certo = seele_proto::transport::key_fingerprint(&publica);
        assert_eq!(
            ConfereCodigo::novo(codigo, Some(pino_certo))
                .confere_apresentacao(&apresentacao(SEMENTE, &fp), &fp),
            Ok(publica)
        );
        let pino_de_outra = seele_proto::transport::key_fingerprint(&[9; 32]);
        assert_eq!(
            ConfereCodigo::novo(codigo, Some(pino_de_outra))
                .confere_apresentacao(&apresentacao(SEMENTE, &fp), &fp),
            Err(FalhaDoCodigo::SePassouPor { codigo }),
            "uma chave diferente da pinada passou só porque o código batia"
        );
    }

    #[test]
    fn um_seeled_que_se_apresenta_vazio_nao_e_uma_sala() {
        let codigo = codigo_de(SEMENTE);
        assert_eq!(
            ConfereCodigo::novo(codigo, None).confere_apresentacao(&[], &[5; 32]),
            Err(FalhaDoCodigo::NaoEhChaveDePessoa { esperado: codigo })
        );
    }

    #[test]
    fn o_pino_de_um_codigo_nao_se_confunde_com_um_endereco() {
        let codigo = codigo_de(SEMENTE);
        let chave = chave_do_pino(&codigo);
        assert!(chave.starts_with('@'), "{chave}");
        assert!(chave.ends_with(&codigo.to_string()), "{chave}");
    }
}
