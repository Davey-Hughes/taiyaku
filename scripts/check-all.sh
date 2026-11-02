#!/usr/bin/env bash
# The local mirror of CI (.forgejo/workflows/ci.yml). Run it yourself before pushing a branch —
# nothing runs it for you, and the forge runs the same four jobs after the push rather than before.
#
#   scripts/check-all.sh                 # everything CI runs, plus the corpus checks
#   scripts/check-all.sh --no-wasm       # skip the whole `wasm` job
#   scripts/check-all.sh --no-web        # skip the whole `web` job
#   scripts/check-all.sh --corpus-debug  # corpus checks in debug — see "Two profiles" below
#
# The `web` job additionally runs a small **browser** test project that CI does
# not: it needs a downloaded Chromium, which a cacheless CI would fetch on every
# run. It covers what no Node environment can — real layout and a real image
# decode — and is named as unmeasured when the browser is absent.
#
# The point of this file is that the local gate and the CI gate cannot drift: CI's four jobs are
# reproduced here in the same order with the same flags. When a job changes there, change it here.
#
# --- Two profiles, and which one measures what -------------------------------
#
# **Debug: every unit test, and it is the only pass that has overflow checks.** Integer overflow
# panics and `debug_assert!` exist only under `debug_assertions`, and this repository has two of the
# latter that matter — `PageRange::new`'s "page range must not be inverted" and `Run::paired`'s
# equal-length check, both in `taiyaku-core/src/lib.rs`. The `test` job below runs the whole
# workspace in debug and is what exercises them.
#
# **Release: the standing corpus checks.** Those decode and fingerprint real pages, and `image` in
# debug is much slower. They are not a CI job (CI has no corpus), so nothing about the mirror above
# depends on their profile. The first build is slow, since `[profile.release]` here is
# `lto = "thin"` with `codegen-units = 1`.
#
# **The `test` job runs with `TAIYAKU_CORPUS` removed from its environment.** The variable is
# exported at the top of this file, so the job would otherwise inherit it and run the standing
# corpus checks in debug, and the dedicated pass below would then run them again for their output.
# Without the variable the job is also a *truer* mirror of CI, which has no corpus and skips them.
#
# **What the default run does not measure**, named here and again where it happens: the corpus
# checks in release have no overflow checks and neither `debug_assert` above fires, so those two
# invariants are exercised by the unit tests and not over real material. `--corpus-debug` runs that
# pass in debug instead. It is slow — that is the whole reason it is not the default — so it is a
# thing to run when touching `PageRange`, `Run::paired`, or arithmetic on page and slot indices,
# rather than every time.
#
# Anything not measured is named twice — once where it would have run, and again in the closing
# summary, which refuses to print a bare "all green" when something was skipped.
#
# --- what this deliberately does NOT run -------------------------------------
#
# `scripts/mutants.sh`. Mutation testing measures something no gate here can — whether the
# assertions see a defect, rather than whether the lines ran — and it costs far more than this
# file's whole budget. It also has untriaged survivors deferred behind L2, so it is red for a
# correct reason on a clean tree. It runs in `.forgejo/workflows/mutants.yml`, gating nothing, and
# `scripts/mutants.sh --since main` is the version worth running by hand before opening a pull
# request. See docs/ci.md, "Mutation testing, and why it cannot be a gate".
#
# This is the one place the "local gate and CI gate cannot drift" claim above has an exception,
# and it is stated here so that the exception is a decision rather than a gap: `mutants.yml` is
# not one of the four jobs, and nothing waits on it.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# Parse up front so a typo (`--no-wasmm`) fails immediately rather than silently falling through to
# a full run — a flag that quietly does the opposite of what was asked is the same class of bug as a
# gate that quietly covers less than it claims.
no_wasm=0
no_web=0
corpus_debug=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --no-wasm)      no_wasm=1 ;;
    --no-web)       no_web=1 ;;
    --corpus-debug) corpus_debug=1 ;;
    *)              echo "error: unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

run() { echo; echo "==> $*"; "$@"; }

# What ran but measured nothing, collected so the last line of output cannot claim more than was
# done. A skip is loud where it happens *and* repeated at the end, because the line where it
# happened has scrolled past a `cargo test` by then.
skipped=()
note_skip() {
  skipped+=("$1")
  echo
  echo "==> NOT MEASURED: $1"
}

# --- resolve TAIYAKU_CORPUS before anything runs ---
# Absolute, deliberately. `cargo test` runs each package's tests with the
# working directory set to that package's own directory rather than the
# workspace root, so a relative `TAIYAKU_CORPUS=corpus` resolves to
# `crates/taiyaku-serve/corpus` and finds nothing.
if [ -n "${TAIYAKU_CORPUS:-}" ]; then
  if [ ! -d "${TAIYAKU_CORPUS}" ]; then
    echo "error: TAIYAKU_CORPUS=${TAIYAKU_CORPUS} is not a directory" >&2
    exit 1
  fi
  TAIYAKU_CORPUS="$(cd "${TAIYAKU_CORPUS}" && pwd -P)"
  export TAIYAKU_CORPUS
fi

# --- a source file the repository cannot see, which is not a CI job ---
# CI catches this and catches it *late*: a `.rs` file that exists on disk but is
# gitignored compiles perfectly here and fails on the forge with `E0583: file not
# found for module`, three minutes after the push. Nothing else in this gate can
# see it, because every other check reads the working tree rather than the index.
#
# A bare `corpus/` in `.gitignore` matches a directory of that name at any
# depth, so it would swallow `crates/taiyaku-cli/src/corpus/tests.rs` —
# silently, with no complaint from `git add -A`.
#
# `--others --ignored --exclude-standard` lists exactly the files git is hiding,
# and the filter is deliberately narrow: `crates/**/*.rs` only. Generated
# artefacts under `crates/taiyaku-wasm/pkg/` are `.js` and `.ts`, and
# `.taiyaku-cache/` holds no Rust, so neither trips this.
hidden_rust=$(git ls-files --others --ignored --exclude-standard -- 'crates/*.rs' || true)
if [ -n "${hidden_rust}" ]; then
  echo "error: Rust sources exist on disk but are gitignored — they will not reach CI:" >&2
  echo "${hidden_rust}" | sed 's/^/  /' >&2
  echo "Check .gitignore for an unanchored pattern matching one of their parent directories." >&2
  exit 1
fi

# --- the `lint` job ---
run cargo fmt --all -- --check
run cargo clippy --workspace --all-targets --all-features -- -D warnings

# **`broken_intra_doc_links` specifically, not `-D warnings` — the difference is
# the whole reason this can be a gate.** A `[`name`]` that resolves to nothing
# is always a defect: the reader is sent to a symbol that does not exist, and a
# rename is how it happens. `private_intra_doc_links` is a different thing
# entirely and must stay a warning: linking a public item's doc to the private
# helper it delegates to is this workspace's own convention, so `-D warnings`
# here would fail on day one and be deleted by the end of the week.
run env RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links" \
  cargo doc --workspace --no-deps

# Advisory and non-blocking — the `|| true` is the point, not an oversight, and
# it mirrors `continue-on-error: true` on the same step in ci.yml. See
# docs/ci.md, "The pedantic lints, and why they are advisory": pedantic stays
# out of [workspace.lints] so a floating toolchain cannot turn the blocking gate
# above red, while the per-site #[expect] attributes stay enforced by it, since
# a stale one is an ordinary rustc lint.
#
# The two halves of that mirror are not equally proven. `|| true` is a shell
# operator — this script runs it, on this machine, every time, so it is
# verified rather than assumed. `continue-on-error: true` in ci.yml is
# Forgejo Actions' own key for the same job, and nothing local can exercise
# it: it only does anything when the step it guards fails.
# `-W clippy::pedantic` exits 0 no matter how many warnings it prints, only
# `-D warnings` sets a non-zero exit, so the step has no reason to fail on
# lints — and it cannot fail on lints at all, since its only possible failure
# is a compile error the blocking step above would already have caught. Both
# guards are belt-and-braces for a failure lints cannot produce today; the
# key stays untriggered rather than untested.
#
# Written out rather than passed to `run`, because `run` inherits this script's
# `set -e` and would abort the whole gate on a warning — the opposite of what
# "advisory" means here.
#
# Not quite empty at baseline, and that is expected rather than a leak: every
# `assert_eq!` on a float literal in this workspace's tests prints a
# `clippy::float_cmp` warning here, always, because clippy's JSON marks that
# lint's primary span inside `assert_eq!`'s own expansion in rustc's
# `core/src/macros/mod.rs`, never a file in this repository.
# `scripts/pedantic-sites.mjs` drops paths outside `crates/`, so it reports no
# site for them even though this unfiltered, plain-text run still prints all of
# them. See docs/ci.md, "The pedantic lints, and why they are advisory".
# Anything *else* here is a new unguarded cast or a lint a newer clippy added.
echo; echo "==> cargo clippy --workspace --all-targets --all-features -- -W clippy::pedantic (advisory)"
cargo clippy --workspace --all-targets --all-features -- -W clippy::pedantic || true

# --- the `test` job ---
# **`TAIYAKU_CORPUS` is removed from this one's environment**, which is the difference between a
# gate that finishes and one nobody runs — see "Two profiles" at the top. It is also the more
# faithful mirror: CI has no corpus, so this is what CI actually executes. The standing checks are
# not lost: they run in the pass below, which is where their output is read from.
#
# Debug, deliberately and not merely by default: this is the only pass with overflow checks and
# `debug_assert!` compiled in.
#
# `cargo llvm-cov` runs the tests itself, so it replaces the `cargo test` invocation rather than
# joining it — the same trade CI's `test` job makes, and for the same reason: instrumentation on a
# build this gate already pays for, not a second cold compile. A machine without the tool falls
# back to plain `cargo test` and names the skip, rather than failing the whole gate over a coverage
# tool nobody installed.
#
# **The per-file floor is the second command, and it is not a second test run.** `--fail-under-lines`
# is workspace-aggregate and cannot see one module go dark: a module going dark passes as long as it
# accounts for no more covered lines than the slack below.
#
# **The current figure is not written down here, it is printed by the run.** `coverage-floor.mjs`
# derives the slack — `covered - ceil(0.93 * total)` — and the files at or under it from the same
# report it already walks, and prints both on every invocation below. `docs/ci.md` keeps the
# derivation and one dated measurement.
#
# What does not change is the shape: the slack **widens as coverage rises**, so improving coverage
# enlarges the set of modules the aggregate would let go entirely dark. The per-file floor is what
# does not move with the margin, which is the whole reason it exists.
#
# `cargo llvm-cov report` re-renders the profile data the run above already produced — no rebuild, no
# re-run. `--text` and `--json` are mutually exclusive, which is why this is a second command rather
# than another flag on the first.
#
# The `--ignore-filename-regex` appears twice as a result: if the report's is narrower, the excluded
# files leak in at 12–40% and the floor names them; if it is wider, the file count drops, and the
# minimum-file check fires once it falls below the minimum.
#
# Keep the six invocation lines below in step with .forgejo/workflows/ci.yml, which runs them byte
# for byte. The property is checkable: take those six lines from each file, strip the leading
# whitespace and any `run` / `run env -u TAIYAKU_CORPUS` prefix, and the two hash identically. The
# `cargo test` fallback above is the one line with no CI counterpart, deliberately — CI always has
# the tool, because it installs it.
if ! command -v cargo-llvm-cov >/dev/null 2>&1 || ! command -v cargo-nextest >/dev/null 2>&1; then
  command -v cargo-llvm-cov >/dev/null 2>&1 \
    || note_skip "the Rust coverage gate on the test job (cargo-llvm-cov not on PATH — tests still ran, the threshold did not)"
  command -v cargo-nextest >/dev/null 2>&1 \
    || note_skip "the per-test timeout bound, and the Rust coverage gate with it (cargo-nextest not on PATH — the fallback is a plain cargo test, so nothing bounded how long any test could take and neither coverage threshold ran)"
  run env -u TAIYAKU_CORPUS cargo test --workspace --all-features
else
  run env -u TAIYAKU_CORPUS cargo llvm-cov nextest --workspace --all-features \
    --ignore-filename-regex '(band_sweep|borrow_frequency|geometry_experiment|model_experiment|region_experiment|truth_population|score_corpus|/corpus|test_support)\.rs$' \
    --fail-under-lines 93
  # Guarded the same way the tool above is, and for the same reason: a Rust-only machine without
  # node should not fail the whole gate over the half of it that node runs. CI has no such problem —
  # its `test` job fetches the pinned node tarball — so this branch is local-only.
  if ! command -v node >/dev/null 2>&1; then
    note_skip "the Rust per-file coverage floor (node not on PATH — the aggregate threshold ran, the per-file one did not)"
  else
    run cargo llvm-cov report --json --output-path target/rust-coverage.json \
      --ignore-filename-regex '(band_sweep|borrow_frequency|geometry_experiment|model_experiment|region_experiment|truth_population|score_corpus|/corpus|test_support)\.rs$'
    run node scripts/coverage-floor.mjs target/rust-coverage.json
  fi
fi

# --- the test count docs/status.md states ---
# `status.md` gives a total test count twice, and nothing else derives either figure, so they are
# checked here.
#
# **This reads the run above rather than running anything.** `.config/nextest.toml` sets
# `junit.path`, so the `cargo llvm-cov nextest` invocation a few lines up writes
# `target/nextest/default/junit.xml` on its way past, and this is a `node` process over an XML file.
# The marginal cost is approximately zero.
#
# Enumerating the tests with `cargo nextest list` instead is a more robust source, but it would
# rebuild the entire workspace, because the coverage step builds with instrumentation and a plain
# `nextest list` cannot reuse those artifacts. `scripts/status-test-count.mjs` records why that
# trade is wrong and how the robustness is recovered without it.
#
# **The guard has to match the condition that WRITES the report, not just the tools this step
# itself needs.** The report is written only on the coverage block's `else` branch, which needs
# *both* `cargo-llvm-cov` and `cargo-nextest`; its fallback runs plain `cargo test` and writes
# nothing. A guard on `cargo-nextest` alone would pass on a machine with nextest but no llvm-cov,
# run this check against a file the fallback never creates, and fail the whole gate with an ENOENT
# about a missing JUnit report — a message pointing at the wrong problem entirely.
#
# So all three are required, and the first two are named in the same order and with the same
# `command -v` form as the block above, so the two conditions can be read against each other.
if ! command -v cargo-llvm-cov >/dev/null 2>&1 \
  || ! command -v cargo-nextest >/dev/null 2>&1 \
  || ! command -v node >/dev/null 2>&1; then
  note_skip "the status.md test-count check (cargo-llvm-cov, cargo-nextest or node not on PATH — the run above took its fallback path, which writes no JUnit report, so the count in the docs was not compared against the tests that ran)"
else
  run node scripts/status-test-count.mjs target/nextest/default/junit.xml docs/status.md
fi

# --- the comment guard ---
# The last step of CI's `test` job, mirrored here. It reads the tree and the branch's diff against
# its merge base with origin/main, so unlike the step above it needs node and git alone, and runs
# whether or not the coverage tools are installed.
if ! command -v node >/dev/null 2>&1; then
  note_skip "the comment guard (node not on PATH)"
else
  run node scripts/check-comments.mjs --self-test
  run node scripts/check-comments.mjs
fi

# --- the standing corpus checks ---
# Not a CI job: CI has no corpus, so these skip there and are the one thing this script measures
# that the forge cannot. `cargo test` captures stderr for passing tests, so a check that skipped and
# one that measured real pages are indistinguishable in the output above. These run again with
# output, or say plainly that they were not run at all. A gate that quietly covers less than it
# claims is the defect this repository exists to avoid in the aligner; it should not ship in the
# gate itself.
if [ -n "${TAIYAKU_CORPUS:-}" ]; then
  # Already validated and made absolute at the top of this file.
  echo; echo "==> corpus: ${TAIYAKU_CORPUS}"
  if [ "$corpus_debug" -eq 1 ]; then
    echo "==> profile: debug (--corpus-debug) — overflow checks and debug_assert! are live, and this is slow"
    run cargo test --workspace --all-features corpus:: -- --nocapture --test-threads=1
  else
    # The second naming the header promised. A reader who scrolled past the top of this file still
    # sees, right next to the checks themselves, which invariants this pass cannot fail on.
    echo "==> profile: release — no overflow checks, and PageRange/Run's debug_assert!s do not fire"
    echo "    over corpus material; pass --corpus-debug for those. The unit tests above cover them."
    run cargo test --workspace --all-features --release corpus:: -- --nocapture --test-threads=1
  fi
else
  note_skip "standing corpus checks (TAIYAKU_CORPUS unset)"
fi

# --- the `wasm` job ---
# taiyaku-core is built for wasm32 in CI, so a change that compiles natively but not for wasm (a
# stray std-only dependency, most often) fails there and not here. Reproduced rather than trusted.
# taiyaku-wasm is built too, and for a second reason: a `wasm_bindgen` signature that cannot cross
# the boundary compiles natively and fails only for this target.
#
# The two `cargo build`s are the fast fail-fast half. The `wasm-pack` build and the smoke run are
# the other half, and they catch a different class: `cargo build` proves the crate compiles for the
# target, while `scripts/smoke-wasm.mjs` *executes* the generated module —
# `cargo test -p taiyaku-wasm` exercises `binding.rs` as plain Rust, which cannot see a codegen
# break. See `docs/ci.md`, "Running the compiled module in CI".
#
# Note that `build-wasm.sh` deletes and regenerates `crates/taiyaku-wasm/pkg`, which is the
# gitignored artefact `web/node_modules/taiyaku-wasm` symlinks to. That is the same thing a
# developer runs by hand; it is called out because this script is otherwise read-only.
if [ "$no_wasm" -eq 1 ]; then
  note_skip "the wasm job (--no-wasm)"
else
  if ! rustup target list --installed | grep -qx wasm32-unknown-unknown; then
    run rustup target add wasm32-unknown-unknown
  fi
  run cargo build -p taiyaku-core --target wasm32-unknown-unknown --all-features
  run cargo build -p taiyaku-wasm --target wasm32-unknown-unknown --all-features

  # CI installs `wasm-pack` from a prebuilt release binary; locally it is whatever is on PATH.
  # Named rather than skipped past, so a machine without it does not read as a full wasm run.
  if ! command -v wasm-pack >/dev/null 2>&1; then
    note_skip "wasm-pack package build and smoke run (wasm-pack not on PATH)"
  elif ! command -v node >/dev/null 2>&1; then
    note_skip "wasm-pack package build and smoke run (node not on PATH)"
  else
    run ./scripts/build-wasm.sh
    run node scripts/smoke-wasm.mjs
  fi
fi

# --- the `web` job ---
# `pnpm -C web run lint` is `biome check ./src ./tests`, which is formatting *and* lint in one
# command — see `docs/ci.md`, "Formatting, and the tool that checks it".
#
# **The lockfile *is* checked here.** CI's install builds `node_modules` from nothing, while
# running the same command locally would delete a working install and rebuild it on every gate
# run, so, left unchecked, a drifted lockfile would pass this script and fail the forge. pnpm has
# a command for exactly the check wanted and nothing else: `--lockfile-only` verifies the lockfile
# satisfies `package.json` without touching `node_modules`. It is run directly rather than piped,
# because a pipe would mask its exit code.
#
# It runs ahead of the `web/node_modules` guard below, deliberately. It needs no install,
# and a fresh clone — no `node_modules` yet — is exactly the state where drift is likeliest
# to go unnoticed.
if [ "$no_web" -eq 1 ]; then
  note_skip "the web job (--no-web)"
else
  run pnpm -C web install --frozen-lockfile --lockfile-only

  if [ ! -d web/node_modules ]; then
    note_skip "the rest of the web job (web/node_modules absent — run \`pnpm -C web install\`)"
  else
    run pnpm -C web run lint
    run pnpm -C web run typecheck

    # `test:coverage` is `--project node --project jsdom --coverage`, so it runs
    # both of the projects CI runs and replaces the separate `test` and
    # `test:jsdom` invocations rather than joining them — the same trade the
    # `test` job makes with `cargo llvm-cov` above, and for the same reason:
    # instrumentation on a run this gate already pays for, not a second pass.
    # The thresholds live in `web/vite.config.ts` and only fire under
    # `--coverage`, so running plain `vitest run` here would have left the web
    # half of the gate enforcing nothing while reading green.
    run pnpm -C web run test:coverage

    # **The other half of the web coverage gate, and it is not redundant.**
    # `vite.config.ts`'s thresholds are aggregate, and an aggregate cannot see
    # one module go dark. Skipping the whole of `pairing.test.ts` leaves the
    # total almost unchanged, because `App.test.tsx` exercises the same lines
    # through a mounted reader, and every vitest threshold still passes. This
    # check fails on it. vitest cannot express both halves; see the script's own
    # comment for the two forms vitest offers.
    run node scripts/coverage-floor.mjs

    # --- the browser tests, which are not a CI job ---
    # `test:coverage` above is the `node` and `jsdom` projects, which is exactly
    # what the forge runs: a browser project would need a
    # Chromium download on every CI run, and CI has no cache to keep one in. So
    # these are local-only, and named here when they cannot run rather than
    # passed over.
    #
    # They exist for what neither `node` nor `jsdom` can answer: `jsdom` has no
    # layout engine and no real `HTMLImageElement.decode()`, and every defect
    # this reader has shipped in the page area was visual. See
    # `web/tests/PageView.browser.test.tsx`.
    if [ ! -d "${HOME}/.cache/ms-playwright" ]; then
      note_skip "the browser tests (no Chromium — run \`pnpm -C web exec playwright install chromium\`)"
    else
      run pnpm -C web run test:browser
    fi
  fi
fi

echo
if [ "${#skipped[@]}" -eq 0 ]; then
  echo "all green"
else
  echo "all green, except NOT MEASURED:"
  for what in "${skipped[@]}"; do
    echo "  - ${what}"
  done
fi
