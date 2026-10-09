//! What opens a screen transmission's own stream.
//!
//! `docs/superpowers/specs/2026-08-22-compartilhamento-de-tela-design.md` §3.1:
//! **one unidirectional QUIC stream per transmission, and never a datagram.**
//! The reason is measured rather than argued — `spikes/tela-no-transporte` put
//! video on `send_datagram` and lost 16,1% of the voice with 2,16 s of delay,
//! because `quinn-proto` puts voice and video in the same FIFO and drops the
//! **oldest** when it fills. Shrinking that buffer inverts the failure instead
//! of fixing it: 98,1% of the voice discarded before leaving the machine. With
//! the video on a stream the voice loses 0,1%, because `quinn-proto` writes
//! `DATAGRAM` frames before `STREAM` frames in every packet.
//!
//! # The opening header, and nothing else
//!
//! §3.6 asks for a header "saying what it is (monitor or window), the
//! resolution and the codec — versioned by the first byte, like every control
//! frame". That is this, and the frames follow it to the end of the stream.
//!
//! It is a **fixed layout read with [`slice::get`]**, not a `postcard` message
//! like [`crate::attachment::AttachmentHeader`], and the difference is what the
//! two carry: an attachment header holds strings and needs a length in front of
//! it, while every field here is a small number of known width. Eleven bytes
//! read into an array on the stack means a receiver allocates nothing at all
//! before it knows what it is being sent, which is the posture
//! `specs/08-seguranca.md` asks of every network input.
//!
//! # Os quadros que seguem a abertura
//!
//! Depois dos onze bytes, o fluxo é uma sequência de quadros, cada um com um
//! cabeçalho de [`CABECALHO_DE_QUADRO_LEN`] bytes: um de tipo
//! ([`TipoDeQuadro`]: imagem comum, imagem-chave ou som) e quatro de tamanho.
//!
//! **Até 09/10/2026 esta camada vivia duas vezes**, no `seele-core` e no
//! `seele-server`, com um comentário dizendo que a cópia era deliberada,
//! porque um crate de transporte não teria dono. O dono é este crate, que já
//! tinha a abertura do mesmo fluxo. O celular (D-celular-2) precisava de uma
//! terceira leitura, e ela é esta. As duas cópias passam a usar este módulo
//! num passo da F1.
//!
//! Há duas leituras, e as duas leem o cabeçalho de um quadro pela mesma função
//! privada, que é onde a regra «recusa antes de alocar» mora:
//! - o [`Enquadramento`] **conta bytes** e acha a porta de entrada de quem
//!   chega no meio. Ele não remonta quadro, de propósito: o servidor repassa o
//!   pedaço como chegou (§3.3);
//! - o [`LeitorDeTela`] **remonta**: a abertura primeiro, os quadros inteiros
//!   depois, para quem decodifica.
//!
//! # `ssrc` is not used here
//!
//! §3.6 again, and it is the one thing that section puts in bold: [`crate::ids::Ssrc`] is
//! the **audio** source identifier the server assigns on voice room entry, and every
//! client keeps a table of `ssrc` → person built out of it. A screen is not a
//! voice, so it gets [`ScreenId`], and nobody has to rewrite that table to make
//! room for something that was never a talker.
//!
//! # Byte order
//!
//! Big-endian, for the reason [`crate::media`] already gives: it is what every
//! other real-time media protocol writes, so a capture opened in Wireshark
//! reads the way an engineer expects.

use thiserror::Error;

use crate::ids::ScreenId;
use crate::version::PROTOCOL_VERSION;

/// Bytes of opening header before the first encoded picture.
pub const SCREEN_HEADER_LEN: usize = 11;

/// A versão **deste cabeçalho**, que não é a versão do protocolo.
///
/// # Por que ela existe, e o que ela conserta
///
/// O primeiro byte carregava [`PROTOCOL_VERSION`] e era conferido com a janela
/// do protocolo. Mas estes onze bytes **nunca mudaram**: a ordem dos campos é a
/// mesma desde que o compartilhamento de tela nasceu, e nenhuma subida de
/// protocolo mexeu neles — a 2 acrescentou uma mensagem de controle, e nada
/// aqui.
///
/// O efeito era o cabeçalho dizer «você precisa da versão N para ler estes onze
/// bytes» quando a verdade é «qualquer build que tenha esta funcionalidade sabe
/// lê-los». **Cada subida de protocolo levantava falsamente a barra do fluxo de
/// tela**, e quem transmitia da versão nova ficava ilegível para quem assistia
/// da anterior.
///
/// Relatado assim: «quem assiste com uma versão mais velha vê tela preta, sem
/// mensagem nenhuma, e a sessão morre em ~3 segundos sem dizer por quê» — e:
/// «se usuários de diferentes versões estão compartilhando a tela, a stream
/// cai».
///
/// **1, e sobe quando estes onze bytes mudarem** — não quando o protocolo
/// mudar. São duas perguntas diferentes e mereciam dois números.
pub const SCREEN_HEADER_VERSION: u8 = 1;

/// Todo número que um build já publicado escreveu neste primeiro byte.
///
/// Cada um deles escreveu o `PROTOCOL_VERSION` **dele**, porque era isso que
/// este campo carregava. Como o layout é o mesmo em todos, todos são legíveis, e
/// recusá-los seria recusar uma transmissão que se sabe ler.
///
/// O teto acompanha [`PROTOCOL_VERSION`] em vez de ser um número escrito: um
/// build que suba o protocolo continua aceitando o que os anteriores escreveram,
/// sem que ninguém precise lembrar de mexer aqui.
const fn cabecalho_legivel(version: u8) -> bool {
    version >= 1 && version <= PROTOCOL_VERSION
}

/// Longest side a transmission may declare, in pixels.
///
/// The spec's §6 item 10 puts "nothing above 1080p" among the things v1 does
/// not do, and §5 fixes the list the interface offers at 1080p, 720p and 540p.
/// 1920 is that ceiling expressed as a **side** rather than as a width, which
/// is what makes a monitor stood on end shareable at all: 1080 × 1920 is a
/// portrait screen, not an oversized one, and a rule written as "width ≤ 1920,
/// height ≤ 1080" would refuse exactly the person who bought the second
/// monitor to read code on.
pub const MAX_SCREEN_SIDE: u16 = 1920;

/// Most pixels a transmission may declare.
///
/// [`MAX_SCREEN_SIDE`] alone would admit 1920 × 1920, which is 1,8 times the
/// pixels of 1080p and outside everything §2 measured. Two limits rather than
/// one, because the pair says what a single number cannot: any shape, up to the
/// area the CPU measurements in `spikes/tela-no-codec` actually cover.
pub const MAX_SCREEN_PIXELS: u32 = 1920 * 1080;

/// What is being transmitted.
///
/// §4: the same choice for us and three different things for the three systems
/// — on Wayland the compositor picks and hands back only what the person chose,
/// so this is what the sender **learned** it got, not what it asked for.
///
/// The codes are append-only, like the variants of every enum on this wire: a
/// number is never reused and a new kind goes at the end, so a peer one version
/// older refuses what it does not know instead of drawing a window as a
/// monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenSource {
    /// A whole display.
    Monitor,
    /// A single window.
    Window,
}

impl ScreenSource {
    /// The byte this travels as.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Monitor => 0,
            Self::Window => 1,
        }
    }

    /// Reads the byte back, or `None` for a code this build does not know.
    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Monitor),
            1 => Some(Self::Window),
            _ => None,
        }
    }
}

/// What the pictures are encoded with.
///
/// One variant today, and the enum exists precisely because there will be a
/// second: §2 chooses H.264 baseline through `shiguredo_openh264` and names VP8
/// as **the successor**, for the day its binding leaves canary and somebody
/// accepts paying for nasm. A codec declared on the wire is what lets that day
/// arrive without a flag day — the receiver refuses a codec it cannot decode
/// instead of feeding a decoder bytes meant for another one.
///
/// Codes are append-only, for the reason [`ScreenSource`] gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenCodec {
    /// H.264 baseline profile. §2, and ADR 0008's family of bindings.
    H264Baseline,
}

impl ScreenCodec {
    /// The byte this travels as.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::H264Baseline => 0,
        }
    }

    /// Reads the byte back, or `None` for a codec this build cannot decode.
    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::H264Baseline),
            _ => None,
        }
    }
}

/// Why an opening header could not be read, or could not be written.
///
/// Enumerated, per `specs/02-protocolo.md`, and each one carries the numbers a
/// shell would need to write its own sentence — ADR 0012. The `Display` text is
/// for `tracing`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ScreenError {
    /// Fewer bytes than a header.
    #[error("stream opened with {len} bytes, shorter than the {SCREEN_HEADER_LEN}-byte header")]
    TooShort {
        /// Length received.
        len: usize,
    },

    /// A version this build does not speak.
    #[error("stream announces protocol {found}, this build implements {expected}")]
    UnsupportedVersion {
        /// Version in the header.
        found: u8,
        /// Version this build implements.
        expected: u8,
    },

    /// A source kind this build does not know.
    #[error("stream announces source kind {code}, which this build does not know")]
    UnknownSource {
        /// The code received.
        code: u8,
    },

    /// A codec this build cannot decode.
    #[error("stream announces codec {code}, which this build cannot decode")]
    UnknownCodec {
        /// The code received.
        code: u8,
    },

    /// A picture size outside what this build will carry.
    ///
    /// Carries both sides rather than a flag, because the two ways to fail it
    /// are different sentences: a side too long is "that screen is bigger than
    /// this product carries", and too many pixels is "that shape is".
    #[error("stream announces {width}×{height}, outside the {MAX_SCREEN_SIDE}-pixel side and {MAX_SCREEN_PIXELS}-pixel area this build carries")]
    BadResolution {
        /// Declared width.
        width: u16,
        /// Declared height.
        height: u16,
    },
}

/// The first eleven bytes of a screen transmission's stream.
///
/// ```text
/// ┌─────────┬────────────┬────────────┬───────────┬───────────┬────────────┐
/// │ ver (1) │ screen (4) │ source (1) │ codec (1) │ width (2) │ height (2) │
/// └─────────┴────────────┴────────────┴───────────┴───────────┴────────────┘
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenHeader {
    /// Protocol version. Always [`PROTOCOL_VERSION`] on the way out.
    pub version: u8,
    /// Which transmission this stream carries.
    ///
    /// Assigned by the server when the transmission is announced
    /// ([`crate::control::ServerMessage::ScreenShareStarted`]) and never chosen
    /// by the sender, which is the rule [`crate::ids::Ssrc`] already follows in
    /// `specs/08-seguranca.md`: an identifier a client picks is an identifier a
    /// client can pick somebody else's. It is here so that a receiver can bind
    /// the stream it was just offered to the transmission it was told about,
    /// and refuse a stream that matches nothing.
    pub screen: ScreenId,
    /// Monitor or window.
    pub source: ScreenSource,
    /// What the pictures are encoded with.
    pub codec: ScreenCodec,
    /// Width in pixels, as the sender is actually encoding it.
    ///
    /// A **number, not a name.** §5 fixes the list the interface offers at
    /// 1080p, 720p and 540p, and that list is a list of buttons: it changes the
    /// day somebody adds a fourth. What crosses the wire is what the encoder is
    /// doing, so no peer has to hold the same list of pretty names to
    /// understand a picture.
    ///
    /// It is also **not a promise**. §5: "a tela não promete a escolha" — the
    /// resolution holds and the frame rate cedes, between 5 and 30 per second,
    /// so this is what a receiver will get and the frame rate is not declared
    /// here at all. Writing a rate into a header written once would be writing
    /// down the one number the design says will move.
    pub width: u16,
    /// Height in pixels, as the sender is actually encoding it.
    pub height: u16,
}

impl ScreenHeader {
    /// Refuses a header this build will not carry.
    ///
    /// Asked on the way out as well as on the way in, so a size that came from
    /// a capture nobody bounded cannot travel further than one a receiver would
    /// accept.
    ///
    /// # Errors
    ///
    /// Returns [`ScreenError::BadResolution`] for a side of zero, a side over
    /// [`MAX_SCREEN_SIDE`], or an area over [`MAX_SCREEN_PIXELS`].
    pub fn check(&self) -> Result<(), ScreenError> {
        let bad = ScreenError::BadResolution {
            width: self.width,
            height: self.height,
        };
        // Zero is refused with the same breath as too large, for the reason
        // `check_server_icon` gives about a PNG declaring a side of nothing: it
        // is far more often a capture that failed than a deliberate choice, and
        // there is no screen behind it either way.
        if self.width == 0 || self.height == 0 {
            return Err(bad);
        }
        if self.width > MAX_SCREEN_SIDE || self.height > MAX_SCREEN_SIDE {
            return Err(bad);
        }
        if u32::from(self.width) * u32::from(self.height) > MAX_SCREEN_PIXELS {
            return Err(bad);
        }
        Ok(())
    }

    /// Writes the header into the first [`SCREEN_HEADER_LEN`] bytes of `out`.
    ///
    /// Returns the number of bytes written.
    ///
    /// # Errors
    ///
    /// Returns [`ScreenError::TooShort`] if `out` cannot hold a header, or
    /// [`ScreenError::BadResolution`] if [`Self::check`] refuses it.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, ScreenError> {
        self.check()?;
        let Some(slot) = out.get_mut(..SCREEN_HEADER_LEN) else {
            return Err(ScreenError::TooShort { len: out.len() });
        };
        let (version, rest) = slot.split_at_mut(1);
        let (screen, rest) = rest.split_at_mut(4);
        let (source, rest) = rest.split_at_mut(1);
        let (codec, rest) = rest.split_at_mut(1);
        let (width, height) = rest.split_at_mut(2);

        version.copy_from_slice(&[self.version]);
        screen.copy_from_slice(&self.screen.get().to_be_bytes());
        source.copy_from_slice(&[self.source.code()]);
        codec.copy_from_slice(&[self.codec.code()]);
        width.copy_from_slice(&self.width.to_be_bytes());
        height.copy_from_slice(&self.height.to_be_bytes());
        Ok(SCREEN_HEADER_LEN)
    }

    /// Reads the opening header off the front of a stream.
    ///
    /// Returns the header and whatever followed it in the same read, which is
    /// the first bytes of the first picture — a QUIC stream is a byte stream,
    /// so a receiver that read more than eleven bytes must not be made to
    /// forget the rest.
    ///
    /// Every read is [`slice::get`]. `specs/08-seguranca.md` names this crate
    /// as the surface that receives untrusted bytes, and a panicking indexation
    /// here would turn a truncated stream — which is just a laptop closing —
    /// into a dead process.
    ///
    /// # Errors
    ///
    /// Returns [`ScreenError`] for every malformed case. Never panics, whatever
    /// the bytes are.
    pub fn decode(bytes: &[u8]) -> Result<(Self, &[u8]), ScreenError> {
        let Some(header) = bytes.get(..SCREEN_HEADER_LEN) else {
            return Err(ScreenError::TooShort { len: bytes.len() });
        };
        let rest = bytes.get(SCREEN_HEADER_LEN..).unwrap_or_default();

        // The version before anything else, so a layout this build has never
        // seen is refused rather than read as one it has.
        //
        // **E não é mais `negotiate`.** Aquela é a janela do protocolo — N−1 —, e
        // usá-la aqui fazia o fluxo de tela herdar um limite que não é dele: um
        // build que subisse o protocolo passava a recusar o cabeçalho de quem
        // estava uma versão atrás, apesar de os onze bytes serem idênticos. Ver
        // [`SCREEN_HEADER_VERSION`].
        let version = header.first().copied().unwrap_or_default();
        if !cabecalho_legivel(version) {
            return Err(ScreenError::UnsupportedVersion {
                found: version,
                expected: SCREEN_HEADER_VERSION,
            });
        }

        let screen = ScreenId(read_u32(header, 1));
        let source_code = header.get(5).copied().unwrap_or_default();
        let Some(source) = ScreenSource::from_code(source_code) else {
            return Err(ScreenError::UnknownSource { code: source_code });
        };
        let codec_code = header.get(6).copied().unwrap_or_default();
        let Some(codec) = ScreenCodec::from_code(codec_code) else {
            return Err(ScreenError::UnknownCodec { code: codec_code });
        };

        let decoded = Self {
            version,
            screen,
            source,
            codec,
            width: read_u16(header, 7),
            height: read_u16(header, 9),
        };
        decoded.check()?;
        Ok((decoded, rest))
    }
}

/// Reads four big-endian bytes, or zero if they are not there.
///
/// The bounds can only fail if [`SCREEN_HEADER_LEN`] and the offsets disagree,
/// which would be a bug here rather than bad input — but this is the
/// untrusted-input surface, so it returns a value instead of panicking either
/// way. The same shape [`crate::media`] uses, and for the same reason.
fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    bytes
        .get(offset..offset + 4)
        .and_then(|slice| slice.try_into().ok())
        .map_or(0, u32::from_be_bytes)
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    bytes
        .get(offset..offset + 2)
        .and_then(|slice| slice.try_into().ok())
        .map_or(0, u16::from_be_bytes)
}

// ---------------------------------------------------------------------------
// Os quadros
// ---------------------------------------------------------------------------

/// Bytes de cabeçalho na frente de cada quadro.
///
/// Um byte de tipo e quatro de tamanho. Ao contrário do [`SCREEN_HEADER_LEN`],
/// que sai uma vez por transmissão, este sai trinta vezes por segundo: cinco
/// bytes são 150 B/s, que somem ao lado do teto, e valem o que compram — um
/// receptor que sabe onde o quadro acaba sem parsear NAL nenhuma.
pub const CABECALHO_DE_QUADRO_LEN: usize = 5;

/// O maior quadro que a linha 1 carrega, em bytes: **4 MiB** (D-fio-9,
/// 09/10/2026).
///
/// **Congelado com a linha 1** (ADR 0058): uma 1.x não pode mandar quadro maior
/// para uma 1.0, que o recusaria. Por isso o número cobre o maior quadro
/// legítimo possível, e não o maior que alguém mediu:
/// - a abertura aceita no máximo [`MAX_SCREEN_SIDE`] de lado e
///   [`MAX_SCREEN_PIXELS`] de área, e o codificador tem teto em 1080p;
/// - um quadro I420 cru de 1080p tem 3037,5 KiB, e o H.264 nunca precisa de
///   mais (o macrobloco PCM carrega o pixel cru), fora alguns bytes de
///   cabeçalho;
/// - os 512 KiB de antes cortavam quadros legítimos: com conteúdo realista e
///   tetos que o produto permite na rede local, um quadro passou de 512 KiB a
///   partir de 12,4 Mbps, e chegou a 973 KiB
///   (`docs/superpowers/sdd/2026-10-08-0.16/F4/medidas/idr-no-teto.md`).
///
/// Dois guardas o prendem: `o_teto_cabe_o_quadro_cru_da_maior_forma_que_a_abertura_aceita`,
/// aqui, e `seele-core/tests/teto_do_quadro_de_tela.rs`, contra as resoluções
/// do codificador.
///
/// O custo é a memória de um quadro em andamento por transmissão assistida, no
/// leitor que remonta ([`LeitorDeTela::bytes_reservados`]), e ela segue o que
/// chegou, e não o anúncio ([`RESERVA_INICIAL_DE_QUADRO`]). O [`Enquadramento`] conta bytes
/// e não guarda quadro.
pub const MAX_QUADRO_LEN: usize = 4 * 1024 * 1024;

/// Quanto um leitor reserva de uma vez para o corpo de um quadro: **64 KiB**.
///
/// Regra da orquestradora em 09/10/2026, sobre a revisão deste módulo: quem lê
/// reserva `min(tamanho, RESERVA_INICIAL_DE_QUADRO)` e cresce com o que chega,
/// e não o tamanho anunciado de uma vez. Com o teto em 4 MiB, reservar pelo
/// anúncio faria um cabeçalho de cinco bytes custar 4 MiB antes de o corpo
/// existir; assim, a memória acompanha os bytes recebidos.
pub const RESERVA_INICIAL_DE_QUADRO: usize = 64 * 1024;

/// O que vem dentro de um quadro do fluxo de tela.
///
/// O byte de tipo era zero ou um, imagem comum ou imagem-chave, e ganhou um
/// terceiro valor quando o som passou a viajar junto. Os códigos só crescem no
/// fim, como os de [`ScreenSource`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TipoDeQuadro {
    /// Imagem que depende do quadro anterior.
    Comum = 0,
    /// Imagem que basta a si mesma. É por onde quem chega no meio entra.
    Chave = 1,
    /// Som do que está sendo mostrado, em Opus.
    ///
    /// **Dentro do mesmo fluxo da imagem, e não ao lado dele.** Dois fluxos
    /// chegariam em ordens diferentes e precisariam de carimbo e de fila de
    /// alinhamento; no mesmo fluxo, a ordem de chegada é a ordem de saída. E o
    /// orçamento de banda da tela já mede o fluxo inteiro.
    Som = 2,
}

impl TipoDeQuadro {
    /// O byte que vai no fio.
    #[must_use]
    pub const fn byte(self) -> u8 {
        self as u8
    }

    /// O tipo que um byte nomeia, ou nada.
    ///
    /// `None` é um fluxo que não é o nosso, e quem lê o trata como fim — nunca
    /// como um quadro de tipo desconhecido a pular. Pular exigiria confiar no
    /// tamanho que veio junto, e o tamanho é a única coisa que um fluxo de lixo
    /// pode usar para pedir uma alocação absurda.
    #[must_use]
    pub const fn de_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::Comum),
            1 => Some(Self::Chave),
            2 => Some(Self::Som),
            _ => None,
        }
    }

    /// É imagem que basta a si mesma?
    #[must_use]
    pub const fn e_chave(self) -> bool {
        matches!(self, Self::Chave)
    }
}

/// Por que um quadro não pôde ser lido.
///
/// O nome não é `ErroDeTela` porque esse é o do `seele-core`, que também tem as
/// falhas de rede. Lá ele embrulha este; o `FimDaTela` do servidor o traduz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ErroDeQuadro {
    /// A abertura do fluxo não serve.
    #[error("screen header refused: {0}")]
    Cabecalho(#[from] ScreenError),
    /// Um byte de tipo que este build não conhece.
    #[error("frame type byte {byte} is not one this build knows")]
    TipoDesconhecido {
        /// O byte que veio.
        byte: u8,
    },
    /// Um quadro anunciado com zero bytes.
    #[error("a frame announced zero bytes")]
    QuadroVazio,
    /// Um quadro anunciado acima de [`MAX_QUADRO_LEN`].
    #[error("a frame announced {len} bytes, over the {MAX_QUADRO_LEN}-byte limit")]
    QuadroGrandeDemais {
        /// O tamanho anunciado.
        len: usize,
    },
}

/// O cabeçalho de um quadro: o tipo e o tamanho, em big-endian.
#[must_use]
pub const fn cabecalho_de_quadro(
    tipo: TipoDeQuadro,
    tamanho: u32,
) -> [u8; CABECALHO_DE_QUADRO_LEN] {
    let [a, b, c, d] = tamanho.to_be_bytes();
    [tipo.byte(), a, b, c, d]
}

/// Lê o cabeçalho de um quadro. **A única porta por onde o tamanho de um quadro
/// entra**: nas duas leituras deste módulo e no receptor do `seele-core`, que lê
/// do fluxo e não passa pelo [`LeitorDeTela`]. Tipo desconhecido, quadro vazio e
/// tamanho acima do teto saem daqui, antes de qualquer reserva.
///
/// # Errors
///
/// [`ErroDeQuadro::TipoDesconhecido`], [`ErroDeQuadro::QuadroVazio`] e
/// [`ErroDeQuadro::QuadroGrandeDemais`].
pub fn ler_cabecalho_de_quadro(
    cabecalho: &[u8; CABECALHO_DE_QUADRO_LEN],
) -> Result<(TipoDeQuadro, usize), ErroDeQuadro> {
    let [byte, a, b, c, d] = *cabecalho;
    let Some(tipo) = TipoDeQuadro::de_byte(byte) else {
        return Err(ErroDeQuadro::TipoDesconhecido { byte });
    };
    let tamanho = u32::from_be_bytes([a, b, c, d]) as usize;
    if tamanho == 0 {
        return Err(ErroDeQuadro::QuadroVazio);
    }
    if tamanho > MAX_QUADRO_LEN {
        return Err(ErroDeQuadro::QuadroGrandeDemais { len: tamanho });
    }
    Ok((tipo, tamanho))
}

/// Junta o cabeçalho de um quadro que pode chegar partido entre pedaços.
#[derive(Debug, Default)]
struct CabecalhoParcial {
    bytes: [u8; CABECALHO_DE_QUADRO_LEN],
    tem: usize,
}

impl CabecalhoParcial {
    /// Toma do começo de `pedaco` o que falta ao cabeçalho. Devolve quantos
    /// bytes tomou e, se ele ficou inteiro, os cinco bytes (e se recomeça).
    fn tomar(&mut self, pedaco: &[u8]) -> (usize, Option<[u8; CABECALHO_DE_QUADRO_LEN]>) {
        let anda = (CABECALHO_DE_QUADRO_LEN - self.tem).min(pedaco.len());
        if let (Some(destino), Some(origem)) = (
            self.bytes.get_mut(self.tem..self.tem + anda),
            pedaco.get(..anda),
        ) {
            destino.copy_from_slice(origem);
        }
        self.tem += anda;
        if self.tem < CABECALHO_DE_QUADRO_LEN {
            return (anda, None);
        }
        self.tem = 0;
        (anda, Some(self.bytes))
    }

    fn vazio(&self) -> bool {
        self.tem == 0
    }
}

/// Onde o enquadramento de um fluxo de tela está, quadro a quadro.
///
/// **Conta bytes e acha a porta de entrada**, e não remonta quadro: o servidor
/// repassa o pedaço como chegou (§3.3), e quem chega no meio de uma
/// transmissão entra no começo de um quadro-chave.
///
/// # Por que contar bytes, e não olhar o primeiro byte do pedaço
///
/// Um pedaço não é um quadro: é o que uma leitura devolveu, e ela corta onde o
/// I/O corta. Um pedaço pode carregar dois quadros inteiros, ou só a metade de
/// um. Ler o primeiro byte de cada pedaço como se fosse sempre um byte de tipo
/// lê lixo nos dois casos — o meio de um quadro H.264 Annex-B está cheio de
/// `0x00` e `0x01`. Contar bytes é a única forma de saber onde um cabeçalho de
/// verdade começa.
#[derive(Debug, Default)]
pub struct Enquadramento {
    /// Quantos bytes faltam do quadro que está passando.
    restam: usize,
    /// O cabeçalho do próximo quadro, enquanto ele chega partido.
    cabecalho: CabecalhoParcial,
}

impl Enquadramento {
    /// Um enquadramento no começo dos quadros, esperando o primeiro cabeçalho.
    #[must_use]
    pub fn novo() -> Self {
        Self::default()
    }

    /// Passa um pedaço pelo enquadramento e diz onde alguém pode entrar.
    ///
    /// Devolve o deslocamento, dentro deste pedaço, do primeiro cabeçalho de
    /// **quadro-chave que começa e termina neste mesmo pedaço**. Um cabeçalho
    /// partido entre dois pedaços não vira porta: a primeira metade já pode ter
    /// saído para quem estava do outro lado, e quem entrasse agora leria a
    /// segunda metade como se fosse a primeira.
    ///
    /// # Errors
    ///
    /// [`ErroDeQuadro`] para um tipo desconhecido, um quadro vazio ou um
    /// tamanho acima de [`MAX_QUADRO_LEN`].
    pub fn entrada(&mut self, bytes: &[u8]) -> Result<Option<usize>, ErroDeQuadro> {
        let mut entrada = None;
        let mut i = 0;
        while let Some(resto) = bytes.get(i..).filter(|r| !r.is_empty()) {
            if self.restam > 0 {
                let anda = self.restam.min(resto.len());
                self.restam -= anda;
                i += anda;
                continue;
            }
            let comeca_aqui = self.cabecalho.vazio();
            let inicio = i;
            let (anda, inteiro) = self.cabecalho.tomar(resto);
            i += anda;
            let Some(cabecalho) = inteiro else {
                break;
            };
            let (tipo, tamanho) = ler_cabecalho_de_quadro(&cabecalho)?;
            self.restam = tamanho;
            // **A porta de entrada é um quadro-chave de imagem, e só ele.** Um
            // quadro de som não começa nada.
            if tipo.e_chave() && comeca_aqui && entrada.is_none() {
                entrada = Some(inicio);
            }
        }
        Ok(entrada)
    }
}

/// O que o [`LeitorDeTela`] leu.
///
/// O nome não é `Lido` porque esse é o de [`crate::control::Lido`], que o
/// celular importa junto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LidoDaTela {
    /// A abertura do fluxo, uma vez, antes de tudo.
    Abertura(ScreenHeader),
    /// Um quadro inteiro.
    Quadro {
        /// Imagem comum, imagem-chave ou som.
        tipo: TipoDeQuadro,
        /// O corpo, sem o cabeçalho.
        bytes: Vec<u8>,
    },
}

/// Onde o leitor está.
#[derive(Debug)]
enum Estado {
    /// Juntando os onze bytes da abertura.
    Abertura {
        bytes: [u8; SCREEN_HEADER_LEN],
        tem: usize,
    },
    /// Entre quadros, ou no meio de um.
    Quadros {
        cabecalho: CabecalhoParcial,
        /// O quadro em andamento: o tipo, os bytes até aqui e o tamanho
        /// anunciado.
        quadro: Option<(TipoDeQuadro, Vec<u8>, usize)>,
    },
    /// Um erro fechou o leitor.
    Fechado(ErroDeQuadro),
}

/// Lê um fluxo de tela, a partir do byte depois do tipo do fluxo.
///
/// Quem aceita o fluxo despacha pelo tipo (`fluxo:tipo:tela`); daqui em
/// diante vêm a abertura e os quadros. É o caminho de
/// `Recepcao::do_fluxo_ja_tipado`, no `seele-core`.
///
/// # Por que ele confere no `empurrar`, e não no `proximo`
///
/// Ao contrário do [`crate::quadro::Desenquadrador`], que confere quando o
/// quadro é pedido, este confere o cabeçalho de cada quadro **assim que ele
/// chega**, e só então reserva o corpo. É o que limita a memória a um quadro em
/// andamento: um tamanho absurdo é recusado antes de qualquer reserva, e nunca
/// fica um corpo inteiro guardado à espera de alguém pedi-lo para descobrir que
/// ele não podia existir.
///
/// O que fica pronto espera em ordem até o [`LeitorDeTela::proximo`]. Um pedaço
/// de N bytes produz no máximo N ÷ 6 quadros (o menor quadro tem um byte de
/// corpo), então a fila cresce com o pedaço, e não com o que um par anuncia.
///
/// **Um erro fecha o leitor.** Um fluxo de lixo não pede uma segunda leitura, e
/// todo `empurrar` depois dele devolve o mesmo erro.
#[derive(Debug)]
pub struct LeitorDeTela {
    estado: Estado,
    prontos: std::collections::VecDeque<LidoDaTela>,
    /// A maior reserva que o leitor já fez para um quadro. Ver [`reservar`].
    maior_reserva: usize,
    /// Quantas vezes um corpo cresceu: o guarda de que ele dobra, e não cresce
    /// pedaço a pedaço.
    crescimentos: usize,
}

/// **A única reserva de corpo do leitor**, e ela anota o maior pedido.
///
/// O guarda da ordem «conferir, e só então reservar» olha este pico: uma
/// reserva feita antes da conferência é soltada quando o erro fecha o leitor, e
/// um guarda que olhasse só a reserva de agora não a veria. (Um alocador
/// contador veria, mas o workspace proíbe `unsafe`, e um `forbid` não se
/// afrouxa nem num teste.)
fn reservar(maior_reserva: &mut usize, tamanho: usize) -> Vec<u8> {
    *maior_reserva = (*maior_reserva).max(tamanho);
    Vec::with_capacity(tamanho)
}

/// Faz o corpo em andamento caber `precisa` bytes: dobra a reserva, sem passar
/// do `anunciado`, e anota o maior pedido, como [`reservar`].
///
/// **Dobrar, e não crescer o exato de cada pedaço:** com o exato, um fluxo que
/// pinga byte a byte custaria uma realocação por byte, e remontar um quadro
/// viraria trabalho quadrático. Dobrando, a memória fica em no máximo o dobro do
/// que chegou (ou a [`RESERVA_INICIAL_DE_QUADRO`] do começo).
fn crescer(
    maior_reserva: &mut usize,
    crescimentos: &mut usize,
    corpo: &mut Vec<u8>,
    precisa: usize,
    anunciado: usize,
) {
    if precisa <= corpo.capacity() {
        return;
    }
    *crescimentos += 1;
    let nova = precisa
        .max(corpo.capacity().saturating_mul(2))
        .min(anunciado);
    corpo.reserve_exact(nova - corpo.len());
    *maior_reserva = (*maior_reserva).max(nova);
}

impl Default for LeitorDeTela {
    fn default() -> Self {
        Self::novo()
    }
}

impl LeitorDeTela {
    /// Um leitor no começo do fluxo, esperando a abertura.
    #[must_use]
    pub fn novo() -> Self {
        Self {
            estado: Estado::Abertura {
                bytes: [0; SCREEN_HEADER_LEN],
                tem: 0,
            },
            prontos: std::collections::VecDeque::new(),
            maior_reserva: 0,
            crescimentos: 0,
        }
    }

    /// Passa um pedaço do fluxo pelo leitor.
    ///
    /// # Errors
    ///
    /// [`ErroDeQuadro`] para uma abertura que não serve, um tipo desconhecido,
    /// um quadro vazio ou um tamanho acima de [`MAX_QUADRO_LEN`]. O leitor fica
    /// fechado, e o que já estava pronto continua saindo pelo `proximo`.
    pub fn empurrar(&mut self, pedaco: &[u8]) -> Result<(), ErroDeQuadro> {
        let mut i = 0;
        while let Some(resto) = pedaco.get(i..).filter(|r| !r.is_empty()) {
            match &mut self.estado {
                Estado::Fechado(erro) => return Err(*erro),
                Estado::Abertura { bytes, tem } => {
                    let anda = (SCREEN_HEADER_LEN - *tem).min(resto.len());
                    if let (Some(destino), Some(origem)) =
                        (bytes.get_mut(*tem..*tem + anda), resto.get(..anda))
                    {
                        destino.copy_from_slice(origem);
                    }
                    *tem += anda;
                    i += anda;
                    if *tem == SCREEN_HEADER_LEN {
                        match ScreenHeader::decode(bytes.as_slice()) {
                            Ok((abertura, _)) => {
                                self.prontos.push_back(LidoDaTela::Abertura(abertura));
                                self.estado = Estado::Quadros {
                                    cabecalho: CabecalhoParcial::default(),
                                    quadro: None,
                                };
                            }
                            Err(erro) => return Err(self.fechar(erro.into())),
                        }
                    }
                }
                Estado::Quadros { cabecalho, quadro } => {
                    if let Some((tipo, bytes, tamanho)) = quadro {
                        let anda = (*tamanho - bytes.len()).min(resto.len());
                        crescer(
                            &mut self.maior_reserva,
                            &mut self.crescimentos,
                            bytes,
                            bytes.len() + anda,
                            *tamanho,
                        );
                        bytes.extend_from_slice(resto.get(..anda).unwrap_or_default());
                        i += anda;
                        if bytes.len() == *tamanho {
                            let tipo = *tipo;
                            let bytes = std::mem::take(bytes);
                            *quadro = None;
                            self.prontos.push_back(LidoDaTela::Quadro { tipo, bytes });
                        }
                        continue;
                    }
                    let (anda, inteiro) = cabecalho.tomar(resto);
                    i += anda;
                    let Some(inteiro) = inteiro else {
                        continue;
                    };
                    match ler_cabecalho_de_quadro(&inteiro) {
                        // **Só agora**, com o tamanho conferido, o corpo é
                        // reservado, e só o começo dele: o resto cresce com o
                        // que chegar ([`RESERVA_INICIAL_DE_QUADRO`]).
                        Ok((tipo, tamanho)) => {
                            let inicio = reservar(
                                &mut self.maior_reserva,
                                tamanho.min(RESERVA_INICIAL_DE_QUADRO),
                            );
                            *quadro = Some((tipo, inicio, tamanho));
                        }
                        Err(erro) => return Err(self.fechar(erro)),
                    }
                }
            }
        }
        match &self.estado {
            Estado::Fechado(erro) => Err(*erro),
            _ => Ok(()),
        }
    }

    /// O próximo que ficou pronto, em ordem: a abertura primeiro, os quadros
    /// depois.
    pub fn proximo(&mut self) -> Option<LidoDaTela> {
        self.prontos.pop_front()
    }

    /// Quantos bytes o leitor segura para o quadro em andamento: a memória que
    /// uma transmissão assistida custa aqui, para o diagnóstico.
    #[must_use]
    pub fn bytes_reservados(&self) -> usize {
        match &self.estado {
            Estado::Quadros {
                quadro: Some((_, bytes, _)),
                ..
            } => bytes.capacity(),
            _ => 0,
        }
    }

    /// O maior quadro que este fluxo pediu até agora, em bytes: para o
    /// diagnóstico do custo de uma transmissão, e para o guarda da ordem.
    #[must_use]
    pub fn maior_reserva(&self) -> usize {
        self.maior_reserva
    }

    fn fechar(&mut self, erro: ErroDeQuadro) -> ErroDeQuadro {
        self.estado = Estado::Fechado(erro);
        erro
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> ScreenHeader {
        ScreenHeader {
            version: SCREEN_HEADER_VERSION,
            screen: ScreenId(0x00C0_FFEE),
            source: ScreenSource::Window,
            codec: ScreenCodec::H264Baseline,
            width: 1280,
            height: 720,
        }
    }

    #[test]
    fn the_opening_header_is_exactly_eleven_bytes() {
        // Once per transmission rather than fifty times a second, so the number
        // is not a budget the way `media::HEADER_LEN` is. It is pinned because
        // the receiver reads a fixed array off the stream: a field added here
        // without moving this channel reads the first bytes of the first picture
        // as a resolution.
        assert_eq!(SCREEN_HEADER_LEN, 1 + 4 + 1 + 1 + 2 + 2);

        let mut buffer = [0_u8; 32];
        assert_eq!(header().encode(&mut buffer), Ok(SCREEN_HEADER_LEN));
    }

    #[test]
    fn a_header_round_trips_and_keeps_what_followed_it() {
        // The tail matters: a QUIC stream is a byte stream, and a receiver that
        // read the header and the start of the first picture in one syscall
        // must not be made to forget the second half.
        let mut buffer = [0_u8; 32];
        header().encode(&mut buffer).unwrap();
        buffer[SCREEN_HEADER_LEN] = 0x77;

        let (decoded, rest) = ScreenHeader::decode(&buffer).unwrap();
        assert_eq!(decoded, header());
        assert_eq!(rest.first(), Some(&0x77));
    }

    #[test]
    fn the_wire_layout_is_big_endian_and_in_the_documented_order() {
        // Pinned against the byte level, not just against the decoder. A round
        // trip alone would pass with width and height swapped, and the first
        // person to notice would be somebody watching a stretched screen.
        let mut buffer = [0_u8; SCREEN_HEADER_LEN];
        header().encode(&mut buffer).unwrap();

        assert_eq!(
            buffer[0], SCREEN_HEADER_VERSION,
            "version first — a **deste cabeçalho**, e não a do protocolo"
        );
        assert_eq!(
            &buffer[1..5],
            &[0x00, 0xC0, 0xFF, 0xEE],
            "screen, big-endian"
        );
        assert_eq!(buffer[5], 1, "source: window");
        assert_eq!(buffer[6], 0, "codec: H.264 baseline");
        assert_eq!(&buffer[7..9], &[0x05, 0x00], "width");
        assert_eq!(&buffer[9..11], &[0x02, 0xD0], "height");
    }

    #[test]
    fn a_truncated_header_is_rejected_not_misread() {
        // The laptop that closed mid-transmission. `specs/08-seguranca.md`
        // names this surface for fuzzing: every one of these must be a refusal
        // and none of them a panic.
        let mut whole = [0_u8; SCREEN_HEADER_LEN];
        header().encode(&mut whole).unwrap();
        for len in 0..SCREEN_HEADER_LEN {
            assert_eq!(
                ScreenHeader::decode(&whole[..len]),
                Err(ScreenError::TooShort { len }),
                "accepted a {len}-byte opening"
            );
        }
    }

    #[test]
    fn a_foreign_version_is_refused_with_the_numbers() {
        // **Este teste dizia que a janela era a mesma do controle**, «decidida
        // num lugar só em vez de duas com uma diferença que ninguém nota até um
        // release». A intenção era boa e o efeito foi o oposto: o fluxo de tela
        // herdou um limite que não é dele, e quem transmitia da versão nova
        // ficava ilegível para quem assistia da anterior — apesar de os onze
        // bytes serem idênticos nas duas. Ver `SCREEN_HEADER_VERSION`.
        //
        // O que se confere agora é o que este cabeçalho de fato sabe: um número
        // que build nenhum escreveu.
        let mut buffer = [0_u8; SCREEN_HEADER_LEN];
        header().encode(&mut buffer).unwrap();
        buffer[0] = PROTOCOL_VERSION.wrapping_add(7);

        assert_eq!(
            ScreenHeader::decode(&buffer),
            Err(ScreenError::UnsupportedVersion {
                found: PROTOCOL_VERSION.wrapping_add(7),
                expected: SCREEN_HEADER_VERSION,
            })
        );

        // E o oposto, que é a razão de tudo isto: o número que **cada** build já
        // publicado escreveu aqui continua sendo lido. Recusá-los seria recusar
        // uma transmissão que se sabe ler.
        for escrito in 1..=PROTOCOL_VERSION {
            let mut de_outra_versao = [0_u8; SCREEN_HEADER_LEN];
            header().encode(&mut de_outra_versao).unwrap();
            de_outra_versao[0] = escrito;
            assert!(
                ScreenHeader::decode(&de_outra_versao).is_ok(),
                "o cabeçalho escrito por um build v{escrito} foi recusado, e os \
                 onze bytes dele são os mesmos"
            );
        }

        // Zero nunca foi escrito por ninguém, e é o que um buffer vazio dá.
        let mut zerado = [0_u8; SCREEN_HEADER_LEN];
        header().encode(&mut zerado).unwrap();
        zerado[0] = 0;
        assert!(ScreenHeader::decode(&zerado).is_err());
    }

    #[test]
    fn an_unknown_source_or_codec_is_refused_rather_than_guessed() {
        // The whole reason both travel as codes with an append-only meaning. A
        // build that met a codec it does not have and started its H.264 decoder
        // anyway would draw garbage and blame the network; §2 names VP8 as the
        // successor, so the second codec is a matter of when.
        let mut buffer = [0_u8; SCREEN_HEADER_LEN];
        header().encode(&mut buffer).unwrap();

        let mut strange_source = buffer;
        strange_source[5] = 9;
        assert_eq!(
            ScreenHeader::decode(&strange_source),
            Err(ScreenError::UnknownSource { code: 9 })
        );

        let mut strange_codec = buffer;
        strange_codec[6] = 4;
        assert_eq!(
            ScreenHeader::decode(&strange_codec),
            Err(ScreenError::UnknownCodec { code: 4 })
        );
    }

    #[test]
    fn a_screen_stood_on_end_is_carried_and_not_refused() {
        // The trap in writing the ceiling as a width. 1080 × 1920 is a portrait
        // monitor — the second screen somebody bought to read code on — and a
        // rule of "width ≤ 1920, height ≤ 1080" refuses exactly that person
        // while accepting every landscape screen, which is a bug that looks
        // like a policy.
        let portrait = ScreenHeader {
            width: 1080,
            height: 1920,
            ..header()
        };
        let mut buffer = [0_u8; SCREEN_HEADER_LEN];
        portrait.encode(&mut buffer).unwrap();
        assert_eq!(ScreenHeader::decode(&buffer).unwrap().0, portrait);
    }

    #[test]
    fn a_resolution_outside_the_ceiling_is_refused_in_both_directions() {
        // §6 item 10 puts anything above 1080p outside v1, and §2 measured the
        // CPU only up to it. Refused on the way out so we cannot send one, and
        // on the way in so a peer cannot skip the check by writing the eleven
        // bytes by hand — the same pair every bounded field in `control` gets.
        for (width, height) in [
            (0, 720),
            (1280, 0),
            (1921, 1080),
            (1080, 1921),
            // Both sides inside the ceiling and 1,8 times the pixels of 1080p:
            // the case the side limit alone cannot refuse.
            (1920, 1920),
        ] {
            let outside = ScreenHeader {
                width,
                height,
                ..header()
            };
            let mut buffer = [0_u8; SCREEN_HEADER_LEN];
            assert_eq!(
                outside.encode(&mut buffer),
                Err(ScreenError::BadResolution { width, height }),
                "encoded {width}×{height}"
            );

            // And now the way a hostile peer would build it, skipping the
            // encoder entirely.
            let mut frame = [0_u8; SCREEN_HEADER_LEN];
            header().encode(&mut frame).unwrap();
            frame[7..9].copy_from_slice(&width.to_be_bytes());
            frame[9..11].copy_from_slice(&height.to_be_bytes());
            assert_eq!(
                ScreenHeader::decode(&frame),
                Err(ScreenError::BadResolution { width, height }),
                "decoded a hand-rolled {width}×{height}"
            );
        }
    }

    #[test]
    fn every_offered_resolution_fits_the_header() {
        // §5 fixes the list at 1080p, 720p and 540p, and it travels as numbers
        // precisely so this list can change without the wire changing. This
        // test is the list of today meeting the wire of today; a fourth entry
        // added tomorrow needs no byte moved, which is the property being
        // recorded.
        for (width, height) in [(1920, 1080), (1280, 720), (960, 540)] {
            let offered = ScreenHeader {
                width,
                height,
                ..header()
            };
            let mut buffer = [0_u8; SCREEN_HEADER_LEN];
            offered.encode(&mut buffer).unwrap();
            assert_eq!(ScreenHeader::decode(&buffer).unwrap().0, offered);
        }
    }

    #[test]
    fn encoding_into_a_small_buffer_fails_instead_of_truncating() {
        let mut buffer = [0_u8; 4];
        assert_eq!(
            header().encode(&mut buffer),
            Err(ScreenError::TooShort { len: 4 })
        );
    }

    #[test]
    fn a_screen_is_not_an_audio_source() {
        // §3.6, and the one thing it puts in bold. `Ssrc` is assigned on voice room
        // entry and every client keeps a table of `ssrc` → person built out of
        // it; passing one where the other belongs is a bug the compiler catches
        // for free, and this test documents the guarantee — `ScreenId(1)` does
        // not compile where an `Ssrc` is wanted.
        fn takes_screen(id: ScreenId) -> u32 {
            id.get()
        }
        assert_eq!(takes_screen(header().screen), 0x00C0_FFEE);
    }
}

#[cfg(test)]
mod entre_versoes {
    use super::*;

    /// **O cabeçalho de um build mais novo continua legível num mais velho.**
    ///
    /// É o caso do relato, encenado com os números em vez de com dois builds:
    /// «se usuários de diferentes versões estão compartilhando a tela, a stream
    /// cai», e «quem assiste com uma versão mais velha vê tela preta».
    ///
    /// A causa era o primeiro byte carregar o `PROTOCOL_VERSION` de quem
    /// transmite e ser conferido com a janela do protocolo. Um transmissor v3
    /// escrevia 3; um espectador v2 fazia `negotiate(3)`, via um número acima do
    /// dele, e recusava — apesar de os onze bytes serem idênticos nas duas
    /// versões, porque este layout nunca mudou.
    ///
    /// Agora quem transmite escreve [`SCREEN_HEADER_VERSION`], que é o número
    /// **deste layout**, e ele é 1 desde que o compartilhamento de tela nasceu.
    /// Todo build já publicado sabe ler 1.
    #[test]
    fn quem_transmite_escreve_a_versao_do_layout_e_nao_a_do_protocolo() {
        let mut buffer = [0_u8; SCREEN_HEADER_LEN];
        ScreenHeader {
            version: SCREEN_HEADER_VERSION,
            screen: crate::ids::ScreenId(7),
            source: ScreenSource::Monitor,
            codec: ScreenCodec::H264Baseline,
            width: 1920,
            height: 1080,
        }
        .encode(&mut buffer)
        .expect("o cabeçalho cabe");

        assert_eq!(
            buffer[0], 1,
            "o primeiro byte voltou a ser a versão do protocolo, e com ela volta \
             a barra que este layout nunca precisou"
        );

        // **Quem já está no mundo consegue ler o 1, e a conta é a janela deles.**
        //
        // Todo build publicado confere este byte com `negotiate`, cuja janela é
        // N−1: um build de protocolo `v` aceita `v-1..=v`. Então o 1 passa em
        // todos com `v <= 2` — e os publicados são a v1 (até a 0.9.9) e a v2
        // (da 0.10.0 em diante). É a conta inteira, e ela é aritmética e não
        // suposição.
        // A janela daqueles builds era 1 — `COMPATIBILITY_WINDOW`, que saiu
        // com o Servidor 1.0 —, e a conta é sobre eles, então ela fica escrita
        // aqui como número deles.
        const JANELA_DAQUELES_BUILDS: u8 = 1;
        for publicado in 1_u8..=2 {
            let mais_velho = publicado.saturating_sub(JANELA_DAQUELES_BUILDS);
            assert!(
                (mais_velho..=publicado).contains(&SCREEN_HEADER_VERSION),
                "um build de protocolo v{publicado} recusaria o cabeçalho que \
                 esta versão escreve"
            );
        }

        // **E o build de agora o lê pela conferência própria, não pela janela.**
        //
        // Esta é a parte que só vale porque as duas coisas saem juntas: um v3
        // com a conferência antiga recusaria o 1, porque a janela dele começa em
        // 2. Nenhum v3 assim chegou a existir — a subida do protocolo e esta
        // troca são o mesmo commit, e é por isso que elas não podem se separar.
        let mut de_volta = [0_u8; SCREEN_HEADER_LEN];
        de_volta.copy_from_slice(&buffer);
        assert!(
            ScreenHeader::decode(&de_volta).is_ok(),
            "este build não lê o cabeçalho que ele mesmo escreve"
        );
        assert!(
            crate::version::confere_o_carimbo(SCREEN_HEADER_VERSION).is_err(),
            "a janela do protocolo passou a aceitar o 1 de novo; se isso for \
             verdade, este teste perdeu o que ele estava guardando"
        );
    }
}

#[cfg(test)]
mod os_quadros {
    //! Os quadros que seguem a abertura: o [`Enquadramento`], que conta bytes,
    //! e o [`LeitorDeTela`], que remonta. Os oito testes do `Enquadramento`
    //! vêm do gêmeo em `seele-core/src/tela.rs` (módulo `o_enquadramento`),
    //! com o [`ErroDeQuadro`] no lugar do `ErroDeTela` de lá.

    use super::*;

    /// Um quadro como o fio o leva: tipo, tamanho, corpo.
    fn quadro(tipo: TipoDeQuadro, tamanho: usize) -> Vec<u8> {
        let mut bytes = cabecalho_de_quadro(tipo, u32::try_from(tamanho).unwrap()).to_vec();
        bytes.extend(std::iter::repeat_n(0xAB, tamanho));
        bytes
    }

    fn abertura() -> Vec<u8> {
        let mut bytes = [0_u8; SCREEN_HEADER_LEN];
        ScreenHeader {
            version: SCREEN_HEADER_VERSION,
            screen: crate::ids::ScreenId(5),
            source: ScreenSource::Monitor,
            codec: ScreenCodec::H264Baseline,
            width: 1920,
            height: 1080,
        }
        .encode(&mut bytes)
        .unwrap();
        bytes.to_vec()
    }

    /// Tudo o que o leitor devolveu até agora.
    fn lidos(leitor: &mut LeitorDeTela) -> Vec<LidoDaTela> {
        std::iter::from_fn(|| leitor.proximo()).collect()
    }

    // ---- o cabeçalho de um quadro ----

    #[test]
    fn o_cabecalho_de_um_quadro_e_o_tipo_e_o_tamanho_em_big_endian() {
        assert_eq!(
            cabecalho_de_quadro(TipoDeQuadro::Comum, 0x0102_0304),
            [0, 1, 2, 3, 4]
        );
        assert_eq!(cabecalho_de_quadro(TipoDeQuadro::Chave, 9), [1, 0, 0, 0, 9]);
        assert_eq!(cabecalho_de_quadro(TipoDeQuadro::Som, 2), [2, 0, 0, 0, 2]);
    }

    #[test]
    fn os_bytes_de_tipo_nao_mudam() {
        assert_eq!(TipoDeQuadro::Comum.byte(), 0);
        assert_eq!(TipoDeQuadro::Chave.byte(), 1);
        assert_eq!(TipoDeQuadro::Som.byte(), 2);
        for byte in 0..=u8::MAX {
            assert_eq!(
                TipoDeQuadro::de_byte(byte).map(TipoDeQuadro::byte),
                (byte <= 2).then_some(byte)
            );
        }
    }

    /// **O teto cabe o maior quadro que o fio admite** (D-fio-9, 09/10/2026).
    ///
    /// O pior caso é o quadro I420 cru da maior forma que a abertura aceita
    /// ([`MAX_SCREEN_SIDE`] e [`MAX_SCREEN_PIXELS`]), com o croma arredondado
    /// para cima numa forma ímpar, mais o cabeçalho. O H.264 nunca precisa de
    /// mais: o macrobloco PCM carrega o pixel cru. O guarda contra as
    /// resoluções do codificador mora em
    /// `seele-core/tests/teto_do_quadro_de_tela.rs`, onde o `seele-video` é
    /// alcançável.
    #[test]
    fn o_teto_cabe_o_quadro_cru_da_maior_forma_que_a_abertura_aceita() {
        let lado = usize::from(MAX_SCREEN_SIDE);
        let area = MAX_SCREEN_PIXELS as usize;
        let pior = (1..=lado)
            .map(|largura| {
                let altura = (area / largura).min(lado);
                largura * altura + 2 * largura.div_ceil(2) * altura.div_ceil(2)
            })
            .max()
            .unwrap();
        assert!(
            MAX_QUADRO_LEN >= pior + CABECALHO_DE_QUADRO_LEN,
            "o teto do quadro ({MAX_QUADRO_LEN}) corta o quadro cru de {pior} bytes que a \
             abertura admite"
        );
    }

    // ---- o Enquadramento, portado do seele-core ----

    #[test]
    fn a_porta_de_entrada_e_o_comeco_de_um_quadro_chave() {
        let mut enq = Enquadramento::novo();
        let mut fluxo = quadro(TipoDeQuadro::Comum, 10);
        let onde = fluxo.len();
        fluxo.extend(quadro(TipoDeQuadro::Chave, 20));
        fluxo.extend(quadro(TipoDeQuadro::Comum, 5));
        assert_eq!(enq.entrada(&fluxo), Ok(Some(onde)));
    }

    #[test]
    fn sem_quadro_chave_nao_ha_porta() {
        let mut enq = Enquadramento::novo();
        let mut fluxo = quadro(TipoDeQuadro::Comum, 10);
        fluxo.extend(quadro(TipoDeQuadro::Comum, 20));
        assert_eq!(enq.entrada(&fluxo), Ok(None));
    }

    #[test]
    fn um_cabecalho_partido_ao_meio_nao_vira_porta() {
        let mut enq = Enquadramento::novo();
        let chave = quadro(TipoDeQuadro::Chave, 20);
        assert_eq!(enq.entrada(chave.get(..3).unwrap_or_default()), Ok(None));
        assert_eq!(enq.entrada(chave.get(3..).unwrap_or_default()), Ok(None));
        assert_eq!(enq.entrada(&quadro(TipoDeQuadro::Chave, 8)), Ok(Some(0)));
    }

    #[test]
    fn o_enquadramento_atravessa_pedacos_de_qualquer_tamanho() {
        let mut fluxo = quadro(TipoDeQuadro::Comum, 300);
        let onde = fluxo.len();
        fluxo.extend(quadro(TipoDeQuadro::Chave, 100));
        let mut inteiro = Enquadramento::novo();
        assert_eq!(inteiro.entrada(&fluxo), Ok(Some(onde)));

        let mut picado = Enquadramento::novo();
        let mut achado = None;
        for (i, byte) in fluxo.iter().enumerate() {
            if let Ok(Some(_)) = picado.entrada(&[*byte]) {
                achado = Some(i);
            }
        }
        assert_eq!(achado, None);
    }

    #[test]
    fn um_tamanho_impossivel_encerra_o_fluxo() {
        let mut enq = Enquadramento::novo();
        let tamanho = u32::try_from(MAX_QUADRO_LEN).unwrap() + 1;
        assert_eq!(
            enq.entrada(&cabecalho_de_quadro(TipoDeQuadro::Comum, tamanho)),
            Err(ErroDeQuadro::QuadroGrandeDemais {
                len: tamanho as usize
            })
        );
    }

    #[test]
    fn um_quadro_vazio_encerra_o_fluxo() {
        let mut enq = Enquadramento::novo();
        assert_eq!(
            enq.entrada(&[TipoDeQuadro::Comum.byte(), 0, 0, 0, 0]),
            Err(ErroDeQuadro::QuadroVazio)
        );
    }

    #[test]
    fn um_byte_de_tipo_que_este_fluxo_nao_conhece_o_encerra() {
        let byte = 3_u8;
        let mut enq = Enquadramento::novo();
        assert_eq!(
            enq.entrada(&[byte, 0, 0, 0, 8]),
            Err(ErroDeQuadro::TipoDesconhecido { byte })
        );
    }

    #[test]
    fn um_quadro_de_som_atravessa_e_nao_e_porta_de_entrada() {
        let mut enq = Enquadramento::novo();
        assert_eq!(enq.entrada(&quadro(TipoDeQuadro::Som, 4)), Ok(None));

        let mut enq = Enquadramento::novo();
        let mut fluxo = quadro(TipoDeQuadro::Som, 4);
        let onde = fluxo.len();
        fluxo.extend(quadro(TipoDeQuadro::Chave, 20));
        assert_eq!(enq.entrada(&fluxo), Ok(Some(onde)));
    }

    // ---- o LeitorDeTela ----

    /// A abertura e três quadros, um de cada tipo, com corpos diferentes.
    fn um_fluxo() -> (Vec<u8>, Vec<LidoDaTela>) {
        let (cabecalho, _) = ScreenHeader::decode(&abertura()).unwrap();
        let corpos = [
            (TipoDeQuadro::Chave, vec![0, 0, 0, 1, 0x65, 7, 7]),
            (TipoDeQuadro::Som, vec![0xFC, 0xFF]),
            (TipoDeQuadro::Comum, vec![0, 0, 0, 1, 0x41]),
        ];
        let mut bytes = abertura();
        let mut esperado = vec![LidoDaTela::Abertura(cabecalho)];
        for (tipo, corpo) in corpos {
            bytes.extend_from_slice(&cabecalho_de_quadro(
                tipo,
                u32::try_from(corpo.len()).unwrap(),
            ));
            bytes.extend_from_slice(&corpo);
            esperado.push(LidoDaTela::Quadro { tipo, bytes: corpo });
        }
        (bytes, esperado)
    }

    #[test]
    fn o_leitor_devolve_a_abertura_e_os_quadros_em_ordem() {
        let (bytes, esperado) = um_fluxo();
        let mut leitor = LeitorDeTela::novo();
        leitor.empurrar(&bytes).unwrap();
        assert_eq!(lidos(&mut leitor), esperado);
        assert_eq!(leitor.proximo(), None);
    }

    #[test]
    fn o_leitor_da_o_mesmo_resultado_em_todo_corte() {
        let (bytes, esperado) = um_fluxo();
        // Dois pedaços, cortados em cada posição possível.
        for corte in 0..=bytes.len() {
            let mut leitor = LeitorDeTela::novo();
            let (a, b) = bytes.split_at(corte);
            leitor.empurrar(a).unwrap();
            let mut tudo = lidos(&mut leitor);
            leitor.empurrar(b).unwrap();
            tudo.extend(lidos(&mut leitor));
            assert_eq!(tudo, esperado, "cortado no byte {corte}");
        }
        // E byte a byte.
        let mut leitor = LeitorDeTela::novo();
        let mut tudo = Vec::new();
        for byte in &bytes {
            leitor.empurrar(std::slice::from_ref(byte)).unwrap();
            tudo.extend(lidos(&mut leitor));
        }
        assert_eq!(tudo, esperado);
    }

    #[test]
    fn o_leitor_recusa_e_fecha() {
        for (cabecalho, erro) in [
            ([7, 0, 0, 0, 1], ErroDeQuadro::TipoDesconhecido { byte: 7 }),
            ([0, 0, 0, 0, 0], ErroDeQuadro::QuadroVazio),
            (
                cabecalho_de_quadro(TipoDeQuadro::Chave, u32::MAX),
                ErroDeQuadro::QuadroGrandeDemais {
                    len: u32::MAX as usize,
                },
            ),
        ] {
            let mut leitor = LeitorDeTela::novo();
            let mut bytes = abertura();
            bytes.extend_from_slice(&cabecalho);
            assert_eq!(leitor.empurrar(&bytes), Err(erro), "{cabecalho:?}");
            // **Um erro fecha o leitor**: um fluxo de lixo não pede uma
            // segunda leitura, e o que vier depois é recusado com o mesmo erro.
            assert_eq!(leitor.empurrar(&quadro(TipoDeQuadro::Chave, 3)), Err(erro));
            assert_eq!(
                lidos(&mut leitor).len(),
                1,
                "só a abertura sai: {cabecalho:?}"
            );
        }
    }

    /// **O tamanho é recusado antes de qualquer reserva.** O tamanho é o
    /// único número que um fluxo de lixo controla, e ele chega antes do corpo:
    /// um leitor que reservasse pelo anunciado e só depois conferisse daria a
    /// quem manda a alocação que ele quisesse (`specs/08-seguranca.md`).
    ///
    /// Um tamanho logo acima do teto, e não `u32::MAX`: com a ordem trocada, a
    /// reserva de 4 MiB + 1 acontece de verdade e este teste reprova limpo, em
    /// vez de o processo abortar por falta de memória. (Um `#[global_allocator]`
    /// contador não cabe aqui: o workspace proíbe `unsafe`, e um `forbid` não
    /// se afrouxa nem num teste.)
    #[test]
    fn o_tamanho_e_recusado_antes_de_reservar() {
        let tamanho = u32::try_from(MAX_QUADRO_LEN).unwrap() + 1;
        let mut bytes = abertura();
        bytes.extend_from_slice(&cabecalho_de_quadro(TipoDeQuadro::Chave, tamanho));
        let mut leitor = LeitorDeTela::novo();
        assert_eq!(
            leitor.empurrar(&bytes),
            Err(ErroDeQuadro::QuadroGrandeDemais {
                len: tamanho as usize
            })
        );
        assert_eq!(
            leitor.maior_reserva(),
            0,
            "o leitor reservou memória para um quadro que ele recusou"
        );
        assert_eq!(leitor.bytes_reservados(), 0);
    }

    #[test]
    fn um_quadro_aceito_reserva_o_que_anunciou_e_solta_ao_sair() {
        let mut bytes = abertura();
        let corpo = quadro(TipoDeQuadro::Chave, 1000);
        bytes.extend_from_slice(corpo.get(..CABECALHO_DE_QUADRO_LEN + 10).unwrap());
        let mut leitor = LeitorDeTela::novo();
        leitor.empurrar(&bytes).unwrap();
        assert_eq!(leitor.bytes_reservados(), 1000, "o quadro em andamento");
        assert_eq!(leitor.maior_reserva(), 1000);
        leitor
            .empurrar(corpo.get(CABECALHO_DE_QUADRO_LEN + 10..).unwrap())
            .unwrap();
        assert_eq!(
            leitor.bytes_reservados(),
            0,
            "o quadro pronto saiu do andamento"
        );
    }

    /// **A memória segue o que chegou, e não o que foi anunciado** (sugestão
    /// da F1 na revisão de 09/10). Um par que anuncia 4 MiB e manda devagar
    /// não segura 4 MiB: o leitor começa com até [`RESERVA_INICIAL_DE_QUADRO`] e cresce
    /// dobrando, sem passar do anunciado. Dobrar, e não crescer o exato do
    /// pedaço: um fluxo que pingasse byte a byte faria uma realocação por byte.
    #[test]
    fn um_quadro_grande_que_chega_devagar_segura_o_que_chegou() {
        let anunciado = u32::try_from(MAX_QUADRO_LEN).unwrap();
        let mut leitor = LeitorDeTela::novo();
        leitor.empurrar(&abertura()).unwrap();
        // Só o cabeçalho de um quadro de 4 MiB: no máximo os 64 KiB do começo.
        leitor
            .empurrar(&cabecalho_de_quadro(TipoDeQuadro::Chave, anunciado))
            .unwrap();
        assert!(
            leitor.bytes_reservados() <= RESERVA_INICIAL_DE_QUADRO,
            "só o cabeçalho de um quadro de 4 MiB segurou {} bytes",
            leitor.bytes_reservados()
        );
        leitor.empurrar(&[0xAB; 10]).unwrap();
        assert!(
            leitor.bytes_reservados() <= RESERVA_INICIAL_DE_QUADRO,
            "dez bytes de um quadro de 4 MiB seguram {} bytes",
            leitor.bytes_reservados()
        );
        let mut chegou = 10;
        for _ in 0..50 {
            leitor.empurrar(&[0xCD; 4096]).unwrap();
            chegou += 4096;
            let reservado = leitor.bytes_reservados();
            assert!(
                reservado >= chegou,
                "o que chegou não cabe no que foi reservado"
            );
            assert!(
                reservado <= (2 * chegou).max(RESERVA_INICIAL_DE_QUADRO),
                "com {chegou} bytes recebidos, o leitor segura {reservado}"
            );
        }
        assert!(leitor.maior_reserva() <= (2 * chegou).max(RESERVA_INICIAL_DE_QUADRO));
    }

    /// **O corpo cresce dobrando.** Um quadro de 256 KiB que chega byte a
    /// byte cresce duas vezes (64 → 128 → 256 KiB), e não uma vez por byte:
    /// crescer o exato de cada pedaço também caberia no limite de memória, mas
    /// faria de um fluxo que pinga um trabalho quadrático.
    #[test]
    fn um_quadro_que_pinga_byte_a_byte_cresce_poucas_vezes() {
        let tamanho = 256 * 1024;
        let mut leitor = LeitorDeTela::novo();
        leitor.empurrar(&abertura()).unwrap();
        leitor
            .empurrar(&cabecalho_de_quadro(
                TipoDeQuadro::Chave,
                u32::try_from(tamanho).unwrap(),
            ))
            .unwrap();
        for _ in 0..tamanho {
            leitor.empurrar(&[0xEE]).unwrap();
        }
        assert!(
            leitor.crescimentos <= 4,
            "um quadro de {tamanho} bytes, byte a byte, cresceu {} vezes",
            leitor.crescimentos
        );
        assert!(
            matches!(lidos(&mut leitor).last(), Some(LidoDaTela::Quadro { bytes, .. }) if bytes.len() == tamanho)
        );
    }

    #[test]
    fn uma_abertura_ilegivel_fecha_o_leitor() {
        let mut bytes = abertura();
        bytes[0] = 0;
        let mut leitor = LeitorDeTela::novo();
        assert_eq!(
            leitor.empurrar(&bytes),
            Err(ErroDeQuadro::Cabecalho(ScreenError::UnsupportedVersion {
                found: 0,
                expected: SCREEN_HEADER_VERSION
            }))
        );
        assert_eq!(leitor.proximo(), None);
    }

    #[test]
    fn um_quadro_do_tamanho_do_teto_passa() {
        let mut bytes = abertura();
        bytes.extend(quadro(TipoDeQuadro::Chave, MAX_QUADRO_LEN));
        let mut leitor = LeitorDeTela::novo();
        leitor.empurrar(&bytes).unwrap();
        let tudo = lidos(&mut leitor);
        assert!(
            matches!(tudo.get(1), Some(LidoDaTela::Quadro { bytes, .. }) if bytes.len() == MAX_QUADRO_LEN)
        );
    }
}
