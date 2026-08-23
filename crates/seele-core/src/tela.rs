//! O transporte do compartilhamento de tela.
//!
//! Tudo aqui sai do §3 de
//! `docs/superpowers/specs/2026-08-22-compartilhamento-de-tela-design.md`, que
//! é o único pedaço daquela spec **medido** antes de virar decisão. Os números
//! citados nos comentários abaixo são de `spikes/tela-no-transporte`, e o
//! `README` dele traz a tabela inteira.
//!
//! # As quatro decisões, e o que cada uma custou para descobrir
//!
//! 1. **O vídeo vai num fluxo unidirecional QUIC da conexão que já existe**
//!    ([`Transmissao::abrir`]). Não em datagrama: `send_datagram` põe voz e
//!    vídeo na **mesma fila FIFO** do `quinn-proto`, que descarta o mais velho
//!    quando enche — 16,1% da voz perdida e 2,16 s de atraso com o buffer
//!    padrão de 1 MiB, e **98,1% descartada** ao encolher o buffer para 32 KiB,
//!    porque aí os pedaços de vídeo enchem a fila entre dois quadros de voz.
//!    Com o vídeo num fluxo a perda de voz é 0,1%, e não é sorte: o
//!    `quinn-proto` escreve os quadros `DATAGRAM` **antes** dos `STREAM` em
//!    cada pacote. E não numa segunda conexão: duas conexões QUIC competem no
//!    mesmo gargalo, e a segunda devolve 4 ms por um aperto de mão inteiro.
//! 2. **O que protege a voz é o teto de banda, não o transporte**
//!    ([`TetoDeVideo`]). A prioridade do QUIC evita a perda e não faz nada
//!    contra o atraso: com o vídeo solto, a voz chega inteira e chega com
//!    225,7 ms em vez de 21,7, porque a fila de 262 ms do gargalo fica cheia o
//!    tempo todo — e essa fila não está nesta máquina, está no meio do
//!    caminho, onde prioridade de frame não alcança. O que alcança é não
//!    enchê-la.
//! 3. **O quadro-chave é metade do que sobra depois do teto**
//!    ([`Transmissao::enviar_quadro`]). Espalhar o **mesmo** quadro-chave em
//!    vez de despejá-lo num tique leva o p95 da voz de 78,9 para 35,8 ms e o
//!    pior caso de 114,9 para 42,7 ms, com o mesmo bitrate entregue. Custo:
//!    nenhum.
//! 4. **A voz nunca cede à tela.** Quem baixa resolução e quem para é o vídeo.
//!    É o critério de aceite do ciclo, e está escrito como teste — veja
//!    `a_voz_nunca_cede_a_tela` no fim deste arquivo.
//!
//! # Por que o enquadramento de quadro mora aqui e não em `seele-proto`
//!
//! O §3.6 da spec para no cabeçalho de abertura ([`seele_proto::screen`]) e não
//! diz o que separa um quadro do outro dentro do fluxo. Isto é essa peça, e ela
//! **deveria** morar ao lado do cabeçalho: é formato de fio, e formato de fio é
//! de `seele-proto`. Está aqui porque a tarefa que a escreveu não é dona
//! daquele crate, e mudá-lo por fora seria decidir sozinho uma coisa de dois
//! donos. Mover é uma linha de `pub use` no dia em que alguém puder.
//!
//! # `quinn-proto` fica onde está
//!
//! §3.5, e vale independentemente de tela: **`quinn-proto` 0.11.17 aborta o
//! processo no primeiro datagrama que estoura o buffer de envio** — o caminho
//! de descarte desconta `payload_bytes` duas vezes, o `usize` dá a volta, e o
//! `expect` seguinte estoura. O `Cargo.lock` trava 0.11.16 de propósito. É o
//! caminho por onde a **voz** sai hoje: basta a subida sumir por dois segundos
//! para o processo morrer em vez de perder quadros. Não subir sem conferir se
//! foi consertado, e no dia de subir, um teste que encha o buffer de propósito.

use std::time::{Duration, Instant};

use seele_proto::screen::{ScreenError, ScreenHeader, SCREEN_HEADER_LEN};
use seele_proto::sync_ratio::SyncBand;
use thiserror::Error;

// ---------------------------------------------------------------------------
// O teto de banda
// ---------------------------------------------------------------------------

/// Que fração do caminho medido o vídeo pode ocupar, em por cento.
///
/// **60, e é medida e não gosto.** Com o vídeo pedindo 1200 kbps num caminho de
/// 2000 — 60% —, a voz volta para 23,1 ms de p50 e 0% de perda; solto, ela vai
/// a 225,7 ms no mesmo cano. É o único ponto em que o spike viu a voz de volta à
/// linha de base, e o §8 pergunta 1 diz o que ainda não se sabe dele: Wi-Fi ruim
/// tem perda esporádica e atraso que anda sozinho, e nenhum dos dois estava na
/// prova.
///
/// O que sobra — os outros 40% — é [`TetoDeVideo::reserva_da_voz`], e é a razão
/// de este número ser uma fração e não um valor de configuração: um teto fixo
/// num caminho estreito é um teto que come a voz, e num caminho largo é um teto
/// que desperdiça tela.
pub const FRACAO_DO_CAMINHO: u32 = 60;

/// O caminho que se assume enquanto ninguém mediu, em bits por segundo.
///
/// **Isto é uma hipótese, e está escrita como hipótese de propósito.** O §8
/// pergunta 2 continua aberta — *«como se mede o caminho quando ninguém está
/// enchendo?»* — e o produto hoje não tem resposta: o sinal da voz diz que está
/// bom a 40 kbps, e não diz quanto cabe. Subir devagar até doer é o que todo
/// mundo faz e é o que faz a voz doer.
///
/// Então se assume o caminho sobre o qual as duas provas rodaram, 2000 kbps de
/// subida, que dá o teto de 1200 kbps que `spikes/tela-no-codec` usou em todas
/// as linhas. Assumir o cano da prova é a única suposição com número atrás;
/// qualquer outra seria inventada. Quem tiver menos que isso descobre pela
/// única medida que o produto de fato faz — o sinal da voz —, e é o que
/// [`TetoDeVideo::teto`] usa para baixar e para parar.
pub const CAMINHO_DA_PROVA_BPS: u32 = 2_000_000;

/// Abaixo deste teto o compartilhamento **para**, em bits por segundo.
///
/// O §2 pede piso com nome: *«se o encoder não sustenta nem o piso, o
/// compartilhamento para, com motivo enumerado. Degradar para sempre é como um
/// instrumento falso: consultado justamente quando algo deu errado.»*
///
/// **De onde sai o número, e o que nele é extrapolação.** `spikes/tela-no-codec`
/// mediu 540p — o piso da lista que o §5 fechou — gastando 796 kbps a 30
/// quadros. A faixa automática desce até 5 quadros (`PISO_DE_QUADROS` do
/// `seele-video`), e a conta ingênua daria 796/6 ≈ 133 kbps. Ingênua porque bits
/// não escalam com quadros: o quadro-chave custa o mesmo e o conteúdo parado
/// também. 200 kbps é essa conta com margem, e **não foi medida** — nenhuma
/// linha do spike rodou abaixo de 1200 kbps de teto. É o número que fica até
/// alguém medir; o que não pode faltar é o piso existir.
pub const PISO_DE_BANDA_BPS: u32 = 200_000;

/// Por que o vídeo não está saindo.
///
/// Enumerado, como `specs/02-protocolo.md` manda: quem recebe isto tem de poder
/// escrever a frase na língua da pessoa, e uma string de erro não deixa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum MotivoDeParada {
    /// O sinal da voz caiu para [`SyncBand::Critical`].
    ///
    /// §3.2: *«quando o sinal cai de faixa, quem baixa é o vídeo; se continuar
    /// caindo, quem para é o vídeo»*. Uma conversa com a tela travando é o
    /// produto funcionando; uma conversa picotando porque alguém abriu a tela é
    /// o produto quebrado.
    #[error("the voice signal is critical; the screen gives way")]
    SinalCritico,
    /// O que sobrou do caminho não sustenta nem [`PISO_DE_BANDA_BPS`].
    #[error("the ceiling fell below the {PISO_DE_BANDA_BPS}-bps floor")]
    AbaixoDoPiso,
}

/// O teto de banda do vídeo neste instante.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Teto {
    /// Pode transmitir, a no máximo estes bits por segundo.
    Bps(u32),
    /// Não pode transmitir, e a razão é dizível.
    Parado(MotivoDeParada),
}

impl Teto {
    /// Os bits por segundo, ou zero quando o vídeo está parado.
    ///
    /// Existe para a aritmética de quem compara tetos. **Não** serve para
    /// decidir se transmite: zero e parado são a mesma conta e frases
    /// diferentes, e é a frase que a pessoa lê.
    #[must_use]
    pub const fn bps(self) -> u32 {
        match self {
            Self::Bps(bps) => bps,
            Self::Parado(_) => 0,
        }
    }
}

/// O teto do vídeo, pendurado no sinal que a voz já calcula.
///
/// § 3.2, regra 2: *«quem mede o caminho é a voz, que já mede»*. O produto
/// calcula RTT, jitter e perda por conexão e os transforma em Taxa de
/// Sincronização (ADR 0024); o teto do vídeo pendura nesse número em vez de
/// abrir um segundo medidor que discordaria do primeiro no primeiro dia ruim.
///
/// # O tempo não entra aqui
///
/// Nada nesta estrutura lê relógio nem guarda histórico: ela é uma conta sobre
/// o que se sabe agora. Quem suaviza é a própria [`seele_proto::sync_ratio`],
/// com o α ≈ 0,2 que `specs/02-protocolo.md` fixa — suavizar duas vezes seria
/// pôr o teto atrás do sinal que ele existe para seguir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TetoDeVideo {
    caminho_bps: u32,
    escolha_bps: Option<u32>,
}

impl Default for TetoDeVideo {
    fn default() -> Self {
        Self::novo()
    }
}

impl TetoDeVideo {
    /// O teto de quem ainda não mediu o caminho: o cano da prova.
    #[must_use]
    pub const fn novo() -> Self {
        Self {
            caminho_bps: CAMINHO_DA_PROVA_BPS,
            escolha_bps: None,
        }
    }

    /// O teto sobre um caminho de subida conhecido, em bits por segundo.
    ///
    /// **Nada no produto chama isto ainda**, e é honesto que assim seja: o §8
    /// pergunta 2 é a pergunta de onde este número viria. Existe para o dia em
    /// que existir, e para que a fração seja fração de algo em vez de virar um
    /// literal escondido.
    #[must_use]
    pub const fn com_caminho(caminho_bps: u32) -> Self {
        Self {
            caminho_bps,
            escolha_bps: None,
        }
    }

    /// A escolha de quem compartilha, que é **teto e nunca piso** (§5).
    ///
    /// A regra que não se negocia: o que a pessoa escolhe é o máximo, e o
    /// sistema continua livre para ficar abaixo. Se virasse piso, a regra de
    /// aceite do §3.2 cairia — alguém escolhe 1080p60 numa subida de 2 Mbps, o
    /// vídeo insiste, e a conversa fica impossível **por causa da tela**. Aí o
    /// produto fica pior com o recurso do que sem ele.
    #[must_use]
    pub const fn com_escolha(mut self, escolha_bps: Option<u32>) -> Self {
        self.escolha_bps = escolha_bps;
        self
    }

    /// O que fica para a voz, em bits por segundo, aconteça o que acontecer.
    ///
    /// **Este número não depende da faixa**, e é isso que a frase *«a voz nunca
    /// cede à tela»* quer dizer em aritmética: quando o sinal piora, o que muda
    /// é o teto do vídeo. A reserva da voz é o que sobra do caminho depois de
    /// [`FRACAO_DO_CAMINHO`] e não encolhe nunca.
    #[must_use]
    pub const fn reserva_da_voz(&self) -> u32 {
        self.caminho_bps - self.teto_da_faixa_nominal()
    }

    /// 60% do caminho: o teto de quem está com o sinal nominal.
    const fn teto_da_faixa_nominal(&self) -> u32 {
        // Em `u64` porque `caminho_bps × 60` estoura `u32` a partir de uns
        // 71 Mbit/s, que é uma fibra doméstica comum. Um teto que dá a volta
        // vira um teto minúsculo, e o defeito apareceria só na casa boa.
        ((self.caminho_bps as u64 * FRACAO_DO_CAMINHO as u64) / 100) as u32
    }

    /// O teto agora, dada a faixa em que o sinal da voz está.
    ///
    /// As três saídas são as três frases do §3.2, nesta ordem:
    ///
    /// - [`SyncBand::Nominal`] — o vídeo tem [`FRACAO_DO_CAMINHO`] do caminho;
    /// - [`SyncBand::Degraded`] — **quem baixa é o vídeo**, e baixa pela
    ///   metade. A metade não foi medida: o que foi medido é o 60% na faixa
    ///   nominal. Metade é o menor passo que ainda **é** um passo — um corte de
    ///   10% seria indistinguível do ruído do próprio encoder, que já descarta
    ///   16% dos quadros em 1080p por conta própria;
    /// - [`SyncBand::Critical`] — **quem para é o vídeo**, com motivo.
    ///
    /// E, por baixo das três, a escolha da pessoa e o piso.
    #[must_use]
    pub fn teto(&self, faixa: SyncBand) -> Teto {
        let nominal = self.teto_da_faixa_nominal();
        let da_faixa = match faixa {
            SyncBand::Nominal => nominal,
            SyncBand::Degraded => nominal / 2,
            SyncBand::Critical => return Teto::Parado(MotivoDeParada::SinalCritico),
        };
        // O mínimo entre o que o caminho aguenta e o que a pessoa pediu. Os
        // dois são teto; quem manda é o menor, sempre.
        let teto = match self.escolha_bps {
            Some(escolha) => da_faixa.min(escolha),
            None => da_faixa,
        };
        if teto < PISO_DE_BANDA_BPS {
            return Teto::Parado(MotivoDeParada::AbaixoDoPiso);
        }
        Teto::Bps(teto)
    }
}

// ---------------------------------------------------------------------------
// O orçamento de bytes do fluxo
// ---------------------------------------------------------------------------

/// O orçamento de bytes de uma transmissão, em balde de fichas.
///
/// **Não é o limitador do vídeo — é a rede de segurança dele.** Quem faz o
/// vídeo caber no teto é o controle de taxa do OpenH264, e ele é bom nisso: no
/// teto de 1200 kbps ele descarta 16% dos próprios quadros em 1080p para não
/// estourar. Este balde existe para o caso em que ele estoura assim mesmo,
/// porque um byte escrito num fluxo QUIC é um byte na fila do gargalo, e a fila
/// do gargalo é exatamente o que o §3.2 mediu custando 200 ms de voz.
///
/// Duplicado do `crate::taxa::Balde` do `seele-server` de propósito, como
/// `crate::frame` é duplicado: o ADR 0002 impede o cliente e o daemon de
/// dividirem um crate de transporte, e quarenta linhas de balde custam menos
/// que um crate que os dois dependeriam e nenhum seria dono.
///
/// O tempo entra por parâmetro, como em [`crate::battery`] e em
/// `seele_server::taxa`: é o que torna testável o comportamento no limite sem
/// um único `sleep`.
#[derive(Debug, Clone, Copy)]
struct Balde {
    /// Rajada máxima, em bytes.
    capacidade: f64,
    /// Reposição, em bytes por segundo.
    por_segundo: f64,
    fichas: f64,
    ultima: Instant,
}

impl Balde {
    /// Um balde cheio, dimensionado para um teto em bits por segundo.
    ///
    /// Cheio, e não vazio: o primeiro quadro de uma transmissão não deve
    /// esperar. Capacidade de um segundo de orçamento porque é a unidade em que
    /// o teto é dito, e porque um quadro-chave de 1080p — 65 KiB, o maior que
    /// `spikes/tela-no-codec` mediu — cabe em menos da metade dela a 1200 kbps.
    fn novo(teto_bps: u32, agora: Instant) -> Self {
        let por_segundo = f64::from(teto_bps) / 8.0;
        Self {
            capacidade: por_segundo,
            por_segundo,
            fichas: por_segundo,
            ultima: agora,
        }
    }

    fn repor(&mut self, agora: Instant) {
        let decorrido = agora.saturating_duration_since(self.ultima).as_secs_f64();
        self.fichas = (self.fichas + decorrido * self.por_segundo).min(self.capacidade);
        self.ultima = agora;
    }

    /// Gasta `bytes` fichas, se houver todas. Tudo ou nada: meio quadro
    /// autorizado não é autorização nenhuma, e o outro meio não tem para onde
    /// ir num fluxo ordenado.
    fn gastar(&mut self, bytes: usize, agora: Instant) -> bool {
        self.repor(agora);
        let custo = bytes as f64;
        if self.fichas >= custo {
            self.fichas -= custo;
            true
        } else {
            false
        }
    }
}

// ---------------------------------------------------------------------------
// O enquadramento de quadro dentro do fluxo
// ---------------------------------------------------------------------------

/// Bytes de cabeçalho na frente de cada quadro codificado.
///
/// Um byte de tipo e quatro de tamanho. Ao contrário do
/// [`SCREEN_HEADER_LEN`], que sai uma vez por transmissão, este sai trinta
/// vezes por segundo: cinco bytes são 150 B/s, que somem ao lado dos 150 kB/s
/// do teto, e valem o que compram — um receptor que sabe onde o quadro acaba
/// sem parsear NAL nenhuma.
pub const CABECALHO_DE_QUADRO_LEN: usize = 5;

/// Maior quadro codificado que este build carrega, em bytes.
///
/// `specs/08-seguranca.md`: o tamanho é conferido **antes** de qualquer
/// alocação. Ler um tamanho de 4 GiB de um par e reservar por ele é a negação
/// de serviço mais velha que existe.
///
/// 512 KiB é oito vezes o maior quadro que alguém mediu — o quadro-chave de
/// 1080p, 65 KiB em `spikes/tela-no-codec`. Folga para o encoder ter um dia
/// ruim, e ainda três ordens de grandeza abaixo do que dói.
pub const MAX_QUADRO_LEN: usize = 512 * 1024;

/// Em quantas fatias um quadro-chave sai (§3.3).
///
/// Quatro, e o número vem da forma da medida e não de gosto: o quadro-chave de
/// 1080p é **quatro vezes** um quadro comum (65 KiB contra ~5 KiB no teto de
/// 1200 kbps), então quatro fatias é o que o faz caber no mesmo tique que um
/// quadro comum cabe. Espalhar assim leva o p95 da voz de 78,9 para 35,8 ms e o
/// pior caso de 114,9 para 42,7 ms, **com o mesmo bitrate entregue**: não se
/// manda menos, manda-se em quatro tiques.
pub const FATIAS_DO_QUADRO_CHAVE: usize = 4;

/// Prioridade do fluxo de tela, abaixo de tudo o mais que este cliente escreve.
///
/// O controle é 1 e as transferências são −1 (`crate::client`). A tela é −2, e
/// a ordem importa menos do que parece: o §3.2 é explícito em que **prioridade
/// dentro do QUIC não alcança a fila do gargalo**, que é onde a voz sofre. Isto
/// só arruma a ordem de saída desta máquina, e o que arruma a voz é o teto.
pub const PRIORIDADE_DA_TELA: i32 = -2;

/// Escreve o cabeçalho de um quadro nos primeiros [`CABECALHO_DE_QUADRO_LEN`]
/// bytes.
fn escrever_cabecalho_de_quadro(chave: bool, tamanho: u32) -> [u8; CABECALHO_DE_QUADRO_LEN] {
    let mut bytes = [0_u8; CABECALHO_DE_QUADRO_LEN];
    // Big-endian, pelo motivo que `seele_proto::media` já dá: é o que todo
    // protocolo de mídia em tempo real escreve, então uma captura aberta no
    // Wireshark se lê do jeito que um engenheiro espera.
    let tamanho = tamanho.to_be_bytes();
    bytes[0] = u8::from(chave);
    bytes[1] = tamanho[0];
    bytes[2] = tamanho[1];
    bytes[3] = tamanho[2];
    bytes[4] = tamanho[3];
    bytes
}

// ---------------------------------------------------------------------------
// Erros
// ---------------------------------------------------------------------------

/// Por que uma transmissão não abriu, não escreveu ou não leu.
///
/// Enumerado por `specs/02-protocolo.md`, e as duas metades de rede vêm como
/// texto de propósito: os erros do `quinn` são tipos do `quinn`, e devolvê-los
/// inteiros poria a versão de um crate de transporte na API pública deste — que
/// é a mesma razão pela qual [`crate::FlowControl`] copia quatro contadores em
/// vez de reexportar `quinn::ConnectionStats`.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ErroDeTela {
    /// O cabeçalho de abertura não serve.
    #[error("screen header refused: {0}")]
    Cabecalho(#[from] ScreenError),
    /// O par anunciou um quadro maior do que este build carrega.
    #[error("peer announced a {len}-byte picture, over the {MAX_QUADRO_LEN}-byte limit")]
    QuadroGrandeDemais {
        /// O tamanho anunciado.
        len: usize,
    },
    /// O par anunciou um quadro vazio.
    ///
    /// Recusado com o mesmo fôlego que o grande demais, pelo motivo que
    /// `ScreenHeader::check` dá sobre um lado de zero: é muito mais vezes uma
    /// captura que falhou do que uma escolha, e não há quadro atrás dele de
    /// qualquer jeito.
    #[error("peer announced an empty picture")]
    QuadroVazio,
    /// A conexão, o fluxo ou a leitura acabaram.
    #[error("screen stream: {0}")]
    Fluxo(String),
}

// ---------------------------------------------------------------------------
// O lado de quem compartilha
// ---------------------------------------------------------------------------

/// O que aconteceu com um quadro entregue a [`Transmissao::enviar_quadro`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Envio {
    /// Saiu inteiro.
    Enviado,
    /// É quadro-chave, e a primeira fatia saiu. O resto sai nos próximos
    /// tiques, uma fatia por chamada. Ver [`FATIAS_DO_QUADRO_CHAVE`].
    Espalhando,
    /// Não saiu, e o motivo é dizível.
    Descartado(MotivoDeDescarte),
}

/// Por que um quadro não saiu.
///
/// **Descartar é a política, não a falha.** É a mesma decisão que
/// `specs/03-audio.md` já tomou para o áudio e que o §1 repete para a captura:
/// um quadro velho entregue tarde é pior que um quadro perdido, e uma fila de
/// quadros de 1080p come memória depressa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum MotivoDeDescarte {
    /// O quadro não cabia no que resta do orçamento deste segundo.
    #[error("the picture did not fit the ceiling's budget")]
    AcimaDoTeto,
    /// Um quadro-chave ainda está saindo em fatias.
    ///
    /// Enquanto ele sai, nada mais pode ser escrito: o fluxo é uma sequência
    /// ordenada de bytes, e um quadro comum escrito no meio das fatias sairia
    /// **dentro** do quadro-chave. Descartar é a saída certa e barata — quem
    /// recebe não teria o que fazer com esses quadros de qualquer jeito, porque
    /// eles predizem justamente do quadro-chave que ainda não chegou.
    #[error("a key frame is still being spread over the stream")]
    QuadroChaveEmVoo,
    /// O quadro passa de [`MAX_QUADRO_LEN`].
    #[error("the picture is over the {MAX_QUADRO_LEN}-byte limit")]
    GrandeDemais,
    /// O quadro estava vazio.
    #[error("there was no picture")]
    Vazio,
}

/// O que falta escrever de um quadro-chave espalhado.
#[derive(Debug)]
struct ChaveEmVoo {
    bytes: Vec<u8>,
    escrito: usize,
    fatia: usize,
}

/// Uma transmissão de tela saindo desta máquina.
///
/// Um fluxo unidirecional por transmissão, aberto por quem compartilha, na
/// conexão QUIC que já existe (§3.1 e §3.6).
#[derive(Debug)]
pub struct Transmissao {
    fluxo: quinn::SendStream,
    cabecalho: ScreenHeader,
    balde: Balde,
    em_voo: Option<ChaveEmVoo>,
    enviados: u64,
    descartados: u64,
    bytes_enviados: u64,
}

impl Transmissao {
    /// Abre o fluxo e escreve o cabeçalho de abertura.
    ///
    /// `teto_bps` é o que [`TetoDeVideo::teto`] devolveu. Uma transmissão só se
    /// abre com [`Teto::Bps`]: [`Teto::Parado`] não é um teto baixo, é a
    /// resposta de que não se transmite agora — e um valor que serve para os
    /// dois casos seria a interface ensinando a ignorar a diferença.
    ///
    /// # Errors
    ///
    /// [`ErroDeTela::Cabecalho`] se o cabeçalho não passa em
    /// `ScreenHeader::check`, ou [`ErroDeTela::Fluxo`] se a conexão não abre.
    pub async fn abrir(
        conexao: &quinn::Connection,
        cabecalho: ScreenHeader,
        teto_bps: u32,
        agora: Instant,
    ) -> Result<Self, ErroDeTela> {
        let mut abertura = [0_u8; SCREEN_HEADER_LEN];
        // Antes de abrir o fluxo, e não depois: uma resolução que a prova não
        // cobre não vale um fluxo aberto que só será fechado.
        cabecalho.encode(&mut abertura)?;

        let mut fluxo = conexao
            .open_uni()
            .await
            .map_err(|erro| ErroDeTela::Fluxo(erro.to_string()))?;
        // Antes do primeiro byte, para que nem a abertura passe na frente do
        // controle. `set_priority` só falha em fluxo já fechado, que aqui
        // acabou de nascer — e mesmo assim não vale derrubar a transmissão por
        // uma prioridade não aplicada.
        let _ = fluxo.set_priority(PRIORIDADE_DA_TELA);
        fluxo
            .write_all(&abertura)
            .await
            .map_err(|erro| ErroDeTela::Fluxo(erro.to_string()))?;

        Ok(Self {
            fluxo,
            cabecalho,
            balde: Balde::novo(teto_bps, agora),
            em_voo: None,
            enviados: 0,
            descartados: 0,
            bytes_enviados: 0,
        })
    }

    /// O cabeçalho com que esta transmissão abriu.
    #[must_use]
    pub const fn cabecalho(&self) -> &ScreenHeader {
        &self.cabecalho
    }

    /// Troca o teto, mantendo o que já foi gasto neste segundo.
    ///
    /// Chamado toda vez que a faixa do sinal muda. **Não repõe o balde**: um
    /// teto novo que devolvesse fichas faria uma queda de faixa liberar uma
    /// rajada, que é o oposto exato do que a queda de faixa quer dizer.
    pub fn ajustar_teto(&mut self, teto_bps: u32, agora: Instant) {
        self.balde.repor(agora);
        let por_segundo = f64::from(teto_bps) / 8.0;
        self.balde.capacidade = por_segundo;
        self.balde.por_segundo = por_segundo;
        self.balde.fichas = self.balde.fichas.min(por_segundo);
    }

    /// Entrega um quadro codificado ao fluxo, ou diz por que não.
    ///
    /// Um quadro comum sai inteiro. Um quadro-chave sai em
    /// [`FATIAS_DO_QUADRO_CHAVE`] fatias, uma por chamada, e é aí que o §3.3
    /// acontece: o cabeçalho do quadro anuncia o tamanho **inteiro** e as
    /// fatias vão preenchendo, porque um fluxo QUIC é uma sequência ordenada de
    /// bytes e quem lê só termina quando a última fatia chega. Não há bandeira
    /// de continuação nem remontagem do outro lado — espalhar é uma decisão de
    /// **quando escrever**, e não um formato.
    ///
    /// # Errors
    ///
    /// [`ErroDeTela::Fluxo`] quando o fluxo morreu. Um quadro recusado não é
    /// erro: volta como [`Envio::Descartado`], porque descartar é a política.
    pub async fn enviar_quadro(
        &mut self,
        bytes: &[u8],
        chave: bool,
        agora: Instant,
    ) -> Result<Envio, ErroDeTela> {
        if let Some(mut voando) = self.em_voo.take() {
            let acabou = self.escrever_fatia(&mut voando).await?;
            if !acabou {
                self.em_voo = Some(voando);
            }
            self.descartados += 1;
            return Ok(Envio::Descartado(MotivoDeDescarte::QuadroChaveEmVoo));
        }

        if bytes.is_empty() {
            self.descartados += 1;
            return Ok(Envio::Descartado(MotivoDeDescarte::Vazio));
        }
        if bytes.len() > MAX_QUADRO_LEN {
            self.descartados += 1;
            return Ok(Envio::Descartado(MotivoDeDescarte::GrandeDemais));
        }

        let total = CABECALHO_DE_QUADRO_LEN.saturating_add(bytes.len());
        if !self.balde.gastar(total, agora) {
            self.descartados += 1;
            return Ok(Envio::Descartado(MotivoDeDescarte::AcimaDoTeto));
        }

        // `u32` cabe: `MAX_QUADRO_LEN` é 512 KiB e já foi conferido acima.
        let tamanho = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
        self.escrever(&escrever_cabecalho_de_quadro(chave, tamanho))
            .await?;
        self.enviados += 1;
        self.bytes_enviados += total as u64;

        if !chave {
            self.escrever(bytes).await?;
            return Ok(Envio::Enviado);
        }

        let mut voando = ChaveEmVoo {
            bytes: bytes.to_vec(),
            escrito: 0,
            fatia: 0,
        };
        let acabou = self.escrever_fatia(&mut voando).await?;
        if acabou {
            // Cabe numa fatia só: um quadro-chave de 540p a 8 quadros pode ser
            // menor que quatro pedaços úteis. Espalhar quatro bytes em quatro
            // escritas não protege voz nenhuma.
            return Ok(Envio::Enviado);
        }
        self.em_voo = Some(voando);
        Ok(Envio::Espalhando)
    }

    /// Escreve a próxima fatia. Devolve `true` quando não sobra nada.
    async fn escrever_fatia(&mut self, voando: &mut ChaveEmVoo) -> Result<bool, ErroDeTela> {
        let total = voando.bytes.len();
        voando.fatia += 1;
        // A última fatia leva o resto, para que arredondamento não deixe bytes
        // órfãos: quem lê espera exatamente `tamanho` bytes e ficaria pendurado
        // para sempre por causa de uma divisão.
        let fim = if voando.fatia >= FATIAS_DO_QUADRO_CHAVE {
            total
        } else {
            (total.div_ceil(FATIAS_DO_QUADRO_CHAVE) * voando.fatia).min(total)
        };
        let inicio = voando.escrito;
        let pedaco = voando.bytes.get(inicio..fim).unwrap_or_default().to_vec();
        self.escrever(&pedaco).await?;
        voando.escrito = fim;
        Ok(fim >= total)
    }

    async fn escrever(&mut self, bytes: &[u8]) -> Result<(), ErroDeTela> {
        self.fluxo
            .write_all(bytes)
            .await
            .map_err(|erro| ErroDeTela::Fluxo(erro.to_string()))
    }

    /// Quantos quadros saíram, quantos foram descartados, quantos bytes foram.
    #[must_use]
    pub const fn contagem(&self) -> (u64, u64, u64) {
        (self.enviados, self.descartados, self.bytes_enviados)
    }

    /// Fecha o fluxo, dizendo a quem recebe que a transmissão acabou.
    ///
    /// O fim do fluxo é a segunda maneira de dizer «parei», e a de controle
    /// (`ClientMessage::StopScreenShare`) é a primeira. As duas existem porque
    /// uma delas — esta — também acontece quando a máquina simplesmente some, e
    /// o §3.6 quer que a sala consiga distinguir «ela parou de compartilhar» de
    /// «o enlace dela caiu».
    pub fn encerrar(mut self) {
        let _ = self.fluxo.finish();
    }
}

// ---------------------------------------------------------------------------
// O lado de quem assiste
// ---------------------------------------------------------------------------

/// Um quadro codificado que chegou inteiro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuadroRecebido {
    /// Se dá para começar a decodificar por ele.
    pub chave: bool,
    /// Os bytes, em Annex-B, como o encoder do outro lado os produziu.
    pub bytes: Vec<u8>,
}

/// Uma transmissão de tela chegando nesta máquina.
#[derive(Debug)]
pub struct Recepcao {
    fluxo: quinn::RecvStream,
    cabecalho: ScreenHeader,
}

impl Recepcao {
    /// Aceita o próximo fluxo unidirecional e lê o cabeçalho de abertura.
    ///
    /// # Errors
    ///
    /// [`ErroDeTela::Cabecalho`] para um cabeçalho malformado — versão
    /// estranha, fonte ou codec desconhecidos, resolução fora do teto — e
    /// [`ErroDeTela::Fluxo`] quando a conexão acaba.
    pub async fn aceitar(conexao: &quinn::Connection) -> Result<Self, ErroDeTela> {
        let fluxo = conexao
            .accept_uni()
            .await
            .map_err(|erro| ErroDeTela::Fluxo(erro.to_string()))?;
        Self::do_fluxo(fluxo).await
    }

    /// A mesma leitura, sobre um fluxo que quem chama já aceitou.
    ///
    /// Existe porque uma conexão tem um `accept_uni` só e mais de um uso para
    /// fluxo unidirecional: quem multiplexa aceita e decide, e esta metade é a
    /// que sabe ler tela.
    ///
    /// # Errors
    ///
    /// As mesmas de [`Self::aceitar`].
    pub async fn do_fluxo(mut fluxo: quinn::RecvStream) -> Result<Self, ErroDeTela> {
        let mut abertura = [0_u8; SCREEN_HEADER_LEN];
        fluxo
            .read_exact(&mut abertura)
            .await
            .map_err(|erro| ErroDeTela::Fluxo(erro.to_string()))?;
        let (cabecalho, _) = ScreenHeader::decode(&abertura)?;
        Ok(Self { fluxo, cabecalho })
    }

    /// O cabeçalho com que esta transmissão abriu.
    #[must_use]
    pub const fn cabecalho(&self) -> &ScreenHeader {
        &self.cabecalho
    }

    /// Lê o próximo quadro, ou `None` quando o outro lado encerrou.
    ///
    /// # Errors
    ///
    /// [`ErroDeTela::QuadroGrandeDemais`] ou [`ErroDeTela::QuadroVazio`] para
    /// um tamanho que este build não carrega — conferido **antes** de alocar —,
    /// e [`ErroDeTela::Fluxo`] para um fluxo cortado no meio de um quadro.
    pub async fn proximo_quadro(&mut self) -> Result<Option<QuadroRecebido>, ErroDeTela> {
        let mut cabecalho = [0_u8; CABECALHO_DE_QUADRO_LEN];
        match self.fluxo.read_exact(&mut cabecalho).await {
            Ok(()) => {}
            // O fim limpo do fluxo: quem compartilha parou. Não é erro, e
            // tratá-lo como erro faria toda transmissão terminar com uma
            // mensagem de falha na tela de quem assistia até o fim.
            Err(quinn::ReadExactError::FinishedEarly(0)) => return Ok(None),
            Err(erro) => return Err(ErroDeTela::Fluxo(erro.to_string())),
        }

        let chave = cabecalho.first().copied().unwrap_or_default() != 0;
        let tamanho = cabecalho
            .get(1..CABECALHO_DE_QUADRO_LEN)
            .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
            .map_or(0, u32::from_be_bytes) as usize;

        // Antes de alocar. `specs/08-seguranca.md`.
        if tamanho == 0 {
            return Err(ErroDeTela::QuadroVazio);
        }
        if tamanho > MAX_QUADRO_LEN {
            return Err(ErroDeTela::QuadroGrandeDemais { len: tamanho });
        }

        let mut bytes = vec![0_u8; tamanho];
        self.fluxo
            .read_exact(&mut bytes)
            .await
            .map_err(|erro| ErroDeTela::Fluxo(erro.to_string()))?;
        Ok(Some(QuadroRecebido { chave, bytes }))
    }
}

/// Quanto dura um intervalo de quadro a esta cadência.
///
/// Aqui e não no `seele-video` porque quem espalha o quadro-chave é o
/// transporte, e o intervalo é a unidade em que ele espalha.
#[must_use]
pub fn intervalo_de_quadro(quadros_por_segundo: u32) -> Duration {
    if quadros_por_segundo == 0 {
        return Duration::ZERO;
    }
    Duration::from_secs(1) / quadros_por_segundo
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::sync::Arc;

    use seele_proto::ids::ScreenId;
    use seele_proto::screen::{ScreenCodec, ScreenSource};

    use super::*;

    fn cabecalho() -> ScreenHeader {
        ScreenHeader {
            version: seele_proto::version::PROTOCOL_VERSION,
            screen: ScreenId(0x00C0_FFEE),
            source: ScreenSource::Monitor,
            codec: ScreenCodec::H264Baseline,
            width: 1280,
            height: 720,
        }
    }

    // -----------------------------------------------------------------------
    // O teto de banda
    // -----------------------------------------------------------------------

    /// **O critério de aceite do ciclo inteiro**, escrito como teste e não como
    /// comentário.
    ///
    /// §3.2, regra 3: *«a voz nunca cede à tela. Quando o sinal cai de faixa,
    /// quem baixa é o vídeo; se continuar caindo, quem para é o vídeo. Uma
    /// conversa com a tela travando é o produto funcionando; uma conversa
    /// picotando porque alguém abriu a tela é o produto quebrado.»*
    ///
    /// Três propriedades, e as três têm de valer ao mesmo tempo em todo caminho
    /// e em toda faixa:
    ///
    /// 1. o teto do vídeo **nunca** passa de [`FRACAO_DO_CAMINHO`] do caminho —
    ///    é o único ponto em que `spikes/tela-no-transporte` viu a voz voltar
    ///    aos 23,1 ms de p50 e 0% de perda, contra 225,7 ms com o vídeo solto;
    /// 2. a reserva da voz **não depende da faixa**. Quando o sinal piora, o
    ///    que encolhe é o vídeo, e o que a voz tem reservado é o mesmo número;
    /// 3. piorar a faixa nunca dá mais ao vídeo, e a faixa crítica **para** o
    ///    vídeo com motivo enumerado em vez de deixá-lo caindo para sempre.
    ///
    /// Se este teste ficar vermelho, o recurso está pior que não existir: é a
    /// tela tornando a conversa impossível, que é exatamente o que o spike
    /// mediu e o que este ciclo existe para não repetir.
    #[test]
    fn a_voz_nunca_cede_a_tela() {
        for caminho in [
            PISO_DE_BANDA_BPS * 4,
            1_000_000,
            CAMINHO_DA_PROVA_BPS,
            10_000_000,
            // Fibra doméstica, onde um `u32` estourando na multiplicação por 60
            // daria um teto minúsculo e o defeito só apareceria na casa boa.
            900_000_000,
        ] {
            let teto = TetoDeVideo::com_caminho(caminho);
            let reserva = teto.reserva_da_voz();

            // 0 — a reserva é de verdade, e vale os 40% que o spike mediu.
            //
            // O 60 vai escrito à mão, e não como [`FRACAO_DO_CAMINHO`], de
            // propósito: com a constante, este teste concordaria com qualquer
            // número que alguém pusesse nela — inclusive 100, que zera a
            // reserva e faz as três propriedades abaixo passarem enquanto a voz
            // volta aos 225,7 ms que este ciclo existe para não repetir.
            let reserva_minima = caminho - ((u64::from(caminho) * 60 / 100) as u32);
            assert!(
                reserva >= reserva_minima,
                "num caminho de {caminho} bps sobraram só {reserva} para a voz"
            );

            let mut anterior = u32::MAX;
            for faixa in [SyncBand::Nominal, SyncBand::Degraded, SyncBand::Critical] {
                let agora = teto.teto(faixa);

                // 1 — o vídeo nunca passa da fração medida, então a voz sempre
                // tem os outros 40% do caminho para chegar em 23 ms.
                assert!(
                    u64::from(agora.bps()) + u64::from(reserva) <= u64::from(caminho),
                    "em {faixa:?} sobre {caminho} bps o vídeo levou {} e a voz tinha {reserva}",
                    agora.bps()
                );

                // 2 — a reserva da voz é a mesma nas três faixas. Quem cede é o
                // vídeo, sempre, e é isto que a frase quer dizer em aritmética.
                assert_eq!(
                    teto.reserva_da_voz(),
                    reserva,
                    "a reserva da voz mudou de tamanho em {faixa:?}"
                );

                // 3 — cair de faixa nunca dá mais ao vídeo.
                assert!(
                    agora.bps() <= anterior,
                    "em {faixa:?} o vídeo ganhou banda ao piorar o sinal"
                );
                anterior = agora.bps();
            }

            // E o fim da escada é parar, com nome — não é um teto muito baixo.
            assert_eq!(
                teto.teto(SyncBand::Critical),
                Teto::Parado(MotivoDeParada::SinalCritico),
                "sinal crítico tinha de parar o vídeo em {caminho} bps"
            );
        }
    }

    #[test]
    fn o_teto_e_uma_fracao_do_caminho_e_nao_um_numero_fixo() {
        // §3.2, regra 1: «o vídeo tem teto, e o teto é uma fração do caminho
        // medido, não um valor fixo de configuração». Um caminho duas vezes
        // maior dá um teto duas vezes maior; um teto fixo daria o mesmo nos
        // dois e seria estreito num e desperdiçado no outro.
        let estreito = TetoDeVideo::com_caminho(1_000_000);
        let largo = TetoDeVideo::com_caminho(2_000_000);
        assert_eq!(estreito.teto(SyncBand::Nominal), Teto::Bps(600_000));
        assert_eq!(largo.teto(SyncBand::Nominal), Teto::Bps(1_200_000));

        // E o padrão é o cano da prova, que dá exatamente os 1200 kbps sob os
        // quais as duas provas rodaram.
        assert_eq!(
            TetoDeVideo::novo().teto(SyncBand::Nominal),
            Teto::Bps(1_200_000)
        );
    }

    #[test]
    fn a_escolha_da_pessoa_e_teto_e_nunca_piso() {
        // §5, a regra que não se negocia. Escolher mais que o caminho aguenta
        // não levanta o teto; escolher menos abaixa.
        let caminho = TetoDeVideo::com_caminho(CAMINHO_DA_PROVA_BPS);

        let pedindo_demais = caminho.com_escolha(Some(50_000_000));
        assert_eq!(
            pedindo_demais.teto(SyncBand::Nominal),
            Teto::Bps(1_200_000),
            "a escolha virou piso e levantou o teto do caminho"
        );

        let pedindo_pouco = caminho.com_escolha(Some(500_000));
        assert_eq!(pedindo_pouco.teto(SyncBand::Nominal), Teto::Bps(500_000));

        // E continua sendo teto depois de o sinal cair: a faixa degradada corta
        // o que o caminho dá, e a escolha continua por cima do resultado.
        assert_eq!(
            caminho.com_escolha(Some(400_000)).teto(SyncBand::Degraded),
            Teto::Bps(400_000)
        );
    }

    #[test]
    fn abaixo_do_piso_o_video_para_com_nome_em_vez_de_degradar_para_sempre() {
        // §2: «se o encoder não sustenta nem o piso, o compartilhamento para,
        // com motivo enumerado. Degradar para sempre é como um instrumento
        // falso: consultado justamente quando algo deu errado.»
        let apertado = TetoDeVideo::com_caminho(500_000);
        assert_eq!(apertado.teto(SyncBand::Nominal), Teto::Bps(300_000));
        // Metade de 300 kbps são 150, abaixo do piso de 200.
        assert_eq!(
            apertado.teto(SyncBand::Degraded),
            Teto::Parado(MotivoDeParada::AbaixoDoPiso)
        );

        // E pela escolha da pessoa também: pedir menos que o piso é pedir para
        // não transmitir, e o produto diz isso em vez de transmitir um borrão.
        assert_eq!(
            TetoDeVideo::novo()
                .com_escolha(Some(PISO_DE_BANDA_BPS - 1))
                .teto(SyncBand::Nominal),
            Teto::Parado(MotivoDeParada::AbaixoDoPiso)
        );
    }

    // -----------------------------------------------------------------------
    // O orçamento
    // -----------------------------------------------------------------------

    #[test]
    fn o_balde_repoe_com_o_tempo_e_nunca_passa_da_capacidade() {
        let inicio = Instant::now();
        // 800 kbps são 100 000 bytes por segundo.
        let mut balde = Balde::novo(800_000, inicio);
        assert!(balde.gastar(100_000, inicio), "o balde nasce cheio");
        assert!(!balde.gastar(1, inicio), "e vazio não empresta");

        let meio_segundo = inicio + Duration::from_millis(500);
        assert!(balde.gastar(50_000, meio_segundo));
        assert!(!balde.gastar(1, meio_segundo));

        // Uma hora parado não compra uma rajada de uma hora.
        let muito_depois = inicio + Duration::from_secs(3600);
        assert!(balde.gastar(100_000, muito_depois));
        assert!(!balde.gastar(1, muito_depois));
    }

    // -----------------------------------------------------------------------
    // O par QUIC
    // -----------------------------------------------------------------------

    /// Um par QUIC ligado, sem o handshake do produto.
    ///
    /// Mesma forma que `crate::frame::tests::par`, e pelo mesmo motivo: o que
    /// está sob teste é o transporte da tela, e um handshake no meio só
    /// acrescentaria maneiras de o teste falhar que não têm a ver com a
    /// pergunta.
    async fn par() -> (quinn::Connection, quinn::Connection) {
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
            (conexao, escuta)
        });

        let saida = cliente
            .connect(endereco, "localhost")
            .expect("conectar")
            .await
            .expect("conexão");
        let (entrada, escuta) = aceitando.await.expect("junção");

        // Vazam de propósito: derrubar os endpoints fecharia as conexões que o
        // teste ainda vai usar.
        std::mem::forget(cliente);
        std::mem::forget(escuta);
        (saida, entrada)
    }

    /// §3.1, e é a decisão mais cara da spec: **o vídeo vai num fluxo, e a voz
    /// continua nos datagramas dela.**
    ///
    /// `spikes/tela-no-transporte` mediu o que acontece quando os dois dividem
    /// a fila de datagramas do `quinn`: 16,1% da voz perdida e 2,16 s de atraso
    /// com o buffer padrão de 1 MiB, e **98,1% da voz descartada** ao encolher
    /// o buffer para 32 KiB — porque `send_datagram` põe voz e vídeo na mesma
    /// FIFO e descarta o mais velho.
    ///
    /// Este teste roda em loopback, onde não há gargalo para medir atraso.
    /// O que ele prende é o que sobrevive à ausência de gargalo: **nenhum byte
    /// de vídeo aparece como datagrama.** Troque o `open_uni` de
    /// `Transmissao::abrir` por `send_datagram` e ele fica vermelho na hora,
    /// porque o leitor de voz passa a receber quadros de tela.
    #[tokio::test(flavor = "multi_thread")]
    async fn o_video_vai_no_fluxo_e_nunca_no_datagrama() {
        const VOZ: &[u8] = b"opus-20ms";
        const QUADROS: usize = 40;

        let (saida, entrada) = par().await;
        let agora = Instant::now();

        let mut transmissao = Transmissao::abrir(&saida, cabecalho(), 8_000_000, agora)
            .await
            .expect("abrir a transmissão");

        let ouvindo = tokio::spawn({
            let entrada = entrada.clone();
            async move {
                let mut recebidos = Vec::new();
                while recebidos.len() < QUADROS {
                    match entrada.read_datagram().await {
                        Ok(bytes) => recebidos.push(bytes.to_vec()),
                        Err(_) => break,
                    }
                }
                recebidos
            }
        });

        for numero in 0..QUADROS {
            // Um quadro de tela de 6 KiB, que é a ordem de grandeza de um
            // quadro comum de 1080p no teto de 1200 kbps.
            let quadro = vec![u8::try_from(numero % 251).unwrap_or_default(); 6 * 1024];
            transmissao
                .enviar_quadro(
                    &quadro,
                    false,
                    agora + Duration::from_millis(numero as u64 * 33),
                )
                .await
                .expect("enviar");
            saida
                .send_datagram(VOZ.to_vec().into())
                .expect("a voz sai por datagrama");
        }

        let voz = tokio::time::timeout(Duration::from_secs(5), ouvindo)
            .await
            .expect("a voz não chegou a tempo")
            .expect("junção");

        assert_eq!(voz.len(), QUADROS, "faltou voz");
        for datagrama in &voz {
            assert_eq!(
                datagrama, VOZ,
                "um datagrama trouxe algo que não é voz — o vídeo entrou na fila da voz"
            );
        }

        // E a tela chegou, inteira, pelo fluxo.
        let mut recepcao = Recepcao::aceitar(&entrada).await.expect("aceitar a tela");
        assert_eq!(recepcao.cabecalho(), &cabecalho());
        for numero in 0..QUADROS {
            let quadro = recepcao
                .proximo_quadro()
                .await
                .expect("ler")
                .expect("o fluxo acabou cedo");
            assert!(!quadro.chave);
            assert_eq!(quadro.bytes.len(), 6 * 1024);
            assert_eq!(
                quadro.bytes.first().copied(),
                Some(u8::try_from(numero % 251).unwrap_or_default()),
                "os quadros chegaram fora de ordem"
            );
        }
    }

    /// §3.3: o quadro-chave sai espalhado, e sai **inteiro**.
    ///
    /// Espalhar leva o p95 da voz de 78,9 para 35,8 ms e o pior caso de 114,9
    /// para 42,7 ms, com o mesmo bitrate entregue — não se manda menos, manda-se
    /// em quatro tiques. Daí as duas metades deste teste: o remetente diz
    /// `Espalhando` e precisa de [`FATIAS_DO_QUADRO_CHAVE`] tiques para
    /// terminar, e quem recebe recebe o quadro-chave **byte por byte igual** ao
    /// que entrou, num quadro só.
    ///
    /// A segunda metade é a que não deixa espalhar virar formato: não há
    /// bandeira de continuação nem remontagem do outro lado, porque o fluxo é
    /// ordenado e espalhar é uma decisão de quando escrever.
    #[tokio::test(flavor = "multi_thread")]
    async fn o_quadro_chave_sai_espalhado_e_chega_inteiro() {
        // 65 KiB é o quadro-chave de 1080p que `spikes/tela-no-codec` mediu, e
        // os **três bytes a mais** não são enfeite: um tamanho que divide certo
        // por [`FATIAS_DO_QUADRO_CHAVE`] esconde o defeito que este teste
        // existe para pegar. Com sobra, uma última fatia que não levasse o
        // resto deixaria bytes órfãos — e quem lê espera exatamente `tamanho`
        // bytes e ficaria pendurado para sempre por causa de uma divisão.
        let chave: Vec<u8> = (0..65 * 1024 + 3)
            .map(|i| u8::try_from(i % 256).unwrap_or(0))
            .collect();

        let (saida, entrada) = par().await;
        let inicio = Instant::now();
        let mut transmissao = Transmissao::abrir(&saida, cabecalho(), 8_000_000, inicio)
            .await
            .expect("abrir");

        let tique = |n: u64| inicio + Duration::from_millis(n * 33);

        assert_eq!(
            transmissao
                .enviar_quadro(&chave, true, tique(0))
                .await
                .expect("chave"),
            Envio::Espalhando,
            "o quadro-chave saiu num tique só"
        );

        // Os tiques seguintes carregam o resto dele, e os quadros comuns
        // entregues no meio são **descartados**: escritos ali, sairiam dentro
        // do quadro-chave, porque um fluxo QUIC é uma sequência ordenada.
        let comum = vec![9_u8; 4096];
        for numero in 1..FATIAS_DO_QUADRO_CHAVE {
            assert_eq!(
                transmissao
                    .enviar_quadro(&comum, false, tique(numero as u64))
                    .await
                    .expect("comum"),
                Envio::Descartado(MotivoDeDescarte::QuadroChaveEmVoo),
                "o tique {numero} deixou um quadro comum entrar no meio da chave"
            );
        }

        // Terminada a chave, a transmissão volta ao normal.
        assert_eq!(
            transmissao
                .enviar_quadro(&comum, false, tique(FATIAS_DO_QUADRO_CHAVE as u64))
                .await
                .expect("comum"),
            Envio::Enviado
        );

        let mut recepcao = Recepcao::aceitar(&entrada).await.expect("aceitar");
        // Com prazo: uma fatia que não levasse o resto deixaria a leitura
        // pendurada em vez de errada, e um teste que trava é um teste que
        // ninguém lê o motivo.
        let recebida = tokio::time::timeout(Duration::from_secs(5), recepcao.proximo_quadro())
            .await
            .expect("o quadro-chave nunca terminou de chegar")
            .expect("ler")
            .expect("o fluxo acabou cedo");
        assert!(recebida.chave, "chegou sem a marca de quadro-chave");
        assert_eq!(
            recebida.bytes, chave,
            "o quadro-chave espalhado não chegou igual ao que saiu"
        );

        let seguinte = recepcao
            .proximo_quadro()
            .await
            .expect("ler")
            .expect("o fluxo acabou cedo");
        assert_eq!(seguinte.bytes, comum);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn um_quadro_acima_do_teto_e_descartado_e_nao_enfileirado() {
        // §1, e é a mesma decisão que `specs/03-audio.md` já tomou para o
        // áudio: um quadro velho entregue tarde é pior que um quadro perdido.
        // Enfileirar aqui seria pôr bytes na fila do gargalo, que é exatamente
        // o que o §3.2 mediu custando 200 ms de voz.
        let (saida, _entrada) = par().await;
        let agora = Instant::now();
        // 80 kbps: 10 000 bytes por segundo de orçamento.
        let mut transmissao = Transmissao::abrir(&saida, cabecalho(), 80_000, agora)
            .await
            .expect("abrir");

        assert_eq!(
            transmissao
                .enviar_quadro(&[1_u8; 9_000], false, agora)
                .await
                .expect("cabe"),
            Envio::Enviado
        );
        assert_eq!(
            transmissao
                .enviar_quadro(&[2_u8; 9_000], false, agora)
                .await
                .expect("não cabe"),
            Envio::Descartado(MotivoDeDescarte::AcimaDoTeto),
            "o segundo quadro do mesmo segundo passou do teto e saiu assim mesmo"
        );

        // Um segundo depois o orçamento voltou.
        assert_eq!(
            transmissao
                .enviar_quadro(&[3_u8; 9_000], false, agora + Duration::from_secs(1))
                .await
                .expect("cabe de novo"),
            Envio::Enviado
        );

        let (enviados, descartados, _) = transmissao.contagem();
        assert_eq!((enviados, descartados), (2, 1));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn baixar_o_teto_nao_devolve_fichas() {
        // Uma queda de faixa que liberasse rajada seria o oposto exato do que a
        // queda de faixa quer dizer: o sinal piorou, e o vídeo passaria a poder
        // mandar mais de uma vez.
        let (saida, _entrada) = par().await;
        let agora = Instant::now();
        let mut transmissao = Transmissao::abrir(&saida, cabecalho(), 800_000, agora)
            .await
            .expect("abrir");

        assert_eq!(
            transmissao
                .enviar_quadro(&[1_u8; 99_000], false, agora)
                .await
                .expect("cabe"),
            Envio::Enviado
        );
        // O sinal caiu de faixa: o teto vai à metade.
        transmissao.ajustar_teto(400_000, agora);
        assert_eq!(
            transmissao
                .enviar_quadro(&[2_u8; 1_000], false, agora)
                .await
                .expect("não cabe"),
            Envio::Descartado(MotivoDeDescarte::AcimaDoTeto)
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn um_quadro_grande_demais_ou_vazio_nao_sai() {
        let (saida, _entrada) = par().await;
        let agora = Instant::now();
        let mut transmissao = Transmissao::abrir(&saida, cabecalho(), 800_000_000, agora)
            .await
            .expect("abrir");
        assert_eq!(
            transmissao
                .enviar_quadro(&vec![0_u8; MAX_QUADRO_LEN + 1], false, agora)
                .await
                .expect("recusa não é erro"),
            Envio::Descartado(MotivoDeDescarte::GrandeDemais)
        );
        assert_eq!(
            transmissao
                .enviar_quadro(&[], false, agora)
                .await
                .expect("vazio"),
            Envio::Descartado(MotivoDeDescarte::Vazio)
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn um_tamanho_absurdo_do_par_e_recusado_antes_de_alocar() {
        // `specs/08-seguranca.md`, e conferido no receptor e não só no emissor:
        // um par escreve os cinco bytes de cabeçalho à mão e pula o emissor
        // inteiro. Ler um tamanho de 4 GiB e reservar por ele é a negação de
        // serviço mais velha que existe.
        let (saida, entrada) = par().await;
        let mut fluxo = saida.open_uni().await.expect("abrir na mão");
        let mut abertura = [0_u8; SCREEN_HEADER_LEN];
        cabecalho().encode(&mut abertura).expect("cabeçalho");
        fluxo.write_all(&abertura).await.expect("abertura");
        fluxo
            .write_all(&escrever_cabecalho_de_quadro(false, u32::MAX))
            .await
            .expect("quadro absurdo");

        let mut recepcao = Recepcao::aceitar(&entrada).await.expect("aceitar");
        assert_eq!(
            recepcao.proximo_quadro().await,
            Err(ErroDeTela::QuadroGrandeDemais {
                len: u32::MAX as usize
            }),
            "um tamanho absurdo virou uma alocação de 4 GiB"
        );

        // E o zero, pelo mesmo motivo que `ScreenHeader::check` recusa um lado
        // de zero: é muito mais vezes uma captura que falhou do que uma
        // escolha, e não há quadro atrás dele de qualquer jeito.
        let mut vazio = saida.open_uni().await.expect("abrir na mão");
        vazio.write_all(&abertura).await.expect("abertura");
        vazio
            .write_all(&escrever_cabecalho_de_quadro(false, 0))
            .await
            .expect("quadro vazio");
        let mut recepcao = Recepcao::aceitar(&entrada).await.expect("aceitar");
        assert_eq!(
            recepcao.proximo_quadro().await,
            Err(ErroDeTela::QuadroVazio)
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn o_fim_do_fluxo_e_o_fim_da_transmissao_e_nao_um_erro() {
        // Toda transmissão termina por aqui. Tratar o fim limpo como erro faria
        // quem assistiu até o fim ver uma mensagem de falha.
        let (saida, entrada) = par().await;
        let agora = Instant::now();
        let mut transmissao = Transmissao::abrir(&saida, cabecalho(), 800_000, agora)
            .await
            .expect("abrir");
        transmissao
            .enviar_quadro(&[7_u8; 128], false, agora)
            .await
            .expect("um quadro");
        transmissao.encerrar();

        let mut recepcao = Recepcao::aceitar(&entrada).await.expect("aceitar");
        assert!(recepcao.proximo_quadro().await.expect("ler").is_some());
        assert_eq!(
            recepcao.proximo_quadro().await.expect("fim limpo"),
            None,
            "o fim do fluxo virou erro"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn uma_resolucao_fora_do_teto_nao_abre_fluxo_nenhum() {
        // Conferida antes do `open_uni`: uma resolução que a prova não cobre
        // não vale um fluxo aberto que só será fechado. §6 item 10 põe tudo
        // acima de 1080p fora da v1, e o §2 mediu a CPU só até ali.
        let (saida, _entrada) = par().await;
        let grande = ScreenHeader {
            width: 1920,
            height: 1920,
            ..cabecalho()
        };
        let erro = Transmissao::abrir(&saida, grande, 800_000, Instant::now())
            .await
            .expect_err("aceitou 1920×1920");
        assert!(matches!(erro, ErroDeTela::Cabecalho(_)));
    }
}
