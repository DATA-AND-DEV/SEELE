//! Identifiers, as distinct types.
//!
//! Every one of these is a number on the wire, and every one of them would
//! compile fine in the wrong slot if they were all `u32`. Passing a `LineId`
//! where a `CageId` belongs is a bug the compiler can catch for free, and
//! `specs/04-servidor-seele.md` makes Cages and Linhas independent so the mix-up
//! is available to make.
//!
//! Names follow `docs/glossario.md`, which is normative in both languages
//! (`specs/10-convencoes.md`).

use serde::{Deserialize, Serialize};

/// Builds a transparent newtype over an integer.
macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident, $inner:ty) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub $inner);

        impl $name {
            /// The underlying value.
            #[must_use]
            pub const fn get(self) -> $inner {
                self.0
            }
        }

        impl From<$inner> for $name {
            fn from(value: $inner) -> Self {
                Self(value)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "{}", self.0)
            }
        }
    };
}

id_type!(
    /// A voice channel. `Cage` in both languages (`docs/glossario.md`).
    CageId,
    u32
);

id_type!(
    /// A text channel. `Linha` in Portuguese, `Line` in English.
    LineId,
    u32
);

id_type!(
    /// A user account. `Piloto` in Portuguese, `Pilot` in English.
    PilotId,
    u64
);

id_type!(
    /// One connection's authenticated session.
    SessionId,
    u64
);

id_type!(
    /// A media source inside a Cage.
    ///
    /// `specs/08-seguranca.md`: assigned by the server, **never accepted from
    /// the client**. The server binds it to the connection and refuses any
    /// datagram whose `ssrc` does not match, which is what stops one pilot
    /// impersonating another.
    Ssrc,
    u32
);

id_type!(
    /// One screen transmission inside a Cage.
    ///
    /// **Not an [`Ssrc`], and that is the whole reason it exists.** §3.6 of
    /// `docs/superpowers/specs/2026-08-22-compartilhamento-de-tela-design.md`:
    /// the `ssrc` is the audio source assigned on Cage entry, and every client
    /// builds a table of `ssrc` → person out of it. Sharing a screen is not
    /// becoming a second talker, so it gets an identifier of its own and nobody
    /// has to rewrite that table to make room for something that never speaks.
    ///
    /// Assigned by the Dogma when the transmission is announced, never taken
    /// from the sender — the rule `specs/08-seguranca.md` already applies to
    /// [`Ssrc`], for the same reason: an identifier a client chooses is an
    /// identifier a client can choose somebody else's.
    ScreenId,
    u32
);

id_type!(
    /// A text message.
    MessageId,
    u64
);

id_type!(
    /// A client-chosen identifier for a message it is sending.
    ///
    /// `specs/02-protocolo.md` requires `EnviarMensagem` to be "idempotent by
    /// `client_msg_id`", but leaves the field out of the payload column. It is
    /// carried explicitly here — see gap G9 in `docs/plano-m0-m1.md`. Without it
    /// a retry after a lost acknowledgement posts the message twice.
    ClientMessageId,
    u64
);

id_type!(
    /// A permission role.
    RoleId,
    u32
);

id_type!(
    /// A file hanging off a message.
    ///
    /// The row, and not the bytes. ADR 0027 keeps the two apart on purpose:
    /// expiring an attachment deletes the bytes and **keeps this row**, so the
    /// message can still say that a file was here and what it was called. Two
    /// pilots who sent the same file have two of these and one blob.
    AttachmentId,
    u64
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_do_not_interchange() {
        // The whole point of the newtypes. This is a compile-time property, so
        // the test documents it rather than proving it: `takes_cage(LineId(1))`
        // does not compile, and that is the guarantee.
        fn takes_cage(id: CageId) -> u32 {
            id.get()
        }
        assert_eq!(takes_cage(CageId(7)), 7);
        assert_eq!(LineId::from(7).get(), 7);
    }

    #[test]
    fn identifiers_are_transparent_on_the_wire() {
        // `serde(transparent)` keeps a newtype the same bytes as its integer, so
        // wrapping costs nothing in a header budgeted at 11 bytes.
        let wrapped = postcard::to_allocvec(&CageId(300)).unwrap();
        let bare = postcard::to_allocvec(&300_u32).unwrap();
        assert_eq!(wrapped, bare);
    }
}
