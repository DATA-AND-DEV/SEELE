//! **A sessão sobre dois transportes** (Tarefa 2.6 da F4; ADR 0057).
//!
//! O nativo entra por `seele/1`, numa conexão QUIC. O navegador entra por `h3`
//! no mesmo socket, e o `wtransport` adota a conexão QUIC dele como uma sessão
//! WebTransport ([`crate::Daemon::run`]). Daqui para dentro, a sessão é uma só:
//! o mesmo aperto de mão, o mesmo fluxo de controle, os mesmos quadros.
//!
//! # Por que tão pouco
//!
//! Um fluxo WebTransport é um fluxo QUIC com um cabeçalho curto na frente (o
//! tipo e a sessão), que o `wtransport` escreve e lê. Depois dele, os bytes são
//! os do fluxo QUIC de baixo (`quic_stream_mut`), e o `frame.rs` os lê e
//! escreve como os do nativo. Por isso [`Envio`] e [`Recebe`] **são** o fluxo
//! QUIC, por `Deref`, e só [`Conexao`] precisa saber de qual transporte veio:
//! abrir e aceitar fluxos, e os datagramas, que na WebTransport levam o
//! identificador da sessão na frente.
//!
//! O resto — o endereço, o tempo de ida e volta, o fechar — é o da conexão QUIC
//! de baixo, nos dois.

use std::ops::{Deref, DerefMut};

use anyhow::{Context, Result};

/// A conexão de uma sessão: a do nativo, ou a WebTransport do navegador.
#[derive(Clone)]
pub enum Conexao {
    /// `seele/1`, o QUIC do nativo.
    Quic(quinn::Connection),
    /// `h3`, a WebTransport do navegador.
    Web(wtransport::Connection),
}

/// Um fluxo de envio, do nativo ou do navegador: o fluxo QUIC, por `Deref`.
pub enum Envio {
    /// Do nativo.
    Quic(quinn::SendStream),
    /// Do navegador.
    Web(wtransport::SendStream),
}

/// Um fluxo de leitura, do nativo ou do navegador: o fluxo QUIC, por `Deref`.
pub enum Recebe {
    /// Do nativo.
    Quic(quinn::RecvStream),
    /// Do navegador.
    Web(wtransport::RecvStream),
}

impl Deref for Envio {
    type Target = quinn::SendStream;

    fn deref(&self) -> &quinn::SendStream {
        match self {
            Self::Quic(fluxo) => fluxo,
            Self::Web(fluxo) => fluxo.quic_stream(),
        }
    }
}

impl DerefMut for Envio {
    fn deref_mut(&mut self) -> &mut quinn::SendStream {
        match self {
            Self::Quic(fluxo) => fluxo,
            Self::Web(fluxo) => fluxo.quic_stream_mut(),
        }
    }
}

impl Deref for Recebe {
    type Target = quinn::RecvStream;

    fn deref(&self) -> &quinn::RecvStream {
        match self {
            Self::Quic(fluxo) => fluxo,
            Self::Web(fluxo) => fluxo.quic_stream(),
        }
    }
}

impl DerefMut for Recebe {
    fn deref_mut(&mut self) -> &mut quinn::RecvStream {
        match self {
            Self::Quic(fluxo) => fluxo,
            Self::Web(fluxo) => fluxo.quic_stream_mut(),
        }
    }
}

impl Conexao {
    /// A conexão QUIC de baixo: o endereço, o tempo de ida e volta, o fechar.
    #[must_use]
    pub fn quic(&self) -> &quinn::Connection {
        match self {
            Self::Quic(conexao) => conexao,
            Self::Web(conexao) => conexao.quic_connection(),
        }
    }

    /// Se é o navegador.
    #[must_use]
    pub const fn do_navegador(&self) -> bool {
        matches!(self, Self::Web(_))
    }

    /// O fluxo bidirecional que o par abriu: o de controle.
    ///
    /// # Errors
    ///
    /// A conexão que caiu antes.
    pub async fn accept_bi(&self) -> Result<(Envio, Recebe)> {
        match self {
            Self::Quic(conexao) => {
                let (envio, recebe) = conexao.accept_bi().await?;
                Ok((Envio::Quic(envio), Recebe::Quic(recebe)))
            }
            Self::Web(conexao) => {
                let (envio, recebe) = conexao.accept_bi().await?;
                Ok((Envio::Web(envio), Recebe::Web(recebe)))
            }
        }
    }

    /// Um fluxo unidirecional que o par abriu: um anexo ou uma tela subindo.
    ///
    /// # Errors
    ///
    /// A conexão que caiu.
    pub async fn accept_uni(&self) -> Result<Recebe> {
        match self {
            Self::Quic(conexao) => Ok(Recebe::Quic(conexao.accept_uni().await?)),
            Self::Web(conexao) => Ok(Recebe::Web(conexao.accept_uni().await?)),
        }
    }

    /// Abre um fluxo unidirecional para o par: um anexo ou uma tela descendo.
    ///
    /// # Errors
    ///
    /// A conexão que caiu, ou o par sem crédito de fluxos.
    pub async fn open_uni(&self) -> Result<Envio> {
        match self {
            Self::Quic(conexao) => Ok(Envio::Quic(conexao.open_uni().await?)),
            Self::Web(conexao) => {
                let abrindo = conexao.open_uni().await?;
                Ok(Envio::Web(
                    abrindo.await.context("o fluxo da WebTransport não abriu")?,
                ))
            }
        }
    }

    /// O próximo datagrama do par: a voz. Em bytes próprios, porque a sala de
    /// voz os recebe assim.
    ///
    /// # Errors
    ///
    /// A conexão que caiu.
    pub async fn read_datagram(&self) -> Result<Vec<u8>> {
        match self {
            Self::Quic(conexao) => Ok(conexao.read_datagram().await?.to_vec()),
            Self::Web(conexao) => Ok(conexao.receive_datagram().await?.payload().to_vec()),
        }
    }

    /// Manda um datagrama ao par.
    ///
    /// # Errors
    ///
    /// O datagrama que não cabe no caminho, ou a conexão sem datagramas.
    pub fn send_datagram(&self, bytes: Vec<u8>) -> Result<()> {
        match self {
            Self::Quic(conexao) => Ok(conexao.send_datagram(bytes.into())?),
            Self::Web(conexao) => Ok(conexao.send_datagram(bytes)?),
        }
    }

    /// O endereço do par.
    #[must_use]
    pub fn remote_address(&self) -> std::net::SocketAddr {
        self.quic().remote_address()
    }

    /// Um número desta conexão, estável enquanto ela vive.
    #[must_use]
    pub fn stable_id(&self) -> usize {
        self.quic().stable_id()
    }

    /// Os contadores do QUIC de baixo.
    #[must_use]
    pub fn stats(&self) -> quinn::ConnectionStats {
        self.quic().stats()
    }

    /// O tempo de ida e volta estimado.
    #[must_use]
    pub fn rtt(&self) -> std::time::Duration {
        self.quic().rtt()
    }

    /// Fecha a conexão com um código e um motivo.
    pub fn close(&self, codigo: quinn::VarInt, motivo: &[u8]) {
        self.quic().close(codigo, motivo);
    }

    /// Espera a conexão cair, por qualquer lado.
    pub async fn closed(&self) -> quinn::ConnectionError {
        self.quic().closed().await
    }
}
