//! Shared primitives: identifiers, secrets, the error model, capabilities, cancellation
//! and redacted Debug output.

use std::collections::HashSet;
use std::time::Duration;

use tp_contracts::cancel::CancelToken;
use tp_contracts::capability::{Availability, Capability, PlatformCapabilities, Scope};
use tp_contracts::codec::{Codec, EncodedFrame};
use tp_contracts::error::{CoreError, ErrorCategory, ErrorClass};
use tp_contracts::id::{DeviceId, Generation, IdError, SessionId};
use tp_contracts::identity::{IdentityPublicKey, Signature};
use tp_contracts::secret::{Secret, Wipe};
use tp_contracts::time::MonoTime;
use tp_contracts::transport::{Fingerprint, TransportEvent};

const HEX: &str = "0123456789abcdef0123456789abcdef";

// "Reject invalid identifiers" (BOOT-02 §6).
#[test]
fn identifiers_accept_only_32_lowercase_hex_characters() {
    let id = SessionId::parse(HEX).expect("valid");
    assert_eq!(id.to_hex(), HEX);
    assert_eq!(id.tag(), "01234567");
    assert_eq!(
        SessionId::parse(&HEX.to_uppercase()),
        Err(IdError::NotLowercaseHex)
    );
    assert_eq!(SessionId::parse(&HEX[..31]), Err(IdError::WrongLength));
    assert_eq!(
        SessionId::parse(&format!("{HEX}0")),
        Err(IdError::WrongLength)
    );
    assert_eq!(
        SessionId::parse("0123456789abcdef0123456789abcdeg"),
        Err(IdError::NotLowercaseHex)
    );
    assert_eq!(
        SessionId::parse(" 123456789abcdef0123456789abcdef"),
        Err(IdError::NotLowercaseHex)
    );
    assert_eq!(SessionId::parse(&"0".repeat(32)), Err(IdError::AllZero));
    assert_eq!(SessionId::from_bytes([0; 16]), Err(IdError::AllZero));
    // 'é' is two bytes: 32 bytes in total, rejected as non-hex rather than by length.
    assert_eq!(
        DeviceId::parse("é123456789abcdef0123456789abcde"),
        Err(IdError::NotLowercaseHex)
    );
}

#[test]
fn identifiers_serialize_as_hex_and_reject_invalid_json() {
    let id = SessionId::parse(HEX).expect("valid");
    let json = serde_json::to_string(&id).expect("serialize");
    assert_eq!(json, format!("\"{HEX}\""));
    assert_eq!(
        serde_json::from_str::<SessionId>(&json).expect("deserialize"),
        id
    );
    for bad in [
        "\"\"",
        "\"00000000000000000000000000000000\"",
        "\"XYZ\"",
        "42",
        "null",
    ] {
        assert!(serde_json::from_str::<SessionId>(bad).is_err(), "{bad}");
    }
}

// T-LOG-02: Debug of identifiers shows the tag only.
#[test]
fn identifier_debug_output_shows_only_the_tag() {
    let id = SessionId::parse(HEX).expect("valid");
    let debug = format!("{id:?}");
    assert_eq!(debug, "SessionId(01234567…)");
    assert!(!debug.contains(HEX));
}

#[test]
fn generations_increase() {
    let first = Generation::FIRST;
    let second = first.next().expect("next");
    assert!(second > first);
    assert_eq!(second.get(), 2);
}

// T-LOG-02: secrets cannot be printed by accident; access is explicit; drop wipes.
#[test]
fn secrets_debug_redacted_and_wiped() {
    let code = Secret::new(String::from("424242919"));
    assert_eq!(format!("{code:?}"), "Secret(<redacted>)");
    assert_eq!(format!("{:?}", Some(&code)), "Some(Secret(<redacted>))");
    assert_eq!(code.expose(), "424242919");

    let mut bytes = vec![1u8, 2, 3];
    bytes.wipe();
    assert!(bytes.is_empty());
    let mut array = [7u8; 4];
    array.wipe();
    assert_eq!(array, [0; 4]);
    let mut text = String::from("Hunter2");
    text.wipe();
    assert!(text.is_empty());
}

#[test]
fn every_error_category_is_classified_and_named() {
    let names: HashSet<_> = ErrorCategory::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(names.len(), 13);
    for category in ErrorCategory::ALL {
        let json = serde_json::to_string(&category).expect("serialize");
        assert_eq!(json, format!("\"{}\"", category.as_str()));
        assert_eq!(
            serde_json::from_str::<ErrorCategory>(&json).expect("round trip"),
            category
        );
        assert!(!category.user_message().is_empty());
    }
    let user_correctable = [
        ErrorCategory::PermissionDenied,
        ErrorCategory::ConsentRequired,
        ErrorCategory::AuthorizationExpired,
        ErrorCategory::SessionRevoked,
        ErrorCategory::ProtocolMismatch,
        ErrorCategory::Timeout,
        ErrorCategory::Cancelled,
        ErrorCategory::TransportUnavailable,
    ];
    let unsupported = [
        ErrorCategory::UnsupportedCapability,
        ErrorCategory::PlatformUnavailable,
    ];
    for category in ErrorCategory::ALL {
        let expected = if user_correctable.contains(&category) {
            ErrorClass::UserCorrectable
        } else if unsupported.contains(&category) {
            ErrorClass::Unsupported
        } else {
            ErrorClass::Implementation
        };
        assert_eq!(category.class(), expected, "{category:?}");
        assert_eq!(
            category.is_user_correctable(),
            expected == ErrorClass::UserCorrectable
        );
    }
}

// T-LOG-04: UI-facing errors carry no internal context.
#[test]
fn public_errors_hide_internal_context() {
    let error = CoreError::new(ErrorCategory::InternalError, "registry poisoned at slot 7");
    assert_eq!(
        error.to_string(),
        "internalError: registry poisoned at slot 7"
    );
    let public = error.public();
    let json = serde_json::to_value(public).expect("serialize");
    assert_eq!(
        json,
        serde_json::json!({
            "category": "internalError",
            "class": "implementation",
            "message": "An internal error occurred."
        })
    );
    assert!(!json.to_string().contains("slot 7"));
}

#[test]
fn capabilities_and_scopes() {
    for capability in Capability::ALL {
        assert_eq!(Capability::from_wire(capability.as_str()), Some(capability));
    }
    assert!(Scope::ViewOnly.includes(Capability::ViewScreen));
    assert!(!Scope::ViewOnly.includes(Capability::ControlInput));
    assert!(Scope::ViewAndControl.includes(Capability::ControlInput));
    assert!(Scope::ViewAndControl.covers(Scope::ViewOnly));
    assert!(!Scope::ViewOnly.covers(Scope::ViewAndControl));

    assert!(PlatformCapabilities::NONE.advertised().is_empty());
    let partial = PlatformCapabilities {
        view_screen: Availability::Available,
        control_input: Availability::NotImplemented,
    };
    assert_eq!(partial.advertised(), vec![Capability::ViewScreen]);
}

#[test]
fn cancellation_propagates_to_children_only() {
    let parent = CancelToken::new();
    let child = parent.child();
    let grandchild = child.child();
    let sibling = parent.child();
    sibling.cancel();
    assert!(!parent.is_cancelled() && !child.is_cancelled());
    parent.cancel();
    assert!(child.is_cancelled() && grandchild.is_cancelled());
    let clone = CancelToken::new();
    let shared = clone.clone();
    shared.cancel();
    assert!(clone.is_cancelled(), "clones share one flag");
}

#[test]
fn monotonic_time_saturates() {
    let t = MonoTime::from_secs(5);
    assert_eq!(
        t.saturating_add(Duration::from_secs(1)),
        MonoTime::from_secs(6)
    );
    assert_eq!(
        MonoTime::from_micros(u64::MAX)
            .saturating_add(Duration::from_secs(1))
            .as_micros(),
        u64::MAX
    );
    assert_eq!(MonoTime::ZERO.saturating_duration_since(t), Duration::ZERO);
}

// T-LOG-02: no payload in Debug output of media, transport and identity types.
#[test]
fn debug_output_never_contains_payloads() {
    let frame = EncodedFrame {
        seq: 1,
        captured_at: MonoTime::ZERO,
        keyframe: true,
        codec: Codec::H264ConstrainedBaseline,
        bitstream: vec![0xDE, 0xAD, 0xBE, 0xEF],
    };
    let debug = format!("{frame:?}");
    assert!(debug.contains("bitstream_len: 4"), "{debug}");
    assert!(!debug.contains("222") && !debug.contains("[222"), "{debug}");

    let control =
        TransportEvent::ControlMessage(b"{\"type\":\"inputBatch\",\"key\":\"SECRETKEY\"}".to_vec());
    let debug = format!("{control:?}");
    assert!(
        !debug.contains("SECRETKEY") && !debug.contains("inputBatch"),
        "{debug}"
    );

    assert_eq!(
        format!("{:?}", Fingerprint([0xAB; 32])),
        "Fingerprint(abababab…)"
    );
    assert_eq!(format!("{:?}", Signature([1; 64])), "Signature(<64 bytes>)");
    assert_eq!(
        format!("{:?}", IdentityPublicKey([0xCD; 32])),
        "IdentityPublicKey(cdcdcdcd…)"
    );
}
