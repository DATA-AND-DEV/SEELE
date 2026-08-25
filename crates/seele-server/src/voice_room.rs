//! MEDIA — media routing.
//!
//! `specs/04-servidor-seele.md`:
//!
//! > 1. Receive a datagram from a known `ssrc`.
//! > 2. Validate that the sender is in the voice room and has permission to speak.
//! >    **Always validate** — do not trust the client.
//! > 3. Forward the payload intact to every other subscriber of the voice room.
//! > 4. Never decode the Opus.
//!
//! Never decoding is what keeps the server's CPU flat regardless of how many
//! people are talking, and it is the precondition that makes end-to-end
//! encryption an increment rather than a rewrite (`specs/01-arquitetura.md`).
//!
//! # One task per voice room
//!
//! `specs/04-servidor-seele.md`: "one task per **voice room**, owning that voice room's
//! state. In and out by `mpsc`. This eliminates the global lock and makes media
//! routing trivially parallel." No `Mutex` appears in this module.

use std::collections::HashMap;
use std::time::Instant;

use seele_proto::ids::{VoiceRoomId, PersonId, ScreenId, Ssrc};
use seele_proto::transport::MAX_FRAMES_PER_SECOND;
use tokio::sync::{broadcast, mpsc};

use crate::dogma::Event;
use crate::tela::{AberturaDeTela, Enquadramento, FimDaTela, Pedaco};

/// How many datagrams a voice room task will hold before it starts shedding.
///
/// Fifty a second per talker (`specs/03-audio.md`), so this is roughly a
/// second of a full voice room. A queue that grows past this is a task that has
/// stopped keeping up, and buffering more of it only adds latency to audio that
/// is already late.
const CHANNEL_DEPTH: usize = 1024;

/// What a connection asks its voice room to do.
pub enum VoiceRoomCommand {
    /// A person entered. `specs/07-tema-evangelion.md` calls it "inserir plug".
    Join {
        /// Who.
        person: PersonId,
        /// The media source the server assigned to their connection.
        ssrc: Ssrc,
        /// Whether they may transmit.
        may_speak: bool,
        /// Where to deliver the datagrams they should hear.
        outbound: mpsc::Sender<Vec<u8>>,
        /// Por onde se convida esta pessoa a assistir uma transmissão de tela.
        ///
        /// Ao lado do `outbound` e não dentro dele porque as duas mídias falham
        /// de maneiras opostas: o áudio **descarta** quando o ouvinte atrasa e
        /// a tela **corta**, porque um fluxo ordenado não perde bytes no meio
        /// sem deslocar o enquadramento de quem lê para sempre.
        /// `crate::tela::Pedaco` escreve isso por extenso.
        tela: mpsc::Sender<AberturaDeTela>,
    },
    /// A person left, or their connection dropped.
    Leave {
        /// Who.
        person: PersonId,
    },
    /// A datagram arrived from a connection.
    Datagram {
        /// Which connection sent it. Taken from the connection, **never** from
        /// the datagram. The two are compared before anything is forwarded,
        /// which is what stops one person being credited with another's audio.
        from: Ssrc,
        /// The bytes as received, header included.
        bytes: Vec<u8>,
    },

    // ---- compartilhamento de tela ----
    //
    // O §5.1 da spec de compartilhamento de tela, decidido em 22/08/2026: **o
    // servidor encaminha, como já faz com a voz.** Estes três comandos são o
    // mesmo caminho de [`Self::Datagram`] com a mídia trocada — e com uma
    // diferença que não é de gosto: a voz vai em datagrama e a tela vai em
    // fluxo, porque `spikes/tela-no-transporte` mediu 16,1% da voz perdida
    // quando as duas dividem a fila de datagramas (§3.1).
    /// Quem compartilha abriu o fluxo, e este é o cabeçalho dele.
    TelaAbriu {
        /// Quem. Vem da **sessão**, nunca do fluxo, pelo motivo que
        /// [`VoiceRoom::forward`] dá sobre o `ssrc`.
        from: PersonId,
        /// Como o Dogma batizou esta transmissão.
        screen: ScreenId,
        /// Os bytes do cabeçalho de abertura, para repassar a cada espectador.
        abertura: Vec<u8>,
        /// Por onde avisar quem compartilha se o Dogma encerrar por conta
        /// própria.
        fim: mpsc::Sender<FimDaTela>,
    },
    /// Bytes do fluxo de quem compartilha, como chegaram.
    TelaBytes {
        /// Quem.
        from: PersonId,
        /// Os bytes, sem nenhuma interpretação além de onde os quadros acabam.
        bytes: Vec<u8>,
    },
    /// O fluxo de quem compartilha terminou.
    TelaFechou {
        /// Quem.
        from: PersonId,
    },
}

/// Why a datagram was not forwarded.
///
/// Counted rather than logged per event: at fifty frames a second per talker, a
/// log line per drop is its own denial of service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DropCounts {
    /// The sender is not in this voice room.
    pub not_a_member: u64,
    /// The sender lacks [`seele_proto::control::Permission::Speak`].
    pub not_permitted: u64,
    /// The `ssrc` in the header did not match the connection that sent it.
    pub forged_ssrc: u64,
    /// The sender exceeded its frame budget.
    pub rate_limited: u64,
    /// The datagram was not a valid media frame.
    pub malformed: u64,
    /// A subscriber's queue was full.
    pub subscriber_lagging: u64,
    /// Uma segunda transmissão de tela tentou começar na mesma sala.
    ///
    /// §6 item 3: uma por sala de voz na v1. O aviso com nome
    /// (`AlertReason::ScreenShareTaken`) sai do plano de controle, que corre
    /// antes disto; este contador é a segunda parede, e ela conta o cliente que
    /// abriu o fluxo sem ter ganho a corrida.
    pub tela_ja_tomada: u64,
    /// Um espectador não acompanhou e teve a cópia dele cortada.
    pub espectador_cortado: u64,
    /// Um fluxo de tela chegou de quem não estava registrado transmitindo.
    pub tela_sem_dono: u64,
}

/// One member of a voice room.
struct Member {
    ssrc: Ssrc,
    may_speak: bool,
    outbound: mpsc::Sender<Vec<u8>>,
    /// This sender's media budget.
    ///
    /// A token bucket rather than the fixed one-second window this used to
    /// keep: a fixed window admits the whole limit at the end of one window and
    /// the whole limit at the start of the next — twice the contracted rate,
    /// and always at the same instant of the clock, which is the instant an
    /// attacker synchronises with. `crate::taxa` explains the choice once for
    /// the three places that limit anything.
    orcamento: crate::taxa::Balde,
    /// Por onde convidar esta pessoa a assistir. Ver [`VoiceRoomCommand::Join`].
    tela: mpsc::Sender<AberturaDeTela>,
}

/// A transmissão de tela que está passando por esta sala agora.
///
/// # Por que mora aqui e não ao lado de [`crate::dogma::Telas`]
///
/// `Telas` é o **plano de controle**: quem tem a vaga da sala, para responder
/// `ScreenShareTaken` a quem chega depois. Isto é o **plano de dados**, e ele
/// precisa de uma coisa que só a sala de voz tem sem perguntar a ninguém: **quem está
/// na sala neste instante**. O §5.1 transformou esse número, N, num termo do
/// teto — `caminho de quem hospeda × 60% ÷ N` — e quem encaminha é quem o sabe
/// primeiro, porque é o mesmo mapa de onde saem as cópias.
struct EmCurso {
    dono: PersonId,
    /// Como o Dogma batizou esta transmissão. Vai no convite de cada
    /// espectador; o cabeçalho de abertura o repete, porque é ele que atravessa
    /// o fio.
    screen: ScreenId,
    abertura: Vec<u8>,
    enquadramento: Enquadramento,
    canos: HashMap<PersonId, mpsc::Sender<Pedaco>>,
    /// Quem entrou na sala depois do começo e ainda espera um quadro-chave.
    esperando: Vec<PersonId>,
    fim: mpsc::Sender<FimDaTela>,
}

/// The state of one voice channel.
pub struct VoiceRoom {
    id: VoiceRoomId,
    members: HashMap<PersonId, Member>,
    /// Reverse index, so a datagram's sender is found without scanning.
    by_ssrc: HashMap<Ssrc, PersonId>,
    drops: DropCounts,
    forwarded: u64,
    /// A subida que se assume deste Dogma, em bits por segundo.
    ///
    /// Parâmetro e não constante para que o teto do §5.1 seja testável sem
    /// depender do número que ninguém mediu — ver
    /// [`crate::tela::CAMINHO_DO_DOGMA_BPS`].
    caminho_bps: u32,
    tela: Option<EmCurso>,
    /// Por onde o **N** desta sala chega ao plano de controle.
    ///
    /// A sala de voz é o único lugar do Dogma que sabe quem está na sala sem
    /// perguntar a ninguém, e o §5.1 fez desse número um termo do teto que as
    /// **duas** pontas calculam. Ou ele sai daqui, ou a outra ponta conta a
    /// mesma coisa de novo em outro lugar — e a segunda conta é a que fica
    /// errada primeiro.
    ///
    /// `None` numa sala sem barramento: a conta do teto é a mesma, e o que
    /// muda é só não haver quem escute. É o que os testes deste módulo usam.
    eventos: Option<broadcast::Sender<Event>>,
}

impl VoiceRoom {
    /// An empty voice room.
    #[must_use]
    pub fn new(id: VoiceRoomId) -> Self {
        Self::com_caminho(id, crate::tela::CAMINHO_DO_DOGMA_BPS, None)
    }

    /// A mesma sala, sobre uma subida de Dogma conhecida e com quem avisar.
    #[must_use]
    pub fn com_caminho(
        id: VoiceRoomId,
        caminho_bps: u32,
        eventos: Option<broadcast::Sender<Event>>,
    ) -> Self {
        Self {
            id,
            members: HashMap::new(),
            by_ssrc: HashMap::new(),
            drops: DropCounts::default(),
            forwarded: 0,
            caminho_bps,
            tela: None,
            eventos,
        }
    }

    /// Which voice room this is.
    #[must_use]
    pub fn id(&self) -> VoiceRoomId {
        self.id
    }

    /// How many people are inside.
    #[must_use]
    pub fn occupancy(&self) -> usize {
        self.members.len()
    }

    /// Datagrams forwarded so far, counting each copy sent.
    #[must_use]
    pub fn forwarded(&self) -> u64 {
        self.forwarded
    }

    /// Why datagrams were dropped.
    #[must_use]
    pub fn drops(&self) -> DropCounts {
        self.drops
    }

    /// Applies one command.
    pub fn handle(&mut self, command: VoiceRoomCommand) {
        self.handle_at(command, Instant::now());
    }

    /// Applies one command as of a given instant.
    ///
    /// The clock is a parameter so the rate limit can be tested at the edge —
    /// what happens at the last frame of the budget — without a `sleep` and
    /// without depending on how busy the machine is.
    pub fn handle_at(&mut self, command: VoiceRoomCommand, now: Instant) {
        match command {
            VoiceRoomCommand::Join {
                person,
                ssrc,
                may_speak,
                outbound,
                tela,
            } => {
                self.by_ssrc.insert(ssrc, person);
                self.members.insert(
                    person,
                    Member {
                        ssrc,
                        may_speak,
                        outbound,
                        orcamento: crate::taxa::Balde::novo(
                            MAX_FRAMES_PER_SECOND,
                            f64::from(MAX_FRAMES_PER_SECOND),
                            now,
                        ),
                        tela,
                    },
                );
                // Quem chega no meio de uma transmissão entra na fila, e não no
                // fluxo. Ligado num byte qualquer, o enquadramento dele ficaria
                // deslocado para sempre; ligado no começo do próximo
                // quadro-chave, ele acerta o passo e ainda consegue
                // decodificar. Quem entra pede um quadro-chave, e a onda 1 já
                // atende esse pedido.
                if let Some(curso) = self.tela.as_mut() {
                    if curso.dono != person {
                        curso.esperando.push(person);
                    }
                }
                self.reconferir_o_teto();
            }
            VoiceRoomCommand::Leave { person } => {
                if let Some(member) = self.members.remove(&person) {
                    self.by_ssrc.remove(&member.ssrc);
                }
                // A saída de quem compartilha mata a transmissão, e é o caminho
                // por onde **toda** saída passa: `salas de voz::leave_everywhere` é
                // chamado ao ejetar o plug, ao ser movido, ao a sala ser
                // destruída e em qualquer `?` do meio da sessão. Um
                // encaminhamento que sobrevivesse a isso seria um fluxo
                // bombeando para uma sala que já não tem de onde receber.
                let do_dono = self.tela.as_ref().is_some_and(|curso| curso.dono == person);
                if do_dono {
                    self.encerrar_tela(None);
                } else if let Some(curso) = self.tela.as_mut() {
                    curso.canos.remove(&person);
                    curso.esperando.retain(|quem| *quem != person);
                }
                // Depois de tirar o cano: sair da sala **devolve** teto a quem
                // ficou, e é a metade boa de N mudar.
                self.reconferir_o_teto();
            }
            VoiceRoomCommand::Datagram { from, bytes } => self.forward(from, &bytes, now),
            VoiceRoomCommand::TelaAbriu {
                from,
                screen,
                abertura,
                fim,
            } => self.tela_abriu(from, screen, abertura, fim),
            VoiceRoomCommand::TelaBytes { from, bytes } => self.tela_bytes(from, &bytes),
            VoiceRoomCommand::TelaFechou { from } => {
                if self.tela.as_ref().is_some_and(|curso| curso.dono == from) {
                    self.encerrar_tela(None);
                }
            }
        }
    }

    /// Quantas cópias uma transmissão desta sala teria de subir.
    ///
    /// Todo mundo menos quem compartilha. É o **N** do §5.1, e é este o número
    /// que o teto divide — não a ocupação da sala, que conta quem manda os
    /// bytes junto com quem os recebe.
    #[must_use]
    pub fn espectadores(&self) -> usize {
        self.members.len().saturating_sub(1)
    }

    /// Refaz a conta do §5.1 depois de N mudar, e para se ela não fecha.
    ///
    /// **Onde o número que só o encaminhador sabe é aplicado.** A subida deste
    /// Dogma é `N × teto`, então cada pessoa que entra na sala encolhe o teto
    /// de todo mundo; quando ele passa por baixo do piso do §2, a transmissão
    /// para com motivo, que é a escalada que o §3.2 escreve. A alternativa —
    /// continuar subindo o que a máquina não tem — é a sala inteira picotando
    /// por causa da tela, e essa é a única coisa que a spec chama de produto
    /// quebrado.
    fn reconferir_o_teto(&mut self) {
        if self.tela.is_none() {
            return;
        }
        if crate::tela::teto_do_hospedeiro(self.caminho_bps, self.espectadores()).is_none() {
            self.encerrar_tela(Some(FimDaTela::AlemDoQueOHospedeiroCarrega));
            return;
        }
        // O mesmo instante, e por isso está aqui e não ao lado de
        // `PersonJoined`: o N que encolhe o teto desta linha é o mesmo N que a
        // outra ponta precisa para encolher o dela. Contado em dois lugares,
        // ele passaria a discordar de si mesmo, e o §5.1 divide por ele.
        //
        // Depois da parada, nunca antes: uma transmissão que acabou de morrer
        // não anuncia quantos a assistem.
        self.anunciar_espectadores();
    }

    /// Põe o N desta sala no barramento, se há transmissão e há quem ouça.
    fn anunciar_espectadores(&self) {
        let (Some(curso), Some(eventos)) = (self.tela.as_ref(), self.eventos.as_ref()) else {
            return;
        };
        let _ = eventos.send(Event::ScreenViewers {
            voice_room: self.id,
            screen: curso.screen,
            // Um Dogma é dimensionado em cinquenta pessoas
            // (`specs/04-servidor-seele.md`), então isto nunca satura; saturar
            // ainda assim é melhor que dar a volta, porque um N pequeno demais
            // devolveria um teto grande demais.
            quantos: u32::try_from(self.espectadores()).unwrap_or(u32::MAX),
        });
    }

    /// Abre a transmissão e liga nela todo mundo que já está na sala.
    fn tela_abriu(
        &mut self,
        from: PersonId,
        screen: ScreenId,
        abertura: Vec<u8>,
        fim: mpsc::Sender<FimDaTela>,
    ) {
        // As mesmas duas perguntas que [`Self::forward`] faz do áudio, e pela
        // mesma razão: `specs/04-servidor-seele.md` manda validar sempre, e
        // quem não pode transmitir mídia nesta sala não passa a poder
        // transmitindo-a como imagem.
        let Some(member) = self.members.get(&from) else {
            self.drops.not_a_member += 1;
            return;
        };
        if !member.may_speak {
            self.drops.not_permitted += 1;
            return;
        }
        // Uma por sala (§6 item 3). A corrida já foi decidida no controle; isto
        // é a parede que não depende de o cliente ter respeitado a resposta.
        if self.tela.is_some() {
            self.drops.tela_ja_tomada += 1;
            return;
        }
        if crate::tela::teto_do_hospedeiro(self.caminho_bps, self.members.len().saturating_sub(1))
            .is_none()
        {
            let _ = fim.try_send(FimDaTela::AlemDoQueOHospedeiroCarrega);
            return;
        }
        self.tela = Some(EmCurso {
            dono: from,
            screen,
            abertura,
            enquadramento: Enquadramento::novo(),
            canos: HashMap::new(),
            esperando: Vec::new(),
            fim,
        });
        // Quem já está na sala entra do primeiro byte: o fluxo ainda não tem
        // byte nenhum, então não há passo a acertar.
        let ja_estao: Vec<PersonId> = self
            .members
            .keys()
            .copied()
            .filter(|quem| *quem != from)
            .collect();
        self.ligar(&ja_estao);
        // A primeira contagem, e ela tem de sair mesmo quando a sala está
        // vazia: quem compartilha para uma sala de uma pessoa precisa saber
        // que N é zero tanto quanto precisa saber que virou seis.
        self.anunciar_espectadores();
    }

    /// Liga estes espectadores na transmissão em curso.
    ///
    /// Um cano por pessoa e por transmissão. É o fechamento dele que diz a
    /// `crate::tela::bombear` se o fluxo terminou ou foi cortado, sem uma
    /// segunda bandeira que pudesse discordar do canal.
    fn ligar(&mut self, quem: &[PersonId]) {
        let mut cortados = 0_u64;
        let Some(curso) = self.tela.as_mut() else {
            return;
        };
        for person in quem {
            let Some(member) = self.members.get(person) else {
                continue;
            };
            let (tx, rx) = mpsc::channel(crate::tela::PEDACOS_DEPTH);
            let convite = AberturaDeTela {
                screen: curso.screen,
                abertura: curso.abertura.clone(),
                pedacos: rx,
            };
            // `try_send` e não `send`: a sala de voz é uma tarefa só e esperar por um
            // espectador seria parar a sala inteira por causa dele — o mesmo
            // raciocínio de `forward`, com a sanção trocada.
            if member.tela.try_send(convite).is_ok() {
                curso.canos.insert(*person, tx);
            } else {
                cortados += 1;
            }
        }
        self.drops.espectador_cortado += cortados;
    }

    /// Encaminha um pedaço do fluxo de quem compartilha para cada espectador.
    ///
    /// Byte por byte, sem tocar no conteúdo — `specs/04-servidor-seele.md` diz
    /// que o servidor nunca decodifica o Opus, e a imagem herda a regra e o
    /// motivo: é ela que mantém a CPU do Dogma plana e que deixa o E2EE de
    /// mídia ser um acréscimo. Os cinco bytes que o [`Enquadramento`] lê dizem
    /// onde um quadro acaba, e nada sobre o que há dentro dele.
    fn tela_bytes(&mut self, from: PersonId, bytes: &[u8]) {
        let Some(curso) = self.tela.as_mut() else {
            self.drops.tela_sem_dono += 1;
            return;
        };
        if curso.dono != from {
            self.drops.tela_sem_dono += 1;
            return;
        }
        let porta = match curso.enquadramento.entrada(bytes) {
            Ok(porta) => porta.filter(|_| !curso.esperando.is_empty()),
            Err(motivo) => {
                self.encerrar_tela(Some(motivo));
                return;
            }
        };
        // Quem esperava um quadro-chave entra exatamente no cabeçalho dele: o
        // que veio antes vai só para quem já assistia, e do quadro-chave em
        // diante todo mundo recebe o mesmo.
        let Some(corte) = porta else {
            self.escrever(bytes);
            return;
        };
        let entrando = std::mem::take(&mut curso.esperando);
        let (antes, depois) = bytes.split_at(corte.min(bytes.len()));
        self.escrever(antes);
        self.ligar(&entrando);
        self.escrever(depois);
    }

    /// Escreve o mesmo pedaço em cada cano ligado.
    fn escrever(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let Some(curso) = self.tela.as_mut() else {
            return;
        };
        let mut cortados = Vec::new();
        for (person, cano) in &curso.canos {
            if cano.try_send(Pedaco::Bytes(bytes.to_vec())).is_err() {
                cortados.push(*person);
            }
        }
        for person in &cortados {
            // Tirar o cano é o corte: `bombear` vê o canal fechar sem um
            // `Fim` e faz `reset` no fluxo daquela pessoa, que é a diferença
            // entre «a transmissão acabou» e «a sua cópia se perdeu».
            curso.canos.remove(person);
        }
        self.drops.espectador_cortado += cortados.len() as u64;
    }

    /// Acaba com a transmissão desta sala, com ou sem motivo.
    ///
    /// `None` é o fim honesto — quem mandava parou ou saiu —, e cada espectador
    /// recebe [`Pedaco::Fim`] para que o fluxo dele **termine** em vez de ser
    /// cortado. `Some` é o Dogma encerrando por conta própria, e o motivo sobe
    /// para a sessão de quem compartilha, que é quem tem como anunciá-lo.
    fn encerrar_tela(&mut self, motivo: Option<FimDaTela>) {
        let Some(curso) = self.tela.take() else {
            return;
        };
        for cano in curso.canos.values() {
            let _ = cano.try_send(Pedaco::Fim);
        }
        if let Some(motivo) = motivo {
            let _ = curso.fim.try_send(motivo);
        }
    }

    /// Validates a datagram and fans it out.
    ///
    /// `from` is the `ssrc` bound to the *connection*, which the server assigned
    /// at voice room entry. The `ssrc` inside the datagram is compared against it and
    /// a mismatch is refused.
    ///
    /// That comparison is gap G2 in `docs/plano-m0-m1.md`.
    /// `specs/08-seguranca.md` promises that a client "forging another's
    /// identity" is handled because "`ssrc` is assigned by the server, never
    /// accepted from the client" — but `specs/02-protocolo.md` also says the
    /// server "forwards intact", and nothing anywhere stated that the two must
    /// be checked against each other. Without this line a person could put
    /// somebody else's `ssrc` in their own datagrams and every listener would
    /// attribute the audio to the wrong person.
    fn forward(&mut self, from: Ssrc, bytes: &[u8], now: Instant) {
        let Some(person) = self.by_ssrc.get(&from).copied() else {
            self.drops.not_a_member += 1;
            return;
        };

        let Ok((header, _payload)) = seele_proto::MediaHeader::decode(bytes) else {
            self.drops.malformed += 1;
            return;
        };

        if header.ssrc != from.get() {
            self.drops.forged_ssrc += 1;
            return;
        }

        let Some(member) = self.members.get_mut(&person) else {
            self.drops.not_a_member += 1;
            return;
        };

        if !member.may_speak {
            self.drops.not_permitted += 1;
            return;
        }

        // specs/04-servidor-seele.md: a per-sender frames-per-second limit, so a
        // malicious client cannot saturate the voice room. Dropping rather than
        // disconnecting, as the spec says: audio that arrives too fast is a
        // stuttering sender far more often than an attack, and cutting somebody
        // off mid-sentence for it would be the wrong trade.
        if !member.orcamento.tentar(now) {
            self.drops.rate_limited += 1;
            return;
        }

        // Forward to everybody else. The payload is never touched: specs/04 says
        // the server never decodes Opus, which is what keeps its CPU flat and
        // leaves the door open for E2EE.
        let mut lagging = 0_u64;
        let mut delivered = 0_u64;
        for (other, subscriber) in &self.members {
            if *other == person {
                continue;
            }
            match subscriber.outbound.try_send(bytes.to_vec()) {
                Ok(()) => delivered += 1,
                // Dropping is correct: a subscriber whose queue is full is
                // already behind, and old audio helps nobody.
                Err(_) => lagging += 1,
            }
        }
        self.forwarded += delivered;
        self.drops.subscriber_lagging += lagging;
    }
}

/// Spawns a voice room on its own task and returns the handle to talk to it.
///
/// `specs/04-servidor-seele.md`: one task per voice room, owning its state, reached by
/// `mpsc`. Nothing shares a lock.
#[must_use]
pub fn spawn(
    id: VoiceRoomId,
    caminho_bps: u32,
    eventos: broadcast::Sender<Event>,
) -> mpsc::Sender<VoiceRoomCommand> {
    let (tx, mut rx) = mpsc::channel(CHANNEL_DEPTH);
    tokio::spawn(async move {
        let mut voice_room = VoiceRoom::com_caminho(id, caminho_bps, Some(eventos));
        while let Some(command) = rx.recv().await {
            voice_room.handle(command);
        }
        tracing::info!(voice_room = %id, forwarded = voice_room.forwarded(), "voice room closed");
    });
    tx
}

/// Every voice room task this Dogma is running.
///
/// # Why this had to exist the moment a Dogma could grow a second room
///
/// The Dogma used to spawn exactly one voice room task, at boot, for the one voice room in
/// `DogmaConfig` — and every session held that single sender. That was correct
/// while a Dogma had one room and *silently wrong* the instant it could have
/// two: two people in two different rooms would have had their datagrams
/// delivered to each other, because there was only ever one room to deliver
/// into. A voice channel that is not a channel is worse than a missing feature,
/// because it looks like it works.
///
/// # Lazily, not at boot
///
/// A voice room task is a channel and a `HashMap`; the cost of one nobody has entered
/// is not worth a boot-time scan of PERSISTENCE that would then be stale the first
/// time somebody made a room. The task appears the first time a person walks in
/// and lives until the Dogma stops.
pub struct VoiceRooms {
    tasks: tokio::sync::Mutex<HashMap<VoiceRoomId, mpsc::Sender<VoiceRoomCommand>>>,
    /// A subida deste Dogma, repassada a cada sala que nasce.
    ///
    /// Uma cópia só, e ela é a mesma que viaja no `HostUplink`: a sala divide
    /// este número por N e o cliente o divide de novo, e as duas contas têm de
    /// partir do mesmo lugar. Ver [`crate::tela::caminho_do_dogma`].
    caminho_bps: u32,
    eventos: broadcast::Sender<Event>,
}

impl VoiceRooms {
    /// A Dogma with na sala de voz task running yet.
    ///
    /// Sem `Default`, e de propósito: uma sala precisa saber quanto a subida
    /// deste Dogma carrega antes de deixar alguém transmitir nela, e um
    /// `Default` teria de inventar esse número.
    #[must_use]
    pub fn new(caminho_bps: u32, eventos: broadcast::Sender<Event>) -> Self {
        Self {
            tasks: tokio::sync::Mutex::new(HashMap::new()),
            caminho_bps,
            eventos,
        }
    }

    /// The way in to one voice room, starting its task if this is the first arrival.
    pub async fn of(&self, id: VoiceRoomId) -> mpsc::Sender<VoiceRoomCommand> {
        self.tasks
            .lock()
            .await
            .entry(id)
            .or_insert_with(|| spawn(id, self.caminho_bps, self.eventos.clone()))
            .clone()
    }

    /// Takes a person out of every voice room.
    ///
    /// Broadcast rather than aimed, and deliberately so. A session can end at
    /// any `?` in the middle of the loop, which is a path that does not know
    /// which room the person was in; tracking that separately would be a second
    /// copy of a fact, and the copy that goes stale is the one that leaves
    /// somebody's `ssrc` receiving audio in a room they left. `Leave` for a
    /// person who is not there is a no-op, and `specs/04-servidor-seele.md` sizes
    /// a Dogma at five active voice_rooms, so the fan-out is five sends.
    pub async fn leave_everywhere(&self, person: PersonId) {
        let tasks: Vec<mpsc::Sender<VoiceRoomCommand>> =
            self.tasks.lock().await.values().cloned().collect();
        for task in tasks {
            let _ = task.send(VoiceRoomCommand::Leave { person }).await;
        }
    }

    /// How many voice room tasks are running. For tests and for tooling.
    pub async fn running(&self) -> usize {
        self.tasks.lock().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use seele_proto::MediaHeader;

    /// Um conjunto de salas sobre o cano das duas provas, 2000 kbps.
    fn salas() -> VoiceRooms {
        let (eventos, _) = broadcast::channel(64);
        VoiceRooms::new(crate::tela::CAMINHO_DO_DOGMA_BPS, eventos)
    }

    fn datagram(ssrc: u32, seq: u16) -> Vec<u8> {
        let header = MediaHeader {
            version: seele_proto::PROTOCOL_VERSION,
            ssrc,
            seq,
            timestamp: u32::from(seq) * 960,
        };
        let mut out = vec![0_u8; 64];
        let len = header.encode_datagram(&[1, 2, 3, 4], &mut out).unwrap();
        out.truncate(len);
        out
    }

    /// Uma sala ligada ao barramento, e a ponta por onde se lê o que ela diz.
    fn sala_com_barramento(caminho_bps: u32) -> (VoiceRoom, broadcast::Receiver<Event>) {
        let (eventos, ouvinte) = broadcast::channel(64);
        (
            VoiceRoom::com_caminho(VoiceRoomId(1), caminho_bps, Some(eventos)),
            ouvinte,
        )
    }

    /// Os `ScreenViewers` que a sala anunciou até agora, em ordem.
    fn contagens(ouvinte: &mut broadcast::Receiver<Event>) -> Vec<u32> {
        let mut vistos = Vec::new();
        while let Ok(evento) = ouvinte.try_recv() {
            if let Event::ScreenViewers { quantos, .. } = evento {
                vistos.push(quantos);
            }
        }
        vistos
    }

    fn member(voice_room: &mut VoiceRoom, person: u64, ssrc: u32, may_speak: bool) -> mpsc::Receiver<Vec<u8>> {
        let (tx, rx) = mpsc::channel(64);
        let (tela, _) = mpsc::channel(4);
        voice_room.handle(VoiceRoomCommand::Join {
            person: PersonId(person),
            ssrc: Ssrc(ssrc),
            may_speak,
            outbound: tx,
            tela,
        });
        rx
    }

    /// Alguém que entra na sala e fica de olho no que chega **de tela**.
    fn espectador(voice_room: &mut VoiceRoom, person: u64) -> mpsc::Receiver<AberturaDeTela> {
        let (outbound, _) = mpsc::channel(64);
        let (tela, tela_rx) = mpsc::channel(crate::tela::ABERTURAS_DEPTH);
        voice_room.handle(VoiceRoomCommand::Join {
            person: PersonId(person),
            ssrc: Ssrc(person as u32 * 10),
            may_speak: true,
            outbound,
            tela,
        });
        tela_rx
    }

    /// O cabeçalho de abertura, como quem compartilha o escreve.
    fn abertura(screen: u32) -> Vec<u8> {
        let cabecalho = seele_proto::screen::ScreenHeader {
            version: seele_proto::PROTOCOL_VERSION,
            screen: ScreenId(screen),
            source: seele_proto::screen::ScreenSource::Monitor,
            codec: seele_proto::screen::ScreenCodec::H264Baseline,
            width: 1280,
            height: 720,
        };
        let mut bytes = vec![0_u8; seele_proto::screen::SCREEN_HEADER_LEN];
        let len = cabecalho.encode(&mut bytes).unwrap();
        bytes.truncate(len);
        bytes
    }

    /// Um quadro codificado, com os cinco bytes de enquadramento na frente.
    fn quadro(chave: bool, tamanho: usize) -> Vec<u8> {
        let mut bytes = vec![u8::from(chave)];
        bytes.extend_from_slice(&(tamanho as u32).to_be_bytes());
        bytes.extend(std::iter::repeat_n(0xAB, tamanho));
        bytes
    }

    /// Abre uma transmissão de `person` e devolve por onde o Dogma reclamaria.
    fn compartilhar(
        voice_room: &mut VoiceRoom,
        person: u64,
        screen: u32,
    ) -> mpsc::Receiver<crate::tela::FimDaTela> {
        let (fim, fim_rx) = mpsc::channel(1);
        voice_room.handle(VoiceRoomCommand::TelaAbriu {
            from: PersonId(person),
            screen: ScreenId(screen),
            abertura: abertura(screen),
            fim,
        });
        fim_rx
    }

    /// Tudo o que chegou num cano de espectador, achatado.
    fn recebido(convite: &mut AberturaDeTela) -> Vec<u8> {
        let mut tudo = Vec::new();
        while let Ok(pedaco) = convite.pedacos.try_recv() {
            if let Pedaco::Bytes(bytes) = pedaco {
                tudo.extend(bytes);
            }
        }
        tudo
    }

    #[test]
    fn a_datagram_reaches_everybody_but_its_sender() {
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let mut alice = member(&mut voice_room, 1, 100, true);
        let mut bob = member(&mut voice_room, 2, 200, true);
        let mut carol = member(&mut voice_room, 3, 300, true);

        voice_room.handle(VoiceRoomCommand::Datagram {
            from: Ssrc(100),
            bytes: datagram(100, 1),
        });

        assert!(bob.try_recv().is_ok(), "bob should hear alice");
        assert!(carol.try_recv().is_ok(), "carol should hear alice");
        assert!(alice.try_recv().is_err(), "alice must not hear herself");
        assert_eq!(voice_room.forwarded(), 2);
    }

    #[test]
    fn the_payload_is_forwarded_byte_for_byte() {
        // specs/04-servidor-seele.md: "never decodes the Opus". Rewriting even
        // one byte would break the E2EE path specs/08 sketches, where the server
        // can read the header and nothing else.
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let _alice = member(&mut voice_room, 1, 100, true);
        let mut bob = member(&mut voice_room, 2, 200, true);

        let original = datagram(100, 7);
        voice_room.handle(VoiceRoomCommand::Datagram {
            from: Ssrc(100),
            bytes: original.clone(),
        });

        assert_eq!(bob.try_recv().unwrap(), original);
    }

    #[test]
    fn a_forged_ssrc_is_refused() {
        // Gap G2. specs/08-seguranca.md promises that "a client forging another's
        // identity" is handled, but nothing said the header's ssrc had to be
        // checked against the connection's. Without this, Bob puts Alice's ssrc
        // in his datagrams and every listener credits her with his audio.
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let _alice = member(&mut voice_room, 1, 100, true);
        let _bob = member(&mut voice_room, 2, 200, true);
        let mut carol = member(&mut voice_room, 3, 300, true);

        voice_room.handle(VoiceRoomCommand::Datagram {
            from: Ssrc(200),         // the connection is Bob's
            bytes: datagram(100, 1), // the header claims Alice
        });

        assert!(carol.try_recv().is_err(), "a forged datagram was forwarded");
        assert_eq!(voice_room.drops().forged_ssrc, 1);
        assert_eq!(voice_room.forwarded(), 0);
    }

    #[test]
    fn a_person_without_permission_cannot_speak() {
        // specs/04-servidor-seele.md: "always validate — do not trust the
        // client". specs/07 calls the role that cannot speak an Observador.
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let _observer = member(&mut voice_room, 1, 100, false);
        let mut person = member(&mut voice_room, 2, 200, true);

        voice_room.handle(VoiceRoomCommand::Datagram {
            from: Ssrc(100),
            bytes: datagram(100, 1),
        });

        assert!(person.try_recv().is_err(), "an observer was forwarded");
        assert_eq!(voice_room.drops().not_permitted, 1);
    }

    #[test]
    fn a_stranger_is_refused() {
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let mut alice = member(&mut voice_room, 1, 100, true);

        voice_room.handle(VoiceRoomCommand::Datagram {
            from: Ssrc(999),
            bytes: datagram(999, 1),
        });

        assert!(alice.try_recv().is_err());
        assert_eq!(voice_room.drops().not_a_member, 1);
    }

    #[test]
    fn a_malformed_datagram_is_counted_not_forwarded() {
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let _alice = member(&mut voice_room, 1, 100, true);
        let mut bob = member(&mut voice_room, 2, 200, true);

        voice_room.handle(VoiceRoomCommand::Datagram {
            from: Ssrc(100),
            bytes: vec![0xFF; 3],
        });

        assert!(bob.try_recv().is_err());
        assert_eq!(voice_room.drops().malformed, 1);
    }

    #[test]
    fn a_flood_is_cut_off_at_the_documented_rate() {
        // specs/04-servidor-seele.md: an honest client sends 50/s; above the
        // limit, discard and log.
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let _alice = member(&mut voice_room, 1, 100, true);
        let mut bob = member(&mut voice_room, 2, 200, true);

        // One instant for the whole flood: the budget must come from elapsed
        // time, not from how long the loop took to run.
        let now = Instant::now();
        for seq in 0..(MAX_FRAMES_PER_SECOND * 3) {
            voice_room.handle_at(
                VoiceRoomCommand::Datagram {
                    from: Ssrc(100),
                    bytes: datagram(100, seq as u16),
                },
                now,
            );
        }

        let received = std::iter::from_fn(|| bob.try_recv().ok()).count();
        assert_eq!(
            received, MAX_FRAMES_PER_SECOND as usize,
            "the rate limit did not hold"
        );
        assert!(voice_room.drops().rate_limited > 0);
    }

    #[test]
    fn an_honest_sender_is_never_rate_limited() {
        // The other half: a limit that cuts off legitimate speech is worse than
        // no limit at all.
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let _alice = member(&mut voice_room, 1, 100, true);
        let mut bob = member(&mut voice_room, 2, 200, true);

        let start = Instant::now();
        for seq in 0..seele_proto::transport::NOMINAL_FRAMES_PER_SECOND {
            // Twenty milliseconds apart, which is what a 20 ms frame is.
            voice_room.handle_at(
                VoiceRoomCommand::Datagram {
                    from: Ssrc(100),
                    bytes: datagram(100, seq as u16),
                },
                start + std::time::Duration::from_millis(u64::from(seq) * 20),
            );
        }

        assert_eq!(voice_room.drops().rate_limited, 0);
        assert_eq!(
            std::iter::from_fn(|| bob.try_recv().ok()).count(),
            seele_proto::transport::NOMINAL_FRAMES_PER_SECOND as usize
        );
    }

    #[test]
    fn the_limit_has_no_edge_to_synchronise_with() {
        // What the fixed one-second window used to allow: the whole budget at
        // the end of one window and the whole budget at the start of the next,
        // twice the contracted rate inside a couple of milliseconds — and
        // always at the same instant of the clock, which is the instant an
        // attacker lines up with.
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let _alice = member(&mut voice_room, 1, 100, true);
        let mut bob = member(&mut voice_room, 2, 200, true);

        let start = Instant::now();
        let edge = start + std::time::Duration::from_millis(999);
        let just_after = start + std::time::Duration::from_millis(1_001);
        for (seq, when) in std::iter::repeat_n(edge, MAX_FRAMES_PER_SECOND as usize)
            .chain(std::iter::repeat_n(
                just_after,
                MAX_FRAMES_PER_SECOND as usize,
            ))
            .enumerate()
        {
            voice_room.handle_at(
                VoiceRoomCommand::Datagram {
                    from: Ssrc(100),
                    bytes: datagram(100, seq as u16),
                },
                when,
            );
        }

        let received = std::iter::from_fn(|| bob.try_recv().ok()).count();
        assert!(
            received <= MAX_FRAMES_PER_SECOND as usize + 2,
            "{received} frames passed across the window edge, and the limit is \
             {MAX_FRAMES_PER_SECOND}/s"
        );
    }

    #[test]
    fn leaving_stops_delivery_and_frees_the_ssrc() {
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let _alice = member(&mut voice_room, 1, 100, true);
        let mut bob = member(&mut voice_room, 2, 200, true);
        assert_eq!(voice_room.occupancy(), 2);

        voice_room.handle(VoiceRoomCommand::Leave { person: PersonId(2) });
        assert_eq!(voice_room.occupancy(), 1);

        voice_room.handle(VoiceRoomCommand::Datagram {
            from: Ssrc(100),
            bytes: datagram(100, 1),
        });
        assert!(bob.try_recv().is_err(), "a departed person still received");

        // The ssrc must be released, or a stale mapping outlives the session.
        voice_room.handle(VoiceRoomCommand::Datagram {
            from: Ssrc(200),
            bytes: datagram(200, 1),
        });
        assert_eq!(voice_room.drops().not_a_member, 1);
    }

    #[tokio::test]
    async fn two_rooms_do_not_hear_each_other() {
        // The whole reason [`voice_rooms`] exists. With one task for the whole Dogma
        // — which is what there was — a person in the room made at nine o'clock
        // and a person in the room made at ten would have been delivered each
        // other's audio, because there was only ever one room to deliver into.
        let voice_rooms = salas();
        let primeiro = voice_rooms.of(VoiceRoomId(1)).await;
        let segundo = voice_rooms.of(VoiceRoomId(2)).await;

        let (alice_tx, mut alice) = mpsc::channel(8);
        primeiro
            .send(VoiceRoomCommand::Join {
                person: PersonId(1),
                ssrc: Ssrc(100),
                may_speak: true,
                outbound: alice_tx,
                tela: mpsc::channel(4).0,
            })
            .await
            .unwrap();

        let (bob_tx, mut bob) = mpsc::channel(8);
        segundo
            .send(VoiceRoomCommand::Join {
                person: PersonId(2),
                ssrc: Ssrc(200),
                may_speak: true,
                outbound: bob_tx,
                tela: mpsc::channel(4).0,
            })
            .await
            .unwrap();

        segundo
            .send(VoiceRoomCommand::Datagram {
                from: Ssrc(200),
                bytes: datagram(200, 1),
            })
            .await
            .unwrap();

        // Long enough for a delivery that was going to happen to have happened.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            alice.try_recv().is_err(),
            "a person in voice room 1 heard somebody talking in voice room 2"
        );
        assert!(bob.try_recv().is_err(), "bob heard himself");
    }

    #[tokio::test]
    async fn the_same_voice_room_is_asked_for_twice_and_started_once() {
        // Two people walking into the same room must find the same room. A
        // registry that spawned per request would give each of them a private
        // copy of a voice room they both believe they are in.
        let voice_rooms = salas();
        let _ = voice_rooms.of(VoiceRoomId(1)).await;
        let _ = voice_rooms.of(VoiceRoomId(1)).await;
        let _ = voice_rooms.of(VoiceRoomId(2)).await;
        assert_eq!(voice_rooms.running().await, 2);
    }

    #[tokio::test]
    async fn leaving_everywhere_reaches_the_room_the_person_was_actually_in() {
        // A session can end at any `?`, on a path that does not know where the
        // person was sitting. Aiming the `Leave` at a remembered voice room would leave
        // a departed person's ssrc receiving audio whenever that memory was
        // wrong.
        let voice_rooms = salas();
        let sala = voice_rooms.of(VoiceRoomId(7)).await;

        let (alice_tx, mut alice) = mpsc::channel(8);
        sala.send(VoiceRoomCommand::Join {
            person: PersonId(1),
            ssrc: Ssrc(100),
            may_speak: true,
            outbound: alice_tx,
            tela: mpsc::channel(4).0,
        })
        .await
        .unwrap();
        let (bob_tx, _bob) = mpsc::channel(8);
        sala.send(VoiceRoomCommand::Join {
            person: PersonId(2),
            ssrc: Ssrc(200),
            may_speak: true,
            outbound: bob_tx,
            tela: mpsc::channel(4).0,
        })
        .await
        .unwrap();

        voice_rooms.leave_everywhere(PersonId(1)).await;

        sala.send(VoiceRoomCommand::Datagram {
            from: Ssrc(200),
            bytes: datagram(200, 1),
        })
        .await
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(
            alice.try_recv().is_err(),
            "a person whose session ended is still being delivered audio"
        );
    }

    // ---- compartilhamento de tela ----

    #[test]
    fn o_quadro_chega_a_cada_espectador_e_nunca_a_quem_compartilha() {
        // O §5.1 em uma linha: **o servidor encaminha, como já faz com a voz.**
        // É o gêmeo de `a_datagram_reaches_everybody_but_its_sender`, e a
        // segunda metade é a que prende o defeito caro: quem compartilha
        // recebendo a própria tela veria a si mesmo com o atraso da rede, que é
        // o efeito de espelho infinito que todo produto deste tipo já teve.
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let mut quem_compartilha = espectador(&mut voice_room, 1);
        let mut bob = espectador(&mut voice_room, 2);
        let mut carol = espectador(&mut voice_room, 3);
        let mut dave = espectador(&mut voice_room, 4);

        let _fim = compartilhar(&mut voice_room, 1, 7);
        assert_eq!(voice_room.espectadores(), 3);

        let mut convites: Vec<AberturaDeTela> = [&mut bob, &mut carol, &mut dave]
            .into_iter()
            .map(|quem| quem.try_recv().expect("cada espectador é convidado"))
            .collect();
        assert!(
            quem_compartilha.try_recv().is_err(),
            "quem compartilha foi convidado a assistir a si mesmo"
        );
        for convite in &convites {
            assert_eq!(convite.screen, ScreenId(7));
            assert_eq!(convite.abertura, abertura(7));
        }

        let primeiro = quadro(true, 40);
        voice_room.handle(VoiceRoomCommand::TelaBytes {
            from: PersonId(1),
            bytes: primeiro.clone(),
        });
        for convite in &mut convites {
            assert_eq!(
                recebido(convite),
                primeiro,
                "um espectador não recebeu o quadro inteiro, byte por byte"
            );
        }
        assert!(quem_compartilha.try_recv().is_err());
    }

    #[test]
    fn uma_transmissao_por_sala() {
        // §6 item 3. A corrida é decidida no controle, que responde
        // `ScreenShareTaken`; isto é a parede que não depende de o cliente ter
        // respeitado a resposta.
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let _alice = espectador(&mut voice_room, 1);
        let _bob = espectador(&mut voice_room, 2);
        let mut carol = espectador(&mut voice_room, 3);

        let _fim = compartilhar(&mut voice_room, 1, 7);
        let _tambem = compartilhar(&mut voice_room, 2, 8);

        assert_eq!(voice_room.drops().tela_ja_tomada, 1);
        // E carol continua vendo a primeira, não duas.
        assert_eq!(carol.try_recv().map(|c| c.screen), Ok(ScreenId(7)));
        assert!(carol.try_recv().is_err());
    }

    #[test]
    fn sair_da_sala_encerra_a_transmissao() {
        // O caminho por onde **toda** saída passa — ejetar o plug, ser movido,
        // a sala ser destruída, a conexão cair em qualquer `?`. Sem isto fica
        // um fluxo aberto na tela de quem assistia, prometendo imagem que já
        // não tem de onde vir.
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let _alice = espectador(&mut voice_room, 1);
        let mut bob = espectador(&mut voice_room, 2);

        let _fim = compartilhar(&mut voice_room, 1, 7);
        let mut convite = bob.try_recv().unwrap();

        voice_room.handle(VoiceRoomCommand::Leave { person: PersonId(1) });
        assert!(
            matches!(convite.pedacos.try_recv(), Ok(Pedaco::Fim)),
            "o espectador não foi avisado de que a transmissão acabou"
        );

        // E o encaminhamento morreu junto: o que chegar depois não vai a lugar
        // nenhum.
        voice_room.handle(VoiceRoomCommand::TelaBytes {
            from: PersonId(1),
            bytes: quadro(true, 8),
        });
        assert_eq!(voice_room.drops().tela_sem_dono, 1);
    }

    #[test]
    fn quem_entra_no_meio_so_e_ligado_num_quadro_chave() {
        // N muda no meio da transmissão, e é o §5.1 em movimento. Ligar alguém
        // num byte qualquer deslocaria o enquadramento dele para sempre: o
        // quadro seguinte leria o meio do anterior como cabeçalho.
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let _alice = espectador(&mut voice_room, 1);
        let mut bob = espectador(&mut voice_room, 2);
        let _fim = compartilhar(&mut voice_room, 1, 7);
        let mut de_bob = bob.try_recv().unwrap();

        let mut carol = espectador(&mut voice_room, 3);
        assert_eq!(voice_room.espectadores(), 2);
        assert!(
            carol.try_recv().is_err(),
            "quem entrou no meio foi ligado antes de haver onde entrar"
        );

        // Um quadro comum não abre a porta.
        let comum = quadro(false, 20);
        voice_room.handle(VoiceRoomCommand::TelaBytes {
            from: PersonId(1),
            bytes: comum.clone(),
        });
        assert!(carol.try_recv().is_err());

        // O quadro-chave abre, e ele chega inteiro a quem entrou.
        let chave = quadro(true, 30);
        voice_room.handle(VoiceRoomCommand::TelaBytes {
            from: PersonId(1),
            bytes: chave.clone(),
        });
        let mut de_carol = carol.try_recv().expect("carol devia ter sido ligada");
        assert_eq!(recebido(&mut de_carol), chave);
        // E quem já assistia recebeu os dois, sem repetição.
        let mut esperado = comum;
        esperado.extend(chave);
        assert_eq!(recebido(&mut de_bob), esperado);
    }

    #[test]
    fn o_n_da_sala_e_anunciado_ao_abrir_e_a_cada_entrada_e_saida() {
        // §5.1 divide o caminho do anfitrião por N, e quem compartilha calcula
        // o mesmo `min` do outro lado. Sem este anúncio ele aplicaria a conta
        // com uma perna que inventa, que é o defeito que a seção chama de mais
        // caro. Vem daqui, e não de junto do `PersonJoined`, porque este é o
        // único mapa que sabe quem está na sala sem perguntar a ninguém.
        let (mut voice_room, mut ouvinte) = sala_com_barramento(crate::tela::CAMINHO_DO_DOGMA_BPS);
        let _alice = espectador(&mut voice_room, 1);
        let _fim = compartilhar(&mut voice_room, 1, 7);
        // Zero é uma resposta, e ela tem de sair: quem compartilha para uma
        // sala vazia precisa saber que ninguém assiste tanto quanto precisa
        // saber que seis assistem.
        assert_eq!(contagens(&mut ouvinte), vec![0]);

        let _bob = espectador(&mut voice_room, 2);
        let _carol = espectador(&mut voice_room, 3);
        assert_eq!(contagens(&mut ouvinte), vec![1, 2]);

        // E a saída é a metade boa de N mudar: ela devolve teto.
        voice_room.handle(VoiceRoomCommand::Leave { person: PersonId(2) });
        assert_eq!(contagens(&mut ouvinte), vec![1]);
    }

    #[test]
    fn uma_sala_sem_transmissao_nao_anuncia_contagem_nenhuma() {
        // O número só quer dizer alguma coisa enquanto há transmissão: fora
        // disso ele seria um quadro por entrada e saída em toda sala do Dogma,
        // sobre um teto que ninguém está calculando.
        let (mut voice_room, mut ouvinte) = sala_com_barramento(crate::tela::CAMINHO_DO_DOGMA_BPS);
        let _alice = espectador(&mut voice_room, 1);
        let _bob = espectador(&mut voice_room, 2);
        assert_eq!(contagens(&mut ouvinte), Vec::<u32>::new());
    }

    #[test]
    fn a_transmissao_que_para_pelo_teto_nao_anuncia_uma_ultima_contagem() {
        // A ordem importa: anunciar antes de reconferir poria no fio o N de uma
        // transmissão que morre no mesmo instante, e quem recebesse desenharia
        // «3 pessoas assistindo» ao lado de uma tela que acabou de parar.
        let (mut voice_room, mut ouvinte) = sala_com_barramento(600_000);
        let _alice = espectador(&mut voice_room, 1);
        let _bob = espectador(&mut voice_room, 2);
        let mut fim = compartilhar(&mut voice_room, 1, 7);
        assert_eq!(contagens(&mut ouvinte), vec![1]);

        let _carol = espectador(&mut voice_room, 3);
        assert_eq!(
            fim.try_recv(),
            Ok(crate::tela::FimDaTela::AlemDoQueOHospedeiroCarrega)
        );
        assert_eq!(contagens(&mut ouvinte), Vec::<u32>::new());
    }

    #[test]
    fn a_sala_que_cresce_alem_da_subida_do_dogma_para_a_transmissao() {
        // A primeira linha do `min` do §5.1, que é a que faltava: a subida de
        // quem hospeda é `N × teto`, então cada pessoa que entra encolhe o teto
        // de todo mundo. Quando ele passa por baixo do piso do §2, o que para é
        // o vídeo — com motivo — porque a alternativa é a sala inteira
        // picotando por causa da tela.
        let mut voice_room = VoiceRoom::com_caminho(VoiceRoomId(1), 600_000, None);
        let _alice = espectador(&mut voice_room, 1);
        let mut bob = espectador(&mut voice_room, 2);
        let mut fim = compartilhar(&mut voice_room, 1, 7);
        let mut de_bob = bob.try_recv().unwrap();
        assert!(fim.try_recv().is_err(), "um espectador já não cabia");

        // 360 kbps de teto para dois espectadores são 180, abaixo dos 200 do
        // piso.
        let _carol = espectador(&mut voice_room, 3);
        assert_eq!(
            fim.try_recv(),
            Ok(crate::tela::FimDaTela::AlemDoQueOHospedeiroCarrega)
        );
        assert!(matches!(de_bob.pedacos.try_recv(), Ok(Pedaco::Fim)));
    }

    #[test]
    fn um_espectador_que_nao_acompanha_e_cortado_e_nao_descartado() {
        // Onde o áudio descarta, a tela corta. Um fluxo QUIC é uma sequência
        // ordenada de bytes: pular um pedaço no meio não atrasa um espectador,
        // desloca o enquadramento dele para sempre. Cortar é a única sanção
        // honesta — e ela é dele, nunca da sala.
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let _alice = espectador(&mut voice_room, 1);
        let mut lento = espectador(&mut voice_room, 2);
        let mut atento = espectador(&mut voice_room, 3);
        let _fim = compartilhar(&mut voice_room, 1, 7);
        let mut _do_lento = lento.try_recv().unwrap();
        let mut do_atento = atento.try_recv().unwrap();

        let mut chegou = Vec::new();
        for _ in 0..(crate::tela::PEDACOS_DEPTH + 8) {
            let bytes = quadro(false, 16);
            voice_room.handle(VoiceRoomCommand::TelaBytes {
                from: PersonId(1),
                bytes: bytes.clone(),
            });
            chegou.extend(recebido(&mut do_atento));
        }
        assert!(
            voice_room.drops().espectador_cortado > 0,
            "o espectador lento não foi cortado"
        );
        assert_eq!(
            chegou.len(),
            (crate::tela::PEDACOS_DEPTH + 8) * quadro(false, 16).len(),
            "cortar um espectador tirou bytes de quem estava acompanhando"
        );
    }

    #[test]
    fn um_fluxo_de_quem_nao_esta_transmitindo_nao_passa() {
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let _alice = espectador(&mut voice_room, 1);
        let mut bob = espectador(&mut voice_room, 2);
        let _fim = compartilhar(&mut voice_room, 1, 7);
        let mut de_bob = bob.try_recv().unwrap();

        voice_room.handle(VoiceRoomCommand::TelaBytes {
            from: PersonId(2),
            bytes: quadro(true, 12),
        });
        assert_eq!(voice_room.drops().tela_sem_dono, 1);
        assert!(de_bob.pedacos.try_recv().is_err());
    }

    #[test]
    fn quem_nao_pode_falar_tambem_nao_pode_mostrar() {
        // `specs/08-seguranca.md`: verificado no servidor, sempre. Quem não
        // pode transmitir mídia nesta sala não passa a poder transmitindo-a
        // como imagem — e nenhuma permissão nova foi inventada para isso.
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let _observador = member(&mut voice_room, 1, 100, false);
        let mut bob = espectador(&mut voice_room, 2);

        let _fim = compartilhar(&mut voice_room, 1, 7);
        assert_eq!(voice_room.drops().not_permitted, 1);
        assert!(bob.try_recv().is_err());
    }

    #[test]
    fn a_lagging_subscriber_is_dropped_rather_than_blocking_the_voice_room() {
        // A slow listener must not add latency for everybody else. Old audio is
        // worth nothing anyway.
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let _alice = member(&mut voice_room, 1, 100, true);
        let (tx, _rx) = mpsc::channel(1);
        voice_room.handle(VoiceRoomCommand::Join {
            person: PersonId(2),
            ssrc: Ssrc(200),
            may_speak: true,
            outbound: tx,
            tela: mpsc::channel(4).0,
        });

        for seq in 0..10 {
            voice_room.handle(VoiceRoomCommand::Datagram {
                from: Ssrc(100),
                bytes: datagram(100, seq),
            });
        }

        assert!(voice_room.drops().subscriber_lagging > 0);
    }
}
