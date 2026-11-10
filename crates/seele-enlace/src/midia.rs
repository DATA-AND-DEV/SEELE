//! A mídia RTP de uma conexão: vídeo H.264 e som Opus, **sem SDP** (ADR 0067).
//!
//! O enlace não troca descrição nenhuma (ADR 0063), então as duas pontas
//! declaram, pela API direta do `str0m`, a mesma tabela fixa: os `mid`, os
//! SSRC de cada sentido e os tipos de carga. A tabela é parte do protocolo:
//! mudar um número dela é mudar de versão.
//!
//! A mídia só existe depois de [`crate::Conexao::ligar_midia`], que as duas
//! pontas chamam quando a versão negociada no `ctl` diz que o par fala RTP.
//! Uma ponta que não ligou não manda nem espera pacote de mídia nenhum, e é
//! por isso que um par antigo (que entregaria RTP à fila do ponto de
//! encontro) nunca vê um.
//!
//! A unidade é o **quadro**: o `str0m` empacota na ida e desempacota na volta,
//! pede retransmissão (NACK/RTX) e estima a banda (TWCC). Quem usa manda e
//! recebe quadros inteiros.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use str0m::bwe::Bitrate;
use str0m::format::Codec;
use str0m::media::{Frequency, KeyframeRequestKind, MediaKind, MediaTime, Mid, Pt};
use str0m::rtp::Ssrc;
use str0m::{Rtc, RtcConfig};
use tokio::sync::mpsc;

use crate::conexao::{Comando, Compartilhado};
use crate::Recusa;

/// O `mid` da mídia de vídeo.
pub const MID_DO_VIDEO: &str = "v";
/// O `mid` da mídia de som (o som da tela).
pub const MID_DO_SOM: &str = "s";

/// Os SSRC de quem **atende** (o anfitrião): vídeo, a retransmissão do vídeo, som.
pub const SSRC_DO_ANFITRIAO: [u32; 3] = [0x5EE1_0001, 0x5EE1_0002, 0x5EE1_0003];
/// Os SSRC de quem **liga**: vídeo, a retransmissão do vídeo, som.
pub const SSRC_DE_QUEM_LIGA: [u32; 3] = [0x5EE1_0011, 0x5EE1_0012, 0x5EE1_0013];

/// O relógio do vídeo no RTP.
pub const RELOGIO_DO_VIDEO: u32 = 90_000;
/// O relógio do som no RTP.
pub const RELOGIO_DO_SOM: u32 = 48_000;

/// O maior quadro que [`Midia::mandar`] aceita. É o teto do quadro de tela
/// do protocolo (`MAX_QUADRO_LEN`).
pub const MAX_QUADRO: usize = 4 * 1024 * 1024;

/// Quantos comandos de quadro podem esperar a tarefa da ponta, por conexão.
/// Além disso o quadro é recusado: vídeo atrasado não serve.
pub const FILA_DE_MIDIA: usize = 64;

/// Quantos quadros recebidos esperam quem lê. Cheia, o quadro é largado e o
/// seguinte chega marcado como não contíguo.
pub const FILA_DE_RECEPCAO: usize = 128;

/// De onde a estimativa de banda parte, antes de qualquer retorno.
pub const ESTIMATIVA_INICIAL_BPS: u64 = 300_000;

/// Quanto o receptor espera por um pacote de vídeo perdido antes de largar o
/// quadro. O padrão do `str0m` é 2 s, e a medida
/// (`docs/medidas/2026-11-10-rtp-sob-perda.md`) mostrou que cada quebra custa
/// esse tempo de imagem parada.
pub const ESPERA_PELA_RETRANSMISSAO: Duration = Duration::from_millis(500);

/// Qual das duas mídias.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trilha {
    /// O vídeo, H.264 em Annex B, um quadro (uma unidade de acesso) por vez.
    Video,
    /// O som, um pacote Opus por vez.
    Som,
}

/// O que chega da mídia de uma conexão.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventoDeMidia {
    /// Um quadro inteiro.
    Quadro {
        /// De qual mídia.
        trilha: Trilha,
        /// O instante do quadro, no relógio da mídia (90 kHz o vídeo, 48 kHz o som).
        tempo: u64,
        /// **Nada se perdeu entre o quadro anterior e este.** Com `false`, um
        /// quadro comum de vídeo não pode ser decodificado: quem lê espera um
        /// quadro-chave (e pede um).
        contiguo: bool,
        /// Os bytes do quadro. Compartilhados: repassar a vários não copia.
        bytes: Arc<[u8]>,
    },
    /// O par pediu um quadro-chave (PLI).
    PediramChave,
}

/// A mídia de uma conexão. Vem de [`crate::Conexao::ligar_midia`].
pub struct Midia {
    pub(crate) comp: Arc<Compartilhado>,
    pub(crate) cmd: mpsc::UnboundedSender<Comando>,
    pub(crate) fila: mpsc::Receiver<EventoDeMidia>,
}

impl std::fmt::Debug for Midia {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Midia").field("par", &self.comp.id).finish()
    }
}

impl Midia {
    /// Manda um quadro. Nunca espera.
    ///
    /// `tempo` é o instante do quadro no relógio da mídia
    /// ([`RELOGIO_DO_VIDEO`], [`RELOGIO_DO_SOM`]).
    ///
    /// # Errors
    ///
    /// [`Recusa`]: grande demais, fila cheia, ou a conexão acabou.
    pub fn mandar(&self, trilha: Trilha, tempo: u64, bytes: Arc<[u8]>) -> Result<(), Recusa> {
        if bytes.len() > MAX_QUADRO {
            return Err(Recusa::Grande);
        }
        if self.comp.motivo.borrow().is_some() {
            return Err(Recusa::Fechada);
        }
        if self.comp.midia_na_fila.fetch_add(1, Ordering::Relaxed) >= FILA_DE_MIDIA {
            self.comp.midia_na_fila.fetch_sub(1, Ordering::Relaxed);
            return Err(Recusa::Cheia);
        }
        self.cmd
            .send(Comando::Midia {
                par: self.comp.id,
                trilha,
                tempo,
                bytes,
            })
            .map_err(|_| Recusa::Fechada)
    }

    /// Pede um quadro-chave ao par (PLI).
    pub fn pedir_chave(&self) {
        let _ = self.cmd.send(Comando::PedirChave { par: self.comp.id });
    }

    /// Diz à estimativa de banda até onde vale a pena procurar: sem isso
    /// ela não sonda acima do que já vê passar.
    pub fn taxa_desejada(&self, desejada_bps: u64) {
        let _ = self.cmd.send(Comando::TaxaDaMidia {
            par: self.comp.id,
            desejada_bps,
        });
    }

    /// A banda que o caminho de saída carrega, pela estimativa do RTP (TWCC).
    /// `None` enquanto nenhum retorno chegou.
    #[must_use]
    pub fn estimativa_bps(&self) -> Option<u64> {
        let bps = self.comp.estimativa_bps.load(Ordering::Relaxed);
        (bps > 0).then_some(bps)
    }

    /// O próximo evento. `None` quando a conexão acabou.
    pub async fn receber(&mut self) -> Option<EventoDeMidia> {
        self.fila.recv().await
    }
}

/// O que a tarefa da ponta guarda da mídia de um par.
pub(crate) struct MidiaDoPar {
    fila: mpsc::Sender<EventoDeMidia>,
    /// Um quadro foi largado por fila cheia: o próximo não é contíguo.
    furo: bool,
    video: Pt,
    som: Pt,
}

/// O que todo `Rtc` do enlace leva para poder ter mídia. Sem mídia declarada,
/// nada disto manda um pacote.
pub(crate) fn configurar(config: RtcConfig) -> RtcConfig {
    config
        .clear_codecs()
        .enable_h264(true)
        .enable_opus(true, false)
        .enable_bwe(Some(Bitrate::bps(ESTIMATIVA_INICIAL_BPS)))
        .set_reordering_timeout_video(Some(ESPERA_PELA_RETRANSMISSAO))
        .set_reordering_size_audio(0)
}

/// Declara a mídia num `Rtc`. `anfitriao` diz de que lado da tabela ele está.
/// `None` se a configuração do `Rtc` não tem H.264 ou Opus.
pub(crate) fn declarar(
    rtc: &mut Rtc,
    anfitriao: bool,
    fila: mpsc::Sender<EventoDeMidia>,
) -> Option<MidiaDoPar> {
    let (meus, dele) = if anfitriao {
        (SSRC_DO_ANFITRIAO, SSRC_DE_QUEM_LIGA)
    } else {
        (SSRC_DE_QUEM_LIGA, SSRC_DO_ANFITRIAO)
    };
    let video = pt_de(rtc, Codec::H264)?;
    let som = pt_de(rtc, Codec::Opus)?;
    let mid_v = Mid::from(MID_DO_VIDEO);
    let mid_s = Mid::from(MID_DO_SOM);
    let mut api = rtc.direct_api();
    api.declare_media(mid_v, MediaKind::Video);
    api.declare_media(mid_s, MediaKind::Audio);
    let [v, rtx, s] = meus.map(Ssrc::from);
    api.declare_stream_tx(v, Some(rtx), mid_v, None);
    api.declare_stream_tx(s, None, mid_s, None);
    let [v, rtx, s] = dele.map(Ssrc::from);
    api.expect_stream_rx(v, Some(rtx), mid_v, None);
    api.expect_stream_rx(s, None, mid_s, None);
    api.enable_twcc_feedback();
    Some(MidiaDoPar {
        fila,
        furo: false,
        video,
        som,
    })
}

/// O primeiro tipo de carga do codec na configuração do `Rtc`. As duas pontas
/// têm a mesma configuração ([`configurar`]), então escolhem o mesmo.
fn pt_de(rtc: &Rtc, codec: Codec) -> Option<Pt> {
    rtc.codec_config()
        .params()
        .iter()
        .find(|p| p.spec().codec == codec)
        .map(|p| p.pt())
}

impl MidiaDoPar {
    /// Um punho novo: os eventos passam a ir para a fila dele.
    pub(crate) fn trocar_a_fila(&mut self, fila: mpsc::Sender<EventoDeMidia>) {
        self.fila = fila;
        self.furo = true;
    }

    /// Escreve um quadro no `Rtc`. `false` se ele não aceitou.
    pub(crate) fn escrever(
        &self,
        rtc: &mut Rtc,
        agora: Instant,
        trilha: Trilha,
        tempo: u64,
        bytes: Arc<[u8]>,
    ) -> bool {
        let (mid, pt, relogio) = match trilha {
            Trilha::Video => (MID_DO_VIDEO, self.video, Frequency::NINETY_KHZ),
            Trilha::Som => (MID_DO_SOM, self.som, Frequency::FORTY_EIGHT_KHZ),
        };
        rtc.writer(Mid::from(mid)).is_some_and(|escritor| {
            escritor
                .write(pt, agora, MediaTime::new(tempo, relogio), bytes)
                .is_ok()
        })
    }

    /// Pede um quadro-chave do vídeo ao par.
    pub(crate) fn pedir_chave(rtc: &mut Rtc) {
        if let Some(mut escritor) = rtc.writer(Mid::from(MID_DO_VIDEO)) {
            let _ = escritor.request_keyframe(None, KeyframeRequestKind::Pli);
        }
    }

    /// Um quadro chegou do `Rtc`.
    pub(crate) fn chegou(&mut self, dados: str0m::media::MediaData) {
        let trilha = if dados.mid == Mid::from(MID_DO_VIDEO) {
            Trilha::Video
        } else if dados.mid == Mid::from(MID_DO_SOM) {
            Trilha::Som
        } else {
            return;
        };
        let contiguo = dados.contiguous && !(self.furo && trilha == Trilha::Video);
        let evento = EventoDeMidia::Quadro {
            trilha,
            tempo: dados.time.numer(),
            contiguo,
            bytes: dados.data,
        };
        match self.fila.try_send(evento) {
            Ok(()) => {
                if trilha == Trilha::Video {
                    self.furo = false;
                }
            }
            Err(_) => {
                if trilha == Trilha::Video {
                    self.furo = true;
                }
            }
        }
    }

    /// O par pediu um quadro-chave.
    pub(crate) fn pediram_chave(&self) {
        let _ = self.fila.try_send(EventoDeMidia::PediramChave);
    }
}
