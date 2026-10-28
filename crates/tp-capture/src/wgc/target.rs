//! Capture targets. In production a [`WgcTarget`] can only come from the system picker
//! (`picker.rs`); the programmatic constructors in `testing.rs` exist only with the
//! `test-support` feature, which the desktop app never enables.

use std::fmt;

use tp_contracts::capture::{PixelSize, TargetInfo, TargetKind};
use tp_contracts::error::{CoreError, ErrorCategory};
use windows::Graphics::Capture::GraphicsCaptureItem;
use windows::Win32::Graphics::Gdi::HMONITOR;
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;

use super::monitors::MonitorRecord;
use super::os_error;

pub(crate) fn interop() -> Result<IGraphicsCaptureItemInterop, CoreError> {
    windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>().map_err(os_error(
        ErrorCategory::PlatformUnavailable,
        "capture interop unavailable",
    ))
}

/// The name Windows.Graphics.Capture gives each monitor (e.g. `Display 2`), which is what
/// a picked display item reports. It does not follow GDI device numbering (observed on
/// DEV-W1: `DISPLAY1` is `Display 2`). The items created here are only read for their
/// name and dropped; they never reach a capture session. Each created item leaves one
/// process handle behind on DEV-W1 (CAP-02A finding), so callers cache the result per
/// display layout.
pub(crate) fn wgc_monitor_names(monitors: &[MonitorRecord]) -> Vec<Option<String>> {
    let Ok(interop) = interop() else {
        return vec![None; monitors.len()];
    };
    monitors
        .iter()
        .map(|m| {
            let handle = HMONITOR(m.handle as *mut core::ffi::c_void);
            // SAFETY: `handle` comes from the enumeration that produced `monitors`; the
            // call validates it and fails for stale handles.
            let item: GraphicsCaptureItem = unsafe { interop.CreateForMonitor(handle) }.ok()?;
            item.DisplayName().ok().map(|name| name.to_string_lossy())
        })
        .collect()
}

/// How a target was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetOrigin {
    /// Chosen by the local user in the Windows capture picker.
    SystemPicker,
    /// Created from a monitor or window handle by test code (`test-support` only).
    Programmatic,
}

/// An opaque Windows capture item. Not `Clone`: starting a capture consumes it, and it is
/// released when that capture stops.
pub struct WgcTarget {
    pub(crate) item: GraphicsCaptureItem,
    origin: TargetOrigin,
}

impl WgcTarget {
    pub(crate) fn from_picker(item: GraphicsCaptureItem) -> WgcTarget {
        WgcTarget {
            item,
            origin: TargetOrigin::SystemPicker,
        }
    }

    #[cfg(feature = "test-support")]
    pub(crate) fn programmatic(item: GraphicsCaptureItem) -> WgcTarget {
        WgcTarget {
            item,
            origin: TargetOrigin::Programmatic,
        }
    }

    pub fn origin(&self) -> TargetOrigin {
        self.origin
    }
}

impl fmt::Debug for WgcTarget {
    // Never prints the item's name: for a window it is the window title.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WgcTarget")
            .field("origin", &self.origin)
            .finish_non_exhaustive()
    }
}

pub(crate) fn item_size(item: &GraphicsCaptureItem) -> PixelSize {
    let size = item.Size().unwrap_or_default();
    PixelSize {
        width: u32::try_from(size.Width).unwrap_or(0),
        height: u32::try_from(size.Height).unwrap_or(0),
    }
}

/// Describes a target. It is a `Display` only when its WGC name and size match exactly
/// one current monitor (`names` from [`wgc_monitor_names`], same order as `monitors`);
/// anything else (usually a window) is `Unknown`. The result is informational (shown to
/// the local user); no security decision depends on it.
pub(crate) fn describe(
    item: &GraphicsCaptureItem,
    monitors: &[MonitorRecord],
    names: &[Option<String>],
) -> TargetInfo {
    let raw = item
        .DisplayName()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    let size = item_size(item);
    let mut matches = monitors.iter().zip(names).filter(|(m, name)| {
        m.info.size_px == size && name.as_deref().is_some_and(|n| names_match(&raw, n))
    });
    let display = match (matches.next().map(|(m, _)| m), matches.next()) {
        (Some(only), None) => Some(only.info.id),
        _ => None,
    };
    TargetInfo {
        kind: if display.is_some() {
            TargetKind::Display
        } else {
            TargetKind::Unknown
        },
        label: sanitize_label(&raw),
        size,
        display,
    }
}

/// Whether a picked item's name is the WGC name of a monitor (exact match).
pub(crate) fn names_match(item_name: &str, monitor_wgc_name: &str) -> bool {
    !monitor_wgc_name.is_empty() && item_name == monitor_wgc_name
}

/// Control characters removed, whitespace trimmed, at most 120 characters.
pub(crate) fn sanitize_label(raw: &str) -> String {
    let cleaned: String = raw.chars().filter(|c| !c.is_control()).take(120).collect();
    cleaned.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_exact_wgc_monitor_names_only() {
        assert!(names_match("Display 2", "Display 2"));
        assert!(!names_match("display 2", "Display 2"));
        assert!(!names_match("Display 2 - Notepad", "Display 2"));
        assert!(!names_match("Display 1", "Display 12"));
        // GDI device names are a different numbering and must not match.
        assert!(!names_match("DISPLAY2", "Display 2"));
        assert!(!names_match("", ""));
    }

    #[test]
    fn labels_are_sanitized() {
        assert_eq!(sanitize_label("  a\u{0}b\nc  "), "abc");
        assert_eq!(sanitize_label(&"x".repeat(500)).chars().count(), 120);
    }
}
