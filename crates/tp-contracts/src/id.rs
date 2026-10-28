//! Identifiers shared across the engine.
//!
//! Text form: exactly 32 lowercase hexadecimal characters. Anything else, including the
//! all-zero value, is rejected.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Why an identifier was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdError {
    WrongLength,
    NotLowercaseHex,
    AllZero,
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            IdError::WrongLength => "identifier must be 32 hexadecimal characters",
            IdError::NotLowercaseHex => "identifier must use lowercase hexadecimal characters",
            IdError::AllZero => "identifier must not be all zeros",
        })
    }
}

impl std::error::Error for IdError {}

fn parse_hex16(text: &str) -> Result<[u8; 16], IdError> {
    let bytes = text.as_bytes();
    if bytes.len() != 32 {
        return Err(IdError::WrongLength);
    }
    let mut out = [0u8; 16];
    let (pairs, _) = bytes.as_chunks::<2>();
    for (slot, [high, low]) in out.iter_mut().zip(pairs) {
        *slot = (nibble(*high)? << 4) | nibble(*low)?;
    }
    nonzero(out)
}

fn nibble(digit: u8) -> Result<u8, IdError> {
    match digit {
        b'0'..=b'9' => Ok(digit - b'0'),
        b'a'..=b'f' => Ok(digit - b'a' + 10),
        _ => Err(IdError::NotLowercaseHex),
    }
}

fn nonzero(bytes: [u8; 16]) -> Result<[u8; 16], IdError> {
    if bytes == [0u8; 16] {
        Err(IdError::AllZero)
    } else {
        Ok(bytes)
    }
}

fn to_hex(bytes: &[u8; 16]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(32);
    for byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    out
}

macro_rules! hex_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name([u8; 16]);

        impl $name {
            pub fn from_bytes(bytes: [u8; 16]) -> Result<Self, IdError> {
                nonzero(bytes).map(Self)
            }

            pub fn parse(text: &str) -> Result<Self, IdError> {
                parse_hex16(text).map(Self)
            }

            pub fn as_bytes(&self) -> &[u8; 16] {
                &self.0
            }

            pub fn to_hex(&self) -> String {
                to_hex(&self.0)
            }

            /// First 8 hex characters: enough to correlate log lines, not to address the
            /// object. Logs use the tag, never the full identifier.
            pub fn tag(&self) -> String {
                let mut hex = to_hex(&self.0);
                hex.truncate(8);
                hex
            }
        }

        // Debug shows only the tag, so `{:?}` in a log line cannot leak the full value.
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({}…)", stringify!($name), self.tag())
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.to_hex())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = String::deserialize(deserializer)?;
                Self::parse(&text).map_err(serde::de::Error::custom)
            }
        }
    };
}

hex_id!(
    /// 128-bit session identifier. Not a secret, but never reused. Production values must
    /// come from a CSPRNG (FUTURE: NET-03); tests use deterministic sources.
    SessionId
);

hex_id!(
    /// 128-bit device identifier, derived from the device identity key (FUTURE: NET-03).
    DeviceId
);

/// Distinguishes successive sessions created by one registry. Strictly increasing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Generation(u64);

impl Generation {
    pub const FIRST: Generation = Generation(1);

    pub const fn get(self) -> u64 {
        self.0
    }

    /// The following generation, or `None` on overflow.
    pub fn next(self) -> Option<Generation> {
        self.0.checked_add(1).map(Generation)
    }
}

/// Address of one session: identifier plus generation. Every event targeting a session
/// carries both, so events of an earlier session can be recognised and rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionRef {
    pub id: SessionId,
    pub generation: Generation,
}
