//! Test-only helpers (feature `test-support`, never enabled in the desktop app):
//! - programmatic capture targets, which bypass the picker and exist only so automated
//!   tests and the capture bench can run without a person;
//! - a synthetic, animated test-pattern window with known colours, so tests can prove
//!   that real OS frames (not fabricated data) were captured, without capturing anything
//!   private.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;

use tp_contracts::capture::DisplayId;
use tp_contracts::error::{CoreError, ErrorCategory};
use windows::Graphics::Capture::GraphicsCaptureItem;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, HMONITOR, InvalidateRect,
    PAINTSTRUCT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetClientRect, GetMessageW, HWND_NOTOPMOST, HWND_TOPMOST, IsWindow, KillTimer, MSG,
    PostMessageW, PostQuitMessage, RegisterClassExW, SW_SHOWMINNOACTIVE, SW_SHOWNOACTIVATE,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetTimer, SetWindowPos, ShowWindow,
    TranslateMessage, WM_CLOSE, WM_DESTROY, WM_PAINT, WM_TIMER, WNDCLASSEXW, WS_EX_APPWINDOW,
    WS_OVERLAPPEDWINDOW,
};
use windows::core::{HSTRING, PCWSTR, w};

use super::monitors;
use super::os_error;
use super::target::{WgcTarget, interop};

/// Colour of the top half of the test pattern's client area (magenta), as RGB.
pub const PATTERN_TOP_RGB: [u8; 3] = [255, 0, 255];
/// Background colour of the bottom half (dark grey), as RGB.
pub const PATTERN_BOTTOM_RGB: [u8; 3] = [32, 32, 32];

/// A target for a whole monitor, without the picker (tests only).
pub fn target_for_monitor(display: DisplayId) -> Result<WgcTarget, CoreError> {
    let record = monitors::enumerate()?
        .into_iter()
        .find(|m| m.info.id == display)
        .ok_or(CoreError::new(
            ErrorCategory::InvalidPayload,
            "no such display",
        ))?;
    let handle = HMONITOR(record.handle as *mut core::ffi::c_void);
    // SAFETY: `handle` was returned by EnumDisplayMonitors just now.
    let item: GraphicsCaptureItem =
        unsafe { interop()?.CreateForMonitor(handle) }.map_err(os_error(
            ErrorCategory::PlatformUnavailable,
            "CreateForMonitor failed",
        ))?;
    Ok(WgcTarget::programmatic(item))
}

/// The display name WGC reports for a monitor item, to validate target-to-monitor
/// matching. Monitor names are not private (they are not window titles).
pub fn monitor_item_name(display: DisplayId) -> Result<String, CoreError> {
    let target = target_for_monitor(display)?;
    target
        .item
        .DisplayName()
        .map(|name| name.to_string_lossy())
        .map_err(os_error(
            ErrorCategory::PlatformUnavailable,
            "DisplayName failed",
        ))
}

/// A stand-in for the system picker, for automated tests of the preview host only:
/// `pick` "selects" a fixed window of this process (the synthetic test pattern) without
/// showing any UI. It bypasses the trusted selection boundary by design and therefore
/// exists only with `test-support`; the desktop app never contains it.
pub struct WindowPickerForTests {
    provider: super::WgcProvider,
    hwnd: isize,
}

impl WindowPickerForTests {
    pub fn new(hwnd: isize) -> WindowPickerForTests {
        WindowPickerForTests {
            provider: super::WgcProvider::new(),
            hwnd,
        }
    }
}

impl tp_contracts::capture::CaptureProvider for WindowPickerForTests {
    type Frame = super::WgcFrame;
    type Target = WgcTarget;
    type Stream = super::WgcStream;

    fn capabilities(&self) -> tp_contracts::capture::CaptureCapabilities {
        self.provider.capabilities()
    }

    fn displays(&self) -> Result<tp_contracts::capture::DisplayLayout, CoreError> {
        self.provider.displays()
    }

    fn start<S>(
        &self,
        target: WgcTarget,
        config: tp_contracts::capture::CaptureConfig,
        sink: std::sync::Arc<S>,
    ) -> Result<super::WgcStream, CoreError>
    where
        S: tp_contracts::capture::FrameSink<super::WgcFrame> + 'static,
    {
        self.provider.start(target, config, sink)
    }
}

impl tp_contracts::capture::CapturePicker for WindowPickerForTests {
    fn pick(
        &self,
        _owner: tp_contracts::capture::PickerOwner,
        done: tp_contracts::capture::PickDone<WgcTarget>,
    ) -> Result<(), CoreError> {
        let outcome = match target_for_window(self.hwnd) {
            Ok(target) => tp_contracts::capture::PickOutcome::Selected(target),
            Err(error) => tp_contracts::capture::PickOutcome::Failed(error),
        };
        // Like the real picker, complete later and on another thread.
        std::thread::Builder::new()
            .name("tp-test-picker".into())
            .spawn(move || done(outcome))
            .map(|_| ())
            .map_err(|_| CoreError::new(ErrorCategory::InternalError, "spawn failed"))
    }
}

/// Whether `hwnd` still refers to a window.
pub fn window_exists(hwnd: isize) -> bool {
    // SAFETY: IsWindow accepts any handle value and only reads.
    unsafe { IsWindow(Some(HWND(hwnd as *mut core::ffi::c_void))) }.as_bool()
}

/// A target for a window of this process, without the picker (tests only).
pub fn target_for_window(hwnd: isize) -> Result<WgcTarget, CoreError> {
    // SAFETY: CreateForWindow validates the handle and fails for invalid windows.
    let item: GraphicsCaptureItem =
        unsafe { interop()?.CreateForWindow(HWND(hwnd as *mut core::ffi::c_void)) }.map_err(
            os_error(ErrorCategory::PlatformUnavailable, "CreateForWindow failed"),
        )?;
    Ok(WgcTarget::programmatic(item))
}

static FRAME: AtomicU32 = AtomicU32::new(0);
static PAINTS: AtomicU32 = AtomicU32::new(0);

/// WM_PAINT messages handled by test-pattern windows of this process (diagnostics:
/// tells whether the source kept changing while capture is measured).
pub fn pattern_paints() -> u32 {
    PAINTS.load(Ordering::Relaxed)
}
const TIMER_ID: usize = 1;
/// USER_TIMER_MINIMUM. The system timer resolution (≈15.6 ms) rounds it up, so the
/// pattern repaints about 64 times per second, above a 60 Hz display's refresh.
const TIMER_MS: u32 = 10;

/// An animated window: top half magenta, bottom half dark grey with a white bar that
/// moves on every timer tick. Runs its own message loop on its own thread. Shown without
/// activation so it never takes keyboard focus.
pub struct TestPatternWindow {
    hwnd: isize,
    thread: Option<JoinHandle<()>>,
}

impl TestPatternWindow {
    pub fn open(title: &str, x: i32, y: i32, width: i32, height: i32) -> Result<Self, CoreError> {
        let (sender, receiver) = mpsc::channel::<Result<isize, CoreError>>();
        let title = HSTRING::from(title);
        let thread = std::thread::Builder::new()
            .name("tp-test-pattern".into())
            .spawn(move || run_window(&title, x, y, width, height, &sender))
            .map_err(|_| CoreError::new(ErrorCategory::InternalError, "spawn failed"))?;
        match receiver.recv() {
            Ok(Ok(hwnd)) => Ok(TestPatternWindow {
                hwnd,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => {
                let _ = thread.join();
                Err(CoreError::new(
                    ErrorCategory::InternalError,
                    "window thread died",
                ))
            }
        }
    }

    pub fn hwnd(&self) -> isize {
        self.hwnd
    }

    /// Resizes the outer window (title bar and borders included).
    pub fn resize(&self, width: i32, height: i32) -> Result<(), CoreError> {
        // SAFETY: SetWindowPos validates the handle; it may be called from any thread.
        unsafe {
            SetWindowPos(
                HWND(self.hwnd as *mut core::ffi::c_void),
                None,
                0,
                0,
                width,
                height,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
        }
        .map_err(os_error(
            ErrorCategory::InternalError,
            "SetWindowPos failed",
        ))
    }

    /// Keeps the window above all non-topmost windows (`true`) or releases it (`false`),
    /// without activating it. Used to separate occlusion effects from capture behaviour.
    pub fn set_topmost(&self, topmost: bool) -> Result<(), CoreError> {
        let after = if topmost {
            HWND_TOPMOST
        } else {
            HWND_NOTOPMOST
        };
        // SAFETY: SetWindowPos validates the handle; it may be called from any thread.
        unsafe {
            SetWindowPos(
                HWND(self.hwnd as *mut core::ffi::c_void),
                Some(after),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
        }
        .map_err(os_error(
            ErrorCategory::InternalError,
            "SetWindowPos failed",
        ))
    }

    /// Minimizes (`true`) or restores (`false`) the window without activating it.
    pub fn set_minimized(&self, minimized: bool) {
        let command = if minimized {
            SW_SHOWMINNOACTIVE
        } else {
            SW_SHOWNOACTIVATE
        };
        // SAFETY: ShowWindow validates the handle; cross-thread calls are allowed.
        let _ = unsafe { ShowWindow(HWND(self.hwnd as *mut core::ffi::c_void), command) };
    }

    /// Closes the window and waits for its thread to finish.
    pub fn close(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        if let Some(thread) = self.thread.take() {
            // SAFETY: posting to a window handle is safe even if it is already gone.
            let _ = unsafe {
                PostMessageW(
                    Some(HWND(self.hwnd as *mut core::ffi::c_void)),
                    WM_CLOSE,
                    WPARAM(0),
                    LPARAM(0),
                )
            };
            let _ = thread.join();
        }
    }
}

impl Drop for TestPatternWindow {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run_window(
    title: &HSTRING,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    ready: &mpsc::Sender<Result<isize, CoreError>>,
) {
    let class = w!("TpRemoteTestPattern");
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(window_proc),
        lpszClassName: class,
        ..Default::default()
    };
    // SAFETY: `wc` is fully initialised; registering twice fails harmlessly (the class
    // then already exists for this process).
    unsafe { RegisterClassExW(&wc) };
    // SAFETY: class registered above; title outlives the call; no parent or menu.
    let created = unsafe {
        CreateWindowExW(
            WS_EX_APPWINDOW,
            class,
            PCWSTR(title.as_ptr()),
            WS_OVERLAPPEDWINDOW,
            x,
            y,
            width,
            height,
            None,
            None,
            None,
            None,
        )
    };
    let hwnd = match created {
        Ok(hwnd) => hwnd,
        Err(error) => {
            let _ = ready.send(Err(os_error(
                ErrorCategory::InternalError,
                "CreateWindowExW failed",
            )(error)));
            return;
        }
    };
    // SAFETY: `hwnd` is the window just created on this thread.
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetTimer(Some(hwnd), TIMER_ID, TIMER_MS, None);
    }
    let _ = ready.send(Ok(hwnd.0 as isize));
    let mut msg = MSG::default();
    // SAFETY: standard message loop for windows owned by this thread.
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// SAFETY contract: only the system calls this, as the window procedure of the class
/// registered in `run_window`, on the window's own thread.
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY (whole function): called by the system on the window's own thread with a
    // valid `hwnd`; every call below receives that handle or locals.
    unsafe {
        match message {
            WM_TIMER => {
                FRAME.fetch_add(1, Ordering::Relaxed);
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_PAINT => {
                PAINTS.fetch_add(1, Ordering::Relaxed);
                let mut ps = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut ps);
                let mut client = RECT::default();
                let _ = GetClientRect(hwnd, &mut client);
                let middle = client.top + (client.bottom - client.top) / 2;
                let fill = |rect: RECT, rgb: [u8; 3]| {
                    let colour = COLORREF(
                        u32::from(rgb[0]) | (u32::from(rgb[1]) << 8) | (u32::from(rgb[2]) << 16),
                    );
                    let brush = CreateSolidBrush(colour);
                    FillRect(dc, &rect, brush);
                    let _ = DeleteObject(brush.into());
                };
                fill(
                    RECT {
                        bottom: middle,
                        ..client
                    },
                    PATTERN_TOP_RGB,
                );
                fill(
                    RECT {
                        top: middle,
                        ..client
                    },
                    PATTERN_BOTTOM_RGB,
                );
                let span = (client.right - client.left).max(1);
                let bar_x = client.left + (FRAME.load(Ordering::Relaxed) as i32 * 8) % span;
                fill(
                    RECT {
                        left: bar_x,
                        right: bar_x + 24,
                        top: middle,
                        bottom: client.bottom,
                    },
                    [255, 255, 255],
                );
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = KillTimer(Some(hwnd), TIMER_ID);
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}
