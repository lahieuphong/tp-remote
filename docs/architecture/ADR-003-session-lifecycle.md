# ADR-003 — Session Lifecycle Clarifications (amends ARCH-00 §4.4 and §6)

| Field | Value |
|---|---|
| Status | **Accepted for BOOT-02**, pending user review |
| Date | 2026-10-28 |
| Phase | BOOT-02 |
| Implemented in | `crates/tp-session` (machine, registry), `crates/tp-core/src/service.rs` (consent and authentication guards) |
| Tests | `crates/tp-session/tests/{machine,exploration,registry}.rs`, `crates/tp-core/tests/service.rs` |

## 1. Context

ARCH-00 §6 defines eight states, a transition table (rows 1–21), rejection rules and timers. Implementing it exposed one contradiction, several gaps and one contract change. This ADR records how each was resolved. ARCH-00's lifecycle invariant is unchanged: **every exit from an active state passes through `Disconnecting`, and `Disconnected` / `Failed` are terminal**.

## 2. Decisions

| # | Topic | ARCH-00 said | Decision (implemented and tested) |
|---|---|---|---|
| C-1 | **Contradiction:** `Busy` | Row 4 lists `Busy` as a `Fault` (→ `Failed`); the outcome classes list `PeerBye` with reason `Busy` as **Normal** (→ `Disconnected`) | `Busy` is a peer reason: `PeerBye(Busy)` → Normal → `Disconnected`. On the Host, a second request never creates a session: `SessionRegistry::begin` returns `RegistryError::Busy` while a session is active |
| C-2 | Stale events | A single-use machine per session, but no rule for events addressed to an older session | Every event carries a `SessionRef { id, generation }`. The registry applies it only if both match the current session; otherwise `StaleSession`, with no state change and an audit record. The generation strictly increases. A stale stop can never stop a newer session |
| C-3 | How Failed, timeouts and denials are cleaned up | "Exits via Disconnecting" | All of them enter `Disconnecting` and emit the role's full ordered teardown (§3). The terminal state is chosen by the recorded outcome when `TeardownComplete` arrives or the watchdog fires. A `Fault` during teardown, or the watchdog, sets an `Escalation` and forces `Failed`; the original `EndReason` is kept |
| C-4 | Timers | Values listed (§6.5); implementation open | The machine stores absolute deadlines and fires them in `tick(now)`. There are no external timer events, so a timer cannot fire for a state the machine has already left. Deadlines and outcomes are listed in §4 |
| C-5 | Host waiting with no request | Not specified | `OfferExpiry` (10 min, the code TTL) ends a Host's `Preparing` **normally** (`OfferExpired` → `Disconnected`). An unused hosting offer is not a failure |
| C-6 | Stop / Bye during `Disconnecting` | "LocalStop in Disconnecting rejected; API idempotent" | Rejected with `AlreadyStopping` (benign), for both `LocalStop` and `PeerBye`. `SessionRegistry::stop` and `SessionManager::disconnect` return the snapshot unchanged (`AlreadyStopping` / `AlreadyEnded`) |
| C-7 | Consent from the wrong origin | "Rejected; raises Fault{Protocol}" | The machine rejects without mutation (`ConsentOriginNotPermitted`). The **service** then applies `Fault(ProtocolViolation)` when a network-originated decision reaches a Host, so the session fails instead of connecting |
| C-8 | Preparation guard | "PAKE ok + SessionRequest" | `SessionService::handle(Prepared)` requires an `AuthenticatedPeer` for exactly this session; otherwise `PermissionDenied` with no state change. FUTURE: the NET-03 handshake produces `AuthenticatedPeer` |
| C-9 | Grant lifetime | Input gate checks `now < expires_at` | The granted scope is cleared on entering `Disconnecting`. `input_allowed(now)` compares against the TTL directly, so input stops at the TTL instant even if `tick` has not yet run |
| C-10 | `SessionManager` contract | Async methods (§4.4 sketch) | **Synchronous**, `&mut self`. The owner serializes calls; I/O happens through returned effects. `type ConsentProof` binds consent to an unforgeable type (`tp_security::LocalConsent` in `tp-core`) |
| C-11 | Concurrency | Actor model (§5.1) | `SessionRegistry` is single-owner. `SharedRegistry` wraps it in a mutex, so each operation is one critical section and events are totally ordered. A poisoned lock fails closed (`RegistryError::Poisoned`) |
| C-12 | Audit | "Logged as a security event" | The registry keeps a bounded (256) audit trail of applied and rejected events: target session, from/to state, cause name, verdict. No payloads or secrets. `tp-core` logs transitions, rejections and denials through `tp-diagnostics` |

## 3. Effects (what the driver must do; idempotent instructions)

| Transition | Host effects | Controller effects |
|---|---|---|
| Idle → Preparing | `OpenHostOffer` | `ConnectToHost` |
| Preparing → AwaitingConsent | `PromptLocalConsent` | `AwaitConsentResult` |
| AwaitingConsent → Connecting | `StartCapture`, `StartTransport` | `StartTransport` |
| Connecting → Connected | `ShowSessionIndicator` (+ `OpenInputGate` if scope is view-and-control) | `ShowSessionIndicator` |
| Link degraded | `CloseInputGate`, `PauseMedia` | same |
| Link restored | `ResumeMedia`, `RequestKeyframe` (+ `OpenInputGate`) | same, without the gate |
| → Disconnecting | `CloseInputGate`, `ReleaseAllInput`, `StopCapture`, `StopEncoder`, `SendBye`, `CloseTransport`, `CloseSignaling`, `WipeSessionSecrets` | `SendBye`, `CloseTransport`, `CloseSignaling`, `WipeSessionSecrets` |
| → Disconnected / Failed | `ReportOutcome(outcome)` | same |

The Host's teardown order is ARCH-00 §5.5, with the input gate closed first.

## 4. Timers and outcomes

| Timer | Armed in | Default | Fires as | Outcome |
|---|---|---|---|---|
| `OfferExpiry` | Host `Preparing` | 10 min | `OfferExpired` | Normal |
| `PrepareTimeout` | Controller `Preparing` | 10 s | `Fault(PrepareTimeout)` | Error |
| `ConsentDecision` | Host `AwaitingConsent` | 60 s | `ConsentTimeout` | Normal |
| `ConsentWait` | Controller `AwaitingConsent` | 70 s | `Fault(ConsentWaitTimeout)` | Error |
| `ConnectTimeout` | `Connecting` | 20 s | `Fault(ConnectTimeout)` | Error |
| `ReconnectGrace` | `Connected` + link degraded | 15 s | `Fault(TransportLost)` | Error |
| `SessionTtl` | Host, from consent | 60 min | `SessionExpired` | Normal |
| `TeardownWatchdog` | `Disconnecting` | 5 s | forces `Failed`, `Escalation::TeardownTimeout` | Error |

## 5. Consequences

- **Positive:** the lifecycle is a pure function of (state, event, now). It is testable exhaustively: 4,728 reachable configurations were explored. Stale and late events cannot revive or redirect a session.
- **Negative:** the driver must call `tick` at `next_deadline()`; nothing fires by itself. Effects describe work that does not exist yet (capture, transport, prompts). NET-03, CAP-02 and VID-04 must implement them, and must call the input gate and the policy. A tested policy is not enforcement until those adapters are tested.
- **FUTURE:** production session identifiers from a CSPRNG (NET-03); `TransportReady` must require the DTLS fingerprint check (NET-03).
