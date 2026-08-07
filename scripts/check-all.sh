#!/usr/bin/env bash
# The local mirror of CI (.forgejo/workflows/ci.yml). Run it yourself before pushing a branch —
# nothing runs it for you, and the forge runs the same three jobs after the push rather than before.
#
#   scripts/check-all.sh             # everything CI runs
#   scripts/check-all.sh --no-wasm   # skip the wasm target build
#
# The point of this file is that the local gate and the CI gate cannot drift: CI's three jobs are
# reproduced here in the same order with the same flags. When a job changes there, change it here.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# Parse up front so a typo (`--no-wasmm`) fails immediately rather than silently falling through to
# a full run — a flag that quietly does the opposite of what was asked is the same class of bug as a
# gate that quietly covers less than it claims.
no_wasm=0
case "${1:-}" in
  "")        ;;
  --no-wasm) no_wasm=1 ;;
  *)         echo "error: unknown argument: $1" >&2; exit 2 ;;
esac
[ "$#" -le 1 ] || { echo "error: too many arguments" >&2; exit 2; }

run() { echo; echo "==> $*"; "$@"; }

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
# `cargo test` captures stderr for passing tests, so a check that skipped and one
# that measured 617 pages are indistinguishable in the output above. These run
# again with output, or say plainly that they were not run at all. A gate that
# quietly covers less than it claims is the defect this repository exists to
# avoid in the aligner; it should not ship in the gate itself.
if [ -n "${TAIYAKU_CORPUS:-}" ]; then
  # Already validated and made absolute at the top of this file.
  echo; echo "==> corpus: ${TAIYAKU_CORPUS}"
  run cargo test --workspace --all-features corpus:: -- --nocapture --test-threads=1
else
  echo
  echo "==> NOT MEASURED: standing corpus checks (TAIYAKU_CORPUS unset)"
fi

# --- the `wasm` job ---
# taiyaku-core is built for wasm32 in CI, so a change that compiles natively but not for wasm (a
# stray std-only dependency, most often) fails there and not here. Reproduced rather than trusted.
# taiyaku-wasm is built too, and for a second reason: a `wasm_bindgen` signature that cannot cross
# the boundary compiles natively and fails only for this target.
if [ "$no_wasm" -eq 1 ]; then
  echo; echo "==> SKIPPED: wasm build (--no-wasm)"
else
  if ! rustup target list --installed | grep -qx wasm32-unknown-unknown; then
    run rustup target add wasm32-unknown-unknown
  fi
  run cargo build -p taiyaku-core --target wasm32-unknown-unknown --all-features
  run cargo build -p taiyaku-wasm --target wasm32-unknown-unknown --all-features
fi

echo
echo "all green"
