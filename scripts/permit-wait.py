#!/usr/bin/env python3
"""The permit wait: how long a reading page waits behind a cold filmstrip.

`docs/notes.md`, "O111". Run it through `scripts/permit-wait.sh`, which
starts the server this talks to.

Each run wipes the tile cache and, on a `y` run, requests all 187 `en-mono`
tiles cold over six connections, as a browser's filmstrip does. 300 ms in,
one connection requests in turn, every 50 ms for three seconds: `raw-mono`
page 10 at `?w=1024` (685x1024, so it fits), the same page at full size, the
same page at `?w=768` (a cache hit, warmed first), and a `raw-mono` page that
needs a resize, at `?w=768`, a different page each round so none is a hit.
An `n` run makes the same requests with no filmstrip.

Times are whole responses (request sent to body read) on keep-alive
connections, in milliseconds. p90 is nearest-rank.

usage: permit-wait.py PORT USER:KEY WORK THUMBS_DIR OUT.json [PLAN]
PLAN is one letter a run, `y` with the filmstrip and `n` without; default nynyny.
"""

import base64
import http.client
import json
import math
import os
import queue
import statistics
import sys
import threading
import time

PORT = int(sys.argv[1])
AUTH = "Basic " + base64.b64encode(sys.argv[2].encode()).decode()
WORK = sys.argv[3]
THUMBS = sys.argv[4]
OUT = sys.argv[5]
PLAN = sys.argv[6] if len(sys.argv) > 6 else "nynyny"

TILES = 187  # en-mono
PAGE = 10  # raw-mono page 10, 685x1024: fits 1024, needs a resize at 768
RESIZE_FROM = 20  # raw-mono pages 20.. at ?w=768, one per round, never PAGE
WINDOW_S = 3.0
LEAD_S = 0.3
GAP_S = 0.05


def conn():
    return http.client.HTTPConnection("127.0.0.1", PORT, timeout=60)


def get(c, path):
    start = time.perf_counter()
    c.request("GET", path, headers={"Authorization": AUTH})
    r = c.getresponse()
    body = r.read()
    ms = (time.perf_counter() - start) * 1000
    if r.status != 200:
        raise SystemExit(f"{path}: HTTP {r.status} {body[:200]!r}")
    return ms, r.getheader("Content-Type"), len(body)


def page_path(edition, index, w=None):
    q = f"?w={w}" if w else ""
    return f"/api/works/{WORK}/{edition}/pages/{index}{q}"


def wipe_except(keep):
    removed = 0
    for name in os.listdir(THUMBS):
        if name not in keep:
            os.remove(os.path.join(THUMBS, name))
            removed += 1
    return removed


def p90(xs):
    s = sorted(xs)
    return s[max(0, math.ceil(0.9 * len(s)) - 1)]


def summary(xs):
    return {
        "median": round(statistics.median(xs), 2),
        "p90": round(p90(xs), 2),
        "max": round(max(xs), 2),
        "n": len(xs),
    }


def filmstrip(results):
    todo = queue.Queue()
    for i in range(TILES):
        todo.put(i)

    def worker():
        c = conn()
        while True:
            try:
                i = todo.get_nowait()
            except queue.Empty:
                return
            ms, ctype, size = get(c, page_path("en-mono", i) + "/thumb")
            results.append(ms)

    threads = [threading.Thread(target=worker) for _ in range(6)]
    for t in threads:
        t.start()
    return threads


def main():
    # Warm the one file that must stay: raw-mono PAGE at 768, the cache hit.
    # The cache must start empty, so the one file after the warm is the hit.
    c = conn()
    if os.path.isdir(THUMBS) and os.listdir(THUMBS):
        raise SystemExit(f"{THUMBS} is not empty: use a fresh --cache")
    get(c, page_path("raw-mono", PAGE, 768))
    keep = set(os.listdir(THUMBS))
    if len(keep) != 1 or "-r768v" not in next(iter(keep)):
        raise SystemExit(f"expected one reading page cached after the warm, found {sorted(keep)}")
    ms, ctype, size = get(c, page_path("raw-mono", PAGE, 768))
    fits = get(c, page_path("raw-mono", PAGE, 1024))
    full = get(c, page_path("raw-mono", PAGE))
    shape = {
        "hit": [ctype, size],
        "fits": [fits[1], fits[2]],
        "full": [full[1], full[2]],
    }
    if shape["fits"] != shape["full"]:
        raise SystemExit(f"the fitting page is not the original: {shape}")
    c.close()

    runs = []
    for number, strip in enumerate(PLAN, start=1):
        removed = wipe_except(keep)
        with open("/proc/loadavg") as f:
            load = float(f.read().split()[0])
        tiles = []
        t0 = time.perf_counter()
        threads = filmstrip(tiles) if strip == "y" else []
        time.sleep(LEAD_S)
        c = conn()
        timed = {"fits": [], "full": [], "hit": [], "resize": []}
        resize_at = RESIZE_FROM
        end = time.perf_counter() + WINDOW_S
        while time.perf_counter() < end:
            timed["fits"].append(get(c, page_path("raw-mono", PAGE, 1024))[0])
            timed["full"].append(get(c, page_path("raw-mono", PAGE))[0])
            timed["hit"].append(get(c, page_path("raw-mono", PAGE, 768))[0])
            timed["resize"].append(get(c, page_path("raw-mono", resize_at, 768))[0])
            resize_at += 1
            time.sleep(GAP_S)
        c.close()
        for t in threads:
            t.join()
        strip_s = time.perf_counter() - t0 if threads else None
        if threads and len(tiles) != TILES:
            raise SystemExit(f"run {number}: {len(tiles)} tiles, not {TILES}")
        # `raw-mono`'s pages are JPEG already, so the response's type cannot
        # say whether a page was resized. The cache can: every resize writes
        # one `-r768` file, and the wipe above left only the warmed hit.
        resized = [n for n in os.listdir(THUMBS) if "-r768" in n and n not in keep]
        if len(resized) != len(timed["resize"]):
            raise SystemExit(
                f"run {number}: {len(timed['resize'])} resizes asked for, "
                f"{len(resized)} cached"
            )
        run = {
            "run": number,
            "filmstrip": strip == "y",
            "load1": load,
            "wiped": removed,
            "filmstrip_s": round(strip_s, 2) if strip_s else None,
            "tile_median_ms": round(statistics.median(tiles), 1) if tiles else None,
            **{k: summary(v) for k, v in timed.items()},
            "raw": timed,
        }
        runs.append(run)
        print(
            f"run {number} filmstrip={strip} load={load:.2f} "
            + (f"strip={run['filmstrip_s']}s tiles={run['tile_median_ms']}ms " if tiles else "")
            + " ".join(
                f"{k}={run[k]['median']}/{run[k]['p90']}/{run[k]['max']} n={run[k]['n']}"
                for k in ("fits", "full", "hit", "resize")
            ),
            flush=True,
        )
        time.sleep(1.0)
    with open(OUT, "w") as f:
        json.dump({"shape": shape, "runs": runs}, f, indent=1)


main()
