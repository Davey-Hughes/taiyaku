//! Documents into rows.
//!
//! The matching rule is §4.1 of the spec: `content_key` first; on a miss,
//! `sidecar_path`; if the path hits and the key differs, the archives were
//! replaced, so the key is updated rather than a second work inserted.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension};
use taiyaku_core::{Alignment, Anchor, Direction, Edition, Half, Merged, Sidecar, Source, Verdict};

/// **A document declaring a format version this build does not know is
/// refused, before a row is written.**
///
/// The rule and its wording live on [`Sidecar::readable_version`], which
/// `Sidecar::from_json` now applies to every document any command parses —
/// this used to be the only check of it, and covered only the two entry points
/// below while `align --write` and `credits --write` read, stripped and
/// rewrote a newer document in silence.
///
/// **It is still checked here, and not redundantly.** A `Sidecar` reaches this
/// crate as a value, not as text: `from_json` guards the documents that were
/// parsed, and says nothing about one a caller built by hand or projected out
/// of some other store. The store is where tolerating a newer version stops
/// being free — import drops whatever this build has no column for, and
/// [`crate::project::sidecar`] projects those rows back out stamped
/// `taiyaku: VERSION`, so a round trip through `taiyaku library import` +
/// `export --in-place` is a *silent downgrade* over the reader's only copy —
/// which makes this the last boundary that can still refuse, rather than a
/// second opinion about the first.
fn readable_version(doc: &Sidecar) -> Result<(), String> {
    doc.readable_version()
}

/// Import one sidecar, returning the `work.id` it landed on.
///
/// # Errors
///
/// Returns an error if the document declares a format version newer than this
/// build reads (see [`readable_version`]), if `content_key` fails — a hub
/// naming no edition — or if SQLite refuses a statement.
pub fn sidecar(conn: &Connection, doc: &Sidecar, path: &Path) -> Result<i64, String> {
    readable_version(doc)?;
    let key = crate::identity::content_key(doc)?;
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("starting import transaction: {e}"))?;

    let existing = find_by_key(&tx, &key)?.or(find_by_path(&tx, path)?);

    let work_id = match existing {
        Some(id) => {
            update_work(&tx, id, doc, path, &key)?;
            id
        }
        None => insert_work(&tx, doc, path, &key)?,
    };

    // Content decided this, not path — so if some other work was still
    // claiming this path, that pointer is now false: whatever is on disk
    // there is `work_id`'s document. Clear it rather than let two rows claim
    // one address, which would make `find_by_path` arbitrary between them.
    clear_stale_path_claim(&tx, work_id, path)?;

    // Editions, members, page flags and alignments are derived from the
    // document, so they are replaced wholesale inside this transaction rather
    // than diffed. Hand corrections do not live here — they arrive through
    // `Sidecar::merge_spoke` in `reimport`, below, before this is called.
    replace_editions(&tx, work_id, doc)?;
    replace_alignments(&tx, work_id, doc)?;

    tx.commit()
        .map_err(|e| format!("committing import of work {work_id}: {e}"))?;
    Ok(work_id)
}

/// Re-import a document over a work the library already holds.
///
/// **A re-solve wins.** The incoming document's `work` block, hub name and
/// hub edition are written unconditionally — not `held`'s, the database's own
/// projection of what it already has. A changed hub means the stored runs on
/// a spoke describe a page sequence that may no longer exist, so what was
/// held is *stale*, never the other way round: refusing to start over it, the
/// way this function used to, is the C1 defect, not a safety property (see
/// this module's own tests, `a_hand_run_on_a_spoke_whose_hub_moved_is_
/// dropped_and_reported` above all).
///
/// **Spoke corrections still merge, not overwrite.** The rule that decides
/// what a re-solve does to a hand correction is `Sidecar::merge_spoke` — it
/// lives in `taiyaku-core` and is already tested there. A second
/// implementation over rows is how two mechanisms come to disagree, so this
/// reaches it: project what the library holds, merge each incoming spoke into
/// that projection — *while its stored hub is still whatever the library
/// actually held*, which is what lets `merge_spoke`'s from-side guard
/// (`core/sidecar.rs:628`) tell a genuinely stale spoke from an ordinary
/// carry — and only afterward overwrite the hub and `work` block with the
/// incoming document's own copies before writing the result back through
/// [`sidecar`], which already replaces editions and alignments wholesale
/// inside a transaction.
///
/// **`merge_spoke`'s refusal never reaches `serve` as a hard error.** When it
/// refuses — overwhelmingly because the hub moved underneath the spoke being
/// merged, so the stored runs address a page sequence that no longer exists —
/// [`drop_stale_spoke`] drops the stored edge and takes the incoming,
/// freshly-solved one instead, reporting how many hand-corrected runs were
/// lost with it: a human decision no re-solve can recover, so unlike a
/// dropped `credits` list this is always reported, never swallowed. `serve`
/// must never fail to start over a stale alignment (C1b).
///
/// **A work the library has not seen yet is a plain import, not a merge** —
/// there is nothing held to carry a correction forward from, so this defers
/// to [`sidecar`] and returns no `Merged` outcomes.
///
/// **The hub edition is not a spoke** and is skipped in the merge loop; only
/// `merge_spoke` ever touches a spoke, and it does not touch the hub.
///
/// **An incoming edition with no alignment to the hub** — solving refused it,
/// or it holds only a direct edge to some other spoke — has no `Alignment`
/// for `merge_spoke` to take, so it cannot go through the one rule that
/// decides what a re-solve does to a hand correction. Rather than drop it
/// silently or overwrite it without that protection, it is added if the
/// library has never held it — nothing to carry forward there — and
/// otherwise left exactly as the library already had it, corrections and
/// all.
///
/// # Errors
///
/// Returns an error if the incoming document declares a format version newer
/// than this build reads (see [`readable_version`]), if the projection fails,
/// if the incoming document's hub names no edition, or if SQLite refuses a
/// statement.
pub fn reimport(
    conn: &Connection,
    incoming: &Sidecar,
    path: &Path,
) -> Result<(i64, Vec<Merged>), String> {
    // Checked here as well as in `sidecar`, and not only for the paths that
    // reach it directly: on the merge path what `sidecar` finally writes is
    // `held` — this crate's own projection, which carries `VERSION` by
    // construction — so `incoming`'s declared version is gone by the time
    // `sidecar` sees a document. This is the only place it can be read.
    readable_version(incoming)?;
    let key = crate::identity::content_key(incoming)?;
    let Some(work_id) = find_by_key(conn, &key)?.or(find_by_path(conn, path)?) else {
        return Ok((sidecar(conn, incoming, path)?, Vec::new()));
    };

    // 1. project what the library holds.
    let mut held = crate::project::sidecar(conn, work_id)?;

    // A row `find_by_key`/`find_by_path` matched but whose projection holds
    // no edition under the incoming hub has nothing to merge a correction
    // forward from. The row that shape describes is `taiyaku-serve`'s
    // `works::row_id_for_write`'s placeholder — a `work` row minted so a
    // reading position could be recorded before anything was imported,
    // carrying a sentinel `content_key` and no `edition` rows at all — found
    // here by `sidecar_path` rather than by key, exactly as the hazard this
    // function's own module doc (`taiyaku-cli/src/library.rs`) describes.
    // `merge_spoke`, below, assumes `held` already carries the hub edition
    // it is merging spokes onto — true of any row a real prior import wrote,
    // since `content_key` cannot be computed without one — so a projection
    // missing it is not a document to merge into, it is one that has never
    // really existed. Reusing `work_id` is still the point (that is what
    // keeps the placeholder's progress and notes attached rather than
    // orphaned), so this defers to `sidecar`, which reaches the identical
    // row through the identical `find_by_key`/`find_by_path` lookup and
    // replaces its (empty) editions wholesale — a plain import over that
    // row's identity, not a merge with nothing to merge against.
    if !held.editions.contains_key(&incoming.hub) {
        return Ok((sidecar(conn, incoming, path)?, Vec::new()));
    }

    // `from_side` is the hub edition just freshly described by this solve —
    // `incoming`'s copy, not `held`'s. `merge_spoke`'s from-side guard exists
    // to catch the held document's hub drifting from what was actually just
    // aligned; comparing `held`'s hub against itself would make that guard a
    // no-op. `taiyaku-cli`'s own `--write` merge passes the same shape: a
    // fresh `describe_edition`, never the on-disk copy being merged into.
    //
    // This is also why `held`'s own hub is *not* synced to `incoming`'s
    // before the loop below, even though step 3 will overwrite it
    // unconditionally once the loop is done: syncing early would make every
    // comparison in this loop compare a value against itself, the exact dead
    // guard the doc comment above warns `--write` against, and the one thing
    // that would keep this function from ever noticing a hub had moved.
    let from_side = incoming
        .editions
        .get(&incoming.hub)
        .ok_or_else(|| {
            format!(
                "incoming document's hub `{}` names no edition",
                incoming.hub
            )
        })?
        .clone();

    // 2. merge each incoming spoke into the held projection, whose stored hub
    // is still whatever the library actually held.
    let mut outcomes = Vec::new();
    for (name, edition) in &incoming.editions {
        if *name == incoming.hub {
            continue;
        }
        let Some(alignment) = incoming
            .alignments
            .iter()
            .find(|a| a.from == incoming.hub && a.to == *name)
        else {
            if !held.editions.contains_key(name) {
                held.editions.insert(name.clone(), edition.clone());
            }
            continue;
        };
        let outcome = held.merge_spoke(
            incoming.work.reading,
            Anchor::Hub,
            &from_side,
            name,
            edition.clone(),
            alignment.clone(),
        );
        outcomes.push(match outcome {
            Ok(merged) => merged,
            // C1b: a re-solve wins, and `serve` must never fail to start over
            // a stale alignment. Whatever `held` stored for this spoke could
            // not be safely carried forward — overwhelmingly because the hub
            // moved underneath it — so this drops it and takes the incoming,
            // freshly-solved edge instead, reporting the hand-corrected runs
            // it cost. The message itself is not inspected: `merge_spoke`'s
            // `Result<Merged, String>` is a plain string precisely because no
            // caller before this one needed to branch on why it refused
            // (`NotCorrected`, a few lines away in `core/sidecar.rs`, is the
            // typed refusal that exists where a caller does), and this
            // caller does not distinguish either — every refusal means the
            // same thing here: the stored edge cannot be trusted, so it goes.
            Err(_) => drop_stale_spoke(
                &mut held,
                &incoming.hub,
                name,
                edition.clone(),
                alignment.clone(),
            ),
        });
    }

    // 3. the incoming document's work block, hub name and hub edition carry
    // forward unconditionally — a re-solve wins, and a changed hub means the
    // *old* projection was the stale one, not the new one.
    held.work = incoming.work.clone();
    held.hub = incoming.hub.clone();
    held.editions.insert(incoming.hub.clone(), from_side);

    // 4. write the merged document back.
    sidecar(conn, &held, path)?;
    Ok((work_id, outcomes))
}

/// A spoke `merge_spoke` refused to merge onto `held` — see `reimport`'s own
/// doc comment for why this is never propagated as a hard error. Drops
/// whatever stored edge exists between `hub` and `name` and replaces it with
/// the incoming, freshly-solved `edition`/`alignment` — exactly what
/// `merge_spoke` would have done had it not refused — and reports the loss in
/// the same vocabulary `merge_spoke` itself uses: `Added` or `Replaced` when
/// nothing of value was lost, `DroppedHandRuns` when a hand-corrected run
/// was.
///
/// **Scoped to the `hub` <-> `name` edge specifically**, not every alignment
/// mentioning `name` — the same scope `merge_spoke`'s own O13 stored-edge
/// check uses, and narrower than its C1-core "any alignment mentioning
/// `name`" guard, which exists to protect a *third* edition's edge from a
/// spoke replacement this function is not performing. The edge this function
/// drops is the one `reimport`'s caller just tried to re-solve; a direct edge
/// to some other edition is untouched, stale or not, exactly as an ordinary
/// `merge_spoke` success would leave it.
fn drop_stale_spoke(
    held: &mut Sidecar,
    hub: &str,
    name: &str,
    edition: Edition,
    alignment: Alignment,
) -> Merged {
    let dropped_hand_runs: usize = held
        .alignments
        .iter()
        .filter(|a| (a.from == hub && a.to == name) || (a.from == name && a.to == hub))
        .map(|a| a.runs.iter().filter(|r| r.by == Some(Source::Hand)).count())
        .sum();
    held.alignments
        .retain(|a| !((a.from == hub && a.to == name) || (a.from == name && a.to == hub)));
    let replaced = held.editions.insert(name.to_string(), edition).is_some();
    held.alignments.push(alignment);
    match (replaced, dropped_hand_runs) {
        (_, n) if n > 0 => Merged::DroppedHandRuns(n),
        (true, _) => Merged::Replaced,
        (false, _) => Merged::Added,
    }
}

fn find_by_key(conn: &Connection, key: &str) -> Result<Option<i64>, String> {
    conn.query_row("SELECT id FROM work WHERE content_key = ?1", [key], |row| {
        row.get(0)
    })
    .optional()
    .map_err(|e| format!("looking up a work by content key: {e}"))
}

fn find_by_path(conn: &Connection, path: &Path) -> Result<Option<i64>, String> {
    let text = path.to_string_lossy();
    conn.query_row(
        "SELECT id FROM work WHERE sidecar_path = ?1",
        [text.as_ref()],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| format!("looking up a work by sidecar path: {e}"))
}

/// If some work other than `work_id` still has `sidecar_path` set to `path`,
/// null it out. Reached when content decided `work_id` is the answer while
/// `path` used to belong to a different row — that row's pointer no longer
/// describes what is on disk.
fn clear_stale_path_claim(conn: &Connection, work_id: i64, path: &Path) -> Result<(), String> {
    let text = path.to_string_lossy();
    conn.execute(
        "UPDATE work SET sidecar_path = NULL WHERE sidecar_path = ?1 AND id != ?2",
        rusqlite::params![text.as_ref(), work_id],
    )
    .map_err(|e| format!("clearing a stale sidecar path claim on `{}`: {e}", text))?;
    Ok(())
}

/// The archives at a known path were replaced: the content key moves, the
/// title and reading direction follow the document, but the slug — the
/// work's URL — is left alone rather than reslugified out from under a
/// bookmark.
fn update_work(
    conn: &Connection,
    work_id: i64,
    doc: &Sidecar,
    path: &Path,
    key: &str,
) -> Result<(), String> {
    conn.execute(
        "UPDATE work SET content_key = ?1, title = ?2, reading = ?3, hub = ?4, sidecar_path = ?5
         WHERE id = ?6",
        rusqlite::params![
            key,
            doc.work.title,
            direction_text(doc.work.reading),
            doc.hub,
            path.to_string_lossy(),
            work_id,
        ],
    )
    .map_err(|e| format!("updating work {work_id}: {e}"))?;
    Ok(())
}

fn insert_work(conn: &Connection, doc: &Sidecar, path: &Path, key: &str) -> Result<i64, String> {
    let slug = unique_slug(conn, &doc.work.title)?;
    conn.execute(
        "INSERT INTO work (content_key, slug, title, reading, hub, sidecar_path)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            key,
            slug,
            doc.work.title,
            direction_text(doc.work.reading),
            doc.hub,
            path.to_string_lossy(),
        ],
    )
    .map_err(|e| format!("inserting work: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Slugify `title` the same shape `taiyaku-serve`'s `work_id` slugifies a
/// filename — lowercased, non-alphanumerics to `-`, trimmed — with a numeric
/// suffix appended until the result is unused.
///
/// From the title rather than the filename: the sidecar beside every work in
/// this corpus is `taiyaku.json`, so slugifying the filename would collide
/// every work in a library, which is the whole reason this crate exists.
fn unique_slug(conn: &Connection, title: &str) -> Result<String, String> {
    let base = slugify(title);
    let mut candidate = base.clone();
    let mut suffix = 2;
    loop {
        let taken: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM work WHERE slug = ?1)",
                [&candidate],
                |row| row.get(0),
            )
            .map_err(|e| format!("checking slug `{candidate}` for collision: {e}"))?;
        if !taken {
            return Ok(candidate);
        }
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
}

fn slugify(title: &str) -> String {
    let lowered = title.to_lowercase();
    let slug: String = lowered
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "work".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Replace every edition, member and page flag under `work_id` with what
/// `doc` holds. `ON DELETE CASCADE` from `edition` clears `member` and
/// `page_flag` for free.
///
/// **Also restores the columns the sidecar does not own.**
/// `edition.language`/`kind`, `credit` rows on an edition, and
/// `member.chapter_declared`/`chapter_source` are hand-entered — set by
/// `librarydoc::apply` or by a person, never by `taiyaku.json` — but they
/// live on rows this function deletes and reinserts wholesale because the
/// rest of those rows *is* derived from the document. Deleting them without
/// a plan would silently null a hand-classified edition or discard a
/// hand-set chapter number on the next routine rescan, which is exactly the
/// data `<stem>.library.json` exists to keep from costing a re-derivation.
///
/// So this reads those columns before the delete and restores them after
/// the reinsert, **matching editions by `key` and members by `sha256`**
/// rather than by row id, since the ids are gone once the delete runs.
/// `sha256` is the right member identity for the same reason
/// `<stem>.library.json` uses it: it survives a path change. A key or hash
/// the incoming document no longer mentions is simply absent from the held
/// maps below, so nothing resurrects a row the document says is gone —
/// preservation applies only to identities that survive the replacement.
///
/// This mirrors what `Sidecar::merge_spoke` already does for hand-corrected
/// runs, which is this codebase's established pattern: derived data is
/// replaced, hand-entered data is carried across the replacement.
fn replace_editions(conn: &Connection, work_id: i64, doc: &Sidecar) -> Result<(), String> {
    let held_editions = held_edition_meta(conn, work_id)?;
    let held_members = held_member_meta(conn, work_id)?;

    conn.execute("DELETE FROM edition WHERE work_id = ?1", [work_id])
        .map_err(|e| format!("clearing editions of work {work_id}: {e}"))?;

    for (key, edition) in &doc.editions {
        conn.execute(
            "INSERT INTO edition (work_id, key, label) VALUES (?1, ?2, ?3)",
            rusqlite::params![work_id, key, edition.label],
        )
        .map_err(|e| format!("inserting edition `{key}`: {e}"))?;
        let edition_id = conn.last_insert_rowid();

        if let Some(held) = held_editions.get(key) {
            conn.execute(
                "UPDATE edition SET language = ?1, kind = ?2 WHERE id = ?3",
                rusqlite::params![held.language, held.kind, edition_id],
            )
            .map_err(|e| format!("restoring metadata on edition `{key}`: {e}"))?;
            for (person_id, role) in &held.credits {
                conn.execute(
                    "INSERT INTO credit (person_id, role, edition_id) VALUES (?1, ?2, ?3)",
                    rusqlite::params![person_id, role, edition_id],
                )
                .map_err(|e| format!("restoring a credit on edition `{key}`: {e}"))?;
            }
        }

        // `ordinal` is the position in the document, counting from 0.
        // `project.rs` reads members back `ORDER BY ordinal`.
        for (ordinal, member) in edition.members.iter().enumerate() {
            conn.execute(
                "INSERT INTO member (edition_id, ordinal, path, sha256, pages)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    edition_id,
                    ordinal as i64,
                    member.path,
                    member.sha256,
                    member.pages,
                ],
            )
            .map_err(|e| format!("inserting a member of `{key}`: {e}"))?;

            if let Some(chapter) = held_members.get(&member.sha256) {
                let member_id = conn.last_insert_rowid();
                conn.execute(
                    "UPDATE member SET chapter_declared = ?1, chapter_source = ?2 WHERE id = ?3",
                    rusqlite::params![chapter.declared, chapter.source, member_id],
                )
                .map_err(|e| format!("restoring a chapter on a member of `{key}`: {e}"))?;
            }
        }
        // OR IGNORE, because a page listed twice in `spreads` or `credits`
        // states one fact twice rather than contradicting itself, and nothing
        // in the format forbids it — `Sidecar::from_json` is a plain serde
        // parse. The flag has always been a set (`project.rs` reads these back
        // `ORDER BY page`), so the repetition carries nothing to lose. Plain
        // INSERT refused such a document with a raw `UNIQUE constraint failed:
        // page_flag…`, which took `serve` down for every other work on the
        // command line: review 2, I3.
        for page in &edition.spreads {
            conn.execute(
                "INSERT OR IGNORE INTO page_flag (edition_id, kind, page)
                 VALUES (?1, 'spread', ?2)",
                rusqlite::params![edition_id, page],
            )
            .map_err(|e| format!("inserting a spread flag on `{key}`: {e}"))?;
        }
        for page in &edition.credits {
            conn.execute(
                "INSERT OR IGNORE INTO page_flag (edition_id, kind, page)
                 VALUES (?1, 'credit', ?2)",
                rusqlite::params![edition_id, page],
            )
            .map_err(|e| format!("inserting a credit flag on `{key}`: {e}"))?;
        }
    }
    Ok(())
}

/// One edition's hand-entered columns, read before `replace_editions`'
/// wholesale delete so they can be restored after the reinsert. `role` is
/// carried as the raw stored text rather than parsed into an enum: it
/// already passed through `librarydoc`'s validation on the way in, and this
/// function only ever copies it back onto a fresh `credit` row, never reads
/// it as anything else.
struct HeldEdition {
    language: Option<String>,
    kind: Option<String>,
    /// `(person_id, role)` pairs — enough to reinsert each `credit` row
    /// verbatim without re-resolving the person by name.
    credits: Vec<(i64, String)>,
}

/// Every edition's hand-entered metadata currently on `work_id`, keyed by
/// `edition.key` — the identity `replace_editions` restores by, since the
/// row ids are gone once its delete runs.
fn held_edition_meta(
    conn: &Connection,
    work_id: i64,
) -> Result<HashMap<String, HeldEdition>, String> {
    let mut stmt = conn
        .prepare("SELECT id, key, language, kind FROM edition WHERE work_id = ?1")
        .map_err(|e| format!("preparing held edition metadata: {e}"))?;
    let rows = stmt
        .query_map([work_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|e| format!("querying held edition metadata: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading held edition metadata: {e}"))?;

    let mut out = HashMap::new();
    for (edition_id, key, language, kind) in rows {
        let credits = held_credits(conn, edition_id)?;
        out.insert(
            key,
            HeldEdition {
                language,
                kind,
                credits,
            },
        );
    }
    Ok(out)
}

/// One edition's credit list, **in the order it was entered**.
///
/// `ORDER BY id` for the same reason `librarydoc::credits_of` and
/// `state::truth_box` carry one: SQL has no order without it, and this list is
/// hand-entered — "Negi Haruba, then the translator, then the group" is a
/// person's decision, not a set. `replace_editions` reinserts these rows in
/// whatever order they arrive in, and they get fresh `id`s in that order, so
/// an unordered read here is the whole of what a reimport writes back.
/// Adding `DESC` used to pass all 547 `taiyaku-library` + `taiyaku-cli` tests;
/// it was right only because nothing indexes `credit.edition_id`, and this
/// path runs on every `serve` start (whole-branch review 3, Minor 1 —
/// review 2's Minor 9 left standing on the one route that had no test).
fn held_credits(conn: &Connection, edition_id: i64) -> Result<Vec<(i64, String)>, String> {
    let mut stmt = conn
        .prepare("SELECT person_id, role FROM credit WHERE edition_id = ?1 ORDER BY id")
        .map_err(|e| format!("preparing held credits: {e}"))?;
    let rows = stmt
        .query_map([edition_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("querying held credits: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading held credits: {e}"))
}

/// One member's hand-set chapter columns, read before `replace_editions`'
/// wholesale delete. Keyed by `sha256` alone in the map that holds these —
/// see `held_member_meta`'s doc comment for the work-wide-match limitation
/// that follows from that.
struct HeldChapter {
    declared: Option<f64>,
    source: Option<String>,
}

/// Every member's hand-set chapter number currently on `work_id`, keyed by
/// `sha256` — content identity, the same reason `<stem>.library.json` keys
/// its own `members` on it rather than on path (`librarydoc.rs`'s own doc
/// comment). A path or an edition can move underneath a reimport; the hash
/// does not.
///
/// **The match is work-wide, not edition-scoped — a real limitation, not a
/// hypothetical one.** Nothing forbids the same `sha256` from being a member
/// of two editions of one work: not the `Sidecar`/`Edition`/`Member` types,
/// not JSON parsing, and not the schema (`member_by_sha` indexes `sha256`
/// with no uniqueness constraint). A repack or omnibus edition that shares a
/// literal file with a standalone edition produces exactly this. When it
/// happens, only one of the two held chapter rows can survive into this map,
/// keyed as it is by hash alone — so the query below carries an explicit
/// `ORDER BY edition.id, member.ordinal`, and building the map with
/// `or_insert` keeps only the first row seen in that order: the
/// earliest-inserted edition (lowest `edition.id`, i.e. first in the
/// document), and within it the earliest member (lowest `ordinal`). That
/// choice is arbitrary — there is no principled reason the earlier edition's
/// chapter number should be the one that wins — but it is now deterministic
/// rather than depending on whatever order SQLite happens to return rows in.
/// Whether a hand-set chapter number should instead be scoped to the edition
/// it was set on, so two editions sharing a file could carry different
/// chapter numbers for it, is an open design question this function does not
/// settle.
fn held_member_meta(
    conn: &Connection,
    work_id: i64,
) -> Result<HashMap<String, HeldChapter>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT member.sha256, member.chapter_declared, member.chapter_source
             FROM member JOIN edition ON edition.id = member.edition_id
             WHERE edition.work_id = ?1
               AND (member.chapter_declared IS NOT NULL OR member.chapter_source IS NOT NULL)
             ORDER BY edition.id, member.ordinal",
        )
        .map_err(|e| format!("preparing held member chapters: {e}"))?;
    let rows = stmt
        .query_map([work_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                HeldChapter {
                    declared: row.get(1)?,
                    source: row.get(2)?,
                },
            ))
        })
        .map_err(|e| format!("querying held member chapters: {e}"))?;

    let mut out = HashMap::new();
    for row in rows {
        let (sha256, chapter) = row.map_err(|e| format!("reading held member chapters: {e}"))?;
        // First row wins: the earliest edition, earliest ordinal within it,
        // per the `ORDER BY` above. A later duplicate for the same hash is
        // dropped rather than overwriting it.
        out.entry(sha256).or_insert(chapter);
    }
    Ok(out)
}

/// Replace every alignment and run under `work_id` with what `doc` holds.
///
/// `pub(crate)` rather than private because [`crate::state::put_alignments`]
/// is the same write reached from `taiyaku-serve`'s correction route, and a
/// second implementation of these two `INSERT`s is how the column list and
/// `project.rs`'s reader come to disagree. **No transaction of its own**: both
/// callers already hold one — [`sidecar`]'s import transaction and
/// `put_alignments`' own — and `unchecked_transaction` issues a plain `BEGIN`,
/// which SQLite refuses inside an open transaction. The delete and the inserts
/// are still atomic; the caller is what makes them so.
pub(crate) fn replace_alignments(
    conn: &Connection,
    work_id: i64,
    doc: &Sidecar,
) -> Result<(), String> {
    conn.execute("DELETE FROM alignment WHERE work_id = ?1", [work_id])
        .map_err(|e| format!("clearing alignments of work {work_id}: {e}"))?;

    for alignment in &doc.alignments {
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
        .map_err(|e| {
            format!(
                "inserting alignment `{}`->`{}`: {e}",
                alignment.from, alignment.to
            )
        })?;
        let alignment_id = conn.last_insert_rowid();

        // `ordinal` is the position in the document, counting from 0.
        // `project.rs` reads runs back `ORDER BY ordinal`.
        for (ordinal, run) in alignment.runs.iter().enumerate() {
            let (b_start_page, b_start_half, b_end_page, b_end_half) = match &run.b {
                // `None` becomes NULL in `b_start_page`, which is how
                // `project.rs` tells an unpaired run from a zero-length span.
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
            .map_err(|e| format!("inserting a run: {e}"))?;
        }
    }
    Ok(())
}

fn direction_text(direction: Direction) -> &'static str {
    match direction {
        Direction::RightToLeft => "rtl",
        Direction::LeftToRight => "ltr",
    }
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

#[cfg(test)]
mod tests;
