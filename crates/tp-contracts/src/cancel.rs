//! Cooperative cancellation.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A cancellation flag shared between the owner of some work and the work itself.
///
/// Cancelling a token also cancels every token derived from it with [`CancelToken::child`];
/// cancelling a child does not affect its parent. Work must check the token at its own
/// safe points and then release its resources; cancellation never interrupts code.
#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
    parent: Option<Box<CancelToken>>,
}

impl CancelToken {
    pub fn new() -> CancelToken {
        CancelToken::default()
    }

    /// A token that is cancelled when either it or `self` is cancelled.
    pub fn child(&self) -> CancelToken {
        CancelToken {
            flag: Arc::new(AtomicBool::new(false)),
            parent: Some(Box::new(self.clone())),
        }
    }

    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Acquire) || self.parent.as_ref().is_some_and(|p| p.is_cancelled())
    }
}
