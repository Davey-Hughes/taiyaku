# Libraries, F2b (Move): Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An administrator moves a folder from one library to another, or to
a new one, from the folder's row on Configuration → Libraries. The folder's
works keep their rows, addresses and reading history; a work the move divides
is split with each edition's history following it; and a move that cannot
keep a reader's place is refused, naming the volumes, before anything is
written.

**Architecture:** Move is a new writer in front of the derive, and the derive
does not change. A new `relocate` module in `taiyaku-library` re-files the
works whose hub edition is under the folder, changes the folder's library,
derives, and then gives each divided work's misfiled editions to the work the
derive made for them, through a new `fold::rehome_editions` that moves the
edition rows themselves with what readers put on them. The whole of it runs
in one transaction, which a rehearsal rolls back and a move commits. It is
refused twice over: by checks that read the store before anything is written,
and by a verdict taken after the derive, which compares the store with a
census taken before. `taiyaku-serve` puts one route over it, and the web app
one dialog on the folder row.

**Tech Stack:** Rust 2024, rusqlite against the system SQLite, axum 0.8;
React 19, vitest, biome, pnpm.

## Read this first

**This plan was written on 2026-10-06, before F2a was built.** Tasks 1 to 3
touch only `taiyaku-library` and stand as written. Tasks 4 and 5 build on
files F2a creates (`libraries_api.rs`, `libraries.ts`, `LibraryCard.tsx`,
`RootRow.tsx`'s props) and name them as F2a's plan
(`docs/superpowers/plans/2026-10-06-libraries-f2a-admin.md`) names them.
Before Task 4, read those files on `main` as merged and correct this plan's
names against them. If F2a shipped something this plan contradicts, stop and
ask.

## Global Constraints

- **Branch:** `folder-move`, cut from `main` after F2a has merged.
- **The spec is `docs/superpowers/specs/2026-10-05-libraries-design.md`.**
  Read "Decisions the user made, 2026-10-06, for F2", §6.2 and §12's Move
  list before Task 1, and O192's row in `docs/status.md` with "What a folder
  moved between libraries does" in `docs/notes.md`. Where this plan and the
  spec disagree, stop and ask. The plan's own disagreements are under "Spec
  corrections needed" below and are for the user, not for you.
- **No attribution lines anywhere.**
- **Every command runs from `/home/davey/projects/taiyaku`.** Never `cd`: this
  shell's `cd` is zoxide. Web commands are `pnpm -C web …`. The Bash tool runs
  zsh with `noclobber`: write scratch output to a new file name or use `>|`.
  Never name a shell variable `path`. Brace a variable before a colon.
  `grep` is a shell function that skips logs it takes for binary: write
  `command grep -a`.
- **Throttle every Rust test run**: `cargo test … -- --test-threads=8`, and
  `NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8` for the gate.
- **Every new test is watched failing with its defect put back by hand.** Copy
  the production file aside first, make the edit the step names, run the
  test, read the failure, restore with `cp` and prove the restore with `cmp`.
  Put the failure's first line in the task's report. If the named defect does
  **not** make the test fail, stop and report. Never `git stash`.
- **A hand defect in SQL changes what the statement means, never how many
  parameters it binds.**
- **A Rust task is done only when whole crates pass**:
  `cargo test -p taiyaku-library -p taiyaku-serve -p taiyaku-cli --
  --test-threads=8`, `cargo clippy --workspace --all-targets --all-features
  -- -D warnings`, `cargo fmt --all -- --check`.
- **A web task is done only when whole projects pass**: `pnpm -C web run
  lint`, `pnpm -C web run typecheck`, `pnpm -C web exec vitest run --project
  node --project jsdom --reporter=default`, `pnpm -C web run test:browser`.
- **A class a component writes has a stylesheet rule in the same commit.**
  `web/src/classNames.test.ts`, in the node project, fails for a class no
  selector names, so Task 5's node project is red until its Step 4 has added
  the rules for `move-root`, `move-counts` and `move-volumes`.
- **"Pre-existing" means it fails the same way on `main`**, checked in a
  worktree at the commit this branch was cut from.
- **No figure in a code comment that a later change would have to correct.**
  Comments say what the code does and why it must, in the surrounding files'
  voice, and do not narrate this plan.
- **The snippets below are a starting point.** The interfaces are exact. The
  fixtures' plumbing is yours to fix against the code as it is. If a step's
  *claim about existing code* is wrong, stop and report.
- **What this plan says the derive does was probed, not only read.** On
  2026-10-06, at `062d09ef` (`main` `d963659a` plus documents), each
  arrangement Task 3 builds was made by hand in a throwaway test and derived:
  `.superpowers/sdd/folder-move-evidence/plan-probe-062d09ef.log`, with the
  probe's source beside it. What was **not** run is the move itself, which
  did not exist: `relocate`'s own statements, `fold::rehome_editions`, a
  folder that keeps documents, and a work placed in a series by hand. Where
  the store does something this plan does not say, stop and report what it
  did: do not bend the test, and do not change `derive.rs`.
- **`derive.rs`, `import.rs` and `fold::absorb` do not change.** A defect
  found in them goes in the report for `docs/status.md`.
- **Evidence goes in `.superpowers/sdd/folder-move-evidence/`** (`$EVID`),
  gitignored, never under `target/`. Stamp each log with its commit.
- **Never touch `/home/davey/temp/manga-test/`.** Task 6 copies its
  `library.db` and proves the original byte-identical afterwards.

## Spec corrections needed

The user should rule on these before or during execution. The plan is written
to the reading given here.

1. **A split drops the alignments between the editions it separates.** §6.2
   says each edition's positions and notes go with it. An alignment names two
   editions of one work, and after a split they are editions of two works in
   two libraries. The rehearsal counts them (`alignments_dropped`) and the
   dialog says the number. An alignment between two editions that move
   together is kept.
2. **Which series a moved work lands in.** §6.2 says: where the target holds
   a series of the title, the moved works join it and its titles stand. The
   plan adds the two cases the spec leaves open. Where the target holds none
   and every work of the series moves, the series row moves too, with its
   alternative titles and external ids. Where some works of the series stay
   behind, the series stays with them and the target gets a new series of
   that title, without the alternative titles.
3. **A third refusal.** A work whose hub edition is under the moved folder,
   whose other editions stay, and whose volume the target already holds
   cannot be split and joined in one move: the work row would have to go to
   the target and also be folded away there. It is refused with the others.
4. **"Would not have reported before the move" is measured.** The move
   derives once before it changes anything, so that what the derive after it
   reports is the move's doing. A rehearsal therefore costs two derives.
5. **"A reader's place kept" is tested by counting.** Beside the derive's own
   report, the move is refused if the store holds fewer reading positions,
   notes or ground-truth pages after it than before.
6. **A refusal's body lists the volumes** (`"volumes": [...]`) beside its
   sentence, which names the first few and how many more.

## Decided while planning

- **The work row goes with its hub edition**, as the user ruled on
  2026-10-06. Mechanically: every work whose hub edition is under the folder
  is re-filed to the target before the derive, and a derive finds a work by
  its content key within its library (`import::scanned_in`), which is a hash
  of the hub edition's members. So the row, its slug, its hand columns and
  its document's address are untouched.
- **A split moves the edition row, not a copy of it.** The derive makes a
  fresh, empty edition on the other side; `fold::rehome_editions` deletes
  that one and re-parents the old row in its place, so spread flags, credit
  flags, labels and declared chapters stay on it and every slot still names
  the page it named.
- **A series left holding no work by a move is left as the store leaves any
  such series.** The derive counts it (`series_holding_no_work`). The probe
  shows one: after a volume joins the target's work, the folder's old series
  stands in no library with its title.
- **A derive gives each edition it writes a new row id**, and leaves an
  edition it does not name as it was (the probe). So the edition a divided
  work still holds after the derive is the old row, with its flags, and that
  is the row `rehome_editions` moves.
- **No CLI command.** The spec's CLI parity bullet does not name one.
- **A move holds `ServeState::library_job`** for its length and answers 409
  where a scan holds it, for a rehearsal and a move alike.

## File structure

**`crates/taiyaku-library/src/`**

- `fold.rs`: `rehome_editions`, `Rehomed`, `Rehoming`. Its three row movers
  are called from it and stay private.
- `relocate.rs` (new) and `relocate/tests.rs`: `Target`, `Moved`,
  `MoveError`, `move_cost`, `move_root`.
- `scan.rs`: `only_grant_accounts` cut out of `removal_refusal`, for the
  move's own sentence.
- `lib.rs`: `pub mod relocate;`.

**`crates/taiyaku-serve/src/`**

- `libraries_api.rs`: `move_folder` and its JSON.
- `routes.rs`, `auth/tests.rs`: the route lists.

**`web/src/`**

- `libraries.ts`: `MoveTarget`, `MoveAnswer`, `moveCost`, `moveFolder`.
- `MoveRoot.tsx` (new) and its test: the dialog.
- `RootRow.tsx`, `LibraryCard.tsx`, `Libraries.tsx`: a `moveTo` prop down to
  the row.
- `styles/admin.css` and its tests.

---

## Task 1: `fold::rehome_editions` moves edition rows between two works

**Files:**
- Modify: `crates/taiyaku-library/src/fold.rs`
- Test: `crates/taiyaku-library/src/fold/tests.rs`

**Interfaces:**
- Consumes: `fold`'s private `move_positions`, `move_notes`, `move_truth`,
  `digests_by_edition`; `crate::Nested`.
- Produces:
  ```rust
  /// What [`rehome_editions`] carried.
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
  pub struct Rehomed {
      pub positions: usize,
      pub notes: usize,
      pub truth_pages: usize,
      /// Alignments between a rehomed edition and one that stayed.
      pub alignments_dropped: usize,
  }

  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum Rehoming {
      Done(Rehomed),
      /// Nothing was moved. The sentence says why.
      Refused(String),
  }

  /// Give `into_work` the edition rows `keys` of `from_work`, in place of
  /// its own editions of those keys.
  pub(crate) fn rehome_editions(
      conn: &Connection,
      from_work: i64,
      into_work: i64,
      keys: &[String],
  ) -> Result<Rehoming, String>;
  ```

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-library/src/fold/tests.rs`, importing `Rehomed`,
`Rehoming` and `rehome_editions` from `super`. `open`, `seed_work`,
`seed_note`, `seed_truth` and `put_progress` are that file's own; re-read
their signatures.

```rust
/// A work divided by a folder move, as a derive leaves it: work 1 still holds
/// `raw` (its hub, which stays) and `en`, and work 2 is the fresh work the
/// derive made for `en`'s archives on the other side.
fn divided_pair() -> (Connection, i64, i64) {
    let conn = open();
    seed_work(&conn, 1, "stays", &[("raw", &["aaa"]), ("en", &["bbb", "ccc"])]);
    seed_work(&conn, 2, "fresh", &[("en", &["bbb", "ccc"])]);
    (conn, 1, 2)
}

fn rehomed(conn: &Connection, from: i64, into: i64, keys: &[&str]) -> Rehomed {
    let keys: Vec<String> = keys.iter().map(|k| (*k).to_string()).collect();
    match rehome_editions(conn, from, into, &keys).expect("rehoming") {
        Rehoming::Done(moved) => moved,
        Rehoming::Refused(why) => panic!("refused: {why}"),
    }
}

fn refused(conn: &Connection, from: i64, into: i64, keys: &[&str]) -> String {
    let keys: Vec<String> = keys.iter().map(|k| (*k).to_string()).collect();
    match rehome_editions(conn, from, into, &keys).expect("rehoming") {
        Rehoming::Done(moved) => panic!("rehomed: {moved:?}"),
        Rehoming::Refused(why) => why,
    }
}

fn edition_id(conn: &Connection, work: i64, key: &str) -> i64 {
    conn.query_row(
        "SELECT id FROM edition WHERE work_id = ?1 AND key = ?2",
        params![work, key],
        |r| r.get(0),
    )
    .expect("the edition")
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).expect("counts")
}

/// The row moves: the edition `into` ends up holding is the one `from` held,
/// by id, with the spread flag and the label a person put on it.
#[test]
fn a_rehomed_edition_is_the_same_row_with_what_was_on_it() {
    let (conn, from, into) = divided_pair();
    let old = edition_id(&conn, from, "en");
    conn.execute("UPDATE edition SET label = 'Viz' WHERE id = ?1", [old])
        .expect("a hand label");
    conn.execute(
        "INSERT INTO page_flag (edition_id, kind, page) VALUES (?1, 'spread', 1)",
        [old],
    )
    .expect("a spread flag");

    rehomed(&conn, from, into, &["en"]);

    assert_eq!(edition_id(&conn, into, "en"), old, "the row itself");
    assert_eq!(
        count(&conn, &format!("SELECT count(*) FROM edition WHERE work_id = {from}")),
        1,
        "and the work it left keeps only what stays"
    );
    assert_eq!(
        count(&conn, "SELECT count(*) FROM edition WHERE key = 'en'"),
        1,
        "the fresh edition it replaced is gone"
    );
    assert_eq!(
        count(&conn, &format!("SELECT count(*) FROM page_flag WHERE edition_id = {old}")),
        1
    );
    assert_eq!(
        count(&conn, &format!("SELECT count(*) FROM member WHERE edition_id = {old}")),
        2
    );
}

/// One of everything a reader can put on an edition, on the edition that
/// goes and on the one that stays. What is on `en` goes; what is on `raw`
/// does not move.
#[test]
fn what_readers_put_on_a_rehomed_edition_goes_with_it_and_nothing_else_does() {
    let (conn, from, into) = divided_pair();
    for edition in ["raw", "en"] {
        put_progress(
            &conn,
            1,
            from,
            &Progress {
                edition: edition.to_string(),
                slot: 1,
                ..Progress::default()
            },
        )
        .expect("a position");
        seed_note(&conn, from, edition, 0);
        seed_truth(&conn, from, edition, 0, "bbb");
    }

    let moved = rehomed(&conn, from, into, &["en"]);

    assert_eq!(
        (moved.positions, moved.notes, moved.truth_pages),
        (1, 1, 1)
    );
    for (table, column) in [
        ("progress", "edition"),
        ("note", "edition"),
        ("truth_page", "edition"),
        ("truth_box", "edition"),
        ("truth_member", "edition"),
    ] {
        assert_eq!(
            count(
                &conn,
                &format!("SELECT count(*) FROM {table} WHERE work_id = {into} AND {column} = 'en'")
            ),
            1,
            "{table}: `en`'s row is on the work `en` went to"
        );
        assert_eq!(
            count(
                &conn,
                &format!("SELECT count(*) FROM {table} WHERE work_id = {from} AND {column} = 'en'")
            ),
            0,
            "{table}: and not left behind"
        );
        assert_eq!(
            count(
                &conn,
                &format!("SELECT count(*) FROM {table} WHERE work_id = {from} AND {column} = 'raw'")
            ),
            1,
            "{table}: `raw`'s row stayed"
        );
    }
}

#[test]
fn an_alignment_goes_with_two_editions_that_move_together_and_is_dropped_between_two_that_part() {
    let conn = open();
    seed_work(
        &conn,
        1,
        "stays",
        &[("raw", &["aaa"]), ("en", &["bbb"]), ("fr", &["ccc"])],
    );
    seed_work(&conn, 2, "fresh", &[("en", &["bbb"]), ("fr", &["ccc"])]);
    for (from_key, to_key) in [("en", "fr"), ("raw", "en")] {
        conn.execute(
            "INSERT INTO alignment (work_id, from_key, to_key, by) VALUES (1, ?1, ?2, 'hand')",
            params![from_key, to_key],
        )
        .expect("an alignment");
        conn.execute(
            "INSERT INTO run (alignment_id, ordinal, a_start_page, a_end_page)
             VALUES (?1, 0, 0, 0)",
            [conn.last_insert_rowid()],
        )
        .expect("its run");
    }

    let moved = rehomed(&conn, 1, 2, &["en", "fr"]);

    assert_eq!(moved.alignments_dropped, 1, "raw to en: the two parted");
    assert_eq!(
        count(
            &conn,
            "SELECT count(*) FROM alignment WHERE work_id = 2 AND from_key = 'en' AND to_key = 'fr'"
        ),
        1,
        "en to fr went with them"
    );
    assert_eq!(count(&conn, "SELECT count(*) FROM alignment"), 1);
    assert_eq!(count(&conn, "SELECT count(*) FROM run"), 1, "with its run");
}

/// Each refusal, and after each the store is as it was.
#[test]
fn a_rehoming_is_refused_before_anything_is_written() {
    let dump = |conn: &Connection| -> Vec<String> {
        let mut rows = Vec::new();
        for sql in [
            "SELECT 'edition ' || id || ' ' || work_id || ' ' || key FROM edition ORDER BY id",
            "SELECT 'progress ' || work_id || ' ' || edition || ' ' || slot FROM progress ORDER BY 1",
            "SELECT 'note ' || work_id || ' ' || edition || ' ' || id FROM note ORDER BY 1",
        ] {
            let mut statement = conn.prepare(sql).expect(sql);
            rows.extend(
                statement
                    .query_map([], |r| r.get::<_, String>(0))
                    .expect(sql)
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .expect(sql),
            );
        }
        rows
    };

    let (conn, from, into) = divided_pair();
    put_progress(
        &conn,
        1,
        from,
        &Progress {
            edition: "en".to_string(),
            slot: 1,
            ..Progress::default()
        },
    )
    .expect("a position");
    let before = dump(&conn);

    assert!(refused(&conn, from, from, &["en"]).contains("itself"));
    assert!(refused(&conn, from, into, &["raw"]).contains("hub"));
    assert!(refused(&conn, from, into, &["zz"]).contains("`zz`"));
    assert_eq!(dump(&conn), before);

    // The other side's edition holds different archives.
    conn.execute(
        "UPDATE member SET sha256 = 'zzz' WHERE edition_id = ?1 AND ordinal = 1",
        [edition_id(&conn, into, "en")],
    )
    .expect("a different archive");
    let before = dump(&conn);
    assert!(refused(&conn, from, into, &["en"]).contains("same archives"));
    assert_eq!(dump(&conn), before);
    conn.execute(
        "UPDATE member SET sha256 = 'ccc' WHERE edition_id = ?1 AND ordinal = 1",
        [edition_id(&conn, into, "en")],
    )
    .expect("back");

    // The other side already holds something a reader put on that key.
    put_progress(
        &conn,
        1,
        into,
        &Progress {
            edition: "en".to_string(),
            slot: 0,
            ..Progress::default()
        },
    )
    .expect("a position on the fresh edition");
    let before = dump(&conn);
    assert!(refused(&conn, from, into, &["en"]).contains("already holds"));
    assert_eq!(dump(&conn), before);
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p taiyaku-library fold:: -- --test-threads=8`
Expected: FAIL to compile, `cannot find function `rehome_editions``.

- [ ] **Step 3: Implement**

In `crates/taiyaku-library/src/fold.rs`, after `absorb`:

```rust
/// What [`rehome_editions`] carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rehomed {
    pub positions: usize,
    pub notes: usize,
    pub truth_pages: usize,
    /// Alignments between a rehomed edition and one that stayed. An
    /// alignment names two editions of one work, and these two are editions
    /// of two works now.
    pub alignments_dropped: usize,
}

/// What [`rehome_editions`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rehoming {
    Done(Rehomed),
    /// Nothing was moved and nothing deleted. As with [`Outcome::Refused`],
    /// a refusal is an outcome and not an error.
    Refused(String),
}

/// Give `into_work` the edition rows `keys` of `from_work`, in place of its
/// own editions of those keys, with everything a reader put on them.
///
/// **For a work a folder move divides** (`crate::relocate`). The derive that
/// follows the move leaves `from_work` holding editions whose archives now
/// belong to another library, and makes `into_work` there with fresh
/// editions over the same archives. [`absorb`] cannot carry them: it moves a
/// whole work and deletes it, and `from_work` keeps its other editions.
///
/// **The row moves, not a copy of it.** `into_work`'s fresh edition is
/// deleted and `from_work`'s is re-parented in its place, so its spread
/// flags, credit flags, label and declared chapters are still on it, and a
/// slot names the page it named. That is why this needs none of
/// [`plan_slots`]: nothing about the edition changed but the work it is in.
///
/// Refused, each read before the first write:
///
/// 1. `from_work == into_work`.
/// 2. A key either work does not hold, or held over different archives:
///    member digests are compared in ordinal order, as [`edition_map`]
///    compares them.
/// 3. A key that is `from_work`'s hub. A work's content key and its hand
///    document are its hub's, so a work goes where its hub goes and what is
///    rehomed is the rest.
/// 4. `into_work` already holding a position, a note, a ground-truth page or
///    an alignment on one of the keys. Its edition is expected fresh, and
///    merging two readers' rows is [`absorb`]'s job, with its refusals.
/// 5. A note id one reader holds on both works: `note`'s key is
///    `(user_id, work_id, id)`.
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub(crate) fn rehome_editions(
    conn: &Connection,
    from_work: i64,
    into_work: i64,
    keys: &[String],
) -> Result<Rehoming, String> {
    let refuse = |why: String| {
        Ok(Rehoming::Refused(format!(
            "{why}; nothing was moved and nothing was deleted"
        )))
    };
    if from_work == into_work {
        return refuse(format!("work {from_work} cannot give editions to itself"));
    }

    let tx = crate::Nested::open(conn, "rehome_editions")
        .map_err(|e| format!("starting to rehome editions of work {from_work}: {e}"))?;

    let hub: String = tx
        .query_row("SELECT hub FROM work WHERE id = ?1", [from_work], |r| r.get(0))
        .map_err(|e| format!("reading the hub of work {from_work}: {e}"))?;
    let source = digests_by_edition(&tx, from_work)?;
    let target = digests_by_edition(&tx, into_work)?;
    let digests = |held: &[(String, Vec<String>)], key: &str| {
        held.iter()
            .find(|(held_key, _)| held_key == key)
            .map(|(_, digests)| digests.clone())
    };
    for key in keys {
        if *key == hub {
            return refuse(format!(
                "`{key}` is the hub edition of work {from_work}, and a work goes where its hub \
                 goes"
            ));
        }
        let (Some(ours), Some(theirs)) = (digests(&source, key), digests(&target, key)) else {
            return refuse(format!(
                "work {from_work} and work {into_work} do not both hold an edition `{key}`"
            ));
        };
        if ours != theirs {
            return refuse(format!(
                "edition `{key}` of work {from_work} and of work {into_work} do not hold the \
                 same archives in the same order"
            ));
        }
        for (table, column) in [
            ("progress", "edition"),
            ("note", "edition"),
            ("truth_page", "edition"),
            ("alignment", "from_key"),
            ("alignment", "to_key"),
        ] {
            let held: i64 = tx
                .query_row(
                    &format!("SELECT count(*) FROM {table} WHERE work_id = ?1 AND {column} = ?2"),
                    rusqlite::params![into_work, key],
                    |r| r.get(0),
                )
                .map_err(|e| format!("reading {table} of work {into_work}: {e}"))?;
            if held > 0 {
                return refuse(format!(
                    "work {into_work} already holds {held} {table} row(s) on edition `{key}`"
                ));
            }
        }
    }
    let shared: i64 = tx
        .query_row(
            "SELECT count(*) FROM note ours JOIN note theirs
                ON theirs.user_id = ours.user_id AND theirs.id = ours.id
             WHERE ours.work_id = ?1 AND theirs.work_id = ?2",
            [from_work, into_work],
            |r| r.get(0),
        )
        .map_err(|e| format!("reading the notes of works {from_work} and {into_work}: {e}"))?;
    if shared > 0 {
        return refuse(format!(
            "one reader holds {shared} note id(s) on both work {from_work} and work {into_work}"
        ));
    }

    let mut moved = Rehomed::default();

    // An alignment both of whose editions go, goes. One with an edition on
    // each side is between two works now, and is dropped and counted.
    let alignments: Vec<(i64, String, String)> = tx
        .prepare("SELECT id, from_key, to_key FROM alignment WHERE work_id = ?1")
        .map_err(|e| format!("reading the alignments of work {from_work}: {e}"))?
        .query_map([from_work], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(|e| format!("reading the alignments of work {from_work}: {e}"))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("reading the alignments of work {from_work}: {e}"))?;
    for (id, from_key, to_key) in alignments {
        match (keys.contains(&from_key), keys.contains(&to_key)) {
            (true, true) => {
                tx.execute(
                    "UPDATE alignment SET work_id = ?2 WHERE id = ?1",
                    [id, into_work],
                )
                .map_err(|e| format!("moving alignment {id}: {e}"))?;
            }
            (false, false) => {}
            _ => {
                tx.execute("DELETE FROM run WHERE alignment_id = ?1", [id])
                    .and_then(|_| tx.execute("DELETE FROM alignment WHERE id = ?1", [id]))
                    .map_err(|e| format!("dropping alignment {id}: {e}"))?;
                moved.alignments_dropped += 1;
            }
        }
    }

    for key in keys {
        tx.execute(
            "DELETE FROM edition WHERE work_id = ?1 AND key = ?2",
            rusqlite::params![into_work, key],
        )
        .map_err(|e| format!("removing the fresh edition `{key}` of work {into_work}: {e}"))?;
        tx.execute(
            "UPDATE edition SET work_id = ?2 WHERE work_id = ?1 AND key = ?3",
            rusqlite::params![from_work, into_work, key],
        )
        .map_err(|e| format!("moving edition `{key}` to work {into_work}: {e}"))?;

        moved.positions += move_positions(&tx, from_work, into_work, key, key)?;
        // `move_positions` copies, and `absorb` leaves the source rows to the
        // delete of the work. This work stays, so they are deleted here.
        tx.execute(
            "DELETE FROM progress WHERE work_id = ?1 AND edition = ?2",
            rusqlite::params![from_work, key],
        )
        .map_err(|e| format!("clearing moved positions off work {from_work}: {e}"))?;
        moved.notes += move_notes(&tx, from_work, into_work, key, key)?;
        let (pages, _) = move_truth(&tx, from_work, into_work, key, key)?;
        moved.truth_pages += pages;
        // `move_truth` copies too. `truth_box` goes with its page.
        tx.execute(
            "DELETE FROM truth_page WHERE work_id = ?1 AND edition = ?2",
            rusqlite::params![from_work, key],
        )
        .and_then(|_| {
            tx.execute(
                "DELETE FROM truth_member WHERE work_id = ?1 AND edition = ?2",
                rusqlite::params![from_work, key],
            )
        })
        .map_err(|e| format!("clearing moved ground truth off work {from_work}: {e}"))?;
    }

    tx.commit()
        .map_err(|e| format!("committing the rehoming of work {from_work}'s editions: {e}"))?;
    Ok(Rehoming::Done(moved))
}
```

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p taiyaku-library fold:: -- --test-threads=8`
Expected: PASS. If `truth_box` rows do not go when their `truth_page` is
deleted, the fixture's connection has foreign keys off: `open()` turns them
on and says why.

- [ ] **Step 5: Watch each fail against its defect**

One at a time, restoring between:

1. Replace the edition `DELETE` and `UPDATE` with nothing, leaving the fresh
   edition in place. Expect
   `a_rehomed_edition_is_the_same_row_with_what_was_on_it` to fail.
2. Delete the `DELETE FROM progress` statement. Expect
   `what_readers_put_on_a_rehomed_edition_goes_with_it_and_nothing_else_does`
   to fail at "and not left behind".
3. Replace `rusqlite::params![from_work, key]` in the `truth_page` delete
   with a statement over the whole work,
   `DELETE FROM truth_page WHERE work_id = ?1 AND (edition = ?2 OR 1 = 1)`.
   Expect the same test to fail at "`raw`'s row stayed".
4. In the alignment loop, make the `_` arm do nothing. Expect
   `an_alignment_goes_with_two_editions_that_move_together_and_is_dropped_between_two_that_part`
   to fail.
5. Delete the `if *key == hub` block. Expect
   `a_rehoming_is_refused_before_anything_is_written` to fail at `hub`.
6. Delete the `held > 0` refusal. Expect the same test to fail at
   `already holds`.

- [ ] **Step 6: Whole crates, then commit**

Run: `cargo test -p taiyaku-library -p taiyaku-serve -p taiyaku-cli -- --test-threads=8`
Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Run: `cargo fmt --all -- --check`
Expected: all pass.

```bash
git add crates/taiyaku-library/src/fold.rs crates/taiyaku-library/src/fold/tests.rs
git commit -m "fold::rehome_editions: an edition row moves to another work with what readers put on it"
```

---

## Task 2: What refuses a move before anything is written

**Files:**
- Create: `crates/taiyaku-library/src/relocate.rs`,
  `crates/taiyaku-library/src/relocate/tests.rs`
- Modify: `crates/taiyaku-library/src/lib.rs` (`pub mod relocate;`),
  `crates/taiyaku-library/src/scan.rs` (`only_grant_accounts`)

**Interfaces:**
- Consumes: `scan::{roots, Root, update_root, RootPatch}`,
  `libraries::{name_of, id_of, create, trim_name}`, F2a's `only_grant_of`.
- Produces:
  ```rust
  // scan.rs
  /// The folder's path, its library's name, and each account whose only
  /// grant is that library, by name, where `id` is the library's last
  /// folder. `None` where it is not, or no such account exists.
  pub(crate) fn only_grant_accounts(conn: &Connection, id: i64)
      -> Result<Option<(String, String, Vec<String>)>, String>;

  // relocate.rs
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum Target<'a> { Library(i64), New(&'a str) }

  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum MoveError {
      /// No such folder, or no such library: 404.
      Missing(String),
      /// The caller's own mistake: 400.
      Unusable(String),
      /// The move cannot keep every reader's place, or would delete an
      /// account's only grant: 409.
      Refused { why: String, volumes: Vec<String> },
      /// A store fault: 500.
      Failed(String),
  }

  // Private to relocate.rs, used by Task 3:
  struct Folder { id: i64, path: String, library_id: i64, library: String }
  fn folder(conn: &Connection, root_id: i64) -> Result<Folder, MoveError>;
  /// The target's id, having made the library where one is named.
  fn resolve(conn: &Connection, folder: &Folder, target: Target<'_>) -> Result<i64, MoveError>;
  fn refused_before(conn: &Connection, folder: &Folder, target_id: i64)
      -> Result<Option<MoveError>, String>;
  ```
  Volumes are named `"<work title> (<edition key>)"`.

- [ ] **Step 1: Read the probe**

`.superpowers/sdd/folder-move-evidence/plan-probe-062d09ef.log` is what the
store did, on 2026-10-06, in each arrangement this plan's Task 3 builds, with
the probe's source beside it. Read both before writing a fixture: they show
the rows a derive leaves, and that a derive gives each edition it writes a
new row id while leaving an edition it does not name as it was.

- [ ] **Step 2: Write the failing tests**

Create `crates/taiyaku-library/src/relocate/tests.rs`:

```rust
use std::path::Path;

use rusqlite::Connection;

use super::*;
use crate::scan::tests::write_pages;
use crate::scan::{self, InLibrary, Root, RootPatch};
use crate::tests::tempdir;
use crate::{Library, derive};

/// The one integer `sql` answers with: a count, or an id where the fixture
/// holds one row.
pub(super) fn scalar(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).expect(sql)
}

/// A folder at `dir` holding each `(series, volume, pages)`, filed under
/// `library`, with `edition` as its edition key where one is given.
pub(super) fn a_folder(
    conn: &Connection,
    dir: &Path,
    library: &str,
    edition: Option<&str>,
    volumes: &[(&str, &str, usize)],
) -> Root {
    for (series, volume, pages) in volumes {
        write_pages(&dir.join(series).join(format!("{series} - {volume}")), *pages);
    }
    let root = scan::add_root_in(conn, dir, InLibrary::Named(library)).expect("adds");
    if let Some(key) = edition {
        scan::update_root(
            conn,
            root.id,
            &RootPatch {
                edition_key: Some(Some(key.to_string())),
                ..Default::default()
            },
        )
        .expect("sets the edition key");
    }
    root
}

pub(super) fn scanned(conn: &Connection) {
    for root in scan::roots(conn).expect("lists") {
        scan::scan_root(conn, &root).expect("scans");
    }
    derive::derive(conn).expect("derives");
}

pub(super) fn library_called(conn: &Connection, name: &str) -> i64 {
    crate::libraries::id_of(conn, name)
        .expect("reads")
        .expect("the library exists")
}

/// What a move may touch, as text, for "the store is as it was".
///
/// No edition or member id is in it: a derive gives each edition it writes
/// a new row, so their ids differ after any derive and say nothing.
pub(super) fn dump(conn: &Connection) -> Vec<String> {
    let mut rows = Vec::new();
    for sql in [
        "SELECT 'library ' || id || ' ' || name FROM library ORDER BY id",
        "SELECT 'root ' || id || ' ' || library_id FROM root ORDER BY id",
        "SELECT 'series ' || id || ' ' || coalesce(library_id, 'none') || ' ' || sort_title
           FROM series ORDER BY id",
        "SELECT 'work ' || id || ' ' || slug || ' ' || coalesce(library_id, 'none') || ' '
                || coalesce(series_id, 'none') || ' ' || content_key || ' ' || title
           FROM work ORDER BY id",
        "SELECT 'edition ' || work_id || ' ' || key FROM edition ORDER BY 1",
        "SELECT 'member ' || e.work_id || ' ' || e.key || ' ' || m.ordinal || ' ' || m.path
           FROM member m JOIN edition e ON e.id = m.edition_id ORDER BY 1",
        "SELECT 'progress ' || user_id || ' ' || work_id || ' ' || edition || ' ' || slot
           FROM progress ORDER BY 1",
        "SELECT 'note ' || user_id || ' ' || work_id || ' ' || edition || ' ' || id
           FROM note ORDER BY 1",
        "SELECT 'grant ' || user_id || ' ' || library_id FROM user_library ORDER BY 1",
    ] {
        let mut statement = conn.prepare(sql).expect(sql);
        rows.extend(
            statement
                .query_map([], |r| r.get::<_, String>(0))
                .expect(sql)
                .collect::<rusqlite::Result<Vec<_>>>()
                .expect(sql),
        );
    }
    rows
}

fn refusal(conn: &Connection, root: &Root, target: i64) -> (String, Vec<String>) {
    let folder = folder(conn, root.id).expect("the folder");
    match refused_before(conn, &folder, target).expect("reads") {
        Some(MoveError::Refused { why, volumes }) => (why, volumes),
        other => panic!("not refused: {other:?}"),
    }
}

#[test]
fn a_folder_or_a_library_that_is_not_there_is_missing_and_its_own_library_is_a_mistake() {
    let dir = tempdir("relocate-resolve");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = a_folder(&conn, &dir.path().join("a"), "Manga", None, &[("S", "v01", 2)]);
    a_folder(&conn, &dir.path().join("b"), "Comics", None, &[("T", "v01", 2)]);
    let held = folder(&conn, root.id).expect("the folder");
    assert_eq!(held.library, "Manga");

    assert!(matches!(folder(&conn, 4242), Err(MoveError::Missing(why)) if why.contains("4242")));
    assert!(matches!(
        resolve(&conn, &held, Target::Library(4242)),
        Err(MoveError::Missing(why)) if why.contains("4242")
    ));
    assert!(matches!(
        resolve(&conn, &held, Target::Library(held.library_id)),
        Err(MoveError::Unusable(why)) if why.contains("already in `Manga`")
    ));
    for (name, says) in [("  ", "needs a name"), ("Comics", "already called `Comics`")] {
        assert!(
            matches!(
                resolve(&conn, &held, Target::New(name)),
                Err(MoveError::Unusable(why)) if why.contains(says)
            ),
            "{name:?}"
        );
    }
    assert_eq!(
        resolve(&conn, &held, Target::Library(library_called(&conn, "Comics"))),
        Ok(library_called(&conn, "Comics"))
    );
    let made = resolve(&conn, &held, Target::New(" Extras ")).expect("makes it");
    assert_eq!(
        crate::libraries::name_of(&conn, made).expect("reads").as_deref(),
        Some("Extras")
    );
}

/// §6.2's case 4: the target holds the volume under the same edition key.
/// The other volume, which the target does not hold, is not named.
#[test]
fn the_same_volume_under_the_same_edition_key_in_the_target_refuses_the_move() {
    let dir = tempdir("relocate-same-key");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_folder(
        &conn,
        &dir.path().join("a"),
        "Chapters",
        None,
        &[("Series", "v01", 2), ("Series", "v02", 2)],
    );
    a_folder(&conn, &dir.path().join("b"), "Manga", None, &[("Series", "v01", 3)]);
    scanned(&conn);

    let (why, volumes) = refusal(&conn, &moving, library_called(&conn, "Manga"));

    assert!(why.contains("same edition key"), "{why}");
    assert!(why.contains("`Manga`"), "{why}");
    assert_eq!(volumes.len(), 1, "{volumes:?}");
    assert!(volumes[0].ends_with("(raw)"), "{volumes:?}");
}

/// The same two folders with an edition key apiece is case 3, which a move
/// makes: nothing refuses it here.
#[test]
fn the_same_volume_under_another_edition_key_does_not_refuse_the_move() {
    let dir = tempdir("relocate-other-key");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_folder(
        &conn,
        &dir.path().join("a"),
        "Chapters",
        Some("chapters"),
        &[("Series", "v01", 2)],
    );
    a_folder(&conn, &dir.path().join("b"), "Manga", None, &[("Series", "v01", 3)]);
    scanned(&conn);
    let held = folder(&conn, moving.id).expect("the folder");
    assert_eq!(
        refused_before(&conn, &held, library_called(&conn, "Manga")).expect("reads"),
        None
    );
}

/// One edition with a member under each of two folders of one library. Moving
/// either folder away would divide the edition itself.
#[test]
fn an_edition_with_members_on_both_sides_refuses_the_move() {
    let dir = tempdir("relocate-spanning");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_folder(&conn, &dir.path().join("a"), "Manga", None, &[("Series", "v01", 2)]);
    a_folder(&conn, &dir.path().join("b"), "Manga", None, &[("Series", "v01", 3)]);
    a_folder(&conn, &dir.path().join("c"), "Other", None, &[("Else", "v01", 2)]);
    scanned(&conn);
    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM edition e JOIN work w ON w.id = e.work_id
                        WHERE w.title LIKE 'Series%'"),
        1,
        "the fixture: both folders' files are one edition of one work"
    );

    let (why, volumes) = refusal(&conn, &moving, library_called(&conn, "Other"));

    assert!(why.contains("one edition"), "{why}");
    assert_eq!(volumes.len(), 1, "{volumes:?}");
}

/// F1's decision, asked of a move: the folder is its library's last and the
/// library is an account's only grant.
#[test]
fn moving_the_last_folder_of_an_accounts_only_library_is_refused_by_name() {
    let dir = tempdir("relocate-only-grant");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let solo = a_folder(&conn, &dir.path().join("a"), "Solo", None, &[("S", "v01", 2)]);
    let shelf_a = a_folder(&conn, &dir.path().join("b"), "Shelf", None, &[("T", "v01", 2)]);
    a_folder(&conn, &dir.path().join("c"), "Shelf", None, &[("U", "v01", 2)]);
    scanned(&conn);
    conn.execute(
        "INSERT INTO user (name, password_hash, role) VALUES ('sam', 'x', 'user')",
        [],
    )
    .expect("an account");
    let sam = conn.last_insert_rowid();
    crate::account::grant_library(&conn, sam, solo.library_id).expect("grants");

    let (why, volumes) = refusal(&conn, &solo, shelf_a.library_id);
    assert!(why.contains("`Solo`") && why.contains("`sam`"), "{why}");
    assert!(why.contains("Moving it"), "{why}");
    assert!(volumes.is_empty());

    crate::account::grant_library(&conn, sam, shelf_a.library_id).expect("grants Shelf too");
    crate::account::revoke_library(&conn, sam, solo.library_id).expect("revokes Solo");
    let held = folder(&conn, shelf_a.id).expect("the folder");
    assert_eq!(
        refused_before(&conn, &held, solo.library_id).expect("reads"),
        None,
        "Shelf keeps another folder, so the grant stands"
    );
}
```

- [ ] **Step 3: Run them to verify they fail**

Run: `cargo test -p taiyaku-library relocate:: -- --test-threads=8`
Expected: FAIL to compile, the module does not exist.

- [ ] **Step 4: Cut `only_grant_accounts` out of `removal_refusal`**

In `crates/taiyaku-library/src/scan.rs`, `removal_refusal`'s read becomes a
function, and `removal_refusal` builds its unchanged sentence from it:

```rust
/// Where root `id` is its library's last folder and that library is the only
/// grant of some account: the folder's path, the library's name and those
/// accounts by name. `None` where it is not the last folder, where no such
/// account exists, and where `id` names no root.
pub(crate) fn only_grant_accounts(
    conn: &Connection,
    id: i64,
) -> Result<Option<(String, String, Vec<String>)>, String> {
    let mut statement = conn
        .prepare(
            "SELECT r.path, l.name, u.name
               FROM root r
               JOIN library l ON l.id = r.library_id
               JOIN user_library ul ON ul.library_id = r.library_id
               JOIN user u ON u.id = ul.user_id
              WHERE r.id = ?1
                AND NOT EXISTS (SELECT 1 FROM root other
                                 WHERE other.library_id = r.library_id AND other.id <> r.id)
                AND NOT EXISTS (SELECT 1 FROM user_library held
                                 WHERE held.user_id = ul.user_id
                                   AND held.library_id <> ul.library_id)
              ORDER BY u.name",
        )
        .map_err(|e| format!("checking whose only grant root {id} holds: {e}"))?;
    let restricted = statement
        .query_map([id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| format!("checking whose only grant root {id} holds: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("checking whose only grant root {id} holds: {e}"))?;
    let Some((path, library, _)) = restricted.first().cloned() else {
        return Ok(None);
    };
    Ok(Some((
        path,
        library,
        restricted.into_iter().map(|(_, _, account)| account).collect(),
    )))
}

pub fn removal_refusal(conn: &Connection, id: i64) -> Result<Option<String>, String> {
    let Some((path, library, names)) = only_grant_accounts(conn, id)? else {
        return Ok(None);
    };
    let (accounts, advice) = only_grant_of(&names);
    Ok(Some(format!(
        "{path} is the last folder of the library `{library}`, which is the only library \
         {accounts}. {advice}"
    )))
}
```

Run `cargo test -p taiyaku-library scan:: -- --test-threads=8`. Expected:
PASS: the sentence is the one it was.

- [ ] **Step 5: Write `relocate.rs`'s first half**

Create `crates/taiyaku-library/src/relocate.rs`, and add `pub mod relocate;`
to `lib.rs`:

```rust
//! Moving a folder from one library to another: §6.2 of
//! `docs/superpowers/specs/2026-10-05-libraries-design.md`.
//!
//! **A move is a writer in front of the derive, and the derive does not
//! change.** Left to the derive alone, a folder whose library changed has its
//! works written again under the new library and the old ones folded into
//! them, so each has a new id and slug, and a work with an edition on each
//! side is not divided at all. So the move files the works itself first, and
//! divides them itself afterwards.
//!
//! **A work goes where its hub edition goes.** Its content key is a hash of
//! the hub's members and its hand document is addressed off the hub's
//! folder, so the row that keeps both is the one that stays with the hub.
//!
//! **It is refused where a reader's place cannot be kept exactly**, and
//! refused whole: by what [`refused_before`] reads before anything is
//! written, and by the verdict taken after the derive. Either way nothing
//! changes.
//!
//! **A rehearsal is the move, rolled back**, as `scan::removal_cost` is a
//! removal rolled back. It derives with `Documents::Skip`, so it writes no
//! file either.

use rusqlite::{Connection, OptionalExtension as _};

/// Where a folder is moved to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target<'a> {
    /// A library that exists, by its id.
    Library(i64),
    /// A library made for the folder. A library is born with a folder, so
    /// this is the only way to give a folder one of its own again.
    New(&'a str),
}

/// Why a move was not made. Nothing was changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveError {
    /// No such folder, or no such library.
    Missing(String),
    /// The caller's own mistake: the folder's own library, or a name for a
    /// new one that is empty or taken.
    Unusable(String),
    /// The move cannot keep every reader's place, or would delete an
    /// account's only grant. `volumes` names what stands in the way, each as
    /// `title (edition key)`; it is empty where no volume does.
    Refused { why: String, volumes: Vec<String> },
    /// A store fault.
    Failed(String),
}

impl std::fmt::Display for MoveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing(why) | Self::Unusable(why) | Self::Failed(why) => f.write_str(why),
            Self::Refused { why, .. } => f.write_str(why),
        }
    }
}

/// The folder being moved, as it stands before the move.
struct Folder {
    id: i64,
    path: String,
    library_id: i64,
    library: String,
}

fn folder(conn: &Connection, root_id: i64) -> Result<Folder, MoveError> {
    conn.query_row(
        "SELECT r.id, r.path, l.id, l.name FROM root r JOIN library l ON l.id = r.library_id
          WHERE r.id = ?1",
        [root_id],
        |r| {
            Ok(Folder {
                id: r.get(0)?,
                path: r.get(1)?,
                library_id: r.get(2)?,
                library: r.get(3)?,
            })
        },
    )
    .optional()
    .map_err(|e| MoveError::Failed(format!("reading root {root_id}: {e}")))?
    .ok_or_else(|| MoveError::Missing(format!("there is no root {root_id}")))
}

/// The id of the library `target` names, making it where a new one is named.
/// Made inside the caller's transaction, so a refused or rehearsed move
/// leaves no library behind.
fn resolve(conn: &Connection, folder: &Folder, target: Target<'_>) -> Result<i64, MoveError> {
    match target {
        Target::Library(id) => {
            let name = crate::libraries::name_of(conn, id)
                .map_err(MoveError::Failed)?
                .ok_or_else(|| MoveError::Missing(format!("there is no library {id}")))?;
            if id == folder.library_id {
                return Err(MoveError::Unusable(format!(
                    "{} is already in `{name}`",
                    folder.path
                )));
            }
            Ok(id)
        }
        Target::New(name) => {
            if name.trim().is_empty() {
                return Err(MoveError::Unusable("a library needs a name".to_string()));
            }
            let name = crate::libraries::trim_name(name);
            if crate::libraries::id_of(conn, name)
                .map_err(MoveError::Failed)?
                .is_some()
            {
                return Err(MoveError::Unusable(format!(
                    "a library is already called `{name}`; move the folder to it instead"
                )));
            }
            crate::libraries::create(conn, name).map_err(MoveError::Failed)
        }
    }
}

/// How many volumes a refusal's sentence names before it counts the rest.
/// The whole list travels beside the sentence.
const NAMED: usize = 3;

/// `a, b, c and 4 more`.
fn first_few(volumes: &[String]) -> String {
    let shown: Vec<&str> = volumes.iter().take(NAMED).map(String::as_str).collect();
    match volumes.len() - shown.len() {
        0 => shown.join(", "),
        rest => format!("{} and {rest} more", shown.join(", ")),
    }
}

/// Each `title (edition key)` a statement of two columns answers, in order.
fn volumes_of(
    conn: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
) -> Result<Vec<String>, String> {
    let mut statement = conn
        .prepare(sql)
        .map_err(|e| format!("reading what stands in a move's way: {e}"))?;
    let rows = statement
        .query_map(params, |r| {
            Ok(format!("{} ({})", r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| format!("reading what stands in a move's way: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading what stands in a move's way: {e}"))
}

/// Why `folder` may not be moved to `target_id`, as far as the store can say
/// before anything is written.
///
/// 1. **The folder is its library's last and that library is an account's
///    only grant.** F1's rule for a removal, in a move's words. Migration
///    15's `only_grant_refuses_move` stands behind it with a sentence that
///    names no account.
/// 2. **One edition has members under the folder and elsewhere.** The move
///    would divide the edition itself, and no slot of it would name the page
///    it named.
/// 3. **The target holds a volume of the folder's under the same edition
///    key.** A volume is a series title and a number within a library, so
///    derived together the two are one edition of one work, holding both
///    sets of pages.
///
/// Each of 2 and 3 names its volumes.
fn refused_before(
    conn: &Connection,
    folder: &Folder,
    target_id: i64,
) -> Result<Option<MoveError>, String> {
    if let Some((path, library, names)) = crate::scan::only_grant_accounts(conn, folder.id)? {
        let (accounts, _) = crate::scan::only_grant_of(&names);
        return Ok(Some(MoveError::Refused {
            why: format!(
                "{path} is the last folder of the library `{library}`, which is the only \
                 library {accounts}. Moving it away would delete that library and the grant \
                 with it, and an account with no grant sees every library. First grant another \
                 library, or revoke this one"
            ),
            volumes: Vec::new(),
        }));
    }

    let spanning = volumes_of(
        conn,
        "SELECT w.title, e.key FROM edition e JOIN work w ON w.id = e.work_id
          WHERE EXISTS (SELECT 1 FROM member m WHERE m.edition_id = e.id AND m.root_id = ?1)
            AND EXISTS (SELECT 1 FROM member m
                         WHERE m.edition_id = e.id AND m.root_id IS NOT ?1)
          ORDER BY w.title, e.key",
        &[&folder.id],
    )?;
    if !spanning.is_empty() {
        return Ok(Some(MoveError::Refused {
            why: format!(
                "{} holds part of one edition whose other files are elsewhere, and a move \
                 would divide it: {}. Nothing was changed",
                folder.path,
                first_few(&spanning)
            ),
            volumes: spanning,
        }));
    }

    let target = crate::libraries::name_of(conn, target_id)?.unwrap_or_default();
    let doubled = volumes_of(
        conn,
        "SELECT DISTINCT w.title, e.key
           FROM work w
           JOIN series s ON s.id = w.series_id
           JOIN edition e ON e.work_id = w.id
          WHERE w.volume IS NOT NULL
            AND EXISTS (SELECT 1 FROM member m WHERE m.edition_id = e.id AND m.root_id = ?1)
            AND EXISTS (SELECT 1 FROM work tw
                          JOIN series ts ON ts.id = tw.series_id
                          JOIN edition te ON te.work_id = tw.id
                         WHERE tw.library_id = ?2 AND tw.id <> w.id
                           AND ts.sort_title = s.sort_title AND tw.volume = w.volume
                           AND te.key = e.key)
          ORDER BY w.title, e.key",
        &[&folder.id, &target_id],
    )?;
    if !doubled.is_empty() {
        return Ok(Some(MoveError::Refused {
            why: format!(
                "`{target}` already holds volumes of {} under the same edition key, and one \
                 edition cannot hold both sets of pages and keep a reader's place in either: \
                 {}. Nothing was changed",
                folder.path,
                first_few(&doubled)
            ),
            volumes: doubled,
        }));
    }
    Ok(None)
}

#[cfg(test)]
mod tests;
```

`scan::only_grant_of` becomes `pub(crate)` for this.

- [ ] **Step 6: Run them to verify they pass**

Run: `cargo test -p taiyaku-library relocate:: -- --test-threads=8`
Expected: PASS. `an_edition_with_members_on_both_sides_refuses_the_move`
rests on two folders of one library holding one volume under one edition key
making one edition of one work, which the probe shows (its third case).

- [ ] **Step 7: Watch each fail against its defect**

One at a time, restoring between:

1. `refused_before`, the `doubled` statement: write
   `AND (te.key = e.key OR 1 = 1)`. Expect
   `the_same_volume_under_another_edition_key_does_not_refuse_the_move` to
   fail.
2. The same statement: write `AND (tw.volume = w.volume OR 1 = 1)`. Expect
   `the_same_volume_under_the_same_edition_key_in_the_target_refuses_the_move`
   to fail with two volumes for one.
3. The `spanning` statement: write `m.root_id IS NOT ?1 AND 1 = 0` in its
   second `EXISTS`. Expect
   `an_edition_with_members_on_both_sides_refuses_the_move` to fail.
4. Delete the `only_grant_accounts` block. Expect
   `moving_the_last_folder_of_an_accounts_only_library_is_refused_by_name` to
   fail.
5. `resolve`: delete the `id == folder.library_id` check. Expect the first
   test to fail.

- [ ] **Step 8: Whole crates, then commit**

Run the three commands of Task 1's Step 6. Nothing outside the tests calls
this module's private items until Task 3, so put one expectation at the top
of `relocate.rs`, under its doc comment, and take it off in Task 3:

```rust
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "the move that calls these is the next commit")
)]
```

```bash
git add crates/taiyaku-library/src
git commit -m "relocate: what refuses a folder move before anything is written"
```

---

## Task 3: The move, its rehearsal, and its verdict

**Files:**
- Modify: `crates/taiyaku-library/src/relocate.rs`,
  `crates/taiyaku-library/src/relocate/tests.rs`

**Interfaces:**
- Consumes: Task 1's `fold::{rehome_editions, Rehoming, Rehomed}`; Task 2's
  `folder`, `resolve`, `refused_before`, `first_few`; `derive::{derive,
  derive_with, Documents, DeriveReport}`; `crate::Nested`.
- Produces:
  ```rust
  /// What a move did, or in a rehearsal would do.
  #[derive(Debug, Clone, PartialEq, Eq, Default)]
  pub struct Moved {
      /// The library the folder is in afterwards, and its name.
      pub library_id: i64,
      pub library: String,
      /// Works wholly under the folder that kept their row.
      pub works_kept: usize,
      /// Works wholly under the folder that were folded into a work the
      /// target already held, as another edition of it.
      pub works_joined: usize,
      /// Works with editions on both sides, divided.
      pub works_split: usize,
      /// Series the target already held that gained a volume from the folder.
      pub series_joined: usize,
      /// Reading positions and notes on the works above. They all go with
      /// them, or the move is refused.
      pub positions: usize,
      pub notes: usize,
      /// Alignments between two editions the move separated.
      pub alignments_dropped: usize,
      /// The library the folder left, where it was that library's last
      /// folder, and the grants that went with it.
      pub library_removed: Option<String>,
      pub grants_removed: usize,
  }

  pub fn move_cost(conn: &Connection, root_id: i64, target: Target<'_>)
      -> Result<(Moved, crate::derive::DeriveReport), MoveError>;
  pub fn move_root(conn: &Connection, root_id: i64, target: Target<'_>)
      -> Result<(Moved, crate::derive::DeriveReport), MoveError>;
  ```

**What the move does, in order, inside one transaction:**

1. `folder`, `resolve`, `refused_before`.
2. A derive (`Documents::Skip`), so that what the derive in step 6 reports is
   the move's doing.
3. A census: which works have a member under the folder, which of those are
   wholly under it and which are divided; the positions and notes on them;
   how many positions, notes and ground-truth pages the store holds; which
   works are filed under a library; whether the folder is its library's
   last, and that library's grants.
4. **Re-file.** Each work whose hub edition is under the folder takes the
   target's `library_id`, unless the target already holds a work of its
   content key, or a work at its address (its series' `sort_title` and its
   volume). Those are left for the derive to fold into the target's work.
   Then each series whose works were all re-filed takes the target's
   `library_id`, unless the target holds a series of its `sort_title`.
5. `UPDATE root SET library_id`. Migration 15 deletes the library the folder
   left where it was the last.
6. A derive (`Documents::Skip`).
7. **Split.** For each divided work, each edition whose members are under
   folders of another library than the work's is rehomed onto the work of
   that library holding the same edition over the same members.
8. **The verdict**, refusing the move if any of these is true, naming the
   works: an edition in step 7 had nowhere to go, or was refused; a work
   wholly under the folder still stands outside the target; a work that was
   filed is filed under no library; the derive reports more unfolded works
   than step 2's did, or a lost position, or a dropped note; the store holds
   fewer positions, notes or ground-truth pages than the census counted.

`move_cost` drops the transaction. `move_root` commits it and derives again
with documents reconciled.

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-library/src/relocate/tests.rs`. Each test is one of
§6.2's cases; the comment over each says which.

```rust
fn a_position_and_a_note(conn: &Connection, work: i64, edition: &str) {
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, ?2, 1)",
        rusqlite::params![work, edition],
    )
    .expect("a position");
    conn.execute(
        "INSERT INTO note (user_id, id, work_id, edition, slot, body, created, updated)
         VALUES (1, 'n' || ?1 || ?2, ?1, ?2, 0, 'kept', 'now', 'now')",
        rusqlite::params![work, edition],
    )
    .expect("a note");
}

fn the_work(conn: &Connection, title_like: &str) -> (i64, String, Option<i64>) {
    conn.query_row(
        "SELECT id, slug, library_id FROM work WHERE title LIKE ?1",
        [title_like],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .unwrap_or_else(|e| panic!("one work like {title_like}: {e}"))
}

fn moved(conn: &Connection, root: &Root, target: Target<'_>) -> Moved {
    move_root(conn, root.id, target).expect("moves").0
}

/// Case 2, the one a move exists for: the library keeps another folder, and
/// the moved folder's work keeps its row, its slug, its position, its note
/// and what was put on it by hand. `keeps_documents` is off, so nothing but
/// the row holds the hand title.
#[test]
fn a_folder_leaving_a_library_that_keeps_another_keeps_its_works_as_they_were() {
    let dir = tempdir("relocate-case-2");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    a_folder(&conn, &dir.path().join("kept"), "A", None, &[("Kept", "v01", 2)]);
    let moving = a_folder(&conn, &dir.path().join("moved"), "A", None, &[("Moved", "v01", 3)]);
    let other = a_folder(&conn, &dir.path().join("else"), "B", None, &[("Else", "v01", 4)]);
    scanned(&conn);
    let (id, slug, _) = the_work(&conn, "Moved%");
    a_position_and_a_note(&conn, id, "raw");
    conn.execute(
        "UPDATE work SET title = 'By Hand', title_hand = 'By Hand' WHERE id = ?1",
        [id],
    )
    .expect("a hand title");
    derive::derive(&conn).expect("derives");
    assert_eq!(
        the_work(&conn, "By Hand").0,
        id,
        "the fixture: a derive leaves a hand title standing. If it does not, set the \
         title the way `librarydoc` does and keep this assertion"
    );
    let series: i64 = scalar(&conn, &format!("SELECT series_id FROM work WHERE id = {id}"));
    conn.execute(
        "INSERT INTO series_title (series_id, title, is_primary) VALUES (?1, 'Alt', 0)",
        [series],
    )
    .expect("an alternative title");

    let report = moved(&conn, &moving, Target::Library(other.library_id));

    assert_eq!(
        the_work(&conn, "By Hand"),
        (id, slug, Some(other.library_id)),
        "the same row, the same slug, the hand title, under B"
    );
    assert_eq!(
        (report.works_kept, report.works_joined, report.works_split),
        (1, 0, 0)
    );
    assert_eq!((report.positions, report.notes), (1, 1));
    assert_eq!(report.library, "B");
    assert_eq!(report.library_removed, None);
    assert_eq!(
        scalar(&conn, &format!("SELECT count(*) FROM progress WHERE work_id = {id}")),
        1
    );
    assert_eq!(
        scalar(&conn, &format!("SELECT count(*) FROM note WHERE work_id = {id}")),
        1
    );
    assert_eq!(
        scalar(
            &conn,
            &format!(
                "SELECT count(*) FROM series s JOIN work w ON w.series_id = s.id
                  WHERE w.id = {id} AND s.id = {series} AND s.library_id = {}",
                other.library_id
            )
        ),
        1,
        "every work of the series moved, so the series went too"
    );
    assert_eq!(
        scalar(&conn, &format!("SELECT count(*) FROM series_title WHERE series_id = {series}")),
        2,
        "with its alternative title"
    );
    assert_eq!(scalar(&conn, "SELECT count(*) FROM work"), 3, "nothing was written again");

    // A second derive finds nothing to do.
    let before = dump(&conn);
    let again = derive::derive(&conn).expect("derives again");
    assert_eq!(dump(&conn), before);
    assert_eq!((again.emptied_works, again.rekeyed, again.kept_editions), (0, 0, 0));
}

/// Case 1: the folder is alone in its library, so the library goes with the
/// move, and its grant with it. The account holds another library, so that
/// is allowed, and counted.
#[test]
fn a_folder_alone_in_its_library_takes_its_works_and_the_library_goes() {
    let dir = tempdir("relocate-case-1");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_folder(&conn, &dir.path().join("moved"), "Solo", None, &[("Moved", "v01", 3)]);
    let other = a_folder(&conn, &dir.path().join("else"), "B", None, &[("Else", "v01", 4)]);
    scanned(&conn);
    let (id, slug, _) = the_work(&conn, "Moved%");
    conn.execute(
        "INSERT INTO user (name, password_hash, role) VALUES ('sam', 'x', 'user')",
        [],
    )
    .expect("an account");
    let sam = conn.last_insert_rowid();
    for library in [moving.library_id, other.library_id] {
        crate::account::grant_library(&conn, sam, library).expect("grants");
    }

    let report = moved(&conn, &moving, Target::Library(other.library_id));

    assert_eq!(the_work(&conn, "Moved%"), (id, slug, Some(other.library_id)));
    assert_eq!(report.library_removed.as_deref(), Some("Solo"));
    assert_eq!(report.grants_removed, 1);
    assert_eq!(report.works_kept, 1);
    assert_eq!(
        crate::libraries::name_of(&conn, moving.library_id).expect("reads"),
        None
    );
    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM work WHERE library_id IS NULL"),
        0
    );
    assert_eq!(
        scalar(&conn, "SELECT count(*) FROM series WHERE library_id IS NULL"),
        0
    );
}

/// Case 3, Chapters into Manga: the target holds the volume under another
/// edition key, so the two are one work with two editions. The target's row
/// is the one that stands, and the moved work's position is on it.
#[test]
fn a_volume_the_target_holds_under_another_key_joins_it_as_an_edition() {
    let dir = tempdir("relocate-case-3");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_folder(
        &conn,
        &dir.path().join("chapters"),
        "Chapters",
        Some("chapters"),
        &[("Series", "v01", 2), ("Series", "v02", 2)],
    );
    let volumes = a_folder(&conn, &dir.path().join("volumes"), "Manga", None, &[("Series", "v01", 3)]);
    scanned(&conn);
    let target: i64 = scalar(
        &conn,
        &format!(
            "SELECT id FROM work WHERE library_id = {} AND volume = 1.0",
            volumes.library_id
        ),
    );
    let joining: i64 = scalar(
        &conn,
        &format!(
            "SELECT id FROM work WHERE library_id = {} AND volume = 1.0",
            moving.library_id
        ),
    );
    let staying: i64 = scalar(
        &conn,
        &format!(
            "SELECT id FROM work WHERE library_id = {} AND volume = 2.0",
            moving.library_id
        ),
    );
    a_position_and_a_note(&conn, joining, "chapters");

    let report = moved(&conn, &moving, Target::Library(volumes.library_id));

    assert_eq!(
        (report.works_kept, report.works_joined, report.works_split),
        (1, 1, 0),
        "v02 kept its row; v01 joined Manga's"
    );
    assert_eq!(report.series_joined, 1);
    assert_eq!(scalar(&conn, &format!("SELECT count(*) FROM work WHERE id = {joining}")), 0);
    assert_eq!(
        scalar(&conn, &format!("SELECT count(*) FROM edition WHERE work_id = {target}")),
        2
    );
    assert_eq!(
        scalar(
            &conn,
            &format!(
                "SELECT count(*) FROM progress WHERE work_id = {target} AND edition = 'chapters'"
            )
        ),
        1
    );
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT library_id FROM work WHERE id = {staying}")
        ),
        volumes.library_id
    );
    assert_eq!(
        scalar(
            &conn,
            &format!(
                "SELECT count(DISTINCT series_id) FROM work WHERE id IN ({target}, {staying})"
            )
        ),
        1,
        "and both volumes are in Manga's series of that title"
    );
}

/// Case 5 with the hub staying: the work stays where its hub is, and the
/// moved edition is a work of the target with that edition's position and
/// note. The alignment between the two editions is dropped and counted.
#[test]
fn a_divided_work_stays_with_its_hub_and_its_other_edition_takes_its_history() {
    let dir = tempdir("relocate-split-stays");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    a_folder(&conn, &dir.path().join("hub"), "Manga", Some("en"), &[("Series", "v01", 2)]);
    let moving = a_folder(&conn, &dir.path().join("raw"), "Manga", None, &[("Series", "v01", 3)]);
    scanned(&conn);
    let (id, slug, home) = the_work(&conn, "Series%");
    assert_eq!(
        scalar(&conn, &format!("SELECT count(*) FROM work WHERE id = {id} AND hub = 'en'")),
        1,
        "the fixture: one work, its hub the folder that stays"
    );
    for edition in ["en", "raw"] {
        a_position_and_a_note(&conn, id, edition);
    }
    conn.execute(
        "INSERT INTO alignment (work_id, from_key, to_key, by) VALUES (?1, 'en', 'raw', 'hand')",
        [id],
    )
    .expect("an alignment between the two");

    let report = moved(&conn, &moving, Target::New("Raws"));

    let raws = library_called(&conn, "Raws");
    assert_eq!(
        (report.works_kept, report.works_joined, report.works_split),
        (0, 0, 1)
    );
    assert_eq!(report.alignments_dropped, 1);
    assert_eq!((report.positions, report.notes), (2, 2));
    assert_eq!(
        scalar(&conn, &format!("SELECT library_id FROM work WHERE id = {id}")),
        home.expect("filed")
    );
    assert_eq!(
        scalar(&conn, &format!("SELECT slug = '{slug}' FROM work WHERE id = {id}")),
        1
    );
    assert_eq!(
        scalar(&conn, &format!("SELECT count(*) FROM edition WHERE work_id = {id}")),
        1,
        "it holds its hub edition and no other"
    );
    let split: i64 = scalar(&conn, &format!("SELECT id FROM work WHERE library_id = {raws}"));
    for table in ["progress", "note"] {
        assert_eq!(
            scalar(
                &conn,
                &format!("SELECT count(*) FROM {table} WHERE work_id = {split} AND edition = 'raw'")
            ),
            1,
            "{table}: `raw`'s row went with `raw`"
        );
        assert_eq!(
            scalar(&conn, &format!("SELECT count(*) FROM {table} WHERE work_id = {id}")),
            1,
            "{table}: `en`'s stayed"
        );
    }
    assert_eq!(scalar(&conn, "SELECT count(*) FROM alignment"), 0);
}

/// Case 5 with the hub moving: the row, its slug and its hand title go to
/// the target with the hub, and the edition that stays is a new work where
/// it stays, with its own position.
#[test]
fn a_divided_work_goes_with_its_hub_and_the_edition_that_stays_keeps_its_history() {
    let dir = tempdir("relocate-split-moves");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_folder(&conn, &dir.path().join("hub"), "Manga", Some("en"), &[("Series", "v01", 2)]);
    a_folder(&conn, &dir.path().join("raw"), "Manga", None, &[("Series", "v01", 3)]);
    scanned(&conn);
    let (id, slug, home) = the_work(&conn, "Series%");
    for edition in ["en", "raw"] {
        a_position_and_a_note(&conn, id, edition);
    }
    conn.execute(
        "UPDATE work SET title = 'By Hand', title_hand = 'By Hand' WHERE id = ?1",
        [id],
    )
    .expect("a hand title");
    derive::derive(&conn).expect("derives");
    assert_eq!(
        the_work(&conn, "By Hand").0,
        id,
        "the fixture: a derive leaves a hand title standing. If it does not, set the \
         title the way `librarydoc` does and keep this assertion"
    );

    let report = moved(&conn, &moving, Target::New("English"));

    let english = library_called(&conn, "English");
    assert_eq!(report.works_split, 1);
    assert_eq!(the_work(&conn, "By Hand"), (id, slug, Some(english)));
    assert_eq!(
        scalar(&conn, &format!("SELECT count(*) FROM edition WHERE work_id = {id} AND key = 'en'")),
        1
    );
    assert_eq!(
        scalar(&conn, &format!("SELECT count(*) FROM edition WHERE work_id = {id}")),
        1
    );
    let stayed: i64 = scalar(
        &conn,
        &format!("SELECT id FROM work WHERE library_id = {}", home.expect("filed")),
    );
    assert_ne!(stayed, id);
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT count(*) FROM progress WHERE work_id = {stayed} AND edition = 'raw'")
        ),
        1
    );
    assert_eq!(
        scalar(
            &conn,
            &format!("SELECT count(*) FROM progress WHERE work_id = {id} AND edition = 'en'")
        ),
        1
    );
}

/// A rehearsal is the move rolled back: the same answer, and neither the
/// store nor the folders' `.taiyaku/` directories changed. Both folders keep
/// documents, so a derive that reconciled would write there.
#[test]
fn a_rehearsal_answers_what_the_move_would_and_leaves_everything_as_it_was() {
    let dir = tempdir("relocate-rehearsal");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    a_folder(&conn, &dir.path().join("kept"), "A", None, &[("Kept", "v01", 2)]);
    let moving = a_folder(&conn, &dir.path().join("moved"), "A", None, &[("Moved", "v01", 3)]);
    scan::update_root(
        &conn,
        moving.id,
        &RootPatch {
            keeps_documents: Some(true),
            ..Default::default()
        },
    )
    .expect("keeps documents");
    scanned(&conn);
    let (id, _, _) = the_work(&conn, "Moved%");
    a_position_and_a_note(&conn, id, "raw");
    let files = |root: &Path| -> Vec<String> {
        let mut found = Vec::new();
        let mut pending = vec![root.to_path_buf()];
        while let Some(next) = pending.pop() {
            for entry in std::fs::read_dir(&next).expect("lists").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    pending.push(path);
                } else {
                    let modified = entry.metadata().expect("stats").modified().expect("mtime");
                    found.push(format!("{} {modified:?}", path.display()));
                }
            }
        }
        found.sort();
        found
    };
    let before = (dump(&conn), files(dir.path()));

    let (rehearsed, _derived) =
        move_cost(&conn, moving.id, Target::New("Fresh")).expect("rehearses");

    assert_eq!((dump(&conn), files(dir.path())), before);
    assert_eq!(crate::libraries::id_of(&conn, "Fresh").expect("reads"), None);

    let (done, _derived) = move_root(&conn, moving.id, Target::New("Fresh")).expect("moves");
    assert_eq!(
        Moved { library_id: 0, ..done.clone() },
        Moved { library_id: 0, ..rehearsed },
        "the move did what the rehearsal said, in a library made twice"
    );
    assert_eq!((done.works_kept, done.positions, done.notes), (1, 1, 1));
}

/// The pre-checks are asked by the rehearsal and the move alike, and a
/// refused move leaves the store as it was, the library it would have made
/// included.
#[test]
fn a_refused_move_changes_nothing_whether_rehearsed_or_made() {
    let dir = tempdir("relocate-refused");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_folder(&conn, &dir.path().join("a"), "Chapters", None, &[("Series", "v01", 2)]);
    let target = a_folder(&conn, &dir.path().join("b"), "Manga", None, &[("Series", "v01", 3)]);
    scanned(&conn);
    let before = dump(&conn);

    for attempt in [
        move_cost(&conn, moving.id, Target::Library(target.library_id)),
        move_root(&conn, moving.id, Target::Library(target.library_id)),
    ] {
        assert!(
            matches!(&attempt, Err(MoveError::Refused { volumes, .. }) if volumes.len() == 1),
            "{attempt:?}"
        );
    }
    assert_eq!(dump(&conn), before);
    assert!(matches!(
        move_root(&conn, moving.id, Target::Library(moving.library_id)),
        Err(MoveError::Unusable(_))
    ));
    assert!(matches!(
        move_root(&conn, 4242, Target::New("X")),
        Err(MoveError::Missing(_))
    ));
    assert_eq!(dump(&conn), before);
}

/// The verdict, where no pre-check sees it coming. A position on the target's
/// work under the key the moved edition will arrive as, which no edition
/// there holds yet: the fold meets it and one of the two positions stops
/// existing. The move is refused on the count.
#[test]
fn a_move_that_would_lose_a_position_is_refused_by_the_verdict() {
    let dir = tempdir("relocate-verdict");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let moving = a_folder(
        &conn,
        &dir.path().join("chapters"),
        "Chapters",
        Some("chapters"),
        &[("Series", "v01", 2)],
    );
    let volumes = a_folder(&conn, &dir.path().join("volumes"), "Manga", None, &[("Series", "v01", 3)]);
    scanned(&conn);
    let joining: i64 = scalar(
        &conn,
        &format!("SELECT id FROM work WHERE library_id = {}", moving.library_id),
    );
    let target: i64 = scalar(
        &conn,
        &format!("SELECT id FROM work WHERE library_id = {}", volumes.library_id),
    );
    for work in [joining, target] {
        conn.execute(
            "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'chapters', 1)",
            [work],
        )
        .expect("a position under that key on each");
    }
    let before = dump(&conn);

    let refused = move_root(&conn, moving.id, Target::Library(volumes.library_id));

    assert!(
        matches!(&refused, Err(MoveError::Refused { why, .. }) if why.contains("reading position")),
        "{refused:?}"
    );
    assert_eq!(dump(&conn), before);
}
```

Add four more, in the same shape, each with the assertions named:

- `the_works_of_a_series_the_target_holds_join_it`: `A` holds `Series v01`
  and `B` holds `Series v02`, each one folder; move `A`'s folder to `B`.
  Assert the moved work kept its id and slug, both works have B's
  `series_id`, `report.series_joined == 1`, and `report.works_kept == 1`.
- `a_series_that_keeps_works_behind_stays_and_the_target_gets_one_of_its_title`:
  library `A` of two folders each holding one volume of `Series`; move one
  folder to a new library. Assert the moved work kept its id and slug, its
  `series_id` is a series of the new library with that `sort_title`, and the
  other volume's `series_id` is unchanged and still in `A`.
- `a_new_library_is_made_for_the_folder_and_named_in_the_answer`:
  `Target::New(" Extras ")`; assert `report.library == "Extras"`,
  `report.library_id` is that library's, and the folder's `root.library_id`
  is it.
- `a_move_with_documents_kept_leaves_the_works_document_where_it_was`: the
  moved folder has `keeps_documents` on; after `scanned`, read
  `crate::hand::path_for(&conn, id)` and assert the file exists; move the
  folder; assert `path_for` answers the same path, the file is still there,
  and the work's id, slug and content key are unchanged.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p taiyaku-library relocate:: -- --test-threads=8`
Expected: FAIL to compile, `cannot find function `move_root``.

- [ ] **Step 3: Implement**

In `crates/taiyaku-library/src/relocate.rs`, take Task 2's `dead_code`
expectation off the module, add
`use std::collections::{BTreeMap, BTreeSet};` and
`use crate::derive::{DeriveReport, Documents, derive_with};`, and add:

```rust
/// What a move did, or in a rehearsal would do.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Moved {
    pub library_id: i64,
    pub library: String,
    pub works_kept: usize,
    pub works_joined: usize,
    pub works_split: usize,
    pub series_joined: usize,
    pub positions: usize,
    pub notes: usize,
    pub alignments_dropped: usize,
    pub library_removed: Option<String>,
    pub grants_removed: usize,
}

/// What the store held before a move, for the verdict after it.
struct Census {
    /// Works every member of which is under the folder, with their titles.
    whole: BTreeMap<i64, String>,
    /// Works with a member under the folder and a member elsewhere.
    divided: BTreeMap<i64, String>,
    /// Positions and notes on those works.
    positions: usize,
    notes: usize,
    /// How many of each the whole store holds.
    all_positions: i64,
    all_notes: i64,
    all_truth_pages: i64,
    /// Every work filed under a library.
    filed: BTreeSet<i64>,
    /// The series the target held before the move.
    target_series: BTreeSet<i64>,
    /// The folder's library, where the folder is its last, and its grants.
    library_removed: Option<String>,
    grants_removed: usize,
}

fn ids_and_titles(
    conn: &Connection,
    sql: &str,
    root_id: i64,
) -> Result<BTreeMap<i64, String>, String> {
    let mut statement = conn
        .prepare(sql)
        .map_err(|e| format!("reading the works under root {root_id}: {e}"))?;
    let rows = statement
        .query_map([root_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|e| format!("reading the works under root {root_id}: {e}"))?;
    rows.collect::<Result<_, _>>()
        .map_err(|e| format!("reading the works under root {root_id}: {e}"))
}

fn count(conn: &Connection, sql: &str, params: &[&dyn rusqlite::ToSql]) -> Result<i64, String> {
    conn.query_row(sql, params, |r| r.get(0))
        .map_err(|e| format!("counting for a move: {e}"))
}

impl Census {
    fn take(conn: &Connection, folder: &Folder, target_id: i64) -> Result<Self, String> {
        let under = "EXISTS (SELECT 1 FROM edition e JOIN member m ON m.edition_id = e.id
                              WHERE e.work_id = w.id AND m.root_id = ?1)";
        let elsewhere = "EXISTS (SELECT 1 FROM edition e JOIN member m ON m.edition_id = e.id
                                  WHERE e.work_id = w.id AND m.root_id IS NOT ?1)";
        let whole = ids_and_titles(
            conn,
            &format!("SELECT w.id, w.title FROM work w WHERE {under} AND NOT {elsewhere}"),
            folder.id,
        )?;
        let divided = ids_and_titles(
            conn,
            &format!("SELECT w.id, w.title FROM work w WHERE {under} AND {elsewhere}"),
            folder.id,
        )?;
        let on_them = |table: &str| {
            count(
                conn,
                &format!("SELECT count(*) FROM {table} t JOIN work w ON w.id = t.work_id WHERE {under}"),
                &[&folder.id],
            )
        };
        let last = count(
            conn,
            "SELECT count(*) FROM root WHERE library_id = ?1 AND id <> ?2",
            &[&folder.library_id, &folder.id],
        )? == 0;
        let filed = conn
            .prepare("SELECT id FROM work WHERE library_id IS NOT NULL")
            .map_err(|e| format!("reading the filed works: {e}"))?
            .query_map([], |r| r.get(0))
            .map_err(|e| format!("reading the filed works: {e}"))?
            .collect::<Result<_, _>>()
            .map_err(|e| format!("reading the filed works: {e}"))?;
        let target_series = conn
            .prepare("SELECT id FROM series WHERE library_id = ?1")
            .map_err(|e| format!("reading the series of library {target_id}: {e}"))?
            .query_map([target_id], |r| r.get(0))
            .map_err(|e| format!("reading the series of library {target_id}: {e}"))?
            .collect::<Result<_, _>>()
            .map_err(|e| format!("reading the series of library {target_id}: {e}"))?;
        Ok(Self {
            positions: usize::try_from(on_them("progress")?).unwrap_or(usize::MAX),
            notes: usize::try_from(on_them("note")?).unwrap_or(usize::MAX),
            whole,
            divided,
            all_positions: count(conn, "SELECT count(*) FROM progress", &[])?,
            all_notes: count(conn, "SELECT count(*) FROM note", &[])?,
            all_truth_pages: count(conn, "SELECT count(*) FROM truth_page", &[])?,
            filed,
            target_series,
            library_removed: last.then(|| folder.library.clone()),
            grants_removed: if last {
                usize::try_from(count(
                    conn,
                    "SELECT count(*) FROM user_library WHERE library_id = ?1",
                    &[&folder.library_id],
                )?)
                .unwrap_or(usize::MAX)
            } else {
                0
            },
        })
    }
}

/// File under `target_id` each work whose hub edition is under the folder,
/// and each series all of whose works were so filed, unless the target holds
/// a series of its title: the works then join that one at the derive.
///
/// **Before the folder's own library changes**, so that a library migration
/// 15 deletes with its last folder takes no row's `library_id` with it.
///
/// **A work the target already holds is left where it is.** One of the same
/// content key cannot be filed there: `work` is unique on its content key
/// within a library. One at the same address, its series' `sort_title` and
/// its volume, is the volume the target already has: the derive writes the
/// folder's files onto the target's row as another edition, and folds this
/// one into it.
///
/// **Why filing is enough.** A derive finds the row a group of files belongs
/// on by content key within the group's library (`import::scanned_in`)
/// before it asks for the address, and the key is a hash of the hub
/// edition's members, which a move does not touch.
fn refile(conn: &Connection, folder: &Folder, target_id: i64) -> Result<(), String> {
    let moving: Vec<(i64, Option<i64>)> = conn
        .prepare(
            "SELECT w.id, w.series_id
               FROM work w
               JOIN edition e ON e.work_id = w.id AND e.key = w.hub
              WHERE EXISTS (SELECT 1 FROM member m WHERE m.edition_id = e.id AND m.root_id = ?1)
                AND NOT EXISTS (SELECT 1 FROM work tw
                                 WHERE tw.library_id = ?2 AND tw.id <> w.id
                                   AND tw.content_key = w.content_key)
                AND NOT (w.volume IS NOT NULL AND EXISTS (
                           SELECT 1 FROM work tw
                             JOIN series ts ON ts.id = tw.series_id
                             JOIN series s ON s.id = w.series_id
                            WHERE tw.library_id = ?2 AND tw.id <> w.id
                              AND ts.sort_title = s.sort_title AND tw.volume = w.volume))
              ORDER BY w.id",
        )
        .map_err(|e| format!("reading the works to file under library {target_id}: {e}"))?
        .query_map([folder.id, target_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|e| format!("reading the works to file under library {target_id}: {e}"))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("reading the works to file under library {target_id}: {e}"))?;
    for (work, _) in &moving {
        conn.execute(
            "UPDATE work SET library_id = ?2 WHERE id = ?1",
            [*work, target_id],
        )
        .map_err(|e| format!("filing work {work} under library {target_id}: {e}"))?;
    }

    let series: BTreeSet<i64> = moving.iter().filter_map(|(_, series)| *series).collect();
    for series in series {
        let keeps_works_behind = count(
            conn,
            "SELECT count(*) FROM work WHERE series_id = ?1 AND library_id IS NOT ?2",
            &[&series, &target_id],
        )? > 0;
        if keeps_works_behind {
            continue;
        }
        let held = count(
            conn,
            "SELECT count(*) FROM series t JOIN series s ON s.id = ?1
              WHERE t.library_id = ?2 AND t.id <> s.id AND t.sort_title = s.sort_title",
            &[&series, &target_id],
        )? > 0;
        if held {
            continue;
        }
        conn.execute(
            "UPDATE series SET library_id = ?2 WHERE id = ?1",
            [series, target_id],
        )
        .map_err(|e| format!("filing series {series} under library {target_id}: {e}"))?;
    }
    Ok(())
}

/// Give each divided work's misfiled editions to the work of the library
/// their files are in. Answers what was carried, and each work for which an
/// edition had nowhere to go or was refused.
///
/// After the derive, a divided work still holds the editions whose files are
/// now another library's: a derive leaves standing an edition it did not
/// name. The same derive made, in that other library, a work holding a fresh
/// edition of the same key over the same members.
fn split(
    conn: &Connection,
    divided: &BTreeMap<i64, String>,
) -> Result<(crate::fold::Rehomed, Vec<String>), String> {
    let mut carried = crate::fold::Rehomed::default();
    let mut unplaced = Vec::new();
    for (&work, title) in divided {
        // An edition whose members are all under folders of one library,
        // which is not this work's, with the work of that library that
        // holds the same key over a member of the same folder, path and
        // digest.
        let misfiled: Vec<(String, Option<i64>)> = conn
            .prepare(
                "SELECT e.key,
                        (SELECT te.work_id
                           FROM member m
                           JOIN member tm ON tm.root_id = m.root_id AND tm.path = m.path
                                         AND tm.sha256 = m.sha256
                           JOIN edition te ON te.id = tm.edition_id AND te.key = e.key
                                          AND te.id <> e.id
                           JOIN work tw ON tw.id = te.work_id
                          WHERE m.edition_id = e.id
                            AND tw.library_id = (SELECT min(r.library_id)
                                                   FROM member m2 JOIN root r ON r.id = m2.root_id
                                                  WHERE m2.edition_id = e.id)
                          LIMIT 1)
                   FROM edition e
                   JOIN work w ON w.id = e.work_id
                  WHERE e.work_id = ?1
                    AND (SELECT count(DISTINCT r.library_id)
                           FROM member m JOIN root r ON r.id = m.root_id
                          WHERE m.edition_id = e.id) = 1
                    AND NOT EXISTS (SELECT 1 FROM member m
                                     WHERE m.edition_id = e.id AND m.root_id IS NULL)
                    AND (SELECT min(r.library_id)
                           FROM member m JOIN root r ON r.id = m.root_id
                          WHERE m.edition_id = e.id) IS NOT w.library_id
                  ORDER BY e.key",
            )
            .map_err(|e| format!("reading the misfiled editions of work {work}: {e}"))?
            .query_map([work], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| format!("reading the misfiled editions of work {work}: {e}"))?
            .collect::<Result<_, _>>()
            .map_err(|e| format!("reading the misfiled editions of work {work}: {e}"))?;

        let mut by_twin: BTreeMap<i64, Vec<String>> = BTreeMap::new();
        for (key, twin) in misfiled {
            match twin {
                Some(twin) => by_twin.entry(twin).or_default().push(key),
                None => unplaced.push(format!("{title} ({key})")),
            }
        }
        for (twin, keys) in by_twin {
            match crate::fold::rehome_editions(conn, work, twin, &keys)? {
                crate::fold::Rehoming::Done(moved) => {
                    carried.positions += moved.positions;
                    carried.notes += moved.notes;
                    carried.truth_pages += moved.truth_pages;
                    carried.alignments_dropped += moved.alignments_dropped;
                }
                crate::fold::Rehoming::Refused(_) => {
                    unplaced.extend(keys.iter().map(|key| format!("{title} ({key})")));
                }
            }
        }
    }
    Ok((carried, unplaced))
}

fn refused(folder: &Folder, library: &str, why: &str, volumes: Vec<String>) -> MoveError {
    let named = if volumes.is_empty() {
        String::new()
    } else {
        format!(": {}", first_few(&volumes))
    };
    MoveError::Refused {
        why: format!(
            "{} cannot be moved to `{library}` and keep every reader's place: {why}{named}. \
             Nothing was changed",
            folder.path
        ),
        volumes,
    }
}

/// The move, inside the caller's transaction.
fn apply_move(
    conn: &Connection,
    root_id: i64,
    target: Target<'_>,
) -> Result<(Moved, DeriveReport), MoveError> {
    let folder = folder(conn, root_id)?;
    let target_id = resolve(conn, &folder, target)?;
    if let Some(refusal) = refused_before(conn, &folder, target_id).map_err(MoveError::Failed)? {
        return Err(refusal);
    }
    let library = crate::libraries::name_of(conn, target_id)
        .map_err(MoveError::Failed)?
        .unwrap_or_default();

    // What a derive reports before the move is not the move's doing.
    let baseline = derive_with(conn, Documents::Skip).map_err(MoveError::Failed)?;
    let census = Census::take(conn, &folder, target_id).map_err(MoveError::Failed)?;

    refile(conn, &folder, target_id).map_err(MoveError::Failed)?;
    conn.execute(
        "UPDATE root SET library_id = ?2 WHERE id = ?1",
        [folder.id, target_id],
    )
    .map_err(|e| MoveError::Failed(format!("moving root {root_id}: {e}")))?;
    let derived = derive_with(conn, Documents::Skip).map_err(MoveError::Failed)?;
    let (carried, unplaced) = split(conn, &census.divided).map_err(MoveError::Failed)?;

    // The verdict. Each arm is a refusal of the whole move.
    if !unplaced.is_empty() {
        return Err(refused(
            &folder,
            &library,
            "an edition the move separates from its work has no work to go to",
            unplaced,
        ));
    }
    if derived.unfolded_works > baseline.unfolded_works {
        let new: Vec<String> = derived
            .unfoldable
            .iter()
            .filter(|why| !baseline.unfoldable.contains(why))
            .cloned()
            .collect();
        return Err(refused(
            &folder,
            &library,
            "a work could not be joined to the one that holds its volume",
            new,
        ));
    }
    let titles = |sql: &str| -> Result<Vec<(i64, String)>, MoveError> {
        conn.prepare(sql)
            .and_then(|mut statement| {
                statement
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(|e| MoveError::Failed(format!("reading a move's outcome: {e}")))
    };
    let standing = titles(&format!(
        "SELECT id, title FROM work WHERE library_id IS NOT {target_id} ORDER BY title"
    ))?;
    let outside: Vec<String> = standing
        .iter()
        .filter(|(id, _)| census.whole.contains_key(id))
        .map(|(_, title)| title.clone())
        .collect();
    if !outside.is_empty() {
        return Err(refused(
            &folder,
            &library,
            "works of the folder would be left outside it",
            outside,
        ));
    }
    let unfiled: Vec<String> = titles("SELECT id, title FROM work WHERE library_id IS NULL ORDER BY title")?
        .into_iter()
        .filter(|(id, _)| census.filed.contains(id))
        .map(|(_, title)| title)
        .collect();
    if !unfiled.is_empty() {
        return Err(refused(
            &folder,
            &library,
            "works would be left in no library",
            unfiled,
        ));
    }
    let after = |table: &str| {
        count(conn, &format!("SELECT count(*) FROM {table}"), &[]).map_err(MoveError::Failed)
    };
    for (table, before, what) in [
        ("progress", census.all_positions, "reading position"),
        ("note", census.all_notes, "note"),
        ("truth_page", census.all_truth_pages, "ground-truth page"),
    ] {
        let lost = before - after(table)?;
        if lost > 0 {
            return Err(refused(
                &folder,
                &library,
                &format!("{lost} {what}(s) would stop existing"),
                Vec::new(),
            ));
        }
    }
    if derived.lost_positions > 0 || derived.dropped_notes > 0 {
        return Err(refused(
            &folder,
            &library,
            "the derive reports a position lost or a note dropped",
            Vec::new(),
        ));
    }

    let still: BTreeSet<i64> = titles("SELECT id, title FROM work")?
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let works_kept = census.whole.keys().filter(|id| still.contains(id)).count();
    // Each series the target held before that now holds a work with a file
    // under the folder.
    let gained: BTreeSet<i64> = conn
        .prepare(
            "SELECT DISTINCT w.series_id FROM work w
              WHERE w.series_id IS NOT NULL
                AND EXISTS (SELECT 1 FROM edition e JOIN member m ON m.edition_id = e.id
                             WHERE e.work_id = w.id AND m.root_id = ?1)",
        )
        .and_then(|mut statement| {
            statement
                .query_map([folder.id], |r| r.get(0))?
                .collect::<Result<BTreeSet<i64>, _>>()
        })
        .map_err(|e| MoveError::Failed(format!("reading a move's outcome: {e}")))?;
    let series_joined = gained.intersection(&census.target_series).count();
    Ok((
        Moved {
            library_id: target_id,
            library,
            works_kept,
            works_joined: census.whole.len() - works_kept,
            works_split: census.divided.len(),
            series_joined,
            positions: census.positions,
            notes: census.notes,
            alignments_dropped: carried.alignments_dropped,
            library_removed: census.library_removed,
            grants_removed: census.grants_removed,
        },
        derived,
    ))
}

/// What moving root `root_id` to `target` would do, found by doing it inside
/// a transaction this rolls back. It writes no file: each derive in it runs
/// under [`Documents::Skip`].
///
/// # Errors
///
/// See [`MoveError`]. A move that would be refused is refused here in the
/// same words.
pub fn move_cost(
    conn: &Connection,
    root_id: i64,
    target: Target<'_>,
) -> Result<(Moved, DeriveReport), MoveError> {
    let tx = crate::Nested::open(conn, "move_cost")
        .map_err(|e| MoveError::Failed(format!("starting to rehearse a move: {e}")))?;
    let answer = apply_move(&tx, root_id, target);
    // Not committed: dropping `tx` is the rollback, whatever the answer.
    drop(tx);
    answer
}

/// Move root `root_id` to `target`, then derive again with documents
/// reconciled, after the commit, for `scan::remove_root`'s reason: a
/// document is written or renamed only once the rows it follows are
/// committed.
///
/// The [`DeriveReport`] answered is that second pass's.
///
/// # Errors
///
/// See [`MoveError`]. A refused move changed nothing. A failure of the pass
/// after the commit is [`MoveError::Failed`], and by then the move is made.
pub fn move_root(
    conn: &Connection,
    root_id: i64,
    target: Target<'_>,
) -> Result<(Moved, DeriveReport), MoveError> {
    let tx = crate::Nested::open(conn, "move_root")
        .map_err(|e| MoveError::Failed(format!("starting a move: {e}")))?;
    let (moved, _predicted) = apply_move(&tx, root_id, target)?;
    tx.commit()
        .map_err(|e| MoveError::Failed(format!("committing the move of root {root_id}: {e}")))?;
    let derived = crate::derive::derive(conn).map_err(|e| {
        MoveError::Failed(format!("reconciling documents after moving root {root_id}: {e}"))
    })?;
    Ok((moved, derived))
}
```

- [ ] **Step 4: Run the tests, and read what the store did**

Run: `cargo test -p taiyaku-library relocate:: -- --test-threads=8`

Expected: every test passes. Each rests on something the probe showed the
derive do, in a store arranged by hand. Where one fails, compare the rows
with the probe's log for that case first, in this order:

1. *A work filed under the target is found by the derive and written in
   place* (`import::scanned_in`'s content probe). If
   `a_folder_leaving_a_library_that_keeps_another_keeps_its_works_as_they_were`
   shows a new id, print the two work rows and their content keys.
2. *A work the target holds at the same address under another key takes the
   moved files as an edition, and the moved work is folded into it*
   (`derive::held_work`, `derive::superseded`, `fold::absorb`).
3. *A divided work keeps the editions the derive did not name, and the other
   side gets a fresh work* (`DeriveReport::kept_editions`; F1's
   `a_work_left_spanning_two_libraries_is_split_and_keeps_the_other_librarys_edition`).
4. *A series filed under the target is the one `reconcile_series` finds.*

Report each failure with the rows, the derive's report, and what the probe's
log shows for the same case. Do not change the test's expectation to match,
and do not change `derive.rs`. If the move cannot be made to do what a test
says, the design needs the user.

- [ ] **Step 5: Watch each fail against its defect**

One at a time, restoring between:

1. `apply_move`: delete the `refile` call. Expect
   `a_folder_leaving_a_library_that_keeps_another_keeps_its_works_as_they_were`
   to fail: the work has a new id.
2. `refile`: delete the series loop's `UPDATE series`. Expect the same test
   to fail at "the series went too".
3. `refile`: delete the address `AND NOT (w.volume IS NOT NULL AND EXISTS …)`
   clause. Expect
   `a_volume_the_target_holds_under_another_key_joins_it_as_an_edition` to
   fail, or the move to be refused: say which.
4. `apply_move`: delete the `split` call (use
   `(crate::fold::Rehomed::default(), Vec::new())`). Expect both
   `a_divided_work_…` tests to fail.
5. `apply_move`: delete the count loop over `progress`, `note` and
   `truth_page`, and the `lost_positions` check after it. Expect
   `a_move_that_would_lose_a_position_is_refused_by_the_verdict` to fail.
6. `move_cost`: replace `drop(tx);` with a commit when the answer is `Ok`.
   Expect `a_rehearsal_answers_what_the_move_would_and_leaves_everything_as_it_was`
   to fail.
7. `apply_move`: derive under `Documents::Reconcile` in place of the second
   `Documents::Skip`. Expect the same test to fail on the files, if the
   fixture's folder wrote a document; if it does not fail, the fixture holds
   no document to write, so give the work a hand title through the API
   `crate::hand`'s own tests use, and watch it fail then.

- [ ] **Step 6: Run O190's case and write down what it does**

One test, asserting nothing about proposals that the store does not already
promise: two folders of one library holding series a proposal pairs (take
the fixture from `proposals/tests.rs` that raises an `edition-of` proposal),
`scanned`, read the open proposal's id, move one of the two folders to a new
library, and print the proposal's row and what
`proposals::acceptable` (or the function `--accept` calls) answers for it.
Put the output in `$EVID/o190-after-a-move.log`. Assert only that the move
succeeded and the proposal row still exists. Task 6 writes what the log
shows into O190.

- [ ] **Step 7: Whole crates, then commit**

Run the three commands of Task 1's Step 6. Expected: all pass.

```bash
git add crates/taiyaku-library/src
git commit -m "relocate: a folder moves with its works' rows, a divided work is split, and a rehearsal says what would happen"
```

---

## Task 4: The route

**Files:**
- Modify: `crates/taiyaku-serve/src/libraries_api.rs`,
  `crates/taiyaku-serve/src/libraries_api/tests.rs`
- Modify: `crates/taiyaku-serve/src/routes.rs`,
  `crates/taiyaku-serve/src/auth/tests.rs`

Read "Read this first" at the top of this plan before starting.

**Interfaces:**
- Consumes: Task 3's `relocate::{move_cost, move_root, Target, Moved,
  MoveError}`; F2a's `LibraryAnswer`, `library_panicked`,
  `config_api::derive_report_json`; `ServeState::library_job`.
- Produces, admin-only:

  `POST /api/library/roots/{id}/move[?dry_run=true]`, body
  `{"library": <id>}` or `{"new_library": "<name>"}`.

  | Answer | Body |
  | --- | --- |
  | 200 | `{"moved": Moved, "derive": {…}, "dry_run": bool}` |
  | 400 | `{"error"}`: a body naming both targets, neither, or the wrong type; the folder's own library; a new library's name empty or taken; a misspelt query key |
  | 404 | `{"error"}`: no such folder, or no such library |
  | 409 | `{"error", "volumes": [..]}`: the move is refused, or a scan or a rule save holds `library_job` (`volumes` is then empty) |

  `Moved` is `{"library_id", "library", "works_kept", "works_joined",
  "works_split", "series_joined", "positions", "notes",
  "alignments_dropped", "library_removed": name or null, "grants_removed"}`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-serve/src/libraries_api/tests.rs`, using that
file's `answered`, `a_folder_of` and `the_library`:

```rust
async fn moved(
    router: &axum::Router,
    folder: i64,
    dry_run: bool,
    body: &serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let query = if dry_run { "?dry_run=true" } else { "" };
    answered(
        router,
        Method::POST,
        &format!("/api/library/roots/{folder}/move{query}"),
        body,
    )
    .await
}

/// Two libraries of one folder each. The rehearsal says what the move would
/// do and changes nothing; the move does it.
#[tokio::test]
async fn a_folder_is_rehearsed_and_then_moved_to_another_library() {
    let (router, _state, _fixture) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let a = the_library(&listed, "root-a")["id"].as_i64().expect("an id");
    let folder = the_library(&listed, "root-b")["folders"][0]["id"]
        .as_i64()
        .expect("a folder id");
    let to = serde_json::json!({ "library": a });

    let (status, rehearsed) = moved(&router, folder, true, &to).await;
    assert_eq!(status, StatusCode::OK, "{rehearsed}");
    assert_eq!(rehearsed["dry_run"], true);
    assert_eq!(rehearsed["moved"]["library"], "root-a");
    assert_eq!(rehearsed["moved"]["works_kept"], 1, "{rehearsed}");
    assert_eq!(rehearsed["moved"]["library_removed"], "root-b", "{rehearsed}");
    assert!(rehearsed["derive"].is_object(), "{rehearsed}");
    assert_eq!(get_json(&router, "/api/library/libraries").await, listed);

    let (status, done) = moved(&router, folder, false, &to).await;
    assert_eq!(status, StatusCode::OK, "{done}");
    assert_eq!(done["dry_run"], false);
    assert_eq!(done["moved"], rehearsed["moved"]);
    let after = get_json(&router, "/api/library/libraries").await;
    let libraries = after["libraries"].as_array().expect("libraries");
    assert_eq!(libraries.len(), 1, "{after}");
    assert_eq!(libraries[0]["folders"].as_array().expect("folders").len(), 2);
    assert_eq!(libraries[0]["works"], 2, "{after}");
}

#[tokio::test]
async fn a_folder_is_moved_to_a_library_made_for_it() {
    let (router, _state, _held) = test_support::state_with_two_scanned_roots();
    let fixture = test_support::Fixture::new("move-new");
    let listed = get_json(&router, "/api/library/libraries").await;
    let a = the_library(&listed, "root-a");
    let (status, added) = answered(
        &router,
        Method::POST,
        &format!("/api/library/libraries/{}/folders", a["id"]),
        &serde_json::json!({ "path": a_folder_of(&fixture, "Extra", "Gamma") }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{added}");
    let folder = added["root"]["id"].as_i64().expect("a folder id");

    let (status, done) =
        moved(&router, folder, false, &serde_json::json!({ "new_library": " Extras " })).await;

    assert_eq!(status, StatusCode::OK, "{done}");
    assert_eq!(done["moved"]["library"], "Extras");
    assert_eq!(done["moved"]["library_removed"], serde_json::Value::Null);
    let after = get_json(&router, "/api/library/libraries").await;
    assert_eq!(the_library(&after, "Extras")["folders"][0]["id"], folder);
    assert_eq!(the_library(&after, "root-a")["folders"].as_array().expect("folders").len(), 1);
}

/// The same volume under the same edition key in both libraries: 409, the
/// volume named in the sentence and listed beside it, and nothing changed.
#[tokio::test]
async fn a_refused_move_is_a_409_naming_its_volumes() {
    let (router, _state, _held) = test_support::router_over_a_single_scanned_work();
    let fixture = test_support::Fixture::new("move-refused");
    let mut ids = Vec::new();
    for name in ["One", "Two"] {
        let (status, made) = answered(
            &router,
            Method::POST,
            "/api/library/libraries",
            &serde_json::json!({ "name": name, "folders": [a_folder_of(&fixture, name, "Same")] }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{made}");
        let id = made["library"]["id"].as_i64().expect("an id");
        let (status, _) = answered(
            &router,
            Method::POST,
            &format!("/api/library/libraries/{id}/scan"),
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::ACCEPTED);
        until_the_library_scan_ends(&router, id).await;
        ids.push((id, made["library"]["folders"][0]["id"].as_i64().expect("a folder")));
    }
    let listed = get_json(&router, "/api/library/libraries").await;

    for dry_run in [true, false] {
        let (status, refused) =
            moved(&router, ids[0].1, dry_run, &serde_json::json!({ "library": ids[1].0 })).await;
        assert_eq!(status, StatusCode::CONFLICT, "{refused}");
        assert!(
            refused["error"].as_str().expect("an error").contains("same edition key"),
            "{refused}"
        );
        assert_eq!(refused["volumes"].as_array().expect("volumes").len(), 1, "{refused}");
    }
    assert_eq!(get_json(&router, "/api/library/libraries").await, listed);
}

#[tokio::test]
async fn a_move_that_names_no_one_target_or_nothing_there_is_the_callers_mistake() {
    let (router, _state, _fixture) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let a = the_library(&listed, "root-a");
    let own = a["id"].as_i64().expect("an id");
    let folder = a["folders"][0]["id"].as_i64().expect("a folder id");

    for (body, status, says) in [
        (serde_json::json!({}), StatusCode::BAD_REQUEST, "exactly one"),
        (
            serde_json::json!({ "library": 1, "new_library": "X" }),
            StatusCode::BAD_REQUEST,
            "exactly one",
        ),
        (serde_json::json!({ "library": "one" }), StatusCode::BAD_REQUEST, "`library`"),
        (serde_json::json!({ "new_library": 7 }), StatusCode::BAD_REQUEST, "`new_library`"),
        (serde_json::json!({ "new_library": "  " }), StatusCode::BAD_REQUEST, "needs a name"),
        (
            serde_json::json!({ "new_library": "root-b" }),
            StatusCode::BAD_REQUEST,
            "already called `root-b`",
        ),
        (serde_json::json!({ "library": own }), StatusCode::BAD_REQUEST, "already in `root-a`"),
        (serde_json::json!({ "library": 4242 }), StatusCode::NOT_FOUND, "4242"),
    ] {
        let (got, refused) = moved(&router, folder, false, &body).await;
        assert_eq!(got, status, "{body}: {refused}");
        assert!(
            refused["error"].as_str().expect("an error").contains(says),
            "{body}: {refused}"
        );
    }
    let (status, missing) =
        moved(&router, 4242, false, &serde_json::json!({ "new_library": "X" })).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{missing}");
    let (status, misspelt) = answered(
        &router,
        Method::POST,
        &format!("/api/library/roots/{folder}/move?dryrun=true"),
        &serde_json::json!({ "new_library": "X" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{misspelt}");
    assert_eq!(get_json(&router, "/api/library/libraries").await, listed);
}

/// A move derives the whole store, so it does not run beside a scan: 409 at
/// once, for a rehearsal too, and nothing moved.
#[tokio::test]
async fn a_move_is_refused_while_a_scan_holds_the_library() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let a = the_library(&listed, "root-a")["id"].as_i64().expect("an id");
    let folder = the_library(&listed, "root-b")["folders"][0]["id"]
        .as_i64()
        .expect("a folder id");
    // Held on a thread of its own, as a scan holds it: a guard held across
    // an `.await` here would be the test's own defect.
    let (release, released) = std::sync::mpsc::channel::<()>();
    let (holding, held) = std::sync::mpsc::channel::<()>();
    let job = std::sync::Arc::clone(&state.library_job);
    let holder = std::thread::spawn(move || {
        let _job = job.lock().expect("the job");
        holding.send(()).expect("says it holds the job");
        released.recv().ok();
    });
    held.recv().expect("the job is held");

    for dry_run in [true, false] {
        let (status, refused) =
            moved(&router, folder, dry_run, &serde_json::json!({ "library": a })).await;
        assert_eq!(status, StatusCode::CONFLICT, "{refused}");
        assert!(refused["error"].as_str().expect("an error").contains("running"));
    }
    release.send(()).expect("lets go");
    holder.join().expect("the holder ends");
    assert_eq!(get_json(&router, "/api/library/libraries").await, listed);
}
```

Add `(Method::POST, "/api/library/roots/1/move")` to
`every_libraries_route_refuses_a_reader`.

Run: `cargo test -p taiyaku-serve libraries_api:: -- --test-threads=8`
Expected: FAIL: 404 or 405 for every move.

- [ ] **Step 2: Implement**

In `crates/taiyaku-serve/src/libraries_api.rs`, add `relocate` to the
`taiyaku_library` import and, before `#[cfg(test)]`:

```rust
/// The one query parameter [`move_folder`] reads. `deny_unknown_fields`, for
/// [`DeleteLibraryQuery`]'s reason: a misspelt `dry_run` must not make the
/// move.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MoveQuery {
    #[serde(default)]
    dry_run: bool,
}

/// Where a move's body says the folder goes.
enum MoveTo {
    Library(i64),
    New(String),
}

/// Exactly one of `library`, a library's id, and `new_library`, a name.
fn move_target_from_json(body: &serde_json::Value) -> Result<MoveTo, String> {
    match (body.get("library"), body.get("new_library")) {
        (Some(id), None) => id
            .as_i64()
            .map(MoveTo::Library)
            .ok_or_else(|| "`library` must be a library id".to_string()),
        (None, Some(name)) => name
            .as_str()
            .map(|name| MoveTo::New(name.to_string()))
            .ok_or_else(|| "`new_library` must be a name, a string".to_string()),
        _ => Err(
            "a move names exactly one of `library`, a library id, and `new_library`, a name"
                .to_string(),
        ),
    }
}

/// [`relocate::Moved`], over the wire.
///
/// Destructured with no `..`, so that a field added to it is a compile error
/// here and not a count the screen never draws.
fn moved_json(moved: &relocate::Moved) -> serde_json::Value {
    let relocate::Moved {
        library_id,
        library,
        works_kept,
        works_joined,
        works_split,
        series_joined,
        positions,
        notes,
        alignments_dropped,
        library_removed,
        grants_removed,
    } = moved;
    serde_json::json!({
        "library_id": library_id,
        "library": library,
        "works_kept": works_kept,
        "works_joined": works_joined,
        "works_split": works_split,
        "series_joined": series_joined,
        "positions": positions,
        "notes": notes,
        "alignments_dropped": alignments_dropped,
        "library_removed": library_removed,
        "grants_removed": grants_removed,
    })
}

/// `POST /api/library/roots/{id}/move`: move a folder to another library or
/// to a new one, or with `?dry_run=true` answer what that would do. §6.2 of
/// the libraries spec; `relocate` is where the move is.
///
/// **It asks for [`ServeState::library_job`] and does not wait for it.** A
/// move derives the whole store, and under a scan's derive or a rule save it
/// would file works by rules or facts the other is replacing. So it answers
/// 409 at once where one holds the lock, as `config_api::save_rules` does,
/// for the rehearsal as for the move: the rehearsal runs the same derives.
///
/// **A refusal is a 409 with its sentence and its volumes.** The sentence
/// names the first few; the list is all of them.
pub(crate) async fn move_folder(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
    Query(query): Query<MoveQuery>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let to = match move_target_from_json(&body) {
        Ok(read) => read,
        Err(why) => return LibraryAnswer::Unusable(why).into_response(),
    };
    let dry_run = query.dry_run;

    let moved = tokio::task::spawn_blocking(move || {
        let _job = match state.library_job.try_lock() {
            Ok(held) => held,
            Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => {
                return Err(relocate::MoveError::Refused {
                    why: "a scan or a rule save is running; move the folder when it has \
                          finished"
                        .to_string(),
                    volumes: Vec::new(),
                });
            }
        };
        let working = state
            .library
            .working()
            .map_err(relocate::MoveError::Failed)?;
        let target = match &to {
            MoveTo::Library(library) => relocate::Target::Library(*library),
            MoveTo::New(name) => relocate::Target::New(name),
        };
        if dry_run {
            relocate::move_cost(&working, id, target)
        } else {
            relocate::move_root(&working, id, target)
        }
    })
    .await;

    match moved {
        Ok(Ok((moved, derived))) => Json(serde_json::json!({
            "moved": moved_json(&moved),
            "derive": derive_report_json(&derived),
            "dry_run": dry_run,
        }))
        .into_response(),
        Ok(Err(relocate::MoveError::Refused { why, volumes })) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": why, "volumes": volumes })),
        )
            .into_response(),
        Ok(Err(relocate::MoveError::Missing(why))) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": why })),
        )
            .into_response(),
        Ok(Err(relocate::MoveError::Unusable(why))) => {
            LibraryAnswer::Unusable(why).into_response()
        }
        Ok(Err(relocate::MoveError::Failed(why))) => LibraryAnswer::Failed(why).into_response(),
        Err(why) => library_panicked("moving a folder", &why),
    }
}
```

In `routes.rs`, `guarded_routes`, beside the folders route:

```rust
        (
            "/api/library/roots/{id}/move",
            axum::routing::post(crate::libraries_api::move_folder),
        ),
```

and in `grant_postures`:

```rust
        (
            "/api/library/roots/{id}/move",
            GrantIndependent {
                reason: "moves one folder between libraries, a write to the \
                          whole store that changes what a library is and so \
                          what a grant on it means, with no account's own \
                          grant to filter it by",
            },
        ),
```

Add the path to both lists in `auth/tests.rs`.

- [ ] **Step 3: Run, watch the defects, commit**

Run: `cargo test -p taiyaku-serve -- --test-threads=8`
Expected: PASS.

Watch each fail, restoring between:

1. `move_folder`: swap `move_cost` and `move_root`. Expect
   `a_folder_is_rehearsed_and_then_moved_to_another_library` to fail.
2. `move_folder`: replace `try_lock()` with `lock()` and delete the
   `WouldBlock` arm. Expect `a_move_is_refused_while_a_scan_holds_the_library`
   to hang: run it alone with a ten-second timeout, and record that it timed
   out.
3. `move_folder`: answer the `Refused` arm with `{ "error": why }` alone.
   Expect `a_refused_move_is_a_409_naming_its_volumes` to fail.
4. `move_target_from_json`: make the `_` arm answer
   `Ok(MoveTo::New(String::new()))`. Expect the mistakes test to fail on `{}`:
   the status is still 400, and the sentence is the store's about a name.
5. Delete the `administering_or_forbidden` check. Expect
   `every_libraries_route_refuses_a_reader` to fail naming the move.

Run the three whole-crate commands of Task 1's Step 6, then:

```bash
git add crates/taiyaku-serve/src
git commit -m "POST /api/library/roots/{id}/move: a folder moved or rehearsed, refused by name, never beside a scan"
```

---

## Task 5: Move… on the folder row

**Files:**
- Modify: `web/src/libraries.ts`, `web/src/libraries.test.ts`,
  `web/src/message.ts`, `web/src/message.test.ts`
- Create: `web/src/MoveRoot.tsx`, `web/src/MoveRoot.test.tsx`
- Modify: `web/src/RootRow.tsx`, `web/src/LibraryCard.tsx`,
  `web/src/Libraries.tsx` and their tests
- Modify: `web/src/styles/admin.css`,
  `web/src/styles/admin-config.css.test.ts`,
  `web/src/Config.browser.test.tsx`

**Interfaces:**
- Consumes: Task 4's route; F2a's `sentenceOf`, `Library`, `RootRow`,
  `LibraryCard`, `testing/input.ts`, `testing/derive.ts`.
- Produces:
  ```ts
  // libraries.ts
  export type MoveTarget = { library: number } | { new_library: string }
  export interface Moved {
    library_id: number
    library: string
    works_kept: number
    works_joined: number
    works_split: number
    series_joined: number
    positions: number
    notes: number
    alignments_dropped: number
    library_removed: string | null
    grants_removed: number
  }
  export interface MoveAnswer { moved: Moved; derive: Derived; dry_run: boolean }
  export const moveCost: (rootId: number, to: MoveTarget) => Promise<MoveAnswer>
  export const moveFolder: (rootId: number, to: MoveTarget) => Promise<MoveAnswer>

  // message.ts
  /** The `volumes` a refusal lists, or none. */
  export function volumesOf(err: unknown): string[]

  // MoveRoot.tsx
  export interface NamedLibrary { id: number; name: string }
  export interface MoveRootProps {
    root: Root
    /** The library the folder is in. */
    from: NamedLibrary
    /** Every other library. */
    targets: NamedLibrary[]
    onCancel: () => void
    onMoved: (answer: MoveAnswer) => void
  }

  // RootRow.tsx: RootRowProps gains
  //   moveTo?: { from: NamedLibrary; targets: NamedLibrary[] }
  // LibraryCard.tsx: LibraryCardProps gains
  //   others: NamedLibrary[]
  ```

- [ ] **Step 1: The client and `volumesOf`, tests first**

In `web/src/libraries.test.ts`, add two rows to the `it.each` table:

```ts
    [
      'moveCost',
      () => moveCost(3, { library: 7 }),
      'POST',
      '/api/library/roots/3/move?dry_run=true',
      { library: 7 },
    ],
    [
      'moveFolder',
      () => moveFolder(3, { new_library: 'Extras' }),
      'POST',
      '/api/library/roots/3/move',
      { new_library: 'Extras' },
    ],
```

In `web/src/message.test.ts`:

```ts
describe('volumesOf', () => {
  it('answers the volumes a refusal lists', () => {
    const thrown = new Error(
      '/api/library/roots/3/move: 409 {"error":"cannot be moved","volumes":["A v01 (raw)","B v02 (raw)"]}',
    )
    expect(volumesOf(thrown)).toEqual(['A v01 (raw)', 'B v02 (raw)'])
  })

  it('answers none where there is no list, or it is not one of strings', () => {
    for (const message of [
      '/api/library/roots/3/move: 409 {"error":"a scan is running","volumes":[]}',
      '/api/library/roots/3/move: 409 {"error":"x"}',
      '/api/library/roots/3/move: 409 {"error":"x","volumes":[1,2]}',
      '/api/library/roots/3/move: 500 plain text',
      'Failed to fetch',
    ]) {
      expect(volumesOf(new Error(message))).toEqual([])
    }
  })
})
```

Run: `pnpm -C web exec vitest run --project node --reporter=default src/libraries.test.ts src/message.test.ts`
Expected: FAIL on the new cases.

In `web/src/libraries.ts`, add the types in this task's Interfaces, each
field with a one-line comment that says what it counts in the words of
`relocate::Moved`'s own, and:

```ts
/**
 * What moving folder `rootId` to `to` would do, without doing it. A move
 * that would be refused is refused here in the same words, with the volumes
 * that stand in its way.
 */
export const moveCost = (rootId: number, to: MoveTarget): Promise<MoveAnswer> =>
  expectBody(
    sendJson<MoveAnswer>(
      `/api/library/roots/${rootId}/move?dry_run=true`,
      'POST',
      to,
    ),
  )

export const moveFolder = (rootId: number, to: MoveTarget): Promise<MoveAnswer> =>
  expectBody(
    sendJson<MoveAnswer>(`/api/library/roots/${rootId}/move`, 'POST', to),
  )
```

In `web/src/message.ts`, cut `sentenceOf`'s parse into a helper both read:

```ts
/** The JSON object a thrown `path: status {…}` carries, if it carries one. */
function bodyOf(err: unknown): Record<string, unknown> | undefined {
  const found = /^\S+: \d{3} (\{[\s\S]*\})$/.exec(messageOf(err))
  if (found === null) return undefined
  try {
    const body: unknown = JSON.parse(found[1])
    return typeof body === 'object' && body !== null
      ? (body as Record<string, unknown>)
      : undefined
  } catch {
    return undefined
  }
}

export function sentenceOf(err: unknown): string {
  const error = bodyOf(err)?.error
  return typeof error === 'string' ? error : messageOf(err)
}

/**
 * The volumes a refused move lists beside its sentence, or none. The
 * sentence names the first few; this is all of them.
 */
export function volumesOf(err: unknown): string[] {
  const volumes = bodyOf(err)?.volumes
  return Array.isArray(volumes) && volumes.every((v) => typeof v === 'string')
    ? volumes
    : []
}
```

`sentenceOf` keeps its doc comment. Run the same command. Expected: PASS,
`sentenceOf`'s own cases among them.

- [ ] **Step 2: `MoveRoot`, test first**

Create `web/src/MoveRoot.test.tsx`:

```tsx
import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type * as config from './config'
import * as api from './libraries'
import { MoveRoot } from './MoveRoot'
import { deferred, flush } from './testing/async'
import { emptyDerive } from './testing/derive'
import { press, typeInto } from './testing/input'
import { mount } from './testing/mount'

vi.mock('./libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./libraries')>()),
  moveCost: vi.fn(),
  moveFolder: vi.fn(),
}))

const root: config.Root = {
  id: 3,
  path: '/media/Manga/Chapters',
  edition_key: 'chapters',
  default_kind: null,
  default_language: null,
  exclude_globs: null,
  scan_interval_secs: null,
  keeps_documents: false,
  last_scan_at: null,
  last_scan_status: null,
}

const answer = (over: Partial<api.Moved> = {}, dry_run = true): api.MoveAnswer => ({
  moved: {
    library_id: 7,
    library: 'Manga',
    works_kept: 12,
    works_joined: 3,
    works_split: 0,
    series_joined: 2,
    positions: 5,
    notes: 1,
    alignments_dropped: 0,
    library_removed: null,
    grants_removed: 0,
    ...over,
  },
  derive: emptyDerive,
  dry_run,
})

function shown(onMoved = vi.fn(), onCancel = vi.fn()) {
  const host = mount(
    <MoveRoot
      root={root}
      from={{ id: 5, name: 'Chapters' }}
      targets={[
        { id: 7, name: 'Manga' },
        { id: 8, name: 'Comics' },
      ]}
      onCancel={onCancel}
      onMoved={onMoved}
    />,
  )
  return { host, onMoved, onCancel }
}

const target = (host: HTMLElement) =>
  host.querySelector('select[name="move-target"]') as HTMLSelectElement
const see = (host: HTMLElement) =>
  host.querySelector('button[name="rehearse"]') as HTMLButtonElement
const confirm = (host: HTMLElement) =>
  host.querySelector('button.confirm') as HTMLButtonElement | null

async function choose(host: HTMLElement, value: string) {
  await act(async () => {
    const box = target(host)
    Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set?.call(
      box,
      value,
    )
    box.dispatchEvent(new Event('change', { bubbles: true }))
  })
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(api.moveCost).mockResolvedValue(answer())
})

describe('MoveRoot', () => {
  it('offers every other library and a new one, and moves nothing unasked', () => {
    const { host } = shown()
    expect(
      host.querySelector('[role="group"]')?.getAttribute('aria-label'),
    ).toBe('Move the folder at /media/Manga/Chapters')
    expect([...target(host).options].map((o) => o.textContent)).toEqual([
      'Choose a library…',
      'Manga',
      'Comics',
      'A new library…',
    ])
    expect(see(host).disabled).toBe(true)
    expect(confirm(host)).toBeNull()
    expect(api.moveCost).not.toHaveBeenCalled()
  })

  it('says what the move would do before it offers to make it', async () => {
    const { host } = shown()
    await choose(host, '7')
    expect(confirm(host)).toBeNull()
    await press(see(host))

    expect(api.moveCost).toHaveBeenCalledWith(3, { library: 7 })
    const said = host.querySelector('.move-counts')?.textContent ?? ''
    expect(said).toContain('12 works keep their address')
    expect(said).toContain('3 works join a work Manga already holds, as another edition')
    expect(said).toContain('0 works are split between the two libraries')
    expect(said).toContain('5 in-progress positions and 1 note go with them')
    expect(said).toContain('2 series Manga already holds gain volumes')
    expect(said).not.toContain('alignment')
    expect(said).not.toContain('is removed')
    expect(confirm(host)?.textContent).toBe('Move /media/Manga/Chapters to Manga')
  })

  it('says what is dropped and what goes with the folder’s last library', async () => {
    vi.mocked(api.moveCost).mockResolvedValue(
      answer({
        works_kept: 1,
        works_joined: 0,
        works_split: 1,
        series_joined: 0,
        positions: 1,
        notes: 0,
        alignments_dropped: 1,
        library_removed: 'Chapters',
        grants_removed: 2,
      }),
    )
    const { host } = shown()
    await choose(host, '7')
    await press(see(host))
    const said = host.querySelector('.move-counts')?.textContent ?? ''
    expect(said).toContain('1 work keeps its address')
    expect(said).toContain('1 work is split between the two libraries')
    expect(said).toContain('1 in-progress position and 0 notes go with them')
    expect(said).toContain('1 alignment between editions the move separates is dropped')
    expect(said).toContain(
      'Chapters has no other folder, so it is removed, with 2 grants',
    )
    expect(said).not.toContain('gain volumes')
  })

  it('makes the move once, for the target it rehearsed, and hands back what it did', async () => {
    const done = answer({}, false)
    const held = deferred<api.MoveAnswer>()
    vi.mocked(api.moveFolder).mockReturnValue(held.promise)
    const { host, onMoved } = shown()
    await choose(host, '7')
    await press(see(host))

    act(() => {
      confirm(host)?.click()
      confirm(host)?.click()
    })
    await flush(() => held.resolve(done))

    expect(api.moveFolder).toHaveBeenCalledTimes(1)
    expect(api.moveFolder).toHaveBeenCalledWith(3, { library: 7 })
    expect(onMoved).toHaveBeenCalledWith(done)
  })

  it('takes the rehearsal away when the target changes', async () => {
    const { host } = shown()
    await choose(host, '7')
    await press(see(host))
    expect(confirm(host)).not.toBeNull()

    await choose(host, '8')

    expect(confirm(host)).toBeNull()
    expect(host.querySelector('.move-counts')).toBeNull()
  })

  it('names a new library, and sends the name', async () => {
    vi.mocked(api.moveCost).mockResolvedValue(
      answer({ library: 'Extras', library_id: 9 }),
    )
    const { host } = shown()
    await choose(host, 'new')
    expect(see(host).disabled).toBe(true)
    await typeInto(
      host.querySelector('input[name="new-library"]') as HTMLInputElement,
      'Extras',
    )
    await press(see(host))
    expect(api.moveCost).toHaveBeenCalledWith(3, { new_library: 'Extras' })
    expect(confirm(host)?.textContent).toBe('Move /media/Manga/Chapters to Extras')
  })

  it("draws a refusal's sentence and its volumes, and offers no confirm", async () => {
    vi.mocked(api.moveCost).mockRejectedValue(
      new Error(
        '/api/library/roots/3/move?dry_run=true: 409 {"error":"`Manga` already holds volumes of /media/Manga/Chapters under the same edition key: A v01 (raw) and 1 more. Nothing was changed","volumes":["A v01 (raw)","B v02 (raw)"]}',
      ),
    )
    const { host } = shown()
    await choose(host, '7')
    await press(see(host))
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'under the same edition key',
    )
    expect(
      [...host.querySelectorAll('.move-volumes li')].map((li) => li.textContent),
    ).toEqual(['A v01 (raw)', 'B v02 (raw)'])
    expect(confirm(host)).toBeNull()
  })

  it('draws the rehearsal asked for last when two answers cross', async () => {
    const slow = deferred<api.MoveAnswer>()
    vi.mocked(api.moveCost).mockReturnValueOnce(slow.promise)
    const { host } = shown()
    await choose(host, '7')
    await press(see(host))
    await choose(host, '8')
    vi.mocked(api.moveCost).mockResolvedValue(answer({ library: 'Comics', library_id: 8 }))
    await press(see(host))
    expect(confirm(host)?.textContent).toBe('Move /media/Manga/Chapters to Comics')

    await flush(() => slow.resolve(answer()))
    expect(confirm(host)?.textContent).toBe('Move /media/Manga/Chapters to Comics')
  })

  it('keeps the dialog and says why when the move itself fails', async () => {
    vi.mocked(api.moveFolder).mockRejectedValue(
      new Error('/api/library/roots/3/move: 409 {"error":"a scan or a rule save is running; move the folder when it has finished","volumes":[]}'),
    )
    const { host, onMoved } = shown()
    await choose(host, '7')
    await press(see(host))
    await press(confirm(host) as Element)
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('is running')
    expect(onMoved).not.toHaveBeenCalled()
    expect(host.querySelector('.move-volumes')).toBeNull()
  })
})
```

Run it. Expected: FAIL, the module does not exist.

Create `web/src/MoveRoot.tsx`:

```tsx
/**
 * Moving a folder to another library, and what that does.
 *
 * **Nothing is asked of the server until a target is chosen and the
 * rehearsal asked for**, and the confirm does not exist until the rehearsal
 * has answered. `RemoveRoot`'s rule, for a move: a rehearsal is two
 * derivations of the whole store, and a button that moves a folder under a
 * sentence that says nothing is what this component is for preventing.
 *
 * **The rehearsal is for one target.** Choosing another takes the counts and
 * the confirm away, so the button never says one library over counts taken
 * for another.
 *
 * **A refusal is the sentence and the list.** The server refuses a move
 * that cannot keep every reader's place, names the first few volumes in its
 * sentence and lists them all beside it.
 *
 * **Zero is drawn** for the three things a work can do and for what goes
 * with them. A sentence that leaves a count out reads as a count that could
 * not be taken.
 */
import { useRef, useState } from 'react'
import type { Root } from './config'
import {
  type MoveAnswer,
  type MoveTarget,
  moveCost,
  moveFolder,
} from './libraries'
import { sentenceOf, volumesOf } from './message'
import { DeriveSection } from './ScanReport'

/** A library, as far as a move needs one. */
export interface NamedLibrary {
  id: number
  name: string
}

export interface MoveRootProps {
  root: Root
  /** The library the folder is in. */
  from: NamedLibrary
  /** Every other library. */
  targets: NamedLibrary[]
  onCancel: () => void
  /** Called once the folder has moved, with what the move did. */
  onMoved: (answer: MoveAnswer) => void
}

/** `n` of a thing with its verb, in the number the count is. */
function count(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`
}

export function MoveRoot({
  root,
  from,
  targets,
  onCancel,
  onMoved,
}: MoveRootProps) {
  /** `''`, a library's id as text, or `'new'`. */
  const [choice, setChoice] = useState('')
  const [name, setName] = useState('')
  const [cost, setCost] = useState<MoveAnswer | undefined>()
  const [failure, setFailure] = useState<string | undefined>()
  const [volumes, setVolumes] = useState<string[]>([])
  const [asking, setAsking] = useState(false)
  const [moving, setMoving] = useState(false)
  /** A ref as well as `moving`: two presses before a render both read false. */
  const inFlight = useRef(false)
  /** The newest rehearsal. Answers need not arrive in order. */
  const latest = useRef(0)

  const to: MoveTarget | undefined =
    choice === ''
      ? undefined
      : choice === 'new'
        ? name.trim() === ''
          ? undefined
          : { new_library: name }
        : { library: Number(choice) }

  /** A different target is a different move: what was rehearsed is gone. */
  const forget = () => {
    latest.current += 1
    setCost(undefined)
    setFailure(undefined)
    setVolumes([])
    setAsking(false)
  }

  const refuse = (err: unknown) => {
    setFailure(sentenceOf(err))
    setVolumes(volumesOf(err))
  }

  const rehearse = () => {
    if (to === undefined) return
    const seq = ++latest.current
    setCost(undefined)
    setFailure(undefined)
    setVolumes([])
    setAsking(true)
    moveCost(root.id, to)
      .then((answer) => {
        if (seq !== latest.current) return
        setAsking(false)
        setCost(answer)
      })
      .catch((err: unknown) => {
        if (seq !== latest.current) return
        setAsking(false)
        refuse(err)
      })
  }

  const confirm = () => {
    if (to === undefined || inFlight.current) return
    inFlight.current = true
    setMoving(true)
    moveFolder(root.id, to)
      .then((answer) => onMoved(answer))
      .catch((err: unknown) => {
        inFlight.current = false
        setMoving(false)
        refuse(err)
      })
  }

  const moved = cost?.moved

  return (
    <div
      className="move-root"
      role="group"
      aria-label={`Move the folder at ${root.path}`}
    >
      <div className="fields">
        {/* O175: each `aria-label` repeats its label's words. */}
        <label className="field">
          <span>Move to</span>
          <select
            name="move-target"
            aria-label="Move to"
            value={choice}
            disabled={moving}
            onChange={(e) => {
              setChoice(e.target.value)
              forget()
            }}
          >
            <option value="">Choose a library…</option>
            {targets.map((library) => (
              <option key={library.id} value={String(library.id)}>
                {library.name}
              </option>
            ))}
            <option value="new">A new library…</option>
          </select>
        </label>
        {choice === 'new' && (
          <label className="field">
            <span>Its name</span>
            <input
              type="text"
              name="new-library"
              aria-label="Its name"
              value={name}
              disabled={moving}
              onChange={(e) => {
                setName(e.target.value)
                forget()
              }}
            />
          </label>
        )}
      </div>
      <button
        type="button"
        name="rehearse"
        onClick={rehearse}
        disabled={to === undefined || asking || moving}
      >
        {asking ? 'Working out what this would do…' : 'See what this would do'}
      </button>

      {failure !== undefined && <p role="alert">{failure}</p>}
      {volumes.length > 0 && (
        <details className="move-volumes">
          <summary>{count(volumes.length, 'volume', 'volumes')} in the way</summary>
          <ul>
            {volumes.map((volume) => (
              <li key={volume}>{volume}</li>
            ))}
          </ul>
        </details>
      )}

      {moved !== undefined && cost !== undefined && (
        <>
          <div className="move-counts">
            <p className="counts">
              Moving this folder from {from.name} to {moved.library}:{' '}
              {count(moved.works_kept, 'work keeps its', 'works keep their')}{' '}
              address,{' '}
              {count(moved.works_joined, 'work joins', 'works join')} a work{' '}
              {moved.library} already holds, as another edition, and{' '}
              {count(moved.works_split, 'work is', 'works are')} split between
              the two libraries.
            </p>
            <p>
              {count(
                moved.positions,
                'in-progress position',
                'in-progress positions',
              )}{' '}
              and {count(moved.notes, 'note', 'notes')} go with them.
            </p>
            {moved.series_joined > 0 && (
              <p>
                {count(moved.series_joined, 'series', 'series')}{' '}
                {moved.library} already holds{' '}
                {moved.series_joined === 1 ? 'gains' : 'gain'} volumes.
              </p>
            )}
            {moved.alignments_dropped > 0 && (
              <p>
                {count(
                  moved.alignments_dropped,
                  'alignment between editions the move separates is',
                  'alignments between editions the move separates are',
                )}{' '}
                dropped.
              </p>
            )}
            {moved.library_removed !== null && (
              <p>
                {moved.library_removed} has no other folder, so it is removed,
                with {count(moved.grants_removed, 'grant', 'grants')}.
              </p>
            )}
          </div>
          <button
            type="button"
            className="confirm"
            onClick={confirm}
            disabled={moving}
          >
            {moving ? 'Moving…' : `Move ${root.path} to ${moved.library}`}
          </button>
          <details className="removal-derive">
            <summary>What deriving after the move would find</summary>
            <DeriveSection derive={cost.derive} />
          </details>
        </>
      )}
      <button type="button" onClick={onCancel} disabled={moving}>
        Cancel
      </button>
    </div>
  )
}
```

The test's "2 series Manga already holds gain volumes" and "1 series …
gains volumes" follow from the code above; if the sentence reads badly on
screen in Step 5, change the words in the component and the test together.

Run it. Expected: PASS. Watch each fail, restoring between:

1. Delete `forget()` from the select's `onChange`. Expect "takes the
   rehearsal away when the target changes" to fail.
2. Delete both `if (seq !== latest.current) return` lines. Expect "draws the
   rehearsal asked for last when two answers cross" to fail.
3. Delete `if (to === undefined || inFlight.current) return`'s second half.
   Expect "makes the move once" to fail with 2 calls.
4. In `refuse`, delete `setVolumes(volumesOf(err))`. Expect "draws a
   refusal's sentence and its volumes" to fail.

- [ ] **Step 3: The row, the card and the screen**

`web/src/RootRow.tsx`: add `moveTo?: { from: NamedLibrary; targets:
NamedLibrary[] }` to its props ("Where this folder could be moved. Absent
where the row is drawn outside a library's card"), a `moving` state, a
`Move…` button in `.actions` between Scan now and Remove…, drawn only where
`moveTo !== undefined` and disabled as Remove… is and while `moving`, and:

```tsx
      {moving && moveTo !== undefined && (
        <MoveRoot
          root={root}
          from={moveTo.from}
          targets={moveTo.targets}
          onCancel={() => setMoving(false)}
          onMoved={() => {
            setMoving(false)
            onChanged()
          }}
        />
      )}
```

Scan now and Remove… are disabled while `moving`, for the reason they are
while `confirming`: a rehearsal's counts are of the store as it stood.

`web/src/LibraryCard.tsx`: `others: NamedLibrary[]` on its props ("Every
other library, for a folder's Move…"), passed to each row as
`moveTo={{ from: { id: library.id, name: library.name }, targets: others }}`.

`web/src/Libraries.tsx`: each card takes

```tsx
                others={libraries
                  .filter((other) => other.id !== library.id)
                  .map(({ id, name }) => ({ id, name }))}
```

Tests, first:

- `RootRow.test.tsx`: "offers Move… only where it was told where to" (no
  button without `moveTo`; with it, pressing opens
  `[aria-label="Move the folder at /srv/manga"]` and disables Scan now and
  Remove…), and "reloads once the folder has moved" (mock `./libraries`'
  `moveCost` and `moveFolder`, drive the dialog, assert `onChanged`).
- `LibraryCard.test.tsx`: `shown` passes `others={[{ id: 8, name: 'Comics' }]}`;
  add "offers each folder the other libraries": press a row's Move… and
  assert the select's options are `Choose a library…`, `Comics`,
  `A new library…`.
- `Libraries.test.tsx`: in "draws a card per library, in the order sent",
  press Move… on Manga's first row and assert the options name `Comics` and
  not `Manga`.

- [ ] **Step 4: Styles and the browser case**

In `web/src/styles/admin.css`: `.move-root` takes `.remove-root`'s border,
radius, padding and `margin-top`, and not its danger strip: a move that is
made loses nothing. `.move-root .confirm` and `.move-volumes li` take
`overflow-wrap: anywhere`. `.move-root .field` joins the three field
selector lists (with ` input` and ` select` in the two control lists).
`.move-root .counts` joins the rule that makes a dialog's counts
`font-weight: 600`.

In `admin-config.css.test.ts`: `move-root` joins the measure test's pattern;
`'.move-root .confirm'` and `'.move-volumes li'` join the list that breaks a
long path; and assert no rule gives `.move-root` a `box-shadow`.

In `Config.browser.test.tsx`, in the 390px block, one case: a library
holding `UNBROKEN_ROOT` and a second library whose name is sixty characters
with no space; press the row's Move…, choose the second library, press See
what this would do with `moveCost` rejecting with a refusal whose `volumes`
holds twenty entries of eighty characters with no space; open the list;
`assertNoSidewaysScroll(host)`. And the same with `moveCost` resolving, for
the confirm button.

- [ ] **Step 5: Look at it, run everything, commit**

Serve a scratch library of two folders in two libraries and take screenshots
of the dialog at 390px and 1280px, light and dark: with the rehearsal
answered, and with a refusal and its list open. Read each. The confirm
button must read as the action and Cancel as the way out.

Run the four web commands of the Global Constraints. Expected: all pass.

```bash
git add -A web/src
git commit -m "Move… on a folder row: a target, a rehearsal that says what would happen, then the move"
```

---

## Task 6: On real material, the docs, the sweep, the gate and the claim check

**Files:**
- Modify: `docs/status.md`, `docs/notes.md`, `docs/mutants-baseline.md`,
  `README.md` (one sentence, where Configuration → Libraries is described)
- Create (gitignored): `.superpowers/sdd/folder-move-evidence/*`,
  `.superpowers/sdd/folder-move-pr-body.md`

- [ ] **Step 1: Drive it over a copy of the real library**

As F2a's Task 12 Step 1 set its drive up (its log is in
`.superpowers/sdd/libraries-f2a-evidence/drive.log`): the original's digest
and a marker first, a copy of `library.db` in your scratchpad, the release
binary and the built app, a scratch share served with `--folders`, the
server's PID recorded.

Before anything else, dump what the copy holds:

```bash
sqlite3 "$S/library.db" \
  "SELECT id, slug, content_key, title, series_id, library_id FROM work ORDER BY id" \
  >| "$S/works-before.txt"
```

Make a scratch folder `Chapters` holding two small volumes copied from the
real library's folder: one under a series directory the copy already holds a
volume of, with that volume's number, and one of a series it does not hold.
Then, in a real browser, as an administrator:

1. Make a library `Chapters` from that folder, set the folder's Edition key
   to `chapters` on its row, and Scan now. Open the volume the copy also
   holds and turn two pages, so it holds a position.
2. Move… the folder to the copy's own library. Read the rehearsal: one work
   keeps its address, one joins a work the library holds. Note how long the
   rehearsal took. Confirm.
3. Open the joined volume in the reader: it has two editions, and the
   position is on the `chapters` one. The `Chapters` library is gone from the
   list.
4. Move… the folder out again, to a new library `Chapters Again`. Read the
   rehearsal: one work keeps its address and one is split. Confirm. The
   volume in the copy's library has one edition again; the one in
   `Chapters Again` holds the position.
5. Take the Edition key off the folder's row, Scan now, and Move… it back.
   Read the refusal and its list: the volume both hold is named.
6. Start a Scan now on the copy's own library and press See what this would
   do on a Move… while it runs: read the refusal.

Then:

```bash
sqlite3 "$S/library.db" \
  "SELECT id, slug, content_key, title, series_id, library_id FROM work ORDER BY id" \
  >| "$S/works-after.txt"
diff "$S/works-before.txt" "$S/works-after.txt"
```

Expected: the only lines that differ are rows the scratch folder's two
volumes made. Every work the copy held before is the same line. Then kill
the server by PID, and prove the original untouched as F2a's drive did.

Record each answer, each rehearsal's own counts and each elapsed time in
`$EVID/drive.log`, stamped with the commit. If a step shows something a
person would call wrong, stop and report before the docs are written.

- [ ] **Step 2: `docs/status.md`**

Closing a row is five edits: the checkbox; the row's text, saying what was
done and what remains, with what was filed kept and marked; the test count,
at both its sites; "N are now closed, leaving M"; and "What remains", whose
count word wraps across a line break. A row has exactly six `|` characters.

- Close **O192**. Its text says what Move does for each of the cases the row
  lists, in a sentence each, naming the test in `relocate/tests.rs` that
  holds it, and says what is still true of a `root.library_id` set by hand:
  the derive alone still does what the row describes, and nothing but Move
  writes that column.
- **O191** gains one sentence: Move divides a work whose editions are on
  both sides, so it does not make this shape; a store that migration 15
  split still holds it.
- **O190** gains what Task 3's Step 6 log shows of an open proposal whose
  two series a move put in two libraries: one sentence, with "run".
- Open a row for each defect a task's report set aside. Among those this
  plan expects: what a hand-placed series (`series_id_hand`) does across a
  move, if Task 3 found it wrong; and a series a move leaves holding no
  work, with what the next library to name that title inherits from it, if
  the drive showed it.
- Counts, as F2a's Task 12 Step 2 measured them, and the sweep:

  ```bash
  command grep -a -c '^| O[0-9]' docs/status.md
  command grep -a -c '^| O[0-9]* | \[x\]' docs/status.md
  command grep -a -n -E '[0-9]{3,} tests|Nine of the' docs/status.md
  command grep -a -n -A1 '^What remains:' docs/status.md
  ```

- `command grep -a -rn 'O192' docs web/src crates` and read every hit.

- [ ] **Step 3: `docs/notes.md`, and one README sentence**

Add "## Libraries, F2b (<date>)": what Move does, step by step as
`relocate.rs`'s doc says it; each of the six spec corrections and what was
built; each thing this plan lists as not run, with what the store
did when Task 3 ran it; the drive's answers and times; counts against
`main`; and what was not measured (the NAS; a library larger than the copy;
a move of a folder holding thousands of works that all join). Every figure
is one read off a log in the evidence directory.

In `README.md`, where F2a described Configuration → Libraries, one sentence:
a folder's row has Move…, which shows what the move would do before it is
made.

- [ ] **Step 4: The sweep, the review, the gate**

As F2a's Task 12, Steps 4 to 6, on this branch: commit, then
`scripts/mutants.sh --since main` into `$EVID`, read against its traps
(`relocate.rs`'s refusals and its re-filing are SQL in string literals, for
which it makes no mutant: Tasks 2 and 3's hand defects are the evidence);
a review of `git diff main...HEAD` whole by a reviewer who has not seen the
tasks, asked in particular for a store state in which a move loses or
misplaces a reader's row without being refused, with each claim run and not
read; fixes with tests watched failing, and the fixes reviewed; then

```bash
H=$(git rev-parse --short HEAD)
env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 \
  scripts/check-all.sh >| "$EVID/gate-${H}.log" 2>&1; echo $? >| "$EVID/gate-${H}.exit"
command grep -a -n 'all green' "$EVID/gate-${H}.log"; cat "$EVID/gate-${H}.exit"
```

and the three crates' tests on SQLite 3.40.1 in `rust:1-bookworm`, by the
recipe in F2a's plan, Task 12 Step 6.

- [ ] **Step 5: The PR body, the claim check, and stop**

Write `.superpowers/sdd/folder-move-pr-body.md`, not hard-wrapped. Title:
"Libraries, F2b: a folder moves between libraries with its works' rows and
reading history (closes O192)". Check every factual sentence in the status
rows, the notes section, the PR body and this branch's new comments against
the code and the logs, in a separate pass by a checker handed the files and
no list of what to expect, the PR body in the first pass. Fix by deleting a
figure or a reason before correcting it twice. Repeat on each fix commit
until a pass finds no false sentence.

Report to the user: the head commit, the gate's line, the sweep's figures,
what the drive showed, what each thing the plan had not run turned out to be,
and each open question. Ask before pushing. Opening the PR and merging it
are each a separate yes.

---

## Self-review

**Spec coverage (§6.2 and §12's Move list).**

| Spec | Task |
| --- | --- |
| The route, its two bodies, `?dry_run` | 4 |
| Re-file works wholly the folder's, and their series | 3 (`refile`) |
| A series of the same title in the target | 3 (`refile`, `series_joined`) |
| Split, the work with its hub, history with the edition | 1, 3 (`split`) |
| Derive, then derive again after the commit | 3 |
| The rehearsal runs the same statements, writes no document | 3 (`move_cost`) |
| The rehearsal's counts, the library that goes, its grants | 3 (`Moved`, `Census`) |
| Refused: same volume, same edition key | 2 |
| Refused: one edition on both sides | 2 |
| Refused by what the derive reports, and a work left unfiled | 3 (the verdict) |
| Refused: an account's only grant, by name | 2 |
| 409 while a scan runs | 4 |
| The screen: target, counts, Confirm, a refusal with its volumes | 5 |
| O190's case run and reported | 3 Step 6, 6 Step 2 |
| On a copy of the real library, the untouched works unchanged | 6 |
| Closes O192; O191's sentence | 6 |

**What was probed and what was not.** Probed on 2026-10-06
(`.superpowers/sdd/folder-move-evidence/plan-probe-062d09ef.log`), in a store
arranged by hand: a work filed under the target is written in place, with
its id, slug, hand title, position and note; a volume the target holds under
another key takes the moved files as an edition, and the moved work is
folded into it with its position and note; two folders of one library
holding one volume under one key make one edition; a fold meeting a position
on the arriving key reports one position lost; a divided work keeps the
edition the derive did not name, and the other library gets a work with a
fresh one, which the plan's twin query finds, for the hub staying and for
the hub moving. Not run: `relocate`'s own code, `fold::rehome_editions`, a
folder that keeps documents, a work placed in a series by hand, and a volume
held in two libraries by identical bytes under different titles.

**Names used across tasks.** `fold::{rehome_editions, Rehomed, Rehoming}`
(1 → 3); `scan::{only_grant_accounts, only_grant_of}` (2);
`relocate::{Target, MoveError}` and the private `Folder`, `folder`,
`resolve`, `refused_before`, `first_few` (2 → 3);
`relocate::{Moved, move_cost, move_root}` (3 → 4); the route and `Moved`'s
eleven JSON keys (4 → 5); `libraries.ts`'s `MoveTarget`, `Moved`,
`MoveAnswer`, `moveCost`, `moveFolder`, `message.ts`'s `volumesOf`,
`NamedLibrary`, `RootRow`'s `moveTo`, `LibraryCard`'s `others` (5).
