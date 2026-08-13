#!/usr/bin/env bash
# Copy one volume's worth of a fan edition off the NAS into a corpus work, and
# print the provenance rows that docs/notes.md's "Provenance: where the
# measured material came from" table wants.
#
#   scripts/ingest-fan-edition.sh \
#     "/mnt/daveynet/nfs/media 1/Manga (Chapters)/Kobayashi-san cno Meidoragon - Fan Translated" \
#     ' - v1 c' \
#     corpus/kobayashi-v01
#
# The filter is a fixed string matched against the file name, not a glob and
# not a regex: the names hold brackets, apostrophes and multiplication signs,
# and every one of those is a metacharacter to something.
#
# **The source is read-only.** It is the user's media library. This script
# copies out of it and never writes, moves or deletes anything under it.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

src="${1:?usage: ingest-fan-edition.sh <source-dir> <name-filter> <corpus-work>}"
filter="${2:?usage: ingest-fan-edition.sh <source-dir> <name-filter> <corpus-work>}"
work="${3:?usage: ingest-fan-edition.sh <source-dir> <name-filter> <corpus-work>}"

[ -d "$src" ] || { echo "error: $src is not a directory" >&2; exit 2; }
[ -d "$work" ] || { echo "error: $work is not a corpus work directory" >&2; exit 2; }

dest="$work/en-fan"
# `${dest:?}` rather than `$dest`: the colon form aborts when the variable is
# set but empty, which `set -u` does not catch and quoting does not fix. The
# suffix is the whole danger - bare "$dest" is harmless when empty, "$dest"/*
# is not.
if [ -d "$dest" ]; then
  echo "error: ${dest} already exists; remove it by hand if you mean to re-ingest" >&2
  exit 3
fi
mkdir -p "${dest:?}"

# One header line before the rows, so the provenance block is self-describing a
# year later. The rows alone are hash + filename, which says what a file is and
# not where it came from - and "which member of a 61 GB archive" is precisely
# the expensive question corpus/README.md says these docs exist to answer.
printf '# ingest %s\n# from   %s\n# filter %s\n' "$work/en-fan" "$src" "$filter"

copied=0
while IFS= read -r -d '' file; do
  name="$(basename "$file")"
  case "$name" in
    *"$filter"*) ;;
    *) continue ;;
  esac
  cp -- "$file" "$dest/"
  printf '%s  %s\n' "$(sha256sum < "$file" | cut -d' ' -f1)" "$name"
  copied=$((copied + 1))
done < <(find "$src" -maxdepth 1 -type f -name '*.cbz' -print0 | sort -z)

# A run that copied nothing is a wrong filter, not an edition with no volume.
# Exiting 0 with an empty directory would put that mistake into the corpus as
# a surveyable edition holding no members.
[ "$copied" -gt 0 ] || {
  echo "error: no .cbz under $src matched '$filter'; nothing ingested" >&2
  # `|| :` so a cleanup that fails cannot pre-empt the exit code this branch
  # exists to return. Under `set -e` a failing command inside a `|| { ... }`
  # block aborts at that line with its own status, and the caller would see
  # rmdir's 1 instead of the 4 that means "wrong filter". The directory is
  # freshly created and nothing writes into it before this check, so a failure
  # here is close to unreachable - but an exit code is a contract.
  rmdir "${dest:?}" || :
  exit 4
}

echo "ingest-fan-edition: $copied member(s) into $dest" >&2
