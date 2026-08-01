#!/usr/bin/env bash
# Collect one `set:` line per duplicate set, for a table in docs/notes.md.
#
#   scripts/duplicate-survey.sh corpus/incoming/op-duplicates
#
# `taiyaku inventory` already decides how many duplicate sets a release
# directory holds, grouping by the same parsed volume number this script's
# `sed` line groups by. Both are run, and the counts are compared below: if
# this script's shell grouping and inventory's parse ever disagree on how many
# sets exist, that is a hard failure here rather than a table with the wrong
# number of rows drifting into docs/notes.md unnoticed. A mismatch means this
# script's approximation has diverged from the real grouping, so the table it
# would print cannot be trusted — better to stop than to hand back a wrong
# answer that looks like a right one.
#
# The grouping below stays a shell approximation rather than a call into
# `inventory` itself, because `taiyaku duplicates` takes explicit copy paths on
# argv — turning inventory's `Candidate` list back into shell arguments would
# be more machinery than this script needs for a handful of already-known
# directories. Acceptable here because the count comparison against inventory's
# own summary, below, is exactly the check that the approximation has not
# drifted — not a human eyeballing two numbers.
#
# The point of this file is that the table in notes.md can be rebuilt rather
# than retyped: the same argument produces the same rows.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

root="${1:?usage: duplicate-survey.sh <root>}"
[ -d "$root" ] || { echo "error: $root is not a directory" >&2; exit 2; }

taiyaku=(cargo run --quiet --release -p taiyaku-cli --)

# What `inventory` itself decided over the same root, so its set count sits
# next to this script's own grouping below rather than only in a separate run
# nobody thinks to compare.
summary="$("${taiyaku[@]}" inventory "$root" 2>/dev/null)"
echo "== taiyaku inventory $root" >&2
echo "$summary" >&2

inventory_duplicated=""
if [[ "$summary" =~ duplicated=([0-9]+) ]]; then
  inventory_duplicated="${BASH_REMATCH[1]}"
fi
[ -n "$inventory_duplicated" ] || {
  echo "error: could not find duplicated=<n> in inventory's summary line" >&2
  exit 2
}

# Group by the volume number in each directory's name, which is what inventory
# groups by. Anything with fewer than two copies is not a duplicate set.
#
# Captured through a plain assignment rather than `mapfile < <(find ...)`:
# process substitution does not surface its exit status to this shell, so a
# `find` that fails partway (a permission error, say) would leave `mapfile`
# happily reading whatever partial output arrived, silently truncating the
# listing under a script that opens with `set -euo pipefail` and so looks like
# it would abort. An assignment puts `find` back under `set -e` and
# `pipefail`, where a failure actually stops the script.
listing="$(find "$root" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' | sort)"

declare -A group_paths=()
group_order=()
skipped=()

if [ -n "$listing" ]; then
  while IFS= read -r name; do
    raw="$(printf '%s\n' "$name" | sed -n 's/.*[vV]\([0-9][0-9]*\).*/\1/p')"
    if [ -z "$raw" ]; then
      # `inventory` never silently drops a candidate whose name carries no
      # volume number — it always reports an unparsed group, because a
      # candidate that reached the threshold and then vanished from the
      # output is the silence this project exists to avoid. Named here for
      # the same reason, rather than just falling out of the `sed` filter.
      skipped+=("$name")
      continue
    fi
    # Strip leading zeros the same way `inventory::volume_number` does — it
    # parses the digit run as a number rather than comparing it as text — so
    # `v007` and `v7` group together here exactly as they would over there.
    # This mirrors `volume_number` deliberately: the two mechanisms agree by
    # construction, not because `op-duplicates` happens to be uniformly
    # three-digit padded.
    key=$((10#$raw))
    if [ -z "${group_paths[$key]+set}" ]; then
      group_paths[$key]="$root/$name"
      group_order+=("$key")
    else
      group_paths[$key]+=$'\n'"$root/$name"
    fi
  done <<< "$listing"
fi

for name in "${skipped[@]:-}"; do
  [ -n "$name" ] && echo "== skipped (no volume number in name): $name" >&2
done

sorted_keys=()
if [ "${#group_order[@]}" -gt 0 ]; then
  sorted_listing="$(printf '%s\n' "${group_order[@]}" | sort -n)"
  mapfile -t sorted_keys <<< "$sorted_listing"
fi

shell_duplicated=0
for key in "${sorted_keys[@]:-}"; do
  [ -n "$key" ] || continue
  mapfile -t set <<< "${group_paths[$key]}"
  [ "${#set[@]}" -ge 2 ] || continue
  shell_duplicated=$((shell_duplicated + 1))

  args=()
  for copy in "${set[@]}"; do
    [ "${#args[@]}" -eq 0 ] || args+=("--")
    args+=("$copy")
  done

  echo "== v${key} (${#set[@]} copies)" >&2
  "${taiyaku[@]}" duplicates "${args[@]}" 2>/dev/null
done

if [ "$shell_duplicated" -ne "$inventory_duplicated" ]; then
  echo "error: set-count mismatch — this script's shell grouping found" \
    "$shell_duplicated duplicate set(s), but taiyaku inventory reports" \
    "duplicated=$inventory_duplicated over the same root; the table above" \
    "does not have the right number of rows, so it is not safe to use" >&2
  exit 3
fi
