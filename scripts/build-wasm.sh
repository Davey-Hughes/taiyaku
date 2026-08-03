#!/usr/bin/env bash
# Build the browser binding as an npm package.
#
# `--target web` emits an ES module with an explicit `init()`, which is what
# Vite wants; `--target bundler` hides the instantiation and makes the wasm a
# bare import that Vite's dep optimiser then has to be told about. One
# explicit await at startup is cheaper to reason about than that.
#
# The output is not committed: it is a build artefact of a crate in this
# repository, and a checked-in binary would be the second source of truth for
# what the binding does.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${ROOT:?}/crates/taiyaku-wasm/pkg"

rm -rf "${OUT:?}"
wasm-pack build "${ROOT:?}/crates/taiyaku-wasm" \
  --target web \
  --out-dir pkg \
  --out-name taiyaku_wasm

echo "built: ${OUT}"
ls -la "${OUT}"
