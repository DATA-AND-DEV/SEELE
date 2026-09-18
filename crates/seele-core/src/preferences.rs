//! Settings that stay on this machine.
//!
//! The comp calls the screen these belong to "Terminal server · configuração
//! local", and the word that matters is *local*: none of this is sent anywhere,
//! none of it follows the person to another computer, and every one of them is
//! about the hardware in front of the person rather than about the server.
//!
//! Today there are two — which microphone to open, and where the sound comes
//! out. They are here rather than in the desktop shell because the terminal
//! client has the same questions to answer, and a preference written down by one
//! client and ignored by the other is a preference that appears to have been
//! forgotten.
//!
//! # Why not in `conhecidos`
//!
//! That file is one line per server, and this is not per server. A microphone
//! chosen while visiting one server is the same microphone at the next one, and
//! folding it in there would either repeat it on every line or invent a line for
//! a server nobody visited.
//!
//! # Format
//!
//! One setting per line, name and value separated by a tab:
//!
//! ```text
//! capture <TAB> coreaudio:AppleUSBAudioEngine:Focusrite:Scarlett Solo:1
//! playback <TAB> coreaudio:AppleHDAEngineOutput:1:0:1:0
//! ```
//!
//! Text, and a name per line rather than a fixed column order, for the reason
//! `conhecidos` gives and one more: a version of this file written by an older
//! build is missing lines rather than misaligned, so an unknown name is skipped
//! and a missing one is simply unset. Neither can turn into the wrong value.
//!
//! A file that cannot be read is an empty one. Refusing to start a client
//! because a settings file got truncated would be the wrong trade — the whole
//! product still works with every default.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::voice::VoiceMode;

/// The name of the microphone setting on disk.
///
/// Written once here rather than at both ends: a reader and a writer that
/// disagree about the spelling is a setting that saves and never loads, and
/// nothing about that failure is visible.
const CAPTURE: &str = "capture";

/// The name of the sound-output setting on disk.
///
/// Spelled once, for the reason [`CAPTURE`] gives.
const PLAYBACK: &str = "playback";

/// The name of the microphone-gate setting on disk.
///
/// Spelled once, for the reason [`CAPTURE`] gives.
const VOICE_MODE: &str = "voice_mode";

/// The name of the push-to-talk key setting on disk.
///
/// Spelled once, for the reason [`CAPTURE`] gives.
const PUSH_TO_TALK_KEY: &str = "push_to_talk_key";

/// The name of the local nickname setting on disk.
///
/// Spelled once, for the reason [`CAPTURE`] gives.
const NICKNAME: &str = "nickname";
/// Quantos pares esta máquina aceita atender. `0` é «não empresto».
const PARES_QUE_ATENDE: &str = "pares_que_atende";
/// Se aceita que o próprio endereço seja entregue a quem for servi-la.
const ASSISTE_POR_PAR: &str = "assiste_por_par";
/// A maior subida que a sonda já mediu **nesta máquina**, em bits por segundo.
const CAMINHO_DA_MAQUINA: &str = "caminho_da_maquina_bps";

/// The local settings, on disk.
#[derive(Debug, Clone, Default)]
pub struct Preferences {
    path: PathBuf,
    capture: Option<String>,
    playback: Option<String>,
    voice_mode: Option<VoiceMode>,
    push_to_talk_key: Option<String>,
    nickname: Option<String>,
    /// As duas metades do consentimento do caminho entre pares — §5.
    ///
    /// **Guardadas, e não perguntadas a cada sessão.** O consentimento viaja na
    /// declaração de identidade, que sai a cada conexão; se ele não
    /// sobrevivesse ao fechamento da janela, quem optou por emprestar teria de
    /// optar de novo toda vez — e um opt-in que se perde é um opt-in que
    /// ninguém usa.
    ///
    /// Ausente é `0` e `false`, que é «não participo». O padrão de quem nunca
    /// escolheu é não participar, e é o que o §5 manda.
    pares_que_atende: Option<u8>,
    assiste_por_par: Option<bool>,
    /// A maior subida que a sonda já mediu nesta máquina.
    ///
    /// **Da máquina, e não do servidor.** A lista de conhecidos já guarda uma
    /// medida por servidor visitado, e ela é mais específica — o caminho até
    /// cada servidor é diferente. Mas ela não cobre dois casos, e os dois doem:
    /// um servidor hospedado **aqui** nunca entra naquela lista, de propósito,
    /// e a primeira visita a um servidor novo não tem entrada nenhuma.
    ///
    /// Nos dois, a sonda partia de `CAMINHO_DA_PROVA_BPS` — 2 Mbps —, e 60%
    /// disso não compra 720p. Medido: o primeiro segundo é 540p mesmo com o
    /// loopback por baixo e cinquenta megabits de subida, porque a perna que
    /// aperta não é o cano, é o que a sonda mediu.
    ///
    /// A subida é da máquina; o caminho até cada servidor é que difere. Por isso
    /// este número é um **ponto de partida**, e o da lista de conhecidos, quando
    /// existe, tem precedência.
    caminho_da_maquina_bps: Option<u32>,
}

impl Preferences {
    /// Reads the settings, or starts with the defaults.
    ///
    /// # Errors
    ///
    /// Only if the directory cannot be created. An unreadable or malformed file
    /// is treated as an unwritten one — see the module note.
    pub fn open(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("could not create {}", parent.display()))?;
        }

        let mut settings = Self {
            path,
            capture: None,
            playback: None,
            voice_mode: None,
            push_to_talk_key: None,
            nickname: None,
            pares_que_atende: None,
            assiste_por_par: None,
            caminho_da_maquina_bps: None,
        };
        if let Ok(text) = std::fs::read_to_string(&settings.path) {
            for line in text.lines() {
                let Some((name, value)) = line.split_once('\t') else {
                    continue;
                };
                let value = value.trim();
                let value = (!value.is_empty()).then(|| value.to_owned());
                // An unknown name is a setting from a newer build, and skipping
                // it is the only honest thing this one can do with it.
                match name.trim() {
                    CAPTURE => settings.capture = value,
                    PLAYBACK => settings.playback = value,
                    // A name this build does not know for a mode it does not
                    // know reads as unset, not as the default: see
                    // `VoiceMode::from_name`.
                    VOICE_MODE => {
                        settings.voice_mode = value.as_deref().and_then(VoiceMode::from_name)
                    }
                    PUSH_TO_TALK_KEY => settings.push_to_talk_key = value,
                    NICKNAME => settings.nickname = value,
                    // Um número que não é número lê como não escrito, e não
                    // como zero: o segundo seria esta versão decidindo «não
                    // empresto» por causa de um arquivo torto.
                    PARES_QUE_ATENDE => {
                        settings.pares_que_atende = value.as_deref().and_then(|v| v.parse().ok())
                    }
                    CAMINHO_DA_MAQUINA => {
                        settings.caminho_da_maquina_bps =
                            value.as_deref().and_then(|v| v.parse().ok())
                    }
                    ASSISTE_POR_PAR => {
                        settings.assiste_por_par = match value.as_deref() {
                            Some("sim") => Some(true),
                            Some("nao") => Some(false),
                            _ => None,
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(settings)
    }

    /// Which microphone to open, as a `CaptureDevice` id.
    ///
    /// `None` is the machine's default, and is what every client did before
    /// there was a screen to choose on.
    ///
    /// A device that is no longer plugged in still reads back from here. That is
    /// deliberate: the caller falls back to the default for *this* session and
    /// leaves the preference alone, so plugging the interface back in restores
    /// the choice instead of requiring it to be made again.
    #[must_use]
    pub fn capture(&self) -> Option<&str> {
        self.capture.as_deref()
    }

    /// Writes down which microphone to open. `None` goes back to the default.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be written.
    pub fn set_capture(&mut self, device: Option<&str>) -> Result<()> {
        // A tab or a newline inside an id would make the next read see one
        // setting as two, or as none.
        self.capture = device
            .map(sanitise)
            .filter(|device| !device.trim().is_empty());
        self.write()
    }

    /// Where the sound comes out, as a `PlaybackDevice` id.
    ///
    /// `None` is the machine's default. Everything [`Preferences::capture`] says
    /// about a device that is no longer plugged in holds here too, and matters
    /// more: falling back to the machine's speakers for one session makes no
    /// sound of its own, so a preference erased on the way would be a choice
    /// that vanished without anything to notice.
    #[must_use]
    pub fn playback(&self) -> Option<&str> {
        self.playback.as_deref()
    }

    /// Writes down where the sound comes out. `None` goes back to the default.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be written.
    pub fn set_playback(&mut self, device: Option<&str>) -> Result<()> {
        self.playback = device
            .map(sanitise)
            .filter(|device| !device.trim().is_empty());
        self.write()
    }

    /// How the microphone opens. `None` is what `specs/03-audio.md` defaults to.
    ///
    /// Push-to-talk is the default *because it never false-triggers*, and that
    /// argument is about a person who has not chosen. Somebody who has chosen
    /// voice activation and finds push-to-talk again the next morning was not
    /// protected by the default — they were ignored by it.
    #[must_use]
    pub const fn voice_mode(&self) -> Option<VoiceMode> {
        self.voice_mode
    }

    /// Writes down how the microphone opens. `None` goes back to the default.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be written.
    pub fn set_voice_mode(&mut self, mode: Option<VoiceMode>) -> Result<()> {
        self.voice_mode = mode;
        self.write()
    }

    /// Which key opens the microphone in push-to-talk, or `None` for the space bar.
    ///
    /// A `KeyboardEvent.code` — `Space`, `KeyF`, `ControlLeft`. **Opaque here**,
    /// exactly like a device id: this side never decides what a key means, it
    /// only remembers which one was chosen. The shell that reads keyboards is
    /// the only place that can name them, and it is the only place that does.
    ///
    /// The layout-independent `code` and not `key`: `key` on an AZERTY keyboard
    /// gives a different letter for the same physical spot, so a choice made on
    /// one layout would land somewhere else on another.
    #[must_use]
    pub fn push_to_talk_key(&self) -> Option<&str> {
        self.push_to_talk_key.as_deref()
    }

    /// Writes down which key opens the microphone. `None` goes back to the space bar.
    ///
    /// # Errors
    ///
    /// Fails if the file cannot be written.
    pub fn set_push_to_talk_key(&mut self, key: Option<&str>) -> Result<()> {
        self.push_to_talk_key = key.map(sanitise).filter(|key| !key.trim().is_empty());
        self.write()
    }

    /// O apelido com que se entra, quando não se disse outro.
    ///
    /// # Por que ele mora aqui, e não no servidor
    ///
    /// Porque é **antes** de haver servidor. A comp da 0.9.0 tira o campo de
    /// apelido da tela de entrada e o põe num perfil que se abre sem sessão
    /// nenhuma — e um nome escolhido antes de conectar não tem onde ser gravado
    /// a não ser aqui.
    ///
    /// O apelido **do servidor** é outra coisa e continua sendo dele: cada
    /// servidor guarda o seu, único, e trocá-lo lá é o `SetNickname`. Este é o
    /// que se leva ao entrar num servidor novo, e o que o `conhecidos` já
    /// grava por servidor sobrescreve para os que já se conhece.
    #[must_use]
    pub fn nickname(&self) -> Option<&str> {
        self.nickname.as_deref()
    }

    /// Escreve o apelido de entrada. `None` volta a não ter nenhum.
    ///
    /// # Errors
    ///
    /// Falha se o arquivo não puder ser escrito.
    pub fn set_nickname(&mut self, nickname: Option<&str>) -> Result<()> {
        self.nickname = nickname
            .map(sanitise)
            .filter(|nickname| !nickname.trim().is_empty());
        self.write()
    }

    /// A maior subida já medida nesta máquina, ou `None` enquanto não houve uma.
    #[must_use]
    pub fn caminho_da_maquina(&self) -> Option<u32> {
        self.caminho_da_maquina_bps
    }

    /// Anota uma medida nova, **se ela for maior que a guardada**.
    ///
    /// Maior, e não a última: a sonda mede o que a janela carregou, e uma
    /// transmissão curta num momento ruim mede pouco sem que o cano tenha
    /// encolhido. Guardar o menor faria a próxima sessão começar pior por causa
    /// de um instante, e a sonda desce sozinha quando dói — subir é que custa
    /// os vinte e cinco segundos.
    ///
    /// Zero não anota nada: é o que a sonda devolve quando ninguém compartilhou
    /// tela, e não é uma medida de zero.
    ///
    /// # Errors
    ///
    /// Falha se o arquivo não puder ser escrito.
    pub fn anotar_caminho_da_maquina(&mut self, bps: u32) -> Result<()> {
        if bps == 0
            || self
                .caminho_da_maquina_bps
                .is_some_and(|antes| antes >= bps)
        {
            return Ok(());
        }
        self.caminho_da_maquina_bps = Some(bps);
        self.write()
    }

    /// O consentimento guardado: quantos pares atende, e se aceita ser servida.
    ///
    /// Ausente é «não participo», que é o padrão de quem nunca escolheu.
    #[must_use]
    pub fn caminho_entre_pares(&self) -> (u8, bool) {
        (
            self.pares_que_atende.unwrap_or(0),
            self.assiste_por_par.unwrap_or(false),
        )
    }

    /// Escreve as duas metades do consentimento.
    ///
    /// # Errors
    ///
    /// Falha se o arquivo não puder ser escrito.
    pub fn set_caminho_entre_pares(
        &mut self,
        pares_que_atende: u8,
        assiste_por_par: bool,
    ) -> Result<()> {
        self.pares_que_atende = Some(pares_que_atende);
        self.assiste_por_par = Some(assiste_por_par);
        self.write()
    }

    fn write(&self) -> Result<()> {
        // Every setting, not only the one that just changed: this rewrites the
        // whole file, so a line left out here is a line deleted from disk. That
        // is how a second setting turns into a bug in the first one.
        let mut text = String::new();
        let modo = self.voice_mode.map(|mode| mode.as_str().to_owned());
        let pares = self.pares_que_atende.map(|n| n.to_string());
        let assiste = self
            .assiste_por_par
            .map(|sim| if sim { "sim" } else { "nao" }.to_owned());
        let caminho = self.caminho_da_maquina_bps.map(|bps| bps.to_string());
        for (name, value) in [
            (CAPTURE, &self.capture),
            (PLAYBACK, &self.playback),
            (VOICE_MODE, &modo),
            (PUSH_TO_TALK_KEY, &self.push_to_talk_key),
            (NICKNAME, &self.nickname),
            (PARES_QUE_ATENDE, &pares),
            (ASSISTE_POR_PAR, &assiste),
            (CAMINHO_DA_MAQUINA, &caminho),
        ] {
            let Some(value) = value else {
                continue;
            };
            text.push_str(name);
            text.push('\t');
            text.push_str(value);
            text.push('\n');
        }
        write_private(&self.path, text.as_bytes())
            .with_context(|| format!("could not write {}", self.path.display()))
    }
}

/// Removes what would break the format.
fn sanitise(value: &str) -> String {
    value
        .chars()
        .filter(|character| *character != '\t' && *character != '\n' && *character != '\r')
        .collect()
}

/// The same restricted mode the identity and the visited list are written with.
#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("seele-preferences-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        path.join("preferences")
    }

    #[test]
    fn a_chosen_microphone_survives_the_process() {
        // The whole reason this file exists. A picker whose pick is forgotten
        // when the window closes is a picker that has to be used every day.
        let path = scratch("survives");
        {
            let mut settings = Preferences::open(path.clone()).expect("open");
            settings
                .set_capture(Some("coreaudio:Scarlett Solo"))
                .expect("write");
        }
        let settings = Preferences::open(path).expect("reopen");
        assert_eq!(settings.capture(), Some("coreaudio:Scarlett Solo"));
    }

    #[test]
    fn a_chosen_output_survives_the_process() {
        let path = scratch("output-survives");
        {
            let mut settings = Preferences::open(path.clone()).expect("open");
            settings
                .set_playback(Some("coreaudio:Studio Display Speakers"))
                .expect("write");
        }
        let settings = Preferences::open(path).expect("reopen");
        assert_eq!(
            settings.playback(),
            Some("coreaudio:Studio Display Speakers")
        );
    }

    #[test]
    fn writing_one_setting_does_not_erase_the_other() {
        // The defect a second setting invents. `write` rewrites the whole file,
        // so a setter that forgot to put the other line back would delete it —
        // and the way that shows up is somebody choosing an output and finding
        // their microphone quietly back on the machine's default the next time
        // they start, with nothing anywhere connecting the two.
        // **Cada ajuste novo multiplica esta armadilha**, e por isso os quatro
        // são escritos um por um, na ordem em que um esquecimento apareceria:
        // quem escreve o último é quem tem mais chance de ter deixado os
        // outros três de fora do `write`.
        let path = scratch("both");
        {
            let mut settings = Preferences::open(path.clone()).expect("open");
            settings.set_capture(Some("alsa:hw:1,0")).expect("write");
            settings.set_playback(Some("alsa:hw:2,0")).expect("write");
            settings
                .set_voice_mode(Some(VoiceMode::VoiceActivated))
                .expect("write");
            settings.set_push_to_talk_key(Some("KeyF")).expect("write");
            settings.set_nickname(Some("marcela")).expect("write");
        }
        let settings = Preferences::open(path).expect("reopen");
        assert_eq!(settings.capture(), Some("alsa:hw:1,0"));
        assert_eq!(settings.playback(), Some("alsa:hw:2,0"));
        assert_eq!(settings.voice_mode(), Some(VoiceMode::VoiceActivated));
        assert_eq!(settings.push_to_talk_key(), Some("KeyF"));
        assert_eq!(settings.nickname(), Some("marcela"));
    }

    #[test]
    fn o_modo_escolhido_atravessa_o_processo() {
        // O pedido que trouxe este ajuste: quem escolhe voz não quer achar
        // push-to-talk de volta amanhã. Os três, porque um `match` que
        // esquecesse um ramo só erraria naquele.
        for modo in [
            VoiceMode::PushToTalk,
            VoiceMode::VoiceActivated,
            VoiceMode::Open,
        ] {
            let path = scratch(&format!("modo-{}", modo.as_str()));
            {
                let mut settings = Preferences::open(path.clone()).expect("open");
                settings.set_voice_mode(Some(modo)).expect("write");
            }
            let settings = Preferences::open(path).expect("reopen");
            assert_eq!(settings.voice_mode(), Some(modo), "{}", modo.as_str());
        }
    }

    #[test]
    fn um_modo_que_esta_versao_nao_conhece_le_como_nao_escolhido() {
        // Uma versão mais nova pode escrever um quarto modo. Esta não pode
        // adivinhar qual é — e transformá-lo em push-to-talk seria sobrescrever
        // uma escolha que ela apenas não entendeu. Fica sem valor, que é o que
        // o cabeçalho do módulo promete para nome desconhecido.
        let path = scratch("modo-do-futuro");
        // O `open` é quem cria o diretório; semear antes dele escreveria no nada.
        Preferences::open(path.clone()).expect("criar o diretório");
        std::fs::write(&path, "voice_mode\tsussurro\n").expect("semear");
        let settings = Preferences::open(path).expect("open");
        assert_eq!(settings.voice_mode(), None);
    }

    #[test]
    fn a_tecla_nao_pode_quebrar_o_formato() {
        // Uma tabulação dentro do valor faria a próxima leitura ver dois
        // ajustes onde há um. O `code` de um teclado nunca teria uma — mas o
        // que chega aqui vem da casca, e o que a casca manda é dela.
        let path = scratch("tecla-suja");
        {
            let mut settings = Preferences::open(path.clone()).expect("open");
            settings
                .set_push_to_talk_key(Some("Key\tF\nplayback\tfalso"))
                .expect("write");
        }
        let settings = Preferences::open(path).expect("reopen");
        assert_eq!(settings.push_to_talk_key(), Some("KeyFplaybackfalso"));
        assert_eq!(
            settings.playback(),
            None,
            "a tecla inventou um segundo ajuste"
        );
    }

    #[test]
    fn the_two_settings_do_not_read_as_each_other() {
        // Both are ids of the same shape, written with the same grammar, and a
        // reader that matched the wrong name would send somebody's speakers to
        // the microphone. The compiler has nothing to say about it: they are
        // both `Option<String>`.
        let path = scratch("crossed");
        {
            let mut settings = Preferences::open(path.clone()).expect("open");
            settings.set_capture(Some("o microfone")).expect("write");
        }
        let settings = Preferences::open(path).expect("reopen");
        assert_eq!(settings.capture(), Some("o microfone"));
        assert_eq!(
            settings.playback(),
            None,
            "the microphone was read back as the sound output"
        );
    }

    #[test]
    fn nothing_written_down_means_the_machines_default() {
        let settings = Preferences::open(scratch("unset")).expect("open");
        assert_eq!(settings.capture(), None);
        assert_eq!(settings.playback(), None);
    }

    #[test]
    fn going_back_to_the_default_erases_the_choice() {
        // Not merely "stops being applied": the line has to leave the file, or
        // the next build to read it would find the old id still sitting there.
        let path = scratch("cleared");
        {
            let mut settings = Preferences::open(path.clone()).expect("open");
            settings.set_capture(Some("alsa:hw:1,0")).expect("write");
            settings.set_capture(None).expect("clear");
        }
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(
            !text.contains("alsa:hw:1,0"),
            "the cleared id is still in the file: {text:?}"
        );
        assert_eq!(Preferences::open(path).expect("reopen").capture(), None);
    }

    #[test]
    fn an_id_with_a_tab_in_it_cannot_forge_a_second_setting() {
        // `cpal` builds ids out of strings the operating system hands over, and
        // this file's whole grammar is one tab per line. Without the filter, an
        // id carrying one would be read back as a different, shorter id.
        let path = scratch("forged");
        {
            let mut settings = Preferences::open(path.clone()).expect("open");
            settings
                .set_capture(Some("coreaudio:Mic\tcapture\televado"))
                .expect("write");
        }
        let settings = Preferences::open(path).expect("reopen");
        assert_eq!(settings.capture(), Some("coreaudio:Miccaptureelevado"));
    }

    #[test]
    fn an_output_id_with_a_tab_in_it_cannot_forge_the_microphone_setting() {
        // Worse than the same hole on the capture side, and that is why it gets
        // its own test rather than being assumed from the twin: with two
        // settings in one file, an unfiltered tab in an output id writes a
        // `capture` line. Choosing where the sound comes out would change which
        // microphone opens, and nothing on either screen would connect the two.
        let path = scratch("forged-capture");
        {
            let mut settings = Preferences::open(path.clone()).expect("open");
            settings
                .set_playback(Some("coreaudio:Caixa\ncapture\tum microfone alheio"))
                .expect("write");
        }
        let settings = Preferences::open(path).expect("reopen");
        assert_eq!(
            settings.capture(),
            None,
            "an output id wrote itself a microphone setting"
        );
    }

    #[test]
    fn a_file_full_of_nonsense_reads_as_the_defaults() {
        // A settings file that got truncated must not stop a client from
        // starting. Every default still works.
        let path = scratch("nonsense");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(&path, "\0\u{feff}not a setting\nfuturo\tvalor\n").expect("write");

        let settings = Preferences::open(path).expect("open");
        assert_eq!(
            settings.capture(),
            None,
            "an unreadable file must not become a microphone nobody chose"
        );
        assert_eq!(
            settings.playback(),
            None,
            "an unreadable file must not become an output nobody chose"
        );
    }

    /// **O consentimento da malha sobrevive ao fechamento da janela.**
    ///
    /// Ele viaja na declaração de identidade, que sai a cada conexão. Se não
    /// fosse guardado, quem optou por emprestar teria de optar de novo toda
    /// vez — e um opt-in que se perde é um opt-in que ninguém usa.
    #[test]
    fn o_consentimento_do_caminho_entre_pares_atravessa_o_fechamento() {
        let caminho = scratch("malha-atravessa");

        {
            let mut p = Preferences::open(caminho.clone()).expect("abrir");
            assert_eq!(
                p.caminho_entre_pares(),
                (0, false),
                "o padrão é não participar"
            );
            p.set_caminho_entre_pares(1, true).expect("gravar");
        }

        let relido = Preferences::open(caminho).expect("reabrir");
        assert_eq!(relido.caminho_entre_pares(), (1, true));
    }

    /// E desligar é um estado gravado, e não a ausência de arquivo: sem isto,
    /// «desliguei» e «nunca escolhi» seriam indistinguíveis no disco — e a
    /// primeira escrita seguinte poderia ressuscitar a escolha antiga.
    #[test]
    fn desligar_a_malha_e_gravado_e_nao_apagado() {
        let caminho = scratch("malha-desligada");

        {
            let mut p = Preferences::open(caminho.clone()).expect("abrir");
            p.set_caminho_entre_pares(1, true).expect("ligar");
            p.set_caminho_entre_pares(0, false).expect("desligar");
        }

        let texto = std::fs::read_to_string(&caminho).expect("ler o arquivo");
        assert!(
            texto.contains("pares_que_atende\t0"),
            "o zero foi gravado: {texto}"
        );
        assert!(
            texto.contains("assiste_por_par\tnao"),
            "o não foi gravado: {texto}"
        );
        assert_eq!(
            Preferences::open(caminho)
                .expect("reabrir")
                .caminho_entre_pares(),
            (0, false)
        );
    }

    /// **A subida da máquina só sobe.**
    ///
    /// A sonda mede o que a janela carregou, e uma transmissão curta num
    /// momento ruim mede pouco sem que o cano tenha encolhido. Guardar o menor
    /// faria a sessão seguinte começar pior por causa de um instante — e a
    /// sonda desce sozinha quando dói; subir é que custa os vinte e cinco
    /// segundos.
    #[test]
    fn a_subida_da_maquina_guarda_a_maior_e_ignora_o_instante_ruim() {
        let caminho = scratch("caminho-da-maquina");
        let mut p = Preferences::open(caminho.clone()).expect("abrir");
        assert_eq!(p.caminho_da_maquina(), None, "nada medido ainda");

        p.anotar_caminho_da_maquina(12_000_000).expect("primeira");
        assert_eq!(p.caminho_da_maquina(), Some(12_000_000));

        p.anotar_caminho_da_maquina(3_000_000)
            .expect("um instante ruim");
        assert_eq!(
            p.caminho_da_maquina(),
            Some(12_000_000),
            "uma janela ruim não encolhe o que a máquina já provou carregar"
        );

        p.anotar_caminho_da_maquina(20_000_000).expect("cano maior");
        assert_eq!(p.caminho_da_maquina(), Some(20_000_000));

        assert_eq!(
            Preferences::open(caminho)
                .expect("reabrir")
                .caminho_da_maquina(),
            Some(20_000_000),
            "e atravessa o fechamento da janela"
        );
    }

    /// Zero é «ninguém compartilhou tela», e não uma medida de zero.
    #[test]
    fn zero_nao_e_medida_e_nao_apaga_a_que_havia() {
        let caminho = scratch("caminho-zero");
        let mut p = Preferences::open(caminho).expect("abrir");
        p.anotar_caminho_da_maquina(8_000_000).expect("medir");
        p.anotar_caminho_da_maquina(0).expect("sessão sem tela");
        assert_eq!(p.caminho_da_maquina(), Some(8_000_000));
    }
}
