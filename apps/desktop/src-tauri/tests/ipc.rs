//! IPC behaviour of the shell, exercised through Tauri's mock runtime with the app's real
//! configuration and capabilities (`generate_context!` reads tauri.conf.json and
//! capabilities/ at compile time), so these tests cover the real ACL.

use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder};
use tauri::webview::InvokeRequest;
use tauri::{App, Manager, WebviewWindow, WebviewWindowBuilder};
use tp_contracts::ipc::{AppInfo, IpcErrorCode, RemoteFeatures};

const LOCAL_ORIGIN: &str = if cfg!(windows) {
    "http://tauri.localhost"
} else {
    "tauri://localhost"
};

fn app() -> App<MockRuntime> {
    tp_desktop_lib::builder(mock_builder())
        .build(tauri::generate_context!())
        .expect("build mock app")
}

fn window(app: &App<MockRuntime>, label: &str) -> WebviewWindow<MockRuntime> {
    app.get_webview_window(label).unwrap_or_else(|| {
        WebviewWindowBuilder::new(app, label, Default::default())
            .build()
            .expect("create window")
    })
}

fn request(cmd: &str, origin: &str) -> InvokeRequest {
    InvokeRequest {
        cmd: cmd.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: origin.parse().expect("origin url"),
        body: InvokeBody::default(),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_string(),
    }
}

fn invoke(
    window: &WebviewWindow<MockRuntime>,
    cmd: &str,
    origin: &str,
) -> Result<serde_json::Value, serde_json::Value> {
    get_ipc_response(window, request(cmd, origin)).map(|body| {
        body.deserialize::<serde_json::Value>()
            .expect("response body is JSON")
    })
}

#[test]
fn get_app_info_returns_the_typed_contract() {
    let app = app();
    let main = window(&app, "main");
    let value = invoke(&main, "get_app_info", LOCAL_ORIGIN).expect("get_app_info allowed");
    let info: AppInfo = serde_json::from_value(value).expect("response matches AppInfo");

    assert_eq!(info.app_name, "TP Remote");
    assert_eq!(info.app_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(info.phase, "BOOT-01");
    assert_eq!(info.remote_features, RemoteFeatures::NotImplemented);
    assert_eq!(info.protocol.current, 1);
    assert_eq!(info.platform.os, std::env::consts::OS);
    assert_eq!(info.platform.arch, std::env::consts::ARCH);
    assert_eq!(info.shell.tauri_version, tauri::VERSION);
    assert_eq!(info.engine_modules.len(), 11);
}

#[test]
fn unknown_commands_are_rejected() {
    let app = app();
    let main = window(&app, "main");
    for cmd in [
        "launch_remote_session",
        "start_capture",
        "inject_input",
        "open_listener",
        "GET_APP_INFO",
        "get_app_info ",
    ] {
        let result = invoke(&main, cmd, LOCAL_ORIGIN);
        eprintln!("{cmd:?} -> {result:?}");
        assert!(result.is_err(), "{cmd} must be rejected");
    }
}

#[test]
fn plugins_that_are_not_installed_are_rejected() {
    let app = app();
    let main = window(&app, "main");
    for cmd in [
        "plugin:shell|execute",
        "plugin:shell|spawn",
        "plugin:fs|read_text_file",
        "plugin:fs|write_file",
        "plugin:http|fetch",
        "plugin:process|exit",
    ] {
        let result = invoke(&main, cmd, LOCAL_ORIGIN);
        eprintln!("{cmd:?} -> {result:?}");
        assert!(result.is_err(), "{cmd} must be rejected");
    }
}

#[test]
fn core_commands_are_denied_because_no_core_permission_is_granted() {
    let app = app();
    let main = window(&app, "main");
    for cmd in [
        "plugin:app|version",
        "plugin:window|close",
        "plugin:window|set_title",
        "plugin:webview|create_webview_window",
        "plugin:event|listen",
        "plugin:path|resolve_directory",
    ] {
        let result = invoke(&main, cmd, LOCAL_ORIGIN);
        eprintln!("{cmd:?} -> {result:?}");
        assert!(result.is_err(), "{cmd} must be denied by the ACL");
    }
}

#[test]
fn windows_other_than_main_cannot_call_get_app_info() {
    let app = app();
    let other = window(&app, "secondary");
    let result = invoke(&other, "get_app_info", LOCAL_ORIGIN);
    eprintln!("secondary window -> {result:?}");
    assert!(result.is_err());
}

#[test]
fn remote_origins_cannot_call_get_app_info() {
    let app = app();
    let main = window(&app, "main");
    for origin in ["https://example.com", "http://192.168.1.10:1420"] {
        let result = invoke(&main, "get_app_info", origin);
        eprintln!("{origin} -> {result:?}");
        assert!(result.is_err(), "{origin} must not reach get_app_info");
    }
}

fn shell() -> tp_core::ShellContext {
    tp_core::ShellContext {
        app_name: "TP Remote".to_owned(),
        app_version: "0.1.0".to_owned(),
        tauri_version: tauri::VERSION.to_owned(),
        webview_version: None,
    }
}

#[test]
fn without_fault_injection_the_response_is_ok() {
    assert!(tp_desktop_lib::app_info_response(shell(), false).is_ok());
}

#[cfg(debug_assertions)]
#[test]
fn debug_builds_can_inject_an_ipc_fault() {
    let error = tp_desktop_lib::app_info_response(shell(), true).expect_err("fault injected");
    assert_eq!(error.code, IpcErrorCode::InjectedFault);
}

#[cfg(not(debug_assertions))]
#[test]
fn release_builds_ignore_fault_injection() {
    assert!(tp_desktop_lib::app_info_response(shell(), true).is_ok());
    let _ = IpcErrorCode::InjectedFault;
}

#[test]
fn startup_creates_only_the_main_window_and_teardown_is_clean() {
    let app = app();
    let main = window(&app, "main");
    assert!(invoke(&main, "get_app_info", LOCAL_ORIGIN).is_ok());

    let labels: Vec<String> = app.webview_windows().keys().cloned().collect();
    assert_eq!(
        labels,
        vec!["main".to_owned()],
        "unexpected windows: {labels:?}"
    );

    main.close().expect("close main window");
    app.cleanup_before_exit();
    drop(app);
}
