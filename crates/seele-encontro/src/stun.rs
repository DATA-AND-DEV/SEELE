//! O STUN Binding do ponto (contrato da 0.17, D7 e D8): o `ONDE` no formato
//! que o navegador fala.
//!
//! O navegador não manda `ONDE`. Para saber o próprio endereço público (o
//! candidato «srflx» do ICE), ele manda um STUN Binding Request ao servidor
//! STUN que a página lhe der, e a página dá **este ponto**, o do `enc=` do
//! convite, e nenhum outro (D8). Assim nenhum terceiro aprende o metadado de quem
//! liga, e o ponto continua trocável.
//!
//! # Só o mínimo da RFC 8489
//!
//! Responde a um Binding Request (`0x0001`) com um Binding Success Response
//! (`0x0101`) com um só atributo, `XOR-MAPPED-ADDRESS`. Não autentica, não
//! assina (sem `MESSAGE-INTEGRITY` e sem `FINGERPRINT`: o navegador não os pede
//! a um servidor STUN sem credencial) e não conhece nenhum outro método.
//!
//! # A amplificação, dita
//!
//! É a única resposta do ponto maior que o pedido: 32 bytes para um IPv4, 44
//! para um IPv6, contra um pedido de pelo menos 20. O ganho chega a 2,2:1, e o
//! ponto o segura com três coisas: o balde próprio por prefixo
//! ([`crate::taxa::Classe::Stun`]), que cala o excesso; a recusa de origem em
//! porta abaixo de 1024, onde moram os serviços que refletem de volta; e a
//! recusa de pedido que não é exatamente um Binding Request bem formado.

use std::net::{IpAddr, SocketAddr};

/// O cookie mágico da RFC 5389, nos bytes 4 a 7 de toda mensagem STUN.
pub const COOKIE_MAGICO: u32 = 0x2112_A442;

/// O tamanho do cabeçalho STUN.
const CABECALHO: usize = 20;

/// O tipo do Binding Request.
const BINDING_REQUEST: u16 = 0x0001;
/// O tipo do Binding Success Response.
const BINDING_SUCCESS: u16 = 0x0101;
/// O atributo `XOR-MAPPED-ADDRESS`.
const XOR_MAPPED_ADDRESS: u16 = 0x0020;

/// Se o datagrama parece STUN pelo primeiro byte (RFC 7983: de 0 a 3).
#[must_use]
pub fn parece_stun(datagrama: &[u8]) -> bool {
    datagrama.first().is_some_and(|primeiro| *primeiro <= 3)
}

/// A resposta a um Binding Request vindo de `origem`, ou `None` para calar.
///
/// Cala para: menos de 20 bytes, outro tipo, cookie errado, comprimento que
/// não bate com o datagrama ou que não é múltiplo de 4, e origem em porta
/// abaixo de 1024.
#[must_use]
pub fn responder(datagrama: &[u8], origem: SocketAddr) -> Option<Vec<u8>> {
    if origem.port() < 1024 {
        return None;
    }
    let cabecalho = datagrama.get(..CABECALHO)?;
    let tipo = u16::from_be_bytes([*cabecalho.first()?, *cabecalho.get(1)?]);
    let comprimento = usize::from(u16::from_be_bytes([*cabecalho.get(2)?, *cabecalho.get(3)?]));
    let cookie = u32::from_be_bytes(cabecalho.get(4..8)?.try_into().ok()?);
    if tipo != BINDING_REQUEST
        || cookie != COOKIE_MAGICO
        || comprimento % 4 != 0
        || CABECALHO.checked_add(comprimento)? != datagrama.len()
    {
        return None;
    }
    let transacao = cabecalho.get(8..20)?;

    let porta = origem.port() ^ u16::try_from(COOKIE_MAGICO >> 16).ok()?;
    let mut atributo = Vec::with_capacity(20);
    atributo.push(0);
    match origem.ip().to_canonical() {
        IpAddr::V4(quatro) => {
            atributo.push(0x01);
            atributo.extend_from_slice(&porta.to_be_bytes());
            let x = u32::from(quatro) ^ COOKIE_MAGICO;
            atributo.extend_from_slice(&x.to_be_bytes());
        }
        IpAddr::V6(seis) => {
            atributo.push(0x02);
            atributo.extend_from_slice(&porta.to_be_bytes());
            let mut chave = [0_u8; 16];
            chave
                .get_mut(..4)?
                .copy_from_slice(&COOKIE_MAGICO.to_be_bytes());
            chave.get_mut(4..)?.copy_from_slice(transacao);
            for (byte, x) in seis.octets().iter().zip(chave) {
                atributo.push(byte ^ x);
            }
        }
    }

    let tamanho = u16::try_from(4 + atributo.len()).ok()?;
    let tamanho_do_atributo = u16::try_from(atributo.len()).ok()?;
    let mut resposta = Vec::with_capacity(CABECALHO + usize::from(tamanho));
    resposta.extend_from_slice(&BINDING_SUCCESS.to_be_bytes());
    resposta.extend_from_slice(&tamanho.to_be_bytes());
    resposta.extend_from_slice(&COOKIE_MAGICO.to_be_bytes());
    resposta.extend_from_slice(transacao);
    resposta.extend_from_slice(&XOR_MAPPED_ADDRESS.to_be_bytes());
    resposta.extend_from_slice(&tamanho_do_atributo.to_be_bytes());
    resposta.extend_from_slice(&atributo);
    Some(resposta)
}

#[cfg(test)]
mod testes {
    #![allow(
        clippy::expect_used,
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "num teste, o pânico é o relatório"
    )]

    use super::*;

    fn pedido(atributos: &[u8]) -> Vec<u8> {
        let mut p = vec![0x00, 0x01];
        p.extend_from_slice(&u16::try_from(atributos.len()).unwrap().to_be_bytes());
        p.extend_from_slice(&COOKIE_MAGICO.to_be_bytes());
        p.extend_from_slice(&[7; 12]);
        p.extend_from_slice(atributos);
        p
    }

    /// Lê o XOR-MAPPED-ADDRESS de uma resposta, como um cliente STUN o lê.
    fn endereco_da_resposta(resposta: &[u8]) -> SocketAddr {
        assert_eq!(&resposta[0..2], &[0x01, 0x01]);
        assert_eq!(&resposta[20..22], &[0x00, 0x20]);
        let familia = resposta[25];
        let porta = u16::from_be_bytes([resposta[26], resposta[27]]) ^ 0x2112;
        if familia == 1 {
            let x = u32::from_be_bytes(resposta[28..32].try_into().unwrap()) ^ COOKIE_MAGICO;
            SocketAddr::from((std::net::Ipv4Addr::from(x), porta))
        } else {
            let mut chave = [0_u8; 16];
            chave[..4].copy_from_slice(&COOKIE_MAGICO.to_be_bytes());
            chave[4..].copy_from_slice(&resposta[8..20]);
            let mut ip = [0_u8; 16];
            for i in 0..16 {
                ip[i] = resposta[28 + i] ^ chave[i];
            }
            SocketAddr::from((std::net::Ipv6Addr::from(ip), porta))
        }
    }

    #[test]
    fn o_binding_devolve_o_endereco_visto_nas_duas_familias() {
        for origem in ["198.51.100.7:51000", "[2001:db8::42]:3478"] {
            let origem: SocketAddr = origem.parse().unwrap();
            let resposta = responder(&pedido(&[]), origem).expect("responde");
            assert_eq!(endereco_da_resposta(&resposta), origem);
            assert_eq!(&resposta[8..20], &[7; 12], "a transação volta igual");
            assert_eq!(
                usize::from(u16::from_be_bytes([resposta[2], resposta[3]])) + 20,
                resposta.len()
            );
            // O ganho máximo é o dito no cabeçalho do módulo.
            assert!(resposta.len() <= 44);
        }
    }

    #[test]
    fn o_binding_com_atributos_tambem_responde_e_o_resto_cala() {
        let origem: SocketAddr = "198.51.100.7:51000".parse().unwrap();
        // Um SOFTWARE qualquer, de 8 bytes: o pedido é aceito e o atributo ignorado.
        assert!(responder(&pedido(&[0x80, 0x22, 0, 4, b'a', b'b', b'c', b'd']), origem).is_some());
        // Comprimento que não bate, que não é múltiplo de 4, tipo errado, cookie errado.
        let mut ruim = pedido(&[0, 0, 0, 0]);
        ruim.pop();
        assert!(responder(&ruim, origem).is_none());
        let mut ruim = pedido(&[]);
        ruim[3] = 2;
        assert!(responder(&ruim, origem).is_none());
        let mut ruim = pedido(&[]);
        ruim[1] = 0x11;
        assert!(responder(&ruim, origem).is_none());
        let mut ruim = pedido(&[]);
        ruim[4] = 0;
        assert!(responder(&ruim, origem).is_none());
        assert!(responder(&pedido(&[])[..19], origem).is_none());
        // Origem em porta de serviço não é respondida.
        assert!(responder(&pedido(&[]), "198.51.100.7:53".parse().unwrap()).is_none());
    }

    #[test]
    fn o_enc_nunca_parece_stun() {
        assert!(!parece_stun(b"SEELE-ENC/3 QUEM3 a"));
        assert!(parece_stun(&pedido(&[])));
    }
}
