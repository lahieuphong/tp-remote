//! T-UNI-04: exhaustive exploration of every reachable machine configuration.
//!
//! Starting from Idle for each role and requested scope, every known event is applied in
//! every reachable configuration, and every pending timer is fired by advancing time to
//! its deadline. Each edge is checked against the safety invariants below. The state
//! space is finite because every timer leads towards termination.

mod common;

use std::collections::{HashSet, VecDeque};

use common::{T0, all_events, spec_allows, sref};
use tp_contracts::capability::Scope;
use tp_contracts::session::{LinkState, Outcome, Role, SessionState};
use tp_contracts::time::MonoTime;
use tp_session::{Cause, ConsentOrigin, Effect, Event, SessionMachine, Timeouts, Transition};

fn check_edge(before: &SessionMachine, t: &Transition, after: &SessionMachine, now: MonoTime) {
    let ctx = format!(
        "{:?} {:?} --{:?}--> {:?}",
        before.role(),
        t.from,
        t.cause,
        t.to
    );

    // A Connected session is only ever entered from Connecting via TransportReady.
    if t.to == SessionState::Connected && t.from != SessionState::Connected {
        assert_eq!(t.from, SessionState::Connecting, "{ctx}");
        assert_eq!(t.cause, Cause::Event(Event::TransportReady), "{ctx}");
        assert!(after.snapshot().granted_scope.is_some(), "{ctx}");
    }
    // On the Host, consent can only come from the local user.
    if before.role() == Role::Host
        && t.from == SessionState::AwaitingConsent
        && t.to == SessionState::Connecting
    {
        assert!(
            matches!(
                t.cause,
                Cause::Event(Event::ConsentGranted {
                    origin: ConsentOrigin::LocalUser,
                    ..
                })
            ),
            "{ctx}"
        );
    }
    // Terminal states are only reached from Disconnecting.
    if t.to.is_terminal() {
        assert_eq!(t.from, SessionState::Disconnecting, "{ctx}");
    }
    // Entering Disconnecting from an active state emits the ordered teardown, gate first.
    if t.to == SessionState::Disconnecting && t.from.is_active() {
        match before.role() {
            Role::Host => assert_eq!(t.effects.first(), Some(&Effect::CloseInputGate), "{ctx}"),
            Role::Controller => assert_eq!(t.effects.first(), Some(&Effect::SendBye), "{ctx}"),
        }
        assert_eq!(t.effects.last(), Some(&Effect::WipeSessionSecrets), "{ctx}");
    }
    check_configuration(after, now);
}

fn check_configuration(m: &SessionMachine, now: MonoTime) {
    let s = m.snapshot();
    let ctx = format!("{:?} {:?}", s.role, s.state);
    // Input gate ⇒ Host ∧ Connected ∧ link up ∧ control scope.
    if m.input_allowed(now) {
        assert_eq!(s.role, Role::Host, "{ctx}");
        assert_eq!(s.state, SessionState::Connected, "{ctx}");
        assert_eq!(s.link, Some(LinkState::Up), "{ctx}");
        assert_eq!(s.granted_scope, Some(Scope::ViewAndControl), "{ctx}");
    }
    // Link state is only reported while Connected.
    assert_eq!(
        s.link.is_some(),
        s.state == SessionState::Connected,
        "{ctx}"
    );
    // A grant only exists while connecting or connected.
    if s.granted_scope.is_some() {
        assert!(
            matches!(s.state, SessionState::Connecting | SessionState::Connected),
            "{ctx}"
        );
    }
    // Outcomes match terminal states; ended machines have no timers.
    match s.state {
        SessionState::Disconnected => assert_eq!(s.outcome, Some(Outcome::Normal), "{ctx}"),
        SessionState::Failed => assert_eq!(s.outcome, Some(Outcome::Error), "{ctx}"),
        _ => {}
    }
    if s.state.is_terminal() {
        assert_eq!(m.next_deadline(), None, "{ctx}");
    }
    if s.state == SessionState::Disconnecting {
        assert!(s.outcome.is_some() && s.end_reason.is_some(), "{ctx}");
    }
}

#[test]
fn every_reachable_configuration_satisfies_the_safety_invariants() {
    let events = all_events();
    let mut total_configurations = 0usize;
    let mut total_edges = 0usize;

    for role in [Role::Host, Role::Controller] {
        for requested in [Scope::ViewOnly, Scope::ViewAndControl] {
            let start = SessionMachine::new(sref(1), role, requested, Timeouts::default());
            let mut seen: HashSet<(SessionMachine, MonoTime)> = HashSet::new();
            let mut queue = VecDeque::from([(start, T0)]);
            let mut states_reached: HashSet<SessionState> = HashSet::new();

            while let Some((m, now)) = queue.pop_front() {
                if !seen.insert((m.clone(), now)) {
                    continue;
                }
                states_reached.insert(m.state());
                check_configuration(&m, now);

                for &event in &events {
                    let mut next = m.clone();
                    total_edges += 1;
                    match next.apply(event, now) {
                        Ok(t) => {
                            assert!(spec_allows(&m, event), "accepted against spec: {event:?}");
                            check_edge(&m, &t, &next, now);
                            queue.push_back((next, now));
                        }
                        Err(_) => {
                            assert!(!spec_allows(&m, event), "rejected against spec: {event:?}");
                            assert_eq!(next, m, "rejection mutated the machine");
                        }
                    }
                }
                if let Some((_, at)) = m.next_deadline() {
                    let later = at.max(now);
                    let mut next = m.clone();
                    total_edges += 1;
                    if let Some(t) = next.tick(later) {
                        check_edge(&m, &t, &next, later);
                        queue.push_back((next, later));
                    }
                }
            }

            for state in SessionState::ALL {
                assert!(
                    states_reached.contains(&state),
                    "{role:?} {requested:?} never reached {state:?}"
                );
            }
            total_configurations += seen.len();
        }
    }
    eprintln!("explored {total_configurations} configurations and {total_edges} edges");
    assert!(total_configurations > 50);
}
