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
# workspace in debug and is what exercises them. It costs **4.4 seconds**.
#
# **Release: the standing corpus checks.** Those decode and fingerprint real pages — one compares
# 2,427 pairs, another 617 — and `image` in debug is roughly two orders of magnitude slower. They
# are not a CI job (CI has no corpus), so nothing about the mirror above depends on their profile.
# **6.9 seconds** warm; about 1:13 the first time, since `[profile.release]` here is `lto = "thin"`
# with `codegen-units = 1`.
#
# **This used to run the corpus checks twice, both times in debug, and that is why nobody could
# finish the gate.** `TAIYAKU_CORPUS` is exported at the top of this file, so the `test` job
# inherited it and ran all fifteen standing checks itself; the dedicated pass below then ran them
# again for their output. Measured before this change: over 25 minutes without reaching the second
# pass. After: about 11 seconds for both, warm. The `test` job now runs with the variable removed
# from its environment, which also makes it a *truer* mirror of CI than it was — CI has no corpus
# and has always skipped them.
#
# **What the default run does not measure**, named here and again where it happens: the corpus
# checks in release have no overflow checks and neither `debug_assert` above fires, so those two
# invariants are exercised by the unit tests and not over real material. `--corpus-debug` runs that
# pass in debug instead. It is slow — that is the whole reason it is not the default — so it is a
# thing to run when touching `PageRange`, `Run::paired`, or arithmetic on page and slot indices,
# rather than every time.
#
# It said "three jobs" until 2026-08-07 and reproduced three, while CI had grown a fourth — the
# `web` job, running biome, `tsc -b` and vitest over `web/`. So a TypeScript change could take this
# gate green having had nothing whatsoever run against it, which is precisely the drift the
# paragraph above claims cannot happen. The `wasm` job had drifted the narrower way at the same
# time: its two `cargo build`s were here, but not the `wasm-pack` package build or the smoke run
# that executes the compiled module.
#
# Anything not measured is named twice — once where it would have run, and again in the closing
# summary, which refuses to print a bare "all green" when something was skipped.
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
#
# Resolved *here*, at the top, and not beside the corpus run below — which is
# where it used to be, and that was too late. The `test` job further down
# inherits this variable too, so a relative path left it skipping every corpus
# check in silence while the gate went green; only the second run, with the path
# canonicalised, actually measured anything. O30 turned that skip into a failure,
# which is what surfaced the ordering: the gate began failing its own first job
# on an invocation it had always accepted.
if [ -n "${TAIYAKU_CORPUS:-}" ]; then
  if [ ! -d "${TAIYAKU_CORPUS}" ]; then
    echo "error: TAIYAKU_CORPUS=${TAIYAKU_CORPUS} is not a directory" >&2
    exit 1
  fi
  TAIYAKU_CORPUS="$(cd "${TAIYAKU_CORPUS}" && pwd -P)"
  export TAIYAKU_CORPUS
fi

# --- the `lint` job ---
run cargo fmt --all -- --check
run cargo clippy --workspace --all-targets --all-features -- -D warnings

# --- the `test` job ---
# **`TAIYAKU_CORPUS` is removed from this one's environment**, which is the difference between a
# gate that finishes and one nobody runs — see "Two profiles" at the top. It is also the more
# faithful mirror: CI has no corpus, so this is what CI actually executes. The standing checks are
# not lost, they move to the pass below, which is where their output was already being read from.
#
# Debug, deliberately and not merely by default: this is the only pass with overflow checks and
# `debug_assert!` compiled in.
run env -u TAIYAKU_CORPUS cargo test --workspace --all-features

# --- the standing corpus checks ---
# Not a CI job: CI has no corpus, so these skip there and are the one thing this script measures
# that the forge cannot. `cargo test` captures stderr for passing tests, so a check that skipped and
# one that measured 617 pages are indistinguishable in the output above. These run again with
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
# target, while `scripts/smoke-wasm.mjs` is the only thing anywhere that *executes* the generated
# module — `cargo test -p taiyaku-wasm` exercises `binding.rs` as plain Rust, which cannot see a
# codegen break. See `docs/ci.md`, "Running the compiled module in CI".
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
# The job this script did not have until 2026-08-07. `pnpm -C web run lint` is `biome check ./src`,
# which is formatting *and* lint in one command — see `docs/ci.md`, "Formatting, and the tool that
# checks it".
#
# **The lockfile *is* checked here, and it did not used to be.** `docs/ci.md` carried this as a
# permanent gap: CI's install builds `node_modules` from nothing, while running the same command
# locally would delete a working install and rebuild it on every gate run — so the one property it
# checks, that `package.json` and the lockfile agree, went unchecked and a drifted lockfile passed
# this script and failed the forge. pnpm has a command for exactly that check and nothing else:
# `--lockfile-only` verifies the lockfile satisfies `package.json` without touching `node_modules`.
# Measured at roughly 240ms. It is run directly rather than piped, because a pipe would mask its
# exit code.
#
# It runs ahead of the `web/node_modules` guard below, deliberately. It needs no install, and a
# fresh clone — no `node_modules` yet — is exactly the state where drift is likeliest to have gone
# unnoticed. It used to sit inside that guard, which meant a fresh clone skipped the one check most
# able to catch a mismatch it introduced.
if [ "$no_web" -eq 1 ]; then
  note_skip "the web job (--no-web)"
else
  run pnpm -C web install --frozen-lockfile --lockfile-only

  if [ ! -d web/node_modules ]; then
    note_skip "the rest of the web job (web/node_modules absent — run \`pnpm -C web install\`)"
  else
    run pnpm -C web run lint
    run pnpm -C web run typecheck
    run pnpm -C web run test

    # --- the browser tests, which are not a CI job ---
    # `pnpm -C web run test` is the `node` project alone, which is exactly what the
    # forge runs: a browser project would need a Chromium download on every CI run,
    # and CI has no cache to keep one in. So these are local-only, and named here
    # when they cannot run rather than passed over.
    #
    # They exist for what a Node environment cannot answer: `jsdom` has no layout
    # engine and no real `HTMLImageElement.decode()`, and every defect this reader
    # has shipped in the page area was visual. See `web/src/PageView.browser.test.tsx`.
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
