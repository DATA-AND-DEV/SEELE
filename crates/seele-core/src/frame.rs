//! Length-prefixed framing over a QUIC stream.
//!
//! **A regra mora em [`seele_proto::quadro`]** — quantos bytes, em que ordem,
//! com que teto —, e aqui fica só a E/S do `quinn`. Essa metade continua
//! duplicada do `seele-server` de propósito: o ADR 0002 não deixa o daemon e o
//! cliente dividirem um crate de transporte, e quarenta linhas de E/S custam
//! menos que um crate que os dois usariam e nenhum seria dono.
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
use seele_proto::control::{ControlError, Lido, ListaDeCima, Validate, MAX_FRAME_LEN};
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
    let frame = ler_bruto(stream).await?;
    Ok(seele_proto::control::decode::<T>(&frame)?)
}

/// Um quadro inteiro, com o carimbo no byte 0, **sem decodificar**.
///
/// É o que deixa o aperto de mão olhar o carimbo da primeira resposta antes do
/// `decode`, que recusaria um número de fora sem dizer qual veio — e é pelo
/// número que se sabe a versão de quem respondeu (`ConnectError::OutraLinha`).
/// Nunca devolve um quadro vazio: o comprimento zero é recusado antes.
///
/// # Errors
///
/// Fails on a closed stream, or an empty or oversized length.
pub async fn ler_bruto(stream: &mut quinn::RecvStream) -> Result<Vec<u8>> {
    let mut prefixo = [0_u8; seele_proto::quadro::PREFIXO_LEN];
    stream.read_exact(&mut prefixo).await?;

    // Before allocating. specs/08-seguranca.md.
    let length = match seele_proto::quadro::comprimento(prefixo) {
        Ok(length) => length,
        Err(ControlError::Empty) => bail!("peer announced an empty control frame"),
        Err(_) => bail!(
            "peer announced a {}-byte control frame, over the {MAX_FRAME_LEN}-byte limit",
            u32::from_be_bytes(prefixo)
        ),
    };

    let mut frame = vec![0_u8; length];
    stream.read_exact(&mut frame).await?;
    Ok(frame)
}

/// Como [`read`], para uma das listas de cima: um ordinal que esta build não
/// conhece vira [`Lido::Desconhecida`] em vez de erro (adendo de 08/10/2026,
/// §6). Um ordinal conhecido com o corpo errado continua erro.
///
/// # Errors
///
/// Os mesmos de [`read`].
pub async fn ler_da_lista<T: ListaDeCima>(stream: &mut quinn::RecvStream) -> Result<Lido<T>> {
    let frame = ler_bruto(stream).await?;
    Ok(seele_proto::control::decode_da_lista::<T>(&frame)?)
}

/// Encodes a message and writes it as one frame.
///
/// Carimbado com o número **desta build**. O fluxo de controle de uma sessão
/// escreve com [`write_em`], no número que o servidor carimbou na primeira
/// resposta (adendo de 08/10/2026, §4); este serve aos fluxos sem negociação.
///
/// # Errors
///
/// Fails if the message is invalid or the stream is closed.
pub async fn write<T>(stream: &mut quinn::SendStream, message: &T) -> Result<()>
where
    T: Serialize + Validate,
{
    write_em(stream, seele_proto::version::PROTOCOL_VERSION, message).await
}

/// Como [`write`], com o carimbo escolhido por quem escreve.
///
/// O `Hello` sai com a mínima desta build, que qualquer servidor da mesma linha
/// lê; o resto da sessão, com o número que o servidor carimbou na primeira
/// resposta. Na 1.0 os dois são 9 e o quadro é o mesmo de [`write`].
///
/// # Errors
///
/// Fails if the message is invalid or the stream is closed.
pub async fn write_em<T>(stream: &mut quinn::SendStream, versao: u8, message: &T) -> Result<()>
where
    T: Serialize + Validate,
{
    let frame = seele_proto::control::encode_em(versao, message)?;
    stream
        .write_all(&seele_proto::quadro::enquadrar(&frame)?)
        .await?;
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
