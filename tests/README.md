# Shared test fixtures

These fixtures are read by **both** the Rust and the TypeScript tests, so the two sides of each contract are checked against the same bytes.

| Path | Used by | Purpose |
|---|---|---|
| `fixtures/ipc/app-info.valid.json` | `crates/tp-contracts/tests/ipc_fixtures.rs`, `apps/desktop/src/contract/appInfo.contract.test.ts`, `apps/desktop/src/app/App.test.tsx` | The JSON that Rust's `AppInfo` serializes to; TypeScript must accept it unchanged |
| `fixtures/ipc/invalid/*.json` | same Rust and TS contract tests | Must be rejected by both sides: missing field, wrong type, unknown field, bad enum, bad nested enum |
| `fixtures/protocol/cases.json` | `crates/tp-protocol/tests/fixtures.rs` | Manifest: each protocol fixture, the channel it is decoded on, and the expected result (`ok` or a `ProtocolErrorKind`) |
| `fixtures/protocol/valid/hello.canonical.json` | `crates/tp-protocol/tests/codec.rs` | Golden bytes of the deterministic v1 encoding |

When you add a protocol fixture file, list it in `cases.json`. A test fails if any fixture file is not covered.
