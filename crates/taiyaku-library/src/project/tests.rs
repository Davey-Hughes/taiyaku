use super::*;
use crate::Library;

/// Go-Toubun volume 1's real alignment, from `docs/sidecar.md`'s worked
/// example, and **all five of its runs**. An earlier draft of this fixture
/// truncated the runs to three while keeping the real `quality` block, which
/// made the document contradict itself: three runs pair 5 slots at an average
/// of 2.5, not the 193 at 64.33333 the block claims. Use it whole — the numbers
/// only mean anything together.
const ONE_WORK: &str = r#"{
  "taiyaku": 1,
  "work": { "title": "Go-Toubun no Hanayome v01", "reading": "rtl" },
  "editions": {
    "en-mono": { "label": "English official",
                 "members": [{ "path": "en/v01.cbz", "sha256": "c104", "pages": 187 }],
                 "spreads": [4, 45] },
    "raw-mono": { "label": "Raw, monochrome",
                  "members": [{ "path": "raw/v01.cbz", "sha256": "3f9a", "pages": 195 }] }
  },
  "hub": "raw-mono",
  "alignments": [
    { "from": "raw-mono", "to": "en-mono", "by": "solver",
      "solver": { "hash": "dhash-16", "floor": 0.69921875 },
      "quality": { "verdict": "sound", "slots": 195, "paired": 193, "avg_run": 64.33333 },
      "runs": [
        { "a": "0", "b": "0", "margin": 0.08984375 },
        { "a": "1" },
        { "a": "2-5", "b": "1-4R", "margin": 0.24609375 },
        { "a": "6" },
        { "a": "7-194", "b": "5-186", "margin": -0.12890625 }
      ] }
  ]
}"#;

/// The whole store is a projection, so the test that matters is that the
/// projection is lossless. Byte-for-byte rather than semantic equality:
/// `Sidecar::to_json` is deterministic — BTreeMap keys, pretty-printed, and
/// `sidecar.md` pins the float formatting down to `avg_run`'s five decimals —
/// so a comparison written to tolerate differences would hide exactly the bugs
/// this exists to catch.
#[test]
fn a_sidecar_survives_a_round_trip_byte_for_byte() {
    let library = Library::open_in_memory().expect("library");
    let parsed = taiyaku_core::Sidecar::from_json(ONE_WORK).expect("fixture parses");
    let canonical = parsed.to_json();

    let conn = library.conn();
    let work_id = insert_by_hand(&conn, &parsed);
    let projected = sidecar(&conn, work_id).expect("projection");

    assert_eq!(projected.to_json(), canonical);
}

/// A spread list is page indices and must come back in order and complete.
#[test]
fn page_flags_survive_the_round_trip() {
    let library = Library::open_in_memory().expect("library");
    let parsed = taiyaku_core::Sidecar::from_json(ONE_WORK).expect("fixture parses");
    let conn = library.conn();
    let work_id = insert_by_hand(&conn, &parsed);
    let projected = sidecar(&conn, work_id).expect("projection");
    assert_eq!(projected.editions["en-mono"].spreads, vec![4, 45]);
}

/// Write one parsed sidecar's rows by hand, plain SQL rather than a call to
/// `import::sidecar` — which does not exist until Task 6. A *second* writer of
/// these rows on purpose: if this task's projection disagreed with Task 6's
/// import, going through the same writer would hide it, and this way a
/// projection bug stays distinguishable from an import bug. Returns the new
/// work's id.
fn insert_by_hand(conn: &Connection, sidecar: &Sidecar) -> i64 {
    let reading = match sidecar.work.reading {
        Direction::RightToLeft => "rtl",
        Direction::LeftToRight => "ltr",
    };
    conn.execute(
        "INSERT INTO series (sort_title) VALUES (?1)",
        [&sidecar.work.title],
    )
    .expect("series");
    let series_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO work (content_key, slug, series_id, title, reading, hub)
         VALUES ('key', 'slug', ?1, ?2, ?3, ?4)",
        rusqlite::params![series_id, sidecar.work.title, reading, sidecar.hub],
    )
    .expect("work");
    let work_id = conn.last_insert_rowid();

    for (key, edition) in &sidecar.editions {
        conn.execute(
            "INSERT INTO edition (work_id, key, label) VALUES (?1, ?2, ?3)",
            rusqlite::params![work_id, key, edition.label],
        )
        .expect("edition");
        let edition_id = conn.last_insert_rowid();

        for (ordinal, member) in edition.members.iter().enumerate() {
            conn.execute(
                "INSERT INTO member (edition_id, ordinal, path, sha256, pages)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    edition_id,
                    ordinal as i64,
                    member.path,
                    member.sha256,
                    member.pages
                ],
            )
            .expect("member");
        }
        for page in &edition.spreads {
            conn.execute(
                "INSERT INTO page_flag (edition_id, kind, page) VALUES (?1, 'spread', ?2)",
                rusqlite::params![edition_id, page],
            )
            .expect("spread flag");
        }
        for page in &edition.credits {
            conn.execute(
                "INSERT INTO page_flag (edition_id, kind, page) VALUES (?1, 'credit', ?2)",
                rusqlite::params![edition_id, page],
            )
            .expect("credit flag");
        }
    }

    for alignment in &sidecar.alignments {
        conn.execute(
            "INSERT INTO alignment
                 (work_id, from_key, to_key, by, solver_hash, solver_floor,
                  verdict, slots, paired, avg_run)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                work_id,
                alignment.from,
                alignment.to,
                source_text(alignment.by),
                alignment.solver.as_ref().map(|s| s.hash.clone()),
                alignment.solver.as_ref().map(|s| f64::from(s.floor)),
                alignment.quality.as_ref().map(|q| verdict_text(q.verdict)),
                alignment.quality.as_ref().map(|q| q.slots),
                alignment.quality.as_ref().map(|q| q.paired),
                alignment.quality.as_ref().map(|q| q.avg_run),
            ],
        )
        .expect("alignment");
        let alignment_id = conn.last_insert_rowid();

        for (ordinal, run) in alignment.runs.iter().enumerate() {
            let (b_start_page, b_start_half, b_end_page, b_end_half) = match &run.b {
                Some(span) => (
                    Some(span.start.page),
                    half_text(span.start.half),
                    Some(span.end.page),
                    half_text(span.end.half),
                ),
                None => (None, None, None, None),
            };
            conn.execute(
                "INSERT INTO run
                     (alignment_id, ordinal, a_start_page, a_start_half, a_end_page, a_end_half,
                      b_start_page, b_start_half, b_end_page, b_end_half, margin, by, note)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                rusqlite::params![
                    alignment_id,
                    ordinal as i64,
                    run.a.start.page,
                    half_text(run.a.start.half),
                    run.a.end.page,
                    half_text(run.a.end.half),
                    b_start_page,
                    b_start_half,
                    b_end_page,
                    b_end_half,
                    run.margin.map(f64::from),
                    run.by.map(source_text),
                    run.note,
                ],
            )
            .expect("run");
        }
    }

    work_id
}

fn source_text(source: Source) -> &'static str {
    match source {
        Source::Solver => "solver",
        Source::Hand => "hand",
    }
}

fn verdict_text(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Refused => "refused",
        Verdict::Fragmented => "fragmented",
        Verdict::Sound => "sound",
    }
}

fn half_text(half: Option<Half>) -> Option<&'static str> {
    match half {
        Some(Half::Left) => Some("left"),
        Some(Half::Right) => Some("right"),
        None => None,
    }
}

/// Every closed set this module reads back out of a text column must refuse a
/// value none of its own writers ever puts there — the same rule
/// `state::region_from_columns` polices for a note's region, applied here to
/// a hand edit that reaches `work.reading`.
#[test]
fn sidecar_refuses_a_stored_reading_direction_it_never_writes() {
    let library = Library::open_in_memory().expect("library");
    let parsed = taiyaku_core::Sidecar::from_json(ONE_WORK).expect("fixture parses");
    let conn = library.conn();
    let work_id = insert_by_hand(&conn, &parsed);

    conn.execute(
        "UPDATE work SET reading = 'diagonal' WHERE id = ?1",
        [work_id],
    )
    .expect("hand edit");

    let result = sidecar(&conn, work_id);
    assert!(
        result.is_err(),
        "a reading direction neither `rtl` nor `ltr` must not be silently accepted"
    );
}

/// The `alignment.by` analogue of
/// `sidecar_refuses_a_stored_reading_direction_it_never_writes`.
#[test]
fn sidecar_refuses_a_stored_alignment_source_it_never_writes() {
    let library = Library::open_in_memory().expect("library");
    let parsed = taiyaku_core::Sidecar::from_json(ONE_WORK).expect("fixture parses");
    let conn = library.conn();
    let work_id = insert_by_hand(&conn, &parsed);

    conn.execute(
        "UPDATE alignment SET by = 'robot' WHERE work_id = ?1",
        [work_id],
    )
    .expect("hand edit");

    let result = sidecar(&conn, work_id);
    assert!(
        result.is_err(),
        "a source neither `solver` nor `hand` must not be silently accepted"
    );
}

/// The `alignment.verdict` analogue.
#[test]
fn sidecar_refuses_a_stored_verdict_it_never_writes() {
    let library = Library::open_in_memory().expect("library");
    let parsed = taiyaku_core::Sidecar::from_json(ONE_WORK).expect("fixture parses");
    let conn = library.conn();
    let work_id = insert_by_hand(&conn, &parsed);

    conn.execute(
        "UPDATE alignment SET verdict = 'maybe' WHERE work_id = ?1",
        [work_id],
    )
    .expect("hand edit");

    let result = sidecar(&conn, work_id);
    assert!(
        result.is_err(),
        "a verdict outside {{refused, fragmented, sound}} must not be silently accepted"
    );
}

/// The `run.a_start_half`/`a_end_half`/`b_start_half`/`b_end_half` analogue —
/// `sidecar`'s own doc comment names exactly this: "a stored slot span cannot
/// be parsed back ... which means a hand edit reached the database."
#[test]
fn sidecar_refuses_a_stored_slot_half_it_never_writes() {
    let library = Library::open_in_memory().expect("library");
    let parsed = taiyaku_core::Sidecar::from_json(ONE_WORK).expect("fixture parses");
    let conn = library.conn();
    let work_id = insert_by_hand(&conn, &parsed);

    conn.execute("UPDATE run SET a_start_half = 'sideways'", [])
        .expect("hand edit");

    let result = sidecar(&conn, work_id);
    assert!(
        result.is_err(),
        "a slot half neither `left` nor `right` must not be silently accepted"
    );
}
