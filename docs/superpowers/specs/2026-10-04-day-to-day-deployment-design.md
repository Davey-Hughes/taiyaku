# Day-to-day deployment

**Date:** 2026-10-04. **Status:** each part's approach was chosen in
conversation; the details below are written for review.

Komga and Stump both run as containers on the Unraid NAS, and taiyaku does
not. This spec covers what has to change before taiyaku can replace them for
everyday reading of the real library: about 1,900 works (1,896 in the
2026-09-17 copy measured below) on `/mnt/user/media 1/Manga`, a share that
spans `disk1` and `disk4`.

It is five parts. Each part is its own PR with its own plan:

| Part | What | Closes or narrows | Depends on |
| --- | --- | --- | --- |
| **A. Live works** | the reader sees what the library holds now, not what it held at startup | O183 (the startup snapshot), opened and closed | — |
| **B. Deployable** | `taiyaku serve` serves the web app; an image; CI pushes it | — | — |
| **C. Scheduled scans** | rescans by watcher and by timer, set from Configuration | O51; measures O52 and O42 on the NAS | A, and B deployed |
| **D. Missing works** | works whose files are gone are hidden and recoverable, and removed only on request | O91, O48 | A |
| **E. Pending library writes** | writes into the library wait until an admin flushes them | O82 | B |

A and B can run in parallel lanes. C needs B on the NAS, because the questions
it measures can only be answered there. C, D and E each add a migration and
each touch Configuration, so they land one after another: C, then D, then E.

## 1. What was measured

- **Re-projecting every work is cheap.** `taiyaku serve --db` over a copy of
  the real library's database (1,896 works, 1,896 editions, 1,896 members) goes
  from process start to `listening` in **under 50 ms**, in each of three runs.
  That figure covers `works::from_library` and everything else startup does.
  Release build, this workstation.
- **A rescan is cheap.** `taiyaku library scan` on the same copy (root on local
  NVMe, 490 GiB, 129 directories, 2,044 entries) takes **0.38–0.41
  s** for walk plus derive. Three runs, 1,896 works derived each time. The
  database copy was the only thing written; the tree's only change was its
  root directory's mtime, from the watch probe.
- **The server's scan already avoids the shared connection.** `config_api.rs`'s
  `run_scan_inner` scans on `library.working()`, a second connection, so a scan
  started from the roots screen does not hold the `Mutex<Connection>` that O52
  and O42 describe. What remains is write contention under WAL, which
  `busy_timeout` governs. Part C measures this on the NAS.
- **A `Work` is built from six tables**: `project.rs` reads `work`, `edition`,
  `member`, `page_flag`, `run` and `alignment`. Nothing in the tree updates any
  of them on a page turn. Every `UPDATE` against them is structural: derive,
  import, fold, stamp, `librarydoc`.
- **There is no database to migrate.** The workstation's
  `~/.local/share/taiyaku/library.db` holds thirteen mutation-test roots, eight
  works, no progress and no notes. The container starts from a fresh database.
- **The library is not writable by the container's user, and stays that way.**
  `/mnt/user/media 1/Manga` is `root:root 755`. Of its 580 directories, 73
  cannot be written by uid 99 (some, such as `Chapters/`, are `nobody:users
  555`), and neither can 2,910 files. Every file is readable. **The person
  keeps these permissions** and loosens them only to let writes through, which
  is why Part E exists.

## 2. Part A — live works

### 2.1 The defect

`ServeState.works` is a `Vec<Work>` built once, at startup
(`crates/taiyaku-serve/src/lib.rs`, the field's own doc comment). Every
`/api/library/*` route reads the database directly. `/api/works`, the reader's
routes and OPDS read this list instead. So after a scan the library grid shows
a new volume, and opening it shows "The server found no work named …". The same
staleness follows a rule save, an accepted proposal, a root removal and a
ComicInfo write-back: the reader serves the old structure until a restart.
`finished_scan_json`'s `note` field already says so in every scan report.

It has no open item. It gets one, opened and closed by this part.

### 2.2 The design

**A change counter in the database.** Migration 14 adds a one-row table:

```sql
CREATE TABLE structure_version (n INTEGER NOT NULL);
INSERT INTO structure_version VALUES (0);
```

It also adds `AFTER INSERT`, `AFTER UPDATE` and `AFTER DELETE` triggers on each
of the six tables in §1. Each trigger runs `UPDATE structure_version SET n =
n + 1`. Triggers fire for every connection and every process, so a
`docker exec … taiyaku library scan` is seen exactly like a scan started from
the roots screen. Nobody has to remember to call anything, and nothing has to
list the jobs that change structure.

**`ServeState.works` becomes a registry.** It holds the works keyed by slug,
plus the counter value they were projected at. Handlers take an `Arc<Work>`
from it rather than borrowing `&Work` from the state.

**When it checks:**

1. **Every 2 s**, a background task reads `n` on a connection of its own, not
   the shared one.
2. **Before answering a miss.** A request naming a slug the registry does not
   hold checks `n` first, and refreshes if it moved. A volume a scan has just
   derived therefore opens on the first click, with no window to fall into.

**What a refresh does.** If `n` has not moved, nothing. If it has, it reads
every work's row on a `working()` connection and reconciles:

- **A slug the registry already holds keeps its `Arc<Work>`.** Its sidecar is
  re-projected and swapped **while holding that work's own write lock**, the
  one `Work::update` takes for a correction. So a correction in flight and a
  refresh run one after the other. A refresh cannot overwrite a correction
  that committed after the refresh began, and the per-work `notes` mutex is
  never duplicated across a swap. The one exception is a work whose
  `sidecar_path` changed (only `library import` of a moved `taiyaku.json` does
  that): `Work.path` is fixed at creation, so it gets a new `Work`.
- A slug that is new gets a new `Work`.
- A slug no longer in the database leaves the registry.

At under 50 ms for all 1,896 works there is no per-work dirty tracking. A refresh
re-projects everything, and a correction, which bumps the counter like any
other write, costs one refresh.

### 2.3 Boundaries

- **Only `serve` with no paths refreshes.** `serve foo/taiyaku.json` serves the
  works it was handed and keeps its startup list, so O61 is unchanged.
- A configured Stump source resolves only the works present at startup. Stump
  survives only as a migration path (store spec §10), so newly added works are
  served from disk. This is stated in the code, not fixed.
- The field's doc comment and `finished_scan_json`'s `note` are deleted, not
  reworded. Both describe the defect this part removes. The web app reads and
  draws that `note`, so its reader goes with it.
- **One work that will not project no longer stops the server starting.**
  Today startup refuses the whole library. Under WUD's auto-deploy that would
  be a restart loop until someone fixed the row, so startup now leaves that
  work out and logs which one and why. A running refresh already behaves this
  way: it keeps the previous document for a work it cannot re-project.

### 2.4 Testing

- A work derived after the server started opens through `/api/works/{slug}`
  with no restart, both through the 2 s tick and through the miss path. Each
  path has its own test, with the other one disabled.
- A write made on a second connection, standing in for another process, is
  picked up.
- A slug whose work was deleted stops answering.
- **A correction racing a refresh survives it.** The correction holds the
  work's write lock while a refresh is pending, and the in-memory sidecar
  afterwards carries the correction.
- The counter moves for an insert, an update and a delete on each of the six
  tables (eighteen cases, generated), and does not move for a `progress` or
  `note` write.

## 3. Part B — deployable

### 3.1 The server serves the app

Before this part there was no static file serving in `taiyaku-serve`, so the
web app ran only under `vite dev` or `vite preview`, with a proxy splitting
`/api` from `/`.

- **`--web DIR`.** Given, the server serves `DIR` (a built `web/dist`) through
  `tower-http`'s `ServeDir`. Without it, nothing changes, and development keeps
  Vite's proxy. `tower-http` comes in with the `fs` feature only.
- **Any path that is not `/api/…`, not `/opds/…` and not a file answers
  `index.html`**, so a reload on `/read/x`, `/series/3` or `/config/users`
  works. An unknown path under `/api` or `/opds` keeps today's answer, a 404
  with an empty body. A missing file under `/assets/` is a 404, never
  `index.html`.
- **Static files need no credential.** The sign-in screen has to load before
  there is one. Everything under `/api` and `/opds` stays behind the guard
  exactly as it is.
- **Cache headers**: Vite's content-hashed `/assets/*` are
  `public, max-age=31536000, immutable`; `index.html` is `no-cache`. A phone
  then picks up a new deployment on its next load.
- `serve()`'s options, `--web` among them, arrive as one `ServeOptions`
  struct rather than as a ninth positional argument.
- **Rejected**: a static handler written here, because this crate would then
  own path-traversal safety and conditional requests; and embedding `dist` in
  the binary, which would couple `cargo build` to the web build and put Node in
  every Rust CI job.

### 3.2 The image

A `Dockerfile` with four stages:

1. **wasm**: Rust plus `wasm-pack`, building `crates/taiyaku-wasm/pkg` the way
   `scripts/build-wasm.sh` does.
2. **web**: Node 26 and `pnpm@11.3.0` (installed with `npm i -g`, as CI does),
   `pnpm install --frozen-lockfile`, `pnpm run build`.
3. **server**: `cargo build --release -p taiyaku-cli` on bookworm. `rusqlite`
   links the system SQLite (the dev binary's `ldd` names `libsqlite3.so.0`),
   so the build and the runtime stages share one distribution's SQLite.
4. **runtime**: `debian:bookworm-slim` with `libsqlite3-0`, the binary, and
   stage 2's `dist` at `/app/web`. Runs as `99:100`, Unraid's `nobody:users`.

| Container path | Host | Why |
| --- | --- | --- |
| `/config` | `/mnt/user/appdata/taiyaku/config` | `XDG_DATA_HOME`: `library.db` and `backups/`. Appdata Backup copies it |
| `/cache` | `/mnt/user/appdata/taiyaku-cache` | `XDG_CACHE_HOME`: covers, tiles and reading-size pages. Regenerable, so kept out of the backup |
| `/library` | `/mnt/user/media 1/Manga` | mounted read-write, but the host's permissions keep uid 99 out (§1). Only admins write into it: metadata documents under `<root>/.taiyaku/`, ComicInfo write-back (both admin-gated in `metadata.rs` and `stamp.rs`), and the watch probe's temporary file when a root is added or scanned from the CLI (§4). Part E queues the ComicInfo write-backs until the person flushes them (metadata documents may move under `/config`, §6). A reader's pairing correction on a scanned work writes database rows, not files |

- The command is `taiyaku serve --addr 0.0.0.0:8484 --web /app/web`.
  `--trusted-proxy` also reads `TAIYAKU_TRUSTED_PROXY`, so the Unraid template
  sets it without overriding the command.
- A `HEALTHCHECK` on `GET /`, which needs no credential once `--web` is given.
- The first account: `docker exec -it taiyaku taiyaku library user promote
  davey`.
- **The database is in WAL mode.** taiyaku stays on Appdata Backup's stop
  list, which is what makes the copy consistent, and the cache mapping is
  excluded from the backup. Both settled by the person.
- **It runs as an Unraid Compose Manager stack**, like `redextape` and
  `ws-sim`: the image `forge.daveynet.xyz/davey/taiyaku:latest`, host port
  55006, and their three WUD labels (`wud.watch=true`,
  `wud.watch.digest=true`, `wud.trigger.include=docker.redeploy`).

### 3.3 CI

A `docker` job in `.forgejo/workflows/ci.yml`, on pushes to `main` only, copied
from redextape's (the forge's runner automounts the NAS's Docker socket):

- an isolated buildx builder named `taiyaku-builder`, so nothing under the
  registry name lands in the host's image store. BuildKit then runs as its own
  container, outside the runner's `--cpus 4`, so the builder is capped through
  its own driver options;
- tags `sha-<7>`, and `latest` when the commit is main's tip, pushed to
  `forge.daveynet.xyz/davey/taiyaku` with `REGISTRY_USER` and `REGISTRY_TOKEN`
  repository secrets (the person creates these), and
  `--provenance=false --sbom=false`;
- **it runs the built image and probes `/` before pushing.** Redextape's
  `Dockerfile` records a health check that passed a green pipeline while every
  container built from it reported `unhealthy`, because no job ever started
  the image.

WUD then deploys `:latest`. **Every merge to `main` goes live** (normally the
newest merge's image when two builds overlap; `docs/ci.md` records the window
between the tip check and the push), including any migration, which runs on
startup. Appdata Backup is the rollback.

### 3.4 The proxy

Nginx Proxy Manager runs on the NAS, on host ports 180 and 1443, with a
`*.daveynet.xyz` wildcard certificate. taiyaku gets a proxy host for
**`taiyaku.daveynet.xyz`**, proxied through Cloudflare (orange cloud) like the
user's other sites (decided 2026-10-04). Cloudflare's edge reaches NPM
directly, not through the Cloudflare tunnel, so **taiyaku is public**.

The README requires the proxy to set `Host` and `X-Forwarded-Host` itself, or
every login answers 403. NPM's generated `location /` sets `Host`,
`X-Forwarded-For` and three others, never `X-Forwarded-Host`, and nginx does
not inherit a server-level `proxy_set_header` into a location that sets its
own. So:

- the host's Advanced configuration carries a **whole `location / { … }`
  block**, which NPM then uses in place of its own;
- it sets `X-Forwarded-Host` (and `Host`) from **`$http_host`, not `$host`**.
  A browser at `https://taiyaku.daveynet.xyz:1443` sends an `Origin` with the
  port, and `$host` drops it, so the Origin check would refuse every login.
  Through Cloudflare the browser's Origin carries no port at all, and
  `$http_host` is still the right choice;
- **`real_ip_header CF-Connecting-IP;`** sits on its own line **above** the
  block, not inside it. nginx's real-IP module applies at most once per
  request, at server level first, and NPM sets `real_ip_header X-Real-IP` at
  http level, so a line inside `location /` loses to an `X-Real-IP` header the
  visitor sends. NPM places the Advanced tab's content at server
  level, so a line written above the `location` there wins.
  Measured read-only on the NAS, NPM's global nginx config trusts private
  ranges and Cloudflare's published ranges (`set_real_ip_from`, via
  `conf.d/include/ip_ranges.conf`) but sets `real_ip_header X-Real-IP;`, a
  header Cloudflare does not send. taiyaku takes the rightmost
  `X-Forwarded-For` entry (`crates/taiyaku-serve/src/auth.rs`, the
  `rsplit(',')` near line 152), which would be a Cloudflare edge address, and
  the login rate limiter would key every visitor together. With the line
  at server level, NPM's `$remote_addr` is the client's address as Cloudflare
  reports it, for requests from Cloudflare's ranges, and
  `$proxy_add_x_forwarded_for`'s rightmost entry is that address;
- **"Cache Assets" and "Block Common Exploits" are off.** The first caches
  scripts and fonts in a proxy cache that ignores `Cache-Control` and
  `Set-Cookie`. The second answers 403 to any query value starting with `/`.

Cloudflare's cache is not a risk for signed-in content. Cloudflare does not
cache a response marked `private`, `no-store` or `no-cache`, and every guarded
response carries one: `per_reader_by_default` in `routes.rs` adds
`private, no-store` to any that set no `Cache-Control` of their own, and pages
and downloads set `private, no-cache` themselves (`PAGE_CACHE_CONTROL`, and
`download.rs`). Content-hashed `/assets/*` are cached at the edge, which is
fine because they are immutable.

What NPM actually forwards is read off the running proxy, not assumed, and
`TAIYAKU_TRUSTED_PROXY` is set to the address taiyaku actually sees NPM
connect from.

### 3.5 Testing

- Route tests: a deep link answers `index.html`; `/api/nope` answers a 404
  with an empty body; `..` and encoded traversal never serve a file outside
  the directory; a static file answers without a credential while `/api/me`
  still answers 401; the two cache headers.
- The CI job's own run-and-probe.
- **By hand on the NAS after merge, on the image CI builds and pushes** (no
  image is loaded by hand): the container starts, sign-in works through NPM, a page turns on the phone, and the health check
  reads healthy in `docker inspect`.

## 4. Part C — scheduled scans

### 4.1 What exists

- `watch::probe` answers by experiment whether a root can be watched.
  `interval_for` turns that into 24 h (watched) or 15 min (not). The server
  runs it only on `POST /api/library/roots` and keeps no answer, so a root
  loaded from the list has no watch state (`config.ts`'s `regimeFacts` says
  "unknown — probed when a root is added"). The CLI's `library scan` probes
  each root it scans.
- The roots screen already has a "Scan interval (seconds)" box
  (`RootRow.tsx`), saved to `root.scan_interval_secs`. **Nothing reads it.**
  O51.
- Spec B §8 designed three tiers: a watcher where the probe succeeds, an
  interval as the safety net (long where watching works, short where it does
  not), and the cheap walk that makes both affordable. Only the third is built.

### 4.2 The design

**Probe at startup.** The server probes every root when it starts, as spec B §8
asked, and keeps the answer in memory. The roots list reports it, so the
screen's `watch` fact is never "unknown" on a running server. The probe stays
at startup and on adding a root, never per scan: it writes into the library
tree, and a scheduled scan has no reason to.

**A probe that cannot write its file proves nothing about events.** On the real
library it will always fail that way, because the root stays unwritable to
uid 99 (§1). So the probe now has three answers: *watching* (the event
arrived), *failed* (the file was written and no event came), and
*unverified* (the file could not be written). For *unverified*, the root's
watch switch decides, and the watcher runs as untested. The daily interval is
then the safety net it always was, and the screen says the watch is
unverified.

**The watcher.** For each root whose probe succeeded and whose switch (below)
is on, a recursive `notify` watch:

- **It ignores what the walk ignores**: dot-prefixed paths (which covers the
  probe file and `.taiyaku/`, where metadata edits write) and paths matching
  the root's `exclude_globs`. A metadata edit therefore does not trigger a
  scan, and a ComicInfo write-back, which changes an archive's bytes, does.
- **Debounced**: a scan starts once the root has been quiet for 60 s, and no
  later than 10 min after the first event, so a long copy still scans.
- **A watch that cannot be established whole is reported, never used
  partially.** Spec B §8 says a partially watched root is the worst of the
  three states. If adding a directory's watch fails (the inotify ceiling, a
  permission error), the root falls back to its interval, and the screen says
  why.

**The scheduler.** One task holds each root's next due time. A root is due at
`last_scan_at + interval`. The interval is the root's own
`scan_interval_secs` when set, and the probe's default when not.

- **Every root is scanned once at startup**, after its probe, because events
  that arrived while the server was down were seen by nobody. At 0.4 s on the
  workstation this is cheap. Whether it is cheap on the NAS with disks asleep
  is one of §4.4's measurements.
- **Scheduled and watched scans go through the same path as the roots
  screen's Scan button**: the `scans` registry and `library_job`. A scan that
  is due while that root is already scanning becomes one pending rescan, run
  when the current one finishes, never a 409 that nobody reads.
- **A settings change takes effect at once.** A `PATCH` to a root, an added
  root and a removed root each wake the scheduler and watcher to re-read the
  roots, with no restart.

### 4.3 The settings

On Configuration → Roots, per root:

- **Scan schedule**: *Automatic*, *Every* (a number with a minutes or hours
  unit), or *Off*. Automatic follows the probe (24 h watched, 15 min not) and
  is stored as `NULL`. Off means no scheduled scans: the watcher, if on, and
  the Scan button still work. **Adding a root stops writing the probe's
  interval into `scan_interval_secs`.** Today it does, which pins a root to
  whatever the probe said on the day it was added, even after a later probe
  disagrees.
- **Watch for changes**: on by default. A new column, `root.watch`, in this
  part's migration. Turning it off covers a watcher that misbehaves, or one
  that keeps disks awake.
- The facts beside them read: `watch` (watching / unverified / probe failed:
  why / off), `scan every`, `last scan`, and a new `next scan`.
- *Automatic* follows the probe: 24 h when watching or unverified-and-on, and
  15 min when the probe failed or the switch is off.

### 4.4 What C measures on the NAS, and what it decides

- **Whether the probe succeeds through `shfs` inside the container.** If it
  does not, the 15 min default sits exactly on the disks' 15 min spin-down
  (`spindownDelay="15"`) and can keep them awake. The roots screen can then
  set a longer interval, and the measurement is recorded in `notes.md` beside
  the decision.
- **How long a rescan takes on the NAS**, warm and with disks asleep.
- **O52 and O42**: page-request latency while a scan runs, compared with idle.
  If it does not move, O52 closes on the measurement (its scan half was
  answered when C4 moved the scan onto `working()`), and O42 is narrowed to
  what it still claims.

### 4.5 Testing

- The scheduler, driven by a clock the test controls: a due root scans; Off
  never schedules; Automatic follows the probe's answer; a `PATCH` reschedules
  without a restart; a due scan during a running one runs once, afterwards.
- The watcher, on a real temp tree: a new archive triggers one scan after the
  quiet period; writes under `.taiyaku/` and under an excluded path trigger
  none; a burst triggers one scan, not one per event.
- The ceiling case: a watch that cannot be added falls back to the interval and
  reports why. Driven by a watcher stand-in, since the real inotify limit is
  not something a test should exhaust.

## 5. Part D — missing works

### 5.1 The defect

O91: a root that loses some of its files, not all, leaves the vanished works
standing forever, with nothing that can clean them up. `derive` deletes a work
only when `fold::absorb` moves its material elsewhere. O48: a root mounted one
directory deep, or 99% unmounted, is applied rather than refused, because §8.1's
refusal fires only for a wholly empty root. O92's exclude patterns reach the
same state from a text box; the real-library run left 242 works standing that
way.

### 5.2 The design

**A work whose files are all gone is marked missing, not deleted.**

- Migration adds `work.missing_since TEXT NULL`. Derive sets it on a scanned
  work none of whose members has a `scan_entry` left, and clears it when one
  returns. **A work imported from a `taiyaku.json`, which sits under no root,
  is never marked**, because no scan is the authority for it.
- **A missing work is hidden** from the library grid, series pages, search,
  the continue strip and every OPDS feed. The rule is one predicate, applied
  at each listing: `browse`'s queries, and OPDS, which walks Part A's registry
  rather than querying. The plan enumerates those listings from the code
  before the first change, and the review checks the enumeration, not the
  predicate.
- A direct link to a missing work opens the reader's failed-load screen with a
  reason that says the volume's files are missing, not "no work named".
- **When the files return** (a disk remounts, an exclude pattern is cleared),
  the next scan clears the mark. The work keeps its id, its progress and its
  notes. This is the part's central claim, and §5.4's first test.
- Works an exclude pattern removed become missing too. They are hidden, they
  appear in the list below, and clearing the pattern brings them back.

**Removal is an admin's act, counted first**, the way spec C4's root removal
already is:

- Configuration gets a **Missing works** list: title, root, the path last seen,
  missing since, and the reading positions and notes each one holds.
- Remove works one at a time or all together. Each removal first answers what
  it would delete (works, positions, notes: `scan::Removal`'s fields, reused),
  then deletes on confirmation.
- The CLI mirrors it, in the house style of printing by default and acting on
  a flag: `taiyaku library missing` lists, and
  `taiyaku library missing --remove ID… | --all` removes.

**O48 closes by consequence.** A partial scan is still applied, but applying
it now hides works rather than stranding them, and the next good scan brings
them back. That is the recovery O48 wanted a threshold for, without choosing a
threshold. The wholly empty root stays refused, as §8.1 has it.

### 5.3 What it costs a returning volume

`forget_unseen` deleted the vanished entries' `scan_entry` rows, digests
included. So a returning file is read and hashed again once, the same cost as
a first scan of it. That is acceptable for an event this rare, and it is
stated rather than optimised.

### 5.4 Testing

- **Delete a volume's file, scan, restore the file, scan: the same work id,
  with its progress and notes intact.** This is a real-filesystem test, and it
  is written and watched failing before derive changes. It is the one claim
  in this part nobody has checked yet.
- A missing work is absent from each listing the plan enumerated. One test
  per listing, so that a listing added later without the predicate fails its
  own test.
- An imported (unrooted) work is never marked.
- Removal's counts match what it deletes, and nothing is deleted without
  confirmation.
- A partial mount (half a root's directories gone) hides that half, and
  remounting brings it back. O48's own scenario.

## 6. Part E — pending library writes

*Decided 2026-10-04:* hand-written metadata documents may live under `/config`
instead of `<root>/.taiyaku/`, so a metadata edit no longer needs the library
to be writable. Part E's plan settles the documents' address under `/config`
and how existing `.taiyaku/` documents are read. The queue then holds ComicInfo
write-backs only.

### 6.1 The defect

The library stays unwritable to the container's user (§1). The person loosens
its permissions only to let writes through, then tightens them again. Besides
the watch probe's temporary file (§4), two things write into it, both
admin-only, and today both write at the moment they are asked to:

- **Hand documents.** A metadata edit on a root with `keeps_documents` on (off
  by default) writes `<root>/.taiyaku/<content_key>.library.json`. When that
  fails, the route answers `persisted: false` and nothing ever retries it. That
  is O82, which also measured a worse third state: an edit against a
  read-only root whose document already exists leaves that document **stale**,
  still holding the previous correction, and a rebuild reads the older value
  back.
- **ComicInfo write-back.** `stamp::apply` rewrites the archives a confirmed
  plan names. Against an unwritable directory it simply fails.

### 6.2 The design

**One list of pending writes, two kinds, and one button that flushes them.**

- **Documents are computed, not queued.** A work owes a document when its
  `LIBRARY` marker is set (`absorbed = 1`, which `hand::save` records whether
  or not the file landed). It is pending when that document is absent or
  stale. `hand::reconcile` already finds both inside derive, and the list
  reuses that answer, so there is no table for documents. A metadata save
  still tries the write immediately, as today; this changes only what happens
  after it fails.
- **ComicInfo write-backs are queued.** Confirming a write-back stores the
  plan the person confirmed (the `expected` that `stamp::apply` compares)
  in a new table, `pending_stamp` (work, confirmed plan, queued at, last
  error), in this part's migration. It no longer rewrites at once. The
  write-back screen offers *Write now*, which is a flush of that one item.
- **Configuration → Pending writes** lists both kinds: the work, what would be
  written, since when, and the last error. *Write pending changes* flushes
  everything and reports each item. A failure stays pending with its error,
  so tightening the permissions too early loses nothing.
- **A queued write-back is never rewritten without the person seeing it
  again.** The flush re-plans and compares field for field against the queued
  plan, which is `stamp::apply`'s existing guard. On a mismatch (the archive
  grew, or a scan moved it since) the item is not written. It stays listed as
  *needs review*, with *Discard*.
- **A flush takes `library_job`**, the lock scans and rule saves already
  take. A rewrite therefore never lands mid-scan. Part A's counter and Part
  C's watcher then pick up the new digests like any other change.
- The CLI mirrors it in the house style: `taiyaku library pending` lists, and
  `taiyaku library pending --write` flushes.

O82 closes. A stale document is pending like an absent one, and a flush
writes the database's current correction over it. O81 (a document stranded
when the hub's own files change) is a different address problem and stays
open.

### 6.3 Testing

- A metadata edit against an unwritable root lists one pending document.
  Making the root writable and flushing writes it and empties the list.
- **The stale case**: a document already on disk, the root made unwritable, an
  edit, then a flush after making it writable. The file holds the new value.
- A queued write-back flushed against a writable directory rewrites the
  archive and moves its digest through the database. The same item, with the
  archive changed after queueing, is not rewritten and is listed as *needs
  review*.
- A flush against a still-unwritable root fails every item, leaves every item
  pending with its error, and leaves no archive half-written.
- **Permission tests must not run as root.** CI's containers run as root,
  which ignores mode bits; an existing note records that this has bitten
  before. Each such test checks its own effective uid and says so when it
  cannot run, rather than passing vacuously.

## 7. What this does not do

- **TLS**: NPM terminates it.
- **O57, O58 and O60** stay open and unchanged. Because taiyaku is public
  (§3.4), O57 (a burst of concurrent logins can allocate about 9.5 GiB of
  argon2 memory before the rate limiter bounds it) should be fixed soon after
  this part. O58 and O60 matter once other accounts exist.
- **Importing progress from Komga or Stump** (store spec §13.6, row J). Not
  asked for.
- **Refreshing works in argv mode** (O61), or resolving new works against
  Stump.
- **Evicting the cache** (O11). It lives on its own mapping, outside the
  backup, and grows only with reading.

## 8. What could not be measured from here

- Whether the watch probe succeeds through `shfs` inside a container, and what
  a rescan costs on the NAS with disks asleep. Part C measures both after B
  is deployed.
- Which forwarded headers NPM sends, and which address taiyaku sees it connect
  from. Part B reads both off the running deployment.
- Whether the CI job's `docker push` reaches `forge.daveynet.xyz` from the
  host daemon the runner automounts. Redextape's identical job does, which is
  evidence but not a measurement of this job.
