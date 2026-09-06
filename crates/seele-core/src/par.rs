//! O caminho entre pares: um cliente serve tela a outro.
//!
//! O `§5.1` do desenho de compartilhamento de tela recusou isto em 22/08 com
//! uma frase — *«custa um caminho que este produto nunca teve»*. Este módulo é
//! esse caminho, e a spec de 05/09 conta por que ele passou a valer a pena.
//!
//! # Por que o certificado não vem do `seele-server`
//!
//! O `tls.rs` de lá faz a mesma coisa, e o ADR 0002 proíbe o `seele-core` de
//! depender do daemon. Repetir as poucas linhas de `rcgen` é o que `crate::tela`
//! já decidiu para as constantes de enquadramento, e pela mesma razão:
//! *«quarenta linhas repetidas custam menos que um crate de transporte que os
//! dois dependeriam e nenhum seria dono»*. O que não pode divergir é o
//! **formato da impressão digital**, e ele não diverge porque os dois lados
//! chamam `seele_proto::transport::certificate_fingerprint`.

use rustls::pki_types::{CertificateDer, PrivateKeyDer};

/// Um certificado e a chave que assina por ele.
pub struct Identidade {
    /// O certificado, em DER.
    pub cadeia: Vec<CertificateDer<'static>>,
    /// A chave privada, em DER.
    pub chave: PrivateKeyDer<'static>,
}

/// Por que o caminho entre pares não deu certo.
///
/// Enumerado, e cada variante distingue um conserto diferente — ver a tabela do
/// §4 da spec.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroDePar {
    /// Não deu para gerar o certificado desta sessão.
    #[error("não deu para gerar o certificado deste par: {0}")]
    Certificado(String),
    /// Não deu para pôr a ponta a atender.
    #[error("não deu para pôr esta ponta a atender: {0}")]
    Escuta(String),
    /// Nenhum dos endereços fechou aperto de mão.
    #[error("nenhum endereço deste par respondeu")]
    NaoAlcancou,
    /// Alguém respondeu, e não era quem o servidor apresentou.
    ///
    /// **Não é o mesmo que [`Self::NaoAlcancou`]**, e juntá-las apagaria a
    /// informação inteira: a diferença entre «não consegui falar com ele» e
    /// «alguém respondeu no lugar dele» é a que o ADR 0003 existe para nomear.
    #[error("o par apresentou {veio}, e o servidor tinha dito {esperada}")]
    ImpressaoNaoBate {
        /// O que o servidor apresentou.
        esperada: String,
        /// O que veio no aperto de mão.
        veio: String,
    },
}

/// Gera a identidade desta sessão. **Nunca vai para o disco.**
///
/// # Errors
///
/// Falha se o `rcgen` não gerar chave ou certificado.
pub fn identidade_efemera() -> Result<Identidade, ErroDePar> {
    // A forma exata que `tela.rs:2049` já usa neste crate, e por isso está
    // provada contra esta versão do `rcgen`. Não invente outra.
    let gerado = rcgen::generate_simple_self_signed(vec!["seele-par".to_owned()])
        .map_err(|erro| ErroDePar::Certificado(erro.to_string()))?;
    let cadeia = vec![CertificateDer::from(gerado.cert.der().to_vec())];
    let chave = rustls::pki_types::PrivatePkcs8KeyDer::from(gerado.signing_key.serialize_der());
    Ok(Identidade {
        cadeia,
        chave: chave.into(),
    })
}

/// A impressão digital que o servidor vai apresentar por esta identidade.
#[must_use]
pub fn impressao(identidade: &Identidade) -> String {
    identidade.cadeia.first().map_or_else(String::new, |cert| {
        seele_proto::transport::certificate_fingerprint(cert.as_ref())
    })
}

/// Diz a uma ponta que já existe que ela também atende.
///
/// **Só é chamada quando a pessoa optou por emprestar a subida.** Quem não
/// optou nunca passa por aqui, e a ponta dela continua só discando, como antes
/// desta onda existir.
///
/// # Errors
///
/// Falha se o `rustls` recusar o certificado ou a chave.
pub fn passar_a_atender(ponta: &quinn::Endpoint, identidade: Identidade) -> Result<(), ErroDePar> {
    let mut tls = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(identidade.cadeia, identidade.chave)
        .map_err(|erro| ErroDePar::Escuta(erro.to_string()))?;
    tls.alpn_protocols = vec![seele_proto::transport::ALPN.to_vec()];
    let quic = quinn::crypto::rustls::QuicServerConfig::try_from(tls)
        .map_err(|erro| ErroDePar::Escuta(erro.to_string()))?;
    ponta.set_server_config(Some(quinn::ServerConfig::with_crypto(std::sync::Arc::new(
        quic,
    ))));
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "um teste que trata o caso impossível deixa de ser uma afirmação sobre o código"
)]
mod testes {
    use super::*;

    #[test]
    fn cada_identidade_e_nova_e_a_impressao_a_distingue() {
        // **Efêmero é o ponto, e não um detalhe.** O certificado do servidor é
        // persistido porque o pino do ADR 0003 depende dele. Este não é pinado
        // por ninguém: quem confere recebe a impressão digital pelo servidor a
        // cada apresentação. Guardá-lo em disco criaria um identificador
        // estável da máquina de quem empresta, que é metadado que ninguém pediu.
        let uma = identidade_efemera().unwrap();
        let outra = identidade_efemera().unwrap();
        assert_ne!(
            impressao(&uma),
            impressao(&outra),
            "duas identidades saíram com a mesma impressão: ela não distingue nada"
        );
    }

    #[test]
    fn a_impressao_e_a_do_certificado_e_no_formato_de_sempre() {
        // Um formato só para a mesma coisa. Se isto divergir de
        // `certificate_fingerprint`, o pino do `seele://` e a apresentação
        // entre pares passam a dizer o mesmo hash de dois jeitos, e um dia
        // discordam.
        let identidade = identidade_efemera().unwrap();
        let der = identidade.cadeia.first().unwrap();
        assert_eq!(
            impressao(&identidade),
            seele_proto::transport::certificate_fingerprint(der.as_ref())
        );
        assert_eq!(impressao(&identidade).len(), 64, "não é SHA-256 em hex");
        assert!(impressao(&identidade)
            .chars()
            .all(|c| c.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn uma_ponta_de_cliente_passa_a_atender_sem_socket_novo() {
        // **Nem escuta nova, nem porta nova.** `set_server_config` recebe `&self`,
        // então a ponta que o cliente já usa para falar com o servidor aprende a
        // atender na mesma porta — e naquela porta o mapeamento de NAT já está
        // vivo, mantido pelo keep-alive da conexão que já existe. Abrir uma porta
        // à parte perderia essa propriedade, que é a melhor do desenho.
        let ponta = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let porta_antes = ponta.local_addr().unwrap();

        passar_a_atender(&ponta, identidade_efemera().unwrap()).unwrap();

        assert_eq!(
            ponta.local_addr().unwrap(),
            porta_antes,
            "atender trocou a porta: o mapeamento de NAT que já estava vivo se perdeu"
        );
        // **A prova de que atender funciona é da Task 4**, porque esta asserção
        // só confere que a porta não mudou. Se `set_server_config` foi de verdade
        // chamado com a configuração correta, é testado lá.
    }
}
