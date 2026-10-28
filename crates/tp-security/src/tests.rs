//! Policy tests. They prove the policy logic only, not that a future adapter calls it.

use tp_contracts::capability::{Availability, Capability, PlatformCapabilities, Scope};
use tp_contracts::id::{DeviceId, Generation, SessionId, SessionRef};
use tp_contracts::session::{EndReason, LinkState, Outcome, Role, SessionSnapshot, SessionState};
use tp_contracts::time::MonoTime;

use crate::{
    AuthenticatedPeer, AuthorizationContext, ConsentDecision, ConsentGrant, Decision, DenyReason,
    LocalConsent, authorize,
};

const T0: MonoTime = MonoTime::from_secs(100);
const TTL_END: MonoTime = MonoTime::from_secs(3_700);

fn session(n: u8, generation: u64) -> SessionRef {
    let mut bytes = [0u8; 16];
    bytes[15] = n;
    let mut g = Generation::FIRST;
    for _ in 1..generation {
        g = g.next().expect("generation");
    }
    SessionRef {
        id: SessionId::from_bytes(bytes).expect("id"),
        generation: g,
    }
}

fn device(n: u8) -> DeviceId {
    let mut bytes = [0u8; 16];
    bytes[0] = n;
    DeviceId::from_bytes(bytes).expect("device")
}

/// Everything available: only for testing the policy's own logic.
const ALL_AVAILABLE: PlatformCapabilities = PlatformCapabilities {
    view_screen: Availability::Available,
    control_input: Availability::Available,
};

fn connected(s: SessionRef, scope: Scope) -> SessionSnapshot {
    SessionSnapshot {
        session: s,
        role: Role::Host,
        state: SessionState::Connected,
        link: Some(LinkState::Up),
        requested_scope: Scope::ViewAndControl,
        granted_scope: Some(scope),
        expires_at: Some(TTL_END),
        end_reason: None,
        escalation: None,
        outcome: None,
    }
}

fn grant(s: SessionRef, peer: &AuthenticatedPeer, scope: Scope) -> ConsentGrant {
    let consent = LocalConsent::for_tests(s, ConsentDecision::Allow(scope), T0);
    ConsentGrant::from_local(&consent, peer, TTL_END).expect("grant")
}

struct World {
    s: SessionRef,
    snapshot: SessionSnapshot,
    peer: AuthenticatedPeer,
    grant: ConsentGrant,
}

fn world(scope: Scope) -> World {
    let s = session(1, 1);
    let peer = AuthenticatedPeer::for_tests(s, device(7));
    World {
        s,
        snapshot: connected(s, scope),
        grant: grant(s, &peer, scope),
        peer,
    }
}

fn ctx<'a>(w: &'a World) -> AuthorizationContext<'a> {
    AuthorizationContext {
        session: Some(&w.snapshot),
        peer: Some(&w.peer),
        grant: Some(&w.grant),
        platform: ALL_AVAILABLE,
    }
}

fn denied(decision: Decision) -> DenyReason {
    decision.deny_reason().expect("expected a denial")
}

#[test]
fn positive_control_allows_only_with_every_condition_met() {
    let w = world(Scope::ViewAndControl);
    let now = MonoTime::from_secs(200);
    for capability in Capability::ALL {
        let Decision::Allow(permit) = authorize(&ctx(&w), w.s, capability, now) else {
            panic!("{capability:?} should be allowed");
        };
        assert_eq!(permit.session(), w.s);
        assert_eq!(permit.capability(), capability);
        assert_eq!(permit.valid_until(), TTL_END);
    }
}

#[test]
fn an_empty_context_denies_everything() {
    let w = world(Scope::ViewAndControl);
    let empty = AuthorizationContext {
        session: None,
        peer: None,
        grant: None,
        platform: ALL_AVAILABLE,
    };
    for capability in Capability::ALL {
        assert_eq!(
            denied(authorize(&empty, w.s, capability, T0)),
            DenyReason::NoSession
        );
    }
}

// T-SEC-CORE-01
#[test]
fn no_consent_means_no_control_permission() {
    let w = world(Scope::ViewAndControl);
    let c = AuthorizationContext {
        grant: None,
        ..ctx(&w)
    };
    for capability in Capability::ALL {
        assert_eq!(
            denied(authorize(&c, w.s, capability, T0)),
            DenyReason::ConsentRequired
        );
    }
}

// T-SEC-CORE-02
#[test]
fn an_authenticated_but_unapproved_peer_is_rejected() {
    let w = world(Scope::ViewAndControl);
    // Authentication alone is not consent.
    let c = AuthorizationContext {
        grant: None,
        ..ctx(&w)
    };
    assert_eq!(
        denied(authorize(&c, w.s, Capability::ViewScreen, T0)),
        DenyReason::ConsentRequired
    );
    // A local Deny can never become a grant.
    let deny = LocalConsent::for_tests(w.s, ConsentDecision::Deny, T0);
    assert_eq!(
        ConsentGrant::from_local(&deny, &w.peer, TTL_END),
        Err(DenyReason::ConsentDenied)
    );
    // Consent without authentication is not enough either.
    let c = AuthorizationContext {
        peer: None,
        ..ctx(&w)
    };
    assert_eq!(
        denied(authorize(&c, w.s, Capability::ViewScreen, T0)),
        DenyReason::NotAuthenticated
    );
}

// T-SEC-CORE-03
#[test]
fn expired_authorization_is_rejected() {
    let w = world(Scope::ViewAndControl);
    assert_eq!(
        denied(authorize(&ctx(&w), w.s, Capability::ControlInput, TTL_END)),
        DenyReason::ConsentExpired
    );
    // Session TTL earlier than the grant.
    let mut w = world(Scope::ViewAndControl);
    w.snapshot.expires_at = Some(MonoTime::from_secs(500));
    assert_eq!(
        denied(authorize(
            &ctx(&w),
            w.s,
            Capability::ControlInput,
            MonoTime::from_secs(500)
        )),
        DenyReason::SessionExpired
    );
    // A host snapshot without a TTL is treated as expired.
    w.snapshot.expires_at = None;
    assert_eq!(
        denied(authorize(&ctx(&w), w.s, Capability::ViewScreen, T0)),
        DenyReason::SessionExpired
    );
    // A grant that would be born expired is refused.
    let consent = LocalConsent::for_tests(w.s, ConsentDecision::Allow(Scope::ViewOnly), T0);
    assert_eq!(
        ConsentGrant::from_local(&consent, &w.peer, T0),
        Err(DenyReason::ConsentExpired)
    );
}

// T-SEC-CORE-04
#[test]
fn revocation_immediately_invalidates_authorization() {
    let mut w = world(Scope::ViewAndControl);
    let now = MonoTime::from_secs(200);
    assert!(authorize(&ctx(&w), w.s, Capability::ControlInput, now).is_allowed());
    w.grant.revoke();
    for capability in Capability::ALL {
        assert_eq!(
            denied(authorize(&ctx(&w), w.s, capability, now)),
            DenyReason::ConsentRevoked
        );
    }
    assert!(w.grant.is_revoked());
}

// T-SEC-CORE-05
#[test]
fn a_wrong_session_identifier_is_rejected() {
    let w = world(Scope::ViewAndControl);
    let other_id = session(2, 1);
    let other_generation = session(1, 2);
    assert_eq!(
        denied(authorize(&ctx(&w), other_id, Capability::ViewScreen, T0)),
        DenyReason::WrongSession
    );
    assert_eq!(
        denied(authorize(
            &ctx(&w),
            other_generation,
            Capability::ViewScreen,
            T0
        )),
        DenyReason::StaleGeneration
    );
    // Proofs bound to another session do not count for this one.
    let foreign_peer = AuthenticatedPeer::for_tests(other_id, device(7));
    let c = AuthorizationContext {
        peer: Some(&foreign_peer),
        ..ctx(&w)
    };
    assert_eq!(
        denied(authorize(&c, w.s, Capability::ViewScreen, T0)),
        DenyReason::NotAuthenticated
    );
    let foreign_grant = grant(other_id, &foreign_peer, Scope::ViewAndControl);
    let c = AuthorizationContext {
        grant: Some(&foreign_grant),
        ..ctx(&w)
    };
    assert_eq!(
        denied(authorize(&c, w.s, Capability::ViewScreen, T0)),
        DenyReason::ConsentRequired
    );
    // Consent and authentication for different sessions cannot be combined.
    let consent = LocalConsent::for_tests(other_id, ConsentDecision::Allow(Scope::ViewOnly), T0);
    assert_eq!(
        ConsentGrant::from_local(&consent, &w.peer, TTL_END),
        Err(DenyReason::WrongSession)
    );
    let consent = LocalConsent::for_tests(
        other_generation,
        ConsentDecision::Allow(Scope::ViewOnly),
        T0,
    );
    assert_eq!(
        ConsentGrant::from_local(&consent, &w.peer, TTL_END),
        Err(DenyReason::StaleGeneration)
    );
}

#[test]
fn a_grant_for_a_different_peer_is_rejected() {
    let w = world(Scope::ViewAndControl);
    let impostor = AuthenticatedPeer::for_tests(w.s, device(9));
    let c = AuthorizationContext {
        peer: Some(&impostor),
        ..ctx(&w)
    };
    assert_eq!(
        denied(authorize(&c, w.s, Capability::ViewScreen, T0)),
        DenyReason::PeerMismatch
    );
}

// T-SEC-CORE-06
#[test]
fn unsupported_capabilities_are_rejected() {
    let w = world(Scope::ViewAndControl);
    let c = AuthorizationContext {
        platform: PlatformCapabilities::NONE,
        ..ctx(&w)
    };
    for capability in Capability::ALL {
        assert_eq!(
            denied(authorize(&c, w.s, capability, T0)),
            DenyReason::CapabilityUnsupported
        );
    }
    let partial = PlatformCapabilities {
        view_screen: Availability::Available,
        control_input: Availability::Unavailable(
            tp_contracts::capability::UnavailableReason::PermissionDenied,
        ),
    };
    let c = AuthorizationContext {
        platform: partial,
        ..ctx(&w)
    };
    assert!(authorize(&c, w.s, Capability::ViewScreen, T0).is_allowed());
    assert_eq!(
        denied(authorize(&c, w.s, Capability::ControlInput, T0)),
        DenyReason::CapabilityUnsupported
    );
    // Capabilities that do not exist cannot even be named.
    for name in ["fileTransfer", "clipboard", "audio", "ControlInput", ""] {
        assert_eq!(Capability::from_wire(name), None, "{name}");
    }
}

// T-SEC-CORE-07
#[test]
fn there_is_no_implicit_privilege_escalation() {
    let w = world(Scope::ViewOnly);
    assert!(authorize(&ctx(&w), w.s, Capability::ViewScreen, T0).is_allowed());
    assert_eq!(
        denied(authorize(&ctx(&w), w.s, Capability::ControlInput, T0)),
        DenyReason::ScopeNotGranted
    );
    // A wider grant does not help while the session itself only granted view.
    let mut w2 = world(Scope::ViewOnly);
    w2.grant = grant(w2.s, &w2.peer, Scope::ViewAndControl);
    assert_eq!(
        denied(authorize(&ctx(&w2), w2.s, Capability::ControlInput, T0)),
        DenyReason::ScopeNotGranted
    );
    // Only a new local decision for the wider scope (and a session granted that scope)
    // allows control.
    let mut w3 = world(Scope::ViewAndControl);
    w3.grant = grant(w3.s, &w3.peer, Scope::ViewOnly);
    assert_eq!(
        denied(authorize(&ctx(&w3), w3.s, Capability::ControlInput, T0)),
        DenyReason::ScopeNotGranted
    );
    w3.grant = grant(w3.s, &w3.peer, Scope::ViewAndControl);
    assert!(authorize(&ctx(&w3), w3.s, Capability::ControlInput, T0).is_allowed());
}

// T-SEC-CORE-08
#[test]
fn a_closed_session_cannot_regain_access() {
    let mut w = world(Scope::ViewAndControl);
    w.snapshot.state = SessionState::Disconnecting;
    w.snapshot.link = None;
    w.snapshot.end_reason = Some(EndReason::SessionExpired);
    w.snapshot.outcome = Some(Outcome::Normal);
    assert_eq!(
        denied(authorize(&ctx(&w), w.s, Capability::ControlInput, T0)),
        DenyReason::SessionNotActive
    );
    for terminal in [SessionState::Disconnected, SessionState::Failed] {
        w.snapshot.state = terminal;
        for capability in Capability::ALL {
            assert_eq!(
                denied(authorize(&ctx(&w), w.s, capability, T0)),
                DenyReason::SessionEnded
            );
        }
    }
}

#[test]
fn only_a_connected_host_with_a_healthy_link_can_be_authorized() {
    let base = world(Scope::ViewAndControl);
    for state in SessionState::ALL {
        for link in [None, Some(LinkState::Up), Some(LinkState::Degraded)] {
            for role in [Role::Host, Role::Controller] {
                let mut w = world(Scope::ViewAndControl);
                w.snapshot.state = state;
                w.snapshot.link = link;
                w.snapshot.role = role;
                let allowed = authorize(&ctx(&w), base.s, Capability::ViewScreen, T0).is_allowed();
                let expected = state == SessionState::Connected
                    && link == Some(LinkState::Up)
                    && role == Role::Host;
                assert_eq!(allowed, expected, "{state:?} {link:?} {role:?}");
            }
        }
    }
}

#[test]
fn deny_reasons_map_to_typed_error_categories() {
    use tp_contracts::error::{CoreError, ErrorCategory};
    let cases = [
        (DenyReason::ConsentRequired, ErrorCategory::ConsentRequired),
        (DenyReason::ConsentRevoked, ErrorCategory::SessionRevoked),
        (
            DenyReason::ConsentExpired,
            ErrorCategory::AuthorizationExpired,
        ),
        (
            DenyReason::SessionExpired,
            ErrorCategory::AuthorizationExpired,
        ),
        (
            DenyReason::CapabilityUnsupported,
            ErrorCategory::UnsupportedCapability,
        ),
        (DenyReason::StaleGeneration, ErrorCategory::InvalidState),
        (
            DenyReason::NotAuthenticated,
            ErrorCategory::PermissionDenied,
        ),
        (
            DenyReason::LinkDegraded,
            ErrorCategory::TransportUnavailable,
        ),
    ];
    for (reason, category) in cases {
        let error = CoreError::from(reason);
        assert_eq!(error.category(), category, "{reason:?}");
        assert_eq!(error.context(), "authorization denied");
    }
}
