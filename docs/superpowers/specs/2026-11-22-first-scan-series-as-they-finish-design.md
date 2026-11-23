# First scan PR 2: volumes appear as a scan goes

Status: written 2026-11-22 from the owner's picks of that day (§5), and
built 2026-11-23: the filling derive and its early passes, the feed's
`written` marker and the admin's count of volumes ready, the reader's
scan line and its polling, and, after the drive, a restart's take-up of a
cut scan (§5 rulings 9 and 10, §7 "Server: a restart mid-job" and "A
cancel"). Two of §7's rules are built stricter than they are worded,
each only holding back more: rule 2 holds back a volume where any work
stands at its address, not only one `held_work` would take, and rule 3
names a file's directory by its held reading as well as its digest. The
early mode is a parameter of `derive_in`, the body `derive_watched` runs
in full, not of `derive_watched` itself. It closes O314. It is PR 2 of
the three that came from the owner's first NAS import (2026-11-16). PR 1
is `2026-11-15-first-scan-progress-and-cancel-design.md` ("FS" below). Also
cited: `2026-11-16-paused-directory-frozen-design.md` ("FZ") and
`2026-11-19-content-matching-volume-joins-design.md` ("VJ"). The recon
behind this document is `.superpowers/sdd/2026-11-22-o314/recon.md`.

Line citations are to main at 42b68c29. **Read** marks what I saw in the
code at the line given; **inferred** marks my reasoning from it. Paths
are shortened: `derive.rs`, `scan.rs`, `import.rs`, `progress.rs` and
`lease.rs` are in `crates/taiyaku-library/src/`, and `joins.rs` in its
`derive/`; `scan_jobs.rs`, `scan_feed.rs`, `match_pass.rs` and
`fingerprint_pass.rs` are in `crates/taiyaku-serve/src/`; `.ts` and
`.tsx` files are in `web/src/`.

## 1. Why

After PR 1 a first scan says which step it is in, counts what it does,
and can be cancelled. It still writes no work a reader can open until
the whole job's hashing has ended. The owner called a scan that fills in
nothing as it goes "pretty bad ux" (2026-11-16, during the first NAS
import).

On the owner's Manga/Volumes directory (539 GB, 2,097 `.cbz` files) that
wait is the hashing of every archive in full. The notes of 2026-11-16
put it at about 75 minutes: about 120 MB/s, read off an open file's
position on the NAS, over 539 GB. That is an **estimate**, not a timed
run; the deployment's own figure waits for the owner's redo of the
import.

While PR 1 was designed, the owner was asked (2026-11-16, 03:48 UTC,
session 2deb0e20): "One question: while a first scan of a big directory
is running, what do you want to do with what it has found so far? Your
answer decides how much PR 2 has to do. If you want to read series as
they land, derive has to work series by series. If seeing what's been
found and how far along it is would be enough, PR 2 stays much smaller."
On 2026-11-22 the owner answered: read series as they land (§5).

## 2. How it works today (at 42b68c29)

**The job** (read). `scan_jobs::run` (scan_jobs.rs:479) hands
`run_watched` (:530) the walk, `scan::scan_root_watched`, and the
derive, `derive::derive_watched(conn, Documents::Reconcile, fresh,
watch)` (:493-495). `run_watched` takes the job lock, then walks each
directory in turn (:604-645), each directory's entries saved in one
transaction (scan.rs:3231-3304), and marks it `deriving` (:620). Once
every directory is walked it takes the derive lease
(`lease_for_the_derive`, :656, fn :806) and runs the one derive under it
(:674). The lease allows one whole-store derive at a time across
processes and "is held from before a derive's first read to after its
last write" (lease.rs:1-6).

**The derive** (read, derive.rs). `derive_watched` (:694) reads the
rules and every root (`scan::roots`, ORDER BY id, scan.rs:109-116). A
paused root that `fresh` does not name is frozen (:715-718). Then:

1. `grouped` (:999) reads each root's entries in path order (`entries`
   :1325, or `frozen_entries` :1341 for a frozen root), admits them,
   works out claims, and sizes folders, so the bytes to hash are known
   before the first digest (:1070). It then classifies root by root,
   path by path (:1076-1101), hashing each entry `hashes` accepts (:1625:
   not chapter-only, holds pages, no digest) through `digest` (:1913):
   one read of the whole archive and an autocommitted UPDATE.
   `classify` (:1671) stages an entry only once it has a digest.
2. `place` (:1791) files the staged entries: a byte-identical file in
   another directory is a copy of one primary. A group is keyed
   `(series, volume bits, alone)` with no library (`GroupKey`,
   :1107-1140); directories sharing an edition key share one edition.
3. `joins::apply` (:734) applies stored pairs, then "directories follow"
   (`follow`, joins.rs:813). `reconcile_series` runs for every series
   named (:741, fn :1989). `Placed::of` reads every group (:759).
4. **Write**: one transaction per work, the cancel flag read between
   works (:764-789, `write_work` :2361). A work that `unchanged` (:2137)
   finds already written takes no write lock (O272).
5. **The tail** (:795-983): `move_left_behind`, `superseded` and
   `fold::absorb`, alignments, `digest_held_members`, `mark_missing`,
   `proposals::raise`, `hand::reconcile`.

So works become visible one at a time as Writing commits them (read),
but Writing starts only after the last digest of the whole store (read).
Because hashing goes root by root, a series with files in two
directories is complete only when the later directory's turn comes
(inferred).

**After the job** (read). A finished job moves the feed's `ended`
marker (scan_jobs.rs:198) and starts the fingerprint pass, which starts
no file while the job lock is held (`fingerprint_pass::waits`, :437-440;
test `no_file_starts_while_the_job_lock_is_held`). When it goes idle,
the match pass runs; if it stored a pair at or above the bar
(`matches::BAR` = 0.87, matches.rs:19) it derives, then moves `ended`
"so an open Library view reads the works it joined" (match_pass.rs:580-596).

**Pages** (read). One feed, `GET /api/library/scans` (scan_feed.rs:38),
answers from memory. `scanFeed.ts` reads the library status, the
libraries, the directories and the Library views again when a scan
leaves the feed or `ended` moves (`scanEnded` :186, `readScanAgain`
:203). The reader's Library view reads the feed only while it is empty:
"Only this reads the feed, so a tab polls it only while the view is
empty: a view drawing works has no scan to show, and a reader's tab left
on one would otherwise poll for as long as it is open" (Library.tsx:
188-194, `EmptyView` :195). The admin's step line is `stepLine`
(scanFeed.ts:390), which the library page, the directory row, the
Libraries table and the pass line draw.

**What a reader's state hangs on** (read). `progress` and `note` rows
are keyed by `work_id` and carry an `edition` and a `slot`
(`schema/001_initial.sql`:210, :224; `ON DELETE CASCADE` on the work).
A slug is minted once, when the row is inserted (`import::mint_slug`,
import.rs:624, called at derive.rs:2617), and "left alone rather than
reslugified" (import.rs:495). "The hub is fixed when the work is
created" (derive.rs:2062).

## 3. Goals

1. **Volumes appear as the scan goes.** While a job hashes, early passes
   write new volumes (works), a volume only once every file that becomes
   part of it is hashed (§7, "The early pass" and "When a pass runs"). A
   series fills in volume by volume.
2. **Series by series across directories.** Hashing takes a series' files
   in every directory together, so a volume with an edition in another
   directory waits only for that series' files there, not for the whole
   of the other directory.
3. **Shown once, whole.** A volume an early pass writes already has every
   edition and member it will have when the job ends. The changes it can
   still undergo are those today's code also makes to a volume written at
   the end: a byte-identical file elsewhere adds a copy (reach, no
   edition), and after the job the match pass can join it (VJ).
4. **Every scan.** A rescan's new volumes appear the same way. A change
   to an existing work waits for the full derive at the end, as today.
5. **Cancel keeps them.** The volumes written before a cancel stay
   visible, frozen as written. A file hashed but not yet written waits
   for Scan, as FZ has it.
6. **The reader's Library view follows.** A view that already shows works
   says a scan is running while one holds a library it shows, and reads
   its works again as volumes arrive, so new series appear without a
   reload.
7. **The admin sees the count.** During Hashing, the step line also shows
   how many volumes the job has written so far, a count the server keeps.

## 4. Non-goals and anti-goals

**Not in this PR:**
- **Batched walk saves, and resuming an interrupted walk** (the owner's
  pick 8). FS §4 put them in PR 2 because "PR 2 needs batched saves
  anyway, for series to appear as they finish". It does not: no work can
  be written before its files are hashed (FS §2), and hashing starts
  only once every directory of the job is walked (read, scan_jobs.rs:
  604-674). Batching the walk's commit would make `scan_entry` rows
  appear sooner, which no reader sees. What it would buy is a walk that
  survives a stop, and since O310 the walk no longer seeks every page:
  the walk-only bench read an archive in about 167 ms over this
  machine's NFS mount, against 4,475 ms before (status.md, O310's
  bullet). A restart re-walks, and the entries that did not change skip
  the resolve step (read, scan.rs:3200-3203).
- Changes to existing works during the job (pick 7): they wait for the
  full derive, as today.
- The series page and the home page polling the feed. They show the
  volumes written when they were read, and read again on navigation.
- A time estimate, a percentage, or a total of volumes to come (FS §4).
- Faster hashing: the hash is the work's identity (FS §2).
- Fingerprinting during the job. The fingerprints design rejected it
  because the job holds the job lock; early-written volumes are
  fingerprinted after the job, as every volume is today.
- Filling in for the CLI's `library scan`, a rules save, a removal, a
  rules preview, or the match pass's derive. Each keeps one derive (my
  call 11).

**Must not happen (anti-goals).** From FS §4, still in force:
- A figure on screen the server did not count.
- A cancel that leaves the store in a state a server stop at the same
  moment could not.
- A page that shows "Not read yet", or idle, while a job holds its
  directories. This PR adds the reader's non-empty Library view to the
  pages it covers (my call 9).

New:
- **A reader sees a volume vanish, move to another series, or change
  identity (slug, hub, content key) because an early pass wrote it.**
  A change the full derive or the match pass would make to the same
  volume written at the job's end is not this (pick 3).
- **A reader's progress or notes on an early-written volume are lost,
  moved, or shifted to another page by the job's full derive.**
- A half-written volume: one shown with some of its files or editions
  (FZ §3 goal 3's second half, kept).
- An early pass that hashes a file, opens one (sizing a folder lists its
  pages), or takes the write lock for a work it does not write.
- A library that differs from what one derive over the same files,
  rules and matches would make (VJ's anti-goal), beyond the two
  history-dependent cases accepted in my call 14.

## 5. Decided (owner, 2026-11-22)

The recon put eight questions; the owner answered "your picks" on
2026-11-22, taking the recommendations below.

1. **The original question is answered: read series as they land**, not
   only see what has been found and how far along it is.
2. **Volume by volume.** Each volume appears once every file that
   becomes part of it is hashed; a series fills in while the scan runs.
   Not series at once.
3. **Hashing goes series by series across directories** (Volumes/X and
   Raw/X together), so a volume with a raw edition in another directory
   waits only for that series' raws. A volume does not appear with one
   edition and gain another later in the same scan, except where the
   full derive at the end makes a change it would make today (content
   joins from the fingerprint pass, folds).
4. **Cancel keeps the volumes already written visible.** This reverses
   FZ §8 call 2, which reads: "A new file that was hashed before the
   cancel waits too (§3, goal 3), so a half-hashed first import shows
   nothing until Scan rather than the volumes that happened to be
   hashed." It follows the owner's Cancel ruling of 2026-11-15 (FS §5):
   "Works already written stay." The owner was told of the reversal on
   2026-11-22. FZ §3 goal 3's first half, "a first import cancelled
   while hashing shows none of its series", goes with it; its second
   half, "no half-written volume appears", stays.
5. **The reader's Library view that already shows works says a scan is
   running, and polls the feed while one holds a library it shows**,
   reading its works again as volumes arrive. Lane B (O259, O321, O322;
   a separate PR that merges first) owns `scanFeed.ts`'s end-of-scan step
   and `EmptyView`; this PR builds on it.
6. **Admin progress:** during Hashing the step also shows a count of
   volumes written so far, kept by the server. No percentage and no
   guessed total.
7. **Every scan**, not only first imports: early passes write works that
   do not exist yet; changes to existing works wait for the full derive.
8. **No batched walk saves in this PR** (§4 says why).
9. **A restart takes up a scan a stop cut, whatever the directory's
   schedule** (owner, 2026-11-23, answer "a" to: "After a restart, any
   directory whose scan was cut gets scanned again on the next tick,
   whatever its schedule. A directory you cancelled stays paused."). The
   drive found Off directories left `interrupted` with no job after a
   restart; main does the same. It is fixed in this PR because the
   owner's NAS directories are Off and every deploy restarts the server.
10. **Only a stop of this version marks a scan to take up** (the
    controller's call before review, 2026-11-23 06:48Z, reported to the
    owner; ruling 9 is the owner's own). Migration 032 adds the mark with no
    backfill, and the start marks nothing. A directory reading
    `interrupted` from before the migration says nothing of whether its
    scan is still wanted: the owner's Volumes/Raw import of 2026-11-16
    was stopped on purpose (schedule Off and a restart, before Cancel
    existed), and a backfill would restart a 539 GB import unasked at
    the first start after the deploy. So a scan main was running when
    this version is deployed is not taken up either; it reads
    `interrupted`, as before.

**Rulings this builds on:** FS §5 (Cancel pauses; it stops the whole
job after the file it is on; hashes and written works are kept; the
tail waits for the next derive; one shared polled feed); FZ §5 (paused
means frozen); the owner's ruling of 2026-11-04 that a byte-identical
file under another directory is a copy (derive.rs:1781-1787); VJ's
rulings (content joins at the 0.87 bar, automatic but separable; a
hand rule wins only by a literal `set_series`).

## 6. Approaches considered

- **A. Interleave inside the one derive.** Rework `derive_watched` to
  hash series by series and, after each, place, join and write what is
  complete, with the tail once at the end. No repeated reads. But
  `grouped`, `place`, `Placed` and `joins::apply` are all whole-set
  computations today (read), and copies by digest, `Placed::claims`,
  join components and follow tallies would each need an incremental
  form, in the module that holds the freeze, joins and fold invariants.
  Content PR 4's lane is also working in `place`. High risk.
- **B. Hash first in series order, and write in early passes that reuse
  the derive (chosen).** Hashing moves out of the derive into a loop over
  every entry to hash, in series order across directories. Every so
  often the loop runs the derive in an **early** mode: it hashes
  nothing, leaves out every group with a file still to hash, writes only
  groups no work holds yet, and runs no tail. At the end the full derive
  runs as today; it finds nothing to hash, finds the early works
  unchanged, writes the rest and runs the tail.
  - Each early pass leaves what a derive cancelled before its tail
    leaves, which FS §5 already accepts.
  - The derive's rules for building a work are reused whole. Its input
    filter has a precedent in `frozen_entries` (O320).
  - Costs: each pass reads the whole store's entries and regroups them
    (unmeasured on the NAS; my call 5 bounds the share of time passes
    take); a feed field and a count; the Library view polls.
  - Risks: the hold-back rules must be right for copies, editions in
    other directories and joins (§7); the final store can differ from one
    derive's where history decides (my call 14).
- **C. Show what has been found, not readable works.** List the series
  and file counts the walk saved, without hashing. Small and low risk,
  but nothing in it can be opened. The owner's pick 1 rules it out.

B, because it changes no rule of how a work is built: each early pass is
a derive the design already allows to stop before its tail, and the
full derive at the end is today's.

## 7. Design

### Server: the hashing queue

Hashing moves out of the derive into a loop the job's derive step runs
first (a new public function in `derive.rs`, the **filling derive**,
which `scan_jobs::run` passes in place of `derive_watched` at
scan_jobs.rs:493-495).

- **One reading of the input.** The part of `grouped` that reads,
  admits and claims each root's entries and sizes the folders to hash
  (derive.rs:1023-1075) becomes one function the queue and every pass
  call, so they read the store one way. Frozen roots come through
  `frozen_entries` as today.
- **What is queued:** every admitted entry `hashes` accepts, in every
  root the derive reads, not only the job's directories, since today's
  derive hashes the whole store's (read, :1070-1101). The bytes to hash,
  the folder sizing and its cancel checks are as today, so the Hashing
  count and total do not change.
- **Each entry's own key:** the `GroupKey` `place` would file it under
  were it not a copy. It is computed by the same code `place` keys groups
  with (extracted for both), so the two cannot disagree.
- **Order (my call 1):** by own key, as `GroupKey` orders (series title,
  then volume, then the volume-less key), then root id, then path. A
  series' files in every directory are hashed together, and its volumes
  in order.
- Each entry is hashed through `digest` (:1913), the cancel flag read
  before each, as `classify` reads it today.
- A group is **complete** once the queue has passed the last entry of its
  own key. The loop counts the groups completed since the last pass.

### Server: the early pass

The early pass is `derive_watched` in an early mode (a parameter, not a
copy of the function). It differs from the full derive in these places
and no others:

- **Same input.** The same rules, roots, `fresh` and so the same frozen
  set as the job's full derive (derive.rs:715-718).
- **`classify` hashes nothing.** An entry `hashes` accepts is not read.
  Its own key goes into a **waiting** set, and it is counted nowhere in
  the report (not vanished, not uncounted). No folder is sized.
- `place` and `joins::apply` run as today over what was staged, and
  `Placed::of` reads every group placed, written or not.
- **Which groups it writes.** A group is written only where all of these
  hold:
  1. Its key is not in the waiting set: no file whose own key is this
     group's is still to hash.
  2. No work holds it: `held_work` (derive.rs:2937) finds no row at its
     address, and no member of any work holds one of its digests. Its
     write is then an insert.
  3. None of its files, primaries or copies, sits in a directory (root and
     parent path) holding a file that a stored match row names, in any
     state (my call 4).
- `reconcile_series` runs only for the series of the groups it writes
  (my call 12).
- It writes those groups with `write_work`, in key order, with the
  job's `Documents` (`Reconcile`), so a hand document at a new work's
  address applies as it would at the end. The cancel flag is read
  between works.
- **No tail.** No folds, no `mark_missing`, no proposals, no
  `hand::reconcile`, no `digest_held_members`.
- It reports no step. It reports the count of works it wrote; the rest
  of its report is dropped.

**Why a volume written early is final** (inferred from the code read
above):

- Rule 1 means each edition holds every file it will hold at the job's
  end. The hub a work is created with is the group's hint, else the
  first-sorting edition key present (derive.rs:2058-2091), so it is the
  hub one derive would give, and so is the content key.
- Rule 2 means no existing work is rekeyed, re-membered or folded by an
  early pass.
- Rule 3 means the full derive's joins cannot touch it, and nor can
  "directories follow": `follow` moves a group only where its files'
  origin (root, parent directory, series) holds files that joined
  (joins.rs:788-878, read), and a file joins only through a stored pair.
- At the full derive the group is the same. `held_work` finds its row,
  `Placed::claims` names this group for its content key, and `unchanged`
  finds it written: no write lock, no rewrite (derive.rs:2380-2391). A
  copy hashed after the volume was written makes `unchanged` see a
  change, and `write_work` rewrites the work with the same members and
  content key, adding the copy's `member_root` row: no rekey, the same
  pages, so a saved slot still names the same page.
- `superseded` takes only a work "this derive did not derive"
  (derive.rs:2693). The full derive derives every early work (it
  is in `written`), so none is folded away.
- With hashing in key order, works and series rows are minted in about
  the order one derive mints them (its write loop goes in key order,
  :764), so ids and slug suffixes mostly agree with one derive's (my
  call 14 names where they do not).
- A series' first volume is hashed first (volume-less works before
  numbered ones, as `Option` orders), and the series cover falls back by
  volume order with NULL first (browse.rs:622, SQLite's order), so a
  series' cover does not change as it fills (inferred).

### Server: when a pass runs (my call 5)

After a group completes, the loop runs a pass if at least
max(10 s, 4 × the last pass's own duration) has passed since the last
pass ended, or since hashing began for the first. No pass runs after the
last digest: the full derive follows at once. A pass's cost is not
measured on the NAS and grows with the store; the factor keeps passes to
at most a fifth of the job's time whatever they cost (arithmetic: a pass
of p seconds is followed by at least 4p of hashing). The floor keeps a
small store from passing after every volume. Both are constants a test
can set; a test runs a pass after every completed group.

### Server: the lease and the write lock

The job takes the derive lease where it does today (scan_jobs.rs:656) and
holds it from the first digest through every pass to the end of the full
derive, as today it holds it over a derive whose first step is hashing.
No rules save, removal, CLI derive or match-pass derive runs between
passes, so each pass reads the store as this job left it.

A pass takes the store's write lock only inside `write_work`'s
transaction for a work it writes (`Nested::open`, derive.rs:2396), so a
page turn's progress save waits at most for one work's write, as in
today's Writing step. A group it does not write never reaches
`write_work`. Digests are written as today, one autocommit each.

### Server: Cancel

- The flag is read before each digest (as today), before each pass,
  between a pass's works, and before the full derive.
- A cancel inside a pass ends the job as cancelled in Hashing, the step
  the job shows. `run_watched`'s cancel path is unchanged (scan_jobs.rs:
  682-684, `cancel` :716): the job's directories are paused, each
  unfinished directory's last scan is written back (FS §10.2), and the
  report names the step.
- No last pass on a cancel (my call 7).
- What a cancel leaves: every digest made, every work a pass committed,
  nothing half-written (one transaction per work). A server stop at the
  same moment leaves the same (inferred), which is the anti-goal's test.
- **Afterwards the freeze keeps them, with no change to its code**
  (inferred). The paused directories are frozen at the next derive of
  anything; `frozen_entries` passes an entry whose digest a member under
  that root holds (its doc, derive.rs:1333-1340), so the early works
  stand as written (FZ §8 call 7). A file hashed but not written waits
  for Scan, as FZ §3 goal 3 has it for a file no work holds.
- The PR adds a dated note under FZ §3 goal 3 and §8 call 2 saying what
  §5 pick 4 reverses, as FZ's call 6 carries one.
- A cancelled first import now shows works while its directory reads
  "Never" and paused (FS §10.2 keeps its old `last_scan_at`). That could
  already happen when a cancel landed during Writing. The reader's view
  draws the works; the admin's page says paused until Scan is pressed.

### Server: a restart mid-job

The next start takes up a scan a stop of this version cut, whatever the
directory's schedule (§5, rulings 9 and 10). Digests and early works are
committed.

**The mark.** A job marks every directory it was asked for held
(`root.scan_held_at`, migration 032). A job that waits for the job lock
marks them with its `waiting`, in the same write (`scan::mark_queued`);
every job marks them again once it holds the lock (`scan::mark_held`).
It clears the mark as it ends, a cancel's end and an error's among them
(`scan_jobs::run_watched`). A stop or a panic skips the clearing, so a
mark the next start finds is a scan cut partway: waiting for the job
lock, in a walk that has recorded nothing, hashing, in a pass or in the
full derive. `last_scan_status` cannot carry it: a walk leaves that word
as the last scan left it until the walk records, so a cut walk leaves no
word of its own. The mark is written no earlier: a store another job is
writing would hold the job's start for the busy timeout before its
reservation said it waits, and the queued mark shares `waiting`'s write
so the job reads its cancel no later than it did.

**The window it leaves.** A stop after a job starts and before its first
mark (its queued write, or the job lock where that is free) leaves no
mark, nor does one between a press's or a tick's reservation and its
job's start. Nothing has been walked by then, so nothing is lost but the
scan asked for, as before this design: Scan or the directory's schedule
takes it up.

**A cancel** (the controller's call, 2026-11-23 07:04Z). The job writes
its pause only at its next read of the flag, after the file or the step
it is on, so the cancel route clears the held mark of the job's
directories as it accepts the cancel, after setting the flag
(`scan_jobs::unmark_cancelled`). A stop before the job's pause then
leaves nothing the next start takes up, and the directory reads its
last word, unpaused, until Scan or its schedule. The pause stays the
job's: clearing, not pausing, keeps a cancel that lands after the last
file from pausing a finished scan. The clearing runs one UPDATE per
directory, each able to wait out the 5 s busy timeout, so a cancel that
meets a whole-derive write, such as a rules save's, can answer slowly,
and a clearing that late could wipe the mark of a newer job of the same
directories (the controller's call, 2026-11-23 07:19Z, leaves it). The
clearing is best effort; where
the store refuses it, a stop before the job's pause is taken up at the
next start, as before it. A job that writes its first mark after the
clearing, a cancel accepted in the instant before its queued write or
before it holds the lock, reads the flag next, so only a stop between
that mark and the pause it then writes leaves one. An in-memory store
is not cleared: it does not outlive a stop.

**The start.** Before the bind, `interrupt_derives` (through
`scheduler::interrupt_cut_derives`) marks the `deriving` and `waiting`
directories `interrupted`, as before, and marks none held (§5, ruling
10): each of this version's cut scans holds its own mark already.
`schedule::scheduled` lists every held directory whatever its schedule,
never due by it, unless a cancel paused it (O313, O320). The startup tick
takes every directory listed, so it takes the held ones; a later tick
does not, so a mark a job's panic leaves in a running server waits for
the next start. A directory added Off with "Don't scan yet" (O275) has
had no scan and holds no mark. Migration 032 adds the column with no
mark on any directory, one reading `interrupted` included. The CLI's
scan marks nothing.

**The scan taken up** is an ordinary batch job (the filling derive), and
its end clears the mark, so a cut is taken up once. Its walk skips the
resolve step for every entry that did not change. The queue holds only
what was not hashed. The early works are works that exist, so the passes
leave them and the full derive finds them unchanged. The pages show it
as any job: the tables draw Scanning in place of the last scan's word,
and a directory's own page keeps its last scan, `interrupted`, beside
the job's step line until the job ends, as it keeps `cancelled` or
`failed` beside a pressed scan.

### Server: the paused freeze (O320)

Every pass reads the same frozen set as the full derive: `scan::paused`
less the job's `fresh`. A new, unhashed file in a frozen directory is in
neither's input, so a group whose other files lie in the job's
directories is written early without it, as the full derive writes it
(inferred). A batch job runs no paused directory and has no `fresh`.

### Server: the fingerprint pass and content joins at 0.87

Unchanged. No file is fingerprinted while the job lock is held (read), so
a new file has no fingerprint, and so no pair, until after the job. A
stored pair applies only at a derive (`joins::applicable`, joins.rs:
1139). Call 4 keeps back any directory whose files
the match table names: a rescanned directory with joins, and files whose
digests the match table already names from an earlier import. After the
job the match pass joins at or above the bar as today, and a join moves
the edition with its readers' state (`move_left_behind`, joins.rs:
617-620), as VJ's anti-goal requires.

On the owner's library this decides what pick 3's wait covers (inferred:
grouping is by series title, derive.rs:1107-1140). Where Raw/X's rules
give it Volumes/X's series title, the raw is an edition of the same
volume, and the volume waits for it. Where they do not, the raw is a
work of its own until the match pass joins it after the job, as today.

### Server: the report and the feed

- **The job's report** is the full derive's. Its `works` counts every
  work written or found standing (derive.rs:2387, 2624), so early works
  are in it.
- `progress.rs`: `Watch` (:49) gains a method, with a default that does
  nothing, through which the filling derive reports the number of works
  the job's passes have written so far. `Step` is unchanged: the step
  stays Hashing through a pass.
- `scan_jobs.rs`: `Snapshot` (:84-96) gains that count, and `Running`'s
  `Watch` (:270) stores it and raises a server-wide marker when it moves.
- `scan_feed.rs`: an admin's job (`job_json`, :188) carries the count,
  null until a pass has written a work. Every account's answer carries
  the marker beside `ended`; like `ended`, its value means only that it
  moved, and it is not filtered by grant (my call 8).
- The full derive at the end has nothing to hash, so its Hashing reports
  are not sent: the line goes from Hashing to Writing without "sizing
  files" or a zero total between.

### Web

**The feed client (`scanFeed.ts`).**
- `ScanFeed` gains the marker; `FeedJob` gains the count.
- When an answer's marker differs from the last answer's, and no scan
  ended in it (which already reads everything again), the client calls
  `invalidateLibraryViews` alone (pageReads.ts:96): the series, home and
  search views. The library status, libraries and directories are read
  again when the job ends, as today.
- `stepLine`'s hashing case adds the count when it is above zero, for
  example "Hashing · 12.4 GB of 539 GB · 41 volumes ready".
- Lane B owns `scanEnded` and `readScanAgain`. This goes beside them,
  written against main after lane B merges.

**The reader's Library view (`Library.tsx`).**
- Where the view draws works (the grid at :798 and the search hits at
  :649), a line above them says a scan is running while the feed
  lists a library the view shows: the library it is narrowed to, or any
  the status lists when it is not narrowed. An admin reads `scanLine`'s
  step line with its counts, anyone else the step word, as `EmptyView`
  draws it (:175-186). For example: "Scanning · more series appear as
  they are read".
- The view draws the feed whenever it is drawn, so the tab polls: every
  `SCAN_POLL_MS` while a job holds a library, every `FEED_IDLE_MS`
  otherwise, never while hidden (the client's rule; my call 9).
- `EmptyView`'s comment (:188-194) stops being true and is rewritten.
  Lane B also edits `EmptyView`, so this edit follows its merge.
- **The hand-off.** When the first pass writes a library's first works,
  the marker moves, the grid is read again, and the view swaps
  `EmptyView`'s Scanning for the grid and the scan line. The feed still
  lists the library meanwhile, so no "Nothing here" frame should show
  (O321's shape); the drive checks it.

**Admin pages.** The library page, the directory row, the Libraries
table and the pass line draw `stepLine` (read: LibraryPage.tsx:1257,
RootRow.tsx:1230, LibraryTable.tsx:143, PassLine.tsx:38), so they show
the count with no change of their own.

### Files touched (expected)

- `crates/taiyaku-library/src/derive.rs`: the queue, the early mode in
  `derive_watched`, `grouped` and `classify`, the shared input and
  own-key functions, the filling derive. Tests in `derive/tests.rs`.
- `crates/taiyaku-library/src/progress.rs`: the `Watch` method.
- `crates/taiyaku-serve/src/scan_jobs.rs`: `run`, `Snapshot`,
  `Running`. Tests in `scan_jobs/tests.rs`.
- `crates/taiyaku-serve/src/scan_feed.rs` and `scan_feed/tests.rs`.
- `crates/taiyaku-serve/src/lib.rs`: the marker on `ServeState`. Every
  test constructor that names `jobs_ended` (`rg 'jobs_ended: '`) needs
  the new field too, unless the marker shares a struct with `jobs_ended`.
- `web/src/scanFeed.ts`, `web/src/Library.tsx` and their tests.
- `docs/status.md` (O314 closed), FZ's notes, and `docs/notes.md` for
  the drive.
- For §5 rulings 9 and 10: `crates/taiyaku-library/src/schema/032_root_scan_held_at.sql`
  and `schema.rs`; `scan.rs` (`mark_held`, `unmark_held`, `mark_queued`); `schedule.rs`
  (`scheduled`); `crates/taiyaku-serve/src/scan_jobs.rs` (`run_watched`)
  and `scheduler.rs`; `scan_feed.rs` (`cancel`). Tests in
  `schema/tests.rs`, `scan/tests.rs`, `schedule/tests.rs`,
  `scheduler/tests.rs`, `scan_jobs/tests.rs` and `scan_feed/tests.rs`;
  `derive/tests.rs`'s hand rollback to schema 26 drops the new column.

## 8. Verification

Each test below is watched failing with its defect applied by hand,
then passing. Each fixture is one where the wrong behaviour shows.

**Rust** (`derive/tests.rs` unless named):
1. **Series order across directories** (goal 2). Two roots, `en` and
   `ja`, each holding series A and B, with ids so that root order and key
   order differ. A recording `Watch` sees A's files in both roots before
   any of B's, and volumes in order. Defect: hash root by root.
2. **A waiting group is held back** (goals 1, 3). A volume with a file
   in each root, one hashed: a pass writes no work. Hash the other, pass
   again: one work, both editions, the hub one derive gives. Defect:
   drop rule 1, and the work is written with one edition.
3. **Shown once, whole** (goal 3, the first new anti-goal). A whole job
   with a pass after every completed group, over a fixture with a
   multi-file volume, a volume in two roots, two volume-less files a
   rule titles alike, and a byte-identical copy in another root hashed
   after its primary's volume is written. Record each work's id, slug,
   series, hub, content key, editions and members when first written;
   after the full derive each is the same, a copy's `member_root` row
   aside. The final store equals one derive's over a copy of the
   fixture (works, slugs, series, editions, members, `member_root`).
4. **Progress and notes survive** (second new anti-goal). A progress row
   and a note on an early-written work keep their `work_id`, `edition`
   and `slot` through the full derive.
5. **Existing works wait** (goal 4). A rescan with one existing work's
   file changed, one unchanged, and one new volume: a pass writes only
   the new volume and takes no write lock for the others (the O272
   tests are the model); the changed work is rewritten only by the full
   derive.
6. **Joins and follow** (rule 3). A new volume in a directory a match row
   at or above 0.87 names, and a new volume in a directory whose other
   volumes joined another series: neither is written by a pass, and the
   full derive places them as today. Defect: drop rule 3, and the
   follower is written in its own series and moves at the end.
7. **A pass reads no file.** The waiting entries' files are removed after
   the walk, one a folder: a pass reads no bytes, sizes no folder and
   reports nothing vanished.
8. **Cancel** (goal 5, FS's anti-goal). Cancel after a pass wrote works,
   while the next file hashes: those works stand, the directories are
   paused, a later derive of another directory keeps them and writes no
   hashed-but-unwritten volume, and the store equals a stop's at the same
   point. Models: `a_cancel_while_writing_leaves_exactly_the_works_written`,
   and `scan_jobs/tests.rs`'s `cancelled_after_one_digest` and
   `a_cancelled_first_scan_reads_never_cancelled_and_paused_with_its_digests_kept`.
9. **Restart.** A job dropped after a pass, then run again, ends with the
   store an uninterrupted job leaves, and the early works keep their ids.
10. **Freeze.** A paused root outside `fresh` holding a new, unhashed
    file of a group whose other file is in the scanned root: the pass
    writes the group without it, as the full derive does.
11. **Cadence.** With a clock the test sets, passes run no closer than
    the floor and the factor allow, and only after a group completed.
12. **Feed** (`scan_feed/tests.rs`, `scan_jobs/tests.rs`; goal 7). An
    admin's job carries the count during Hashing, and the step stays
    `hashing` through a pass. The marker moves after a pass that wrote a
    work and not after one that wrote none. A reader's answer carries
    the marker and no count.

**Web** (vitest; assert per element, never a whole-page `toContain`):
13. A Library view drawing works, with the feed listing its library,
    draws the scan line; a moved marker reads the grid again and a new
    series' card appears; the job's end takes the line away.
14. The view polls while drawn: at the idle interval with no job, at
    the scan interval with one, not while hidden.
15. `stepLine` with the count, and without it before the first pass.
16. Lane B's `EmptyView` tests pass unchanged.

**By hand, on a live server built from the branch**, with a scratch
store and roots on a scratch copy of the local test library
`/home/davey/temp/manga-test`, never the NAS:
- A library of two directories with different edition keys that share
  a series by name, with distinct files (byte-identical ones are
  copies). One directory is scanned first, so the reader's view already
  draws works.
- A reader's tab on that library and an admin's tab; press Scan
  library. Watch the admin line's count rise during Hashing, the
  reader's scan line, new series arriving without a reload, a series
  filling volume by volume, and a volume in both directories arriving
  with both editions at once.
- Read a few pages of an early volume. After the job, the URL (slug) and
  the page are the same.
- Cancel during Hashing: the written volumes stay and the directories
  read paused; Scan finishes the rest. Restart the server during
  Hashing: the job resumes and the early volumes stay.
- Compare the end state with main's single derive over a second scratch
  store of the same copy (works, slugs).
- If the copy is hard-linked, the roots keep no documents, and a listing
  of `manga-test` with sizes and mtimes taken before and after shows
  nothing changed. The copy is deleted when the drive ends.

## 9. Left to the builder

- The names of the filling derive, the early mode's parameter, the
  `Watch` method, the `Snapshot` field, the marker and the JSON fields.
- Whether the queue lives in `derive.rs` or a module beside it.
- How rule 2's digest check is read: one query per pass or per group.
- The two cadence constants, within call 5's rule.
- The wording, apart from the count's noun, "volumes".
- How the view subscribes to the feed, within call 9.
- Where the marker lives, within call 8.
- If content PR 4 (copies and range packs, in `place`) merges first, the
  own key must follow its `place`; the planner reads `place` again then.

## 10. My calls in this document

1. **Hash order** is each entry's own group key as `GroupKey` orders it,
   then directory id, then path, across every directory the derive
   hashes. It makes pick 3's series-by-series order, puts a series'
   first volume first, and mints works in about the order one derive
   does.
2. **The unit a pass writes is a group** (one work), and a group waits
   while any file whose own key is the group's is still to hash.
3. **A pass writes only a group no work holds** (rule 2): none at its
   address, no member holding one of its digests.
4. **A directory holding any file a stored match row names waits, whole,
   for the full derive** (rule 3). It is wider than the joins need, and
   it is what keeps a join or a follow from moving a volume a reader has
   already seen. A first import with no match rows loses nothing to it.
5. **A pass runs** once a group has completed and max(10 s, 4 × the last
   pass's time) has passed since the last one ended; never after the
   last digest.
6. **The step stays Hashing during a pass**, and the line adds "N
   volumes ready". The full derive's empty Hashing is not reported.
7. **A cancel runs no last pass.** Cancel takes effect after the file the
   job is on (FS §5); a pass would delay it.
8. **A new marker, not `ended`,** moves after a pass that wrote works.
   `ended` would read the status, libraries and directories again on
   every page at every pass, and lane B owns that step. Like `ended` it
   carries no count and is not filtered by grant.
9. **The reader's Library view polls whenever it is drawn**, at the idle
   rate between scans, so a scan that starts while it is open shows
   within one idle poll (FS §4's anti-goal, now covering this view).
   This reverses the reason given at Library.tsx:188-194; each poll costs
   the session guard's read and a slide write that never waits (FS §7).
10. **An admin's scan line in the reader's view carries the counts**,
    anyone else's the step word alone, as `EmptyView` does today.
11. **Only scan jobs fill in.** The CLI's `library scan`, rules saves,
    removals, previews and the match pass keep one derive: no page
    learns of their writes as they go.
12. **A pass reconciles only the series of the groups it writes**, so no
    series row without works appears early.
13. **A pass applies hand documents** at a new work's address, as the
    full derive would, but runs no tail.
14. **Two differences from one derive are accepted**, both decided by
    history and neither moving a work once shown: a copy's primary
    where a byte-identical file sorts first in walk order but is hashed
    later (one derive would make the earlier file primary, under its
    own rules' group), and slug suffixes where two works share a slug
    base and complete out of key order.
15. **The freeze's code is unchanged**; FZ gains dated notes at §3 goal 3
    and §8 call 2.
16. **No row is opened for resuming a walk.** Pick 8 drops batched walk
    saves, and closing O314 says so.

**Settled after writing (owner, 2026-11-22: "your picks").** Two questions
this document left open:

17. **How often the reader's Library view polls the feed.** Every 5 s
    (`FEED_IDLE_MS`) while the tab is visible, and not at all while it is
    hidden, asking at once when it is shown again; every 1 s
    (`SCAN_POLL_MS`) while a scan holds a library the view shows. These
    are the feed client's own rates (`scanFeed.ts`), so call 9 adds no
    number of its own. The client polls once per tab, and it takes the
    faster rate while its answer lists any held library or, for an
    administrator, a running pass (`ask`); this PR keeps that rule rather
    than giving one view a rate of its own.
18. **Raw series titles.** On the NAS, the folder names of the four
    excluded raw series match their Volumes series (checked 2026-11-16);
    the other raws' titles are unverified. A raw whose title differs is a
    work of its own until the match pass joins it after the job, as today
    (§7, "the fingerprint pass and content joins at 0.87"). No change to
    the design.
