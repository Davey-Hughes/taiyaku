#!/usr/bin/env bash
# Summarise a taiyaku request log into the numbers docs/notes.md wants:
# prefetch depth, page-turn concurrency, image sizes and re-request patterns.
#
#   scripts/request-summary.sh panels.jsonl
set -euo pipefail

log="${1:?usage: request-summary.sh <log.jsonl>}"
[ -s "$log" ] || { echo "error: $log is empty or missing" >&2; exit 1; }

echo "requests:            $(wc -l < "$log")"
echo "max in-flight:       $(jq -s 'map(.in_flight) | max' "$log")"
echo "mean page bytes:     $(jq -s '[.[] | select(.path | test("/pages/")) | .bytes] | if length == 0 then 0 else (add / length | floor) end' "$log")"
echo "page requests:       $(jq -s '[.[] | select(.path | test("/pages/"))] | length' "$log")"
echo "distinct pages:      $(jq -s '[.[] | select(.path | test("/pages/")) | .path] | unique | length' "$log")"
echo "re-requested pages:  $(jq -s '[.[] | select(.path | test("/pages/")) | .path] | group_by(.) | map(select(length > 1)) | length' "$log")"
echo "session duration:    $(jq -s 'if length < 2 then 0 else (((map(.started_at) | max) - (map(.started_at) | min)) / 1000000 | floor) end' "$log")s"
echo
echo "page-turn cadence (ms between page requests, arrival order):"
jq -s -r '
  [.[] | select(.path | test("/pages/"))] | sort_by(.started_at) | map(.started_at) as $t
  | if ($t | length) < 2 then
      "  n/a (fewer than two page requests)"
    else
      ([range(1; $t | length) | ($t[.] - $t[.-1]) / 1000]) as $deltas
      | "  mean \(($deltas | add / ($deltas | length)) | floor)ms  min \($deltas | min | floor)ms  max \($deltas | max | floor)ms  (\($deltas | length) intervals)"
    end
' "$log"
echo
echo "distinct query strings on page requests (dimension parameters live here):"
jq -s -r '
  [.[] | select(.path | test("/pages/")) | (.path | split("?") | if length > 1 then .[1] else "(none)" end)]
  | group_by(.) | map({q: .[0], n: length}) | sort_by(-.n) | .[] | "  \(.n)  \(.q)"
' "$log"
echo
# The measurement O1 turns on. An ETag is a line of code; whether it buys
# anything depends entirely on whether a client would ever send the request
# that an ETag answers. `.headers` is null on logs written before request
# headers were recorded, and jq indexes null happily, so an old log reports
# zero rather than failing.
echo "conditional requests (what an ETag would be answering):"
jq -s -r '
  (map(select((.headers["if-none-match"] // .headers["if-modified-since"]) != null)) | length) as $conditional
  | "  \($conditional) of \(length) requests carried If-None-Match or If-Modified-Since"
' "$log"
echo
echo "range requests (partial fetches of a page):"
jq -s -r '
  (map(select(.headers["range"] != null)) | length) as $ranged
  | "  \($ranged) of \(length) requests carried Range"
' "$log"
echo
echo "clients seen:"
jq -s -r '
  [.[] | .headers["user-agent"] // "(not recorded)"]
  | group_by(.) | map({ua: .[0], n: length}) | sort_by(-.n) | .[] | "  \(.n)  \(.ua)"
' "$log"
echo
echo "slowest five:"
jq -s 'sort_by(-.micros) | .[:5] | .[] | "  \(.micros/1000 | floor)ms  \(.status)  \(.path)"' -r "$log"
