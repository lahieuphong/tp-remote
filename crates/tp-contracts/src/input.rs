//! Input injection contract (ARCH-00 §4.3). FUTURE implementations: CGEvent (macOS) and
//! SendInput (Windows) in `tp-input`, CTRL-05.
//!
//! [`InjectableEvent`] is an internal, already-mapped, already-authorized event. No
//! protocol message decodes into it in BOOT-02, and the only code allowed to call
//! [`InputController::inject`] is `tp-core`'s input gate, which re-authorizes every
//! event (enforced by tp-test-harness/tests/workspace_boundaries.rs).

use std::fmt;

use crate::capability::Availability;
use crate::error::CoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointerButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WheelMode {
    Lines,
    Pixels,
}

/// USB HID usage (page 0x07). Its `Debug` output is redacted: keystrokes are never logged.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyUsage(pub u16);

impl fmt::Debug for KeyUsage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("KeyUsage(<redacted>)")
    }
}

/// An input event in OS coordinates, ready for injection.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum InjectableEvent {
    PointerMove {
        x: i32,
        y: i32,
    },
    PointerButton {
        x: i32,
        y: i32,
        button: PointerButton,
        pressed: bool,
    },
    Wheel {
        dx: i16,
        dy: i16,
        mode: WheelMode,
    },
    Key {
        usage: KeyUsage,
        pressed: bool,
    },
}

impl InjectableEvent {
    pub const fn kind(&self) -> &'static str {
        match self {
            InjectableEvent::PointerMove { .. } => "pointerMove",
            InjectableEvent::PointerButton { .. } => "pointerButton",
            InjectableEvent::Wheel { .. } => "wheel",
            InjectableEvent::Key { .. } => "key",
        }
    }
}

// Coordinates and keys can reveal what a user types (including on on-screen keyboards),
// so Debug shows only the event kind.
impl fmt::Debug for InjectableEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "InjectableEvent::{}(<redacted>)", self.kind())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputCapabilities {
    pub availability: Availability,
    pub absolute_pointer: bool,
}

/// Injects input into the local OS session. Owned exclusively by `tp-core`'s input gate.
pub trait InputController: Send {
    fn capabilities(&self) -> InputCapabilities;

    fn inject(&mut self, event: &InjectableEvent) -> Result<(), CoreError>;

    /// Releases every key and button this controller pressed. Must be safe to call
    /// repeatedly and is called on every teardown (ARCH-00 §5.5 step 2).
    fn release_all(&mut self) -> Result<(), CoreError>;
}
