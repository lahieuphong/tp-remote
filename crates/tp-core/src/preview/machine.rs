//! Local preview lifecycle: a pure, deterministic state machine (ADR-007 §4).
//!
//! It is deliberately separate from the remote session machine in `tp-session`: a local
//! preview has no peer, no consent and no transport.
//!
//! ```text
//! NotAvailable (terminal)
//! Ready | Stopped | Failed --request_start--> SelectingTarget(g+1)
//! SelectingTarget(g) --cancelled(g)--> Ready            (nothing was captured)
//! SelectingTarget(g) --picker_failed(g)--> Failed
//! SelectingTarget(g) --selected(g)--> Initializing(g)
//! SelectingTarget(g) --stop--> Stopped                  (a late selection is stale)
//! Initializing(g) --started(g)--> Capturing(g)
//! Initializing(g) --start_failed(g)--> Failed
//! Initializing(g) | Capturing(g) --stop / stream_ended(g)--> Stopping(g)
//! Stopping(g) --stopped(g)--> Stopped | Failed          (by the recorded end reason)
//! ```
//! Every event for another generation, or for a state that does not expect it, is
//! rejected and counted. `stop` is idempotent.

use tp_contracts::error::CoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PreviewState {
    NotAvailable,
    Ready,
    SelectingTarget,
    Initializing,
    Capturing,
    Stopping,
    Stopped,
    Failed,
}

impl PreviewState {
    pub const ALL: [PreviewState; 8] = [
        PreviewState::NotAvailable,
        PreviewState::Ready,
        PreviewState::SelectingTarget,
        PreviewState::Initializing,
        PreviewState::Capturing,
        PreviewState::Stopping,
        PreviewState::Stopped,
        PreviewState::Failed,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            PreviewState::NotAvailable => "notAvailable",
            PreviewState::Ready => "ready",
            PreviewState::SelectingTarget => "selectingTarget",
            PreviewState::Initializing => "initializing",
            PreviewState::Capturing => "capturing",
            PreviewState::Stopping => "stopping",
            PreviewState::Stopped => "stopped",
            PreviewState::Failed => "failed",
        }
    }

    /// A capture session may exist (or is being created) in this state.
    pub const fn may_capture(self) -> bool {
        matches!(
            self,
            PreviewState::Initializing | PreviewState::Capturing | PreviewState::Stopping
        )
    }
}

/// Why a preview run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    /// The picker was closed without a selection.
    Cancelled,
    StoppedByUser,
    TargetClosed,
    DeviceLost,
    Failed(CoreError),
}

impl EndReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            EndReason::Cancelled => "cancelled",
            EndReason::StoppedByUser => "stoppedByUser",
            EndReason::TargetClosed => "targetClosed",
            EndReason::DeviceLost => "deviceLost",
            EndReason::Failed(_) => "failed",
        }
    }

    const fn final_state(self) -> PreviewState {
        match self {
            EndReason::Cancelled => PreviewState::Ready,
            EndReason::StoppedByUser | EndReason::TargetClosed => PreviewState::Stopped,
            EndReason::DeviceLost | EndReason::Failed(_) => PreviewState::Failed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reject {
    /// The capture platform is not available; nothing can start.
    NotAvailable,
    /// A preview is already being selected, started, running or stopped.
    Busy,
    /// The event belongs to an older request.
    StaleGeneration,
    /// The event does not apply in the current state.
    UnexpectedState(PreviewState),
}

impl Reject {
    pub const fn as_str(self) -> &'static str {
        match self {
            Reject::NotAvailable => "notAvailable",
            Reject::Busy => "busy",
            Reject::StaleGeneration => "staleGeneration",
            Reject::UnexpectedState(_) => "unexpectedState",
        }
    }
}

/// What the caller must do after [`PreviewMachine::stop`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopAction {
    /// Nothing is running; nothing to release.
    None,
    /// Stop the stream of this generation (if one exists) and then call `stopped`.
    StopStream { generation: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewMachine {
    state: PreviewState,
    generation: u32,
    last_end: Option<EndReason>,
    pending_end: Option<EndReason>,
    rejected: u32,
}

impl PreviewMachine {
    pub fn new(available: bool) -> PreviewMachine {
        PreviewMachine {
            state: if available {
                PreviewState::Ready
            } else {
                PreviewState::NotAvailable
            },
            generation: 0,
            last_end: None,
            pending_end: None,
            rejected: 0,
        }
    }

    pub fn state(&self) -> PreviewState {
        self.state
    }

    /// Number of the current (or last) request. 0 before the first request.
    pub fn generation(&self) -> u32 {
        self.generation
    }

    pub fn last_end(&self) -> Option<EndReason> {
        self.last_end
    }

    /// Events rejected so far (stale or out of order).
    pub fn rejected(&self) -> u32 {
        self.rejected
    }

    fn reject(&mut self, reason: Reject) -> Reject {
        self.rejected = self.rejected.saturating_add(1);
        reason
    }

    fn expect(&mut self, generation: u32, state: PreviewState) -> Result<(), Reject> {
        if generation != self.generation {
            return Err(self.reject(Reject::StaleGeneration));
        }
        if self.state != state {
            return Err(self.reject(Reject::UnexpectedState(self.state)));
        }
        Ok(())
    }

    /// The local user asked to start a preview: the picker is about to open.
    pub fn request_start(&mut self) -> Result<u32, Reject> {
        match self.state {
            PreviewState::NotAvailable => Err(self.reject(Reject::NotAvailable)),
            PreviewState::Ready | PreviewState::Stopped | PreviewState::Failed => {
                self.generation = self.generation.wrapping_add(1).max(1);
                self.state = PreviewState::SelectingTarget;
                self.pending_end = None;
                Ok(self.generation)
            }
            _ => Err(self.reject(Reject::Busy)),
        }
    }

    pub fn cancelled(&mut self, generation: u32) -> Result<(), Reject> {
        self.expect(generation, PreviewState::SelectingTarget)?;
        self.finish(EndReason::Cancelled);
        Ok(())
    }

    pub fn picker_failed(&mut self, generation: u32, error: CoreError) -> Result<(), Reject> {
        self.expect(generation, PreviewState::SelectingTarget)?;
        self.finish(EndReason::Failed(error));
        Ok(())
    }

    /// The user selected a target for this request: capture may now be created.
    pub fn selected(&mut self, generation: u32) -> Result<(), Reject> {
        self.expect(generation, PreviewState::SelectingTarget)?;
        self.state = PreviewState::Initializing;
        Ok(())
    }

    pub fn started(&mut self, generation: u32) -> Result<(), Reject> {
        self.expect(generation, PreviewState::Initializing)?;
        self.state = PreviewState::Capturing;
        Ok(())
    }

    /// Starting failed. If a stop arrived meanwhile, the run simply ends as stopped.
    pub fn start_failed(&mut self, generation: u32, error: CoreError) -> Result<(), Reject> {
        if generation == self.generation && self.state == PreviewState::Stopping {
            let reason = self.pending_end.take().unwrap_or(EndReason::StoppedByUser);
            self.finish(reason);
            return Ok(());
        }
        self.expect(generation, PreviewState::Initializing)?;
        self.finish(EndReason::Failed(error));
        Ok(())
    }

    /// The local user (or app shutdown) stops the preview. Always allowed.
    pub fn stop(&mut self) -> StopAction {
        match self.state {
            PreviewState::SelectingTarget => {
                // The picker cannot be closed from here; its result will be stale.
                self.finish(EndReason::StoppedByUser);
                StopAction::None
            }
            PreviewState::Initializing | PreviewState::Capturing => {
                self.state = PreviewState::Stopping;
                self.pending_end = Some(EndReason::StoppedByUser);
                StopAction::StopStream {
                    generation: self.generation,
                }
            }
            _ => StopAction::None,
        }
    }

    /// The provider ended the run (target closed, device lost, failure).
    pub fn stream_ended(
        &mut self,
        generation: u32,
        reason: EndReason,
    ) -> Result<StopAction, Reject> {
        self.expect(generation, PreviewState::Capturing)?;
        self.state = PreviewState::Stopping;
        self.pending_end = Some(reason);
        Ok(StopAction::StopStream { generation })
    }

    /// The stream of `generation` has been released.
    pub fn stopped(&mut self, generation: u32) -> Result<(), Reject> {
        self.expect(generation, PreviewState::Stopping)?;
        let reason = self.pending_end.take().unwrap_or(EndReason::StoppedByUser);
        self.finish(reason);
        Ok(())
    }

    fn finish(&mut self, reason: EndReason) {
        self.state = reason.final_state();
        self.last_end = Some(reason);
        self.pending_end = None;
    }
}
