//! Message types for protocol v1.

use serde::{Deserialize, Serialize};

use crate::error::ProtocolError;
use crate::limits::{Channel, MAX_APP_VERSION_LEN};

/// A decoded, validated message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// Per-direction sequence number, starting at 1.
    pub seq: u64,
    pub body: Body,
}

/// Typed message bodies. v1 defines only the shape of `hello`; no handshake uses it yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    Hello(Hello),
}

impl Body {
    pub fn kind(&self) -> MessageKind {
        match self {
            Body::Hello(_) => MessageKind::Hello,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            Body::Hello(hello) => hello.validate(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Hello,
}

impl MessageKind {
    /// Value of the envelope's `type` field.
    pub const fn wire_name(self) -> &'static str {
        match self {
            MessageKind::Hello => "hello",
        }
    }

    pub fn from_wire(name: &str) -> Option<Self> {
        match name {
            "hello" => Some(MessageKind::Hello),
            _ => None,
        }
    }

    pub const fn allowed_on(self, channel: Channel) -> bool {
        match self {
            MessageKind::Hello => matches!(channel, Channel::Signaling),
        }
    }
}

/// Message types named in ARCH-00 §7.3 that belong to later phases. They are
/// recognised so they can be rejected as not authorised instead of as unknown.
pub const RESERVED_MESSAGE_TYPES: &[&str] = &[
    "pakeMsg",
    "keyConfirm",
    "sessionRequest",
    "hostIdentity",
    "consentResult",
    "sdpOffer",
    "sdpAnswer",
    "iceCandidate",
    "iceRestart",
    "resume",
    "bye",
    "error",
    "heartbeat",
    "displayLayout",
    "selectDisplay",
    "inputBatch",
    "keyframeRequest",
    "streamState",
    "scopeChanged",
    "sessionTimer",
];

/// `hello` payload: the sender's application version and supported protocol range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Hello {
    pub app_version: String,
    pub min_protocol_version: u16,
    pub max_protocol_version: u16,
}

impl Hello {
    fn validate(&self) -> Result<(), ProtocolError> {
        let v = &self.app_version;
        if v.is_empty() || v.len() > MAX_APP_VERSION_LEN {
            return Err(ProtocolError::InvalidPayload(format!(
                "appVersion must be 1..={MAX_APP_VERSION_LEN} characters"
            )));
        }
        if !v
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
        {
            return Err(ProtocolError::InvalidPayload(
                "appVersion may contain only ASCII letters, digits, '.', '-' and '+'".to_owned(),
            ));
        }
        if self.min_protocol_version == 0 {
            return Err(ProtocolError::InvalidPayload(
                "minProtocolVersion must be at least 1".to_owned(),
            ));
        }
        if self.min_protocol_version > self.max_protocol_version {
            return Err(ProtocolError::InvalidPayload(
                "minProtocolVersion must not exceed maxProtocolVersion".to_owned(),
            ));
        }
        Ok(())
    }
}
