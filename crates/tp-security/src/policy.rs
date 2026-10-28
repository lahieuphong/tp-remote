//! Default-deny authorization policy.
//!
//! [`authorize`] allows a capability only if every condition below holds, checked in this
//! order (the first failure is reported):
//!
//! 1. A current session exists, and the request names it exactly (id and generation).
//! 2. The session has not ended, is `Connected`, and its link is up.
//! 3. The session is a Host session (only a Host exposes capabilities).
//! 4. An authenticated peer exists for exactly this session.
//! 5. A consent grant exists for exactly this session and that peer.
//! 6. The grant is not revoked, and neither it nor the session TTL has expired.
//! 7. Both the grant and the session's granted scope include the capability.
//! 8. The platform advertises the capability as `Available`.
//!
//! The function is pure: no I/O, no clock, no global state.

use tp_contracts::capability::{Availability, Capability, PlatformCapabilities};
use tp_contracts::error::{CoreError, ErrorCategory};
use tp_contracts::id::SessionRef;
use tp_contracts::session::{LinkState, Role, SessionSnapshot, SessionState};
use tp_contracts::time::MonoTime;

use crate::authn::AuthenticatedPeer;
use crate::consent::ConsentGrant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DenyReason {
    NoSession,
    WrongSession,
    StaleGeneration,
    SessionEnded,
    SessionNotActive,
    LinkDegraded,
    RoleNotPermitted,
    NotAuthenticated,
    PeerMismatch,
    ConsentRequired,
    ConsentDenied,
    ConsentRevoked,
    ConsentExpired,
    SessionExpired,
    ScopeNotGranted,
    CapabilityUnsupported,
}

impl DenyReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            DenyReason::NoSession => "noSession",
            DenyReason::WrongSession => "wrongSession",
            DenyReason::StaleGeneration => "staleGeneration",
            DenyReason::SessionEnded => "sessionEnded",
            DenyReason::SessionNotActive => "sessionNotActive",
            DenyReason::LinkDegraded => "linkDegraded",
            DenyReason::RoleNotPermitted => "roleNotPermitted",
            DenyReason::NotAuthenticated => "notAuthenticated",
            DenyReason::PeerMismatch => "peerMismatch",
            DenyReason::ConsentRequired => "consentRequired",
            DenyReason::ConsentDenied => "consentDenied",
            DenyReason::ConsentRevoked => "consentRevoked",
            DenyReason::ConsentExpired => "consentExpired",
            DenyReason::SessionExpired => "sessionExpired",
            DenyReason::ScopeNotGranted => "scopeNotGranted",
            DenyReason::CapabilityUnsupported => "capabilityUnsupported",
        }
    }

    pub const fn category(self) -> ErrorCategory {
        match self {
            DenyReason::NoSession
            | DenyReason::WrongSession
            | DenyReason::StaleGeneration
            | DenyReason::SessionEnded
            | DenyReason::SessionNotActive => ErrorCategory::InvalidState,
            DenyReason::LinkDegraded => ErrorCategory::TransportUnavailable,
            DenyReason::RoleNotPermitted
            | DenyReason::NotAuthenticated
            | DenyReason::PeerMismatch
            | DenyReason::ScopeNotGranted => ErrorCategory::PermissionDenied,
            DenyReason::ConsentRequired | DenyReason::ConsentDenied => {
                ErrorCategory::ConsentRequired
            }
            DenyReason::ConsentRevoked => ErrorCategory::SessionRevoked,
            DenyReason::ConsentExpired | DenyReason::SessionExpired => {
                ErrorCategory::AuthorizationExpired
            }
            DenyReason::CapabilityUnsupported => ErrorCategory::UnsupportedCapability,
        }
    }
}

impl From<DenyReason> for CoreError {
    fn from(reason: DenyReason) -> CoreError {
        CoreError::new(reason.category(), "authorization denied")
    }
}

/// Everything the policy may look at. Missing parts mean "not established".
#[derive(Debug, Clone, Copy)]
pub struct AuthorizationContext<'a> {
    pub session: Option<&'a SessionSnapshot>,
    pub peer: Option<&'a AuthenticatedPeer>,
    pub grant: Option<&'a ConsentGrant>,
    pub platform: PlatformCapabilities,
}

/// Evidence of one positive decision. Only [`authorize`] can create it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permit {
    session: SessionRef,
    capability: Capability,
    valid_until: MonoTime,
}

impl Permit {
    pub fn session(&self) -> SessionRef {
        self.session
    }

    pub fn capability(&self) -> Capability {
        self.capability
    }

    pub fn valid_until(&self) -> MonoTime {
        self.valid_until
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow(Permit),
    Deny(DenyReason),
}

impl Decision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Decision::Allow(_))
    }

    pub fn deny_reason(&self) -> Option<DenyReason> {
        match self {
            Decision::Allow(_) => None,
            Decision::Deny(reason) => Some(*reason),
        }
    }
}

/// Default-deny evaluation of one capability request for `session` at `now`.
pub fn authorize(
    ctx: &AuthorizationContext<'_>,
    session: SessionRef,
    capability: Capability,
    now: MonoTime,
) -> Decision {
    match evaluate(ctx, session, capability, now) {
        Ok(permit) => Decision::Allow(permit),
        Err(reason) => Decision::Deny(reason),
    }
}

fn evaluate(
    ctx: &AuthorizationContext<'_>,
    session: SessionRef,
    capability: Capability,
    now: MonoTime,
) -> Result<Permit, DenyReason> {
    let snapshot = ctx.session.ok_or(DenyReason::NoSession)?;
    if snapshot.session.id != session.id {
        return Err(DenyReason::WrongSession);
    }
    if snapshot.session.generation != session.generation {
        return Err(DenyReason::StaleGeneration);
    }
    if snapshot.state.is_terminal() {
        return Err(DenyReason::SessionEnded);
    }
    if snapshot.state != SessionState::Connected {
        return Err(DenyReason::SessionNotActive);
    }
    if snapshot.link != Some(LinkState::Up) {
        return Err(DenyReason::LinkDegraded);
    }
    if snapshot.role != Role::Host {
        return Err(DenyReason::RoleNotPermitted);
    }

    let peer = ctx.peer.ok_or(DenyReason::NotAuthenticated)?;
    if peer.session() != session {
        return Err(DenyReason::NotAuthenticated);
    }

    let grant = ctx.grant.ok_or(DenyReason::ConsentRequired)?;
    if grant.session() != session {
        return Err(DenyReason::ConsentRequired);
    }
    if grant.peer() != peer.peer() {
        return Err(DenyReason::PeerMismatch);
    }
    if grant.is_revoked() {
        return Err(DenyReason::ConsentRevoked);
    }
    if now >= grant.expires_at() {
        return Err(DenyReason::ConsentExpired);
    }
    let session_expiry = snapshot.expires_at.ok_or(DenyReason::SessionExpired)?;
    if now >= session_expiry {
        return Err(DenyReason::SessionExpired);
    }

    let granted_by_session = snapshot
        .granted_scope
        .is_some_and(|s| s.includes(capability));
    if !grant.scope().includes(capability) || !granted_by_session {
        return Err(DenyReason::ScopeNotGranted);
    }
    if ctx.platform.availability(capability) != Availability::Available {
        return Err(DenyReason::CapabilityUnsupported);
    }

    Ok(Permit {
        session,
        capability,
        valid_until: grant.expires_at().min(session_expiry),
    })
}
