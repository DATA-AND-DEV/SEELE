//! BALTHASAR — media routing.
//!
//! `specs/04-servidor-seele.md`:
//!
//! > 1. Receive a datagram from a known `ssrc`.
//! > 2. Validate that the sender is in the Cage and has permission to speak.
//! >    **Always validate** — do not trust the client.
//! > 3. Forward the payload intact to every other subscriber of the Cage.
//! > 4. Never decode the Opus.
//!
//! Never decoding is what keeps the server's CPU flat regardless of how many
//! people are talking, and it is the precondition that makes end-to-end
//! encryption an increment rather than a rewrite (`specs/01-arquitetura.md`).
//!
//! # One task per Cage
//!
//! `specs/04-servidor-seele.md`: "one task per **Cage**, owning that Cage's
//! state. In and out by `mpsc`. This eliminates the global lock and makes media
//! routing trivially parallel." No `Mutex` appears in this module.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use seele_proto::ids::{CageId, PilotId, Ssrc};
use seele_proto::transport::MAX_FRAMES_PER_SECOND;
use tokio::sync::mpsc;

/// How many datagrams a Cage task will hold before it starts shedding.
///
/// Fifty a second per talker (`specs/03-audio.md`), so this is roughly a
/// second of a full Cage. A queue that grows past this is a task that has
/// stopped keeping up, and buffering more of it only adds latency to audio that
/// is already late.
const CHANNEL_DEPTH: usize = 1024;

/// What a connection asks its Cage to do.
pub enum CageCommand {
    /// A pilot entered. `specs/07-tema-evangelion.md` calls it "inserir plug".
    Join {
        /// Who.
        pilot: PilotId,
        /// The media source the server assigned to their connection.
        ssrc: Ssrc,
        /// Whether they may transmit.
        may_speak: bool,
        /// Where to deliver the datagrams they should hear.
        outbound: mpsc::Sender<Vec<u8>>,
    },
    /// A pilot left, or their connection dropped.
    Leave {
        /// Who.
        pilot: PilotId,
    },
    /// A datagram arrived from a connection.
    Datagram {
        /// Which connection sent it. Taken from the connection, **never** from
        /// the datagram. The two are compared before anything is forwarded,
        /// which is what stops one pilot being credited with another's audio.
        from: Ssrc,
        /// The bytes as received, header included.
        bytes: Vec<u8>,
    },
}

/// Why a datagram was not forwarded.
///
/// Counted rather than logged per event: at fifty frames a second per talker, a
/// log line per drop is its own denial of service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DropCounts {
    /// The sender is not in this Cage.
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
}

/// One member of a Cage.
struct Member {
    ssrc: Ssrc,
    may_speak: bool,
    outbound: mpsc::Sender<Vec<u8>>,
    /// Start of the current rate-limiting window.
    window_started: Instant,
    /// Frames seen in the current window.
    window_frames: u32,
}

/// The state of one voice channel.
pub struct Cage {
    id: CageId,
    members: HashMap<PilotId, Member>,
    /// Reverse index, so a datagram's sender is found without scanning.
    by_ssrc: HashMap<Ssrc, PilotId>,
    drops: DropCounts,
    forwarded: u64,
}

impl Cage {
    /// An empty Cage.
    #[must_use]
    pub fn new(id: CageId) -> Self {
        Self {
            id,
            members: HashMap::new(),
            by_ssrc: HashMap::new(),
            drops: DropCounts::default(),
            forwarded: 0,
        }
    }

    /// Which Cage this is.
    #[must_use]
    pub fn id(&self) -> CageId {
        self.id
    }

    /// How many pilots are inside.
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
    pub fn handle(&mut self, command: CageCommand) {
        match command {
            CageCommand::Join {
                pilot,
                ssrc,
                may_speak,
                outbound,
            } => {
                self.by_ssrc.insert(ssrc, pilot);
                self.members.insert(
                    pilot,
                    Member {
                        ssrc,
                        may_speak,
                        outbound,
                        window_started: Instant::now(),
                        window_frames: 0,
                    },
                );
            }
            CageCommand::Leave { pilot } => {
                if let Some(member) = self.members.remove(&pilot) {
                    self.by_ssrc.remove(&member.ssrc);
                }
            }
            CageCommand::Datagram { from, bytes } => self.forward(from, &bytes),
        }
    }

    /// Validates a datagram and fans it out.
    ///
    /// `from` is the `ssrc` bound to the *connection*, which the server assigned
    /// at Cage entry. The `ssrc` inside the datagram is compared against it and
    /// a mismatch is refused.
    ///
    /// That comparison is gap G2 in `docs/plano-m0-m1.md`.
    /// `specs/08-seguranca.md` promises that a client "forging another's
    /// identity" is handled because "`ssrc` is assigned by the server, never
    /// accepted from the client" — but `specs/02-protocolo.md` also says the
    /// server "forwards intact", and nothing anywhere stated that the two must
    /// be checked against each other. Without this line a pilot could put
    /// somebody else's `ssrc` in their own datagrams and every listener would
    /// attribute the audio to the wrong person.
    fn forward(&mut self, from: Ssrc, bytes: &[u8]) {
        let Some(pilot) = self.by_ssrc.get(&from).copied() else {
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

        let Some(member) = self.members.get_mut(&pilot) else {
            self.drops.not_a_member += 1;
            return;
        };

        if !member.may_speak {
            self.drops.not_permitted += 1;
            return;
        }

        // specs/04-servidor-seele.md: a per-sender frames-per-second limit, so a
        // malicious client cannot saturate the Cage.
        let now = Instant::now();
        if now.duration_since(member.window_started) >= Duration::from_secs(1) {
            member.window_started = now;
            member.window_frames = 0;
        }
        member.window_frames += 1;
        if member.window_frames > MAX_FRAMES_PER_SECOND {
            self.drops.rate_limited += 1;
            return;
        }

        // Forward to everybody else. The payload is never touched: specs/04 says
        // the server never decodes Opus, which is what keeps its CPU flat and
        // leaves the door open for E2EE.
        let mut lagging = 0_u64;
        let mut delivered = 0_u64;
        for (other, subscriber) in &self.members {
            if *other == pilot {
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

/// Spawns a Cage on its own task and returns the handle to talk to it.
///
/// `specs/04-servidor-seele.md`: one task per Cage, owning its state, reached by
/// `mpsc`. Nothing shares a lock.
#[must_use]
pub fn spawn(id: CageId) -> mpsc::Sender<CageCommand> {
    let (tx, mut rx) = mpsc::channel(CHANNEL_DEPTH);
    tokio::spawn(async move {
        let mut cage = Cage::new(id);
        while let Some(command) = rx.recv().await {
            cage.handle(command);
        }
        tracing::info!(cage = %id, forwarded = cage.forwarded(), "cage closed");
    });
    tx
}

#[cfg(test)]
mod tests {
    use super::*;
    use seele_proto::MediaHeader;

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

    fn member(cage: &mut Cage, pilot: u64, ssrc: u32, may_speak: bool) -> mpsc::Receiver<Vec<u8>> {
        let (tx, rx) = mpsc::channel(64);
        cage.handle(CageCommand::Join {
            pilot: PilotId(pilot),
            ssrc: Ssrc(ssrc),
            may_speak,
            outbound: tx,
        });
        rx
    }

    #[test]
    fn a_datagram_reaches_everybody_but_its_sender() {
        let mut cage = Cage::new(CageId(1));
        let mut alice = member(&mut cage, 1, 100, true);
        let mut bob = member(&mut cage, 2, 200, true);
        let mut carol = member(&mut cage, 3, 300, true);

        cage.handle(CageCommand::Datagram {
            from: Ssrc(100),
            bytes: datagram(100, 1),
        });

        assert!(bob.try_recv().is_ok(), "bob should hear alice");
        assert!(carol.try_recv().is_ok(), "carol should hear alice");
        assert!(alice.try_recv().is_err(), "alice must not hear herself");
        assert_eq!(cage.forwarded(), 2);
    }

    #[test]
    fn the_payload_is_forwarded_byte_for_byte() {
        // specs/04-servidor-seele.md: "never decodes the Opus". Rewriting even
        // one byte would break the E2EE path specs/08 sketches, where the server
        // can read the header and nothing else.
        let mut cage = Cage::new(CageId(1));
        let _alice = member(&mut cage, 1, 100, true);
        let mut bob = member(&mut cage, 2, 200, true);

        let original = datagram(100, 7);
        cage.handle(CageCommand::Datagram {
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
        let mut cage = Cage::new(CageId(1));
        let _alice = member(&mut cage, 1, 100, true);
        let _bob = member(&mut cage, 2, 200, true);
        let mut carol = member(&mut cage, 3, 300, true);

        cage.handle(CageCommand::Datagram {
            from: Ssrc(200),         // the connection is Bob's
            bytes: datagram(100, 1), // the header claims Alice
        });

        assert!(carol.try_recv().is_err(), "a forged datagram was forwarded");
        assert_eq!(cage.drops().forged_ssrc, 1);
        assert_eq!(cage.forwarded(), 0);
    }

    #[test]
    fn a_pilot_without_permission_cannot_speak() {
        // specs/04-servidor-seele.md: "always validate — do not trust the
        // client". specs/07 calls the role that cannot speak an Observador.
        let mut cage = Cage::new(CageId(1));
        let _observer = member(&mut cage, 1, 100, false);
        let mut pilot = member(&mut cage, 2, 200, true);

        cage.handle(CageCommand::Datagram {
            from: Ssrc(100),
            bytes: datagram(100, 1),
        });

        assert!(pilot.try_recv().is_err(), "an observer was forwarded");
        assert_eq!(cage.drops().not_permitted, 1);
    }

    #[test]
    fn a_stranger_is_refused() {
        let mut cage = Cage::new(CageId(1));
        let mut alice = member(&mut cage, 1, 100, true);

        cage.handle(CageCommand::Datagram {
            from: Ssrc(999),
            bytes: datagram(999, 1),
        });

        assert!(alice.try_recv().is_err());
        assert_eq!(cage.drops().not_a_member, 1);
    }

    #[test]
    fn a_malformed_datagram_is_counted_not_forwarded() {
        let mut cage = Cage::new(CageId(1));
        let _alice = member(&mut cage, 1, 100, true);
        let mut bob = member(&mut cage, 2, 200, true);

        cage.handle(CageCommand::Datagram {
            from: Ssrc(100),
            bytes: vec![0xFF; 3],
        });

        assert!(bob.try_recv().is_err());
        assert_eq!(cage.drops().malformed, 1);
    }

    #[test]
    fn a_flood_is_cut_off_at_the_documented_rate() {
        // specs/04-servidor-seele.md: an honest client sends 50/s; above the
        // limit, discard and log.
        let mut cage = Cage::new(CageId(1));
        let _alice = member(&mut cage, 1, 100, true);
        let mut bob = member(&mut cage, 2, 200, true);

        for seq in 0..(MAX_FRAMES_PER_SECOND * 3) {
            cage.handle(CageCommand::Datagram {
                from: Ssrc(100),
                bytes: datagram(100, seq as u16),
            });
        }

        let received = std::iter::from_fn(|| bob.try_recv().ok()).count();
        assert_eq!(
            received, MAX_FRAMES_PER_SECOND as usize,
            "the rate limit did not hold"
        );
        assert!(cage.drops().rate_limited > 0);
    }

    #[test]
    fn an_honest_sender_is_never_rate_limited() {
        // The other half: a limit that cuts off legitimate speech is worse than
        // no limit at all.
        let mut cage = Cage::new(CageId(1));
        let _alice = member(&mut cage, 1, 100, true);
        let mut bob = member(&mut cage, 2, 200, true);

        for seq in 0..seele_proto::transport::NOMINAL_FRAMES_PER_SECOND {
            cage.handle(CageCommand::Datagram {
                from: Ssrc(100),
                bytes: datagram(100, seq as u16),
            });
        }

        assert_eq!(cage.drops().rate_limited, 0);
        assert_eq!(
            std::iter::from_fn(|| bob.try_recv().ok()).count(),
            seele_proto::transport::NOMINAL_FRAMES_PER_SECOND as usize
        );
    }

    #[test]
    fn leaving_stops_delivery_and_frees_the_ssrc() {
        let mut cage = Cage::new(CageId(1));
        let _alice = member(&mut cage, 1, 100, true);
        let mut bob = member(&mut cage, 2, 200, true);
        assert_eq!(cage.occupancy(), 2);

        cage.handle(CageCommand::Leave { pilot: PilotId(2) });
        assert_eq!(cage.occupancy(), 1);

        cage.handle(CageCommand::Datagram {
            from: Ssrc(100),
            bytes: datagram(100, 1),
        });
        assert!(bob.try_recv().is_err(), "a departed pilot still received");

        // The ssrc must be released, or a stale mapping outlives the session.
        cage.handle(CageCommand::Datagram {
            from: Ssrc(200),
            bytes: datagram(200, 1),
        });
        assert_eq!(cage.drops().not_a_member, 1);
    }

    #[test]
    fn a_lagging_subscriber_is_dropped_rather_than_blocking_the_cage() {
        // A slow listener must not add latency for everybody else. Old audio is
        // worth nothing anyway.
        let mut cage = Cage::new(CageId(1));
        let _alice = member(&mut cage, 1, 100, true);
        let (tx, _rx) = mpsc::channel(1);
        cage.handle(CageCommand::Join {
            pilot: PilotId(2),
            ssrc: Ssrc(200),
            may_speak: true,
            outbound: tx,
        });

        for seq in 0..10 {
            cage.handle(CageCommand::Datagram {
                from: Ssrc(100),
                bytes: datagram(100, seq),
            });
        }

        assert!(cage.drops().subscriber_lagging > 0);
    }
}
