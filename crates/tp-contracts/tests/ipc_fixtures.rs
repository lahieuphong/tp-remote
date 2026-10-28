//! The Rust DTOs and tests/fixtures/ipc must describe the same JSON. The TypeScript
//! contract tests (apps/desktop/src/contract) read the same fixture files.

use std::path::PathBuf;

use tp_contracts::ipc::{
    AppInfo, BuildProfile, EngineModuleStatus, IpcError, IpcErrorCode, PlatformInfo,
    ProtocolSupport, RemoteFeatures, ShellInfo,
};
use tp_contracts::module::ModuleState;

fn fixture(path: &str) -> Vec<u8> {
    let full = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/ipc")
        .join(path);
    std::fs::read(&full).unwrap_or_else(|e| panic!("read {}: {e}", full.display()))
}

fn canonical() -> AppInfo {
    AppInfo {
        app_name: "TP Remote".to_owned(),
        app_version: "0.1.0".to_owned(),
        phase: "BOOT-01".to_owned(),
        platform: PlatformInfo {
            os: "windows".to_owned(),
            family: "windows".to_owned(),
            arch: "x86_64".to_owned(),
        },
        build_profile: BuildProfile::Debug,
        protocol: ProtocolSupport {
            current: 1,
            min_supported: 1,
            max_supported: 1,
        },
        shell: ShellInfo {
            tauri_version: "2.12.5".to_owned(),
            webview_version: Some("155.0.4283.62".to_owned()),
        },
        remote_features: RemoteFeatures::NotImplemented,
        engine_modules: vec![
            EngineModuleStatus {
                crate_name: "tp-protocol".to_owned(),
                role: "protocol".to_owned(),
                state: ModuleState::Partial,
                owner_phase: "NET-03".to_owned(),
            },
            EngineModuleStatus {
                crate_name: "tp-capture".to_owned(),
                role: "platform-capture".to_owned(),
                state: ModuleState::Foundation,
                owner_phase: "CAP-02".to_owned(),
            },
        ],
    }
}

#[test]
fn rust_serialization_equals_the_valid_fixture() {
    let expected: serde_json::Value =
        serde_json::from_slice(&fixture("app-info.valid.json")).expect("fixture is JSON");
    assert_eq!(
        serde_json::to_value(canonical()).expect("serialize"),
        expected
    );
}

#[test]
fn the_valid_fixture_deserializes_to_the_canonical_value() {
    let parsed: AppInfo =
        serde_json::from_slice(&fixture("app-info.valid.json")).expect("deserialize");
    assert_eq!(parsed, canonical());
}

#[test]
fn serialization_round_trips() {
    let json = serde_json::to_string(&canonical()).expect("serialize");
    assert_eq!(
        serde_json::from_str::<AppInfo>(&json).expect("deserialize"),
        canonical()
    );
}

#[test]
fn a_missing_webview_version_serializes_as_null() {
    let mut info = canonical();
    info.shell.webview_version = None;
    let value = serde_json::to_value(info).expect("serialize");
    assert_eq!(value["shell"]["webviewVersion"], serde_json::Value::Null);
}

#[test]
fn invalid_fixtures_are_rejected_by_rust() {
    for name in [
        "missing-field.json",
        "wrong-type.json",
        "unknown-field.json",
        "bad-enum.json",
        "bad-nested-enum.json",
    ] {
        let result = serde_json::from_slice::<AppInfo>(&fixture(&format!("invalid/{name}")));
        assert!(result.is_err(), "{name} was accepted");
    }
}

#[test]
fn ipc_error_uses_camel_case_codes() {
    let error = IpcError {
        code: IpcErrorCode::InjectedFault,
        message: "x".to_owned(),
    };
    assert_eq!(
        serde_json::to_value(error).expect("serialize"),
        serde_json::json!({ "code": "injectedFault", "message": "x" })
    );
}
