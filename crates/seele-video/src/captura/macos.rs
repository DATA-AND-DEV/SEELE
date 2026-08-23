//! Captura no macOS, pela ScreenCaptureKit.
//!
//! O §1 da spec escolheu a ScreenCaptureKit porque desde a depreciação do
//! `CGDisplayStream` ela é a única porta. Este arquivo é o passo 2 da ordem de
//! construção (§7): captura sozinha, imprimindo tamanho e cadência, na
//! plataforma de quem desenvolve — e é o passo que descobre se o §4 está certo
//! sobre o TCC.
//!
//! # O que sai daqui, e em que formato
//!
//! [`QuadroI420`], porque é o único formato que o OpenH264 aceita. Pedimos à
//! ScreenCaptureKit o formato `420v` — YCbCr 4:2:0 bi-planar, **intervalo de
//! TV** — e não BGRA, por dois motivos que se somam:
//!
//! 1. converter BGRA em I420 é uma matriz de cores por pixel, em ponto
//!    flutuante ou em tabela; converter `420v` em I420 é **desentrelaçar** o
//!    plano de croma, que é uma cópia de bytes. O compositor já fez a conta
//!    cara, na GPU, de graça;
//! 2. `420v` é intervalo de TV (16–235), que é o mesmo intervalo de
//!    [`QuadroI420::preto`] — «16 e 128 são o preto do intervalo de TV, que é o
//!    que um quadro de captura usa». Pedir `420f` traria intervalo cheio e o
//!    preto do codec deixaria de ser o preto da captura.
//!
//! **Isto não estava medido, e continua não estando.** `spikes/tela-no-codec`
//! diz na cara que a textura dele é real mas o movimento é sintetizado, e que a
//! conversão de espaço de cor e a cópia de `IOSurface` «somam ao do encoder».
//! Escolher `420v` reduz essa soma ao mínimo possível; não a mede.
//!
//! # A regra do §1, e como ela está escrita aqui
//!
//! **A captura descarta, nunca enfileira.** [`Vaga`] tem **uma** posição: o
//! quadro que chega substitui o que estava lá, e o que estava lá é contado como
//! descartado. Não há `Vec`, não há canal com fila, não há como crescer.
//!
//! # O que este arquivo **não** faz
//!
//! **Não desenha lista de escolha.** [`fontes`] devolve o que a
//! `SCShareableContent` enxerga, com rótulo e tamanho, e quem escolhe é a
//! casca. No macOS somos nós que desenhamos a lista (§4), ao contrário do
//! Linux — mas quem a desenha é a interface, não a biblioteca.
//!
//! **Não pede permissão sozinho.** [`permissao`] só olha e [`pedir_permissao`]
//! só pede; nada aqui pede por conta própria no meio de uma captura. O §4 é
//! explícito: conferir **antes de oferecer o botão**, em vez de descobrir pelo
//! fracasso.
//!
//! # O TCC, e o defeito que é diferente do microfone
//!
//! O `Info.plist` precisa ganhar `NSScreenCaptureUsageDescription`, com a mesma
//! forma e o mesmo motivo do `NSMicrophoneUsageDescription` que já está lá — e
//! o texto é para quem lê o alerta, não para o revisor de loja.
//!
//! **O `Entitlements.plist` não ganha nada.** Não existe direito de *hardened
//! runtime* para gravação de tela: não há `com.apple.security.screen-capture`
//! nem equivalente, e a ScreenCaptureKit é guardada **só** pelo TCC. O
//! comentário daquele arquivo convida a acrescentar um direito «por simetria»
//! com o do microfone; uma chave inventada não abre nada e suja um arquivo que
//! hoje diz exatamente uma coisa verdadeira (§4).
//!
//! E o defeito é outro: sem a chave do microfone o macOS nega **sem
//! perguntar**; com a tela o TCC guarda a concessão contra a **identidade
//! assinada**, e um app não assinado que muda de binário a perde a cada build.
//! O sintoma é o pior possível — funcionou ontem, hoje não, e nada mudou no
//! código.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use core_graphics::access::ScreenCaptureAccess;
use screencapturekit::cm::{CMSampleBufferExt, CMSampleBufferSCExt, CMTime};
use screencapturekit::prelude::{
    PixelFormat, SCContentFilter, SCDisplay, SCShareableContent, SCStream, SCStreamConfiguration,
    SCStreamOutputTrait, SCStreamOutputType, SCWindow,
};

use crate::codec::{Cadencia, QuadroI420, Resolucao};
use crate::erro::ErroDeVideo;

/// Por que a tela não foi capturada, sempre com um nome.
///
/// **Vive aqui e não em [`crate::erro`] por uma razão de tarefa, não de
/// desenho:** quem escreveu este arquivo não podia mexer naquele. Estes motivos
/// pertencem a `ErroDeVideo`, junto com os do módulo do Cisco, pelo mesmo
/// argumento que aquele arquivo já faz — *«todos os motivos de erro são
/// enumerados»* —, e a mudança é uma mudança de casa, não de conteúdo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroDeCaptura {
    /// O TCC não concedeu gravação de tela a este processo.
    ///
    /// **Não é falha de código, e quase nunca é falha de quem usa.** Num app
    /// não assinado a concessão é guardada contra a identidade do binário, e
    /// ela some sozinha a cada build (§4). Quem recebe isto oferece
    /// [`pedir_permissao`] e, se já tinha pedido, manda para Ajustes — o alerta
    /// do sistema só aparece **uma vez** por identidade.
    SemPermissaoDeTela,

    /// Não há nada para capturar: nenhum monitor e nenhuma janela.
    ///
    /// Acontece numa sessão sem tela — SSH, um agente de CI. É estado, não
    /// erro: o botão de compartilhar não deve estar lá.
    NadaParaCapturar,

    /// A ScreenCaptureKit recusou.
    ///
    /// A mensagem crua vai junto porque ela nomeia o domínio e o código do
    /// erro: serve a quem depura e não diz nada a quem usa.
    SistemaRecusou {
        /// O que se estava tentando fazer, em português.
        operacao: &'static str,
        /// O que a biblioteca disse.
        detalhe: String,
    },

    /// Chegou um quadro que não dá para ler.
    ///
    /// Formato de pixel inesperado, plano faltando, linha mais curta que a
    /// largura anunciada. Vale ter nome próprio porque a alternativa — entregar
    /// meio quadro — é a que produz imagem verde e ninguém sabe de onde veio.
    QuadroIlegivel {
        /// O que estava errado, em português.
        motivo: &'static str,
    },

    /// Os planos convertidos não formam um I420 do tamanho pedido.
    ///
    /// Envelopa [`ErroDeVideo::PlanosInconsistentes`], que é quem de fato
    /// confere — a conferência mora num lugar só, e é a do codec.
    PlanosRecusados(ErroDeVideo),
}

impl std::fmt::Display for ErroDeCaptura {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SemPermissaoDeTela => {
                write!(f, "o macOS não concedeu gravação de tela a este programa")
            }
            Self::NadaParaCapturar => write!(f, "não há monitor nem janela para capturar"),
            Self::SistemaRecusou { operacao, detalhe } => {
                write!(f, "a ScreenCaptureKit recusou {operacao}: {detalhe}")
            }
            Self::QuadroIlegivel { motivo } => write!(f, "quadro ilegível: {motivo}"),
            Self::PlanosRecusados(erro) => write!(f, "{erro}"),
        }
    }
}

impl std::error::Error for ErroDeCaptura {}

/// O que o TCC respondeu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permissao {
    /// Dá para capturar agora.
    Concedida,
    /// Não dá. Ou nunca se pediu, ou a concessão foi perdida.
    Ausente,
}

impl Permissao {
    /// Se dá para capturar.
    #[must_use]
    pub const fn concedida(self) -> bool {
        matches!(self, Self::Concedida)
    }
}

/// Olha se o TCC já concedeu, **sem** abrir alerta nenhum.
///
/// `CGPreflightScreenCaptureAccess`, que é o que o §4 manda chamar antes de
/// oferecer o botão. Chamar isto é barato e não tem efeito colateral: dá para
/// chamar toda vez que a tela de compartilhar abre, e é o que se deve fazer,
/// porque a concessão de um app não assinado morre a cada build.
#[must_use]
pub fn permissao() -> Permissao {
    if ScreenCaptureAccess.preflight() {
        Permissao::Concedida
    } else {
        Permissao::Ausente
    }
}

/// Pede ao TCC, abrindo o alerta do sistema se ele ainda não foi mostrado.
///
/// `CGRequestScreenCaptureAccess`. **O alerta aparece uma vez só** por
/// identidade assinada: da segunda em diante esta função devolve o mesmo que
/// [`permissao`] sem mostrar nada, e o único caminho de volta é Ajustes. Por
/// isso ela não deve ser chamada em laço nem «para conferir» — para conferir
/// existe [`permissao`].
#[must_use]
pub fn pedir_permissao() -> Permissao {
    if ScreenCaptureAccess.request() {
        Permissao::Concedida
    } else {
        Permissao::Ausente
    }
}

/// Uma coisa que dá para transmitir: um monitor ou uma janela.
///
/// «Um app ou um monitor» é a mesma escolha para nós e duas coisas diferentes
/// para o sistema (§4). No macOS quem desenha a lista somos nós, com a
/// `SCShareableContent` — ao contrário do Linux, onde só o compositor enxerga.
pub enum Fonte {
    /// Um monitor inteiro.
    Monitor {
        /// O que a `CGDirectDisplayID` chama de identificador. Sobrevive a um
        /// desligar e ligar do mesmo monitor; **não** sobrevive a trocar de
        /// monitor.
        id: u32,
        /// Largura em pixels.
        largura: u32,
        /// Altura em pixels.
        altura: u32,
        /// O objeto da ScreenCaptureKit, que é o que o filtro precisa.
        alvo: SCDisplay,
    },
    /// Uma janela isolada.
    Janela {
        /// O identificador de janela do sistema.
        id: u32,
        /// O título, quando há um. Uma janela sem título é comum e não é
        /// defeito — é a interface que resolve o que mostrar no lugar.
        titulo: Option<String>,
        /// O nome do aplicativo dono, quando há.
        aplicativo: Option<String>,
        /// O objeto da ScreenCaptureKit.
        alvo: SCWindow,
    },
}

impl Fonte {
    /// Um rótulo para a interface mostrar.
    ///
    /// Montado aqui e não na casca porque a regra de montagem é do sistema — o
    /// título pode faltar, o aplicativo pode faltar, e os dois faltando é uma
    /// janela real que a lista ainda precisa nomear.
    #[must_use]
    pub fn rotulo(&self) -> String {
        match self {
            Self::Monitor {
                id,
                largura,
                altura,
                ..
            } => format!("Monitor {id} ({largura}×{altura})"),
            Self::Janela {
                id,
                titulo,
                aplicativo,
                ..
            } => match (aplicativo.as_deref(), titulo.as_deref()) {
                (Some(app), Some(t)) => format!("{app} — {t}"),
                (Some(app), None) => app.to_owned(),
                (None, Some(t)) => t.to_owned(),
                (None, None) => format!("Janela {id}"),
            },
        }
    }
}

impl std::fmt::Debug for Fonte {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `SCDisplay` e `SCWindow` são ponteiros opacos sem `Debug`, e o
        // ponteiro não diz nada a ninguém. O rótulo diz.
        write!(f, "Fonte({})", self.rotulo())
    }
}

/// O que a `SCShareableContent` enxerga desta máquina.
///
/// # Errors
///
/// [`ErroDeCaptura::SemPermissaoDeTela`] quando o TCC não concedeu — e a
/// conferência é feita **antes** de chamar o sistema, porque sem permissão a
/// `SCShareableContent` devolve uma lista vazia ou um erro de domínio, e
/// nenhum dos dois diz que o problema é permissão.
///
/// [`ErroDeCaptura::NadaParaCapturar`] numa sessão sem tela, que é estado
/// normal e não falha.
pub fn fontes() -> Result<Vec<Fonte>, ErroDeCaptura> {
    if !permissao().concedida() {
        return Err(ErroDeCaptura::SemPermissaoDeTela);
    }

    let conteudo = SCShareableContent::create()
        // Janelas de área de trabalho e janelas escondidas não são coisas que
        // alguém escolhe transmitir; oferecê-las é uma lista de cento e tantas
        // entradas onde a pessoa procura a dela.
        .with_exclude_desktop_windows(true)
        .with_on_screen_windows_only(true)
        .get()
        .map_err(|erro| ErroDeCaptura::SistemaRecusou {
            operacao: "listar o que dá para compartilhar",
            detalhe: erro.to_string(),
        })?;

    let mut lista: Vec<Fonte> = conteudo
        .displays()
        .into_iter()
        .map(|monitor| Fonte::Monitor {
            id: monitor.display_id(),
            largura: monitor.width(),
            altura: monitor.height(),
            alvo: monitor,
        })
        .collect();

    lista.extend(conteudo.windows().into_iter().map(|janela| {
        Fonte::Janela {
            id: janela.window_id(),
            titulo: janela.title().filter(|t| !t.is_empty()),
            aplicativo: janela
                .owning_application()
                .map(|app| app.application_name())
                .filter(|n| !n.is_empty()),
            alvo: janela,
        }
    }));

    if lista.is_empty() {
        return Err(ErroDeCaptura::NadaParaCapturar);
    }
    Ok(lista)
}

/// Um quadro que saiu da tela, com o instante em que saiu.
///
/// O instante vai junto porque é o único jeito de medir a **idade** do quadro
/// que o codificador pegou, que é a grandeza que `spikes/tela-no-codec` usou
/// para decidir entre enfileirar e descartar: enfileirando ela cresce sem
/// limite (1,9 s em oito segundos de corrida), descartando ela fica em 3 ms.
#[derive(Debug, Clone)]
pub struct QuadroDaTela {
    /// Os pixels, prontos para o codificador.
    pub quadro: QuadroI420,
    /// Quando a ScreenCaptureKit entregou este quadro.
    pub capturado_em: Instant,
}

/// **Uma** posição. É aqui que a regra do §1 está escrita.
///
/// Não é uma fila com teto um: é uma posição. Um quadro que chega e encontra a
/// posição ocupada **substitui** o que estava lá, e o antigo é contado. A
/// diferença importa porque uma fila com teto um ainda teria a pergunta «e
/// quando encher?»; uma posição não tem essa pergunta.
///
/// A [`Vaga`] é a fronteira entre a thread da ScreenCaptureKit, que escreve, e
/// a thread do codificador, que lê — e o §2 manda que sejam threads diferentes,
/// com o codificador **nunca** perto do caminho do áudio.
#[derive(Debug, Default)]
pub struct Vaga {
    posicao: Mutex<Option<QuadroDaTela>>,
    escritos: AtomicU64,
    descartados: AtomicU64,
    sem_conteudo: AtomicU64,
    ilegiveis: AtomicU64,
}

impl Vaga {
    /// Põe um quadro, substituindo o que estiver lá.
    pub fn por(&self, quadro: QuadroDaTela) {
        // Um `Mutex` envenenado aqui guarda um `Option<QuadroDaTela>`, que é
        // bytes de imagem sem invariante nenhuma: não há estado meio-escrito
        // que possa enganar quem lê depois. Derrubar a captura porque outra
        // thread entrou em pânico seria trocar um defeito por um pior.
        let mut posicao = self.posicao.lock().unwrap_or_else(|e| e.into_inner());
        if posicao.replace(quadro).is_some() {
            self.descartados.fetch_add(1, Ordering::Relaxed);
        }
        self.escritos.fetch_add(1, Ordering::Relaxed);
    }

    /// Tira o quadro que estiver lá, deixando a posição vazia.
    #[must_use]
    pub fn tomar(&self) -> Option<QuadroDaTela> {
        self.posicao
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }

    /// Quantos quadros a ScreenCaptureKit entregou e couberam na posição.
    #[must_use]
    pub fn escritos(&self) -> u64 {
        self.escritos.load(Ordering::Relaxed)
    }

    /// Quantos foram substituídos antes de alguém pegá-los.
    ///
    /// **Não é perda de rede e não é defeito.** É a regra do §1 funcionando:
    /// significa que o codificador está mais devagar que a tela, e um quadro
    /// velho entregue tarde é pior que um quadro perdido.
    #[must_use]
    pub fn descartados(&self) -> u64 {
        self.descartados.load(Ordering::Relaxed)
    }

    /// Quantas amostras vieram sem pixels.
    ///
    /// A ScreenCaptureKit entrega uma amostra por intervalo mesmo quando **nada
    /// mudou na tela**; parte delas vem sem `IOSurface`. Não é erro: é o
    /// sistema dizendo que o quadro anterior continua valendo. Quem transmite
    /// não pode ler isto como «a captura morreu», e quem recebe continua com o
    /// último quadro até o próximo.
    #[must_use]
    pub fn sem_conteudo(&self) -> u64 {
        self.sem_conteudo.load(Ordering::Relaxed)
    }

    /// Quantas amostras chegaram com pixels que não deu para converter.
    ///
    /// Isto **é** defeito, e por isso é contado à parte de [`Vaga::sem_conteudo`]:
    /// misturar os dois esconderia um formato de pixel inesperado dentro de um
    /// número que sobe sozinho numa tela parada.
    #[must_use]
    pub fn ilegiveis(&self) -> u64 {
        self.ilegiveis.load(Ordering::Relaxed)
    }
}

/// O que a ScreenCaptureKit chama a cada amostra.
struct Entregador {
    vaga: Arc<Vaga>,
    largura: usize,
    altura: usize,
}

impl SCStreamOutputTrait for Entregador {
    fn did_output_sample_buffer(
        &self,
        amostra: screencapturekit::cm::CMSampleBuffer,
        tipo: SCStreamOutputType,
    ) {
        if tipo != SCStreamOutputType::Screen {
            return;
        }
        // `Idle`, `Blank` e `Suspended` são a tela dizendo que nada mudou, que
        // está em branco, ou que o sistema suspendeu a captura.
        //
        // **O desconhecido não conta como «sem conteúdo», e isso é medido.** Em
        // `screencapturekit` 8.0.1 sobre macOS 26.5 o acessor do anexo
        // `SCStreamFrameInfo` devolve `None` em **toda** amostra — 87 de 87 numa
        // corrida de três segundos, todas com uma imagem de 1280×720 dentro.
        // A primeira versão deste arquivo tratava `None` como «sem pixels» e
        // descartou 145 quadros seguidos sem uma linha de erro: a captura
        // parecia funcionar e não entregava nada. Quem confere se há pixels é
        // quem procura os pixels, logo abaixo.
        if amostra
            .frame_status()
            .is_some_and(|estado| !estado.has_content())
        {
            self.vaga.sem_conteudo.fetch_add(1, Ordering::Relaxed);
            return;
        }
        match converter(&amostra, self.largura, self.altura) {
            Ok(Some(quadro)) => self.vaga.por(QuadroDaTela {
                quadro,
                capturado_em: Instant::now(),
            }),
            Ok(None) => {
                self.vaga.sem_conteudo.fetch_add(1, Ordering::Relaxed);
            }
            Err(_) => {
                // O motivo não vai para lugar nenhum daqui: esta função roda na
                // fila de despacho da ScreenCaptureKit, onde não há a quem
                // contar. O contador é o que atravessa para a outra thread, e
                // quem lê a captura é quem decide o que fazer com ele.
                self.vaga.ilegiveis.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

/// Uma transmissão viva.
///
/// Uma por sala de voz — o §6 item 3 mantém fora da v1 mais de uma tela ao
/// mesmo tempo na mesma sala.
pub struct CapturaDaTela {
    fluxo: SCStream,
    vaga: Arc<Vaga>,
    largura: usize,
    altura: usize,
}

impl CapturaDaTela {
    /// Começa a capturar, e volta assim que o sistema aceitar.
    ///
    /// `teto` e `cadencia` são **teto**, como manda o §5: a ScreenCaptureKit
    /// entrega **até** essa cadência e nunca mais que ela, e entrega menos
    /// sozinha quando a tela não muda. A resolução segura e o quadro cede.
    ///
    /// O quadro sai com exatamente as dimensões de `teto`, mesmo quando a fonte
    /// tem outra proporção: o conteúdo é encaixado dentro delas, sem esticar, e
    /// o que sobra vira tarja preta. **É uma escolha forçada por quem está do
    /// outro lado**: [`crate::codec::Codificador`] é armado com uma
    /// [`Resolucao`] e recusa qualquer quadro de outro tamanho. A alternativa —
    /// capturar na proporção da fonte, sem tarja — pediria que o codificador
    /// aceitasse uma dimensão calculada, e aí a resolução deixaria de ser uma
    /// escolha de três e viraria um número que a interface não sabe mostrar.
    ///
    /// # Errors
    ///
    /// [`ErroDeCaptura::SemPermissaoDeTela`] quando o TCC não concedeu, e é
    /// conferido aqui de novo mesmo que a casca já tenha conferido: entre
    /// oferecer o botão e alguém clicar nele cabe uma reconstrução do binário,
    /// e é exatamente assim que a concessão some.
    ///
    /// [`ErroDeCaptura::SistemaRecusou`] quando a ScreenCaptureKit não começa.
    pub fn iniciar(
        fonte: &Fonte,
        teto: Resolucao,
        cadencia: Cadencia,
    ) -> Result<Self, ErroDeCaptura> {
        if !permissao().concedida() {
            return Err(ErroDeCaptura::SemPermissaoDeTela);
        }

        let filtro = match fonte {
            Fonte::Monitor { alvo, .. } => SCContentFilter::create()
                .with_display(alvo)
                .with_excluding_windows(&[])
                .build(),
            Fonte::Janela { alvo, .. } => SCContentFilter::create().with_window(alvo).build(),
        };

        let largura = teto.largura();
        let altura = teto.altura();

        let config = SCStreamConfiguration::new()
            .with_width(u32::try_from(largura).unwrap_or(u32::MAX))
            .with_height(u32::try_from(altura).unwrap_or(u32::MAX))
            // Ver o cabeçalho do módulo: `420v` já vem do compositor e é
            // intervalo de TV, o mesmo de `QuadroI420::preto`. BGRA custaria
            // uma matriz de cor por pixel para chegar no mesmo lugar.
            .with_pixel_format(PixelFormat::YCbCr_420v)
            // O teto de quadros, e o sistema fica livre para entregar menos.
            .with_minimum_frame_interval(&CMTime::new(
                1,
                i32::try_from(cadencia.hz()).unwrap_or(30),
            ))
            // A profundidade **da ScreenCaptureKit**, que não é a nossa: a
            // nossa é a [`Vaga`], que tem uma posição. Três é o mínimo que a
            // Apple documenta; pedir mais seria pedir ao sistema a fila que o
            // §1 acabou de recusar.
            .with_queue_depth(3)
            // O cursor entra porque quem mostra a tela está apontando para
            // coisas nela. O §6 item 12 deixa fora **desenhar** o cursor à
            // parte, com anotação e ponteiro; este é o que o compositor já
            // compôs, e ele é de graça.
            .with_shows_cursor(true)
            // §6 item 1: áudio da tela fica fora da v1 — permissão separada nos
            // três sistemas, mistura com o microfone, e eco. Dito por extenso
            // porque o padrão da biblioteca pode mudar e um `false` calado
            // ficaria a mercê disso.
            .with_captures_audio(false)
            // O conteúdo é encaixado, não esticado, e o que sobra é preto — a
            // mesma cor do quadro preto do codec, para que a tarja não custe
            // bits nem chame atenção.
            .with_scales_to_fit(true)
            .with_preserves_aspect_ratio(true)
            .with_background_color(0.0, 0.0, 0.0);

        let vaga = Arc::new(Vaga::default());
        let mut fluxo = SCStream::new(&filtro, &config);
        fluxo.add_output_handler(
            Entregador {
                vaga: Arc::clone(&vaga),
                largura,
                altura,
            },
            SCStreamOutputType::Screen,
        );
        fluxo
            .start_capture()
            .map_err(|erro| ErroDeCaptura::SistemaRecusou {
                operacao: "começar a capturar",
                detalhe: erro.to_string(),
            })?;

        Ok(Self {
            fluxo,
            vaga,
            largura,
            altura,
        })
    }

    /// A posição onde os quadros aparecem.
    ///
    /// Devolvida como [`Arc`] de propósito: quem codifica mora noutra thread
    /// (§2), e é ela que lê. Esta é a única fronteira entre as duas.
    #[must_use]
    pub fn vaga(&self) -> Arc<Vaga> {
        Arc::clone(&self.vaga)
    }

    /// Tira o quadro mais recente, se houver um.
    #[must_use]
    pub fn tomar(&self) -> Option<QuadroDaTela> {
        self.vaga.tomar()
    }

    /// O tamanho que os quadros têm, em pixels.
    #[must_use]
    pub const fn tamanho(&self) -> (usize, usize) {
        (self.largura, self.altura)
    }

    /// Para de capturar.
    ///
    /// # Errors
    ///
    /// [`ErroDeCaptura::SistemaRecusou`] se a ScreenCaptureKit reclamar. Parar
    /// duas vezes é um desses casos, e é por isso que isto consome `self`.
    pub fn parar(self) -> Result<(), ErroDeCaptura> {
        self.fluxo
            .stop_capture()
            .map_err(|erro| ErroDeCaptura::SistemaRecusou {
                operacao: "parar a captura",
                detalhe: erro.to_string(),
            })
    }
}

impl Drop for CapturaDaTela {
    fn drop(&mut self) {
        // Sem isto, largar a captura sem chamar `parar` deixa o indicador de
        // gravação de tela aceso — o sistema continua achando que estamos
        // transmitindo. É o defeito que a pessoa vê e não consegue explicar.
        // O erro morre aqui porque `drop` não tem a quem contar, e porque o
        // caminho que interessa é o de quem chamou `parar` e leu o resultado.
        let _ = self.fluxo.stop_capture();
    }
}

impl std::fmt::Debug for CapturaDaTela {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapturaDaTela")
            .field("largura", &self.largura)
            .field("altura", &self.altura)
            .field("escritos", &self.vaga.escritos())
            .field("descartados", &self.vaga.descartados())
            .finish()
    }
}

/// Tira os pixels da amostra e monta o I420.
///
/// `Ok(None)` é a amostra que chegou **sem imagem**, que é como a tela parada
/// se apresenta quando o estado do quadro não vem junto. Não é erro, e por isso
/// não é `Err`: distingui-los é o que mantém [`Vaga::ilegiveis`] significando
/// defeito de verdade.
fn converter(
    amostra: &screencapturekit::cm::CMSampleBuffer,
    largura: usize,
    altura: usize,
) -> Result<Option<QuadroI420>, ErroDeCaptura> {
    let Some(buffer) = amostra.image_buffer() else {
        return Ok(None);
    };
    let travado = buffer
        .lock_read_only()
        .map_err(|_| ErroDeCaptura::QuadroIlegivel {
            motivo: "não deu para travar o buffer de pixels",
        })?;

    // Dois planos: luma, e croma entrelaçado. Um plano só significa que o
    // sistema entregou um formato empacotado — BGRA, por exemplo — e aí a
    // conversão daqui leria lixo em vez de falhar.
    if travado.plane_count() != 2 {
        return Err(ErroDeCaptura::QuadroIlegivel {
            motivo: "o formato de pixel não é bi-planar 4:2:0",
        });
    }
    // A ScreenCaptureKit pode entregar um buffer maior que o pedido — ela
    // alinha. Menor, não pode: seria pedir uma largura que não veio.
    if travado.width_of_plane(0) < largura || travado.height_of_plane(0) < altura {
        return Err(ErroDeCaptura::QuadroIlegivel {
            motivo: "o quadro veio menor que a resolução pedida",
        });
    }

    let luma: Vec<&[u8]> = (0..altura)
        .filter_map(|l| travado.plane_row(0, l))
        .collect();
    let linhas_croma = altura.div_ceil(2);
    let croma: Vec<&[u8]> = (0..linhas_croma)
        .filter_map(|l| travado.plane_row(1, l))
        .collect();

    montar_i420(largura, altura, &luma, &croma).map(Some)
}

/// Copia as linhas úteis e desentrelaça o croma.
///
/// Separada da ScreenCaptureKit de propósito: é a única parte disto que dá para
/// testar sem uma tela, sem TCC e sem um Mac na frente de alguém, e é a parte
/// onde um erro produz imagem verde em vez de falha.
///
/// As linhas chegam **com o preenchimento do sistema**: `bytes_per_row` é maior
/// que a largura em pixels, porque o compositor alinha. Copiar a linha inteira
/// entortaria a imagem de um jeito característico — cada linha deslocada um
/// tanto para a esquerda da anterior.
fn montar_i420(
    largura: usize,
    altura: usize,
    luma: &[&[u8]],
    croma: &[&[u8]],
) -> Result<QuadroI420, ErroDeCaptura> {
    let colunas_croma = largura.div_ceil(2);
    let linhas_croma = altura.div_ceil(2);

    let mut y = Vec::with_capacity(largura * altura);
    for linha in luma.iter().take(altura) {
        let util = linha.get(..largura).ok_or(ErroDeCaptura::QuadroIlegivel {
            motivo: "uma linha de luma veio mais curta que a largura",
        })?;
        y.extend_from_slice(util);
    }

    let mut u = Vec::with_capacity(colunas_croma * linhas_croma);
    let mut v = Vec::with_capacity(colunas_croma * linhas_croma);
    for linha in croma.iter().take(linhas_croma) {
        let util = linha
            .get(..colunas_croma * 2)
            .ok_or(ErroDeCaptura::QuadroIlegivel {
                motivo: "uma linha de croma veio mais curta que a largura",
            })?;
        for par in util.chunks_exact(2) {
            if let [cb, cr] = par {
                u.push(*cb);
                v.push(*cr);
            }
        }
    }

    // Quem confere se os três planos fecham é o codec, e é ele quem tem de
    // conferir: um plano curto entregue ao C é leitura fora de área, e a
    // conferência num lugar só é a que não pode divergir da outra.
    QuadroI420::novo(largura, altura, y, u, v).map_err(ErroDeCaptura::PlanosRecusados)
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Uma linha de luma com preenchimento: `largura` bytes úteis e mais lixo
    /// depois, que é exatamente o que o sistema entrega.
    fn linha(util: u8, largura: usize, preenchimento: usize) -> Vec<u8> {
        let mut l = vec![util; largura];
        l.extend(std::iter::repeat_n(0xFF, preenchimento));
        l
    }

    #[test]
    fn o_croma_entrelacado_vira_dois_planos() {
        // Cb e Cr diferentes de propósito: se a desentrelaçagem trocar os dois,
        // ou copiar o mesmo para os dois, este teste vê.
        let luma: Vec<Vec<u8>> = (0..4).map(|_| linha(0x40, 4, 60)).collect();
        let croma: Vec<Vec<u8>> = (0..2)
            .map(|_| {
                let mut l = vec![];
                for _ in 0..2 {
                    l.push(0x10);
                    l.push(0xE0);
                }
                l.extend(std::iter::repeat_n(0xFF, 60));
                l
            })
            .collect();

        let refs_luma: Vec<&[u8]> = luma.iter().map(Vec::as_slice).collect();
        let refs_croma: Vec<&[u8]> = croma.iter().map(Vec::as_slice).collect();
        let quadro = montar_i420(4, 4, &refs_luma, &refs_croma).expect("os planos fecham");

        assert_eq!(quadro.largura(), 4);
        assert_eq!(quadro.altura(), 4);
        assert_eq!(quadro.luma(), &[0x40; 16]);
        assert_eq!(quadro.croma_u(), &[0x10; 4]);
        assert_eq!(quadro.croma_v(), &[0xE0; 4]);
    }

    #[test]
    fn o_preenchimento_da_linha_nao_entra_no_quadro() {
        // O defeito que este teste guarda: copiar `bytes_per_row` inteiro em vez
        // de `largura`. A imagem sai entortando para a esquerda, linha a linha,
        // e ninguém adivinha a causa olhando.
        let luma: Vec<Vec<u8>> = (0..2).map(|_| linha(0x40, 2, 30)).collect();
        let croma = [{
            let mut l = vec![0x10, 0xE0];
            l.extend(std::iter::repeat_n(0xFF, 30));
            l
        }];

        let refs_luma: Vec<&[u8]> = luma.iter().map(Vec::as_slice).collect();
        let refs_croma: Vec<&[u8]> = croma.iter().map(Vec::as_slice).collect();
        let quadro = montar_i420(2, 2, &refs_luma, &refs_croma).expect("os planos fecham");

        assert!(
            !quadro.luma().contains(&0xFF),
            "o preenchimento da linha vazou para dentro do quadro"
        );
    }

    #[test]
    fn uma_linha_curta_e_recusada_com_nome() {
        let luma = [vec![0x40_u8; 3]];
        let refs_luma: Vec<&[u8]> = luma.iter().map(Vec::as_slice).collect();
        let erro = montar_i420(4, 1, &refs_luma, &[]).expect_err("a linha é mais curta que 4");
        assert!(matches!(erro, ErroDeCaptura::QuadroIlegivel { .. }));
    }

    #[test]
    fn faltando_linha_o_codec_e_quem_recusa() {
        // Só duas das quatro linhas de luma. A conferência é a do codec, e o
        // motivo tem de chegar inteiro em vez de virar um quadro meio preto.
        let luma: Vec<Vec<u8>> = (0..2).map(|_| linha(0x40, 4, 4)).collect();
        let refs_luma: Vec<&[u8]> = luma.iter().map(Vec::as_slice).collect();
        let erro = montar_i420(4, 4, &refs_luma, &[]).expect_err("faltam linhas");
        assert!(matches!(
            erro,
            ErroDeCaptura::PlanosRecusados(ErroDeVideo::PlanosInconsistentes { .. })
        ));
    }

    #[test]
    fn a_vaga_substitui_e_conta() {
        let vaga = Vaga::default();
        let quadro = |v: u8| QuadroDaTela {
            quadro: QuadroI420::novo(2, 2, vec![v; 4], vec![128; 1], vec![128; 1])
                .expect("2x2 fecha"),
            capturado_em: Instant::now(),
        };

        vaga.por(quadro(1));
        vaga.por(quadro(2));
        vaga.por(quadro(3));

        assert_eq!(vaga.descartados(), 2, "dois quadros foram substituídos");
        assert_eq!(vaga.escritos(), 3);
        let tirado = vaga.tomar().expect("há um quadro");
        assert_eq!(
            tirado.quadro.luma(),
            &[3; 4],
            "quem fica é o mais novo, nunca o mais velho"
        );
        assert!(vaga.tomar().is_none(), "a posição fica vazia depois");
    }

    /// A prova de campo: uma tela de verdade, com TCC de verdade.
    ///
    /// **Pula em voz alta** onde não há nenhum dos dois — uma sessão SSH, um
    /// agente de CI — porque um teste que passa por não ter rodado é pior que
    /// um teste vermelho.
    #[test]
    fn captura_um_quadro_da_tela_de_verdade() {
        if !permissao().concedida() {
            eprintln!(
                "PULADO: o TCC não concedeu gravação de tela a este processo. \
                 Num app não assinado a concessão morre a cada build (§4); em \
                 terminal, quem a tem é o terminal."
            );
            return;
        }
        let lista = match fontes() {
            Ok(l) => l,
            Err(ErroDeCaptura::NadaParaCapturar) => {
                eprintln!("PULADO: esta máquina não tem monitor nem janela.");
                return;
            }
            Err(erro) => panic!("listar as fontes falhou: {erro}"),
        };
        let monitor = match lista.iter().find(|f| matches!(f, Fonte::Monitor { .. })) {
            Some(m) => m,
            None => {
                eprintln!("PULADO: nenhum monitor na lista.");
                return;
            }
        };

        let captura = CapturaDaTela::iniciar(monitor, Resolucao::P720, Cadencia::Q30)
            .expect("a captura começa");
        let comeco = Instant::now();
        let mut pego = None;
        while comeco.elapsed().as_secs_f64() < 3.0 {
            if let Some(q) = captura.tomar() {
                pego = Some(q);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let vaga = captura.vaga();
        let quadro = pego.expect("um quadro chega em três segundos");

        assert_eq!(
            (quadro.quadro.largura(), quadro.quadro.altura()),
            (1280, 720),
            "o quadro sai com a resolução pedida"
        );
        assert_eq!(vaga.ilegiveis(), 0, "nenhum quadro chegou ilegível");
        assert!(
            quadro.quadro.luma().iter().any(|&p| p > 16),
            "o quadro veio todo preto — ou a tela está apagada, ou a conversão perdeu os pixels"
        );
        captura.parar().expect("a captura para");
    }
}
