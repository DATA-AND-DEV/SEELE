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
//! **A borda amarela fica.** O `windows-capture` oferece
//! [`DrawBorderSettings::WithoutBorder`], que chama `SetIsBorderRequired(false)`
//! — e isso exige a capacidade `graphicsCaptureWithoutBorder` num manifesto de
//! pacote, que um instalador NSIS não tem. Este arquivo passa
//! [`DrawBorderSettings::Default`] **de propósito**: pedir o que não se pode
//! ter trocaria uma borda por um erro na hora de começar a transmitir, que é
//! justamente o defeito que o §2 proíbe («o botão de compartilhar não pode
//! falhar»).
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

use crate::codec::{bytes_de_croma, bytes_de_luma, Cadencia, QuadroI420, Resolucao};
use crate::erro::ErroDeVideo;

/// O preto do intervalo de TV, o mesmo que [`QuadroI420::preto`] usa.
///
/// É com ele que as tarjas se preenchem quando a fonte não tem a proporção do
/// destino. Zerar daria verde, que é o defeito clássico de quem trata I420 como
/// se fosse RGB.
const PRETO_LUMA: u8 = 16;

/// O centro do croma, que é o cinza sem cor.
const CENTRO_CROMA: u8 = 128;

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

/// De onde cada pixel de destino vem, calculado uma vez por tamanho de origem.
///
/// A conta é refeita quando a fonte muda de tamanho — uma janela redimensionada
/// no meio da transmissão —, e só então: por quadro ela seria uma divisão por
/// pixel de destino, que é dois milhões de divisões em 1080p.
#[derive(Debug, PartialEq, Eq)]
struct Mapa {
    origem: (usize, usize),
    destino: Resolucao,
    /// A área útil dentro do destino, em pixels. Sempre par nos dois lados,
    /// porque o croma de um I420 anda de dois em dois.
    ativa: (usize, usize),
    /// O canto de cima e à esquerda da área útil. Também par, pelo mesmo motivo.
    canto: (usize, usize),
    /// Para cada coluna útil, a faixa de colunas de origem que a alimenta.
    colunas: Vec<(usize, usize)>,
    /// Para cada linha útil, a faixa de linhas de origem que a alimenta.
    linhas: Vec<(usize, usize)>,
}

/// Arredonda para baixo até um número par, com teto e piso.
const fn par_ate(valor: usize, teto: usize) -> usize {
    let cortado = if valor > teto { teto } else { valor };
    let par = cortado & !1;
    if par < 2 {
        2
    } else {
        par
    }
}

/// Para cada um dos `destino` pixels, a faixa de pixels de origem que ele cobre.
fn faixas(origem: usize, destino: usize) -> Vec<(usize, usize)> {
    (0..destino)
        .map(|i| {
            let inicio = i * origem / destino;
            let fim = ((i + 1) * origem / destino).max(inicio + 1).min(origem);
            (inicio, fim)
        })
        .collect()
}

impl Mapa {
    /// Monta o mapa de uma origem para uma resolução de destino.
    ///
    /// **A proporção é preservada, e o que sobra vira tarja preta.** Esticar a
    /// imagem seria a única alternativa sem tarja, e ela deforma texto — que é
    /// exatamente o conteúdo que o §2 diz que se está transmitindo. Uma sessão
    /// SSH do Windows enxerga 1024×768, ou seja 4:3 dentro de um destino 16:9:
    /// a tarja não é caso raro, é o caso da máquina onde isto foi medido.
    fn novo(origem: (usize, usize), destino: Resolucao) -> Self {
        let (largura_origem, altura_origem) = origem;
        let largura_destino = destino.largura();
        let altura_destino = destino.altura();

        let (ativa_largura, ativa_altura) = if largura_origem == 0 || altura_origem == 0 {
            (largura_destino, altura_destino)
        } else if largura_origem * altura_destino <= largura_destino * altura_origem {
            // A altura é quem limita: a origem é mais «alta» que o destino.
            (
                par_ate(
                    largura_origem * altura_destino / altura_origem,
                    largura_destino,
                ),
                altura_destino,
            )
        } else {
            (
                largura_destino,
                par_ate(
                    altura_origem * largura_destino / largura_origem,
                    altura_destino,
                ),
            )
        };

        let canto = (
            ((largura_destino - ativa_largura) / 2) & !1,
            ((altura_destino - ativa_altura) / 2) & !1,
        );

        Self {
            origem,
            destino,
            ativa: (ativa_largura, ativa_altura),
            canto,
            colunas: faixas(largura_origem.max(1), ativa_largura),
            linhas: faixas(altura_origem.max(1), ativa_altura),
        }
    }

    /// Converte um quadro BGRA de `passo` bytes por linha num [`QuadroI420`].
    ///
    /// **O `passo` não é `largura × 4`**, e tratá-lo como se fosse é o defeito
    /// clássico deste caminho: a textura que o D3D11 mapeia tem o `row_pitch`
    /// que o driver quiser, e ler linha após linha sem ele entorta a imagem em
    /// diagonal. É o que o teste `o_passo_maior_que_a_largura_nao_vaza` prende.
    fn converter(&self, bytes: &[u8], passo: usize) -> Result<QuadroI420, ErroDeVideo> {
        let largura = self.destino.largura();
        let altura = self.destino.altura();
        let largura_croma = largura.div_ceil(2);
        let (ativa_largura, ativa_altura) = self.ativa;
        let (canto_x, canto_y) = self.canto;

        let mut luma = vec![PRETO_LUMA; bytes_de_luma(largura, altura)];
        let mut croma_u = vec![CENTRO_CROMA; bytes_de_croma(largura, altura)];
        let mut croma_v = vec![CENTRO_CROMA; bytes_de_croma(largura, altura)];

        let pares_de_linha = luma
            .chunks_exact_mut(largura * 2)
            .skip(canto_y / 2)
            .take(ativa_altura / 2);
        let linhas_u = croma_u
            .chunks_exact_mut(largura_croma)
            .skip(canto_y / 2)
            .take(ativa_altura / 2);
        let linhas_v = croma_v
            .chunks_exact_mut(largura_croma)
            .skip(canto_y / 2)
            .take(ativa_altura / 2);

        for (bloco_y, ((par_de_linhas, linha_u), linha_v)) in
            pares_de_linha.zip(linhas_u).zip(linhas_v).enumerate()
        {
            let (
                Some(&(origem_y0_cima, origem_y1_cima)),
                Some(&(origem_y0_baixo, origem_y1_baixo)),
            ) = (
                self.linhas.get(bloco_y * 2),
                self.linhas.get(bloco_y * 2 + 1),
            )
            else {
                continue;
            };

            let (linha_cima, linha_baixo) = par_de_linhas.split_at_mut(largura);
            let (Some(faixa_cima), Some(faixa_baixo)) = (
                linha_cima.get_mut(canto_x..canto_x + ativa_largura),
                linha_baixo.get_mut(canto_x..canto_x + ativa_largura),
            ) else {
                continue;
            };
            let (Some(faixa_u), Some(faixa_v)) = (
                linha_u.get_mut(canto_x / 2..canto_x / 2 + ativa_largura / 2),
                linha_v.get_mut(canto_x / 2..canto_x / 2 + ativa_largura / 2),
            ) else {
                continue;
            };

            for (bloco_x, (((alto, baixo), destino_u), destino_v)) in faixa_cima
                .chunks_exact_mut(2)
                .zip(faixa_baixo.chunks_exact_mut(2))
                .zip(faixa_u.iter_mut())
                .zip(faixa_v.iter_mut())
                .enumerate()
            {
                let (Some(&(origem_x0_esq, origem_x1_esq)), Some(&(origem_x0_dir, origem_x1_dir))) = (
                    self.colunas.get(bloco_x * 2),
                    self.colunas.get(bloco_x * 2 + 1),
                ) else {
                    continue;
                };

                // As quatro cores médias do bloco 2×2. A média é sobre a área de
                // origem inteira que cada pixel de destino cobre — e não uma
                // amostra do vizinho mais próximo —, porque texto reduzido por
                // vizinho mais próximo perde linhas de pixel inteiras: some a
                // barra do «t» e o «e» vira «c». Custa ler cada pixel de origem
                // uma vez, que é o mesmo que qualquer conversão faria.
                let cor_ce = media(
                    bytes,
                    passo,
                    origem_x0_esq,
                    origem_x1_esq,
                    origem_y0_cima,
                    origem_y1_cima,
                );
                let cor_cd = media(
                    bytes,
                    passo,
                    origem_x0_dir,
                    origem_x1_dir,
                    origem_y0_cima,
                    origem_y1_cima,
                );
                let cor_be = media(
                    bytes,
                    passo,
                    origem_x0_esq,
                    origem_x1_esq,
                    origem_y0_baixo,
                    origem_y1_baixo,
                );
                let cor_bd = media(
                    bytes,
                    passo,
                    origem_x0_dir,
                    origem_x1_dir,
                    origem_y0_baixo,
                    origem_y1_baixo,
                );

                if let [esquerda, direita] = alto {
                    *esquerda = luma_de(cor_ce);
                    *direita = luma_de(cor_cd);
                }
                if let [esquerda, direita] = baixo {
                    *esquerda = luma_de(cor_be);
                    *direita = luma_de(cor_bd);
                }

                // O croma do I420 é um por bloco de 2×2, então ele sai da cor
                // média dos quatro — e não de um dos quatro. Escolher um canto
                // deslocaria a cor meio pixel para aquele lado em toda a imagem.
                let media_do_bloco = (
                    (cor_ce.0 + cor_cd.0 + cor_be.0 + cor_bd.0) / 4,
                    (cor_ce.1 + cor_cd.1 + cor_be.1 + cor_bd.1) / 4,
                    (cor_ce.2 + cor_cd.2 + cor_be.2 + cor_bd.2) / 4,
                );
                let (u, v) = croma_de(media_do_bloco);
                *destino_u = u;
                *destino_v = v;
            }
        }

        QuadroI420::novo(largura, altura, luma, croma_u, croma_v)
    }
}

/// A cor média de um retângulo da origem, em (R, G, B).
///
/// A origem é BGRA — que é a ordem que a WGC entrega e que
/// [`ColorFormat::Bgra8`] pede. Trocar B por R aqui é o defeito mais fácil de
/// cometer e o mais difícil de ver num teste de tela cinza; o teste
/// `o_vermelho_nao_vira_azul` existe só para isso.
///
/// # O que esta função custa, medido
///
/// **A conversão é o maior gasto de CPU do compartilhamento de tela, e não o
/// codificador.** Num Ryzen 7 5800X3D, capturando um monitor de 2560×1440:
/// 9,0 ms por quadro para 720p e 16,5 ms para 1080p, ou seja **0,27 e 0,50 de
/// um núcleo a 30 quadros por segundo**. `spikes/tela-no-codec` mediu o
/// OpenH264 na mesma máquina em **0,105** de núcleo a 1080p30: converter custa
/// cinco vezes codificar, e o spike disse, na cara, que este número faltava.
///
/// **O gasto anda com o número de pixels de destino, não de origem** — a mesma
/// tela de 2560×1440 custa 1,8× mais para 1080p que para 720p —, e duas
/// tentativas de espremê-lo não mudaram nada de medido: tirar a conferência de
/// limite por byte (`first_chunk`) e trocar três divisões inteiras por um
/// recíproco deram 16,5 e 16,9 ms contra os mesmos 16,6 de antes. O que sobra é
/// custo de laço por pixel de destino, e baixá-lo é **mudança de desenho** —
/// converter na GPU, ou converter só as regiões sujas que a WGC já sabe
/// apontar —, não ajuste. Está no relatório para quem coordena decidir.
fn media(
    bytes: &[u8],
    passo: usize,
    x0: usize,
    x1: usize,
    y0: usize,
    y1: usize,
) -> (u32, u32, u32) {
    let mut soma = (0u32, 0u32, 0u32);
    let mut quantos = 0u32;

    for y in y0..y1 {
        let Some(faixa) = bytes.get(y * passo + x0 * 4..y * passo + x1 * 4) else {
            continue;
        };
        for pixel in faixa.chunks_exact(4) {
            if let [azul, verde, vermelho, _alfa] = pixel {
                soma.0 += u32::from(*vermelho);
                soma.1 += u32::from(*verde);
                soma.2 += u32::from(*azul);
                quantos += 1;
            }
        }
    }

    if quantos == 0 {
        return (0, 0, 0);
    }
    (soma.0 / quantos, soma.1 / quantos, soma.2 / quantos)
}

/// Luma de uma cor, em BT.709 de faixa de TV.
///
/// **709 e não 601**, e o motivo é o conteúdo: as três resoluções que o §5
/// oferece são 540p, 720p e 1080p, e um decodificador que precisa adivinhar
/// adivinha 709 para as duas maiores. O codec não escreve VUI nenhum hoje —
/// está no relatório, porque quem conserta isso é `codec.rs`, e enquanto não
/// escrever, quem recebe adivinha.
///
/// Faixa de TV (16..235 para luma, 16..240 para croma) e não faixa cheia,
/// porque é o que [`QuadroI420::preto`] já assume ao pintar 16 e 128.
fn luma_de((vermelho, verde, azul): (u32, u32, u32)) -> u8 {
    let soma = 47 * vermelho as i32 + 157 * verde as i32 + 16 * azul as i32;
    (16 + soma.div_euclid(256)).clamp(16, 235) as u8
}

/// Croma de uma cor, em BT.709 de faixa de TV, na ordem (U, V).
fn croma_de((vermelho, verde, azul): (u32, u32, u32)) -> (u8, u8) {
    let (vermelho, verde, azul) = (vermelho as i32, verde as i32, azul as i32);
    let u = 128 + (-25 * vermelho - 87 * verde + 112 * azul).div_euclid(256);
    let v = 128 + (112 * vermelho - 102 * verde - 10 * azul).div_euclid(256);
    (u.clamp(16, 240) as u8, v.clamp(16, 240) as u8)
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
fn ligar<T>(
    item: T,
    caixa: Arc<Caixa>,
    destino: Resolucao,
    intervalo: Duration,
    do_sistema: MinimumUpdateIntervalSettings,
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
        // A borda amarela fica (§4). Pedir `WithoutBorder` chamaria
        // `SetIsBorderRequired(false)`, que exige a capacidade
        // `graphicsCaptureWithoutBorder` num manifesto de pacote que um
        // instalador NSIS não tem.
        DrawBorderSettings::Default,
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
    /// pintado de branco para que quem o ler por engano seja pego.
    fn origem(largura: usize, altura: usize, passo: usize, cor: (u8, u8, u8)) -> Vec<u8> {
        let mut bytes = vec![0xFF; passo * altura];
        for linha in bytes.chunks_exact_mut(passo) {
            if let Some(util) = linha.get_mut(..largura * 4) {
                for pixel in util.chunks_exact_mut(4) {
                    if let [azul, verde, vermelho, alfa] = pixel {
                        *azul = cor.0;
                        *verde = cor.1;
                        *vermelho = cor.2;
                        *alfa = 0xFF;
                    }
                }
            }
        }
        bytes
    }

    fn luma_em(quadro: &QuadroI420, x: usize, y: usize) -> u8 {
        quadro
            .luma()
            .get(y * quadro.largura() + x)
            .copied()
            .unwrap_or(0)
    }

    fn croma_em(quadro: &QuadroI420, x: usize, y: usize) -> (u8, u8) {
        let indice = y * quadro.largura().div_ceil(2) + x;
        (
            quadro.croma_u().get(indice).copied().unwrap_or(0),
            quadro.croma_v().get(indice).copied().unwrap_or(0),
        )
    }

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

    #[test]
    fn a_proporcao_e_preservada_e_o_resto_vira_tarja() {
        // 1024×768 é o que uma sessão SSH do Windows enxerga, e 4:3 num destino
        // 16:9 é o caso desta casa, não um caso de borda.
        let mapa = Mapa::novo((1024, 768), Resolucao::P720);
        assert_eq!(mapa.ativa, (960, 720));
        assert_eq!(mapa.canto, (160, 0));

        let quadro = mapa
            .converter(&origem(1024, 768, 1024 * 4, (0xFF, 0xFF, 0xFF)), 1024 * 4)
            .expect("os planos fecham");

        // Dentro da área útil, branco; nas tarjas, o preto de TV.
        assert_eq!(luma_em(&quadro, 600, 360), 235);
        assert_eq!(luma_em(&quadro, 0, 360), PRETO_LUMA);
        assert_eq!(luma_em(&quadro, 1279, 360), PRETO_LUMA);
        assert_eq!(croma_em(&quadro, 0, 180), (CENTRO_CROMA, CENTRO_CROMA));
    }

    #[test]
    fn o_vermelho_nao_vira_azul() {
        // A origem é BGRA. Trocar os canais dá uma imagem que parece certa em
        // cinza e erra em tudo que tem cor, e é o defeito mais fácil de deixar
        // passar. Em BT.709 de faixa de TV, vermelho puro dá U ≈ 103 e V ≈ 239;
        // se azul e vermelho trocassem de lugar, viria U ≈ 240 e V ≈ 118, que é
        // o que estes dois limites separam.
        let mapa = Mapa::novo((16, 16), Resolucao::P540);
        let quadro = mapa
            .converter(&origem(16, 16, 16 * 4, (0x00, 0x00, 0xFF)), 16 * 4)
            .expect("os planos fecham");

        let (u, v) = croma_em(&quadro, 240, 135);
        assert!((100..=106).contains(&u), "U de vermelho puro veio {u}");
        assert!((235..=240).contains(&v), "V de vermelho puro veio {v}");
        assert_eq!(luma_em(&quadro, 480, 270), 62);
    }

    #[test]
    fn o_passo_maior_que_a_largura_nao_vaza() {
        // A textura que o D3D11 mapeia tem o `row_pitch` que o driver quiser, e
        // ler `largura × 4` por linha entorta a imagem em diagonal. Aqui o
        // preenchimento é branco e a imagem é preta: qualquer luma acima do
        // preto de TV é preenchimento lido por engano.
        let largura = 8;
        let altura = 8;
        let passo = largura * 4 + 96;
        let bytes = origem(largura, altura, passo, (0x00, 0x00, 0x00));

        let quadro = Mapa::novo((largura, altura), Resolucao::P540)
            .converter(&bytes, passo)
            .expect("os planos fecham");

        let maior = quadro.luma().iter().copied().max().unwrap_or(0);
        assert_eq!(maior, PRETO_LUMA, "o preenchimento de fim de linha vazou");
    }

    #[test]
    fn a_media_le_a_area_inteira_e_nao_um_vizinho() {
        // Metade branca e metade preta reduzidas a um pixel dão cinza. Com
        // vizinho mais próximo dariam uma das duas pontas — que é como texto
        // reduzido perde linhas de pixel inteiras.
        let mut bytes = origem(2, 1, 8, (0x00, 0x00, 0x00));
        if let Some([azul, verde, vermelho, _alfa]) = bytes.get_mut(4..8) {
            *azul = 0xFF;
            *verde = 0xFF;
            *vermelho = 0xFF;
        }

        let cor = media(&bytes, 8, 0, 2, 0, 1);
        assert_eq!(cor, (127, 127, 127));
    }
}
