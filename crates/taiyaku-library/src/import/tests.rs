use super::*;
use crate::Library;
use std::path::Path;

fn doc(hub_sha: &str) -> taiyaku_core::Sidecar {
    let json = format!(
        r#"{{"taiyaku":1,"work":{{"title":"T","reading":"rtl"}},
             "editions":{{"raw":{{"label":"R","members":[
               {{"path":"raw/v01.cbz","sha256":"{hub_sha}","pages":10}}]}}}},
             "hub":"raw","alignments":[]}}"#
    );
    taiyaku_core::Sidecar::from_json(&json).expect("parses")
}

/// A hub edition plus a spoke edition and an alignment between them. Shares
/// its hub member's `sha256` — and so its `content_key` — with
/// `hub_only_doc`, since `content_key` hashes the hub edition's members
/// alone; the spoke and the alignment are what a reimport with `hub_only_doc`
/// must remove.
fn two_edition_doc() -> taiyaku_core::Sidecar {
    let json = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
         "editions":{
           "raw":{"label":"R","members":[
             {"path":"raw/v01.cbz","sha256":"hh","pages":10}]},
           "en":{"label":"En","members":[
             {"path":"en/v01.cbz","sha256":"ee","pages":10}]}
         },
         "hub":"raw",
         "alignments":[
           {"from":"raw","to":"en","by":"hand",
            "runs":[{"a":"0","b":"0"}]}
         ]}"#;
    taiyaku_core::Sidecar::from_json(json).expect("parses")
}

/// A hub edition ("raw") and a spoke ("en"), aligned by exactly the runs
/// given: `(a, b, by)` triples in the document's own span notation, `b`
/// absent for an unpaired run and `by` given only for a run recorded
/// hand-corrected in the document itself. Builds the JSON the way `doc` and
/// `two_edition_doc` do, parameterized on the runs a `reimport` test wants to
/// exercise merging against.
fn two_edition_doc_with_runs(runs: &[(&str, Option<&str>, Option<&str>)]) -> taiyaku_core::Sidecar {
    two_edition_doc_with_runs_and_hub_sha(runs, "hh")
}

/// `two_edition_doc_with_runs`, with the hub member's `sha256` parameterised
/// rather than fixed at `"hh"`. What a reimport test needs to simulate the
/// hub's archives having been replaced underneath an existing import.
fn two_edition_doc_with_runs_and_hub_sha(
    runs: &[(&str, Option<&str>, Option<&str>)],
    hub_sha: &str,
) -> taiyaku_core::Sidecar {
    let runs_json: Vec<String> = runs
        .iter()
        .map(|(a, b, by)| {
            let mut fields = vec![format!(r#""a":"{a}""#)];
            if let Some(b) = b {
                fields.push(format!(r#""b":"{b}""#));
            }
            if let Some(by) = by {
                fields.push(format!(r#""by":"{by}""#));
            }
            format!("{{{}}}", fields.join(","))
        })
        .collect();
    let json = format!(
        r#"{{"taiyaku":1,"work":{{"title":"T","reading":"rtl"}},
             "editions":{{
               "raw":{{"label":"R","members":[
                 {{"path":"raw/v01.cbz","sha256":"{hub_sha}","pages":10}}]}},
               "en":{{"label":"En","members":[
                 {{"path":"en/v01.cbz","sha256":"ee","pages":10}}]}}
             }},
             "hub":"raw",
             "alignments":[
               {{"from":"raw","to":"en","by":"solver","runs":[{}]}}
             ]}}"#,
        runs_json.join(",")
    );
    taiyaku_core::Sidecar::from_json(&json).expect("parses")
}

/// `doc(hub_sha)` plus a second edition named `name` and no alignment
/// mentioning it — an orphan edition, the shape solving leaves behind when it
/// refuses to align a spoke to the hub, or when the edition holds only a
/// direct edge to some other spoke.
fn doc_with_unaligned_edition(hub_sha: &str, name: &str) -> taiyaku_core::Sidecar {
    let json = format!(
        r#"{{"taiyaku":1,"work":{{"title":"T","reading":"rtl"}},
             "editions":{{
               "raw":{{"label":"R","members":[
                 {{"path":"raw/v01.cbz","sha256":"{hub_sha}","pages":10}}]}},
               "{name}":{{"label":"X","members":[
                 {{"path":"{name}/v01.cbz","sha256":"xx","pages":10}}]}}
             }},
             "hub":"raw","alignments":[]}}"#
    );
    taiyaku_core::Sidecar::from_json(&json).expect("parses")
}

/// The hub edition alone, matching `two_edition_doc`'s hub member exactly so
/// the two share a `content_key` — reimporting one over the other is the
/// same work losing an edition, not a second work.
fn hub_only_doc() -> taiyaku_core::Sidecar {
    let json = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
         "editions":{
           "raw":{"label":"R","members":[
             {"path":"raw/v01.cbz","sha256":"hh","pages":10}]}
         },
         "hub":"raw","alignments":[]}"#;
    taiyaku_core::Sidecar::from_json(json).expect("parses")
}

/// A hub (`raw`) and **two** spokes (`en`, `fr`), each with its own hub->spoke
/// alignment and a run count of its own so the two edges cannot be mistaken
/// for one another.
///
/// Every other reimport fixture here is a hub plus at most one spoke, and with
/// one spoke there is one alignment, so `reimport`'s spoke->alignment `find`
/// cannot be observed to pick the wrong one: replacing its `&&` with `||`
/// passed the entire workspace suite while the resulting binary discarded a
/// whole freshly-solved alignment on real material (whole-branch review 3,
/// I2). Every work in the corpus has three to five editions, so this is the
/// ordinary shape, not an exotic one.
fn three_edition_doc(en_runs: usize, fr_runs: usize) -> taiyaku_core::Sidecar {
    three_edition_doc_with_hub_sha(en_runs, fr_runs, "hh")
}

/// `three_edition_doc`, with the hub member's `sha256` parameterised — what a
/// test needs to make `merge_spoke`'s from-side guard refuse *every* spoke at
/// once, which is the only way to reach [`drop_stale_spoke`]'s `Added` arm: a
/// spoke the library has never held, on a re-solve the library cannot merge.
fn three_edition_doc_with_hub_sha(
    en_runs: usize,
    fr_runs: usize,
    hub_sha: &str,
) -> taiyaku_core::Sidecar {
    fn runs(n: usize) -> String {
        (0..n)
            .map(|i| format!(r#"{{"a":"{i}","b":"{i}"}}"#))
            .collect::<Vec<_>>()
            .join(",")
    }
    let json = format!(
        r#"{{"taiyaku":1,"work":{{"title":"T","reading":"rtl"}},
             "editions":{{
               "raw":{{"label":"R","members":[
                 {{"path":"raw/v01.cbz","sha256":"{hub_sha}","pages":20}}]}},
               "en":{{"label":"En","members":[
                 {{"path":"en/v01.cbz","sha256":"ee","pages":20}}]}},
               "fr":{{"label":"Fr","members":[
                 {{"path":"fr/v01.cbz","sha256":"ff","pages":20}}]}}
             }},
             "hub":"raw",
             "alignments":[
               {{"from":"raw","to":"en","by":"solver","runs":[{}]}},
               {{"from":"raw","to":"fr","by":"solver","runs":[{}]}}
             ]}}"#,
        runs(en_runs),
        runs(fr_runs)
    );
    taiyaku_core::Sidecar::from_json(&json).expect("parses")
}

/// The edges of a projected work, as `(from, to, runs)`, sorted — what a
/// spoke paired with the wrong alignment destroys.
fn edges(conn: &Connection, work_id: i64) -> Vec<(String, String, usize)> {
    let mut found: Vec<(String, String, usize)> = crate::project::sidecar(conn, work_id)
        .expect("projection")
        .alignments
        .iter()
        .map(|a| (a.from.clone(), a.to.clone(), a.runs.len()))
        .collect();
    found.sort();
    found
}

/// **Whole-branch review 3, I2.** Each spoke must be merged against *its own*
/// hub edge. With one spoke the `find` at `reimport`'s merge loop cannot be
/// wrong; with two, taking the first alignment that mentions the hub pairs the
/// second spoke with the first spoke's edge, and the second edge — eleven runs
/// of freshly-solved alignment on the corpus work this was reproduced against
/// — is discarded with nothing in the summary naming it.
#[test]
fn every_spoke_is_merged_against_its_own_hub_edge() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let path = Path::new("/x/taiyaku.json");

    let solved = three_edition_doc(5, 11);
    let id = sidecar(&conn, &solved, path).expect("first import");
    assert_eq!(
        edges(&conn, id),
        vec![
            ("raw".to_string(), "en".to_string(), 5),
            ("raw".to_string(), "fr".to_string(), 11),
        ],
        "the fixture must really hold two hub edges, or this test proves nothing"
    );

    // A reader corrects the first run of each edge — one hand run per spoke,
    // which is what a mis-paired merge loses along with the edge itself.
    let corrected = conn
        .execute("UPDATE run SET by = 'hand' WHERE ordinal = 0", [])
        .expect("correction");
    assert_eq!(
        corrected, 2,
        "one hand-corrected run on each of the two edges"
    );

    let (again, outcomes) = reimport(&conn, &solved, path).expect("re-import");
    assert_eq!(again, id);

    // `Merged` carries no edition name, so the pairing is asserted where it
    // shows: both edges survive the re-solve, each with its own run count.
    assert_eq!(
        edges(&conn, id),
        vec![
            ("raw".to_string(), "en".to_string(), 5),
            ("raw".to_string(), "fr".to_string(), 11),
        ],
        "each spoke keeps the alignment solved for it"
    );
    assert_eq!(
        outcomes,
        vec![
            taiyaku_core::Merged::CarriedHandRuns(1),
            taiyaku_core::Merged::CarriedHandRuns(1),
        ],
        "one outcome per spoke, in `editions` order, each carrying its own \
         edge's hand correction forward: {outcomes:?}"
    );
    let hand: i64 = conn
        .query_row("SELECT count(*) FROM run WHERE by = 'hand'", [], |r| {
            r.get(0)
        })
        .expect("counting hand runs");
    assert_eq!(hand, 2, "neither spoke's hand correction was dropped");
}

/// A document whose hub edge is recorded **backwards** — `en` -> `raw` rather
/// than `raw` -> `en` — with a hand-corrected run on it. `Sidecar::correct`
/// creates a direct edge in whichever direction it was first asked for, so an
/// edge stored this way is a document `taiyaku align --direct` produced, not
/// a corruption.
fn reverse_edge_doc() -> taiyaku_core::Sidecar {
    let json = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
         "editions":{
           "raw":{"label":"R","members":[
             {"path":"raw/v01.cbz","sha256":"hh","pages":10}]},
           "en":{"label":"En","members":[
             {"path":"en/v01.cbz","sha256":"ee","pages":10}]}
         },
         "hub":"raw",
         "alignments":[
           {"from":"en","to":"raw","by":"hand",
            "runs":[{"a":"0-9","b":"0-9","by":"hand"}]}
         ]}"#;
    taiyaku_core::Sidecar::from_json(json).expect("parses")
}

/// **Whole-branch review 3, Minor 6, the reverse-orientation clause.**
/// `drop_stale_spoke` scopes itself to the `hub` <-> `name` edge in *either*
/// orientation, and no fixture stored one the other way round, so
/// `(a.from == name && a.to == hub)` was unpinned in both the count and the
/// `retain` — eight surviving mutants between them and the arms below.
///
/// This is also the one shape `merge_spoke` refuses *because* of the
/// orientation: a hand run on a reverse-stored edge is addressed against
/// `en`'s slots, and carrying it onto a `raw` -> `en` edge without inverting
/// it first would misfile it, so core refuses rather than doing that silently
/// (`core/sidecar.rs:699`). `reimport` then has to drop the edge, and dropping
/// it means finding it — which the mutated filter does not.
#[test]
fn a_stale_hub_edge_stored_backwards_is_dropped_with_the_hand_runs_on_it() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let path = Path::new("/x/taiyaku.json");

    let id = sidecar(&conn, &reverse_edge_doc(), path).expect("first import");
    assert_eq!(
        edges(&conn, id),
        vec![("en".to_string(), "raw".to_string(), 1)],
        "the fixture must really store the edge backwards, or this test proves nothing"
    );

    // The freshly-solved document: the same hub bytes, the edge the ordinary
    // way round, and no hand runs on it.
    let solved =
        two_edition_doc_with_runs(&[("0-4", Some("0-4"), None), ("5-9", Some("5-9"), None)]);
    let (again, outcomes) = reimport(&conn, &solved, path).expect("re-import");
    assert_eq!(again, id, "same work");

    assert_eq!(
        outcomes,
        vec![taiyaku_core::Merged::DroppedHandRuns(1)],
        "the hand run on the backwards edge is a human decision, and losing it \
         is reported rather than counted as an ordinary replacement: {outcomes:?}"
    );
    assert_eq!(
        edges(&conn, id),
        vec![("raw".to_string(), "en".to_string(), 2)],
        "and the stale edge really goes: one edge afterwards, the incoming one"
    );
    let hand: i64 = conn
        .query_row("SELECT count(*) FROM run WHERE by = 'hand'", [], |r| {
            r.get(0)
        })
        .expect("counting hand runs");
    assert_eq!(
        hand, 0,
        "nothing of the dropped edge survives under the new one"
    );
}

/// **Whole-branch review 3, Minor 6, the `Replaced` and `Added` arms.** Only
/// `DroppedHandRuns` had a fixture, and it is the *rare* case: a re-solve of a
/// work nobody has hand-corrected is the common one, and it takes the same
/// refusal path the moment the hub moves.
///
/// One re-solve reaches both remaining arms. The hub's archives were replaced,
/// so `merge_spoke`'s from-side guard refuses every spoke — `en`, which the
/// library holds, and `fr`, which it has never seen. The first is a
/// replacement and the second is an addition, and nothing was lost in either,
/// so neither may report `DroppedHandRuns`.
#[test]
fn a_re_solve_over_a_moved_hub_reports_replaced_and_added_rather_than_a_loss() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let path = Path::new("/x/taiyaku.json");

    let held = two_edition_doc_with_runs(&[("0-9", Some("0-9"), None)]);
    let id = sidecar(&conn, &held, path).expect("first import");
    assert_eq!(
        edges(&conn, id),
        vec![("raw".to_string(), "en".to_string(), 1)],
        "one spoke, no hand runs — the common shape a re-solve meets"
    );

    // The hub's archives were replaced and a third edition arrived with the
    // re-solve. Every stored run addresses a page sequence that is gone.
    let resolved = three_edition_doc_with_hub_sha(4, 7, "zz");
    let (again, outcomes) = reimport(&conn, &resolved, path).expect("re-import");
    assert_eq!(again, id, "same work, found by path");

    assert_eq!(
        outcomes,
        vec![taiyaku_core::Merged::Replaced, taiyaku_core::Merged::Added],
        "`editions` order: `en` was held and is replaced, `fr` is new and is \
         added, and neither cost a hand correction: {outcomes:?}"
    );
    assert_eq!(
        edges(&conn, id),
        vec![
            ("raw".to_string(), "en".to_string(), 4),
            ("raw".to_string(), "fr".to_string(), 7),
        ],
        "both freshly-solved edges land, each with its own run count"
    );
}

/// **Whole-branch review 3, Minor 1.** A credit list is hand-entered and
/// ordered — "the artist, then the translator, then the group" is somebody's
/// decision — and `held_credits` read it with no `ORDER BY` at all. The rows
/// are deleted and reinserted wholesale on every reimport, taking fresh `id`s
/// in whatever order they came back in, so the read order *is* the stored
/// order from then on. `librarydoc::of` reads them back `ORDER BY credit.id`,
/// which is why this asserts through the projection rather than the raw table:
/// it is the order a reader sees in their own `taiyaku.library.json`.
///
/// This is the one route that had no test. `serve` reimports every sidecar on
/// every start, so it is also the one that runs most.
#[test]
fn a_hand_entered_credit_list_keeps_its_order_across_a_reimport() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let path = Path::new("/x/taiyaku.json");
    let d = doc("aa");
    let id = sidecar(&conn, &d, path).expect("first import");

    let credited = r#"{"taiyaku_library":1,"updated":"2026-08-17T09:00:00Z",
        "editions":{"raw":{"credits":[
          {"role":"artist","name":"Negi Haruba"},
          {"role":"translator","name":"Steven LeCroy"},
          {"role":"group","name":"Kodansha"},
          {"role":"publisher","name":"Kodansha USA"}
        ]}}}"#;
    let metadata = crate::librarydoc::LibraryDoc::from_json(credited).expect("parses");
    crate::librarydoc::apply(&conn, id, &metadata).expect("the reader enters the credits");

    let names = |conn: &Connection| -> Vec<String> {
        crate::librarydoc::of(conn, id)
            .expect("projection")
            .editions["raw"]
            .credits
            .iter()
            .map(|c| c.name.clone())
            .collect()
    };
    let entered = names(&conn);
    assert_eq!(
        entered,
        vec!["Negi Haruba", "Steven LeCroy", "Kodansha", "Kodansha USA"],
        "the order the document gave, before any reimport"
    );

    sidecar(&conn, &d, path).expect("the reimport `serve` runs on every start");

    assert_eq!(
        names(&conn),
        entered,
        "and the same order afterwards: a routine rescan is not a licence to \
         reshuffle a list a person typed"
    );
}

/// **Whole-branch review 3, Minor 7.** `unique_slug`'s collision loop was
/// pinned only at its first step: `two_slugs_never_collide` produces two works
/// with one title, so `suffix += 1` runs once and never gets to be wrong.
/// Both `-= 1` (giving `t-2` then `t-1`) and `*= 2` (giving `t-2` then `t-4`)
/// passed. A third volume of one series imported before anyone has retitled
/// them is the ordinary way to get there.
#[test]
fn a_third_work_with_one_title_takes_the_third_slug_in_sequence() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let slugs: Vec<String> = ["aa", "bb", "cc"]
        .iter()
        .enumerate()
        .map(|(n, sha)| {
            let id = sidecar(
                &conn,
                &doc(sha),
                &Path::new("/x").join(format!("v{n}")).join("taiyaku.json"),
            )
            .expect("import");
            conn.query_row("SELECT slug FROM work WHERE id = ?1", [id], |r| r.get(0))
                .expect("slug")
        })
        .collect();

    assert_eq!(
        slugs,
        vec!["t", "t-2", "t-3"],
        "the suffix counts up by one from 2, so the third is `t-3` — not `t-1`, \
         which reads as a first volume, and not `t-4`, which reads as a gap"
    );
}

#[test]
fn importing_the_same_document_twice_is_one_work() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let first = sidecar(&conn, &doc("aa"), Path::new("/x/taiyaku.json")).expect("first");
    let second = sidecar(&conn, &doc("aa"), Path::new("/x/taiyaku.json")).expect("second");
    assert_eq!(first, second);

    let works: i64 = conn
        .query_row("SELECT count(*) FROM work", [], |r| r.get(0))
        .unwrap();
    assert_eq!(works, 1);
}

/// The path is a hint, never identity. Move the sidecar and it is the same work
/// — which is what keeps its progress and its notes.
#[test]
fn the_same_content_at_a_new_path_is_the_same_work() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let first = sidecar(&conn, &doc("aa"), Path::new("/mnt/nas/taiyaku.json")).expect("first");
    let moved = sidecar(&conn, &doc("aa"), Path::new("/Volumes/nas/taiyaku.json")).expect("moved");
    assert_eq!(first, moved, "a remount must not duplicate the work");

    let path: String = conn
        .query_row(
            "SELECT sidecar_path FROM work WHERE id = ?1",
            [first],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        path, "/Volumes/nas/taiyaku.json",
        "the hint follows the file"
    );
}

/// Both keys miss: the archives were replaced under the same path. That is the
/// same work with new content, so the key is updated rather than a second work
/// inserted.
#[test]
fn replaced_archives_at_a_known_path_update_the_key_rather_than_inserting() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let first = sidecar(&conn, &doc("aa"), Path::new("/x/taiyaku.json")).expect("first");
    let again = sidecar(&conn, &doc("bb"), Path::new("/x/taiyaku.json")).expect("again");
    assert_eq!(first, again);

    let works: i64 = conn
        .query_row("SELECT count(*) FROM work", [], |r| r.get(0))
        .unwrap();
    assert_eq!(works, 1, "replacing the archives is not a second work");
}

/// Content matches one work while the path matches another — reachable by
/// copying a sidecar elsewhere and then replacing the original's archives.
/// Content wins, and the work that used to hold the path stops claiming it,
/// because whatever is on disk there is now the other work's document.
#[test]
fn taking_over_a_path_clears_it_from_the_work_that_held_it() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let a = sidecar(&conn, &doc("aa"), Path::new("/p1")).expect("work a");
    let b = sidecar(&conn, &doc("bb"), Path::new("/p2")).expect("work b");

    // The document whose content is A's now sits at B's path.
    let again = sidecar(&conn, &doc("aa"), Path::new("/p2")).expect("content wins");
    assert_eq!(again, a, "content decides, not path");

    let works: i64 = conn
        .query_row("SELECT count(*) FROM work", [], |r| r.get(0))
        .unwrap();
    assert_eq!(works, 2, "no third work");

    let a_path: Option<String> = conn
        .query_row("SELECT sidecar_path FROM work WHERE id = ?1", [a], |r| {
            r.get(0)
        })
        .unwrap();
    let b_path: Option<String> = conn
        .query_row("SELECT sidecar_path FROM work WHERE id = ?1", [b], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(a_path.as_deref(), Some("/p2"), "the taker holds the path");
    assert_eq!(b_path, None, "and the former holder no longer claims it");
}

/// Re-importing a document with FEWER editions than before must not leave the
/// removed edition's rows behind. Editions are derived from the document and
/// replaced wholesale, so this is the case where a diffing implementation
/// would quietly accumulate orphans.
#[test]
fn reimporting_with_fewer_editions_removes_the_ones_that_went() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();

    let two = two_edition_doc();
    let id = sidecar(&conn, &two, Path::new("/x/taiyaku.json")).expect("first import");
    let before: i64 = conn
        .query_row(
            "SELECT count(*) FROM edition WHERE work_id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(before, 2);

    // The same work — same hub members, so the same content_key — with the
    // spoke edition and its alignment gone.
    //
    // **Deliberately at a different path.** Re-importing at the same path would
    // resolve through `find_by_path` whether or not the content keys matched,
    // so the shared hub edition would be decoration: a review proved exactly
    // that by breaking the hub's sha256 and watching this test still pass. A
    // different path forces `find_by_key` to be what resolves it, which is the
    // thing this test claims to exercise.
    let one = hub_only_doc();
    let again = sidecar(&conn, &one, Path::new("/moved/taiyaku.json")).expect("reimport");
    assert_eq!(again, id, "content resolved it, not the path");

    let editions: i64 = conn
        .query_row(
            "SELECT count(*) FROM edition WHERE work_id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    let members: i64 = conn
        .query_row("SELECT count(*) FROM member", [], |r| r.get(0))
        .unwrap();
    let alignments: i64 = conn
        .query_row(
            "SELECT count(*) FROM alignment WHERE work_id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    let runs: i64 = conn
        .query_row("SELECT count(*) FROM run", [], |r| r.get(0))
        .unwrap();

    assert_eq!(editions, 1, "the removed edition is gone");
    assert_eq!(members, 1, "and its members with it");
    assert_eq!(alignments, 0, "and the alignment that referenced it");
    assert_eq!(runs, 0, "and that alignment's runs");
}

/// The opposite failure mode from `a_hand_set_edition_language_and_kind_survive_a_reimport`
/// and `a_hand_set_chapter_number_survives_a_reimport`: hand-set metadata on
/// an edition (and its member) that a reimport *drops* — the document no
/// longer mentions that edition at all — must not survive under some other
/// identity, and it must especially not leak onto the edition that *does*
/// survive. That second failure would be worse than the loss the
/// preservation fix (`150be53`) exists to prevent: a stray restore that
/// looks like it worked, on the wrong row.
///
/// `held_edition_meta`/`held_member_meta` key by `edition.key`/`member.sha256`
/// — identities the dropped "en" edition and its member no longer have once
/// `replace_editions` reinserts only what `hub_only_doc` names — so neither
/// held map has an entry that could reattach to "raw". This was checked by
/// hand once during Task 10's review and reverted rather than committed;
/// this pins it so a future change to the restore logic cannot silently
/// resurrect a dropped edition's metadata.
#[test]
fn a_hand_set_edition_that_a_reimport_drops_does_not_survive_or_leak() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();

    let two = two_edition_doc();
    let id = sidecar(&conn, &two, Path::new("/x/taiyaku.json")).expect("first import");

    conn.execute(
        "UPDATE edition SET language = 'en', kind = 'official'
         WHERE work_id = ?1 AND key = 'en'",
        [id],
    )
    .expect("hand-set metadata on the edition that will be dropped");
    conn.execute(
        "UPDATE member SET chapter_declared = 3.0, chapter_source = 'hand'
         WHERE sha256 = 'ee'",
        [],
    )
    .expect("hand-set chapter on that edition's member");

    // A reimport at the same path whose document no longer holds "en" at
    // all — not "en" with different content, "en" gone.
    let one = hub_only_doc();
    let again = sidecar(&conn, &one, Path::new("/x/taiyaku.json")).expect("reimport");
    assert_eq!(again, id, "same work, content resolved it");

    let editions: i64 = conn
        .query_row(
            "SELECT count(*) FROM edition WHERE work_id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(editions, 1, "the dropped edition must not survive");

    let en_survives: i64 = conn
        .query_row(
            "SELECT count(*) FROM edition WHERE work_id = ?1 AND key = 'en'",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(en_survives, 0, "specifically 'en', not some other row");

    let dropped_member: i64 = conn
        .query_row("SELECT count(*) FROM member WHERE sha256 = 'ee'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        dropped_member, 0,
        "the dropped edition's member must not survive either"
    );

    let (language, kind): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT language, kind FROM edition WHERE work_id = ?1 AND key = 'raw'",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        language, None,
        "the surviving edition must not inherit the dropped one's language"
    );
    assert_eq!(
        kind, None,
        "the surviving edition must not inherit the dropped one's kind"
    );

    let (chapter, source): (Option<f64>, Option<String>) = conn
        .query_row(
            "SELECT member.chapter_declared, member.chapter_source
             FROM member JOIN edition ON edition.id = member.edition_id
             WHERE edition.work_id = ?1 AND member.sha256 = 'hh'",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        chapter, None,
        "the surviving member must not inherit a chapter number"
    );
    assert_eq!(
        source, None,
        "the surviving member must not inherit a chapter source"
    );
}

#[test]
fn two_slugs_never_collide() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    sidecar(&conn, &doc("aa"), Path::new("/a/taiyaku.json")).expect("a");
    sidecar(&conn, &doc("bb"), Path::new("/b/taiyaku.json")).expect("b");

    let slugs: i64 = conn
        .query_row("SELECT count(DISTINCT slug) FROM work", [], |r| r.get(0))
        .unwrap();
    assert_eq!(slugs, 2, "two works must have two slugs");
}

/// The four tests above use `doc`'s minimal fixture on purpose, to keep them
/// obviously about the matching rule. None of them holds a second edition, a
/// page flag, an alignment, or a paired run, so none of that code in this
/// module — `replace_editions`' two page-flag loops, all of `replace_alignments`,
/// and `direction_text`'s `ltr` arm — runs under any of them.
///
/// This test closes that gap, the way Task 5's own round trip closes it for
/// `project::sidecar`: written by the real `import::sidecar` rather than
/// `project::tests::insert_by_hand`, and checked byte-for-byte against
/// `to_json` so a dropped or misordered field fails loudly rather than
/// quietly.
#[test]
fn a_full_sidecar_round_trips_through_import_and_projection() {
    let json = r#"{
      "taiyaku": 1,
      "work": { "title": "Kimi ni Todoke v01", "reading": "ltr" },
      "editions": {
        "raw": { "label": "Raw",
                 "members": [{ "path": "raw/v01.cbz", "sha256": "r1", "pages": 10 }],
                 "spreads": [2], "credits": [0] },
        "en":  { "label": "English",
                 "members": [{ "path": "en/v01.cbz", "sha256": "e1", "pages": 10 }] },
        "jp":  { "label": "JP alt",
                 "members": [{ "path": "jp/v01.cbz", "sha256": "j1", "pages": 10 }] }
      },
      "hub": "raw",
      "alignments": [
        { "from": "raw", "to": "en", "by": "solver",
          "solver": { "hash": "dhash-16", "floor": 0.5 },
          "quality": { "verdict": "sound", "slots": 10, "paired": 8, "avg_run": 4.0 },
          "runs": [
            { "a": "0", "b": "0" },
            { "a": "1" },
            { "a": "2L-3R", "b": "1L-2R", "margin": 0.1, "by": "hand", "note": "hand-fixed" }
          ] },
        { "from": "en", "to": "jp", "by": "hand",
          "quality": { "verdict": "fragmented", "slots": 10, "paired": 2, "avg_run": 1.0 },
          "runs": [ { "a": "0" } ] },
        { "from": "raw", "to": "jp", "by": "solver",
          "quality": { "verdict": "refused", "slots": 10, "paired": 0, "avg_run": 0.0 },
          "runs": [] }
      ]
    }"#;
    let library = Library::open_in_memory().expect("library");
    let parsed = taiyaku_core::Sidecar::from_json(json).expect("fixture parses");
    let canonical = parsed.to_json();

    let conn = library.conn();
    let work_id = sidecar(
        &conn,
        &parsed,
        Path::new("/lib/kimi-ni-todoke/taiyaku.json"),
    )
    .expect("import");
    let projected = crate::project::sidecar(&conn, work_id).expect("projection");

    assert_eq!(projected.to_json(), canonical);
}

/// The database holds a hand correction that the incoming document does not.
/// Re-import must not lose it — and it must not reimplement the rule that says
/// so, which lives in `taiyaku-core` and is already tested there.
#[test]
fn a_hand_run_in_the_database_survives_a_re_solve() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();

    let solved = two_edition_doc_with_runs(&[("0-9", Some("0-9"), None)]);
    let id = sidecar(&conn, &solved, Path::new("/x/taiyaku.json")).expect("first import");

    // A reader corrects one run. This is what `PATCH /alignments` produces.
    conn.execute("UPDATE run SET by = 'hand' WHERE ordinal = 0", [])
        .expect("correction");

    let (again, outcomes) =
        reimport(&conn, &solved, Path::new("/x/taiyaku.json")).expect("re-import");
    assert_eq!(again, id);

    let hand: i64 = conn
        .query_row("SELECT count(*) FROM run WHERE by = 'hand'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(hand, 1, "the hand correction must survive the re-solve");
    assert!(
        outcomes
            .iter()
            .any(|m| matches!(m, taiyaku_core::Merged::CarriedHandRuns(1))),
        "and the outcome must be reported rather than swallowed: {outcomes:?}"
    );
}

/// **C1a.** Before the fix, `reimport` wrote `sidecar(conn, &held, path)` —
/// `held` being the *projection of what the database already holds* — so
/// `work.title` never moved even though the incoming document said otherwise,
/// and the command printed success regardless. A re-solve wins: the incoming
/// document's `work` block must reach the row.
#[test]
fn a_hand_edited_title_in_the_incoming_document_reaches_the_database() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let path = Path::new("/x/taiyaku.json");

    let id = sidecar(&conn, &doc("aa"), path).expect("first import");

    let mut edited = doc("aa");
    edited.work.title = "A Hand-Edited Title".to_string();
    reimport(&conn, &edited, path).expect("reimport");

    let title: String = conn
        .query_row("SELECT title FROM work WHERE id = ?1", [id], |r| r.get(0))
        .unwrap();
    assert_eq!(
        title, "A Hand-Edited Title",
        "the incoming document's title must reach the database, not the held projection's"
    );
}

/// §4.1 rule 2: "the archives were replaced, so update the key rather than
/// inserting a second work." `import::sidecar` implements it directly, but
/// before C1 `reimport` was the only path any command reached, and it wrote
/// the held projection's hub back — so a hub whose bytes changed underneath
/// an existing import never moved `content_key` at all. This is the same
/// scenario `a_hand_run_on_a_spoke_whose_hub_moved_is_dropped_and_reported`
/// exercises with a hand run in play; this version has none, isolating the
/// key-update claim on its own.
#[test]
fn a_changed_hub_member_sha_updates_the_content_key_rather_than_leaving_the_old_one() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let path = Path::new("/x/taiyaku.json");

    let id = sidecar(&conn, &doc("aa"), path).expect("first import");
    let original_key: String = conn
        .query_row("SELECT content_key FROM work WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .unwrap();

    let replaced = doc("bb");
    let (again, _) = reimport(&conn, &replaced, path).expect("reimport");
    assert_eq!(
        again, id,
        "same work, archives replaced under the same path"
    );

    let new_key: String = conn
        .query_row("SELECT content_key FROM work WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .unwrap();
    let expected = crate::identity::content_key(&replaced).expect("key of the incoming document");
    assert_ne!(new_key, original_key, "the key must move with the hub");
    assert_eq!(
        new_key, expected,
        "the stored key must be the incoming document's, not the held one's"
    );
}

/// **C1a.** The hub edition's `spreads`/`credits` lists are exactly the sort
/// of fact a hand edit or a fresh `taiyaku credits --write` would change on
/// disk between imports. Before the fix they came from `held` — the
/// database's own projection of what it already had — so a hand edit to
/// either list on the hub was silently discarded on every re-solve.
#[test]
fn the_hub_editions_spreads_and_credits_lists_come_from_the_incoming_document() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let path = Path::new("/x/taiyaku.json");

    let first = doc("aa");
    let id = sidecar(&conn, &first, path).expect("first import");

    let mut edited = first.clone();
    {
        let hub = edited.editions.get_mut("raw").expect("hub edition");
        hub.spreads = vec![2, 4];
        hub.credits = vec![0, 1];
    }
    reimport(&conn, &edited, path).expect("reimport");

    let held = crate::project::sidecar(&conn, id).expect("projection");
    let hub = &held.editions["raw"];
    assert_eq!(
        hub.spreads,
        vec![2, 4],
        "the hub's spreads must come from the incoming document"
    );
    assert_eq!(
        hub.credits,
        vec![0, 1],
        "the hub's credits must come from the incoming document"
    );
}

/// **C1b, end to end at the library layer.** Adding a volume to the hub and
/// re-solving is the ordinary case for an ongoing series, and it must not
/// refuse: `merge_spoke`'s from-side guard (`core/sidecar.rs:628`) compares
/// its own stored edition at `alignment.from` against the `from_side` it is
/// handed, on `sha256`, and refuses when they differ — which they do here, on
/// purpose, since the hub gained a member. Before C1, that refusal propagated
/// out of `reimport` as a hard `Err`, which is what made `serve` exit 1
/// telling a reader to re-solve the work they had just re-solved. The fix
/// decided a re-solve wins: the stale alignment is dropped, the fresh one
/// takes its place, and the loss — a hand-corrected run, a human decision — is
/// reported rather than swallowed.
///
/// This also still catches the regression an earlier draft of this plan
/// introduced: passing the *held* document's hub instead of the *incoming*
/// one as `from_side` makes the guard's comparison compare a value against
/// itself — always equal, guard dead, and a merge would proceed as an
/// ordinary carry rather than a drop. That earlier bug went uncaught because
/// the only `reimport` test at the time re-imported an identical document,
/// where held and incoming hubs are byte-identical either way. Asserting the
/// specific `DroppedHandRuns` outcome (not merely "no error") is what would
/// catch it now: a dead guard would carry the hand run forward as
/// `CarriedHandRuns` instead.
#[test]
fn a_hand_run_on_a_spoke_whose_hub_moved_is_dropped_and_reported() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let path = Path::new("/x/taiyaku.json");

    let original = two_edition_doc_with_runs(&[("0-9", Some("0-9"), None)]);
    let id = sidecar(&conn, &original, path).expect("first import");
    let original_key: String = conn
        .query_row("SELECT content_key FROM work WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .unwrap();

    // A reader corrects the one run on the "raw" -> "en" edge. This is what
    // `PATCH /alignments` produces.
    conn.execute("UPDATE run SET by = 'hand' WHERE ordinal = 0", [])
        .expect("correction");

    // Same path, different hub bytes: the archives behind the hub were
    // replaced (or, as in C1b, a member was added). The stored runs describe
    // a page sequence that no longer exists.
    let replaced = two_edition_doc_with_runs_and_hub_sha(&[("0-9", Some("0-9"), None)], "zz");

    let (again, outcomes) =
        reimport(&conn, &replaced, path).expect("a changed hub must not refuse the reimport");
    assert_eq!(again, id, "same work, not a second one");
    assert!(
        outcomes
            .iter()
            .any(|m| matches!(m, taiyaku_core::Merged::DroppedHandRuns(1))),
        "the dropped hand run must be reported rather than swallowed: {outcomes:?}"
    );

    let new_key: String = conn
        .query_row("SELECT content_key FROM work WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_ne!(
        new_key, original_key,
        "the content key must move with the changed hub"
    );

    let hand: i64 = conn
        .query_row("SELECT count(*) FROM run WHERE by = 'hand'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        hand, 0,
        "the stale hand run must not survive under the new hub"
    );
}

/// Deviation from the plan's original pseudocode, which said `continue` and so
/// silently dropped the edition. A document that says *nothing* about an
/// edition is not a document saying it is *gone*.
#[test]
fn an_incoming_edition_with_no_hub_alignment_is_added_when_never_held() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let path = Path::new("/x/taiyaku.json");

    sidecar(&conn, &doc("aa"), path).expect("hub only");
    let (id, _) =
        reimport(&conn, &doc_with_unaligned_edition("aa", "extra"), path).expect("reimport");

    let held = crate::project::sidecar(&conn, id).expect("projection");
    assert!(
        held.editions.contains_key("extra"),
        "a new orphan edition is added"
    );
}

/// `edition.language`/`kind` are set by `librarydoc::apply`, never by the
/// sidecar — `doc` names neither. A routine reimport of the same document
/// must not null them out from under a hand-classified edition.
#[test]
fn a_hand_set_edition_language_and_kind_survive_a_reimport() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let path = Path::new("/x/taiyaku.json");

    let d = doc("aa");
    let id = sidecar(&conn, &d, path).expect("first import");

    conn.execute(
        "UPDATE edition SET language = 'ja', kind = 'raw' WHERE work_id = ?1 AND key = 'raw'",
        [id],
    )
    .expect("hand-set edition metadata");

    sidecar(&conn, &d, path).expect("reimport");

    let (language, kind): (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT language, kind FROM edition WHERE work_id = ?1 AND key = 'raw'",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        language.as_deref(),
        Some("ja"),
        "language must survive a reimport"
    );
    assert_eq!(kind.as_deref(), Some("raw"), "kind must survive a reimport");
}

/// `credit` rows on an edition cascade away the instant `replace_editions`
/// deletes that edition's row. Set by `librarydoc::apply`, never by the
/// sidecar, so a reimport must restore them rather than let the cascade
/// silently erase a translator credit.
#[test]
fn a_credit_on_an_edition_survives_a_reimport() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let path = Path::new("/x/taiyaku.json");

    let d = doc("aa");
    let id = sidecar(&conn, &d, path).expect("first import");

    conn.execute("INSERT INTO person (name) VALUES ('Jane Doe')", [])
        .expect("insert person");
    let person_id = conn.last_insert_rowid();
    let edition_id: i64 = conn
        .query_row(
            "SELECT id FROM edition WHERE work_id = ?1 AND key = 'raw'",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    conn.execute(
        "INSERT INTO credit (person_id, role, edition_id) VALUES (?1, 'translator', ?2)",
        rusqlite::params![person_id, edition_id],
    )
    .expect("hand-set credit");

    sidecar(&conn, &d, path).expect("reimport");

    let credits: i64 = conn
        .query_row(
            "SELECT count(*) FROM credit
             JOIN edition ON edition.id = credit.edition_id
             WHERE edition.work_id = ?1 AND credit.person_id = ?2 AND credit.role = 'translator'",
            rusqlite::params![id, person_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(credits, 1, "the credit must survive a reimport");
}

/// `member.chapter_declared`/`chapter_source` are exactly the design's
/// "hand-entered and expensive to backfill" data — set by
/// `librarydoc::apply` or by a person, never by the sidecar. Matched by
/// `sha256`, the content identity that survives a reimport, not by row id.
#[test]
fn a_hand_set_chapter_number_survives_a_reimport() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let path = Path::new("/x/taiyaku.json");

    let d = doc("aa");
    let id = sidecar(&conn, &d, path).expect("first import");

    conn.execute(
        "UPDATE member SET chapter_declared = 5.5, chapter_source = 'hand' WHERE sha256 = 'aa'",
        [],
    )
    .expect("hand-set chapter");

    sidecar(&conn, &d, path).expect("reimport");

    let (chapter, source): (Option<f64>, Option<String>) = conn
        .query_row(
            "SELECT member.chapter_declared, member.chapter_source
             FROM member JOIN edition ON edition.id = member.edition_id
             WHERE edition.work_id = ?1 AND member.sha256 = 'aa'",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        chapter,
        Some(5.5),
        "chapter_declared must survive a reimport"
    );
    assert_eq!(
        source.as_deref(),
        Some("hand"),
        "chapter_source must survive a reimport"
    );
}

/// The same column, reached by the route only the scanner opens: a work
/// [`scanned`] into the library with no document at all, which someone then
/// aligns for the first time.
///
/// `taiyaku_core::Member` is `{ path, sha256, pages }` — the sidecar format
/// has nowhere to carry `chapter_declared`, and `replace_editions` replaces
/// members wholesale from the document. Without preservation, the first
/// `align --write` over a scanned chapter edition erases a fact that costs a
/// full re-read of every member to recover, which is exactly why the store
/// spec put the column in migration 1.
///
/// Distinct from `a_hand_set_chapter_number_survives_a_reimport` directly
/// above in the two ways this task made reachable: the work arrives through
/// `scanned` rather than `sidecar` — so the first import claims no path and
/// the second one does — and the source is `filename`, `derive.rs`'s own,
/// rather than a person's `hand`.
#[test]
fn a_chapter_number_the_scanner_recorded_survives_a_later_align() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let id = scanned(&conn, &doc("aa"), None).expect("scan");
    conn.execute(
        "UPDATE member SET chapter_declared = 1.0, chapter_source = 'filename'",
        [],
    )
    .expect("record the chapter");

    sidecar(&conn, &doc("aa"), Path::new("/library/series/taiyaku.json")).expect("align");

    let (chapter, source): (Option<f64>, Option<String>) = conn
        .query_row(
            "SELECT chapter_declared, chapter_source FROM member
             JOIN edition ON edition.id = member.edition_id WHERE edition.work_id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("select");
    assert_eq!(chapter, Some(1.0));
    assert_eq!(source.as_deref(), Some("filename"));
}

#[test]
fn an_incoming_edition_with_no_hub_alignment_leaves_a_held_one_alone() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let path = Path::new("/x/taiyaku.json");

    let with_extra = doc_with_unaligned_edition("aa", "extra");
    let (id, _) = (
        sidecar(&conn, &with_extra, path).expect("first import"),
        Vec::<taiyaku_core::Merged>::new(),
    );

    // The same document again. The orphan edition must survive untouched
    // rather than being dropped and re-added.
    let (again, _) = reimport(&conn, &with_extra, path).expect("reimport");
    assert_eq!(again, id);

    let held = crate::project::sidecar(&conn, id).expect("projection");
    assert!(
        held.editions.contains_key("extra"),
        "the orphan edition survives"
    );
}

/// `reimport`'s doc comment names this explicitly: "a work the library has
/// not seen yet is a plain import, not a merge." Every `reimport` test above
/// first calls `sidecar` to establish the work, so none of them reaches this
/// fallback — the one where both `find_by_key` and `find_by_path` miss.
#[test]
fn reimporting_a_work_the_library_has_never_seen_is_a_plain_import() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let path = Path::new("/x/taiyaku.json");

    let (id, outcomes) = reimport(&conn, &doc("aa"), path).expect("reimport of a new work");
    assert!(
        outcomes.is_empty(),
        "nothing was held before, so there is nothing to report a merge outcome for"
    );

    let title: String = conn
        .query_row("SELECT title FROM work WHERE id = ?1", [id], |r| r.get(0))
        .expect("the work was actually inserted, not merely accepted without effect");
    assert_eq!(title, "T");
}

/// The placeholder-path hazard (`taiyaku-cli/src/library.rs`'s own doc
/// comment describes it in full, and where the fix that makes this test pass
/// actually runs in production): a `work` row `taiyaku-serve`'s
/// `works::row_id_for_write` mints so a reading position can be recorded
/// before anything was imported — a sentinel `content_key` and no `edition`
/// rows at all, found here only by `sidecar_path`. `find_by_path` matches it
/// exactly as `reimporting_a_work_the_library_has_never_seen_is_a_plain_import`'s
/// missing-row case matches nothing, one step earlier — but a row with
/// nothing to merge from is not a document `merge_spoke` can merge onto, so
/// this is a plain import over that row's *identity*: reusing its `work_id`,
/// not minting a second row and silently orphaning whatever was recorded
/// against the first.
#[test]
fn reimporting_over_a_placeholder_row_with_no_editions_is_a_plain_import_that_reuses_it() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let path = Path::new("/x/taiyaku.json");

    // Exactly the shape `taiyaku-serve`'s `works::row_id_for_write` mints:
    // a `work` row with a sentinel content_key, a real sidecar_path, and no
    // `edition` rows — that function is `pub(crate)` to `taiyaku-serve` and
    // unreachable from here, so this replicates its one `INSERT`.
    conn.execute(
        "INSERT INTO work (content_key, slug, title, reading, hub, sidecar_path)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            "bridge:placeholder",
            "placeholder",
            "Placeholder",
            "rtl",
            "raw",
            "/x/taiyaku.json",
        ],
    )
    .expect("inserting the placeholder row");
    let placeholder_id = conn.last_insert_rowid();

    let (work_id, outcomes) =
        reimport(&conn, &doc("aa"), path).expect("reimport over the placeholder");

    assert_eq!(
        work_id, placeholder_id,
        "the placeholder's row is reused, not left behind while a second row is minted"
    );
    assert!(
        outcomes.is_empty(),
        "nothing was really held before, so there is nothing to report a merge outcome for"
    );

    let held = crate::project::sidecar(&conn, work_id).expect("projection");
    assert!(
        held.editions.contains_key("raw"),
        "the incoming document's editions actually landed on the reused row"
    );

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM work", [], |r| r.get(0))
        .expect("count");
    assert_eq!(count, 1, "no second work was minted");
}

/// `slugify`'s fallback: a title with nothing ASCII-alphanumeric in it — every
/// character replaced by `-` and then trimmed away — must not become the
/// empty string, which `unique_slug`'s collision check would otherwise treat
/// as a real, empty slug.
#[test]
fn a_title_with_no_alphanumeric_characters_slugifies_to_work() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let json = r#"{"taiyaku":1,"work":{"title":"★★★","reading":"rtl"},
        "editions":{"raw":{"label":"R","members":[
          {"path":"raw/v01.cbz","sha256":"aa","pages":10}]}},
        "hub":"raw","alignments":[]}"#;
    let parsed = taiyaku_core::Sidecar::from_json(json).expect("parses");

    let id = sidecar(&conn, &parsed, Path::new("/x/taiyaku.json")).expect("import");

    let slug: String = conn
        .query_row("SELECT slug FROM work WHERE id = ?1", [id], |r| r.get(0))
        .expect("slug");
    assert_eq!(
        slug, "work",
        "a title with nothing alphanumeric in it must not slugify to an empty string"
    );
}

/// **Two alignments for one edition pair are stored, both of them, in the
/// document's order.**
///
/// This test used to assert the opposite — that the pair was refused — on the
/// strength of `alignment`'s `UNIQUE (work_id, from_key, to_key)`. That
/// constraint was a rule this store had no business making. `taiyaku_core`
/// places no uniqueness rule on `alignments`: `Sidecar::from_json` is a plain
/// serde parse, and every lookup in `sidecar.rs` resolves an edge with
/// `.find(…)` or `.position(…)`, so a document with two edges for one pair
/// has always meant "the first one", and `serve` read exactly that meaning
/// off the file before this crate existed. What the constraint actually did
/// was refuse a legal document at import with a raw SQLite message
/// (`UNIQUE constraint failed: alignment.work_id, …`) — and because `serve`
/// imports every path it is handed before serving any, one such work aborted
/// the whole server. Whole-branch review 2, I3.
///
/// So the constraint is gone and the rows are kept in insertion order, which
/// `project::alignments_for` reads back `ORDER BY id`. The document survives a
/// round trip through the database unchanged, hand edit and all, and whichever
/// edge `taiyaku_core` would have chosen from the file it still chooses from
/// the projection. Deciding *between* two edges is that crate's business, not
/// this one's — the same division this module's own doc comment draws for
/// merge rules.
#[test]
fn two_alignments_for_one_edition_pair_are_both_kept_in_document_order() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let json = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
        "editions":{
          "raw":{"label":"R","members":[{"path":"raw/v01.cbz","sha256":"aa","pages":10}]},
          "en":{"label":"E","members":[{"path":"en/v01.cbz","sha256":"bb","pages":10}]}
        },
        "hub":"raw",
        "alignments":[
          {"from":"raw","to":"en","by":"hand","runs":[{"a":"0","b":"0"}]},
          {"from":"raw","to":"en","by":"solver","solver":{"hash":"dhash-16","floor":0.5},
           "runs":[{"a":"1","b":"1"}]}
        ]}"#;
    let parsed = taiyaku_core::Sidecar::from_json(json).expect("parses");

    let work_id = sidecar(&conn, &parsed, Path::new("/x/taiyaku.json"))
        .expect("a document the format permits must import");

    let back = crate::project::sidecar(&conn, work_id).expect("projection");
    assert_eq!(back.alignments.len(), 2, "both edges must be stored");
    assert_eq!(
        back.alignments[0].by,
        taiyaku_core::Source::Hand,
        "the document's first edge must come back first — `taiyaku_core` \
         resolves a duplicate pair by taking the first"
    );
    assert_eq!(back.alignments[0].runs.len(), 1);
    assert_eq!(back.alignments[0].runs[0].a.start.page, 0);
    assert_eq!(back.alignments[1].by, taiyaku_core::Source::Solver);
    assert_eq!(back.alignments[1].runs[0].a.start.page, 1);
}

/// **A page listed twice in `spreads` is one flag, not a refusal.**
///
/// `page_flag`'s `PRIMARY KEY (edition_id, kind, page)` is a true statement
/// about the rows — a page is flagged or it is not — but a document that says
/// so twice is stating one fact twice, not contradicting itself, and
/// `Sidecar::from_json` accepts it because nothing in the format forbids it.
/// Refusing the import over it, with `UNIQUE constraint failed: page_flag…`,
/// took the whole server down for every other work on the command line
/// (review 2, I3). `INSERT OR IGNORE` keeps the row invariant and drops the
/// repetition, which is what the flag already meant: `project.rs` reads these
/// back `ORDER BY page`, so the list has been a set — order and all — since
/// the first import.
#[test]
fn a_page_repeated_in_spreads_is_flagged_once_rather_than_refused() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let json = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
        "editions":{"raw":{"label":"R","members":[
          {"path":"raw/v01.cbz","sha256":"aa","pages":10}],
          "spreads":[3,3,5],"credits":[7,7]}},
        "hub":"raw","alignments":[]}"#;
    let parsed = taiyaku_core::Sidecar::from_json(json).expect("parses");

    let work_id = sidecar(&conn, &parsed, Path::new("/x/taiyaku.json"))
        .expect("a document the format permits must import");

    let back = crate::project::sidecar(&conn, work_id).expect("projection");
    let edition = &back.editions["raw"];
    assert_eq!(
        edition.spreads,
        vec![3, 5],
        "the repeated spread page is one flag"
    );
    assert_eq!(
        edition.credits,
        vec![7],
        "and the same holds for a repeated credit page"
    );
}

/// **`held_member_meta`'s tie-break is deterministic, and `5a6bd8a` added the
/// tie-break without a test that could see it.**
///
/// Nothing forbids one archive being a member of two editions of one work — a
/// repack or an omnibus sharing a literal file with a standalone edition
/// produces exactly that, and `member_by_sha` indexes `sha256` with no
/// uniqueness constraint. The held map is keyed by hash alone, so only one of
/// the candidate rows can survive into it, and the query's
/// `ORDER BY edition.id, member.ordinal` plus `or_insert` is what picks
/// *which*: the earliest edition, and within it the earliest member. That
/// choice is arbitrary, as the function's own doc says. Being *stable* is not:
/// without the `ORDER BY`, which chapter number a reimport restores depends on
/// whatever order SQLite happens to return rows in, so the same document
/// imported twice can leave two different libraries. Reversing the `ORDER BY`
/// survived the whole suite (whole-branch review 2, I6).
///
/// The three candidate rows are found and sorted here in Rust, by `edition_id`
/// then `ordinal`, rather than by reusing the production query's own `ORDER
/// BY` — otherwise the test would agree with the implementation by
/// construction rather than by checking it.
#[test]
fn held_member_meta_keeps_the_earliest_edition_and_ordinal_for_a_shared_hash() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    // One hash, three member rows: twice in one edition, once in the other.
    let json = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
        "editions":{
          "raw":{"label":"R","members":[
            {"path":"raw/v01.cbz","sha256":"dup","pages":10},
            {"path":"raw/v01-again.cbz","sha256":"dup","pages":10}]},
          "en":{"label":"E","members":[
            {"path":"en/v01.cbz","sha256":"dup","pages":10}]}
        },
        "hub":"raw","alignments":[]}"#;
    let parsed = taiyaku_core::Sidecar::from_json(json).expect("parses");
    let work_id = sidecar(&conn, &parsed, Path::new("/x/taiyaku.json")).expect("import");

    let mut rows: Vec<(i64, i64, i64)> = {
        let mut stmt = conn
            .prepare(
                "SELECT member.id, member.edition_id, member.ordinal
                 FROM member JOIN edition ON edition.id = member.edition_id
                 WHERE edition.work_id = ?1",
            )
            .expect("preparing members");
        let found = stmt
            .query_map([work_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .expect("querying members");
        found.map(|r| r.expect("member row")).collect()
    };
    assert_eq!(rows.len(), 3, "one hash, three member rows");
    rows.sort_by_key(|(_, edition_id, ordinal)| (*edition_id, *ordinal));

    // 1.0 on the row the tie-break must choose, then 2.0 and 3.0 on the rows
    // it must not — so any other choice reads back as a different number.
    for (chapter, (member_id, _, _)) in [1.0_f64, 2.0, 3.0].into_iter().zip(&rows) {
        conn.execute(
            "UPDATE member SET chapter_declared = ?1, chapter_source = 'hand' WHERE id = ?2",
            rusqlite::params![chapter, member_id],
        )
        .expect("hand-set chapter");
    }

    let held = held_member_meta(&conn, work_id).expect("held member metadata");
    assert_eq!(
        held.get("dup").expect("the shared hash is held").declared,
        Some(1.0),
        "the earliest edition's earliest member must win, not whichever row \
         SQLite returned first"
    );
}

/// **A hash held under two distinct roots must come back NULL, not either
/// root — this is the half of `held_member_meta` that is not allowed to be
/// arbitrary.** Unlike a chapter number, where "the earliest edition wins" is
/// a coin flip with nothing at stake, a wrongly-restored `root_id` is a
/// permission-shaped fact stated wrong: an account granted only the root the
/// member did *not* come from would see a work that does not physically live
/// there, and one granted the root it actually lives under would lose it.
/// Reachable without any hypothetical setup — two roots added with
/// `library scan --add` and no `edition_key` both fall back to `raw` and
/// derivation merges their members into one edition (O54), and the same file
/// present under two roots is ordinary — so this fixture is exactly that
/// shape: one hash, two editions, each edition's copy of it scanned from a
/// different root.
///
/// Confirmed red against the code before this test's fix landed: it restored
/// one of the two roots (whichever `held_member_meta`'s earliest-edition tie-
/// break preferred) rather than NULL on both.
#[test]
fn a_hash_held_under_two_distinct_roots_comes_back_with_no_root_on_either() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let path = Path::new("/x/taiyaku.json");

    // One hash, two editions — an omnibus/standalone pair, or two roots
    // merged under the shared `raw` fallback edition key (O54).
    let json = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
        "editions":{
          "raw":{"label":"R","members":[
            {"path":"raw/v01.cbz","sha256":"dup","pages":10}]},
          "omni":{"label":"O","members":[
            {"path":"omni/v01.cbz","sha256":"dup","pages":10}]}
        },
        "hub":"raw","alignments":[]}"#;
    let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
    let work_id = sidecar(&conn, &doc, path).expect("first import");

    conn.execute("INSERT INTO root (path) VALUES ('/a')", [])
        .expect("root a");
    let root_a = conn.last_insert_rowid();
    conn.execute("INSERT INTO root (path) VALUES ('/b')", [])
        .expect("root b");
    let root_b = conn.last_insert_rowid();

    // Each edition's copy of the shared hash was resolved from a different
    // root, exactly as a scan would leave it.
    conn.execute(
        "UPDATE member SET root_id = ?1 WHERE sha256 = 'dup' AND edition_id = (
           SELECT id FROM edition WHERE work_id = ?2 AND key = 'raw')",
        rusqlite::params![root_a, work_id],
    )
    .expect("stamp raw's copy with root a");
    conn.execute(
        "UPDATE member SET root_id = ?1 WHERE sha256 = 'dup' AND edition_id = (
           SELECT id FROM edition WHERE work_id = ?2 AND key = 'omni')",
        rusqlite::params![root_b, work_id],
    )
    .expect("stamp omni's copy with root b");

    sidecar(&conn, &doc, path).expect("reimport");

    let roots: Vec<Option<i64>> = {
        let mut stmt = conn
            .prepare(
                "SELECT member.root_id FROM member
                 JOIN edition ON edition.id = member.edition_id
                 WHERE edition.work_id = ?1 AND member.sha256 = 'dup'
                 ORDER BY edition.id",
            )
            .expect("preparing");
        let found = stmt.query_map([work_id], |r| r.get(0)).expect("querying");
        found.map(|r| r.expect("row")).collect()
    };
    assert_eq!(
        roots.len(),
        2,
        "both editions must still hold the shared hash"
    );
    assert!(
        roots.iter().all(Option::is_none),
        "a hash claimed by two distinct roots must come back NULL on both \
         members rather than either root winning: got {roots:?}"
    );
}

/// **An unambiguous root must survive even when the row the chapter
/// tie-break keeps never carried it.** `root_id` used to ride along on
/// whichever row `or_insert` kept for `declared`/`source` — so when the
/// earlier-sorting row (by `edition.id, member.ordinal`) is the one with a
/// hand-set chapter and no root, and a *different*, later-sorting row is the
/// only one that ever carried a root, the representative's own `root_id` is
/// `None` and the ambiguity pass never fires (only one distinct non-null root
/// was ever seen), so the one legitimate root was silently dropped. This is
/// reachable without any contrived setup: a hand-corrected chapter on one
/// copy alongside a scanned copy of identical bytes in another edition is
/// exactly the ordinary "same hash, two editions" shape the sibling tests
/// above already establish.
///
/// Confirmed red against the code before this test's fix landed: it restored
/// `declared=Some(5.0) source=None root_id=None` — the chapter survived, the
/// root did not.
#[test]
fn an_unambiguous_root_survives_even_when_the_chapter_tie_break_row_lacks_it() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let path = Path::new("/x/taiyaku.json");

    // "aaa" sorts before "zzz" in the `BTreeMap` iteration `replace_editions`
    // walks, so "aaa" gets the lower `edition.id` and is the row
    // `held_member_meta`'s chapter tie-break keeps.
    let json = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
        "editions":{
          "aaa":{"label":"A","members":[
            {"path":"aaa/v01.cbz","sha256":"dup","pages":10}]},
          "zzz":{"label":"Z","members":[
            {"path":"zzz/v01.cbz","sha256":"dup","pages":10}]}
        },
        "hub":"aaa","alignments":[]}"#;
    let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
    let work_id = sidecar(&conn, &doc, path).expect("first import");

    conn.execute("INSERT INTO root (path) VALUES ('/scanned')", [])
        .expect("root");
    let root_id = conn.last_insert_rowid();

    // `aaa`'s copy: a hand-set chapter, never scanned under a root — the
    // edition whose copy of this file was never touched by a scan.
    conn.execute(
        "UPDATE member SET chapter_declared = 5.0, chapter_source = 'hand'
         WHERE sha256 = 'dup' AND edition_id = (
           SELECT id FROM edition WHERE work_id = ?1 AND key = 'aaa')",
        [work_id],
    )
    .expect("hand-set chapter on aaa's copy");
    // `zzz`'s copy: the one root a scan actually found it under.
    conn.execute(
        "UPDATE member SET root_id = ?1 WHERE sha256 = 'dup' AND edition_id = (
           SELECT id FROM edition WHERE work_id = ?2 AND key = 'zzz')",
        rusqlite::params![root_id, work_id],
    )
    .expect("stamp zzz's copy with the scanned root");

    let held = held_member_meta(&conn, work_id).expect("held member metadata");
    let held = held.get("dup").expect("the shared hash is held");
    assert_eq!(
        held.declared,
        Some(5.0),
        "the chapter tie-break must still keep the earlier-sorting row's \
         value, unchanged by fixing root_id"
    );
    assert_eq!(
        held.root_id,
        Some(root_id),
        "the one unambiguous root must be restored even though the row the \
         chapter tie-break kept never carried it: declared={:?} source={:?} \
         root_id={:?}",
        held.declared,
        held.source,
        held.root_id
    );
}

/// **Two rows agreeing on one root is not ambiguity.** The obvious
/// wrong-but-plausible implementation of the ambiguity check is "a hash held
/// by more than one row loses its root," rather than "a hash held by more
/// than one *distinct non-null root* loses its root" — and that version
/// passes both `held_member_meta_keeps_the_earliest_edition_and_ordinal_for_
/// a_shared_hash` and `a_hash_held_under_two_distinct_roots_comes_back_with_
/// no_root_on_either` above, since neither has two rows sharing the *same*
/// non-null root. This is that missing shape: one hash, two editions, both
/// copies scanned under the same root.
#[test]
fn two_rows_agreeing_on_the_same_root_still_restore_it() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let path = Path::new("/x/taiyaku.json");

    let json = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
        "editions":{
          "raw":{"label":"R","members":[
            {"path":"raw/v01.cbz","sha256":"dup","pages":10}]},
          "omni":{"label":"O","members":[
            {"path":"omni/v01.cbz","sha256":"dup","pages":10}]}
        },
        "hub":"raw","alignments":[]}"#;
    let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
    let work_id = sidecar(&conn, &doc, path).expect("first import");

    conn.execute("INSERT INTO root (path) VALUES ('/shared')", [])
        .expect("root");
    let root_id = conn.last_insert_rowid();

    // Both editions' copies were scanned under the same root — no
    // disagreement, so nothing here is ambiguous.
    conn.execute(
        "UPDATE member SET root_id = ?1 WHERE sha256 = 'dup'",
        [root_id],
    )
    .expect("stamp both copies with the same root");

    let held = held_member_meta(&conn, work_id).expect("held member metadata");
    assert_eq!(
        held.get("dup").expect("the shared hash is held").root_id,
        Some(root_id),
        "two rows carrying the same non-null root must still restore it, \
         not be zeroed as though they disagreed"
    );
}

/// **A document from a newer taiyaku is refused, by both public entry points,
/// before a row is written.** `Sidecar::from_json` checks nothing, so a
/// version 2 document parses on a version 1 build with every field it does not
/// understand silently discarded — and the round trip that follows
/// (`library import`, then `export --in-place`) writes it back out as a
/// version 1 document over the reader's only copy. The assertion reads the
/// message and the row count, not merely the `Err`: refusing while having
/// already inserted the work would be the same defect wearing an error.
#[test]
fn a_document_from_a_newer_taiyaku_is_refused_rather_than_silently_downgraded() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let mut newer = doc("hh");
    newer.taiyaku = taiyaku_core::VERSION + 1;

    let plain = sidecar(&conn, &newer, Path::new("/x/taiyaku.json")).map(|_| ());
    let merged = reimport(&conn, &newer, Path::new("/x/taiyaku.json")).map(|_| ());

    for (route, result) in [("sidecar", &plain), ("reimport", &merged)] {
        let message = match result {
            Ok(()) => panic!("{route} accepted a version this build cannot read"),
            Err(e) => e,
        };
        assert!(
            message.contains(&format!("version {}", taiyaku_core::VERSION + 1)),
            "{route}'s refusal must name the version it was handed: {message}"
        );
        assert!(
            message.contains(&format!("this build reads {}", taiyaku_core::VERSION)),
            "{route}'s refusal must name the version this build reads: {message}"
        );
    }

    let works: i64 = conn
        .query_row("SELECT count(*) FROM work", [], |row| row.get(0))
        .expect("counting works");
    assert_eq!(works, 0, "a refused document leaves no row behind");
}

/// The other side of the same rule: the version this build *does* write is
/// accepted, so the guard above cannot be satisfied by refusing everything.
#[test]
fn a_document_declaring_this_builds_version_still_imports() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let mut current = doc("hh");
    current.taiyaku = taiyaku_core::VERSION;
    let work_id = sidecar(&conn, &current, Path::new("/x/taiyaku.json")).expect("import");
    assert_eq!(
        crate::project::sidecar(&conn, work_id)
            .expect("projection")
            .taiyaku,
        taiyaku_core::VERSION,
        "and the projection stamps the version this build writes"
    );
}

#[test]
fn a_scanned_work_is_stored_with_no_sidecar_path() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();

    let id = scanned(&conn, &doc("aa"), None).expect("import");

    let path: Option<String> = conn
        .query_row("SELECT sidecar_path FROM work WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .expect("select");
    assert_eq!(path, None, "a scanned work claims no file on disk");
}

#[test]
fn two_scanned_works_in_one_directory_do_not_collide() {
    // The case that made this function necessary: a series directory holds
    // fourteen volumes as files, so `sidecar_path` could only ever have named
    // the directory and all fourteen would have matched each other on
    // `find_by_path`. Identity is `content_key` alone here — which is why
    // these two, sharing a title and differing only in their member digest,
    // are two works.
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();

    let first = scanned(&conn, &doc("aa"), None).expect("first");
    let second = scanned(&conn, &doc("bb"), None).expect("second");

    assert_ne!(first, second);
    let works: i64 = conn
        .query_row("SELECT count(*) FROM work", [], |r| r.get(0))
        .expect("count");
    assert_eq!(works, 2);
}

/// The library `work_id` is filed under, or `None` for none.
fn library_of(conn: &Connection, work_id: i64) -> Option<i64> {
    conn.query_row(
        "SELECT library_id FROM work WHERE id = ?1",
        [work_id],
        |r| r.get(0),
    )
    .expect("reads")
}

#[test]
fn scanned_in_finds_its_own_librarys_work_and_not_anothers() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute_batch("INSERT INTO library (id, name) VALUES (70, 'A'), (80, 'B');")
        .expect("two libraries");

    let in_a = scanned_in(&conn, &doc("aa"), 70, None).expect("into A");
    let in_b = scanned_in(&conn, &doc("aa"), 80, None).expect("into B");
    let again = scanned_in(&conn, &doc("aa"), 70, None).expect("into A again");

    assert_ne!(
        in_a, in_b,
        "the same content in another library is another work"
    );
    assert_eq!(again, in_a, "and A finds its own");
    assert_eq!(
        (library_of(&conn, in_a), library_of(&conn, in_b)),
        (Some(70), Some(80))
    );
}

#[test]
fn scanned_in_adopts_a_work_in_no_library() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute_batch("INSERT INTO library (id, name) VALUES (70, 'A');")
        .expect("a library");
    let unfiled = scanned(&conn, &doc("aa"), None).expect("a work in no library");

    let filed = scanned_in(&conn, &doc("aa"), 70, None).expect("into A");

    assert_eq!(filed, unfiled, "the same row");
    assert_eq!(library_of(&conn, filed), Some(70), "now filed");
}

#[test]
fn a_document_import_finds_a_scanned_work_in_any_library() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute_batch("INSERT INTO library (id, name) VALUES (70, 'A');")
        .expect("a library");
    let scanned_id = scanned_in(&conn, &doc("aa"), 70, None).expect("into A");

    let imported = sidecar(&conn, &doc("aa"), Path::new("/x/taiyaku.json")).expect("imports");

    assert_eq!(
        imported, scanned_id,
        "content still decides for a document, across libraries"
    );
}

/// Two works in no library holding one key, which `work`'s `UNIQUE
/// (content_key, library_id)` allows since it does not compare NULLs: two
/// libraries each held a copy and each has gone, as a library goes when its
/// last directory moves away.
fn two_unfiled_works_sharing_a_key(conn: &Connection) -> (i64, i64) {
    conn.execute_batch("INSERT INTO library (id, name) VALUES (70, 'A'), (80, 'B');")
        .expect("two libraries");
    let older = scanned_in(conn, &doc("aa"), 70, None).expect("into A");
    let newer = scanned_in(conn, &doc("aa"), 80, None).expect("into B");
    conn.execute_batch("DELETE FROM library WHERE id IN (70, 80);")
        .expect("both libraries go");
    assert!(older < newer, "the fixture: two rows, in this order");
    assert_eq!(
        (library_of(conn, older), library_of(conn, newer)),
        (None, None),
        "the fixture: both in no library"
    );
    (older, newer)
}

#[test]
fn scanned_in_adopts_the_older_of_two_works_in_no_library_and_only_that_one() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (older, newer) = two_unfiled_works_sharing_a_key(&conn);
    conn.execute_batch("INSERT INTO library (id, name) VALUES (90, 'C');")
        .expect("a third library");

    let adopted = scanned_in(&conn, &doc("aa"), 90, None).expect("into C");
    let again = scanned_in(&conn, &doc("aa"), 90, None)
        .expect("into C again, which must not file the other row beside the first");

    assert_eq!(adopted, older, "the older row is the one adopted");
    assert_eq!(again, older, "and C finds it as its own from then on");
    assert_eq!(
        (library_of(&conn, older), library_of(&conn, newer)),
        (Some(90), None),
        "one row filed, the other left in no library"
    );
}

#[test]
fn scanned_in_finds_its_own_work_before_an_older_one_in_no_library() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute_batch("INSERT INTO library (id, name) VALUES (70, 'A'), (80, 'B');")
        .expect("two libraries");
    let older = scanned_in(&conn, &doc("aa"), 70, None).expect("into A");
    let own = scanned_in(&conn, &doc("aa"), 80, None).expect("into B");
    conn.execute_batch("DELETE FROM library WHERE id = 70;")
        .expect("A goes, leaving its work in no library");
    assert_eq!(library_of(&conn, older), None, "the fixture");

    let found = scanned_in(&conn, &doc("aa"), 80, None).expect("into B again");

    assert_eq!(found, own, "B's own work, though the other row is older");
    assert_eq!(
        (library_of(&conn, older), library_of(&conn, own)),
        (None, Some(80)),
        "and the older row is left where it was"
    );
}

/// A `known` row is the caller's to choose. One of another library is still
/// the row the import lands on when content finds nothing, and it stays in
/// the library it is in.
#[test]
fn scanned_in_leaves_a_known_work_in_the_library_it_is_in() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute_batch("INSERT INTO library (id, name) VALUES (70, 'A'), (80, 'B');")
        .expect("two libraries");
    let in_a = scanned_in(&conn, &doc("aa"), 70, None).expect("into A");

    let landed = scanned_in(&conn, &doc("bb"), 80, Some(in_a)).expect("into B, known in A");

    assert_eq!(
        landed, in_a,
        "the fixture: no work holds the new key, so the known row is the answer"
    );
    assert_eq!(
        library_of(&conn, in_a),
        Some(70),
        "and it is filed where it was"
    );
}

/// With no library to look in, an import of a document no work holds takes a
/// work in a library before one in none, however old the unfiled one is, and
/// then the oldest.
#[test]
fn a_document_import_prefers_a_filed_work_and_then_the_oldest() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute_batch("INSERT INTO library (id, name) VALUES (70, 'A'), (80, 'B'), (90, 'C');")
        .expect("three libraries");
    let unfiled = scanned_in(&conn, &doc("aa"), 70, None).expect("into A");
    let older = scanned_in(&conn, &doc("aa"), 80, None).expect("into B");
    let newer = scanned_in(&conn, &doc("aa"), 90, None).expect("into C");
    conn.execute_batch("DELETE FROM library WHERE id = 70;")
        .expect("A goes, leaving the oldest row in no library");
    assert!(unfiled < older && older < newer, "the fixture");

    let imported = sidecar(&conn, &doc("aa"), Path::new("/x/taiyaku.json")).expect("imports");

    assert_eq!(imported, older, "the older of the two filed works");
}

/// The document at `document` imported, with a position and a note on its
/// work; that work taken by library A's scan; the same content scanned into
/// library B; and A gone, as a library goes with its last directory. Returns the
/// document's work, which is the older and in no library, and B's.
fn a_documents_work_in_no_library_beside_a_filed_one(
    conn: &Connection,
    document: &Path,
) -> (i64, i64) {
    conn.execute_batch("INSERT INTO library (id, name) VALUES (70, 'A'), (80, 'B');")
        .expect("two libraries");
    let (held, _) = reimport(conn, &doc("aa"), document).expect("imports the document");
    conn.execute_batch(&format!(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, {held}, 'raw', 2);
         INSERT INTO note (user_id, id, work_id, edition, slot, body, created, updated)
         VALUES (1, 'n', {held}, 'raw', 1, 'kept', 'now', 'now');"
    ))
    .expect("a position and a note on the document's work");
    let taken = scanned_in(conn, &doc("aa"), 70, None).expect("A's scan");
    let filed = scanned_in(conn, &doc("aa"), 80, None).expect("B's scan");
    conn.execute_batch("DELETE FROM library WHERE id = 70;")
        .expect("A goes, leaving the document's work in no library");
    assert_eq!(
        (taken, library_of(conn, held), library_of(conn, filed)),
        (held, None, Some(80)),
        "the fixture: the document's work in no library, and B's work of the same content"
    );
    assert_eq!(
        works_holding(conn, document),
        vec![held],
        "the fixture: the document's work holds its path"
    );
    (held, filed)
}

/// The works whose `sidecar_path` is `document`, oldest first.
fn works_holding(conn: &Connection, document: &Path) -> Vec<i64> {
    conn.prepare("SELECT id FROM work WHERE sidecar_path = ?1 ORDER BY id")
        .expect("prepares")
        .query_map([document.to_string_lossy()], |r| r.get(0))
        .expect("reads")
        .collect::<rusqlite::Result<_>>()
        .expect("reads")
}

/// A document that already has a work lands on that work again, though it is
/// in no library and another work of the same content is filed. The work
/// still holds the document's path, and the position and the note.
#[test]
fn a_document_lands_on_the_work_that_already_holds_its_path() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let document = Path::new("/a/series/taiyaku.json");
    let (held, _filed) = a_documents_work_in_no_library_beside_a_filed_one(&conn, document);

    let (again, _) = reimport(&conn, &doc("aa"), document).expect("imports again");

    assert_eq!(again, held, "the work that already holds the document");
    assert_eq!(
        works_holding(&conn, document),
        vec![held],
        "and it holds the document's path still"
    );
    let on = |table: &str| -> i64 {
        conn.query_row(
            &format!("SELECT count(*) FROM {table} WHERE work_id = ?1"),
            [again],
            |r| r.get(0),
        )
        .expect("counts")
    };
    assert_eq!(
        (on("progress"), on("note")),
        (1, 1),
        "the position and the note are on the work the document landed on"
    );
}

/// A document no work holds lands by the order every such document does: on
/// the filed work, though the older work of that content holds the path of
/// another document.
#[test]
fn a_document_no_work_holds_is_not_drawn_to_another_documents_work() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (_held, filed) = a_documents_work_in_no_library_beside_a_filed_one(
        &conn,
        Path::new("/a/series/taiyaku.json"),
    );

    let imported =
        sidecar(&conn, &doc("aa"), Path::new("/b/series/taiyaku.json")).expect("imports");

    assert_eq!(imported, filed, "the filed work before the one in none");
}

/// The same where the work holding the document is in a library, and an older
/// work of that content is in another: the document lands on its holder, where
/// the order for a document no work holds would answer the older.
#[test]
fn a_document_lands_on_its_holder_though_an_older_work_is_filed_too() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute_batch("INSERT INTO library (id, name) VALUES (70, 'A'), (80, 'B');")
        .expect("two libraries");
    let older = scanned_in(&conn, &doc("aa"), 70, None).expect("into A");
    let holder = scanned_in(&conn, &doc("aa"), 80, None).expect("into B");
    conn.execute_batch("DELETE FROM library WHERE id = 70;")
        .expect("A goes, leaving the older work in no library");
    let document = Path::new("/b/series/taiyaku.json");
    let first = sidecar(&conn, &doc("aa"), document).expect("imports onto B's work");
    conn.execute_batch("INSERT INTO library (id, name) VALUES (90, 'C');")
        .expect("a third library");
    let adopted = scanned_in(&conn, &doc("aa"), 90, None).expect("into C");
    assert_eq!(
        (
            first,
            adopted,
            library_of(&conn, older),
            library_of(&conn, holder),
            works_holding(&conn, document),
        ),
        (holder, older, Some(90), Some(80), vec![holder]),
        "the fixture: both works filed, the older in C, and B's holding the document"
    );

    let again = sidecar(&conn, &doc("aa"), document).expect("imports again");

    assert_eq!(
        again, holder,
        "the work that holds the document, not the older"
    );
    assert_eq!(works_holding(&conn, document), vec![holder]);
}

/// A scan names no document, so a path plays no part in its lookup: a library
/// finds its own work, which holds a document's path, before a work in no
/// library, which holds none.
#[test]
fn scanned_in_finds_its_own_work_though_it_holds_a_documents_path() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute_batch("INSERT INTO library (id, name) VALUES (70, 'A'), (80, 'B');")
        .expect("two libraries");
    let own = scanned_in(&conn, &doc("aa"), 80, None).expect("into B");
    let document = Path::new("/b/series/taiyaku.json");
    let imported = sidecar(&conn, &doc("aa"), document).expect("imports onto B's work");
    let unfiled = scanned_in(&conn, &doc("aa"), 70, None).expect("into A");
    conn.execute_batch("DELETE FROM library WHERE id = 70;")
        .expect("A goes, leaving its work in no library");
    assert_eq!(
        (
            imported,
            works_holding(&conn, document),
            library_of(&conn, unfiled)
        ),
        (own, vec![own], None),
        "the fixture: B's work holds a path, and the work in no library does not"
    );

    let found = scanned_in(&conn, &doc("aa"), 80, None).expect("into B again");

    assert_eq!(found, own, "B's own work");
    assert_eq!(library_of(&conn, unfiled), None, "and the other is left");
}

#[test]
fn aligning_a_scanned_work_later_lands_on_the_same_row() {
    // The whole point: the scanner records a volume, someone aligns it, and
    // `align --write` reaches the same work rather than minting a second —
    // so the scan's series, volume and any reading position come with it.
    // `two_edition_doc`'s hub member sha is `hh`, matching `doc("hh")`, and
    // `content_key` hashes the hub edition's members alone.
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let scanned_id = scanned(&conn, &doc("hh"), None).expect("scan");

    let aligned_id = sidecar(
        &conn,
        &two_edition_doc(),
        Path::new("/library/series/taiyaku.json"),
    )
    .expect("align");

    assert_eq!(scanned_id, aligned_id, "the same volume, not a second work");
    let path: Option<String> = conn
        .query_row(
            "SELECT sidecar_path FROM work WHERE id = ?1",
            [aligned_id],
            |r| r.get(0),
        )
        .expect("select");
    assert_eq!(path.as_deref(), Some("/library/series/taiyaku.json"));
    let works: i64 = conn
        .query_row("SELECT count(*) FROM work", [], |r| r.get(0))
        .expect("count");
    assert_eq!(works, 1);
}

#[test]
fn rescanning_an_aligned_work_does_not_erase_its_sidecar_path() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let id = sidecar(&conn, &doc("aa"), Path::new("/library/series/taiyaku.json")).expect("align");

    scanned(&conn, &doc("aa"), None).expect("rescan");

    let path: Option<String> = conn
        .query_row("SELECT sidecar_path FROM work WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .expect("select");
    assert_eq!(path.as_deref(), Some("/library/series/taiyaku.json"));
}

/// A scan-shaped document over the same members: **what `derive.rs` actually
/// builds**, which is not this document with its extras stripped.
///
/// `derive::write_work` names the work after the thing on disk —
/// `title_of` takes the last component of the hub member's path when no rule
/// captured a title — sets `Direction::RightToLeft` unconditionally, because
/// neither `root` nor `rule` carries a reading direction to set, and labels
/// every edition with its own key, because neither carries a display name
/// either. A scan's `work` block and its labels are therefore *different
/// values*, not the document's own carried through.
///
/// **This used to clone the document and clear only `alignments`, `spreads`
/// and `credits`**, while its doc comment claimed to be "exactly what
/// `derive.rs` builds". It kept the document's `work.title`, `work.reading`
/// and edition labels — precisely the three fields derivation does not
/// preserve — so every `Scope::Scan` test was blind to an overwrite by
/// construction, and a rescan rewriting a reader's own `taiyaku.json` passed
/// all 254 of them (whole-branch review, C2).
fn as_scanned(doc: &taiyaku_core::Sidecar) -> taiyaku_core::Sidecar {
    let mut scan = doc.clone();
    scan.alignments.clear();
    scan.work.title = doc.editions[&doc.hub].members[0]
        .path
        .rsplit('/')
        .next()
        .expect("rsplit yields at least one part")
        .to_string();
    scan.work.reading = taiyaku_core::Direction::RightToLeft;
    for (key, edition) in &mut scan.editions {
        edition.label.clone_from(key);
        edition.spreads.clear();
        edition.credits.clear();
    }
    scan
}

/// A document whose work block and edition label are a **person's**: a title
/// naming the volume in words, a left-to-right reading direction, and an
/// edition called `Japanese raw` rather than `raw`.
///
/// Shares its hub member's `sha256` with `doc("aa")`, and so its
/// `content_key`, which is what lets a scan of the same archives land on this
/// exact row. Each of the four values is one a scan would otherwise have
/// written over: measured through `library import`, `library scan` and
/// `library export --in-place`, which rewrote the reader's own `taiyaku.json`
/// from `"Go-Toubun no Hanayome, volume 1"` / `ltr` / `"Japanese raw"` to
/// `"Series - v01"` / `rtl` / `"raw"`.
fn authored_doc() -> taiyaku_core::Sidecar {
    let json = r#"{"taiyaku":1,
         "work":{"title":"Go-Toubun no Hanayome, volume 1","reading":"ltr"},
         "editions":{"raw":{"label":"Japanese raw","members":[
           {"path":"raw/v01.cbz","sha256":"aa","pages":10}]}},
         "hub":"raw","alignments":[]}"#;
    taiyaku_core::Sidecar::from_json(json).expect("parses")
}

#[test]
fn a_rescan_leaves_the_work_block_a_document_established() {
    // A scan derives a title from a filename and a direction from a constant.
    // A document carries what a person or the aligner established, and
    // `scanned`'s own doc enumerates what a scan speaks for — the work block
    // is not on that list. `hub` goes with them: it decides identity, since
    // `content_key` hashes the hub edition's members.
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let authored = authored_doc();
    let id = sidecar(&conn, &authored, Path::new("/x/taiyaku.json")).expect("import");

    let again = scanned(&conn, &as_scanned(&authored), None).expect("rescan");

    assert_eq!(again, id, "content resolved it to the same work");
    let (title, reading, hub): (String, String, String) = conn
        .query_row(
            "SELECT title, reading, hub FROM work WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("work");
    assert_eq!(title, "Go-Toubun no Hanayome, volume 1");
    assert_eq!(reading, "ltr", "not the scanner's rtl constant");
    assert_eq!(hub, "raw");
    // The key still moves — that is the one thing a scan does speak for here,
    // and asserting the row is untouched wholesale would pass just as well
    // against an import that never ran.
    let key: String = conn
        .query_row("SELECT content_key FROM work WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .expect("work");
    assert_eq!(
        key,
        crate::identity::content_key(&authored).expect("key"),
        "the same archives, so the same key — but written, not merely left"
    );
}

#[test]
fn a_rescan_leaves_the_label_a_document_gave_an_edition() {
    // `derive::write_work` writes `label: key.clone()`, because there is no
    // `set_label` column for a rule to fill — so the label a scan carries is
    // never a name anybody chose, and taking it renames `Japanese raw` to
    // `raw` on the next routine rescan.
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let authored = authored_doc();
    let id = sidecar(&conn, &authored, Path::new("/x/taiyaku.json")).expect("import");

    scanned(&conn, &as_scanned(&authored), None).expect("rescan");

    let label: String = conn
        .query_row(
            "SELECT label FROM edition WHERE work_id = ?1 AND key = 'raw'",
            [id],
            |r| r.get(0),
        )
        .expect("edition");
    assert_eq!(label, "Japanese raw");
}

#[test]
fn a_document_still_names_its_own_editions_labels() {
    // The other half, and the half a blanket carry would break: an edition's
    // label in a `taiyaku.json` is a person's to change, and only a scan is
    // barred from writing one.
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let path = Path::new("/x/taiyaku.json");
    let id = sidecar(&conn, &authored_doc(), path).expect("import");

    let mut renamed = authored_doc();
    renamed
        .editions
        .get_mut("raw")
        .expect("the hub edition")
        .label = "Raws (2019)".to_string();
    sidecar(&conn, &renamed, path).expect("a second import");

    let label: String = conn
        .query_row(
            "SELECT label FROM edition WHERE work_id = ?1 AND key = 'raw'",
            [id],
            |r| r.get(0),
        )
        .expect("edition");
    assert_eq!(label, "Raws (2019)");
}

/// **Step 6c.** `update_work`'s `Scope::Document` branch used to overwrite
/// `title` from the sidecar on every reimport — routine, since `serve`
/// reimports on every start — even though the sidecar's title is a derived
/// value and a person may have since corrected it through `librarydoc::apply`.
#[test]
fn a_document_reimport_over_a_hand_set_title_preserves_its_provenance() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let path = Path::new("/x/taiyaku.json");
    let first = authored_doc();
    let id = sidecar(&conn, &first, path).expect("first import");

    // A person corrects the work's title through `librarydoc::apply`.
    let mut hand_doc = crate::librarydoc::of(&conn, id).expect("projecting");
    hand_doc.title = Some("A Person's Correction".to_string());
    crate::librarydoc::apply(&conn, id, &hand_doc).expect("applying");

    // The sidecar itself is edited — an aligner re-titling it, say — and
    // reimported. This must not overwrite the hand correction, and must still
    // record what the sidecar now says as the derived answer.
    let mut resolved = first.clone();
    resolved.work.title = "A Different Derived Title".to_string();
    sidecar(&conn, &resolved, path).expect("reimport");

    let (title, title_hand, title_derived): (String, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT title, title_hand, title_derived FROM work WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("work row");

    assert_eq!(
        title, "A Person's Correction",
        "the hand title must survive a routine reimport"
    );
    assert_eq!(
        title_hand.as_deref(),
        Some("A Person's Correction"),
        "provenance must not be wiped by a routine reimport"
    );
    assert_eq!(
        title_derived.as_deref(),
        Some("A Different Derived Title"),
        "title_derived must track the sidecar's own current title, not the \
         hand text the safety net exists to fall back from"
    );
}

/// **Task 2, closing review.** The rescan test below and the title-reimport
/// test above each cover one half of `replace_editions`'s `Scope::Document`
/// arm's neighbour; neither covers a hand-set **label** surviving a
/// **document** reimport, the one combination `held.label_derived` could be
/// substituted into `edition.label.clone()` in the `Scope::Document` match
/// arm without any existing test noticing.
#[test]
fn a_document_reimport_over_a_hand_set_label_preserves_its_provenance() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let path = Path::new("/x/taiyaku.json");
    let first = authored_doc();
    let id = sidecar(&conn, &first, path).expect("first import");

    // A person corrects the edition's label through `librarydoc::apply`.
    let mut hand_doc = crate::librarydoc::of(&conn, id).expect("projecting");
    hand_doc.editions.insert(
        "raw".to_string(),
        crate::librarydoc::EditionMeta {
            label: Some("A Person's Correction".to_string()),
            ..Default::default()
        },
    );
    crate::librarydoc::apply(&conn, id, &hand_doc).expect("applying");

    // The sidecar itself is edited — an aligner relabelling it, say — and
    // reimported. This must not overwrite the hand correction, and must
    // still record what the document now says as the derived answer.
    let mut resolved = first.clone();
    resolved
        .editions
        .get_mut("raw")
        .expect("the hub edition")
        .label = "A Different Derived Label".to_string();
    sidecar(&conn, &resolved, path).expect("reimport");

    let (label, label_hand, label_derived): (String, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT label, label_hand, label_derived FROM edition
             WHERE work_id = ?1 AND key = 'raw'",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("edition row");

    assert_eq!(
        label, "A Person's Correction",
        "the hand label must survive a routine document reimport"
    );
    assert_eq!(
        label_hand.as_deref(),
        Some("A Person's Correction"),
        "provenance must not be wiped by a routine reimport"
    );
    assert_eq!(
        label_derived.as_deref(),
        Some("A Different Derived Label"),
        "label_derived must track the document's own current label, not the \
         hand text the safety net exists to fall back from, and not the \
         label held from before this reimport either"
    );
}

/// **Step 6c.** `held_edition_meta` used to select the effective `label`
/// column and neither provenance column, and `replace_editions` bound that one
/// value to both `label` and `label_derived` on the reinsert every scan
/// performs. So a routine rescan over a hand-set label wiped `label_hand` to
/// NULL *and* rebound `label_derived` to the hand text — the safety net
/// overwritten with the very value it exists to fall back from.
#[test]
fn a_rescan_over_a_hand_set_label_preserves_its_provenance() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let authored = authored_doc();
    let id = sidecar(&conn, &authored, Path::new("/x/taiyaku.json")).expect("import");

    // A person corrects the edition's label through `librarydoc::apply`,
    // which is the only production route onto `label_hand`.
    let mut hand_doc = crate::librarydoc::of(&conn, id).expect("projecting");
    hand_doc.editions.insert(
        "raw".to_string(),
        crate::librarydoc::EditionMeta {
            label: Some("Hand-Set Raw".to_string()),
            ..Default::default()
        },
    );
    crate::librarydoc::apply(&conn, id, &hand_doc).expect("applying");

    // A routine rescan follows — `serve` runs one on every start.
    scanned(&conn, &as_scanned(&authored), None).expect("rescan");

    let (label, label_hand, label_derived): (String, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT label, label_hand, label_derived FROM edition
             WHERE work_id = ?1 AND key = 'raw'",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("edition row");

    assert_eq!(
        label, "Hand-Set Raw",
        "the hand label must survive a rescan"
    );
    assert_eq!(
        label_hand.as_deref(),
        Some("Hand-Set Raw"),
        "provenance must not be wiped by a routine rescan, a walk that has no \
         display name of its own to give"
    );
    assert_eq!(
        label_derived.as_deref(),
        Some("Japanese raw"),
        "label_derived must still hold what the document said, not the hand \
         text the safety net exists to fall back from"
    );
}

#[test]
fn a_rescan_that_no_longer_names_an_edition_leaves_it_standing() {
    // "The scan never saw this edition" and "a rule edit removed it" are one
    // statement from inside `scanned`, and the two outcomes are not
    // comparable: a deleted hand-aligned edition is unrecoverable, an
    // orphaned one is visible and fixable. So the orphan stands, and
    // `DeriveReport::kept_editions` is what says it is there.
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let two = two_edition_doc();
    let id = sidecar(&conn, &two, Path::new("/x/taiyaku.json")).expect("align --write");

    // The next scan finds only the raw archives: the English release moved
    // out of every root the library watches.
    let mut gone = as_scanned(&two);
    gone.editions.remove("en");
    scanned(&conn, &gone, None).expect("rescan");

    let held = crate::project::sidecar(&conn, id).expect("projects");
    assert!(
        held.editions.contains_key("en"),
        "the edition a scan stopped finding is not a deletion instruction"
    );
    assert_eq!(
        held.editions["en"].members.len(),
        1,
        "and its member came with it"
    );
    assert_eq!(held.alignments.len(), 1, "so the edge still resolves");
    assert_eq!(
        held.runs_of(&held.alignments[0])
            .expect("resolvable, because both editions are there")
            .len(),
        1
    );
}

/// A hub edition carrying a spread and two credit pages, so a rescan has
/// something to lose.
fn doc_with_flags() -> taiyaku_core::Sidecar {
    let json = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
         "editions":{"raw":{"label":"R","members":[
           {"path":"raw/v01.cbz","sha256":"aa","pages":10}],
           "spreads":[4],"credits":[8,9]}},
         "hub":"raw","alignments":[]}"#;
    taiyaku_core::Sidecar::from_json(json).expect("parses")
}

#[test]
fn a_rescan_leaves_an_alignment_a_person_made_alone() {
    // The scanner walks a filesystem. It cannot know how two editions
    // correspond, so it must not be the thing that deletes the answer: before
    // this, `scanned` passed `alignments: Vec::new()` into a wholesale
    // replace, and a rescan after an `align --write` dropped every run —
    // hand-corrected ones included, which no re-solve recovers.
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let two = two_edition_doc();
    let id = sidecar(&conn, &two, Path::new("/x/taiyaku.json")).expect("align --write");

    let again = scanned(&conn, &as_scanned(&two), None).expect("rescan");

    assert_eq!(again, id, "content resolved it to the same work");
    let (edges, runs): (i64, i64) = conn
        .query_row(
            "SELECT (SELECT count(*) FROM alignment WHERE work_id = ?1),
                    (SELECT count(*) FROM run JOIN alignment ON alignment.id = run.alignment_id
                      WHERE alignment.work_id = ?1)",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("count");
    assert_eq!(edges, 1, "the edge survived the rescan");
    assert_eq!(runs, 1, "and so did its run");
    let by: String = conn
        .query_row("SELECT by FROM alignment WHERE work_id = ?1", [id], |r| {
            r.get(0)
        })
        .expect("by");
    assert_eq!(by, "hand", "and it is still the one a person made");
}

#[test]
fn a_rescan_leaves_the_credit_pages_a_run_of_credits_found_alone() {
    // `taiyaku credits --write` writes `page_flag` rows through the same
    // `replace_editions` a rescan reaches, and a walk has no opinion about
    // which pages are a scanlation group's donation cards. Losing them costs
    // a re-run of a detector over every page of the edition.
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let flagged = doc_with_flags();
    let id = sidecar(&conn, &flagged, Path::new("/x/taiyaku.json")).expect("credits --write");

    scanned(&conn, &as_scanned(&flagged), None).expect("rescan");

    let held: Vec<(String, i64)> = conn
        .prepare(
            "SELECT page_flag.kind, page_flag.page FROM page_flag
             JOIN edition ON edition.id = page_flag.edition_id
             WHERE edition.work_id = ?1 ORDER BY page_flag.kind, page_flag.page",
        )
        .expect("prepare")
        .query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("collect");
    assert_eq!(
        held,
        vec![
            ("credit".to_string(), 8),
            ("credit".to_string(), 9),
            ("spread".to_string(), 4),
        ]
    );
}

#[test]
fn a_document_still_replaces_both_because_a_document_speaks_for_them() {
    // The other half of the rule, and the half a union would break: a
    // `taiyaku.json` that no longer lists page 8 as a credit means page 8 is
    // not a credit, and a re-solve that produced no alignment means there is
    // none. Only `scanned` carries them across; `sidecar` replaces, as it
    // always did.
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let path = Path::new("/x/taiyaku.json");
    let id = sidecar(&conn, &doc_with_flags(), path).expect("first");
    // Asserted before the second import as well as after, or the whole test
    // passes against an import that never wrote either of them: `0 == 0` is
    // what "replaced" and "never inserted" both look like from the far side.
    let before: i64 = conn
        .query_row(
            "SELECT count(*) FROM page_flag JOIN edition ON edition.id = page_flag.edition_id
              WHERE edition.work_id = ?1",
            [id],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(before, 3, "a spread and two credit pages went in");

    sidecar(&conn, &doc("aa"), path).expect("a document that lists neither");

    let flags: i64 = conn
        .query_row(
            "SELECT count(*) FROM page_flag JOIN edition ON edition.id = page_flag.edition_id
              WHERE edition.work_id = ?1",
            [id],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(flags, 0, "the document dropped them, so the library did");

    let two = two_edition_doc();
    let other = sidecar(&conn, &two, Path::new("/y/taiyaku.json")).expect("align --write");
    let raised: i64 = conn
        .query_row(
            "SELECT count(*) FROM alignment WHERE work_id = ?1",
            [other],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(raised, 1, "and the same for the edge: it existed first");

    sidecar(&conn, &hub_only_doc(), Path::new("/y/taiyaku.json")).expect("a re-solve with none");
    let edges: i64 = conn
        .query_row(
            "SELECT count(*) FROM alignment WHERE work_id = ?1",
            [other],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(edges, 0);
}

#[test]
fn a_work_whose_spoke_edition_disappeared_still_projects_and_serves_its_hub() {
    // An edge naming an edition the work does not hold, stated rather than
    // discovered. Nothing corrupts — `runs_of` says which edition is missing,
    // `quality_of` and `trust` answer `None`, and the hub is untouched — so
    // the work still opens and re-solving is the recovery.
    //
    // **Reached through a document, because a rescan can no longer reach
    // it.** This used to rescan a work whose `en` edition had gone from every
    // root, back when `scanned` deleted every edition the incoming document
    // did not name; that deletion is C2's second finding and the edition now
    // stands. A `taiyaku.json` declaring an edge to an edition it does not
    // define is what is left, and it is an ordinary hand edit: drop an
    // edition from the document, leave the alignment that names it.
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let mut two = two_edition_doc();
    two.editions.remove("en");
    let id = sidecar(&conn, &two, Path::new("/x/taiyaku.json")).expect("import");

    let held = crate::project::sidecar(&conn, id).expect("the work still projects");
    assert!(held.editions.contains_key("raw"));
    assert!(!held.editions.contains_key("en"), "the edition really went");
    assert_eq!(held.editions["raw"].members.len(), 1, "the hub is intact");
    assert_eq!(held.editions["raw"].total_pages(), 10);
    assert_eq!(held.alignments.len(), 1, "the edge outlived what it names");

    let message = held
        .runs_of(&held.alignments[0])
        .expect_err("unresolvable, rather than a panic");
    assert!(message.contains("no edition named `en`"), "{message}");
    assert_eq!(held.quality_of("raw", "en"), None);
    assert_eq!(held.trust("raw", "en"), None);
    assert_eq!(
        held.trust("raw", "raw"),
        Some(taiyaku_core::Verdict::Sound),
        "and the hub still answers for itself"
    );
}
