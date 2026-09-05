use crate::preview::{
    Conflict, Decided, Impact, Rows, conflicts, grouping, preview, preview_with_rows,
};

/// The promise the whole screen rests on: what the preview says will happen
/// is what happens. Same fixture, same proposed list, one previewed and one
/// saved — and the comparison is the *whole* [`crate::preview::Impact`], not
/// one field of it.
///
/// **Why the whole struct, and why `diff` rather than a field of `saved`.**
/// Comparing `predicted.works_rehomed` against a count of members whose
/// series is `"Alpha"` would only be equal by coincidence of this fixture,
/// and it would say nothing about `series_merged`, `works_orphaned` or
/// `proposals_reopened` — a `preview` that always answered
/// `Impact::default()` would still pass that narrower check on a fixture
/// with no orphans. Running the real save's own before/after groupings
/// through `diff` — the exact function `preview` itself calls — is what
/// makes this a check on `preview`'s answer rather than a check on one
/// number that happens to agree with it.
///
/// **The save side is [`crate::preview::apply`], which is what makes this a
/// claim about shipped code.** It used to be `replace_all` followed by
/// `derive` — two lines that existed nowhere but here, so the test asserted
/// that a preview agrees with a sequence this test types out rather than with
/// the save this product actually performs. Spec C4 §4.1, and the failure
/// mode `plans-need-executed-fixtures` names, sitting inside the test that
/// guards the screen's central promise.
///
/// **Important 2.** `Alpha` (`library_with_two_roots`'s third root) already
/// reads `"Alpha"` before this test's own widening rule sends everything
/// there, so its placement does not move — `works_rehomed: 2`, not `3`, is
/// what tells `after.len()` (3) apart from a filter that counts every member
/// regardless of whether its series actually changed.
#[test]
fn a_preview_and_a_real_save_produce_the_same_structure() {
    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();
    let mut proposed = crate::rules::read(&conn).unwrap().0;
    proposed.push(crate::tests::a_rule_setting_series("Alpha"));

    let predicted = preview(&conn, &proposed).unwrap();

    // **Critical 1.** The comparison below — `predicted` against `diff`'s own
    // output over the real save — is circular on its own: it is satisfied by
    // any `diff` whatsoever, including one that always returns
    // `Impact::default()`, because both sides route through the same
    // function. Asserting the literal numbers is what makes `diff` itself
    // load-bearing rather than merely self-consistent — a mutation that
    // replaces `diff`'s body with `Impact::default()` must fail *this*
    // assertion, since the fixture's true impact is not all zeroes.
    assert_eq!(
        predicted,
        Impact {
            series_before: 3,
            series_after: 1,
            series_merged: 2,
            works_rehomed: 2,
            works_orphaned: 0,
            proposals_reopened: 0,
            proposals_raised: 0,
        },
        "the fixture's literal impact — merging Beta, Gamma and Alpha under one series, while \
         Alpha's own work does not move, must show exactly this"
    );

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
}

/// **Whole-branch review, Critical 1.** `librarydoc::apply`'s own
/// `unchecked_transaction` used to refuse outright the moment `preview`'s
/// own `BEGIN IMMEDIATE` was already open — "cannot start a transaction
/// within a transaction" — for any work whose hand document had not yet
/// been absorbed. `write_work` swallows that refusal into
/// `DeriveReport::unreadable_hand`, which `preview` never even reads, so a
/// preview silently skipped applying the chosen series a document on disk
/// names, while a real save — no wrapping transaction, so the identical
/// `unchecked_transaction` succeeds — applied it: the two disagreeing about
/// the exact thing `a_preview_and_a_real_save_produce_the_same_structure`,
/// just above, exists to promise they cannot.
///
/// **The specific state:** [`crate::tests::library_with_a_kept_document`]'s
/// one work, with its `migrated_document` marker cleared — "present but
/// unabsorbed," one of the reachable states this finding names (a rebuilt or
/// freshly imported database; a hand-written or restored
/// `.taiyaku/*.library.json`; or removing, re-adding and rescanning a root)
/// — and its on-disk document hand-edited to name a different, chosen
/// series, bypassing [`crate::librarydoc::apply`] so only the *file*
/// changes and the database keeps the old series until something re-applies
/// it. A derive that reads this document back — preview's own rolled-back
/// one, or a real save's — is exactly what is supposed to do that.
///
/// **The save side is [`crate::preview::apply`], for the reason
/// `a_preview_and_a_real_save_produce_the_same_structure`'s own doc comment
/// gives — whole-branch review, M5.** This test drove `crate::derive::derive`
/// directly until now, which made it a claim about a sequence typed out only
/// in this file rather than about the save this product actually performs;
/// this is the one test covering a present-but-unabsorbed document, so it was
/// the one place that gap survived the sibling's own conversion.
#[test]
fn a_preview_and_a_real_save_agree_on_a_work_with_an_unabsorbed_hand_document() {
    let fixture = crate::tests::library_with_a_kept_document();
    let conn = fixture.conn();
    let work_id: i64 = conn
        .query_row("SELECT id FROM work", [], |r| r.get(0))
        .expect("the fixture's one work");

    // The document on disk names a *different*, chosen series — written
    // directly to the file, never through `librarydoc::apply`, so the
    // database itself still reads the old series until something re-applies
    // this file. That gap is the point: a document present but not yet
    // absorbed.
    let path = crate::hand::path_for(&conn, work_id)
        .expect("resolving the document's address")
        .expect("a kept root's work has an address");
    let mut doc = crate::librarydoc::LibraryDoc::from_json(
        &std::fs::read_to_string(&path)
            .expect("the fixture's own second derive already settled this document"),
    )
    .expect("the fixture's own document parses");
    doc.series.titles = vec![crate::librarydoc::SeriesTitle {
        title: "Handset".to_string(),
        kind: crate::librarydoc::TitleKind::Romaji,
        primary: true,
    }];
    doc.series.chosen = true;
    std::fs::write(&path, doc.to_json()).expect("hand-editing the document on disk");

    // Clearing the marker is what makes `write_work`'s own guard
    // (`!migrate::absorbed(…)`) read this document back on the very next
    // derive — preview's or a real save's — rather than skipping it as
    // already applied.
    conn.execute(
        "DELETE FROM migrated_document WHERE work_id = ?1 AND document = 'library'",
        [work_id],
    )
    .expect("clearing the migrated_document marker");

    let current_rules = crate::rules::read(&conn).unwrap().0;
    let predicted = preview(&conn, &current_rules).expect("previewing must not itself fail");

    // Same shape as `a_preview_and_a_real_save_produce_the_same_structure`:
    // the real save's own `Applied::impact` is compared directly, which is a
    // check on `preview`'s answer rather than on one number that happens to
    // agree with it.
    let before_save = grouping(&conn).unwrap();
    let applied =
        crate::preview::apply(&conn, &current_rules).expect("the real save must not fail");
    let after_save = grouping(&conn).unwrap();

    assert_ne!(
        before_save, after_save,
        "the real save must actually move the work onto the chosen series, or this proves \
         nothing"
    );
    assert_eq!(
        predicted, applied.impact,
        "a preview must predict exactly what a real save does, even for a work whose hand \
         document is present but not yet absorbed"
    );
    assert_eq!(
        predicted.works_rehomed, 1,
        "the chosen series on disk must actually move the work — proving this is not a \
         vacuous agreement between two passes that both silently skipped the document"
    );

    let after_series: String = conn
        .query_row(
            "SELECT s.sort_title FROM series s JOIN work w ON w.series_id = s.id WHERE w.id = ?1",
            [work_id],
            |r| r.get(0),
        )
        .expect("the work still has a series after the save");
    assert_eq!(
        after_series, "Handset",
        "the real save must actually have adopted the chosen series' own title"
    );
}

/// A preview writes nothing: the rule table and the grouping are as they
/// were.
#[test]
fn a_preview_rolls_back_every_row_it_touched() {
    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();
    let before_rules = crate::rules::read(&conn).unwrap().0;
    let before_grouping = grouping(&conn).unwrap();

    let mut proposed = before_rules.clone();
    proposed.push(crate::tests::a_rule_setting_series("Alpha"));
    preview(&conn, &proposed).unwrap();

    assert_eq!(crate::rules::read(&conn).unwrap().0, before_rules);
    assert_eq!(grouping(&conn).unwrap(), before_grouping);
}

/// A preview leaves the library tree alone — Task 1's policy, asserted
/// through the caller that needs it rather than only at its own level.
///
/// **The document must have somewhere to go, or `Skip` and `Reconcile` are
/// indistinguishable here.** `library_with_a_kept_document`'s document is
/// already settled — present on disk and matching what a derive would
/// write — so a *third* derive under `Reconcile` writes nothing whether or
/// not `preview` ever checks `Documents` at all: a hard-coded
/// `Documents::Reconcile` would still pass a version of this test that only
/// snapshotted the settled tree. Deleting the file first reopens the exact
/// gap `hand::reconcile`'s own gap repair exists to close — the same move
/// `hand/tests.rs`'s `a_gap_is_written_on_the_next_pass` uses — so a preview
/// that actually ran `Reconcile` would write the file straight back,
/// observably, in `tree_snapshot`'s own output, and a preview that actually
/// runs `Skip` leaves the gap exactly as open as it found it.
#[test]
fn a_preview_leaves_the_library_tree_byte_identical() {
    let fixture = crate::tests::library_with_a_kept_document();
    let conn = fixture.conn();
    let work_id: i64 = conn
        .query_row("SELECT id FROM work", [], |r| r.get(0))
        .expect("the fixture's one work");
    let document = crate::hand::path_for(&conn, work_id)
        .unwrap()
        .expect("the fixture's document has settled at an address");
    std::fs::remove_file(&document).expect("reopening the gap `reconcile` would repair");

    let before = crate::tests::tree_snapshot(fixture.root_path());
    let mut proposed = crate::rules::read(&conn).unwrap().0;
    proposed.push(crate::tests::a_rule_setting_series("Alpha"));
    preview(&conn, &proposed).unwrap();

    assert_eq!(crate::tests::tree_snapshot(fixture.root_path()), before);
}

/// A save **writes the library tree**, which is the one thing that
/// distinguishes it from the preview it is otherwise identical to.
///
/// **This is the mirror of `a_preview_leaves_the_library_tree_byte_identical`
/// just above, and it exists because nothing else in this crate could tell
/// the two apart.** [`crate::preview::apply`] differs from
/// [`crate::preview::preview_with_rows`] in exactly two things: it commits
/// rather than rolling back, and it derives under
/// [`crate::derive::Documents::Reconcile`] rather than `Skip`. The commit is
/// covered by `apply_commits_what_preview_rolls_back`. The `Documents` value
/// was covered by nothing at all: hand-swapping `Reconcile` for `Skip` inside
/// `apply` left **all 720 tests in this crate green**, so the one line
/// deciding whether a save touches the user's filesystem — spec C4 §4.1 calls
/// it "the whole difference in what reaches the user's tree" — was free to
/// change in either direction unnoticed. Found by the whole-branch review's
/// own hand-applied mutation, after `cargo-mutants` reported `apply`'s only
/// mutant *unviable* (`Applied` derives no `Default`) and so reported the
/// sweep clean while proving nothing about this function — O88's remaining
/// cause, doing exactly what its entry says it does.
///
/// **The document must have somewhere to go, or `Skip` and `Reconcile` are
/// indistinguishable**, which is the same trap the preview test above
/// documents: `library_with_a_kept_document`'s document is already settled,
/// so a further derive under `Reconcile` writes nothing either way. Deleting
/// the file first reopens the gap [`crate::hand::reconcile`]'s own repair
/// exists to close, so a save that really runs `Reconcile` writes it straight
/// back and a save that runs `Skip` leaves the gap exactly as open as it
/// found it.
#[test]
fn a_save_writes_the_library_tree_a_preview_would_have_left_alone() {
    let fixture = crate::tests::library_with_a_kept_document();
    let conn = fixture.conn();
    let work_id: i64 = conn
        .query_row("SELECT id FROM work", [], |r| r.get(0))
        .expect("the fixture's one work");
    let document = crate::hand::path_for(&conn, work_id)
        .unwrap()
        .expect("the fixture's document has settled at an address");

    let settled = crate::tests::tree_snapshot(fixture.root_path());
    std::fs::remove_file(&document).expect("reopening the gap `reconcile` would repair");
    let with_the_gap = crate::tests::tree_snapshot(fixture.root_path());
    assert_ne!(
        settled, with_the_gap,
        "the gap must be observable in the snapshot, or this test cannot see the repair either"
    );

    let proposed = crate::rules::read(&conn).unwrap().0;
    crate::preview::apply(&conn, &proposed).unwrap();

    assert_eq!(
        crate::tests::tree_snapshot(fixture.root_path()),
        settled,
        "a save derives under Documents::Reconcile, so it repairs the gap it found; under Skip \
         the document would still be missing"
    );
}

/// **Important 4.** Every test above `push`es a rule; none removes one. The
/// editor's delete button previews exactly that, and it is the only
/// direction in which `series_after > series_before` — nothing above
/// exercises that arm of `diff` at all.
///
/// **The merging rule is saved for real first, not just proposed.**
/// Previewing "remove a rule this library never actually held" would only
/// exercise the addition direction a second time under a different name. So
/// `base_rules` — the fixture's rules before any edit — is captured, the
/// widening rule is appended and saved through [`crate::rules::replace_all`]
/// plus a real [`crate::derive::derive`], merging Alpha, Beta and Gamma into
/// one series for real; only then does `preview` propose going back to
/// `base_rules`, which is what removing that one rule from the editor would
/// send.
///
/// **A vacuous version of this test was the trap, not a hypothetical.**
/// Dropping the last of this fixture's four seeded default rules — the
/// version of "delete a rule" that does not first save a merge — is a
/// structural no-op here: nothing in this fixture's derivation depends on
/// those seeded rules at all, so its `Impact` would read `{ series_before: 3,
/// series_after: 3, series_merged: 0, works_rehomed: 0, works_orphaned: 0,
/// proposals_reopened: 0, proposals_raised: 0 }` and pass whether or not
/// `preview` — or `diff` — does anything. Asserting the literal numbers here
/// (Critical 1's fix, applied to this test too) is also load-bearing on its
/// own: without it, this test would not catch a regressed `replace_all`.
///
/// **Important 2.** `works_rehomed: 2`, not `3`: `Alpha`'s own work reads
/// `"Alpha"` on both sides of this preview — merged, and split back apart —
/// since that was already its series before the widening rule ever existed.
/// Only `Beta` and `Gamma` actually move.
#[test]
fn a_preview_of_removing_a_merging_rule_shows_the_split() {
    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();
    let base_rules = crate::rules::read(&conn).unwrap().0;

    let mut merged = base_rules.clone();
    merged.push(crate::tests::a_rule_setting_series("Alpha"));
    crate::rules::replace_all(&conn, &merged).unwrap();
    crate::derive::derive(&conn).unwrap();
    assert_eq!(
        grouping(&conn)
            .unwrap()
            .values()
            .map(|p| p.series.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        1,
        "the merging rule must actually have merged the fixture's three series, or this proves \
         nothing about the deletion direction"
    );

    let predicted = preview(&conn, &base_rules).unwrap();

    assert_eq!(
        predicted,
        Impact {
            series_before: 1,
            series_after: 3,
            series_merged: 0,
            works_rehomed: 2,
            works_orphaned: 0,
            proposals_reopened: 0,
            proposals_raised: 0,
        },
        "removing the merging rule must preview the split back into Alpha, Beta and Gamma, with \
         Alpha's own work not counted as rehomed since it never moved"
    );
}

/// The reopen case: `proposals_reopened` used to be `proposals_resolved`,
/// which every test above — and every test that existed before this one —
/// could only ever assert as `0`, because `open_before.saturating_sub(open_after)`
/// can never read anything else. A field only ever seen at `0` is exactly the
/// defect this module's fields were rewritten to remove, so this fixture
/// makes it nonzero on purpose: an accepted proposal, previewed against the
/// one rule list that drops the rule its accept wrote.
///
/// **Important 3.** Swapping `reopened` and `report.raised` at `diff`'s call
/// site used to pass every test in this module, because the only fixture
/// making either field nonzero — this one — made them both read `1`. The
/// `Series C` pair `crate::tests::library_with_an_accepted_proposal` now
/// scans without ever deriving is what tells them apart: `preview`'s own
/// derivation is the first to raise it, so it counts toward
/// `proposals_raised` without ever having been accepted, and so cannot count
/// toward `proposals_reopened`. Asserting the whole literal struct, not just
/// its two proposal fields, is what makes a swap at the call site fail this
/// test specifically rather than merely fail to be checked by it.
#[test]
fn a_preview_that_drops_an_accepted_rule_shows_the_proposal_it_answered_reopened() {
    let (fixture, base_rules) = crate::tests::library_with_an_accepted_proposal();
    let conn = fixture.conn();
    assert!(
        crate::proposals::open(&conn, 0.0).unwrap().is_empty(),
        "the fixture's proposal is accepted, not open, before the preview"
    );

    let predicted = preview(&conn, &base_rules).unwrap();

    assert_eq!(
        predicted,
        Impact {
            series_before: 2,
            series_after: 4,
            series_merged: 0,
            works_rehomed: 0,
            works_orphaned: 0,
            proposals_reopened: 1,
            proposals_raised: 2,
        },
        "dropping the accepted rule must reopen the one proposal it answered and split \
         `Go-Toubun` back in two, while `Series C` — scanned but never derived before this \
         preview — is raised for the first time rather than reopened"
    );

    // The preview must not have written any of this for real: a rolled-back
    // transaction leaves the proposal exactly as accepted as it found it, and
    // `Series C` exactly as un-raised.
    assert!(
        crate::proposals::open(&conn, 0.0).unwrap().is_empty(),
        "a preview must roll back, not actually reopen the proposal or raise a new one"
    );
}

/// **The bug this module was built to catch, reproduced directly.**
/// Previewing the exact rule list already in the database — the no-op a
/// person previewing without touching anything sends — reported
/// `proposals_reopened: 1` for a proposal this preview does not touch at
/// all, because [`crate::rules::replace_all`] used to re-mint every rule's
/// id on *every* call regardless of whether its content survived. That set
/// `proposal.created_rule_id` `NULL` through its `ON DELETE SET NULL`
/// foreign key for the fixture's one accepted proposal on every preview,
/// unconditionally, and [`crate::proposals::raise`]'s own orphan check
/// reopened it every time — the same always-wrong-number shape as
/// `proposals_resolved` before it, just reached through `replace_all`
/// instead of through `diff`.
///
/// **Read straight from the database, not from `base_rules`.** The other
/// tests in this file preview a list that differs from what is stored —
/// that is the point of a preview. This one previews
/// [`crate::rules::read`]'s own current answer back at itself, ids
/// included, which is the only way to ask "does nothing change" rather than
/// "does the one change I made show up".
#[test]
fn a_preview_of_the_librarys_own_unchanged_rules_reports_no_reopens() {
    let (fixture, _base_rules) = crate::tests::library_with_an_accepted_proposal();
    let conn = fixture.conn();
    let current_rules = crate::rules::read(&conn).unwrap().0;
    assert!(
        current_rules
            .iter()
            .any(|r| r.set_series.as_deref() == Some("Go-Toubun no Hanayome")),
        "the fixture's accepted rule must still be in the database's own current rule list, or \
         this preview is not actually the no-op it claims to be"
    );

    let predicted = preview(&conn, &current_rules).unwrap();

    // The whole struct, not just `proposals_reopened` — Critical 1's own
    // standard, applied here too: asserting one field alone would not catch
    // a `diff` that always returns `Impact::default()`, since `0` is that
    // field's default anyway. `series_after` and `works_rehomed` read
    // nonzero here — the fixture's `Go-Toubun` pair genuinely merges under
    // its still-live accepted rule, and `Series C`, scanned but never
    // derived until this preview's own derivation, is raised for the first
    // time — so this call has real work to get right, not a vacuous no-op.
    assert_eq!(
        predicted,
        Impact {
            series_before: 2,
            series_after: 3,
            series_merged: 0,
            works_rehomed: 1,
            works_orphaned: 0,
            proposals_reopened: 0,
            proposals_raised: 1,
        },
        "previewing the library's own current rule list — the accepted proposal's rule, id and \
         all, still in it — must not reopen a proposal nothing about this preview touches, while \
         still merging `Go-Toubun` for real and raising `Series C` for the first time"
    );

    // Rolled back, same as every other preview: the accepted proposal reads
    // accepted, not open, whether or not the bug above fires.
    assert!(
        crate::proposals::open(&conn, 0.0).unwrap().is_empty(),
        "a preview must roll back regardless of what it predicts"
    );
}

/// **Important 1.** `works_orphaned` used to be unconstrained: hard-coding it
/// to `0` left every other test in this module green, because none of them
/// ever made a rule swallow a member that used to exist on its own.
///
/// **Why a plain broad `set_series` rule cannot do it.** `derive.rs`'s own
/// `a_broad_rule_that_reaches_the_root_does_not_swallow_the_library` already
/// pins that a `set_series` rule matching an ancestor directory makes that
/// ancestor *defer* to whatever is inside it (`Produces::Deferred`) rather
/// than claim it — `Seen::produces`'s step 3 only takes the claiming branch
/// when `resolved.volume` is `None`, and `set_series` never touches `volume`.
/// So the fixture needs the proposed rule to resolve a volume for the
/// ancestor too, which is what makes it a candidate in its own right
/// (`Produces::Member`) rather than a name in a path.
///
/// **Why the ancestor has to land on `Series - v01`'s own address, not a
/// fresh one.** A derive never deletes a work its own pass does not revisit
/// — `write_work` only runs for `GroupKey`s that still produce a candidate,
/// and `Series - v02`'s row is left standing, stale, for exactly that reason
/// (see the `AFTER` state this test does not assert on, below). Giving the
/// ancestor the *same* `(series, volume)` address `Series - v01` already
/// holds is what makes `import::scanned`'s address probe find and rewrite
/// that existing row instead of minting a new one beside it — replacing its
/// one member, `Series 1/Series - v01`, with the ancestor's own path and
/// dropping the original clean out of the `member` table. That is
/// `works_orphaned`'s own definition: present in `before`, gone from `after`.
///
/// A regex capture is what supplies the ancestor's volume, rather than a
/// second `set_volume` column — `rules.rs` has none, by design (see
/// `Rule`'s own doc): only a capture or `builtin-volume` can set a number,
/// and `builtin-volume` needs a marker word this ancestor's name does not
/// carry. So the directory is named `Series 1`, which no marker matches on
/// its own, and the one proposed rule both sets its series (the mechanism
/// `DeriveReport::suppressed`'s doc comment names) and captures the trailing
/// `1` as its volume.
#[test]
fn a_rule_that_gives_an_ancestor_a_volumes_own_address_orphans_the_volume_it_swallowed() {
    let tree = crate::tests::tempdir("preview-ancestor-swallow");
    crate::scan::tests::write_pages(&tree.path.join("Series 1").join("Series - v01"), 2);
    crate::scan::tests::write_pages(&tree.path.join("Series 1").join("Series - v02"), 2);
    let library = crate::Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = crate::scan::add_root(&conn, &tree.path).expect("add");
    crate::scan::scan_root(&conn, &root).expect("scan");
    crate::derive::derive(&conn).expect("the base derive");

    let before = grouping(&conn).unwrap();
    assert_eq!(
        before.len(),
        2,
        "the base library must hold both volumes as separate members, or there is nothing to \
         orphan: {before:?}"
    );

    let mut proposed = crate::rules::read(&conn).unwrap().0;
    proposed.push(crate::rules::Rule {
        id: 0,
        ordinal: 999,
        enabled: true,
        root_id: None,
        kind: crate::rules::RuleKind::Regex,
        // Anchored to the ancestor's own `rel_path` exactly — `Series 1` —
        // so this rule never reaches either child directory beneath it.
        pattern: "^Series (?P<volume>1)$".to_string(),
        match_on: crate::rules::MatchOn::Path,
        set_edition_key: None,
        set_kind: None,
        set_language: None,
        // The same series `Series - v01` already resolves to, not a new
        // name — the collapse this test pins is onto that volume's own
        // address, not onto a third one nobody held before.
        set_series: Some("Series".to_string()),
        note: None,
    });

    let predicted = preview(&conn, &proposed).unwrap();

    assert_eq!(
        predicted,
        Impact {
            series_before: 1,
            series_after: 1,
            series_merged: 0,
            works_rehomed: 0,
            works_orphaned: 1,
            proposals_reopened: 0,
            proposals_raised: 0,
        },
        "the ancestor directory taking over `Series - v01`'s own address must orphan that \
         volume's member path — `Series - v02` is left stale rather than orphaned, since this \
         derive never revisits its address at all, which is the one member this fixture does \
         not touch"
    );

    // The preview must not have written any of this for real.
    assert_eq!(
        grouping(&conn).unwrap(),
        before,
        "a preview must roll back its own derive"
    );
}

/// **Important 2's rejected fixture, reachable now.** `reopened`'s predicate
/// — pinned by hand in `proposals::tests::reopened_counts_only_a_proposal_that_moved_from_accepted_to_open`
/// — used to be checked through a real [`preview`] call only on fixtures
/// where every currently-accepted proposal reopened, because
/// `crate::rules::replace_all` used to delete and reinsert the whole table
/// on any call, nulling every accepted proposal's `created_rule_id`
/// regardless of whether its own rule left the proposed list. A second
/// accepted proposal that *survives* a preview — the one shape a swapped
/// `reopened`/`report.raised` argument pair at `diff`'s call site cannot
/// fake, alongside the one this module already pins with a genuine reopen —
/// was unreachable through `preview` for exactly that reason.
///
/// It is reachable now: [`crate::tests::library_with_a_surviving_and_a_dropped_accept`]
/// accepts `Series One` first, so `base_rules` already carries its rule, and
/// accepts `Series Two` second, entirely absent from `base_rules`.
/// Previewing `base_rules` drops only `Series Two`'s rule.
#[test]
fn a_preview_that_drops_one_of_two_accepts_leaves_the_surviving_one_accepted() {
    let (fixture, base_rules) = crate::tests::library_with_a_surviving_and_a_dropped_accept();
    let conn = fixture.conn();
    let proposals_before = crate::proposals::states(&conn).unwrap();
    assert_eq!(
        proposals_before
            .values()
            .filter(|s| *s == "accepted")
            .count(),
        2,
        "both accepts must have landed before this preview, or there is nothing for either \
         proposal to survive or lose: {proposals_before:?}"
    );

    let predicted = preview(&conn, &base_rules).unwrap();

    assert_eq!(
        predicted,
        Impact {
            series_before: 4,
            series_after: 3,
            series_merged: 1,
            works_rehomed: 1,
            works_orphaned: 0,
            proposals_reopened: 1,
            proposals_raised: 1,
        },
        "dropping only `Series Two`'s rule must merge `Series One`'s pair back down (the rule \
         `base_rules` still carries), leave `Series Two`'s pair split, and reopen only the \
         proposal whose rule left the list"
    );

    // Rolled back, same as every other preview: both proposals read exactly
    // as they did before this call, `Series One`'s accepted and
    // `Series Two`'s too — a preview must not have reopened it for real.
    let proposals_after = crate::proposals::states(&conn).unwrap();
    assert_eq!(
        proposals_after, proposals_before,
        "a preview must roll back regardless of what it predicts"
    );
}

/// **What a *save* costs, beside what a preview costs.** The sibling below
/// measures `preview`; this measures [`crate::preview::apply`], and it exists
/// because the spec priced the preview and never priced the save at all —
/// while the save is the one that commits, reconciles hand documents, and
/// holds the write lock for a whole derivation with a person waiting on it.
///
/// **It also re-asserts the branch's central claim at real scale.** That a
/// preview and a save produce the same `Impact` is pinned on fixtures by
/// `a_preview_and_a_real_save_produce_the_same_structure`; a fixture has
/// three works. Here it is checked against a library with 1858, where a
/// disagreement would have somewhere to hide.
///
/// **Run it against a copy**, for the reason the sibling gives and one more:
/// unlike a preview, this one *commits*. Under a root with
/// `keeps_documents` on it also writes beneath `<root>/.taiyaku/`.
///
/// ```sh
/// TAIYAKU_PREVIEW_DB=/path/to/copy.db \
///   cargo test -p taiyaku-library --release apply_cost -- --ignored --nocapture
/// ```
#[test]
#[ignore = "measurement; needs TAIYAKU_PREVIEW_DB"]
fn apply_cost_on_a_real_library() {
    let Some(path) = std::env::var_os("TAIYAKU_PREVIEW_DB") else {
        panic!("set TAIYAKU_PREVIEW_DB to a copy of a real library");
    };
    let library = crate::Library::open(std::path::Path::new(&path)).expect("opens the library");
    let conn = library.conn();

    let rules = crate::rules::read(&conn).expect("reading rules").0;
    let accepted: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM proposal WHERE state = 'accepted'",
            [],
            |r| r.get(0),
        )
        .expect("counting accepted proposals");
    println!("accepted proposals before: {accepted}");

    let start = std::time::Instant::now();
    let predicted = crate::preview::preview(&conn, &rules).expect("previewing");
    println!("preview  unchanged: {:?}", start.elapsed());

    let start = std::time::Instant::now();
    let applied = crate::preview::apply(&conn, &rules).expect("saving");
    println!("apply    unchanged: {:?}", start.elapsed());

    assert_eq!(
        predicted, applied.impact,
        "the preview and the save disagree on a real library, which is the one thing this \
         screen's promise cannot survive"
    );
    assert!(
        applied.reopened_proposals.is_empty(),
        "a save of the unchanged list reopened {:?} — `replace_all`'s differential update is \
         what stops this, and a real library is where a delete-and-reinsert would show",
        applied.reopened_proposals
    );

    // Twice, because the second is the one that would catch a save whose own
    // answer changed what a following save sees — ids minted, ordinals
    // rewritten, or an accepted proposal orphaned by the first pass.
    let start = std::time::Instant::now();
    let again = crate::preview::apply(&conn, &rules).expect("saving again");
    println!("apply    again:     {:?}", start.elapsed());
    assert!(
        again.reopened_proposals.is_empty(),
        "a second save of the unchanged list reopened {:?}",
        again.reopened_proposals
    );
}

/// **A rule that can set nothing near-misses nothing** — the editor's own
/// "+ Add", which was filling the Matched files tab with the whole library.
///
/// `blankRule()` in `web/src/rules.ts` mints a `regex` rule with an empty
/// pattern and no `set_*` columns, and the screen selects it the moment it is
/// added. [`crate::rules::fields_settable_by`] returns an **empty set** for
/// such a rule — nothing to set statically, and no capture names in a pattern
/// that is the empty string — and `fields_set.iter().all(..)` over an empty
/// set is vacuously `true`. So every `scan_entry` in the library read as a
/// near-miss: on a real library the tab opened on 500 rows and a truncation
/// notice, on the most ordinary gesture the screen has.
///
/// A near-miss means "this rule could have set something here and did not".
/// With nothing settable there is nothing to have missed, which is the
/// distinction `fields_settable_by`'s own doc comment exists to draw. Found
/// by C4 PR B's whole-branch review.
#[test]
fn a_rule_that_sets_nothing_near_misses_nothing() {
    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();
    let mut proposed = crate::rules::read(&conn).unwrap().0;

    // The editor's freshly-added rule, exactly as `blankRule()` builds it.
    proposed.push(crate::rules::Rule {
        id: 0,
        ordinal: 9_000,
        enabled: true,
        root_id: None,
        kind: crate::rules::RuleKind::Regex,
        pattern: String::new(),
        match_on: crate::rules::MatchOn::Path,
        set_edition_key: None,
        set_kind: None,
        set_language: None,
        set_series: None,
        note: None,
    });
    let added = proposed.len() - 1;

    let (_impact, rows) = preview_with_rows(&conn, &proposed, Some(added), 500).unwrap();

    assert!(
        rows.rows.is_empty(),
        "a rule setting nothing reached {} rows; every one of them is an entry it had nothing          to say about, not one it missed",
        rows.rows.len()
    );
    assert!(!rows.truncated, "and so there is nothing to truncate");

    // The fixture must actually hold entries, or the assertion above passes
    // for the wrong reason — an empty library near-misses nothing either.
    let entries: i64 = conn
        .query_row("SELECT COUNT(*) FROM scan_entry", [], |r| r.get(0))
        .unwrap();
    assert!(entries > 0, "the fixture must have entries to have missed");
}

/// **A measurement, not a check.** C4 §11 marks its "34 ms" as an
/// illustration rather than a figure, because when that spec was written
/// nothing existed to measure. Something does now, and everything downstream
/// of "debounced on every keystroke" waits on this number.
///
/// Ignored by default and pointed at a real library through
/// `TAIYAKU_PREVIEW_DB`, the same shape as the corpus tests' own
/// `TAIYAKU_CORPUS`. Run it against a **copy**: it writes nothing, but a
/// measurement is not a reason to open the only real library on a machine.
///
/// ```sh
/// TAIYAKU_PREVIEW_DB=/path/to/copy.db \
///   cargo test -p taiyaku-library --release preview_cost -- --ignored --nocapture
/// ```
#[test]
#[ignore = "measurement; needs TAIYAKU_PREVIEW_DB"]
fn preview_cost_on_a_real_library() {
    let Some(path) = std::env::var_os("TAIYAKU_PREVIEW_DB") else {
        panic!("set TAIYAKU_PREVIEW_DB to a copy of a real library");
    };
    let library = crate::Library::open(std::path::Path::new(&path)).expect("opens the library");
    let conn = library.conn();

    let counts: Vec<(String, i64)> = ["series", "work", "edition", "member", "scan_entry", "rule"]
        .iter()
        .map(|t| {
            let n: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0))
                .expect("counting");
            ((*t).to_string(), n)
        })
        .collect();
    println!("library: {counts:?}");

    let current = crate::rules::read(&conn).expect("reading rules").0;
    let mut appended = current.clone();
    appended.push(crate::rules::Rule {
        id: 0,
        ordinal: 9_000,
        enabled: true,
        root_id: None,
        kind: crate::rules::RuleKind::Regex,
        pattern: "(?<series>.+)".to_string(),
        match_on: crate::rules::MatchOn::Path,
        set_edition_key: None,
        set_kind: None,
        set_language: None,
        set_series: None,
        note: Some("preview cost measurement".to_string()),
    });

    for (label, rules) in [("unchanged", &current), ("one rule appended", &appended)] {
        let mut times = Vec::new();
        for _ in 0..10 {
            let start = std::time::Instant::now();
            let impact = crate::preview::preview(&conn, rules).expect("previewing");
            times.push(start.elapsed());
            std::hint::black_box(impact);
        }
        times.sort_unstable();
        println!(
            "{label}: median {:?}  best {:?}  worst {:?}",
            times[times.len() / 2],
            times[0],
            times[times.len() - 1]
        );
        println!(
            "  impact: {:?}",
            crate::preview::preview(&conn, rules).unwrap()
        );
    }

    let start = std::time::Instant::now();
    let grouping = crate::preview::grouping(&conn).expect("grouping");
    println!(
        "grouping alone: {:?} over {} members",
        start.elapsed(),
        grouping.len()
    );
}

// --- Task 5: matched files, near-misses and provenance ---------------------

/// C4 §1: "a regex's *failure* is the thing you cannot see by reading it". A
/// near-miss here is an entry this rule did **not** match whose field nothing
/// else decided either — `Yu Yu Hakusho - Colored (Colored Council)` against a
/// colour-edition regex, with `edition_key` left completely undecided.
#[test]
fn matched_files_include_the_entries_the_rule_missed_and_nothing_else_caught() {
    let fixture = crate::tests::library_with_two_colour_editions();
    let conn = fixture.conn();
    let mut proposed = crate::rules::read(&conn).unwrap().0;
    let colour = crate::tests::a_colour_edition_rule(); // matches "- Full Color"
    let colour_index = proposed.len();
    proposed.push(colour.clone());

    let (_impact, rows) = preview_with_rows(&conn, &proposed, Some(colour_index), 200).unwrap();

    let missed: Vec<&str> = rows
        .rows
        .iter()
        .filter(|r| !r.matched)
        .map(|r| r.rel_path.as_str())
        .collect();
    assert!(
        missed
            .iter()
            .any(|p| p.contains("Colored (Colored Council)")),
        "the near-miss is the row worth showing, and it is not here: {missed:?}"
    );
    assert!(
        !rows.truncated,
        "200 is well past this fixture's own entry count; nothing should have been cut"
    );
}

/// **Fix round 2, Important 2.** Every rule above this one sets exactly one
/// field, and for a one-element set `.all()` and `.any()` are
/// indistinguishable — a reviewer mutating the near-miss check's `.all(` to
/// `.any(` left every test in this module green. The two only diverge for a
/// rule that sets *two* fields, over an entry where one of those two is
/// already decided by some other rule and the other is genuinely undecided:
/// under `.all()` (this module's own near-miss definition — "neither was
/// caught by anything else") that entry is excluded outright, not shown as a
/// near-miss; under `.any()` it would be shown, because one of the two
/// fields is still free.
///
/// **No new fixture rule needed — `library_with_two_colour_editions`'s own
/// seeded defaults supply the split decision.** Rule 4
/// (`^(?<series>.+?)[ _-]+v`, matched on the file's own name) already
/// captures `series` for
/// `"Yu Yu Hakusho - Colored (Colored Council)/Yu Yu Hakusho - Colored
/// (Colored Council) - v01.cbz"` — its name contains `" - v01"` — while
/// nothing in the seeded set ever touches `edition_key` at all. Adding
/// `set_series` to the colour-edition candidate below (alongside its
/// existing `set_edition_key`) is the whole change: for that one file,
/// `Series` is already decided by rule 4 and `EditionKey` is not decided by
/// anything, so `fields_set = {Series, EditionKey}` is split down the middle
/// — exactly the case a one-field rule can never construct.
#[test]
fn a_two_field_rule_is_a_near_miss_only_when_neither_field_was_caught_elsewhere() {
    let fixture = crate::tests::library_with_two_colour_editions();
    let conn = fixture.conn();
    let mut proposed = crate::rules::read(&conn).unwrap().0;
    let colour_index = proposed.len();
    proposed.push(crate::rules::Rule {
        id: 0,
        ordinal: 500,
        enabled: true,
        root_id: None,
        kind: crate::rules::RuleKind::Regex,
        pattern: "- Full Color".to_string(),
        match_on: crate::rules::MatchOn::Path,
        set_edition_key: Some("full-color".to_string()),
        set_kind: None,
        set_language: None,
        // The second field is the whole point — a rule setting only
        // `edition_key`, like `crate::tests::a_colour_edition_rule`, cannot
        // tell `.all()` from `.any()` apart at all.
        set_series: Some("Yu Yu Hakusho (Full Color)".to_string()),
        note: Some("two-field colour edition: matches \"- Full Color\"".to_string()),
    });

    let (_impact, rows) = preview_with_rows(&conn, &proposed, Some(colour_index), 200).unwrap();

    let split_decision_file = "Yu Yu Hakusho - Colored (Colored Council)/Yu Yu Hakusho - Colored (Colored Council) - \
         v01.cbz";
    assert!(
        rows.rows.iter().all(|r| r.rel_path != split_decision_file),
        "Series is already decided by the seeded default rule that captures it from this \
         file's own name, so this entry is not a near-miss even though EditionKey is still \
         wholly undecided — `.all()`'s own definition of a near-miss, and the one `.any()` \
         gets wrong: {:?}",
        rows.rows
    );

    // Sanity: the candidate rule's own genuine match must still show up, so
    // an empty table is not what is making the assertion above pass.
    let full_color_file = "Yu Yu Hakusho - Full Color/Yu Yu Hakusho - Full Color - v01.cbz";
    assert!(
        rows.rows
            .iter()
            .any(|r| r.rel_path == full_color_file && r.matched),
        "the candidate rule must still match its own genuine entry: {:?}",
        rows.rows
    );
}

/// Provenance says which rule set each field, and says so for the root's own
/// column too — over a rule kept from the database, addressed by its
/// position in `proposed` like every other rule in this design.
///
/// **Why `library_with_two_roots`, not the colour-edition fixture above.**
/// That fixture's root deliberately carries no `edition_key` default at all
/// — the near-miss above has to be *undecided*, not merely root-decided, to
/// prove the miss is total. This test needs the opposite: a field that
/// genuinely is decided by the root's own column, which is exactly what
/// `library_with_two_roots`'s own fixture comment gives every one of its
/// three roots, for this test by name.
#[test]
fn provenance_names_the_root_default_when_the_roots_own_column_decides_a_field() {
    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();
    let proposed = crate::rules::read(&conn).unwrap().0;

    // Index 0 — `proposed`'s first rule, whatever database id it happens to
    // hold. Nothing past this call ever needs that id again.
    let (_impact, rows) = preview_with_rows(&conn, &proposed, Some(0), 200).unwrap();
    assert!(
        !rows.rows.is_empty(),
        "rule 1 must reach at least one scan_entry in this fixture, or there is no row to check"
    );
    let decided = &rows.rows[0].decided_by;

    // The fixture's every root carries `edition_key` and no rule sets it, so
    // this field is decided by the root's own column and by nothing else —
    // the one case where `RootDefault` is the only correct answer. Asserting
    // "`RootDefault` **or** any rule" would pass whatever the mapping did.
    assert_eq!(
        decided.get(&crate::rules::Field::EditionKey),
        Some(&Decided::RootDefault),
        "the root's own column decided this and the wire format must say so: {decided:?}"
    );
}

/// The cap is real, and so is the flag that says the table was cut short —
/// the same "a partial table cannot read as a complete one" discipline
/// `inventory` already holds over its own listings.
///
/// Rule 2 (`builtin-volume`) matches all three of this fixture's volume
/// files — `Alpha - v03.cbz`, `Beta - v01.cbz`, `Gamma - v02.cbz` — and
/// nothing else in the seeded rule set ever sets `Field::Volume`, so it is
/// the sole decider on every one of them: exactly three matched rows.
///
/// **The uncapped table holds more than three rows, and that is correct, not
/// a bug this test papers over.** Every bare directory this fixture scanned —
/// `Alpha`, `Beta`, `Gamma`, and each root's own `""` entry — never gets a
/// volume number at all (`builtin-volume` reads a marker out of a *file*
/// name), so `Field::Volume` is genuinely undecided on every one of them: six
/// more rows, all near-misses by this module's own definition. The cap test
/// only needs the *matched* three to be exactly three and to sort first, and
/// a `limit` inside that matched region (`2`, below) is what keeps the
/// truncation check free of that near-miss count entirely.
#[test]
fn the_row_cap_is_honoured_and_the_truncation_flag_says_so() {
    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();
    let proposed = crate::rules::read(&conn).unwrap().0;
    let volume_rule = proposed
        .iter()
        .position(|r| r.kind == crate::rules::RuleKind::BuiltinVolume)
        .expect("the seeded rule set carries a builtin-volume rule");

    let (_impact, full) = preview_with_rows(&conn, &proposed, Some(volume_rule), 200).unwrap();
    let matched: Vec<_> = full.rows.iter().filter(|r| r.matched).collect();
    assert_eq!(
        matched.len(),
        3,
        "all three volume files must match rule 2 with nothing else deciding Volume: {:?}",
        full.rows
    );
    assert!(
        full.rows.len() > 3,
        "the bare directories and root entries must show up too, as near-misses on Volume — \
         undecided is undecided: {:?}",
        full.rows
    );
    assert!(
        full.rows[..3].iter().all(|r| r.matched),
        "matches must sort before near-misses: {:?}",
        full.rows
    );
    assert!(
        !full.truncated,
        "200 is well past this fixture's own entry count; nothing should have been cut"
    );

    let (_impact, capped) = preview_with_rows(&conn, &proposed, Some(volume_rule), 2).unwrap();
    assert_eq!(
        capped.rows.len(),
        2,
        "the cap must be honoured even though more rows exist"
    );
    assert!(
        capped.truncated,
        "cutting the table down to two must say so, not read as though only two ever existed"
    );
    assert_eq!(
        capped.rows,
        &full.rows[..2],
        "the two rows kept must be the same two rel_path-ordered rows the uncapped table starts \
         with, not some other two"
    );
}

/// **Fix round 2, Minor 3.** Mutating `rows.len() > limit` to `>=` leaves
/// every test above passing — none of them ever sets `limit` to exactly the
/// row count a fixture holds, only well past it (`200`) or well short of it
/// (`2`). The shipped code is correct; the gap is that a future regression
/// making `truncated: true` for a complete table would go unnoticed. Pinned
/// directly here: a `limit` equal to the real, uncapped row count must still
/// read as untruncated.
#[test]
fn a_limit_equal_to_the_exact_row_count_is_not_truncated() {
    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();
    let proposed = crate::rules::read(&conn).unwrap().0;
    let volume_rule = proposed
        .iter()
        .position(|r| r.kind == crate::rules::RuleKind::BuiltinVolume)
        .expect("the seeded rule set carries a builtin-volume rule");

    let (_impact, uncapped) = preview_with_rows(&conn, &proposed, Some(volume_rule), 200).unwrap();
    let exact = uncapped.rows.len();
    assert!(
        exact > 0,
        "the fixture must hold at least one row, or a limit of zero proves nothing about the \
         boundary"
    );

    let (_impact, at_boundary) =
        preview_with_rows(&conn, &proposed, Some(volume_rule), exact).unwrap();

    assert_eq!(
        at_boundary.rows.len(),
        exact,
        "a limit exactly matching the row count must keep every row"
    );
    assert!(
        !at_boundary.truncated,
        "exactly `limit` rows available must read as a complete table, not a truncated one — \
         `rows.len() > limit` must stay strict, or a regression to `>=` would silently flag a \
         complete table as cut short"
    );
}

/// **The skip has to be real, not incidental.** `selected: None` is what the
/// HTTP route sends when no rule is open in the editor, and it must not merely
/// happen to produce an empty table on some fixture — it must never touch
/// `scan_entry` at all. Reusing `library_with_two_roots` here is deliberate:
/// every one of its roots is scoped so that `Some(rule_1_id)` would return
/// several rows (near-misses, at least — see the provenance test above), so a
/// build that quietly ignored `selected` and built rows anyway has fixture
/// evidence to be caught by, not a vacuous "there was never anything to find"
/// pass.
#[test]
fn no_rule_selected_builds_no_rows_at_all() {
    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();
    let proposed = crate::rules::read(&conn).unwrap().0;

    let (_impact, rows) = preview_with_rows(&conn, &proposed, None, 200).unwrap();

    assert_eq!(
        rows,
        Rows::default(),
        "no rule open in the editor must build no rows at all, not merely return none"
    );
}

// --- Fix round 1: a rule is addressed by list position, not by id ----------
//
// `selected` used to be a rule id, and an unsaved rule's `id` is always `0` —
// `rules::replace_all`'s own "mint a fresh id" sentinel — which collides with
// `rules::ROOT_FALLBACK`, also `0`. See `Decided`'s own doc comment.

/// **The mis-attribution collision, reproduced directly.** An id-keyed
/// `Decided` cannot tell "the unsaved rule sitting in the editor" apart from
/// "the root's own column decided this" — both are `0` — so previewing a
/// brand-new rule against a root that sets `edition_key` would silently
/// report that field as decided by the unsaved rule. `library_with_two_roots`
/// gives every one of its three roots an `edition_key` default and none of
/// its four seeded rules ever sets that field, so `EditionKey` is root-decided
/// on every entry this fixture has — the fixture the collision needs to have
/// anything to mis-attribute in the first place.
#[test]
fn previewing_an_unsaved_rule_does_not_attribute_the_roots_own_default_to_it() {
    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();
    let mut proposed = crate::rules::read(&conn).unwrap().0;
    assert!(
        proposed.iter().all(|r| r.set_edition_key.is_none()),
        "none of the seeded default rules may set edition_key, or this fixture no longer proves \
         the field is root-decided rather than merely undecided"
    );

    // `id: 0`, never saved — exactly the rule a person is still typing into
    // C4's editor. Matches every path (`".*"`) and sets only `series`, never
    // `edition_key`.
    let unsaved_index = proposed.len();
    proposed.push(crate::tests::a_rule_setting_series("Zeta"));

    let (_impact, rows) = preview_with_rows(&conn, &proposed, Some(unsaved_index), 200).unwrap();
    let row = rows
        .rows
        .iter()
        .find(|r| r.matched)
        .expect("the unsaved rule's \".*\" pattern must match at least one scan_entry");

    assert_eq!(
        row.decided_by.get(&crate::rules::Field::EditionKey),
        Some(&Decided::RootDefault),
        "the root's own column decided edition_key here, not the unsaved rule open in the \
         editor: {:?}",
        row.decided_by
    );
    assert_eq!(
        row.decided_by.get(&crate::rules::Field::Series),
        Some(&Decided::Rule(unsaved_index)),
        "the unsaved rule must still be named for the one field it actually set: {:?}",
        row.decided_by
    );
}

/// **The second half of the same collision.** Two rules the editor has not
/// saved yet both carry `id: 0` — an id-keyed `Decided` cannot tell them
/// apart at all, so selecting either would resolve to whichever the id
/// happened to name first. Addressed by index, each is its own, unambiguous
/// position in `proposed`, regardless of how many other unsaved rules sit
/// beside it.
#[test]
fn two_unsaved_rules_in_one_proposed_list_are_distinguishable_by_their_index() {
    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();
    let mut proposed = crate::rules::read(&conn).unwrap().0;

    let beta_index = proposed.len();
    proposed.push(crate::rules::Rule {
        id: 0,
        ordinal: 500,
        enabled: true,
        root_id: None,
        kind: crate::rules::RuleKind::Prefix,
        pattern: "Beta/".to_string(),
        match_on: crate::rules::MatchOn::Path,
        set_edition_key: None,
        set_kind: None,
        set_language: None,
        set_series: Some("Beta Renamed".to_string()),
        note: Some("unsaved candidate A: matches only Beta's own file".to_string()),
    });
    let gamma_index = proposed.len();
    proposed.push(crate::rules::Rule {
        id: 0,
        ordinal: 501,
        enabled: true,
        root_id: None,
        kind: crate::rules::RuleKind::Prefix,
        pattern: "Gamma/".to_string(),
        match_on: crate::rules::MatchOn::Path,
        set_edition_key: None,
        set_kind: None,
        set_language: None,
        set_series: Some("Gamma Renamed".to_string()),
        note: Some("unsaved candidate B: matches only Gamma's own file".to_string()),
    });

    let (_impact, beta_rows) = preview_with_rows(&conn, &proposed, Some(beta_index), 200).unwrap();
    let (_impact, gamma_rows) =
        preview_with_rows(&conn, &proposed, Some(gamma_index), 200).unwrap();

    let beta_matched: Vec<&str> = beta_rows
        .rows
        .iter()
        .filter(|r| r.matched)
        .map(|r| r.rel_path.as_str())
        .collect();
    let gamma_matched: Vec<&str> = gamma_rows
        .rows
        .iter()
        .filter(|r| r.matched)
        .map(|r| r.rel_path.as_str())
        .collect();

    assert_eq!(
        beta_matched,
        vec!["Beta/Beta - v01.cbz"],
        "candidate A, selected by its own index, must match only Beta's own file: \
         {beta_matched:?}"
    );
    assert_eq!(
        gamma_matched,
        vec!["Gamma/Gamma - v02.cbz"],
        "candidate B, selected by its own index, must match only Gamma's own file: \
         {gamma_matched:?}"
    );
}

/// A rule already in the database and a rule the editor has not saved yet
/// sit in the same proposed list, and both are addressable by their own
/// index — the kept rule's real database id never has to be looked up, and
/// the new rule's `id: 0` never has to be told apart from anything.
#[test]
fn a_kept_rule_and_a_newly_added_rule_are_both_addressable_by_their_index() {
    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();
    let mut proposed = crate::rules::read(&conn).unwrap().0;
    let volume_index = proposed
        .iter()
        .position(|r| r.kind == crate::rules::RuleKind::BuiltinVolume)
        .expect("the seeded rule set carries a builtin-volume rule");

    let new_index = proposed.len();
    proposed.push(crate::rules::Rule {
        id: 0,
        ordinal: 500,
        enabled: true,
        root_id: None,
        kind: crate::rules::RuleKind::Prefix,
        pattern: "Alpha/".to_string(),
        match_on: crate::rules::MatchOn::Path,
        set_edition_key: None,
        set_kind: None,
        set_language: None,
        set_series: Some("Alpha Renamed".to_string()),
        note: Some("a newly added, unsaved rule".to_string()),
    });

    let (_impact, kept_rows) =
        preview_with_rows(&conn, &proposed, Some(volume_index), 200).unwrap();
    let (_impact, new_rows) = preview_with_rows(&conn, &proposed, Some(new_index), 200).unwrap();

    let kept_matched: std::collections::BTreeSet<&str> = kept_rows
        .rows
        .iter()
        .filter(|r| r.matched)
        .map(|r| r.rel_path.as_str())
        .collect();
    assert_eq!(
        kept_matched,
        std::collections::BTreeSet::from([
            "Alpha/Alpha - v03.cbz",
            "Beta/Beta - v01.cbz",
            "Gamma/Gamma - v02.cbz",
        ]),
        "the kept builtin-volume rule, addressed by its own index, must still match exactly the \
         three volume files: {kept_matched:?}"
    );

    let new_matched: Vec<&str> = new_rows
        .rows
        .iter()
        .filter(|r| r.matched)
        .map(|r| r.rel_path.as_str())
        .collect();
    assert_eq!(
        new_matched,
        vec!["Alpha/Alpha - v03.cbz"],
        "the newly added rule, addressed by its own index, must match only Alpha's own file: \
         {new_matched:?}"
    );
}

/// **`Rule::root_id` actually scopes `rows_for`'s walk.** Every rule in
/// every fixture above carries `root_id: None` — nothing before this test
/// ever proposed a rule scoped to one root and checked which roots its rows
/// came from, so `rows_for`'s own `if rule.root_id.is_some_and(|scoped|
/// scoped != root.id) { continue; }` had nothing holding it to `!=` rather
/// than `==`: flipping it inverts which roots a scoped rule reaches — skip
/// the one it names and walk every other one instead — and the whole suite
/// stayed green.
#[test]
fn a_root_scoped_rule_only_matches_scan_entries_under_that_root() {
    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();
    let root_a = crate::scan::roots(&conn)
        .unwrap()
        .into_iter()
        .find(|r| r.path.file_name() == Some(std::ffi::OsStr::new("root-a")))
        .expect("library_with_two_roots's own root a, holding Beta");

    let mut proposed = crate::rules::read(&conn).unwrap().0;
    let scoped_index = proposed.len();
    // `Prefix "Beta/"`, not `Regex ".*"` — `scan_root` writes a `scan_entry`
    // for every directory level, not only the archive itself, and `"Beta"`
    // (no trailing slash) and `""` (the root's own entry) both fail
    // `starts_with("Beta/")`, the same trick
    // `two_unsaved_rules_in_one_proposed_list_are_distinguishable_by_their_index`
    // uses to match only the one file this test cares about.
    proposed.push(crate::rules::Rule {
        id: 0,
        ordinal: 500,
        enabled: true,
        root_id: Some(root_a.id),
        kind: crate::rules::RuleKind::Prefix,
        pattern: "Beta/".to_string(),
        match_on: crate::rules::MatchOn::Path,
        set_edition_key: None,
        set_kind: None,
        set_language: None,
        set_series: Some("Scoped".to_string()),
        note: Some("scoped to root a alone".to_string()),
    });

    let (_impact, rows) = preview_with_rows(&conn, &proposed, Some(scoped_index), 200).unwrap();

    assert!(
        !rows.rows.is_empty(),
        "the scoped rule's \"Beta/\" prefix must still match at least root a's own file"
    );
    assert!(
        rows.rows.iter().all(|r| r.root_id == root_a.id),
        "a rule scoped to root a must never produce a row from any other root: {:?}",
        rows.rows
    );
    let matched: Vec<&str> = rows
        .rows
        .iter()
        .filter(|r| r.matched)
        .map(|r| r.rel_path.as_str())
        .collect();
    assert_eq!(
        matched,
        vec!["Beta/Beta - v01.cbz"],
        "root a holds only Beta — Gamma and Alpha sit under root b and root c, and a scoped rule \
         must never reach either: {matched:?}"
    );
}

/// An index naming no position in `proposed` must be refused, naming both
/// the index a caller asked for and the length it should have fit inside —
/// not silently read as "no rule selected."
#[test]
fn an_out_of_range_selected_index_is_refused_naming_the_index_and_the_lists_length() {
    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();
    let proposed = crate::rules::read(&conn).unwrap().0;
    let out_of_range = proposed.len() + 5;

    let err = preview_with_rows(&conn, &proposed, Some(out_of_range), 200)
        .expect_err("an index past the end of `proposed` names no rule");

    assert!(
        err.contains(&out_of_range.to_string()),
        "the error must name the index that was actually asked for: {err}"
    );
    assert!(
        err.contains(&proposed.len().to_string()),
        "the error must name the proposed list's own length: {err}"
    );
}

// --- conflict detection ------------------------------------------------

/// C4 §1's own worked example, and the reason `conflicts` exists: a rule
/// sets `edition_key = "raw"` for everything under `Manga (RAW)/`, a later
/// colour rule sets `edition_key = "en-color"` for anything ending "Full
/// Color", both match `Manga (RAW)/Go-Toubun no Hanayome - Full Color`, the
/// later wins, and a Japanese colour edition is labelled English. Correct
/// behaviour producing a wrong answer.
///
/// **Pins `first` and `wins` individually, not merely that they differ.** A
/// bare `assert_ne!(one.first, one.wins)` cannot tell a correct answer from
/// the two swapped — both are "not equal" either way. Checking which of
/// `proposed`'s own rules each index actually names (`set_edition_key ==
/// Some("raw")` for `first`, `"en-color"` for `wins`) is what a swap
/// mutation cannot survive.
///
/// **Filtered to `EditionKey` explicitly, not just `rel_path.contains(..)`.**
/// The seeded default rule 1 captures `series` from a path's *first*
/// directory level only (`^(?<series>[^/]+)/`) — a real limitation this
/// fixture's own two-level `Manga (RAW)/<series>/` path incidentally
/// surfaces: rule 1 reads the file's series as `"Manga (RAW)"`, rule 4
/// (filename-based) reads it correctly, and the two genuinely disagree,
/// which `conflicts` correctly reports as a *second*, unrelated `Series`
/// conflict on the same file. `Series` sorts ahead of `EditionKey` in
/// `crate::rules::Field`'s own declaration order, so an unfiltered
/// `.find(rel_path.contains(..))` finds that one first. This test is about
/// C4 §1's `EditionKey` conflict specifically, not about rule 1's own
/// separate, orthogonal limitation.
#[test]
fn two_rules_setting_edition_on_one_file_are_reported_with_both_ids() {
    let fixture = crate::tests::library_with_the_seeded_defaults();
    let conn = fixture.conn();
    let proposed = crate::rules::read(&conn).unwrap().0;

    let found = conflicts(&conn, &proposed).unwrap();

    let one = found
        .iter()
        .find(|c| {
            c.field == crate::rules::Field::EditionKey
                && c.rel_path.contains("Go-Toubun no Hanayome - Full Color")
        })
        .expect("the seeded defaults' own conflict is not reported");
    assert_eq!(one.field, crate::rules::Field::EditionKey);
    assert_ne!(
        one.first, one.wins,
        "a conflict names both rules, not one twice"
    );
    assert_eq!(
        proposed[one.first].set_edition_key.as_deref(),
        Some("raw"),
        "`first` must name the rule that lost — the raw-edition one — not the winner: {found:?}"
    );
    assert_eq!(
        proposed[one.wins].set_edition_key.as_deref(),
        Some("en-color"),
        "`wins` must name the rule that actually decided edition_key for real — the colour \
         one — not the one it overrode: {found:?}"
    );
    assert_eq!(
        one.others, 0,
        "the ordinary two-rule case has no disagreement besides the one `first` already names, \
         so `others` must read 0 and the UI must show nothing extra: {found:?}"
    );
}

/// A field one rule alone decides is not a conflict, however many files it
/// touches.
///
/// **Not `crate::tests::a_rule_setting_series` and not `Field::Series`,
/// despite that being the brief this test was originally written from.**
/// `library_with_two_roots` already carries two of its own seeded rules that
/// set `Field::Series` — migration 3's rule 1, from the parent directory,
/// and its rule 4, from the filename "where it repeats it" — and every file
/// this fixture writes is named so the two agree (`Beta/Beta - v01.cbz`
/// yields `"Beta"` both ways), which is exactly the harmless case
/// [`Conflict`]'s own doc comment carves out: two rules setting a field to
/// the *same* value are not reported. `a_rule_setting_series("Alpha")`
/// pushes a rule that sends *every* file to the literal string `"Alpha"` —
/// which genuinely disagrees with what rule 4 already captured for `Beta`
/// and `Gamma` (only this fixture's own `Alpha` root already reads
/// `"Alpha"`, per its own doc comment) — so using it here would not test
/// "one rule alone decides a field" at all; it would recreate C4 §1's own
/// shape by accident, and a correct `conflicts` reports it, which is not
/// what this test is for. `Field::Kind` is untouched by every seeded rule
/// and by this fixture's own roots (only `edition_key` is set on them), so
/// a rule setting it really is the only thing that ever decides it here.
#[test]
fn one_rule_setting_a_field_across_the_library_is_not_a_conflict() {
    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();
    let mut proposed = crate::rules::read(&conn).unwrap().0;
    proposed.push(crate::rules::Rule {
        id: 0,
        ordinal: 500,
        enabled: true,
        root_id: None,
        kind: crate::rules::RuleKind::Regex,
        pattern: ".*".to_string(),
        match_on: crate::rules::MatchOn::Path,
        set_edition_key: None,
        set_kind: Some("manga".to_string()),
        set_language: None,
        set_series: None,
        note: Some("the only rule in this fixture that ever sets Kind".to_string()),
    });

    let found = conflicts(&conn, &proposed).unwrap();
    assert!(
        found.iter().all(|c| c.field != crate::rules::Field::Kind),
        "one rule deciding Kind for every file in the library is not a conflict: {found:?}"
    );
}

/// A disabled rule can never be `first` or `wins`, even when its own value
/// would genuinely disagree with the enabled rule that is the field's only
/// real decider — [`crate::rules::Engine::resolve`] skips a disabled rule
/// outright, so it never reaches `apply_set_columns` for it at all, and
/// [`conflicts`] leans on exactly that rather than filtering `enabled`
/// itself. The C4 editor shows disabled rules in the same list as enabled
/// ones, which is what makes this worth pinning rather than assuming.
#[test]
fn a_disabled_rules_own_value_is_never_compared_against_the_enabled_rule_that_alone_decides_a_field()
 {
    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();
    let mut proposed = crate::rules::read(&conn).unwrap().0;
    proposed.push(crate::rules::Rule {
        id: 0,
        ordinal: 500,
        enabled: true,
        root_id: None,
        kind: crate::rules::RuleKind::Regex,
        pattern: ".*".to_string(),
        match_on: crate::rules::MatchOn::Path,
        set_edition_key: None,
        set_kind: Some("manga".to_string()),
        set_language: None,
        set_series: None,
        note: None,
    });
    proposed.push(crate::rules::Rule {
        id: 0,
        ordinal: 501,
        enabled: false,
        root_id: None,
        kind: crate::rules::RuleKind::Regex,
        pattern: ".*".to_string(),
        match_on: crate::rules::MatchOn::Path,
        set_edition_key: None,
        set_kind: Some("light-novel".to_string()),
        set_language: None,
        set_series: None,
        note: Some("disabled: would disagree with the rule above if it ever ran".to_string()),
    });

    let found = conflicts(&conn, &proposed).unwrap();
    assert!(
        found.iter().all(|c| c.field != crate::rules::Field::Kind),
        "the disabled rule must never be counted as a second decider of Kind: {found:?}"
    );
}

/// One [`Conflict`] per (field, first, wins), not one row per file — its
/// `examples` count is the number of entries that actually share it, read
/// back and checked for real rather than assumed to be more than the `1` a
/// hard-coded count would also produce.
///
/// `library_with_two_roots` holds three roots, each contributing three
/// `scan_entry` rows (the root itself, the series directory, the archive) —
/// nine rows in total, every one of which the two `".*"` rules below match,
/// so nine is what a genuine count reads back as.
#[test]
fn the_conflict_count_is_the_real_number_of_entries_that_share_it_not_a_constant() {
    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();
    let mut proposed = crate::rules::read(&conn).unwrap().0;
    proposed.push(crate::rules::Rule {
        id: 0,
        ordinal: 500,
        enabled: true,
        root_id: None,
        kind: crate::rules::RuleKind::Regex,
        pattern: ".*".to_string(),
        match_on: crate::rules::MatchOn::Path,
        set_edition_key: None,
        set_kind: Some("manga".to_string()),
        set_language: None,
        set_series: None,
        note: None,
    });
    proposed.push(crate::rules::Rule {
        id: 0,
        ordinal: 501,
        enabled: true,
        root_id: None,
        kind: crate::rules::RuleKind::Regex,
        pattern: ".*".to_string(),
        match_on: crate::rules::MatchOn::Path,
        set_edition_key: None,
        set_kind: Some("light-novel".to_string()),
        set_language: None,
        set_series: None,
        note: None,
    });

    let found = conflicts(&conn, &proposed).unwrap();
    let kind_conflicts: Vec<&Conflict> = found
        .iter()
        .filter(|c| c.field == crate::rules::Field::Kind)
        .collect();
    assert_eq!(
        kind_conflicts.len(),
        1,
        "both rules match every entry under every scope, so this is one (field, first, wins) \
         triple, not several: {found:?}"
    );
    assert_eq!(
        kind_conflicts[0].examples, 9,
        "every one of the fixture's nine scan_entry rows shares this conflict: {found:?}"
    );
}

/// `first` names the most recent **disagreement**, not merely the earliest
/// rule to touch the field — the distinction the value-aware reduction in
/// `conflicts` (`earlier.iter().rev().find(|(_, value)| value != wins_value)`)
/// exists for, and the one every other `preview::` test up to this one
/// happens not to exercise: replacing that reduction with `earlier.first()`
/// still passes all of them.
///
/// Three rules on one file: ordinal 500 sets `Kind = "Z"`, ordinal 501 sets
/// `Kind = "Y"`, ordinal 502 sets `Kind = "Z"` and wins. `earlier.first()`
/// would report the ordinal-500 rule — whose value is *identical* to the
/// winner's, directly contradicting [`Conflict`]'s own doc comment on what a
/// conflict is about. The correct answer is the ordinal-501 rule: the only
/// earlier rule whose value for `Kind` actually differs from `"Z"`.
///
/// `Field::Kind` again, for the same reason
/// `one_rule_setting_a_field_across_the_library_is_not_a_conflict` uses it:
/// untouched by every seeded rule and by `library_with_two_roots`'s own root
/// defaults, so nothing else in this fixture can produce a `Kind` conflict
/// by accident.
#[test]
fn the_first_disagreement_is_reported_even_when_an_earlier_rule_agreed_with_the_winner() {
    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();
    let mut proposed = crate::rules::read(&conn).unwrap().0;
    proposed.push(crate::rules::Rule {
        id: 0,
        ordinal: 500,
        enabled: true,
        root_id: None,
        kind: crate::rules::RuleKind::Regex,
        pattern: ".*".to_string(),
        match_on: crate::rules::MatchOn::Path,
        set_edition_key: None,
        set_kind: Some("Z".to_string()),
        set_language: None,
        set_series: None,
        note: Some(
            "agrees with the eventual winner — must never be reported as `first`".to_string(),
        ),
    });
    proposed.push(crate::rules::Rule {
        id: 0,
        ordinal: 501,
        enabled: true,
        root_id: None,
        kind: crate::rules::RuleKind::Regex,
        pattern: ".*".to_string(),
        match_on: crate::rules::MatchOn::Path,
        set_edition_key: None,
        set_kind: Some("Y".to_string()),
        set_language: None,
        set_series: None,
        note: Some("the only rule whose value genuinely disagrees with the winner".to_string()),
    });
    proposed.push(crate::rules::Rule {
        id: 0,
        ordinal: 502,
        enabled: true,
        root_id: None,
        kind: crate::rules::RuleKind::Regex,
        pattern: ".*".to_string(),
        match_on: crate::rules::MatchOn::Path,
        set_edition_key: None,
        set_kind: Some("Z".to_string()),
        set_language: None,
        set_series: None,
        note: Some("wins".to_string()),
    });

    let found = conflicts(&conn, &proposed).unwrap();
    let kind_conflicts: Vec<&Conflict> = found
        .iter()
        .filter(|c| c.field == crate::rules::Field::Kind)
        .collect();
    assert_eq!(
        kind_conflicts.len(),
        1,
        "one (field, first, wins) triple, not one per file or per rule pair: {found:?}"
    );
    let conflict = kind_conflicts[0];
    assert_eq!(
        proposed[conflict.wins].set_kind.as_deref(),
        Some("Z"),
        "`wins` must name the ordinal-502 rule: {found:?}"
    );
    assert_eq!(
        proposed[conflict.first].set_kind.as_deref(),
        Some("Y"),
        "`first` must name the ordinal-501 rule — the only earlier rule whose value genuinely \
         differs from the winner's `\"Z\"` — not the ordinal-500 rule, whose value is the same \
         `\"Z\"` the winner produces: {found:?}"
    );
    assert_eq!(
        conflict.others, 0,
        "only one earlier rule (ordinal 501) actually disagrees with the winner — the \
         ordinal-500 rule agrees with it and so is not a disagreement to count: {found:?}"
    );
}

/// Three rules that pairwise disagree — `Kind` set to `"v1"`, `"v2"`, then
/// `"v3"`, all distinct — still produce exactly one `Conflict` for the
/// field: `first` names `"v2"`'s rule (the most recent disagreement, per
/// [`Conflict`]'s own doc comment), `wins` names `"v3"`'s. The
/// `"v1"`-versus-`"v3"` disagreement is real and is not the one named by
/// `first` — `others` is how that stays discoverable instead of silent:
/// decided by the owner 2026-09-03, recorded on [`Conflict`] itself.
#[test]
fn a_three_way_disagreement_reports_one_conflict_with_one_other() {
    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();
    let mut proposed = crate::rules::read(&conn).unwrap().0;
    proposed.push(crate::rules::Rule {
        id: 0,
        ordinal: 500,
        enabled: true,
        root_id: None,
        kind: crate::rules::RuleKind::Regex,
        pattern: ".*".to_string(),
        match_on: crate::rules::MatchOn::Path,
        set_edition_key: None,
        set_kind: Some("v1".to_string()),
        set_language: None,
        set_series: None,
        note: None,
    });
    proposed.push(crate::rules::Rule {
        id: 0,
        ordinal: 501,
        enabled: true,
        root_id: None,
        kind: crate::rules::RuleKind::Regex,
        pattern: ".*".to_string(),
        match_on: crate::rules::MatchOn::Path,
        set_edition_key: None,
        set_kind: Some("v2".to_string()),
        set_language: None,
        set_series: None,
        note: None,
    });
    proposed.push(crate::rules::Rule {
        id: 0,
        ordinal: 502,
        enabled: true,
        root_id: None,
        kind: crate::rules::RuleKind::Regex,
        pattern: ".*".to_string(),
        match_on: crate::rules::MatchOn::Path,
        set_edition_key: None,
        set_kind: Some("v3".to_string()),
        set_language: None,
        set_series: None,
        note: None,
    });

    let found = conflicts(&conn, &proposed).unwrap();
    let kind_conflicts: Vec<&Conflict> = found
        .iter()
        .filter(|c| c.field == crate::rules::Field::Kind)
        .collect();
    assert_eq!(
        kind_conflicts.len(),
        1,
        "one row for the field, not one per disagreeing pair — the N-choose-2 alternative the \
         owner's decision rejected: {found:?}"
    );
    let conflict = kind_conflicts[0];
    assert_eq!(
        proposed[conflict.first].set_kind.as_deref(),
        Some("v2"),
        "`first` must name the most recent disagreement, v2's rule: {found:?}"
    );
    assert_eq!(
        proposed[conflict.wins].set_kind.as_deref(),
        Some("v3"),
        "`wins` must name the rule that actually decided it, v3's: {found:?}"
    );
    assert_eq!(
        conflict.others, 1,
        "the v1-versus-v3 disagreement is real and is not the one `first` names — `others` is \
         what keeps it discoverable rather than silent: {found:?}"
    );
}

// --- the relabelled id `conflicts` mints for an unsaved rule --------------

/// **The relabelled id two same-ordinal unsaved rules get must preserve
/// their `proposed` order — the tie-break a real save would produce.**
/// `conflicts`'s own doc comment states the intent directly: ties are broken
/// "on the relabelled id exactly as `compile` breaks them on the real one,"
/// and a real [`crate::rules::replace_all`] mints a *later*-inserted
/// same-ordinal rule the *higher* id, since SQLite's own rowid only ever
/// increases. Every other `conflicts` test in this module gives its unsaved
/// rules distinct ordinals, so `ordinal` alone already orders them and this
/// tie-break never runs at all.
///
/// **Both of `i64::MAX - (proposed.len() as i64) + index as i64`'s own
/// surviving mutants break this the same way.** Replacing `+` with `-`
/// gives `i64::MAX - len - index`, decreasing as `index` increases.
/// Replacing `+` with `*` gives `i64::MAX - len * index` — `*` binds
/// tighter than the leading `-`, so this is `i64::MAX - (len * index)`, not
/// `(i64::MAX - len) * index` — which is *also* decreasing as `index`
/// increases, just at a different rate (verified by hand against
/// `cargo mutants --diff`, not inferred from the source text alone).
/// Both stay injective and stay far above any real row id — nothing about
/// *that* half of the contract breaks — but both silently reverse which of
/// two same-ordinal unsaved rules resolves last and so wins their tie.
#[test]
fn two_same_ordinal_unsaved_rules_break_their_tie_in_proposed_order() {
    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();

    let earlier_in_the_list = crate::rules::Rule {
        id: 0,
        ordinal: 500,
        enabled: true,
        root_id: None,
        kind: crate::rules::RuleKind::Regex,
        pattern: ".*".to_string(),
        match_on: crate::rules::MatchOn::Path,
        set_edition_key: None,
        set_kind: Some("earlier".to_string()),
        set_language: None,
        set_series: None,
        note: None,
    };
    let later_in_the_list = crate::rules::Rule {
        set_kind: Some("later".to_string()),
        ..earlier_in_the_list.clone()
    };
    let proposed = vec![earlier_in_the_list, later_in_the_list];

    let found = conflicts(&conn, &proposed).unwrap();
    let conflict = found
        .iter()
        .find(|c| c.field == crate::rules::Field::Kind)
        .expect("two same-ordinal rules disagreeing on Kind must be reported as a conflict");

    assert_eq!(
        proposed[conflict.wins].set_kind.as_deref(),
        Some("later"),
        "the rule appearing later in `proposed` must win an equal-ordinal tie — the same rule a \
         real save would mint the higher id for: {found:?}"
    );
    assert_eq!(
        proposed[conflict.first].set_kind.as_deref(),
        Some("earlier"),
        "and the earlier rule must be the one reported as the disagreement, not the winner: \
         {found:?}"
    );
}

// --- Task 2: apply, the committed twin --------------------------------------

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
    let (fixture, _base_rules) = crate::tests::library_with_an_accepted_proposal();
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

/// **I5.** `apply` used to keep only `report.raised` out of the
/// `DeriveReport` its own `Documents::Reconcile` pass produced — every other
/// field `hand::reconcile` populates (`repaired_documents`,
/// `unwritable_documents`, `orphan_documents`, `declined_documents`,
/// `stale_documents`) was computed and thrown away. This is the only route in
/// this product that writes under `<root>/.taiyaku/` and, before this,
/// the only `derive` caller that reported nothing about what happened there:
/// a save that could not write a hand document answered `200` with a rehome
/// count and silence.
///
/// **The technique is `hand/tests.rs`'s own** —
/// `a_root_that_cannot_take_the_document_still_arms_the_guard`'s plain file
/// where the directory belongs, not a read-only permission bit. CI runs as
/// root, which ignores the write bit, so a `set_readonly` root would let
/// `create_dir_all` succeed there and this test would pass on a developer's
/// own machine and nowhere it matters.
#[test]
fn apply_reports_a_document_it_could_not_write() {
    let fixture = crate::tests::library_with_a_kept_document();
    let conn = fixture.conn();
    let work_id: i64 = conn
        .query_row("SELECT id FROM work", [], |r| r.get(0))
        .expect("the fixture's one work");
    let path = crate::hand::path_for(&conn, work_id)
        .expect("resolving the document's address")
        .expect("a kept root's work has an address");

    // Reopens the gap `hand::reconcile`'s own gap repair exists to close —
    // the same move `a_preview_leaves_the_library_tree_byte_identical` uses —
    // then blocks the repair itself: a plain file where the `.taiyaku`
    // directory belongs makes `create_dir_all` fail with `AlreadyExists` for
    // any uid.
    std::fs::remove_file(&path).expect("clearing the settled document");
    let taiyaku_dir = path.parent().expect("an address always has a directory");
    std::fs::remove_dir(taiyaku_dir).expect("removing the now-empty .taiyaku directory");
    std::fs::write(taiyaku_dir, b"not a directory")
        .expect("planting a file where the directory must go");

    let proposed = crate::rules::read(&conn).unwrap().0;
    let applied = crate::preview::apply(&conn, &proposed)
        .expect("a document apply cannot write must not fail the save itself");

    assert_eq!(
        applied.derive_report.unwritable_documents.len(),
        1,
        "the save must report the document it could not write: {:?}",
        applied.derive_report.unwritable_documents
    );
    assert!(
        applied.derive_report.unwritable_documents[0]
            .contains(&taiyaku_dir.to_string_lossy().to_string()),
        "the reason should name the path that could not be written: {:?}",
        applied.derive_report.unwritable_documents
    );
}

/// **I6.** `apply` used to read the rule table back and compile an `Engine`
/// from it — `crate::rules::read`, fallible on a genuine SQLite fault —
/// between `derive_with` returning and its own `tx.commit()`. Any failure in
/// that window rolled the database back while the real files
/// `derive_with`'s own `Documents::Reconcile` pass had already written or
/// renamed stayed exactly where it left them: a hand document renamed to a
/// `content_key` the database never adopted, reported as an orphan on the
/// next pass — this module's own doc comment on `apply` states the argument
/// in full, and it is the reason [`preview_with_rows`] runs `Skip` instead.
///
/// **Watches the SQL `apply` actually sends rather than forcing the read to
/// fail** — the same `trace_v2` technique
/// `taiyaku-library/src/tests.rs`'s own
/// `committing_the_real_transaction_branch_sends_no_savepoint_statement`
/// uses, because there is no seam that fails `crate::rules::read`'s query
/// without also breaking the very commit it is meant to sit on one side of.
/// What this test can pin instead is the fact that actually decides which
/// side of `COMMIT` such a failure would land on: the order `apply` itself
/// sends statements to SQLite in.
#[test]
fn apply_reads_the_rule_table_back_after_committing_not_before() {
    use std::sync::Mutex;

    static RULE_APPLY_STATEMENTS: Mutex<Vec<String>> = Mutex::new(Vec::new());
    fn watch(event: rusqlite::trace::TraceEvent<'_>) {
        if let rusqlite::trace::TraceEvent::Stmt(_, sql) = event {
            RULE_APPLY_STATEMENTS.lock().unwrap().push(sql.to_string());
        }
    }

    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();
    let mut proposed = crate::rules::read(&conn).unwrap().0;
    proposed.push(crate::tests::a_rule_setting_series("Alpha"));

    conn.trace_v2(
        rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
        Some(watch),
    );
    crate::preview::apply(&conn, &proposed).expect("the save must not fail");
    conn.trace_v2(rusqlite::trace::TraceEventCodes::empty(), None);

    let statements = RULE_APPLY_STATEMENTS.lock().unwrap();
    let commit_at = statements
        .iter()
        .position(|sql| sql.trim().eq_ignore_ascii_case("COMMIT"))
        .expect("apply commits its transaction");
    // **Every occurrence, not just the first.** `derive_with`'s own
    // `Engine::load` reads the rule table too — the identical SQL text
    // `rules::read` sends — so the *first* occurrence is always the one
    // before the commit, whichever code this test runs against. What
    // distinguishes the fix is `apply`'s own read, the last one issued,
    // moving to the far side of `COMMIT`.
    let rule_read_positions: Vec<usize> = statements
        .iter()
        .enumerate()
        .filter(|(_, sql)| sql.contains("FROM rule ORDER BY ordinal, id"))
        .map(|(i, _)| i)
        .collect();
    assert!(
        rule_read_positions.len() >= 2,
        "expected at least two reads of the rule table — `derive_with`'s own `Engine::load` \
         before the commit, and `apply`'s own read of the table it just saved after it: {:?}",
        *statements
    );
    let last_rule_read_at = *rule_read_positions.last().expect("checked non-empty above");

    assert!(
        last_rule_read_at > commit_at,
        "apply's own read of the rule table back must come after the commit, not before it — a \
         failure in that read must never be able to roll the database back while the files \
         `derive_with` already wrote or renamed stay exactly where it put them: {:?}",
        *statements
    );
}
