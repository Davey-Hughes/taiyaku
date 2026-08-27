# C3 PR 1 — the proposals inbox: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the screen that answers spec B's proposals — a filterable inbox
whose rows expand to triage-grade evidence, where accepting writes a rule the
person was shown first and can widen to resolve many proposals at once.

**Architecture:** Three layers, each already having a home. `taiyaku-library`'s
`proposals.rs` splits `accept` into `preview` / `resolves` / `accept_with` so
the client can be shown the rule before it is written, and gains `list`,
`counts_by_kind` and `evidence` queries. A new `taiyaku-serve` module,
`curation.rs`, is six thin handlers over those — parse, call, serialise —
following `library_api.rs`. The web adds `curation.ts` (HTTP + pure helpers)
and three components at `/proposals`.

**Tech Stack:** Rust (`rusqlite`, `axum`, `serde_json`), TypeScript/React 19
(`react-router`, `vitest` + `jsdom`, `@testing-library/react`).

## Global Constraints

- **Spec:** `docs/superpowers/specs/2026-08-26-library-curation-build-design.md`
  is authoritative; C3's product design is
  `docs/superpowers/specs/2026-08-14-library-curation-design.md`, and the
  mockups it was drawn against are
  `.superpowers/brainstorm/42470-1786918333/content/{proposals,proposal-detail}.html`.
- **Derivation has exactly one input.** Accept writes a `rule` row and never
  touches `series`, `work`, `edition` or `member`. A test that would pass with
  a direct table write is not a test of this.
- **Every new route is admin-gated** (spec A2) and **must** get a row in
  `routes.rs`'s grant table alongside its registration — the enumeration test
  derives its count from that table and from the route lists.
- **The gate is `env -u TAIYAKU_CORPUS scripts/check-all.sh`.** Never run it
  with `TAIYAKU_CORPUS` set: the script exports it into an unfiltered
  `cargo test --workspace`, which runs `geometry_experiment` in debug (40+ min).
  Quote the `all green` line when reporting; piping through `tail` masks the
  exit status.
- **No `Date.now()`-style ambient values in fixtures**, and no attribution
  lines in commit messages.
- **`pnpm -C web test` is NOT the web suite.** It is
  `vitest run --project node` — it never executes a single `.test.tsx` file.
  The gate runs `pnpm -C web run test:coverage`, which is
  `--project node --project jsdom --coverage`, and **that** is the command to
  verify against. `pnpm -C web test:jsdom` runs the component half alone.
  Three tasks were verified with the node-only command before this was noticed;
  nothing was broken by it, but nothing component-shaped was checked either.
- **Web tests land in whichever vitest project matches what they test** —
  `jsdom` for components (`.test.tsx`), **`node` for DOM-free modules**
  (`.test.ts`), which is where `library.test.ts` lives and where
  `curation.test.ts` correctly landed. An earlier wording said "the `jsdom`
  project" flatly, which would have put a DOM-free test where nothing runs it.
- **`@testing-library/react`'s `screen` and `userEvent` are not available
  here** — they are not dependencies, and `web/src/testing/mount.tsx`
  explicitly rejects them. Component tests use this repo's own
  `mount()` + `act()` + `querySelector` convention. Any `screen.getByRole(...)`
  in this plan's example code is illustrative of intent, not of API.
  Either way, nothing here may import `App.tsx` or `resolver.ts`, which
  value-import `taiyaku-wasm` from a gitignored generated directory and break
  a fresh clone (C1 §9).
- **Score scale is `{0, 1, 2, 3}`.** `SIGNAL = 1.0`; `CERTAIN = 2.0 * SIGNAL +
  1.0`. `strong` is 2 and above.
- Rust: `cargo test -p taiyaku-library proposals::` for this module's suite.
  Web: `pnpm -C web test`.

**Two failures this branch has repeated. Both are process, not knowledge —
every author knew the rule and shipped the violation anyway.**

- **A comment must not assert more than its code enforces.** Found **eleven
  times** across Tasks 1–4, every one written by an author actively reasoning
  about the invariant. Two sub-shapes, and the second keeps recurring:
  - *A guard whose prose describes a stronger refusal than the condition
    underneath* — three in Tasks 1–2, each hiding a real hole.
  - **A comment stating a count, where the code around it later changed.**
    "the three `UPDATE`s that reach these rows" beside six; "one helper, two
    callers" beside three; "the five single-row lookups … all seven places"
    beside four and six; "the one query every read goes through" beside five
    reads that do not. **If you write a number in a comment, count it — and
    prefer a phrasing that carries no number**, because the next person to add
    a caller will not come back to update your sentence. The measurement in a comment is load-bearing in this
  repository — `docs/status.md`'s preamble promises every claim traces to a
  measurement — so a comment that overstates is a defect, not untidiness. Read
  the sentence back against the code before committing it.
- **A fixture where the right and the wrong implementation render the same
  proves nothing.** Task 3 shipped with two tests, both supplied by this plan,
  whose filter axis nothing in the fixture could exclude; each still passed
  with its filter fully neutered. Task 4 shipped three more, and its review
  found 13 of 21 mutations surviving. That fix round's best move was building a
  fixture where one side's hub edition is *not* `raw` — because against an
  all-`raw` library `assert_eq!(edition_key, hub)` compares `"raw"` to `"raw"`
  and a hard-coded literal passes it. **Ask what your fixture is incapable of
  disproving before you write the assertion.** **Every test must contain the row it is
  supposed to exclude, the case it is supposed to reject, the value it is
  supposed to change.** For each test: break the thing it covers, confirm
  *that* test fails for the reason you intend, restore. Report those checks.
  Code in this plan is a starting point, not evidence that its fixtures are
  adequate — five of them were not.

- **Mutate the code and ask which test fails — not the reverse.** Task 6
  reported "18 mutations, all caught", honestly, using a *per-test* method: for
  each test added, mutate the thing that test covers. That method cannot find
  behaviour with **no test at all**, and four review findings lived exactly
  there — `consequence()` never asserted the series title it exists to name,
  and the wire-shape fixtures were untyped, so renaming a field on the
  interface passed both the suite and `typecheck`. **A coverage figure has the
  same blind spot**: `curation.ts` measured 100% statements/branches/functions
  /lines while all four survivors were live. Walk the changed lines and ask
  what breaks if each one is wrong.

- **A regression guard that lives in a comment is not a guard.** Task 4's fix
  round asserted a single-sweep property was "pinned structurally" by three
  function signatures; the review found two of the three could reintroduce the
  sweep with no signature change, and that the claimed obstacle to testing it —
  needing rusqlite's `trace` feature — was false. A `#[cfg(test)] static
  AtomicUsize` is three lines and needs no feature. If a property matters
  enough to write a paragraph about, it matters enough to fail a test.

---

## File Structure

**Modified**

| File | Responsibility after this PR |
| --- | --- |
| `crates/taiyaku-library/src/proposals.rs` | gains `ProposedRule`, `Resolution`, `Filter`, `Evidence`; `accept` splits into `preview`/`resolves`/`accept_with`; `open` generalises to `list` |
| `crates/taiyaku-library/src/proposals/tests.rs` | the new behaviours, beside the 1423 lines already there |
| `crates/taiyaku-serve/src/routes.rs` | six route registrations and six grant-table rows |
| `crates/taiyaku-serve/src/lib.rs` | declares `mod curation` |
| `web/src/api.ts` | gains `sendJson`, so the 401 rule stays one rule |
| `web/src/Routes.tsx` | `/proposals` |
| `web/src/Sidebar.tsx` | an admin-only entry carrying the open count |
| `docs/status.md`, `docs/notes.md` | the counts sweep and the measurements |

**Created**

| File | Responsibility |
| --- | --- |
| `crates/taiyaku-serve/src/curation.rs` | six handlers: parse, call `proposals`, serialise |
| `crates/taiyaku-serve/src/curation/tests.rs` | route-level tests including the admin refusal |
| `web/src/curation.ts` | the six calls, their types, and the pure helpers |
| `web/src/curation.test.ts` | the pure helpers |
| `web/src/Proposals.tsx` | the screen: header, filter tabs, list |
| `web/src/Proposals.test.tsx` | |
| `web/src/ProposalRow.tsx` | collapsed line and expanded evidence |
| `web/src/ProposalRow.test.tsx` | |
| `web/src/AcceptForm.tsx` | the form, the rule preview, the widening field |
| `web/src/AcceptForm.test.tsx` | |

---

### Task 1: split `accept` into `preview` and `accept_with`

Split everything `accept` derives *before* it writes into a function that
returns it, so a caller can be shown the rule. `accept`'s behaviour must not
change — the existing suite is the proof.

**Files:**
- Modify: `crates/taiyaku-library/src/proposals.rs`
- Test: `crates/taiyaku-library/src/proposals/tests.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `pub struct ProposedRule { root_id: Option<i64>, kind: String,
  pattern: String, match_on: String, set_series: Option<String>,
  set_edition_key: Option<String>, set_kind: Option<String>,
  set_language: Option<String>, note: String }`;
  `pub fn preview(conn: &Connection, id: i64) -> Result<ProposedRule, String>`;
  `pub fn accept_with(conn: &Connection, id: i64, rule: &ProposedRule) -> Result<i64, String>`;
  `fn matched_edition_word(suffix: &str) -> Option<&'static str>`.
  `accept(conn, id)` becomes exactly `accept_with(conn, id, &preview(conn, id)?)`.

**One task, not two.** An intermediate `accept` that called `preview` and then
re-derived what it returned would be redundant code shipped in its own commit
purely to be deleted by the next one. The split is one refactor with one proof:
the module's existing tests, unchanged, still passing.

- [ ] **Step 1: Write the failing test**

In `crates/taiyaku-library/src/proposals/tests.rs`:

```rust
#[test]
fn preview_returns_the_narrow_prefix_rule_accept_would_write() {
    // §2 of the build design: the client is shown the rule, and the server
    // writes what it was shown. That is only honest if the preview is the
    // same derivation the write uses.
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);

    let previewed = proposals::preview(&conn, proposal.id).expect("preview");

    assert_eq!(previewed.kind, "prefix");
    assert_eq!(previewed.match_on, "path");
    assert!(
        previewed.pattern.ends_with('/'),
        "a prefix pattern names a directory: {}",
        previewed.pattern
    );
    assert_eq!(
        previewed.set_edition_key.as_deref(),
        Some("full-color-edition"),
    );

    // And writing it produces exactly that rule.
    let rule_id = proposals::accept(&conn, proposal.id).expect("accept");
    let (kind, pattern, key): (String, String, Option<String>) = conn
        .query_row(
            "SELECT kind, pattern, set_edition_key FROM rule WHERE id = ?1",
            [rule_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("the written rule");
    assert_eq!(kind, previewed.kind);
    assert_eq!(pattern, previewed.pattern);
    assert_eq!(key, previewed.set_edition_key);
}

#[test]
fn preview_fills_the_edition_kind_the_vocabulary_implies() {
    // §4 of the build design. The matched vocabulary word *is* the signal the
    // proposal rests on, so reading a kind off it asserts nothing new.
    // `language` is deliberately NOT filled: an English-looking suffix is not
    // evidence of a language, and `comicinfo.rs` already refuses that reasoning.
    let library = library_with_series(&[
        "Sousou no Frieren",
        "Sousou no Frieren - Fan Translated",
    ]);
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "edition-of")
        .expect("an edition-of proposal");

    let previewed = proposals::preview(&conn, proposal.id).expect("preview");

    assert_eq!(previewed.set_kind.as_deref(), Some("fan"));
    assert_eq!(previewed.set_language, None);
}

#[test]
fn preview_refuses_exactly_what_accept_refuses() {
    // The refusals are the reason `accept` derived its own pattern. Splitting
    // it must not leave them only on the write path, or a client would be
    // shown a rule the server then rejects.
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    proposals::dismiss(&conn, proposal.id).expect("dismiss");

    let refused = proposals::preview(&conn, proposal.id);

    assert!(
        refused.is_err_and(|e| e.contains("not open")),
        "a dismissed proposal has no rule to preview",
    );
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-library proposals::tests::preview -- --nocapture`

Expected: FAIL to compile — `no function or associated item named `preview``.

- [ ] **Step 3: Extract the vocabulary helper without changing what it decides**

In `crates/taiyaku-library/src/proposals.rs`, add beside `EDITION_WORDS`:

```rust
/// Which vocabulary word a suffix matched, if any.
///
/// [`raise_edition_of`] only ever needed *whether* one did. [`preview`] needs
/// *which*, because the word is what implies the edition kind — see the
/// mapping in [`implied_edition_kind`]. Extracted rather than duplicated so
/// there is one list and one matching rule; `any` and `find(..).is_some()`
/// decide identically, which is what keeps this a refactor.
fn matched_edition_word(suffix: &str) -> Option<&'static str> {
    EDITION_WORDS
        .iter()
        .copied()
        .find(|word| suffix.contains(word))
}

/// The `edition.kind` a matched vocabulary word implies.
///
/// **This is a reading of the word, not a guess about the material.** Seven of
/// the eight words name a publisher's own edition; `Fan Translated` names the
/// other thing entirely. `language` gets no such treatment and is left unset —
/// an English-looking suffix is evidence about the *suffix*, and
/// `serve/src/comicinfo.rs` already records why deriving a language from a key
/// would be a guess presented as metadata.
fn implied_edition_kind(word: &str) -> &'static str {
    if word == "Fan Translated" { "fan" } else { "official" }
}
```

Change the one existing use at the `vocab_hit` line inside `raise_edition_of`:

```rust
let vocab_hit = matched_edition_word(suffix).is_some();
```

- [ ] **Step 4: Add `ProposedRule` and `preview`**

```rust
/// The `rule` row an accept would write, before it is written.
///
/// §7 of C3: *"the client shows the rule, the server writes what it was
/// shown"*. A server that re-derived the rule at write time could write
/// something the preview did not say, which is the whole reason this type
/// crosses the wire in both directions.
#[derive(Debug, Clone, PartialEq)]
pub struct ProposedRule {
    /// `NULL` means every root, which is what a widened rule usually wants.
    pub root_id: Option<i64>,
    /// `prefix` for the narrow default; `regex` once widened.
    pub kind: String,
    pub pattern: String,
    /// Always `path` here. `rules.rs` also has `name` and `parent`; neither
    /// addresses "this directory's material", which is what an accept claims.
    pub match_on: String,
    pub set_series: Option<String>,
    pub set_edition_key: Option<String>,
    pub set_kind: Option<String>,
    pub set_language: Option<String>,
    pub note: String,
}

/// The rule [`accept`] would write for proposal `id`, without writing it.
///
/// Every refusal [`accept`] makes before its transaction opens is made here
/// instead, so a client is never shown a rule the write will reject: the
/// state guard, the `attach-chapters` refusal by name, a missing `target_id`,
/// a subject whose entries share no single directory, a target renamed since
/// the proposal was raised, and a suffix with no letters or digits to make an
/// edition key from.
///
/// # Errors
///
/// As [`accept`], for every condition listed above.
pub fn preview(conn: &Connection, id: i64) -> Result<ProposedRule, String> {
    let (kind, subject_id, target_id, state): (String, i64, Option<i64>, String) = conn
        .query_row(
            "SELECT kind, subject_id, target_id, state FROM proposal WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(|e| format!("looking up proposal {id}: {e}"))?;

    if state != STATE_OPEN {
        return Err(format!(
            "proposal {id} is `{state}`, not open — accept only applies to an open proposal"
        ));
    }
    if kind == KIND_ATTACH_CHAPTERS {
        return Err(format!("proposal {id}: {ATTACH_CHAPTERS_REFUSAL}"));
    }
    let Some(target_id) = target_id else {
        return Err(format!(
            "proposal {id} (kind `{kind}`) has no target series to write a rule against"
        ));
    };

    let subject_title = series_title(conn, subject_id)?;
    let target_title = series_title(conn, target_id)?;
    let (pattern, root_id) = common_directory_pattern(conn, id, &subject_title)?;

    let (set_edition_key, set_kind) = if kind == KIND_EDITION_OF {
        let Some(suffix) = subject_title.strip_prefix(&format!("{target_title} - ")) else {
            return Err(format!(
                "proposal {id}: \"{subject_title}\" no longer starts with \"{target_title} - \" \
                 — one of the two series was renamed since this proposal was raised, and \
                 accepting now would set only set_series, silently merging the colour volume's \
                 pages into the base edition. Dismiss this proposal and let the next raise \
                 propose it again against the current titles."
            ));
        };
        let slug = slugify(suffix);
        if slug.is_empty() {
            return Err(format!(
                "proposal {id}: the suffix \"{suffix}\" has no letters or digits to make an \
                 edition key from. Write the rule by hand."
            ));
        }
        let implied = matched_edition_word(suffix).map(implied_edition_kind);
        (Some(slug), implied.map(str::to_owned))
    } else {
        (None, None)
    };

    Ok(ProposedRule {
        root_id: Some(root_id),
        kind: "prefix".to_string(),
        pattern,
        match_on: "path".to_string(),
        set_series: Some(target_title.clone()),
        set_edition_key,
        set_kind,
        // Never derived. See `implied_edition_kind`'s own comment.
        set_language: None,
        note: format!(
            "accepted from proposal {id}: \"{subject_title}\" is \"{target_title}\""
        ),
    })
}
```

- [ ] **Step 5: Move `accept`'s body into `accept_with`**

Rename the existing `accept` to `accept_with`, taking the rule as a parameter.
Delete from its body everything `preview` now does — its own `query_row`, the
state guard, the `attach-chapters` refusal, the `target_id` check, the
`common_directory_pattern` call and the `set_edition_key` derivation — and have
what remains read `rule`'s fields instead of the locals it used to compute.

```rust
/// Accept proposal `id` by writing exactly `rule`.
///
/// **The guards are not about who chose the pattern.** `accept` derived its
/// own rule so that two measured failures could not happen — a pattern that
/// matched nothing, and a pattern that reached an unrelated series. Both are
/// properties of the *pattern*, so both are checked here regardless of where
/// it came from, and a widened pattern is checked harder rather than less.
///
/// # Errors
///
/// If `id` names no proposal, if it is not `open`, if it is
/// `attach-chapters`, if `rule` reaches a series that is neither the subject
/// nor the target — naming it — or if `rule` matches no `scan_entry` at all,
/// in which case it is rolled back rather than left deciding nothing.
pub fn accept_with(conn: &Connection, id: i64, rule: &ProposedRule) -> Result<i64, String> {
    // The `other_series_under_prefix` check and the I5 transaction exactly as
    // `accept` had them, reading `rule.pattern`, `rule.root_id`,
    // `rule.set_series`, `rule.set_edition_key`, `rule.set_kind`,
    // `rule.set_language`, `rule.kind`, `rule.match_on` and `rule.note`.
    //
    // The INSERT gains `set_kind` and `set_language`, which `accept` never
    // wrote and always left NULL.
}

/// Accept proposal `id` with the rule [`preview`] derives for it.
///
/// The narrow default: exactly what this function did before the split, and
/// what `taiyaku library proposals --accept` still calls.
///
/// # Errors
///
/// As [`preview`] and [`accept_with`].
pub fn accept(conn: &Connection, id: i64) -> Result<i64, String> {
    accept_with(conn, id, &preview(conn, id)?)
}
```

- [ ] **Step 5b: Add the `accept_with` tests**

```rust
#[test]
fn accept_with_writes_the_rule_it_was_handed_not_one_it_derived() {
    // §7 of C3. A server that re-derived would write something the preview
    // did not say; this is the test that would fail if anyone made it do so.
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);

    let mut rule = proposals::preview(&conn, proposal.id).expect("preview");
    rule.set_edition_key = Some("en-color".to_string());
    rule.set_language = Some("en".to_string());

    let rule_id = proposals::accept_with(&conn, proposal.id, &rule).expect("accept");

    let (key, language): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT set_edition_key, set_language FROM rule WHERE id = ?1",
            [rule_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("the written rule");
    assert_eq!(key.as_deref(), Some("en-color"));
    assert_eq!(language.as_deref(), Some("en"));
}

#[test]
fn accept_with_still_refuses_a_pattern_that_would_move_another_series() {
    // The measured `Publisher/` bug: a pattern that merely matches something
    // is not enough, and handing the rule in from outside must not become a
    // way around the check.
    let library = library_with_a_shared_publisher_directory();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    let mut rule = proposals::preview(&conn, proposal.id).expect("preview");
    rule.pattern = "Publisher/".to_string();

    let refused = proposals::accept_with(&conn, proposal.id, &rule);

    assert!(
        refused.is_err_and(|e| e.contains("would move that material too")),
        "a widened pattern that reaches an unrelated series is still refused",
    );
    let rules: i64 = conn
        .query_row("SELECT count(*) FROM rule WHERE note LIKE 'accepted%'", [], |r| r.get(0))
        .expect("count");
    assert_eq!(rules, 0, "a refused accept leaves no rule behind");
}
```

Add the fixture beside the existing ones:

```rust
/// A root laid out `Publisher/<series>/<series> - v01/`, holding the two
/// related series and one unrelated third — the shape that made a
/// correctly-derived `Publisher/` pattern move somebody else's volumes into
/// the accepted series' page sequence.
fn library_with_a_shared_publisher_directory() -> Library {
    let dir = crate::tests::tempdir("proposals-shared-publisher");
    for (n, title) in (2_usize..).zip([
        "Go-Toubun no Hanayome",
        "Go-Toubun no Hanayome - Full Color Edition",
        "Kaguya-sama wa Kokurasetai",
    ]) {
        write_pages(
            &dir.path
                .join("Publisher")
                .join(title)
                .join(format!("{title} - v01")),
            n,
        );
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &dir.path).expect("add root");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("derive");
    drop(conn);
    library
}
```

- [ ] **Step 6: Run the whole module suite**

Run: `cargo test -p taiyaku-library proposals::`

Expected: PASS, including every pre-existing test. **They are the evidence
this was a refactor** — a green suite here means `accept` decides exactly what
it decided before, and a red one means the split changed behaviour.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-library/src/proposals.rs crates/taiyaku-library/src/proposals/tests.rs
git commit -m "proposals: preview the rule an accept would write, and write the one you were handed"
```

---

### Task 2: `resolves` — the dry run, and the guard generalised to regex

The task the whole widening rests on. `Resolution` is one computation read two
ways: `also_resolves` is the form's *"resolves 6 more"*, `collateral` is the
refusal.

**Files:**
- Modify: `crates/taiyaku-library/src/rules.rs` (split `Engine::load`)
- Modify: `crates/taiyaku-library/src/proposals.rs`
- Test: `crates/taiyaku-library/src/proposals/tests.rs`

**Interfaces:**
- Consumes: `ProposedRule`, `accept_with` (Task 1).
- Produces: `pub struct Resolution { also_resolves: Vec<i64>, collateral: Vec<String>, works_moved: usize }`;
  `pub fn resolves(conn: &Connection, id: i64, rule: &ProposedRule) -> Result<Resolution, String>`.
  `accept_with` gains group behaviour; `unaccept` reopens the whole group.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_widened_rule_reports_the_other_proposals_it_would_resolve() {
    // C3 §3: "the moment a person can answer 37 questions once instead of 37
    // times". The count has to come from the same computation that refuses
    // collateral, or the form would promise something the write disagrees with.
    let library = library_with_three_colour_editions();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let first = proposals::open(&conn, 0.0).expect("open").remove(0);
    let mut rule = proposals::preview(&conn, first.id).expect("preview");
    rule.kind = "regex".to_string();
    rule.pattern = "^(?<series>.+) - Full Color Edition/".to_string();
    rule.root_id = None;

    let resolution = proposals::resolves(&conn, first.id, &rule).expect("resolves");

    assert_eq!(
        resolution.also_resolves.len(),
        2,
        "the other two colour editions, not counting this proposal itself",
    );
    assert!(
        resolution.collateral.is_empty(),
        "every series this reaches is the subject of a proposal: {:?}",
        resolution.collateral,
    );
}

#[test]
fn a_widened_rule_that_reaches_a_series_no_proposal_names_reports_it_as_collateral() {
    let library = library_with_a_shared_publisher_directory();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    let mut rule = proposals::preview(&conn, proposal.id).expect("preview");
    rule.pattern = "Publisher/".to_string();

    let resolution = proposals::resolves(&conn, proposal.id, &rule).expect("resolves");

    assert!(
        resolution
            .collateral
            .iter()
            .any(|s| s.contains("Kaguya-sama")),
        "the unrelated third series is named: {:?}",
        resolution.collateral,
    );
}

#[test]
fn a_regex_rule_s_collateral_is_computed_with_regex_semantics() {
    // Task 1 refused non-`prefix` rules outright, because
    // `other_series_under_prefix` would have prefix-matched the regex's own
    // source text, found nothing, and reported a clean bill of health. This is
    // the test that proves `resolves` did not inherit that no-op: the same
    // reach, expressed as a regex, must name the same collateral.
    let library = library_with_a_shared_publisher_directory();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    let mut rule = proposals::preview(&conn, proposal.id).expect("preview");
    rule.kind = "regex".to_string();
    rule.pattern = "^Publisher/(?<series>.+)/".to_string();
    rule.root_id = None;

    let resolution = proposals::resolves(&conn, proposal.id, &rule).expect("resolves");

    assert!(
        resolution
            .collateral
            .iter()
            .any(|s| s.contains("Kaguya-sama")),
        "a regex reaching the unrelated third series names it: {:?}",
        resolution.collateral,
    );
    assert!(
        proposals::accept_with(&conn, proposal.id, &rule).is_err(),
        "and the accept is refused, not merely warned about",
    );
}

#[test]
fn accepting_a_widened_rule_accepts_every_proposal_it_resolves() {
    let library = library_with_three_colour_editions();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let first = proposals::open(&conn, 0.0).expect("open").remove(0);
    let mut rule = proposals::preview(&conn, first.id).expect("preview");
    rule.kind = "regex".to_string();
    rule.pattern = "^(?<series>.+) - Full Color Edition/".to_string();
    rule.root_id = None;

    let rule_id = proposals::accept_with(&conn, first.id, &rule).expect("accept");

    let still_open = proposals::open(&conn, 0.0).expect("open");
    assert!(
        !still_open.iter().any(|p| p.kind == "edition-of"),
        "one accept answered all three: {still_open:?}",
    );
    let pointing: i64 = conn
        .query_row(
            "SELECT count(*) FROM proposal WHERE created_rule_id = ?1",
            [rule_id],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(pointing, 3, "each records the rule that answered it");
}

#[test]
fn un_accepting_one_of_a_widened_group_reopens_all_of_them() {
    // Without this, deleting the rule leaves the other two `accepted` and
    // pointing at nothing — the exact state `created_rule_id` exists to
    // prevent, reached by a road that did not exist before widening.
    let library = library_with_three_colour_editions();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let first = proposals::open(&conn, 0.0).expect("open").remove(0);
    let mut rule = proposals::preview(&conn, first.id).expect("preview");
    rule.kind = "regex".to_string();
    rule.pattern = "^(?<series>.+) - Full Color Edition/".to_string();
    rule.root_id = None;
    proposals::accept_with(&conn, first.id, &rule).expect("accept");

    proposals::unaccept(&conn, first.id).expect("unaccept");

    let reopened = proposals::open(&conn, 0.0).expect("open");
    assert_eq!(
        reopened.iter().filter(|p| p.kind == "edition-of").count(),
        3,
        "all three are answerable again",
    );
    let orphans: i64 = conn
        .query_row(
            "SELECT count(*) FROM proposal WHERE state = 'accepted'",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(orphans, 0, "none is left accepted against a deleted rule");
}
```

Add the fixture:

```rust
/// Three series each with a `- Full Color Edition` sibling — the shape that
/// makes one widened rule worth writing instead of three narrow ones.
fn library_with_three_colour_editions() -> Library {
    let dir = crate::tests::tempdir("proposals-three-colour");
    let mut n = 2_usize;
    for base in [
        "Go-Toubun no Hanayome",
        "Kaguya-sama wa Kokurasetai",
        "Kobayashi-san Chi no Meidoragon",
    ] {
        for title in [base.to_string(), format!("{base} - Full Color Edition")] {
            write_pages(&dir.path.join(&title).join(format!("{title} - v01")), n);
            n += 1;
        }
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &dir.path).expect("add root");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("derive");
    drop(conn);
    library
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-library proposals::tests::a_widened`

Expected: FAIL to compile — `no function named `resolves``.

- [ ] **Step 3: Add `Resolution` and `resolves`**

```rust
/// What a candidate rule would do, before it does it.
///
/// **One computation, read two ways.** `also_resolves` is the form's
/// *"resolves 6 more"*; `collateral` is the measured refusal that stops a
/// `Publisher/` pattern moving somebody else's volumes. They are the same
/// query because the question is the same one: which series does this rule
/// decide for, and which of those did anybody ask about.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolution {
    /// Other open proposals this rule would answer. Never includes `id`.
    pub also_resolves: Vec<i64>,
    /// Series this rule decides for that no proposal named. Non-empty is a
    /// refusal, not a warning.
    pub collateral: Vec<String>,
    /// How many `work` rows would change series, for the consequence line.
    pub works_moved: usize,
}

/// What `rule` would resolve if accepted for proposal `id`.
///
/// Built on [`crate::rules::Engine`] rather than on a second matcher: the
/// engine is what `derive` will actually use, so a dry run that agreed with a
/// reimplementation and disagreed with the engine would be worse than none.
///
/// # Errors
///
/// If `id` names no proposal, if the rule is not a valid `regex` when it says
/// it is, or if the reads against `scan_entry`, `rule` or `series` fail.
pub fn resolves(
    conn: &Connection,
    id: i64,
    rule: &ProposedRule,
) -> Result<Resolution, String> {
    let (subject_id, target_id): (i64, Option<i64>) = conn
        .query_row(
            "SELECT subject_id, target_id FROM proposal WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| format!("looking up proposal {id}: {e}"))?;

    // Every series this rule would set, by resolving every scan entry through
    // the live engine with the candidate appended.
    let reached = series_reached_by(conn, rule)?;

    // The legitimate set: this proposal's two ends, plus the subject of every
    // other open proposal the same rule answers.
    let mut also_resolves = Vec::new();
    let mut allowed: BTreeMap<i64, String> = BTreeMap::new();
    allowed.insert(subject_id, series_title(conn, subject_id)?);
    if let Some(target_id) = target_id {
        allowed.insert(target_id, series_title(conn, target_id)?);
    }
    for other in open(conn, 0.0)? {
        if other.id == id {
            continue;
        }
        if reached.contains_key(&other.subject_id) {
            also_resolves.push(other.id);
            allowed.insert(other.subject_id, series_title(conn, other.subject_id)?);
            if let Some(other_target) = other.target_id {
                allowed.insert(other_target, series_title(conn, other_target)?);
            }
        }
    }

    let collateral: Vec<String> = reached
        .iter()
        .filter(|(series_id, _)| !allowed.contains_key(series_id))
        .map(|(_, title)| title.clone())
        .collect();
    let works_moved = works_under(conn, reached.keys().copied())?;

    Ok(Resolution {
        also_resolves,
        collateral,
        works_moved,
    })
}
```

**First, split `rules::Engine::load` so a candidate rule can join the live
set.** `load` today reads the rows *and* compiles them in one function, so
there is no way to obtain `Vec<Rule>` and add to it. Extract the reading half —
this is a refactor, and `load`'s existing callers (`derive.rs:215`, `1292`,
`1361`) must keep working unchanged:

```rust
/// Every rule in the database, in `ordinal, id` order, with the complaints
/// raised by rows whose `kind` or `match_on` would not parse.
///
/// Split out of [`Engine::load`] so a caller can add a rule that is not in
/// the table yet — `proposals::resolves` asks what a *candidate* rule would
/// decide, and the only honest way to answer is to put it in the set the real
/// engine compiles.
///
/// # Errors
///
/// Only if SQLite refuses the query. A row that will not parse is a
/// complaint, exactly as it was inside `load`.
pub fn read(conn: &Connection) -> Result<(Vec<Rule>, Vec<String>), String> {
    // …the body of `load` from its `prepare` down to the end of the row loop,
    // returning `(rules, complaints)` instead of compiling.
}
```

and reduce `load` to:

```rust
pub fn load(conn: &Connection) -> Result<(Self, Vec<String>), String> {
    let (rules, mut complaints) = read(conn)?;
    let (engine, compile_complaints) = Self::compile(&rules);
    complaints.extend(compile_complaints);
    Ok((engine, complaints))
}
```

Then add the two helpers `resolves` needs, beside `common_directory_pattern`:

```rust
/// Every series `rule` would decide for, as `(series_id, sort_title)`.
///
/// **The candidate joins the live rule set and the real engine compiles it.**
/// A dry run built on a second matcher could agree with itself and disagree
/// with `derive`, which is the one thing it must not do: this answer is what
/// `accept_with` refuses on and what the form promises.
fn series_reached_by(
    conn: &Connection,
    rule: &ProposedRule,
) -> Result<BTreeMap<i64, String>, String> {
    let (mut rules, _complaints) = crate::rules::read(conn)?;
    rules.push(crate::rules::Rule {
        // Not a row in the table. `0` is the id no rule has.
        id: 0,
        // Above every existing rule, which is where `accept_with` writes it —
        // a dry run that resolved the candidate at a different ordinal than
        // the accept would use is answering a different question.
        ordinal: i64::MAX,
        enabled: true,
        root_id: rule.root_id,
        kind: rule.kind.parse()?,
        pattern: rule.pattern.clone(),
        match_on: rule.match_on.parse()?,
        set_edition_key: rule.set_edition_key.clone(),
        set_kind: rule.set_kind.clone(),
        set_language: rule.set_language.clone(),
        set_series: rule.set_series.clone(),
        note: Some(rule.note.clone()),
    });
    // **A pattern that will not compile is an error here, not a complaint.**
    // `Engine::compile` disables such a rule and carries on, which is right
    // for a scan — one bad rule must not kill a library. It is wrong for a
    // dry run: "resolves nothing" and "your regex has a typo" would render
    // identically in the form, and a person would widen a pattern that does
    // nothing and accept it. Checked here, on the candidate alone, rather
    // than by reading `compile`'s complaint list back and matching the
    // candidate's sentinel id out of it — that would couple this to a message
    // format, and this way the caller gets `regex`'s own error.
    if rule.kind == "regex" {
        regex::Regex::new(&rule.pattern)
            .map_err(|e| format!("this pattern will not compile: {e}"))?;
    }
    let (engine, _complaints) = crate::rules::Engine::compile(&rules);

    let roots = crate::scan::roots(conn)?;
    let mut reached = BTreeMap::new();
    for root in &roots {
        let mut stmt = conn
            .prepare("SELECT rel_path FROM scan_entry WHERE root_id = ?1")
            .map_err(|e| format!("reading scan entries for the dry run: {e}"))?;
        let rows = stmt
            .query_map([root.id], |r| r.get::<_, String>(0))
            .map_err(|e| format!("reading scan entries for the dry run: {e}"))?;
        for row in rows {
            let rel_path = row.map_err(|e| format!("reading a scan entry: {e}"))?;
            let Some(series) = engine.resolve(root, &rel_path).series else {
                continue;
            };
            if let Some(series_id) = series_id_by_sort_title(conn, &series)? {
                reached.insert(series_id, series);
            }
        }
    }
    Ok(reached)
}

/// How many `work` rows sit under these series.
fn works_under(
    conn: &Connection,
    series: impl Iterator<Item = i64>,
) -> Result<usize, String> {
    let mut total = 0_usize;
    for series_id in series {
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM work WHERE series_id = ?1",
                [series_id],
                |r| r.get(0),
            )
            .map_err(|e| format!("counting works in series {series_id}: {e}"))?;
        total += usize::try_from(n).unwrap_or(0);
    }
    Ok(total)
}
```

> **Implementer notes, all three verified against the current source.**
>
> 1. `Engine::load` is `pub fn load(conn: &Connection) -> Result<(Self,
>    Vec<String>), String>` and `Engine::resolve` is
>    `pub fn resolve(&self, root: &Root, rel_path: &str) -> Resolved` — it
>    takes a **`&Root`**, not a `root_id`. `crate::scan::roots(conn)` returns
>    `Vec<Root>`. This is exactly the loop `derive.rs:215-231` runs; read it
>    and follow it.
> 2. `rules::Rule` has a **`note: Option<String>`** field. `RuleKind` and
>    `MatchOn` both implement `FromStr`, so `.parse()` works, and their
>    `Err` is a `String` — check that `?` converts cleanly and map it if not.
> 3. `Engine::compile` only ever complains about a `RuleKind::Regex` whose
>    pattern fails `Regex::new`, and its complaints do carry the rule id
>    (`"rule {id}: pattern {:?} does not compile: {e}"`). The code above
>    deliberately does not read them back: validating the candidate directly
>    gives the same answer without coupling to that string. `regex` is already
>    a dependency of this crate — `rules.rs` uses it — so no manifest change is
>    needed. Confirm that before assuming it.
> 4. `proposals.rs` already contains `SELECT id FROM series WHERE sort_title =
>    ?1` around line 874. Reuse or extract that as
>    `series_id_by_sort_title`; do not add a second copy.

- [ ] **Step 4: Make `accept_with` refuse on `collateral` and accept the group**

**This step is what makes a widened rule possible at all.** Task 1 left
`accept_with` with **two** fail-closed refusals, both because
`other_series_under_prefix` can only reason about a literal path prefix under a
single root:

1. a rule whose `root_id` is `None` is refused — the guard has no root to scope
   to;
2. a rule whose `kind`/`match_on` is anything but `prefix`/`path` is refused —
   the guard would prefix-match a regex's *source text* and silently find
   nothing, which is a no-op wearing a guard's clothes.

A widened rule is exactly an every-root `regex` rule, so it trips both. **Both
refusals are replaced here, not deleted** — `resolves` is built on
`rules::Engine`, so it evaluates the candidate with the same semantics `derive`
will, whatever its `kind` and `match_on`, and across every root. That is what
earns the right to remove them. Delete both refusals and the whole
`other_series_under_prefix` block together, and put in their place:

```rust
    let resolution = resolves(conn, id, rule)?;
    if !resolution.collateral.is_empty() {
        return Err(format!(
            "proposal {id}'s rule also decides for {}, so accepting it would move that \
             material too. Narrow the pattern or write the rule by hand.",
            resolution.collateral.join(", ")
        ));
    }
```

and, inside the transaction after the rule is written and this proposal is
marked, mark the rest of the group:

```rust
    // **A widened rule answers every proposal it resolves, and each records
    // it.** Leaving them `open` would show a person questions the library has
    // already decided; leaving them `accepted` with a NULL `created_rule_id`
    // would make them un-un-acceptable.
    for other in &resolution.also_resolves {
        tx.execute(
            "UPDATE proposal SET state = ?1, created_rule_id = ?2 WHERE id = ?3",
            rusqlite::params![STATE_ACCEPTED, rule_id, other],
        )
        .map_err(|e| format!("recording proposal {other} against rule {rule_id}: {e}"))?;
    }
```

- [ ] **Step 5: Make `unaccept` reopen the whole group**

Replace its single-row `UPDATE` with one keyed on the rule:

```rust
    // **Keyed on the rule, not on the id.** One rule can answer many
    // proposals, and un-accepting is the undo of the accept that wrote it —
    // so it reopens everything that accept closed, not only the row the
    // caller happened to name.
    conn.execute(
        "UPDATE proposal SET state = ?1, created_rule_id = NULL WHERE created_rule_id = ?2",
        rusqlite::params![STATE_OPEN, rule_id],
    )
    .map_err(|e| format!("reopening the proposals rule {rule_id} answered: {e}"))?;
```

Read the `created_rule_id` **before** deleting the rule — `ON DELETE SET NULL`
clears it, so a delete-then-update finds nothing.

- [ ] **Step 6: Run the module suite**

Run: `cargo test -p taiyaku-library proposals::`

Expected: PASS, all of it. If
`accepting_writes_a_rule_and_un_accepting_removes_it` fails, the ordering in
Step 5 is wrong.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-library/src/proposals.rs crates/taiyaku-library/src/proposals/tests.rs
git commit -m "proposals: a dry run that answers what a widened rule would resolve"
```

---

### Task 3: `list`, and the counts the filter tabs read

**Files:**
- Modify: `crates/taiyaku-library/src/proposals.rs`
- Test: `crates/taiyaku-library/src/proposals/tests.rs`

**Interfaces:**
- Consumes: `Proposal` (existing).
- Produces: `pub struct Filter { state: Option<String>, kind: Option<String>, min_score: f64 }`;
  `pub fn list(conn: &Connection, filter: &Filter) -> Result<Vec<Proposal>, String>`;
  `pub fn counts_by_kind(conn: &Connection, state: &str) -> Result<BTreeMap<String, usize>, String>`.
  `open(conn, min_score)` stays, delegating to `list`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn list_filters_by_state_and_kind_and_keeps_open_as_it_was() {
    // C3 §4: accepted and dismissed proposals stay listed under their own
    // filters — "a dismissal you disagree with later is a thing you have to be
    // able to find".
    let library = library_with_three_colour_editions();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let first = proposals::open(&conn, 0.0).expect("open").remove(0);
    proposals::dismiss(&conn, first.id).expect("dismiss");

    let dismissed = proposals::list(
        &conn,
        &proposals::Filter {
            state: Some("dismissed".to_string()),
            kind: None,
            min_score: 0.0,
        },
    )
    .expect("list");
    assert_eq!(dismissed.len(), 1);
    assert_eq!(dismissed[0].id, first.id);

    let editions = proposals::list(
        &conn,
        &proposals::Filter {
            state: Some("open".to_string()),
            kind: Some("edition-of".to_string()),
            min_score: 0.0,
        },
    )
    .expect("list");
    assert!(editions.iter().all(|p| p.kind == "edition-of"));
    assert!(!editions.iter().any(|p| p.id == first.id));
}

#[test]
fn counts_by_kind_feeds_the_filter_tabs() {
    let library = library_with_three_colour_editions();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");

    let counts = proposals::counts_by_kind(&conn, "open").expect("counts");

    assert_eq!(counts.get("edition-of").copied(), Some(3));
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p taiyaku-library proposals::tests::list_filters`

Expected: FAIL to compile — `no function named `list``.

- [ ] **Step 3: Implement**

```rust
/// What the inbox is asking for. C3 §7's query string.
#[derive(Debug, Clone)]
pub struct Filter {
    /// `open` | `accepted` | `dismissed`; `None` is every state.
    pub state: Option<String>,
    /// `edition-of` | `same-series` | `attach-chapters` | `align`; `None` is
    /// every kind.
    pub kind: Option<String>,
    /// §6's threshold. The scale is `{0, 1, 2, 3}` — see [`SIGNAL`] and
    /// [`CERTAIN`] — so this is a choice among four values, not a percentile.
    pub min_score: f64,
}

/// Proposals matching `filter`, highest score first.
///
/// # Errors
///
/// If the query against `proposal` fails.
pub fn list(conn: &Connection, filter: &Filter) -> Result<Vec<Proposal>, String> {
    let mut sql = String::from(
        "SELECT id, kind, subject_id, target_id, score, reason, state, created_rule_id \
         FROM proposal WHERE score >= ?1",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(filter.min_score)];
    if let Some(state) = &filter.state {
        params.push(Box::new(state.clone()));
        let _ = write!(sql, " AND state = ?{}", params.len());
    }
    if let Some(kind) = &filter.kind {
        params.push(Box::new(kind.clone()));
        let _ = write!(sql, " AND kind = ?{}", params.len());
    }
    sql.push_str(" ORDER BY score DESC, id");

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("listing proposals: {e}"))?;
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(AsRef::as_ref).collect();
    let rows = stmt
        .query_map(refs.as_slice(), |r| {
            Ok(Proposal {
                id: r.get(0)?,
                kind: r.get(1)?,
                subject_id: r.get(2)?,
                target_id: r.get(3)?,
                score: r.get(4)?,
                reason: r.get(5)?,
                state: r.get(6)?,
                created_rule_id: r.get(7)?,
            })
        })
        .map_err(|e| format!("listing proposals: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("listing proposals: {e}"))
}

/// How many proposals of each kind sit in `state`, for the inbox's tabs.
///
/// # Errors
///
/// If the query against `proposal` fails.
pub fn counts_by_kind(
    conn: &Connection,
    state: &str,
) -> Result<BTreeMap<String, usize>, String> {
    let mut stmt = conn
        .prepare("SELECT kind, count(*) FROM proposal WHERE state = ?1 GROUP BY kind")
        .map_err(|e| format!("counting proposals: {e}"))?;
    let rows = stmt
        .query_map([state], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })
        .map_err(|e| format!("counting proposals: {e}"))?;
    let mut counts = BTreeMap::new();
    for row in rows {
        let (kind, n) = row.map_err(|e| format!("counting proposals: {e}"))?;
        counts.insert(kind, usize::try_from(n).unwrap_or(0));
    }
    Ok(counts)
}
```

Reimplement `open` over it, so there is one query:

```rust
pub fn open(conn: &Connection, min_score: f64) -> Result<Vec<Proposal>, String> {
    list(
        conn,
        &Filter {
            state: Some(STATE_OPEN.to_string()),
            kind: None,
            min_score,
        },
    )
}
```

`use std::fmt::Write as _;` goes at the top of the module for the `write!`
calls.

- [ ] **Step 4: Run the module suite**

Run: `cargo test -p taiyaku-library proposals::`

Expected: PASS. Every existing `open` test is now also a test of `list`.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-library/src/proposals.rs crates/taiyaku-library/src/proposals/tests.rs
git commit -m "proposals: list by state and kind, and count by kind"
```

---

### Task 4: `evidence` — what the expanded row shows

**Files:**
- Modify: `crates/taiyaku-library/src/proposals.rs`
- Test: `crates/taiyaku-library/src/proposals/tests.rs`

**Interfaces:**
- Consumes: `Proposal`, `ProposedRule`, `preview`.
- Produces:
  `pub struct FirstVolume { work_slug: String, edition_key: String, page_count: i64, formats: Vec<String>, interior: Vec<i64> }`;
  `pub struct Side { series_id: i64, title: String, directories: Vec<String>, volume_count: usize, volumes: Vec<f64>, root_name: Option<String>, first_volume: Option<FirstVolume> }`;
  `pub struct Reason { text: String, sign: char }`;
  `pub struct Evidence { proposal: Proposal, subject: Side, target: Option<Side>, reasons: Vec<Reason>, rule: Option<ProposedRule> }`;
  `pub fn evidence(conn: &Connection, id: i64) -> Result<Evidence, String>`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn evidence_carries_both_sides_and_degrades_rather_than_breaking() {
    // C3 §8: "every evidence field renders when present and degrades when
    // absent — a proposal whose subject has no interior pages yet must not
    // render a broken panel". The store's half of that is never handing the
    // client an index it cannot fetch.
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);

    let ev = proposals::evidence(&conn, proposal.id).expect("evidence");

    assert_eq!(ev.proposal.id, proposal.id);
    assert!(!ev.subject.path.is_empty(), "the path is shown verbatim");
    assert!(ev.target.is_some(), "an edition-of proposal has two sides");
    let first = ev.subject.first_volume.expect("volume 1");
    assert!(
        first.interior.iter().all(|i| *i < first.page_count),
        "no interior index past the end: {:?} of {}",
        first.interior,
        first.page_count,
    );
    assert!(
        first.interior.iter().all(|i| *i > 0),
        "the cover is shown separately, so interiors skip page 0",
    );
    assert!(ev.rule.is_some(), "an open, previewable proposal carries its rule");
}

#[test]
fn evidence_for_an_attach_chapters_proposal_carries_no_rule() {
    // `preview` refuses that kind by name. The evidence route must still
    // answer — the row is listed and expandable, just not acceptable — so the
    // refusal becomes an absent rule rather than a failed request.
    let library = library_with_unattached_chapters();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "attach-chapters")
        .expect("an attach-chapters proposal");

    let ev = proposals::evidence(&conn, proposal.id).expect("evidence");

    assert!(ev.rule.is_none());
}
```

> **Implementer note.** `library_with_unattached_chapters` may already exist in
> `proposals/tests.rs` under another name — the module already tests
> `attach-chapters` raising. Reuse it rather than adding a second fixture.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p taiyaku-library proposals::tests::evidence`

Expected: FAIL to compile — `no function named `evidence``.

- [ ] **Step 3: Implement**

```rust
/// Volume 1 of a side, and the pages the evidence panel draws from it.
#[derive(Debug, Clone, PartialEq)]
pub struct FirstVolume {
    pub work_slug: String,
    pub edition_key: String,
    pub page_count: i64,
    /// Every distinct container token across this edition's members, sorted.
    ///
    /// **From `scan_entry.format`, and a list.** `member.format` is a column
    /// nothing in this repository writes — the only `INSERT INTO member` omits
    /// it — so the obvious source is always `NULL`, and a permanently blank
    /// field is indistinguishable from an honest §8 degradation. Edition-wide
    /// like `page_count`, because an edition can hold a directory and a `.cbz`
    /// at once and naming one of them would describe the other wrongly.
    pub formats: Vec<String>,
    /// Two interior page indices, or fewer for a short member.
    ///
    /// **Fractions of the page count, not fixed numbers.** C3 §2's argument is
    /// that a colour edition's *cover* is a different image while an interior
    /// page is the same panel layout — so the indices have to land inside
    /// every member they are asked for. The mockup's `p012`/`p047` are
    /// illustrative; hard-coding them renders a broken panel for a three-page
    /// member, which §8 forbids.
    pub interior: Vec<i64>,
}

/// One end of a proposal, as the expanded row draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct Side {
    pub series_id: i64,
    pub title: String,
    /// The directories this side's material sits in, absolute and sorted.
    ///
    /// **Data, not a sentence.** Empty means nothing has been scanned for this
    /// side and nothing else; one entry is the ordinary case; several is a
    /// side whose material spans directories, which is a fact worth showing
    /// rather than an error. An earlier version rendered `"{root} (2
    /// directories)"` here — prose in a field the client draws inside a
    /// `<code>`, which reads as a path that does not exist and cannot be
    /// copied, and which the client could only use by parsing it back apart.
    pub directories: Vec<String>,
    pub volume_count: usize,
    pub volumes: Vec<f64>,
    pub root_name: Option<String>,
    pub first_volume: Option<FirstVolume>,
}

/// One scored signal, as the expanded row lists it.
///
/// **Structured rather than split out of `Proposal::reason`.** That field is
/// one human sentence built for a CLI line, and a client that split it on
/// punctuation would break the moment the wording changed. The mockup shows
/// each signal on its own line with its sign, which is a different rendering
/// of the same facts rather than a reformatting of the same string.
#[derive(Debug, Clone, PartialEq)]
pub struct Reason {
    pub text: String,
    /// `+` a signal for, `-` against, `?` a fact stated without a sign — the
    /// mockup's *"target is itself an edition of Monster — accepting nests"*.
    pub sign: char,
}

/// Everything the expanded row shows for one proposal.
#[derive(Debug, Clone, PartialEq)]
pub struct Evidence {
    pub proposal: Proposal,
    pub subject: Side,
    /// Absent for a kind that names only a subject.
    pub target: Option<Side>,
    /// The scored signals, each with its sign. C3 §2.
    pub reasons: Vec<Reason>,
    /// The rule an accept would write, or `None` when this proposal cannot be
    /// accepted — `attach-chapters`, or a target renamed since it was raised.
    /// The row still expands; only the accept control is absent.
    pub rule: Option<ProposedRule>,
}

/// The expanded evidence for proposal `id`.
///
/// # Errors
///
/// If `id` names no proposal, or if the reads against `series`, `work`,
/// `edition`, `member` or `root` fail. A proposal that cannot be previewed is
/// **not** an error: it answers with `rule: None`.
pub fn evidence(conn: &Connection, id: i64) -> Result<Evidence, String> {
    let proposal = one(conn, id)?;
    let subject = side(conn, proposal.subject_id)?;
    let target = match proposal.target_id {
        Some(target_id) => Some(side(conn, target_id)?),
        None => None,
    };
    let reasons = scored_reasons(conn, &proposal)?;
    Ok(Evidence {
        proposal,
        subject,
        target,
        reasons,
        rule: preview(conn, id).ok(),
    })
}

/// Two interior indices inside a member of `page_count` pages.
///
/// Deterministic, so the same two pages are compared every time; and skipping
/// 0, which the cover already shows.
fn interior_indices(page_count: i64) -> Vec<i64> {
    if page_count < 2 {
        return Vec::new();
    }
    let mut picked: Vec<i64> = [page_count / 4, page_count * 3 / 5]
        .into_iter()
        .map(|i| i.clamp(1, page_count - 1))
        .collect();
    picked.dedup();
    picked
}
```

Write `one(conn, id) -> Result<Proposal, String>` (a single-row `SELECT` with
the same column list `list` uses), `side(conn, series_id) -> Result<Side,
String>`, and `scored_reasons(conn, &Proposal) -> Result<Vec<Reason>, String>`
beside them.

**`scored_reasons` must share its facts with `raise_edition_of`, not restate
them.** That function already computes `vocab_hit` and `volumes_match` and
formats them into one sentence. Factor the pair out into a small
`fn edition_of_signals(conn, subject_id, target_id) -> Result<(Option<&'static
str>, bool, i64, i64), String>` that both call — `raise_edition_of` to build
its `reason` string and score, `scored_reasons` to build the signed list. Two
independent computations of the same signals would drift, and the drift would
show as an expanded row disagreeing with the score beside it. `side` reads the series title, its `work` rows for
`volumes` and `volume_count`, the lowest-numbered volume's hub edition for
`first_volume`, and `root.path`'s final component for `root_name`.

**`directories` must not go through `common_directory_pattern`.** That function
*errors* when a series' entries span more than one directory — correctly, since
an accept then cannot be expressed as one prefix rule — but evidence is shown
for both sides of a proposal and the *target* is never required to be
single-directory. Reusing it would fail the whole evidence request for a
perfectly ordinary target. Go to its source instead:
`derive::rel_paths_for_series(conn, title)` returns `Vec<(root_id, rel_path)>`;
take `derive::parent_of` of each, join each onto its root, sort, and hand the
list over. A side that spans two directories is a fact worth showing, not an
error — and the client, not the store, decides how to show it.

- [ ] **Step 4: Run the module suite**

Run: `cargo test -p taiyaku-library proposals::`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-library/src/proposals.rs crates/taiyaku-library/src/proposals/tests.rs
git commit -m "proposals: the evidence an expanded row shows"
```

---

### Task 5: `curation.rs` — six handlers, six routes, six grant rows

**Files:**
- Create: `crates/taiyaku-serve/src/curation.rs`
- Create: `crates/taiyaku-serve/src/curation/tests.rs`
- Modify: `crates/taiyaku-serve/src/lib.rs` (declare `mod curation;`)
- Modify: `crates/taiyaku-serve/src/routes.rs` (registrations + grant table)

**Interfaces:**
- Consumes: `proposals::{list, counts_by_kind, evidence, preview, resolves, accept_with, unaccept, dismiss, Filter, ProposedRule}` (Tasks 1–4).
- Produces: the six routes below. JSON field names are the contract `curation.ts` (Task 6) reads.

```
GET    /api/library/proposals?state=&kind=&min_score=
         -> { proposals: [ {id,kind,subject_id,target_id,score,reason,state,created_rule_id} ],
              counts: { "edition-of": 29, ... } }
GET    /api/library/proposals/{id}
         -> { proposal: {...}, subject: Side, target: Side|null,
               reasons: [{text,sign}], rule: Rule|null }
POST   /api/library/proposals/{id}/resolution   body: Rule
         -> { also_resolves: [id], collateral: [title], targets_reached: [title],
              entries_matched: n, subject_reached: bool, works_moved: n }
POST   /api/library/proposals/{id}/accept       body: Rule  -> { rule_id: n }
DELETE /api/library/proposals/{id}/accept                   -> 204
POST   /api/library/proposals/{id}/dismiss                  -> 204
```

`Side` is `{ series_id, title, directories, volume_count, volumes, root_name, first_volume }`,
`first_volume` is `{ work_slug, edition_key, page_count, formats, interior }` or `null`,
`Rule` is `{ root_id, kind, pattern, match_on, set_series, set_edition_key, set_kind, set_language, note }`.

- [ ] **Step 1: Write the failing tests**

**First promote the helpers rather than copying them.** `get` and `get_json`
are private to `library_api/tests.rs`, and `get_json`'s own comment records why
its non-200 panic is load-bearing — *"a helper that silently returned `null`
from a 401 would be a test asserting nothing"*. Two copies would be two places
for that to be forgotten, which is the same argument `api.ts` makes for
exporting `getJson` instead of re-wrapping `fetch`. Move both into
`crates/taiyaku-serve/src/test_support.rs` as `pub(crate)`, add `send_json`
beside them, and change `library_api/tests.rs` to use them from there — its
tests must pass unchanged, which is the proof the move was a move.

Then, in `crates/taiyaku-serve/src/curation/tests.rs`:

```rust
#[tokio::test]
async fn the_inbox_lists_proposals_and_their_counts() {
    let (router, state, _fixture) = test_support::router_over_a_scanned_work_in_a_series();
    {
        let conn = state.library.conn();
        taiyaku_library::proposals::raise(&conn).expect("raise");
    }

    let body = get_json(&router, "/api/library/proposals?state=open&min_score=0").await;

    assert!(body["proposals"].is_array());
    assert!(body["counts"].is_object());
}

#[tokio::test]
async fn a_reader_is_refused_the_inbox() {
    // C3 §7: everything here is admin-gated per A2. A 403 rather than a 404 —
    // the caller is who they say they are, and there is nothing to
    // re-authenticate.
    let (router, _state, _fixture) = test_support::router_over_a_scanned_work_as_a_reader();

    let response = get(&router, "/api/library/proposals").await;

    assert_eq!(response.status(), axum::http::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn accept_writes_the_rule_in_the_body() {
    let (router, state, _fixture) = test_support::router_over_two_suffixed_series();
    let id = {
        let conn = state.library.conn();
        taiyaku_library::proposals::raise(&conn).expect("raise");
        taiyaku_library::proposals::open(&conn, 0.0).expect("open")[0].id
    };

    let detail = get_json(&router, &format!("/api/library/proposals/{id}")).await;
    let mut rule = detail["rule"].clone();
    rule["set_edition_key"] = serde_json::json!("en-color");

    let response = send_json(
        &router,
        axum::http::Method::POST,
        &format!("/api/library/proposals/{id}/accept"),
        &serde_json::json!(rule),
    )
    .await;
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let conn = state.library.conn();
    let key: Option<String> = conn
        .query_row(
            "SELECT set_edition_key FROM rule ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .expect("the written rule");
    assert_eq!(key.as_deref(), Some("en-color"), "the server wrote what it was shown");
}

#[tokio::test]
async fn a_rule_that_would_move_an_unrelated_series_is_refused_with_its_name() {
    let (router, state, _fixture) = test_support::router_over_a_shared_publisher_directory();
    let id = {
        let conn = state.library.conn();
        taiyaku_library::proposals::raise(&conn).expect("raise");
        taiyaku_library::proposals::open(&conn, 0.0).expect("open")[0].id
    };
    let detail = get_json(&router, &format!("/api/library/proposals/{id}")).await;
    let mut rule = detail["rule"].clone();
    rule["pattern"] = serde_json::json!("Publisher/");

    let response = send_json(
        &router,
        axum::http::Method::POST,
        &format!("/api/library/proposals/{id}/accept"),
        &serde_json::json!(rule),
    )
    .await;

    assert_eq!(response.status(), axum::http::StatusCode::CONFLICT);
    let body = test_support::body_string(response).await;
    assert!(body.contains("Kaguya-sama"), "names what it would move: {body}");
}
```

> **Implementer note.** Three `test_support` helpers above do not exist yet:
> `router_over_a_scanned_work_as_a_reader`, `router_over_two_suffixed_series`
> and `router_over_a_shared_publisher_directory`. Add them beside
> `router_over_a_scanned_work_in_a_series`, following its shape exactly. The
> reader one differs only in the fixture account's `role`; read how
> `fixture_authorization` and the account rows are built and give it a
> non-admin account. **Do not** reach for `authenticating(...)` to fake a
> reader — read the note on `anonymous` in `library_api/tests.rs`, which
> records that helper injecting credentials into every request and a test
> silently reporting 200 as a result.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-serve curation::`

Expected: FAIL to compile — `curation` is not a module.

- [ ] **Step 3: Write the module**

`crates/taiyaku-serve/src/curation.rs`, opening with a doc comment in the
house style:

```rust
//! The `/api/library/proposals/*` handlers. Spec C3 §7.
//!
//! Thin by design, like `library_api`: parse, call
//! [`taiyaku_library::proposals`], serialise. Every decision about what an
//! accept may write lives in that module, so there is one place the collateral
//! guard is applied and one place to read in order to know whether it is.
//!
//! **Not in `library_api`.** That module is C1's read layer over `browse` and
//! carries the root grant into every query; these routes write, are
//! admin-only, and answer about the library's *organisation* rather than about
//! what a reader may see.
//!
//! # Admin, and the status codes
//!
//! Every route here refuses a non-admin with **403**: the credential is valid
//! and the caller is who they say, which is exactly why a 401 would be the
//! wrong answer — the same reasoning `session.rs` records for
//! `refuse_a_device_credential`.
//!
//! A refused accept is **409**, not 400. The request is well-formed and the
//! rule is one the server itself previewed; what is wrong is the state of the
//! library it would be applied to, and the body names the series it would have
//! moved.
```

Then a `fn admin_or_forbidden(principal: &Principal) -> Option<Response>`
mirroring `library_api::failed`'s shape, a `fn rule_json(&ProposedRule) ->
serde_json::Value` and `fn rule_from_json(&serde_json::Value) ->
Result<ProposedRule, String>` pair, and six handlers. Register them in
`routes.rs`'s `guarded_routes`, **inside the same `routes.extend([...])` block
as the C1 routes** so `route_layer` covers them:

```rust
        (
            "/api/library/proposals",
            get(crate::curation::list),
        ),
        (
            "/api/library/proposals/{id}",
            get(crate::curation::detail),
        ),
        (
            "/api/library/proposals/{id}/resolution",
            axum::routing::post(crate::curation::resolution),
        ),
        (
            "/api/library/proposals/{id}/accept",
            axum::routing::post(crate::curation::accept)
                .delete(crate::curation::unaccept),
        ),
        (
            "/api/library/proposals/{id}/dismiss",
            axum::routing::post(crate::curation::dismiss),
        ),
```

- [ ] **Step 4: Add the grant-table rows**

In the same commit — the enumeration test derives its count from this table,
so a registration without a row fails the suite, which is the point.

```rust
        (
            "/api/library/proposals",
            GrantIndependent {
                reason: "admin-only and about the library's organisation, not \
                          about what any one account may see: a proposal names \
                          two series, and an administrator answering it is \
                          deciding structure for everybody",
            },
        ),
        // …the same reason for the other four.
```

- [ ] **Step 5: Run the serve suite**

Run: `cargo test -p taiyaku-serve`

Expected: PASS, including `every_route_refuses_an_anonymous_request` and the
grant enumeration test. If the enumeration test fails with a count mismatch,
a route was registered without a table row.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-serve/src/curation.rs crates/taiyaku-serve/src/curation/tests.rs \
        crates/taiyaku-serve/src/lib.rs crates/taiyaku-serve/src/routes.rs \
        crates/taiyaku-serve/src/test_support.rs
git commit -m "serve: the proposals API, admin-gated and enumerated"
```

---

### Task 6: `curation.ts` and `sendJson`

**Files:**
- Create: `web/src/curation.ts`, `web/src/curation.test.ts`
- Modify: `web/src/api.ts`

**Interfaces:**
- Consumes: Task 5's JSON contract, **as shipped** — which gained statuses the
  original contract block did not name. Verified against `curation.rs` after
  Task 5's two review rounds:

| Status | Means | Body |
| --- | --- | --- |
| 200 | fine | the JSON |
| 204 | fine, nothing to say | empty — `unaccept` and `dismiss` |
| 400 | the rule cannot be evaluated at all — a pattern that will not compile | the reason |
| 403 | not an administrator | — |
| 404 | no proposal with that id, on all five id-addressed routes | the reason |
| 409 | the library refuses this — collateral, a target reached, a rule deciding nothing | **the reason, naming what it would have moved** |
| 500 | a read failed | the reason |

**409's body is the feature, not the status.** The server refuses an accept by
naming the series it would have moved, and a client that surfaced only "409"
would throw away the whole point of the refusal. `sendJson` must carry the body
into the thrown error.

**A store fault during a write still answers 409, not 500** — `accept_with`,
`unaccept` and `dismiss` report refusals and store failures as one
`Result<_, String>`, and splitting them is a store change recorded as an open
item rather than done here. Do not build client logic that assumes a 409 is
always the library's state.
- Produces: `listProposals`, `proposalDetail`, `resolution`, `acceptProposal`,
  `unacceptProposal`, `dismissProposal`; types `Proposal`, `Side`,
  `FirstVolume`, `Rule`, `Detail`, `Resolution`, `Inbox`; pure helpers
  `strength(score): 'strong' | 'weak'`, `consequence(detail, rule, resolution): string`,
  `refusals(resolution): string[]`.

**`Resolution` ships with six fields and the server refuses on five of them.**
This table is the shipped type as of Task 2, checked against
`proposals.rs` rather than against the original design — which said three
refusals and was wrong twice over. **A form that mirrors fewer than all five
offers an Accept the server then refuses**, which is the exact failure
mirroring exists to prevent:

| Field | Meaning |
| --- | --- |
| `also_resolves: number[]` | other open proposals this rule answers — not a refusal |
| `collateral: string[]` | series it decides for that nobody asked about — **refusal** |
| `targets_reached: string[]` | series it decides for that are a target it is accepting *into* — **refusal**, because a rule applies every `set_*` column to everything it matches, so an `edition-of` rule reaching its own target stamps the edition key onto the base's own volumes |
| `entries_matched: number` | `scan_entry` rows it would decide a field for; **zero is a refusal** — a rule deciding nothing is indistinguishable from a decision nobody made |
| `subject_reached: boolean` | whether any matched entry belongs to this proposal's own subject; **false is a refusal**. Note what it does *not* say: a rule can reach the subject and give it back the series it already has |
| `works_moved: number` | works with at least one member changing series — **zero is a refusal**, and it is what catches the rule that reaches the subject and changes nothing. Also the number the consequence line renders |

`refusals(resolution)` is the pure helper turning all five conditions into the
sentences the form shows, so the component does not encode the rule in five
places and the list is testable without a DOM. **Its test must enumerate all
five**, or the next field added to `Resolution` goes unmirrored the same way
these two did.

- [ ] **Step 1: Write the failing test**

`web/src/curation.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import { consequence, strength } from './curation'

describe('strength', () => {
  it('calls two signals strong and one weak', () => {
    // The scale is {0,1,2,3}: `edition-of` adds one per signal and the two
    // certain kinds sit at 3. Strong is 2 and above.
    expect(strength(2)).toBe('strong')
    expect(strength(3)).toBe('strong')
    expect(strength(1)).toBe('weak')
    expect(strength(0)).toBe('weak')
  })
})

describe('consequence', () => {
  it('states what accepting does, including what it does not touch', () => {
    const line = consequence(
      { title: 'Monster - The Perfect Edition' },
      {
        set_edition_key: 'en-color',
        set_kind: 'official',
        set_language: null,
      },
      { also_resolves: [], collateral: [], targets_reached: [],
        entries_matched: 9, subject_reached: true, works_moved: 9 },
    )
    expect(line).toContain('en-color')
    expect(line).toContain('official')
    expect(line).toContain('9 works')
    expect(line).toContain('Nothing on disk is touched')
    expect(line).not.toContain('language')
  })

  it('says how many other proposals a widened rule answers', () => {
    const line = consequence(
      { title: 'Monster - The Perfect Edition' },
      { set_edition_key: 'en-color', set_kind: 'official', set_language: null },
      { also_resolves: [2, 3, 4, 5, 6, 7], collateral: [], targets_reached: [],
        entries_matched: 54, subject_reached: true, works_moved: 54 },
    )
    expect(line).toContain('6 more')
  })
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm -C web test curation`

Expected: FAIL — `Failed to resolve import "./curation"`.

- [ ] **Step 3: Add `sendJson` to `api.ts`**

Beside `getJson`, with the reasoning that module already uses:

```ts
/**
 * A guarded write returning JSON, or `undefined` for a 204.
 *
 * Exported for the same reason `getJson` is: `curation.ts` is a third module
 * of calls against the same server and the same session, and a second `fetch`
 * wrapper there would be a second place for the 401 rule to be forgotten.
 *
 * A non-2xx throws with the body attached rather than the status alone — the
 * proposals API refuses an accept by *naming the series it would have moved*,
 * and a caller that showed only "409" would throw away the whole point of the
 * refusal.
 */
export async function sendJson<T>(
  path: string,
  method: 'POST' | 'DELETE' | 'PATCH',
  body?: unknown,
): Promise<T | undefined> {
  const res = await request(path, {
    method,
    ...(body === undefined
      ? {}
      : {
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify(body),
        }),
  })
  if (!res.ok) throw new Error(await res.text())
  if (res.status === 204) return undefined
  return (await res.json()) as T
}
```

- [ ] **Step 4: Write `curation.ts`**

Types mirroring Task 5's contract exactly, the six calls over `getJson` and
`sendJson`, and the two pure helpers:

```ts
export const strength = (score: number): 'strong' | 'weak' =>
  score >= 2 ? 'strong' : 'weak'

/**
 * The sentence the expanded row shows above its buttons.
 *
 * States the consequence in the terms that bite, and states the *absence* of
 * one too: nothing on disk is touched, which is the difference between this
 * and the ComicInfo write-back C3 §6 keeps deliberately separate.
 *
 * `set_language` is omitted when null rather than rendered as "language
 * unknown" — the form offers it and an accept that did not set it made no
 * claim about language at all.
 */
export function consequence(
  target: { title: string },
  rule: Pick<Rule, 'set_edition_key' | 'set_kind' | 'set_language'>,
  resolution: Resolution,
): string {
  const parts = [`add edition ${rule.set_edition_key ?? '(none)'}`]
  if (rule.set_kind) parts.push(`kind ${rule.set_kind}`)
  if (rule.set_language) parts.push(`language ${rule.set_language}`)
  const more =
    resolution.also_resolves.length > 0
      ? `, answer ${resolution.also_resolves.length} more proposal${
          resolution.also_resolves.length === 1 ? '' : 's'
        }`
      : ''
  return `Accepting will: ${parts.join(' · ')} to series ${target.title}, move ${
    resolution.works_moved
  } works${more}, and keep every volume number as-is. Nothing on disk is touched.`
}
```

- [ ] **Step 5: Run the test**

Run: `pnpm -C web test curation`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add web/src/curation.ts web/src/curation.test.ts web/src/api.ts
git commit -m "web: the proposals API client, and one place the 401 rule lives"
```

---

### Task 7: `Proposals.tsx` — the screen, the route, the sidebar

**Files:**
- Create: `web/src/Proposals.tsx`, `web/src/Proposals.test.tsx`
- Modify: `web/src/Routes.tsx`, `web/src/Sidebar.tsx`

**Interfaces:**
- Consumes: `curation.ts` (Task 6).
- Produces: `export function Proposals(): JSX.Element` at `/proposals`.

- [ ] **Step 1: Write the failing test**

`web/src/Proposals.test.tsx`, following `Series.test.tsx`'s `vi.mock` shape:

```tsx
vi.mock('./curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./curation')>()),
  listProposals: vi.fn(),
}))

it('shows the open count and one line per proposal', async () => {
  vi.mocked(curation.listProposals).mockResolvedValue({
    proposals: [proposal({ id: 1 }), proposal({ id: 2, score: 0 })],
    counts: { 'edition-of': 2 },
  })

  await mount(<Proposals />)

  expect(screen.getByText(/2 open/)).toBeInTheDocument()
  expect(screen.getAllByRole('listitem')).toHaveLength(2)
})

it('filters by kind through the tabs', async () => {
  vi.mocked(curation.listProposals).mockResolvedValue({
    proposals: [proposal({ id: 1 })],
    counts: { 'edition-of': 29, 'attach-chapters': 6 },
  })
  await mount(<Proposals />)

  await userEvent.click(screen.getByRole('button', { name: /Chapters 6/ }))

  expect(vi.mocked(curation.listProposals)).toHaveBeenLastCalledWith(
    expect.objectContaining({ kind: 'attach-chapters' }),
  )
})

it('keeps a weak proposal reachable behind the threshold rather than hiding it', async () => {
  // C3 §4: the pattern-level control is a score threshold, not a suppression
  // list — "a dismissal you disagree with later is a thing you have to be able
  // to find", and so is a proposal the threshold is currently hiding.
  vi.mocked(curation.listProposals).mockResolvedValue({
    proposals: [],
    counts: {},
  })
  await mount(<Proposals />)

  await userEvent.click(screen.getByRole('button', { name: /show weak/i }))

  expect(vi.mocked(curation.listProposals)).toHaveBeenLastCalledWith(
    expect.objectContaining({ min_score: 0 }),
  )
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm -C web test Proposals`

Expected: FAIL — `Failed to resolve import "./Proposals"`.

- [ ] **Step 3: Write the component**

Header (`N open`), the tab row built from `counts`, the list, and one row
expanded at a time held as `useState<number | null>`. Default `min_score` is
**1**, with a "show weak" control that drops it to 0 — the four-value scale
from the Global Constraints, hiding only the no-signal proposals.

- [ ] **Step 4: Register the route and the sidebar entry**

`Routes.tsx`, beside `/search`:

```tsx
<Route path="/proposals" element={<Proposals />} />
```

`Sidebar.tsx`, admin-only, using the bit `Shell.tsx` already reads:

```tsx
{admin && (
  <li>
    <Link to="/proposals">Proposals{open > 0 ? ` (${open})` : ''}</Link>
  </li>
)}
```

*What shipped differs from that sketch in two ways, both found by the branch's
whole-branch review, and the sketch is kept as written rather than edited.*
**First, the entry shipped with no count at all** — Task 1 took the deviation
deliberately and recorded it (a `listProposals` on every page load, since
`Sidebar` mounts around every screen), and the review reversed it: the request
is the flat one — no rule compiled and no `scan_entry` walked, unlike the
detail and dry-run routes — at 0.10 ms for 8 rows and 0.23 ms for 25,008, and
admin-only.
**Second, `open > 0` is not the condition that shipped.** It hides an emptied
inbox, and "0 open" and "not read yet" are different facts a person can act
on differently — so the count is `number | undefined`, and only `undefined`
draws nothing. The threshold it counts at is `curation.ts`'s
`INBOX_MIN_SCORE`, the same constant the screen's first request carries, so
the badge and the header cannot mean different things; `Shell.tsx` owns the
fetch and `shell.ts`'s `InboxRefreshContext` is how an accept or a dismiss
moves it.

- [ ] **Step 5: Run the web suite**

Run: `pnpm -C web test`

Expected: PASS, including `Sidebar.test.tsx` — add a case there that a reader
is shown no Proposals link.

- [ ] **Step 6: Commit**

```bash
git add web/src/Proposals.tsx web/src/Proposals.test.tsx web/src/Routes.tsx \
        web/src/Sidebar.tsx web/src/Sidebar.test.tsx
git commit -m "web: the proposals inbox, its filters and its sidebar entry"
```

---

### Task 8: `ProposalRow.tsx` — the expanded evidence

**Files:**
- Create: `web/src/ProposalRow.tsx`, `web/src/ProposalRow.test.tsx`
- Modify: `web/src/Proposals.tsx` (render rows through it)

**Interfaces:**
- Consumes: `curation.ts`'s `Detail`, `Side`, `FirstVolume`; `api.ts`'s `pageUrl`/`thumbUrl`.
- Produces: `export function ProposalRow(props: { proposal: Proposal; expanded: boolean; onToggle: () => void }): JSX.Element`.

- [ ] **Step 1: Write the failing test**

```tsx
it('shows a cover and two interior pages per side', async () => {
  vi.mocked(curation.proposalDetail).mockResolvedValue(detail())

  await mount(<ProposalRow proposal={proposal()} expanded onToggle={() => {}} />)

  expect(await screen.findAllByRole('img')).toHaveLength(6)
})

it('degrades rather than breaking when a side has no volume yet', async () => {
  // C3 §8's last line. A member of three pages yields fewer interiors, and a
  // side with no work at all yields none — neither may render a broken panel.
  vi.mocked(curation.proposalDetail).mockResolvedValue(
    detail({ subject: side({ first_volume: null }) }),
  )

  await mount(<ProposalRow proposal={proposal()} expanded onToggle={() => {}} />)

  expect(screen.queryByRole('img', { name: /subject/i })).not.toBeInTheDocument()
  expect(screen.getByText(/no volume has been scanned/i)).toBeInTheDocument()
})

it('lists each scored signal on its own line with its sign', async () => {
  vi.mocked(curation.proposalDetail).mockResolvedValue(
    detail({
      reasons: [
        { text: 'suffix "Colored" matches the edition vocabulary', sign: '+' },
        { text: 'volume counts identical — 9 and 9', sign: '+' },
      ],
    }),
  )

  await mount(<ProposalRow proposal={proposal()} expanded onToggle={() => {}} />)

  expect(await screen.findAllByRole('listitem')).toHaveLength(2)
})

it('says plainly that nothing has compared the artwork', async () => {
  // C3 §2's second decision. An absent number that looked like a low score
  // would be worse than a control that admits it has not run.
  vi.mocked(curation.proposalDetail).mockResolvedValue(detail())

  await mount(<ProposalRow proposal={proposal()} expanded onToggle={() => {}} />)

  expect(
    await screen.findByText(/nothing here compared the artwork/i),
  ).toBeInTheDocument()
  expect(screen.getByRole('button', { name: /compare v01/i })).toBeDisabled()
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm -C web test ProposalRow`

Expected: FAIL — `Failed to resolve import "./ProposalRow"`.

- [ ] **Step 3: Write the component**

Collapsed: kind, subject title, `↳ an edition of {target}`, the reason line,
the strength pill, Accept and Dismiss. Expanded: two `Side` panels each drawing
the cover plus `first_volume.interior.map(...)` through `thumbUrl`, the
directories, the four facts, the volume chips, the signed reason list, the
disabled `Compare v01` control with its label, and `AcceptForm` (Task 9).

`directories` is a list, so it renders three ways from one field: the single
path in a `<code>` when there is one, the first plus "+N more" when there are
several, and §8's "nothing scanned yet" state when it is empty. Do not
re-join them into a sentence — the store deliberately stopped doing that.

- [ ] **Step 4: Run the web suite**

Run: `pnpm -C web test`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/ProposalRow.tsx web/src/ProposalRow.test.tsx web/src/Proposals.tsx
git commit -m "web: triage-grade evidence, on demand, per proposal"
```

---

### Task 9: `AcceptForm.tsx` — the rule, shown before it is written

**Files:**
- Create: `web/src/AcceptForm.tsx`, `web/src/AcceptForm.test.tsx`
- Modify: `web/src/ProposalRow.tsx`

**Interfaces:**
- Consumes: `curation.ts`'s `Rule`, `resolution`, `acceptProposal`, `consequence`.
- Produces: `export function AcceptForm(props: { id: number; rule: Rule; target: Side; onAccepted: () => void }): JSX.Element`.

- [ ] **Step 1: Write the failing test**

```tsx
it('shows the rule it is about to write, defaulted narrow', async () => {
  await mount(<AcceptForm id={1} rule={rule()} target={side()} onAccepted={() => {}} />)

  expect(screen.getByLabelText(/pattern/i)).toHaveValue(
    'Go-Toubun no Hanayome - Full Color Edition/',
  )
  expect(screen.getByLabelText(/edition key/i)).toHaveValue('full-color-edition')
  expect(screen.getByLabelText(/kind/i)).toHaveValue('official')
  expect(screen.getByLabelText(/language/i)).toHaveValue('')
})

it('says how many more a widened pattern resolves', async () => {
  vi.mocked(curation.resolution).mockResolvedValue({
    also_resolves: [2, 3, 4, 5, 6, 7],
    collateral: [],
    targets_reached: [],
    entries_matched: 54,
    subject_reached: true,
    works_moved: 54,
  })
  await mount(<AcceptForm id={1} rule={rule()} target={side()} onAccepted={() => {}} />)

  await userEvent.clear(screen.getByLabelText(/pattern/i))
  await userEvent.type(screen.getByLabelText(/pattern/i), '^(.+) - Full Color Edition/')

  expect(await screen.findByText(/resolves 6 more/i)).toBeInTheDocument()
})

it('refuses on a target it would stamp, not only on collateral', async () => {
  // `targets_reached` is the second of three refusals `Resolution` carries. A
  // rule applies every `set_*` column to everything it matches, so an
  // `edition-of` rule reaching its own target makes the base its own colour
  // edition. A form that only watched `collateral` would offer an Accept the
  // server then refuses.
  vi.mocked(curation.resolution).mockResolvedValue({
    also_resolves: [],
    collateral: [],
    targets_reached: ['Monster - The Perfect Edition'],
    entries_matched: 9,
    subject_reached: true,
    works_moved: 9,
  })
  await mount(<AcceptForm id={1} rule={rule()} target={side()} onAccepted={() => {}} />)

  expect(await screen.findByText(/its own volumes/i)).toBeInTheDocument()
  expect(screen.getByRole('button', { name: /^accept/i })).toBeDisabled()
})

it('refuses a rule that reaches its subject and changes nothing', async () => {
  // The subtlest of the five, and the one three review rounds took to close in
  // the store: a `(?<series>...)` capture overrides `set_series` and hands the
  // subject back the series it already has. Everything looks fine — an entry
  // matched, the subject was reached, nothing is collateral — and accepting
  // writes a rule that changes not one row.
  vi.mocked(curation.resolution).mockResolvedValue({
    also_resolves: [],
    collateral: [],
    targets_reached: [],
    entries_matched: 1,
    subject_reached: true,
    works_moved: 0,
  })
  await mount(<AcceptForm id={1} rule={rule()} target={side()} onAccepted={() => {}} />)

  expect(await screen.findByText(/would change nothing/i)).toBeInTheDocument()
  expect(screen.getByRole('button', { name: /^accept/i })).toBeDisabled()
})

it('refuses a rule that would decide nothing', async () => {
  vi.mocked(curation.resolution).mockResolvedValue({
    also_resolves: [],
    collateral: [],
    targets_reached: [],
    entries_matched: 0,
    subject_reached: false,
    works_moved: 0,
  })
  await mount(<AcceptForm id={1} rule={rule()} target={side()} onAccepted={() => {}} />)

  expect(await screen.findByText(/matches nothing/i)).toBeInTheDocument()
  expect(screen.getByRole('button', { name: /^accept/i })).toBeDisabled()
})

it('names the collateral and refuses to submit rather than warning', async () => {
  // The measured `Publisher/` bug, on the surface a person actually uses. The
  // server refuses too; showing an enabled button that 409s would make the
  // guard look like a suggestion.
  vi.mocked(curation.resolution).mockResolvedValue({
    also_resolves: [],
    collateral: ['Kaguya-sama wa Kokurasetai'],
    targets_reached: [],
    entries_matched: 27,
    subject_reached: true,
    works_moved: 27,
  })
  await mount(<AcceptForm id={1} rule={rule()} target={side()} onAccepted={() => {}} />)

  await userEvent.clear(screen.getByLabelText(/pattern/i))
  await userEvent.type(screen.getByLabelText(/pattern/i), 'Publisher/')

  expect(await screen.findByText(/Kaguya-sama/)).toBeInTheDocument()
  expect(screen.getByRole('button', { name: /^accept/i })).toBeDisabled()
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm -C web test AcceptForm`

Expected: FAIL — `Failed to resolve import "./AcceptForm"`.

- [ ] **Step 3: Write the component**

Four controls (pattern, edition key, kind, language), the consequence sentence
from `consequence(...)`, and a debounced `resolution` call on each pattern
edit — use `debounce.ts`, which already exists and is tested. **Accept is
disabled whenever `refusals(resolution)` is non-empty**, which covers all five
of the server's refusal conditions rather than only `collateral`; the sentences
it returns are shown beneath the field. Mirroring the server's refusals here is
what stops the form offering a button that 409s.

- [ ] **Step 4: Run the web suite**

Run: `pnpm -C web test`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/AcceptForm.tsx web/src/AcceptForm.test.tsx web/src/ProposalRow.tsx
git commit -m "web: accept as a form, with the rule and what widening it costs"
```

---

### Task 10: the gate, and the documents this moved

**Files:**
- Modify: `docs/status.md`, `docs/notes.md`
- Modify: `scripts/coverage-floor.mjs` if the web floor moved

- [ ] **Step 1: Run the full gate**

Run: `env -u TAIYAKU_CORPUS scripts/check-all.sh`

Expected: the literal line `all green`. Do not pipe through `tail` or append
`; echo` — either masks the exit status.

- [ ] **Step 2: Run the corpus job separately**

Run:
```bash
TAIYAKU_CORPUS="$PWD/corpus" cargo test --workspace --all-features corpus:: \
  -- --nocapture --test-threads=1
```

Expected: PASS, ~85 s, twelve standing checks.

- [ ] **Step 3: Record the measurements in `docs/notes.md`**

The `resolution` dry run's cost on the corpus — the one thing §11 of the spec
says was not measured. Time one call and write down the figure and the corpus
size it was taken against. `status.md`'s preamble promises every claim there
traces to a measurement recorded in notes, so this goes in notes first.

- [ ] **Step 4: Update `docs/status.md`**

The **opening** shape of the ritual, since this opens nothing and closes
nothing — but the test counts move:

```sh
grep -c '^| O[0-9]' docs/status.md                 # rows
grep -c '^| O[0-9]* | \[x\]' docs/status.md        # closed
grep -nE '[0-9]{3} tests|Nine of the' docs/status.md
grep -n -A1 '^What remains:' docs/status.md        # the count word wraps
```

The test count appears **twice** — in "Five crates, N tests" and again two
lines later in "Nine of the N are standing checks". Update the `web/` row and
add `curation.rs` to the `taiyaku-serve` description.

- [ ] **Step 5: Commit and open the PR**

```bash
git add docs/status.md docs/notes.md
git commit -m "docs: the proposals inbox, and what its dry run costs"
```

Open the PR through the **gitea MCP**, not `gh` — `gh` is authenticated
against the wrong host here. The remote is squash-only, so the PR body *is*
the commit message: write it as one, and never hard-wrap it.

---

## Self-Review

**Spec coverage.** C3 §1 → Tasks 3, 7. §2 → Tasks 4, 8. §3 → Tasks 1, 2, 9.
§4 → Tasks 3, 7 (threshold), 8 (dismiss control). §7 → Task 5. §8's test list →
distributed, each named in the task that owns it. Build design §4's three
corrections → Task 1 (kind and language), Task 3 and Task 7 (the threshold).

**Deliberately absent, and where each went.** C3 §5 metadata editing and §6
ComicInfo, plus O62 — all PR 2, per build design §1. `kind: 'align'` proposals
list and do not act, per C3 §9. The `Compare` control ships disabled, per C3 §2.

**Known soft spots in this plan, flagged rather than hidden.**

1. **Task 2's `rules::` API was verified against the source on 2026-08-26 and
   the plan's first draft was wrong about it** — `Engine::load` returns
   `(Self, Vec<String>)` rather than a rule list, and `resolve` takes `&Root`
   rather than a `root_id`. The corrected code is in the task. The remaining
   unknown is how `Engine::compile` identifies the rule a complaint is about,
   which `candidate_complaint` depends on.
2. **Task 4's `side()` is described rather than written out**, and its one
   trap is named in the task: `common_directory_pattern` errors on a
   multi-directory series, so evidence must not be built on it.
3. **Task 5 needs three new `test_support` helpers**, including a non-admin
   router. The trap recorded in `library_api/tests.rs` — `authenticating`
   injecting credentials into every request — is what makes that one worth
   care.
