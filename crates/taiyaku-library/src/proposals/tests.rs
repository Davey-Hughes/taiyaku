use crate::scan::tests::{fixture, write_pages};
use crate::{Library, derive, proposals, scan};

/// A library holding one series per title, **derived from a real scanned
/// tree** rather than from hand-inserted rows — one directory per title,
/// holding one volume.
///
/// **It used to insert `series` rows directly, and that stopped being
/// adequate on 2026-08-17.** Once `accept` derives its rule's pattern from
/// the paths of the entries that resolved to a series — and verifies the
/// rule matches something — a series with no `scan_entry` rows behind it has
/// nothing to accept, and refusing is the correct answer. A fixture that
/// cannot reach `accept`'s success path cannot test it.
///
/// The weaker version also hid a second thing: with no `work` rows anywhere,
/// every volume-count comparison was `0` against `0`, so the scoring tests
/// discriminated on vocabulary alone while appearing to exercise both
/// signals. Building from a scanned tree fixes both at once, which is the
/// argument for doing it here rather than per test — every series raised
/// from this helper now carries exactly one real volume, so the
/// equal-volume-count signal is genuinely evaluated rather than vacuously
/// true. It does not by itself discriminate between two series drawn from
/// here, since every one of them holds the same count: that is
/// `the_volume_count_signal_needs_volumes_to_count`'s job, via `score_for`.
///
/// Returns the `Library` rather than a bare `Connection`: `Library` owns the
/// connection behind a `Mutex` and there is no `into_connection`, deliberately
/// — widening this crate's public surface so four tests can hold a connection
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
    // **Belt-and-braces, not load-bearing.** `write_pages` now bakes its own
    // directory into every page's bytes (Task 5/8 review), so two volumes can
    // no longer collide on `content_key` no matter what page count either one
    // gets — varying the count here is not what prevents that any more, and
    // this loop would still be safe as a single fixed number. Kept anyway: it
    // is what turned up the collision in the first place, before the helper
    // itself was fixed, and a mix of counts is closer to a real library than
    // every volume being the same length.
    for (n, title) in (2_usize..).zip(titles) {
        write_pages(&dir.path.join(title).join(format!("{title} - v01")), n);
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &dir.path).expect("add root");
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
/// `library_with_series` above creates no `work` rows at all — which is
/// exactly why the volume-count half of §6's score went unexercised until
/// this helper existed to attach some. `content_key` and `slug` only need to
/// be unique within one call's database, so they are derived from the title
/// and volume rather than carrying any meaning `import.rs`'s own rows would.
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

/// A real tree reproducing the bug review found `accept` capable of: the
/// subject's own volume sits under a directory named nothing like its
/// title, `Publisher/`, resolved to its actual series purely by seeded rule
/// 4's filename capture (rule 1's path-based guess, `series = "Publisher"`,
/// is overridden per file) — exactly as it would be under a real, published
/// layout.
///
/// **The base series lives under its own directory, deliberately, not also
/// under `Publisher/`.** An earlier version of this fixture put both under
/// one directory and reproduced a *different* bug by accident: once `accept`
/// also refuses when a directory holds more than the subject (review's
/// "matches only the subject" fix), that layout can never be accepted at
/// all, so it stopped being able to test this one.
/// `library_with_an_unrelated_series_sharing_the_subjects_directory` is the
/// fixture that puts two series under one directory on purpose, for that
/// other bug.
///
/// **`library_with_two_suffixed_series` cannot stand in for this**, even
/// though it too scans a real tree now. Its layout is flat — one series, one
/// top-level directory named after it — so the subject's `sort_title` and
/// its entries' common directory happen to coincide, which is exactly the
/// coincidence that let the old, title-derived pattern pass unnoticed. Only
/// a directory named unlike the subject exposes the bug, which is the
/// entire reason this helper exists rather than being folded into that one.
///
/// Returns the `TempDir` too: it must outlive every use of `library`, or the
/// tree it holds is removed while `scan_entry` still names paths under it.
fn library_scanned_with_the_subject_under_an_unrelated_directory()
-> (crate::tests::TempDir, Library) {
    let dir = crate::tests::tempdir("proposals-accept-unrelated-dir");
    write_pages(
        &dir.path
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01"),
        2,
    );
    write_pages(
        &dir.path
            .join("Publisher")
            .join("Go-Toubun no Hanayome - Full Color Edition - v01"),
        3,
    );
    let library = Library::open_in_memory().expect("open");
    {
        let conn = library.conn();
        let root = scan::add_root(&conn, &dir.path).expect("add root");
        scan::scan_root(&conn, &root).expect("scan");
        derive::derive(&conn).expect("derive");
    }
    (dir, library)
}

/// A real tree matching review's own measured example: one publisher
/// directory holding the two related series an `edition-of` accept is about
/// (the base and its colour sibling) and one entirely unrelated series,
/// "Chainsaw Man", with two volumes of its own.
fn library_with_an_unrelated_series_sharing_the_subjects_directory()
-> (crate::tests::TempDir, Library) {
    let dir = crate::tests::tempdir("proposals-accept-swallow");
    let publisher = dir.path.join("Publisher");
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
        let root = scan::add_root(&conn, &dir.path).expect("add root");
        scan::scan_root(&conn, &root).expect("scan");
        derive::derive(&conn).expect("derive");
    }
    (dir, library)
}

/// Two roots that happen to share a directory *name*, `Publisher/`, holding
/// completely unrelated material — the shape I6 exists for. Root A holds the
/// same `edition-of` pair `library_scanned_with_the_subject_under_an_
/// unrelated_directory` does; root B holds a series with no relation to
/// either, under a directory that merely spells the same as root A's.
///
/// Returns both `TempDir`s: each must outlive every use of `library`.
fn library_with_two_roots_sharing_a_directory_name()
-> (crate::tests::TempDir, crate::tests::TempDir, Library) {
    let a = crate::tests::tempdir("proposals-accept-two-roots-a");
    write_pages(
        &a.path
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01"),
        2,
    );
    write_pages(
        &a.path
            .join("Publisher")
            .join("Go-Toubun no Hanayome - Full Color Edition - v01"),
        3,
    );
    let b = crate::tests::tempdir("proposals-accept-two-roots-b");
    write_pages(&b.path.join("Publisher").join("Something Else - v01"), 4);

    let library = Library::open_in_memory().expect("open");
    {
        let conn = library.conn();
        let root_a = scan::add_root(&conn, &a.path).expect("add root a");
        scan::scan_root(&conn, &root_a).expect("scan a");
        let root_b = scan::add_root(&conn, &b.path).expect("add root b");
        scan::scan_root(&conn, &root_b).expect("scan b");
        derive::derive(&conn).expect("derive");
    }
    (a, b, library)
}

#[test]
fn a_dismissed_proposal_stays_dismissed_across_a_rescan() {
    // §6's first argument. Without this, every rescan re-raises the same 37
    // suffixed directories forever and the person stops reading them.
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("first");
    let first = proposals::open(&conn, 0.0).expect("open");
    assert_eq!(first.len(), 1);
    proposals::dismiss(&conn, first[0].id).expect("dismiss");

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

    proposals::unaccept(&conn, proposal.id).expect("unaccept");

    let rules: i64 = conn
        .query_row("SELECT count(*) FROM rule WHERE id = ?1", [rule_id], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(rules, 0, "the rule went with the acceptance");
    assert_eq!(proposals::open(&conn, 0.0).expect("open").len(), 1);
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
    // A real scanned tree, not bare `series` rows: `accept` now derives its
    // pattern from `scan_entry` (review finding 1), so a subject with no
    // facts behind it has nothing to derive one from and correctly refuses —
    // see `library_with_series`'s own doc. Built by hand rather than through
    // that helper because this test needs a *known alt title* recorded after
    // the scan, which no title in a plain `titles: &[&str]` list can express.
    let dir = crate::tests::tempdir("proposals-same-series");
    // Different page counts — belt-and-braces, not required; see
    // `library_with_series`'s doc.
    write_pages(
        &dir.path
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01"),
        2,
    );
    write_pages(&dir.path.join("五等分の花嫁").join("五等分の花嫁 - v01"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &dir.path).expect("add root");
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
    // Two things Task 7 found that land here. First: a chapter-organised
    // directory produces no work and stays visibly unattached, which is what
    // `proposals::raise` — called from the end of `derive::derive` — turns
    // into the inbox entry. Second: accepting it cannot produce a work either
    // (a `prefix` rule sets no volume), so accept refuses this kind by name
    // rather than silently doing nothing. `scan::tests::fixture()` holds
    // exactly one such directory, "Kaguya-sama - Fan Translated".
    let tree = fixture();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &tree.path).expect("add root");
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

#[test]
fn accept_derives_the_pattern_from_scan_entry_not_the_subject_title() {
    // Review finding 1, reproducing the exact scenario measured: an earlier
    // version derived the pattern from the subject's own `sort_title`,
    // reasoning that seeded rule 1 resolves a series from the first path
    // component — false whenever a higher-ordinal rule decides it first.
    // Seeded rule 4 exists precisely because a filename sometimes repeats the
    // series where the directory does not, and the colour volume here sits
    // under `Publisher/` with its series decided by rule 4 alone. The old,
    // title-derived pattern (`Go-Toubun no Hanayome - Full Color Edition/`)
    // would have matched nothing, since the real path begins `Publisher/`.
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
    // Critical: the first version of finding 1's fix asked "does this match
    // something", not "does this match ONLY the subject". Measured, on the
    // layout this fixture reproduces exactly: accepting the colour proposal
    // would have written `Publisher/`, which also matches Chainsaw Man's two
    // volumes — silently moving them into the accepted series' page
    // sequence, and `write_work`'s own doc records that member order is page
    // order and every alignment offset is computed from it.
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
    // Important: `set_series` alone makes the accepted directory's material
    // extra pages of the base edition rather than a second edition of the
    // work — measured: one `raw` edition per work holding both volumes'
    // members concatenated, base then colour. `set_edition_key` is what
    // keeps them two editions of one work instead of one edition of two.
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
    // Minor 3: a suffix with no letters or digits slugifies to `""`, and
    // `edition.key` is `TEXT NOT NULL` — SQLite takes an empty string
    // happily, which is worse than refusing.
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
    // Minor 4: `strip_prefix` used to fall back to `None` silently, which
    // would restore Important 1's own bug — `set_series` alone, no
    // `set_edition_key` — the moment a target is renamed between raise and
    // accept. Simulated directly on the `series` row, since a rename between
    // two calls is otherwise invisible from outside this test.
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
    // Minor: the post-write verification branch — a pattern derived
    // correctly but matching nothing once checked against the raw facts —
    // had no test. Reachable when the subject's own entries sit directly
    // under the root: their common "directory" is the empty string, so the
    // derived pattern is `/`, and no real `rel_path` ever begins with one.
    let dir = crate::tests::tempdir("proposals-accept-root-level");
    write_pages(
        &dir.path
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01"),
        2,
    );
    write_pages(
        &dir.path
            .join("Go-Toubun no Hanayome - Full Color Edition - v01"),
        3,
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &dir.path).expect("add root");
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
    // The other half of finding 1: when a single common directory does not
    // exist, `accept` must refuse by name rather than pick one arbitrarily
    // and risk writing exactly the "matches nothing" rule this fix exists to
    // prevent for the paths it does not cover.
    let dir = crate::tests::tempdir("proposals-accept-split");
    // Different page counts — belt-and-braces, not required now that
    // `write_pages` cannot collide; see `library_with_series`'s doc. Was
    // never load-bearing for this test's own assertions either way, which
    // read `scan_entry` rather than `work`.
    write_pages(
        &dir.path
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01"),
        2,
    );
    write_pages(
        &dir.path
            .join("PublisherA")
            .join("Go-Toubun no Hanayome - Full Color Edition - v01"),
        3,
    );
    write_pages(
        &dir.path
            .join("PublisherB")
            .join("Go-Toubun no Hanayome - Full Color Edition - v02"),
        4,
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &dir.path).expect("add root");
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
    // Review finding 2. Without a state guard, a second `accept` writes a
    // second rule and overwrites `created_rule_id` with its id, so `unaccept`
    // deletes only the second and the first stays in force, deciding
    // structure invisible to the proposal that made it.
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

    proposals::unaccept(&conn, proposal.id).expect("unaccept");
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
    // Review finding 2's other hole: dismissing an accepted proposal must not
    // silently leave its rule alive under a `dismissed` label, invisible to
    // `open` and never seen by `reopen_orphaned_accepts`, which only looks at
    // `state = 'accepted'`.
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    let rule_id = proposals::accept(&conn, proposal.id).expect("accept");

    let err = proposals::dismiss(&conn, proposal.id).expect_err("refused");
    assert!(err.contains("un-accept"), "{err}");

    let rules: i64 = conn
        .query_row("SELECT count(*) FROM rule WHERE id = ?1", [rule_id], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(rules, 1, "the rule survives a refused dismiss");
}

#[test]
fn raise_refreshes_a_stale_open_proposals_score_and_reason() {
    // Review finding 3: `INSERT OR IGNORE` never updates, so a proposal
    // raised before its series had any works would otherwise keep its first
    // score and reason forever, and `open`'s threshold would keep hiding it
    // even after the volume-count signal starts agreeing.
    //
    // Bare `series` rows here, not a scanned tree: this test never calls
    // `accept`, only `raise`/`open`, and it needs to start at zero volumes
    // on both sides — `library_with_two_suffixed_series` no longer offers
    // that, deliberately, now that it gives every series a real volume.
    // `score_for`'s own doc makes the same call for the same reason.
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

#[test]
fn a_failure_between_the_insert_and_the_update_leaves_no_orphaned_rule() {
    // I5. Before `accept` wrapped its writes in one transaction, a failure
    // between `INSERT INTO rule` and `UPDATE proposal` left a live rule no
    // proposal pointed at while the proposal still read `open` — and a
    // second `accept` on that same proposal then wrote a *second* rule,
    // exactly the state `created_rule_id` exists to prevent, reached by a
    // different road.
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
    // I6. Without a `root_id` on the written rule, this proposal could not
    // even be accepted honestly under the *old* code: an unscoped
    // `other_series_under_prefix` would see root B's "Something Else" under
    // the same-spelled `Publisher/` prefix and refuse the whole accept over
    // material the rule, once scoped, can never actually reach. Scoped, it
    // succeeds — and the rule it writes must never touch root B's own series
    // on a later derive, which is the silent-corruption half of I6 measured
    // directly rather than only inferred from the column being non-NULL.
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
                .path
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

/// I6's other half: a subject whose own material sits under one
/// *same-spelled* directory in two different roots has no single root to
/// scope the written rule to, and `common_directory_pattern` must refuse
/// rather than pick one arbitrarily. Both roots hold a volume of the colour
/// series under their own `Publisher/`, so the subject's own directory
/// component is identical text (`"Publisher"`) in both — the one case the
/// "more than one directory" check above cannot catch on its own, since the
/// two directories are not spelled differently.
#[test]
fn accept_refuses_when_the_subjects_own_directory_is_spelled_the_same_in_two_roots() {
    let a = crate::tests::tempdir("proposals-accept-multiroot-a");
    write_pages(
        &a.path
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01"),
        2,
    );
    write_pages(
        &a.path
            .join("Publisher")
            .join("Go-Toubun no Hanayome - Full Color Edition - v01"),
        3,
    );
    let b = crate::tests::tempdir("proposals-accept-multiroot-b");
    write_pages(
        &b.path
            .join("Publisher")
            .join("Go-Toubun no Hanayome - Full Color Edition - v02"),
        4,
    );

    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root_a = scan::add_root(&conn, &a.path).expect("add root a");
    scan::scan_root(&conn, &root_a).expect("scan a");
    let root_b = scan::add_root(&conn, &b.path).expect("add root b");
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
            a.path.canonicalize().unwrap().display(),
            b.path.canonicalize().unwrap().display(),
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
    assert!(proposals::dismiss(&conn, 999).is_err());
}

#[test]
fn unaccepting_a_never_accepted_proposal_is_an_error() {
    let library = library_with_two_suffixed_series();
    let conn = library.conn();
    proposals::raise(&conn).expect("raise");
    let proposal = proposals::open(&conn, 0.0).expect("open").remove(0);
    assert!(proposals::unaccept(&conn, proposal.id).is_err());
}

// --- what the mutation sweep found unpinned --------------------------------

/// [`proposals::open`]'s own doc says "highest score first", and nothing
/// checked it: `the_score_threshold_hides_the_weak_proposal_but_not_the_strong_one`
/// reads `.len()` and the reason string, both of which a reversed `ORDER BY`
/// leaves alone. An inbox that lists the weakest guesses first is the inverse
/// of §6.1's whole ranking argument, and it looks exactly like a working one.
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
/// No test covered this at all; the seven lines of justification above the
/// query were the only thing standing behind it. Without the explicit order
/// the pick is whatever a b-tree scan returns first — unspecified, and liable
/// to change across a page split that has nothing to do with this data — and
/// a proposal that points somewhere different on each rescan cannot be
/// dismissed once and stay dismissed, which is §6's first argument.
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
/// `edition-of` match, and every test that reads a proposal back asks
/// `open(&conn, 0.0)` — which is every score at once, and so no score at all.
/// Zeroing `CERTAIN` left the whole suite green while hiding both kinds below
/// any threshold a person would actually type.
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
/// `insert_proposal`'s `AND state = ?6` is — and which no test reached:
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
    proposals::dismiss(&conn, raised.id).expect("dismiss");

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
    let publisher = dir.path.join("Publisher");
    write_pages(&publisher.join("Go-Toubun no Hanayome - v01"), 2);
    write_pages(
        &publisher.join("Go-Toubun no Hanayome - Full Color Edition - v01"),
        3,
    );
    let library = Library::open_in_memory().expect("open");
    {
        let conn = library.conn();
        let root = scan::add_root(&conn, &dir.path).expect("add root");
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
/// accept "would move too" misdescribes what it is. With the filter comparing
/// against the empty string instead of the target's title, the target lands
/// in `collateral`, `collateral.is_empty()` is never true, and this second
/// message becomes unreachable while the suite stays green.
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
            "proposal {}'s directory is shared with \"Go-Toubun no Hanayome\" itself, so \
             accepting it would set the colour edition key on Go-Toubun no Hanayome's own \
             volumes too. Narrow the layout or write the rule by hand.",
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
