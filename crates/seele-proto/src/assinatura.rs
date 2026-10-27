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
//! | o `MORO2` do ENC/2 ([`moro2`]) | [`ROTULO_MORO2`] |
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
/// A mensagem do `MORO2` é [`moro2`], neste módulo, ao lado da prova.
pub const ROTULO_MORO2: &[u8] = b"SEELE-ENC/2 moro\0";

/// O rótulo da prova de identidade do cliente na linha 2 (0.17, contrato D5):
/// a prova passa a amarrar também o certificado DTLS de quem liga.
pub const ROTULO_PROVA_DE_CLIENTE_2: &[u8] = b"SEELE/2 prova-de-cliente\0";

/// O rótulo da apresentação da sala pessoal (0.17, contrato D3): a chave da
/// pessoa assina a impressão do certificado DTLS efêmero da sala.
pub const ROTULO_APRESENTACAO_DA_SALA: &[u8] = b"SEELE/2 apresentacao-da-sala\0";

/// Todos os rótulos, para o guarda de que eles não se encontram.
pub const ROTULOS: [&[u8]; 7] = [
    ROTULO_PROVA_DE_CLIENTE,
    ROTULO_MORO2,
    ROTULO_PROVA_DE_CLIENTE_2,
    ROTULO_APRESENTACAO_DA_SALA,
    crate::encontro::ROTULO_MORO3,
    ROTULO_CONVITE_PESSOAL,
    ROTULO_PEDIDO_PESSOAL,
];

/// O convite pessoal compartilhado fora do ponto de encontro.
pub const ROTULO_CONVITE_PESSOAL: &[u8] = b"SEELE-CHAMADA/1 convite\0";
/// Pedido pessoal vinculado ao ponto, às duas chaves e à chamada.
pub const ROTULO_PEDIDO_PESSOAL: &[u8] = b"SEELE-CHAMADA/1 pedido\0";

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

/// O que o cliente assina para provar quem é **a este servidor, nesta conexão**
/// (0.17, contrato D5).
///
/// `ROTULO_PROVA_DE_CLIENTE_2 ‖ impressao_servidor ‖ impressao_dtls_cliente ‖ nonce`:
///
/// - `impressao_servidor`: a impressão que o cliente conferiu, a do `fp=`. Na sala
///   pessoal, a impressão da sala.
/// - `impressao_dtls_cliente`: o SHA-256 do DER do certificado DTLS que **o
///   cliente** apresentou nesta conexão. O servidor a confere contra o
///   certificado que o DTLS mostrou: uma prova feita numa conexão não vale em
///   outra, nem repassada por quem estiver no meio com outro certificado.
/// - `nonce`: os 32 bytes do `Challenge`.
#[must_use]
pub fn prova_de_cliente_2(
    impressao_servidor: &[u8; IMPRESSAO_BYTES],
    impressao_dtls_cliente: &[u8; IMPRESSAO_BYTES],
    nonce: &[u8; NONCE_BYTES],
) -> Vec<u8> {
    let mut mensagem =
        Vec::with_capacity(ROTULO_PROVA_DE_CLIENTE_2.len() + 2 * IMPRESSAO_BYTES + NONCE_BYTES);
    mensagem.extend_from_slice(ROTULO_PROVA_DE_CLIENTE_2);
    mensagem.extend_from_slice(impressao_servidor);
    mensagem.extend_from_slice(impressao_dtls_cliente);
    mensagem.extend_from_slice(nonce);
    mensagem
}

/// O que a chave da sala pessoal assina na apresentação do enlace (a `0x03`
/// do `ctl`, contrato D3): `ROTULO_APRESENTACAO_DA_SALA ‖ impressao_dtls`.
///
/// A sala apresenta um certificado DTLS P-256 efêmero (o navegador não negocia
/// Ed25519 no DTLS), e esta assinatura amarra esse certificado à chave da
/// pessoa. Quem liga confere a assinatura com a chave que o código nomeia, e a
/// impressão com a do certificado que o DTLS mostrou, antes do `Hello`.
#[must_use]
pub fn apresentacao_da_sala(impressao_dtls: &[u8; IMPRESSAO_BYTES]) -> Vec<u8> {
    let mut mensagem = Vec::with_capacity(ROTULO_APRESENTACAO_DA_SALA.len() + IMPRESSAO_BYTES);
    mensagem.extend_from_slice(ROTULO_APRESENTACAO_DA_SALA);
    mensagem.extend_from_slice(impressao_dtls);
    mensagem
}

/// O que a chave da pessoa assina no `MORO2` do ENC/2 (ADR 0062).
///
/// `ROTULO_MORO2 ‖ n(marca) ‖ marca ‖ chave ‖ carimbo ‖ n(dicas) ‖ dicas`: os
/// dois tamanhos num byte, o carimbo em `u64` big-endian, e cada dica como a
/// família (`4` ou `6`), o IP cru e a porta big-endian. Todo campo de tamanho
/// variável vai com o tamanho dito, para que dois registros diferentes nunca
/// deem a mesma mensagem.
///
/// O endereço público de quem registra não entra: quem o diz é o ponto, que o
/// vê. A menor mensagem tem 60 bytes, então nenhuma cabe nos 32 de um nonce.
#[must_use]
pub fn moro2(
    marca: &crate::encontro::Marca,
    chave: &[u8; 32],
    carimbo: u64,
    dicas: &[std::net::SocketAddr],
) -> Vec<u8> {
    let marca = marca.texto().as_bytes();
    let mut mensagem = Vec::with_capacity(
        ROTULO_MORO2.len() + 1 + marca.len() + chave.len() + 8 + 1 + dicas.len() * 19,
    );
    mensagem.extend_from_slice(ROTULO_MORO2);
    // Uma marca tem no máximo `LIMITE_DA_MARCA` (32) caracteres, e o ENC/2 leva
    // no máximo `LIMITE_DE_DICAS` (2) dicas: os dois tamanhos cabem num byte, e
    // o recuo é inalcançável.
    mensagem.push(u8::try_from(marca.len()).unwrap_or(u8::MAX));
    mensagem.extend_from_slice(marca);
    mensagem.extend_from_slice(chave);
    mensagem.extend_from_slice(&carimbo.to_be_bytes());
    mensagem.push(u8::try_from(dicas.len()).unwrap_or(u8::MAX));
    for dica in dicas {
        match dica.ip() {
            std::net::IpAddr::V4(quatro) => {
                mensagem.push(4);
                mensagem.extend_from_slice(&quatro.octets());
            }
            std::net::IpAddr::V6(seis) => {
                mensagem.push(6);
                mensagem.extend_from_slice(&seis.octets());
            }
        }
        mensagem.extend_from_slice(&dica.port().to_be_bytes());
    }
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

    #[test]
    fn a_prova_2_e_rotulo_servidor_dtls_do_cliente_e_nonce_nessa_ordem() {
        let mensagem = prova_de_cliente_2(&[0xF0; 32], &[0xC1; 32], &[0x0A; 32]);
        let mut esperada = b"SEELE/2 prova-de-cliente\0".to_vec();
        esperada.extend_from_slice(&[0xF0; 32]);
        esperada.extend_from_slice(&[0xC1; 32]);
        esperada.extend_from_slice(&[0x0A; 32]);
        assert_eq!(mensagem, esperada);
        assert_ne!(
            mensagem,
            prova_de_cliente_2(&[0xF0; 32], &[0xC2; 32], &[0x0A; 32]),
            "outro certificado DTLS, outra mensagem"
        );
    }

    #[test]
    fn a_apresentacao_da_sala_e_rotulo_e_impressao() {
        let mut esperada = b"SEELE/2 apresentacao-da-sala\0".to_vec();
        esperada.extend_from_slice(&[7; 32]);
        assert_eq!(apresentacao_da_sala(&[7; 32]), esperada);
    }

    fn marca(texto: &str) -> crate::encontro::Marca {
        crate::encontro::Marca::nova(texto).expect("marca de teste")
    }

    fn de(texto: &str) -> std::net::SocketAddr {
        texto.parse().expect("endereço de teste")
    }

    #[test]
    fn a_mensagem_do_moro2_e_rotulo_marca_chave_carimbo_e_dicas_nessa_ordem() {
        let dicas = [de("192.168.0.10:8385"), de("[fd12::1]:8386")];
        let mensagem = moro2(&marca("pAs"), &[0xAB; 32], 0x0102_0304_0506_0708, &dicas);
        let mut esperada = b"SEELE-ENC/2 moro\0".to_vec();
        esperada.push(3);
        esperada.extend_from_slice(b"pAs");
        esperada.extend_from_slice(&[0xAB; 32]);
        esperada.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        esperada.push(2);
        esperada.extend_from_slice(&[4, 192, 168, 0, 10, 0x20, 0xC1]);
        esperada.push(6);
        esperada.extend_from_slice(&[0xfd, 0x12, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        esperada.extend_from_slice(&[0x20, 0xC2]);
        assert_eq!(mensagem, esperada);
    }

    #[test]
    fn cada_campo_do_moro2_muda_a_mensagem() {
        let dica = de("192.168.0.10:8385");
        let base = moro2(&marca("pAs"), &[1; 32], 10, &[dica]);
        assert_ne!(base, moro2(&marca("pAe"), &[1; 32], 10, &[dica]));
        assert_ne!(base, moro2(&marca("pAs"), &[2; 32], 10, &[dica]));
        assert_ne!(base, moro2(&marca("pAs"), &[1; 32], 11, &[dica]));
        assert_ne!(base, moro2(&marca("pAs"), &[1; 32], 10, &[]));
        assert_ne!(
            base,
            moro2(&marca("pAs"), &[1; 32], 10, &[de("192.168.0.10:8386")])
        );
    }

    /// A menor mensagem do `MORO2` passa de 32 bytes, então uma 0.15.2 que só
    /// assine nonce de 32 bytes nunca assina um `MORO2` (o achado A1 da F7).
    #[test]
    fn a_mensagem_do_moro2_nunca_cabe_em_32_bytes() {
        let menor = moro2(&marca("p"), &[0; 32], 0, &[]);
        assert_eq!(menor.len(), 60);
        assert!(menor.starts_with(ROTULO_MORO2));
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
