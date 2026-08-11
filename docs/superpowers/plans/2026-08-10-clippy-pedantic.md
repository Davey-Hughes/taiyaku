# `clippy::pedantic` — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every `as` conversion in the workspace either provably bounded or explicitly documented as bounded, and leave behind an advisory gate whose unique-site count is zero — its raw output is never empty; 15 `clippy::float_cmp` warnings from `assert_eq!` on floats in tests print unconditionally, every run, with primary spans clippy attributes to rustc's own `core/src/macros/mod.rs` rather than any file here, which is why the site-counting script excludes them — so that L2's text-block detector, which is float↔int page geometry, is written under a lint that can see a truncating cast.

**Architecture:** No behaviour changes. Every site is resolved one of three ways: a real fix (`u32::from` for a lossless cast, `#[must_use]` on a pure function, `if`/`else` for a two-arm boolean match), written prose (`# Errors`, `# Panics`), or a per-site `#[expect(clippy::…, reason = "…")]` **stating the invariant that bounds the cast and where that invariant is enforced**. `clippy::pedantic` never enters `[workspace.lints]`; it is switched on only by an advisory, non-blocking step in CI and `check-all.sh`.

**Tech Stack:** Rust 2024, clippy on the pinned stable toolchain, Node (for the counting script, matching `scripts/coverage-floor.mjs`'s precedent).

**Source of truth:** `docs/superpowers/specs/2026-08-10-mutation-testing-and-pedantic-design.md`, committed as `2cb7434`. **Part 2 only.** Part 1 (mutation testing) landed as `#39`–`#44` and is finished; this branch stacks on `df8cbd9`.

---

## Global Constraints

- **`clippy::pedantic` must never enter `[workspace.lints]`.** The existing gate — `cargo clippy --workspace --all-targets --all-features -- -D warnings` — stays byte-identical in both `.forgejo/workflows/ci.yml` and `scripts/check-all.sh`. `rust-toolchain.toml` says `channel = "stable"` and CI runs `rust:1-bookworm`; both float, and clippy adds lints to `pedantic` on roughly a six-week cadence. Without this, a rustc release turns someone's unrelated PR red in a file they did not touch.
- **`#[expect]`, never `#[allow]`**, for every per-site suppression. A stale `#[expect]` fails the default gate, which is the mechanism that stops ~180 suppressions decaying into ~180 unchecked claims. This is measured — see "Four probes" below — and it is the one part of this branch that is *not* warn-only. The cost is accepted: a refactor that deletes a cast must delete its attribute too.
- **Per-site, never module- or crate-level**, with exactly one exception category — the two one-off `*_experiment` modules — granted in Task 15 and argued there, producing two module-level suppressions rather than one. A blanket allow in `taiyaku-cli` would switch the lint off in the file with the most cast sites, which is strictly worse than not enabling it at all.
- **A `reason` must state the invariant and where it is enforced.** `reason = "cast is fine"` and `reason = "value is small"` are plan failures. `reason = "nx is non-negative: the explicit nx < 0 || nx >= width guard three lines above returns before reaching this"` is the standard. If the bound cannot be stated, the site is not an `#[expect]` — it is a `try_from` (see Task 9 Step 3).
- **No behaviour changes.** Every commit in Tasks 2–15 must leave `cargo test --workspace --all-features` at the same pass count. Where a fix would change behaviour, the site becomes an `#[expect]` and the observation is recorded in `docs/notes.md`.
- **`env -u TAIYAKU_CORPUS` on every command in this plan.** With the variable set, an unfiltered workspace test run also executes `geometry_experiment` in the debug profile — a run was once killed at 1h09m inside that one test.
- **Doc prose is written, not stamped.** `/// # Errors` followed by `/// Returns an error if it fails.` is worse than nothing and does not count as done (spec §10.4).
- **Commit after every task. No attribution trailers in commit messages.**
- **This repository is on Gitea, not GitHub.** Use the gitea MCP for PRs; `gh` is authenticated against the wrong host. Stacked PRs need `git rebase --onto`. Never hard-wrap a PR body.

---

## The measurement, at `df8cbd9`

Re-measured for this plan rather than quoted. **420 unique sites** under `--all-targets`; **305** of them exist without it, so **115 are test-only**.

| lint | sites | | lint | sites |
| ---- | ----: | - | ---- | ----: |
| `cast_possible_truncation` | 87 | | `too_many_lines` | 9 |
| `cast_precision_loss` | 61 | | `needless_pass_by_value` | 7 |
| `must_use_candidate` | 47 | | `items_after_statements` | 7 |
| `match_bool` | 39 | | `many_single_char_names` | 6 |
| `missing_errors_doc` | 31 | | `missing_panics_doc` | 6 |
| `cast_lossless` | 28 | | `format_push_string` | 6 |
| `redundant_closure_for_method_calls` | 28 | | `format_collect` | 5 |
| `cast_sign_loss` | 16 | | `similar_names` | 5 |
| `map_unwrap_or` | 14 | | 15 lints at 1–2 each | 14 |

The cast family is **192**: 164 needing a stated bound, 28 (`cast_lossless`) mechanically removable.

`float_cmp` does not appear. Both of its unique sites are inside `assert_eq!`'s expansion in rustc's own `core/src/macros/mod.rs`, not in this repository — confirmed by path, not inferred.

## Four probes, and the one that refuted the spec

Run against this workspace before the plan was written, because §9's table was measured on a scratch crate and one of its consequences does not survive contact with this repository.

| probe | expected by spec | measured here |
| ----- | ---------------- | ------------- |
| `#[expect]` live, pedantic **off**, `-D warnings` | silent | **silent, gate exits 0** ✓ |
| `#[expect]` live, pedantic **on** | suppressed | **suppressed** ✓ |
| `#[expect]` **stale**, `-D warnings` | exit 101 | **`error: this lint expectation is unfulfilled`, gate fails** ✓ |
| `[workspace.lints.clippy] must_use_candidate = "allow"` + `-W clippy::pedantic` | lint off | **all 47 still fire; total still 420** ✗ |

**Spec §10.1's mechanism does not work.** Cargo emits `[lints]` as `--allow` flags *before* the `--` arguments, and rustc takes the level from the last matching flag, so the trailing `-W clippy::pedantic` re-enables anything the table allowed. Lint-group specificity resolves attribute levels, not command-line ones.

**The consequence is that §10.1 is deleted rather than repaired.** Its two lints are *fixed* instead of switched off — Tasks 2 and 3 — which was checked against the sites before it was chosen:

- `must_use_candidate` (47) — every site is a pure, side-effect-free function returning a value, and several are ones where discarding the result is a live bug: `PageRange::new`, `Run::paired`, `solve`, `verdict`, `route`, `resolve`, `compose`. `#[must_use]` is the fix clippy is asking for, and it costs one attribute line each.
- `match_bool` (39) — §10.1 justified switching this off with "this codebase leans on `match` for exhaustive dispatch deliberately". That is true of the codebase and **false of these 39 sites**: every one is `match <bool expr> { true => A, false => B }`, a two-arm boolean match, not exhaustive enum dispatch. All 39 convert to `if`/`else`.

So **no lint goes off by name**, the advisory command is exactly what §10.5 wrote with no `-A` flags, and there is nothing to keep in step between `ci.yml` and `check-all.sh` beyond the one command.

## The five PRs

Stacked, in this order. The lint only goes live at the last one, so the advisory step never lands red.

| PR | tasks | sites | what |
| -- | ----- | ----: | ---- |
| 1 | 1, 2, 3 | 86 | The counting script, plus the two lints §10.1 would have switched off |
| 2 | 4, 5, 6 | 79 | Mechanical fixes, no behaviour change |
| 3 | 7, 8 | 37 | Written prose for `# Errors` and `# Panics` |
| 4 | 9–13 | 131 | The casts: read the bound, state it or fix it |
| 5 | 14, 15, 16 | 87 | Style residue, the two experiment files, and the gate wiring |

420 sites, 420 accounted for.

## The verification loop, used by every task

Task 1 builds `scripts/pedantic-sites.mjs`. Every task afterwards ends with:

```bash
env -u TAIYAKU_CORPUS cargo clippy --workspace --all-targets --all-features \
  --message-format=json -- -W clippy::pedantic 2>/dev/null \
  | node scripts/pedantic-sites.mjs
```

which prints the deduplicated total and a per-lint table. **A task is not done because it compiles. It is done when the total dropped by exactly the number this plan states** — no more and no less. A drop larger than stated means a fix removed a site the plan did not account for; find it and say so in the commit message. A drop smaller than stated means a site was missed.

The raw warning count is roughly 1.7× the site count, because a line like `(width as u64 * page.height() as u64) / page.width().max(1) as u64` carries four separate primary spans. **Never read the raw count.** Deduplication is by `(file, line, column, lint)` over paths under `crates/` only.

Both gates must also stay green after every task:

```bash
env -u TAIYAKU_CORPUS cargo clippy --workspace --all-targets --all-features -- -D warnings
env -u TAIYAKU_CORPUS cargo test --workspace --all-features
```

---

# PR 1 — The counter, and the two lints §10.1 would have switched off

### Task 1: The counting script

**Files:**
- Create: `scripts/pedantic-sites.mjs`
- Modify: `docs/ci.md` (append a section)

**Interfaces:**
- Consumes: nothing.
- Produces: `node scripts/pedantic-sites.mjs`, reading clippy's `--message-format=json` on stdin, exiting 0 always and printing a total plus a per-lint and per-file table. Every later task's verification step calls it.

- [ ] **Step 1: Write the script**

Create `scripts/pedantic-sites.mjs`:

```javascript
#!/usr/bin/env node
// Count `clippy::pedantic` sites, deduplicated.
//
// The raw warning count is not the site count and is roughly 1.7x it: clippy
// emits one message per primary span, and a single line such as
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
```

- [ ] **Step 2: Run it and confirm the baseline**

```bash
env -u TAIYAKU_CORPUS cargo clippy --workspace --all-targets --all-features \
  --message-format=json -- -W clippy::pedantic 2>/dev/null \
  | node scripts/pedantic-sites.mjs
```

Expected first line: `420 unique pedantic sites under crates/`

If clippy prints nothing because the build is warm, force it: `touch crates/*/src/lib.rs crates/taiyaku-cli/src/main.rs` and re-run. **Every verification step in this plan needs this same care** — a cached clippy run reports zero sites and looks like success.

- [ ] **Step 3: Document the invocation in `docs/ci.md`**

Append a section, in the voice of the surrounding file:

````markdown
## The pedantic lints, and why they are advisory

`clippy::pedantic` is **not** in `[workspace.lints]` and must not be put there.
The blocking gate stays `cargo clippy --workspace --all-targets --all-features
-- -D warnings`, which sees the two lints that table does set — `todo` and
`dbg_macro` — and nothing from the pedantic group.

The reason is drift in the toolchain rather than distaste for the lints.
`rust-toolchain.toml` says `channel = "stable"` and this file's jobs run
`rust:1-bookworm`; both float, and clippy adds lints to `pedantic` on roughly a
six-week cadence. A pedantic lint in the blocking gate means a rustc release
turns an unrelated PR red in a file its author did not touch.

What *is* enforced is narrower and is the point of the whole exercise: every
suppression in the tree is a per-site `#[expect(clippy::…, reason = "…")]`, and
`unfulfilled_lint_expectations` is a plain rustc lint that the `-D warnings`
gate above already denies. **So a stale expectation fails CI even though the
lint it names is off.** Measured, on this workspace:

| pedantic | the lint would fire | result under `-D warnings` |
| -------- | ------------------- | -------------------------- |
| off      | yes                 | silent, exit 0 |
| on       | yes                 | silent |
| off      | no (stale)          | `error: this lint expectation is unfulfilled`, exit 101 |

That is what stops ~180 suppressions decaying into ~180 unchecked claims:
delete a cast without deleting its attribute and the gate says so. The cost is
real and accepted — a refactor that removes a cast must remove its attribute in
the same commit.

**A `[workspace.lints.clippy]` entry cannot switch a pedantic lint off for the
advisory step.** Measured: with `must_use_candidate = "allow"` in the table, all
47 of its sites still fired under `-W clippy::pedantic`, and the total was
unchanged at 420. Cargo emits `[lints]` as `--allow` flags before the `--`
arguments and rustc takes the last matching level, so the trailing group `-W`
re-enables them. Lint-group specificity resolves *attribute* levels, not
command-line ones. Nothing in this repository relies on that mechanism — no
pedantic lint is off by name — but the next person to reach for it should know
it does not work.

Counting is done with `scripts/pedantic-sites.mjs`, and by hand it is done
wrong: clippy emits one message per primary span, so a line with four casts
reports four warnings at one site, and the raw count runs about 1.7× the real
one.
````

- [ ] **Step 4: Both gates green**

```bash
env -u TAIYAKU_CORPUS cargo clippy --workspace --all-targets --all-features -- -D warnings
env -u TAIYAKU_CORPUS cargo test --workspace --all-features
```

Expected: exit 0 from both. Nothing has changed in `crates/` yet.

- [ ] **Step 5: Commit**

```bash
git add scripts/pedantic-sites.mjs docs/ci.md
git commit -m "Add scripts/pedantic-sites.mjs, and record why pedantic stays advisory

The raw clippy count double-counts: one message per primary span, so a line
with four casts reports four warnings at one site and the raw figure runs
about 1.7x the real one. Deduplicated by (file, line, column, lint) over
crates/ only, the workspace measures 420 sites at df8cbd9.

docs/ci.md records the measured behaviour of #[expect] under a lint that is
off, including the one thing the design assumed and this workspace refutes: a
[workspace.lints.clippy] allow does not survive a trailing -W clippy::pedantic."
```

---

### Task 2: `#[must_use]` on 47 pure functions

**Files:** 14 files across four crates. Full list in Appendix, **T2**.

- Modify: `crates/taiyaku-core/src/lib.rs` (9), `route.rs` (6), `sidecar.rs` (8), `solve.rs` (9), `regions.rs` (4), `slot.rs` (2)
- Modify: `crates/taiyaku-serve/src/lib.rs` (2), `works.rs` (2), `composite.rs` (1), `notes.rs` (1), `reqlog.rs` (1)
- Modify: `crates/taiyaku-read/src/naming.rs` (1), `pages.rs` (1)

**Interfaces:**
- Consumes: `scripts/pedantic-sites.mjs` from Task 1.
- Produces: nothing other tasks depend on. `#[must_use]` changes no signature and no behaviour.

- [ ] **Step 1: Confirm the starting count**

```bash
env -u TAIYAKU_CORPUS cargo clippy --workspace --all-targets --all-features \
  --message-format=json -- -W clippy::pedantic 2>/dev/null \
  | node scripts/pedantic-sites.mjs | head -1
```

Expected: `420 unique pedantic sites under crates/`

- [ ] **Step 2: Add the attribute at all 47 sites**

`#[must_use]` goes **below** the doc comment and **above** any other attribute, which is what `rustfmt` produces and what the existing `#[must_use]`-free code in these files will look like beside it. The shape, using `crates/taiyaku-core/src/lib.rs:50`:

```rust
    /// How many pages the range covers.
    #[must_use]
    pub fn len(&self) -> u32 {
        self.end - self.start + 1
    }
```

Work down the Appendix **T2** table file by file. Every one of the 47 is a `pub fn` that returns a value and has no side effects; there is no site in this task that needs a judgement call.

**One shape needs care.** `crates/taiyaku-core/src/lib.rs:54` is `pub fn is_empty(&self) -> bool`. Clippy's `len_without_is_empty` already forced this to exist beside `len`; `#[must_use]` on it is still correct.

- [ ] **Step 3: Let the compiler find the call sites that discard a result**

```bash
env -u TAIYAKU_CORPUS cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: exit 0. **If it does not**, a call site somewhere discards the return of a function that has no other effect, which is a real finding rather than an obstacle. Do not silence it with `let _ =`. Read the call site, decide whether the call should be deleted or its result used, and record what it was in the commit message and in `docs/notes.md`.

- [ ] **Step 4: Verify the drop**

```bash
env -u TAIYAKU_CORPUS cargo clippy --workspace --all-targets --all-features \
  --message-format=json -- -W clippy::pedantic 2>/dev/null \
  | node scripts/pedantic-sites.mjs | head -1
```

Expected: `373 unique pedantic sites under crates/` (420 − 47).

Confirm `must_use_candidate` is absent from the by-lint table entirely, not merely reduced.

- [ ] **Step 5: Tests, then commit**

```bash
env -u TAIYAKU_CORPUS cargo test --workspace --all-features
git add crates/
git commit -m "must_use_candidate: 47 to 0, fixed rather than switched off

The design (2cb7434 §10.1) proposed turning this lint off by name, on the
grounds that it is meant for published library crates and four of the five
here are internal. Two things against that. The mechanism it named does not
work — a [workspace.lints.clippy] allow is overridden by the trailing -W of
the group, measured, all 47 still firing. And the sites do not support the
argument: every one is a pure function returning a value, and several are ones
where discarding the result is a live bug rather than a style question —
PageRange::new, Run::paired, solve, verdict, route, resolve, compose.

No signature changes and no behaviour changes."
```

---

### Task 3: `match_bool` → `if`/`else` at 39 sites

**Files:** 17 files. Full list in Appendix, **T3**.

- Modify: `crates/taiyaku-cli/src/` — `duplicates.rs` (8), `main.rs` (8), `credits.rs` (5), `fingerprint.rs` (3), `corpus.rs` (1), `detect.rs` (1), `inventory.rs` (1), `regions.rs` (1), `stats.rs` (1)
- Modify: `crates/taiyaku-core/src/` — `route.rs` (2), `sidecar.rs` (2), `solve.rs` (2), `credits.rs` (1), `slot.rs` (1)
- Modify: `crates/taiyaku-read/src/pages.rs` (1), `crates/taiyaku-serve/src/works.rs` (1)

**Interfaces:**
- Consumes: `scripts/pedantic-sites.mjs`.
- Produces: nothing. Every rewrite is expression-for-expression.

- [ ] **Step 1: Rewrite the 35 that are the plain shape**

The plain shape, `crates/taiyaku-core/src/solve.rs:336`:

```rust
    match average_run_length(runs) < MIN_AVERAGE_RUN.min(pages as f32) {
        true => Verdict::Fragmented,
        false => Verdict::Sound,
    }
```

becomes

```rust
    if average_run_length(runs) < MIN_AVERAGE_RUN.min(pages as f32) {
        Verdict::Fragmented
    } else {
        Verdict::Sound
    }
```

Note the `as f32` stays for now — it is a `cast_precision_loss` site and belongs to Task 11.

- [ ] **Step 2: Rewrite the four that are not**

**`crates/taiyaku-cli/src/main.rs:760`** has its arms in `false, true` order. Do not preserve the inversion with `if !`; put the condition the right way up:

```rust
        if self.verdict_split() {
            format!(
                "{} != {}",
                stats::verdict_word(self.composed),
                stats::verdict_word(self.worse_leg)
            )
        } else {
            stats::verdict_word(self.composed).to_string()
        }
```

**`crates/taiyaku-core/src/slot.rs:98`** carries an early `return` in one arm, inside a `let … = if … { … } else { … };`:

```rust
        match all.is_empty() {
            true => return Vec::new(),
            false => all[all.len() / 2] / 2.0,
        }
```

The `return` leaves the whole function, so lift it out rather than nesting it:

```rust
        if all.is_empty() {
            return Vec::new();
        }
        all[all.len() / 2] / 2.0
```

Check the surrounding `if … { … } else { portrait[portrait.len() / 2] }` still type-checks as one expression after the lift; if the borrow checker objects, keep the `if`/`else` shape with the `return` inside the `if` block instead.

**`crates/taiyaku-cli/src/fingerprint.rs:188`** and **`crates/taiyaku-cli/src/inventory.rs:276`** are statement-position matches whose arms are side effects rather than values:

```rust
        if *size >= LARGE_PAGE_BYTES {
            alone.push(index);
        } else {
            parallel.push(index);
        }
```

- [ ] **Step 3: Keep the comments attached to the right branch**

**`crates/taiyaku-cli/src/credits.rs:280`** has a comment above its `true` arm explaining the empty case. It must move with the branch it explains, into the `if` body:

```rust
    Err(if spokes.is_empty() {
        // The empty case: with nothing to suggest, the sentence has to end on
        // its own rather than trail off after a bare `--against:`.
        format!(
            "`{edition}` is this work's hub, so which edition it is unpaired against is \
             a choice rather than a default, and this document has no alignment naming an \
             edition to suggest. Name one with --against."
        )
    } else {
        // … the existing `false` arm, unchanged
    })
```

Read every one of the 39 for an attached comment before rewriting it; the four named above are the ones found on the first read, not a guarantee there are no others.

- [ ] **Step 4: Format, then verify the drop**

```bash
cargo fmt --all
env -u TAIYAKU_CORPUS cargo clippy --workspace --all-targets --all-features \
  --message-format=json -- -W clippy::pedantic 2>/dev/null \
  | node scripts/pedantic-sites.mjs | head -1
```

Expected: `334 unique pedantic sites under crates/` (373 − 39).

`match_bool` must be absent from the by-lint table. Watch for a *larger* drop: rewriting `credits.rs:196` and `:203` may also clear a `redundant_closure_for_method_calls` or `format_push_string` site that Task 5 or 14 has counted. If the total lands below 334, find which lint moved and note it — the later task's expected figure needs adjusting.

- [ ] **Step 5: Tests, then commit**

```bash
env -u TAIYAKU_CORPUS cargo test --workspace --all-features
env -u TAIYAKU_CORPUS cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/
git commit -m "match_bool: 39 to 0, and the design's reason for keeping them did not describe them

2cb7434 §10.1 proposed switching this lint off, on the grounds that this
codebase leans on `match` for exhaustive dispatch deliberately. That is true
of the codebase and false of these 39: every one is
`match <bool> { true => A, false => B }`, a two-arm boolean match rather than
exhaustive dispatch over an enum. All 39 convert.

Four needed more than a mechanical rewrite: main.rs:760 had its arms in
false/true order and is now written the right way up rather than behind a
`!`; slot.rs:98 carried an early return, lifted out of the expression;
fingerprint.rs:188 and inventory.rs:276 are statement-position matches whose
arms are pushes rather than values. Comments attached to a `true` arm moved
into the branch they explain."
```

**PR 1 ends here. Open it against `main` at 334 sites.**

---

# PR 2 — Mechanical fixes

Rebase onto PR 1's head. Nothing in this PR changes behaviour; every fix is one clippy is able to describe exactly.

### Task 4: `cast_lossless` — 25 sites

**Files:** Appendix **T4**. Largest: `crates/taiyaku-core/src/sidecar.rs`, `crates/taiyaku-cli/src/report.rs`, `crates/taiyaku-cli/src/main.rs`.

**Interfaces:**
- Consumes: `scripts/pedantic-sites.mjs`.
- Produces: the `From` conversions Task 9–13 will read around; a cast rewritten here is one fewer bound to state there.

- [ ] **Step 1: Replace each lossless `as` with the `From` conversion**

`x as u64` where `x: u32` becomes `u64::from(x)`. `x as f64` where `x: u32` becomes `f64::from(x)`. The rule is that the conversion is infallible and `From` is implemented for it, which is exactly the set clippy flags.

`crates/taiyaku-cli/src/report.rs:56` carries three of them on one line:

```rust
    let height = ((width as u64 * page.height() as u64) / page.width().max(1) as u64).max(1) as u32;
```

becomes

```rust
    let height = ((u64::from(width) * u64::from(page.height()))
        / u64::from(page.width().max(1)))
    .max(1) as u32;
```

The trailing `as u32` is **not** a `cast_lossless` site and stays — it is `cast_possible_truncation`, and Task 13 states its bound.

- [ ] **Step 2: Format, verify, test**

```bash
cargo fmt --all
env -u TAIYAKU_CORPUS cargo clippy --workspace --all-targets --all-features \
  --message-format=json -- -W clippy::pedantic 2>/dev/null \
  | node scripts/pedantic-sites.mjs | head -1
env -u TAIYAKU_CORPUS cargo test --workspace --all-features
```

Expected: `309 unique pedantic sites under crates/` (334 − 25), `cast_lossless` absent.

- [ ] **Step 3: Commit**

```bash
git add crates/
git commit -m "cast_lossless: 25 to 0

u32::from and f64::from in place of `as` where the conversion cannot lose
anything. Behaviour identical by construction — these are the conversions the
standard library implements From for."
```

---

### Task 5: `redundant_closure_for_method_calls` (28) and `map_unwrap_or` (12)

**Files:** Appendix **T5**, 40 sites. Largest: `crates/taiyaku-cli/src/credits.rs`, `main.rs`, `crates/taiyaku-core/src/sidecar.rs`.

- [ ] **Step 1: Replace the redundant closures**

`.map(|s| s.to_string())` becomes `.map(ToString::to_string)`. `.map(|e| e.to_string())` on an error becomes `.map_err(ToString::to_string)` only where it already was a `map_err`; do not change which combinator is called. `.map(|p| p.to_string_lossy())` becomes `.map(Path::to_string_lossy)` — the path form needs the type named, and the type is whatever the iterator yields, so read it rather than assuming `Path`.

- [ ] **Step 2: Replace the `map(…).unwrap_or(…)` chains**

`.map(f).unwrap_or(d)` becomes `.map_or(d, f)`. `.map(f).unwrap_or_else(g)` becomes `.map_or_else(g, f)`. **The argument order reverses** — the default comes first — which is the one way to get this wrong silently, because both arguments are often closures returning the same type.

After each one, read the result aloud: `map_or(default, function)`. `crates/taiyaku-cli/src/credits.rs:201` already uses the correct form and is a good model:

```rust
            self.recurs
                .map_or_else(|| "-".to_string(), |d| d.to_string()),
```

- [ ] **Step 3: Format, verify, test**

Expected: `269 unique pedantic sites under crates/` (309 − 40). Both lints absent.

```bash
cargo fmt --all
env -u TAIYAKU_CORPUS cargo test --workspace --all-features
```

- [ ] **Step 4: Commit**

```bash
git add crates/
git commit -m "redundant_closure_for_method_calls and map_unwrap_or: 40 to 0

Method references in place of closures that only forward, and map_or/
map_or_else in place of map().unwrap_or(). The second reverses argument
order — the default comes first — which is the one way to get this wrong
without the compiler noticing, so each was read back after the change."
```

---

### Task 6: The 14 singletons

**Files:** Appendix **T6**. Fourteen lints firing once or twice each, across 12 files.

- [ ] **Step 1: Fix each, guided by clippy's own suggestion**

Run clippy scoped to one lint at a time to read its suggestion in full:

```bash
env -u TAIYAKU_CORPUS cargo clippy --workspace --all-targets --all-features \
  -- -A clippy::all -W clippy::manual_let_else
```

The fourteen: `explicit_iter_loop` (2), `manual_midpoint` (2), `manual_ilog2`, `manual_assert_eq`, `manual_let_else`, `semicolon_if_nothing_returned`, `single_match_else`, `nonminimal_bool`, `needless_raw_string_hashes`, `unreadable_literal`, `enum_glob_use`, `doc_markdown`.

- [ ] **Step 2: Two need judgement rather than the suggestion**

**`manual_midpoint`** at `crates/taiyaku-core/src/solve.rs:772` is inside a test assertion:

```rust
        (cal.floor - (cal.noise + cal.signal) / 2.0).abs() < 1e-6,
```

Clippy wants `f32::midpoint(cal.noise, cal.signal)`. That is the right change *and* it changes the arithmetic: `midpoint` is overflow-free and rounds differently at the extremes. This assertion pins the calibration floor's definition, so **check the corresponding production line first** — if production computes `(noise + signal) / 2.0`, the test must keep matching production, and this site becomes an `#[expect(clippy::manual_midpoint, reason = "…")]` naming the production line it mirrors. If production is also flagged, change both together.

**`unreadable_literal`** at `crates/taiyaku-serve/src/reqlog.rs:289` is `bytes: 1180617` in a test fixture. `1_180_617` is the fix. Confirm no assertion elsewhere compares against the literal spelled the old way.

- [ ] **Step 3: Format, verify, test, commit**

Expected: `255 unique pedantic sites under crates/` (269 − 14), or `256` if `manual_midpoint` at `solve.rs:772` became an `#[expect]` rather than a fix — in which case it is still resolved and the by-lint table shows `manual_midpoint` gone.

```bash
cargo fmt --all
env -u TAIYAKU_CORPUS cargo test --workspace --all-features
git add crates/
git commit -m "The fourteen pedantic singletons: 14 to 0

Twelve took clippy's own suggestion. manual_midpoint in solve.rs's
calibration assertion did not: f32::midpoint is overflow-free and rounds
differently at the extremes, and that assertion exists to pin what production
computes, so it keeps mirroring production rather than improving on it."
```

**PR 2 ends here. Open it stacked on PR 1 at 255 sites.**

---

# PR 3 — Written prose

Rebase onto PR 2's head. **This is the PR where a stamped template is a failure rather than a shortcut** (spec §10.4).

### Task 7: `missing_errors_doc` — 31 sites

**Files:** Appendix **T7**. Largest: `crates/taiyaku-core/src/sidecar.rs` (9), `crates/taiyaku-read/src/pages.rs` (7), `crates/taiyaku-serve/src/notes.rs` (4).

- [ ] **Step 1: For each function, name the conditions rather than the type**

The standard, written against what the function actually returns `Err` for. For `crates/taiyaku-read/src/pages.rs`'s reader:

```rust
/// # Errors
///
/// Returns an error if the archive cannot be opened, or if it holds no entry
/// this crate reads as a page — the message then lists the extensions it did
/// hold, so a `.webp`-only release is distinguishable from an empty file.
```

The test of a good one: a caller who has not read the body can tell from it which of their inputs would produce which error. `Returns an error if the operation fails` fails that test and is a plan failure.

> **This example was wrong when first written, and the way it was wrong is the
> point.** It carried a third clause — "or if an entry's name is not valid
> UTF-8, which the natural-order sort needs" — which is false against the
> locked `zip` 7.2.0. `central_header_to_zip_file_inner` builds the name with
> `String::from_utf8_lossy` or `.from_cp437()`, both total and infallible, so a
> mis-encoded entry name is **silently corrupted into a garbled page name, not
> refused**. A reviewer established this by reading the dependency's source and
> then hand-crafting a zip with an entry named `[0xFF, b'.', b'j', b'p', b'g']`:
> it opened, and the entry was accepted as an ordinary `.jpg` page.
>
> By then the invented clause had been copied into **six** of `pages.rs`'s
> seven `# Errors` sections, because it was in this plan and this plan said it
> was the standard. A specific, confident, plausible sentence propagates
> further than a vague one, which is exactly the risk §10.4 creates by
> demanding specificity: **an example in a plan is read as verified, so a
> plan's examples must be held to the standard the plan sets for the work.**
> This one was not measured before it was written down.

- [ ] **Step 2: Where a function returns `Result` but cannot fail, say so and check**

Some of the 31 will turn out to have exactly one `?` that itself cannot fail, or a `Result` kept only for signature symmetry. Do not write fiction. If a function genuinely cannot return `Err`, that is a finding: record it in `docs/notes.md` and consider whether the signature should change — but **do not change the signature in this PR**, because that is a behaviour change and this PR has none. Write the `# Errors` section describing the reachable cases and note the discrepancy.

- [ ] **Step 3: Verify and commit**

Expected: `224 unique pedantic sites under crates/` (255 − 31).

```bash
env -u TAIYAKU_CORPUS cargo test --workspace --all-features
git add crates/
git commit -m "missing_errors_doc: 31 to 0, written rather than stamped

Each names the conditions that produce an Err, so a caller can tell from the
doc which of their inputs produces which error. '# Errors / Returns an error
if it fails' would have cleared the lint and told a reader nothing."
```

---

### Task 8: `missing_panics_doc` — 6 sites

**Files:** Appendix **T8**.

- [ ] **Step 1: Find what actually panics, and say where**

Six functions. For each, locate the `unwrap`, `expect`, slice index or `debug_assert!` that clippy found, and write the condition:

```rust
/// # Panics
///
/// Panics if `start > end`. `PageRange` is constructed from solver output and
/// from parsed span notation, both of which are checked before they reach
/// here; the `debug_assert!` exists to catch a third caller appearing.
```

Two of these will be `debug_assert!` in `crates/taiyaku-core/src/lib.rs` — `PageRange::new`'s "page range must not be inverted" and `Run::paired`'s equal-length check. Those fire only under `debug_assertions`, which the `# Panics` section must say, because `check-all.sh`'s corpus pass runs in release where they do not.

- [ ] **Step 2: Verify and commit**

Expected: `218 unique pedantic sites under crates/` (224 − 6).

```bash
env -u TAIYAKU_CORPUS cargo test --workspace --all-features
git add crates/
git commit -m "missing_panics_doc: 6 to 0

Each names the condition and where it is already checked. The two debug_assert
cases say so explicitly: they do not fire in release, which is the profile
check-all.sh runs the corpus checks under."
```

**PR 3 ends here. Open it stacked on PR 2 at 218 sites.**

---

# PR 4 — The casts

Rebase onto PR 3's head. **131 sites, and this is the substance of the branch.** Every one is read against its surrounding code and resolved as a `try_from` where the bound is genuinely unproven, or an `#[expect]` stating the bound and where it is enforced.

The procedure, applied identically in Tasks 9–13:

1. Read the cast and the function around it. Find what constrains the input.
2. If the constraint is enforced in code — a guard, a `.clamp`, a `.min`, a contract on a constructor — the `reason` names that enforcement and where it is. This is the common case.
3. If the constraint is a fact about the material rather than about the code — "no page is 4 billion pixels wide" — the `reason` says *that*, plainly, and says it is a property of the input rather than of the code. That is still a valid `#[expect]` and it is more honest than a `try_from().unwrap()`.
4. If there is no constraint, it is a defect. Fix it with `try_from` and propagate the error, or with a `clamp` that has a stated reason. **Record every one of these in `docs/notes.md`** — the design predicted zero live defects from a sample of ten, and a defect found here contradicts that prediction and is the most valuable thing this branch could produce.

The five bounds the design already read and recorded (§7) are quoted in the tasks below and should be checked rather than re-derived; if the code has moved since `2cb7434`, the check is the finding.

### Task 9: `crates/taiyaku-cli/src/detect.rs` — 26 sites

**Files:** Modify `crates/taiyaku-cli/src/detect.rs`. Appendix **T9**.

**Interfaces:**
- Consumes: nothing from earlier tasks beyond the counting script.
- Produces: the precedent every later cast task follows. Do this file first; it is the one the branch exists for.

- [ ] **Step 1: Read the file's relationship to L2 before writing a single reason**

`detect.rs` is the text-block detector, and `docs/status.md` records it as measuring the wrong population and wanting a redesign — O31 to O34. Mutation testing deliberately skipped its 37 survivors for that reason.

**That is not a reason to skip it here, and the distinction matters.** Mutation testing skipped it because 44 new tests would *pin* behaviour those open items exist to change. An `#[expect]` pins nothing: it documents a bound the code already relies on, and if the redesign removes the cast, the stale-expect rule forces the attribute out with it. The design's whole argument for enabling pedantic is this file's successor (§8: "L2's text-block detector is float↔int page geometry — the branch after this one").

- [ ] **Step 2: The one the design already read**

`detect.rs:161` is the flood-fill's `nx as u32`, and §7 records the bound:

```rust
                #[expect(
                    clippy::cast_sign_loss,
                    reason = "nx is non-negative here: the explicit `nx < 0 || nx >= width` \
                              test three lines above continues before reaching this."
                )]
```

Check that the guard is still three lines above and still says that. If the file has moved, the reason names where the guard is *now*.

- [ ] **Step 3: Work the remaining 25**

Group them as you go — several will share one bound (a page dimension that `image` guarantees fits `u32`, a fraction already `.clamp(0.0, 1.0)`). **A shared bound still gets one attribute per site**, but the reason may name the shared invariant once and refer to it: `reason = "as detect_page's scale contract above: stroke_scale is >= 1.0 by the max() on line NNN"`.

- [ ] **Step 4: Verify and commit**

Expected: `192 unique pedantic sites under crates/` (218 − 26).

```bash
cargo fmt --all
env -u TAIYAKU_CORPUS cargo clippy --workspace --all-targets --all-features -- -D warnings
env -u TAIYAKU_CORPUS cargo test --workspace --all-features
git add crates/taiyaku-cli/src/detect.rs
git commit -m "detect.rs: 26 cast sites, each with its bound stated

The file O31-O34 say wants a redesign, and the one the whole branch is for:
L2's detector is float-int page geometry, and `as` is invisible to every other
gate this repository has. An #[expect] pins nothing the redesign must keep —
if the redesign removes a cast, the stale-expect rule removes its attribute
with it."
```

---

### Task 10: `crates/taiyaku-core/src/sidecar.rs` — 25 sites

**Files:** Modify `crates/taiyaku-core/src/sidecar.rs`. Appendix **T10**.

- [ ] **Step 1: The two the design already read**

`sidecar.rs:1232` — `b_slots[(a_from as i64 + offset) as usize]`. §7's bound, and §8 names this as the case that motivates the whole mechanism: the bound is currently held in a comment three paragraphs away from the cast it constrains.

```rust
                #[expect(
                    clippy::cast_sign_loss,
                    reason = "the sum is non-negative: a_from >= s_start by split_around's \
                              contract, and offset = index(b_slots, ..) - s_start, so \
                              a_from + offset >= index(b_slots, ..) >= 0. A negative sum \
                              would wrap to a huge index and panic here rather than \
                              silently mis-index."
                )]
```

`sidecar.rs:1281` — `cs.max(a_start as i64) as u32`: the `.max`/`.min` clamp to u32-valued bounds and the guard on the line above forces non-negative.

**Verify both against the current code before copying them in.** They were read at `2cb7434`; `#43` and `#44` touched this crate.

- [ ] **Step 2: Work the remaining 23, then verify and commit**

Expected: `167 unique pedantic sites under crates/` (192 − 25).

```bash
git add crates/taiyaku-core/src/sidecar.rs
git commit -m "sidecar.rs: 25 cast sites, each with its bound stated

Including the two §7 of the design read by hand. 1232's bound was held in a
comment three paragraphs from the cast it constrains; it now sits on it."
```

---

### Task 11: `crates/taiyaku-core/src/solve.rs` — 17 sites

**Files:** Modify `crates/taiyaku-core/src/solve.rs`. Appendix **T11**.

- [ ] **Step 1: The one the design already read**

`solve.rs:153` — `(values.len() as f32 - 1.0) * fraction`. f32 loses integer precision above 2²⁴, which is 16.7 million pages.

```rust
    #[expect(
        clippy::cast_precision_loss,
        reason = "f32 represents every integer below 2^24 exactly. values.len() is a slot \
                  count, and every edition in this corpus is a few hundred slots; the loss begins at \
                  16.7 million."
    )]
```

- [ ] **Step 2: Note where a bound is a property of the material**

Most of `solve.rs`'s remaining 16 are page and slot counts crossing into `f32` for the similarity and calibration arithmetic. The bound is a fact about editions rather than about the code, and the `reason` should say so in those terms.

> **Do not cite "617" as an edition size. It was wrong here, and it propagated.**
> Earlier drafts of this section said "the largest edition in the corpus is 617
> slots". It is not an edition size at all: `docs/status.md` line 101 records
> **617 pages *across three* editions** of one work — the sum, not the maximum.
> `status.md` separately records a single edition at 195 pages, and 629 *slots*
> across the same three. The largest single edition is a few hundred, not 617.
>
> By the time a reviewer checked the citation against `status.md`, the figure
> had been written into seven `#[expect]` reasons in `main.rs` and thirteen in
> `solve.rs`, and one review had already passed it on the grounds that it
> "traces to the authoritative source the agent was handed" — which was this
> plan. **A number in a plan is treated as measured.** Cite the document and
> line you took a figure from, or do not cite a figure: the safety argument
> here never needed one, since any count of a few hundred is more than twenty
> orders of magnitude below `f32`'s 2^24 and `u32::MAX` alike.

This is procedure point 3 above, and it is the honest answer here: there is no guard to point at, and inventing a `try_from` on a page count would add an error path no caller can trigger.

- [ ] **Step 3: Verify and commit**

Expected: `150 unique pedantic sites under crates/` (167 − 17).

```bash
git add crates/taiyaku-core/src/solve.rs
git commit -m "solve.rs: 17 cast sites, each with its bound stated

Mostly slot counts crossing into f32 for the calibration arithmetic, where the
bound is a property of the material rather than of the code — f32 is exact
below 2^24 and every edition in this corpus is a few hundred slots. Those reasons say so
plainly rather than pointing at a guard that does not exist."
```

---

### Task 12: `crates/taiyaku-cli/src/main.rs` — 15 sites

**Files:** Modify `crates/taiyaku-cli/src/main.rs`. Appendix **T12**.

- [ ] **Step 1: Work the 15**

`main.rs` is argument parsing and the command handlers. Expect the casts to be page indices and counts crossing between `usize` and `u32` for display and for the sidecar's span notation. `#44` closed this file's 70 mutation survivors and extracted several helpers, so several casts now sit in named functions with clearer contracts than they had at `2cb7434`.

- [ ] **Step 2: Verify and commit**

Expected: `135 unique pedantic sites under crates/` (150 − 15).

```bash
git add crates/taiyaku-cli/src/main.rs
git commit -m "main.rs: 15 cast sites, each with its bound stated"
```

---

### Task 13: The remaining 48 cast sites

**Files:** 21 files across all five crates. Appendix **T13**. Largest: `report.rs` (4), `cover.rs` (4), `credits.rs` ×2 (3 each), `regions.rs` (3), `route.rs` (3), `slot.rs` (3), `pages.rs` (3).

- [ ] **Step 1: The one the design already read**

`report.rs:56` — `(width as u64 * height as u64 / width) as u32`, bounded by `height`. After Task 4 this line reads `((u64::from(width) * u64::from(page.height())) / u64::from(page.width().max(1))).max(1) as u32`, and the bound to state is that the quotient cannot exceed `page.height()`, which is a `u32`:

```rust
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the quotient cannot exceed page.height(), which is u32: width is the \
                  caller's thumbnail width and page.width().max(1) >= 1, so the product \
                  divided by the divisor is at most width * height, and thumbnail is \
                  called only with THUMB_WIDTH. Check that if a caller is added."
    )]
```

**Check the claim about callers before writing it.** `grep -rn 'thumbnail(' crates/` and confirm the width argument at each.

- [ ] **Step 2: Work the remaining 47, one file at a time, committing per crate**

Four commits: `taiyaku-cli` (13 remaining), `taiyaku-core` (12), `taiyaku-serve` (14), `taiyaku-read` + `taiyaku-wasm` (7). Each is small enough to review as a unit and each leaves the tree green.

- [ ] **Step 3: Verify the whole PR**

Expected: `87 unique pedantic sites under crates/` (135 − 48).

Confirm the by-lint table shows `cast_possible_truncation`, `cast_precision_loss` and `cast_sign_loss` **only** in `geometry_experiment.rs` and `region_experiment.rs`, which Task 15 handles. If any other file still carries one, it was missed.

```bash
env -u TAIYAKU_CORPUS cargo clippy --workspace --all-targets --all-features -- -D warnings
env -u TAIYAKU_CORPUS cargo test --workspace --all-features
```

- [ ] **Step 4: Write up what the reading found**

Append a section to `docs/notes.md` titled "192 casts, read one at a time". It must record, at minimum: how many sites were bounded by an enforced guard, how many by a property of the material, how many became a `try_from`, and **every site where the reading found a genuine defect** — with the input that would trigger it. If the count of defects is zero, say zero: the design predicted zero from a sample of ten, and confirming that over 192 is a result worth stating.

```bash
git add docs/notes.md
git commit -m "notes: what reading 192 casts found"
```

**PR 4 ends here. Open it stacked on PR 3 at 87 sites.**

---

# PR 5 — Style, the experiment files, and the gate

Rebase onto PR 4's head.

### Task 14: The style residue — 42 sites

**Files:** Appendix **T14**. Eleven lints across 20 files.

`needless_pass_by_value` (7), `items_after_statements` (6), `many_single_char_names` (6), `format_push_string` (6), `format_collect` (5), `too_many_lines` (5), `similar_names` (3), `trivially_copy_pass_by_ref`, `ref_option`, `struct_field_names`, `option_option`.

- [ ] **Step 1: Fix the ones where clippy is right**

`format_push_string` (6) — `s.push_str(&format!(…))` becomes `write!(s, …)` with `use std::fmt::Write`, which avoids the intermediate allocation. `format_collect` (5) — `.map(|x| format!(…)).collect::<String>()` becomes a `fold` with `write!`, same reason. `items_after_statements` (6) — move the `const` or `fn` to the top of its block. `needless_pass_by_value` (7) — take a reference, unless the value is genuinely consumed; check each call site.

- [ ] **Step 2: `#[expect]` the ones where it is not, with the reason stating why**

`too_many_lines` (5) and `many_single_char_names` (6) are the likely candidates. A 200-line corpus check that prints a table is long because the table has many columns, not because it should be split; a flood-fill using `x`, `y`, `nx`, `ny`, `w`, `h` is using the names its domain uses. Say that:

```rust
#[expect(
    clippy::many_single_char_names,
    reason = "x, y, nx, ny, w, h are the names image geometry uses; renaming them to \
              `next_x` and `height` would make this harder to check against the \
              flood-fill it implements, not easier."
)]
```

**Do not `#[expect]` a lint you have not tried to fix.** For each of the 42, the commit message should be able to say which of the two it was.

- [ ] **Step 3: Verify and commit**

Expected: `45 unique pedantic sites under crates/` (87 − 42) — all 45 in the two experiment files.

```bash
cargo fmt --all
env -u TAIYAKU_CORPUS cargo test --workspace --all-features
git add crates/
git commit -m "The style residue: 42 to 0"
```

---

### Task 15: The two experiment files — 45 sites

**Files:**
- Modify: `crates/taiyaku-cli/src/main.rs:25-36` — the two `mod` declarations and the comments already above them

**Interfaces:**
- Consumes: nothing.
- Produces: two module-level suppressions in the tree — one per experiment file, and the only ones. Everything else is per-site.

- [ ] **Step 1: Grant the exception at the two declarations that already argue for it**

`geometry_experiment.rs` (29 sites) and `region_experiment.rs` (16) are one-off measurements rather than shipped code, and `docs/status.md` says so at length. They are already excluded from the coverage gate by name — `--ignore-filename-regex '(geometry_experiment|region_experiment|/corpus)\.rs$'` appears twice in `check-all.sh` and twice in `ci.yml` — and both are already `#[cfg(test)]` with a comment saying why. This task extends an existing precedent; it does not invent one.

The current state of `crates/taiyaku-cli/src/main.rs:25-29`:

```rust
// A one-off measurement rather than a shipped command, and gated on
// `TAIYAKU_CORPUS` exactly as `corpus` is — so it is declared the same way, and
// no part of it is compiled into the binary.
#[cfg(test)]
mod geometry_experiment;
```

Extend the existing comment rather than replacing it, and add the attribute:

```rust
// A one-off measurement rather than a shipped command, and gated on
// `TAIYAKU_CORPUS` exactly as `corpus` is — so it is declared the same way, and
// no part of it is compiled into the binary.
//
// It also carries one of this workspace's two module-level lint
// suppressions — the other is `region_experiment`'s, below, granted on the
// same grounds. Everything else pedantic flags is fixed or carries a
// per-site #[expect] stating its bound; §10.3 of the design forbids
// module-level allows precisely so that a file with many casts cannot
// switch the lint off wholesale. The exception is granted on the same
// grounds the coverage gate already excludes this file by name: its
// arithmetic exists to print a table for docs/notes.md once, and writing 29
// bounds for it would spend the mechanism's credibility on the code least
// able to hurt anyone.
//
// The cost, stated: a truncating cast introduced here is invisible. That is
// accepted because a wrong number in a measurement shows up as a wrong number
// in the table it prints, which a person reads before it reaches notes.md.
#[cfg(test)]
#[allow(
    clippy::pedantic,
    reason = "one-off measurement; see above and docs/ci.md"
)]
mod geometry_experiment;
```

and the same at `:32-36` for `region_experiment`, with `16` in place of `29`.

`#[allow]`, not `#[expect]`, and deliberately: an `#[expect(clippy::pedantic)]` would become stale the moment the file stops firing any pedantic lint, which for a module-wide suppression is a false alarm rather than a signal.

- [ ] **Step 2: Verify**

Expected: `0 unique pedantic sites under crates/`.

If the count is not zero, the remaining sites are the ones this plan's arithmetic missed. Resolve each per Task 9's procedure — do not widen the allow.

- [ ] **Step 3: Commit**

```bash
git add crates/
git commit -m "The two experiment files: one module-level allow each, argued

The only two module-level suppressions in the workspace. Granted on the same
grounds the coverage gate already excludes these two files by name: they are
one-off measurements whose output is a table a person reads, not code that
ships. The cost — a truncating cast in either is invisible — is stated at the
declaration rather than left implicit.

Workspace pedantic sites: 0."
```

---

### Task 16: Wire the advisory step, and close the item

**Files:**
- Modify: `.forgejo/workflows/ci.yml` (the `lint` job)
- Modify: `scripts/check-all.sh` (the `lint` job section)
- Modify: `docs/status.md`
- Modify: `docs/notes.md`

- [ ] **Step 1: Add the advisory step to `ci.yml`**

In the `lint` job, after the existing `Clippy` step and leaving it byte-identical:

```yaml
      - name: Clippy
        run: cargo clippy --workspace --all-targets --all-features -- -D warnings
      # Advisory and non-blocking, deliberately. `clippy::pedantic` is not in
      # [workspace.lints] and must not be: this container is `rust:1-bookworm`
      # and rust-toolchain.toml says `channel = "stable"`, both of which float,
      # and clippy adds lints to the group on roughly a six-week cadence. In the
      # blocking gate above, a rustc release would turn an unrelated PR red in a
      # file its author did not touch.
      #
      # Its output is empty as of this commit. So anything that appears here is
      # either a cast written without a stated bound or a lint a new rustc
      # added — and the per-site #[expect] attributes are still enforced by the
      # blocking step above, because a stale one is `unfulfilled_lint_expectations`,
      # an ordinary rustc lint that `-D warnings` already denies.
      #
      # See docs/ci.md, "The pedantic lints, and why they are advisory".
      - name: Clippy (pedantic, advisory)
        continue-on-error: true
        run: cargo clippy --workspace --all-targets --all-features -- -W clippy::pedantic
```

Confirm `continue-on-error: true` is the right key for this Forgejo runner — check another job in the file for prior art, and if there is none, verify against a pushed branch before claiming the step is non-blocking.

- [ ] **Step 2: Add the same to `check-all.sh`**

In the `--- the lint job ---` section:

```bash
# --- the `lint` job ---
run cargo fmt --all -- --check
run cargo clippy --workspace --all-targets --all-features -- -D warnings

# Advisory and non-blocking — the `|| true` is the point, not an oversight, and
# it mirrors `continue-on-error: true` on the same step in ci.yml. See
# docs/ci.md, "The pedantic lints, and why they are advisory": pedantic stays
# out of [workspace.lints] so a floating toolchain cannot turn the blocking gate
# above red, while the per-site #[expect] attributes stay enforced by it, since
# a stale one is an ordinary rustc lint.
#
# Empty output is the expected state. Anything here is a new unguarded cast or a
# lint a newer clippy added.
echo; echo "==> cargo clippy --workspace --all-targets --all-features -- -W clippy::pedantic (advisory)"
cargo clippy --workspace --all-targets --all-features -- -W clippy::pedantic || true
```

It is written out rather than passed to `run`, because `run` inherits `set -e` and would abort the gate on a warning.

- [ ] **Step 3: Confirm both gates behave**

```bash
scripts/check-all.sh --no-web --no-wasm
```

Expected: `all green`, with the advisory step printing no warnings.

Then prove the advisory step can still speak. Add a throwaway `let _x = 1u64 as u32;` to any file, re-run, confirm it appears and the gate is still green, then remove it.

- [ ] **Step 4: Update `docs/status.md`**

Two edits. In "What is built", record that the workspace carries `N` per-site `#[expect]` attributes and one module-level allow, and that the advisory step exists. In "What is settled", add the entry:

```markdown
- **`clippy::pedantic` is on as an advisory step, never in `[workspace.lints]`.**
  420 sites measured at `df8cbd9`; 375 fixed or given a per-site
  `#[expect(…, reason = …)]` stating the bound, and 45 in the two one-off
  experiment files covered by a module-level allow, argued at the declaration.
  It beat a blocking gate, which a floating toolchain turns red on an unrelated
  PR every six weeks or so; and it beat leaving the lint off, because `as` is
  invisible to every other instrument here — `docs/ci.md` records the debug
  profile as the only one with overflow checks, and overflow checks do not apply
  to `as`. The enforcement that remains is narrow and real: a stale `#[expect]`
  is `unfulfilled_lint_expectations`, an ordinary rustc lint the existing
  `-D warnings` gate already denies, so a cast deleted without its attribute
  fails CI. **The design's own mechanism for switching two lints off by name
  does not work** — a `[workspace.lints.clippy]` allow is overridden by the
  trailing `-W` of the group, measured — so both were fixed instead, which
  turned out to be the better answer on the sites: all 47 `must_use_candidate`
  were pure functions where discarding the result is a bug, and all 39
  `match_bool` were two-arm boolean matches rather than the exhaustive dispatch
  the design's argument described.
```

- [ ] **Step 5: Update `docs/notes.md`**

Append the record: the four probes and their results, the refuted one first; the 420 → 0 arithmetic per task; and whatever Task 13's reading found.

- [ ] **Step 6: Final verification, then commit**

```bash
env -u TAIYAKU_CORPUS cargo clippy --workspace --all-targets --all-features \
  --message-format=json -- -W clippy::pedantic 2>/dev/null \
  | node scripts/pedantic-sites.mjs | head -1
scripts/check-all.sh
```

Expected: `0 unique pedantic sites under crates/`, and `all green`.

```bash
git add .forgejo/ scripts/ docs/
git commit -m "Enable clippy::pedantic as an advisory step, at 0 sites

420 measured at df8cbd9, 420 resolved: 86 fixed where the design would have
switched the lint off, 79 mechanical, 37 written doc sections, 131 casts with
their bounds stated, 42 style, 45 under two module-level allows, one per
one-off experiment file.

Advisory rather than blocking because rust-toolchain.toml floats and clippy
adds to the group every six weeks or so. What stays enforced is the part that
matters: a stale #[expect] is unfulfilled_lint_expectations, which the
existing -D warnings gate already denies, so a cast deleted without its
attribute fails CI."
```

**PR 5 ends here. Open it stacked on PR 4.**

---

## Self-review

**Spec coverage.** §10.1 — deleted, with the refuting measurement recorded and both lints fixed instead (Tasks 2, 3). §10.2 mechanical — Tasks 4, 5, 6. §10.3 per-site with a stated bound, never module- or crate-level — Tasks 9–13, with the single exception argued in Task 15. §10.4 written prose, stamped templates refused — Tasks 7, 8. §10.5 advisory step in both `ci.yml` and `check-all.sh` — Task 16. §9's three `#[expect]` consequences — verified as probes before the plan and recorded in `docs/ci.md` by Task 1. §11's ordering — satisfied: Part 1 landed at `df8cbd9` and this stacks on it.

**Deviations from the spec, all deliberate and all recorded above:** §10.1's mechanism is refuted rather than implemented; §10.3's "never module-level" gains one argued exception (Task 15); §10's "one branch" becomes five stacked PRs. The first is a measurement, the second and third are the owner's calls made on 2026-08-10.

**Arithmetic.** 420 = 47 + 39 + 25 + 40 + 14 + 31 + 6 + 26 + 25 + 17 + 15 + 48 + 42 + 45. Running totals: 420 → 373 → 334 → 309 → 269 → 255 → 224 → 218 → 192 → 167 → 150 → 135 → 87 → 45 → 0.

**Known soft spots, named rather than hidden.** Tasks 9–13 cannot pre-state 131 bounds; they state the procedure, quote the five bounds the design already read, and fix the acceptance rule for a `reason`. That is the irreducible part of the work. And Task 16 Step 1 asks the implementer to confirm `continue-on-error` works on this Forgejo runner rather than asserting it does — no job in `ci.yml` currently uses it.

---

## Appendix — every site, by task

Generated from the `df8cbd9` measurement. Line and column are clippy's primary span. Paths are relative to `crates/`.

### T2 must_use — 47 sites

| file | line:col | lint |
| ---- | -------- | ---- |
| `taiyaku-core/src/lib.rs` | 45:12 | `must_use_candidate` |
| `taiyaku-core/src/lib.rs` | 50:12 | `must_use_candidate` |
| `taiyaku-core/src/lib.rs` | 54:12 | `must_use_candidate` |
| `taiyaku-core/src/lib.rs` | 58:12 | `must_use_candidate` |
| `taiyaku-core/src/lib.rs` | 76:12 | `must_use_candidate` |
| `taiyaku-core/src/lib.rs` | 82:12 | `must_use_candidate` |
| `taiyaku-core/src/lib.rs` | 96:8 | `must_use_candidate` |
| `taiyaku-core/src/lib.rs` | 108:8 | `must_use_candidate` |
| `taiyaku-core/src/lib.rs` | 145:8 | `must_use_candidate` |
| `taiyaku-core/src/regions.rs` | 94:12 | `must_use_candidate` |
| `taiyaku-core/src/regions.rs` | 113:12 | `must_use_candidate` |
| `taiyaku-core/src/regions.rs` | 129:12 | `must_use_candidate` |
| `taiyaku-core/src/regions.rs` | 144:12 | `must_use_candidate` |
| `taiyaku-core/src/route.rs` | 37:8 | `must_use_candidate` |
| `taiyaku-core/src/route.rs` | 74:12 | `must_use_candidate` |
| `taiyaku-core/src/route.rs` | 86:12 | `must_use_candidate` |
| `taiyaku-core/src/route.rs` | 95:12 | `must_use_candidate` |
| `taiyaku-core/src/route.rs` | 110:8 | `must_use_candidate` |
| `taiyaku-core/src/route.rs` | 184:8 | `must_use_candidate` |
| `taiyaku-core/src/sidecar.rs` | 174:12 | `must_use_candidate` |
| `taiyaku-core/src/sidecar.rs` | 185:12 | `must_use_candidate` |
| `taiyaku-core/src/sidecar.rs` | 201:12 | `must_use_candidate` |
| `taiyaku-core/src/sidecar.rs` | 391:12 | `must_use_candidate` |
| `taiyaku-core/src/sidecar.rs` | 1004:12 | `must_use_candidate` |
| `taiyaku-core/src/sidecar.rs` | 1021:12 | `must_use_candidate` |
| `taiyaku-core/src/sidecar.rs` | 1058:12 | `must_use_candidate` |
| `taiyaku-core/src/sidecar.rs` | 1088:12 | `must_use_candidate` |
| `taiyaku-core/src/slot.rs` | 67:8 | `must_use_candidate` |
| `taiyaku-core/src/slot.rs` | 118:8 | `must_use_candidate` |
| `taiyaku-core/src/solve.rs` | 14:8 | `must_use_candidate` |
| `taiyaku-core/src/solve.rs` | 75:12 | `must_use_candidate` |
| `taiyaku-core/src/solve.rs` | 80:12 | `must_use_candidate` |
| `taiyaku-core/src/solve.rs` | 94:8 | `must_use_candidate` |
| `taiyaku-core/src/solve.rs` | 196:8 | `must_use_candidate` |
| `taiyaku-core/src/solve.rs` | 326:8 | `must_use_candidate` |
| `taiyaku-core/src/solve.rs` | 347:8 | `must_use_candidate` |
| `taiyaku-core/src/solve.rs` | 407:8 | `must_use_candidate` |
| `taiyaku-core/src/solve.rs` | 444:8 | `must_use_candidate` |
| `taiyaku-read/src/naming.rs` | 49:8 | `must_use_candidate` |
| `taiyaku-read/src/pages.rs` | 34:8 | `must_use_candidate` |
| `taiyaku-serve/src/composite.rs` | 139:12 | `must_use_candidate` |
| `taiyaku-serve/src/lib.rs` | 58:12 | `must_use_candidate` |
| `taiyaku-serve/src/lib.rs` | 68:12 | `must_use_candidate` |
| `taiyaku-serve/src/notes.rs` | 222:8 | `must_use_candidate` |
| `taiyaku-serve/src/reqlog.rs` | 98:12 | `must_use_candidate` |
| `taiyaku-serve/src/works.rs` | 40:12 | `must_use_candidate` |
| `taiyaku-serve/src/works.rs` | 155:8 | `must_use_candidate` |

### T3 match_bool — 39 sites

| file | line:col | lint |
| ---- | -------- | ---- |
| `taiyaku-cli/src/corpus.rs` | 112:5 | `match_bool` |
| `taiyaku-cli/src/credits.rs` | 196:13 | `match_bool` |
| `taiyaku-cli/src/credits.rs` | 203:13 | `match_bool` |
| `taiyaku-cli/src/credits.rs` | 233:38 | `match_bool` |
| `taiyaku-cli/src/credits.rs` | 272:19 | `match_bool` |
| `taiyaku-cli/src/credits.rs` | 280:9 | `match_bool` |
| `taiyaku-cli/src/detect.rs` | 390:5 | `match_bool` |
| `taiyaku-cli/src/duplicates.rs` | 212:27 | `match_bool` |
| `taiyaku-cli/src/duplicates.rs` | 235:24 | `match_bool` |
| `taiyaku-cli/src/duplicates.rs` | 315:18 | `match_bool` |
| `taiyaku-cli/src/duplicates.rs` | 589:9 | `match_bool` |
| `taiyaku-cli/src/duplicates.rs` | 644:13 | `match_bool` |
| `taiyaku-cli/src/duplicates.rs` | 665:19 | `match_bool` |
| `taiyaku-cli/src/duplicates.rs` | 1333:22 | `match_bool` |
| `taiyaku-cli/src/duplicates.rs` | 1338:21 | `match_bool` |
| `taiyaku-cli/src/fingerprint.rs` | 125:5 | `match_bool` |
| `taiyaku-cli/src/fingerprint.rs` | 169:5 | `match_bool` |
| `taiyaku-cli/src/fingerprint.rs` | 188:9 | `match_bool` |
| `taiyaku-cli/src/inventory.rs` | 276:5 | `match_bool` |
| `taiyaku-cli/src/main.rs` | 242:5 | `match_bool` |
| `taiyaku-cli/src/main.rs` | 260:5 | `match_bool` |
| `taiyaku-cli/src/main.rs` | 328:5 | `match_bool` |
| `taiyaku-cli/src/main.rs` | 356:5 | `match_bool` |
| `taiyaku-cli/src/main.rs` | 760:9 | `match_bool` |
| `taiyaku-cli/src/main.rs` | 818:8 | `match_bool` |
| `taiyaku-cli/src/main.rs` | 1472:30 | `match_bool` |
| `taiyaku-cli/src/main.rs` | 1539:21 | `match_bool` |
| `taiyaku-cli/src/regions.rs` | 65:20 | `match_bool` |
| `taiyaku-cli/src/stats.rs` | 24:13 | `match_bool` |
| `taiyaku-core/src/credits.rs` | 80:21 | `match_bool` |
| `taiyaku-core/src/route.rs` | 137:27 | `match_bool` |
| `taiyaku-core/src/route.rs` | 173:10 | `match_bool` |
| `taiyaku-core/src/sidecar.rs` | 897:26 | `match_bool` |
| `taiyaku-core/src/sidecar.rs` | 947:17 | `match_bool` |
| `taiyaku-core/src/slot.rs` | 98:9 | `match_bool` |
| `taiyaku-core/src/solve.rs` | 130:22 | `match_bool` |
| `taiyaku-core/src/solve.rs` | 336:5 | `match_bool` |
| `taiyaku-read/src/pages.rs` | 106:5 | `match_bool` |
| `taiyaku-serve/src/works.rs` | 166:5 | `match_bool` |

### T4 cast_lossless — 25 sites

| file | line:col | lint |
| ---- | -------- | ---- |
| `taiyaku-cli/src/corpus.rs` | 245:21 | `cast_lossless` |
| `taiyaku-cli/src/corpus.rs` | 245:35 | `cast_lossless` |
| `taiyaku-cli/src/corpus.rs` | 250:17 | `cast_lossless` |
| `taiyaku-cli/src/corpus.rs` | 250:35 | `cast_lossless` |
| `taiyaku-cli/src/detect.rs` | 156:34 | `cast_lossless` |
| `taiyaku-cli/src/detect.rs` | 157:34 | `cast_lossless` |
| `taiyaku-cli/src/detect.rs` | 158:54 | `cast_lossless` |
| `taiyaku-cli/src/detect.rs` | 158:76 | `cast_lossless` |
| `taiyaku-cli/src/main.rs` | 1603:21 | `cast_lossless` |
| `taiyaku-cli/src/main.rs` | 1603:38 | `cast_lossless` |
| `taiyaku-cli/src/report.rs` | 56:20 | `cast_lossless` |
| `taiyaku-cli/src/report.rs` | 56:35 | `cast_lossless` |
| `taiyaku-cli/src/report.rs` | 56:59 | `cast_lossless` |
| `taiyaku-core/src/sidecar.rs` | 1239:41 | `cast_lossless` |
| `taiyaku-core/src/sidecar.rs` | 1240:39 | `cast_lossless` |
| `taiyaku-core/src/sidecar.rs` | 1261:22 | `cast_lossless` |
| `taiyaku-core/src/sidecar.rs` | 1261:55 | `cast_lossless` |
| `taiyaku-core/src/sidecar.rs` | 1281:22 | `cast_lossless` |
| `taiyaku-core/src/sidecar.rs` | 1281:55 | `cast_lossless` |
| `taiyaku-core/src/sidecar.rs` | 1285:26 | `cast_lossless` |
| `taiyaku-core/src/sidecar.rs` | 1286:26 | `cast_lossless` |
| `taiyaku-core/src/sidecar.rs` | 1287:24 | `cast_lossless` |
| `taiyaku-core/src/sidecar.rs` | 1287:48 | `cast_lossless` |
| `taiyaku-core/src/sidecar.rs` | 1288:38 | `cast_lossless` |
| `taiyaku-core/src/sidecar.rs` | 1288:69 | `cast_lossless` |

### T5 closures/map_unwrap_or — 40 sites

| file | line:col | lint |
| ---- | -------- | ---- |
| `taiyaku-cli/src/corpus.rs` | 61:15 | `map_unwrap_or` |
| `taiyaku-cli/src/credits.rs` | 512:65 | `redundant_closure_for_method_calls` |
| `taiyaku-cli/src/credits.rs` | 526:18 | `redundant_closure_for_method_calls` |
| `taiyaku-cli/src/credits.rs` | 538:65 | `redundant_closure_for_method_calls` |
| `taiyaku-cli/src/credits.rs` | 556:18 | `redundant_closure_for_method_calls` |
| `taiyaku-cli/src/credits.rs` | 567:54 | `redundant_closure_for_method_calls` |
| `taiyaku-cli/src/credits.rs` | 583:14 | `redundant_closure_for_method_calls` |
| `taiyaku-cli/src/credits.rs` | 798:25 | `redundant_closure_for_method_calls` |
| `taiyaku-cli/src/duplicates.rs` | 1502:18 | `redundant_closure_for_method_calls` |
| `taiyaku-cli/src/duplicates.rs` | 1513:53 | `redundant_closure_for_method_calls` |
| `taiyaku-cli/src/duplicates.rs` | 1526:18 | `redundant_closure_for_method_calls` |
| `taiyaku-cli/src/duplicates.rs` | 1543:18 | `redundant_closure_for_method_calls` |
| `taiyaku-cli/src/main.rs` | 453:29 | `map_unwrap_or` |
| `taiyaku-cli/src/main.rs` | 486:25 | `map_unwrap_or` |
| `taiyaku-cli/src/main.rs` | 2202:14 | `redundant_closure_for_method_calls` |
| `taiyaku-cli/src/main.rs` | 2415:25 | `redundant_closure_for_method_calls` |
| `taiyaku-cli/src/main.rs` | 2521:14 | `redundant_closure_for_method_calls` |
| `taiyaku-cli/src/main.rs` | 2544:14 | `redundant_closure_for_method_calls` |
| `taiyaku-cli/src/main.rs` | 2556:25 | `redundant_closure_for_method_calls` |
| `taiyaku-cli/src/regions.rs` | 246:25 | `redundant_closure_for_method_calls` |
| `taiyaku-cli/src/report.rs` | 100:23 | `map_unwrap_or` |
| `taiyaku-cli/src/report.rs` | 107:17 | `map_unwrap_or` |
| `taiyaku-core/src/regions.rs` | 130:20 | `map_unwrap_or` |
| `taiyaku-read/src/pages.rs` | 35:5 | `map_unwrap_or` |
| `taiyaku-serve/src/atomic.rs` | 202:25 | `redundant_closure_for_method_calls` |
| `taiyaku-serve/src/composite.rs` | 91:29 | `redundant_closure_for_method_calls` |
| `taiyaku-serve/src/composite.rs` | 157:29 | `redundant_closure_for_method_calls` |
| `taiyaku-serve/src/corpus.rs` | 60:9 | `map_unwrap_or` |
| `taiyaku-serve/src/cover.rs` | 253:66 | `redundant_closure_for_method_calls` |
| `taiyaku-serve/src/cover.rs` | 258:33 | `redundant_closure_for_method_calls` |
| `taiyaku-serve/src/cover.rs` | 276:29 | `redundant_closure_for_method_calls` |
| `taiyaku-serve/src/progress.rs` | 174:25 | `redundant_closure_for_method_calls` |
| `taiyaku-serve/src/reqlog.rs` | 215:57 | `redundant_closure_for_method_calls` |
| `taiyaku-serve/src/reqlog.rs` | 238:5 | `map_unwrap_or` |
| `taiyaku-serve/src/routes.rs` | 1049:32 | `map_unwrap_or` |
| `taiyaku-serve/src/routes.rs` | 1052:30 | `redundant_closure_for_method_calls` |
| `taiyaku-serve/src/routes.rs` | 3012:34 | `redundant_closure_for_method_calls` |
| `taiyaku-serve/src/stump/cache.rs` | 37:16 | `map_unwrap_or` |
| `taiyaku-serve/src/stump/http.rs` | 396:47 | `redundant_closure_for_method_calls` |
| `taiyaku-serve/src/works.rs` | 95:20 | `map_unwrap_or` |

### T6 singletons — 14 sites

| file | line:col | lint |
| ---- | -------- | ---- |
| `taiyaku-cli/src/detect.rs` | 229:22 | `manual_ilog2` |
| `taiyaku-cli/src/main.rs` | 914:5 | `manual_let_else` |
| `taiyaku-cli/src/main.rs` | 951:25 | `semicolon_if_nothing_returned` |
| `taiyaku-cli/src/main.rs` | 1128:9 | `single_match_else` |
| `taiyaku-core/src/sidecar.rs` | 618:25 | `nonminimal_bool` |
| `taiyaku-core/src/sidecar.rs` | 828:22 | `explicit_iter_loop` |
| `taiyaku-core/src/solve.rs` | 141:16 | `manual_midpoint` |
| `taiyaku-core/src/solve.rs` | 538:21 | `explicit_iter_loop` |
| `taiyaku-core/src/solve.rs` | 772:26 | `manual_midpoint` |
| `taiyaku-serve/src/cover.rs` | 490:9 | `manual_assert_eq` |
| `taiyaku-serve/src/lib.rs` | 75:9 | `doc_markdown` |
| `taiyaku-serve/src/reqlog.rs` | 289:20 | `unreadable_literal` |
| `taiyaku-serve/src/routes.rs` | 971:25 | `enum_glob_use` |
| `taiyaku-serve/src/stump/api.rs` | 74:17 | `needless_raw_string_hashes` |

### T7 missing_errors_doc — 31 sites

| file | line:col | lint |
| ---- | -------- | ---- |
| `taiyaku-core/src/credits.rs` | 48:1 | `missing_errors_doc` |
| `taiyaku-core/src/regions.rs` | 140:5 | `missing_errors_doc` |
| `taiyaku-core/src/sidecar.rs` | 24:5 | `missing_errors_doc` |
| `taiyaku-core/src/sidecar.rs` | 79:1 | `missing_errors_doc` |
| `taiyaku-core/src/sidecar.rs` | 387:5 | `missing_errors_doc` |
| `taiyaku-core/src/sidecar.rs` | 426:5 | `missing_errors_doc` |
| `taiyaku-core/src/sidecar.rs` | 670:5 | `missing_errors_doc` |
| `taiyaku-core/src/sidecar.rs` | 926:5 | `missing_errors_doc` |
| `taiyaku-read/src/pages.rs` | 49:1 | `missing_errors_doc` |
| `taiyaku-read/src/pages.rs` | 61:1 | `missing_errors_doc` |
| `taiyaku-read/src/pages.rs` | 276:1 | `missing_errors_doc` |
| `taiyaku-read/src/pages.rs` | 297:1 | `missing_errors_doc` |
| `taiyaku-read/src/pages.rs` | 335:1 | `missing_errors_doc` |
| `taiyaku-read/src/pages.rs` | 353:1 | `missing_errors_doc` |
| `taiyaku-read/src/pages.rs` | 377:1 | `missing_errors_doc` |
| `taiyaku-serve/src/atomic.rs` | 76:1 | `missing_errors_doc` |
| `taiyaku-serve/src/lib.rs` | 82:5 | `missing_errors_doc` |
| `taiyaku-serve/src/lib.rs` | 93:5 | `missing_errors_doc` |
| `taiyaku-serve/src/lib.rs` | 145:1 | `missing_errors_doc` |
| `taiyaku-serve/src/notes.rs` | 120:1 | `missing_errors_doc` |
| `taiyaku-serve/src/notes.rs` | 235:1 | `missing_errors_doc` |
| `taiyaku-serve/src/notes.rs` | 240:1 | `missing_errors_doc` |
| `taiyaku-serve/src/notes.rs` | 257:1 | `missing_errors_doc` |
| `taiyaku-serve/src/reqlog.rs` | 163:5 | `missing_errors_doc` |
| `taiyaku-serve/src/source.rs` | 45:5 | `missing_errors_doc` |
| `taiyaku-serve/src/works.rs` | 134:5 | `missing_errors_doc` |
| `taiyaku-serve/src/works.rs` | 176:1 | `missing_errors_doc` |
| `taiyaku-wasm/src/binding.rs` | 82:5 | `missing_errors_doc` |
| `taiyaku-wasm/src/lib.rs` | 62:5 | `missing_errors_doc` |
| `taiyaku-wasm/src/lib.rs` | 83:5 | `missing_errors_doc` |
| `taiyaku-wasm/src/lib.rs` | 109:5 | `missing_errors_doc` |

### T8 missing_panics_doc — 6 sites

| file | line:col | lint |
| ---- | -------- | ---- |
| `taiyaku-core/src/credits.rs` | 48:1 | `missing_panics_doc` |
| `taiyaku-core/src/regions.rs` | 144:5 | `missing_panics_doc` |
| `taiyaku-core/src/sidecar.rs` | 391:5 | `missing_panics_doc` |
| `taiyaku-core/src/sidecar.rs` | 426:5 | `missing_panics_doc` |
| `taiyaku-core/src/sidecar.rs` | 670:5 | `missing_panics_doc` |
| `taiyaku-serve/src/reqlog.rs` | 98:5 | `missing_panics_doc` |

### T9 casts detect.rs — 26 sites

| file | line:col | lint |
| ---- | -------- | ---- |
| `taiyaku-cli/src/detect.rs` | 104:26 | `cast_precision_loss` |
| `taiyaku-cli/src/detect.rs` | 104:50 | `cast_precision_loss` |
| `taiyaku-cli/src/detect.rs` | 105:26 | `cast_precision_loss` |
| `taiyaku-cli/src/detect.rs` | 105:63 | `cast_precision_loss` |
| `taiyaku-cli/src/detect.rs` | 106:24 | `cast_precision_loss` |
| `taiyaku-cli/src/detect.rs` | 106:39 | `cast_precision_loss` |
| `taiyaku-cli/src/detect.rs` | 109:26 | `cast_possible_truncation` |
| `taiyaku-cli/src/detect.rs` | 161:41 | `cast_possible_truncation` |
| `taiyaku-cli/src/detect.rs` | 161:41 | `cast_sign_loss` |
| `taiyaku-cli/src/detect.rs` | 161:52 | `cast_possible_truncation` |
| `taiyaku-cli/src/detect.rs` | 161:52 | `cast_sign_loss` |
| `taiyaku-cli/src/detect.rs` | 252:26 | `cast_precision_loss` |
| `taiyaku-cli/src/detect.rs` | 252:50 | `cast_precision_loss` |
| `taiyaku-cli/src/detect.rs` | 253:26 | `cast_precision_loss` |
| `taiyaku-cli/src/detect.rs` | 253:63 | `cast_precision_loss` |
| `taiyaku-cli/src/detect.rs` | 254:24 | `cast_precision_loss` |
| `taiyaku-cli/src/detect.rs` | 254:39 | `cast_precision_loss` |
| `taiyaku-cli/src/detect.rs` | 501:27 | `cast_precision_loss` |
| `taiyaku-cli/src/detect.rs` | 504:16 | `cast_precision_loss` |
| `taiyaku-cli/src/detect.rs` | 504:34 | `cast_precision_loss` |
| `taiyaku-cli/src/detect.rs` | 505:16 | `cast_precision_loss` |
| `taiyaku-cli/src/detect.rs` | 505:34 | `cast_precision_loss` |
| `taiyaku-cli/src/detect.rs` | 506:16 | `cast_precision_loss` |
| `taiyaku-cli/src/detect.rs` | 506:39 | `cast_precision_loss` |
| `taiyaku-cli/src/detect.rs` | 507:16 | `cast_precision_loss` |
| `taiyaku-cli/src/detect.rs` | 507:40 | `cast_precision_loss` |

### T10 casts sidecar.rs — 25 sites

| file | line:col | lint |
| ---- | -------- | ---- |
| `taiyaku-core/src/sidecar.rs` | 697:23 | `cast_possible_truncation` |
| `taiyaku-core/src/sidecar.rs` | 701:29 | `cast_possible_truncation` |
| `taiyaku-core/src/sidecar.rs` | 719:39 | `cast_possible_truncation` |
| `taiyaku-core/src/sidecar.rs` | 789:37 | `cast_possible_truncation` |
| `taiyaku-core/src/sidecar.rs` | 802:25 | `cast_possible_truncation` |
| `taiyaku-core/src/sidecar.rs` | 805:17 | `cast_possible_truncation` |
| `taiyaku-core/src/sidecar.rs` | 821:29 | `cast_possible_truncation` |
| `taiyaku-core/src/sidecar.rs` | 824:21 | `cast_possible_truncation` |
| `taiyaku-core/src/sidecar.rs` | 829:25 | `cast_possible_truncation` |
| `taiyaku-core/src/sidecar.rs` | 832:17 | `cast_possible_truncation` |
| `taiyaku-core/src/sidecar.rs` | 906:20 | `cast_possible_truncation` |
| `taiyaku-core/src/sidecar.rs` | 945:45 | `cast_possible_truncation` |
| `taiyaku-core/src/sidecar.rs` | 1030:20 | `cast_possible_truncation` |
| `taiyaku-core/src/sidecar.rs` | 1200:9 | `cast_possible_truncation` |
| `taiyaku-core/src/sidecar.rs` | 1239:40 | `cast_possible_truncation` |
| `taiyaku-core/src/sidecar.rs` | 1239:40 | `cast_sign_loss` |
| `taiyaku-core/src/sidecar.rs` | 1240:38 | `cast_possible_truncation` |
| `taiyaku-core/src/sidecar.rs` | 1240:38 | `cast_sign_loss` |
| `taiyaku-core/src/sidecar.rs` | 1288:31 | `cast_possible_truncation` |
| `taiyaku-core/src/sidecar.rs` | 1288:31 | `cast_sign_loss` |
| `taiyaku-core/src/sidecar.rs` | 1288:62 | `cast_possible_truncation` |
| `taiyaku-core/src/sidecar.rs` | 1288:62 | `cast_sign_loss` |
| `taiyaku-core/src/sidecar.rs` | 1524:27 | `cast_possible_truncation` |
| `taiyaku-core/src/sidecar.rs` | 1525:25 | `cast_possible_truncation` |
| `taiyaku-core/src/sidecar.rs` | 1526:27 | `cast_possible_truncation` |

### T11 casts solve.rs — 17 sites

| file | line:col | lint |
| ---- | -------- | ---- |
| `taiyaku-core/src/solve.rs` | 20:11 | `cast_precision_loss` |
| `taiyaku-core/src/solve.rs` | 175:14 | `cast_possible_truncation` |
| `taiyaku-core/src/solve.rs` | 175:14 | `cast_sign_loss` |
| `taiyaku-core/src/solve.rs` | 175:16 | `cast_precision_loss` |
| `taiyaku-core/src/solve.rs` | 210:28 | `cast_precision_loss` |
| `taiyaku-core/src/solve.rs` | 214:28 | `cast_precision_loss` |
| `taiyaku-core/src/solve.rs` | 243:29 | `cast_possible_truncation` |
| `taiyaku-core/src/solve.rs` | 243:48 | `cast_possible_truncation` |
| `taiyaku-core/src/solve.rs` | 248:29 | `cast_possible_truncation` |
| `taiyaku-core/src/solve.rs` | 260:33 | `cast_possible_truncation` |
| `taiyaku-core/src/solve.rs` | 336:58 | `cast_precision_loss` |
| `taiyaku-core/src/solve.rs` | 419:14 | `cast_precision_loss` |
| `taiyaku-core/src/solve.rs` | 419:29 | `cast_precision_loss` |
| `taiyaku-core/src/solve.rs` | 445:9 | `cast_possible_truncation` |
| `taiyaku-core/src/solve.rs` | 453:32 | `cast_possible_truncation` |
| `taiyaku-core/src/solve.rs` | 467:39 | `cast_possible_truncation` |
| `taiyaku-core/src/solve.rs` | 870:25 | `cast_precision_loss` |

### T12 casts main.rs — 15 sites

| file | line:col | lint |
| ---- | -------- | ---- |
| `taiyaku-cli/src/main.rs` | 523:9 | `cast_precision_loss` |
| `taiyaku-cli/src/main.rs` | 624:69 | `cast_possible_truncation` |
| `taiyaku-cli/src/main.rs` | 638:41 | `cast_precision_loss` |
| `taiyaku-cli/src/main.rs` | 639:45 | `cast_precision_loss` |
| `taiyaku-cli/src/main.rs` | 640:44 | `cast_precision_loss` |
| `taiyaku-cli/src/main.rs` | 662:42 | `cast_precision_loss` |
| `taiyaku-cli/src/main.rs` | 663:46 | `cast_precision_loss` |
| `taiyaku-cli/src/main.rs` | 664:45 | `cast_precision_loss` |
| `taiyaku-cli/src/main.rs` | 1332:41 | `cast_possible_truncation` |
| `taiyaku-cli/src/main.rs` | 1438:24 | `cast_possible_truncation` |
| `taiyaku-cli/src/main.rs` | 1622:31 | `cast_precision_loss` |
| `taiyaku-cli/src/main.rs` | 1622:51 | `cast_precision_loss` |
| `taiyaku-cli/src/main.rs` | 1629:37 | `cast_precision_loss` |
| `taiyaku-cli/src/main.rs` | 2112:44 | `cast_possible_truncation` |
| `taiyaku-cli/src/main.rs` | 3467:30 | `cast_possible_truncation` |

### T13 casts remainder — 48 sites

| file | line:col | lint |
| ---- | -------- | ---- |
| `taiyaku-cli/src/corpus.rs` | 222:53 | `cast_possible_truncation` |
| `taiyaku-cli/src/credits.rs` | 59:28 | `cast_possible_truncation` |
| `taiyaku-cli/src/credits.rs` | 72:23 | `cast_possible_truncation` |
| `taiyaku-cli/src/credits.rs` | 334:12 | `cast_possible_truncation` |
| `taiyaku-cli/src/duplicates.rs` | 237:39 | `cast_possible_truncation` |
| `taiyaku-cli/src/duplicates.rs` | 238:39 | `cast_possible_truncation` |
| `taiyaku-cli/src/fingerprint.rs` | 326:16 | `cast_possible_truncation` |
| `taiyaku-cli/src/fingerprint.rs` | 326:31 | `cast_possible_truncation` |
| `taiyaku-cli/src/regions.rs` | 67:18 | `cast_precision_loss` |
| `taiyaku-cli/src/regions.rs` | 67:34 | `cast_precision_loss` |
| `taiyaku-cli/src/regions.rs` | 202:34 | `cast_possible_truncation` |
| `taiyaku-cli/src/report.rs` | 56:18 | `cast_possible_truncation` |
| `taiyaku-cli/src/report.rs` | 84:31 | `cast_precision_loss` |
| `taiyaku-cli/src/report.rs` | 84:50 | `cast_precision_loss` |
| `taiyaku-cli/src/report.rs` | 117:22 | `cast_possible_truncation` |
| `taiyaku-cli/src/signature.rs` | 57:20 | `cast_possible_truncation` |
| `taiyaku-cli/src/stats.rs` | 40:31 | `cast_precision_loss` |
| `taiyaku-cli/src/stats.rs` | 40:51 | `cast_precision_loss` |
| `taiyaku-core/src/credits.rs` | 101:53 | `cast_possible_truncation` |
| `taiyaku-core/src/credits.rs` | 108:9 | `cast_possible_truncation` |
| `taiyaku-core/src/credits.rs` | 113:43 | `cast_possible_truncation` |
| `taiyaku-core/src/route.rs` | 75:20 | `cast_possible_truncation` |
| `taiyaku-core/src/route.rs` | 123:42 | `cast_possible_truncation` |
| `taiyaku-core/src/route.rs` | 124:42 | `cast_possible_truncation` |
| `taiyaku-core/src/slot.rs` | 84:39 | `cast_precision_loss` |
| `taiyaku-core/src/slot.rs` | 84:50 | `cast_precision_loss` |
| `taiyaku-core/src/slot.rs` | 127:24 | `cast_possible_truncation` |
| `taiyaku-read/src/pages.rs` | 262:48 | `cast_possible_truncation` |
| `taiyaku-read/src/pages.rs` | 323:40 | `cast_possible_truncation` |
| `taiyaku-read/src/pages.rs` | 363:8 | `cast_possible_truncation` |
| `taiyaku-serve/src/corpus.rs` | 105:34 | `cast_possible_truncation` |
| `taiyaku-serve/src/corpus.rs` | 279:21 | `cast_possible_truncation` |
| `taiyaku-serve/src/cover.rs` | 430:25 | `cast_possible_truncation` |
| `taiyaku-serve/src/cover.rs` | 430:41 | `cast_possible_truncation` |
| `taiyaku-serve/src/cover.rs` | 446:25 | `cast_possible_truncation` |
| `taiyaku-serve/src/cover.rs` | 446:42 | `cast_possible_truncation` |
| `taiyaku-serve/src/lib.rs` | 461:25 | `cast_possible_truncation` |
| `taiyaku-serve/src/lib.rs` | 461:41 | `cast_possible_truncation` |
| `taiyaku-serve/src/notes.rs` | 158:25 | `cast_possible_truncation` |
| `taiyaku-serve/src/opds.rs` | 1109:25 | `cast_possible_truncation` |
| `taiyaku-serve/src/opds.rs` | 1109:41 | `cast_possible_truncation` |
| `taiyaku-serve/src/reqlog.rs` | 209:21 | `cast_possible_truncation` |
| `taiyaku-serve/src/reqlog.rs` | 240:18 | `cast_possible_truncation` |
| `taiyaku-serve/src/routes.rs` | 764:21 | `cast_possible_truncation` |
| `taiyaku-wasm/src/binding.rs` | 138:14 | `cast_possible_truncation` |
| `taiyaku-wasm/src/binding.rs` | 174:26 | `cast_possible_truncation` |
| `taiyaku-wasm/src/lib.rs` | 131:21 | `cast_possible_truncation` |
| `taiyaku-wasm/src/lib.rs` | 132:17 | `cast_possible_truncation` |

### T14 style — 42 sites

| file | line:col | lint |
| ---- | -------- | ---- |
| `taiyaku-cli/src/corpus.rs` | 126:1 | `too_many_lines` |
| `taiyaku-cli/src/credits.rs` | 627:9 | `items_after_statements` |
| `taiyaku-cli/src/detect.rs` | 226:5 | `items_after_statements` |
| `taiyaku-cli/src/duplicates.rs` | 403:5 | `items_after_statements` |
| `taiyaku-cli/src/main.rs` | 374:1 | `too_many_lines` |
| `taiyaku-cli/src/main.rs` | 1146:1 | `too_many_lines` |
| `taiyaku-cli/src/main.rs` | 1536:21 | `trivially_copy_pass_by_ref` |
| `taiyaku-cli/src/main.rs` | 1596:17 | `format_push_string` |
| `taiyaku-cli/src/main.rs` | 1606:21 | `format_push_string` |
| `taiyaku-cli/src/report.rs` | 88:9 | `format_push_string` |
| `taiyaku-cli/src/report.rs` | 139:9 | `format_push_string` |
| `taiyaku-cli/src/signature.rs` | 101:5 | `format_collect` |
| `taiyaku-core/src/sidecar.rs` | 426:5 | `too_many_lines` |
| `taiyaku-core/src/sidecar.rs` | 773:15 | `needless_pass_by_value` |
| `taiyaku-core/src/sidecar.rs` | 775:15 | `needless_pass_by_value` |
| `taiyaku-core/src/sidecar.rs` | 1321:5 | `ref_option` |
| `taiyaku-core/src/sidecar.rs` | 2458:5 | `too_many_lines` |
| `taiyaku-core/src/solve.rs` | 196:14 | `many_single_char_names` |
| `taiyaku-core/src/solve.rs` | 196:30 | `many_single_char_names` |
| `taiyaku-core/src/solve.rs` | 204:10 | `many_single_char_names` |
| `taiyaku-core/src/solve.rs` | 204:13 | `many_single_char_names` |
| `taiyaku-core/src/solve.rs` | 239:14 | `many_single_char_names` |
| `taiyaku-core/src/solve.rs` | 239:21 | `many_single_char_names` |
| `taiyaku-read/src/lib.rs` | 63:9 | `items_after_statements` |
| `taiyaku-read/src/lib.rs` | 64:9 | `items_after_statements` |
| `taiyaku-read/src/lib.rs` | 72:9 | `items_after_statements` |
| `taiyaku-read/src/pages.rs` | 345:8 | `format_collect` |
| `taiyaku-serve/src/cover.rs` | 392:11 | `needless_pass_by_value` |
| `taiyaku-serve/src/opds.rs` | 133:9 | `format_push_string` |
| `taiyaku-serve/src/opds.rs` | 201:9 | `format_push_string` |
| `taiyaku-serve/src/opds.rs` | 932:13 | `similar_names` |
| `taiyaku-serve/src/reqlog.rs` | 116:5 | `struct_field_names` |
| `taiyaku-serve/src/reqlog.rs` | 163:22 | `needless_pass_by_value` |
| `taiyaku-serve/src/reqlog.rs` | 203:35 | `needless_pass_by_value` |
| `taiyaku-serve/src/routes.rs` | 224:23 | `format_collect` |
| `taiyaku-serve/src/routes.rs` | 239:23 | `format_collect` |
| `taiyaku-serve/src/routes.rs` | 843:8 | `option_option` |
| `taiyaku-serve/src/routes.rs` | 2421:13 | `similar_names` |
| `taiyaku-serve/src/routes.rs` | 2863:14 | `similar_names` |
| `taiyaku-serve/src/stump/resolve.rs` | 27:5 | `format_collect` |
| `taiyaku-wasm/src/lib.rs` | 88:15 | `needless_pass_by_value` |
| `taiyaku-wasm/src/lib.rs` | 113:15 | `needless_pass_by_value` |

### T15 experiment files — 45 sites

| file | line:col | lint |
| ---- | -------- | ---- |
| `taiyaku-cli/src/geometry_experiment.rs` | 330:17 | `cast_possible_truncation` |
| `taiyaku-cli/src/geometry_experiment.rs` | 330:17 | `cast_sign_loss` |
| `taiyaku-cli/src/geometry_experiment.rs` | 330:19 | `cast_lossless` |
| `taiyaku-cli/src/geometry_experiment.rs` | 330:30 | `cast_lossless` |
| `taiyaku-cli/src/geometry_experiment.rs` | 330:44 | `cast_lossless` |
| `taiyaku-cli/src/geometry_experiment.rs` | 355:17 | `cast_possible_truncation` |
| `taiyaku-cli/src/geometry_experiment.rs` | 355:17 | `cast_sign_loss` |
| `taiyaku-cli/src/geometry_experiment.rs` | 374:21 | `cast_possible_truncation` |
| `taiyaku-cli/src/geometry_experiment.rs` | 374:21 | `cast_sign_loss` |
| `taiyaku-cli/src/geometry_experiment.rs` | 374:32 | `cast_possible_truncation` |
| `taiyaku-cli/src/geometry_experiment.rs` | 374:32 | `cast_sign_loss` |
| `taiyaku-cli/src/geometry_experiment.rs` | 375:29 | `cast_possible_truncation` |
| `taiyaku-cli/src/geometry_experiment.rs` | 375:29 | `cast_sign_loss` |
| `taiyaku-cli/src/geometry_experiment.rs` | 375:47 | `cast_possible_truncation` |
| `taiyaku-cli/src/geometry_experiment.rs` | 375:47 | `cast_sign_loss` |
| `taiyaku-cli/src/geometry_experiment.rs` | 385:51 | `cast_precision_loss` |
| `taiyaku-cli/src/geometry_experiment.rs` | 385:62 | `cast_precision_loss` |
| `taiyaku-cli/src/geometry_experiment.rs` | 423:20 | `similar_names` |
| `taiyaku-cli/src/geometry_experiment.rs` | 423:32 | `similar_names` |
| `taiyaku-cli/src/geometry_experiment.rs` | 458:17 | `map_unwrap_or` |
| `taiyaku-cli/src/geometry_experiment.rs` | 483:45 | `cast_precision_loss` |
| `taiyaku-cli/src/geometry_experiment.rs` | 526:55 | `cast_possible_truncation` |
| `taiyaku-cli/src/geometry_experiment.rs` | 595:25 | `cast_possible_truncation` |
| `taiyaku-cli/src/geometry_experiment.rs` | 595:25 | `cast_sign_loss` |
| `taiyaku-cli/src/geometry_experiment.rs` | 595:27 | `cast_precision_loss` |
| `taiyaku-cli/src/geometry_experiment.rs` | 678:1 | `too_many_lines` |
| `taiyaku-cli/src/geometry_experiment.rs` | 746:13 | `map_unwrap_or` |
| `taiyaku-cli/src/geometry_experiment.rs` | 834:1 | `too_many_lines` |
| `taiyaku-cli/src/geometry_experiment.rs` | 874:5 | `items_after_statements` |
| `taiyaku-cli/src/region_experiment.rs` | 349:1 | `too_many_lines` |
| `taiyaku-cli/src/region_experiment.rs` | 362:14 | `cast_precision_loss` |
| `taiyaku-cli/src/region_experiment.rs` | 362:28 | `cast_precision_loss` |
| `taiyaku-cli/src/region_experiment.rs` | 428:27 | `cast_precision_loss` |
| `taiyaku-cli/src/region_experiment.rs` | 449:26 | `cast_precision_loss` |
| `taiyaku-cli/src/region_experiment.rs` | 449:40 | `cast_precision_loss` |
| `taiyaku-cli/src/region_experiment.rs` | 454:26 | `cast_precision_loss` |
| `taiyaku-cli/src/region_experiment.rs` | 610:26 | `cast_precision_loss` |
| `taiyaku-cli/src/region_experiment.rs` | 610:42 | `cast_precision_loss` |
| `taiyaku-cli/src/region_experiment.rs` | 620:1 | `too_many_lines` |
| `taiyaku-cli/src/region_experiment.rs` | 750:17 | `cast_possible_truncation` |
| `taiyaku-cli/src/region_experiment.rs` | 750:17 | `cast_sign_loss` |
| `taiyaku-cli/src/region_experiment.rs` | 751:17 | `cast_possible_truncation` |
| `taiyaku-cli/src/region_experiment.rs` | 751:17 | `cast_sign_loss` |
| `taiyaku-cli/src/region_experiment.rs` | 765:54 | `cast_precision_loss` |
| `taiyaku-cli/src/region_experiment.rs` | 936:53 | `cast_precision_loss` |