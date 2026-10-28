//! Session lifecycle types (ARCH-00 §6) and the `SessionManager` contract (ARCH-00 §4.4,
//! amended by ADR-003).

use crate::capability::Scope;
use crate::error::CoreError;
use crate::id::SessionRef;
use crate::time::MonoTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Host,
    Controller,
}

/// The eight lifecycle states. Transitions are defined only by `tp-session`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionState {
    Idle,
    Preparing,
    AwaitingConsent,
    Connecting,
    Connected,
    Disconnecting,
    Disconnected,
    Failed,
}

impl SessionState {
    pub const ALL: [SessionState; 8] = [
        SessionState::Idle,
        SessionState::Preparing,
        SessionState::AwaitingConsent,
        SessionState::Connecting,
        SessionState::Connected,
        SessionState::Disconnecting,
        SessionState::Disconnected,
        SessionState::Failed,
    ];

    pub const fn is_terminal(self) -> bool {
        matches!(self, SessionState::Disconnected | SessionState::Failed)
    }

    /// Preparing, AwaitingConsent, Connecting or Connected.
    pub const fn is_active(self) -> bool {
        matches!(
            self,
            SessionState::Preparing
                | SessionState::AwaitingConsent
                | SessionState::Connecting
                | SessionState::Connected
        )
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            SessionState::Idle => "idle",
            SessionState::Preparing => "preparing",
            SessionState::AwaitingConsent => "awaitingConsent",
            SessionState::Connecting => "connecting",
            SessionState::Connected => "connected",
            SessionState::Disconnecting => "disconnecting",
            SessionState::Disconnected => "disconnected",
            SessionState::Failed => "failed",
        }
    }
}

/// Connection health while `Connected`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkState {
    Up,
    Degraded,
}

/// How a session ended: normally (`Disconnected`) or because of an error (`Failed`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Outcome {
    Normal,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StopKind {
    Cancel,
    Disconnect,
    Revoke,
    AppExit,
}

/// Reason carried by an authenticated `Bye` from the peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PeerByeReason {
    UserDisconnect,
    Revoked,
    Denied,
    Expired,
    ConsentTimeout,
    Busy,
    ProtocolError,
}

impl PeerByeReason {
    pub const fn outcome(self) -> Outcome {
        match self {
            PeerByeReason::ProtocolError => Outcome::Error,
            _ => Outcome::Normal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FaultKind {
    AuthenticationFailed,
    PrepareTimeout,
    ConsentWaitTimeout,
    ConnectTimeout,
    TransportLost,
    ProtocolViolation,
    PermissionRevoked,
    Internal,
}

/// Why a session left its active states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EndReason {
    LocalStop(StopKind),
    ConsentDenied,
    ConsentTimeout,
    OfferExpired,
    PeerBye(PeerByeReason),
    SessionExpired,
    Fault(FaultKind),
}

impl EndReason {
    pub const fn outcome(self) -> Outcome {
        match self {
            EndReason::Fault(_) => Outcome::Error,
            EndReason::PeerBye(reason) => reason.outcome(),
            _ => Outcome::Normal,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            EndReason::LocalStop(StopKind::Cancel) => "localCancel",
            EndReason::LocalStop(StopKind::Disconnect) => "localDisconnect",
            EndReason::LocalStop(StopKind::Revoke) => "localRevoke",
            EndReason::LocalStop(StopKind::AppExit) => "appExit",
            EndReason::ConsentDenied => "consentDenied",
            EndReason::ConsentTimeout => "consentTimeout",
            EndReason::OfferExpired => "offerExpired",
            EndReason::PeerBye(PeerByeReason::UserDisconnect) => "peerDisconnect",
            EndReason::PeerBye(PeerByeReason::Revoked) => "peerRevoked",
            EndReason::PeerBye(PeerByeReason::Denied) => "peerDenied",
            EndReason::PeerBye(PeerByeReason::Expired) => "peerExpired",
            EndReason::PeerBye(PeerByeReason::ConsentTimeout) => "peerConsentTimeout",
            EndReason::PeerBye(PeerByeReason::Busy) => "peerBusy",
            EndReason::PeerBye(PeerByeReason::ProtocolError) => "peerProtocolError",
            EndReason::SessionExpired => "sessionExpired",
            EndReason::Fault(FaultKind::AuthenticationFailed) => "faultAuthentication",
            EndReason::Fault(FaultKind::PrepareTimeout) => "faultPrepareTimeout",
            EndReason::Fault(FaultKind::ConsentWaitTimeout) => "faultConsentWaitTimeout",
            EndReason::Fault(FaultKind::ConnectTimeout) => "faultConnectTimeout",
            EndReason::Fault(FaultKind::TransportLost) => "faultTransportLost",
            EndReason::Fault(FaultKind::ProtocolViolation) => "faultProtocolViolation",
            EndReason::Fault(FaultKind::PermissionRevoked) => "faultPermissionRevoked",
            EndReason::Fault(FaultKind::Internal) => "faultInternal",
        }
    }
}

/// Something that turned a normal teardown into a failure (ADR-003 §2, C-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Escalation {
    FaultDuringTeardown(FaultKind),
    TeardownTimeout,
}

/// Read-only view of one session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionSnapshot {
    pub session: SessionRef,
    pub role: Role,
    pub state: SessionState,
    /// `Some` only while `Connected`.
    pub link: Option<LinkState>,
    pub requested_scope: Scope,
    /// Set once consent was granted; cleared again on teardown.
    pub granted_scope: Option<Scope>,
    /// Host only: end of the session TTL, set when consent is granted.
    pub expires_at: Option<MonoTime>,
    pub end_reason: Option<EndReason>,
    pub escalation: Option<Escalation>,
    /// `Some` once the session entered `Disconnecting`.
    pub outcome: Option<Outcome>,
}

/// Lifecycle control of sessions, independent of transports and UI.
///
/// The contract is synchronous: implementations change state and report what must happen
/// next; I/O (network, capture, prompts) is performed by whoever drives the manager. All
/// calls are serialized by the owner (one mutable borrow), so events cannot interleave.
pub trait SessionManager {
    /// Proof that a consent decision came from a trusted local interaction on the Host.
    /// Implementations bind this to an unforgeable type; the contract never accepts a
    /// plain flag or a value received from the network or the WebView.
    type ConsentProof;

    /// Starts a new session (Idle → Preparing). Fails while another session is active.
    fn begin(
        &mut self,
        role: Role,
        requested: Scope,
        now: MonoTime,
    ) -> Result<SessionRef, CoreError>;

    /// Applies the Host user's consent decision to the session it was taken for.
    fn respond_consent(
        &mut self,
        session: SessionRef,
        proof: Self::ConsentProof,
        now: MonoTime,
    ) -> Result<SessionSnapshot, CoreError>;

    /// Stops the session. Idempotent: stopping a stopping or ended session succeeds
    /// without changing it. A stale `session` is rejected and never stops a newer one.
    fn disconnect(
        &mut self,
        session: SessionRef,
        kind: StopKind,
        now: MonoTime,
    ) -> Result<SessionSnapshot, CoreError>;

    /// Fires due timeouts (consent, connect, grace, TTL, teardown watchdog).
    fn tick(&mut self, now: MonoTime) -> Result<Option<SessionSnapshot>, CoreError>;

    /// The session's current snapshot, or `None` if `session` is not the current session.
    fn snapshot(&self, session: SessionRef) -> Option<SessionSnapshot>;
}
