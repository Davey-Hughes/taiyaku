#!/usr/bin/env bash
# Refuses to measure coverage over a `cargo llvm-cov` build another rustc made.
# cargo-llvm-cov keeps its build between runs and its report reads every test
# binary in that folder, so after a toolchain change the older binaries' lines
# count as never run and the gate goes red with no test lost. The rustc that
# built the folder is recorded beside it, and a run with another one stops here.
#
# It deletes nothing: which build to throw away is a person's call, so it names
# the folder and both versions. A record whose folder is gone is rewritten, so
# deleting the folder as told clears the refusal; a folder with no record is
# trusted, as there is nothing to compare. The folder resolves from the
# repository's top, where rustc runs so `rust-toolchain.toml`'s pin picks the
# compiler cargo will. See docs/ci.md, "Why the Rust floor is 91, and the
# toolchain is pinned".
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

dir="${CARGO_LLVM_COV_TARGET_DIR:-${CARGO_TARGET_DIR:-target}/llvm-cov-target}"
record="${dir}.rustc"
now="$("${RUSTC:-rustc}" -V)"

if [ -d "${dir}" ] && [ -f "${record}" ]; then
  was="$(cat "${record}")"
  if [ "${was}" != "${now}" ]; then
    {
      echo "error: ${dir} holds a coverage build made by ${was}, and this run would build with ${now}."
      echo "The report would read both builds' test binaries, and count every line the older ones never ran as uncovered."
      echo "Delete ${dir} and run this again."
    } >&2
    exit 1
  fi
  exit 0
fi

mkdir -p "$(dirname "${record}")"
printf '%s\n' "${now}" > "${record}"
