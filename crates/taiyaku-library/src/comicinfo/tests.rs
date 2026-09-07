use super::{ComicInfo, xml};

fn full() -> ComicInfo {
    ComicInfo {
        series: Some("Go-Toubun no Hanayome".to_string()),
        number: Some(1.0),
        title: Some("Go-Toubun no Hanayome, volume 1".to_string()),
        page_count: 194,
        language: Some("ja".to_string()),
        credits: vec![
            ("author".to_string(), "Negi Haruba".to_string()),
            ("artist".to_string(), "An Artist".to_string()),
            ("translator".to_string(), "A Translator".to_string()),
            ("group".to_string(), "Some Group".to_string()),
            ("publisher".to_string(), "Kodansha".to_string()),
        ],
        notes: Some("Japanese raw".to_string()),
    }
}

#[test]
fn every_fact_that_exists_becomes_an_element() {
    let out = xml(&full());
    assert!(out.starts_with("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n"));
    assert!(out.contains("<Series>Go-Toubun no Hanayome</Series>"));
    assert!(
        out.contains("<Number>1</Number>"),
        "an integral volume drops the .0"
    );
    assert!(out.contains("<PageCount>194</PageCount>"));
    assert!(out.contains("<LanguageISO>ja</LanguageISO>"));
    assert!(out.contains("<Writer>Negi Haruba</Writer>"));
    assert!(out.contains("<Penciller>An Artist</Penciller>"));
    assert!(out.contains("<Translator>A Translator</Translator>"));
    assert!(out.contains("<Notes>Japanese raw</Notes>"));
}

/// Every one of `element_for`'s five roles, pinned to the exact element it
/// must produce — "the only judgement in this module" deserves a test that
/// notices if any one of the five is renamed.
#[test]
fn each_role_maps_to_its_named_element() {
    let out = xml(&full());
    assert!(
        out.contains("<Writer>Negi Haruba</Writer>"),
        "author -> Writer; got {out}"
    );
    assert!(
        out.contains("<Penciller>An Artist</Penciller>"),
        "artist -> Penciller; got {out}"
    );
    assert!(
        out.contains("<Translator>A Translator</Translator>"),
        "translator -> Translator; got {out}"
    );
    assert!(
        out.contains("<Publisher>Some Group, Kodansha</Publisher>"),
        "group and publisher -> Publisher; got {out}"
    );
}

#[test]
fn a_group_and_a_publisher_share_one_element() {
    let out = xml(&full());
    assert!(
        out.contains("<Publisher>Some Group, Kodansha</Publisher>"),
        "a scanlation group is who published that edition; got {out}"
    );
}

#[test]
fn a_half_volume_keeps_its_fraction() {
    let mut info = full();
    info.number = Some(1.5);
    assert!(xml(&info).contains("<Number>1.5</Number>"));
}

#[test]
fn an_absent_fact_is_an_absent_element_not_an_empty_one() {
    let info = ComicInfo {
        series: None,
        number: None,
        title: None,
        page_count: 3,
        language: None,
        credits: Vec::new(),
        notes: None,
    };
    let out = xml(&info);
    for absent in [
        "<Series",
        "<Number",
        "<Title",
        "<LanguageISO",
        "<Writer",
        "<Publisher",
        "<Notes",
    ] {
        assert!(
            !out.contains(absent),
            "an empty <Writer/> claims the writer is unknown; leaving it out claims nothing. Found {absent} in {out}"
        );
    }
    // The one element that is always true.
    assert!(out.contains("<PageCount>3</PageCount>"));
}

#[test]
fn a_title_with_markup_in_it_is_escaped() {
    let mut info = full();
    info.title = Some("A & B <not a tag>".to_string());
    let out = xml(&info);
    assert!(out.contains("<Title>A &amp; B &lt;not a tag&gt;</Title>"));
}

/// A hand-edited sidecar has no character-class validation on the way in, so
/// a name or title can carry C0 control bytes XML 1.0 forbids outright — they
/// cannot be escaped into legality, only dropped. Tab, newline and carriage
/// return are legal XML and must survive.
#[test]
fn control_characters_are_dropped_but_tab_and_newline_survive() {
    let mut info = full();
    info.title =
        Some("Bad\u{0}Name\u{1}\u{8}\u{b}\u{c}\u{1f}\twith\ntab and newline\r.".to_string());
    let out = xml(&info);
    assert!(
        out.contains("<Title>BadName\twith\ntab and newline\r.</Title>"),
        "forbidden C0 controls must be dropped, tab/newline/CR kept; got {out}"
    );
}

/// `U+FFFE` and `U+FFFF` are valid Rust `char`s — unlike a surrogate, which
/// cannot survive as one — and XML 1.0's `Char` production forbids both:
/// its BMP range stops at `U+FFFD`. A hand-edited sidecar can carry either
/// through `serde_json` into a title, exactly as it can carry a C0 control,
/// and `escape` must drop them the same way rather than passing them
/// through into a document that is not well-formed.
#[test]
fn noncharacters_u_fffe_and_u_ffff_are_dropped_too() {
    let mut info = full();
    info.title = Some("Bad\u{fffe}Name\u{ffff}.".to_string());
    let out = xml(&info);
    assert!(
        out.contains("<Title>BadName.</Title>"),
        "U+FFFE and U+FFFF must be dropped, not passed through; got {out}"
    );
}

#[test]
fn read_gets_the_series_the_language_and_the_label_out_of_the_rows() {
    let library = crate::Library::open_in_memory().expect("in-memory library");
    let conn = library.conn();
    conn.execute(
        "INSERT INTO series (id, sort_title) VALUES (1, 'Sortable')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO series_title (series_id, title, kind, is_primary)
         VALUES (1, 'Alternative', 'alt', 0), (1, 'The Primary One', 'english', 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO work (id, content_key, slug, series_id, volume, title, reading, hub)
         VALUES (1, 'k', 'w-v01', 1, 1.0, 'Volume One', 'rtl', 'raw')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO edition (id, work_id, key, label, language)
         VALUES (1, 1, 'raw', 'Japanese raw', 'ja')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO person (id, name) VALUES (1, 'Negi Haruba')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO credit (person_id, role, series_id) VALUES (1, 'author', 1)",
        [],
    )
    .unwrap();

    let info = super::read(&conn, "w-v01", "raw", 194).unwrap();

    assert_eq!(
        info.series.as_deref(),
        Some("The Primary One"),
        "is_primary wins"
    );
    assert_eq!(info.language.as_deref(), Some("ja"));
    assert_eq!(info.notes.as_deref(), Some("Japanese raw"));
    assert_eq!(info.number, Some(1.0));
    assert_eq!(info.page_count, 194);
    assert_eq!(
        info.credits,
        vec![("author".to_string(), "Negi Haruba".to_string())]
    );
}

/// A work with no series at all still gets its credits.
///
/// **Not a NULL-semantics curiosity — the ordinary case for half this
/// library.** `import` never writes `series_id` and cannot, since
/// `taiyaku.json` has no field for it, so every hand-imported work reaches
/// this query with `series_id` bound to SQL NULL. The credit query's third
/// arm has to sit out cleanly rather than matching every row whose
/// `series_id` is also NULL, and `read` has to answer with `series: None`
/// rather than failing.
#[test]
fn a_work_with_no_series_still_reads_its_work_level_credits() {
    let library = crate::Library::open_in_memory().expect("in-memory library");
    let conn = library.conn();
    // A second series and a credit attached to it, so a query that leaked
    // across the NULL would pick this up and fail the assertion below.
    conn.execute(
        "INSERT INTO series (id, sort_title) VALUES (9, 'Someone Else')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO work (id, content_key, slug, series_id, volume, title, reading, hub)
         VALUES (1, 'k', 'hand-imported', NULL, NULL, 'A Hand Import', 'rtl', 'raw')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO edition (id, work_id, key, label, language)
         VALUES (1, 1, 'raw', 'Raw', NULL)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO person (id, name) VALUES (1, 'Mine'), (2, 'Not Mine'), (3, 'Nobody''s')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO credit (person_id, role, work_id) VALUES (1, 'author', 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO credit (person_id, role, series_id) VALUES (2, 'author', 9)",
        [],
    )
    .unwrap();
    // A credit with series_id, work_id AND edition_id all NULL — `credit` has
    // a surrogate primary key rather than a composite one over the three
    // nullable columns for exactly this reason. A query whose third arm
    // conflates NULL with NULL (`IS ?3`, or a COALESCE-equality) matches this
    // row and leaks it into a work it has nothing to do with; `= ?3` never
    // does, since SQL NULL is never `=` to anything, including another NULL.
    conn.execute(
        "INSERT INTO credit (person_id, role) VALUES (3, 'author')",
        [],
    )
    .unwrap();

    let info = super::read(&conn, "hand-imported", "raw", 12).unwrap();

    assert_eq!(info.series, None, "no series row to name");
    assert_eq!(info.number, None);
    assert_eq!(
        info.credits,
        vec![("author".to_string(), "Mine".to_string())],
        "the other series' credit must not leak across a NULL series_id"
    );
    assert!(!super::xml(&info).contains("<Series"));
}
