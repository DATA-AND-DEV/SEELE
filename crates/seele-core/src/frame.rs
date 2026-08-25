//! Length-prefixed framing over a QUIC stream.
//!
//! Deliberately duplicated from `seele-server`: ADR 0002 keeps the daemon and the
//! client from sharing a transport crate, and forty channels of framing is a much
//! smaller cost than a crate both would depend on and neither would own.
//!
//! `specs/02-protocolo.md` puts control on "bidirectional stream #0, long
//! lived". A QUIC stream is a byte stream, so message boundaries have to be
//! written down: four bytes of big-endian length, then the frame.
//!
//! The length is checked against [`seele_proto::control::MAX_FRAME_LEN`] **before**
//! anything is allocated, per `specs/08-seguranca.md`. Reading a 4 GiB length
//! from an unauthenticated peer and reserving for it is the oldest denial of
//! service there is.

use anyhow::{bail, Result};
use seele_proto::control::{Validate, MAX_FRAME_LEN};
use serde::{Deserialize, Serialize};

/// Reads one frame and decodes it.
///
/// # Errors
///
/// Fails on a closed stream, an oversized length, or a malformed frame.
pub async fn read<T>(stream: &mut quinn::RecvStream) -> Result<T>
where
    T: for<'de> Deserialize<'de> + Validate,
{
    let mut length = [0_u8; 4];
    stream.read_exact(&mut length).await?;
    let length = u32::from_be_bytes(length) as usize;

    // Before allocating. specs/08-seguranca.md.
    if length > MAX_FRAME_LEN {
        bail!("peer announced a {length}-byte control frame, over the {MAX_FRAME_LEN}-byte limit");
    }
    if length == 0 {
        bail!("peer announced an empty control frame");
    }

    let mut frame = vec![0_u8; length];
    stream.read_exact(&mut frame).await?;
    Ok(seele_proto::control::decode::<T>(&frame)?)
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

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::sync::Arc;
    use std::time::Duration;

    use seele_proto::control::ClientMessage;

    /// Um par QUIC ligado, com um fluxo bidirecional aberto dos dois lados.
    ///
    /// Não usa o handshake do produto de propósito: o que está sob teste é o
    /// enquadramento, e um handshake no meio só acrescentaria motivos para o
    /// teste falhar que não têm nada a ver com a pergunta.
    async fn par() -> (quinn::SendStream, quinn::RecvStream) {
        let _ = rustls::crypto::ring::default_provider().install_default();

        let certificado =
            rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).expect("certificado");
        let cadeia = vec![rustls::pki_types::CertificateDer::from(
            certificado.cert.der().to_vec(),
        )];
        let chave =
            rustls::pki_types::PrivatePkcs8KeyDer::from(certificado.signing_key.serialize_der());

        let mut tls_servidor = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(cadeia.clone(), chave.into())
            .expect("config do servidor");
        tls_servidor.alpn_protocols = vec![b"seele-test".to_vec()];
        let servidor = quinn::ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(tls_servidor).expect("quic"),
        ));

        let escuta = quinn::Endpoint::server(servidor, SocketAddr::from(([127, 0, 0, 1], 0)))
            .expect("escutar");
        let endereco = escuta.local_addr().expect("endereço");

        let mut raiz = rustls::RootCertStore::empty();
        raiz.add(cadeia[0].clone()).expect("raiz");
        let mut tls_cliente = rustls::ClientConfig::builder()
            .with_root_certificates(raiz)
            .with_no_client_auth();
        tls_cliente.alpn_protocols = vec![b"seele-test".to_vec()];

        let mut cliente =
            quinn::Endpoint::client(SocketAddr::from(([127, 0, 0, 1], 0))).expect("cliente");
        cliente.set_default_client_config(quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(tls_cliente).expect("quic"),
        )));

        let aceitando = tokio::spawn(async move {
            let conexao = escuta
                .accept()
                .await
                .expect("entrada")
                .await
                .expect("aceitar");
            let fluxo = conexao.accept_bi().await.expect("bi");
            // O endpoint precisa continuar vivo enquanto o fluxo existir.
            (fluxo, escuta)
        });

        let conexao = cliente
            .connect(endereco, "localhost")
            .expect("conectar")
            .await
            .expect("conexão");
        let (mut envio, _) = conexao.open_bi().await.expect("abrir");
        // Um byte para o outro lado enxergar o fluxo. Vai como um quadro
        // inteiro, para não sujar o que o teste vai medir depois.
        super::write(&mut envio, &ClientMessage::Ping { timestamp: 0 })
            .await
            .expect("abre-alas");

        let ((_, recebe), escuta) = aceitando.await.expect("junção");
        let mut recebe = recebe;
        let abertura: ClientMessage = super::read(&mut recebe).await.expect("abre-alas");
        assert!(matches!(abertura, ClientMessage::Ping { .. }));

        // Vazam de propósito: o teste é curto e derrubar os endpoints fecharia
        // os fluxos que ele ainda vai usar.
        std::mem::forget(conexao);
        std::mem::forget(cliente);
        std::mem::forget(escuta);
        (envio, recebe)
    }

    /// Cancelar `read` no meio de um quadro **dessincroniza o fluxo para
    /// sempre**.
    ///
    /// É a razão pela qual nem o cliente nem a sessão do servidor podem chamar
    /// `read` dentro de um `select!`. `read` faz dois `read_exact` — tamanho e
    /// corpo — e cancelado entre os dois joga fora o tamanho já consumido. O
    /// próximo `read` lê os primeiros quatro bytes do **corpo** como se fossem
    /// um tamanho, e daí em diante tudo o que chega está deslocado.
    ///
    /// Isto custou dois diagnósticos errados. A primeira vez eu suspeitei do
    /// mecanismo, não consegui prová-lo, corrigi só o lado do cliente e aumentei
    /// um prazo de teste chamando aquilo de mitigação. O defeito voltou pelo
    /// lado do servidor, onde um `tokio::time::interval` de um segundo corre
    /// contra a leitura no mesmo `select!` — uma oportunidade de cancelar por
    /// segundo, para sempre. Este teste é a prova que faltava.
    #[tokio::test(flavor = "multi_thread")]
    async fn cancelar_a_leitura_no_meio_do_quadro_dessincroniza_o_fluxo() {
        let (mut envio, mut recebe) = par().await;

        // Um quadro em duas partes, com uma pausa no meio — exatamente o que
        // acontece quando o corpo chega num pacote posterior ao do tamanho.
        let corpo = seele_proto::control::encode(&ClientMessage::SendMessage {
            channel: seele_proto::ids::ChannelId(1),
            body: "dito no terminal".to_owned(),
            replies_to: None,
            client_message_id: seele_proto::ids::ClientMessageId(1),
        })
        .expect("codificar");
        let tamanho = u32::try_from(corpo.len()).expect("cabe");

        envio
            .write_all(&tamanho.to_be_bytes())
            .await
            .expect("tamanho");
        // Sem isto o quinn junta os dois `write_all` num pacote só e não há
        // meio de quadro para cancelar.
        tokio::time::sleep(Duration::from_millis(150)).await;

        // A leitura perde a corrida para o relógio, como perde para o ticker de
        // telemetria da sessão.
        let cancelada = tokio::select! {
            lido = super::read::<ClientMessage>(&mut recebe) => Some(lido.is_ok()),
            () = tokio::time::sleep(Duration::from_millis(50)) => None,
        };
        assert_eq!(cancelada, None, "o relógio tinha que ganhar esta corrida");

        envio.write_all(&corpo).await.expect("corpo");

        // O quadro está inteiro no fluxo agora. Um enquadramento que
        // sobrevivesse ao cancelamento entregaria a mensagem aqui.
        let depois = tokio::time::timeout(
            Duration::from_secs(2),
            super::read::<ClientMessage>(&mut recebe),
        )
        .await;

        let perdido = match depois {
            // Bloqueou esperando bytes que não vêm: o tamanho lido do corpo
            // pede mais do que existe. É o caso que trava a sessão calada.
            Err(_) => true,
            // Ou o "tamanho" saiu absurdo e a decodificação recusou.
            Ok(Err(_)) => true,
            Ok(Ok(mensagem)) => {
                !matches!(&mensagem, ClientMessage::SendMessage { body, .. } if body == "dito no terminal")
            }
        };
        assert!(
            perdido,
            "o quadro sobreviveu ao cancelamento — se isto passar a valer, \
             a tarefa leitora dedicada deixou de ser necessária"
        );
    }
}
