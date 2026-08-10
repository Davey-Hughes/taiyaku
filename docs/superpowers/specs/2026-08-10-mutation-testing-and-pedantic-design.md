# Mutation testing, and the pedantic lints — design

**Status: settled in brainstorming on 2026-08-10. Nothing built.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

Two branches, stacked, and they share a premise rather than a mechanism. The
coverage gate that shipped in PR #37 measures *execution*. Neither of these
measures execution. Mutation testing asks whether a line that ran was
**verified**; the pedantic cast lints ask whether an arithmetic conversion that
compiles is **bounded**. Both are asking what the existing gates cannot see.

## Where it came from

Asked directly: look at mutation testing, and say whether turning on clippy's
pedantic warnings is worth it — the casting ones in particular, since casts are
error-prone. Two questions. Both were answered by measuring rather than
arguing, and the measurement moved the answer on each.

---

# Part 1 — Mutation testing

## 1. What the estimates got wrong

The plan recorded after PR #37 guessed "maybe 1,000+ mutants at 10–30s each
(hours)" for the Rust half. Measured on this machine with
`cargo-mutants 27.1.0`:

| figure | estimated | measured |
| ------ | --------- | -------- |
| workspace mutants | 1,000+ | **1,660** |
| per-mutant cost | 10–30 s | **≈1.6 s** averaged over the workspace |
| full baseline | hours | **43 minutes** |

The per-mutant estimate was an order of magnitude too high for one reason, and
it is the reason mutation testing is affordable here at all: **the whole
workspace suite is 526 tests in 1.9 s** under
`cargo nextest run --workspace --all-features`. Almost all of a mutant's cost is
the incremental rebuild, not the test run — which is also why the figure is
2× worse than `solve.rs`'s 0.8 s in isolation: `taiyaku-cli`'s 752 mutants
rebuild a far larger crate than `taiyaku-core`'s do.

Forty-three minutes is a coffee break, not a nightly job. That is what lets §5
defer the gate without deferring the measurement.

## 1a. The baseline

`env -u TAIYAKU_CORPUS cargo mutants --gitignore=true --test-tool=nextest`, on
this machine, against the CI-visible suite:

**1,660 mutants in 43 min — 1,149 caught, 346 missed, 136 unviable, 29 timed
out.** Of 1,524 viable mutants, **75.4% were caught**.

| crate | mutants | caught | missed |
| ----- | ------- | ------ | ------ |
| `taiyaku-cli` | 752 | 478 | **217** |
| `taiyaku-core` | 460 | 370 | **47** |
| `taiyaku-serve` | 304 | 217 | **52** |
| `taiyaku-read` | 93 | 56 | **17** |
| `taiyaku-wasm` | 51 | 28 | **13** |

`taiyaku-core`'s 47 are this branch's whole scope, and they are concentrated:

| file | survivors |
| ---- | --------- |
| `solve.rs` | 26 |
| `sidecar.rs` | 11 |
| `credits.rs` | 7 |
| `slot.rs` | 2 |
| `lib.rs` | 1 |

**Forty-seven is the figure that makes this branch tractable, and it is worth
stating against the number it replaces.** "Triage `taiyaku-core`'s 460 mutants"
sounds like 460 units of work; 370 of them are already caught and need nothing.
The work is 47 survivors in five files, and over half sit in one function.

Per crate: `taiyaku-cli` 752, `taiyaku-core` 460, `taiyaku-serve` 304,
`taiyaku-read` 93, `taiyaku-wasm` 51.

The per-mutant estimate was 30× too high for one reason, and it is the reason
mutation testing is affordable here at all: **the whole workspace suite is 526
tests in 1.9 s** under `cargo nextest run --workspace --all-features`. Nearly
all of a mutant's cost is the incremental rebuild, not the test run.

## 2. `cargo-mutants` does not work on this repository unmodified

The first run died:

```
Error: Failed to copy .../corpus/incoming/needles/colorwalk-comp2/cwc2-310.tif
Caused by: No space left on device (os error 28)
```

`cargo-mutants` copies the source tree to a scratch directory and **does not
honour `.gitignore` by default**. `corpus/` is gitignored (line 8) and is 26 GB
of NAS-backed material, including the archival TIFFs at 430 MB a page. The copy
filled a 31 GB `/tmp` and aborted before testing a single mutant.

`--gitignore=true` fixes it. That flag is not optional here and the branch
documents *why*, because the failure is loud but its cause is not: nothing in
the error names the corpus as gitignored, and the obvious reading is that the
machine is short of disk.

## 3. The finding: the solver's arithmetic is verified by one corpus check

`solve.rs` alone, 144 mutants in 2 minutes: **111 of 138 viable mutants caught,
26 missed, 1 timed out**, with 6 unviable. Every survivor is in the aligner's
dynamic-programming recurrence:

| site | mutation | outcome |
| ---- | -------- | ------- |
| `solve.rs:183` | `i * (n + 1) + j` → `(n - 1)` | survives |
| `solve.rs:188` | delete the `-` in `-(i as f32) * GAP_PENALTY` | survives |
| `solve.rs:198` | `+ similarity(…) - floor` → `+ floor` | survives |
| `solve.rs:199` | `score[…] - GAP_PENALTY` → `+` | survives |
| `solve.rs:204` | `pair >= skip_a && pair >= skip_b` → `\|\|` | survives |

Plus `calibrate` (`>` → `>=` at 106 and 109), `percentile` (153, 154), and
`confidence` (445).

Deleting the minus at 188 turns the gap *penalty* into a gap *bonus* for the
first column. Line 199 does the same inside the recurrence itself. Line 204
replaces "pairing wins only if it beats both alternatives" with "either will
do." **Not one of the 526 tests fails.**

**Then the obvious next question was asked rather than assumed.** The 526 are
what CI runs; they exclude the twelve corpus-gated standing checks. The
line-188 mutant was applied by hand and the corpus checks run under `--release`:

```
11 passed; 1 failed  (15.6 s)
failures:
    corpus::composing_through_the_hub_agrees_with_a_direct_solve
```

So the arithmetic **is** verified — by exactly one standing check, running
against 26 GB of material on a NAS that CI cannot reach. Eleven of the twelve
corpus checks are indifferent to it, and every test CI can run is indifferent
to it.

**That is what shapes this branch, and it inverts the plan recorded after
PR #37.** That plan named `--in-diff` on PRs as the deliverable. An `--in-diff`
gate running the CI-visible suite would report `solve.rs`'s recurrence as
unverified — twenty-six false alarms in the file that matters most — while
staying silent about the one check that would actually catch a regression.
Building the gate first would have shipped an instrument calibrated against the
wrong baseline.

## 4. What this branch builds

**The survivors are the deliverable, not the tooling.**

1. **Full baseline.** `env -u TAIYAKU_CORPUS cargo mutants --gitignore=true
   --test-tool=nextest`. Record actual wall time and per-crate survivor counts
   in `docs/notes.md`.
2. **Triage `taiyaku-core`'s 460 mutants.** Separate genuine gaps from
   equivalent mutants; the equivalent ones are recorded with the reason they are
   equivalent, not silently dropped.
3. **Write tests that kill the genuine survivors.** CI-runnable, which is the
   binding constraint: the suite is 526 tests in 1.9 s and the coverage job runs
   it, so fixtures stay small. The point is to move the solver's verification
   from a NAS-backed corpus check into tests the forge can run.
4. **Record the rest.** `taiyaku-cli` 752, `taiyaku-serve` 304, `taiyaku-read`
   93, `taiyaku-wasm` 51 — counts and survivor lists in `notes.md`, named as
   measured-but-not-addressed. The precedent is L2a's detector: a measured
   failure written down beats an unmeasured intention.
5. **Document the invocation** in `docs/ci.md`, including `--gitignore=true` and
   why, and the `--output` flag that keeps `mutants.out/` out of the working
   tree.

## 5. What this branch deliberately does not build

**No `--in-diff` PR gate, and no threshold.** Both were in the plan this design
replaces. The numbers needed to size them do not exist until step 1 finishes,
and a threshold picked before the baseline is a number chosen to be passed
rather than a number that means something. This is the discipline the coverage
branch used: `--fail-under-lines 93` was 95.75% measured, less two points.

**No StrykerJS.** `web/` runs Vitest 4.1.10; `@stryker-mutator/vitest-runner`'s
support for Vitest 4 is **unverified** — a guess, stated as one. It gets
measured before it enters a plan, not assumed into one. The Rust half is where
the finding is.

## 6. What mutation testing still will not catch

Recorded up front so the branch does not oversell itself:

1. **Equivalent mutants** are semantically identical changes that survive
   legitimately. They are false alarms and need triage — which is why step 2 is
   triage rather than a count.
2. **It only mutates code that exists.** All three real defects PR #37 found — a
   412 discarding the reader's notes, a missing `ETag` killing every later
   write, timers outliving their component — were found by *reading*, and every
   mutant would have died correctly.
3. **It cannot tell you the spec is right.** `keyplan.ts` faithfully preserved a
   doc comment describing behaviour no key could reach.

Coverage says what is unprotected, mutation says what is unverified, neither
says what is unwritten.

---

# Part 2 — The pedantic lints

## 7. Measured first, and the measurement said no

`cargo clippy --workspace --all-targets --all-features -- -W clippy::pedantic`,
deduplicated to unique sites under `crates/`: **422 warnings across 33 lints.**

| lint | sites |
| ---- | ----- |
| `cast_possible_truncation` | 90 |
| `cast_precision_loss` | 61 |
| `must_use_candidate` | 46 |
| `match_bool` | 38 |
| `cast_lossless` | 35 |
| `missing_errors_doc` | 31 |
| `redundant_closure_for_method_calls` | 28 |
| `cast_sign_loss` | 16 |

The cast family is 186 of the 422 — `taiyaku-cli` 118, `taiyaku-core` 63,
`taiyaku-serve` 14, `taiyaku-wasm` 4, `taiyaku-read` 3.

**Ten of the highest-risk cast sites were read against the surrounding code.
All ten are bounded by construction. Zero live defects.** The five worth
naming, because each is the kind that looks dangerous and is not:

- `sidecar.rs:1232` — `b_slots[(a_from as i64 + offset) as usize]`. A negative
  sum would wrap to a huge index and panic. `a_from >= s_start` by
  `split_around`'s contract and `offset = index(b_slots, …) - s_start`, so the
  sum is non-negative. **Safe by construction, not by type** — which is exactly
  the case an `#[expect]` with a stated reason documents and a silent `as` does
  not.
- `sidecar.rs:1281` — `cs.max(a_start as i64) as u32`. The `.max`/`.min` clamp to
  u32-valued bounds and the guard on the line above forces non-negative.
- `solve.rs:153` — `(values.len() as f32 - 1.0) * fraction`. f32 loses integer
  precision above 2²⁴; that is 16.7 million pages.
- `detect.rs:161` — flood-fill `nx as u32`, guarded by the explicit
  `nx < 0 || nx >= width` test three lines above.
- `report.rs:56` — `(width as u64 * height as u64 / width) as u32`, bounded by
  `height`.

`float_cmp` (4 raw hits) is a **false positive**: both unique sites are inside
`assert_eq!`'s expansion in `core/src/macros/mod.rs`, not in this repository's
code.

## 8. Why it is being enabled anyway

The measurement above is about the code that exists. Three arguments beat it:

**The casts that matter have not been written yet.** 81 of the 186 cast sites
already involve floats, and L2's text-block detector is float↔int page geometry
— the branch after this one. A lint live *before* that code is written is worth
more than one added after, and §3 above is a demonstration that "the tests pass"
was covering a gap in exactly that kind of arithmetic.

**`as` is invisible to every gate this repository has.** `docs/ci.md` records the
debug profile as the only one with overflow checks — and overflow checks do not
apply to `as`. A truncating cast is silent in debug, in release, in the tests
and in the coverage report. Clippy is the only instrument that can see it.

**An `#[expect]` with a reason is documentation of an invariant**, which is how
the rest of this repository already works: every guard rail in the solver exists
because something in the corpus broke without it. `sidecar.rs:1232`'s bound is
currently held in a comment three paragraphs away from the cast it constrains.

## 9. How warn-only and `#[expect]` compose — measured

The two requirements pull against each other on their face: per-site `#[expect]`
suppression wants the lint enabled, and "warn-only" wants it not to gate. Four
probes on a scratch crate settle it.

| pedantic | cast present | result |
| -------- | ------------ | ------ |
| off | yes | expectation fulfilled, **silent** |
| on | yes | fulfilled, silent |
| off | no (stale) | `unfulfilled_lint_expectations` warns |
| off | no (stale), `-D warnings` | **exit 101** |

The expectation is evaluated on whether the lint *would* fire, independent of
its configured level. Three consequences, and they are the design:

1. **`clippy::pedantic` never enters `[workspace.lints]`.** The existing gate —
   `cargo clippy --workspace --all-targets --all-features -- -D warnings` —
   stays byte-identical. A pedantic lint added by a future rustc is invisible to
   it. That is the warn-only property, at zero configuration cost, and it
   matters because `rust-toolchain.toml` says `channel = "stable"` and CI runs
   `rust:1-bookworm`: both float, and clippy adds lints to `pedantic` on roughly
   a six-week cadence. Without this, a rustc release turns someone's unrelated
   PR red in a file they did not touch.
2. **The `#[expect]` attributes still work** in that same default build, silently.
3. **A stale `#[expect]` fails the gate**, and that is kept deliberately. It is
   the mechanism that stops ~190 suppressions decaying into ~190 unchecked
   claims: remove a cast without removing its attribute and CI says so. The cost
   is real and is accepted — a refactor that deletes a cast must delete its
   attribute too. This is the one part of the pedantic work that is *not*
   warn-only.

## 10. What this branch builds

All five crates, one branch, stacked on Part 1.

1. **Two lints off by name**, in `[workspace.lints.clippy]` with a reason
   comment each — not "fixed":
   - `must_use_candidate` (46) — designed for published library crates. Four of
     the five crates here are internal; only `taiyaku-wasm`'s binding has an
     external surface.
   - `match_bool` (38) — clippy prefers `if`/`else`. This codebase leans on
     `match` for exhaustive dispatch deliberately.
2. **Mechanical fixes, no behaviour change**: `cast_lossless` (35, `as` →
   `u32::from`/`f64::from`), `redundant_closure_for_method_calls` (28),
   `map_unwrap_or` (14).
3. **Every remaining site** gets either a real fix — `try_from` where a bound is
   genuinely unproven — or a per-site
   `#[expect(clippy::…, reason = "…")]` **stating the bound**. Per-site, never
   module- or crate-level: a blanket allow in `taiyaku-cli` would switch the
   lint off in the file with 118 cast sites, which is strictly worse than not
   enabling it at all.
4. **`missing_errors_doc` (31) and `missing_panics_doc` (7)** get written prose.
   `/// # Errors` / `/// Returns an error if it fails.` is worse than nothing and
   does not count as done.
5. **An advisory step**, non-blocking, in both `.forgejo/workflows/ci.yml` and
   `scripts/check-all.sh`:
   `cargo clippy --workspace --all-targets --all-features -- -W clippy::pedantic`.
   After this branch its output is empty, so a new unguarded cast or a lint from
   a new rustc is the only thing that ever appears in it.

Estimated 1.5–2 days, and the diff is large: roughly 190 attributes plus ~77
mechanical edits across all five crates. That size is the argument this design
lost — crate-by-crate was proposed and rejected in favour of finishing it once,
so that L2's detector starts with the lint already live.

## 11. Order, and why

Part 1 first, Part 2 stacked on it (`rebase --onto`, per the gitea flow).

Part 1 rewrites tests in `taiyaku-core/src/solve.rs`; Part 2 adds attributes to
the same file. The conflict risk is low — different lines — but the ordering is
not arbitrary: Part 1's triage means reading the solver's arithmetic closely,
which is the same reading Part 2's `#[expect]` reasons require. Doing the
lints first would mean writing bounds for arithmetic whose verification gaps
were not yet known.

## 12. What could not be measured

- **StrykerJS against Vitest 4.1.10.** Not attempted. §5.
- **The cost of `--in-diff` on a real PR.** Deferred with the gate itself, §5.
- **Whether the 29 timeouts are genuine survivors.** `cargo-mutants` auto-set a
  20 s timeout from the baseline; a mutant that makes a loop stop terminating is
  reported as `TIMEOUT` rather than caught or missed. **Five are in
  `taiyaku-core`** — `solve.rs:227` (`-=` → `/=` in the traceback) and four in
  `credits.rs::unpaired_runs` (93 ×3, 94) — and they are counted separately
  from the 346 throughout this document, so they are **not** among the 47.
  A traceback that stops decrementing is an infinite loop rather than a wrong
  answer, which is arguably caught-by-hanging; whether the other 24 are the same
  shape is unexamined. Task 10 decides whether they need tests or a documented
  dismissal.

`solve.rs` in isolation missed 19% of its viable mutants; the workspace missed
23%. So `solve.rs` was not an outlier picked because it looked bad — it is
representative, and slightly better than average.
