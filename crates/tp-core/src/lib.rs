//! App-core: the only crate that depends on every engine crate (ARCH-00 §3.3).
//!
//! In BOOT-01 it answers one question for the shell — what this build is and which
//! engine modules exist — through [`app_info`]. No sessions, pipelines or input gate
//! exist yet.
#![forbid(unsafe_code)]

use tp_contracts::ipc::{AppInfo, BuildProfile, EngineModuleStatus, PlatformInfo, RemoteFeatures};
use tp_contracts::module::{ModuleDescriptor, ModuleState};

/// Roadmap phase this build belongs to.
pub const PHASE: &str = "BOOT-01";

/// Facts only the desktop shell knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellContext {
    pub app_name: String,
    pub app_version: String,
    pub tauri_version: String,
    pub webview_version: Option<String>,
}

/// Every engine crate linked into this build, in dependency order.
/// `tp-test-harness` is excluded: it is test-only and never linked into the app.
pub fn engine_modules() -> Vec<ModuleDescriptor> {
    vec![
        tp_contracts::MODULE,
        tp_protocol::MODULE,
        tp_session::MODULE,
        tp_security::MODULE,
        tp_capture::MODULE,
        tp_input::MODULE,
        tp_media::MODULE,
        tp_transport::MODULE,
        tp_signaling::MODULE,
        tp_diagnostics::MODULE,
        MODULE,
    ]
}

pub fn platform_info() -> PlatformInfo {
    PlatformInfo {
        os: std::env::consts::OS.to_owned(),
        family: std::env::consts::FAMILY.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
    }
}

pub fn build_profile() -> BuildProfile {
    if cfg!(debug_assertions) {
        BuildProfile::Debug
    } else {
        BuildProfile::Release
    }
}

/// Builds the `get_app_info` response.
pub fn app_info(shell: ShellContext) -> AppInfo {
    AppInfo {
        app_name: shell.app_name,
        app_version: shell.app_version,
        phase: PHASE.to_owned(),
        platform: platform_info(),
        build_profile: build_profile(),
        protocol: tp_protocol::support(),
        shell: tp_contracts::ipc::ShellInfo {
            tauri_version: shell.tauri_version,
            webview_version: shell.webview_version,
        },
        remote_features: RemoteFeatures::NotImplemented,
        engine_modules: engine_modules()
            .into_iter()
            .map(EngineModuleStatus::from)
            .collect(),
    }
}

/// Crates whose implementation would give the build a remote-desktop capability.
/// `RemoteFeatures` must change deliberately once any of them stops being Foundation.
#[cfg(test)]
const REMOTE_CAPABILITY_CRATES: &[&str] = &[
    "tp-session",
    "tp-security",
    "tp-capture",
    "tp-input",
    "tp-media",
    "tp-transport",
    "tp-signaling",
];

/// Descriptor for this crate.
pub const MODULE: ModuleDescriptor = ModuleDescriptor {
    crate_name: "tp-core",
    role: "app-core",
    state: ModuleState::Partial,
    owner_phase: "NET-03",
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn shell() -> ShellContext {
        ShellContext {
            app_name: "TP Remote".to_owned(),
            app_version: "0.1.0".to_owned(),
            tauri_version: "2.12.5".to_owned(),
            webview_version: Some("155.0.4283.62".to_owned()),
        }
    }

    #[test]
    fn engine_modules_cover_every_engine_crate_exactly_once() {
        let names: Vec<_> = engine_modules().iter().map(|m| m.crate_name).collect();
        let unique: BTreeSet<_> = names.iter().copied().collect();
        assert_eq!(names.len(), unique.len(), "duplicate module: {names:?}");
        let expected: BTreeSet<_> = [
            "tp-contracts",
            "tp-protocol",
            "tp-session",
            "tp-security",
            "tp-capture",
            "tp-input",
            "tp-media",
            "tp-transport",
            "tp-signaling",
            "tp-diagnostics",
            "tp-core",
        ]
        .into_iter()
        .collect();
        assert_eq!(unique, expected);
    }

    #[test]
    fn no_remote_capability_crate_claims_an_implementation_in_boot01() {
        for m in engine_modules() {
            if REMOTE_CAPABILITY_CRATES.contains(&m.crate_name) {
                assert_eq!(
                    m.state,
                    ModuleState::Foundation,
                    "{} must stay Foundation",
                    m.crate_name
                );
            }
        }
    }

    #[test]
    fn app_info_reports_this_build_honestly() {
        let info = app_info(shell());
        assert_eq!(info.phase, "BOOT-01");
        assert_eq!(info.remote_features, RemoteFeatures::NotImplemented);
        assert_eq!(info.protocol, tp_protocol::support());
        assert_eq!(info.platform.os, std::env::consts::OS);
        assert_eq!(info.platform.arch, std::env::consts::ARCH);
        assert_eq!(info.build_profile, build_profile());
        assert_eq!(info.app_version, "0.1.0");
        assert_eq!(info.shell.webview_version.as_deref(), Some("155.0.4283.62"));
        assert_eq!(info.engine_modules.len(), engine_modules().len());
    }

    #[test]
    fn build_profile_matches_debug_assertions() {
        let expected = if cfg!(debug_assertions) {
            BuildProfile::Debug
        } else {
            BuildProfile::Release
        };
        assert_eq!(build_profile(), expected);
    }
}
