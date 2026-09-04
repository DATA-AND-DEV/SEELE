//! Length-prefixed framing over a QUIC stream.
//!
//! `specs/02-protocolo.md` puts control on "bidirectional stream #0, long
//! lived". A QUIC stream is a byte stream, so message boundaries have to be
//! written down: four bytes of big-endian length, then the frame.
//!
//! The length is checked against [`seele_proto::control::MAX_FRAME_LEN`] **before**
//! anything is allocated, per `specs/08-seguranca.md`. Reading a 4 GiB length
//! from an unauthenticated peer and reserving for it is the oldest denial of
//! service there is.

use anyhow::Result;
use seele_proto::control::{Validate, MAX_FRAME_LEN};
use serde::{Deserialize, Serialize};

/// Por que a leitura de um quadro parou.
///
/// # Por que os dois casos precisam de nomes diferentes
///
/// Eles voltavam iguais — um `anyhow::Error` cada — e o laço de controle os
/// tratava iguais: `debug!("o fluxo de controle do cliente terminou")` e sai. A
/// frase é verdadeira para o primeiro e mentirosa para o segundo, e as duas
/// consequências não têm nada a ver uma com a outra.
///
/// **Fechar é rotina.** Alguém apertou sair, a janela fechou, a máquina
/// hibernou. Não há o que dizer a ninguém.
///
/// **Não entender é incompatibilidade**, e é a falha mais cara que este produto
/// tem: o postcard indexa variante por posição e não é autodescritivo, então um
/// quadro que o par não conhece não é ignorado — ele desloca a leitura do fluxo
/// para sempre. Quem está do outro lado vê a sessão morrer em segundos, sem
/// mensagem. Foi relatado assim: «quem assiste com uma versão mais velha vê tela
/// preta, sem mensagem nenhuma, e a sessão morre em ~3 segundos sem dizer por
/// quê».
///
/// Distinguir é o que permite dizer a frase certa, registrar no nível certo, e
/// mandar ao outro lado uma razão em vez de um fio cortado.
#[derive(Debug)]
pub enum FimDoQuadro {
    /// O par fechou o fluxo. Fim normal de sessão.
    Fechou(anyhow::Error),
    /// Chegou um quadro inteiro e ele não decodificou.
    ///
    /// Quase sempre um par que fala outro vocabulário: uma variante que este
    /// build não tem, ou um campo que mudou de forma.
    NaoEntendi(anyhow::Error),
}

impl FimDoQuadro {
    /// O erro por dentro, para quem só quer registrá-lo.
    #[must_use]
    pub fn erro(&self) -> &anyhow::Error {
        match self {
            Self::Fechou(erro) | Self::NaoEntendi(erro) => erro,
        }
    }

    /// Se isto foi o par falando o que este build não entende.
    #[must_use]
    pub const fn e_incompatibilidade(&self) -> bool {
        matches!(self, Self::NaoEntendi(_))
    }
}

impl std::fmt::Display for FimDoQuadro {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fechou(erro) => write!(f, "o par fechou o fluxo de controle: {erro}"),
            Self::NaoEntendi(erro) => write!(
                f,
                "chegou um quadro de controle que este build não entende, o que \
                 quase sempre é um par de outra versão: {erro}"
            ),
        }
    }
}

/// Reads one frame and decodes it.
///
/// # Errors
///
/// Fails on a closed stream, an oversized length, or a malformed frame.
pub async fn read<T>(stream: &mut quinn::RecvStream) -> Result<T>
where
    T: for<'de> Deserialize<'de> + Validate,
{
    ler(stream).await.map_err(|fim| match fim {
        FimDoQuadro::Fechou(erro) | FimDoQuadro::NaoEntendi(erro) => erro,
    })
}

/// O mesmo, dizendo **qual** dos dois fins aconteceu.
///
/// [`read`] continua existindo para as oito chamadas a que a diferença não
/// importa — o aperto de mão, onde qualquer falha derruba a conexão do mesmo
/// jeito. Quem precisa dela é o laço de controle, que fica de pé por horas e é
/// onde um par de outra versão aparece.
///
/// # Errors
///
/// [`FimDoQuadro`], que nomeia o caso.
pub async fn ler<T>(stream: &mut quinn::RecvStream) -> Result<T, FimDoQuadro>
where
    T: for<'de> Deserialize<'de> + Validate,
{
    let mut length = [0_u8; 4];
    stream
        .read_exact(&mut length)
        .await
        .map_err(|erro| FimDoQuadro::Fechou(erro.into()))?;
    let length = u32::from_be_bytes(length) as usize;

    // Before allocating. specs/08-seguranca.md.
    if length > MAX_FRAME_LEN {
        return Err(FimDoQuadro::NaoEntendi(anyhow::anyhow!(
            "peer announced a {length}-byte control frame, over the {MAX_FRAME_LEN}-byte limit"
        )));
    }
    if length == 0 {
        return Err(FimDoQuadro::NaoEntendi(anyhow::anyhow!(
            "peer announced an empty control frame"
        )));
    }

    let mut frame = vec![0_u8; length];
    // **Aqui ainda é `Fechou`.** O comprimento chegou e o corpo não: o par
    // desligou no meio de um quadro, que é rotina numa queda de rede. Chamar
    // isso de incompatibilidade mandaria a pessoa trocar de versão para
    // resolver um cabo solto.
    stream
        .read_exact(&mut frame)
        .await
        .map_err(|erro| FimDoQuadro::Fechou(erro.into()))?;
    // E aqui é `NaoEntendi`: o quadro chegou inteiro e não virou mensagem.
    seele_proto::control::decode::<T>(&frame).map_err(|erro| FimDoQuadro::NaoEntendi(erro.into()))
}

/// Encodes a message and writes it as one frame.
///
/// # Errors
///
/// Fails if the message is invalid or the stream is closed.
pub async fn write<T>(stream: &mut quinn::SendStream, message: &T) -> Result<()>
where
    T: Serialize + Validate,
{
    let frame = seele_proto::control::encode(message)?;
    let length = u32::try_from(frame.len())?;
    stream.write_all(&length.to_be_bytes()).await?;
    stream.write_all(&frame).await?;
    Ok(())
}
