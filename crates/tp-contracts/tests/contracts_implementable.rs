//! T-CONTRACT-01: every ARCH-00 §4 contract compiles, can be implemented, and can be used
//! through generic code (no trait objects, no async runtime).
//!
//! The implementations below are test-only and honest: each one reports
//! `NotImplemented` and fails with `PlatformUnavailable`. They are not the real
//! implementations (CAP-02A adds a Windows `CaptureProvider` in `tp-capture`).

use std::future::Future;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

use tp_contracts::cancel::CancelToken;
use tp_contracts::capability::{Availability, Scope};
use tp_contracts::capture::{
    CaptureCapabilities, CaptureConfig, CaptureEvent, CapturePicker, CaptureProvider, CaptureStats,
    CaptureStream, DisplayLayout, FrameMeta, FrameSink, PickDone, PickerOwner, TargetInfo,
};
use tp_contracts::codec::{
    CodecInfo, EncodeOptions, EncodedFrame, EncoderConfig, VideoDecoder, VideoEncoder,
};
use tp_contracts::error::{CoreError, ErrorCategory};
use tp_contracts::id::{DeviceId, SessionRef};
use tp_contracts::identity::{DeviceIdentity, IdentityPublicKey, SignContext, Signature};
use tp_contracts::input::{InjectableEvent, InputCapabilities, InputController};
use tp_contracts::permission::{OsPermission, PermissionManager, PermissionStatus};
use tp_contracts::session::{Role, SessionManager, SessionSnapshot, StopKind};
use tp_contracts::time::MonoTime;
use tp_contracts::transport::{
    CloseReason, Fingerprint, SendOutcome, SignalingLink, Transport, TransportCapabilities,
    TransportEvent, TransportParams, TransportSession,
};

const UNAVAILABLE: CoreError = CoreError::new(ErrorCategory::PlatformUnavailable, "test double");

/// Polls a future that must be ready immediately (no executor needed).
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("test doubles never block"),
    }
}

struct NoCapture;
struct NoStream(TargetInfo);
struct NullSink;

impl FrameSink<Vec<u8>> for NullSink {
    fn on_frame(&self, _meta: FrameMeta, _frame: Vec<u8>) {}
    fn on_event(&self, _event: CaptureEvent) {}
}

impl CaptureStream for NoStream {
    fn target(&self) -> &TargetInfo {
        &self.0
    }
    fn generation(&self) -> u64 {
        0
    }
    fn reconfigure(&mut self, _config: CaptureConfig) -> Result<(), CoreError> {
        Err(UNAVAILABLE)
    }
    fn stop(self) -> Result<CaptureStats, CoreError> {
        Ok(CaptureStats::default())
    }
}

impl CaptureProvider for NoCapture {
    type Frame = Vec<u8>;
    type Target = ();
    type Stream = NoStream;
    fn capabilities(&self) -> CaptureCapabilities {
        CaptureCapabilities {
            availability: Availability::NotImplemented,
            max_fps: 0,
            gpu_frames: false,
            system_picker: false,
        }
    }
    fn displays(&self) -> Result<DisplayLayout, CoreError> {
        Err(UNAVAILABLE)
    }
    fn start<S>(
        &self,
        _target: (),
        _config: CaptureConfig,
        _sink: Arc<S>,
    ) -> Result<NoStream, CoreError>
    where
        S: FrameSink<Vec<u8>> + 'static,
    {
        Err(UNAVAILABLE)
    }
}

impl CapturePicker for NoCapture {
    fn pick(&self, _owner: PickerOwner, _done: PickDone<()>) -> Result<(), CoreError> {
        Err(UNAVAILABLE)
    }
}

struct NoInput;
impl InputController for NoInput {
    fn capabilities(&self) -> InputCapabilities {
        InputCapabilities {
            availability: Availability::NotImplemented,
            absolute_pointer: false,
        }
    }
    fn inject(&mut self, _event: &InjectableEvent) -> Result<(), CoreError> {
        Err(UNAVAILABLE)
    }
    fn release_all(&mut self) -> Result<(), CoreError> {
        Ok(())
    }
}

struct NoCodec;
const NO_CODEC: CodecInfo = CodecInfo {
    name: "none",
    hardware: false,
    availability: Availability::NotImplemented,
};
impl VideoEncoder for NoCodec {
    type Input = Vec<u8>;
    fn info(&self) -> CodecInfo {
        NO_CODEC
    }
    fn configure(&mut self, _config: &EncoderConfig) -> Result<(), CoreError> {
        Err(UNAVAILABLE)
    }
    fn encode(
        &mut self,
        _m: &FrameMeta,
        _f: &Vec<u8>,
        _o: EncodeOptions,
        _out: &mut Vec<EncodedFrame>,
    ) -> Result<(), CoreError> {
        Err(UNAVAILABLE)
    }
    fn set_rate(&mut self, _bps: u32, _fps: u32) -> Result<(), CoreError> {
        Err(UNAVAILABLE)
    }
    fn flush(&mut self, _out: &mut Vec<EncodedFrame>) -> Result<(), CoreError> {
        Ok(())
    }
}
impl VideoDecoder for NoCodec {
    type Output = Vec<u8>;
    fn info(&self) -> CodecInfo {
        NO_CODEC
    }
    fn decode(&mut self, _frame: &EncodedFrame) -> Result<Option<Vec<u8>>, CoreError> {
        Err(UNAVAILABLE)
    }
    fn reset(&mut self) {}
}

struct NoLink;
impl SignalingLink for NoLink {
    fn send(&mut self, _m: &[u8]) -> impl Future<Output = Result<(), CoreError>> + Send {
        std::future::ready(Err(UNAVAILABLE))
    }
    fn recv(&mut self) -> impl Future<Output = Result<Vec<u8>, CoreError>> + Send {
        std::future::ready(Err(UNAVAILABLE))
    }
}

struct NoTransport;
struct NoTransportSession;
impl TransportSession for NoTransportSession {
    fn send_control(&mut self, _m: &[u8]) -> Result<(), CoreError> {
        Err(UNAVAILABLE)
    }
    fn send_video(&mut self, _f: &EncodedFrame) -> Result<SendOutcome, CoreError> {
        Err(UNAVAILABLE)
    }
    fn poll_event(&mut self) -> Option<TransportEvent> {
        None
    }
    fn remote_fingerprint(&self) -> Option<Fingerprint> {
        None
    }
    fn close(self, _reason: CloseReason) -> impl Future<Output = Result<(), CoreError>> + Send {
        std::future::ready(Ok(()))
    }
}
impl Transport for NoTransport {
    type Session = NoTransportSession;
    fn capabilities(&self) -> TransportCapabilities {
        TransportCapabilities {
            availability: Availability::NotImplemented,
        }
    }
    fn establish<L: SignalingLink>(
        &self,
        _params: TransportParams,
        _link: &mut L,
        cancel: &CancelToken,
    ) -> impl Future<Output = Result<NoTransportSession, CoreError>> + Send {
        let result = if cancel.is_cancelled() {
            Err(CoreError::new(ErrorCategory::Cancelled, "cancelled"))
        } else {
            Err(UNAVAILABLE)
        };
        std::future::ready(result)
    }
}

struct NoPermissions;
impl PermissionManager for NoPermissions {
    fn status(&self, _p: OsPermission) -> PermissionStatus {
        PermissionStatus::NotApplicable
    }
    fn request(&self, _p: OsPermission) -> PermissionStatus {
        PermissionStatus::NotApplicable
    }
    fn open_system_settings(&self, _p: OsPermission) -> Result<(), CoreError> {
        Err(UNAVAILABLE)
    }
}

struct NoIdentity;
impl DeviceIdentity for NoIdentity {
    fn device_id(&self) -> DeviceId {
        DeviceId::from_bytes([1; 16]).expect("id")
    }
    fn public_key(&self) -> IdentityPublicKey {
        IdentityPublicKey([0; 32])
    }
    fn sign(&self, _c: SignContext, _m: &[u8]) -> Result<Signature, CoreError> {
        Err(UNAVAILABLE)
    }
    fn display_fingerprint(&self) -> String {
        String::new()
    }
}

/// A consent proof type that cannot be constructed outside this module, mirroring how
/// real implementations bind `ConsentProof` to an unforgeable type.
struct OpaqueProof(());
struct NoSessions;
impl SessionManager for NoSessions {
    type ConsentProof = OpaqueProof;
    fn begin(&mut self, _r: Role, _s: Scope, _n: MonoTime) -> Result<SessionRef, CoreError> {
        Err(UNAVAILABLE)
    }
    fn respond_consent(
        &mut self,
        _s: SessionRef,
        _p: OpaqueProof,
        _n: MonoTime,
    ) -> Result<SessionSnapshot, CoreError> {
        Err(UNAVAILABLE)
    }
    fn disconnect(
        &mut self,
        _s: SessionRef,
        _k: StopKind,
        _n: MonoTime,
    ) -> Result<SessionSnapshot, CoreError> {
        Err(UNAVAILABLE)
    }
    fn tick(&mut self, _n: MonoTime) -> Result<Option<SessionSnapshot>, CoreError> {
        Ok(None)
    }
    fn snapshot(&self, _s: SessionRef) -> Option<SessionSnapshot> {
        None
    }
}

// Generic consumers, as tp-core will use them.
fn start_capture<P: CaptureProvider<Frame = Vec<u8>, Target = ()>>(
    provider: &P,
) -> Result<P::Stream, CoreError> {
    let config = CaptureConfig {
        max_fps: 30,
        format: tp_contracts::capture::PixelFormat::Nv12,
        show_cursor: true,
    };
    provider.start((), config, Arc::new(NullSink))
}

fn open_picker<P: CapturePicker>(provider: &P) -> Result<(), CoreError> {
    provider.pick(PickerOwner(0), Box::new(|_outcome| {}))
}

fn pair<P, E>(_provider: &P, _encoder: &E)
where
    P: CaptureProvider,
    E: VideoEncoder<Input = P::Frame>,
{
}

#[test]
fn every_contract_is_implementable_and_reports_honestly() {
    let capture = NoCapture;
    assert_eq!(
        capture.capabilities().availability,
        Availability::NotImplemented
    );
    assert_eq!(start_capture(&capture).err(), Some(UNAVAILABLE));
    assert_eq!(open_picker(&capture), Err(UNAVAILABLE));
    let stream = NoStream(TargetInfo {
        kind: tp_contracts::capture::TargetKind::Unknown,
        label: String::new(),
        size: tp_contracts::capture::PixelSize {
            width: 0,
            height: 0,
        },
        display: None,
    });
    assert_eq!(stream.target().kind.as_str(), "unknown");
    assert_eq!(stream.generation(), 0);
    assert!(stream.stop().is_ok());
    pair(&capture, &NoCodec);

    let mut input = NoInput;
    assert_eq!(
        input.capabilities().availability,
        Availability::NotImplemented
    );
    assert!(
        input
            .inject(&InjectableEvent::PointerMove { x: 0, y: 0 })
            .is_err()
    );
    assert!(input.release_all().is_ok());

    let mut decoder = NoCodec;
    assert_eq!(
        VideoDecoder::info(&decoder).availability,
        Availability::NotImplemented
    );
    assert!(
        decoder
            .decode(&EncodedFrame {
                seq: 0,
                captured_at: MonoTime::ZERO,
                keyframe: false,
                codec: tp_contracts::codec::Codec::H264ConstrainedBaseline,
                bitstream: Vec::new(),
            })
            .is_err()
    );

    let transport = NoTransport;
    assert_eq!(
        transport.capabilities().availability,
        Availability::NotImplemented
    );
    let params = TransportParams {
        role: Role::Host,
        session: SessionRef {
            id: tp_contracts::id::SessionId::from_bytes([2; 16]).expect("id"),
            generation: tp_contracts::id::Generation::FIRST,
        },
        stun_urls: Vec::new(),
        video: true,
    };
    let cancel = CancelToken::new();
    let mut link = NoLink;
    assert_eq!(
        ready(transport.establish(params.clone(), &mut link, &cancel)).err(),
        Some(UNAVAILABLE)
    );
    cancel.cancel();
    assert_eq!(
        ready(transport.establish(params, &mut link, &cancel))
            .err()
            .map(|e| e.category()),
        Some(ErrorCategory::Cancelled)
    );
    assert!(ready(NoTransportSession.close(CloseReason::Local(StopKind::Disconnect))).is_ok());
    assert!(ready(link.recv()).is_err());

    assert_eq!(
        NoPermissions.status(OsPermission::ScreenCapture),
        PermissionStatus::NotApplicable
    );
    assert!(
        NoIdentity
            .sign(SignContext::SessionTranscript, b"x")
            .is_err()
    );
    assert_eq!(
        SignContext::SessionTranscript.label(),
        b"tp-remote/session-transcript/v1"
    );

    let mut sessions = NoSessions;
    assert!(
        sessions
            .begin(Role::Host, Scope::ViewOnly, MonoTime::ZERO)
            .is_err()
    );
    let _proof_type_exists = OpaqueProof(());
}
