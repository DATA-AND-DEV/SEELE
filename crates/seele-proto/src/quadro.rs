//! O enquadramento de 4 bytes de todo fluxo do SEELE: o comprimento em
//! big-endian, depois o quadro.
//!
//! `specs/02-protocolo.md` põe o controle num fluxo bidirecional de longa vida,
//! e um fluxo QUIC é uma sequência de bytes: a fronteira de cada mensagem tem de
//! ser escrita. O teto [`MAX_FRAME_LEN`] é conferido **antes** de qualquer
//! alocação (`specs/08-seguranca.md`): ler um comprimento de 4 GiB de um par não
//! autenticado e reservar para ele é a negação de serviço mais velha que existe.
//!
//! # Por que aqui, e sem E/S
//!
//! A regra morava duas vezes, no `seele-core` e no `seele-server`, porque o
//! ADR 0002 não deixa os dois dividirem um crate de transporte. O que eles
//! dividem é este crate, e a regra é dele: quantos bytes, em que ordem, com que
//! teto. A E/S do `quinn` continua em cada ponta.
//!
//! O motivo de ela ter saído agora é o celular (ADR 0057): o cliente web roda o
//! `seele-proto` em `wasm`, e o JavaScript só move bytes opacos. Quem recebe
//! pedaços de um fluxo da WebTransport precisa de [`Desenquadrador`], porque lá
//! não existe `read_exact`.

use crate::control::{ControlError, MAX_FRAME_LEN};

/// Bytes do prefixo de comprimento.
pub const PREFIXO_LEN: usize = 4;

/// O comprimento que o prefixo anuncia, conferido contra o teto **antes** de
/// qualquer alocação.
///
/// # Errors
///
/// [`ControlError::Empty`] para zero, e [`ControlError::TooLong`] acima de
/// [`MAX_FRAME_LEN`].
pub fn comprimento(prefixo: [u8; PREFIXO_LEN]) -> Result<usize, ControlError> {
    let len = u32::from_be_bytes(prefixo) as usize;
    if len == 0 {
        return Err(ControlError::Empty);
    }
    if len > MAX_FRAME_LEN {
        return Err(ControlError::TooLong { len });
    }
    Ok(len)
}

/// O prefixo e o quadro num `Vec` só, que é o que vai para o fluxo.
///
/// Um `write_all` só, e não dois: o prefixo e o corpo saem juntos, e um
/// cancelamento entre eles deixaria de existir de quem escreve.
///
/// # Errors
///
/// Os mesmos de [`comprimento`], aplicados ao quadro de saída: um quadro vazio
/// ou acima do teto não sai.
pub fn enquadrar(quadro: &[u8]) -> Result<Vec<u8>, ControlError> {
    let len =
        u32::try_from(quadro.len()).map_err(|_| ControlError::TooLong { len: quadro.len() })?;
    comprimento(len.to_be_bytes())?;
    let mut saida = Vec::with_capacity(PREFIXO_LEN + quadro.len());
    saida.extend_from_slice(&len.to_be_bytes());
    saida.extend_from_slice(quadro);
    Ok(saida)
}

/// Junta os bytes que chegam em pedaços e devolve quadros inteiros.
///
/// É o que um leitor sem `read_exact` precisa, como o da WebTransport. O
/// prefixo é conferido assim que os quatro bytes chegam, então o que fica
/// guardado nunca passa de um quadro inteiro mais o último pedaço empurrado.
#[derive(Debug, Default)]
pub struct Desenquadrador {
    pendente: Vec<u8>,
}

impl Desenquadrador {
    /// Acrescenta o que chegou.
    pub fn empurrar(&mut self, bytes: &[u8]) {
        self.pendente.extend_from_slice(bytes);
    }

    /// O próximo quadro inteiro, se ele já chegou.
    ///
    /// # Errors
    ///
    /// Os de [`comprimento`]. Depois de um erro a leitura está perdida, porque
    /// não há como saber onde o próximo quadro começa, e quem chamou deve
    /// fechar o fluxo.
    pub fn proximo(&mut self) -> Result<Option<Vec<u8>>, ControlError> {
        let Some(prefixo) = self.pendente.get(..PREFIXO_LEN) else {
            return Ok(None);
        };
        let mut lido = [0_u8; PREFIXO_LEN];
        lido.copy_from_slice(prefixo);
        let fim = PREFIXO_LEN + comprimento(lido)?;
        let Some(quadro) = self.pendente.get(PREFIXO_LEN..fim).map(<[u8]>::to_vec) else {
            return Ok(None);
        };
        self.pendente.drain(..fim);
        Ok(Some(quadro))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn o_prefixo_e_o_comprimento_em_big_endian() {
        assert_eq!(enquadrar(&[9, 11, 0]).unwrap(), vec![0, 0, 0, 3, 9, 11, 0]);
    }

    #[test]
    fn um_comprimento_acima_do_teto_e_recusado_antes_de_alocar() {
        let grande = u32::try_from(MAX_FRAME_LEN + 1).unwrap().to_be_bytes();
        assert_eq!(
            comprimento(grande),
            Err(ControlError::TooLong {
                len: MAX_FRAME_LEN + 1
            })
        );
        assert_eq!(comprimento([0, 0, 0, 0]), Err(ControlError::Empty));
        assert!(comprimento([0xFF; 4]).is_err());
    }

    #[test]
    fn um_quadro_vazio_ou_grande_demais_nao_sai() {
        assert_eq!(enquadrar(&[]), Err(ControlError::Empty));
        assert!(matches!(
            enquadrar(&vec![0; MAX_FRAME_LEN + 1]),
            Err(ControlError::TooLong { .. })
        ));
        assert!(enquadrar(&vec![0; MAX_FRAME_LEN]).is_ok());
    }

    #[test]
    fn o_desenquadrador_junta_pedacos_e_separa_quadros_colados() {
        let mut fluxo = enquadrar(&[9, 10, 1]).unwrap();
        fluxo.extend(enquadrar(&[9, 11, 0]).unwrap());
        let mut desenquadrador = Desenquadrador::default();
        let mut saiu = Vec::new();
        // Um byte por vez, que é o pior corte possível: um pedaço incompleto
        // não é erro, só ainda não é quadro.
        for byte in &fluxo {
            desenquadrador.empurrar(std::slice::from_ref(byte));
            while let Some(quadro) = desenquadrador.proximo().unwrap() {
                saiu.push(quadro);
            }
        }
        assert_eq!(saiu, vec![vec![9, 10, 1], vec![9, 11, 0]]);
    }

    #[test]
    fn o_desenquadrador_entrega_na_ordem_quando_tudo_chega_junto() {
        let mut fluxo = enquadrar(&[9, 10, 1]).unwrap();
        fluxo.extend(enquadrar(&[9, 11, 0]).unwrap());
        let mut desenquadrador = Desenquadrador::default();
        desenquadrador.empurrar(&fluxo);
        assert_eq!(desenquadrador.proximo().unwrap(), Some(vec![9, 10, 1]));
        assert_eq!(desenquadrador.proximo().unwrap(), Some(vec![9, 11, 0]));
        assert_eq!(desenquadrador.proximo().unwrap(), None);
    }

    #[test]
    fn um_prefixo_acima_do_teto_e_recusado_antes_de_o_corpo_chegar() {
        let mut desenquadrador = Desenquadrador::default();
        desenquadrador.empurrar(&u32::MAX.to_be_bytes());
        assert!(matches!(
            desenquadrador.proximo(),
            Err(ControlError::TooLong { .. })
        ));
    }

    proptest::proptest! {
        /// A superfície de bytes não confiáveis: nada aqui entra em pânico.
        #[test]
        fn o_desenquadrador_nunca_entra_em_panico(pedacos: Vec<Vec<u8>>) {
            let mut desenquadrador = Desenquadrador::default();
            for pedaco in &pedacos {
                desenquadrador.empurrar(pedaco);
                while let Ok(Some(_)) = desenquadrador.proximo() {}
            }
        }

        /// O que sai de `enquadrar` volta inteiro pelo desenquadrador, em
        /// qualquer corte.
        #[test]
        fn todo_quadro_enquadrado_volta_igual(
            quadro in proptest::collection::vec(proptest::num::u8::ANY, 1..64),
            corte in 0_usize..80,
        ) {
            let fluxo = enquadrar(&quadro).unwrap();
            let corte = corte.min(fluxo.len());
            let (antes, depois) = fluxo.split_at(corte);
            let mut desenquadrador = Desenquadrador::default();
            desenquadrador.empurrar(antes);
            let cedo = desenquadrador.proximo().unwrap();
            desenquadrador.empurrar(depois);
            let tarde = desenquadrador.proximo().unwrap();
            proptest::prop_assert_eq!(cedo.or(tarde), Some(quadro));
        }
    }
}
