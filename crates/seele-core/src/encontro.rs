//! Degrau 4 do ADR 0022, do lado de quem entra.
//!
//! O trabalho pesado é de quem hospeda — ver `alcance::encontro`, no
//! `seele-server`. Deste lado o degrau 4 é **um datagrama**: "ponto de encontro,
//! diga ao anfitrião de onde este pacote veio". O anfitrião recebe aquele
//! endereço, manda alguns pacotes para cá, e o roteador de lá passa a deixar
//! entrar o aperto de mão que vem em seguida.
//!
//! # Por que o socket importa mais que o datagrama
//!
//! O NAT mapeia por porta interna. Se este aviso sair de um socket e o QUIC sair
//! de outro, o anfitrião fura o caminho para a porta errada e o aperto de mão
//! continua batendo numa porta fechada — em quase todo roteador doméstico, que
//! filtra por endereço **e** porta.
//!
//! É por isso que `Batida` guarda o socket por onde bateu, em vez de só mandar
//! o pacote: quem conecta em seguida tem de conectar por ele. É o mesmo motivo
//! pelo qual o outro lado precisou de um espelho do socket do servidor.
//!
//! # O que este lado lê do ponto de encontro, e o que não lê
//!
//! **A batida não lê resposta nenhuma.** O `LEVE` é de mão única, e quem entra
//! não espera resposta a ele.
//!
//! **A consulta ao quarto lê**, e é o único caminho por onde um ponto de
//! encontro põe um endereço na lista de candidatos ([`onde_mora_hoje`]). Um
//! ponto hostil, ou quem ocupou a marca no quarto, consegue mandar quem chega
//! para o endereço errado. O que impede isso de virar conexão com um impostor é
//! a impressão digital, conferida dentro do TLS antes de qualquer `Hello`, e ela
//! só protege quando existe: a esperada vem do link desta sessão ou, na volta
//! pela lista, da impressão que a lista guardou (análise de 22/09/2026, §3.1,
//! S3). Vale também num endereço que esta máquina já fixou com a chave de outro
//! servidor: o endereço do quarto, quando entra na corrida, é um candidato que
//! a pessoa não escolheu, e ali o pino que confere não passa por cima da
//! esperada (o adendo de 2026-09-29 ao ADR 0003, `crate::tofu::TofuVerifier`).
//!
//! A mesma resposta traz a escuta de avisos, que vira o destino do `LEVE`, e
//! essa não tem TLS atrás: o `LEVE` sai antes do aperto de mão, com o endereço
//! de quem chega. Ela só vale no IP do servidor que a mesma resposta deu
//! ([`OndeMora::escuta_do_anfitriao`], [`bilhete_desta_volta`]).

use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use seele_proto::encontro::{self};
use seele_proto::uri::Bilhete;

// `Marca` e `Marcas` reexportadas: quem chama `onde_mora_hoje` de fora precisa
// das marcas prontas, e fazê-las vir daqui poupa a camada de cima de nomear
// `seele-proto`, que o ADR 0002 não deixa a casca alcançar de qualquer jeito.
pub use seele_proto::encontro::{Marca, Marcas, PONTO_PADRAO, VARIAVEL};

/// O ponto de encontro de quem liga por código: o que `$SEELE_ENCONTRO` diz,
/// e o [`PONTO_PADRAO`] sem ela. `None` quando a variável o desliga: sem
/// ponto, código não funciona, e link com endereço continua funcionando.
///
/// **Só a produção chama isto**, como o `com_o_ponto_padrao` de quem hospeda
/// (a decisão (d) do lote G-servidor da F5): os testes dão o ponto deles a
/// [`localizar`]. A leitura do ambiente é a única, a de
/// `seele_proto::encontro::ponto_escolhido`. Um código não carrega ponto
/// nenhum, e com `$SEELE_ENCONTRO` as duas pontas precisam do mesmo.
#[must_use]
pub fn ponto_de_quem_liga() -> Option<String> {
    encontro::ponto_escolhido(std::env::var(VARIAVEL).ok().as_deref(), Some(PONTO_PADRAO))
}

// A feature do guarda nunca entra num build otimizado: com ela, perguntar ao
// ponto padrão entra em pânico, e é o que o produto faz ao ligar por código.
// Ver o gêmeo dela no `seele-server`.
#[cfg(all(feature = "recusar-encontro-de-producao", not(debug_assertions)))]
compile_error!(
    "a feature `recusar-encontro-de-producao` do seele-core é só de teste e não pode ir num \
     build otimizado: com ela, ligar por código pelo ponto de encontro padrão entra em pânico"
);

/// O gêmeo do guarda da F5, do lado de quem liga: nenhum teste pergunta à VPS
/// de produção, nem pelo quarto ([`localizar`], [`onde_mora_hoje`]) nem pela
/// batida (o `LEVE`).
///
/// A regra é a mesma do guarda de quem hospeda,
/// `seele_proto::encontro::aponta_para_a_producao`: o [`PONTO_PADRAO`] escrito
/// como for, ou um IP da rede dele. Age antes do DNS, nos dois lugares em que
/// este módulo resolve um ponto. Ligado nos testes deste crate (`cfg(test)`) e
/// pela feature `recusar-encontro-de-producao`, que só a `[dev-dependencies]`
/// do `seele-conformance` pede; fora disso, não faz nada.
///
/// `onde` vai na frase do pânico e diz qual dos dois lugares pegou o teste.
#[cfg(any(test, feature = "recusar-encontro-de-producao"))]
#[track_caller]
fn recusar_a_producao(ponto: &str, onde: &str) {
    assert!(
        !encontro::aponta_para_a_producao(ponto),
        "um teste ia perguntar ao ponto de encontro de produção («{ponto}», pego em {onde}): \
         cada rodada mandaria pacotes de teste à VPS de produção, e o teste passaria a depender \
         da rede de fora. Dê a ele um ponto local"
    );
}

/// Fora dos testes, nada: ver a versão ligada logo acima.
#[cfg(not(any(test, feature = "recusar-encontro-de-producao")))]
fn recusar_a_producao(_ponto: &str, _onde: &str) {}

/// Quanto tempo se gasta resolvendo o ponto antes de desistir e conectar
/// assim mesmo: um ponto fora do ar não atrasa uma conexão que talvez nem
/// precisasse dele.
const PRAZO: Duration = Duration::from_millis(600);

/// Quanto se espera a resposta do ponto a um `CHAMA`.
///
/// O ponto espera o `ATENDE` do anfitrião por 3 s (`enc3.md`) e responde
/// `CALADO` depois disso; um segundo a mais cobre as duas pernas até ele.
pub const PRAZO_DA_CHAMADA: Duration = Duration::from_secs(4);

/// De quanto em quanto o `CHAMA` se repete enquanto o ponto não responde.
///
/// UDP perde pacote, e um `CHAMA` perdido custava a chamada inteira em
/// silêncio. Cada repetição vira um `CHAMADO` novo e um furo novo do anfitrião,
/// que tem teto próprio de furos por minuto (R2): três envios cabem nele.
const INTERVALO_DA_CHAMADA: Duration = Duration::from_millis(800);

/// Quantos `CHAMA` saem no máximo.
const CHAMADAS: u32 = 3;

/// O degrau 4 da 0.17, do lado de quem liga: o `CHAMA` do ENC/3 (D7).
///
/// Substitui a `Batida` do ENC/1 e do ENC/2. Lá o aviso era de mão única: o
/// `LEVE` saía, o anfitrião furava, e quem ligava discava às cegas. Aqui é uma
/// pergunta com resposta: o `CHAMA` leva a credencial ICE (o `U`) desta
/// tentativa, o ponto o repassa à escuta do anfitrião, o anfitrião fura com
/// Binding Indications **do socket do servidor** para o endereço de quem liga
/// (o que o ponto viu) e responde `ATENDE` com o endereço público dele. O
/// ponto devolve `AQUI` com esse endereço e as dicas de rede local, e é para
/// eles que a discagem vai, com o mesmo `U`.
///
/// # A segunda chamada, com o IPv6 desta máquina
///
/// O ponto junta ao `CHAMADO` o endereço que ele viu, numa família só: a do
/// caminho até ele, que [`escolher_ponto`] faz ser a do anfitrião, quase sempre
/// IPv4. O anfitrião fura esse endereço, e o IPv6 dos dois nunca é tentado: o
/// roteador dele não tem NAT em IPv6, mas tem firewall, que só deixa entrar o
/// que responde a algo que saiu (pendência 26, medido em 24/08/2026). Quando o
/// ponto diz que o anfitrião atendeu e o IPv4 não abre (duas casas atrás de
/// CGNAT), quem liga **chama de novo** ([`Chamada::chamar_pelo_ipv6`]): um
/// `CHAMA` com um `U` novo e os candidatos IPv6 globais desta máquina
/// ([`candidatos_ipv6`]). O anfitrião fura e checa também por IPv6
/// (`Ponta::chamado`, ICE completo), e esse pacote de saída é o que abre o
/// firewall dele para quem chama.
///
/// **Depois, e não junto.** O candidato podia ir no primeiro `CHAMA`, e não
/// vai, por duas razões:
///
/// - O ponto escreve os candidatos de quem chama **antes** do endereço que ele
///   viu, e o anfitrião toma o primeiro como a origem do par
///   (`Motor::chamado`; a 0.19.6 faz o mesmo, e não muda). É por essa origem
///   que um aperto novo da mesma porta aposenta o par pendente, em vez de
///   disputar o DTLS com ele. Com o IPv6 na frente, o par do `CHAMADO` deixaria
///   de ter a origem de onde a discagem direta também bate, e o caminho IPv4,
///   que é o de quase todo mundo e funciona, mudaria de comportamento.
/// - O IPv6 desta máquina é metadado. Quem entra pelo IPv4 não precisa
///   contá-lo a ninguém, e assim não conta.
///
/// O primeiro `CHAMA` é o de sempre, byte a byte, e a discagem que o segue
/// também. A segunda chamada só existe depois de essa discagem dar em
/// silêncio, que é quando o produto de antes desistia (`NatNaoAbriu`).
///
/// # Por que o socket importa
///
/// O NAT mapeia por porta interna. O `CHAMA` sai **da ponta da sessão**
/// ([`crate::transporte::PontaDaSessao`]), a mesma que vai discar: é o endereço
/// dessa porta que o ponto vê e que o anfitrião fura. Um `CHAMA` de outro
/// socket abriria o caminho para a porta errada.
///
/// # O que ela não autentica
///
/// O ponto pode mentir no `AQUI`. O que impede isso de virar conexão com um
/// impostor é a impressão do DTLS, conferida antes do `Hello`
/// (`client::conferir_quem_atendeu`). O pior que um ponto hostil faz é não
/// entregar.
#[derive(Clone)]
pub(crate) struct Chamada {
    /// O ponto, já resolvido.
    ponto: SocketAddr,
    /// O texto do ponto, para o log.
    ponto_escrito: String,
    /// As marcas do anfitrião: o servidor e a escuta.
    marcas: Marcas,
}

impl std::fmt::Debug for Chamada {
    /// Sem as marcas: numa sala, elas são o código de quem se procura.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Chamada")
            .field("ponto", &self.ponto)
            .finish_non_exhaustive()
    }
}

/// O que o ponto respondeu a um `CHAMA` atendido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Atendida {
    /// O endereço público do servidor, como o anfitrião o disse no `ATENDE`.
    pub(crate) endereco: SocketAddr,
    /// As dicas de rede local do anfitrião.
    pub(crate) dicas: Vec<SocketAddr>,
    /// O `U` que foi no `CHAMA`, e com que a discagem tem de sair: é por ele
    /// que o anfitrião reconhece quem furou.
    pub(crate) credencial: String,
}

/// Por que um `CHAMA` não deu em endereço.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FalhaDaChamada {
    /// O ponto disse `NINGUEM`: o anfitrião não está registrado lá (fora do ar,
    /// ou uma versão que não registra o ENC/3).
    Ninguem,
    /// O ponto disse `CALADO`: achou o anfitrião e ele não respondeu ao
    /// `CHAMADO` a tempo.
    Calado,
    /// O ponto recusou (taxa, relógio, cheio).
    Recusa,
    /// O ponto não respondeu nada no prazo, ou o `CHAMA` nem saiu.
    Mudo,
}

impl Chamada {
    /// Resolve o ponto. **Não manda nada.**
    ///
    /// `None` quando não dá para chamar: nome que não resolve no prazo, ou
    /// destino sem marcas. Nesse caso quem liga disca como sempre discou:
    /// nenhum endereço do convite depende disto, e o degrau 4 é o único que se
    /// perde.
    pub(crate) async fn preparar(bilhete: &Bilhete, marcas: Option<Marcas>) -> Option<Self> {
        let marcas = marcas?;
        let ponto = tokio::time::timeout(PRAZO, resolver(bilhete))
            .await
            .ok()??;
        Some(Self {
            ponto,
            ponto_escrito: bilhete.ponto.clone(),
            marcas,
        })
    }

    /// Onde o `CHAMA` vai, para o log de quem chama.
    pub(crate) fn ponto(&self) -> SocketAddr {
        self.ponto
    }

    /// Chama o anfitrião pelo ponto, da ponta desta sessão, e espera o `AQUI`.
    ///
    /// Repete o `CHAMA` a cada [`INTERVALO_DA_CHAMADA`], até [`CHAMADAS`]
    /// vezes, enquanto o ponto não responde, e desiste em `prazo`. Lê as
    /// respostas pelos datagramas ENC da ponta (`Ponta::mensagens_enc`), e os
    /// devolve ao acabar: uma chamada por vez em cada ponta.
    ///
    /// **Sem candidato nenhum**: o ponto junta o endereço que ele viu, o desta
    /// porta depois do NAT, e é ele que tem de ser o primeiro do `CHAMADO`.
    ///
    /// # Errors
    ///
    /// [`FalhaDaChamada`], com o que o ponto disse ou calou. Cada uma vai ao
    /// log em `info`, com o ponto.
    pub(crate) async fn chamar(
        &self,
        ponta: &seele_enlace::Ponta,
        prazo: Duration,
    ) -> Result<Atendida, FalhaDaChamada> {
        self.chamar_com(ponta, prazo, Vec::new()).await
    }

    /// A segunda chamada: de novo pelo ponto, agora dizendo o IPv6 global
    /// desta máquina, para o anfitrião furar também o firewall IPv6 dele.
    ///
    /// Para depois de [`Chamada::chamar`] ter sido atendida e a discagem ao
    /// endereço que ela deu ter dado em silêncio. Um `U` novo: a discagem de
    /// antes acabou, e o par dela no anfitrião com ela.
    ///
    /// `do_anfitriao` são os endereços IPv6 do anfitrião que quem chama já
    /// conhece (os do convite): é para eles que o sistema escolhe a origem que
    /// vira candidato ([`candidatos_ipv6`]). Vazio serve.
    ///
    /// `None` quando esta máquina não tem IPv6 global para dizer (ou a ponta é
    /// só IPv4): **nenhum pacote sai**, e nada muda para quem não tem IPv6.
    pub(crate) async fn chamar_pelo_ipv6(
        &self,
        ponta: &seele_enlace::Ponta,
        prazo: Duration,
        do_anfitriao: &[SocketAddr],
    ) -> Option<Result<Atendida, FalhaDaChamada>> {
        let candidatos = self.candidatos_proprios(ponta.local_addr(), do_anfitriao);
        if candidatos.is_empty() {
            return None;
        }
        // Quantos, e nunca quais: o candidato é o endereço IPv6 desta máquina,
        // e ele não vai ao `seele.log`.
        tracing::info!(
            ponto = %self.ponto_escrito,
            candidatos_ipv6 = candidatos.len(),
            "degrau 4: o anfitrião atendeu e o caminho que o ponto viu não abriu; chamando de \
             novo, agora com o IPv6 desta máquina, para ele furar também por IPv6"
        );
        Some(self.chamar_com(ponta, prazo, candidatos).await)
    }

    /// O `CHAMA` e a espera pelo `AQUI`, com os candidatos dados.
    async fn chamar_com(
        &self,
        ponta: &seele_enlace::Ponta,
        prazo: Duration,
        candidatos: Vec<SocketAddr>,
    ) -> Result<Atendida, FalhaDaChamada> {
        let Some(mut caixa) = CaixaDoPonto::tomar(ponta) else {
            tracing::warn!(
                ponto = %self.ponto_escrito,
                "os datagramas do ponto desta ponta já foram tomados: o CHAMA não sai"
            );
            return Err(FalhaDaChamada::Mudo);
        };
        // O que já estava na fila não é resposta a este `CHAMA`: o `AQUI` não
        // diz a que chamada responde, e o de uma chamada anterior (o ponto
        // responde a cada repetição) passaria por resposta a esta.
        caixa.esvaziar();
        let credencial = seele_enlace::sortear_credencial();
        let Some(u) = encontro::Credencial::nova(&credencial) else {
            tracing::error!("a credencial sorteada pelo enlace não serve ao ENC/3");
            return Err(FalhaDaChamada::Mudo);
        };
        let pedido = encontro::chama(&encontro::Chama {
            servidor: self.marcas.servidor().clone(),
            escuta: self.marcas.escuta().clone(),
            credencial: u,
            candidatos,
        });
        let ate = tokio::time::Instant::now() + prazo;
        let mut enviados = 0_u32;
        let mut proximo = tokio::time::Instant::now();
        loop {
            if enviados < CHAMADAS && tokio::time::Instant::now() >= proximo {
                match ponta.enviar_bruto(self.ponto, &pedido) {
                    Ok(()) => {
                        enviados += 1;
                        tracing::info!(
                            ponto = %self.ponto_escrito,
                            vez = enviados,
                            "degrau 4: chamamos o anfitrião pelo ponto de encontro (ENC/3)"
                        );
                    }
                    Err(erro) => {
                        tracing::info!(%erro, ponto = %self.ponto_escrito, "o CHAMA não saiu");
                        if enviados == 0 {
                            return Err(FalhaDaChamada::Mudo);
                        }
                    }
                }
                proximo = tokio::time::Instant::now() + INTERVALO_DA_CHAMADA;
            }
            let espera = if enviados < CHAMADAS {
                proximo.min(ate)
            } else {
                ate
            };
            let chegou = match tokio::time::timeout_at(espera, caixa.receber()).await {
                Ok(Some(chegou)) => chegou,
                Ok(None) => return Err(FalhaDaChamada::Mudo),
                Err(_) if tokio::time::Instant::now() >= ate => {
                    tracing::info!(
                        ponto = %self.ponto_escrito,
                        enviados,
                        ?prazo,
                        "o ponto de encontro não respondeu ao CHAMA no prazo"
                    );
                    return Err(FalhaDaChamada::Mudo);
                }
                Err(_) => continue,
            };
            let (de, bytes) = chegou;
            // Só o que veio do ponto: um datagrama de outro endereço que chegue
            // a esta porta não fala por ele.
            if de.ip().to_canonical() != self.ponto.ip().to_canonical()
                || de.port() != self.ponto.port()
            {
                continue;
            }
            let Some(resposta) = encontro::ler_resposta3(&bytes) else {
                continue;
            };
            match resposta {
                encontro::Resposta3::Aqui {
                    marca,
                    endereco,
                    dicas,
                } if marca == *self.marcas.servidor() => {
                    tracing::info!(
                        ponto = %self.ponto_escrito,
                        %endereco,
                        dicas = dicas.len(),
                        "o anfitrião atendeu o CHAMA: discando para o endereço que ele deu"
                    );
                    return Ok(Atendida {
                        endereco,
                        dicas,
                        credencial,
                    });
                }
                encontro::Resposta3::Ninguem { marca } if marca == *self.marcas.servidor() => {
                    tracing::info!(ponto = %self.ponto_escrito, "o ponto disse NINGUEM ao CHAMA");
                    return Err(FalhaDaChamada::Ninguem);
                }
                encontro::Resposta3::Calado { marca } if marca == *self.marcas.servidor() => {
                    tracing::info!(
                        ponto = %self.ponto_escrito,
                        "o ponto achou o anfitrião e ele não atendeu o CHAMA (CALADO)"
                    );
                    return Err(FalhaDaChamada::Calado);
                }
                encontro::Resposta3::Recusa { marca, motivo, .. }
                    if marca == *self.marcas.servidor() =>
                {
                    tracing::info!(
                        ponto = %self.ponto_escrito,
                        motivo = motivo.texto(),
                        "o ponto recusou o CHAMA"
                    );
                    return Err(FalhaDaChamada::Recusa);
                }
                _ => {}
            }
        }
    }

    /// Os candidatos que a segunda chamada leva: os IPv6 globais desta máquina
    /// ([`candidatos_ipv6`]), na porta de `local`, que é a ponta da sessão.
    fn candidatos_proprios(
        &self,
        local: SocketAddr,
        do_anfitriao: &[SocketAddr],
    ) -> Vec<SocketAddr> {
        // Nos testes, um ponto pode ter os candidatos ditos: ver a costura.
        #[cfg(any(test, feature = "costura-de-teste"))]
        if let Some(ditos) = costura_de_teste::ditos_para(self.ponto) {
            return caber_no_chama(ditos);
        }
        candidatos_ipv6(local, do_anfitriao, origem_ipv6_para)
    }
}

/// Os datagramas do ponto que chegam a uma ponta (`Ponta::mensagens_enc`),
/// tomados por uma chamada e **devolvidos ao cair**.
///
/// A ponta os entrega a um leitor só. A chamada os toma enquanto espera o
/// `AQUI` e os devolve por qualquer saída, para a segunda chamada da mesma
/// sessão ([`Chamada::chamar_pelo_ipv6`]) poder tomá-los de novo.
struct CaixaDoPonto<'a> {
    ponta: &'a seele_enlace::Ponta,
    fila: Option<tokio::sync::mpsc::Receiver<(SocketAddr, Vec<u8>)>>,
}

impl<'a> CaixaDoPonto<'a> {
    /// `None` quando outro leitor está com eles.
    fn tomar(ponta: &'a seele_enlace::Ponta) -> Option<Self> {
        let fila = ponta.mensagens_enc()?;
        Some(Self {
            ponta,
            fila: Some(fila),
        })
    }

    /// Larga o que já estava na fila.
    fn esvaziar(&mut self) {
        if let Some(fila) = self.fila.as_mut() {
            while fila.try_recv().is_ok() {}
        }
    }

    /// O próximo datagrama; `None` quando a ponta fechou.
    async fn receber(&mut self) -> Option<(SocketAddr, Vec<u8>)> {
        self.fila.as_mut()?.recv().await
    }
}

impl Drop for CaixaDoPonto<'_> {
    fn drop(&mut self) {
        if let Some(fila) = self.fila.take() {
            self.ponta.devolver_mensagens_enc(fila);
        }
    }
}

/// Para onde a sonda de origem aponta quando o convite não trouxe IPv6 do
/// anfitrião: `2001:db8::/32`, a faixa de documentação (RFC 3849), como o
/// `endereco_de_saida_v6` de quem hospeda. Nada é mandado a ela.
fn alvo_da_sonda() -> SocketAddr {
    SocketAddr::from((Ipv6Addr::new(0x2001, 0x0db8, 0, 0, 0, 0, 0, 1), 9))
}

/// Quantos endereços IPv6 do anfitrião viram sonda de origem. Um convite
/// leva poucos; o teto só impede uma lista comprida de abrir um socket por
/// linha.
const SONDAS_DE_ORIGEM: usize = 4;

/// Se este IPv6 é **global**: um endereço que a internet roteia até esta
/// máquina, e o único tipo que vai como candidato num `CHAMA`.
///
/// Só o que está em `2000::/3`, o espaço unicast global que a IANA distribui.
/// Ficam de fora, por estarem fora dele: o loopback (`::1`), o «qualquer»
/// (`::`), o link-local (`fe80::/10`), o local único (`fc00::/7`, o `192.168`
/// do IPv6), o multicast (`ff00::/8`), o IPv4 mapeado (`::ffff:0:0/96`) e o
/// NAT64 (`64:ff9b::/96`). E, de dentro dele, as faixas que não são o
/// endereço de uma máquina na internet:
///
/// - `2001::/23`, os usos especiais da IANA (o Teredo em `2001::/32`, o ORCHID,
///   a faixa de medição);
/// - `2001:db8::/32` e `3fff::/20`, documentação (RFC 3849 e RFC 9637);
/// - `2002::/16`, o 6to4, um túnel aposentado (RFC 7526) cujo caminho de volta
///   depende de um relay de terceiro.
///
/// Mais estreito que o `alcancavel` do ponto de propósito: aquele decide para
/// onde o ponto aceita apontar, e este decide o que esta máquina conta de si.
#[must_use]
pub(crate) fn e_ipv6_global(ip: Ipv6Addr) -> bool {
    let [primeiro, segundo, ..] = ip.segments();
    let unicast_global = primeiro & 0xe000 == 0x2000;
    let uso_especial = primeiro == 0x2001 && segundo < 0x0200;
    let documentacao =
        (primeiro == 0x2001 && segundo == 0x0db8) || (primeiro == 0x3fff && segundo < 0x1000);
    let seis_para_quatro = primeiro == 0x2002;
    unicast_global && !uso_especial && !documentacao && !seis_para_quatro
}

/// A origem IPv6 que o sistema usaria para falar com `alvo`, **sem mandar
/// pacote**: conectar um socket UDP escolhe a rota e associa o endereço local,
/// e é só isso (o truque de `alcance::endereco_de_saida_v6`, de quem hospeda).
///
/// Um socket novo, e não o da sessão: `connect` no socket da sessão o prenderia
/// a um destino só. O endereço que sai daqui vale para ele porque a ponta de
/// quem liga é `[::]`, e quem escolhe a origem de cada pacote dela é esta mesma
/// regra do sistema (RFC 6724).
fn origem_ipv6_para(alvo: SocketAddr) -> Option<Ipv6Addr> {
    let socket = std::net::UdpSocket::bind(SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0))).ok()?;
    socket.connect(alvo).ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V6(origem) => Some(origem),
        IpAddr::V4(_) => None,
    }
}

/// Os candidatos IPv6 que a segunda chamada leva: os endereços IPv6
/// **globais** desta máquina, na porta da ponta da sessão.
///
/// Por que eles vão, e por que só na segunda chamada: ver [`Chamada`].
///
/// # Quais endereços
///
/// **A origem que o sistema escolhe**, e não a lista das interfaces
/// (`origem`, em produção [`origem_ipv6_para`]): uma para cada IPv6 do
/// anfitrião que o convite trouxe (`do_anfitriao`, até [`SONDAS_DE_ORIGEM`]) e
/// uma para a internet em geral ([`alvo_da_sonda`]). É o endereço de onde os
/// pacotes desta ponta saem de fato, e só ele serve: o firewall do anfitrião
/// abre para o endereço que ele furou, e um pacote vindo de outro endereço
/// desta mesma máquina bate fechado.
///
/// **Com endereços temporários de privacidade (RFC 8981), a origem é o
/// temporário**, e é ele que vai. Não é só o que funciona: é também o que
/// menos conta, porque o temporário troca sozinho e o estável é o que
/// identificaria a máquina de uma semana para outra. O estável nunca é
/// anunciado quando o sistema sai pelo temporário.
///
/// Só os que [`e_ipv6_global`] aceita, sem repetir, no máximo
/// `encontro::LIMITE_DE_CANDIDATOS` ([`caber_no_chama`]). Nenhum quando a ponta
/// é só IPv4 (o recuo de `transporte::abrir_ponta`): ela não receberia a
/// resposta. Uma ponta ligada a um endereço IPv6 só sai por ele.
///
/// # A quem isto conta o endereço
///
/// A quem já recebia o IPv4 público desta máquina, pelo mesmo caminho: ao
/// ponto de encontro do convite, que passa a ler também o IPv6, e ao anfitrião
/// do convite, que o recebe no `CHAMADO` ao lado do IPv4 que o ponto viu. A
/// mais ninguém: não vai às outras pessoas da sala, a outro servidor nem a
/// relato nenhum, e não é escrito no `seele.log` (o log de quem chama diz
/// quantos candidatos foram, e não quais). E só quando o caminho IPv4 não
/// abriu: quem entra por ele não conta o IPv6 a ninguém. **É metadado novo
/// para quem opera o ponto**: até aqui ele só via o IPv4 de quem ligava a um
/// anfitrião registrado por IPv4 (`docs/ponto-de-encontro.md`, «O que ele fica
/// sabendo»).
fn candidatos_ipv6(
    local: SocketAddr,
    do_anfitriao: &[SocketAddr],
    origem: impl Fn(SocketAddr) -> Option<Ipv6Addr>,
) -> Vec<SocketAddr> {
    let SocketAddr::V6(local) = local else {
        return Vec::new();
    };
    let origens: Vec<Ipv6Addr> = if local.ip().is_unspecified() {
        do_anfitriao
            .iter()
            .copied()
            // O IPv4 mapeado (`::ffff:a.b.c.d`) é IPv4 escrito de outro jeito.
            .filter(|alvo| alvo.ip().to_canonical().is_ipv6())
            .take(SONDAS_DE_ORIGEM)
            .chain(std::iter::once(alvo_da_sonda()))
            .filter_map(origem)
            .collect()
    } else {
        vec![*local.ip()]
    };
    caber_no_chama(
        origens
            .into_iter()
            .filter(|origem| e_ipv6_global(*origem))
            .map(|origem| SocketAddr::from((origem, local.port())))
            // A regra do ponto (`enc3::candidatos_do_chamado`): o que ela
            // recusaria não chegaria ao anfitrião.
            .filter(|candidato| encontro::alcancavel(*candidato))
            .collect(),
    )
}

/// O que cabe no campo de candidatos de um `CHAMA`: só os que
/// `encontro::serve_de_candidato` aceita, sem repetir, no máximo
/// `encontro::LIMITE_DE_CANDIDATOS`.
///
/// **A forma é do fio, e errar aqui custa a chamada inteira**: o ponto lê o
/// campo com «um só que não sirva recusa todos», e um `CHAMA` que ele não lê é
/// um `CHAMA` que ele cala. Um candidato com porta baixa, repetido ou a mais
/// transformaria o degrau 4 em silêncio, o IPv4 junto.
fn caber_no_chama(candidatos: Vec<SocketAddr>) -> Vec<SocketAddr> {
    let mut cabem: Vec<SocketAddr> = Vec::new();
    for candidato in candidatos {
        if cabem.len() >= encontro::LIMITE_DE_CANDIDATOS {
            break;
        }
        if encontro::serve_de_candidato(candidato) && !cabem.contains(&candidato) {
            cabem.push(candidato);
        }
    }
    cabem
}

// A feature da costura nunca entra num build otimizado, como a do guarda: com
// ela, um teste escolhe o que o `CHAMA` conta desta máquina.
#[cfg(all(feature = "costura-de-teste", not(debug_assertions)))]
compile_error!(
    "a feature `costura-de-teste` do seele-core é só de teste e não pode ir num build \
     otimizado: com ela, os candidatos do CHAMA podem ser ditos de fora"
);

/// **Só nos testes** (o `cfg(test)` deste crate e a feature `costura-de-teste`,
/// que só a `[dev-dependencies]` do `seele-conformance` liga): os candidatos
/// que o `CHAMA` leva a um ponto dado, no lugar dos desta máquina.
///
/// # Por que existe
///
/// O que [`candidatos_ipv6`] anuncia é um endereço **global**, e numa máquina
/// só não há dois: o caminho IPv6 de ponta a ponta se prova em `[::1]`, com um
/// firewall de mentira no meio, e `[::1]` nunca passa em [`e_ipv6_global`].
/// A costura troca **quais** endereços vão; o resto do caminho (o teto do fio,
/// o `CHAMA`, o ponto, o furo do anfitrião, a discagem) é o do produto.
///
/// # Por que por ponto, e não global
///
/// Os testes de um arquivo rodam juntos num processo, cada um com o ponto
/// dele. Dito por ponto, o candidato de um teste não alcança o `CHAMA` de
/// outro.
#[cfg(any(test, feature = "costura-de-teste"))]
pub mod costura_de_teste {
    use std::collections::BTreeMap;
    use std::net::SocketAddr;
    use std::sync::Mutex;

    static DITOS: Mutex<BTreeMap<SocketAddr, Vec<SocketAddr>>> = Mutex::new(BTreeMap::new());

    /// Enquanto o guarda viver, todo `CHAMA` que for a `ponto` leva
    /// `candidatos` (dentro do teto do fio), e não os desta máquina.
    #[must_use = "os candidatos valem só enquanto o guarda vive"]
    pub fn anunciar_a_este_ponto(ponto: SocketAddr, candidatos: Vec<SocketAddr>) -> Ditos {
        if let Ok(mut ditos) = DITOS.lock() {
            ditos.insert(ponto, candidatos);
        }
        Ditos { ponto }
    }

    /// Os candidatos ditos para este ponto, se há.
    pub(crate) fn ditos_para(ponto: SocketAddr) -> Option<Vec<SocketAddr>> {
        DITOS.lock().ok()?.get(&ponto).cloned()
    }

    /// Desfaz [`anunciar_a_este_ponto`] ao cair.
    #[derive(Debug)]
    pub struct Ditos {
        ponto: SocketAddr,
    }

    impl Drop for Ditos {
        fn drop(&mut self) {
            if let Ok(mut ditos) = DITOS.lock() {
                ditos.remove(&self.ponto);
            }
        }
    }
}

/// Onde o ponto de encontro atende.
async fn resolver(bilhete: &Bilhete) -> Option<SocketAddr> {
    // Nos testes, antes do DNS: ver [`recusar_a_producao`].
    recusar_a_producao(&bilhete.ponto, "encontro::resolver, a batida, antes do DNS");
    let alvo = bilhete.ponto().ok()?;
    if seele_proto::uri::ponto_aposentado(alvo.maquina) {
        tracing::warn!(ponto = %bilhete.ponto, "ponto de encontro descontinuado: atualize SEELE_ENCONTRO; a batida não saiu");
        return None;
    }
    let aviso = bilhete.aviso().ok()?;
    let achados: Vec<SocketAddr> = tokio::net::lookup_host((alvo.maquina, alvo.porta))
        .await
        .ok()?
        .collect();
    escolher_ponto(&achados, aviso)
}

/// Qual endereço do ponto de encontro usar, entre os que o DNS devolveu.
///
/// **O da família do anfitrião**, e não o primeiro. O pedido que sai daqui é um
/// `LEVE`: ele manda o ponto de encontro avisar o anfitrião do endereço em que
/// esta máquina foi vista. Esse endereço é o que o anfitrião vai furar, e o furo
/// só serve se ele couber no mesmo caminho por onde o QUIC vai tentar passar.
///
/// Com `.next()` — que era o que estava aqui — quem chega por uma rede com IPv6
/// falava com o ponto de encontro por IPv6 para pedir um aviso a um anfitrião
/// IPv4. Duas coisas quebravam de uma vez: o ponto de encontro não tem por onde
/// repassar um aviso que cruza famílias, e mesmo se tivesse, o endereço
/// anunciado seria um IPv6 que o anfitrião IPv4 não alcança — um furo aberto no
/// lugar errado, enquanto o QUIC tenta o outro.
///
/// Sem nenhum da família certa, vale o primeiro: é o que havia antes, e falhar
/// ali é melhor que não tentar.
fn escolher_ponto(achados: &[SocketAddr], aviso: SocketAddr) -> Option<SocketAddr> {
    let mesma_familia = |ponto: &&SocketAddr| ponto.is_ipv4() == aviso.is_ipv4();
    achados
        .iter()
        .find(mesma_familia)
        .or_else(|| achados.first())
        .copied()
}

/// Onde o ponto de encontro atende, a partir do texto que o link carrega.
///
/// **Com a porta padrão quando ela falta**, pela mesma regra de
/// [`Bilhete::ponto`] ([`seele_proto::uri::separar_ponto`]). O texto ia cru
/// para `lookup_host`, que recusa nome sem porta, e como o link sai sem porta
/// (`enc=encontro.seele.app.br/…`) a pergunta ao quarto nunca saiu.
///
/// **Todos os endereços, IPv4 primeiro** ([`ordenar_os_achados`]). A consulta
/// pergunta ao primeiro, e ao seguinte só quando nada saiu ([`consultar`]).
///
/// **Diz por que não achou.** O defeito passou sem ser visto porque o `.ok()?`
/// engolia o erro: o produto sabia por que a pergunta não saía e não contava.
/// Cada lista vazia daqui deixa a causa no `seele.log` (que só grava `info`), e
/// quem lê o log distingue um texto que não é endereço de um nome que não
/// resolve.
async fn resolver_ponto(ponto: &str) -> Vec<SocketAddr> {
    // Nos testes, antes do DNS: ver [`recusar_a_producao`].
    recusar_a_producao(ponto, "encontro::resolver_ponto, antes do DNS");
    let alvo = match seele_proto::uri::separar_ponto(ponto) {
        Ok(alvo) => alvo,
        Err(erro) => {
            tracing::info!(
                ponto,
                %erro,
                "o ponto de encontro do link não é um endereço: a pergunta ao quarto não saiu"
            );
            return Vec::new();
        }
    };
    if seele_proto::uri::ponto_aposentado(alvo.maquina) {
        tracing::warn!(ponto, novo = PONTO_PADRAO, "SEELE_ENCONTRO usa domínio descontinuado: atualize a configuração; a pergunta ao quarto não saiu");
        return Vec::new();
    }
    match tokio::net::lookup_host((alvo.maquina, alvo.porta)).await {
        Ok(achados) => ordenar_os_achados(ponto, achados.collect()),
        Err(erro) => {
            tracing::info!(
                ponto,
                %erro,
                "o ponto de encontro do link não resolveu: a pergunta ao quarto não saiu"
            );
            Vec::new()
        }
    }
}

/// Os endereços que o nome do ponto deu, na ordem em que a consulta os tenta.
///
/// **IPv4 primeiro**, e a ordem do DNS não decide isto: numa máquina sem rota
/// IPv6, o AAAA na frente engole a pergunta em silêncio quando o envio dá certo
/// e o pacote some. O anfitrião aprendeu isso em campo (`resolver`, no
/// `seele-server`).
///
/// **Diz quando não sobrou nenhum.** Um nome que resolve para lista vazia não é
/// erro do DNS, e sem esta linha a pergunta ao quarto deixava de sair sem nada
/// no `seele.log`.
fn ordenar_os_achados(ponto: &str, mut achados: Vec<SocketAddr>) -> Vec<SocketAddr> {
    achados.sort_by_key(|achado| u8::from(achado.is_ipv6()));
    if achados.is_empty() {
        tracing::info!(
            ponto,
            "o nome do ponto de encontro resolveu para nenhum endereço: a pergunta ao quarto \
             não saiu"
        );
    }
    achados
}

/// Quanto se espera o quarto dizer onde um anfitrião mora.
///
/// Um segundo e meio, e é o teto, não o custo comum. As `ENVIOS_AO_QUARTO`
/// voltas cabem nele. Quanto cada consulta paga de fato está em
/// [`onde_mora_hoje`], caso a caso. O prazo inteiro é pago quando o ponto cala
/// nas três voltas, ou quando o nome dele não resolve a tempo, e aí o que sobra
/// são os endereços guardados.
pub const PRAZO_DO_QUARTO: Duration = Duration::from_millis(1500);

/// Quantas voltas de pergunta saem enquanto o ponto não responde.
///
/// Três, e não uma. Era uma, e um datagrama perdido custava a consulta
/// inteira em silêncio.
const ENVIOS_AO_QUARTO: u32 = 3;

/// O intervalo entre uma volta e a seguinte.
const INTERVALO_DOS_ENVIOS: Duration = Duration::from_millis(500);

// As voltas cabem no prazo, como [`PRAZO_DO_QUARTO`] promete: cada uma, a última
// inclusive, com um intervalo inteiro para ser respondida. Uma última volta que
// saísse em cima do prazo seria uma pergunta que ninguém espera ouvir. Conferido
// em tempo de compilação, e não pelo teste do ponto que limita: aquele mede o
// comportamento com um prazo largo, para a carga da máquina não o reprovar.
const _: () = assert!(
    ENVIOS_AO_QUARTO as u128 * INTERVALO_DOS_ENVIOS.as_millis() <= PRAZO_DO_QUARTO.as_millis(),
    "as voltas ao quarto não cabem no PRAZO_DO_QUARTO"
);

/// O piso da espera pela marca que falta, depois da primeira resposta.
///
/// Num caminho curto o dobro da ida e volta são microssegundos, e isso não daria
/// ao ponto tempo nem de responder à segunda pergunta de uma rajada que saiu
/// junto com a primeira.
const FOLGA_MINIMA: Duration = Duration::from_millis(100);

/// Até quando se espera pela marca que falta, depois de o ponto responder.
///
/// A primeira resposta, mais o dobro da ida e volta que o `ONDE` da volta
/// mediu, nunca menos que [`FOLGA_MINIMA`]. As três perguntas saem juntas, e o
/// ponto responde a cada uma assim que ela chega: uma resposta que ainda não
/// veio depois de duas idas e voltas é uma pergunta que ele calou. Quem chama
/// não espera além da volta seguinte de qualquer jeito.
fn teto_depois_da_resposta(
    primeira: tokio::time::Instant,
    ida_e_volta: Duration,
) -> tokio::time::Instant {
    primeira + ida_e_volta.saturating_mul(2).max(FOLGA_MINIMA)
}

/// A marca do `ONDE` que corre junto das duas `QUEM`.
///
/// Oito caracteres, com letras fora do hexadecimal. Tem menos de dezesseis, e
/// por isso nenhuma marca de servidor (`fp16`, `fp16e`, `fp16s`) nem de código
/// (`p` e quinze símbolos, mais o sufixo) é igual a ela: a resposta dela nunca
/// se confunde com a de um morador.
const MARCA_DA_CONSULTA: &str = "consulta";

/// O que o quarto disse sobre onde um anfitrião mora hoje.
///
/// É um tipo, e não `(Option, Option)`, porque «não achei» tem causas que
/// apontam para lugares diferentes. O ponto estar fora do ar é problema de
/// rede, ou de quem opera o ponto. O ponto estar no ar e ninguém morar lá é o
/// anfitrião desligado, ou uma versão que registra outras marcas. Enquanto as
/// duas eram o mesmo `None`, quem investigava não tinha por onde começar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OndeMora {
    /// O ponto respondeu por pelo menos uma das marcas.
    ///
    /// Uma das duas pode faltar: um anfitrião 0.15.0 registra o servidor, e
    /// não registra a escuta com a marca que se pergunta.
    Achado {
        /// Onde o socket do servidor mora: para onde se conecta.
        servidor: Option<SocketAddr>,
        /// Onde o quarto diz que a escuta de avisos mora, **crua**: quem
        /// ocupou a marca da escuta também aparece aqui. Ela só vira o aviso do
        /// `LEVE` pela regra ([`OndeMora::escuta_do_anfitriao`], e o bilhete
        /// que a usa, [`bilhete_desta_volta`]).
        escuta: Option<SocketAddr>,
    },
    /// O ponto respondeu e calou para as duas marcas.
    ///
    /// Ele está no ar, e ninguém mora lá. Ou o anfitrião está fora do ar há
    /// mais que o prazo do quarto, ou o ponto é anterior ao quarto e não
    /// conhece `QUEM`.
    NinguemMora,
    /// O ponto não respondeu a nada dentro do prazo, **ou a pergunta nem saiu**.
    ///
    /// Os dois casos voltam iguais, e o segundo não é culpa do ponto: uma falha
    /// local (sem rota, um socket que o sistema não abriu ou recusou) também
    /// acaba aqui, e a consulta volta na hora em vez de esperar o prazo. Quem lê
    /// isto como «o ponto caiu» erra o diagnóstico. A causa de cada um está no
    /// log (`info`), e é lá que se distingue.
    PontoMudo,
    /// O texto do ponto não é um endereço, ou o nome não resolveu no prazo.
    PontoNaoResolve,
    /// O ponto recusou por excesso de pedidos desta rede
    /// ([`Localizacao::PontoRecusou`]). Só a consulta pelo ENC/3 o distingue
    /// (`seele_ffi::onde_mora_hoje`); ele está no ar, e não está mudo.
    PontoRecusou,
    /// Quem chamou não tinha como formar as marcas, e nenhuma pergunta saiu.
    ///
    /// [`onde_mora_hoje`] nunca devolve isto, porque recebe as marcas prontas.
    /// Quem devolve é quem as monta a partir de uma impressão digital que não
    /// forma marca.
    SemMarca,
}

impl OndeMora {
    /// O endereço do servidor, se o quarto o deu.
    #[must_use]
    pub fn servidor(&self) -> Option<SocketAddr> {
        match self {
            Self::Achado { servidor, .. } => *servidor,
            Self::NinguemMora
            | Self::PontoMudo
            | Self::PontoNaoResolve
            | Self::PontoRecusou
            | Self::SemMarca => None,
        }
    }

    /// O endereço da escuta de avisos, se o quarto o deu, **cru**.
    ///
    /// É o que o quarto disse, e não serve de aviso do `LEVE`: quem ocupou a
    /// marca da escuta também é dito aqui. Para o aviso, a regra é
    /// [`OndeMora::escuta_do_anfitriao`], e o bilhete que a usa é
    /// [`bilhete_desta_volta`].
    #[must_use]
    pub fn escuta(&self) -> Option<SocketAddr> {
        match self {
            Self::Achado { escuta, .. } => *escuta,
            Self::NinguemMora
            | Self::PontoMudo
            | Self::PontoNaoResolve
            | Self::PontoRecusou
            | Self::SemMarca => None,
        }
    }

    /// A escuta que o quarto deu, julgada pelo servidor que ele deu **na mesma
    /// resposta**.
    ///
    /// # Por que o servidor decide
    ///
    /// A escuta vira o aviso do `LEVE`, e o `LEVE` sai antes de qualquer aperto
    /// de mão: o ponto repassa a esse aviso o endereço público de quem chega, e
    /// o instante. A marca dela (os 16 primeiros caracteres da impressão
    /// digital, e um `e`) está em todo link, e no quarto fica quem escreveu
    /// primeiro. Um anfitrião 0.15.0 nunca a registra. A de um anfitrião de
    /// hoje fica livre quando ele passa mais de 60 s fora do ar; quando ele não
    /// se registra naquele ponto (com IPv4 global ele não abre o degrau 4, e
    /// `$SEELE_ENCONTRO` pode desligá-lo ou trocar o ponto, e o bilhete que a
    /// lista guardou continua apontando o ponto de antes); e logo depois de o
    /// ponto reiniciar, até o próximo registro dele, que sai a cada 15 s. As
    /// mesmas três janelas valem para a marca do servidor. Quem a tomou não
    /// fica sabendo de conteúdo nenhum, mas ficava sabendo de onde e quando
    /// cada um tentava chegar (o I1 da revisão final do Plano 1).
    ///
    /// A escuta e o servidor de um anfitrião moram na mesma máquina, e saem
    /// pelo mesmo IP público para o mesmo ponto: os dois sockets se registram
    /// juntos, no mesmo tique de `atender` (`registrar`, no `seele-server`).
    /// Com a escuta noutro IP que o servidor da mesma resposta, uma das duas
    /// marcas foi ocupada por outra pessoa (a da escuta ou a do servidor), ou o
    /// anfitrião sai por dois IPs públicos. A regra não sabe qual das três, e
    /// não usa a escuta: com a marca do servidor tomada, a escuta deixada de
    /// lado é a do anfitrião de verdade. Uma escuta sem servidor não tem quem
    /// a confirme: ou a marca do servidor está vazia, ou a resposta sobre ele
    /// não voltou a tempo.
    ///
    /// # O que sobra
    ///
    /// Quem toma **as duas** marcas passa por aqui: o servidor dele mora no IP
    /// dele. O TLS recusa o servidor errado logo depois, e o convite, a senha e
    /// o apelido não saem. O que sai antes é o `LEVE`, com o IP e a porta
    /// públicos de quem chega. Isto é do servidor guardado, que continua no
    /// ENC/1: a sala pessoal se registra assinada pelo ENC/2 ([`localizar`]).
    ///
    /// **Compara o IP, e não a porta**: os dois sockets têm portas diferentes,
    /// e é a porta que o NAT troca. O IPv4 escrito como IPv6 mapeado
    /// (`::ffff:a.b.c.d`) é o mesmo IP.
    #[must_use]
    pub fn escuta_do_anfitriao(&self) -> EscutaDoQuarto {
        let Some(escuta) = self.escuta() else {
            return EscutaDoQuarto::Nenhuma;
        };
        match self.servidor() {
            Some(servidor) if servidor.ip().to_canonical() == escuta.ip().to_canonical() => {
                EscutaDoQuarto::DoAnfitriao(escuta)
            }
            servidor => EscutaDoQuarto::NaoConfirmada { escuta, servidor },
        }
    }
}

/// O que fazer com a escuta que o quarto deu: o veredito de
/// [`OndeMora::escuta_do_anfitriao`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscutaDoQuarto {
    /// O quarto não deu escuta. O aviso do bilhete guardado fica como estava.
    Nenhuma,
    /// A escuta mora no IP do servidor que a mesma resposta deu. Vira o aviso
    /// do `LEVE` desta volta.
    DoAnfitriao(SocketAddr),
    /// O quarto deu uma escuta que o servidor da mesma resposta não confirma:
    /// ou não veio servidor, ou ele mora noutro IP. Ela não vira aviso de nada.
    NaoConfirmada {
        /// A escuta que o quarto deu.
        escuta: SocketAddr,
        /// O servidor da mesma resposta, se veio.
        servidor: Option<SocketAddr>,
    },
}

/// O bilhete desta volta: o guardado, com o aviso trocado pela escuta que o
/// quarto deu **só** quando o servidor da mesma resposta a confirma.
///
/// O guardado é o do link desta sessão, ou o da lista de conhecidos. A troca
/// vale só para esta volta: quem chama não grava o bilhete devolvido na lista
/// (o `connect` do app grava o guardado). A regra é a de
/// [`OndeMora::escuta_do_anfitriao`].
///
/// Uma escuta deixada de lado vai para o log em `info`, o nível que o
/// `seele.log` grava, com ela, o servidor e o ponto: quem investiga um
/// anfitrião que não foi avisado precisa saber que o quarto deu outra escuta, e
/// por que ela não foi usada.
#[must_use]
pub fn bilhete_desta_volta(guardado: &Bilhete, no_quarto: &OndeMora) -> Bilhete {
    match no_quarto.escuta_do_anfitriao() {
        EscutaDoQuarto::Nenhuma => guardado.clone(),
        EscutaDoQuarto::DoAnfitriao(escuta) => {
            match Bilhete::novo(guardado.ponto.clone(), escuta.to_string()) {
                Ok(desta_volta) => desta_volta,
                // Um `SocketAddr` escrito é sempre um endereço que o bilhete
                // aceita. Se um dia não for, a volta segue com o guardado, e
                // diz por quê.
                Err(erro) => {
                    tracing::info!(
                        ?erro,
                        %escuta,
                        ponto = %guardado.ponto,
                        "quarto: a escuta que ele deu não coube no bilhete; o LEVE vai ao aviso \
                         guardado (do link ou da lista)"
                    );
                    guardado.clone()
                }
            }
        }
        EscutaDoQuarto::NaoConfirmada {
            escuta,
            servidor: None,
        } => {
            tracing::info!(
                %escuta,
                ponto = %guardado.ponto,
                "quarto: deu a escuta e não deu o servidor, que é quem a confirma; ela não vira o \
                 aviso do LEVE, que vai ao aviso guardado (do link ou da lista). Ou o anfitrião \
                 está fora do ar e outra pessoa ocupou a marca da escuta, ou o registro do \
                 servidor dele não chegou ao ponto, ou a resposta sobre o servidor não voltou a \
                 tempo"
            );
            guardado.clone()
        }
        EscutaDoQuarto::NaoConfirmada {
            escuta,
            servidor: Some(servidor),
        } => {
            tracing::info!(
                %escuta,
                %servidor,
                ponto = %guardado.ponto,
                "quarto: a escuta que ele deu mora noutro IP que o servidor da mesma resposta; \
                 ela não vira o aviso do LEVE, que vai ao aviso guardado (do link ou da lista). \
                 A escuta e o servidor de um anfitrião saem pelo mesmo IP público: uma das duas \
                 marcas foi ocupada por outra pessoa (a da escuta ou a do servidor), ou o \
                 anfitrião sai por dois IPs públicos"
            );
            guardado.clone()
        }
    }
}

/// Onde um anfitrião mora **agora**: o socket do servidor e a escuta de avisos.
///
/// # Por que ela existe
///
/// Todo endereço de um anfitrião atrás de NAT é perecível: o mapeamento nasce
/// quando um pacote sai, e o roteador dá outro na abertura seguinte. O que não
/// envelhece é a identidade, e é dela que saem as [`Marcas`]. O anfitrião
/// registra o endereço de hoje sob elas, na subida e a cada quinze segundos,
/// e esta função pergunta.
///
/// # Como pergunta
///
/// O nome do ponto vira os endereços dele, IPv4 primeiro. A consulta pergunta
/// ao primeiro, e passa ao seguinte só quando **nada saiu** por ele: o socket
/// da família dele não abriu, ou o sistema recusou as três perguntas da
/// primeira volta. Uma linha `info` diz que ela passou. Um endereço que recebeu
/// as perguntas e calou não leva a consulta ao seguinte: o ponto está no ar ou
/// fora do ar pelos dois (`consultar`).
///
/// Três perguntas por volta, num socket só: `QUEM` pelo servidor, `QUEM` pela
/// escuta, e um `ONDE` cuja única função é o ponto provar que está no ar. É
/// ele que separa «o ponto não respondeu» de «o ponto respondeu e ninguém mora
/// lá». Enquanto nada responder, saem até três voltas, uma a cada meio
/// segundo.
///
/// # Quanto ela custa
///
/// Cada espera é paga num caso só, e nenhuma passa do `prazo`:
///
/// - **as duas marcas respondem**: volta na hora em que a segunda chega, uma
///   ida e volta até o ponto;
/// - **o ponto responde e uma marca cala** (um anfitrião 0.15.0, que não
///   registra a escuta com esta marca; um anfitrião fora do ar, que não
///   registra nada e volta como [`OndeMora::NinguemMora`]): volta na primeira
///   resposta mais o dobro da ida e volta medida pelo `ONDE`, nunca menos de
///   100 ms, e nunca depois da hora em que a volta seguinte sairia (meio
///   segundo depois do envio). Se o `ONDE` daquela volta se perdeu, não há ida
///   e volta medida, e a espera vai até essa hora. A pergunta não se repete a
///   um ponto que está no ar;
/// - **o ponto só responde numa volta repetida**: cada volta perdida custa meio
///   segundo, e a que respondeu custa o de cima;
/// - **nada sai pela primeira volta de um endereço** (o socket não abriu; sem
///   rota, o sistema recusa as três): a consulta passa na hora ao endereço
///   seguinte, que paga o próprio caso desta lista. Sem endereço seguinte,
///   volta na hora, como [`OndeMora::PontoMudo`];
/// - **o sistema recusa uma volta depois da primeira**: a recusada conta como
///   volta. As perguntas da anterior saíram, e a consulta espera a resposta a
///   elas até a hora da volta seguinte, como se a recusada tivesse saído; a
///   ida e volta do `ONDE` se mede da última volta que saiu. Custa o mesmo que
///   a volta perdida;
/// - **a leitura do socket falha**: a consulta acaba ali, com o que chegou
///   até então: como [`OndeMora::PontoMudo`] se nada respondeu antes; se o
///   ponto já respondeu, como [`OndeMora::NinguemMora`], ou
///   [`OndeMora::Achado`] com o que chegou. Não passa ao endereço seguinte: as
///   perguntas saíram;
/// - **o ponto cala nas três voltas, ou o nome não resolve a tempo**: o prazo
///   inteiro. Resolver o nome conta dentro dele.
///
/// # O que se faz com a resposta
///
/// Ela entra na lista de candidatos, na frente dos guardados. Não substitui a
/// conferência da identidade de quem atender naquele endereço.
///
/// O resultado vai para o log, com o ponto, o que ele disse e quanto a
/// consulta levou (`levou=`, com a resolução do nome dentro). É o dado que
/// faltava quando a pergunta nunca saía e nada dizia isso, e o tempo é o que a
/// medida de campo do G1 só achava numa captura de pacotes. Cada caminho que
/// acaba sem resposta deixa também a sua causa, em `info` (o único nível que o
/// `seele.log` grava): o prazo que venceu, o envio que o sistema recusou, a
/// leitura que falhou, o endereço que a consulta deixou pelo seguinte.
///
/// # É a pergunta do ENC/1
///
/// Um anfitrião 0.17 só se registra pelo `MORA3`, e esta pergunta (o `QUEM`) não
/// o acha. Ela fica pelo que o ponto ainda serve à linha anterior, e pelos
/// testes do ponto. Para achar um anfitrião 0.17, a pergunta é [`localizar3`]:
/// é a que a `Chegada` e `seele_ffi::onde_mora_hoje` fazem.
pub async fn onde_mora_hoje(ponto: &str, marcas: &Marcas, prazo: Duration) -> OndeMora {
    consultar_o_ponto(
        ponto,
        || PeloQuem::nova(marcas),
        prazo,
        "quarto: onde o anfitrião mora hoje",
    )
    .await
}

/// Onde a sala pessoal de quem tem estas marcas mora **agora**, pelo ENC/2
/// (ADR 0062): o socket do servidor, a escuta de avisos e as dicas de rede
/// local.
///
/// As marcas são as da sala (`Marcas::da_sala`), e as perguntas são o `QUEM2`
/// de cada uma, com o `ONDE` da versão 1 como sonda. É a sonda que separa os
/// quatro casos que, para quem liga, apontam para lugares diferentes
/// ([`Localizacao`]): o ponto fala o ENC/2 e a sala está lá; fala, e ela não
/// está (`NINGUEM`); responde ao `ONDE` e cala o `QUEM2`, porque é anterior ao
/// ENC/2; ou não responde a nada.
///
/// Custa o que [`onde_mora_hoje`] custa, caso a caso, e passa pelo mesmo laço:
/// três perguntas por volta, até três voltas, o teto depois da primeira
/// resposta, e o endereço seguinte do ponto só quando nada saiu pelo primeiro.
/// **Uma diferença**: enquanto só a sonda respondeu, a pergunta se repete a um
/// ponto que está no ar, porque um ponto com o limite estourado diz a recusa uma
/// vez por segundo e cala o resto. O caso [`Localizacao::Enc2Calado`] paga o
/// prazo inteiro. O resultado vai para o log, como lá, sem as marcas: elas são
/// o código de quem se procurava.
pub async fn localizar(ponto: &str, marcas: &Marcas, prazo: Duration) -> Localizacao {
    consultar_o_ponto(
        ponto,
        || PeloQuem2::nova(marcas),
        prazo,
        "quarto: onde a sala mora hoje (ENC/2)",
    )
    .await
}

/// O que [`localizar`] achou, para quem liga pelo código dizer o que houve
/// (spec de 23/09/2026, «Erros e frases»).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Localizacao {
    /// O ponto deu pelo menos um endereço da sala.
    ///
    /// Um dos dois pode faltar, quando a resposta sobre ele não voltou a
    /// tempo. As duas marcas só se registram com a chave da pessoa, então a
    /// escuta não precisa da regra do IP do servidor que a do ENC/1 precisa
    /// ([`OndeMora::escuta_do_anfitriao`]).
    Achada {
        /// Para onde se conecta, visto de fora.
        servidor: Option<SocketAddr>,
        /// Para onde vai o `LEVE`, com a marca do aviso da sala.
        escuta: Option<SocketAddr>,
        /// As dicas de rede local do servidor, já conferidas por
        /// `seele_proto::encontro::serve_de_dica`: é por elas que duas pessoas
        /// na mesma casa se ligam sem *hairpin* no roteador.
        dicas: Vec<SocketAddr>,
    },
    /// O ponto fala o ENC/2 e disse `NINGUEM`: a sala não está registrada. O
    /// SEELE do outro lado está fechado, ou não está recebendo ligações.
    NinguemMora,
    /// O ponto respondeu ao `ONDE` e calou para o `QUEM2`, mesmo perguntado de
    /// novo dentro do prazo. Dois casos dão isto, e a culpa não é de quem
    /// recebe em nenhum: o ponto é anterior ao ENC/2 (ninguém acha ninguém pelo
    /// código nele), ou está recusando os pedidos desta rede por excesso e a
    /// recusa, que ele diz uma vez por segundo, foi para outro pedido.
    Enc2Calado,
    /// O ponto recusou por excesso de pedidos desta rede (o limite é por
    /// endereço público, e a rede pode dividi-lo com outras pessoas).
    PontoRecusou,
    /// O ponto não respondeu a nada no prazo, ou a pergunta nem saiu. Como em
    /// [`OndeMora::PontoMudo`].
    PontoMudo,
    /// O texto do ponto não é um endereço, ou o nome não resolveu no prazo.
    PontoNaoResolve,
}

/// Por que quem liga pelo código não achou onde conectar: o lado de
/// [`Localizacao`] que não dá endereço nenhum, mais a falta de ponto a quem
/// perguntar.
///
/// Atravessa como o motivo de `ConnectError::NaoLocalizou`, e cada variante é
/// uma frase diferente da spec de 23/09 («Erros e frases»): o ponto calado, a
/// pessoa que não está disponível. As frases são da casca (tarefa 7f).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FalhaAoLocalizar {
    /// Não há ponto de encontro: `$SEELE_ENCONTRO` o desligou
    /// ([`ponto_de_quem_liga`]). Sem ponto, código não funciona; link com
    /// endereço continua funcionando.
    SemPonto,
    /// [`Localizacao::NinguemMora`].
    NinguemMora,
    /// [`Localizacao::Enc2Calado`].
    Enc2Calado,
    /// [`Localizacao::PontoRecusou`].
    PontoRecusou,
    /// [`Localizacao::PontoMudo`].
    PontoMudo,
    /// [`Localizacao::PontoNaoResolve`].
    PontoNaoResolve,
    /// O ponto deu só a escuta: a resposta sobre o servidor não voltou a
    /// tempo, e sem ela não há endereço a que conectar.
    SoAEscuta,
    /// O ponto achou o anfitrião e repassou o `CHAMA`, e o anfitrião não
    /// respondeu a tempo (`CALADO`, ENC/3): a escuta dele não está recebendo,
    /// ou o teto de chamadas dele estourou.
    AnfitriaoCalado,
}

/// O que as duas consultas ao quarto têm em comum: resolver o ponto dentro do
/// prazo, perguntar endereço por endereço, e dizer no log o que voltou.
async fn consultar_o_ponto<C: Consulta>(
    ponto: &str,
    nova: impl Fn() -> C,
    prazo: Duration,
    o_que: &'static str,
) -> C::Resposta {
    // **O prazo cobre a consulta inteira, e resolver o nome é parte dela.**
    // Numa rede sem internet (duas máquinas na mesma casa, o caso que menos
    // precisa de ponto de encontro), resolver o nome não falha rápido: espera
    // um servidor de DNS que não vai responder. Isto roda antes de qualquer
    // tentativa de conexão, e uma pergunta acessória que atrasa a principal é
    // pior que não perguntar.
    let comecou = tokio::time::Instant::now();
    let ate = comecou + prazo;
    let resposta = match tokio::time::timeout_at(ate, resolver_ponto(ponto)).await {
        // `resolver_ponto` já deixou no log por que não achou.
        Ok(destinos) if destinos.is_empty() => C::nao_resolve(),
        Ok(destinos) => consultar_com_todos(&destinos, nova, ate).await,
        Err(_) => {
            tracing::info!(
                ponto,
                ?prazo,
                "quarto: o nome do ponto de encontro não resolveu dentro do prazo"
            );
            C::nao_resolve()
        }
    };
    tracing::info!(
        %ponto,
        ?resposta,
        levou = ?comecou.elapsed(),
        "{o_que}"
    );
    resposta
}

/// A consulta do ENC/1 pelos endereços do ponto: [`consultar_com_todos`] com
/// [`PeloQuem`], o mesmo caminho de [`onde_mora_hoje`]. Os testes do laço a
/// chamam pelo nome de antes do ENC/2.
#[cfg(test)]
async fn consultar(
    destinos: &[SocketAddr],
    marcas: &Marcas,
    ate: tokio::time::Instant,
) -> OndeMora {
    consultar_com_todos(destinos, || PeloQuem::nova(marcas), ate).await
}

/// A consulta pelos endereços do ponto, um de cada vez, **dentro do mesmo
/// `ate`**.
///
/// Pergunta ao primeiro, e passa ao seguinte só quando nenhuma pergunta saiu
/// por ele: o socket da família dele não abriu, ou o sistema recusou as três
/// perguntas da primeira volta. É o caso de um nome com A e AAAA numa máquina
/// sem rota para a família do primeiro: até aqui a consulta parava ali, como
/// [`OndeMora::PontoMudo`], com o outro endereço do mesmo ponto intocado. Um
/// ponto que recebeu as perguntas e calou não é tentado de novo por outro
/// endereço: ele está no ar ou fora do ar pelos dois. A escolha é
/// [`escolher`].
///
/// Sem endereço que sirva, o «ponto mudo» de cada consulta, como antes: para
/// fora, [`OndeMora`] não muda.
async fn consultar_com_todos<C: Consulta>(
    destinos: &[SocketAddr],
    nova: impl Fn() -> C,
    ate: tokio::time::Instant,
) -> C::Resposta {
    for (indice, destino) in destinos.iter().enumerate() {
        match escolher(perguntar_a(*destino, nova(), ate).await) {
            Escolha::Devolver(resposta) => return resposta,
            Escolha::TentarOSeguinte => {
                // A causa deste endereço já está no log, numa linha de quem
                // falhou. Esta diz que a consulta não parou nele.
                if let Some(seguinte) = destinos.get(indice + 1) {
                    tracing::info!(
                        %destino,
                        %seguinte,
                        "quarto: nenhuma pergunta saiu por este endereço do ponto; a consulta \
                         tenta o seguinte"
                    );
                }
            }
        }
    }
    C::mudo()
}

/// O que a consulta por um endereço do ponto deu, **por dentro**.
///
/// Para fora, os dois primeiros e um ponto que calou até o prazo são o mesmo
/// «ponto mudo». Por dentro eles se separam, porque só os dois primeiros dizem
/// que vale tentar o endereço seguinte: neles nenhuma pergunta chegou a sair, e
/// o ponto não teve chance de responder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Desfecho<R> {
    /// O socket da família deste endereço não abriu.
    SemSocket,
    /// Nenhuma pergunta da primeira volta saiu: o sistema recusou as três.
    NadaSaiu,
    /// As perguntas saíram, e isto é o que o ponto disse, ou calou, até a
    /// consulta acabar.
    Perguntou(R),
}

/// O [`Desfecho`] da consulta do ENC/1, pelo nome que os testes do laço usam.
#[cfg(test)]
type PorUmEndereco = Desfecho<OndeMora>;

/// O que [`consultar_com_todos`] faz com o que um endereço deu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Escolha<R> {
    /// A consulta acaba com esta resposta.
    Devolver(R),
    /// Nenhuma pergunta saiu por este endereço: vale o seguinte.
    TentarOSeguinte,
}

/// A escolha de [`consultar_com_todos`] depois de um endereço do ponto.
fn escolher<R>(desfecho: Desfecho<R>) -> Escolha<R> {
    match desfecho {
        Desfecho::SemSocket | Desfecho::NadaSaiu => Escolha::TentarOSeguinte,
        Desfecho::Perguntou(resposta) => Escolha::Devolver(resposta),
    }
}

/// Abre um socket da família de `destino` e pergunta por ele
/// ([`consultar_com`]).
async fn perguntar_a<C: Consulta>(
    destino: SocketAddr,
    consulta: C,
    ate: tokio::time::Instant,
) -> Desfecho<C::Resposta> {
    let local = if destino.is_ipv4() {
        SocketAddr::from(([0, 0, 0, 0], 0))
    } else {
        SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, 0))
    };
    let socket = match tokio::net::UdpSocket::bind(local).await {
        Ok(socket) => socket,
        Err(erro) => {
            tracing::info!(%erro, %destino, "quarto: não abriu socket para perguntar");
            return Desfecho::SemSocket;
        }
    };
    consultar_com(&socket, destino, consulta, ate).await
}

/// O que uma consulta ao quarto pergunta, como lê as respostas, e o que
/// devolve. São duas: o `QUEM` do ENC/1 ([`PeloQuem`]) e o `QUEM2` do ENC/2
/// ([`PeloQuem2`]). O laço que as pergunta é um só, [`consultar_com`], e é nele
/// que mora tudo o que custou a acertar: as voltas, o teto depois da primeira
/// resposta, a volta recusada que conta.
trait Consulta: std::fmt::Debug {
    /// O que a consulta devolve.
    type Resposta: std::fmt::Debug;
    /// As três perguntas de cada volta: as duas marcas e a sonda (`ONDE`).
    fn pedidos(&self, sonda: &Marca) -> [Vec<u8>; 3];
    /// Lê um datagrama do ponto. `None` quando ele não é deste protocolo, e
    /// então não conta como resposta.
    fn ler(&mut self, datagrama: &[u8], sonda: &Marca) -> Option<Lido>;
    /// Se as duas marcas já responderam.
    fn completa(&self) -> bool;
    /// Se vale perguntar de novo a um ponto que já respondeu a alguma coisa.
    ///
    /// No ENC/1, não: a pergunta não se repete a um ponto que está no ar, e o
    /// teto depois da primeira resposta decide quanto se espera. No ENC/2, sim,
    /// enquanto só a sonda respondeu: um ponto com o limite estourado cala o
    /// `QUEM2` e diz a recusa uma vez por segundo, e é a volta seguinte que a
    /// ouve.
    fn espera_mais(&self) -> bool {
        false
    }
    /// O que a consulta devolve com o que chegou, sabendo se o ponto
    /// respondeu a alguma coisa.
    fn concluir(self, no_ar: bool) -> Self::Resposta;
    /// Nenhum endereço do ponto serviu.
    fn mudo() -> Self::Resposta;
    /// O ponto não resolveu.
    fn nao_resolve() -> Self::Resposta;
}

/// O que um datagrama lido do ponto era.
enum Lido {
    /// A resposta à sonda: o ponto está no ar, e dela sai a ida e volta.
    Sonda,
    /// Outra resposta do ponto: uma das marcas, ou uma que não é de ninguém.
    /// O ponto está no ar.
    Outra,
}

/// A consulta do ENC/1: `QUEM` pelas duas marcas, e o `ONDE` como sonda.
struct PeloQuem<'m> {
    marcas: &'m Marcas,
    servidor: Option<SocketAddr>,
    escuta: Option<SocketAddr>,
}

impl<'m> PeloQuem<'m> {
    fn nova(marcas: &'m Marcas) -> Self {
        Self {
            marcas,
            servidor: None,
            escuta: None,
        }
    }
}

/// Só o que o ponto respondeu, e nunca as marcas: o rastro vai no `seele.log`,
/// e a marca diz quem se procurava.
impl std::fmt::Debug for PeloQuem<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PeloQuem")
            .field("servidor", &self.servidor)
            .field("escuta", &self.escuta)
            .finish()
    }
}

impl Consulta for PeloQuem<'_> {
    type Resposta = OndeMora;

    fn pedidos(&self, sonda: &Marca) -> [Vec<u8>; 3] {
        [
            encontro::quem(self.marcas.servidor()),
            encontro::quem(self.marcas.escuta()),
            encontro::onde(sonda),
        ]
    }

    fn ler(&mut self, datagrama: &[u8], sonda: &Marca) -> Option<Lido> {
        let (marca, endereco) = encontro::ler_aqui(datagrama)?;
        if marca == *sonda {
            return Some(Lido::Sonda);
        }
        if marca == *self.marcas.servidor() {
            self.servidor = Some(endereco);
        } else if marca == *self.marcas.escuta() {
            self.escuta = Some(endereco);
        }
        Some(Lido::Outra)
    }

    fn completa(&self) -> bool {
        self.servidor.is_some() && self.escuta.is_some()
    }

    fn concluir(self, no_ar: bool) -> OndeMora {
        match (self.servidor, self.escuta) {
            (None, None) if no_ar => OndeMora::NinguemMora,
            (None, None) => OndeMora::PontoMudo,
            (servidor, escuta) => OndeMora::Achado { servidor, escuta },
        }
    }

    fn mudo() -> OndeMora {
        OndeMora::PontoMudo
    }

    fn nao_resolve() -> OndeMora {
        OndeMora::PontoNaoResolve
    }
}

/// A consulta do ENC/2: `QUEM2` pelas duas marcas da sala, e o `ONDE` da
/// versão 1 como sonda. É a sonda que separa «o ponto não fala o ENC/2» (ela
/// responde, e os `QUEM2` calam) de «ninguém mora» (os `QUEM2` dizem
/// `NINGUEM`).
///
/// Um `AQUI` da versão 1 com a marca da sala não é endereço da sala: só o
/// `AQUI2` é. A marca da sala não se registra pela versão 1 num ponto com o
/// ENC/2, e um ponto sem ele não a guarda assinada.
struct PeloQuem2<'m> {
    marcas: &'m Marcas,
    servidor: Option<encontro::Resposta2>,
    escuta: Option<encontro::Resposta2>,
}

impl<'m> PeloQuem2<'m> {
    fn nova(marcas: &'m Marcas) -> Self {
        Self {
            marcas,
            servidor: None,
            escuta: None,
        }
    }
}

/// Só o que o ponto respondeu, sem a marca: no ENC/2 a marca é o código de
/// quem se procurava, e o rastro vai no `seele.log`.
impl std::fmt::Debug for PeloQuem2<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        /// A resposta sem a marca.
        fn sem_marca(resposta: Option<&encontro::Resposta2>) -> String {
            match resposta {
                None => "nada".to_owned(),
                Some(encontro::Resposta2::Aqui2 {
                    endereco, dicas, ..
                }) => format!("AQUI2 {endereco} dicas={dicas:?}"),
                Some(encontro::Resposta2::Ninguem { .. }) => "NINGUEM".to_owned(),
                Some(encontro::Resposta2::Recusa { motivo, agora, .. }) => {
                    format!("RECUSA {} {agora}", motivo.texto())
                }
                Some(encontro::Resposta2::Morou { endereco, .. }) => format!("MOROU {endereco}"),
                Some(encontro::Resposta2::Desafio { .. }) => "DESAFIO".to_owned(),
            }
        }
        f.debug_struct("PeloQuem2")
            .field("servidor", &sem_marca(self.servidor.as_ref()))
            .field("escuta", &sem_marca(self.escuta.as_ref()))
            .finish()
    }
}

impl Consulta for PeloQuem2<'_> {
    type Resposta = Localizacao;

    fn pedidos(&self, sonda: &Marca) -> [Vec<u8>; 3] {
        [
            encontro::quem2(self.marcas.servidor()),
            encontro::quem2(self.marcas.escuta()),
            encontro::onde(sonda),
        ]
    }

    fn ler(&mut self, datagrama: &[u8], sonda: &Marca) -> Option<Lido> {
        if let Some((marca, _)) = encontro::ler_aqui(datagrama) {
            return Some(if marca == *sonda {
                Lido::Sonda
            } else {
                Lido::Outra
            });
        }
        let resposta = encontro::ler_resposta2(datagrama)?;
        if resposta.marca() == self.marcas.servidor() {
            self.servidor = Some(resposta);
        } else if resposta.marca() == self.marcas.escuta() {
            self.escuta = Some(resposta);
        }
        Some(Lido::Outra)
    }

    fn completa(&self) -> bool {
        self.servidor.is_some() && self.escuta.is_some()
    }

    fn espera_mais(&self) -> bool {
        self.servidor.is_none() && self.escuta.is_none()
    }

    fn concluir(self, no_ar: bool) -> Localizacao {
        let endereco = |resposta: &Option<encontro::Resposta2>| match resposta {
            Some(encontro::Resposta2::Aqui2 { endereco, .. }) => Some(*endereco),
            _ => None,
        };
        let (servidor, escuta) = (endereco(&self.servidor), endereco(&self.escuta));
        if servidor.is_some() || escuta.is_some() {
            let dicas = match self.servidor {
                Some(encontro::Resposta2::Aqui2 { dicas, .. }) => dicas,
                _ => Vec::new(),
            };
            return Localizacao::Achada {
                servidor,
                escuta,
                dicas,
            };
        }
        let respostas = [self.servidor, self.escuta];
        if respostas
            .iter()
            .flatten()
            .any(|resposta| matches!(resposta, encontro::Resposta2::Recusa { .. }))
        {
            Localizacao::PontoRecusou
        } else if respostas.iter().flatten().next().is_some() {
            Localizacao::NinguemMora
        } else if no_ar {
            Localizacao::Enc2Calado
        } else {
            Localizacao::PontoMudo
        }
    }

    fn mudo() -> Localizacao {
        Localizacao::PontoMudo
    }

    fn nao_resolve() -> Localizacao {
        Localizacao::PontoNaoResolve
    }
}

/// A consulta do ENC/3 (a 0.17): `QUEM3` pelas duas marcas, e o `ONDE` da
/// versão 1 como sonda, no mesmo molde de [`PeloQuem2`].
///
/// O anfitrião 0.17 registra as duas marcas pelo `MORA3` (servidor guardado) ou
/// pelo `MORO3` assinado (sala pessoal), e não publica nada do ENC/1 nem do
/// ENC/2. Por isso quem procura um anfitrião 0.17 pergunta isto; e quem só acha
/// pelo `QUEM2` achou uma 0.16 ([`localizar_com_a_linha`]).
struct PeloQuem3<'m> {
    marcas: &'m Marcas,
    servidor: Option<encontro::Resposta3>,
    escuta: Option<encontro::Resposta3>,
}

impl<'m> PeloQuem3<'m> {
    fn nova(marcas: &'m Marcas) -> Self {
        Self {
            marcas,
            servidor: None,
            escuta: None,
        }
    }
}

/// Só o tipo do que o ponto respondeu, sem a marca nem o endereço.
impl std::fmt::Debug for PeloQuem3<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn sem_marca(resposta: Option<&encontro::Resposta3>) -> &'static str {
            match resposta {
                None => "nada",
                Some(encontro::Resposta3::Aqui { .. }) => "AQUI",
                Some(encontro::Resposta3::Ninguem { .. }) => "NINGUEM",
                Some(encontro::Resposta3::Recusa { .. }) => "RECUSA",
                Some(encontro::Resposta3::Calado { .. }) => "CALADO",
                Some(_) => "outra",
            }
        }
        f.debug_struct("PeloQuem3")
            .field("servidor", &sem_marca(self.servidor.as_ref()))
            .field("escuta", &sem_marca(self.escuta.as_ref()))
            .finish()
    }
}

/// A marca de uma resposta do ENC/3, quando ela tem uma das que se perguntam.
fn marca_da_resposta3(resposta: &encontro::Resposta3) -> Option<&Marca> {
    match resposta {
        encontro::Resposta3::Aqui { marca, .. }
        | encontro::Resposta3::Ninguem { marca }
        | encontro::Resposta3::Recusa { marca, .. }
        | encontro::Resposta3::Calado { marca }
        | encontro::Resposta3::Morou { marca, .. }
        | encontro::Resposta3::Desafio { marca, .. } => Some(marca),
        encontro::Resposta3::Chamado(_) => None,
    }
}

impl Consulta for PeloQuem3<'_> {
    type Resposta = Localizacao;

    fn pedidos(&self, sonda: &Marca) -> [Vec<u8>; 3] {
        [
            encontro::quem3(self.marcas.servidor()),
            encontro::quem3(self.marcas.escuta()),
            encontro::onde(sonda),
        ]
    }

    fn ler(&mut self, datagrama: &[u8], sonda: &Marca) -> Option<Lido> {
        if let Some((marca, _)) = encontro::ler_aqui(datagrama) {
            return Some(if marca == *sonda {
                Lido::Sonda
            } else {
                Lido::Outra
            });
        }
        let resposta = encontro::ler_resposta3(datagrama)?;
        let marca = marca_da_resposta3(&resposta).cloned();
        if marca.as_ref() == Some(self.marcas.servidor()) {
            self.servidor = Some(resposta);
        } else if marca.as_ref() == Some(self.marcas.escuta()) {
            self.escuta = Some(resposta);
        }
        Some(Lido::Outra)
    }

    fn completa(&self) -> bool {
        self.servidor.is_some() && self.escuta.is_some()
    }

    fn espera_mais(&self) -> bool {
        self.servidor.is_none() && self.escuta.is_none()
    }

    fn concluir(self, no_ar: bool) -> Localizacao {
        let endereco = |resposta: &Option<encontro::Resposta3>| match resposta {
            Some(encontro::Resposta3::Aqui { endereco, .. }) => Some(*endereco),
            _ => None,
        };
        let (servidor, escuta) = (endereco(&self.servidor), endereco(&self.escuta));
        if servidor.is_some() || escuta.is_some() {
            let dicas = match self.servidor {
                Some(encontro::Resposta3::Aqui { dicas, .. }) => dicas
                    .into_iter()
                    .filter(|dica| encontro::serve_de_dica(*dica))
                    .collect(),
                _ => Vec::new(),
            };
            return Localizacao::Achada {
                servidor,
                escuta,
                dicas,
            };
        }
        let respostas = [self.servidor, self.escuta];
        if respostas
            .iter()
            .flatten()
            .any(|resposta| matches!(resposta, encontro::Resposta3::Recusa { .. }))
        {
            Localizacao::PontoRecusou
        } else if respostas.iter().flatten().next().is_some() {
            Localizacao::NinguemMora
        } else if no_ar {
            Localizacao::Enc2Calado
        } else {
            Localizacao::PontoMudo
        }
    }

    fn mudo() -> Localizacao {
        Localizacao::PontoMudo
    }

    fn nao_resolve() -> Localizacao {
        Localizacao::PontoNaoResolve
    }
}

/// Onde um anfitrião 0.17 mora agora, pelo ENC/3 (`QUEM3`).
///
/// Serve à sala pessoal (marcas de `Marcas::da_sala`) e ao servidor guardado
/// (marcas de `Marcas::do_servidor`). Custa o que [`localizar`] custa, pelo
/// mesmo laço. [`Localizacao::Enc2Calado`] aqui quer dizer «o ponto responde
/// ao `ONDE` e cala o `QUEM3`»: um ponto anterior ao ENC/3.
pub async fn localizar3(ponto: &str, marcas: &Marcas, prazo: Duration) -> Localizacao {
    consultar_o_ponto(
        ponto,
        || PeloQuem3::nova(marcas),
        prazo,
        "quarto: onde o anfitrião mora hoje (ENC/3)",
    )
    .await
}

/// O que se achou sabendo **de que linha** é quem atende (R6 da revisão).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalizacaoComLinha {
    /// Achado pelo ENC/3: um anfitrião da linha 2 (0.17 em diante).
    Achada(Localizacao),
    /// O ENC/3 não achou, e o ENC/2 achou: **a pessoa está num Servidor 1**
    /// (a 0.16 ou anterior). A 0.17 não conversa com ela; quem abre essa
    /// linha é o launcher.
    NaLinhaUm,
    /// Nenhum dos dois achou: o motivo é o do ENC/3.
    NaoAchou(Localizacao),
}

/// Pergunta `QUEM3` e `QUEM2` **em paralelo** (R6 da revisão do coordenador).
///
/// A 0.17 só conversa com quem registrou pelo ENC/3. Uma pessoa que continua
/// na 0.16 registra a sala pelo `MORO2`: perguntar só o `QUEM3` daria
/// «ninguém mora», e a tela mandaria esperar alguém que está no ar, na outra
/// linha. Com o `QUEM2` junto, esse caso tem nome: [`LocalizacaoComLinha::NaLinhaUm`].
///
/// O ENC/2 só é olhado quando o ENC/3 não achou: um anfitrião que registra nas
/// duas (não há, hoje) vale como da linha 2.
pub async fn localizar_com_a_linha(
    ponto: &str,
    marcas: &Marcas,
    prazo: Duration,
) -> LocalizacaoComLinha {
    let (tres, dois) = tokio::join!(
        localizar3(ponto, marcas, prazo),
        localizar(ponto, marcas, prazo)
    );
    match (tres, dois) {
        (achada @ Localizacao::Achada { .. }, _) => LocalizacaoComLinha::Achada(achada),
        (_, Localizacao::Achada { .. }) => {
            tracing::info!(
                ponto,
                "quarto: o ENC/3 não achou, e o ENC/2 achou: quem se procura está num \
                 Servidor 1 (0.16 ou anterior), que a 0.17 não alcança"
            );
            LocalizacaoComLinha::NaLinhaUm
        }
        (falha, _) => LocalizacaoComLinha::NaoAchou(falha),
    }
}

/// A consulta do ENC/1 por um endereço, num socket já aberto: [`consultar_com`]
/// com [`PeloQuem`], o mesmo caminho de [`onde_mora_hoje`]. Os testes do laço a
/// chamam pelo nome de antes do ENC/2.
#[cfg(test)]
async fn consultar_por(
    canal: &impl Canal,
    destino: SocketAddr,
    marcas: &Marcas,
    ate: tokio::time::Instant,
) -> PorUmEndereco {
    consultar_com(canal, destino, PeloQuem::nova(marcas), ate).await
}

/// As três perguntas a um endereço do ponto, num socket já aberto, repetidas
/// até alguém responder ou o prazo vencer.
///
/// Um socket só para as três: as respostas se separam pela marca, e é o `ONDE`
/// saindo pelo mesmo caminho que as perguntas das marcas que prova que aquele
/// caminho funciona.
///
/// Sai do laço, e com isso decide quanto a conexão espera (a lista caso a caso
/// está em [`onde_mora_hoje`]):
///
/// - quando as duas marcas responderam;
/// - quando o ponto respondeu e venceu o [`teto_depois_da_resposta`], medido
///   pela ida e volta do `ONDE`;
/// - na hora da volta seguinte, com o ponto no ar e o teto ainda por vencer
///   (o `ONDE` se perdeu, ou a ida e volta é longa);
/// - na hora, quando nenhuma pergunta da primeira volta saiu ou a leitura
///   falhou;
/// - no prazo, quando nada respondeu.
///
/// Recebe o socket, e não o abre, só para o teste poder dar o socket. É assim
/// que ele manda a esta consulta um `AQUI` de outro IP, e prova a chamada de
/// [`aceita_origem`] aqui dentro: o predicado sozinho tem teste, e a chamada
/// trocada por um no-op passava em todos (o m1 da revisão final do Plano 1).
/// Ver `um_aqui_de_outro_ip_nao_entra_na_resposta_da_consulta`. E é por isso
/// que ele recebe um [`Canal`], e não o socket: um teste precisa de um envio
/// que o sistema recusa numa volta e não na outra.
async fn consultar_com<C: Consulta>(
    canal: &impl Canal,
    destino: SocketAddr,
    mut consulta: C,
    ate: tokio::time::Instant,
) -> Desfecho<C::Resposta> {
    let Some(sonda) = Marca::nova(MARCA_DA_CONSULTA) else {
        // Inalcançável: a constante é uma marca válida, e
        // `a_marca_da_consulta_e_valida_e_nao_colide_com_marca_de_morador` o prova.
        return Desfecho::NadaSaiu;
    };
    let pedidos = consulta.pedidos(&sonda);

    let mut no_ar = false;
    let mut estourou = false;
    let mut enviados = 0_u32;
    let mut proxima_volta = tokio::time::Instant::now();
    // Quando a volta corrente saiu, quando chegou a primeira resposta, e quanto
    // o `ONDE` levou para voltar: é deles que sai o teto da espera pela marca
    // que falta ([`teto_depois_da_resposta`]).
    let mut saiu_em = proxima_volta;
    let mut primeira_resposta = None;
    let mut ida_e_volta = None;
    // Do tamanho do maior datagrama das duas versões, e um byte a mais: um
    // datagrama maior chega com o byte a mais e não se lê como resposta.
    let mut balde = [0_u8; encontro::TAMANHO2 + 1];

    while !consulta.completa() {
        let agora = tokio::time::Instant::now();
        if agora >= ate {
            estourou = true;
            break;
        }
        let teto = primeira_resposta
            .zip(ida_e_volta)
            .map(|(primeira, rtt)| teto_depois_da_resposta(primeira, rtt));
        // **O ponto respondeu e o teto venceu.** Quem ainda não respondeu não
        // está no quarto: um anfitrião 0.15.0 nunca registra a escuta com esta
        // marca, e um fora do ar não registra nada. Esperar a volta seguinte
        // era meio segundo cobrado de toda conexão a eles.
        let espera_mais = consulta.espera_mais();
        if teto.is_some_and(|teto| agora >= teto) && !espera_mais {
            break;
        }
        if agora >= proxima_volta && enviados < ENVIOS_AO_QUARTO {
            // O ponto já respondeu, e o teto não venceu antes desta hora: ou o
            // `ONDE` daquela volta se perdeu e não há ida e volta medida, ou ela
            // é tão longa que o teto passa da volta seguinte. Quem não
            // respondeu teve um intervalo inteiro para isso, e repetir a
            // pergunta a um ponto que está no ar só atrasa a conexão.
            if no_ar && !espera_mais {
                break;
            }
            // Uma linha por volta, e não por pergunta: um envio que o sistema
            // recusa costuma recusar as três, e a causa é a mesma.
            let mut saidas = 0_u32;
            let mut recusa = None;
            for pedido in &pedidos {
                match canal.mandar(pedido, destino).await {
                    Ok(_) => saidas += 1,
                    Err(erro) => {
                        recusa.get_or_insert(erro);
                    }
                }
            }
            if let Some(erro) = recusa {
                tracing::info!(
                    %erro,
                    %destino,
                    volta = enviados + 1,
                    saidas,
                    "quarto: a pergunta não saiu"
                );
            }
            if depois_da_volta(saidas, enviados) == DepoisDaVolta::NadaSaiu {
                return Desfecho::NadaSaiu;
            }
            // A ida e volta do `ONDE` se mede da última volta que saiu: numa
            // volta recusada, as perguntas no ar são as da anterior.
            if saidas > 0 {
                saiu_em = agora;
            }
            enviados += 1;
            proxima_volta += INTERVALO_DOS_ENVIOS;
        }
        let acordar = if enviados < ENVIOS_AO_QUARTO {
            proxima_volta.min(ate)
        } else {
            ate
        };
        let acordar = teto
            .filter(|_| !espera_mais)
            .map_or(acordar, |teto| teto.min(acordar));
        let (lidos, origem) =
            match tokio::time::timeout_at(acordar, canal.receber(&mut balde)).await {
                Ok(Ok(recebido)) => recebido,
                // Hora da próxima volta, ou do fim: o topo do laço decide.
                Err(_) => continue,
                // O Windows entrega aqui o «porta fechada» de um envio anterior.
                Ok(Err(erro)) if erro.kind() == std::io::ErrorKind::ConnectionReset => continue,
                Ok(Err(erro)) => {
                    tracing::info!(%erro, %destino, "quarto: a leitura falhou");
                    break;
                }
            };
        // Só do ponto a que se perguntou. Uma resposta que chega de outro
        // lugar é ruído da internet, ou alguém tentando escolher para onde esta
        // máquina vai conectar.
        if !aceita_origem(origem, destino) {
            continue;
        }
        let Some(lido) = balde
            .get(..lidos)
            .and_then(|datagrama| consulta.ler(datagrama, &sonda))
        else {
            continue;
        };
        no_ar = true;
        let chegou = tokio::time::Instant::now();
        primeira_resposta.get_or_insert(chegou);
        if let Lido::Sonda = lido {
            ida_e_volta.get_or_insert(chegou.saturating_duration_since(saiu_em));
        }
    }

    if estourou {
        tracing::info!(
            %destino,
            enviados,
            no_ar,
            ?consulta,
            "quarto: o prazo da consulta venceu antes de as duas marcas responderem"
        );
    }
    Desfecho::Perguntou(consulta.concluir(no_ar))
}

/// Por onde [`consultar_por`] fala com o ponto: em produção, o socket UDP que
/// [`perguntar_a`] abre.
///
/// Um traço, e não o `tokio::net::UdpSocket` direto, por um teste só:
/// `uma_volta_recusada_depois_da_primeira_ainda_ouve_a_resposta_tardia`. Ele
/// precisa de um envio que dá certo na volta 1 e é recusado na 2, e nenhum
/// socket de verdade faz isso do mesmo jeito no macOS, no Windows e no Linux
/// (as recusas que cada sistema faz estão medidas em `destino_recusado`, nos
/// testes deste módulo).
trait Canal {
    /// Manda um datagrama a `destino`, como `send_to`.
    async fn mandar(&self, datagrama: &[u8], destino: SocketAddr) -> std::io::Result<usize>;
    /// Espera o próximo datagrama, como `recv_from`.
    async fn receber(&self, balde: &mut [u8]) -> std::io::Result<(usize, SocketAddr)>;
}

impl Canal for tokio::net::UdpSocket {
    async fn mandar(&self, datagrama: &[u8], destino: SocketAddr) -> std::io::Result<usize> {
        self.send_to(datagrama, destino).await
    }

    async fn receber(&self, balde: &mut [u8]) -> std::io::Result<(usize, SocketAddr)> {
        self.recv_from(balde).await
    }
}

/// O que [`consultar_por`] faz depois de mandar uma volta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DepoisDaVolta {
    /// Nenhuma pergunta saiu, e nenhuma volta antes desta: não há resposta que
    /// possa vir, e a consulta por este endereço acaba na hora.
    NadaSaiu,
    /// A volta conta, tenha saído ou não, e a consulta espera a resposta até a
    /// volta seguinte.
    Contar,
}

/// A decisão de [`consultar_por`] depois de mandar a volta `enviados + 1`, da
/// qual saíram `saidas` perguntas.
///
/// **Só a primeira volta recusada inteira acaba a consulta.** Sem rota
/// (`ENETUNREACH`, `EHOSTUNREACH`) o sistema recusa as três na hora, e repetir a
/// volta e dormir até o prazo só faria a conexão pagar 1,5 s por uma resposta
/// que não tem como vir. A `perguntar` antiga também saía ali, na hora.
///
/// Numa volta 2 ou 3 não é assim: as perguntas da volta anterior saíram, e a
/// resposta a elas ainda pode chegar. Parar ali perdia essa resposta, e a
/// consulta voltava como `PontoMudo` com o anfitrião no quarto (o T6f da
/// pendência 51, item 17). A volta recusada **conta como volta**: a seguinte sai
/// meio segundo depois, e a recusa não vira laço quente.
fn depois_da_volta(saidas: u32, enviados: u32) -> DepoisDaVolta {
    if saidas == 0 && enviados == 0 {
        DepoisDaVolta::NadaSaiu
    } else {
        DepoisDaVolta::Contar
    }
}

/// Se uma resposta à consulta veio do ponto a que se perguntou.
///
/// É a única barreira entre um `AQUI` de terceiro e a resposta da consulta. O
/// servidor que o quarto responde entra na frente da escada, e a escuta vira o
/// aviso do `LEVE` ([`bilhete_desta_volta`]). Um `AQUI` forjado que passasse
/// daqui escolheria para onde esta máquina conecta, e, com as duas marcas
/// forjadas no mesmo IP, para onde vai o `LEVE`, que sai antes do TLS com o
/// endereço de quem chega. Quem protege a conexão depois disso é a impressão
/// digital conferida no aperto de mão; o `LEVE` não tem outra barreira.
///
/// A chamada dentro de [`consultar_por`] tem prova própria, com um `AQUI` de
/// outro IP mandado ao socket da consulta
/// (`um_aqui_de_outro_ip_nao_entra_na_resposta_da_consulta`).
///
/// **Compara o IP, não a porta**, pelo motivo de `aviso_e_do_ponto`, no
/// anfitrião: quem consegue forjar um endereço de origem forja a porta junto, e
/// recusar outra porta só quebraria um ponto atrás de um balanceador.
fn aceita_origem(origem: SocketAddr, destino: SocketAddr) -> bool {
    origem.ip() == destino.ip()
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::sync::Arc;

    /// O ponto de encontro é escolhido pela família do anfitrião, não pela ordem
    /// do DNS.
    ///
    /// O caso de campo: Mac numa rede 5G com IPv6, anfitrião Windows com
    /// endereço IPv4. O DNS devolve o AAAA primeiro, e com `.next()` o pedido
    /// saía por IPv6 — o ponto de encontro registrou `Network is unreachable`
    /// ao tentar repassar o aviso ao anfitrião IPv4, três vezes seguidas.
    #[test]
    fn o_ponto_de_encontro_e_o_da_familia_do_anfitriao() {
        let seis = "[2001:db8::1]:8384".parse::<SocketAddr>();
        let quatro = "216.128.168.216:8384".parse::<SocketAddr>();
        let anfitriao_v4 = "187.255.97.152:9621".parse::<SocketAddr>();
        let anfitriao_v6 = "[2804:388::1]:9621".parse::<SocketAddr>();
        let (Ok(seis), Ok(quatro), Ok(anfitriao_v4), Ok(anfitriao_v6)) =
            (seis, quatro, anfitriao_v4, anfitriao_v6)
        else {
            panic!("os endereços deste teste têm de ser válidos");
        };

        // O DNS devolve o IPv6 primeiro, como no 5G.
        let achados = [seis, quatro];
        assert_eq!(
            escolher_ponto(&achados, anfitriao_v4),
            Some(quatro),
            "um anfitrião IPv4 só é avisado por um ponto de encontro IPv4"
        );
        assert_eq!(escolher_ponto(&achados, anfitriao_v6), Some(seis));

        // Sem a família certa, o primeiro — falhar ali é melhor que não tentar.
        assert_eq!(escolher_ponto(&[seis], anfitriao_v4), Some(seis));
        assert_eq!(escolher_ponto(&[], anfitriao_v4), None);
    }

    #[tokio::test]
    async fn o_ponto_escrito_sem_porta_e_procurado_na_porta_do_ponto_de_encontro() {
        // O defeito de campo: o link carrega `enc=encontro.seele.app.br/…`, sem
        // porta, e este texto ia cru para `lookup_host`, que recusa nome sem
        // porta. A pergunta ao quarto nunca saiu, e nada dizia isso.
        assert_eq!(
            resolver_ponto("127.0.0.1").await,
            [SocketAddr::from(([127, 0, 0, 1], encontro::PORTA_PADRAO))],
            "um ponto sem porta não foi procurado na porta do ponto de encontro"
        );
        assert_eq!(
            resolver_ponto("[::1]").await,
            [SocketAddr::from((
                std::net::Ipv6Addr::LOCALHOST,
                encontro::PORTA_PADRAO
            ))],
            "um IPv6 entre colchetes sem porta não foi procurado na porta do ponto de encontro"
        );
        assert_eq!(
            resolver_ponto("127.0.0.1:9000").await,
            [SocketAddr::from(([127, 0, 0, 1], 9000))],
            "a porta escrita deixou de mandar"
        );
        assert!(
            resolver_ponto("tem espaço").await.is_empty(),
            "um texto que não é endereço virou pergunta"
        );
    }

    #[test]
    fn os_achados_vao_com_o_ipv4_na_frente_e_a_lista_vazia_diz_no_rastro() {
        // T5a e T5c da pendência 51, item 17. O DNS devolve o AAAA primeiro
        // numa máquina com IPv6, e a consulta pergunta ao primeiro da lista:
        // sem rota IPv6, o envio dá certo, o pacote some, e a consulta paga o
        // prazo inteiro calada. E um nome que resolve para nada não é erro do
        // DNS: sem a linha, a pergunta não saía e o `seele.log` não dizia.
        let captura = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::INFO);
        let _guarda = tracing::subscriber::set_default(captura.clone());
        let seis = SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, encontro::PORTA_PADRAO));
        let quatro = SocketAddr::from(([127, 0, 0, 1], encontro::PORTA_PADRAO));

        assert_eq!(
            ordenar_os_achados("encontro.exemplo", vec![seis, quatro]),
            [quatro, seis],
            "o AAAA que o DNS deu primeiro ficou na frente: numa máquina sem rota IPv6, a \
             pergunta ao quarto some em silêncio e a consulta paga o prazo inteiro"
        );
        assert!(
            captura.linhas().is_empty(),
            "uma lista com endereços deixou linha no rastro: {:?}",
            captura.linhas()
        );

        assert!(
            ordenar_os_achados("encontro.exemplo", Vec::new()).is_empty(),
            "a lista vazia ganhou um endereço"
        );
        let linhas = captura.linhas();
        assert!(
            linhas.iter().any(|linha| linha.starts_with("INFO")
                && linha.contains("resolveu para nenhum endereço")
                && linha.contains("encontro.exemplo")),
            "o nome que resolveu para nenhum endereço não disse no rastro (`info`, o único nível \
             que o seele.log grava) qual era: a pergunta ao quarto não sai e ninguém fica sabendo \
             por quê. Rastro: {linhas:?}"
        );
    }

    #[tokio::test]
    async fn um_ponto_que_nao_se_procura_diz_no_rastro_por_que_nao() {
        // «O produto sabe e não conta»: o `.ok()?` calado que havia aqui deixou a
        // pergunta ao quarto sem sair, e o `seele.log` sem uma linha sobre isso.
        // `#[tokio::test]` de thread única, de propósito: `set_default` fixa o
        // `Subscriber` só na thread corrente, e a resolução acontece nela.
        let captura = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::INFO);
        let _guarda = tracing::subscriber::set_default(captura.clone());

        assert!(
            resolver_ponto("tem espaço").await.is_empty(),
            "um texto que não é endereço virou pergunta: este teste não mede nada"
        );
        assert!(
            resolver_ponto("nao-existe-mesmo.invalid").await.is_empty(),
            "um nome que não existe resolveu: este teste não mede nada"
        );

        let linhas = captura.linhas();
        assert!(
            linhas.iter().any(|linha| linha.starts_with("INFO")
                && linha.contains("não é um endereço")
                && linha.contains("tem espaço")
                && linha.contains("EnderecoInvalido")),
            "um ponto que não é endereço deixou de dizer no rastro (`info`, o único nível que o \
             seele.log grava) qual era o texto e qual foi a causa: a pergunta ao quarto não sai \
             e ninguém fica sabendo por quê. Rastro: {linhas:?}"
        );
        assert!(
            linhas.iter().any(|linha| linha.starts_with("INFO")
                && linha.contains("não resolveu")
                && linha.contains("nao-existe-mesmo.invalid")),
            "um ponto que não resolve deixou de dizer no rastro (`info`) qual era o nome: a \
             pergunta ao quarto não sai e ninguém fica sabendo por quê. Rastro: {linhas:?}"
        );
    }

    #[test]
    fn a_consulta_so_aceita_resposta_do_ip_a_que_perguntou() {
        // A única barreira entre um `AQUI` de terceiro e a lista de candidatos:
        // o que o quarto responde entra na frente da escada, e um `AQUI` forjado
        // que passasse daqui escolheria para onde esta máquina conecta.
        let ponto = SocketAddr::from(([216, 128, 168, 216], encontro::PORTA_PADRAO));
        let terceiro = SocketAddr::from(([203, 0, 113, 9], encontro::PORTA_PADRAO));
        assert!(
            !aceita_origem(terceiro, ponto),
            "um AQUI que veio de outro IP que não o do ponto foi aceito: qualquer um na internet \
             escolhe para onde esta máquina conecta"
        );
        assert!(
            aceita_origem(ponto, ponto),
            "a resposta do próprio ponto foi recusada: a consulta nunca acharia ninguém"
        );
        // Compara o IP e não a porta, como o anfitrião faz em `aviso_e_do_ponto`:
        // quem forja a origem forja a porta junto, e recusar outra porta só
        // quebraria um ponto atrás de um balanceador.
        let outra_porta = SocketAddr::from(([216, 128, 168, 216], 50_000));
        assert!(
            aceita_origem(outra_porta, ponto),
            "a resposta do IP do ponto por outra porta foi recusada: um ponto atrás de um \
             balanceador deixaria de ser ouvido"
        );
    }

    #[tokio::test]
    async fn um_aqui_de_outro_ip_nao_entra_na_resposta_da_consulta() {
        // O m1 da revisão final do Plano 1. O predicado tem teste (logo acima),
        // e a **chamada** dele dentro da consulta não tinha nenhum: trocada por
        // um no-op, os testes de `encontro` e do quarto passavam todos, e o
        // único sinal era um aviso de função sem uso.
        //
        // Aqui um terceiro, noutro IP, manda à consulta um `AQUI` com a marca
        // do servidor antes de o ponto responder. O ponto está em `::1`, e o
        // terceiro em `127.0.0.1`: é o outro IP desta máquina (o macOS recusa o
        // `127.0.0.2`). A consulta corre num socket de pilha dupla, o mesmo que
        // a ponta da sessão usa, e é por ele que o pacote do IPv4 chega.
        let dupla = crate::client::socket_de_quem_conecta().expect("o socket de pilha dupla");
        dupla.set_nonblocking(true).unwrap();
        let consulta = tokio::net::UdpSocket::from_std(dupla).unwrap();
        assert!(
            consulta.local_addr().unwrap().is_ipv6(),
            "o socket local caiu para IPv4: sem pilha dupla, o `AQUI` do terceiro não chega à \
             consulta e este teste não mede nada"
        );
        let porta_da_consulta = consulta.local_addr().unwrap().port();
        let ponto = tokio::net::UdpSocket::bind("[::1]:0").await.unwrap();
        let onde_ponto = ponto.local_addr().unwrap();
        let terceiro = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let para_a_consulta = SocketAddr::from(([127, 0, 0, 1], porta_da_consulta));
        let Some(marcas) = Marcas::do_servidor(FP) else {
            panic!("a impressão digital de teste tem de formar marcas");
        };
        let forjado = SocketAddr::from(([203, 0, 113, 66], 4_444));

        // O ponto: na primeira pergunta, o terceiro manda o `AQUI` forjado, e
        // só depois o ponto responde ao `ONDE`. O `QUEM` ele cala: ninguém mora
        // no quarto.
        let atender = async {
            let mut balde = [0_u8; encontro::TAMANHO];
            let mut forjou = false;
            loop {
                let (lidos, de) = ponto.recv_from(&mut balde).await.unwrap();
                if !forjou {
                    forjou = true;
                    terceiro
                        .send_to(&encontro::aqui(marcas.servidor(), forjado), para_a_consulta)
                        .await
                        .unwrap();
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                if let Some(encontro::Pedido::Onde { marca }) =
                    balde.get(..lidos).and_then(encontro::analisar)
                {
                    ponto
                        .send_to(&encontro::aqui(&marca, de), de)
                        .await
                        .unwrap();
                }
            }
        };
        let ate = tokio::time::Instant::now() + PRAZO_DO_QUARTO;
        let achado = tokio::select! {
            achado = consultar_por(&consulta, onde_ponto, &marcas, ate) => achado,
            () = atender => unreachable!("o ponto deste teste não para"),
        };

        assert_eq!(
            achado,
            PorUmEndereco::Perguntou(OndeMora::NinguemMora),
            "a consulta aceitou um `AQUI` que veio de outro IP que o do ponto ({achado:?}): \
             qualquer um que acerte a porta efêmera escolhe para onde esta máquina conecta e, \
             com a marca da escuta, para onde vai o `LEVE`, antes do TLS"
        );

        // E o caminho do terceiro existe: sem isto, um `NinguemMora` também
        // sairia de um pacote que nunca chegou, e o teste passaria sem medir.
        terceiro
            .send_to(&encontro::aqui(marcas.servidor(), forjado), para_a_consulta)
            .await
            .unwrap();
        let mut balde = [0_u8; encontro::TAMANHO];
        let chegou =
            tokio::time::timeout(Duration::from_secs(1), consulta.recv_from(&mut balde)).await;
        assert!(
            matches!(chegou, Ok(Ok((_, de))) if de.ip() != onde_ponto.ip()),
            "o `AQUI` do terceiro não chega ao socket da consulta ({chegou:?}): este teste não \
             mede a barreira, e o `NinguemMora` de cima não prova nada"
        );
    }

    /// O servidor de um anfitrião, a escuta dele, e quem ocupou a marca da
    /// escuta de outro lugar. Endereços de documentação (RFC 5737).
    const SERVIDOR: ([u8; 4], u16) = ([203, 0, 113, 7], 9_621);
    const ESCUTA: ([u8; 4], u16) = ([203, 0, 113, 7], 51_000);
    const OCUPANTE: ([u8; 4], u16) = ([198, 51, 100, 9], 51_000);

    #[test]
    fn a_escuta_do_quarto_so_vale_no_ip_do_servidor_da_mesma_resposta() {
        // O I1 da revisão final do Plano 1. A escuta que o quarto dá vira o
        // aviso do `LEVE`, que sai antes do TLS e leva o endereço de quem chega.
        // A marca dela (fp16 + `e`) está em todo link, e no quarto fica quem
        // escreveu primeiro: um anfitrião 0.15.0 nunca a registra, e qualquer um
        // com o link a toma. A escuta e o servidor de um anfitrião moram na
        // mesma máquina e saem pelo mesmo IP público, e o servidor é conferido
        // no TLS logo depois. Com a escuta noutro IP, uma das duas marcas é de
        // outra pessoa (a da escuta ou a do servidor), ou o anfitrião sai por
        // dois IPs: a regra não usa a escuta em nenhum dos três casos. O
        // `OCUPANTE` daqui é o caso barato, quem tomou só a da escuta.
        let servidor = SocketAddr::from(SERVIDOR);
        let escuta = SocketAddr::from(ESCUTA);
        let ocupante = SocketAddr::from(OCUPANTE);

        assert_eq!(
            OndeMora::Achado {
                servidor: Some(servidor),
                escuta: Some(escuta),
            }
            .escuta_do_anfitriao(),
            EscutaDoQuarto::DoAnfitriao(escuta),
            "a escuta no IP do servidor da mesma resposta foi recusada: o anfitrião que mudou de \
             porta deixa de ser avisado pela escuta de hoje"
        );
        assert_eq!(
            OndeMora::Achado {
                servidor: Some(servidor),
                escuta: Some(ocupante),
            }
            .escuta_do_anfitriao(),
            EscutaDoQuarto::NaoConfirmada {
                escuta: ocupante,
                servidor: Some(servidor),
            },
            "uma escuta noutro IP que o do servidor da mesma resposta virou aviso: quem ocupou a \
             marca da escuta recebe, antes do TLS, o endereço de quem chega"
        );
        assert_eq!(
            OndeMora::Achado {
                servidor: None,
                escuta: Some(ocupante),
            }
            .escuta_do_anfitriao(),
            EscutaDoQuarto::NaoConfirmada {
                escuta: ocupante,
                servidor: None,
            },
            "uma escuta sem servidor na mesma resposta virou aviso: com o anfitrião fora do ar, \
             quem ocupou a marca da escuta recebe o endereço de quem chega"
        );
        assert_eq!(
            OndeMora::Achado {
                servidor: Some(servidor),
                escuta: None,
            }
            .escuta_do_anfitriao(),
            EscutaDoQuarto::Nenhuma,
            "o quarto não deu escuta (o anfitrião 0.15.0) e a regra inventou uma"
        );
        for sem_resposta in [
            OndeMora::NinguemMora,
            OndeMora::PontoMudo,
            OndeMora::PontoNaoResolve,
            OndeMora::SemMarca,
        ] {
            assert_eq!(
                sem_resposta.escuta_do_anfitriao(),
                EscutaDoQuarto::Nenhuma,
                "{sem_resposta:?} não traz escuta, e a regra inventou uma"
            );
        }

        // O mesmo IP escrito nas duas formas: um ponto de pilha dupla escreve o
        // IPv4 como `::ffff:a.b.c.d`. A forma não muda de quem é o endereço.
        let mapeado = SocketAddr::new(
            std::net::Ipv4Addr::from(SERVIDOR.0).to_ipv6_mapped().into(),
            SERVIDOR.1,
        );
        assert_eq!(
            OndeMora::Achado {
                servidor: Some(mapeado),
                escuta: Some(escuta),
            }
            .escuta_do_anfitriao(),
            EscutaDoQuarto::DoAnfitriao(escuta),
            "o mesmo IP, escrito como IPv4 mapeado num lado e cru no outro, foi tratado como \
             dois: um ponto de pilha dupla deixaria de avisar o anfitrião pela escuta de hoje"
        );
    }

    #[test]
    fn o_bilhete_desta_volta_so_troca_o_aviso_pela_escuta_que_o_servidor_confirma() {
        // `bilhete_desta_volta` é o que a casca usa (pela FFI) para montar o
        // `LEVE` da volta. O guardado é o do link desta sessão ou o da lista.
        // E o que ele deixa de lado vai para o `seele.log` (`info`), com a
        // escuta e o servidor: «o produto sabe e não conta» é o defeito que
        // mais custou neste repositório.
        let captura = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::INFO);
        let _guarda = tracing::subscriber::set_default(captura.clone());
        let guardado =
            Bilhete::novo("192.0.2.1:8384", "203.0.113.7:40000").expect("bilhete de teste");
        let servidor = SocketAddr::from(SERVIDOR);
        let escuta = SocketAddr::from(ESCUTA);
        let ocupante = SocketAddr::from(OCUPANTE);

        let confirmada = bilhete_desta_volta(
            &guardado,
            &OndeMora::Achado {
                servidor: Some(servidor),
                escuta: Some(escuta),
            },
        );
        assert_eq!(
            confirmada,
            Bilhete::novo("192.0.2.1:8384", escuta.to_string()).expect("bilhete de teste"),
            "a escuta confirmada pelo servidor não trocou o aviso: o anfitrião que mudou de porta \
             é avisado no endereço velho"
        );

        for (no_quarto, o_que_quebra) in [
            (
                OndeMora::Achado {
                    servidor: Some(servidor),
                    escuta: Some(ocupante),
                },
                "uma escuta noutro IP que o do servidor",
            ),
            (
                OndeMora::Achado {
                    servidor: None,
                    escuta: Some(ocupante),
                },
                "uma escuta sem servidor na mesma resposta",
            ),
        ] {
            assert_eq!(
                bilhete_desta_volta(&guardado, &no_quarto),
                guardado,
                "{o_que_quebra} trocou o aviso do bilhete: o `LEVE` vai a quem ocupou a marca \
                 da escuta, com o endereço de quem chega"
            );
        }
        assert_eq!(
            bilhete_desta_volta(&guardado, &OndeMora::PontoMudo),
            guardado,
            "sem resposta do quarto, o bilhete guardado mudou"
        );

        let linhas = captura.linhas();
        // Cada linha diz também as causas possíveis, e não só a escuta: com o
        // servidor noutro IP, quem ocupou uma marca pode ter ocupado a do
        // servidor, e aí a escuta deixada de lado é a do anfitrião de verdade
        // (F-Seg-m1 da revisão final do Plano 1). Sem servidor, a resposta
        // dele pode só não ter voltado a tempo.
        for (caso, dito, causas) in [
            (
                "noutro IP que o servidor",
                servidor.to_string(),
                &[
                    "uma das duas marcas foi ocupada por outra pessoa (a da escuta ou a do \
                     servidor)",
                    "ou o anfitrião sai por dois IPs públicos",
                ][..],
            ),
            (
                "sem servidor",
                "não deu o servidor".to_owned(),
                &["ou a resposta sobre o servidor não voltou a tempo"][..],
            ),
        ] {
            let Some(linha) = linhas.iter().find(|linha| {
                linha.starts_with("INFO")
                    && linha.contains("não vira o aviso")
                    && linha.contains(&ocupante.to_string())
                    && linha.contains(&dito)
            }) else {
                panic!(
                    "a escuta deixada de lado {caso} não foi dita no rastro (`info`, o único \
                     nível que o seele.log grava) com a escuta e «{dito}»: quem investiga um \
                     anfitrião que não foi avisado não sabe que o quarto deu outra escuta, nem \
                     por que ela ficou de lado. Rastro: {linhas:?}"
                );
            };
            for causa in causas {
                assert!(
                    linha.contains(causa),
                    "a linha da escuta deixada de lado {caso} não diz «{causa}»: quem lê o \
                     seele.log culpa quem ocupou a escuta, e pode ser o anfitrião de verdade com \
                     a marca do servidor tomada, ou a resposta que se perdeu. Linha: {linha}"
                );
            }
        }
        assert!(
            !linhas
                .iter()
                .any(|linha| linha.contains("não vira o aviso")
                    && linha.contains(&escuta.to_string())),
            "a escuta confirmada foi dita como deixada de lado: o log acusa o anfitrião de \
             verdade. Rastro: {linhas:?}"
        );
    }

    #[test]
    fn a_marca_da_consulta_e_valida_e_nao_colide_com_marca_de_morador() {
        // O `ONDE` da consulta só prova que o ponto está no ar se a resposta
        // dele não puder ser confundida com a de um morador. Uma marca de
        // servidor tem 16 ou 17 caracteres, e os 16 primeiros são
        // hexadecimais. Uma de código começa com `p` e tem 16 ou 17.
        assert!(
            Marca::nova(MARCA_DA_CONSULTA).is_some(),
            "a marca do ONDE não é uma marca: a consulta nunca saberia que o ponto está no ar"
        );
        assert!(
            MARCA_DA_CONSULTA.len() < 16,
            "a marca do ONDE tem o tamanho de uma marca de morador"
        );
        assert!(
            MARCA_DA_CONSULTA.chars().any(|c| !c.is_ascii_hexdigit()),
            "a marca do ONDE é hexadecimal pura e pode colidir com o começo de uma impressão digital"
        );
    }

    #[tokio::test]
    async fn a_consulta_que_estoura_o_prazo_diz_no_rastro_o_que_perguntou_e_o_que_ouviu() {
        // «O produto sabe e não conta»: um ponto que não responde acabava em
        // `PontoMudo` sem uma linha sobre quanto se esperou, e o `seele.log`
        // (que só grava `info`) ficava mudo junto com ele. Este teste fixa a
        // `Subscriber` só na thread corrente, e a consulta corre nela.
        let captura = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::INFO);
        let _guarda = tracing::subscriber::set_default(captura.clone());
        let mudo = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let onde_fica = mudo.local_addr().unwrap().to_string();
        let Some(marcas) = Marcas::do_servidor(FP) else {
            panic!("a impressão digital de teste tem de formar marcas");
        };

        let achado = onde_mora_hoje(&onde_fica, &marcas, Duration::from_millis(150)).await;

        assert_eq!(
            achado,
            OndeMora::PontoMudo,
            "um ponto que não respondeu virou outra coisa: este teste não mede nada"
        );
        let linhas = captura.linhas();
        assert!(
            linhas.iter().any(|linha| linha.starts_with("INFO")
                && linha.contains("prazo")
                && linha.contains(&onde_fica)),
            "o prazo da consulta venceu sem dizer no rastro (`info`, o único nível que o \
             seele.log grava) a que ponto se perguntou: quem lê o log não distingue um ponto \
             fora do ar de uma pergunta que nunca saiu. Rastro: {linhas:?}"
        );
        assert!(
            linhas.iter().any(|linha| linha.starts_with("INFO")
                && linha.contains("onde o anfitrião mora hoje")
                && linha.contains("PontoMudo")
                && linha.contains(&onde_fica)),
            "o resultado da consulta deixou de ir para o rastro (`info`): «o ponto calou» e «o \
             anfitrião não está» voltam a não deixar dado nenhum. Rastro: {linhas:?}"
        );
    }

    #[tokio::test]
    async fn um_nome_que_nao_resolve_no_prazo_diz_no_rastro_que_foi_o_prazo() {
        // O outro estouro: o do nome. Sem internet, resolver não falha rápido, e
        // o prazo vence antes de a pergunta existir. Sem isto o rastro trazia só
        // `PontoNaoResolve`, o mesmo de um texto que não é endereço, e quem lê
        // não distinguia um DNS mudo de um link malformado.
        //
        // Prazo zero, e um nome que só a resolução de verdade responde: a
        // primeira sondagem do `lookup_host` fica pendente, e o prazo, já
        // vencido, ganha. Um endereço em números resolveria sem esperar.
        let captura = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::INFO);
        let _guarda = tracing::subscriber::set_default(captura.clone());
        let Some(marcas) = Marcas::do_servidor(FP) else {
            panic!("a impressão digital de teste tem de formar marcas");
        };

        let achado = onde_mora_hoje("nao-existe-mesmo.invalid:8384", &marcas, Duration::ZERO).await;

        assert_eq!(
            achado,
            OndeMora::PontoNaoResolve,
            "um nome que não resolveu no prazo virou outra coisa: este teste não mede nada"
        );
        let linhas = captura.linhas();
        // **Quando a resolução responde na primeira sondagem, não houve prazo
        // a vencer**, e este teste não mede o que nomeia. Aconteceu no Windows
        // do CI (13/11): o sistema já sabia que `.invalid` não existe e disse
        // «No such host is known» sem ficar pendente. O rastro dessa resposta
        // é outro, e é cobrado no teste do nome que não resolve; aqui se diz
        // que a rodada foi parcial, em vez de reprovar pelo ramo errado.
        if linhas
            .iter()
            .any(|linha| linha.contains("nao-existe-mesmo.invalid") && linha.contains("erro="))
            && !linhas.iter().any(|linha| linha.contains("dentro do prazo"))
        {
            eprintln!(
                "PARCIAL: a resolução respondeu antes de o prazo zero ser conferido; o ramo do \
                 prazo não rodou nesta máquina"
            );
            return;
        }
        assert!(
            linhas.iter().any(|linha| linha.starts_with("INFO")
                && linha.contains("dentro do prazo")
                && linha.contains("nao-existe-mesmo.invalid")),
            "o prazo que venceu na resolução do nome deixou de dizer no rastro (`info`, o único \
             nível que o seele.log grava) que foi o prazo: um DNS mudo e um link malformado \
             voltam a parecer a mesma coisa. Rastro: {linhas:?}"
        );
        // E quanto a consulta levou, na linha do resultado: a medida de campo
        // do G1 lia esse tempo numa captura de pacotes, porque o `seele.log`
        // não o trazia.
        assert!(
            linhas.iter().any(|linha| linha.starts_with("INFO")
                && linha.contains("onde o anfitrião mora hoje")
                && linha.contains("levou=")),
            "a linha do resultado da consulta ao quarto não diz quanto ela levou (`levou=`): \
             quem mede uma conexão lenta volta a precisar de uma captura de pacotes para saber \
             quanto dela foi o quarto. Rastro: {linhas:?}"
        );
    }

    #[tokio::test]
    async fn uma_pergunta_que_nao_sai_volta_na_hora_e_diz_no_rastro_por_que_nao_saiu() {
        // O envio recusado pelo kernel era engolido num `debug!`: a consulta
        // esperava o prazo inteiro por uma resposta a uma pergunta que nunca
        // saiu, e o log não tinha uma palavra sobre isso.
        //
        // **E esperava mesmo.** Sem rota (`ENETUNREACH`, `EHOSTUNREACH`) nenhuma
        // das três perguntas sai, e não há resposta a aguardar: repetir a volta
        // e dormir até o prazo custava 1,5 s a toda conexão, antes de qualquer
        // tentativa. Uma pergunta acessória que atrasa a principal é pior que
        // não perguntar, então a consulta volta na hora.
        //
        // O destino é **descoberto**, como em `destino_recusado`: `0.0.0.0:0` é
        // recusado pelo macOS e pelo Windows (medidos) e, pela porta zero, pelo
        // Linux (não medido). Se esta máquina o aceitar, quem falha é o teste,
        // dizendo isso, e não a consulta acusada de engolir um erro que nunca
        // houve.
        let destino = SocketAddr::from(([0, 0, 0, 0], 0));
        let sonda = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
        assert!(
            sonda.send_to(&[0_u8; encontro::TAMANHO], destino).is_err(),
            "esta máquina aceitou um envio a {destino}: sem uma recusa, este teste não tem um \
             envio falho para observar. Não é a consulta que está errada, é o mecanismo daqui \
             que não vale neste sistema"
        );
        let captura = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::INFO);
        let _guarda = tracing::subscriber::set_default(captura.clone());
        let Some(marcas) = Marcas::do_servidor(FP) else {
            panic!("a impressão digital de teste tem de formar marcas");
        };

        // O prazo de produção, e não um curto: é ele que a consulta pagaria.
        let comecou = std::time::Instant::now();
        let ate = tokio::time::Instant::now() + PRAZO_DO_QUARTO;
        let achado = consultar(&[destino], &marcas, ate).await;
        let levou = comecou.elapsed();

        assert_eq!(
            achado,
            OndeMora::PontoMudo,
            "uma pergunta que nunca saiu virou outra coisa: este teste não mede nada"
        );
        assert!(
            levou < Duration::from_millis(500),
            "nenhuma pergunta saiu e a consulta ainda esperou {levou:?}: sem rota, quem paga é a \
             conexão que vem depois, com {PRAZO_DO_QUARTO:?} a mais antes de qualquer tentativa"
        );
        let linhas = captura.linhas();
        assert!(
            linhas
                .iter()
                .any(|linha| linha.starts_with("INFO") && linha.contains("não saiu")),
            "o envio recusado ao quarto deixou de dizer no rastro (`info`, o único nível que o \
             seele.log grava) por que a pergunta não saiu: a consulta espera o prazo inteiro e \
             ninguém fica sabendo o motivo. Rastro: {linhas:?}"
        );
    }

    #[test]
    fn a_consulta_passa_ao_endereco_seguinte_so_quando_nada_saiu_pelo_primeiro() {
        // O M2 do item 19 da pendência 51. Um nome com A e AAAA dá dois
        // endereços, e qual deles tem rota é propriedade desta máquina. Sem
        // rota para a família do primeiro, o sistema recusa as três perguntas
        // (ou o socket nem abre), e a consulta parava ali, como `PontoMudo`,
        // com o outro endereço do mesmo ponto intocado.
        for nada_saiu in [PorUmEndereco::SemSocket, PorUmEndereco::NadaSaiu] {
            assert_eq!(
                escolher(nada_saiu),
                Escolha::TentarOSeguinte,
                "{nada_saiu:?} acabou a consulta: nenhuma pergunta saiu por este endereço, e o \
                 endereço seguinte do mesmo ponto, que pode ter rota, nunca é tentado"
            );
        }
        // E um ponto que recebeu as perguntas não é perguntado de novo por outro
        // endereço: ele está no ar ou fora do ar pelos dois, e cada endereço a
        // mais custaria o prazo inteiro de novo.
        for perguntou in [
            OndeMora::PontoMudo,
            OndeMora::NinguemMora,
            OndeMora::Achado {
                servidor: Some(SocketAddr::from(SERVIDOR)),
                escuta: None,
            },
        ] {
            assert_eq!(
                escolher(PorUmEndereco::Perguntou(perguntou)),
                Escolha::Devolver(perguntou),
                "um endereço que recebeu as perguntas não acabou a consulta com o que deu \
                 ({perguntou:?})"
            );
        }
    }

    /// O que o ponto falso anotou: cada pedido que chegou, na ordem.
    type Anotados = Arc<std::sync::Mutex<Vec<encontro::Pedido>>>;

    /// Um ponto de encontro falso, no laço local. A cada pedido que chega,
    /// `responder` diz o que devolver a quem perguntou, cada datagrama com a
    /// demora dele, e o pedido fica anotado.
    async fn ponto_falso(
        responder: impl Fn(&encontro::Pedido, SocketAddr) -> Vec<(Duration, Vec<u8>)> + Send + 'static,
    ) -> (SocketAddr, Anotados) {
        let socket = Arc::new(tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let onde = socket.local_addr().unwrap();
        let anotados: Anotados = Arc::default();
        let anotador = Arc::clone(&anotados);
        tokio::spawn(async move {
            let mut balde = [0_u8; encontro::TAMANHO];
            while let Ok((lidos, de)) = socket.recv_from(&mut balde).await {
                let Some(pedido) = balde.get(..lidos).and_then(encontro::analisar) else {
                    continue;
                };
                for (demora, datagrama) in responder(&pedido, de) {
                    let socket = Arc::clone(&socket);
                    tokio::spawn(async move {
                        tokio::time::sleep(demora).await;
                        let _ = socket.send_to(&datagrama, de).await;
                    });
                }
                anotador.lock().unwrap().push(pedido);
            }
        });
        (onde, anotados)
    }

    #[tokio::test]
    async fn um_endereco_sem_rota_nao_esconde_o_seguinte_do_mesmo_ponto() {
        // O par da escolha, com a consulta de verdade: a função pura correta
        // que `consultar` não chamasse deixaria aquele teste verde e a
        // consulta parada no primeiro endereço. O primeiro é um destino que
        // esta máquina recusa (o de
        // `uma_pergunta_que_nao_sai_volta_na_hora_e_diz_no_rastro_por_que_nao_saiu`);
        // o segundo, um ponto falso que responde às duas marcas.
        let recusado = SocketAddr::from(([0, 0, 0, 0], 0));
        let sonda = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
        assert!(
            sonda.send_to(&[0_u8; encontro::TAMANHO], recusado).is_err(),
            "esta máquina aceitou um envio a {recusado}: sem uma recusa, o primeiro endereço \
             deste teste não é um sem rota, e o teste não mede nada"
        );
        let captura = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::INFO);
        let _guarda = tracing::subscriber::set_default(captura.clone());
        let Some(marcas) = Marcas::do_servidor(FP) else {
            panic!("a impressão digital de teste tem de formar marcas");
        };
        let (servidor, escuta) = (SocketAddr::from(SERVIDOR), SocketAddr::from(ESCUTA));
        let quem = marcas.clone();
        let (ponto, _) = ponto_falso(move |pedido, de| match pedido {
            encontro::Pedido::Quem { marca } if marca == quem.servidor() => {
                vec![(Duration::ZERO, encontro::aqui(marca, servidor))]
            }
            encontro::Pedido::Quem { marca } if marca == quem.escuta() => {
                vec![(Duration::ZERO, encontro::aqui(marca, escuta))]
            }
            encontro::Pedido::Onde { marca } => vec![(Duration::ZERO, encontro::aqui(marca, de))],
            _ => Vec::new(),
        })
        .await;

        let ate = tokio::time::Instant::now() + PRAZO_DO_QUARTO;
        let achado = consultar(&[recusado, ponto], &marcas, ate).await;

        assert_eq!(
            achado,
            OndeMora::Achado {
                servidor: Some(servidor),
                escuta: Some(escuta),
            },
            "nenhuma pergunta saiu pelo primeiro endereço do ponto, e a consulta não chegou ao \
             segundo, que responde: um nome com A e AAAA numa máquina sem rota para a família do \
             primeiro perde o quarto inteiro"
        );
        let linhas = captura.linhas();
        assert!(
            linhas.iter().any(|linha| linha.starts_with("INFO")
                && linha.contains("tenta o seguinte")
                && linha.contains(&recusado.to_string())
                && linha.contains(&ponto.to_string())),
            "a consulta passou ao endereço seguinte do ponto sem dizer no rastro (`info`, o \
             único nível que o seele.log grava) de qual para qual: quem lê o «não saiu» do \
             primeiro acha que a consulta parou ali. Rastro: {linhas:?}"
        );
    }

    #[test]
    fn uma_volta_recusada_depois_da_primeira_conta_como_volta() {
        // O T6f da pendência 51, item 17. Nenhuma pergunta sair na primeira
        // volta é não haver resposta que possa vir, e a consulta volta na hora
        // (`uma_pergunta_que_nao_sai_volta_na_hora_e_diz_no_rastro_por_que_nao_saiu`).
        // Numa volta 2 ou 3 não é: as perguntas da volta anterior saíram, e a
        // resposta a elas ainda pode chegar.
        assert_eq!(
            depois_da_volta(0, 0),
            DepoisDaVolta::NadaSaiu,
            "nenhuma pergunta saiu na primeira volta e a consulta não desistiu dela: sem rota, \
             a conexão paga o prazo inteiro por uma resposta que não tem como vir"
        );
        for enviados in 1..ENVIOS_AO_QUARTO {
            assert_eq!(
                depois_da_volta(0, enviados),
                DepoisDaVolta::Contar,
                "a volta {} não saiu, depois de {enviados} que saíram, e a consulta parou: a \
                 resposta tardia às perguntas que já saíram se perde, e a consulta volta como \
                 `PontoMudo`",
                enviados + 1
            );
        }
        for saidas in [1, 3] {
            for enviados in 0..ENVIOS_AO_QUARTO {
                assert_eq!(
                    depois_da_volta(saidas, enviados),
                    DepoisDaVolta::Contar,
                    "a volta {} saiu ({saidas} perguntas) e a consulta não a contou",
                    enviados + 1
                );
            }
        }
    }

    /// Um canal que deixa sair as primeiras perguntas e recusa as seguintes,
    /// como uma máquina que perde a rota no meio da consulta.
    struct RecusaDepois {
        socket: tokio::net::UdpSocket,
        /// Quantas perguntas ainda saem.
        deixa: std::sync::atomic::AtomicU32,
    }

    impl Canal for RecusaDepois {
        async fn mandar(&self, datagrama: &[u8], destino: SocketAddr) -> std::io::Result<usize> {
            let saiu = self.deixa.fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |restam| restam.checked_sub(1),
            );
            if saiu.is_err() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NetworkUnreachable,
                    "a rota caiu no meio da consulta",
                ));
            }
            self.socket.send_to(datagrama, destino).await
        }

        async fn receber(&self, balde: &mut [u8]) -> std::io::Result<(usize, SocketAddr)> {
            self.socket.recv_from(balde).await
        }
    }

    #[tokio::test]
    async fn uma_volta_recusada_depois_da_primeira_ainda_ouve_a_resposta_tardia() {
        // O par de `uma_volta_recusada_depois_da_primeira_conta_como_volta`,
        // com a consulta de verdade. A volta 1 sai; a 2, meio segundo depois,
        // o sistema recusa inteira; e a resposta à volta 1 chega aos 700 ms.
        // Parar na volta recusada perdia essa resposta, e a consulta voltava
        // como `PontoMudo` com o servidor no quarto.
        let Some(marcas) = Marcas::do_servidor(FP) else {
            panic!("a impressão digital de teste tem de formar marcas");
        };
        let servidor = SocketAddr::from(SERVIDOR);
        let quem = marcas.clone();
        let (ponto, anotados) = ponto_falso(move |pedido, _| match pedido {
            encontro::Pedido::Quem { marca } if marca == quem.servidor() => {
                vec![(Duration::from_millis(700), encontro::aqui(marca, servidor))]
            }
            _ => Vec::new(),
        })
        .await;
        let canal = RecusaDepois {
            socket: tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap(),
            deixa: std::sync::atomic::AtomicU32::new(3),
        };

        let ate = tokio::time::Instant::now() + PRAZO_DO_QUARTO;
        let achado = consultar_por(&canal, ponto, &marcas, ate).await;

        assert_eq!(
            anotados.lock().unwrap().len(),
            3,
            "o ponto não recebeu exatamente as três perguntas da volta 1: o canal deste teste não \
             recusou a volta 2, e o teste não mede o que diz medir"
        );
        assert_eq!(
            achado,
            PorUmEndereco::Perguntou(OndeMora::Achado {
                servidor: Some(servidor),
                escuta: None,
            }),
            "a volta 2 não saiu e a consulta perdeu a resposta tardia à volta 1: o servidor estava \
             no quarto e quem chega fica sem o endereço de hoje"
        );
    }

    #[tokio::test]
    async fn a_ida_e_volta_se_mede_da_ultima_volta_que_saiu() {
        // O outro lado de contar a volta recusada: as perguntas no ar são as
        // da volta 1, e a ida e volta do `ONDE` se mede dela. Medida da volta
        // 2, que não saiu, a resposta aos 550 ms viraria uma ida e volta de
        // 50 ms, o teto cairia no piso, aos 650 ms, e a escuta que chega aos
        // 825 ms se perderia antes da hora da volta seguinte (1000 ms). As
        // duas margens são de ~175 ms: até o teto errado, e até a volta
        // seguinte.
        let Some(marcas) = Marcas::do_servidor(FP) else {
            panic!("a impressão digital de teste tem de formar marcas");
        };
        let (servidor, escuta) = (SocketAddr::from(SERVIDOR), SocketAddr::from(ESCUTA));
        let quem = marcas.clone();
        let (ponto, _) = ponto_falso(move |pedido, de| match pedido {
            encontro::Pedido::Onde { marca } => {
                vec![(Duration::from_millis(550), encontro::aqui(marca, de))]
            }
            encontro::Pedido::Quem { marca } if marca == quem.servidor() => {
                vec![(Duration::from_millis(550), encontro::aqui(marca, servidor))]
            }
            encontro::Pedido::Quem { marca } if marca == quem.escuta() => {
                vec![(Duration::from_millis(825), encontro::aqui(marca, escuta))]
            }
            _ => Vec::new(),
        })
        .await;
        let canal = RecusaDepois {
            socket: tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap(),
            deixa: std::sync::atomic::AtomicU32::new(3),
        };

        let ate = tokio::time::Instant::now() + PRAZO_DO_QUARTO;
        let achado = consultar_por(&canal, ponto, &marcas, ate).await;

        assert_eq!(
            achado,
            PorUmEndereco::Perguntou(OndeMora::Achado {
                servidor: Some(servidor),
                escuta: Some(escuta),
            }),
            "a escuta respondeu antes da hora da volta seguinte e se perdeu: a ida e volta foi \
             medida da volta recusada, e não da que saiu, e o teto venceu cedo demais"
        );
    }

    #[test]
    fn o_teto_e_o_dobro_da_ida_e_volta_e_nunca_menos_que_a_folga() {
        // O C1 da pendência 51, item 17: os dois ramos do teto. Num caminho
        // curto, o dobro da ida e volta são microssegundos, e o piso dá ao
        // ponto tempo de responder à segunda pergunta da rajada. Num caminho
        // longo, o dobro manda.
        let primeira = tokio::time::Instant::now();
        assert_eq!(
            teto_depois_da_resposta(primeira, Duration::from_millis(1)),
            primeira + Duration::from_millis(100),
            "com 1 ms de ida e volta o teto não ficou no piso de 100 ms: num caminho curto a \
             consulta para antes de o ponto responder à segunda marca da rajada"
        );
        assert_eq!(
            teto_depois_da_resposta(primeira, Duration::from_millis(200)),
            primeira + Duration::from_millis(400),
            "com 200 ms de ida e volta o teto não ficou no dobro: num caminho longo a consulta \
             para antes de a segunda marca voltar, ou espera mais que duas idas e voltas por uma \
             pergunta que o ponto calou"
        );
    }

    /// O ponto falso que responde ao `ONDE` na hora, ao `QUEM` do servidor na
    /// hora e ao `QUEM` da escuta depois de `escuta_em`, se vier. Com
    /// `onde_volta` falso, o `ONDE` se perde.
    async fn ponto_que_demora_a_escuta(
        marcas: &Marcas,
        onde_volta: bool,
        escuta_em: Option<Duration>,
    ) -> (SocketAddr, Anotados) {
        let (servidor, escuta) = (SocketAddr::from(SERVIDOR), SocketAddr::from(ESCUTA));
        let quem = marcas.clone();
        ponto_falso(move |pedido, de| match pedido {
            encontro::Pedido::Onde { marca } if onde_volta => {
                vec![(Duration::ZERO, encontro::aqui(marca, de))]
            }
            encontro::Pedido::Quem { marca } if marca == quem.servidor() => {
                vec![(Duration::ZERO, encontro::aqui(marca, servidor))]
            }
            encontro::Pedido::Quem { marca } if marca == quem.escuta() => escuta_em
                .map(|demora| (demora, encontro::aqui(marca, escuta)))
                .into_iter()
                .collect(),
            _ => Vec::new(),
        })
        .await
    }

    #[tokio::test]
    async fn a_segunda_marca_que_chega_dentro_do_teto_entra_na_resposta() {
        // O C1: o ponto responde ao servidor e ao `ONDE` na hora, e à escuta 15
        // ms depois. No laço local a ida e volta são microssegundos, e o teto é
        // o piso de 100 ms: a escuta chega dentro dele.
        //
        // 15 ms, e não perto do piso: o teste roda numa thread só, e uma pausa
        // dela que atravessasse o instante da escuta e o do teto faria os dois
        // timers vencerem no mesmo tique, com a consulta saindo sem a escuta.
        // Sobram ~85 ms para isso, e sem o piso o teto cai em microssegundos,
        // antes dos 15 ms.
        let Some(marcas) = Marcas::do_servidor(FP) else {
            panic!("a impressão digital de teste tem de formar marcas");
        };
        let (ponto, _) =
            ponto_que_demora_a_escuta(&marcas, true, Some(Duration::from_millis(15))).await;
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();

        let ate = tokio::time::Instant::now() + PRAZO_DO_QUARTO;
        let achado = consultar_por(&socket, ponto, &marcas, ate).await;

        assert_eq!(
            achado,
            PorUmEndereco::Perguntou(OndeMora::Achado {
                servidor: Some(SocketAddr::from(SERVIDOR)),
                escuta: Some(SocketAddr::from(ESCUTA)),
            }),
            "a escuta respondeu 15 ms depois do servidor, dentro do teto de 100 ms, e ficou de \
             fora: a consulta parou na primeira marca, e quem chega manda o `LEVE` ao aviso \
             guardado em vez da escuta de hoje"
        );
    }

    #[tokio::test]
    async fn com_o_onde_perdido_a_espera_vai_ate_a_volta_seguinte_e_nao_repete() {
        // O C1, o ramo do `ONDE` perdido: sem ida e volta medida não há teto, e
        // a espera pela marca que falta vai até a hora da volta seguinte (meio
        // segundo depois do envio). A escuta que chega aos 300 ms entra; uma
        // que nunca chega custa essa meia volta, e a pergunta não se repete a
        // um ponto que está no ar.
        let Some(marcas) = Marcas::do_servidor(FP) else {
            panic!("a impressão digital de teste tem de formar marcas");
        };
        let (ponto, _) =
            ponto_que_demora_a_escuta(&marcas, false, Some(Duration::from_millis(300))).await;
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let ate = tokio::time::Instant::now() + PRAZO_DO_QUARTO;
        assert_eq!(
            consultar_por(&socket, ponto, &marcas, ate).await,
            PorUmEndereco::Perguntou(OndeMora::Achado {
                servidor: Some(SocketAddr::from(SERVIDOR)),
                escuta: Some(SocketAddr::from(ESCUTA)),
            }),
            "o `ONDE` se perdeu, e a escuta que respondeu aos 300 ms, antes da hora da volta \
             seguinte, ficou de fora: sem ida e volta medida, a consulta inventou um teto"
        );

        let (ponto, anotados) = ponto_que_demora_a_escuta(&marcas, false, None).await;
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let comecou = std::time::Instant::now();
        let ate = tokio::time::Instant::now() + PRAZO_DO_QUARTO;
        let achado = consultar_por(&socket, ponto, &marcas, ate).await;
        let levou = comecou.elapsed();
        assert_eq!(
            achado,
            PorUmEndereco::Perguntou(OndeMora::Achado {
                servidor: Some(SocketAddr::from(SERVIDOR)),
                escuta: None,
            }),
            "o ponto deu o servidor e calou a escuta, e a consulta não voltou com o que achou"
        );
        assert!(
            levou >= INTERVALO_DOS_ENVIOS - Duration::from_millis(50)
                && levou < INTERVALO_DOS_ENVIOS * 2,
            "sem o `ONDE`, a espera pela escuta devia ir até a hora da volta seguinte \
             ({INTERVALO_DOS_ENVIOS:?}) e parar ali, e levou {levou:?}"
        );
        assert_eq!(
            anotados.lock().unwrap().len(),
            3,
            "a pergunta se repetiu a um ponto que já tinha respondido: quem não respondeu teve um \
             intervalo inteiro, e a volta a mais só atrasa a conexão"
        );
    }

    fn bilhete(ponto: &str) -> Bilhete {
        Bilhete::novo(ponto, "45.33.32.156:41234").expect("bilhete de teste")
    }

    #[tokio::test]
    async fn sem_marcas_nao_se_chama_ponto_nenhum() {
        // Sem marcas o `CHAMA` chegaria com uma etiqueta que ninguém
        // registrou: rede gasta para produzir `NINGUEM`.
        assert!(Chamada::preparar(&bilhete("192.0.2.1:8384"), None)
            .await
            .is_none());
    }

    #[tokio::test]
    async fn um_ponto_que_nao_resolve_nao_segura_a_conexao() {
        let comecou = std::time::Instant::now();
        let chamada = Chamada::preparar(
            &bilhete("nao-existe-mesmo.invalid:8384"),
            Marcas::do_servidor(FP),
        )
        .await;
        assert!(chamada.is_none(), "um ponto que não resolve virou chamada");
        assert!(
            comecou.elapsed() <= PRAZO + Duration::from_millis(400),
            "resolver o ponto segurou a conexão além do prazo: {:?}",
            comecou.elapsed()
        );
    }

    /// Uma impressão digital de teste, com pelo menos 16 caracteres — é dela
    /// que `preparar` tira a marca do aviso.
    const IMPRESSAO_DE_TESTE: &str =
        "3cbcfb0212da738f89c156de86eb280adee30fd6b907523b898fedcb2b1de5b9";
    const FP: &str = IMPRESSAO_DE_TESTE;

    /// Um ponto de teste da versão 2: responde ao `ONDE` (se `fala_v1`) e a
    /// cada `QUEM2` com o que `responder` disser, ou cala.
    async fn ponto_v2(
        fala_v1: bool,
        responder: impl Fn(&Marca) -> Option<encontro::Resposta2> + Send + 'static,
    ) -> SocketAddr {
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let onde = socket.local_addr().unwrap();
        tokio::spawn(async move {
            let mut balde = [0_u8; encontro::TAMANHO2 + 1];
            while let Ok((lidos, de)) = socket.recv_from(&mut balde).await {
                let dados = balde.get(..lidos).unwrap_or_default();
                let resposta = if let Some(encontro::Pedido::Onde { marca }) =
                    encontro::analisar(dados)
                {
                    fala_v1.then(|| encontro::aqui(&marca, de))
                } else if let Some(encontro::Pedido2::Quem2 { marca }) = encontro::analisar2(dados)
                {
                    responder(&marca).map(|resposta| encontro::resposta2(&resposta))
                } else {
                    None
                };
                if let Some(resposta) = resposta {
                    let _ = socket.send_to(&resposta, de).await;
                }
            }
        });
        onde
    }

    fn marcas_da_sala() -> Marcas {
        Marcas::da_sala(&seele_proto::codigo::Codigo::da_chave(&[9; 32]))
    }

    #[tokio::test]
    async fn a_sala_achada_traz_o_servidor_a_escuta_e_as_dicas() {
        let marcas = marcas_da_sala();
        let (servidor, escuta) = (marcas.servidor().clone(), marcas.escuta().clone());
        let dica: SocketAddr = "192.168.0.10:8385".parse().unwrap();
        let ponto = ponto_v2(true, move |marca| {
            if *marca == servidor {
                Some(encontro::Resposta2::Aqui2 {
                    marca: marca.clone(),
                    endereco: "203.0.113.7:4001".parse().unwrap(),
                    dicas: vec![dica],
                })
            } else if *marca == escuta {
                Some(encontro::Resposta2::Aqui2 {
                    marca: marca.clone(),
                    endereco: "203.0.113.7:4002".parse().unwrap(),
                    dicas: Vec::new(),
                })
            } else {
                None
            }
        })
        .await;
        assert_eq!(
            localizar(&ponto.to_string(), &marcas, PRAZO_DO_QUARTO).await,
            Localizacao::Achada {
                servidor: Some("203.0.113.7:4001".parse().unwrap()),
                escuta: Some("203.0.113.7:4002".parse().unwrap()),
                dicas: vec![dica],
            }
        );
    }

    #[tokio::test]
    async fn duas_marcas_vazias_sao_ninguem_mora() {
        let ponto = ponto_v2(true, |marca| {
            Some(encontro::Resposta2::Ninguem {
                marca: marca.clone(),
            })
        })
        .await;
        assert_eq!(
            localizar(&ponto.to_string(), &marcas_da_sala(), PRAZO_DO_QUARTO).await,
            Localizacao::NinguemMora
        );
    }

    #[tokio::test]
    async fn um_ponto_que_so_fala_o_enc1_e_dito_como_tal_e_nao_como_ninguem_mora() {
        // O ADR 0047, um degrau depois: com o ponto de produção sem o ENC/2,
        // «ALICE NÃO ESTÁ DISPONÍVEL» seria mentira. O ONDE responde, e os
        // QUEM2 calam.
        let ponto = ponto_v2(true, |_| None).await;
        assert_eq!(
            localizar(&ponto.to_string(), &marcas_da_sala(), PRAZO_DO_QUARTO).await,
            Localizacao::Enc2Calado
        );
    }

    #[tokio::test]
    async fn o_ponto_que_recusa_por_taxa_e_dito() {
        let ponto = ponto_v2(true, |marca| {
            Some(encontro::Resposta2::Recusa {
                marca: marca.clone(),
                motivo: encontro::Motivo::Taxa,
                agora: 1,
            })
        })
        .await;
        assert_eq!(
            localizar(&ponto.to_string(), &marcas_da_sala(), PRAZO_DO_QUARTO).await,
            Localizacao::PontoRecusou
        );
    }

    #[tokio::test]
    async fn um_ponto_mudo_para_as_duas_versoes_e_ponto_mudo() {
        let ponto = ponto_v2(false, |_| None).await;
        assert_eq!(
            localizar(
                &ponto.to_string(),
                &marcas_da_sala(),
                Duration::from_millis(300)
            )
            .await,
            Localizacao::PontoMudo
        );
    }

    #[tokio::test]
    async fn um_ponto_que_nao_e_endereco_nao_resolve() {
        assert_eq!(
            localizar("tem espaço", &marcas_da_sala(), PRAZO_DO_QUARTO).await,
            Localizacao::PontoNaoResolve
        );
    }

    #[tokio::test]
    async fn uma_resposta_de_versao_1_nao_vira_achado_da_sala() {
        // Um AQUI da versão 1 com a marca da sala, de um ponto que não fala o
        // ENC/2, não é um endereço da sala: o QUEM2 é o único caminho.
        let marcas = marcas_da_sala();
        let servidor = marcas.servidor().clone();
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let ponto = socket.local_addr().unwrap();
        tokio::spawn(async move {
            let mut balde = [0_u8; encontro::TAMANHO2 + 1];
            while let Ok((lidos, de)) = socket.recv_from(&mut balde).await {
                let dados = balde.get(..lidos).unwrap_or_default();
                if let Some(encontro::Pedido::Onde { marca }) = encontro::analisar(dados) {
                    let _ = socket.send_to(&encontro::aqui(&marca, de), de).await;
                    let _ = socket
                        .send_to(
                            &encontro::aqui(&servidor, "203.0.113.66:1".parse().unwrap()),
                            de,
                        )
                        .await;
                }
            }
        });
        assert_eq!(
            localizar(&ponto.to_string(), &marcas, PRAZO_DO_QUARTO).await,
            Localizacao::Enc2Calado
        );
    }

    /// Um ponto que limita como o de verdade: responde ao `ONDE`, e a um
    /// `QUEM2` só com a `RECUSA taxa`, uma vez por segundo, calando o resto.
    /// A última recusa acabou de ser dita a um vizinho.
    ///
    /// **«Acabou de» é antes de a consulta começar, e é marcado aqui**, antes
    /// da tarefa nascer. Marcado dentro dela, o relógio do ponto começava
    /// quando o runtime de uma linha só a punha para rodar, que é depois de a
    /// consulta mandar a primeira volta. A terceira volta sai a 1 s do começo
    /// da consulta, e chegava ao ponto 1,1 ms depois do segundo dele, no
    /// pior de 60 rodadas: a folga era o atraso do timer, e um soluço de 10 ms
    /// no começo da tarefa a calava 5 vezes em 5 (`Enc2Calado` no lugar de
    /// `PontoRecusou`, a intermitente de 10/10). Marcado aqui, a terceira volta
    /// sai sempre mais de 1 s depois da última recusa, com soluço ou sem.
    async fn ponto_que_limita() -> SocketAddr {
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let onde = socket.local_addr().unwrap();
        let mut ultima_recusa = tokio::time::Instant::now();
        tokio::spawn(async move {
            let mut balde = [0_u8; encontro::TAMANHO2 + 1];
            while let Ok((lidos, de)) = socket.recv_from(&mut balde).await {
                let dados = balde.get(..lidos).unwrap_or_default();
                if let Some(encontro::Pedido::Onde { marca }) = encontro::analisar(dados) {
                    let _ = socket.send_to(&encontro::aqui(&marca, de), de).await;
                } else if let Some(encontro::Pedido2::Quem2 { marca }) = encontro::analisar2(dados)
                {
                    let agora = tokio::time::Instant::now();
                    if agora.duration_since(ultima_recusa) >= Duration::from_secs(1) {
                        ultima_recusa = agora;
                        let recusa = encontro::Resposta2::Recusa {
                            marca,
                            motivo: encontro::Motivo::Taxa,
                            agora: 1,
                        };
                        let _ = socket.send_to(&encontro::resposta2(&recusa), de).await;
                    }
                }
            }
        });
        onde
    }

    #[tokio::test]
    async fn quem_e_limitado_pela_taxa_ouve_que_o_ponto_recusou_e_nao_que_ele_e_velho() {
        // O achado da revisão de 08/10: acima do limite, o ponto cala tudo
        // menos uma recusa por segundo, o `ONDE` responde, e a consulta
        // concluía que o ponto era anterior ao ENC/2. Ela pergunta de novo
        // dentro do prazo, depois da janela da recusa.
        //
        // As marcas antes do ponto: entre a última recusa e a primeira volta
        // não fica nada além do socket da consulta. Com mais de 1 s ali, a
        // primeira volta já ouviria a recusa, e o teste passaria com uma
        // consulta que não pergunta de novo.
        //
        // **Com um prazo largo, e não o `PRAZO_DO_QUARTO`.** A pergunta aqui é
        // se a consulta pergunta de novo depois da janela da recusa. Que as
        // voltas cabem no prazo do produto é conta, e é conferida em tempo de
        // compilação, ao lado das constantes. Com o prazo do produto, uma linha
        // parada meio segundo perto da terceira volta reprovava este teste, e a
        // bateria inteira carrega a máquina por uma hora e meia.
        let marcas = marcas_da_sala();
        let ponto = ponto_que_limita().await;
        assert_eq!(
            localizar(&ponto.to_string(), &marcas, Duration::from_secs(30)).await,
            Localizacao::PontoRecusou
        );
    }

    #[tokio::test]
    async fn uma_marca_recusada_e_a_outra_calada_dizem_que_o_ponto_recusou() {
        // O caso comum do limite de verdade: a primeira pergunta da volta ouve
        // a recusa, e a segunda, dentro do mesmo segundo, é calada.
        let marcas = marcas_da_sala();
        let servidor = marcas.servidor().clone();
        let ponto = ponto_v2(true, move |marca| {
            (*marca == servidor).then(|| encontro::Resposta2::Recusa {
                marca: marca.clone(),
                motivo: encontro::Motivo::Taxa,
                agora: 1,
            })
        })
        .await;
        assert_eq!(
            localizar(&ponto.to_string(), &marcas, PRAZO_DO_QUARTO).await,
            Localizacao::PontoRecusou
        );
    }

    #[tokio::test]
    async fn o_rastro_da_consulta_nao_grava_a_marca_de_quem_se_procura() {
        // O `seele.log` vai em relatório de defeito. A marca da sala é o
        // código da pessoa procurada, e a do servidor guardado, o começo da
        // impressão dele: o rastro diz o que o ponto respondeu, e não quem se
        // procurava.
        let captura = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::INFO);
        let _guarda = tracing::subscriber::set_default(captura.clone());
        let mudo = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let onde_fica = mudo.local_addr().unwrap().to_string();
        let da_sala = marcas_da_sala();
        let Some(do_servidor) = Marcas::do_servidor(FP) else {
            panic!("a impressão digital de teste tem de formar marcas");
        };

        let _ = localizar(&onde_fica, &da_sala, Duration::from_millis(150)).await;
        let _ = onde_mora_hoje(&onde_fica, &do_servidor, Duration::from_millis(150)).await;

        let linhas = captura.linhas();
        assert!(
            linhas.iter().any(|linha| linha.contains("prazo")),
            "o prazo venceu e o rastro não disse: este teste não mede nada. Rastro: {linhas:?}"
        );
        for marcas in [&da_sala, &do_servidor] {
            for marca in [marcas.servidor(), marcas.escuta()] {
                assert!(
                    !linhas.iter().any(|linha| linha.contains(marca.texto())),
                    "o rastro gravou a marca {} de quem se procurava: {linhas:?}",
                    marca.texto()
                );
            }
        }
    }

    /// **O gêmeo do guarda da F5, do lado de quem liga.** Um IP escrito à mão,
    /// e não o nome: o `resolver_ponto` com um IP volta sem rede, e nenhum
    /// pacote sai daqui para a VPS em caso nenhum — o que reprova, desfeito o
    /// guarda, é a falta do pânico.
    #[tokio::test]
    #[should_panic(expected = "pego em encontro::resolver_ponto, antes do DNS")]
    async fn perguntar_ao_ip_da_vps_de_producao_reprova_antes_do_primeiro_pacote() {
        let _ = resolver_ponto("216.128.168.216:8384").await;
    }

    /// O mesmo, na batida: o `LEVE` sai para o ponto do bilhete.
    #[tokio::test]
    #[should_panic(expected = "pego em encontro::resolver, a batida, antes do DNS")]
    async fn bater_no_ip_da_vps_de_producao_reprova_antes_do_primeiro_pacote() {
        let Ok(bilhete) = Bilhete::novo("216.128.168.216:8384", "203.0.113.7:41234") else {
            panic!("o bilhete de teste não se montou");
        };
        let _ = resolver(&bilhete).await;
    }

    // -------------------------------------------------------------- ENC/3

    /// Um ponto do ENC/3 de mentira: a cada `CHAMA` responde o que `responder`
    /// disser (ou cala), e conta quantos chegaram e de onde.
    async fn ponto_do_chama(
        responder: impl Fn(&encontro::Chama, SocketAddr) -> Option<encontro::Resposta3> + Send + 'static,
    ) -> (
        SocketAddr,
        std::sync::Arc<std::sync::Mutex<Vec<(SocketAddr, encontro::Chama)>>>,
    ) {
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let onde = socket.local_addr().unwrap();
        let vistos = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let anotados = std::sync::Arc::clone(&vistos);
        tokio::spawn(async move {
            let mut balde = [0_u8; encontro::TAMANHO3 + 1];
            while let Ok((lidos, de)) = socket.recv_from(&mut balde).await {
                let Some(encontro::Pedido3::Chama(chama)) =
                    encontro::analisar3(balde.get(..lidos).unwrap_or_default())
                else {
                    continue;
                };
                anotados.lock().unwrap().push((de, chama.clone()));
                if let Some(resposta) = responder(&chama, de) {
                    let _ = socket.send_to(&encontro::resposta3(&resposta), de).await;
                }
            }
        });
        (onde, vistos)
    }

    /// A ponta de uma sessão em `127.0.0.1`, para o `CHAMA` sair dela.
    fn ponta_local() -> crate::transporte::PontaDaSessao {
        let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        crate::transporte::abrir_ponta(Some(socket)).unwrap()
    }

    fn chamada_para(ponto: SocketAddr) -> Chamada {
        Chamada {
            ponto,
            ponto_escrito: ponto.to_string(),
            marcas: Marcas::do_servidor(FP).unwrap(),
        }
    }

    #[tokio::test]
    async fn o_chama_atendido_da_o_endereco_e_a_credencial_que_foi_no_pedido() {
        // O furo é pela credencial: o anfitrião reconhece quem furou pelo `U`
        // que veio no `CHAMA`, e a discagem tem de sair com ele.
        let publico: SocketAddr = "203.0.113.9:8383".parse().unwrap();
        let (ponto, vistos) = ponto_do_chama(move |chama, _| {
            Some(encontro::Resposta3::Aqui {
                marca: chama.servidor.clone(),
                endereco: publico,
                dicas: vec!["192.168.0.9:8383".parse().unwrap()],
            })
        })
        .await;
        let ponta = ponta_local();
        let atendida = chamada_para(ponto)
            .chamar(ponta.ponta(), Duration::from_secs(2))
            .await
            .expect("o CHAMA atendido não deu endereço");
        assert_eq!(atendida.endereco, publico);
        assert_eq!(
            atendida.dicas,
            ["192.168.0.9:8383".parse::<SocketAddr>().unwrap()]
        );
        let vistos = vistos.lock().unwrap();
        let (de, chama) = vistos.first().expect("o ponto não viu CHAMA nenhum");
        assert_eq!(
            chama.credencial.texto(),
            atendida.credencial,
            "a discagem sairia com um U que o anfitrião não conhece"
        );
        assert_eq!(
            de.port(),
            ponta.ponta().local_addr().port(),
            "o CHAMA saiu de outra porta que a da sessão: o anfitrião furaria a porta errada"
        );
        assert_eq!(chama.servidor, *Marcas::do_servidor(FP).unwrap().servidor());
        assert_eq!(chama.escuta, *Marcas::do_servidor(FP).unwrap().escuta());
    }

    #[tokio::test]
    async fn ninguem_e_calado_tem_nome_proprio() {
        for (resposta, esperada) in [
            ("ninguem", FalhaDaChamada::Ninguem),
            ("calado", FalhaDaChamada::Calado),
        ] {
            let (ponto, _) = ponto_do_chama(move |chama, _| {
                Some(if resposta == "ninguem" {
                    encontro::Resposta3::Ninguem {
                        marca: chama.servidor.clone(),
                    }
                } else {
                    encontro::Resposta3::Calado {
                        marca: chama.servidor.clone(),
                    }
                })
            })
            .await;
            let ponta = ponta_local();
            assert_eq!(
                chamada_para(ponto)
                    .chamar(ponta.ponta(), Duration::from_secs(2))
                    .await,
                Err(esperada)
            );
        }
    }

    #[tokio::test]
    async fn um_ponto_mudo_recebe_o_chama_repetido_e_a_chamada_diz_que_ele_calou() {
        // UDP perde pacote: um CHAMA perdido custava a chamada inteira em
        // silêncio. Três saem, espaçados, e o fim tem nome.
        let (ponto, vistos) = ponto_do_chama(|_, _| None).await;
        let ponta = ponta_local();
        let comecou = std::time::Instant::now();
        let falha = chamada_para(ponto)
            .chamar(ponta.ponta(), Duration::from_secs(2))
            .await;
        assert_eq!(falha, Err(FalhaDaChamada::Mudo));
        assert!(
            comecou.elapsed() >= Duration::from_secs(2),
            "desistiu antes do prazo"
        );
        assert!(
            comecou.elapsed() < Duration::from_secs(4),
            "passou do prazo"
        );
        assert_eq!(
            vistos.lock().unwrap().len(),
            CHAMADAS as usize,
            "o CHAMA não se repetiu enquanto o ponto calava"
        );
    }

    #[tokio::test]
    async fn um_aqui_que_nao_veio_do_ponto_nao_vale() {
        // Um terceiro que mande um `AQUI` à porta da sessão não escolhe para
        // onde ela disca.
        let (ponto, _) = ponto_do_chama(|_, _| None).await;
        let ponta = ponta_local();
        let alvo = ponta.ponta().local_addr();
        let terceiro = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let forjado = encontro::resposta3(&encontro::Resposta3::Aqui {
            marca: Marcas::do_servidor(FP).unwrap().servidor().clone(),
            endereco: "198.51.100.66:8383".parse().unwrap(),
            dicas: Vec::new(),
        });
        let mandando = tokio::spawn(async move {
            for _ in 0..10 {
                let _ = terceiro.send_to(&forjado, alvo).await;
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        });
        let falha = chamada_para(ponto)
            .chamar(ponta.ponta(), Duration::from_millis(1200))
            .await;
        mandando.abort();
        assert_eq!(
            falha,
            Err(FalhaDaChamada::Mudo),
            "um AQUI de quem não é o ponto virou destino da discagem"
        );
    }

    /// Um ponto que responde o `ONDE` da versão 1 e, para cada marca, o que
    /// `v3` e `v2` disserem.
    async fn ponto_de_duas_linhas(
        v3: impl Fn(&Marca) -> Option<encontro::Resposta3> + Send + 'static,
        v2: impl Fn(&Marca) -> Option<encontro::Resposta2> + Send + 'static,
    ) -> SocketAddr {
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let onde = socket.local_addr().unwrap();
        tokio::spawn(async move {
            let mut balde = [0_u8; encontro::TAMANHO3 + 1];
            while let Ok((lidos, de)) = socket.recv_from(&mut balde).await {
                let dados = balde.get(..lidos).unwrap_or_default();
                let resposta = if let Some(encontro::Pedido::Onde { marca }) =
                    encontro::analisar(dados)
                {
                    Some(encontro::aqui(&marca, de))
                } else if let Some(encontro::Pedido3::Quem3 { marca }) = encontro::analisar3(dados)
                {
                    v3(&marca).map(|resposta| encontro::resposta3(&resposta))
                } else if let Some(encontro::Pedido2::Quem2 { marca }) = encontro::analisar2(dados)
                {
                    v2(&marca).map(|resposta| encontro::resposta2(&resposta))
                } else {
                    None
                };
                if let Some(resposta) = resposta {
                    let _ = socket.send_to(&resposta, de).await;
                }
            }
        });
        onde
    }

    #[tokio::test]
    async fn quem_so_e_achado_pelo_enc2_esta_na_linha_um() {
        // R6: uma pessoa que ficou na 0.16 registra pelo MORO2. Só o QUEM3
        // diria «ninguém mora», e a tela mandaria esperar quem está no ar na
        // outra linha.
        let marcas = Marcas::do_servidor(FP).unwrap();
        let ponto = ponto_de_duas_linhas(
            |marca| {
                Some(encontro::Resposta3::Ninguem {
                    marca: marca.clone(),
                })
            },
            |marca| {
                Some(encontro::Resposta2::Aqui2 {
                    marca: marca.clone(),
                    endereco: "203.0.113.5:8383".parse().unwrap(),
                    dicas: Vec::new(),
                })
            },
        )
        .await;
        assert_eq!(
            localizar_com_a_linha(&ponto.to_string(), &marcas, Duration::from_millis(1500)).await,
            LocalizacaoComLinha::NaLinhaUm
        );
    }

    #[tokio::test]
    async fn quem_e_achado_pelo_enc3_e_da_linha_dois_mesmo_com_o_enc2_respondendo() {
        let marcas = Marcas::do_servidor(FP).unwrap();
        let servidor: SocketAddr = "203.0.113.7:8383".parse().unwrap();
        let ponto = ponto_de_duas_linhas(
            move |marca| {
                Some(encontro::Resposta3::Aqui {
                    marca: marca.clone(),
                    endereco: servidor,
                    dicas: Vec::new(),
                })
            },
            |marca| {
                Some(encontro::Resposta2::Ninguem {
                    marca: marca.clone(),
                })
            },
        )
        .await;
        let achado =
            localizar_com_a_linha(&ponto.to_string(), &marcas, Duration::from_millis(1500)).await;
        let LocalizacaoComLinha::Achada(Localizacao::Achada {
            servidor: Some(achado),
            ..
        }) = achado
        else {
            panic!("o anfitrião da linha 2 não foi achado: {achado:?}");
        };
        assert_eq!(achado, servidor);
    }

    // ------------------------------------------- o candidato IPv6 do CHAMA

    fn v6(texto: &str) -> Ipv6Addr {
        texto.parse().expect("IPv6 de teste inválido")
    }

    fn de(texto: &str) -> SocketAddr {
        texto.parse().expect("endereço de teste inválido")
    }

    /// A ponta de uma sessão como a de produção: `[::]` de pilha dupla.
    /// `None` numa máquina sem IPv6 nenhum, onde o recuo é só IPv4.
    fn ponta_de_pilha_dupla() -> Option<crate::transporte::PontaDaSessao> {
        let ponta = crate::transporte::abrir_ponta(None).unwrap();
        ponta.ponta().local_addr().is_ipv6().then_some(ponta)
    }

    #[test]
    fn so_um_ipv6_global_e_global() {
        for global in [
            // Uma casa na Claro, uma na Vivo, e um endereço temporário qualquer.
            "2804:14c:5bb1:8a3f:1c2d:3e4f:5a6b:7c8d",
            "2804:7f0:b0c0:1234::10",
            "2a02:8108:1:2:3:4:5:6",
            "2600:1f18::1",
            "2001:4860:4860::8888",
            // O primeiro bloco depois dos usos especiais (`2001::/23`).
            "2001:200::1",
            // O fim de `2000::/3`.
            "3ffe:ffff::1",
        ] {
            assert!(e_ipv6_global(v6(global)), "{global} devia ser global");
        }
        for (fora, por_que) in [
            ("::1", "loopback"),
            ("::", "sem destino"),
            ("fe80::1c2d:3e4f:5a6b:7c8d", "link-local: não sai do cabo"),
            ("febf::1", "o fim de fe80::/10"),
            ("fc00::1", "local único (fc00::/7)"),
            ("fd12:3456:789a::1", "local único, a rede de casa"),
            ("ff02::1", "multicast"),
            ("::ffff:203.0.113.7", "IPv4 mapeado"),
            ("::ffff:192.168.0.2", "IPv4 mapeado, e privado"),
            ("64:ff9b::cb00:7107", "NAT64"),
            ("2001:db8::1", "documentação (RFC 3849)"),
            ("3fff:0abc::1", "documentação (RFC 9637)"),
            ("2001:0:4136:e378::1", "Teredo (2001::/32)"),
            ("2001:20::1", "ORCHIDv2, um uso especial de 2001::/23"),
            ("2002:cb00:7107::1", "6to4"),
            ("4000::1", "fora de 2000::/3"),
            ("100::1", "o prefixo de descarte"),
        ] {
            assert!(
                !e_ipv6_global(v6(fora)),
                "{fora} passou por global, e é {por_que}"
            );
        }
        // A fronteira do 3fff::/20: o que vem depois dele é espaço global.
        assert!(e_ipv6_global(v6("3fff:1000::1")));
    }

    #[test]
    fn o_candidato_e_a_origem_global_que_o_sistema_escolhe_na_porta_da_sessao() {
        let local = de("[::]:50123");
        let global = v6("2804:14c:5bb1:8a3f:1c2d:3e4f:5a6b:7c8d");

        // O caso comum: nenhum IPv6 do anfitrião no convite, e a sonda geral
        // diz por onde esta máquina sai.
        let candidatos = candidatos_ipv6(local, &[], |alvo| {
            assert_eq!(alvo, alvo_da_sonda(), "a sonda foi a outro lugar");
            Some(global)
        });
        assert_eq!(
            candidatos,
            vec![SocketAddr::from((global, 50123))],
            "o IPv6 global desta máquina não virou candidato, ou saiu com outra porta \
             que a da sessão: o anfitrião furaria a porta errada"
        );

        // Nenhum endereço que não é global entra, um a um.
        for fora in [
            "::1",
            "::",
            "fe80::1",
            "fd00::1",
            "fc00::1",
            "ff02::1",
            "::ffff:203.0.113.7",
            "2001:db8::7",
            "2002:cb00:7107::1",
            "2001:0:4136:e378::1",
        ] {
            assert_eq!(
                candidatos_ipv6(local, &[], |_| Some(v6(fora))),
                Vec::<SocketAddr>::new(),
                "{fora} não é global e foi anunciado como candidato"
            );
        }

        // Uma máquina sem rota IPv6 não tem origem, e não anuncia nada.
        assert!(candidatos_ipv6(local, &[], |_| None).is_empty());
    }

    #[test]
    fn a_origem_e_perguntada_para_os_ipv6_do_anfitriao_que_o_convite_trouxe() {
        // Duas saídas: para o IPv6 do anfitrião o sistema sai por um endereço,
        // e para o resto da internet por outro (uma VPN, dois prefixos). O
        // candidato que serve é o do caminho até o anfitrião, e ele vai na
        // frente; o IPv4 e o IPv4 mapeado do convite não viram sonda.
        let local = de("[::]:50123");
        let anfitriao = de("[2804:7f0:b0c0:1234::10]:8383");
        let ate_ele = v6("2804:14c:5bb1:8a3f::aaaa");
        let geral = v6("2a02:8108:1:2::bbbb");
        let perguntados = std::sync::Mutex::new(Vec::new());
        let candidatos = candidatos_ipv6(
            local,
            &[
                de("203.0.113.7:8383"),
                de("[::ffff:203.0.113.7]:8383"),
                anfitriao,
            ],
            |alvo| {
                perguntados.lock().unwrap().push(alvo);
                Some(if alvo == anfitriao { ate_ele } else { geral })
            },
        );
        assert_eq!(
            *perguntados.lock().unwrap(),
            vec![anfitriao, alvo_da_sonda()],
            "a origem foi perguntada para um endereço que não é IPv6 do anfitrião"
        );
        assert_eq!(
            candidatos,
            vec![
                SocketAddr::from((ate_ele, 50123)),
                SocketAddr::from((geral, 50123))
            ]
        );

        // A mesma origem para os dois não vai duas vezes (o ponto recusa um
        // `CHAMA` com candidato repetido, e a chamada inteira calaria).
        assert_eq!(
            candidatos_ipv6(local, &[anfitriao], |_| Some(ate_ele)),
            vec![SocketAddr::from((ate_ele, 50123))]
        );

        // E nunca mais que o teto do fio, mesmo com um convite cheio de IPv6.
        let muitos: Vec<SocketAddr> = (1..=6_u16)
            .map(|n| SocketAddr::from((Ipv6Addr::new(0x2804, 0x7f0, n, 0, 0, 0, 0, 1), 8383)))
            .collect();
        let sondas = std::sync::atomic::AtomicUsize::new(0);
        let candidatos = candidatos_ipv6(local, &muitos, |alvo| {
            let n = sondas.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let SocketAddr::V6(alvo) = alvo else {
                return None;
            };
            // Uma origem diferente por alvo.
            Some(Ipv6Addr::new(
                0x2804,
                0x14c,
                alvo.ip().segments()[2],
                0,
                0,
                0,
                0,
                u16::try_from(n).unwrap() + 1,
            ))
        });
        assert_eq!(candidatos.len(), encontro::LIMITE_DE_CANDIDATOS);
        assert_eq!(
            sondas.load(std::sync::atomic::Ordering::SeqCst),
            SONDAS_DE_ORIGEM + 1,
            "cada IPv6 de um convite comprido abriu uma sonda"
        );
    }

    #[test]
    fn uma_ponta_que_nao_fala_ipv6_ou_de_porta_baixa_nao_anuncia_nada() {
        let global = v6("2804:14c:5bb1:8a3f::aaaa");
        // O recuo de `abrir_ponta`: um socket só IPv4 não receberia a resposta.
        assert!(candidatos_ipv6(de("0.0.0.0:50123"), &[], |_| Some(global)).is_empty());
        assert!(candidatos_ipv6(de("127.0.0.1:50123"), &[], |_| Some(global)).is_empty());
        // Porta abaixo de 1024: o ponto recusaria o `CHAMA` inteiro por ela.
        assert!(candidatos_ipv6(de("[::]:443"), &[], |_| Some(global)).is_empty());
        // Uma ponta ligada a um endereço só sai por ele: é ele o candidato, se
        // for global, e a sonda nem é feita.
        let ligada = SocketAddr::from((global, 50123));
        assert_eq!(
            candidatos_ipv6(ligada, &[], |_| panic!("sondou com a ponta ligada")),
            vec![ligada]
        );
        assert!(candidatos_ipv6(de("[::1]:50123"), &[], |_| Some(global)).is_empty());
    }

    #[test]
    fn o_que_nao_cabe_no_fio_nao_vai_no_chama() {
        // O ponto lê os candidatos com «um só que não sirva recusa todos»
        // (`ler_enderecos`): cada um destes, sozinho, calaria a chamada.
        let bom = de("[2804:14c:5bb1:8a3f::aaaa]:50123");
        let outro = de("[2804:14c:5bb1:8a3f::bbbb]:50123");
        let cabem = caber_no_chama(vec![
            de("[2804:14c::1]:80"),
            de("[::]:50123"),
            de("[ff02::1]:50123"),
            bom,
            bom,
            outro,
            de("[2804:14c:5bb1:8a3f::cccc]:50123"),
        ]);
        assert_eq!(cabem, vec![bom, outro]);
        // E o que sobra é um `CHAMA` que o leitor do ponto lê.
        let marcas = Marcas::do_servidor(FP).unwrap();
        let datagrama = encontro::chama(&encontro::Chama {
            servidor: marcas.servidor().clone(),
            escuta: marcas.escuta().clone(),
            credencial: encontro::Credencial::nova(&"a".repeat(32)).unwrap(),
            candidatos: cabem.clone(),
        });
        let Some(encontro::Pedido3::Chama(lido)) = encontro::analisar3(&datagrama) else {
            panic!("o ponto não leria este CHAMA");
        };
        assert_eq!(lido.candidatos, cabem);
    }

    /// Um ponto que atende todo `CHAMA` com `AQUI`, como um anfitrião que
    /// responde.
    fn atende_sempre(chama: &encontro::Chama, _: SocketAddr) -> Option<encontro::Resposta3> {
        Some(encontro::Resposta3::Aqui {
            marca: chama.servidor.clone(),
            endereco: "203.0.113.9:8383".parse().unwrap(),
            dicas: Vec::new(),
        })
    }

    #[tokio::test]
    async fn a_segunda_chamada_leva_o_candidato_ipv6_com_um_u_novo_e_o_log_nao_o_escreve() {
        // De ponta a ponta até o ponto: o que a segunda chamada monta chega no
        // datagrama, o leitor do ponto (`analisar3`) o lê, e ele vem da porta
        // da sessão. Os endereços são ditos pela costura, porque numa máquina
        // de teste o IPv6 global é o que ela tiver, ou nenhum.
        let captura = crate::rastro_de_teste::Rastro::a_partir_de(tracing::Level::INFO);
        let _guarda = tracing::subscriber::set_default(Arc::clone(&captura));
        let (ponto, vistos) = ponto_do_chama(atende_sempre).await;
        let ponta = ponta_local();
        let porta = ponta.ponta().local_addr().port();
        let global = SocketAddr::from((v6("2804:14c:5bb1:8a3f:1c2d:3e4f:5a6b:7c8d"), porta));
        let _ditos = costura_de_teste::anunciar_a_este_ponto(
            ponto,
            // O repetido e o de porta baixa não cabem no fio, e ficam.
            vec![global, global, de("[2804:14c::2]:80")],
        );
        let chamada = chamada_para(ponto);
        let primeira = chamada
            .chamar(ponta.ponta(), Duration::from_secs(2))
            .await
            .expect("o CHAMA atendido não deu endereço");
        // Pela mesma ponta, de novo: a primeira chamada devolveu os datagramas
        // do ponto ao acabar.
        let segunda = chamada
            .chamar_pelo_ipv6(ponta.ponta(), Duration::from_secs(2), &[])
            .await
            .expect("com IPv6 global para dizer, a segunda chamada não saiu")
            .expect(
                "a segunda chamada não foi atendida: a ponta não devolveu os datagramas do ponto",
            );

        let vistos = vistos.lock().unwrap();
        let [(de_onde, primeiro), (do_segundo, segundo)] = vistos.as_slice() else {
            panic!("o ponto leu {} CHAMA, e eram dois", vistos.len());
        };
        assert!(
            primeiro.candidatos.is_empty(),
            "o primeiro CHAMA passou a levar candidato: o anfitrião tomaria o IPv6 como a \
             origem do par, e o caminho IPv4 de sempre mudaria"
        );
        assert_eq!(
            segundo.candidatos,
            vec![global],
            "o candidato IPv6 de quem chama não foi no CHAMA da segunda chamada: o anfitrião \
             fura só o endereço que o ponto viu, numa família"
        );
        assert_ne!(
            segundo.credencial, primeiro.credencial,
            "a segunda chamada reusou o U da primeira, cuja discagem já acabou"
        );
        assert_eq!(primeiro.credencial.texto(), primeira.credencial);
        assert_eq!(
            segundo.credencial.texto(),
            segunda.credencial,
            "a discagem sairia com um U que o anfitrião não conhece"
        );
        assert_eq!(
            (de_onde.port(), do_segundo.port()),
            (porta, porta),
            "um dos CHAMA saiu de outra porta que a da sessão"
        );

        // O endereço IPv6 desta máquina não vai ao `seele.log` (que grava
        // `info`): o log diz quantos candidatos foram, e não quais.
        let linhas = captura.linhas();
        assert!(
            linhas
                .iter()
                .any(|linha| linha.contains("candidatos_ipv6=1")),
            "o log não disse quantos candidatos foram: {linhas:?}"
        );
        assert!(
            !linhas.iter().any(|linha| linha.contains("2804:14c")),
            "o log gravou o endereço IPv6 desta máquina: {linhas:?}"
        );
    }

    #[tokio::test]
    async fn sem_ipv6_global_a_segunda_chamada_nao_manda_nada() {
        // Quem não tem IPv6 global não chama de novo, e nada muda para ele:
        // nem um pacote a mais ao ponto, nem um furo a mais no anfitrião.
        let (ponto, vistos) = ponto_do_chama(atende_sempre).await;
        let ponta = ponta_local();
        let _ditos = costura_de_teste::anunciar_a_este_ponto(ponto, Vec::new());
        let chamada = chamada_para(ponto);
        chamada
            .chamar(ponta.ponta(), Duration::from_secs(2))
            .await
            .expect("o CHAMA atendido não deu endereço");
        assert!(
            chamada
                .chamar_pelo_ipv6(ponta.ponta(), Duration::from_secs(2), &[])
                .await
                .is_none(),
            "sem candidato nenhum, a segunda chamada saiu"
        );
        tokio::time::sleep(Duration::from_millis(300)).await;
        let vistos = vistos.lock().unwrap();
        assert_eq!(vistos.len(), 1, "saiu um CHAMA a mais sem candidato nenhum");
        assert!(vistos.first().unwrap().1.candidatos.is_empty());
    }

    #[tokio::test]
    async fn um_aqui_atrasado_da_primeira_chamada_nao_responde_a_segunda() {
        // O `AQUI` não diz a que `CHAMA` responde. O ponto responde a cada
        // repetição do primeiro, e um `AQUI` que sobrou na fila passaria por
        // resposta à segunda chamada: a discagem sairia sem o anfitrião ter
        // sido chamado de novo, e sem furo IPv6 nenhum.
        //
        // Este ponto responde duas vezes ao `CHAMA` sem candidato, e cala para
        // o que leva um.
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let ponto = socket.local_addr().unwrap();
        tokio::spawn(async move {
            let mut balde = [0_u8; encontro::TAMANHO3 + 1];
            while let Ok((lidos, de)) = socket.recv_from(&mut balde).await {
                let Some(encontro::Pedido3::Chama(chama)) =
                    encontro::analisar3(balde.get(..lidos).unwrap_or_default())
                else {
                    continue;
                };
                if !chama.candidatos.is_empty() {
                    continue;
                }
                let aqui = encontro::resposta3(&atende_sempre(&chama, de).unwrap());
                for _ in 0..2 {
                    let _ = socket.send_to(&aqui, de).await;
                }
            }
        });
        let ponta = ponta_local();
        let global = SocketAddr::from((
            v6("2804:14c:5bb1:8a3f::aaaa"),
            ponta.ponta().local_addr().port(),
        ));
        let _ditos = costura_de_teste::anunciar_a_este_ponto(ponto, vec![global]);
        let chamada = chamada_para(ponto);
        chamada
            .chamar(ponta.ponta(), Duration::from_secs(2))
            .await
            .expect("o CHAMA atendido não deu endereço");
        // O segundo `AQUI` chega à fila da ponta.
        tokio::time::sleep(Duration::from_millis(200)).await;
        let segunda = chamada
            .chamar_pelo_ipv6(ponta.ponta(), Duration::from_millis(1200), &[])
            .await;
        assert_eq!(
            segunda,
            Some(Err(FalhaDaChamada::Mudo)),
            "um AQUI que sobrou da primeira chamada passou por resposta à segunda"
        );
    }

    #[tokio::test]
    async fn a_segunda_chamada_de_verdade_so_anuncia_o_ipv6_global_desta_maquina() {
        // Sem costura: o que o produto anuncia **nesta** máquina. Depende da
        // rede dela, e diz o que mediu: com IPv6 global, um candidato, que é a
        // origem que o sistema escolhe, na porta da sessão; sem, nenhum, e
        // nenhuma segunda chamada. Em nenhum caso um endereço que não é global.
        let Some(ponta) = ponta_de_pilha_dupla() else {
            eprintln!("pulado: esta máquina não abre socket IPv6");
            return;
        };
        let (ponto, vistos) = ponto_do_chama(atende_sempre).await;
        let esperado: Vec<SocketAddr> = origem_ipv6_para(alvo_da_sonda())
            .filter(|origem| e_ipv6_global(*origem))
            .map(|origem| SocketAddr::from((origem, ponta.ponta().local_addr().port())))
            .into_iter()
            .collect();
        eprintln!(
            "esta máquina anuncia {} candidato(s) IPv6 global(is) na segunda chamada",
            esperado.len()
        );
        let segunda = chamada_para(ponto)
            .chamar_pelo_ipv6(ponta.ponta(), Duration::from_secs(2), &[])
            .await;
        assert_eq!(
            segunda.is_some(),
            !esperado.is_empty(),
            "a segunda chamada saiu sem IPv6 global para dizer, ou não saiu tendo um"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
        let anunciados: Vec<SocketAddr> = vistos
            .lock()
            .unwrap()
            .iter()
            .flat_map(|(_, chama)| chama.candidatos.clone())
            .collect();
        assert_eq!(
            anunciados, esperado,
            "o que foi ao ponto não é a origem IPv6 global que o sistema escolhe"
        );
        for candidato in &anunciados {
            let IpAddr::V6(ip) = candidato.ip() else {
                panic!("um candidato IPv4 foi anunciado: o ponto já o vê");
            };
            assert!(e_ipv6_global(ip), "anunciou um endereço que não é global");
        }
    }

    #[test]
    fn quem_liga_por_codigo_usa_o_ponto_padrao_sem_a_variavel() {
        // O caminho de produção de quem liga, como o `com_o_ponto_padrao` de
        // quem hospeda: sem a variável, o ponto do projeto. A leitura é a
        // única do `seele-proto`.
        assert_eq!(
            encontro::ponto_escolhido(None, Some(PONTO_PADRAO)).as_deref(),
            Some("rendzvous.seele.sh")
        );
    }
}
