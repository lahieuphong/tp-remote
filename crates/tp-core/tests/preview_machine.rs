//! Exhaustive exploration of the local preview state machine (CAP-02A). Every event,
//! with the current, an older and a newer generation, is applied in every reachable
//! configuration up to a fixed depth, and the invariants below are checked on every edge.

use tp_contracts::error::{CoreError, ErrorCategory};
use tp_core::preview::{EndReason, PreviewMachine, PreviewState, Reject, StopAction};

const ERROR: CoreError = CoreError::new(ErrorCategory::InternalError, "exploration");

#[derive(Debug, Clone, Copy)]
enum Ev {
    RequestStart,
    Cancelled(u32),
    PickerFailed(u32),
    Selected(u32),
    Started(u32),
    StartFailed(u32),
    Stop,
    StreamEnded(u32, EndReason),
    Stopped(u32),
}

fn events(current: u32) -> Vec<Ev> {
    let mut all = vec![Ev::RequestStart, Ev::Stop];
    for g in [current.saturating_sub(1), current, current + 1] {
        all.extend([
            Ev::Cancelled(g),
            Ev::PickerFailed(g),
            Ev::Selected(g),
            Ev::Started(g),
            Ev::StartFailed(g),
            Ev::StreamEnded(g, EndReason::TargetClosed),
            Ev::StreamEnded(g, EndReason::DeviceLost),
            Ev::StreamEnded(g, EndReason::Failed(ERROR)),
            Ev::Stopped(g),
        ]);
    }
    all
}

/// Applies `ev`; returns whether it was accepted.
fn apply(m: &mut PreviewMachine, ev: Ev) -> bool {
    match ev {
        Ev::RequestStart => m.request_start().is_ok(),
        Ev::Cancelled(g) => m.cancelled(g).is_ok(),
        Ev::PickerFailed(g) => m.picker_failed(g, ERROR).is_ok(),
        Ev::Selected(g) => m.selected(g).is_ok(),
        Ev::Started(g) => m.started(g).is_ok(),
        Ev::StartFailed(g) => m.start_failed(g, ERROR).is_ok(),
        Ev::Stop => {
            let _ = m.stop();
            true
        }
        Ev::StreamEnded(g, reason) => m.stream_ended(g, reason).is_ok(),
        Ev::Stopped(g) => m.stopped(g).is_ok(),
    }
}

fn event_generation(ev: Ev) -> Option<u32> {
    match ev {
        Ev::RequestStart | Ev::Stop => None,
        Ev::Cancelled(g)
        | Ev::PickerFailed(g)
        | Ev::Selected(g)
        | Ev::Started(g)
        | Ev::StartFailed(g)
        | Ev::StreamEnded(g, _)
        | Ev::Stopped(g) => Some(g),
    }
}

fn check_edge(before: &PreviewMachine, ev: Ev, after: &PreviewMachine, accepted: bool) {
    use PreviewState::*;
    let (from, to) = (before.state(), after.state());
    let ctx = || format!("{from:?} --{ev:?}--> {to:?}");

    // I1: NotAvailable is absorbing.
    if from == NotAvailable {
        assert_eq!(to, NotAvailable, "{}", ctx());
    }
    // I2: events of another generation change nothing.
    if let Some(g) = event_generation(ev)
        && g != before.generation()
    {
        assert!(!accepted, "stale event accepted: {}", ctx());
        assert_eq!(to, from, "{}", ctx());
        assert_eq!(after.rejected(), before.rejected() + 1, "{}", ctx());
    }
    // I3: the generation changes only on an accepted start request, by exactly one.
    if after.generation() != before.generation() {
        assert!(matches!(ev, Ev::RequestStart), "{}", ctx());
        assert_eq!(after.generation(), before.generation() + 1, "{}", ctx());
        assert_eq!(to, SelectingTarget, "{}", ctx());
    }
    // I4: capture can only begin with a selection for the current request, and run only
    // after the provider confirmed it.
    if to == Initializing && from != Initializing {
        assert!(
            matches!(ev, Ev::Selected(g) if g == before.generation()) && from == SelectingTarget,
            "{}",
            ctx()
        );
    }
    if to == Capturing && from != Capturing {
        assert!(
            matches!(ev, Ev::Started(g) if g == before.generation()) && from == Initializing,
            "{}",
            ctx()
        );
    }
    // I5: a cancelled picker never leads to capture.
    if matches!(ev, Ev::Cancelled(_)) && accepted {
        assert_eq!(to, Ready, "{}", ctx());
        assert_eq!(after.last_end(), Some(EndReason::Cancelled));
    }
    // I6: stop never leaves a running capture behind: either nothing may capture, or the
    // run is stopping and the caller must release it.
    if matches!(ev, Ev::Stop) {
        assert!(!matches!(to, Capturing | Initializing), "{}", ctx());
    }
}

/// From any configuration, `stop` and then releasing the stream (`stopped`, or the
/// starter's `start_failed` for a run that was still initializing) reaches a state
/// without capture.
fn can_always_stop(m: &PreviewMachine) {
    let mut m = m.clone();
    let _ = m.stop();
    if m.state() == PreviewState::Stopping {
        let generation = m.generation();
        let mut released = m.clone();
        assert!(released.stopped(generation).is_ok());
        assert!(!released.state().may_capture());
        let mut start_failed = m.clone();
        assert!(start_failed.start_failed(generation, ERROR).is_ok());
        assert!(!start_failed.state().may_capture());
        m = released;
    }
    assert!(!m.state().may_capture(), "stuck in {:?}", m.state());
}

#[test]
fn every_reachable_configuration_respects_the_invariants() {
    let mut frontier = vec![PreviewMachine::new(true), PreviewMachine::new(false)];
    let mut seen: Vec<PreviewMachine> = frontier.clone();
    let mut edges = 0usize;
    let mut reached = std::collections::BTreeSet::new();
    for _depth in 0..7 {
        let mut next = Vec::new();
        for m in &frontier {
            reached.insert(m.state().as_str());
            can_always_stop(m);
            for ev in events(m.generation()) {
                let mut after = m.clone();
                let accepted = apply(&mut after, ev);
                check_edge(m, ev, &after, accepted);
                edges += 1;
                // A rejected event changes only the rejection counter (checked by I2), so
                // only accepted events can lead to new behaviour.
                if accepted && !seen.contains(&after) {
                    seen.push(after.clone());
                    next.push(after);
                }
            }
        }
        frontier = next;
    }
    for state in PreviewState::ALL {
        assert!(reached.contains(state.as_str()), "{state:?} never reached");
    }
    println!("configurations={} edges={edges}", seen.len());
    assert!(edges > 1_000);
}

#[test]
fn a_full_run_and_its_rejections() {
    let mut m = PreviewMachine::new(true);
    let g = m.request_start().expect("start");
    assert_eq!(m.request_start(), Err(Reject::Busy));
    m.selected(g).expect("selected");
    assert_eq!(m.started(g + 1), Err(Reject::StaleGeneration));
    m.started(g).expect("started");
    assert_eq!(
        m.stream_ended(g, EndReason::TargetClosed),
        Ok(StopAction::StopStream { generation: g })
    );
    assert_eq!(m.stop(), StopAction::None, "already stopping");
    m.stopped(g).expect("stopped");
    assert_eq!(m.state(), PreviewState::Stopped);
    assert_eq!(m.last_end(), Some(EndReason::TargetClosed));
    assert_eq!(
        m.stopped(g),
        Err(Reject::UnexpectedState(PreviewState::Stopped))
    );
    assert_eq!(m.rejected(), 3);
}
