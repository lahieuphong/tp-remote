//! Generated TypeScript bindings: determinism, shape and drift against the committed files.

use std::path::PathBuf;

use tp_test_harness::bindings::{TS_RS_HEADER, check, generate};

fn committed_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/desktop/src/bindings")
}

fn contents_of(name: &str) -> String {
    generate()
        .expect("generate")
        .into_iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("{name} not generated"))
        .contents
}

#[test]
fn generation_is_deterministic() {
    assert_eq!(generate().expect("first"), generate().expect("second"));
}

#[test]
fn every_type_file_carries_the_ts_rs_header() {
    for file in generate().expect("generate") {
        if file.name != "index.ts" {
            assert!(
                file.contents.starts_with(TS_RS_HEADER),
                "{} lacks the ts-rs header",
                file.name
            );
        }
    }
}

#[test]
fn app_info_binding_matches_the_camel_case_json_shape() {
    let app_info = contents_of("AppInfo.ts");
    for field in [
        "appName: string",
        "appVersion: string",
        "phase: string",
        "platform: PlatformInfo",
        "buildProfile: BuildProfile",
        "protocol: ProtocolSupport",
        "shell: ShellInfo",
        "remoteFeatures: RemoteFeatures",
        "engineModules: Array<EngineModuleStatus>",
    ] {
        assert!(
            app_info.contains(field),
            "AppInfo.ts lacks `{field}`:\n{app_info}"
        );
    }
    assert!(contents_of("ShellInfo.ts").contains("webviewVersion: string | null"));
    assert!(contents_of("BuildProfile.ts").contains("\"debug\" | \"release\""));
    assert!(contents_of("RemoteFeatures.ts").contains("\"notImplemented\""));
}

#[test]
fn no_binding_uses_bigint() {
    for file in generate().expect("generate") {
        assert!(
            !file.contents.contains("bigint"),
            "{} uses bigint",
            file.name
        );
    }
}

#[test]
fn committed_bindings_have_not_drifted() {
    if let Err(drift) = check(&committed_dir()) {
        panic!(
            "apps/desktop/src/bindings drifted; run `npm run bindings:generate`:\n{}",
            drift.join("\n")
        );
    }
}
