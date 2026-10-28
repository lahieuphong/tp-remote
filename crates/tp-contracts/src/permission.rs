//! OS permission contract (ARCH-00 §4.7). FUTURE implementations: macOS TCC checks
//! (CAP-02, CTRL-05). Windows needs no special permission at medium integrity.

use crate::error::CoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OsPermission {
    /// macOS Screen Recording.
    ScreenCapture,
    /// macOS Accessibility (PostEvent).
    InputInjection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PermissionStatus {
    Granted,
    Denied,
    NotDetermined,
    Restricted,
    RequiresRelaunch,
    /// The platform has no such permission (e.g. Windows).
    NotApplicable,
    Unknown,
}

pub trait PermissionManager: Send + Sync {
    /// Re-queried on every call; implementations must not cache a result across sessions.
    fn status(&self, permission: OsPermission) -> PermissionStatus;

    /// Shows the OS prompt where the OS offers one. Must never automate, bypass or
    /// script the OS consent UI (SR-09).
    fn request(&self, permission: OsPermission) -> PermissionStatus;

    fn open_system_settings(&self, permission: OsPermission) -> Result<(), CoreError>;
}
