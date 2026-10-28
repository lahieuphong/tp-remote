# BOOT-02 — Test Results

| Field | Value |
|---|---|
| Phase | BOOT-02 |
| Date | 2026-10-28 |
| Device | DEV-W1: Windows 11 Enterprise LTSC 2024, build 26100, x64 |
| Toolchain | rustc/cargo 1.99.1 (MSVC 14.44), Node 24.19.0, npm 11.16.0 (unchanged) |
| Baseline | git `7a9ae99` (clean tree) |
| Evidence | `docs/qa/evidence/BOOT-02/` |
| Certification | **Windows only, engine level.** Mock-based tests prove logic, not real-world OS or network enforcement |

## 1. Summary (observed)

| Gate | Command | Result |
|---|---|---|
| Baseline before changes | `npm run check` | 9 of 9 PASS: 51 Rust and 18 Vitest tests (`baseline/check-all-before-changes.log`) |
| Rust format | `cargo fmt --all -- --check` | PASS |
| Rust check | `cargo check --workspace` | PASS |
| Rust tests | `cargo test --workspace` | **141 passed, 0 failed, 0 ignored** (44 result lines) |
| Clippy | `cargo clippy --workspace --all-targets -- -D warnings` | PASS (0 findings) |
| All gates | `npm run check` | **9 of 9 PASS**: Rust gates, bindings drift, typecheck, ESLint, Vitest (**18 passed**), frontend build |
| Native builds | `npm run build:app`, `npm run build:app:debug` | PASS. Release 8,697,344 B, SHA-256 `4DE228A9…3F76`; debug 12,428,800 B, `244DE7E1…DE8A` |
| Windows runtime | `scripts/smoke-windows.ps1` (×4), `scripts/smoke-dev-windows.ps1` | PASS (§6) |

No coverage percentage is claimed (no coverage tool).

### 1.1 Test count by target (from `gates/cargo-test.log`)

| Crate / target | Tests | New in BOOT-02 |
|---|---|---|
| tp-contracts `ipc_fixtures` | 6 | 0 |
| tp-contracts `primitives` | 11 | 11 |
| tp-contracts `contracts_implementable` | 1 | 1 |
| tp-contracts doctests (1 positive + 3 compile-fail) | 4 | 4 |
| tp-core (unit) | 5 | 1 |
| tp-core `service` | 11 | 11 |
| tp-core `gate_and_logging` | 7 | 7 |
| tp-diagnostics (unit) | 10 | 6 |
| tp-protocol `codec` / `fixtures` / `reserved` | 11 / 2 / 4 | 0 / 0 / 4 |
| tp-security (unit) | 13 | 13 |
| tp-session `machine` / `exploration` / `registry` | 15 / 1 / 13 | 29 |
| tp-test-harness `bindings` / `workspace_boundaries` | 5 / 9 | 0 / 3 |
| tp-desktop `config` / `ipc` | 4 / 9 | 0 |
| **Total** | **141** | **90** |

### 1.2 BOOT-01 regression (51 tests)

All 51 BOOT-01 tests still pass:
- **45 unchanged.**
- **3 renamed with updated expectations:**
  - tp-diagnostics `formats_plain_fields` → `formats_plain_allowed_fields`;
  - `keys_and_events_are_restricted_to_identifier_characters` → `event_names_and_keys_are_restricted_to_identifier_characters` (the field allowlist now redacts ad-hoc keys);
  - tp-core `no_remote_capability_crate_claims_an_implementation_in_boot01` → `no_capability_crate_claims_an_implementation`. tp-session and tp-security are now legitimately *partial*; the invariant now covers the capability crates and the advertised capabilities.
- **3 updated in place:** tp-core `app_info_reports_this_build_honestly` (phase BOOT-02), tp-diagnostics `empty_and_quoted_values_are_escaped` (allowed keys), tp-desktop `get_app_info_returns_the_typed_contract` (asserts `tp_core::PHASE`).
- All 18 Vitest tests are unchanged and pass. No UI or binding file changed (git).

## 2. Required scenarios → canonical IDs (each test counted once)

**Command key:**

| Key | Command |
|---|---|
| A | `cargo test -p tp-session --test machine` |
| B | `cargo test -p tp-session --test exploration` |
| C | `cargo test -p tp-session --test registry` |
| D | `cargo test -p tp-security` |
| E | `cargo test -p tp-core --test service` |
| F | `cargo test -p tp-core --test gate_and_logging` |
| G | `cargo test -p tp-diagnostics` |
| H | `cargo test -p tp-contracts` |
| I | `cargo test -p tp-protocol` |
| J | `cargo test -p tp-test-harness` |
| K | `npm run test` |

Evidence for every row: `docs/qa/evidence/BOOT-02/gates/cargo-test.log` and `gates/test-names.txt`. All observed results are from the full run in §1.

| Prompt ID | Canonical ID | Purpose | Cmd | Expected | Observed | Result |
|---|---|---|---|---|---|---|
| T-CORE-01 Valid initialization | T-UNI-01 | A new machine is Idle, inert, with no deadline | A | Idle, no grant, no deadline, no input | `initialization_is_idle_and_inert` ok | PASS |
| T-CORE-02 Valid transitions | T-UNI-01 | Rows 1–21, with exact ordered effects | A | Every valid row gives its state and effects | `normal_path_host_view_and_control`, `normal_path_controller_and_view_only_host`, `every_active_state_exits_through_disconnecting`, `link_degradation_closes_and_restoration_reopens_the_gate`, `teardown_escalation_and_watchdog` ok | PASS |
| T-CORE-03 Invalid transition rejection | T-UNI-02 (+ T-UNI-04, T-UNI-05) | All (state, event) pairs match an independent spec; rejections don't mutate | A, B | 1,116 pairs consistent; exploration finds no violation | `invalid_transitions_are_rejected_without_mutation`: 1,116 checked, 670 rejected. Exploration: 4,728 configurations, 148,157 edges. `consent_from_the_wrong_origin_is_rejected`, `consent_cannot_exceed_the_requested_scope` ok | PASS |
| T-CORE-04 Consent denied | T-UNI-03 | Denial ends normally after full cleanup; nothing started | A, C, E | Disconnecting → Disconnected; no `StartCapture` | `consent_denied_ends_normally_after_cleanup`, `scenario_b_local_denial`, `a_local_denial_grants_nothing` ok | PASS |
| T-CORE-05 Preparation timeout | T-UNI-06 | Every timer fires exactly at its deadline with the ADR-003 outcome | A, C | No early firing; correct `EndReason` and outcome | `timers_fire_exactly_at_their_deadlines` (6 cases), `reconnect_grace_and_session_ttl`, `scenario_c_preparation_failure_and_timeout`, `scenario_d_connection_timeout` ok | PASS |
| T-CORE-06 Duplicate disconnect | T-UNI-23 | Stops are idempotent | A, C, E | `AlreadyStopping` / `AlreadyEnded`, state unchanged | `duplicate_stops_during_teardown_change_nothing`, `scenario_h_duplicate_disconnects`, `revocation_takes_effect_before_the_next_authorization` ok | PASS |
| T-CORE-07 Disconnect during Connecting | T-UNI-01 | A stop while connecting wins over a late transport | A | Late `TransportReady` rejected; gate closed | `disconnect_during_connecting_cannot_be_undone_by_a_late_transport` ok | PASS |
| T-CORE-08 Reconnect with new identity | T-UNI-23 | A new session gets a new id and a higher generation | C, E | id ≠ old; generation > old | `scenario_k_previous_session_events_during_a_new_session`, `stale_sessions_regain_nothing` ok | PASS |
| T-CORE-09 Reject stale session events | T-UNI-23 | Old or forged references and post-end events are rejected | A, C, E | `StaleSession` / `SessionEnded`; new session untouched | `scenario_k_…` (all events, stop, forged generation), `scenario_i_events_after_the_end_are_rejected`, `an_ended_session_rejects_every_event_and_has_no_timers`, `events_without_any_session_are_rejected` ok | PASS |
| T-CORE-10 Concurrent lifecycle events | T-UNI-24 | Race between `TransportReady` and stop | C | Exactly one stop starts teardown; never Connected after it | `scenario_j_concurrent_connect_and_disconnect`: 200 rounds × 8 threads; TransportReady won 62 times; invariant held in every round | PASS |
| T-SEC-CORE-01 No consent → no control | T-UNI-17 (+ T-UNI-28) | Policy denies without a grant; the gate injects nothing | D, E, F | `ConsentRequired`; 0 injected | `no_consent_means_no_control_permission`, `nothing_is_authorized_before_local_consent`, `input_before_consent_is_never_injected` ok | PASS |
| T-SEC-CORE-02 Authenticated but unapproved | T-UNI-17 | Authentication ≠ consent | D, E | Denied; a Deny never becomes a grant | `an_authenticated_but_unapproved_peer_is_rejected`, `preparation_requires_an_authenticated_peer` ok | PASS |
| T-SEC-CORE-03 Expired authorization | T-UNI-17 | Grant and TTL expiry | D, E | `ConsentExpired` / `SessionExpired` exactly at expiry | `expired_authorization_is_rejected`, `authorization_ends_at_the_session_ttl` ok | PASS |
| T-SEC-CORE-04 Revocation immediate | T-UNI-17 | Revocation invalidates the next decision | D, E, F | Allowed → revoke → denied at the same instant | `revocation_immediately_invalidates_authorization`, `revocation_takes_effect_before_the_next_authorization`, `input_is_injected_only_while_fully_authorized` ok | PASS |
| T-SEC-CORE-05 Wrong session id | T-UNI-17 | Id, generation and proof binding | D, E | `WrongSession` / `StaleGeneration`; foreign proofs refused | `a_wrong_session_identifier_is_rejected`, `a_grant_for_a_different_peer_is_rejected`, `consent_proofs_are_bound_to_their_session` ok | PASS |
| T-SEC-CORE-06 Invalid capability | T-UNI-17 | Unsupported or unknown capabilities | D | `CapabilityUnsupported`; unknown names unparseable | `unsupported_capabilities_are_rejected` ok | PASS |
| T-SEC-CORE-07 No implicit escalation | T-UNI-17 (+ T-UNI-28) | View-only cannot become control without new consent | A, D, F | `ScopeNotGranted`; machine rejects a grant above the request | `there_is_no_implicit_privilege_escalation`, `consent_cannot_exceed_the_requested_scope`, `view_only_consent_never_allows_input` ok | PASS |
| T-SEC-CORE-08 Closed session regains nothing | T-UNI-17 | Disconnecting / terminal sessions are denied | D, E | `SessionNotActive` / `SessionEnded` | `a_closed_session_cannot_regain_access`, `stale_sessions_regain_nothing`, `only_a_connected_host_with_a_healthy_link_can_be_authorized` ok | PASS |
| T-LOG-01 Secrets redacted | T-UNI-18 | Synthetic secrets never appear | G | Unlisted keys and secret patterns redacted | `unlisted_field_names_are_redacted`, `secrets_inside_allowed_free_text_are_scrubbed` (10 patterns), `memory_sink_collects_rendered_lines` ok. **First run failed (defect F-1, fixed)** | PASS |
| T-LOG-02 Debug output safe | T-UNI-26 | `Debug` of secrets, ids, input, frames, control bytes, keys | F, H | Placeholders, tags or lengths only | `secrets_debug_redacted_and_wiped`, `identifier_debug_output_shows_only_the_tag`, `debug_output_never_contains_payloads`, `debug_output_of_input_events_is_redacted`, 3 compile-fail doctests ok | PASS |
| T-LOG-03 Unauthorized events logged without payload | T-UNI-18 | Denials log kind and reason, never data | F | Reason and kind present; coordinates and key code absent | `denied_input_is_logged_without_payload`, `lifecycle_logging_is_complete_and_payload_free` ok | PASS |
| T-LOG-04 Errors reveal nothing private | T-UNI-25 | Error model and mapping | D, H, I | Fixed public messages; no peer text | `public_errors_hide_internal_context`, `every_error_category_is_classified_and_named`, `protocol_errors_map_to_categories_without_peer_text`, `deny_reasons_map_to_typed_error_categories` ok | PASS |
| T-CONTRACT-01 Traits compile | T-UNI-27 | Every ARCH-00 §4 contract is implementable and usable generically | H | Compiles; honest test doubles | `every_contract_is_implementable_and_reports_honestly` ok | PASS |
| T-CONTRACT-02 Boundary rules | T-STA-08 | Crate edges, cycles, Tauri isolation, `forbid(unsafe)`, inject call site, no production implementations, no test-support in the app | J | 9 of 9 pass; negative controls fail as designed | `workspace_boundaries` 9 ok; NC-2/3/4 (§5) | PASS |
| T-CONTRACT-03 Protocol compatibility | T-UNI-14, T-UNI-15, T-UNI-16 | v1 fixtures, golden bytes, limits | I | 24 fixture cases + 11 codec tests pass | `fixtures` 2, `codec` 11 ok (unchanged from BOOT-01) | PASS |
| T-CONTRACT-04 Bindings synchronized | T-INT-10 | Rust ↔ TS types | J, K, `npm run bindings:check` | No drift | `bindings` 5 ok; Vitest contract 5 ok; `bindings:check` PASS; no binding file changed | PASS |
| T-CONTRACT-05 Unsupported capabilities not advertised | T-UNI-27 | Nothing advertised; nothing claims an implementation | E, F, J, tp-core unit | `platform_capabilities()` = NONE; capability crates Foundation; app shows notImplemented | `no_capability_crate_claims_an_implementation`, `no_production_platform_implementation_exists_yet`, `the_production_platform_lets_no_input_through`; runtime screenshot shows notImplemented | PASS |
| T-CONTRACT-06 Reserved types cannot execute | T-UNI-29 | All 20 reserved types rejected on both channels; no message kind | I | `NotAuthorized` everywhere | `reserved_types_are_rejected_on_every_channel`, `input_and_session_commands_are_among_the_reserved_types`, `reserved_types_with_a_plausible_payload_are_still_rejected` ok | PASS |

## 3. Additional BOOT-02 tests (not in the prompt's list)

| Canonical ID | Tests | Result |
|---|---|---|
| T-UNI-23 (registry) | `only_one_active_session_at_a_time`, `the_audit_trail_records_applied_and_rejected_events_without_payloads` | PASS |
| T-UNI-17 (service level) | `a_controller_never_decides_consent_and_is_never_authorized`, `a_second_session_cannot_start_while_one_is_active`, `remote_consent_on_the_host_is_rejected_and_ends_the_session`, `a_fully_consented_session_is_authorized_only_where_the_platform_provides`, `positive_control_allows_only_with_every_condition_met`, `an_empty_context_denies_everything` | PASS |
| Primitives (identifiers, cancellation, time, capabilities) | `identifiers_accept_only_32_lowercase_hex_characters`, `identifiers_serialize_as_hex_and_reject_invalid_json`, `generations_increase`, `cancellation_propagates_to_children_only`, `monotonic_time_saturates`, `capabilities_and_scopes`, `the_monotonic_clock_never_goes_backwards` | PASS |
| Logging context | `ordinary_context_survives_scrubbing`, `sessions_are_logged_by_tag_never_by_full_identifier`, `long_text_is_capped` | PASS |

## 4. Mocks vs. enforcement

| What a test proves | What it does not prove |
|---|---|
| The policy and the gate deny or allow correctly given the state (`RecordingController` mock) | That a future SendInput or CGEvent adapter is reachable only through the gate (the boundary test only checks call sites in source) |
| The state machine's lifecycle, timers and cleanup *instructions* | That a capture, transport or prompt driver executes those effects |
| Redaction of the patterns tested | That every future log call follows the rules |

## 5. Negative controls (`negative-controls/negative-controls.log`)

| ID | Deliberate violation | Expected | Observed | Restored |
|---|---|---|---|---|
| NC-1 / NC-1b | Machine accepts `TransportReady` in `AwaitingConsent` | Lifecycle tests fail | Exploration failed ("accepted against spec: TransportReady"). With `--no-fail-fast`, `invalid_transitions_are_rejected_without_mutation` also failed | `machine.rs` hash identical; 29 tests pass again |
| NC-2 | `.inject(` called from `tp-core/src/clock.rs` | Boundary test fails | `only_the_input_gate_calls_inject` FAILED ("calls inject outside the input gate") | `clock.rs` hash identical |
| NC-3 | `impl PermissionManager for Probe` in production code | Boundary test fails | `no_production_platform_implementation_exists_yet` FAILED | same |
| NC-4 | `test-support` forced into the app graph via tp-core | Boundary test fails | `the_app_build_graph_has_no_test_support` FAILED ("tp-core … test-support") | Manifests and `Cargo.lock` hashes identical; 9 of 9 pass again |

## 6. Windows runtime regression (real WebView2, DEV-W1)

| Check | Run | Observed | Result | Evidence (`windows/`) |
|---|---|---|---|---|
| WIN-BOOT-01/02/03 | release-run1 | `ipc.get_app_info result=ok` after 0.45 s; Medium integrity, not elevated; PerMonitorV2; UI shows **BOOT-02**, tp-session and tp-security *partial*, capability crates *foundation*, notImplemented | PASS | `release-run1.*` |
| WIN-BOOT-04 | Mock runtime with the real ACL (`tp-desktop` `ipc` 9 tests) | All non-granted calls rejected; capability file and `generate_handler!` unchanged | PASS (same limitation as BOOT-01: no live-WebView injection) | `gates/cargo-test.log` |
| WIN-BOOT-05 | release-run1 `-Resize` | 500×400, 1024×700, 1600×1000 and the negative-coordinate monitor render correctly (100% scaling only) | PASS at 100% | `release-run1.resize-*.png` |
| WIN-BOOT-06/07 | release-run1, release-run2 | Exit 0, 0 survivors, restart OK | PASS | `release-run2.summary.json` |
| WIN-BOOT-08 | debug-fault | `result=err error_code=injectedFault`; UI shows **Failed** | PASS | `debug-fault.initial.png` |
| Fault ignored in release | release-fault-env-ignored | `result=ok` with the variable set | PASS | `release-fault-env-ignored.summary.json` |
| No listeners | all runs | 0 TCP / 0 UDP endpoints in every app tree | PASS | summaries |
| Dev launch | dev-launch | Ready in 12.5 s; Vite on 127.0.0.1:1420 only; whole tree exited; port freed | PASS | `dev-launch.*` |
| `STATUS_ENTRYPOINT_NOT_FOUND` recurrence | all test runs | Did not recur. Only `tp-desktop` links Tauri (boundary test `only_the_shell_depends_on_tauri`), and its `build.rs` manifest embedding is unchanged; its 13 tests ran | PASS | `gates/cargo-test.log` |

## 7. Defects found during BOOT-02

| Defect | Found by | Fix |
|---|---|---|
| Scrubber leaked the token in `Authorization: Bearer <token>` (F-1) | `secrets_inside_allowed_free_text_are_scrubbed` | An auth scheme after a sensitive key keeps redaction armed for the following token |
| Test expectation error: `é…` is 32 bytes, so it is rejected as non-hex, not by length | `identifiers_accept_only_32_lowercase_hex_characters` | Test expectation corrected; the parser was already correct (rejects) |
| Clippy 1.99 `chunks_exact_to_as_chunks` | `cargo clippy -D warnings` | `as_chunks::<2>()` |

## 8. Not run

| Item | Reason |
|---|---|
| macOS (any) | No Mac; nothing is claimed for macOS |
| Non-100% scaling, live-WebView ACL injection | As in BOOT-01; no system-setting changes; no automation backdoor |
| `cargo audit`, `cargo deny`, Playwright, fuzzing | Not approved for installation |
| T-UNI-22 (bounded media queues), T-INT-01 (capture pipeline) | No media pipeline exists (FUTURE: CAP-02 / VID-04) |
| Coverage | No tool |
