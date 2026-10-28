//! Runs every case in tests/fixtures/protocol/cases.json through the real decoder.

use std::path::PathBuf;

use serde::Deserialize;
use tp_protocol::{Channel, decode};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    #[allow(dead_code)]
    description: String,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    file: String,
    channel: String,
    expect: String,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/protocol")
}

fn channel(name: &str) -> Channel {
    match name {
        "signaling" => Channel::Signaling,
        "control" => Channel::Control,
        other => panic!("unknown channel {other:?} in manifest"),
    }
}

#[test]
fn every_fixture_produces_its_expected_result() {
    let manifest: Manifest = serde_json::from_slice(
        &std::fs::read(fixtures_dir().join("cases.json")).expect("read cases.json"),
    )
    .expect("parse cases.json");
    assert!(
        manifest.cases.len() >= 20,
        "fixture manifest unexpectedly small"
    );

    let mut failures = Vec::new();
    for case in &manifest.cases {
        let bytes = std::fs::read(fixtures_dir().join(&case.file))
            .unwrap_or_else(|e| panic!("read {}: {e}", case.file));
        let got = match decode(&bytes, channel(&case.channel)) {
            Ok(_) => "ok".to_owned(),
            Err(e) => e.kind().as_str().to_owned(),
        };
        if got != case.expect {
            failures.push(format!(
                "{} on {}: expected {}, got {got}",
                case.file, case.channel, case.expect
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "fixture mismatches:\n{}",
        failures.join("\n")
    );
}

#[test]
fn every_fixture_file_is_listed_in_the_manifest() {
    let manifest: Manifest = serde_json::from_slice(
        &std::fs::read(fixtures_dir().join("cases.json")).expect("read cases.json"),
    )
    .expect("parse cases.json");
    let listed: std::collections::BTreeSet<_> =
        manifest.cases.iter().map(|c| c.file.clone()).collect();
    for sub in ["valid", "invalid"] {
        for entry in std::fs::read_dir(fixtures_dir().join(sub)).expect("read fixture dir") {
            let name = format!(
                "{sub}/{}",
                entry.expect("dir entry").file_name().to_string_lossy()
            );
            assert!(
                listed.contains(&name),
                "{name} is not covered by cases.json"
            );
        }
    }
}
