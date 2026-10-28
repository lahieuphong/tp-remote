//! The input gate (T-SEC-08 mock level) and what the engine logs (T-LOG-02/03/04).
//! A mock controller proves gate logic only; no OS input API exists in BOOT-02.

use std::sync::Arc;

use tp_contracts::capability::{Availability, Capability, PlatformCapabilities, Scope};
use tp_contracts::error::CoreError;
use tp_contracts::id::{DeviceId, SessionRef};
use tp_contracts::input::{
    InjectableEvent, InputCapabilities, InputController, KeyUsage, PointerButton,
};
use tp_contracts::session::{Role, SessionManager, StopKind};
use tp_contracts::time::MonoTime;
use tp_core::gate::InputGate;
use tp_core::service::{LifecycleEvent, SessionService};
use tp_diagnostics::{Logger, MemorySink};
use tp_security::{AuthenticatedPeer, ConsentDecision, LocalConsent};
use tp_session::{SequentialIds, Timeouts};

const T0: MonoTime = MonoTime::from_secs(1_000);

const ALL_AVAILABLE: PlatformCapabilities = PlatformCapabilities {
    view_screen: Availability::Available,
    control_input: Availability::Available,
};

// Distinctive values: if they ever show up in a log line, input data leaked.
const SECRET_X: i32 = 31_337;
const SECRET_Y: i32 = 27_182;
const SECRET_KEY: u16 = 0x2A7F;

/// Records what would have been injected.
#[derive(Default)]
struct RecordingController {
    injected: Vec<InjectableEvent>,
    releases: u32,
}

impl InputController for RecordingController {
    fn capabilities(&self) -> InputCapabilities {
        InputCapabilities {
            availability: Availability::Available,
            absolute_pointer: true,
        }
    }
    fn inject(&mut self, event: &InjectableEvent) -> Result<(), CoreError> {
        self.injected.push(*event);
        Ok(())
    }
    fn release_all(&mut self) -> Result<(), CoreError> {
        self.releases += 1;
        Ok(())
    }
}

fn events() -> [InjectableEvent; 3] {
    [
        InjectableEvent::PointerMove {
            x: SECRET_X,
            y: SECRET_Y,
        },
        InjectableEvent::PointerButton {
            x: SECRET_X,
            y: SECRET_Y,
            button: PointerButton::Left,
            pressed: true,
        },
        InjectableEvent::Key {
            usage: KeyUsage(SECRET_KEY),
            pressed: true,
        },
    ]
}

fn setup(
    platform: PlatformCapabilities,
) -> (SessionService<SequentialIds>, Arc<MemorySink>, SessionRef) {
    let sink = Arc::new(MemorySink::new());
    let mut svc = SessionService::new(
        SequentialIds::default(),
        Timeouts::default(),
        platform,
        Logger::new(sink.clone()),
    );
    let s = svc
        .begin(Role::Host, Scope::ViewAndControl, T0)
        .expect("begin");
    let mut device = [0u8; 16];
    device[0] = 7;
    svc.record_authenticated_peer(AuthenticatedPeer::for_tests(
        s,
        DeviceId::from_bytes(device).expect("id"),
    ))
    .expect("peer");
    svc.handle(s, LifecycleEvent::Prepared, T0)
        .expect("prepared");
    (svc, sink, s)
}

fn consent_and_connect(svc: &mut SessionService<SequentialIds>, s: SessionRef, scope: Scope) {
    svc.respond_consent(
        s,
        LocalConsent::for_tests(s, ConsentDecision::Allow(scope), T0),
        T0,
    )
    .expect("consent");
    svc.handle(s, LifecycleEvent::TransportReady, T0)
        .expect("ready");
}

fn assert_no_input_data(lines: &[String]) {
    for line in lines {
        for needle in [
            SECRET_X.to_string(),
            SECRET_Y.to_string(),
            format!("{SECRET_KEY}"),
            "2a7f".into(),
            "2A7F".into(),
        ] {
            assert!(
                !line.contains(&needle),
                "input data {needle} in log: {line}"
            );
        }
    }
}

// T-SEC-08 (mock level): input before consent never reaches the controller.
#[test]
fn input_before_consent_is_never_injected() {
    let (svc, sink, s) = setup(ALL_AVAILABLE);
    let mut gate = InputGate::new(RecordingController::default());
    for event in events() {
        assert!(gate.inject(&svc, s, &event, T0).is_err());
    }
    assert!(gate.controller().injected.is_empty());
    assert_eq!((gate.injected(), gate.denied()), (0, 3));
    assert_no_input_data(&sink.lines());
}

#[test]
fn input_is_injected_only_while_fully_authorized() {
    let (mut svc, sink, s) = setup(ALL_AVAILABLE);
    consent_and_connect(&mut svc, s, Scope::ViewAndControl);
    let mut gate = InputGate::new(RecordingController::default());
    for event in events() {
        gate.inject(&svc, s, &event, T0).expect("authorized");
    }
    assert_eq!(gate.controller().injected.len(), 3);

    // Link loss closes the gate for the next event.
    svc.handle(s, LifecycleEvent::LinkDegraded, T0)
        .expect("degraded");
    assert!(gate.inject(&svc, s, &events()[0], T0).is_err());
    svc.handle(s, LifecycleEvent::LinkRestored, T0)
        .expect("restored");
    gate.inject(&svc, s, &events()[0], T0).expect("restored");

    // Revocation closes it immediately (T-SEC-06 mock level).
    svc.disconnect(s, StopKind::Revoke, T0).expect("revoke");
    for event in events() {
        assert!(gate.inject(&svc, s, &event, T0).is_err());
    }
    assert_eq!(
        gate.controller().injected.len(),
        4,
        "nothing after revocation"
    );
    gate.release_all().expect("release");
    assert_eq!(gate.controller().releases, 1);
    assert_no_input_data(&sink.lines());
}

#[test]
fn view_only_consent_never_allows_input() {
    let (mut svc, _, s) = setup(ALL_AVAILABLE);
    consent_and_connect(&mut svc, s, Scope::ViewOnly);
    let mut gate = InputGate::new(RecordingController::default());
    let error = gate
        .inject(&svc, s, &events()[2], T0)
        .expect_err("view only");
    assert_eq!(
        error.category(),
        tp_contracts::error::ErrorCategory::PermissionDenied
    );
    assert!(gate.controller().injected.is_empty());
}

// The production platform advertises nothing: the gate denies even a fully consented
// session.
#[test]
fn the_production_platform_lets_no_input_through() {
    let (mut svc, _, s) = setup(tp_core::platform_capabilities());
    consent_and_connect(&mut svc, s, Scope::ViewAndControl);
    let mut gate = InputGate::new(RecordingController::default());
    let error = gate
        .inject(&svc, s, &events()[0], T0)
        .expect_err("not implemented");
    assert_eq!(
        error.category(),
        tp_contracts::error::ErrorCategory::UnsupportedCapability
    );
    assert!(gate.controller().injected.is_empty());
}

// T-LOG-03: unauthorized events are logged with their kind and reason, never their data.
#[test]
fn denied_input_is_logged_without_payload() {
    let (svc, sink, s) = setup(ALL_AVAILABLE);
    let mut gate = InputGate::new(RecordingController::default());
    let _ = gate.inject(&svc, s, &events()[2], T0);
    let lines = sink.lines();
    let denial = lines
        .iter()
        .find(|l| l.contains("event=authz.denied"))
        .expect("denial logged");
    assert!(denial.contains("capability=controlInput"), "{denial}");
    assert!(denial.contains("reason=sessionNotActive"), "{denial}");
    assert!(denial.contains("event_kind=key"), "{denial}");
    assert!(denial.contains("session=00000000/g1"), "{denial}");
    assert_no_input_data(&lines);
}

#[test]
fn lifecycle_logging_is_complete_and_payload_free() {
    let (mut svc, sink, s) = setup(ALL_AVAILABLE);
    consent_and_connect(&mut svc, s, Scope::ViewAndControl);
    svc.disconnect(s, StopKind::Disconnect, T0).expect("stop");
    let _ = svc.handle(s, LifecycleEvent::TransportReady, T0); // rejected: already stopping
    let lines = sink.lines();
    let transitions = lines
        .iter()
        .filter(|l| l.contains("event=session.transition"))
        .count();
    assert_eq!(transitions, 5, "{lines:#?}");
    assert!(
        lines
            .iter()
            .any(|l| l.contains("event=session.rejected") && l.contains("cause=transportReady"))
    );
    for line in &lines {
        assert!(
            !line.contains("[REDACTED]"),
            "engine events must use allowed fields only: {line}"
        );
    }
}

// T-LOG-02: Debug output of input and capability types is safe.
#[test]
fn debug_output_of_input_events_is_redacted() {
    for event in events() {
        let debug = format!("{event:?}");
        assert!(!debug.contains(&SECRET_X.to_string()), "{debug}");
        assert!(!debug.contains(&SECRET_KEY.to_string()), "{debug}");
        assert!(debug.contains("<redacted>"), "{debug}");
    }
    assert_eq!(
        format!("{:?}", KeyUsage(SECRET_KEY)),
        "KeyUsage(<redacted>)"
    );
    let _ = Capability::ALL;
}
