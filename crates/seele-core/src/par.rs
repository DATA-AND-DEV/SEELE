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
    /// Fechou aperto de mão do lado de quem disca, e quem atendeu recusou.
    ///
    /// **Não é o mesmo que [`Self::NaoAlcancou`].** Alguém respondeu, e a
    /// discagem chegou a concluir seu lado do TLS — o 1.3 considera isso
    /// pronto assim que processa o `Finished` do par, antes de saber se o par
    /// vai aceitar a contrapartida (o certificado de cliente que
    /// [`ConfereQuemChega`] pode exigir do lado de quem atende). A recusa
    /// chega depois, como fechamento desta conexão.
    #[error("a ligação fechou logo depois de conectar: {0}")]
    RecusadoDepoisDeLigar(String),
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
/// que *chega* nunca passaria pelo [`ConfereQuemChega`], porque os dois lados
/// discam e qualquer um pode acabar sendo quem aceita.
///
/// **Também registra a identidade para quando esta mesma ponta disca.** É a
/// mesma ponta, a mesma porta — de propósito, para reaproveitar o mapeamento
/// de NAT já vivo — e o par do outro lado também pode exigir certificado de
/// cliente. Sem isto, [`ligar`] não teria de onde tirar qual identidade
/// apresentar quando é esta ponta quem disca.
///
/// # Errors
///
/// Falha se o `rustls` recusar o certificado ou a chave.
pub fn passar_a_atender(
    ponta: &quinn::Endpoint,
    identidade: Identidade,
    impressao_de_quem_vem: String,
) -> Result<(), ErroDePar> {
    registrar_identidade_para_discar(ponta, &identidade);
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

/// Onde cada ponta que aprendeu a atender guarda a identidade que ela mesma
/// apresenta quando, pela mesma porta, também disca.
///
/// **Por que existe.** No A1, quem empresta a subida reaproveita a mesma
/// ponta e a mesma porta para discar e para atender — é o mapeamento de NAT
/// já vivo que não se perde, provado pelos testes deste módulo. Do lado de
/// quem disca, agora o par pode exigir certificado de cliente
/// ([`ConfereQuemChega`], mandatório desde esta tarefa) — e [`ligar`] precisa
/// de uma identidade para apresentar, que só existe porque
/// [`passar_a_atender`] já foi chamado nesta mesma ponta.
///
/// A chave é o endereço local da ponta: a API pública do `quinn::Endpoint`
/// não devolve nenhum identificador mais estável, e um endereço UDP local só
/// pertence a uma ponta viva por vez.
type ChaveDeIdentidade = std::net::SocketAddr;
type IdentidadeGuardada = (Vec<CertificateDer<'static>>, PrivateKeyDer<'static>);
static IDENTIDADE_PARA_DISCAR: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<ChaveDeIdentidade, IdentidadeGuardada>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// Guarda uma cópia da identidade desta ponta para [`ligar`] usar depois.
fn registrar_identidade_para_discar(ponta: &quinn::Endpoint, identidade: &Identidade) {
    let Ok(endereco) = ponta.local_addr() else {
        return;
    };
    let copia = (identidade.cadeia.clone(), identidade.chave.clone_key());
    if let Ok(mut tabela) = IDENTIDADE_PARA_DISCAR.lock() {
        tabela.insert(endereco, copia);
    }
}

/// A identidade que esta ponta apresenta ao discar, se ela já atende.
fn identidade_para_discar(ponta: &quinn::Endpoint) -> Option<IdentidadeGuardada> {
    let endereco = ponta.local_addr().ok()?;
    let tabela = IDENTIDADE_PARA_DISCAR.lock().ok()?;
    let (cadeia, chave) = tabela.get(&endereco)?;
    Some((cadeia.clone(), chave.clone_key()))
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
/// cliente, se ela já atende (ver [`identidade_para_discar`]). Sem ela — uma
/// ponta que nunca chamou [`passar_a_atender`] — a discagem sai sem
/// certificado nenhum, e só serve contra um par cujo [`ConfereQuemChega`]
/// ainda não seja mandatório.
///
/// # Errors
///
/// Falha se o `rustls` recusar a configuração.
pub(crate) fn config_de_cliente(
    esperada: String,
    identidade_propria: Option<IdentidadeGuardada>,
) -> Result<(quinn::ClientConfig, Relato), ErroDePar> {
    let confere = ConfereImpressao::nova(esperada);
    let relato = std::sync::Arc::clone(&confere.relato);
    let sem_certificado = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(std::sync::Arc::new(confere));
    let mut tls = match identidade_propria {
        Some((cadeia, chave)) => sem_certificado
            .with_client_auth_cert(cadeia, chave)
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
    prazo: std::time::Duration,
) -> Result<ParLigado, ErroDePar> {
    let identidade_propria = identidade_para_discar(ponta);
    let mut tentativas = tokio::task::JoinSet::new();
    for endereco in enderecos {
        // `PrivateKeyDer` não implementa `Clone` — só `clone_key()` — então
        // cada tentativa pede a sua própria cópia em vez de compartilhar uma.
        let copia = identidade_propria
            .as_ref()
            .map(|(cadeia, chave)| (cadeia.clone(), chave.clone_key()));
        let (config, relato) = config_de_cliente(impressao_esperada.clone(), copia)?;
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
                // **O aperto de mão do lado de quem disca conclui antes de
                // saber se o par vai aceitar a contrapartida.** O TLS 1.3
                // considera a sessão pronta assim que processa o `Finished`
                // do par — antes de enviar, e muito antes de o par validar, o
                // certificado de cliente que o [`ConfereQuemChega`] dele pode
                // exigir. Uma recusa por causa disso chega como fechamento
                // desta conexão, não como erro deste `.await`; sem checar
                // aqui, `ligar` daria uma ligação recusada por boa.
                if let Some(motivo) = recusada_logo_apos_ligar(&conexao, endereco).await {
                    return Err(motivo);
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
/// `None` quando a ponta fechou sem ninguém chegar, ou quando quem chegou não
/// completou o aperto de mão — inclusive por [`ConfereQuemChega`] ter recusado
/// o certificado apresentado.
pub async fn atender(ponta: quinn::Endpoint) -> Option<ParLigado> {
    let chegando = ponta.accept().await?;
    let conexao = match chegando.await {
        Ok(conexao) => conexao,
        Err(erro) => {
            // O rastro diz de quem se trata: é a regra do `CLAUDE.md` deste
            // repositório, e a Task 4 já pagou por ignorá-la uma vez.
            tracing::warn!(%erro, "uma ligação que chegou não fechou o aperto de mão");
            return None;
        }
    };
    let ida_e_volta = conexao.rtt();
    let como = como_chegou(conexao.remote_address());
    tracing::info!(par = %conexao.remote_address(), ?como, ?ida_e_volta, "um par foi atendido");
    Some(ParLigado {
        conexao,
        como,
        ida_e_volta,
    })
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

/// Dá ao par uma folga curta para fechar a conexão antes de dar por boa.
///
/// **Por que existe.** No TLS 1.3, quem disca conclui seu lado do aperto de
/// mão assim que processa o `Finished` do par — antes de o par validar a
/// contrapartida que ele próprio pediu (o certificado de cliente que
/// [`ConfereQuemChega`] pode exigir). Uma recusa por causa disso chega como
/// fechamento assíncrono desta conexão, não como erro do `.await` que a abriu.
///
/// A folga é o dobro do ida-e-volta que o próprio aperto de mão já mediu —
/// tempo de sobra para uma recusa que o par manda assim que processa a
/// resposta, sem impor uma espera fixa às ligações que vão dar certo.
async fn recusada_logo_apos_ligar(
    conexao: &quinn::Connection,
    endereco: std::net::SocketAddr,
) -> Option<ErroDePar> {
    if let Some(motivo) = conexao.close_reason() {
        return Some(traduzir_recusa(endereco, &motivo));
    }
    let folga = conexao
        .rtt()
        .saturating_mul(2)
        .max(std::time::Duration::from_millis(20));
    tokio::select! {
        motivo = conexao.closed() => Some(traduzir_recusa(endereco, &motivo)),
        () = tokio::time::sleep(folga) => None,
    }
}

/// Registra e traduz o fechamento que [`recusada_logo_apos_ligar`] observou.
fn traduzir_recusa(endereco: std::net::SocketAddr, motivo: &quinn::ConnectionError) -> ErroDePar {
    tracing::warn!(
        par = %endereco,
        %motivo,
        "a ligação fechou logo depois de conectar: o par recusou depois do aperto de mão"
    );
    ErroDePar::RecusadoDepoisDeLigar(motivo.to_string())
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
        let _atendendo_a = tokio::spawn(atender(a));

        passar_a_atender(&b, ib, impressao_a.clone()).unwrap();

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
        passar_a_atender(
            &a,
            identidade_efemera().unwrap(),
            impressao(&identidade_efemera().unwrap()),
        )
        .unwrap();
        let endereco_a = a.local_addr().unwrap();
        let _atendendo_a = tokio::spawn(atender(a));

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
        let _atendendo_impostor = tokio::spawn(atender(impostor));

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
            atender(legitimo).await
        });

        passar_a_atender(&b, ib, esperada.clone()).unwrap();
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
        let atendendo = tokio::spawn(atender(anfitriao.clone()));

        let ponta_do_intruso = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        passar_a_atender(&ponta_do_intruso, intruso, impressao_do_anfitriao.clone()).unwrap();
        let tentou = ligar(
            &ponta_do_intruso,
            &[onde],
            impressao_do_anfitriao,
            std::time::Duration::from_secs(3),
        )
        .await;

        assert!(
            tentou.is_err(),
            "o intruso apresentou um certificado que o anfitrião nunca esperou, e entrou"
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

        passar_a_atender(&a, ia, impressao_b.clone()).unwrap();
        passar_a_atender(&b, ib, impressao_a.clone()).unwrap();
        let onde_a = a.local_addr().unwrap();
        let atendendo = tokio::spawn(atender(a.clone()));

        let ligado = ligar(
            &b,
            &[onde_a],
            impressao_a,
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
        let atendendo = tokio::spawn(atender(anfitriao));

        let sem_identidade = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let tentou = ligar(
            &sem_identidade,
            &[onde],
            impressao_do_anfitriao,
            std::time::Duration::from_secs(3),
        )
        .await;

        assert!(
            tentou.is_err(),
            "quem nunca apresentou certificado nenhum entrou mesmo assim"
        );
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(200), atendendo)
                .await
                .map(|ligado| ligado.ok().flatten().is_none())
                .unwrap_or(true),
            "o anfitrião deu por boa uma ligação sem certificado de cliente nenhum"
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
