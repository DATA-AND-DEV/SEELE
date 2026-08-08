//! The Dogma's shared state: storage, the write batch, and the event bus.
//!
//! `specs/04-servidor-magi.md` puts Cage state in a task per Cage with no global
//! lock. Text is different: it is one durable log per Line, and the thing worth
//! avoiding is not contention but `fsync` per message. So storage sits behind a
//! single mutex — SQLite in WAL mode has one writer anyway — and the batching
//! happens in [`spawn_writer`].
//!
//! # Confirmation order
//!
//! A message is broadcast **after** its batch commits, never before. The
//! acceptance criterion in `specs/04-servidor-magi.md` is "reinício não perde
//! mensagem confirmada ao cliente", and announcing before the commit is exactly
//! how that promise gets broken by a power cut nobody planned for.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use magi_proto::control::{PilotProfile, PilotState};
use magi_proto::ids::{CageId, LineId, MessageId, PilotId, Ssrc};
use tokio::sync::{broadcast, mpsc, Mutex};

use crate::casper::messages::{Messages, PendingMessage, StoredMessage};
use crate::casper::Casper;

/// How long the writer waits before committing what it has.
///
/// `specs/04-servidor-magi.md`: "flush por tempo (~200 ms)". Long enough that a
/// busy Line commits once instead of fifty times; short enough that a message
/// still feels sent immediately.
pub const FLUSH_INTERVAL: Duration = Duration::from_millis(200);

/// Events every connection may care about.
///
/// Broadcast to all, filtered per connection. `specs/04-servidor-magi.md` sizes
/// a Dogma at ~50 pilots, so filtering at the edge costs nothing and keeps the
/// bus from needing to know who is subscribed to what.
#[derive(Debug, Clone)]
pub enum Event {
    /// A message was committed and is now durable.
    MessagePosted(StoredMessage),
    /// A message was edited.
    MessageEdited {
        /// Which Line.
        line: LineId,
        /// Which message.
        id: MessageId,
        /// New body.
        body: String,
    },
    /// A message was removed.
    MessageRemoved {
        /// Which Line.
        line: LineId,
        /// Which message.
        id: MessageId,
    },
    /// A pilot entered a Cage.
    PilotJoined {
        /// Which Cage.
        cage: CageId,
        /// Who.
        profile: PilotProfile,
        /// Their media source.
        ssrc: Ssrc,
    },
    /// A pilot left a Cage.
    PilotLeft {
        /// Which Cage.
        cage: CageId,
        /// Who.
        pilot: PilotId,
    },
    /// A pilot's state changed, including their Sync Ratio.
    PilotState(PilotState),
}

/// A Cage seat held open for a pilot who dropped.
///
/// `specs/02-protocolo.md`: "O servidor guarda o slot pelo mesmo período" — the
/// five minutes of the internal battery. Without this a pilot whose train enters
/// a tunnel comes back to find their Cage full.
#[derive(Debug, Clone, Copy)]
struct ReservedSlot {
    cage: CageId,
    ssrc: Ssrc,
    expires_at: Instant,
}

/// Seats held for pilots who are expected back.
#[derive(Debug, Default)]
pub struct Slots {
    reserved: HashMap<PilotId, ReservedSlot>,
}

impl Slots {
    /// Holds a seat for the grace period.
    pub fn reserve(&mut self, pilot: PilotId, cage: CageId, ssrc: Ssrc, now: Instant) {
        self.reserved.insert(
            pilot,
            ReservedSlot {
                cage,
                ssrc,
                expires_at: now + magi_proto::transport::SESSION_GRACE,
            },
        );
    }

    /// Reclaims a seat, if one is still being held.
    ///
    /// Returns the Cage and the `ssrc` the pilot had, so a reconnection lands
    /// where it left off rather than looking like somebody new.
    pub fn reclaim(&mut self, pilot: PilotId, now: Instant) -> Option<(CageId, Ssrc)> {
        let slot = self.reserved.get(&pilot).copied()?;
        if slot.expires_at <= now {
            self.reserved.remove(&pilot);
            return None;
        }
        self.reserved.remove(&pilot);
        Some((slot.cage, slot.ssrc))
    }

    /// Drops seats whose grace period has passed.
    pub fn sweep(&mut self, now: Instant) -> usize {
        let before = self.reserved.len();
        self.reserved.retain(|_, slot| slot.expires_at > now);
        before - self.reserved.len()
    }

    /// How many seats are currently held.
    #[must_use]
    pub fn held(&self) -> usize {
        self.reserved.len()
    }
}

/// Somebody sitting in a Cage right now.
#[derive(Debug, Clone)]
pub struct Occupant {
    /// Who.
    pub pilot: PilotId,
    /// What they are called.
    pub nickname: String,
    /// Their media source.
    pub ssrc: Ssrc,
}

/// Who is in which Cage at this moment.
///
/// Separate from [`Slots`], which holds seats for pilots who are *away*. This
/// is who is actually there, and it exists to answer one question the protocol
/// could not: **who was already here when I walked in.**
///
/// `specs/02-protocolo.md` announces arrivals going forward and nothing else,
/// so a pilot entering an occupied Cage saw an empty room until somebody moved.
/// Gap G15, found by running two clients where the second started after the
/// first had already sat down.
#[derive(Debug, Default)]
pub struct Occupancy {
    by_cage: HashMap<CageId, Vec<Occupant>>,
}

impl Occupancy {
    /// Seats a pilot, replacing any earlier seat they held.
    ///
    /// Replacing rather than appending: a reconnection inside the grace period
    /// re-enters the same Cage, and a roster with the same person twice is a
    /// roster nobody trusts.
    pub fn seat(&mut self, cage: CageId, occupant: Occupant) {
        self.vacate_everywhere(occupant.pilot);
        self.by_cage.entry(cage).or_default().push(occupant);
    }

    /// Removes a pilot from one Cage.
    pub fn vacate(&mut self, cage: CageId, pilot: PilotId) {
        if let Some(seated) = self.by_cage.get_mut(&cage) {
            seated.retain(|occupant| occupant.pilot != pilot);
        }
    }

    /// Removes a pilot from wherever they were. Used when a connection drops.
    pub fn vacate_everywhere(&mut self, pilot: PilotId) {
        for seated in self.by_cage.values_mut() {
            seated.retain(|occupant| occupant.pilot != pilot);
        }
    }

    /// Who is in a Cage, in the order they arrived.
    #[must_use]
    pub fn in_cage(&self, cage: CageId) -> Vec<Occupant> {
        self.by_cage.get(&cage).cloned().unwrap_or_default()
    }
}

/// One message on its way to the batch, with somewhere to report the outcome.
pub struct WriteRequest {
    /// What to write.
    pub message: PendingMessage,
}

/// Everything a connection needs from the Dogma.
pub struct Dogma {
    /// Persistent state. One connection, one mutex — SQLite has one writer.
    pub casper: Arc<Mutex<Casper>>,
    /// The event bus.
    pub events: broadcast::Sender<Event>,
    /// Where messages go to be batched.
    pub writes: mpsc::Sender<WriteRequest>,
    /// Seats held for pilots who are expected back.
    pub slots: Arc<Mutex<Slots>>,
    /// Who is sitting in which Cage right now — gap G15.
    pub occupancy: Arc<Mutex<Occupancy>>,
}

/// Starts the batching writer.
///
/// Collects messages until [`FLUSH_INTERVAL`] elapses, writes them in one
/// transaction, and only then broadcasts them. See the module docs on why the
/// order is fixed.
pub fn spawn_writer(
    casper: Arc<Mutex<Casper>>,
    events: broadcast::Sender<Event>,
) -> mpsc::Sender<WriteRequest> {
    let (tx, mut rx) = mpsc::channel::<WriteRequest>(1024);

    tokio::spawn(async move {
        let mut pending: Vec<PendingMessage> = Vec::new();
        let mut ticker = tokio::time::interval(FLUSH_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                request = rx.recv() => {
                    match request {
                        Some(request) => pending.push(request.message),
                        // The Dogma is shutting down. Flush what is left rather
                        // than dropping messages the clients believe are queued.
                        None => {
                            flush(&casper, &events, &mut pending).await;
                            return;
                        }
                    }
                }
                _ = ticker.tick() => {
                    flush(&casper, &events, &mut pending).await;
                }
            }
        }
    });

    tx
}

async fn flush(
    casper: &Arc<Mutex<Casper>>,
    events: &broadcast::Sender<Event>,
    pending: &mut Vec<PendingMessage>,
) {
    if pending.is_empty() {
        return;
    }
    let batch = std::mem::take(pending);
    let stored = {
        let mut guard = casper.lock().await;
        let mut messages = Messages::new(&mut guard);
        match messages.append_batch(&batch) {
            Ok(stored) => stored,
            Err(error) => {
                // Losing the batch is bad; losing it silently is worse. The
                // clients will not see their messages appear, which is the
                // honest outcome of a write that failed.
                tracing::error!(%error, count = batch.len(), "message batch failed");
                return;
            }
        }
    };

    // Committed, therefore durable, therefore safe to announce.
    for message in stored {
        let _ = events.send(Event::MessagePosted(message));
    }
}

impl Dogma {
    /// Queues a message for the next batch.
    ///
    /// Returns once it is queued, not once it is durable. The caller must not
    /// confirm anything to the client here; the broadcast after the commit is
    /// what does that.
    ///
    /// # Errors
    ///
    /// Fails if the writer task has stopped.
    pub async fn post(&self, message: PendingMessage) -> Result<()> {
        self.writes.send(WriteRequest { message }).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instant() -> Instant {
        Instant::now()
    }

    #[test]
    fn a_seat_is_held_for_the_grace_period() {
        // specs/02-protocolo.md: the server holds the slot for the same five
        // minutes as the client's internal battery.
        let mut slots = Slots::default();
        let now = instant();
        slots.reserve(PilotId(1), CageId(1), Ssrc(7), now);

        let reclaimed = slots.reclaim(PilotId(1), now + Duration::from_secs(60));
        assert_eq!(reclaimed, Some((CageId(1), Ssrc(7))));
    }

    #[test]
    fn a_reconnecting_pilot_gets_their_own_ssrc_back() {
        // Otherwise a sixty-second outage looks to everybody else like the pilot
        // left and a stranger arrived, and every listener's jitter buffer starts
        // from scratch.
        let mut slots = Slots::default();
        let now = instant();
        slots.reserve(PilotId(1), CageId(2), Ssrc(42), now);

        let (cage, ssrc) = slots.reclaim(PilotId(1), now).expect("seat held");
        assert_eq!(cage, CageId(2));
        assert_eq!(ssrc, Ssrc(42));
    }

    #[test]
    fn an_expired_seat_is_not_reclaimable() {
        let mut slots = Slots::default();
        let now = instant();
        slots.reserve(PilotId(1), CageId(1), Ssrc(7), now);

        let after = now + magi_proto::transport::SESSION_GRACE + Duration::from_secs(1);
        assert_eq!(slots.reclaim(PilotId(1), after), None);
    }

    #[test]
    fn reclaiming_twice_only_works_once() {
        // The seat is taken by the reconnection. A second claim would let one
        // pilot occupy two.
        let mut slots = Slots::default();
        let now = instant();
        slots.reserve(PilotId(1), CageId(1), Ssrc(7), now);

        assert!(slots.reclaim(PilotId(1), now).is_some());
        assert!(slots.reclaim(PilotId(1), now).is_none());
    }

    #[test]
    fn the_sweeper_frees_expired_seats() {
        // Without this a Dogma slowly fills with seats held for people who are
        // never coming back, and specs/04 caps a Cage at a member limit.
        let mut slots = Slots::default();
        let now = instant();
        slots.reserve(PilotId(1), CageId(1), Ssrc(1), now);
        slots.reserve(PilotId(2), CageId(1), Ssrc(2), now);
        assert_eq!(slots.held(), 2);

        let after = now + magi_proto::transport::SESSION_GRACE + Duration::from_secs(1);
        assert_eq!(slots.sweep(after), 2);
        assert_eq!(slots.held(), 0);
    }

    #[test]
    fn the_sweeper_leaves_live_seats_alone() {
        let mut slots = Slots::default();
        let now = instant();
        slots.reserve(PilotId(1), CageId(1), Ssrc(1), now);
        assert_eq!(slots.sweep(now + Duration::from_secs(30)), 0);
        assert_eq!(slots.held(), 1);
    }
}
