//! O gerente de uma chegada: dar nome a cada etapa de uma conexão.
//!
//! Só existe do lado de **quem entra**. Quem hospeda sobe a escada do ADR 0022
//! uma vez, antes de existir par nenhum, e o que ela produz não é uma conexão —
//! é uma frase e uma lista de endereços para o `seele://`. Ciclo de vida com
//! começo, tentativas e fim só existe deste lado, e é por isso que as duas
//! metades não compartilham máquina de estados nenhuma: a costura entre elas
//! continua sendo o link e o `SEELE-ENC/1`.
//!
//! # O que este módulo conserta
//!
//! Um teste de campo com duas casas falhou e ninguém soube dizer **em que
//! ponto**, porque não havia ponto nomeado: quatro candidatos eram tentados em
//! série atrás de um spinner mudo, e o que sobrava no fim era um erro só — o de
//! um dos quatro. [`Etapa`] dá nome a cada instante dessa travessia e
//! [`Chegada::trilha`] os guarda em ordem, com o relógio, para que a pergunta
//! «tentei quatro candidatos, qual deu o quê» tenha resposta.
//!
//! A trilha **não acrescenta conhecimento**: todo endereço dentro dela já
//! estava no convite de quem a lê, o que mantém o custo de privacidade em zero.
//!
//! # Três estados que não existem aqui
//!
//! O desenho de fora que originou este ciclo assume ICE, e três dos estados
//! dele não descrevem nada que aconteça neste lado:
//!
//! - `DISCOVERING` e `CANDIDATES_FOUND` — quem entra não descobre candidato
//!   nenhum. Eles chegam prontos no `seele://`, já ordenados e já truncados; a
//!   descoberta é do outro lado e aconteceu antes deste processo existir.
//! - `NAT_TRAVERSAL_FAILED` e `DISCOVERY_FAILED` — o que se observa é «todos os
//!   candidatos falharam». Atribuir isso ao furo é chute, e quem responde por
//!   quê é o diagnóstico do `plug --rede`, que mede em vez de supor.
//!
//! O quarto, `PATH_ESTABLISHED`, era inafirmável enquanto nada deste lado
//! aprendesse que o furo abriu. Deixou de ser: quem entra **pode ler** o
//! datagrama `FURO`, porque ele vem do anfitrião e não do ponto de encontro — a
//! invariante do ADR 0022 é sobre o ponto de encontro. Ele virou
//! [`Etapa::CaminhoAberto`], com o que está escrito lá.
//!
//! # Uso único
//!
//! Uma [`Chegada`] atravessa uma vez e morre: [`Chegada::chegar`] consome o
//! objeto. Uma reconexão constrói outra, e é assim que ela passa a ter a lista
//! inteira de candidatos em vez do endereço único com que
//! [`crate::enlace::Enlace`] reconecta hoje.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use seele_proto::uri::Bilhete;
use tokio::sync::watch;

use crate::client::ConnectError;
use crate::enlace::{Destino, Enlace};
use crate::tofu::PinStore;

/// O endereço de enfeite dos exemplares de [`Etapa::TODAS`].
///
/// `0.0.0.0:0` de propósito: quem ler um destes num log tem de reconhecer na
/// hora que está olhando para um exemplar de variante, e não para um candidato
/// que alguém tentou.
const EXEMPLO: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0));

/// Onde uma chegada está.
///
/// Os nomes são **identificadores estáveis, nunca frases**, pela mesma regra de
/// `Degrau::nome()`: a frase que uma pessoa lê mora na casca — ADR 0012 e 0023
/// — e o Rust exporta o nome que a casca usa como chave. Renomear uma variante
/// daqui quebra a tela de todo mundo, e é para ser assim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Etapa {
    /// Ainda parada, com o convite lido e nada tentado.
    Parada {
        /// Quantos endereços o convite trouxe.
        candidatos: u8,
        /// O link trouxe `enc=` **e** o primeiro candidato tem impressão
        /// digital.
        ///
        /// É exatamente isso, e o nome diz as duas metades porque a bandeira
        /// não sabe mais do que elas. Em particular ela **não** promete que
        /// vai haver aviso: `crate::encontro::Batida::preparar` exige mais —
        /// impressão de pelo menos 16 caracteres, uma `Marca` que se deixe
        /// fazer com eles, um `bilhete.aviso()` que seja um endereço, o nome do
        /// ponto de encontro resolvido dentro do prazo, e um socket local que
        /// abra. Com o ponto de encontro fora do ar, ou com um nome que não
        /// resolve — os dois casos que a seção 1 do spec nomeia por escrito —
        /// esta bandeira continua verdadeira, [`Etapa::Avisando`] é publicada e
        /// nenhum datagrama sai.
        ///
        /// Isso não estraga nada, e é de propósito que fique escrito em vez de
        /// consertado aqui: nenhum endereço do convite depende do aviso, e a
        /// aresta que não existe (`Avisando → Desistiu`) é o que garante isso.
        /// Apertar a bandeira exigiria tentar `preparar` antes de publicar a
        /// etapa, o que é trabalho do passo seguinte da migração — quando o
        /// laço mudar de casa e esta camada passar a ver o envio.
        com_bilhete_e_impressao: bool,
    },
    /// Avisando o ponto de encontro de que estamos chegando.
    Avisando {
        /// O ponto de encontro, como o convite o escreveu: `host[:porta]`.
        ponto: String,
    },
    /// Um aperto de mão correndo contra um endereço do convite.
    Tentando {
        /// Qual da lista, contando do zero.
        candidato: u8,
        /// De quantos.
        de: u8,
        /// O endereço.
        onde: SocketAddr,
        /// Um `LEVE` saiu pelo ponto de encontro por causa **deste** candidato.
        ///
        /// A metade da informação de que [`Caminho`] é feito, e a única que não
        /// se pode ler do endereço: `EnderecoPublico` e `FuroDeNat` são os dois
        /// IPv4 público, e o que os separa é o aviso. Sem este campo a tabela
        /// não é computável e a tela acabaria adivinhando.
        ///
        /// Verdadeiro quando `crate::enlace` conseguiu mandar o datagrama por
        /// este candidato — nem o bilhete sozinho, nem a intenção: o envio.
        /// Falso no candidato que não precisa de furo, no convite sem `enc=`, e
        /// no aviso que o sistema recusou. Ver `avisar_pelo_candidato`, que é
        /// quem decide, e a bandeira de [`Etapa::Parada`], que promete bem
        /// menos que isto.
        avisou: bool,
    },
    /// Um `FURO` com a marca certa chegou: o caminho até aqui abriu.
    ///
    /// **Marca não é autenticação**, e isso fica escrito no tipo em vez de num
    /// documento: os primeiros dígitos de uma impressão digital que já viaja no
    /// link não provam nada sobre quem mandou o pacote. Esta etapa não decide
    /// para onde conectar nem dispensa conferência nenhuma do aperto de mão —
    /// ela só antecipa o **instante** da tentativa, encurtando a espera do furo
    /// quando ele chega antes dela.
    CaminhoAberto {
        /// De onde o furo veio.
        onde: SocketAddr,
    },
    /// Dentro: o aperto de mão terminou e há sessão.
    Dentro,
    /// Nenhum candidato entrou, e este é o motivo.
    ///
    /// Carrega o [`ConnectError`] que já existe, inteiro. Achatar `PinChanged`
    /// e `InviteMismatch` num «falhou» apagaria o alarme do ADR 0003: são os
    /// dois erros desta lista que **não são de rede**.
    Desistiu(ConnectError),
}

impl Etapa {
    /// Um exemplar de cada etapa que esta máquina pode publicar.
    ///
    /// # Por que uma lista, e não três
    ///
    /// Havia três: esta, o array do teste de conformidade e a lista do guarda
    /// da casca — as três escritas à mão, e **nenhuma ligada ao enum**. Uma
    /// variante nova atravessava o `seele-ffi` e caía no «falha que esta tela
    /// não sabe nomear» no meio de uma conexão que ia bem, sem que teste nenhum
    /// acendesse: o compilador cobra o braço de [`Etapa::nome`] e a travessia
    /// do `seele-ffi`, e não cobra lista nenhuma escrita à mão.
    ///
    /// As outras duas agora leem esta. A da casca chega por
    /// `seele_ffi::ConnectStage::todas`, porque o ADR 0002 não deixa
    /// `apps/seele-app` ver o núcleo — e é por isso que a versão anterior
    /// disto, que dizia existir «para a casca poder cobrar cobertura total»,
    /// não podia cumprir o que prometia.
    ///
    /// O buraco não fechou; ele passou a ser **um só**, e tem guarda:
    /// `estados.rs` confere que esta lista tem uma entrada por braço do `match`
    /// de [`Etapa::nome`], que é o que o compilador cobra de verdade. Uma
    /// variante nova que não entre aqui acende lá.
    ///
    /// Os valores são de exemplo e não significam nada — o que se lê deles é a
    /// variante.
    pub const TODAS: [Self; 6] = [
        Self::Parada {
            candidatos: 0,
            com_bilhete_e_impressao: false,
        },
        Self::Avisando {
            ponto: String::new(),
        },
        Self::Tentando {
            candidato: 0,
            de: 0,
            onde: EXEMPLO,
            avisou: false,
        },
        Self::CaminhoAberto { onde: EXEMPLO },
        Self::Dentro,
        Self::Desistiu(ConnectError::Unreachable),
    ];

    /// O nome estável que atravessa para a casca.
    #[must_use]
    pub fn nome(&self) -> &'static str {
        match self {
            Self::Parada { .. } => "Parada",
            Self::Avisando { .. } => "Avisando",
            Self::Tentando { .. } => "Tentando",
            Self::CaminhoAberto { .. } => "CaminhoAberto",
            Self::Dentro => "Dentro",
            Self::Desistiu(_) => "Desistiu",
        }
    }

    /// Se desta etapa se pode ir para a que tem este nome.
    ///
    /// A máquina inteira em uma função pura, para ela ser conferível sem socket
    /// nenhum. O destino entra como nome e não como valor porque a legalidade
    /// não depende do conteúdo do destino — depende de onde se está.
    ///
    /// # A ausência que é requisito: não existe `Avisando → Desistiu`
    ///
    /// Um ponto de encontro fora do ar, um nome que não resolve, um convite sem
    /// impressão digital — **nenhum deles pode reprovar uma chegada**, porque
    /// nenhum endereço do convite depende dele. O degrau 4 é o de cima da
    /// escada, e perdê-lo não perde os de baixo. Hoje isso é garantido por um
    /// prazo de 600 ms lá em `crate::encontro`; aqui é uma transição que não se
    /// pode escrever, que é mais barato de conferir e não depende do relógio.
    ///
    /// # A linha do spec que esta máquina passou a cobrar
    ///
    /// «Do **último** candidato para `Desistiu`». Ela era frouxa aqui, e a
    /// razão era de observação: [`Chegada::chegar`] via o primeiro candidato e
    /// o fim, e não os do meio, então o índice do último não existia para ser
    /// comparado. **Essa precondição foi cumprida** — o laço de
    /// [`crate::enlace::Enlace`] passou a contar cada candidato com o índice
    /// dele ([`crate::enlace::Tentativa`]) —, e a regra é cobrada:
    /// `Tentando{i} → Desistiu` exige `i + 1 == de`.
    ///
    /// O que ela pega é uma desistência anunciada com candidatos ainda por
    /// tentar, que é a forma que um passo perdido no meio do caminho toma
    /// quando ele chega aqui.
    ///
    /// # A linha que continua frouxa, e por um motivo que não é observação
    ///
    /// A tabela diz `Tentando|CaminhoAberto → Tentando{i+1}`. Aqui
    /// `Tentando → Tentando` vale com **qualquer** índice, inclusive o mesmo, e
    /// o mesmo para `CaminhoAberto → Tentando`. O motivo agora é outro: **o
    /// destino entra como nome e não como valor**, então o índice de destino
    /// não está nesta função para ser comparado com nada.
    ///
    /// E isso é a forma da função, não um acidente dela: a legalidade de sair
    /// de onde se está não depende do conteúdo de aonde se vai, e passar o
    /// destino inteiro faria o `match` casar sobre dois valores para cobrar uma
    /// única aresta. A conta `i + 1` é sequencial por construção — o laço conta
    /// com `enumerate` —, e quem a quebrasse quebraria antes o índice que a
    /// regra de cima já cobra.
    ///
    /// Nenhuma das duas afrouxa a ausência que é requisito, que é a de cima.
    #[must_use]
    pub fn transicao_legal(atual: &Self, para: &str) -> bool {
        match (atual, para) {
            // Só com `enc=` no link e impressão digital no primeiro
            // candidato, que é o que a bandeira sabe — e não mais que isso.
            // Ver o que ela promete, e o que ela não promete, em `Parada`.
            (
                Self::Parada {
                    com_bilhete_e_impressao: true,
                    ..
                },
                "Avisando",
            ) => true,
            // Sem candidato nenhum não há o que tentar. É a única desistência
            // que não passa por uma tentativa, e ela é honesta: um convite sem
            // endereço não tem aonde chegar.
            (Self::Parada { candidatos, .. }, "Tentando") => *candidatos > 0,
            (Self::Parada { candidatos: 0, .. }, "Desistiu") => true,
            (Self::Avisando { .. }, "Tentando") => true,
            // Só do último candidato. Uma desistência com endereços ainda por
            // tentar é a forma que um passo perdido toma quando chega aqui —
            // ver a seção sobre esta linha no doc.
            //
            // Em `u16` porque os dois campos saturam em `u8::MAX`: um convite de
            // mais de 255 endereços não existe, e `candidato + 1` em `u8` seria
            // um estouro à espera dele.
            (Self::Tentando { candidato, de, .. }, "Desistiu") => {
                u16::from(*candidato) + 1 == u16::from(*de)
            }
            (Self::Tentando { .. }, "CaminhoAberto" | "Tentando" | "Dentro") => true,
            (Self::CaminhoAberto { .. }, "Tentando" | "Dentro" | "Desistiu") => true,
            _ => false,
        }
    }
}

/// Por qual caminho a conversa saiu, depois que ela saiu.
///
/// Quatro nomes estáveis, no padrão de `Degrau::nome()` e pela mesma regra de
/// [`Etapa`]: a frase que uma pessoa lê mora na casca (ADR 0012 e 0023), e o
/// Rust exporta a chave.
///
/// **Não é a escada do ADR 0022, e não podia ser.** A escada tem cinco degraus
/// e é o que **quem hospeda** conseguiu anunciar, antes de existir par nenhum;
/// isto é por onde **quem entra** de fato passou, e só existe depois de haver
/// sessão. Dois dos nomes se repetem entre as duas listas — `FuroDeNat` e
/// `Ipv6Direto` — e são fatos diferentes sobre lados diferentes, o que é por que
/// a casca os arquiva em dicionários separados.
///
/// # De onde estes quatro saem
///
/// De duas coisas, e só delas, porque só elas quem entra sabe:
///
/// 1. **a forma do endereço que venceu** — privado, IPv6 global ou IPv4 público;
/// 2. **se um `LEVE` saiu por aquele candidato**, que é decisão desta camada e
///    está gravada em [`Etapa::Tentando`].
///
/// | o que venceu | `LEVE` saiu por ele? | nome |
/// |---|---|---|
/// | endereço privado | não | [`Caminho::RedeLocal`] |
/// | IPv6 global | — | [`Caminho::Ipv6Direto`] |
/// | IPv4 público | não | [`Caminho::EnderecoPublico`] |
/// | IPv4 público | **sim** | [`Caminho::FuroDeNat`] |
///
/// `EnderecoPublico` e `FuroDeNat` **não se distinguem pela forma do endereço**:
/// os dois são IPv4 público. O que os separa é o aviso.
///
/// # O grau de certeza, escrito aqui porque não é «prova»
///
/// Mandar um `LEVE` não prova que o furo abriu. A conexão pode ter subido por um
/// caminho que já estava aberto — uma porta mapeada no roteador, um NAT de cone
/// cheio, um endereço que nunca precisou de furo nenhum. O que `FuroDeNat`
/// afirma é o que se observou: **o candidato que venceu é público, e nós
/// avisamos por ele**. Evidência forte, e não prova.
///
/// É o **mesmo grau de certeza** com que o anfitrião declara `Degrau::FuroDeNat`
/// do outro lado — e é por isso que a frase daquele degrau diz «deve funcionar»
/// e não «funciona». Quem provaria é o datagrama `FURO`, e quem o lê é o
/// `seele-udp`, que vem depois do portão de campo e pode nem ser construído.
/// Enquanto ele não existir, isto é o que há; e isto é muito melhor que
/// «DIRECT», que apagaria a distinção inteira — em `FuroDeNat` a conversa **é**
/// direta, e alguém soube que ela existe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caminho {
    /// O endereço que venceu não é roteável fora daqui: a mesma rede, ou a
    /// mesma máquina.
    ///
    /// # Loopback conta — **na forma escrita**, e não na mapeada
    ///
    /// `127.0.0.1:8383` sai como `RedeLocal`. `[::ffff:127.0.0.1]:8383` **não**:
    /// ele sai como [`Caminho::EnderecoPublico`], ou como
    /// [`Caminho::FuroDeNat`] se o link trouxer `enc=`.
    ///
    /// Não é descuido, é consistência comprada de propósito. [`Caminho::de`]
    /// faz **a mesma** pergunta que decide se o `LEVE` sai — `e_publico`, que
    /// testa loopback no endereço como ele está escrito. Canonizar aqui daria
    /// uma classificação mais bonita e duas respostas diferentes para o mesmo
    /// endereço: o laço mandaria o aviso e a tela diria que ninguém precisou
    /// dele. Entre a pureza e as duas metades concordarem, concordar vale mais.
    ///
    /// O preço não custa segurança, e é o mesmo argumento que `e_publico` já
    /// carrega: o candidato decide apenas **se** o `LEVE` sai, nunca **para
    /// onde** o anfitrião fura — o destino do furo é `bilhete.aviso()`, fixado
    /// em `crate::encontro::Batida::preparar`. Um candidato mal classificado não
    /// redireciona pacote contra terceiro nenhum.
    ///
    /// # O caso real em que isso aparece
    ///
    /// Um servidor cujo ponto de encontro roda **na mesma máquina**, atrás de um
    /// socket de pilha dupla — o arranjo de quem hospeda na máquina de
    /// desenvolvimento. O ponto observa a origem de quem bateu como
    /// `::ffff:127.0.0.1` e publica isso no convite;
    /// `alcance::anunciar_com_porta` empurra o endereço refletido para a lista
    /// conferindo só a família, sem filtro de loopback nenhum.
    ///
    /// Quem entra por aquele link vê no rodapé **`FURO DE NAT`** sobre uma
    /// conversa que nunca saiu da máquina. A frase não é falsa no que ela
    /// afirma — o candidato é público para esta classificação, e nós avisamos
    /// por ele —, e é o mesmo grau de certeza do resto: evidência do que se
    /// observou, e não prova do que aconteceu. Em campo, entre duas casas, o
    /// arranjo não existe.
    RedeLocal,
    /// Um endereço IPv6 global respondeu, e não houve NAT no caminho.
    ///
    /// Vale **com ou sem** aviso, e a assimetria com o IPv4 é deliberada: um
    /// `LEVE` sai por qualquer candidato público, IPv6 inclusive, mas o que o
    /// anfitrião abre lá é buraco de firewall e não tradução de endereço.
    /// Chamar isso de `FuroDeNat` poria a palavra NAT onde não houve NAT.
    Ipv6Direto,
    /// Um IPv4 público respondeu sem que precisássemos avisar ninguém.
    ///
    /// O degrau que uma porta mapeada no roteador produz, e o que um servidor com
    /// endereço próprio produz sempre.
    EnderecoPublico,
    /// Um IPv4 público respondeu, e avisamos o ponto de encontro por ele.
    ///
    /// A leitura honesta está no cabeçalho do enum: evidência forte, não prova.
    FuroDeNat,
}

impl Caminho {
    /// Um exemplar de cada caminho, para quem precisa cobrir todos.
    ///
    /// Pela mesma regra de [`Etapa::TODAS`], e pelo mesmo motivo: a casca cobra
    /// cobertura de frase contra uma lista, e uma lista escrita à mão que o
    /// compilador não liga ao enum deixa uma variante nova atravessar calada.
    /// O guarda desta é `estados.rs`, que a confere contra os braços do `match`
    /// de [`Caminho::nome`] — que é o que o compilador cobra de verdade.
    pub const TODOS: [Self; 4] = [
        Self::RedeLocal,
        Self::Ipv6Direto,
        Self::EnderecoPublico,
        Self::FuroDeNat,
    ];

    /// O nome estável que atravessa para a casca.
    #[must_use]
    pub fn nome(&self) -> &'static str {
        match self {
            Self::RedeLocal => "RedeLocal",
            Self::Ipv6Direto => "Ipv6Direto",
            Self::EnderecoPublico => "EnderecoPublico",
            Self::FuroDeNat => "FuroDeNat",
        }
    }

    /// A tabela do cabeçalho, em código.
    ///
    /// `avisou` é lido **depois** da forma, e só quando ela é IPv4 público: é a
    /// única linha da tabela em que a forma não decide sozinha.
    #[must_use]
    pub fn de(onde: SocketAddr, avisou: bool) -> Self {
        // A mesma pergunta que decide se um `LEVE` sai, feita pela mesma
        // função: privado, loopback, sem destino e multicast são todos «nada
        // atravessou a internet». Duplicar o critério aqui deixaria os dois
        // discordarem, e o desacordo apareceria como uma tela dizendo
        // `EnderecoPublico` sobre um `192.168.x.x`.
        if !crate::enlace::e_publico(onde.ip()) {
            return Self::RedeLocal;
        }
        // Na forma canônica: um `::ffff:203.0.113.7` é IPv4 público escrito de
        // outro jeito, e é **a forma comum** — é assim que o ponto de encontro
        // reflete a origem de quem está atrás de pilha dupla. Sem canonizar,
        // todo candidato refletido viraria `Ipv6Direto`.
        match onde.ip().to_canonical() {
            IpAddr::V6(_) => Self::Ipv6Direto,
            IpAddr::V4(_) if avisou => Self::FuroDeNat,
            IpAddr::V4(_) => Self::EnderecoPublico,
        }
    }
}

/// Por qual caminho esta trilha saiu, se ela saiu.
///
/// `None` é a resposta certa em três casos, e nos três a casca não escreve nada:
/// uma trilha vazia, uma chegada que não chegou, e uma que chegou sem nenhuma
/// tentativa registrada. **Inventar um nome quando não se sabe é a mentira
/// confiante que o ADR 0022 existe para não produzir** — e «DIRECT», o nome que
/// se inventaria, é justamente o que apaga a distinção que importa.
///
/// Lê a **última** [`Etapa::Tentando`] antes de [`Etapa::Dentro`], que é a que
/// venceu: as anteriores falharam, e o laço só publica um passo por candidato.
#[must_use]
pub fn caminho(trilha: &[Passo]) -> Option<Caminho> {
    let dentro = trilha
        .iter()
        .rposition(|passo| passo.etapa == Etapa::Dentro)?;
    trilha
        .iter()
        .take(dentro)
        .rev()
        .find_map(|passo| match &passo.etapa {
            Etapa::Tentando { onde, avisou, .. } => Some(Caminho::de(*onde, *avisou)),
            Etapa::Parada { .. }
            | Etapa::Avisando { .. }
            | Etapa::CaminhoAberto { .. }
            | Etapa::Dentro
            | Etapa::Desistiu(_) => None,
        })
}

/// Uma etapa e quando ela aconteceu.
///
/// Um por transição, **nunca por retentativa interna**: o aviso que sai três
/// vezes por candidato é um passo só, porque quem lê a trilha está perguntando
/// por onde a conexão passou e não quantos pacotes saíram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passo {
    /// Onde a chegada estava.
    pub etapa: Etapa,
    /// Quanto tempo depois do começo desta chegada.
    pub em: Duration,
}

/// Uma travessia, do convite lido até a sessão — ou até o motivo de não haver.
///
/// De uso único: [`Chegada::chegar`] consome o objeto. Ver o cabeçalho do
/// módulo.
pub struct Chegada {
    /// Os endereços do convite, na ordem em que o anfitrião os ordenou.
    destinos: Vec<Destino>,
    /// O ponto de encontro do link, quando ele trouxe um.
    bilhete: Option<Bilhete>,
    /// O relógio de que a trilha mede as distâncias.
    nascida: Instant,
    /// Por onde esta chegada passou, em ordem.
    trilha: Vec<Passo>,
    /// Para quem acompanha ao vivo.
    estado: watch::Sender<Etapa>,
}

impl std::fmt::Debug for Chegada {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Chegada")
            .field("candidatos", &self.destinos.len())
            .field("bilhete", &self.bilhete.is_some())
            .field("passos", &self.trilha.len())
            .finish()
    }
}

impl Chegada {
    /// Uma chegada parada, com o convite já lido.
    ///
    /// Nada de rede acontece aqui: o relógio começa a correr e a trilha ganha o
    /// primeiro passo, que é [`Etapa::Parada`].
    #[must_use]
    pub fn nova(destinos: Vec<Destino>, bilhete: Option<Bilhete>) -> Self {
        let candidatos = u8::try_from(destinos.len()).unwrap_or(u8::MAX);
        // As duas metades que a bandeira nomeia, e nada além delas: se o
        // aviso vai mesmo sair é `Batida::preparar` quem decide, e ela decide
        // depois daqui. Ver `Etapa::Parada`.
        let com_bilhete_e_impressao = bilhete.is_some()
            && destinos
                .first()
                .is_some_and(|destino| destino.impressao_esperada.is_some());
        let parada = Etapa::Parada {
            candidatos,
            com_bilhete_e_impressao,
        };
        let (estado, _) = watch::channel(parada.clone());
        Self {
            destinos,
            bilhete,
            nascida: Instant::now(),
            trilha: vec![Passo {
                etapa: parada,
                em: Duration::ZERO,
            }],
            estado,
        }
    }

    /// Onde esta chegada está, agora e a cada mudança.
    ///
    /// Um `watch` e não uma fila: quem desenha quer o estado atual, e uma tela
    /// que ficou para trás não tem nada a ganhar desenhando as etapas antigas
    /// uma a uma. Quem quer a história inteira lê a trilha, que a guarda.
    #[must_use]
    pub fn acompanhar(&self) -> watch::Receiver<Etapa> {
        self.estado.subscribe()
    }

    /// Por onde esta chegada passou até agora.
    #[must_use]
    pub fn trilha(&self) -> &[Passo] {
        &self.trilha
    }

    /// Atravessa: tenta os candidatos do convite e devolve a sessão.
    ///
    /// # O que esta versão vê
    ///
    /// O laço de candidatos continua em [`crate::enlace::Enlace`], inteiro e sem
    /// uma linha movida — com o aviso por candidato, a repetição em tarefa de
    /// fundo e o prazo curto do candidato distante. O que mudou é que ele
    /// **conta o que está fazendo**: cada candidato, no instante em que a
    /// tentativa dele começa, e com o aviso já decidido. Antes desta tarefa esta
    /// camada observava o primeiro candidato e o fim, e os do meio atravessavam
    /// sem nome — a trilha dizia menos do que a pergunta que ela existe para
    /// responder.
    ///
    /// Nada aqui é inventado: um passo por candidato que o laço de fato tentou,
    /// publicado quando ele o tenta. O canal é ouvido **junto** com a conexão, e
    /// esvaziado depois dela, para que o último candidato — o que ganhou, ou o
    /// que perdeu por último — esteja na trilha antes de [`Etapa::Dentro`] ou de
    /// [`Etapa::Desistiu`].
    ///
    /// # Por que o `avisou` só existe aqui
    ///
    /// Porque é o laço quem manda o `LEVE`, e a resposta só existe depois do
    /// envio. Esta camada publicava a primeira tentativa **antes** de chamar o
    /// laço, então um `avisou` escrito ali seria sempre um chute — e é dele que
    /// [`Caminho`] é feito.
    ///
    /// # Errors
    ///
    /// [`Frustrada`], que é o [`ConnectError`] de sempre **mais a trilha**. O
    /// motivo escolhido é o mesmo de [`crate::enlace::Enlace::conectar_entre`]:
    /// o de quem respondeu, se algum respondeu.
    pub async fn chegar(
        mut self,
        chave: SigningKey,
        pins: Arc<dyn PinStore>,
    ) -> Result<Chegado, Frustrada> {
        let de = u8::try_from(self.destinos.len()).unwrap_or(u8::MAX);
        if self.destinos.is_empty() {
            // Um convite sem endereço nenhum. Ninguém chama assim, e desistir
            // com o motivo na mão é melhor que devolver uma chegada que não
            // consta de lugar nenhum.
            return Err(self.desistir(ConnectError::Unreachable));
        }

        // O aviso é publicado sob as mesmas condições em que ele sai — ver a
        // bandeira em `Etapa::Parada`. Que o `LEVE` de um candidato específico
        // tenha saído ou não é assunto do laço, e é ele quem responde, um
        // candidato por vez, no `avisou` de cada `Tentando`.
        if let Some(ponto) = self.ponto_a_avisar() {
            self.marcar(Etapa::Avisando { ponto });
        }

        let destinos = std::mem::take(&mut self.destinos);
        let bilhete = self.bilhete.clone();
        let (contando, mut tentativas) = tokio::sync::mpsc::unbounded_channel();
        let conectando = Enlace::conectar_entre_observado(destinos, bilhete, chave, pins, contando);
        tokio::pin!(conectando);

        let resultado = loop {
            tokio::select! {
                // `biased` de propósito, e é sobre **quando** o passo é
                // publicado e não sobre se ele é: o esvaziamento lá embaixo
                // garante que nenhum se perde. Com a escolha aleatória do
                // `select!`, um candidato que entrou na fila enquanto a conexão
                // ficava pronta sairia às vezes ao vivo e às vezes em bloco no
                // fim — e a etapa ao vivo existe para acompanhar a travessia,
                // não para resumi-la depois.
                biased;
                Some(tentativa) = tentativas.recv() => self.tentando(tentativa, de),
                resultado = &mut conectando => break resultado,
            }
        };
        // O que o laço mandou no mesmo instante em que terminou. Sem isto, o
        // candidato que venceu ficaria de fora da trilha justamente por ter
        // vencido — e é dele que sai o [`Caminho`].
        while let Ok(tentativa) = tentativas.try_recv() {
            self.tentando(tentativa, de);
        }

        match resultado {
            Ok(enlace) => {
                self.marcar(Etapa::Dentro);
                Ok(Chegado {
                    enlace,
                    trilha: self.trilha,
                })
            }
            Err(erro) => Err(self.desistir(erro)),
        }
    }

    /// Publica um candidato que o laço acabou de começar a tentar.
    fn tentando(&mut self, tentativa: crate::enlace::Tentativa, de: u8) {
        self.marcar(Etapa::Tentando {
            candidato: tentativa.candidato,
            de,
            onde: tentativa.onde,
            avisou: tentativa.avisou,
        });
    }

    /// O ponto de encontro que vai ser avisado, se algum vai.
    fn ponto_a_avisar(&self) -> Option<String> {
        let bilhete = self.bilhete.as_ref()?;
        let primeiro = self.destinos.first()?;
        primeiro.impressao_esperada.as_ref()?;
        Some(bilhete.ponto.clone())
    }

    /// Fecha a chegada no motivo, e entrega a trilha a quem vai lê-la.
    fn desistir(mut self, motivo: ConnectError) -> Frustrada {
        self.marcar(Etapa::Desistiu(motivo.clone()));
        Frustrada {
            motivo,
            trilha: self.trilha,
        }
    }

    /// Publica uma transição, se ela existir nesta máquina.
    ///
    /// A conferência é aqui, e não num auxiliar que só o teste chama, porque é
    /// esta linha que faz de [`Etapa::transicao_legal`] uma regra em vez de uma
    /// opinião: uma aresta que a máquina não tem não entra na trilha nem sai no
    /// `watch`, e o passo que falta acende o teste que espera por ele.
    fn marcar(&mut self, etapa: Etapa) {
        let legal = self
            .trilha
            .last()
            .is_some_and(|passo| Etapa::transicao_legal(&passo.etapa, etapa.nome()));
        if !legal {
            tracing::error!(
                para = etapa.nome(),
                "uma transição que a máquina de estados da chegada não tem"
            );
            return;
        }
        self.trilha.push(Passo {
            etapa: etapa.clone(),
            em: self.nascida.elapsed(),
        });
        // `send` só falha quando não há ninguém ouvindo, que é o caso comum: a
        // trilha é o registro, e o `watch` é a comodidade de quem desenha.
        let _ = self.estado.send(etapa);
    }
}

/// Uma chegada que chegou: a sessão, e por onde ela passou.
///
/// Simétrico de [`Frustrada`], e pela mesma razão. A trilha sobrevivia à falha e
/// **morria com o sucesso**, o que deixava a pergunta «por qual caminho esta
/// conversa saiu» sem lugar de onde ser respondida — a [`Chegada`] é de uso
/// único e já morreu quando este valor existe. Quem quer o nome pronto chama
/// [`Chegado::caminho`]; quem quer a história inteira lê a trilha.
#[derive(Debug)]
pub struct Chegado {
    /// A sessão.
    pub enlace: Enlace,
    /// Por onde a chegada passou, em ordem, terminando em [`Etapa::Dentro`].
    pub trilha: Vec<Passo>,
}

impl Chegado {
    /// Por qual caminho esta conversa saiu.
    ///
    /// `None` quando a trilha não sabe dizer — e a casca, então, não escreve
    /// nada. Ver [`caminho`].
    #[must_use]
    pub fn caminho(&self) -> Option<Caminho> {
        caminho(&self.trilha)
    }
}

/// Uma chegada que não chegou: o motivo, e por onde ela passou.
///
/// A trilha é lida daqui e não do objeto porque a [`Chegada`] é de uso único e
/// já morreu quando este valor existe. Ela sobrevive à falha de propósito —
/// «tentei quatro candidatos, o primeiro deu prazo esgotado em 4 s, o quarto
/// recusou» é o dado que faltou quando o teste de campo das duas casas falhou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frustrada {
    motivo: ConnectError,
    trilha: Vec<Passo>,
}

impl Frustrada {
    /// Por que não deu.
    #[must_use]
    pub fn motivo(&self) -> &ConnectError {
        &self.motivo
    }

    /// Por onde a chegada passou antes de acabar.
    #[must_use]
    pub fn trilha(&self) -> &[Passo] {
        &self.trilha
    }
}

impl From<Frustrada> for ConnectError {
    /// Para quem só quer o erro de sempre.
    fn from(frustrada: Frustrada) -> Self {
        frustrada.motivo
    }
}

impl std::fmt::Display for Frustrada {
    /// Para log e para `Error`, nunca para uma pessoa: a frase é da casca.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{:?} depois de {} passos",
            self.motivo,
            self.trilha.len()
        )
    }
}

impl std::error::Error for Frustrada {}

#[cfg(test)]
mod tests {
    use super::*;

    fn destino(porta: u16) -> Destino {
        Destino {
            servidor: SocketAddr::from(([127, 0, 0, 1], porta)),
            nome_tls: "localhost".into(),
            chave_do_pin: format!("127.0.0.1:{porta}"),
            apelido: "pessoa".into(),
            segredo: None,
            impressao_esperada: Some("0123456789abcdef0123".into()),
        }
    }

    fn bilhete() -> Bilhete {
        let Ok(bilhete) = Bilhete::novo("encontro.exemplo:8384", "203.0.113.7:8383") else {
            panic!("o bilhete de teste deixou de ser um bilhete");
        };
        bilhete
    }

    #[test]
    fn uma_transicao_que_a_maquina_nao_tem_nao_entra_na_trilha() {
        // A conferência de `transicao_legal` tem de estar **na fiação**, e não
        // só num auxiliar que o teste chama: uma regra que só o teste consulta
        // é uma regra que a produção não tem.
        let mut chegada = Chegada::nova(vec![destino(1)], None);
        chegada.marcar(Etapa::Dentro);

        assert_eq!(
            chegada.trilha().len(),
            1,
            "`Parada → Dentro` não existe, e ainda assim foi publicada: {:?}",
            chegada.trilha()
        );
        assert!(matches!(
            chegada.trilha().first().map(|passo| &passo.etapa),
            Some(Etapa::Parada { .. })
        ));
    }

    #[test]
    fn um_convite_sem_impressao_digital_nao_promete_aviso_nenhum() {
        // Sem a impressão digital não sai datagrama nenhum — a marca do aviso é
        // feita dela. Publicar `Avisando` aqui seria a tela afirmando um degrau
        // que não vai acontecer, que é o mesmo defeito do log que dizia
        // «avisamos o ponto de encontro» com o envio recusado.
        let mut cego = destino(1);
        cego.impressao_esperada = None;
        let chegada = Chegada::nova(vec![cego], Some(bilhete()));

        assert!(chegada.ponto_a_avisar().is_none());
        assert!(
            matches!(
                chegada.trilha().first().map(|passo| &passo.etapa),
                Some(Etapa::Parada {
                    com_bilhete_e_impressao: false,
                    ..
                })
            ),
            "{:?}",
            chegada.trilha()
        );

        let com_impressao = Chegada::nova(vec![destino(1)], Some(bilhete()));
        assert_eq!(
            com_impressao.ponto_a_avisar().as_deref(),
            Some("encontro.exemplo:8384")
        );
    }

    #[test]
    fn a_trilha_mede_o_tempo_desde_o_comeco_e_nao_desde_o_passo_anterior() {
        // «O primeiro deu prazo esgotado em 4 s» é uma frase sobre o começo da
        // chegada. Medir desde o passo anterior daria a mesma trilha com outro
        // significado, e ninguém notaria lendo os números.
        let mut chegada = Chegada::nova(vec![destino(1)], None);
        std::thread::sleep(Duration::from_millis(20));
        chegada.marcar(Etapa::Tentando {
            candidato: 0,
            de: 1,
            onde: SocketAddr::from(([127, 0, 0, 1], 1)),
            avisou: false,
        });
        std::thread::sleep(Duration::from_millis(20));
        chegada.marcar(Etapa::Desistiu(ConnectError::Unreachable));

        let Some(primeiro) = chegada.trilha().get(1) else {
            panic!("a tentativa não entrou na trilha");
        };
        let Some(fim) = chegada.trilha().get(2) else {
            panic!("a desistência não entrou na trilha");
        };
        assert!(
            fim.em >= primeiro.em + Duration::from_millis(15),
            "o segundo passo teria de estar ao menos 20 ms depois do primeiro, e \
             está em {:?} contra {:?}",
            fim.em,
            primeiro.em
        );
    }

    /// Um passo qualquer, sem relógio: o que se lê dele é a etapa.
    fn passo(etapa: Etapa) -> Passo {
        Passo {
            etapa,
            em: Duration::ZERO,
        }
    }

    /// Uma tentativa contra `onde`, com ou sem aviso.
    fn tentando(onde: &str, avisou: bool) -> Passo {
        let Ok(onde) = onde.parse::<SocketAddr>() else {
            panic!("o endereço de teste `{onde}` não é um endereço");
        };
        passo(Etapa::Tentando {
            candidato: 0,
            de: 1,
            onde,
            avisou,
        })
    }

    #[test]
    fn sem_saber_o_caminho_a_casca_nao_escreve_nada() {
        // «DIRECT» não é dizível: a escada tem cinco degraus, e a distinção que
        // essa palavra apagaria é justamente a que importa — em `FuroDeNat` a
        // conversa é direta **e** alguém soube que ela existe.
        //
        // Inventar um nome quando não se sabe é a mentira confiante que o ADR
        // 0022 existe para não produzir. `None` é a resposta certa, e a casca
        // cala.
        assert_eq!(caminho(&[]), None);
        assert_eq!(
            caminho(&[passo(Etapa::Dentro)]),
            None,
            "chegar não diz por onde; quem diz é a tentativa que venceu"
        );
        assert_eq!(
            caminho(&[
                tentando("203.0.113.7:8383", true),
                passo(Etapa::Desistiu(ConnectError::Unreachable)),
            ]),
            None,
            "uma chegada que não chegou não saiu por caminho nenhum"
        );
    }

    #[test]
    fn a_forma_do_endereco_decide_o_caminho_e_o_aviso_desempata() {
        // A tabela da seção 5 do spec, inteira, e a linha que ela existe para
        // ter: `EnderecoPublico` e `FuroDeNat` são **os dois** IPv4 público, e o
        // que os separa é o aviso. Um `Caminho::de` que ignorasse `avisou`
        // continuaria certo em três das quatro linhas.
        let caminho_de = |onde: &str, avisou: bool| {
            let Ok(onde) = onde.parse::<SocketAddr>() else {
                panic!("o endereço de teste `{onde}` não é um endereço");
            };
            Caminho::de(onde, avisou).nome()
        };

        assert_eq!(caminho_de("192.168.1.5:8383", false), "RedeLocal");
        assert_eq!(caminho_de("[fd00::1]:8383", false), "RedeLocal");
        assert_eq!(caminho_de("127.0.0.1:8383", false), "RedeLocal");
        assert_eq!(caminho_de("[2001:db8::1]:8383", false), "Ipv6Direto");
        assert_eq!(caminho_de("203.0.113.7:8383", false), "EnderecoPublico");
        assert_eq!(caminho_de("203.0.113.7:8383", true), "FuroDeNat");

        // A forma mapeada é **o caso comum** do candidato refletido: é assim que
        // um ponto de encontro atrás de pilha dupla enxerga quem bateu. Sem
        // canonizar, todo candidato refletido viraria `Ipv6Direto` e a linha
        // acima nunca aconteceria em campo.
        assert_eq!(caminho_de("[::ffff:203.0.113.7]:8383", true), "FuroDeNat");
        assert_eq!(caminho_de("[::ffff:192.168.1.5]:8383", false), "RedeLocal");

        // Um IPv6 global avisado continua sendo IPv6 direto: o `LEVE` sai por
        // qualquer candidato público, mas o que o anfitrião abre lá é buraco de
        // firewall, e não tradução de endereço nenhuma.
        assert_eq!(caminho_de("[2001:db8::1]:8383", true), "Ipv6Direto");

        // O loopback **mapeado** não é rede local, e a assimetria com a linha
        // do `127.0.0.1` acima é uma decisão que fica presa aqui em vez de ficar
        // só no doc. `Caminho::de` faz a mesma pergunta que decide se o `LEVE`
        // sai, e ela testa loopback na forma escrita; canonizar daria uma
        // classificação mais bonita e duas respostas diferentes para o mesmo
        // endereço — o laço avisando e a tela dizendo que ninguém precisou.
        //
        // Quem paga é o arranjo em que o ponto de encontro roda na mesma
        // máquina, atrás de pilha dupla. Ver `Caminho::RedeLocal`.
        assert_eq!(
            caminho_de("[::ffff:127.0.0.1]:8383", false),
            "EnderecoPublico"
        );
        assert_eq!(caminho_de("[::ffff:127.0.0.1]:8383", true), "FuroDeNat");
    }

    #[test]
    fn uma_desistencia_com_candidatos_por_tentar_nao_e_uma_desistencia() {
        // A linha do spec que esta tarefa passou a cobrar. Ela era frouxa
        // enquanto esta camada via o primeiro candidato e o fim; o laço passou a
        // contar cada um com o índice dele, e a precondição que o doc nomeava
        // deixou de ser hipótese.
        //
        // O que ela pega: um passo perdido no meio do caminho chega aqui como
        // uma desistência anunciada com endereços ainda por tentar.
        let tentando = |candidato: u8, de: u8| Etapa::Tentando {
            candidato,
            de,
            onde: EXEMPLO,
            avisou: false,
        };

        assert!(
            !Etapa::transicao_legal(&tentando(0, 3), "Desistiu"),
            "desistir no primeiro de três é desistir com dois endereços que \
             ninguém tentou"
        );
        assert!(!Etapa::transicao_legal(&tentando(1, 3), "Desistiu"));
        assert!(
            Etapa::transicao_legal(&tentando(2, 3), "Desistiu"),
            "o último candidato falhou e a chegada não pôde acabar"
        );
        assert!(
            Etapa::transicao_legal(&tentando(0, 1), "Desistiu"),
            "um convite de um endereço só desiste no primeiro, que é o último"
        );

        // As outras saídas de uma tentativa não dependem do índice: qualquer
        // candidato pode ser o que vence, e o próximo pode ser tentado de
        // qualquer um.
        for saida in ["Dentro", "Tentando", "CaminhoAberto"] {
            assert!(
                Etapa::transicao_legal(&tentando(0, 3), saida),
                "`Tentando → {saida}` deixou de valer no primeiro candidato"
            );
        }
    }

    #[test]
    fn o_caminho_e_o_da_tentativa_que_venceu_e_nao_o_da_primeira() {
        // O defeito que esta ordem evita, e que a versão anterior desta camada
        // teria produzido: ela publicava **só** a primeira tentativa, então uma
        // conexão que subisse pelo terceiro candidato seria nomeada pelo
        // primeiro. Quatro endereços num convite, e o primeiro é o da rede de
        // casa — a tela diria `RedeLocal` sobre uma conversa que atravessou a
        // internet.
        let trilha = vec![
            passo(Etapa::Parada {
                candidatos: 3,
                com_bilhete_e_impressao: true,
            }),
            tentando("192.168.1.5:8383", false),
            tentando("203.0.113.7:8383", true),
            passo(Etapa::Dentro),
        ];

        assert_eq!(caminho(&trilha).map(|c| c.nome()), Some("FuroDeNat"));
    }

    #[tokio::test]
    async fn um_candidato_que_falha_sem_ceder_a_vez_ainda_entra_na_trilha() {
        // O passo que se perde sem o esvaziamento do canal depois do laço, e a
        // única forma de perdê-lo: o `select!` só volta a olhar a fila quando a
        // conexão cede a vez, e uma tentativa que falha **sem ceder** leva o
        // laço inteiro ao fim numa poltrona só. O candidato é contado, o
        // `select!` quebra no resultado, e a `Tentando` fica na fila.
        //
        // Um nome TLS com espaço é exatamente esse caso: o quinn recusa antes de
        // mandar pacote nenhum, e `Endpoint::connect` devolve erro na hora. É o
        // mesmo truque de `furo.rs`, usado lá para medir o que o laço faz quando
        // uma tentativa acaba depressa.
        //
        // Sem este teste a linha do esvaziamento é uma guarda que nunca dispara,
        // e uma guarda que nunca dispara não guarda propriedade nenhuma — é o
        // argumento que este arquivo já faz sobre o `max` que não existe em
        // `gravar_jitter_de_chegada`.
        let mut impossivel = destino(1);
        impossivel.nome_tls = "nome inválido com espaço".into();
        let chegada = Chegada::nova(vec![impossivel], None);

        let Err(frustrada) = chegada
            .chegar(
                SigningKey::from_bytes(&[7; 32]),
                Arc::new(crate::tofu::MemoryPinStore::new()),
            )
            .await
        else {
            panic!("um nome TLS inválido deixou alguém entrar");
        };

        assert!(
            frustrada
                .trilha()
                .iter()
                .any(|passo| matches!(passo.etapa, Etapa::Tentando { .. })),
            "o candidato foi tentado e não consta da trilha: {:?}",
            frustrada.trilha()
        );
    }

    #[tokio::test]
    async fn quem_acompanha_ve_a_etapa_mudar() {
        // O `watch` é a metade que a tela lê. Ele nasce em `Parada` e anda com
        // a trilha; um `send` que sumisse deixaria o spinner mudo que esta
        // tarefa existe para acabar.
        let mut chegada = Chegada::nova(vec![destino(1)], None);
        let mut olhos = chegada.acompanhar();
        assert!(matches!(&*olhos.borrow_and_update(), Etapa::Parada { .. }));

        chegada.marcar(Etapa::Tentando {
            candidato: 0,
            de: 1,
            onde: SocketAddr::from(([127, 0, 0, 1], 1)),
            avisou: false,
        });
        assert!(olhos.has_changed().unwrap_or(false));
        assert!(matches!(
            &*olhos.borrow(),
            Etapa::Tentando { candidato: 0, .. }
        ));
    }
}
