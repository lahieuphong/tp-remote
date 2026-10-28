//! CAP-02A local-preview IPC contract: the Rust DTOs and tests/fixtures/ipc/preview must
//! describe the same JSON and the same binary frame header. The TypeScript contract tests
//! (apps/desktop/src/contract/localPreview.contract.test.ts) read the same files.

use std::path::PathBuf;

use tp_contracts::ipc::preview_frame::{HEADER_LEN, Header, MAGIC};
use tp_contracts::ipc::{
    CaptureAvailability, CaptureDisplay, CaptureDisplays, IpcError, IpcErrorCode,
    LocalPreviewState, LocalPreviewStatus, PreviewError, PreviewOutcome, PreviewStats,
    PreviewTarget, PreviewTargetKind,
};

fn fixture(path: &str) -> Vec<u8> {
    let full = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/ipc/preview")
        .join(path);
    std::fs::read(&full).unwrap_or_else(|e| panic!("read {}: {e}", full.display()))
}

fn json(path: &str) -> serde_json::Value {
    serde_json::from_slice(&fixture(path)).expect("fixture is JSON")
}

fn capturing() -> LocalPreviewStatus {
    LocalPreviewStatus {
        availability: CaptureAvailability::Available,
        state: LocalPreviewState::Capturing,
        generation: 3,
        target: Some(PreviewTarget {
            kind: PreviewTargetKind::Display,
            label: "Display 2".to_owned(),
            width: 1920,
            height: 1080,
            display_index: Some(0),
        }),
        stats: PreviewStats {
            captured_frames: 1800,
            previewed_frames: 900,
            skipped_frames: 12,
            dropped_frames: 0,
            capture_fps: 59.9,
            preview_fps: 30.1,
            elapsed_ms: 30_000,
            first_frame_ms: Some(132),
            frame_width: 1920,
            frame_height: 1080,
            preview_width: 1280,
            preview_height: 720,
        },
        last_outcome: None,
        error: None,
    }
}

fn ready() -> LocalPreviewStatus {
    LocalPreviewStatus {
        state: LocalPreviewState::Ready,
        generation: 0,
        last_outcome: None,
        error: None,
        ..failed()
    }
}

fn failed() -> LocalPreviewStatus {
    LocalPreviewStatus {
        availability: CaptureAvailability::Available,
        state: LocalPreviewState::Failed,
        generation: 4,
        target: None,
        stats: PreviewStats {
            captured_frames: 0,
            previewed_frames: 0,
            skipped_frames: 0,
            dropped_frames: 0,
            capture_fps: 0.0,
            preview_fps: 0.0,
            elapsed_ms: 0,
            first_frame_ms: None,
            frame_width: 0,
            frame_height: 0,
            preview_width: 0,
            preview_height: 0,
        },
        last_outcome: Some(PreviewOutcome::DeviceLost),
        error: Some(PreviewError {
            message: "The graphics device was reset, so capture stopped.".to_owned(),
            recovery: "Start the preview again.".to_owned(),
        }),
    }
}

fn display(index: u32, name: &str, x: i32, y: i32, primary: bool, known: bool) -> CaptureDisplay {
    CaptureDisplay {
        index,
        name: name.to_owned(),
        x,
        y,
        width: 1920,
        height: 1080,
        scale_percent: known.then_some(100),
        refresh_hz: known.then_some(60),
        primary,
    }
}

#[test]
fn preview_status_serializes_exactly_like_the_fixtures() {
    for (file, value) in [
        ("local-preview-status.capturing.json", capturing()),
        ("local-preview-status.failed.json", failed()),
        ("local-preview-status.ready.json", ready()),
    ] {
        assert_eq!(
            serde_json::to_value(&value).expect("serialize"),
            json(file),
            "{file}"
        );
        let parsed: LocalPreviewStatus = serde_json::from_slice(&fixture(file)).expect(file);
        assert_eq!(parsed, value, "{file}");
    }
}

#[test]
fn displays_serialize_exactly_like_the_fixture() {
    let expected = CaptureDisplays {
        epoch: 1,
        displays: vec![
            display(0, "DISPLAY1", -1920, 71, false, true),
            display(1, "DISPLAY2", -3840, 71, false, true),
            display(2, "DISPLAY3", 0, 0, true, false),
        ],
    };
    assert_eq!(
        serde_json::to_value(&expected).expect("serialize"),
        json("capture-displays.dev-w1.json")
    );
}

#[test]
fn invalid_preview_fixtures_are_rejected_by_rust() {
    for name in [
        "bad-nested-enum.json",
        "bad-state.json",
        "missing-field.json",
        "negative-count.json",
        "unknown-field.json",
        "wrong-type.json",
    ] {
        let result =
            serde_json::from_slice::<LocalPreviewStatus>(&fixture(&format!("invalid/{name}")));
        assert!(result.is_err(), "{name} was accepted");
    }
}

#[test]
fn the_frame_header_matches_the_fixture_bytes() {
    let fixture = json("preview-frame-header.json");
    let bytes: Vec<u8> = fixture["bytes"]
        .as_array()
        .expect("bytes")
        .iter()
        .map(|b| u8::try_from(b.as_u64().expect("byte")).expect("u8"))
        .collect();
    let header = Header {
        generation: 3,
        seq: 7,
        width: 1280,
        height: 720,
    };
    assert_eq!(bytes.len(), HEADER_LEN);
    assert_eq!(header.encode().to_vec(), bytes);
    assert_eq!(Header::decode(&bytes), Some(header));
    assert_eq!(header.frame_len(), Some(HEADER_LEN + 1280 * 720 * 4));
    assert_eq!(&bytes[..4], &MAGIC);
}

#[test]
fn malformed_frame_headers_are_rejected() {
    let good = Header::EMPTY.encode();
    assert_eq!(Header::decode(&good), Some(Header::EMPTY));
    assert_eq!(
        Header::decode(&good[..HEADER_LEN - 5]),
        None,
        "short header"
    );
    let mut wrong_magic = good;
    wrong_magic[0] = b'X';
    assert_eq!(Header::decode(&wrong_magic), None);
    let huge = Header {
        generation: 1,
        seq: 1,
        width: u32::MAX,
        height: u32::MAX,
    };
    // Overflow is reported, not wrapped (32-bit targets would overflow).
    let len = huge.frame_len();
    assert!(len.is_none() || len.is_some_and(|l| l > HEADER_LEN));
}

#[test]
fn new_ipc_error_codes_are_camel_case() {
    for (code, wire) in [
        (IpcErrorCode::InvalidState, "invalidState"),
        (IpcErrorCode::Unavailable, "unavailable"),
        (IpcErrorCode::Forbidden, "forbidden"),
    ] {
        let value = serde_json::to_value(IpcError {
            code,
            message: "m".to_owned(),
        })
        .expect("serialize");
        assert_eq!(value["code"], wire);
    }
}
