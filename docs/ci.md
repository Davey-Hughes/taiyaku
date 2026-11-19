# Continuous integration

Workflows live in `.forgejo/workflows/`. Forgejo looks there first and falls back
to `.github/workflows/`; using the Forgejo path keeps the intent unambiguous.

## Jobs

| Job | Runs |
| --- | ---- |
| `lint` | `cargo fmt --check`, then installs `libarchive-dev` by apt for `cargo clippy -D warnings` and `cargo doc -D rustdoc::broken_intra_doc_links`, both `--all-features`, then `cargo check --workspace --all-targets --all-features --locked` at the `rust-version` the root `Cargo.toml` declares — see "Toolchain note" |
| `test` | installs `strace` and `libarchive-dev` by apt, then `cargo llvm-cov nextest --workspace --all-features` — runs the tests under nextest's per-test timeout *and* gates their aggregate coverage — then installs node and runs `scripts/coverage-floor.mjs` over a re-rendered JSON report for the per-file half, `scripts/status-test-count.mjs`, and last the comment guard, `scripts/check-comments.mjs` |
| `default-features` | the build a plain `cargo build` makes, with no libarchive installed: fails if the default dependency graph reaches `compress-tools`, then `cargo clippy -D warnings`, the doc-link check and `cargo nextest run --workspace`, all without features — see "The default-features job, and why RAR is a feature" |
| `wasm` | builds `taiyaku-core` and `taiyaku-wasm` for `wasm32-unknown-unknown`, builds the browser package with `wasm-pack`, and runs `scripts/smoke-wasm.mjs` against the compiled module |
| `web` | `pnpm install --frozen-lockfile`, `biome check` (formatting **and** lint), `tsc -b` typecheck, `vitest run --coverage` in `web/`, then `scripts/coverage-floor.mjs` for the per-file half |
| `docker` | **on `main` only (a push, the weekly schedule or a dispatch), after the five above pass**: builds the image in an isolated buildx builder, runs it until its health check passes, probes three paths and checks its binary holds the commit, then pushes `forge.daveynet.xyz/davey/taiyaku:sha-<7>`, and `:latest` when its commit is main's tip (a refresh re-pushes, with patched packages, the `sha-<7>` of the commit it built and, at the tip, `:latest`) — see "The image job" |

The `test` job's tests run with `TAIYAKU_COMMIT` set to the commit under test,
so `about`'s route test compares the commit `/api/about` reports with a real
40-character hash, and its `dirty` with `false`, because a commit passed in is
never marked. Where it is unset, as `check-all.sh` leaves it unless one is
exported before the script runs, both sides are what `taiyaku-serve`'s build
script read from git: HEAD, with `dirty` true when tracked files differed from
it, or `null` and `false` where it read no commit.

The `test` job installs node for its three node steps: the per-file floor, the
test count and the comment guard. `rust:1-bookworm` carries none, and the
per-file floor is a node script because it is the *same*
script the `web` job runs — see "The other half" below for why one file rather
than two.

The `wasm` job is the one worth keeping even when it feels redundant. The design
depends on the core being shareable between the browser reader and a future
native shell, and that property is easy to destroy accidentally — a single
dependency that assumes threads or links a native library will do it. Without a
gate, the breakage stays invisible until the reader is being built, by which
point the offending dependency is load-bearing.

### The doc-link check, and why it denies one lint rather than warnings

**`RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links" cargo doc --workspace
--no-deps --all-features`**, in the `lint` job and in `check-all.sh` beside it.
The `default-features` job runs it again without `--all-features`, for a link
that resolves only when a feature is on.

An unresolved `[`name`]` sends the reader to a symbol that does not exist, and
**a rename is how it happens**. One shipped: #92 renamed `missing_members` to
`missing_by_edition` and left `EditionCard::missing`'s own `See
[`missing_members`]` behind, which survived that branch's gate, its CI *and* a
mutation sweep and was found by reading. Adding this also turned up two more
unresolved links and two ambiguous ones — `[`derive`]`, which is both this
module's function and the built-in attribute macro, and so resolved to neither
until it was written `[`fn@derive`]`.

**`-D warnings` here would be wrong and would not survive.** The other rustdoc
lint that fires in this workspace is `private_intra_doc_links`, and linking a
public item's doc to the private helper it delegates to is this codebase's own
convention: `browse::missing_by_edition` names `member_clause`, `home` names
`LIST_CAP`, `search` names `TRIGRAM_MIN`. There were **79** of those against
the **4** genuine link errors when this was added, so denying warnings
wholesale would have failed on the first run and been deleted by the end of the
week. One rule, denied precisely.

**It is cheap because `--no-deps` is doing the work**: documenting the
workspace's own crates and nothing they depend on.

### The default-features job, and why RAR is a feature

`taiyaku-read` reads RAR through libarchive, a C library, behind its
`rar-libarchive` feature. `taiyaku-library`, `taiyaku-serve` and
`taiyaku-cli` each pass the feature down, and it is off by default, so a
plain `cargo build` needs neither libarchive nor its headers. `lint` and
`test` build with `--all-features`, installing `libarchive-dev` from Debian's
archive for it, and `wasm` builds two crates that never reach `taiyaku-read`,
so without this job nothing would compile the default build, or run the tests
that pin how it refuses a RAR: by name, saying this build of taiyaku reads no
RAR.

`default-features` installs no libarchive, and that is the point. Its first
step after the tools fails when `cargo tree --workspace -e normal,build,dev`,
with no features, lists `compress-tools`, and prints the crates that pulled
it in. The `dev` edges are there because a dev-dependency that turned the
feature on would link libarchive into every default test build. Then clippy,
the doc-link check and `cargo nextest run --workspace`, all without
features. `check-all.sh` runs the same steps after the test job's, on a
machine that has libarchive, so there the guard is what shows the default
graph does without it; and it refuses to start at all without libarchive's
development files (`pkg-config libarchive`), naming the packages to install,
because the lint and test jobs need them.

`compress-tools` comes from the owner's fork on GitHub, pinned by commit in
the workspace's `[patch.crates-io]` until a release carries its fixes (O382).
The fork's `taiyaku-rar` branch is the only ref that holds that commit, so it
is kept, and not moved, until the patch goes.
Cargo loads a patch's source whenever it resolves the workspace, with the
feature or without it, so every job in `ci.yml` that runs cargo fetches it
from there, as do the image build's two Rust stages whenever they rebuild,
and `advisories.yml`'s fetch step.

### The workflows that are not jobs

`mutants.yml` is dispatched by hand and **gates nothing**. It is not in the table above,
it is not in `check-all.sh`, and no pull request waits on it. See "Mutation
testing, and why it cannot be a gate" below.

`advisories.yml` runs `cargo deny check advisories` on pushes to `main`, a
dispatch, a weekly schedule, and pull requests that touch the lockfile, the
manifests or its own configuration. Nothing in `ci.yml` waits on it, so a newly
published advisory cannot hold up the image. See "The advisory check" below.

## The local mirror, and the one thing it cannot mirror

`scripts/check-all.sh` reproduces all five jobs above, in the same order with
the same flags, so the local gate and the CI gate cannot drift. That claim was
false between M5a and 2026-08-07: the script said "three jobs" and reproduced
three while CI had grown a fourth, so **a TypeScript change could take the local
gate green having had nothing run against it at all**. The `wasm` job had
drifted the narrower way over the same period — its two `cargo build`s were
mirrored, but not the `wasm-pack` package build or the smoke run, which is the
only thing anywhere that *executes* the generated module.

Skips are named twice, once where they happen and again in a closing summary
that refuses to print a bare `all green` when anything was skipped:

```
all green, except NOT MEASURED:
  - standing corpus checks (TAIYAKU_CORPUS unset)
  - the wasm job (--no-wasm)
  - the web job (web/node_modules absent — run `pnpm -C web install`)
```

The check at the declared Rust version runs locally only where its toolchain is
already installed. CI installs that toolchain; the script never downloads one,
and names the check under `NOT MEASURED` instead.

The corpus checks are the one thing the script measures that the forge cannot,
since CI has no corpus. Everything else runs in both places — with one
exception.

## Two profiles, and the gate that took an hour

**Setting `TAIYAKU_CORPUS` used to cost far more than the corpus job itself.**
The script exports it before the `test` job, and that job is an *unfiltered*
`cargo test --workspace --all-features` — the `corpus::` filter belongs to the
separate corpus job below it. So a local gate run with a corpus present ran all
thirteen standing checks twice, both times in **debug**, and ran the two one-off
measurement modules as well. `geometry_experiment` fingerprints roughly 1.4
million Lanczos3 crops: about a minute under `--release`, about forty without.
Measured rather than estimated — one gate run was killed at **1h09m** still
inside that single test, and another at over 25 minutes without reaching the
second corpus pass.

**Fixed by giving the two jobs the profiles they each want**, rather than by
documenting a workaround:

- The `test` job runs `env -u TAIYAKU_CORPUS cargo test --workspace
  --all-features`. Removing the variable drops the corpus checks and both
  experiment modules, and makes the job a *truer* mirror of CI, which has no
  corpus and never ran any of them. **4.4 seconds.**
- The corpus job runs `--release`. It is not a CI job, so the mirror above does
  not depend on its profile. **6.9 seconds** warm; about 1:13 the first time,
  since `[profile.release]` is `lto = "thin"` with `codegen-units = 1`.

The whole gate — all four jobs plus the corpus — now takes about **15 seconds**
warm.

**What that gives up, and how to get it back.** Integer overflow panics and
`debug_assert!` are compiled only under `debug_assertions`, and this repository
has two of the latter that matter: `PageRange::new`'s "page range must not be
inverted" and `Run::paired`'s equal-length check, both in
`taiyaku-core/src/lib.rs`. The debug `test` job exercises both through the unit
tests; the release corpus job cannot fail on either over real material.
`scripts/check-all.sh --corpus-debug` runs that pass in debug instead — worth
doing when touching `PageRange`, `Run::paired`, or arithmetic on page and slot
indices, and not otherwise. The script says which profile the corpus job used,
right above the run, so a reader does not have to remember this page.

The experiment modules are still not filtered out of the test target: a
one-off measurement that silently stops being *compiled* is the failure their own
assertions exist to prevent. What changed is the gate's environment, not their
gating — each module's header names the command it is meant to
be run with.

**CI stays in debug, and that was measured rather than assumed.** The obvious
next step after the split above is to make CI match — the local corpus job got
much faster under `--release`, so why not the forge. It is the wrong move here,
for three reasons that are properties of *this* CI rather than of release builds
in general:

- **There is no cache.** Every job above clones with `git init` and a `--depth 1`
  fetch into a fresh container, so every run compiles the workspace from
  nothing. There is no warm second run for a release profile to pay off across.
- **There is no corpus.** CI's `test` job runs the unit suite alone — 4.4
  seconds in debug. The tests release actually speeds up are the thirteen
  standing corpus checks, and CI skips every one.
- **`[profile.release]` is tuned for the aligner, not for compiling.**
  `lto = "thin"` with `codegen-units = 1` is the slowest configuration to build,
  across an `axum`/`tokio` tree.

Measured cold, with an empty `CARGO_TARGET_DIR`, which is CI's condition on
every run:

| Profile | `cargo test --workspace --all-features --no-run` |
| ------- | ----------------------------------------------: |
| debug   | 22.5 s |
| release | 1 m 26.3 s |

So release would cost CI about 64 extra seconds per run to save a 4.4-second
test suite, and give up overflow checks and both `debug_assert!`s while doing
it. The local gate and CI therefore run the same `test` job in the same profile,
and the mirror in `scripts/check-all.sh` holds exactly — the release profile
appears only in the corpus job, which is the one thing CI does not have.

**The lockfile check *is* mirrored, and until 2026-08-09 it was not.** This
document used to carry that as a permanent gap: CI's install builds
`node_modules` from nothing, while running the same command locally would
delete a working install and rebuild it on every gate run, so the one property
it checks — that `package.json` and the lockfile agree — went unchecked, and a
drifted lockfile passed `check-all.sh` to fail the forge.

pnpm closes it, because it has a command that is that check and nothing else:
`pnpm -C web install --frozen-lockfile --lockfile-only` verifies the lockfile
satisfies `package.json` without touching `node_modules`. **Measured across
three consecutive runs: 249ms, 239ms and 237ms.** Verified by adding a
dependency the lockfile had never seen — the gate was green before the step
existed and failed after it, naming the package:

    specifiers in the lockfile don't match specifiers in package.json:
    * 1 dependencies were added: is-odd@^3.0.1

It is run directly rather than piped, because a pipe masks the exit code — which
was caught while writing the probe, and is exactly how a gate stops gating.

## Bounding a test's runtime: nextest and the job timeout

Two independent bounds, added on `o39-test-timeout` to close O39: nothing in
this repository bounded a test's runtime before this branch, and CI did not
run nextest at all — the `test` job ran `cargo llvm-cov`, which wraps
`cargo test`, which has no timeout. A regression that stopped a loop
terminating would have hung the forge until the runner's own limit rather than
failed it.

`.config/nextest.toml` bounds a **test**, precisely and by name:
`slow-timeout = { period = "30s", terminate-after = 2 }` marks a test `SLOW`
at 30 s and kills it at 60 s, both sized off a bisection of the whole
workspace rather than guessed — nothing in 639 tests exceeds 0.75 s, so 30 s
is 40x that and 60 s is 80x. An override exempts the tests in every file
`--ignore-filename-regex` exempts from both coverage steps above except
`borrow_frequency.rs` and the four `test_support.rs`: its filter names tests,
not files (read from the filter against the files on 2026-11-16; this said
nine of fourteen on 2026-11-06, and twelve, the regex's whole count, on
2026-08-18) — the standing corpus checks, three `corpus::tests::` helpers that
only assert how `TAIYAKU_CORPUS` is validated and touch no material, and the
one-off experiment modules — at a 60 s `SLOW` mark with no `terminate-after`,
which is what makes a `slow-timeout` advisory rather than a kill:
`geometry_experiment` in the debug profile against a real corpus runs 40+
minutes, and a global 60 s kill would destroy that measurement every time
someone ran one.

`timeout-minutes: 10` on all five jobs bounds a **job** — the only thing that
can catch a hang outside a test, such as a stalled `curl` in one of the six
tool-install steps across this file or a `cargo build` that never finishes.
It is sized at 4.5x the worst honest job observed over the ten runs before
this branch (134 s), applied uniformly rather than tuned per job. Neither
bound subsumes the other: the per-test bound cannot see a hang outside a test,
and a job timeout alone produces a red step naming nothing — the same defect
this file already rejected once in `--fail-under-file-lines`'s own case, see
"Proving the gate gates" below.

`cargo-nextest` is installed the same way as the other three tool binaries in
this file — a checksummed prebuilt release tarball rather than `cargo
install`, the pattern `cargo-llvm-cov` sets directly above it in this same
job and `wasm-pack` sets in the `wasm` job: this job only runs the tool,
never links against it. `cargo-nextest-0.9.140-x86_64-unknown-linux-musl.tar.gz`
is **9,605,767 bytes** (≈9.2 MiB), pinned to digest
`bc2a998567816eef7b087b920036f49fb89aa2a8a603f6a3d84098be160b62c5`, computed
here from that artefact rather than copied from upstream, since nextest
publishes none of its own for its release assets. The URL was confirmed to
resolve and download before being wired in:
`get.nexte.st/0.9.140/x86_64-unknown-linux-musl`, nextest's own install
shortlink, was tried first and **404s**, so the GitHub releases URL above is
the one that works. `tar -tzf` confirms the archive holds `cargo-nextest` at
the tarball root — the same shape as the `cargo-llvm-cov` step directly
above it — which is why `tar -xzf … -C /usr/local/bin` places it correctly
with no subdirectory to strip, and `cargo nextest --version` then reports
`cargo-nextest 0.9.140`. It is the fourth such binary this workflow fetches,
after `cargo-llvm-cov`, `node` and `wasm-pack`, a fourth install sized and
checksummed the same way as those three.

`strace` is the one tool this workflow takes from Debian's archive rather than
a pinned tarball (`libarchive-dev`, a library's headers, is the other package
it takes from there), installed in the `test` and `default-features` jobs and
in `mutants.yml`'s sweep.
O104's three tests — two of `atomic`'s and one of `stamp`'s — prove that a
staged file is synced after its last write and before it is renamed into
place, and its directory after, and nothing a
test can read tells a synced file from an unsynced one, so each re-runs itself
as a child under `strace -f -qq -y` and asserts on the syscalls. **They fail
rather than skip without it**, unlike the `NOT MEASURED:` shape `format`'s
`tar` test uses, because they are the only guard on a crash destroying a
reader's file and a skip is a green run that measured nothing. So in the sweep
the install is not optional either: without it the baseline run of
`taiyaku-read` or `taiyaku-library` fails and the sweep reports no verdict.
The checksum reasoning above does not carry over — it exists because a GitHub
release asset can be replaced in place, while apt checks every package against
the archive's signed Release file — and neither does the version reasoning,
since the four flags the tests use all predate bookworm's strace 6.1 by a
decade.

**Not measured:** no timing comparison against `cargo install cargo-nextest`
was taken. The only paragraph on this pattern that measured the alternative
it beat is `wasm-pack`'s above — a multi-minute compile, rejected in favour
of a 2.9 MB prebuilt binary — and this step inherits that precedent rather
than re-measuring it; the gap is stated rather than left silent.

Switching the `test` job's runner from `cargo llvm-cov` to
`cargo llvm-cov nextest` was measured for a side effect before it shipped:
nextest runs each test in its own process, which can move coverage — see
`docs/notes.md`, O39, for the comparison and why neither coverage gate is at
risk from the resulting few lines of run-to-run variance.

The per-test bound was triggered on the forge itself, not only reasoned
about: the branch's first push carried a deliberately non-terminating test,
and the `test` job failed by name at 153 s, then passed at 117 s on the exact
revert. The job-level bound stays untriggered, deliberately — proving it
fires would cost a ten-minute CI run to verify one documented key.
`docs/status.md` and `docs/notes.md`, both under O39, carry the full
measurement record.

### `NOT MEASURED:` is invisible without a nextest override

Found on the library scanner branch, and it recurs anywhere a test guards
itself and skips — `taiyaku-serve/src/tests.rs`'s
`one_work_that_cannot_be_forgotten_does_not_stop_the_others` set the pattern
first, and `taiyaku-library/src/watch/tests.rs`'s
`a_root_that_cannot_be_written_falls_back_to_the_short_interval` repeats it: a
test that cannot force the condition it exists to pin — root ignores `chmod
0555`, here, because **CI's** `test` job runs inside `rust:1-bookworm` with
no `user:` directive, so every step in it executes as root ("Jobs", above,
and the comment on "executing as root in the test container" in the `test`
job's own steps). `check-all.sh` has no container of its own — it is a shell
script that runs the same commands as whichever user invokes it, so locally
the `chmod` ordinarily does restrict and the test measures rather than
skipping; it is CI's containerised job, not `check-all.sh` itself, that
reliably hits the guard. Either way, the test
prints `eprintln!("NOT MEASURED: …")` naming itself and returns, rather than
asserting something the setup did not actually arrange.

**That line is silent in the job this repository actually gates on.** The
`test` job runs `cargo llvm-cov nextest --workspace`, and nextest — like
plain `cargo test` — captures a passing test's stdout and stderr and shows
none of it. `--nocapture` (`cargo test`'s flag; nextest's own is
`--no-capture`) is what would surface it, and `check-all.sh` passes that only
to the two `corpus::` invocations near the end of the script (`run cargo test
--workspace --all-features corpus:: -- --nocapture --test-threads=1` and its
`--release` sibling), never to the main `cargo llvm-cov nextest` run this
section is about. So a test that self-skips in the main suite would pass green,
and the one sentence saying it measured nothing would go nowhere anyone
running the gate normally will see it — a skip nobody is told about reads
exactly like a pass, which is the opposite of what the pattern exists for.

**That is closed for this test, by the override this section's title names.**
`.config/nextest.toml` carries

```toml
[[profile.default.overrides]]
filter = "test(a_root_that_cannot_be_written) + …"
success-output = "final"
```

`final` prints a passing test's captured output in the summary at the end of
the run, with no `--nocapture` needed anywhere the binary is invoked —
including the `cargo llvm-cov nextest` run `check-all.sh` and `ci.yml` both
use, neither of which passes one. So under root, where the `chmod` buys
nothing, the `NOT MEASURED:` line does reach the log.

The filter names eight self-skipping tests: the four `CAP_DAC_OVERRIDE` ones in
`taiyaku-library`, `taiyaku-serve`'s
`one_work_that_cannot_be_forgotten_does_not_stop_the_others` — the test that
set the pattern, and which this section previously left out —
`taiyaku-read`'s `a_hand_written_tar_is_one_gnu_tar_can_list`, which skips
where `tar` is not on `PATH`, and the fingerprint pass's
`lower_renices_and_idles_this_thread_alone` and `a_worker_runs_niced_and_idle`,
which skip where `renice` or `ionice` is not on `PATH` or the thread is already
lowered. It also names a module, `jxl::process_tests`, whose priority tests
skip where `renice`, `ionice` or `nice` is not on `PATH` or the process
is already near niceness 19. Two of the four this section first named were added
*after* the override and not added to it, which is this trap arriving a second time in the
same branch.

**A new self-skipping test does not join it by itself.** nextest's `filter`
selects tests by name, not by what they print, so there is no expression that
catches the pattern — writing `eprintln!("NOT MEASURED: …")` in a new test
means editing `.config/nextest.toml` in the same commit, or the line goes
nowhere again.

Nothing here re-plumbs `--no-capture` into the main run — that would
un-capture *every* passing test's output, trading one invisible line for
thousands of visible ones on every green run. The trap is worth naming so the
next branch that reaches for this pattern outside `corpus::` does not assume
the sentence its test prints reaches anyone: a claim that depends on a
permission actually holding needs a second home that is not stderr in the
main job — a comment at the assertion it stands in for, and a line in
`docs/status.md` naming what could not be measured, the way the scanner
branch's own `docs/notes.md` entries name what could not be measured rather
than leaving a silent skip to speak for itself.

## The coverage gates

Two gates, one per language, and **four CI steps** — each language runs an
aggregate check and then a per-file one. This section said "two of them, one per
language" until 2026-08-10, which was true when the Rust half had no per-file
check and the web half's had just been added. They are gates rather than reports:
each fails its job rather than filing a number somewhere. Every one was **proven
by regression** before being written up — see "Proving the gate gates" below —
because an unproven gate is indistinguishable from no gate.

| | aggregate step | per-file step | set in |
| --- | -------------- | ------------- | ------ |
| web | four thresholds — `statements`/`branches`/`functions`/`lines` | four floors, and ≥161 files in the report | `web/vite.config.ts`; `scripts/coverage-floor.mjs` |
| Rust | `--fail-under-lines 91` | one floor, on lines, ≥72 files in the report, and a file for every workspace crate | `ci.yml` and `check-all.sh`; `scripts/coverage-floor.mjs` |

Four steps, but **three kinds of failure**, and it is worth being plain about
which catches what, because each is invisible to the others:

- **the tree regressing overall** — what the aggregate thresholds see;
- **one file that is *in* the report and has stopped being tested** — which no
  aggregate can see, because a 400-line module going dark moves a workspace total
  by half a point. Proven below, on both languages;
- **files that have left the report entirely** — which no *per-file* check can
  see either, because a file that is absent is a file nothing visits. Only the
  count sees a collapse, and only the count catches a one-character edit to a
  glob taking 14 files out of the gate for less than half a point on every
  metric — and, four times now, doing it without tripping the count either:
  until 2026-08-14 at `minFiles: 50`, again until 2026-09-11 at 59, again
  until 2026-09-19 at 101, and again until 2026-10-28 at 108.

Neither gate is a separate job. `cargo llvm-cov nextest` *runs* the tests, so it
replaced the `test` job's `cargo test`; `vitest --coverage` runs the same two
projects the `web` job already ran, so `pnpm -C web run test:coverage` replaced
the separate `test` and `test:jsdom` steps. The cost is instrumentation on a
build both jobs already paid for, not a second cold compile. **Rejected
alternative:** a dedicated `coverage` job, which would have duplicated the whole
workspace build for the Rust half.

**What the per-file halves added to that bill.** On the Rust side, one extra
`cargo llvm-cov report --json` — and it is not a second test run. It re-renders
the profile data the aggregate step already produced: no rebuild, no re-executed
test binary. **Measured here three times against the existing profile data:
0.176s cold, then 0.128s and 0.131s.** The instrumented run it sits beside was
**7.68s**; that figure is the one taken when the step was written and is carried
forward rather than re-measured, since re-measuring it means paying for a fresh
instrumented pass to learn something already known. Either way the ratio is not
close: the report pass is under two percent of the run.

It is a separate command rather than another flag because `--text` and `--json`
are mutually exclusive, and dropping the text table would cost the `test` job its
only human-readable coverage output.

The `test` job also now pays a node install it did not pay before —
`node-v26.7.0-linux-x64.tar.xz`, **33.5 MB** (`content-length: 33539588`,
confirmed against nodejs.org), checksummed and extracted. The same fetch the
`wasm` job already makes, measured there at **1.4–1.6 seconds** on this
development machine across three runs; see "Running the compiled module in CI"
for that measurement and for why a tarball beats a dual-toolchain image. **That
is a local figure, not a runner one** — no runner network was reachable to time
it — but it is the cost the job took on, and it bought the Rust half the check
`--fail-under-lines` cannot express.

### The numbers, and where they came from

Measured on this machine on 2026-08-09, on the commit that set the thresholds.
Each threshold is **two points below measured, rounded down to a whole number**,
and the measured figure sits beside it in the source so a later reader can see
the headroom without re-running anything.

*The web statements and branches rows read 98.07% and 94.44% until 2026-08-10,
when a review re-ran the suite and got 98.03% and 94.38%. Corrected from
`web/coverage/coverage-summary.json` rather than from the text table. Neither
threshold moves — 98.03 and 94.38 still round down to 96 and 92 — so the gate
was never set wrong, only this document's record of what it was set from, and
no run of it could have caught that. The same two figures were wrong in
`web/vite.config.ts`'s comment beside the thresholds and in one bullet of
`docs/status.md`, while a second bullet a few lines further down that same
document had already recorded 98.03% for the same run — the repository
disagreeing with itself in two places about one measurement, which is the
signal a re-measurement was owed.*

| gate | metric | measured | threshold | set in |
| ---- | ------ | -------- | --------- | ------ |
| web | statements, aggregate | 97.72% | 96 | `web/vite.config.ts` |
| web | branches, aggregate | 94.05% | 92 | `web/vite.config.ts` |
| web | functions, aggregate | 99.16% | 97 | `web/vite.config.ts` |
| web | lines, aggregate | 99.31% | 97 | `web/vite.config.ts` |
| Rust | lines, aggregate | 92.28% (2026-11-12, Rust 1.100.0) | 91 | `--fail-under-lines`, in `ci.yml` **and** `check-all.sh` |
| Rust | lines, **per file** | 74.48%, `taiyaku-serve/src/lib.rs` | 57 | `scripts/coverage-floor.mjs` |
| web | **files in the report** | 168 (2026-10-28) | 161 | `scripts/coverage-floor.mjs` |
| Rust | **files in the report** | 92 (2026-11-02) | 72, and every workspace crate present | `scripts/coverage-floor.mjs` |

**The four web *aggregate* rows are measured 2026-09-12, on the branch that
closed O103, and the margin they carry was worked for rather than found.** They
read 97.49 / 93.99 / 99.10 / 99.17 on 2026-08-14 and 97.42 / **92.80** / 99.16 /
99.22 a month later — `branches` down to **0.80** over its threshold from the
2.38 it was set with. O98 left them as they stood, on the ground that whether 92
still holds is a judgement rather than a transcription; O103 is that judgement,
and it went the way this file's own rule says it must. **92 did not move.** The
arithmetic is under "Why the branches margin narrowed" below and the run is in
`docs/notes.md` under "O103: the branches margin, and what was actually under
it". The Rust *per-file* row moved with
the same run — `taiyaku-serve/src/lib.rs` at **74.48%** against a floor of 57 —
and the second summary table further down had named `truth.rs` at 73.68% for it,
a file that has since reached 100%. Two rows for one figure, disagreeing; both
now read the measured one. Counts and worst-file readings are transcriptions and
are corrected on sight; the four aggregate *thresholds* are judgements and are
not.

The Rust rows are recomputed from `target/rust-coverage.json` on 2026-08-18,
under the gate's own `--ignore-filename-regex`. They previously read 93.95%,
`truth.rs` at 73.68%, and 43 files against 32 — figures from before the test
modules moved into files of their own and before the scanner's crate grew,
and none of them survived checking. **The gate was green throughout**: every
threshold is derived by `scripts/coverage-floor.mjs` from the data rather
than typed in beside this prose, which is why the prose could go stale
without anything failing. That is the pattern `docs/status.md` warns about,
appearing in the document that describes the gate.

*The Rust aggregate row read 93.64% against a threshold of 93 from that
recomputation until 2026-11-12, when Rust 1.100.0 changed what a line's
coverage counts. It now carries the 1.100.0 reading and the owner's threshold
of 91, which is not the two-points rule's figure; see "Why the Rust floor is
91, and the toolchain is pinned" below.*

The four web per-file floors have their own table under "The other half" below,
beside the lowest file each was derived from **and the lowest file today**,
which are not the same and stopped being the same without a run saying so.

**The two minimums do not follow the two-points rule, and are not meant to.**
They are counts, not percentages, and the failure they exist to catch is not a
slide — it is a collapse. Web carries eight of margin (161 against 169) because a
looser number is too loose to catch the near miss recorded below, where a
one-character edit to a glob drops 14 files and every percentage stays green.
**It was 50 against 57 until 2026-08-14, and by then the tree had grown past
it**: the same near miss reached 52 files and 50 let it through. Seven is the
rule; the number under it has to be re-derived as web/src grows.
Rust is not held by a margin alone: a count floor cannot catch a crate leaving
without going stale, so every workspace crate is checked by name as well. The
floor and the reason are under "Why the Rust `minFiles` is 72, and a crate check
holds the rest" below.

**The Rust per-file floor is loose by construction and that is stated rather than
apologised for.** The spread between the workspace's best-covered file and its
worst is too wide for one number to catch a module sliding from 95% to 80%. The
five lowest on a clean tree, re-measured 2026-08-18:
`taiyaku-serve/src/lib.rs` 78.75%, `taiyaku-cli/src/credits.rs` 82.51%,
`taiyaku-core/src/regions.rs` 85.71%, `stump/resolve.rs` 87.61%,
`taiyaku-library/src/watch.rs` 87.76%. (It read `taiyaku-serve/src/truth.rs`
73.68% at the head of that list on 2026-08-14; that file's **lines** now measure
100.00% — its 93.75% is the *regions* column, which is not what this floor
reads — and `lib.rs` is 78.75% rather than the 77.85% recorded beside it.)
Every one of
them fell on 2026-08-14 without a line of implementation changing, when the
test modules moved out of the files they test — see "Why the Rust per-file
floor exists" below. What 57
catches is a module that stops being tested *at all* — see "Why the Rust
per-file floor exists" below, which is the arithmetic that fixes the number.

**Lines only on the Rust side, and the other three metrics are refusals rather
than oversights.** `branches` is `count: 0` on all 60 files — llvm-cov emits no
branch data without a nightly flag, verified by counting: zero files report a
non-zero branch count — so any floor on it would fail every file. `regions` and
`instantiations` read far below lines on `match`-heavy and generic code (lowest
measured 2026-08-14: 72.22% regions on `taiyaku-serve/src/truth.rs` and
**25.00%** instantiations on `stump/cache.rs`), so they would need their own
numbers for no signal the line floor does not already give.

The Rust figure is the workspace total with the files below ignored;
without them it read **71.46%** lines on 2026-08-14, when eleven were ignored,
and the difference is not work anyone did
— it is code nobody intends to run in CI, which is the whole reason the
exclusions exist. `taiyaku-cli/src/main.rs`, this branch's long pole, went from
43.79% to **95.82%** regions and 97.23% lines when it was measured; its test
module now lives in `taiyaku-cli/src/tests.rs` and the file reads 95.83% lines,
1,012 of the 1,056 that remain.

**Two points, rather than one or five.** v8 counts what the run loaded, so an
unrelated change to which files a test imports moves the web figures by
fractions; one point would make the gate flap. Five would let a whole module
regress in silence. The margin is a compromise and the regression proofs below
record exactly what it does and does not catch.

### Why the branches margin narrowed, and what was actually under it

**Nothing regressed.** Measured 2026-09-12, by splitting the report against
`git log`: the 66 files that were in the tree on 2026-08-14 measure **93.96%**
branches — the 93.99% recorded for them then — and the 42 that arrived in
between measure **90.91%**, carrying 97 of the 203 uncovered branches on 38% of
the branch mass. No web source file was deleted or renamed in that window, so
the two sets are exactly the old tree and the new work. Had the 42 landed at the
tree's own rate the total would still have read 93.96% and the margin 1.96. The
whole 1.19-point slide is dilution by new code arriving three points below the
tree, which is a thing a gate should notice and this one could not: the
aggregate was green at every commit on the way down.

**Half the uncovered branches were three idioms, and the largest was a
duplicate.** Of the 203: **30** were the false arm of
`err instanceof Error ? err.message : String(err)`, at 40 sites across 21 files
— four of which had already grown a private one-line helper for it, three under
the name `messageOf` and one under `message`. 26 were race guards
(`if (cancelled) return`, `if (seq !== latest.current) return`), 35 were `??`
fallbacks and 38 defensive early returns of the `if (!current) return` shape.

**So the fix was mostly to remove branches rather than to test them.** The forty
sites became one `messageOf` in `web/src/message.ts`, tested once: 78 fewer
branch locations, 30 fewer uncovered, and the aggregate moved 92.80 → 93.69
without a single assertion about an unreachable state. A branch that exists only
because forty files spell the same ternary is not coverage the gate wants; it is
a subtraction no test anybody would write could pay back.

**The remaining nine were bought honestly, and the choice of which nine is the
point.** The `?? fallback` and `if (!current) return` branches are states the
code refuses, and a test for one asserts that an impossible thing is handled —
the definition of the theatre this gate exists to avoid. The race guards are the
opposite: each is reachable the moment two reads are in flight, and taking one
out changes what a person sees. Nine were pinned (`Users.tsx` ×2, `Library.tsx`
×2, `Series.tsx` ×2, `RuleEditor.tsx` ×2, `Search.tsx`), every one of them
hand-mutated and watched to fail — see `docs/notes.md` for the list. The
**`cancelled`-on-unmount** guards were deliberately left alone: React 19 warns
on nothing when state is set after unmount, so a test for one would pass with
the guard removed, which is a test that passes against its own defect.

**Lines, not regions, for Rust.** llvm-cov reports regions below lines on
`match`-heavy code — 90.65% against 93.64% here, re-measured 2026-08-18 (it
read 92.27% against 93.95%, from the same stale report as the table above) —
so gating both would mean two
numbers to maintain and re-derive for no additional signal. One gate, on the
metric whose meaning a reader guesses correctly.

### The ignored Rust files, named

`ci.yml`'s comment used to say these were "named in `docs/ci.md`". They were
not — the claim was a forward reference to a section nobody had written, false
from the day it landed until this one. Here they are, with a reason each:

| ignored | reason |
| ------- | ------ |
| `taiyaku-cli/src/geometry_experiment.rs` | a gated one-off measurement module; it fingerprints roughly 1.4 million crops when it runs, and a gate run was once killed at 1h09m inside it |
| `taiyaku-cli/src/region_experiment.rs` | the same: a gated one-off measurement, not a code path the reader takes |
| `taiyaku-cli/src/truth_population.rs` | the same: a gated one-off measurement, reading 480 hand-drawn boxes over 45 pages to derive `detect.rs`'s size band, ink floor and smear gap rather than guess them |
| `taiyaku-cli/src/model_experiment.rs` | the same: a gated one-off measurement, scoring phase 3's model candidate against the same 45 pages for a head-to-head table a person reads — asserts only that it measured something, never which detector wins |
| `taiyaku-cli/src/band_sweep.rs` | the same: a gated one-off measurement, scoring eight component-size bands over those 45 pages so `BAND` is read off a table rather than argued for |
| `taiyaku-cli/src/book_experiment.rs` | the same: a gated one-off measurement, aligning whole books at sixteen settings and scoring each against hand-made keys, for the tables in `docs/notes.md` |
| `taiyaku-cli/src/match_experiment.rs` and the files under `match_experiment/` | the same: a gated one-off measurement, scoring whether page fingerprints can match manga and comics editions by content, for the section in `docs/notes.md`; its regex entry, `match_experiment[/a-z_]*`, matches the submodule files too, so one row covers them all |
| `taiyaku-cli/src/borrow_frequency.rs` | the same: a gated one-off measurement, walking the corpus to count how often a reader meets a gap and who lends the page — added to the regex in `ca20d05` and to this table on 2026-08-14, having matched in between without being listed |
| `taiyaku-cli/src/corpus.rs` | cannot execute without `TAIYAKU_CORPUS`, which CI does not have |
| `taiyaku-serve/src/corpus.rs` | the same |
| `taiyaku-library/src/corpus.rs` | the same — added by the library scanner branch (the standing scan/derive check over real material) and matched by `/corpus` from the day it landed, listed here on 2026-08-18 having been exempt without being named, exactly as `borrow_frequency.rs` was |
| `taiyaku-read/src/corpus.rs` | the same — added with `taiyaku_read::read_book`, the books work's standing check that every EPUB under `$TAIYAKU_CORPUS/books/` reads, and matched by `/corpus` from the day it landed |
| `taiyaku-cli/src/score_corpus.rs` | the same |
| `taiyaku-serve/src/test_support.rs` | `#[cfg(test)]` scaffolding, and a third category rather than a gated module: cargo-llvm-cov's default exclusion drops a file whose basename contains `tests` and so misses this one, for want of an `s` |
| `taiyaku-serve/src/stump/http/test_support.rs` | the same |
| `taiyaku-read/src/test_support.rs` | the same, and the first of the four that is not only `#[cfg(test)]`: `taiyaku-read`'s `test-support` feature exposes it to `taiyaku-library`'s tests, since O104's strace harness serves `stamp` as well as `atomic`. Since O117 `taiyaku-serve` and `taiyaku-cli` enable it too, for its `TempDir`. Matched by the existing alternation from the day it landed, and listed the same day |
| `taiyaku-library/src/test_support.rs` | the same, and like `taiyaku-read`'s also behind a `test-support` feature, which `taiyaku-serve` and `taiyaku-cli` enable for their tests: one copy of each fixture that files a directory planted by SQL under a library as the add would. Matched by the existing alternation from the day it landed, and listed before its branch merged |

**The regex matches `/corpus.rs` by path segment, and that is the sharp edge.**
A *new* `corpus.rs` added to a crate that is **not** corpus-gated would be
excluded silently, and its coverage would leave the number without leaving a
trace. Nothing like that exists today — verified, not assumed, and **re-measured
on 2026-08-14 from `cargo llvm-cov report --json` with and without the flag: 54
files without the regex, 43 with it, and the eleven that disappear are exactly
the eleven above.** *The table has since grown to fourteen — `taiyaku-library`'s
`corpus.rs` on 2026-08-18, `taiyaku-read`'s `test_support.rs` with O104 and
`taiyaku-library`'s with the overlapping-libraries PR 3 — then to sixteen with
the books work's `taiyaku-read` `corpus.rs` and `book_experiment.rs`, then to
seventeen with content matching's row, the one row that names more than one
file (`match_experiment.rs` and every file under `match_experiment/`). The
2026-08-14 figures in this paragraph are left as measured.* The workspace reads **71.46%** lines without the regex
(7,573 of 10,598) against **93.95%** with it (6,735 of 7,169), both
re-measured the same way.

Both pairs of counts are roughly 40% of what this paragraph carried on
2026-08-12, and no test was deleted: 2026-08-14 is the day the `#[cfg(test)]`
modules moved into their own files and left the report. The *gap* between the two
figures widened from 8.91 points to 22.68, which is the same effect read from
the other side — the ignored files are nearly all test-driven measurement
modules, so removing test code from the report left them looking far worse than
the code that stays.

*Why the raw counts are here and the percentages alone are not.* An earlier
pass of this section carried 88.62% and 97.53%, taken from profile data one
commit older. Nothing was wrong with either number when it was written, and a
reader re-measuring today would get neither — which is indistinguishable from
a stale document. The counts make the claim checkable: divide them.

**This table said "four" while listing six, then listed eight while nine
matched, and that is worth recording rather than quietly correcting.**
`band_sweep.rs` and `score_corpus.rs` were in the regex and had never been in
the table; `truth_population.rs` and `model_experiment.rs` were added on
2026-08-12 without the paragraph below them being re-read; `borrow_frequency.rs`
entered the regex with `ca20d05` and reached this table on 2026-08-14, found by
counting the two reports rather than by reading the list. So a count written
once as a measurement has now drifted four times while the list beside it
stayed authoritative — the same lesson `scripts/status-test-count.mjs` exists
for, arriving in the one document that explains that script. **A per-site list
is checkable; a count is not** — and the fourth drift shows the converse edge:
the list is only authoritative if something puts new entries *into* it, which is
why the diff below is worth running when a file joins the regex and not only
when a crate joins the workspace. The seventeen rows above are the list, one of
them naming every file under `match_experiment/`; every number
in this paragraph is derived from them and was re-measured, not carried
forward.

**That check is now half a gate rather than wholly a ritual.** This paragraph
used to end "re-run that diff when adding a crate", and asking a reader to
remember a thing is the weakest form of a check there is. The per-file floor
script now asserts the file count itself — 82 on 2026-09-13, 40 required — so a
regex that quietly swallows far more files than these rows name fails the job instead of waiting to
be noticed. Only *half*, because the automated half sees the count and not the
names: a regex that dropped one file while a new crate added one would net to
zero and pass. So the diff is still worth running when adding a crate, and it is
now a confirmation rather than the only thing standing between the exclusion list
and silent growth:

    env -u TAIYAKU_CORPUS cargo llvm-cov --workspace --all-features --summary-only > /tmp/all.txt
    env -u TAIYAKU_CORPUS cargo llvm-cov --workspace --all-features --summary-only \
      --ignore-filename-regex '(band_sweep|book_experiment|borrow_frequency|geometry_experiment|match_experiment[/a-z_]*|model_experiment|region_experiment|truth_population|score_corpus|/corpus|test_support)\.rs$' > /tmp/gated.txt
    diff <(awk '{print $1}' /tmp/all.txt) <(awk '{print $1}' /tmp/gated.txt)

Both commands above run the tests. To diff the file lists alone against profile
data that already exists, `cargo llvm-cov report --json --output-path …` twice
costs about a tenth of a second each and rebuilds nothing.

`resolver.ts` is deliberately **not** excluded on the web side. It is untested
because it imports `taiyaku-wasm` as a value and no test loads it — a
testability problem with a fix, not a property of the code deserving permanent
amnesty.

### Why `coverage.include` is set explicitly

This is the single most important line in the web gate, and the shortest proof of
it is this. Set `include: ['nope/**/*.ts']` — a directory that does not exist —
and vitest reports:

    Statements   : Unknown% ( 0/0 )
    Branches     : Unknown% ( 0/0 )
    Functions    : Unknown% ( 0/0 )
    Lines        : Unknown% ( 0/0 )

**and exits 0.** Measured, by doing it. Every one of the four thresholds passed
against nothing at all. A gate whose input is empty does not fail loudly; it
reports success, and a typo in one glob is enough to get there.

The reason is the thing to carry away. **v8 instruments what the run loaded**, so
a file no test imports is not reported at 0% — it is *absent*. The headline then
answers "is the code we test tested?", to which the answer is always yes, and in
the degenerate case there is no code at all and the answer is still yes.

The milder version of the same failure was the first one measured, when the block
landed: the default `include` reported **98.43% over 15 files**, while 50 were
inside the gate and the real figure was **48.13%**. A gate built on the default
would have enforced nothing while reading beautifully.

That is why `include` is set explicitly and why exclusions are named one at a
time with a reason, never as a wildcard. It is also why the gate grew a minimum
file count: no threshold *value* can defend against `0/0`, because `0/0` is not a
number that failed a comparison — it is a comparison that never happened. See
"Why `minFiles` is 161, and was 59 for too long — four times now" below.

**The `.tsx` half of that glob is inert, and was assumed rather than measured
until 2026-08-10.** `include` reads `['src/**/*.ts', 'src/**/*.tsx']`, and the
second entry contributes nothing: vitest's coverage `include` resolves a `*.ts`
pattern across its whole extension list, so `['src/**/*.ts']` alone reports the
same file count and the same figures. Measured by running it, at 57 files then
and re-confirmed at 66, again at 108, and again at 115 since. The entry is kept because it is what a reader expects to see and costs
nothing, but the document should not have claimed it was doing work.

**`--coverage.include` cannot be repeated on the command line.** Passing the
flag twice does not union the patterns — the last one silently wins. This
branch's first baseline measurement came from a run whose reported file list did
not match the flags it was invoked with, and the discrepancy was only caught by
counting files. Set it in `vite.config.ts`, where it is one value, and never
reach for the flag.

### Read the JSON, not the table

`reporter: ['text', 'json-summary']` produces two views and they do not agree on
what a file list is. **The text table omits every file at 100% on all four
metrics.** At the thresholds above it printed **18 file rows against the JSON's
57 files** — 39 fully covered files simply absent, measured by counting both; the
JSON holds 115 files as of 2026-09-19 and the table's omission rule is unchanged.
The gap was recorded as 28 rows against 50 files when this branch began, which
is the earlier reading carried forward rather than a second measurement here.

The totals are identical; only the rows differ. But anyone reading the table as
an inventory — "these are the files in the gate" — reaches a false conclusion,
and that is the same failure mode as the 98.43% above, one layer down. Take
per-file figures from `web/coverage/coverage-summary.json`.

### Proving the gate gates

Both gates were regressed on purpose, and every output is recorded because the
result was repeatedly not what the plan expected. Four proofs follow, one per
check: the two aggregates fire (and one of them says nothing useful when it
does), and the two per-file steps exist because of what the aggregates were
measured *failing* to notice.

**Rust — fires, and says nothing.** Re-measured 2026-08-14 on the tree where
the test module lives in `taiyaku-cli/src/tests.rs`: `#[ignore]` on all **100**
of its tests took `main.rs` from 95.83% to **11.84%** lines and the workspace
from 93.95% to **81.32%**. `--fail-under-lines 93` exited **1**, and restoring
the file returned it to 0.

It read differently while that module was inline in `main.rs`, and the
difference is worth keeping rather than overwriting: 84 tests then, taking
`main.rs` to **2.12%** and the workspace to **82.19%**. `main.rs` bottomed out
near zero because the ignored tests were *in* it, so their own covered lines
went with them; today they are in another file, and the 11.84% that remains is
`main.rs` being exercised by `tests/cli.rs` and by the other modules' tests —
which is a truer reading of what one module's suite is worth. The workspace
figure barely moved between the two (82.19% against 81.32%) because the same
work disappeared from a report a third the size.

Its failure output is worth knowing: **cargo-llvm-cov prints no message at all.**
Counted on that same run: **9,165 bytes to stdout, all of it the ordinary
coverage table, and zero bytes to stderr.** Nothing names the threshold, the
metric or the shortfall. A CI log will show a red step whose output looks exactly
like a passing one, so read the `TOTAL` row against the 91 in `ci.yml`. vitest,
by contrast, names each unmet threshold explicitly.

That is still true of `--fail-under-lines`, but the Rust half is no longer silent
end to end, and this is the run that shows it: with those 100 tests ignored, the
per-file floor step after it printed

    coverage-floor: 1 file/metric pair(s) below the per-file floor:
      crates/taiyaku-cli/src/main.rs  lines 11.84% < 57%

and exited 1 — the file, the metric and the number it missed, in the same format
the web half uses. Both halves of the Rust gate were therefore observed firing on
one regression: the aggregate silently, and the floor by name.

**`cargo llvm-cov --fail-under-file-lines` exists and was rejected, measured
rather than assumed.** The flag was present in 0.8.7, pinned when this was
measured, and is in the 0.9.1 pinned now — `cargo llvm-cov report --help`
lists it alongside `--fail-under-lines`, `--fail-under-regions`
and `--fail-under-functions` — so the obvious question is why a script does work
a flag already does. Run against this tree at `--fail-under-file-lines 80` it
exits **1** having written **7995 bytes to stdout, all of it the ordinary
coverage table, and zero bytes to stderr.** It names neither the file that
tripped it nor the threshold it tripped. That is a red step above a 45-row table
with no indication which row is the reason — the same defect as the paragraph
above, one layer worse, because now there are 45 candidates rather than one
`TOTAL`. Checked in `scripts/coverage-floor.mjs` instead, where the failure can
say what to open first. **The cost of that choice** is a node install in a Rust
container and a script kept in step with two workflows; it was paid because a
gate nobody can act on is a gate that gets disabled.

**Web — fires, but not on everything, and the reason is structural.** Skipping a
whole suite is *not* reliably enough:

| suite skipped | statements | branches | exit |
| ------------- | ---------- | -------- | :--: |
| `viewport.test.ts` | 94.71% | 89.70% | **1** |
| `Annotations.test.tsx` | 97.43% | 91.96% | **1** |
| `alignedPairs.test.ts` | 97.43% | 93.75% | 0 |
| `notes.test.ts` | 97.18% | 92.65% | 0 |
| `pairing.test.ts` | 97.49% | 93.77% | 0 |

Deleting all 59 `pairing.test.ts` cases costs **0.54 statement points**, well
inside the margin. The cause is not a slack threshold: `App.test.tsx` mounts the
whole application, so `pairing.ts` and `notes.ts` are covered *twice* — once by
their unit suites and again as a side effect of the integration tests. Removing
one of the two leaves the other. `pairing.ts` still reported 83.58% statements
with its entire suite skipped.

**So state plainly what the aggregate half catches**: a new source file with no
test at all, the loss of a large area's tests, or a regression in a module only
its own suite reaches (`viewport.ts` at 103 statements is over the line;
`alignedPairs.ts` at 77 is not). It does **not** catch the deletion of a unit
suite whose module `App.test.tsx` also exercises. That is a real limit of a
whole-project percentage and no threshold *value* fixes it. The failure output,
from the `viewport.test.ts` run:

    ERROR: Coverage for lines (96.34%) does not meet global threshold (97%)
    ERROR: Coverage for functions (96.46%) does not meet global threshold (97%)
    ERROR: Coverage for statements (94.71%) does not meet global threshold (96%)
    ERROR: Coverage for branches (89.7%) does not meet global threshold (92%)

*(Those last three lines sat orphaned at the end of the next section until
2026-08-10, detached from the run that produced them. Rejoined here, unchanged.)*

### Why `minFiles` is 161, and was 59 for too long — four times now

The collapse to zero files is not the only failure a file count has to catch, and
picking the number for the collapse alone would have left the interesting one
through. **Deleting one `*` from the include glob is a one-character edit.**
The table is the 2026-09-19 run of such edits, by the whole-branch review of
style pass ③: each edit made, and `web/coverage/coverage-summary.json` read rather
than the text table. The same three rows were first run 2026-08-14 and
**re-run unchanged on 2026-09-11**, and the counts below are the ones the
2026-09-19 tree gave:

| `coverage.include` | files | vitest | at 50 | at 59 | at 101 | at 108 |
| ------------------ | ----- | ------ | :--: | :--: | :--: | :--: |
| `['nope/**/*.ts']` | 0 | `Unknown% ( 0/0 )` on all four metrics, **exit 0** | 1 | 1 | 1 | 1 |
| `['src/*.ts']` | 101 *(was 94, was 52)* | 97.64 / 93.89 / 99.14 / 99.29 — every threshold passed, **exit 0** | **0** | **0** | **0** | **1** |
| `['src/**/*.ts']` | 115 *(was 108, was 66)* | 97.78 / 94.15 / 99.19 / 99.33, unchanged | 0 | 0 | 0 | 0 |

**The middle row is the one that fixes the number, and it has now been the row
that showed the number had stopped working on four separate occasions.** One
deleted `*` drops all 14 files of `src/modules` — `annotations`, `borrow`,
`credits`, `direction`, `fit`, `help`, `layout`, `motion`, `prefetch`,
`readouts`, `registry`, `regroup`, `settings` and `zoom` — out of the gate
entirely, and **it costs less than half a point on every metric**. The aggregate
barely twitches, because it is an average over what remains and what left was
well covered. Nothing in the run is red. The per-file floor sees nothing either:
a file that is absent is a file no per-file check visits. Only a count can see
it.

At `minFiles: 50` the count did not, because the tree had grown from 57 files to
66 and taken the near miss from 44 to **52**, clear of the floor set for it. That
was re-derived to 59 on 2026-08-14. **At `minFiles: 59` the count did not either,
because the tree then grew from 66 to 108 and took the near miss to 94** — so 59
tolerated 49 files leaving the gate rather than the seven it was built for, and
the middle row above passed a second time, on 2026-09-11, printing *"94 files
(floor requires 59), all above the per-file floor"* while 13% of the gate's files
sat outside it. That second staleness is `docs/status.md`'s O98, and the
interval between the two was three and a half weeks.

**At `minFiles: 101` the count did not either, for the same reason a third
time.** Style pass ③'s reader work grew `web/src` from 108 to 115 (`TopBar.tsx`
and `readerTitle.ts`, plus unrelated growth on `main` since 2026-09-11), and took
the near miss from 94 to **101** — exactly the floor set for it, and `101 < 101`
is false, so the middle row above passed a third time, on 2026-09-19, printing
*"101 files (floor requires 101), all above the per-file floor"*. That third
staleness was found by the whole-branch review of style pass ③ (#165), and the
interval since O98 was just over a week.

**At `minFiles: 108` the count did not either, a fourth time.** By 2026-10-28
the clean report held 168 files and the near miss **154**, so 108 tolerated 60
files leaving the gate rather than seven, and the middle row passed a fourth
time, vitest green on all four thresholds and `coverage-floor` printing *"154
files (floor requires 108), all above the per-file floor"*. That fourth
staleness was found in review on the web tests tree's branch.

So 161 rather than 108: seven of margin over the 168 measured, which is enough
for a deliberate exclusion — the `exclude` list gives each file it names a
reason — and not so much that dropping a directory passes. At 161 the middle
row fails, *"lists 154 file(s) for web/src, fewer than the 161 this gate
requires"*, and the clean tree passes at *"168 files (floor requires 161)"*;
the right-hand columns above are the same proof at 2026-09-19's counts.

**161 is derived, not scaled.** Scaling an older floor by the tree's growth
leaves whatever room the ratio happens to give between the near miss and the
floor, a margin nobody measured. The measured derivation is the same arithmetic
as every time before: clean tree 168, near miss 154, and the fourteen between
them split seven and seven. That it lands on the same split as 2026-08-14,
2026-09-11 and 2026-09-19 is structural rather than lucky — the near miss *is*
`src/modules` leaving the gate, and that directory held 14 files then and holds
14 now, so the gap it opens has not widened with the tree, only the tree around
it has. `src/modules` is also the only subdirectory of `src` in the report, so
"drop a directory" has exactly one meaning here.

**A fixed count against a growing tree loosens on its own**, which is the one
thing the original derivation got wrong — it said "growth only ever moves the
count the safe way", and that is true of the count and false of the *gap* between
the count and the number under it. Re-run the three rows when web/src grows by
roughly ten files, and check the property ("the near miss still trips it") rather
than the reading ("the count is still above the floor"). Four times now the
count was above the floor at the moment the floor had stopped working, and the
third time took a more dangerous shape than the others: at 2026-08-14,
2026-09-11 and 2026-10-28 the near miss cleared the stale floor comfortably (52
against 50, 94 against 59, then 154 against 108), but at 2026-09-19 it landed
exactly on the stale floor (101 against 101) — one file's difference from
tripping it, and nothing about a green exit code says which side of that line a
run is on.
**If a deliberate exclusion ever does trip this, that is the check working**:
re-derive the number beside the exclusion rather than widening it on reflex.

### Why the Rust `minFiles` is 72, and a crate check holds the rest

The Rust report held 92 files at this derivation: `taiyaku-serve` 35,
`taiyaku-library` 27, `taiyaku-cli` 16, `taiyaku-core` 8, `taiyaku-read` 4 and
`taiyaku-wasm` 2. The near miss is the smallest crate leaving, which is
`taiyaku-wasm` (92 to 90). A count floor that catches it must sit at 91, one file
under the clean count, and one new file anywhere then moves the clean count to
93 and leaves the floor catching nothing: the drift the web section above
records four times. The old floor of 40 sat 52 files under the clean count
for the same reason and caught no crate.

So the count and the crate are checked separately. `scripts/coverage-floor.mjs`
requires at least one report file under every `members` entry of the root
`Cargo.toml` (read at run time, `dir/*` globs expanded), and names each crate
that has none. `minFiles` is 72, which keeps 20 files of room under the clean
count, as the original floor had (40 against 60), and now only has to catch a
collapse or a regex that swallows far more than it should.

Hand-applied on the clean report: the report with every `taiyaku-wasm` file
removed passed the old script (90 files against 40) and fails the crate check;
the report cut to 71 files with every crate still present fails the count.

### Why the Rust per-file floor exists

`--fail-under-lines` is workspace-aggregate, and an aggregate cannot see one
module go dark. Measured against the clean report on 2026-08-14, under the
floor of 93 that stood until 2026-11-12 — 7169 lines,
6735 covered, 93.95% — the arithmetic is: a module going entirely dark still
passes as long as it accounts for **67 covered lines or fewer**. That is 17 of
the 43 files; the other 26 now turn the aggregate red on their own.

| module | covered lines | workspace after it goes dark |
| ------ | -------------: | ---------------------------- |
| `taiyaku-core/src/lib.rs` | 63 | 93.07% |
| `taiyaku-serve/src/works.rs` | 70 | **92.97%** |

are the largest that gets away with it and the smallest that does not, and the
tail of the first group runs down through `progress.rs` at 19, `serve/truth.rs`
at 14 and `sample.rs` at 12. **Every one of the 17 trips the per-file floor**,
because a dark file reports 0% lines against a floor of 57.

**The margin under the threshold was 68 lines on that measurement**, and that is
worth stating in lines rather than points, because 0.95% sounds like room and 68
lines is not. A branch adding a module of 200 lines at 60% coverage turns this
step red. That is deliberate: the number stayed at 93 while what it measures
narrowed.

*Measured again 2026-08-17, after the library store and its review fixes: 9490
lines, 8834 covered, 93.09% over 53 files, and the margin was **8 lines**.* At 8
lines the smallest file in the report (`taiyaku-serve/src/progress.rs`, 11
covered lines) could not have gone dark without turning the aggregate red, so
the blind spot this whole section exists to describe was, for that one commit,
closed — an accident of the margin and not a property of the check, said at the
time to reopen the moment coverage rose.

*It rose, and it reopened. Measured at the head of the library-store branch:
9665 lines, 9053 covered, 93.67% over 54 files, and the margin **64 lines**,
with 20 of the 54 at or under it.* That is the state the rest of this section
describes: any one of those 20 could go entirely dark with the aggregate green.
Every figure in the rest of it is quoted from the 2026-08-14 report and is kept
as the derivation it is — the arithmetic is what the section is teaching, and it
does not change.

**The current figure is derived, and this is the only place it is written
down.** `scripts/coverage-floor.mjs` computes the margin — `covered - ceil(91%
of total)` — and the list of files at or under it from the report it already
walks, and prints both on every run, so the live answer is one `check-all.sh`
away and never needs re-deriving by hand. The paragraph above is a **dated
measurement**, which is a record rather than a claim about now; the three other
places that carried the same figures as present-tense facts — `docs/status.md`,
`scripts/check-all.sh` and `coverage-floor.mjs`'s own header — now point here and
at that printed line instead. Four hand-maintained copies of one measurement is
the drift this repository has already been bitten by twice, in this figure and
in `docs/status.md`'s test count, and the second of those is why
`scripts/status-test-count.mjs` exists (delta review M9).

The 91 that arithmetic depends on is mirrored into `coverage-floor.mjs` and
**checked against the `--fail-under-lines` in `check-all.sh` on every run**,
exiting 2 if the two ever disagree — a mirror nobody checks would just be a
fifth copy of the same problem.

**About five of those lines are noise, and the noise has a name.** Two runs of
the identical command on the identical tree gave 6735 and 6740 covered lines —
93.95% and 94.02%. The whole difference is `taiyaku-serve/src/cover.rs` lines
257-260, `Semaphore::acquire`'s `while *available == 0 { … wait … }`, which is
covered when two cover decodes actually collide on the permit and uncovered when
the scheduler does not overlap them. Every figure in this section is quoted from
the lower run, so the honest margin is **67 lines**. This is the one place the
Rust gate is not reproducible, and it is 0.07 points against a 0.95-point
margin. *(The semaphore has since moved to `taiyaku-serve/src/limit.rs`, with
O57 on 2026-10-12, where a password verify waits on it as well as a cover
decode. The two runs above are from before that.)*

The two-point rule was not re-applied on top of that, because its stated reason
above is v8 counting what a run loaded — a different and much larger
instability than four lines behind a mutex. While the floor was 93, this
paragraph named 92 as the number to fall back to if the gate started costing
honest branches their afternoon rather than catching untested ones. Neither
the rule nor that fallback set the 91 that stands now: it is the owner's
ruling, under "Why the Rust floor is 91, and the toolchain is pinned" below.

**This section read very differently before 2026-08-14, and no test changed.**
The report then held 19486 lines at 95.75–97.40%, because every inline
`#[cfg(test)] mod tests` counted its own executed lines as coverage of the file
it was testing. The blind spot was **413 covered lines** on the figures this
document was first written against and **857** by the time it was re-measured,
with 39 of 44 files inside it — `taiyaku-read/src/pages.rs` at 406 covered lines
could go entirely dark and land the workspace at 93.05%, green. Moving the test
modules into their own files took them out of the report (cargo-llvm-cov 0.8.7
excludes any source file whose basename contains `tests`), and the two
`test_support.rs` modules that survived that by spelling were then named in the
regex, so the same `pages.rs` regression now lands at 89.85% and fails. The
number in the gate did not move; what it measures did.

That is the division of labour, stated plainly: `--fail-under-lines` catches the
workspace sliding; it structurally cannot catch one module being switched off.
The floor is loose — it will not notice 95% becoming 80% — and it does not need
to be tight to catch the thing the aggregate cannot see at all.

**One implementation detail is recorded because it is exactly how a check gets
silently defeated.** On a collapsed report istanbul writes `"pct": "Unknown"` — a
**string**, not a number. Coerced for comparison that is `NaN`, and `NaN < floor`
is `false`, so a naive numeric comparison would have *passed* such a file rather
than failing it. `coverage-floor.mjs` therefore skips every non-number rather
than comparing it, and detects the istanbul format on `total.lines.total` — a
count, `0` on a collapsed report — rather than on `total.lines.pct`, which is the
field that turns into a string in precisely the case the gate exists for. The
first attempt sniffed `pct`, and it failed the collapsed report for the wrong
reason: not "this report is empty" but "this is not an istanbul summary".

### Why the Rust floor is 91, and the toolchain is pinned

**Rust 1.100.0 changed what a line's coverage counts, not what the tests run.**
Measured 2026-11-12 on one commit, `main` at `28b18b60`, with the same tests and
the same commands on two toolchains:

| toolchain | lines | covered | lines covered |
| --------- | ----: | ------: | ------------: |
| 1.99.1 | 31158 | 29359 | 94.23% |
| 1.100.0 | 29822 | 27519 | 92.28% |

CI ran whatever `stable` was then, so the second reading failed
`--fail-under-lines 93` on every pull request.

The cause is upstream: rust-lang/rust#161517, "coverage: Switch over to hybrid
HIR-aware coverage spans" (<https://github.com/rust-lang/rust/pull/161517>),
milestone 1.100.0. A method name no longer gets a coverage region of its own,
so a line that ran, such as `crates/taiyaku-library/src/librarydoc.rs:310`,

    .map_err(|e| format!("reading work {work_id}: {e}"))?;

is credited only with its closure's regions and its `?`'s, and all of them are
0 on a run where the call succeeds. That line ran 609 times and reads uncovered.
Crediting lines like it back gives **94.21%**, so the drop is a change in what
is counted, not testing lost.

**The owner's rulings.** The floor moves from 93 to 91, which keeps about the
margin 93 had under 1.99.1. `rust-toolchain.toml` pins `channel = "1.100.0"`,
so a rustc release no longer reaches the gate on its own, and CI's
cargo-llvm-cov moves from 0.8.7 to 0.9.1, the version the local gate runs. The
`rust:1-bookworm` image tag still floats; the pin decides the compiler whatever
the image ships (see "Toolchain note").

**Bumping the toolchain is a pull request of its own.** Measure the same commit
on both toolchains, with the same tests and the same commands, then move the
pin. Re-set the floor only if the counting changed, as it did here.
`scripts/check-all.sh` records the rustc it measures with beside
`target/llvm-cov-target`, in `target/llvm-cov-target.rustc`, and refuses to
measure over that folder while the record names another rustc, naming the
folder to delete and both versions; it deletes nothing itself. A folder with no
record is trusted, and a `cargo llvm-cov` run by hand is not recorded, so
measure the other toolchain in a folder of its own
(`CARGO_LLVM_COV_TARGET_DIR`).

## The other half: `scripts/coverage-floor.mjs`

**One script, and it runs in both gates.** It is the per-file half of the web
gate *and* the per-file half of the Rust gate — the same file, invoked twice with
different arguments, once by the `web` job over
`web/coverage/coverage-summary.json` and once by the `test` job over
`target/rust-coverage.json`.

An earlier draft of the section above rejected a per-file threshold "at the cost
of 57 numbers to maintain". That objection is right about per-file *values* and
wrong about per-file *floors*: **one floor per metric, applied to every file, is
one number per metric.** Five in total across both languages, each two points
below its metric's lowest honest figure on a clean tree, plus two file-count
minimums:

| format | metric | floor | derived from | lowest now (2026-09-12) |
| ------ | ------ | ----- | ------------ | ----------------------- |
| istanbul (web) | statements | 91 | `Labeller.tsx`, 91.85% | `Shell.tsx`, 92.85% |
| istanbul (web) | branches | 78 | `api.ts`, 79.41% | `Shell.tsx`, 81.25% |
| istanbul (web) | functions | 91 | `decode.ts`, 93.33% | `decode.ts`, 93.33% |
| istanbul (web) | lines | 92 | `api.ts`, 93.02% | `users.ts`, 95.00% |
| llvm-cov (Rust) | lines | 57 | `taiyaku-serve/src/lib.rs`, 74.48% | unchanged |

| format | files measured | `minFiles` |
| ------ | -------------: | ---------: |
| istanbul (web) | 169 | 161 |
| llvm-cov (Rust) | 92 | 72 |

**A fifth column, because the fourth stopped being true and nothing said so.**
The floors were derived from the fourth; the fifth is what a run measures today,
and the two came apart quietly. On 2026-09-12 the web **branches** margin here
was **0.78** — `Users.tsx` at 78.78% against a floor of 78, one uncovered branch
away from a red gate — having overtaken the statements margin that
`coverage-floor.mjs`'s own comment named as the one to watch. It was found by
re-deriving this table rather than by any run, which is the same failure mode
the aggregate rows above have had twice. O103's guard tests moved it to 3.25.

**The count is checked ahead of the floors, deliberately.** If the report
collapsed then every per-file finding under it is noise and the count is the real
story, so that failure is reported on its own and names the config to open first
— the `include`/`exclude` lists for the web format, the `--ignore-filename-regex`
for the Rust one.

**Two report formats, detected by shape rather than by filename**, since the path
is an argument and could be anything. istanbul is recognised on
`total.lines.total` and llvm-cov on `data[0].files` being an array; a report
matching neither exits **2** rather than passing, because a gate that cannot read
its own input must not report green. The two adapters differ in exactly three
places, and each difference is a measured property of the tool rather than a
style choice:

- **How to enumerate files** — istanbul keys the object by path with a `total`
  entry mixed in; llvm-cov nests them under `data[0].files`.
- **What an empty metric looks like.** istanbul writes `"pct": 100` for a metric
  with nothing in it on a populated report — `total.branchesTrue` is 0/0 on every
  run here — and the string `"Unknown"` on a collapsed one. llvm-cov writes `0`,
  not 100, for the same situation. Both were measured. So the web adapter skips
  every non-number and the Rust one skips every zero `count`; swapping the two
  rules would make one gate blind and the other all false alarms.
- **Which config a reader should open first** when the count check fires.

**One file rather than two, and the alternative it beat.** A sibling script was
the obvious other shape. This is one file because the two checks are the same
idea twice — same floor comparison, same minimum, same thing to say on failure —
and only the adapter differs. Two files would have meant maintaining that
reasoning twice and leaving two chances to fix a hole in only one of them. The
cost is a `FORMATS` table a reader has to scan past to find the half they care
about.

Its job is not to be tight. It is to notice a module that stops being tested at
all, which is the one thing an aggregate provably cannot see. On the
`pairing.test.ts` regression that every aggregate threshold passed:

    coverage-floor: 3 file/metric pair(s) below the per-file floor:
      web/src/pairing.ts  statements 83.58% < 91%
      web/src/pairing.ts  functions 81.81% < 91%
      web/src/pairing.ts  lines 86.20% < 92%

and on a clean tree it says what it measured rather than only that it passed
(2026-09-12; it read `57 files (floor requires 50)` when this block was written,
and the two numbers moving apart is the whole of what "The other half"'s fifth
column is for):

    coverage-floor: 109 files (floor requires 101), all above the per-file floor — istanbul json-summary

**Why a script and not more configuration.** vitest cannot express both halves.
A glob entry in `coverage.thresholds` is aggregated over the files it matches —
`'src/**': { statements: 91 }` passed with `pairing.ts` at 83.58%, measured — and
`perFile` inside a glob entry is ignored, also measured. `perFile: true` at the
top level *does* apply per file, but then one number serves both roles, and the
aggregate bar of 96 is far above the lowest honest file, so a clean tree would
fail. Both forms were tried before this file was written. On the Rust side the
equivalent flag *does* exist — `--fail-under-file-lines` — and was rejected for
saying nothing when it fires; see "Proving the gate gates" above for that
measurement.

**What none of the four checks can do.** Coverage counts execution, not
verification. Measured on this branch: adding a test that genuinely pinned new
behaviour — proven by deleting the code and watching the test fail — moved
statement coverage by **zero**, because a pre-existing test already executed the
same line while asserting nothing about it. Seven tests that could not fail were
found here, five of them by mutation rather than by reading; the gate would have
caught none of them. It stops coverage regressing. It cannot tell you the tests
underneath mean anything.

### Why component tests are in `jsdom` and not the browser project

`web/` has three vitest projects and the gate runs two of them. The `browser`
project needs a downloaded Chromium, and this CI has no cache to keep one in, so
it would be re-fetched on every run — the same cost reasoning recorded under
"Caching, and why there is none".

The consequence is the deciding one, and it is about the gate's integrity rather
than about speed: **anything measured in the browser project is invisible to the
gate**, so a developer's local coverage number would sit above CI's and the two
gates would disagree about the same tree. A gate must measure what CI can
reproduce. Component tests therefore run in `jsdom`, which is cheap enough for
every push.

What `jsdom` structurally cannot see stays in the `browser` project and stays
outside the gate: it has no layout engine, so `getBoundingClientRect` returns
zeroes, and no real `HTMLImageElement.decode()` — the seam the whole page-turn
mechanism is built on. `check-all.sh` runs those tests locally and names them
as unmeasured when no Chromium is present.

The three projects find their files in `web/tests/`, which mirrors `web/src/`,
and a file's suffix picks its project: `node` runs `*.test.ts`, `jsdom` runs
`*.test.tsx`, and `browser` runs `*.browser.test.tsx`, with
`*.scrollbars.browser.test.tsx` in a second Chromium launch that shows
scrollbars. A test file under `src/` would match none of them and never run,
so `tests/srcHoldsNoTests.test.ts` fails while one is there, naming it.
Coverage measures `src/` alone, so nothing under `tests/` is counted as source.

The first launch's `--hide-scrollbars` hides a scrollbar but not a gutter
that `scrollbar-gutter: stable` keeps. Below 900 px the Series page keeps the
document's gutter while it draws its volume grid (O283), so at 390 px the
body there is 375 px wide while the root's `clientWidth` still reads 390, and
`Shell.browser.test.tsx` measures sideways scroll by the root's `scrollLeft`
at its far edge.

## The one documentation claim a gate can check

`docs/status.md` opens "What is built" with "Five crates, N tests", and repeats
N once more a few lines below. Nothing derived either figure, and both went
stale on **two consecutive branches in one day**: 639 stated against 656
actual, then 656 against 658. Both were caught by hand, and only because
someone happened to check.

That is the fourth place this repository has logged the same failure. The
open-items preamble's count word drifted three times by its own italic
admission; the cache-root design's §6 miscounted its documentation sites, and
its §7 the tests it shipped — the latter in the section immediately below the
one recording count drift as the recurring problem. The conclusion those
entries reached is that **a per-site list is checkable and a count is not**.
This gate is the exception that proves the shape of it: this particular count
*is* mechanically derivable, so it is derived instead of remembered.

`scripts/status-test-count.mjs` reads a static enumeration of the workspace's
tests and compares it against both stated figures, naming the file, the line
and both numbers when they disagree. It runs in the `test` job after the
coverage floor, and in `scripts/check-all.sh` at the same point.

**It reads the run three steps above it**, rather than running anything.
`.config/nextest.toml` sets `junit.path`, so `cargo llvm-cov nextest` writes
`target/nextest/default/junit.xml` on its way past, and this step is one `node`
process over an XML file. The marginal cost is approximately zero.

**It cost 14.3 seconds a push until 2026-08-11, and the reasoning that got it
there is the part worth keeping.** The first version shelled out to `cargo
nextest list --message-format json` for a *static enumeration*, on the argument
that a list is more robust than a run: it reads no environment, runs nothing,
and cannot be perturbed by a failing suite. That argument is true. It was also
not worth 14.3 seconds — `cargo llvm-cov nextest` builds with coverage
instrumentation, so a plain `nextest list` could not reuse those artifacts and
rebuilt the entire workspace. Observed on run 224: `coverage-floor` finished at
16:46:13.74 and the count printed at 16:46:28.00, **14.3 s of a 46.3 s job, 31%
of this job's wall clock**, to check one number in one sentence.

The three objections that justified the expensive version do not survive being
looked at:

- *"A run has two output formats."* In CI there is only ever one. The fallback
  to plain `cargo test` exists only in `check-all.sh`, on a machine missing
  `cargo-nextest`, and that branch now skips this check the same way it already
  skips the per-file coverage floor.
- *"A run's totals move with the environment."* Both callers run the suite with
  `TAIYAKU_CORPUS` removed, deliberately and for unrelated reasons, and CI has
  no corpus at all. The variable cannot reach it.
- *"A run's totals move when something fails."* True, and now **handled** rather
  than avoided: the script refuses to compare when the report shows a failure or
  an error, and says so. A red suite gets one clear message about the failing
  tests instead of a second confusing one about documentation.

Sharing the instrumented artifacts was tried before the design changed, and does
not work: `cargo llvm-cov nextest list` is parsed by llvm-cov as `nextest run`
with `list` as a test *filter*, not as the `list` subcommand, and fails on
`--message-format json`. Reordering the two steps does not help either —
whichever runs first pays the uninstrumented build. That dead end is what made
the cost unavoidable *given the approach*, which is what forced the approach to
change.

**Proved by regression, six ways**, in the discipline the coverage gates
established. A stale figure in `status.md` exits 1 naming the line. A test added
without touching the docs exits 1 naming *both* lines — 659 against 658. A claim
reworded out of existence exits **2**, not 0, because a gate that can no longer
find what it measures must say so rather than pass. A report of a run with
failures in it exits 2 and names the failures rather than the documentation. A
report with zero tests exits 2, the same collapse `minFiles` exists to catch on
the coverage side. A report of the wrong shape exits 2.

**What it does not check, and why.** The same paragraph says "Thirteen of the N
are standing checks gated on `TAIYAKU_CORPUS` — ten in
`crates/taiyaku-cli/src/corpus.rs` and three in
`crates/taiyaku-serve/src/corpus.rs`". All three figures are correct, verified
by hand against the same enumeration: 16 tests are named `corpus::…`, of which
3 are `corpus::tests::…` — unit tests *of* the corpus-path helper rather than
standing checks. 16 − 3 = 13, split 10 and 3 exactly as stated. It is not
gated because the only rule separating the two populations is a name prefix,
and a gate resting on a naming convention starts lying the first time someone
nests a module differently. A check that is subtly wrong is worse than none,
because it is believed.

**And one figure it must never touch.** `status.md` carries a third test
count — "nothing slower than 0.75 s in 639 tests" — inside O39's record. That
is a dated measurement, not a live claim: it says what was true when the
slowest test was bisected. The two live sites are matched on their surrounding
prose for exactly this reason; a looser "a number near the word tests" pattern
would sweep the historical one in and demand it be falsified on every branch.

## The comment guard

**A comment holds the why**: the reason, the invariant, what breaks if the code
changes. That is the user's rule of 2026-10-17. History (what the code used to
do, how a defect was found, a pull request's story) and measurements go to the
docs or are dropped, and a comment may point at a notes section or an O-number.
Trimming the comments that break the rule is a clean-up; this gate keeps them
from growing back.

`scripts/check-comments.mjs` runs last in the `test` job, and in
`scripts/check-all.sh` straight after the test count. Each run is preceded by
`--self-test`, which runs the lexer and all three rules over fixed cases, and
reads a diff from a scratch repository it makes under the temp directory. It
needs node and git and nothing else, so `check-all.sh` runs it whether or not
the coverage tools are installed. In CI it is the `test` job's last step, so
a step that fails before it skips it.

**It lexes before it judges.** Each source file (`.rs`, `.ts`, `.tsx`, `.mjs`,
`.js`, `.css`, `.sh`, the YAML under `.forgejo/`, and the `Dockerfile`) is
split into comment, string and code, so a `//` inside a string or a URL is
not a comment (JSX text aside: see "What it does not see"). It handles
Rust's nested block comments, raw strings and lifetimes, and TypeScript's
template literals, regex literals and JSX `{/* */}`. A line is a comment
line when everything on it that is not whitespace is comment. In shell,
YAML and the `Dockerfile` only a whole-line
`#` is a comment: a trailing one may sit inside a string or a `${#var}`, and
reading it as code errs to the safe side (Docker itself reads a `#` as a
comment only at a line's start). Python, SQL, TOML, JSON, Markdown and other
YAML are not read.

### The three rules

1. **History markers.** A comment on a line the branch adds fails when it
   holds any of these:
   - an ISO date, unless it is part of a file name
     (`2026-10-28-loading-library-design.md`), comes before a `§` (a pointer at
     a notes section, `spec 2026-09-17 §7.2`), or follows `ruling` (`the
     user's ruling of 2026-10-27`, or `rulings of`);
   - a 7–40-character lowercase hex run holding a digit and a letter (a commit);
   - `#` and digits (a pull request), unless a backtick or `&` sits right
     before the `#` (`` `#808080` `` is a colour, `&#123;` an HTML entity), or
     the file is CSS, where `#000` in a comment is a colour;
   - `PR` as a word, `previously`, `originally`, `formerly`, `was tried` or
     `were tried`, `before` followed by `this` or `the` and then `fix` or
     `change`, and `~~`, except on a line whose comment text opens with three
     or more tildes (a fence line) once a leading delimiter followed by
     whitespace is taken off: `/// ~~~` is skipped, `///~~~` is not;
   - `used to`, unless it follows `is`, `are`, `was`, `were`, `be`, `been`,
     `being`, `get`, `gets` or `got` ("is used to sort" is not history).

   Each pattern reads the line's comment text between the comment text of the
   lines just above and below it (a code line's trailing comment included), so
   a pointer that wraps (`spec 2026-09-17` at a line's end with `§7.2` on the
   next, or `rulings of` above its date) is still seen as one. Only a hit that
   starts on the added line fails it.

   O-numbers (`O148`) and `no longer` are allowed: the first points at the
   ledger, and the second usually states an invariant.
2. **The block cap.** A block is a maximal run of whole-line comments, and a
   blank line inside a block comment continues it. A line is counted when it
   holds text: one that is only a delimiter (`/**`, `*/`, `/*`, or a bare `//`
   or `///`) is not, so every comment style gets 8 lines of text. A block
   fails when it has more than **8** counted lines and the branch grew it: the
   counted comment lines added inside it outnumber the counted comment lines
   removed by the hunks that touch it. A new 9-line block fails. Trimming a
   20-line block to 12, or editing one of its lines, passes; adding a line to
   it fails.
3. **The comment share.** For each area (the six crates, `web/src`,
   `web/tests` and `scripts`) the share is comment lines ÷ (comment lines +
   code lines), with blank lines counting as neither, compared with
   `scripts/comment-budget.json` at its 4 decimals. A share above its budget
   fails. A share more than 0.0050 below it fails too, as stale, when the
   branch has fewer comment lines in that area than its base had, so a
   trimming branch locks its gain in rather than leaving room to grow back
   into. A share that fell only because the branch added more code than
   comments passes. A crate with no budget fails until it has one, as does
   `web/src`, `web/tests` or `scripts` left out of the budget, and so does a
   budget area with no lines under it.

### What is exempt

- **Existing comments, from rules 1 and 2.** Both read only the lines a branch
  adds. A line whose comment text the diff also removes somewhere is carried,
  not added: a moved function, a re-indented block, or an edit to the code
  beside an old trailing comment brings nothing new. For the block cap, only
  a removed line the cap counted carries, so a header line moved into a block
  grows it. Rule 3 sees every comment, but against a budget written from the
  tree as it stood.
- **From the block cap's count:** in a doc comment (Rust's `///`, `//!`,
  `/** */` and `/*! */`, TypeScript's `/** */`), fenced code (a Rust doctest)
  between two fence lines, and an `# Errors`, `# Panics`, `# Safety` or
  `# Examples` section, up to the next heading; Rust `//!` module docs; and a
  file's leading header. The header is the file's first block, when only a
  shebang and blank lines come before it. In Rust, TypeScript and JavaScript a
  blank line or an import (`import`, `export {` or `export *`, `use` or `mod`
  with or without `pub`, `extern crate`, an inner attribute) must come after
  it, since a block that runs straight into an item is that item's doc, and
  counts. In shell, workflow YAML, a Dockerfile and CSS the first block is the
  header whatever follows it. A block made only of these passes at any length.
  A fence that never closes exempts nothing.
- **Strings.** A marker in a string is code, which is how the gate's own
  self-test can spell out every marker it checks.

### Raising a budget, deliberately

Rule 3's message gives the exact `"area": value` to write. A branch whose new
comments earn their place raises its area's budget **in the same diff**, where
a reviewer reads the new value beside the comments that needed it. A branch
that trims writes the lower value the stale message gives. A budget is never
raised to leave room for later.

Two trimming branches can each stay inside the 0.0050 margin and together pass
it. A pull request's run sees only its own branch, so the second to land can
pass there and fail as stale on `main`'s push run; writing the value its
message gives clears it.

Rules 1 and 2 have no budget and no allow-list. A comment they fail is
rewritten: the history goes to the docs or the pull request's description, and
the comment keeps the reason, or points at the O-number that records it.

### How the base is chosen

Rules 1 and 2 diff the working tree against the branch's merge base with
`git diff -U0 --find-renames`, and read every untracked file as wholly added,
so they see the same files rule 3 does. Rule 3's stale arm reads the base's
tree too, for the comment lines each area had there.

- **Locally**, the base is the merge base of HEAD and `origin/main`, which is
  how `check-all.sh` runs it.
- **In CI**, the checkout is a `--depth 1` fetch with no history, so the step
  deepens it first. Whenever the run has a target branch
  (`FORGEJO_BASE_REF`, which a pull request's run has), it fetches that branch
  and the pull request's ref at depth 50, then 500, then 5,000, stopping once
  `git merge-base` resolves, and passes `--base refs/remotes/origin/<target>`.
  On a push to `main`, the weekly schedule or a dispatch it deepens the
  checkout by one commit and passes `--base HEAD^`, so a push judges the
  commit it lands. `--deepen 1` counts from the commit checked out, so a push
  that lands on `main` while the job runs cannot leave `HEAD^` missing, as a
  fresh `--depth 2` fetch of the moved branch would. Any other event, or a
  pull request's run without a `FORGEJO_BASE_REF`, fails the step, rather than
  judge one commit of a longer branch.
- **With no base, the gate exits 2**, never 0. An unknown revision, an empty
  `--base`, or a shallow clone too short to reach the merge base each stops it
  with a message naming what is missing, because a diff against nothing would
  pass every comment unread.

A rule failure exits 1; exit 2 is a gate that cannot judge, for want of a
base or a readable budget.

### What it does not see

- JSX text is lexed as code. An apostrophe, a quote or a `//` in it can
  mislabel the rest of that line, and only that line. A backtick reaches
  further: it opens a template literal that runs to the next backtick, and
  every comment line between reads as string, unseen by rules 1 and 2. A
  `/*` likewise runs to the next `*/`, reading the code between as comment.
  Write either as an expression in braces (`` {'`'} ``) instead.
- A trailing `#` comment in shell or YAML (`true # see #212`) is read as code,
  so rules 1 and 2 never judge it.
- A whole-line `#` inside a shell heredoc reads as a comment.
- Two old blocks joined by deleting the code between them make one longer
  block, but no added comment line grew it, so it passes. So does a block
  moved from elsewhere to sit against another: its lines are carried.
- A blank line ends a block, so two paragraphs of 8 `//` lines with a blank
  line between them pass. Joining them would fail separate comments that
  happen to sit a blank line apart.
- History told in words the patterns do not list, and a pull request number
  in a CSS comment. The list was tuned on the tree: a pattern more than one in
  five of whose hits in a sample were legitimate comments was narrowed or
  dropped. Most of the past-tense verbs went that way, since in this app a
  file or a library that was added, removed, moved or renamed is an event the
  code handles; only `was tried` and `were tried` stay.

## Formatting, and the tool that checks it

Until 2026-08-07 the TypeScript half had **no formatting check at all**, which
was measured rather than assumed: a deliberately mangled exported function
(`(   a:number,b:number ):number{`) passed `oxlint`, `tsc -b` and all 414 tests.
The Rust half has always been checked — `cargo fmt --all -- --check` exits 1 on
mangled Rust, also measured — so the gap was one-sided and nothing said so.

`biome` now covers both halves of the `web` job's first step: `pnpm -C web run
lint` is `biome check ./src ./tests`, which is formatting *and* lint in one
command, so the CI step needed no change. `oxlint` is gone, along with
`.oxlintrc.json`.

**The config file is `biome.jsonc`, and the extension is load-bearing.** A
`biome.json` containing comments is silently discarded — biome falls back to its
own defaults (tabs, double quotes, semicolons) and emits **no warning and no
error**. That was measured here the expensive way: the first version of this
config was `biome.json` with its rationale in comments, and every run used
defaults while appearing to work. The A/B is clean — same file, comments
stripped, config applied. Since the rationale is the point of the file, the file
is `.jsonc`.

**Four rules are off**, listed with their sites in `biome.jsonc` itself. The
branch that adopted biome swapped one tool for another and deliberately did not
change what is enforced; each of the four is a rule `oxlint` never applied, so
turning it on would have been a second claim inside a tooling commit. Three of
them stay open questions about taste, and the fourth does not.

**`noArrayIndexKey` is the one that must stay off**, and it is worth knowing
why, because the branch that turned it off originally recorded the opposite —
"the one worth returning to", a possible defect wanting investigation. It was
investigated, and the index key at `App.tsx`'s scroll mode is the *mechanism*
rather than an oversight. Reconciling by position is what keeps `decode.ts`'s
gate alive across a page turn; a key derived from the slot remounts `PageView`
whenever slots shift, and a freshly-mounted `PageView` has nothing decoded.
`renderGroup` says so where it happens, and "The flicker, before and after" in
`notes.md` measured it: **3 of 79 animation-frame samples landed mid-load
without the gate, 0 of 83 with it.** So the rule's fix would undo a measured
improvement.

The rule's own premise fails here too. Its bug needs state that survives a
shift onto the wrong item, and nothing under that key has any: the wrapper
`<div>` holds none, `PageView` resets from `[src, buffered]`, and
`PageAnnotations` re-measures from `[match]` — and that component's own doc
comment already reasons about this key by name. The one visible consequence of
a `skip` toggle in scroll mode is a reused `PageView` showing the previous page
until the new one decodes, which is the designed no-blank-frame behaviour.

Read from the code and the measurements it cites; nobody has driven a browser,
toggled `skip` in scroll mode and watched. That is the limit of the claim.

**One rule is on by name because biome does not enable it and `oxlint` did.**
`.oxlintrc.json` named two rules: `react/rules-of-hooks`, whose biome
counterpart `correctness/useHookAtTopLevel` is on by default, and
`react/only-export-components`, whose counterpart
`style/useComponentExportOnlyModules` is **not** in the recommended set. Both
were checked by probe rather than by reading a list — a file exporting a
component beside a pure function drew nothing until the rule was named. Left at
biome's default it would have dropped coverage on the branch whose whole claim
was that coverage did not change. `tiles.ts` exists because of this rule.

**Two rules are on with inline suppressions carrying their reasons.**
`correctness/useExhaustiveDependencies` is the descendant of the rule `oxlint`
was enforcing, and it carries **11** suppressions. *This said four until
2026-08-10 and nobody revisited it, while this branch's extractions moved
effects into files of their own and added most of the rest.* Seven of the 11
say the same thing: the flagged dependencies are *triggers* rather than closure
reads, and biome's own suggestion — remove them — would leave scroll mode never
following the reader, the filmstrip centring once, and a reopened note never
refocusing. The other four each carry a different reason, which is the argument
for reading them rather than counting them: two in `PageView.tsx` (one
dependency the effect itself sets, where obeying the rule would tear down an
in-flight decode and restart it the moment it settles; one a parent's fresh
closure, where obeying it would fire the effect every render), one in `App.tsx`
where `session` is reached through `setSession`'s updater rather than closed
over, and one in `hooks.test.tsx` where subscribing exactly once is the subject
of the test.
`suspicious/noTsIgnore` is suppressed at file level in `resolver.ts`, where
biome's suggested fix (`@ts-expect-error`) would break the build in exactly the
state the section below describes.

### `cargo fmt` manufactures whitespace on the Rust side, and nothing in the gate can catch it

Found on the library scanner branch, on five strings, three of them messages a
person actually reads. A `format!`/`assert!` argument continued across lines
with a trailing `\` — the idiom every long message in this codebase uses to
stay under the line-length `cargo fmt --check` itself enforces — carries the
next line's leading indentation as literal content unless that indentation is
stripped by hand before the backslash. Merging two such lines, or reflowing
one that already fit, is easy to get wrong in exactly the direction `cargo
fmt` cannot see: it re-wraps the *code* around a string literal's width but
never looks *inside* one, so a run of stripped-then-restored spaces baked into
the middle of a message survives every `cargo fmt --all -- --check` this
repository runs, silently, because there is nothing there for it to disagree
with.

**No line-count or diff check catches this either.** The bug is bytes inside
a string that was always going to pass `rustfmt`'s own formatting rules; only
running the string — printing the message, or asserting on it in a test —
shows the extra space. This is the reason `watch/tests.rs`'s
`the_report_names_the_shortfall_when_the_directory_count_exceeds_the_watch_ceiling`
asserts the exact string produced by `report_line_against` rather than
`.contains()` fragments of it: a `.contains()` check on either side of an
extra space would still pass. Any multi-line message added or edited on this
branch was checked the same way — by calling the function and reading what it
actually produced, not by reading the source and trusting `cargo fmt` to have
left it alone.

## The `web` job

`web` runs on `node:26-bookworm`, not one of the dual-toolchain images below,
and that is the point. Everything the job tests is pure TypeScript: the
reading model sits behind a `Resolver` interface, and only `resolver.ts`
imports `taiyaku-wasm` **as a value**. The one test that loads `resolver.ts`,
`resolver.test.ts`, mocks `taiyaku-wasm` before it does, and the tests that
would reach it through `App.tsx` mock `resolver.ts` or `App.tsx` itself, so
`vitest` never loads the binding. `reading.ts` and `fixtures.ts` import only
`resolver.ts`'s *types* (`import type`), which is enough for `typecheck` to
need the file but not enough to need the value it wraps. Building the binding
here would still mean adding a dual-toolchain image or a `cargo install
wasm-pack` compile to a job that otherwise needs neither — the `wasm` job
below does that work instead, once, in a container that already has Rust.

**What that leaves ungated, stated rather than glossed:** because
`resolver.ts`'s `taiyaku-wasm` import cannot resolve in this job (see below),
`typecheck` checks that file against no real types at all here, so a call in
`wasmResolver` that no longer matches the generated `.d.ts` (a renamed method,
a changed argument) goes green in `web` regardless of what `wasm` catches.
That risk is carried by `pnpm -C web run build` after `./scripts/build-wasm.sh`
locally — the one discipline this project still asks of a human rather than
a gate. Revisit if that discipline slips.

**Node 26 is Current, not LTS, and that was chosen rather than defaulted into.**
Per `nodejs/Release`'s own schedule, 26 goes LTS on 2026-10-28 and reaches EOL
on 2029-04-30; 24 has been Active LTS since 2025-10-28. Taking 26 means some
weeks on a Current release, bought against local and CI staying close — this
development machine runs 26.7.0, and skew between a developer's node and the
gate's is the kind of gap that hides a defect until the forge finds it. That
agreement is not exact for this job, though: `node:26-bookworm` (above) is a
floating tag, so the image can pick up 26.8.0 or later without this document
or `ci.yml` changing, and nothing pins the `web` job to 26.7.0 the way the
`wasm` job's own node install does. Exact agreement holds only there — see
that job's section below.

**Five places name a Node version, and nothing keeps them in step.** This
paragraph said "three" and then, four lines later, called `.node-version` "a
fourth place" — it disagreed with itself, and it was written before the `test`
job grew a node install of its own. Counted afresh on 2026-08-10, by grepping the
tree rather than from memory:

| place | what it says | how it can drift |
| ----- | ------------ | ---------------- |
| `.node-version`, repository root | `26` | read by nothing here; drifts unnoticed |
| `web/package.json` | `@types/node` at `^26.2.0` | a types package tracking the runtime major, by convention only |
| `ci.yml`, `web` job | image tag `node:26-bookworm` | a floating tag: picks up 26.8.0 without any file changing |
| `ci.yml`, `test` job | `26.7.0` **twice** — download URL and symlink target | the two halves can disagree with each other |
| `ci.yml`, `wasm` job | `26.7.0` **twice** — download URL and symlink target | the same |

Five places, **seven literal occurrences**. `.node-version` is read by nothing in
this repository — no document, script or workflow references it — so it is not a
single stated source for the pin, only one more thing that can drift. Bump all of
them together if any one moves, the same discipline `ci.yml` already asks of
`wasm-pack`'s version.

*`ci.yml`'s own comment on the `wasm` job's node step names the same five
places and points here for the count.*

**The `wasm` job was on Node 18 and nothing said so.** It installed node with
`apt-get install -y nodejs`, and Debian bookworm's `nodejs` package is
`18.20.4+dfsg-1~deb12u2` — confirmed against Debian's own package index. So the
only thing anywhere that *executes* the compiled module ran on a different major
from every other job. That was not a decision; it is what `apt-get` happened to
ship. It now fetches a checksummed 26.7.0 tarball, the same prebuilt-binary
approach this job already takes for `wasm-pack`. **Guess, not measurement:** the
larger download should be roughly a wash against `apt-get`'s recorded 20–30s,
but no runner was reachable to time it. Confirm against a real run.

## The typecheck, and the compiler that stopped being JavaScript

`"typecheck": "tsc -b"` is unchanged from the TypeScript 6 era and that is the
point: in TypeScript 7, `tsc` **is** the native compiler. Verified rather than
taken on trust — `typescript@7.0.2` declares exactly one bin, `tsc`, which
dispatches through `optionalDependencies` to a platform package that is a
statically linked Go ELF binary. There is no `tsgo` bin in the package; that
name belonged to `@typescript/native-preview`, the preview channel that carried
the port before 7.0 shipped. The package size says the same from the other
side: 24.3 MB unpacked at 6.0.2 against 2.5 MB at 7.0.2.

Measured against this repository's own `tsconfig.app.json` and
`tsconfig.node.json`, cold each time with `node_modules/.tmp` removed between
runs, TS 7 twice rather than once:

| | wall | CPU |
| --- | ---- | --- |
| TS 6.0.3 | 1.511s | 3.19s |
| TS 7.0.2 (run 1) | 0.164s | 0.93s |
| TS 7.0.2 (run 2) | 0.166s | 0.99s |

A full typecheck in under two tenths of a second invites the suspicion that
nothing ran, so that was checked too: injecting `export const probe: number =
"not a number"` into `web/src` made both compilers fail at the identical
position with the identical code, `TS2322`. Project references, `tsc -b`
build mode, `moduleResolution: bundler`, `verbatimModuleSyntax`,
`erasableSyntaxOnly` and `allowImportingTsExtensions` all work on the native
port.

**One behavioural difference, recorded rather than glossed:** on type errors
TS 6 exits `2` and TS 7 exits `1`. Both are non-zero so no gate changes, but
anything that ever tests for a specific code would break.

**One thing this migration found, unasked.** `pnpm -C web outdated` reported
nothing behind, but `pnpm -C web update --latest` bumped four packages anyway —
`vite`, `@vitejs/plugin-react`, `@types/react` and `@types/react-dom`, each one
patch release ahead of what `outdated` had just called current, all still
inside their existing caret ranges. `typescript`, `@types/node` and the
`taiyaku-wasm` `link:` protocol were checked afterward and confirmed
untouched, since `update --latest` does not distinguish a dependency pinned on
purpose from one merely unpinned. A reader who runs `pnpm outdated` alone to
decide whether an update is needed should not trust a clean result on its own.

## Running the compiled module in CI

Until M5b's close, nothing in CI ever executed the compiled `.wasm` module —
`cargo test -p taiyaku-wasm` exercises `binding.rs` as plain Rust, which
catches conversion drift (a transposed `from`/`to`, a dropped half) but not a
codegen break, and `scripts/smoke-wasm.mjs` was a one-line command nobody but
a human ran. The `wasm` job now closes that: after the two `cargo build`
steps above (unchanged, and still the fast fail-fast check that the core and
the binding compile for `wasm32-unknown-unknown` at all), three more steps
fetch and unpack a checksummed `node` tarball, fetch a prebuilt `wasm-pack`
release binary, build the real npm package with `./scripts/build-wasm.sh`,
and run `node scripts/smoke-wasm.mjs` against it — the same two commands a
developer runs locally, unmodified for CI.

**Why a tarball fetch, not a dual-toolchain image.** Reusing the `wasm` job's
existing `rust:1-bookworm` container and dropping a `node` binary into it is
cheaper than switching to `cimg/rust:1.97-node` or `catthehacker/ubuntu:rust-latest`
(see the image table below): those cost 200 MB–500 MB more and, per that
table's own probe, five minutes of cold image pull. Measured directly on this
development machine, not the runner (whose network this task could not
reach): fetching `node-v26.7.0-linux-x64.tar.xz` (33.5 MB), verifying its
checksum against the value pinned in `ci.yml`, and extracting it took
**1.4–1.6 seconds** across three runs. That is not a runner figure, but it is
well under the 200 MB–500 MB / five-minute cost of the alternative, so the
conclusion — stay in this container — does not turn on closing that gap
precisely.

**Why a curl'd binary, not `cargo install wasm-pack`.** Measured directly:
`cargo install wasm-pack --locked` is a multi-minute compile of a tool this
job only *runs*, never links against. A prebuilt release binary is a better
fit — `wasm-pack-v0.15.0-x86_64-unknown-linux-musl.tar.gz` from the project's
GitHub releases is 2.9 MB and was confirmed to resolve and extract to a
working `wasm-pack --version` before being wired into the workflow, rather
than guessed at. It is a static-PIE `musl` binary, so it runs on the
`bookworm` (glibc) container without needing a matching libc. `wasm-pack
build`'s default mode also downloads `wasm-bindgen-cli` (pinned to the
version `Cargo.lock` names, `0.2.126`) and `wasm-opt` on first use if neither
is already on `PATH`. The `wasm-bindgen-cli` fetch was tested locally with its
tool cache cleared, forcing a real download rather than a cache hit, and it
still built in under five seconds; the `wasm-opt` fetch was not, since this
machine has a system `wasm-opt` that could not be removed without root to
force it. Upstream (`wasm-pack`'s own source and documentation) confirms the
same download-on-first-use behaviour applies to `wasm-opt`, but that half is
inferred rather than measured.

**What was measured versus inferred.** The node tarball fetch (curl,
checksum, extract), the `wasm-pack` download, and a full
`./scripts/build-wasm.sh` run with that download's binary and a cleared
`wasm-pack` tool cache, were all measured directly on this development
machine — fast on its network (1.4–1.6s for the node tarball, above; well
under a second for the `wasm-pack` binary; a few seconds for the full build
including a fresh `wasm-bindgen-cli` fetch) but not representative of the
Forgejo runner's hardware or network, which this task could not reach to
measure directly (no local `docker` access to replicate `rust:1-bookworm`
here, and pushing a probe branch was out of scope).

**The previous total here is retracted, not adjusted.** This section used to
put the job's addition at roughly 30–60 seconds by taking `apt-get install
nodejs`'s recorded ~20–30s as the dominant term and adding the smaller,
measured pieces to it. `apt-get` was removed from this job, not merely slowed
down or sped up, so there is no dominant-cost figure left to build that
total on — and quietly substituting this development machine's 1.4–1.6s
would smuggle a non-representative number into what read as a runner
estimate. So: the small pieces above are measured, the node fetch's shape (one
~33.5 MB download, checksummed, then extracted) is known, and the `wasm`
job's total wall-clock addition on the actual runner is **unmeasured**.
Confirm against an actual run before writing a new figure here; nothing here
has watched this workflow execute on the real runner.

**The `link:` dependency and a fresh clone.** `web/package.json` depends on
`taiyaku-wasm` via `"link:../crates/taiyaku-wasm/pkg"`, and `pkg/` is
gitignored — it does not exist in a clone that has only ever run `git fetch`.
This is verified rather than assumed, and it was re-verified when the package
manager changed rather than carried over: cloning the repository fresh (so
`crates/taiyaku-wasm/pkg/` is absent entirely) and running
`pnpm -C web install --frozen-lockfile` succeeds anyway, creating a dangling
symlink at `node_modules/taiyaku-wasm` without reading or validating anything
at the far end. `pnpm -C web run build`, `typecheck` and `test` then all pass,
because nothing reachable from any of them *requires* that symlink to resolve.
No `optionalDependencies` fallback was needed.

**Why `link:` and not `file:`, which is what npm used.** Two measurements, both
against pnpm 11.3.0. First, `file:` does not survive the fresh clone at all —
it fails with `ERR_PNPM_LINKED_PKG_DIR_NOT_FOUND` where `npm ci` was content to
dangle. Second, and worse, `file:` *with* the target present resolves to a
hard-linked **copy** in pnpm's store rather than a symlink. Shared inodes make
an in-place edit show through, which makes it look harmless; it is not, because
`scripts/build-wasm.sh` deletes and regenerates `pkg/`, and new files get new
inodes. Simulating exactly that, `web/node_modules/taiyaku-wasm` still served
the **pre-rebuild** file. A developer who rebuilt the binding and reran the gate
would have been testing against the previous build with nothing announcing it.
`link:` is a live symlink and has neither problem. It is pnpm-specific and not
resolvable by other package managers, which costs nothing here: `web` is
`"private": true` and is never published.

**`typecheck` walks into `resolver.ts` without needing it to resolve.**
`tsc -b` type-checks every file `tsconfig.app.json` includes, and `resolver.ts`
is one of them — `reading.ts`'s `import type` from it means `tsc` opens the
file regardless of whether anything imports it as a value, so excluding
`resolver.ts` from the project would not stop it being checked. The
`taiyaku-wasm` import at its top is preceded by a single `@ts-ignore`, not a
`@ts-expect-error`: in a fresh clone the module can't resolve and the ignore
suppresses that one diagnostic, but after `./scripts/build-wasm.sh` has run
locally the import resolves fine and there is no error to suppress — an
`@ts-expect-error` would itself fail in that state, whereas `@ts-ignore` is
silent either way. This was verified in both states, not assumed: with `pkg/`
present, `tsc -b` still catches a real type error introduced elsewhere in
`wasmResolver` (confirming the ignore does not blanket-suppress the file); with
`pkg/` absent, `tsc -b` passes cleanly on the same source. This is the same
fact the `file:` dependency section above rests on, extended one file further:
`resolver.ts` degrades to typechecking against nothing, rather than failing
the job, the day its one import can't be resolved.

## Runner requirements

A registered `forgejo-runner` with:

- a label matching `runs-on: docker`
- Docker support, since every job but `docker` specifies a `container:`
- the host's Docker socket in the default job container (`docker_host:
  automount` in the runner's config), which the `docker` job builds with

The label was `ubuntu-latest` until it was measured. git.daveynet.xyz's runner
advertises `docker`, so every job asked for a label nothing provided and sat in
`waiting` until a later push superseded and cancelled it — sixteen runs, none
of which ever recorded a start. Nothing in the workflow reports this: a job
matching no runner is queued, not failed.

If the runner advertises a different label, change `runs-on:` in `ci.yml` to
match. Check the instance's registered runners under repository settings if
unsure; note that an instance-wide runner shows in neither the repository's nor
the user's `actions/runners` API listing, so an empty list there is not evidence
that no runner exists.

## Why there are no actions at all

Every job clones with `git` in a `run:` step. No `uses:` anywhere, which is
unusual enough to explain.

`actions/checkout` is a JavaScript action, and a JS action needs a node inside
the job's container to execute it. `rust:1-bookworm` carries no node —
`command -v node` exits non-zero — and this runner does not inject one. A job
combining `container:` with any JS action therefore dies before its first step.

That was measured rather than assumed, because the symptom names none of it:
all three jobs failed inside twenty-five seconds with no output worth reading.
Probe jobs settled it — a container with no action passes, an action with no
container passes, and both together is the only failing combination.

The consequence to keep in mind: **no action can be added to these workflows
while the image is `rust:1-bookworm`**. Caching, artifact upload and the rest
are all JavaScript actions. The escape hatch, when one is wanted, is an image
carrying both toolchains; two are measured working below.

## Caching, and why there is none

Every job compiles from cold. `Swatinem/rust-cache` was in the first version of
this workflow and was removed with the rest of the actions, and no replacement
was added, because there is no such thing as a cache action that is not
JavaScript — `actions/cache` and every Rust-specific wrapper alike.

The measurement says not to care yet, but it is older than the pipeline it
describes. On a green run the **three** jobs of the time took 51s, 39s and 17s,
and they run in parallel, so the pipeline was under a minute wall-clock.
Caching would save perhaps twenty or thirty seconds of dependency compilation
against real added complexity. Revisit when a single job passes a couple of
minutes, or when artifact upload is wanted for the `verify` reports.

**Two things have been added since those figures were taken, and neither has
been timed on the runner.** The `web` job is a fourth job that did not exist
then, and the `wasm` job has grown three steps — the node tarball fetch, the
`wasm-pack` fetch and the package build plus smoke run — with no total
estimate above any more (the figure that used to stand there leaned on
`apt-get`'s cost, which this job no longer pays; see above). So the 17s
figure is certainly low and the "under a minute" conclusion is no longer
measured, only inherited. It is left standing because the *decision* it supports
— do not add caching yet — does not turn on the difference, and replacing a
stale number with a guess would be worse than marking it stale. Take fresh
figures before using this paragraph to argue anything.

Two options exist that do not need an action at all, neither yet tested:

- **Named Docker volumes** on the job container, mounting `/usr/local/cargo` and
  the target directory across runs. Pure workflow YAML. The risk is that
  `act_runner` gates mounts behind `container.valid_volumes`, which denies by
  default, so it may need a runner config change.
- **Installing a throwaway node as the first step**, via the container's own
  package manager rather than a pinned tarball — this path needs *a* node, not
  a specific version, so the fastest available install is fine. JavaScript
  actions then work for the rest of the job, since the runner only needs a
  node when it reaches the action. That install costs most of what caching
  would save.

## Images carrying both Rust and node

Measured on this runner, each by running `actions/checkout` — the step that
fails on `rust:1-bookworm` — followed by `node --version`, `cargo --version` and
a real `cargo fmt --check`:

| Image | Compressed | Result |
| ----- | ---------- | ------ |
| `cimg/rust:1.97-node` | 0.77 GB | works |
| `catthehacker/ubuntu:rust-latest` | 1.05 GB | works |
| `mcr.microsoft.com/devcontainers/rust:1-bookworm` | — | no node |
| `rust:1-bookworm` (control) | 0.57 GB | fails, as predicted |

The control is the point of the table: it failed the same way the real jobs did,
which is what makes the two passes mean something rather than being a guess that
happened to work.

**Not switched, deliberately.** The only thing an image change buys today is the
caching described above, and that is not worth paying for yet. The costs are
real: `cimg/rust` adds 200 MB and runs as a non-root user, and the probe run
spent five minutes pulling these images before any job started, which is what a
cold layer cache costs on this runner. `catthehacker/ubuntu` is the closer match
to a GitHub-style runner if the deciding factor ever becomes action
compatibility rather than size.

Note that neither image removes the toolchain download described below unless
it ships exactly the version `rust-toolchain.toml` pins.

## Why the clone is authenticated

The instance requires sign-in for all content, so an anonymous fetch is refused
even though this repository is public — the same setting that makes the REST API
reject unauthenticated calls. Without credentials the clone fails with
`could not read Username for 'http://forgejo:3000'`.

The runner supplies `FORGEJO_TOKEN` for this. Note that `FORGEJO_SERVER_URL` is
the runner's own view of the forge, an address on its container network rather
than the public hostname — an unauthenticated clone fails against
`http://forgejo:3000`, not against `git.daveynet.xyz`. The workflow derives the
clone URL from the variable instead of hard-coding either name.

## The image job

`docker` runs on `main` only, and only once `lint`, `test`, `default-features`,
`wasm` and `web` have passed: on a push, on a dispatch, and every Sunday at
10:45 UTC (03:45 PDT / 02:45 PST). The weekly run and a dispatch on `main` are
refreshes, there so a week with no merge still ships patched Debian packages; a
dispatch on another branch runs the five checks and skips this job. A refresh
builds with `--no-cache-filter runtime`, which reruns the `runtime` stage's
`apt-get upgrade` and `apt-get install`; the job's isolated builder resolves
the base images on every build, so the compiled stages stay cached only while
those are unchanged. It is never a required status: branch protection on `main`
names only `(pull_request)` contexts, and this job never runs on a pull
request.

**It sets no `container:`.** It needs a Docker CLI and the host's daemon, not
a toolchain. The runner's default image has git and curl, and the runner
automounts the host's socket into it. It installs a static Docker CLI matching
the NAS's daemon (29.5.3) and buildx 0.34.0, both by checksum.

**It builds in an isolated builder named `taiyaku-builder`**, a
`docker-container` driver, so nothing it builds is tagged under the registry
name in the host's image store (the throwaway `taiyaku-ci:<sha7>` is loaded
there, and removed). A local `:latest` moving underneath WUD would look to WUD like no
change at all. BuildKit then runs as a container of its own, outside the
runner's `--cpus 4`, so the builder is capped at four CPUs with
`cpu-quota=400000` and `cpu-period=100000`.

**It runs the image before pushing it.** The image is loaded under a throwaway
tag (`taiyaku-ci:<sha7>`) and started. The job waits for its health check to
report `healthy`, and probes `/read/anything` for 200, `/api/me` for 401 and
`/api/nothing-here` for 404 through `taiyaku-probe`. It then removes the
container and the image, and pushes. No other job starts the image, and a
health check that cannot pass would otherwise reach the NAS on a green
pipeline.

**It compiles the commit in, and checks it is there.** Both builds pass
`--build-arg TAIYAKU_COMMIT=` the full commit, which the server stage's
`cargo build` reads through `option_env!` for `/api/about`. The crate's build
script asks git for HEAD, but `.git` is not in the build context, so it finds
no repository and the argument is the only source. The push passes it too
because it rebuilds, and without it would publish a server other than the one
probed. Before the image is removed,
`grep -qaF` over `/usr/local/bin/taiyaku` must find the full commit: a build
that lost the argument still starts and passes every probe. Because the
argument changes with every commit, the server stage's `cargo build` runs on
every push to `main`, a docs-only or web-only one included, rather than being
taken from the layer cache.

**Each probe runs under `timeout 10`**, so a server that accepts a connection
and never answers fails the job quickly instead of holding it until the job's
own timeout.

**`:latest` moves only for the commit that is still `main`'s tip.** The runner
takes two jobs at once and nothing serialises them, so an older merge's job can
finish after a newer one's, and the order in which pushes finish is not the
order of the merges. Before pushing, the job asks the forge for
`refs/heads/main` (`git ls-remote origin`, through the clone's authenticated
remote, under `timeout 30`) and compares it with its own commit. When they
differ, or when the forge cannot be read, it pushes the `sha-<7>` tag alone and
says so in the log. That keeps a stale build from taking `:latest` in nearly
every case; a check followed by a push leaves a small window, in which a newer
merge can land between the two. A refresh tags the same way: it re-pushes the
`sha-<7>` of the commit it built, which is main's tip when the run started,
with patched packages, and `:latest` only if that commit is still the tip.

**Two secrets**, `REGISTRY_USER` and `REGISTRY_TOKEN`, hold a forge account and
a token with package write access. Without them the job fails at
the `Log in to the forge's registry` step, which runs `docker login` after the
build and the probe have passed; nothing is pushed.

**Its timeout is 60 minutes**, unlike the others' 10. Nothing has timed a
cold image build on the NAS, whose i7-9700K is slower per core than the
workstation and carries other load. A first run also pulls three base images
and installs the toolchain twice. Re-size the figure from the first real runs.

## The advisory check

`.forgejo/workflows/advisories.yml` checks the workspace's `Cargo.lock` against
the RustSec database with cargo-deny 0.20.2, a pinned and checksummed release
binary, in the same `rust:1-bookworm` container as the Rust gate jobs (lint,
test, default-features, wasm), on the image's own toolchain. The database is
cloned fresh each run, so this is the one check that can go red with nothing
merged, and that is why it is a workflow of its own: an advisory published on a
Saturday must not stop Sunday's rebuild, which exists to ship fixes. For the
same reason it runs weekly, Mondays 15:00 UTC, and on every push to `main`.
Pull requests run it only when they touch `Cargo.lock`, a manifest,
`deny.toml`, the cargo config or the workflow, so a new advisory does not turn
every open pull request red at once — which is also why it is not a required
status: a pull request that touches none of those never reports it.
`check-all.sh` does not run it: it is not one of the five `ci.yml` jobs the
script mirrors.

`deny.toml` gives the command that checks the same graph locally. How to
accept a finding — an `ignore` entry for one advisory, or the `ADVISORIES_MODE`
repository variable for all of them for a while — is described once, in the
workflow's header.

`spike/Cargo.lock` is not checked: `probe` never ships, and the `tract-onnx`
it drives is checked through `taiyaku-cli`'s dev-dependency.

**This check covers the bundled SQLite.** `rusqlite`'s `bundled` feature
compiles SQLite into the binary, so a SQLite security fix ships by updating
`libsqlite3-sys` in the lockfile, with a `rusqlite` bump when the new release
is outside the range `rusqlite` accepts, and `cargo deny` reports an advisory
against the copy the lockfile holds. The image's weekly refresh does not bring
one: it reruns `apt-get upgrade`, and the image installs no SQLite package for
that to update.

## Why the variables say Forgejo and not GitHub

Forgejo Actions implements GitHub's interface, so its runner exports every value
twice: once under `GITHUB_*`, which is what lets actions written for GitHub work
unchanged, and once under `FORGEJO_*`. Both were measured working on this
instance, and the values are identical — `GITHUB_SERVER_URL` here is
`http://forgejo:3000`, not anything on github.com.

The `FORGEJO_*` set is used because nothing in this project is hosted on or
fetched from GitHub, and a workflow that reads as though it were invites the
question every time someone opens it.

## Toolchain note

`rust-toolchain.toml` pins `channel = "1.100.0"` and requests the `wasm32`
target plus `rustfmt` and `clippy`, which makes a fresh local checkout work with
no setup. It names a version rather than `stable` because a rustc release can
change what the coverage gate counts; "Why the Rust floor is 91, and the
toolchain is pinned" has the measurement and how a bump is done.

The tradeoff is in CI. The `rust:1-bookworm` tag still floats, and the image
installs its toolchain under its own version number. Whenever that version is
not the pin, rustup fetches the pinned toolchain separately on each job, which
costs a download per job; when it is the pin, rustup adds whichever of
`rustfmt`, `clippy` and the `wasm32` target the image lacks. Either way the gate
jobs compile with the pinned rustc: the floating tag moves the image's system
packages, not the compiler. Pinning the image to the same version would remove
the download, at the cost of bumping both together.

**The `lint` job also checks the workspace at the version it declares.** The step
"Check at the declared Rust version" reads `rust-version` from the root
`Cargo.toml`, installs that release and runs `cargo check --workspace
--all-targets --all-features --locked` with it. A two-part value gets `.0`
appended: cargo reads `1.91` as 1.91.0, while rustup resolves the channel `1.91`
to the newest 1.91.x, a newer compiler than the one declared.

It proves that the locked dependency tree accepts that compiler and that every
target of every workspace crate type-checks with it, dev-dependencies included.
It does not prove more than that:

- **The host target only.** The `wasm32` build is not checked at this version.
- **No test runs at this version**, and `cargo check` links no library, binary
  or test of the workspace, so nothing here shows the workspace links or behaves
  correctly on that compiler.

## Mutation testing

`env -u TAIYAKU_CORPUS cargo mutants --gitignore=true --test-tool=nextest
--output <dir>` is the invocation, and every flag on it earned its place by
something breaking first.

**`--gitignore=true` is not optional on this repository.** `cargo-mutants`
copies the whole source tree into a scratch directory before mutating it, and
does not honour `.gitignore` by default. `corpus/` is gitignored and is 26 GB
of NAS-backed material, including archival TIFFs at 430 MB a page, and the
first run without the flag died mid-copy:

    Error: Failed to copy .../corpus/incoming/needles/colorwalk-comp2/cwc2-310.tif
    Caused by: No space left on device (os error 28)

Nothing in that message names the corpus or the ignore file, and the natural
misreading is that the machine itself is short of disk — it is not; the copy
filled a 31 GB `/tmp` before a single mutant was tested. `--gitignore=true`
fixes it by making the scratch copy honour the same file `git` already does.

**Every feature is on.** `.cargo/mutants.toml` sets `all_features = true`:
`taiyaku-read`'s RAR backend compiles only with `rar-libarchive`, so a sweep
without it would build none of that file, and every mutant there would be
missed. So the sweep's container installs `libarchive-dev` beside `strace`,
and a local sweep needs libarchive's development files, as the gate does.

**`env -u TAIYAKU_CORPUS`, for the reason "Two profiles, and the gate that
took an hour" above already gives `cargo test`.** `cargo-mutants` rebuilds and
reruns the test binary once per mutant, so leaving the variable set would run
the standing corpus checks and the one-off measurement modules gated on it —
`geometry_experiment` among them — on every mutant a run tries, rather than
once. The baseline in `docs/notes.md` was taken with the variable stripped.

**Always pass `--output`, pointed outside the working tree.** `mutants.out/`
is gitignored as a second line of defence, not the first — the flag is what
actually keeps a run from writing its scratch copy beside the source it was
copied from.

**This is not a CI job and not a gate.** The numbers needed to size a
threshold — how many survivors are genuine gaps against how many are
equivalent mutants that cannot be killed by construction — did not exist
before the baseline in `docs/notes.md` was measured, and a threshold picked
before its baseline exists is a number chosen to be passed rather than one
that means something. See
`docs/superpowers/specs/2026-08-10-mutation-testing-and-pedantic-design.md`,
§5, for the fuller argument: it is also why no `--in-diff` PR gate ships
alongside this, since such a gate running the CI-visible suite would report
`solve.rs`'s recurrence as unverified while staying silent about the one
corpus check that actually verifies it.

### Hand-mutation, and the two ways it lies

Writing a test against a survivor needs the mutation applied by hand at least
once, because **a new test passing proves nothing** — the code it tests was
already correct, so a test that asserts nothing passes exactly as loudly as one
that asserts the right thing. The only evidence is watching the test go red
against the mutant. The branch that killed `taiyaku-core`'s survivors got a
false answer from that procedure twice, in opposite directions, and both are
cheap to defend against.

**A mutation that did not land reports PASS.** One edit was written by a Python
helper and was invisible to the `cargo` invocation in the same command: no
`Compiling` line, a 0.02 s build, and a green run. Two processes in one command
had disagreed about the file's contents. Read as evidence, that PASS says "the
test cannot kill this mutant, so it must be equivalent" — and that is precisely
the wrong conclusion that shipped in one agent's first report, on a mutant a
reviewer then killed in twenty-five lines. So: apply the edit with `sed -i`,
**`grep` the file afterwards to confirm it is there**, and check the run
actually rebuilt.

**A mutation left applied poisons everything measured after it.** A reviewer
finished a hand-mutation pass, reported the tree restored and `md5`-verified,
and left `carried >= 0` sitting in `sidecar.rs`. It was caught only because a
later `git cherry-pick` refused to run against a dirty tree — two full-workspace
suite runs before that had been measured against mutated source and had
reported green. So: back the file up with `cp` and restore from the backup
rather than trusting an inverse edit, and **check `git status` before believing
any number**. Do not reach for `git checkout` or `git stash` to undo a
mutation if anything else is running in the same tree.

Both failures share a shape worth naming, because it is the same shape the
whole technique exists to expose: a green result that means "nothing was
tested" is indistinguishable from a green result that means "everything
passed", unless something independent confirms the thing under test was
actually there.

## The pedantic lints, and why they are advisory

`clippy::pedantic` is **not** in `[workspace.lints]` and must not be put there.
The blocking gate stays `cargo clippy --workspace --all-targets --all-features
-- -D warnings`, which sees the two lints that table does set — `todo` and
`dbg_macro` — and nothing from the pedantic group.

The reason is drift in the toolchain rather than distaste for the lints, and
clippy adds lints to `pedantic` on roughly a six-week cadence. When this was
decided `rust-toolchain.toml` said `channel = "stable"`, so a pedantic lint in
the blocking gate meant a rustc release turned an unrelated PR red in a file its
author did not touch. The toolchain is pinned now (see "Why the Rust floor is
91, and the toolchain is pinned"), so a new clippy reaches the gate only through
the pull request that moves the pin, and that argument no longer holds as
written. The decision stands as taken. What keeping the group advisory does
under the pin is keep that bump from being held red by the lints its clippy
adds, in files it does not otherwise touch; they show in the advisory step's
output instead.

The mechanism that makes the step advisory rather than blocking is
`continue-on-error: true`, on that one step and nowhere else in `ci.yml`.
Unlike `scripts/check-all.sh`'s `|| true`, which is a shell operator this
repository's own gate exercises on every local run, this key needs the step
it guards to actually fail before it does anything, and nothing local can
force that. The branch has since been pushed and the pipeline came back
green — but a green run does not exercise the key: `-W clippy::pedantic`
exits 0 no matter how many warnings it prints (measured: exit 0, 15
warnings), and only `-D warnings` sets a non-zero exit, so the advisory
step never had a reason to fail on lints. It cannot fail on lints at all,
in fact — its only possible failure is a compile error, which the blocking
step just above would already have caught — so `continue-on-error` and
`|| true` are both belt-and-braces for a failure lints cannot produce
today. The key stays untriggered rather than untested: it has still never
been observed doing its job.

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

That is what stops 40 suppressions decaying into 40 unchecked claims: delete
a cast without deleting its attribute and the gate says so. The cost is real
and accepted — a refactor that removes a cast must remove its attribute in the
same commit. (40 is the count after a same-day campaign fixed 60 of the 100
this step first shipped with — see `docs/status.md`, "`clippy::pedantic` runs,
advisory and non-blocking".)

**Most of the 40 are per-site, and one is not.**
`clippy::similar_names` and `clippy::many_single_char_names` are
function-scoped lints — they judge every binding in a function together, not
one expression — so an `#[expect]` on the `let` that introduces the offending
name does not suppress them; it has to sit on the enclosing `fn`. One of the
40 is a function-level `#[expect]` for exactly this reason: `many_single_char_names`
on `solve` (`taiyaku-core/src/solve.rs`), covering all six of `a`, `b`, `m`,
`n`, `i` and `j` in one attribute rather than a `let` each — the three
`clippy::similar_names` sites the branch found were fixed outright instead of
suppressed, which is why only one function-level `#[expect]` remains for this
reason. `clippy::too_many_lines` (3 attributes) and
`clippy::needless_pass_by_value` (3) are also function-level, but for the
ordinary reason that neither lint has a narrower span to attach to — not the
per-binding nuance `similar_names`/`many_single_char_names` are about.
That is not a breach of the per-site rule in the plan (§10.3): the site clippy
reports *is* the whole function for these two lints, and the attribute matches
it.

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

**The advisory step's own plain-text output is not quite empty, and that is
not a leak.** Every `assert_eq!` comparing floats in this workspace's tests —
there are 15, all pre-existing — prints a `clippy::float_cmp` warning under
`-W clippy::pedantic`, unconditionally, every run. Checked directly against
this workspace's clippy JSON stream rather than assumed: the rendered warning
shows the call site (`crates/taiyaku-core/src/solve.rs:973:9`, and the like),
but the `is_primary: true` span clippy actually attaches the lint to has
`file_name: .../library/core/src/macros/mod.rs` — `assert_eq!`'s own expansion
in the standard library, not any file here. `scripts/pedantic-sites.mjs`
filters on exactly that field, which is why it reports 0 unique sites under
`crates/` even though `.forgejo/workflows/ci.yml`'s advisory step and
`scripts/check-all.sh`'s mirror of it both print all 15, unfiltered, every
time. Nothing to fix: an `#[expect]` here would sit on a real test function
suppressing a warning clippy itself does not attribute to that function, which
is the wrong shape for the mechanism this file already documents above. A
reader watching either gate should expect to see these and read past them —
what is worth reading is anything *beyond* the 15.

## Mutation testing, and why it cannot be a gate

`cargo mutants` changes the source — a `>` becomes a `>=`, a function body
becomes `Ok(())` — and reruns the suite. A mutation the suite still passes
against is a **survivor**: a line the tests execute without asserting anything
about what it does. Coverage cannot see this. A survivor is 100%-covered code
whose behaviour nothing checks.

`.forgejo/workflows/mutants.yml` runs **one crate at a time, dispatched by
hand**, and **gates nothing**. It had a weekly cron until 2026-08-20; the forge's
host is an 8-core machine carrying 68 other containers at a load average around
16, with no CPU or memory limit on the runner container, and a sweep is hours of
build-and-test. A weekly timer degraded that box for hours, unattended, to
re-measure something that moves at the speed of test-writing.

`docs/mutants-baseline.md` carries a dated log of every sweep, because a manual
job with no record is one nobody can tell has lapsed — read it to decide whether
one is due. Two reasons it cannot be a gate, and the second matters more than the
first:

1. **A full sweep is ~8.5 hours of runner time.** 3,191 mutants took 3 h 12 m
   locally on 2026-08-19, and the forge's runner is **3x slower** — measured
   from `taiyaku-cli`'s own log at ~10 s per mutant against ~3.5 s. Spending
   that unprompted on a job that gates nothing is not a trade worth making, so a
   dispatch takes one crate at a time. `scripts/mutants.sh --since main` is what
   catches a regression as it is written; this is the backstop for code nobody
   touched but whose *tests* somebody weakened, and a backstop that reaches each
   crate monthly is still a backstop.

   The figure was 1,660 mutants in 43 minutes in August, before spec B's scanner
   and spec A2's auth. Quoting it forward is what set the timeout to 1.35x the
   real cost under a comment claiming 6x.
2. **A job that is red by default teaches you to skip it.** 92 survivors are
   recorded and untriaged, and `detect.rs` and `regions.rs` are excluded outright
   rather than swept — writing fixtures against a detector about to be replaced
   is work thrown away. A gate would be red on every commit for a correct reason,
   which is the state in which nobody reads it. `plaintextesports`'s `live-smoke.yml`
   made the same call for the same reason.

**A failure means "go look", not "the build is broken."**

### Two more bounds the timeout does not cover

**A time bound is not a memory bound.** A mutant that removes an allocation
guard does not spin — it allocates. Measured 2026-08-19: two sweeps were killed
by the kernel's OOM killer, one test binary at **36.6 GB** anon-rss and another
at **43.8 GB**, on a 60 GB machine. Neither came near the 60-second per-test
timeout, because forty gigabytes takes seconds. Worse, the kernel picks a victim
by score rather than by culprit — the first kill was *invoked by* `kwin_wayland`,
which was simply the next process to ask for memory.

`scripts/mutants.sh` sets `ulimit -v` (16 GB, overridable with
`TAIYAKU_MUTANTS_VM_KB`) before running. It is per-process and inherited, so a
runaway gets an allocation failure, its test aborts, and cargo-mutants records
the mutant as **caught** — which is the right answer, since a mutation that
allocates forty gigabytes has changed observable behaviour and the suite did
notice. Verified both ways: `cargo build --workspace --all-targets` completes
under the cap, and a 4 GB allocation under a 2 GB cap fails cleanly.

A cgroup (`systemd-run --scope -p MemoryMax=`) was tried first and rejected: a
ceiling on the whole run lets one runaway consume the budget and take innocent
concurrent mutants with it, reporting them as caught when nothing tested them.
**False *caught* is the same failure as false green.**

So there are three bounds, for three different runaways: `timeout-minutes` on
the job, `.config/nextest.toml`'s 60 s on a test, and `ulimit -v` on a test's
memory. None subsumes another, and the third was missing until it cost two
sweeps and a desktop session.

### The baseline is what makes the output readable

`docs/mutants-baseline.md` records the survivors that have been looked at and
accepted, each with its reason, and `scripts/mutants-baseline.mjs` diffs a run
against it. **A survivor not in the baseline is the whole of what is worth
reading.** Without that, every run prints the same 92 untriaged survivors and
the one new entry is invisible in the middle of them.

**An entry can also be `UNTRIAGED:`, which is a third state and not a weaker
second one.** The first workspace-wide sweep produced 150 survivors; writing 150
verified reasons in one sitting produces 150 sentences that sound like reasons.
So an untriaged entry is silenced in the new-survivor diff — it is not new — and
counted out loud on every run, because nothing about it has been established.
"Accepted" asserts somebody checked; "untriaged" asserts nothing. Collapsing the
two is how a baseline becomes the place defects go to be forgotten. As of
2026-08-20: **13 accepted, 92 untriaged**, the 45 L2 deferrals having left the file when their two source files were excluded from the sweep.

It is also the only memory this project keeps of what has already been checked.
Four whole-branch reviews of spec A2 each returned fifteen findings, and the rate
never fell — because nothing remembered what the previous round had looked at, so
each reviewer re-sampled the whole branch. Coverage records which lines *ran*.
The baseline records which lines have an assertion behind them and which are
known not to, and it is committed, so the answer survives the session that found
it.

An entry is keyed on the mutant's `file:line:col: description`, verbatim. Moving
a function therefore invalidates its entries and its survivors reappear as new.
That is deliberate: matching on the description alone would be stabler and would
carry an accepted survivor across a rewrite that turned it into a real defect,
with the entry still explaining why the old code was fine.

### Sharding, and the ceiling above which no timeout can bind

`taiyaku-cli` is ~3.4 hours on the runner and **cannot fit in one job at any
timeout setting**. Its first attempt was killed at 3 h 01 m with *"The runner
cancelled the job because it exceeds the maximum run time"* — while
`timeout-minutes: 210` sat above that hard limit and had never come near firing.

**A bound above the ceiling cannot bind**, which is the same shape as the two
figures before it: 260 minutes justified as "six times" a measurement that had
doubled, and 210 justified as "three times" a figure from a machine three times
faster. Each was arithmetic on the wrong number.

So the crates are sharded — `cargo mutants --shard i/N`, one job per shard,
sized so no piece exceeds about an hour: cli 5, serve 3, library 3, and one each
for core, read and wasm. `timeout-minutes` is **150**, deliberately *under* the
runner's ceiling rather than over it. If a shard ever reaches it, the answer is
more shards; there is no bigger number available.

Each shard re-runs the unmutated baseline, so N shards cost a little more than
one run divided by N. That is the price of fitting.

`scripts/mutants-baseline.mjs` compares only the mutants a run actually tested,
so a shard reports its own new survivors and says nothing about the rest. That
property came from fixing `--since` and is what makes sharding work without any
further change.

### Running it on a branch

```
scripts/mutants.sh --since main    # only code this branch touched
scripts/mutants.sh --crate taiyaku-serve
scripts/mutants.sh --file 'crates/**/auth.rs'
scripts/mutants.sh                 # everything — the three hours
```

`--since` is the one to reach for before opening a pull request. It passes the
merge-base diff to `cargo mutants --in-diff`, so it mutates what the branch
changed rather than what `main` gained meanwhile.

### What it cannot tell you

**cargo-mutants does not mutate every expression.** It generated no mutant at
all for `run.b.map(|_| run.a)` in a `match` arm, so O38's defect was invisible to
it both before and after the fix. A file with no survivors is not a verified
file; it is a file with no *known* survivors.

**A non-terminating mutant is reported TIMEOUT, not MISSED.** `left += 1`
becoming `left *= 1` never finishes. `.config/nextest.toml` bounds a single test
at 60 s, which is what keeps that a timeout rather than a hung job — the two
bounds are the same pair CI already reasons about above.

**`const { assert!(...) }` makes a mutant unviable rather than caught**, so
clippy's suggested fix for `assertions_on_constants` silently improves the score
without improving anything.
