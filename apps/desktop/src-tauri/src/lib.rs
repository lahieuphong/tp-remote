//! TP Remote desktop shell (Tauri 2).
//!
//! The shell is deliberately thin: it owns the window and the IPC boundary and delegates
//! every answer to `tp-core`. Commands:
//! - `get_app_info` (BOOT-01);
//! - CAP-02A local self-preview (ADR-007): `get_capture_displays`,
//!   `get_local_preview_status`, `start_local_preview`, `stop_local_preview`,
//!   `get_local_preview_frame`.
//!
//! The preview commands cannot name a capture target: `start_local_preview` only opens
//! the Windows capture picker over the main window, and capture starts only when the
//! local user selects something there. Only the `main` window may call them (capability
//! ACL, plus a label check in each command).
#![forbid(unsafe_code)]

use tauri::ipc::Response;
use tauri::{AppHandle, Manager, Runtime, State, WebviewWindow};
use tp_contracts::ipc::{AppInfo, CaptureDisplays, IpcError, IpcErrorCode, LocalPreviewStatus};
use tp_core::preview::{Reject, StartError};
use tp_core::{NativePreview, ShellContext};
use tp_diagnostics::{Level, Logger, emit};

/// Debug builds only: when this variable is set, `get_app_info` fails on purpose so the
/// UI's error state can be verified against the real IPC path. Release builds ignore it.
#[cfg(debug_assertions)]
pub const FAULT_INJECTION_ENV: &str = "TP_REMOTE_DEV_FAIL_APP_INFO";

/// The only window allowed to use capture commands.
pub const MAIN_WINDOW: &str = "main";

#[tauri::command]
fn get_app_info<R: Runtime>(app: AppHandle<R>) -> Result<AppInfo, IpcError> {
    #[cfg(debug_assertions)]
    let inject_fault = std::env::var_os(FAULT_INJECTION_ENV).is_some();
    #[cfg(not(debug_assertions))]
    let inject_fault = false;

    app_info_response(shell_context(&app), inject_fault)
}

/// The command's behaviour, separated from environment access so it can be tested.
pub fn app_info_response(shell: ShellContext, inject_fault: bool) -> Result<AppInfo, IpcError> {
    if inject_fault && cfg!(debug_assertions) {
        emit(
            Level::Warn,
            "ipc.get_app_info",
            &[("result", "err"), ("error_code", "injectedFault")],
        );
        return Err(IpcError {
            code: IpcErrorCode::InjectedFault,
            message: "get_app_info failed: fault injected for testing (debug build)".to_owned(),
        });
    }
    let info = tp_core::app_info(shell);
    emit(Level::Info, "ipc.get_app_info", &[("result", "ok")]);
    Ok(info)
}

fn shell_context<R: Runtime>(app: &AppHandle<R>) -> ShellContext {
    let package = app.package_info();
    ShellContext {
        app_name: package.name.clone(),
        app_version: package.version.to_string(),
        tauri_version: tauri::VERSION.to_owned(),
        webview_version: tauri::webview_version().ok(),
    }
}

fn error(code: IpcErrorCode, message: &str) -> IpcError {
    IpcError {
        code,
        message: message.to_owned(),
    }
}

/// Defence in depth: the capability already limits these commands to `main`.
fn require_main<R: Runtime>(window: &WebviewWindow<R>, command: &str) -> Result<(), IpcError> {
    if window.label() == MAIN_WINDOW {
        Ok(())
    } else {
        emit(
            Level::Warn,
            "ipc.forbidden",
            &[("command", command), ("result", "err")],
        );
        Err(error(
            IpcErrorCode::Forbidden,
            "Only the main TP Remote window may use screen capture.",
        ))
    }
}

#[tauri::command]
fn get_capture_displays<R: Runtime>(
    window: WebviewWindow<R>,
    preview: State<'_, NativePreview>,
) -> Result<CaptureDisplays, IpcError> {
    require_main(&window, "get_capture_displays")?;
    preview.displays().map_err(|e| {
        error(
            IpcErrorCode::Unavailable,
            if e.category() == tp_contracts::error::ErrorCategory::PlatformUnavailable {
                "Displays cannot be listed on this system."
            } else {
                "The display list could not be read."
            },
        )
    })
}

#[tauri::command]
fn get_local_preview_status<R: Runtime>(
    window: WebviewWindow<R>,
    preview: State<'_, NativePreview>,
) -> Result<LocalPreviewStatus, IpcError> {
    require_main(&window, "get_local_preview_status")?;
    Ok(preview.status())
}

/// Opens the Windows capture picker over the main window. Runs on the main thread (a
/// synchronous Tauri command), as the picker requires. Returns at once; the picker's
/// result arrives later and is visible through `get_local_preview_status`.
#[tauri::command]
fn start_local_preview<R: Runtime>(
    window: WebviewWindow<R>,
    preview: State<'_, NativePreview>,
) -> Result<LocalPreviewStatus, IpcError> {
    require_main(&window, "start_local_preview")?;
    let owner = picker_owner(&window)?;
    preview.start(owner).map_err(|e| {
        let (code, text) = match e {
            StartError::Rejected(Reject::NotAvailable) => (
                IpcErrorCode::Unavailable,
                "Screen capture is not available on this system.",
            ),
            StartError::Rejected(_) => (
                IpcErrorCode::InvalidState,
                "A preview is already being started or is running.",
            ),
            StartError::Platform(core)
                if core.category() == tp_contracts::error::ErrorCategory::PermissionDenied =>
            {
                (
                    IpcErrorCode::Forbidden,
                    "The capture picker can only be opened over the TP Remote window.",
                )
            }
            StartError::Platform(_) => (
                IpcErrorCode::Unavailable,
                "The Windows capture picker could not be opened.",
            ),
        };
        error(code, text)
    })
}

#[cfg(windows)]
fn picker_owner<R: Runtime>(
    window: &WebviewWindow<R>,
) -> Result<tp_contracts::capture::PickerOwner, IpcError> {
    let hwnd = window.hwnd().map_err(|_| {
        error(
            IpcErrorCode::Unavailable,
            "The TP Remote window handle is not available.",
        )
    })?;
    Ok(tp_contracts::capture::PickerOwner(hwnd.0 as isize))
}

#[cfg(not(windows))]
fn picker_owner<R: Runtime>(
    _window: &WebviewWindow<R>,
) -> Result<tp_contracts::capture::PickerOwner, IpcError> {
    Err(error(
        IpcErrorCode::Unavailable,
        "Screen capture is not implemented for this operating system yet.",
    ))
}

#[tauri::command]
fn stop_local_preview<R: Runtime>(
    window: WebviewWindow<R>,
    preview: State<'_, NativePreview>,
) -> Result<LocalPreviewStatus, IpcError> {
    require_main(&window, "stop_local_preview")?;
    Ok(preview.stop())
}

/// The latest preview frame as raw bytes (`tp_contracts::ipc::preview_frame` layout).
/// Delivered through Tauri's IPC protocol: no network socket is involved.
#[tauri::command]
fn get_local_preview_frame<R: Runtime>(
    window: WebviewWindow<R>,
    preview: State<'_, NativePreview>,
    generation: u32,
    painted: u32,
) -> Result<Response, IpcError> {
    require_main(&window, "get_local_preview_frame")?;
    Ok(Response::new(preview.take_frame(generation, painted)))
}

/// Registers the shell's state and commands. Used by [`run`] and by the IPC tests.
pub fn builder<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder
        .manage(tp_core::native_preview(Logger::stderr()))
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            get_capture_displays,
            get_local_preview_status,
            start_local_preview,
            stop_local_preview,
            get_local_preview_frame,
        ])
}

/// Starts the desktop application. Blocks until the application exits.
pub fn run() {
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let pid = std::process::id().to_string();
    emit(
        Level::Info,
        "app.start",
        &[
            ("version", env!("CARGO_PKG_VERSION")),
            ("profile", profile),
            ("pid", &pid),
        ],
    );

    let app = match builder(tauri::Builder::default()).build(tauri::generate_context!()) {
        Ok(app) => app,
        Err(error) => {
            emit(
                Level::Error,
                "app.start_failed",
                &[("error", &error.to_string())],
            );
            std::process::exit(1);
        }
    };

    app.run(|app, event| match event {
        tauri::RunEvent::Ready => emit(Level::Info, "app.ready", &[]),
        // Capture must not outlive the window: release it before the process exits.
        tauri::RunEvent::ExitRequested { .. } => {
            app.state::<NativePreview>().stop();
        }
        tauri::RunEvent::Exit => {
            app.state::<NativePreview>().stop();
            emit(Level::Info, "app.exit", &[]);
        }
        _ => {}
    });
}
