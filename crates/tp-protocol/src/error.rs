//! Protocol errors. Peer-supplied text is truncated and sanitised before it is
//! stored in an error, so errors are safe to log.

use crate::limits::MAX_ECHOED_LEN;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolError {
    #[error("message is {len} bytes; the limit for this channel is {max}")]
    TooLarge { len: usize, max: usize },
    #[error("message is not valid JSON")]
    MalformedJson,
    #[error("invalid envelope: {0}")]
    InvalidEnvelope(String),
    #[error("unsupported protocol version {got}; supported {min}..={max}")]
    UnsupportedVersion { got: u64, min: u16, max: u16 },
    #[error("unknown message type `{type_name}`")]
    UnknownMessageType { type_name: String },
    #[error("message type `{type_name}` is not authorized on this channel")]
    NotAuthorized { type_name: String },
    #[error("invalid payload: {0}")]
    InvalidPayload(String),
    #[error("message could not be encoded")]
    Encode,
}

/// Stable, data-free classification of a [`ProtocolError`] (used by fixtures and metrics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolErrorKind {
    TooLarge,
    MalformedJson,
    InvalidEnvelope,
    UnsupportedVersion,
    UnknownMessageType,
    NotAuthorized,
    InvalidPayload,
    Encode,
}

impl ProtocolError {
    pub fn kind(&self) -> ProtocolErrorKind {
        match self {
            ProtocolError::TooLarge { .. } => ProtocolErrorKind::TooLarge,
            ProtocolError::MalformedJson => ProtocolErrorKind::MalformedJson,
            ProtocolError::InvalidEnvelope(_) => ProtocolErrorKind::InvalidEnvelope,
            ProtocolError::UnsupportedVersion { .. } => ProtocolErrorKind::UnsupportedVersion,
            ProtocolError::UnknownMessageType { .. } => ProtocolErrorKind::UnknownMessageType,
            ProtocolError::NotAuthorized { .. } => ProtocolErrorKind::NotAuthorized,
            ProtocolError::InvalidPayload(_) => ProtocolErrorKind::InvalidPayload,
            ProtocolError::Encode => ProtocolErrorKind::Encode,
        }
    }
}

impl ProtocolErrorKind {
    /// camelCase name, as used in fixture manifests.
    pub const fn as_str(self) -> &'static str {
        match self {
            ProtocolErrorKind::TooLarge => "tooLarge",
            ProtocolErrorKind::MalformedJson => "malformedJson",
            ProtocolErrorKind::InvalidEnvelope => "invalidEnvelope",
            ProtocolErrorKind::UnsupportedVersion => "unsupportedVersion",
            ProtocolErrorKind::UnknownMessageType => "unknownMessageType",
            ProtocolErrorKind::NotAuthorized => "notAuthorized",
            ProtocolErrorKind::InvalidPayload => "invalidPayload",
            ProtocolErrorKind::Encode => "encode",
        }
    }
}

/// Maps to the cross-platform error model. Peer-supplied details (type names, serde
/// messages) are dropped: only static context crosses this boundary.
impl From<ProtocolError> for tp_contracts::error::CoreError {
    fn from(error: ProtocolError) -> Self {
        use tp_contracts::error::{CoreError, ErrorCategory};
        match error {
            ProtocolError::UnsupportedVersion { .. } => CoreError::new(
                ErrorCategory::ProtocolMismatch,
                "unsupported protocol version",
            ),
            ProtocolError::TooLarge { .. } => {
                CoreError::new(ErrorCategory::InvalidPayload, "message too large")
            }
            ProtocolError::MalformedJson => {
                CoreError::new(ErrorCategory::InvalidPayload, "message is not valid JSON")
            }
            ProtocolError::InvalidEnvelope(_) => {
                CoreError::new(ErrorCategory::InvalidPayload, "invalid envelope")
            }
            ProtocolError::UnknownMessageType { .. } => {
                CoreError::new(ErrorCategory::InvalidPayload, "unknown message type")
            }
            ProtocolError::NotAuthorized { .. } => {
                CoreError::new(ErrorCategory::InvalidPayload, "message type not authorized")
            }
            ProtocolError::InvalidPayload(_) => {
                CoreError::new(ErrorCategory::InvalidPayload, "invalid payload")
            }
            ProtocolError::Encode => {
                CoreError::new(ErrorCategory::InternalError, "message could not be encoded")
            }
        }
    }
}

/// Truncates to [`MAX_ECHOED_LEN`] characters and replaces anything that is not
/// printable ASCII, so attacker-controlled text cannot inject into logs.
pub(crate) fn sanitize(text: &str) -> String {
    let mut out: String = text
        .chars()
        .take(MAX_ECHOED_LEN)
        .map(|c| {
            if c.is_ascii_graphic() || c == ' ' {
                c
            } else {
                '?'
            }
        })
        .collect();
    if text.chars().count() > MAX_ECHOED_LEN {
        out.push_str("...");
    }
    out
}

/// Error details from serde_json can quote peer input; keep them short and printable.
pub(crate) fn sanitize_detail(text: &str) -> String {
    const MAX_DETAIL_LEN: usize = 160;
    text.chars()
        .take(MAX_DETAIL_LEN)
        .map(|c| {
            if c.is_ascii_graphic() || c == ' ' {
                c
            } else {
                '?'
            }
        })
        .collect()
}
