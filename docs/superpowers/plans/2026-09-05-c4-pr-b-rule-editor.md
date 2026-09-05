# C4 PR B — the rule editor, and the save route nothing in this product has

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the rule editor screen, and with it `preview::apply` behind
`PUT /api/library/rules` — the first thing in this product that applies a rule.

**Architecture:** `preview::apply` is `preview::preview_with_rows` without the
rollback and with `Documents::Reconcile` in place of `Documents::Skip`, keeping
`rules::replace_all` `pub(crate)` so nothing outside `taiyaku-library` can
rewrite the rule table without deriving. `PUT /api/library/rules` is symmetric
with `POST /api/library/rules/preview` down to the body key, so the same list
previews and saves. The screen is a four-file split — list, form, tabs, screen
— over one pure client module, previewing on commit and never while typing.

**Tech Stack:** Rust (rusqlite, axum, tokio), TypeScript/React 19 (react-router,
vitest with `node` and `jsdom` projects), biome.

## Global Constraints

Copied verbatim from `docs/superpowers/specs/2026-09-04-c4-screens-design.md`
and the repository's standing rules. Every task's requirements implicitly
include this section.

- **`rules::replace_all` stays `pub(crate)`.** Widening it to `pub` for one
  caller "would export exactly the half of the operation that is dangerous
  alone" (§4.1). The route reaches it only through `preview::apply`.
- **The editor previews on commit, never while typing.** A preview costs a
  median 281 ms (unchanged list) and 495 ms (one rule appended) on 130 series
  and 1858 works. Triggers: *field blur or Enter · an enable toggle · a
  drag-reorder · an add · a delete · selecting a different rule in the list*
  (§5.3). No debounce, no keystroke hook, no explicit Preview button.
- **No client-side path, nesting or pattern validation** (§8). The server owns
  all three; a regex that compiles in JS is not the regex the store compiles.
- **No client-side admin guard** (§2). `/config/*` is reachable by URL and every
  screen renders the server's own 403 sentence. A client that *decides* who may
  administer is the shape O93 closed.
- **`Decided` and `Conflict::first`/`wins` are indices into the request's own
  `rules` array, never rule ids.** Never render a rule id from those.
- **No `exclude_globs` behaviour** (O92, §1), **no "proposals resolved"**
  (§5.5 — a derivation cannot produce it), **no audit log** (§1).
- **Wire spellings, from `rules.rs`'s own `Display` impls** — `kind` is one of
  `prefix` · `regex` · `builtin-volume` · `builtin-chapter`; `match_on` is one
  of `path` · `name` · `parent`. *(The spec's §5.1 mockup writes `dir_regex`
  and `rel`; those strings do not exist in this codebase. Use the four and the
  three above.)* `Field` on the wire is `series` · `edition_key` · `kind` ·
  `language` · `volume` · `chapter` · `title` (`config_api::field_key`).
- **Never add AI/Claude attribution** to any file, comment, doc, commit or PR.
- **Shell:** the Bash tool runs zsh with `noclobber`; fish syntax is a parse
  error. `cd` is zoxide — avoid `cd subdir && …` chains.
- **The gate:** `env -u TAIYAKU_CORPUS scripts/check-all.sh`, and quote its
  final `all green` line rather than piping through `tail`.

---

## Three findings that correct the spec

Established by reading the code on 2026-09-05, before this plan was written.
Each changes a task below.

1. **`PUT /api/library/rules` is a *method* on a path that already exists**, so
   §4.2's "a new route goes in four lists" is wrong in detail. `expected` in
   `auth/tests.rs` and the `table.len() - driven` count both stay exactly as
   they are — the pattern `/api/library/rules` is already in both, and the
   grant-posture table is keyed by path. What actually changes is
   `guarded_routes()` (the existing entry gains `.put(...)`), that table entry's
   *reason text*, and a **fifth** list the spec does not name:
   `config_api::tests::every_config_api_route_refuses_a_reader`, whose
   hand-typed `(method, path)` array needs `(Method::PUT, "/api/library/rules")`.
   Task 3 asserts all of this rather than assuming it.
2. **`proposals::reopened` answers a count, not ids.** `Applied.reopened_proposals`
   needs ids, so Task 1 adds `reopened_ids` and makes `reopened` its `.len()` —
   one definition, the discipline that module's own doc comment states.
3. **`api.ts`'s `sendJson` union is `'POST' | 'DELETE' | 'PATCH'`** — no `'PUT'`.
   Task 4 adds it.

**One deliberate addition beyond §4.2's stated body.** `PUT`'s answer carries
`complaints` alongside `impact`, `rules` and `reopened_proposals`, exactly as
`list_rules` and `import_rules` already do. `rules::read`'s complaints cover an
unparseable `kind`/`match_on` and `Engine::compile`'s cover a pattern that will
not compile — and *no other route the editor calls reports either*:
`preview_rules` answers `impact`/`rows`/`rows_truncated`/`conflicts` and a rule
whose pattern does not compile is silently dropped by the engine the derivation
loads. Without this key, a person can save a broken regex and be told nothing.
One line, symmetric with two sibling routes.

---

## File structure

**Rust — the one piece of server work.**

| File | Responsibility |
| --- | --- |
| `crates/taiyaku-library/src/proposals.rs` | `reopened_ids` added; `reopened` becomes its `.len()` |
| `crates/taiyaku-library/src/preview.rs` | `Applied`, `apply` — the committed twin of `preview_with_rows` |
| `crates/taiyaku-library/src/preview/tests.rs` | the existing preview-equals-save test drives `apply` |
| `crates/taiyaku-serve/src/config_api.rs` | `save_rules` |
| `crates/taiyaku-serve/src/routes.rs` | `.put(save_rules)`, and the posture table's reason |
| `crates/taiyaku-serve/src/config_api/tests.rs` | route tests, and the reader-refusal array |

**Web — one pure module and four components.** The split follows
`Proposals`/`ProposalRow` and `Roots`/`RootRow`: the screen owns which rules
there are and what the preview says; each child owns one bounded thing.

| File | Responsibility |
| --- | --- |
| `web/src/api.ts` | `sendJson` learns `'PUT'` |
| `web/src/rules.ts` | wire types, the client, the `root_path`↔`root_id` converter, pure helpers |
| `web/src/RuleForm.tsx` | one rule's fields; commits on blur or Enter |
| `web/src/RuleList.tsx` | the ordered list: selection, enable toggle, drag-reorder, add, delete |
| `web/src/PreviewTabs.tsx` | Impact · Matched files · Provenance · Conflicts |
| `web/src/RuleEditor.tsx` | the screen: rule state, preview state, save |
| `web/src/config.ts` | `TABS` prepends `{ path: 'rules', label: 'Rules' }` |
| `web/src/Config.tsx` | `SCREENS.rules` |

---

## Task 1: `proposals::reopened_ids`

**Files:**
- Modify: `crates/taiyaku-library/src/proposals.rs` (`reopened`, near line 475)
- Test: `crates/taiyaku-library/src/proposals/tests.rs`

**Interfaces:**
- Consumes: `proposals::States` (`pub type States = BTreeMap<i64, String>`),
  `STATE_OPEN`, `STATE_ACCEPTED`.
- Produces: `pub(crate) fn reopened_ids(before: &States, after: &States) -> Vec<i64>`,
  ascending by id. `pub(crate) fn reopened(before: &States, after: &States) -> usize`
  keeps its signature and becomes `reopened_ids(before, after).len()`.

- [ ] **Step 1: Write the failing test**

Append to `crates/taiyaku-library/src/proposals/tests.rs`:

```rust
/// [`crate::preview::Applied::reopened_proposals`] answers *which* proposals a
/// save put back in front of someone, not merely how many — the same shape
/// [`crate::rules::Deleted::reopened_proposals`] and `rules::import_json`
/// already answer, so all three routes report one key one way.
///
/// **Ascending, and that is `States`' own `BTreeMap` ordering rather than a
/// sort this function performs.** Asserted here because a later change to
/// `States`' type would silently make the order arbitrary, and a list of ids
/// whose order drifts between requests reads as a different answer.
#[test]
fn reopened_ids_names_every_proposal_a_derivation_put_back() {
    let before: crate::proposals::States = [
        (7, "accepted".to_string()),
        (3, "accepted".to_string()),
        (5, "accepted".to_string()),
        (9, "open".to_string()),
    ]
    .into_iter()
    .collect();
    let after: crate::proposals::States = [
        (7, "open".to_string()),
        (3, "open".to_string()),
        (5, "accepted".to_string()),
        (9, "open".to_string()),
    ]
    .into_iter()
    .collect();

    assert_eq!(
        crate::proposals::reopened_ids(&before, &after),
        vec![3, 7],
        "only the two that went accepted -> open, ascending; 5 never moved and 9 was open already"
    );
    assert_eq!(
        crate::proposals::reopened(&before, &after),
        2,
        "the count is the list's own length, not a second definition of what reopened means"
    );
}
```

- [ ] **Step 2: Run it and watch it fail**

```bash
cd /home/davey/projects/taiyaku && cargo test -p taiyaku-library reopened_ids_names_every 2>&1 | tail -20
```

Expected: `error[E0425]: cannot find function 'reopened_ids' in module 'crate::proposals'`.

- [ ] **Step 3: Implement**

In `crates/taiyaku-library/src/proposals.rs`, replace the body of `reopened`
(keeping its existing doc comment above `reopened_ids`, and giving `reopened`
a short one) with:

```rust
pub(crate) fn reopened_ids(before: &States, after: &States) -> Vec<i64> {
    after
        .iter()
        .filter(|(id, state)| {
            state.as_str() == STATE_OPEN && before.get(*id).is_some_and(|was| was == STATE_ACCEPTED)
        })
        .map(|(id, _state)| *id)
        .collect()
}

/// How many proposals [`reopened_ids`] named — the count
/// [`crate::preview::Impact::proposals_reopened`] carries.
///
/// **Its `.len()`, not a second filter.** Two ways of arriving at one number
/// is how they come to disagree — the argument [`crate::preview::Impact`]'s
/// own `proposals_raised` field makes for carrying the derive report's count
/// through rather than recounting it here.
pub(crate) fn reopened(before: &States, after: &States) -> usize {
    reopened_ids(before, after).len()
}
```

Move the existing multi-paragraph doc comment that sits above `reopened` today
so it introduces `reopened_ids` instead — it explains what "reopened" means,
which is now that function's job.

- [ ] **Step 4: Run the test and the module**

```bash
cd /home/davey/projects/taiyaku && cargo test -p taiyaku-library proposals:: 2>&1 | tail -5
```

Expected: `test result: ok.` with no failures.

- [ ] **Step 5: Commit**

```bash
cd /home/davey/projects/taiyaku && git add crates/taiyaku-library/src/proposals.rs crates/taiyaku-library/src/proposals/tests.rs && git commit -m "proposals: reopened_ids names the proposals a derivation put back"
```

---

## Task 2: `preview::apply` — the committed twin

**Files:**
- Modify: `crates/taiyaku-library/src/preview.rs` (after `preview_with_rows`, ~line 493)
- Modify: `crates/taiyaku-library/src/preview/tests.rs:27-79` (`a_preview_and_a_real_save_produce_the_same_structure`)

**Interfaces:**
- Consumes: `grouping`, `diff`, `Impact` (Task 0 state, unchanged);
  `proposals::states`, `proposals::reopened_ids` (Task 1); `rules::replace_all`,
  `rules::read`, `derive::derive_with`, `derive::Documents::Reconcile`.
- Produces:
  ```rust
  pub struct Applied {
      pub impact: Impact,
      pub reopened_proposals: Vec<i64>,
      pub rules: Vec<crate::rules::Rule>,
      pub complaints: Vec<String>,
  }
  pub fn apply(conn: &Connection, proposed: &[crate::rules::Rule]) -> Result<Applied, String>
  ```
  Task 3 calls `apply` and reads all four fields.

- [ ] **Step 1: Write the failing test**

Two tests. Append to `crates/taiyaku-library/src/preview/tests.rs`:

```rust
/// **A save writes, where a preview does not** — the one difference that
/// matters, asserted by reading the grouping back off the connection after
/// `apply` returns rather than by trusting the `Impact` it reported. An
/// `apply` that rolled back like its twin would answer the identical
/// `Impact` and leave this assertion red.
#[test]
fn apply_commits_what_preview_rolls_back() {
    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();
    let mut proposed = crate::rules::read(&conn).unwrap().0;
    proposed.push(crate::tests::a_rule_setting_series("Alpha"));

    let before = grouping(&conn).unwrap();
    let predicted = preview(&conn, &proposed).unwrap();
    assert_eq!(
        grouping(&conn).unwrap(),
        before,
        "a preview must leave the library exactly as it found it"
    );

    let applied = crate::preview::apply(&conn, &proposed).unwrap();

    assert_eq!(
        applied.impact, predicted,
        "the save's own impact must equal the preview made from the same list"
    );
    assert_ne!(
        grouping(&conn).unwrap(),
        before,
        "the save must have changed the library — this is the whole of what apply adds"
    );
    assert_eq!(
        applied.rules.len(),
        proposed.len(),
        "the answer carries the rules as written, so the editor need not re-fetch"
    );
    assert!(
        applied.rules.iter().all(|rule| rule.id != 0),
        "every rule the save answers carries the real id it was written under, including the \
         appended one that arrived as id 0: {:?}",
        applied.rules
    );
}

/// **The property `rules::replace_all`'s differential update exists for, and
/// the one `POST /rules/import` breaks** — spec §4's whole argument for why
/// this route is not that one. Saving the list unchanged must reopen nothing,
/// because every kept row keeps its id and `created_rule_id`'s
/// `ON DELETE SET NULL` never fires.
#[test]
fn a_save_of_the_unchanged_list_reopens_nothing() {
    let fixture = crate::tests::library_with_an_accepted_proposal();
    let conn = fixture.conn();
    let proposed = crate::rules::read(&conn).unwrap().0;

    let applied = crate::preview::apply(&conn, &proposed).unwrap();

    assert_eq!(
        applied.reopened_proposals,
        Vec::<i64>::new(),
        "an unchanged list keeps every rule id, so nothing orphans a created_rule_id"
    );
    assert_eq!(applied.impact.proposals_reopened, 0);
    let still_accepted: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM proposal WHERE state = 'accepted'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        still_accepted, 1,
        "the accepted proposal is still answered after a save of the list it wrote"
    );
}
```

**Before writing these, check the fixture name.** Run:

```bash
cd /home/davey/projects/taiyaku && grep -n 'fn library_with_an_accepted_proposal\|accepted_proposal\|fn a_rule_setting_series\|fn library_with_two_roots' crates/taiyaku-library/src/tests.rs
```

If no fixture builds an accepted proposal with a `created_rule_id`, add one to
`crates/taiyaku-library/src/tests.rs` beside `library_with_two_roots`, built
the way `proposals/tests.rs` already builds one — accept a proposal through
`proposals::resolve` so `created_rule_id` is set by the store rather than by a
hand-written `INSERT`, which is what makes the FK real.

- [ ] **Step 2: Run and watch it fail**

```bash
cd /home/davey/projects/taiyaku && cargo test -p taiyaku-library preview::tests::apply_commits 2>&1 | tail -20
```

Expected: `error[E0425]: cannot find function 'apply' in module 'crate::preview'`.

- [ ] **Step 3: Implement `Applied` and `apply`**

In `crates/taiyaku-library/src/preview.rs`, immediately after
`preview_with_rows` ends (before the `// --- conflict detection ---` banner):

```rust
/// What a save did — [`apply`]'s answer.
///
/// **The `Impact` is the same shape [`preview`] answers, on purpose.** Spec
/// C4 §4.2: the screen compares the two, and a save whose impact differs from
/// the preview it was made from is drawn as a warning rather than swallowed.
/// Nothing should change between them on a single-writer library; the screen's
/// entire promise is that the preview is true, and the cheapest place to
/// notice it is false is the save that just disproved it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    pub impact: Impact,
    /// Every proposal this derivation put back in front of someone, ascending
    /// — [`crate::proposals::reopened_ids`]. The ids, not the count, because
    /// spec C4 §1 asks the screen to *state* them after a save;
    /// `impact.proposals_reopened` is the same list's length.
    pub reopened_proposals: Vec<i64>,
    /// The rule table as this save left it, read back inside the same
    /// transaction — so a rule that arrived as `id: 0` comes back carrying
    /// the id it was actually written under and the editor needs no second
    /// request to learn it.
    pub rules: Vec<crate::rules::Rule>,
    /// [`crate::rules::read`]'s complaints plus
    /// [`crate::rules::Engine::compile`]'s, the same pair `rules::import_json`'s
    /// route reports. A pattern that will not compile is dropped silently by
    /// the engine every derivation loads, so this is the only place a save can
    /// say so.
    pub complaints: Vec<String>,
}

/// Replace the rule table with `proposed` and derive over it, committed.
///
/// **[`preview_with_rows`] without the rollback, and with
/// [`crate::derive::Documents::Reconcile`] in place of `Skip`** — spec C4 §4.1.
/// A real derivation reconciles hand documents under `<root>/.taiyaku/`; a
/// preview must not, because a rollback would not undo a file it wrote. That
/// one enum value is the whole difference in what reaches the user's tree.
///
/// **Why this function exists rather than the sequence inlined in the route.**
/// [`crate::rules::replace_all`] is `pub(crate)`, which is correct — nothing
/// outside this crate should be able to rewrite the rule table without
/// deriving. Widening it to `pub` for one caller would export exactly the half
/// of the operation that is dangerous alone: a rule set replaced with nothing
/// derived over it is a library whose structure silently disagrees with the
/// rules that are supposed to have produced it, until somebody scans.
///
/// **One `BEGIN IMMEDIATE` over the whole library, the same trade
/// [`preview_with_rows`] makes and for the same reason.** A plain
/// [`crate::derive::derive`] is atomic per work rather than across the
/// library, so a half-derived library still has most of its works current.
/// This is not that: a save either lands whole or does not land, because a
/// rule set half-applied is a state nothing in this product knows how to
/// describe. The cost is the write lock, held for one full derivation —
/// measured at a median 281 ms on 130 series and 1858 works — which is why
/// the route takes [`crate::Library::working`] rather than the shared
/// connection.
///
/// # Errors
///
/// If SQLite refuses the transaction, if [`crate::rules::replace_all`] refuses
/// `proposed` (a nonzero id naming no row this table holds — the stale-editor
/// case — or one id named twice), if [`crate::derive::derive_with`] fails, or
/// if the commit itself is refused.
pub fn apply(conn: &Connection, proposed: &[crate::rules::Rule]) -> Result<Applied, String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("starting the save transaction: {e}"))?;
    let before = grouping(&tx)?;
    let proposals_before = crate::proposals::states(&tx)?;

    crate::rules::replace_all(&tx, proposed)?;
    let report = crate::derive::derive_with(&tx, crate::derive::Documents::Reconcile)?;
    let after = grouping(&tx)?;
    let proposals_after = crate::proposals::states(&tx)?;

    // Read back inside the transaction, so the answer describes what is about
    // to be committed rather than what a second read might find afterwards.
    let (rules, mut complaints) = crate::rules::read(&tx)?;
    let (_engine, compile_complaints) = crate::rules::Engine::compile(&rules);
    complaints.extend(compile_complaints);

    let reopened_proposals = crate::proposals::reopened_ids(&proposals_before, &proposals_after);

    // Explicit, for the reason `preview_with_rows`'s own `rollback` is
    // explicit: a commit that happens because nobody rolled back is a commit
    // nobody can find when reading this later.
    tx.commit()
        .map_err(|e| format!("committing the save: {e}"))?;

    Ok(Applied {
        impact: diff(
            &before,
            &after,
            reopened_proposals.len(),
            report.raised,
        ),
        reopened_proposals,
        rules,
        complaints,
    })
}
```

- [ ] **Step 4: Run and watch it pass**

```bash
cd /home/davey/projects/taiyaku && cargo test -p taiyaku-library preview:: 2>&1 | tail -5
```

Expected: `test result: ok.`

- [ ] **Step 5: Make the existing preview-equals-save test drive `apply`**

This is §4.1's second reason for the function. In
`crates/taiyaku-library/src/preview/tests.rs`, replace lines 58-78 of
`a_preview_and_a_real_save_produce_the_same_structure` — the hand-typed
`replace_all` + `derive` sequence — with:

```rust
    let before_save = grouping(&conn).unwrap();
    let applied = crate::preview::apply(&conn, &proposed).unwrap();
    let after_save = grouping(&conn).unwrap();

    assert_ne!(
        before_save, after_save,
        "the fixture must actually change, or this proves nothing"
    );
    assert_eq!(
        predicted, applied.impact,
        "the predicted Impact is not what the save produced"
    );
```

Add to the test's doc comment, above `**Important 2.**`:

```
/// **The save side is [`crate::preview::apply`], which is what makes this a
/// claim about shipped code.** It used to be `replace_all` followed by
/// `derive` — two lines that existed nowhere but here, so the test asserted
/// that a preview agrees with a sequence this test types out rather than with
/// the save this product actually performs. Spec C4 §4.1, and the failure
/// mode `plans-need-executed-fixtures` names, sitting inside the test that
/// guards the screen's central promise.
```

Delete the now-unused `use` items from the `crate::proposals::states` /
`crate::rules::replace_all` calls this removed, and let the compiler name any
that are still needed.

- [ ] **Step 6: Run the whole crate**

```bash
cd /home/davey/projects/taiyaku && cargo test -p taiyaku-library 2>&1 | tail -5
```

Expected: `test result: ok.` — no failures, no warnings about unused imports.

- [ ] **Step 7: Commit**

```bash
cd /home/davey/projects/taiyaku && git add crates/taiyaku-library/src/preview.rs crates/taiyaku-library/src/preview/tests.rs crates/taiyaku-library/src/tests.rs && git commit -m "preview: apply is the save this product performs, and the preview test drives it"
```

---

## Task 3: `PUT /api/library/rules`

**Files:**
- Modify: `crates/taiyaku-serve/src/config_api.rs` (after `preview_rules`, ~line 875)
- Modify: `crates/taiyaku-serve/src/routes.rs:1906-1908` and `:2352-2360`
- Modify: `crates/taiyaku-serve/src/config_api/tests.rs:126-133` and append tests

**Interfaces:**
- Consumes: `preview::apply`, `preview::Applied` (Task 2); the existing
  `rule_from_json`, `rule_json`, `impact_json`, `MAX_PROPOSED_RULES`,
  `NoAnswer`, `panicked`, `session::administering_or_forbidden`.
- Produces: `pub(crate) async fn save_rules(...) -> Response`, answering
  `200 { "impact": {…}, "rules": [...], "reopened_proposals": [ids], "complaints": [...] }`.
  Task 4's `saveRules` client is typed against exactly this body.

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-serve/src/config_api/tests.rs`:

```rust
/// **The route derives**, asserted by reading the grouping back after the
/// save rather than by trusting the `impact` it reported. Spec C4 §7: a
/// handler that wrote the rule table and returned would answer the identical
/// body and leave this red — which is precisely the defect this route exists
/// to close, since every other rules route does exactly that.
#[tokio::test]
async fn saving_rules_derives_over_the_library() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let listed = get_json(&router, "/api/library/rules").await;
    let existing = listed["rules"].as_array().cloned().expect("rules array");

    let mut proposed = existing.clone();
    proposed.push(retitle_rule(&existing));

    let before = {
        let conn = state.library.conn();
        taiyaku_library::preview::grouping(&conn).expect("the grouping before the save")
    };

    let response = send_json(
        &router,
        Method::PUT,
        "/api/library/rules",
        &serde_json::json!({ "rules": proposed }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let json = body_json(response).await;

    let after = {
        let conn = state.library.conn();
        taiyaku_library::preview::grouping(&conn).expect("the grouping after the save")
    };
    assert_ne!(
        before, after,
        "the save must have re-derived the library, not merely written the rule table: {json}"
    );
    assert!(
        json["impact"]["works_rehomed"].as_u64().unwrap() > 0,
        "{json}"
    );
    assert_eq!(
        json["rules"].as_array().unwrap().len(),
        proposed.len(),
        "the answer carries the rules as written: {json}"
    );
    assert!(
        json["reopened_proposals"].is_array(),
        "the same key delete_rule and import_rules answer: {json}"
    );
    assert!(json["complaints"].is_array(), "{json}");
}

/// **The save and the preview it was made from must agree** — the screen
/// compares them and draws a warning when they differ, and this is the check
/// that the comparison is between two things that can agree at all.
#[tokio::test]
async fn a_save_answers_the_impact_its_own_preview_predicted() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let listed = get_json(&router, "/api/library/rules").await;
    let existing = listed["rules"].as_array().cloned().expect("rules array");

    let mut proposed = existing.clone();
    proposed.push(retitle_rule(&existing));

    let previewed = preview_json(&router, &serde_json::json!({ "rules": proposed })).await;
    let response = send_json(
        &router,
        Method::PUT,
        "/api/library/rules",
        &serde_json::json!({ "rules": proposed }),
    )
    .await;
    let saved = body_json(response).await;

    assert_eq!(
        previewed["impact"], saved["impact"],
        "the preview and the save it was made from disagree, which is the one thing this \
         screen's promise cannot survive"
    );
}

/// A stale editor — a rule the caller's own copy still shows, deleted by
/// somebody else since — is the caller's mistake, not a store fault.
/// [`crate::config_api::preview_rules`]'s own doc comment states the same fold
/// for the same reason.
#[tokio::test]
async fn saving_a_rule_that_no_longer_exists_is_the_callers_mistake() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let listed = get_json(&router, "/api/library/rules").await;
    let mut proposed = listed["rules"].as_array().cloned().expect("rules array");
    proposed[0]["id"] = serde_json::json!(9_999);

    let response = send_json(
        &router,
        Method::PUT,
        "/api/library/rules",
        &serde_json::json!({ "rules": proposed }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let text = body_text(response).await;
    assert!(
        text.contains("9999") || text.contains("no such rule"),
        "the refusal must name the rule that is not there: {text}"
    );
}

/// The same bound the preview route holds, spelled the same way — spec §4.2:
/// the same list previews and saves, so the client cannot send one shape to
/// one and another to the other.
#[tokio::test]
async fn saving_more_rules_than_the_limit_is_refused() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let listed = get_json(&router, "/api/library/rules").await;
    let existing = listed["rules"].as_array().cloned().expect("rules array");
    let one = existing.first().cloned().expect("at least one seeded rule");

    let mut proposed = Vec::new();
    for _ in 0..=201 {
        let mut copy = one.clone();
        copy["id"] = serde_json::json!(0);
        proposed.push(copy);
    }

    let response = send_json(
        &router,
        Method::PUT,
        "/api/library/rules",
        &serde_json::json!({ "rules": proposed }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let text = body_text(response).await;
    assert!(text.contains("200"), "the refusal names the limit: {text}");
}

/// A body with no `rules` array at all.
#[tokio::test]
async fn saving_without_a_rules_array_is_refused() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let response = send_json(
        &router,
        Method::PUT,
        "/api/library/rules",
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
```

Then extend the reader-refusal array at
`crates/taiyaku-serve/src/config_api/tests.rs:126-133` — **finding 1: this is
the fifth list, which §4.2 does not name**:

```rust
    for (method, path) in [
        (Method::GET, "/api/library/rules"),
        (Method::POST, "/api/library/rules"),
        // Spec C4 §4.2's save route. A *method* on a path already in this
        // list, which is exactly why it needs its own line: `guarded_routes`
        // and `auth/tests.rs`'s `expected` array are keyed by path and
        // neither of them moves when a method is added, so this loop is the
        // only thing that would notice a save route shipping ungated.
        (Method::PUT, "/api/library/rules"),
        (Method::PATCH, "/api/library/rules/1"),
        (Method::DELETE, "/api/library/rules/1"),
        (Method::POST, "/api/library/rules/preview"),
        (Method::POST, "/api/library/rules/import"),
    ] {
```

**Check the helpers exist before writing.** Run:

```bash
cd /home/davey/projects/taiyaku && grep -n 'async fn send_json\|async fn get_json\|async fn body_json\|async fn body_text\|async fn preview_json\|fn retitle_rule' crates/taiyaku-serve/src/config_api/tests.rs
```

If `body_json`, `body_text` or `preview_json` do not exist with those names,
use the names that do — do not add duplicates. If `preview_json` requires a
`selected` key, pass `"selected": serde_json::Value::Null`.

- [ ] **Step 2: Run and watch them fail**

```bash
cd /home/davey/projects/taiyaku && cargo test -p taiyaku-serve config_api::tests::saving 2>&1 | tail -20
```

Expected: every one of them fails with `405 Method Not Allowed` (asserted
`200`/`400`, got `405`), because the path exists and the method does not.

- [ ] **Step 3: Write `save_rules`**

In `crates/taiyaku-serve/src/config_api.rs`, immediately after `preview_rules`
ends and before `import_rules`' doc comment:

```rust
/// `PUT /api/library/rules` — replace the whole rule set with the `rules` the
/// request carries **and derive over the library**, committed.
///
/// **The first route in this product that applies a rule.** Every other rules
/// route writes the `rule` table and returns — [`patch_rule`]'s own doc
/// comment says so in as many words — so a person editing rules through this
/// screen changed the rule table and nothing else until somebody scanned.
/// Spec C4 §1's Save button states "apply to 71 works"; this is what makes
/// that sentence true. See the build design's §4.
///
/// **`PUT`, not `POST`**, because the body is the complete desired state of a
/// collection that already has a URL, which is what `PUT` means.
/// [`import_rules`] stays what it is — a *document* import that mints fresh
/// ids by design, for a document from another library that has none of this
/// one's, and which therefore reopens every accepted proposal whose rule it
/// replaces. That is correct for a foreign document and wrong for a save,
/// which is the whole reason these are two routes.
///
/// **Symmetric with [`preview_rules`] down to the body key**, the
/// [`MAX_PROPOSED_RULES`] bound and the [`rule_from_json`] reader — the same
/// list previews and saves, so the client cannot send one shape to one and
/// another to the other. It takes no `selected`: rows are a property of which
/// rule is open in the editor, and a save has no such thing.
///
/// **On [`taiyaku_library::Library::working`], not `conn`** — this module's
/// own doc comment states why in full, and a save is the longest job here:
/// one full derivation over the whole library under a single write lock,
/// measured at a median 281 ms on 130 series and 1858 works.
///
/// **Every [`taiyaku_library::preview::apply`] failure is answered 400**, the
/// same admitted fold [`preview_rules`] states for the identical reason:
/// `rules::replace_all` refuses a nonzero id naming no row this table holds,
/// which is exactly what a stale editor sends — a rule the caller's own copy
/// still shows, deleted by someone else since — and there is no
/// `evaluable`-shaped split to tell that apart from a genuine store fault.
pub(crate) async fn save_rules(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }

    let Some(raw_rules) = body.get("rules").and_then(serde_json::Value::as_array) else {
        return NoAnswer::Unusable("the save body must carry a `rules` array".to_string())
            .into_response();
    };
    let proposed: Vec<rules::Rule> = match raw_rules.iter().map(rule_from_json).collect() {
        Ok(proposed) => proposed,
        Err(why) => return NoAnswer::Unusable(why).into_response(),
    };
    if proposed.len() > MAX_PROPOSED_RULES {
        return NoAnswer::Unusable(format!(
            "the proposed list holds {} rules, over this route's {MAX_PROPOSED_RULES}-rule limit",
            proposed.len()
        ))
        .into_response();
    }

    let saved = tokio::task::spawn_blocking(move || {
        let working = state.library.working().map_err(NoAnswer::Failed)?;
        let applied = preview::apply(&working, &proposed).map_err(NoAnswer::Unusable)?;
        Ok::<_, NoAnswer>(serde_json::json!({
            "impact": impact_json(&applied.impact),
            "rules": applied.rules.iter().map(rule_json).collect::<Vec<_>>(),
            "reopened_proposals": applied.reopened_proposals,
            "complaints": applied.complaints,
        }))
    })
    .await;

    match saved {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => panicked("saving the proposed rule list", &why),
    }
}
```

- [ ] **Step 4: Register the route — the lists that actually change**

In `crates/taiyaku-serve/src/routes.rs`, at the `guarded_routes()` entry
(line ~1906):

```rust
        (
            "/api/library/rules",
            get(crate::config_api::list_rules)
                .post(crate::config_api::create_rule)
                // Spec C4 §4.2's save route, PR B. A method on this same
                // path, so nothing else in this file's route *lists* moves —
                // and `config_api::tests::every_config_api_route_refuses_a_
                // reader` grows a line, because a per-path list cannot see a
                // method arrive.
                .put(crate::config_api::save_rules),
        ),
```

And in the grant-posture table (line ~2353), extend that entry's `reason` so
it names the save — the table holds every entry to stating its own reason:

```rust
        (
            "/api/library/rules",
            GrantIndependent {
                reason: "admin-only and about how the library is organised, \
                          not about what any one account may see: a rule is \
                          a property of the scan, and an administrator's \
                          roots are not filtered — the same standing \
                          /api/library/proposals holds above. PR B's PUT \
                          derives over every work in the library from the \
                          proposed set, which is as far from a per-account \
                          scope as a write in this product gets",
            },
        ),
```

**Do not touch `auth/tests.rs`.** `expected` already holds
`/api/library/rules`, and `driven`/`table.len() - driven` count patterns rather
than methods. Step 5 proves that rather than assuming it.

- [ ] **Step 5: Run the tests, including the two counting ones**

```bash
cd /home/davey/projects/taiyaku && cargo test -p taiyaku-serve 2>&1 | tail -8
```

Expected: `test result: ok.` — in particular `auth::tests` green with no edit,
which is the empirical proof of finding 1. **If either count assertion fails,
the finding was wrong**: fix the count and say so in the commit message rather
than silently adjusting it.

- [ ] **Step 6: Prove the gate on this route is real**

Hand-apply a mutation and watch a test go red — the two-minute defence
`plans-need-executed-fixtures` names. Edit by hand, do not script it:

1. Comment out the three-line `administering_or_forbidden` block at the top of
   `save_rules`.
2. Run the command below and confirm it **FAILS**, naming `PUT /api/library/rules`.
3. Restore the block.
4. Run it again and confirm it passes.

A gate that cannot be made to fail is not a gate.

```bash
cd /home/davey/projects/taiyaku && cargo test -p taiyaku-serve every_config_api_route_refuses_a_reader 2>&1 | tail -5
```

Expected after restoring: `test result: ok. 1 passed`.

- [ ] **Step 7: Commit**

```bash
cd /home/davey/projects/taiyaku && git add crates/taiyaku-serve/src/config_api.rs crates/taiyaku-serve/src/config_api/tests.rs crates/taiyaku-serve/src/routes.rs && git commit -m "serve: PUT /api/library/rules saves a rule set and derives over it"
```

---

## Task 4: `web/src/rules.ts` — types, client, converter

**Files:**
- Modify: `web/src/api.ts:127-131` (`sendJson`'s method union)
- Create: `web/src/rules.ts`
- Test: `web/src/rules.test.ts` (the `node` project — `src/**/*.test.ts`)

**Interfaces:**
- Consumes: `getJson`, `sendJson` from `./api`; `Root`, `listRoots` from `./config`.
- Produces, all named exactly as later tasks use them:
  ```ts
  export interface Rule { id, ordinal, enabled, root_id, kind, pattern, match_on,
                          set_series, set_edition_key, set_kind, set_language, note }
  export interface Impact { series_before, series_after, series_merged,
                            works_rehomed, works_orphaned,
                            proposals_reopened, proposals_raised }
  export interface Placement { series, work, edition }
  export type Decided = { kind: 'rule'; index: number } | { kind: 'root_default' }
  export interface Row { rel_path, root_id, before, after, matched, decided_by }
  export interface Conflict { field, rel_path, first, wins, others, examples }
  export interface Preview { impact, rows, rows_truncated, conflicts }
  export interface Saved { impact, rules, reopened_proposals, complaints }
  export interface RuleDoc { ordinal, enabled, root_path, kind, pattern, match_on,
                             set_edition_key, set_kind, set_language, set_series, note }
  export const RULE_KINDS: readonly string[]
  export const MATCH_ONS: readonly string[]
  export const PREVIEW_EXPENSIVE_ABOVE: number
  export const listRules: () => Promise<{ rules: Rule[]; complaints: string[] }>
  export const previewRules: (rules: Rule[], selected?: number) => Promise<Preview>
  export const saveRules: (rules: Rule[]) => Promise<Saved>
  export const importRules: (doc: string) => Promise<{ rules: Rule[]; complaints: string[]; reopened_proposals: number[] }>
  export function toDocument(rules: Rule[], roots: Root[]): RuleDoc[]
  export function fromDocument(doc: RuleDoc[], roots: Root[]): Rule[]
  export function renumber(rules: Rule[]): Rule[]
  export function moved<T>(list: T[], from: number, to: number): T[]
  export function blankRule(ordinal: number): Rule
  ```

- [ ] **Step 1: Teach `sendJson` about `PUT`**

In `web/src/api.ts`, line 129:

```ts
export async function sendJson<T>(
  path: string,
  method: 'POST' | 'PUT' | 'DELETE' | 'PATCH',
  body?: unknown,
): Promise<T | undefined> {
```

- [ ] **Step 2: Write the failing tests**

Create `web/src/rules.test.ts`:

```ts
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { Root } from './config'
import {
  blankRule,
  fromDocument,
  MATCH_ONS,
  moved,
  previewRules,
  renumber,
  type Rule,
  type RuleDoc,
  RULE_KINDS,
  saveRules,
  toDocument,
} from './rules'

const rule = (over: Partial<Rule> = {}): Rule => ({
  id: 1,
  ordinal: 1,
  enabled: true,
  root_id: null,
  kind: 'regex',
  pattern: 'Manga \\(RAW\\)/.*',
  match_on: 'path',
  set_series: null,
  set_edition_key: 'raw',
  set_kind: null,
  set_language: null,
  note: null,
  ...over,
})

const root = (over: Partial<Root> = {}): Root => ({
  id: 1,
  path: '/srv/manga',
  edition_key: 'en',
  default_kind: 'manga',
  default_language: 'en',
  scan_interval_secs: 86400,
  keeps_documents: false,
  last_scan_at: null,
  last_scan_status: null,
  ...over,
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('the wire vocabularies', () => {
  it('spells the four rule kinds the store parses', () => {
    // `rules.rs`'s own `FromStr` — a fifth value here is a 400 the editor
    // would offer in a dropdown.
    expect([...RULE_KINDS]).toEqual([
      'prefix',
      'regex',
      'builtin-volume',
      'builtin-chapter',
    ])
  })

  it('spells the three match_on values the store parses', () => {
    expect([...MATCH_ONS]).toEqual(['path', 'name', 'parent'])
  })
})

describe('the export/import converter', () => {
  it('writes a root path, never a root id', () => {
    // A document carried between libraries cannot mean anything by a bare
    // number — the build design's §5.6.
    const doc = toDocument([rule({ root_id: 1 })], [root()])
    expect(doc[0].root_path).toBe('/srv/manga')
    expect('root_id' in doc[0]).toBe(false)
    expect('id' in doc[0]).toBe(false)
  })

  it('writes null for a rule scoped to every root', () => {
    expect(toDocument([rule({ root_id: null })], [root()])[0].root_path).toBe(
      null,
    )
  })

  it('round-trips through one converter, so an export and an import cannot disagree', () => {
    const roots = [root(), root({ id: 2, path: '/srv/raw' })]
    const rules = [
      rule({ id: 3, ordinal: 1, root_id: 2 }),
      rule({ id: 4, ordinal: 2, root_id: null, kind: 'prefix' }),
    ]
    const back = fromDocument(toDocument(rules, roots), roots)
    expect(back.map((r) => r.root_id)).toEqual([2, null])
    expect(back.map((r) => r.kind)).toEqual(['regex', 'prefix'])
  })

  it('reads an imported rule back as unsaved', () => {
    // `id: 0` is the store's own sentinel for "not written yet". An imported
    // document carries none of this library's ids and must not invent one.
    const doc: RuleDoc[] = [
      {
        ordinal: 1,
        enabled: true,
        root_path: '/srv/manga',
        kind: 'regex',
        pattern: '.*',
        match_on: 'path',
        set_edition_key: null,
        set_kind: null,
        set_language: null,
        set_series: null,
        note: null,
      },
    ]
    expect(fromDocument(doc, [root()])[0].id).toBe(0)
  })

  it('refuses a root path this library does not have, naming it', () => {
    const doc: RuleDoc[] = [
      {
        ordinal: 1,
        enabled: true,
        root_path: '/somewhere/else',
        kind: 'regex',
        pattern: '.*',
        match_on: 'path',
        set_edition_key: null,
        set_kind: null,
        set_language: null,
        set_series: null,
        note: null,
      },
    ]
    expect(() => fromDocument(doc, [root()])).toThrow('/somewhere/else')
  })
})

describe('reordering', () => {
  it('moves one entry and closes the gap behind it', () => {
    expect(moved(['a', 'b', 'c', 'd'], 0, 2)).toEqual(['b', 'c', 'a', 'd'])
    expect(moved(['a', 'b', 'c', 'd'], 3, 1)).toEqual(['a', 'd', 'b', 'c'])
  })

  it('leaves the list alone when nothing moves', () => {
    expect(moved(['a', 'b', 'c'], 1, 1)).toEqual(['a', 'b', 'c'])
  })

  it('renumbers ordinals to the list position, one-based', () => {
    // Layering is positional: `Engine::compile` orders by `ordinal` then id,
    // so a drag that changed the array and not the ordinals would preview
    // exactly as it did before it.
    const out = renumber([rule({ ordinal: 9 }), rule({ id: 2, ordinal: 3 })])
    expect(out.map((r) => r.ordinal)).toEqual([1, 2])
  })
})

describe('a blank rule', () => {
  it('carries the unsaved sentinel and the given ordinal', () => {
    const fresh = blankRule(4)
    expect(fresh.id).toBe(0)
    expect(fresh.ordinal).toBe(4)
    expect(fresh.enabled).toBe(true)
    expect(RULE_KINDS).toContain(fresh.kind)
    expect(MATCH_ONS).toContain(fresh.match_on)
  })
})

describe('the client', () => {
  it('sends the whole proposed list to the preview route', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          impact: {},
          rows: [],
          rows_truncated: false,
          conflicts: [],
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      ),
    )

    await previewRules([rule()], 0)

    const [path, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(path).toBe('/api/library/rules/preview')
    expect(init.method).toBe('POST')
    expect(JSON.parse(init.body as string)).toEqual({
      rules: [rule()],
      selected: 0,
    })
  })

  it('omits `selected` entirely when no rule is open', async () => {
    // The route reads absent and null the same way, but sending `selected`
    // for nothing open would make the server build a row table nobody asked
    // for — one `scan_entry` walk and a second Engine, per `preview_with_rows`.
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          impact: {},
          rows: [],
          rows_truncated: false,
          conflicts: [],
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      ),
    )

    await previewRules([rule()])

    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(JSON.parse(init.body as string)).toEqual({ rules: [rule()] })
  })

  it('saves with PUT, to the collection URL', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          impact: {},
          rules: [],
          reopened_proposals: [],
          complaints: [],
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      ),
    )

    await saveRules([rule()])

    const [path, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(path).toBe('/api/library/rules')
    expect(init.method).toBe('PUT')
    expect(JSON.parse(init.body as string)).toEqual({ rules: [rule()] })
  })

  it('throws the server’s own sentence when a save is refused', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response('rule 9999: no such rule exists to keep', { status: 400 }),
    )
    await expect(saveRules([rule()])).rejects.toThrow('no such rule exists')
  })
})
```

- [ ] **Step 3: Run and watch it fail**

```bash
cd /home/davey/projects/taiyaku/web && pnpm vitest run --project node src/rules.test.ts 2>&1 | tail -20
```

Expected: `Failed to resolve import "./rules"`.

- [ ] **Step 4: Write `web/src/rules.ts`**

```ts
/**
 * The `/api/library/rules*` API and the rule editor's pure helpers — spec C4
 * §1 and §6, over the routes that shipped in #105 and PR B's own `PUT`.
 *
 * **Separate from `config.ts`** for the reason `config.ts` gives for being
 * separate from `library.ts`: the two share a session and nothing else. The
 * one thing they do share is `Root`, which this module reads to convert a
 * root id to the path an export document has to carry instead.
 *
 * **The `root_path` ↔ `root_id` conversion is one pair of functions used in
 * both directions**, so an export and an import cannot disagree about the
 * mapping — the build design's §5.6. `POST /api/library/rules/import` takes a
 * bare array of documents that name a root by *path*, because a document
 * carried between libraries cannot mean anything by a bare number.
 *
 * **Nothing here validates a pattern.** A regex that compiles in JS is not the
 * regex the store compiles, and the server owns the answer — the build
 * design's §8. What a bad pattern produces is a `complaints` entry from the
 * save, which is the only route that reports one.
 */
import { getJson, sendJson } from './api'
import { type Root } from './config'

/** The four values `rule.kind` holds — `rules.rs`'s own `FromStr`. */
export const RULE_KINDS = [
  'prefix',
  'regex',
  'builtin-volume',
  'builtin-chapter',
] as const

/** Which of a `rel_path`'s three derived strings a rule is tested against. */
export const MATCH_ONS = ['path', 'name', 'parent'] as const

/**
 * Where the editor says a preview is getting expensive.
 *
 * **Forty, and it is a guess.** The conflicts term is quadratic in the number
 * of proposed rules — measured in a debug build on a one-work fixture: 10
 * extra rules 47 ms, 40 → 401 ms, 80 → 1.34 s, 160 → 5.01 s. This is anchored
 * to that curve's knee, not to a measurement of this screen. The preview route
 * itself refuses past 200.
 */
export const PREVIEW_EXPENSIVE_ABOVE = 40

/** One `rule` row, as `GET /api/library/rules` sends it. */
export interface Rule {
  /** `0` is the store's own sentinel for a rule the editor has not saved yet. */
  id: number
  ordinal: number
  enabled: boolean
  /** `null` scopes the rule to every root. */
  root_id: number | null
  kind: string
  pattern: string
  match_on: string
  set_series: string | null
  set_edition_key: string | null
  set_kind: string | null
  set_language: string | null
  note: string | null
}

/** What a proposed list would change — the Impact tab. */
export interface Impact {
  series_before: number
  series_after: number
  series_merged: number
  works_rehomed: number
  works_orphaned: number
  /**
   * Proposals a derivation would put back in front of someone. **Not
   * `proposals_resolved`, which is not reportable** — a derivation reaches one
   * proposal writer and it only ever raises or reopens. The build design's §5.5.
   */
  proposals_reopened: number
  proposals_raised: number
}

/** Where one member sits. */
export interface Placement {
  series: string
  work: string
  edition: string
}

/**
 * Which rule decided one field — **a position in the request's own `rules`
 * array, never a rule id.** The store holds this boundary deliberately: an
 * unsaved rule carries `id: 0`, which is indistinguishable from a second
 * unsaved rule and from the root-fallback sentinel. Render the rule at
 * `index`, never `rules[index].id`.
 */
export type Decided = { kind: 'rule'; index: number } | { kind: 'root_default' }

/** One `scan_entry` the selected rule's scope reaches. */
export interface Row {
  rel_path: string
  root_id: number
  before: Placement | null
  after: Placement | null
  /** `false` is a near-miss, which is the point of the table — not an absence. */
  matched: boolean
  decided_by: Record<string, Decided>
}

/**
 * One field two enabled rules would decide differently.
 *
 * `first` and `wins` are positions in the request's `rules` array, the same
 * boundary `Decided` holds. `others` counts the disagreements `first` does not
 * name, so a chain of three or more reads "and 2 others" rather than implying
 * it is the whole story.
 */
export interface Conflict {
  field: string
  rel_path: string
  first: number
  wins: number
  others: number
  examples: number
}

/** `POST /api/library/rules/preview`'s answer. */
export interface Preview {
  impact: Impact
  rows: Row[]
  /** Whether more rows existed than the route's 500-row cap. Drawn, not swallowed. */
  rows_truncated: boolean
  conflicts: Conflict[]
}

/** `PUT /api/library/rules`'s answer. */
export interface Saved {
  impact: Impact
  rules: Rule[]
  reopened_proposals: number[]
  complaints: string[]
}

/**
 * One rule as the export document carries it — **`root_path`, not `root_id`,
 * and no `id` at all.**
 */
export interface RuleDoc {
  ordinal: number
  enabled: boolean
  root_path: string | null
  kind: string
  pattern: string
  match_on: string
  set_edition_key: string | null
  set_kind: string | null
  set_language: string | null
  set_series: string | null
  note: string | null
}

// --- the client -----------------------------------------------------------

/** `config.ts`'s own helper, for the reason its doc comment gives. */
async function expectBody<T>(sent: Promise<T | undefined>): Promise<T> {
  const body = await sent
  if (body === undefined)
    throw new Error('expected a JSON body, got an empty 204')
  return body
}

export const listRules = () =>
  getJson<{ rules: Rule[]; complaints: string[] }>('/api/library/rules')

/**
 * What the whole proposed list would do, plus which pairs of it disagree.
 *
 * **`selected` is omitted rather than sent as null when nothing is open.**
 * The route reads both the same way, but `preview_with_rows` does no
 * row-building work at all for `None` — no `scan_entry` walk, no second
 * `Engine` — and a key present is a key somebody later reads as intent.
 */
export const previewRules = (rules: Rule[], selected?: number) =>
  expectBody(
    sendJson<Preview>('/api/library/rules/preview', 'POST', {
      rules,
      ...(selected === undefined ? {} : { selected }),
    }),
  )

/**
 * Save: replace the rule set with `rules` **and derive over the library**.
 *
 * The only route in this product that applies a rule. Not `importRules`, which
 * mints fresh ids by design and so reopens every accepted proposal whose rule
 * it replaces — correct for a foreign document, wrong for a save.
 */
export const saveRules = (rules: Rule[]) =>
  expectBody(sendJson<Saved>('/api/library/rules', 'PUT', { rules }))

/**
 * Replace the whole set from an export document. **Destructive**: fresh ids,
 * so every accepted proposal whose rule this replaces is reopened, and the
 * answer names them.
 */
export const importRules = (doc: string) =>
  expectBody(
    sendJson<{ rules: Rule[]; complaints: string[]; reopened_proposals: number[] }>(
      '/api/library/rules/import',
      'POST',
      JSON.parse(doc) as unknown,
    ),
  )

// --- the document converter, used in both directions ----------------------

/** Rules as the export document carries them. */
export function toDocument(rules: Rule[], roots: Root[]): RuleDoc[] {
  const pathOf = new Map(roots.map((r) => [r.id, r.path]))
  return rules.map((rule) => {
    if (rule.root_id !== null && !pathOf.has(rule.root_id))
      throw new Error(
        `rule ${rule.ordinal} names root ${rule.root_id}, which this library does not have`,
      )
    return {
      ordinal: rule.ordinal,
      enabled: rule.enabled,
      root_path: rule.root_id === null ? null : (pathOf.get(rule.root_id) ?? null),
      kind: rule.kind,
      pattern: rule.pattern,
      match_on: rule.match_on,
      set_edition_key: rule.set_edition_key,
      set_kind: rule.set_kind,
      set_language: rule.set_language,
      set_series: rule.set_series,
      note: rule.note,
    }
  })
}

/**
 * An export document read back against this library's roots.
 *
 * **Every rule comes back `id: 0`.** A document carries none of this library's
 * ids, and inventing one is exactly what would make a save claim to keep a row
 * it has never seen.
 */
export function fromDocument(doc: RuleDoc[], roots: Root[]): Rule[] {
  const idOf = new Map(roots.map((r) => [r.path, r.id]))
  return doc.map((entry) => {
    if (entry.root_path !== null && !idOf.has(entry.root_path))
      throw new Error(
        `this document names the root ${entry.root_path}, which this library does not have`,
      )
    return {
      id: 0,
      ordinal: entry.ordinal,
      enabled: entry.enabled,
      root_id: entry.root_path === null ? null : (idOf.get(entry.root_path) ?? null),
      kind: entry.kind,
      pattern: entry.pattern,
      match_on: entry.match_on,
      set_series: entry.set_series,
      set_edition_key: entry.set_edition_key,
      set_kind: entry.set_kind,
      set_language: entry.set_language,
      note: entry.note,
    }
  })
}

// --- pure helpers ---------------------------------------------------------

/** `list` with the entry at `from` moved to `to`. */
export function moved<T>(list: T[], from: number, to: number): T[] {
  const out = [...list]
  const [entry] = out.splice(from, 1)
  out.splice(to, 0, entry)
  return out
}

/**
 * Ordinals set to list position, one-based.
 *
 * **A drag that changed the array and not the ordinals would preview exactly
 * as it did before it.** Layering is positional — `Engine::compile` orders by
 * `ordinal` then id — so the array's order means nothing to the server until
 * it is written into the column the server reads.
 */
export function renumber(rules: Rule[]): Rule[] {
  return rules.map((rule, index) => ({ ...rule, ordinal: index + 1 }))
}

/** A rule the editor has added and not saved. */
export function blankRule(ordinal: number): Rule {
  return {
    id: 0,
    ordinal,
    enabled: true,
    root_id: null,
    kind: 'regex',
    pattern: '',
    match_on: 'path',
    set_series: null,
    set_edition_key: null,
    set_kind: null,
    set_language: null,
    note: null,
  }
}
```

- [ ] **Step 5: Run and watch it pass**

```bash
cd /home/davey/projects/taiyaku/web && pnpm vitest run --project node src/rules.test.ts 2>&1 | tail -8
```

Expected: all tests passing.

- [ ] **Step 6: Commit**

```bash
cd /home/davey/projects/taiyaku && git add web/src/api.ts web/src/rules.ts web/src/rules.test.ts && git commit -m "web: the rules client, and one converter both directions of the document use"
```

---

## Task 5: `RuleForm.tsx` — one rule's fields

**Files:**
- Create: `web/src/RuleForm.tsx`
- Test: `web/src/RuleForm.test.tsx` (the `jsdom` project — `src/**/*.test.tsx`)

**Interfaces:**
- Consumes: `Rule`, `RULE_KINDS`, `MATCH_ONS` from `./rules`; `Root` from `./config`.
- Produces:
  ```tsx
  export function RuleForm(props: {
    rule: Rule
    roots: Root[]
    onCommit: (rule: Rule) => void
  }): React.ReactElement
  ```
  `onCommit` fires on **blur or Enter** with the whole edited rule — never on
  keystroke. Task 8 passes a handler that sets state and previews.

- [ ] **Step 1: Write the failing test**

Create `web/src/RuleForm.test.tsx`:

```tsx
import { act } from 'react'
import { describe, expect, it, vi } from 'vitest'
import type { Root } from './config'
import { RuleForm } from './RuleForm'
import type { Rule } from './rules'
import { mount } from './testing/mount'

const rule = (over: Partial<Rule> = {}): Rule => ({
  id: 1,
  ordinal: 1,
  enabled: true,
  root_id: null,
  kind: 'regex',
  pattern: 'Manga \\(RAW\\)/.*',
  match_on: 'path',
  set_series: null,
  set_edition_key: 'raw',
  set_kind: null,
  set_language: null,
  note: null,
  ...over,
})

const roots: Root[] = [
  {
    id: 1,
    path: '/srv/manga',
    edition_key: 'en',
    default_kind: 'manga',
    default_language: 'en',
    scan_interval_secs: 86400,
    keeps_documents: false,
    last_scan_at: null,
    last_scan_status: null,
  },
]

function render(props: Partial<Parameters<typeof RuleForm>[0]> = {}) {
  const onCommit = vi.fn()
  let host: HTMLElement | undefined
  act(() => {
    host = mount(
      <RuleForm rule={rule()} roots={roots} onCommit={onCommit} {...props} />,
    )
  })
  if (!host) throw new Error('nothing mounted')
  return { host, onCommit }
}

function typeInto(input: HTMLInputElement, value: string) {
  act(() => {
    const setter = Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      'value',
    )?.set
    setter?.call(input, value)
    input.dispatchEvent(new Event('input', { bubbles: true }))
  })
}

describe('RuleForm', () => {
  it('draws the rule’s current values', () => {
    const { host } = render()
    const pattern = host.querySelector<HTMLInputElement>('input[name="pattern"]')
    expect(pattern?.value).toBe('Manga \\(RAW\\)/.*')
    expect(
      host.querySelector<HTMLSelectElement>('select[name="kind"]')?.value,
    ).toBe('regex')
    expect(
      host.querySelector<HTMLInputElement>('input[name="set_edition_key"]')
        ?.value,
    ).toBe('raw')
  })

  it('offers every kind and match_on the store parses, and no others', () => {
    const { host } = render()
    const kinds = [
      ...host.querySelectorAll<HTMLOptionElement>('select[name="kind"] option'),
    ].map((o) => o.value)
    expect(kinds).toEqual(['prefix', 'regex', 'builtin-volume', 'builtin-chapter'])
    const matches = [
      ...host.querySelectorAll<HTMLOptionElement>(
        'select[name="match_on"] option',
      ),
    ].map((o) => o.value)
    expect(matches).toEqual(['path', 'name', 'parent'])
  })

  it('scopes to every root, or to one named root', () => {
    const { host } = render()
    const scope = host.querySelector<HTMLSelectElement>('select[name="root_id"]')
    const options = [...(scope?.options ?? [])].map((o) => o.value)
    expect(options).toEqual(['', '1'])
    expect(scope?.value).toBe('')
  })

  it('fires no commit while typing', () => {
    // The screen's central cost decision: a preview costs a median 281 ms and
    // a half-typed regex is a rule that matches nothing. The build design's §5.3.
    const { host, onCommit } = render()
    const pattern = host.querySelector<HTMLInputElement>(
      'input[name="pattern"]',
    ) as HTMLInputElement
    typeInto(pattern, 'M')
    typeInto(pattern, 'Ma')
    typeInto(pattern, 'Man')
    expect(onCommit).not.toHaveBeenCalled()
  })

  it('commits the whole edited rule on blur', () => {
    const { host, onCommit } = render()
    const pattern = host.querySelector<HTMLInputElement>(
      'input[name="pattern"]',
    ) as HTMLInputElement
    typeInto(pattern, 'Colour/.*')
    act(() => {
      pattern.dispatchEvent(new FocusEvent('blur', { bubbles: true }))
    })
    expect(onCommit).toHaveBeenCalledTimes(1)
    expect(onCommit.mock.calls[0][0]).toMatchObject({
      id: 1,
      pattern: 'Colour/.*',
      set_edition_key: 'raw',
    })
  })

  it('commits on Enter', () => {
    const { host, onCommit } = render()
    const pattern = host.querySelector<HTMLInputElement>(
      'input[name="pattern"]',
    ) as HTMLInputElement
    typeInto(pattern, 'Colour/.*')
    act(() => {
      pattern.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }),
      )
    })
    expect(onCommit).toHaveBeenCalledTimes(1)
    expect(onCommit.mock.calls[0][0].pattern).toBe('Colour/.*')
  })

  it('does not commit a blur that changed nothing', () => {
    // A click through the form is not five derivations.
    const { host, onCommit } = render()
    const pattern = host.querySelector<HTMLInputElement>(
      'input[name="pattern"]',
    ) as HTMLInputElement
    act(() => {
      pattern.dispatchEvent(new FocusEvent('blur', { bubbles: true }))
    })
    expect(onCommit).not.toHaveBeenCalled()
  })

  it('reads an emptied text field as null, not as an empty string', () => {
    // The columns are nullable and the store reads absent-or-null as "no
    // value". An empty string is a value, and it would set the field to "".
    const { host, onCommit } = render()
    const edition = host.querySelector<HTMLInputElement>(
      'input[name="set_edition_key"]',
    ) as HTMLInputElement
    typeInto(edition, '')
    act(() => {
      edition.dispatchEvent(new FocusEvent('blur', { bubbles: true }))
    })
    expect(onCommit.mock.calls[0][0].set_edition_key).toBe(null)
  })

  it('redraws when a different rule is selected', () => {
    const { host } = render()
    act(() => {
      mount(
        <RuleForm
          rule={rule({ id: 2, pattern: 'Other/.*' })}
          roots={roots}
          onCommit={vi.fn()}
        />,
        host,
      )
    })
    expect(
      host.querySelector<HTMLInputElement>('input[name="pattern"]')?.value,
    ).toBe('Other/.*')
  })
})
```

**Check `mount` first.** Run:

```bash
cd /home/davey/projects/taiyaku && sed -n '1,40p' web/src/testing/mount.tsx 2>/dev/null || ls web/src/testing/
```

Use whatever signature `mount` actually has. If it does not accept a second
"remount into this host" argument, drop the last test and instead assert the
redraw through a small stateful wrapper, the way another `*.test.tsx` in this
repo already does.

- [ ] **Step 2: Run and watch it fail**

```bash
cd /home/davey/projects/taiyaku/web && pnpm vitest run --project jsdom src/RuleForm.test.tsx 2>&1 | tail -15
```

Expected: `Failed to resolve import "./RuleForm"`.

- [ ] **Step 3: Write `web/src/RuleForm.tsx`**

```tsx
/**
 * One rule's fields — spec C4 §1's right-hand pane, top half.
 *
 * **It commits on blur or Enter, never on keystroke.** A preview costs a
 * median 281 ms on a real library and the derivation is the entire cost, so
 * deriving to answer a question about a pattern nobody has finished writing
 * holds the write lock repeatedly for nothing — and a half-typed regex is a
 * rule that matches nothing, so the answer would be wrong as well as
 * expensive. The build design's §5.3, where the alternatives (a debounce, an
 * explicit Preview button) are weighed and refused.
 *
 * **A blur that changed nothing commits nothing.** Tabbing through the form
 * is one gesture to a person and would otherwise be six derivations.
 *
 * **The fields are local state seeded from the prop, keyed on the rule.** The
 * whole point of committing on blur is that the parent does not hold every
 * keystroke; that means this component does, and a `key` on the instance is
 * what makes selecting a different rule redraw rather than keep the last
 * one's half-typed text.
 *
 * **An emptied text field reads as `null`, not `""`.** Every `set_*` column is
 * nullable and the store reads absent-or-null as "this rule sets nothing
 * here"; an empty string is a value, and it would set the field to the empty
 * string for every entry the rule matches.
 *
 * **No pattern validation.** A regex that compiles in JS is not the regex the
 * store compiles — the build design's §8. What a bad pattern produces is a
 * `complaints` entry from the save.
 */
import { useState } from 'react'
import { type Root } from './config'
import { MATCH_ONS, type Rule, RULE_KINDS } from './rules'

/** `""` from a text input is "nothing set", never the empty string. */
const orNull = (value: string): string | null => (value === '' ? null : value)

export function RuleForm({
  rule,
  roots,
  onCommit,
}: {
  rule: Rule
  roots: Root[]
  onCommit: (rule: Rule) => void
}) {
  const [draft, setDraft] = useState<Rule>(rule)

  const commit = () => {
    // A blur or an Enter that changed nothing is not a preview. Compared by
    // value against the prop rather than tracked with a dirty flag, so a field
    // edited back to what it was is also not one.
    if (JSON.stringify(draft) === JSON.stringify(rule)) return
    onCommit(draft)
  }

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      e.preventDefault()
      commit()
    }
  }

  const text = (
    name: 'pattern' | 'set_series' | 'set_edition_key' | 'set_kind' | 'set_language' | 'note',
    label: string,
  ) => (
    <label>
      <span>{label}</span>
      <input
        type="text"
        name={name}
        value={draft[name] ?? ''}
        onChange={(e) =>
          setDraft((held) => ({
            ...held,
            [name]: name === 'pattern' ? e.target.value : orNull(e.target.value),
          }))
        }
        onBlur={commit}
        onKeyDown={onKeyDown}
      />
    </label>
  )

  return (
    <div className="rule-form">
      <label>
        <span>kind</span>
        <select
          name="kind"
          value={draft.kind}
          onChange={(e) => {
            const kind = e.target.value
            setDraft((held) => ({ ...held, kind }))
            onCommit({ ...draft, kind })
          }}
        >
          {RULE_KINDS.map((kind) => (
            <option key={kind} value={kind}>
              {kind}
            </option>
          ))}
        </select>
      </label>

      <label>
        <span>match on</span>
        <select
          name="match_on"
          value={draft.match_on}
          onChange={(e) => {
            const match_on = e.target.value
            setDraft((held) => ({ ...held, match_on }))
            onCommit({ ...draft, match_on })
          }}
        >
          {MATCH_ONS.map((on) => (
            <option key={on} value={on}>
              {on}
            </option>
          ))}
        </select>
      </label>

      {text('pattern', 'pattern')}

      <fieldset className="rule-sets">
        <legend>sets</legend>
        {text('set_series', 'series')}
        {text('set_edition_key', 'edition')}
        {text('set_kind', 'kind')}
        {text('set_language', 'language')}
      </fieldset>

      {text('note', 'note')}

      <label>
        <span>scope</span>
        <select
          name="root_id"
          value={draft.root_id === null ? '' : String(draft.root_id)}
          onChange={(e) => {
            const root_id = e.target.value === '' ? null : Number(e.target.value)
            setDraft((held) => ({ ...held, root_id }))
            onCommit({ ...draft, root_id })
          }}
        >
          <option value="">every root</option>
          {roots.map((root) => (
            <option key={root.id} value={String(root.id)}>
              {root.path}
            </option>
          ))}
        </select>
      </label>
    </div>
  )
}
```

- [ ] **Step 4: Run and watch it pass**

```bash
cd /home/davey/projects/taiyaku/web && pnpm vitest run --project jsdom src/RuleForm.test.tsx 2>&1 | tail -8
```

Expected: all passing.

- [ ] **Step 5: Commit**

```bash
cd /home/davey/projects/taiyaku && git add web/src/RuleForm.tsx web/src/RuleForm.test.tsx && git commit -m "web: the rule form, committing on blur and Enter and never on keystroke"
```

---

## Task 6: `RuleList.tsx` — the ordered list

**Files:**
- Create: `web/src/RuleList.tsx`
- Test: `web/src/RuleList.test.tsx`

**Interfaces:**
- Consumes: `Rule` from `./rules`; `Proposal` from `./curation`.
- Produces:
  ```tsx
  export function RuleList(props: {
    rules: Rule[]
    selected: number | undefined
    proposals: Proposal[]
    conflictCount: number
    onSelect: (index: number) => void
    onToggle: (index: number) => void
    onReorder: (from: number, to: number) => void
    onAdd: () => void
    onDelete: (index: number) => void
  }): React.ReactElement
  ```

- [ ] **Step 1: Write the failing test**

Create `web/src/RuleList.test.tsx`:

```tsx
import { act } from 'react'
import { describe, expect, it, vi } from 'vitest'
import type { Proposal } from './curation'
import { RuleList } from './RuleList'
import type { Rule } from './rules'
import { mount } from './testing/mount'

const rule = (over: Partial<Rule> = {}): Rule => ({
  id: 1,
  ordinal: 1,
  enabled: true,
  root_id: null,
  kind: 'regex',
  pattern: 'Manga/.*',
  match_on: 'path',
  set_series: 'Series',
  set_edition_key: null,
  set_kind: null,
  set_language: null,
  note: null,
  ...over,
})

const proposal = (over: Partial<Proposal> = {}): Proposal => ({
  id: 10,
  kind: 'same-series',
  subject_id: 1,
  target_id: 2,
  score: 0.9,
  reason: 'two spellings of one title',
  state: 'accepted',
  created_rule_id: 2,
  ...over,
})

function render(props: Partial<Parameters<typeof RuleList>[0]> = {}) {
  const handlers = {
    onSelect: vi.fn(),
    onToggle: vi.fn(),
    onReorder: vi.fn(),
    onAdd: vi.fn(),
    onDelete: vi.fn(),
  }
  let host: HTMLElement | undefined
  act(() => {
    host = mount(
      <RuleList
        rules={[rule(), rule({ id: 2, ordinal: 2, pattern: 'Colour/.*' })]}
        selected={0}
        proposals={[]}
        conflictCount={0}
        {...handlers}
        {...props}
      />,
    )
  })
  if (!host) throw new Error('nothing mounted')
  return { host, ...handlers }
}

describe('RuleList', () => {
  it('draws one row per rule, in list order, numbered from one', () => {
    const { host } = render()
    const rows = host.querySelectorAll('.rule-row')
    expect(rows).toHaveLength(2)
    expect(rows[0].textContent).toContain('1')
    expect(rows[1].textContent).toContain('2')
  })

  it('marks the selected rule', () => {
    const { host } = render({ selected: 1 })
    const rows = host.querySelectorAll('.rule-row')
    expect(rows[1].getAttribute('aria-current')).toBe('true')
    expect(rows[0].getAttribute('aria-current')).not.toBe('true')
  })

  it('selects by index, never by rule id', () => {
    // Two unsaved rules both carry `id: 0`; a list keyed on id could not tell
    // them apart, and `Decided`/`Conflict` name a position for the same reason.
    const { host, onSelect } = render({
      rules: [rule({ id: 0 }), rule({ id: 0, pattern: 'Second/.*' })],
    })
    act(() => {
      host
        .querySelectorAll<HTMLElement>('.rule-row button.select')[1]
        .click()
    })
    expect(onSelect).toHaveBeenCalledWith(1)
  })

  it('toggles enable from the row', () => {
    const { host, onToggle } = render()
    act(() => {
      host
        .querySelectorAll<HTMLInputElement>('.rule-row input[type="checkbox"]')[1]
        .click()
    })
    expect(onToggle).toHaveBeenCalledWith(1)
  })

  it('says which rules are off', () => {
    const { host } = render({
      rules: [rule(), rule({ id: 2, ordinal: 2, enabled: false })],
    })
    expect(host.querySelectorAll('.rule-row')[1].textContent).toContain('off')
  })

  it('reorders on drop, from the dragged index to the dropped one', () => {
    const { host, onReorder } = render()
    const rows = host.querySelectorAll<HTMLElement>('.rule-row')
    act(() => {
      rows[1].dispatchEvent(
        Object.assign(new Event('dragstart', { bubbles: true }), {
          dataTransfer: { setData: vi.fn(), effectAllowed: '' },
        }),
      )
      rows[0].dispatchEvent(
        Object.assign(new Event('drop', { bubbles: true }), {
          dataTransfer: { getData: () => '1' },
          preventDefault: () => {},
        }),
      )
    })
    expect(onReorder).toHaveBeenCalledWith(1, 0)
  })

  it('badges a rule an accepted proposal wrote', () => {
    // C4 §1 requires those marked. The mark comes from the proposals list's
    // own `created_rule_id`, which needs no server change.
    const { host } = render({ proposals: [proposal({ created_rule_id: 2 })] })
    const rows = host.querySelectorAll('.rule-row')
    expect(rows[0].querySelector('.from-proposal')).toBeNull()
    expect(rows[1].querySelector('.from-proposal')).not.toBeNull()
  })

  it('warns before deleting a rule a proposal wrote, naming the proposal', () => {
    const { host, onDelete } = render({
      proposals: [proposal({ id: 42, created_rule_id: 2 })],
    })
    act(() => {
      host.querySelectorAll<HTMLElement>('.rule-row button.delete')[1].click()
    })
    expect(onDelete).not.toHaveBeenCalled()
    expect(host.textContent).toContain('42')
    act(() => {
      host.querySelector<HTMLElement>('button.confirm-delete')?.click()
    })
    expect(onDelete).toHaveBeenCalledWith(1)
  })

  it('deletes a rule no proposal wrote without a confirmation', () => {
    const { host, onDelete } = render()
    act(() => {
      host.querySelectorAll<HTMLElement>('.rule-row button.delete')[0].click()
    })
    expect(onDelete).toHaveBeenCalledWith(0)
  })

  it('adds a rule', () => {
    const { host, onAdd } = render()
    act(() => {
      host.querySelector<HTMLElement>('button.add-rule')?.click()
    })
    expect(onAdd).toHaveBeenCalled()
  })

  it('draws the rule count, and says nothing about cost below the threshold', () => {
    const { host } = render()
    expect(host.textContent).toContain('2 rules')
    expect(host.textContent).not.toContain('expensive')
  })

  it('says a preview is getting expensive past forty rules', () => {
    const many = Array.from({ length: 41 }, (_, i) =>
      rule({ id: i + 1, ordinal: i + 1 }),
    )
    const { host } = render({ rules: many })
    expect(host.textContent).toContain('expensive')
  })

  it('draws the conflict count as a badge', () => {
    const { host } = render({ conflictCount: 2 })
    expect(host.querySelector('.conflict-badge')?.textContent).toContain('2')
  })
})
```

- [ ] **Step 2: Run and watch it fail**

```bash
cd /home/davey/projects/taiyaku/web && pnpm vitest run --project jsdom src/RuleList.test.tsx 2>&1 | tail -15
```

Expected: `Failed to resolve import "./RuleList"`.

- [ ] **Step 3: Write `web/src/RuleList.tsx`**

```tsx
/**
 * The ordered rule list — spec C4 §1's left-hand pane.
 *
 * **Everything here addresses a rule by its position, never by its id.** Two
 * rules the editor has added and not saved both carry `id: 0` — the store's
 * own sentinel — so an id cannot tell them apart, and `preview::Decided` and
 * `preview::Conflict` hold the same boundary in the store for the same
 * reason. `key` is the index too, which is normally the wrong answer in React
 * and is the right one here: the identity of a row in this list *is* its
 * position, and reordering is the operation the list exists to support.
 *
 * **Order is the layering.** `Engine::compile` orders by `ordinal` then id, so
 * a drag that moved the array without renumbering would preview exactly as it
 * did before it — `rules.ts`'s `renumber` is what the screen calls, and this
 * component only reports which index moved where.
 *
 * **Deleting a rule an accepted proposal wrote reopens that proposal**, so it
 * asks first and names the proposal. C4 §1 requires the mark and the
 * reopening; the mark is joined from `GET /api/library/proposals`'s own
 * `created_rule_id`, which needs no server change (the build design's §5.5).
 * A rule no proposal wrote deletes without a dialog — nothing is written until
 * Save, so an ordinary delete is undoable by not saving.
 */
import { useState } from 'react'
import { type Proposal } from './curation'
import { PREVIEW_EXPENSIVE_ABOVE, type Rule } from './rules'

export function RuleList({
  rules,
  selected,
  proposals,
  conflictCount,
  onSelect,
  onToggle,
  onReorder,
  onAdd,
  onDelete,
}: {
  rules: Rule[]
  selected: number | undefined
  proposals: Proposal[]
  conflictCount: number
  onSelect: (index: number) => void
  onToggle: (index: number) => void
  onReorder: (from: number, to: number) => void
  onAdd: () => void
  onDelete: (index: number) => void
}) {
  /** Which index is awaiting a confirmed delete, and the proposal it would reopen. */
  const [confirming, setConfirming] = useState<
    { index: number; proposal: number } | undefined
  >()

  /** The accepted proposal that wrote each rule, by rule id. Never by index. */
  const wroteRule = new Map(
    proposals
      .filter((p) => p.created_rule_id !== null)
      .map((p) => [p.created_rule_id as number, p.id]),
  )

  const askOrDelete = (index: number) => {
    const rule = rules[index]
    const proposal = rule.id === 0 ? undefined : wroteRule.get(rule.id)
    if (proposal === undefined) {
      onDelete(index)
      return
    }
    setConfirming({ index, proposal })
  }

  return (
    <div className="rule-list">
      <ol>
        {rules.map((rule, index) => (
          // The index *is* the key here — see this file's doc comment. No
          // suppression comment: `noArrayIndexKey` is deliberately off in
          // `web/biome.jsonc`, where "off" is the decision rather than an open
          // question, and `App.tsx` already keys a list this way.
          <li
            key={index}
            className="rule-row"
            aria-current={selected === index ? 'true' : undefined}
            draggable
            onDragStart={(e) => {
              e.dataTransfer.setData('text/plain', String(index))
              e.dataTransfer.effectAllowed = 'move'
            }}
            onDragOver={(e) => e.preventDefault()}
            onDrop={(e) => {
              e.preventDefault()
              const from = Number(e.dataTransfer.getData('text/plain'))
              if (Number.isNaN(from) || from === index) return
              onReorder(from, index)
            }}
          >
            <span className="ordinal">{index + 1}</span>
            <input
              type="checkbox"
              checked={rule.enabled}
              aria-label={`rule ${index + 1} enabled`}
              onChange={() => onToggle(index)}
            />
            <button type="button" className="select" onClick={() => onSelect(index)}>
              {rule.set_series ??
                rule.set_edition_key ??
                rule.set_kind ??
                rule.set_language ??
                rule.pattern ??
                'new rule'}
            </button>
            {!rule.enabled && <span className="off">off</span>}
            {rule.id !== 0 && wroteRule.has(rule.id) && (
              <span className="from-proposal" title="written by an accepted proposal">
                from a proposal
              </span>
            )}
            <button
              type="button"
              className="delete"
              aria-label={`delete rule ${index + 1}`}
              onClick={() => askOrDelete(index)}
            >
              Delete
            </button>
          </li>
        ))}
      </ol>

      {confirming !== undefined && (
        <div className="confirm" role="alertdialog">
          <p>
            Rule {confirming.index + 1} was written by proposal{' '}
            {confirming.proposal}. Deleting it reopens that proposal — the
            question comes back to the inbox.
          </p>
          <button
            type="button"
            className="confirm-delete"
            onClick={() => {
              onDelete(confirming.index)
              setConfirming(undefined)
            }}
          >
            Delete and reopen
          </button>
          <button type="button" onClick={() => setConfirming(undefined)}>
            Keep it
          </button>
        </div>
      )}

      <p className="rule-count">
        {rules.length} rules
        {conflictCount > 0 && (
          <span className="conflict-badge">{conflictCount} conflicts</span>
        )}
      </p>
      {rules.length > PREVIEW_EXPENSIVE_ABOVE && (
        <p className="cost">
          Past {PREVIEW_EXPENSIVE_ABOVE} rules the preview gets expensive —
          conflict detection compares every pair, so the cost rises faster than
          the list does.
        </p>
      )}

      <button type="button" className="add-rule" onClick={onAdd}>
        + Add
      </button>
    </div>
  )
}
```

- [ ] **Step 4: Run and watch it pass**

```bash
cd /home/davey/projects/taiyaku/web && pnpm vitest run --project jsdom src/RuleList.test.tsx 2>&1 | tail -8
```

Expected: all passing. If the drag test cannot construct a `DataTransfer` in
jsdom, keep the assertion and build the event with
`new CustomEvent('drop')` plus an assigned `dataTransfer` object — do not delete
the test; reordering is one of §7's named cases.

- [ ] **Step 5: Commit**

```bash
cd /home/davey/projects/taiyaku && git add web/src/RuleList.tsx web/src/RuleList.test.tsx && git commit -m "web: the rule list, addressing every rule by position and never by id"
```

---

## Task 7: `PreviewTabs.tsx` — four tabs

**Files:**
- Create: `web/src/PreviewTabs.tsx`
- Test: `web/src/PreviewTabs.test.tsx`

**Interfaces:**
- Consumes: `Preview`, `Rule`, `Row`, `Conflict`, `Impact` from `./rules`.
- Produces:
  ```tsx
  export function PreviewTabs(props: {
    preview: Preview | undefined
    rules: Rule[]
    selected: number | undefined
    busy: boolean
    onReorder: (from: number, to: number) => void
  }): React.ReactElement
  ```

- [ ] **Step 1: Write the failing test**

Create `web/src/PreviewTabs.test.tsx`:

```tsx
import { act } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { PreviewTabs } from './PreviewTabs'
import type { Conflict, Impact, Preview, Row, Rule } from './rules'
import { mount } from './testing/mount'

const impact = (over: Partial<Impact> = {}): Impact => ({
  series_before: 8,
  series_after: 2,
  series_merged: 6,
  works_rehomed: 71,
  works_orphaned: 0,
  proposals_reopened: 3,
  proposals_raised: 1,
  ...over,
})

const row = (over: Partial<Row> = {}): Row => ({
  rel_path: 'Manga (RAW)/Vol 1.cbz',
  root_id: 1,
  before: { series: 'Old', work: 'old-1', edition: 'en' },
  after: { series: 'New', work: 'new-1', edition: 'raw' },
  matched: true,
  decided_by: {
    series: { kind: 'rule', index: 0 },
    edition_key: { kind: 'root_default' },
  },
  ...over,
})

const conflict = (over: Partial<Conflict> = {}): Conflict => ({
  field: 'edition_key',
  rel_path: 'Manga (RAW)/Go-Toubun no Hanayome - Full Color',
  first: 0,
  wins: 1,
  others: 0,
  examples: 3,
  ...over,
})

const rules: Rule[] = [
  {
    id: 1,
    ordinal: 1,
    enabled: true,
    root_id: null,
    kind: 'regex',
    pattern: 'Manga \\(RAW\\)/.*',
    match_on: 'path',
    set_series: null,
    set_edition_key: 'raw',
    set_kind: null,
    set_language: null,
    note: null,
  },
  {
    id: 2,
    ordinal: 2,
    enabled: true,
    root_id: null,
    kind: 'regex',
    pattern: '.*Full Color$',
    match_on: 'path',
    set_series: null,
    set_edition_key: 'en-color',
    set_kind: null,
    set_language: null,
    note: null,
  },
]

const preview = (over: Partial<Preview> = {}): Preview => ({
  impact: impact(),
  rows: [row()],
  rows_truncated: false,
  conflicts: [],
  ...over,
})

function render(props: Partial<Parameters<typeof PreviewTabs>[0]> = {}) {
  const onReorder = vi.fn()
  let host: HTMLElement | undefined
  act(() => {
    host = mount(
      <PreviewTabs
        preview={preview()}
        rules={rules}
        selected={0}
        busy={false}
        onReorder={onReorder}
        {...props}
      />,
    )
  })
  if (!host) throw new Error('nothing mounted')
  const click = (label: string) => {
    act(() => {
      ;[...host!.querySelectorAll<HTMLElement>('.preview-tabs button')]
        .find((b) => b.textContent?.includes(label))
        ?.click()
    })
  }
  return { host, onReorder, click }
}

describe('PreviewTabs', () => {
  it('draws four tabs', () => {
    const { host } = render()
    const labels = [
      ...host.querySelectorAll<HTMLElement>('.preview-tabs button'),
    ].map((b) => b.textContent ?? '')
    expect(labels[0]).toContain('Impact')
    expect(labels[1]).toContain('Matched')
    expect(labels[2]).toContain('Provenance')
    expect(labels[3]).toContain('Conflicts')
  })

  it('badges the conflict count on the tab, from every tab', () => {
    // C4 §1 describes conflict detection as a warning; the server half calls
    // it a tab. It is a tab *with a count badge drawn from every tab*, which
    // is what a warning-that-is-a-tab otherwise loses: a conflict hidden
    // behind a tab nobody clicked.
    const { host } = render({ preview: preview({ conflicts: [conflict()] }) })
    expect(host.querySelector('.preview-tabs .badge')?.textContent).toContain(
      '1',
    )
  })

  it('draws the impact in C4’s own words', () => {
    const { host } = render()
    expect(host.textContent).toContain('6 series merged')
    expect(host.textContent).toContain('71 works re-homed')
    expect(host.textContent).toContain('0 orphaned')
    expect(host.textContent).toContain('3 proposals reopened')
    expect(host.textContent).toContain('1 proposal raised')
  })

  it('never draws “proposals resolved”', () => {
    // A derivation cannot produce it — the build design's §5.5. Drawing it as
    // a zero would be an invented figure.
    const { host } = render()
    expect(host.textContent?.toLowerCase()).not.toContain('resolved')
  })

  it('lists a matched file as before → after', () => {
    const { host, click } = render()
    click('Matched')
    expect(host.textContent).toContain('Manga (RAW)/Vol 1.cbz')
    expect(host.textContent).toContain('Old')
    expect(host.textContent).toContain('New')
  })

  it('marks a near-miss as a near-miss rather than omitting it', () => {
    const { host, click } = render({
      preview: preview({ rows: [row({ matched: false })] }),
    })
    click('Matched')
    expect(host.textContent).toContain('near miss')
  })

  it('says the table was capped, rather than stopping silently', () => {
    const { host, click } = render({
      preview: preview({ rows_truncated: true }),
    })
    click('Matched')
    expect(host.textContent).toContain('500')
  })

  it('names the deciding rule by its position, not its id', () => {
    // `Decided` is an index into the list the request carried. Rendering
    // `rules[index].id` would name a different thing, and for an unsaved rule
    // it would name `0`.
    const { host, click } = render()
    click('Provenance')
    expect(host.textContent).toContain('rule 1')
    expect(host.textContent).toContain('root default')
  })

  it('reads the same rows the matched tab does', () => {
    const { host, click } = render({ preview: preview({ rows: [] }) })
    click('Provenance')
    expect(host.textContent).toContain('No files')
  })

  it('names both rules of a conflict and offers reorder', () => {
    const { host, click, onReorder } = render({
      preview: preview({ conflicts: [conflict()] }),
    })
    click('Conflicts')
    expect(host.textContent).toContain('edition_key')
    expect(host.textContent).toContain('Full Color')
    act(() => {
      host.querySelector<HTMLElement>('button.reorder')?.click()
    })
    expect(onReorder).toHaveBeenCalledWith(1, 0)
  })

  it('says how many other rules disagree, when three or more do', () => {
    const { host, click } = render({
      preview: preview({ conflicts: [conflict({ others: 2 })] }),
    })
    click('Conflicts')
    expect(host.textContent).toContain('2 others')
  })

  it('says nothing extra in the ordinary two-rule case', () => {
    const { host, click } = render({
      preview: preview({ conflicts: [conflict({ others: 0 })] }),
    })
    click('Conflicts')
    expect(host.textContent).not.toContain('others')
  })

  it('says it is working rather than drawing a stale answer', () => {
    const { host } = render({ busy: true })
    expect(host.textContent).toContain('Previewing')
  })

  it('says there is nothing yet before the first preview', () => {
    const { host } = render({ preview: undefined })
    expect(host.textContent).toContain('No preview yet')
  })
})
```

- [ ] **Step 2: Run and watch it fail**

```bash
cd /home/davey/projects/taiyaku/web && pnpm vitest run --project jsdom src/PreviewTabs.test.tsx 2>&1 | tail -15
```

Expected: `Failed to resolve import "./PreviewTabs"`.

- [ ] **Step 3: Write `web/src/PreviewTabs.tsx`**

```tsx
/**
 * The preview's four tabs — spec C4 §1's right-hand pane, bottom half, and
 * the build design's §5.2.
 *
 * **Four, not three, and the fourth carries a badge drawn from every tab.**
 * C4 §1 names three tabs and describes conflict detection as a warning; the
 * server half calls it a tab. A warning-that-is-a-tab has the failure mode a
 * warning does not — a conflict hidden behind a tab nobody clicked — so the
 * count is on the tab strip itself, visible from wherever you are.
 *
 * **Matched files and Provenance read one array.** `rows` is scoped to the
 * selected rule and arrives in one request; the two tabs are two readings of
 * it — one down the file list, one across the seven fields.
 *
 * **Every rule named here is named by position.** `Decided`, `Conflict::first`
 * and `Conflict::wins` are indices into the list the request carried, and the
 * store holds that boundary deliberately: an unsaved rule carries `id: 0`,
 * indistinguishable from a second unsaved rule and from the root-fallback
 * sentinel. "Rule 3" on this screen means the third rule in the editor, which
 * is the rule the person is looking at.
 *
 * **"Proposals resolved" is not drawn, because it is not reportable.** C4 §1
 * lists it; a derivation reaches one proposal writer and it only ever raises
 * or reopens, so the number could only ever be zero. Drawing a structural zero
 * as a figure is the invented number this repository's own discipline refuses.
 *
 * **The row cap is drawn.** A table that quietly stopped at 500 reads as the
 * whole list.
 */
import { useState } from 'react'
import { type Preview, type Rule } from './rules'

const TABS = ['Impact', 'Matched files', 'Provenance', 'Conflicts'] as const
type TabName = (typeof TABS)[number]

/** The seven fields, in the order `rules::Field` declares them. */
const FIELDS = [
  'series',
  'edition_key',
  'kind',
  'language',
  'volume',
  'chapter',
  'title',
] as const

const plural = (n: number, one: string, many: string) =>
  `${n} ${n === 1 ? one : many}`

export function PreviewTabs({
  preview,
  rules,
  selected,
  busy,
  onReorder,
}: {
  preview: Preview | undefined
  rules: Rule[]
  selected: number | undefined
  busy: boolean
  onReorder: (from: number, to: number) => void
}) {
  const [tab, setTab] = useState<TabName>('Impact')
  const conflicts = preview?.conflicts ?? []

  return (
    <div className="preview">
      <nav className="preview-tabs" aria-label="Preview">
        {TABS.map((name) => (
          <button
            key={name}
            type="button"
            aria-current={tab === name ? 'true' : undefined}
            onClick={() => setTab(name)}
          >
            {name}
            {name === 'Conflicts' && conflicts.length > 0 && (
              <span className="badge">{conflicts.length}</span>
            )}
          </button>
        ))}
      </nav>

      {busy && <p className="busy">Previewing…</p>}

      {preview === undefined ? (
        <p className="empty">
          No preview yet. Pick a rule, or edit one — the preview runs when you
          finish a change rather than while you type.
        </p>
      ) : (
        <div className="preview-body">
          {tab === 'Impact' && (
            <ul className="impact">
              <li>{plural(preview.impact.series_merged, 'series merged', 'series merged')}</li>
              <li>{plural(preview.impact.works_rehomed, 'work re-homed', 'works re-homed')}</li>
              <li>{preview.impact.works_orphaned} orphaned</li>
              <li>
                {plural(
                  preview.impact.proposals_reopened,
                  'proposal reopened',
                  'proposals reopened',
                )}
              </li>
              <li>
                {plural(
                  preview.impact.proposals_raised,
                  'proposal raised',
                  'proposals raised',
                )}
              </li>
              <li className="series-count">
                {preview.impact.series_before} series before ·{' '}
                {preview.impact.series_after} after
              </li>
            </ul>
          )}

          {tab === 'Matched files' &&
            (preview.rows.length === 0 ? (
              <p className="empty">
                No files. {selected === undefined
                  ? 'Select a rule to see what it reaches.'
                  : 'This rule matched nothing and came near nothing.'}
              </p>
            ) : (
              <>
                <table className="matched">
                  <thead>
                    <tr>
                      <th>file</th>
                      <th>before</th>
                      <th>after</th>
                      <th>match</th>
                    </tr>
                  </thead>
                  <tbody>
                    {preview.rows.map((row) => (
                      <tr key={`${row.root_id}:${row.rel_path}`}>
                        <td>{row.rel_path}</td>
                        <td>{row.before?.series ?? '—'}</td>
                        <td>{row.after?.series ?? '—'}</td>
                        <td>{row.matched ? 'matched' : 'near miss'}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                {preview.rows_truncated && (
                  <p className="truncated">
                    The first 500 files only — more matched than fit here.
                  </p>
                )}
              </>
            ))}

          {tab === 'Provenance' &&
            (preview.rows.length === 0 ? (
              <p className="empty">No files to show provenance for.</p>
            ) : (
              <table className="provenance">
                <thead>
                  <tr>
                    <th>file</th>
                    {FIELDS.map((field) => (
                      <th key={field}>{field}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {preview.rows.map((row) => (
                    <tr key={`${row.root_id}:${row.rel_path}`}>
                      <td>{row.rel_path}</td>
                      {FIELDS.map((field) => {
                        const decided = row.decided_by[field]
                        return (
                          <td key={field}>
                            {decided === undefined
                              ? '—'
                              : decided.kind === 'root_default'
                                ? 'root default'
                                : `rule ${decided.index + 1}`}
                          </td>
                        )
                      })}
                    </tr>
                  ))}
                </tbody>
              </table>
            ))}

          {tab === 'Conflicts' &&
            (conflicts.length === 0 ? (
              <p className="empty">
                No two enabled rules decide a field differently. Two rules
                setting one field to the <em>same</em> value are not a conflict
                — the seeded defaults do that on purpose.
              </p>
            ) : (
              <ul className="conflicts">
                {conflicts.map((conflict, index) => (
                  <li key={`${conflict.field}:${conflict.rel_path}:${index}`}>
                    <p>
                      <strong>{conflict.field}</strong> on {conflict.rel_path}:
                      rule {conflict.first + 1} says{' '}
                      {rules[conflict.first]?.pattern ?? '—'}, rule{' '}
                      {conflict.wins + 1} wins
                      {conflict.others > 0 && ` — and ${conflict.others} others`}
                      . {conflict.examples} file
                      {conflict.examples === 1 ? '' : 's'} like this one.
                    </p>
                    <button
                      type="button"
                      className="reorder"
                      onClick={() => onReorder(conflict.wins, conflict.first)}
                    >
                      Move rule {conflict.wins + 1} above rule{' '}
                      {conflict.first + 1}
                    </button>
                  </li>
                ))}
              </ul>
            ))}
        </div>
      )}
    </div>
  )
}
```

- [ ] **Step 4: Run and watch it pass**

```bash
cd /home/davey/projects/taiyaku/web && pnpm vitest run --project jsdom src/PreviewTabs.test.tsx 2>&1 | tail -8
```

Expected: all passing.

- [ ] **Step 5: Commit**

```bash
cd /home/davey/projects/taiyaku && git add web/src/PreviewTabs.tsx web/src/PreviewTabs.test.tsx && git commit -m "web: the four preview tabs, with the conflict count drawn from every one"
```

---

## Task 8: `RuleEditor.tsx`, and the tab that makes it the landing screen

**Files:**
- Create: `web/src/RuleEditor.tsx`
- Test: `web/src/RuleEditor.test.tsx`
- Modify: `web/src/config.ts:36` (`TABS`)
- Modify: `web/src/Config.tsx:30-32` (`SCREENS`) and its import
- Modify: `web/src/config.test.ts` (the `TABS[0]` assertion)

**Interfaces:**
- Consumes: everything from Tasks 4-7, plus `listRoots` from `./config` and
  `listProposals` (or whatever `curation.ts` names it) for the proposal badges.
- Produces: `export function RuleEditor(): React.ReactElement`.

- [ ] **Step 1: Check the proposals client's name**

```bash
cd /home/davey/projects/taiyaku && grep -n '^export const list\|^export async function list\|^export function list' web/src/curation.ts
```

Use the name that exists. If it requires a filter argument, pass
`{ state: 'accepted' }` — the badge only concerns accepted proposals.

- [ ] **Step 2: Write the failing test**

Create `web/src/RuleEditor.test.tsx`:

```tsx
import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from './config'
import * as curation from './curation'
import { RuleEditor } from './RuleEditor'
import * as rulesApi from './rules'
import type { Impact, Preview, Rule, Saved } from './rules'
import { mount } from './testing/mount'

vi.mock('./rules', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./rules')>()),
  listRules: vi.fn(),
  previewRules: vi.fn(),
  saveRules: vi.fn(),
  importRules: vi.fn(),
}))
vi.mock('./config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./config')>()),
  listRoots: vi.fn(),
}))
vi.mock('./curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./curation')>()),
  listProposals: vi.fn(),
}))

const rule = (over: Partial<Rule> = {}): Rule => ({
  id: 1,
  ordinal: 1,
  enabled: true,
  root_id: null,
  kind: 'regex',
  pattern: 'Manga/.*',
  match_on: 'path',
  set_series: 'Series',
  set_edition_key: null,
  set_kind: null,
  set_language: null,
  note: null,
  ...over,
})

const impact = (over: Partial<Impact> = {}): Impact => ({
  series_before: 8,
  series_after: 2,
  series_merged: 6,
  works_rehomed: 71,
  works_orphaned: 0,
  proposals_reopened: 0,
  proposals_raised: 0,
  ...over,
})

const previewBody = (over: Partial<Preview> = {}): Preview => ({
  impact: impact(),
  rows: [],
  rows_truncated: false,
  conflicts: [],
  ...over,
})

const savedBody = (over: Partial<Saved> = {}): Saved => ({
  impact: impact(),
  rules: [rule()],
  reopened_proposals: [],
  complaints: [],
  ...over,
})

async function render() {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(<RuleEditor />)
    await Promise.resolve()
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

const clickText = async (host: HTMLElement, text: string) => {
  await act(async () => {
    ;[...host.querySelectorAll<HTMLElement>('button')]
      .find((b) => b.textContent?.includes(text))
      ?.click()
    await Promise.resolve()
    await Promise.resolve()
  })
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(rulesApi.listRules).mockResolvedValue({
    rules: [rule(), rule({ id: 2, ordinal: 2, pattern: 'Colour/.*' })],
    complaints: [],
  })
  vi.mocked(config.listRoots).mockResolvedValue({ roots: [] })
  vi.mocked(curation.listProposals).mockResolvedValue([])
  vi.mocked(rulesApi.previewRules).mockResolvedValue(previewBody())
  vi.mocked(rulesApi.saveRules).mockResolvedValue(savedBody())
})

describe('RuleEditor', () => {
  it('draws the rules it loaded', async () => {
    const host = await render()
    expect(host.querySelectorAll('.rule-row')).toHaveLength(2)
  })

  it('previews on selecting a different rule', async () => {
    // `rows` is scoped to `selected`, so changing which rule is open is the
    // only way to populate Matched files and Provenance for it — and it is one
    // gesture, not a stream. The build design's §5.3.
    const host = await render()
    vi.mocked(rulesApi.previewRules).mockClear()
    await act(async () => {
      host.querySelectorAll<HTMLElement>('.rule-row button.select')[1].click()
      await Promise.resolve()
    })
    expect(rulesApi.previewRules).toHaveBeenCalledTimes(1)
    expect(vi.mocked(rulesApi.previewRules).mock.calls[0][1]).toBe(1)
  })

  it('previews on an enable toggle', async () => {
    const host = await render()
    vi.mocked(rulesApi.previewRules).mockClear()
    await act(async () => {
      host
        .querySelectorAll<HTMLInputElement>('.rule-row input[type="checkbox"]')[0]
        .click()
      await Promise.resolve()
    })
    expect(rulesApi.previewRules).toHaveBeenCalledTimes(1)
    expect(
      vi.mocked(rulesApi.previewRules).mock.calls[0][0][0].enabled,
    ).toBe(false)
  })

  it('previews on an add', async () => {
    const host = await render()
    vi.mocked(rulesApi.previewRules).mockClear()
    await clickText(host, '+ Add')
    expect(rulesApi.previewRules).toHaveBeenCalledTimes(1)
    expect(vi.mocked(rulesApi.previewRules).mock.calls[0][0]).toHaveLength(3)
  })

  it('fires no preview while typing in a rule field, and one on blur', async () => {
    // The screen's central cost decision, asserted rather than described —
    // spec C4 §7's own named test.
    const host = await render()
    await act(async () => {
      host.querySelectorAll<HTMLElement>('.rule-row button.select')[0].click()
      await Promise.resolve()
    })
    vi.mocked(rulesApi.previewRules).mockClear()

    const pattern = host.querySelector<HTMLInputElement>(
      'input[name="pattern"]',
    ) as HTMLInputElement
    const setter = Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      'value',
    )?.set
    for (const value of ['M', 'Ma', 'Man', 'Manga']) {
      act(() => {
        setter?.call(pattern, value)
        pattern.dispatchEvent(new Event('input', { bubbles: true }))
      })
    }
    expect(rulesApi.previewRules).not.toHaveBeenCalled()

    await act(async () => {
      pattern.dispatchEvent(new FocusEvent('blur', { bubbles: true }))
      await Promise.resolve()
    })
    expect(rulesApi.previewRules).toHaveBeenCalledTimes(1)
  })

  it('renumbers ordinals when a reorder changes the preview', async () => {
    // Reordering two rules changes the preview — spec C4 §7. Layering is
    // positional, and the server reads `ordinal`, so an array that moved
    // without renumbering would preview identically.
    const host = await render()
    vi.mocked(rulesApi.previewRules).mockClear()
    const rows = host.querySelectorAll<HTMLElement>('.rule-row')
    await act(async () => {
      rows[1].dispatchEvent(
        Object.assign(new Event('dragstart', { bubbles: true }), {
          dataTransfer: { setData: vi.fn(), effectAllowed: '' },
        }),
      )
      rows[0].dispatchEvent(
        Object.assign(new Event('drop', { bubbles: true }), {
          dataTransfer: { getData: () => '1' },
        }),
      )
      await Promise.resolve()
    })
    const sent = vi.mocked(rulesApi.previewRules).mock.calls[0][0]
    expect(sent.map((r) => r.pattern)).toEqual(['Colour/.*', 'Manga/.*'])
    expect(sent.map((r) => r.ordinal)).toEqual([1, 2])
  })

  it('states what Save will do, from the live preview', async () => {
    const host = await render()
    expect(host.querySelector('.save')?.textContent).toContain('71')
  })

  it('sends the whole list to the save route and states what it did', async () => {
    const host = await render()
    await clickText(host, 'Save')
    expect(rulesApi.saveRules).toHaveBeenCalledTimes(1)
    expect(vi.mocked(rulesApi.saveRules).mock.calls[0][0]).toHaveLength(2)
    expect(host.textContent).toContain('71')
  })

  it('states the proposals a save reopened, by id', async () => {
    vi.mocked(rulesApi.saveRules).mockResolvedValue(
      savedBody({
        reopened_proposals: [4, 9],
        impact: impact({ proposals_reopened: 2 }),
      }),
    )
    const host = await render()
    await clickText(host, 'Save')
    expect(host.textContent).toContain('4')
    expect(host.textContent).toContain('9')
  })

  it('warns when the save’s impact differs from the preview it was made from', async () => {
    // Nothing should change between them on a single-writer library — and the
    // screen's entire promise is that the preview is true, so the cheapest
    // place to notice it is false is the save that just disproved it. §4.2.
    vi.mocked(rulesApi.saveRules).mockResolvedValue(
      savedBody({ impact: impact({ works_rehomed: 4 }) }),
    )
    const host = await render()
    await clickText(host, 'Save')
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'differs',
    )
  })

  it('draws the server’s own sentence when a save is refused', async () => {
    vi.mocked(rulesApi.saveRules).mockRejectedValue(
      new Error('/api/library/rules: 400 rule 9999: no such rule exists to keep'),
    )
    const host = await render()
    await clickText(host, 'Save')
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'no such rule exists to keep',
    )
  })

  it('draws the server’s own sentence when a non-admin loads it', async () => {
    // No client-side admin guard: `/config/rules` is reachable by URL and the
    // screen renders the 403's message. The build design's §2.
    vi.mocked(rulesApi.listRules).mockRejectedValue(
      new Error('/api/library/rules: 403 only an administrator may do this'),
    )
    const host = await render()
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'only an administrator',
    )
  })

  it('reports a complaint a save answered', async () => {
    vi.mocked(rulesApi.saveRules).mockResolvedValue(
      savedBody({ complaints: ['rule 2: pattern will not compile'] }),
    )
    const host = await render()
    await clickText(host, 'Save')
    expect(host.textContent).toContain('will not compile')
  })

  it('draws import as destructive and says what it reopened', async () => {
    vi.mocked(rulesApi.importRules).mockResolvedValue({
      rules: [rule()],
      complaints: [],
      reopened_proposals: [7],
    })
    const host = await render()
    expect(
      [...host.querySelectorAll('button')].some((b) =>
        b.textContent?.includes('Import'),
      ),
    ).toBe(true)
    expect(host.textContent).toContain('replaces')
  })
})
```

- [ ] **Step 3: Run and watch it fail**

```bash
cd /home/davey/projects/taiyaku/web && pnpm vitest run --project jsdom src/RuleEditor.test.tsx 2>&1 | tail -15
```

Expected: `Failed to resolve import "./RuleEditor"`.

- [ ] **Step 4: Write `web/src/RuleEditor.tsx`**

```tsx
/**
 * The rule editor — spec C4 §1, and the build design's §5. The one screen
 * Komga structurally cannot have.
 *
 * **It previews on commit and never while typing.** A preview costs a median
 * 281 ms on a real library (130 series, 1858 works) and 495 ms with a rule
 * appended, with the diff 1.3 ms of it — the derivation is the entire cost.
 * So a preview runs on: a field blur or Enter, an enable toggle, a
 * drag-reorder, an add, a delete, and selecting a different rule. The last is
 * the one that is not obvious: `rows` is scoped to the selected rule, so
 * changing which rule is open is the only way to populate the Matched files
 * and Provenance tabs for it — and it is one gesture, not a stream. A person
 * clicking down a five-rule list pays five derivations; a person typing a
 * regex pays none. The alternatives — an explicit Preview button, a long
 * typing debounce — are weighed and refused in the build design's §5.3.
 *
 * **Nothing is written until Save**, per C4 §1, and the button states what it
 * will do from the live preview's `works_rehomed`. Because the preview is
 * never stale, that number is never a lie.
 *
 * **The save's own impact is compared against the preview it was made from.**
 * Nothing should differ on a single-writer library — but the screen's entire
 * promise is that the preview is true, and the cheapest place to notice it is
 * false is the save that just disproved it. So a difference is drawn rather
 * than swallowed.
 *
 * **No admin guard.** `/config/rules` is reachable by URL and this screen
 * renders the server's own 403 sentence, exactly as `/proposals` does. A
 * client that *decides* who may administer is the shape O93 closed.
 *
 * **A sequence number on the preview, because responses need not arrive in
 * order.** Several commits can be in flight — a blur that starts a preview,
 * then a click on another rule before it lands — and without this an earlier
 * response arriving later would write a stale impact under a Save button that
 * states a number from it. `Roots.tsx` and `Search.tsx` carry the same
 * counter for the same reason.
 */
import { useCallback, useEffect, useRef, useState } from 'react'
import { listRoots, type Root } from './config'
import { listProposals, type Proposal } from './curation'
import { PreviewTabs } from './PreviewTabs'
import { RuleForm } from './RuleForm'
import { RuleList } from './RuleList'
import {
  blankRule,
  importRules,
  listRules,
  moved,
  type Preview,
  previewRules,
  renumber,
  type Rule,
  type Saved,
  saveRules,
  toDocument,
} from './rules'

const message = (err: unknown) => (err instanceof Error ? err.message : String(err))

export function RuleEditor() {
  const [rules, setRules] = useState<Rule[]>([])
  const [roots, setRoots] = useState<Root[]>([])
  const [proposals, setProposals] = useState<Proposal[]>([])
  const [selected, setSelected] = useState<number | undefined>()
  const [preview, setPreview] = useState<Preview | undefined>()
  const [busy, setBusy] = useState(false)
  const [failure, setFailure] = useState<string | undefined>()
  const [saved, setSaved] = useState<Saved | undefined>()
  const [saving, setSaving] = useState(false)
  /** The preview whose impact `saved` is compared against. */
  const predicted = useRef<Preview | undefined>(undefined)
  /** The newest `previewRules` call — see this file's own doc comment. */
  const latest = useRef(0)
  /** Whether a save is already on its way. A ref, for the reason `Roots.tsx`'s is. */
  const savingNow = useRef(false)

  const runPreview = useCallback((next: Rule[], open: number | undefined) => {
    const seq = ++latest.current
    setBusy(true)
    previewRules(next, open)
      .then((body) => {
        if (seq !== latest.current) return
        setPreview(body)
        predicted.current = body
        setBusy(false)
        setFailure(undefined)
      })
      .catch((err: unknown) => {
        if (seq !== latest.current) return
        setBusy(false)
        setFailure(message(err))
      })
  }, [])

  useEffect(() => {
    Promise.all([listRules(), listRoots(), listProposals({ state: 'accepted' })])
      .then(([listed, rootList, accepted]) => {
        setRules(listed.rules)
        setRoots(rootList.roots)
        setProposals(accepted)
        setFailure(undefined)
        runPreview(listed.rules, undefined)
      })
      .catch((err: unknown) => setFailure(message(err)))
  }, [runPreview])

  /** Every editor gesture lands here: set the rules, then preview once. */
  const commit = (next: Rule[], open: number | undefined) => {
    setRules(next)
    setSelected(open)
    setSaved(undefined)
    runPreview(next, open)
  }

  const save = () => {
    if (savingNow.current) return
    savingNow.current = true
    setSaving(true)
    setFailure(undefined)
    const made = predicted.current
    saveRules(rules)
      .then((answer) => {
        savingNow.current = false
        setSaving(false)
        setSaved(answer)
        setRules(answer.rules)
        if (
          made !== undefined &&
          JSON.stringify(made.impact) !== JSON.stringify(answer.impact)
        )
          setFailure(
            'The save differs from the preview it was made from. Something changed the library ' +
              'between the two — the numbers below are what actually happened.',
          )
        // The list just changed under every accepted proposal's
        // `created_rule_id`, so the badges are re-read rather than assumed.
        listProposals({ state: 'accepted' })
          .then(setProposals)
          .catch(() => {})
      })
      .catch((err: unknown) => {
        savingNow.current = false
        setSaving(false)
        setFailure(message(err))
      })
  }

  const doImport = (text: string) => {
    importRules(text)
      .then((answer) => {
        setRules(answer.rules)
        setSelected(undefined)
        setSaved(undefined)
        runPreview(answer.rules, undefined)
        if (answer.reopened_proposals.length > 0)
          setFailure(
            `The import reopened ${answer.reopened_proposals.length} accepted proposals: ` +
              `${answer.reopened_proposals.join(', ')}. An imported document carries none of this ` +
              'library’s rule ids, so every rule it replaces is a new row.',
          )
      })
      .catch((err: unknown) => setFailure(message(err)))
  }

  const exportDoc = () => {
    try {
      return JSON.stringify(toDocument(rules, roots), null, 2)
    } catch (err: unknown) {
      setFailure(message(err))
      return ''
    }
  }

  return (
    <section className="rule-editor">
      <h2>Rules</h2>
      {failure !== undefined && <p role="alert">{failure}</p>}

      <div className="editor-panes">
        <RuleList
          rules={rules}
          selected={selected}
          proposals={proposals}
          conflictCount={preview?.conflicts.length ?? 0}
          onSelect={(index) => commit(rules, index)}
          onToggle={(index) =>
            commit(
              rules.map((rule, i) =>
                i === index ? { ...rule, enabled: !rule.enabled } : rule,
              ),
              selected,
            )
          }
          onReorder={(from, to) => {
            const next = renumber(moved(rules, from, to))
            // The selected rule travels with the drag rather than staying at
            // an index that now holds a different rule.
            const open =
              selected === undefined
                ? undefined
                : selected === from
                  ? to
                  : selected
            commit(next, open)
          }}
          onAdd={() => {
            const next = renumber([...rules, blankRule(rules.length + 1)])
            commit(next, next.length - 1)
          }}
          onDelete={(index) => {
            const next = renumber(rules.filter((_, i) => i !== index))
            commit(
              next,
              selected === undefined || selected === index
                ? undefined
                : selected > index
                  ? selected - 1
                  : selected,
            )
          }}
        />

        <div className="editor-right">
          {selected !== undefined && rules[selected] !== undefined ? (
            <RuleForm
              // Local draft state lives in the form; a new key is what makes
              // selecting a different rule redraw rather than keep the last
              // one's half-typed text.
              key={`${selected}:${rules[selected].id}`}
              rule={rules[selected]}
              roots={roots}
              onCommit={(edited) =>
                commit(
                  rules.map((rule, i) => (i === selected ? edited : rule)),
                  selected,
                )
              }
            />
          ) : (
            <p className="empty">
              Pick a rule to edit it. The impact below is for the whole list.
            </p>
          )}

          <PreviewTabs
            preview={preview}
            rules={rules}
            selected={selected}
            busy={busy}
            onReorder={(from, to) => {
              const next = renumber(moved(rules, from, to))
              commit(next, selected === from ? to : selected)
            }}
          />
        </div>
      </div>

      {saved !== undefined && (
        <p className="saved">
          Saved: {saved.impact.works_rehomed} works re-homed,{' '}
          {saved.impact.series_merged} series merged.
          {saved.reopened_proposals.length > 0 &&
            ` Reopened proposals ${saved.reopened_proposals.join(', ')}.`}
        </p>
      )}
      {saved !== undefined &&
        saved.complaints.map((complaint) => (
          <p className="complaint" key={complaint}>
            {complaint}
          </p>
        ))}

      <div className="editor-actions">
        <button
          type="button"
          className="save"
          disabled={saving || preview === undefined}
          onClick={save}
        >
          {saving
            ? 'Saving…'
            : `Save — apply to ${preview?.impact.works_rehomed ?? 0} works`}
        </button>

        <label className="import">
          <span>
            Import — replaces every rule with the document’s, minting fresh ids,
            which reopens every accepted proposal whose rule it replaces
          </span>
          <input
            type="file"
            accept="application/json"
            onChange={(e) => {
              const file = e.target.files?.[0]
              if (file === undefined) return
              file
                .text()
                .then(doImport)
                .catch((err: unknown) => setFailure(message(err)))
            }}
          />
        </label>

        <a
          className="export"
          download="rules.json"
          href={`data:application/json,${encodeURIComponent(exportDoc())}`}
        >
          Export
        </a>
      </div>
    </section>
  )
}
```

- [ ] **Step 5: Run and watch it pass**

```bash
cd /home/davey/projects/taiyaku/web && pnpm vitest run --project jsdom src/RuleEditor.test.tsx 2>&1 | tail -10
```

Expected: all passing.

- [ ] **Step 6: Prepend the tab, and make the flagship the landing screen**

`web/src/config.ts`, line 36:

```ts
export const TABS: Tab[] = [
  { path: 'rules', label: 'Rules' },
  { path: 'roots', label: 'Roots' },
]
```

`web/src/Config.tsx` — add the import and the screen:

```tsx
import { RuleEditor } from './RuleEditor'
...
const SCREENS: Record<string, React.ReactNode> = {
  rules: <RuleEditor />,
  roots: <Roots />,
}
```

`web/src/config.test.ts` — the `TABS[0]` assertion, with its reason:

```ts
  it('leads with the tab the index redirect lands on', () => {
    // `Config.tsx` sends `/config` to `TABS[0].path`. A tab list whose first
    // entry is not a real route is a redirect into nothing. PR B prepends the
    // rule editor, which is what makes the flagship the landing screen — the
    // build design's §2 records that as the one intended move of this list.
    expect(TABS.length).toBeGreaterThan(0)
    expect(TABS[0].path).toBe('rules')
  })
```

`web/src/Config.test.tsx` — its `sends a bare /config to the leading tab` test
asserts `Roots`. Update it to assert the rule editor, and give the mock block
whatever `RuleEditor` now fetches (add `listRules`, `listProposals` mocks
alongside `listRoots`).

- [ ] **Step 7: Run the whole web suite**

```bash
cd /home/davey/projects/taiyaku/web && pnpm vitest run 2>&1 | tail -12
```

Expected: no failures. Note the new totals for `node` and `jsdom` — Task 9
needs both figures.

- [ ] **Step 8: Commit**

```bash
cd /home/davey/projects/taiyaku && git add web/src/RuleEditor.tsx web/src/RuleEditor.test.tsx web/src/config.ts web/src/config.test.ts web/src/Config.tsx web/src/Config.test.tsx && git commit -m "web: the rule editor, previewing on commit and saving through PUT"
```

---

## Task 9: The gate, the counts, and the docs

**Files:**
- Modify: `docs/status.md` (test counts at both sites, `taiyaku-serve` route prose, `web/` prose)
- Modify: `README.md` (configuring a library stops being CLI-only)
- Modify: `scripts/coverage-floor.mjs` (per-file floors and the file count)
- Modify: `web/vite.config.ts` (aggregate thresholds, only if they moved)
- Modify: `docs/notes.md` (anything measured on the way)

**Interfaces:** none — this task ships no code.

- [ ] **Step 1: Run the full gate and read the failures**

```bash
cd /home/davey/projects/taiyaku && env -u TAIYAKU_CORPUS scripts/check-all.sh 2>&1 | tail -40
```

Expected to fail on the test-count check (`status.md` claims 2007 Rust and
1,812 web) and probably on the coverage floor's file count. Both are claims
this branch moved.

- [ ] **Step 2: Get the real counts**

```bash
cd /home/davey/projects/taiyaku && cargo nextest run --workspace 2>&1 | tail -3
cd /home/davey/projects/taiyaku/web && pnpm vitest run --project node 2>&1 | grep -E 'Tests +[0-9]+'
cd /home/davey/projects/taiyaku/web && pnpm vitest run --project jsdom 2>&1 | grep -E 'Tests +[0-9]+'
```

- [ ] **Step 3: Update `docs/status.md`**

Five things, per the open-items ritual:

1. **The Rust test count at both sites** — `Six crates, N tests` (line ~121)
   and the `Nine of the N are standing checks` line two lines later.
2. **The `web/` figure** on the same line: `plus N in web/'s TypeScript — N in
   the node ... and N in the jsdom`.
3. **`taiyaku-serve`'s route prose** — add `PUT /api/library/rules` where the
   rules routes are described, saying it is the only route that applies a rule
   and that `preview::apply` is `preview` committed with `Documents::Reconcile`.
4. **The `web/` prose** — the settings area gains the rule editor and it is now
   the landing tab.
5. **The open-items sweep**, on the way in and out:

```bash
cd /home/davey/projects/taiyaku && grep -c '^| O[0-9]' docs/status.md && grep -c '^| O[0-9]* | \[x\]' docs/status.md && grep -n -A1 '^What remains:' docs/status.md
```

Expected before this branch: 94 rows, 46 closed. **This branch closes nothing
and opens nothing unless a review finds something** — if it does, do the full
five-part edit rather than a tick, and remember the total count *word* at the
head of the section is the one that drifts.

- [ ] **Step 4: Update `README.md`**

Find where the README says rules are configured through the CLI and add that
the settings area's Rules tab now edits them in a browser, with a live preview
and a save that applies. Keep it to a sentence or two — `docs/status.md` is
where the argument lives.

- [ ] **Step 5: Update `scripts/coverage-floor.mjs` and re-run**

```bash
cd /home/davey/projects/taiyaku && node scripts/coverage-floor.mjs 2>&1 | tail -25
```

Add the five new files to the per-file floor table at their honest measured
figures and bump the minimum file count. Do not lower an existing floor to make
the run pass — if a file dropped, that is a test to write.

- [ ] **Step 6: Record the measurements in `docs/notes.md`**

At minimum, under a new heading dated 2026-09-05:

- what a save costs against the preview's 281 ms on the same library, measured
  through `PUT /api/library/rules` (the derivation is the same; the reconcile
  pass over `<root>/.taiyaku/` is what is new, and no figure exists for it);
- the new Rust and web test counts;
- anything the gate reported that a later reader would otherwise re-derive.

Every figure in `status.md` has to trace to one here — that is the preamble's
standing promise.

- [ ] **Step 7: Run the gate to green, and quote the line**

```bash
cd /home/davey/projects/taiyaku && env -u TAIYAKU_CORPUS scripts/check-all.sh 2>&1 | grep -E 'all green|FAIL|not measured'
```

Expected: the literal `all green` line, and no `FAIL`. **Do not pipe through
`tail` or append `; echo`** — both mask the exit status.

- [ ] **Step 8: Commit**

```bash
cd /home/davey/projects/taiyaku && git add docs/status.md docs/notes.md README.md scripts/coverage-floor.mjs web/vite.config.ts && git commit -m "docs: the rule editor ships, and configuring a library stops being CLI-only"
```

---

## Task 10: Verify against real material, and review the whole branch

**Files:** none — this task changes nothing unless it finds something.

- [ ] **Step 1: Drive it against a copy of the real library**

Per `verifying-against-real-material`: **isolate the writes and prove it
afterwards.** Copy, do not point at, the real library — this branch's whole
subject is a route that writes and derives.

```bash
cd /home/davey/projects/taiyaku && ls /home/davey/temp/manga-test 2>/dev/null || echo "no scratch library — make one before this step"
```

Then serve against a scratch copy, open `/config/rules`, and check by eye:

1. A preview of the unchanged list reports zero rehomed and zero reopened.
2. Editing a pattern fires **one** request on blur and none while typing —
   read it off the network panel, not off the code.
3. Save reports an impact equal to the preview that preceded it.
4. Saving the unchanged list a second time reopens nothing.
5. A rule an accepted proposal wrote carries its badge, and deleting it warns.

**Look at the pixels** — crop the region before writing any finding up, and read
the numbers rather than the nouns.

- [ ] **Step 2: Run the mutation sweep on what this branch changed**

```bash
cd /home/davey/projects/taiyaku && scripts/mutants.sh --since main 2>&1 | tail -30
```

`mutants.sh --since main` reads `git diff main...HEAD`, so **every fix must be
committed first** or it re-reports it. Read the result against
`taiyaku-mutation-testing-next`: neither a survivor list nor a clean run can be
taken at face value, an *errored* run's verdict is fiction in both directions,
and **check the `unviable` count against what you actually changed**. After a
rule change, sweep what the rule *governs*, not what the diff touched — O87
tests only the mutated package.

- [ ] **Step 3: Whole-branch review**

Per `whole-branch-review-finds-what-task-review-cannot`: per-task review passed
every task on #91, #105, #106 and #108 while whole-branch review found seven,
two, three and twenty-nine defects respectively. **Review the fixes too** — on
#106 that found three more, one a regression the fix round itself introduced —
and on #108 four passes did not converge, with seven findings existing only
because an earlier fix round was partial.

**After fixing a finding, grep for its shape rather than its function.**

```bash
cd /home/davey/projects/taiyaku && git diff main...HEAD --stat
```

Read the whole diff, not the task list.

- [ ] **Step 4: Open the PR**

Per `taiyaku-gitea-not-github`: `gh` is authenticated against the wrong host —
use the gitea MCP. **Never hard-wrap the PR body**; the PR body *is* the squash
commit message, and it appears once.

---

## Self-review

**Spec coverage.**

| Spec | Task |
| --- | --- |
| §4.1 `preview::apply`, `Applied` | 2 |
| §4.1 the existing test drives `apply` | 2, step 5 |
| §4.2 `PUT /api/library/rules`, the lists | 3 |
| §5.1 layout — list left, editor and preview right | 8 |
| §5.2 four tabs, badge from every tab, `rows` read twice, the cap drawn | 7 |
| §5.3 preview triggers; no keystroke hook | 5, 8 |
| §5.4 conflicts, `others`, the forty-rule warning | 6, 7 |
| §5.5 the proposal badge and the reopening | 6, 8 |
| §5.6 import/export, one converter both directions | 4, 8 |
| §5.7 files | file structure table |
| §7 the six tests this design adds | 2 (two), 3 (one), 8 (one), 3 + 8 (refusals) |
| §9 what this invalidates | 9 |

**Two §7 tests belong to PR A's screens and are deliberately not here**: *"a
root that has never been scanned draws 'never scanned'"* and *"a second scan of
a root in flight draws the 409's naming"*. Both are about `Roots`/`RootRow`,
which shipped in #108. Check whether #108 wrote them; if it did not, they are a
gap in PR A and belong in a follow-up, not in this branch.

**Type consistency.** `Rule`, `Impact`, `Preview`, `Saved`, `Row`, `Conflict`,
`Decided`, `RuleDoc` are defined once in Task 4 and used unchanged in 5-8.
`renumber`, `moved`, `blankRule`, `toDocument`, `fromDocument` likewise.
`preview::Applied`'s four fields are read by exactly one caller, Task 3's
`save_rules`. `reopened_ids` (Task 1) is called by exactly one caller, Task 2's
`apply`.

**Known unknowns the executor must check rather than assume**, each flagged at
its step: `mount`'s real signature (Task 5), `curation.ts`'s list function name
and whether it takes a filter (Task 8), the `config_api/tests.rs` helper names
(Task 3), and whether a fixture with an accepted proposal already exists
(Task 2).
