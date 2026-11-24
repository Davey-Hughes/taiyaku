# First scan: progress, Cancel, and a scan every page can see

Status: approved by the owner; built on `first-scan`.
This is PR 1 of three that come from the owner's first NAS import
(2026-11-16). PR 2 makes series appear as they finish. PR 3 is
setup: help for "How it is read", the directory picker's buttons, and a
compact picker list.

## 1. Why

The owner's first real import scanned one directory, Manga/Volumes: 539
GB, 138 series, 2,097 `.cbz` files on the Unraid share. For 27 minutes
the scan gave no sign of life:

- The reader's view of the library said "Not read yet".
- No page showed progress, only elapsed seconds on a page that had
  started the scan itself.
- No works appeared.
- There was nothing to press to stop it. The only way out was to set
  each directory's schedule to Off and restart the server.

The owner stopped it at about 35% and asked for the first-scan
experience to be fixed before the import is redone. The goal of this
PR: someone setting up a library can see that a scan is running, from
any page, can see how far it has got in figures the server actually
counts, and can stop it cleanly.

## 2. How it works today (at 210ec5e3)

**The job.** `scan_jobs::spawn` runs each job in `spawn_blocking`. The
three entry points are the Scan directory and Scan library presses and
the scheduler. A job takes the job lock, then walks each directory
(`scan_root_inner`, `taiyaku-library/src/scan.rs`) in three steps:

1. **List.** Read every directory and stat every file in the tree. No
   file is opened.
2. **Resolve.** For each entry whose size or mtime changed (on a first
   scan, every entry): `sniff` reads a 512-byte header, and
   `zip_flavour` opens the zip's central directory. Then
   `member_pages` opens the central directory again, and
   `page_names_from` (`taiyaku-read/src/pages.rs`) calls
   `zip.by_index(i)` for every entry. In zip 9.1.3 each `by_index` call
   seeks to that entry's local header and reads it, so it costs about
   one seek per page. No transaction is open during this step.
3. **Save.** One write transaction per directory inserts every
   `scan_entry` and `dir_state` row and commits.

After every directory has been walked, one **derive** runs over the
whole store (`taiyaku-library/src/derive.rs`):

1. **Classify.** Every member about to become part of a work is hashed
   in full (`member_digest`, an `fs::read` of the whole archive). Each
   digest is saved by its own UPDATE.
2. **Place.**
3. **Write.** One transaction per work.
4. **Tail.** Runs over the whole store: fold, `mark_missing`,
   proposals, hand reconcile.

No work exists until every undigested archive has been hashed. The
hash is the work's identity (`content_key`, store spec §4.1). That is
what keeps progress and notes attached when a file is renamed, so a
work cannot be made before its files are hashed.

**What the NAS showed.** These readings are from session 98e70579's
transcript, taken against the live DB:

- At 03:30Z, 27 minutes in, `/proc/<pid>/io` gave `rchar` 146,114,768:
  about 139 MiB read in total.
- `scan_entry` held 0 rows and there were 0 works.
- The WAL was 4,120,032 bytes. That is exactly SQLite's 1000-frame
  autocheckpoint size, so it says nothing about an open transaction.

So the job was still in the resolve step, bound by seeks on the share.
I estimate about 2 s per archive, or about 730 of 2,097 archives in 27
minutes. That estimate is inferred from the archive's position in the
sorted series list, not timed. The 539 GB of full reads for hashing had
not started.

**What a page knows.** `ServeState.scans` holds a `ScanProgress` for
each held directory: `started`, `done`, `report`, `library` and
`waiting`. It has no counters. The admin endpoints `GET
roots/{id}/scan` and `GET libraries/{id}/scan` report `running` and
`elapsed_secs`. A page polls every 1 s (`SCAN_POLL_MS`), but only for a
scan it started, or one already running when it mounted. A scan that
starts while a page sits idle is never seen (O273's shape).

The rule "Live progress is elapsed seconds and nothing else"
(`RootRow.tsx`, C4 screens design) was written because no counter
existed. It was not a ruling against progress. This PR keeps C4's
discipline that every figure on screen is one the server computed.

**The reader's empty view.** `EmptyState`'s `not-scanned` ("Not read
yet") is chosen when `LibraryEntry.scanned` is false. `scanned` is
`EXISTS (SELECT 1 FROM dir_state …)`, served by `/api/library/status`.
That endpoint has no running flag, so the view says "Not read yet"
during a first scan, and a reload changes nothing.

**Stopping.**
- Nothing can cancel a job. The `JoinHandle` is dropped, and no flag is
  checked inside the walk or the derive.
- A server stop abandons the job thread. A walk that had not committed
  is lost.
- The scheduler's first tick after a start takes every directory that
  is not Off (`scheduler.rs:89`).
- A never-scanned directory is always due (`schedule.rs:91`).

So a plain restart restarts the scan. A save to Off during a job goes
through but does not stop it, because the job does not read the
schedule again.

## 3. Goals

1. **A fast walk.** Listing a `.cbz`'s pages reads only its central
   directory: no per-page seek, and a read count that does not grow with
   the page count. The page list and its order stay exactly as they are
   today for every well-formed archive. `page_names_from` is the one
   place that decides page order, for listing and for reading alike,
   and it stays that place. Both paths list from the central directory.
   - **The difference for a damaged archive (my call).** Today, an
     entry whose local header fails to parse is dropped from the list.
     After this change it is listed, and fails when read. That would
     shift the page indices of such an archive.
   - **The guard on that call.** The plan counts the entries in the
     real library (the manga-test copy) that today's code drops for
     this reason. If there are any, the owner decides before the merge.
2. **Progress.** While a job runs, every page that shows it reports
   which step the job is in and a count the server keeps for that step:
   - Listing: files found so far.
   - Resolving: entries read, out of those to read.
   - Hashing: bytes hashed, out of bytes to hash.
   - Writing: works written, out of works to write.
   - The directory it is on, out of the job's directories.
3. **A scan every page can see.** A job is visible within one idle poll
   on:
   - the Libraries table,
   - a library's page,
   - a directory's page,
   - the reader's empty view of a library whose directories the job
     holds. That view says the library is being scanned, never "Not
     read yet".

   This holds whoever started the job: the scheduler, another tab, or
   this page. When the job ends, each of these pages reads again what
   it shows (Last scan, next scan, the reader's works), so O273 closes
   with this PR.
4. **Cancel.** While a job holds any of a library's directories, that
   library's Scan library button reads **Cancel**. Each held
   directory's Scan directory button does too. Cancel stops the job (§5).
5. **Paused.** Every directory of a cancelled job is paused:
   - The scheduler, its first tick after a start included, passes it
     over until someone presses Scan directory or Scan library on a
     scan that covers it.
   - The schedule setting does not change.
   - The library and directory pages say it is paused until Scan is
     pressed, and since when.

## 4. Non-goals and anti-goals

**Not in this PR:**
- Series appearing as they finish. Works still appear only after the
  job's hashing ends. That is PR 2.
- Saving the walk in batches, or resuming an interrupted walk. PR 2
  needs batched saves anyway, for series to appear as they finish.
  After Goal 1 a lost walk should cost little, and this PR measures
  whether it does.
- Setup help for "How it is read", and the picker's buttons and compact
  list. That is PR 3.
- O-rows for the chapter-only and issue gaps. They open after lane 2
  merges.
- A time estimate.
- Faster hashing (a partial hash, say): the hash is the work's identity.
- Cancelling a CLI scan from the web.
- Pushing updates to the page (SSE). Rejected below.

**Must not happen (anti-goals):**
- A figure on screen the server did not count. That rules out a bar
  over a guessed total, and an overall percentage that weighs the
  steps against each other.
- A cancel that leaves the store in a state a server stop at the same
  moment could not.
- Cancel quietly changing a directory's schedule.
- A page that shows "Not read yet", or idle, while a job holds its
  directories.

## 5. Decided (owner, 2026-11-15)

- **Three PRs**, this one first.
- **Cancel pauses.** Cancel pauses the job's directories until Scan is
  pressed, and the schedule stays as set. The owner picked this over
  "counts as a run" (the next scheduled time applies, but a restart
  starts the scan again) and over "sets the schedule to Off" (it
  quietly changes a chosen setting).
- **Cancel stops the whole running job.** A scheduled job can cover
  several libraries, so the page names them.
- **When Cancel takes effect:** after the file the job is on.
- **What a cancel keeps and loses:**
  - Hashes already computed are kept.
  - Works already written stay.
  - A directory whose walk had not committed loses that walk, as a
    server stop loses it today.
  - The derive's tail does not run. The next derive runs it, as after a
    stop.
- **How pages learn about scans: one shared scan feed that every page
  polls** (approach A below).

## 6. Approaches considered (for how pages learn of a scan)

- **A. One shared feed that every page polls (chosen).** One endpoint
  lists the running jobs with their step, counts and directories. Pages
  poll it every 1 s while a job runs, and every few seconds while idle.
  This follows the polling the pages already do.
- **B. Server-sent events.** Updates arrive at once and nothing polls
  while idle. But the project has no push machinery, the stream needs
  reconnect and login handling on a long-held request, and the README's
  nginx example would need buffering turned off. Too much for a count
  that changes once a second.
- **C. Progress fields on each page's existing poll.** Smallest diff,
  but a page still misses a scan it did not start, and the reader's
  empty view needs polling of its own anyway.

## 7. Design

### Server

- **Job state.** A job's in-memory state gains:
  - its step,
  - the counts for Goal 2,
  - its directories and their libraries,
  - a cancel flag that the job's thread can read.

  The walk and the derive report their counts through a sink passed in
  from serve, because `taiyaku-library` must not depend on
  `taiyaku-serve`. A job waiting for the job lock counts as a job in
  the step "waiting".
- **Cancel checks** run:
  - between entries in the resolve loop,
  - between directories,
  - between digests in classify,
  - between works in write,
  - before the tail.

  A job cancelled while waiting never starts. A cancelled job ends with
  a report that says it was cancelled and at which step.
  - **The data** a cancel leaves (rows, digests, works) is what a stop
    at the same point would leave. That is the anti-goal's test.
  - **The status word (my call):** each directory the job had not
    finished gets a new status word, `cancelled`.
    `scan::interrupt_derives` leaves that word alone at the next
    start.
  - **`last_scan_at`** keeps its old value, so a never-scanned
    directory still reads "Never". The page shows when it was
    cancelled and that it is paused until Scan is pressed.
- **Pause** is saved per directory: a nullable column on `root` holding
  when it was paused. A migration adds it, and it must run on SQLite
  3.40.1.
  - `schedule::scheduled` leaves paused directories out, and so the
    first tick does too.
  - An explicit scan that is accepted clears the pause on the
    directories it covers. That means a press of Scan directory or Scan
    library, or the CLI's `library scan` (my call: the CLI is how an
    operator presses Scan). A refused press clears nothing, and the
    scheduler never runs a paused directory at all.
  - Removing a directory removes its pause along with the row.
- **The feed** is one endpoint whose handler answers from memory only.
  - **The store cost of a poll.** Like every signed-in request, a poll
    passes the session guard. That costs one read, plus a slide write
    made through `without_waiting`, so it never waits on a scan's
    write lock (O272). The handler adds no store access of its own.
  - **Grant filtering** uses the `Principal` the guard already holds.

  It has two forms:
  - **For an admin:** every running job with its step, counts,
    directories (id and path), libraries, elapsed time and whether a
    cancel is pending.
  - **For any other account:** only the libraries it can see that a job
    holds, and each one's step, with **no counts and no directory
    paths** (my call). A job's counts cover every library it holds, and
    hashing and writing cannot be split by library, so counts would
    show a reader figures from libraries they have no grant to.

  *O521: a server task, the registry's refresher, may raise the feed's
  `ended` for a scan another process ran, seen by a directory's last scan,
  the derive lease or the store's structure version (`cli_watch`), so the
  handler still answers from memory only.*
- **Cancel endpoint:** admin only, it cancels a job by its id. Pressing
  it twice is harmless, and cancelling a job that has already ended
  answers the job's actual result.

### Web

- **One feed client per tab.** A module-level store polls once for
  every mounted consumer. It polls every 1 s while any job runs, every
  5 s while idle (my call, tunable), and not at all while the tab is
  hidden. When a job disappears from the feed, it invalidates:
  - the library status,
  - the library and directory page reads,
  - the series, home and search views (`invalidateLibraryViews`).
- **Library page:**
  - Scan library becomes Cancel while a job holds any of its
    directories.
  - The step and its count replace the elapsed-only line.
  - While a cancel is pending, the page shows "Cancelling…".
  - After a cancel, the page says the library's directories are
    paused until Scan is pressed.
- **Directory page:** the same, for Scan directory.
- **Libraries table:** a running job shows as "Scanning" in the
  library's row, with its step's count.
- **Reader's empty view:** while a job holds any of the library's
  directories, a "Scanning" state replaces every empty state of the
  unfiltered view. That includes the "nothing here" state, which the
  view reaches as soon as one directory's walk is saved and which
  would otherwise last the whole hashing step. The state shows the
  step's count for an admin, and the step alone for others.
  - **With a filter or a search active** (my call), the view keeps its
    no-match state and adds a line saying a scan is still running.
  - **When the job ends,** the view reads again.
  - **A paused, never-scanned library** shows "Not read yet" to
    readers (my call: pausing is an admin's concern).
- **No confirmation dialog for Cancel** (my call). A cancel costs at
  most one directory's unsaved walk, and Scan resumes the rest.
- The per-page polling that the feed replaces is deleted, not left
  running beside it.

## 8. Verification

- **Rust:**
  - The fast walk's read count for an archive does not grow with its
    page count (the strace-style tests are the model).
  - Page lists are identical to before for the existing fixtures.
  - A cancel at each step leaves the store as a stop at that point
    would.
  - A paused directory is skipped by the first tick and by later ticks.
  - An accepted press clears the pause; a refused press leaves it.
  - The feed's JSON in both forms, and that its handler makes no store
    access beyond the guard's.
  - A CLI `library scan` of a paused directory clears its pause.
  - A `cancelled` status word survives a restart.
- **Web:**
  - Each of the four pages shows a job it did not start within one idle
    poll.
  - The Cancel swap, the cancelling state and the paused line.
  - The reader's empty view shows "Scanning" and reads again at the end.
  - A single poll serves several consumers.
- **By hand, on a live server** serving the manga-test copy or a
  scratch library:
  - Start a first scan and watch the steps count on each page.
  - Cancel during the walk and again during hashing.
  - Restart the server and confirm the paused directories stay idle.
  - Press Scan to resume.
- **Walk timing.** A walk-only bench times listing the same Volumes
  files over this machine's NFS mount of the share, before and after
  Goal 1 (my call):
  - The bench opens files read-only and runs no server, so nothing is
    written and no read-only mount is needed.
  - The figure is labelled as NFS, because the seek cost belongs to
    the substrate.
  - The deployment's own figure comes from the owner's redo of the
    import. The merge does not wait for it.
- **Status rows.** `docs/status.md` rows open from O310, since lane 2
  holds O301–O309:
  - Rows this PR closes: the slow walk, the reader's "Not read yet"
    during a scan, progress, and Cancel with pause. O273 closes too.
  - Rows opened for later: PR 2's series as they finish, and PR 3's
    three setup items.

  Whichever of this PR and lane 2 lands second redoes status.md's
  counts.

## 9. Left to the builder

- The endpoint paths and JSON field names.
- The sink's form.
- How the cancel flag is threaded.
- The exact wording, apart from **Cancel** and "paused until Scan is
  pressed".
- The idle poll interval, within "every few seconds, never while
  hidden".
- Whether `zip_flavour` and `member_pages` share one opening of the
  central directory.
- Whether O257, O259 and O260 fall out of the feed. Close one only with
  a test that fails on main.

## 10. Corrections found while planning (2026-11-15)

The plan checked this design against the code and found six places
where the code differs. None changes a decision in §5.

1. **The fast walk needs a buffered reader.** zip 9.1.3 parses each
   central-directory entry with separate reads. Goal 1 therefore reads
   the archive through one 64 KiB buffer. "A read count that does not
   grow with the page count" holds while the central directory fits in
   that buffer.
2. **A cancel has to write the old `last_scan_at` back.** `scan_root`
   writes `last_scan_at` as each directory's walk commits, before the
   derive.
3. **`scanned` also requires a `last_scan_at`.** It checks only for
   `dir_state` rows (`browse::status`). Without this, a cancelled first
   scan whose walk had committed would read as scanned.
4. **The feed carries an `ended` marker.** A job that starts and ends
   between two idle polls never shows in the feed. The marker changes
   whenever a job ends, so pages still read again.
5. **The waits must check for a cancel.** Waiting for the job lock and
   for the derive lease both block, so they become polled waits (every
   100 ms) that check the cancel flag between tries.
6. **Holds that are not scans keep their own polls.** These are
   removals and held moves. The feed covers scans only.
