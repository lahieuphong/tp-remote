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
