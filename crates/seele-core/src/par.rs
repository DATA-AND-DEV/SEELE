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

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};

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

/// Onde o verificador deixa o [`ErroDePar`] que o `rustls` não sabe carregar.
///
/// **É o que salva a distinção entre «ninguém respondeu» e «alguém respondeu
/// no lugar dele».** O `rustls` só aceita de volta um `rustls::Error`, e o
/// nosso motivo tipado teria de virar `Error::General(String)` no caminho —
/// depois disso, quem disca só teria texto de mensagem para comparar. Esta
/// vaga é uma segunda saída para o erro de verdade: o verificador o deixa aqui
/// antes de responder ao `rustls`, e [`ligar`] o recolhe quando a conexão
/// falha. Uma vaga por tentativa, criada em [`config_de_cliente`], então não
/// há duas discagens a disputá-la.
pub(crate) type Relato = std::sync::Arc<std::sync::Mutex<Option<ErroDePar>>>;

/// Aceita **um** certificado, o que o servidor apresentou, e nenhum outro.
///
/// Espelho do `TofuVerifier` de `crate::tofu`, com a diferença que é o assunto
/// todo: aquele **aprende** na primeira vez e guarda; este não aprende nada e
/// não guarda nada. A impressão digital chega pelo servidor a cada
/// apresentação, então não há primeira vez a confiar.
#[derive(Debug)]
pub(crate) struct ConfereImpressao {
    esperada: String,
    provedor: std::sync::Arc<rustls::crypto::CryptoProvider>,
    relato: Relato,
}

impl ConfereImpressao {
    /// Um verificador que só aceita esta impressão digital.
    ///
    /// A impressão é dada, e não lida do certificado nem do nome TLS, porque
    /// quem a apresenta é o servidor: o par não tem como vouchear por si
    /// mesmo. O nome TLS aqui é só um rótulo — este verificador nunca o
    /// confere —, e é a mesma razão pela qual `TofuVerifier::new` recebe a
    /// chave de pino em vez de a deduzir.
    #[must_use]
    pub(crate) fn nova(esperada: String) -> Self {
        Self {
            esperada,
            provedor: std::sync::Arc::new(rustls::crypto::ring::default_provider()),
            relato: Relato::default(),
        }
    }

    /// A conferência em si, fora do `trait`, para o teste poder afirmá-la sem
    /// montar uma sessão TLS inteira.
    pub(crate) fn confere(&self, certificado: &CertificateDer<'_>) -> Result<(), ErroDePar> {
        let veio = seele_proto::transport::certificate_fingerprint(certificado.as_ref());
        if veio == self.esperada {
            Ok(())
        } else {
            Err(ErroDePar::ImpressaoNaoBate {
                esperada: self.esperada.clone(),
                veio,
            })
        }
    }
}

impl ServerCertVerifier for ConfereImpressao {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        self.confere(end_entity)
            .map(|()| ServerCertVerified::assertion())
            .map_err(|erro| {
                // O motivo tipado sai por aqui **antes** de ser achatado em
                // texto; ver [`Relato`]. Se a vaga estiver envenenada, quem
                // discou cai em `NaoAlcancou`, que é pior mas não é errado.
                if let Ok(mut vaga) = self.relato.lock() {
                    *vaga = Some(erro.clone());
                }
                rustls::Error::General(erro.to_string())
            })
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provedor.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provedor.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provedor
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// O `ClientConfig` com que se disca para um par, e a vaga do motivo.
///
/// Devolve as duas coisas porque a configuração sozinha não basta: quando o
/// aperto de mão falha, o `quinn` só sabe dizer que falhou, e o motivo tipado
/// está na [`Relato`] que o verificador desta configuração — e só ele — enche.
/// Separá-las obrigaria [`ligar`] a adivinhar qual vaga é de qual discagem.
///
/// # Errors
///
/// Falha se o `rustls` recusar a configuração.
pub(crate) fn config_de_cliente(
    esperada: String,
) -> Result<(quinn::ClientConfig, Relato), ErroDePar> {
    let confere = ConfereImpressao::nova(esperada);
    let relato = std::sync::Arc::clone(&confere.relato);
    let mut tls = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(std::sync::Arc::new(confere))
        .with_no_client_auth();
    tls.alpn_protocols = vec![seele_proto::transport::ALPN.to_vec()];
    let quic = quinn::crypto::rustls::QuicClientConfig::try_from(tls)
        .map_err(|erro| ErroDePar::Escuta(erro.to_string()))?;
    Ok((quinn::ClientConfig::new(std::sync::Arc::new(quic)), relato))
}

/// Como a ligação com um par foi conseguida.
///
/// **É metade da razão de o subprojeto A existir.** Toda a aritmética da malha
/// supõe que dois clientes domésticos se alcançam, e ninguém mediu isso. Se o
/// furo falhar em boa parte dos pares, a árvore do subprojeto B não pode supor
/// que qualquer par se alcança — e vira «árvore entre quem se alcança, estrela
/// para o resto», que é um desenho bem diferente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComoChegou {
    /// Mesma rede: nenhum furo foi necessário.
    Local,
    /// Endereço público: o furo deu certo.
    Furo,
}

/// Um par ligado, e o que a ligação ensinou.
#[derive(Debug)]
pub struct ParLigado {
    /// A conexão viva.
    pub conexao: quinn::Connection,
    /// Como ela foi conseguida.
    pub como: ComoChegou,
    /// O ida e volta que o `quinn` está medindo nela. É o custo de um salto.
    pub ida_e_volta: std::time::Duration,
}

/// Disca para um par e devolve a primeira conexão que fechar.
///
/// Todos os endereços em paralelo, como o ADR 0037 faz para o servidor: uma
/// lista tentada em série multiplica o pior caso pelo número de candidatos, e o
/// pior caso é justamente o endereço que não responde.
///
/// **Só disca.** Quem atende do outro lado é o laço de [`quinn::Endpoint::accept`]
/// de quem chamou [`passar_a_atender`] — o `quinn` enfileira a tentativa que
/// chega e não responde nada até alguém a aceitar. As duas coisas juntas é que
/// são o furo: a discagem abre o mapeamento de NAT desta ponta, e o atendimento
/// deixa entrar a discagem que vem pelo mapeamento aberto do outro lado.
///
/// # Errors
///
/// [`ErroDePar::ImpressaoNaoBate`] quando alguém respondeu e não era quem o
/// servidor apresentou; [`ErroDePar::NaoAlcancou`] quando ninguém respondeu no
/// prazo.
pub async fn ligar(
    ponta: &quinn::Endpoint,
    enderecos: &[std::net::SocketAddr],
    impressao_esperada: String,
    prazo: std::time::Duration,
) -> Result<ParLigado, ErroDePar> {
    let mut tentativas = tokio::task::JoinSet::new();
    for endereco in enderecos {
        let (config, relato) = config_de_cliente(impressao_esperada.clone())?;
        let ponta = ponta.clone();
        let endereco = *endereco;
        // Cada tentativa devolve **o próprio endereço** junto do resultado. Sem
        // isso, o rastro diz que «um endereço» falhou sem dizer qual, e o que
        // sobra no fim do prazo não tem nome nenhum.
        tentativas.spawn(async move {
            let tentada = async {
                let ligando = ponta
                    .connect_with(config, endereco, "seele-par")
                    .map_err(|erro| ErroDePar::Escuta(erro.to_string()))?;
                let conexao = ligando
                    .await
                    .map_err(|erro| classificar(&relato, endereco, &erro))?;
                Ok::<_, ErroDePar>((conexao, como_chegou(endereco)))
            }
            .await;
            (endereco, tentada)
        });
    }

    let mut no_ar = enderecos.to_vec();
    let mut ultimo = ErroDePar::NaoAlcancou;
    let ate = tokio::time::Instant::now() + prazo;
    // O laço é explícito para o fim saber **por que** acabou: um `while let`
    // não distingue «venceu o prazo» de «todas as tentativas responderam», e a
    // linha de rastro lá embaixo mente se confundir as duas.
    let mut venceu_o_prazo = false;
    loop {
        let acabou = match tokio::time::timeout_at(ate, tentativas.join_next()).await {
            Err(_) => {
                venceu_o_prazo = true;
                break;
            }
            Ok(None) => break,
            Ok(Some(acabou)) => acabou,
        };
        let (endereco, tentada) = match acabou {
            Ok(devolvido) => devolvido,
            Err(erro) => {
                // A tarefa nem chegou a devolver endereço; é o único caso em
                // que o rastro não sabe de quem fala.
                tracing::warn!(%erro, "uma tentativa de ligação morreu sem responder");
                ultimo = ErroDePar::Escuta(erro.to_string());
                continue;
            }
        };
        if let Some(posicao) = no_ar.iter().position(|candidato| *candidato == endereco) {
            no_ar.remove(posicao);
        }
        match tentada {
            Ok((conexao, como)) => {
                let ida_e_volta = conexao.rtt();
                // **Por qual endereço a ligação entrou, dito por extenso** — a
                // lição do `e56dbb2`, que consertou a mesma falta na corrida de
                // candidatos do servidor. Sem esta linha só dava para inferir
                // pelos milissegundos, e a inferência erra.
                tracing::info!(
                    par = %conexao.remote_address(),
                    ?como,
                    ?ida_e_volta,
                    "um par ligou"
                );
                if !no_ar.is_empty() {
                    tracing::debug!(?no_ar, "estas tentativas foram abandonadas: outra venceu");
                }
                // A primeira que fecha vence; as outras são abandonadas, e
                // abandoná-las é o que fecha as conexões que sobraram.
                tentativas.abort_all();
                return Ok(ParLigado {
                    conexao,
                    como,
                    ida_e_volta,
                });
            }
            // **A impressão que não bate ganha do silêncio no fim — e só no
            // fim.** Devolvê-la na hora seria preempção, não precedência: um
            // único endereço obsoleto da lista, reciclado por outra máquina,
            // mataria a discagem inteira antes de o endereço legítimo fechar o
            // aperto de mão um instante depois. Quem consegue pôr um endereço
            // na lista teria negação de serviço de graça. A impostura já saiu
            // no `warn!` da `classificar`, que é onde ela é notícia; aqui ela
            // só espera, e é o motivo devolvido se ninguém vencer.
            Err(erro) => {
                if !matches!(ultimo, ErroDePar::ImpressaoNaoBate { .. }) {
                    ultimo = erro;
                }
            }
        }
    }
    if venceu_o_prazo && !no_ar.is_empty() {
        // O prazo venceu com gente no ar. O que essas tentativas teriam a
        // dizer morre no `abort`, então o rastro diz ao menos **quais** eram —
        // a alternativa é uma falha que não deixa nome nenhum para trás.
        tracing::info!(
            ?prazo,
            ?no_ar,
            "o prazo venceu e estes endereços do par ainda não tinham respondido"
        );
        tentativas.abort_all();
    }
    Err(ultimo)
}

/// Se este endereço é da mesma rede, e portanto não precisou de furo.
fn como_chegou(endereco: std::net::SocketAddr) -> ComoChegou {
    let local = match endereco.ip() {
        std::net::IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local(),
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.segments().first().is_some_and(|s| {
                    // `fc00::/7`, o endereço único local, e `fe80::/10`, o
                    // link-local — o par de `is_private()` e `is_link_local()`
                    // do lado v4. Nenhum dos dois atravessa roteador, então
                    // quem chega por eles está na mesma rede e não furou nada.
                    // O `std` sabe dizer isto, mas só em API instável.
                    s & 0xfe00 == 0xfc00 || s & 0xffc0 == 0xfe80
                })
        }
    };
    if local {
        ComoChegou::Local
    } else {
        ComoChegou::Furo
    }
}

/// Traduz uma falha de conexão para o motivo enumerado, e a registra.
///
/// **Sem comparar texto de mensagem.** O motivo real é o que o verificador
/// deixou na [`Relato`] desta discagem; o `quinn::ConnectionError` que chega
/// aqui é a mesma falha vista de longe, já achatada, e serve só para o rastro.
/// Vaga cheia é «alguém respondeu, e não era ele»; vaga vazia é silêncio —
/// ninguém chegou a apresentar certificado nenhum.
///
/// Recebe o `endereco` porque é aqui que o rastro sabe de quem fala: uma linha
/// que diz que «um endereço» falhou, numa lista de candidatos, não diz nada.
fn classificar(
    relato: &Relato,
    endereco: std::net::SocketAddr,
    erro: &quinn::ConnectionError,
) -> ErroDePar {
    match relato.lock().ok().and_then(|mut vaga| vaga.take()) {
        Some(motivo @ ErroDePar::ImpressaoNaoBate { .. }) => {
            // **Notícia, e não pode esperar o fim da discagem.** Quem discou
            // pode até ligar por outro endereço e nunca devolver este erro —
            // alguém ter respondido no lugar do par continua sendo o evento de
            // segurança que o §4 da spec quer contado, e é contado aqui.
            tracing::warn!(par = %endereco, %motivo, "alguém respondeu no lugar do par");
            motivo
        }
        Some(outro) => outro,
        None => {
            // O produto sabe qual erro o `quinn` deu; `NaoAlcancou` não tem
            // onde o guardar, e perdê-lo em silêncio é o defeito de sempre.
            tracing::debug!(par = %endereco, %erro, "este endereço do par não fechou aperto de mão");
            ErroDePar::NaoAlcancou
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "um teste que trata o caso impossível deixa de ser uma afirmação sobre o código"
)]
mod testes {
    use super::*;

    /// Atende numa ponta e segura o que chegar, até ela fechar.
    ///
    /// **Não é enfeite de teste, é metade do furo.** O `quinn` 0.11 enfileira
    /// a tentativa que chega e não responde *nada* até alguém a aceitar: uma
    /// ponta que só chamou [`passar_a_atender`] fica muda, e a discagem do
    /// outro lado morre de [`ErroDePar::NaoAlcancou`] no fim do prazo. Foi
    /// exatamente o que estes dois testes fizeram antes deste laço existir —
    /// medido, não suposto. No produto, quem roda este laço é quem recebeu
    /// `SirvaTelaPara`; aqui é isto.
    ///
    /// Segura as conexões porque largar uma `quinn::Connection` a fecha, e um
    /// dos testes pergunta a quem discou se ela continua viva.
    ///
    /// O `atraso` é esperado **depois** de a tentativa chegar e antes de ser
    /// respondida, e serve a um teste só: o que precisa que o impostor falhe
    /// antes de o par legítimo fechar. Nos outros é `ZERO`.
    fn atender_em_segundo_plano(
        ponta: quinn::Endpoint,
        atraso: std::time::Duration,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut vivas = Vec::new();
            while let Some(chegando) = ponta.accept().await {
                tokio::time::sleep(atraso).await;
                if let Ok(conexao) = chegando.await {
                    vivas.push(conexao);
                }
            }
        })
    }

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

    #[test]
    fn o_par_certo_passa_e_o_errado_e_recusado_com_o_motivo_certo() {
        // **A identidade aqui não é TOFU — é apresentação.** O ADR 0003 vale para
        // o `seele://`, onde não há intermediário e a primeira vez tem de ser
        // confiada. Aqui há: os dois clientes já fixaram o mesmo servidor e já se
        // autenticaram nele por chave pública (ADR 0004). O servidor ocupa o lugar
        // que o link ocupa no `seele://`.
        //
        // Sem pino novo em disco, e sem par anônimo alimentando quadro.
        let identidade = identidade_efemera().unwrap();
        let der = identidade.cadeia.first().unwrap().clone();
        let certa = impressao(&identidade);

        assert!(ConfereImpressao::nova(certa.clone()).confere(&der).is_ok());

        let erro = ConfereImpressao::nova("f".repeat(64))
            .confere(&der)
            .unwrap_err();
        match erro {
            ErroDePar::ImpressaoNaoBate { esperada, veio } => {
                assert_eq!(esperada, "f".repeat(64));
                assert_eq!(veio, certa);
            }
            outro => panic!("o motivo errado saiu de uma impressão que não bate: {outro:?}"),
        }
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

    #[tokio::test(flavor = "multi_thread")]
    async fn dois_pares_se_ligam_e_o_teste_sabe_como() {
        // **Os dois discam, e o furo sai de graça.** Como as duas pontas atendem,
        // as próprias tentativas de conexão são os pacotes que abrem o NAT dos dois
        // lados; a primeira que fecha o aperto de mão vence e a outra é descartada.
        // Resolve o caso assimétrico sozinho — se só um lado consegue sair, é a
        // conexão dele que vinga — e usa só a API pública do `quinn`.
        //
        // Em `127.0.0.1` não há NAT a furar, então o que este teste prende é o
        // resto: que a ligação fecha, que a impressão digital é conferida no
        // caminho, e que o resultado diz **como** chegou. A taxa de furo de verdade
        // é o roteiro de duas máquinas que mede, e nenhum teste daqui pode medi-la.
        let a = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let ia = identidade_efemera().unwrap();
        let impressao_a = impressao(&ia);
        passar_a_atender(&a, ia).unwrap();
        let endereco_a = a.local_addr().unwrap();
        let _atendendo_a = atender_em_segundo_plano(a, std::time::Duration::ZERO);

        let b = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let ib = identidade_efemera().unwrap();
        passar_a_atender(&b, ib).unwrap();

        let ligado = ligar(
            &b,
            &[endereco_a],
            impressao_a,
            std::time::Duration::from_secs(5),
        )
        .await
        .expect("os dois pares não se ligaram");

        assert_eq!(
            ligado.como,
            ComoChegou::Local,
            "127.0.0.1 não é rede local?"
        );
        assert!(
            ligado.conexao.close_reason().is_none(),
            "a conexão já morreu"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn um_par_com_a_impressao_errada_nao_liga() {
        // O caso que separa «não consegui falar com ele» de «alguém respondeu no
        // lugar dele». Sem esta parede, qualquer um que alcance a porta alimenta
        // quadro de tela a quem estava esperando o par certo.
        let a = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        passar_a_atender(&a, identidade_efemera().unwrap()).unwrap();
        let endereco_a = a.local_addr().unwrap();
        let _atendendo_a = atender_em_segundo_plano(a, std::time::Duration::ZERO);

        let b = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let erro = ligar(
            &b,
            &[endereco_a],
            "f".repeat(64),
            std::time::Duration::from_secs(5),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(erro, ErroDePar::ImpressaoNaoBate { .. }),
            "a recusa saiu com o motivo errado: {erro:?}"
        );
        // **E o motivo veio inteiro** — que é o que o caminho limpo comprou.
        // `classificar` recolhe o `ErroDePar` que o verificador guardou na
        // `Relato`, então os dois hashes chegam até aqui; uma `classificar`
        // que olhasse o texto da mensagem do `quinn` só saberia dizer «não
        // bateu», com os dois campos vazios. Esta asserção é também o que faz
        // o guarda morder: uma `ligar` que ignorasse a impressão pedida e
        // discasse com outra passaria pelo `matches!` acima sem tropeçar.
        if let ErroDePar::ImpressaoNaoBate { esperada, veio } = &erro {
            assert_eq!(esperada, &"f".repeat(64), "não foi a impressão pedida");
            assert_eq!(veio.len(), 64, "o que veio não é SHA-256 em hex");
            assert_ne!(veio, esperada, "os dois lados do erro são o mesmo hash");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn um_impostor_na_lista_nao_derruba_a_discagem_inteira() {
        // **A impostura é notícia, não é motivo para desistir.** A lista de um
        // par mistura endereço de LAN e endereço público, e um deles estar
        // obsoleto — reciclado por outra máquina — é cenário de todo dia. Se
        // bastasse um responder com o certificado errado para a discagem
        // inteira morrer, quem conseguisse pôr um endereço na lista teria uma
        // negação de serviço de graça contra o par legítimo. O evento de
        // segurança sai no `warn!` da `classificar`, na hora; a discagem
        // continua, e `ImpressaoNaoBate` só é devolvido se ninguém vencer.
        //
        // O legítimo atende com atraso **de propósito**: sem isso as duas
        // tentativas correm juntas e o teste não prende nada — ele tem de ver
        // o impostor falhar antes de o legítimo fechar o aperto de mão.
        let impostor = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        passar_a_atender(&impostor, identidade_efemera().unwrap()).unwrap();
        let endereco_impostor = impostor.local_addr().unwrap();
        let _atendendo_impostor = atender_em_segundo_plano(impostor, std::time::Duration::ZERO);

        let legitimo = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let identidade = identidade_efemera().unwrap();
        let esperada = impressao(&identidade);
        passar_a_atender(&legitimo, identidade).unwrap();
        let endereco_legitimo = legitimo.local_addr().unwrap();
        let _atendendo_legitimo =
            atender_em_segundo_plano(legitimo, std::time::Duration::from_millis(150));

        let b = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let ligado = ligar(
            &b,
            &[endereco_impostor, endereco_legitimo],
            esperada,
            std::time::Duration::from_secs(5),
        )
        .await
        .expect("o impostor derrubou a discagem inteira");

        assert_eq!(
            ligado.conexao.remote_address(),
            endereco_legitimo,
            "a ligação fechou com quem não era o par"
        );
    }

    #[test]
    fn o_que_e_da_mesma_rede_nunca_conta_como_furo() {
        // **`ComoChegou` é o número que o subprojeto A existe para produzir.**
        // Um endereço da própria rede contado como `Furo` infla a taxa que vai
        // decidir se a árvore do subprojeto B pode supor que qualquer par se
        // alcança. O `fe80::/10` faltava — o lado v4 conferia `is_link_local()`
        // e o v6 não conferia nada equivalente.
        for texto in [
            "127.0.0.1:9",
            "10.0.0.1:9",
            "172.16.0.1:9",
            "192.168.1.10:9",
            "169.254.7.7:9",
            "[::1]:9",
            "[fd00::1]:9",
            "[fe80::1]:9",
            "[febf:ffff::1]:9",
        ] {
            assert_eq!(
                como_chegou(texto.parse().unwrap()),
                ComoChegou::Local,
                "{texto} é da mesma rede e foi contado como furo"
            );
        }
        for texto in ["203.0.113.5:9", "[2001:db8::1]:9"] {
            assert_eq!(
                como_chegou(texto.parse().unwrap()),
                ComoChegou::Furo,
                "{texto} não é da rede local"
            );
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn o_prazo_manda_quando_ninguem_responde() {
        // Dois endereços que nunca respondem (`192.0.2.0/24` e
        // `198.51.100.0/24` são as faixas de documentação, reservadas
        // justamente para isto). O prazo tem de mandar, e o motivo tem de ser
        // o de rotina — não o de segurança.
        //
        // **O que este teste não prova:** que a linha de rastro do prazo saiu.
        // Ele não instala assinante de `tracing` e não lê log nenhum; o que
        // prende é o prazo e o motivo. A linha em si é lida por olho humano,
        // e é honesto dizer isso aqui em vez de fingir cobertura.
        let b = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let comecou = std::time::Instant::now();
        let erro = ligar(
            &b,
            &[
                "192.0.2.1:9".parse().unwrap(),
                "198.51.100.1:9".parse().unwrap(),
            ],
            "f".repeat(64),
            std::time::Duration::from_millis(300),
        )
        .await
        .unwrap_err();

        assert!(
            matches!(erro, ErroDePar::NaoAlcancou),
            "silêncio não é evento de segurança: {erro:?}"
        );
        assert!(
            comecou.elapsed() < std::time::Duration::from_secs(3),
            "o prazo não mandou: a discagem levou {:?}",
            comecou.elapsed()
        );
    }
}
