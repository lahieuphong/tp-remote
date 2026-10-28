//! Encoding determinism, round-trips, limits and error sanitisation.

use std::path::PathBuf;

use tp_protocol::{
    Body, Channel, Hello, Message, PROTOCOL_VERSION, ProtocolError, ProtocolErrorKind, decode,
    encode, is_supported,
};

fn hello(seq: u64) -> Message {
    Message {
        seq,
        body: Body::Hello(Hello {
            app_version: "0.1.0".to_owned(),
            min_protocol_version: 1,
            max_protocol_version: 1,
        }),
    }
}

fn canonical_fixture() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/protocol/valid/hello.canonical.json");
    std::fs::read(path).expect("read canonical fixture")
}

#[test]
fn encoding_matches_the_canonical_fixture_byte_for_byte() {
    let bytes = encode(&hello(1), Channel::Signaling).expect("encode");
    assert_eq!(
        String::from_utf8(bytes).expect("utf8"),
        String::from_utf8(canonical_fixture()).expect("utf8")
    );
}

#[test]
fn encoding_is_deterministic_across_calls() {
    let a = encode(&hello(42), Channel::Signaling).expect("encode");
    let b = encode(&hello(42), Channel::Signaling).expect("encode");
    assert_eq!(a, b);
}

#[test]
fn encode_then_decode_round_trips() {
    for seq in [1, 2, 1_000, u64::MAX] {
        let original = hello(seq);
        let bytes = encode(&original, Channel::Signaling).expect("encode");
        assert_eq!(
            decode(&bytes, Channel::Signaling).expect("decode"),
            original
        );
    }
}

#[test]
fn writer_uses_the_current_protocol_version() {
    let bytes = encode(&hello(1), Channel::Signaling).expect("encode");
    let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(value["protocolVersion"], u64::from(PROTOCOL_VERSION));
}

#[test]
fn version_support_is_exactly_v1() {
    assert!(!is_supported(0));
    assert!(is_supported(1));
    assert!(!is_supported(2));
    assert!(!is_supported(u64::MAX));
}

#[test]
fn unsupported_version_reports_the_supported_range() {
    let bytes = br#"{"protocolVersion":9,"type":"hello","seq":1,"payload":{"appVersion":"0.1.0","minProtocolVersion":1,"maxProtocolVersion":9}}"#;
    assert_eq!(
        decode(bytes, Channel::Signaling),
        Err(ProtocolError::UnsupportedVersion {
            got: 9,
            min: 1,
            max: 1
        })
    );
}

#[test]
fn oversized_messages_are_rejected_before_parsing() {
    for channel in [Channel::Signaling, Channel::Control] {
        let max = channel.max_message_bytes();
        // Not even valid JSON: proves the size check runs first.
        let bytes = vec![b'x'; max + 1];
        assert_eq!(
            decode(&bytes, channel),
            Err(ProtocolError::TooLarge { len: max + 1, max })
        );
    }
}

#[test]
fn a_message_at_the_limit_is_not_rejected_for_size() {
    let max = Channel::Control.max_message_bytes();
    let bytes = vec![b' '; max];
    let err = decode(&bytes, Channel::Control).expect_err("whitespace is not a message");
    assert_ne!(err.kind(), ProtocolErrorKind::TooLarge);
}

#[test]
fn encoder_refuses_invalid_messages() {
    let mut bad = hello(1);
    let Body::Hello(h) = &mut bad.body;
    h.min_protocol_version = 5;
    assert_eq!(
        encode(&bad, Channel::Signaling).map_err(|e| e.kind()),
        Err(ProtocolErrorKind::InvalidPayload)
    );
    assert_eq!(
        encode(&hello(0), Channel::Signaling).map_err(|e| e.kind()),
        Err(ProtocolErrorKind::InvalidEnvelope)
    );
    assert_eq!(
        encode(&hello(1), Channel::Control).map_err(|e| e.kind()),
        Err(ProtocolErrorKind::NotAuthorized)
    );
}

#[test]
fn errors_do_not_echo_long_or_control_characters_from_the_peer() {
    let long_type = format!("evil\u{1b}[31m{}", "A".repeat(500));
    let bytes = format!(
        r#"{{"protocolVersion":1,"type":"{}","seq":1,"payload":{{}}}}"#,
        long_type.replace('\u{1b}', "\\u001b")
    );
    match decode(bytes.as_bytes(), Channel::Signaling) {
        Err(ProtocolError::UnknownMessageType { type_name }) => {
            assert!(
                type_name.len() <= 35,
                "echoed name too long: {}",
                type_name.len()
            );
            assert!(!type_name.contains('\u{1b}'));
            assert!(type_name.chars().all(|c| c.is_ascii_graphic() || c == ' '));
        }
        other => panic!("expected UnknownMessageType, got {other:?}"),
    }
}

#[test]
fn deeply_nested_payloads_fail_without_crashing() {
    let depth = 5_000;
    let bytes = format!(
        r#"{{"protocolVersion":1,"type":"hello","seq":1,"payload":{}{}}}"#,
        "[".repeat(depth),
        "]".repeat(depth)
    );
    let err = decode(bytes.as_bytes(), Channel::Signaling).expect_err("must be rejected");
    assert!(matches!(
        err.kind(),
        ProtocolErrorKind::MalformedJson | ProtocolErrorKind::InvalidEnvelope
    ));
}
