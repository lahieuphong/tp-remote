//! Secret-bearing values.
//!
//! [`Secret`] wraps a value that must never reach logs, diagnostics or the UI: one-time
//! codes, passwords, keys, tokens. It deliberately implements neither `Display`,
//! `Serialize`, `Clone` nor `PartialEq`, and its `Debug` output is a fixed placeholder.
//! Reading the value requires an explicit [`Secret::expose`] call, so every place where a
//! secret leaves its wrapper can be found with a text search.
//!
//! ```
//! use tp_contracts::secret::Secret;
//! let code = Secret::new(String::from("123456789"));
//! assert_eq!(format!("{code:?}"), "Secret(<redacted>)");
//! assert_eq!(code.expose(), "123456789");
//! ```
//!
//! The following must not compile:
//!
//! ```compile_fail
//! let code = tp_contracts::secret::Secret::new(String::from("123456789"));
//! println!("{code}"); // no Display
//! ```
//!
//! ```compile_fail
//! let code = tp_contracts::secret::Secret::new(String::from("123456789"));
//! let _ = serde_json::to_string(&code); // no Serialize
//! ```
//!
//! ```compile_fail
//! let code = tp_contracts::secret::Secret::new(String::from("123456789"));
//! let _copy = code.clone(); // no Clone
//! ```

use std::fmt;

/// Overwrites a value in place.
///
/// Best effort only: the overwrite is followed by an optimisation barrier
/// (`std::hint::black_box`) so it is not removed as a dead store, but copies made earlier
/// (for example by a reallocation) cannot be reached. A vetted crate such as `zeroize` is
/// to be evaluated once real key material exists (NET-03).
pub trait Wipe {
    fn wipe(&mut self);
}

impl Wipe for Vec<u8> {
    fn wipe(&mut self) {
        for byte in self.iter_mut() {
            *byte = 0;
        }
        std::hint::black_box(&*self);
        self.clear();
    }
}

impl Wipe for String {
    fn wipe(&mut self) {
        let mut bytes = std::mem::take(self).into_bytes();
        bytes.wipe();
    }
}

impl<const N: usize> Wipe for [u8; N] {
    fn wipe(&mut self) {
        for byte in self.iter_mut() {
            *byte = 0;
        }
        std::hint::black_box(&*self);
    }
}

/// A value that must not be logged, displayed or serialized.
pub struct Secret<T: Wipe>(T);

impl<T: Wipe> Secret<T> {
    pub fn new(value: T) -> Self {
        Secret(value)
    }

    /// Explicit access to the secret. Keep call sites few and obvious.
    pub fn expose(&self) -> &T {
        &self.0
    }
}

impl<T: Wipe> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

impl<T: Wipe> Drop for Secret<T> {
    fn drop(&mut self) {
        self.0.wipe();
    }
}
