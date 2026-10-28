//! T-CONTRACT-06: message types reserved for later phases cannot decode into a message on
//! any channel, and protocol errors map to the typed error model without peer data.

use tp_contracts::error::{CoreError, ErrorCategory};
use tp_protocol::{
    Channel, MessageKind, ProtocolError, ProtocolErrorKind, RESERVED_MESSAGE_TYPES, decode,
};

fn envelope(type_name: &str) -> String {
    format!(r#"{{"protocolVersion":1,"type":"{type_name}","seq":1,"payload":{{}}}}"#)
}

#[test]
fn reserved_types_are_rejected_on_every_channel() {
    assert!(RESERVED_MESSAGE_TYPES.len() >= 20);
    for type_name in RESERVED_MESSAGE_TYPES {
        assert_eq!(
            MessageKind::from_wire(type_name),
            None,
            "{type_name} must have no message kind"
        );
        for channel in [Channel::Signaling, Channel::Control] {
            let error = decode(envelope(type_name).as_bytes(), channel).expect_err("reserved");
            assert_eq!(
                error.kind(),
                ProtocolErrorKind::NotAuthorized,
                "{type_name} on {channel:?}"
            );
        }
    }
}

#[test]
fn input_and_session_commands_are_among_the_reserved_types() {
    for type_name in [
        "inputBatch",
        "sessionRequest",
        "consentResult",
        "sdpOffer",
        "selectDisplay",
    ] {
        assert!(RESERVED_MESSAGE_TYPES.contains(&type_name), "{type_name}");
    }
}

#[test]
fn reserved_types_with_a_plausible_payload_are_still_rejected() {
    let bytes = br#"{"protocolVersion":1,"type":"inputBatch","seq":3,"payload":{"events":[{"kind":"key","usage":4,"pressed":true}]}}"#;
    assert_eq!(
        decode(bytes, Channel::Control).map_err(|e| e.kind()),
        Err(ProtocolErrorKind::NotAuthorized)
    );
}

// T-LOG-04: protocol errors become CoreErrors with static context only.
#[test]
fn protocol_errors_map_to_categories_without_peer_text() {
    let cases = [
        (
            ProtocolError::UnsupportedVersion {
                got: 9,
                min: 1,
                max: 1,
            },
            ErrorCategory::ProtocolMismatch,
        ),
        (
            ProtocolError::TooLarge { len: 1, max: 0 },
            ErrorCategory::InvalidPayload,
        ),
        (ProtocolError::MalformedJson, ErrorCategory::InvalidPayload),
        (
            ProtocolError::InvalidEnvelope("PEER-TEXT-SYNTHETIC".into()),
            ErrorCategory::InvalidPayload,
        ),
        (
            ProtocolError::UnknownMessageType {
                type_name: "PEER-TEXT-SYNTHETIC".into(),
            },
            ErrorCategory::InvalidPayload,
        ),
        (
            ProtocolError::NotAuthorized {
                type_name: "PEER-TEXT-SYNTHETIC".into(),
            },
            ErrorCategory::InvalidPayload,
        ),
        (
            ProtocolError::InvalidPayload("PEER-TEXT-SYNTHETIC".into()),
            ErrorCategory::InvalidPayload,
        ),
        (ProtocolError::Encode, ErrorCategory::InternalError),
    ];
    for (protocol_error, category) in cases {
        let error = CoreError::from(protocol_error);
        assert_eq!(error.category(), category);
        let shown = format!("{error} {error:?} {}", error.public().message);
        assert!(!shown.contains("PEER-TEXT-SYNTHETIC"), "{shown}");
    }
}
