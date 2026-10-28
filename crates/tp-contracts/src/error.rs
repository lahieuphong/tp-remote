//! Cross-platform error model.
//!
//! A [`CoreError`] is a category plus a static, programmer-written context string. It
//! never carries peer-supplied text, OS error strings, paths or secrets, so it is safe to
//! log. What a user sees is [`CoreError::public`]: the category, its class and a fixed
//! message, without the internal context.

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCategory {
    InvalidState,
    PermissionDenied,
    ConsentRequired,
    AuthorizationExpired,
    SessionRevoked,
    UnsupportedCapability,
    ProtocolMismatch,
    InvalidPayload,
    Timeout,
    Cancelled,
    TransportUnavailable,
    PlatformUnavailable,
    InternalError,
}

/// Who can resolve an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorClass {
    /// The user can act: grant a permission, approve, retry, update, reconnect.
    UserCorrectable,
    /// The build or the system lacks the feature; retrying will not help.
    Unsupported,
    /// A defect, a misbehaving peer or a broken invariant; not the user's doing.
    Implementation,
}

impl ErrorCategory {
    pub const ALL: [ErrorCategory; 13] = [
        ErrorCategory::InvalidState,
        ErrorCategory::PermissionDenied,
        ErrorCategory::ConsentRequired,
        ErrorCategory::AuthorizationExpired,
        ErrorCategory::SessionRevoked,
        ErrorCategory::UnsupportedCapability,
        ErrorCategory::ProtocolMismatch,
        ErrorCategory::InvalidPayload,
        ErrorCategory::Timeout,
        ErrorCategory::Cancelled,
        ErrorCategory::TransportUnavailable,
        ErrorCategory::PlatformUnavailable,
        ErrorCategory::InternalError,
    ];

    pub const fn class(self) -> ErrorClass {
        match self {
            ErrorCategory::PermissionDenied
            | ErrorCategory::ConsentRequired
            | ErrorCategory::AuthorizationExpired
            | ErrorCategory::SessionRevoked
            | ErrorCategory::ProtocolMismatch
            | ErrorCategory::Timeout
            | ErrorCategory::Cancelled
            | ErrorCategory::TransportUnavailable => ErrorClass::UserCorrectable,
            ErrorCategory::UnsupportedCapability | ErrorCategory::PlatformUnavailable => {
                ErrorClass::Unsupported
            }
            ErrorCategory::InvalidState
            | ErrorCategory::InvalidPayload
            | ErrorCategory::InternalError => ErrorClass::Implementation,
        }
    }

    pub const fn is_user_correctable(self) -> bool {
        matches!(self.class(), ErrorClass::UserCorrectable)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorCategory::InvalidState => "invalidState",
            ErrorCategory::PermissionDenied => "permissionDenied",
            ErrorCategory::ConsentRequired => "consentRequired",
            ErrorCategory::AuthorizationExpired => "authorizationExpired",
            ErrorCategory::SessionRevoked => "sessionRevoked",
            ErrorCategory::UnsupportedCapability => "unsupportedCapability",
            ErrorCategory::ProtocolMismatch => "protocolMismatch",
            ErrorCategory::InvalidPayload => "invalidPayload",
            ErrorCategory::Timeout => "timeout",
            ErrorCategory::Cancelled => "cancelled",
            ErrorCategory::TransportUnavailable => "transportUnavailable",
            ErrorCategory::PlatformUnavailable => "platformUnavailable",
            ErrorCategory::InternalError => "internalError",
        }
    }

    /// Fixed, user-facing text. Contains no details of the failure.
    pub const fn user_message(self) -> &'static str {
        match self {
            ErrorCategory::InvalidState => "The session is not in a state that allows this action.",
            ErrorCategory::PermissionDenied => "This action is not permitted.",
            ErrorCategory::ConsentRequired => "The host user has not approved this session.",
            ErrorCategory::AuthorizationExpired => "The session authorization has expired.",
            ErrorCategory::SessionRevoked => "The host user ended or revoked the session.",
            ErrorCategory::UnsupportedCapability => "This build does not support that capability.",
            ErrorCategory::ProtocolMismatch => {
                "The other device uses an incompatible protocol version."
            }
            ErrorCategory::InvalidPayload => "Received data was invalid and was rejected.",
            ErrorCategory::Timeout => "The operation timed out.",
            ErrorCategory::Cancelled => "The operation was cancelled.",
            ErrorCategory::TransportUnavailable => "The network connection is not available.",
            ErrorCategory::PlatformUnavailable => "This feature is not available on this system.",
            ErrorCategory::InternalError => "An internal error occurred.",
        }
    }
}

/// An engine error: a category plus static context written by the programmer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoreError {
    category: ErrorCategory,
    context: &'static str,
}

impl CoreError {
    pub const fn new(category: ErrorCategory, context: &'static str) -> CoreError {
        CoreError { category, context }
    }

    pub const fn category(self) -> ErrorCategory {
        self.category
    }

    /// Internal context for diagnostics. Not shown to users.
    pub const fn context(self) -> &'static str {
        self.context
    }

    pub const fn class(self) -> ErrorClass {
        self.category.class()
    }

    /// The representation that may cross the UI boundary.
    pub const fn public(self) -> PublicError {
        PublicError {
            category: self.category,
            class: self.category.class(),
            message: self.category.user_message(),
        }
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.category.as_str(), self.context)
    }
}

impl std::error::Error for CoreError {}

/// UI-facing error: category, class and a fixed message. No internal context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicError {
    pub category: ErrorCategory,
    pub class: ErrorClass,
    pub message: &'static str,
}
