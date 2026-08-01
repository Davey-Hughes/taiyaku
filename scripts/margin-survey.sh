#!/bin/sh
# Collect calibration figures for every pair in the corpus worth measuring.
#
# Settled pairs are discovered rather than listed, so a corpus that grows a
# volume is surveyed without editing this file. The incoming/ cases are named
# explicitly because each one exists to test a specific thing.
#
# Usage: scripts/margin-survey.sh [path/to/taiyaku]
set -eu

TAIYAKU="${1:-./target/release/taiyaku}"
test -x "$TAIYAKU" || { echo "no taiyaku at $TAIYAKU; cargo build --release" >&2; exit 1; }

# Every corpus path below is relative to the repository root. Run from anywhere
# else and the discovery loop matches nothing while the named rows all fail, so
# without this the survey would print a table with no good rows in it rather
# than an error.
test -d corpus || { echo "no corpus/ here; run from the repository root" >&2; exit 1; }

ROWS=0
GOOD=0

# One row per pair: group, label, then the two sides as shell words.
row() {
    group=$1; label=$2; shift 2
    line=$("$TAIYAKU" align "$@" --stats 2>&1 | grep -E '^(stats|shape):' | tr '\n' ' ')
    # An align that fails prints no stats line, and grep's non-zero exit is
    # swallowed by the pipe into tr, where `set -e` cannot see it. Left
    # unchecked the row prints blank and reads as a measurement rather than as
    # a failure, which is how a table gets a hole in it.
    if [ -z "$line" ]; then
        echo "margin-survey: no stats for $label ($*)" >&2
        exit 1
    fi
    ROWS=$((ROWS + 1))
    case $group in
        good) GOOD=$((GOOD + 1)) ;;
    esac
    printf '%-10s %-34s %s\n' "$group" "$label" "$line"
}

# The member for an edition directory: the archive inside it if there is one,
# otherwise the directory itself. Three of the four raw series are stored as
# loose images rather than archives, so both shapes have to work.
member() {
    for f in "$1"/*.cbz; do
        [ -f "$f" ] && { printf '%s\n' "$f"; return; }
    done
    printf '%s\n' "$1"
}

# --- confirmed good: every settled directory holding both a raw and an English
# --- edition. Discovered, so this needs no editing when the corpus grows.
#
# Named exactly, NOT globbed as raw-mono*: the sibling directories
# raw-mono-altcopy and raw-mono-BADCOPY exist, and a glob would file the known
# mismatched Oshi no Ko copy under `good` and quietly poison the threshold.
for dir in corpus/*-v[0-9][0-9]; do
    test -d "$dir/raw-mono" || continue
    test -d "$dir/en-mono" || continue
    row good "$(basename "$dir")" "$(member "$dir/raw-mono")" -- "$(member "$dir/en-mono")"
done

# --- confirmed good: the repagination, letterbox and identity cases
row good  "tongari-v01"        corpus/incoming/tongari/raw-v01 -- corpus/incoming/tongari/en-v01.cbz
row good  "tongari-v09-letterbox" corpus/incoming/tongari/raw-v09 -- corpus/incoming/tongari/en-v09.cbz
row good  "monster-2to1"       corpus/incoming/monster-pair/raw-v01 corpus/incoming/monster-pair/raw-v02 -- corpus/incoming/monster-pair/en-pe-v01.cbz
row good  "nausicaa-4to1"      corpus/incoming/nausicaa/raw-v01 corpus/incoming/nausicaa/raw-v02 corpus/incoming/nausicaa/raw-v03 corpus/incoming/nausicaa/raw-v04 -- corpus/incoming/nausicaa/en-deluxe-v01.cbz
row good  "op-v092-spread-repagination" corpus/incoming/op-duplicates/v092-mono-c0 -- corpus/incoming/op-duplicates/v092-mono-c1
row good  "bocchi-gaiden-real" corpus/incoming/bocchi/raw-v01-src-mangazip -- corpus/incoming/bocchi/en-gaiden-v01.cbz

# --- known bad: aligns, but wrongly
row bad   "oshinoko-v01-badcopy" corpus/oshinoko-v01/raw-mono-BADCOPY -- "$(member corpus/oshinoko-v01/en-mono)"
row bad   "op-v077-c0-vs-c1"     corpus/incoming/op-duplicates/v077-mono-c0 -- corpus/incoming/op-duplicates/v077-mono-c1

# --- needles: a short A against a long B, with no true counterpart anywhere.
# --- None of these is refused by calibration; the verdict is what catches them.
# --- colorwalk-comp2 against Go-Toubun is the worst case measured: it beats
# --- four confirmed-good pairings on margin and seven on separation.
row needle "colorwalk-to-op-v105" corpus/incoming/colorwalk-pages -- corpus/incoming/onepiece/en-v105.cbz
row needle "colorwalk-comp2-to-gotoubun" corpus/incoming/needles/colorwalk-comp2 -- corpus/gotoubun-v01/raw-mono/*.cbz
row needle "xcalendar-2015-to-op-v105" corpus/incoming/needles/op-xcalendar-2015 -- corpus/incoming/onepiece/op-raw-v105
row needle "jump-2021-24-to-op-v105" corpus/incoming/needles/op-jump-2021-24 -- corpus/incoming/onepiece/op-raw-v105
row needle "jump-2021-31-to-op-v105" corpus/incoming/needles/op-jump-2021-31 -- corpus/incoming/onepiece/op-raw-v105

# --- subset: also a short A against a long B that must end up refused, but
# --- not a needle. Kaguya volume 1 is chapters c1-c10, so this fan chapter
# --- is a genuine piece of the raw volume rather than unrelated material
# --- offered a lot of candidates, and its margin is real signal rather than
# --- the noise a needle contributes. Surveyed anyway because a partial
# --- alignment still has to be refused, and it is the corpus's only example
# --- of that shape. See docs/notes.md, "The margin survey".
# The `?` matches the space in "- v1 c1.cbz". A bare *c1.cbz would be fragile
# against a series numbering past c10, and this is one chapter, not ten.
row subset "one-chapter-to-volume" corpus/kaguya-v01/en-fan/*v1?c1.cbz -- corpus/kaguya-v01/raw-mono

# --- unrelated: already refused, and must stay so
row unrel "gotoubun-raw-vs-kaguya-en" corpus/gotoubun-v01/raw-mono/*.cbz -- corpus/kaguya-v01/en-mono/*.cbz
row unrel "bocchi-gaiden-vs-main"     corpus/incoming/bocchi/raw-v01-src-mangazip -- corpus/incoming/bocchi/en-v01.cbz

# The good rows are discovered, so their count is the check that the discovery
# found the corpus the table was built from. docs/notes.md records 24 rows, 18
# of them good; a count that disagrees means a different corpus, not a moved
# measurement.
echo "margin-survey: $ROWS rows, $GOOD good (docs/notes.md records 24 and 18)" >&2
