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

impl Clone for Identidade {
    // Não dá para `#[derive(Clone)]`: `PrivateKeyDer` não implementa `Clone`,
    // só `clone_key()`. Quem chama `passar_a_atender` e depois `ligar` na
    // mesma ponta precisa das duas — a função consome a identidade — então
    // este `Clone` existe para essa cópia, e não para persistência nenhuma:
    // continua tudo em memória, como `identidade_efemera` documenta.
    fn clone(&self) -> Self {
        Self {
            cadeia: self.cadeia.clone(),
            chave: self.chave.clone_key(),
        }
    }
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
    /// Fechou aperto de mão do lado de quem disca, e quem atendeu recusou.
    ///
    /// **Não é o mesmo que [`Self::NaoAlcancou`].** Alguém respondeu, e a
    /// discagem chegou a concluir seu lado do TLS — o 1.3 considera isso
    /// pronto assim que processa o `Finished` do par, antes de saber se o par
    /// vai aceitar a contrapartida (o certificado de cliente que
    /// `ConfereQuemChega` pode exigir do lado de quem atende). A recusa
    /// chega depois, como fechamento desta conexão.
    #[error("a ligação fechou logo depois de conectar: {0}")]
    RecusadoDepoisDeLigar(MotivoDaRecusa),
    /// Alguém completou o aperto de mão, e o prazo venceu antes da
    /// confirmação de aplicação.
    ///
    /// **Não é [`Self::NaoAlcancou`], pela mesma razão que
    /// [`Self::ImpressaoNaoBate`] não é.** Aquele é silêncio total: nenhum
    /// candidato respondeu. Aqui, pelo menos um respondeu e completou o
    /// TLS — só não trocou o byte de confirmação a tempo, e dizer «nenhum
    /// endereço deste par respondeu» seria falso. Também não
    /// é [`Self::RecusadoDepoisDeLigar`]: aquele já tem um `CONNECTION_CLOSE`
    /// de verdade para classificar; aqui não chegou fechamento nenhum, só o
    /// prazo de `ligar` venceu primeiro.
    #[error("um candidato completou o aperto de mão, e o prazo venceu antes da confirmação")]
    ConfirmacaoNaoChegouATempo,
}

/// Por que a conexão fechou depois que quem disca já a considerava pronta.
///
/// **Enumerado, não texto solto** — `specs/02-protocolo.md` e o ADR 0012
/// exigem isto de todo motivo de erro que chega à interface, e
/// [`ErroDePar::ImpressaoNaoBate`] é o molde. Vem do `error_code` que o
/// `CONNECTION_CLOSE` do `quinn` carrega: um alerta TLS, na faixa
/// `0x100..0x200` de [RFC 8446 §6], quando é uma recusa de certificado — e
/// qualquer outra coisa quando não é.
///
/// [RFC 8446 §6]: https://www.rfc-editor.org/rfc/rfc8446#section-6
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MotivoDaRecusa {
    /// `certificate_required` (alerta 116): quem atendeu exige certificado de
    /// cliente, e não veio nenhum.
    #[error("não apresentou certificado nenhum")]
    SemCertificado,
    /// `handshake_failure` (40), `bad_certificate` (42) ou `unknown_ca` (48):
    /// veio um certificado, e quem atendeu não o aceitou.
    #[error("apresentou um certificado que não foi aceito")]
    CertificadoErrado,
    /// Fechou por outro motivo, sem relação com a conferência de certificado
    /// — prazo, reinício do par, ou algo que este catálogo ainda não nomeia.
    #[error("{0}")]
    Outro(String),
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
/// `impressao_de_quem_vem` é a impressão digital de quem vai discar para esta
/// ponta — a mesma que a mensagem `SirvaTelaPara` carrega. Sem ela, a conexão
/// que *chega* nunca passaria pelo `ConfereQuemChega`, porque os dois lados
/// discam e qualquer um pode acabar sendo quem aceita.
///
/// **Não guarda a identidade para quando esta mesma ponta disca.** Quem
/// também vai chamar [`ligar`] nesta ponta precisa passar a própria
/// identidade a ela — ver o parâmetro `identidade_propria` de [`ligar`] —,
/// porque esta função já consome a que recebeu.
///
/// # Errors
///
/// Falha se o `rustls` recusar o certificado ou a chave.
pub fn passar_a_atender(
    ponta: &quinn::Endpoint,
    identidade: Identidade,
    impressao_de_quem_vem: String,
) -> Result<(), ErroDePar> {
    let mut tls = rustls::ServerConfig::builder()
        .with_client_cert_verifier(std::sync::Arc::new(ConfereQuemChega::nova(
            impressao_de_quem_vem,
        )))
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

/// Desarma o que [`passar_a_atender`] armou.
///
/// **Não é limpeza opcional — achado do fix round 3.** Sem isto, a ponta
/// continua aceitando conexões pelo resto da sessão, muito depois de a única
/// vaga de [`atender`] já ter sido servida, com o verificador ainda fixado na
/// impressão do **último** par apontado. E QUIC sem `use_retry` responde ao
/// primeiro pacote de quem quer que bata, antes de qualquer autenticação:
/// uma porta armada é um refletor de amplificação (até 3× o `Initial`
/// recebido, contra qualquer endereço de origem que o pacote alegue) por todo
/// esse tempo — não só enquanto está de fato servindo alguém.
///
/// Chame depois que [`atender`] devolver, sirva ele ou não sirva. A conexão
/// de controle desta mesma ponta não é afetada: `set_server_config` só rege
/// o que a ponta faz com um `Initial` que chega, e uma conexão já
/// estabelecida não passa por aí de novo.
pub fn parar_de_atender(ponta: &quinn::Endpoint) {
    ponta.set_server_config(None);
}

/// Confere quem **chega**, contra a impressão que o servidor apresentou.
///
/// Espelho de [`ConfereImpressao`] na outra direção. As duas existem porque os
/// dois lados discam: quem disca confere com aquele, quem atende confere com
/// este, e sem os dois metade das ligações não passaria por conferência nenhuma.
#[derive(Debug)]
pub(crate) struct ConfereQuemChega {
    esperada: String,
    provedor: std::sync::Arc<rustls::crypto::CryptoProvider>,
}

impl ConfereQuemChega {
    /// Um verificador de cliente que só aceita esta impressão digital.
    #[must_use]
    pub(crate) fn nova(esperada: String) -> Self {
        Self {
            esperada,
            provedor: std::sync::Arc::new(rustls::crypto::ring::default_provider()),
        }
    }
}

impl rustls::server::danger::ClientCertVerifier for ConfereQuemChega {
    fn client_auth_mandatory(&self) -> bool {
        // Um par que não apresente certificado nenhum tem de ser recusado, não
        // aceito por omissão — é a diferença entre este verificador e
        // `with_no_client_auth()`, que aceitava qualquer um.
        true
    }

    fn root_hint_subjects(&self) -> &[rustls::DistinguishedName] {
        // Não há autoridade a sugerir: a conferência é por impressão digital,
        // não por cadeia.
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<rustls::server::danger::ClientCertVerified, rustls::Error> {
        let veio = seele_proto::transport::certificate_fingerprint(end_entity.as_ref());
        if veio == self.esperada {
            Ok(rustls::server::danger::ClientCertVerified::assertion())
        } else {
            // Quem chegou não é quem foi apresentado. O rastro diz o que veio
            // e o que se esperava — a mesma regra do `CLAUDE.md` deste
            // repositório de que o rastro conta qual endereço, não só que algo
            // falhou.
            tracing::warn!(
                esperada = %self.esperada,
                veio = %veio,
                "um par que chegou apresentou uma impressão digital diferente da esperada"
            );
            Err(rustls::Error::General(format!(
                "o par apresentou {veio}, e esperava-se {esperada}",
                esperada = self.esperada
            )))
        }
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
/// `identidade_propria` é o que esta ponta apresenta como certificado de
/// cliente, dada por quem chama — ver o parâmetro de mesmo nome em
/// [`ligar`]. Sem ela a discagem sai sem certificado nenhum, e só serve
/// contra um par cujo `ConfereQuemChega` ainda não seja mandatório.
///
/// # Errors
///
/// Falha se o `rustls` recusar a configuração.
pub(crate) fn config_de_cliente(
    esperada: String,
    identidade_propria: Option<&Identidade>,
) -> Result<(quinn::ClientConfig, Relato), ErroDePar> {
    let confere = ConfereImpressao::nova(esperada);
    let relato = std::sync::Arc::clone(&confere.relato);
    let sem_certificado = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(std::sync::Arc::new(confere));
    let mut tls = match identidade_propria {
        Some(identidade) => sem_certificado
            .with_client_auth_cert(identidade.cadeia.clone(), identidade.chave.clone_key())
            .map_err(|erro| ErroDePar::Escuta(erro.to_string()))?,
        None => sem_certificado.with_no_client_auth(),
    };
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
    identidade_propria: Option<&Identidade>,
    prazo: std::time::Duration,
) -> Result<ParLigado, ErroDePar> {
    let mut tentativas = tokio::task::JoinSet::new();
    // **Marca se alguma tentativa chegou a completar o TLS.** Uma tentativa
    // presa em `confirmar_com_quem_atende` quando o prazo vence é abortada
    // sem nunca devolver nada — e sem esta marca, `ultimo` continuaria no
    // `NaoAlcancou` inicial, que é falso: alguém respondeu, e completou o
    // aperto de mão. É a mesma distinção que faz `ImpressaoNaoBate` não ser
    // `NaoAlcancou`, só que para um candidato que trava depois de conectar
    // em vez de mentir sobre quem é.
    let apertou_a_mao = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    for endereco in enderecos {
        // `config_de_cliente` roda aqui, fora da tarefa, e devolve uma
        // configuração já dona dos próprios bytes — é o que deixa
        // `identidade_propria` (emprestada de quem chamou `ligar`, e por
        // isso não `'static`) de fora do `async move` logo abaixo.
        let (config, relato) = config_de_cliente(impressao_esperada.clone(), identidade_propria)?;
        let ponta = ponta.clone();
        let endereco = *endereco;
        let apertou_a_mao = std::sync::Arc::clone(&apertou_a_mao);
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
                apertou_a_mao.store(true, std::sync::atomic::Ordering::Relaxed);
                // **O aperto de mão do lado de quem disca conclui antes de
                // saber se o par vai aceitar a contrapartida.** O TLS 1.3
                // considera a sessão pronta assim que processa o `Finished`
                // do par — antes de enviar, e muito antes de o par validar, o
                // certificado de cliente que o `ConfereQuemChega` dele pode
                // exigir. Uma recusa por causa disso chega como fechamento
                // desta conexão, não como erro deste `.await`; a troca de um
                // byte com quem atende, a seguir, é o sinal determinístico de
                // que a ligação foi mesmo aceita.
                if let Err(motivo) = confirmar_com_quem_atende(&conexao).await {
                    tracing::warn!(
                        par = %endereco,
                        %motivo,
                        "a ligação fechou logo depois de conectar"
                    );
                    return Err(ErroDePar::RecusadoDepoisDeLigar(motivo));
                }
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
    // **A tentativa abortada não teve como falar por si.** Se `ultimo` ainda
    // é o `NaoAlcancou` de largada, e alguma tentativa chegou a completar o
    // TLS, `NaoAlcancou` está mentindo — foi abortada depois de apertar a
    // mão, não por silêncio nenhum.
    if matches!(ultimo, ErroDePar::NaoAlcancou)
        && apertou_a_mao.load(std::sync::atomic::Ordering::Relaxed)
    {
        ultimo = ErroDePar::ConfirmacaoNaoChegouATempo;
    }
    Err(ultimo)
}

/// Atende **uma** ligação de par, e devolve o que chegou.
///
/// Uma só, e não um laço: no A1 quem empresta serve um par por vez, e um laço
/// aqui prometeria a topologia que o subprojeto B ainda vai desenhar.
///
/// **É a metade que faltava de [`passar_a_atender`].** Instalar o
/// `ServerConfig` não basta: o `quinn` enfileira a tentativa que chega e não
/// responde nada até alguém chamar [`quinn::Endpoint::accept`]. Sem esta
/// função, dois pares que só chamem [`ligar`] nunca se ligam.
///
/// `prazo_de_confirmacao` limita só a espera **depois** do aperto de mão, pela
/// troca de byte que substitui o relógio em [`ligar`] (ver
/// `confirmar_com_quem_atende`). **É obrigatório, e não um enfeite:** sem
/// prazo nenhum, `accept_bi` fica preso ao `max_idle_timeout` do `quinn`
/// (30 s nos padrões 0.11.11 deste crate) se o par ficar calado —
/// **indefinidamente** se ele mandar qualquer coisa para manter a conexão
/// viva sem nunca abrir o fluxo. Como esta função serve **uma** vaga só, um
/// par autenticado e parado nega a vaga inteira de quem empresta a subida
/// até vencer esse relógio. Não limita a espera por alguém chegar — essa,
/// por desenho, não tem prazo.
///
/// `None` quando a ponta fechou sem ninguém chegar, quando quem chegou não
/// completou o aperto de mão — inclusive por `ConfereQuemChega` ter recusado
/// o certificado apresentado —, ou quando apertou a mão e não confirmou
/// dentro do prazo.
pub async fn atender(
    ponta: quinn::Endpoint,
    prazo_de_confirmacao: std::time::Duration,
) -> Option<ParLigado> {
    let chegando = ponta.accept().await?;
    // Lido **antes** do `.await` que segue: `chegando` já sabe de onde a
    // tentativa veio, e uma recusa por aí não pode ficar sem nome — é a
    // mesma regra do `CLAUDE.md` deste repositório que a Task 4 já pagou por
    // ignorar uma vez.
    let remoto = chegando.remote_address();
    let conexao = match chegando.await {
        Ok(conexao) => conexao,
        Err(erro) => {
            tracing::warn!(par = %remoto, %erro, "uma ligação que chegou não fechou o aperto de mão");
            return None;
        }
    };
    // A metade de quem atende na troca que substitui o relógio em `ligar` —
    // ver `confirmar_com_quem_atende`. Com prazo próprio: sem ele, um par que
    // aperta a mão e some prenderia esta vaga até o `max_idle_timeout` do
    // `quinn` — ou para sempre, se mandar qualquer coisa para manter a
    // conexão viva.
    match tokio::time::timeout(prazo_de_confirmacao, confirmar_para_quem_ligou(&conexao)).await {
        Ok(Ok(())) => {}
        Ok(Err(erro)) => {
            tracing::warn!(par = %remoto, %erro, "a troca de confirmação com quem ligou falhou");
            return None;
        }
        Err(_elapsed) => {
            tracing::warn!(
                par = %remoto,
                ?prazo_de_confirmacao,
                "quem ligou apertou a mão e não confirmou dentro do prazo"
            );
            return None;
        }
    }
    let ida_e_volta = conexao.rtt();
    let como = como_chegou(conexao.remote_address());
    tracing::info!(par = %conexao.remote_address(), ?como, ?ida_e_volta, "um par foi atendido");
    Some(ParLigado {
        conexao,
        como,
        ida_e_volta,
    })
}

/// De onde a imagem desta transmissão vai vir.
///
/// `Box` em [`Self::Par`] porque [`ParLigado`] carrega uma [`quinn::Connection`]
/// e [`Self::Servidor`] não carrega nada além do motivo: sem a caixa o `enum`
/// inteiro teria o tamanho da maior variante, e o `clippy::large_enum_variant`
/// reclamaria com razão — a variante pequena pagaria pelo tamanho da grande a
/// cada vez que aparecesse.
#[derive(Debug)]
pub enum PorOndeAssistir {
    /// Por este par.
    Par(Box<ParLigado>),
    /// Pelo servidor, como sempre — e por quê.
    ///
    /// **Achado do fix round 1 da Task 8.** A primeira versão não devolvia
    /// motivo nenhum, só o registrava no `tracing`; mas o motivo enumerado
    /// existe para o **servidor** saber o que aconteceu, não só para quem lê
    /// o log desta máquina. `ImpressaoNaoBate` é o evento de segurança que o
    /// §4 da spec quer contado como tal, e escondê-lo dentro de um
    /// `NaoAlcancou` genérico apagaria a diferença entre «ninguém respondeu» e
    /// «alguém respondeu no lugar do par» bem no ponto em que ela mais
    /// importa: o relato que chega ao servidor.
    Servidor(seele_proto::control::MotivoDeFalhaDePar),
}

/// Tenta o par, e cai para o servidor sem drama quando ele não vem.
///
/// **Nunca devolve erro**, e é de propósito: quem chama não tem decisão a tomar
/// sobre a falha. A malha é alívio; falhar nela é voltar ao caminho de antes
/// dela existir, e isso não é um erro, é o normal — a mesma regra que a spec de
/// 05/09 registra: ninguém perde imagem por causa da máquina de outra pessoa.
///
/// `identidade_propria` é o que esta ponta apresenta como certificado de
/// cliente ao discar — ver o parâmetro de mesmo nome em [`ligar`]. **Não é
/// opcional na prática**, mesmo sendo `Option` aqui: a parede simétrica da
/// Task 5 faz quem atende exigir certificado sempre
/// (`ConfereQuemChega::client_auth_mandatory`), então quem chama sem
/// identidade recebe `RecusadoDepoisDeLigar(SemCertificado)` de qualquer par
/// que exista de verdade. `None` só serve para o caso em que não há
/// identidade nenhuma a apresentar — o que hoje não deveria acontecer, porque
/// `enlace::Motor` declara a própria identidade ao servidor assim que conecta
/// (ver o doc de `Motor::declarar_identidade_de_par`), empreste ela a subida
/// ou não.
///
/// O motivo enumerado da falha **não some**: vai para o `tracing` **e** para o
/// [`PorOndeAssistir::Servidor`] que esta função devolve — quem chama não
/// precisa mais adivinhar ou achatar tudo num motivo genérico para avisar o
/// servidor.
pub async fn por_onde(
    ponta: &quinn::Endpoint,
    enderecos: &[std::net::SocketAddr],
    impressao: String,
    identidade_propria: Option<&Identidade>,
    prazo: std::time::Duration,
) -> PorOndeAssistir {
    match ligar(ponta, enderecos, impressao, identidade_propria, prazo).await {
        Ok(ligado) => PorOndeAssistir::Par(Box::new(ligado)),
        Err(erro) => {
            let motivo = motivo_de_falha(&erro);
            tracing::info!(%erro, ?motivo, "o par não veio; a tela vem do servidor");
            PorOndeAssistir::Servidor(motivo)
        }
    }
}

/// Traduz o motivo detalhado de [`ligar`] para o motivo enumerado que o
/// protocolo leva ao servidor.
///
/// **Duas traduções exatas, e o resto é rotina.**
/// [`ErroDePar::ImpressaoNaoBate`] é evento de segurança — alguém respondeu no
/// lugar de quem o servidor apresentou — e vira
/// [`MotivoDeFalhaDePar::ImpressaoNaoBate`].
/// [`ErroDePar::RecusadoDepoisDeLigar`] é o inverso — quem respondeu **era**
/// quem o servidor apresentou, e foi ele que recusou a identidade que eu
/// ofereci — e vira [`MotivoDeFalhaDePar::NaoFuiAceito`] (fix round 2: os dois
/// pedem consertos opostos do lado do servidor, e viajar como o mesmo motivo
/// escondia essa diferença). Tudo o mais — certificado que não gerou, ponta
/// que não abriu, silêncio, confirmação que não chegou a tempo — é rotina de
/// rede sem nada a provar sobre nenhuma declaração, e cai em
/// [`MotivoDeFalhaDePar::NaoAlcancou`]: a leitura mais honesta disponível sem
/// inventar uma das outras duas variantes, que descrevem falhas de **depois**
/// de já estar servindo (`CaiuNoMeio`, `ParouDeMandar`), não desta discagem.
fn motivo_de_falha(erro: &ErroDePar) -> seele_proto::control::MotivoDeFalhaDePar {
    match erro {
        ErroDePar::ImpressaoNaoBate { .. } => {
            seele_proto::control::MotivoDeFalhaDePar::ImpressaoNaoBate
        }
        ErroDePar::RecusadoDepoisDeLigar(_) => {
            seele_proto::control::MotivoDeFalhaDePar::NaoFuiAceito
        }
        ErroDePar::Certificado(_)
        | ErroDePar::Escuta(_)
        | ErroDePar::NaoAlcancou
        | ErroDePar::ConfirmacaoNaoChegouATempo => {
            seele_proto::control::MotivoDeFalhaDePar::NaoAlcancou
        }
    }
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

/// Troca um byte com quem atende, e só então dá a ligação por boa.
///
/// **Por que existe, e por que não é um relógio.** No TLS 1.3, quem disca
/// conclui seu lado do aperto de mão assim que processa o `Finished` do
/// par — antes de o par validar a contrapartida que ele próprio pediu (o
/// certificado de cliente que `ConfereQuemChega` pode exigir). Uma recusa por
/// causa disso chega como `CONNECTION_CLOSE`, e esse datagrama **não se
/// retransmite sozinho**: perdido ele, um relógio (folga de RTT, o desenho
/// anterior desta função) deixaria `ligar` dar por boa uma ligação que o
/// outro lado já fechou. Um fluxo de aplicação que só fecha depois de o
/// outro lado responder é o sinal determinístico: numa conexão recusada, o
/// `open_bi` ou a leitura seguinte falha com o `ConnectionError` de verdade,
/// sem margem e sem relógio.
///
/// [`confirmar_para_quem_ligou`] é a metade que responde, do lado de
/// [`atender`].
async fn confirmar_com_quem_atende(conexao: &quinn::Connection) -> Result<(), MotivoDaRecusa> {
    let (mut envio, mut recebe) = conexao
        .open_bi()
        .await
        .map_err(|erro| motivo_da_recusa(&erro))?;
    if let Err(erro) = envio.write_all(&[0]).await {
        return Err(motivo_do_erro_de_envio(erro));
    }
    let mut resposta = [0u8; 1];
    if let Err(erro) = recebe.read_exact(&mut resposta).await {
        return Err(motivo_do_erro_de_leitura(erro));
    }
    let _ = envio.finish();
    Ok(())
}

/// A metade de [`atender`] na troca de [`confirmar_com_quem_atende`]: lê o
/// byte que quem discou mandou, e devolve outro.
async fn confirmar_para_quem_ligou(conexao: &quinn::Connection) -> Result<(), std::io::Error> {
    let (mut envio, mut recebe) = conexao.accept_bi().await.map_err(std::io::Error::other)?;
    let mut byte = [0u8; 1];
    recebe
        .read_exact(&mut byte)
        .await
        .map_err(std::io::Error::other)?;
    envio.write_all(&[0]).await.map_err(std::io::Error::other)?;
    let _ = envio.finish();
    Ok(())
}

/// Classifica um `ConnectionError` pelo alerta TLS que o `CONNECTION_CLOSE`
/// carrega, quando ele carrega um.
///
/// Os números são os do registro de alertas da TLS — [RFC 8446 §6] —, e o
/// `quinn` os expõe como `TransportErrorCode::crypto(alerta)` na faixa
/// `0x100..0x200`.
///
/// [RFC 8446 §6]: https://www.rfc-editor.org/rfc/rfc8446#section-6
fn motivo_da_recusa(erro: &quinn::ConnectionError) -> MotivoDaRecusa {
    let quinn::ConnectionError::ConnectionClosed(fechamento) = erro else {
        return MotivoDaRecusa::Outro(erro.to_string());
    };
    if fechamento.error_code == quinn::TransportErrorCode::crypto(116) {
        // `certificate_required`.
        MotivoDaRecusa::SemCertificado
    } else if [40, 42, 48]
        .into_iter()
        .any(|alerta| fechamento.error_code == quinn::TransportErrorCode::crypto(alerta))
    {
        // `handshake_failure`, `bad_certificate`, `unknown_ca`.
        MotivoDaRecusa::CertificadoErrado
    } else {
        MotivoDaRecusa::Outro(erro.to_string())
    }
}

/// Recolhe o `ConnectionError` de dentro de um erro de envio, se houver um.
fn motivo_do_erro_de_envio(erro: quinn::WriteError) -> MotivoDaRecusa {
    match erro {
        quinn::WriteError::ConnectionLost(erro) => motivo_da_recusa(&erro),
        outro => MotivoDaRecusa::Outro(outro.to_string()),
    }
}

/// Recolhe o `ConnectionError` de dentro de um erro de leitura, se houver um.
fn motivo_do_erro_de_leitura(erro: quinn::ReadExactError) -> MotivoDaRecusa {
    match erro {
        quinn::ReadExactError::ReadError(quinn::ReadError::ConnectionLost(erro)) => {
            motivo_da_recusa(&erro)
        }
        outro => MotivoDaRecusa::Outro(outro.to_string()),
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

    /// O prazo de confirmação que os testes deste módulo dão a `atender`.
    ///
    /// Generoso para `127.0.0.1` (onde um RTT real custa microssegundos) e
    /// curto o bastante para um teste que trava por causa deste prazo faltar
    /// não segurar a suíte pelos 30 s do `max_idle_timeout` do `quinn`, ou
    /// pior, para sempre.
    const PRAZO_DE_CONFIRMACAO_NO_TESTE: std::time::Duration = std::time::Duration::from_secs(2);

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

        passar_a_atender(
            &ponta,
            identidade_efemera().unwrap(),
            impressao(&identidade_efemera().unwrap()),
        )
        .unwrap();

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
        let b = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let ib = identidade_efemera().unwrap();
        let impressao_b = impressao(&ib);
        passar_a_atender(&a, ia, impressao_b).unwrap();
        let endereco_a = a.local_addr().unwrap();
        let _atendendo_a = tokio::spawn(atender(a, PRAZO_DE_CONFIRMACAO_NO_TESTE));

        let ib_para_discar = ib.clone();
        passar_a_atender(&b, ib, impressao_a.clone()).unwrap();

        let ligado = ligar(
            &b,
            &[endereco_a],
            impressao_a,
            Some(&ib_para_discar),
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
        passar_a_atender(
            &a,
            identidade_efemera().unwrap(),
            impressao(&identidade_efemera().unwrap()),
        )
        .unwrap();
        let endereco_a = a.local_addr().unwrap();
        let _atendendo_a = tokio::spawn(atender(a, PRAZO_DE_CONFIRMACAO_NO_TESTE));

        let b = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let erro = ligar(
            &b,
            &[endereco_a],
            "f".repeat(64),
            None,
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
        let b = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let ib = identidade_efemera().unwrap();
        let impressao_b = impressao(&ib);

        let impostor = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        passar_a_atender(
            &impostor,
            identidade_efemera().unwrap(),
            impressao_b.clone(),
        )
        .unwrap();
        let endereco_impostor = impostor.local_addr().unwrap();
        let _atendendo_impostor = tokio::spawn(atender(impostor, PRAZO_DE_CONFIRMACAO_NO_TESTE));

        let legitimo = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let identidade = identidade_efemera().unwrap();
        let esperada = impressao(&identidade);
        passar_a_atender(&legitimo, identidade, impressao_b).unwrap();
        let endereco_legitimo = legitimo.local_addr().unwrap();
        let _atendendo_legitimo = tokio::spawn(async move {
            // O legítimo atende com atraso **de propósito**: sem isso as duas
            // tentativas correm juntas e o teste não prende nada — ele tem de
            // ver o impostor falhar antes de o legítimo fechar o aperto de
            // mão.
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            atender(legitimo, PRAZO_DE_CONFIRMACAO_NO_TESTE).await
        });

        let ib_para_discar = ib.clone();
        passar_a_atender(&b, ib, esperada.clone()).unwrap();
        let ligado = ligar(
            &b,
            &[endereco_impostor, endereco_legitimo],
            esperada,
            Some(&ib_para_discar),
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

    #[tokio::test(flavor = "multi_thread")]
    async fn quem_atende_recusa_quem_o_servidor_nao_apresentou() {
        // **A parede simétrica.** Os dois lados discam, então qualquer um dos dois
        // pode acabar sendo quem aceita — e quem aceita não passa pelo
        // `ConfereImpressao`, que só roda em quem disca. Sem esta parede, metade
        // das ligações não confere nada, e quem empresta a subida serve quadro a
        // qualquer um que alcance a porta.
        //
        // Não é hipótese: `with_no_client_auth()` é literalmente «aceite qualquer
        // um».
        let anfitriao = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let ia = identidade_efemera().unwrap();
        let impressao_do_anfitriao = impressao(&ia);

        let intruso = identidade_efemera().unwrap();
        let esperada_de_outro = impressao(&identidade_efemera().unwrap());

        // O anfitrião só aceita quem apresentar `esperada_de_outro` — e o intruso
        // apresenta a dele, que é outra.
        passar_a_atender(&anfitriao, ia, esperada_de_outro).unwrap();
        let onde = anfitriao.local_addr().unwrap();
        let atendendo = tokio::spawn(atender(anfitriao.clone(), PRAZO_DE_CONFIRMACAO_NO_TESTE));

        let ponta_do_intruso = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let intruso_para_discar = intruso.clone();
        passar_a_atender(&ponta_do_intruso, intruso, impressao_do_anfitriao.clone()).unwrap();
        let tentou = ligar(
            &ponta_do_intruso,
            &[onde],
            impressao_do_anfitriao,
            Some(&intruso_para_discar),
            std::time::Duration::from_secs(3),
        )
        .await;

        // **A variante exata, não só `is_err()`.** `RecusadoDepoisDeLigar` e
        // `MotivoDaRecusa` existem para distinguir «apresentou o errado» de
        // «não apresentou nenhum» — uma regressão que jogasse os dois em
        // `MotivoDaRecusa::Outro(String)` deixaria só `is_err()` verde.
        assert!(
            matches!(
                tentou,
                Err(ErroDePar::RecusadoDepoisDeLigar(
                    MotivoDaRecusa::CertificadoErrado
                ))
            ),
            "o intruso apresentou um certificado que o anfitrião nunca esperou, e entrou \
             (ou saiu com o motivo errado): {tentou:?}"
        );
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(200), atendendo)
                .await
                .map(|ligado| ligado.ok().flatten().is_none())
                .unwrap_or(true),
            "o anfitrião deu por boa uma ligação de quem o servidor não apresentou"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn dois_pares_apresentados_se_ligam_pelos_dois_lados() {
        // O caso feliz, e a razão de `atender` existir no produto e não só no
        // teste: com os dois lados discando **e** os dois lados atendendo, a
        // primeira ligação que fechar vence, venha de que direção vier. É isso que
        // faz o furo assimétrico se resolver sozinho.
        let a = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let ia = identidade_efemera().unwrap();
        let impressao_a = impressao(&ia);
        let b = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let ib = identidade_efemera().unwrap();
        let impressao_b = impressao(&ib);

        let ib_para_discar = ib.clone();
        passar_a_atender(&a, ia, impressao_b.clone()).unwrap();
        passar_a_atender(&b, ib, impressao_a.clone()).unwrap();
        let onde_a = a.local_addr().unwrap();
        let atendendo = tokio::spawn(atender(a.clone(), PRAZO_DE_CONFIRMACAO_NO_TESTE));

        let ligado = ligar(
            &b,
            &[onde_a],
            impressao_a,
            Some(&ib_para_discar),
            std::time::Duration::from_secs(5),
        )
        .await
        .expect("dois pares apresentados um ao outro não se ligaram");
        assert_eq!(ligado.como, ComoChegou::Local);

        let do_outro_lado = tokio::time::timeout(std::time::Duration::from_secs(2), atendendo)
            .await
            .expect("o lado que atende travou")
            .expect("a tarefa de atender morreu");
        assert!(
            do_outro_lado.is_some(),
            "quem atendeu não devolveu a ligação: quem empresta não tem por onde mandar quadro"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn quem_atende_recusa_quem_nao_apresentou_certificado_nenhum() {
        // **Por que este teste existe além do da parede simétrica.** Naquele,
        // o intruso tem identidade própria — registrada ao chamar
        // `passar_a_atender` nele mesmo — e o par certo o recusa por a
        // impressão não bater. Isso prova `verify_client_cert`, mas nunca
        // exercita `client_auth_mandatory`: revertê-lo para `false` e rodar
        // toda a suíte não derruba teste nenhum, porque nenhum outro chega ao
        // aperto de mão com o certificado do cliente genuinamente vazio.
        //
        // Este é esse caso: quem disca nunca chamou `passar_a_atender`, então
        // não tem identidade nenhuma registrada — e o certificado do
        // anfitrião é o certo, então a discagem chega inteira até a decisão
        // de `client_auth_mandatory`.
        let anfitriao = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let ia = identidade_efemera().unwrap();
        let impressao_do_anfitriao = impressao(&ia);
        let esperada_de_quem_liga = impressao(&identidade_efemera().unwrap());
        passar_a_atender(&anfitriao, ia, esperada_de_quem_liga).unwrap();
        let onde = anfitriao.local_addr().unwrap();
        let atendendo = tokio::spawn(atender(anfitriao, PRAZO_DE_CONFIRMACAO_NO_TESTE));

        let sem_identidade = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let tentou = ligar(
            &sem_identidade,
            &[onde],
            impressao_do_anfitriao,
            None,
            std::time::Duration::from_secs(3),
        )
        .await;

        // A variante exata — ver o comentário equivalente em
        // `quem_atende_recusa_quem_o_servidor_nao_apresentou`.
        assert!(
            matches!(
                tentou,
                Err(ErroDePar::RecusadoDepoisDeLigar(
                    MotivoDaRecusa::SemCertificado
                ))
            ),
            "quem nunca apresentou certificado nenhum entrou mesmo assim \
             (ou saiu com o motivo errado): {tentou:?}"
        );
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(200), atendendo)
                .await
                .map(|ligado| ligado.ok().flatten().is_none())
                .unwrap_or(true),
            "o anfitrião deu por boa uma ligação sem certificado de cliente nenhum"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn atender_nao_trava_para_sempre_com_um_par_que_aperta_a_mao_e_para() {
        // **O achado do round 2 da revisão.** Antes da troca de byte, `atender`
        // devolvia assim que o TLS fechava. Agora ela também espera o par abrir
        // o fluxo de confirmação — e sem prazo próprio, um par que autentica e
        // some prenderia a vaga inteira até o `max_idle_timeout` do `quinn`
        // (30 s nos padrões), ou para sempre se mandar algo para manter a
        // conexão viva. Este teste finge exatamente esse par: completa o TLS
        // discando direto (sem passar por `ligar`, que sempre confirma) e
        // nunca abre fluxo nenhum.
        let anfitriao = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let ia = identidade_efemera().unwrap();
        let impressao_do_anfitriao = impressao(&ia);
        let quieto = identidade_efemera().unwrap();
        let impressao_do_quieto = impressao(&quieto);
        passar_a_atender(&anfitriao, ia, impressao_do_quieto).unwrap();
        let onde = anfitriao.local_addr().unwrap();
        let prazo_de_confirmacao = std::time::Duration::from_millis(150);
        let atendendo = tokio::spawn(atender(anfitriao, prazo_de_confirmacao));

        let ponta_do_quieto = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let (config, _relato) = config_de_cliente(impressao_do_anfitriao, Some(&quieto)).unwrap();
        let conexao_quieta = ponta_do_quieto
            .connect_with(config, onde, "seele-par")
            .unwrap()
            .await
            .unwrap();

        // Um limite bem maior que `prazo_de_confirmacao`: o que se prova aqui
        // é que `atender` volta **por causa do prazo dele**, não por acaso do
        // agendador.
        let resultado = tokio::time::timeout(std::time::Duration::from_secs(2), atendendo)
            .await
            .expect("atender devia ter voltado dentro do próprio prazo, e travou")
            .expect("a tarefa de atender morreu");
        assert!(
            resultado.is_none(),
            "atender deu por boa uma ligação que apertou a mão e nunca confirmou"
        );
        drop(conexao_quieta); // mantida viva de propósito até aqui
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn ligar_nao_diz_naoalcancou_para_quem_apertou_a_mao_e_travou() {
        // **O terceiro achado do round 2.** `NaoAlcancou` diz «nenhum endereço
        // respondeu» — falso aqui: o anfitrião completa o TLS e só não abre o
        // fluxo de confirmação (segura a conexão de propósito, sem nunca
        // chamar `accept_bi`). `ligar` tem de saber a diferença, pela mesma
        // razão que `ImpressaoNaoBate` não é `NaoAlcancou`.
        let anfitriao = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let ia = identidade_efemera().unwrap();
        let impressao_do_anfitriao = impressao(&ia);
        let quem_disca = identidade_efemera().unwrap();
        let impressao_de_quem_disca = impressao(&quem_disca);
        passar_a_atender(&anfitriao, ia, impressao_de_quem_disca).unwrap();
        let onde = anfitriao.local_addr().unwrap();

        let _segurando = tokio::spawn(async move {
            if let Some(chegando) = anfitriao.accept().await {
                if let Ok(_conexao_apertada) = chegando.await {
                    // Aperta a mão, e para — de propósito, sem `accept_bi`.
                    std::future::pending::<()>().await;
                }
            }
        });

        let ponta_de_quem_disca = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let erro = ligar(
            &ponta_de_quem_disca,
            &[onde],
            impressao_do_anfitriao,
            Some(&quem_disca),
            std::time::Duration::from_millis(300),
        )
        .await
        .unwrap_err();

        assert!(
            matches!(erro, ErroDePar::ConfirmacaoNaoChegouATempo),
            "o par apertou a mão e travou, e o motivo devolvido foi outro: {erro:?}"
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
            None,
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

    #[tokio::test(flavor = "multi_thread")]
    async fn quando_o_par_nao_liga_a_resposta_e_o_servidor() {
        // **A malha é alívio, nunca dependência.** Decisão de quem desenha o
        // produto, 05/09/2026: ninguém perde imagem por causa da máquina de outra
        // pessoa. É a propriedade de segurança da malha inteira, e por isso ela é
        // provada aqui e não adiada para o subprojeto B — uma propriedade de
        // segurança provada depois é uma propriedade que passou um tempo sem
        // existir.
        let ponta = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        // Uma porta em que ninguém atende: o endereço é válido e o aperto de mão
        // nunca fecha.
        let ninguem = std::net::SocketAddr::from(([127, 0, 0, 1], 1));

        let onde = por_onde(
            &ponta,
            &[ninguem],
            "a".repeat(64),
            None,
            std::time::Duration::from_millis(300),
        )
        .await;

        assert!(
            matches!(
                onde,
                PorOndeAssistir::Servidor(seele_proto::control::MotivoDeFalhaDePar::NaoAlcancou)
            ),
            "o par não ligou e o cliente não caiu para o servidor com o motivo certo: {onde:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn quando_alguem_responde_no_lugar_do_par_o_motivo_nao_vira_naoalcancou() {
        // **Achado do fix round 1.** A primeira versão de `por_onde` escondia
        // até o motivo detalhado do chamador, e por isso todo `Servidor` saía
        // com `NaoAlcancou` — inclusive quando alguém tinha respondido no
        // lugar do par de verdade. `NaoAlcancou` é rotina; `ImpressaoNaoBate`
        // é o evento de segurança do §4 da spec, e o servidor precisa saber a
        // diferença, não só o `tracing` local desta máquina.
        let a = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        passar_a_atender(
            &a,
            identidade_efemera().unwrap(),
            impressao(&identidade_efemera().unwrap()),
        )
        .unwrap();
        let endereco_a = a.local_addr().unwrap();
        let _atendendo_a = tokio::spawn(atender(a, PRAZO_DE_CONFIRMACAO_NO_TESTE));

        let b = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let onde = por_onde(
            &b,
            &[endereco_a],
            "f".repeat(64),
            None,
            std::time::Duration::from_secs(5),
        )
        .await;

        assert!(
            matches!(
                onde,
                PorOndeAssistir::Servidor(
                    seele_proto::control::MotivoDeFalhaDePar::ImpressaoNaoBate
                )
            ),
            "alguém respondeu no lugar do par, e o motivo devolvido não foi o de segurança: {onde:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn quando_o_anfitriao_recusa_minha_identidade_o_motivo_e_naofuiaceito() {
        // **Achado do fix round 2.** `RecusadoDepoisDeLigar` é o inverso de
        // `ImpressaoNaoBate`: aqui quem respondeu **era** quem o servidor
        // apresentou, e foi ele que recusou a identidade que eu ofereci — não
        // impostura do par, e sim (o caso comum) a minha própria declaração
        // desatualizada. Antes desta variante existir, os dois motivos
        // viajavam como `NaoAlcancou`, que diz «ninguém respondeu» — falso
        // quando o TLS fechou dos dois lados.
        let anfitriao = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let ia = identidade_efemera().unwrap();
        let impressao_do_anfitriao = impressao(&ia);
        let esperada_de_quem_liga = impressao(&identidade_efemera().unwrap());
        passar_a_atender(&anfitriao, ia, esperada_de_quem_liga).unwrap();
        let onde_atende = anfitriao.local_addr().unwrap();
        let _atendendo = tokio::spawn(atender(anfitriao, PRAZO_DE_CONFIRMACAO_NO_TESTE));

        // Quem disca não tem a identidade que o anfitrião espera — a mesma
        // configuração de `quem_atende_recusa_quem_nao_apresentou_certificado_nenhum`,
        // só que por `por_onde`.
        let sem_a_identidade_certa =
            quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let onde = por_onde(
            &sem_a_identidade_certa,
            &[onde_atende],
            impressao_do_anfitriao,
            None,
            std::time::Duration::from_secs(3),
        )
        .await;

        assert!(
            matches!(
                onde,
                PorOndeAssistir::Servidor(seele_proto::control::MotivoDeFalhaDePar::NaoFuiAceito)
            ),
            "o anfitrião recusou a identidade de quem discou, e o motivo não foi NaoFuiAceito: {onde:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn parar_de_atender_desarma_o_que_passar_a_atender_armou() {
        // **O achado do fix round 3.** Sem `parar_de_atender`, a ponta
        // continua aceitando conexões pelo resto da sessão depois de servir a
        // única vaga de `atender` — com o verificador fixado na impressão do
        // último par, e funcionando como refletor de amplificação para quem
        // quer que bata, sem `use_retry`.
        // As duas identidades combinam de propósito: se `parar_de_atender`
        // não desarmar nada, este par fecha o aperto de mão sem obstáculo
        // nenhum, e o guarda tem algo de verdade para prender — um `ligar`
        // que desse `NaoAlcancou` mesmo com a ponta armada (por exemplo, por
        // impressões que nunca bateriam) não provaria nada sobre
        // `parar_de_atender`.
        let ponta = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let identidade_da_ponta = identidade_efemera().unwrap();
        let impressao_da_ponta = impressao(&identidade_da_ponta);
        let identidade_de_quem_discaria = identidade_efemera().unwrap();
        let impressao_de_quem_discaria = impressao(&identidade_de_quem_discaria);
        passar_a_atender(&ponta, identidade_da_ponta, impressao_de_quem_discaria).unwrap();
        let onde = ponta.local_addr().unwrap();

        parar_de_atender(&ponta);

        // Alguém pronto para aceitar, mesmo assim: se a ponta continuasse
        // armada, é `atender` quem completaria o aperto de mão.
        let _atendendo = tokio::spawn(atender(ponta, PRAZO_DE_CONFIRMACAO_NO_TESTE));

        let discando = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let tentou = ligar(
            &discando,
            &[onde],
            impressao_da_ponta,
            Some(&identidade_de_quem_discaria),
            std::time::Duration::from_millis(300),
        )
        .await;

        assert!(
            matches!(tentou, Err(ErroDePar::NaoAlcancou)),
            "a ponta continuou atendendo depois de parar_de_atender: {tentou:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_ponta_atende_um_par_sem_derrubar_a_conexao_de_controle() {
        // **A propriedade central do §3.1, provada em código.** O teste
        // antigo (`uma_ponta_de_cliente_passa_a_atender_sem_socket_novo`) só
        // confere que a porta local não muda — ele nunca tem conexão nenhuma
        // no ar para derrubar, e o próprio comentário dele admite isso. Aqui
        // há uma conexão de controle de verdade, contra um "servidor" QUIC de
        // teste, **antes** de `passar_a_atender` entrar em cena na MESMA
        // ponta; a prova que importa é que essa conexão continua trocando
        // bytes depois.

        // O "servidor" — um par comum atendendo, fazendo o papel do
        // seele-server só para dar a `ponta` uma conexão de controle de
        // verdade.
        let servidor = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let identidade_do_servidor = identidade_efemera().unwrap();
        let impressao_do_servidor = impressao(&identidade_do_servidor);
        let identidade_da_ponta = identidade_efemera().unwrap();
        let impressao_da_ponta = impressao(&identidade_da_ponta);
        passar_a_atender(&servidor, identidade_do_servidor, impressao_da_ponta).unwrap();
        let endereco_do_servidor = servidor.local_addr().unwrap();
        let atendendo_o_controle = tokio::spawn(atender(servidor, PRAZO_DE_CONFIRMACAO_NO_TESTE));

        // `ponta`: só disca, do jeito que uma conexão de controle sai — sem
        // `passar_a_atender` nenhum ainda.
        let ponta = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let controle = ligar(
            &ponta,
            &[endereco_do_servidor],
            impressao_do_servidor,
            Some(&identidade_da_ponta),
            std::time::Duration::from_secs(5),
        )
        .await
        .expect("a conexão de controle não fechou");
        let controle_do_lado_do_servidor = atendendo_o_controle
            .await
            .expect("a tarefa do servidor de teste morreu")
            .expect("o servidor de teste não aceitou a conexão de controle");

        // Um eco simples do lado do servidor: aceita um fluxo, devolve o que
        // recebeu. Só o suficiente para provar, depois de tudo que acontece a
        // seguir, que os bytes ainda atravessam nos dois sentidos.
        //
        // A tarefa recebe um **clone** da conexão, e não o `ParLigado`
        // inteiro: soltar a última alça de uma `quinn::Connection` fecha a
        // ligação, e a tarefa termina (e larga a dela) assim que ecoa —
        // antes de o lado do cliente terminar de ler. `controle_do_lado_do_servidor`
        // continua viva no escopo do teste até o fim, e é essa cópia que
        // mantém a conexão de pé.
        let conexao_para_o_eco = controle_do_lado_do_servidor.conexao.clone();
        let eco = tokio::spawn(async move {
            let (mut envio, mut recebe) = conexao_para_o_eco
                .accept_bi()
                .await
                .expect("o servidor de teste não recebeu o fluxo de prova");
            let mut buf = [0_u8; 10];
            recebe
                .read_exact(&mut buf)
                .await
                .expect("a leitura da prova falhou");
            envio
                .write_all(&buf)
                .await
                .expect("a escrita da prova falhou");
            let _ = envio.finish();
        });

        // Agora, na MESMA `ponta` — nem escuta nova, nem porta nova —, ela
        // também passa a atender um par, exatamente como `Motor::servir_par`
        // faz.
        let identidade_para_o_par = identidade_efemera().unwrap();
        let impressao_de_quem_atende = impressao(&identidade_para_o_par);
        let identidade_do_terceiro = identidade_efemera().unwrap();
        let impressao_do_terceiro = impressao(&identidade_do_terceiro);
        passar_a_atender(&ponta, identidade_para_o_par, impressao_do_terceiro).unwrap();
        let onde_a_ponta_atende = ponta.local_addr().unwrap();
        let atendendo_o_par = tokio::spawn(atender(ponta.clone(), PRAZO_DE_CONFIRMACAO_NO_TESTE));

        // Um terceiro par — quem `SirvaTelaPara` mandaria discar para
        // `ponta` — liga para ela.
        let terceiro = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let ligado_pelo_terceiro = ligar(
            &terceiro,
            &[onde_a_ponta_atende],
            impressao_de_quem_atende,
            Some(&identidade_do_terceiro),
            std::time::Duration::from_secs(5),
        )
        .await
        .expect("o terceiro par não conseguiu ligar para a ponta que também atende");
        // Guardado, e não descartado: soltar o `ParLigado` deste lado
        // fecharia a conexão imediatamente, e a asserção abaixo veria uma
        // ligação morta por causa do próprio teste, não por
        // `passar_a_atender`.
        let _atendido_do_lado_da_ponta = atendendo_o_par
            .await
            .expect("a tarefa que atende o par morreu")
            .expect("a ponta não aceitou o par que discou para ela");
        assert!(
            ligado_pelo_terceiro.conexao.close_reason().is_none(),
            "a ligação com o terceiro par já morreu"
        );

        // **A prova que importa:** a conexão de controle, aberta antes de
        // tudo isso, continua viva e trocando bytes — não foi derrubada nem
        // por `passar_a_atender` nem pelo aperto de mão do terceiro par.
        let (mut envio, mut recebe) = controle
            .conexao
            .open_bi()
            .await
            .expect("a conexão de controle não abre mais fluxo nenhum");
        envio
            .write_all(b"ainda viva")
            .await
            .expect("a escrita na conexão de controle falhou");
        let _ = envio.finish();
        let mut resposta = [0_u8; 10];
        recebe
            .read_exact(&mut resposta)
            .await
            .expect("a leitura na conexão de controle falhou");
        assert_eq!(&resposta, b"ainda viva");
        eco.await.expect("a tarefa de eco do servidor morreu");
    }
}
