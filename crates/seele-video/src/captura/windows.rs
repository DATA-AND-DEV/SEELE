//! Captura no Windows, pela Windows Graphics Capture.
//!
//! # A escolha, e o que ela custa
//!
//! **`windows-capture` 2.0.1**, que é o que o §1 decidiu. As duas opções que o
//! §1 pesou continuam ambas verdadeiras, e o que desempata não é o tamanho da
//! árvore:
//!
//! - a alternativa nomeada — chamar a WGC direto pelo `windows` 0.61 que o
//!   Tauri já põe na árvore — acrescenta **zero** crates e custa `unsafe`. E
//!   `unsafe_code` é **`forbid`** no workspace, que é um nível que nenhum
//!   `allow` relaxa: escolhê-la não seria escrever este arquivo, seria abrir
//!   uma exceção nomeada num `Cargo.toml` de crate, como `seele-ffi` e a
//!   binding de áudio têm. A spec já respondeu isso com todas as letras —
//!   *«Não é para v1; é a saída se a família duplicada incomodar»*;
//! - o preço aceito, então, é o que o §1 escreveu: `windows` **0.62** ao lado
//!   do 0.61 do Tauri (o `deny.toml` marca `multiple-versions = "warn"`, ou
//!   seja aviso e não falha), mais a família de metadados duplicada, mais
//!   `rayon`. Recontado numa máquina Windows com `cargo tree --edges normal`,
//!   a conta do §1 bate na vírgula: **31 crates**, o `windows-capture` e mais
//!   trinta. Onze deles são a família `windows-*` 0.62, que é a duplicação que
//!   a tabela nomeia.
//!
//! **O segundo pool de threads não chega a existir**, e isso é verificável em
//! vez de esperado: em `windows-capture` 2.0.1 o `rayon` aparece em exatamente
//! dois lugares — `FrameBuffer::as_nopadding_buffer` e o caminho da DXGI
//! Desktop Duplication. Este arquivo não chama nenhum dos dois: lê o buffer
//! cru e respeita o `row_pitch` na conversão, que também poupa uma cópia do
//! quadro inteiro por quadro. O pool do `rayon` é construído na primeira
//! `par_iter`, e ela nunca acontece. O peso no binário fica; as threads, não.
//!
//! # O que o sistema cobra, e o que ele não cobra
//!
//! **Ninguém pergunta nada.** A WGC não tem prompt: o único consentimento é o
//! da nossa interface (§4). Não há aqui nenhuma função de «pedir permissão»
//! porque não há a quem pedir — e a ausência é o motivo de a interface ter de
//! ser mais explícita do que o sistema exige.
//!
//! **A borda amarela sai, quando o Windows deixa.** O `windows-capture` oferece
//! [`DrawBorderSettings::WithoutBorder`], que chama `SetIsBorderRequired(false)`.
//!
//! Este cabeçalho dizia, com todas as letras, que isso exigia a capacidade
//! `graphicsCaptureWithoutBorder` num manifesto de pacote — que um instalador
//! NSIS não tem — e que portanto a borda ficava. **Estava errado, e o erro era
//! meu por escrever de cabeça o que a documentação diz de uma rota e não da
//! outra.** `spikes/tela-sem-borda` mediu num Windows 11 build 26200, com este
//! mesmo crate e sem manifesto nenhum: a chamada passa e o contorno some.
//!
//! O que **não** dá para assumir é que passa em toda parte. Builds mais antigos
//! não têm a propriedade, e não há como saber sem tentar:
//! `GraphicsCaptureApi::is_border_settings_supported` só confere se a
//! propriedade **existe** no sistema, não se este processo pode usá-la — ela
//! responderia `true` e a captura morreria ao começar.
//!
//! Daí a forma de [`ligar`]: tenta sem a borda, e se a sessão não abrir, abre
//! de novo com ela. Perguntar e confiar na resposta trocaria uma borda amarela
//! por um botão de compartilhar que falha, que é o defeito que o §2 proíbe por
//! escrito.
//!
//! # A cadência não é nossa, e este é o achado que muda o desenho de quem chama
//!
//! A WGC **só entrega quadro quando o conteúdo muda**. Uma tela parada entrega
//! zero quadros por segundo, indefinidamente, sem erro nenhum — não é falha, é
//! a API funcionando. Quem transmite não pode ler «nada chegou» como «a captura
//! morreu», e quem recebe não pode ficar sem imagem por causa disso: o último
//! quadro entregue continua valendo até o próximo. O teto de quadros
//! ([`Cadencia`]) é, aqui, só um teto — como tudo no §5.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame;
use windows_capture::graphics_capture_api::{GraphicsCaptureApi, InternalCaptureControl};
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    GraphicsCaptureItemType, MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;

use super::reamostragem::Mapa;
use crate::codec::{Cadencia, QuadroI420, Resolucao};
use crate::erro::ErroDeVideo;

/// Por que a captura não aconteceu, sempre com um nome.
///
/// **Isto devia morar em [`crate::erro::ErroDeVideo`]**, ao lado dos motivos do
/// módulo e do codec — `specs/02-protocolo.md` manda enumerar os motivos num
/// lugar só, e um segundo enum é uma fronteira que ninguém consegue defender.
/// Está aqui porque quem escreveu este arquivo não é dono de `erro.rs`, e não
/// porque a separação seja certa: está no relatório para quem coordena juntar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroDeCaptura {
    /// O sistema não listou monitor nenhum.
    ///
    /// Acontece numa sessão sem área de trabalho — um serviço, um contêiner —,
    /// e é o estado que a interface tem de saber distinguir de «falhou».
    SemMonitor,

    /// A janela pedida não existe mais.
    ///
    /// É estado normal: entre listar e transmitir, alguém fecha a janela. Quem
    /// mostra isto para gente pede outra escolha, não mostra um alerta.
    AlvoSumiu {
        /// Como ele se chamava quando foi listado.
        nome: String,
    },

    /// A Windows Graphics Capture recusou a operação.
    ///
    /// A mensagem crua vai junto porque nomeia a chamada e um `HRESULT`: serve
    /// a quem depura e não diz nada a quem usa.
    SistemaRecusou {
        /// O que se estava tentando fazer, em português.
        operacao: &'static str,
        /// O que a biblioteca disse.
        detalhe: String,
    },

    /// A conversão para I420 produziu um quadro que o codificador recusaria.
    ///
    /// Não deveria acontecer — os tamanhos saem de [`Resolucao`] —, e é por
    /// isso que ele não vira um `panic`: o caminho existe para que um erro de
    /// conta aqui apareça como motivo enumerado em vez de derrubar a chamada.
    QuadroInvalido(ErroDeVideo),
}

impl std::fmt::Display for ErroDeCaptura {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SemMonitor => write!(f, "este sistema não tem monitor para capturar"),
            Self::AlvoSumiu { nome } => write!(f, "a janela «{nome}» não existe mais"),
            Self::SistemaRecusou { operacao, detalhe } => {
                write!(f, "o Windows recusou {operacao}: {detalhe}")
            }
            Self::QuadroInvalido(erro) => write!(f, "a conversão para I420 falhou: {erro}"),
        }
    }
}

impl std::error::Error for ErroDeCaptura {}

impl From<ErroDeVideo> for ErroDeCaptura {
    fn from(erro: ErroDeVideo) -> Self {
        Self::QuadroInvalido(erro)
    }
}

/// De onde os pixels vêm: um monitor ou uma janela.
///
/// **Quem desenha a lista somos nós**, que é o que o §4 diz do Windows — ao
/// contrário do Linux, onde só o compositor sabe o que existe. O
/// `GraphicsCapturePicker` do sistema também serviria; uma lista nossa é o que
/// deixa a mesma interface valer no macOS, onde ela é obrigatória.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fonte {
    Monitor(Monitor),
    Janela(Window),
}

/// Uma coisa que se pode transmitir, com o rótulo que a interface mostra.
#[derive(Debug, Clone)]
pub struct Alvo {
    nome: String,
    largura: u32,
    altura: u32,
    fonte: Fonte,
}

impl Alvo {
    /// O rótulo, já pronto para a tela de escolha.
    #[must_use]
    pub fn nome(&self) -> &str {
        &self.nome
    }

    /// O tamanho de agora, em pixels.
    ///
    /// **Não é promessa**: uma janela muda de tamanho enquanto é transmitida, e
    /// a captura acompanha. Serve para a tela de escolha dizer o que a pessoa
    /// está prestes a mandar.
    #[must_use]
    pub const fn tamanho(&self) -> (u32, u32) {
        (self.largura, self.altura)
    }

    /// Se é um monitor inteiro, e não uma janela.
    #[must_use]
    pub const fn e_monitor(&self) -> bool {
        matches!(self.fonte, Fonte::Monitor(_))
    }
}

/// Os monitores desta máquina, na ordem em que o sistema os enumera.
///
/// # Errors
///
/// [`ErroDeCaptura::SemMonitor`] quando não há nenhum — uma sessão sem área de
/// trabalho é o caso normal disso, não uma falha.
pub fn listar_monitores() -> Result<Vec<Alvo>, ErroDeCaptura> {
    let monitores = Monitor::enumerate().map_err(|erro| ErroDeCaptura::SistemaRecusou {
        operacao: "listar os monitores",
        detalhe: erro.to_string(),
    })?;

    let alvos: Vec<Alvo> = monitores
        .into_iter()
        .filter_map(|monitor| {
            // Um monitor que some entre o `enumerate` e o `name` não é erro da
            // lista inteira: é um a menos. Devolver `Err` aqui deixaria a
            // pessoa sem escolha nenhuma por causa de um monitor desconectado
            // no instante errado.
            let nome = monitor.name().ok()?;
            let largura = monitor.width().ok()?;
            let altura = monitor.height().ok()?;
            Some(Alvo {
                nome,
                largura,
                altura,
                fonte: Fonte::Monitor(monitor),
            })
        })
        .collect();

    if alvos.is_empty() {
        return Err(ErroDeCaptura::SemMonitor);
    }
    Ok(alvos)
}

/// As janelas que dá para transmitir, com o título que elas mostram.
///
/// # Errors
///
/// [`ErroDeCaptura::SistemaRecusou`] se a enumeração falhar. Uma lista **vazia**
/// não é erro: é uma máquina sem janela aberta.
pub fn listar_janelas() -> Result<Vec<Alvo>, ErroDeCaptura> {
    let janelas = Window::enumerate().map_err(|erro| ErroDeCaptura::SistemaRecusou {
        operacao: "listar as janelas",
        detalhe: erro.to_string(),
    })?;

    Ok(janelas
        .into_iter()
        .filter_map(|janela| {
            let nome = janela.title().ok()?;
            // Uma janela sem título é quase sempre uma janela-ferramenta que
            // ninguém quer transmitir, e é uma linha em branco na lista.
            if nome.trim().is_empty() {
                return None;
            }
            let largura = u32::try_from(janela.width().ok()?).ok()?;
            let altura = u32::try_from(janela.height().ok()?).ok()?;
            Some(Alvo {
                nome,
                largura,
                altura,
                fonte: Fonte::Janela(janela),
            })
        })
        .collect())
}

/// A caixa de **um** quadro, que é a regra do §1 feita de código.
///
/// **Descarta, nunca enfileira.** Um quadro que chega enquanto o anterior ainda
/// não foi retirado **substitui** o anterior. `spikes/tela-no-codec` mediu as
/// duas políticas lado a lado, alimentando o encoder acima do que ele
/// sustentava: enfileirando, a idade do quadro que sai cresce sem limite —
/// 958 ms de mediana e 1,87 s no pior caso em oito segundos de corrida, com
/// 1165 quadros parados na fila; descartando, a idade fica em **3 ms** e não
/// anda.
#[derive(Debug, Default)]
struct Caixa {
    quadro: Mutex<Option<QuadroI420>>,
    convertidos: AtomicU64,
    descartados: AtomicU64,
    entregues: AtomicU64,
    fora_de_ritmo: AtomicU64,
    falhas: AtomicU64,
    nanos_convertendo: AtomicU64,
}

impl Caixa {
    /// Põe o quadro novo, jogando fora o que ainda estava lá.
    fn depositar(&self, quadro: QuadroI420) {
        // Um `Mutex` envenenado significa que a thread da WGC entrou em pânico
        // segurando-o. Recuperar o conteúdo e seguir é o certo aqui: o pior que
        // há dentro é um quadro pela metade, e a alternativa — propagar — seria
        // derrubar a chamada de voz por causa de um quadro de tela, que é
        // exatamente a inversão que o §3.2 proíbe.
        let mut caixa = self.quadro.lock().unwrap_or_else(PoisonError::into_inner);
        if caixa.replace(quadro).is_some() {
            self.descartados.fetch_add(1, Ordering::Relaxed);
        }
        self.convertidos.fetch_add(1, Ordering::Relaxed);
    }

    /// Tira o quadro que estiver lá, se houver.
    fn retirar(&self) -> Option<QuadroI420> {
        let mut caixa = self.quadro.lock().unwrap_or_else(PoisonError::into_inner);
        let quadro = caixa.take();
        if quadro.is_some() {
            self.entregues.fetch_add(1, Ordering::Relaxed);
        }
        quadro
    }
}

/// Quem recebe cada quadro da WGC, converte e deposita na caixa.
///
/// Mora na thread que o `windows-capture` cria, e é a única coisa que roda
/// nela. O §2 manda o **codificador** morar numa thread própria; esta é a
/// anterior, e a caixa de um quadro é a fronteira entre as duas.
struct Manipulador {
    caixa: Arc<Caixa>,
    destino: Resolucao,
    intervalo: Duration,
    ultimo: Option<Instant>,
    mapa: Option<Mapa>,
}

/// A folga do ritmo, em oitavos do intervalo.
///
/// Sem ela, uma fonte que entrega exatamente na cadência pedida perderia um
/// quadro sim, outro não: basta o quadro chegar um microssegundo antes da hora
/// para ser recusado, e o seguinte já vem um intervalo depois desse. Sete
/// oitavos aceitam o que chegou quase na hora e continuam recusando o dobro da
/// cadência.
const FOLGA_DO_RITMO: u32 = 7;

impl GraphicsCaptureApiHandler for Manipulador {
    type Flags = (Arc<Caixa>, Resolucao, Duration);
    type Error = ErroDeCaptura;

    fn new(contexto: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let (caixa, destino, intervalo) = contexto.flags;
        Ok(Self {
            caixa,
            destino,
            intervalo,
            ultimo: None,
            mapa: None,
        })
    }

    fn on_frame_arrived(
        &mut self,
        quadro: &mut Frame<'_>,
        _controle: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        let agora = Instant::now();
        // O ritmo é conferido **antes** de tocar na textura: recusar depois de
        // converter gastaria a cópia do D3D11 e a conversão inteira para jogar
        // fora. O teto de quadros do §5 é teto, e um teto que custa o preço do
        // quadro não teria por que existir.
        if let Some(ultimo) = self.ultimo {
            if agora.duration_since(ultimo) * 8 < self.intervalo * FOLGA_DO_RITMO {
                self.caixa.fora_de_ritmo.fetch_add(1, Ordering::Relaxed);
                return Ok(());
            }
        }

        let mut buffer = match quadro.buffer() {
            Ok(buffer) => buffer,
            Err(_) => {
                // Uma cópia da GPU que falha é um quadro perdido, não o fim da
                // transmissão: devolver `Err` aqui derrubaria a captura inteira
                // por causa de um quadro. Fica contado, e quem olha o contador
                // vê se é um caso isolado ou o desenho quebrado.
                self.caixa.falhas.fetch_add(1, Ordering::Relaxed);
                return Ok(());
            }
        };

        let origem = (buffer.width() as usize, buffer.height() as usize);
        let passo = buffer.row_pitch() as usize;
        if self.mapa.as_ref().is_none_or(|mapa| mapa.origem != origem) {
            self.mapa = Some(Mapa::novo(origem, self.destino));
        }
        let Some(mapa) = self.mapa.as_ref() else {
            return Ok(());
        };

        // `as_raw_buffer` e não `as_nopadding_buffer`: o segundo copia o quadro
        // inteiro só para tirar o preenchimento de fim de linha, e é o único
        // lugar do `windows-capture` que entra no `rayon`. A conversão já sabe
        // andar de `row_pitch` em `row_pitch`.
        let comeco = Instant::now();
        let convertido = mapa.converter(buffer.as_raw_buffer(), passo)?;
        // O relógio fica aqui, e não em quem chama, porque o custo que
        // interessa é o desta thread: `spikes/tela-no-codec` mediu o
        // codificador e disse, na cara, que a conversão de espaço de cor de uma
        // captura de verdade *«soma ao do encoder»* e não estava medida. Este
        // contador é o que a faz medida em vez de estimada.
        self.caixa.nanos_convertendo.fetch_add(
            u64::try_from(comeco.elapsed().as_nanos()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        self.ultimo = Some(agora);
        self.caixa.depositar(convertido);
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        // A janela transmitida fechou. Não é erro — é o fim da transmissão, e
        // quem retira da caixa descobre pelo `esta_viva`.
        Ok(())
    }
}

/// Uma transmissão de tela em andamento.
///
/// Segura a thread da WGC e a caixa de um quadro. Parar é [`Captura::parar`],
/// e largar sem parar também para: o [`Drop`] manda a thread embora, porque
/// uma captura que continua depois que ninguém a segura é uma borda amarela
/// que não vai embora da tela de quem compartilha.
pub struct Captura {
    controle: Option<CaptureControl<Manipulador, ErroDeCaptura>>,
    caixa: Arc<Caixa>,
    alvo: String,
}

/// Liga a captura para um item que a WGC saiba converter.
///
/// **Tenta sem a borda e cai para a borda se o sistema recusar**, nesta ordem e
/// nunca ao contrário. Ver [`sem_borda_ou_com`].
fn ligar<T>(
    item: T,
    caixa: Arc<Caixa>,
    destino: Resolucao,
    intervalo: Duration,
    do_sistema: MinimumUpdateIntervalSettings,
) -> Result<CaptureControl<Manipulador, ErroDeCaptura>, ErroDeCaptura>
where
    T: TryInto<GraphicsCaptureItemType> + Clone + Send + 'static,
{
    match ligar_com_borda(
        item.clone(),
        Arc::clone(&caixa),
        destino,
        intervalo,
        do_sistema,
        DrawBorderSettings::WithoutBorder,
    ) {
        Ok(controle) => Ok(controle),
        Err(erro) => {
            // Não é defeito: é um Windows que não deixa este app desligar a
            // borda. A alternativa seria devolver o erro, e aí uma borda
            // amarela teria virado um botão de compartilhar que falha — o
            // defeito que o §2 proíbe por escrito.
            // Sem `tracing`: este crate não o tem na árvore, e trazê-lo por
            // causa de uma linha seria pagar caro por um recado que não é de
            // erro. Quem quiser o detalhe roda `spikes/tela-sem-borda`.
            let _ = &erro;
            ligar_com_borda(
                item,
                caixa,
                destino,
                intervalo,
                do_sistema,
                DrawBorderSettings::Default,
            )
        }
    }
}

/// A metade que fala com a WGC, com a decisão da borda vinda de fora.
fn ligar_com_borda<T>(
    item: T,
    caixa: Arc<Caixa>,
    destino: Resolucao,
    intervalo: Duration,
    do_sistema: MinimumUpdateIntervalSettings,
    borda: DrawBorderSettings,
) -> Result<CaptureControl<Manipulador, ErroDeCaptura>, ErroDeCaptura>
where
    T: TryInto<GraphicsCaptureItemType> + Send + 'static,
{
    let ajustes = Settings::new(
        item,
        // O cursor vai junto porque quem mostra a tela está apontando para
        // coisas nela. O §6 item 12 tira de v1 o cursor **desenhado à parte** —
        // um segundo ponteiro, com anotação —, e não o que a composição já põe
        // no quadro de graça.
        CursorCaptureSettings::WithCursor,
        borda,
        SecondaryWindowSettings::Default,
        do_sistema,
        DirtyRegionSettings::Default,
        // BGRA e não RGBA: é a ordem em que a composição já tem os pixels, e
        // pedir a outra faria o sistema trocar os canais antes de nos entregar.
        ColorFormat::Bgra8,
        (caixa, destino, intervalo),
    );

    Manipulador::start_free_threaded(ajustes).map_err(|erro| ErroDeCaptura::SistemaRecusou {
        operacao: "começar a captura",
        detalhe: erro.to_string(),
    })
}

impl Captura {
    /// Começa a transmitir um alvo, com a resolução e o teto de quadros que a
    /// pessoa escolheu.
    ///
    /// Os dois são **teto** (§5): a resolução segura e o quadro cede, e nada
    /// aqui promete entregar a cadência pedida — a WGC só entrega quadro quando
    /// o conteúdo muda.
    ///
    /// # Errors
    ///
    /// [`ErroDeCaptura::SistemaRecusou`] se a WGC não abrir a sessão, e
    /// [`ErroDeCaptura::AlvoSumiu`] se a janela tiver fechado entre a lista e
    /// aqui.
    pub fn iniciar(
        alvo: &Alvo,
        resolucao: Resolucao,
        cadencia: Cadencia,
    ) -> Result<Self, ErroDeCaptura> {
        let caixa = Arc::new(Caixa::default());
        let intervalo = Duration::from_nanos(u64::from(1_000_000_000 / cadencia.hz()));

        // O freio do sistema poupa a cópia da GPU antes mesmo de o quadro
        // chegar, mas só existe em builds recentes do Windows, e a própria
        // documentação dele diz que **não** garante cadência nenhuma: é um
        // teto, como tudo no §5. Por isso o ritmo é conferido no manipulador de
        // qualquer jeito, e este ajuste é só um desconto quando há.
        let do_sistema = match GraphicsCaptureApi::is_minimum_update_interval_supported() {
            Ok(true) => MinimumUpdateIntervalSettings::Custom(intervalo),
            _ => MinimumUpdateIntervalSettings::Default,
        };

        let controle = match alvo.fonte {
            Fonte::Monitor(monitor) => ligar(
                monitor,
                Arc::clone(&caixa),
                resolucao,
                intervalo,
                do_sistema,
            )?,
            Fonte::Janela(janela) => {
                if !janela.is_valid() {
                    return Err(ErroDeCaptura::AlvoSumiu {
                        nome: alvo.nome.clone(),
                    });
                }
                ligar(janela, Arc::clone(&caixa), resolucao, intervalo, do_sistema)?
            }
        };

        Ok(Self {
            controle: Some(controle),
            caixa,
            alvo: alvo.nome.clone(),
        })
    }

    /// O quadro mais recente, se houver um que ainda não foi retirado.
    ///
    /// `None` não é falha: ou nada mudou na tela desde a última vez, ou o
    /// codificador está mais rápido que a fonte.
    #[must_use]
    pub fn pegar(&self) -> Option<QuadroI420> {
        self.caixa.retirar()
    }

    /// Se a thread da captura continua de pé.
    ///
    /// Vira `false` quando a janela transmitida fecha, e é assim que quem
    /// transmite descobre que acabou sem ter pedido.
    #[must_use]
    pub fn esta_viva(&self) -> bool {
        self.controle
            .as_ref()
            .is_some_and(|controle| !controle.is_finished())
    }

    /// O nome do alvo, como ele foi listado.
    #[must_use]
    pub fn alvo(&self) -> &str {
        &self.alvo
    }

    /// Quantos quadros foram convertidos e depositados.
    #[must_use]
    pub fn convertidos(&self) -> u64 {
        self.caixa.convertidos.load(Ordering::Relaxed)
    }

    /// Quantos quadros foram substituídos na caixa antes de alguém os retirar.
    ///
    /// **Não é defeito, é o desenho** (§1): um quadro velho entregue tarde é
    /// pior que um quadro perdido. Um número alto aqui diz que o codificador
    /// está mais devagar que a tela, que é a informação que o §5 manda mostrar.
    #[must_use]
    pub fn descartados(&self) -> u64 {
        self.caixa.descartados.load(Ordering::Relaxed)
    }

    /// Quantos quadros saíram por [`Captura::pegar`].
    #[must_use]
    pub fn entregues(&self) -> u64 {
        self.caixa.entregues.load(Ordering::Relaxed)
    }

    /// Quantos quadros a WGC entregou antes da hora e foram recusados pelo teto
    /// de cadência, sem custar conversão.
    #[must_use]
    pub fn fora_de_ritmo(&self) -> u64 {
        self.caixa.fora_de_ritmo.load(Ordering::Relaxed)
    }

    /// Quantas vezes a cópia da GPU falhou e o quadro foi perdido.
    ///
    /// Zero é o esperado. Qualquer outra coisa é para o relatório de quem
    /// depura, não para a tela de quem usa.
    #[must_use]
    pub fn falhas(&self) -> u64 {
        self.caixa.falhas.load(Ordering::Relaxed)
    }

    /// Quanto tempo se gastou convertendo BGRA em I420, somado.
    ///
    /// Dividido por [`Captura::convertidos`], dá o custo de um quadro — que é o
    /// número que falta na tabela de `spikes/tela-no-codec`, onde só o
    /// codificador foi medido.
    #[must_use]
    pub fn nanos_convertendo(&self) -> u64 {
        self.caixa.nanos_convertendo.load(Ordering::Relaxed)
    }

    /// Para a transmissão e espera a thread terminar.
    ///
    /// # Errors
    ///
    /// [`ErroDeCaptura::SistemaRecusou`] se a thread não puder ser avisada nem
    /// esperada.
    pub fn parar(mut self) -> Result<(), ErroDeCaptura> {
        match self.controle.take() {
            Some(controle) => controle
                .stop()
                .map_err(|erro| ErroDeCaptura::SistemaRecusou {
                    operacao: "parar a captura",
                    detalhe: erro.to_string(),
                }),
            None => Ok(()),
        }
    }
}

impl Drop for Captura {
    fn drop(&mut self) {
        if let Some(controle) = self.controle.take() {
            // O erro de parar não tem para onde ir num `Drop`, e é por isso que
            // [`Captura::parar`] existe: quem quer saber, chama-o.
            let _ = controle.stop();
        }
    }
}

impl std::fmt::Debug for Captura {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Captura")
            .field("alvo", &self.alvo)
            .field("convertidos", &self.convertidos())
            .field("descartados", &self.descartados())
            .field("entregues", &self.entregues())
            .field("fora_de_ritmo", &self.fora_de_ritmo())
            .field("falhas", &self.falhas())
            .finish()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Uma origem BGRA de uma cor só, com o preenchimento de fim de linha

    #[test]
    fn a_caixa_descarta_o_velho_e_conta() {
        let caixa = Caixa::default();
        caixa.depositar(QuadroI420::preto(4, 4));
        caixa.depositar(QuadroI420::preto(8, 8));

        let quadro = caixa.retirar().expect("a caixa tinha um quadro");
        // O que sai é o **novo**. Se um dia sair o velho, a fila que o §1
        // proíbe voltou por baixo, e a idade do quadro volta a crescer.
        assert_eq!(quadro.largura(), 8);
        assert_eq!(caixa.descartados.load(Ordering::Relaxed), 1);
        assert_eq!(caixa.convertidos.load(Ordering::Relaxed), 2);
        assert_eq!(caixa.entregues.load(Ordering::Relaxed), 1);
        assert!(caixa.retirar().is_none());
    }
}
