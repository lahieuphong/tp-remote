//! Static checks of the shell's security-relevant configuration (T-STA-10 subset).

use std::path::PathBuf;

use serde_json::Value;

fn read(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn json(relative: &str) -> Value {
    serde_json::from_str(&read(relative)).unwrap_or_else(|e| panic!("parse {relative}: {e}"))
}

// Updated in CAP-02A: the exact list now includes the five local-preview commands
// (ADR-007). Anything else — core, plugin or wildcard permissions — still fails here.
#[test]
fn the_only_capability_grants_only_the_app_commands_to_the_main_window() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("capabilities");
    let files: Vec<_> = std::fs::read_dir(&dir)
        .expect("read capabilities")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(files, vec!["main-window.json".to_owned()]);

    let capability = json("capabilities/main-window.json");
    assert_eq!(capability["identifier"], "main-window");
    assert_eq!(capability["windows"], serde_json::json!(["main"]));
    assert_eq!(
        capability["permissions"],
        serde_json::json!([
            "allow-get-app-info",
            "allow-get-capture-displays",
            "allow-get-local-preview-status",
            "allow-start-local-preview",
            "allow-stop-local-preview",
            "allow-get-local-preview-frame"
        ])
    );
    assert!(
        capability.get("remote").is_none(),
        "no remote origins may be granted"
    );
    assert!(capability.get("platforms").is_none());
}

#[test]
fn tauri_config_is_locked_down() {
    let config = json("tauri.conf.json");
    let app = &config["app"];
    let security = &app["security"];

    assert_eq!(app["withGlobalTauri"], false);
    assert_eq!(security["capabilities"], serde_json::json!(["main-window"]));
    assert_eq!(security["freezePrototype"], true);
    assert!(
        security
            .get("dangerousDisableAssetCspModification")
            .is_none()
    );
    assert!(security.get("assetProtocol").is_none());

    let csp = security["csp"].as_str().expect("production CSP is set");
    assert!(csp.contains("script-src 'self';"), "csp: {csp}");
    for forbidden in ["'unsafe-inline'", "'unsafe-eval'", "https:", "ws:", "*"] {
        assert!(
            !csp.contains(forbidden),
            "production CSP must not contain {forbidden}"
        );
    }
    assert!(csp.contains("object-src 'none'"));

    let windows: Vec<&str> = app["windows"]
        .as_array()
        .expect("windows")
        .iter()
        .map(|w| w["label"].as_str().expect("label"))
        .collect();
    assert_eq!(windows, vec!["main"]);

    assert_eq!(
        config["bundle"]["active"], false,
        "BOOT-01 must not produce installers"
    );
    assert!(
        config
            .get("plugins")
            .is_none_or(|p| p.as_object().is_some_and(|o| o.is_empty()))
    );
    let dev_url = config["build"]["devUrl"].as_str().expect("devUrl");
    assert!(
        dev_url.starts_with("http://127.0.0.1:"),
        "dev server must be loopback-only"
    );
}

#[test]
fn no_tauri_plugins_are_linked() {
    let manifest = read("Cargo.toml");
    assert!(
        !manifest.contains("tauri-plugin"),
        "BOOT-01 links no Tauri plugins"
    );
}

#[test]
fn the_vite_dev_server_binds_to_loopback_only() {
    let vite = read("../vite.config.ts");
    assert!(
        vite.contains("host: \"127.0.0.1\""),
        "vite.config.ts must bind to 127.0.0.1"
    );
    assert!(vite.contains("strictPort: true"));
}
