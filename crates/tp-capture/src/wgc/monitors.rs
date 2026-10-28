//! Monitor enumeration (Win32 GDI and HiDpi). The process is Per-Monitor-V2 DPI aware
//! (app manifest), so monitor rectangles are physical pixels of the virtual desktop and
//! can be negative.

use std::mem::size_of;

use tp_contracts::capture::{DisplayId, DisplayInfo, PixelSize, RectI32};
use tp_contracts::error::{CoreError, ErrorCategory};
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    DEVMODEW, ENUM_CURRENT_SETTINGS, EnumDisplayMonitors, EnumDisplaySettingsW, GetMonitorInfoW,
    HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;
use windows::core::{BOOL, PCWSTR};

/// One monitor as Windows reports it now.
#[derive(Debug, Clone)]
pub(crate) struct MonitorRecord {
    pub info: DisplayInfo,
    /// GDI device name without the `\\.\` prefix, e.g. `DISPLAY1`.
    pub device: String,
    /// `HMONITOR` value. Valid only until the next display change.
    pub handle: isize,
}

/// Enumerates the monitors, sorted by device name so the order (and therefore the
/// [`DisplayId`]s) is deterministic for an unchanged layout. The order says nothing
/// about physical left-to-right placement.
pub(crate) fn enumerate() -> Result<Vec<MonitorRecord>, CoreError> {
    let mut handles: Vec<HMONITOR> = Vec::new();
    // SAFETY: the callback only runs during this call and receives the address of
    // `handles`, which outlives the call; no clip rectangle or DC is passed.
    let ok = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(collect_monitor),
            LPARAM(&mut handles as *mut Vec<HMONITOR> as isize),
        )
    };
    if !ok.as_bool() {
        return Err(CoreError::new(
            ErrorCategory::PlatformUnavailable,
            "EnumDisplayMonitors failed",
        ));
    }

    let mut records: Vec<MonitorRecord> = handles.into_iter().filter_map(describe).collect();
    records.sort_by(|a, b| a.device.cmp(&b.device));
    for (index, record) in records.iter_mut().enumerate() {
        record.info.id = DisplayId(index as u32);
    }
    Ok(records)
}

/// SAFETY contract: only EnumDisplayMonitors calls this, synchronously, with the `data`
/// value `enumerate` passed (a pointer to its local `Vec<HMONITOR>`).
unsafe extern "system" fn collect_monitor(
    monitor: HMONITOR,
    _dc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // SAFETY: `data` is the `&mut Vec<HMONITOR>` passed by `enumerate`, alive for the
    // whole enumeration; this callback runs on the enumerating thread only.
    let handles = unsafe { &mut *(data.0 as *mut Vec<HMONITOR>) };
    handles.push(monitor);
    BOOL::from(true)
}

fn describe(monitor: HMONITOR) -> Option<MonitorRecord> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: `info` is a MONITORINFOEXW whose cbSize says so; GetMonitorInfoW writes at
    // most that many bytes.
    let ok = unsafe {
        GetMonitorInfoW(
            monitor,
            &mut info as *mut MONITORINFOEXW as *mut MONITORINFO,
        )
    };
    if !ok.as_bool() {
        return None;
    }
    let rect = info.monitorInfo.rcMonitor;
    let width = u32::try_from(rect.right - rect.left).ok()?;
    let height = u32::try_from(rect.bottom - rect.top).ok()?;
    let device_raw = wide_to_string(&info.szDevice);

    let (mut dpi_x, mut dpi_y) = (0u32, 0u32);
    // SAFETY: valid monitor handle from the enumeration; out-pointers are locals.
    let scale_percent =
        unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }
            .ok()
            .filter(|()| dpi_x > 0)
            .map(|()| dpi_x * 100 / 96);

    let mut mode = DEVMODEW {
        dmSize: size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };
    // SAFETY: `szDevice` is NUL-terminated (GetMonitorInfoW guarantees it) and outlives
    // the call; `mode.dmSize` is set as the API requires.
    let have_mode = unsafe {
        EnumDisplaySettingsW(
            PCWSTR(info.szDevice.as_ptr()),
            ENUM_CURRENT_SETTINGS,
            &mut mode,
        )
    }
    .as_bool();
    let refresh_hz = (have_mode && mode.dmDisplayFrequency > 1).then_some(mode.dmDisplayFrequency);

    let device = sanitize_device_name(&device_raw);
    Some(MonitorRecord {
        info: DisplayInfo {
            id: DisplayId(0),
            name: device.clone(),
            bounds_os: RectI32 {
                x: rect.left,
                y: rect.top,
                width,
                height,
            },
            size_px: PixelSize { width, height },
            // Per-Monitor-V2: one OS unit is one physical pixel.
            scale_milli: 1000,
            scale_percent,
            refresh_hz,
            is_primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
        },
        device,
        handle: monitor.0 as isize,
    })
}

fn wide_to_string(wide: &[u16]) -> String {
    let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..end])
}

/// `\\.\DISPLAY1` → `DISPLAY1`; only ASCII letters, digits, `_` and `-` are kept.
pub(crate) fn sanitize_device_name(raw: &str) -> String {
    raw.trim_start_matches(r"\\.\")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        .take(32)
        .collect()
}

/// Assigns layout epochs: a new epoch whenever the observed display list changes.
#[derive(Debug, Default)]
pub(crate) struct LayoutTracker {
    epoch: u32,
    last: Option<Vec<DisplayInfo>>,
}

impl LayoutTracker {
    pub fn observe(&mut self, displays: &[DisplayInfo]) -> u32 {
        if self.last.as_deref() != Some(displays) {
            self.epoch = self.epoch.saturating_add(1);
            self.last = Some(displays.to_vec());
        }
        self.epoch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display(x: i32) -> DisplayInfo {
        DisplayInfo {
            id: DisplayId(0),
            name: "DISPLAY1".into(),
            bounds_os: RectI32 {
                x,
                y: 0,
                width: 1920,
                height: 1080,
            },
            size_px: PixelSize {
                width: 1920,
                height: 1080,
            },
            scale_milli: 1000,
            scale_percent: Some(100),
            refresh_hz: Some(60),
            is_primary: true,
        }
    }

    #[test]
    fn epoch_changes_only_when_the_layout_changes() {
        let mut tracker = LayoutTracker::default();
        assert_eq!(tracker.observe(&[display(0)]), 1);
        assert_eq!(tracker.observe(&[display(0)]), 1);
        assert_eq!(tracker.observe(&[display(-1920)]), 2);
        assert_eq!(tracker.observe(&[]), 3);
        assert_eq!(tracker.observe(&[]), 3);
    }

    #[test]
    fn device_names_are_sanitized() {
        assert_eq!(sanitize_device_name(r"\\.\DISPLAY12"), "DISPLAY12");
        assert_eq!(sanitize_device_name("A\u{0}B C\u{202e}D"), "ABCD");
        assert_eq!(sanitize_device_name(&"X".repeat(100)).len(), 32);
    }
}
