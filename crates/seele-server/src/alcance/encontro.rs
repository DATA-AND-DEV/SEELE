//! Degrau 4 do ADR 0022: furo de NAT com ponto de encontro.
//!
//! É o degrau que faz "manda o link e funciona" virar verdade numa casa com
//! CGNAT ou com o UPnP desligado — onde os degraus 2 e 3 não têm o que fazer e a
//! escada parava no 1.
//!
//! # O que acontece, na ordem
//!
//! 1. O Dogma abre uma **escuta de avisos** própria, num socket dele, e pergunta
//!    ao ponto de encontro qual é o endereço público dela (`ONDE`). Esse
//!    endereço é metade do bilhete que vai no `seele://`.
//! 2. Pelo socket **do Dogma** — o mesmo que o QUIC usa —, manda um `LEVE`
//!    apontando para aquela escuta de avisos. O ponto de encontro responde para
//!    lá dizendo de onde aquele pacote veio: é o endereço público do Dogma, e é
//!    ele que entra no convite como mais um candidato.
//! 3. Quem recebe o convite manda o próprio `LEVE` para a escuta de avisos, pelo
//!    socket com que vai conectar. Chega aqui um aviso com o endereço dele.
//! 4. O Dogma manda alguns pacotes para aquele endereço, **pelo socket do
//!    Dogma**. O roteador daqui passa a ter uma saída registrada para lá, e o
//!    aperto de mão QUIC que vem em seguida entra.
//!
//! # Por que tem de ser o socket do Dogma, e não outro
//!
//! Porque o NAT mapeia por porta interna. Um pacote saindo de outro socket abre
//! caminho para *aquele* socket, e o QUIC continua batendo numa porta fechada. É
//! o mesmo motivo pelo qual o furo tem de sair daqui e não da escuta de avisos:
//! a escuta de avisos existe **só** porque este processo não pode ler do socket
//! do Dogma — quem lê dele é o quinn.
//!
//! # O que isto não faz, e não vai fazer
//!
//! **NAT simétrico dos dois lados não fura.** Nesse caso o mapeamento muda a
//! cada destino, então o endereço que o ponto de encontro viu não é o endereço
//! por onde o outro lado chegaria. A resposta a isso seria retransmissão, que o
//! ADR 0022 deixou **fora de escopo por decisão** — e por isso a frase do degrau
//! 4 diz "deve funcionar" e não "funciona", e a escada continua caindo para os
//! degraus de baixo.
//!
//! **Nada do que é dito passa por aqui.** O ponto de encontro apresenta e sai; o
//! TLS 1.3 e o TOFU do ADR 0003 continuam ponta a ponta, e a impressão digital
//! continua sendo conferida contra a do Dogma. O que ele aprende é metadado —
//! que endereço falou com que endereço, e quando —, e isso está escrito em
//! `docs/alcance-pela-internet.md` em vez de ficar implícito.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use seele_proto::encontro::{self, Marca};
use seele_proto::uri::Bilhete;

/// O ponto de encontro do projeto, quando ninguém disser outro.
///
/// O ADR 0022 decidiu construir **com um ponto de encontro nosso por padrão**, e
/// é isto. Ele é trocável por `$SEELE_ENCONTRO`, e o endereço de quem hospeda
/// viaja dentro do próprio convite — ver [`Bilhete`].
///
/// Um **nome** e não um endereço, porque isto está compilado dentro de cada
/// executável do mundo: com um nome, trocar de VPS é um registro de DNS; com um
/// IP, seria uma versão nova e todo mundo reinstalando.
///
/// E como DNS é mais uma coisa que pode estar ruim num dado dia, há uma rede
/// embaixo — ver [`REDE_DO_PADRAO`]. Quando as duas falham, a escada cai para o
/// degrau de baixo e a frase que a pessoa lê é a mesma de antes deste degrau
/// existir. Quem quiser o seu próprio sobe em dez linhas —
/// `docs/ponto-de-encontro.md`.
pub const PONTO_PADRAO: &str = "encontro.seele.app.br";

/// Os endereços do [`PONTO_PADRAO`], para quando o nome não resolver.
///
/// # Por que existir, e por que só para o nosso
///
/// O nome é o caminho principal porque ele é o que torna o servidor trocável:
/// ele está compilado dentro de **cada executável do mundo**, e um IP gravado
/// ali significaria que mudar de VPS quebra todo mundo até a próxima versão.
///
/// Mas DNS é mais uma coisa que pode estar ruim num dado dia — resolvedor do
/// provedor caído, rede que sequestra consulta, zona em carência depois de uma
/// mudança. Nesses casos o degrau 4 sumiria por um motivo que não tem nada a
/// ver com ele.
///
/// **Isto vale só para o endereço padrão**, e a exceção é a parte importante: se
/// alguém apontou `$SEELE_ENCONTRO` para o ponto de encontro dela e o nome não
/// resolve, cair no nosso mandaria o metadado dessa pessoa para nós sem que ela
/// tivesse pedido. Um recuo que atravessa uma escolha explícita de outra pessoa
/// não é resiliência, é traição silenciosa.
///
/// IPv6 antes de IPv4, e a ordem é decidida pela máquina que pergunta: quem tem
/// IPv6 global usa o IPv6, porque é justamente o par que só se alcança por lá
/// que mais precisa deste degrau.
const REDE_DO_PADRAO: [&str; 2] = [
    "[2001:19f0:5400:2f6c:5400:6ff:fe94:68f5]:8384",
    "45.32.222.33:8384",
];

/// A variável que troca o ponto de encontro, ou o desliga.
///
/// `SEELE_ENCONTRO=nao` (ou vazio) desliga o degrau 4 inteiro: nenhum pacote sai
/// daqui para ninguém, e a escada volta a ser exatamente a de antes.
pub const VARIAVEL: &str = "SEELE_ENCONTRO";

/// Quanto tempo se espera pelo ponto de encontro antes de seguir a escada.
///
/// Um segundo, e o número vem de duas contas em sentidos opostos.
///
/// Para baixo: é uma ida e volta a um servidor na internet, que numa rede
/// doméstica brasileira custa entre 20 ms e 200 ms. Um segundo cabe cinco vezes
/// o pior caso plausível, e ainda cabe uma pergunta repetida no meio para o caso
/// de um datagrama se perder.
///
/// Para cima: isto roda entre apertar **HOSPEDAR AQUI** e a sala abrir, depois
/// de o degrau 3 já ter gasto o prazo dele. O ADR 0022 já reclamou uma vez de
/// prazo longo demais no caminho comum — a busca de UPnP esgotava dez segundos
/// numa rede sem UPnP —, e a lição vale igual aqui: com o ponto de encontro fora
/// do ar, **todo** anfitrião de rede difícil paga este número inteiro, e paga
/// exatamente no caminho que já vai terminar em más notícias.
///
/// A diferença para o degrau 3 é que lá a espera era multicast na rede local, e
/// aqui é um pacote unicast: ou volta rápido, ou está bloqueado.
pub const PRAZO: Duration = Duration::from_secs(1);

/// De quanto em quanto tempo a pergunta é repetida enquanto o prazo corre.
const REPETICAO: Duration = Duration::from_millis(300);

/// De quanto em quanto tempo o caminho até o ponto de encontro é reavivado.
///
/// Um mapeamento de NAT para UDP some sozinho depois de um tempo de silêncio, e
/// os roteadores mais apertados esquecem em 30 segundos. Quinze é metade disso:
/// se o mapeamento morre, o endereço que está no convite deixa de valer e o
/// bilhete vira um endereço para onde não chega mais aviso nenhum.
const REAVIVAR: Duration = Duration::from_secs(15);

/// Quantos pacotes o furo manda, e de quanto em quanto tempo.
///
/// Cinco, espaçados: um só se perde, e o outro lado pode estar chegando alguns
/// milissegundos depois. Não é mais que isso porque cada pacote é um pacote
/// mandado para um endereço que quem tem o link escolheu.
const PACOTES_DO_FURO: u8 = 5;
/// O intervalo entre eles.
const INTERVALO_DO_FURO: Duration = Duration::from_millis(120);

/// Quantos furos cabem numa janela, para o Dogma não virar refletor.
///
/// A marca do aviso já limita quem consegue provocar um furo a quem tem o link
/// (ver [`Marca`]), e isto é o segundo cinto: mesmo com o link, ninguém faz este
/// processo mandar pacotes sem parar para um endereço escolhido.
const FUROS_POR_JANELA: usize = 20;
/// O tamanho dessa janela.
const JANELA: Duration = Duration::from_secs(10);

/// Por que o degrau 4 não deu.
///
/// Variantes, e não um texto, pelo mesmo motivo do degrau 3: o que a pessoa pode
/// fazer a respeito é diferente em cada uma.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FalhaNoEncontro {
    /// Ninguém pediu um ponto de encontro — `$SEELE_ENCONTRO=nao`.
    ///
    /// Não é falha de rede e não vira frase de erro: é uma escolha de quem
    /// hospeda, e ela é respeitada em silêncio.
    Desligado,
    /// O nome do ponto de encontro não resolve.
    NaoResolve(String),
    /// O ponto de encontro não respondeu dentro do prazo.
    ///
    /// Fora do ar, bloqueado por firewall de saída, ou uma rede que não deixa
    /// UDP sair. Não dá para distinguir daqui, e prometer que dá seria pior.
    SemResposta,
    /// A escuta de avisos não abriu nesta máquina.
    SemEscutaDeAvisos(String),
    /// O socket do Dogma não pôde ser usado para falar com o ponto de encontro.
    ///
    /// Sem ele não há furo possível: o pacote tem de sair da porta em que o QUIC
    /// atende, ou o roteador abre caminho para o socket errado.
    SemSocketDoDogma,
}

impl std::fmt::Display for FalhaNoEncontro {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Desligado => write!(
                f,
                "o ponto de encontro está desligado nesta máquina ({VARIAVEL})"
            ),
            // Duas frases, e a diferença não é zelo: elas apontam para pessoas
            // diferentes.
            //
            // Quando o nome é o **nosso** e ele não resolve, a causa é que
            // ninguém o publicou ainda — pendência 21, e é tarefa de infra
            // nossa. Dizer «o nome não resolve, ou esta máquina está sem DNS»
            // manda quem hospeda procurar defeito no próprio computador por uma
            // coisa que não é dele. Apareceu exatamente assim numa tela de
            // verdade, e é o tipo de mentira por omissão que o ADR 0022 existe
            // para não deixar acontecer.
            //
            // Quando o nome é um que a pessoa escolheu, aí sim a suspeita é do
            // lado dela, e a frase antiga é a certa.
            Self::NaoResolve(nome) if nome == PONTO_PADRAO => write!(
                f,
                "o ponto de encontro padrão «{nome}» ainda não está no ar — é \
                 pendência nossa, não desta máquina; aponte {VARIAVEL} para um \
                 que você suba"
            ),
            Self::NaoResolve(nome) => write!(
                f,
                "não achei o ponto de encontro «{nome}»: o nome não resolve, ou \
                 esta máquina está sem DNS"
            ),
            Self::SemResposta => write!(
                f,
                "o ponto de encontro não respondeu a tempo — fora do ar, ou esta \
                 rede não deixa UDP sair"
            ),
            Self::SemEscutaDeAvisos(erro) => write!(
                f,
                "não deu para abrir a escuta de avisos do furo de NAT: {erro}"
            ),
            Self::SemSocketDoDogma => write!(
                f,
                "não consegui falar com o ponto de encontro pelo mesmo socket em \
                 que o Dogma atende"
            ),
        }
    }
}

impl std::error::Error for FalhaNoEncontro {}

/// O que o degrau 4 precisa saber para tentar.
///
/// Existe para o degrau ser **fácil de não usar**: um `None` na
/// [`super::Escada::subir`] e nenhum pacote sai desta máquina para ninguém.
pub struct Convocacao {
    /// O socket em que o Dogma atende, clonado.
    ///
    /// Tem de ser este e não outro: ver o cabeçalho do módulo.
    pub socket: Arc<std::net::UdpSocket>,
    /// A impressão digital do Dogma, de onde sai a marca que os avisos trazem.
    pub impressao_digital: String,
    /// O endereço do ponto de encontro, como texto.
    pub ponto: String,
}

impl Convocacao {
    /// O que o ambiente pediu, ou `None` se pediu para não haver degrau 4.
    ///
    /// Lê `$SEELE_ENCONTRO`: um endereço troca o ponto de encontro, `nao` ou
    /// vazio desligam o degrau.
    #[must_use]
    pub fn do_ambiente(socket: Arc<std::net::UdpSocket>, impressao_digital: &str) -> Option<Self> {
        let escolhido = std::env::var(VARIAVEL).unwrap_or_else(|_| PONTO_PADRAO.to_owned());
        let escolhido = escolhido.trim();
        if escolhido.is_empty() || escolhido.eq_ignore_ascii_case("nao") {
            return None;
        }
        Some(Self {
            socket,
            impressao_digital: impressao_digital.to_owned(),
            ponto: escolhido.to_owned(),
        })
    }
}

/// A marca que um aviso legítimo traz: os primeiros dígitos da impressão
/// digital do Dogma.
///
/// Ela está no `seele://` e em nenhum outro lugar, então um aviso com esta marca
/// veio de alguém com o link na mão. Não é autenticação — quem tem o link, tem —
/// e não precisa ser: o que ela faz é impedir que um varredor de portas faça
/// este Dogma mandar pacotes para onde ele quiser.
fn marca_do_convite(impressao_digital: &str) -> Option<Marca> {
    Marca::nova(impressao_digital.get(..16)?)
}

/// Um encontro aberto: o que o convite precisa dizer, e a tarefa que o mantém.
pub struct Encontro {
    ponto: String,
    aviso: SocketAddr,
    publico: SocketAddr,
    tarefa: tokio::task::JoinHandle<()>,
}

impl std::fmt::Debug for Encontro {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Encontro")
            .field("ponto", &self.ponto)
            .field("aviso", &self.aviso)
            .field("publico", &self.publico)
            .finish()
    }
}

impl Encontro {
    /// O endereço público do Dogma, para entrar no convite como candidato.
    #[must_use]
    pub fn publico(&self) -> SocketAddr {
        self.publico
    }

    /// O bilhete que vai no `seele://`.
    #[must_use]
    pub fn bilhete(&self) -> Bilhete {
        // As duas metades já passaram por `validar_alvo` ao serem lidas ou são
        // `SocketAddr`, que sempre escrevem um endereço válido. O recuo é
        // inalcançável e existe para não haver `expect` aqui.
        Bilhete::novo(&self.ponto, self.aviso.to_string()).unwrap_or(Bilhete {
            ponto: self.ponto.clone(),
            aviso: self.aviso.to_string(),
        })
    }

    /// Para de reavivar o caminho e de atender avisos.
    ///
    /// Nada precisa ser devolvido a ninguém: ao contrário do degrau 3, aqui não
    /// ficou regra nenhuma num roteador. O mapeamento de NAT some sozinho assim
    /// que este processo para de falar.
    pub fn fechar(self) {
        self.tarefa.abort();
    }
}

/// Sobe o degrau 4, ou diz por que não deu.
///
/// Nunca demora mais que [`PRAZO`], e essa é a promessa que importa: com o ponto
/// de encontro fora do ar, tudo o que funcionava continua funcionando um segundo
/// depois — rede local, IPv6 e porta no roteador não passam por aqui.
///
/// # Errors
///
/// [`FalhaNoEncontro`], sempre dizendo qual dos casos foi.
pub async fn abrir(convocacao: &Convocacao) -> Result<Encontro, FalhaNoEncontro> {
    // Um prazo para tudo, e não um por pergunta: são três esperas — DNS e duas
    // perguntas —, e três prazos de um segundo seriam três segundos de espera
    // com cara de um. Quem apertou HOSPEDAR espera [`PRAZO`], ponto.
    let ate = tokio::time::Instant::now() + PRAZO;

    let Some(marca_de_quem_chega) = marca_do_convite(&convocacao.impressao_digital) else {
        // Uma impressão digital que não tem 16 dígitos hexadecimais não é uma
        // impressão digital, e sem marca não há como separar aviso de ruído.
        return Err(FalhaNoEncontro::SemSocketDoDogma);
    };

    let ponto = resolver(&convocacao.ponto, ate).await?;
    let avisos = escuta_de_avisos(ponto)
        .await
        .map_err(|erro| FalhaNoEncontro::SemEscutaDeAvisos(erro.to_string()))?;

    // Primeira pergunta: o endereço público da escuta de avisos. Vai no
    // bilhete, e é para lá que quem tem o link manda o `LEVE` dele.
    let minha_marca = Marca::nova("anfitriao").ok_or(FalhaNoEncontro::SemSocketDoDogma)?;
    let aviso = tokio::time::timeout_at(
        ate,
        perguntar(&avisos, ponto, &encontro::onde(&minha_marca), &minha_marca),
    )
    .await
    .map_err(|_| FalhaNoEncontro::SemResposta)?
    .ok_or(FalhaNoEncontro::SemResposta)?;

    // Segunda pergunta, e a que só o socket do Dogma pode fazer: qual é o
    // endereço público **dele**. A resposta vem pela escuta de avisos, porque
    // deste socket não dá para ler — quem lê é o quinn.
    let publico = tokio::time::timeout_at(
        ate,
        perguntar_pelo_dogma(&convocacao.socket, &avisos, ponto, aviso, &minha_marca),
    )
    .await
    .map_err(|_| FalhaNoEncontro::SemResposta)?
    .ok_or(FalhaNoEncontro::SemResposta)?;

    tracing::info!(%aviso, %publico, ponto = %convocacao.ponto, "degrau 4: o ponto de encontro nos viu");

    let tarefa = tokio::spawn(atender(
        avisos,
        Arc::clone(&convocacao.socket),
        ponto,
        aviso,
        minha_marca,
        marca_de_quem_chega,
    ));

    Ok(Encontro {
        ponto: convocacao.ponto.clone(),
        aviso,
        publico,
        tarefa,
    })
}

/// Onde o ponto de encontro atende, resolvendo o nome se for um nome.
async fn resolver(texto: &str, ate: tokio::time::Instant) -> Result<SocketAddr, FalhaNoEncontro> {
    // O mesmo `Bilhete` que lê o endereço do link lê o do ambiente: a porta
    // padrão de um ponto de encontro não é a de um Dogma, e essa regra mora em
    // um lugar só.
    let alvo = Bilhete::novo(texto, "0.0.0.0:0").ok().and_then(|bilhete| {
        bilhete
            .ponto()
            .ok()
            .map(|alvo| (alvo.maquina.to_owned(), alvo.porta))
    });
    let Some((maquina, porta)) = alvo else {
        return Err(FalhaNoEncontro::NaoResolve(texto.to_owned()));
    };

    // Com prazo: um DNS que não responde é a outra forma de o degrau 4 segurar
    // quem apertou HOSPEDAR, e ele não pode segurar ninguém.
    let procura = tokio::time::timeout_at(ate, tokio::net::lookup_host((maquina, porta)))
        .await
        .map_err(|_| FalhaNoEncontro::NaoResolve(texto.to_owned()))?;
    let achado = procura.ok().and_then(|mut achados| achados.next());
    if let Some(alvo) = achado {
        return Ok(alvo);
    }

    // O nome não resolveu. Se ele é o **nosso**, há uma rede embaixo; se é o de
    // outra pessoa, não há — ver [`REDE_DO_PADRAO`].
    if texto == PONTO_PADRAO {
        if let Some(alvo) = rede_do_padrao() {
            tracing::info!(%texto, %alvo, "o nome não resolveu; usando o endereço de reserva");
            return Ok(alvo);
        }
    }
    Err(FalhaNoEncontro::NaoResolve(texto.to_owned()))
}

/// O endereço de reserva que serve **esta** máquina.
///
/// IPv6 primeiro para quem tem IPv6 global, porque o par que só se alcança por
/// lá é justamente o que mais precisa do degrau 4. Sem IPv6, IPv4 — que é o que
/// toda máquina tem.
fn rede_do_padrao() -> Option<SocketAddr> {
    let tem_seis = super::endereco_de_saida_v6().is_some();
    REDE_DO_PADRAO
        .iter()
        .filter_map(|texto| texto.parse::<SocketAddr>().ok())
        .find(|alvo| alvo.is_ipv6() == tem_seis)
        .or_else(|| {
            REDE_DO_PADRAO
                .iter()
                .filter_map(|texto| texto.parse::<SocketAddr>().ok())
                .next_back()
        })
}

/// A escuta de avisos, na mesma família do ponto de encontro.
///
/// Mesma família de propósito: assim não há nada a saber sobre `IPV6_V6ONLY`,
/// cujo padrão muda de sistema para sistema e já custou caro ao degrau 2.
async fn escuta_de_avisos(ponto: SocketAddr) -> std::io::Result<tokio::net::UdpSocket> {
    let local: SocketAddr = if ponto.is_ipv4() {
        SocketAddr::from(([0, 0, 0, 0], 0))
    } else {
        SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, 0))
    };
    tokio::net::UdpSocket::bind(local).await
}

/// Manda um pedido pela escuta de avisos e espera o `AQUI` com a marca certa.
///
/// Repete enquanto o prazo de fora não estourar: um datagrama perdido não pode
/// custar o degrau inteiro.
async fn perguntar(
    avisos: &tokio::net::UdpSocket,
    ponto: SocketAddr,
    pedido: &[u8],
    esperada: &Marca,
) -> Option<SocketAddr> {
    loop {
        let _ = avisos.send_to(pedido, ponto).await;
        if let Ok(Some(endereco)) =
            tokio::time::timeout(REPETICAO, esperar_aqui(avisos, |marca| marca == esperada)).await
        {
            return Some(endereco);
        }
    }
}

/// O mesmo, mas o pedido sai pelo socket do Dogma.
///
/// É a única forma de descobrir o endereço público **daquele** socket, que é o
/// que vai no convite: o NAT mapeia por porta interna, e o endereço da escuta de
/// avisos não diz nada sobre a porta em que o QUIC atende.
async fn perguntar_pelo_dogma(
    dogma: &std::net::UdpSocket,
    avisos: &tokio::net::UdpSocket,
    ponto: SocketAddr,
    para: SocketAddr,
    esperada: &Marca,
) -> Option<SocketAddr> {
    let pedido = encontro::leve(para, esperada);
    loop {
        mandar_pelo_dogma(dogma, &pedido, ponto);
        if let Ok(Some(endereco)) =
            tokio::time::timeout(REPETICAO, esperar_aqui(avisos, |marca| marca == esperada)).await
        {
            return Some(endereco);
        }
    }
}

/// Um `send_to` no socket que o quinn possui.
///
/// Ele está em modo não-bloqueante — o `Drop` de nada disto muda isso —, então
/// um `WouldBlock` é possível e não é erro: o pedido é repetido pelo laço de
/// fora, e a fila de saída de um socket UDP esvazia em microssegundos.
fn mandar_pelo_dogma(dogma: &std::net::UdpSocket, datagrama: &[u8], destino: SocketAddr) {
    if let Err(erro) = dogma.send_to(datagrama, destino) {
        tracing::debug!(%erro, %destino, "não saiu pelo socket do Dogma");
    }
}

/// Lê da escuta de avisos até chegar um `AQUI` que interesse.
async fn esperar_aqui(
    avisos: &tokio::net::UdpSocket,
    interessa: impl Fn(&Marca) -> bool,
) -> Option<SocketAddr> {
    let mut balde = [0_u8; encontro::TAMANHO];
    loop {
        let (lidos, _) = avisos.recv_from(&mut balde).await.ok()?;
        let Some((marca, endereco)) = balde.get(..lidos).and_then(encontro::ler_aqui) else {
            continue;
        };
        if interessa(&marca) {
            return Some(endereco);
        }
    }
}

/// O laço que mantém o degrau 4 de pé enquanto o Dogma estiver.
///
/// Duas coisas ao mesmo tempo, e as duas precisam do mesmo socket de leitura:
///
/// - **reavivar** o caminho até o ponto de encontro, ou o mapeamento de NAT some
///   sozinho e o endereço que está no convite deixa de valer;
/// - **atender** os avisos de quem tem o link, furando o NAT para o endereço que
///   cada um traz.
async fn atender(
    avisos: tokio::net::UdpSocket,
    dogma: Arc<std::net::UdpSocket>,
    ponto: SocketAddr,
    aviso: SocketAddr,
    minha_marca: Marca,
    de_quem_chega: Marca,
) {
    let mut relogio = tokio::time::interval(REAVIVAR);
    relogio.tick().await;
    let mut balde = [0_u8; encontro::TAMANHO];
    let mut furos: Vec<tokio::time::Instant> = Vec::new();

    loop {
        tokio::select! {
            _ = relogio.tick() => {
                // Os dois caminhos, porque são dois mapeamentos: o da escuta de
                // avisos e o do socket do Dogma.
                let _ = avisos.send_to(&encontro::onde(&minha_marca), ponto).await;
                mandar_pelo_dogma(&dogma, &encontro::leve(aviso, &minha_marca), ponto);
            }
            recebido = avisos.recv_from(&mut balde) => {
                let Ok((lidos, origem)) = recebido else { continue };
                if !aviso_e_do_ponto(origem, ponto) {
                    tracing::debug!(%origem, "aviso de fora do ponto de encontro; ignorado");
                    continue;
                }
                let Some((marca, endereco)) = balde.get(..lidos).and_then(encontro::ler_aqui)
                else {
                    continue;
                };
                if marca != de_quem_chega {
                    // Ou é a resposta do nosso próprio reavivamento, ou é ruído
                    // da internet. Nenhum dos dois vira furo.
                    continue;
                }
                if !cabe_mais_um_furo(&mut furos) {
                    tracing::warn!(%endereco, "furos demais na janela; este aviso foi ignorado");
                    continue;
                }
                tracing::info!(%endereco, "degrau 4: alguém com o link está chegando; furando");
                furar(&dogma, endereco, &de_quem_chega).await;
            }
        }
    }
}

/// Se este aviso veio de onde o ponto de encontro atende.
///
/// A marca já separa "alguém com o convite" de "a internet batendo na porta", e
/// continua sendo a cinta principal. Esta é a segunda, e ela fecha um caminho
/// mais barato que o outro: um `AQUI` forjado direto nesta escuta não passa pelo
/// ponto de encontro, então quem o manda não paga a ida até lá.
///
/// **Compara o endereço, não a porta.** Um ponto de encontro atrás de um
/// balanceador responde de porta efêmera, e recusar isso quebraria topologias
/// legítimas sem ganhar nada: quem consegue forjar um endereço de origem forja a
/// porta junto.
fn aviso_e_do_ponto(origem: SocketAddr, ponto: SocketAddr) -> bool {
    origem.ip() == ponto.ip()
}

/// Se ainda cabe um furo na janela corrente.
fn cabe_mais_um_furo(furos: &mut Vec<tokio::time::Instant>) -> bool {
    let agora = tokio::time::Instant::now();
    furos.retain(|quando| agora.duration_since(*quando) < JANELA);
    if furos.len() >= FUROS_POR_JANELA {
        return false;
    }
    furos.push(agora);
    true
}

/// Abre o caminho para um endereço, pelo socket do Dogma.
///
/// Vários pacotes espaçados porque um se perde, e porque o outro lado pode estar
/// chegando alguns milissegundos depois. O conteúdo é irrelevante para quem
/// recebe — o quinn do outro lado descarta o que não for QUIC —, e é um `FURO`
/// nomeado para que quem estiver olhando um `tcpdump` saiba o que é.
async fn furar(dogma: &std::net::UdpSocket, destino: SocketAddr, marca: &Marca) {
    let pacote = encontro::furo(marca);
    for _ in 0..PACOTES_DO_FURO {
        mandar_pelo_dogma(dogma, &pacote, destino);
        tokio::time::sleep(INTERVALO_DO_FURO).await;
    }
}

#[cfg(test)]
mod testes {
    #[test]
    fn a_rede_do_padrao_existe_e_serve_esta_maquina() {
        // Se as duas linhas não forem endereços válidos, o recuo é decoração —
        // ele existiria no código e nunca devolveria nada.
        let escolhido = super::rede_do_padrao();
        assert!(
            escolhido.is_some(),
            "nenhum endereço de reserva serve esta máquina"
        );

        // E a escolha acompanha a máquina: quem tem IPv6 global fala IPv6 com o
        // ponto de encontro, porque é justamente o par que só se alcança por lá
        // que mais precisa deste degrau.
        let tem_seis = crate::alcance::endereco_de_saida_v6().is_some();
        if tem_seis {
            assert!(
                escolhido.is_some_and(|alvo| alvo.is_ipv6()),
                "esta máquina tem IPv6 global e o recuo escolheu IPv4"
            );
        }
    }

    #[tokio::test]
    async fn o_recuo_nunca_atravessa_o_ponto_que_outra_pessoa_escolheu() {
        // A propriedade que vale mais que a resiliência.
        //
        // Se alguém apontou `$SEELE_ENCONTRO` para o ponto de encontro dela e o
        // nome não resolve, cair no **nosso** mandaria o metadado dessa pessoa
        // para nós sem que ela tivesse pedido. Um recuo que atravessa uma
        // escolha explícita de outra pessoa não é resiliência.
        let ate = tokio::time::Instant::now() + super::PRAZO;
        let alheio = "encontro.invalido.invalid:8384";

        let resultado = super::resolver(alheio, ate).await;

        assert!(
            resultado.is_err(),
            "um ponto de encontro alheio que não resolve caiu no nosso: {resultado:?}"
        );
    }

    #[test]
    fn o_ponto_padrao_que_nao_resolve_nao_culpa_a_maquina_de_quem_hospeda() {
        // Apareceu numa tela de verdade: «o nome não resolve, ou esta máquina
        // está sem DNS», sobre o nosso próprio endereço, que ninguém publicou
        // ainda. Quem lê procura defeito no próprio computador por uma
        // pendência nossa — a 21.
        let nosso = super::FalhaNoEncontro::NaoResolve(super::PONTO_PADRAO.to_owned()).to_string();

        // A frase acusadora inteira, e não o pedaço `esta máquina`.
        //
        // A primeira versão desta asserção procurava só o pedaço — e reprovou o
        // próprio conserto, porque a frase nova diz «não desta máquina», que é o
        // contrário. Um guarda que casa com o texto que o desmente é um guarda
        // que não sabe o que está lendo.
        assert!(
            !nosso.contains("está sem DNS"),
            "a frase do ponto padrão joga a suspeita para o lado de quem hospeda: {nosso}"
        );
        assert!(
            nosso.contains("pendência nossa") || nosso.contains("não está no ar"),
            "a frase não diz que a causa é nossa: {nosso}"
        );
        assert!(
            nosso.contains(super::VARIAVEL),
            "a frase não diz o que fazer para usar o degrau 4 hoje: {nosso}"
        );

        // E o outro lado da mesma moeda: um nome que a pessoa escolheu e não
        // resolve **é** suspeita do lado dela, e a frase antiga é a certa.
        let dela =
            super::FalhaNoEncontro::NaoResolve("encontro.davi.exemplo".to_owned()).to_string();
        assert!(
            dela.contains("esta máquina") || dela.contains("não resolve"),
            "um ponto escolhido pela pessoa perdeu a frase que aponta para o lado dela: {dela}"
        );
        assert_ne!(nosso, dela, "as duas situações dizem a mesma coisa");
    }

    use super::*;

    #[test]
    fn a_marca_do_convite_sai_da_impressao_digital_e_nada_mais() {
        // Ela é o que separa "alguém com o link" de "a internet batendo na
        // porta". Se saísse de outro lugar, qualquer um a adivinharia.
        let fp = "3cbcfb0212da738f89c156de86eb280adee30fd6b907523b898fedcb2b1de5b9";
        let marca = marca_do_convite(fp).expect("uma impressão digital sempre dá uma marca");
        assert_eq!(marca.texto(), "3cbcfb0212da738f");
        assert!(
            fp.starts_with(marca.texto()),
            "a marca não é o começo da impressão digital"
        );
        // E um texto que não é uma impressão digital não vira marca nenhuma.
        assert!(marca_do_convite("curto").is_none());
    }

    #[test]
    fn desligar_o_degrau_4_e_uma_variavel_de_ambiente() {
        // «Opcional» cobrado: com `nao`, nenhum pacote sai desta máquina para
        // ponto de encontro nenhum, porque não há nem convocação para tentar.
        let socket = Arc::new(std::net::UdpSocket::bind("127.0.0.1:0").expect("socket"));
        let fp = "3cbcfb0212da738f89c156de86eb280adee30fd6b907523b898fedcb2b1de5b9";

        // A variável é global ao processo, então este teste a devolve como
        // estava — outros testes deste crate leem o mesmo ambiente.
        let antes = std::env::var(VARIAVEL).ok();
        // SAFETY-de-teste: os testes deste módulo que mexem no ambiente estão
        // todos aqui e são serializados por rodarem em sequência neste teste.
        std::env::set_var(VARIAVEL, "nao");
        assert!(Convocacao::do_ambiente(Arc::clone(&socket), fp).is_none());

        std::env::set_var(VARIAVEL, "");
        assert!(Convocacao::do_ambiente(Arc::clone(&socket), fp).is_none());

        std::env::set_var(VARIAVEL, "meu.ponto:9000");
        let minha = Convocacao::do_ambiente(Arc::clone(&socket), fp).expect("trocável");
        assert_eq!(
            minha.ponto, "meu.ponto:9000",
            "o ponto de encontro não é trocável"
        );

        std::env::remove_var(VARIAVEL);
        let padrao = Convocacao::do_ambiente(socket, fp).expect("padrão");
        assert_eq!(padrao.ponto, PONTO_PADRAO);

        match antes {
            Some(valor) => std::env::set_var(VARIAVEL, valor),
            None => std::env::remove_var(VARIAVEL),
        }
    }

    #[tokio::test]
    async fn um_ponto_de_encontro_que_nao_existe_nao_segura_ninguem() {
        // O requisito do ADR 0022 escrito como asserção: com o ponto de
        // encontro fora do ar, subir um Dogma não pode demorar mais nem falhar.
        // Aqui isso é o prazo; em `hospedagem` é o Dogma inteiro.
        //
        // O endereço é de documentação (RFC 5737): não existe rota para ele em
        // lugar nenhum, que é a forma mais próxima de "fora do ar" que cabe num
        // teste sem rede.
        let socket = Arc::new(std::net::UdpSocket::bind("127.0.0.1:0").expect("socket"));
        let convocacao = Convocacao {
            socket,
            impressao_digital: "3cbcfb0212da738f89c156de86eb280adee30fd6b907523b898fedcb2b1de5b9"
                .to_owned(),
            ponto: "192.0.2.1:8384".to_owned(),
        };

        let comeco = std::time::Instant::now();
        let Err(falha) = abrir(&convocacao).await else {
            panic!("um buraco negro respondeu");
        };
        let levou = comeco.elapsed();

        assert_eq!(falha, FalhaNoEncontro::SemResposta);
        assert!(
            levou < PRAZO + Duration::from_millis(500),
            "o degrau 4 segurou quem apertou HOSPEDAR por {levou:?}"
        );
    }

    #[test]
    fn a_janela_de_furos_fecha_e_depois_reabre() {
        // Sem isto, quem tem o link faz este Dogma mandar pacotes sem parar
        // para um endereço escolhido — um refletor com dono.
        let mut furos = Vec::new();
        for _ in 0..FUROS_POR_JANELA {
            assert!(cabe_mais_um_furo(&mut furos));
        }
        assert!(!cabe_mais_um_furo(&mut furos), "a janela não fecha nunca");
        assert_eq!(furos.len(), FUROS_POR_JANELA);
    }

    #[test]
    fn o_prazo_e_curto_porque_o_caminho_comum_o_paga_inteiro() {
        // A mesma conta do degrau 3: numa rede em que isto não funciona, o
        // prazo é gasto inteiro, e é gasto entre apertar HOSPEDAR e a sala
        // abrir — depois de o degrau 3 já ter gasto o dele.
        const { assert!(PRAZO.as_millis() <= 1500) };
        // E a repetição tem de caber dentro do prazo, ou a segunda pergunta
        // nunca chega a ser feita.
        const { assert!(REPETICAO.as_millis() * 2 < PRAZO.as_millis()) };
        // O reavivamento cabe com folga no esquecimento mais apertado de NAT
        // que se vê por aí, que é de 30 segundos.
        const { assert!(REAVIVAR.as_secs() * 2 <= 30) };
    }

    #[tokio::test]
    async fn um_aqui_de_origem_estranha_nao_vira_furo() {
        // O `AQUI` é o único datagrama que faz o Dogma mandar pacote para um
        // endereço que outra pessoa escolheu. Forjá-lo direto na escuta de avisos é
        // mais barato que forjar um `LEVE`: não passa pelo ponto de encontro, então
        // nem a marca nem a janela de furos são pagas duas vezes.
        //
        // A marca continua sendo a cinta principal — quem tem o link, tem. Esta é a
        // segunda: o pacote também tem de ter vindo de onde o ponto de encontro
        // atende.
        let ponto = SocketAddr::from(([203, 0, 113, 7], encontro::PORTA_PADRAO));
        let intruso = SocketAddr::from(([198, 51, 100, 9], 9000));

        assert!(
            !aviso_e_do_ponto(intruso, ponto),
            "um AQUI que não veio do ponto de encontro não abre caminho nenhum"
        );
        assert!(aviso_e_do_ponto(ponto, ponto));
        // A porta de origem não conta: um ponto de encontro atrás de um balanceador
        // responde de porta efêmera, e recusar isso quebraria topologias legítimas
        // sem baixar a superfície de abuso — quem forja endereço forja porta.
        let mesma_maquina_outra_porta = SocketAddr::from(([203, 0, 113, 7], 40000));
        assert!(aviso_e_do_ponto(mesma_maquina_outra_porta, ponto));
    }

    /// Sobe um `atender` de teste com sockets reais de loopback, para os dois
    /// casos abaixo. Devolve a tarefa (para poder abortá-la ao fim do teste), o
    /// endereço da escuta de avisos (para onde o `AQUI` é mandado), o socket que
    /// receberia o `FURO`, o endereço dele, e a marca que um `AQUI` tem de
    /// trazer para não ser tratado como ruído.
    async fn subir_atender_de_teste(
        ponto: SocketAddr,
    ) -> (
        tokio::task::JoinHandle<()>,
        SocketAddr,
        tokio::net::UdpSocket,
        SocketAddr,
        Marca,
    ) {
        let Ok(avisos) = tokio::net::UdpSocket::bind("127.0.0.1:0").await else {
            panic!("não deu para abrir a escuta de avisos de teste");
        };
        let Ok(avisos_endereco) = avisos.local_addr() else {
            panic!("a escuta de avisos de teste não tem endereço local");
        };

        let Ok(dogma) = std::net::UdpSocket::bind("127.0.0.1:0") else {
            panic!("não deu para abrir o socket do Dogma de teste");
        };

        let Ok(alvo) = tokio::net::UdpSocket::bind("127.0.0.1:0").await else {
            panic!("não deu para abrir o alvo do furo de teste");
        };
        let Ok(alvo_endereco) = alvo.local_addr() else {
            panic!("o alvo do furo de teste não tem endereço local");
        };

        let Some(minha_marca) = Marca::nova("anfitriao") else {
            panic!("marca de teste inválida");
        };
        let Some(de_quem_chega) = Marca::nova("visitante") else {
            panic!("marca de teste inválida");
        };

        let tarefa = tokio::spawn(atender(
            avisos,
            Arc::new(dogma),
            ponto,
            avisos_endereco,
            minha_marca,
            de_quem_chega.clone(),
        ));

        (tarefa, avisos_endereco, alvo, alvo_endereco, de_quem_chega)
    }

    #[tokio::test]
    async fn atender_recusa_furo_para_aqui_que_nao_veio_do_ponto() {
        // `um_aqui_de_origem_estranha_nao_vira_furo`, acima, exercita
        // `aviso_e_do_ponto` isolada — e uma função pura correta não garante que
        // `atender` de fato a chame. Se alguém apagar por engano o `if
        // !aviso_e_do_ponto(...) { continue; }` de dentro do laço, aquele teste
        // continua passando, porque ele nunca roda `atender`. Este aqui roda.
        //
        // O `ponto` é `192.0.2.1` — TEST-NET-1, RFC 5737, reservado para
        // documentação e que não existe em rede nenhuma. `atender` nunca manda
        // nada para lá dentro da janela deste teste: ele só compara. O intruso
        // manda do loopback, os IPs não batem, e tudo fica só nesta máquina —
        // sem depender de segunda interface de rede nenhuma.
        let ponto = SocketAddr::from(([192, 0, 2, 1], encontro::PORTA_PADRAO));
        let (tarefa, avisos_endereco, alvo, alvo_endereco, marca) =
            subir_atender_de_teste(ponto).await;

        let Ok(intruso) = std::net::UdpSocket::bind("127.0.0.1:0") else {
            tarefa.abort();
            panic!("não deu para abrir o socket do intruso de teste");
        };
        let aviso_forjado = encontro::aqui(&marca, alvo_endereco);
        let _ = intruso.send_to(&aviso_forjado, avisos_endereco);

        let mut balde = [0_u8; encontro::TAMANHO];
        let nada_chegou =
            tokio::time::timeout(Duration::from_millis(300), alvo.recv_from(&mut balde)).await;
        tarefa.abort();
        assert!(
            nada_chegou.is_err(),
            "um AQUI que não veio do ponto de encontro furou mesmo assim"
        );
    }

    #[tokio::test]
    async fn atender_fura_para_aqui_que_veio_do_ponto() {
        // O par do teste acima: sem este, um `atender` que recusasse *todo*
        // `AQUI` passaria no teste hostil e ninguém notaria. Os dois juntos é
        // que fazem a checagem de origem reprovar nos dois sentidos.
        //
        // O `ponto` de encontro de teste é um socket de loopback de verdade, e
        // o `AQUI` sai exatamente dele — mesmo endereço que `atender` recebeu
        // como `ponto`, então a checagem deixa passar.
        let Ok(ponto_socket) = std::net::UdpSocket::bind("127.0.0.1:0") else {
            panic!("não deu para abrir o ponto de encontro de teste");
        };
        let Ok(ponto) = ponto_socket.local_addr() else {
            panic!("o ponto de encontro de teste não tem endereço local");
        };
        let (tarefa, avisos_endereco, alvo, alvo_endereco, marca) =
            subir_atender_de_teste(ponto).await;

        let aviso_legitimo = encontro::aqui(&marca, alvo_endereco);
        let _ = ponto_socket.send_to(&aviso_legitimo, avisos_endereco);

        let mut balde = [0_u8; encontro::TAMANHO];
        let chegou = tokio::time::timeout(Duration::from_secs(1), alvo.recv_from(&mut balde)).await;
        tarefa.abort();
        let Ok(Ok((lidos, _))) = chegou else {
            panic!("o FURO não chegou depois de um AQUI que veio do ponto de encontro");
        };
        assert!(lidos > 0, "o FURO chegou vazio");
    }

    #[test]
    fn toda_falha_do_degrau_4_diz_o_que_aconteceu() {
        // Mesmo critério do degrau 3: variantes existem porque as frases são
        // diferentes, e nenhuma pode sair vazia.
        let falhas = [
            FalhaNoEncontro::Desligado,
            FalhaNoEncontro::NaoResolve("encontro.exemplo".to_owned()),
            FalhaNoEncontro::SemResposta,
            FalhaNoEncontro::SemEscutaDeAvisos("endereço em uso".to_owned()),
            FalhaNoEncontro::SemSocketDoDogma,
        ];
        let frases: Vec<String> = falhas.iter().map(ToString::to_string).collect();
        for frase in &frases {
            assert!(frase.len() > 20, "falha sem frase de verdade: {frase}");
        }
        for (indice, frase) in frases.iter().enumerate() {
            for outra in frases.iter().skip(indice + 1) {
                assert_ne!(frase, outra, "duas falhas dizem a mesma coisa");
            }
        }
    }
}
