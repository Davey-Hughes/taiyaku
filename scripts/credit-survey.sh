#!/usr/bin/env bash
# Collect one `credits:` line per surveyable fan edition, for a table in
# docs/notes.md.
#
#   scripts/credit-survey.sh corpus
#
# `taiyaku credits` prints its per-run table to stderr and one fixed-field
# `credits:` line to stdout, so this script can let both through: the tables are
# what a person judges candidates by, and the lines are the table this survey
# exists to produce. Redirect stdout alone to keep the lines without the tables.
#
# **The bound in `crates/taiyaku-cli/src/credits.rs` is not chosen from this
# output.** It is chosen from this output *and* from extracting every candidate
# page and looking at it. What this script gives you is which pages to look
# at, and the reproducibility that the table in notes.md can be rebuilt rather
# than retyped.
#
# Only `en-fan` is surveyed. Credit pages are a scanlation group's own matter,
# so an official edition has none to find; and `raw-mono` is every corpus work's
# hub, which `credits` would refuse for want of an `--against` anyway.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

root="${1:?usage: credit-survey.sh <corpus root>}"
[ -d "$root" ] || { echo "error: $root is not a directory" >&2; exit 2; }

taiyaku=(cargo run --quiet --release -p taiyaku-cli --)

# Named rather than left to the command's default. The default is this same
# path, but a survey whose figures are quoted in notes.md should not change
# which fingerprints it read because a default moved.
cache=.taiyaku-cache

surveyed=0
skipped=()

# Captured through a plain assignment rather than `mapfile < <(find ...)`, for
# the reason duplicate-survey.sh gives at length: process substitution hides
# find's exit status from `set -o pipefail`, so a partial listing would survey
# some of the corpus under a script that looks like it aborts on error.
listing="$(find "$root" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' | sort)"

while IFS= read -r name; do
  [ -n "$name" ] || continue
  work="$root/$name"
  edition="$work/en-fan"

  # Every work that cannot be surveyed is named, never silently dropped. A work
  # missing from the output is indistinguishable from a work with nothing to
  # report, and "the candidate that reached the threshold and then vanished" is
  # the silence this repository exists to avoid.
  if [ ! -d "$edition" ]; then
    skipped+=("$name (no en-fan edition)")
    continue
  fi
  if [ ! -f "$work/taiyaku.json" ]; then
    skipped+=("$name (en-fan, but no taiyaku.json to align it against)")
    continue
  fi

  members=()
  for archive in "$edition"/*.cbz; do
    [ -f "$archive" ] && members+=("$archive")
  done
  if [ "${#members[@]}" -eq 0 ]; then
    skipped+=("$name (en-fan holds no .cbz)")
    continue
  fi

  echo "== $name: en-fan, ${#members[@]} member(s)" >&2
  # `|| true` on the grep, deliberately. `credits` prints its line last and
  # only after `--write` has done whatever it promised, so a run that failed
  # partway leaves nothing for grep to match — and a grep that matches nothing
  # exits 1, which under `set -e` would abort this assignment and take the
  # named error below with it. Swallowed here so the failure is reported by the
  # branch written to report it, rather than as a bare non-zero exit.
  line="$("${taiyaku[@]}" credits "${members[@]}" \
    --sidecar "$work/taiyaku.json" --cache "$cache" | grep '^credits:' || true)"

  # Checked rather than assumed: an empty row in the collected table reads as
  # an edition with nothing to report, which is the opposite of what it means.
  [ -n "$line" ] || {
    echo "error: $name produced no credits: line; the run above failed" >&2
    exit 3
  }
  printf '%s\n' "$line"
  surveyed=$((surveyed + 1))
done <<< "$listing"

for name in "${skipped[@]:-}"; do
  [ -n "$name" ] && echo "== skipped: $name" >&2
done

# A survey that found nothing is a broken invocation — a wrong root, most
# likely — and not a corpus with no fan editions in it. Exiting 0 with an empty
# table would put that mistake into notes.md as a measurement.
[ "$surveyed" -gt 0 ] || {
  echo "error: no surveyable fan edition under $root; every work was skipped above" >&2
  exit 4
}

echo "credit-survey: $surveyed edition(s) surveyed, ${#skipped[@]} skipped" >&2
