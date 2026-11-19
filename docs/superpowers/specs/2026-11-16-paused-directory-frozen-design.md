# A paused directory is frozen: no derive hashes it or changes its works until Scan

Status: approved by the owner, 2026-11-16.
This closes O320 with option (a), which the owner chose on 2026-11-16
as its own PR after the first scan's progress and Cancel (#256), and
the owner chose on 2026-11-16 to build (a) as "paused means frozen".
It also closes O270 (a stamp leaves the file's directory row on the
old digest), which the freeze would otherwise turn into a deleted
member; the owner chose on 2026-11-16 to fix it here, first.

## 1. Why

Cancel stops a scan's job and pauses its directories until Scan is
pressed (O313). The pause keeps the scheduler away, but not the hashing:
every derive covers the whole store (`derive::derive_watched` classifies
over `scan::roots`), so the next derive of anything hashes the paused
directory's files whose digests were cleared or never made. In the
owner's case, a first NAS import cancelled after its walk, that is
hundreds of gigabytes, hashed by:

- a Scan of another library, whose Hashing count then includes those
  bytes, and cancelling that job pauses that library's directories too;
- a scheduled batch;
- a rules preview, inside a transaction that holds the store's write
  lock for the whole hash, in no feed, with no Cancel, its digests
  rolled back at the end;
- a rules save, and a directory's or a library's removal.

Cancel should mean what it says: the paused directory costs nothing
until someone presses Scan, and nothing it held is lost meanwhile.

## 2. How it works today (at b1c842e0)

- A pause is `root.paused_at` (migration 026); `scan::paused` returns
  the paused ids. A pressed Scan directory or Scan library, and a CLI
  `library scan`, clear the pause of what they cover before their walk
  and derive (`scan_jobs::run_watched`, `cli/library.rs`), so a pressed
  scan's own directories are never paused when it derives.
- The walk saves each directory's entries. When an entry's size or
  mtime moved, it rewrites `kind`, `format`, `size`, `mtime`, `pages`,
  `title` and sets `sha256 = NULL` (`scan.rs`, the `(true, Some(_))`
  arm of the entry update). A new entry has no digest.
- The derive reads every root's entries (`derive::entries`), admits
  them, works out claims and nesting, and hashes each entry `hashes()`
  accepts: not chapter-only, `pages > 0`, `sha256 IS NULL`. The bytes
  it will hash are the job's Hashing total. A second site,
  `digest_held_members` in the derive's tail, hashes each undigested
  entry whose path a member names, outside the totals.
- An entry is staged only once digested. Its group is one
  `(series, volume)`; a byte-identical entry in another directory is a
  copy and goes under its primary's group. Writing a group replaces
  every edition it names (`import::replace_editions`, `Scope::Scan`)
  with exactly the members staged for it, and rewrites `member_root`
  from the primary and its copies (`derive::write_roots`).
- Nothing in a derive deletes a work because an entry is missing from
  its input; a member is marked missing only when no entry at its path
  (with pages) or with its digest remains in its directory
  (`browse::held_by`), which happens only when the walk deleted the
  entry's row.

So leaving a paused directory's undigested entries out of the derive
is safe for a first import, whose files make no works yet, but not for
a re-scan cancelled after its walk. There, a file that changed (or only
had its mtime moved) still belongs to a volume, and skipping it:

- drops its member when the volume's other files are written (a volume
  of several files, volume-less files sharing a title, one edition key
  across directories), since the replaced edition holds exactly what
  was staged;
- drops the paused directory's `member_root` row when it was a copy,
  so the library can lose its reach to the work;
- moves a work's primary to another directory's copy, or its hub
  edition, so the work is rekeyed and changes back on the Scan.

## 3. Goals

1. No derive hashes a paused directory's files: not a scan of another
   directory or library, the scheduled batch, a rules preview or save,
   a removal, or a CLI `library scan` of other directories. Neither
   hashing site does, and the job's Hashing total leaves those bytes
   out.
2. Until Scan is pressed, a derive reads a paused directory as its
   works last held it: every file that makes a member, or a copy,
   counts with the reading those works were written from, whether or
   not the walk has since seen it change. Its works, members, copies
   and library reach, and each work's identity, stay as they were.
3. A file in a paused directory that no work holds waits for the Scan,
   whether or not it was hashed before the cancel: a first import
   cancelled while hashing shows none of its series, and no
   half-written volume appears. A cancel that lands while works are
   being written keeps the works already written (a derive commits work
   by work, `derive_watched`'s write loop), and those stand as written.
4. A rules save regroups a paused directory's held files by the new
   rules, from their held readings, as it would if they had not
   changed.
5. A pressed Scan, or a CLI `library scan`, of the directory clears the
   pause and then reads it as today: it hashes what changed or is new,
   writes it, and the held readings it kept are dropped.

## 4. Non-goals and anti-goals

- **Not a freeze of removals.** A file the cancelled walk found gone
  has no entry; its member is marked missing as today, and returns with
  its work's position and note when the file does. Marking missing
  hashes nothing and deletes nothing. A renamed file shows missing
  under its old path until the Scan, which finds it at the new one by
  its digest. (My call; see §8.)
- **No change to what a pause is, who sets it, or what clears it**
  (O313's rules stand), nor to O318 (a Cancel behind a long rules save
  loses its pause).
- **No new words on a page.** The directory's page already says
  "Paused until Scan is pressed, since …". Saying how many files wait
  is not asked for.
- **Not O314** (series appearing as a running scan finishes them).
- **Anti-goal:** any path by which an entry passed over as paused reads
  as vanished, or by which a work, a member, a `member_root` row, a
  position or a note is deleted or rekeyed, because a directory was
  paused.
- **Anti-goal:** a held reading that is newer than what the works hold.
  A digest made after the last derive that wrote the file (hashed, then
  cancelled before writing) must not replace the reading the works
  hold.

## 5. Decided (owner, 2026-11-16)

- O320 option (a): the derive passes over a paused directory's entries
  that have no digest, neither hashing them nor counting them gone, and
  they are hashed when Scan is pressed.
- Its own PR, after #256.
- Of the two ways to keep works whole (below), **"paused means frozen"**.
- **O270 is fixed in this PR, as its first task** (over a PR of its
  own first, or a workaround inside the freeze that would leave O270
  open). See §7, "The stamp (O270)".

## 6. Approaches considered

1. **Paused means frozen (chosen).** As §3. A file that changed keeps
   the reading its works hold; new files wait; nothing is hashed. Needs
   the old reading kept when the walk clears a digest, since a copy's
   path is not stored on the member and a member's own row cannot give
   it back for a copy.
2. **Only new files wait.** A file no work holds waits; a changed file
   a work holds is hashed as today. Simpler and leaves existing works
   untouched, but a cancelled re-scan that saw many files change (a
   restore, a tool resetting dates) is hashed anyway by the next scan
   of another library — and a re-scan with a lot to hash is the one
   most likely to be cancelled.
3. **Hold back groups** (considered, dropped before the owner was
   asked). Skip each undigested file and leave unwritten every group
   it would be in. A copy's group cannot be known without its digest,
   a rules save changes the groups, and other libraries' new editions
   of those volumes would wait for the Scan too.

## 7. Design

### The stamp (O270), first

A stamp rewrites an archive and moves its member's digest to the new
one (`stamp::record_rows`), but leaves the archive's `scan_entry` on
the old digest until that directory's next walk. Today the first
derive in between writes the old digest back onto the member (O270's
row has the consequences for positions and notes). Under the freeze it
is worse: in a paused directory the next walk does not come, the entry's
digest is in no member's held set, so the frozen input leaves the file
out, and where the file shares a group's edition with files that are
written (the files of one volume, one edition key across directories),
that edition is rewritten without it and its member is deleted. A stamp
rewrites only some members of an edition (zip members only; no member
with copies), so this is a live path.

The fix is O270's own: the stamp also writes each new digest to the
archive's `scan_entry` row, **found by the member's root and its path
relative to that root, never by digest**, since two copies under one
directory share one. Where that row keeps a held reading equal to the
digest the stamp replaced, the held reading moves to the new digest
too, since the works now hold the new one. Whether the stamp also
writes the row's new size and mtime, so the next walk does not clear
the digest and hash the file again, is the builder's (§10); either way
nothing reads the old digest after the stamp.

### The held reading (schema and walk)

Migration 027 adds to `scan_entry` the reading the walk replaced:
`held_sha256`, and every other field the walk rewrites that the derive
reads (`format`, `pages`, `title`; the builder lists them from
`derive::entries` and `Entry`). Names are the builder's.

When the walk rewrites a changed entry, it keeps the old reading
there, **but only if no held reading is kept already**
(`COALESCE(held_sha256, sha256)` and the same for the others). The
digest it replaces may be one hashed and never written (a scan
cancelled after hashing it): keeping the first one is what keeps the
reading the works were written from (§4's second anti-goal). An entry
the walk sees unchanged keeps its digest and its held reading as they
are. Its page count is the exception, as today: the walk's unchanged
branch rewrites a directory's `pages` on every walk (`COALESCE(?3,
pages)`) and keeps its digest, so a folder volume whose chapter
subfolder gained a page reaches a frozen derive with its digest held
and its new page count, and its member's pages follow, as they would
today. Nothing is hashed for it and nothing is dropped. (My call; §8.)

*Note, 2026-11-18: the JPEG XL pages PR reversed this call, because no read of
a folder whose page count moved could hash to the old digest. A folder the
walk lists whole now gives up its digest as a changed folder does
(`Held::digest_outdated_by` in `scan.rs`), so while its directory is paused it
waits for the Scan, read by its held reading. A walk that could not list the
folder whole, at a size and mtime that did not move, leaves the count of a
folder that holds a digest as it is, whether its total is lower or higher, so
it brings no new count to a frozen derive; the next walk that lists the folder
whole compares its total with that count.*

The held reading is dropped when a derive that is not frozen for the
entry's directory **writes** its current reading: in the write phase,
not at classify, so a derive cancelled before writing keeps it. A
derive rolled back (a rules preview) drops nothing. Where the drop
happens in the write path is the builder's.

### The frozen input (derive)

The derive reads `scan::paused` once, less the roots its caller reads
fresh (below): those are the **frozen** roots. For each frozen root, it
builds the root's **held set**: the digests of the members whose
`member_root` names that root (the primary's root and every copy's).
Then, in `derive_watched`'s loop over roots, where it reads the root's
entries, a frozen root contributes in place of its rows:

- a chapter-only entry, as it is: it is never hashed, makes no work,
  and leaving it out would change the series rows and unattached
  counts the library shows;
- an entry whose current `sha256` is in the held set, as it is;
- else an entry whose `held_sha256` is in the held set, with its held
  reading in place of the current one;
- no other entry. A new file, hashed or not, and a changed file whose
  held reading no work holds, wait.

The freeze is in the derive's input only. `derive::entries` itself is
unchanged, because the proposals read it too
(`unattached_chapter_dirs` for `proposals::raise`,
`rel_paths_by_series` / `rel_paths_for_series` for an accept and the
merge proposals): they keep reading the directory as the walk saved
it. A proposal raised for a paused directory can be accepted; its
rules save regroups the held files at once and the new files at the
Scan.

Everything downstream (admission, claims and nesting, copies, groups,
`unchanged`, writing) runs on that input unchanged. Because the input
is what the works were written from, claims and nesting come out as
before, a copy stays a copy of the same primary, and `unchanged`
finds the paused directory's groups unchanged unless something outside
the directory changed them. `hashes()` accepts none of these entries,
so the bytes to hash leave them out without a change to the totals.

`digest_held_members` passes over paused roots.

`browse::held_by` and `mark_missing` read `scan_entry` directly and
are not changed: a paused directory's entry still holds the member at
its path (§2), so nothing reads as vanished.

### Who is frozen

Every derive, through `derive_watched`: the paused set is read inside
whatever transaction the caller holds, so a rules preview and save, a
removal, a job's derive, the scheduler's batch and the CLI see the same
pauses.

A pressed job and a CLI `library scan` unpause their own directories
before they derive (today's order), but a pressed job's unpause is
best-effort (`let _ = scan::unpause(..)` in `scan_jobs::run_watched`,
before the job lock and again once it holds it): a rules save holding
the write lock past the busy timeout refuses it. The job still walks
the directory (a pressed job's walk ignores the pause), which moves the
changed files' digests into held readings; were the derive then to find
the directory paused, it would freeze the very directory the owner
pressed Scan for, hash nothing in it, write nothing, report done, and
leave it paused. So **`derive_watched` takes the roots its caller
reads fresh**: a pressed job passes the directories it covers, the CLI
the ones it selected, every other caller none. A fresh root is never
frozen, whatever its pause says. A failed unpause then leaves only what
it leaves today: the directory stays paused, so the scheduler passes
over it until the next press. (My call; §8.)

## 8. Calls I made (for the owner to check)

1. A file the cancelled walk found gone is marked missing as today,
   not frozen (§4). Freezing removals would need the walk to keep
   deleted rows.
2. A new file that was hashed before the cancel waits too (§3, goal 3),
   so a half-hashed first import shows nothing until Scan rather than
   the volumes that happened to be hashed.
3. The old reading is kept in new columns on `scan_entry` (migration
   027), not read back from the member, because a copy's path is not
   stored on the member.
4. `derive_watched` takes the roots its caller reads fresh, so a
   pressed Scan whose best-effort unpause was refused still reads its
   own directories fresh (§7, "Who is frozen").
5. The freeze is in the derive's input, not in `derive::entries`, so
   proposals keep reading a paused directory as the walk saved it.
   Chapter-only entries pass through a freeze as they are.
6. A folder volume whose page count the walk rewrote without clearing
   its digest reaches a frozen derive with the new count, as today.
   *Note, 2026-11-18: reversed by the JPEG XL pages PR, since no read could
   hash to the old digest: a folder the walk lists whole gives up its digest,
   and a walk that could not list it whole, at a size and mtime that did not
   move, leaves its count alone; §7, "The held reading", has the note.*
7. Works a cancelled derive wrote before the cancel stand as written
   and are frozen as such (goal 3).

## 9. Verification

Each test below is shaped like the named neighbour and is watched
failing with the defect it names hand-applied, then restored.

- **O270: a derive between a stamp and the next walk keeps the new
  digest on the member**, and a position and a note written in that
  window stay anchored on it. Watched failing at the branch's base,
  before the fix. Neighbours: the stamp's tests in `stamp.rs` /
  `stamp/tests.rs`.
- **O270: the row is found by path, not digest.** Two copies of one
  archive under one directory, one stamped: only its row takes the new
  digest. Defect: the row found by the old digest.
- **A stamped file in a paused directory keeps its member.** A volume
  of two files in R, one stamped, R paused; a derive elsewhere leaves
  both members. Defect: the stamp's `scan_entry` write removed.

- **A cancelled first import is not hashed by another library's scan.**
  Directory R walked, two of its files hashed, cancelled (paused); a
  scan of library L: L's job hashes 0 bytes of R (its Hashing total and
  bytes hashed equal L's alone), writes none of R's works, and R's two
  digests remain. A pressed Scan of R then writes all of R's works.
  Neighbours: `scan_jobs`' `a_cancelled_first_scan_reads_never_cancelled_and_paused_with_its_digests_kept`,
  derive's `the_bytes_to_hash_leave_out_what_classify_does_not_hash`.
  Defect: the derive ignores the pause.
- **Frozen equals unchanged.** R holds a volume of two files (or two
  volume-less files sharing a title) and a copy of a file in another
  directory; a re-scan sees one file and the copy change, and is
  cancelled after the walk (variant: after the changed file was
  hashed). Another derive leaves R's works, members, `member_root`
  rows, content keys and member pages exactly as before the re-scan.
  Defects: skip without the held reading (the member drops); the held set
  leaves out copies (the `member_root` row drops).
- **A held reading survives a second cancelled scan.** Change, hash,
  cancel; Scan pressed, the file changes again, cancel; a derive
  elsewhere still holds the first reading. Defect: the walk overwrites
  a kept reading.
- **A pressed Scan reads the directory fresh and drops the held
  readings.** After it, no entry of R keeps a held reading and R's
  works carry the new digests. Defect: the held reading is not dropped
  (a later freeze would bring back a stale reading).
- **A cancelled derive keeps the held reading.** Defect: dropped at
  classify instead of the write.
- **Rules preview and save.** With R paused after a cancelled re-scan:
  a preview hashes nothing (bytes hashed 0) and reports no work of R
  orphaned or rehomed by the pause; a rules save that changes R's
  grouping regroups R's held files by their held readings.
- **The scheduled batch and a removal** of another directory hash none
  of R.
- **A pressed Scan whose unpause was refused reads its directory
  fresh.** The unpause made to fail by a trigger on `root.paused_at` that
  refuses every unpause, not by the write lock held past the busy timeout:
  both of the job's unpauses must be refused, which a held lock does only
  by timing (the plan's decision 7). The job hashes and writes R's files,
  and R stays paused. Defect: the job passes no fresh roots.
- **A cancel while writing.** A first import cancelled after some works
  were written: a derive elsewhere keeps exactly those works and writes
  none of R's others.
- **A frozen root keeps its chapter-only entries**: the series rows and
  unattached counts a derive elsewhere leaves are those before. Defect:
  the freeze drops them.
- **`digest_held_members` passes over a paused root.** Defect: it does
  not.
- **A file gone in a paused directory is marked missing** and returns
  with its work's position and note when it is back and Scan is
  pressed. Neighbour: `a_volume_whose_file_goes_is_marked_missing_and_returns_with_its_work_position_and_note`.
- The migration's own tests, shaped like 026's.

## 10. Left to the builder

- Column names, and which fields besides the digest the held reading
  needs (from `derive::entries` and `Entry`).
- Where in the write path the held reading is dropped, and as one
  statement per root rather than per entry where it can.
- How the held set is built: one query per paused root, as
  `digest_held_members` builds its per-root set (`member.path` has no
  index).
- Which tests are unit tests in `derive/tests.rs` and which go through
  `scan_jobs`, the CLI or `preview`.
- Whether the stamp also writes the row's new size and mtime (saving
  the next walk a hash of the stamped file), and from what stat.
- docs/status.md: O320 and O270 closed in the five-part ritual; any new
  row the build finds.

## 11. Corrections found while planning and building (2026-11-17)

The controller's calls, made after approval; the owner was told of the
first two before approving the plan.

1. **A paused folder volume still refuses a directory added inside it.**
   `scan::reads_as_one_volume` finds a folder member's entry by digest
   (O91), and `digest_held_members`, which gave a cleared entry its
   digest back, now passes over a paused root; so the refusal also
   matches the entry's held digest.
2. **A chapter-only entry inside a waiting entry that claims its
   subtree waits with it.** §7's first arm passes chapter-only entries
   as they are; one that lies inside a waiting entry of the same root
   that claims its subtree (a new folder volume, say) would otherwise sit
   under no claim, count as unattached and name a series no work holds.
   A chapter-only entry under no such entry passes as §7 says.
   Proposals still read `derive::entries`, where the waiting volume
   claims its chapters.
3. **A digest equal to the held one clears it.** Where a fresh derive
   hashes a file whose bytes did not change (only its mtime moved), the
   work is unchanged and not written, so the drop in the write phase
   never runs; `derive::digest` clears the held reading when the digest
   it writes equals it, which is safe at classify since the held reading
   is then the current one. A pressed Scan so drops the held reading of
   every entry whose work it writes or whose digest it finds equal. An
   entry whose current reading makes no member (unreadable, no pages,
   deferred or nested, chapter-only after a rules change, gone at
   hashing) keeps it; where its work was not rewritten, that is harmless,
   since it still equals what the works hold. Goal 5 and §9's "no entry
   of R keeps a held reading" hold for every other entry, not for these.
4. **A byte-identical file added to a paused directory and hashed
   before the cancel passes the freeze** (its bytes are held under that
   root) and joins the work until Scan: a visible addition, nothing
   lost. Accepted and filed as O336.
5. **Migration 027 fills in the held reading, once.** For every entry
   whose path a member names, through a directory of that member, and
   whose digest is NULL or differs from that member's, it holds the
   member's digest and pages as the entry's held reading, in every
   directory, paused or not, so a directory paused before the upgrade
   keeps its members. (Task 3a's review.)
6. **The one gap that leaves, accepted.** A copy in a second directory
   whose digest a walk cleared before the upgrade gets no held reading,
   since nothing stored pairs a copy's path with its member. While that
   directory is paused it can lose its library's reach to the work until
   Scan; the member and its digest stay.
7. **A paused directory's report counts can differ.** While a directory
   is paused, a job report's counts for it can differ from an unpaused
   derive's: `suppressed`, and the counts of rows beneath a waiting
   entry. O320's row in `docs/status.md` says how.
8. **The held reading is read before the digest.** §7's second and
   third arms are taken the other way round: an entry whose held digest
   is in the held set is read by its held reading even where its current
   digest is in the set too, since a changed file's new bytes, hashed
   before the cancel, can equal another member's under the same root.
   (The whole-branch review.)
9. **A nested directory takes over the outer rows' readings.** Its walk
   records the files it takes over as new rows, so each takes the outer
   row's reading as the walk would have kept it: its digest where size
   and mtime are the same, its held reading, and where the file changed
   and nothing is held yet, the reading the change clears. Without it,
   a nested directory paused before its first derive held nothing its
   works could be read by. (The whole-branch review.)
10. **A changed file whose held reading no work holds passes on its new
    bytes where they are held.** §7's last arm has it wait, but where its
    bytes, hashed before the cancel, are a member's the directory holds,
    the digest arm passes it as it is, as it passes item 4's added file;
    it adds no member. (The whole-branch review's re-review.)
11. **A copy a nested directory takes over is not reached through it
    if it is paused before its first derive.** Item 9's take-over moves
    only primaries to the nested directory, so a copy it takes over of a
    file whose primary is in another directory gets no `member_root` row
    for it there; where a cancel pauses the nested directory before its
    first derive writes the work, the copy is not reached through it until
    its Scan. The member and its digest stay. (The same re-review.)
