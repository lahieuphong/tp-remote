//! SessionService: the state machine and the authorization policy together, behind the
//! SessionManager contract. These tests prove engine logic only; no real transport,
//! prompt, OS adapter or IPC path exists yet.

use std::sync::Arc;
use std::time::Duration;

use tp_contracts::capability::{Availability, Capability, PlatformCapabilities, Scope};
use tp_contracts::error::ErrorCategory;
use tp_contracts::id::{DeviceId, SessionRef};
use tp_contracts::session::{FaultKind, Role, SessionManager, SessionState, StopKind};
use tp_contracts::time::MonoTime;
use tp_core::service::{LifecycleEvent, SessionService};
use tp_diagnostics::{Logger, MemorySink};
use tp_security::{AuthenticatedPeer, ConsentDecision, DenyReason, LocalConsent};
use tp_session::{SequentialIds, Timeouts};

const T0: MonoTime = MonoTime::from_secs(1_000);

fn at(secs: u64) -> MonoTime {
    T0.saturating_add(Duration::from_secs(secs))
}

/// Only for exercising the policy: the production platform is `NONE`.
const ALL_AVAILABLE: PlatformCapabilities = PlatformCapabilities {
    view_screen: Availability::Available,
    control_input: Availability::Available,
};

fn device(n: u8) -> DeviceId {
    let mut bytes = [0u8; 16];
    bytes[0] = n;
    DeviceId::from_bytes(bytes).expect("device")
}

fn service(platform: PlatformCapabilities) -> (SessionService<SequentialIds>, Arc<MemorySink>) {
    let sink = Arc::new(MemorySink::new());
    let svc = SessionService::new(
        SequentialIds::default(),
        Timeouts::default(),
        platform,
        Logger::new(sink.clone()),
    );
    (svc, sink)
}

fn allow(session: SessionRef, scope: Scope) -> LocalConsent {
    LocalConsent::for_tests(session, ConsentDecision::Allow(scope), T0)
}

/// Host session: authenticated, consented with `scope`, transport ready.
fn connected(svc: &mut SessionService<SequentialIds>, scope: Scope) -> SessionRef {
    let s = svc
        .begin(Role::Host, Scope::ViewAndControl, T0)
        .expect("begin");
    svc.record_authenticated_peer(AuthenticatedPeer::for_tests(s, device(7)))
        .expect("peer");
    svc.handle(s, LifecycleEvent::Prepared, T0)
        .expect("prepared");
    svc.respond_consent(s, allow(s, scope), T0)
        .expect("consent");
    svc.handle(s, LifecycleEvent::TransportReady, T0)
        .expect("ready");
    s
}

fn reason(decision: tp_security::Decision) -> DenyReason {
    decision.deny_reason().expect("expected a denial")
}

#[test]
fn a_fully_consented_session_is_authorized_only_where_the_platform_provides() {
    let (mut svc, _) = service(ALL_AVAILABLE);
    let s = connected(&mut svc, Scope::ViewAndControl);
    assert!(svc.authorize(s, Capability::ViewScreen, at(1)).is_allowed());
    assert!(
        svc.authorize(s, Capability::ControlInput, at(1))
            .is_allowed()
    );

    // The real build advertises nothing, so even a fully consented session gets nothing.
    let (mut real, _) = service(tp_core::platform_capabilities());
    let s = connected(&mut real, Scope::ViewAndControl);
    for capability in Capability::ALL {
        assert_eq!(
            reason(real.authorize(s, capability, at(1))),
            DenyReason::CapabilityUnsupported
        );
    }
}

// T-SEC-CORE-01 / T-SEC-CORE-02 across the whole flow: nothing is authorized before the
// local consent, whatever the lifecycle state.
#[test]
fn nothing_is_authorized_before_local_consent() {
    let (mut svc, _) = service(ALL_AVAILABLE);
    let s = svc
        .begin(Role::Host, Scope::ViewAndControl, T0)
        .expect("begin");
    assert_eq!(
        reason(svc.authorize(s, Capability::ViewScreen, T0)),
        DenyReason::SessionNotActive
    );
    svc.record_authenticated_peer(AuthenticatedPeer::for_tests(s, device(7)))
        .expect("peer");
    svc.handle(s, LifecycleEvent::Prepared, T0)
        .expect("prepared");
    assert_eq!(
        reason(svc.authorize(s, Capability::ControlInput, T0)),
        DenyReason::SessionNotActive
    );
    // A transport that comes up without consent is rejected by the machine.
    assert!(svc.handle(s, LifecycleEvent::TransportReady, T0).is_err());
    assert_eq!(
        svc.current().expect("snapshot").state,
        SessionState::AwaitingConsent
    );
}

#[test]
fn preparation_requires_an_authenticated_peer() {
    let (mut svc, _) = service(ALL_AVAILABLE);
    let s = svc
        .begin(Role::Host, Scope::ViewAndControl, T0)
        .expect("begin");
    let error = svc
        .handle(s, LifecycleEvent::Prepared, T0)
        .expect_err("unauthenticated");
    assert_eq!(error.category(), ErrorCategory::PermissionDenied);
    assert_eq!(
        svc.current().expect("snapshot").state,
        SessionState::Preparing
    );
}

// §6.4: a consent decision received from the network on the Host is a protocol
// violation; the session fails instead of connecting.
#[test]
fn remote_consent_on_the_host_is_rejected_and_ends_the_session() {
    let (mut svc, _) = service(ALL_AVAILABLE);
    let s = svc
        .begin(Role::Host, Scope::ViewAndControl, T0)
        .expect("begin");
    svc.record_authenticated_peer(AuthenticatedPeer::for_tests(s, device(7)))
        .expect("peer");
    svc.handle(s, LifecycleEvent::Prepared, T0)
        .expect("prepared");
    let error = svc
        .handle(
            s,
            LifecycleEvent::RemoteConsent(ConsentDecision::Allow(Scope::ViewAndControl)),
            T0,
        )
        .expect_err("self-approval");
    assert_eq!(error.category(), ErrorCategory::PermissionDenied);
    let snapshot = svc.current().expect("snapshot");
    assert_eq!(snapshot.state, SessionState::Disconnecting);
    assert_eq!(
        snapshot.end_reason,
        Some(tp_contracts::session::EndReason::Fault(
            FaultKind::ProtocolViolation
        ))
    );
    assert_eq!(snapshot.granted_scope, None);
}

#[test]
fn consent_proofs_are_bound_to_their_session() {
    let (mut svc, _) = service(ALL_AVAILABLE);
    let old = connected(&mut svc, Scope::ViewOnly);
    svc.disconnect(old, StopKind::Disconnect, T0).expect("stop");
    svc.handle(old, LifecycleEvent::TeardownComplete, T0)
        .expect("done");

    let new = svc
        .begin(Role::Host, Scope::ViewAndControl, at(1))
        .expect("begin");
    svc.record_authenticated_peer(AuthenticatedPeer::for_tests(new, device(7)))
        .expect("peer");
    svc.handle(new, LifecycleEvent::Prepared, at(1))
        .expect("prepared");
    // A consent taken for the previous session cannot approve the new one.
    let error = svc
        .respond_consent(new, allow(old, Scope::ViewAndControl), at(1))
        .expect_err("old proof");
    assert_eq!(error.category(), ErrorCategory::InvalidState);
    let error = svc
        .respond_consent(old, allow(old, Scope::ViewAndControl), at(1))
        .expect_err("stale");
    assert_eq!(error.category(), ErrorCategory::InvalidState);
    assert_eq!(
        svc.current().expect("snapshot").state,
        SessionState::AwaitingConsent
    );
}

// T-SEC-CORE-04 at service level: revocation is immediate and one-way.
#[test]
fn revocation_takes_effect_before_the_next_authorization() {
    let (mut svc, _) = service(ALL_AVAILABLE);
    let s = connected(&mut svc, Scope::ViewAndControl);
    assert!(
        svc.authorize(s, Capability::ControlInput, at(1))
            .is_allowed()
    );
    svc.disconnect(s, StopKind::Revoke, at(2)).expect("revoke");
    assert_eq!(svc.grant_revoked(s), Some(true));
    assert!(
        !svc.authorize(s, Capability::ControlInput, at(2))
            .is_allowed()
    );
    assert!(!svc.authorize(s, Capability::ViewScreen, at(2)).is_allowed());
    // Duplicate stop: idempotent.
    let again = svc
        .disconnect(s, StopKind::Disconnect, at(3))
        .expect("idempotent");
    assert_eq!(again.state, SessionState::Disconnecting);
}

// T-SEC-CORE-03 at service level: the TTL ends authorization even before tick runs.
#[test]
fn authorization_ends_at_the_session_ttl() {
    let (mut svc, _) = service(ALL_AVAILABLE);
    let s = connected(&mut svc, Scope::ViewAndControl);
    let ttl = T0.saturating_add(Timeouts::default().session_ttl);
    assert!(
        svc.authorize(
            s,
            Capability::ControlInput,
            MonoTime::from_micros(ttl.as_micros() - 1)
        )
        .is_allowed()
    );
    assert_eq!(
        reason(svc.authorize(s, Capability::ControlInput, ttl)),
        DenyReason::ConsentExpired
    );
    svc.tick(ttl).expect("tick");
    assert_eq!(
        svc.current().expect("snapshot").state,
        SessionState::Disconnecting
    );
}

// T-SEC-CORE-05 / T-SEC-CORE-08 / T-CORE-08: after a new session starts, nothing from the
// old one is authorized and its stop cannot touch the new session.
#[test]
fn stale_sessions_regain_nothing() {
    let (mut svc, _) = service(ALL_AVAILABLE);
    let old = connected(&mut svc, Scope::ViewAndControl);
    svc.disconnect(old, StopKind::Disconnect, at(1))
        .expect("stop");
    svc.handle(old, LifecycleEvent::TeardownComplete, at(1))
        .expect("done");
    assert_eq!(
        reason(svc.authorize(old, Capability::ViewScreen, at(1))),
        DenyReason::SessionEnded
    );

    let new = connected(&mut svc, Scope::ViewAndControl);
    assert_ne!(new, old);
    assert_eq!(
        reason(svc.authorize(old, Capability::ViewScreen, at(2))),
        DenyReason::WrongSession
    );
    assert!(svc.disconnect(old, StopKind::Disconnect, at(2)).is_err());
    assert!(
        svc.handle(old, LifecycleEvent::TransportReady, at(2))
            .is_err()
    );
    assert!(
        svc.authorize(new, Capability::ControlInput, at(2))
            .is_allowed()
    );
}

#[test]
fn a_controller_never_decides_consent_and_is_never_authorized() {
    let (mut svc, _) = service(ALL_AVAILABLE);
    let s = svc
        .begin(Role::Controller, Scope::ViewOnly, T0)
        .expect("begin");
    svc.record_authenticated_peer(AuthenticatedPeer::for_tests(s, device(3)))
        .expect("peer");
    svc.handle(s, LifecycleEvent::Prepared, T0)
        .expect("prepared");
    let error = svc
        .respond_consent(s, allow(s, Scope::ViewOnly), T0)
        .expect_err("controller");
    assert_eq!(error.category(), ErrorCategory::PermissionDenied);
    svc.handle(
        s,
        LifecycleEvent::RemoteConsent(ConsentDecision::Allow(Scope::ViewOnly)),
        T0,
    )
    .expect("host decided");
    svc.handle(s, LifecycleEvent::TransportReady, T0)
        .expect("ready");
    assert_eq!(
        reason(svc.authorize(s, Capability::ViewScreen, T0)),
        DenyReason::RoleNotPermitted
    );
}

#[test]
fn a_second_session_cannot_start_while_one_is_active() {
    let (mut svc, _) = service(ALL_AVAILABLE);
    let _s = svc.begin(Role::Host, Scope::ViewOnly, T0).expect("begin");
    let error = svc
        .begin(Role::Host, Scope::ViewOnly, T0)
        .expect_err("busy");
    assert_eq!(error.category(), ErrorCategory::InvalidState);
}

#[test]
fn a_local_denial_grants_nothing() {
    let (mut svc, _) = service(ALL_AVAILABLE);
    let s = svc
        .begin(Role::Host, Scope::ViewAndControl, T0)
        .expect("begin");
    svc.record_authenticated_peer(AuthenticatedPeer::for_tests(s, device(7)))
        .expect("peer");
    svc.handle(s, LifecycleEvent::Prepared, T0)
        .expect("prepared");
    let snapshot = svc
        .respond_consent(s, LocalConsent::for_tests(s, ConsentDecision::Deny, T0), T0)
        .expect("deny");
    assert_eq!(snapshot.state, SessionState::Disconnecting);
    assert_eq!(svc.grant_revoked(s), None, "no grant was created");
    assert!(!svc.authorize(s, Capability::ViewScreen, T0).is_allowed());
}
