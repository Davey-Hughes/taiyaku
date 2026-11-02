#!/usr/bin/env node
// Count `clippy::pedantic` sites, deduplicated.
//
// The raw warning count is not the site count: clippy emits one message per
// primary span, and a single line such as
//
//     let height = ((width as u64 * page.height() as u64) / page.width() as u64) as u32;
//
// carries four. Reading the raw count makes the workspace look worse than it is
// and makes a fix look like it removed more than it did, so this script exists
// rather than a `grep -c`.
//
// Deduplication is by (file, line, column, lint). Paths outside `crates/` are
// dropped: `float_cmp` fires inside `assert_eq!`'s expansion in rustc's own
// `core/src/macros/mod.rs`, which is not this repository's code and cannot be
// fixed here.
//
// Usage:
//   env -u TAIYAKU_CORPUS cargo clippy --workspace --all-targets --all-features \
//     --message-format=json -- -W clippy::pedantic 2>/dev/null \
//     | node scripts/pedantic-sites.mjs
//
// Exits 0 always. This is an instrument, not a gate — the gate is the advisory
// step in ci.yml, which is non-blocking by design (see docs/ci.md).

import { createInterface } from "node:readline";

const sites = new Map();

for await (const line of createInterface({ input: process.stdin })) {
  if (!line.startsWith("{")) continue;
  let msg;
  try {
    msg = JSON.parse(line);
  } catch {
    continue;
  }
  if (msg.reason !== "compiler-message") continue;
  const code = msg.message?.code?.code;
  if (!code?.startsWith("clippy::")) continue;
  for (const span of msg.message.spans ?? []) {
    if (!span.is_primary) continue;
    if (!span.file_name.startsWith("crates/")) continue;
    const key = `${span.file_name}:${span.line_start}:${span.column_start}:${code}`;
    sites.set(key, { file: span.file_name, lint: code.slice("clippy::".length) });
  }
}

const tally = (pick) => {
  const counts = new Map();
  for (const site of sites.values()) {
    counts.set(pick(site), (counts.get(pick(site)) ?? 0) + 1);
  }
  return [...counts].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
};

console.log(`${sites.size} unique pedantic sites under crates/`);
console.log("\nby lint");
for (const [lint, n] of tally((s) => s.lint)) {
  console.log(`${String(n).padStart(5)}  ${lint}`);
}
console.log("\nby file");
for (const [file, n] of tally((s) => s.file)) {
  console.log(`${String(n).padStart(5)}  ${file}`);
}
