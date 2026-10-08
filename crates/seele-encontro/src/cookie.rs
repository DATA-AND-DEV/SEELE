//! O cookie do `MORO2` (D-enc-4, decidido pelo dono em 08/10/2026; ADR 0062).
//!
//! A prova de ida e volta. Um `MORO2` sem cookie, ou com um que não confere para
//! o IP de onde ele veio, ouve um `DESAFIO` com o cookie, sem que o ponto guarde
//! nada, e não gasta o limite. Só o `MORO2` que traz o cookie de volta passa ao
//! limite, à assinatura e ao quarto. Quem forja a origem não recebe o cookie do
//! IP que forja: ele não registra nada em nome desse IP, e não esvazia o balde
//! de registro dele.
//!
//! # O cookie
//!
//! Os 16 primeiros bytes do HMAC-SHA256 do segredo do ponto sobre
//! `"SEELE-ENC/2 cookie\0" ‖ família ‖ IP ‖ janela`, com a janela de [`JANELA`]
//! segundos em `u64` big-endian. Vale a janela corrente e a anterior: de dois a
//! quatro minutos. **Preso ao IP, e não à porta**: a sala o recebe pela escuta
//! de avisos, e o socket do servidor, que sai pelo mesmo IP público, usa o
//! mesmo.
//!
//! # O segredo
//!
//! Trinta e dois bytes sorteados do sistema quando o ponto sobe, só em memória:
//! nunca em disco, nunca no `--barulhento`. Reiniciar o ponto sorteia outro, e
//! os cookies de antes param de valer: cada sala ouve um `DESAFIO` no
//! reavivamento seguinte e manda de novo, com o cookie novo.

use std::net::IpAddr;

use seele_proto::encontro::BYTES_DO_COOKIE;
use sha2::{Digest, Sha256};

/// Quantos segundos tem uma janela do cookie. Ele vale a corrente e a
/// anterior.
pub const JANELA: u64 = 120;

/// O que distingue o HMAC do cookie de qualquer outro uso do mesmo segredo.
const ROTULO: &[u8] = b"SEELE-ENC/2 cookie\0";

/// O segredo do ponto, e o que se faz com ele.
pub struct Cookies {
    segredo: [u8; 32],
}

/// Sem o segredo: ele não aparece em log nenhum.
impl std::fmt::Debug for Cookies {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Cookies { segredo: «guardado» }")
    }
}

impl Default for Cookies {
    fn default() -> Self {
        Self::novo()
    }
}

impl Cookies {
    /// Um segredo sorteado do sistema.
    ///
    /// # Panics
    ///
    /// Se o sistema não der números aleatórios. Sem eles não há segredo, e um
    /// segredo previsível deixaria qualquer um forjar o cookie de qualquer IP;
    /// subir assim seria fingir a defesa. Linux, macOS e Windows sempre dão.
    #[must_use]
    pub fn novo() -> Self {
        let mut segredo = [0_u8; 32];
        if let Err(erro) = getrandom::fill(&mut segredo) {
            panic!(
                "o sistema não deu números aleatórios para o segredo dos cookies ({erro}): sem \
                 ele, qualquer um forjaria o cookie de qualquer IP"
            );
        }
        Self { segredo }
    }

    /// Com um segredo dado, para o teste.
    #[must_use]
    pub fn com_segredo(segredo: [u8; 32]) -> Self {
        Self { segredo }
    }

    /// O cookie deste IP na janela de `agora_unix`.
    #[must_use]
    pub fn emitir(&self, ip: IpAddr, agora_unix: u64) -> [u8; BYTES_DO_COOKIE] {
        self.na_janela(ip, agora_unix / JANELA)
    }

    /// Se este cookie é o deste IP, na janela corrente ou na anterior.
    ///
    /// A comparação não para no primeiro byte diferente: quanto ela leva não
    /// diz quantos bytes acertaram.
    #[must_use]
    pub fn confere(&self, ip: IpAddr, cookie: &[u8; BYTES_DO_COOKIE], agora_unix: u64) -> bool {
        let janela = agora_unix / JANELA;
        let corrente = iguais(&self.na_janela(ip, janela), cookie);
        let anterior = iguais(&self.na_janela(ip, janela.saturating_sub(1)), cookie);
        corrente | anterior
    }

    fn na_janela(&self, ip: IpAddr, janela: u64) -> [u8; BYTES_DO_COOKIE] {
        let mut mensagem = ROTULO.to_vec();
        match ip.to_canonical() {
            IpAddr::V4(quatro) => {
                mensagem.push(4);
                mensagem.extend_from_slice(&quatro.octets());
            }
            IpAddr::V6(seis) => {
                mensagem.push(6);
                mensagem.extend_from_slice(&seis.octets());
            }
        }
        mensagem.extend_from_slice(&janela.to_be_bytes());
        let completo = hmac_sha256(&self.segredo, &mensagem);
        let mut cookie = [0_u8; BYTES_DO_COOKIE];
        for (destino, byte) in cookie.iter_mut().zip(completo) {
            *destino = byte;
        }
        cookie
    }
}

/// Os bytes iguais, sem parar no primeiro diferente.
fn iguais(um: &[u8; BYTES_DO_COOKIE], outro: &[u8; BYTES_DO_COOKIE]) -> bool {
    um.iter()
        .zip(outro)
        .fold(0_u8, |diferenca, (a, b)| diferenca | (a ^ b))
        == 0
}

/// O HMAC-SHA256 da RFC 2104, sobre o `sha2`.
///
/// Escrito aqui porque o crate `hmac` não está na árvore, e são dez linhas: a
/// chave maior que o bloco vira o hash dela, e o resto é `H((K ⊕ opad) ‖ H((K ⊕
/// ipad) ‖ m))`. Os vetores da RFC 4231 o conferem.
fn hmac_sha256(chave: &[u8], mensagem: &[u8]) -> [u8; 32] {
    const BLOCO: usize = 64;
    let mut bloco = [0_u8; BLOCO];
    if chave.len() > BLOCO {
        for (destino, byte) in bloco.iter_mut().zip(Sha256::digest(chave)) {
            *destino = byte;
        }
    } else {
        for (destino, byte) in bloco.iter_mut().zip(chave) {
            *destino = *byte;
        }
    }
    let ipad: Vec<u8> = bloco.iter().map(|byte| byte ^ 0x36).collect();
    let opad: Vec<u8> = bloco.iter().map(|byte| byte ^ 0x5c).collect();
    let dentro = Sha256::new()
        .chain_update(&ipad)
        .chain_update(mensagem)
        .finalize();
    let fora = Sha256::new()
        .chain_update(&opad)
        .chain_update(dentro)
        .finalize();
    let mut saida = [0_u8; 32];
    for (destino, byte) in saida.iter_mut().zip(fora) {
        *destino = byte;
    }
    saida
}

#[cfg(test)]
mod testes {
    #![allow(
        clippy::expect_used,
        clippy::unwrap_used,
        reason = "num teste, o pânico é o relatório"
    )]

    use super::*;

    fn hex(texto: &str) -> Vec<u8> {
        (0..texto.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&texto[i..i + 2], 16).unwrap())
            .collect()
    }

    /// Os vetores da RFC 4231 (HMAC-SHA256): o 1, o 2, e o 6, com a chave
    /// maior que o bloco.
    #[test]
    fn o_hmac_e_o_da_rfc_4231() {
        assert_eq!(
            hmac_sha256(&[0x0b; 20], b"Hi There").to_vec(),
            hex("b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7")
        );
        assert_eq!(
            hmac_sha256(b"Jefe", b"what do ya want for nothing?").to_vec(),
            hex("5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843")
        );
        assert_eq!(
            hmac_sha256(
                &[0xaa; 131],
                b"Test Using Larger Than Block-Size Key - Hash Key First"
            )
            .to_vec(),
            hex("60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54")
        );
    }

    const AGORA: u64 = 1_791_000_000;

    fn ip(texto: &str) -> IpAddr {
        texto.parse().unwrap()
    }

    #[test]
    fn o_cookie_vale_no_mesmo_ip_na_janela_corrente_e_na_seguinte() {
        let cookies = Cookies::com_segredo([7; 32]);
        let dado = cookies.emitir(ip("203.0.113.7"), AGORA);
        assert!(cookies.confere(ip("203.0.113.7"), &dado, AGORA));
        assert!(
            cookies.confere(ip("203.0.113.7"), &dado, AGORA + JANELA),
            "o cookie venceu na janela seguinte à que o deu"
        );
    }

    #[test]
    fn o_cookie_vence_depois_de_duas_janelas() {
        let cookies = Cookies::com_segredo([7; 32]);
        let comeco = AGORA - AGORA % JANELA;
        let dado = cookies.emitir(ip("203.0.113.7"), comeco);
        assert!(
            !cookies.confere(ip("203.0.113.7"), &dado, comeco + 2 * JANELA),
            "um cookie de duas janelas atrás ainda valia"
        );
    }

    #[test]
    fn o_cookie_de_um_ip_nao_vale_noutro() {
        // É a prova de ida e volta inteira: quem forja a origem não recebe o
        // cookie do IP que forja, e o cookie que ele tem é o do IP dele.
        let cookies = Cookies::com_segredo([7; 32]);
        let dele = cookies.emitir(ip("198.51.100.66"), AGORA);
        assert!(!cookies.confere(ip("203.0.113.7"), &dele, AGORA));
        let seis = cookies.emitir(ip("2001:db8::1"), AGORA);
        assert!(!cookies.confere(ip("2001:db8::2"), &seis, AGORA));
    }

    #[test]
    fn o_ipv4_mapeado_e_o_ipv4_dele() {
        let cookies = Cookies::com_segredo([7; 32]);
        let dado = cookies.emitir(ip("203.0.113.7"), AGORA);
        assert!(cookies.confere(ip("::ffff:203.0.113.7"), &dado, AGORA));
    }

    #[test]
    fn outro_segredo_invalida_todo_cookie() {
        // O ponto reiniciado sorteia outro segredo: os cookies de antes param
        // de valer, e a sala ouve DESAFIO e pede um novo.
        let antes = Cookies::com_segredo([7; 32]);
        let depois = Cookies::com_segredo([8; 32]);
        let dado = antes.emitir(ip("203.0.113.7"), AGORA);
        assert!(!depois.confere(ip("203.0.113.7"), &dado, AGORA));
    }

    #[test]
    fn o_segredo_do_sistema_muda_a_cada_ponto() {
        let um = Cookies::novo().emitir(ip("203.0.113.7"), AGORA);
        let outro = Cookies::novo().emitir(ip("203.0.113.7"), AGORA);
        assert_ne!(um, outro, "dois pontos sortearam o mesmo segredo");
    }

    #[test]
    fn a_janela_e_de_dois_minutos() {
        assert_eq!(JANELA, 120);
    }
}
