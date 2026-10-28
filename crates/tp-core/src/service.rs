//! Session service: the session registry plus the security state of the current session,
//! behind the `SessionManager` contract.
//!
//! - Consent enters only through [`SessionManager::respond_consent`] with a
//!   `tp_security::LocalConsent`. There is no other way to make the Host's machine accept
//!   a consent decision, and BOOT-02 has no production `LocalConsent` constructor.
//! - Events from the network ([`LifecycleEvent`]) can never carry local consent. A
//!   consent decision arriving from the network on the Host is a protocol violation and
//!   ends the session (ARCH-00 §6.4).
//! - Every authorization goes through `tp_security::authorize` with the live state.
//!
//! No I/O happens here. Effects returned by the machine are logged, not executed: there
//! is no capture, transport or prompt to drive yet.

use tp_contracts::capability::{Capability, PlatformCapabilities};
use tp_contracts::error::{CoreError, ErrorCategory};
use tp_contracts::id::SessionRef;
use tp_contracts::session::{
    FaultKind, PeerByeReason, Role, SessionManager, SessionSnapshot, SessionState, StopKind,
};
use tp_contracts::time::MonoTime;
use tp_diagnostics::{Event as LogEvent, FieldValue, Logger};
use tp_security::{
    AuthenticatedPeer, AuthorizationContext, ConsentDecision, ConsentGrant, Decision, LocalConsent,
    authorize,
};
use tp_session::{
    AuditRecord, ConsentOrigin, Event, RegistryError, RejectReason, SessionIdSource,
    SessionRegistry, StopOutcome, Timeouts, Transition,
};

/// Lifecycle events reported by the (future) handshake and transport. None of them can
/// express a local consent decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleEvent {
    Prepared,
    /// The Host's authenticated decision, as received by a Controller.
    RemoteConsent(ConsentDecision),
    TransportReady,
    LinkDegraded,
    LinkRestored,
    PeerBye(PeerByeReason),
    Fault(FaultKind),
    TeardownComplete,
}

pub struct SessionService<I> {
    registry: SessionRegistry<I>,
    timeouts: Timeouts,
    peer: Option<AuthenticatedPeer>,
    grant: Option<ConsentGrant>,
    platform: PlatformCapabilities,
    log: Logger,
}

impl<I: SessionIdSource> SessionService<I> {
    pub fn new(ids: I, timeouts: Timeouts, platform: PlatformCapabilities, log: Logger) -> Self {
        SessionService {
            registry: SessionRegistry::new(ids, timeouts),
            timeouts,
            peer: None,
            grant: None,
            platform,
            log,
        }
    }

    /// Stores the handshake result for the current session (FUTURE: NET-03 produces it).
    pub fn record_authenticated_peer(&mut self, peer: AuthenticatedPeer) -> Result<(), CoreError> {
        let current = self.registry.current();
        let ok = current
            .as_ref()
            .is_some_and(|s| s.session == peer.session() && s.state == SessionState::Preparing);
        if !ok {
            self.log_refusal(peer.session(), "recordAuthenticatedPeer", "notPreparing");
            return Err(CoreError::new(
                ErrorCategory::InvalidState,
                "authentication result for a session that is not preparing",
            ));
        }
        self.peer = Some(peer);
        Ok(())
    }

    pub fn handle(
        &mut self,
        session: SessionRef,
        event: LifecycleEvent,
        now: MonoTime,
    ) -> Result<SessionSnapshot, CoreError> {
        let machine_event = match event {
            LifecycleEvent::Prepared => {
                // ARCH-00 §6.3 row 2: preparation completes only after authentication.
                if self.peer.as_ref().map(AuthenticatedPeer::session) != Some(session) {
                    self.log_refusal(session, "prepared", "notAuthenticated");
                    return Err(CoreError::new(
                        ErrorCategory::PermissionDenied,
                        "preparation requires an authenticated peer",
                    ));
                }
                Event::Prepared
            }
            LifecycleEvent::RemoteConsent(ConsentDecision::Allow(scope)) => Event::ConsentGranted {
                scope,
                origin: ConsentOrigin::AuthenticatedRemote,
            },
            LifecycleEvent::RemoteConsent(ConsentDecision::Deny) => Event::ConsentDenied {
                origin: ConsentOrigin::AuthenticatedRemote,
            },
            LifecycleEvent::TransportReady => Event::TransportReady,
            LifecycleEvent::LinkDegraded => Event::LinkDegraded,
            LifecycleEvent::LinkRestored => Event::LinkRestored,
            LifecycleEvent::PeerBye(reason) => Event::PeerBye(reason),
            LifecycleEvent::Fault(kind) => Event::Fault(kind),
            LifecycleEvent::TeardownComplete => Event::TeardownComplete,
        };
        match self.registry.apply(session, machine_event, now) {
            Ok(transition) => {
                self.log_transition(&transition);
                self.snapshot_of(session)
            }
            Err(RegistryError::Rejected(error))
                if error.reason == RejectReason::ConsentOriginNotPermitted =>
            {
                // A remote party tried to decide consent on the Host (§6.4).
                self.log_refusal(session, machine_event.name(), error.reason.as_str());
                let fault = Event::Fault(FaultKind::ProtocolViolation);
                if let Ok(transition) = self.registry.apply(session, fault, now) {
                    self.log_transition(&transition);
                }
                Err(error.into())
            }
            Err(error) => {
                self.log_refusal(session, machine_event.name(), error.as_str());
                Err(error.into())
            }
        }
    }

    /// Authorizes one capability for `session` at `now`. Denials are logged without any
    /// payload.
    pub fn authorize(
        &self,
        session: SessionRef,
        capability: Capability,
        now: MonoTime,
    ) -> Decision {
        self.authorize_event(session, capability, None, now)
    }

    /// Like [`Self::authorize`], recording the kind of event that asked (never its data).
    pub fn authorize_event(
        &self,
        session: SessionRef,
        capability: Capability,
        event_kind: Option<&'static str>,
        now: MonoTime,
    ) -> Decision {
        let snapshot = self.registry.current();
        let ctx = AuthorizationContext {
            session: snapshot.as_ref(),
            peer: self.peer.as_ref(),
            grant: self.grant.as_ref(),
            platform: self.platform,
        };
        let decision = authorize(&ctx, session, capability, now);
        if let Decision::Deny(reason) = decision {
            let mut line = LogEvent::warn("authz.denied")
                .field("session", FieldValue::Session(session))
                .field("capability", FieldValue::Static(capability.as_str()))
                .field("reason", FieldValue::Static(reason.as_str()));
            if let Some(kind) = event_kind {
                line = line.field("event_kind", FieldValue::Static(kind));
            }
            self.log.log(&line);
        }
        decision
    }

    pub fn current(&self) -> Option<SessionSnapshot> {
        self.registry.current()
    }

    pub fn platform(&self) -> PlatformCapabilities {
        self.platform
    }

    /// Whether the current session's consent grant has been revoked.
    pub fn grant_revoked(&self, session: SessionRef) -> Option<bool> {
        self.grant
            .as_ref()
            .filter(|g| g.session() == session)
            .map(ConsentGrant::is_revoked)
    }

    pub fn audit(&self) -> Vec<AuditRecord> {
        self.registry.audit().copied().collect()
    }

    fn snapshot_of(&self, session: SessionRef) -> Result<SessionSnapshot, CoreError> {
        self.registry.snapshot(session).ok_or(CoreError::new(
            ErrorCategory::InternalError,
            "snapshot missing after transition",
        ))
    }

    fn log_transition(&self, t: &Transition) {
        self.log.log(
            &LogEvent::info("session.transition")
                .field("session", FieldValue::Session(t.session))
                .field("from", FieldValue::Static(t.from.as_str()))
                .field("to", FieldValue::Static(t.to.as_str()))
                .field("cause", FieldValue::Static(t.cause.name()))
                .field("effects", FieldValue::U64(t.effects.len() as u64)),
        );
    }

    fn log_refusal(&self, session: SessionRef, cause: &'static str, reason: &'static str) {
        let state = self.registry.current().map_or("none", |s| s.state.as_str());
        self.log.log(
            &LogEvent::warn("session.rejected")
                .field("session", FieldValue::Session(session))
                .field("state", FieldValue::Static(state))
                .field("cause", FieldValue::Static(cause))
                .field("reason", FieldValue::Static(reason)),
        );
    }
}

impl<I: SessionIdSource> SessionManager for SessionService<I> {
    type ConsentProof = LocalConsent;

    fn begin(
        &mut self,
        role: Role,
        requested: tp_contracts::capability::Scope,
        now: MonoTime,
    ) -> Result<SessionRef, CoreError> {
        match self.registry.begin(role, requested, now) {
            Ok(transition) => {
                self.peer = None;
                self.grant = None;
                self.log_transition(&transition);
                Ok(transition.session)
            }
            Err(error) => {
                if let RegistryError::Busy { active } = error {
                    self.log_refusal(active, "begin", error.as_str());
                }
                Err(error.into())
            }
        }
    }

    fn respond_consent(
        &mut self,
        session: SessionRef,
        proof: LocalConsent,
        now: MonoTime,
    ) -> Result<SessionSnapshot, CoreError> {
        if proof.session() != session {
            self.log_refusal(session, "respondConsent", "proofForAnotherSession");
            return Err(CoreError::new(
                ErrorCategory::InvalidState,
                "consent was given for another session",
            ));
        }
        let Some(snapshot) = self.registry.snapshot(session) else {
            self.log_refusal(session, "respondConsent", "staleSession");
            return Err(CoreError::new(
                ErrorCategory::InvalidState,
                "session is not current",
            ));
        };
        if snapshot.role != Role::Host {
            self.log_refusal(session, "respondConsent", "notHost");
            return Err(CoreError::new(
                ErrorCategory::PermissionDenied,
                "only the Host user decides consent",
            ));
        }
        let (event, grant) = match proof.decision() {
            ConsentDecision::Deny => (
                Event::ConsentDenied {
                    origin: ConsentOrigin::LocalUser,
                },
                None,
            ),
            ConsentDecision::Allow(scope) => {
                let Some(peer) = self.peer.as_ref().filter(|p| p.session() == session) else {
                    self.log_refusal(session, "respondConsent", "notAuthenticated");
                    return Err(CoreError::new(
                        ErrorCategory::PermissionDenied,
                        "consent requires an authenticated peer",
                    ));
                };
                // The grant expires with the session TTL that starts now. It is built
                // before the state changes, so a refused grant leaves the session as is.
                let expires_at = now.saturating_add(self.timeouts.session_ttl);
                let grant =
                    ConsentGrant::from_local(&proof, peer, expires_at).map_err(|reason| {
                        self.log_refusal(session, "respondConsent", reason.as_str());
                        CoreError::from(reason)
                    })?;
                (
                    Event::ConsentGranted {
                        scope,
                        origin: ConsentOrigin::LocalUser,
                    },
                    Some(grant),
                )
            }
        };
        match self.registry.apply(session, event, now) {
            Ok(transition) => {
                self.log_transition(&transition);
                self.grant = grant;
                self.snapshot_of(session)
            }
            Err(error) => {
                self.log_refusal(session, event.name(), error.as_str());
                Err(error.into())
            }
        }
    }

    fn disconnect(
        &mut self,
        session: SessionRef,
        kind: StopKind,
        now: MonoTime,
    ) -> Result<SessionSnapshot, CoreError> {
        // Revoke first: authorization fails from this instant, whatever happens next.
        if let Some(grant) = self.grant.as_mut().filter(|g| g.session() == session) {
            grant.revoke();
        }
        match self.registry.stop(session, kind, now) {
            Ok(StopOutcome::Started(transition)) => {
                self.log_transition(&transition);
                self.snapshot_of(session)
            }
            Ok(StopOutcome::AlreadyStopping(snapshot) | StopOutcome::AlreadyEnded(snapshot)) => {
                Ok(snapshot)
            }
            Err(error) => {
                self.log_refusal(session, "localStop", error.as_str());
                Err(error.into())
            }
        }
    }

    fn tick(&mut self, now: MonoTime) -> Result<Option<SessionSnapshot>, CoreError> {
        for transition in self.registry.tick(now) {
            self.log_transition(&transition);
        }
        Ok(self.registry.current())
    }

    fn snapshot(&self, session: SessionRef) -> Option<SessionSnapshot> {
        self.registry.snapshot(session)
    }
}
