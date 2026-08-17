use super::*;

/// A URL has to name a work, and the sidecar carries no id — only a title,
/// which is neither unique nor stable. The filename is, so the id comes
/// from there, slugified so it survives a URL without escaping.
#[test]
fn a_work_is_named_by_its_sidecar_filename() {
    assert_eq!(
        work_id(Path::new("/library/Go-Toubun no Hanayome.taiyaku.json")),
        "go-toubun-no-hanayome-taiyaku"
    );
    assert_eq!(work_id(Path::new("taiyaku.json")), "taiyaku");
}

/// The id comes from the filename, so two sidecars in two directories with
/// the same filename — `taiyaku.json` beside each work is the convention
/// this project documents — are one id. Serving both would make one of
/// them permanently unreachable at every route, with nothing anywhere
/// saying which one won or that a choice was made at all.
///
/// O4: the code has refused this since M2 and nothing exercised it, so it
/// was correct by inspection only. The message has to name **both** paths,
/// because the whole point is that the person running the server cannot
/// otherwise tell which two files collided.
#[test]
fn two_sidecars_claiming_one_work_id_are_refused_rather_than_shadowed() {
    let first = crate::test_support::work("taiyaku-test-works-clash-first");
    let second = crate::test_support::work("taiyaku-test-works-clash-second");

    let refused = load_works(&[first.path.clone(), second.path.clone()]);

    crate::test_support::cleanup("taiyaku-test-works-clash-first");
    crate::test_support::cleanup("taiyaku-test-works-clash-second");

    let message = refused.expect_err("two files naming one work must not load");
    assert!(
        message.contains(&first.path.display().to_string()),
        "must name the sidecar already holding the id: {message}"
    );
    assert!(
        message.contains(&second.path.display().to_string()),
        "must name the sidecar that collided with it: {message}"
    );
    assert!(
        message.contains("taiyaku"),
        "must name the id they both claim: {message}"
    );
}

/// The other half of the same guard, so a refusal cannot be spelled
/// "refuse everything": two sidecars whose filenames differ load together
/// and keep their own ids.
#[test]
fn two_differently_named_sidecars_load_side_by_side() {
    let first = crate::test_support::work("taiyaku-test-works-distinct-first");
    let second = crate::test_support::work("taiyaku-test-works-distinct-second");
    let renamed = second.path.with_file_name("other.taiyaku.json");
    std::fs::rename(&second.path, &renamed).unwrap();

    let loaded = load_works(&[first.path.clone(), renamed]);

    crate::test_support::cleanup("taiyaku-test-works-distinct-first");
    crate::test_support::cleanup("taiyaku-test-works-distinct-second");

    let works = loaded.expect("distinct filenames are distinct works");
    let ids: Vec<&str> = works.iter().map(|w| w.id.as_str()).collect();
    assert_eq!(ids, ["taiyaku", "other-taiyaku"]);
}

/// The property the correction route rests on: what a later reader gets
/// back is what the last write put in, without a restart.
// `update`'s own `#[allow(clippy::result_large_err)]` does not reach a
// closure written at the call site — clippy checks each closure as its
// own item — so the explicit `Result<(), Response>` below needs the same
// allow, for the same reason.
#[allow(clippy::result_large_err)]
#[test]
fn a_replaced_sidecar_is_what_the_next_reader_sees() {
    let work = crate::test_support::work("taiyaku-test-works-replace");
    let before = work.sidecar();

    let swapped: Result<(), axum::response::Response> = work.update(|current| {
        let mut next = (**current).clone();
        next.work.title = "Renamed".into();
        Ok((std::sync::Arc::new(next), ()))
    });

    let after = work.sidecar();
    crate::test_support::cleanup("taiyaku-test-works-replace");
    swapped.expect("the update runs");
    assert_eq!(before.work.title, "T");
    assert_eq!(after.work.title, "Renamed");
}

/// A sidecar for the two library-backed tests below: `n` names the volume and
/// `sha` its hub member's hash, which is what `content_key` hashes — each
/// call must get a distinct `sha` or the three imports collide into one
/// work rather than landing three.
fn fixture_doc(n: u32, sha: &str) -> taiyaku_core::Sidecar {
    fixture_doc_reading(n, sha, "rtl")
}

/// `fixture_doc` with the reading direction of the caller's choosing — a fan
/// translation flipped to read left-to-right is ordinary, and the bridge row's
/// own `reading` column is the one place this crate has to write it down.
fn fixture_doc_reading(n: u32, sha: &str, reading: &str) -> taiyaku_core::Sidecar {
    let json = format!(
        r#"{{"taiyaku":1,"work":{{"title":"Volume {n}","reading":"{reading}"}},
             "editions":{{"raw":{{"label":"R","members":[
               {{"path":"v{n:02}.cbz","sha256":"{sha}","pages":10}}]}}}},
             "hub":"raw","alignments":[]}}"#
    );
    taiyaku_core::Sidecar::from_json(&json).expect("parses")
}

/// The corpus collides today: `work_id` derives from the filename, and
/// `taiyaku.json` beside each work is the documented convention, so
/// `load_works` refuses. Off the database, twelve works coexist — a work's
/// id there is `work.slug`, which `import::sidecar`'s `unique_slug` already
/// guarantees is unique across the library.
///
/// The unit form of the property. `serve_works_serves_two_sidecars_both_named_taiyaku_json`
/// in `taiyaku-cli` is the same claim through the command a person types.
#[test]
fn a_library_serves_many_works_where_load_works_refused_two() {
    let library = taiyaku_library::Library::open_in_memory().expect("library");
    {
        let conn = library.conn();
        for (n, sha) in [(1, "aa"), (2, "bb"), (3, "cc")] {
            taiyaku_library::import::sidecar(
                &conn,
                &fixture_doc(n, sha),
                std::path::Path::new(&format!("/corpus/v{n:02}/taiyaku.json")),
            )
            .expect("import");
        }
    }
    let works = crate::test_support::works_in(&library);
    assert_eq!(works.len(), 3);

    let mut slugs: Vec<&str> = works.iter().map(|w| w.id.as_str()).collect();
    slugs.sort_unstable();
    slugs.dedup();
    assert_eq!(slugs.len(), 3, "three works, three ids");
    assert_eq!(
        slugs,
        vec!["volume-1", "volume-2", "volume-3"],
        "each id is the library's own slug, derived from the title, not from \
         the filename every one of the three shares"
    );
}

/// `summary` reads `series` and `volume` from the row, not the sidecar —
/// there is no field for either on `taiyaku_core::Sidecar`. Nothing in this
/// crate sets `series_id` or `volume` yet (spec B's scanner does not exist),
/// so this proves the join itself works by writing them in by hand, the only
/// way to reach that branch before the scanner does.
#[test]
fn summary_reads_series_and_volume_from_the_row_when_present() {
    let library = taiyaku_library::Library::open_in_memory().expect("library");
    let work_row_id;
    {
        let conn = library.conn();
        work_row_id = taiyaku_library::import::sidecar(
            &conn,
            &fixture_doc(1, "aa"),
            std::path::Path::new("/corpus/v01/taiyaku.json"),
        )
        .expect("import");
        conn.execute(
            "INSERT INTO series (id, sort_title) VALUES (1, 'Some Series')",
            [],
        )
        .expect("insert series");
        conn.execute(
            "UPDATE work SET series_id = 1, volume = 1.0 WHERE id = ?1",
            [work_row_id],
        )
        .expect("attach series and volume");
    }
    let works = crate::test_support::works_in(&library);
    let conn = library.conn();
    let summary = crate::works::summary(&conn, &works[0]).expect("summary");
    assert_eq!(summary.series.as_deref(), Some("Some Series"));
    assert_eq!(summary.volume, Some(1.0));
}

/// A `Work` with no library row at all — `load_works`'s shape, until Task 15
/// makes `taiyaku serve <sidecar>...` an import — answers `None` for both
/// rather than failing or minting a row: `find_row`'s rule ("a read never
/// creates a row") applies here too, exercised through `summary` rather than
/// through `works::find_row` directly.
#[test]
fn summary_answers_none_for_series_and_volume_with_no_row_at_all() {
    let library = taiyaku_library::Library::open_in_memory().expect("library");
    let work = crate::test_support::work("taiyaku-test-works-summary-no-row");
    let conn = library.conn();
    let summary = crate::works::summary(&conn, &work).expect("summary");
    crate::test_support::cleanup("taiyaku-test-works-summary-no-row");
    assert_eq!(summary.slug, "t");
    assert_eq!(summary.title, "T");
    assert_eq!(summary.series, None);
    assert_eq!(summary.volume, None);
    assert_eq!(summary.editions, vec!["raw".to_string()]);
    assert_eq!(summary.cover, "/api/works/t/raw/pages/0/thumb");
}

/// **Two bridge rows, two `content_key`s.** `row_id_for_write` mints a `work`
/// row for a `Work` the library was never told about, and `content_key` is
/// `NOT NULL UNIQUE` in the schema — so the placeholder it writes has to be
/// distinct per work or the *second* work's `INSERT OR IGNORE` is silently
/// ignored, `find_row` then finds nothing under its slug, and the reader's
/// first bookmark on that work fails with "was just registered but cannot be
/// found". `bridge:{slug}` is what makes it distinct, `slug` being `UNIQUE`
/// itself.
///
/// The doc comment argues that uniqueness at length and nothing observed it:
/// replacing the whole `format!` with a constant passed the suite (whole-branch
/// review 2, Minor 6), because every fixture minted one row. Two here, with
/// the stored keys read back so the assertion is on the values rather than on
/// the call not failing.
#[test]
fn two_works_bridged_into_one_library_get_distinct_content_keys() {
    let library = taiyaku_library::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let alpha = crate::Work::new(
        "alpha".to_string(),
        std::path::PathBuf::from("/corpus/alpha/taiyaku.json"),
        fixture_doc(1, "aa"),
    );
    let beta = crate::Work::new(
        "beta".to_string(),
        std::path::PathBuf::from("/corpus/beta/taiyaku.json"),
        fixture_doc(2, "bb"),
    );

    let alpha_id = crate::works::row_id_for_write(&conn, &alpha).expect("mints alpha's row");
    let beta_id = crate::works::row_id_for_write(&conn, &beta).expect("mints beta's row");
    assert_ne!(
        alpha_id, beta_id,
        "each work must get a row of its own, not share one"
    );

    let mut keys: Vec<(String, String)> = {
        let mut stmt = conn
            .prepare("SELECT slug, content_key FROM work ORDER BY id")
            .expect("listing works");
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("listing works")
            .collect::<Result<_, _>>()
            .expect("reading works")
    };
    keys.sort();
    assert_eq!(
        keys,
        vec![
            ("alpha".to_string(), "bridge:alpha".to_string()),
            ("beta".to_string(), "bridge:beta".to_string()),
        ],
        "the placeholder key is derived from the slug, which is what makes it \
         unique across the works one server was started with"
    );

    // And the second call is a lookup, not a second mint — the `INSERT OR
    // IGNORE` path has to be idempotent for the racing-first-writes case its
    // own doc comment describes.
    assert_eq!(
        crate::works::row_id_for_write(&conn, &beta).expect("looks beta up again"),
        beta_id
    );
    let rows: i64 = conn
        .query_row("SELECT count(*) FROM work", [], |row| row.get(0))
        .expect("counting");
    assert_eq!(rows, 2, "two works, two rows, however many writes");
}

/// **Whole-branch review 3, Minor 9.** `row_id_for_write` copies the sidecar's
/// reading direction onto the bridge row through this module's own
/// `direction_text`, and nothing observed the value: making it return `""` for
/// both directions passed every test. That is not a cosmetic column.
/// `taiyaku_library::project::sidecar` parses it back and refuses anything it
/// did not write, so a bridge row carrying an empty string is unprojectable —
/// `taiyaku library export --all` and every later `serve_work` over that row
/// fail, at a distance, on a value written by a server that had it right in
/// memory the whole time.
///
/// Asserted through the projection rather than off the column, because that is
/// the consumer: the claim is that the row round-trips, not that a particular
/// three-letter string is in it. Both directions, since a mutation that
/// returned a single valid direction for everything would round-trip one of
/// them.
#[test]
fn a_bridged_row_carries_the_works_own_reading_direction_back_out() {
    use taiyaku_core::Direction;

    let library = taiyaku_library::Library::open_in_memory().expect("library");
    let conn = library.conn();

    for (slug, sha, reading, expected) in [
        ("rightward", "aa", "rtl", Direction::RightToLeft),
        ("leftward", "bb", "ltr", Direction::LeftToRight),
    ] {
        let work = crate::Work::new(
            slug.to_string(),
            std::path::PathBuf::from(format!("/corpus/{slug}/taiyaku.json")),
            fixture_doc_reading(1, sha, reading),
        );
        let row = crate::works::row_id_for_write(&conn, &work).expect("mints the bridge row");

        let projected = taiyaku_library::project::sidecar(&conn, row).unwrap_or_else(|e| {
            panic!("a bridge row must be projectable, and `{slug}`'s was not: {e}")
        });
        assert_eq!(
            projected.work.reading, expected,
            "`{slug}` reads {reading}, and the row has to say so — reading order \
             decides which half of a spread comes first"
        );
    }
}
