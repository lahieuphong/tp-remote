//! Enforces the crate dependency rules of ARCH-00 §3.3 on the real workspace manifests.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

const DESKTOP: &str = "tp-desktop";
const HARNESS: &str = "tp-test-harness";

/// The 12 crates defined by ARCH-00 §3.3 / §3.4.
const ARCH_CRATES: [&str; 12] = [
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
    HARNESS,
];

/// Internal crates each workspace member may depend on (normal and build dependencies).
fn allowed_internal_deps() -> BTreeMap<&'static str, BTreeSet<&'static str>> {
    let engine: BTreeSet<&str> = ARCH_CRATES
        .iter()
        .copied()
        .filter(|c| *c != HARNESS && *c != "tp-core")
        .collect();
    let set = |items: &[&'static str]| items.iter().copied().collect::<BTreeSet<_>>();
    BTreeMap::from([
        ("tp-contracts", set(&[])),
        ("tp-protocol", set(&["tp-contracts"])),
        ("tp-session", set(&["tp-contracts", "tp-protocol"])),
        ("tp-security", set(&["tp-contracts", "tp-protocol"])),
        ("tp-capture", set(&["tp-contracts"])),
        ("tp-input", set(&["tp-contracts"])),
        ("tp-media", set(&["tp-contracts"])),
        ("tp-transport", set(&["tp-contracts", "tp-protocol"])),
        ("tp-signaling", set(&["tp-contracts", "tp-protocol"])),
        ("tp-diagnostics", set(&["tp-contracts"])),
        ("tp-core", engine.clone()),
        (HARNESS, engine.iter().copied().chain(["tp-core"]).collect()),
        (DESKTOP, set(&["tp-core", "tp-contracts", "tp-diagnostics"])),
    ])
}

/// Crates that must never contain unsafe code (ARCH-00 §3.3 rule 5).
const FORBID_UNSAFE: [&str; 6] = [
    "tp-contracts",
    "tp-protocol",
    "tp-session",
    "tp-signaling",
    "tp-diagnostics",
    "tp-core",
];

/// Platform crates stay synchronous and runtime-agnostic (ARCH-00 §3.3 rule 2).
const PLATFORM_CRATES: [&str; 3] = ["tp-capture", "tp-input", "tp-media"];

struct Package {
    name: String,
    manifest_dir: PathBuf,
    /// (dependency name, kind) where kind is "normal", "dev" or "build".
    deps: Vec<(String, String)>,
}

fn workspace_packages() -> Vec<Package> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
        ])
        .current_dir(&root)
        .output()
        .expect("run cargo metadata");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let meta: Value = serde_json::from_slice(&output.stdout).expect("parse cargo metadata");
    meta["packages"]
        .as_array()
        .expect("packages array")
        .iter()
        .map(|p| Package {
            name: p["name"].as_str().expect("name").to_owned(),
            manifest_dir: PathBuf::from(p["manifest_path"].as_str().expect("manifest_path"))
                .parent()
                .expect("manifest dir")
                .to_path_buf(),
            deps: p["dependencies"]
                .as_array()
                .expect("dependencies")
                .iter()
                .map(|d| {
                    (
                        d["name"].as_str().expect("dep name").to_owned(),
                        d["kind"].as_str().unwrap_or("normal").to_owned(),
                    )
                })
                .collect(),
        })
        .collect()
}

#[test]
fn workspace_contains_exactly_the_architecture_crates_and_the_shell() {
    let names: BTreeSet<String> = workspace_packages().into_iter().map(|p| p.name).collect();
    let expected: BTreeSet<String> = ARCH_CRATES
        .iter()
        .chain([DESKTOP].iter())
        .map(|s| (*s).to_owned())
        .collect();
    assert_eq!(names, expected);
}

#[test]
fn internal_dependencies_follow_the_allowed_edges() {
    let allowed = allowed_internal_deps();
    let internal: BTreeSet<String> = allowed.keys().map(|k| (*k).to_owned()).collect();
    let mut violations = Vec::new();
    for package in workspace_packages() {
        let permitted = allowed
            .get(package.name.as_str())
            .unwrap_or_else(|| panic!("no rule for {}", package.name));
        for (dep, kind) in &package.deps {
            if !internal.contains(dep) {
                continue;
            }
            if dep == HARNESS {
                violations.push(format!("{} depends on {HARNESS} ({kind})", package.name));
            } else if kind != "dev" && !permitted.contains(dep.as_str()) {
                violations.push(format!("{} -> {dep} ({kind}) is not allowed", package.name));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "boundary violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn internal_dependency_graph_is_acyclic() {
    let packages = workspace_packages();
    let internal: BTreeSet<&str> = packages.iter().map(|p| p.name.as_str()).collect();
    let graph: BTreeMap<&str, Vec<&str>> = packages
        .iter()
        .map(|p| {
            let edges = p
                .deps
                .iter()
                .filter(|(d, kind)| kind != "dev" && internal.contains(d.as_str()))
                .map(|(d, _)| d.as_str())
                .collect();
            (p.name.as_str(), edges)
        })
        .collect();

    fn visit<'a>(
        node: &'a str,
        graph: &BTreeMap<&'a str, Vec<&'a str>>,
        stack: &mut Vec<&'a str>,
        done: &mut BTreeSet<&'a str>,
    ) {
        if done.contains(node) {
            return;
        }
        assert!(
            !stack.contains(&node),
            "dependency cycle: {stack:?} -> {node}"
        );
        stack.push(node);
        for next in graph.get(node).into_iter().flatten() {
            visit(next, graph, stack, done);
        }
        stack.pop();
        done.insert(node);
    }

    let mut done = BTreeSet::new();
    for node in graph.keys() {
        visit(node, &graph, &mut Vec::new(), &mut done);
    }
}

#[test]
fn only_the_shell_depends_on_tauri() {
    for package in workspace_packages() {
        let uses_tauri = package
            .deps
            .iter()
            .any(|(d, kind)| kind != "dev" && (d == "tauri" || d == "tauri-build"));
        assert_eq!(
            uses_tauri,
            package.name == DESKTOP,
            "{} tauri dependency rule violated",
            package.name
        );
    }
}

#[test]
fn platform_crates_have_no_async_runtime() {
    for package in workspace_packages() {
        if PLATFORM_CRATES.contains(&package.name.as_str()) {
            for (dep, _) in &package.deps {
                assert!(
                    !matches!(dep.as_str(), "tokio" | "async-std" | "smol"),
                    "{} must stay runtime-agnostic but depends on {dep}",
                    package.name
                );
            }
        }
    }
}

#[test]
fn designated_crates_forbid_unsafe_code() {
    for package in workspace_packages() {
        if FORBID_UNSAFE.contains(&package.name.as_str()) {
            let lib = std::fs::read_to_string(package.manifest_dir.join("src/lib.rs"))
                .expect("read lib.rs");
            assert!(
                lib.contains("#![forbid(unsafe_code)]"),
                "{} must declare #![forbid(unsafe_code)]",
                package.name
            );
        }
    }
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Non-test Rust sources: `src/` of every crate and of the shell, excluding `tests.rs`
/// files that are compiled only under `#[cfg(test)]`.
fn production_sources() -> Vec<(PathBuf, String)> {
    fn walk(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs")
                && path.file_name().is_some_and(|n| n != "tests.rs")
            {
                out.push(path);
            }
        }
    }
    let root = workspace_root();
    let mut dirs = vec![root.join("apps/desktop/src-tauri/src")];
    for entry in std::fs::read_dir(root.join("crates")).expect("crates") {
        dirs.push(entry.expect("entry").path().join("src"));
    }
    let mut files = Vec::new();
    for dir in dirs {
        walk(&dir, &mut files);
    }
    files
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).expect("read source");
            let normalized = path.to_string_lossy().replace('\\', "/");
            (PathBuf::from(normalized), text)
        })
        .collect()
}

fn code_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines()
        .enumerate()
        .map(|(n, line)| (n + 1, line.split("//").next().unwrap_or("")))
}

// T-STA-08 / ARCH-00 §4.3: only tp-core's input gate may call InputController::inject.
#[test]
fn only_the_input_gate_calls_inject() {
    let mut calls_in_gate = 0;
    for (path, text) in production_sources() {
        for (n, code) in code_lines(&text) {
            if code.contains(".inject(") {
                let display = path.display().to_string();
                assert!(
                    display.ends_with("crates/tp-core/src/gate.rs"),
                    "{display}:{n} calls inject outside the input gate"
                );
                calls_in_gate += 1;
            }
        }
    }
    assert_eq!(
        calls_in_gate, 1,
        "the gate itself must call inject exactly once"
    );
}

// T-CONTRACT-05 (amended in CAP-02A): only the capture contracts have production
// implementations, and only inside tp-capture (Windows provider; the uninhabited
// "not implemented" provider for other systems). Every other contract is still
// interface-only. A phase that adds one must change these lists deliberately.
#[test]
fn production_platform_implementations_exist_only_where_allowed() {
    const NOT_YET_IMPLEMENTED: [&str; 7] = [
        "InputController",
        "VideoEncoder",
        "VideoDecoder",
        "Transport",
        "TransportSession",
        "PermissionManager",
        "DeviceIdentity",
    ];
    const CAPTURE: [&str; 4] = [
        "CaptureProvider",
        "CaptureStream",
        "CapturePicker",
        "FrameReadback",
    ];
    let implements = |code: &str, name: &str| {
        code.contains(&format!(" {name} for ")) || code.contains(&format!("::{name} for "))
    };
    let mut capture_impls = Vec::new();
    for (path, text) in production_sources() {
        let display = path.display().to_string();
        for (n, code) in code_lines(&text) {
            let code = code.trim_start();
            if !code.starts_with("impl") {
                continue;
            }
            for name in NOT_YET_IMPLEMENTED {
                assert!(
                    !implements(code, name),
                    "{display}:{n}: production implementation of {name}"
                );
            }
            for name in CAPTURE {
                if implements(code, name) {
                    assert!(
                        display.contains("crates/tp-capture/src/wgc/")
                            || display.ends_with("crates/tp-capture/src/unsupported.rs"),
                        "{display}:{n}: {name} implemented outside tp-capture"
                    );
                    capture_impls.push(format!("{name} @ {display}"));
                }
            }
        }
    }
    assert!(
        capture_impls
            .iter()
            .any(|i| i.contains("CaptureProvider") && i.contains("/wgc/")),
        "the Windows provider is missing: {capture_impls:?}"
    );
}

/// Lines of production code (comments stripped) that contain `needle`, as `path:line`.
fn occurrences(needle: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (path, text) in production_sources() {
        for (n, code) in code_lines(&text) {
            if code.contains(needle) {
                found.push(format!("{}:{n}", path.display()));
            }
        }
    }
    found
}

// T-CAP-RUN-04 (static part): the Windows capture border is never disabled and borderless
// access is never requested anywhere in production code.
#[test]
fn the_capture_border_is_never_disabled() {
    for needle in [
        "IsBorderRequired",
        "Borderless",
        "GraphicsCaptureAccess",
        "graphicsCaptureWithoutBorder",
    ] {
        assert_eq!(occurrences(needle), Vec::<String>::new(), "{needle} used");
    }
}

// Capture sessions are created in exactly one audited place, from the target passed to
// `start`.
#[test]
fn capture_sessions_are_created_in_one_place() {
    let sessions = occurrences("CreateCaptureSession(");
    assert_eq!(sessions.len(), 1, "{sessions:?}");
    assert!(
        sessions[0].contains("crates/tp-capture/src/wgc/stream.rs"),
        "{sessions:?}"
    );
}

// ADR-007: production targets come only from the system picker. Window targets and
// programmatic monitor targets exist only in the test-support module; monitor items are
// otherwise created only to read their names (target.rs).
#[test]
fn programmatic_capture_targets_are_test_only() {
    let windows = occurrences("CreateForWindow(");
    assert!(
        windows
            .iter()
            .all(|w| w.contains("crates/tp-capture/src/wgc/testing.rs")),
        "{windows:?}"
    );
    let monitors = occurrences("CreateForMonitor(");
    assert!(
        monitors
            .iter()
            .all(|m| m.contains("crates/tp-capture/src/wgc/testing.rs")
                || m.contains("crates/tp-capture/src/wgc/target.rs")),
        "{monitors:?}"
    );
    let programmatic = occurrences("WgcTarget::programmatic(");
    assert!(
        programmatic
            .iter()
            .all(|p| p.contains("crates/tp-capture/src/wgc/testing.rs")),
        "{programmatic:?}"
    );
    let picker_targets = occurrences("WgcTarget::from_picker(");
    assert_eq!(picker_targets.len(), 1, "{picker_targets:?}");
    assert!(picker_targets[0].contains("crates/tp-capture/src/wgc/picker.rs"));
    let module = std::fs::read_to_string(workspace_root().join("crates/tp-capture/src/wgc/mod.rs"))
        .expect("read wgc/mod.rs");
    assert!(
        module.contains("#[cfg(feature = \"test-support\")]\npub mod testing;")
            || module.contains("#[cfg(feature = \"test-support\")]\r\npub mod testing;"),
        "the testing module must be gated by test-support"
    );
}

// ADR-007: a LocalPreviewGrant is minted in exactly one place, the picker-completion path.
#[test]
fn only_the_picker_completion_mints_a_preview_grant() {
    let mints = occurrences("LocalPreviewGrant::on_picker_selection(");
    assert_eq!(mints.len(), 1, "{mints:?}");
    assert!(
        mints[0].contains("crates/tp-core/src/preview/host.rs"),
        "{mints:?}"
    );
}

// ARCH-00 §3.3 rule 5 / CAP-02A: unsafe code exists only in tp-capture's Windows module,
// and every unsafe block or function there is preceded by a SAFETY comment.
#[test]
fn unsafe_code_is_confined_and_justified() {
    for (path, text) in production_sources() {
        let display = path.display().to_string();
        let lines: Vec<&str> = text.lines().collect();
        for (n, code) in code_lines(&text) {
            let has_unsafe = code
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .any(|word| word == "unsafe");
            if !has_unsafe {
                continue;
            }
            assert!(
                display.contains("crates/tp-capture/src/wgc/"),
                "{display}:{n}: unsafe outside crates/tp-capture/src/wgc"
            );
            let start = n.saturating_sub(7);
            let justified = lines[start..n].iter().any(|l| l.contains("SAFETY"));
            assert!(justified, "{display}:{n}: unsafe without a SAFETY comment");
        }
    }
}

// CAP-02A: nothing in the capture or preview path writes files (frames are never stored).
#[test]
fn the_capture_and_preview_path_writes_no_files() {
    for (path, text) in production_sources() {
        let display = path.display().to_string();
        let in_scope = display.contains("crates/tp-capture/src/")
            || display.contains("crates/tp-core/src/preview/")
            || display.contains("apps/desktop/src-tauri/src/");
        if !in_scope {
            continue;
        }
        for (n, code) in code_lines(&text) {
            for needle in [
                "std::fs",
                "File::create",
                "OpenOptions",
                "fs::write",
                "CreateFileW",
            ] {
                assert!(!code.contains(needle), "{display}:{n}: {needle}");
            }
        }
    }
}

// T-STA-10: the desktop app's own build graph never enables test-only features or crates.
#[test]
fn the_app_build_graph_has_no_test_support() {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = Command::new(cargo)
        .args([
            "tree",
            "-p",
            DESKTOP,
            "-e",
            "normal,build",
            "--prefix",
            "none",
            "-f",
            "{p}|{f}",
            "--offline",
            "--locked",
        ])
        .current_dir(workspace_root())
        .output()
        .expect("run cargo tree");
    assert!(
        output.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tree = String::from_utf8_lossy(&output.stdout);
    assert!(
        tree.lines().any(|l| l.starts_with("tp-security ")),
        "tree looks empty:\n{tree}"
    );
    for line in tree.lines() {
        assert!(
            !line.contains("test-support"),
            "test-only feature in the app build: {line}"
        );
        assert!(
            !line.starts_with("tp-test-harness "),
            "harness in the app build: {line}"
        );
        assert!(
            !line.starts_with("ts-rs "),
            "ts-rs in the app build: {line}"
        );
        assert!(
            !line.contains("ts-bindings"),
            "binding generation in the app build: {line}"
        );
    }
    // CAP-02A: tp-capture uses the windows crate Tauri already uses; no second copy.
    let windows_versions: BTreeSet<&str> = tree
        .lines()
        .filter_map(|l| l.split('|').next())
        .filter(|p| p.starts_with("windows v"))
        .collect();
    assert!(
        windows_versions.len() <= 1,
        "several windows crate versions: {windows_versions:?}"
    );
}
