//! TP Remote desktop shell (Tauri 2).
//!
//! The shell is deliberately thin: it owns the window and the IPC boundary and delegates
//! every answer to `tp-core`. BOOT-01 exposes exactly one command, `get_app_info`.
#![forbid(unsafe_code)]

use tauri::{AppHandle, Runtime};
use tp_contracts::ipc::{AppInfo, IpcError, IpcErrorCode};
use tp_core::ShellContext;
use tp_diagnostics::{Level, emit};

/// Debug builds only: when this variable is set, `get_app_info` fails on purpose so the
/// UI's error state can be verified against the real IPC path. Release builds ignore it.
#[cfg(debug_assertions)]
pub const FAULT_INJECTION_ENV: &str = "TP_REMOTE_DEV_FAIL_APP_INFO";

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

/// Registers the shell's commands. Used by [`run`] and by the IPC tests.
pub fn builder<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(tauri::generate_handler![get_app_info])
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

    app.run(|_app, event| match event {
        tauri::RunEvent::Ready => emit(Level::Info, "app.ready", &[]),
        tauri::RunEvent::Exit => emit(Level::Info, "app.exit", &[]),
        _ => {}
    });
}
