#!/usr/bin/env bash
# The local mirror of CI (.forgejo/workflows/ci.yml). Run it yourself before pushing a branch —
# nothing runs it for you, and the forge runs the same four jobs after the push rather than before.
#
#   scripts/check-all.sh             # everything CI runs
#   scripts/check-all.sh --no-wasm   # skip the whole `wasm` job
#   scripts/check-all.sh --no-web    # skip the whole `web` job
#
# The point of this file is that the local gate and the CI gate cannot drift: CI's four jobs are
# reproduced here in the same order with the same flags. When a job changes there, change it here.
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
while [ "$#" -gt 0 ]; do
  case "$1" in
    --no-wasm) no_wasm=1 ;;
    --no-web)  no_web=1 ;;
    *)         echo "error: unknown argument: $1" >&2; exit 2 ;;
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
run cargo test --workspace --all-features

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
  run cargo test --workspace --all-features corpus:: -- --nocapture --test-threads=1
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
# The job this script did not have until 2026-08-07. `npm run lint` is `biome check ./src`, which is
# formatting *and* lint in one command — see `docs/ci.md`, "Formatting, and the tool that checks it".
#
# **`npm ci` is deliberately not run, and that is a real gap rather than an oversight.** CI runs it
# to build the job's `node_modules` from nothing; here it would delete a working install and rebuild
# it on every gate run. What that costs is the one thing `npm ci` checks that the three commands
# below do not: that `package.json` and `package-lock.json` agree. A lockfile that has drifted out
# of sync passes this script and fails the forge. Run `npm --prefix web ci` yourself after changing
# dependencies.
if [ "$no_web" -eq 1 ]; then
  note_skip "the web job (--no-web)"
elif [ ! -d web/node_modules ]; then
  note_skip "the web job (web/node_modules absent — run \`npm --prefix web ci\`)"
else
  run npm --prefix web run lint
  run npm --prefix web run typecheck
  run npm --prefix web run test
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
