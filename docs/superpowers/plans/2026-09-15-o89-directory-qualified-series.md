# O89 — directory-qualified series: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop a filename capture discarding the qualifier a parent directory carries, so two releases of one volume stay two series instead of becoming two members of one edition.

**Architecture:** A reconciliation at the end of `rules::Engine::resolve`, after the rule loop and before `apply_root_fallback`. The loop remembers the last series a `path`/`parent` rule set and whether the rule that currently holds the series set it by capture. If it did, and the remembered value *qualifies* the current one, the remembered value is restored with the id of the rule that set it. `classify`, the schema, `proposals` and the seeded rules are untouched.

**Tech Stack:** Rust (edition 2024, toolchain 1.98.1), `regex`, `rusqlite`, `cargo test`, `scripts/check-all.sh`.

**Spec:** [`docs/superpowers/specs/2026-09-15-o89-directory-qualified-series-design.md`](../specs/2026-09-15-o89-directory-qualified-series-design.md)

**Amended before dispatch (2026-09-16).** A pre-flight scan found the first
version of this plan could not be built as specified. Its guard was redundant,
the precedence spec §4 promised was unimplementable, its `extends` let a
trailing space count as a qualifier on a real 37-member directory, Task 1
alone failed `clippy -D warnings` (an uncalled private function), its
predicted figures came from before the Vagabond repack, and its end-to-end
task named no real helpers. The spec's §1.3 records each one. The original is
commit `cdd6ae5`.

## Global Constraints

- **Only a captured series is reconciled.** A literal `set_series`, on a rule of any `match_on`, is never overridden.
- **`qualifies(directory, captured)`** is true exactly when `directory` starts with `captured`, the remainder begins with one of `' '`, `'-'`, `'_'`, `'('`, `'['`, and the remainder contains at least one alphanumeric character.
- **A restored series is credited to the `path`/`parent` rule that set it**, never to the capturing rule and never to an invented id.
- The reconciliation runs after the rule loop and before `apply_root_fallback`.
- No change to `crates/taiyaku-library/src/schema/*.sql`, `derive::classify`, `DeriveReport`, `proposals`, or the seeded rules. No new dependencies.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` must pass at every commit; `clippy::pedantic` is advisory.
- Test temp directories come from `crate::tests::tempdir` and nowhere else (O117).
- **Treat this plan's inline code as untrusted transcription.** Compile it; do not assume it is right. If it is wrong, repair it minimally and say so in the report — never silently.
- A test that passes on first run has not been seen to work. Every new test is watched failing, either before the implementation exists or against the hand mutation its task names, and the result is recorded.
- Commits carry no attribution lines.

## File Structure

| File | Responsibility | Change |
| --- | --- | --- |
| `crates/taiyaku-library/src/rules.rs` | The rules engine | Add `QUALIFIER_SEPARATORS`, `qualifies`, `captured_series`; route `apply_captures` through `captured_series`; add the tracking and the reconciliation to `Engine::resolve` |
| `crates/taiyaku-library/src/rules/tests.rs` | Engine unit tests | Add `Rule::sets_series`; add the reconciliation's tests |
| `crates/taiyaku-library/src/derive/tests.rs` | Derivation tests | Add the end-to-end test |
| `docs/status.md`, `docs/notes.md` | The open-items table and the working log | Close O89 (controller) |

Tasks 1–3 go to implementers. Tasks 4–6 are the controller's own: the
mutation table, the real-library drive, the gate, and the docs.

---

### Task 1: `qualifies`, `captured_series`, and the reconciliation

**Files:**
- Modify: `crates/taiyaku-library/src/rules.rs` — add items after `match_target` (≈ line 640); edit `apply_captures` (≈ line 672); edit `Engine::resolve` (≈ lines 437–496)
- Test: `crates/taiyaku-library/src/rules/tests.rs` (append)

**Interfaces:**
- Consumes: `MatchOn`, `Field::Series`, `Resolved::set`, `Resolved::provenance`, `apply_set_columns`, `apply_captures`, `apply_root_fallback`; test helpers `engine_of`, `defaults`, `root` (all existing).
- Produces:
  - `const QUALIFIER_SEPARATORS: [char; 5]` (private)
  - `fn qualifies(directory: &str, captured: &str) -> bool` (private)
  - `fn captured_series<'h>(caps: &regex::Captures<'h>) -> Option<&'h str>` (private)
  - `Engine::resolve` keeps its signature `pub fn resolve(&self, root: &Root, rel_path: &str) -> Resolved`.

`qualifies` is folded into this task with its caller on purpose. Committed on
its own it has no non-test caller, and `-D warnings` fails it as dead code.
O92's run met the same thing.

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-library/src/rules/tests.rs` (the file already has `use super::*;`, so private items are in scope):

```rust
/// O89: what "a directory qualifies the captured series" means, exactly.
///
/// The separator condition stops `One Piecey` counting as a qualified
/// `One Piece`. The letter-or-digit condition stops a trailing space counting
/// as a qualifier — the real library has
/// `Hunter × Hunter (Official Colored) ` with one, and 37 members would have
/// moved to a series name ending in whitespace.
#[test]
fn a_directory_qualifies_a_series_only_across_a_separator_and_with_something_to_say() {
    // The three merged shapes measured in the real library, 2026-09-15.
    assert!(qualifies("[Oshi no Ko] - Fan Translated", "[Oshi no Ko]"));
    assert!(qualifies("Death Note (Colored Council)", "Death Note"));
    assert!(qualifies("Yu Yu Hakusho - Colored (Colored Council)", "Yu Yu Hakusho"));

    // Every admitted separator.
    for directory in ["A Q", "A - Q", "A_Q", "A (Q)", "A [Q]"] {
        assert!(qualifies(directory, "A"), "{directory:?} qualifies A");
    }

    // A remainder with nothing alphanumeric in it is not a qualifier.
    assert!(!qualifies("Hunter × Hunter (Official Colored) ", "Hunter × Hunter (Official Colored)"));
    assert!(!qualifies("A - ", "A"));
    assert!(!qualifies("A ()", "A"));

    // Equal strings leave no remainder at all.
    assert!(!qualifies("Vagabond", "Vagabond"));

    // A prefix running straight into a word character is a different series.
    assert!(!qualifies("One Piecey", "One Piece"));

    // Not a prefix at all: the case seeded rule 4 exists for.
    assert!(!qualifies("Downloads", "One Piece"));

    // Never in the other direction.
    assert!(!qualifies("A", "A - Q"));
}

/// O89's headline: seeded rule 1 reads the directory, seeded rule 4 — the next
/// rule to set a series — captures a shorter one from the filename, and the
/// qualifier survives.
#[test]
fn a_directory_that_qualifies_the_captured_series_keeps_its_qualifier() {
    let engine = engine_of(&defaults());

    let fan = engine.resolve(&root(), "[Oshi no Ko] - Fan Translated/[Oshi no Ko] - v01.cbz");
    assert_eq!(fan.series.as_deref(), Some("[Oshi no Ko] - Fan Translated"));
    assert_eq!(fan.volume, Some(1.0), "the volume still comes from rule 2");

    let official = engine.resolve(&root(), "[Oshi no Ko]/[Oshi no Ko] - v01.cbz");
    assert_eq!(official.series.as_deref(), Some("[Oshi no Ko]"));

    let parenthesised = engine.resolve(
        &root(),
        "Death Note (Colored Council)/Death Note - v01 (Colored Council).cbz",
    );
    assert_eq!(parenthesised.series.as_deref(), Some("Death Note (Colored Council)"));
}

/// The case seeded rule 4 exists for, which this must not break: a root whose
/// first directory is not a series. The test that fails if the reconciliation
/// is written too wide.
#[test]
fn a_directory_that_is_not_the_series_still_loses_to_the_filename() {
    let engine = engine_of(&defaults());

    let resolved = engine.resolve(&root(), "Downloads/One Piece - v01.cbz");
    assert_eq!(resolved.series.as_deref(), Some("One Piece"));
    assert_eq!(resolved.provenance[&Field::Series], 4, "rule 4 still decided it");
}

/// The trailing-space directory, through the engine rather than the predicate.
#[test]
fn a_trailing_space_on_a_directory_does_not_become_part_of_the_series() {
    let engine = engine_of(&defaults());

    let resolved = engine.resolve(
        &root(),
        "Hunter × Hunter (Official Colored) /Hunter × Hunter (Official Colored) - v01.cbz",
    );
    assert_eq!(resolved.series.as_deref(), Some("Hunter × Hunter (Official Colored)"));
}
```

- [ ] **Step 2: Watch them fail**

Run: `cargo test -p taiyaku-library rules::tests 2>&1 | tail -30`
Expected: a compile error, `cannot find function 'qualifies' in this scope`.

- [ ] **Step 3: Add the three items after `match_target`**

```rust
/// What may stand between a series and the qualifier a directory adds to it.
///
/// **A prefix that runs straight into a word character is a different name**,
/// not a qualified one: `One Piecey` is not `One Piece` with something added.
const QUALIFIER_SEPARATORS: [char; 5] = [' ', '-', '_', '(', '['];

/// Whether `directory` is `captured` plus a qualifier — `[Oshi no Ko] - Fan
/// Translated` against `[Oshi no Ko]`, or `Death Note (Colored Council)`
/// against `Death Note`.
///
/// Three conditions: the directory starts with the captured series, what
/// follows begins with a separator, and what follows says something — at
/// least one letter or digit. The third is O89's pre-flight finding: the real
/// library holds `Hunter × Hunter (Official Colored) ` with a trailing space,
/// and without it 37 members would have moved to a series name ending in
/// whitespace. Equal strings leave nothing after the prefix and fail the
/// second condition, so they need no case of their own.
fn qualifies(directory: &str, captured: &str) -> bool {
    directory.strip_prefix(captured).is_some_and(|rest| {
        rest.starts_with(QUALIFIER_SEPARATORS) && rest.chars().any(char::is_alphanumeric)
    })
}

/// The non-empty `series` a capture carries.
///
/// Shared by [`apply_captures`], which sets the field from it, and
/// [`Engine::resolve`], which has to know whether a capture — rather than a
/// literal `set_series` on the same rule — decided the field. Provenance
/// cannot say, because both paths record the same rule id. One function, so
/// the two cannot disagree about an empty group.
fn captured_series<'h>(caps: &regex::Captures<'h>) -> Option<&'h str> {
    caps.name("series").map(|m| m.as_str()).filter(|s| !s.is_empty())
}
```

- [ ] **Step 4: Route `apply_captures` through the helper**

In `apply_captures`, replace

```rust
    if let Some(m) = caps.name("series").filter(|m| !m.as_str().is_empty()) {
        resolved.set(Field::Series, rule.id, m.as_str().to_string());
    }
```

with

```rust
    if let Some(series) = captured_series(caps) {
        resolved.set(Field::Series, rule.id, series.to_string());
    }
```

- [ ] **Step 5: Add the tracking and the reconciliation to `Engine::resolve`**

Immediately after `let mut resolved = Resolved::empty();`:

```rust
        // O89: the last series a directory-reading rule set, with that rule's
        // id, and whether the rule now holding the series set it by capture.
        // `Resolved::set` overwrites, so a value a later rule discards has to
        // be kept here to be restorable after the loop.
        let mut from_directory: Option<(String, i64)> = None;
        let mut series_was_captured = false;
```

Inside the loop, replace

```rust
            if let Some(caps) = captures {
                apply_captures(rule, &caps, &mut resolved);
            }
```

with

```rust
            // O89: whether a capture, rather than one of this rule's literal
            // columns, is what decides the series. The capture applies after
            // the literals, so a capture carrying a series has the last word.
            let series_from_capture = captures.as_ref().and_then(captured_series).is_some();
            if let Some(caps) = captures {
                apply_captures(rule, &caps, &mut resolved);
            }
            if resolved.provenance.get(&Field::Series) == Some(&rule.id) {
                series_was_captured = series_from_capture;
                if matches!(rule.match_on, MatchOn::Path | MatchOn::Parent) {
                    from_directory = resolved.series.clone().map(|series| (series, rule.id));
                }
            }
```

After the loop, immediately before `apply_root_fallback(root, &mut resolved);`:

```rust
        // O89: restore a qualifier a capture discarded.
        //
        // Seeded rule 1 reads `[Oshi no Ko] - Fan Translated` from the
        // directory; seeded rule 4, the next rule to set a series, captures
        // `[Oshi no Ko]` from the filename. Only a *captured* series is reconciled — a literal
        // `set_series` is a decision and keeps its value — and only when the
        // directory's value qualifies it, so `Downloads/One Piece - v01.cbz`,
        // the case rule 4 exists for, is untouched. Provenance names the rule
        // that read the directory: that rule decided the value, and this step
        // is not a rule.
        if series_was_captured
            && let (Some(current), Some((directory, directory_rule))) =
                (resolved.series.as_deref(), from_directory.as_ref())
            && qualifies(directory, current)
        {
            let (directory, directory_rule) = (directory.clone(), *directory_rule);
            resolved.set(Field::Series, directory_rule, directory);
        }
```

If the borrow checker or a lint rejects any of this as written, repair it
minimally and record what and why in the report.

- [ ] **Step 6: Watch them pass, then run the whole crate**

Run: `cargo test -p taiyaku-library rules::tests 2>&1 | tail -15`
Expected: PASS, including the four new tests.

Run: `cargo test -p taiyaku-library 2>&1 | grep -E '^test result|FAILED|panicked'`
Expected: `test result: ok.` with **835** passed (the baseline on this branch was 831 passed, 0 failed, 4 ignored), 0 failed. *(Executed 2026-09-16: 832 passed and 3 failed — three `preview::tests` pinned O95's colour-edition fold, which this reconciliation reverses. The user decided O89 governs, and a fix round moved them, landing on **836** passed. See the ledger and spec §1.4.)* If an existing test fails, read it before changing anything: it may encode the old resolution on purpose, and that is a finding for the report, not something to edit away.

- [ ] **Step 7: Watch the guard tests fail against their mutations**

`a_directory_that_is_not_the_series_still_loses_to_the_filename` and
`a_trailing_space_on_a_directory_does_not_become_part_of_the_series` guard
behaviour that was already right before this task, so Step 2 did not show them
failing for the right reason. Apply each mutation, run
`cargo test -p taiyaku-library rules::tests 2>&1 | grep -E 'FAILED|test result'`,
record which tests failed, and restore:

| Mutation | Must fail |
| --- | --- |
| In `qualifies`, replace the whole closure body with `true` | ~~`a_directory_that_is_not_the_series_still_loses_to_the_filename`~~ — *cannot: `"Downloads"` is not a prefix of `"One Piece"`, so the closure never runs. Measured 2026-09-16: fails the `qualifies` unit test, the trailing-space test, `the_four_defaults_read_a_real_library_path` and `load_reads_the_seeded_defaults_from_a_real_migration`.* |
| In `qualifies`, delete `&& rest.chars().any(char::is_alphanumeric)` | `a_trailing_space_on_a_directory_does_not_become_part_of_the_series` |

Then confirm `git diff` shows only this task's intended changes.

- [ ] **Step 8: Lint**

Run: `cargo clippy -p taiyaku-library --all-targets --all-features -- -D warnings 2>&1 | tail -5`
Expected: no warnings or errors.

Run: `cargo fmt --all -- --check`
Expected: no output. If there is a diff, run `cargo fmt --all` and include the result.

- [ ] **Step 9: Commit**

```bash
git add crates/taiyaku-library/src/rules.rs crates/taiyaku-library/src/rules/tests.rs
git commit -m "O89: a directory that qualifies a captured series keeps its qualifier

Seeded rule 1 reads [Oshi no Ko] - Fan Translated from the directory and
seeded rule 4, the next rule to set a series, captures [Oshi no Ko],
discarding the qualifier. Engine::resolve now restores it when the
directory's value extends the captured one across a separator and the
remainder has something alphanumeric in it, crediting the rule that read
the directory.

Downloads/One Piece - v01.cbz, the case rule 4 exists for, still resolves
One Piece. A trailing space on a directory is not a qualifier, which is
the real library's Hunter x Hunter (Official Colored) directory."
```

---

### Task 2: Precedence and provenance, pinned in both directions

**Files:**
- Test: `crates/taiyaku-library/src/rules/tests.rs`

**Interfaces:**
- Consumes: Task 1's reconciliation; helpers `engine_of`, `rule`, `defaults`, `root`.
- Produces: `Rule::sets_series(self, series: &str) -> Rule`, test-only, beside `sets_edition_key` and `sets_language`.

These tests pin behaviour Task 1 already implements, so they pass on first
run. *(The last three were added after Task 1's review found the tracking arms they
guard untested: `| MatchOn::Parent`, capture-of-series versus captures-at-all, and
two directory rules in sequence.)* **Each is only evidence once it has been seen failing against the
mutation named for it in Step 3.**

- [ ] **Step 1: Add the missing builder**

`Rule` has a `set_series` field and `rule()` sets it to `None`, but only
`sets_edition_key` and `sets_language` exist as builders. Add the third to the
`impl Rule` block near the top of `crates/taiyaku-library/src/rules/tests.rs`,
in the same form:

```rust
    #[cfg(test)]
    fn sets_series(mut self, series: &str) -> Self {
        self.set_series = Some(series.to_string());
        self
    }
```

- [ ] **Step 2: Write the tests**

Append:

```rust
/// A restored series is credited to the rule that read the directory.
///
/// `crate::preview` reads provenance to tell a person why a field holds what it
/// holds. Any id the reconciliation invented would either name a rule that did
/// not decide this or collide with `ROOT_FALLBACK`.
#[test]
fn a_restored_series_is_credited_to_the_rule_that_read_the_directory() {
    let engine = engine_of(&defaults());

    let resolved = engine.resolve(&root(), "[Oshi no Ko] - Fan Translated/[Oshi no Ko] - v01.cbz");
    assert_eq!(resolved.series.as_deref(), Some("[Oshi no Ko] - Fan Translated"));
    assert_eq!(resolved.provenance[&Field::Series], 1);
}

/// Literals win: a person who writes the short name as a literal keeps it.
#[test]
fn a_literal_series_on_a_name_rule_is_never_reconciled() {
    let mut rules = defaults();
    rules.push(rule(5, "prefix", "", "name").sets_series("[Oshi no Ko]"));
    let engine = engine_of(&rules);

    let resolved = engine.resolve(&root(), "[Oshi no Ko] - Fan Translated/[Oshi no Ko] - v01.cbz");
    assert_eq!(
        resolved.series.as_deref(),
        Some("[Oshi no Ko]"),
        "a literal is a decision, so the directory does not override it"
    );
    assert_eq!(resolved.provenance[&Field::Series], 5);
}

/// Captures reconcile: a user rule that *extracts* the short name from the
/// filename is treated exactly like seeded rule 4.
#[test]
fn a_captured_series_on_a_users_name_rule_is_reconciled_like_rule_four() {
    let mut rules = defaults();
    rules.push(rule(5, "regex", r"^(?<series>\[Oshi no Ko\])", "name"));
    let engine = engine_of(&rules);

    let resolved = engine.resolve(&root(), "[Oshi no Ko] - Fan Translated/[Oshi no Ko] - v01.cbz");
    assert_eq!(resolved.series.as_deref(), Some("[Oshi no Ko] - Fan Translated"));
    assert_eq!(resolved.provenance[&Field::Series], 1);
}

/// A directory equal to the captured series changes nothing, including which
/// rule is credited.
#[test]
fn an_unqualified_directory_leaves_rule_four_credited() {
    let engine = engine_of(&defaults());

    let resolved = engine.resolve(&root(), "Vagabond/Vagabond - v03.cbz");
    assert_eq!(resolved.series.as_deref(), Some("Vagabond"));
    assert_eq!(resolved.provenance[&Field::Series], 4);
}

/// A `parent` rule reads a directory as surely as a `path` rule does. A root with
/// a grouping level above its series — `Manga/<series>/<file>` — is where one is
/// written, because a `path` capture would read `Manga`.
#[test]
fn a_series_read_by_a_parent_rule_is_restored_too() {
    let engine = engine_of(&[
        rule(1, "regex", "^(?<series>.+)$", "parent"),
        rule(2, "builtin-volume", "", "name"),
        rule(4, "regex", "^(?<series>.+?)[ _-]+v", "name"),
    ]);

    let resolved = engine.resolve(
        &root(),
        "Manga/[Oshi no Ko] - Fan Translated/[Oshi no Ko] - v01.cbz",
    );
    assert_eq!(resolved.series.as_deref(), Some("[Oshi no Ko] - Fan Translated"));
    assert_eq!(resolved.provenance[&Field::Series], 1);
}

/// A literal `set_series` stays a literal on a rule that also captures something
/// else. Having captures at all is not the same as capturing the series, and only
/// the second makes the value an extraction that can be reconciled.
#[test]
fn a_literal_series_beside_a_capture_of_another_field_is_never_reconciled() {
    let mut rules = defaults();
    rules.push(rule(5, "regex", r"v(?<volume>\d+)", "name").sets_series("[Oshi no Ko]"));
    let engine = engine_of(&rules);

    let resolved = engine.resolve(&root(), "[Oshi no Ko] - Fan Translated/[Oshi no Ko] - v01.cbz");
    assert_eq!(
        resolved.series.as_deref(),
        Some("[Oshi no Ko]"),
        "rule 5 captured a volume, not a series, so its series is a literal"
    );
    assert_eq!(resolved.provenance[&Field::Series], 5);
}

/// When two directory-reading rules set the series in turn, the *later* one is the
/// value a capture discarded, so it is the one restored and the one credited.
#[test]
fn the_last_directory_reading_rule_is_the_one_restored() {
    let engine = engine_of(&[
        rule(1, "regex", "^(?<series>[^/]+)/", "path"),
        rule(2, "prefix", "", "path").sets_series("[Oshi no Ko] - Fan TL"),
        rule(3, "builtin-volume", "", "name"),
        rule(4, "regex", "^(?<series>.+?)[ _-]+v", "name"),
    ]);

    let resolved = engine.resolve(&root(), "[Oshi no Ko] - Fan Translated/[Oshi no Ko] - v01.cbz");
    assert_eq!(resolved.series.as_deref(), Some("[Oshi no Ko] - Fan TL"));
    assert_eq!(resolved.provenance[&Field::Series], 2);
}
```

- [ ] **Step 3: Watch each fail against its mutation**

Run each against `cargo test -p taiyaku-library rules::tests 2>&1 | grep -E 'FAILED|test result'`, record, restore:

| Mutation (in `rules.rs`) | Must fail |
| --- | --- |
| In the reconciliation, credit `resolved.provenance[&Field::Series]` instead of `directory_rule` | `a_restored_series_is_credited_to_the_rule_that_read_the_directory` |
| Delete `series_was_captured &&` from the reconciliation's condition | `a_literal_series_on_a_name_rule_is_never_reconciled` |
| Replace `let series_from_capture = …;` with `let series_from_capture = false;` | `a_captured_series_on_a_users_name_rule_is_reconciled_like_rule_four` |
| In `qualifies`, replace the whole closure body with `true` | `an_unqualified_directory_leaves_rule_four_credited` |
| In the loop, change `MatchOn::Path \| MatchOn::Parent` to `MatchOn::Path` | `a_series_read_by_a_parent_rule_is_restored_too` |
| Replace `let series_from_capture = …;` with `let series_from_capture = captures.is_some();` | `a_literal_series_beside_a_capture_of_another_field_is_never_reconciled` |
| In the loop, only record `from_directory` when it is still `None` (first directory rule wins) | `the_last_directory_reading_rule_is_the_one_restored` |

- [ ] **Step 4: Run the crate, lint, commit**

Run: `cargo test -p taiyaku-library 2>&1 | grep -E '^test result|FAILED|panicked'`
Expected: `test result: ok.` with **843** passed, 0 failed. *(836 after Task 1 and its fix round, plus Task 2's seven tests.)*

Run: `cargo clippy -p taiyaku-library --all-targets --all-features -- -D warnings 2>&1 | tail -5` and `cargo fmt --all -- --check`
Expected: clean.

```bash
git add crates/taiyaku-library/src/rules/tests.rs
git commit -m "O89: literals win, captures reconcile, and provenance names the directory

Seven guards on the reconciliation, each watched failing against its own
mutation: a restored series is credited to the rule that read the
directory, a literal set_series on a name rule is never overridden even
beside a capture of another field, a user's capturing name rule is
reconciled exactly like seeded rule 4, an unqualified directory leaves
rule 4 credited, a parent rule is a directory rule, and the last of two
directory rules is the one restored."
```

---

### Task 3: End to end through `derive`

**Files:**
- Test: `crates/taiyaku-library/src/derive/tests.rs` (append)

**Interfaces:**
- Consumes: Task 1's reconciliation. Existing: `crate::tests::tempdir`, `crate::scan::tests::write_archive` (already imported at the top of the file as `write_archive`), `Library::open_in_memory`, `scan::add_root`, `scan::scan_root`, `derive::derive`, `DeriveReport::{merged, works, members}`.
- Produces: nothing.

This follows `two_entries_joining_one_edition_are_counted_as_a_merge` in the
same file, which is the shape to match.

- [ ] **Step 1: Write the test**

The two archives get different page counts so they are not byte-identical.
Derivation treats content identity as evidence that a file moved, and that has
to stay out of this test.

```rust
/// O89 end to end: two releases of one volume, told apart only by their
/// directories, derive as two works of two series rather than one work of two
/// members. Before the reconciliation this was `merged: 1` — a reader opening
/// volume 1 got one release followed by the other as a single page sequence.
#[test]
fn two_releases_told_apart_only_by_their_directories_are_two_works() {
    let tree = crate::tests::tempdir("derive-o89-two-releases");
    write_archive(&tree.path().join("[Oshi no Ko]").join("[Oshi no Ko] - v01.cbz"), 3);
    write_archive(
        &tree
            .path()
            .join("[Oshi no Ko] - Fan Translated")
            .join("[Oshi no Ko] - v01.cbz"),
        5,
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add");
    scan::scan_root(&conn, &root).expect("scan");

    let report = derive::derive(&conn).expect("derive");

    assert_eq!(report.merged, 0, "neither release joined the other's page sequence");
    assert_eq!(report.works, 2);
    assert_eq!(report.members, 2);

    let mut stmt = conn
        .prepare("SELECT sort_title FROM series ORDER BY sort_title")
        .expect("prepare");
    let titles: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("rows");
    assert_eq!(titles, ["[Oshi no Ko]", "[Oshi no Ko] - Fan Translated"]);
}
```

- [ ] **Step 2: Watch it fail without the reconciliation**

Comment out the reconciliation statement in `Engine::resolve` (the
`if series_was_captured && …` statement) and run:

`cargo test -p taiyaku-library derive::tests::two_releases_told_apart_only_by_their_directories_are_two_works 2>&1 | grep -E 'panicked|left|right|test result'`

Expected: FAIL at the `merged` assertion, with left `1` and right `0`. Record
the output. Restore the statement and confirm `git diff crates/taiyaku-library/src/rules.rs` is empty.

- [ ] **Step 3: Watch it pass, run the crate, lint, commit**

Run: `cargo test -p taiyaku-library 2>&1 | grep -E '^test result|FAILED|panicked'`
Expected: `test result: ok.` with **844** passed, 0 failed.

Run: `cargo clippy -p taiyaku-library --all-targets --all-features -- -D warnings 2>&1 | tail -5` and `cargo fmt --all -- --check`
Expected: clean.

```bash
git add crates/taiyaku-library/src/derive/tests.rs
git commit -m "O89: two releases told apart only by their directories derive as two works

The end-to-end shape. Watched failing with the reconciliation commented
out — merged: 1, the two releases as one page sequence — and passing with
it restored."
```

---

### Task 4 (controller): The mutation table and the gate

The controller applies every mutation below by hand. Tasks 1–2 each watched
their own mutations; this is the whole set in one place, against the whole
crate, to find any mutation that fails nothing.

- [ ] **Step 1: Control.** `cargo test -p taiyaku-library -- rules:: derive:: 2>&1 | grep '^test result'` — record the passing count. (Two filters need the `--`.)
- [ ] **Step 2: Apply, run the same command, record, restore — one at a time:**

| # | Mutation | Must fail at least |
| --- | --- | --- |
| 1 | Delete the reconciliation statement | headline, provenance, capture-reconciled, end-to-end |
| 2 | `qualifies`: drop `rest.starts_with(QUALIFIER_SEPARATORS) &&` | the `qualifies` unit test (`One Piecey`) |
| 3 | `qualifies`: drop `&& rest.chars().any(char::is_alphanumeric)` | unit test, trailing-space test |
| 4 | Call site: `qualifies(current, directory)` | headline |
| 5 | Condition: drop `series_was_captured &&` | literal-on-name |
| 6 | Loop: record `from_directory` for every `match_on` (drop the `matches!`) | headline |
| 7 | Credit `resolved.provenance[&Field::Series]` instead of `directory_rule` | provenance, capture-reconciled |
| 8 | `QUALIFIER_SEPARATORS`: remove `'('` | unit test, headline (Death Note) |
| 9 | `qualifies` closure body → `true` | the `qualifies` unit test, unqualified-directory, `the_four_defaults_read_a_real_library_path` *(not `Downloads`, whose directory is no prefix, so the closure never runs)* |
| 10 | `let series_from_capture = true;` | literal-on-name |
| 11 | `captured_series`: drop `.filter(\|s\| !s.is_empty())` | any existing empty-capture test — **record whether one exists** |
| 12 | `MatchOn::Path \| MatchOn::Parent` → `MatchOn::Path` | parent-rule test |
| 13 | `series_from_capture = captures.is_some()` | literal-beside-a-capture test |
| 14 | record `from_directory` only while `None` | last-directory-rule test |
| 15 | `preview.rs` ≈1043–1051: a row's values taken from the last entry sharing its triple rather than the first | `a_rows_values_belong_to_the_entry_it_names` — the hazard its doc names, which its fix-round red did not exercise |

A mutation that fails nothing is a gap. Write the test that catches it before continuing.

- [ ] **Step 3:** `git diff --stat` is empty apart from committed work.
- [ ] **Step 4: Gate.** `./scripts/check-all.sh` — exit 0 and the `all green` verdict line quoted as printed, never read through `tail` or a trailing `; echo`.

### Task 5 (controller): The real library, re-derived and diffed

- [ ] **Step 1:** Back up `/home/davey/temp/manga-test/library.db` to the scratchpad.
- [ ] **Step 2:** Dump `member.path → series.sort_title` before, sorted.
- [ ] **Step 3:** `cargo build --release`, then `target/release/taiyaku library scan --db /home/davey/temp/manga-test/library.db`. Quote the printed summary.
- [ ] **Step 4:** Dump after, diff. Predicted in spec §1.2: `merged` 25 → **0**, works 1870 → **1895**, members **1895**, distinct series holding a member 129 → **133**. Changed lines come from exactly the seven directories tabled there, 61 members in all, and nothing else.
- [ ] **Step 4b:** Check the diff for numbered or range directories, a shape like `Naruto 1-72/Naruto - v01.cbz`. A digit remainder qualifies, so such a directory would become its own series. The simulation predicts none in this library; record what the diff shows.
- [ ] **Step 5:** Record the open `edition-of` proposals. Predicted: present for `[Oshi no Ko] - Fan Translated` and `Yu Yu Hakusho - Colored (Colored Council)`, absent for `Death Note (Colored Council)`.

### Task 6 (controller): Close O89 in the docs

- [ ] **Step 1:** `docs/notes.md` gains `O89: a directory that qualifies a captured series`, holding Task 5's figures, the diff, the proposals result, and Task 4's mutation table.
- [ ] **Step 2:** `docs/status.md`, the five-part edit. `grep -n 'O89' docs/status.md` first.
  1. The row: `[ ]` → `[x]`, a closing clause `— *closed 2026-09-16 by …*` on the What cell, and the new test names on the Where cell.
  2. The closed-ID list in the preamble gains `O89` in numeric position.
  3. The counts: closed +1, open −1.
  4. The history parenthetical gains the established sentence for a close.
  5. Other prose naming O89 as open is marked in place: `*(O89 has since closed — see its entry above)*`.
  Plus both Rust test-count sites, if the gate's count check names them.
- [ ] **Step 3:** Check the counts against the table: `grep -c '^| O[0-9]* | \[ \]' docs/status.md` and the same for `\[x\]`. The grep decides; the preamble must match it.
- [ ] **Step 4:** `./scripts/check-all.sh` — exit 0, verdict quoted.
- [ ] **Step 5:** Commit.

---

## Self-Review

**Spec coverage.** §1's rule, including literals win, captures reconcile, and the letter-or-digit condition → Task 1. §1.1, rule 4's case preserved → Task 1's `Downloads` test. *(It fails when the `qualifies` call is removed, which is the controller's mutation during Task 1's review. Mutation 9 cannot reach it: `Downloads` is no prefix of `One Piece`, so the closure never runs.)* §1.2's figures → Task 5. §1.3's regression → Task 1's trailing-space tests and mutation 3. §2, what does not change → Global Constraints. §4, tests → Tasks 1–3. §5, risks: diff-not-total → Task 5 Step 4; provenance → Task 2 and mutation 7; a test passing against its own defect → the watched-red steps in Tasks 1–3 and the whole of Task 4.

**Placeholder scan.** None. Task 3 names the real helpers
(`crate::tests::tempdir`, `write_archive`, `scan::add_root`,
`scan::scan_root`, `derive::derive`) instead of the first version's "read the
file and adapt".

**Type consistency.** `qualifies(directory: &str, captured: &str) -> bool` is
defined in Task 1, called in that argument order in Task 1 Step 5, and
guarded against a swap by mutation 4. `captured_series` returns
`Option<&'h str>` and is used in `apply_captures` and in the loop.
`from_directory: Option<(String, i64)>` and `series_was_captured: bool` are
local to `resolve`. `Rule::sets_series` is added in Task 2 Step 1, before its
only use in Task 2 Step 2. Expected pass counts: baseline 831 → 835 after
Task 1 (4 tests) → 839 after Task 2 (4 tests) → 840 after Task 3 (1 test).
*Corrected during execution: Task 1's fix round added one preview test and Task 1's
review added three to Task 2, so it is 836 → 843 → 844.*
