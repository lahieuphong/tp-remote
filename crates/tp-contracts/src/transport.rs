//! Transport contracts (ARCH-00 §4.5, ADR-002). FUTURE implementation: a Rust WebRTC
//! stack chosen in NET-03 (U-01), in `tp-transport`.
//!
//! The contract is runtime-agnostic: asynchronous operations return `impl Future + Send`
//! without depending on a specific executor. Events are pulled with `poll_event`, which
//! suits sans-IO stacks and deterministic simulation.

use std::fmt;
use std::future::Future;

use crate::cancel::CancelToken;
use crate::capability::Availability;
use crate::codec::EncodedFrame;
use crate::error::{CoreError, ErrorCategory};
use crate::id::SessionRef;
use crate::session::{Role, StopKind};

/// SHA-256 DTLS certificate fingerprint (D-10, THREAT-MODEL §6).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint(pub [u8; 32]);

impl fmt::Debug for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Fingerprint({:02x}{:02x}{:02x}{:02x}…)",
            self.0[0], self.0[1], self.0[2], self.0[3]
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportParams {
    pub role: Role,
    pub session: SessionRef,
    /// STUN URLs only. TURN credentials are FUTURE (WAN-07) and will be `Secret`s.
    pub stun_urls: Vec<String>,
    pub video: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportCapabilities {
    pub availability: Availability,
}

#[derive(Clone, PartialEq, Eq)]
pub enum TransportEvent {
    LinkDegraded,
    LinkRestored,
    KeyframeRequested,
    BitrateEstimate {
        bps: u32,
    },
    /// Raw `tp-control` bytes; must go through `tp-protocol` validation before use.
    ControlMessage(Vec<u8>),
    Closed {
        by_peer: bool,
    },
}

// Control messages may contain input events, so only their length is shown.
impl fmt::Debug for TransportEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportEvent::LinkDegraded => f.write_str("LinkDegraded"),
            TransportEvent::LinkRestored => f.write_str("LinkRestored"),
            TransportEvent::KeyframeRequested => f.write_str("KeyframeRequested"),
            TransportEvent::BitrateEstimate { bps } => write!(f, "BitrateEstimate({bps})"),
            TransportEvent::ControlMessage(bytes) => {
                write!(f, "ControlMessage(<{} bytes redacted>)", bytes.len())
            }
            TransportEvent::Closed { by_peer } => write!(f, "Closed(by_peer={by_peer})"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendOutcome {
    Sent,
    Backpressure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    Local(StopKind),
    Error(ErrorCategory),
}

/// The PAKE-authenticated signaling channel (U-03). SDP and fingerprints must only travel
/// through it.
pub trait SignalingLink: Send {
    fn send(&mut self, message: &[u8]) -> impl Future<Output = Result<(), CoreError>> + Send;
    fn recv(&mut self) -> impl Future<Output = Result<Vec<u8>, CoreError>> + Send;
}

pub trait TransportSession: Send {
    fn send_control(&mut self, message: &[u8]) -> Result<(), CoreError>;

    /// Non-blocking; reports backpressure instead of queueing without bound.
    fn send_video(&mut self, frame: &EncodedFrame) -> Result<SendOutcome, CoreError>;

    fn poll_event(&mut self) -> Option<TransportEvent>;

    /// Remote DTLS fingerprint after the handshake; `tp-core` compares it with the
    /// fingerprint received over the authenticated signaling channel.
    fn remote_fingerprint(&self) -> Option<Fingerprint>;

    fn close(self, reason: CloseReason) -> impl Future<Output = Result<(), CoreError>> + Send
    where
        Self: Sized;
}

pub trait Transport {
    type Session: TransportSession;

    fn capabilities(&self) -> TransportCapabilities;

    /// Negotiates and connects. Must observe `cancel` and fail with `Cancelled`.
    fn establish<L: SignalingLink>(
        &self,
        params: TransportParams,
        link: &mut L,
        cancel: &CancelToken,
    ) -> impl Future<Output = Result<Self::Session, CoreError>> + Send;
}
