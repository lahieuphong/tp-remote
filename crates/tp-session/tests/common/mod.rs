//! Shared helpers for tp-session tests.
#![allow(dead_code)]

use tp_contracts::capability::Scope;
use tp_contracts::id::{Generation, SessionId, SessionRef};
use tp_contracts::session::{FaultKind, PeerByeReason, Role, SessionState, StopKind};
use tp_contracts::time::MonoTime;
use tp_session::{
    ConsentOrigin, Event, SessionIdSource, SessionMachine, SessionRegistry, Timeouts,
};

pub const T0: MonoTime = MonoTime::from_secs(1_000);

/// Deterministic, never-zero identifiers (tests must not depend on randomness).
#[derive(Debug, Default)]
pub struct Ids(u64);

impl SessionIdSource for Ids {
    fn next_session_id(&mut self) -> SessionId {
        self.0 += 1;
        let mut bytes = [0u8; 16];
        bytes[8..].copy_from_slice(&self.0.to_be_bytes());
        SessionId::from_bytes(bytes).expect("non-zero id")
    }
}

pub fn registry() -> SessionRegistry<Ids> {
    SessionRegistry::new(Ids::default(), Timeouts::default())
}

pub fn sref(n: u8) -> SessionRef {
    let mut bytes = [0u8; 16];
    bytes[15] = n;
    SessionRef {
        id: SessionId::from_bytes(bytes).expect("id"),
        generation: Generation::FIRST,
    }
}

pub fn local_origin(role: Role) -> ConsentOrigin {
    match role {
        Role::Host => ConsentOrigin::LocalUser,
        Role::Controller => ConsentOrigin::AuthenticatedRemote,
    }
}

/// A machine driven to `target` (one of the non-terminal states) along the normal path.
pub fn machine_in(role: Role, scope: Scope, target: SessionState) -> SessionMachine {
    let mut m = SessionMachine::new(sref(1), role, scope, Timeouts::default());
    let path = [
        (SessionState::Preparing, Event::Start),
        (SessionState::AwaitingConsent, Event::Prepared),
        (
            SessionState::Connecting,
            Event::ConsentGranted {
                scope,
                origin: local_origin(role),
            },
        ),
        (SessionState::Connected, Event::TransportReady),
        (
            SessionState::Disconnecting,
            Event::LocalStop(StopKind::Disconnect),
        ),
        (SessionState::Disconnected, Event::TeardownComplete),
    ];
    if target == SessionState::Idle {
        return m;
    }
    for (state, event) in path {
        m.apply(event, T0).expect("normal path");
        if state == target {
            return m;
        }
    }
    panic!("{target:?} is not on the normal path");
}

/// Every event the machine knows, with every parameter value.
pub fn all_events() -> Vec<Event> {
    let mut events = vec![
        Event::Start,
        Event::Prepared,
        Event::TransportReady,
        Event::LinkDegraded,
        Event::LinkRestored,
        Event::TeardownComplete,
    ];
    for scope in [Scope::ViewOnly, Scope::ViewAndControl] {
        for origin in [ConsentOrigin::LocalUser, ConsentOrigin::AuthenticatedRemote] {
            events.push(Event::ConsentGranted { scope, origin });
        }
    }
    for origin in [ConsentOrigin::LocalUser, ConsentOrigin::AuthenticatedRemote] {
        events.push(Event::ConsentDenied { origin });
    }
    for kind in [
        StopKind::Cancel,
        StopKind::Disconnect,
        StopKind::Revoke,
        StopKind::AppExit,
    ] {
        events.push(Event::LocalStop(kind));
    }
    for reason in [
        PeerByeReason::UserDisconnect,
        PeerByeReason::Revoked,
        PeerByeReason::Denied,
        PeerByeReason::Expired,
        PeerByeReason::ConsentTimeout,
        PeerByeReason::Busy,
        PeerByeReason::ProtocolError,
    ] {
        events.push(Event::PeerBye(reason));
    }
    for fault in [
        FaultKind::AuthenticationFailed,
        FaultKind::PrepareTimeout,
        FaultKind::ConsentWaitTimeout,
        FaultKind::ConnectTimeout,
        FaultKind::TransportLost,
        FaultKind::ProtocolViolation,
        FaultKind::PermissionRevoked,
        FaultKind::Internal,
    ] {
        events.push(Event::Fault(fault));
    }
    events
}

/// Independent statement of ARCH-00 §6.3 (as clarified by ADR-003): may `event` be applied
/// to `m`? Written separately from the machine so the two can be compared.
pub fn spec_allows(m: &SessionMachine, event: Event) -> bool {
    use SessionState as S;
    let snapshot = m.snapshot();
    let active = snapshot.state.is_active();
    match (snapshot.state, event) {
        (S::Idle, Event::Start) => true,
        (S::Preparing, Event::Prepared) => true,
        (S::AwaitingConsent, Event::ConsentGranted { scope, origin }) => {
            origin == local_origin(m.role()) && snapshot.requested_scope.covers(scope)
        }
        (S::AwaitingConsent, Event::ConsentDenied { origin }) => origin == local_origin(m.role()),
        (S::Connecting, Event::TransportReady) => true,
        (S::Connected, Event::LinkDegraded) => {
            snapshot.link == Some(tp_contracts::session::LinkState::Up)
        }
        (S::Connected, Event::LinkRestored) => {
            snapshot.link == Some(tp_contracts::session::LinkState::Degraded)
        }
        (_, Event::LocalStop(_) | Event::PeerBye(_) | Event::Fault(_)) if active => true,
        (S::Disconnecting, Event::Fault(_) | Event::TeardownComplete) => true,
        _ => false,
    }
}
