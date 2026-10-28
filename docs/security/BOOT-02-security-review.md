# BOOT-02 — Security Review

| Field | Value |
|---|---|
| Phase | BOOT-02 |
| Date | 2026-10-28 |
| Scope | Session lifecycle, authorization policy, consent and capability gating, secret redaction, error model |
| Method | Design review against THREAT-MODEL §5–§8; executed tests; four negative controls (deliberate violations that must make tests fail) |
| Platform | Windows 11 (DEV-W1), engine level only. **Windows-only certification; nothing here is verified on macOS or on a real network** |

> **What the tests prove.** A passing policy or gate test proves the *logic* of the policy or gate. It does **not** prove that a future WebRTC adapter, OS input adapter, capture provider or IPC command calls them. Those enforcement paths do not exist yet, and each must be tested in the phase that builds it.

## 1. Authorization model (IMPLEMENTED, TESTED at engine level)

| Concept | Representation | How it is obtained |
|---|---|---|
| Authenticated peer | `tp_security::AuthenticatedPeer { session, peer }` | FUTURE: only the NET-03 handshake. No production constructor; `for_tests` behind `test-support` |
| Local host consent | `tp_security::LocalConsent { session, decision, decided_at }` | FUTURE: only the trusted local consent surface (§3). No production constructor |
| Active authorized session | `SessionSnapshot` with state `Connected`, link `Up`, role Host | `tp-session` machine (ADR-003) |
| Screen-view permission | `Capability::ViewScreen`, included in both scopes | `ConsentGrant.scope` **and** the session's granted scope |
| Remote-input permission | `Capability::ControlInput`, only in `Scope::ViewAndControl` | as above; checked per event by `InputGate` |
| Revocation | `ConsentGrant::revoke()` (one-way) plus `LocalStop(Revoke)` → `Disconnecting` | `SessionManager::disconnect` revokes first, then stops |
| Expiration | `ConsentGrant.expires_at` and the session TTL (`expires_at`) | Both are compared with the caller's `now` on every decision |

- **Authentication and consent are independent.** Each has its own type, and a grant needs both proofs for the same session reference (id and generation).
- An incoming request does not imply approval. `Prepared` requires authentication; `Connecting` requires a local `ConsentGranted`.
- **Default deny.** `tp_security::authorize` allows only when all eight conditions in `policy.rs` hold. An empty context denies everything (`an_empty_context_denies_everything`).
- **Production capability advertisement is `NONE`.** Even a fully consented session is denied every capability (`the_production_platform_lets_no_input_through`, `a_fully_consented_session_is_authorized_only_where_the_platform_provides`).

## 2. Rejections required by the brief → evidence

| The policy must reject | Mechanism | Test(s) |
|---|---|---|
| Unauthenticated peers | Step 4 `NotAuthenticated`; `Prepared` guard | `an_authenticated_but_unapproved_peer_is_rejected`, `preparation_requires_an_authenticated_peer` |
| Missing consent | Step 5 `ConsentRequired` | `no_consent_means_no_control_permission`, `nothing_is_authorized_before_local_consent`, `input_before_consent_is_never_injected` |
| Expired consent | Step 6 `ConsentExpired` | `expired_authorization_is_rejected`, `authorization_ends_at_the_session_ttl` |
| Revoked consent | Step 6 `ConsentRevoked`; state leaves `Connected` | `revocation_immediately_invalidates_authorization`, `revocation_takes_effect_before_the_next_authorization`, `input_is_injected_only_while_fully_authorized` |
| Expired sessions | Step 6 `SessionExpired`; TTL timer | `expired_authorization_is_rejected`, `reconnect_grace_and_session_ttl` |
| Incorrect session identifiers | Step 1 `WrongSession`; registry `StaleSession` | `a_wrong_session_identifier_is_rejected`, `scenario_k_…`, `stale_sessions_regain_nothing` |
| Stale generation identifiers | Step 1 `StaleGeneration`; registry `StaleSession` | same, plus the forged-generation case in `scenario_k_…` |
| Unsupported capabilities | Step 8 `CapabilityUnsupported`; typed `Capability::from_wire` | `unsupported_capabilities_are_rejected` |
| Events after teardown | Machine `SessionEnded`; policy `SessionEnded` / `SessionNotActive` | `scenario_i_…`, `a_closed_session_cannot_regain_access`, `an_ended_session_rejects_every_event_and_has_no_timers` |
| Escalation without new authorization | No API widens a grant; both scopes must include the capability; the machine rejects a grant above the request | `there_is_no_implicit_privilege_escalation`, `consent_cannot_exceed_the_requested_scope`, `view_only_consent_never_allows_input` |
| Consent decided by a remote party on the Host | Machine `ConsentOriginNotPermitted`; service `Fault(ProtocolViolation)` | `consent_from_the_wrong_origin_is_rejected`, `remote_consent_on_the_host_is_rejected_and_ends_the_session` |

## 3. Trusted-consent boundary (requirement for NET-03 / U-06; **FUTURE**)

A `LocalConsent` may only be created by an interaction that is all of the following:

1. **Local.** The decision is taken on the Host by its physically present user. It is never received from the network, the signaling server or the Controller.
2. **Out of the WebView's control.** A compromised or XSS-affected WebView must not be able to produce a consent decision (TM-16). Acceptable designs: a native OS dialog owned by the Rust shell, or a shell-side confirmation the WebView cannot trigger by itself. A plain IPC command such as `consent_respond(allow)` callable from JavaScript is **not** acceptable on its own.
3. **Out of injected input's control.** Remote input must not be able to click "Allow" (TM-12). The surface must ignore injected input where the OS allows detecting it (FS-C: `IMO_INJECTED` on Windows, event-source tagging on macOS — UNKNOWN). In any case, consent prompts never appear during an active session, because there is one session per device.
4. **Bound.** The decision is bound to exactly one `SessionRef` (id and generation) and timestamped (`decided_at`). It is never reusable for another session; `consent_proofs_are_bound_to_their_session` tests this.
5. **Fail-closed.** Timeout means deny (60 s, ADR-003 §4). Closing the prompt means deny.

**Enforcement today.** `LocalConsent` has private fields, and its only constructor is `for_tests` behind the `test-support` feature. The boundary test `the_app_build_graph_has_no_test_support` proves that the desktop app's build graph never enables that feature. Negative control NC-4 showed the test fails when the feature is forced in.

**Residual risk.**
- Rust visibility is an in-process boundary, not a sandbox. Code compiled into the app with `test-support` could forge proofs; the boundary test prevents that for the app graph.
- Workspace-wide test builds (`cargo test --workspace`) unify the feature into *test* binaries only. Shipped artifacts come from `tauri build`, which builds `tp-desktop` alone.

## 4. Mandatory logging rules (all later phases)

1. Log only typed `tp_diagnostics::Event`s. Field names must be in `ALLOWED_KEYS`; any other name is written as `[REDACTED]`. Adding a key is a reviewed change.
2. **Never** log: passwords, one-time codes, PAKE or session keys, private keys, tokens, TURN credentials, clipboard contents, screen contents or frame bytes, raw keyboard input or coordinates of input events, authentication payloads, or full SDP. This applies to allowed fields too.
3. Secret-bearing values are `tp_contracts::secret::Secret<T>`: no `Display`, `Serialize` or `Clone`, and `Debug` is a placeholder. Use `expose()` only where the secret is consumed.
4. Peer-, library- or OS-supplied text goes through `FieldValue::text`: control characters replaced, secret-looking patterns scrubbed, capped at 160 characters. The scrubber is a **safety net** with known limits (§6); it does not make free-text logging safe.
5. Session identifiers are logged as tags (`01234567/g1`), never in full.
6. Security events: log the rejection reason and event *kind* (e.g. `event_kind=key`), never the event *data*.
7. No persistent log files and no remote telemetry without a new ADR and user approval.
8. Types that may carry payloads (`InjectableEvent`, `KeyUsage`, `EncodedFrame`, `TransportEvent::ControlMessage`, identity keys and signatures) keep their redacting `Debug` implementations. New payload types must do the same.

## 5. THREAT-MODEL status after BOOT-02

| Threat | BOOT-02 control | Level of evidence |
|---|---|---|
| TM-05 Replay / hijack | Stale `SessionRef` rejection; single-use machines; strictly increasing generations | TESTED (engine). Protocol-level sequence and replay: FUTURE (NET-03) |
| TM-07 Input before consent / after revoke or expiry | Machine gate + policy + `InputGate` per-event authorization | TESTED with a mock controller. No OS adapter exists |
| TM-08 Streaming before consent | `StartCapture` effect only on `AwaitingConsent → Connecting`; policy `ViewScreen` | TESTED (engine). No capture exists |
| TM-10 Unauthorized access | Authentication + local consent both required; one session per device | TESTED (engine) |
| TM-12 Remote self-approval / driving the Host UI | Origin guard + protocol-violation fault; single session | Origin guard TESTED. Injected-input detection on the consent surface: FUTURE / UNKNOWN |
| TM-13 Secrets in logs | Allowlist + typed events + `Secret` + scrubber | TESTED with synthetic secrets |
| TM-16 WebView compromise | No new IPC command; consent cannot enter through IPC (no production constructor) | TESTED (static and boundary); the trusted surface is FUTURE |
| TM-24 Expiry not enforced | Host-only TTL; the input gate compares `now` with the TTL even before `tick` | TESTED (engine) |
| TM-25 Stuck input | `ReleaseAllInput` in the Host teardown; `InputGate::release_all` | Effect order TESTED. Real release: FUTURE (CTRL-05) |

## 6. Findings

| ID | Finding | Status |
|---|---|---|
| F-1 | **Defect found by a test:** the scrubber redacted the word `Bearer` but leaked the token in `Authorization: Bearer <token>` | FIXED; `secrets_inside_allowed_free_text_are_scrubbed` passes |
| F-2 | `Secret` wiping is best effort (no `zeroize`, no `unsafe`); copies made before drop are not reachable | ACCEPTED for BOOT-02 (no real key material exists). Evaluate `zeroize` in NET-03 |
| F-3 | The scrubber recognises patterns (`word=…`, `word: …`, JSON keys, `Bearer`, PEM blocks, bare sensitive words). A secret without such context is not recognised | ACCEPTED as a safety net; rules §4.1–4.3 are the control |
| F-4 | `PlatformCapabilities` is plain data, so any code could construct `Available` | MITIGATED: `tp-core` reports `NONE` (test). The boundary test fails if a production capture, input, codec, transport, permission or identity implementation appears |
| F-5 | `test-support` features are unified into workspace *test* binaries | ACCEPTED: shipped builds are separate (`tauri build -p tp-desktop`); the boundary test covers the app graph |
| F-6 | The live-WebView ACL test (BOOT-01 WIN-BOOT-04 limitation) is still not done | OPEN. Deliberately no automation backdoor (prompt §11) |
| F-7 | Tauri's default `dynamic-acl` feature is still enabled (unused) | OPEN, hardening candidate (from BOOT-01) |
| F-8 | BOOT-01 documents stated "not a git repository"; the repository existed since 11:16:57 (commit `3166183`). The claim was carried over unverified | CORRECTED by errata notes in BOOT-01-environment and BOOT-01-report |

## 7. Security invariants → executable tests

| Invariant | Test (crate::file::function) | Status |
|---|---|---|
| No `Connected` without a preceding local consent (Host) and `TransportReady` | tp-session `exploration::every_reachable_configuration_satisfies_the_safety_invariants` | TESTED |
| Rejected events never change state | `machine::invalid_transitions_are_rejected_without_mutation`, exploration | TESTED |
| Input gate open ⇒ Host ∧ Connected ∧ link up ∧ control scope ∧ before TTL | exploration, `machine::reconnect_grace_and_session_ttl` | TESTED |
| Every exit passes through `Disconnecting`; teardown closes the input gate first | exploration, `machine::every_active_state_exits_through_disconnecting` | TESTED |
| A stale session reference never affects the current session | `registry::scenario_k_…`, `service::stale_sessions_regain_nothing` | TESTED |
| Concurrent stop/connect never yields `Connected` after an accepted stop | `registry::scenario_j_concurrent_connect_and_disconnect` (200 rounds × 8 threads) | TESTED |
| Authorization is default-deny and requires authentication **and** consent | tp-security `tests::*` (13), tp-core `service::*` | TESTED |
| Only the input gate calls `inject` | `workspace_boundaries::only_the_input_gate_calls_inject` + NC-2 | TESTED |
| No production platform implementation exists yet | `workspace_boundaries::no_production_platform_implementation_exists_yet` + NC-3 | TESTED |
| The app build never enables `test-support` | `workspace_boundaries::the_app_build_graph_has_no_test_support` + NC-4 | TESTED |
| No new IPC command or permission | Capability file and `generate_handler!` unchanged (git diff); BOOT-01 `config.rs` and `ipc.rs` tests | TESTED |
| Secrets cannot be displayed, serialized or cloned | `secret.rs` compile-fail doctests (3) | TESTED |
| Real OS input, capture or network paths call the policy | — | **UNVERIFIED** (no such path exists; FUTURE) |
| Consent cannot be produced by the WebView or remote input | — | **UNVERIFIED**: the surface is FUTURE (§3) |
