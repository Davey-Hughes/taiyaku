# O92 — the scanner half: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `root.exclude_globs` do what its box says — a scan skips what
the patterns name, never descends into an excluded directory, and confesses in
its report exactly what it skipped.

**Architecture:** An `Excluder`, compiled once per `scan_root` from the
column's lines by `ignore::gitignore::GitignoreBuilder`, is consulted in
`measure_dir`'s entry loop before a child is counted or descended into. Four
new `ScanReport` fields carry what was skipped and what was consequently
forgotten; `scan_report_json`'s no-`..` destructure makes every one of them a
compile error until it is handled, all the way to the screen.

**Tech Stack:** Rust 2024 (`taiyaku-library`, `taiyaku-serve`, `taiyaku-cli`),
`ignore` 0.4, rusqlite/SQLite, React 18 + TypeScript, Vitest, Biome.

The design this implements is
[`../specs/2026-09-09-o92-scanner-exclusion-design.md`](../specs/2026-09-09-o92-scanner-exclusion-design.md).
Read it first; this plan does not restate its reasoning, only its consequences.

## Global Constraints

- **The gate is `env -u TAIYAKU_CORPUS scripts/check-all.sh`.** Never run it
  with `TAIYAKU_CORPUS` set — the script exports it into an unfiltered
  `cargo test --workspace`, which runs `geometry_experiment` in the debug
  profile (40+ min; a run was once killed at 1h09m). Verify the exit by
  quoting its own `all green` line; piping through `tail` or appending
  `; echo` masks a failure.
- **`pnpm -C web run typecheck` is the typecheck, not `npx tsc --noEmit`.**
  On #119 the latter passed while the former found six files constructing a
  `config.Root`. Adding a required field to `config.ts`'s `Walk` has exactly
  that shape.
- **Every test is hand-applied against its own defect and watched to fail
  alone.** Comment out the guard the test names, run only that test, see it
  fail, restore. A test that passes against the defect it names is this
  repository's most-repeated finding — eleven on one PR, and cargo-mutants
  reported 0 survivors through the worst of them.
- **Assert per element, never a whole-page `toContain`.** A page-level string
  match cannot see a pairing: it cannot tell "3 directories excluded" beside
  the right label from the same number beside the wrong one.
- **`docs/status.md` states this workspace's dependency list as a fact about
  the project.** Adding `ignore` moves it. So does `scan/tests.rs`'s own
  fixture doc comment, which enumerates the crates.
- **The Rust test count in `docs/status.md` is gated** by
  `scripts/status-test-count.mjs`, at **both** of its sites. The web counts
  are **not** gated — measure them with `npx vitest run --project node` and
  `--project jsdom` from `web/`, never by adding to the number in the
  document.
- **`cargo test` takes ONE positional filter before `--`.** Two or more
  (`cargo test -p X scan:: derive::`) fails with "unexpected argument". Write
  `cargo test -p X -- scan:: derive::`. Two implementers hit this in the
  commands below before it was corrected.
- Commit after every task. The remote is squash-only, so local commit messages
  are never read; keep them short and keep the branch to one claim.

---

## File Structure

| file | responsibility after this plan |
|---|---|
| `crates/taiyaku-library/Cargo.toml` | declares `ignore`, with its justifying paragraph |
| `crates/taiyaku-library/src/scan.rs` | `Excluder`, `check_exclude_globs`, `Held`, `Walk`, exclusion in `measure_dir`, four `ScanReport` fields |
| `crates/taiyaku-library/src/scan/tests.rs` | the walk's behaviour under patterns |
| `crates/taiyaku-serve/src/config_api.rs` | 400 on an unusable pattern; four keys in the scan report JSON |
| `crates/taiyaku-serve/src/config_api/tests.rs` | the 400, and the four keys present |
| `crates/taiyaku-cli/src/library.rs` | `scan_summary` prints each new field, non-zero only |
| `web/src/config.ts` | four fields on `Walk` |
| `web/src/ScanReport.tsx` | four rows, among the did-not-do fields that lead |
| `web/src/ScanReport.test.tsx` | those four rows, asserted per element |
| `web/src/RootRow.tsx` | a `<textarea>` for the column; the label says what it now does |
| `web/src/RootRow.test.tsx` | the textarea, a two-line round trip, the label |
| `docs/status.md`, `docs/notes.md`, two specs | O92 closed; the measurement recorded |

---

## Task 1: The dependency and the matcher

**Files:**
- Modify: `crates/taiyaku-library/Cargo.toml`
- Modify: `crates/taiyaku-library/src/scan.rs` (new `// --- exclusion ---` section, after the `// --- roots ---` section and before `// --- the scan ---`)
- Test: `crates/taiyaku-library/src/scan/tests.rs`

**Interfaces:**
- Produces: `struct Excluder` (private) with `Excluder::build(patterns: Option<&str>) -> Excluder`, `.excludes(rel_path: &str, is_dir: bool) -> bool`, `.excludes_with_parents(rel_path: &str, is_dir: bool) -> bool`, `.complaints: Vec<String>`; and `pub fn check_exclude_globs(patterns: &str) -> Result<(), String>`.
- Consumes: nothing.

- [ ] **Step 1: Add the dependency**

In `crates/taiyaku-library/Cargo.toml`, in the `[dependencies]` table, keeping
its alphabetical-ish grouping and its habit of a paragraph per non-obvious
crate — insert after the `argon2`/`base64` pair and before `notify`:

```toml
# O92's scanner half: `root.exclude_globs` applied to the walk. The matcher
# behind ripgrep, taken whole rather than reimplemented — it reads a line
# exactly as a `.gitignore` file holds it, so negation, anchoring,
# directory-only patterns, comments and blank lines all arrive with it and
# none of them is written here. `rules.rs` makes the same argument against
# itself for `taiyaku_read::volume_number_marked`: a second, drifting
# reimplementation is worse than a dependency.
#
# **Three crates, not the ten `cargo add` suggests** — `ignore`, `globset` and
# `bstr`. `notify` already supplies `walkdir`, `same-file`, the `crossbeam-*`
# trio and `winapi-util`; measured against this workspace's lock on
# 2026-09-09. `globset` alone would have cost two of the same three and could
# not express negation.
#
# **User-supplied patterns are safe here for the reason `regex` below is
# safe**, and it is worth restating because it is not obvious it transfers:
# `globset` compiles to `regex-automata`, which does not backtrack, so a
# pattern typed into C4's box cannot produce catastrophic matching behaviour.
ignore = "0.4.33"
```

- [ ] **Step 2: Write the failing tests**

Append to `crates/taiyaku-library/src/scan/tests.rs`:

```rust
/// The pattern language is gitignore's, taken from the crate rather than
/// reimplemented — so this test pins the five behaviours the design chose it
/// *for*, not the crate's whole surface.
///
/// Verified against `ignore` 0.4.33 on 2026-09-09 before it was written here;
/// each row is a behaviour a hand-rolled matcher would have had to be given
/// on purpose, and four of the five are ones `globset` alone cannot express.
#[test]
fn the_pattern_language_is_gitignores() {
    let excluder = super::Excluder::build(Some(
        "**/extras/**\nextras\n*.nfo\n!keep.nfo\n# a comment\n\n/toplevel\nbuild/",
    ));

    for (path, is_dir, expected, why) in [
        ("Naruto/extras", true, true, "a bare name matches at any depth"),
        ("Naruto/extras/art.jpg", false, true, "and so does what is under it"),
        ("a/b/c.nfo", false, true, "a file pattern matches at any depth"),
        ("a/keep.nfo", false, false, "negation re-includes"),
        ("toplevel", true, true, "a leading slash anchors, and this is the root's own"),
        ("deep/toplevel", true, false, "a leading slash anchors, and this is not"),
        ("build", true, true, "a trailing slash means the directory"),
        ("build", false, false, "and not a file of that name"),
        ("Naruto/v01.cbz", false, false, "an ordinary member is untouched"),
    ] {
        assert_eq!(
            excluder.excludes(path, is_dir),
            expected,
            "{path} (dir={is_dir}): {why}"
        );
    }
}

/// A column holding nothing excludes nothing — the path every scan in this
/// repository takes, and the one a matcher that defaulted to matching
/// everything would destroy silently.
#[test]
fn no_pattern_excludes_nothing() {
    for patterns in [None, Some(""), Some("\n\n"), Some("# only a comment")] {
        let excluder = super::Excluder::build(patterns);
        assert!(
            !excluder.excludes("anything/at/all.cbz", false),
            "{patterns:?} must exclude nothing"
        );
        assert!(
            excluder.complaints.is_empty(),
            "{patterns:?} must complain about nothing"
        );
    }
}

/// A line that will not compile disables itself and leaves every other line
/// standing — `rules.rs`'s rule, for its reason: a scan of a real library
/// must not die over one bad pattern.
///
/// **`\` and `a{b` are the two shapes `ignore` 0.4.33 actually rejects**,
/// measured on 2026-09-09. `a[`, `[a-`, `[[:bogus:]]` and `***` all compile,
/// so this test names the two that exist rather than a category that does
/// not.
#[test]
fn a_bad_line_disables_itself_and_nothing_else() {
    let excluder = super::Excluder::build(Some("extras\n\\\n*.nfo"));

    assert_eq!(excluder.complaints.len(), 1, "{:?}", excluder.complaints);
    assert!(
        excluder.complaints[0].contains('\\'),
        "the complaint names the line: {:?}",
        excluder.complaints
    );
    assert!(excluder.excludes("a/extras", true), "the line before it applies");
    assert!(excluder.excludes("a/b.nfo", false), "the line after it applies");
}

/// `matched_path_or_any_parents`, which is what `forget_unseen` needs: a
/// stale row under a directory the walk never descended into.
#[test]
fn a_child_of_an_excluded_directory_is_excluded_with_parents() {
    let excluder = super::Excluder::build(Some("extras"));

    assert!(
        !excluder.excludes("Naruto/extras/art.jpg", false),
        "the path itself does not match the pattern"
    );
    assert!(
        excluder.excludes_with_parents("Naruto/extras/art.jpg", false),
        "but its parent does, which is the question forget_unseen asks"
    );
}

/// `check_exclude_globs` is what the PATCH route refuses on, so its two
/// answers are pinned in both directions.
#[test]
fn check_exclude_globs_names_the_line_it_refuses() {
    assert!(super::check_exclude_globs("extras\n*.nfo\n!keep.nfo").is_ok());
    assert!(super::check_exclude_globs("").is_ok());

    let why = super::check_exclude_globs("extras\na{b").expect_err("a{b is unclosed");
    assert!(why.contains("a{b"), "the message names the line: {why}");
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run:
```sh
cargo test -p taiyaku-library scan::tests::the_pattern_language_is_gitignores
```
Expected: FAIL to compile — `cannot find type Excluder in module super`.

- [ ] **Step 4: Write the implementation**

In `crates/taiyaku-library/src/scan.rs`, add a new section between the
`// --- roots ---` section (which ends with `pub fn remove_root`) and
`// --- the scan ---`:

```rust
// --- exclusion -------------------------------------------------------------

/// The patterns one root excludes from its own walk, compiled once per scan.
///
/// **gitignore's language, from `ignore`, not a glob matcher of this
/// module's own.** `root.exclude_globs` holds one pattern per line, and a
/// line means exactly what the same line in a `.gitignore` file means:
/// negation with `!`, anchoring with a leading `/`, a directory only with a
/// trailing `/`, `**` across separators, `#` comments and blank lines
/// ignored. None of that is written here, which is the point — `rules.rs`
/// refuses "a second, drifting reimplementation" for the same reason.
///
/// **Building never fails.** A line that will not compile is dropped and
/// named in [`Self::complaints`], which [`ScanReport::bad_exclude_patterns`]
/// carries out to whoever asked for the scan. That is `rules.rs`'s rule
/// again: a malformed pattern disables its own rule and nothing else,
/// because a scan of a real library must not die over one bad line. The
/// column predates any validation — migration 1 — so a row can already hold
/// anything, and treating a build failure as impossible would make it a
/// crash rather than a complaint.
struct Excluder {
    gitignore: ignore::gitignore::Gitignore,
    /// One message per line that would not compile, naming the line.
    complaints: Vec<String>,
}

impl Excluder {
    /// Compile `patterns`, one per line. `None` and `""` both mean "exclude
    /// nothing", which is the path every scan in this repository took before
    /// this existed.
    fn build(patterns: Option<&str>) -> Self {
        let mut builder = ignore::gitignore::GitignoreBuilder::new("");
        let mut complaints = Vec::new();
        for line in patterns.unwrap_or_default().lines() {
            if let Err(why) = builder.add_line(None, line) {
                complaints.push(format!("`{}`: {why}", line.trim()));
            }
        }
        // `build` cannot fail once `add_line` has accepted every line it
        // kept — measured against `ignore` 0.4.33 on 2026-09-09, a builder
        // holding only a rejected line still builds, empty. Handled rather
        // than unwrapped anyway: this module's rule is that a pattern
        // problem is a complaint, never a panic.
        let gitignore = builder.build().unwrap_or_else(|why| {
            complaints.push(format!("compiling this root's exclude patterns: {why}"));
            ignore::gitignore::Gitignore::empty()
        });
        Self { gitignore, complaints }
    }

    /// Whether `rel_path` itself is excluded. The walk's question, asked
    /// about a child before it is counted or descended into.
    fn excludes(&self, rel_path: &str, is_dir: bool) -> bool {
        matches!(
            self.gitignore.matched(rel_path, is_dir),
            ignore::Match::Ignore(_)
        )
    }

    /// Whether `rel_path` or any directory above it is excluded —
    /// [`forget_unseen`]'s question, since a row under a directory the walk
    /// never descended into is stale for a reason the row's own path does
    /// not state.
    ///
    /// **Fractionally more permissive than git**, and it is worth naming:
    /// this tests the path itself before its parents, so a whitelisted child
    /// of an excluded parent (`!a/extras/keep` under an excluded `extras`)
    /// answers false here while the walk, correctly, never reaches it. It
    /// decides only which of two counters a number lands in, never whether a
    /// row is deleted.
    fn excludes_with_parents(&self, rel_path: &str, is_dir: bool) -> bool {
        matches!(
            self.gitignore.matched_path_or_any_parents(rel_path, is_dir),
            ignore::Match::Ignore(_)
        )
    }
}

/// Whether every line of `patterns` is a pattern this library can compile.
///
/// **`taiyaku-serve`'s `PATCH /api/library/roots/{id}` refuses on this**, so
/// an admin hears about a typo at save rather than after a scan. A pattern
/// already stored that will not build is a different matter and is not this
/// function's business — see [`Excluder::build`], which drops it and
/// complains.
///
/// **What this actually catches is narrow, and callers must not imply
/// otherwise.** Measured against `ignore` 0.4.33 on 2026-09-09, `add_line`
/// rejects a dangling `\` and an unclosed `{`. It accepts `a[`, `[a-`,
/// `[[:bogus:]]`, `[]`, `***` and `a**b`. This is a guard against two
/// specific typos, not a check that a pattern means what its author meant.
///
/// # Errors
///
/// Returns a message naming the offending line, ready to be handed to a
/// person, if any line will not compile.
pub fn check_exclude_globs(patterns: &str) -> Result<(), String> {
    let mut builder = ignore::gitignore::GitignoreBuilder::new("");
    for line in patterns.lines() {
        builder.add_line(None, line).map_err(|why| {
            format!(
                "the root's `exclude_globs` line `{}` is not a usable pattern: {why}",
                line.trim()
            )
        })?;
    }
    builder
        .build()
        .map(|_| ())
        .map_err(|why| format!("the root's `exclude_globs` will not compile: {why}"))
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run:
```sh
cargo test -p taiyaku-library -- \
  scan::tests::the_pattern_language_is_gitignores \
  scan::tests::no_pattern_excludes_nothing \
  scan::tests::a_bad_line_disables_itself_and_nothing_else \
  scan::tests::a_child_of_an_excluded_directory_is_excluded_with_parents \
  scan::tests::check_exclude_globs_names_the_line_it_refuses
```
Expected: `test result: ok. 5 passed`.

- [ ] **Step 6: Hand-apply each test's defect and watch it fail**

One at a time, restoring between each:

| change | test that must fail |
|---|---|
| `excludes` matches `Match::Whitelist(_)` too | `the_pattern_language_is_gitignores` (`a/keep.nfo`) |
| `Excluder::build` returns early on the first `Err`, keeping earlier lines | `a_bad_line_disables_itself_and_nothing_else` (`*.nfo` no longer applies) |
| `excludes_with_parents` delegates to `matched` | `a_child_of_an_excluded_directory_is_excluded_with_parents` |
| `check_exclude_globs` returns `Ok(())` unconditionally | `check_exclude_globs_names_the_line_it_refuses` |
| `build` treats `None` as `Some("*")` | `no_pattern_excludes_nothing` |

Any that still passes means the test is not testing what it names. Fix the
test, not the note.

- [ ] **Step 7: Commit**

```sh
git add crates/taiyaku-library/Cargo.toml Cargo.lock \
        crates/taiyaku-library/src/scan.rs crates/taiyaku-library/src/scan/tests.rs
git commit -m "O92: the exclude matcher"
```

---

## Task 2: `held_for` reads `kind`

A pure refactor with no behaviour change, taken on its own so that a green
suite is the whole proof. Task 4 needs `is_dir` for a held row and this is
where it arrives.

**Files:**
- Modify: `crates/taiyaku-library/src/scan.rs` — `held_for`, `unchanged`, `record`, `forget_unseen`, `scan_root_inner`

**Interfaces:**
- Produces: `struct Held { size: i64, mtime: i64, is_dir: bool }` (private, `Clone + Copy`); `held_for(&Connection, i64) -> Result<HashMap<String, Held>, String>`.
- Consumes: nothing from Task 1.

- [ ] **Step 1: Add the type and change the query**

Replace `held_for` (currently at `scan.rs:1161`) and add the struct directly
above it:

```rust
/// One row `scan_entry` already holds for this root, as much of it as the
/// walk needs: `(size, mtime)` to decide whether anything must be reopened,
/// and `is_dir` so [`forget_unseen`] can ask [`Excluder`] about a stale path
/// without a second query.
///
/// A named type rather than the `(i64, i64)` tuple this was — the shape
/// [`Seen`], [`Resolved`] and [`Measured`] already use in this module.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Held {
    size: i64,
    mtime: i64,
    is_dir: bool,
}

fn held_for(conn: &Connection, root_id: i64) -> Result<HashMap<String, Held>, String> {
    let mut stmt = conn
        .prepare("SELECT rel_path, size, mtime, kind FROM scan_entry WHERE root_id = ?1")
        .map_err(|e| format!("reading what this root held: {e}"))?;
    let rows = stmt
        .query_map([root_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                Held {
                    size: r.get(1)?,
                    mtime: r.get(2)?,
                    is_dir: r.get::<_, String>(3)? == "dir",
                },
            ))
        })
        .map_err(|e| format!("reading what this root held: {e}"))?;
    Ok(rows.filter_map(Result::ok).collect())
}
```

Keep `held_for`'s existing doc comment above the new `Held`'s — it belongs to
the function, so leave it attached to `fn held_for`.

- [ ] **Step 2: Change the three consumers**

`unchanged` (`scan.rs:955`):

```rust
fn unchanged(held: Option<Held>, seen: &Seen) -> bool {
    let size = i64::try_from(seen.size).unwrap_or(i64::MAX);
    held.is_some_and(|held| held.size == size && held.mtime == seen.mtime)
}
```

`record`'s parameter (`scan.rs:982`): `held: Option<(i64, i64)>` becomes
`held: Option<Held>`. Its body branches only on `held.is_some()`, so nothing
inside it changes.

`forget_unseen`'s parameter (`scan.rs:1183`): `held: &HashMap<String, (i64, i64)>`
becomes `held: &HashMap<String, Held>`. Its body reads only `held.keys()`, so
nothing inside it changes.

`scan_root_inner`'s two `held.get(...).copied()` calls (`scan.rs:888` and
`:906`) are unchanged — `Held` is `Copy`.

- [ ] **Step 3: Run the whole scan and derive suites**

Run:
```sh
cargo test -p taiyaku-library -- scan:: derive::
```
Expected: PASS, with the same counts as before this task. This is a refactor;
a single moved assertion means it was not one.

- [ ] **Step 4: Prove the new column is actually read**

Change `is_dir: r.get::<_, String>(3)? == "dir"` to `is_dir: false` and re-run
the two suites. Expected: still green — **and that is the point**. Nothing
consumes `is_dir` until Task 4, so there is no test to protect it yet. Restore
the line and note that Task 4's test is its first witness. Do not leave the
mutation in.

- [ ] **Step 5: Commit**

```sh
git add crates/taiyaku-library/src/scan.rs
git commit -m "O92: held_for carries kind"
```

---

## Task 3: The walk skips what the patterns name

**Files:**
- Modify: `crates/taiyaku-library/src/scan.rs` — `ScanReport`, `ScanReport::for_root`, `scan_root_inner`, `measure`, `measure_dir`, new `Walk`
- Modify: `crates/taiyaku-serve/src/config_api.rs` — `scan_report_json`
- Modify: `crates/taiyaku-cli/src/library.rs` — `scan_summary`
- Modify: `web/src/config.ts` — `Walk`
- Test: `crates/taiyaku-library/src/scan/tests.rs`

**Interfaces:**
- Consumes: `Excluder::build`, `.excludes`, `.complaints` from Task 1.
- Produces: `ScanReport.excluded_dirs: Vec<String>`, `.excluded_files: usize`, `.bad_exclude_patterns: Vec<String>`; `struct Walk { unreadable: Vec<String>, excluded_dirs: Vec<String>, excluded_files: usize }`; `measure(&Path, u32, &Excluder) -> Result<(Measured, Walk), String>`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-library/src/scan/tests.rs`:

```rust
/// A root whose fixture holds a directory worth excluding and a file worth
/// excluding, beside material that must survive both.
///
/// Built on [`fixture`] rather than beside it, so what "survives" means is
/// the same seven entries every other test in this module already pins.
fn fixture_with_junk() -> crate::tests::TempDir {
    let dir = fixture();
    write_pages(&dir.path.join("Go-Toubun no Hanayome/extras"), 2);
    std::fs::write(dir.path.join("Go-Toubun no Hanayome/series.nfo"), b"junk")
        .expect("write");
    dir
}

/// A root with `exclude_globs` set, ready to scan.
fn root_excluding(conn: &Connection, path: &Path, patterns: &str) -> Root {
    Root {
        exclude_globs: Some(patterns.to_string()),
        ..add_root(conn, path).expect("add")
    }
}

/// An excluded directory is not descended into, and the report names it.
///
/// **Both halves, because either alone passes against a defect.** A test
/// asserting only the report passes against an implementation that names the
/// directory and walks it anyway; a test asserting only the rows passes
/// against one that skips it silently, which is the confession C4 §3 exists
/// to require.
#[test]
fn an_excluded_directory_is_not_walked_and_is_named() {
    let tree = fixture_with_junk();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = root_excluding(&conn, &tree.path, "extras");

    let report = scan_root(&conn, &root).expect("scan");

    assert_eq!(
        report.excluded_dirs,
        vec!["Go-Toubun no Hanayome/extras".to_string()],
        "the directory the pattern named, and only it"
    );
    let under: i64 = conn
        .query_row(
            "SELECT count(*) FROM scan_entry WHERE rel_path LIKE '%extras%'",
            [],
            |r| r.get(0),
        )
        .expect("counting");
    assert_eq!(under, 0, "nothing under it reached scan_entry");
    assert!(
        report.dirs >= 6,
        "the rest of the tree was still walked: {} dirs",
        report.dirs
    );
}

/// An excluded file is skipped and counted, and the count is not a list.
#[test]
fn an_excluded_file_is_skipped_and_counted() {
    let tree = fixture_with_junk();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = root_excluding(&conn, &tree.path, "*.nfo");

    let report = scan_root(&conn, &root).expect("scan");

    assert_eq!(report.excluded_files, 1);
    assert!(report.excluded_dirs.is_empty(), "a file pattern names no directory");
    let held: i64 = conn
        .query_row(
            "SELECT count(*) FROM scan_entry WHERE rel_path LIKE '%.nfo'",
            [],
            |r| r.get(0),
        )
        .expect("counting");
    assert_eq!(held, 0, "it never became an entry");
}

/// A negation re-includes what its own pattern excluded, through the walk
/// and not merely through the matcher.
#[test]
fn a_negation_re_includes_through_the_walk() {
    let tree = fixture_with_junk();
    std::fs::write(tree.path.join("Go-Toubun no Hanayome/keep.nfo"), b"keep")
        .expect("write");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = root_excluding(&conn, &tree.path, "*.nfo\n!keep.nfo");

    let report = scan_root(&conn, &root).expect("scan");

    assert_eq!(report.excluded_files, 1, "series.nfo, and not keep.nfo");
    let kept: i64 = conn
        .query_row(
            "SELECT count(*) FROM scan_entry WHERE rel_path LIKE '%keep.nfo'",
            [],
            |r| r.get(0),
        )
        .expect("counting");
    assert_eq!(kept, 1, "the negated line put it back");
}

/// An excluded entry does not move `dir_state.entry_count`.
///
/// The exclusion happens before the counter, so a junk file arriving in a
/// directory is not a change this library reports. Asserted by scanning the
/// same tree twice with the file added in between.
#[test]
fn an_excluded_file_does_not_move_the_entry_count() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = root_excluding(&conn, &tree.path, "*.nfo");
    scan_root(&conn, &root).expect("first");

    let before: i64 = conn
        .query_row(
            "SELECT entry_count FROM dir_state WHERE rel_path = 'Go-Toubun no Hanayome'",
            [],
            |r| r.get(0),
        )
        .expect("reading dir_state");

    std::fs::write(tree.path.join("Go-Toubun no Hanayome/late.nfo"), b"junk")
        .expect("write");
    scan_root(&conn, &root).expect("second");

    let after: i64 = conn
        .query_row(
            "SELECT entry_count FROM dir_state WHERE rel_path = 'Go-Toubun no Hanayome'",
            [],
            |r| r.get(0),
        )
        .expect("reading dir_state");
    assert_eq!(before, after, "an excluded arrival is not a change");
}

/// A bad line reaches the report and the good lines still apply.
#[test]
fn a_bad_pattern_is_reported_by_the_scan() {
    let tree = fixture_with_junk();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = root_excluding(&conn, &tree.path, "extras\n\\");

    let report = scan_root(&conn, &root).expect("scan");

    assert_eq!(report.bad_exclude_patterns.len(), 1, "{report:?}");
    assert_eq!(
        report.excluded_dirs,
        vec!["Go-Toubun no Hanayome/extras".to_string()],
        "the line before the bad one still applied"
    );
}

/// A root with no patterns reports three empty fields rather than omitting
/// them — C4 §3.4's rule, at the layer that produces the numbers.
#[test]
fn a_root_with_no_patterns_reports_the_fields_empty() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = add_root(&conn, &tree.path).expect("add");

    let report = scan_root(&conn, &root).expect("scan");

    assert!(report.excluded_dirs.is_empty());
    assert_eq!(report.excluded_files, 0);
    assert!(report.bad_exclude_patterns.is_empty());
}
```

- [ ] **Step 2: Run them to verify they fail**

Run:
```sh
cargo test -p taiyaku-library scan::tests::an_excluded_directory_is_not_walked_and_is_named
```
Expected: FAIL to compile — `no field excluded_dirs on type ScanReport`.

- [ ] **Step 3: Add the three report fields**

In `ScanReport` (`scan.rs:691`), after `unreadable_dirs`:

```rust
    /// Directories a pattern in `root.exclude_globs` named, which this walk
    /// therefore never descended into — no `read_dir`, no `stat`, no
    /// `sniff`, no `member_pages` for anything beneath them.
    ///
    /// **C4 §3's fourth confession, finally a field.** That section requires
    /// a scan to report "directories skipped by `exclude_globs`" alongside
    /// the containers it could not open and the formats it cannot read; the
    /// build design asserted every item of that discipline "is already a
    /// field", which was true of the other three and false of this one until
    /// now (O92).
    pub excluded_dirs: Vec<String>,
    /// Files a pattern named, counted rather than listed.
    ///
    /// **A count where [`Self::excluded_dirs`] is a list, deliberately.** A
    /// library excludes a handful of subtrees and may exclude thousands of
    /// `*.nfo` files; C4 §3.4 requires this report to render whole, and a
    /// list of thousands cannot. Page images are included: a pattern that
    /// excludes one reduces its directory's page total, which is what
    /// excluding it means.
    pub excluded_files: usize,
    /// Lines of `root.exclude_globs` that would not compile, each naming
    /// itself.
    ///
    /// A malformed pattern disables its own line and nothing else —
    /// `rules.rs`'s rule, because a scan of a real library must not die over
    /// one bad line, and migration 1's column predates any validation.
    pub bad_exclude_patterns: Vec<String>,
```

In `ScanReport::for_root` (`scan.rs:715`), after `unreadable_dirs: Vec::new(),`:

```rust
            excluded_dirs: Vec::new(),
            excluded_files: 0,
            bad_exclude_patterns: Vec::new(),
```

- [ ] **Step 4: Add the `Walk` accumulator and thread the excluder**

Directly above `fn measure` (`scan.rs:1292`):

```rust
/// What one walk accumulated besides the tree itself.
///
/// One accumulator rather than three out-params: [`measure_dir`] already
/// threaded `unreadable: &mut Vec<String>`, and exclusion adds two more
/// collections to a signature that was at its limit.
#[derive(Default)]
struct Walk {
    unreadable: Vec<String>,
    excluded_dirs: Vec<String>,
    excluded_files: usize,
}
```

`measure`:

```rust
fn measure(
    path: &Path,
    depth: u32,
    excluder: &Excluder,
) -> Result<(Measured, Walk), String> {
    let mut walk = Walk::default();
    let measured = measure_dir(path, String::new(), depth, excluder, &mut walk)
        .map_err(MeasureError::into_message)?;
    Ok((measured, walk))
}
```

`measure_dir`'s signature — replace `unreadable: &mut Vec<String>` with the
two new parameters:

```rust
fn measure_dir(
    path: &Path,
    rel_path: String,
    depth: u32,
    excluder: &Excluder,
    walk: &mut Walk,
) -> Result<Measured, MeasureError> {
```

Inside it, every `unreadable.push(message)` becomes `walk.unreadable.push(message)`.

The entry loop becomes:

```rust
    for entry in entries.flatten() {
        let child_path = entry.path();
        let name = child_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if name.starts_with('.') {
            continue;
        }
        let child_rel = join_rel(&rel_path, &name);
        let is_dir = child_path.is_dir();
        // **Before `entry_count`, and before the page/file split.**
        // `dir_state.entry_count` exists to say whether a directory changed;
        // an excluded file arriving or leaving is not a change this library
        // has any interest in, and counting it would make one show up. The
        // cost is that `entry_count` no longer equals what `read_dir`
        // returned — consistent, since it is only ever compared against a
        // number computed the same way, and a pattern edit *should* read as
        // a change.
        //
        // A directory is skipped here rather than filtered later, which is
        // the whole saving: nothing beneath it is listed, stat'ed, sniffed
        // or opened. It also gives git's own rule for free — a file whose
        // parent directory is excluded cannot be re-included, because the
        // walk never reaches it.
        if excluder.excludes(&child_rel, is_dir) {
            if is_dir {
                walk.excluded_dirs.push(child_rel);
            } else {
                walk.excluded_files += 1;
            }
            continue;
        }
        entry_count += 1;
        if is_dir {
            dir_entries.push((child_rel, child_path));
        } else if taiyaku_read::is_page_name(&name) {
            here += 1;
        } else {
            file_entries.push((child_rel, child_path));
        }
    }
```

Both `sort()` calls are unchanged: within one directory every `child_rel`
shares the same prefix, so sorting by it is sorting by name.

The two loops below now carry a rel path rather than a name:

```rust
    for (child_rel, child_path) in dir_entries {
        match measure_dir(&child_path, child_rel, depth + 1, excluder, walk) {
```

and

```rust
    for (child_rel, child_path) in file_entries {
        if let Some(file) = measure_file(&child_path, child_rel) {
            children.push(file);
        }
    }
```

- [ ] **Step 5: Wire it into `scan_root_inner`**

At the top of `scan_root_inner` (`scan.rs:838`):

```rust
    let excluder = Excluder::build(root.exclude_globs.as_deref());
    let (measured, walk) = measure(&root.path, 0, &excluder)?;
    let seen = flatten(&measured);
```

In the §8.1 refusal's `ScanReport` literal, replace `unreadable_dirs,` with:

```rust
                unreadable_dirs: walk.unreadable,
                excluded_dirs: walk.excluded_dirs,
                excluded_files: walk.excluded_files,
                bad_exclude_patterns: excluder.complaints,
```

and below, where the report is filled in:

```rust
    report.unreadable_dirs = walk.unreadable;
    report.excluded_dirs = walk.excluded_dirs;
    report.excluded_files = walk.excluded_files;
    report.bad_exclude_patterns = excluder.complaints;
```

The refusal path returns early, so both moves of `walk`/`excluder` are fine;
if the borrow checker disagrees, clone `excluder.complaints` on the refusal
path rather than reordering the function.

- [ ] **Step 5b: Remove the `expect(dead_code)` Task 1 left on what you just called**

Task 1 shipped `Excluder` with `#[cfg_attr(not(test), expect(dead_code, …))]`
on the struct and on all three methods, because nothing outside `cfg(test)`
called them yet and the clippy gate is `-D warnings`.

`#[expect]` — not `#[allow]` — was chosen precisely so this cannot be
forgotten: once an item is used, the expectation goes **unfulfilled** and
`-D warnings` turns it into a build error. So remove the attribute from the
three items this task makes reachable:

- the `Excluder` struct
- `Excluder::build`
- `Excluder::excludes`

**Leave the one on `Excluder::excludes_with_parents`.** Nothing calls it until
Task 4, and removing it early fails the gate the other way — as an ordinary
dead-code warning. Task 4 removes that last one.

Run `cargo clippy -p taiyaku-library --all-targets -- -D warnings` and confirm
it is clean before committing. A leftover `expect` and a missing one both fail
it, in opposite directions.

- [ ] **Step 6: Make the three downstream consumers compile**

`crates/taiyaku-serve/src/config_api.rs`, `scan_report_json` — add to both the
destructure and the object, keeping the did-not-do fields together at the top
of the JSON per C4 §3.4:

```rust
    let scan::ScanReport {
        root,
        dirs,
        files,
        added,
        updated,
        gone,
        refused,
        unreadable_dirs,
        excluded_dirs,
        excluded_files,
        bad_exclude_patterns,
    } = report;
    serde_json::json!({
        "root": root.to_string_lossy(),
        "dirs": dirs,
        "files": files,
        "added": added,
        "updated": updated,
        "gone": gone,
        "refused": refused,
        "unreadable_dirs": unreadable_dirs,
        "excluded_dirs": excluded_dirs,
        "excluded_files": excluded_files,
        "bad_exclude_patterns": bad_exclude_patterns,
    })
```

`crates/taiyaku-cli/src/library.rs`, `scan_summary` — after the
`unreadable_dirs` block, each printing only when non-zero, that function's own
stated rule:

```rust
    if !report.excluded_dirs.is_empty() {
        let _ = write!(
            line,
            "; {} directories excluded: {}",
            report.excluded_dirs.len(),
            report.excluded_dirs.join(", ")
        );
    }
    if report.excluded_files > 0 {
        let _ = write!(line, "; {} files excluded", report.excluded_files);
    }
    if !report.bad_exclude_patterns.is_empty() {
        let _ = write!(
            line,
            "; {} unusable exclude pattern(s): {}",
            report.bad_exclude_patterns.len(),
            report.bad_exclude_patterns.join(", ")
        );
    }
```

Update that function's doc comment: the sentence naming `unreadable_dirs` as
the one field printed conditionally now names four.

`web/src/config.ts`, the `Walk` interface — after `unreadable_dirs`:

```ts
  /** Directories `root.exclude_globs` named, never descended into. */
  excluded_dirs: string[]
  /** Files a pattern named. A count, not a list — a library may exclude thousands. */
  excluded_files: number
  /** Lines of `exclude_globs` that would not compile, each naming itself. */
  bad_exclude_patterns: string[]
```

- [ ] **Step 7: Fix every `Walk` fixture the new required fields break**

Run:
```sh
pnpm -C web run typecheck
```
Expected before fixing: errors in `web/src/ScanReport.test.tsx` and
`web/src/RootRow.test.tsx`, each constructing a `Walk` without the three new
fields. Add `excluded_dirs: [], excluded_files: 0, bad_exclude_patterns: []`
to each fixture's defaults. **Do not use `npx tsc --noEmit`** — it does not
find these.

- [ ] **Step 8: Run the tests to verify they pass**

Run:
```sh
cargo test -p taiyaku-library scan::
cargo test -p taiyaku-serve config_api::
cargo test -p taiyaku-cli library::
pnpm -C web run typecheck
```
Expected: all green, six new Rust tests passing.

- [ ] **Step 9: Hand-apply each test's defect and watch it fail**

| change | test that must fail |
|---|---|
| move the `excluder.excludes` check to after `entry_count += 1` | `an_excluded_file_does_not_move_the_entry_count` |
| push to `excluded_dirs` but do not `continue` | `an_excluded_directory_is_not_walked_and_is_named` |
| `continue` without pushing to `excluded_dirs` | same test, other half |
| drop `excluded_files += 1`, keep the `continue` | `an_excluded_file_is_skipped_and_counted` |
| `Excluder::build` stops at the first bad line | `a_bad_pattern_is_reported_by_the_scan` |
| `excludes` ignores `Match::Whitelist` and treats any match as exclusion | `a_negation_re_includes_through_the_walk` |

- [ ] **Step 10: Commit**

```sh
git add crates/taiyaku-library/src/scan.rs crates/taiyaku-library/src/scan/tests.rs \
        crates/taiyaku-serve/src/config_api.rs crates/taiyaku-cli/src/library.rs \
        web/src/config.ts web/src/ScanReport.test.tsx web/src/RootRow.test.tsx
git commit -m "O92: the walk applies exclude_globs"
```

---

## Task 4: `forgotten_by_exclusion`

**Files:**
- Modify: `crates/taiyaku-library/src/scan.rs` — `ScanReport`, `for_root`, `forget_unseen`, `scan_root_inner`
- Modify: `crates/taiyaku-serve/src/config_api.rs`, `crates/taiyaku-cli/src/library.rs`, `web/src/config.ts`
- Test: `crates/taiyaku-library/src/scan/tests.rs`

**Interfaces:**
- Consumes: `Held.is_dir` (Task 2), `Excluder::excludes_with_parents` (Task 1).
- Produces: `ScanReport.forgotten_by_exclusion: usize`; `forget_unseen(..., excluder: &Excluder) -> Result<(usize, usize), String>` returning `(gone, forgotten_by_exclusion)`.

- [ ] **Step 1: Write the failing test**

Append to `crates/taiyaku-library/src/scan/tests.rs`:

```rust
/// A row a new pattern now covers is counted apart from a file that
/// genuinely disappeared.
///
/// **One test, both assertions, because the pairing is the claim.** A test
/// asserting only `forgotten_by_exclusion` passes against an implementation
/// that counts every stale row there; a test asserting only `gone` passes
/// against one that counts every stale row there, which is what this
/// codebase did before this field existed. Neither number means anything
/// without the other.
#[test]
fn an_excluded_row_is_counted_apart_from_one_that_vanished() {
    let tree = fixture_with_junk();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();

    // First scan: no patterns, so `extras` and `notes.pdf` are both recorded.
    let plain = add_root(&conn, &tree.path).expect("add");
    let first = scan_root(&conn, &plain).expect("first");
    assert_eq!(first.forgotten_by_exclusion, 0, "nothing was excluded yet");

    // Now exclude `extras`, and delete `notes.pdf` from disk in the same
    // pass. Three rows go — the directory and its two pages — for one
    // reason, and one row goes for the other.
    std::fs::remove_file(tree.path.join("notes.pdf")).expect("rm");
    let root = Root {
        exclude_globs: Some("extras".to_string()),
        ..plain
    };
    let second = scan_root(&conn, &root).expect("second");

    assert_eq!(
        second.gone, 1,
        "notes.pdf disappeared, and only notes.pdf"
    );
    assert_eq!(
        second.forgotten_by_exclusion, 3,
        "the extras directory and its two pages, forgotten for a different reason"
    );
}
```

- [ ] **Step 2: Run it to verify it fails**

Run:
```sh
cargo test -p taiyaku-library scan::tests::an_excluded_row_is_counted_apart_from_one_that_vanished
```
Expected: FAIL to compile — `no field forgotten_by_exclusion on type ScanReport`.

- [ ] **Step 3: Add the field**

In `ScanReport`, after `bad_exclude_patterns`:

```rust
    /// Rows deleted from `scan_entry` because a pattern now covers them,
    /// rather than because the file disappeared.
    ///
    /// **Split out of [`Self::gone`] rather than folded into it.** `gone`
    /// means the file is no longer on disk. "You excluded it" and "it
    /// disappeared" are different facts, and not conflating those is the
    /// whole of this report's discipline — an admin who adds a pattern and
    /// watches `gone` jump has been told the wrong thing.
    ///
    /// Nothing about a work, an edition, a member, a reading position or a
    /// note is destroyed by this: nothing references `scan_entry`
    /// (`schema/004_auth.sql`), and `derive`'s only `DELETE FROM work` is a
    /// fold, which needs the material to have a home elsewhere. The works an
    /// exclusion covers are left standing and stale. See O92's entry.
    pub forgotten_by_exclusion: usize,
```

In `ScanReport::for_root`: `forgotten_by_exclusion: 0,`.
In the §8.1 refusal literal: `forgotten_by_exclusion: 0,` — nothing was
applied, so nothing was forgotten.

- [ ] **Step 4: Teach `forget_unseen` to classify**

```rust
fn forget_unseen(
    conn: &Connection,
    root_id: i64,
    seen: &[Seen],
    held: &HashMap<String, Held>,
    excluder: &Excluder,
) -> Result<(usize, usize), String> {
    let kept: HashSet<&str> = seen.iter().map(|entry| entry.rel_path.as_str()).collect();
    let mut gone = 0;
    let mut forgotten_by_exclusion = 0;
    let stale: Vec<(&str, Held)> = held
        .iter()
        .filter(|(rel_path, _)| !kept.contains(rel_path.as_str()))
        .map(|(rel_path, held)| (rel_path.as_str(), *held))
        .collect();
    for (rel_path, held) in stale {
        conn.execute(
            "DELETE FROM scan_entry WHERE root_id = ?1 AND rel_path = ?2",
            rusqlite::params![root_id, rel_path],
        )
        .map_err(|e| format!("forgetting {rel_path}: {e}"))?;
        conn.execute(
            "DELETE FROM dir_state WHERE root_id = ?1 AND rel_path = ?2",
            rusqlite::params![root_id, rel_path],
        )
        .map_err(|e| format!("forgetting the state of {rel_path}: {e}"))?;
        // Why it is stale, not merely that it is. A row under a directory
        // the walk never descended into has a path that need not match any
        // pattern itself, which is what `excludes_with_parents` is for.
        if excluder.excludes_with_parents(rel_path, held.is_dir) {
            forgotten_by_exclusion += 1;
        } else {
            gone += 1;
        }
    }
    Ok((gone, forgotten_by_exclusion))
}
```

Update its doc comment's last line to say it returns both counts and which is
which.

In `scan_root_inner`, the call site:

```rust
    let (gone, forgotten_by_exclusion) = forget_unseen(&tx, root.id, &seen, &held, &excluder)?;
    report.gone = gone;
    report.forgotten_by_exclusion = forgotten_by_exclusion;
```

- [ ] **Step 4b: Remove the last `expect(dead_code)`**

`Excluder::excludes_with_parents` still carries
`#[cfg_attr(not(test), expect(dead_code, …))]` from Task 1 — Task 3 removed the
other three and deliberately left this one, because this task is its first
non-test caller. Remove it now.

`cargo clippy -p taiyaku-library --all-targets -- -D warnings` must be clean:
with the attribute left on and the method now called, the expectation is
unfulfilled and the gate fails.

- [ ] **Step 5: Add the field downstream**

`config_api.rs` `scan_report_json`: `forgotten_by_exclusion` in the
destructure and `"forgotten_by_exclusion": forgotten_by_exclusion,` in the
object, beside `"gone"`.

`cli/library.rs` `scan_summary`, after the `excluded_files` block:

```rust
    if report.forgotten_by_exclusion > 0 {
        let _ = write!(
            line,
            "; {} entries forgotten because a pattern now covers them",
            report.forgotten_by_exclusion
        );
    }
```

`web/src/config.ts` `Walk`:

```ts
  /** Rows dropped because a pattern now covers them — not because a file vanished. */
  forgotten_by_exclusion: number
```

- [ ] **Step 6: Extend the whole-report test with all four keys**

`the_scan_report_names_what_it_could_not_read` in
`crates/taiyaku-serve/src/config_api/tests.rs` already asserts the report is
sent whole — *"every field present, the empty collections included, not only
the one this test is actually about"*. Extend it rather than writing a second
test; that comment is the contract this change has to keep. Beside the
existing `report["scan"]["unreadable_dirs"]` assertion:

```rust
    // O92: the four fields the exclusion half added, present even though
    // this fixture's root excludes nothing. C4 §3.4 — "nothing excluded this
    // time" and "this field does not exist in this version" have to look
    // different.
    assert!(report["scan"]["excluded_dirs"].is_array(), "{report}");
    assert!(report["scan"]["bad_exclude_patterns"].is_array(), "{report}");
    assert!(report["scan"]["excluded_files"].is_number(), "{report}");
    assert!(
        report["scan"]["forgotten_by_exclusion"].is_number(),
        "{report}"
    );
```

Run it:
```sh
cargo test -p taiyaku-serve config_api::tests::the_scan_report_names_what_it_could_not_read
```
Expected: PASS. Then delete `"excluded_dirs": excluded_dirs,` from
`scan_report_json`'s object — leaving it in the destructure — and watch it
fail. That is the only witness that the route sends the key, since the
destructure alone proves the field is *read*, not that it is *sent*.

- [ ] **Step 7: Fix the two web fixtures again**

Run `pnpm -C web run typecheck`; add `forgotten_by_exclusion: 0` to the `Walk`
defaults in `web/src/ScanReport.test.tsx` and `web/src/RootRow.test.tsx`.

- [ ] **Step 8: Run the tests**

Run:
```sh
cargo test -p taiyaku-library scan::
cargo test -p taiyaku-serve config_api::
pnpm -C web run typecheck
```
Expected: all green.

- [ ] **Step 9: Hand-apply the defects**

| change | test that must fail |
|---|---|
| count every stale row in `gone` and leave `forgotten_by_exclusion` at 0 | `an_excluded_row_is_counted_apart_from_one_that_vanished` |
| count every stale row in `forgotten_by_exclusion` | same test, other assertion |
| `excludes_with_parents` → `excludes` | same test — the two page rows under `extras` land in `gone` |
| `Held.is_dir` hard-coded `false` (Task 2's Step 4 mutation) | same test — `extras` itself is a `dir` row |

The last row is the point of Task 2's note: this test is `is_dir`'s first
witness.

- [ ] **Step 10: Commit**

```sh
git add crates/taiyaku-library/src/scan.rs crates/taiyaku-library/src/scan/tests.rs \
        crates/taiyaku-serve/src/config_api.rs crates/taiyaku-cli/src/library.rs \
        web/src/config.ts web/src/ScanReport.test.tsx web/src/RootRow.test.tsx
git commit -m "O92: forgotten_by_exclusion, apart from gone"
```

---

## Task 5: `PATCH` refuses an unusable pattern

**Files:**
- Modify: `crates/taiyaku-serve/src/config_api.rs` — `root_patch_from_json`
- Test: `crates/taiyaku-serve/src/config_api/tests.rs`

**Interfaces:**
- Consumes: `scan::check_exclude_globs` (Task 1).
- Produces: nothing later tasks depend on.

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-serve/src/config_api/tests.rs`, using that file's
own helpers — `test_support::router_over_a_single_scanned_work()`,
`only_root_id`, `one_field_patch`, `send_json`, `root_text_column` and
`test_support::body_string`, all already in scope there:

```rust
/// An `exclude_globs` line that will not compile is refused at 400 and the
/// column is left alone, so a typo cannot be stored for a scan to meet
/// later.
///
/// **Both halves.** A test asserting only the status passes against a
/// handler that answers 400 after writing; a test asserting only the column
/// passes against one that rejects every patch.
#[tokio::test]
async fn a_roots_patch_refuses_an_exclude_globs_line_that_will_not_compile() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let root_id = only_root_id(&state);

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/roots/{root_id}"),
        &one_field_patch("exclude_globs", serde_json::json!("extras")),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // `a{b` is one of the two shapes `ignore` 0.4.33 actually rejects,
    // measured 2026-09-09. `a[` and `[[:bogus:]]` compile, so neither would
    // exercise this route.
    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/roots/{root_id}"),
        &one_field_patch("exclude_globs", serde_json::json!("extras\na{b")),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let text = test_support::body_string(response).await;
    assert!(text.contains("a{b"), "the refusal names the line: {text}");

    assert_eq!(
        root_text_column(&state, root_id, "exclude_globs").as_deref(),
        Some("extras"),
        "the stored pattern survived the refused patch"
    );
}

/// Clearing the column is not a pattern and must not be validated as one.
#[tokio::test]
async fn a_roots_patch_may_still_clear_exclude_globs() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let root_id = only_root_id(&state);

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/roots/{root_id}"),
        &one_field_patch("exclude_globs", serde_json::json!("extras")),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/roots/{root_id}"),
        &one_field_patch("exclude_globs", serde_json::Value::Null),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        root_text_column(&state, root_id, "exclude_globs"),
        None,
        "an empty column is not a pattern and must not be refused as one"
    );
}
```

- [ ] **Step 2: Run them to verify they fail**

Run:
```sh
cargo test -p taiyaku-serve config_api::tests::a_roots_patch_refuses_an_exclude_globs_line_that_will_not_compile
```
Expected: FAIL — 204 where 400 was expected.

- [ ] **Step 3: Validate in `root_patch_from_json`**

In `config_api.rs`, replace the `exclude_globs` line of the `scan::RootPatch`
literal with a validated local built above the `Ok(...)`:

```rust
    // **Refused here rather than at the store**, so a typo is caught at save
    // rather than after a scan. `Some(None)` — clearing the column — is not
    // a pattern and is not checked. What this catches is narrow: `ignore`
    // rejects a dangling `\` and an unclosed `{` and accepts a great deal
    // that means nothing useful, which is why a stored pattern that will not
    // build is *also* handled, in `scan::Excluder::build`, rather than
    // treated as impossible.
    let exclude_globs = patch_nullable_text(body, "exclude_globs", "root")?;
    if let Some(Some(patterns)) = &exclude_globs {
        scan::check_exclude_globs(patterns)?;
    }
```

and use `exclude_globs,` in the struct literal.

- [ ] **Step 4: Run the tests**

Run:
```sh
cargo test -p taiyaku-serve config_api::
```
Expected: all green, two new tests passing.

- [ ] **Step 5: Hand-apply the defects**

| change | test that must fail |
|---|---|
| drop the `check_exclude_globs` call | `a_roots_patch_refuses_an_exclude_globs_line_that_will_not_compile` |
| validate `Some(None)` too, treating a cleared column as `""` that must parse | neither — `""` is valid; instead make it `check_exclude_globs("!")?` on the `None` arm and watch `a_roots_patch_may_still_clear_exclude_globs` fail |
| call `check_exclude_globs` after `update_root` | the "stored pattern survived" assertion |

- [ ] **Step 6: Commit**

```sh
git add crates/taiyaku-serve/src/config_api.rs crates/taiyaku-serve/src/config_api/tests.rs
git commit -m "O92: PATCH refuses a pattern that will not compile"
```

---

## Task 6: The scan report draws all four

**Files:**
- Modify: `web/src/ScanReport.tsx` — `WalkSection`
- Test: `web/src/ScanReport.test.tsx`

**Interfaces:**
- Consumes: the four `Walk` fields from Tasks 3 and 4.

- [ ] **Step 1: Write the failing tests**

Append to `web/src/ScanReport.test.tsx`, in the walk-section describe block:

```tsx
it('draws all four exclusion fields when nothing was excluded', () => {
  render(<ScanReport scan={walk({})} derive={derived({})} />)

  // Per element, never a whole-page toContain: a page-level match cannot
  // tell a zero beside the right label from one beside the wrong one.
  for (const [field, label] of [
    ['excluded_dirs', 'Directories excluded'],
    ['excluded_files', 'Files excluded'],
    ['forgotten_by_exclusion', 'Forgotten because excluded'],
    ['bad_exclude_patterns', 'Unusable exclude patterns'],
  ]) {
    const row = document.querySelector(`[data-field="${field}"]`)
    expect(row, `${field} must be drawn`).not.toBeNull()
    expect(row?.querySelector('dt')?.textContent).toBe(label)
    expect(row?.querySelector('dd')?.textContent).toContain('0')
  }
})

it('names each excluded directory and each unusable pattern', () => {
  render(
    <ScanReport
      scan={walk({
        excluded_dirs: ['Naruto/extras', 'Bleach/scans'],
        excluded_files: 12,
        forgotten_by_exclusion: 40,
        bad_exclude_patterns: ['`\\`: dangling'],
      })}
      derive={derived({})}
    />,
  )

  const dirs = document.querySelector('[data-field="excluded_dirs"]')
  expect(dirs?.querySelectorAll('li')).toHaveLength(2)
  expect(dirs?.textContent).toContain('Naruto/extras')
  expect(dirs?.textContent).toContain('Bleach/scans')

  expect(
    document.querySelector('[data-field="excluded_files"] dd')?.textContent,
  ).toBe('12')
  expect(
    document.querySelector('[data-field="forgotten_by_exclusion"] dd')
      ?.textContent,
  ).toBe('40')
  expect(
    document.querySelector('[data-field="bad_exclude_patterns"]')?.textContent,
  ).toContain('dangling')
})
```

- [ ] **Step 2: Run them to verify they fail**

Run:
```sh
npx vitest run --project jsdom src/ScanReport.test.tsx
```
from `web/`. Expected: FAIL — `excluded_dirs must be drawn`, received `null`.

- [ ] **Step 3: Draw the four rows**

In `web/src/ScanReport.tsx`'s `WalkSection`, the did-not-do fields lead, so
these go immediately after the `unreadable_dirs` `<Paths>` and before the
`refused` count:

```tsx
        <Paths
          field="excluded_dirs"
          label="Directories excluded"
          paths={walk.excluded_dirs}
        />
        <Paths
          field="bad_exclude_patterns"
          label="Unusable exclude patterns"
          paths={walk.bad_exclude_patterns}
        />
        <Count
          field="excluded_files"
          label="Files excluded"
          n={walk.excluded_files}
        />
        <Count
          field="forgotten_by_exclusion"
          label="Forgotten because excluded"
          n={walk.forgotten_by_exclusion}
        />
```

- [ ] **Step 4: Run the tests**

Run:
```sh
npx vitest run --project jsdom src/ScanReport.test.tsx
```
Expected: PASS.

- [ ] **Step 5: Hand-apply the defects**

| change | test that must fail |
|---|---|
| render `excluded_dirs` only when non-empty | the first test |
| swap the `excluded_files` and `forgotten_by_exclusion` labels | the first test — this is what per-element assertion buys |
| pass `paths={[]}` to the `excluded_dirs` `<Paths>` | the second test |

- [ ] **Step 6: Commit**

```sh
git add web/src/ScanReport.tsx web/src/ScanReport.test.tsx
git commit -m "O92: the scan report draws what was excluded"
```

---

## Task 7: The box becomes a textarea and the label stops apologising

**Files:**
- Modify: `web/src/RootRow.tsx` — the `TextField` interface, `TEXT_FIELDS`, the field JSX, the file-level doc comment
- Test: `web/src/RootRow.test.tsx`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `TextField.multiline?: boolean`.

- [ ] **Step 1: Write the failing tests**

Append to `web/src/RootRow.test.tsx`:

```tsx
it('draws exclude_globs as a textarea holding every line', async () => {
  const host = await settle({ exclude_globs: '**/extras/**\n*.nfo' })

  const box = host.querySelector('textarea[name="exclude_globs"]')
  expect(box, 'the column is multi-line and its box must be too').not.toBeNull()
  expect((box as HTMLTextAreaElement).value).toBe('**/extras/**\n*.nfo')
  expect(
    host.querySelector('input[name="exclude_globs"]'),
    'and it is no longer a single-line input',
  ).toBeNull()
})

it('patches a two-line exclude_globs with its newline intact', async () => {
  vi.mocked(config.patchRoot).mockResolvedValue(undefined)
  const host = await settle({ exclude_globs: null })

  await edit(host, 'exclude_globs', 'extras\n*.nfo')

  expect(config.patchRoot).toHaveBeenCalledWith(3, {
    exclude_globs: 'extras\n*.nfo',
  })
})

it('no longer says exclude_globs is unapplied', async () => {
  const host = await settle({ exclude_globs: null })

  const label = host.querySelector('textarea[name="exclude_globs"]')
    ?.closest('label')
    ?.querySelector('span')?.textContent
  expect(label).toBe('Exclude globs (one gitignore pattern per line)')
})
```

**`edit` currently targets `input[name=...]`.** Widen its selector to
`[name="..."]` so it finds either, and check no other test depended on the
narrower one.

- [ ] **Step 2: Run them to verify they fail**

Run:
```sh
npx vitest run --project jsdom src/RootRow.test.tsx
```
Expected: FAIL — `the column is multi-line and its box must be too`.

- [ ] **Step 3: Add the flag and the label**

In `web/src/RootRow.tsx`, the `TextField` interface gains:

```ts
  /**
   * Drawn as a `<textarea>` rather than an `<input>`.
   *
   * `exclude_globs` alone: it holds one pattern per line, and a newline is
   * the only separator a path cannot itself contain — a comma and a space
   * both occur in real directory names, so either would make some real
   * pattern unwritable.
   */
  multiline?: boolean
```

and the `exclude_globs` entry of `TEXT_FIELDS` becomes:

```ts
  {
    name: 'exclude_globs',
    // O92 is closed: the scanner applies this now. The label says what the
    // syntax is, because gitignore's is not what "globs" alone implies —
    // `!` negates, a leading `/` anchors, a trailing `/` means the
    // directory. It does *not* claim the pattern is checked: the server
    // rejects a dangling `\` and an unclosed `{` and accepts everything
    // else, which is a guard against two typos rather than a verification.
    label: 'Exclude globs (one gitignore pattern per line)',
    multiline: true,
    from: (r) => r.exclude_globs ?? '',
    patch: (exclude_globs) => ({ exclude_globs }),
  },
```

- [ ] **Step 4: Branch the JSX**

The handlers are identical for both elements, so lift them rather than
duplicating:

```tsx
        {TEXT_FIELDS.map((field) => {
          const boxProps = {
            name: field.name,
            defaultValue: field.from(root),
            onFocus: () => {
              edited.current.delete(field.name)
            },
            onChange: () => {
              edited.current.add(field.name)
            },
            onBlur: (
              e: React.FocusEvent<HTMLInputElement | HTMLTextAreaElement>,
            ) => {
              const next = e.target.value
              // Nothing typed since this box was focused: nothing to write,
              // whatever the box happens to show.
              if (!edited.current.has(field.name)) return
              edited.current.delete(field.name)
              // Edited back to what the row already holds: a no-op.
              if (next === field.from(root)) return
              save(field, next)
            },
          }
          return (
            <label key={field.name}>
              <span>{field.label}</span>
              {field.multiline ? (
                <textarea rows={3} {...boxProps} />
              ) : (
                <input type="text" {...boxProps} />
              )}
            </label>
          )
        })}
```

- [ ] **Step 5: Rewrite the file's doc comment**

Three paragraphs of `RootRow.tsx`'s header describe the column as applied
nowhere. Rewrite them to say what is true now: the column is read, patched
**and applied**; O92 is closed; the `<textarea>` is because the column holds
one pattern per line. **Keep** the paragraph explaining why "edited" is an
`input` event and not a value comparison — that rule is untouched and it cost
three review passes to reach.

- [ ] **Step 6: Run the tests**

Run:
```sh
npx vitest run --project jsdom src/RootRow.test.tsx
pnpm -C web run typecheck
```
Expected: PASS, and no other test in the file broken by the widened `edit`
selector.

- [ ] **Step 7: Hand-apply the defects**

| change | test that must fail |
|---|---|
| `multiline: false` on `exclude_globs` | `draws exclude_globs as a textarea holding every line` |
| `defaultValue={field.from(root).replace('\n', ' ')}` | the same test's value assertion |
| leave the old label string | `no longer says exclude_globs is unapplied` |
| drop `onChange` from `boxProps` | `patches a two-line exclude_globs with its newline intact` |

- [ ] **Step 8: Commit**

```sh
git add web/src/RootRow.tsx web/src/RootRow.test.tsx
git commit -m "O92: the exclude box is a textarea and its label is honest"
```

---

## Task 8: The gate, the real library, and the documents

**Files:**
- Modify: `docs/status.md`, `docs/notes.md`
- Modify: `docs/superpowers/specs/2026-09-04-c4-screens-design.md`, `docs/superpowers/specs/2026-09-02-library-configuration-build-design.md`
- Modify: `crates/taiyaku-library/src/scan/tests.rs` — the `fixture` doc comment's dependency list

- [ ] **Step 1: Run the gate**

Run:
```sh
env -u TAIYAKU_CORPUS scripts/check-all.sh
```
Quote its own `all green` line in the commit or the PR body. Do not pipe
through `tail`; do not append `; echo`.

Then the corpus job separately:
```sh
TAIYAKU_CORPUS="$PWD/corpus" cargo test --workspace --all-features corpus:: \
  -- --nocapture --test-threads=1
```
Expected: ~85 s, twelve standing checks. Read the untruncated output — a
`tail` on a multi-target cargo run reads as exhaustive and is not.

- [ ] **Step 2: Measure against a copy of the real library**

Never against the library itself. Isolate the writes and prove it afterwards.

```sh
# 1. baseline
taiyaku library scan --root /home/davey/temp/manga-test
# record: entries, wall time

# 2. exclude a real subtree, then rescan
#    (set exclude_globs through the API or `update_root`; record which)
# record: excluded_dirs, excluded_files, forgotten_by_exclusion, entries, wall time

# 3. the works under it are still listed and still open
# 4. clear the column, rescan; entries return to (1); no work, progress row
#    or note moved
```

**Step 2's wall-time delta is the figure this work is judged on.** If the walk
is not measurably cheaper, say so rather than omitting it.

- [ ] **Step 3: Record the runs in `docs/notes.md`**

All four, with the date, the machine, and the isolation proof. Measurements
live there, never in `status.md`.

- [ ] **Step 4: Measure the test counts, do not compute them**

```sh
cd web && npx vitest run --project node && npx vitest run --project jsdom
```
Read both numbers off the runs. The Rust figure comes from the gate's JUnit
report and is checked by `scripts/status-test-count.mjs`; the web figures are
gated by nothing, and adding to the number in the document is how it has gone
wrong before.

- [ ] **Step 5: Close O92 — the five-part edit**

In `docs/status.md`:

1. O92's row checkbox → `[x]`.
2. The row's annotation gains the closing date.
3. The prose bullet, rewritten to say what was done and what remains — and
   **marking, not deleting, the passage that reasons from a destructive chain
   that does not exist.** §0.1 of the design has the three links that are
   missing; the wrong reasoning is why the item sat where it did.
4. The test count, at **both** sites: "Five crates, N tests" and "Nine of the
   N are standing checks".
5. The "N are now closed — leaving M" sentence, and the "What remains … M open
   items" line near the bottom. That count word **wraps across a line break** —
   `grep -n -A1 '^What remains:'` and read the next line.

The dependency-list claim gains `ignore`. So does `scan/tests.rs`'s `fixture`
doc comment, which enumerates the crates and already carries a note about
having been written wrong twice.

- [ ] **Step 6: Run the consistency sweep**

```sh
grep -c '^| O[0-9]' docs/status.md
grep -c '^| O[0-9]* | \[x\]' docs/status.md
grep -nE '[0-9]{3} tests|Nine of the' docs/status.md
grep -n -A1 '^What remains:' docs/status.md
grep -rn 'O92' docs/ web/src/ crates/
```
The last one is the part the five-part edit does not cover: read **every** hit,
not the ones matching a phrase you expect. Closing O25 left a paragraph four
lines above "What remains" saying the opposite, and no phrase-grep found it.

- [ ] **Step 7: Correct the two specs**

`2026-09-04-c4-screens-design.md` §1 and §8 record "no `exclude_globs`
behaviour" as this screen's deliberate gap. `2026-09-02-library-configuration-build-design.md`'s
*"every one of those is already a field"* is now true. Both are historical
narrative: mark them (`*O92 has since closed — see its entry.*`) rather than
rewriting them.

- [ ] **Step 8: Re-run the gate and commit**

```sh
env -u TAIYAKU_CORPUS scripts/check-all.sh
git add docs crates web
git commit -m "O92: close it, and record the measurement"
```

A docs-and-tests commit still needs the gate: the test count is a claim
`docs/status.md` makes, and `check-all.sh` runs two checks `fmt`, `clippy` and
`nextest` do not.

---

## After the plan

**Whole-branch review, not per-task.** Per-task review passed on every task of
#91 while a whole-branch pass found seven Criticals. **Review the fix rounds
too** — on #106 that found three more, one of them a regression the fix round
introduced; on #112 passes 2–4 found *only* defects inside earlier fixes. After
fixing a finding, grep for its **shape**, not its function.

**The mutation sweep will have little to say here** and a clean result is not
evidence. It generates no mutant for a string literal, a struct field
initialiser or a `json!` key — which is most of Tasks 3, 4 and 6. Run
`scripts/mutants.sh --since main` after committing the fixes, not before, and
check the `unviable` count against what was changed.

**A reported green gate is a claim, not a result.** If any of this is
delegated, re-run `check-all.sh` and quote its own `all green` line yourself —
a figure that arrives in a report has not been measured by you, and every
figure recorded without being measured on this project has been wrong.
