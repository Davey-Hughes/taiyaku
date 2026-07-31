#!/usr/bin/env bash
# The local mirror of CI (.forgejo/workflows/ci.yml). Run it before landing a branch —
# scripts/land.sh runs it for you, on the merged tree, before the commit exists.
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

# --- the `lint` job ---
run cargo fmt --all -- --check
run cargo clippy --workspace --all-targets --all-features -- -D warnings

# --- the `test` job ---
run cargo test --workspace --all-features

# --- the `wasm` job ---
# taiyaku-core is built for wasm32 in CI, so a change that compiles natively but not for wasm (a
# stray std-only dependency, most often) fails there and not here. Reproduced rather than trusted.
if [ "$no_wasm" -eq 1 ]; then
  echo; echo "==> SKIPPED: wasm build (--no-wasm)"
else
  if ! rustup target list --installed | grep -qx wasm32-unknown-unknown; then
    run rustup target add wasm32-unknown-unknown
  fi
  run cargo build -p taiyaku-core --target wasm32-unknown-unknown --all-features
fi

echo
echo "all green"
