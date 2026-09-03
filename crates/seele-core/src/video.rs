//! A cola: captura → codificador → teto.
//!
//! `seele-video` sabe tirar pixels da tela e transformá-los em H.264, e
//! [`crate::tela`] sabe quanto disso cabe no fio. **Nenhum dos dois chamava o
//! outro**, e o relatório da onda 1 registrou a falta com todas as letras:
//! *«`seele-core` não depende de `seele-video`, então nada liga captura →
//! encoder → `ajustar_teto` ainda»*. Este módulo é essa ligação, e a aresta que
//! ele usa é a que o [ADR 0002](../../../docs/adr/0002-regra-de-dependencia.md)
//! já permitia — `core` vê `proto`, `audio` e `video`.
//!
//! # Por que a decisão mora deste lado, e não dentro do codec
//!
//! Porque o §3.2 a pôs aqui: o teto é uma fração do caminho medido, pendurada
//! no sinal que a **voz** calcula, e a voz é conta de `seele-core`. O
//! codificador só sabe obedecer a um número — se ele decidisse quanto pode
//! gastar, o produto teria dois medidores discordando no primeiro dia ruim, que
//! é exatamente o que a regra 2 do §3.2 existe para impedir. O comentário do
//! `xtask check-deps` sobre `seele-video` diz a mesma coisa do outro lado: *«se
//! um dia ele precisar de `seele-core`, quer dizer que a decisão de o que
//! transmitir migrou para dentro do codec»*.
//!
//! # O que esta cola **não** faz
//!
//! Está escrito aqui e não num relatório porque é o que alguém precisa saber
//! antes de chamá-la:
//!
//! - **não abre fluxo e não escreve byte nenhum na rede.** Ela devolve
//!   [`QuadroCodificado`], e quem o entrega a [`crate::tela::Transmissao`] é
//!   quem tem a conexão. Juntar as duas coisas poria o codificador — que o §2
//!   manda morar numa thread própria, com prioridade abaixo do normal — dentro
//!   do runtime que carrega os datagramas de voz;
//! - **não cria thread.** Quem escolhe a thread e a prioridade é quem chama, e
//!   o §2 diz qual: própria, abaixo do normal, e **nunca** perto do caminho de
//!   áudio;
//! - **não troca a resolução sozinha.** Ela diz que o degrau mudou
//!   ([`Ajuste::ResolucaoPedida`]) e para por aí. A resolução vai no cabeçalho
//!   de abertura do fluxo (§3.6), então trocá-la é reabrir o fluxo — e isso é
//!   decisão de quem é dono da transmissão, não de uma cola;
//! - **não mede o caminho.** Continua a pergunta 2 do §8, e o padrão continua
//!   sendo o cano das provas.

use seele_video::codec::{
    armar, Cadencia, CodificaVideo, ConfigDoCodificador, QuadroCodificado, QuadroI420, Resolucao,
};
use seele_video::{BibliotecaDeVideo, ErroDeVideo};
use thiserror::Error;

use crate::tela::{menor_resolucao, MotivoDeParada, Prioridade, Teto, TetoDeVideo};
use seele_proto::signal::SignalBand;

/// De onde os quadros vêm.
///
/// Existe porque as duas capturas do §1 têm a mesma forma e nomes diferentes —
/// `CapturaDaTela::tomar` no macOS, `Captura::pegar` no Windows — e porque uma
/// cola que só compilasse num dos dois sistemas seria metade de uma cola. As
/// implementações estão logo abaixo, atrás de `cfg`, e o resto deste módulo não
/// sabe em que sistema está.
///
/// **`&self` e não `&mut self`, de propósito.** Quem escreve é a thread do
/// sistema operacional e quem lê é a thread do codificador; as duas capturas já
/// resolvem isso por dentro, com a vaga de uma posição só do §1. Pedir `&mut`
/// aqui obrigaria quem chama a pôr um cadeado em cima de algo que já é seguro.
///
/// # `None` não é morte
///
/// É estado normal, e o relatório do Windows mediu por quê: **a WGC só entrega
/// quadro quando a tela muda** — 2,0 a 2,9 quadros por segundo numa área de
/// trabalho parada, sem erro nenhum. Quem transmite não pode ler «nada chegou»
/// como «a captura morreu».
pub trait FonteDeQuadros {
    /// O quadro mais novo, ou `None` se nenhum chegou desde a última chamada.
    ///
    /// **Nunca uma fila.** A regra do §1 é da captura e as duas a cumprem: um
    /// quadro que chega e encontra a vaga ocupada substitui o que estava lá. Um
    /// quadro velho entregue tarde é pior que um quadro perdido.
    fn tomar(&self) -> Option<QuadroI420>;

    /// O som que a máquina está tocando, desde a última chamada.
    ///
    /// Vazio por padrão: uma captura que não sabe capturar som devolve nada, e
    /// a transmissão sai muda em vez de não sair. É o que faz este método poder
    /// nascer sem tocar em nenhum implementador de teste.
    ///
    /// **Uma fila, ao contrário de [`Self::tomar`].** A regra do §1 vale para
    /// imagem: um quadro velho entregue tarde é pior que um quadro perdido. Som
    /// é o contrário — uma amostra pulada é um estalo, e ninguém prefere o
    /// silêncio de agora ao som de um décimo de segundo atrás.
    ///
    /// Mono, a 48 kHz, que é a taxa da casa.
    fn tomar_som(&self) -> Vec<f32> {
        Vec::new()
    }
}

#[cfg(target_os = "macos")]
impl FonteDeQuadros for seele_video::captura::macos::CapturaDaTela {
    fn tomar(&self) -> Option<QuadroI420> {
        // O instante da captura fica para trás aqui, e é uma perda de verdade:
        // é com ele que se mede a **idade** do quadro que o codificador pegou,
        // que é a grandeza com que `spikes/tela-no-codec` decidiu entre
        // enfileirar e descartar. Quem quiser medi-la chama `tomar` do tipo
        // concreto; esta ponte carrega só o que as duas capturas têm em comum,
        // e a do Windows não carimba o instante.
        Self::tomar(self).map(|da_tela| da_tela.quadro)
    }

    /// O som vem do **mesmo** `SCStream` da imagem — ver
    /// `seele-video/src/captura/macos.rs`. É por isso que o macOS não precisa
    /// do caminho de loopback que o Windows usa.
    fn tomar_som(&self) -> Vec<f32> {
        // Um quinto de segundo por vez: mais que um tique de vídeo, para uma
        // pausa de captura não virar um buraco no som, e menos que a folga da
        // fila, para nunca esvaziá-la de uma vez.
        self.som().tomar(9_600)
    }
}

/// A captura do Windows, com o som que a máquina está tocando ao lado dela.
///
/// **Duas capturas e não uma**, ao contrário do macOS, onde a imagem e o som
/// saem do mesmo `SCStream`. No Windows são coisas separadas: a imagem vem do
/// Desktop Duplication e o som vem do *loopback* do WASAPI, que é uma saída
/// aberta como entrada. Este par é o que as junta numa fonte só, para o resto
/// do produto não precisar saber de qual sistema está falando.
///
/// O som é opcional dentro do par: uma máquina que não empresta a saída — ou um
/// dispositivo que sumiu no meio — transmite **muda**, e não deixa de
/// transmitir. Mostrar a tela sem som é metade do que se queria; não mostrar
/// nada é zero.
#[cfg(target_os = "windows")]
pub struct CapturaComSom {
    imagem: seele_video::captura::windows::Captura,
    som: Option<SomDaTela>,
}

/// De onde o som da transmissão vem, no Windows.
///
/// **Duas fontes, e a diferença é o que vai no fio.** Compartilhando uma janela,
/// o som é o do programa dela e de mais nada — foi o pedido de campo: «deve
/// enviar somente o áudio da janela selecionada, não de todo o PC».
/// Compartilhando um monitor, o som é o da máquina, que é o que compartilhar um
/// monitor quer dizer.
enum SomDaTela {
    /// A árvore de processos de uma janela.
    ///
    /// Pequena: o que ela guarda é um anel e uma bandeira — o WASAPI mora na
    /// linha dela.
    DoPrograma(seele_audio::laco_por_processo::SomDoPrograma),
    /// Tudo o que a máquina toca.
    ///
    /// **No `Box` por causa do tamanho, e não por gosto.** Ela carrega o fluxo
    /// do `cpal` inteiro; ao lado da outra, a diferença fazia toda `SomDaTela`
    /// ocupar o tamanho da maior — inclusive nos casos em que a menor está
    /// dentro.
    DaMaquina(Box<seele_audio::laco::CapturaDaSaida>),
}

/// `Fonte` tem de ser `Send`: ela nasce na thread que abre a captura e vive na
/// do codificador. O `cpal::Stream` **não** é `Send` em todo backend — o WASAPI
/// o declara e é por isso que este par existe no Windows e não em toda parte.
///
/// Aqui, e não num comentário: no dia em que o cpal tirar aquele `unsafe impl`,
/// o erro aponta para esta linha e diz o nome do tipo, em vez de aparecer como
/// um limite não satisfeito no meio de `Captura for CapturaDoSistema`.
#[cfg(target_os = "windows")]
const _: fn() = || {
    fn exige_send<T: Send>() {}
    exige_send::<CapturaComSom>();
};

#[cfg(target_os = "windows")]
impl FonteDeQuadros for CapturaComSom {
    fn tomar(&self) -> Option<QuadroI420> {
        self.imagem.pegar()
    }

    fn tomar_som(&self) -> Vec<f32> {
        let Some(captura) = self.som.as_ref() else {
            return Vec::new();
        };
        // Um quinto de segundo por vez, como no macOS: mais que um tique de
        // vídeo, para uma pausa de captura não virar um buraco no som, e menos
        // que a folga do anel, para nunca esvaziá-lo de uma vez.
        const TETO: usize = 9_600;

        // **A conversão de taxa mora na captura**, e não aqui.
        //
        // Aqui havia uma conferência: fora de 48 kHz, devolvia vazio. A razão
        // escrita era «melhor som nenhum que som errado» — uma escolha entre
        // duas coisas ruins quando existe uma terceira. 44,1 kHz é a taxa de
        // metade das placas do mundo, e o efeito foi a transmissão sair muda
        // para quem tem uma delas, com um `debug!` que ninguém vê.
        //
        // `CapturaDaSaida` converte, com o mesmo `RateConverter` que a voz usa
        // desde sempre. O que sai de `tomar` está na taxa da casa, sempre.
        match captura {
            SomDaTela::DaMaquina(captura) => captura.tomar(TETO),
            // A captura por processo já nasce em 48 kHz — ela **declara** o
            // formato em vez de aceitar o do dispositivo, porque nesta ativação
            // não há dispositivo a quem perguntar. Não há taxa a converter.
            //
            // E ela não bloqueia: quem espera o evento do WASAPI é a linha dela,
            // e aqui só se tira do anel. Esperar neste ponto seguraria o quadro
            // seguinte do codificador.
            SomDaTela::DoPrograma(captura) => captura.tomar(TETO),
        }
    }
}

/// A captura não recomeçou no degrau pedido.
///
/// Texto e não o erro da plataforma de propósito: `seele-video` tem **dois**
/// `ErroDeCaptura`, um por sistema, e nenhum dos dois existe no build do outro.
/// Um tipo que este crate pudesse nomear teria de ser inventado aqui e traduzido
/// duas vezes lá; é a mesma razão pela qual [`crate::tela::ErroDeTela::Fluxo`]
/// carrega o erro do `quinn` como texto.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("the capture would not start at {largura}×{altura}: {detalhe}")]
pub struct CapturaRecusou {
    /// A largura pedida, em pixels.
    pub largura: usize,
    /// A altura pedida, em pixels.
    pub altura: usize,
    /// O que o sistema disse, cru. Serve a quem depura e não a quem usa.
    pub detalhe: String,
}

impl CapturaRecusou {
    /// O erro de uma captura que recusou este degrau.
    #[must_use]
    pub fn nova(resolucao: Resolucao, detalhe: impl Into<String>) -> Self {
        Self {
            largura: resolucao.largura(),
            altura: resolucao.altura(),
            detalhe: detalhe.into(),
        }
    }
}

/// Quem sabe **começar** uma captura, e recomeçá-la noutro degrau.
///
/// # Por que uma fábrica e não uma captura pronta
///
/// Porque trocar de degrau não é ajustar nada: é recomeçar. A ScreenCaptureKit
/// é armada com largura e altura fixas — `CapturaDaTela::iniciar` põe
/// `with_width`/`with_height` na `SCStreamConfiguration` e o comentário dela diz
/// por quê: *«[`seele_video::codec::Codificador`] é armado com uma [`Resolucao`] e recusa qualquer
/// quadro de outro tamanho»*. Então um degrau novo tem três metades que andam
/// juntas — captura nova, codificador novo, fluxo novo (§3.6, a resolução mora
/// no cabeçalho de abertura) —, e uma captura entregue pronta deixaria a
/// primeira delas sem dono.
///
/// A [`crate::bomba::Bomba`] é quem as junta, e é para isso que ela pede isto em
/// vez de pedir uma [`FonteDeQuadros`].
pub trait Captura: Send + 'static {
    /// De onde os quadros saem depois de começar.
    type Fonte: FonteDeQuadros + Send;

    /// Começa a capturar neste degrau, largando o que estava capturando antes.
    ///
    /// # Errors
    ///
    /// [`CapturaRecusou`] quando o sistema não começa — sem permissão, alvo que
    /// sumiu, degrau que a fonte não aceita.
    fn iniciar(
        &mut self,
        resolucao: Resolucao,
        cadencia: Cadencia,
    ) -> Result<Self::Fonte, CapturaRecusou>;
}

// ---------------------------------------------------------------------------
// A captura sem o tipo dela
// ---------------------------------------------------------------------------

/// Uma [`Captura`] com o tipo apagado, para caber num comando.
///
/// [`Captura`] tem tipo associado — a fonte que ela abre —, então não é objeto
/// seguro, e o `Comando` que leva a escolha da pessoa até o motor do
/// [`crate::enlace::Enlace`] é um enum de tipos concretos. Esta caixa é o que
/// reconcilia os dois.
///
/// **E é o que deixa a máquina de estados ser provada sem uma tela na frente.**
/// Se o comando carregasse [`CapturaDoSistema`], o único jeito de exercer
/// «pedido guardado → `ScreenShareStarted` → bomba nascendo» seria numa máquina
/// com monitor e com o TCC concedido, que é o mesmo que dizer que ele nunca
/// seria exercido.
pub struct CapturaEmCaixa(Box<dyn AbreCaptura>);

impl CapturaEmCaixa {
    /// Guarda uma captura qualquer na caixa.
    pub fn nova<C>(captura: C) -> Self
    where
        C: Captura,
        C::Fonte: 'static,
    {
        Self(Box::new(captura))
    }
}

impl std::fmt::Debug for CapturaEmCaixa {
    /// Só o nome: o que está dentro é um `SCStream` ou uma sessão da WGC, e
    /// nenhum dos dois tem o que dizer num log.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CapturaEmCaixa")
    }
}

/// A metade objeto-segura de [`Captura`]: a fonte também sai numa caixa.
trait AbreCaptura: Send {
    fn iniciar(
        &mut self,
        resolucao: Resolucao,
        cadencia: Cadencia,
    ) -> Result<Box<dyn FonteDeQuadros + Send>, CapturaRecusou>;
}

impl<C> AbreCaptura for C
where
    C: Captura,
    C::Fonte: 'static,
{
    fn iniciar(
        &mut self,
        resolucao: Resolucao,
        cadencia: Cadencia,
    ) -> Result<Box<dyn FonteDeQuadros + Send>, CapturaRecusou> {
        Captura::iniciar(self, resolucao, cadencia)
            .map(|fonte| Box::new(fonte) as Box<dyn FonteDeQuadros + Send>)
    }
}

impl FonteDeQuadros for Box<dyn FonteDeQuadros + Send> {
    fn tomar(&self) -> Option<QuadroI420> {
        (**self).tomar()
    }

    fn tomar_som(&self) -> Vec<f32> {
        (**self).tomar_som()
    }
}

impl Captura for CapturaEmCaixa {
    type Fonte = Box<dyn FonteDeQuadros + Send>;

    fn iniciar(
        &mut self,
        resolucao: Resolucao,
        cadencia: Cadencia,
    ) -> Result<Self::Fonte, CapturaRecusou> {
        self.0.iniciar(resolucao, cadencia)
    }
}

// ---------------------------------------------------------------------------
// A captura desta máquina
// ---------------------------------------------------------------------------

/// O alvo nativo que [`FonteDeTela`] guarda, um por sistema.
///
/// Fora do macOS e do Windows é [`Infallible`](std::convert::Infallible), e
/// isso é o desenho e não um recheio: sem captura não há como construir uma
/// [`FonteDeTela`], então o tipo diz por construção o que
/// [`fontes_de_tela`] diria por erro.
#[cfg(target_os = "macos")]
type AlvoDoSistema = seele_video::captura::macos::Fonte;
#[cfg(target_os = "windows")]
type AlvoDoSistema = seele_video::captura::windows::Alvo;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
type AlvoDoSistema = std::convert::Infallible;

/// De onde os quadros saem depois que [`CapturaDoSistema`] começa.
#[cfg(target_os = "macos")]
type FonteDoSistema = seele_video::captura::macos::CapturaDaTela;
#[cfg(target_os = "windows")]
type FonteDoSistema = CapturaComSom;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
type FonteDoSistema = std::convert::Infallible;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl FonteDeQuadros for std::convert::Infallible {
    fn tomar(&self) -> Option<QuadroI420> {
        match *self {}
    }
}

/// O que o sistema operacional respondeu sobre gravar a tela.
///
/// Quatro respostas e não três: a última não é sobre a pessoa nem sobre o
/// sistema, é sobre **esta compilação**, e confundi-la com «negada» mandaria
/// quem usa procurar um ajuste que não existe. É a mesma razão pela qual o §4
/// separa consultar de pedir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissaoDeTela {
    /// Dá para capturar agora.
    Concedida,
    /// O sistema disse não, e o caminho de volta são os Ajustes — no macOS o
    /// alerta do TCC não aparece duas vezes.
    Negada,
    /// Ninguém perguntou ainda, e perguntar pode dar `Concedida`.
    NaoPerguntada,
    /// Este build não tem captura de tela, então não há a quem perguntar.
    ///
    /// É o Linux da v1: o portal XDG exige `ashpd` mais `pipewire`, e com eles
    /// o binário deixa de ser autocontido — decisão de 22/08/2026, §7 item 5.
    SemCaptura,
}

/// Olha o que o sistema já concedeu, **sem** perguntar nada.
///
/// Barato e sem efeito colateral: dá para chamar toda vez que a tela de
/// escolher abre, e é o que se deve fazer — num app não assinado a concessão
/// morre a cada build.
#[must_use]
pub fn permissao_de_tela() -> PermissaoDeTela {
    #[cfg(target_os = "macos")]
    {
        // `Ausente` vira `NaoPerguntada` e não `Negada` **aqui**, e o
        // contrário em `pedir_permissao_de_tela`. A ScreenCaptureKit tem uma
        // resposta só para os dois estados, e quem sabe distingui-los é quem
        // chamou: uma consulta que nunca perguntou não pode afirmar que foi
        // negada, e um pedido que voltou vazio não pode dizer que ninguém
        // perguntou.
        if seele_video::captura::macos::permissao().concedida() {
            PermissaoDeTela::Concedida
        } else {
            PermissaoDeTela::NaoPerguntada
        }
    }
    // No Windows não há permissão de sistema para capturar a tela: a WGC
    // captura, e o único consentimento é o da nossa interface (§4).
    #[cfg(target_os = "windows")]
    {
        PermissaoDeTela::Concedida
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        PermissaoDeTela::SemCaptura
    }
}

/// Pede ao sistema, abrindo o alerta dele se ainda houver um a abrir.
///
/// **Não é a mesma coisa que [`permissao_de_tela`]**, e o §4 é quem separa: no
/// macOS o alerta do TCC aparece **uma vez só** por identidade assinada, então
/// uma consulta que perguntasse gastaria a única chance que a pessoa tem.
#[must_use]
pub fn pedir_permissao_de_tela() -> PermissaoDeTela {
    #[cfg(target_os = "macos")]
    {
        if seele_video::captura::macos::pedir_permissao().concedida() {
            PermissaoDeTela::Concedida
        } else {
            PermissaoDeTela::Negada
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        permissao_de_tela()
    }
}

/// Por que a lista de fontes não veio.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ErroDeFontes {
    /// Este build não tem captura de tela. Ver [`PermissaoDeTela::SemCaptura`].
    #[error("this build has no screen capture")]
    SemCaptura,
    /// O sistema não concedeu gravação de tela.
    ///
    /// Separado do resto porque é o único com conserto do lado de quem usa, e
    /// porque uma lista vazia diria a mesma coisa sem dizer o motivo — que é o
    /// beco que o §4 manda evitar.
    #[error("screen recording was not allowed")]
    SemPermissao,
    /// O sistema recusou, e o que ele disse vai junto para quem depura.
    #[error("the system would not list what can be shared: {0}")]
    SistemaRecusou(String),
}

/// Uma coisa que esta máquina pode transmitir: um monitor ou uma janela.
///
/// Carrega o alvo nativo por dentro, e por isso não é `Clone` nem atravessa a
/// fronteira do [ADR 0002](../../../docs/adr/0002-regra-de-dependencia.md)
/// inteira — a casca lê os campos e guarda a lista.
#[derive(Debug)]
pub struct FonteDeTela {
    id: u64,
    rotulo: String,
    monitor: bool,
    largura: u32,
    altura: u32,
    alvo: AlvoDoSistema,
}

impl FonteDeTela {
    /// Como esta fonte é chamada nesta listagem, e **só nesta**.
    ///
    /// É o índice, não um identificador do sistema, e a diferença é do
    /// Windows: um `Alvo` da WGC não
    /// publica `HWND` nenhum, então um número estável entre duas listagens não
    /// existe nos dois sistemas. Quem escolhe por este número tem de estar
    /// segurando a lista de onde ele saiu — que é o que a casca faz, porque a
    /// lista é o que ela acabou de desenhar.
    #[must_use]
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// O rótulo que a interface mostra, montado pelo sistema que o conhece.
    #[must_use]
    pub fn rotulo(&self) -> &str {
        &self.rotulo
    }

    /// `true` para um monitor inteiro, `false` para uma janela.
    #[must_use]
    pub const fn monitor(&self) -> bool {
        self.monitor
    }

    /// Largura de agora, em pixels. **Não é promessa**: uma janela muda de
    /// tamanho enquanto é transmitida.
    #[must_use]
    pub const fn largura(&self) -> u32 {
        self.largura
    }

    /// Altura de agora, em pixels.
    #[must_use]
    pub const fn altura(&self) -> u32 {
        self.altura
    }

    /// O que o cabeçalho de abertura do fluxo declara (§3.6).
    #[must_use]
    pub const fn origem(&self) -> seele_proto::screen::ScreenSource {
        if self.monitor {
            seele_proto::screen::ScreenSource::Monitor
        } else {
            seele_proto::screen::ScreenSource::Window
        }
    }

    /// Quem sabe começar — e recomeçar — a captura desta fonte.
    #[must_use]
    pub fn captura(self) -> CapturaDoSistema {
        CapturaDoSistema { alvo: self.alvo }
    }
}

/// O que esta máquina pode transmitir, na ordem em que o sistema enumera:
/// monitores primeiro, janelas depois.
///
/// # Errors
///
/// [`ErroDeFontes`], e o caso que **não** é erro está dito ali: uma sessão sem
/// monitor e sem janela devolve lista vazia, porque não ter o que compartilhar
/// é estado e não falha.
pub fn fontes_de_tela() -> Result<Vec<FonteDeTela>, ErroDeFontes> {
    #[cfg(target_os = "macos")]
    {
        use seele_video::captura::macos::{fontes, ErroDeCaptura, Fonte};

        let lista = match fontes() {
            Ok(lista) => lista,
            Err(ErroDeCaptura::SemPermissaoDeTela) => return Err(ErroDeFontes::SemPermissao),
            // Uma sessão sem tela — SSH, um agente de integração contínua.
            // Estado, e não falha: a lista é vazia e quem desenha o botão
            // decide não desenhá-lo.
            Err(ErroDeCaptura::NadaParaCapturar) => Vec::new(),
            Err(outro) => return Err(ErroDeFontes::SistemaRecusou(outro.to_string())),
        };

        Ok(lista
            .into_iter()
            .enumerate()
            .map(|(indice, fonte)| {
                let rotulo = fonte.rotulo();
                let (monitor, largura, altura) = match &fonte {
                    Fonte::Monitor {
                        largura, altura, ..
                    } => (true, *largura, *altura),
                    // Uma janela não publica tamanho pela `SCWindow` que esta
                    // lista carrega, e zero aqui quer dizer «não sei» — o mesmo
                    // contrato do `HostUplink` do protocolo. A casca escreve
                    // travessão, e nunca `0×0`.
                    Fonte::Janela { .. } => (false, 0, 0),
                };
                FonteDeTela {
                    id: indice as u64,
                    rotulo,
                    monitor,
                    largura,
                    altura,
                    alvo: fonte,
                }
            })
            .collect())
    }
    #[cfg(target_os = "windows")]
    {
        use seele_video::captura::windows::{listar_janelas, listar_monitores, ErroDeCaptura};

        let monitores = match listar_monitores() {
            Ok(lista) => lista,
            // A mesma decisão do macOS: sem área de trabalho não é falha.
            Err(ErroDeCaptura::SemMonitor) => Vec::new(),
            Err(outro) => return Err(ErroDeFontes::SistemaRecusou(outro.to_string())),
        };
        let janelas =
            listar_janelas().map_err(|erro| ErroDeFontes::SistemaRecusou(erro.to_string()))?;

        Ok(monitores
            .into_iter()
            .chain(janelas)
            .enumerate()
            .map(|(indice, alvo)| {
                let (largura, altura) = alvo.tamanho();
                FonteDeTela {
                    id: indice as u64,
                    rotulo: alvo.nome().to_owned(),
                    monitor: alvo.e_monitor(),
                    largura,
                    altura,
                    alvo,
                }
            })
            .collect())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err(ErroDeFontes::SemCaptura)
    }
}

/// A captura desta máquina, presa a uma fonte escolhida.
///
/// É o implementador de [`Captura`] que faltava: até aqui o trait era exportado
/// sem ninguém que o cumprisse fora dos testes, e a bomba não tinha o que ligar.
#[derive(Debug)]
pub struct CapturaDoSistema {
    alvo: AlvoDoSistema,
}

impl Captura for CapturaDoSistema {
    type Fonte = FonteDoSistema;

    fn iniciar(
        &mut self,
        resolucao: Resolucao,
        cadencia: Cadencia,
    ) -> Result<Self::Fonte, CapturaRecusou> {
        #[cfg(target_os = "macos")]
        {
            seele_video::captura::macos::CapturaDaTela::iniciar(&self.alvo, resolucao, cadencia)
                .map_err(|erro| CapturaRecusou::nova(resolucao, erro.to_string()))
        }
        #[cfg(target_os = "windows")]
        {
            let imagem =
                seele_video::captura::windows::Captura::iniciar(&self.alvo, resolucao, cadencia)
                    .map_err(|erro| CapturaRecusou::nova(resolucao, erro.to_string()))?;
            // O som ao lado, e **sem derrubar a transmissão quando não abre**:
            // mostrar a tela sem som é metade do que se queria; não mostrar
            // nada é zero.
            // **`info!` e não `debug!` no caminho bom.** A pergunta que este log
            // responde é «a transmissão saiu muda por quê», e ela é feita depois
            // do fato, por alguém lendo o arquivo: um caminho bom silencioso não
            // distingue «abriu e não veio som» de «nem abriu».
            // **Uma janela leva o som dela; um monitor leva o da máquina.**
            //
            // E quando a captura por processo não abre — Windows anterior à
            // build 20348, ou a janela fechou entre a lista e agora —, o som da
            // máquina é a queda: mandar o som do computador inteiro é pior que
            // mandar só o da janela, e melhor que mandar silêncio.
            let som = match self.alvo.processo() {
                Some(processo) => match seele_audio::laco_por_processo::SomDoPrograma::abrir(
                    processo,
                ) {
                    Ok(captura) => {
                        tracing::info!(processo, "o som deste programa abriu para a transmissão");
                        Some(SomDaTela::DoPrograma(captura))
                    }
                    Err(erro) => {
                        tracing::warn!(
                            %erro,
                            processo,
                            "não abri o som só deste programa; caio para o som da máquina"
                        );
                        som_da_maquina()
                    }
                },
                None => som_da_maquina(),
            };
            Ok(CapturaComSom { imagem, som })
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let _ = (resolucao, cadencia);
            match self.alvo {}
        }
    }
}

/// O que a pessoa escolheu como teto, e **todos são teto** (§5).
///
/// Teto e nunca piso: o sistema continua livre para ficar abaixo de cada um
/// destes números, e a regra de aceite do §3.2 depende disso — *a voz nunca
/// cede à tela*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LimitesDeTela {
    /// Teto de banda em bits por segundo, ou `None` para «só o que o caminho
    /// permitir».
    pub banda_bps: Option<u32>,
    /// O degrau escolhido, que é o **maior** que pode sair.
    pub resolucao: Resolucao,
    /// A cadência escolhida, também máximo.
    pub cadencia: Cadencia,
    /// O que cede primeiro quando o orçamento aperta.
    ///
    /// Não é teto como os três acima: é **direção**. Os outros dizem «no
    /// máximo isto»; este diz o que sacrificar quando o máximo não couber. Ver
    /// [`crate::tela::Prioridade`].
    pub prioridade: Prioridade,
}

impl Default for LimitesDeTela {
    /// O que a lista fechada do §5 tem como padrão: 720p a 30, sem teto de
    /// banda próprio.
    fn default() -> Self {
        Self {
            banda_bps: None,
            resolucao: Resolucao::P720,
            cadencia: Cadencia::Q30,
            // O padrão é o do §2: texto. Compartilhar tela ainda é, na
            // maioria das vezes, mostrar uma tela.
            prioridade: Prioridade::Nitidez,
        }
    }
}

/// Tudo o que é preciso para começar a transmitir, menos o nome da transmissão.
///
/// O nome falta porque ele não existe ainda: `StartScreenShare` é um pedido, e
/// o [`ScreenId`](seele_proto::ids::ScreenId) volta depois, num
/// `ScreenShareStarted`. Esta estrutura é o que espera nesse intervalo.
#[derive(Debug)]
pub struct PedidoDeTela {
    /// O módulo do Cisco, já carregado.
    ///
    /// Vem pronto de quem chamou pelo mesmo argumento de
    /// `seele_video::modulo::procurar_em`: onde os arquivos do produto moram é
    /// decisão da casca, e uma biblioteca que adivinha `~/Library` passa a ter
    /// opinião sobre uma coisa que não é dela.
    pub biblioteca: BibliotecaDeVideo,
    /// Quem sabe começar a captura escolhida.
    pub captura: CapturaEmCaixa,
    /// Monitor ou janela, para o cabeçalho de abertura (§3.6).
    pub origem: seele_proto::screen::ScreenSource,
}

/// Por que o compartilhamento não anda.
#[derive(Debug, Error)]
pub enum ErroDeCompartilhamento {
    /// O teto disse para não transmitir. **Não é falha**: é o §3.2 respondendo
    /// que agora não dá, com o motivo que a interface escreve na língua da
    /// pessoa.
    #[error("the screen share cannot run: {0}")]
    Parado(#[source] MotivoDeParada),
    /// O codec ou o módulo do Cisco recusaram.
    #[error(transparent)]
    Video(#[from] ErroDeVideo),
}

/// O que saiu de um tique.
///
/// Três respostas e não um `Option`, porque «não veio quadro da tela» e «o
/// controle de taxa pulou este quadro» são coisas diferentes que a interface
/// mostra diferente — e um `None` para as duas ensinaria quem chama a tratá-las
/// igual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Passo {
    /// A captura não tinha quadro novo. Estado normal — ver [`FonteDeQuadros`].
    SemQuadro,
    /// O controle de taxa do OpenH264 pulou este quadro para não estourar o
    /// teto.
    ///
    /// **Não é perda e não é erro.** No teto de 1200 kbps que a voz permite são
    /// 16,2% dos quadros em 1080p e 11,1% em 720p, medidos em duas máquinas. É
    /// exatamente o caso para o qual o §5 obriga a tela a mostrar o que está
    /// saindo ao lado do que foi pedido.
    PuladoPeloTeto,
    /// Saiu um quadro, pronto para [`crate::tela::Transmissao::enviar_quadro`].
    Quadro(QuadroCodificado),
}

/// O que mudou quando o teto andou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ajuste {
    /// Nada mudou.
    Igual,
    /// Só o teto de banda mudou, e o codificador continua o mesmo.
    ///
    /// É um `SetOption` e não uma reconstrução, ao contrário do que a voz faz
    /// em `seele_audio::codec::VoiceEncoder::set_bitrate`: aqui refazer o
    /// encoder custaria um quadro-chave inteiro, que são 65 KiB e 446 ms do
    /// orçamento de 1200 kbps.
    TetoNovo {
        /// O teto que passou a valer.
        teto_bps: u32,
    },
    /// O degrau de resolução mudou, e o codificador **continua no antigo**.
    ///
    /// A resolução vai no cabeçalho de abertura do fluxo (§3.6), então trocá-la
    /// no meio é reabrir o fluxo com outro cabeçalho — e quem faz isso é o dono
    /// da transmissão. Esta cola avisa e obedece ao teto de banda mesmo assim,
    /// que é a parte que não pode esperar.
    ResolucaoPedida {
        /// O degrau em uso.
        de: Resolucao,
        /// O degrau que o teto de agora compraria.
        para: Resolucao,
        /// O teto que passou a valer.
        teto_bps: u32,
    },
    /// O vídeo parou, com motivo (§3.2).
    Parou(MotivoDeParada),
}

/// O que o teto manda o codificador fazer, sem tocar em codificador nenhum.
///
/// Separado do resto para poder ser conferido sem o módulo do Cisco na
/// máquina — é a decisão inteira, e ela é aritmética.
///
/// `escolha` é a resolução que a pessoa pediu, e ela é **teto** (§5): o degrau
/// que sai daqui é o menor entre o que ela pediu e o que o orçamento compra.
/// `None` quando o teto mandou parar: não há configuração para uma transmissão
/// que não acontece.
#[must_use]
pub fn config_para(
    teto: Teto,
    escolha: Resolucao,
    cadencia: Cadencia,
    prioridade: Prioridade,
) -> Option<ConfigDoCodificador> {
    let Teto::Bps(bps) = teto else { return None };
    let degrau = crate::tela::resolucao_para(bps, prioridade);
    Some(ConfigDoCodificador {
        resolucao: menor_resolucao(degrau, escolha),
        cadencia,
        teto_bps: teto.bps(),
    })
}

/// Uma transmissão de tela viva, do lado de quem compartilha.
///
/// Guarda o codificador, o teto que está valendo e a escolha da pessoa, e é a
/// única coisa deste crate que conhece os dois lados.
///
/// É `Send` porque o §2 manda o codificador morar numa thread própria, com
/// prioridade abaixo do normal, e **nunca** no runtime que carrega os
/// datagramas de voz. Não é `Sync`, e isso é certo: dois lados codificando no
/// mesmo encoder embaralhariam a predição.
#[derive(Debug)]
pub struct Compartilhamento {
    biblioteca: BibliotecaDeVideo,
    /// Atrás da costura de propósito, e não um [`seele_video::codec::Codificador`] concreto.
    ///
    /// Ver [`CodificaVideo`]: é por aqui que o codificador por hardware entra,
    /// sem que esta cola precise saber qual está armado. Quem escolhe é
    /// [`armar`], num lugar só.
    codificador: Box<dyn CodificaVideo>,
    escolha_de_resolucao: Resolucao,
    prioridade: Prioridade,
    teto: Teto,
}

impl Compartilhamento {
    /// Arma o codificador para o teto de agora.
    ///
    /// # Errors
    ///
    /// [`ErroDeCompartilhamento::Parado`] quando o teto disse para não
    /// transmitir — o sinal está crítico ou o que sobrou não sustenta nem o
    /// piso. [`ErroDeCompartilhamento::Video`] quando o OpenH264 recusa a
    /// configuração.
    pub fn abrir(
        biblioteca: BibliotecaDeVideo,
        teto_de_video: &TetoDeVideo,
        faixa: SignalBand,
        escolha_de_resolucao: Resolucao,
        cadencia: Cadencia,
        prioridade: Prioridade,
    ) -> Result<Self, ErroDeCompartilhamento> {
        let teto = teto_de_video.teto(faixa);
        let config = match (
            teto,
            config_para(teto, escolha_de_resolucao, cadencia, prioridade),
        ) {
            (_, Some(config)) => config,
            (Teto::Parado(motivo), None) => return Err(ErroDeCompartilhamento::Parado(motivo)),
            // `config_para` só devolve `None` para um teto parado, e um teto
            // parado só tem essa forma. Este braço existe porque o compilador
            // não sabe disso e porque um `unwrap` aqui seria proibido pelo
            // `forbid` do workspace — e com razão.
            (Teto::Bps(_), None) => {
                return Err(ErroDeCompartilhamento::Parado(MotivoDeParada::AbaixoDoPiso))
            }
        };
        let codificador = armar(&biblioteca, config)?;
        Ok(Self {
            biblioteca,
            codificador,
            escolha_de_resolucao,
            prioridade,
            teto,
        })
    }

    /// O teto que está valendo.
    #[must_use]
    pub const fn teto(&self) -> Teto {
        self.teto
    }

    /// A resolução com que o codificador está armado — a que **está saindo**, e
    /// não a que foi pedida.
    ///
    /// A diferença é a regra do §5: *a tela não promete a escolha*. Quem mostra
    /// «o que está saindo agora ao lado do que foi pedido» lê este número de um
    /// lado e [`Self::escolha_de_resolucao`] do outro.
    ///
    /// **Deixou de ser `const`** quando o codificador foi para trás da costura,
    /// e o mesmo vale para [`Self::cadencia`] e [`Self::quadros_por_segundo`]:
    /// método de trait não é `const`, e não há como ser enquanto a
    /// implementação for escolhida em tempo de execução — que é o ponto inteiro
    /// de haver costura. Ninguém as chamava em contexto `const`; se chamasse,
    /// isto não compilaria e a troca teria aparecido aqui em vez de em campo.
    #[must_use]
    pub fn resolucao(&self) -> Resolucao {
        self.codificador.resolucao()
    }

    /// A resolução que a pessoa pediu, que é teto e nunca piso (§5).
    #[must_use]
    pub const fn escolha_de_resolucao(&self) -> Resolucao {
        self.escolha_de_resolucao
    }

    /// A cadência com que o codificador está armado.
    ///
    /// Existe porque quem gira o laço precisa dela para saber quanto dormir
    /// entre dois tiques — [`crate::tela::intervalo_de_quadro`] —, e ler isso do
    /// `Arranjo` que abriu a transmissão seria ler a intenção em vez do que o
    /// codificador de fato aceitou.
    #[must_use]
    pub fn cadencia(&self) -> Cadencia {
        self.codificador.cadencia()
    }

    /// Quantos quadros por segundo o codificador está armado para aceitar.
    #[must_use]
    pub fn quadros_por_segundo(&self) -> u32 {
        self.codificador.quadros_por_segundo()
    }

    /// Reage a um teto novo: aplica a banda e diz se o degrau mudou.
    ///
    /// **Isto é o §3.2 e o §5.1 virando código.** O teto muda quando o sinal da
    /// voz cai de faixa e também quando alguém entra ou sai da sala — a perna
    /// de quem hospeda é dividida pelo número de espectadores —, e as duas
    /// coisas chegam aqui pelo mesmo caminho, porque N já está dentro do teto.
    ///
    /// # Errors
    ///
    /// [`ErroDeCompartilhamento::Video`] se o OpenH264 recusar a banda nova.
    /// Um teto que mandou parar **não** é erro: volta como [`Ajuste::Parou`],
    /// porque parar com motivo é uma resposta do produto e não uma falha.
    pub fn ajustar(
        &mut self,
        teto_de_video: &TetoDeVideo,
        faixa: SignalBand,
    ) -> Result<Ajuste, ErroDeCompartilhamento> {
        let novo = teto_de_video.teto(faixa);
        self.teto = novo;
        let bps = match novo {
            Teto::Parado(motivo) => return Ok(Ajuste::Parou(motivo)),
            Teto::Bps(bps) => bps,
        };

        let mudou_a_banda = bps != self.codificador.teto_bps();
        if mudou_a_banda {
            // **O teto passa a aparecer no log**, e é a quarta vez nesta casa que
            // um mecanismo decide e não conta.
            //
            // Ele é quem escolhe entre 540p e 1080p, e nada dele chegava ao
            // arquivo: um relato de «muito pixelada» só podia ser respondido com
            // a resolução armada, que diz o **efeito** e não a causa. Com o
            // número aqui, «ficou em 540p» deixa de ser mistério e vira «a banda
            // medida era esta».
            //
            // Só na mudança, e não a cada tique: a sonda mexe no número o tempo
            // todo, e uma linha por medição afogaria o arquivo. Uma mudança de
            // teto é decisão, não medição.
            tracing::info!(
                de = self.codificador.teto_bps(),
                para = bps,
                "o teto de banda da tela mudou"
            );
            self.codificador.ajustar_teto(bps)?;
        }

        // O eixo entra aqui, e num lugar só: `resolucao_para` é
        // `resolucao_estimada_para` mais um degrau abaixo quando quem
        // compartilha escolheu movimento. A escolha explícita da pessoa continua
        // sendo teto por cima disso — os dois são teto, e manda o menor (§5).
        let do_teto = match novo {
            Teto::Bps(bps) => crate::tela::resolucao_para(bps, self.prioridade),
            Teto::Parado(_) => self.escolha_de_resolucao,
        };
        let degrau = menor_resolucao(do_teto, self.escolha_de_resolucao);
        if degrau != self.codificador.resolucao() {
            return Ok(Ajuste::ResolucaoPedida {
                de: self.codificador.resolucao(),
                para: degrau,
                teto_bps: bps,
            });
        }
        if mudou_a_banda {
            return Ok(Ajuste::TetoNovo { teto_bps: bps });
        }
        Ok(Ajuste::Igual)
    }

    /// Refaz o codificador num degrau novo, depois de o fluxo ter sido reaberto.
    ///
    /// Custa um quadro-chave inteiro — 65 KiB, 446 ms do orçamento de 1200 kbps
    /// —, e é por isso que [`Self::ajustar`] não faz isto sozinho: um degrau que
    /// oscila entre dois valores queimaria o orçamento em quadros-chave e não
    /// mostraria tela nenhuma.
    ///
    /// # Errors
    ///
    /// [`ErroDeCompartilhamento::Video`] se o OpenH264 recusar a configuração.
    pub fn refazer_com(&mut self, resolucao: Resolucao) -> Result<(), ErroDeCompartilhamento> {
        let config = ConfigDoCodificador {
            resolucao,
            cadencia: self.codificador.cadencia(),
            teto_bps: self.teto.bps(),
        };
        self.codificador = armar(&self.biblioteca, config)?;
        Ok(())
    }

    /// Um tique: pega o quadro mais novo da captura e codifica.
    ///
    /// `pedido_de_chave` é o §3.3 — quadro-chave **quando quem recebe pede**, e
    /// não de tempos em tempos.
    ///
    /// # Errors
    ///
    /// [`ErroDeCompartilhamento::Video`] com
    /// `ErroDeVideo::QuadroDeTamanhoErrado` quando a captura entrega um quadro
    /// de outro tamanho — que é o que acontece se alguém trocar o degrau sem
    /// reconfigurar a captura, e é um erro nomeado justamente para não virar um
    /// borrão sem explicação.
    pub fn passo(
        &mut self,
        fonte: &impl FonteDeQuadros,
        pedido_de_chave: bool,
    ) -> Result<Passo, ErroDeCompartilhamento> {
        let Some(quadro) = fonte.tomar() else {
            return Ok(Passo::SemQuadro);
        };
        match self.codificador.codificar(&quadro, pedido_de_chave)? {
            Some(codificado) => Ok(Passo::Quadro(codificado)),
            None => Ok(Passo::PuladoPeloTeto),
        }
    }
}

/// O som da máquina inteira, que é o que um monitor compartilhado leva.
///
/// **`info!` e não `debug!` no caminho bom.** A pergunta que este log responde é
/// «a transmissão saiu muda por quê», e ela é feita depois do fato, por alguém
/// lendo o arquivo: um caminho bom silencioso não distingue «abriu e não veio
/// som» de «nem abriu».
#[cfg(target_os = "windows")]
fn som_da_maquina() -> Option<SomDaTela> {
    match seele_audio::laco::CapturaDaSaida::abrir(None) {
        Ok(captura) => {
            tracing::info!(
                taxa = captura.taxa(),
                "o som desta máquina abriu para a transmissão"
            );
            Some(SomDaTela::DaMaquina(Box::new(captura)))
        }
        Err(erro) => {
            tracing::warn!(%erro, "não abri o som desta máquina; a transmissão sai muda");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Mutex;

    use seele_video::modulo;

    use super::*;
    use crate::tela::{CAMINHO_DA_PROVA_BPS, PISO_DE_BANDA_BPS};

    /// A cola arma codificador **só** pela costura.
    ///
    /// Uma costura que alguém pode contornar não é costura: bastaria um
    /// `Codificador::novo` aqui para o codificador por hardware nascer valendo
    /// em metade dos caminhos. O compilador não pega isso — `Box::new` de um
    /// concreto satisfaz `Box<dyn CodificaVideo>` sem reclamar —, então quem
    /// pega é este teste, lendo o próprio arquivo.
    ///
    /// Ler o fonte é o mesmo recurso que `apps/seele-app/tests/frontend.rs` usa
    /// para as regras que não viram tipo. Aqui ele cobre uma invariante de uma
    /// linha e é o único jeito de cobri-la.
    #[test]
    fn a_cola_nao_arma_codificador_por_fora_da_costura() {
        let fonte = include_str!("video.rs");
        // A agulha é montada e não escrita: um literal com a chamada inteira
        // estaria **neste arquivo**, que é o arquivo que o teste lê, e ele
        // falharia sozinho. Foi o que aconteceu na primeira versão.
        let agulha = format!("{}::novo(", "Codificador");
        assert!(
            !fonte.contains(&agulha),
            "`video.rs` voltou a armar um `Codificador` concreto.\n\
             Quem arma é `seele_video::codec::armar`, e é lá que o codificador \
             por hardware entra com queda para o OpenH264: uma chamada direta \
             aqui fica de fora dessa escolha e some do dia da troca."
        );
    }

    /// Uma captura de mentira, para provar a cola sem uma tela na frente.
    ///
    /// A regra do §1 não é imitada aqui de propósito: quem descarta é a
    /// captura de verdade, e uma imitação que descartasse provaria a imitação.
    #[derive(Debug, Default)]
    struct FonteDeMentira {
        quadros: Mutex<Vec<QuadroI420>>,
    }

    impl FonteDeMentira {
        fn com(quadros: Vec<QuadroI420>) -> Self {
            Self {
                quadros: Mutex::new(quadros),
            }
        }
    }

    impl FonteDeQuadros for FonteDeMentira {
        fn tomar(&self) -> Option<QuadroI420> {
            self.quadros.lock().ok()?.pop()
        }
    }

    /// Onde procurar o módulo do Cisco, na ordem: o que quem roda apontou,
    /// depois a pasta de build.
    fn pastas() -> Vec<PathBuf> {
        let mut pastas = Vec::new();
        if let Some(apontado) = std::env::var_os("SEELE_OPENH264") {
            let caminho = PathBuf::from(apontado);
            pastas.push(if caminho.is_dir() {
                caminho
            } else {
                caminho.parent().map_or(caminho.clone(), PathBuf::from)
            });
        }
        pastas.push(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("target"),
        );
        pastas
    }

    /// A biblioteca, ou `None` com o motivo impresso.
    ///
    /// **Pula em vez de falhar, e o motivo é a licença**, o mesmo de
    /// `seele-video/tests/ida_e_volta.rs`: o módulo do Cisco não pode morar
    /// neste repositório, e um teste que o exigisse seria vermelho em toda
    /// máquina limpa — um teste sempre vermelho é um teste que todo mundo
    /// aprende a ignorar.
    fn biblioteca() -> Option<BibliotecaDeVideo> {
        match BibliotecaDeVideo::procurar_e_carregar(&pastas()) {
            Ok(biblioteca) => Some(biblioteca),
            Err(motivo) => {
                let onde = modulo::publicado_para_este_sistema()
                    .map_or_else(|| "—".to_owned(), |m| m.url());
                // Ver `seele-video/tests/ida_e_volta.rs`: onde o codec é
                // exigido, faltar é falha e não licença para pular.
                // Só onde **há** módulo publicado. No Linux o Cisco não
                // publica nada, e ali pular é a resposta certa e não um buraco.
                assert!(
                    std::env::var_os("SEELE_EXIGE_CODEC").is_none()
                        || modulo::publicado_para_este_sistema().is_none(),
                    "SEELE_EXIGE_CODEC está ligado, este sistema tem módulo publicado \
                     e ele não está aqui: {motivo}.\n  Buscar: {onde}"
                );
                eprintln!(
                    "PULADO: {motivo}.\n  O produto não vem com codec, e é a licença que impõe \
                     isso.\n  Busque {onde} e aponte-o com SEELE_OPENH264.\n  Ligue \
                     SEELE_EXIGE_CODEC para que faltar vire falha em vez de pulo."
                );
                None
            }
        }
    }

    /// Um quadro com bordas duras, que é o conteúdo caro de uma tela de
    /// trabalho. Um quadro chapado sairia com trinta bytes e não provaria nada.
    fn quadro(resolucao: Resolucao, passo: usize) -> QuadroI420 {
        let (largura, altura) = (resolucao.largura(), resolucao.altura());
        let mut y = Vec::with_capacity(largura * altura);
        for linha in 0..altura {
            for coluna in 0..largura {
                let claro = ((coluna + passo) / 8 + linha / 12).is_multiple_of(2);
                y.push(if claro { 235 } else { 16 });
            }
        }
        let croma = vec![128; largura.div_ceil(2) * altura.div_ceil(2)];
        QuadroI420::novo(largura, altura, y, croma.clone(), croma)
            .expect("os planos de um I420 montado aqui")
    }

    #[test]
    fn o_teto_decide_a_configuracao_do_codificador_sem_precisar_de_codificador() {
        // A decisão inteira é aritmética, e esta é ela: o degrau sai do teto
        // (§5.1) e a escolha da pessoa fica por cima como teto (§5).
        let fibra = Teto::Bps(4_000_000);
        let config = config_para(fibra, Resolucao::P1080, Cadencia::Q30, Prioridade::Nitidez)
            .expect("um teto de 4 Mbps compra alguma coisa");
        assert_eq!(config.resolucao, Resolucao::P1080);
        assert_eq!(config.teto_bps, 4_000_000);

        // A mesma fibra, com quem escolheu 540p: continua 540p.
        let modesto = config_para(fibra, Resolucao::P540, Cadencia::Q8, Prioridade::Nitidez)
            .expect("um teto de 4 Mbps compra alguma coisa");
        assert_eq!(modesto.resolucao, Resolucao::P540);
        assert_eq!(modesto.cadencia, Cadencia::Q8);

        // E o teto apertado não obedece a quem pediu 1080p.
        let apertado = config_para(
            Teto::Bps(500_000),
            Resolucao::P1080,
            Cadencia::Q30,
            Prioridade::Nitidez,
        )
        .expect("500 kbps ainda compram 540p");
        assert_eq!(apertado.resolucao, Resolucao::P540);

        // Parado não tem configuração: não há como armar um codificador para
        // uma transmissão que o §3.2 acabou de recusar.
        assert!(config_para(
            Teto::Parado(MotivoDeParada::SinalCritico),
            Resolucao::P720,
            Cadencia::Q30,
            Prioridade::Nitidez,
        )
        .is_none());
    }

    #[test]
    fn a_sala_que_cresce_aperta_o_codificador_e_nao_a_voz() {
        // O caminho completo do §5.1, e é o que esta cola existe para fazer:
        // alguém entra na sala → a perna de quem hospeda é dividida por mais um
        // → o teto cai → o codificador obedece. Sem o codificador na mão o
        // teste ainda prova a metade que decide.
        let Some(biblioteca) = biblioteca() else {
            return;
        };

        // Uma casa que hospeda com 6 Mbps de subida, sozinha na sala.
        let sozinho = TetoDeVideo::com_caminho(6_000_000)
            .com_caminho_de_quem_hospeda(6_000_000)
            .com_espectadores(1);
        let mut compartilhamento = Compartilhamento::abrir(
            biblioteca,
            &sozinho,
            SignalBand::Nominal,
            Resolucao::P1080,
            Cadencia::Q30,
            Prioridade::Nitidez,
        )
        .expect("armar o codificador com 3,6 Mbps de teto");
        assert_eq!(compartilhamento.resolucao(), Resolucao::P1080);
        assert_eq!(compartilhamento.teto(), Teto::Bps(3_600_000));

        // Entra a segunda pessoa: 3,6 Mbps ÷ 2 = 1,8, que ainda compra 1080p.
        let a_dois = sozinho.com_espectadores(2);
        assert_eq!(
            compartilhamento
                .ajustar(&a_dois, SignalBand::Nominal)
                .expect("baixar a banda do codificador"),
            Ajuste::TetoNovo {
                teto_bps: 1_800_000
            }
        );
        assert_eq!(compartilhamento.resolucao(), Resolucao::P1080);

        // Entra a terceira: 1,2 Mbps, e aí o degrau cai — é a linha do §5.1,
        // «a 1200 kbps o 1080p joga fora um sexto do que captura».
        let a_tres = sozinho.com_espectadores(3);
        assert_eq!(
            compartilhamento
                .ajustar(&a_tres, SignalBand::Nominal)
                .expect("baixar a banda do codificador"),
            Ajuste::ResolucaoPedida {
                de: Resolucao::P1080,
                para: Resolucao::P720,
                teto_bps: 1_200_000,
            }
        );
        // E o codificador **continua** em 1080p até alguém reabrir o fluxo: a
        // resolução mora no cabeçalho de abertura (§3.6), e trocá-la por baixo
        // faria quem recebe decodificar lixo.
        assert_eq!(compartilhamento.resolucao(), Resolucao::P1080);

        compartilhamento
            .refazer_com(Resolucao::P720)
            .expect("refazer o codificador em 720p");
        assert_eq!(compartilhamento.resolucao(), Resolucao::P720);

        // E a voz nunca cedeu: os 40% da subida desta casa continuam de pé em
        // toda a escada acima.
        for espectadores in [1, 2, 3] {
            assert_eq!(
                sozinho.com_espectadores(espectadores).reserva_da_voz(),
                2_400_000
            );
        }
    }

    #[test]
    fn a_cola_vai_da_captura_ao_quadro_codificado() {
        let Some(biblioteca) = biblioteca() else {
            return;
        };

        let teto = TetoDeVideo::com_caminho(CAMINHO_DA_PROVA_BPS);
        let mut compartilhamento = Compartilhamento::abrir(
            biblioteca,
            &teto,
            SignalBand::Nominal,
            Resolucao::P720,
            Cadencia::Q30,
            Prioridade::Nitidez,
        )
        .expect("armar o codificador no cano da prova");

        // Uma captura sem quadro novo não é uma captura morta (§ a WGC só
        // entrega quando a tela muda).
        let vazia = FonteDeMentira::default();
        assert_eq!(
            compartilhamento
                .passo(&vazia, false)
                .expect("um tique sem quadro"),
            Passo::SemQuadro
        );

        // E com quadro, sai H.264 de verdade: o primeiro é chave, com SPS e PPS
        // na frente, que é o que faz quem recebe conseguir abrir o fluxo.
        let fonte =
            FonteDeMentira::com(vec![quadro(Resolucao::P720, 8), quadro(Resolucao::P720, 0)]);
        let Passo::Quadro(primeiro) = compartilhamento
            .passo(&fonte, true)
            .expect("codificar o primeiro quadro")
        else {
            panic!("o primeiro quadro tinha de sair, e sair como chave");
        };
        assert!(primeiro.chave, "o primeiro quadro de um fluxo é chave");
        assert!(
            primeiro.bytes.starts_with(&[0, 0, 0, 1]),
            "Annex-B começa com um código de início"
        );
        assert!(!primeiro.bytes.is_empty());
    }

    #[test]
    fn um_teto_parado_nao_arma_codificador_nenhum() {
        let Some(biblioteca) = biblioteca() else {
            return;
        };
        // §3.2: quem para é o vídeo, com motivo. Armar um codificador para
        // depois não usá-lo seria gastar memória para dizer «não».
        let teto = TetoDeVideo::com_caminho(PISO_DE_BANDA_BPS);
        let erro = Compartilhamento::abrir(
            biblioteca,
            &teto,
            SignalBand::Nominal,
            Resolucao::P720,
            Cadencia::Q30,
            Prioridade::Nitidez,
        )
        .expect_err("120 kbps estão abaixo do piso");
        assert!(matches!(
            erro,
            ErroDeCompartilhamento::Parado(MotivoDeParada::AbaixoDoPiso)
        ));
    }
}
