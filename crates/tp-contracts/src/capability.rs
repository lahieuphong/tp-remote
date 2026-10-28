//! Typed capabilities, consent scopes and capability advertisement.
//!
//! A capability is something a Host can expose to an authorized Controller. Only the
//! variants below exist; anything else (file transfer, clipboard, audio, …) is not
//! representable and [`Capability::from_wire`] rejects it.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Capability {
    /// Stream the Host's screen to the Controller.
    ViewScreen,
    /// Inject the Controller's mouse and keyboard input on the Host.
    ControlInput,
}

impl Capability {
    pub const ALL: [Capability; 2] = [Capability::ViewScreen, Capability::ControlInput];

    pub const fn as_str(self) -> &'static str {
        match self {
            Capability::ViewScreen => "viewScreen",
            Capability::ControlInput => "controlInput",
        }
    }

    /// Parses a wire name. Unknown names are rejected rather than mapped to a default.
    pub fn from_wire(name: &str) -> Option<Capability> {
        Capability::ALL.into_iter().find(|c| c.as_str() == name)
    }
}

/// What the Host user consented to (ARCH-00 §1.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Scope {
    ViewOnly,
    ViewAndControl,
}

impl Scope {
    pub const fn includes(self, capability: Capability) -> bool {
        match capability {
            Capability::ViewScreen => true,
            Capability::ControlInput => matches!(self, Scope::ViewAndControl),
        }
    }

    /// Whether `self` grants at least everything `other` grants.
    pub const fn covers(self, other: Scope) -> bool {
        matches!(
            (self, other),
            (Scope::ViewAndControl, _) | (Scope::ViewOnly, Scope::ViewOnly)
        )
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Scope::ViewOnly => "viewOnly",
            Scope::ViewAndControl => "viewAndControl",
        }
    }
}

/// Whether a capability can actually be provided by this build on this system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Availability {
    /// No implementation exists in this build.
    NotImplemented,
    /// An implementation exists but cannot be used right now.
    Unavailable(UnavailableReason),
    Available,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnavailableReason {
    PermissionDenied,
    PlatformUnsupported,
    DisabledByPolicy,
}

impl Availability {
    pub const fn as_str(self) -> &'static str {
        match self {
            Availability::NotImplemented => "notImplemented",
            Availability::Unavailable(UnavailableReason::PermissionDenied) => {
                "unavailable:permissionDenied"
            }
            Availability::Unavailable(UnavailableReason::PlatformUnsupported) => {
                "unavailable:platformUnsupported"
            }
            Availability::Unavailable(UnavailableReason::DisabledByPolicy) => {
                "unavailable:disabledByPolicy"
            }
            Availability::Available => "available",
        }
    }
}

/// Capability advertisement of the Host platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlatformCapabilities {
    pub view_screen: Availability,
    pub control_input: Availability,
}

impl PlatformCapabilities {
    /// Nothing implemented. This is what every BOOT-02 build reports.
    pub const NONE: PlatformCapabilities = PlatformCapabilities {
        view_screen: Availability::NotImplemented,
        control_input: Availability::NotImplemented,
    };

    pub const fn availability(&self, capability: Capability) -> Availability {
        match capability {
            Capability::ViewScreen => self.view_screen,
            Capability::ControlInput => self.control_input,
        }
    }

    /// Capabilities that may be offered to a peer: only those that are `Available`.
    pub fn advertised(&self) -> Vec<Capability> {
        Capability::ALL
            .into_iter()
            .filter(|c| matches!(self.availability(*c), Availability::Available))
            .collect()
    }
}
