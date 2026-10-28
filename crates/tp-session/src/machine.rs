//! The single-session lifecycle machine.
//!
//! A machine is created in `Idle`, used for exactly one session and never revived:
//! `Disconnected` and `Failed` reject every event. Every exit from an active state goes
//! through `Disconnecting`, whose ordered cleanup effects are emitted on entry.
//!
//! Time is never read here: every call takes `now`, and deadlines are stored as absolute
//! [`MonoTime`]s that [`SessionMachine::tick`] fires. Because deadlines belong to the
//! state that set them, a timer can never fire for a state the machine has already left.

use std::fmt;
use std::time::Duration;

use tp_contracts::capability::Scope;
use tp_contracts::error::{CoreError, ErrorCategory};
use tp_contracts::id::SessionRef;
use tp_contracts::session::{
    EndReason, Escalation, FaultKind, LinkState, Outcome, PeerByeReason, Role, SessionSnapshot,
    SessionState, StopKind,
};
use tp_contracts::time::MonoTime;

/// Timeouts. Defaults are the initial values of ARCH-00 §6.5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Timeouts {
    /// Host: how long a hosting offer waits for an authenticated request.
    pub offer: Duration,
    /// Controller: connect and handshake.
    pub prepare: Duration,
    /// Host: time the local user has to decide.
    pub consent_decision: Duration,
    /// Controller: time it waits for the Host's decision.
    pub consent_wait: Duration,
    pub connect: Duration,
    pub reconnect_grace: Duration,
    /// Host: session lifetime, counted from consent.
    pub session_ttl: Duration,
    pub teardown: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Timeouts {
            offer: Duration::from_secs(10 * 60),
            prepare: Duration::from_secs(10),
            consent_decision: Duration::from_secs(60),
            consent_wait: Duration::from_secs(70),
            connect: Duration::from_secs(20),
            reconnect_grace: Duration::from_secs(15),
            session_ttl: Duration::from_secs(60 * 60),
            teardown: Duration::from_secs(5),
        }
    }
}

/// Where a consent decision came from. On the Host only `LocalUser` is accepted; on the
/// Controller only `AuthenticatedRemote` (the Host's authenticated `ConsentResult`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConsentOrigin {
    LocalUser,
    AuthenticatedRemote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Event {
    Start,
    /// Authentication succeeded and the session request was exchanged.
    Prepared,
    ConsentGranted {
        scope: Scope,
        origin: ConsentOrigin,
    },
    ConsentDenied {
        origin: ConsentOrigin,
    },
    /// Transport up and the DTLS fingerprint verified.
    TransportReady,
    LinkDegraded,
    LinkRestored,
    LocalStop(StopKind),
    PeerBye(PeerByeReason),
    Fault(FaultKind),
    TeardownComplete,
}

impl Event {
    pub const fn name(&self) -> &'static str {
        match self {
            Event::Start => "start",
            Event::Prepared => "prepared",
            Event::ConsentGranted { .. } => "consentGranted",
            Event::ConsentDenied { .. } => "consentDenied",
            Event::TransportReady => "transportReady",
            Event::LinkDegraded => "linkDegraded",
            Event::LinkRestored => "linkRestored",
            Event::LocalStop(_) => "localStop",
            Event::PeerBye(_) => "peerBye",
            Event::Fault(_) => "fault",
            Event::TeardownComplete => "teardownComplete",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimerKind {
    OfferExpiry,
    PrepareTimeout,
    ConsentDecision,
    ConsentWait,
    ConnectTimeout,
    ReconnectGrace,
    SessionTtl,
    TeardownWatchdog,
}

impl TimerKind {
    pub const fn name(self) -> &'static str {
        match self {
            TimerKind::OfferExpiry => "timer:offerExpiry",
            TimerKind::PrepareTimeout => "timer:prepareTimeout",
            TimerKind::ConsentDecision => "timer:consentDecision",
            TimerKind::ConsentWait => "timer:consentWait",
            TimerKind::ConnectTimeout => "timer:connectTimeout",
            TimerKind::ReconnectGrace => "timer:reconnectGrace",
            TimerKind::SessionTtl => "timer:sessionTtl",
            TimerKind::TeardownWatchdog => "timer:teardownWatchdog",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cause {
    Event(Event),
    Timer(TimerKind),
}

impl Cause {
    pub const fn name(&self) -> &'static str {
        match self {
            Cause::Event(event) => event.name(),
            Cause::Timer(timer) => timer.name(),
        }
    }
}

/// What the driver must do after a transition. Effects are idempotent instructions
/// ("stop capture if running"); the machine never performs them itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Effect {
    OpenHostOffer,
    ConnectToHost,
    PromptLocalConsent,
    AwaitConsentResult,
    StartCapture,
    StartTransport,
    ShowSessionIndicator,
    OpenInputGate,
    CloseInputGate,
    PauseMedia,
    ResumeMedia,
    RequestKeyframe,
    ReleaseAllInput,
    StopCapture,
    StopEncoder,
    SendBye,
    CloseTransport,
    CloseSignaling,
    WipeSessionSecrets,
    ReportOutcome(Outcome),
}

/// Ordered teardown (ARCH-00 §5.5). The input gate is closed first, synchronously.
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    pub session: SessionRef,
    pub from: SessionState,
    pub to: SessionState,
    pub cause: Cause,
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RejectReason {
    /// The (state, event) pair is not in the transition table.
    NotAllowedInState,
    /// The session already ended; it can never be revived.
    SessionEnded,
    /// A consent decision arrived from an origin this role must not accept.
    ConsentOriginNotPermitted,
    /// Consent granted more than the Controller requested.
    ScopeExceedsRequest,
    /// The session is already tearing down.
    AlreadyStopping,
}

impl RejectReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            RejectReason::NotAllowedInState => "notAllowedInState",
            RejectReason::SessionEnded => "sessionEnded",
            RejectReason::ConsentOriginNotPermitted => "consentOriginNotPermitted",
            RejectReason::ScopeExceedsRequest => "scopeExceedsRequest",
            RejectReason::AlreadyStopping => "alreadyStopping",
        }
    }
}

/// A rejected event. The machine is unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransitionError {
    pub session: SessionRef,
    pub state: SessionState,
    pub event: &'static str,
    pub reason: RejectReason,
}

impl fmt::Display for TransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "event {} rejected in state {}: {}",
            self.event,
            self.state.as_str(),
            self.reason.as_str()
        )
    }
}

impl std::error::Error for TransitionError {}

impl From<TransitionError> for CoreError {
    fn from(error: TransitionError) -> CoreError {
        match error.reason {
            RejectReason::ConsentOriginNotPermitted => CoreError::new(
                ErrorCategory::PermissionDenied,
                "consent decision from an origin that may not decide",
            ),
            RejectReason::SessionEnded => {
                CoreError::new(ErrorCategory::InvalidState, "session has ended")
            }
            RejectReason::AlreadyStopping => {
                CoreError::new(ErrorCategory::InvalidState, "session is already stopping")
            }
            RejectReason::ScopeExceedsRequest => CoreError::new(
                ErrorCategory::PermissionDenied,
                "granted scope exceeds the requested scope",
            ),
            RejectReason::NotAllowedInState => CoreError::new(
                ErrorCategory::InvalidState,
                "event not allowed in the current state",
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionMachine {
    session: SessionRef,
    role: Role,
    requested: Scope,
    timeouts: Timeouts,
    state: SessionState,
    link: LinkState,
    granted: Option<Scope>,
    state_deadline: Option<(TimerKind, MonoTime)>,
    ttl_deadline: Option<MonoTime>,
    grace_deadline: Option<MonoTime>,
    end_reason: Option<EndReason>,
    escalation: Option<Escalation>,
    outcome: Option<Outcome>,
}

impl SessionMachine {
    pub fn new(session: SessionRef, role: Role, requested: Scope, timeouts: Timeouts) -> Self {
        SessionMachine {
            session,
            role,
            requested,
            timeouts,
            state: SessionState::Idle,
            link: LinkState::Up,
            granted: None,
            state_deadline: None,
            ttl_deadline: None,
            grace_deadline: None,
            end_reason: None,
            escalation: None,
            outcome: None,
        }
    }

    pub fn session(&self) -> SessionRef {
        self.session
    }

    pub fn role(&self) -> Role {
        self.role
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            session: self.session,
            role: self.role,
            state: self.state,
            link: (self.state == SessionState::Connected).then_some(self.link),
            requested_scope: self.requested,
            granted_scope: self.granted,
            expires_at: self.ttl_deadline,
            end_reason: self.end_reason,
            escalation: self.escalation,
            outcome: self.outcome,
        }
    }

    /// ARCH-00 §6.6 input gate, lifecycle part: Host, Connected, link up, control scope,
    /// within the TTL. OS permission and consent revocation are checked by the policy.
    pub fn input_allowed(&self, now: MonoTime) -> bool {
        self.role == Role::Host
            && self.state == SessionState::Connected
            && self.link == LinkState::Up
            && self.granted == Some(Scope::ViewAndControl)
            && self.ttl_deadline.is_some_and(|ttl| now < ttl)
    }

    /// The earliest pending deadline, if any.
    pub fn next_deadline(&self) -> Option<(TimerKind, MonoTime)> {
        [
            self.state_deadline,
            self.ttl_deadline.map(|at| (TimerKind::SessionTtl, at)),
            self.grace_deadline
                .map(|at| (TimerKind::ReconnectGrace, at)),
        ]
        .into_iter()
        .flatten()
        .min_by_key(|(_, at)| *at)
    }

    /// Applies an event. On rejection the machine is unchanged.
    pub fn apply(&mut self, event: Event, now: MonoTime) -> Result<Transition, TransitionError> {
        use SessionState as S;

        let from = self.state;
        if from.is_terminal() {
            return Err(self.reject(event, RejectReason::SessionEnded));
        }
        let effects = match (from, event) {
            (S::Idle, Event::Start) => self.enter_preparing(now),
            (S::Preparing, Event::Prepared) => self.enter_awaiting_consent(now),
            (S::AwaitingConsent, Event::ConsentGranted { scope, origin }) => {
                self.check_origin(event, origin)?;
                if !self.requested.covers(scope) {
                    return Err(self.reject(event, RejectReason::ScopeExceedsRequest));
                }
                self.enter_connecting(scope, now)
            }
            (S::AwaitingConsent, Event::ConsentDenied { origin }) => {
                self.check_origin(event, origin)?;
                self.enter_disconnecting(EndReason::ConsentDenied, now)
            }
            (S::Connecting, Event::TransportReady) => self.enter_connected(),
            (S::Connected, Event::LinkDegraded) if self.link == LinkState::Up => {
                self.link = LinkState::Degraded;
                self.grace_deadline = Some(now.saturating_add(self.timeouts.reconnect_grace));
                vec![Effect::CloseInputGate, Effect::PauseMedia]
            }
            (S::Connected, Event::LinkRestored) if self.link == LinkState::Degraded => {
                self.link = LinkState::Up;
                self.grace_deadline = None;
                let mut effects = vec![Effect::ResumeMedia, Effect::RequestKeyframe];
                if self.controls_input() {
                    effects.push(Effect::OpenInputGate);
                }
                effects
            }
            (
                S::Preparing | S::AwaitingConsent | S::Connecting | S::Connected,
                Event::LocalStop(kind),
            ) => self.enter_disconnecting(EndReason::LocalStop(kind), now),
            (
                S::Preparing | S::AwaitingConsent | S::Connecting | S::Connected,
                Event::PeerBye(reason),
            ) => self.enter_disconnecting(EndReason::PeerBye(reason), now),
            (
                S::Preparing | S::AwaitingConsent | S::Connecting | S::Connected,
                Event::Fault(kind),
            ) => self.enter_disconnecting(EndReason::Fault(kind), now),
            (S::Disconnecting, Event::Fault(kind)) => {
                if self.escalation.is_none() {
                    self.escalation = Some(Escalation::FaultDuringTeardown(kind));
                }
                self.outcome = Some(Outcome::Error);
                Vec::new()
            }
            (S::Disconnecting, Event::TeardownComplete) => self.finish(),
            (S::Disconnecting, Event::LocalStop(_) | Event::PeerBye(_)) => {
                return Err(self.reject(event, RejectReason::AlreadyStopping));
            }
            _ => return Err(self.reject(event, RejectReason::NotAllowedInState)),
        };
        Ok(self.transition(from, Cause::Event(event), effects))
    }

    /// Fires the earliest deadline if it is due at `now`. Call repeatedly until `None`.
    pub fn tick(&mut self, now: MonoTime) -> Option<Transition> {
        let (timer, at) = self.next_deadline()?;
        if at > now {
            return None;
        }
        let from = self.state;
        let effects = match timer {
            TimerKind::OfferExpiry => self.enter_disconnecting(EndReason::OfferExpired, now),
            TimerKind::PrepareTimeout => {
                self.enter_disconnecting(EndReason::Fault(FaultKind::PrepareTimeout), now)
            }
            TimerKind::ConsentDecision => self.enter_disconnecting(EndReason::ConsentTimeout, now),
            TimerKind::ConsentWait => {
                self.enter_disconnecting(EndReason::Fault(FaultKind::ConsentWaitTimeout), now)
            }
            TimerKind::ConnectTimeout => {
                self.enter_disconnecting(EndReason::Fault(FaultKind::ConnectTimeout), now)
            }
            TimerKind::ReconnectGrace => {
                self.enter_disconnecting(EndReason::Fault(FaultKind::TransportLost), now)
            }
            TimerKind::SessionTtl => self.enter_disconnecting(EndReason::SessionExpired, now),
            TimerKind::TeardownWatchdog => {
                if self.escalation.is_none() {
                    self.escalation = Some(Escalation::TeardownTimeout);
                }
                self.outcome = Some(Outcome::Error);
                self.finish()
            }
        };
        Some(self.transition(from, Cause::Timer(timer), effects))
    }

    fn transition(&self, from: SessionState, cause: Cause, effects: Vec<Effect>) -> Transition {
        Transition {
            session: self.session,
            from,
            to: self.state,
            cause,
            effects,
        }
    }

    fn reject(&self, event: Event, reason: RejectReason) -> TransitionError {
        TransitionError {
            session: self.session,
            state: self.state,
            event: event.name(),
            reason,
        }
    }

    fn check_origin(&self, event: Event, origin: ConsentOrigin) -> Result<(), TransitionError> {
        let allowed = match self.role {
            Role::Host => ConsentOrigin::LocalUser,
            Role::Controller => ConsentOrigin::AuthenticatedRemote,
        };
        if origin == allowed {
            Ok(())
        } else {
            Err(self.reject(event, RejectReason::ConsentOriginNotPermitted))
        }
    }

    fn controls_input(&self) -> bool {
        self.role == Role::Host && self.granted == Some(Scope::ViewAndControl)
    }

    fn set_state_deadline(&mut self, timer: TimerKind, now: MonoTime, after: Duration) {
        self.state_deadline = Some((timer, now.saturating_add(after)));
    }

    fn enter_preparing(&mut self, now: MonoTime) -> Vec<Effect> {
        self.state = SessionState::Preparing;
        match self.role {
            Role::Host => {
                self.set_state_deadline(TimerKind::OfferExpiry, now, self.timeouts.offer);
                vec![Effect::OpenHostOffer]
            }
            Role::Controller => {
                self.set_state_deadline(TimerKind::PrepareTimeout, now, self.timeouts.prepare);
                vec![Effect::ConnectToHost]
            }
        }
    }

    fn enter_awaiting_consent(&mut self, now: MonoTime) -> Vec<Effect> {
        self.state = SessionState::AwaitingConsent;
        match self.role {
            Role::Host => {
                let after = self.timeouts.consent_decision;
                self.set_state_deadline(TimerKind::ConsentDecision, now, after);
                vec![Effect::PromptLocalConsent]
            }
            Role::Controller => {
                self.set_state_deadline(TimerKind::ConsentWait, now, self.timeouts.consent_wait);
                vec![Effect::AwaitConsentResult]
            }
        }
    }

    fn enter_connecting(&mut self, scope: Scope, now: MonoTime) -> Vec<Effect> {
        self.state = SessionState::Connecting;
        self.granted = Some(scope);
        self.set_state_deadline(TimerKind::ConnectTimeout, now, self.timeouts.connect);
        match self.role {
            Role::Host => {
                self.ttl_deadline = Some(now.saturating_add(self.timeouts.session_ttl));
                vec![Effect::StartCapture, Effect::StartTransport]
            }
            Role::Controller => vec![Effect::StartTransport],
        }
    }

    fn enter_connected(&mut self) -> Vec<Effect> {
        self.state = SessionState::Connected;
        self.link = LinkState::Up;
        self.state_deadline = None;
        let mut effects = vec![Effect::ShowSessionIndicator];
        if self.controls_input() {
            effects.push(Effect::OpenInputGate);
        }
        effects
    }

    fn enter_disconnecting(&mut self, reason: EndReason, now: MonoTime) -> Vec<Effect> {
        self.state = SessionState::Disconnecting;
        self.end_reason = Some(reason);
        self.outcome = Some(reason.outcome());
        self.granted = None;
        self.ttl_deadline = None;
        self.grace_deadline = None;
        self.set_state_deadline(TimerKind::TeardownWatchdog, now, self.timeouts.teardown);
        match self.role {
            Role::Host => HOST_TEARDOWN.to_vec(),
            Role::Controller => CONTROLLER_TEARDOWN.to_vec(),
        }
    }

    fn finish(&mut self) -> Vec<Effect> {
        let outcome = self.outcome.unwrap_or(Outcome::Error);
        self.state = match outcome {
            Outcome::Normal => SessionState::Disconnected,
            Outcome::Error => SessionState::Failed,
        };
        self.state_deadline = None;
        vec![Effect::ReportOutcome(outcome)]
    }
}
