//! Peer authentication results.

use tp_contracts::id::{DeviceId, SessionRef};

/// Proof that the peer of one session authenticated successfully.
///
/// FUTURE: produced only by the NET-03 handshake (PAKE with key confirmation plus an
/// identity signature over the transcript). Authentication answers "who is connected";
/// it never implies consent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedPeer {
    session: SessionRef,
    peer: DeviceId,
}

impl AuthenticatedPeer {
    pub fn session(&self) -> SessionRef {
        self.session
    }

    pub fn peer(&self) -> DeviceId {
        self.peer
    }

    /// Test-only constructor. Not available to the desktop app.
    #[cfg(any(test, feature = "test-support"))]
    pub fn for_tests(session: SessionRef, peer: DeviceId) -> Self {
        AuthenticatedPeer { session, peer }
    }
}
