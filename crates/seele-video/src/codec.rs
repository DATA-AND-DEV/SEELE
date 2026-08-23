//! H.264 baseline pelo OpenH264, configurado como o §2 e o §5 decidiram, e
//! nada além disso.
//!
//! # As escolhas, e por que cada uma
//!
//! | | de onde vem | por quê |
//! |---|---|---|
//! | CAVLC, ou seja perfil baseline | §2 | é o único perfil que o OpenH264 codifica, e é o que o navegador e o telefone falam sem tradução |
//! | modo de taxa por **bitrate** | §3.2 | o vídeo tem teto; o encoder que interessa é o que respeita um teto |
//! | **uma fatia, uma thread** | `spikes/tela-no-codec` | quatro fatias dão 2,4× de quadros por 2,5× de CPU e sobem o descarte de 16,2% para 23,9% |
//! | quadro-chave **sob demanda** | §3.3 | numa conversa entre dois pares não há quem entre no meio da transmissão |
//! | resolução escolhida, quadro cede | §2 | texto continua legível a 8 quadros e vira borrão se a resolução baixar |
//!
//! # O que o encoder faz sozinho, e o que ele não faz
//!
//! [`Codificador::codificar`] é síncrono: entra um quadro, sai um quadro, e ele
//! volta quando terminou. **Ele não enfileira e não descarta por falta de
//! tempo** — não tem fila, não tem thread interna e não sabe que existe um
//! relógio. Quem decide o que fazer com um quadro que chegou enquanto o
//! anterior era codificado é quem captura, e a regra do §1 é descartar o velho.
//!
//! A única coisa que ele faz por conta própria é pular quadro por **falta de
//! bits**, e isso aparece como `Ok(None)`. Não é erro e não pode ser tratado
//! como perda: no teto de 1200 kbps que a voz permite são 16% dos quadros em
//! 1080p e 11% em 720p, medidos. É exatamente o caso para o qual o §5 obriga a
//! interface a mostrar o que está saindo ao lado do que foi pedido.

use std::num::NonZeroUsize;

use shiguredo_openh264::{
    Decoder, EncodeOptions, EncodedFrame, Encoder, EncoderConfig, EntropyCodingMode, FrameType,
    RateControlMode, SliceMode,
};

use crate::erro::ErroDeVideo;
use crate::modulo::BibliotecaDeVideo;

/// O piso da faixa automática de quadros por segundo (§2).
///
/// **Não é uma opção de interface.** O §5 é explícito: o menor que se oferece a
/// escolher é 8, porque «texto continua legível a 8 quadros»; escolher 5 seria
/// escolher desistir, e desistir é o que o sistema faz sozinho, com motivo
/// enumerado, quando nem o piso se sustenta.
pub const PISO_DE_QUADROS: u32 = 5;

/// O único teto de banda que alguém mediu, em bits por segundo.
///
/// **Não é configuração**, e pôr este número num arquivo de ajustes seria trair
/// o §3.2: o teto do vídeo é uma **fração do caminho medido** — 60% —, pendurada
/// no sinal que a voz já calcula (ADR 0024), e não um valor fixo. Ele está aqui
/// porque é o teto sob o qual as duas provas rodaram, e porque um padrão
/// precisa existir antes de haver medida do caminho.
///
/// A lista de tetos que a interface vai oferecer continua **em aberto**: o §5
/// fecha as resoluções e os quadros e diz, com todas as letras, que fixar as
/// bandas exige medir quanto o caminho aguenta — a pergunta 2 do §8.
pub const TETO_DA_PROVA_BPS: u32 = 1_200_000;

/// As três resoluções que o §5 fechou, depois de `spikes/tela-no-codec` medir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Resolucao {
    /// 960×540. O piso da lista.
    ///
    /// Abaixo dele o encoder deixa de conseguir gastar o orçamento — 360p rende
    /// 416 kbps dos 1200 disponíveis —, e aí baixar a resolução torra nitidez
    /// sem devolver nada. Quem quer gastar menos internet mexe no teto de banda,
    /// que é o controle desenhado para isso.
    P540,
    /// 1280×720. O padrão.
    #[default]
    P720,
    /// 1920×1080.
    ///
    /// Entra por medida: custa 0,073 de um núcleo no M5 Pro e 0,105 no Ryzen a
    /// 30 quadros. E entra com o que a mesma medida diz — no teto de 1200 kbps o
    /// próprio controle de taxa descarta 16% dos quadros.
    P1080,
}

impl Resolucao {
    /// As três, da menor para a maior. **Não há uma quarta**: o §6 item 10
    /// mantém fora tudo acima de 1080p, e a medida não mudou isso.
    pub const TODAS: [Self; 3] = [Self::P540, Self::P720, Self::P1080];

    /// Largura em pixels.
    #[must_use]
    pub const fn largura(self) -> usize {
        match self {
            Self::P540 => 960,
            Self::P720 => 1280,
            Self::P1080 => 1920,
        }
    }

    /// Altura em pixels.
    #[must_use]
    pub const fn altura(self) -> usize {
        match self {
            Self::P540 => 540,
            Self::P720 => 720,
            Self::P1080 => 1080,
        }
    }
}

/// Os três tetos de quadros por segundo que o §5 fechou.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cadencia {
    /// 8 por segundo, o menor que se oferece — o número que o §2 nomeia como o
    /// ponto em que texto ainda se lê.
    Q8,
    /// 15 por segundo.
    Q15,
    /// 30 por segundo. O padrão, e o máximo (§6 item 10).
    #[default]
    Q30,
}

impl Cadencia {
    /// As três, da menor para a maior.
    pub const TODAS: [Self; 3] = [Self::Q8, Self::Q15, Self::Q30];

    /// Quadros por segundo.
    #[must_use]
    pub const fn hz(self) -> u32 {
        match self {
            Self::Q8 => 8,
            Self::Q15 => 15,
            Self::Q30 => 30,
        }
    }
}

/// O que a pessoa escolheu, e é **tudo teto** (§5).
///
/// A regra que não se negocia: o que se escolhe é o **máximo**, e o sistema
/// continua livre para ficar abaixo. Se virar piso, a regra de aceite do §3.2
/// cai — *a voz nunca cede à tela* — e volta o que o `tela-no-transporte`
/// mediu: 225 ms de atraso na voz contra 22 ms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConfigDoCodificador {
    /// A resolução, que **segura**: é o quadro que cede, não ela.
    pub resolucao: Resolucao,
    /// O máximo de quadros por segundo. Por baixo dele roda a faixa automática
    /// de [`PISO_DE_QUADROS`] a [`Cadencia::hz`].
    pub cadencia: Cadencia,
    /// O teto de banda, em bits por segundo.
    ///
    /// Zero significa [`TETO_DA_PROVA_BPS`], que é o que [`Default`] entrega —
    /// e o valor de verdade é 60% do caminho medido (§3.2).
    pub teto_bps: u32,
}

impl ConfigDoCodificador {
    /// O teto de banda em uso, resolvendo o zero para [`TETO_DA_PROVA_BPS`].
    #[must_use]
    pub const fn teto_efetivo_bps(&self) -> u32 {
        if self.teto_bps == 0 {
            TETO_DA_PROVA_BPS
        } else {
            self.teto_bps
        }
    }
}

/// Um quadro em I420 (YUV 4:2:0 planar), que é o único formato que o OpenH264
/// aceita e o único que ele devolve.
///
/// Os planos vêm empacotados: `y` tem `largura × altura` bytes, `u` e `v` têm
/// `⌈largura/2⌉ × ⌈altura/2⌉` cada. **A captura converte para cá**, e essa
/// conversão tem custo próprio — `spikes/tela-no-codec` não a mediu, e diz isso
/// na cara.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuadroI420 {
    largura: usize,
    altura: usize,
    y: Vec<u8>,
    u: Vec<u8>,
    v: Vec<u8>,
}

/// Quantos bytes o plano de luma tem.
#[must_use]
pub const fn bytes_de_luma(largura: usize, altura: usize) -> usize {
    largura * altura
}

/// Quantos bytes cada plano de croma tem.
#[must_use]
pub const fn bytes_de_croma(largura: usize, altura: usize) -> usize {
    largura.div_ceil(2) * altura.div_ceil(2)
}

impl QuadroI420 {
    /// Monta um quadro a partir dos três planos.
    ///
    /// # Errors
    ///
    /// [`ErroDeVideo::PlanosInconsistentes`] se os tamanhos não fecham. A
    /// binding devolveria «invalid input YUV size», que não diz qual plano nem
    /// qual lado errou; um plano curto entregue ao C é leitura fora de área.
    pub fn novo(
        largura: usize,
        altura: usize,
        y: Vec<u8>,
        u: Vec<u8>,
        v: Vec<u8>,
    ) -> Result<Self, ErroDeVideo> {
        let esperado = (
            bytes_de_luma(largura, altura),
            bytes_de_croma(largura, altura),
            bytes_de_croma(largura, altura),
        );
        let recebido = (y.len(), u.len(), v.len());
        if recebido != esperado {
            return Err(ErroDeVideo::PlanosInconsistentes { esperado, recebido });
        }
        Ok(Self {
            largura,
            altura,
            y,
            u,
            v,
        })
    }

    /// Um quadro preto, para quem precisa de um quadro e não de uma imagem.
    #[must_use]
    pub fn preto(largura: usize, altura: usize) -> Self {
        // 16 e 128 são o preto do intervalo de TV, que é o que um quadro de
        // captura usa. Zerar tudo daria verde.
        Self {
            largura,
            altura,
            y: vec![16; bytes_de_luma(largura, altura)],
            u: vec![128; bytes_de_croma(largura, altura)],
            v: vec![128; bytes_de_croma(largura, altura)],
        }
    }

    /// Largura em pixels.
    #[must_use]
    pub const fn largura(&self) -> usize {
        self.largura
    }

    /// Altura em pixels.
    #[must_use]
    pub const fn altura(&self) -> usize {
        self.altura
    }

    /// O plano de luma.
    #[must_use]
    pub fn luma(&self) -> &[u8] {
        &self.y
    }

    /// O plano de croma azul.
    #[must_use]
    pub fn croma_u(&self) -> &[u8] {
        &self.u
    }

    /// O plano de croma vermelho.
    #[must_use]
    pub fn croma_v(&self) -> &[u8] {
        &self.v
    }
}

/// Um quadro já codificado, em Annex-B, pronto para o fluxo do §3.6.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuadroCodificado {
    /// Se é quadro-chave, ou seja se quem recebe pode começar a decodificar por
    /// ele.
    ///
    /// Num quadro-chave os bytes **já trazem SPS e PPS na frente**. Isso é
    /// decisão desta camada, não da binding: ela devolve os três separados, e
    /// mandar só o `data` produziria um fluxo que nenhum decoder abre. É a
    /// primeira coisa que o teste de ida-e-volta prova.
    pub chave: bool,
    /// Os bytes, com códigos de início Annex-B.
    ///
    /// **Um quadro-chave de 1080p tem 65 KiB**, quatro vezes um quadro comum, e
    /// 65 KiB são 446 ms do orçamento inteiro de 1200 kbps. O §3.3 manda
    /// espalhá-los por alguns intervalos de quadro em vez de despejá-los num
    /// tique — quem faz isso é o transporte, e é por isso que este campo é um
    /// `Vec` inteiro e não um `write` direto no fluxo.
    pub bytes: Vec<u8>,
}

const INICIO_ANNEX_B: [u8; 4] = [0, 0, 0, 1];

fn montar_annex_b(quadro: &EncodedFrame) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(quadro.data.len() + 64);
    for parametro in quadro.sps_list.iter().chain(quadro.pps_list.iter()) {
        bytes.extend_from_slice(&INICIO_ANNEX_B);
        bytes.extend_from_slice(parametro);
    }
    // A binding separa SPS e PPS do resto e devolve os dois **sem** código de
    // início. Recolocá-los na frente é o que faz o quadro-chave ser
    // autossuficiente, que é a única propriedade que interessa nele.
    bytes.extend_from_slice(&quadro.data);
    bytes
}

/// Um fluxo de saída. Um por transmissão.
///
/// É `Send` porque o §2 manda o encoder morar numa **thread própria**, com
/// prioridade abaixo do normal, e nunca no runtime que carrega os datagramas de
/// voz. Não é `Sync`, e isso é certo: dois lados codificando no mesmo encoder
/// embaralhariam a predição.
#[derive(Debug)]
pub struct Codificador {
    encoder: Encoder,
    resolucao: Resolucao,
    cadencia: Cadencia,
    quadros_por_segundo: u32,
    teto_bps: u32,
}

impl Codificador {
    /// Arma um codificador com a escolha de quem compartilha.
    ///
    /// # Errors
    ///
    /// [`ErroDeVideo::CodecRecusou`] se o OpenH264 não aceitar a configuração.
    pub fn novo(
        biblioteca: &BibliotecaDeVideo,
        config: ConfigDoCodificador,
    ) -> Result<Self, ErroDeVideo> {
        let resolucao = config.resolucao;
        let quadros = config.cadencia.hz();
        let teto = config.teto_efetivo_bps();

        let mut bruta = EncoderConfig::new(
            resolucao.largura(),
            resolucao.altura(),
            teto as usize,
            quadros as usize,
            1,
        );
        // Modo bitrate, e não qualidade: o §3.2 diz que o vídeo tem teto, e este
        // é o único modo em que o OpenH264 respeita um. É também o único em que
        // ele pula quadro sozinho — o que é informação, não defeito, e sai daqui
        // como `Ok(None)`.
        bruta.rate_control_mode = Some(RateControlMode::Bitrate);
        // CAVLC é o que faz o OpenH264 escolher o perfil baseline; CABAC o
        // levaria a High. O §2 pediu baseline, e é também o único perfil que ele
        // sabe codificar — dizê-lo aqui não muda o que os spikes mediram, torna
        // a decisão visível no lugar onde alguém iria mexer nela.
        bruta.entropy_coding_mode = Some(EntropyCodingMode::Cavlc);
        // Zero é «nenhum quadro-chave periódico», que é o §3.3: chave **sob
        // demanda**, quando quem recebe pede. O preço do periódico está medido —
        // forçar um a cada 2 s tira 21% dos quadros por segundo e sobe o
        // descarte de 16,2% para 17,6%, para um receptor que não precisava.
        //
        // **Hoje esta linha não muda comportamento nenhum**, e vale escrever em
        // vez de deixar sugerido o contrário: o `GetDefaultParams` do OpenH264
        // já entrega zero, e tirá-la daqui não deixa nenhum teste vermelho —
        // conferido por mutação. Ela fica porque um padrão de biblioteca não é
        // uma decisão nossa: no dia em que o OpenH264 mudar o dele, o §3.3
        // cairia em silêncio.
        bruta.intra_period = Some(0);
        // Uma fatia, uma thread, e as duas linhas andam juntas: o OpenH264 só
        // paraleliza **entre fatias**, então pedir mais threads sem fatiar não
        // faz nada, e fatiar custa qualidade porque a predição não atravessa
        // fatia. Medido em duas máquinas: 2,4× de quadros por 2,5× de CPU, com o
        // descarte subindo de 16,2% para 23,9%. Numa máquina que entrega
        // dezesseis vezes o necessário, isso é qualidade jogada fora por
        // latência que ninguém pediu — e seriam três threads que o §2 não
        // desenhou, ao lado da única que ele desenhou.
        bruta.slice_mode = Some(SliceMode::Single);
        bruta.thread_count = NonZeroUsize::new(1);

        let encoder = Encoder::new(biblioteca.lib.clone(), bruta).map_err(|erro| {
            ErroDeVideo::CodecRecusou {
                operacao: "armar o codificador",
                detalhe: erro.to_string(),
            }
        })?;

        Ok(Self {
            encoder,
            resolucao,
            cadencia: config.cadencia,
            quadros_por_segundo: quadros,
            teto_bps: teto,
        })
    }

    /// A resolução com que ele foi armado. Ela não cede (§2).
    #[must_use]
    pub const fn resolucao(&self) -> Resolucao {
        self.resolucao
    }

    /// O teto de quadros que quem compartilha escolheu.
    #[must_use]
    pub const fn cadencia(&self) -> Cadencia {
        self.cadencia
    }

    /// Quantos quadros por segundo ele está mirando agora, dentro da faixa.
    #[must_use]
    pub const fn quadros_por_segundo(&self) -> u32 {
        self.quadros_por_segundo
    }

    /// O teto de banda em uso, em bits por segundo.
    #[must_use]
    pub const fn teto_bps(&self) -> u32 {
        self.teto_bps
    }

    /// Muda o teto de banda sem refazer o codificador.
    ///
    /// **Isto é o §3.2 virando código.** O teto é uma fração do caminho medido,
    /// e o caminho muda enquanto a conversa acontece: quando o sinal da voz cai
    /// de faixa, quem baixa é o vídeo. Por isso é um `SetOption` e não uma
    /// reconstrução — ao contrário do que a voz faz em
    /// `seele_audio::codec::VoiceEncoder::set_bitrate`, aqui refazer o encoder
    /// custaria um quadro-chave inteiro, que são 65 KiB e 446 ms de orçamento.
    ///
    /// # Errors
    ///
    /// [`ErroDeVideo::CodecRecusou`] se o OpenH264 recusar o valor.
    pub fn ajustar_teto(&mut self, teto_bps: u32) -> Result<(), ErroDeVideo> {
        if teto_bps == self.teto_bps {
            return Ok(());
        }
        self.encoder
            .set_bitrate(teto_bps as usize)
            .map_err(|erro| ErroDeVideo::CodecRecusou {
                operacao: "mudar o teto de banda",
                detalhe: erro.to_string(),
            })?;
        self.teto_bps = teto_bps;
        Ok(())
    }

    /// Muda quantos quadros por segundo ele mira, dentro da faixa automática.
    ///
    /// O pedido é **grampeado** entre [`PISO_DE_QUADROS`] e a cadência que a
    /// pessoa escolheu, e devolve o que ficou valendo. É a regra do §5 escrita
    /// em código: a escolha é teto, nunca piso, e quem quiser mais que ela não
    /// consegue por aqui.
    ///
    /// Abaixo do piso não se degrada: o §2 manda **parar**, com motivo
    /// enumerado. Degradar para sempre é como um instrumento falso, consultado
    /// justamente quando algo deu errado.
    ///
    /// # Errors
    ///
    /// [`ErroDeVideo::CodecRecusou`] se o OpenH264 recusar o valor.
    pub fn ajustar_quadros(&mut self, quadros_por_segundo: u32) -> Result<u32, ErroDeVideo> {
        let valendo = quadros_por_segundo.clamp(PISO_DE_QUADROS, self.cadencia.hz());
        if valendo == self.quadros_por_segundo {
            return Ok(valendo);
        }
        self.encoder
            .set_frame_rate(valendo as usize, 1)
            .map_err(|erro| ErroDeVideo::CodecRecusou {
                operacao: "mudar os quadros por segundo",
                detalhe: erro.to_string(),
            })?;
        self.quadros_por_segundo = valendo;
        Ok(valendo)
    }

    /// Codifica um quadro.
    ///
    /// `pedido_de_chave` é o §3.3: quadro-chave **quando quem recebe pede**, e
    /// não de tempos em tempos.
    ///
    /// `Ok(None)` é o controle de taxa tendo pulado este quadro para não
    /// estourar o teto. **Não é perda e não é erro** — é o comportamento que o
    /// §5 obriga a interface a mostrar, porque quem escolheu 1080p num caminho
    /// doméstico recebe uns 25 quadros e não 30.
    ///
    /// # Errors
    ///
    /// [`ErroDeVideo::QuadroDeTamanhoErrado`] se o quadro não é do tamanho com
    /// que o codificador foi armado. [`ErroDeVideo::CodecRecusou`] se o
    /// OpenH264 recusar.
    pub fn codificar(
        &mut self,
        quadro: &QuadroI420,
        pedido_de_chave: bool,
    ) -> Result<Option<QuadroCodificado>, ErroDeVideo> {
        if quadro.largura() != self.resolucao.largura()
            || quadro.altura() != self.resolucao.altura()
        {
            return Err(ErroDeVideo::QuadroDeTamanhoErrado {
                esperado: (self.resolucao.largura(), self.resolucao.altura()),
                recebido: (quadro.largura(), quadro.altura()),
            });
        }

        let opcoes = EncodeOptions {
            force_idr: pedido_de_chave,
        };
        let saida = self
            .encoder
            .encode(quadro.luma(), quadro.croma_u(), quadro.croma_v(), &opcoes)
            .map_err(|erro| ErroDeVideo::CodecRecusou {
                operacao: "codificar um quadro",
                detalhe: erro.to_string(),
            })?;

        Ok(saida.map(|bruto| QuadroCodificado {
            chave: bruto.frame_type == FrameType::Idr,
            bytes: montar_annex_b(&bruto),
        }))
    }
}

/// Um fluxo de entrada. Um por transmissão que se está assistindo.
///
/// **Quanto custa decodificar não foi medido.** `spikes/tela-no-codec` mediu só
/// o encode e diz isso na cara. Quem recebe também gasta CPU, e o número não
/// existe.
#[derive(Debug)]
pub struct Decodificador {
    decoder: Decoder,
}

impl Decodificador {
    /// Arma um decodificador.
    ///
    /// # Errors
    ///
    /// [`ErroDeVideo::CodecRecusou`] se o OpenH264 não criar o decoder.
    pub fn novo(biblioteca: &BibliotecaDeVideo) -> Result<Self, ErroDeVideo> {
        Decoder::new(biblioteca.lib.clone())
            .map(|decoder| Self { decoder })
            .map_err(|erro| ErroDeVideo::CodecRecusou {
                operacao: "armar o decodificador",
                detalhe: erro.to_string(),
            })
    }

    /// Decodifica um quadro Annex-B.
    ///
    /// `Ok(None)` quando os bytes não completaram um quadro — parâmetros
    /// sozinhos, por exemplo. É estado normal de um fluxo que acabou de abrir,
    /// e não motivo para desistir dele.
    ///
    /// # Errors
    ///
    /// [`ErroDeVideo::CodecRecusou`] se o OpenH264 recusar os bytes.
    pub fn decodificar(&mut self, bytes: &[u8]) -> Result<Option<QuadroI420>, ErroDeVideo> {
        let saida = self
            .decoder
            .decode(bytes)
            .map_err(|erro| ErroDeVideo::CodecRecusou {
                operacao: "decodificar um quadro",
                detalhe: erro.to_string(),
            })?;
        saida.map(|bruto| empacotar(&bruto)).transpose()
    }
}

/// O decoder devolve planos com **passo maior que a largura** — ele alinha as
/// linhas para o SIMD. Copiar linha a linha é o que transforma isso num I420
/// empacotado; entregar os planos crus com o passo perdido daria uma imagem
/// enviesada, que é o defeito clássico deste ponto do código.
fn empacotar(quadro: &shiguredo_openh264::DecodedFrame) -> Result<QuadroI420, ErroDeVideo> {
    let largura = quadro.width();
    let altura = quadro.height();

    let y = compactar(quadro.y_plane(), quadro.y_stride(), largura, altura);
    let u = compactar(
        quadro.u_plane(),
        quadro.u_stride(),
        largura.div_ceil(2),
        altura.div_ceil(2),
    );
    let v = compactar(
        quadro.v_plane(),
        quadro.v_stride(),
        largura.div_ceil(2),
        altura.div_ceil(2),
    );

    QuadroI420::novo(largura, altura, y, u, v)
}

fn compactar(plano: &[u8], passo: usize, largura: usize, altura: usize) -> Vec<u8> {
    let mut saida = Vec::with_capacity(largura * altura);
    for linha in 0..altura {
        let inicio = linha * passo;
        // `chunks` em vez de índice: `indexing_slicing` é aviso nesta casa
        // justamente porque um passo mentiroso vindo de FFI viraria pânico.
        let resto = plano.get(inicio..).unwrap_or_default();
        saida.extend_from_slice(resto.get(..largura).unwrap_or(resto));
    }
    saida
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Prova em tempo de compilação do que o §2 exige: o encoder mora numa
    /// thread própria, com prioridade abaixo do normal, e nunca no runtime que
    /// carrega os datagramas de voz. Se um campo não-`Send` entrar aqui, essa
    /// frase deixa de ser possível e este teste é onde se descobre.
    const fn _e_send<T: Send>() {}
    const _: () = _e_send::<Codificador>();
    const _: () = _e_send::<Decodificador>();
    const _: () = _e_send::<QuadroI420>();

    #[test]
    fn a_lista_de_resolucoes_e_a_que_o_spike_fechou() {
        // O §5 deixou esta lista em aberto de propósito até haver medida, e
        // `spikes/tela-no-codec` a fechou. Cada linha aqui tem um motivo escrito
        // na spec, e uma quarta entrada precisaria do seu.
        assert_eq!(Resolucao::TODAS.len(), 3);
        assert_eq!(Resolucao::default(), Resolucao::P720);
        assert_eq!(
            Resolucao::TODAS.map(|r| (r.largura(), r.altura())),
            [(960, 540), (1280, 720), (1920, 1080)]
        );
        // §6 item 10: nada acima de 1080p, e a medida não mudou isso.
        assert!(Resolucao::TODAS.iter().all(|r| r.altura() <= 1080));
        // 360p ficou de fora com número: rende 416 kbps dos 1200 disponíveis,
        // ou seja torra nitidez sem gastar o orçamento que já tinha.
        assert!(Resolucao::TODAS.iter().all(|r| r.altura() >= 540));
    }

    #[test]
    fn a_lista_de_quadros_nao_oferece_o_piso() {
        // O 5 é o piso da faixa automática, e escolher o piso é escolher
        // desistir. Desistir é o que o sistema faz sozinho, com motivo
        // enumerado — não é uma opção de tela (§5).
        assert_eq!(Cadencia::TODAS.map(Cadencia::hz), [8, 15, 30]);
        assert_eq!(Cadencia::default(), Cadencia::Q30);
        assert!(!Cadencia::TODAS.iter().any(|c| c.hz() == PISO_DE_QUADROS));
        assert!(Cadencia::TODAS.iter().all(|c| c.hz() <= 30));
    }

    #[test]
    fn os_planos_de_um_i420_fecham_a_conta() {
        // Um plano curto entregue ao C é leitura fora de área, e a binding
        // devolveria «invalid input YUV size» sem dizer qual dos três.
        let erro = QuadroI420::novo(4, 4, vec![0; 16], vec![0; 4], vec![0; 3])
            .expect_err("o plano V está curto");
        assert_eq!(
            erro,
            ErroDeVideo::PlanosInconsistentes {
                esperado: (16, 4, 4),
                recebido: (16, 4, 3),
            }
        );

        // Ímpar arredonda para cima nos dois eixos, que é o que 4:2:0 quer
        // dizer, e é onde uma conta com divisão inteira erra por uma linha.
        assert_eq!(bytes_de_croma(1919, 1079), 960 * 540);
        QuadroI420::novo(3, 3, vec![0; 9], vec![0; 4], vec![0; 4]).expect("3x3 é um I420 válido");
    }

    #[test]
    fn o_teto_zero_cai_no_da_prova() {
        assert_eq!(
            ConfigDoCodificador::default().teto_efetivo_bps(),
            TETO_DA_PROVA_BPS
        );
        assert_eq!(
            ConfigDoCodificador {
                teto_bps: 640_000,
                ..ConfigDoCodificador::default()
            }
            .teto_efetivo_bps(),
            640_000
        );
    }

    #[test]
    fn o_quadro_chave_leva_sps_e_pps_na_frente() {
        // Sem módulo do Cisco não há encoder para produzir um, então a montagem
        // é testada aqui sobre um `EncodedFrame` armado à mão. É a única parte
        // do caminho que é nossa, e é a que produziria um fluxo que nenhum
        // decoder abre se estivesse errada.
        let bruto = EncodedFrame {
            frame_type: FrameType::Idr,
            sps_list: vec![vec![0x67, 0x42]],
            pps_list: vec![vec![0x68, 0xCE]],
            data: vec![0, 0, 0, 1, 0x65, 0x88],
        };

        assert_eq!(
            montar_annex_b(&bruto),
            vec![
                0, 0, 0, 1, 0x67, 0x42, // SPS, com o código de início recolocado
                0, 0, 0, 1, 0x68, 0xCE, // PPS
                0, 0, 0, 1, 0x65, 0x88, // e o quadro, que já vinha com o dele
            ]
        );
    }

    #[test]
    fn um_quadro_comum_nao_ganha_parametros_que_nao_tem() {
        let bruto = EncodedFrame {
            frame_type: FrameType::P,
            sps_list: Vec::new(),
            pps_list: Vec::new(),
            data: vec![0, 0, 0, 1, 0x41, 0x9A],
        };
        assert_eq!(montar_annex_b(&bruto), bruto.data);
    }

    #[test]
    fn compactar_joga_fora_o_enchimento_de_cada_linha() {
        // Duas linhas de 3 pixels num plano de passo 5. Guardar o enchimento
        // enviesaria a imagem em um pixel por linha, que é o defeito clássico
        // deste ponto e o que não aparece num teste de tamanho.
        let plano = vec![1, 2, 3, 9, 9, 4, 5, 6, 9, 9];
        assert_eq!(compactar(&plano, 5, 3, 2), vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn compactar_nao_entra_em_panico_com_um_passo_mentiroso() {
        // O passo vem de FFI. `specs/08-seguranca.md` é o motivo de
        // `indexing_slicing` ser aviso mesmo em teste: um número absurdo tem de
        // devolver pouco, nunca derrubar quem estava assistindo.
        assert!(compactar(&[1, 2, 3], 999, 3, 2).len() <= 6);
        assert_eq!(compactar(&[], 4, 2, 2), Vec::<u8>::new());
    }
}
