#!/usr/bin/env bash
# Time the permit wait against one build: `docs/notes.md`, "O111".
#
# Serves `gotoubun-v01` from a copy of its sidecar with every member
# path made absolute, with a new database and cache under WORKDIR, then runs
# scripts/permit-wait.py against it and stops the server by its own PID.
# Nothing is written under corpus/. WORKDIR must not exist yet, so one build's
# cache can never warm another's run.
#
# The figures mean nothing on a busy machine. Wait until no cargo, rustc or
# test run is going, and put the 1-minute load beside every figure; the
# script records it per run.
#
# Usage: scripts/permit-wait.sh path/to/taiyaku WORKDIR [PORT] [PLAN]
set -euo pipefail

BIN=${1:?usage: permit-wait.sh path/to/taiyaku WORKDIR [PORT] [PLAN]}
WORKDIR=${2:?usage: permit-wait.sh path/to/taiyaku WORKDIR [PORT] [PLAN]}
PORT=${3:-47200}
PLAN=${4:-nynyny}
REPO=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
# `TAIYAKU_CORPUS` as the corpus checks read it, for a worktree with no corpus/.
CORPUS=${TAIYAKU_CORPUS:-$REPO/corpus}/gotoubun-v01

test -x "$BIN" || { echo "no taiyaku at $BIN; cargo build --release" >&2; exit 1; }
test -f "$CORPUS/taiyaku.json" || { echo "no sidecar at $CORPUS" >&2; exit 1; }
if [ -e "$WORKDIR" ]; then echo "$WORKDIR exists; use a new one" >&2; exit 1; fi
mkdir -p "$WORKDIR/cache/thumbs"
WORKDIR=$(cd "$WORKDIR" && pwd)

python3 - "$CORPUS" "$WORKDIR/taiyaku.json" <<'PY'
import json, os, sys
corpus, out = sys.argv[1], sys.argv[2]
d = json.load(open(os.path.join(corpus, 'taiyaku.json')))
for e in d['editions'].values():
    for m in e['members']:
        m['path'] = os.path.join(corpus, m['path'])
        assert os.path.isfile(m['path']), m['path']
json.dump(d, open(out, 'w'), indent=1)
PY

printf 'bench-password-1\nbench-password-1\n' \
  | "$BIN" library user promote bench --db "$WORKDIR/library.db" >/dev/null
KEY=$("$BIN" library key create bench --name bench --db "$WORKDIR/library.db" | tr -d '\n')
sha256sum "$BIN" >"$WORKDIR/binary.sha256"

"$BIN" serve "$WORKDIR/taiyaku.json" --addr "127.0.0.1:$PORT" \
  --db "$WORKDIR/library.db" --cache "$WORKDIR/cache" >"$WORKDIR/server.log" 2>&1 &
PID=$!
trap 'kill "$PID" 2>/dev/null || true; wait "$PID" 2>/dev/null || true' EXIT

for _ in $(seq 200); do
  if curl -sf -o "$WORKDIR/works.json" -u "bench:$KEY" "http://127.0.0.1:$PORT/api/works"; then break; fi
  python3 -c 'import time; time.sleep(0.1)'
done
WORK=$(python3 -c '
import json, sys
d = json.load(open(sys.argv[1]))
works = d if isinstance(d, list) else d.get("works", d)
if len(works) != 1:
    sys.exit(f"expected one work, found {len(works)}")
print(works[0]["slug"])
' "$WORKDIR/works.json")

python3 "$REPO/scripts/permit-wait.py" "$PORT" "bench:$KEY" "$WORK" \
  "$WORKDIR/cache/thumbs" "$WORKDIR/result.json" "$PLAN"
