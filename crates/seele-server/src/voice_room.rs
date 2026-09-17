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
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use seele_proto::ids::{PersonId, ScreenId, SessionId, Ssrc, VoiceRoomId};
use seele_proto::transport::MAX_FRAMES_PER_SECOND;
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::server::Event;
use crate::tela::{AberturaDeTela, Enquadramento, FimDaTela, Pedaco};

/// How many datagrams a voice room task will hold before it starts shedding.
///
/// Fifty a second per talker (`specs/03-audio.md`), so this is roughly a
/// second of a full voice room. A queue that grows past this is a task that has
/// stopped keeping up, and buffering more of it only adds latency to audio that
/// is already late.
const CHANNEL_DEPTH: usize = 1024;

/// De quanto em quanto tempo a perda de subida de cada pessoa é recalculada.
///
/// Uma vez por segundo e não a cada pacote: `PerdaDeSubida::fracao` varre a
/// janela, e fazê-lo cinquenta vezes por segundo por participante seria pagar a
/// varredura cinquenta vezes por um número que o cliente só usa uma. Ver o
/// ADR 0036.
const INTERVALO_DE_MEDIDA: Duration = Duration::from_secs(1);

/// What a connection asks its voice room to do.
pub enum VoiceRoomCommand {
    /// A person entered. `specs/07-estetica.md` calls it "inserir connection".
    Join {
        /// Who.
        person: PersonId,
        /// **Qual conexão** desta pessoa. Ver [`Self::Leave`].
        sessao: SessionId,
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
        /// De **qual conexão** dessa pessoa é esta saída.
        ///
        /// `Some` quando quem sai é uma sessão: a saída só vale se o membro
        /// ainda for dela. Numa queda silenciosa há duas sessões da mesma pessoa
        /// vivas ao mesmo tempo — a nova sobe perto dos 15 s e a velha só morre
        /// aos 20 s —, e sem esta conferência a velha tirava a nova da tarefa da
        /// sala. É a metade do defeito que o usuário relata como «não ouço nem
        /// vejo esse amigo».
        ///
        /// `None` quando quem decide é pela pessoa inteira, de qual conexão for:
        /// andar de uma sala para outra, por exemplo, tem de tirar o membro
        /// anterior seja qual for a sessão que o pôs ali.
        sessao: Option<SessionId>,
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
        /// Como o servidor batizou esta transmissão.
        screen: ScreenId,
        /// Os bytes do cabeçalho de abertura, para repassar a cada espectador.
        abertura: Vec<u8>,
        /// Por onde avisar quem compartilha se o servidor encerrar por conta
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
        /// **De qual conexão.** A tarefa da tela vive fora do laço da sessão e
        /// pode chegar ao fim depois de a conexão nova da mesma pessoa já ter
        /// reaberto a transmissão; sem a sessão, o fim da velha apagaria a nova.
        /// É a última porta desta família que ainda era só por pessoa.
        sessao: SessionId,
    },
    /// Alguém pediu para assistir a uma transmissão desta sala.
    ///
    /// **É o pedido que cria a cópia.** Sem ele o servidor não abre cano, e uma
    /// transmissão que ninguém abriu não ocupa subida nenhuma — é o que faz
    /// «todos podem transmitir» caber na conta.
    TelaAssistir {
        /// Quem quer ver.
        person: PersonId,
        /// O quê.
        screen: ScreenId,
    },
    /// Alguém fechou a janela de uma transmissão.
    TelaParouDeAssistir {
        /// Quem.
        person: PersonId,
        /// O quê.
        screen: ScreenId,
    },
    /// Quem está recebendo esta transmissão desta sala, agora?
    ///
    /// **A única pergunta deste canal, e ela existe porque a resposta mora
    /// aqui.** `crate::pares::Pares::escolher` precisa saber quem pode
    /// repassar uma transmissão, e um par só repassa o que ele mesmo está
    /// recebendo — quem liga e desliga os canos é a sala, e perguntar a
    /// qualquer outro registro seria uma segunda conta a discordar desta.
    ///
    /// Responde **sempre**, inclusive com o conjunto vazio para uma
    /// transmissão que não existe nesta sala: quem perguntou está esperando o
    /// canal, e um `responder` largado sem valor trocaria uma resposta por um
    /// erro de canal fechado no meio de `WatchScreen`.
    QuemRecebe {
        /// Qual transmissão.
        screen: ScreenId,
        /// Por onde a resposta volta.
        responder: tokio::sync::oneshot::Sender<std::collections::HashSet<PersonId>>,
    },
    /// A subida desta máquina foi **medida**, e o número mudou.
    ///
    /// A sala nasce com [`crate::tela::caminho_do_server`] — o declarado, ou a
    /// hipótese das provas — e é por esse número que ela divide N. Enquanto
    /// isto não existia, era o **único** número que ela conhecia: a `Subida`
    /// media o cano de verdade, mandava o resultado para cada cliente pelo
    /// `HostUplink`, e o portão que decide quem entra continuava dividindo a
    /// hipótese. O produto media a perna certa e não a usava onde ela decide.
    Subida,
    /// Quanto esta sala descartou, e por quê — perguntado de fora da tarefa.
    ///
    /// Existe para que um teste de ponta a ponta possa afirmar que o guarda de
    /// sessão **agiu**, e não apenas que nada de ruim aconteceu:
    /// [`DropCounts::saida_de_sessao_velha`] só anda quando uma conexão velha
    /// morre tentando tirar da sala a conexão nova da mesma pessoa. Sem esta
    /// pergunta, o contador existia e ninguém de fora do módulo o lia — e um
    /// contador que ninguém lê não prova nada.
    ///
    /// A resposta vai por `oneshot` porque a sala é uma tarefa: os contadores
    /// são um campo dela, e a fila é o único caminho para dentro. Perguntar pela
    /// mesma fila também **ordena** a resposta depois de todo comando já
    /// enfileirado, que é exatamente o que um teste precisa.
    Contadores(oneshot::Sender<DropCounts>),
}

/// Why a datagram was not forwarded.
///
/// Counted rather than logged per event: at fifty frames a second per talker, a
/// log channel per drop is its own denial of service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DropCounts {
    /// The sender is not in this voice room.
    pub not_a_member: u64,
    /// Uma saída chegou de uma sessão que já não era a desta pessoa.
    ///
    /// Contado porque é o defeito se defendendo, e não nada acontecendo: cada
    /// unidade aqui é uma conexão velha que morreu tentando tirar da sala a
    /// conexão nova da mesma pessoa. Zero é o normal; um número que cresce
    /// durante uma chamada é a rede de alguém caindo e voltando — e, antes do
    /// guarda, seria uma pessoa ficando muda e invisível sem ninguém saber por
    /// quê.
    pub saida_de_sessao_velha: u64,

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
    /// Alguém quis assistir e a subida de quem hospeda não carregava a cópia.
    ///
    /// **É a recusa que substituiu um encerramento.** Antes, o espectador que
    /// não coubesse entrava na conta assim mesmo, e o reconferir do teto
    /// encerrava a transmissão mais nova — que numa sala com uma é a única. A
    /// sétima pessoa a entrar apagava a tela de todo mundo.
    ///
    /// Contado e não silencioso porque é a diferença entre «ninguém está
    /// transmitindo» e «não coube a sua cópia», e as duas dão a mesma tela vazia
    /// para quem chegou.
    pub espectador_nao_coube: u64,
}

/// One member of a voice room.
struct Member {
    ssrc: Ssrc,
    /// Qual conexão desta pessoa é este membro. Ver [`VoiceRoomCommand::Leave`].
    sessao: SessionId,

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
    /// Quanto da voz desta pessoa não está chegando. Ver o ADR 0036.
    perda: crate::perda_de_subida::PerdaDeSubida,
    /// Quando medir de novo. Ver [`INTERVALO_DE_MEDIDA`].
    proxima_medida: Instant,
}

/// A transmissão de tela que está passando por esta sala agora.
///
/// # Por que mora aqui e não ao lado de [`crate::server::Telas`]
///
/// `Telas` é o **plano de controle**: quem tem a vaga da sala, para responder
/// `ScreenShareTaken` a quem chega depois. Isto é o **plano de dados**, e ele
/// precisa de uma coisa que só a sala de voz tem sem perguntar a ninguém: **quem está
/// na sala neste instante**. O §5.1 transformou esse número, N, num termo do
/// teto — `caminho de quem hospeda × 60% ÷ N` — e quem encaminha é quem o sabe
/// primeiro, porque é o mesmo mapa de onde saem as cópias.
struct EmCurso {
    dono: PersonId,
    /// A conexão de onde esta transmissão saiu.
    ///
    /// Copiada do `Member` no instante da abertura, e não recebida do comando,
    /// pela mesma razão que o `ssrc`: a sala já sabe de quem é a sessão vigente,
    /// e o que ela sabe não precisa ser jurado por quem fala com ela.
    sessao: SessionId,
    /// Como o servidor batizou esta transmissão. Vai no convite de cada
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

impl EmCurso {
    /// Quantas cópias esta transmissão custa ao cano de quem hospeda.
    ///
    /// **Os ligados mais os que esperam.** Quem entrou no meio ainda não tem
    /// cano — ele é ligado no próximo quadro-chave —, mas vai ter, e em
    /// milissegundos. Contar só os ligados deixaria o teto otimista exatamente
    /// no instante em que alguém chega, que é quando ele mais precisa estar
    /// certo: a conta serve para decidir se a sala nova ainda cabe.
    fn recebem(&self) -> usize {
        self.canos.len() + self.esperando.len()
    }
}

/// The state of one voice channel.
pub struct VoiceRoom {
    id: VoiceRoomId,
    members: HashMap<PersonId, Member>,
    /// Reverse index, so a datagram's sender is found without scanning.
    by_ssrc: HashMap<Ssrc, PersonId>,
    drops: DropCounts,
    forwarded: u64,
    /// A subida deste servidor, **partilhada com quem a mede**.
    ///
    /// Parâmetro e não constante para que o teto do §5.1 seja testável sem
    /// depender do número que ninguém mediu — ver
    /// [`crate::tela::CAMINHO_DO_SERVER_BPS`].
    ///
    /// `Arc<AtomicU32>` e não `u32` porque este número **anda**, e porque a sala
    /// tem de ler o valor de agora em vez de receber cópias dele. Uma cópia
    /// entregue por mensagem pode ser descartada — [`VoiceRooms::subida_medida`]
    /// usa `try_send` para não segurar todas as salas atrás de uma atolada — e
    /// a sala ficaria dividindo um número velho para sempre, sem ninguém saber.
    /// É a falha que este repositório paga mais caro: o produto sabe e não
    /// conta. [`VoiceRoomCommand::Subida`] continua existindo, para **disparar**
    /// o reconferir; o número nunca viaja nele.
    caminho_bps: Arc<AtomicU32>,
    /// As transmissões em curso, pela pessoa que as manda.
    ///
    /// **Pela pessoa e não pela `ScreenId`** porque toda pergunta que este
    /// arquivo faz é «o que fulano está mandando» — os bytes chegam com o
    /// remetente, a saída da sala é de uma pessoa, e o fim também. Indexar pela
    /// tela obrigaria a procurar o dono em toda uma delas.
    ///
    /// Uma pessoa manda uma tela por vez: mandar duas seria dobrar a subida dela
    /// sem que ninguém tivesse pedido a segunda.
    telas: HashMap<PersonId, EmCurso>,
    /// Por onde o **N** desta sala chega ao plano de controle.
    ///
    /// A sala de voz é o único lugar do servidor que sabe quem está na sala sem
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
        Self::com_caminho(id, crate::tela::CAMINHO_DO_SERVER_BPS, None)
    }

    /// A mesma sala, sobre uma subida de servidor conhecida e com quem avisar.
    #[must_use]
    pub fn com_caminho(
        id: VoiceRoomId,
        caminho_bps: u32,
        eventos: Option<broadcast::Sender<Event>>,
    ) -> Self {
        Self::com_caminho_partilhado(id, Arc::new(AtomicU32::new(caminho_bps)), eventos)
    }

    /// A mesma sala, sobre a subida que [`VoiceRooms`] mede e todas partilham.
    #[must_use]
    pub fn com_caminho_partilhado(
        id: VoiceRoomId,
        caminho_bps: Arc<AtomicU32>,
        eventos: Option<broadcast::Sender<Event>>,
    ) -> Self {
        Self {
            id,
            members: HashMap::new(),
            by_ssrc: HashMap::new(),
            drops: DropCounts::default(),
            forwarded: 0,
            caminho_bps,
            telas: HashMap::new(),
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
                sessao,
                ssrc,
                may_speak,
                outbound,
                tela,
            } => {
                // O `ssrc` de quem está sendo substituído sai junto. Entrar de
                // novo troca o `Member` desta pessoa, mas `by_ssrc` é outro
                // mapa: sem esta linha o número antigo continuaria apontando
                // para ela, e agora que a saída da sessão velha volta cedo —
                // porque o guarda de sessão a impede de apagar o que é da
                // conexão nova — ninguém mais apagaria esse resto. Seria uma
                // entrada a mais por reconexão, pela vida inteira da sala.
                if let Some(anterior) = self.members.get(&person) {
                    if anterior.ssrc != ssrc {
                        self.by_ssrc.remove(&anterior.ssrc);
                    }
                }
                self.by_ssrc.insert(ssrc, person);
                self.members.insert(
                    person,
                    Member {
                        ssrc,
                        sessao,

                        may_speak,
                        outbound,
                        orcamento: crate::taxa::Balde::novo(
                            MAX_FRAMES_PER_SECOND,
                            f64::from(MAX_FRAMES_PER_SECOND),
                            now,
                        ),
                        tela,
                        perda: crate::perda_de_subida::PerdaDeSubida::nova(),
                        proxima_medida: now + INTERVALO_DE_MEDIDA,
                    },
                );
                // Quem chega no meio de uma transmissão entra na fila, e não no
                // fluxo. Ligado num byte qualquer, o enquadramento dele ficaria
                // deslocado para sempre; ligado no começo do próximo
                // quadro-chave, ele acerta o passo e ainda consegue
                // decodificar. Quem entra pede um quadro-chave, e a onda 1 já
                // atende esse pedido.
                // **Só entra sozinho na única.** Com uma transmissão na sala,
                // quem chega vê o que está acontecendo sem clicar — é o que o
                // produto sempre fez, e ninguém quer clicar para ver a única
                // coisa que há. Com duas, entrar nas duas custaria duas cópias e
                // dois decodificadores a quem acabou de chegar, sem ter pedido
                // nenhum dos dois; a escolha passa a ser dela, por `WatchScreen`.
                //
                // **E só se a cópia dele couber.** Cada pessoa que entra é uma
                // cópia a mais na subida de quem hospeda. Sem esta pergunta, o
                // `reconferir_o_teto` logo abaixo via as cópias não caberem e
                // **encerrava a transmissão** — numa sala com uma só, a mais
                // nova é a única, então a sétima pessoa a entrar apagava a tela
                // de todo mundo. Quem chegou por último derrubava o que estava
                // no ar, e ninguém tinha como saber por quê.
                //
                // Quando não cabe, quem fica de fora é **quem chegou**, e não a
                // transmissão. É a mesma direção da regra que já existia para as
                // transmissões — a última a entrar é a primeira a sair —,
                // aplicada a espectador em vez de a tela.
                if self.telas.len() == 1 && self.cabe_mais_uma_copia() {
                    for curso in self.telas.values_mut() {
                        if curso.dono != person {
                            curso.esperando.push(person);
                        }
                    }
                } else if self.telas.len() == 1 {
                    self.drops.espectador_nao_coube += 1;
                }
                self.reconferir_o_teto();
            }
            VoiceRoomCommand::Leave { person, sessao } => {
                // **Só sai quem ainda é membro desta sessão.** Sem esta
                // pergunta, a conexão velha de alguém, ao morrer, tirava da sala
                // a conexão nova da mesma pessoa: ela continuava desenhada na
                // tela de quem a pôs lá e a voz dela era descartada aqui como
                // `not_a_member`, sem aviso para ninguém.
                if sessao.is_some_and(|quem| {
                    self.members
                        .get(&person)
                        .is_some_and(|member| member.sessao != quem)
                }) {
                    self.drops.saida_de_sessao_velha += 1;
                    // **Menos o que ainda é da conexão que morreu.** A sala não
                    // mexe no que é da nova, mas a transmissão da velha não tem
                    // mais de onde receber bytes e, deixada no mapa, tranca a
                    // vaga de tela desta sala contra a própria conexão nova
                    // desta pessoa — que abriria e receberia `tela_ja_tomada`
                    // por uma tela que já morreu.
                    if self
                        .telas
                        .get(&person)
                        .is_some_and(|curso| Some(curso.sessao) == sessao)
                    {
                        self.encerrar_tela(person, None);
                    }
                    // E dito em voz alta, não só contado. O contador é para o
                    // teste; quem opera precisa saber que a rede de alguém caiu
                    // e voltou, e este é o único lugar que sabe. Pode sair por
                    // evento sem virar enxurrada: acontece no máximo uma vez por
                    // conexão desmontada, e não por quadro — é a razão pela qual
                    // o resto de `DropCounts` não pode fazer o mesmo.
                    tracing::info!(
                        %person,
                        sessao = ?sessao,
                        "uma conexão velha tentou sair da sala pela nova desta pessoa; a sala não mexeu"
                    );
                    return;
                }
                if let Some(member) = self.members.remove(&person) {
                    self.by_ssrc.remove(&member.ssrc);
                }
                // A saída de quem compartilha mata a transmissão, e é o caminho
                // por onde **toda** saída passa: `salas de voz::leave_everywhere` é
                // chamado ao sair da sala, ao ser movido, ao a sala ser
                // destruída e em qualquer `?` do meio da sessão. Um
                // encaminhamento que sobrevivesse a isso seria um fluxo
                // bombeando para uma sala que já não tem de onde receber.
                if self.telas.contains_key(&person) {
                    self.encerrar_tela(person, None);
                }
                // E sai de espectadora das outras, que continuam de pé: a saída
                // de quem assiste não derruba transmissão nenhuma.
                for curso in self.telas.values_mut() {
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
            VoiceRoomCommand::TelaAssistir { person, screen } => self.assistir(person, screen),
            VoiceRoomCommand::TelaParouDeAssistir { person, screen } => {
                self.parar_de_assistir(person, screen);
            }
            VoiceRoomCommand::TelaFechou { from, sessao } => {
                // **Só encerra a transmissão a conexão que a abriu.** O fim
                // limpo do fluxo velho, chegando depois de a conexão nova ter
                // reaberto a tela, deixaria quem assiste com o palco apagado
                // enquanto quem transmite continua mandando bytes.
                if self
                    .telas
                    .get(&from)
                    .is_some_and(|curso| curso.sessao == sessao)
                {
                    self.encerrar_tela(from, None);
                }
            }
            VoiceRoomCommand::QuemRecebe { screen, responder } => {
                // **Os ligados mais os que esperam**, pela mesma razão que
                // `EmCurso::recebem` conta os dois: quem entrou no meio ainda
                // não tem cano, e vai ter em milissegundos. Deixá-lo de fora
                // faria a escolha do par recusar, por um instante, alguém que
                // está recebendo.
                let quem = self
                    .telas
                    .values()
                    .find(|curso| curso.screen == screen)
                    .map(|curso| {
                        curso
                            .canos
                            .keys()
                            .chain(curso.esperando.iter())
                            .copied()
                            .collect()
                    })
                    .unwrap_or_default();
                // O resultado é descartado: quem perguntou pode ter desistido
                // entre a pergunta e a resposta, e não há o que fazer a
                // respeito.
                let _ = responder.send(quem);
            }
            VoiceRoomCommand::Contadores(responder) => {
                // Quem perguntou pode ter desistido; a sala não tem o que fazer
                // a respeito, e derrubá-la por isso seria pior que o silêncio.
                let _ = responder.send(self.drops);
            }
            VoiceRoomCommand::Subida => {
                // **O número já chegou**, pelo `Arc` que esta sala partilha com
                // quem o mede. Este comando é só o empurrão para reconferir.
                //
                // **Nos dois sentidos, e o de apertar é o que não pode faltar.**
                // A medida costuma ser maior que a hipótese, e aí isto não faz
                // nada além de deixar o próximo espectador entrar. Quando ela é
                // menor, é aqui que a proteção do §3.2 continua de pé: o que já
                // está no ar deixou de caber, e o reconferir é quem sabe qual
                // transmissão encerrar e com que motivo.
                self.reconferir_o_teto();
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

    /// Quantas cópias este servidor está subindo agora, somadas.
    ///
    /// **A conta que o §5.1 divide, e ela é de cópias e não de espectadores.**
    /// Com uma transmissão as duas coincidem; com duas, não: quem hospeda manda
    /// uma cópia por espectador **de cada** transmissão, e é a soma que sai pelo
    /// cano dele.
    ///
    /// Contadas dos canos ligados, e não de `transmissões × membros`: quem não
    /// está assistindo não tem cano, e uma cópia que ninguém recebe não ocupa
    /// subida nenhuma.
    #[must_use]
    fn copias(&self) -> usize {
        self.telas.values().map(EmCurso::recebem).sum()
    }

    /// Refaz a conta do §5.1 depois de N mudar, e para as que não cabem.
    ///
    /// **Onde o número que só o encaminhador sabe é aplicado.** A subida deste
    /// servidor é `N × teto`, então cada pessoa que entra na sala encolhe o teto
    /// de todo mundo; quando ele passa por baixo do piso do §2, a transmissão
    /// para com motivo, que é a escalada que o §3.2 escreve. A alternativa —
    /// continuar subindo o que a máquina não tem — é a sala inteira picotando
    /// por causa da tela, e essa é a única coisa que a spec chama de produto
    /// quebrado.
    fn reconferir_o_teto(&mut self) {
        if self.telas.is_empty() {
            return;
        }
        if crate::tela::teto_do_hospedeiro(self.caminho_bps.load(Ordering::Relaxed), self.copias())
            .is_none()
        {
            // **A última a entrar é a primeira a sair.**
            //
            // Quando o cano não carrega mais todas, alguma tem de parar, e
            // derrubar a mais antiga puniria quem estava lá primeiro por alguém
            // ter chegado depois. Encerrar só uma, e reconferir: pode ser que
            // com uma a menos as outras caibam, e derrubar todas de uma vez
            // seria apagar a sala inteira por causa de uma pessoa.
            let ultima = self
                .telas
                .values()
                .max_by_key(|curso| curso.screen.0)
                .map(|curso| curso.dono);
            if let Some(dono) = ultima {
                self.encerrar_tela(dono, Some(FimDaTela::AlemDoQueOHospedeiroCarrega));
            }
            self.reconferir_o_teto();
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
        let Some(eventos) = self.eventos.as_ref() else {
            return;
        };
        // Um aviso por transmissão: o N de cada uma é o mesmo hoje, e será
        // diferente no dia em que cada pessoa escolher o que assiste.
        for curso in self.telas.values() {
            let _ = eventos.send(Event::ScreenViewers {
                voice_room: self.id,
                screen: curso.screen,
                // Um servidor é dimensionado em cinquenta pessoas
                // (`specs/04-servidor-seele.md`), então isto nunca satura; saturar
                // ainda assim é melhor que dar a volta, porque um N pequeno demais
                // devolveria um teto grande demais.
                quantos: u32::try_from(curso.recebem()).unwrap_or(u32::MAX),
            });
        }
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
        // Guardada agora, do `Member` vigente, para viajar com a transmissão
        // até o fim dela.
        let sessao = member.sessao;
        // Uma por sala (§6 item 3). A corrida já foi decidida no controle; isto
        // é a parede que não depende de o cliente ter respeitado a resposta.
        if self.telas.contains_key(&from) {
            // A mesma pessoa abrindo de novo. A corrida já foi decidida no
            // controle; isto é a parede que não depende de o cliente ter
            // respeitado a resposta.
            self.drops.tela_ja_tomada += 1;
            return;
        }
        // **O teto é conferido com as cópias que esta transmissão vai somar**, e
        // não com as que já existem: aceitar primeiro e descobrir depois deixaria
        // a sala com uma transmissão a mais por um instante — e o instante é o
        // suficiente para todas picotarem.
        let depois = self.copias() + self.members.len().saturating_sub(1);
        if crate::tela::teto_do_hospedeiro(self.caminho_bps.load(Ordering::Relaxed), depois)
            .is_none()
        {
            let _ = fim.try_send(FimDaTela::AlemDoQueOHospedeiroCarrega);
            return;
        }
        self.telas.insert(
            from,
            EmCurso {
                dono: from,
                sessao,
                screen,
                abertura,
                enquadramento: Enquadramento::novo(),
                canos: HashMap::new(),
                esperando: Vec::new(),
                fim,
            },
        );
        // **A primeira abre para a sala; da segunda em diante, quem quiser pede.**
        //
        // Com uma transmissão só, empurrá-la para todo mundo é o certo: ninguém
        // quer clicar para ver a única coisa que está acontecendo, e é o que o
        // produto fez desde sempre. A partir da segunda, empurrar cobraria de
        // cada pessoa uma cópia na descida e um decodificador na CPU por
        // transmissão — e nenhuma das duas aparece na conta do teto, que só mede
        // a subida de quem hospeda.
        //
        // Quem já está na sala entra do primeiro byte: o fluxo ainda não tem
        // byte nenhum, então não há passo a acertar.
        if self.telas.len() == 1 {
            let ja_estao: Vec<PersonId> = self
                .members
                .keys()
                .copied()
                .filter(|quem| *quem != from)
                .collect();
            self.ligar(from, &ja_estao);
        }
        // A primeira contagem, e ela tem de sair mesmo quando a sala está
        // vazia: quem compartilha para uma sala de uma pessoa precisa saber
        // que N é zero tanto quanto precisa saber que virou seis.
        self.anunciar_espectadores();
    }

    /// Põe alguém para assistir a uma transmissão desta sala.
    ///
    /// **Na fila do próximo quadro-chave, e não no fluxo.** Ligado num byte
    /// qualquer, o enquadramento de quem chega fica deslocado para sempre;
    /// ligado no começo do próximo quadro-chave, ele acerta o passo e decodifica.
    /// É a mesma regra de quem entra na sala no meio de uma transmissão.
    ///
    /// O teto é reconferido **depois**, porque a cópia nova entra na conta: uma
    /// pessoa a mais assistindo pode ser a que não cabe.
    fn assistir(&mut self, person: PersonId, screen: ScreenId) {
        let Some(curso) = self.telas.values_mut().find(|curso| curso.screen == screen) else {
            self.drops.tela_sem_dono += 1;
            return;
        };
        // Quem manda não se assiste, e quem já assiste não entra duas vezes: a
        // segunda entrada abriria um cano a mais para a mesma pessoa, e a conta
        // de cópias passaria a mentir para mais.
        if curso.dono == person
            || curso.canos.contains_key(&person)
            || curso.esperando.contains(&person)
        {
            return;
        }
        // **Antes de entrar, e não depois.** Isto conferia o teto **depois** de
        // pôr a pessoa na fila, e o `reconferir_o_teto` encerrava a transmissão
        // mais nova quando ela não coubesse — ou seja, pedir para assistir podia
        // derrubar o que se queria assistir. Quem não cabe fica de fora; a
        // transmissão fica.
        if !self.cabe_mais_uma_copia() {
            self.drops.espectador_nao_coube += 1;
            return;
        }
        let Some(curso) = self.telas.values_mut().find(|curso| curso.screen == screen) else {
            return;
        };
        curso.esperando.push(person);
    }

    /// Se a subida de quem hospeda carrega **mais uma** cópia além das de agora.
    ///
    /// A pergunta que faltava nos dois caminhos por onde um espectador entra.
    /// Ela é feita **antes** de a pessoa entrar na conta, porque a alternativa —
    /// entrar e reconferir — tem uma sanção que não é dela: `reconferir_o_teto`
    /// encerra transmissão, e um espectador a mais não pode ser motivo para
    /// apagar a tela de quem já estava vendo.
    fn cabe_mais_uma_copia(&self) -> bool {
        crate::tela::teto_do_hospedeiro(self.caminho_bps.load(Ordering::Relaxed), self.copias() + 1)
            .is_some()
    }

    /// Tira alguém de uma transmissão, sem tocar nas outras.
    fn parar_de_assistir(&mut self, person: PersonId, screen: ScreenId) {
        let Some(curso) = self.telas.values_mut().find(|curso| curso.screen == screen) else {
            return;
        };
        curso.canos.remove(&person);
        curso.esperando.retain(|quem| *quem != person);
        // Devolve teto a quem ficou, que é a metade boa de alguém fechar a
        // janela — a mesma razão que a saída da sala tem.
        self.reconferir_o_teto();
    }

    /// Liga estes espectadores na transmissão em curso.
    ///
    /// Um cano por pessoa e por transmissão. É o fechamento dele que diz a
    /// `crate::tela::bombear` se o fluxo terminou ou foi cortado, sem uma
    /// segunda bandeira que pudesse discordar do canal.
    fn ligar(&mut self, dono: PersonId, quem: &[PersonId]) {
        let mut cortados = 0_u64;
        let Some(curso) = self.telas.get_mut(&dono) else {
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
        if cortados > 0 {
            // **Cortar é a sanção, e ela era muda.** Um espectador que não
            // acompanha perde a cópia dele — e do lado dele isso é a imagem
            // congelando sem explicação, que é metade do relato da tela preta.
            self.drops.espectador_cortado += cortados;
            tracing::warn!(
                sala = %self.id.get(),
                dono = %dono.get(),
                cortados = %cortados,
                total = %self.drops.espectador_cortado,
                "espectador cortado por não acompanhar o fluxo da tela"
            );
        }
    }

    /// Encaminha um pedaço do fluxo de quem compartilha para cada espectador.
    ///
    /// Byte por byte, sem tocar no conteúdo — `specs/04-servidor-seele.md` diz
    /// que o servidor nunca decodifica o Opus, e a imagem herda a regra e o
    /// motivo: é ela que mantém a CPU do servidor plana e que deixa o E2EE de
    /// mídia ser um acréscimo. Os cinco bytes que o [`Enquadramento`] lê dizem
    /// onde um quadro acaba, e nada sobre o que há dentro dele.
    fn tela_bytes(&mut self, from: PersonId, bytes: &[u8]) {
        let Some(curso) = self.telas.get_mut(&from) else {
            // **Bytes de tela de quem não está transmitindo.**
            //
            // Não é ruído: para chegar aqui alguém abriu um fluxo QUIC de tela
            // e escreveu nele. As duas causas são um cliente que não respeitou a
            // recusa do plano de controle, e — a que morde em produção — dois
            // lados que discordam sobre o que é uma transmissão aberta.
            self.drops.tela_sem_dono += 1;
            tracing::warn!(
                sala = %self.id.get(),
                de = %from.get(),
                bytes = %bytes.len(),
                total = %self.drops.tela_sem_dono,
                "chegaram bytes de tela de quem não está registrado transmitindo"
            );
            return;
        };
        let porta = match curso.enquadramento.entrada(bytes) {
            Ok(porta) => porta.filter(|_| !curso.esperando.is_empty()),
            Err(motivo) => {
                self.encerrar_tela(from, Some(motivo));
                return;
            }
        };
        // Quem esperava um quadro-chave entra exatamente no cabeçalho dele: o
        // que veio antes vai só para quem já assistia, e do quadro-chave em
        // diante todo mundo recebe o mesmo.
        let Some(corte) = porta else {
            self.escrever(from, bytes);
            return;
        };
        let entrando = std::mem::take(&mut curso.esperando);
        let (antes, depois) = bytes.split_at(corte.min(bytes.len()));
        self.escrever(from, antes);
        self.ligar(from, &entrando);
        self.escrever(from, depois);
    }

    /// Escreve o mesmo pedaço em cada cano ligado.
    fn escrever(&mut self, dono: PersonId, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let Some(curso) = self.telas.get_mut(&dono) else {
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
        if !cortados.is_empty() {
            // O outro ponto onde uma cópia é cortada — este é o do meio do
            // fluxo, o de cima é o do quadro-chave. Os dois eram mudos, e a
            // consequência é a mesma do lado de quem assiste: a imagem para e
            // nada diz por quê.
            self.drops.espectador_cortado += cortados.len() as u64;
            tracing::warn!(
                sala = %self.id.get(),
                dono = %dono.get(),
                cortados = %cortados.len(),
                total = %self.drops.espectador_cortado,
                "espectador cortado no meio do fluxo da tela"
            );
        }
    }

    /// Acaba com a transmissão desta sala, com ou sem motivo.
    ///
    /// `None` é o fim honesto — quem mandava parou ou saiu —, e cada espectador
    /// recebe [`Pedaco::Fim`] para que o fluxo dele **termine** em vez de ser
    /// cortado. `Some` é o servidor encerrando por conta própria, e o motivo sobe
    /// para a sessão de quem compartilha, que é quem tem como anunciá-lo.
    fn encerrar_tela(&mut self, dono: PersonId, motivo: Option<FimDaTela>) {
        let Some(curso) = self.telas.remove(&dono) else {
            return;
        };
        // **O fim de uma transmissão passa a deixar rastro.**
        //
        // Este módulo tinha **uma** linha de `tracing` para o encaminhamento
        // inteiro, e os contadores de descarte eram incrementados e nunca lidos
        // fora dos testes — o que é pior que não contar nada, porque dá a
        // impressão de que alguém está olhando.
        //
        // Foi o que deixou este relato sem resposta: «quem assiste com uma
        // versão mais velha vê tela preta, sem mensagem nenhuma, e a sessão
        // morre em ~3 segundos sem dizer por quê». Do lado do servidor não havia
        // uma linha para procurar.
        tracing::info!(
            sala = %self.id.get(),
            dono = %dono.get(),
            tela = %curso.screen.get(),
            espectadores = %curso.canos.len(),
            motivo = ?motivo,
            "transmissão de tela encerrada"
        );
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
    /// be checked against each other. Without this channel a person could put
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

        // A perda de subida desta pessoa, contada aqui porque é aqui que o
        // cabeçalho já foi decodificado — a conferência do `ssrc` acima precisa
        // dele, então `seq` vem de graça e nenhum byte de payload é tocado.
        //
        // **Antes do limitador de taxa, de propósito.** Um quadro que o
        // limitador descarta *chegou*; contá-lo como perda misturaria uma
        // decisão nossa com o que a rede fez, e o número passaria a acusar o
        // enlace de alguém por uma política do servidor.
        member.perda.chegou(header.seq, now);
        let medida = if now >= member.proxima_medida {
            member.proxima_medida = now + INTERVALO_DE_MEDIDA;
            member.perda.fracao(now)
        } else {
            None
        };
        if let Some(fracao) = medida {
            if let Some(eventos) = self.eventos.as_ref() {
                let _ = eventos.send(Event::UplinkLoss {
                    person,
                    fraction: fracao,
                });
            }
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
    caminho_bps: Arc<AtomicU32>,
    eventos: broadcast::Sender<Event>,
) -> mpsc::Sender<VoiceRoomCommand> {
    let (tx, mut rx) = mpsc::channel(CHANNEL_DEPTH);
    tokio::spawn(async move {
        let mut voice_room = VoiceRoom::com_caminho_partilhado(id, caminho_bps, Some(eventos));
        while let Some(command) = rx.recv().await {
            voice_room.handle(command);
        }
        tracing::info!(voice_room = %id, forwarded = voice_room.forwarded(), "voice room closed");
    });
    tx
}

/// Every voice room task this server is running.
///
/// # Why this had to exist the moment a server could grow a second room
///
/// The server used to spawn exactly one voice room task, at boot, for the one voice room in
/// `ServerConfig` — and every session held that single sender. That was correct
/// while a server had one room and *silently wrong* the instant it could have
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
/// and lives until the server stops.
pub struct VoiceRooms {
    tasks: tokio::sync::Mutex<HashMap<VoiceRoomId, mpsc::Sender<VoiceRoomCommand>>>,
    /// A subida deste servidor, repassada a cada sala que nasce.
    ///
    /// Uma cópia só, e ela é a mesma que viaja no `HostUplink`: a sala divide
    /// este número por N e o cliente o divide de novo, e as duas contas têm de
    /// partir do mesmo lugar. Ver [`crate::tela::caminho_do_server`].
    ///
    /// **Atômico porque ele anda.** Era o número do boot, entregue para sempre:
    /// [`crate::tela::caminho_do_server`] sem `caminho_bps` declarado — e nada
    /// neste repositório o declara — é a hipótese das provas. Uma sala nasce
    /// quando a primeira pessoa entra nela, o que costuma ser **depois** de a
    /// `Subida` já ter medido; sem isto, ela nasceria na hipótese de novo.
    caminho_bps: Arc<AtomicU32>,
    eventos: broadcast::Sender<Event>,
}

impl VoiceRooms {
    /// A server with na sala de voz task running yet.
    ///
    /// Sem `Default`, e de propósito: uma sala precisa saber quanto a subida
    /// deste servidor carrega antes de deixar alguém transmitir nela, e um
    /// `Default` teria de inventar esse número.
    #[must_use]
    pub fn new(caminho_bps: u32, eventos: broadcast::Sender<Event>) -> Self {
        Self {
            tasks: tokio::sync::Mutex::new(HashMap::new()),
            caminho_bps: Arc::new(AtomicU32::new(caminho_bps)),
            eventos,
        }
    }

    /// A subida desta máquina foi **medida**, e o número mudou.
    ///
    /// Duas coisas, e as duas são necessárias: guarda o número para as salas que
    /// ainda vão nascer, e conta às que já estão vivas. Só a primeira deixaria
    /// a conversa de agora dividindo a hipótese; só a segunda deixaria a próxima
    /// sala nascer nela.
    pub async fn subida_medida(&self, bps: u32) {
        self.caminho_bps.store(bps, Ordering::Relaxed);
        // Sem `await` dentro do laço com a trava na mão: `try_send` e não
        // `send`. Uma sala com a fila cheia está atolada de comando, e a
        // próxima medida vem daqui a um segundo — esperar por ela seguraria
        // todas as outras salas atrás de uma.
        for sala in self.tasks.lock().await.values() {
            let _ = sala.try_send(VoiceRoomCommand::Subida);
        }
    }

    /// Os contadores de uma sala que já existe, perguntados de fora.
    ///
    /// `None` quando ninguém nunca entrou nessa sala — e de propósito não a faz
    /// nascer: uma pergunta de leitura que cria uma tarefa mudaria o que ela foi
    /// chamada para medir. `None` também quando a tarefa já morreu.
    ///
    /// Ver [`VoiceRoomCommand::Contadores`] para por que a resposta vem por
    /// `oneshot` e o que ela prova.
    pub async fn contadores(&self, id: VoiceRoomId) -> Option<DropCounts> {
        let sala = self.tasks.lock().await.get(&id).cloned()?;
        let (responder, resposta) = oneshot::channel();
        sala.send(VoiceRoomCommand::Contadores(responder))
            .await
            .ok()?;
        resposta.await.ok()
    }

    /// The way in to one voice room, starting its task if this is the first arrival.
    pub async fn of(&self, id: VoiceRoomId) -> mpsc::Sender<VoiceRoomCommand> {
        self.tasks
            .lock()
            .await
            .entry(id)
            .or_insert_with(|| spawn(id, Arc::clone(&self.caminho_bps), self.eventos.clone()))
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
    /// a server at five active voice_rooms, so the fan-out is five sends.
    /// `sessao` é `Some` quando quem sai é uma conexão, e aí a saída só vale para
    /// os membros que **aquela** conexão pôs na sala — ver
    /// [`VoiceRoomCommand::Leave`].
    pub async fn leave_everywhere(&self, person: PersonId, sessao: Option<SessionId>) {
        let tasks: Vec<mpsc::Sender<VoiceRoomCommand>> =
            self.tasks.lock().await.values().cloned().collect();
        for task in tasks {
            let _ = task.send(VoiceRoomCommand::Leave { person, sessao }).await;
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
        VoiceRooms::new(crate::tela::CAMINHO_DO_SERVER_BPS, eventos)
    }

    /// Um conjunto de salas sobre um cano escolhido, e a ponta que o escuta.
    fn salas_com(caminho_bps: u32) -> (VoiceRooms, broadcast::Receiver<Event>) {
        let (eventos, ouvinte) = broadcast::channel(64);
        (VoiceRooms::new(caminho_bps, eventos), ouvinte)
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

    fn member(
        voice_room: &mut VoiceRoom,
        person: u64,
        ssrc: u32,
        may_speak: bool,
    ) -> mpsc::Receiver<Vec<u8>> {
        let (tx, rx) = mpsc::channel(64);
        let (tela, _) = mpsc::channel(4);
        voice_room.handle(VoiceRoomCommand::Join {
            person: PersonId(person),
            sessao: SessionId(person),
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
            sessao: SessionId(person),
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

    /// Abre uma transmissão de `person` e devolve por onde o servidor reclamaria.
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
        // attacker channels up with.
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
    fn a_saida_da_sessao_velha_nao_tira_da_sala_a_conexao_nova() {
        // **A metade do defeito que o usuário relata como «não ouço nem vejo esse
        // amigo».** Numa queda silenciosa há duas conexões da mesma pessoa vivas
        // ao mesmo tempo: a nova sobe perto dos 15 s e a velha só é desmontada
        // aos 20 s. Sem conferir de qual sessão é a saída, a velha tirava a nova
        // da tarefa da sala — e a voz dela passava a morrer aqui como
        // `not_a_member`, sem aviso para ninguém.
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let mut ouvinte = member(&mut voice_room, 1, 100, true);
        let (tx, _rx) = mpsc::channel(64);
        let (tela, _) = mpsc::channel(4);
        // A reconexão: mesma pessoa, outra sessão, outro `ssrc`.
        voice_room.handle(VoiceRoomCommand::Join {
            person: PersonId(2),
            sessao: SessionId(8),
            ssrc: Ssrc(201),
            may_speak: true,
            outbound: tx,
            tela,
        });

        // E agora a conexão velha morre.
        voice_room.handle(VoiceRoomCommand::Leave {
            person: PersonId(2),
            sessao: Some(SessionId(7)),
        });

        assert_eq!(
            voice_room.occupancy(),
            2,
            "a conexão velha tirou da sala a conexão nova da mesma pessoa"
        );
        assert_eq!(
            voice_room.drops().saida_de_sessao_velha,
            1,
            "o guarda trabalhou e não contou; uma defesa que não conta não aparece \
             em nenhuma investigação"
        );

        // A prova do que isso compra: a voz de quem reconectou ainda é entregue.
        voice_room.handle(VoiceRoomCommand::Datagram {
            from: Ssrc(201),
            bytes: datagram(201, 1),
        });
        assert!(
            ouvinte.try_recv().is_ok(),
            "a voz de quem reconectou deixou de ser encaminhada"
        );
        assert_eq!(voice_room.drops().not_a_member, 0);

        // E a sessão vigente continua podendo sair.
        voice_room.handle(VoiceRoomCommand::Leave {
            person: PersonId(2),
            sessao: Some(SessionId(8)),
        });
        assert_eq!(voice_room.occupancy(), 1);
    }

    #[test]
    fn o_fim_da_tela_da_conexao_velha_nao_apaga_a_transmissao_da_nova() {
        // A última porta desta família que ainda era só por pessoa. A tarefa que
        // lê o fluxo de tela vive **fora** do laço da sessão: ela pode terminar
        // limpa depois de a conexão nova da mesma pessoa já ter reaberto a
        // transmissão, e o `TelaFechou` da velha apagava a nova — palco apagado
        // para quem assiste enquanto quem transmite segue mandando bytes.
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let mut convites = espectador(&mut voice_room, 1);

        let (outbound, _rx) = mpsc::channel(64);
        let (tela, _telas) = mpsc::channel(4);
        voice_room.handle(VoiceRoomCommand::Join {
            person: PersonId(2),
            sessao: SessionId(7),
            ssrc: Ssrc(200),
            may_speak: true,
            outbound,
            tela,
        });
        let _fim_velho = compartilhar(&mut voice_room, 2, 70);
        let _convite_velho = convites
            .try_recv()
            .expect("quem já estava na sala entra na única");

        // A reconexão, e a conexão velha morrendo em seguida.
        let (outbound, _rx) = mpsc::channel(64);
        let (tela, _telas) = mpsc::channel(4);
        voice_room.handle(VoiceRoomCommand::Join {
            person: PersonId(2),
            sessao: SessionId(8),
            ssrc: Ssrc(201),
            may_speak: true,
            outbound,
            tela,
        });
        voice_room.handle(VoiceRoomCommand::Leave {
            person: PersonId(2),
            sessao: Some(SessionId(7)),
        });
        assert!(
            voice_room.telas.is_empty(),
            "a transmissão da conexão morta ficou no mapa trancando a vaga de tela \
             da sala contra a conexão nova da mesma pessoa"
        );

        // E a conexão nova abre a dela.
        let _fim_novo = compartilhar(&mut voice_room, 2, 90);
        let mut convite_novo = convites
            .try_recv()
            .expect("a transmissão nova convidou quem assiste");

        // Agora o fim limpo do fluxo velho, atrasado.
        voice_room.handle(VoiceRoomCommand::TelaFechou {
            from: PersonId(2),
            sessao: SessionId(7),
        });
        assert_eq!(
            voice_room.telas.get(&PersonId(2)).map(|curso| curso.screen),
            Some(ScreenId(90)),
            "o fim do fluxo da conexão velha apagou a transmissão da nova"
        );
        voice_room.handle(VoiceRoomCommand::TelaBytes {
            from: PersonId(2),
            bytes: quadro(true, 32),
        });
        assert!(
            !recebido(&mut convite_novo).is_empty(),
            "quem assistia parou de receber uma transmissão que continuava no ar"
        );

        // O contrapeso: a própria conexão que a abriu ainda a encerra.
        voice_room.handle(VoiceRoomCommand::TelaFechou {
            from: PersonId(2),
            sessao: SessionId(8),
        });
        assert!(
            voice_room.telas.is_empty(),
            "o fim do fluxo desta conexão deixou de encerrar a transmissão dela"
        );
    }

    #[test]
    fn o_ssrc_da_conexao_velha_para_de_valer_quando_a_pessoa_entra_de_novo() {
        // O outro lado do guarda de sessão. Entrar de novo troca o `Member`
        // desta pessoa, e antes disso a saída da conexão velha era quem
        // apagava o número antigo de `by_ssrc`; agora ela volta cedo e não
        // apaga nada. Sem tirar o número na entrada, o `ssrc` de uma conexão
        // morta seguiria entregando voz em nome desta pessoa, e a sala
        // guardaria uma entrada a mais a cada reconexão.
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let mut ouvinte = member(&mut voice_room, 1, 100, true);
        let _velha = member(&mut voice_room, 2, 200, true);

        let (tx, _rx) = mpsc::channel(64);
        let (tela, _) = mpsc::channel(4);
        voice_room.handle(VoiceRoomCommand::Join {
            person: PersonId(2),
            sessao: SessionId(8),
            ssrc: Ssrc(201),
            may_speak: true,
            outbound: tx,
            tela,
        });

        voice_room.handle(VoiceRoomCommand::Datagram {
            from: Ssrc(200),
            bytes: datagram(200, 1),
        });
        assert!(
            ouvinte.try_recv().is_err(),
            "o ssrc da conexão morta continuou entregando voz em nome de quem reconectou"
        );
        assert_eq!(voice_room.drops().not_a_member, 1);

        // E o número novo continua valendo.
        voice_room.handle(VoiceRoomCommand::Datagram {
            from: Ssrc(201),
            bytes: datagram(201, 1),
        });
        assert!(ouvinte.try_recv().is_ok(), "a voz de quem reconectou sumiu");
    }

    #[test]
    fn leaving_stops_delivery_and_frees_the_ssrc() {
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let _alice = member(&mut voice_room, 1, 100, true);
        let mut bob = member(&mut voice_room, 2, 200, true);
        assert_eq!(voice_room.occupancy(), 2);

        voice_room.handle(VoiceRoomCommand::Leave {
            person: PersonId(2),
            sessao: None,
        });
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
    async fn a_sala_que_nasce_depois_da_medida_nasce_com_ela() {
        // **`VoiceRooms` guardava o número do boot e o entregava para sempre.**
        //
        // Uma sala nasce quando a primeira pessoa entra nela, o que pode ser
        // horas depois de o servidor subir. Avisar só as salas vivas deixaria a
        // próxima nascer na hipótese de novo — e numa conversa a sala costuma
        // nascer depois da medida, não antes.
        let (salas, mut ouvinte) = salas_com(600_000);
        salas.subida_medida(2_000_000).await;

        let sala = salas.of(VoiceRoomId(1)).await;
        // As pontas receptoras ficam vivas: quem não tem por onde ser convidado
        // não entra na conta de cópias, e o teste mediria a própria negligência.
        let mut pontas = Vec::new();
        for pessoa in 1..=3_u64 {
            let (outbound, ouve) = mpsc::channel(8);
            let (tela, convites) = mpsc::channel(crate::tela::ABERTURAS_DEPTH);
            pontas.push((ouve, convites));
            sala.send(VoiceRoomCommand::Join {
                person: PersonId(pessoa),
                sessao: SessionId(pessoa),
                ssrc: Ssrc(pessoa as u32 * 10),
                may_speak: true,
                outbound,
                tela,
            })
            .await
            .unwrap();
        }
        let (fim, mut fim_rx) = mpsc::channel(1);
        sala.send(VoiceRoomCommand::TelaAbriu {
            from: PersonId(1),
            screen: ScreenId(7),
            abertura: abertura(7),
            fim,
        })
        .await
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Em 600 kbps, duas cópias dariam 180 kbps cada — abaixo do piso — e a
        // transmissão nem começaria. Em 2 Mbps ela começa e anuncia as duas.
        assert!(
            fim_rx.try_recv().is_err(),
            "a sala nasceu com a hipótese e recusou a transmissão"
        );
        assert_eq!(
            contagens(&mut ouvinte),
            vec![2],
            "a sala nasceu com a hipótese em vez da medida"
        );
    }

    #[tokio::test]
    async fn two_rooms_do_not_hear_each_other() {
        // The whole reason [`voice_rooms`] exists. With one task for the whole server
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
                sessao: SessionId(1),
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
                sessao: SessionId(2),
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
            sessao: SessionId(1),
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
            sessao: SessionId(2),
            ssrc: Ssrc(200),
            may_speak: true,
            outbound: bob_tx,
            tela: mpsc::channel(4).0,
        })
        .await
        .unwrap();

        voice_rooms.leave_everywhere(PersonId(1), None).await;

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
    fn duas_pessoas_transmitem_e_cada_espectador_recebe_as_duas() {
        // A regra era uma por sala. Ela caiu porque o argumento que a sustentava
        // — «duas dobram a subida» — só vale para a perna de quem **hospeda**: a
        // de quem compartilha não se divide, cada pessoa sobe o próprio fluxo
        // pelo próprio cano.
        //
        // O que limita passou a ser a medida: o teto por cópia tem de continuar
        // acima do piso, e é `reconferir_o_teto` quem cobra isso.
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let _alice = espectador(&mut voice_room, 1);
        let _bob = espectador(&mut voice_room, 2);
        let mut carol = espectador(&mut voice_room, 3);

        let _fim = compartilhar(&mut voice_room, 1, 7);
        let _tambem = compartilhar(&mut voice_room, 2, 8);

        assert_eq!(voice_room.drops().tela_ja_tomada, 0, "nenhuma foi recusada");

        // **A primeira chega sozinha; a segunda, só se ela pedir.**
        //
        // Com uma transmissão na sala, empurrá-la é o certo — ninguém quer
        // clicar para ver a única coisa que há. Da segunda em diante, empurrar
        // cobraria de Carol uma cópia na descida e um decodificador na CPU sem
        // que ela tivesse pedido nenhum dos dois.
        assert_eq!(carol.try_recv().map(|c| c.screen), Ok(ScreenId(7)));
        assert!(
            carol.try_recv().is_err(),
            "a segunda chegou sem ela pedir: é a cópia que ninguém autorizou"
        );

        // Pedindo, ela entra — no próximo quadro-chave, como quem chega no meio.
        voice_room.handle(VoiceRoomCommand::TelaAssistir {
            person: PersonId(3),
            screen: ScreenId(8),
        });
        assert!(
            carol.try_recv().is_err(),
            "o pedido não liga no meio do fluxo, e sim no quadro-chave"
        );
        voice_room.handle(VoiceRoomCommand::TelaBytes {
            from: PersonId(2),
            bytes: quadro(true, 30),
        });
        assert_eq!(carol.try_recv().map(|c| c.screen), Ok(ScreenId(8)));
    }

    #[test]
    fn quem_recebe_uma_transmissao_pode_ser_perguntado_de_fora() {
        // **Um par repassa o que ele mesmo recebe.** Quem escolhe o par
        // (`crate::pares::Pares::escolher`) precisa saber quem está recebendo
        // aquela transmissão **agora**, e quem sabe isso é a sala: é ela que
        // liga e desliga os canos. Perguntar de outro lugar seria uma segunda
        // conta, a discordar desta no primeiro dia ruim.
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let _alice = espectador(&mut voice_room, 1);
        let _bob = espectador(&mut voice_room, 2);
        let _carol = espectador(&mut voice_room, 3);
        let _fim = compartilhar(&mut voice_room, 1, 7);

        // Bob e Carol entraram antes: a única transmissão da sala chega a eles
        // sem pedir. Quem compartilha não recebe a própria tela.
        assert_eq!(
            perguntar_quem_recebe(&mut voice_room, ScreenId(7)),
            std::collections::HashSet::from([PersonId(2), PersonId(3)])
        );

        voice_room.handle(VoiceRoomCommand::TelaParouDeAssistir {
            person: PersonId(3),
            screen: ScreenId(7),
        });
        assert_eq!(
            perguntar_quem_recebe(&mut voice_room, ScreenId(7)),
            std::collections::HashSet::from([PersonId(2)]),
            "quem fechou a janela continuou contado como quem recebe"
        );
    }

    #[test]
    fn perguntar_por_uma_transmissao_que_nao_existe_devolve_ninguem() {
        // E não fica sem resposta: quem perguntou está esperando o canal, e um
        // `responder` largado sem valor faria `apontar_um_par` esperar por um
        // erro em vez de por uma resposta.
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let _alice = espectador(&mut voice_room, 1);
        assert!(perguntar_quem_recebe(&mut voice_room, ScreenId(99)).is_empty());
    }

    fn perguntar_quem_recebe(
        voice_room: &mut VoiceRoom,
        screen: ScreenId,
    ) -> std::collections::HashSet<PersonId> {
        let (responder, resposta) = tokio::sync::oneshot::channel();
        voice_room.handle(VoiceRoomCommand::QuemRecebe { screen, responder });
        resposta
            .blocking_recv()
            .expect("a sala não respondeu quem recebe a transmissão")
    }

    #[test]
    fn quem_fecha_a_janela_devolve_a_copia() {
        // `UnwatchScreen` existe pela razão que `StopScreenShare` já tinha: um
        // verbo que só existe como ausência não distingue «fechei a janela» de
        // «minha conexão caiu». O servidor precisa da diferença para devolver o
        // teto a quem ficou — e é a contagem de cópias que o teto divide.
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let _alice = espectador(&mut voice_room, 1);
        let _bob = espectador(&mut voice_room, 2);
        let _fim = compartilhar(&mut voice_room, 1, 7);

        assert_eq!(voice_room.copias(), 1, "bob assiste sozinho");

        voice_room.handle(VoiceRoomCommand::TelaParouDeAssistir {
            person: PersonId(2),
            screen: ScreenId(7),
        });
        assert_eq!(
            voice_room.copias(),
            0,
            "a cópia dele tinha de sair da conta na hora"
        );
    }

    #[test]
    fn a_mesma_pessoa_abrindo_duas_vezes_continua_sendo_uma_transmissao() {
        // A parede que sobrou, e ela não depende de o cliente ter respeitado a
        // resposta do controle: uma pessoa manda **uma** tela: mandar duas
        // dobraria a subida dela sem que ninguém tivesse pedido a segunda.
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let _alice = espectador(&mut voice_room, 1);
        let mut bob = espectador(&mut voice_room, 2);

        let _fim = compartilhar(&mut voice_room, 1, 7);
        let _de_novo = compartilhar(&mut voice_room, 1, 8);

        assert_eq!(voice_room.drops().tela_ja_tomada, 1);
        assert_eq!(bob.try_recv().map(|c| c.screen), Ok(ScreenId(7)));
        assert!(bob.try_recv().is_err(), "a segunda não chegou a existir");
    }

    #[test]
    fn sair_da_sala_encerra_a_transmissao() {
        // O caminho por onde **toda** saída passa — sair da sala, ser movido,
        // a sala ser destruída, a conexão cair em qualquer `?`. Sem isto fica
        // um fluxo aberto na tela de quem assistia, prometendo imagem que já
        // não tem de onde vir.
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let _alice = espectador(&mut voice_room, 1);
        let mut bob = espectador(&mut voice_room, 2);

        let _fim = compartilhar(&mut voice_room, 1, 7);
        let mut convite = bob.try_recv().unwrap();

        voice_room.handle(VoiceRoomCommand::Leave {
            person: PersonId(1),
            sessao: None,
        });
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
    fn a_sala_que_cresce_nao_derruba_a_transmissao_de_quem_ja_estava() {
        // **Quem chega depois tem de ver o que está no ar.** É requisito, na
        // palavra de quem o pediu: «um usuário DEVE ser capaz de ver a live,
        // mesmo entrando depois de ela iniciar.»
        //
        // O caminho por onde isso se perde não é o de ligar quem chega — esse
        // funciona, e o teste ao lado o prende. É o `reconferir_o_teto` que roda
        // logo depois: cada pessoa que entra é uma cópia a mais na subida de
        // quem hospeda, e quando as cópias não cabem ele **encerra** a
        // transmissão mais nova. Numa sala com uma transmissão só, a mais nova é
        // a única — então a sétima pessoa a entrar apagaria a tela de todo
        // mundo, e quem chegou por último veria a culpa cair sobre si.
        //
        // Com a subida suposta de 2 Mbit/s e o piso de 200 kbit/s, cabem seis
        // cópias. Este teste enche a sala até lá e mais uma.
        let mut voice_room = VoiceRoom::new(VoiceRoomId(1));
        let _dono = espectador(&mut voice_room, 1);
        let mut fim = compartilhar(&mut voice_room, 1, 7);

        let mut chegando = Vec::new();
        for quem in 2..=8 {
            chegando.push(espectador(&mut voice_room, quem));
        }

        assert!(
            fim.try_recv().is_err(),
            "a transmissão foi encerrada porque a sala cresceu. Quem entra tem \
             de ver o que está no ar — e se a subida não carrega, quem sobra de \
             fora é quem chegou, não a transmissão inteira"
        );
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
        let (mut voice_room, mut ouvinte) = sala_com_barramento(crate::tela::CAMINHO_DO_SERVER_BPS);
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
        voice_room.handle(VoiceRoomCommand::Leave {
            person: PersonId(2),
            sessao: None,
        });
        assert_eq!(contagens(&mut ouvinte), vec![1]);
    }

    #[test]
    fn uma_sala_sem_transmissao_nao_anuncia_contagem_nenhuma() {
        // O número só quer dizer alguma coisa enquanto há transmissão: fora
        // disso ele seria um quadro por entrada e saída em toda sala do servidor,
        // sobre um teto que ninguém está calculando.
        let (mut voice_room, mut ouvinte) = sala_com_barramento(crate::tela::CAMINHO_DO_SERVER_BPS);
        let _alice = espectador(&mut voice_room, 1);
        let _bob = espectador(&mut voice_room, 2);
        assert_eq!(contagens(&mut ouvinte), Vec::<u32>::new());
    }

    #[test]
    fn quem_nao_cabe_fica_de_fora_e_a_contagem_nao_muda() {
        // **Este teste foi virado ao contrário em 03/09/2026**, e o que ele
        // afirmava está logo abaixo, em `a_sala_que_cresce...`. O que se
        // preservou daqui é a ordem: uma contagem só sai quando há o que contar.
        //
        // Chegar sem caber não é evento nenhum para quem já assiste: o N deles
        // não mudou, porque a cópia da pessoa nova não foi aberta.
        let (mut voice_room, mut ouvinte) = sala_com_barramento(600_000);
        let _alice = espectador(&mut voice_room, 1);
        let _bob = espectador(&mut voice_room, 2);
        let mut fim = compartilhar(&mut voice_room, 1, 7);
        assert_eq!(contagens(&mut ouvinte), vec![1]);

        let _carol = espectador(&mut voice_room, 3);
        assert!(
            fim.try_recv().is_err(),
            "a chegada de alguém encerrou a transmissão de quem já estava"
        );
        // O número **não muda**, que é o que importa: a cópia de carol não foi
        // aberta, então quem assiste continua sendo um. Que ele seja reanunciado
        // é ruído do reconferir e não mentira — e uma contagem repetida é muito
        // melhor que a que este teste afirmava antes, que era nenhuma, porque a
        // transmissão tinha acabado de morrer.
        assert_eq!(
            contagens(&mut ouvinte),
            vec![1],
            "a chegada de quem não coube mudou a contagem de quem já assistia"
        );
        assert_eq!(voice_room.drops.espectador_nao_coube, 1);
    }

    #[test]
    fn a_sala_que_cresce_alem_da_subida_deixa_de_fora_quem_chegou() {
        // **A decisão que este teste afirmava foi revista em 03/09/2026**, e
        // vale escrever o argumento antigo inteiro porque ele não era tolo:
        //
        // > A subida de quem hospeda é `N × teto`, então cada pessoa que entra
        // > encolhe o teto de todo mundo. Quando ele passa por baixo do piso do
        // > §2, o que para é o vídeo — com motivo — porque a alternativa é a
        // > sala inteira picotando por causa da tela.
        //
        // A conclusão não segue da premissa. Ela supõe **duas** saídas — manter
        // todas as cópias, ou parar o vídeo — e há uma terceira: não abrir a
        // cópia que não cabe. A voz fica protegida exatamente igual, porque o
        // número de cópias no fio é o mesmo dos dois jeitos; o que muda é quem
        // paga. Antes pagavam todos; agora paga quem chegou por último.
        //
        // O relato que forçou a revisão: «não dá pra ver a live se ela tiver
        // iniciada antes do usuário entrar. Um usuário DEVE ser capaz de ver a
        // live, mesmo entrando depois de ela iniciar.» O sintoma era pior que o
        // relatado — a chegada não deixava a pessoa de fora, apagava a tela de
        // todo mundo — e a culpa caía sobre quem tinha acabado de entrar.
        let mut voice_room = VoiceRoom::com_caminho(VoiceRoomId(1), 600_000, None);
        let _alice = espectador(&mut voice_room, 1);
        let mut bob = espectador(&mut voice_room, 2);
        let mut fim = compartilhar(&mut voice_room, 1, 7);
        let mut de_bob = bob.try_recv().unwrap();
        assert!(fim.try_recv().is_err(), "um espectador já não cabia");

        // 360 kbps de teto para dois espectadores seriam 180, abaixo dos 200 do
        // piso. Então carol não entra — e bob continua vendo.
        let _carol = espectador(&mut voice_room, 3);
        assert!(
            fim.try_recv().is_err(),
            "a transmissão foi encerrada porque a sala cresceu"
        );
        assert!(
            de_bob.pedacos.try_recv().is_err(),
            "quem já assistia foi cortado por causa de quem chegou"
        );
        assert_eq!(voice_room.drops.espectador_nao_coube, 1);
    }

    #[test]
    fn a_subida_medida_abre_a_porta_que_a_hipotese_tinha_fechado() {
        // **O portão dividia um número que ninguém tinha medido.**
        //
        // A sala nasce com `caminho_do_server`, que sem `caminho_bps` declarado
        // — e nada neste repositório o declara — é a hipótese de 2 Mbps das
        // provas. A `Subida` mede o cano de verdade a cada segundo e manda o
        // resultado para cada cliente pelo `HostUplink`; o portão que decide
        // quem entra continuava dividindo a hipótese. O produto media a perna
        // certa e não a usava onde ela decide.
        //
        // O gêmeo de cima prova a recusa. Este prova que ela **desfaz** quando
        // a medida chega, que é a metade que faltava.
        //
        // **E sem comando nenhum.** A sala lê o número que `VoiceRooms` mede,
        // e não uma cópia que alguém lhe entrega: uma cópia entregue por
        // mensagem pode ser descartada — `try_send` numa fila cheia —, e a sala
        // ficaria dividindo um número velho para sempre, sem ninguém saber. O
        // comando existe para **disparar o reconferir**, nunca para carregar o
        // número.
        let cano = Arc::new(AtomicU32::new(600_000));
        let mut voice_room =
            VoiceRoom::com_caminho_partilhado(VoiceRoomId(1), Arc::clone(&cano), None);
        let _alice = espectador(&mut voice_room, 1);
        let _bob = espectador(&mut voice_room, 2);
        let _fim = compartilhar(&mut voice_room, 1, 7);

        // 360 kbps para dois espectadores seriam 180, abaixo do piso de 200.
        let _carol = espectador(&mut voice_room, 3);
        assert_eq!(
            voice_room.drops.espectador_nao_coube, 1,
            "carol deveria ter ficado de fora pela hipótese"
        );

        // A medida chega: o cano é maior do que o número com que a sala nasceu.
        // 1,2 Mbps repartidos carregam seis cópias, e a segunda é uma delas.
        cano.store(2_000_000, Ordering::Relaxed);

        let _dave = espectador(&mut voice_room, 4);
        assert_eq!(
            voice_room.drops.espectador_nao_coube, 1,
            "a medida chegou e o portão continuou dividindo a hipótese"
        );
        assert_eq!(
            voice_room.copias(),
            2,
            "a cópia de dave não foi aberta depois da medida"
        );
    }

    #[test]
    fn a_subida_medida_que_encolhe_aperta_o_teto_e_nao_so_o_afrouxa() {
        // **A metade que impede esta mudança de ser um afrouxamento.**
        //
        // Ler a medida em vez da hipótese é útil porque a hipótese quase sempre
        // erra para baixo. Mas ela também erra para cima — numa casa com menos
        // de 2 Mbps de subida —, e uma medida que só abre portas trocaria a
        // proteção do §3.2 por um número maior. O portão tem de andar nos dois
        // sentidos, ou não é portão.
        let cano = Arc::new(AtomicU32::new(2_000_000));
        let mut voice_room =
            VoiceRoom::com_caminho_partilhado(VoiceRoomId(1), Arc::clone(&cano), None);
        let _alice = espectador(&mut voice_room, 1);
        let _bob = espectador(&mut voice_room, 2);
        let _carol = espectador(&mut voice_room, 3);
        let _dave = espectador(&mut voice_room, 4);
        let mut fim = compartilhar(&mut voice_room, 1, 7);
        assert_eq!(voice_room.copias(), 3, "as três cópias cabiam em 2 Mbps");
        assert!(fim.try_recv().is_err());

        // O cano medido é bem menor do que a hipótese: 360 kbps repartidos por
        // três seriam 120, abaixo do piso de 200. O número chega pelo `Arc`; o
        // comando é só o empurrão para reconferir o que já está no ar.
        cano.store(600_000, Ordering::Relaxed);
        voice_room.handle(VoiceRoomCommand::Subida);

        assert_eq!(
            fim.try_recv().ok(),
            Some(crate::tela::FimDaTela::AlemDoQueOHospedeiroCarrega),
            "a medida encolheu e a transmissão continuou subindo o que o cano não tem"
        );
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
            sessao: SessionId(2),
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

    /// A sala mede a subida de quem fala, e conta a quem falou.
    ///
    /// O sinal do ADR 0036. Cem quadros com o de número cinquenta faltando: uma
    /// lacuna de `seq` que é, pela definição do protocolo, um pacote que saiu e
    /// não chegou.
    #[test]
    fn a_sala_relata_a_perda_de_subida_de_quem_fala() {
        let (mut voice_room, mut ouvinte) = sala_com_barramento(crate::tela::CAMINHO_DO_SERVER_BPS);
        let _alice = member(&mut voice_room, 1, 100, true);
        let _bob = member(&mut voice_room, 2, 200, true);

        let inicio = Instant::now();
        for passo in 0..100_u16 {
            if passo == 50 {
                continue;
            }
            voice_room.handle_at(
                VoiceRoomCommand::Datagram {
                    from: Ssrc(100),
                    bytes: datagram(100, passo),
                },
                inicio + std::time::Duration::from_millis(u64::from(passo) * 20),
            );
        }

        let mut relatada = None;
        while let Ok(evento) = ouvinte.try_recv() {
            if let Event::UplinkLoss { person, fraction } = evento {
                assert_eq!(person, PersonId(1), "a perda foi atribuída a outra pessoa");
                relatada = Some(fraction);
            }
        }
        let fracao = relatada.expect("a sala não relatou perda nenhuma");
        assert!(
            (fracao - 0.01).abs() < 0.01,
            "um perdido em cem foi relatado como {fracao}"
        );
    }

    /// Quem fala sem perder nada não vira notícia ruim.
    #[test]
    fn uma_subida_limpa_e_relatada_como_zero() {
        let (mut voice_room, mut ouvinte) = sala_com_barramento(crate::tela::CAMINHO_DO_SERVER_BPS);
        let _alice = member(&mut voice_room, 1, 100, true);
        let _bob = member(&mut voice_room, 2, 200, true);

        let inicio = Instant::now();
        for passo in 0..100_u16 {
            voice_room.handle_at(
                VoiceRoomCommand::Datagram {
                    from: Ssrc(100),
                    bytes: datagram(100, passo),
                },
                inicio + std::time::Duration::from_millis(u64::from(passo) * 20),
            );
        }

        let mut relatada = None;
        while let Ok(evento) = ouvinte.try_recv() {
            if let Event::UplinkLoss { fraction, .. } = evento {
                relatada = Some(fraction);
            }
        }
        assert_eq!(relatada, Some(0.0));
    }

    /// A medida é da rede, e não da política do servidor.
    ///
    /// Um quadro que o limitador de taxa descarta **chegou**. Se ele contasse
    /// como perda, quem estourasse o limite — um cliente com soluço de
    /// escalonamento, que é o caso comum e não o ataque — receberia de volta a
    /// acusação de que a rede dele está ruim, e encolheria o próprio microfone
    /// por causa de uma decisão nossa.
    #[test]
    fn quadro_barrado_pelo_limitador_nao_conta_como_perda() {
        let (mut voice_room, mut ouvinte) = sala_com_barramento(crate::tela::CAMINHO_DO_SERVER_BPS);
        let _alice = member(&mut voice_room, 1, 100, true);
        let _bob = member(&mut voice_room, 2, 200, true);

        // Trezentos quadros em 1,5 s — o dobro do teto de sessenta por segundo,
        // então o balde estoura e o limitador descarta. A sequência, porém, não
        // tem lacuna nenhuma: nada se perdeu na rede.
        //
        // O tempo precisa **andar**: a medida só é recalculada uma vez por
        // segundo, e uma rajada inteira no mesmo instante nunca chega a cruzar
        // esse intervalo com amostra suficiente. A primeira redação deste teste
        // fazia isso e reprovava com `None`.
        let inicio = Instant::now();
        for passo in 0..300_u16 {
            voice_room.handle_at(
                VoiceRoomCommand::Datagram {
                    from: Ssrc(100),
                    bytes: datagram(100, passo),
                },
                inicio + std::time::Duration::from_millis(u64::from(passo) * 5),
            );
        }
        assert!(
            voice_room.drops().rate_limited > 0,
            "o limitador não chegou a barrar nada, e este teste não prova o que prova"
        );

        let mut relatada = None;
        while let Ok(evento) = ouvinte.try_recv() {
            if let Event::UplinkLoss { fraction, .. } = evento {
                relatada = Some(fraction);
            }
        }
        assert_eq!(
            relatada,
            Some(0.0),
            "o que o limitador barrou foi contado como perda de rede"
        );
    }
}
