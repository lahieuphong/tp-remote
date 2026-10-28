//! Local self-preview (CAP-02A, ADR-007): the local user previews their own screen inside
//! TP Remote. Nothing here involves a peer, the network or input.
//!
//! Security model:
//! - An IPC request only *opens the Windows capture picker*. It is not consent, and it
//!   cannot name a target: targets are opaque OS objects that only the picker produces.
//! - A [`LocalPreviewGrant`] is minted only here, only when the picker returns a
//!   selection for the current request generation. It authorizes local preview frames of
//!   that generation and nothing else. It is not a `tp_security::ConsentGrant`, cannot
//!   be converted into one, and the remote policy (`tp_security::authorize`)
//!   cannot accept it, so it can never authorize remote viewing or input:
//!
//!   ```compile_fail
//!   fn widen(grant: tp_core::preview::LocalPreviewGrant) -> tp_security::ConsentGrant {
//!       grant.into()
//!   }
//!   ```
//!   ```compile_fail
//!   fn copy(grant: &tp_core::preview::LocalPreviewGrant) -> tp_core::preview::LocalPreviewGrant {
//!       grant.clone()
//!   }
//!   ```
//!   ```compile_fail
//!   // Fails only because the fields are private: grants cannot be forged.
//!   let forged = tp_core::preview::LocalPreviewGrant {
//!       generation: 1,
//!       granted_at: tp_contracts::time::MonoTime::ZERO,
//!   };
//!   ```
//! - Cancelling the picker produces no target and no capture. A selection that arrives
//!   after a stop or for an older request is stale: its target is released uncaptured.
//! - Stopping revokes the grant, clears the preview buffer and discards late frames.
//! - The Windows yellow capture border stays on (see `tp_capture::wgc`).

mod host;
pub mod machine;

pub use host::{LocalPreview, PreviewConfig, StartError};
pub use machine::{EndReason, PreviewMachine, PreviewState, Reject, StopAction};

use tp_contracts::time::MonoTime;

/// Authorization for the local preview of one picker-selected target, for one request
/// generation. Minted only by this module on a picker selection; revoked by dropping it
/// when the run stops. Deliberately not `Clone`, not serializable and not convertible.
#[derive(Debug, PartialEq, Eq)]
pub struct LocalPreviewGrant {
    generation: u32,
    granted_at: MonoTime,
}

impl LocalPreviewGrant {
    /// Only the picker-completion path in `host.rs` calls this.
    fn on_picker_selection(generation: u32, granted_at: MonoTime) -> LocalPreviewGrant {
        LocalPreviewGrant {
            generation,
            granted_at,
        }
    }

    pub fn generation(&self) -> u32 {
        self.generation
    }

    pub fn granted_at(&self) -> MonoTime {
        self.granted_at
    }

    /// Whether a preview frame of `generation` may be shown under this grant.
    pub fn permits_frame(&self, generation: u32) -> bool {
        generation == self.generation
    }
}
