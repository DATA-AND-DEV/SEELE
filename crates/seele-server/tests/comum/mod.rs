//! **O par cru dos testes de integração, sobre o enlace de verdade** (0.17).
//!
//! Até a 0.16 cada arquivo de `tests/` montava um cliente `quinn` com um
//! verificador que aceitava qualquer certificado. Na 0.17 o transporte é o
//! `seele-enlace`: aqui se disca o `seeled` em processo por ICE, DTLS e SCTP
//! reais, em laço local, com `Pino::Qualquer` — o que está sob teste nestes
//! arquivos é o servidor, e não o TOFU de quem liga.
//!
//! A prova do cliente é a D5: `prova_de_cliente_2` sobre a impressão do
//! servidor que o DTLS **desta** conexão mostrou, a impressão DTLS deste lado
//! e o nonce.

#![allow(
    dead_code,
    clippy::expect_used,
    reason = "cada arquivo de tests/ usa uma parte; num teste, o pânico é o relatório"
)]

use std::net::SocketAddr;

use anyhow::{Context, Result};
use seele_enlace::{Conexao, Discagem, Envio, Pino, Ponta, Recebe};

/// Uma conexão de quem liga, com o `ctl` na mão. A ponta tem de viver
/// enquanto a conexão for usada: ela é dona do socket.
pub(crate) struct Ligado {
    /// A ponta de quem liga.
    pub(crate) ponta: Ponta,
    /// A conexão.
    pub(crate) conexao: Conexao,
    /// O `ctl`, para escrever.
    pub(crate) envio: Envio,
    /// O `ctl`, para ler.
    pub(crate) recebe: Recebe,
}

/// A ponta de quem liga, na família do alvo.
///
/// # Errors
///
/// O socket que não abre.
pub(crate) async fn ponta_para(alvo: SocketAddr) -> Result<Ponta> {
    let local = if alvo.is_ipv6() {
        SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, 0))
    } else {
        SocketAddr::from(([127, 0, 0, 1], 0))
    };
    Ok(Ponta::abrir(local, None).await?)
}

/// Disca `endereco` e devolve a conexão com o `ctl` aberto.
///
/// # Errors
///
/// A discagem que falha.
pub(crate) async fn ligar(endereco: SocketAddr) -> Result<Ligado> {
    let ponta = ponta_para(endereco).await?;
    let conexao = ponta
        .discar(Discagem::nova(vec![endereco], Pino::Qualquer))
        .await
        .context("a discagem ao seeled em processo falhou")?;
    let (envio, recebe) = conexao.controle().context("o enlace não entregou o ctl")?;
    Ok(Ligado {
        ponta,
        conexao,
        envio,
        recebe,
    })
}

/// A mensagem que a prova assina (D5) para um `seeled`: a impressão do
/// certificado DTLS que o par mostrou nesta conexão.
#[must_use]
pub(crate) fn prova_amarrada(conexao: &Conexao, nonce: &[u8]) -> Vec<u8> {
    prova_para(&conexao.impressao_dtls_do_par(), conexao, nonce)
}

/// A mensagem que a prova assina com uma impressão de servidor dada (na
/// sala, o SHA-256 da chave dela).
#[must_use]
pub(crate) fn prova_para(
    impressao_do_servidor: &[u8; 32],
    conexao: &Conexao,
    nonce: &[u8],
) -> Vec<u8> {
    let nonce: [u8; 32] = nonce.try_into().expect("o desafio tem 32 bytes");
    seele_proto::assinatura::prova_de_cliente_2(
        impressao_do_servidor,
        &conexao.impressao_dtls_local(),
        &nonce,
    )
}
