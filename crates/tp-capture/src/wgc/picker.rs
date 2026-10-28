//! The Windows capture picker: the trusted local selection boundary (ADR-007).
//!
//! Windows draws the picker itself; the WebView cannot see or operate it, and only a
//! selection by the local user produces a target. Cancelling produces none.

use std::panic::{AssertUnwindSafe, catch_unwind};

use tp_contracts::capture::{PickDone, PickOutcome, PickerOwner};
use tp_contracts::error::{CoreError, ErrorCategory};
use windows::Graphics::Capture::{GraphicsCaptureItem, GraphicsCapturePicker};
use windows::Win32::Foundation::{E_POINTER, ERROR_CANCELLED, HWND};
use windows::Win32::UI::Shell::IInitializeWithWindow;
use windows::Win32::UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow};
use windows::core::{HRESULT, Interface};

use super::target::WgcTarget;
use super::{is_supported, os_error};

const FOREIGN_OWNER: CoreError = CoreError::new(
    ErrorCategory::PermissionDenied,
    "the picker owner is not a live window of this process",
);

/// The picker is only ever shown over a live window that belongs to this process.
pub(crate) fn verify_owner(owner: PickerOwner) -> Result<HWND, CoreError> {
    let hwnd = HWND(owner.0 as *mut core::ffi::c_void);
    if hwnd.is_invalid() {
        return Err(FOREIGN_OWNER);
    }
    // SAFETY: IsWindow and GetWindowThreadProcessId accept any handle value and only
    // read; `pid` is a valid out-pointer.
    let (alive, pid) = unsafe {
        let alive = IsWindow(Some(hwnd)).as_bool();
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        (alive, pid)
    };
    if alive && pid == std::process::id() {
        Ok(hwnd)
    } else {
        Err(FOREIGN_OWNER)
    }
}

pub(crate) fn pick(owner: PickerOwner, done: PickDone<WgcTarget>) -> Result<(), CoreError> {
    let hwnd = verify_owner(owner)?;
    if !is_supported() {
        return Err(CoreError::new(
            ErrorCategory::PlatformUnavailable,
            "Windows.Graphics.Capture is not supported on this system",
        ));
    }
    let failed = || {
        os_error(
            ErrorCategory::PlatformUnavailable,
            "opening the capture picker failed",
        )
    };
    let picker = GraphicsCapturePicker::new().map_err(failed())?;
    let init: IInitializeWithWindow = picker.cast().map_err(failed())?;
    // SAFETY: `hwnd` was verified above to be a live window of this process; Initialize
    // only records it as the picker's owner window.
    unsafe { init.Initialize(hwnd) }.map_err(failed())?;
    let operation = picker.PickSingleItemAsync().map_err(failed())?;

    let keep_alive = picker.clone();
    operation
        .when(move |result: windows::core::Result<GraphicsCaptureItem>| {
            let _ = &keep_alive;
            let outcome = outcome_of(result);
            // Unwinding must not cross into WinRT.
            let _ = catch_unwind(AssertUnwindSafe(move || done(outcome)));
        })
        .map_err(failed())
}

fn outcome_of(result: windows::core::Result<GraphicsCaptureItem>) -> PickOutcome<WgcTarget> {
    match result {
        Ok(item) => PickOutcome::Selected(WgcTarget::from_picker(item)),
        // Closing the picker without a selection completes with a null item; an
        // operation cancelled by the system reports ERROR_CANCELLED.
        Err(error)
            if error.code().is_ok()
                || error.code() == E_POINTER
                || error.code() == HRESULT::from_win32(ERROR_CANCELLED.0) =>
        {
            PickOutcome::Cancelled
        }
        Err(error) => PickOutcome::Failed(os_error(
            ErrorCategory::PlatformUnavailable,
            "the capture picker failed",
        )(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foreign_or_invalid_owners_are_rejected_before_any_picker_exists() {
        // Null, a fabricated handle (what Tauri's mock runtime reports) and the desktop
        // window (alive, but owned by another process).
        // SAFETY: GetDesktopWindow has no preconditions.
        let desktop = unsafe { windows::Win32::UI::WindowsAndMessaging::GetDesktopWindow() };
        for owner in [0, isize::MIN, 0x1234_5678, desktop.0 as isize] {
            assert_eq!(
                verify_owner(PickerOwner(owner)),
                Err(FOREIGN_OWNER),
                "{owner:#x}"
            );
            let result = pick(
                PickerOwner(owner),
                Box::new(|_| panic!("must not complete")),
            );
            assert_eq!(result, Err(FOREIGN_OWNER));
        }
    }
}
