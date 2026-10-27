//! Length-prefixed framing over a channel of the link.
//!
//! `specs/02-protocolo.md` puts control on one long-lived ordered stream: since
//! 0.17 that is the `ctl` channel of `seele-enlace`. The channel is a byte
//! stream (the envelope's data messages concatenate), so message boundaries
//! have to be written down: four bytes of big-endian length, then the frame.
//!
//! **A regra mora em [`seele_proto::quadro`]**, e aqui fica só a E/S do
//! enlace e a distinção entre [`FimDoQuadro::Fechou`] e
//! [`FimDoQuadro::NaoEntendi`], que é do servidor.
//!
//! The length is checked against [`seele_proto::control::MAX_FRAME_LEN`] **before**
//! anything is allocated, per `specs/08-seguranca.md`. Reading a 4 GiB length
//! from an unauthenticated peer and reserving for it is the oldest denial of
//! service there is.

use anyhow::Result;
use seele_proto::control::{ControlError, Lido, ListaDeCima, Validate, MAX_FRAME_LEN};
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
pub async fn read<T>(stream: &mut crate::transporte::Recebe) -> Result<T>
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
pub async fn ler<T>(stream: &mut crate::transporte::Recebe) -> Result<T, FimDoQuadro>
where
    T: for<'de> Deserialize<'de> + Validate,
{
    let frame = ler_bruto(stream).await?;
    // E aqui é `NaoEntendi`: o quadro chegou inteiro e não virou mensagem.
    seele_proto::control::decode::<T>(&frame).map_err(|erro| FimDoQuadro::NaoEntendi(erro.into()))
}

/// Como [`ler`], para uma das listas de cima: um ordinal que esta build não
/// conhece vira [`Lido::Desconhecida`] em vez de [`FimDoQuadro::NaoEntendi`].
///
/// É o laço de controle que usa isto. Um ordinal conhecido com o corpo errado
/// continua `NaoEntendi`: aquilo não é um vocabulário mais novo, é um quadro
/// estragado.
///
/// Devolve também o **ordinal** do quadro, que é como uma recusa diz qual
/// pedido não deu (`AlertReason::PedidoRecusado`).
///
/// # Errors
///
/// [`FimDoQuadro`], que nomeia o caso.
pub async fn ler_da_lista<T: ListaDeCima>(
    stream: &mut crate::transporte::Recebe,
) -> Result<(u32, Lido<T>), FimDoQuadro> {
    let frame = ler_bruto(stream).await?;
    let lido = seele_proto::control::decode_da_lista::<T>(&frame)
        .map_err(|erro| FimDoQuadro::NaoEntendi(erro.into()))?;
    // Um quadro que decodificou tem ordinal: `decode_da_lista` acabou de lê-lo.
    let ordinal = seele_proto::control::ordinal_do_quadro(&frame).unwrap_or_default();
    Ok((ordinal, lido))
}

/// Um quadro inteiro, com o carimbo no byte 0, **sem decodificar**.
///
/// É o que deixa o aperto de mão olhar o carimbo antes do `decode`, que
/// recusaria um número fora da faixa sem dizer qual veio — e a recusa por
/// versão precisa dele para sair num carimbo que o par lê
/// (`control::recusa_por_versao`). E é o que deixa o laço de controle pular
/// uma mensagem de um vocabulário mais novo (`control::decode_da_lista`).
///
/// Nunca devolve um quadro vazio: o comprimento zero é recusado antes.
///
/// # Errors
///
/// [`FimDoQuadro::Fechou`] se o fluxo acabou, inclusive no meio de um quadro;
/// [`FimDoQuadro::NaoEntendi`] para um comprimento vazio ou acima do teto.
pub async fn ler_bruto(stream: &mut crate::transporte::Recebe) -> Result<Vec<u8>, FimDoQuadro> {
    let mut prefixo = [0_u8; seele_proto::quadro::PREFIXO_LEN];
    stream
        .read_exact(&mut prefixo)
        .await
        .map_err(|erro| FimDoQuadro::Fechou(erro.into()))?;

    // Before allocating. specs/08-seguranca.md.
    let length = seele_proto::quadro::comprimento(prefixo).map_err(|erro| {
        FimDoQuadro::NaoEntendi(match erro {
            ControlError::Empty => anyhow::anyhow!("peer announced an empty control frame"),
            _ => anyhow::anyhow!(
                "peer announced a {}-byte control frame, over the {MAX_FRAME_LEN}-byte limit",
                u32::from_be_bytes(prefixo)
            ),
        })
    })?;

    let mut frame = vec![0_u8; length];
    // **Aqui ainda é `Fechou`.** O comprimento chegou e o corpo não: o par
    // desligou no meio de um quadro, que é rotina numa queda de rede. Chamar
    // isso de incompatibilidade mandaria a pessoa trocar de versão para
    // resolver um cabo solto.
    stream
        .read_exact(&mut frame)
        .await
        .map_err(|erro| FimDoQuadro::Fechou(erro.into()))?;
    Ok(frame)
}

/// Encodes a message and writes it as one frame.
///
/// Carimbado com o número **desta build**. Serve a quem não negociou nada: os
/// fluxos de anexo e de imagem, e os pares crus dos testes. **O fluxo de
/// controle de uma sessão escreve com [`write_em`]**, no número que o `Hello`
/// negociou (adendo de 08/10/2026, §4).
///
/// # Errors
///
/// Fails if the message is invalid or the stream is closed.
pub async fn write<T>(stream: &mut crate::transporte::Envio, message: &T) -> Result<()>
where
    T: Serialize + Validate,
{
    write_em(stream, seele_proto::version::PROTOCOL_VERSION, message).await
}

/// Como [`write`], com o carimbo escolhido por quem escreve.
///
/// Depois do `Hello`, é o número negociado; antes, o carimbo que o par mandou,
/// que é o que ele lê. Na 1.0 os dois são 9 e o quadro é o mesmo de [`write`]:
/// a diferença aparece na primeira 1.x, que fala também um número que a 1.0
/// não lê.
///
/// # Errors
///
/// Fails if the message is invalid or the stream is closed.
pub async fn write_em<T>(
    stream: &mut crate::transporte::Envio,
    versao: u8,
    message: &T,
) -> Result<()>
where
    T: Serialize + Validate,
{
    let frame = seele_proto::control::encode_em(versao, message)?;
    stream
        .write_all(&seele_proto::quadro::enquadrar(&frame)?)
        .await?;
    Ok(())
}
