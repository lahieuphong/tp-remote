//! The input gate: the only code allowed to call `InputController::inject`
//! (enforced by tp-test-harness/tests/workspace_boundaries.rs).
//!
//! Every event is authorized afresh against the live session state; no decision is
//! cached, so a revocation, expiry or link loss takes effect on the very next event.
//! No production `InputController` exists in BOOT-02, and the production platform
//! advertises `ControlInput` as not implemented, so the gate denies everything there.

use tp_contracts::capability::Capability;
use tp_contracts::error::CoreError;
use tp_contracts::id::SessionRef;
use tp_contracts::input::{InjectableEvent, InputController};
use tp_contracts::time::MonoTime;
use tp_security::Decision;
use tp_session::SessionIdSource;

use crate::service::SessionService;

pub struct InputGate<C> {
    controller: C,
    injected: u64,
    denied: u64,
}

impl<C: InputController> InputGate<C> {
    /// Takes exclusive ownership of the controller.
    pub fn new(controller: C) -> Self {
        InputGate {
            controller,
            injected: 0,
            denied: 0,
        }
    }

    /// Authorizes `event` for `session` at `now` and injects it only if allowed.
    pub fn inject<I: SessionIdSource>(
        &mut self,
        service: &SessionService<I>,
        session: SessionRef,
        event: &InjectableEvent,
        now: MonoTime,
    ) -> Result<(), CoreError> {
        let decision =
            service.authorize_event(session, Capability::ControlInput, Some(event.kind()), now);
        match decision {
            Decision::Allow(_) => {
                self.controller.inject(event)?;
                self.injected += 1;
                Ok(())
            }
            Decision::Deny(reason) => {
                self.denied += 1;
                Err(reason.into())
            }
        }
    }

    /// Releases all pressed keys and buttons (teardown step 2). Never needs authorization.
    pub fn release_all(&mut self) -> Result<(), CoreError> {
        self.controller.release_all()
    }

    pub fn injected(&self) -> u64 {
        self.injected
    }

    pub fn denied(&self) -> u64 {
        self.denied
    }

    pub fn controller(&self) -> &C {
        &self.controller
    }
}
