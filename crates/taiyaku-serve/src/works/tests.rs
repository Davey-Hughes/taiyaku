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
/// O4: the message has to name **both** paths, because the whole point is
/// that the person running the server cannot otherwise tell which two
/// files collided.
#[test]
fn two_sidecars_claiming_one_work_id_are_refused_rather_than_shadowed() {
    let tmp = crate::test_support::Fixture::new("works-clash-first");
    let first = crate::test_support::work(&tmp);
    let tmp2 = crate::test_support::Fixture::new("works-clash-second");
    let second = crate::test_support::work(&tmp2);

    let refused = load_works(&[first.path.clone(), second.path.clone()]);

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
    let tmp = crate::test_support::Fixture::new("works-distinct-first");
    let first = crate::test_support::work(&tmp);
    let tmp2 = crate::test_support::Fixture::new("works-distinct-second");
    let second = crate::test_support::work(&tmp2);
    let renamed = second.path.with_file_name("other.taiyaku.json");
    std::fs::rename(&second.path, &renamed).unwrap();

    let loaded = load_works(&[first.path.clone(), renamed]);

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
    let tmp = crate::test_support::Fixture::new("works-replace");
    let work = crate::test_support::work(&tmp);
    let before = work.sidecar();

    let swapped: Result<(), axum::response::Response> = work.update(|current| {
        let mut next = (**current).clone();
        next.work.title = "Renamed".into();
        Ok((std::sync::Arc::new(next), ()))
    });

    let after = work.sidecar();
    swapped.expect("the update runs");
    assert_eq!(before.work.title, "T");
    assert_eq!(after.work.title, "Renamed");
}

/// A sidecar for the library-backed tests below: `n` names the volume and
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

/// `work_id` derives from the filename, and `taiyaku.json` beside each work is
/// the documented convention, so a corpus collides and `load_works` refuses.
/// Off the database, works coexist — a work's id there is `work.slug`, which
/// `import::sidecar`'s `unique_slug` guarantees is unique across the library.
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
/// there is no field for either on `taiyaku_core::Sidecar`. This proves the
/// join itself works by writing them in by hand.
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
    let summary = crate::works::summary(&conn, &works[0], None).expect("summary");
    assert_eq!(summary.series.as_deref(), Some("Some Series"));
    assert_eq!(summary.volume, Some(1.0));
}

/// A `Work` with no library row at all — `load_works`'s shape — answers `None`
/// for both rather than failing or minting a row: `find_row`'s rule ("a read
/// never creates a row") applies here too, exercised through `summary` rather
/// than through `works::find_row` directly.
#[test]
fn summary_answers_none_for_series_and_volume_with_no_row_at_all() {
    let library = taiyaku_library::Library::open_in_memory().expect("library");
    let tmp = crate::test_support::Fixture::new("works-summary-no-row");
    let work = crate::test_support::work(&tmp);
    let conn = library.conn();
    let summary = crate::works::summary(&conn, &work, None).expect("summary");
    assert_eq!(summary.slug, "t");
    assert_eq!(summary.title, "T");
    assert_eq!(summary.series, None);
    assert_eq!(summary.volume, None);
    assert_eq!(summary.editions, vec!["raw".to_string()]);
    assert_eq!(summary.cover, "/api/works/t/raw/pages/0/thumb");
}

/// **A restricted grant filters `editions`, and — when the hub itself is
/// hidden — the `cover` URL too.** `list_works` (`routes.rs`) is the only
/// production caller of the `Some` branch; this drives `summary` directly so
/// the filtering rule has a unit test of its own rather than being provable
/// only through a whole HTTP round trip.
#[test]
fn summary_filters_editions_and_falls_back_off_a_hidden_hub() {
    let library = taiyaku_library::Library::open_in_memory().expect("library");
    let mut doc = fixture_doc(1, "aa");
    doc.editions.insert(
        "en".to_string(),
        taiyaku_core::Edition {
            label: "En".to_string(),
            members: vec![taiyaku_core::Member {
                path: "/corpus/v01/en/c1.cbz".to_string(),
                sha256: "en1".to_string(),
                pages: 1,
            }],
            spreads: Vec::new(),
            credits: Vec::new(),
        },
    );
    {
        let conn = library.conn();
        taiyaku_library::import::sidecar(
            &conn,
            &doc,
            std::path::Path::new("/corpus/v01/taiyaku.json"),
        )
        .expect("import");
    }
    let works = crate::test_support::works_in(&library);
    let conn = library.conn();

    // The hub (`fixture_doc`'s "raw") is outside the grant; only "en" is
    // inside it.
    let visible: std::collections::HashSet<String> = ["en".to_string()].into_iter().collect();
    let summary = crate::works::summary(&conn, &works[0], Some(&visible)).expect("summary");

    assert_eq!(
        summary.editions,
        vec!["en".to_string()],
        "the hidden edition must not be named"
    );
    assert_eq!(
        summary.cover, "/api/works/volume-1/en/pages/0/thumb",
        "the cover must not be built from the hidden hub"
    );
}

/// **Two bridge rows, two `content_key`s.** `row_id_for_write` mints a `work`
/// row for a `Work` the library was never told about, and `content_key` is
/// `NOT NULL UNIQUE` in the schema — so the placeholder it writes has to be
/// distinct per work or the *second* work's `INSERT OR IGNORE` is silently
/// ignored, `find_row` then finds nothing under its slug, and the reader's
/// first bookmark on that work fails with "was just registered in the library
/// but cannot be found". `bridge:{slug}` is what makes it distinct, `slug`
/// being `UNIQUE` itself.
///
/// One row cannot tell a per-work key from a constant, so this mints two,
/// with the stored keys read back so the assertion is on the values rather
/// than on the call not failing.
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

/// A bridged work's own untouched document, applied straight back over it,
/// leaves `title` exactly where it stood. `librarydoc::apply`'s title write
/// always runs, `title = COALESCE(?2, title_derived)`, and with no hand title
/// set (every freshly bridged row) that resolves through to
/// `title_derived` alone.
#[test]
fn a_bridged_works_own_untouched_document_can_be_applied_back_over_it() {
    let library = taiyaku_library::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work = crate::Work::new(
        "alpha".to_string(),
        std::path::PathBuf::from("/corpus/alpha/taiyaku.json"),
        fixture_doc(1, "aa"),
    );
    let work_id = crate::works::row_id_for_write(&conn, &work).expect("mints the bridge row");

    let doc = taiyaku_library::librarydoc::of(&conn, work_id).expect("projecting");
    assert_eq!(
        doc.title, None,
        "a freshly bridged row has no hand title, or this test proves nothing"
    );
    taiyaku_library::librarydoc::apply(&conn, work_id, &doc).unwrap_or_else(|e| {
        panic!(
            "applying a bridged work's own untouched document back over \
             itself must not fail: {e}"
        )
    });

    let title: String = conn
        .query_row("SELECT title FROM work WHERE id = ?1", [work_id], |row| {
            row.get(0)
        })
        .expect("reading the title back");
    assert_eq!(
        title, "Volume 1",
        "the title `row_id_for_write` minted must survive unharmed"
    );
}

/// `row_id_for_write` copies the sidecar's reading direction onto the bridge
/// row through this module's own `direction_text`. The reading column is not
/// cosmetic: `taiyaku_library::project::sidecar` parses it back and refuses
/// anything it did not write, so a bridge row carrying an empty string is
/// unprojectable — `taiyaku library export --all` and every later `serve_work`
/// over that row fail, at a distance, on a value written by a server that had
/// it right in memory the whole time.
///
/// Asserted through the projection rather than off the column, because that is
/// the consumer: the claim is that the row round-trips, not that a particular
/// three-letter string is in it. Both directions, since returning one valid
/// direction for everything would round-trip one of them.
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

/// A scanned work has no `sidecar_path` — `import::scanned` leaves it NULL by
/// design — and must still become a servable `Work`.
///
/// Its member paths are absolute, which is what makes this work at all:
/// `derive` writes them from the root down, and `root.join(&member.path)`
/// returns an absolute path unchanged. Spec C1 §5.3.
#[test]
fn a_work_with_no_sidecar_path_still_becomes_a_servable_work() {
    let library = taiyaku_library::Library::open_in_memory().expect("in-memory");
    {
        let conn = library.conn();
        let json = r#"{"taiyaku":1,"work":{"title":"Scanned","reading":"rtl"},
            "editions":{"raw":{"label":"raw","members":[
              {"path":"/abs/shelf/Series - v01","sha256":"deadbeef","pages":2}]}},
            "hub":"raw","alignments":[]}"#;
        let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
        taiyaku_library::import::scanned(&conn, &doc, None).expect("imports as a scan");
    }

    let works = from_library(&library).expect("builds works");
    assert_eq!(works.len(), 1);
    assert_eq!(works[0].sidecar().work.title, "Scanned");
    assert_eq!(
        works[0].path,
        PathBuf::new(),
        "no sidecar path becomes an empty path, which `has_document` reads as none"
    );
    assert!(!works[0].has_document(), "a scanned work has no document");
}

#[test]
fn a_work_with_a_sidecar_path_keeps_it_and_has_a_document() {
    let library = taiyaku_library::Library::open_in_memory().expect("in-memory");
    {
        let conn = library.conn();
        let json = r#"{"taiyaku":1,"work":{"title":"Imported","reading":"rtl"},
            "editions":{"raw":{"label":"raw","members":[
              {"path":"raw/v01.cbz","sha256":"deadbeef","pages":2}]}},
            "hub":"raw","alignments":[]}"#;
        let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
        taiyaku_library::import::sidecar(&conn, &doc, Path::new("/x/taiyaku.json"))
            .expect("imports");
    }

    let works = from_library(&library).expect("builds works");
    assert_eq!(works.len(), 1);
    assert_eq!(works[0].path, PathBuf::from("/x/taiyaku.json"));
    assert!(works[0].has_document());
}
