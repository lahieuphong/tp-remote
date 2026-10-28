//! IPC behaviour of the shell, exercised through Tauri's mock runtime with the app's real
//! configuration and capabilities (`generate_context!` reads tauri.conf.json and
//! capabilities/ at compile time), so these tests cover the real ACL.

use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder};
use tauri::webview::InvokeRequest;
use tauri::{App, Manager, WebviewWindow, WebviewWindowBuilder};
use tp_contracts::ipc::preview_frame::{HEADER_LEN, Header};
use tp_contracts::ipc::{
    AppInfo, CaptureDisplays, IpcError, IpcErrorCode, LocalPreviewState, LocalPreviewStatus,
    RemoteFeatures,
};

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
    request_with(cmd, origin, InvokeBody::default())
}

fn request_with(cmd: &str, origin: &str, body: InvokeBody) -> InvokeRequest {
    InvokeRequest {
        cmd: cmd.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: origin.parse().expect("origin url"),
        body,
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

/// True when Tauri's ACL (not the command itself) rejected the call.
fn denied_by_acl(result: &Result<serde_json::Value, serde_json::Value>) -> bool {
    matches!(result, Err(serde_json::Value::String(text)) if text.contains("not allowed"))
}

#[test]
fn get_app_info_returns_the_typed_contract() {
    let app = app();
    let main = window(&app, "main");
    let value = invoke(&main, "get_app_info", LOCAL_ORIGIN).expect("get_app_info allowed");
    let info: AppInfo = serde_json::from_value(value).expect("response matches AppInfo");

    assert_eq!(info.app_name, "TP Remote");
    assert_eq!(info.app_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(info.phase, tp_core::PHASE);
    assert_eq!(info.remote_features, RemoteFeatures::NotImplemented);
    assert_eq!(info.protocol.current, 1);
    assert_eq!(info.platform.os, std::env::consts::OS);
    assert_eq!(info.platform.arch, std::env::consts::ARCH);
    assert_eq!(info.shell.tauri_version, tauri::VERSION);
    assert_eq!(info.engine_modules.len(), 11);
}

// T-CAP-SEC-07 includes the capture-shaped names below: none of them exists.
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
        "start_local_preview_for_display",
        "capture_display",
        "capture_window",
        "set_capture_border",
        "get_raw_frame",
        "save_preview_frame",
        "START_LOCAL_PREVIEW",
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

const PREVIEW_COMMANDS: [&str; 5] = [
    "get_capture_displays",
    "get_local_preview_status",
    "start_local_preview",
    "stop_local_preview",
    "get_local_preview_frame",
];

fn frame_args(generation: u32) -> InvokeBody {
    InvokeBody::Json(serde_json::json!({ "generation": generation, "painted": 0 }))
}

// The CAP-02A commands are reachable from the main window through the real ACL.
#[test]
fn preview_commands_are_allowed_for_the_main_window() {
    let app = app();
    let main = window(&app, "main");

    let displays: CaptureDisplays = serde_json::from_value(
        invoke(&main, "get_capture_displays", LOCAL_ORIGIN).expect("displays"),
    )
    .expect("CaptureDisplays");
    assert!(!displays.displays.is_empty());

    let status: LocalPreviewStatus = serde_json::from_value(
        invoke(&main, "get_local_preview_status", LOCAL_ORIGIN).expect("status"),
    )
    .expect("LocalPreviewStatus");
    assert_eq!(status.state, LocalPreviewState::Ready);
    assert_eq!(status.target, None);

    let stopped: LocalPreviewStatus = serde_json::from_value(
        invoke(&main, "stop_local_preview", LOCAL_ORIGIN).expect("stop is always allowed"),
    )
    .expect("LocalPreviewStatus");
    assert_eq!(stopped.state, LocalPreviewState::Ready);

    let frame = get_ipc_response(
        &main,
        request_with("get_local_preview_frame", LOCAL_ORIGIN, frame_args(1)),
    )
    .expect("frame command allowed");
    match frame {
        InvokeResponseBody::Raw(bytes) => {
            assert_eq!(
                Header::decode(&bytes),
                Some(Header::EMPTY),
                "no frame without a preview"
            );
            assert_eq!(bytes.len(), HEADER_LEN);
        }
        InvokeResponseBody::Json(json) => panic!("frame must be binary, got JSON {json}"),
    }
}

// T-CAP-SEC-01 at the IPC boundary: the start command alone never captures. Under the
// mock runtime the window handle is fabricated, so the provider refuses to open the
// picker at all (it must be a live window of this process).
#[test]
fn start_without_a_real_owner_window_opens_no_picker() {
    let app = app();
    let main = window(&app, "main");
    let result = invoke(&main, "start_local_preview", LOCAL_ORIGIN);
    eprintln!("start_local_preview (mock window) -> {result:?}");
    assert!(!denied_by_acl(&result), "start must pass the ACL");
    let error: IpcError = serde_json::from_value(result.expect_err("must fail")).expect("IpcError");
    assert_eq!(error.code, IpcErrorCode::Forbidden);
    let status: LocalPreviewStatus = serde_json::from_value(
        invoke(&main, "get_local_preview_status", LOCAL_ORIGIN).expect("status"),
    )
    .expect("status");
    assert_ne!(status.state, LocalPreviewState::Capturing);
    assert_ne!(status.state, LocalPreviewState::SelectingTarget);
    assert_eq!(status.target, None);
}

// T-CAP-SEC-08: other windows and remote origins cannot reach any preview command.
#[test]
fn preview_commands_are_denied_to_other_windows_and_remote_origins() {
    let app = app();
    let main = window(&app, "main");
    let other = window(&app, "secondary");
    for cmd in PREVIEW_COMMANDS {
        let body = || {
            if cmd == "get_local_preview_frame" {
                frame_args(1)
            } else {
                InvokeBody::default()
            }
        };
        let from_other = get_ipc_response(&other, request_with(cmd, LOCAL_ORIGIN, body()));
        eprintln!("secondary {cmd} -> {:?}", from_other.as_ref().err());
        assert!(from_other.is_err(), "{cmd} reachable from another window");
        for origin in [
            "https://example.com",
            "http://192.168.1.10:1420",
            "http://localhost:1420",
        ] {
            let result = get_ipc_response(&main, request_with(cmd, origin, body()));
            assert!(result.is_err(), "{cmd} reachable from {origin}");
        }
    }
}

#[test]
fn frame_requests_with_invalid_arguments_are_rejected() {
    let app = app();
    let main = window(&app, "main");
    for args in [
        serde_json::json!({}),
        serde_json::json!({ "generation": -1, "painted": 0 }),
        serde_json::json!({ "generation": "1", "painted": 0 }),
        serde_json::json!({ "generation": 4_294_967_296u64, "painted": 0 }),
    ] {
        let result = get_ipc_response(
            &main,
            request_with(
                "get_local_preview_frame",
                LOCAL_ORIGIN,
                InvokeBody::Json(args.clone()),
            ),
        );
        assert!(result.is_err(), "accepted invalid arguments {args}");
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
