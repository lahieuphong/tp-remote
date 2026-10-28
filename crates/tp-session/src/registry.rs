//! The session registry: at most one active session per device (D-09), a strictly
//! increasing generation per session, and rejection of every event that does not target
//! the current session. It also keeps a bounded, payload-free audit trail.

use std::collections::VecDeque;
use std::fmt;

use tp_contracts::capability::Scope;
use tp_contracts::error::{CoreError, ErrorCategory};
use tp_contracts::id::{Generation, SessionId, SessionRef};
use tp_contracts::session::{Role, SessionSnapshot, SessionState, StopKind};
use tp_contracts::time::MonoTime;

use crate::machine::{Event, SessionMachine, Timeouts, Transition, TransitionError};

/// Source of new session identifiers. Production sources must be a CSPRNG (FUTURE).
pub trait SessionIdSource {
    fn next_session_id(&mut self) -> SessionId;
}

/// Deterministic identifiers 00…01, 00…02, … for tests only.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Default)]
pub struct SequentialIds {
    counter: u64,
}

#[cfg(any(test, feature = "test-support"))]
impl SessionIdSource for SequentialIds {
    fn next_session_id(&mut self) -> SessionId {
        self.counter = self.counter.wrapping_add(1).max(1);
        let mut bytes = [0u8; 16];
        bytes[8..].copy_from_slice(&self.counter.to_be_bytes());
        match SessionId::from_bytes(bytes) {
            Ok(id) => id,
            Err(_) => unreachable!("the counter is never zero"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryError {
    /// Another session is still active.
    Busy {
        active: SessionRef,
    },
    NoSession,
    /// The event targets a session other than the current one (stale id or generation).
    StaleSession {
        current: SessionRef,
    },
    Rejected(TransitionError),
    IdReused,
    GenerationExhausted,
    Poisoned,
}

impl RegistryError {
    pub const fn as_str(&self) -> &'static str {
        match self {
            RegistryError::Busy { .. } => "busy",
            RegistryError::NoSession => "noSession",
            RegistryError::StaleSession { .. } => "staleSession",
            RegistryError::Rejected(error) => error.reason.as_str(),
            RegistryError::IdReused => "idReused",
            RegistryError::GenerationExhausted => "generationExhausted",
            RegistryError::Poisoned => "poisoned",
        }
    }
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegistryError::Rejected(error) => error.fmt(f),
            other => f.write_str(other.as_str()),
        }
    }
}

impl std::error::Error for RegistryError {}

impl From<RegistryError> for CoreError {
    fn from(error: RegistryError) -> CoreError {
        match error {
            RegistryError::Busy { .. } => {
                CoreError::new(ErrorCategory::InvalidState, "another session is active")
            }
            RegistryError::NoSession => CoreError::new(ErrorCategory::InvalidState, "no session"),
            RegistryError::StaleSession { .. } => CoreError::new(
                ErrorCategory::InvalidState,
                "event for a session that is not current",
            ),
            RegistryError::Rejected(error) => error.into(),
            RegistryError::IdReused => {
                CoreError::new(ErrorCategory::InternalError, "session identifier reused")
            }
            RegistryError::GenerationExhausted => CoreError::new(
                ErrorCategory::InternalError,
                "session generations exhausted",
            ),
            RegistryError::Poisoned => {
                CoreError::new(ErrorCategory::InternalError, "session registry poisoned")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopOutcome {
    /// The session entered `Disconnecting` now.
    Started(Transition),
    /// It was already tearing down; nothing changed.
    AlreadyStopping(SessionSnapshot),
    /// It had already ended; nothing changed.
    AlreadyEnded(SessionSnapshot),
}

/// Result of one attempted transition. Contains names and identifiers only, never
/// payloads or secrets, so it is safe to log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditRecord {
    pub at: MonoTime,
    /// The session the event targeted (which may be stale).
    pub session: SessionRef,
    pub from: Option<SessionState>,
    pub to: Option<SessionState>,
    pub cause: &'static str,
    pub verdict: AuditVerdict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditVerdict {
    Applied,
    Rejected(&'static str),
}

/// Number of audit records kept (oldest dropped first).
pub const AUDIT_CAPACITY: usize = 256;

pub struct SessionRegistry<I> {
    ids: I,
    timeouts: Timeouts,
    last_generation: Option<Generation>,
    last_id: Option<SessionId>,
    current: Option<SessionMachine>,
    audit: VecDeque<AuditRecord>,
}

impl<I: SessionIdSource> SessionRegistry<I> {
    pub fn new(ids: I, timeouts: Timeouts) -> Self {
        SessionRegistry {
            ids,
            timeouts,
            last_generation: None,
            last_id: None,
            current: None,
            audit: VecDeque::with_capacity(AUDIT_CAPACITY),
        }
    }

    /// Starts a new session (Idle → Preparing) with a fresh identifier and generation.
    pub fn begin(
        &mut self,
        role: Role,
        requested: Scope,
        now: MonoTime,
    ) -> Result<Transition, RegistryError> {
        if let Some(active) = self.current.as_ref().filter(|m| !m.state().is_terminal()) {
            return Err(RegistryError::Busy {
                active: active.session(),
            });
        }
        let generation = match self.last_generation {
            None => Generation::FIRST,
            Some(last) => last.next().ok_or(RegistryError::GenerationExhausted)?,
        };
        let id = self.ids.next_session_id();
        if self.last_id == Some(id) {
            return Err(RegistryError::IdReused);
        }
        let session = SessionRef { id, generation };
        let mut machine = SessionMachine::new(session, role, requested, self.timeouts);
        let transition = machine
            .apply(Event::Start, now)
            .map_err(RegistryError::Rejected)?;
        self.last_generation = Some(generation);
        self.last_id = Some(id);
        self.current = Some(machine);
        self.record(now, &transition);
        Ok(transition)
    }

    /// Applies an event to `target`, which must be the current session.
    pub fn apply(
        &mut self,
        target: SessionRef,
        event: Event,
        now: MonoTime,
    ) -> Result<Transition, RegistryError> {
        let result = self
            .resolve(target)
            .and_then(|machine| machine.apply(event, now).map_err(RegistryError::Rejected));
        match &result {
            Ok(transition) => self.record(now, transition),
            Err(error) => self.record_rejection(now, target, event.name(), *error),
        }
        result
    }

    /// Stops `target`. Idempotent for the current session; a stale `target` is rejected
    /// so a late stop for an old session can never stop a newer one.
    pub fn stop(
        &mut self,
        target: SessionRef,
        kind: StopKind,
        now: MonoTime,
    ) -> Result<StopOutcome, RegistryError> {
        let state = match self.resolve(target) {
            Ok(machine) => machine.state(),
            Err(error) => {
                self.record_rejection(now, target, "localStop", error);
                return Err(error);
            }
        };
        if state.is_terminal() || state == SessionState::Disconnecting {
            let snapshot = self.current_snapshot_or_err()?;
            return Ok(if state.is_terminal() {
                StopOutcome::AlreadyEnded(snapshot)
            } else {
                StopOutcome::AlreadyStopping(snapshot)
            });
        }
        self.apply(target, Event::LocalStop(kind), now)
            .map(StopOutcome::Started)
    }

    /// Fires every due deadline of the current session.
    pub fn tick(&mut self, now: MonoTime) -> Vec<Transition> {
        let mut fired = Vec::new();
        while let Some(transition) = self.current.as_mut().and_then(|m| m.tick(now)) {
            self.record(now, &transition);
            fired.push(transition);
        }
        fired
    }

    /// The snapshot of `target`, only if it is the current session.
    pub fn snapshot(&self, target: SessionRef) -> Option<SessionSnapshot> {
        self.current
            .as_ref()
            .filter(|m| m.session() == target)
            .map(SessionMachine::snapshot)
    }

    pub fn current(&self) -> Option<SessionSnapshot> {
        self.current.as_ref().map(SessionMachine::snapshot)
    }

    /// Lifecycle part of the input gate for `target` (false for any other session).
    pub fn input_allowed(&self, target: SessionRef, now: MonoTime) -> bool {
        self.current
            .as_ref()
            .is_some_and(|m| m.session() == target && m.input_allowed(now))
    }

    pub fn next_deadline(&self) -> Option<MonoTime> {
        self.current
            .as_ref()
            .and_then(SessionMachine::next_deadline)
            .map(|(_, at)| at)
    }

    /// Audit trail, oldest first.
    pub fn audit(&self) -> impl Iterator<Item = &AuditRecord> {
        self.audit.iter()
    }

    fn resolve(&mut self, target: SessionRef) -> Result<&mut SessionMachine, RegistryError> {
        let machine = self.current.as_mut().ok_or(RegistryError::NoSession)?;
        if machine.session() != target {
            return Err(RegistryError::StaleSession {
                current: machine.session(),
            });
        }
        Ok(machine)
    }

    fn current_snapshot_or_err(&self) -> Result<SessionSnapshot, RegistryError> {
        self.current().ok_or(RegistryError::NoSession)
    }

    fn push(&mut self, record: AuditRecord) {
        if self.audit.len() == AUDIT_CAPACITY {
            self.audit.pop_front();
        }
        self.audit.push_back(record);
    }

    fn record(&mut self, at: MonoTime, transition: &Transition) {
        self.push(AuditRecord {
            at,
            session: transition.session,
            from: Some(transition.from),
            to: Some(transition.to),
            cause: transition.cause.name(),
            verdict: AuditVerdict::Applied,
        });
    }

    fn record_rejection(
        &mut self,
        at: MonoTime,
        target: SessionRef,
        cause: &'static str,
        error: RegistryError,
    ) {
        let state = self.current.as_ref().map(SessionMachine::state);
        self.push(AuditRecord {
            at,
            session: target,
            from: state,
            to: state,
            cause,
            verdict: AuditVerdict::Rejected(error.as_str()),
        });
    }
}
