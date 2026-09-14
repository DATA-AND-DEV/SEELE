//! Keeps audio alive across device changes.
//!
//! `specs/03-audio.md`:
//!
//! > Handle **device disconnection at run time** (headphones pulled out
//! > mid-call). This will happen and it must not drop the session: pause,
//! > re-enumerate, resume on the new default, tell the interface.
//!
//! "This will happen" is the operative phrase. Someone unplugs a headset in
//! every long call, and the failure mode without this is that audio dies
//! silently while the connection stays up — which reads to the user as the
//! whole product being broken.
//!
//! # Why the state machine is here and not in [`crate::device`]
//!
//! Same rule as everywhere else in this crate: CI has no sound card, so
//! [`crate::device`] cannot be tested. The decisions — when to retry, how long
//! to wait, when to stop trying — are pure and live here, fully covered. The
//! module that actually reopens a stream does nothing but obey.

/// Delay before the first reopen attempt, in milliseconds.
///
/// Not zero: an unplug event usually arrives before the operating system has
/// finished settling on a new default device, and retrying instantly just burns
/// an attempt on the device that is on its way out.
const FIRST_BACKOFF_MS: f64 = 100.0;

/// Ceiling on the backoff.
///
/// Beyond a second the user has already noticed and is reaching for the menu;
/// waiting longer only makes recovery feel broken.
const MAX_BACKOFF_MS: f64 = 1_000.0;

/// How many times to try before giving up and telling the interface.
const MAX_ATTEMPTS: u32 = 8;

/// Intervalo entre as rondas de quem já desistiu, em milissegundos.
///
/// Desistir não pode ser um beco. Quem tira o fone e o religa meio minuto
/// depois — a ligação continua de pé o tempo todo — ficava sem som até ir à
/// tela escolher um aparelho, que é o mesmo «só reiniciando resolve» que esta
/// tarefa existe para acabar.
///
/// Cinco segundos e não o ritmo da recuperação: perdido é perdido, e insistir
/// a cada segundo é o processo girando na máquina de alguém sem nada para
/// achar. Uma tentativa a cada cinco segundos custa uma reenumeração — o
/// mesmo que a tela faz quando alguém abre a lista de aparelhos.
const RONDA_DE_PERDIDO_MS: f64 = 5_000.0;

/// What the audio device layer is currently doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceState {
    /// Audio is flowing.
    Running,
    /// The device failed and reopening is in progress.
    ///
    /// `specs/03-audio.md` wants the interface told about this; it is the audio
    /// equivalent of the reconnection state in `specs/07-estetica.md`.
    Recovering {
        /// Attempts made so far.
        attempt: u32,
    },
    /// Every attempt failed. Audio is down until something changes.
    ///
    /// Não é o fim da linha: a ronda lenta de [`RONDA_DE_PERDIDO_MS`] continua
    /// olhando, porque um aparelho que reaparece tem de voltar a tocar sem
    /// ninguém precisar reiniciar nada. O que muda é o ritmo e o que a
    /// interface diz — «sem aparelho de áudio» enquanto for verdade.
    Lost,
}

/// O que o backend disse quando o fluxo caiu.
///
/// # Por que o tipo do erro não pode ser descartado
///
/// Era o defeito de raiz da troca de aparelho: o retorno de erro do `cpal`
/// chegava a [`crate::device`], virava «mais um» num contador, e o **tipo**
/// morria ali. O tipo é a única parte que separa três coisas muito diferentes:
/// o sistema trocou o aparelho padrão, o aparelho foi embora, ou houve um
/// estalo. As duas primeiras pedem reabertura; a terceira, nada. Sem o tipo,
/// só restam duas opções igualmente erradas — reabrir por qualquer tropeço, ou
/// nunca reabrir. O produto fazia a segunda, e por isso trocar de fone no
/// sistema operacional só valia reiniciando o aplicativo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FalhaDeAparelho {
    /// O sistema mudou a rota de áudio ou o aparelho padrão.
    ///
    /// No Windows/WASAPI o fluxo **continua tocando no aparelho antigo** depois
    /// disto: seguir a troca é trabalho de quem recebe o aviso.
    Trocado,
    /// O aparelho não está mais disponível.
    ///
    /// No macOS a entrada — e qualquer aparelho aberto por id — é *pausada*
    /// pelo `cpal` e nunca despausada. Só reabrir resolve.
    Sumiu,
    /// Um tropeço que não pede aparelho novo: estalo, disputa, limite.
    Transitoria,
}

impl FalhaDeAparelho {
    /// Se esta falha exige reabrir num aparelho reenumerado.
    #[must_use]
    pub fn pede_reabertura(self) -> bool {
        matches!(self, Self::Trocado | Self::Sumiu)
    }
}

/// O que os contadores do fluxo dizem sobre o aparelho, lido de uma vez.
///
/// Números acumulados e não bandeiras: o retorno de erro do `cpal` roda na
/// thread de tempo real, e somar num atômico é tudo o que ele pode fazer.
/// Quem lê compara com o que já tinha visto — ver [`CicloDoAparelho::passo`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AvisoDeAparelho {
    /// Quantas vezes o sistema disse que a rota mudou.
    pub trocas: u64,
    /// Quantas vezes o aparelho foi dado por ausente.
    pub sumicos: u64,
}

/// Quem sabe reenumerar os aparelhos e abrir no padrão de agora.
///
/// Um traço e não uma chamada direta a [`crate::device`] pela regra do
/// cabeçalho deste módulo: a decisão mora aqui, coberta por teste em qualquer
/// máquina, e quem tem placa de som só obedece. É também o que permite provar a
/// troca **por comportamento** num teste — uma máquina de mentira troca o
/// padrão e tira o fone, e o ciclo que roda é o mesmo que roda em produção.
pub trait Reabertura {
    /// O que uma abertura bem-sucedida entrega. `AudioIo`, em produção.
    type Aberto;
    /// Por que não abriu.
    type Erro;

    /// Reenumera e abre. Chamado só quando o ciclo decide que é a hora.
    ///
    /// # Errors
    ///
    /// Quem implementa decide; o ciclo trata qualquer erro como tentativa
    /// falhada e volta a tentar com o intervalo do [`DeviceSupervisor`].
    fn reabrir(&mut self) -> Result<Self::Aberto, Self::Erro>;

    /// O aviso que o aparelho recém-aberto já traz consigo.
    ///
    /// Em produção o laço troca o `AudioIo` inteiro pelo que a reabertura
    /// entregou, e cada abertura em `crate::device::open` cria contadores
    /// **novos e zerados** — é deles que a volta seguinte vai ler. Sem
    /// perguntar aqui, o ciclo continuaria comparando com o número alto do
    /// aparelho anterior, nenhum evento novo venceria o limiar, e a segunda
    /// troca da sessão seria ignorada: quem tira o fone e o põe de volta
    /// ficaria preso ao aparelho da primeira troca.
    ///
    /// Quem reaproveita os mesmos contadores entre aberturas devolve o que
    /// eles dizem agora, e o ponto de comparação simplesmente não anda.
    fn aviso_de(&self, aberto: &Self::Aberto) -> AvisoDeAparelho;
}

/// O [`DeviceSupervisor`] ligado aos contadores e a quem sabe reabrir.
///
/// É a peça que faltava entre os dois: o supervisor decide *quando*, o
/// [`Reabertura`] faz, e isto lê o aviso, não repete o que já viu e leva o
/// resultado de volta ao supervisor. Uma volta do laço de áudio chama
/// [`CicloDoAparelho::passo`] uma vez.
#[derive(Debug)]
pub struct CicloDoAparelho {
    supervisor: DeviceSupervisor,
    trocas_vistas: u64,
    sumicos_vistos: u64,
}

impl Default for CicloDoAparelho {
    fn default() -> Self {
        Self::novo()
    }
}

impl CicloDoAparelho {
    /// Um ciclo para um aparelho que está funcionando.
    #[must_use]
    pub fn novo() -> Self {
        Self {
            supervisor: DeviceSupervisor::new(),
            trocas_vistas: 0,
            sumicos_vistos: 0,
        }
    }

    /// Em que pé está o aparelho.
    #[must_use]
    pub fn estado(&self) -> DeviceState {
        self.supervisor.state()
    }

    /// Quantas vezes o aparelho foi recuperado. A interface redesenha nisto.
    #[must_use]
    pub fn reaberturas(&self) -> u64 {
        self.supervisor.recoveries()
    }

    /// Uma volta: lê o aviso, decide, e reabre quando for a hora.
    ///
    /// Devolve o que abriu, e só nesse caso — o chamador troca o aparelho em uso
    /// pelo que voltar aqui. `None` é o caso comum e não é falha.
    ///
    /// O aviso é comparado com o que já se viu porque os contadores só crescem:
    /// um número lido duas vezes é o mesmo evento, e reagir de novo a ele
    /// reabriria o aparelho a cada volta do laço.
    pub fn passo<R: Reabertura>(
        &mut self,
        aviso: AvisoDeAparelho,
        agora_ms: f64,
        quem: &mut R,
    ) -> Option<R::Aberto> {
        if aviso.trocas > self.trocas_vistas || aviso.sumicos > self.sumicos_vistos {
            self.trocas_vistas = aviso.trocas;
            self.sumicos_vistos = aviso.sumicos;
            self.supervisor.handle(DeviceEvent::Failed, agora_ms);
        }

        if self.supervisor.poll(agora_ms) != Action::Reopen {
            return None;
        }

        match quem.reabrir() {
            Ok(aberto) => {
                // O ponto de comparação passa a ser o do aparelho que acabou de
                // abrir, e não o do que ficou para trás — ver
                // [`Reabertura::aviso_de`].
                let daqui = quem.aviso_de(&aberto);
                self.trocas_vistas = daqui.trocas;
                self.sumicos_vistos = daqui.sumicos;
                self.supervisor.handle(DeviceEvent::Reopened, agora_ms);
                Some(aberto)
            }
            Err(_) => {
                self.supervisor.handle(DeviceEvent::ReopenFailed, agora_ms);
                None
            }
        }
    }
}

/// Something the supervisor needs to know about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceEvent {
    /// The backend reported a stream error, or the device vanished.
    Failed,
    /// A reopen attempt succeeded.
    Reopened,
    /// A reopen attempt failed.
    ReopenFailed,
    /// The user picked a device explicitly, which resets everything.
    ///
    /// # O que hoje **não** entra por aqui, e é honesto dizer
    ///
    /// A troca feita na tela do SEELE não passa por este evento: ela constrói
    /// uma [`crate::device::AudioIo`] nova e um caminho de voz novo, que nasce
    /// com o seu próprio acompanhamento em `Running` — abrir antes de largar o
    /// velho é o que deixa quem escolheu um aparelho que sumiu continuar
    /// falando pelo anterior. Este evento é o caminho para o dia em que a
    /// escolha da pessoa reaproveitar o laço em vez de trocá-lo, e as duas
    /// armadilhas que ele tinha estão consertadas antes disso: ele não declara
    /// `Running` antes de a abertura acontecer, e uma abertura que falha não
    /// some. Ver `a_escolha_do_usuario_nao_declara_sucesso_antes_de_reabrir`.
    UserSelected,
}

/// What the caller should do next.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    /// Nothing to do.
    Wait,
    /// Re-enumerate and try to open the default device.
    Reopen,
    /// Stop trying. The interface should say so.
    GiveUp,
}

/// Pure decision-making for device recovery.
///
/// Time is passed in rather than read, so a ten-second recovery sequence can be
/// tested in microseconds and always the same way.
#[derive(Debug)]
pub struct DeviceSupervisor {
    state: DeviceState,
    /// When the next attempt becomes due, on the caller's clock.
    next_attempt_at_ms: Option<f64>,
    backoff_ms: f64,
    transitions: u64,
    recoveries: u64,
}

impl Default for DeviceSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceSupervisor {
    /// A supervisor for a device that is currently working.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: DeviceState::Running,
            next_attempt_at_ms: None,
            backoff_ms: FIRST_BACKOFF_MS,
            transitions: 0,
            recoveries: 0,
        }
    }

    /// Current state.
    #[must_use]
    pub fn state(&self) -> DeviceState {
        self.state
    }

    /// How many times the device has been successfully recovered.
    #[must_use]
    pub fn recoveries(&self) -> u64 {
        self.recoveries
    }

    /// How many state changes have happened. The interface redraws on these.
    #[must_use]
    pub fn transitions(&self) -> u64 {
        self.transitions
    }

    /// Feeds in an event and returns what to do.
    pub fn handle(&mut self, event: DeviceEvent, now_ms: f64) -> Action {
        match (self.state, event) {
            // A working device fell over. Start recovering, but wait a moment:
            // the OS is often still deciding what the new default is.
            (DeviceState::Running, DeviceEvent::Failed) => {
                self.enter(DeviceState::Recovering { attempt: 0 });
                self.backoff_ms = FIRST_BACKOFF_MS;
                self.next_attempt_at_ms = Some(now_ms + self.backoff_ms);
                Action::Wait
            }

            // Further failures while already recovering are the same event
            // arriving twice; they must not multiply the attempt count.
            (DeviceState::Recovering { .. } | DeviceState::Lost, DeviceEvent::Failed) => {
                Action::Wait
            }

            (DeviceState::Recovering { .. }, DeviceEvent::Reopened) => {
                self.enter(DeviceState::Running);
                self.recoveries += 1;
                self.next_attempt_at_ms = None;
                self.backoff_ms = FIRST_BACKOFF_MS;
                Action::Wait
            }

            (DeviceState::Recovering { attempt }, DeviceEvent::ReopenFailed) => {
                let next = attempt + 1;
                if next >= MAX_ATTEMPTS {
                    self.enter(DeviceState::Lost);
                    // O cronômetro **não** é apagado: ele muda de ritmo. Ver
                    // [`RONDA_DE_PERDIDO_MS`].
                    self.next_attempt_at_ms = Some(now_ms + RONDA_DE_PERDIDO_MS);
                    return Action::GiveUp;
                }
                self.enter(DeviceState::Recovering { attempt: next });
                self.backoff_ms = (self.backoff_ms * 2.0).min(MAX_BACKOFF_MS);
                self.next_attempt_at_ms = Some(now_ms + self.backoff_ms);
                Action::Wait
            }

            // An explicit choice by the user clears everything, including a
            // device the supervisor had written off.
            //
            // O estado **não** vai para `Running` aqui, e essa é a correção do
            // achado F: `Running` quer dizer «há som saindo daqui», e neste
            // ponto nada foi aberto ainda — só foi pedido. Quem põe `Running` é
            // o relato de que a reabertura aconteceu. Enquanto ele não chega, a
            // interface desenha «trocando», que é a verdade; e se a abertura
            // falhar, a falha cai no caminho de tentativa em vez de sumir num
            // par `(Running, ReopenFailed)` que devolvia `Wait`.
            (_, DeviceEvent::UserSelected) => {
                self.enter(DeviceState::Recovering { attempt: 0 });
                self.backoff_ms = FIRST_BACKOFF_MS;
                // Cronômetro armado mesmo entregando `Reopen` agora: ver
                // `poll`, e o achado G.
                self.next_attempt_at_ms = Some(now_ms + self.backoff_ms);
                Action::Reopen
            }

            // Um relato de falha sobre um estado que se julga são é um aparelho
            // que não abriu — não um evento a ignorar. Devolver `Wait` e
            // continuar dizendo `Running` era o produto sabendo e não contando.
            (DeviceState::Running, DeviceEvent::ReopenFailed) => {
                self.enter(DeviceState::Recovering { attempt: 0 });
                self.backoff_ms = FIRST_BACKOFF_MS;
                self.next_attempt_at_ms = Some(now_ms + self.backoff_ms);
                Action::Wait
            }

            (DeviceState::Running, DeviceEvent::Reopened) => Action::Wait,
            // A ronda achou alguma coisa. Voltar a tocar é o ponto inteiro de
            // continuar olhando, e a interface vê a mudança pelo contador de
            // transições, como vê qualquer outra.
            (DeviceState::Lost, DeviceEvent::Reopened) => {
                self.enter(DeviceState::Running);
                self.recoveries += 1;
                self.next_attempt_at_ms = None;
                self.backoff_ms = FIRST_BACKOFF_MS;
                Action::Wait
            }

            // Continua não havendo aparelho. A ronda segue no ritmo dela; sem
            // este rearme a primeira recusa apagaria o cronômetro e a
            // desistência voltaria a ser definitiva.
            (DeviceState::Lost, DeviceEvent::ReopenFailed) => {
                self.next_attempt_at_ms = Some(now_ms + RONDA_DE_PERDIDO_MS);
                Action::Wait
            }
        }
    }

    /// Called on a timer. Returns [`Action::Reopen`] when an attempt is due.
    ///
    /// # Por que o prazo é rearmado em vez de apagado
    ///
    /// Achado G. Entregar `Reopen` e limpar `next_attempt_at_ms` deixava a
    /// recuperação inteira dependendo de o chamador relatar de volta. Um relato
    /// perdido — a thread que morreu, um caminho de erro que não avisa — e o
    /// estado ficava em `Recovering` para sempre: sem tentar de novo, sem
    /// desistir, sem nada que a interface pudesse contar. Rearmar custa uma
    /// tentativa a mais no pior caso e tira do desenho o único jeito de travar
    /// sem cronômetro.
    pub fn poll(&mut self, now_ms: f64) -> Action {
        let intervalo = match self.state {
            DeviceState::Running => return Action::Wait,
            DeviceState::Recovering { .. } => self.backoff_ms,
            // Quem desistiu continua olhando, devagar.
            DeviceState::Lost => RONDA_DE_PERDIDO_MS,
        };
        match self.next_attempt_at_ms {
            Some(due) if now_ms >= due => {
                self.next_attempt_at_ms = Some(now_ms + intervalo);
                Action::Reopen
            }
            _ => Action::Wait,
        }
    }

    fn enter(&mut self, state: DeviceState) {
        if self.state != state {
            self.transitions += 1;
        }
        self.state = state;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_healthy_device_does_nothing() {
        let mut supervisor = DeviceSupervisor::new();
        assert_eq!(supervisor.state(), DeviceState::Running);
        assert_eq!(supervisor.poll(0.0), Action::Wait);
        assert_eq!(supervisor.poll(10_000.0), Action::Wait);
        assert_eq!(supervisor.transitions(), 0);
    }

    #[test]
    fn unplugging_a_headset_recovers_on_the_new_default() {
        // The scenario specs/03-audio.md says will happen.
        let mut supervisor = DeviceSupervisor::new();

        assert_eq!(supervisor.handle(DeviceEvent::Failed, 0.0), Action::Wait);
        assert!(matches!(supervisor.state(), DeviceState::Recovering { .. }));

        // Nothing happens immediately: the OS is still choosing a new default.
        assert_eq!(supervisor.poll(50.0), Action::Wait);
        assert_eq!(supervisor.poll(FIRST_BACKOFF_MS), Action::Reopen);

        assert_eq!(
            supervisor.handle(DeviceEvent::Reopened, 120.0),
            Action::Wait
        );
        assert_eq!(supervisor.state(), DeviceState::Running);
        assert_eq!(supervisor.recoveries(), 1);
    }

    #[test]
    fn repeated_failure_events_do_not_burn_attempts() {
        // A single unplug can produce an error on both streams, and cpal may
        // report it more than once. Counting those as separate failures would
        // exhaust the retry budget before a single attempt was made.
        let mut supervisor = DeviceSupervisor::new();
        supervisor.handle(DeviceEvent::Failed, 0.0);
        supervisor.handle(DeviceEvent::Failed, 1.0);
        supervisor.handle(DeviceEvent::Failed, 2.0);

        assert_eq!(supervisor.state(), DeviceState::Recovering { attempt: 0 });
    }

    /// Advances a virtual clock until the next attempt is due, and returns when.
    fn next_due(supervisor: &mut DeviceSupervisor, from_ms: f64) -> f64 {
        let mut now = from_ms;
        while supervisor.poll(now) == Action::Wait {
            now += 1.0;
            assert!(now < from_ms + 100_000.0, "no attempt ever became due");
        }
        now
    }

    #[test]
    fn backoff_doubles_between_attempts_and_is_capped() {
        // Retrying flat-out against a device that is gone spins a CPU; retrying
        // too slowly makes recovery feel broken. Both halves matter.
        let mut supervisor = DeviceSupervisor::new();
        supervisor.handle(DeviceEvent::Failed, 0.0);

        let mut now = 0.0_f64;
        let mut intervals = Vec::new();
        for _ in 0..6 {
            let due = next_due(&mut supervisor, now);
            intervals.push(due - now);
            now = due;
            supervisor.handle(DeviceEvent::ReopenFailed, now);
        }

        assert_eq!(
            intervals.first().copied(),
            Some(FIRST_BACKOFF_MS),
            "first wait should be the settling delay"
        );
        assert!(
            intervals.windows(2).all(|pair| match pair {
                [a, b] => b >= a,
                _ => true,
            }),
            "backoff must never shrink: {intervals:?}"
        );
        assert!(
            intervals.iter().all(|wait| *wait <= MAX_BACKOFF_MS),
            "backoff exceeded its cap: {intervals:?}"
        );
        assert!(
            intervals.last().copied().unwrap_or(0.0) >= MAX_BACKOFF_MS,
            "backoff never reached its cap: {intervals:?}"
        );
    }

    #[test]
    fn it_gives_up_after_a_bounded_number_of_attempts() {
        // Retrying forever against a device that is gone is how a process ends
        // up spinning in the background of someone's laptop.
        let mut supervisor = DeviceSupervisor::new();
        supervisor.handle(DeviceEvent::Failed, 0.0);

        let mut action = Action::Wait;
        for attempt in 0..MAX_ATTEMPTS {
            action = supervisor.handle(DeviceEvent::ReopenFailed, f64::from(attempt) * 1_000.0);
        }

        assert_eq!(action, Action::GiveUp);
        assert_eq!(supervisor.state(), DeviceState::Lost);
        // O que acaba é o ritmo da recuperação, não o olhar: a ronda de quem
        // desistiu é lenta e está coberta em
        // `o_fone_religado_depois_da_desistencia_volta_a_ter_som`.
        let ultima = f64::from(MAX_ATTEMPTS - 1) * 1_000.0;
        assert_eq!(
            supervisor.poll(ultima + MAX_BACKOFF_MS),
            Action::Wait,
            "still retrying"
        );
    }

    #[test]
    fn o_fone_religado_depois_da_desistencia_volta_a_ter_som() {
        // O achado que sobrou da revisão: desistir era um beco. Quem religa o
        // fone quinze segundos tarde demais ficava sem som até ir à tela
        // escolher um aparelho — que é exatamente o «só reiniciando resolve»
        // desta tarefa, com outra roupa.
        let mut supervisor = DeviceSupervisor::new();
        supervisor.handle(DeviceEvent::Failed, 0.0);
        for _ in 0..MAX_ATTEMPTS {
            supervisor.handle(DeviceEvent::ReopenFailed, 0.0);
        }
        assert_eq!(supervisor.state(), DeviceState::Lost);

        // A ronda é lenta de propósito: perdido é perdido, e insistir no ritmo
        // da recuperação é o processo girando na máquina de alguém.
        assert_eq!(
            supervisor.poll(1_000.0),
            Action::Wait,
            "depois de desistir, a ronda não pode ter o ritmo de quem não desistiu"
        );
        assert_eq!(
            supervisor.poll(60_000.0),
            Action::Reopen,
            "o aparelho reapareceu e ninguém foi ver"
        );

        supervisor.handle(DeviceEvent::Reopened, 60_010.0);
        assert_eq!(supervisor.state(), DeviceState::Running);
        assert_eq!(supervisor.recoveries(), 1);
    }

    #[test]
    fn a_ronda_de_quem_desistiu_nao_para_na_primeira_recusa() {
        // Uma ronda que se apaga ao falhar é a mesma desistência de antes, só
        // que cinco segundos depois.
        let mut supervisor = DeviceSupervisor::new();
        supervisor.handle(DeviceEvent::Failed, 0.0);
        for _ in 0..MAX_ATTEMPTS {
            supervisor.handle(DeviceEvent::ReopenFailed, 0.0);
        }
        assert_eq!(supervisor.state(), DeviceState::Lost);

        let mut agora = 0.0;
        for ronda in 0..4 {
            let mut tentou = false;
            for _ in 0..1_000 {
                agora += 100.0;
                if supervisor.poll(agora) == Action::Reopen {
                    tentou = true;
                    break;
                }
            }
            assert!(tentou, "a ronda {ronda} nunca veio");
            supervisor.handle(DeviceEvent::ReopenFailed, agora);
            assert_eq!(supervisor.state(), DeviceState::Lost, "ronda {ronda}");
        }
    }

    #[test]
    fn the_user_can_always_recover_a_lost_device() {
        // Giving up must never be permanent from the user's side. Plugging the
        // headset back in and picking it has to work.
        let mut supervisor = DeviceSupervisor::new();
        supervisor.handle(DeviceEvent::Failed, 0.0);
        for _ in 0..MAX_ATTEMPTS {
            supervisor.handle(DeviceEvent::ReopenFailed, 0.0);
        }
        assert_eq!(supervisor.state(), DeviceState::Lost);

        assert_eq!(
            supervisor.handle(DeviceEvent::UserSelected, 5_000.0),
            Action::Reopen
        );
        // Escolher tira o aparelho de `Lost` e manda reabrir, mas quem declara
        // `Running` é a reabertura que aconteceu — ver o achado F.
        assert!(matches!(supervisor.state(), DeviceState::Recovering { .. }));
        supervisor.handle(DeviceEvent::Reopened, 5_100.0);
        assert_eq!(supervisor.state(), DeviceState::Running);
    }

    #[test]
    fn recovering_twice_is_counted_twice() {
        let mut supervisor = DeviceSupervisor::new();
        for round in 0..2 {
            let base = f64::from(round) * 10_000.0;
            supervisor.handle(DeviceEvent::Failed, base);
            supervisor.poll(base + FIRST_BACKOFF_MS);
            supervisor.handle(DeviceEvent::Reopened, base + 200.0);
        }
        assert_eq!(supervisor.recoveries(), 2);
        assert_eq!(supervisor.state(), DeviceState::Running);
    }

    #[test]
    fn every_state_change_is_visible_to_the_interface() {
        // specs/03-audio.md: "tell the interface". A recovery the user never
        // sees is indistinguishable from a glitch they will report as a bug.
        let mut supervisor = DeviceSupervisor::new();
        let before = supervisor.transitions();

        supervisor.handle(DeviceEvent::Failed, 0.0);
        supervisor.handle(DeviceEvent::Reopened, 200.0);

        assert!(
            supervisor.transitions() >= before + 2,
            "going down and coming back must be two visible changes"
        );
    }

    #[test]
    fn uma_abertura_depois_da_desistencia_nao_acontece_escondida() {
        // Este teste dizia o contrário: uma reabertura chegando depois da
        // desistência era ignorada, para não «ressuscitar o áudio pelas costas
        // da interface». A metade certa daquele medo era a interface, não o
        // áudio — e ela é atendida pelo contador de transições, que é onde a
        // tela repara que algo mudou. Ignorar a abertura era jogar fora som que
        // já estava aberto e deixar a pessoa sem áudio com um aparelho
        // funcionando na mão.
        let mut supervisor = DeviceSupervisor::new();
        supervisor.handle(DeviceEvent::Failed, 0.0);
        for _ in 0..MAX_ATTEMPTS {
            supervisor.handle(DeviceEvent::ReopenFailed, 0.0);
        }
        assert_eq!(supervisor.state(), DeviceState::Lost);
        let antes = supervisor.transitions();

        supervisor.handle(DeviceEvent::Reopened, 9_999.0);

        assert_eq!(supervisor.state(), DeviceState::Running);
        assert_eq!(supervisor.recoveries(), 1);
        assert!(
            supervisor.transitions() > antes,
            "voltar a ter som é uma mudança que a interface tem de ver"
        );
    }

    #[test]
    fn a_escolha_do_usuario_nao_declara_sucesso_antes_de_reabrir() {
        // Achado F. `Running` quer dizer «há som saindo daqui». Pô-lo antes de
        // a reabertura acontecer é o produto afirmando o que ele ainda vai
        // tentar — e é a interface desenhando normalidade sobre um silêncio.
        let mut supervisor = DeviceSupervisor::new();

        assert_eq!(
            supervisor.handle(DeviceEvent::UserSelected, 0.0),
            Action::Reopen
        );
        assert!(
            matches!(supervisor.state(), DeviceState::Recovering { .. }),
            "a escolha ainda não abriu nada; o estado não pode ser {:?}",
            supervisor.state()
        );

        assert_eq!(supervisor.handle(DeviceEvent::Reopened, 10.0), Action::Wait);
        assert_eq!(supervisor.state(), DeviceState::Running);
    }

    #[test]
    fn a_escolha_do_usuario_que_nao_abre_continua_sendo_tentada() {
        // A outra metade do achado F: com o estado em `Running`, o par
        // `(Running, ReopenFailed)` caía em `Wait` e a falha sumia para sempre.
        let mut supervisor = DeviceSupervisor::new();
        supervisor.handle(DeviceEvent::UserSelected, 0.0);

        assert_eq!(
            supervisor.handle(DeviceEvent::ReopenFailed, 1.0),
            Action::Wait
        );
        assert!(
            matches!(supervisor.state(), DeviceState::Recovering { attempt } if attempt >= 1),
            "a falha da reabertura foi engolida: {:?}",
            supervisor.state()
        );
        let due = next_due(&mut supervisor, 1.0);
        assert!(due > 1.0, "nova tentativa tem de ter prazo");
    }

    #[test]
    fn uma_falha_de_reabertura_sobre_um_estado_sao_nao_e_engolida() {
        // Um relato de falha que chega quando o supervisor se julga são é um
        // aparelho que não abriu. Devolver `Wait` e continuar dizendo `Running`
        // é a definição de «o produto sabe e não conta».
        let mut supervisor = DeviceSupervisor::new();

        supervisor.handle(DeviceEvent::ReopenFailed, 0.0);

        assert!(
            matches!(supervisor.state(), DeviceState::Recovering { .. }),
            "estado depois da falha: {:?}",
            supervisor.state()
        );
        assert_eq!(supervisor.poll(FIRST_BACKOFF_MS), Action::Reopen);
    }

    #[test]
    fn quem_nao_relata_a_tentativa_nao_fica_sem_cronometro() {
        // Achado G. `poll` entregava `Reopen` e limpava o prazo; se o relato de
        // volta se perdesse — uma thread que morreu, um `Err` num caminho que
        // não avisa —, o estado ficava em `Recovering` para sempre, sem nunca
        // mais tentar e sem nunca desistir. Silêncio permanente com cara de
        // «recuperando».
        let mut supervisor = DeviceSupervisor::new();
        supervisor.handle(DeviceEvent::Failed, 0.0);

        assert_eq!(supervisor.poll(FIRST_BACKOFF_MS), Action::Reopen);

        // Ninguém relata nada. A próxima tentativa ainda tem de vencer.
        let due = next_due(&mut supervisor, FIRST_BACKOFF_MS);
        assert!(
            due <= FIRST_BACKOFF_MS + MAX_BACKOFF_MS,
            "sem relato de volta, a próxima tentativa venceu em {due} ms"
        );
    }

    /// Um aparelho de mentira, para exercitar o ciclo sem placa de som.
    ///
    /// Ele é a **máquina**: tem uma lista de aparelhos e um padrão, e o que o
    /// teste faz com ele é o que uma pessoa faz com a bandeja do sistema —
    /// trocar o padrão, tirar o fone da tomada.
    struct MaquinaDeMentira {
        padrao: Option<&'static str>,
        aberturas: u32,
        /// O que os contadores desta máquina dizem — ela só tem um jogo deles.
        visto: AvisoDeAparelho,
    }

    impl MaquinaDeMentira {
        fn nova(padrao: &'static str) -> Self {
            Self {
                padrao: Some(padrao),
                aberturas: 0,
                visto: AvisoDeAparelho::default(),
            }
        }
    }

    impl Reabertura for MaquinaDeMentira {
        type Aberto = &'static str;
        type Erro = &'static str;

        fn reabrir(&mut self) -> Result<Self::Aberto, Self::Erro> {
            self.aberturas += 1;
            self.padrao.ok_or("esta máquina não tem aparelho nenhum")
        }

        /// Esta máquina não troca de contadores: o aviso que o ciclo já viu
        /// continua valendo, e é o que estes testes exercitam.
        fn aviso_de(&self, _aberto: &Self::Aberto) -> AvisoDeAparelho {
            self.visto
        }
    }

    #[test]
    fn a_troca_do_padrao_do_sistema_reabre_no_aparelho_novo() {
        let mut maquina = MaquinaDeMentira::nova("fone-usb");
        let mut ciclo = CicloDoAparelho::novo();

        // Nada aconteceu ainda: ninguém reabre por nada.
        assert_eq!(
            ciclo.passo(AvisoDeAparelho::default(), 0.0, &mut maquina),
            None
        );
        assert_eq!(maquina.aberturas, 0);

        // A pessoa troca o padrão na bandeja do sistema. O cpal avisa pelo
        // retorno de erro e continua tocando no aparelho antigo.
        maquina.padrao = Some("caixas-da-mesa");
        maquina.visto = AvisoDeAparelho {
            trocas: 1,
            sumicos: 0,
        };
        let aviso = maquina.visto;

        assert_eq!(
            ciclo.passo(aviso, 0.0, &mut maquina),
            None,
            "o sistema ainda \
             está decidindo qual é o novo padrão; reabrir agora queima a tentativa"
        );
        assert!(matches!(ciclo.estado(), DeviceState::Recovering { .. }));

        assert_eq!(
            ciclo.passo(aviso, FIRST_BACKOFF_MS, &mut maquina),
            Some("caixas-da-mesa")
        );
        assert_eq!(ciclo.estado(), DeviceState::Running);
        assert_eq!(ciclo.reaberturas(), 1);
    }

    #[test]
    fn o_mesmo_aviso_lido_de_novo_nao_reabre_duas_vezes() {
        // Os contadores só crescem: o laço lê o mesmo número a cada volta, e
        // reabrir por ele outra vez seria trocar de aparelho cinquenta vezes
        // por segundo.
        let mut maquina = MaquinaDeMentira::nova("fone-usb");
        let mut ciclo = CicloDoAparelho::novo();
        maquina.visto = AvisoDeAparelho {
            trocas: 1,
            sumicos: 0,
        };
        let aviso = maquina.visto;

        ciclo.passo(aviso, 0.0, &mut maquina);
        assert!(ciclo.passo(aviso, FIRST_BACKOFF_MS, &mut maquina).is_some());

        for volta in 0..50 {
            assert_eq!(
                ciclo.passo(aviso, FIRST_BACKOFF_MS + f64::from(volta), &mut maquina),
                None
            );
        }
        assert_eq!(maquina.aberturas, 1);
    }

    #[test]
    fn a_retirada_do_aparelho_e_seguida_ate_a_maquina_ter_um_de_novo() {
        // O fone sai da tomada e a máquina fica sem padrão nenhum por um
        // instante — é o que acontece de verdade entre o aparelho sumir e o
        // sistema escolher outro. O ciclo tem de continuar tentando.
        let mut maquina = MaquinaDeMentira::nova("fone-usb");
        maquina.padrao = None;
        let mut ciclo = CicloDoAparelho::novo();
        maquina.visto = AvisoDeAparelho {
            trocas: 0,
            sumicos: 1,
        };
        let aviso = maquina.visto;

        ciclo.passo(aviso, 0.0, &mut maquina);
        assert_eq!(ciclo.passo(aviso, FIRST_BACKOFF_MS, &mut maquina), None);
        assert_eq!(maquina.aberturas, 1, "tentou e não conseguiu");
        assert!(matches!(ciclo.estado(), DeviceState::Recovering { .. }));

        maquina.padrao = Some("alto-falante-do-laptop");
        let mut agora = FIRST_BACKOFF_MS;
        let mut aberto = None;
        for _ in 0..20 {
            agora += FIRST_BACKOFF_MS;
            if let Some(novo) = ciclo.passo(aviso, agora, &mut maquina) {
                aberto = Some(novo);
                break;
            }
        }
        assert_eq!(aberto, Some("alto-falante-do-laptop"));
        assert_eq!(ciclo.estado(), DeviceState::Running);
    }

    #[test]
    fn um_tropeco_transitorio_nao_derruba_o_aparelho() {
        // `AvisoDeAparelho` só cresce com troca e sumiço; um `Xrun` não chega
        // aqui. Se chegasse, cada estalo custaria um segundo de silêncio.
        let mut maquina = MaquinaDeMentira::nova("fone-usb");
        let mut ciclo = CicloDoAparelho::novo();

        for volta in 0..100 {
            assert_eq!(
                ciclo.passo(
                    AvisoDeAparelho::default(),
                    f64::from(volta) * 10.0,
                    &mut maquina
                ),
                None
            );
        }
        assert_eq!(maquina.aberturas, 0);
        assert_eq!(ciclo.estado(), DeviceState::Running);
    }

    /// Um aparelho de mentira que traz os **próprios contadores**, como o de
    /// verdade: cada abertura em `device::open` cria um `StreamCounters` novo e
    /// zerado, e o laço passa a ler esse.
    struct MaquinaComContadores {
        padrao: Option<&'static str>,
        aberturas: u32,
    }

    struct AparelhoComContadores {
        nome: &'static str,
        contadores: std::sync::Arc<crate::rt::StreamCounters>,
    }

    impl Reabertura for MaquinaComContadores {
        type Aberto = AparelhoComContadores;
        type Erro = &'static str;

        fn reabrir(&mut self) -> Result<Self::Aberto, Self::Erro> {
            self.aberturas += 1;
            self.padrao
                .map(|nome| AparelhoComContadores {
                    nome,
                    contadores: crate::rt::StreamCounters::shared(),
                })
                .ok_or("esta máquina não tem aparelho nenhum")
        }

        fn aviso_de(&self, aberto: &Self::Aberto) -> AvisoDeAparelho {
            aberto.contadores.aviso_de_aparelho()
        }
    }

    #[test]
    fn a_segunda_troca_da_sessao_tambem_e_seguida() {
        // Quem troca o fone uma vez troca de volta. Em produção a reabertura
        // entrega contadores **novos e zerados**, e um ciclo que guardasse o
        // número alto do aparelho anterior nunca mais veria evento nenhum: a
        // pessoa ficaria presa ao aparelho da primeira troca — o mesmo defeito
        // desta tarefa, só que adiado para a segunda vez.
        let mut maquina = MaquinaComContadores {
            padrao: Some("fone-usb"),
            aberturas: 0,
        };
        let mut ciclo = CicloDoAparelho::novo();
        let mut atual = crate::rt::StreamCounters::shared();
        let mut agora = 0.0;

        for troca in ["caixas-da-mesa", "fone-usb"] {
            maquina.padrao = Some(troca);
            atual.record_stream_error(FalhaDeAparelho::Trocado);

            let mut aberto = None;
            for _ in 0..500 {
                agora += 10.0;
                if let Some(novo) = ciclo.passo(atual.aviso_de_aparelho(), agora, &mut maquina) {
                    aberto = Some(novo);
                    break;
                }
            }
            let aberto = aberto.unwrap_or_else(|| {
                panic!(
                    "a troca para «{troca}» nunca foi seguida: o laço ficou no aparelho anterior"
                )
            });
            assert_eq!(aberto.nome, troca);
            // O laço troca o `AudioIo` inteiro, e com ele os contadores.
            atual = aberto.contadores;
        }

        assert_eq!(ciclo.reaberturas(), 2);
    }
}
