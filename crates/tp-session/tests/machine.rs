//! The transition table of ARCH-00 §6.3 (rows 1–21), rejection rules (§6.4), timers
//! (§6.5) and ordered cleanup effects (§5.5), as clarified by ADR-003.

mod common;

use std::time::Duration;

use common::{T0, all_events, local_origin, machine_in, spec_allows, sref};
use tp_contracts::capability::Scope;
use tp_contracts::session::{
    EndReason, Escalation, FaultKind, LinkState, Outcome, PeerByeReason, Role, SessionState,
    StopKind,
};
use tp_contracts::time::MonoTime;
use tp_session::{
    Cause, ConsentOrigin, Effect, Event, RejectReason, SessionMachine, Timeouts, TimerKind,
};

const HOST_TEARDOWN: [Effect; 8] = [
    Effect::CloseInputGate,
    Effect::ReleaseAllInput,
    Effect::StopCapture,
    Effect::StopEncoder,
    Effect::SendBye,
    Effect::CloseTransport,
    Effect::CloseSignaling,
    Effect::WipeSessionSecrets,
];

const CONTROLLER_TEARDOWN: [Effect; 4] = [
    Effect::SendBye,
    Effect::CloseTransport,
    Effect::CloseSignaling,
    Effect::WipeSessionSecrets,
];

fn after(secs: u64) -> MonoTime {
    T0.saturating_add(Duration::from_secs(secs))
}

// T-UNI-01 / T-CORE-01: a fresh machine is Idle, has no deadline and allows nothing.
#[test]
fn initialization_is_idle_and_inert() {
    for role in [Role::Host, Role::Controller] {
        let m = SessionMachine::new(sref(1), role, Scope::ViewAndControl, Timeouts::default());
        let snapshot = m.snapshot();
        assert_eq!(snapshot.state, SessionState::Idle);
        assert_eq!(snapshot.granted_scope, None);
        assert_eq!(snapshot.outcome, None);
        assert_eq!(m.next_deadline(), None);
        assert!(!m.input_allowed(T0));
    }
}

// T-UNI-01 / T-CORE-02: rows 1, 2, 5, 10 and 20 on the normal path, with exact effects.
#[test]
fn normal_path_host_view_and_control() {
    let mut m = SessionMachine::new(
        sref(1),
        Role::Host,
        Scope::ViewAndControl,
        Timeouts::default(),
    );
    let steps: [(Event, SessionState, &[Effect]); 6] = [
        (
            Event::Start,
            SessionState::Preparing,
            &[Effect::OpenHostOffer],
        ),
        (
            Event::Prepared,
            SessionState::AwaitingConsent,
            &[Effect::PromptLocalConsent],
        ),
        (
            Event::ConsentGranted {
                scope: Scope::ViewAndControl,
                origin: ConsentOrigin::LocalUser,
            },
            SessionState::Connecting,
            &[Effect::StartCapture, Effect::StartTransport],
        ),
        (
            Event::TransportReady,
            SessionState::Connected,
            &[Effect::ShowSessionIndicator, Effect::OpenInputGate],
        ),
        (
            Event::LocalStop(StopKind::Disconnect),
            SessionState::Disconnecting,
            &HOST_TEARDOWN,
        ),
        (
            Event::TeardownComplete,
            SessionState::Disconnected,
            &[Effect::ReportOutcome(Outcome::Normal)],
        ),
    ];
    for (event, to, effects) in steps {
        let t = m.apply(event, T0).expect("valid transition");
        assert_eq!(t.to, to, "{event:?}");
        assert_eq!(t.effects, effects, "{event:?}");
        assert_eq!(t.cause, Cause::Event(event));
    }
    let end = m.snapshot();
    assert_eq!(
        end.end_reason,
        Some(EndReason::LocalStop(StopKind::Disconnect))
    );
    assert_eq!(end.outcome, Some(Outcome::Normal));
    assert_eq!(m.next_deadline(), None);
}

#[test]
fn normal_path_controller_and_view_only_host() {
    let mut c = SessionMachine::new(
        sref(1),
        Role::Controller,
        Scope::ViewOnly,
        Timeouts::default(),
    );
    assert_eq!(
        c.apply(Event::Start, T0).expect("start").effects,
        [Effect::ConnectToHost]
    );
    assert_eq!(
        c.apply(Event::Prepared, T0).expect("prepared").effects,
        [Effect::AwaitConsentResult]
    );
    let t = c
        .apply(
            Event::ConsentGranted {
                scope: Scope::ViewOnly,
                origin: ConsentOrigin::AuthenticatedRemote,
            },
            T0,
        )
        .expect("consent");
    assert_eq!(t.effects, [Effect::StartTransport]);
    assert_eq!(
        c.apply(Event::TransportReady, T0).expect("ready").effects,
        [Effect::ShowSessionIndicator]
    );
    assert_eq!(
        c.apply(Event::PeerBye(PeerByeReason::UserDisconnect), T0)
            .expect("bye")
            .effects,
        CONTROLLER_TEARDOWN
    );
    assert!(
        c.snapshot().expires_at.is_none(),
        "only the Host owns the TTL"
    );

    // A view-only Host never opens the input gate.
    let mut h = machine_in(Role::Host, Scope::ViewOnly, SessionState::Connecting);
    let t = h.apply(Event::TransportReady, T0).expect("ready");
    assert_eq!(t.effects, [Effect::ShowSessionIndicator]);
    assert!(!h.input_allowed(T0));
}

// Rows 3, 4, 8, 9, 11, 12, 15–18: every active state exits through Disconnecting with the
// role's ordered teardown, and the outcome follows the reason.
#[test]
fn every_active_state_exits_through_disconnecting() {
    let exits = [
        (Event::LocalStop(StopKind::Cancel), Outcome::Normal),
        (Event::LocalStop(StopKind::Revoke), Outcome::Normal),
        (
            Event::PeerBye(PeerByeReason::UserDisconnect),
            Outcome::Normal,
        ),
        (Event::PeerBye(PeerByeReason::Busy), Outcome::Normal),
        (Event::PeerBye(PeerByeReason::ProtocolError), Outcome::Error),
        (
            Event::Fault(FaultKind::AuthenticationFailed),
            Outcome::Error,
        ),
        (Event::Fault(FaultKind::Internal), Outcome::Error),
    ];
    for role in [Role::Host, Role::Controller] {
        let teardown: &[Effect] = match role {
            Role::Host => &HOST_TEARDOWN,
            Role::Controller => &CONTROLLER_TEARDOWN,
        };
        for state in [
            SessionState::Preparing,
            SessionState::AwaitingConsent,
            SessionState::Connecting,
            SessionState::Connected,
        ] {
            for (event, outcome) in exits {
                let mut m = machine_in(role, Scope::ViewAndControl, state);
                let t = m.apply(event, T0).expect("exit allowed");
                assert_eq!(
                    t.to,
                    SessionState::Disconnecting,
                    "{role:?} {state:?} {event:?}"
                );
                assert_eq!(t.effects, teardown);
                assert_eq!(m.snapshot().outcome, Some(outcome));
                assert_eq!(
                    m.snapshot().granted_scope,
                    None,
                    "teardown clears the grant"
                );
                let end = m.apply(Event::TeardownComplete, T0).expect("teardown done");
                let terminal = match outcome {
                    Outcome::Normal => SessionState::Disconnected,
                    Outcome::Error => SessionState::Failed,
                };
                assert_eq!(end.to, terminal);
                assert_eq!(end.effects, [Effect::ReportOutcome(outcome)]);
            }
        }
    }
}

// T-CORE-04 / row 6: denial ends normally after cleanup.
#[test]
fn consent_denied_ends_normally_after_cleanup() {
    let mut m = machine_in(
        Role::Host,
        Scope::ViewAndControl,
        SessionState::AwaitingConsent,
    );
    let t = m
        .apply(
            Event::ConsentDenied {
                origin: ConsentOrigin::LocalUser,
            },
            T0,
        )
        .expect("deny");
    assert_eq!(t.to, SessionState::Disconnecting);
    assert_eq!(t.effects, HOST_TEARDOWN);
    assert!(
        !t.effects.contains(&Effect::StartCapture),
        "nothing may start after a denial"
    );
    m.apply(Event::TeardownComplete, T0).expect("done");
    let s = m.snapshot();
    assert_eq!(s.state, SessionState::Disconnected);
    assert_eq!(s.end_reason, Some(EndReason::ConsentDenied));
}

// T-UNI-05 / §6.4: consent from the wrong origin is rejected and changes nothing.
#[test]
fn consent_from_the_wrong_origin_is_rejected() {
    for role in [Role::Host, Role::Controller] {
        let wrong = match local_origin(role) {
            ConsentOrigin::LocalUser => ConsentOrigin::AuthenticatedRemote,
            ConsentOrigin::AuthenticatedRemote => ConsentOrigin::LocalUser,
        };
        for event in [
            Event::ConsentGranted {
                scope: Scope::ViewOnly,
                origin: wrong,
            },
            Event::ConsentDenied { origin: wrong },
        ] {
            let mut m = machine_in(role, Scope::ViewAndControl, SessionState::AwaitingConsent);
            let before = m.clone();
            let error = m.apply(event, T0).expect_err("wrong origin");
            assert_eq!(error.reason, RejectReason::ConsentOriginNotPermitted);
            assert_eq!(m, before);
        }
    }
}

#[test]
fn consent_cannot_exceed_the_requested_scope() {
    let mut m = machine_in(Role::Host, Scope::ViewOnly, SessionState::AwaitingConsent);
    let before = m.clone();
    let error = m
        .apply(
            Event::ConsentGranted {
                scope: Scope::ViewAndControl,
                origin: ConsentOrigin::LocalUser,
            },
            T0,
        )
        .expect_err("escalation");
    assert_eq!(error.reason, RejectReason::ScopeExceedsRequest);
    assert_eq!(m, before);
    // Granting less than requested is allowed.
    let mut m = machine_in(
        Role::Host,
        Scope::ViewAndControl,
        SessionState::AwaitingConsent,
    );
    m.apply(
        Event::ConsentGranted {
            scope: Scope::ViewOnly,
            origin: ConsentOrigin::LocalUser,
        },
        T0,
    )
    .expect("narrower grant");
    assert_eq!(m.snapshot().granted_scope, Some(Scope::ViewOnly));
}

// T-UNI-02 / T-CORE-03: every (state, event) pair is accepted exactly when the
// independent spec allows it; rejections never change the machine.
#[test]
fn invalid_transitions_are_rejected_without_mutation() {
    let mut checked = 0usize;
    let mut rejected = 0usize;
    for role in [Role::Host, Role::Controller] {
        for scope in [Scope::ViewOnly, Scope::ViewAndControl] {
            let mut starts = vec![
                machine_in(role, scope, SessionState::Idle),
                machine_in(role, scope, SessionState::Preparing),
                machine_in(role, scope, SessionState::AwaitingConsent),
                machine_in(role, scope, SessionState::Connecting),
                machine_in(role, scope, SessionState::Connected),
                machine_in(role, scope, SessionState::Disconnecting),
                machine_in(role, scope, SessionState::Disconnected),
            ];
            let mut degraded = machine_in(role, scope, SessionState::Connected);
            degraded.apply(Event::LinkDegraded, T0).expect("degrade");
            starts.push(degraded);
            let mut failed = machine_in(role, scope, SessionState::Disconnecting);
            failed
                .apply(Event::Fault(FaultKind::Internal), T0)
                .expect("escalate");
            failed.apply(Event::TeardownComplete, T0).expect("fail");
            starts.push(failed);

            for start in &starts {
                for event in all_events() {
                    let mut m = start.clone();
                    let result = m.apply(event, T0);
                    checked += 1;
                    assert_eq!(
                        result.is_ok(),
                        spec_allows(start, event),
                        "{role:?} {:?} {event:?}",
                        start.state()
                    );
                    if let Err(error) = result {
                        rejected += 1;
                        assert_eq!(&m, start, "rejected event mutated the machine");
                        assert_eq!(error.state, start.state());
                        assert_eq!(error.event, event.name());
                        if start.state().is_terminal() {
                            assert_eq!(error.reason, RejectReason::SessionEnded);
                        }
                    }
                }
            }
        }
    }
    eprintln!("checked {checked} (state, event) pairs, {rejected} rejected");
    assert!(checked > 1_000);
}

// T-CORE-06: duplicate stop requests during teardown are rejected as AlreadyStopping.
#[test]
fn duplicate_stops_during_teardown_change_nothing() {
    let mut m = machine_in(
        Role::Host,
        Scope::ViewAndControl,
        SessionState::Disconnecting,
    );
    let before = m.clone();
    for event in [
        Event::LocalStop(StopKind::Disconnect),
        Event::LocalStop(StopKind::Revoke),
        Event::PeerBye(PeerByeReason::UserDisconnect),
    ] {
        assert_eq!(
            m.apply(event, T0).expect_err("dup").reason,
            RejectReason::AlreadyStopping
        );
        assert_eq!(m, before);
    }
}

// T-CORE-07 / row 11: a stop while Connecting wins over a late TransportReady.
#[test]
fn disconnect_during_connecting_cannot_be_undone_by_a_late_transport() {
    let mut m = machine_in(Role::Host, Scope::ViewAndControl, SessionState::Connecting);
    m.apply(Event::LocalStop(StopKind::Disconnect), T0)
        .expect("stop");
    let error = m
        .apply(Event::TransportReady, T0)
        .expect_err("late transport");
    assert_eq!(error.reason, RejectReason::NotAllowedInState);
    assert_eq!(m.state(), SessionState::Disconnecting);
    assert!(!m.input_allowed(T0));
}

// Rows 13–14: link degradation closes the input gate at once; restoration reopens it.
#[test]
fn link_degradation_closes_and_restoration_reopens_the_gate() {
    let mut m = machine_in(Role::Host, Scope::ViewAndControl, SessionState::Connected);
    assert!(m.input_allowed(T0));
    let t = m.apply(Event::LinkDegraded, T0).expect("degrade");
    assert_eq!(t.effects, [Effect::CloseInputGate, Effect::PauseMedia]);
    assert!(!m.input_allowed(T0));
    assert_eq!(m.snapshot().link, Some(LinkState::Degraded));
    let t = m.apply(Event::LinkRestored, after(5)).expect("restore");
    assert_eq!(
        t.effects,
        [
            Effect::ResumeMedia,
            Effect::RequestKeyframe,
            Effect::OpenInputGate
        ]
    );
    assert!(m.input_allowed(after(5)));
}

// Rows 19–21: faults during teardown escalate; the watchdog forces Failed.
#[test]
fn teardown_escalation_and_watchdog() {
    let mut m = machine_in(
        Role::Host,
        Scope::ViewAndControl,
        SessionState::Disconnecting,
    );
    assert_eq!(m.snapshot().outcome, Some(Outcome::Normal));
    let t = m
        .apply(Event::Fault(FaultKind::Internal), T0)
        .expect("escalate");
    assert_eq!(
        (t.from, t.to),
        (SessionState::Disconnecting, SessionState::Disconnecting)
    );
    m.apply(Event::TeardownComplete, T0).expect("done");
    let s = m.snapshot();
    assert_eq!(s.state, SessionState::Failed);
    assert_eq!(
        s.escalation,
        Some(Escalation::FaultDuringTeardown(FaultKind::Internal))
    );
    assert_eq!(
        s.end_reason,
        Some(EndReason::LocalStop(StopKind::Disconnect))
    );

    let mut m = machine_in(
        Role::Host,
        Scope::ViewAndControl,
        SessionState::Disconnecting,
    );
    assert!(m.tick(after(4)).is_none(), "watchdog is 5 s");
    let t = m.tick(after(5)).expect("watchdog fires");
    assert_eq!(t.cause, Cause::Timer(TimerKind::TeardownWatchdog));
    assert_eq!(t.to, SessionState::Failed);
    assert_eq!(m.snapshot().escalation, Some(Escalation::TeardownTimeout));
}

// T-UNI-06 / T-CORE-05: every timer fires exactly at its deadline, never earlier, with
// the outcome ADR-003 assigns to it.
#[test]
fn timers_fire_exactly_at_their_deadlines() {
    let cases: [(Role, SessionState, u64, TimerKind, EndReason); 6] = [
        (
            Role::Host,
            SessionState::Preparing,
            600,
            TimerKind::OfferExpiry,
            EndReason::OfferExpired,
        ),
        (
            Role::Controller,
            SessionState::Preparing,
            10,
            TimerKind::PrepareTimeout,
            EndReason::Fault(FaultKind::PrepareTimeout),
        ),
        (
            Role::Host,
            SessionState::AwaitingConsent,
            60,
            TimerKind::ConsentDecision,
            EndReason::ConsentTimeout,
        ),
        (
            Role::Controller,
            SessionState::AwaitingConsent,
            70,
            TimerKind::ConsentWait,
            EndReason::Fault(FaultKind::ConsentWaitTimeout),
        ),
        (
            Role::Host,
            SessionState::Connecting,
            20,
            TimerKind::ConnectTimeout,
            EndReason::Fault(FaultKind::ConnectTimeout),
        ),
        (
            Role::Controller,
            SessionState::Connecting,
            20,
            TimerKind::ConnectTimeout,
            EndReason::Fault(FaultKind::ConnectTimeout),
        ),
    ];
    for (role, state, secs, timer, reason) in cases {
        let mut m = machine_in(role, Scope::ViewAndControl, state);
        assert_eq!(
            m.next_deadline(),
            Some((timer, after(secs))),
            "{role:?} {state:?}"
        );
        let before = m.clone();
        assert!(m.tick(after(secs - 1)).is_none());
        assert_eq!(m, before, "an early tick changes nothing");
        let t = m.tick(after(secs)).expect("fires");
        assert_eq!(t.cause, Cause::Timer(timer));
        assert_eq!(t.to, SessionState::Disconnecting);
        assert_eq!(m.snapshot().end_reason, Some(reason));
        assert_eq!(m.snapshot().outcome, Some(reason.outcome()));
    }
}

#[test]
fn reconnect_grace_and_session_ttl() {
    // Grace: lost link → Failed(TransportLost) after 15 s.
    let mut m = machine_in(Role::Host, Scope::ViewAndControl, SessionState::Connected);
    m.apply(Event::LinkDegraded, after(100)).expect("degrade");
    assert!(m.tick(after(114)).is_none());
    let t = m.tick(after(115)).expect("grace expires");
    assert_eq!(t.cause, Cause::Timer(TimerKind::ReconnectGrace));
    assert_eq!(
        m.snapshot().end_reason,
        Some(EndReason::Fault(FaultKind::TransportLost))
    );

    // Restored within grace: the grace timer is gone.
    let mut m = machine_in(Role::Host, Scope::ViewAndControl, SessionState::Connected);
    m.apply(Event::LinkDegraded, after(100)).expect("degrade");
    m.apply(Event::LinkRestored, after(110)).expect("restore");
    assert!(m.tick(after(200)).is_none());
    assert_eq!(m.state(), SessionState::Connected);

    // TTL: counted from consent (T0 in machine_in), host only, normal outcome.
    let mut m = machine_in(Role::Host, Scope::ViewAndControl, SessionState::Connected);
    let ttl = T0.saturating_add(Timeouts::default().session_ttl);
    assert_eq!(m.snapshot().expires_at, Some(ttl));
    assert!(m.input_allowed(MonoTime::from_micros(ttl.as_micros() - 1)));
    assert!(
        !m.input_allowed(ttl),
        "input stops at the TTL even before tick runs"
    );
    let t = m.tick(ttl).expect("ttl");
    assert_eq!(t.cause, Cause::Timer(TimerKind::SessionTtl));
    assert_eq!(m.snapshot().end_reason, Some(EndReason::SessionExpired));
    assert_eq!(m.snapshot().outcome, Some(Outcome::Normal));
}

// T-CORE-09 (machine part) / I: an ended machine rejects everything, timers included.
#[test]
fn an_ended_session_rejects_every_event_and_has_no_timers() {
    for terminal in [SessionState::Disconnected] {
        let m = machine_in(Role::Host, Scope::ViewAndControl, terminal);
        assert_eq!(m.next_deadline(), None);
        for event in all_events() {
            let mut copy = m.clone();
            assert_eq!(
                copy.apply(event, T0).expect_err("ended").reason,
                RejectReason::SessionEnded
            );
            assert!(
                copy.tick(MonoTime::from_secs(u64::MAX / 2_000_000))
                    .is_none()
            );
            assert_eq!(copy, m);
        }
    }
}
