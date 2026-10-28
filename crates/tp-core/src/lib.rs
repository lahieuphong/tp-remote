//! App-core: the only crate that depends on every engine crate (ARCH-00 §3.3).
//!
//! IMPLEMENTED (BOOT-02): build information for the shell ([`app_info`]), the honest
//! capability advertisement ([`platform_capabilities`]), the [`service::SessionService`]
//! that combines the session registry with the security policy behind the
//! `SessionManager` contract, the [`gate::InputGate`], and a monotonic clock.
//!
//! IMPLEMENTED (CAP-02A): [`preview`], the local self-preview: the user picks a display
//! or window in the Windows capture picker and sees it inside TP Remote. It is local
//! only and has its own authorization ([`preview::LocalPreviewGrant`], ADR-007).
//!
//! NOT IMPLEMENTED: remote viewing, input injection, networking and consent prompts. The
//! remote capability advertisement stays empty: a local preview is not a remote
//! `ViewScreen` capability.
#![forbid(unsafe_code)]

pub mod clock;
pub mod gate;
pub mod preview;
pub mod service;

use std::sync::Arc;

use tp_contracts::capability::PlatformCapabilities;
use tp_contracts::ipc::{AppInfo, BuildProfile, EngineModuleStatus, PlatformInfo, RemoteFeatures};
use tp_contracts::module::{ModuleDescriptor, ModuleState};

/// Roadmap phase this build belongs to.
pub const PHASE: &str = "CAP-02A";

/// The local preview service on this operating system's capture provider.
pub type NativePreview = preview::LocalPreview<tp_capture::native::Provider>;

/// Creates the local preview service with the real clock and provider.
pub fn native_preview(logger: tp_diagnostics::Logger) -> NativePreview {
    preview::LocalPreview::new(
        tp_capture::native::Provider::new(),
        Arc::new(clock::MonotonicClock::new()),
        logger,
        preview::PreviewConfig::default(),
    )
}

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

/// What this build can provide to a *peer*. Nothing: CAP-02A captures for the local
/// preview only, and there is no transport, consent flow or input yet (NET-03, CTRL-05).
pub const fn platform_capabilities() -> PlatformCapabilities {
    PlatformCapabilities::NONE
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

/// Crates whose implementation would give the build a remote-desktop capability
/// (capture, input, codec, transport, signaling). `RemoteFeatures` and
/// `platform_capabilities` must change deliberately once any of them is implemented.
#[cfg(test)]
const CAPABILITY_CRATES: &[&str] = &[
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
    use tp_contracts::capability::{Availability, Capability};

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

    // T-CONTRACT-05 (amended in CAP-02A): only tp-capture has an implementation (Windows
    // local capture); nothing is advertised to peers, because a local preview is not a
    // remote capability.
    #[test]
    fn no_capability_is_advertised_and_only_capture_has_progressed() {
        for m in engine_modules() {
            if CAPABILITY_CRATES.contains(&m.crate_name) {
                let expected = if m.crate_name == "tp-capture" && cfg!(windows) {
                    ModuleState::Partial
                } else {
                    ModuleState::Foundation
                };
                assert_eq!(m.state, expected, "{} state changed", m.crate_name);
            }
        }
        let platform = platform_capabilities();
        assert_eq!(platform, PlatformCapabilities::NONE);
        assert!(platform.advertised().is_empty());
        for capability in Capability::ALL {
            assert_eq!(
                platform.availability(capability),
                Availability::NotImplemented
            );
        }
    }

    #[test]
    fn app_info_reports_this_build_honestly() {
        let info = app_info(shell());
        assert_eq!(info.phase, "CAP-02A");
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

    #[test]
    fn the_monotonic_clock_never_goes_backwards() {
        use tp_contracts::time::Clock;
        let clock = clock::MonotonicClock::new();
        let a = clock.now();
        let b = clock.now();
        assert!(b >= a);
    }
}
