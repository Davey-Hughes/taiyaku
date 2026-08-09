#!/usr/bin/env bash
# Collect one `regions:` line per surveyable edition, for a table in
# docs/notes.md.
#
#   scripts/region-survey.sh corpus
#
# `taiyaku regions` prints its per-page table to stderr and one fixed-field
# `regions:` line to stdout, so this script can let both through: the tables are
# what a person judges a proposal by, and the lines are the table this survey
# exists to produce. Redirect stdout alone to keep the lines without the tables.
#
# **The constants in crates/taiyaku-cli/src/detect.rs are not chosen from this
# output alone.** They are chosen from this output *and* from drawing the
# proposed boxes over the pages and looking at them. What this script gives you
# is which editions to look at, and the reproducibility that the table in
# notes.md can be rebuilt rather than retyped.
#
# Every edition of every work is surveyed, not only the raws: the axis split is
# the cheapest check that detection is working at all, and it only means
# something if a vertical edition and a horizontal one are both in the table.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

root="${1:?usage: region-survey.sh <corpus root>}"
[ -d "$root" ] || { echo "error: $root is not a directory" >&2; exit 2; }

taiyaku=(cargo run --quiet --release -p taiyaku-cli --)

# No --cache is passed, and that is not an omission: `regions` has no such flag.
# It decodes pixels, and the fingerprint cache holds hashes and dimensions.
surveyed=0
skipped=()

# A plain assignment rather than `mapfile < <(find ...)`, for the reason
# duplicate-survey.sh gives at length: process substitution hides find's exit
# status from `set -o pipefail`, so a partial listing would survey some of the
# corpus under a script that looks like it aborts on error.
listing="$(find "$root" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' | sort)"

while IFS= read -r name; do
  [ -n "$name" ] || continue
  work="$root/$name"

  if [ ! -f "$work/taiyaku.json" ]; then
    skipped+=("$name (no taiyaku.json to identify an edition against)")
    continue
  fi

  # Every work that cannot be surveyed is named, never silently dropped. A work
  # missing from the output is indistinguishable from a work with nothing to
  # report, and that silence is what this repository is written against.
  editions="$(find "$work" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' | sort)"
  [ -n "$editions" ] || { skipped+=("$name (no edition directories)"); continue; }

  while IFS= read -r edition; do
    [ -n "$edition" ] || continue
    members=()
    for archive in "$work/$edition"/*.cbz; do
      [ -f "$archive" ] && members+=("$archive")
    done
    if [ "${#members[@]}" -eq 0 ]; then
      skipped+=("$name/$edition (holds no .cbz)")
      continue
    fi

    echo "== $name: $edition, ${#members[@]} member(s)" >&2

    # `regions` identifies its edition by content against the sidecar, and
    # nothing says every edition directory on disk has to be in it —
    # `gotoubun-v01/en-color` and `raw-color` are exactly this on the corpus
    # this script was written against (see notes.md, "Outside gotoubun..."):
    # present beside a work whose sidecar never names them. That failure is
    # routine, the same shape as no `taiyaku.json` or no `.cbz` above, and
    # belongs in `skipped` rather than aborting a run that has not actually
    # gone wrong. Stderr is captured to tell it apart from a genuine failure
    # — by *why* the command failed — while still `tee`-ing it live to this
    # script's own stderr, so the per-page table a person judges a proposal
    # by still streams as it always has on every run that gets that far.
    err_file="$(mktemp)"
    if ! full_stdout="$("${taiyaku[@]}" regions "${members[@]}" \
      --sidecar "$work/taiyaku.json" 2> >(tee "$err_file" >&2))"; then
      err="$(cat "$err_file")"
      rm -f "$err_file"
      if [[ "$err" == *"no edition of"* ]]; then
        skipped+=("$name/$edition (no edition of taiyaku.json is made of these members)")
        continue
      fi
      echo "error: $name/$edition failed:" >&2
      printf '%s\n' "$err" >&2
      exit 3
    fi
    rm -f "$err_file"

    # Checked rather than assumed: an empty row in the collected table reads as
    # an edition with nothing to report, which is the opposite of what it means.
    line="$(printf '%s\n' "$full_stdout" | grep '^regions:' || true)"
    [ -n "$line" ] || {
      echo "error: $name/$edition exited 0 but produced no regions: line; the run above failed" >&2
      exit 3
    }
    printf '%s %s\n' "work=$name" "$line"
    surveyed=$((surveyed + 1))
  done <<< "$editions"
done <<< "$listing"

for name in "${skipped[@]:-}"; do
  [ -n "$name" ] && echo "== skipped: $name" >&2
done

# A survey that found nothing is a broken invocation — a wrong root, most
# likely — and not a corpus with no editions in it. Exiting 0 with an empty
# table would put that mistake into notes.md as a measurement.
[ "$surveyed" -gt 0 ] || {
  echo "error: no surveyable edition under $root; every work was skipped above" >&2
  exit 4
}

echo "region-survey: $surveyed edition(s) surveyed, ${#skipped[@]} skipped" >&2
