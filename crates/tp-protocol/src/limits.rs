//! Size limits (ARCH-00 §7.6).

/// Logical channel a message travels on. Limits and allowed message types differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Signaling,
    Control,
}

impl Channel {
    /// Largest encoded message accepted or produced on this channel, in bytes.
    pub const fn max_message_bytes(self) -> usize {
        match self {
            Channel::Signaling => 64 * 1024,
            Channel::Control => 16 * 1024,
        }
    }
}

/// Longest `appVersion` string accepted in a `hello` payload.
pub const MAX_APP_VERSION_LEN: usize = 32;

/// Longest peer-supplied string echoed back inside an error.
pub(crate) const MAX_ECHOED_LEN: usize = 32;
