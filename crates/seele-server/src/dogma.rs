//! The Dogma's shared state: storage, the write batch, and the event bus.
//!
//! `specs/04-servidor-seele.md` puts Cage state in a task per Cage with no global
//! lock. Text is different: it is one durable log per Line, and the thing worth
//! avoiding is not contention but `fsync` per message. So storage sits behind a
//! single mutex — SQLite in WAL mode has one writer anyway — and the batching
//! happens in [`spawn_writer`].
//!
//! # Confirmation order
//!
//! A message is broadcast **after** its batch commits, never before. The
//! acceptance criterion in `specs/04-servidor-seele.md` is "reinício não perde
//! mensagem confirmada ao cliente", and announcing before the commit is exactly
//! how that promise gets broken by a power cut nobody planned for.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use seele_proto::control::{CageInfo, LineInfo, PilotProfile, PilotState};
use seele_proto::ids::{CageId, LineId, MessageId, PilotId, Ssrc};
use tokio::sync::{broadcast, mpsc, Mutex};

use crate::casper::messages::{Messages, PendingMessage, StoredMessage};
use crate::casper::Casper;

/// How long the writer waits before committing what it has.
///
/// `specs/04-servidor-seele.md`: "flush por tempo (~200 ms)". Long enough that a
/// busy Line commits once instead of fifty times; short enough that a message
/// still feels sent immediately.
pub const FLUSH_INTERVAL: Duration = Duration::from_millis(200);

/// Events every connection may care about.
///
/// Broadcast to all, filtered per connection. `specs/04-servidor-seele.md` sizes
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
    /// A Cage was created.
    ///
    /// Announced to **everybody**, the pilot who asked included, and this is the
    /// difference between a feature and a demonstration: a room that only shows
    /// up on the next handshake is a room whose maker has to tell their friends
    /// to reconnect before they can use it.
    CageCreated {
        /// The Cage, as it now exists.
        cage: CageInfo,
    },
    /// A Line was created.
    LineCreated {
        /// The Line, as it now exists.
        line: LineInfo,
    },
    /// A Cage was renamed.
    CageRenamed {
        /// Which Cage.
        cage: CageId,
        /// Its new name.
        name: String,
    },
    /// A Line was renamed.
    LineRenamed {
        /// Which Line.
        line: LineId,
        /// Its new name.
        name: String,
    },
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
                expires_at: now + seele_proto::transport::SESSION_GRACE,
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
/// could not: **who was already here before I was watching.**
///
/// `specs/02-protocolo.md` announces arrivals going forward and nothing else,
/// so a pilot entering an occupied Cage saw an empty room until somebody moved.
/// Gap G15, found by running two clients where the second started after the
/// first had already sat down.
///
/// # Why the whole map, and not one Cage
///
/// G15 was closed for the Cage the pilot walked into, and only that one. The
/// screen `design/Entry Plug v3.dc.html` draws occupants under **every** Cage,
/// and for the other four that data had never existed on the client at all:
/// they were drawn empty, always, however many people were in them. Reported
/// from a real session as "o sistema de cages não está bem implementado,
/// mostra que as cages estão vazias quando não deveriam estar".
///
/// So [`Occupancy::everywhere`] hands back the entire picture, and a connection
/// is given it once, at the start of its session. Everything after that is the
/// unfiltered `PilotJoined` / `PilotLeft` broadcast.
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
        let _ = self.vacate_everywhere(occupant.pilot);
        self.by_cage.entry(cage).or_default().push(occupant);
    }

    /// Removes a pilot from one Cage.
    pub fn vacate(&mut self, cage: CageId, pilot: PilotId) {
        if let Some(seated) = self.by_cage.get_mut(&cage) {
            seated.retain(|occupant| occupant.pilot != pilot);
        }
    }

    /// Removes a pilot from wherever they were, and says where that was.
    ///
    /// The Cages come back because somebody has to announce the departure and
    /// the caller does not always know the room: a session can end at any `?`
    /// in the middle of its loop, and that path has no idea where the pilot was
    /// sitting. Returning the answer here is what lets one call at the end of a
    /// connection both clear the seat and tell everybody about it — the same
    /// reasoning `crate::cage::Cages::leave_everywhere` gives for being
    /// broadcast rather than aimed.
    pub fn vacate_everywhere(&mut self, pilot: PilotId) -> Vec<CageId> {
        let mut vacated = Vec::new();
        for (cage, seated) in &mut self.by_cage {
            let before = seated.len();
            seated.retain(|occupant| occupant.pilot != pilot);
            if seated.len() != before {
                vacated.push(*cage);
            }
        }
        vacated
    }

    /// Who is in a Cage, in the order they arrived.
    #[must_use]
    pub fn in_cage(&self, cage: CageId) -> Vec<Occupant> {
        self.by_cage.get(&cage).cloned().unwrap_or_default()
    }

    /// Everybody seated anywhere, with the Cage they are seated in.
    ///
    /// Flattened rather than handed back as a map, because the only caller
    /// walks it once to write a frame per occupant, and a map would make that
    /// caller nest two loops to say one thing.
    #[must_use]
    pub fn everywhere(&self) -> Vec<(CageId, Occupant)> {
        self.by_cage
            .iter()
            .flat_map(|(cage, seated)| seated.iter().map(|occupant| (*cage, occupant.clone())))
            .collect()
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
    /// Quantos apertos de mão cada endereço ainda pode gastar.
    ///
    /// Antes de autenticar, portanto sem identidade nenhuma para contar: a
    /// chave é o endereço de origem. Ver [`crate::taxa`].
    pub portaria: Arc<Mutex<crate::taxa::Portaria>>,
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

        let after = now + seele_proto::transport::SESSION_GRACE + Duration::from_secs(1);
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

        let after = now + seele_proto::transport::SESSION_GRACE + Duration::from_secs(1);
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

    // ---- who is in which Cage ----

    fn occupant(pilot: u64, nickname: &str) -> Occupant {
        Occupant {
            pilot: PilotId(pilot),
            nickname: nickname.to_owned(),
            ssrc: Ssrc(u32::try_from(pilot * 10).expect("ssrc")),
        }
    }

    #[test]
    fn the_whole_dogma_is_readable_at_once_and_not_one_room_at_a_time() {
        // The half of gap G15 that was missing. `in_cage` answered "who is in
        // the room I am walking into"; nothing answered "who is in the other
        // four", and the v3 layout draws those four with their occupants under
        // them. They were drawn empty however many people were in them.
        let mut occupancy = Occupancy::default();
        occupancy.seat(CageId(1), occupant(1, "ayanami"));
        occupancy.seat(CageId(1), occupant(2, "shinji"));
        occupancy.seat(CageId(2), occupant(3, "asuka"));

        let mut everywhere: Vec<(u32, u64)> = occupancy
            .everywhere()
            .into_iter()
            .map(|(cage, seated)| (cage.0, seated.pilot.0))
            .collect();
        everywhere.sort_unstable();

        assert_eq!(everywhere, [(1, 1), (1, 2), (2, 3)]);
    }

    #[test]
    fn leaving_says_which_rooms_were_left() {
        // The caller that needs this is the end of a connection, which does not
        // know where the pilot was sitting: a session can end at any `?`. If
        // this said nothing, the departure could not be announced, and the
        // pilot would stay on everybody's screen until they came back.
        let mut occupancy = Occupancy::default();
        occupancy.seat(CageId(7), occupant(1, "ayanami"));

        assert_eq!(occupancy.vacate_everywhere(PilotId(1)), vec![CageId(7)]);
        assert!(occupancy.everywhere().is_empty());
    }

    #[test]
    fn leaving_a_room_nobody_was_in_announces_nothing() {
        // The other half, and the one that keeps a departure from being sent
        // twice: `serve` calls this after every session, including the ones that
        // already left through `EjectPlug` and said so.
        let mut occupancy = Occupancy::default();
        occupancy.seat(CageId(7), occupant(1, "ayanami"));
        occupancy.vacate(CageId(7), PilotId(1));

        assert!(
            occupancy.vacate_everywhere(PilotId(1)).is_empty(),
            "a pilot who had already left was announced as leaving again"
        );
    }

    #[test]
    fn walking_between_rooms_reports_the_room_that_was_left() {
        // What `InsertPlug` needs in order to tell the old room. Seating alone
        // clears the previous seat silently, and a silent clear is a pilot who
        // stays in the first Cage on every other client for ever.
        let mut occupancy = Occupancy::default();
        occupancy.seat(CageId(1), occupant(1, "ayanami"));

        assert_eq!(occupancy.vacate_everywhere(PilotId(1)), vec![CageId(1)]);
        occupancy.seat(CageId(2), occupant(1, "ayanami"));

        assert_eq!(occupancy.in_cage(CageId(1)).len(), 0);
        assert_eq!(occupancy.in_cage(CageId(2)).len(), 1);
    }
}
