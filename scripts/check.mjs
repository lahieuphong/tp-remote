#!/usr/bin/env node
// Runs every BOOT-01 local gate in order and stops at the first failure.
// Usage: npm run check            (all gates)
//        npm run check -- --fast  (skip the production frontend build)

import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const repo = resolve(import.meta.dirname, "..");
const fast = process.argv.includes("--fast");
const MINUTE = 60_000;

// Fixed command strings only; nothing here is built from user input.
const gates = [
  ["Rust format", "cargo fmt --all -- --check", 2 * MINUTE],
  ["Rust check", "cargo check --workspace --all-targets --locked", 20 * MINUTE],
  ["Rust clippy", "cargo clippy --workspace --all-targets --locked -- -D warnings", 20 * MINUTE],
  ["Rust tests", "cargo test --workspace --locked", 20 * MINUTE],
  ["Bindings drift", "npm run -s bindings:check", 10 * MINUTE],
  ["TypeScript typecheck", "npm run -s typecheck", 5 * MINUTE],
  ["ESLint", "npm run -s lint", 5 * MINUTE],
  ["Vitest", "npm run -s test", 5 * MINUTE],
  ...(fast ? [] : [["Frontend build", "npm run -s build", 5 * MINUTE]]),
];

const results = [];
for (const [name, command, timeout] of gates) {
  console.log(`\n=== ${name}: ${command}`);
  const started = Date.now();
  const run = spawnSync(command, { cwd: repo, shell: true, stdio: "inherit", timeout });
  const seconds = ((Date.now() - started) / 1000).toFixed(1);
  const timedOut = run.error?.code === "ETIMEDOUT";
  const ok = run.status === 0 && !timedOut;
  results.push({ name, ok, seconds, detail: timedOut ? "timed out" : `exit ${String(run.status)}` });
  if (!ok) break;
}

console.log("\n=== Summary");
for (const r of results) console.log(`${r.ok ? "PASS" : "FAIL"}  ${r.name.padEnd(22)} ${r.seconds}s  (${r.detail})`);
const skipped = gates.length - results.length;
if (skipped > 0) console.log(`NOT RUN  ${skipped} later gate(s) after the first failure`);
process.exit(results.every((r) => r.ok) && skipped === 0 ? 0 : 1);
