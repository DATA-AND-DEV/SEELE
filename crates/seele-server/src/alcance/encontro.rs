//! Degrau 4 do ADR 0022: furo de NAT com ponto de encontro.
//!
//! É o degrau que faz "manda o link e funciona" virar verdade numa casa com
//! CGNAT ou com o UPnP desligado — onde os degraus 2 e 3 não têm o que fazer e a
//! escada parava no 1.
//!
//! # O que acontece, na ordem
//!
//! 1. O servidor abre uma **escuta de avisos** própria, num socket dele, e pergunta
//!    ao ponto de encontro qual é o endereço público dela (`ONDE`). Esse
//!    endereço é metade do bilhete que vai no `seele://`.
//! 2. Pelo socket **do servidor** — o mesmo que o QUIC usa —, manda um `LEVE`
//!    apontando para aquela escuta de avisos. O ponto de encontro responde para
//!    lá dizendo de onde aquele pacote veio: é o endereço público do servidor, e é
//!    ele que entra no convite como mais um candidato.
//! 3. Quem recebe o convite manda o próprio `LEVE` para a escuta de avisos, pelo
//!    socket com que vai conectar. Chega aqui um aviso com o endereço dele.
//! 4. O servidor manda alguns pacotes para aquele endereço, **pelo socket do
//!    servidor**. O roteador daqui passa a ter uma saída registrada para lá, e o
//!    aperto de mão QUIC que vem em seguida entra.
//!
//! # Por que tem de ser o socket do servidor, e não outro
//!
//! Porque o NAT mapeia por porta interna. Um pacote saindo de outro socket abre
//! caminho para *aquele* socket, e o QUIC continua batendo numa porta fechada. É
//! o mesmo motivo pelo qual o furo tem de sair daqui e não da escuta de avisos:
//! a escuta de avisos existe **só** porque este processo não pode ler do socket
//! do servidor — quem lê dele é o quinn.
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
//! continua sendo conferida contra a do servidor. O que ele aprende é metadado —
//! que endereço falou com que endereço, e quando —, e isso está escrito em
//! `docs/alcance-pela-internet.md` em vez de ficar implícito.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use seele_proto::encontro::{self, Marca, Marcas};
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
/// Trocada em 2026-08-21, quando o ponto de encontro do projeto mudou de
/// Atlanta para o Brasil. A distância não era detalhe de conforto: com o serviço
/// em Atlanta, as duas pernas do furo — de quem entra até o ponto, e do ponto
/// até quem hospeda — somavam ~280 ms, e quem entra espera
/// [`seele_core`]`::enlace::ESPERA_DO_FURO`, que são 200. O aperto de mão saía
/// **antes** de o anfitrião ter furado, que é o defeito de origem deste degrau
/// reaparecendo por geografia. Do Brasil as duas pernas somam ~34 ms.
///
/// E o endereço velho tinha de sair daqui por uma segunda razão: um IP de VPS
/// destruída volta para o provedor e é entregue a outra pessoa. Enquanto esta
/// constante o carregasse, todo SEELE do mundo mandaria `SEELE-ENC` para a
/// máquina de um desconhecido sempre que o DNS falhasse — e ela aprenderia que
/// endereço falou com que endereço, que é exatamente o metadado que este ADR
/// pesa em voz alta.
const REDE_DO_PADRAO: [&str; 2] = [
    "[2001:19f0:b800:1bf5:5400:6ff:fe97:3c3b]:8384",
    "216.128.168.216:8384",
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

/// Quantos pacotes o servidor manda para abrir o caminho.
///
/// **Um.** Eram cinco, e os cinco nunca compraram resistência a perda: o
/// mapeamento de NAT nasce quando o pacote **sai** do roteador do anfitrião, não
/// quando chega ao outro lado. O que os cinco compravam era cobertura temporal —
/// e ela agora vem do aviso sair colado à tentativa, do outro lado, repetido
/// enquanto o aperto de mão corre. Cada repetição de lá provoca um furo daqui,
/// que é a mesma cobertura pelo lado que sabe quando ela é precisa.
///
/// A conta de segurança melhora junto. A origem de um UDP é forjável, então um
/// `LEVE` forjado com o endereço de uma vítima faz o servidor mandar pacotes para
/// ela. Com cinco, o ganho era 5:1; com um, é **1:1**, que é o teto que o ADR
/// 0022 fixou. Quem repete paga 96 bytes por repetição.
const PACOTES_DO_FURO: u8 = 1;
/// Quanto `furar` espera depois de mandar o pacote.
///
/// Com [`PACOTES_DO_FURO`] em um não há "entre eles" — era o intervalo entre os
/// cinco, e ficou como a pausa que fecha a volta única do laço. Ela não é
/// decoração: `furar` é aguardado dentro do braço do `select!` de `atender`, e
/// portanto esta pausa serializa o furo da **próxima** pessoa que estiver
/// entrando ao mesmo tempo. 120 ms de fila para uma segunda entrada simultânea é
/// barato; tirar a pausa exigiria reescrever `furar`, e isso é de outra tarefa.
const INTERVALO_DO_FURO: Duration = Duration::from_millis(120);

/// Quantos furos cabem numa janela, para o servidor não virar refletor.
///
/// A marca do aviso já limita quem consegue provocar um furo a quem tem o link
/// (ver [`Marca`]), e isto é o segundo cinto: mesmo com o link, ninguém faz este
/// processo mandar pacotes sem parar para um endereço escolhido.
///
/// # Por que sessenta, e não os vinte de antes
///
/// Porque o custo de uma entrada **legítima** mudou. Desde que o aviso passou a
/// sair colado em cada candidato, quem entra manda três avisos por candidato
/// público — e cada aviso é um furo daqui. Um convite de quatro candidatos custa
/// doze furos a uma pessoa só; numa janela rolante de dez segundos, até nove
/// deles caem juntos. Com o teto em vinte, duas ou três pessoas entrando ao
/// mesmo tempo fechavam a janela **contra elas mesmas** — o limite passava a
/// barrar o caso normal em vez do abuso, que é a definição de limite errado.
///
/// A conta que diz que subir é seguro: a janela existe para um varredor não
/// fazer o servidor jorrar pacote, e com a amplificação em 1:1 (ver
/// [`PACOTES_DO_FURO`]) cada furo é um datagrama de
/// [`encontro::TAMANHO`] = 96 bytes. Vinte por dez segundos são 1,9 kB; sessenta
/// são 5,8 kB. Os dois são desprezíveis como vetor — o que a janela limita
/// continua limitado, e o teto só deixou de barrar quem tem o link e está
/// entrando de verdade.
const FUROS_POR_JANELA: usize = 60;
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
    /// O ponto de encontro não respondeu à **primeira** pergunta, o `ONDE`.
    ///
    /// Ela sai de um socket recém-aberto e só pede «de onde este pacote veio».
    /// Se ela falha, o problema é entre esta máquina e o serviço: fora do ar,
    /// firewall de saída, ou uma rede que não deixa UDP sair.
    ///
    /// Separada da seguinte porque as duas apontam para lugares diferentes, e
    /// enquanto foram a mesma variante quem investigava procurava no lugar
    /// errado — foi o que aconteceu num teste de campo, três vezes seguidas.
    SemRespostaAoOnde,
    /// O ponto de encontro respondeu ao `ONDE` e **não** ao `LEVE`.
    ///
    /// É a pergunta que só o socket do servidor pode fazer, porque o NAT mapeia por
    /// porta interna e é a porta do QUIC que precisa do furo. Ela sai pelo
    /// espelho daquele socket e a resposta volta pela escuta de avisos.
    ///
    /// Falhar **só aqui** é informação forte: o caminho até o ponto de encontro
    /// funciona — a primeira pergunta acabou de provar isso — e o que não
    /// funciona é o espelho do socket do servidor, ou a volta até a escuta de
    /// avisos. Nenhuma ferramenta de diagnóstico deste projeto exercita esse
    /// caminho; ele só acontece ao hospedar.
    SemRespostaAoLeve,
    /// A escuta de avisos não abriu nesta máquina.
    SemEscutaDeAvisos(String),
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
            // «é pendência nossa» e o nome da variável de ambiente saíram na
            // auditoria de 2026-08-24: era a equipe escrevendo para quem usa, e
            // era o exemplo que o dono do produto citou. O que **não** pode sair
            // é «não é desta máquina» — é por isso que esta variante existe, e o
            // comentário acima conta o caso real que a produziu. Quem opera e
            // quer subir o próprio ponto tem `docs/ponto-de-encontro.md`; quem
            // apertou HOSPEDAR não tem o que fazer com um nome de variável.
            Self::NaoResolve(nome) if nome == PONTO_PADRAO => write!(
                f,
                "o serviço que abre caminho pela internet ainda não está no ar — \
                 não é problema desta máquina"
            ),
            Self::NaoResolve(nome) => write!(
                f,
                "não achei «{nome}»: confira o endereço, ou esta máquina está sem \
                 internet"
            ),
            // Duas frases porque apontam para lugares diferentes, e enquanto
            // foram uma só quem investigava procurava no lugar errado.
            Self::SemRespostaAoOnde => write!(
                f,
                "o serviço não respondeu — pode estar fora do ar, ou esta rede \
                 pode estar bloqueando a saída"
            ),
            // Esta é a informação mais específica que o degrau 4 consegue dar
            // sobre si mesmo: o serviço respondeu à primeira pergunta e não à
            // segunda, então o caminho até ele funciona e o que não funciona é o
            // socket do servidor — que é justamente o que nenhum diagnóstico deste
            // projeto sabe exercitar.
            // A distinção entre esta e a de cima continua valendo e continua
            // importando para quem investiga — «respondeu» e «não respondeu»
            // apontam para lugares diferentes. O que saiu foi `socket`, `furo` e
            // `Server`, que não dizem nada a quem lê e diziam tudo a quem
            // escreveu. O detalhe que elas carregavam está no `tracing`.
            Self::SemRespostaAoLeve => write!(
                f,
                "o serviço respondeu, mas não conseguiu abrir caminho de volta \
                 até esta máquina"
            ),
            Self::SemEscutaDeAvisos(erro) => write!(
                f,
                "esta máquina não conseguiu abrir uma porta para receber o aviso \
                 de quem está entrando: {erro}"
            ),
        }
    }
}

impl std::error::Error for FalhaNoEncontro {}

/// O que o degrau 4 precisa saber para tentar.
///
/// Existe para o degrau ser **fácil de não usar**: um `None` em
/// [`super::Escada::subir`], e nenhum pacote sai desta máquina para ninguém.
pub struct Convocacao {
    /// O socket em que o servidor atende, clonado.
    ///
    /// Tem de ser este e não outro: ver o cabeçalho do módulo.
    pub socket: Arc<std::net::UdpSocket>,
    /// As três marcas deste anfitrião no quarto, **prontas**.
    ///
    /// Elas vêm feitas, e não derivadas aqui dentro, porque quem as deriva
    /// muda com o que se hospeda. Um servidor guardado as tira da impressão
    /// digital ([`Marcas::do_servidor`]); a sala pessoal vai tirá-las do
    /// código. O degrau 4 não precisa saber qual dos dois é, e o cliente
    /// pergunta pelas mesmas porque usa a mesma função.
    pub marcas: Marcas,
    /// O endereço do ponto de encontro, como texto.
    pub ponto: String,
}

impl Convocacao {
    /// A convocação de um servidor guardado, com as marcas tiradas da
    /// impressão digital.
    ///
    /// Devolve `None` quando a impressão digital não forma marca: sem marca não
    /// há como separar aviso de ruído, e o degrau não é tentado.
    #[must_use]
    pub fn para_servidor(
        socket: Arc<std::net::UdpSocket>,
        impressao_digital: &str,
        ponto: impl Into<String>,
    ) -> Option<Self> {
        Some(Self {
            socket,
            marcas: Marcas::do_servidor(impressao_digital)?,
            ponto: ponto.into(),
        })
    }

    /// O que o ambiente pediu, ou `None` se pediu para não haver degrau 4.
    ///
    /// Lê `$SEELE_ENCONTRO`: um endereço troca o ponto de encontro, e `nao` ou
    /// vazio desligam o degrau.
    ///
    /// Também é `None` quando a impressão digital não forma marca, pelo mesmo
    /// motivo de [`Self::para_servidor`]: sem marca não há como separar aviso de
    /// ruído. Esse caso deixa um `warn` no log dizendo por que o degrau 4 ficou
    /// de fora.
    #[must_use]
    pub fn do_ambiente(socket: Arc<std::net::UdpSocket>, impressao_digital: &str) -> Option<Self> {
        let escolhido = std::env::var(VARIAVEL).unwrap_or_else(|_| PONTO_PADRAO.to_owned());
        let escolhido = escolhido.trim();
        if escolhido.is_empty() || escolhido.eq_ignore_ascii_case("nao") {
            return None;
        }
        let convocacao = Self::para_servidor(socket, impressao_digital, escolhido);
        if convocacao.is_none() {
            // Inalcançável com a impressão de um `Daemon`, que é sempre um
            // SHA-256 em hexadecimal. Se um dia deixar de ser, o degrau 4 some,
            // e esta linha é o que diz por quê: antes, `abrir` devolvia uma
            // falha que a escada registrava; agora a convocação nem nasce.
            tracing::warn!(
                "a impressão digital deste servidor não forma marca; o degrau 4 fica de fora"
            );
        }
        convocacao
    }
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
    /// O endereço público do servidor, para entrar no convite como candidato.
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

impl Drop for Encontro {
    /// Para de reavivar e de atender avisos, mesmo sem [`Encontro::fechar`].
    ///
    /// Largar um `Encontro` é o que acontece quando uma `Hospedagem` é
    /// descartada sem `encerrar`, e o `JoinHandle` largado **não** para a
    /// tarefa. Ela seguia viva para sempre, segurando uma cópia do socket do
    /// servidor: a porta ficava presa, e hospedar de novo nela falhava com
    /// «endereço já em uso». É o mesmo cuidado que `PortaAberta` e
    /// `BuracoAberto` já têm com as tarefas de renovação deles.
    fn drop(&mut self) {
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

    // As marcas vêm prontas na convocação ([`Marcas`]). As duas perguntas da
    // subida saem com a marca da **escuta**: a resposta delas volta para a
    // escuta de avisos, e uma resposta repetida que chegue depois de
    // `atender` começar só não vira furo porque a marca da escuta nunca é a
    // do aviso.
    let marcas = &convocacao.marcas;

    let candidatos = resolver(&convocacao.ponto, ate).await?;

    // Um de cada vez, até um responder, e **cada um com a sua fatia do prazo**.
    //
    // Um nome com A e AAAA vira dois candidatos, e qual deles serve é
    // propriedade desta máquina, que o DNS não conhece. A escuta de avisos nasce
    // da família do candidato, então cada tentativa tem a sua: um socket IPv4
    // não manda para destino IPv6, e o contrário só com pilha dupla.
    //
    // A fatia é o que faz isto funcionar, e ela custou um teste de campo para
    // aparecer. Um endereço que não serve falha de duas formas, e só uma é
    // barata: o `send_to` pode **errar** — e aí `perguntar` volta na hora — ou
    // pode **dar certo e o pacote sumir**, que é o que um IPv6 global sem rota
    // faz em algumas máquinas. No segundo caso não há erro nenhum para observar,
    // e sem fatia o primeiro candidato repetia a cada [`REPETICAO`] até o prazo
    // inteiro acabar: o IPv4, que responde em trezentos milissegundos, nunca
    // chegava a ser tentado.
    //
    // Dividir por candidato conserta as duas formas sem depender de adivinhar
    // qual delas o sistema escolheu. É o mesmo princípio que
    // `seele_core::enlace` aplica do outro lado: um candidato morto não gasta o
    // orçamento do próximo.
    let quantos = candidatos.len().max(1);
    let mut ponto = None;
    let mut aviso = None;
    for alvo in candidatos {
        let Ok(escuta) = escuta_de_avisos(alvo).await else {
            continue;
        };
        // O que sobra do prazo, dividido pelo que falta tentar. O último
        // candidato fica com tudo o que sobrou, que é o comportamento certo:
        // não há próximo para quem guardar.
        let agora = tokio::time::Instant::now();
        let sobra = ate.saturating_duration_since(agora);
        let fatia = agora + sobra / u32::try_from(quantos).unwrap_or(1);
        let resposta = tokio::time::timeout_at(
            fatia,
            perguntar(
                &escuta,
                alvo,
                &encontro::onde(&marcas.escuta),
                &marcas.escuta,
            ),
        )
        .await;
        match resposta {
            Ok(Some(publico)) => {
                ponto = Some((alvo, escuta));
                aviso = Some(publico);
                break;
            }
            // Este endereço não serve, por erro ou por silêncio. Nos dois
            // casos o próximo ainda tem a fatia dele — e é justamente o silêncio
            // que antes comia o prazo de todo mundo.
            Ok(None) => continue,
            Err(_) if tokio::time::Instant::now() < ate => continue,
            Err(_) => break,
        }
    }
    let (Some((ponto, avisos)), Some(aviso)) = (ponto, aviso) else {
        return Err(FalhaNoEncontro::SemRespostaAoOnde);
    };

    // Segunda pergunta, e a que só o socket do servidor pode fazer: qual é o
    // endereço público **dele**. A resposta vem pela escuta de avisos, porque
    // deste socket não dá para ler — quem lê é o quinn.
    let publico = tokio::time::timeout_at(
        ate,
        perguntar_pelo_server(&convocacao.socket, &avisos, ponto, aviso, &marcas.escuta),
    )
    .await
    .map_err(|_| FalhaNoEncontro::SemRespostaAoLeve)?
    .ok_or(FalhaNoEncontro::SemRespostaAoLeve)?;

    tracing::info!(%aviso, %publico, ponto = %convocacao.ponto, "degrau 4: o ponto de encontro nos viu");

    let tarefa = tokio::spawn(atender(
        avisos,
        Arc::clone(&convocacao.socket),
        ponto,
        aviso,
        marcas.clone(),
    ));

    Ok(Encontro {
        ponto: convocacao.ponto.clone(),
        aviso,
        publico,
        tarefa,
    })
}

/// Onde o ponto de encontro atende, resolvendo o nome se for um nome.
async fn resolver(
    texto: &str,
    ate: tokio::time::Instant,
) -> Result<Vec<SocketAddr>, FalhaNoEncontro> {
    // O mesmo `Bilhete` que lê o endereço do link lê o do ambiente: a porta
    // padrão de um ponto de encontro não é a de um servidor, e essa regra mora em
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

    // **Todos**, e não o primeiro. `encontro.seele.app.br` tem A e AAAA, e a
    // ordem em que o DNS os devolve não sabe nada sobre esta máquina: numa sem
    // rota IPv6, o AAAA primeiro fazia a escuta de avisos nascer IPv6 e o degrau
    // inteiro morrer por prazo, com o IPv4 que funcionava intocado. Foi o que o
    // primeiro teste de campo real encontrou.
    let mut achados: Vec<SocketAddr> = procura.map(Iterator::collect).unwrap_or_default();

    // **IPv4 primeiro, e a ordem do DNS não decide isto.**
    //
    // Aqui não se está escolhendo por onde falar com o ponto de encontro — se
    // estivesse, tanto faria qual responde. Está-se escolhendo **o endereço que
    // vai no link que outra pessoa vai usar**: o `enc=` carrega o reflexo desta
    // conversa, e o reflexo é de quem respondeu primeiro.
    //
    // As duas famílias não valem o mesmo aí. Um reflexo IPv4 alcança quem tem
    // IPv4, que é quase todo mundo — é a frase que `Degrau::FuroDeNat` já usa
    // para se pôr acima do IPv6 direto. Um reflexo IPv6 alcança só quem também
    // tem IPv6, e ainda esbarra num detalhe que o furo não conserta: sem NAT
    // não há mapeamento a furar, e o que decide é o firewall do roteador, que
    // vem fechado para entrada não solicitada.
    //
    // Custou um teste de campo, e ele foi o inverso do primeiro: a máquina
    // ganhou IPv6 ao mudar de rede, o AAAA passou a responder antes, e o link
    // saiu sem **nenhum** caminho IPv4 — `enc=` em IPv6, `alt=` só de IPv6, e o
    // endereço da frente sendo o de rede local. Quem estava no 5G não tinha por
    // onde entrar. Antes do IPv6 o mesmo servidor funcionava.
    //
    // Os IPv6 não se perdem: eles viajam no `alt=`, que é onde um endereço
    // global direto pertence. O que esta ordem protege é o furo.
    achados.sort_by_key(|achado| u8::from(achado.is_ipv6()));

    if !achados.is_empty() {
        return Ok(achados);
    }

    // O nome não resolveu. Se ele é o **nosso**, há uma rede embaixo; se é o de
    // outra pessoa, não há — ver [`REDE_DO_PADRAO`].
    if texto == PONTO_PADRAO {
        if let Some(alvo) = rede_do_padrao() {
            tracing::info!(%texto, %alvo, "o nome não resolveu; usando o endereço de reserva");
            return Ok(vec![alvo]);
        }
    }
    Err(FalhaNoEncontro::NaoResolve(texto.to_owned()))
}

/// O endereço de reserva que serve **esta** máquina.
///
/// **IPv4 primeiro**, pela mesma razão que [`resolver`] ordena assim, e o texto
/// aqui dizia o contrário: «IPv6 primeiro para quem tem IPv6 global, porque o
/// par que só se alcança por lá é justamente o que mais precisa do degrau 4».
///
/// O que essa frase esquecia é de quem é o endereço. Ele não é por onde esta
/// máquina fala com o ponto de encontro — é o que sai **no link**, para outra
/// pessoa usar. Um par que só tem IPv6 é raro; um que só tem IPv4 é a maioria,
/// e um link sem caminho IPv4 não serve a ele. Foi exatamente o que aconteceu
/// em campo: a máquina ganhou IPv6, o link saiu inteiro em IPv6, e quem estava
/// no 5G ficou de fora de um servidor que funcionava no dia anterior.
///
/// Sem IPv4 utilizável, IPv6 — a última linha continua servindo de recuo.
fn rede_do_padrao() -> Option<SocketAddr> {
    let mut candidatos: Vec<SocketAddr> = REDE_DO_PADRAO
        .iter()
        .filter_map(|texto| texto.parse::<SocketAddr>().ok())
        .collect();
    candidatos.sort_by_key(|alvo| u8::from(alvo.is_ipv6()));
    candidatos.first().copied()
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
        // O envio que **falha** não é o datagrama que se perde, e tratá-los
        // igual custou o degrau 4 inteiro numa máquina de verdade: num destino
        // que este socket não alcança — outra família, sem rota —, o `send_to`
        // devolve erro na hora e a espera de [`REPETICAO`] fica aguardando
        // resposta de um pacote que nunca saiu. Três voltas dessas comem o
        // [`PRAZO`], e o endereço que funcionava nunca chega a ser tentado.
        //
        // É o mesmo defeito que `seele_core::encontro::Batida::avisar` deixou de
        // ter neste ciclo, no arquivo ao lado: um erro engolido vira espera, e a
        // espera vira uma frase dizendo que o ponto de encontro não respondeu.
        if let Err(erro) = avisos.send_to(pedido, ponto).await {
            tracing::debug!(%erro, %ponto, "o pedido não saiu; este endereço não serve");
            return None;
        }
        if let Ok(Some(endereco)) =
            tokio::time::timeout(REPETICAO, esperar_aqui(avisos, |marca| marca == esperada)).await
        {
            return Some(endereco);
        }
    }
}

/// O mesmo, mas o pedido sai pelo socket do servidor.
///
/// É a única forma de descobrir o endereço público **daquele** socket, que é o
/// que vai no convite: o NAT mapeia por porta interna, e o endereço da escuta de
/// avisos não diz nada sobre a porta em que o QUIC atende.
async fn perguntar_pelo_server(
    server: &std::net::UdpSocket,
    avisos: &tokio::net::UdpSocket,
    ponto: SocketAddr,
    para: SocketAddr,
    esperada: &Marca,
) -> Option<SocketAddr> {
    let pedido = encontro::leve(para, esperada);
    loop {
        // Em `debug`, como sempre foi: um pedido que não sai aqui vence o prazo,
        // e a escada guarda a recusa do degrau inteiro.
        if let Err(motivo) = mandar_pelo_server(server, &pedido, ponto) {
            tracing::debug!(%motivo, %ponto, "o pedido não saiu pelo socket do servidor");
        }
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
///
/// O destino é escrito na família **deste** socket antes de sair. Ver
/// [`na_familia_de`], que existe por causa de um degrau 4 que nunca aconteceu.
///
/// Devolve por que não saiu, e o peso da falha fica com quem chama: o registro
/// no quarto a diz no log quando ela começa e quando acaba
/// ([`RegistroNoQuarto`]); o pedido da subida e o furo a deixam em `debug`.
fn mandar_pelo_server(
    server: &std::net::UdpSocket,
    datagrama: &[u8],
    destino: SocketAddr,
) -> Result<(), String> {
    let destino = na_familia_de(server, destino)
        .map_err(|motivo| format!("este socket não alcança esta família: {motivo}"))?;
    server
        .send_to(datagrama, destino)
        .map(|_| ())
        .map_err(|erro| format!("não saiu pelo socket do servidor rumo a {destino}: {erro}"))
}

/// O mesmo destino, escrito na família em que um socket sabe falar.
///
/// Um socket `AF_INET6` — que é o que o servidor abre, porque `[::]` é o único
/// jeito de atender às duas famílias num descritor só — recusa um
/// `SocketAddr::V4` com `EINVAL`. Não é rota ausente nem firewall: é o
/// endereço escrito na forma errada. O mesmo destino na forma mapeada
/// (`::ffff:a.b.c.d`) sai e chega.
///
/// Isto custou um ciclo inteiro de campo. O `ONDE` chegava ao ponto de
/// encontro e o `LEVE` não, e a diferença entre os dois é só esta: o primeiro
/// sai por uma escuta própria, aberta na família do destino, e o segundo tem de
/// sair pelo socket do servidor, porque furar um NAT exige a porta em que o QUIC
/// atende. Toda máquina de pilha dupla falhava igual, e o erro do sistema ia
/// para um `debug!` que nenhum dos dois clientes coleta.
///
/// # Errors
///
/// Um socket IPv4 não alcança um destino IPv6: não há forma de escrever
/// `2001:db8::1` que caiba num `sockaddr_in`, e mandar assim mesmo seria
/// prometer um caminho que não existe.
fn na_familia_de(socket: &std::net::UdpSocket, destino: SocketAddr) -> Result<SocketAddr, String> {
    let daqui = socket
        .local_addr()
        .map_err(|erro| format!("este socket não sabe dizer onde está: {erro}"))?;
    match (daqui, destino) {
        (SocketAddr::V6(_), SocketAddr::V4(alvo)) => {
            Ok(SocketAddr::from((alvo.ip().to_ipv6_mapped(), alvo.port())))
        }
        (SocketAddr::V4(_), SocketAddr::V6(alvo)) => match alvo.ip().to_ipv4_mapped() {
            // Um IPv4 disfarçado de IPv6 volta a ser o que era e passa.
            Some(ipv4) => Ok(SocketAddr::from((ipv4, alvo.port()))),
            None => Err("este socket atende só em IPv4".to_owned()),
        },
        _ => Ok(destino),
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

/// O laço que mantém o degrau 4 de pé enquanto o servidor estiver no ar.
///
/// Duas coisas ao mesmo tempo, e as duas precisam do mesmo socket de leitura:
///
/// - **reavivar** o caminho até o ponto de encontro, ou o mapeamento de NAT some
///   sozinho e o endereço que está no convite deixa de valer;
/// - **atender** os avisos de quem tem o link, furando o NAT para o endereço que
///   cada um traz.
async fn atender(
    avisos: tokio::net::UdpSocket,
    server: Arc<std::net::UdpSocket>,
    ponto: SocketAddr,
    aviso: SocketAddr,
    marcas: Marcas,
) {
    // **O primeiro tique sai na hora, e é de propósito.** O `interval` do
    // tokio completa o primeiro `tick` imediatamente, e esta função o
    // consumia antes do laço. Isso empurrava o primeiro `MORO` para quinze
    // segundos depois da subida, e nesse meio tempo o quarto não sabia onde
    // este servidor mora: quem voltava pela trilha logo depois de o anfitrião
    // reabrir perguntava a um quarto vazio (análise de 22/09/2026, §2.1).
    // Registrar na subida custa os mesmos três datagramas que o reavivamento
    // já manda.
    let mut relogio = tokio::time::interval(REAVIVAR);
    let mut balde = [0_u8; encontro::TAMANHO];
    let mut furos: Vec<tokio::time::Instant> = Vec::new();
    let mut registro = RegistroNoQuarto::default();

    loop {
        tokio::select! {
            _ = relogio.tick() => {
                // Os dois caminhos, porque são dois mapeamentos de NAT: o da
                // escuta de avisos e o do socket do servidor. `MORO`, e não
                // `ONDE`, registra os dois no quarto de graça, no pacote que já
                // ia sair.
                //
                // **A escuta com a marca dela, e nunca com a do aviso.** A
                // resposta a este `MORO` volta para cá com a marca dele, e o
                // filtro lá embaixo só deixa passar a do aviso. Até a v0.15.0
                // a marca era fixa (`anfitriao`), igual para todo anfitrião do
                // mundo, e quem procurava a escuta pela impressão digital nunca
                // a achava.
                //
                // O socket do **servidor** também se registra, porque é para
                // ele que quem chega conecta. A resposta dele volta para o
                // próprio socket do servidor, onde quem lê é o QUIC, e o QUIC a
                // descarta como já descarta todo `FURO` que chega ali. Ver o
                // cabeçalho de `encontro::furo`.
                let saiu = registrar(&avisos, &server, ponto, aviso, &marcas).await;
                registro.anotar(ponto, saiu);
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
                if marca != marcas.aviso {
                    // Ou é a resposta do nosso próprio registro, que vem com a
                    // marca da escuta ou do servidor e nunca com a do aviso, ou
                    // é ruído da internet. Nenhum dos dois vira furo.
                    continue;
                }
                if !cabe_mais_um_furo(&mut furos) {
                    tracing::warn!(%endereco, "furos demais na janela; este aviso foi ignorado");
                    continue;
                }
                tracing::info!(%endereco, "degrau 4: alguém com o link está chegando; furando");
                furar(&server, endereco, &marcas.aviso).await;
            }
        }
    }
}

/// Os três datagramas de cada tique de [`atender`]: o registro das duas marcas
/// no quarto e o reavivamento dos dois caminhos.
///
/// Os três saem sempre, mesmo que um falhe: são sockets diferentes, e um que não
/// sai não diz nada sobre o outro. Devolve o primeiro motivo de falha, e quem
/// decide se ele vai para o log é [`RegistroNoQuarto`].
async fn registrar(
    avisos: &tokio::net::UdpSocket,
    server: &std::net::UdpSocket,
    ponto: SocketAddr,
    aviso: SocketAddr,
    marcas: &Marcas,
) -> Result<(), String> {
    let escuta = avisos
        .send_to(&encontro::moro(&marcas.escuta), ponto)
        .await
        .map(|_| ())
        .map_err(|erro| format!("o MORO da escuta de avisos não saiu: {erro}"));
    let leve = mandar_pelo_server(server, &encontro::leve(aviso, &marcas.escuta), ponto);
    let servidor = mandar_pelo_server(server, &encontro::moro(&marcas.servidor), ponto);
    escuta.and(leve).and(servidor)
}

/// Se o registro no quarto está saindo, para o log dizer só quando isso muda.
///
/// O registro sai a cada [`REAVIVAR`], e uma falha que dura inundaria o
/// `seele.log` com a mesma linha a cada quinze segundos. Por isso uma linha em
/// `info` na primeira falha depois de um registro bom, e uma na volta: quem lê o
/// log sabe desde quando este anfitrião sumiu do quarto, e quando voltou.
///
/// Começa como se o último registro tivesse saído, porque [`atender`] só sobe
/// depois de [`abrir`] ter falado com o ponto por estes mesmos sockets. A
/// primeira falha depois da subida já é uma transição.
#[derive(Debug, Default)]
struct RegistroNoQuarto {
    falhando: bool,
}

impl RegistroNoQuarto {
    /// Anota o resultado de um tique, e diz no log se ele mudou o estado.
    fn anotar(&mut self, ponto: SocketAddr, saiu: Result<(), String>) {
        match saiu {
            Err(motivo) if !self.falhando => {
                self.falhando = true;
                tracing::info!(
                    %ponto,
                    %motivo,
                    "encontro: o registro no quarto deixou de sair; quem volta pela impressão \
                     digital não acha este anfitrião até ele voltar"
                );
            }
            Ok(()) if self.falhando => {
                self.falhando = false;
                tracing::info!(%ponto, "encontro: o registro no quarto voltou a sair");
            }
            Ok(()) | Err(_) => {}
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

/// Abre o caminho para um endereço, pelo socket do servidor.
///
/// **Um pacote por aviso**, e é o teto do ADR 0022: a origem de um UDP é
/// forjável, então o que sai daqui não pode ser maior que o datagrama que o
/// causou. A resistência a perda que os cinco pacotes de antes pareciam comprar
/// vem hoje do outro lado — quem entra repete o aviso enquanto o aperto de mão
/// corre, e cada repetição provoca uma volta destas. Ver [`PACOTES_DO_FURO`].
///
/// O conteúdo é irrelevante para quem recebe — o quinn do outro lado descarta o
/// que não for QUIC —, e é um `FURO` nomeado para que quem estiver olhando um
/// `tcpdump` saiba o que é.
async fn furar(server: &std::net::UdpSocket, destino: SocketAddr, marca: &Marca) {
    let pacote = encontro::furo(marca);
    for _ in 0..PACOTES_DO_FURO {
        if let Err(motivo) = mandar_pelo_server(server, &pacote, destino) {
            tracing::debug!(%motivo, %destino, "o furo não saiu pelo socket do servidor");
        }
        tokio::time::sleep(INTERVALO_DO_FURO).await;
    }
}

#[cfg(test)]
mod testes {

    /// O socket do servidor atende em `[::]` e o ponto de encontro é IPv4 puro.
    ///
    /// Este é o caso de campo, e ele falhava calado: um `send_to` com um
    /// `SocketAddr::V4` num socket `AF_INET6` devolve `EINVAL`, e o degrau 4
    /// morria aí em toda máquina de pilha dupla — Mac e Windows exatamente
    /// igual. O `ONDE` chegava, porque sai por socket próprio; o `LEVE` não,
    /// porque é o único que sai por este.
    #[test]
    fn o_server_de_pilha_dupla_alcanca_um_ponto_de_encontro_ipv4() {
        let escuta = SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, 0));
        let Ok((server, crate::alcance::Pilha::Dupla)) = crate::alcance::abrir_escuta(escuta)
        else {
            // Sem pilha dupla não há o que este teste afirme.
            return;
        };
        let ponto = match std::net::UdpSocket::bind("127.0.0.1:0").and_then(|s| {
            let onde = s.local_addr()?;
            s.set_read_timeout(Some(std::time::Duration::from_secs(2)))?;
            Ok((s, onde))
        }) {
            Ok(par) => par,
            Err(erro) => panic!("o ponto de encontro de teste tem de abrir: {erro}"),
        };
        let (servico, onde) = ponto;

        let saiu = mandar_pelo_server(&server, b"ONDE", onde);
        assert!(
            saiu.is_ok(),
            "o socket de pilha dupla não mandou ao ponto IPv4: {saiu:?}"
        );

        let mut balde = [0_u8; 8];
        match servico.recv_from(&mut balde) {
            Ok((lidos, _)) => assert_eq!(balde.get(..lidos), Some(&b"ONDE"[..])),
            Err(erro) => panic!("o pedido não chegou ao ponto de encontro IPv4: {erro}"),
        }
    }
    use seele_proto::encontro::PORTA_PADRAO;
    use std::net::Ipv6Addr;

    // --------------------------------------------- a fatia por candidato

    #[tokio::test]
    async fn um_candidato_que_engole_em_silencio_nao_come_a_vez_do_proximo() {
        // O defeito que sobreviveu ao primeiro conserto, e que só um teste de
        // campo revelou. `encontro.seele.app.br` tem A e AAAA. Numa máquina sem
        // rota IPv6, mandar para o AAAA pode **errar** — e aí `perguntar` volta
        // na hora — ou pode **dar certo e o pacote sumir**, que é o que
        // acontece quando sobrou rota de um túnel desligado. No segundo caso não
        // há erro para observar, e sem fatia o primeiro candidato repetia a cada
        // REPETICAO até o PRAZO inteiro acabar: o endereço que respondia nunca
        // era tentado.
        //
        // Aqui o buraco negro é um endereço de documentação (RFC 5737), que
        // aceita o envio e não responde nunca — o mesmo comportamento, sem
        // depender das rotas desta máquina.
        let buraco = SocketAddr::from(([192, 0, 2, 1], PORTA_PADRAO));

        // E o que responde é um ponto de encontro de verdade, no laço local.
        let onde_atende = ponto_que_responde().await;

        let marca =
            Marca::nova("anfitriao").unwrap_or_else(|| panic!("«anfitriao» é uma marca válida"));
        let ate = tokio::time::Instant::now() + PRAZO;
        let candidatos = [buraco, onde_atende];
        let quantos = candidatos.len().max(1);

        let mut achou = None;
        for alvo in candidatos {
            let Ok(escuta) = escuta_de_avisos(alvo).await else {
                continue;
            };
            let agora = tokio::time::Instant::now();
            let sobra = ate.saturating_duration_since(agora);
            let fatia = agora + sobra / u32::try_from(quantos).unwrap_or(1);
            if let Ok(Some(publico)) = tokio::time::timeout_at(
                fatia,
                perguntar(&escuta, alvo, &encontro::onde(&marca), &marca),
            )
            .await
            {
                achou = Some(publico);
                break;
            }
        }

        assert!(
            achou.is_some(),
            "o segundo candidato responde e não foi alcançado: o primeiro, que \
             aceita o envio e nunca responde, comeu o prazo inteiro. É o defeito \
             que fez o degrau 4 morrer numa máquina de verdade com o serviço no ar."
        );
    }

    // --------------------------------------------- a família que não alcança

    #[tokio::test]
    async fn um_endereco_de_familia_inalcancavel_nao_come_o_prazo() {
        // O defeito que o primeiro teste de campo real encontrou, e ele não
        // estava na coordenação do furo: estava antes dela.
        //
        // `encontro.seele.app.br` tem A **e** AAAA. Numa máquina sem rota IPv6 —
        // um Windows atrás de dois roteadores, que foi a máquina do relato —, se
        // o DNS devolver o AAAA primeiro, a escuta de avisos nasce IPv6, o
        // `send_to` falha com «rede inalcançável», e `perguntar` esperava 300 ms
        // por uma resposta que não vinha. Três voltas dessas comem o prazo de um
        // segundo inteiro, e o IPv4 — que funciona — nunca chega a ser tentado.
        //
        // Aqui a rede inalcançável é encenada sem depender da máquina: um socket
        // IPv4 mandando para um destino IPv6 falha na hora, em qualquer sistema,
        // porque as famílias não se falam.
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .unwrap_or_else(|erro| panic!("um socket local tem de abrir: {erro}"));
        let destino_de_outra_familia = SocketAddr::from((
            "2001:db8::1"
                .parse::<Ipv6Addr>()
                .unwrap_or(Ipv6Addr::UNSPECIFIED),
            PORTA_PADRAO,
        ));
        let marca =
            Marca::nova("anfitriao").unwrap_or_else(|| panic!("«anfitriao» é uma marca válida"));

        let comeco = tokio::time::Instant::now();
        let resposta = tokio::time::timeout(
            PRAZO,
            perguntar(
                &socket,
                destino_de_outra_familia,
                &encontro::onde(&marca),
                &marca,
            ),
        )
        .await;
        let gasto = comeco.elapsed();

        // Devolveu, e não estourou o prazo: é a diferença entre «este endereço
        // não serve, tente o próximo» e «o ponto de encontro não respondeu a
        // tempo», que era a frase errada que a máquina de campo lia.
        assert!(
            matches!(resposta, Ok(None)),
            "um destino de outra família tem de voltar na hora dizendo que não \
             serve, e não consumir o prazo até estourar: {resposta:?}"
        );
        assert!(
            gasto < REPETICAO,
            "um destino que o socket não alcança comeu {gasto:?} do prazo — \
             `perguntar` está engolindo o erro do `send_to` e esperando resposta \
             de um envio que nunca saiu. Com duas famílias no DNS, isso gasta o \
             degrau 4 inteiro numa que a máquina não usa."
        );
    }

    #[tokio::test]
    async fn o_ipv4_e_tentado_antes_do_ipv6_quando_os_dois_existem() {
        // **A ordem é a coisa que quebrou**, e ela quebrou pelo caminho mais
        // difícil de prever: uma máquina que não tinha IPv6 ganhou IPv6 ao
        // mudar de rede. O DNS passou a responder AAAA antes, o AAAA respondeu,
        // e o link saiu com `enc=` em IPv6 — sem nenhum caminho IPv4. Quem
        // estava no 5G não tinha por onde entrar num servidor que funcionava no
        // dia anterior.
        //
        // O reflexo do primeiro candidato **é** o que vai no link, e é por isso
        // que a ordem não pode vir do DNS: ela não é sobre com quem esta
        // máquina consegue falar, é sobre quem consegue falar com esta máquina.
        //
        // `localhost` resolve para 127.0.0.1 e ::1 em praticamente toda
        // máquina, e é o único nome com as duas famílias de que um teste pode
        // depender sem rede.
        let ate = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        let Ok(achados) = super::resolver("localhost:8384", ate).await else {
            // Uma máquina que não resolve `localhost` para as duas famílias não
            // tem o que este teste mede. Passar sem medir é melhor que uma
            // bateria que falha por causa do `/etc/hosts` de quem a roda.
            return;
        };
        let Some(primeiro_seis) = achados.iter().position(SocketAddr::is_ipv6) else {
            return;
        };
        let Some(ultimo_quatro) = achados.iter().rposition(SocketAddr::is_ipv4) else {
            return;
        };
        assert!(
            ultimo_quatro < primeiro_seis,
            "um IPv6 foi ordenado antes de um IPv4: {achados:?}"
        );
    }

    #[test]
    fn a_rede_do_padrao_existe_e_serve_esta_maquina() {
        // Se as duas linhas não forem endereços válidos, o recuo é decoração —
        // ele existiria no código e nunca devolveria nada.
        let escolhido = super::rede_do_padrao();
        assert!(
            escolhido.is_some(),
            "nenhum endereço de reserva serve esta máquina"
        );

        // E é IPv4, **mesmo numa máquina com IPv6 global**. Este teste exigia o
        // contrário, e a inversão é o conserto de um defeito de campo: o
        // endereço escolhido aqui vai no `enc=` do link, e um `enc=` em IPv6 só
        // serve a quem também tem IPv6. Ver `rede_do_padrao`.
        assert!(
            escolhido.is_some_and(|alvo| alvo.is_ipv4()),
            "o recuo escolheu IPv6; um link com furo só em IPv6 deixa de fora \
             quem só tem IPv4, que é quase todo mundo"
        );
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
        // **Esta asserção era o contrário até 2026-08-24**: ela exigia que a
        // frase contivesse `SEELE_ENCONTRO`, com a justificativa «a frase não
        // diz o que fazer para usar o degrau 4 hoje». A auditoria de texto
        // derrubou aquela decisão: quem lê isto apertou HOSPEDAR AQUI e não tem
        // o que fazer com um nome de variável de ambiente — quem sobe o próprio
        // ponto de encontro é operador, e operador tem
        // `docs/ponto-de-encontro.md`. A informação não se perdeu; mudou de
        // lugar, que é a única coisa que a auditoria pede.
        //
        // Cobrado pelo avesso de propósito: sem isto, a variável volta na
        // primeira vez que alguém achar que está sendo prestativo.
        assert!(
            !nosso.contains(super::VARIAVEL),
            "a frase do ponto padrão voltou a despejar o nome da variável de \
             ambiente em quem só quer hospedar: {nosso}"
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
    fn a_marca_do_aviso_sai_da_impressao_digital_e_nada_mais() {
        // Ela é o que separa "alguém com o link" de "a internet batendo na
        // porta". Se saísse de outro lugar, qualquer um a adivinharia.
        let socket = Arc::new(std::net::UdpSocket::bind("127.0.0.1:0").expect("socket"));
        let convocacao = Convocacao::para_servidor(Arc::clone(&socket), IMPRESSAO, PONTO_PADRAO)
            .expect("uma impressão digital sempre dá uma convocação");
        assert_eq!(
            convocacao.marcas.aviso.texto(),
            "3cbcfb0212da738f",
            "a marca do aviso não é o começo da impressão digital: o LEVE de quem tem o link \
             deixa de passar no filtro do anfitrião, e ninguém fura para ninguém"
        );
        // E um texto que não é uma impressão digital não vira convocação nenhuma.
        assert!(
            Convocacao::para_servidor(socket, "curto", PONTO_PADRAO).is_none(),
            "uma impressão digital curta demais deu convocação: o degrau 4 subiria com marcas \
             que nenhum cliente sabe perguntar"
        );
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
        assert!(
            Convocacao::do_ambiente(Arc::clone(&socket), "curto").is_none(),
            "uma impressão digital que não forma marca deu convocação pelo ambiente: o degrau 4 \
             subiria sem marcas"
        );
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
        // encontro fora do ar, subir um servidor não pode demorar mais nem falhar.
        // Aqui isso é o prazo; em `hospedagem` é o servidor inteiro.
        //
        // O endereço é de documentação (RFC 5737): não existe rota para ele em
        // lugar nenhum, que é a forma mais próxima de "fora do ar" que cabe num
        // teste sem rede.
        let socket = Arc::new(std::net::UdpSocket::bind("127.0.0.1:0").expect("socket"));
        let convocacao = Convocacao::para_servidor(socket, IMPRESSAO, "192.0.2.1:8384")
            .expect("uma impressão digital dá uma convocação");

        let comeco = std::time::Instant::now();
        let Err(falha) = abrir(&convocacao).await else {
            panic!("um buraco negro respondeu");
        };
        let levou = comeco.elapsed();

        // `AoOnde` e não `AoLeve`: um buraco negro nunca responde à primeira
        // pergunta, então a segunda não chega a ser feita. Distinguir as duas
        // aqui é o que faz esta asserção significar alguma coisa.
        assert_eq!(falha, FalhaNoEncontro::SemRespostaAoOnde);
        assert!(
            levou < PRAZO + Duration::from_millis(500),
            "o degrau 4 segurou quem apertou HOSPEDAR por {levou:?}"
        );
    }

    #[tokio::test]
    async fn a_janela_cabe_uma_entrada_legitima_inteira() {
        // A propriedade que motivou o teto subir de vinte para sessenta, e que
        // não tinha guarda nenhuma: com `FUROS_POR_JANELA = 1` os quinze testes
        // deste crate ficavam verdes, e o limite podia ser dimensionado para
        // qualquer número sem ninguém notar.
        //
        // O número é **doze**, e ele fica literal de propósito. Derivá-lo de
        // `FUROS_POR_JANELA` seria a implicação que passa vazia — a mutação
        // apagaria a premissa junto com o problema. A aritmética que o produz:
        // um convite carrega no máximo `alcance::LIMITE_DE_CANDIDATOS` = 4
        // endereços, e quem entra manda `AVISOS_POR_CANDIDATO` = 3 avisos por
        // candidato que precisa de furo (o de `seele-core`, e é ele que fixa o
        // gasto). Quatro vezes três são doze furos por **uma** pessoa entrando,
        // no pior caso em que todos os quatro candidatos são públicos.
        //
        // Doze é o que uma entrada legítima custa. Se a janela barrar antes
        // disso, ela deixou de limitar abuso e passou a limitar quem tem o link
        // e está entrando de verdade — e a pessoa vê o degrau 4 falhar sem
        // ninguém ter feito nada de errado.
        const ENTRADA_LEGITIMA: usize = 12;

        let Ok(ponto_socket) = std::net::UdpSocket::bind("127.0.0.1:0") else {
            panic!("não deu para abrir o ponto de encontro de teste");
        };
        let Ok(ponto) = ponto_socket.local_addr() else {
            panic!("o ponto de encontro de teste não tem endereço local");
        };
        let (tarefa, avisos_endereco, alvo, alvo_endereco, marca) =
            subir_atender_de_teste(ponto).await;

        let aviso_legitimo = encontro::aqui(&marca, alvo_endereco);
        for _ in 0..ENTRADA_LEGITIMA {
            let _ = ponto_socket.send_to(&aviso_legitimo, avisos_endereco);
        }

        // `furar` é aguardado dentro do braço do `select!` de `atender`, então
        // os furos saem em fila, um a cada `INTERVALO_DO_FURO`. Doze levam
        // ~1,4 s; o prazo aqui é folgado para não virar teste de máquina rápida.
        let mut furos = 0_usize;
        let mut balde = [0_u8; encontro::TAMANHO];
        let ate = tokio::time::Instant::now() + Duration::from_secs(6);
        while furos < ENTRADA_LEGITIMA && tokio::time::Instant::now() < ate {
            let chegou =
                tokio::time::timeout(Duration::from_millis(600), alvo.recv_from(&mut balde)).await;
            match chegou {
                Ok(Ok(_)) => furos += 1,
                _ => break,
            }
        }
        tarefa.abort();

        assert_eq!(
            furos, ENTRADA_LEGITIMA,
            "a janela barrou uma entrada legítima: {furos} furos de \
             {ENTRADA_LEGITIMA}. O teto tem de caber o que uma pessoa que tem o \
             link custa, ou ele passa a barrar o caso normal em vez do abuso"
        );
    }

    #[tokio::test]
    async fn a_janela_fecha_dentro_do_atender_e_nao_so_no_auxiliar() {
        // O outro lado de `a_janela_cabe_uma_entrada_legitima_inteira`, e o que
        // faltava para a propriedade existir de verdade: **o servidor não vira
        // refletor**. É a razão de o ADR 0022 ter aceitado construir o degrau 4,
        // e ela não tinha fiação nenhuma.
        //
        // `a_janela_de_furos_fecha_e_depois_reabre` exercita `cabe_mais_um_furo`
        // isolada, e uma função pura correta não garante que `atender` chegue a
        // chamá-la: apagando o `if !cabe_mais_um_furo(…) { continue }` de dentro
        // do laço, aquele teste continua verde e o teto some. Este aqui roda
        // `atender`.
        //
        // O número de avisos sai de `FUROS_POR_JANELA` de propósito, e aqui isso
        // **não** é a implicação que passa vazia: o que se afirma é que o teto
        // vale onde ele está, seja onde for. Quem prende o teto por baixo é o
        // outro teste, com o 12 literal — os dois juntos são um sanduíche, e
        // mexer no valor sem mexer na aritmética acende um dos dois.
        //
        // Custa a fila de `INTERVALO_DO_FURO` × `FUROS_POR_JANELA`, que é o
        // preço de sair do auxiliar puro. Está pago.
        let alem_do_teto = FUROS_POR_JANELA + 1;

        let Ok(ponto_socket) = std::net::UdpSocket::bind("127.0.0.1:0") else {
            panic!("não deu para abrir o ponto de encontro de teste");
        };
        let Ok(ponto) = ponto_socket.local_addr() else {
            panic!("o ponto de encontro de teste não tem endereço local");
        };
        let (tarefa, avisos_endereco, alvo, alvo_endereco, marca) =
            subir_atender_de_teste(ponto).await;

        let aviso_legitimo = encontro::aqui(&marca, alvo_endereco);
        for _ in 0..alem_do_teto {
            let _ = ponto_socket.send_to(&aviso_legitimo, avisos_endereco);
        }

        // Conta até parar de chegar. O prazo de cada leitura é cinco vezes o
        // `INTERVALO_DO_FURO`, então a fila normal nunca o estoura — quem o
        // estoura é o fim dos furos.
        let mut furos = 0_usize;
        let mut balde = [0_u8; encontro::TAMANHO];
        let ate = tokio::time::Instant::now() + Duration::from_secs(40);
        while furos <= alem_do_teto && tokio::time::Instant::now() < ate {
            let chegou =
                tokio::time::timeout(Duration::from_millis(600), alvo.recv_from(&mut balde)).await;
            match chegou {
                Ok(Ok(_)) => furos += 1,
                _ => break,
            }
        }
        tarefa.abort();

        assert_eq!(
            furos, FUROS_POR_JANELA,
            "{alem_do_teto} avisos legítimos produziram {furos} furos, e o teto é \
             de {FUROS_POR_JANELA}. Acima do teto, o servidor vira refletor sem \
             limite para quem tem o link; abaixo dele, a janela barra uma entrada \
             que tinha direito de acontecer"
        );
    }

    #[test]
    fn a_janela_de_furos_fecha_e_depois_reabre() {
        // Sem isto, quem tem o link faz este servidor mandar pacotes sem parar
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
        // O `AQUI` é o único datagrama que faz o servidor mandar pacote para um
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
        let anfitriao = AnfitriaoDeTeste::abrir().await;
        let avisos_endereco = anfitriao.avisos_endereco;

        let Ok(alvo) = tokio::net::UdpSocket::bind("127.0.0.1:0").await else {
            panic!("não deu para abrir o alvo do furo de teste");
        };
        let Ok(alvo_endereco) = alvo.local_addr() else {
            panic!("o alvo do furo de teste não tem endereço local");
        };

        // As marcas de um anfitrião qualquer: `visitante` é o que um `AQUI`
        // legítimo traz, e as outras duas são as do registro no quarto. O
        // registro da subida vai para `ponto`, que nestes testes não responde
        // e não é o `alvo` que eles contam.
        let (Some(aviso), Some(escuta), Some(servidor)) = (
            Marca::nova("visitante"),
            Marca::nova("visitantee"),
            Marca::nova("visitantes"),
        ) else {
            panic!("marca de teste inválida");
        };
        let marcas = Marcas {
            aviso,
            escuta,
            servidor,
        };

        let tarefa = anfitriao.subir(ponto, marcas.clone());

        (tarefa, avisos_endereco, alvo, alvo_endereco, marcas.aviso)
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
        // documentação e que não existe em rede nenhuma. O registro da subida
        // sai para lá (o primeiro `MORO` sai na hora, e não quinze segundos
        // depois), e de lá nada volta: para o que este teste mede, o ponto só
        // serve de comparação. O intruso manda do loopback, os IPs não batem,
        // e o que o teste mede fica só nesta máquina — sem depender de segunda
        // interface de rede nenhuma.
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

    #[tokio::test]
    async fn um_aviso_faz_sair_um_furo_e_nunca_um_segundo() {
        // A propriedade que o ADR 0022 chama de inegociável, medida na saída e
        // não na constante: quem abusa não ganha banda. Um `AQUI` de 96 bytes
        // faz chegar à vítima escolhida **um** datagrama de 96 bytes, e nada
        // além.
        //
        // Contar aqui, e não conferir `PACOTES_DO_FURO == 1`, é a diferença
        // entre testar a propriedade e testar a própria constante. Uma asserção
        // sobre o valor é tautologia, e uma segunda asserção derivada dele não
        // pode reprovar sozinha: com a constante em um e o laço de `furar`
        // mandando cinco por volta, os dois passavam verdes e a amplificação
        // ficava sem guarda nenhuma. Isto é a mesma família de defeito que este
        // ciclo vem consertando — o auxiliar puro conferido, a fiação nunca.
        //
        // O segundo prazo é de 300 ms de propósito: é mais que o dobro de
        // `INTERVALO_DO_FURO`, então um laço que voltasse a mandar mais de um
        // pacote entrega o segundo bem dentro dele.
        let Ok(ponto_socket) = std::net::UdpSocket::bind("127.0.0.1:0") else {
            panic!("não deu para abrir o ponto de encontro de teste");
        };
        let Ok(ponto) = ponto_socket.local_addr() else {
            panic!("o ponto de encontro de teste não tem endereço local");
        };
        let (tarefa, avisos_endereco, alvo, alvo_endereco, marca) =
            subir_atender_de_teste(ponto).await;

        let aviso_legitimo = encontro::aqui(&marca, alvo_endereco);
        assert_eq!(
            aviso_legitimo.len(),
            encontro::TAMANHO,
            "o aviso que provoca o furo não tem o tamanho fixo do protocolo, e a \
             conta de amplificação abaixo não fecha"
        );
        let _ = ponto_socket.send_to(&aviso_legitimo, avisos_endereco);

        let mut balde = [0_u8; encontro::TAMANHO];
        let primeiro =
            tokio::time::timeout(Duration::from_secs(1), alvo.recv_from(&mut balde)).await;
        let Ok(Ok((lidos, _))) = primeiro else {
            tarefa.abort();
            panic!("nenhum FURO saiu por um AQUI legítimo: não há o que contar");
        };
        assert_eq!(
            lidos,
            encontro::TAMANHO,
            "o FURO que saiu não tem o tamanho do datagrama que o causou"
        );

        let segundo =
            tokio::time::timeout(Duration::from_millis(300), alvo.recv_from(&mut balde)).await;
        tarefa.abort();
        assert!(
            segundo.is_err(),
            "um segundo FURO saiu pelo mesmo AQUI: o servidor virou amplificador, \
             que é o que o ADR 0022 não aceita em hipótese nenhuma"
        );
    }

    #[test]
    fn toda_falha_do_degrau_4_diz_o_que_aconteceu() {
        // Mesmo critério do degrau 3: variantes existem porque as frases são
        // diferentes, e nenhuma pode sair vazia.
        let falhas = [
            FalhaNoEncontro::Desligado,
            FalhaNoEncontro::NaoResolve("encontro.exemplo".to_owned()),
            FalhaNoEncontro::SemRespostaAoOnde,
            FalhaNoEncontro::SemRespostaAoLeve,
            FalhaNoEncontro::SemEscutaDeAvisos("endereço em uso".to_owned()),
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

    /// O que o ponto de encontro de teste anotou: cada pedido, e de onde veio.
    type Caderno = Arc<std::sync::Mutex<Vec<(encontro::Pedido, SocketAddr)>>>;

    /// Um ponto de encontro de teste que anota cada pedido e a origem dele.
    ///
    /// Com `eco`, ele também responde como um ponto de verdade: `ONDE` e `MORO`
    /// voltam para quem perguntou, e `LEVE` vai para o destino. A diferença é
    /// que o `AQUI` aponta para `eco`, e não para o endereço que o ponto viu.
    /// É isso que torna observável um furo contra si mesmo: se a resposta
    /// passar no filtro de `atender`, o `FURO` chega a `eco` e o teste o vê.
    /// Um ponto de verdade poria ali o endereço da própria escuta, e o furo não
    /// deixaria rastro fora deste processo.
    async fn ponto_que_anota(eco: Option<SocketAddr>) -> (SocketAddr, Caderno) {
        let Ok(socket) = tokio::net::UdpSocket::bind("127.0.0.1:0").await else {
            panic!("não deu para abrir o ponto de encontro de teste");
        };
        let Ok(onde) = socket.local_addr() else {
            panic!("o ponto de encontro de teste não tem endereço local");
        };
        let caderno: Caderno = Arc::new(std::sync::Mutex::new(Vec::new()));
        let anotador = Arc::clone(&caderno);
        tokio::spawn(async move {
            let mut balde = [0_u8; encontro::TAMANHO];
            while let Ok((lidos, de)) = socket.recv_from(&mut balde).await {
                let Some(pedido) = balde.get(..lidos).and_then(encontro::analisar) else {
                    continue;
                };
                let resposta = match (&pedido, eco) {
                    (
                        encontro::Pedido::Onde { marca } | encontro::Pedido::Moro { marca },
                        Some(para_onde),
                    ) => Some((encontro::aqui(marca, para_onde), de)),
                    (encontro::Pedido::Leve { destino, marca }, Some(para_onde)) => {
                        Some((encontro::aqui(marca, para_onde), *destino))
                    }
                    _ => None,
                };
                if let Ok(mut lista) = anotador.lock() {
                    lista.push((pedido, de));
                }
                if let Some((datagrama, para)) = resposta {
                    let _ = socket.send_to(&datagrama, para).await;
                }
            }
        });
        (onde, caderno)
    }

    /// Espera até `prazo` que o caderno tenha um pedido que `procura` aceite.
    async fn esperar_no_caderno(
        caderno: &Caderno,
        prazo: Duration,
        procura: impl Fn(&encontro::Pedido, SocketAddr) -> bool,
    ) -> bool {
        let ate = tokio::time::Instant::now() + prazo;
        loop {
            let achou = caderno
                .lock()
                .is_ok_and(|lista| lista.iter().any(|(pedido, de)| procura(pedido, *de)));
            if achou {
                return true;
            }
            if tokio::time::Instant::now() >= ate {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// A impressão digital destes testes. As marcas saem dela pelo caminho de
    /// produção, [`Marcas::do_servidor`], e não escritas à mão.
    const IMPRESSAO: &str = "3cbcfb0212da738f89c156de86eb280adee30fd6b907523b898fedcb2b1de5b9";

    /// As marcas de [`IMPRESSAO`], pelo caminho de produção.
    fn marcas_da_impressao() -> Marcas {
        let Some(marcas) = Marcas::do_servidor(IMPRESSAO) else {
            panic!("a impressão digital de teste não forma marcas");
        };
        marcas
    }

    /// Os dois sockets de um anfitrião de teste, abertos no laço local: a escuta
    /// de avisos e o socket do servidor, cada um com o endereço dele.
    struct AnfitriaoDeTeste {
        avisos: tokio::net::UdpSocket,
        avisos_endereco: SocketAddr,
        server: std::net::UdpSocket,
        server_endereco: SocketAddr,
    }

    impl AnfitriaoDeTeste {
        async fn abrir() -> Self {
            let Ok(avisos) = tokio::net::UdpSocket::bind("127.0.0.1:0").await else {
                panic!("não deu para abrir a escuta de avisos de teste");
            };
            let Ok(avisos_endereco) = avisos.local_addr() else {
                panic!("a escuta de avisos de teste não tem endereço local");
            };
            let Ok(server) = std::net::UdpSocket::bind("127.0.0.1:0") else {
                panic!("não deu para abrir o socket do servidor de teste");
            };
            let Ok(server_endereco) = server.local_addr() else {
                panic!("o socket do servidor de teste não tem endereço local");
            };
            Self {
                avisos,
                avisos_endereco,
                server,
                server_endereco,
            }
        }

        /// Sobe o `atender` com estes sockets contra `ponto`. A tarefa volta
        /// para o teste poder abortá-la ao fim.
        fn subir(self, ponto: SocketAddr, marcas: Marcas) -> tokio::task::JoinHandle<()> {
            tokio::spawn(atender(
                self.avisos,
                Arc::new(self.server),
                ponto,
                self.avisos_endereco,
                marcas,
            ))
        }
    }

    #[tokio::test]
    async fn o_primeiro_registro_no_quarto_sai_na_subida() {
        // O defeito: `atender` consumia o primeiro tique do relógio antes do
        // laço, e o primeiro `MORO` saía quinze segundos depois da subida.
        //
        // E cada socket com a sua marca: a escuta com a da escuta, o servidor
        // com a do servidor. A escuta registrada como `anfitriao`, uma marca
        // igual para todo anfitrião do mundo, era o segundo defeito do link:
        // quem procura pergunta pela marca da impressão digital, e ninguém a
        // registrava.
        let (ponto, caderno) = ponto_que_anota(None).await;
        let anfitriao = AnfitriaoDeTeste::abrir().await;
        let (avisos_endereco, server_endereco) =
            (anfitriao.avisos_endereco, anfitriao.server_endereco);
        let marcas = marcas_da_impressao();

        let tarefa = anfitriao.subir(ponto, marcas.clone());
        let da_escuta = esperar_no_caderno(&caderno, Duration::from_secs(1), |pedido, de| {
            matches!(pedido, encontro::Pedido::Moro { marca } if *marca == marcas.escuta)
                && de == avisos_endereco
        })
        .await;
        let do_servidor = esperar_no_caderno(&caderno, Duration::from_secs(1), |pedido, de| {
            matches!(pedido, encontro::Pedido::Moro { marca } if *marca == marcas.servidor)
                && de == server_endereco
        })
        .await;
        tarefa.abort();

        assert!(
            da_escuta,
            "a escuta de avisos não se registrou no quarto com a marca dela ({}) no primeiro \
             segundo: quem perguntar pela escuta não a acha, e o LEVE cai num endereço de ontem",
            marcas.escuta
        );
        assert!(
            do_servidor,
            "o socket do servidor não se registrou no quarto com a marca dele ({}) no primeiro \
             segundo: quem voltar pela trilha não acha para onde conectar",
            marcas.servidor
        );
    }

    #[tokio::test]
    async fn o_eco_do_proprio_registro_nao_vira_furo_contra_si_mesmo() {
        // O `MORO` da escuta é respondido para a própria escuta, com a marca do
        // `MORO`. Se essa marca fosse a do aviso, que é a que `atender` confere
        // antes de furar, o anfitrião furaria o caminho para si mesmo a cada
        // reavivamento: um pacote a mais por tique, e uma entrada gasta na
        // janela de furos que é de quem está chegando.
        //
        // O ponto deste teste responde a tudo com um `AQUI` que aponta para
        // `alvo`, e não para quem perguntou. É o que deixa ver um furo que, com
        // um ponto de verdade, iria para a própria escuta sem deixar rastro.
        let Ok(alvo) = tokio::net::UdpSocket::bind("127.0.0.1:0").await else {
            panic!("não deu para abrir o alvo do furo de teste");
        };
        let Ok(alvo_endereco) = alvo.local_addr() else {
            panic!("o alvo do furo de teste não tem endereço local");
        };
        let (ponto, caderno) = ponto_que_anota(Some(alvo_endereco)).await;
        let anfitriao = AnfitriaoDeTeste::abrir().await;
        let avisos_endereco = anfitriao.avisos_endereco;

        let tarefa = anfitriao.subir(ponto, marcas_da_impressao());

        // O eco saiu: o ponto recebeu o registro da escuta e já respondeu.
        let ecoou = esperar_no_caderno(&caderno, Duration::from_secs(1), |pedido, de| {
            matches!(pedido, encontro::Pedido::Moro { .. }) && de == avisos_endereco
        })
        .await;
        let mut balde = [0_u8; encontro::TAMANHO];
        let furo =
            tokio::time::timeout(Duration::from_millis(600), alvo.recv_from(&mut balde)).await;
        tarefa.abort();

        assert!(
            ecoou,
            "a escuta não se registrou no quarto, e sem registro não há eco para medir: este \
             teste não diria nada"
        );
        assert!(
            furo.is_err(),
            "a resposta ao próprio registro passou no filtro de `atender` e virou FURO: a marca \
             da escuta é a do aviso, e o anfitrião fura o caminho para si mesmo a cada \
             reavivamento"
        );
    }

    /// Um ponto de encontro de teste que responde como o de verdade, no laço
    /// local.
    async fn ponto_que_responde() -> SocketAddr {
        let Ok(servico) = tokio::net::UdpSocket::bind("127.0.0.1:0").await else {
            panic!("o ponto de encontro de teste tem de abrir");
        };
        let Ok(onde) = servico.local_addr() else {
            panic!("o ponto de encontro de teste não tem endereço local");
        };
        tokio::spawn(async move {
            let mut balde = [0_u8; encontro::TAMANHO];
            while let Ok((lidos, de)) = servico.recv_from(&mut balde).await {
                if let Some(resposta) = balde.get(..lidos).and_then(|bytes| {
                    encontro::responder_em(bytes, de, encontro::Vizinhanca::TambemAqui)
                }) {
                    let _ = servico.send_to(&resposta.datagrama, resposta.destino).await;
                }
            }
        });
        onde
    }

    #[tokio::test]
    async fn largar_o_encontro_solta_o_socket_do_servidor() {
        // Largar um `Encontro` é o que acontece quando uma `Hospedagem` é
        // descartada sem `encerrar`. Sem `Drop`, o `JoinHandle` largado deixava
        // `atender` viva para sempre, com uma cópia do socket do servidor: a
        // porta ficava presa.
        let ponto = ponto_que_responde().await;
        let Ok(server) = std::net::UdpSocket::bind("127.0.0.1:0") else {
            panic!("não deu para abrir o socket do servidor de teste");
        };
        let server = Arc::new(server);
        let Some(convocacao) =
            Convocacao::para_servidor(Arc::clone(&server), IMPRESSAO, ponto.to_string())
        else {
            panic!("a impressão digital de teste não forma convocação");
        };
        let aberto = abrir(&convocacao).await;
        drop(convocacao);
        let degrau = match aberto {
            Ok(degrau) => degrau,
            Err(falha) => panic!("o ponto de teste responde, e o degrau 4 tinha de abrir: {falha}"),
        };
        assert_eq!(
            Arc::strong_count(&server),
            2,
            "a tarefa do degrau 4 devia segurar uma cópia do socket do servidor; sem ela este \
             teste não mede nada"
        );

        drop(degrau);
        // `abort` é pedido, não cumprido na hora: a tarefa some na próxima volta
        // do escalonador.
        let ate = tokio::time::Instant::now() + Duration::from_secs(1);
        while Arc::strong_count(&server) > 1 && tokio::time::Instant::now() < ate {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(
            Arc::strong_count(&server),
            1,
            "largar o Encontro sem `fechar` deixou a tarefa viva segurando o socket do servidor: a \
             porta fica presa, e hospedar de novo nela falha com «endereço já em uso»"
        );
    }

    /// Um `Write` que guarda o que o `tracing` escreve, para o teste ler depois.
    #[derive(Clone, Default)]
    struct Rastro(Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for Rastro {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .expect("o rastro trancou")
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Rastro {
        fn linhas_com(&self, trecho: &str) -> usize {
            self.texto()
                .lines()
                .filter(|linha| linha.contains(trecho))
                .count()
        }

        fn texto(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().expect("o rastro trancou")).into_owned()
        }

        /// Um rastro que guarda o que sai em `info` para cima nesta thread, até o
        /// guarda cair. É o nível do `seele.log`: uma linha em `debug` não conta.
        ///
        /// Só nesta thread: `set_default` fixa o `Subscriber` na thread corrente,
        /// e é nela que um `#[tokio::test]` de thread única roda tudo, inclusive
        /// as tarefas que ele sobe.
        fn de_info() -> (Self, tracing::subscriber::DefaultGuard) {
            let rastro = Self::default();
            let escritor = rastro.clone();
            let subscriber = tracing_subscriber::fmt()
                .with_writer(move || escritor.clone())
                .with_max_level(tracing::Level::INFO)
                .with_ansi(false)
                .without_time()
                .finish();
            (rastro, tracing::subscriber::set_default(subscriber))
        }
    }

    /// Um ponto que nenhum dos dois sockets de [`AnfitriaoDeTeste`] alcança: é
    /// IPv6, e eles são IPv4. O sistema recusa o envio aqui mesmo, e nenhum
    /// pacote sai desta máquina.
    fn ponto_que_o_sistema_recusa() -> SocketAddr {
        SocketAddr::from((Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1), PORTA_PADRAO))
    }

    #[tokio::test]
    async fn o_registro_que_deixa_de_sair_diz_no_log_uma_vez_e_diz_quando_volta() {
        // «O produto sabe e não conta»: o `MORO` da escuta ia com `let _ =`, e o
        // que sai pelo socket do servidor ia para um `debug!` que o `seele.log`
        // não grava. Um anfitrião que sumia do quarto sumia calado, e a pergunta
        // chegava dias depois, de quem não conseguia voltar pela lista.
        //
        // Nenhum pacote sai desta máquina: o ponto «mudo» é um que o sistema
        // recusa aqui mesmo, e o «bom» é um socket no laço local, que só recebe.
        let (rastro, _guarda) = Rastro::de_info();
        let anfitriao = AnfitriaoDeTeste::abrir().await;
        let marcas = marcas_da_impressao();
        let mudo = ponto_que_o_sistema_recusa();
        let Ok(ponto_bom) = std::net::UdpSocket::bind("127.0.0.1:0") else {
            panic!("não deu para abrir o ponto de teste");
        };
        let Ok(bom) = ponto_bom.local_addr() else {
            panic!("o ponto de teste não tem endereço local");
        };
        let mut registro = RegistroNoQuarto::default();

        for _ in 0..3 {
            let saiu = registrar(
                &anfitriao.avisos,
                &anfitriao.server,
                mudo,
                anfitriao.avisos_endereco,
                &marcas,
            )
            .await;
            assert!(
                saiu.is_err(),
                "o registro num ponto que o sistema recusa voltou como se tivesse saído: ou \
                 `registrar` engoliu a recusa (o defeito), ou este sistema aceitou mandar a um \
                 IPv6 por um socket IPv4 e o teste não tem falha para observar"
            );
            registro.anotar(mudo, saiu);
        }
        assert_eq!(
            rastro.linhas_com("deixou de sair"),
            1,
            "três registros seguidos não saíram, e o log (`info`, o nível que o seele.log grava) \
             não disse isso exatamente uma vez: ou o anfitrião some do quarto calado, ou a mesma \
             linha se repete a cada quinze segundos. Rastro: {}",
            rastro.texto()
        );
        assert!(
            rastro.texto().contains("2001:db8::1"),
            "a linha da falha não diz a que ponto se registrava. Rastro: {}",
            rastro.texto()
        );

        for _ in 0..2 {
            let saiu = registrar(
                &anfitriao.avisos,
                &anfitriao.server,
                bom,
                anfitriao.avisos_endereco,
                &marcas,
            )
            .await;
            assert!(
                saiu.is_ok(),
                "o registro no ponto do laço local não saiu: {saiu:?}"
            );
            registro.anotar(bom, saiu);
        }
        assert_eq!(
            rastro.linhas_com("voltou a sair"),
            1,
            "o registro voltou a sair, e o log não disse isso exatamente uma vez: quem lê não \
             sabe quando o anfitrião voltou ao quarto. Rastro: {}",
            rastro.texto()
        );
    }

    #[tokio::test]
    async fn atender_diz_no_log_quando_o_registro_da_subida_nao_sai() {
        // O teste de cima prova `registrar` e `RegistroNoQuarto` isolados, e um
        // par correto que `atender` não chamasse deixaria aquele teste verde e o
        // anfitrião sumindo do quarto calado do mesmo jeito. Este sobe o
        // `atender` de verdade: o primeiro tique sai na subida.
        let (rastro, _guarda) = Rastro::de_info();
        let tarefa = AnfitriaoDeTeste::abrir()
            .await
            .subir(ponto_que_o_sistema_recusa(), marcas_da_impressao());

        let ate = tokio::time::Instant::now() + Duration::from_secs(1);
        while rastro.linhas_com("deixou de sair") == 0 && tokio::time::Instant::now() < ate {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        tarefa.abort();
        assert_eq!(
            rastro.linhas_com("deixou de sair"),
            1,
            "o primeiro registro no quarto não saiu e `atender` não disse isso no log (`info`, o \
             nível que o seele.log grava): o anfitrião some do quarto calado. Rastro: {}",
            rastro.texto()
        );
    }
}
