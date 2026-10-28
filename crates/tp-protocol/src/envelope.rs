//! Envelope encoding and decoding.
//!
//! Wire shape (v1), with fields always written in this order:
//!
//! ```json
//! {"protocolVersion":1,"type":"hello","seq":1,"payload":{...}}
//! ```
//!
//! Decoding order (subset of ARCH-00 §7.4): size limit → JSON syntax → strict envelope
//! (no unknown or duplicate fields) → protocol version → message type (known, then
//! authorised for the channel) → sequence number → strict payload → semantic checks.

use serde::{Deserialize, Serialize};

use crate::error::{ProtocolError, sanitize, sanitize_detail};
use crate::limits::Channel;
use crate::message::{Body, Hello, Message, MessageKind, RESERVED_MESSAGE_TYPES};
use crate::version::{
    MAX_SUPPORTED_PROTOCOL_VERSION, MIN_SUPPORTED_PROTOCOL_VERSION, PROTOCOL_VERSION, is_supported,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawEnvelope {
    // Kept as a JSON value so a wrong type and an out-of-range number can be told apart.
    protocol_version: serde_json::Value,
    #[serde(rename = "type")]
    kind: String,
    seq: u64,
    payload: serde_json::Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireEnvelope<'a, P: Serialize> {
    protocol_version: u16,
    #[serde(rename = "type")]
    kind: &'static str,
    seq: u64,
    payload: &'a P,
}

/// Encodes a message deterministically as UTF-8 JSON.
pub fn encode(message: &Message, channel: Channel) -> Result<Vec<u8>, ProtocolError> {
    let kind = message.body.kind();
    if !kind.allowed_on(channel) {
        return Err(ProtocolError::NotAuthorized {
            type_name: kind.wire_name().to_owned(),
        });
    }
    if message.seq == 0 {
        return Err(ProtocolError::InvalidEnvelope(
            "seq must be at least 1".to_owned(),
        ));
    }
    message.body.validate()?;

    let bytes = match &message.body {
        Body::Hello(hello) => serde_json::to_vec(&WireEnvelope {
            protocol_version: PROTOCOL_VERSION,
            kind: kind.wire_name(),
            seq: message.seq,
            payload: hello,
        }),
    }
    .map_err(|_| ProtocolError::Encode)?;

    let max = channel.max_message_bytes();
    if bytes.len() > max {
        return Err(ProtocolError::TooLarge {
            len: bytes.len(),
            max,
        });
    }
    Ok(bytes)
}

/// Decodes and validates one message received on `channel`.
pub fn decode(bytes: &[u8], channel: Channel) -> Result<Message, ProtocolError> {
    let max = channel.max_message_bytes();
    if bytes.len() > max {
        return Err(ProtocolError::TooLarge {
            len: bytes.len(),
            max,
        });
    }

    let raw: RawEnvelope = serde_json::from_slice(bytes).map_err(|e| {
        if e.is_syntax() || e.is_eof() {
            ProtocolError::MalformedJson
        } else {
            ProtocolError::InvalidEnvelope(sanitize_detail(&e.to_string()))
        }
    })?;

    let version = raw.protocol_version.as_u64().ok_or_else(|| {
        ProtocolError::InvalidEnvelope("protocolVersion must be an unsigned integer".to_owned())
    })?;
    if !is_supported(version) {
        return Err(ProtocolError::UnsupportedVersion {
            got: version,
            min: MIN_SUPPORTED_PROTOCOL_VERSION,
            max: MAX_SUPPORTED_PROTOCOL_VERSION,
        });
    }

    let kind = match MessageKind::from_wire(&raw.kind) {
        Some(kind) if kind.allowed_on(channel) => kind,
        Some(_) => {
            return Err(ProtocolError::NotAuthorized {
                type_name: sanitize(&raw.kind),
            });
        }
        None if RESERVED_MESSAGE_TYPES.contains(&raw.kind.as_str()) => {
            return Err(ProtocolError::NotAuthorized {
                type_name: sanitize(&raw.kind),
            });
        }
        None => {
            return Err(ProtocolError::UnknownMessageType {
                type_name: sanitize(&raw.kind),
            });
        }
    };

    if raw.seq == 0 {
        return Err(ProtocolError::InvalidEnvelope(
            "seq must be at least 1".to_owned(),
        ));
    }

    let body = match kind {
        MessageKind::Hello => Body::Hello(
            serde_json::from_value::<Hello>(raw.payload)
                .map_err(|e| ProtocolError::InvalidPayload(sanitize_detail(&e.to_string())))?,
        ),
    };
    body.validate()?;

    Ok(Message { seq: raw.seq, body })
}
