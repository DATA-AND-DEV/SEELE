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
    /// Every attempt failed. Audio is down until the user intervenes.
    Lost,
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
                    self.next_attempt_at_ms = None;
                    return Action::GiveUp;
                }
                self.enter(DeviceState::Recovering { attempt: next });
                self.backoff_ms = (self.backoff_ms * 2.0).min(MAX_BACKOFF_MS);
                self.next_attempt_at_ms = Some(now_ms + self.backoff_ms);
                Action::Wait
            }

            // An explicit choice by the user clears everything, including a
            // device the supervisor had written off.
            (_, DeviceEvent::UserSelected) => {
                self.enter(DeviceState::Running);
                self.backoff_ms = FIRST_BACKOFF_MS;
                self.next_attempt_at_ms = None;
                Action::Reopen
            }

            (DeviceState::Running, DeviceEvent::Reopened | DeviceEvent::ReopenFailed) => {
                Action::Wait
            }
            (DeviceState::Lost, DeviceEvent::Reopened | DeviceEvent::ReopenFailed) => Action::Wait,
        }
    }

    /// Called on a timer. Returns [`Action::Reopen`] when an attempt is due.
    pub fn poll(&mut self, now_ms: f64) -> Action {
        let DeviceState::Recovering { .. } = self.state else {
            return Action::Wait;
        };
        match self.next_attempt_at_ms {
            Some(due) if now_ms >= due => {
                self.next_attempt_at_ms = None;
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
        assert_eq!(supervisor.poll(1_000_000.0), Action::Wait, "still retrying");
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
    fn a_late_success_after_giving_up_is_ignored() {
        // An attempt already in flight when the supervisor gave up must not
        // silently resurrect it behind the interface's back.
        let mut supervisor = DeviceSupervisor::new();
        supervisor.handle(DeviceEvent::Failed, 0.0);
        for _ in 0..MAX_ATTEMPTS {
            supervisor.handle(DeviceEvent::ReopenFailed, 0.0);
        }
        assert_eq!(supervisor.state(), DeviceState::Lost);

        supervisor.handle(DeviceEvent::Reopened, 9_999.0);
        assert_eq!(supervisor.state(), DeviceState::Lost);
    }
}
