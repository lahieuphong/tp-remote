//! Registry scenarios A–K (BOOT-02 §3): identity, stale events, idempotent stop,
//! concurrency and the audit trail.

mod common;

use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

use common::{T0, all_events, registry};
use tp_contracts::capability::Scope;
use tp_contracts::session::{EndReason, FaultKind, PeerByeReason, Role, SessionState, StopKind};
use tp_session::{
    AUDIT_CAPACITY, AuditVerdict, ConsentOrigin, Event, RegistryError, RejectReason,
    SessionRegistry, SharedRegistry, StopOutcome, Timeouts,
};

fn after(secs: u64) -> tp_contracts::time::MonoTime {
    T0.saturating_add(Duration::from_secs(secs))
}

const LOCAL_GRANT: Event = Event::ConsentGranted {
    scope: Scope::ViewAndControl,
    origin: ConsentOrigin::LocalUser,
};

/// Host session driven to Connected.
fn connected_host<I: tp_session::SessionIdSource>(
    r: &mut SessionRegistry<I>,
) -> tp_contracts::id::SessionRef {
    let s = r
        .begin(Role::Host, Scope::ViewAndControl, T0)
        .expect("begin")
        .session;
    r.apply(s, Event::Prepared, T0).expect("prepared");
    r.apply(s, LOCAL_GRANT, T0).expect("consent");
    r.apply(s, Event::TransportReady, T0).expect("ready");
    s
}

// A + E: normal preparation, authorized connection, shutdown.
#[test]
fn scenario_a_e_normal_session_and_shutdown() {
    let mut r = registry();
    let s = connected_host(&mut r);
    assert!(r.input_allowed(s, after(1)));
    let StopOutcome::Started(t) = r.stop(s, StopKind::Disconnect, after(2)).expect("stop") else {
        panic!("first stop must start teardown");
    };
    assert_eq!(t.to, SessionState::Disconnecting);
    assert!(!r.input_allowed(s, after(2)));
    r.apply(s, Event::TeardownComplete, after(3)).expect("done");
    assert_eq!(
        r.current().expect("snapshot").state,
        SessionState::Disconnected
    );
}

// B: local user denies.
#[test]
fn scenario_b_local_denial() {
    let mut r = registry();
    let s = r
        .begin(Role::Host, Scope::ViewAndControl, T0)
        .expect("begin")
        .session;
    r.apply(s, Event::Prepared, T0).expect("prepared");
    r.apply(
        s,
        Event::ConsentDenied {
            origin: ConsentOrigin::LocalUser,
        },
        T0,
    )
    .expect("deny");
    r.apply(s, Event::TeardownComplete, T0).expect("done");
    let snapshot = r.current().expect("snapshot");
    assert_eq!(snapshot.state, SessionState::Disconnected);
    assert_eq!(snapshot.end_reason, Some(EndReason::ConsentDenied));
    assert_eq!(snapshot.granted_scope, None);
}

// C + T-CORE-05: preparation fails, or times out.
#[test]
fn scenario_c_preparation_failure_and_timeout() {
    let mut r = registry();
    let s = r
        .begin(Role::Controller, Scope::ViewOnly, T0)
        .expect("begin")
        .session;
    r.apply(s, Event::Fault(FaultKind::AuthenticationFailed), T0)
        .expect("fault");
    r.apply(s, Event::TeardownComplete, T0).expect("done");
    assert_eq!(r.current().expect("snapshot").state, SessionState::Failed);

    let s = r
        .begin(Role::Controller, Scope::ViewOnly, after(10))
        .expect("begin")
        .session;
    assert!(r.tick(after(19)).is_empty());
    let fired = r.tick(after(20));
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].to, SessionState::Disconnecting);
    let snapshot = r.snapshot(s).expect("snapshot");
    assert_eq!(
        snapshot.end_reason,
        Some(EndReason::Fault(FaultKind::PrepareTimeout))
    );
    // The watchdog finishes an unacknowledged teardown.
    let fired = r.tick(after(25));
    assert_eq!(fired.len(), 1);
    assert_eq!(r.current().expect("snapshot").state, SessionState::Failed);
}

// D: connection times out.
#[test]
fn scenario_d_connection_timeout() {
    let mut r = registry();
    let s = r
        .begin(Role::Host, Scope::ViewAndControl, T0)
        .expect("begin")
        .session;
    r.apply(s, Event::Prepared, T0).expect("prepared");
    r.apply(s, LOCAL_GRANT, T0).expect("consent");
    let fired = r.tick(after(20));
    assert_eq!(fired.len(), 1, "only the connect timer is due");
    assert_eq!(
        r.snapshot(s).expect("snapshot").end_reason,
        Some(EndReason::Fault(FaultKind::ConnectTimeout))
    );
}

// F: revocation while connected closes the gate in the same transition.
#[test]
fn scenario_f_revocation_while_connected() {
    let mut r = registry();
    let s = connected_host(&mut r);
    let StopOutcome::Started(t) = r.stop(s, StopKind::Revoke, after(1)).expect("revoke") else {
        panic!("revoke must start teardown");
    };
    assert_eq!(t.effects.first(), Some(&tp_session::Effect::CloseInputGate));
    assert!(!r.input_allowed(s, after(1)));
    assert_eq!(
        r.snapshot(s).expect("snapshot").end_reason,
        Some(EndReason::LocalStop(StopKind::Revoke))
    );
}

// G: the peer disconnects, with or without a Bye.
#[test]
fn scenario_g_peer_disconnects_unexpectedly() {
    let mut r = registry();
    let s = connected_host(&mut r);
    r.apply(s, Event::PeerBye(PeerByeReason::UserDisconnect), after(1))
        .expect("bye");
    r.apply(s, Event::TeardownComplete, after(1)).expect("done");
    assert_eq!(
        r.current().expect("snapshot").state,
        SessionState::Disconnected
    );

    let s = connected_host(&mut r);
    r.apply(s, Event::LinkDegraded, after(100))
        .expect("degraded");
    assert!(!r.input_allowed(s, after(100)));
    let fired = r.tick(after(115));
    assert_eq!(fired.len(), 1);
    assert_eq!(
        r.snapshot(s).expect("snapshot").end_reason,
        Some(EndReason::Fault(FaultKind::TransportLost))
    );
}

// H / T-CORE-06: duplicate disconnects are idempotent.
#[test]
fn scenario_h_duplicate_disconnects() {
    let mut r = registry();
    let s = connected_host(&mut r);
    assert!(matches!(
        r.stop(s, StopKind::Disconnect, T0),
        Ok(StopOutcome::Started(_))
    ));
    let before = r.current();
    assert!(matches!(
        r.stop(s, StopKind::Disconnect, T0),
        Ok(StopOutcome::AlreadyStopping(_))
    ));
    assert!(matches!(
        r.stop(s, StopKind::Revoke, T0),
        Ok(StopOutcome::AlreadyStopping(_))
    ));
    assert_eq!(r.current(), before, "duplicates change nothing");
    r.apply(s, Event::TeardownComplete, T0).expect("done");
    assert!(matches!(
        r.stop(s, StopKind::Disconnect, T0),
        Ok(StopOutcome::AlreadyEnded(_))
    ));
    assert_eq!(
        r.current().expect("snapshot").state,
        SessionState::Disconnected
    );
}

// I: events after the session ended.
#[test]
fn scenario_i_events_after_the_end_are_rejected() {
    let mut r = registry();
    let s = connected_host(&mut r);
    r.stop(s, StopKind::Disconnect, T0).expect("stop");
    r.apply(s, Event::TeardownComplete, T0).expect("done");
    let ended = r.current();
    for event in all_events() {
        let error = r.apply(s, event, after(1)).expect_err("ended");
        assert!(
            matches!(error, RegistryError::Rejected(e) if e.reason == RejectReason::SessionEnded),
            "{event:?}: {error:?}"
        );
    }
    assert!(r.tick(after(100_000)).is_empty());
    assert_eq!(r.current(), ended);
    assert!(!r.input_allowed(s, after(1)));
}

// K + T-CORE-08 + T-CORE-09: a new session gets a new identity; the old one's events,
// including stops, are rejected and never touch the new session.
#[test]
fn scenario_k_previous_session_events_during_a_new_session() {
    let mut r = registry();
    let old = connected_host(&mut r);
    r.stop(old, StopKind::Disconnect, T0).expect("stop");
    r.apply(old, Event::TeardownComplete, T0).expect("done");

    let new = connected_host(&mut r);
    assert_ne!(new.id, old.id, "new identifier");
    assert!(
        new.generation > old.generation,
        "generation strictly increases"
    );

    for event in all_events() {
        let error = r.apply(old, event, after(1)).expect_err("stale");
        assert_eq!(error, RegistryError::StaleSession { current: new });
    }
    assert_eq!(
        r.stop(old, StopKind::Disconnect, after(1)),
        Err(RegistryError::StaleSession { current: new })
    );
    assert_eq!(r.snapshot(old), None);
    assert!(!r.input_allowed(old, after(1)));
    let current = r.current().expect("snapshot");
    assert_eq!(current.session, new);
    assert_eq!(current.state, SessionState::Connected);
    assert!(r.input_allowed(new, after(1)));

    // Same id but a different generation is stale too.
    let forged = tp_contracts::id::SessionRef {
        id: new.id,
        generation: old.generation,
    };
    assert_eq!(
        r.apply(forged, Event::LinkDegraded, after(1)),
        Err(RegistryError::StaleSession { current: new })
    );
}

#[test]
fn only_one_active_session_at_a_time() {
    let mut r = registry();
    let s = r
        .begin(Role::Host, Scope::ViewOnly, T0)
        .expect("begin")
        .session;
    assert_eq!(
        r.begin(Role::Host, Scope::ViewOnly, T0).map(|t| t.session),
        Err(RegistryError::Busy { active: s })
    );
    r.stop(s, StopKind::Cancel, T0).expect("stop");
    assert!(matches!(
        r.begin(Role::Host, Scope::ViewOnly, T0),
        Err(RegistryError::Busy { .. })
    ));
    r.apply(s, Event::TeardownComplete, T0).expect("done");
    assert!(r.begin(Role::Host, Scope::ViewOnly, T0).is_ok());
}

#[test]
fn events_without_any_session_are_rejected() {
    let mut r = registry();
    let s = common::sref(9);
    assert_eq!(
        r.apply(s, Event::Prepared, T0),
        Err(RegistryError::NoSession)
    );
    assert_eq!(
        r.stop(s, StopKind::Disconnect, T0),
        Err(RegistryError::NoSession)
    );
}

#[test]
fn the_audit_trail_records_applied_and_rejected_events_without_payloads() {
    let mut r = registry();
    let s = connected_host(&mut r);
    let _ = r.apply(s, Event::Start, T0);
    let records: Vec<_> = r.audit().copied().collect();
    assert_eq!(records.len(), 5);
    assert_eq!(records[0].cause, "start");
    assert_eq!(records[0].verdict, AuditVerdict::Applied);
    assert_eq!(
        records[4].verdict,
        AuditVerdict::Rejected("notAllowedInState")
    );
    let rendered = format!("{records:?}");
    assert!(
        !rendered.contains("ViewAndControl"),
        "audit carries no event parameters"
    );

    for _ in 0..(AUDIT_CAPACITY + 10) {
        let _ = r.apply(s, Event::Start, T0);
    }
    assert_eq!(r.audit().count(), AUDIT_CAPACITY, "bounded");
}

// J / T-CORE-10: concurrent TransportReady and stop requests. Whatever the interleaving,
// exactly one stop starts the teardown and nothing reaches Connected after it.
#[test]
fn scenario_j_concurrent_connect_and_disconnect() {
    const ROUNDS: usize = 200;
    const THREADS: usize = 8;
    let shared = SharedRegistry::new(SessionRegistry::new(
        common::Ids::default(),
        Timeouts::default(),
    ));
    let mut connected_first = 0usize;

    for _ in 0..ROUNDS {
        let s = shared
            .begin(Role::Host, Scope::ViewAndControl, T0)
            .expect("begin")
            .session;
        shared.apply(s, Event::Prepared, T0).expect("prepared");
        shared.apply(s, LOCAL_GRANT, T0).expect("consent");

        let barrier = Arc::new(Barrier::new(THREADS));
        let handles: Vec<_> = (0..THREADS)
            .map(|i| {
                let shared = shared.clone();
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    if i % 2 == 0 {
                        shared.apply(s, Event::TransportReady, T0).map(|_| "ready")
                    } else {
                        shared.stop(s, StopKind::Disconnect, T0).map(|o| match o {
                            StopOutcome::Started(_) => "stopStarted",
                            StopOutcome::AlreadyStopping(_) => "alreadyStopping",
                            StopOutcome::AlreadyEnded(_) => "alreadyEnded",
                        })
                    }
                })
            })
            .collect();
        let results: Vec<_> = handles
            .into_iter()
            .map(|h| h.join().expect("no panic"))
            .collect();

        let stops_started = results.iter().filter(|r| **r == Ok("stopStarted")).count();
        assert_eq!(stops_started, 1, "{results:?}");
        for result in &results {
            if let Err(error) = result {
                assert!(matches!(error, RegistryError::Rejected(_)), "{error:?}");
            }
        }

        let audit: Vec<_> = shared
            .audit()
            .expect("audit")
            .into_iter()
            .filter(|a| a.session == s)
            .collect();
        let stop_index = audit
            .iter()
            .position(|a| a.cause == "localStop" && a.verdict == AuditVerdict::Applied)
            .expect("stop applied");
        assert!(
            audit[stop_index..]
                .iter()
                .all(|a| !(a.verdict == AuditVerdict::Applied
                    && a.to == Some(SessionState::Connected))),
            "Connected after a stop: {audit:?}"
        );
        if audit[..stop_index]
            .iter()
            .any(|a| a.to == Some(SessionState::Connected))
        {
            connected_first += 1;
        }

        let snapshot = shared.current().expect("lock").expect("snapshot");
        assert_eq!(snapshot.state, SessionState::Disconnecting);
        assert_eq!(snapshot.granted_scope, None);
        shared.apply(s, Event::TeardownComplete, T0).expect("done");
    }
    eprintln!("{ROUNDS} rounds; TransportReady won the race in {connected_first}");
}
