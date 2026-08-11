# Bounding a test's runtime — design

**Status: settled in brainstorming on 2026-08-11. Built, and shipped on
`o39-test-timeout`.** Both bounds are live: `.config/nextest.toml`'s 60 s
per-test kill under `cargo llvm-cov nextest`, and `timeout-minutes: 10` on all
four jobs. The per-test bound was triggered on the forge itself; the job bound
stays untriggered, deliberately — §7 below is the record of both.

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This closes **O39**, the last open item from the mutation-testing arc and the
only one of the three the arc produced by accident.

## Where it came from

`cargo-mutants` reports a mutant that stops a loop terminating as `TIMEOUT`
rather than `MISSED` — its own bucket, exit 3, not counted as caught. Asking
whether that disposition was good enough turned up the gap it depends on:

- there is no `.config/nextest.toml` anywhere in the repository,
- there is no `timeout-minutes` in `.forgejo/workflows/ci.yml`,
- and CI does not run nextest at all — the `test` job runs `cargo llvm-cov`,
  which wraps `cargo test`, which has no timeout.

So the auto-timeout that makes a hang legible exists **only inside
`cargo-mutants`**. A regression of that shape reaching `main` would hang the
forge until the runner's own limit rather than fail it.

## 1. Two bounds, because they answer different questions

`timeout-minutes` bounds a **job**. A per-test timeout bounds a **test**. They
are not competing answers and neither subsumes the other:

- `timeout-minutes` alone catches every hang, including a stalled `curl` in a
  tool-install step or a `cargo build` that never finishes — but it names
  nothing. `ci.yml` already rejected exactly this shape of gate once, in its own
  words, when it declined `cargo llvm-cov --fail-under-file-lines`: *"A red step
  above a 38-row table with no indication which row is the reason is not a gate
  anyone can act on."* A red job with "exceeded the maximum execution time" is
  the same defect.
- A per-test bound alone names the test and fails precisely — but it bounds only
  tests, leaving every other step in the job unbounded.

Both land.

## 2. What lands

### `.config/nextest.toml` — new file

```toml
[profile.default]
slow-timeout = { period = "30s", terminate-after = 2 }

[[profile.default.overrides]]
filter = "test(corpus::) + test(geometry_experiment) + test(region_experiment)"
slow-timeout = { period = "60s" }
```

The default profile marks a test `SLOW` at 30 s and terminates it at 60 s. The
override marks the exempt set `SLOW` at 60 s and **never terminates it** —
omitting `terminate-after` is what makes a `slow-timeout` advisory in nextest,
not a separate key.

### `.forgejo/workflows/ci.yml`

- `timeout-minutes: 10` on all four jobs — `lint`, `test`, `wasm`, `web`. A
  stalled tool download hangs the forge from the `wasm` job exactly as well as
  from the `test` job, so bounding only the Rust ones would leave the hole open
  in three quarters of the file.
- The `test` job's runner becomes `cargo llvm-cov nextest`, keeping every
  existing flag: `--workspace --all-features`, the `--ignore-filename-regex`,
  and `--fail-under-lines 93`.
- `cargo-nextest` is installed as a checksummed prebuilt release tarball,
  following the pattern this file already uses three times over — for
  `cargo-llvm-cov`, `wasm-pack` and `node`. The digest is computed here from the
  artefact in use, because nextest publishes no checksum file for its release
  assets and a release asset can be replaced in place by whoever owns the
  repository. Re-compute it on a bump; never copy one from a README.

### `scripts/check-all.sh`

Mirrors the runner switch, so the local gate keeps matching CI. This is not
optional politeness: the script exists to be the CI-equivalent pass, and a local
gate running a different test runner than the forge is a gate that measures
something other than what it claims.

The corpus invocations are **unchanged**. They already run as a separate step
under a separate profile, they are explicitly not a CI job, and section 4
explains why they must not be bounded.

## 3. Every number traces to a measurement

| bound | value | sized against |
| ----- | ----- | ------------- |
| per-test warn | 30 s | slowest test in the workspace: **0.50–0.75 s** |
| per-test kill | 60 s | 80× that slowest test |
| job timeout | 10 min | worst observed job: **134 s** |

**The slowest test was bisected, not estimated.** Running the whole workspace
under a temporary `slow-timeout` and lowering the period until markers appear:
at 250 ms, four `SLOW` lines print; at 500 ms, two; at 750 ms, none. That is
two tests, not four — nextest re-prints a `SLOW` line at every multiple of the
period a test is still running, so the same two tests, `detect::tests::the_gap_
does_not_grow_with_the_page` and `cover::tests::a_cover_is_a_bounded_jpeg_
whatever_the_page_was`, are each printed once at 250 ms and again at 500 ms.
Nothing in 639 tests exceeds 0.75 s, and the corrected count is stronger than
the line count first suggested: it was never four tests, only ever two, each
caught twice by the same clock. Re-measured on 2026-08-11 with
`--config-file`; see `docs/notes.md`, O39, for the raw output and nextest's
own `(2 slow)` summary.

So 30 s is 40× the slowest test that exists, and the kill at 60 s is 80×. That
is deliberate slack for a shared docker runner, which is slower than this
machine by an unmeasured factor — *a guess, and it says so.* The cost of being
generous is bounded: a hang surfaces in 60 s instead of 20 s, against a status
quo where it surfaces in hours.

**Job durations, last ten CI runs**, from the forge's own
`/actions/tasks` API:

| job | min | max |
| --- | --- | --- |
| `test` | 32 s | 134 s |
| `lint` | 21 s | 85 s |
| `wasm` | 33 s | 84 s |
| `web` | 9 s | 25 s |

`timeout-minutes: 10` is 4.5× the worst observed. Uniform across all four rather
than tuned per job: a per-job number has to be re-tuned as jobs shift, and the
only thing the number has to do is be smaller than "hours" and larger than "any
honest run".

## 4. The exempt set is one that already exists

The override filter names exactly the four files that
`--ignore-filename-regex '(geometry_experiment|region_experiment|/corpus)\.rs$'`
already names in both the coverage steps. **One exempt set, two gates, one
reason**: these are gated one-off measurements against 26 GB of real material,
not unit tests, and neither gate can judge them by the standards it applies to
the rest.

The evidence that a global kill would be wrong is recorded rather than feared:
**`geometry_experiment` in the debug profile with `TAIYAKU_CORPUS` set runs 40+
minutes — one run was killed by hand at 1 h 09 m.** A 60 s kill applied globally
would destroy a deliberate measurement every time someone ran one. The corpus
checks themselves are cheaper but not cheap: thirteen standing checks against
real archives.

Bounding them at all — even advisory — is still worth doing. They run locally
under a human's eye, so a `SLOW [> 60.0s]` line is enough signal, and no number
has to be invented for a population nobody has timed.

**A caution for whoever edits the filter.** nextest's `test()` predicate matches
the test name including its module path, so `test(corpus::)` selects the same
sixteen tests `cargo test corpus::` does — **thirteen standing checks** against
real archives (ten in `taiyaku-cli`, three in `taiyaku-serve`) plus three
`corpus::tests::` helpers that only assert how the `TAIYAKU_CORPUS` variable is
validated and touch no material. Matching `cargo test`'s existing selection
exactly is the point: the exempt set must be the set the corpus step already
runs, or the two disagree silently. This was verified with
`cargo nextest list -E`, and an early count of "3" during that verification was
a grep artefact — nextest colours its output, so the literal bytes `corpus::`
do not appear on every line it prints. Count with `--color never` or by parsing,
never by grepping the coloured stream.

## 5. Coverage: measured, and a non-issue

nextest runs each test in its own process; `cargo test` runs them threaded in
one. That can move coverage. Measured on this machine, both with the same
`--ignore-filename-regex`:

| | `cargo llvm-cov` | `cargo llvm-cov nextest` |
| --- | --- | --- |
| lines, three runs | 97.69% / 97.69% / 97.69% | 97.68% / 97.71% / 97.69% |
| uncovered lines | 399, 399, 399 | **402**, 397, 399 |
| `--fail-under-lines 93` | pass | pass |
| `scripts/coverage-floor.mjs` | pass, 38 files | pass, 38 files |
| wall clock, warm | 1501 ms | 915 ms |

**Coverage is unchanged, and nextest makes it nondeterministic.** An earlier
draft of this section claimed nextest *improved* coverage by two lines. That was
a single pair of runs read as a trend, and it is wrong: repeated three times
each, `cargo test` returns 399 uncovered lines every time with zero per-file
variance, while nextest returns 397, 399 and 402 from the identical command.

The variance is nextest's, not the tree's, and the mechanism is the one that
makes nextest worth having: each test gets its own process, so tests that used
to share a process — and a tokio runtime, and each other's warmed paths — no
longer do. Both varying files are in the async server crate:
`taiyaku-serve/src/lib.rs` (44 / 42 / 44 uncovered) and
`taiyaku-serve/src/routes.rs` (55 / 52 / 52).

**It cannot make either gate flaky, and that was checked rather than assumed.**
The aggregate floor is 93 against a measured 97.68–97.71 — 4.7 points of margin
against a 0.03-point swing. The per-file floor is **57% lines**, and the two
varying files sit at **90.74%** and **97.48–97.62%** (97.62% is the modal value
across three runs; a re-run lands at 97.48%): a three-line swing moves the
worse of them by about 0.6 points, leaving 33 points of headroom. Neither gate
needs re-baselining, and neither is at risk.

Worth stating plainly because it is a cost, not a free win: this branch trades a
deterministic coverage number for a bounded test runtime. The number stays far
from every threshold, so the trade is cheap — but anyone who later tightens
`--fail-under-lines` toward the measured figure needs to know the last digit
moves on its own.

**Zero doctests in the workspace**, so nextest's inability to run doctests costs
nothing today. It is a real constraint on the future: a doctest added after this
lands would be silently unrun by CI. Whoever adds the first one has to add a
`cargo test --doc` step alongside.

## 6. What this deliberately does not bound

- **The `web` job is already bounded** and is not touched: vitest's default
  `testTimeout` is 5000 ms per test and nothing in `web/vite.config.ts`
  overrides it. Only the Rust half was ever unbounded. `timeout-minutes` is
  still added to the job, for the non-test steps.
- **`timeout-minutes` bounds a job, not a step.** A stalled `curl` still burns
  the full ten minutes before the job dies. Bounding individual steps is
  possible and is not done: four jobs times a dozen steps is a dozen numbers to
  keep true, against one that has to be right.
- **`cargo-mutants` keeps its own auto-timeout.** It is a local tool run by hand,
  it already bounds itself, and its `TIMEOUT` bucket is the disposition that
  raised this item in the first place. Out of scope.

## 7. How it was verified

Not by argument. The branch's **first push carried a deliberately
non-terminating test**, so the whole path was exercised on the real runner —
nextest under `cargo llvm-cov`, inside `rust:1-bookworm`, on this forge. The
`test` job went red and named the test: run 213 failed at 153 s with
`TIMEOUT [  60.003s] taiyaku-core sidecar::tests::tmp_hangs_to_trigger_the_ci_bound`.
The test was then removed and the branch pushed again; run 214 came back green,
`test` at 117 s.

**Both runs are quoted in the PR body**, with their run URLs.

This is the step #45 could not take for its `continue-on-error` key, which it
had to describe as "untriggered rather than untested" — `-W clippy::pedantic`
exits 0 however many warnings it prints, so a green run never exercised the
guard. A timeout had no such excuse: it could be triggered on demand, so it
was.

The `timeout-minutes` bound was *not* separately triggered, and this says so
plainly rather than implying otherwise. Deliberately hanging a job for ten
minutes to watch it die would have cost a ten-minute CI run to verify a single
documented key. It stayed untriggered, and `ci.yml` carries a comment saying
so. A third, final push — run 215, from branch tip `71bc7cd`, carrying neither
the hang nor its revert — came back green on all four jobs: `lint` 28 s,
`test` 37 s, `wasm` 36 s, `web` 9 s.

## 8. Closing O39

`docs/status.md` is authoritative, and closing an O-number is a five-part edit,
not a tick:

1. the table row's checkbox
2. the prose bullet, rewritten to say what was done and what remains
3. the test count near the top — it appears **twice**
4. the "N are now closed — leaving M" sentence
5. the "What remains … M open items" line near the bottom

Then a sweep for the bare string `O39` across `docs/` and `web/src/`, reading
every hit rather than only the ones matching an expected phrase. There are known
references in `docs/notes.md` and in the mutation-testing narrative in
`docs/status.md` — historical narrative is marked (`*O39 has since closed*`)
rather than rewritten.

The measurements in section 3 and section 5 go in `docs/notes.md`, not
`status.md`: status.md's own preamble promises every claim there traces to a
measurement recorded in notes.

## 9. Risks

- **The nextest install adds a fourth downloaded binary to CI.** Each is a
  supply-chain surface and each is pinned by a digest computed here. This is
  the pattern the repository already accepted three times; it is not a new
  decision, but it is a fourth instance of an existing cost.
- **A slow CI container could false-kill at 60 s.** Unmeasured — this machine
  is not the runner. Mitigated by 80× headroom over the slowest real test. If
  it fires on an honest test, the fix is to raise the period, and the failure
  is at least legible enough to know that is what happened.
- **`cargo llvm-cov nextest` is a different code path** than `cargo llvm-cov`,
  in a tool this repository only consumes. Both gates were measured passing
  under it, but only on this machine, not yet in `rust:1-bookworm`. The first
  push settles that too.
