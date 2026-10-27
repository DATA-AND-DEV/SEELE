//! **Um par cru pelo enlace da 0.17** (ADR 0063): quem liga, sem o `Client` do
//! produto, até a prova da chave.
//!
//! Até a 0.16 cada teste que precisava disto trazia um QUIC cru, com um
//! verificador de certificado que aceitava qualquer um (`AceitaQualquer`), e lia
//! a impressão do servidor da cadeia que o TLS entregou. Na 0.17, quem liga
//! disca pela `Ponta` do enlace com `Pino::Qualquer`, e a impressão sai do
//! próprio enlace:
//!
//! - **num `seeled`**, é a impressão do certificado DTLS que o par apresentou
//!   (`Conexao::impressao_dtls_do_par`), que é o `fp=` do convite;
//! - **numa sala pessoal**, é o SHA-256 da chave Ed25519 que a sala apresentou
//!   na mensagem `0x03` do `ctl` (D3), a mesma conta do
//!   `seele_core::client::conferir_quem_atendeu`.
//!
//! A prova é a D5: `prova_de_cliente_2(impressão, impressão DTLS desta ponta,
//! nonce)`. Nenhuma conferência do servidor é afrouxada: este par prova a chave
//! como o produto prova, só que sem o TOFU, porque o teste não é sobre ele.

#![allow(
    dead_code,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "um módulo de teste dividido por arquivos que usam partes diferentes dele"
)]

use std::net::SocketAddr;
use std::time::Duration;

use ed25519_dalek::{Signer, SigningKey};
use seele_enlace::{Conexao, Discagem, Envio, Pino, Ponta, Recebe};
use seele_proto::control::{ClientMessage, ServerMessage};

/// O prazo de uma discagem de teste: o aperto do enlace e folga.
pub(crate) const PRAZO: Duration = Duration::from_secs(10);

/// Um par cru, com o `ctl` aberto.
pub(crate) struct Par {
    /// A conexão, segura enquanto o par vive: largá-la fecha.
    pub(crate) conexao: Conexao,
    /// O lado de escrita do `ctl`.
    pub(crate) envio: Envio,
    /// O lado de leitura do `ctl`.
    pub(crate) recebe: Recebe,
    /// A ponta de quem liga, segura junto.
    pub(crate) ponta: Ponta,
}

impl Par {
    /// O próximo quadro do servidor.
    pub(crate) async fn proximo(&mut self) -> anyhow::Result<ServerMessage> {
        seele_server::frame::read::<ServerMessage>(&mut self.recebe).await
    }

    /// Escreve um quadro de cliente.
    pub(crate) async fn mandar(&mut self, mensagem: &ClientMessage) -> anyhow::Result<()> {
        seele_server::frame::write(&mut self.envio, mensagem).await
    }
}

/// Disca `endereco` com qualquer certificado e devolve a conexão já com o
/// `ctl` e a apresentação, sem mandar nada.
pub(crate) async fn discar(endereco: SocketAddr) -> (Ponta, Conexao) {
    // O bind acompanha o DESTINO: o loopback IPv4 fica como sempre foi
    // (127.0.0.1:0, o que os testes de conformidade usam); uma LAN IPv4 pede
    // 0.0.0.0:0, e um destino IPv6, [::1]:0 ou [::]:0. Um bind em 127.0.0.1 não
    // alcança um endereço de fora da máquina (SemResposta depois do prazo).
    let local = match endereco {
        SocketAddr::V4(v4) if v4.ip().is_loopback() => SocketAddr::from(([127, 0, 0, 1], 0)),
        SocketAddr::V4(_) => SocketAddr::from(([0, 0, 0, 0], 0)),
        SocketAddr::V6(v6) if v6.ip().is_loopback() => {
            SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, 0))
        }
        SocketAddr::V6(_) => SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, 0)),
    };
    let ponta = Ponta::abrir(local, None)
        .await
        .unwrap_or_else(|erro| panic!("a ponta de quem liga não abriu em {local}: {erro:?}"));
    let conexao = ponta
        .discar(Discagem {
            alvos: vec![endereco],
            pino: Pino::Qualquer,
            credencial: None,
            prazo: PRAZO,
            certificado: None,
        })
        .await
        .unwrap_or_else(|falha| panic!("a discagem para {endereco} falhou: {falha:?}"));
    (ponta, conexao)
}

/// A impressão que a prova de quem liga amarra: a da sala, se ela se
/// apresentou com uma chave, ou a do certificado DTLS do servidor.
#[must_use]
pub(crate) fn impressao_da_prova(conexao: &Conexao) -> [u8; 32] {
    match conexao.apresentacao().get(..32) {
        Some(chave) => seele_proto::transport::certificate_digest(chave),
        None => conexao.impressao_dtls_do_par(),
    }
}

/// A assinatura da prova D5 de `pessoa`, para este `nonce` e esta conexão.
#[must_use]
pub(crate) fn prova(pessoa: &SigningKey, conexao: &Conexao, nonce: &[u8]) -> Vec<u8> {
    let nonce: [u8; 32] = nonce.try_into().expect("o desafio tem 32 bytes");
    pessoa
        .sign(&seele_proto::assinatura::prova_de_cliente_2(
            &impressao_da_prova(conexao),
            &conexao.impressao_dtls_local(),
            &nonce,
        ))
        .to_bytes()
        .to_vec()
}

/// **Um anfitrião cru**: uma `Ponta` do enlace que atende com um certificado
/// DTLS P-256 efêmero e a apresentação vazia (a do `seeled`), em 127.0.0.1.
/// É o lugar do `quinn::Endpoint::server` dos servidores de mentira da 0.16:
/// quem o usa lê e escreve o `ctl` à mão.
pub(crate) async fn anfitriao() -> Ponta {
    let certificado =
        seele_enlace::CertificadoDtls::efemero().expect("um certificado P-256 efêmero");
    Ponta::abrir(
        SocketAddr::from(([127, 0, 0, 1], 0)),
        Some(seele_enlace::ConfigDoAnfitriao::novo(certificado)),
    )
    .await
    .expect("o anfitrião cru abre em 127.0.0.1")
}

/// A primeira conexão que chegou ao anfitrião, com o `ctl`.
pub(crate) async fn primeira_chegada(ponta: &Ponta) -> anyhow::Result<(Conexao, Envio, Recebe)> {
    let conexao = ponta
        .aceitar()
        .await
        .ok_or_else(|| anyhow::anyhow!("ninguém bateu"))?;
    let (envio, recebe) = conexao
        .controle()
        .ok_or_else(|| anyhow::anyhow!("o enlace não entregou o ctl"))?;
    Ok((conexao, envio, recebe))
}

/// Um quadro inteiro do `ctl`, sem decodificar: o prefixo e o corpo.
pub(crate) async fn ler_quadro(recebe: &mut Recebe) -> anyhow::Result<Vec<u8>> {
    let mut prefixo = [0_u8; seele_proto::quadro::PREFIXO_LEN];
    recebe
        .read_exact(&mut prefixo)
        .await
        .map_err(|falha| anyhow::anyhow!("o ctl não deu o prefixo: {falha:?}"))?;
    let mut quadro = vec![0_u8; seele_proto::quadro::comprimento(prefixo)?];
    recebe
        .read_exact(&mut quadro)
        .await
        .map_err(|falha| anyhow::anyhow!("o ctl não deu o quadro: {falha:?}"))?;
    Ok(quadro)
}

/// Escreve bytes já enquadrados no `ctl`.
pub(crate) async fn escrever(envio: &mut Envio, bytes: &[u8]) -> anyhow::Result<()> {
    envio
        .write_all(bytes)
        .await
        .map_err(|falha| anyhow::anyhow!("o ctl não aceitou a escrita: {falha:?}"))
}

/// O `Hello` de um par de teste.
#[must_use]
pub(crate) fn hello(pessoa: &SigningKey, apelido: &str, segredo: Option<String>) -> ClientMessage {
    ClientMessage::Hello {
        faixa: seele_proto::FAIXA,
        client: "par-cru".into(),
        nickname: apelido.into(),
        public_key: pessoa.verifying_key().to_bytes().to_vec(),
        join_secret: segredo,
        material_e2ee: None,
        apis_de_mod: seele_proto::mods::APIS_ACEITAS.to_vec(),
    }
}

/// Disca, manda o `Hello`, responde o `Challenge` com a prova, e para: o que o
/// servidor mandar depois é do teste.
pub(crate) async fn ate_a_prova(endereco: SocketAddr, pessoa: &SigningKey, apelido: &str) -> Par {
    let (ponta, conexao) = discar(endereco).await;
    let (envio, recebe) = conexao.controle().expect("o enlace entrega o ctl uma vez");
    let mut par = Par {
        conexao,
        envio,
        recebe,
        ponta,
    };
    par.mandar(&hello(pessoa, apelido, None)).await.unwrap();
    let ServerMessage::Challenge { nonce } = par.proximo().await.unwrap() else {
        panic!("o servidor não mandou Challenge");
    };
    let assinatura = prova(pessoa, &par.conexao, &nonce);
    par.mandar(&ClientMessage::Response { proof: assinatura })
        .await
        .unwrap();
    par
}
