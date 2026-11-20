use crate::proposals::Offer;
use crate::scan::tests::{fixture, write_pages};
use crate::{Library, derive, proposals, scan};

/// What a write made, where the test needs it made: a refusal or a store
/// failure fails the test with what it said.
fn made<T: std::fmt::Debug>(answered: Result<Offer<T>, String>) -> T {
    match answered {
        Ok(Offer::Made(made)) => made,
        other => panic!("expected the write to be made, got {other:?}"),
    }
}

/// The sentence a write refused with. A store failure fails the test as
/// surely as a write that was made: those two are what O70 separated.
fn refusal_of<T: std::fmt::Debug>(answered: Result<Offer<T>, String>) -> String {
    match answered {
        Ok(Offer::Refused(why)) => why,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// A library holding one series per title, **derived from a real scanned
/// tree** rather than from hand-inserted rows — one directory per title,
/// holding one volume.
///
/// `accept` derives its rule's pattern from the paths of the entries that
/// resolved to a series, and verifies the rule matches something, so a series
/// with no `scan_entry` rows behind it has nothing to accept, and refusing is
/// the correct answer. A fixture that cannot reach `accept`'s success path
/// cannot test it.
///
/// Every series raised from this helper carries exactly one real volume, so
/// the equal-volume-count signal is genuinely evaluated rather than vacuously
/// true. It does not by itself discriminate between two series drawn from
/// here, since every one of them holds the same count: that is
/// `the_volume_count_signal_needs_volumes_to_count`'s job, via `score_for`.
///
/// Returns the `Library` rather than a bare `Connection`: `Library` owns the
/// connection behind a `Mutex` and there is no `into_connection`, deliberately
/// — widening this crate's public surface so tests can hold a connection
/// directly is the wrong trade. Each test takes `&library.conn()` itself,
/// which is what every other test module here already does.
///
/// The `TempDir` this scans is dropped before returning rather than
/// threaded out to the caller: `derive` has already persisted every fact
/// `proposals::raise`, `open`, `accept`, `unaccept` and `dismiss` ever read
/// (`scan_entry`, `series`, `work`) into the database, and none of them
/// reopens a file — the same property `derive/tests.rs`'s own
/// `editing_a_rule_re_derives_the_library_without_touching_the_filesystem`
/// pins by deleting the tree outright between two derives.
fn library_with_series(titles: &[&str]) -> Library {
    let dir = crate::tests::tempdir("proposals-library-with-series");
    // **Belt-and-braces, not load-bearing.** `write_pages` bakes its own
    // directory into every page's bytes, so two volumes cannot collide on
    // `content_key` whatever page count either one gets, and this loop would be
    // safe as a single fixed number. A mix of counts is closer to a real
    // library than every volume being the same length.
    for (n, title) in (2_usize..).zip(titles) {
        write_pages(&dir.path().join(title).join(format!("{title} - v01")), n);
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, dir.path()).expect("add root");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("derive");
    drop(conn);
    library
}

fn library_with_two_suffixed_series() -> Library {
    library_with_series(&[
        "Go-Toubun no Hanayome",
        "Go-Toubun no Hanayome - Full Color Edition",
    ])
}

/// A library holding each `(series title, volume count)` pair with that many
/// `work` rows actually attached, raised, and the score of the one proposal
/// that comes back.
///
/// `content_key` and `slug` only need to be unique within one call's
/// database, so they are derived from the title and volume rather than
/// carrying any meaning `import.rs`'s own rows would.
fn score_for(series: &[(&str, i64)]) -> f64 {
    let library = Library::open_in_memory().expect("open");
    {
        let conn = library.conn();
        for (title, volumes) in series {
            conn.execute("INSERT INTO series (sort_title) VALUES (?1)", [title])
                .expect("series");
            let series_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO series_title (series_id, title, is_primary) VALUES (?1, ?2, 1)",
                rusqlite::params![series_id, title],
            )
            .expect("title");
            for volume in 1..=*volumes {
                conn.execute(
                    "INSERT INTO work (content_key, slug, series_id, volume, title, reading, hub) \
                     VALUES (?1, ?2, ?3, ?4, ?5, 'rtl', 'raw')",
                    rusqlite::params![
                        format!("score_for-key-{title}-{volume}"),
                        format!("score_for-slug-{title}-{volume}"),
                        series_id,
                        f64::from(u32::try_from(volume).expect("a small test volume number")),
                        format!("{title} v{volume}"),
                    ],
                )
                .expect("work");
            }
        }
    }
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let open = proposals::open(&conn, 0.0).expect("open");
    assert_eq!(open.len(), 1, "{open:?}");
    open[0].score
}

/// A real tree: the subject's own volume sits under a directory named
/// nothing like its title, `Publisher/`, resolved to its actual series purely
/// by seeded rule 4's filename capture (rule 1's path-based guess,
/// `series = "Publisher"`, is overridden per file) — exactly as it would be
/// under a real, published layout.
///
/// **The base series lives under its own directory, deliberately, not also
/// under `Publisher/`.** `accept` refuses when a directory holds more than
/// the subject, so with both under one directory that layout could never be
/// accepted at all.
/// `library_with_an_unrelated_series_sharing_the_subjects_directory` is the
/// fixture that puts two series under one directory on purpose.
///
/// **`library_with_two_suffixed_series` cannot stand in for this**, even
/// though it too scans a real tree. Its layout is flat — one series, one
/// top-level directory named after it — so the subject's `sort_title` and
/// its entries' common directory happen to coincide. Only a directory named
/// unlike the subject separates the two, which is the entire reason this
/// helper exists rather than being folded into that one.
///
/// Returns the `TempDir` too: it must outlive every use of `library`, or the
/// tree it holds is removed while `scan_entry` still names paths under it.
fn library_scanned_with_the_subject_under_an_unrelated_directory()
-> (crate::tests::TempDir, Library) {
    let dir = crate::tests::tempdir("proposals-accept-unrelated-dir");
    write_pages(
        &dir.path()
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01"),
        2,
    );
    write_pages(
        &dir.path()
            .join("Publisher")
            .join("Go-Toubun no Hanayome - Full Color Edition - v01"),
        3,
    );
    let library = Library::open_in_memory().expect("open");
    {
        let conn = library.conn();
        let root = scan::add_root(&conn, dir.path()).expect("add root");
        scan::scan_root(&conn, &root).expect("scan");
        derive::derive(&conn).expect("derive");
    }
    (dir, library)
}

/// A real tree: one publisher directory holding the two related series an
/// `edition-of` accept is about (the base and its colour sibling) and one
/// entirely unrelated series, "Chainsaw Man", with two volumes of its own.
fn library_with_an_unrelated_series_sharing_the_subjects_directory()
-> (crate::tests::TempDir, Library) {
    let dir = crate::tests::tempdir("proposals-accept-swallow");
    let publisher = dir.path().join("Publisher");
    write_pages(&publisher.join("Go-Toubun no Hanayome - v01"), 2);
    write_pages(
        &publisher.join("Go-Toubun no Hanayome - Full Color Edition - v01"),
        3,
    );
    write_pages(&publisher.join("Chainsaw Man - v01"), 4);
    write_pages(&publisher.join("Chainsaw Man - v02"), 5);
    let library = Library::open_in_memory().expect("open");
    {
        let conn = library.conn();
        let root = scan::add_root(&conn, dir.path()).expect("add root");
        scan::scan_root(&conn, &root).expect("scan");
        derive::derive(&conn).expect("derive");
    }
    (dir, library)
}

/// Two roots that happen to share a directory *name*, `Publisher/`, holding
/// completely unrelated material. Root A holds the same `edition-of` pair
/// `library_scanned_with_the_subject_under_an_unrelated_directory` does; root
/// B holds a series with no relation to either, under a directory that merely
/// spells the same as root A's.
///
/// Returns both `TempDir`s: each must outlive every use of `library`.
fn library_with_two_roots_sharing_a_directory_name()
-> (crate::tests::TempDir, crate::tests::TempDir, Library) {
    let a = crate::tests::tempdir("proposals-accept-two-roots-a");
    write_pages(
        &a.path()
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01"),
        2,
    );
    write_pages(
        &a.path()
            .join("Publisher")
            .join("Go-Toubun no Hanayome - Full Color Edition - v01"),
        3,
    );
    let b = crate::tests::tempdir("proposals-accept-two-roots-b");
    write_pages(&b.path().join("Publisher").join("Something Else - v01"), 4);

    let library = Library::open_in_memory().expect("open");
    {
        let conn = library.conn();
        let root_a = scan::add_root(&conn, a.path()).expect("add root a");
        scan::scan_root(&conn, &root_a).expect("scan a");
        let root_b = scan::add_root(&conn, b.path()).expect("add root b");
        scan::scan_root(&conn, &root_b).expect("scan b");
        derive::derive(&conn).expect("derive");
    }
    (a, b, library)
}

#[test]
fn a_dismissed_proposal_stays_dismissed_across_a_rescan() {
    // §6's first argument. Without this, every rescan re-raises the same
    // suffixed directories forever and the person stops reading them.
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("first");
    let first = proposals::open(&conn, 0.0).expect("open");
    assert_eq!(first.len(), 1);
    made(proposals::dismiss(&conn, first[0].id));

    let newly_open = proposals::raise(&conn).expect("second");

    assert_eq!(newly_open, 0);
    assert!(proposals::open(&conn, 0.0).expect("open").is_empty());
}

#[test]
fn accepting_writes_a_rule_and_un_accepting_removes_it() {
    // §6.1. Accepting must not write structure directly, or derivation would
    // have two inputs and §2's re-derive claim would need a merge rule to
    // protect manual accepts from it.
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);

    let rule_id = proposals::accept(&conn, proposal.id).expect("accept");

    let (kind, pattern): (String, String) = conn
        .query_row(
            "SELECT kind, pattern FROM rule WHERE id = ?1",
            [rule_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("rule");
    assert_eq!(kind, "prefix", "narrow: this exact directory");
    assert!(pattern.contains("Full Color"), "{pattern}");

    made(proposals::unaccept(&conn, proposal.id));

    let rules: i64 = conn
        .query_row("SELECT count(*) FROM rule WHERE id = ?1", [rule_id], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(rules, 0, "the rule went with the acceptance");
    assert_eq!(proposals::open(&conn, 0.0).expect("open").len(), 1);
}

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
    // `Full Color Edition` is the `official` half of `implied_edition_kind`'s
    // two-way split. Most edition words land here, so a wrong constant would
    // stamp every accepted publisher edition in the library as a fan edition.
    assert_eq!(previewed.set_kind.as_deref(), Some("official"));

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
    let library = library_with_series(&["Sousou no Frieren", "Sousou no Frieren - Fan Translated"]);
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
    made(proposals::dismiss(&conn, proposal.id));

    let refused = proposals::preview(&conn, proposal.id);

    assert!(
        refused.is_err_and(|e| e.contains("not open")),
        "a dismissed proposal has no rule to preview",
    );
}

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

    let rule_id = made(proposals::accept_with(&conn, proposal.id, &rule));

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
    // The `Publisher/` shape: a pattern that merely matches something is not
    // enough, and handing the rule in from outside must not become a way around
    // the check.
    let library = library_with_a_shared_publisher_directory();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    let mut rule = proposals::preview(&conn, proposal.id).expect("preview");
    rule.pattern = "Publisher/".to_string();

    let refused = refusal_of(proposals::accept_with(&conn, proposal.id, &rule));

    assert!(
        refused.contains("would move that material too"),
        "a widened pattern that reaches an unrelated series is still refused",
    );
    let rules: i64 = conn
        .query_row(
            "SELECT count(*) FROM rule WHERE note LIKE 'accepted%'",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(rules, 0, "a refused accept leaves no rule behind");
}

#[test]
fn accept_with_refuses_a_proposal_preview_would_never_have_offered() {
    // `accept_with` is a `pub fn`, and C3's
    // `POST /api/library/proposals/{id}/accept` hands it a rule straight out
    // of a request body — so "`preview` already refused on this" is an
    // assumption it is not entitled to make.
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    // Previewed while still open, so the rule handed in below is a perfectly
    // good one: the only thing wrong is the proposal's state.
    let rule = proposals::preview(&conn, proposal.id).expect("preview");
    made(proposals::dismiss(&conn, proposal.id));

    let refused = refusal_of(proposals::accept_with(&conn, proposal.id, &rule));

    assert!(
        refused.contains("not open"),
        "the state guard is `accept_with`'s too, not only `preview`'s",
    );
    let state: String = conn
        .query_row(
            "SELECT state FROM proposal WHERE id = ?1",
            [proposal.id],
            |r| r.get(0),
        )
        .expect("state");
    assert_eq!(state, "dismissed", "a refused accept_with decides nothing");
    let rules: i64 = conn
        .query_row("SELECT count(*) FROM rule WHERE ordinal > 4", [], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(rules, 0, "and writes no rule");
}

#[test]
fn accept_with_refuses_an_attach_chapters_proposal_handed_a_rule_directly() {
    // Handed a rule it did not derive, `accept_with` must not accept an
    // `attach-chapters` proposal and write a `prefix` rule that sets no volume
    // — the outcome the module doc calls worse than refusing, since `derive`
    // needs a volume to make a work and the person sees nothing change.
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add root");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("derive");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    assert_eq!(proposal.kind, "attach-chapters");

    // A rule every *other* guard passes: the real directory holding the real
    // chapter-organised material, scoped to the real root, naming the
    // subject's own series so nothing reads as collateral. Only the kind
    // refusal stands between it and the `rule` table, which is what makes the
    // assertion below about that refusal and not some other one.
    let subject_title: String = conn
        .query_row(
            "SELECT sort_title FROM series WHERE id = ?1",
            [proposal.subject_id],
            |r| r.get(0),
        )
        .expect("the subject's title");
    let rule = proposals::ProposedRule {
        root_id: Some(root.id),
        kind: "prefix".to_string(),
        pattern: "Kaguya-sama - Fan Translated/".to_string(),
        match_on: "path".to_string(),
        set_series: Some(subject_title),
        set_edition_key: Some("fan-translated".to_string()),
        set_kind: Some("fan".to_string()),
        set_language: None,
        note: "hand-written, as an admin's request body would be".to_string(),
    };

    let refused = refusal_of(proposals::accept_with(&conn, proposal.id, &rule));

    assert!(
        refused.contains("taiyaku align raw-v01.cbz"),
        "the refusal names the command that can answer the question",
    );
    let rules: i64 = conn
        .query_row("SELECT count(*) FROM rule WHERE ordinal > 4", [], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(rules, 0, "no rule that sets no volume was written");
    assert_eq!(proposals::open(&conn, 0.0).expect("open")[0].state, "open");
}

#[test]
fn accept_with_names_a_regex_rules_collateral_rather_than_refusing_to_look() {
    // `^Publisher/` reaches "Kaguya-sama wa Kokurasetai" as surely as the
    // literal `Publisher/` does, and `resolves` evaluates it through the engine
    // that will actually apply it — so the accept is refused for the reach
    // itself, and names what it reached.
    let library = library_with_a_shared_publisher_directory();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    let mut rule = proposals::preview(&conn, proposal.id).expect("preview");
    rule.kind = "regex".to_string();
    rule.pattern = "^Publisher/".to_string();

    let refused = refusal_of(proposals::accept_with(&conn, proposal.id, &rule));

    assert!(
        refused.contains("Kaguya-sama wa Kokurasetai"),
        "a regex rule's collateral is named, not waved through and not shrugged at",
    );
    let rules: i64 = conn
        .query_row(
            "SELECT count(*) FROM rule WHERE note LIKE 'accepted%'",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(rules, 0, "a refused accept leaves no rule behind");
}

#[test]
fn accept_with_refuses_a_parent_matched_rule_that_decides_nothing() {
    // `preview`'s own narrow pattern is already proven to reach only the
    // subject's material and to match something, but `rules.rs` evaluates this
    // exact rule against the immediate parent *directory name*: a different
    // string entirely.
    //
    // Evaluated that way this rule matches no entry at all: no directory here
    // is *named* `Publisher/Go-Toubun no Hanayome - Full Color Edition/`. So it
    // is refused by the check for a rule deciding nothing.
    let library = library_with_a_shared_publisher_directory();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    let mut rule = proposals::preview(&conn, proposal.id).expect("preview");
    rule.match_on = "parent".to_string();

    let refused = refusal_of(proposals::accept_with(&conn, proposal.id, &rule));

    assert!(
        refused.contains("on parent) matched no scan_entry"),
        "the rule is measured against the string the engine gives `parent`, not the path",
    );
    let rules: i64 = conn
        .query_row(
            "SELECT count(*) FROM rule WHERE note LIKE 'accepted%'",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(rules, 0, "a refused accept leaves no rule behind");
}

/// A root laid out `Publisher/<series>/<series> - v01/`, holding the two
/// related series and one unrelated third.
fn library_with_a_shared_publisher_directory() -> Library {
    let dir = crate::tests::tempdir("proposals-shared-publisher");
    for (n, title) in (2_usize..).zip([
        "Go-Toubun no Hanayome",
        "Go-Toubun no Hanayome - Full Color Edition",
        "Kaguya-sama wa Kokurasetai",
    ]) {
        write_pages(
            &dir.path()
                .join("Publisher")
                .join(title)
                .join(format!("{title} - v01")),
            n,
        );
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, dir.path()).expect("add root");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("derive");
    drop(conn);
    library
}

#[test]
fn the_vocabulary_ranks_and_never_decides() {
    // §6: `Shingeki no Kyojin - Kui Naki Sentaku (Complete Color Edition)`
    // matches "Color Edition" and is the colour edition of a SPIN-OFF. Even
    // the best signal available gets that one wrong, so a high score is still
    // a proposal.
    let library = library_with_series(&[
        "Shingeki no Kyojin",
        "Shingeki no Kyojin - Kui Naki Sentaku (Complete Color Edition)",
    ]);
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let raised = proposals::open(&conn, 0.0).expect("open");
    assert_eq!(raised.len(), 1);
    assert_eq!(raised[0].state, "open", "proposed, never applied");
    let structure: i64 = conn
        .query_row("SELECT count(*) FROM rule WHERE ordinal > 4", [], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(structure, 0, "raising writes no rule");
}

#[test]
fn the_measured_false_positives_score_below_the_true_ones() {
    // §1's four named false positives — unrelated proper nouns whose only
    // shared property is scoring low. Every series `library_with_series`
    // raises carries exactly one real volume, so the equal-volume-count
    // signal is genuinely evaluated here — both pairs agree on it (1 == 1) —
    // rather than the `0 == 0` a bare `series` row would give it, so it does
    // not by itself separate the true case from the false one; that is
    // `the_volume_count_signal_needs_volumes_to_count`'s job, via
    // `score_for`. What separates them here is the vocabulary hit. This is
    // what the score threshold has to separate; it is not a claim that the
    // threshold is right, which one library cannot establish.
    let library = library_with_series(&[
        "Kobayashi-san Chi no Maid Dragon",
        "Kobayashi-san Chi no Maid Dragon - Kanna no Nichijo",
        "Go-Toubun no Hanayome",
        "Go-Toubun no Hanayome - Full Color Edition",
    ]);
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let raised = proposals::open(&conn, 0.0).expect("open");
    let scored: std::collections::HashMap<String, f64> =
        raised.iter().map(|p| (p.reason.clone(), p.score)).collect();
    let colour = scored
        .iter()
        .find(|(r, _)| r.contains("Full Color"))
        .expect("colour")
        .1;
    let spin_off = scored
        .iter()
        .find(|(r, _)| r.contains("Kanna"))
        .expect("spin-off")
        .1;
    assert!(colour > spin_off, "{colour} should outrank {spin_off}");
}

#[test]
fn the_volume_count_signal_needs_volumes_to_count() {
    // §6 scores on a vocabulary hit AND an equal volume count. Three cases,
    // and the middle one is the whole point: two series that agree because
    // they each hold three volumes are evidence, and two series that "agree"
    // because neither holds any are not.
    let equal = score_for(&[("Series", 3), ("Series - Full Color Edition", 3)]);
    let unequal = score_for(&[("Series", 3), ("Series - Full Color Edition", 1)]);
    let neither = score_for(&[("Series", 0), ("Series - Full Color Edition", 0)]);

    assert!(equal > unequal, "{equal} should beat {unequal}");
    assert_eq!(
        neither, unequal,
        "no works on either side is no evidence, not agreement — it must not \
         score as though the counts matched"
    );
}

#[test]
fn the_score_threshold_hides_the_weak_proposal_but_not_the_strong_one() {
    // The relative-order test above does not pin that `min_score` actually
    // filters anything — a threshold that compared and did nothing would
    // still pass it. This is §6.1's "one number, with the weak proposals
    // hidden but reachable" checked directly.
    let library = library_with_series(&[
        "Kobayashi-san Chi no Maid Dragon",
        "Kobayashi-san Chi no Maid Dragon - Kanna no Nichijo",
        "Go-Toubun no Hanayome",
        "Go-Toubun no Hanayome - Full Color Edition",
    ]);
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");

    // Every series here carries one real volume, so both pairs agree on the
    // volume-count signal (1 == 1) and it contributes equally to both — the
    // colour case scores 2.0 (vocabulary hit + volume match), the spin-off
    // 1.0 (volume match alone). 1.5 sits between them.
    let above = proposals::open(&conn, 1.5).expect("open");
    assert_eq!(above.len(), 1, "{above:?}");
    assert!(
        above[0].reason.contains("Full Color"),
        "{}",
        above[0].reason
    );

    let all = proposals::open(&conn, 0.0).expect("open");
    assert_eq!(
        all.len(),
        2,
        "the weak one is reachable at a lower threshold"
    );
}

#[test]
fn same_series_relates_a_known_alt_title_to_its_series() {
    // §5's second use of `series_title`: a directory whose name matches any
    // known title of a series joins that series — which is what relates
    // 「五等分の花嫁」 to `Go-Toubun no Hanayome`, since no pattern can.
    //
    // A real scanned tree, not bare `series` rows: `accept` derives its
    // pattern from `scan_entry`, so a subject with no facts behind it has
    // nothing to derive one from and correctly refuses — see
    // `library_with_series`'s own doc. Built by hand rather than through that
    // helper because this test needs a *known alt title* recorded after the
    // scan, which no title in a plain `titles: &[&str]` list can express.
    let dir = crate::tests::tempdir("proposals-same-series");
    // Different page counts — belt-and-braces, not required; see the comment on
    // the loop in `library_with_series`.
    write_pages(
        &dir.path()
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01"),
        2,
    );
    write_pages(
        &dir.path().join("五等分の花嫁").join("五等分の花嫁 - v01"),
        3,
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, dir.path()).expect("add root");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("derive");

    let established: i64 = conn
        .query_row(
            "SELECT id FROM series WHERE sort_title = 'Go-Toubun no Hanayome'",
            [],
            |r| r.get(0),
        )
        .expect("established series");
    let scanned: i64 = conn
        .query_row(
            "SELECT id FROM series WHERE sort_title = '五等分の花嫁'",
            [],
            |r| r.get(0),
        )
        .expect("scanned series");
    conn.execute(
        "INSERT INTO series_title (series_id, title) VALUES (?1, '五等分の花嫁')",
        [established],
    )
    .expect("known alt title");

    proposals::raise(&conn).expect("raise, now that the alt title is known");
    let open = proposals::open(&conn, 0.0).expect("open");
    let proposal = open
        .iter()
        .find(|p| p.kind == "same-series")
        .expect("a same-series proposal");
    assert_eq!(proposal.subject_id, scanned);
    assert_eq!(proposal.target_id, Some(established));

    let rule_id = proposals::accept(&conn, proposal.id).expect("accept");
    let (kind, pattern, set_series): (String, String, Option<String>) = conn
        .query_row(
            "SELECT kind, pattern, set_series FROM rule WHERE id = ?1",
            [rule_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("rule");
    assert_eq!(kind, "prefix");
    assert!(pattern.contains('五'), "{pattern}");
    assert_eq!(set_series.as_deref(), Some("Go-Toubun no Hanayome"));
}

#[test]
fn attach_chapters_is_raised_by_a_derive_and_the_accept_is_refused() {
    // Two things land here. First: a chapter-organised directory produces no
    // work and stays visibly unattached, which is what `proposals::raise` —
    // called from the end of `derive::derive` — turns into the inbox entry.
    // Second: accepting it cannot produce a work either (a `prefix` rule sets
    // no volume), so accept refuses this kind by name rather than silently
    // doing nothing. `scan::tests::fixture()` holds exactly one such
    // directory, "Kaguya-sama - Fan Translated".
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add root");
    scan::scan_root(&conn, &root).expect("scan");
    // No separate call to `proposals::raise`: derive is what is supposed to
    // call it, and this is the test that would fail if that wiring were
    // missing.
    let report = derive::derive(&conn).expect("derive");
    assert_eq!(report.unattached_chapter_dirs, 1);

    let open = proposals::open(&conn, 0.0).expect("open");
    assert_eq!(open.len(), 1, "{open:?}");
    let proposal = &open[0];
    assert_eq!(proposal.kind, "attach-chapters");
    assert!(proposal.target_id.is_none());
    assert!(
        proposal.reason.contains("taiyaku align"),
        "{}",
        proposal.reason
    );

    let err = proposals::accept(&conn, proposal.id).expect_err("refused");
    assert!(
        err.contains("taiyaku align raw-v01.cbz -- en-c001.cbz en-c002.cbz"),
        "{err}"
    );

    // Refusing an accept must not change the proposal's state.
    let still_open = proposals::open(&conn, 0.0).expect("open");
    assert_eq!(still_open.len(), 1);
    assert_eq!(still_open[0].state, "open");
}

#[test]
fn raise_reopens_an_accepted_proposal_whose_rule_has_gone() {
    // `created_rule_id` is `ON DELETE SET NULL`, which is what a rule deleted
    // through C4's editor leaves behind: an accepted proposal that no longer
    // decides anything. `raise` is what notices and reopens it.
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    let rule_id = proposals::accept(&conn, proposal.id).expect("accept");
    assert!(
        proposals::open(&conn, 0.0).expect("open").is_empty(),
        "accepted, not open"
    );

    conn.execute("DELETE FROM rule WHERE id = ?1", [rule_id])
        .expect("delete the rule directly, as C4's editor would");

    let reopened = proposals::raise(&conn).expect("raise again");
    assert_eq!(reopened, 1, "the orphaned accept comes back open");
    let open = proposals::open(&conn, 0.0).expect("open");
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].id, proposal.id);
    assert_eq!(open[0].state, "open");
    assert!(open[0].created_rule_id.is_none());
}

/// `reopened`'s predicate — `after` reads `open` and `before` read `accepted`
/// — has three weakenings to rule out, and a fixture routed through a real
/// [`crate::preview::preview`] cannot tell two of them from the correct
/// behaviour: [`crate::proposals::raise`], the one place a derivation ever
/// writes to `proposal`, only ever inserts a fresh `open` row or flips an
/// orphaned `accepted` one back to `open` — never `dismissed → open` (id `3`
/// below) or `open → dismissed` (id `4`), neither of which any real derive
/// can produce no matter what rule list it is handed. A second accepted
/// proposal surviving into `after` (id `2`) is reachable through a real
/// `preview` —
/// `crate::preview::tests::a_preview_that_drops_one_of_two_accepts_leaves_the_surviving_one_accepted`
/// pins it that way — but `3` and `4` want a hand-built pair of
/// [`proposals::States`], which is what this test is for.
///
/// Four ids, each ruling out one wrong predicate:
///
/// - `1`: `accepted` → `open`. The only genuine reopen, and the only id every
///   version below must count.
/// - `2`: `accepted` → `accepted`, unchanged. Dropping the `state ==
///   STATE_OPEN` half of the predicate counts this one too, since it only
///   ever looks at `before`.
/// - `3`: `dismissed` → `open`. Widening `was == STATE_ACCEPTED` to `was !=
///   STATE_OPEN` counts this one too, since a proposal that was never
///   accepted at all still satisfies "wasn't open before".
/// - `4`: `open` → `dismissed`. Replacing the whole predicate with "any state
///   change" counts this one too, even though it moved the wrong direction
///   entirely.
#[test]
fn reopened_counts_only_a_proposal_that_moved_from_accepted_to_open() {
    let mut before = proposals::States::new();
    before.insert(1, "accepted".to_string());
    before.insert(2, "accepted".to_string());
    before.insert(3, "dismissed".to_string());
    before.insert(4, "open".to_string());

    let mut after = proposals::States::new();
    after.insert(1, "open".to_string());
    after.insert(2, "accepted".to_string());
    after.insert(3, "open".to_string());
    after.insert(4, "dismissed".to_string());

    assert_eq!(
        proposals::reopened(&before, &after),
        1,
        "only id 1 — accepted before, open after — is a reopen; 2 stayed accepted, 3 was never \
         accepted, and 4 moved the other way"
    );
}

#[test]
fn accept_derives_the_pattern_from_scan_entry_not_the_subject_title() {
    // Seeded rule 1 reads a series from the path, but a higher-ordinal rule
    // can decide it first. Seeded rule 4 exists precisely because a filename
    // sometimes repeats the series where the directory does not, and the colour
    // volume here sits under `Publisher/` with its series decided by rule 4
    // alone. A pattern derived from the subject's own `sort_title`
    // (`Go-Toubun no Hanayome - Full Color Edition/`) would match nothing,
    // since the real path begins `Publisher/`.
    let (_tree, library) = library_scanned_with_the_subject_under_an_unrelated_directory();
    let conn = library.conn();
    let proposal = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "edition-of")
        .expect("an edition-of proposal");

    let rule_id = proposals::accept(&conn, proposal.id).expect("accept");
    let pattern: String = conn
        .query_row("SELECT pattern FROM rule WHERE id = ?1", [rule_id], |r| {
            r.get(0)
        })
        .expect("rule");
    assert_eq!(pattern, "Publisher/", "{pattern}");

    let matched: i64 = conn
        .query_row(
            "SELECT count(*) FROM scan_entry WHERE substr(rel_path, 1, length(?1)) = ?1",
            [&pattern],
            |r| r.get(0),
        )
        .expect("count");
    assert!(matched > 0, "the written rule must match something real");
}

#[test]
fn accept_refuses_when_the_directory_also_holds_another_series() {
    // A pattern that merely matches something is not enough: it has to match
    // ONLY the subject. On this layout the colour proposal's derived pattern is
    // `Publisher/`, which also matches Chainsaw Man's two volumes — accepting
    // it would silently move them into the accepted series' page sequence, and
    // `write_work`'s own doc records that member order is page order and every
    // alignment offset is computed from it.
    let (_tree, library) = library_with_an_unrelated_series_sharing_the_subjects_directory();
    let conn = library.conn();
    let proposal = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "edition-of")
        .expect("an edition-of proposal");

    let err = proposals::accept(&conn, proposal.id).expect_err("refused");
    assert!(err.contains("Chainsaw Man"), "{err}");

    let rules: i64 = conn
        .query_row("SELECT count(*) FROM rule WHERE ordinal > 4", [], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(rules, 0, "a refused accept writes no rule at all");

    // The library itself is untouched: Chainsaw Man is still its own
    // two-work series, not folded into Go-Toubun's.
    let chainsaw_works: i64 = conn
        .query_row(
            "SELECT count(*) FROM work w JOIN series s ON w.series_id = s.id \
             WHERE s.sort_title = 'Chainsaw Man'",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(chainsaw_works, 2);
}

#[test]
fn accept_sets_the_edition_key_so_the_colour_volume_becomes_a_second_edition() {
    // `set_edition_key` is what keeps the accepted directory's volume a second
    // edition of one work, rather than extra pages of the base edition.
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    let rule_id = proposals::accept(&conn, proposal.id).expect("accept");

    let set_edition_key: Option<String> = conn
        .query_row(
            "SELECT set_edition_key FROM rule WHERE id = ?1",
            [rule_id],
            |r| r.get(0),
        )
        .expect("rule");
    assert_eq!(
        set_edition_key.as_deref(),
        Some("full-color-edition"),
        "the same slug the proposal's own reason quotes"
    );

    derive::derive(&conn).expect("re-derive with the accepted rule in force");

    let editions: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT e.key FROM edition e \
                 JOIN work w ON e.work_id = w.id \
                 JOIN series s ON w.series_id = s.id \
                 WHERE s.sort_title = 'Go-Toubun no Hanayome' AND w.volume = 1.0 \
                 ORDER BY e.key",
            )
            .expect("prepare");
        stmt.query_map([], |r| r.get(0))
            .expect("query")
            .collect::<Result<_, _>>()
            .expect("collect")
    };
    assert_eq!(
        editions,
        vec!["full-color-edition".to_string(), "raw".to_string()],
        "two editions of one work, not one edition of two volumes"
    );

    for key in &editions {
        let members: i64 = conn
            .query_row(
                "SELECT count(*) FROM member m \
                 JOIN edition e ON m.edition_id = e.id \
                 JOIN work w ON e.work_id = w.id \
                 JOIN series s ON w.series_id = s.id \
                 WHERE s.sort_title = 'Go-Toubun no Hanayome' AND w.volume = 1.0 \
                   AND e.key = ?1",
                [key],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(members, 1, "each edition holds its own one member, not two");
    }
}

#[test]
fn accept_refuses_an_edition_of_proposal_whose_suffix_has_no_edition_key_to_give() {
    // A suffix with no letters or digits slugifies to `""`, and `edition.key`
    // is `TEXT NOT NULL` — SQLite takes an empty string happily, which is worse
    // than refusing.
    let library = library_with_series(&["Series", "Series - ???"]);
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "edition-of")
        .expect("an edition-of proposal");

    let err = proposals::accept(&conn, proposal.id).expect_err("refused");
    assert!(err.contains("no letters or digits"), "{err}");

    let rules: i64 = conn
        .query_row("SELECT count(*) FROM rule WHERE ordinal > 4", [], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(rules, 0, "a refused accept writes no rule at all");
}

#[test]
fn accept_refuses_an_edition_of_proposal_whose_target_no_longer_matches_its_prefix() {
    // A target renamed between raise and accept must be refused rather than
    // accepted with `set_series` alone and no `set_edition_key`. Simulated
    // directly on the `series` row, since a rename between two calls is
    // otherwise invisible from outside this test.
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);

    conn.execute(
        "UPDATE series SET sort_title = 'Something Else Entirely' \
         WHERE sort_title = 'Go-Toubun no Hanayome'",
        [],
    )
    .expect("rename the target");

    let err = proposals::accept(&conn, proposal.id).expect_err("refused");
    assert!(err.contains("renamed"), "{err}");

    let rules: i64 = conn
        .query_row("SELECT count(*) FROM rule WHERE ordinal > 4", [], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(rules, 0, "a refused accept writes no rule at all");
}

#[test]
fn accept_refuses_when_the_subjects_files_sit_directly_in_the_root() {
    // The post-write verification branch — a pattern derived correctly but
    // matching nothing once checked against the raw facts — is reachable when
    // the subject's own entries sit directly under the root: their common
    // "directory" is the empty string, so the derived pattern is `/`, and no
    // real `rel_path` ever begins with one.
    let dir = crate::tests::tempdir("proposals-accept-root-level");
    write_pages(
        &dir.path()
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01"),
        2,
    );
    write_pages(
        &dir.path()
            .join("Go-Toubun no Hanayome - Full Color Edition - v01"),
        3,
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, dir.path()).expect("add root");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("derive");

    let proposal = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "edition-of")
        .expect("an edition-of proposal");

    let err = proposals::accept(&conn, proposal.id).expect_err("refused");
    assert!(err.contains("sit directly in the root"), "{err}");

    let rules: i64 = conn
        .query_row("SELECT count(*) FROM rule WHERE ordinal > 4", [], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(rules, 0, "a refused accept writes no rule at all");
}

#[test]
fn accept_refuses_when_the_subjects_entries_span_more_than_one_directory() {
    // When a single common directory does not exist, `accept` must refuse by
    // name rather than pick one arbitrarily and risk writing a rule that
    // matches nothing for the paths it does not cover.
    let dir = crate::tests::tempdir("proposals-accept-split");
    // Different page counts — belt-and-braces, not required: `write_pages`
    // cannot collide; see the comment on the loop in `library_with_series`. Not
    // load-bearing for this test's own assertions either, which read
    // `scan_entry` rather than `work`.
    write_pages(
        &dir.path()
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01"),
        2,
    );
    write_pages(
        &dir.path()
            .join("PublisherA")
            .join("Go-Toubun no Hanayome - Full Color Edition - v01"),
        3,
    );
    write_pages(
        &dir.path()
            .join("PublisherB")
            .join("Go-Toubun no Hanayome - Full Color Edition - v02"),
        4,
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, dir.path()).expect("add root");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("derive");

    let proposal = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "edition-of")
        .expect("an edition-of proposal");

    let err = proposals::accept(&conn, proposal.id).expect_err("refused");
    assert!(err.contains("spans more than one directory"), "{err}");
    assert!(err.contains("PublisherA"), "{err}");
    assert!(err.contains("PublisherB"), "{err}");

    let rules: i64 = conn
        .query_row("SELECT count(*) FROM rule WHERE ordinal > 4", [], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(rules, 0, "a refused accept writes no rule at all");
}

#[test]
fn double_accept_is_refused_rather_than_orphaning_the_first_rule() {
    // Without a state guard, a second `accept` writes a second rule and
    // overwrites `created_rule_id` with its id, so `unaccept` deletes only the
    // second and the first stays in force, deciding structure invisible to the
    // proposal that made it.
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    let first_rule = proposals::accept(&conn, proposal.id).expect("first accept");

    let err = proposals::accept(&conn, proposal.id).expect_err("refused");
    assert!(err.contains("not open"), "{err}");

    let rules: i64 = conn
        .query_row("SELECT count(*) FROM rule WHERE ordinal > 4", [], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(
        rules, 1,
        "a refused second accept must not add a second rule"
    );

    made(proposals::unaccept(&conn, proposal.id));
    let remaining: i64 = conn
        .query_row(
            "SELECT count(*) FROM rule WHERE id = ?1",
            [first_rule],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(remaining, 0, "unaccept removed the only rule ever written");
}

#[test]
fn dismiss_refuses_an_accepted_proposal_rather_than_orphaning_its_rule() {
    // Dismissing an accepted proposal must not silently leave its rule alive
    // under a `dismissed` label, invisible to `open` and never seen by
    // `reopen_orphaned_accepts`, which only looks at `state = 'accepted'`.
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    let rule_id = proposals::accept(&conn, proposal.id).expect("accept");

    let err = refusal_of(proposals::dismiss(&conn, proposal.id));
    assert!(err.contains("un-accept"), "{err}");

    let rules: i64 = conn
        .query_row("SELECT count(*) FROM rule WHERE id = ?1", [rule_id], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(rules, 1, "the rule survives a refused dismiss");
    // **And the row is untouched, which the rule count alone cannot see**:
    // `dismiss` never deletes a rule, so a version that wrote `dismissed` over
    // the accepted row and then refused anyway would keep the rule and pass the
    // count. That row is the orphan this test is named for.
    assert_eq!(state_of(&conn, proposal.id), "accepted");
    assert_eq!(rule_of(&conn, proposal.id), Some(rule_id));
}

/// `proposal.created_rule_id` for `id`.
fn rule_of(conn: &rusqlite::Connection, id: i64) -> Option<i64> {
    conn.query_row(
        "SELECT created_rule_id FROM proposal WHERE id = ?1",
        [id],
        |r| r.get(0),
    )
    .expect("the proposal's rule")
}

/// **O72.** A row holding a rule an accept wrote, but not `accepted`, is a
/// state `dismiss` must not write over. It is built by hand; this one reads
/// `open`, the other shape `dismiss` could be handed.
#[test]
fn dismiss_refuses_a_proposal_still_holding_a_rule_though_it_is_not_accepted() {
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    let rule_id = proposals::accept(&conn, proposal.id).expect("accept");
    conn.execute(
        "UPDATE proposal SET state = 'open' WHERE id = ?1",
        [proposal.id],
    )
    .expect("a row holding a rule and not accepted");

    let why = refusal_of(proposals::dismiss(&conn, proposal.id));
    assert_eq!(
        why,
        format!(
            "proposal {} is `open` but still holds rule {rule_id}, which an accept wrote — \
             un-accept it first, then dismiss",
            proposal.id
        )
    );
    assert_eq!(state_of(&conn, proposal.id), "open");
    assert_eq!(rule_of(&conn, proposal.id), Some(rule_id));
}

/// **O72.** A `dismissed` row still holding its accept's rule. Reopened, it
/// would be `open` with that rule in force, and `acceptable` would let it be
/// accepted again beside it. Refused — and the sentence's advice is then taken,
/// and shown to leave an open proposal with no rule anywhere.
#[test]
fn reopen_refuses_a_dismissal_still_holding_a_rule_and_un_accepting_repairs_it() {
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    let rule_id = proposals::accept(&conn, proposal.id).expect("accept");
    conn.execute(
        "UPDATE proposal SET state = 'dismissed' WHERE id = ?1",
        [proposal.id],
    )
    .expect("the row the race left");

    let why = refusal_of(proposals::reopen(&conn, proposal.id));
    assert_eq!(
        why,
        format!(
            "proposal {} is `dismissed` but still holds rule {rule_id}, which an accept wrote — \
             un-accepting it removes that rule and reopens it",
            proposal.id
        )
    );
    assert_eq!(state_of(&conn, proposal.id), "dismissed");
    assert_eq!(rule_of(&conn, proposal.id), Some(rule_id));

    made(proposals::unaccept(&conn, proposal.id));
    assert_eq!(state_of(&conn, proposal.id), "open");
    assert_eq!(rule_of(&conn, proposal.id), None);
    let rules: i64 = conn
        .query_row("SELECT count(*) FROM rule WHERE id = ?1", [rule_id], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(rules, 0, "and the rule is gone");
}

/// **O72.** An accepted proposal whose rule has gone — deleted by hand, or by
/// another process before the next `raise` reopens it — is refused by both
/// writes, and neither sentence sends a person to un-accept a proposal
/// `unaccept` would refuse. `dismiss`'s conditional `UPDATE` has to carry the
/// state as well as the rule to keep refusing it.
#[test]
fn an_accepted_proposal_whose_rule_has_gone_is_refused_by_dismiss_and_reopen() {
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    let rule_id = proposals::accept(&conn, proposal.id).expect("accept");
    conn.execute("DELETE FROM rule WHERE id = ?1", [rule_id])
        .expect("the rule goes");
    assert_eq!(rule_of(&conn, proposal.id), None, "ON DELETE SET NULL");
    assert_eq!(state_of(&conn, proposal.id), "accepted");

    let why = refusal_of(proposals::dismiss(&conn, proposal.id));
    assert_eq!(
        why,
        format!(
            "proposal {} is accepted, but the rule it wrote has gone — the next scan reopens \
             it, and it can be dismissed then",
            proposal.id
        )
    );
    assert_eq!(state_of(&conn, proposal.id), "accepted");

    let why = refusal_of(proposals::reopen(&conn, proposal.id));
    assert_eq!(
        why,
        format!(
            "proposal {} is accepted, but the rule it wrote has gone — the next scan reopens it",
            proposal.id
        )
    );
    assert_eq!(state_of(&conn, proposal.id), "accepted");

    proposals::raise(&conn).expect("the next scan's raise");
    assert_eq!(
        state_of(&conn, proposal.id),
        "open",
        "as both sentences say"
    );
}

#[test]
fn raise_refreshes_a_stale_open_proposals_score_and_reason() {
    // `INSERT OR IGNORE` never updates, so a proposal raised before its series
    // had any works would otherwise keep its first score and reason forever,
    // and `open`'s threshold would keep hiding it even after the volume-count
    // signal starts agreeing.
    //
    // Bare `series` rows here, not a scanned tree: this test never calls
    // `accept`, only `raise`/`open`, and it needs to start at zero volumes on
    // both sides, which `library_with_two_suffixed_series` cannot give it:
    // every series there has a real volume.
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    for title in [
        "Go-Toubun no Hanayome",
        "Go-Toubun no Hanayome - Full Color Edition",
    ] {
        conn.execute("INSERT INTO series (sort_title) VALUES (?1)", [title])
            .expect("series");
        let id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO series_title (series_id, title, is_primary) VALUES (?1, ?2, 1)",
            rusqlite::params![id, title],
        )
        .expect("title");
    }

    let first_open = proposals::raise(&conn).expect("first raise");
    assert_eq!(first_open, 1);
    let before = proposals::open(&conn, 0.0).expect("open").remove(0);
    assert!(before.reason.contains("neither side"), "{}", before.reason);
    assert!(
        (before.score - 1.0).abs() < f64::EPSILON,
        "{}",
        before.score
    );

    let go_toubun: i64 = conn
        .query_row(
            "SELECT id FROM series WHERE sort_title = 'Go-Toubun no Hanayome'",
            [],
            |r| r.get(0),
        )
        .expect("base series");
    let colour: i64 = conn
        .query_row(
            "SELECT id FROM series WHERE sort_title = 'Go-Toubun no Hanayome - Full Color Edition'",
            [],
            |r| r.get(0),
        )
        .expect("colour series");
    for series_id in [go_toubun, colour] {
        for volume in 1..=3 {
            conn.execute(
                "INSERT INTO work (content_key, slug, series_id, volume, title, reading, hub) \
                 VALUES (?1, ?2, ?3, ?4, ?5, 'rtl', 'raw')",
                rusqlite::params![
                    format!("stale-key-{series_id}-{volume}"),
                    format!("stale-slug-{series_id}-{volume}"),
                    series_id,
                    f64::from(volume),
                    format!("t{series_id}-{volume}"),
                ],
            )
            .expect("work");
        }
    }

    let second_open = proposals::raise(&conn).expect("second raise");
    assert_eq!(
        second_open, 0,
        "refreshing an already-open proposal is not opening a new one"
    );
    let after = proposals::open(&conn, 0.0).expect("open").remove(0);
    assert_eq!(after.id, before.id);
    assert!((after.score - 2.0).abs() < f64::EPSILON, "{}", after.score);
    assert!(
        after.reason.contains("both hold 3 volume(s)"),
        "{}",
        after.reason
    );
}

/// Both halves of `EditionOfSignals::vocabulary_phrase`, produced rather than
/// supplied.
///
/// **The phrase is half of every `edition-of` proposal's `reason`**: the
/// sentence a person reads in the inbox and the only words there saying *why*
/// the pair was raised.
///
/// Two pairs in one library, raised in one call, because either branch alone
/// is passed by a constant: a version returning only the vocabulary phrase
/// satisfies the first assertion, and one returning only the other satisfies
/// the second. Both assertions also check the *absence* of the opposite
/// phrase, so a version concatenating the two passes neither.
///
/// Bare `series` rows for `raise_refreshes_a_stale_open_proposals_score_and_reason`'s
/// reason — this never calls `accept`, and `library_with_series` would tie the
/// titles to real directories the vocabulary has no bearing on.
#[test]
fn a_suffix_in_the_edition_vocabulary_and_one_outside_it_are_worded_differently() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    // "Full Color Edition" contains `Color`, which is in `EDITION_WORDS`;
    // "Gaiden" is in none of them and is a real thing a suffixed series is
    // called, so this is not a nonsense string standing in for one.
    for title in [
        "Go-Toubun no Hanayome",
        "Go-Toubun no Hanayome - Full Color Edition",
        "Kaguya-sama wa Kokurasetai",
        "Kaguya-sama wa Kokurasetai - Gaiden",
    ] {
        conn.execute("INSERT INTO series (sort_title) VALUES (?1)", [title])
            .expect("series");
        let id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO series_title (series_id, title, is_primary) VALUES (?1, ?2, 1)",
            rusqlite::params![id, title],
        )
        .expect("title");
    }

    let raised = proposals::raise(&conn).expect("raise");
    assert_eq!(raised, 2, "one proposal per suffixed series");

    let open = proposals::open(&conn, 0.0).expect("open");
    let reason_for = |suffixed: &str| -> String {
        open.iter()
            .find(|p| p.reason.starts_with(&format!("\"{suffixed}\"")))
            .unwrap_or_else(|| panic!("no proposal raised for {suffixed}: {open:?}"))
            .reason
            .clone()
    };

    let colour = reason_for("Go-Toubun no Hanayome - Full Color Edition");
    assert!(
        colour.contains("the suffix matches the edition vocabulary"),
        "{colour}"
    );
    assert!(!colour.contains("matches no edition word"), "{colour}");

    let gaiden = reason_for("Kaguya-sama wa Kokurasetai - Gaiden");
    assert!(
        gaiden.contains("the suffix matches no edition word"),
        "{gaiden}"
    );
    assert!(!gaiden.contains("the edition vocabulary"), "{gaiden}");
}

#[test]
fn a_failure_between_the_insert_and_the_update_leaves_no_orphaned_rule() {
    // A failure between `INSERT INTO rule` and `UPDATE proposal` must not
    // leave a live rule that no proposal points at while the proposal still
    // reads `open`.
    //
    // Simulated with a trigger that aborts precisely the `UPDATE proposal`
    // statement `accept` issues, standing in for a crash or a constraint
    // failure between the two writes. SQLite's own `RAISE(ABORT, …)` rolls
    // back only the statement it fires on, not the whole transaction by
    // itself — what this test actually measures is that `accept`'s own
    // transaction rolls back the rest, including the `INSERT` that already
    // ran, once the function returns `Err` without committing.
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);

    conn.execute_batch(
        "CREATE TRIGGER simulate_update_failure BEFORE UPDATE OF state ON proposal \
         WHEN NEW.state = 'accepted' \
         BEGIN SELECT RAISE(ABORT, 'simulated failure between insert and update'); END;",
    )
    .expect("installing the trigger");

    let err = proposals::accept(&conn, proposal.id).expect_err("the simulated failure surfaces");
    assert!(err.contains("simulated failure"), "{err}");

    let rules: i64 = conn
        .query_row("SELECT count(*) FROM rule WHERE ordinal > 4", [], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(
        rules, 0,
        "the rule the insert wrote must not survive a failure later in the same accept"
    );

    let state: String = conn
        .query_row(
            "SELECT state FROM proposal WHERE id = ?1",
            [proposal.id],
            |r| r.get(0),
        )
        .expect("state");
    assert_eq!(state, "open", "the proposal was never actually accepted");
}

#[test]
fn accept_scopes_the_rule_to_the_subjects_own_root_and_leaves_another_roots_material_alone() {
    // The rule is scoped to the subject's own root, so root B's "Something
    // Else" under the same-spelled `Publisher/` prefix is material it can never
    // reach, and the accept succeeds. The rule it writes must never touch root
    // B's own series on a later derive, which is checked directly rather than
    // only inferred from the column being non-NULL.
    let (tree_a, _tree_b, library) = library_with_two_roots_sharing_a_directory_name();
    let conn = library.conn();
    let proposal = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "edition-of")
        .expect("an edition-of proposal");

    let rule_id = proposals::accept(&conn, proposal.id).expect("accept succeeds");

    let (root_id, root_path): (Option<i64>, Option<String>) = conn
        .query_row(
            "SELECT rule.root_id, root.path FROM rule LEFT JOIN root ON root.id = rule.root_id \
             WHERE rule.id = ?1",
            [rule_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("rule");
    assert!(
        root_id.is_some(),
        "the accepted rule must be scoped to one root, not NULL (every root, forever)"
    );
    assert_eq!(
        root_path.as_deref(),
        Some(
            tree_a
                .path()
                .canonicalize()
                .expect("canonicalises")
                .to_string_lossy()
                .as_ref()
        ),
        "and it must be the subject's own root, root A, not root B"
    );

    derive::derive(&conn).expect("re-derive with the accepted rule in force");

    // Root B's own series must be untouched: still its own series, holding
    // exactly its own pages, not folded into Go-Toubun no Hanayome by a rule
    // that was never supposed to reach it.
    let (series_count, member_pages): (i64, i64) = conn
        .query_row(
            "SELECT (SELECT count(*) FROM series WHERE sort_title = 'Something Else'), \
                    (SELECT COALESCE(SUM(m.pages), 0) FROM member m \
                       JOIN edition e ON m.edition_id = e.id \
                       JOIN work w ON e.work_id = w.id JOIN series s ON w.series_id = s.id \
                     WHERE s.sort_title = 'Something Else')",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("counts");
    assert_eq!(
        series_count, 1,
        "Something Else must still be its own series"
    );
    assert_eq!(
        member_pages, 4,
        "holding exactly root B's own 4 pages, not merged with anything from root A"
    );
}

/// A subject whose own material sits under one *same-spelled* directory in
/// two different roots has no single root to scope the written rule to, and
/// `common_directory_pattern` must refuse rather than pick one arbitrarily.
/// Both roots hold a volume of the colour series under their own
/// `Publisher/`, so the subject's own directory component is identical text
/// (`"Publisher"`) in both — the one case the "more than one directory"
/// check above cannot catch on its own, since the two directories are not
/// spelled differently.
#[test]
fn accept_refuses_when_the_subjects_own_directory_is_spelled_the_same_in_two_roots() {
    let a = crate::tests::tempdir("proposals-accept-multiroot-a");
    write_pages(
        &a.path()
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01"),
        2,
    );
    write_pages(
        &a.path()
            .join("Publisher")
            .join("Go-Toubun no Hanayome - Full Color Edition - v01"),
        3,
    );
    let b = crate::tests::tempdir("proposals-accept-multiroot-b");
    write_pages(
        &b.path()
            .join("Publisher")
            .join("Go-Toubun no Hanayome - Full Color Edition - v02"),
        4,
    );

    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root_a =
        scan::add_root_in(&conn, a.path(), scan::InLibrary::Named("Manga")).expect("add root a");
    scan::scan_root(&conn, &root_a).expect("scan a");
    let root_b =
        scan::add_root_in(&conn, b.path(), scan::InLibrary::Named("Manga")).expect("add root b");
    scan::scan_root(&conn, &root_b).expect("scan b");
    derive::derive(&conn).expect("derive");

    let proposal = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "edition-of")
        .expect("an edition-of proposal");

    let err = proposals::accept(&conn, proposal.id).expect_err("refused");
    // The exact string, checked in full — this message is built from a
    // `\`-continued literal, and `cargo fmt` has a real quirk of baking
    // stripped indentation into a merged continuation as literal spaces.
    // An exact match is what would catch that.
    assert_eq!(
        err,
        format!(
            "proposal {}'s subject sits under `Publisher/` in more than one root, so \
             accepting it cannot be scoped to a single root. Its roots are: {}, {}",
            proposal.id,
            a.path().canonicalize().unwrap().display(),
            b.path().canonicalize().unwrap().display(),
        )
    );

    let rules: i64 = conn
        .query_row("SELECT count(*) FROM rule WHERE ordinal > 4", [], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(rules, 0, "a refused accept writes no rule at all");
}

#[test]
fn dismissing_an_unknown_proposal_is_an_error() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let err = proposals::dismiss(&conn, 999).expect_err("an id naming no proposal");
    assert!(err.contains("there is no such proposal"), "{err}");
}

#[test]
fn unaccepting_a_never_accepted_proposal_is_refused() {
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    let why = refusal_of(proposals::unaccept(&conn, proposal.id));
    assert!(why.contains("no accepted rule"), "{why}");
}

/// A trigger that fails one statement a write makes with SQLite's own error,
/// which is what a full disk or a locked database looks like from here:
/// nothing about the library's state that a caller could change.
fn a_store_fault_on(conn: &rusqlite::Connection, on: &str) {
    conn.execute_batch(&format!(
        "CREATE TRIGGER a_store_fault {on} \
         BEGIN SELECT RAISE(ABORT, 'a simulated store fault'); END;"
    ))
    .expect("installs the trigger");
}

fn state_of(conn: &rusqlite::Connection, id: i64) -> String {
    conn.query_row("SELECT state FROM proposal WHERE id = ?1", [id], |r| {
        r.get(0)
    })
    .expect("state")
}

/// **O70, at the store.** A write `accept_with` cannot perform is `Err`, and
/// an accept it declines is `Ok(Offer::Refused)`: two different outcomes.
///
/// The fault is a trigger on the `INSERT INTO rule` the previewed rule
/// reaches once every refusal has passed. The refusal is the state guard over
/// a proposal already accepted: a real refused state, reached by the store's
/// own code.
#[test]
fn accept_with_answers_a_failed_write_as_an_error_and_a_refusal_as_refused() {
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    let rule = proposals::preview(&conn, proposal.id).expect("preview");

    a_store_fault_on(&conn, "BEFORE INSERT ON rule");
    let err = proposals::accept_with(&conn, proposal.id, &rule)
        .expect_err("a write that failed is an error, not a refusal");
    assert!(err.contains("a simulated store fault"), "{err}");
    assert_eq!(accepted_rules(&conn), 0, "and nothing was written");
    assert_eq!(state_of(&conn, proposal.id), "open");

    conn.execute_batch("DROP TRIGGER a_store_fault")
        .expect("drops the trigger");
    made(proposals::accept_with(&conn, proposal.id, &rule));
    let why = refusal_of(proposals::accept_with(&conn, proposal.id, &rule));
    assert!(why.contains("not open"), "{why}");
    assert_eq!(
        accepted_rules(&conn),
        1,
        "a refused second accept writes nothing"
    );
}

/// A **read** `accept_with` cannot make is `Err` as well: the dry run's
/// sweep, which reaches the caller through `resolves` rather than through the
/// transaction.
#[test]
fn accept_with_answers_a_failed_read_as_an_error_rather_than_a_refusal() {
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    let rule = proposals::preview(&conn, proposal.id).expect("preview");
    conn.execute_batch("PRAGMA writable_schema = ON; DROP TABLE scan_entry;")
        .expect("drops the table the dry run sweeps");

    let err = proposals::accept_with(&conn, proposal.id, &rule)
        .expect_err("a read that failed is an error, not a refusal");
    assert!(err.contains("scan_entry"), "{err}");
}

/// `unaccept`: nothing to un-accept is `Refused`, and a write that fails is
/// `Err`. The fault is on the `DELETE FROM rule` that follows the reopening
/// `UPDATE` inside one transaction, so the proposal still reading `accepted`
/// is the rollback's doing too.
#[test]
fn unaccept_answers_a_failed_write_as_an_error_and_a_refusal_as_refused() {
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);

    let why = refusal_of(proposals::unaccept(&conn, proposal.id));
    assert_eq!(
        why,
        format!("proposal {} has no accepted rule to remove", proposal.id)
    );

    proposals::accept(&conn, proposal.id).expect("accept");
    a_store_fault_on(&conn, "BEFORE DELETE ON rule");
    let err = proposals::unaccept(&conn, proposal.id)
        .expect_err("a write that failed is an error, not a refusal");
    assert!(err.contains("a simulated store fault"), "{err}");
    assert_eq!(accepted_rules(&conn), 1, "the rule is still there");
    assert_eq!(state_of(&conn, proposal.id), "accepted");
}

/// `dismiss`: a write that fails is `Err`, and an accepted proposal is
/// `Refused`.
#[test]
fn dismiss_answers_a_failed_write_as_an_error_and_a_refusal_as_refused() {
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);

    a_store_fault_on(
        &conn,
        "BEFORE UPDATE OF state ON proposal WHEN NEW.state = 'dismissed'",
    );
    let err = proposals::dismiss(&conn, proposal.id)
        .expect_err("a write that failed is an error, not a refusal");
    assert!(err.contains("a simulated store fault"), "{err}");
    assert_eq!(state_of(&conn, proposal.id), "open");

    conn.execute_batch("DROP TRIGGER a_store_fault")
        .expect("drops the trigger");
    proposals::accept(&conn, proposal.id).expect("accept");
    let why = refusal_of(proposals::dismiss(&conn, proposal.id));
    assert_eq!(
        why,
        format!(
            "proposal {} is accepted — un-accept it first, then dismiss",
            proposal.id
        )
    );
    assert_eq!(state_of(&conn, proposal.id), "accepted");
}

/// **O72.** A dismissal can be taken back, and what comes back is the same
/// proposal, open, rather than a second row a rescan raised beside it.
///
/// The rescan is the half worth pinning: `raise` treats an open proposal as
/// one it may refresh, and `proposal_identity` is what keeps its
/// `INSERT OR IGNORE` from adding a duplicate — so the row count and the id
/// are asserted, not only that something is open.
#[test]
fn a_reopened_dismissal_is_the_same_proposal_open_again_across_a_rescan() {
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    made(proposals::dismiss(&conn, proposal.id));
    assert!(proposals::open(&conn, 0.0).expect("open").is_empty());

    made(proposals::reopen(&conn, proposal.id));
    assert_eq!(state_of(&conn, proposal.id), "open");

    let newly_open = proposals::raise(&conn).expect("rescan");
    assert_eq!(
        newly_open, 0,
        "reopening is not a proposal the rescan raised"
    );
    let open: Vec<i64> = proposals::open(&conn, 0.0)
        .expect("open")
        .iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(open, vec![proposal.id]);
    let rows: i64 = conn
        .query_row("SELECT count(*) FROM proposal", [], |r| r.get(0))
        .expect("count");
    assert_eq!(rows, 1, "and no second row beside it");
}

/// `reopen` refuses anything but a dismissal, **in words that say which**,
/// and writes nothing.
///
/// Both refusals are exact strings: the accepted one points somewhere else
/// (un-accepting), and a reopen that reached an accepted row would leave an
/// `open` proposal still holding a live `created_rule_id` — the orphaned
/// shape `dismiss`'s own refusal exists to prevent from the other side — so
/// the rule is counted as well as the state read.
#[test]
fn reopen_refuses_a_proposal_that_is_not_dismissed_and_writes_nothing() {
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);

    let why = refusal_of(proposals::reopen(&conn, proposal.id));
    assert_eq!(
        why,
        format!(
            "proposal {} is `open`, not dismissed — only a dismissal can be reopened",
            proposal.id
        )
    );
    assert_eq!(state_of(&conn, proposal.id), "open");

    proposals::accept(&conn, proposal.id).expect("accept");
    let why = refusal_of(proposals::reopen(&conn, proposal.id));
    assert_eq!(
        why,
        format!(
            "proposal {} is accepted, not dismissed — un-accepting it is what reopens it",
            proposal.id
        )
    );
    assert_eq!(state_of(&conn, proposal.id), "accepted");
    assert_eq!(accepted_rules(&conn), 1, "the accept's rule is untouched");
}

#[test]
fn reopening_an_unknown_proposal_is_an_error() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let err = proposals::reopen(&conn, 999).expect_err("an id naming no proposal");
    assert!(err.contains("there is no such proposal"), "{err}");
}

/// `reopen`: a write that fails is `Err`, not a refusal, and the dismissal
/// stands.
#[test]
fn reopen_answers_a_failed_write_as_an_error() {
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    made(proposals::dismiss(&conn, proposal.id));

    a_store_fault_on(
        &conn,
        "BEFORE UPDATE OF state ON proposal WHEN NEW.state = 'open'",
    );
    let err = proposals::reopen(&conn, proposal.id)
        .expect_err("a write that failed is an error, not a refusal");
    assert!(err.contains("a simulated store fault"), "{err}");
    assert_eq!(state_of(&conn, proposal.id), "dismissed");
}

/// [`proposals::open`]'s own doc says "highest score first".
/// `the_score_threshold_hides_the_weak_proposal_but_not_the_strong_one`
/// reads `.len()` and the reason string, both of which a reversed `ORDER BY`
/// leaves alone, so this test reads the order. An inbox that lists the weakest
/// guesses first is the inverse of §6.1's whole ranking argument, and it looks
/// exactly like a working one.
#[test]
fn open_lists_the_strongest_proposal_first() {
    let library = library_with_series(&[
        "Kobayashi-san Chi no Maid Dragon",
        "Kobayashi-san Chi no Maid Dragon - Kanna no Nichijo",
        "Go-Toubun no Hanayome",
        "Go-Toubun no Hanayome - Full Color Edition",
    ]);
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");

    let all = proposals::open(&conn, 0.0).expect("open");
    assert_eq!(all.len(), 2, "{all:?}");
    assert!(
        all[0].score > all[1].score,
        "highest score first: {:?}",
        all.iter().map(|p| p.score).collect::<Vec<_>>()
    );
    assert!(all[0].reason.contains("Full Color"), "{}", all[0].reason);
}

/// A library where two series have both recorded the same title, and a third
/// series' own `sort_title` is that title — so `raise_same_series` has two
/// candidate owners to pick between.
///
/// Bare rows rather than a scanned tree, for the reason
/// `raise_refreshes_a_stale_open_proposals_score_and_reason` gives: nothing
/// here calls `accept`, and two series independently claiming one title is
/// not a shape a scan of a directory tree produces.
fn library_where_two_series_claim_one_title() -> (Library, i64, i64, i64) {
    let library = Library::open_in_memory().expect("open");
    let (first, second, claimant);
    {
        let conn = library.conn();
        let mut ids = Vec::new();
        for title in ["First Claimant", "Second Claimant", "五等分の花嫁"] {
            conn.execute("INSERT INTO series (sort_title) VALUES (?1)", [title])
                .expect("series");
            let id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO series_title (series_id, title, is_primary) VALUES (?1, ?2, 1)",
                rusqlite::params![id, title],
            )
            .expect("primary title");
            ids.push(id);
        }
        (first, second, claimant) = (ids[0], ids[1], ids[2]);
        // Both claimants also record the third series' own title as one of
        // theirs — two people, or two imports, recording it independently.
        for owner in [first, second] {
            conn.execute(
                "INSERT INTO series_title (series_id, title) VALUES (?1, '五等分の花嫁')",
                [owner],
            )
            .expect("alt title");
        }
    }
    (library, first, second, claimant)
}

/// Two series claiming one title resolve to the **lowest id**, stably.
///
/// Without an explicit order the pick would be whatever a b-tree scan returns
/// first — unspecified, and liable to change across a page split that has
/// nothing to do with this data — and a proposal that points somewhere
/// different on each rescan cannot be dismissed once and stay dismissed, which
/// is §6's first argument.
#[test]
fn two_series_claiming_one_title_resolve_to_the_lowest_id() {
    let (library, first, second, claimant) = library_where_two_series_claim_one_title();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");

    let proposal = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "same-series")
        .expect("a same-series proposal");
    assert_eq!(proposal.subject_id, claimant);
    assert_eq!(
        proposal.target_id,
        Some(first),
        "the lowest id of the two claimants, not whichever the scan met last (the other is \
         {second})"
    );
}

/// `same-series` and `attach-chapters` score above the strongest possible
/// `edition-of` match, and most tests that read a proposal back ask
/// `open(&conn, 0.0)` — which is every score at once, and so no score at all.
/// Zeroing `CERTAIN` would hide both kinds below any threshold a person would
/// actually type.
#[test]
fn a_certain_proposal_is_visible_above_the_strongest_edition_of_score() {
    let (library, _first, _second, _claimant) = library_where_two_series_claim_one_title();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");

    // Two signals is the most an `edition-of` proposal can score. A
    // `same-series` match is a known title, not a suffix-stripping guess, so
    // it sits above that scale rather than competing on it.
    let above_every_edition_of = proposals::open(&conn, 2.5).expect("open");
    let proposal = above_every_edition_of
        .iter()
        .find(|p| p.kind == "same-series")
        .expect("a same-series proposal still visible above 2.0");
    assert!(
        (proposal.score - 3.0).abs() < f64::EPSILON,
        "{}",
        proposal.score
    );
}

/// §6.1's "a dismissed or accepted row is never rescored", which
/// `insert_proposal`'s `AND state = ?6` is.
/// `raise_refreshes_a_stale_open_proposals_score_and_reason` covers the open
/// case, and `a_dismissed_proposal_stays_dismissed_across_a_rescan` reads
/// `state` alone. Without the clause, every rescan rewrites the score and
/// reason of dismissed and accepted proposals, so the record of what a person
/// was told when they made the decision drifts away from the decision.
#[test]
fn a_rescan_does_not_rescore_a_proposal_someone_has_already_dismissed() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    for title in [
        "Go-Toubun no Hanayome",
        "Go-Toubun no Hanayome - Full Color Edition",
    ] {
        conn.execute("INSERT INTO series (sort_title) VALUES (?1)", [title])
            .expect("series");
        let id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO series_title (series_id, title, is_primary) VALUES (?1, ?2, 1)",
            rusqlite::params![id, title],
        )
        .expect("title");
    }

    proposals::raise(&conn).expect("first raise");
    let raised = proposals::open(&conn, 0.0).expect("open").remove(0);
    made(proposals::dismiss(&conn, raised.id));

    // The same facts change that `raise_refreshes_...` proves *does* rescore
    // an open proposal: both sides gain three volumes, so the volume signal
    // starts agreeing and the score would move from 1.0 to 2.0.
    for series_id in [raised.subject_id, raised.target_id.expect("a target")] {
        for volume in 1..=3 {
            conn.execute(
                "INSERT INTO work (content_key, slug, series_id, volume, title, reading, hub) \
                 VALUES (?1, ?2, ?3, ?4, ?5, 'rtl', 'raw')",
                rusqlite::params![
                    format!("dismissed-key-{series_id}-{volume}"),
                    format!("dismissed-slug-{series_id}-{volume}"),
                    series_id,
                    f64::from(volume),
                    format!("t{series_id}-{volume}"),
                ],
            )
            .expect("work");
        }
    }
    proposals::raise(&conn).expect("second raise");

    let (state, score, reason): (String, f64, String) = conn
        .query_row(
            "SELECT state, score, reason FROM proposal WHERE id = ?1",
            [raised.id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("the dismissed row");
    assert_eq!(state, "dismissed");
    assert!(
        (score - raised.score).abs() < f64::EPSILON,
        "a dismissed proposal keeps the score it was dismissed at: {score} vs {}",
        raised.score
    );
    assert_eq!(
        reason, raised.reason,
        "and the reason a person read when they dismissed it"
    );
}

/// A publisher directory holding **only** the two series an `edition-of`
/// accept is about — the base and its colour sibling, and nothing else.
fn library_where_the_subject_shares_its_directory_with_the_target_alone()
-> (crate::tests::TempDir, Library) {
    let dir = crate::tests::tempdir("proposals-accept-target-only");
    let publisher = dir.path().join("Publisher");
    write_pages(&publisher.join("Go-Toubun no Hanayome - v01"), 2);
    write_pages(
        &publisher.join("Go-Toubun no Hanayome - Full Color Edition - v01"),
        3,
    );
    let library = Library::open_in_memory().expect("open");
    {
        let conn = library.conn();
        let root = scan::add_root(&conn, dir.path()).expect("add root");
        scan::scan_root(&conn, &root).expect("scan");
        derive::derive(&conn).expect("derive");
    }
    (dir, library)
}

/// The refusal branch `accept_refuses_when_the_directory_also_holds_another_series`
/// cannot reach: the only other series under the derived pattern is the
/// **target itself**.
///
/// The target stays part of the check — a rule matching it would stamp
/// `set_edition_key` onto the base's own volumes and make the base its own
/// colour edition — but it is not collateral, and naming it as material the
/// accept "would move too" misdescribes what it is.
#[test]
fn accept_refuses_by_name_when_the_only_other_series_under_the_pattern_is_the_target() {
    let (_tree, library) = library_where_the_subject_shares_its_directory_with_the_target_alone();
    let conn = library.conn();
    let proposal = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "edition-of")
        .expect("an edition-of proposal");

    let err = proposals::accept(&conn, proposal.id).expect_err("refused");
    assert_eq!(
        err,
        format!(
            "proposal {}'s rule also reaches \"Go-Toubun no Hanayome\" itself — the series it \
             would be accepted into — so accepting it would set the edition key on that \
             series' own volumes too. Narrow the pattern or write the rule by hand.",
            proposal.id
        ),
        "the refusal names the target as the target, not as collateral"
    );

    let rules: i64 = conn
        .query_row("SELECT count(*) FROM rule WHERE ordinal > 4", [], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(rules, 0, "a refused accept writes no rule at all");
}

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
            write_pages(&dir.path().join(&title).join(format!("{title} - v01")), n);
            n += 1;
        }
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, dir.path()).expect("add root");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("derive");
    drop(conn);
    library
}

#[test]
fn a_widened_rule_reports_the_other_proposals_it_would_resolve() {
    // C3 §3: a person answers many questions once instead of many times. The
    // count has to come from the same computation that refuses collateral, or
    // the form would promise something the write disagrees with.
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
    // This proves `resolves` does not prefix-match the regex's own source
    // text, which would find nothing and report a clean bill of health: the
    // same reach, expressed as a regex, must name the same collateral.
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
        matches!(
            proposals::accept_with(&conn, proposal.id, &rule),
            Ok(Offer::Refused(_))
        ),
        "and the accept is refused, not merely warned about",
    );
}

#[test]
fn a_rule_that_reaches_another_proposals_subject_without_answering_it_is_collateral() {
    // The hole that "reaches it" alone would leave in the guard. Widened all
    // the way — an empty `prefix` pattern, which is every path there is, and
    // the shape `derive::nested_in_claim`'s own doc records a person reaching
    // — this rule reaches the other two colour editions. A dry run that
    // called that *resolving* them would put them, and the base series they
    // point at, into the set it has promised not to report on, and hand back
    // a clean bill of health for a rule that folds all six series into one.
    //
    // What makes a proposal answered is the rule giving its subject its own
    // target's title. This one gives every series the *first* proposal's
    // target instead, so it answers nothing and every series it reaches
    // beyond this proposal's own two ends is collateral — including the two
    // that happen to be somebody else's subject.
    let library = library_with_three_colour_editions();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let first = proposals::open(&conn, 0.0).expect("open").remove(0);
    let mut rule = proposals::preview(&conn, first.id).expect("preview");
    rule.pattern = String::new();
    rule.root_id = None;

    let resolution = proposals::resolves(&conn, first.id, &rule).expect("resolves");

    assert!(
        resolution.also_resolves.is_empty(),
        "a rule that would move them where they never asked to go answers nothing: {:?}",
        resolution.also_resolves,
    );
    assert_eq!(
        resolution.collateral.len(),
        4,
        "the four series that are neither this proposal's subject nor its target: {:?}",
        resolution.collateral,
    );
    assert_eq!(
        resolution
            .collateral
            .iter()
            .filter(|s| s.contains("Full Color Edition"))
            .count(),
        2,
        "the other two proposals' own subjects are collateral here, not group members: {:?}",
        resolution.collateral,
    );
    assert!(
        matches!(
            proposals::accept_with(&conn, first.id, &rule),
            Ok(Offer::Refused(_))
        ),
        "and the widest possible widening is refused, not accepted for six series at once",
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

    let rule_id = made(proposals::accept_with(&conn, first.id, &rule));

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
    // Without this, deleting the rule would leave the other two `accepted` and
    // pointing at nothing — the exact state `created_rule_id` exists to
    // prevent, reached by way of widening.
    let library = library_with_three_colour_editions();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let first = proposals::open(&conn, 0.0).expect("open").remove(0);
    let mut rule = proposals::preview(&conn, first.id).expect("preview");
    rule.kind = "regex".to_string();
    rule.pattern = "^(?<series>.+) - Full Color Edition/".to_string();
    rule.root_id = None;
    made(proposals::accept_with(&conn, first.id, &rule));

    made(proposals::unaccept(&conn, first.id));

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

#[test]
fn a_narrow_accept_still_succeeds_under_a_root_that_carries_a_default() {
    // `CANDIDATE_RULE_ID` is `-1` rather than `0` because `rules.rs` spends
    // `0` on its own `ROOT_FALLBACK` provenance. Every fixture in this file
    // goes through `scan::add_root`, which leaves `edition_key`,
    // `default_kind` and `default_language` NULL, so `apply_root_fallback`
    // never records a `ROOT_FALLBACK` at all and the collision has nothing to
    // collide with.
    //
    // One column is enough to make it real. With the sentinel at `0` and this
    // root carrying `default_language`, every entry in the library would
    // resolve a `Language` field the root decided, `series_reached_by` would
    // read that provenance as the candidate's own, and this narrow rule — the
    // one `preview` derives, matching a single directory — would come back
    // with `Kaguya-sama wa Kokurasetai` as collateral, so the accept would be
    // refused: no accept would be possible at all in any library whose root
    // carries a default.
    let library = library_with_a_shared_publisher_directory();
    let conn = library.conn();
    conn.execute("UPDATE root SET default_language = 'ja'", [])
        .expect("give the root a default");
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    let rule = proposals::preview(&conn, proposal.id).expect("preview");

    let resolution = proposals::resolves(&conn, proposal.id, &rule).expect("resolves");

    assert_eq!(
        resolution.entries_matched, 1,
        "the candidate decided one entry — the root's own default decided the other seven, and \
         they are not the same thing: {resolution:?}",
    );
    assert!(
        resolution.collateral.is_empty(),
        "a rule matching one directory reaches nobody else's material: {:?}",
        resolution.collateral,
    );
    // The narrow accept succeeds under a root that carries a default.
    made(proposals::accept_with(&conn, proposal.id, &rule));
}

/// A candidate that matches only a container directory: it decides nothing
/// about anybody's material, and must be refused.
///
/// Accepted, it would take the question out of the inbox with nothing
/// deciding it, which is exactly what the `entries_matched == 0` refusal
/// exists to prevent.
fn a_container_only_rule_is_refused(pattern: &str) {
    let library = library_with_a_shared_publisher_directory();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    let mut rule = proposals::preview(&conn, proposal.id).expect("preview");
    rule.kind = "regex".to_string();
    rule.pattern = pattern.to_string();

    let resolution = proposals::resolves(&conn, proposal.id, &rule).expect("resolves");
    assert_eq!(
        resolution.entries_matched, 1,
        "`{pattern}` does match something — that is the whole difficulty: {resolution:?}",
    );
    assert!(
        !resolution.subject_reached,
        "`{pattern}` reaches no material this proposal is about: {resolution:?}",
    );

    let refused = refusal_of(proposals::accept_with(&conn, proposal.id, &rule));
    assert!(
        refused.contains("none of them this proposal's own subject"),
        "`{pattern}` is refused, and says why: {refused:?}",
    );

    let state: String = conn
        .query_row(
            "SELECT state FROM proposal WHERE id = ?1",
            [proposal.id],
            |r| r.get(0),
        )
        .expect("state");
    assert_eq!(state, "open", "the question is still in the inbox");
    let rules: i64 = conn
        .query_row("SELECT count(*) FROM rule WHERE ordinal > 4", [], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(rules, 0, "and no rule was left standing");
}

#[test]
fn a_rule_reaching_only_the_publisher_directory_is_refused() {
    a_container_only_rule_is_refused("^Publisher$");
}

#[test]
fn a_rule_reaching_only_the_roots_own_entry_is_refused() {
    // The root's own `scan_entry` row has `rel_path` `""` — `derive`'s own
    // doc says so — and `^$` matches exactly it and nothing else.
    a_container_only_rule_is_refused("^$");
}

/// One base series and its colour sibling, **two volumes each**: the shape that
/// tells "every work in the series this rule reaches" apart from "every work
/// this rule would actually move". Over a fixture giving each series one
/// volume, the two numbers cannot differ.
fn library_with_two_volumes_each() -> Library {
    let dir = crate::tests::tempdir("proposals-two-volumes");
    let mut n = 2_usize;
    for title in [
        "Go-Toubun no Hanayome",
        "Go-Toubun no Hanayome - Full Color Edition",
    ] {
        for volume in ["v01", "v02"] {
            write_pages(
                &dir.path().join(title).join(format!("{title} - {volume}")),
                n,
            );
            n += 1;
        }
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, dir.path()).expect("add root");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("derive");
    drop(conn);
    library
}

#[test]
fn works_moved_counts_the_works_that_move_not_every_work_in_reach() {
    // `works_moved` counts the works that move, not every work in every series
    // the rule touches, matched or not: the `v01`-only regex below matches one
    // entry and must report one work moved. The number is rendered in a
    // sentence saying what accepting will do, so an over-count is a false
    // statement to the person deciding.
    let library = library_with_two_volumes_each();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "edition-of")
        .expect("an edition-of proposal");

    let narrow = proposals::preview(&conn, proposal.id).expect("preview");
    let whole = proposals::resolves(&conn, proposal.id, &narrow).expect("resolves");
    assert_eq!(
        (whole.entries_matched, whole.works_moved),
        (2, 2),
        "the narrow rule takes both volumes, and both works move: {whole:?}",
    );

    let mut half = narrow;
    half.kind = "regex".to_string();
    half.pattern = "^Go-Toubun no Hanayome - Full Color Edition/.* - v01$".to_string();
    let resolution = proposals::resolves(&conn, proposal.id, &half).expect("resolves");

    assert_eq!(
        resolution.entries_matched, 1,
        "one volume directory matches: {resolution:?}",
    );
    assert_eq!(
        resolution.works_moved, 1,
        "and one work moves — the other volume stays exactly where it is: {resolution:?}",
    );
}

/// Two `edition-of` pairs, the second holding two volumes: what it takes to
/// hand one proposal a rule that splits *another* proposal's subject across
/// two destinations. One volume each is not enough — a one-volume series has
/// exactly one entry resolving to it, so its `becomes` set can never hold
/// more than one title.
fn library_with_a_splittable_second_proposal() -> Library {
    let dir = crate::tests::tempdir("proposals-splittable");
    let mut n = 2_usize;
    for (title, volumes) in [
        ("Go-Toubun no Hanayome", 1),
        ("Go-Toubun no Hanayome - Full Color Edition", 1),
        ("Kaguya-sama wa Kokurasetai", 2),
        ("Kaguya-sama wa Kokurasetai - Full Color Edition", 2),
    ] {
        for volume in 1..=volumes {
            write_pages(
                &dir.path()
                    .join(title)
                    .join(format!("{title} - v{volume:02}")),
                n,
            );
            n += 1;
        }
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, dir.path()).expect("add root");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("derive");
    drop(conn);
    library
}

fn proposal_whose_subject_is(conn: &rusqlite::Connection, title: &str) -> i64 {
    proposals::open(conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| {
            let subject: String = conn
                .query_row(
                    "SELECT sort_title FROM series WHERE id = ?1",
                    [p.subject_id],
                    |r| r.get(0),
                )
                .expect("subject title");
            subject == title
        })
        .expect("a proposal with that subject")
        .id
}

#[test]
fn a_rule_that_splits_another_proposals_subject_in_two_does_not_answer_it() {
    // `answers` requires `becomes.len() == 1` as well as `becomes` containing
    // the other proposal's target. It is the same guard-disarming
    // `a_rule_that_reaches_another_proposals_subject_without_answering_it_is_
    // collateral` pins, in its partial form: a rule that sends half of a
    // subject where it asked to go and leaves the other half behind has not
    // answered the question, and counting it as an answer puts that subject —
    // and its target — into the allowed set, taking them back out of
    // `collateral`.
    //
    // The rule below does exactly that to the Kaguya-sama proposal: its first
    // alternative carries no `series` capture, so `set_series` stands and
    // `v01` goes to `Kaguya-sama wa Kokurasetai`, the answer that proposal
    // asked for; its second captures the directory it already sits in, so
    // `v02` stays put. Two destinations, one of them right.
    let library = library_with_a_splittable_second_proposal();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let accepting = proposal_whose_subject_is(&conn, "Go-Toubun no Hanayome - Full Color Edition");
    let split = proposal_whose_subject_is(&conn, "Kaguya-sama wa Kokurasetai - Full Color Edition");

    let mut rule = proposals::preview(&conn, accepting).expect("preview");
    rule.kind = "regex".to_string();
    rule.root_id = None;
    rule.set_series = Some("Kaguya-sama wa Kokurasetai".to_string());
    rule.pattern = "^Kaguya-sama wa Kokurasetai - Full Color Edition/[^/]+ - v01$\
                    |^Kaguya-sama wa Kokurasetai - Full Color Edition/(?<series>[^/]+) - v02$"
        .to_string();

    let resolution = proposals::resolves(&conn, accepting, &rule).expect("resolves");

    assert!(
        !resolution.also_resolves.contains(&split),
        "proposal {split} is not answered by a rule that moves half of its subject: {resolution:?}",
    );
    assert_eq!(
        resolution.collateral,
        vec!["Kaguya-sama wa Kokurasetai - Full Color Edition".to_string()],
        "so its subject is still collateral, which is what keeps the guard armed: {resolution:?}",
    );
}

#[test]
fn a_pattern_that_will_not_compile_is_an_error_not_a_clean_bill_of_health() {
    // `Engine::compile` disables a rule whose pattern will not compile and
    // carries on, which is right for a scan and wrong for a dry run: without
    // the check `series_reached_by` makes on the candidate alone, "resolves
    // nothing" and "your regex has a typo" render identically in the form,
    // and a person widens a pattern that does nothing and accepts it.
    let library = library_with_a_shared_publisher_directory();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    let mut rule = proposals::preview(&conn, proposal.id).expect("preview");
    rule.kind = "regex".to_string();
    rule.pattern = "^Publisher/(?<series>.+".to_string();

    let err = proposals::resolves(&conn, proposal.id, &rule)
        .expect_err("an uncompilable pattern is an error, not an empty resolution");

    assert!(
        err.starts_with("this pattern will not compile:"),
        "the form can tell a typo from a no-op: {err}",
    );
    assert!(
        err.contains("unclosed group"),
        "and it carries `regex`'s own diagnosis rather than a message of ours: {err}",
    );
    let refused = refusal_of(proposals::accept_with(&conn, proposal.id, &rule));
    assert!(
        refused.contains("this pattern will not compile"),
        "and the accept refuses for the same reason, rather than failing: {refused:?}",
    );
}

/// A rule can be judged unevaluable **without a library to judge it
/// against**.
///
/// `resolves` folds "this rule cannot be evaluated" and "the read underneath
/// it failed" into one `Err(String)`, and a caller answering the two
/// differently — `taiyaku-serve`'s dry run answers 400 and 500 — cannot tell
/// them apart from the sentence. `evaluable` is that separation, and taking
/// no `Connection` is the property worth pinning rather than a convenience:
/// whatever it answers, it did not answer it because of the state of a
/// library. The test above holds the other half — that `resolves` still
/// refuses the same rule — and both go through `engine_parts`, so neither can
/// quietly stop agreeing with the other.
#[test]
fn a_rule_can_be_judged_unevaluable_without_a_library_to_judge_it_against() {
    let mut rule = proposals::ProposedRule {
        root_id: None,
        kind: "prefix".to_string(),
        pattern: "Publisher/".to_string(),
        match_on: "path".to_string(),
        set_series: Some("Go-Toubun no Hanayome".to_string()),
        set_edition_key: None,
        set_kind: None,
        set_language: None,
        note: "a candidate".to_string(),
    };
    assert_eq!(
        proposals::evaluable(&rule),
        Ok(()),
        "a rule the engine can compile is not the caller's problem",
    );

    rule.kind = "regex".to_string();
    rule.pattern = "^Publisher/(?<series>.+".to_string();
    let err = proposals::evaluable(&rule).expect_err("an unclosed group will not compile");
    assert!(
        err.contains("unclosed group"),
        "and it carries `regex`'s own diagnosis: {err}",
    );

    rule.kind = "not-a-kind-rules-rs-knows".to_string();
    let err = proposals::evaluable(&rule).expect_err("nor is that a rule kind");
    assert!(
        err.starts_with("this rule cannot be checked:"),
        "a kind the engine does not know is the same class of answer: {err}",
    );
}

#[test]
fn the_targets_reached_refusal_names_every_target_and_agrees_in_number() {
    // The pattern below sends every colour edition to its own base and every
    // base to itself, so all three bases are targets this rule reaches and
    // nothing is collateral: the refusal that fires is the targets one.
    let library = library_with_three_colour_editions();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let first = proposals::open(&conn, 0.0).expect("open").remove(0);
    let mut rule = proposals::preview(&conn, first.id).expect("preview");
    rule.kind = "regex".to_string();
    rule.pattern = "^(?<series>.+?)(?: - Full Color Edition)?/".to_string();
    rule.root_id = None;

    let resolution = proposals::resolves(&conn, first.id, &rule).expect("resolves");
    assert_eq!(
        resolution.targets_reached.len(),
        3,
        "all three bases are reached: {resolution:?}",
    );
    assert!(
        resolution.collateral.is_empty(),
        "and none of it is collateral, so the targets refusal is the one that fires: {resolution:?}",
    );

    let err = refusal_of(proposals::accept_with(&conn, first.id, &rule));

    assert_eq!(
        err,
        format!(
            "proposal {}'s rule also reaches \"Go-Toubun no Hanayome\", \
             \"Kaguya-sama wa Kokurasetai\", \"Kobayashi-san Chi no Meidoragon\" themselves — \
             the series it would be accepted into — so accepting it would set the edition key \
             on those series' own volumes too. Narrow the pattern or write the rule by hand.",
            first.id
        ),
        "each target is named on its own, and the sentence agrees with three of them",
    );
}

/// Land a competing write inside the one window the group UPDATE is exposed
/// to, deterministically: `also_resolves` is read before
/// `unchecked_transaction` opens a *deferred* BEGIN, so nothing is locked
/// between that read and the writes. A trigger on the `rule` INSERT fires
/// after `accept_with` has decided what it will do and before it says so,
/// which is exactly where a second process — `serve` beside
/// `library import`, the pair this project expects on one database file —
/// would land.
///
/// **The trigger's own write rolls back with the transaction**, being inside
/// it, so what this pins is the refusal and the absence of a rule rather than
/// the survival of the other process's row. That is the half that is ours:
/// without the guard the UPDATE lands unconditionally, overwrites
/// `created_rule_id`, and leaves the other rule live and pointed at by
/// nothing.
fn steal_a_proposal_when_the_rule_lands(conn: &rusqlite::Connection, id: i64) {
    conn.execute_batch(&format!(
        "CREATE TRIGGER a_second_process AFTER INSERT ON rule BEGIN \
           UPDATE proposal SET state = 'accepted', created_rule_id = 1 WHERE id = {id}; \
         END;"
    ))
    .expect("install the competing write");
}

fn accepted_rules(conn: &rusqlite::Connection) -> i64 {
    conn.query_row("SELECT count(*) FROM rule WHERE ordinal > 4", [], |r| {
        r.get(0)
    })
    .expect("count")
}

#[test]
fn a_group_member_judged_under_the_accept_fails_it_rather_than_orphaning_its_rule() {
    let library = library_with_three_colour_editions();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let mut open = proposals::open(&conn, 0.0).expect("open");
    let first = open.remove(0);
    let stolen = open
        .iter()
        .find(|p| p.kind == "edition-of")
        .expect("another member of the group")
        .id;
    steal_a_proposal_when_the_rule_lands(&conn, stolen);

    let mut rule = proposals::preview(&conn, first.id).expect("preview");
    rule.kind = "regex".to_string();
    rule.pattern = "^(?<series>.+) - Full Color Edition/".to_string();
    rule.root_id = None;

    let refused = refusal_of(proposals::accept_with(&conn, first.id, &rule));

    assert!(
        refused.contains(&format!("proposal {stolen}, which this rule also answers"))
            && refused.contains("stopped being open"),
        "the accept fails, naming the proposal that moved under it: {refused:?}",
    );
    assert_eq!(
        accepted_rules(&conn),
        0,
        "and rolls its own rule back rather than overwriting `created_rule_id`",
    );
}

#[test]
fn a_proposal_judged_under_its_own_accept_fails_it_rather_than_writing_twice() {
    // The same window, on the proposal being accepted itself.
    // `acceptable_proposal` checks `state = 'open'` before the transaction
    // opens, so a second process accepting this proposal in between would have
    // its own `created_rule_id` replaced unless the write re-checks it — the
    // exact state that column exists to prevent, which
    // `double_accept_is_refused_rather_than_orphaning_the_first_rule`
    // pins for one process.
    let library = library_with_three_colour_editions();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let first = proposals::open(&conn, 0.0).expect("open").remove(0);
    steal_a_proposal_when_the_rule_lands(&conn, first.id);
    let rule = proposals::preview(&conn, first.id).expect("preview");

    let refused = refusal_of(proposals::accept_with(&conn, first.id, &rule));

    assert!(
        refused.contains(&format!("proposal {} stopped being open", first.id)),
        "the accept fails rather than writing over the other judgement: {refused:?}",
    );
    assert_eq!(accepted_rules(&conn), 0, "and leaves no rule behind");
}

/// A candidate that reaches the subject's own material and hands it back the
/// series it already has.
///
/// **The motion the form exists for.** Start from `preview`'s own rule and
/// widen it, which is the one thing the UI's rule editor is for, and the
/// widening itself can cancel the decision. Were such a rule accepted, it
/// would be written, the question would leave the inbox answered, nothing
/// would happen, and only `unaccept` would get it back.
///
/// `subject_reached` does not catch this: it asks whether a matched entry
/// *belongs to* the subject, not whether anything about the subject changes.
/// `works_moved == 0` is the number that says so.
fn a_rule_that_moves_nothing_is_refused(widen: impl FnOnce(&mut proposals::ProposedRule)) {
    let library = library_with_a_shared_publisher_directory();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    let mut rule = proposals::preview(&conn, proposal.id).expect("preview");
    widen(&mut rule);

    let resolution = proposals::resolves(&conn, proposal.id, &rule).expect("resolves");
    assert_eq!(
        resolution.entries_matched, 1,
        "the rule matches the subject's own volume directory: {resolution:?}",
    );
    assert!(
        resolution.subject_reached,
        "and that entry belongs to the subject, so the older guard is satisfied — which is \
         exactly why it cannot be the one that refuses this: {resolution:?}",
    );
    assert_eq!(
        resolution.works_moved, 0,
        "while nothing moves at all: {resolution:?}",
    );

    let refused = refusal_of(proposals::accept_with(&conn, proposal.id, &rule));
    assert!(
        refused.contains("moves none of them"),
        "a rule that changes nothing is refused, and says so: {refused:?}",
    );

    let state: String = conn
        .query_row(
            "SELECT state FROM proposal WHERE id = ?1",
            [proposal.id],
            |r| r.get(0),
        )
        .expect("state");
    assert_eq!(state, "open", "the question is still in the inbox");
    assert_eq!(
        accepted_rules(&conn),
        0,
        "and no rule was left standing to decide nothing",
    );
}

#[test]
fn a_widened_rule_whose_series_capture_overrides_set_series_is_refused() {
    // `rules.rs` applies a regex rule's named captures *after* its literal
    // `set_*` columns — `apply_set_columns` then `apply_captures`, both
    // through `Resolved::set`, which overwrites — so a `(?<series>…)` group
    // added while widening silently replaces the `set_series` the accept was
    // about. Here it captures the colour title the material already carries,
    // and the rule hands the subject back to itself.
    a_rule_that_moves_nothing_is_refused(|rule| {
        rule.kind = "regex".to_string();
        rule.pattern = "^Publisher/(?<series>[^/]+ - Full Color Edition)/".to_string();
    });
}

#[test]
fn an_accept_whose_set_series_is_the_subjects_own_title_is_refused() {
    // The same end state without a capture at all: `preview`'s narrow pattern
    // kept, and `set_series` edited to the subject's own title.
    a_rule_that_moves_nothing_is_refused(|rule| {
        rule.set_series = Some("Go-Toubun no Hanayome - Full Color Edition".to_string());
    });
}

#[test]
fn a_rule_that_moves_another_proposals_subject_but_not_this_ones_is_still_refused() {
    // **What `subject_reached` catches that `works_moved` cannot**, and the
    // reason it survives the refusal above rather than being deleted as
    // redundant. This rule moves material — `works_moved: 1` — so the
    // zero-move refusal never fires; what it moves is *another* proposal's
    // subject, which it genuinely answers, so `collateral` is empty and that
    // refusal does not fire either. It decides nothing about the proposal
    // being accepted, and accepting would mark that question answered by a
    // rule that never looks at it.
    let library = library_with_three_colour_editions();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let ours = proposal_whose_subject_is(&conn, "Go-Toubun no Hanayome - Full Color Edition");
    let mut rule = proposals::preview(&conn, ours).expect("preview");
    rule.kind = "regex".to_string();
    rule.root_id = None;
    rule.pattern = "^Kaguya-sama wa Kokurasetai - Full Color Edition/".to_string();
    rule.set_series = Some("Kaguya-sama wa Kokurasetai".to_string());

    let resolution = proposals::resolves(&conn, ours, &rule).expect("resolves");
    assert_eq!(
        resolution.works_moved, 1,
        "something really does move, so the zero-move refusal cannot be the one that \
         fires: {resolution:?}",
    );
    assert!(
        !resolution.also_resolves.is_empty() && resolution.collateral.is_empty(),
        "and what moves is a proposal this rule answers, so nothing is collateral \
         either: {resolution:?}",
    );
    assert!(
        !resolution.subject_reached,
        "but none of it is this proposal's own subject: {resolution:?}",
    );

    let refused = refusal_of(proposals::accept_with(&conn, ours, &rule));
    assert!(
        refused.contains("none of them this proposal's own subject"),
        "so the subject guard is what refuses it: {refused:?}",
    );
    assert_eq!(accepted_rules(&conn), 0, "and no rule was written");
}

#[test]
fn works_moved_counts_the_entries_that_change_series_not_the_ones_that_match() {
    // The discriminator behind `Reach::moved_members` is the whole of "does
    // this entry actually move".
    //
    // This pattern captures each series directory's own name, so all three
    // series under `Publisher/` are given back exactly the titles they
    // already have: three entries matched, nothing moved. Were every matched
    // entry recorded as moving, this would report 3.
    let library = library_with_a_shared_publisher_directory();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    let mut rule = proposals::preview(&conn, proposal.id).expect("preview");
    rule.kind = "regex".to_string();
    rule.pattern = "^Publisher/(?<series>[^/]+)/".to_string();

    let resolution = proposals::resolves(&conn, proposal.id, &rule).expect("resolves");

    assert_eq!(
        resolution.entries_matched, 3,
        "all three series' volume directories match: {resolution:?}",
    );
    assert_eq!(
        resolution.works_moved, 0,
        "and every one of them keeps the series it already has: {resolution:?}",
    );
}

#[test]
fn works_moved_counts_moving_members_not_work_rows_that_change_series() {
    // `works_moved` counts the works holding at least one *moving member*,
    // which is not the same number as the `work` rows that change series:
    // `derive` regroups after the rule lands, and a work whose two members sit
    // in two different roots can shed one of them and keep its own `series_id`.
    //
    // Both roots below hold the same colour volume — `write_pages` bakes the
    // absolute directory into its bytes, so the two copies have different
    // digests and become two members of one work rather than colliding. The
    // rule is scoped to root A, the shape `preview` writes, so it moves
    // exactly one of the two.
    let a = crate::tests::tempdir("proposals-moved-members-a");
    let b = crate::tests::tempdir("proposals-moved-members-b");
    for base in [&a, &b] {
        write_pages(
            &base
                .path()
                .join("Go-Toubun no Hanayome - Full Color Edition")
                .join("Go-Toubun no Hanayome - Full Color Edition - v01"),
            2,
        );
    }
    write_pages(
        &a.path()
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01"),
        3,
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root_a =
        scan::add_root_in(&conn, a.path(), scan::InLibrary::Named("Manga")).expect("add root a");
    scan::scan_root(&conn, &root_a).expect("scan a");
    let root_b =
        scan::add_root_in(&conn, b.path(), scan::InLibrary::Named("Manga")).expect("add root b");
    scan::scan_root(&conn, &root_b).expect("scan b");
    derive::derive(&conn).expect("derive");
    proposals::raise(&conn).expect("raise");

    let spanning: i64 = conn
        .query_row(
            "SELECT e.work_id FROM member m JOIN edition e ON e.id = m.edition_id \
             GROUP BY e.work_id HAVING count(*) = 2",
            [],
            |r| r.get(0),
        )
        .expect("one work holds both roots' copies of the colour volume");
    let series_before = work_series(&conn);

    let proposal = proposal_whose_subject_is(&conn, "Go-Toubun no Hanayome - Full Color Edition");
    let rule = proposals::ProposedRule {
        root_id: Some(root_a.id),
        kind: "prefix".to_string(),
        pattern: "Go-Toubun no Hanayome - Full Color Edition/".to_string(),
        match_on: "path".to_string(),
        set_series: Some("Go-Toubun no Hanayome".to_string()),
        set_edition_key: Some("full-color-edition".to_string()),
        set_kind: Some("official".to_string()),
        set_language: None,
        note: format!("accepted from proposal {proposal}"),
    };

    let resolution = proposals::resolves(&conn, proposal, &rule).expect("resolves");
    assert_eq!(
        resolution.works_moved, 1,
        "one work holds the member that moves — work {spanning}: {resolution:?}",
    );

    // The root-scoped accept succeeds.
    made(proposals::accept_with(&conn, proposal, &rule));
    derive::derive(&conn).expect("re-derive");

    let series_after = work_series(&conn);
    let changed = series_before
        .iter()
        .filter(|(id, series)| {
            series_after
                .iter()
                .any(|(after_id, after_series)| after_id == id && after_series != series)
        })
        .count();
    assert_eq!(
        changed, 0,
        "and no `work` row changed series at all — work {spanning} kept its own and shed one \
         member to another work: {series_before:?} then {series_after:?}",
    );
}

/// `(work.id, work.series_id)` for every work, in id order.
fn work_series(conn: &rusqlite::Connection) -> Vec<(i64, Option<i64>)> {
    conn.prepare("SELECT id, series_id FROM work ORDER BY id")
        .expect("prepare")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("query")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect")
}

/// A library where 「五等分の花嫁」 is a known alt title of
/// `Go-Toubun no Hanayome`, so `raise` proposes them as one series — the
/// shape `same_series_relates_a_known_alt_title_to_its_series` builds inline,
/// factored out because the `same-series` kind is the one whose rule carries
/// no edition key.
fn library_with_a_same_series_proposal() -> (crate::tests::TempDir, Library) {
    let dir = crate::tests::tempdir("proposals-same-series-fixture");
    write_pages(
        &dir.path()
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01"),
        2,
    );
    write_pages(
        &dir.path().join("五等分の花嫁").join("五等分の花嫁 - v01"),
        3,
    );
    let library = Library::open_in_memory().expect("open");
    {
        let conn = library.conn();
        let root = scan::add_root(&conn, dir.path()).expect("add root");
        scan::scan_root(&conn, &root).expect("scan");
        derive::derive(&conn).expect("derive");
        let established: i64 = conn
            .query_row(
                "SELECT id FROM series WHERE sort_title = 'Go-Toubun no Hanayome'",
                [],
                |r| r.get(0),
            )
            .expect("established series");
        conn.execute(
            "INSERT INTO series_title (series_id, title) VALUES (?1, '五等分の花嫁')",
            [established],
        )
        .expect("known alt title");
        proposals::raise(&conn).expect("raise");
    }
    (dir, library)
}

#[test]
fn the_targets_reached_refusal_does_not_name_an_edition_key_the_rule_has_not_got() {
    // `preview` writes `set_edition_key: None` for every kind but
    // `edition-of`, so the refusal must not name an edition key the rule
    // does not carry. What is true of every rule is the general form —
    // `rules.rs` applies every `set_*` column a matching rule carries to
    // everything it matches.
    let (_tree, library) = library_with_a_same_series_proposal();
    let conn = library.conn();
    let proposal = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "same-series")
        .expect("a same-series proposal");
    let mut rule = proposals::preview(&conn, proposal.id).expect("preview");
    assert!(
        rule.set_edition_key.is_none(),
        "a `same-series` accept carries no edition key: {rule:?}",
    );
    rule.kind = "regex".to_string();
    rule.pattern = "^[^/]+/".to_string();
    rule.root_id = None;

    let resolution = proposals::resolves(&conn, proposal.id, &rule).expect("resolves");
    assert_eq!(
        resolution.targets_reached,
        vec!["Go-Toubun no Hanayome".to_string()],
        "the widened rule reaches the series it would be accepted into: {resolution:?}",
    );
    assert!(
        resolution.collateral.is_empty(),
        "and nothing else, so the targets refusal is the one that fires: {resolution:?}",
    );

    let err = refusal_of(proposals::accept_with(&conn, proposal.id, &rule));

    assert_eq!(
        err,
        format!(
            "proposal {}'s rule also reaches \"Go-Toubun no Hanayome\" itself — the series it \
             would be accepted into — so accepting it would apply every column this rule sets \
             to that series' own volumes too. Narrow the pattern or write the rule by hand.",
            proposal.id
        ),
        "the refusal describes what this rule carries, not what an `edition-of` one would",
    );
}

// --- `list` and `counts_by_kind` ---------------------------------------------

#[test]
fn list_filters_by_state_and_kind_and_keeps_open_as_it_was() {
    // C3 §4: accepted and dismissed proposals stay listed under their own
    // filters — "a dismissal you disagree with later is a thing you have to be
    // able to find".
    let library = library_with_three_colour_editions();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let first = proposals::open(&conn, 0.0).expect("open").remove(0);
    made(proposals::dismiss(&conn, first.id));

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
    assert_eq!(dismissed[0].proposal.id, first.id);

    let editions = proposals::list(
        &conn,
        &proposals::Filter {
            state: Some("open".to_string()),
            kind: Some("edition-of".to_string()),
            min_score: 0.0,
        },
    )
    .expect("list");
    assert!(editions.iter().all(|p| p.proposal.kind == "edition-of"));
    assert!(!editions.iter().any(|p| p.proposal.id == first.id));
}

/// The kind filter half of the test above has no row it must exclude: every
/// proposal `library_with_three_colour_editions` raises is `edition-of`, so a
/// `kind` filter that silently matched everything (an empty `AND kind = ?N`
/// clause left off, say) would pass `.all(|p| p.kind == "edition-of")` too —
/// there is nothing else in the set for the bug to let through. This fixture
/// adds a `same-series` proposal alongside the `edition-of` pair
/// `library_with_two_suffixed_series` already raises, so filtering by kind has
/// a row of the other kind that a broken filter would leak and a working one
/// must not.
#[test]
fn list_filters_by_kind_when_open_holds_more_than_one_kind() {
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    let base_id: i64 = conn
        .query_row(
            "SELECT id FROM series WHERE sort_title = 'Go-Toubun no Hanayome'",
            [],
            |r| r.get(0),
        )
        .expect("base series");
    // A third series whose own title is one `Go-Toubun no Hanayome` has
    // recorded as an alt — the same shape
    // `library_where_two_series_claim_one_title` builds, here added onto the
    // edition-of fixture instead of standing alone, so both kinds are open at
    // once.
    conn.execute(
        "INSERT INTO series (sort_title) VALUES ('Alt Title For GTNH')",
        [],
    )
    .expect("claimant series");
    let claimant_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO series_title (series_id, title) VALUES (?1, 'Alt Title For GTNH')",
        [base_id],
    )
    .expect("alt title");
    proposals::raise(&conn).expect("raise");

    let edition_only = proposals::list(
        &conn,
        &proposals::Filter {
            state: None,
            kind: Some("edition-of".to_string()),
            min_score: 0.0,
        },
    )
    .expect("list");
    assert!(
        !edition_only.is_empty(),
        "the edition-of pair is still there"
    );
    assert!(
        edition_only.iter().all(|p| p.proposal.kind == "edition-of"),
        "{edition_only:?}"
    );
    assert!(
        !edition_only
            .iter()
            .any(|p| p.proposal.subject_id == claimant_id),
        "a kind filter that matched everything would let the same-series proposal through: \
         {edition_only:?}"
    );

    let same_series_only = proposals::list(
        &conn,
        &proposals::Filter {
            state: None,
            kind: Some("same-series".to_string()),
            min_score: 0.0,
        },
    )
    .expect("list");
    assert_eq!(same_series_only.len(), 1, "{same_series_only:?}");
    assert_eq!(same_series_only[0].proposal.subject_id, claimant_id);
    assert_eq!(same_series_only[0].proposal.kind, "same-series");
}

/// C3 §3's inbox is a list rather than a triage queue so a person can *see*
/// that most rows ask the same question — which needs rows that can be told
/// apart, and the only per-row identity a collapsed row has is the titles of
/// the series it names.
///
/// **A row's identity does not come from `reason`.** Every `raise_*` in this
/// module happens to open its sentence with the subject's quoted `sort_title`,
/// but that is not a contract, and `scored_reasons` re-words the same signals
/// *without* titles for the evidence panel. This is the test that pins the
/// titles to the join.
///
/// Both fixtures in one test on purpose: the `edition-of` half fails a join
/// that reads the subject's title for both ends (the two titles differ, and
/// one is a prefix of the other, so a swap is visible), and the
/// `attach-chapters` half fails an *inner* join onto the target — which would
/// not return a wrong title, it would drop the row out of the inbox entirely.
#[test]
fn list_names_both_series_a_proposal_is_about() {
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");

    let listed = proposals::list(
        &conn,
        &proposals::Filter {
            state: None,
            kind: None,
            min_score: 0.0,
        },
    )
    .expect("list");

    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(
        listed[0].subject_title, "Go-Toubun no Hanayome - Full Color Edition",
        "the suffixed series is the subject — the thing being proposed",
    );
    assert_eq!(
        listed[0].target_title.as_deref(),
        Some("Go-Toubun no Hanayome"),
        "and the base series is what the row says it may be an edition of",
    );
    // The other kind: one series, no second one to name.
    let chapters = library_with_unattached_chapters();
    let conn = chapters.conn();
    proposals::raise(&conn).expect("raise");

    let listed = proposals::list(
        &conn,
        &proposals::Filter {
            state: None,
            kind: Some("attach-chapters".to_string()),
            min_score: 0.0,
        },
    )
    .expect("list");

    assert_eq!(
        listed.len(),
        1,
        "an attach-chapters proposal has a NULL target and must still be \
         listed — an inner join onto `series` for the target drops it: {listed:?}",
    );
    assert_eq!(listed[0].subject_title, "Kaguya-sama - Fan Translated");
    assert_eq!(
        listed[0].target_title, None,
        "and there is no second series for the row to point at",
    );
}

/// A `list` that quietly kept filtering on `state` or `kind` even when handed
/// `None` (a stray `AND state = 'open'`, say) would pass every test that pins a
/// filter. The fixture is the same two-kind shape
/// `list_filters_by_kind_when_open_holds_more_than_one_kind` builds, plus a
/// dismissal, so the unfiltered call has both a state and a kind that a
/// silently-filtering `list` would drop.
#[test]
fn list_with_no_filter_returns_every_state_and_kind_in_score_order() {
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    let base_id: i64 = conn
        .query_row(
            "SELECT id FROM series WHERE sort_title = 'Go-Toubun no Hanayome'",
            [],
            |r| r.get(0),
        )
        .expect("base series");
    // A same-series proposal alongside the edition-of pair the base fixture
    // already raises.
    conn.execute(
        "INSERT INTO series (sort_title) VALUES ('Alt Title For GTNH')",
        [],
    )
    .expect("claimant series");
    conn.execute(
        "INSERT INTO series_title (series_id, title) VALUES (?1, 'Alt Title For GTNH')",
        [base_id],
    )
    .expect("alt title");
    proposals::raise(&conn).expect("raise");

    let open_proposals = proposals::open(&conn, 0.0).expect("open");
    assert_eq!(open_proposals.len(), 2, "{open_proposals:?}");
    let edition_of = open_proposals
        .iter()
        .find(|p| p.kind == "edition-of")
        .expect("edition-of proposal")
        .clone();
    let same_series = open_proposals
        .iter()
        .find(|p| p.kind == "same-series")
        .expect("same-series proposal")
        .clone();
    assert!(
        same_series.score > edition_of.score,
        "same-series's CERTAIN score must outrank edition-of for the ordering \
         assertion below to mean anything: {open_proposals:?}"
    );
    made(proposals::dismiss(&conn, edition_of.id));

    let all = proposals::list(
        &conn,
        &proposals::Filter {
            state: None,
            kind: None,
            min_score: 0.0,
        },
    )
    .expect("list");

    assert_eq!(
        all.iter().map(|p| p.proposal.id).collect::<Vec<_>>(),
        vec![same_series.id, edition_of.id],
        "unfiltered list must return the dismissed proposal alongside the open \
         one, highest score first: {all:?}"
    );
    assert_eq!(
        all.iter()
            .find(|p| p.proposal.id == edition_of.id)
            .expect("present")
            .proposal
            .state,
        "dismissed"
    );
    assert_eq!(
        all.iter()
            .find(|p| p.proposal.id == same_series.id)
            .expect("present")
            .proposal
            .state,
        "open"
    );
}

#[test]
fn counts_by_kind_feeds_the_filter_tabs() {
    let library = library_with_three_colour_editions();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");

    let counts = proposals::counts_by_kind(&conn, "open", 0.0).expect("counts");

    assert_eq!(counts.get("edition-of").copied(), Some(3));
}

/// The count above has nothing to exclude either — one call to `raise` over
/// this fixture opens `edition-of` proposals and nothing else, so a
/// `counts_by_kind` that ignored `GROUP BY kind` and returned one bucket
/// keyed on the state's total would report the same `{"edition-of": 3}`. This
/// fixture adds a `same-series` proposal alongside the three colour editions
/// and dismisses one of those, so the map has to carry two separate kinds at
/// once, and the `state` filter has a row (the dismissed edition-of) it must
/// leave out of the `open` count and a kind (same-series) it must leave out
/// of the `dismissed` count.
#[test]
fn counts_by_kind_separates_kinds_and_respects_state() {
    let library = library_with_three_colour_editions();
    let conn = library.conn();
    let base_id: i64 = conn
        .query_row(
            "SELECT id FROM series WHERE sort_title = 'Go-Toubun no Hanayome'",
            [],
            |r| r.get(0),
        )
        .expect("base series");
    conn.execute(
        "INSERT INTO series (sort_title) VALUES ('Alt Title For GTNH')",
        [],
    )
    .expect("claimant series");
    let claimant_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO series_title (series_id, title) VALUES (?1, 'Alt Title For GTNH')",
        [base_id],
    )
    .expect("alt title");
    proposals::raise(&conn).expect("raise");
    assert!(
        proposals::open(&conn, 0.0)
            .expect("open")
            .iter()
            .any(|p| p.subject_id == claimant_id),
        "the same-series proposal is open"
    );
    let one_edition = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "edition-of")
        .expect("an edition-of proposal is open");
    made(proposals::dismiss(&conn, one_edition.id));

    let open_counts = proposals::counts_by_kind(&conn, "open", 0.0).expect("counts");
    assert_eq!(
        open_counts.get("edition-of").copied(),
        Some(2),
        "one of the three edition-of proposals was dismissed: {open_counts:?}"
    );
    assert_eq!(
        open_counts.get("same-series").copied(),
        Some(1),
        "{open_counts:?}"
    );

    let dismissed_counts = proposals::counts_by_kind(&conn, "dismissed", 0.0).expect("counts");
    assert_eq!(dismissed_counts.get("edition-of").copied(), Some(1));
    assert_eq!(
        dismissed_counts.get("same-series"),
        None,
        "the same-series proposal is still open, not dismissed: {dismissed_counts:?}"
    );
}

/// The counts and the list have to narrow on the same threshold, or a tab
/// reads "3" over a list of two.
///
/// The score scale is `{0, 1, 2, 3}` — see [`Filter::min_score`] — so this
/// drops one of the three colour editions to a score below the threshold and
/// keeps the other two above it, which is what makes the difference between a
/// count that follows the threshold and one that ignores it visible at all:
/// over a fixture whose proposals all score the same, both answer 3.
#[test]
fn counts_by_kind_narrows_on_min_score_the_way_the_list_does() {
    let library = library_with_three_colour_editions();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let weakened = proposals::open(&conn, 0.0).expect("open")[0].id;
    conn.execute("UPDATE proposal SET score = 1.0 WHERE id = ?1", [weakened])
        .expect("weakens one proposal");

    let counts = proposals::counts_by_kind(&conn, "open", 2.0).expect("counts");

    assert_eq!(
        counts.get("edition-of").copied(),
        Some(2),
        "the weakened proposal is below the threshold: {counts:?}"
    );
    assert_eq!(
        proposals::list(
            &conn,
            &proposals::Filter {
                state: Some("open".to_string()),
                kind: None,
                min_score: 2.0,
            },
        )
        .expect("list")
        .len(),
        2,
        "and the list it sits over says the same"
    );
    assert_eq!(
        proposals::counts_by_kind(&conn, "open", 0.0)
            .expect("counts")
            .get("edition-of")
            .copied(),
        Some(3),
        "at no threshold it is counted again — otherwise this test would \
         pass over a count that had simply lost a row"
    );
}

/// `lookup` answers `None` for a row that is not there, and an error only for
/// a read that failed — the split C3's routes need in order to answer an
/// unknown id 404 without matching on this module's own sentences.
///
/// Both halves, because either alone passes over the wrong implementation: a
/// `lookup` that returned `Ok(None)` for every failure would satisfy the
/// first assertion, and one that returned `Err` for a missing row — which is
/// what [`one`] does — would satisfy the second.
#[test]
fn lookup_separates_a_missing_proposal_from_a_failed_read() {
    let library = library_with_three_colour_editions();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let present = proposals::open(&conn, 0.0).expect("open")[0].id;

    assert_eq!(
        proposals::lookup(&conn, present)
            .expect("a present proposal is not an error")
            .map(|p| p.id),
        Some(present)
    );
    assert!(
        proposals::lookup(&conn, 9999)
            .expect("an absent proposal is not an error either")
            .is_none(),
        "an id naming no proposal is Ok(None)"
    );

    conn.execute_batch("PRAGMA writable_schema = ON; DROP TABLE proposal;")
        .expect("drops the table it reads");
    let why = proposals::lookup(&conn, present).expect_err("a read that failed is an error");
    assert!(why.contains("proposal"), "{why}");
}

/// A library holding one chapter-organised directory and no volume anyone
/// could make out of it — `scan::tests::fixture`'s "Kaguya-sama - Fan
/// Translated", which `derive` leaves visibly unattached and
/// `raise_attach_chapters` turns into the one proposal of that kind.
///
/// **A side with no `work` row behind it at all**, which is what makes it the
/// fixture for `first_volume`'s absent case: a series row exists (`derive`
/// reconciles one for every series that produced *anything*, so
/// `proposal.subject_id` has something to point at) and no work does.
///
/// The tree is dropped before returning, as `library_with_series` does and
/// for the same reason: every fact these tests read is in the database by
/// then, and nothing in `proposals` reopens a file.
fn library_with_unattached_chapters() -> Library {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add root");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("derive");
    drop(conn);
    library
}

#[test]
fn evidence_carries_both_sides_and_degrades_rather_than_breaking() {
    // C3 §8: "every evidence field renders when present and degrades when
    // absent — a proposal whose subject has no interior pages yet must not
    // render a broken panel". The store's half of that is never handing the
    // client an index it cannot fetch.
    //
    // `library_with_two_suffixed_series` is what makes that assertion bite
    // rather than pass vacuously: its two members hold two and three pages,
    // so the mockup's own `p012`/`p047` — the numbers a hard-coded
    // implementation would return — name pages neither side has.
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);

    let ev = proposals::evidence(&conn, proposal.id).expect("evidence");

    assert_eq!(ev.proposal.id, proposal.id);
    assert_eq!(
        ev.subject.directories.len(),
        1,
        "one directory holds this side's material, and it is shown verbatim: {:?}",
        ev.subject.directories,
    );
    assert!(
        ev.subject.directories[0].ends_with("/Go-Toubun no Hanayome - Full Color Edition"),
        "the directory on disk, not a description of it: {:?}",
        ev.subject.directories,
    );
    assert!(ev.target.is_some(), "an edition-of proposal has two sides");
    let first = ev.subject.first_volume.expect("volume 1");
    assert_eq!(
        first.page_count, 3,
        "the subject's only member holds three pages",
    );
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
    assert!(
        !first.interior.is_empty(),
        "and a three-page member still has one interior page to offer",
    );
    assert!(
        ev.rule.is_some(),
        "an open, previewable proposal carries its rule"
    );
    assert_eq!(
        ev.reasons.iter().map(|r| r.sign).collect::<Vec<_>>(),
        vec!['+', '+'],
        "both of §6's signals fired, which is what scored this a 2: {:?}",
        ev.reasons,
    );
    assert_eq!(proposal.score, 2.0, "and the score says the same");
    assert!(
        ev.reasons[1].text.contains("both hold 1 volume(s)"),
        "the volume signal is worded once, for the sentence and for this line: {:?}",
        ev.reasons,
    );
}

#[test]
fn evidence_for_an_attach_chapters_proposal_carries_no_rule() {
    // `preview` refuses that kind by name. The evidence route must still
    // answer — the row is listed and expandable, just not acceptable — so the
    // refusal becomes an absent rule rather than a failed request.
    //
    // And the same side is the one C3 §8's "degrades when absent" is about:
    // a chapter-organised directory produces no `work`, so there is no volume
    // 1 to draw a cover or an interior page from, and saying so is the
    // answer rather than an error.
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
    assert!(ev.target.is_none(), "the kind names only a subject");
    assert_eq!(
        ev.subject.volume_count, 0,
        "no chapter directory became a work",
    );
    assert!(ev.subject.volumes.is_empty());
    assert!(
        ev.subject.first_volume.is_none(),
        "so there is no volume 1 to draw pages from",
    );
    assert!(
        !ev.subject.directories.is_empty(),
        "while the side still says where its material is",
    );
    assert!(
        ev.subject.root_name.is_some(),
        "read from the entries rather than from `member.root_id`, of which          this side has none",
    );
    assert_eq!(
        ev.reasons.iter().map(|r| r.sign).collect::<Vec<_>>(),
        vec!['?'],
        "the one fact this kind states argues for nothing: {:?}",
        ev.reasons,
    );
    // The sentence beside the sign is asserted too: with `text` blank, the
    // expanded row would draw a sign with nothing beside it. The sentence
    // `raise` recorded is carried whole rather than split, which is what
    // `Reason`'s own doc refuses to make a client undo.
    assert_eq!(
        ev.reasons[0].text, proposal.reason,
        "and it is the sentence the proposal already carries, not a second wording",
    );
    assert!(
        ev.reasons[0].text.contains("organised by chapter"),
        "which is a sentence rather than an empty line: {:?}",
        ev.reasons,
    );
}

#[test]
fn evidence_answers_for_a_target_whose_volumes_span_two_directories() {
    // The trap. `common_directory_pattern` *errors* when a series' entries
    // span more than one directory — correct for an accept, which has to be
    // expressible as one prefix rule — but evidence is shown for *both*
    // sides and only the subject is ever asked to be acceptable. Built on
    // that function, this whole request fails for an ordinary target whose
    // volumes sit under two publisher directories, and the person never sees
    // the row at all.
    let dir = crate::tests::tempdir("proposals-evidence-split-target");
    write_pages(
        &dir.path()
            .join("PublisherA")
            .join("Go-Toubun no Hanayome - v01"),
        2,
    );
    write_pages(
        &dir.path()
            .join("PublisherB")
            .join("Go-Toubun no Hanayome - v02"),
        3,
    );
    write_pages(
        &dir.path()
            .join("Colour")
            .join("Go-Toubun no Hanayome - Full Color Edition - v01"),
        4,
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, dir.path()).expect("add root");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("derive");
    let root_path: String = conn
        .query_row("SELECT path FROM root", [], |r| r.get(0))
        .expect("root path");

    let proposal = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "edition-of")
        .expect("an edition-of proposal");
    let ev = proposals::evidence(&conn, proposal.id).expect("evidence answers for both sides");

    let target = ev.target.expect("the base series is the target");
    assert_eq!(target.volume_count, 2);
    assert_eq!(target.volumes, vec![1.0, 2.0], "11 against 14, visibly");
    assert_eq!(
        target.directories,
        vec![
            format!("{root_path}/PublisherA"),
            format!("{root_path}/PublisherB"),
        ],
        "a side that spans two directories hands over both, verbatim and in order, \
         rather than failing or summarising them",
    );
    let name = target.root_name.clone().expect("one root");
    assert!(
        !name.contains('/'),
        "the root's final component, not its whole path: {name}",
    );
    assert!(
        root_path.ends_with(&name),
        "{root_path} does not end {name}"
    );
    let first = target.first_volume.expect("volume 1");
    assert_eq!(
        first.page_count, 2,
        "volume 1 under PublisherA (two pages), not volume 2 under PublisherB (three)",
    );

    assert_eq!(
        ev.subject.directories,
        vec![format!("{root_path}/Colour")],
        "a single-directory side is one element, not a special case",
    );
    assert!(
        ev.rule.is_some(),
        "and the subject being single-directory, an accept is still expressible",
    );

    // The mixed case, which is what makes the signs mean anything: the
    // vocabulary matches and the volume counts do not. One `+` per `SIGNAL`,
    // because the expanded row and the score printed beside it are one
    // computation — `edition_of_signals` — rather than two that could drift.
    assert_eq!(
        ev.reasons.iter().map(|r| r.sign).collect::<Vec<_>>(),
        vec!['+', '-'],
        "{:?}",
        ev.reasons,
    );
    assert_eq!(proposal.score, 1.0, "one signal of the two");
    assert!(
        ev.reasons[0].text.contains("Full Color Edition"),
        "the line names the suffix and the word it matched: {:?}",
        ev.reasons,
    );
    assert!(
        ev.reasons[1].text.contains("2 vs 1 volumes"),
        "and the counts it did not match: {:?}",
        ev.reasons,
    );
}

/// A library whose volume 1 is held **twice** — as a directory of page images
/// and as a `.cbz` of the same volume — which `derive` groups into one edition
/// with two members, and a colour sibling for the proposal to be about.
///
/// **Two members and two formats is the point.** Against a fixture giving every
/// edition exactly one member, a `page_count` summed over the members and a
/// `page_count` read off the first one answer identically, as does a format
/// taken from one member and a format taken from the edition.
///
/// Returns the `TempDir`: it must outlive every use of `library`.
fn library_whose_volume_one_is_a_directory_and_an_archive() -> (crate::tests::TempDir, Library) {
    let dir = crate::tests::tempdir("proposals-evidence-two-members");
    let series = dir.path().join("Go-Toubun no Hanayome");
    write_pages(&series.join("Go-Toubun no Hanayome - v01"), 2);
    crate::scan::tests::write_archive(&series.join("Go-Toubun no Hanayome - v01.cbz"), 3);
    write_pages(
        &dir.path()
            .join("Go-Toubun no Hanayome - Full Color Edition")
            .join("Go-Toubun no Hanayome - Full Color Edition - v01"),
        4,
    );
    let library = Library::open_in_memory().expect("open");
    {
        let conn = library.conn();
        let root = scan::add_root(&conn, dir.path()).expect("add root");
        scan::scan_root(&conn, &root).expect("scan");
        derive::derive(&conn).expect("derive");
    }
    (dir, library)
}

#[test]
fn evidence_names_the_formats_the_edition_was_walked_as_and_sums_its_pages() {
    // `member.format` is written by nothing — the only `INSERT INTO member`
    // in the repository does not list the column — so the formats come from
    // `scan_entry`, where the tokens actually are.
    //
    // This side's edition holds two members, so a `page_count` read off the
    // first member (2) and one summed over both (5) are different numbers.
    // `/api/works/{work}/{edition}/pages/{index}` takes a flat index across the
    // members, so the sum is the one that makes `interior` fetchable at all.
    let (_tree, library) = library_whose_volume_one_is_a_directory_and_an_archive();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "edition-of")
        .expect("an edition-of proposal");

    let ev = proposals::evidence(&conn, proposal.id).expect("evidence");

    let target = ev.target.expect("the base series is the target");
    let first = target.first_volume.expect("volume 1");
    assert_eq!(
        first.formats,
        vec!["dir".to_string(), "zip".to_string()],
        "both members' formats, from `scan_entry` where the tokens actually are",
    );
    assert_eq!(
        first.page_count, 5,
        "two pages in the directory and three in the archive, as one sequence",
    );
    assert!(
        first
            .interior
            .iter()
            .all(|i| *i > 0 && *i < first.page_count),
        "and the interiors land inside it: {:?}",
        first.interior,
    );

    let subject = ev.subject.first_volume.expect("the colour side's volume 1");
    assert_eq!(
        subject.formats,
        vec!["dir".to_string()],
        "a one-member edition still says what it is, once",
    );
}

#[test]
fn evidence_builds_its_urls_from_the_work_and_edition_rows_the_library_holds() {
    // `FirstVolume` exists so "the panel builds its URLs against routes that
    // exist", and both fields that carry that — `work_slug` and `edition_key`
    // — are asserted against the rows the library holds, or every image in the
    // expanded row would 404 with nothing noticing.
    //
    // **The hub is deliberately not `raw` on one of the two sides.** Against
    // a library where every work's hub is `raw` — which is every other
    // fixture here, `DEFAULT_EDITION_KEY` being that — asserting the key
    // cannot tell a read of `work.hub` from the literal `"raw"`. A rule
    // carrying `set_edition_key` over one directory is what makes the two
    // sides answer differently, and it is an ordinary editable rule row
    // rather than a hand-written `work`.
    let dir = crate::tests::tempdir("proposals-evidence-hub");
    write_pages(
        &dir.path()
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01"),
        2,
    );
    write_pages(
        &dir.path()
            .join("Colour")
            .join("Go-Toubun no Hanayome - Full Color Edition - v01"),
        3,
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, dir.path()).expect("add root");
    scan::scan_root(&conn, &root).expect("scan");
    conn.execute(
        "INSERT INTO rule (ordinal, kind, pattern, match_on, set_edition_key) \
         VALUES (99, 'prefix', 'Colour/', 'path', 'colour')",
        [],
    )
    .expect("a rule that gives the colour directory its own edition key");
    derive::derive(&conn).expect("derive");
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "edition-of")
        .expect("an edition-of proposal");

    let ev = proposals::evidence(&conn, proposal.id).expect("evidence");

    for (side, series_id) in [
        (&ev.subject, ev.subject.series_id),
        (
            ev.target.as_ref().expect("two sides"),
            ev.target.as_ref().expect("two sides").series_id,
        ),
    ] {
        let (slug, hub): (String, String) = conn
            .query_row(
                "SELECT slug, hub FROM work WHERE series_id = ?1 ORDER BY volume, id LIMIT 1",
                [series_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("the work row this side's volume 1 is");
        let first = side
            .first_volume
            .clone()
            .expect("every side here has a volume 1");
        assert_eq!(first.work_slug, slug, "{}", side.title);
        assert_eq!(first.edition_key, hub, "{}", side.title);
        // The route the panel builds is `/api/works/{work}/{edition}/…`, so
        // the pair has to name a row rather than merely be non-empty.
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM edition JOIN work ON work.id = edition.work_id \
                 WHERE work.slug = ?1 AND edition.key = ?2",
                rusqlite::params![first.work_slug, first.edition_key],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(exists, 1, "{} names no edition row", side.title);
    }
    assert_eq!(
        ev.subject
            .first_volume
            .as_ref()
            .expect("volume 1")
            .edition_key,
        "colour",
        "the two sides' keys differ, which is what makes the assertion above bite",
    );
    assert_eq!(
        ev.target
            .as_ref()
            .and_then(|t| t.first_volume.as_ref())
            .expect("volume 1")
            .edition_key,
        "raw",
    );
}

#[test]
fn evidence_shows_the_lowest_numbered_volume_rather_than_an_unnumbered_one_shot() {
    // A series holding a one-shot that carries no volume number beside a
    // numbered volume 1.
    //
    // `ORDER BY volume IS NULL` is what keeps the one-shot out of the heading.
    // SQLite sorts NULLs first, and `derive` groups by
    // `(series, Option<volume>, key)` — `None` before `Some` — so the one-shot
    // also takes the *lower* `work.id`. Both tie-breaks a plain
    // `ORDER BY volume, id` would fall back on point the wrong way.
    //
    // `volume_count` counts every `work` row and `volumes` lists only the
    // numbered ones, and against every other fixture here those are the same
    // number. Here they are 2 and 1.
    let dir = crate::tests::tempdir("proposals-evidence-one-shot");
    let series = dir.path().join("Go-Toubun no Hanayome");
    write_pages(&series.join("Go-Toubun no Hanayome - v01"), 2);
    write_pages(&series.join("Go-Toubun no Hanayome (Oneshot)"), 5);
    write_pages(
        &dir.path()
            .join("Go-Toubun no Hanayome - Full Color Edition")
            .join("Go-Toubun no Hanayome - Full Color Edition - v01"),
        3,
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, dir.path()).expect("add root");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("derive");
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "edition-of")
        .expect("an edition-of proposal");

    let ev = proposals::evidence(&conn, proposal.id).expect("evidence");

    let target = ev.target.expect("the base series is the target");
    assert_eq!(
        target.volume_count, 2,
        "the one-shot is a work of this series and is counted as one",
    );
    assert_eq!(
        target.volumes,
        vec![1.0],
        "and carries no number, so it is not in the list",
    );
    let first = target.first_volume.expect("volume 1");
    let slug: String = conn
        .query_row(
            "SELECT slug FROM work WHERE volume = 1.0 AND series_id = ?1",
            [target.series_id],
            |r| r.get(0),
        )
        .expect("the numbered volume 1");
    assert_eq!(
        first.work_slug, slug,
        "the numbered volume, not the one-shot that sorts before it",
    );
    assert_eq!(
        first.page_count, 2,
        "which is the two-page directory rather than the five-page one-shot",
    );
}

#[test]
fn evidence_lists_a_directory_per_root_for_a_side_spanning_two_roots() {
    // Two roots each holding their own `Publisher/`: one directory name,
    // spelled the same in both roots, under which a side's volumes sit in
    // each. The side lists one directory per root and names no single root.
    let a = crate::tests::tempdir("proposals-evidence-roots-a");
    let b = crate::tests::tempdir("proposals-evidence-roots-b");
    write_pages(
        &a.path()
            .join("Publisher")
            .join("Go-Toubun no Hanayome - v01"),
        2,
    );
    write_pages(
        &b.path()
            .join("Publisher")
            .join("Go-Toubun no Hanayome - v02"),
        3,
    );
    write_pages(
        &a.path()
            .join("Colour")
            .join("Go-Toubun no Hanayome - Full Color Edition - v01"),
        4,
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root_a =
        scan::add_root_in(&conn, a.path(), scan::InLibrary::Named("Manga")).expect("add root a");
    scan::scan_root(&conn, &root_a).expect("scan a");
    let root_b =
        scan::add_root_in(&conn, b.path(), scan::InLibrary::Named("Manga")).expect("add root b");
    scan::scan_root(&conn, &root_b).expect("scan b");
    derive::derive(&conn).expect("derive");
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "edition-of")
        .expect("an edition-of proposal");

    let ev = proposals::evidence(&conn, proposal.id).expect("evidence");

    let target = ev.target.expect("the base series is the target");
    let mut expected = vec![
        format!("{}/Publisher", a.path().to_string_lossy()),
        format!("{}/Publisher", b.path().to_string_lossy()),
    ];
    expected.sort();
    assert_eq!(
        target.directories, expected,
        "one directory per root, each joined onto the root it is actually under — \
         joining both onto either root names a path that does not exist",
    );
    assert_eq!(
        target.volumes,
        vec![1.0, 2.0],
        "one volume in each root, and both are this series'",
    );
    assert!(
        target.root_name.is_none(),
        "and no single root to name: {:?}",
        target.root_name,
    );
    assert!(
        ev.subject.root_name.is_some(),
        "while the side that does sit under one still names it",
    );
    assert!(
        ev.rule.is_some(),
        "the subject is under one directory, so an accept is still expressible",
    );
}

#[test]
fn a_refused_rule_is_an_absent_one_and_a_read_that_failed_is_an_error() {
    // `Evidence::rule`'s doc says `None` means "every refusal `preview`
    // makes", and a read that *failed* must not be presented to a person as
    // "this proposal cannot be accepted". `Offer` is what separates them, and
    // this is the one refusal whose own message needs a read to build, so it is
    // where both halves are reachable.
    //
    // **Asserted on `offered_rule` rather than through `evidence`**, because
    // every read `offered_rule` makes that `evidence` has not already made
    // itself is this one — the sweep and both titles come from the caller — so
    // breaking the `root` table to reach it breaks the sweep first and
    // `evidence` fails before the rule is ever asked for. Which is the same
    // outcome, arrived at one step earlier.
    let a = crate::tests::tempdir("proposals-refusal-vs-failure-a");
    write_pages(
        &a.path()
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01"),
        2,
    );
    write_pages(
        &a.path()
            .join("Publisher")
            .join("Go-Toubun no Hanayome - Full Color Edition - v01"),
        3,
    );
    let b = crate::tests::tempdir("proposals-refusal-vs-failure-b");
    write_pages(
        &b.path()
            .join("Publisher")
            .join("Go-Toubun no Hanayome - Full Color Edition - v02"),
        4,
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root_a =
        scan::add_root_in(&conn, a.path(), scan::InLibrary::Named("Manga")).expect("add root a");
    scan::scan_root(&conn, &root_a).expect("scan a");
    let root_b =
        scan::add_root_in(&conn, b.path(), scan::InLibrary::Named("Manga")).expect("add root b");
    scan::scan_root(&conn, &root_b).expect("scan b");
    derive::derive(&conn).expect("derive");
    let proposal = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "edition-of")
        .expect("an edition-of proposal");
    let subject_title = "Go-Toubun no Hanayome - Full Color Edition";
    let paths = derive::rel_paths_for_series(&conn, subject_title).expect("sweep");

    // The refusal: a row that still expands, and still says why it cannot be
    // accepted when asked directly.
    let ev = proposals::evidence(&conn, proposal.id).expect("the row still expands");
    assert!(
        ev.rule.is_none(),
        "a subject under two roots cannot be scoped to one: {:?}",
        ev.rule,
    );
    match proposals::offered_rule(&conn, &proposal, subject_title, &paths).expect("not a failure") {
        proposals::Offer::Refused(why) => assert!(
            why.contains("in more than one root"),
            "the refusal names what it refuses: {why}",
        ),
        proposals::Offer::Made(rule) => panic!("offered a rule it cannot scope: {rule:?}"),
    }

    // The same refusal path, with the read its message is built from gone.
    conn.execute(
        "ALTER TABLE root RENAME TO root_that_is_no_longer_there",
        [],
    )
    .expect("take the root table out from under the read");

    let failed = proposals::offered_rule(&conn, &proposal, subject_title, &paths)
        .expect_err("a read that fails is an error, not a refusal");
    assert!(
        failed.contains("looking up root"),
        "and it says which read failed rather than becoming an absent rule: {failed}",
    );
}

#[test]
fn evidence_for_a_same_series_proposal_states_its_one_fact_and_signs_it_for() {
    // A `same-series` proposal states one fact, and the fact is evidence for:
    // one `+`, not the two signals `scored_reasons`' kind filter keeps for
    // `edition-of`, about a prefix relationship this kind never had.
    let (_tree, library) = library_with_a_same_series_proposal();
    let conn = library.conn();
    let proposal = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "same-series")
        .expect("a same-series proposal");

    let ev = proposals::evidence(&conn, proposal.id).expect("evidence");

    assert_eq!(
        ev.reasons.len(),
        1,
        "one fact, not a pair of signals about a suffix this kind has not got: {:?}",
        ev.reasons,
    );
    assert_eq!(ev.reasons[0].sign, '+', "{:?}", ev.reasons);
    assert_eq!(
        ev.reasons[0].text, proposal.reason,
        "the sentence `raise` recorded, carried whole",
    );
    assert!(
        ev.reasons[0].text.contains("is a known title of"),
        "which is this kind's own wording: {:?}",
        ev.reasons,
    );
    assert!(
        ev.target.is_some(),
        "the kind names two series, and the row shows both",
    );
}

#[test]
fn evidence_says_so_when_neither_side_has_a_volume_to_compare() {
    // `volume_signal`'s three cases are one function so that the sentence and
    // the sign cannot disagree. The `?` case — neither side has any known
    // volumes — must not read as evidence *against* a proposal, where the
    // honest answer is that nobody has asked the question yet. §6 says absence
    // of evidence is not disagreement any more than it is agreement.
    //
    // Two chapter-organised series produce that: `derive` reconciles a
    // `series` row for each and no `work` at all, and the titles are still in
    // the `"{target} - "` relation that raises an `edition-of` proposal.
    let dir = crate::tests::tempdir("proposals-evidence-no-volumes");
    write_pages(
        &dir.path().join("Kaguya-sama").join("Kaguya-sama - c001"),
        2,
    );
    write_pages(
        &dir.path()
            .join("Kaguya-sama - Fan Translated")
            .join("Kaguya-sama - Fan Translated - c001"),
        3,
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, dir.path()).expect("add root");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("derive");
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "edition-of")
        .expect("an edition-of proposal");

    let ev = proposals::evidence(&conn, proposal.id).expect("evidence");

    assert_eq!(
        ev.subject.volume_count, 0,
        "a chapter-organised directory becomes no work",
    );
    assert_eq!(
        ev.target.as_ref().expect("two sides").volume_count,
        0,
        "on either side",
    );
    assert_eq!(
        ev.reasons.iter().map(|r| r.sign).collect::<Vec<_>>(),
        vec!['+', '?'],
        "the vocabulary matched; the volumes were never counted: {:?}",
        ev.reasons,
    );
    assert_eq!(
        ev.reasons[1].text,
        "neither side has any known volumes yet, so there is nothing to compare",
        "and the sentence says which of the three cases this is",
    );
    assert_eq!(
        proposal.score, 1.0,
        "one signal of the two fired, which is what the signs say too",
    );
}

#[test]
fn evidence_for_a_renamed_target_states_the_rename_rather_than_scoring_it() {
    // `preview` refuses an accept for this — `strip_prefix` finds nothing, and
    // setting `set_series` alone would merge the colour volume's pages into the
    // base edition — so the expanded row has to say what happened rather than
    // argue for or against: read as a signal *for*, it would sit beside a
    // proposal whose accept control is not even there.
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    conn.execute(
        "UPDATE series SET sort_title = 'Something Else Entirely' \
         WHERE sort_title = 'Go-Toubun no Hanayome'",
        [],
    )
    .expect("rename the target out from under the proposal");

    let ev = proposals::evidence(&conn, proposal.id).expect("the row still expands");

    assert!(
        ev.rule.is_none(),
        "the accept `preview` refuses is an absent rule: {:?}",
        ev.rule,
    );
    assert_eq!(
        ev.reasons[0].sign, '?',
        "a rename argues neither way — there is no suffix left to have matched \
         anything: {:?}",
        ev.reasons,
    );
    assert!(
        ev.reasons[0].text.contains("renamed"),
        "and the line says so in the words that name both titles: {:?}",
        ev.reasons,
    );
    assert!(
        ev.reasons[0].text.contains("Something Else Entirely"),
        "the target's title as it reads now: {:?}",
        ev.reasons,
    );
}

#[test]
fn interior_indices_land_inside_every_length_they_are_asked_for() {
    // The store's half of C3 §8, checked across every short length rather
    // than at the one or two a fixture happens to hold: the failure this
    // guards is an index off the end of a *short* member.
    for pages in 0..40_i64 {
        let picked = proposals::interior_indices(pages);
        assert!(
            picked.iter().all(|i| *i > 0 && *i < pages),
            "{pages} pages: {picked:?}",
        );
        // None below two pages — there is no page but the cover — one while
        // the two fractions land together, and two from four pages up.
        let expected = usize::from(pages >= 2) + usize::from(pages >= 4);
        assert_eq!(picked.len(), expected, "{pages} pages: {picked:?}");
    }
}

/// Two roots, both holding the base series' only volume as a plain
/// directory, plus a colour edition under the first root so an `edition-of`
/// proposal exists to expand.
///
/// The base's volume 1 is walked from *two* roots — root A's copy and root
/// B's copy each become their own `member` row on the same edition, both
/// `dir`. That is the shape `edition_formats` answers a set for: as a list
/// of each member's format, this edition's `formats` would be `["dir", "dir"]`.
///
/// Returns both `TempDir`s: each must outlive every use of `library`.
fn library_whose_only_volume_is_walked_as_a_directory_from_two_roots()
-> (crate::tests::TempDir, crate::tests::TempDir, Library) {
    let a = crate::tests::tempdir("proposals-evidence-dup-format-a");
    let b = crate::tests::tempdir("proposals-evidence-dup-format-b");
    write_pages(
        &a.path()
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01"),
        2,
    );
    write_pages(
        &a.path()
            .join("Go-Toubun no Hanayome - Full Color Edition")
            .join("Go-Toubun no Hanayome - Full Color Edition - v01"),
        3,
    );
    write_pages(
        &b.path()
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01"),
        4,
    );
    let library = Library::open_in_memory().expect("open");
    {
        let conn = library.conn();
        let root_a = scan::add_root_in(&conn, a.path(), scan::InLibrary::Named("Manga"))
            .expect("add root a");
        scan::scan_root(&conn, &root_a).expect("scan a");
        let root_b = scan::add_root_in(&conn, b.path(), scan::InLibrary::Named("Manga"))
            .expect("add root b");
        scan::scan_root(&conn, &root_b).expect("scan b");
        derive::derive(&conn).expect("derive");
    }
    (a, b, library)
}

#[test]
fn edition_formats_are_distinct_even_when_two_members_share_one() {
    // On this two-root shape the base's volume 1 gets a `dir` member from
    // root A and a second `dir` member from root B, one edition with two
    // members of the same format. As a list of each member's format, this
    // side's `formats` would answer `["dir", "dir"]`.
    let (_a, _b, library) = library_whose_only_volume_is_walked_as_a_directory_from_two_roots();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "edition-of")
        .expect("an edition-of proposal");

    let ev = proposals::evidence(&conn, proposal.id).expect("evidence");

    let target = ev.target.expect("the base series is the target");
    let first = target.first_volume.expect("volume 1, held by two members");
    assert_eq!(
        first.formats,
        vec!["dir".to_string()],
        "two `dir` members on one edition still answer one distinct format",
    );
}

/// One series of one volume, scanned and derived. The volume is whatever
/// `write` puts at the path it is given, a file or a folder; that path is
/// answered with the library.
///
/// Returns the `TempDir`: it must outlive every use of `library`.
fn library_with_one_volume(
    label: &str,
    name: &str,
    write: impl FnOnce(&std::path::Path),
) -> (crate::tests::TempDir, Library, std::path::PathBuf) {
    let dir = crate::tests::tempdir(label);
    let volume = dir.path().join("Series").join(name);
    write(&volume);
    let library = Library::open_in_memory().expect("open");
    {
        let conn = library.conn();
        let root = scan::add_root(&conn, dir.path()).expect("add root");
        scan::scan_root(&conn, &root).expect("scan");
        derive::derive(&conn).expect("derive");
    }
    (dir, library, volume)
}

/// The edition of the library's one member, or `None` where derive made no
/// member at all.
fn the_one_members_edition(conn: &rusqlite::Connection) -> Option<i64> {
    let editions: Vec<i64> = conn
        .prepare("SELECT edition_id FROM member")
        .expect("prepare")
        .query_map([], |r| r.get(0))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("reads the members");
    assert!(editions.len() <= 1, "the fixture: one volume: {editions:?}");
    editions.first().copied()
}

/// Store `token` as the format of the entry at `rel_path`, its size and mtime
/// left as the scan wrote them: a row that disagrees with the bytes it
/// describes, so an answer of `token` came from the row and not from the file.
fn store_format(conn: &rusqlite::Connection, rel_path: &str, token: &str) {
    let changed = conn
        .execute(
            "UPDATE scan_entry SET format = ?1 WHERE rel_path = ?2",
            [token, rel_path],
        )
        .expect("stores the token");
    assert_eq!(changed, 1, "the fixture: one entry at {rel_path}");
}

/// A RAR to `taiyaku_read::sniff` in any build, whether or not it reads one.
const RAR_BYTES: &[u8] = b"Rar!\x1a\x07\x00padding to make a plausible file";

/// A file changed under its name since the scan is not what its row describes,
/// so the formats are what its bytes say now: another format's token, or none
/// where the bytes name no format.
#[test]
fn a_volume_converted_in_place_since_the_scan_reports_the_format_it_is_now() {
    let (_tree, library, volume) =
        library_with_one_volume("proposals-formats-converted", "Series v01.cbz", |at| {
            crate::scan::tests::write_archive(at, 3);
        });
    let conn = library.conn();
    let edition = the_one_members_edition(&conn).expect("the archive is a member");
    assert_eq!(
        proposals::edition_formats(&conn, edition).expect("formats"),
        ["zip"],
        "the fixture: scanned as a zip",
    );

    std::fs::write(&volume, RAR_BYTES).expect("replaces the zip with a RAR");
    assert_eq!(
        proposals::edition_formats(&conn, edition).expect("formats"),
        ["rar"],
        "the row still says zip, of a file that is no longer the one it describes",
    );

    std::fs::write(&volume, b"bytes that name no format").expect("replaces it again");
    assert_eq!(
        proposals::edition_formats(&conn, edition).expect("formats"),
        Vec::<String>::new(),
        "bytes that name nothing leave no format to report, the row's least of all",
    );
}

/// The case the rule is for: a RAR repacked as a zip under the same name. Only
/// a build that reads RAR makes a member of one.
#[test]
fn a_rar_repacked_as_a_zip_under_its_name_reports_zip_before_the_next_scan() {
    let fixture = crate::scan::tests::rar_fixture(crate::scan::tests::READABLE_RAR);
    let (tree, library, volume) =
        library_with_one_volume("proposals-formats-repacked", "Series v01.cbr", |at| {
            std::fs::create_dir_all(at.parent().expect("a folder")).expect("mkdir");
            std::fs::copy(&fixture.path, at).expect("copies the fixture");
        });
    let conn = library.conn();
    let Some(edition) = the_one_members_edition(&conn) else {
        assert!(
            !taiyaku_read::Format::Rar.is_readable(),
            "a build with a RAR backend makes a member of a RAR"
        );
        return;
    };
    assert_eq!(
        proposals::edition_formats(&conn, edition).expect("formats"),
        ["rar"],
        "the fixture: scanned as a RAR",
    );

    let repacked = tree.path().join("repacked.zip");
    crate::scan::tests::write_archive(&repacked, 3);
    std::fs::rename(&repacked, &volume).expect("puts the zip where the RAR was");

    assert_eq!(
        proposals::edition_formats(&conn, edition).expect("formats"),
        ["zip"],
    );
}

/// A file with the size and mtime the scan saw is answered from its row and
/// not opened: the row here says `pdf` of bytes that are a zip, and `pdf` is
/// the answer.
#[test]
fn a_volume_as_the_scan_saw_it_reports_the_stored_format_without_being_read() {
    let (_tree, library, _volume) =
        library_with_one_volume("proposals-formats-untouched", "Series v01.cbz", |at| {
            crate::scan::tests::write_archive(at, 3);
        });
    let conn = library.conn();
    let edition = the_one_members_edition(&conn).expect("the archive is a member");
    store_format(&conn, "Series/Series v01.cbz", "pdf");

    assert_eq!(
        proposals::edition_formats(&conn, edition).expect("formats"),
        ["pdf"],
        "a sniff would have said zip",
    );
}

/// A file that cannot be stat'ed, gone or on a share that is down, keeps the
/// format the scan stored: nothing says it changed.
#[test]
fn a_volume_gone_from_disk_reports_the_format_the_scan_stored() {
    let (_tree, library, volume) =
        library_with_one_volume("proposals-formats-gone", "Series v01.cbz", |at| {
            crate::scan::tests::write_archive(at, 3);
        });
    let conn = library.conn();
    let edition = the_one_members_edition(&conn).expect("the archive is a member");
    std::fs::remove_file(&volume).expect("removes the volume");

    assert_eq!(
        proposals::edition_formats(&conn, edition).expect("formats"),
        ["zip"],
    );
}

/// A folder is compared by the size and mtime the scan wrote for a folder, so
/// one that has not moved is answered from its row, here made to say `pdf`,
/// and one whose mtime has moved is a `dir` by what it is now.
///
/// The mtime is moved by hand and nothing is added to the folder: a folder's
/// size follows its entries on one filesystem and not on another, so a page
/// added would leave it to the filesystem which half of the stamp moved.
#[test]
fn a_folder_reports_the_stored_format_until_it_moves_and_dir_after() {
    let (_tree, library, volume) =
        library_with_one_volume("proposals-formats-folder", "Series v01", |at| {
            write_pages(at, 3);
        });
    let conn = library.conn();
    let edition = the_one_members_edition(&conn).expect("the folder is a member");
    assert_eq!(
        proposals::edition_formats(&conn, edition).expect("formats"),
        ["dir"],
        "the fixture: scanned as a folder of pages",
    );
    store_format(&conn, "Series/Series v01", "pdf");
    assert_eq!(
        proposals::edition_formats(&conn, edition).expect("formats"),
        ["pdf"],
        "a folder as the scan saw it is answered from its row",
    );

    let seen = std::fs::metadata(&volume)
        .and_then(|meta| meta.modified())
        .expect("the folder's mtime");
    set_mtime(&volume, seen + std::time::Duration::from_secs(60));

    assert_eq!(
        proposals::edition_formats(&conn, edition).expect("formats"),
        ["dir"],
        "a folder that moved since the scan is what it is now",
    );
}

/// Set the mtime of the file or folder at `path`, which a test does by hand
/// wherever the mtime is what it is about: what a rewrite does to one is the
/// filesystem's and the clock's to say.
fn set_mtime(path: &std::path::Path, to: std::time::SystemTime) {
    std::fs::File::open(path)
        .and_then(|file| file.set_modified(to))
        .expect("sets the mtime");
}

/// The stamp is a size and an mtime, and either alone says a file moved. Here
/// the size stays: the zip is replaced by a RAR of its own length, and the
/// mtime is first put back to the one the scan saw, where the row answers for
/// bytes it does not describe, and then moved a minute on.
#[test]
fn a_volume_rewritten_at_its_own_length_is_sniffed_once_its_mtime_moves() {
    let (_tree, library, volume) =
        library_with_one_volume("proposals-formats-same-length", "Series v01.cbz", |at| {
            crate::scan::tests::write_archive(at, 3);
        });
    let conn = library.conn();
    let edition = the_one_members_edition(&conn).expect("the archive is a member");
    let scanned = std::fs::metadata(&volume).expect("the volume as the scan saw it");
    let seen = scanned.modified().expect("the mtime the scan saw");
    let length = usize::try_from(scanned.len()).expect("a fixture archive is a few hundred bytes");
    assert!(
        RAR_BYTES.len() <= length,
        "the fixture: the zip is no shorter than a RAR's header"
    );
    let mut rar = RAR_BYTES.to_vec();
    rar.resize(length, 0);
    std::fs::write(&volume, &rar).expect("replaces the zip with a RAR of its length");

    set_mtime(&volume, seen);
    assert_eq!(
        proposals::edition_formats(&conn, edition).expect("formats"),
        ["zip"],
        "the size and the mtime the scan saw, so the row answers",
    );

    set_mtime(&volume, seen + std::time::Duration::from_secs(60));
    assert_eq!(
        proposals::edition_formats(&conn, edition).expect("formats"),
        ["rar"],
        "the mtime alone moved, and that is a file the row does not describe",
    );
}

/// The other half of the stamp. Here the mtime stays: the zip is replaced by a
/// RAR of another length and its mtime put back to the one the scan saw, so
/// the size alone says the file moved, whichever second the rewrite fell in.
#[test]
fn a_volume_rewritten_at_another_length_is_sniffed_though_its_mtime_is_the_scans() {
    let (_tree, library, volume) =
        library_with_one_volume("proposals-formats-same-mtime", "Series v01.cbz", |at| {
            crate::scan::tests::write_archive(at, 3);
        });
    let conn = library.conn();
    let edition = the_one_members_edition(&conn).expect("the archive is a member");
    let scanned = std::fs::metadata(&volume).expect("the volume as the scan saw it");
    let seen = scanned.modified().expect("the mtime the scan saw");
    assert_ne!(
        usize::try_from(scanned.len()).expect("a fixture archive is a few hundred bytes"),
        RAR_BYTES.len(),
        "the fixture: the RAR is not the zip's length"
    );
    std::fs::write(&volume, RAR_BYTES).expect("replaces the zip with a RAR of another length");
    set_mtime(&volume, seen);
    assert_eq!(
        std::fs::metadata(&volume)
            .and_then(|meta| meta.modified())
            .expect("the volume's mtime now"),
        seen,
        "the fixture: the mtime is the one the scan saw"
    );

    assert_eq!(
        proposals::edition_formats(&conn, edition).expect("formats"),
        ["rar"],
        "the size alone moved, and that is a file the row does not describe",
    );
}

/// A row that holds no format says nothing while its file is as the scan saw
/// it, and is no reason to pass over the file once it has moved: the bytes
/// there now may name one.
#[test]
fn a_volume_whose_row_holds_no_format_is_sniffed_once_it_moves() {
    let (_tree, library, volume) =
        library_with_one_volume("proposals-formats-no-token", "Series v01.cbz", |at| {
            crate::scan::tests::write_archive(at, 3);
        });
    let conn = library.conn();
    let edition = the_one_members_edition(&conn).expect("the archive is a member");
    let changed = conn
        .execute(
            "UPDATE scan_entry SET format = NULL WHERE rel_path = 'Series/Series v01.cbz'",
            [],
        )
        .expect("empties the token");
    assert_eq!(changed, 1, "the fixture: one entry for the volume");
    assert_eq!(
        proposals::edition_formats(&conn, edition).expect("formats"),
        Vec::<String>::new(),
        "the file is as the scan saw it, and the scan's row names no format",
    );

    std::fs::write(&volume, RAR_BYTES).expect("replaces the zip with a RAR");
    assert_eq!(
        proposals::edition_formats(&conn, edition).expect("formats"),
        ["rar"],
        "a moved file is sniffed whether or not its row held a format",
    );
}

/// A moved file that cannot be read contributes nothing: its row describes
/// another file, and no bytes say what this one is.
///
/// The file is a link to `/proc/self/mem`: a regular file to `stat`, and one
/// whose read fails for every user. Mode bits would not do it, since they do
/// not stop root, which CI runs as.
#[test]
fn a_moved_volume_that_cannot_be_read_reports_no_format() {
    let (_tree, library, volume) =
        library_with_one_volume("proposals-formats-unreadable", "Series v01.cbz", |at| {
            crate::scan::tests::write_archive(at, 3);
        });
    let conn = library.conn();
    let edition = the_one_members_edition(&conn).expect("the archive is a member");

    std::fs::remove_file(&volume).expect("removes the volume");
    std::os::unix::fs::symlink("/proc/self/mem", &volume).expect("links the unreadable file");
    assert!(
        std::fs::metadata(&volume).is_ok_and(|meta| meta.is_file()),
        "the fixture: a regular file to stat"
    );
    assert!(
        taiyaku_read::sniff(&volume).is_err(),
        "the fixture: a file whose read fails"
    );

    assert_eq!(
        proposals::edition_formats(&conn, edition).expect("formats"),
        Vec::<String>::new(),
        "the row's zip is of a file that is no longer there",
    );
}

/// Only a regular file or a folder is sniffed. `stat` succeeds on a FIFO, and
/// an open of one waits for a writer, so a moved path that is one contributes
/// nothing and is not opened.
///
/// The test holds the FIFO open itself with a RAR's header in it, so an open
/// here returns at once and reads `rar`, where with no writer it would wait
/// for one.
#[test]
fn a_fifo_where_a_volume_was_is_not_opened() {
    use std::io::Write as _;

    let (_tree, library, volume) =
        library_with_one_volume("proposals-formats-fifo", "Series v01.cbz", |at| {
            crate::scan::tests::write_archive(at, 3);
        });
    let conn = library.conn();
    let edition = the_one_members_edition(&conn).expect("the archive is a member");

    std::fs::remove_file(&volume).expect("removes the volume");
    let made = std::process::Command::new("mkfifo")
        .arg(&volume)
        .status()
        .expect("runs mkfifo");
    assert!(made.success(), "the fixture: mkfifo makes the FIFO");
    // Opened for both, which waits for nobody; opened for one, it would wait
    // for the other.
    let mut held = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&volume)
        .expect("opens the FIFO");
    let mut header = RAR_BYTES.to_vec();
    header.resize(1024, 0);
    held.write_all(&header)
        .expect("writes what a sniff would read");

    assert_eq!(
        proposals::edition_formats(&conn, edition).expect("formats"),
        Vec::<String>::new(),
        "a FIFO at the member's path was opened and read",
    );
}

/// The formats are ascending whichever member holds which: the two members'
/// rows are made to say each other's format, and the answer does not move.
#[test]
fn edition_formats_are_ascending_whichever_member_holds_which_format() {
    let (_tree, library) = library_whose_volume_one_is_a_directory_and_an_archive();
    let conn = library.conn();
    let edition: i64 = conn
        .query_row(
            "SELECT edition_id FROM member WHERE path LIKE '%v01.cbz'",
            [],
            |r| r.get(0),
        )
        .expect("the archive's edition");
    let formats = || proposals::edition_formats(&conn, edition).expect("formats");
    assert_eq!(formats(), ["dir", "zip"], "the fixture: a folder and a zip");

    store_format(
        &conn,
        "Go-Toubun no Hanayome/Go-Toubun no Hanayome - v01",
        "zip",
    );
    store_format(
        &conn,
        "Go-Toubun no Hanayome/Go-Toubun no Hanayome - v01.cbz",
        "dir",
    );

    assert_eq!(formats(), ["dir", "zip"]);
}

#[test]
fn directories_for_entries_sitting_directly_under_the_root_carry_no_trailing_slash() {
    // `material_of`'s `dir.is_empty()` branch: an entry sitting directly
    // under the root has a `parent_of` of `""`, and the field must carry the
    // root's own path, without the trailing slash that `{root}/{dir}` would
    // add. Same tree shape as
    // `accept_refuses_when_the_subjects_files_sit_directly_in_the_root`: the
    // subject's volume sits at the root's top level rather than under a
    // series-named subdirectory.
    let dir = crate::tests::tempdir("proposals-evidence-root-level");
    write_pages(
        &dir.path()
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01"),
        2,
    );
    write_pages(
        &dir.path()
            .join("Go-Toubun no Hanayome - Full Color Edition - v01"),
        3,
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, dir.path()).expect("add root");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("derive");
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "edition-of")
        .expect("an edition-of proposal");

    let ev = proposals::evidence(&conn, proposal.id).expect("evidence");

    assert_eq!(
        ev.subject.directories,
        vec![dir.path().to_string_lossy().into_owned()],
        "the root's own path, verbatim — not the root with a trailing slash \
         appended for an entry that sits directly under it",
    );
}

#[test]
fn evidence_performs_exactly_one_sweep() {
    // The one-sweep guarantee `evidence`'s own comment claims. This counts real
    // calls to `derive::rel_paths_by_series` via a `#[cfg(test)]` thread-local
    // counter — no `rusqlite::trace` feature needed, and thread-local rather
    // than a shared `static` so a concurrently running test's own
    // `evidence`/`preview` call on another thread cannot land inside this
    // test's reset-then-read window.
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);

    derive::REL_PATHS_BY_SERIES_CALLS.with(|c| c.set(0));
    let _ev = proposals::evidence(&conn, proposal.id).expect("evidence");
    assert_eq!(
        derive::REL_PATHS_BY_SERIES_CALLS.with(std::cell::Cell::get),
        1,
        "one expanded row, one sweep for both sides and the rule beside them",
    );
}

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

/// **O114, through a proposal.** `proposals` finds a series by `sort_title`
/// wherever it starts from a name the rules resolved — `raise_attach_chapters`,
/// the evidence sweep, a candidate rule's reach. A correction to a series'
/// title leaves a proposal about the same series: its evidence names the
/// same directory, and the next scan does not ask the same question about a
/// second series.
#[test]
fn a_title_correction_leaves_an_attach_chapters_proposal_about_the_same_series() {
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, tree.path()).expect("add root");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("derive");
    let open = proposals::open(&conn, 0.0).expect("open");
    assert_eq!(open.len(), 1, "{open:?}");
    let (id, subject) = (open[0].id, open[0].subject_id);
    let before = proposals::evidence(&conn, id).expect("evidence").subject;
    assert!(
        !before.directories.is_empty(),
        "the fixture's chapters directory is where the subject's material sits: {before:?}"
    );

    crate::librarydoc::set_series_titles(
        &conn,
        subject,
        &[crate::librarydoc::SeriesTitle {
            title: "かぐや様は告らせたい".to_string(),
            kind: crate::librarydoc::TitleKind::Native,
            primary: true,
        }],
    )
    .expect("corrects the subject's title");

    let after = proposals::evidence(&conn, id).expect("evidence").subject;
    assert_eq!(
        after.directories, before.directories,
        "the correction changed nothing on disk, so the subject's material sits \
         where it did: {after:?}"
    );

    scan::scan_root(&conn, &root).expect("rescan");
    derive::derive(&conn).expect("re-derive");
    let open: Vec<(i64, String, i64)> = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .map(|p| (p.id, p.kind, p.subject_id))
        .collect();
    assert_eq!(
        open,
        vec![(id, "attach-chapters".to_string(), subject)],
        "one proposal, the same one, still about series {subject}"
    );
}

// --- Part F1: proposals across libraries ------------------------------------

/// Files a directory under the one library every directory filed this way shares.
const TOGETHER: scan::InLibrary<'static> = scan::InLibrary::Named("Manga");
/// Files a directory under a library of its own.
const APART: scan::InLibrary<'static> = scan::InLibrary::Own;

/// One directory holding volume 1 of each of `titles`, each under a
/// subdirectory of its own name.
fn dir_of(label: &str, titles: &[&str]) -> crate::tests::TempDir {
    let dir = crate::tests::tempdir(label);
    for (n, title) in (2_usize..).zip(titles) {
        write_pages(&dir.path().join(title).join(format!("{title} - v01")), n);
    }
    dir
}

/// A store holding `dirs`, each filed as it says and scanned in the order
/// given, after one derive over all of them. Answers the root each directory
/// became, in that order.
fn derived_from(
    dirs: &[(&crate::tests::TempDir, scan::InLibrary<'_>)],
) -> (Library, Vec<scan::Root>) {
    let library = Library::open_in_memory().expect("open");
    let mut roots = Vec::new();
    {
        let conn = library.conn();
        for (dir, filed) in dirs {
            let root = scan::add_root_in(&conn, dir.path(), *filed).expect("add root");
            scan::scan_root(&conn, &root).expect("scan");
            roots.push(root);
        }
        derive::derive(&conn).expect("derive");
    }
    (library, roots)
}

/// The series called `title` that any directory files its material under:
/// the oldest of that title in the store, as `derive::reconcile_series` finds
/// it, whichever library holds the directory.
fn series_in(conn: &rusqlite::Connection, title: &str) -> i64 {
    conn.query_row(
        "SELECT id FROM series WHERE sort_title = ?1 ORDER BY id LIMIT 1",
        [title],
        |r| r.get(0),
    )
    .expect("a series of that title")
}

/// Every open proposal as `(kind, subject, target)`, ascending.
fn open_pairs(conn: &rusqlite::Connection) -> Vec<(String, i64, Option<i64>)> {
    let mut pairs: Vec<_> = proposals::open(conn, 0.0)
        .expect("open")
        .into_iter()
        .map(|p| (p.kind, p.subject_id, p.target_id))
        .collect();
    pairs.sort();
    pairs
}

/// [`open_pairs`] of `kind` alone, as `(subject, target)`. A fixture holding
/// one title in two libraries holds two series of that title, each recording
/// the title the other is called, so `same-series` pairs them; a test about
/// another kind asks about that kind.
fn open_of_kind(conn: &rusqlite::Connection, kind: &str) -> Vec<(i64, Option<i64>)> {
    open_pairs(conn)
        .into_iter()
        .filter(|(k, ..)| k == kind)
        .map(|(_, subject, target)| (subject, target))
        .collect()
}

#[test]
fn an_edition_of_is_raised_within_a_library_and_across_two() {
    let base = dir_of("proposals-lib-edition-base", &["Go-Toubun no Hanayome"]);
    let colour = dir_of(
        "proposals-lib-edition-colour",
        &["Go-Toubun no Hanayome - Full Color Edition"],
    );
    // What is open, and the one pair an `edition-of` would name.
    let raised = |filed: scan::InLibrary<'static>| {
        let (library, _) = derived_from(&[(&base, filed), (&colour, filed)]);
        let conn = library.conn();
        let pair = vec![(
            "edition-of".to_string(),
            series_in(&conn, "Go-Toubun no Hanayome - Full Color Edition"),
            Some(series_in(&conn, "Go-Toubun no Hanayome")),
        )];
        (open_pairs(&conn), pair)
    };

    let (open, pair) = raised(TOGETHER);
    assert_eq!(
        open, pair,
        "two directories of one library hold the pair, and it is proposed",
    );
    let (open, pair) = raised(APART);
    assert_eq!(
        open, pair,
        "the same two directories as two libraries: the pair is proposed across them",
    );
}

#[test]
fn a_same_series_is_raised_within_a_library_and_across_two() {
    let established = dir_of("proposals-lib-same-established", &["Kaguya-sama"]);
    let scanned = dir_of("proposals-lib-same-scanned", &["Kaguya"]);
    // What is open once `Kaguya-sama` has recorded `Kaguya` as a title of its
    // own, with the series the proposal would be about and the one it would
    // join.
    let raised = |filed: scan::InLibrary<'static>| {
        let (library, _) = derived_from(&[(&established, filed), (&scanned, filed)]);
        let conn = library.conn();
        let owner = series_in(&conn, "Kaguya-sama");
        let claimant = series_in(&conn, "Kaguya");
        conn.execute(
            "INSERT INTO series_title (series_id, title, hand) VALUES (?1, 'Kaguya', 1)",
            [owner],
        )
        .expect("a title recorded by hand");
        proposals::raise(&conn).expect("raise");
        (open_pairs(&conn), claimant, owner)
    };

    let (open, claimant, owner) = raised(TOGETHER);
    assert_eq!(
        open,
        vec![("same-series".to_string(), claimant, Some(owner))],
        "two directories of one library: the series named `Kaguya` is offered the one that \
         records the title",
    );

    let (open, claimant, owner) = raised(APART);
    assert_eq!(
        open,
        vec![("same-series".to_string(), claimant, Some(owner))],
        "the same two directories as two libraries: a title recorded in one names the series \
         of the other",
    );
}

#[test]
fn an_edition_of_across_two_libraries_is_accepted_as_a_rule_for_the_subjects_one_directory() {
    let base = dir_of("proposals-lib-cross-base", &["Foo"]);
    let colour = dir_of("proposals-lib-cross-colour", &["Foo - Color"]);
    let (library, dirs) = derived_from(&[(&base, APART), (&colour, APART)]);
    let conn = library.conn();
    assert_ne!(
        dirs[0].libraries[0], dirs[1].libraries[0],
        "the two directories are two libraries",
    );
    let subject = series_in(&conn, "Foo - Color");
    let target = series_in(&conn, "Foo");
    assert_eq!(
        open_pairs(&conn),
        vec![("edition-of".to_string(), subject, Some(target))],
        "`Foo - Color` in one library is offered as an edition of `Foo` in the other",
    );
    let id = proposals::open(&conn, 0.0).expect("open").remove(0).id;

    let previewed = proposals::preview(&conn, id).expect("a rule is offered");
    assert_eq!(
        (
            previewed.root_id,
            previewed.pattern.as_str(),
            previewed.set_series.as_deref(),
        ),
        (Some(dirs[1].id), "Foo - Color/", Some("Foo")),
        "scoped to the subject's one directory, and naming the target in the other",
    );
    let ev = proposals::evidence(&conn, id).expect("evidence");
    assert_eq!(
        ev.target.expect("the target side").directories,
        vec![dirs[0].path.join("Foo").to_string_lossy().into_owned()],
        "the target's material is read from the other library's directory",
    );
    assert_eq!(
        ev.rule.as_ref(),
        Some(&previewed),
        "and the expanded row offers the same rule",
    );

    let rule_id = proposals::accept(&conn, id).expect("accepting writes that rule");
    let written: (Option<i64>, String, Option<String>) = conn
        .query_row(
            "SELECT root_id, pattern, set_series FROM rule WHERE id = ?1",
            [rule_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("the written rule");
    assert_eq!(
        written,
        (
            Some(dirs[1].id),
            "Foo - Color/".to_string(),
            Some("Foo".to_string())
        ),
        "the rule written is scoped to the subject's one directory",
    );
    assert_eq!(
        (state_of(&conn, id), rule_of(&conn, id)),
        ("accepted".to_string(), Some(rule_id)),
    );
}

#[test]
fn a_subject_in_directories_of_two_libraries_is_refused_in_common_directory_patterns_words() {
    let colour = "Foo - Color";
    let one = dir_of("proposals-lib-spread-one", &["Foo", colour]);
    let other = dir_of("proposals-lib-spread-other", &[colour]);
    let (library, dirs) = derived_from(&[(&one, APART), (&other, APART)]);
    let conn = library.conn();
    let asked = open_of_kind(&conn, "edition-of");
    assert!(
        !asked.is_empty(),
        "the subject is offered as an edition of `Foo`"
    );

    for (subject, _target) in asked {
        let id = proposals::open(&conn, 0.0)
            .expect("open")
            .into_iter()
            .find(|p| p.kind == "edition-of" && p.subject_id == subject)
            .expect("the proposal")
            .id;
        let refusal = format!(
            "proposal {id}'s subject sits under `{colour}/` in more than one root, so accepting \
             it cannot be scoped to a single root. Its roots are: {}, {}",
            dirs[0].path.to_string_lossy(),
            dirs[1].path.to_string_lossy(),
        );
        assert_eq!(
            proposals::preview(&conn, id).expect_err("no rule is offered"),
            refusal,
        );
        assert_eq!(
            proposals::accept(&conn, id).expect_err("and none is written"),
            refusal,
        );
        assert!(
            proposals::evidence(&conn, id)
                .expect("the row still expands")
                .rule
                .is_none(),
            "with no rule to accept",
        );
        assert_eq!(
            (state_of(&conn, id), rule_of(&conn, id)),
            ("open".to_string(), None),
            "the proposal is still open, and holds no rule",
        );
    }
    assert_eq!(accepted_rules(&conn), 0, "no rule was written");
}

#[test]
fn attach_chapters_names_the_series_of_that_title_wherever_its_volumes_are() {
    let volumes = dir_of("proposals-lib-chapters-volumes", &["Kaguya-sama"]);
    let chapters = crate::tests::tempdir("proposals-lib-chapters-chapters");
    for chapter in ["Kaguya-sama - c001", "Kaguya-sama - c002"] {
        write_pages(&chapters.path().join("Kaguya-sama").join(chapter), 2);
    }
    let (library, _) = derived_from(&[(&volumes, APART), (&chapters, APART)]);
    let conn = library.conn();
    let with_volumes = series_in(&conn, "Kaguya-sama");
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM series WHERE sort_title = 'Kaguya-sama'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .expect("counts"),
        1,
        "the fixture: one series of that title across both libraries",
    );

    assert_eq!(
        open_of_kind(&conn, "attach-chapters"),
        vec![(with_volumes, None)],
        "the chapters in one library are a question about the series of that title whose \
         volumes are in the other",
    );
}

#[test]
fn chapter_directories_are_counted_within_a_library_and_across_two() {
    let dir = |label: &str| {
        let top = crate::tests::tempdir(label);
        write_pages(
            &top.path().join("Kaguya-sama").join("Kaguya-sama - c001"),
            2,
        );
        top
    };
    let (one, other) = (
        dir("proposals-lib-chapter-count-one"),
        dir("proposals-lib-chapter-count-other"),
    );
    // Each open `attach-chapters` proposal's subject, and whether it counts
    // one directory or two.
    let counted = |conn: &rusqlite::Connection| -> Vec<(i64, bool, bool)> {
        let mut counted: Vec<_> = proposals::open(conn, 0.0)
            .expect("open")
            .into_iter()
            .filter(|p| p.kind == "attach-chapters")
            .map(|p| {
                (
                    p.subject_id,
                    p.reason.contains("holds one directory"),
                    p.reason.contains("holds 2 directories"),
                )
            })
            .collect();
        counted.sort_unstable();
        counted
    };

    let (together, _) = derived_from(&[(&one, TOGETHER), (&other, TOGETHER)]);
    {
        let conn = together.conn();
        assert_eq!(
            counted(&conn),
            vec![(series_in(&conn, "Kaguya-sama"), false, true)],
            "two directories of one library: one series, asked about both directories",
        );
    }

    let (apart, _) = derived_from(&[(&one, APART), (&other, APART)]);
    let conn = apart.conn();
    assert_eq!(
        counted(&conn),
        vec![(series_in(&conn, "Kaguya-sama"), false, true)],
        "the same two directories as two libraries: one series, the older of that title, asked \
         about both directories",
    );
}

#[test]
fn accepting_reads_the_subjects_directories_wherever_they_are() {
    let colour = "Base - Full Color";
    let other = dir_of("proposals-lib-accept-other", &[colour]);
    let own = dir_of("proposals-lib-accept-own", &["Base", colour]);
    let (library, dirs) = derived_from(&[(&other, APART), (&own, APART)]);
    let conn = library.conn();
    let subject = series_in(&conn, colour);
    let target = series_in(&conn, "Base");
    assert!(
        open_of_kind(&conn, "edition-of").contains(&(subject, Some(target))),
        "the second library's colour series is offered as an edition of `Base`",
    );
    let id = proposals::open(&conn, 0.0)
        .expect("open")
        .into_iter()
        .find(|p| p.kind == "edition-of" && p.subject_id == subject)
        .expect("that proposal")
        .id;

    let ev = proposals::evidence(&conn, id).expect("evidence");
    let mut both: Vec<String> = dirs
        .iter()
        .map(|root| root.path.join(colour).to_string_lossy().into_owned())
        .collect();
    both.sort();
    assert_eq!(
        ev.subject.directories, both,
        "the subject's material is every directory of its title, the other library's too",
    );
    assert!(
        ev.rule.is_none(),
        "so no rule for one directory is offered: {:?}",
        ev.rule
    );
    let refused = proposals::preview(&conn, id).expect_err("preview refuses it too");
    assert!(
        refused.contains("in more than one root"),
        "refused because the subject sits under two roots: {refused}",
    );
}

#[test]
fn a_dry_run_counts_a_title_two_series_hold_as_its_oldest_series() {
    let one = dir_of("proposals-lib-reach-one", &["Series"]);
    let other = dir_of("proposals-lib-reach-other", &["Series"]);
    let (library, _) = derived_from(&[(&one, APART), (&other, APART)]);
    let conn = library.conn();
    let older = series_in(&conn, "Series");
    conn.execute("INSERT INTO series (sort_title) VALUES ('Series')", [])
        .expect("a younger series of that title, as a hand document can mint");
    let newer = conn.last_insert_rowid();
    assert!(
        older < newer,
        "two series of that title: {older} then {newer}"
    );
    let rule = proposals::ProposedRule {
        root_id: None,
        kind: "prefix".to_string(),
        pattern: "Series/".to_string(),
        match_on: "path".to_string(),
        set_series: Some("Elsewhere".to_string()),
        set_edition_key: None,
        set_kind: None,
        set_language: None,
        note: "a candidate for every directory".to_string(),
    };

    let reach = proposals::series_reached_by(&conn, &rule).expect("the dry run");

    assert_eq!(
        reach.series.keys().copied().collect::<Vec<_>>(),
        vec![older],
        "both directories' entries are reached, and are counted against the oldest `Series` \
         in the store",
    );
}

#[test]
fn a_widened_rule_answers_another_proposal_within_a_library_and_across_two() {
    let base = dir_of("proposals-lib-answers-base", &["Base"]);
    let rest = dir_of(
        "proposals-lib-answers-rest",
        &[
            "Kaguya",
            "Kaguya - Full Color Edition",
            "Base - Full Color Edition",
        ],
    );
    // The `Kaguya` colour edition's proposal, and its rule widened to every
    // colour edition in every directory.
    let widened = |conn: &rusqlite::Connection| {
        let asked = proposal_whose_subject_is(conn, "Kaguya - Full Color Edition");
        let mut rule = proposals::preview(conn, asked).expect("preview");
        rule.kind = "regex".to_string();
        rule.pattern = "^(?<series>.+) - Full Color Edition/".to_string();
        rule.root_id = None;
        (asked, rule)
    };

    let (together, _dirs) = derived_from(&[(&base, TOGETHER), (&rest, TOGETHER)]);
    {
        let conn = together.conn();
        let (asked, rule) = widened(&conn);
        let within = proposal_whose_subject_is(&conn, "Base - Full Color Edition");
        let resolution = proposals::resolves(&conn, asked, &rule).expect("resolves");
        assert_eq!(
            (resolution.also_resolves, resolution.collateral),
            (vec![within], vec![]),
            "two directories of one library: the rule answers the `Base` pair too",
        );
    }

    let (apart, _dirs) = derived_from(&[(&base, APART), (&rest, APART)]);
    let conn = apart.conn();
    let (asked, rule) = widened(&conn);
    let across = proposal_whose_subject_is(&conn, "Base - Full Color Edition");

    let resolution = proposals::resolves(&conn, asked, &rule).expect("resolves");

    assert_eq!(
        (resolution.also_resolves, resolution.collateral),
        (vec![across], vec![]),
        "two libraries: the rule answers the `Base` pair across them too",
    );
    let rule_id = made(proposals::accept_with(&conn, asked, &rule));
    assert_eq!(
        (state_of(&conn, across), rule_of(&conn, across)),
        ("accepted".to_string(), Some(rule_id)),
        "and accepting it accepts the pair across the two with the same rule",
    );
}

#[test]
fn chapters_are_a_question_about_the_older_of_two_series_of_one_name_in_the_store() {
    let dir = crate::tests::tempdir("proposals-lib-oldest");
    write_pages(
        &dir.path().join("Kaguya-sama").join("Kaguya-sama - c001"),
        2,
    );
    write_pages(&dir.path().join("Other").join("Other - v01"), 3);
    let (library, _) = derived_from(&[(&dir, APART)]);
    let conn = library.conn();
    let scanned = series_in(&conn, "Kaguya-sama");
    // The subject of each open `attach-chapters` proposal, in id order.
    let asked_about = |conn: &rusqlite::Connection| -> Vec<i64> {
        let mut asked: Vec<_> = proposals::open(conn, 0.0)
            .expect("open")
            .into_iter()
            .filter(|p| p.kind == "attach-chapters")
            .map(|p| (p.id, p.subject_id))
            .collect();
        asked.sort_unstable();
        asked.into_iter().map(|(_id, subject)| subject).collect()
    };
    assert_eq!(asked_about(&conn), vec![scanned]);

    // The scanned series is given another title, and a volume is then filed
    // by hand under a series of the scanned name: a second series of that
    // `sort_title` in the store.
    crate::librarydoc::set_series_titles(
        &conn,
        scanned,
        &[crate::librarydoc::SeriesTitle {
            title: "かぐや様は告らせたい".to_string(),
            kind: crate::librarydoc::TitleKind::Native,
            primary: true,
        }],
    )
    .expect("corrects the scanned series' title");
    let volume: i64 = conn
        .query_row("SELECT id FROM work", [], |r| r.get(0))
        .expect("the one volume");
    let doc = crate::librarydoc::LibraryDoc::from_json(
        r#"{"taiyaku_library":1,"updated":"2026-10-06T00:00:00Z",
            "series":{"chosen":true,"titles":[{"title":"Kaguya-sama","kind":"romaji","primary":true}]}}"#,
    )
    .expect("parses");
    crate::librarydoc::apply(&conn, volume, &doc).expect("files the volume by hand");
    let by_hand: i64 = conn
        .query_row("SELECT series_id FROM work WHERE id = ?1", [volume], |r| {
            r.get(0)
        })
        .expect("the series the volume was filed under");
    let named: Vec<i64> = conn
        .prepare("SELECT id FROM series WHERE sort_title = 'Kaguya-sama' ORDER BY id")
        .expect("prepare")
        .query_map([], |r| r.get(0))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect");
    assert_eq!(
        named,
        vec![scanned, by_hand],
        "two series of the scanned name in the store, the scanned one the older",
    );

    derive::derive(&conn).expect("derive");

    assert_eq!(
        asked_about(&conn),
        vec![scanned],
        "still one question about the chapters, and about the older series",
    );
}
