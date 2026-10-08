//! O que a chave de identidade assina, montado num lugar só.
//!
//! A mesma chave Ed25519 da pessoa assina três coisas: a prova de identidade
//! que o cliente dá a um servidor (o S2), o certificado da sala pessoal e o
//! registro `MORO2` no ponto de encontro. **Cada uma tem um rótulo próprio**, e
//! este módulo é o único lugar que as monta (spec-mestre da 0.16, D-fio-3;
//! adendo de 08/10/2026, §9).
//!
//! # O cliente nunca assina bytes vindos do fio
//!
//! Até a 0.15, o cliente assinava o nonce cru que o servidor mandasse, de até
//! 256 bytes: um oráculo de assinatura. Uma prova feita para um servidor valia
//! para qualquer outro que mandasse o mesmo nonce. Agora a mensagem assinada é
//! o rótulo, a impressão do servidor que o cliente **conferiu** e o nonce, e a
//! prova só vale para quem tem aquela impressão.
//!
//! # Os quatro domínios da mesma chave
//!
//! | o que a chave assina | começa por |
//! |---|---|
//! | a prova do cliente | [`ROTULO_PROVA_DE_CLIENTE`] |
//! | o `MORO2` do ENC/2 | [`ROTULO_MORO2`] |
//! | o certificado da sala (`TBSCertificate`) | `0x30`, um `SEQUENCE` em DER |
//! | o CertificateVerify do TLS 1.3 | 64 vezes `0x20` |
//!
//! Os rótulos terminam em NUL e não têm NUL no meio, então nenhum é prefixo de
//! outro, e nenhum começa como as duas estruturas que o TLS e o X.509 assinam.
//! Preso em `os_dominios_da_mesma_chave_sao_disjuntos`.

/// O rótulo da prova de identidade do cliente (o S2).
pub const ROTULO_PROVA_DE_CLIENTE: &[u8] = b"SEELE/1 prova-de-cliente\0";

/// O rótulo do registro assinado do ponto de encontro (`MORO2`, ENC/2).
///
/// A mensagem do `MORO2` é montada pela F2, neste módulo, ao lado da prova:
/// o formato dos campos dela é decisão do ENC/2.
pub const ROTULO_MORO2: &[u8] = b"SEELE-ENC/2 moro\0";

/// Todos os rótulos, para o guarda de que eles não se encontram.
pub const ROTULOS: [&[u8]; 2] = [ROTULO_PROVA_DE_CLIENTE, ROTULO_MORO2];

/// Os bytes de uma impressão: o SHA-256 do DER do certificado de longa duração.
pub const IMPRESSAO_BYTES: usize = 32;

/// Os bytes do nonce do `Challenge`.
pub const NONCE_BYTES: usize = crate::control::NONCE_LEN;

/// O que o cliente assina para provar quem é **a este servidor**.
///
/// - `impressao`: os 32 bytes do SHA-256 do DER do certificado de longa
///   duração do servidor ([`crate::transport::certificate_digest`]), o mesmo
///   cujo hexadecimal vai no `fp=` do link. No nativo, sai do certificado que o
///   TLS aceitou naquela conexão; no navegador, do link.
/// - `nonce`: os 32 bytes do `Challenge`.
#[must_use]
pub fn prova_de_cliente(impressao: &[u8; IMPRESSAO_BYTES], nonce: &[u8; NONCE_BYTES]) -> Vec<u8> {
    let mut mensagem =
        Vec::with_capacity(ROTULO_PROVA_DE_CLIENTE.len() + IMPRESSAO_BYTES + NONCE_BYTES);
    mensagem.extend_from_slice(ROTULO_PROVA_DE_CLIENTE);
    mensagem.extend_from_slice(impressao);
    mensagem.extend_from_slice(nonce);
    mensagem
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prova_e_rotulo_impressao_e_nonce_nessa_ordem() {
        let impressao = [0xF0; 32];
        let nonce = [0x0A; 32];
        let mensagem = prova_de_cliente(&impressao, &nonce);
        let (rotulo, resto) = mensagem.split_at(ROTULO_PROVA_DE_CLIENTE.len());
        assert_eq!(rotulo, ROTULO_PROVA_DE_CLIENTE);
        let (do_servidor, do_desafio) = resto.split_at(32);
        assert_eq!(do_servidor, impressao);
        assert_eq!(do_desafio, nonce);
        assert_eq!(rotulo, b"SEELE/1 prova-de-cliente\0");
    }

    /// A mesma chave e o mesmo nonce, para dois servidores: duas mensagens.
    /// É o que faz uma prova valer só para quem o cliente conferiu.
    #[test]
    fn duas_impressoes_dao_duas_mensagens() {
        let nonce = [1; 32];
        assert_ne!(
            prova_de_cliente(&[2; 32], &nonce),
            prova_de_cliente(&[3; 32], &nonce)
        );
    }

    /// Os quatro espaços que a mesma chave Ed25519 assina não se encontram: a
    /// prova, o `MORO2`, o `TBSCertificate` da sala (DER, começa por `0x30`) e
    /// o CertificateVerify do TLS 1.3 (64 vezes `0x20`).
    #[test]
    fn os_dominios_da_mesma_chave_sao_disjuntos() {
        for rotulo in ROTULOS {
            assert_eq!(
                rotulo.last(),
                Some(&0),
                "um rótulo termina em NUL, e por isso nenhum é prefixo de outro"
            );
            assert!(
                !rotulo[..rotulo.len() - 1].contains(&0),
                "NUL só no fim do rótulo"
            );
            assert_ne!(
                rotulo.first(),
                Some(&0x20),
                "colide com o CertificateVerify do TLS 1.3"
            );
            assert_ne!(rotulo.first(), Some(&0x30), "colide com um TBSCertificate");
        }
        for (i, um) in ROTULOS.iter().enumerate() {
            for outro in &ROTULOS[i + 1..] {
                assert!(
                    !um.starts_with(outro) && !outro.starts_with(um),
                    "dois rótulos em que um é prefixo do outro"
                );
            }
        }
    }
}
