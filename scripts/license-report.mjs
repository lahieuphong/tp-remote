#!/usr/bin/env node
// Transitive license inventory for TP Remote (U-15 policy: prefer MIT, Apache-2.0, BSD, ISC;
// flag GPL/AGPL/LGPL/MPL/unknown for review). Uses only `cargo metadata`, `npm ls` and the
// installed package.json files; no network access and no extra tools.
//
// This is an engineering inventory, not a legal compliance certification.
//
// Usage: node scripts/license-report.mjs [outDir]

import { execFileSync, execSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve, sep } from "node:path";

const repo = resolve(import.meta.dirname, "..");
const outDir = resolve(repo, process.argv[2] ?? "docs/qa/evidence/BOOT-01/licenses");
const TARGET = "x86_64-pc-windows-msvc";

const PREFERRED = new Set(["MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC"]);
// OSI-approved permissive licenses outside the preferred list: allowed, but listed.
const OTHER_PERMISSIVE = new Set([
  "0BSD", "MIT-0", "Zlib", "Unicode-3.0", "Unicode-DFS-2016", "BSL-1.0", "CC0-1.0", "Unlicense",
  "BlueOak-1.0.0",
]);
// This repository's own workspace packages (proprietary, "UNLICENSED") are not third-party.
const OWN_NPM_PACKAGES = new Set(["@tp-remote/desktop"]);

/** Classifies an SPDX-like expression: preferred, permissive, or review. */
function classify(expression) {
  if (!expression || typeof expression !== "string") return { verdict: "review", reason: "missing" };
  const alternatives = expression
    .replace(/[()]/g, " ")
    .split(/\s+OR\s+|\//)
    .map((alt) => alt.split(/\s+AND\s+/).map((id) => id.trim().split(/\s+WITH\s+/)[0]).filter(Boolean));
  const rank = (ids) =>
    ids.every((id) => PREFERRED.has(id)) ? 2 : ids.every((id) => PREFERRED.has(id) || OTHER_PERMISSIVE.has(id)) ? 1 : 0;
  const best = Math.max(...alternatives.map(rank));
  if (best === 2) return { verdict: "preferred" };
  if (best === 1) return { verdict: "permissive" };
  return { verdict: "review", reason: "not in the permissive allow-list" };
}

function cargoInventory() {
  const meta = JSON.parse(
    execFileSync("cargo", ["metadata", "--format-version", "1", "--locked", "--offline", "--filter-platform", TARGET], {
      cwd: repo, encoding: "utf8", maxBuffer: 256 * 1024 * 1024,
    }),
  );
  const byId = new Map(meta.packages.map((p) => [p.id, p]));
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
  const members = new Set(meta.workspace_members);

  // What `cargo build -p tp-desktop` uses. `cargo metadata` unifies features across the
  // whole workspace (e.g. the harness enables ts-rs), so per-package `cargo tree` is used.
  const tree = (edges) =>
    new Set(
      execFileSync(
        "cargo",
        ["tree", "-p", "tp-desktop", "-e", edges, "--target", TARGET, "--prefix", "none", "-f", "{p}", "--locked", "--offline"],
        { cwd: repo, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
      )
        .split(/\r?\n/)
        .map((line) => line.replace(/\s*\(\*\)$/, "").trim())
        .filter(Boolean)
        .map((line) => {
          const [name, version] = line.split(" ");
          return `${name}@${version.replace(/^v/, "")}`;
        }),
    );
  // Linked into tp-desktop.exe: normal edges, excluding proc-macros and build scripts.
  const linked = tree("normal,no-proc-macro");
  // Also used while building it: build scripts and proc-macros.
  const buildGraph = tree("normal,build");
  const scopeOf = (key) =>
    linked.has(key) ? "linked into app" : buildGraph.has(key) ? "build-time only" : "dev/test only";

  return [...nodes.keys()]
    .filter((id) => !members.has(id))
    .map((id) => {
      const p = byId.get(id);
      return {
        ecosystem: "cargo", name: p.name, version: p.version,
        license: p.license ?? (p.license_file ? `file:${p.license_file}` : null),
        scope: scopeOf(`${p.name}@${p.version}`),
      };
    });
}

function npmPaths(command) {
  // A fixed command string (no interpolated input); npm is a .cmd shim on Windows.
  return execSync(command, { cwd: repo, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 })
    .split(/\r?\n/).filter(Boolean).map((p) => resolve(p));
}

function npmInventory() {
  const prodPaths = new Set(npmPaths("npm ls --omit=dev --all --parseable"));
  const allPaths = npmPaths("npm ls --all --parseable").filter((p) => p.includes(`${sep}node_modules${sep}`));
  const seen = new Set();
  const out = [];
  for (const path of allPaths) {
    const pkg = JSON.parse(readFileSync(join(path, "package.json"), "utf8"));
    const key = `${pkg.name}@${pkg.version}`;
    if (seen.has(key) || OWN_NPM_PACKAGES.has(pkg.name)) continue;
    seen.add(key);
    const license = typeof pkg.license === "string" ? pkg.license
      : pkg.license?.type ?? (Array.isArray(pkg.licenses) ? pkg.licenses.map((l) => l.type ?? l).join(" OR ") : null);
    out.push({
      ecosystem: "npm", name: pkg.name, version: pkg.version, license,
      scope: prodPaths.has(path) ? "shipped (frontend bundle)" : "dev/build tooling",
    });
  }
  return out;
}

const inventory = [...cargoInventory(), ...npmInventory()].map((e) => ({ ...e, ...classify(e.license) }));
inventory.sort((a, b) => `${a.ecosystem}${a.name}${a.version}`.localeCompare(`${b.ecosystem}${b.name}${b.version}`));

const summary = {};
for (const e of inventory) {
  const key = `${e.ecosystem} | ${e.scope}`;
  summary[key] ??= { total: 0, preferred: 0, permissive: 0, review: 0, licenses: {} };
  const s = summary[key];
  s.total += 1;
  s[e.verdict] += 1;
  s.licenses[e.license ?? "(missing)"] = (s.licenses[e.license ?? "(missing)"] ?? 0) + 1;
}
const flagged = inventory.filter((e) => e.verdict === "review");

mkdirSync(outDir, { recursive: true });
writeFileSync(join(outDir, "license-inventory.json"), JSON.stringify({ generated: new Date().toISOString(), target: TARGET, summary, flagged, inventory }, null, 2));

const lines = [`# License inventory (generated ${new Date().toISOString()})`, "", "Engineering inventory only; not a legal certification.", ""];
for (const [key, s] of Object.entries(summary)) {
  lines.push(`## ${key}: ${s.total} packages (preferred ${s.preferred}, other permissive ${s.permissive}, review ${s.review})`, "");
  for (const [lic, n] of Object.entries(s.licenses).sort((a, b) => b[1] - a[1])) lines.push(`- ${n} × \`${lic}\``);
  lines.push("");
}
lines.push(`## Flagged for review (${flagged.length})`, "");
for (const e of flagged) lines.push(`- ${e.ecosystem} \`${e.name}@${e.version}\` (${e.scope}): \`${e.license ?? "missing"}\` — ${e.reason}`);
writeFileSync(join(outDir, "license-inventory.md"), lines.join("\n") + "\n");

console.log(lines.join("\n"));
