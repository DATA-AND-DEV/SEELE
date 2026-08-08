//! The internal battery — bateria interna.
//!
//! `specs/07-tema-evangelion.md`:
//!
//! > Quando a conexão cai, o cliente não fecha nem mostra um spinner. Ele entra
//! > em **bateria interna**: contagem regressiva de 5 minutos em vermelho,
//! > tentativas de reconexão listadas, interface esmaecida mas ainda legível —
//! > o histórico continua ali para leitura.
//! >
//! > Funcionalmente é um período de graça de sessão, sustentado pela migração de
//! > conexão do QUIC. Narrativamente é exato. Este é o melhor casamento entre
//! > tema e engenharia no projeto — **proteger de simplificações**.
//!
//! `specs/02-protocolo.md` supplies the mechanics: a `Ping` every 5 s, three
//! consecutive misses meaning `Reconectando`, five minutes of exponential
//! backoff, and the server holding the slot for the same period.
//!
//! # Why this is a state machine and not a loop with sleeps
//!
//! Everything here takes time as a parameter rather than reading a clock, so a
//! five-minute countdown is testable in microseconds and always the same way.
//! The alternative — a real timer — makes the one behaviour that matters, what
//! happens at 04:59, the one behaviour nobody ever tests.
//!
//! # What this deliberately does not do
//!
//! It does not close anything, clear anything or forget anything. The spec is
//! explicit that history stays readable while the battery runs, so a shell
//! keeps drawing exactly what it drew before, dimmed. Losing the view when the
//! link drops would be the simplification the spec warns against.

use std::time::Duration;

use magi_proto::transport::{KEEPALIVE, SESSION_GRACE};

/// Consecutive missed pings before the link is considered down.
///
/// `specs/02-protocolo.md`: "Três perdidos consecutivos → estado
/// `Reconectando`."
pub const MISSES_BEFORE_RECONNECT: u32 = 3;

/// First reconnection delay.
const FIRST_BACKOFF: Duration = Duration::from_millis(500);

/// Longest reconnection delay.
///
/// Capped well inside the grace window so the last minute still gets several
/// attempts rather than one long wait that happens to straddle the deadline.
const MAX_BACKOFF: Duration = Duration::from_secs(15);

/// Where the link stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// Connected and answering.
    Online,
    /// Running on the internal battery.
    ///
    /// A shell shows the countdown and the attempts; `specs/07-tema-evangelion.md`
    /// asks for both, in red, over an interface that stays legible.
    InternalBattery {
        /// Reconnection attempts made so far.
        attempts: u32,
    },
    /// The grace window closed. The session is over; history is not.
    Discharged,
}

/// What the caller should do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Nothing right now.
    Wait,
    /// Send a `Ping`. `specs/02-protocolo.md`: every 5 s.
    SendPing,
    /// Try to reconnect.
    Reconnect,
    /// Give up. The five minutes are gone.
    EndSession,
}

/// Tracks the link and the five-minute grace window.
///
/// Time is passed in, never read.
#[derive(Debug)]
pub struct Battery {
    state: Link,
    /// When the last ping was sent, on the caller's clock.
    last_ping_at: Option<Duration>,
    /// Pings sent with no answer.
    outstanding: u32,
    /// When the battery started running.
    discharging_since: Option<Duration>,
    /// When the next reconnection attempt is due.
    next_attempt_at: Option<Duration>,
    backoff: Duration,
    attempts: u32,
}

impl Default for Battery {
    fn default() -> Self {
        Self::new()
    }
}

impl Battery {
    /// A battery on a healthy link.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Link::Online,
            last_ping_at: None,
            outstanding: 0,
            discharging_since: None,
            next_attempt_at: None,
            backoff: FIRST_BACKOFF,
            attempts: 0,
        }
    }

    /// Where the link stands.
    #[must_use]
    pub fn state(&self) -> Link {
        self.state
    }

    /// How much of the five minutes is left.
    ///
    /// `specs/07-tema-evangelion.md` puts this on screen as a countdown from
    /// 04:59. Returns `None` while online, because there is nothing counting.
    #[must_use]
    pub fn remaining(&self, now: Duration) -> Option<Duration> {
        let since = self.discharging_since?;
        Some(SESSION_GRACE.saturating_sub(now.saturating_sub(since)))
    }

    /// Reconnection attempts made in the current outage.
    ///
    /// `specs/07-tema-evangelion.md` asks for the attempts to be listed.
    #[must_use]
    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// A `Pong` arrived. The link is healthy.
    ///
    /// Also the recovery path: a `Pong` during the battery window means the
    /// connection came back, which is what QUIC's connection migration makes
    /// possible without a new session (`specs/01-arquitetura.md`).
    pub fn on_pong(&mut self) {
        self.outstanding = 0;
        self.state = Link::Online;
        self.discharging_since = None;
        self.next_attempt_at = None;
        self.backoff = FIRST_BACKOFF;
        self.attempts = 0;
    }

    /// A reconnection attempt succeeded.
    pub fn on_reconnected(&mut self) {
        self.on_pong();
    }

    /// A reconnection attempt failed.
    pub fn on_reconnect_failed(&mut self, now: Duration) {
        self.attempts += 1;
        self.backoff = (self.backoff * 2).min(MAX_BACKOFF);
        self.next_attempt_at = Some(now + self.backoff);
    }

    /// The transport reported the connection gone, without waiting for pings to
    /// time out.
    pub fn on_connection_lost(&mut self, now: Duration) {
        if self.state == Link::Online {
            self.enter_battery(now);
        }
    }

    /// Called on a timer. Returns what to do.
    pub fn poll(&mut self, now: Duration) -> Action {
        match self.state {
            Link::Online => self.poll_online(now),
            Link::InternalBattery { .. } => self.poll_battery(now),
            Link::Discharged => Action::Wait,
        }
    }

    fn poll_online(&mut self, now: Duration) -> Action {
        let due = match self.last_ping_at {
            None => true,
            Some(last) => now.saturating_sub(last) >= KEEPALIVE,
        };
        if !due {
            return Action::Wait;
        }

        // The previous ping went unanswered, so count it before sending another.
        if self.last_ping_at.is_some() {
            self.outstanding += 1;
        }

        if self.outstanding >= MISSES_BEFORE_RECONNECT {
            self.enter_battery(now);
            return Action::Wait;
        }

        self.last_ping_at = Some(now);
        Action::SendPing
    }

    fn poll_battery(&mut self, now: Duration) -> Action {
        // The five minutes are the whole promise. Checked before anything else,
        // so no amount of retry bookkeeping can extend them.
        if self.remaining(now).is_some_and(|left| left.is_zero()) {
            self.state = Link::Discharged;
            return Action::EndSession;
        }

        match self.next_attempt_at {
            Some(due) if now >= due => {
                self.next_attempt_at = None;
                self.state = Link::InternalBattery {
                    attempts: self.attempts,
                };
                Action::Reconnect
            }
            None => {
                self.state = Link::InternalBattery {
                    attempts: self.attempts,
                };
                Action::Reconnect
            }
            Some(_) => Action::Wait,
        }
    }

    fn enter_battery(&mut self, now: Duration) {
        self.state = Link::InternalBattery { attempts: 0 };
        self.discharging_since = Some(now);
        self.next_attempt_at = None;
        self.backoff = FIRST_BACKOFF;
        self.attempts = 0;
        self.outstanding = 0;
        self.last_ping_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: u64) -> Duration {
        Duration::from_secs(seconds)
    }

    /// Drives a healthy link that answers everything.
    fn healthy(battery: &mut Battery, until: u64) {
        for second in 0..until {
            if battery.poll(at(second)) == Action::SendPing {
                battery.on_pong();
            }
        }
    }

    #[test]
    fn a_healthy_link_pings_on_the_documented_interval() {
        // specs/02-protocolo.md: every 5 s.
        let mut battery = Battery::new();
        let mut pings = 0;
        for second in 0..60 {
            if battery.poll(at(second)) == Action::SendPing {
                pings += 1;
                battery.on_pong();
            }
        }
        assert!(
            (11..=13).contains(&pings),
            "sixty seconds should be about twelve pings, saw {pings}"
        );
        assert_eq!(battery.state(), Link::Online);
    }

    #[test]
    fn three_missed_pings_start_the_battery() {
        // specs/02-protocolo.md: "Três perdidos consecutivos → Reconectando".
        let mut battery = Battery::new();
        // Never answer.
        for second in 0..30 {
            battery.poll(at(second));
        }
        assert!(matches!(battery.state(), Link::InternalBattery { .. }));
    }

    #[test]
    fn two_missed_pings_are_not_enough() {
        // A link that drops one packet must not fall over. The threshold is
        // three, and being off by one here is fifteen seconds of a call the user
        // spends looking at a red screen for no reason.
        let mut battery = Battery::new();
        let mut sent = 0;
        let mut second = 0;
        while sent < MISSES_BEFORE_RECONNECT {
            if battery.poll(at(second)) == Action::SendPing {
                sent += 1;
            }
            second += 1;
            if second > 100 {
                break;
            }
        }
        assert_eq!(battery.state(), Link::Online, "gave up too early");
    }

    #[test]
    fn the_countdown_is_five_minutes() {
        // specs/07-tema-evangelion.md shows 04:59 counting down;
        // specs/02-protocolo.md sets the window at five minutes.
        let mut battery = Battery::new();
        battery.on_connection_lost(at(100));

        assert_eq!(battery.remaining(at(100)), Some(SESSION_GRACE));
        assert_eq!(
            battery.remaining(at(100 + 299)),
            Some(Duration::from_secs(1))
        );
        assert_eq!(battery.remaining(at(100 + 300)), Some(Duration::ZERO));
    }

    #[test]
    fn the_session_ends_when_the_battery_runs_out() {
        let mut battery = Battery::new();
        battery.on_connection_lost(at(0));

        // Fail every attempt for the whole window.
        let mut action = Action::Wait;
        for second in 0..400 {
            action = battery.poll(at(second));
            if action == Action::Reconnect {
                battery.on_reconnect_failed(at(second));
            }
            if action == Action::EndSession {
                break;
            }
        }

        assert_eq!(action, Action::EndSession);
        assert_eq!(battery.state(), Link::Discharged);
    }

    #[test]
    fn the_window_is_not_extended_by_retrying() {
        // The five minutes are the promise. No amount of retry bookkeeping may
        // push the deadline out, or the countdown on screen becomes a lie.
        let mut battery = Battery::new();
        battery.on_connection_lost(at(0));

        let mut ended_at = None;
        for second in 0..600 {
            let action = battery.poll(at(second));
            if action == Action::Reconnect {
                battery.on_reconnect_failed(at(second));
            }
            if action == Action::EndSession {
                ended_at = Some(second);
                break;
            }
        }

        assert_eq!(ended_at, Some(SESSION_GRACE.as_secs()), "the window moved");
    }

    #[test]
    fn a_recovered_link_returns_to_online_and_resets_everything() {
        // specs/01-arquitetura.md: QUIC connection migration is what makes this
        // possible without a new session — "o que torna a bateria interna
        // tecnicamente elegante em vez de gambiarra".
        let mut battery = Battery::new();
        battery.on_connection_lost(at(0));
        battery.poll(at(0));
        battery.on_reconnect_failed(at(0));
        assert!(battery.attempts() > 0);

        battery.on_reconnected();

        assert_eq!(battery.state(), Link::Online);
        assert_eq!(battery.attempts(), 0);
        assert_eq!(battery.remaining(at(10)), None, "still counting down");
    }

    #[test]
    fn a_sixty_second_outage_is_survived() {
        // specs/09-roadmap.md, M3 acceptance: "Queda de rede de 60 s é recuperada
        // de forma transparente."
        let mut battery = Battery::new();
        healthy(&mut battery, 30);

        battery.on_connection_lost(at(30));
        for second in 30..90 {
            if battery.poll(at(second)) == Action::Reconnect {
                battery.on_reconnect_failed(at(second));
            }
        }
        assert!(
            matches!(battery.state(), Link::InternalBattery { .. }),
            "gave up during a sixty second outage"
        );

        battery.on_reconnected();
        assert_eq!(battery.state(), Link::Online);
    }

    #[test]
    fn backoff_grows_and_is_capped() {
        // Retrying flat out against a network that is gone drains a phone; too
        // slowly and the last minute of the window gets one attempt.
        let mut battery = Battery::new();
        battery.on_connection_lost(at(0));

        let mut gaps = Vec::new();
        let mut previous = 0_u64;
        for second in 0..300 {
            if battery.poll(at(second)) == Action::Reconnect {
                gaps.push(second - previous);
                previous = second;
                battery.on_reconnect_failed(at(second));
            }
        }

        assert!(gaps.len() > 5, "too few attempts in five minutes");
        assert!(
            gaps.iter().all(|gap| *gap <= MAX_BACKOFF.as_secs() + 1),
            "backoff exceeded its cap: {gaps:?}"
        );
        assert!(
            gaps.last().copied().unwrap_or(0) >= gaps.first().copied().unwrap_or(0),
            "backoff shrank: {gaps:?}"
        );
    }

    #[test]
    fn attempts_are_counted_for_the_interface() {
        // specs/07-tema-evangelion.md asks for the attempts to be listed, so the
        // count has to be available rather than internal.
        let mut battery = Battery::new();
        battery.on_connection_lost(at(0));
        for second in 0..60 {
            if battery.poll(at(second)) == Action::Reconnect {
                battery.on_reconnect_failed(at(second));
            }
        }
        assert!(battery.attempts() >= 3);
        assert!(matches!(
            battery.state(),
            Link::InternalBattery { attempts } if attempts >= 3
        ));
    }

    #[test]
    fn a_discharged_battery_stops_asking() {
        // Retrying forever after the session is over is how a client ends up
        // hammering a server it has no business talking to.
        let mut battery = Battery::new();
        battery.on_connection_lost(at(0));
        for second in 0..400 {
            if battery.poll(at(second)) == Action::Reconnect {
                battery.on_reconnect_failed(at(second));
            }
        }
        assert_eq!(battery.state(), Link::Discharged);
        assert_eq!(battery.poll(at(10_000)), Action::Wait);
    }

    #[test]
    fn nothing_counts_down_while_the_link_is_healthy() {
        let mut battery = Battery::new();
        healthy(&mut battery, 120);
        assert_eq!(battery.remaining(at(120)), None);
        assert_eq!(battery.attempts(), 0);
    }
}
