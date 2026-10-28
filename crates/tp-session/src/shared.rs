//! Thread-safe access to a registry. Every operation takes the lock once, so concurrent
//! events are applied one at a time in a total order; none can observe a half-applied
//! transition.

use std::sync::{Arc, Mutex, MutexGuard};

use tp_contracts::capability::Scope;
use tp_contracts::id::SessionRef;
use tp_contracts::session::{Role, SessionSnapshot, StopKind};
use tp_contracts::time::MonoTime;

use crate::machine::{Event, Transition};
use crate::registry::{AuditRecord, RegistryError, SessionIdSource, SessionRegistry, StopOutcome};

pub struct SharedRegistry<I> {
    inner: Arc<Mutex<SessionRegistry<I>>>,
}

impl<I> Clone for SharedRegistry<I> {
    fn clone(&self) -> Self {
        SharedRegistry {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<I: SessionIdSource> SharedRegistry<I> {
    pub fn new(registry: SessionRegistry<I>) -> Self {
        SharedRegistry {
            inner: Arc::new(Mutex::new(registry)),
        }
    }

    // A poisoned lock means a thread panicked mid-operation; the state can no longer be
    // trusted, so every further operation fails instead of continuing.
    fn lock(&self) -> Result<MutexGuard<'_, SessionRegistry<I>>, RegistryError> {
        self.inner.lock().map_err(|_| RegistryError::Poisoned)
    }

    pub fn begin(
        &self,
        role: Role,
        requested: Scope,
        now: MonoTime,
    ) -> Result<Transition, RegistryError> {
        self.lock()?.begin(role, requested, now)
    }

    pub fn apply(
        &self,
        target: SessionRef,
        event: Event,
        now: MonoTime,
    ) -> Result<Transition, RegistryError> {
        self.lock()?.apply(target, event, now)
    }

    pub fn stop(
        &self,
        target: SessionRef,
        kind: StopKind,
        now: MonoTime,
    ) -> Result<StopOutcome, RegistryError> {
        self.lock()?.stop(target, kind, now)
    }

    pub fn tick(&self, now: MonoTime) -> Result<Vec<Transition>, RegistryError> {
        Ok(self.lock()?.tick(now))
    }

    pub fn current(&self) -> Result<Option<SessionSnapshot>, RegistryError> {
        Ok(self.lock()?.current())
    }

    pub fn audit(&self) -> Result<Vec<AuditRecord>, RegistryError> {
        Ok(self.lock()?.audit().copied().collect())
    }
}
