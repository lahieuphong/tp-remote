//! Local consent and the grants it produces.
//!
//! Trusted-consent boundary (THREAT-MODEL TM-12, TM-16; U-06): a [`LocalConsent`] may only
//! be created by a trusted interaction of the Host user on the Host, such as a native
//! dialog handled by the shell, which neither the WebView, a remote peer nor injected
//! input can drive. That surface does not exist yet, so BOOT-02 has **no production
//! constructor** at all; this is enforced by the type's private fields.

use tp_contracts::capability::Scope;
use tp_contracts::id::{DeviceId, SessionRef};
use tp_contracts::time::MonoTime;

use crate::authn::AuthenticatedPeer;
use crate::policy::DenyReason;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentDecision {
    Allow(Scope),
    Deny,
}

/// A decision the Host user took locally for one session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalConsent {
    session: SessionRef,
    decision: ConsentDecision,
    decided_at: MonoTime,
}

impl LocalConsent {
    pub fn session(&self) -> SessionRef {
        self.session
    }

    pub fn decision(&self) -> ConsentDecision {
        self.decision
    }

    pub fn decided_at(&self) -> MonoTime {
        self.decided_at
    }

    /// Test-only constructor. Not available to the desktop app.
    #[cfg(any(test, feature = "test-support"))]
    pub fn for_tests(session: SessionRef, decision: ConsentDecision, decided_at: MonoTime) -> Self {
        LocalConsent {
            session,
            decision,
            decided_at,
        }
    }
}

/// Authorization derived from a local `Allow` for one authenticated peer and session.
///
/// There is no way to widen `scope`: more access requires a new `LocalConsent` and a new
/// grant. Revocation is one-way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsentGrant {
    session: SessionRef,
    peer: DeviceId,
    scope: Scope,
    granted_at: MonoTime,
    expires_at: MonoTime,
    revoked: bool,
}

impl ConsentGrant {
    /// Builds a grant. Fails unless the consent allows access, both proofs refer to the
    /// same session, and the grant would not already be expired.
    pub fn from_local(
        consent: &LocalConsent,
        peer: &AuthenticatedPeer,
        expires_at: MonoTime,
    ) -> Result<ConsentGrant, DenyReason> {
        let ConsentDecision::Allow(scope) = consent.decision else {
            return Err(DenyReason::ConsentDenied);
        };
        if consent.session.id != peer.session().id {
            return Err(DenyReason::WrongSession);
        }
        if consent.session.generation != peer.session().generation {
            return Err(DenyReason::StaleGeneration);
        }
        if expires_at <= consent.decided_at {
            return Err(DenyReason::ConsentExpired);
        }
        Ok(ConsentGrant {
            session: consent.session,
            peer: peer.peer(),
            scope,
            granted_at: consent.decided_at,
            expires_at,
            revoked: false,
        })
    }

    pub fn session(&self) -> SessionRef {
        self.session
    }

    pub fn peer(&self) -> DeviceId {
        self.peer
    }

    pub fn scope(&self) -> Scope {
        self.scope
    }

    pub fn granted_at(&self) -> MonoTime {
        self.granted_at
    }

    pub fn expires_at(&self) -> MonoTime {
        self.expires_at
    }

    pub fn is_revoked(&self) -> bool {
        self.revoked
    }

    /// Revokes the grant. Takes effect on the next `authorize` call; cannot be undone.
    pub fn revoke(&mut self) {
        self.revoked = true;
    }
}
