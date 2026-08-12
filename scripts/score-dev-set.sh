#!/usr/bin/env bash
#
# Score `detect_page` against the nine labelled editions of the dev set.
#
# Exists because the invocation is a trap that has been fallen into twice, and
# both times it failed *quietly*. An edition is identified by its exact member
# set, and what counts as a member differs by how the edition is stored —
# `corpus::members` states the rule:
#
#   * an edition holding `.cbz` archives is those archives;
#   * an edition holding loose page images is **the directory itself**, one
#     member, not one member per image.
#
# So `score corpus/kaguya-v01/raw-mono/*` reports "no edition is made of these
# 213 members", and `score corpus/gotoubun-v01/raw-mono` reports the same for
# the opposite reason. Neither is wrong about the corpus; both are wrong about
# the invocation. This picks the right form per edition.
#
# The other half of the trap is that a `case` filter looking for `pages=` turns
# a failed edition into a *missing row* rather than an error, so nine editions
# quietly become six and the totals read like a finding. Every edition here
# must produce a row, and the count is asserted at the end.
#
# **The nine are named, not globbed**, for the reason `region_experiment.rs`
# gives for its own `EDITIONS` array: a glob that stops matching is a silent
# nothing. Globbing the work directories was tried while writing this and is
# wrong — `corpus/*/` also holds `en-color`, `raw-color`, `raw-mono-altcopy`
# and `raw-mono-BADCOPY`, none of them part of the dev set, all of them
# indistinguishable from a genuine failure once they error.
#
# Usage:
#   scripts/score-dev-set.sh [path-to-taiyaku-binary]
#
# Defaults to ./target/release/taiyaku, which is the build to use: `detect_page`
# runs a flood fill over every pixel of a full-resolution scan, once per page.

set -euo pipefail

BIN="${1:-./target/release/taiyaku}"
if [ ! -x "$BIN" ]; then
    echo "no taiyaku binary at $BIN — run: cargo build --release -p taiyaku-cli" >&2
    exit 1
fi

# The dev set: the nine editions carrying labels in `<work>/taiyaku.truth.json`.
# Same nine `region_experiment.rs` and the L2 survey report on.
EDITIONS=(
    "gotoubun-v01/raw-mono"
    "gotoubun-v01/en-mono"
    "gotoubun-v01/en-fan"
    "kaguya-v01/raw-mono"
    "kaguya-v01/en-mono"
    "kobayashi-v01/raw-mono"
    "kobayashi-v01/en-mono"
    "oshinoko-v01/raw-mono"
    "oshinoko-v01/en-mono"
)

rows=0
failed=0
for entry in "${EDITIONS[@]}"; do
    work=${entry%%/*}
    edition=${entry##*/}
    dir="corpus/$work/$edition/"
    sidecar="corpus/$work/taiyaku.json"

    if [ ! -d "$dir" ]; then
        printf '%-14s %-9s ERROR: %s does not exist\n' "$work" "$edition" "$dir"
        failed=$((failed + 1))
        continue
    fi

    shopt -s nullglob
    archives=("$dir"*.cbz)
    shopt -u nullglob
    if [ ${#archives[@]} -gt 0 ]; then
        members=("${archives[@]}")
    else
        members=("$dir")
    fi

    # No `| head -1` here. Under `pipefail`, `head` closing the pipe after one
    # line sends SIGPIPE to `taiyaku`, whose non-zero status then aborts the
    # whole script under `set -e` — silently, having printed nothing. Which is
    # this script's own failure mode, found while writing it.
    out=$("$BIN" score "${members[@]}" --sidecar "$sidecar" 2>&1 || true)
    first=${out%%$'\n'*}

    case "$first" in
        pages=*)
            printf '%-14s %-9s %s\n' "$work" "$edition" "$first"
            rows=$((rows + 1))
            ;;
        *)
            printf '%-14s %-9s ERROR: %s\n' "$work" "$edition" "$first"
            failed=$((failed + 1))
            ;;
    esac
done

echo
echo "editions scored: $rows of ${#EDITIONS[@]}"
if [ "$failed" -ne 0 ]; then
    echo "FAILED: $failed edition(s) errored — see above." >&2
    echo "A missing row is a measurement that did not happen, not a zero." >&2
    exit 1
fi
