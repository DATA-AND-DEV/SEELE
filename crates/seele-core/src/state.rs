//! What is true about the session right now.
//!
//! A fold over [`ServerMessage`]: which people are in which voice room, what they are
//! called, what state they announced, what has been said. Nothing here draws
//! anything or decides what a human should read.
//!
//! # Why this is in the core and not in the shells
//!
//! `specs/01-arquitetura.md` is blunt about the consequence of getting this
//! wrong: "Se uma funcionalidade precisa ser implementada duas vezes em duas
//! interfaces diferentes, ela está no lugar errado."
//!
//! The M4 terminal client kept this bookkeeping itself, and it worked. Then
//! `specs/06-clientes-gui.md` asked `seele-ffi` for `estado_atual() -> Snapshot`
//! — the same map from person to nickname, the same seats, the same "who is
//! speaking". Building that separately would have been the second
//! implementation, with the third waiting in M6.
//!
//! So it moved here. What is left in a shell is the part that genuinely differs
//! between a terminal and a window: how to flatten this into rows, how to
//! format a timestamp, how wide a name may be.
//!
//! # What a shell still owns
//!
//! Ordering for display, truncation, colour, and anything involving a clock or
//! a locale. This module has no opinion about what time it is.

use std::collections::HashMap;

use seele_proto::control::{ChannelInfo, Permission, PersonState, VoiceRoomInfo};
use seele_proto::ids::{ChannelId, MessageId, PersonId, ScreenId, Ssrc, VoiceRoomId};
use seele_proto::signal::SignalBand;
use seele_proto::ServerMessage;

use crate::client::SessionInfo;

/// One person, as far as this client knows.
#[derive(Debug, Clone, PartialEq)]
pub struct Person {
    /// Account identifier.
    pub id: PersonId,
    /// Display name.
    pub nickname: String,
    /// Their media source, for per-talker volume.
    pub ssrc: Option<Ssrc>,
    /// Microphone muted — mudo.
    pub muted: bool,
    /// Speakers muted — Isolamento total.
    pub total_isolation: bool,
    /// Transmitting right now.
    pub speaking: bool,
    /// Sync Ratio, 0 to 100.
    pub signal: u8,
}

impl Person {
    fn new(id: PersonId, nickname: String, ssrc: Option<Ssrc>) -> Self {
        Self {
            id,
            nickname,
            ssrc,
            muted: false,
            total_isolation: false,
            speaking: false,
            // Zero rather than a hopeful hundred: an unmeasured Sync Ratio that
            // reads as perfect is worse than one that reads as unknown, because
            // it looks like an answer.
            signal: 0,
        }
    }
}

/// One thing somebody said.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// Server-assigned identifier, ordered.
    pub id: MessageId,
    /// Which Channel it was said in.
    pub channel: ChannelId,
    /// Who said it.
    pub author: PersonId,
    /// Their name at the time it arrived.
    pub author_nickname: String,
    /// When the server accepted it, in **seconds** since the Unix epoch.
    ///
    /// The server's clock. Turning it into something a person reads — a local
    /// time, a locale, a relative "há 3 min" — is a shell's job, and this
    /// module has no opinion about what time it is.
    pub at_seconds: i64,
    /// The body.
    pub body: String,
    /// What it replies to.
    pub replies_to: Option<MessageId>,
    /// Whether this client sent it.
    pub own: bool,
    /// Whether it has been edited since.
    pub edited: bool,
    /// The file hanging off it, if any. ADR 0027.
    ///
    /// **Still `Some` after the bytes are gone**, carrying
    /// `AttachmentState::Expired` with the name and the size the file had. That
    /// is the whole reason the server keeps the row after deleting the blob: a
    /// message that had a picture and now draws as an empty channel would leave
    /// nobody able to tell that there had ever been one.
    pub attachment: Option<seele_proto::control::AttachmentInfo>,
}

/// Something the interface should surface, already carrying its severity.
#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    /// How loud.
    pub severity: seele_proto::control::AlertSeverity,
    /// What about. Enumerated, so each shell writes its own sentence.
    pub reason: seele_proto::control::AlertReason,
    /// The operator's own words, when they have any.
    pub operator_text: Option<String>,
}

/// What became of a file this client was moving.
///
/// Its own kind rather than a [`Notice`]: an `AlertReason` is about the session
/// and a shell draws it in the alert band, while these two are about **one
/// message** and belong beside that message. ADR 0027 also wants the sentence
/// for a fallen transfer to say that retrying starts from zero, which is a
/// thing no alert has ever had to say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferNotice {
    /// A file this client was sending was not taken, and nothing was published.
    Refused {
        /// Which of this client's messages it was.
        client_message_id: seele_proto::ids::ClientMessageId,
        /// Why. Enumerated, so the shell writes a different sentence for a file
        /// that will never fit and one that would fit in a minute.
        reason: seele_proto::control::AttachmentRefusal,
    },
    /// A file this client asked for is not coming.
    Unavailable {
        /// Which attachment.
        attachment: seele_proto::ids::AttachmentId,
        /// Why. The expected reason is `Expired`.
        reason: seele_proto::control::AttachmentRefusal,
    },
}

/// The average Sync Ratio of a voice room, already banded.
///
/// The comp (`comp v2`) draws this as **MÉDIA DO VOICE_ROOM**, a
/// number in the band's colour with the sample size beside it, and computes
/// both in the shell. Here it is computed once, in the core, for the same
/// reason `seele_ffi::types` gives for carrying a band beside every person's
/// number: a threshold known by two shells is a threshold two shells disagree
/// about the day one of them is updated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceRoomSync {
    /// The mean of the seated people' ratios, 0 to 100.
    ///
    /// Rounded to the nearest point, ties up. The comp prints `82.4`, but the
    /// datum is a `u8` at every point it exists — on the wire, in
    /// [`Person::signal`], in the smoothing — so a decimal here would be
    /// precision invented at the last step. `82` is what is known.
    pub ratio: u8,
    /// Which band that mean falls into.
    pub band: SignalBand,
    /// How many people it is the mean of — the comp's `5 PLUGS`.
    ///
    /// Carried so a shell can say what the number is an average *of* without
    /// counting the roster a second time and getting a different answer.
    pub people: usize,
}

/// Why the session ended, if it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ended {
    /// Enumerated reason.
    pub reason: seele_proto::control::DisconnectReason,
}

/// Everything this client knows about the server it is attached to.
#[derive(Debug, Default, Clone)]
pub struct Room {
    /// Which person this connection is.
    pub me: Option<PersonId>,
    /// The media source the server assigned — gap G1.
    pub ssrc: Option<Ssrc>,
    /// What the server is called.
    pub server: String,
    /// The picture the server chose for itself, if it chose one.
    ///
    /// PNG bytes, already bounded by `seele_proto::control` on the way in —
    /// a real PNG, at most `MAX_SERVER_ICON_SIDE` a side, at most
    /// `MAX_SERVER_ICON_LEN` bytes. Nothing here decodes them: a shell draws
    /// them, and the terminal one has nowhere to draw them at all, which is
    /// why this is bytes and not a decoded image.
    ///
    /// `None` is the ordinary state and is what every server that exists today
    /// is in. It is also what a handshake resets this to — see
    /// [`Room::adopt`].
    pub icon: Option<Vec<u8>>,

    /// A imagem de perfil de cada pessoa que tem uma.
    ///
    /// Um mapa por conta, e não um campo de [`Person`], porque `Person` é
    /// reconstruído a cada `PersonJoined`: quem sai de uma sala e entra noutra
    /// vira uma linha nova, e a imagem sobreviveria por acidente ou se perderia,
    /// conforme a ordem dos quadros.
    ///
    /// Quem não tem imagem não aparece aqui — ausência é a resposta, e não um
    /// `None` guardado.
    pub person_icons: std::collections::HashMap<PersonId, Vec<u8>>,
    /// Voice channels visible to this person.
    pub voice_rooms: Vec<VoiceRoomInfo>,
    /// Text channels visible to this person.
    pub channels: Vec<ChannelInfo>,
    /// What this person may do, as PERMISSIONS resolved it.
    ///
    /// Here so a shell can ask "should this control exist at all" without
    /// re-deriving `specs/04-servidor-seele.md`'s "negadas vencem concedidas"
    /// for itself. **Convenience, never enforcement**: the server checks again.
    pub permissions: Vec<Permission>,
    /// The voice room this person's connection is in.
    pub current_voice_room: Option<VoiceRoomId>,
    /// The Channel being read.
    pub current_channel: Option<ChannelId>,
    /// Everybody this client has heard of, by id.
    pub people: HashMap<PersonId, Person>,
    /// Who is seated in which voice room, in arrival order.
    pub seats: HashMap<VoiceRoomId, Vec<PersonId>>,
    /// Quem está conectado neste servidor, esteja ou não numa sala.
    ///
    /// # Por que não dá para deduzir de [`Self::seats`]
    ///
    /// Porque sentar-se numa sala é uma coisa e estar aqui é outra, e por muito
    /// tempo só a primeira tinha mensagem. `PersonJoined` carrega uma sala de voz, então
    /// quem entrava no servidor e ficava fora das salas era invisível para todo
    /// mundo — a lista de pessoas mostrava quem estava sentado e se chamava
    /// «pessoas». Agora `PersonPresent` e `PersonGone` dizem a outra metade.
    ///
    /// Em ordem de chegada, como os assentos, e sem repetição: a mesma pessoa
    /// anunciada duas vezes é uma linha, porque a varredura de abertura e a
    /// difusão podem se sobrepor por um instante.
    pub presentes: Vec<PersonId>,
    /// What has been said, oldest first.
    pub messages: Vec<Message>,
    /// Connection quality as the server reports it.
    pub telemetry: Option<seele_proto::control::Telemetry>,
    /// Quanto da **nossa** voz não está chegando ao servidor, se ele já disse.
    ///
    /// `None` até a primeira medida, e num servidor v1 para sempre — a variante
    /// que carrega isto nasceu na v2 do protocolo (ADR 0036). Distinto de
    /// `Some(0.0)`, que é o servidor afirmando que nada se perdeu.
    ///
    /// **Não é o `loss_fraction` do [`Room::telemetry`]**, e a diferença é a
    /// razão de haver dois campos: aquele mede a direção servidor→cliente e é
    /// cumulativo desde o início da conexão; este mede a subida desta conexão
    /// numa janela que desliza. Confundi-los faria a interface acusar a rede
    /// errada.
    pub perda_de_subida: Option<f32>,
    /// The last thing worth surfacing.
    pub notice: Option<Notice>,
    /// What became of the files this client has been moving, oldest first.
    ///
    /// A queue rather than one slot, unlike [`Self::notice`]: two transfers can
    /// be in the air at once, and the second one failing must not erase the
    /// reason the first one did. A shell drains it.
    pub transfers: Vec<TransferNotice>,
    /// Quem está compartilhando tela em cada sala de voz.
    ///
    /// Uma transmissão por sala de voz, que é o §6 item 3 da spec de compartilhamento
    /// de tela: *«duas dobram a subida de quem recebe e triplicam a
    /// interface»*. Um mapa e não um campo em [`VoiceRoomInfo`] porque o servidor
    /// **não** pôs a transmissão lá — ele reenvia `ScreenShareStarted` a quem
    /// entra numa sala que já tem uma —, e um campo que só o reenvio preenche
    /// seria um campo mentindo no aperto de mão.
    pub telas: HashMap<VoiceRoomId, Tela>,
    /// A subida que o servidor mediu da própria máquina, em bits por segundo.
    ///
    /// A primeira linha do teto do §5.1 — `caminho de quem HOSPEDA × 60% ÷ N` —
    /// e a única perna que quem compartilha **não** consegue ver: os bytes saem
    /// da máquina de quem hospeda e não da dele.
    ///
    /// `None` é «não medi», e o `HostUplink` diz isso com um zero. Traduzir o
    /// zero para `None` aqui, na entrada, é a única maneira de o resto do
    /// produto não ter de lembrar do sentinela: um zero que escapasse daqui
    /// viraria um teto de zero bits por segundo, que é [`MotivoDeParada`]
    /// `AbaixoDoPiso` — o compartilhamento parando **porque o servidor não
    /// mediu**, que é o oposto do que a ausência quer dizer.
    ///
    /// [`MotivoDeParada`]: crate::tela::MotivoDeParada
    pub caminho_de_quem_hospeda_bps: Option<u32>,
    /// O último pedido de quadro-chave recebido, para quem está compartilhando.
    ///
    /// Uma vaga só, e não uma fila: §3.3 conta o que um quadro-chave custa —
    /// 65 KiB em 1080p, 446 ms do orçamento inteiro —, e três pessoas pedindo
    /// no mesmo segundo querem **um** quadro-chave, não três. Quem lê, limpa.
    pub chave_pedida: Option<ChavePedida>,
    /// Set once the session is over.
    pub ended: Option<Ended>,
}

/// Uma transmissão de tela acontecendo num sala de voz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tela {
    /// Quantas pessoas estão recebendo esta transmissão, quem compartilha
    /// excluído.
    ///
    /// **É o N do §5.1, e é o que o servidor sobe.** O servidor encaminha os
    /// quadros, então o que a máquina dele levanta é `N × teto` — e a correção
    /// que aquela seção torna obrigatória divide a subida de quem hospeda por
    /// este número. Sem ele quem compartilha aplica um `min(...)` com uma perna
    /// que inventou, que é o defeito mais caro do §5.1.
    ///
    /// Zero até o `ScreenViewers` chegar, e não um: o um de
    /// [`crate::tela::TetoDeVideo`] é o piso da **divisão**, escolhido lá para
    /// que a entrada da primeira pessoa não dê um salto no teto. Repeti-lo aqui
    /// faria a interface escrever «1 pessoa assistindo» antes de haver uma.
    pub espectadores: u32,
    /// Quem está compartilhando.
    pub person: PersonId,
    /// Como a transmissão se chama.
    ///
    /// Atribuído pelo servidor, como o `ssrc`, e **diferente** dele: a tabela de
    /// `ssrc` → pessoa é sobre quem fala, e uma tela não é um falante.
    pub screen: ScreenId,
}

/// Alguém que está assistindo não tem de que predizer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChavePedida {
    /// Qual transmissão.
    pub screen: ScreenId,
    /// Quem pediu.
    ///
    /// Carregado porque quem compartilha pode segurar um fluxo por espectador,
    /// e porque é a única maneira de distinguir uma pessoa pedindo duas vezes
    /// por segundo de a sala inteira perdendo quadros.
    pub person: PersonId,
}

/// What changed, so a shell can redraw only what it must.
///
/// Returned rather than inferred: a shell that has to diff two snapshots to
/// find out a message arrived is a shell doing the core's work again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Changed {
    /// Somebody joined, left, or changed state.
    pub roster: bool,
    /// A message arrived, changed, or went away.
    pub messages: bool,
    /// voice_rooms or Channels changed.
    pub channels: bool,
    /// The server renamed itself, or changed its picture.
    ///
    /// Its own flag and not folded into [`Self::channels`]: what moves is the
    /// header and the badge, not the room lists, and a shell that redrew every
    /// channel because the server was renamed would be redrawing the one part
    /// of the screen that did not change.
    pub server: bool,
    /// New measurements.
    pub telemetry: bool,
    /// A notice was raised.
    pub notice: bool,
    /// A transfer was refused, or a file asked for is not coming.
    pub transfers: bool,
    /// Uma transmissão de tela começou, acabou, ou pediram quadro-chave.
    ///
    /// Bandeira própria e não dobrada em [`Self::roster`]: o que se mexe é o
    /// painel da tela, e uma casca que redesenhasse a lista de pessoas inteira
    /// a cada pedido de quadro-chave estaria redesenhando justamente a parte
    /// que não mudou.
    pub telas: bool,
    /// The session ended.
    pub ended: bool,
}

impl Changed {
    /// Whether anything at all changed.
    #[must_use]
    pub fn any(self) -> bool {
        self.roster
            || self.messages
            || self.channels
            || self.server
            || self.telemetry
            || self.notice
            || self.transfers
            || self.telas
            || self.ended
    }
}

impl Room {
    /// A room that has not heard anything yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seeds from what the handshake already established.
    ///
    /// The handshake consumes the `Session` message while establishing PADRÃO:
    /// AZUL, so no shell ever sees it go by. Without this a client that
    /// connected perfectly shows an empty server — which looks exactly like a
    /// server with no voice_rooms.
    ///
    /// `nickname` is what this client asked to be called. It has to be passed
    /// in because the server announces arrivals to everybody *else*: nothing on
    /// the wire ever tells this client who it is.
    pub fn adopt(&mut self, info: &SessionInfo, nickname: &str) {
        self.me = Some(info.person);
        self.ssrc = Some(info.ssrc);
        self.server = info.server.clone();
        // Cleared, and not left alone, because this runs again on every
        // reconnection. The handshake describes the server from scratch and the
        // picture arrives just behind it when there is one; keeping the old one
        // here would leave a client that was away while the icon was taken down
        // drawing it for the rest of the session.
        self.icon = None;
        // As imagens de perfil, pela mesma razão que o ícone do servidor logo
        // acima: quem estava fora enquanto alguém tirou a sua continuaria
        // desenhando a antiga, e o servidor reapresenta as que existem logo
        // depois do aperto de mão. Guardá-las seria confiar num quadro que
        // pode não vir.
        self.person_icons.clear();
        self.voice_rooms = info.voice_rooms.clone();
        self.channels = info.channels.clone();
        self.permissions = info.permissions.clone();
        // Limpas pelo mesmo motivo do ícone, e o estrago aqui é maior: uma
        // conexão nova não tem fluxo de tela nenhum — o `Client` que os
        // carregava morreu com ela —, então uma transmissão herdada seria a
        // interface prometendo uma tela que não tem por onde chegar. O servidor
        // reenvia `ScreenShareStarted` a quem entra num sala de voz que está
        // transmitindo, então o que é verdade volta sozinho.
        self.telas.clear();
        self.chave_pedida = None;
        // Pelo mesmo motivo, e com o mesmo estrago: a medida é da máquina que
        // hospeda **esta** conexão, e uma conexão nova pode ser com outro
        // servidor. Carregar a subida do anterior seria dimensionar o teto pela
        // casa errada, que é o defeito que o §5.1 mandou corrigir.
        self.caminho_de_quem_hospeda_bps = None;
        // E pelo mesmo motivo outra vez: a perda de subida é o que **este**
        // enlace estava perdendo, e o enlace acabou. Herdá-la faria a interface
        // acusar a rede nova pelo que a velha fez, e — pior — o servidor novo
        // pode ser v1 e nunca mandar nada, deixando o número velho na tela para
        // sempre, sem nada que o contradiga.
        self.perda_de_subida = None;
        self.people.insert(
            info.person,
            Person::new(info.person, nickname.to_owned(), Some(info.ssrc)),
        );
    }

    /// Records that this person's connection is now in a voice room, and seats them in it.
    ///
    /// Called on the way *out*, when the client asks — not on the way in. The
    /// server confirms a voice room entry by silence, and a roster that waits for a
    /// message that never comes shows an empty room to the person sitting in it.
    pub fn enter_voice_room(&mut self, voice_room: VoiceRoomId) {
        if let Some(me) = self.me {
            if let Some(previous) = self.current_voice_room {
                if let Some(seats) = self.seats.get_mut(&previous) {
                    seats.retain(|seated| *seated != me);
                }
            }
            let seats = self.seats.entry(voice_room).or_default();
            if !seats.contains(&me) {
                seats.push(me);
            }
        }
        self.current_voice_room = Some(voice_room);
    }

    /// Records that this person's connection came **out**, and empties their seat.
    ///
    /// The other half of [`Self::enter_voice_room`], and it was missing for as long
    /// as that one existed. The server does not echo `PersonLeft` back to the
    /// person who caused it — "they already know" — so leaving, exactly like
    /// entering, is bookkeeping this side has to do for itself. Without it the
    /// server empties the seat, every other client empties the seat, and the
    /// one screen that goes on drawing the person in the voice room is the screen of
    /// the person who just left it.
    ///
    /// Idempotent: leaving a voice room nobody is in is not an error, it is a client
    /// asking twice.
    pub fn leave_voice_room(&mut self) {
        if let (Some(me), Some(previous)) = (self.me, self.current_voice_room) {
            if let Some(seats) = self.seats.get_mut(&previous) {
                seats.retain(|seated| *seated != me);
            }
        }
        self.current_voice_room = None;
    }

    /// Esquece o aviso que está na tela, porque alguém o leu e o fechou.
    ///
    /// # Por que o estado tem de mudar
    ///
    /// [`Self::notice`] é uma vaga que guarda a última coisa a dizer, e ela
    /// ficava cheia para sempre. Uma casca que desenhasse a partir dela e
    /// fechasse a caixa só do lado dela veria a caixa voltar no redesenho
    /// seguinte, e o redesenho seguinte vem duas vezes por segundo: o efeito é
    /// um alerta que não fecha nunca, e foi assim que apagar uma sala com
    /// alguém dentro deixou a janela inutilizável.
    ///
    /// Fechar é uma decisão de quem leu, e uma decisão que não muda estado
    /// nenhum não é uma decisão.
    ///
    /// Devolve `true` quando havia o que dispensar. Idempotente.
    pub fn dispensar_aviso(&mut self) -> bool {
        self.notice.take().is_some()
    }

    /// Records that the client is now reading o canal.
    ///
    /// Clears the messages, because a new Channel is a new conversation and keeping
    /// the old one under a new heading misattributes every channel of it.
    pub fn open_channel(&mut self, channel: ChannelId) {
        if self.current_channel != Some(channel) {
            self.messages.clear();
        }
        self.current_channel = Some(channel);
    }

    /// The people seated in a voice room, in arrival order.
    pub fn roster(&self, voice_room: VoiceRoomId) -> impl Iterator<Item = &Person> {
        self.seats
            .get(&voice_room)
            .into_iter()
            .flatten()
            .filter_map(|id| self.people.get(id))
    }

    /// The people in whichever voice room this client is in.
    pub fn current_roster(&self) -> impl Iterator<Item = &Person> {
        self.current_voice_room
            .into_iter()
            .flat_map(|voice_room| self.roster(voice_room))
    }

    /// The average Sync Ratio of a voice room, banded, or `None` if nobody is in it.
    ///
    /// # An empty voice room has no average
    ///
    /// Not zero. Zero is a measurement, and by the bands it is a critical one:
    /// a server with four idle voice_rooms would show four red rooms nobody is in,
    /// and the one voice room that is genuinely in trouble would stop standing out.
    /// `None` says "nothing to average", which is the truth, and leaves the
    /// shell to draw the absence — the comp draws `——` for a connection with no
    /// number, and this is the same case one level up.
    ///
    /// # An ejected person does not count
    ///
    /// Ejecting a connection leaves the voice room: the person comes out of `seats` — via
    /// [`Self::enter_voice_room`] for this client, or `PersonLeft` for anybody else —
    /// while staying in `people`, because their name is still needed for what
    /// they already said. This averages the *seats*, so somebody who left stops
    /// counting the moment they leave and cannot drag the room's number down
    /// from outside it. There is no third state: this client has no notion of a
    /// person who is in the voice room but ejected.
    #[must_use]
    pub fn voice_room_sync(&self, voice_room: VoiceRoomId) -> Option<VoiceRoomSync> {
        let mut total: u32 = 0;
        let mut people: usize = 0;
        for person in self.roster(voice_room) {
            total += u32::from(person.signal);
            people += 1;
        }

        let count = u32::try_from(people).ok()?;
        if count == 0 {
            return None;
        }

        // Integer arithmetic, ties up: `(2a + n) / 2n` is the mean rounded to
        // the nearest point without a float ever holding the value. A float
        // would round the same way here and invite a `.1` onto the screen
        // later, which is precision the datum does not carry.
        let mean = (2 * total + count) / (2 * count);
        let ratio = u8::try_from(mean).unwrap_or(u8::MAX);

        Some(VoiceRoomSync {
            ratio,
            band: SignalBand::of(ratio),
            people,
        })
    }

    /// The average for whichever voice room this client is in.
    #[must_use]
    pub fn current_voice_room_sync(&self) -> Option<VoiceRoomSync> {
        self.current_voice_room
            .and_then(|voice_room| self.voice_room_sync(voice_room))
    }

    /// A person's media source, addressed by nickname.
    #[must_use]
    pub fn ssrc_of(&self, nickname: &str) -> Option<Ssrc> {
        self.people
            .values()
            .find(|person| person.nickname.eq_ignore_ascii_case(nickname))
            .and_then(|person| person.ssrc)
    }

    /// The voice room matching a name or a number.
    #[must_use]
    pub fn find_voice_room(&self, which: &str) -> Option<VoiceRoomId> {
        if let Ok(number) = which.parse::<u32>() {
            if let Some(voice_room) = self
                .voice_rooms
                .iter()
                .find(|voice_room| voice_room.id.0 == number)
            {
                return Some(voice_room.id);
            }
        }
        let wanted = which.to_lowercase();
        self.voice_rooms
            .iter()
            .find(|voice_room| voice_room.name.to_lowercase().contains(&wanted))
            .map(|voice_room| voice_room.id)
    }

    /// The Channel matching a name or a number, with or without its `#`.
    #[must_use]
    pub fn find_channel(&self, which: &str) -> Option<ChannelId> {
        let wanted = which.trim_start_matches('#').to_lowercase();
        if let Ok(number) = wanted.parse::<u32>() {
            if let Some(channel) = self.channels.iter().find(|channel| channel.id.0 == number) {
                return Some(channel.id);
            }
        }
        self.channels
            .iter()
            .find(|channel| channel.name.to_lowercase().contains(&wanted))
            .map(|channel| channel.id)
    }

    /// The name to show for a person, even one never introduced.
    ///
    /// Joining o canal mid-conversation means the first thing said can come from
    /// somebody whose arrival was never seen. Showing an id beats dropping it.
    #[must_use]
    pub fn name_of(&self, person: PersonId) -> String {
        self.people
            .get(&person)
            .map_or_else(|| format!("pessoa {}", person.0), |p| p.nickname.clone())
    }

    /// A transmissão que **este** pessoa está compartilhando, se houver uma.
    ///
    /// Uma por sala de voz (§6 item 3), então achar a minha é achar aquela cujo dono
    /// sou eu — e não a da sala de voz em que estou, que pode ser a de outra pessoa.
    #[must_use]
    pub fn minha_tela(&self) -> Option<Tela> {
        let me = self.me?;
        self.telas.values().copied().find(|tela| tela.person == me)
    }

    /// O teto do vídeo com o que o fio já contou, pronto para a bomba.
    ///
    /// **É aqui que as três pernas do §5.1 se encontram**, e cada uma vem de um
    /// lugar diferente pelo motivo que aquela seção dá: a de quem hospeda vem do
    /// `HostUplink`, o N vem do `ScreenViewers`, e a escolha da pessoa vem da
    /// interface — sempre teto, nunca piso (§5).
    ///
    /// A perna de quem compartilha **entra por argumento**, e a `Room` não a
    /// guarda. Não é omissão: aquela perna é a única das três que vem de uma
    /// **medida em curso** — a [`crate::caminho::Sonda`] a tira do que o
    /// transporte contou enquanto a tela enchia o cano —, e esta estrutura é
    /// uma dobra de mensagens do fio. Guardá-la aqui poria um relógio dentro de
    /// uma coisa que não tem nenhum, e faria duas cópias dela: a do
    /// `crate::enlace`, que é quem mede, e esta, que estaria sempre um pouco
    /// atrás. Quem chama passa `crate::caminho::Sonda::estimativa`, e quem não
    /// mede nada passa [`crate::tela::CAMINHO_DA_PROVA_BPS`] — a suposição do
    /// §8, que continua sendo por onde se começa.
    ///
    /// Era a pergunta 2 do §8, e ela deixou de estar aberta: ver o cabeçalho de
    /// [`crate::caminho`].
    ///
    /// Ausência de medida do anfitrião **não** é zero: quando o `HostUplink`
    /// não chegou, ou chegou dizendo zero, a perna dele fica no cano das provas
    /// em vez de zerar o teto — ver [`Self::caminho_de_quem_hospeda_bps`]. E é
    /// essa perna, e não esta, que trava o teto em 1200 kbps num servidor que não
    /// declarou a própria subida, por mais que esta ponta meça: o `min` do §5.1
    /// é o menor dos três.
    #[must_use]
    pub fn teto_de_video(
        &self,
        caminho_bps: u32,
        escolha_bps: Option<u32>,
    ) -> crate::tela::TetoDeVideo {
        let mut teto = crate::tela::TetoDeVideo::com_caminho(caminho_bps);
        if let Some(medido) = self.caminho_de_quem_hospeda_bps {
            teto = teto.com_caminho_de_quem_hospeda(medido);
        }
        teto.com_espectadores(self.minha_tela().map_or(0, |tela| tela.espectadores))
            .com_escolha(escolha_bps)
    }

    /// Folds one message from the server into this room.
    #[allow(
        clippy::too_many_lines,
        reason = "one match over the protocol; splitting it hides which messages are handled"
    )]
    pub fn apply(&mut self, message: &ServerMessage) -> Changed {
        let mut changed = Changed::default();

        match message {
            ServerMessage::Session {
                person,
                ssrc,
                server,
                voice_rooms,
                channels,
                permissions,
                ..
            } => {
                self.me = Some(*person);
                self.ssrc = Some(*ssrc);
                self.server.clone_from(server);
                // For the reason [`Self::adopt`] gives: a handshake describes
                // the server from scratch, picture included.
                self.icon = None;
                self.voice_rooms.clone_from(voice_rooms);
                self.channels.clone_from(channels);
                self.permissions.clone_from(permissions);
                self.people
                    .entry(*person)
                    .or_insert_with(|| Person::new(*person, format!("pessoa {}", person.0), None))
                    .ssrc = Some(*ssrc);
                changed.channels = true;
                changed.roster = true;
            }

            ServerMessage::PersonJoined {
                voice_room,
                profile,
                ssrc,
            } => {
                let person = self
                    .people
                    .entry(profile.id)
                    .or_insert_with(|| Person::new(profile.id, profile.nickname.clone(), None));
                person.nickname.clone_from(&profile.nickname);
                person.ssrc = Some(*ssrc);

                let seats = self.seats.entry(*voice_room).or_default();
                if !seats.contains(&profile.id) {
                    seats.push(profile.id);
                }
                changed.roster = true;
            }

            ServerMessage::PersonLeft { voice_room, person } => {
                if let Some(seats) = self.seats.get_mut(voice_room) {
                    seats.retain(|seated| seated != person);
                }
                // The person stays in `people`: their name is still needed to
                // attribute everything they said before leaving.
                if let Some(known) = self.people.get_mut(person) {
                    known.speaking = false;
                }
                changed.roster = true;
            }

            ServerMessage::PersonPresent { profile, ssrc } => {
                let person = self
                    .people
                    .entry(profile.id)
                    .or_insert_with(|| Person::new(profile.id, profile.nickname.clone(), None));
                person.nickname.clone_from(&profile.nickname);
                person.ssrc = Some(*ssrc);
                if !self.presentes.contains(&profile.id) {
                    self.presentes.push(profile.id);
                }
                changed.roster = true;
            }

            ServerMessage::PersonGone { person } => {
                self.presentes.retain(|presente| presente != person);
                // Os assentos **não** são limpos aqui: o servidor manda um
                // `PersonLeft` por sala que a pessoa ocupava, e limpar nos dois
                // lugares poria duas autoridades sobre a mesma linha. O que se
                // limpa é o que só esta mensagem sabe.
                //
                // E ela continua em `people`, como em `PersonLeft`: o nome ainda
                // é preciso para atribuir tudo o que ela disse antes de sair.
                if let Some(known) = self.people.get_mut(person) {
                    known.speaking = false;
                }
                changed.roster = true;
            }

            ServerMessage::PersonState(state) => {
                self.absorb_state(state);
                changed.roster = true;
            }

            ServerMessage::MessageReceived {
                channel,
                id,
                author,
                at_seconds,
                author_nickname,
                body,
                replies_to,
                attachment,
                ..
            } => {
                // Idempotent by server id: a history fetch that overlaps what is
                // already on screen must not double every channel in the overlap.
                if self.messages.iter().any(|known| known.id == *id) {
                    return changed;
                }
                self.messages.push(Message {
                    id: *id,
                    channel: *channel,
                    author: *author,
                    // **O nome que veio com a mensagem, sempre.**
                    //
                    // Aqui o roster vencia: «somebody we watched arrive is
                    // somebody whose name we already learned». Aquilo era
                    // verdade enquanto o apelido de uma conta não mudava — e
                    // desde o `SetNickname` da 0.9.0 ele muda.
                    //
                    // Com o roster vencendo, trocar de nome reescreveria o
                    // passado: toda mensagem antiga passaria a mostrar o nome
                    // de hoje, e uma conversa em que alguém foi chamado pelo
                    // nome antigo ficaria citando alguém que não aparece nela.
                    // Quem desenha decidiu o contrário, e é o que preserva o
                    // registro.
                    //
                    // O servidor grava o apelido no instante em que a mensagem
                    // é publicada — `apelido_de_agora`, em `session.rs` —, então
                    // este campo é o nome de então, e é o que se mostra.
                    author_nickname: author_nickname.clone(),
                    at_seconds: *at_seconds,
                    body: body.clone(),
                    replies_to: *replies_to,
                    own: Some(*author) == self.me,
                    edited: false,
                    attachment: attachment.clone(),
                });
                // History arrives oldest-first per page, but a page fetched
                // after live messages have landed would otherwise sit at the
                // bottom. Sorting by the server's own ordering is the only
                // ordering every client agrees on.
                self.messages.sort_by_key(|message| message.id);
                changed.messages = true;
            }

            ServerMessage::MessageEdited { id, body, .. } => {
                if let Some(message) = self.messages.iter_mut().find(|known| known.id == *id) {
                    message.body.clone_from(body);
                    message.edited = true;
                    changed.messages = true;
                }
            }

            ServerMessage::MessageRemoved { id, .. } => {
                let before = self.messages.len();
                self.messages.retain(|message| message.id != *id);
                changed.messages = self.messages.len() != before;
            }

            ServerMessage::Telemetry(telemetry) => {
                self.telemetry = Some(telemetry.clone());
                changed.telemetry = true;
            }

            ServerMessage::Alert {
                severity,
                reason,
                operator_text,
            } => {
                self.notice = Some(Notice {
                    severity: *severity,
                    reason: *reason,
                    operator_text: operator_text.clone(),
                });
                changed.notice = true;
            }

            // ---- attachments ----
            //
            // Neither of these carries bytes; the bytes have their own stream.
            // What arrives here is the reason, which is the half a screen needs.
            ServerMessage::AttachmentRefused {
                client_message_id,
                reason,
            } => {
                self.transfers.push(TransferNotice::Refused {
                    client_message_id: *client_message_id,
                    reason: *reason,
                });
                changed.transfers = true;
            }

            ServerMessage::AttachmentUnavailable { attachment, reason } => {
                // Fold the answer back into the message, so the screen stops
                // offering a file that is gone without anybody having to fetch
                // the page again. The row on the server already says this; the
                // page this client is holding was drawn before it did.
                if *reason == seele_proto::control::AttachmentRefusal::Expired {
                    for message in &mut self.messages {
                        if let Some(carried) = &mut message.attachment {
                            if carried.id == *attachment {
                                carried.state = seele_proto::control::AttachmentState::Expired;
                                changed.messages = true;
                            }
                        }
                    }
                }
                self.transfers.push(TransferNotice::Unavailable {
                    attachment: *attachment,
                    reason: *reason,
                });
                changed.transfers = true;
            }

            ServerMessage::Disconnecting { reason } => {
                self.ended = Some(Ended { reason: *reason });
                changed.ended = true;
            }

            // A room somebody made while this client was already connected.
            //
            // Appended rather than re-sorted: the server sends these in the
            // order it made them and lists them in that same order at the next
            // handshake, so appending is what keeps the list this client is
            // looking at and the list it would get on reconnecting identical.
            //
            // Idempotent by identifier, like `MessageReceived` above. Nothing
            // sends one twice today; a client that reconnects and folds an old
            // event in would otherwise show the same room in the list twice, and
            // a duplicated room is one nobody can tell their friends to join.
            ServerMessage::VoiceRoomCreated { voice_room } => {
                if !self
                    .voice_rooms
                    .iter()
                    .any(|known| known.id == voice_room.id)
                {
                    self.voice_rooms.push(voice_room.clone());
                    changed.channels = true;
                }
            }

            ServerMessage::ChannelCreated { channel } => {
                if !self.channels.iter().any(|known| known.id == channel.id) {
                    self.channels.push(channel.clone());
                    changed.channels = true;
                }
            }

            ServerMessage::VoiceRoomRenamed { voice_room, name } => {
                if let Some(known) = self
                    .voice_rooms
                    .iter_mut()
                    .find(|known| known.id == *voice_room)
                {
                    known.name.clone_from(name);
                    changed.channels = true;
                }
            }

            ServerMessage::ChannelRenamed { channel, name } => {
                if let Some(known) = self.channels.iter_mut().find(|known| known.id == *channel) {
                    known.name.clone_from(name);
                    changed.channels = true;
                }
            }

            // What the server calls itself, changed while everybody is inside.
            //
            // Folded unconditionally, including when the value is the one
            // already held: the server only sends these when it committed a
            // change, and comparing here would mean this module deciding that a
            // rename to the same string is not news — which is a judgement
            // about a screen, and this module has none.
            ServerMessage::ServerRenamed { name } => {
                self.server.clone_from(name);
                changed.server = true;
            }

            ServerMessage::ServerIconChanged { icon } => {
                self.icon.clone_from(icon);
                changed.server = true;
            }

            // A imagem de alguém.
            //
            // Guardada num mapa por pessoa, e **não num campo de `Person`**,
            // porque `Person` é reconstruído a cada `PersonJoined` — quem sai
            // de uma sala e entra noutra vira uma linha nova, e a imagem
            // sobreviveria por acidente ou não sobreviveria de jeito nenhum,
            // conforme a ordem dos quadros. O mapa é por conta e dura o que a
            // sessão durar.
            //
            // `None` **apaga a entrada** em vez de guardar um `None`: quem
            // tirou a imagem não tem imagem, e um mapa que cresce com ausências
            // é um mapa que só cresce.
            // Alguém trocou de nome.
            //
            // **Só o roster muda.** O histórico guarda o apelido de quando cada
            // mensagem foi escrita, e é decisão de produto que continue
            // mostrando aquele: um histórico reescrito é um em que uma conversa
            // passa a citar um nome que ninguém usava quando ela aconteceu.
            ServerMessage::PersonRenamed { person, nickname } => {
                // Os assentos guardam identificadores; quem guarda o nome é o
                // mapa de pessoas. Um lugar só para trocar, que é o que impede
                // um roster com dois nomes para a mesma conta.
                if let Some(pessoa) = self.people.get_mut(person) {
                    pessoa.nickname.clone_from(nickname);
                }
                changed.roster = true;
            }

            ServerMessage::PersonIconChanged { person, icon } => {
                match icon {
                    Some(bytes) => {
                        self.person_icons.insert(*person, bytes.clone());
                    }
                    None => {
                        self.person_icons.remove(person);
                    }
                }
                changed.roster = true;
            }

            // Somebody with the permission moved this connection.
            //
            // The same bookkeeping [`Self::enter_voice_room`] does when *this* client
            // asks, and it has to be here as well as there: entering a voice room is
            // recorded on the way out, when the client sends the request, and a
            // move is a voice room this client never asked for. Without this the
            // person is drawn in the room they left, sending voice into it,
            // reading its roster.
            //
            // The sentence that says it happened arrives separately, as an
            // `Alert` carrying `MovedByOperator`. Two frames rather than one
            // because they are two different things: this is where the connection is,
            // that is what the person should be told, and only the shell knows
            // how to say the second.
            ServerMessage::MovedToVoiceRoom { voice_room } => {
                self.enter_voice_room(*voice_room);
                changed.roster = true;
            }

            // A room somebody destroyed. Dropped rather than marked, because
            // there is nothing left for a mark to be about: the confirmation in
            // front of the verb promised destruction, and a client that kept
            // the row greyed out would be the one screen in the product still
            // claiming the room is there.
            //
            // The seats go with it. Whoever was inside was turned out by the
            // server, which announces each of them as a `PersonLeft` — but the
            // announcement is per person and this map is per voice room, so a voice room
            // whose last `PersonLeft` was lost to a reconnection would leave
            // people seated in a room nobody can see. Clearing it here needs no
            // announcement to be complete.
            ServerMessage::VoiceRoomDeleted { voice_room } => {
                let before = self.voice_rooms.len();
                self.voice_rooms.retain(|known| known.id != *voice_room);
                self.seats.remove(voice_room);
                if self.current_voice_room == Some(*voice_room) {
                    // Not moved anywhere: this client is in na sala de voz now, which
                    // is the truth. Choosing another room for somebody would be
                    // putting them in a conversation they never asked to be in.
                    self.current_voice_room = None;
                }
                changed.channels = self.voice_rooms.len() != before;
                changed.roster = true;
            }

            // The same, and the messages go too. A Channel that is gone leaves no
            // conversation behind: keeping what was drawn would leave the last
            // page of a destroyed Channel readable under the heading of whatever
            // the shell shows next.
            ServerMessage::ChannelDeleted { channel } => {
                let before = self.channels.len();
                self.channels.retain(|known| known.id != *channel);
                changed.channels = self.channels.len() != before;
                if self.current_channel == Some(*channel) {
                    self.current_channel = None;
                    self.messages.clear();
                    changed.messages = true;
                }
            }

            // An answer to a question, and nothing about the room. It is read
            // where it was asked for — the shell holds it only as long as the
            // box it fills is open — so folding it into the room would be
            // storing a number whose whole value is being fresh.
            ServerMessage::ChannelWeighed { .. } => {}

            // ---- compartilhamento de tela ----
            //
            // O servidor manda `ScreenShareStarted` a **todo** mundo da sala de voz, quem
            // compartilha incluído: quem compartilha precisa do `ScreenId` para
            // poder abrir o fluxo, e todo o resto precisa saber que um fluxo
            // vem aí em vez de descobrir sendo entregue um.
            //
            // E manda de novo a quem **entra** num sala de voz que já tem transmissão.
            // É por isso que aqui não há caso especial nenhum: chegar depois e
            // estar lá desde o começo são a mesma mensagem, e um cliente que
            // tratasse os dois de maneiras diferentes teria duas maneiras de
            // errar.
            ServerMessage::ScreenShareStarted {
                voice_room,
                person,
                screen,
            } => {
                // A contagem sobrevive ao reenvio, e é por isso que ela é lida
                // antes de inserir: o servidor manda `ScreenShareStarted` de novo
                // a **cada** pessoa que entra num sala de voz que já transmite, e
                // zerar ali faria a interface piscar «0 assistindo» e o teto
                // subir por um instante — justamente no instante em que a sala
                // acabou de crescer. O `ScreenViewers` que vem atrás corrige o
                // número; o que ele não corrige é o teto que já saiu.
                let espectadores = self
                    .telas
                    .get(voice_room)
                    .filter(|tela| tela.screen == *screen)
                    .map_or(0, |tela| tela.espectadores);
                self.telas.insert(
                    *voice_room,
                    Tela {
                        espectadores,
                        person: *person,
                        screen: *screen,
                    },
                );
                changed.telas = true;
            }
            // Quantas pessoas estão recebendo, que é o N do §5.1.
            //
            // Procurado pelo `ScreenId` e não pelo sala de voz porque é assim que a
            // mensagem se endereça, e porque um `ScreenViewers` atrasado de uma
            // transmissão anterior não pode reescrever a contagem da que
            // começou depois — é o mesmo cuidado do `ScreenShareStopped` logo
            // acima, pelo mesmo motivo.
            ServerMessage::ScreenViewers { tela, quantos } => {
                if let Some(viva) = self
                    .telas
                    .values_mut()
                    .find(|conhecida| conhecida.screen == *tela)
                {
                    if viva.espectadores != *quantos {
                        viva.espectadores = *quantos;
                        changed.telas = true;
                    }
                }
            }
            // A subida de quem hospeda. **Zero é ausência, nunca zero bits.**
            //
            // A tradução acontece aqui, na entrada, e em nenhum outro lugar: é
            // o que faz o resto do produto poder ler
            // `caminho_de_quem_hospeda_bps` sem lembrar do sentinela. A
            // bandeira é `telas` e não `telemetry` porque o que se mexe com
            // isto é o painel da tela — o teto, e a frase que explica por que
            // ele apertou (§5.1) —, e não os números de RTT e perda que a
            // telemetria desenha.
            ServerMessage::HostUplink { bps } => {
                let medido = (*bps != 0).then_some(*bps);
                if self.caminho_de_quem_hospeda_bps != medido {
                    self.caminho_de_quem_hospeda_bps = medido;
                    changed.telas = true;
                }
            }
            // Sem motivo, e o §3.6 explica por quê: as duas maneiras de acabar
            // — alguém apertou parar, ou quem mandava sumiu — já se distinguem
            // por tudo o mais que acontece. Quem foi embora produz um
            // `PersonLeft`; quem continua na sala parou de propósito.
            ServerMessage::ScreenShareStopped { voice_room, screen } => {
                // Conferido antes de tirar: um `ScreenShareStopped` atrasado de
                // uma transmissão anterior não pode apagar a que começou
                // depois. As mensagens chegam em ordem no fluxo de controle,
                // mas o `ScreenId` é o que torna essa garantia verificável em
                // vez de assumida.
                if self
                    .telas
                    .get(voice_room)
                    .is_some_and(|tela| tela.screen == *screen)
                {
                    self.telas.remove(voice_room);
                    changed.telas = true;
                }
            }
            // Só chega a quem está compartilhando.
            ServerMessage::KeyFrameRequested { screen, person } => {
                self.chave_pedida = Some(ChavePedida {
                    screen: *screen,
                    person: *person,
                });
                changed.telas = true;
            }

            // Quanto da nossa voz não chega. Só o servidor pode medi-la, e
            // ele a manda só para quem a produziu. Ver o ADR 0036.
            ServerMessage::UplinkLoss { fraction } => {
                self.perda_de_subida = Some(*fraction);
                changed.telemetry = true;
            }

            // Consumed by the handshake and by the round-trip measurement, both
            // of which are over before any shell is watching.
            ServerMessage::Challenge { .. } | ServerMessage::Pong { .. } => {}
        }

        changed
    }

    fn absorb_state(&mut self, state: &PersonState) {
        let person = self.people.entry(state.person).or_insert_with(|| {
            Person::new(state.person, format!("pessoa {}", state.person.0), None)
        });
        person.muted = state.muted;
        person.total_isolation = state.total_isolation;
        person.speaking = state.speaking;
        person.signal = state.signal;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use seele_proto::control::{
        AlertReason, AlertSeverity, DisconnectReason, PersonProfile, Presence, Telemetry,
    };
    use seele_proto::ids::{ScreenId, SessionId};

    const VOICE_ROOM: VoiceRoomId = VoiceRoomId(1);
    const CHANNEL: ChannelId = ChannelId(1);

    fn session() -> ServerMessage {
        ServerMessage::Session {
            id: SessionId(1),
            person: PersonId(7),
            ssrc: Ssrc(700),
            server: "Casa".into(),
            voice_rooms: vec![VoiceRoomInfo {
                id: VOICE_ROOM,
                name: "SALA-01 CENTRAL".into(),
                limit: 20,
                password_required: false,
                channel: Some(CHANNEL),
            }],
            channels: vec![ChannelInfo {
                id: CHANNEL,
                name: "geral".into(),
            }],
            roles: Vec::new(),
            permissions: Vec::new(),
        }
    }

    fn joined(id: u64, nickname: &str) -> ServerMessage {
        ServerMessage::PersonJoined {
            voice_room: VOICE_ROOM,
            profile: PersonProfile {
                id: PersonId(id),
                nickname: nickname.into(),
                roles: Vec::new(),
            },
            ssrc: Ssrc(u32::try_from(id * 10).expect("ssrc")),
        }
    }

    /// Uma mensagem, com o apelido de quem a escreveu **no instante em que a
    /// escreveu** — que é o que o servidor grava e manda.
    ///
    /// O nome vinha como `pessoa {author}` e o `Room` o trocava pelo do roster.
    /// Aquilo escondia o que este campo é: desde o `SetNickname` da 0.9.0 o
    /// apelido de uma conta muda, e a mensagem guarda o de então. Uma fixture
    /// que manda um nome que ninguém usa testa o caminho errado.
    fn dito_por(id: u64, author: u64, apelido: &str, body: &str) -> ServerMessage {
        ServerMessage::MessageReceived {
            channel: CHANNEL,
            id: MessageId(id),
            author: PersonId(author),
            author_nickname: apelido.into(),
            // Deliberately unrelated to the id: the server's clock can tie or
            // go backwards across a restart, and ordering must not depend on it.
            at_seconds: 1_700_000_000 - i64::try_from(id).expect("at"),
            body: body.into(),
            replies_to: None,
            client_message_id: None,
            attachment: None,
        }
    }

    /// O mesmo, quando o nome de quem falou não é o assunto do teste.
    fn said(id: u64, author: u64, body: &str) -> ServerMessage {
        dito_por(id, author, &format!("pessoa {author}"), body)
    }

    fn room() -> Room {
        let mut room = Room::new();
        room.apply(&session());
        room.enter_voice_room(VOICE_ROOM);
        room.open_channel(CHANNEL);
        room
    }

    #[test]
    fn the_session_message_fills_in_the_server_and_the_channels() {
        let mut room = Room::new();
        let changed = room.apply(&session());

        assert!(changed.channels);
        assert_eq!(room.server, "Casa");
        assert_eq!(room.voice_rooms.len(), 1);
        assert_eq!(room.channels.len(), 1);
        assert_eq!(room.me, Some(PersonId(7)));
    }

    #[test]
    fn the_person_is_seated_in_their_own_roster() {
        // The server announces arrivals to everybody else, so nothing on the
        // wire ever names this client to itself. The one person missing from
        // the roster would be the person reading it.
        let mut room = Room::new();
        let info = SessionInfo {
            id: SessionId(1),
            person: PersonId(7),
            ssrc: Ssrc(700),
            server: "Casa".into(),
            voice_rooms: Vec::new(),
            channels: Vec::new(),
            permissions: Vec::new(),
        };
        room.adopt(&info, "marcela");
        room.enter_voice_room(VOICE_ROOM);

        let names: Vec<&str> = room
            .current_roster()
            .map(|person| person.nickname.as_str())
            .collect();
        assert_eq!(names, ["marcela"]);
    }

    #[test]
    fn moving_between_voice_rooms_does_not_leave_a_copy_behind() {
        let mut room = room();
        room.enter_voice_room(VoiceRoomId(2));

        assert_eq!(room.roster(VOICE_ROOM).count(), 0);
        assert_eq!(room.roster(VoiceRoomId(2)).count(), 1);
    }

    #[test]
    fn a_person_who_leaves_keeps_their_name_for_what_they_already_said() {
        // Dropping the name with the person would turn every channel they wrote
        // into "pessoa 3" the moment they close their client.
        let mut room = room();
        room.apply(&joined(3, "marcela"));
        // Com o apelido de verdade, que é o que o servidor grava e manda. O
        // `said` genérico só serve onde o nome de quem falou não é o assunto.
        room.apply(&dito_por(1, 3, "marcela", "verificando harmônicos"));
        room.apply(&ServerMessage::PersonLeft {
            voice_room: VOICE_ROOM,
            person: PersonId(3),
        });

        assert_eq!(room.current_roster().count(), 1, "only us should be left");
        assert_eq!(room.name_of(PersonId(3)), "marcela");
        assert_eq!(room.messages[0].author_nickname, "marcela");
    }

    #[test]
    fn trocar_de_nome_nao_reescreve_o_que_ja_foi_dito() {
        // **A decisão de quem desenha o produto, em 2026-09-01:** o histórico
        // mantém o nome antigo.
        //
        // O contrário — o roster vencendo — reescreveria o passado: toda
        // mensagem antiga passaria a mostrar o nome de hoje, e uma conversa em
        // que alguém foi chamado pelo nome antigo ficaria citando alguém que
        // não aparece nela.
        let mut room = room();
        room.apply(&joined(3, "marcela"));
        room.apply(&dito_por(1, 3, "marcela", "olá"));

        let mudou = room.apply(&ServerMessage::PersonRenamed {
            person: PersonId(3),
            nickname: "marcela lima".into(),
        });

        assert!(mudou.roster, "o roster mudou e ninguém foi avisado");
        assert_eq!(
            room.name_of(PersonId(3)),
            "marcela lima",
            "o roster tem o nome novo"
        );
        assert_eq!(
            room.messages[0].author_nickname, "marcela",
            "o que já foi dito passou a citar um nome que não existia quando foi dito"
        );

        // E o que ela disser **depois** sai com o nome novo, porque é o
        // servidor que carimba cada mensagem ao publicá-la.
        room.apply(&dito_por(2, 3, "marcela lima", "de novo"));
        assert_eq!(room.messages[1].author_nickname, "marcela lima");
    }

    #[test]
    fn the_same_message_arriving_twice_is_stored_once() {
        // A history fetch that overlaps what is already on screen must not
        // double every channel in the overlap.
        let mut room = room();
        room.apply(&joined(3, "marcela"));

        let first = room.apply(&said(1, 3, "olá"));
        let second = room.apply(&said(1, 3, "olá"));

        assert!(first.messages);
        assert!(!second.messages, "a duplicate was reported as a change");
        assert_eq!(room.messages.len(), 1);
    }

    #[test]
    fn messages_end_up_in_the_servers_order_however_they_arrive() {
        // A page of history fetched after live messages landed would otherwise
        // sit underneath them, which reads as the past happening last.
        let mut room = room();
        room.apply(&said(10, 7, "recente"));
        room.apply(&said(2, 7, "antigo"));
        room.apply(&said(5, 7, "meio"));

        let bodies: Vec<&str> = room.messages.iter().map(|m| m.body.as_str()).collect();
        assert_eq!(bodies, ["antigo", "meio", "recente"]);
    }

    #[test]
    fn an_edit_rewrites_in_place_instead_of_appending() {
        let mut room = room();
        room.apply(&said(1, 7, "sync caiu"));
        let changed = room.apply(&ServerMessage::MessageEdited {
            channel: CHANNEL,
            id: MessageId(1),
            body: "sync voltou".into(),
        });

        assert!(changed.messages);
        assert_eq!(room.messages.len(), 1, "the edit appended a second channel");
        assert_eq!(room.messages[0].body, "sync voltou");
        assert!(room.messages[0].edited);
    }

    #[test]
    fn editing_something_not_on_screen_changes_nothing() {
        let mut room = room();
        let changed = room.apply(&ServerMessage::MessageEdited {
            channel: CHANNEL,
            id: MessageId(99),
            body: "fantasma".into(),
        });

        assert!(!changed.messages);
        assert!(room.messages.is_empty());
    }

    #[test]
    fn a_removal_takes_the_message_off_the_screen() {
        let mut room = room();
        room.apply(&said(1, 7, "apagar isto"));
        let changed = room.apply(&ServerMessage::MessageRemoved {
            channel: CHANNEL,
            id: MessageId(1),
        });

        assert!(changed.messages);
        assert!(room.messages.is_empty());
    }

    #[test]
    fn our_own_messages_are_marked_as_ours() {
        let mut room = room();
        room.apply(&joined(3, "marcela"));
        room.apply(&said(1, 3, "deles"));
        room.apply(&said(2, 7, "nosso"));

        assert!(!room.messages[0].own);
        assert!(room.messages[1].own);
    }

    #[test]
    fn a_message_carries_the_servers_clock_and_not_the_arrival_time() {
        // A page of history whose channels all claim to have been written the
        // moment the app opened has lost what makes it history.
        let mut room = room();
        room.apply(&said(1, 7, "olá"));
        assert_eq!(room.messages[0].at_seconds, 1_700_000_000 - 1);
    }

    #[test]
    fn ordering_follows_the_id_and_not_the_clock() {
        // The server's wall clock can tie, and can go backwards across a
        // restart or an NTP step. Ids are monotonic; that is why they exist.
        let mut room = room();
        room.apply(&said(3, 7, "terceiro"));
        room.apply(&said(1, 7, "primeiro"));
        room.apply(&said(2, 7, "segundo"));

        let bodies: Vec<&str> = room.messages.iter().map(|m| m.body.as_str()).collect();
        assert_eq!(bodies, ["primeiro", "segundo", "terceiro"]);
    }

    #[test]
    fn a_message_from_somebody_never_introduced_is_still_kept() {
        let mut room = room();
        room.apply(&said(1, 99, "olá"));

        assert_eq!(room.messages.len(), 1);
        assert_eq!(room.messages[0].author_nickname, "pessoa 99");
    }

    #[test]
    fn state_updates_reach_the_person_they_are_about() {
        let mut room = room();
        room.apply(&joined(3, "marcela"));
        room.apply(&ServerMessage::PersonState(PersonState {
            person: PersonId(3),
            muted: true,
            total_isolation: false,
            speaking: true,
            presence: Presence::Available,
            signal: 42,
        }));

        let person = room.people.get(&PersonId(3)).expect("person");
        assert!(person.muted);
        assert!(person.speaking);
        assert_eq!(person.signal, 42);
    }

    #[test]
    fn an_unmeasured_sync_ratio_reads_as_zero_and_not_as_perfect() {
        // A hundred that nobody measured looks like an answer. A zero looks
        // like a question, which is what it is.
        let mut room = room();
        room.apply(&joined(3, "marcela"));
        assert_eq!(room.people[&PersonId(3)].signal, 0);
    }

    #[test]
    fn changing_the_line_clears_what_belonged_to_the_old_one() {
        let mut room = room();
        room.apply(&said(1, 7, "na linha 1"));
        room.open_channel(ChannelId(2));

        assert!(room.messages.is_empty());
        assert_eq!(room.current_channel, Some(ChannelId(2)));
    }

    #[test]
    fn opening_the_line_already_open_keeps_the_conversation() {
        // Re-entering the same Channel — a reconnection, a redraw — must not throw
        // the conversation away.
        let mut room = room();
        room.apply(&said(1, 7, "ainda aqui"));
        room.open_channel(CHANNEL);

        assert_eq!(room.messages.len(), 1);
    }

    #[test]
    fn telemetry_and_notices_are_reported_as_separate_changes() {
        // A shell that redraws the roster because the round trip moved is a
        // shell redrawing thirty times a second for nothing.
        let mut room = room();

        let changed = room.apply(&ServerMessage::Telemetry(Telemetry {
            rtt_ms: 38.0,
            jitter_ms: 12.0,
            loss_fraction: 0.002,
            subsystems: Vec::new(),
        }));
        assert!(changed.telemetry);
        assert!(!changed.roster);
        assert!(!changed.messages);

        let changed = room.apply(&ServerMessage::Alert {
            severity: AlertSeverity::Critical,
            reason: AlertReason::PermissionDenied,
            operator_text: None,
        });
        assert!(changed.notice);
        assert!(!changed.telemetry);
    }

    #[test]
    fn the_end_of_a_session_is_recorded_with_its_reason() {
        let mut room = room();
        let changed = room.apply(&ServerMessage::Disconnecting {
            reason: DisconnectReason::Banned,
        });

        assert!(changed.ended);
        assert_eq!(
            room.ended.map(|end| end.reason),
            Some(DisconnectReason::Banned)
        );
    }

    #[test]
    fn a_handshake_message_changes_nothing_a_shell_would_redraw() {
        let mut room = room();
        assert!(!room.apply(&ServerMessage::Pong { timestamp: 1 }).any());
        assert!(!room
            .apply(&ServerMessage::Challenge { nonce: vec![1, 2] })
            .any());
    }

    #[test]
    fn a_voice_room_is_found_by_number_or_by_part_of_its_name() {
        let room = room();
        assert_eq!(room.find_voice_room("1"), Some(VOICE_ROOM));
        assert_eq!(room.find_voice_room("central"), Some(VOICE_ROOM));
        assert_eq!(room.find_voice_room("CENTRAL"), Some(VOICE_ROOM));
        assert_eq!(room.find_voice_room("geofront"), None);
    }

    #[test]
    fn a_line_is_found_with_or_without_its_hash() {
        let room = room();
        assert_eq!(room.find_channel("#geral"), Some(CHANNEL));
        assert_eq!(room.find_channel("geral"), Some(CHANNEL));
        assert_eq!(room.find_channel("1"), Some(CHANNEL));
    }

    #[test]
    fn a_media_source_is_addressable_by_nickname() {
        // What `:volume marcela 40` needs, in the one place that knows both.
        let mut room = room();
        room.apply(&joined(3, "marcela"));

        assert_eq!(room.ssrc_of("marcela"), Some(Ssrc(30)));
        assert_eq!(room.ssrc_of("MARCELA"), Some(Ssrc(30)));
        assert_eq!(room.ssrc_of("ninguém"), None);
    }

    /// Seats a person in `VOICE_ROOM` and gives them a Sync Ratio.
    fn seated_with(room: &mut Room, id: u64, nickname: &str, sync: u8) {
        room.apply(&joined(id, nickname));
        room.apply(&ServerMessage::PersonState(PersonState {
            person: PersonId(id),
            muted: false,
            total_isolation: false,
            speaking: false,
            presence: Presence::Available,
            signal: sync,
        }));
    }

    /// The voice room seen by somebody whose own connection is elsewhere, so the fixture's
    /// own zero does not have to be reasoned about in every average.
    fn watching() -> Room {
        let mut room = Room::new();
        room.apply(&session());
        room
    }

    #[test]
    fn an_empty_voice_room_has_no_average_rather_than_a_zero() {
        // Zero is a measurement, and by the bands a critical one. Four idle
        // voice_rooms drawn in red is four alarms nobody set, and the voice room that is
        // genuinely in trouble stops standing out among them.
        let room = room();
        assert_eq!(
            room.voice_room_sync(VoiceRoomId(9)),
            None,
            "a voice room nobody is in"
        );
        assert_eq!(
            watching().voice_room_sync(VOICE_ROOM),
            None,
            "a voice room nobody has sat in"
        );

        // And the person's own voice room, before anybody is in it, has no average
        // either — including no average of nobody.
        let mut alone = watching();
        assert_eq!(
            alone.current_voice_room_sync(),
            None,
            "na sala de voz entered yet"
        );
        alone.enter_voice_room(VOICE_ROOM);
        assert_eq!(
            alone.current_voice_room_sync().map(|sync| sync.people),
            Some(1),
            "the person sitting in it is one person"
        );
    }

    #[test]
    fn the_average_is_the_seated_persons_rounded_to_a_whole_point() {
        // 90 + 81 + 78 = 249, over three, is 83 exactly. The band is read off
        // the average and not off the members: two of these three are nominal
        // on their own, and together the room is degraded.
        let mut room = watching();
        seated_with(&mut room, 3, "marcela", 90);
        seated_with(&mut room, 4, "carla", 81);
        seated_with(&mut room, 5, "rafael", 78);

        let average = room
            .voice_room_sync(VOICE_ROOM)
            .expect("three people are seated");
        assert_eq!(average.ratio, 83);
        assert_eq!(average.band, SignalBand::Degraded);
        assert_eq!(average.people, 3);
    }

    #[test]
    fn a_half_point_rounds_up_and_never_lands_between_two_numbers() {
        // 84 and 85 average to 84.5. The comp would print 84.5; the datum is a
        // u8 everywhere it exists, so the answer is 85 — and 85 is the nominal
        // floor, which is exactly the tie where guessing changes the colour of
        // the room.
        let mut room = watching();
        seated_with(&mut room, 3, "marcela", 84);
        seated_with(&mut room, 4, "carla", 85);

        let average = room
            .voice_room_sync(VOICE_ROOM)
            .expect("two people are seated");
        assert_eq!(average.ratio, 85);
        assert_eq!(average.band, SignalBand::Nominal);
        assert_eq!(average.people, 2);
        assert_eq!(
            room.current_voice_room_sync(),
            None,
            "our connection is elsewhere"
        );
    }

    #[test]
    fn a_person_who_ejected_stops_counting_towards_the_room() {
        // Ejecting leaves the seats and keeps the name. Somebody who walked out
        // with a dying connection must not go on dragging the room's number
        // down from outside it — and this client has no third state where a
        // person is in the voice room but ejected.
        let mut room = watching();
        seated_with(&mut room, 3, "marcela", 90);
        seated_with(&mut room, 4, "carla", 20);

        assert_eq!(
            room.voice_room_sync(VOICE_ROOM).map(|sync| sync.ratio),
            Some(55)
        );

        room.apply(&ServerMessage::PersonLeft {
            voice_room: VOICE_ROOM,
            person: PersonId(4),
        });

        let average = room
            .voice_room_sync(VOICE_ROOM)
            .expect("marcela is still seated");
        assert_eq!(average.ratio, 90, "somebody who left is still counted");
        assert_eq!(average.people, 1);
        assert_eq!(
            room.name_of(PersonId(4)),
            "carla",
            "the name left with the seat"
        );
    }

    // ---- rooms made while this client was already connected ----

    #[test]
    fn a_voice_room_made_now_joins_the_list_without_a_reconnection() {
        // The whole point of announcing it. Folding this into the next
        // handshake only would mean the person who made the room has to tell
        // everybody to reconnect before they can walk into it.
        let mut room = Room::new();
        room.apply(&session());
        assert_eq!(room.voice_rooms.len(), 1);

        let changed = room.apply(&ServerMessage::VoiceRoomCreated {
            voice_room: VoiceRoomInfo {
                id: VoiceRoomId(2),
                name: "SALA-02 SALA DOS FUNDOS".into(),
                limit: 8,
                password_required: false,
                channel: None,
            },
        });

        assert!(
            changed.channels,
            "nothing told the shell to redraw the list"
        );
        assert_eq!(room.voice_rooms.len(), 2);
        assert_eq!(room.voice_rooms[1].id, VoiceRoomId(2));
        assert_eq!(room.voice_rooms[1].name, "SALA-02 SALA DOS FUNDOS");
    }

    #[test]
    fn a_line_made_now_joins_the_list_without_a_reconnection() {
        let mut room = Room::new();
        room.apply(&session());

        let changed = room.apply(&ServerMessage::ChannelCreated {
            channel: ChannelInfo {
                id: ChannelId(2),
                name: "planejamento".into(),
            },
        });

        assert!(changed.channels);
        assert_eq!(room.channels.len(), 2);
        assert_eq!(room.channels[1].name, "planejamento");
    }

    #[test]
    fn the_same_room_announced_twice_appears_once() {
        // Idempotent by identifier, like `MessageReceived`. A room listed twice
        // is a room nobody can tell a friend to join by name.
        let mut room = Room::new();
        room.apply(&session());
        let made = ServerMessage::VoiceRoomCreated {
            voice_room: VoiceRoomInfo {
                id: VoiceRoomId(2),
                name: "SALA-02".into(),
                limit: 8,
                password_required: false,
                channel: None,
            },
        };

        room.apply(&made);
        let again = room.apply(&made);

        assert_eq!(room.voice_rooms.len(), 2);
        assert!(
            !again.channels,
            "a repeat told the shell to redraw a list that did not change"
        );
    }

    #[test]
    fn renaming_the_server_reaches_a_client_that_never_asked() {
        // ADR 0032 names the failure this prevents: the screen of whoever
        // renamed the server showing the new name and everybody else's showing
        // the old one until they reconnect.
        let mut room = Room::new();
        room.apply(&session());
        assert_eq!(room.server, "Casa");

        let changed = room.apply(&ServerMessage::ServerRenamed {
            name: "Quartel-General".into(),
        });

        assert_eq!(room.server, "Quartel-General");
        assert!(changed.server, "the header was not told to redraw");
        assert!(
            !changed.channels,
            "renaming the server told the shell to redraw both channel lists,              which are the part of the screen that did not change"
        );
    }

    #[test]
    fn the_server_icon_arrives_and_can_be_taken_down_again() {
        // Both halves. An `Option` folded in only when it is `Some` is the
        // classic version of this bug: the picture goes up and never comes
        // down, so it stays on the screen after it has left the database.
        let mut room = Room::new();
        room.apply(&session());
        assert_eq!(room.icon, None, "a fresh room invented a picture");

        let bytes = vec![0x89, b'P', b'N', b'G', 1, 2, 3];
        let changed = room.apply(&ServerMessage::ServerIconChanged {
            icon: Some(bytes.clone()),
        });
        assert_eq!(room.icon, Some(bytes));
        assert!(changed.server);

        let changed = room.apply(&ServerMessage::ServerIconChanged { icon: None });
        assert_eq!(
            room.icon, None,
            "taking the picture down did not reach here"
        );
        assert!(changed.server);
    }

    #[test]
    fn a_handshake_forgets_the_picture_the_last_one_left() {
        // What makes "the server sends the icon only when it has one" honest.
        // A reconnection re-describes the server from scratch, and a client that
        // was away while the picture was taken down would otherwise draw it for
        // the rest of the session — the server says nothing, because there is
        // nothing to say.
        let mut room = Room::new();
        room.apply(&session());
        room.apply(&ServerMessage::ServerIconChanged {
            icon: Some(vec![0x89, b'P', b'N', b'G']),
        });

        room.apply(&session());

        assert_eq!(room.icon, None, "the old picture survived the handshake");
    }

    #[test]
    fn a_rename_keeps_the_room_in_its_place() {
        // The alternative — remove and re-append — reads, to everybody
        // watching, as the room being destroyed and a new one made, and moves
        // it under the cursor of whoever was about to click it.
        let mut room = Room::new();
        room.apply(&session());
        room.apply(&ServerMessage::VoiceRoomCreated {
            voice_room: VoiceRoomInfo {
                id: VoiceRoomId(2),
                name: "SALA-02".into(),
                limit: 8,
                password_required: false,
                channel: None,
            },
        });

        let changed = room.apply(&ServerMessage::VoiceRoomRenamed {
            voice_room: VOICE_ROOM,
            name: "SALA-01 PONTE".into(),
        });

        assert!(changed.channels);
        assert_eq!(room.voice_rooms.len(), 2, "the rename made a second room");
        assert_eq!(room.voice_rooms[0].id, VOICE_ROOM);
        assert_eq!(room.voice_rooms[0].name, "SALA-01 PONTE");
        assert_eq!(
            room.voice_rooms[0].limit, 20,
            "the rename rewrote the rest of it"
        );
    }

    #[test]
    fn renaming_a_room_this_client_never_heard_of_changes_nothing() {
        let mut room = Room::new();
        room.apply(&session());

        let changed = room.apply(&ServerMessage::ChannelRenamed {
            channel: ChannelId(404),
            name: "fantasma".into(),
        });

        assert!(!changed.channels);
        assert_eq!(room.channels.len(), 1);
        assert_eq!(room.channels[0].name, "geral");
    }

    // ---- rooms destroyed while this client was looking at them ----

    #[test]
    fn a_destroyed_voice_room_leaves_the_list_and_takes_its_seats_with_it() {
        // Dropped rather than greyed out. The confirmation in front of the verb
        // promised destruction; a client still drawing the row would be the one
        // screen in the product claiming the room is there.
        let mut room = room();
        room.apply(&joined(3, "marcela"));
        assert_eq!(
            room.roster(VOICE_ROOM).count(),
            2,
            "this client and marcela"
        );

        let changed = room.apply(&ServerMessage::VoiceRoomDeleted {
            voice_room: VOICE_ROOM,
        });

        assert!(
            changed.channels,
            "nothing told the shell to redraw the list"
        );
        assert!(
            room.voice_rooms.is_empty(),
            "the destroyed voice room is still listed"
        );
        assert_eq!(
            room.roster(VOICE_ROOM).count(),
            0,
            "people are still seated in a room nobody can see"
        );
    }

    #[test]
    fn the_plug_comes_out_of_a_voice_room_that_no_longer_exists() {
        // And lands nowhere. Choosing another room for somebody would put them
        // in a conversation they never asked to be in — with a live microphone.
        let mut room = room();
        assert_eq!(room.current_voice_room, Some(VOICE_ROOM));

        room.apply(&ServerMessage::VoiceRoomDeleted {
            voice_room: VOICE_ROOM,
        });

        assert_eq!(room.current_voice_room, None);
        assert_eq!(room.current_roster().count(), 0);
    }

    #[test]
    fn destroying_some_other_voice_room_leaves_this_plug_where_it_is() {
        // The half that a `retain` over the wrong field would break silently:
        // most of these announcements are about a room this client is not in.
        let mut room = room();
        room.apply(&ServerMessage::VoiceRoomCreated {
            voice_room: VoiceRoomInfo {
                id: VoiceRoomId(2),
                name: "SALA-02".into(),
                limit: 8,
                password_required: false,
                channel: None,
            },
        });

        let changed = room.apply(&ServerMessage::VoiceRoomDeleted {
            voice_room: VoiceRoomId(2),
        });

        assert!(changed.channels);
        assert_eq!(room.voice_rooms.len(), 1);
        assert_eq!(
            room.current_voice_room,
            Some(VOICE_ROOM),
            "the wrong connection came out"
        );
    }

    #[test]
    fn a_destroyed_line_takes_the_conversation_off_the_screen() {
        // Keeping what was drawn would leave the last page of a destroyed Channel
        // readable under the heading of whatever the shell shows next — which
        // is the one thing a verb that promises destruction may not do.
        let mut room = room();
        room.apply(&said(1, 3, "isto some junto"));
        assert_eq!(room.messages.len(), 1);

        let changed = room.apply(&ServerMessage::ChannelDeleted { channel: CHANNEL });

        assert!(changed.channels);
        assert!(changed.messages, "the shell was not told to clear the list");
        assert!(room.channels.is_empty());
        assert_eq!(room.current_channel, None);
        assert!(
            room.messages.is_empty(),
            "a destroyed Channel left its conversation on screen"
        );
    }

    #[test]
    fn destroying_a_room_this_client_never_heard_of_changes_nothing() {
        let mut room = room();
        let voice_room = room.apply(&ServerMessage::VoiceRoomDeleted {
            voice_room: VoiceRoomId(404),
        });
        let channel = room.apply(&ServerMessage::ChannelDeleted {
            channel: ChannelId(404),
        });

        assert!(!voice_room.channels);
        assert!(!channel.channels);
        assert_eq!(room.voice_rooms.len(), 1);
        assert_eq!(room.channels.len(), 1);
        assert_eq!(room.current_voice_room, Some(VOICE_ROOM));
        assert_eq!(room.current_channel, Some(CHANNEL));
    }

    #[test]
    fn the_weight_of_a_line_is_read_where_it_was_asked_for_and_not_stored() {
        // Its whole value is being fresh: it is the number in a sentence about
        // what is about to be destroyed, counted at the instant of asking. Kept
        // in the room, it would be a count somebody reads a minute later.
        let mut room = room();
        let before = room.clone();

        let changed = room.apply(&ServerMessage::ChannelWeighed {
            channel: CHANNEL,
            messages: 1_847,
            authors: 6,
            oldest_at_seconds: Some(1_678_600_000),
        });

        assert!(!changed.any(), "weighing o canal changed the room");
        assert_eq!(room.channels, before.channels);
        assert_eq!(room.messages.len(), before.messages.len());
    }

    // ---- moved by somebody else's hand ----

    #[test]
    fn being_moved_takes_the_plug_with_it() {
        // `enter_voice_room` is called on the way out, when this client asks. A move
        // is a voice room nobody here asked for, so without an arm of its own the
        // person stays drawn in the room they left — sending voice into it and
        // reading its roster — while everybody else sees them somewhere new.
        let mut room = room();
        room.apply(&joined(3, "marcela"));
        assert_eq!(room.current_voice_room, Some(VOICE_ROOM));

        let changed = room.apply(&ServerMessage::MovedToVoiceRoom {
            voice_room: VoiceRoomId(2),
        });

        assert!(changed.roster, "nothing told the shell to redraw");
        assert_eq!(room.current_voice_room, Some(VoiceRoomId(2)));
        assert_eq!(
            room.current_roster().count(),
            1,
            "the moved person is not in the room they were moved to"
        );
        assert_eq!(
            room.roster(VOICE_ROOM)
                .map(|p| p.nickname.as_str())
                .collect::<Vec<_>>(),
            ["marcela"],
            "a copy was left behind in the old voice room"
        );
    }

    #[test]
    fn the_session_carries_what_this_person_may_do() {
        // A shell asking "should this control exist" must not have to intersect
        // the role catalogue itself: "negadas vencem concedidas" is one rule and
        // it lives on the server.
        let mut room = Room::new();
        room.apply(&ServerMessage::Session {
            id: SessionId(1),
            person: PersonId(7),
            ssrc: Ssrc(700),
            server: "Casa".into(),
            voice_rooms: Vec::new(),
            channels: Vec::new(),
            roles: Vec::new(),
            permissions: vec![Permission::ManageVoiceRooms, Permission::Speak],
        });

        assert!(room.permissions.contains(&Permission::ManageVoiceRooms));
        assert!(!room.permissions.contains(&Permission::Ban));
    }
    // ---- compartilhamento de tela ----

    #[test]
    fn uma_transmissao_anunciada_fica_no_voice_room_e_some_quando_para() {
        let mut room = room();

        let changed = room.apply(&ServerMessage::ScreenShareStarted {
            voice_room: VOICE_ROOM,
            person: PersonId(9),
            screen: ScreenId(1),
        });
        assert!(changed.telas, "a bandeira da tela não subiu");
        // E só ela: uma casca que redesenhasse o roster inteiro por causa de
        // uma tela estaria redesenhando justamente o que não mudou.
        assert!(!changed.roster);
        assert_eq!(
            room.telas.get(&VOICE_ROOM),
            Some(&Tela {
                // Zero até o `ScreenViewers` chegar: quem começou a
                // compartilhar ainda não sabe se alguém está olhando.
                espectadores: 0,
                person: PersonId(9),
                screen: ScreenId(1)
            })
        );

        let changed = room.apply(&ServerMessage::ScreenShareStopped {
            voice_room: VOICE_ROOM,
            screen: ScreenId(1),
        });
        assert!(changed.telas);
        assert!(room.telas.is_empty());
    }

    /// **`HostUplink { bps: 0 }` é ausência, e nunca zero bits por segundo.**
    ///
    /// É o contrato escrito na própria mensagem, e o custo de errá-lo é grande e
    /// silencioso: um zero lido como medida vira um teto de zero, que é
    /// [`crate::tela::MotivoDeParada::AbaixoDoPiso`] — o compartilhamento
    /// parando **porque o servidor não mediu**, com uma frase na tela que
    /// culpa a conexão de quem está olhando para ela. É o mesmo contrato do
    /// `——` que o resto do produto usa onde não houve medida.
    ///
    /// Confira por mutação: troque `(*bps != 0).then_some(*bps)` por
    /// `Some(*bps)` e a primeira asserção fica vermelha.
    #[test]
    fn a_subida_de_quem_hospeda_com_zero_e_ausencia_e_nao_um_teto_de_zero() {
        let mut room = room();

        // Sem notícia nenhuma: a perna do anfitrião fica no cano das provas, que
        // é a única suposição com número atrás — e é ela, e não a de quem
        // compartilha, que continua sendo suposta neste ponto do §5.1.
        assert_eq!(room.caminho_de_quem_hospeda_bps, None);
        assert_eq!(
            room.teto_de_video(crate::tela::CAMINHO_DA_PROVA_BPS, None)
                .teto(SignalBand::Nominal),
            crate::tela::Teto::Bps(1_200_000)
        );

        // O servidor dizendo «não medi». O teto não pode se mexer.
        let changed = room.apply(&ServerMessage::HostUplink { bps: 0 });
        assert!(
            !changed.telas,
            "um zero que não muda nada não redesenha nada"
        );
        assert_eq!(room.caminho_de_quem_hospeda_bps, None);
        assert_eq!(
            room.teto_de_video(crate::tela::CAMINHO_DA_PROVA_BPS, None)
                .teto(SignalBand::Nominal),
            crate::tela::Teto::Bps(1_200_000),
            "o zero de «não medi» virou um teto de zero"
        );

        // E uma medida de verdade entra inteira.
        let changed = room.apply(&ServerMessage::HostUplink { bps: 6_000_000 });
        assert!(changed.telas);
        assert_eq!(room.caminho_de_quem_hospeda_bps, Some(6_000_000));
    }

    /// §5.1: **o N chega pelo fio e entra no teto pela perna de quem hospeda.**
    ///
    /// Antes desta mensagem quem compartilhava aplicava um `min(...)` com uma
    /// perna inventada, que é o defeito que aquela seção chama de o mais caro
    /// dela. A escada aqui é a mesma do `crate::video`: entra gente, a subida do
    /// anfitrião é dividida por mais um, e o degrau que o orçamento compra cai
    /// junto — sem que a contagem toque na resolução diretamente, que é o
    /// gatilho que o §5.1 recusa.
    #[test]
    fn os_espectadores_do_fio_apertam_o_teto_e_nao_a_voz() {
        let mut room = room();
        room.me = Some(PersonId(7));
        room.apply(&ServerMessage::HostUplink { bps: 6_000_000 });
        room.apply(&ServerMessage::ScreenShareStarted {
            voice_room: VOICE_ROOM,
            person: PersonId(7),
            screen: ScreenId(1),
        });

        // Sozinho: a subida do anfitrião dá 6 Mbps × 60% = 3,6, e quem manda é
        // a **outra** perna — a de quem compartilha, 2 Mbps × 60% = 1,2. Aqui
        // ela vale o cano das provas porque é o que a `crate::caminho::Sonda`
        // devolve antes da primeira janela cheia; numa sessão em curso é ela
        // que põe o número, e o resto desta conta não muda.
        assert_eq!(
            room.teto_de_video(crate::tela::CAMINHO_DA_PROVA_BPS, None)
                .teto(SignalBand::Nominal),
            crate::tela::Teto::Bps(1_200_000)
        );
        assert_eq!(
            room.teto_de_video(crate::tela::CAMINHO_DA_PROVA_BPS, None)
                .perna_que_aperta(SignalBand::Nominal),
            crate::tela::PernaQueAperta::QuemCompartilha
        );

        // Entram quatro: 3,6 ÷ 4 = 900 kbps, e agora quem aperta é a máquina
        // que sobe as quatro cópias. É a linha nova do §5.1, e sem o
        // `ScreenViewers` no fio ela não existiria.
        let changed = room.apply(&ServerMessage::ScreenViewers {
            tela: ScreenId(1),
            quantos: 4,
        });
        assert!(changed.telas, "a contagem mudou e o painel não soube");
        assert_eq!(room.minha_tela().map(|tela| tela.espectadores), Some(4));
        assert_eq!(
            room.teto_de_video(crate::tela::CAMINHO_DA_PROVA_BPS, None)
                .teto(SignalBand::Nominal),
            crate::tela::Teto::Bps(900_000),
            "as quatro cópias que o servidor sobe não foram divididas"
        );
        // E a razão é dizível, que é o que a tela escreve ao lado do degrau:
        // `720p · 4 pessoas assistindo`, e não `720p · sua conexão`.
        assert_eq!(
            room.teto_de_video(crate::tela::CAMINHO_DA_PROVA_BPS, None)
                .perna_que_aperta(SignalBand::Nominal),
            crate::tela::PernaQueAperta::QuemHospeda
        );
        // E a voz da máquina de quem compartilha nunca cedeu: os 40% do caminho
        // dela continuam de pé em toda a escada.
        assert_eq!(
            room.teto_de_video(crate::tela::CAMINHO_DA_PROVA_BPS, None)
                .reserva_da_voz(),
            800_000
        );

        // O reenvio de `ScreenShareStarted` — o servidor manda um a cada pessoa
        // que entra numa sala que já transmite — não pode zerar a contagem, ou
        // o teto subiria justamente no instante em que a sala cresceu.
        room.apply(&ServerMessage::ScreenShareStarted {
            voice_room: VOICE_ROOM,
            person: PersonId(7),
            screen: ScreenId(1),
        });
        assert_eq!(room.minha_tela().map(|tela| tela.espectadores), Some(4));

        // Um `ScreenViewers` de uma transmissão que não é esta não mexe nesta.
        let changed = room.apply(&ServerMessage::ScreenViewers {
            tela: ScreenId(404),
            quantos: 99,
        });
        assert!(!changed.telas);
        assert_eq!(room.minha_tela().map(|tela| tela.espectadores), Some(4));
    }

    #[test]
    fn um_fim_atrasado_nao_apaga_a_transmissao_que_comecou_depois() {
        // O `ScreenId` existe para isto. Sem a conferência, alguém que parou e
        // recomeçou depressa perderia a segunda transmissão para o aviso de fim
        // da primeira — e o sintoma seria uma tela que some sozinha logo depois
        // de aparecer, que é o defeito mais difícil de acreditar que existe.
        let mut room = room();
        room.apply(&ServerMessage::ScreenShareStarted {
            voice_room: VOICE_ROOM,
            person: PersonId(9),
            screen: ScreenId(2),
        });

        let changed = room.apply(&ServerMessage::ScreenShareStopped {
            voice_room: VOICE_ROOM,
            screen: ScreenId(1),
        });
        assert!(!changed.telas, "um fim de outra transmissão mexeu na sala");
        assert_eq!(
            room.telas.get(&VOICE_ROOM).map(|tela| tela.screen),
            Some(ScreenId(2))
        );
    }

    #[test]
    fn um_pedido_de_quadro_chave_fica_numa_vaga_so() {
        // Uma vaga e não uma fila: §3.3 conta que um quadro-chave de 1080p custa
        // 65 KiB, 446 ms do orçamento inteiro. Três pessoas pedindo no mesmo
        // segundo querem **um** quadro-chave, não três.
        let mut room = room();
        room.apply(&ServerMessage::KeyFrameRequested {
            screen: ScreenId(1),
            person: PersonId(9),
        });
        let changed = room.apply(&ServerMessage::KeyFrameRequested {
            screen: ScreenId(1),
            person: PersonId(11),
        });
        assert!(changed.telas);
        assert_eq!(
            room.chave_pedida,
            Some(ChavePedida {
                screen: ScreenId(1),
                person: PersonId(11)
            })
        );
    }

    #[test]
    fn reconectar_nao_herda_transmissao_nenhuma() {
        // A conexão é nova e os fluxos morreram com a antiga, então uma
        // transmissão herdada seria a interface prometendo uma tela que não tem
        // por onde chegar. O servidor reenvia o que é verdade logo em seguida.
        let mut room = room();
        room.apply(&ServerMessage::ScreenShareStarted {
            voice_room: VOICE_ROOM,
            person: PersonId(9),
            screen: ScreenId(1),
        });
        room.apply(&ServerMessage::KeyFrameRequested {
            screen: ScreenId(1),
            person: PersonId(11),
        });

        room.adopt(
            &SessionInfo {
                id: SessionId(2),
                person: PersonId(7),
                ssrc: Ssrc(700),
                server: "Casa".into(),
                voice_rooms: Vec::new(),
                channels: Vec::new(),
                permissions: Vec::new(),
            },
            "marcela",
        );
        assert!(room.telas.is_empty());
        assert_eq!(room.chave_pedida, None);
    }
}
