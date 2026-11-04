//! Documents into rows.
//!
//! The matching rule is §4.1 of the spec: `content_key` first; on a miss,
//! `sidecar_path`; if the path hits and the key differs, the archives were
//! replaced, so the key is updated rather than a second work inserted.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use rusqlite::{Connection, OptionalExtension};
use taiyaku_core::{Alignment, Anchor, Direction, Edition, Half, Merged, Sidecar, Source, Verdict};

/// **A document declaring a format version this build does not know is
/// refused, before a row is written.**
///
/// The rule and its wording live on [`Sidecar::readable_version`], which
/// `Sidecar::from_json` applies to every document any command parses.
///
/// **It is checked here too, and not redundantly.** A `Sidecar` reaches this
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
    import_with(conn, doc, Some(path), Scope::Document, None)
}

/// What the document being imported is entitled to speak for, and so what an
/// import replaces.
///
/// **The two entry points differ in exactly this**, which is why it is a
/// parameter rather than two copies of `import_with`: a `taiyaku.json` is the
/// whole of what the library knows about a work's correspondence, while a scan
/// knows only which files are where.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// A document on disk. It describes editions, members, alignments and page
    /// flags alike, and all four are replaced from it.
    Document,
    /// A scan. It describes editions and members and nothing else — see
    /// [`scanned`].
    Scan,
}

/// Import a work the **scanner** derived, which has no document on disk.
///
/// **Across the store.** Content decides whichever library's directories the
/// files are under, so a byte-identical volume in two libraries lands on one
/// work (overlapping-libraries spec §3.2). Nothing here files a work under a
/// library: which libraries a work is in is read from its members' roots.
///
/// `known` is the row derivation already found for this `(series_id,
/// volume)`, or `None` for a work it has never seen. It is the second
/// identity probe, and it is the exact analogue of [`sidecar`]'s
/// `sidecar_path`: content decides first, and a known address decides on a
/// miss.
///
/// **Without it, a volume whose files change becomes a second work.**
/// `content_key` hashes the hub edition's member digests, so adding a page,
/// repacking an archive or fixing a scan changes the key, `find_by_key` misses,
/// and a new row is inserted while the reader's progress, notes and history
/// stay on the old one.
///
/// **What keeps this safe is not that the address is unique.** An address can
/// be **vacated and reoccupied**. Remove volume 1 from a root, renumber volume
/// 2 to take its place, and repack it so content cannot rescue the match, and
/// this probe hands volume 1's row — with a reader's volume-1 progress on it —
/// to what was physically volume 2.
///
/// What actually makes it right is the **order** and the **counter**:
/// `content_key` decides first, so `known` fires only for content this
/// library has never seen, and `DeriveReport::rekeyed` fires whenever it
/// does. The counterfactual is worse rather than better — without `known`
/// the same scenario mints a third row and strands the progress outright —
/// so this is a trade made deliberately, not an impossibility.
///
/// **A scan replaces the editions it names, and their members. It does not
/// touch alignments, page flags, the work block, an edition's label, or an
/// edition it did not name**, which is the difference between this and
/// [`sidecar`].
///
/// See [`update_work`] and [`replace_editions`] for two of the rules that hold
/// a scan to that, and `DeriveReport::kept_editions` for what the second costs.
///
/// Derivation owns what the facts and the rules decide: which files are members
/// of which edition of which work. It has no opinion about what a work is
/// called, which direction it reads, which pages are credits, which are
/// spreads, or how two editions correspond — those come from `align`, from
/// `credits`, or from a person, and a module that walks a filesystem must not
/// be the thing that deletes them.
///
/// This extends a principle the store already had rather than inventing one:
/// [`replace_editions`] carries `chapter_declared` across its own wholesale
/// replace, keyed on `sha256`, for exactly this reason.
///
/// **What it costs, stated rather than left to be discovered.** A rule edit
/// that removes an edition an alignment references leaves that edge naming an
/// edition the work no longer holds. Nothing corrupts: `Sidecar::edge_between`
/// and `quality_of` resolve with `.find()` and answer `None`,
/// `Sidecar::runs_of` returns `Err("no edition named …")`, and `quality_of`
/// recomputes from the runs so no stored verdict can mean anything on its own.
/// The work still projects and its hub still serves. Re-solving is the
/// recovery, and dropping the edge instead would throw away hand-corrected
/// runs that no re-solve can recover — the same trade `Sidecar::merge_spoke`
/// already makes.
///
/// `work.sidecar_path` stays NULL and identity is
/// [`crate::identity::content_key`] alone. There is no honest path to record:
/// the corpus convention is one `taiyaku.json` per work directory
/// (`docs/sidecar.md`), while a scanned series directory holds every volume as
/// a file beside its siblings — so a path-keyed identity would make them all
/// the same work.
///
/// **What this buys, and the one case where it does not.** When someone later
/// runs `align --write` over the same archives, that import matches on
/// `content_key` and lands on *this* row, carrying the scan's series, volume
/// and reading position with it. `content_key` is a hash over the **hub**
/// edition's members, so this holds exactly while the alignment's hub is the
/// edition the scan derived — the A side of `taiyaku align a... -- b...`.
/// Align the other way round and the key differs, a second work is inserted,
/// and the scanned row is left behind holding whatever progress it had. That
/// is not fixed here, because fixing it means a second identity probe over
/// *any* member rather than the hub's, which would merge two genuinely
/// different works that happen to share one file. Recorded rather than
/// guessed at: if it turns up in real use, the evidence is a duplicate pair
/// of works whose editions do not overlap.
///
/// # Errors
///
/// The same as [`sidecar`].
pub fn scanned(conn: &Connection, doc: &Sidecar, known: Option<i64>) -> Result<i64, String> {
    import_with(conn, doc, None, Scope::Scan, known)
}

fn import_with(
    conn: &Connection,
    doc: &Sidecar,
    path: Option<&Path>,
    scope: Scope,
    known: Option<i64>,
) -> Result<i64, String> {
    readable_version(doc)?;
    let key = crate::identity::content_key(doc)?;
    // `crate::Nested`, not `unchecked_transaction` directly: `derive_with`
    // calls this once per work inside `write_work`'s own scope, and
    // `preview::preview` wraps a whole `derive_with` pass in a transaction of
    // its own to roll it back in one motion — a plain `BEGIN` here would
    // refuse outright inside either. See `Nested`'s own doc for why
    // a `SAVEPOINT` cannot simply replace `unchecked_transaction` everywhere
    // instead.
    let tx = crate::Nested::open(conn, "import_with")
        .map_err(|e| format!("starting import transaction: {e}"))?;

    // Content first, and a known address on a miss — one shape, two addresses.
    // A document-backed work is addressed by the path it was read from; a
    // scanned one by the `(series_id, volume)` derivation already resolved it
    // at, which is what `known` carries. Neither is consulted while the content
    // still matches. See [`scanned`].
    let existing = match find_by_key(&tx, &key)? {
        Some(id) => Some(id),
        None => match path {
            Some(path) => find_by_path(&tx, path)?,
            None => known,
        },
    };

    let work_id = match existing {
        Some(id) => {
            update_work(&tx, id, doc, path, &key, scope)?;
            id
        }
        None => insert_work(&tx, doc, path, &key)?,
    };

    // Only meaningful when a path was claimed: content decided `work_id`, so
    // any other row still pointing at that path is now lying about disk.
    if let Some(path) = path {
        clear_stale_path_claim(&tx, work_id, path)?;
    }

    // Editions, members, page flags and alignments are derived from the
    // document, so they are replaced wholesale inside this transaction rather
    // than diffed. Hand corrections do not live here — they arrive through
    // `Sidecar::merge_spoke` in `reimport`, below, before this is called.
    //
    // **Except under `Scope::Scan`, where the last two are not the document's
    // to replace.** A scan describes which files are members of which edition;
    // it has nothing to say about how two editions correspond or which pages
    // are credits, so it neither carries those facts nor gets to delete them.
    // See [`scanned`].
    replace_editions(&tx, work_id, doc, scope)?;
    if scope == Scope::Document {
        replace_alignments(&tx, work_id, doc)?;
    }

    tx.commit()
        .map_err(|e| format!("committing import of work {work_id}: {e}"))?;
    Ok(work_id)
}

/// Re-import a document over a work the library already holds.
///
/// **A re-solve wins.** The incoming document's `work` block, hub name and hub
/// edition are written unconditionally — not `held`'s, the database's own
/// projection of what it already has. A changed hub means the stored runs on a
/// spoke describe a page sequence that may no longer exist, so what was held is
/// *stale*, never the other way round: refusing to start over it is not a
/// safety property (C1; see this module's own tests,
/// `a_hand_run_on_a_spoke_whose_hub_moved_is_dropped_and_reported` above all).
///
/// **Spoke corrections still merge, not overwrite.** The rule that decides
/// what a re-solve does to a hand correction is `Sidecar::merge_spoke` — it
/// lives in `taiyaku-core` and is already tested there. A second
/// implementation over rows is how two mechanisms come to disagree, so this
/// reaches it: project what the library holds, merge each incoming spoke into
/// that projection — *while its stored hub is still whatever the library
/// actually held*, which is what lets `merge_spoke`'s from-side guard
/// (see its `# Errors`) tell a genuinely stale spoke from an ordinary
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
/// or it holds only a direct edge to some other spoke — has no `Alignment` for
/// `merge_spoke` to take, so it cannot go through the one rule that decides
/// what a re-solve does to a hand correction. Rather than drop it silently or
/// overwrite it without that protection, it is added if the library has never
/// held it — nothing to carry forward there — and otherwise left exactly as the
/// library already had it, corrections and all.
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
    // here by `sidecar_path` rather than by key, exactly as the hazard
    // `taiyaku-cli/src/library.rs`'s module doc describes.
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
    // guard the paragraph above describes, and the one thing that would keep
    // this function from ever noticing a hub had moved.
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
            // C1b: a re-solve wins, and `serve` must never fail to start over a
            // stale alignment. Whatever `held` stored for this spoke could not
            // be safely carried forward — overwhelmingly because the hub moved
            // underneath it — so this drops it and takes the incoming,
            // freshly-solved edge instead, reporting the hand-corrected runs it
            // cost. The message itself is not inspected: `merge_spoke`'s
            // `Result<Merged, String>` carries a plain string, and every
            // refusal means the same thing here: the stored edge cannot be
            // trusted, so it goes.
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

/// The work holding `key`: `content_key` is unique across the store, so
/// content is one work whichever libraries hold it (overlapping-libraries
/// spec §3.1).
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
///
/// **Under [`Scope::Scan`] `content_key` moves.** A scan derives a title from a
/// filename and a reading direction from a constant — `derive.rs` says so in as
/// many words, `Direction::RightToLeft` because nothing in `root` or `rule`
/// carries one — while a row that already holds a title and a direction holds
/// what a person or the aligner established. So a scan is not entitled to
/// either, and the same goes for `hub`, which decides identity: `content_key`
/// hashes the hub edition's members, and a walk re-picking the hub would move a
/// work's key and its reader's position with it.
///
/// **What it costs, which is not nothing.** A work this scanner minted holds
/// a title this scanner derived, and it keeps it: edit a `regex` rule so its
/// `title` capture answers differently, or move the files so `title_of` reads
/// another name, and the row does not follow. Nothing here can tell a title a
/// person established from one an earlier derive did — `work` has no column
/// that records which — and of the two mistakes available, keeping a stale
/// derived title is the recoverable one and overwriting a person's is not.
///
/// **The other residual, stated rather than left to be found.** Where the scan
/// no longer finds the edition the row names as its hub, `derive` must pick a
/// hub among the editions it did derive — `content_key` cannot be computed over
/// an edition the document does not carry — so the key written here is over a
/// different edition than the one `work.hub` names. Nothing reads the two
/// together: the projection takes its hub from the row, and that edition is
/// still there, because [`replace_editions`] does not delete what a scan did
/// not name. It is the same case `DeriveReport::kept_editions` counts, so it is
/// reported rather than silent.
///
/// **Two statements rather than one `COALESCE` per column**, which is the
/// shape the settled wording ("`COALESCE`-style") suggests and the schema
/// refuses: `title`, `reading` and `hub` are all NOT NULL, so there is no
/// stored NULL for a COALESCE to step over. "A row that already has them
/// keeps them" is therefore "a scan does not write them", written as what it
/// is.
fn update_work(
    conn: &Connection,
    work_id: i64,
    doc: &Sidecar,
    path: Option<&Path>,
    key: &str,
    scope: Scope,
) -> Result<(), String> {
    if scope == Scope::Scan {
        // `path` is always NULL here — `scanned` passes none — so the
        // COALESCE below would already have left `sidecar_path` alone. What
        // this leaves alone as well is the work block.
        conn.execute(
            "UPDATE work SET content_key = ?1 WHERE id = ?2",
            rusqlite::params![key, work_id],
        )
        .map_err(|e| format!("updating work {work_id}: {e}"))?;
        return Ok(());
    }
    // COALESCE, not an unconditional assignment: a rescan arrives with no path
    // and must leave a path an earlier `align --write` established. The reverse
    // — a real path replacing NULL — is the ordinary assignment and still
    // happens, since COALESCE takes the non-NULL argument.
    //
    // **`title` is written the same two-column way `librarydoc::apply` writes
    // it.** The sidecar's own title is a derived value — this is a routine
    // reimport, which happens on every `serve` start — so it is recorded into
    // `title_derived` and the effective `title` falls back to it only when no
    // person has set `title_hand`. Writing `title = ?2` unconditionally would
    // overwrite a hand correction on every reimport.
    conn.execute(
        "UPDATE work SET content_key = ?1, title_derived = ?2,
                title = COALESCE(title_hand, ?2), reading = ?3, hub = ?4,
                sidecar_path = COALESCE(?5, sidecar_path)
         WHERE id = ?6",
        rusqlite::params![
            key,
            doc.work.title,
            direction_text(doc.work.reading),
            doc.hub,
            path.map(|p| p.to_string_lossy().into_owned()),
            work_id,
        ],
    )
    .map_err(|e| format!("updating work {work_id}: {e}"))?;
    Ok(())
}

fn insert_work(
    conn: &Connection,
    doc: &Sidecar,
    path: Option<&Path>,
    key: &str,
) -> Result<i64, String> {
    let slug = unique_slug(conn, &doc.work.title)?;
    conn.execute(
        "INSERT INTO work (content_key, slug, title, title_derived, reading, hub, sidecar_path)
         VALUES (?1, ?2, ?3, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            key,
            slug,
            doc.work.title,
            direction_text(doc.work.reading),
            doc.hub,
            path.map(|p| p.to_string_lossy().into_owned()),
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
/// **Under [`Scope::Scan`], "every edition" is every edition the document
/// *names*.** An edition the incoming document does not mention is left
/// standing rather than deleted, because "the scan never saw this edition"
/// and "a rule edit removed it" are the same statement from here and the
/// consequences are not: a deleted hand-aligned edition is unrecoverable,
/// while an orphaned one is visible and fixable. The cost is stated where it
/// is paid — a rule edit that renames an edition key leaves the old edition
/// in place — and counted, as `DeriveReport::kept_editions`. Under
/// [`Scope::Document`] the wholesale delete stands: a `taiyaku.json` that no
/// longer lists an edition means the work no longer has it, which is the same
/// reason a document replaces its own page flags below.
///
/// **Also restores the columns the sidecar does not own.**
/// `edition.label` under a scan, `edition.language`/`kind`, `credit` rows on
/// an edition, and
/// `member.chapter_declared`/`chapter_source` are not this document's — but
/// they live on rows this function deletes and reinserts wholesale
/// because the rest of those rows *is* derived from the document. Deleting
/// them without
/// a plan would silently null a hand-classified edition or discard a
/// hand-set chapter number on the next routine rescan, which is exactly the
/// data `<stem>.library.json` exists to keep from costing a re-derivation.
///
/// **Who does write them, per column, because they do not share a list.**
///
/// - `edition.label` — this function, out of the document it is handed, **and
///   `librarydoc::apply_editions`**, which is the hand edit itself. This
///   function's value is the document's own under [`Scope::Document`], which
///   is where a `taiyaku.json` reaches it, and the *held* value under
///   [`Scope::Scan`], which is the restore below — `derive::write_work` puts
///   `key.clone()` in the document a scan builds, and the restore is what
///   keeps that from landing over a person's `Japanese raw`. `apply_editions`
///   writes it twice over: the blanket clear restores `label` from
///   `label_derived` for an edition the document drops entirely, and the
///   per-key update sets `label_hand = ?1, label = COALESCE(?1, label_derived)`
///   for one it names.
/// - `edition.language`/`kind` — `librarydoc::apply`, `derive::write_edition_metadata`
///   under the per-work guard that paragraph describes, **and this function's
///   own restore below**: the blanket delete-and-reinsert below would silently
///   drop them from a row that survives a rescan, exactly as it would drop a
///   credit, so they are read before the delete and copied onto the fresh row
///   the same way. **Nothing writes either from a `taiyaku.json`**: `project.rs`
///   touches neither column.
/// - `credit` rows — `librarydoc::apply`, which deletes and reinserts them
///   per work, and this function's own restore below.
/// - `member.chapter_declared`/`chapter_source` — `librarydoc::apply`,
///   `derive::write_chapters`, which clears only rows carrying
///   `SCANNER_CHAPTER_SOURCE` so a hand-entered number survives, **and this
///   function's own restore below**, for the identical reason the two columns
///   above need one: the member row is deleted and reinserted along with its
///   edition, so a hand-set chapter would not survive the rescan without it.
///
/// **This function is itself a writer common to all four, alongside
/// `librarydoc::apply`** — not the one exception the wholesale-replace design
/// might suggest. `edition.label` is the one column it *originates* a value
/// for (the document's own, under `Scope::Document`); the other three it only
/// *restores* — copies the held value across its own delete-and-reinsert
/// unchanged — but a restore still writes the column, and skipping it would
/// silently lose a hand correction on the next routine rescan, which is
/// exactly the failure this whole doc comment exists to rule out.
///
/// **`edition.language`/`kind` have a second writer, and this restore is what
/// hands the row over to it rather than being made redundant by it.**
/// `derive::write_edition_metadata` writes both from the rules after this
/// returns — but only over a work no `<stem>.library.json` has been absorbed
/// onto, and only for the editions that derive itself named. For every other
/// row it is the value restored here that stands, so dropping this restore as
/// superfluous would null a hand-classified edition on the next rescan after
/// all.
///
/// So this reads those columns before the delete and restores them after
/// the reinsert, **matching editions by `key` and members by `sha256`**
/// rather than by row id, since the ids are gone once the delete runs.
/// `sha256` is the right member identity for the same reason
/// `<stem>.library.json` uses it: it survives a path change. Nothing
/// resurrects a row the document says is gone — preservation applies only to
/// identities that survive the replacement.
///
/// This mirrors what `Sidecar::merge_spoke` already does for hand-corrected
/// runs, which is this codebase's established pattern: derived data is
/// replaced, hand-entered data is carried across the replacement.
///
/// **Under [`Scope::Scan`] the page flags are carried too**, for the same
/// reason and by the same means: a scan's document declares no `spreads` and no
/// `credits` because a walk cannot know either, so replacing from it would
/// delete `taiyaku credits --write`'s output on every rescan. Under
/// [`Scope::Document`] they are replaced from the document, which is what lets
/// a `taiyaku.json` that drops a credit page actually drop it — a union of held
/// and incoming could only ever grow. Keyed on the edition's `key`, like the
/// metadata above and for the same reason: the row ids are gone once the
/// delete runs.
///
/// **What the key cannot carry, since restoring still beats deleting but is not
/// free.** A page flag is a page *number*, and the key says nothing about how
/// many pages precede it. A work survives a rescan whenever its **hub**'s
/// members are unchanged *and no fold takes it* — `derive::superseded` and
/// `fold::absorb` delete a work precisely when its material is unchanged and
/// already held by another — so a spoke edition that gained or lost a member
/// keeps its credit and spread flags at the page numbers they had before the
/// member set moved, pointing at whatever is now at those offsets. (A folded
/// work's flags are not left pointing anywhere either: a fold writes its spread
/// flags onto a twin holding none, and counts a credit flag, which its `DELETE`
/// takes; O76, O121.) Re-running `taiyaku credits --write` over that edition is
/// the recovery, and nothing signals the need. Deleting them instead would cost
/// that same re-run on every rescan of every edition, which is why this is the
/// residual case rather than the design.
fn replace_editions(
    conn: &Connection,
    work_id: i64,
    doc: &Sidecar,
    scope: Scope,
) -> Result<(), String> {
    let held_editions = held_edition_meta(conn, work_id)?;
    let held_members = held_member_meta(conn, work_id)?;
    // Read only where it will be used. A document import replaces these from
    // the document, so the query would be work thrown away — and this path
    // runs once per work on every `serve` start.
    let held_flags = match scope {
        Scope::Scan => held_page_flags(conn, work_id)?,
        Scope::Document => HashMap::new(),
    };

    match scope {
        Scope::Document => {
            conn.execute("DELETE FROM edition WHERE work_id = ?1", [work_id])
                .map_err(|e| format!("clearing editions of work {work_id}: {e}"))?;
        }
        // **Only the editions this scan actually named.** "The scan never saw
        // this edition" and "a rule edit removed it" are different facts and
        // nothing here can tell them apart, so this must not act as though it
        // can: deleting a hand-aligned edition is unrecoverable, while an
        // orphaned one is visible and fixable. The cost is that a rule edit
        // renaming an edition key leaves the old edition standing, which is
        // why `DeriveReport::kept_editions` counts them — an orphan reported
        // rather than discovered.
        Scope::Scan => {
            for key in doc.editions.keys() {
                conn.execute(
                    "DELETE FROM edition WHERE work_id = ?1 AND key = ?2",
                    rusqlite::params![work_id, key],
                )
                .map_err(|e| format!("clearing edition `{key}` of work {work_id}: {e}"))?;
            }
        }
    }

    for (key, edition) in &doc.editions {
        // **The label is not a scan's to write, and neither is its
        // provenance.** `derive::write_work` sets `label: key.clone()` because
        // neither a root nor a rule carries a display name for an edition —
        // there is no `set_label` column — so an import that took the incoming
        // label at face value would rename a person's `Japanese raw` to `raw`
        // on the next routine rescan. Under `Scope::Document` the label *is*
        // the document's, exactly as `work.title` is — so it is also that
        // scope's `label_derived`. Under `Scope::Scan` a walk has no display
        // name at all, so `label_derived` carries across the rescan unchanged
        // from whatever it already was, and only `label_hand` — carried the
        // same way — stands between a hand correction and this
        // wholesale replace.
        let held = held_editions.get(key);
        let label_hand = held.and_then(|h| h.label_hand.clone());
        let label_derived = match scope {
            Scope::Document => edition.label.clone(),
            Scope::Scan => held
                .and_then(|h| h.label_derived.clone())
                .unwrap_or_else(|| edition.label.clone()),
        };
        let label = label_hand.clone().unwrap_or_else(|| label_derived.clone());
        conn.execute(
            "INSERT INTO edition (work_id, key, label, label_hand, label_derived)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![work_id, key, label, label_hand, label_derived],
        )
        .map_err(|e| format!("inserting edition `{key}`: {e}"))?;
        let edition_id = conn.last_insert_rowid();

        if let Some(held) = held {
            conn.execute(
                // The provenance columns restored beside the effective
                // ones (O80, migration 011): `derive` consults the first pair
                // and would otherwise overwrite a hand choice on the next pass.
                "UPDATE edition
                    SET language = ?1, kind = ?2,
                        language_hand = ?3, kind_hand = ?4
                  WHERE id = ?5",
                rusqlite::params![
                    held.language,
                    held.kind,
                    held.language_hand,
                    held.kind_hand,
                    edition_id
                ],
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

            if let Some(held) = held_members.get(&member.sha256) {
                let member_id = conn.last_insert_rowid();
                conn.execute(
                    "UPDATE member SET chapter_declared = ?1, chapter_source = ?2, root_id = ?3
                      WHERE id = ?4",
                    rusqlite::params![
                        held.declared,
                        held.source,
                        held.root_id(&member.path),
                        member_id
                    ],
                )
                .map_err(|e| format!("restoring metadata on a member of `{key}`: {e}"))?;
                // OR IGNORE: the trigger on `root_id` has already added that
                // directory's row. See `HeldMember::roots_by_path`.
                for root_id in held.roots_by_path.get(&member.path).into_iter().flatten() {
                    conn.execute(
                        "INSERT OR IGNORE INTO member_root (member_id, root_id) VALUES (?1, ?2)",
                        rusqlite::params![member_id, root_id],
                    )
                    .map_err(|e| format!("restoring a directory of a member of `{key}`: {e}"))?;
                }
            }
        }
        match scope {
            // OR IGNORE, because a page listed twice in `spreads` or `credits`
            // states one fact twice rather than contradicting itself, and
            // nothing in the format forbids it — `Sidecar::from_json` is a
            // serde parse and a version check. The flag is a set (`project.rs`
            // reads these back `ORDER BY page`), so the repetition carries
            // nothing to lose. A plain INSERT would refuse such a document with
            // a raw `UNIQUE constraint failed: page_flag…`.
            Scope::Document => {
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
            // A scan's document says nothing about either list, so what the
            // library already held is what stands.
            Scope::Scan => {
                for (kind, page) in held_flags.get(key).into_iter().flatten() {
                    conn.execute(
                        "INSERT OR IGNORE INTO page_flag (edition_id, kind, page)
                         VALUES (?1, ?2, ?3)",
                        rusqlite::params![edition_id, kind, page],
                    )
                    .map_err(|e| format!("restoring a page flag on `{key}`: {e}"))?;
                }
            }
        }
    }
    Ok(())
}

/// Every page flag currently on `work_id`, by `edition.key` — the identity
/// [`replace_editions`] restores by, since the row ids are gone once its
/// delete runs.
///
/// `kind` and `page` are carried as the raw stored values, the same way
/// [`held_credits`] carries `role`: this function's output is only ever copied
/// back onto fresh `page_flag` rows, never read as anything else, so parsing
/// either into a narrower type would add a failure mode to a pure carry.
///
/// `ORDER BY` so the restore is deterministic. `page_flag` is a set and
/// `project.rs` reads it back `ORDER BY page` regardless, so the order carries
/// no meaning — it is here so that two runs over identical rows issue
/// identical statements.
fn held_page_flags(
    conn: &Connection,
    work_id: i64,
) -> Result<HashMap<String, Vec<(String, i64)>>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT edition.key, page_flag.kind, page_flag.page
             FROM page_flag JOIN edition ON edition.id = page_flag.edition_id
             WHERE edition.work_id = ?1
             ORDER BY edition.key, page_flag.kind, page_flag.page",
        )
        .map_err(|e| format!("preparing held page flags: {e}"))?;
    let rows = stmt
        .query_map([work_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(|e| format!("querying held page flags: {e}"))?;

    let mut out: HashMap<String, Vec<(String, i64)>> = HashMap::new();
    for row in rows {
        let (key, kind, page) = row.map_err(|e| format!("reading held page flags: {e}"))?;
        out.entry(key).or_default().push((kind, page));
    }
    Ok(out)
}

/// One edition's hand-entered columns, read before `replace_editions`'
/// wholesale delete so they can be restored after the reinsert. `role` is
/// carried as the raw stored text rather than parsed into an enum: it
/// already passed through `librarydoc`'s validation on the way in, and this
/// function only ever copies it back onto a fresh `credit` row, never reads
/// it as anything else.
struct HeldEdition {
    /// What a person said, or `None` — `edition.label_hand`. Carried through
    /// this struct rather than only the effective column: see
    /// [`replace_editions`]'s comment on the label for why the effective value
    /// alone is not enough for a wholesale delete-and-reinsert to restore the
    /// invariant it must hold.
    label_hand: Option<String>,
    /// What derivation or the document said — `edition.label_derived`. A
    /// scan's document repeats the key because a walk has no other name to
    /// give, which is why a scan carries this forward unchanged rather than
    /// minting it fresh from the incoming document. See [`replace_editions`].
    label_derived: Option<String>,
    language: Option<String>,
    kind: Option<String>,
    /// **The provenance halves of the two above — O80, migration 011.** Carried
    /// for exactly the reason `label_hand` is: `derive` writes
    /// `COALESCE(<hand>, <derived>)`, so a delete-and-reinsert that restored
    /// only the effective columns would leave a hand-chosen kind or language in
    /// place for one pass and lose it on the next, when derivation would find
    /// nothing standing in its way.
    language_hand: Option<String>,
    kind_hand: Option<String>,
    /// `(person_id, role)` pairs — enough to reinsert each `credit` row
    /// verbatim without re-resolving the person by name.
    credits: Vec<(i64, String)>,
}

/// Every edition's hand-entered metadata currently on `work_id`, keyed by
/// `edition.key` — the identity `replace_editions` restores by, since the
/// row ids are gone once its delete runs.
///
/// **Both provenance columns, not the effective `label`.** Selecting `label`
/// alone would leave `replace_editions` reinserting that one value into both
/// `label` and `label_derived`, so a routine rescan would wipe `label_hand` to
/// NULL *and* rebind `label_derived` to whatever `label` currently held, which
/// under `Scope::Scan` is the held, possibly hand-set, value: the safety net
/// overwritten with the value it exists to fall back from.
fn held_edition_meta(
    conn: &Connection,
    work_id: i64,
) -> Result<HashMap<String, HeldEdition>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, key, label_hand, label_derived, language, kind,
                    language_hand, kind_hand
             FROM edition WHERE work_id = ?1",
        )
        .map_err(|e| format!("preparing held edition metadata: {e}"))?;
    let rows = stmt
        .query_map([work_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        })
        .map_err(|e| format!("querying held edition metadata: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading held edition metadata: {e}"))?;

    let mut out = HashMap::new();
    for (edition_id, key, label_hand, label_derived, language, kind, language_hand, kind_hand) in
        rows
    {
        let credits = held_credits(conn, edition_id)?;
        out.insert(
            key,
            HeldEdition {
                label_hand,
                label_derived,
                language,
                kind,
                language_hand,
                kind_hand,
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

/// One member's data that lives on the row `replace_editions`' wholesale
/// delete would otherwise drop: the hand-set chapter columns, and where the
/// bytes are held — `root_id` and the `member_root` set — facts a scan alone
/// establishes and no document, hand-written or projected, carries a field
/// for. Keyed by `sha256` alone in the map that holds these — see
/// `held_member_meta`'s doc comment for the work-wide-match limitation that
/// follows from that.
struct HeldMember {
    declared: Option<f64>,
    source: Option<String>,
    /// Every directory any held row of this hash had a `member_root` row for,
    /// which [`Self::root_id`] reads to tell one directory from several.
    roots: BTreeSet<i64>,
    /// Each held row's `root_id`, NULL included, by its `member.path`.
    root_by_path: HashMap<String, Option<i64>>,
    /// Each held row's `member_root` rows, by its `member.path`. A new row at
    /// a path the work held comes back under exactly those, as `write_roots`
    /// writes them; one edition's copy under another directory gives it none.
    /// The one exception: a path no held row had takes the hash's one
    /// directory as its column, which can be another edition's, and so a
    /// grant on it gains sight. A member new to the document gets none, and
    /// a purely imported work stays under no root (spec C1 §5.1, O64).
    roots_by_path: HashMap<String, BTreeSet<i64>>,
}

impl HeldMember {
    /// The `root_id` a new row of this hash at `path` comes back with.
    ///
    /// **The held row at the same path, else the one directory, else NULL.**
    /// `member.root_id` is the directory of the copy `member.path` names, so
    /// a path the work held answers it exactly, NULL included. A path it did
    /// not hold is answered only where every copy sat under one directory;
    /// with two, a guess could name a directory the file is not under and
    /// give a grant on it sight of the file. NULL fails closed: the member is
    /// under no directory until the next derive writes it from the scan, and
    /// that derive's primary is the lowest root id, not necessarily the one
    /// held.
    fn root_id(&self, path: &str) -> Option<i64> {
        if let Some(&root_id) = self.root_by_path.get(path) {
            return root_id;
        }
        let mut roots = self.roots.iter();
        match (roots.next(), roots.next()) {
            (Some(&only), None) => Some(only),
            _ => None,
        }
    }
}

/// Every member's hand-set chapter number and directories currently on
/// `work_id`, keyed by `sha256` — content identity, the same reason
/// `<stem>.library.json` keys its own `members` on it rather than on path
/// (`librarydoc.rs`'s own doc comment). A path or an edition can move
/// underneath a reimport; the hash does not.
///
/// **The match is work-wide, not edition-scoped — a real limitation, not a
/// hypothetical one.** Nothing forbids the same `sha256` from being a member
/// of two editions of one work: not the `Sidecar`/`Edition`/`Member` types,
/// not JSON parsing, and not the schema (`member_by_sha` indexes `sha256`
/// with no uniqueness constraint). A repack or omnibus edition that shares a
/// literal file with a standalone edition produces exactly this, and so do
/// two byte-identical files under one directory, which the derive keeps as
/// two members of one edition. When it happens, only one of the held rows can
/// survive into this map, keyed as it is by hash alone — so the query below
/// carries an explicit `ORDER BY edition.id, member.ordinal`, and building the
/// map with `or_insert` keeps only the first row seen in that order: the
/// earliest-inserted edition (lowest `edition.id`, i.e. first in the
/// document), and within it the earliest member (lowest `ordinal`).
///
/// **For `declared`/`source`, that choice is arbitrary — there is no principled
/// reason the earlier edition's chapter number should be the one that wins —
/// but it is deterministic** rather than depending on whatever order SQLite
/// happens to return rows in. Whether a hand-set chapter number should instead
/// be scoped to the edition it was set on, so two editions sharing a file could
/// carry different values for it, is an open design question this function does
/// not settle.
///
/// **Where the bytes are held is not decided by that choice at all.** Every
/// row of the hash contributes its `root_id` and its `member_root` rows under
/// its own path ([`HeldMember::root_by_path`], [`HeldMember::roots_by_path`]),
/// so a hand-set chapter on one edition's copy does not cost another
/// edition's copy its scanned directory, and a held copy is not given
/// another's, except that a path no held row had takes the hash's one
/// directory, which can be another edition's ([`HeldMember::root_id`]).
fn held_member_meta(
    conn: &Connection,
    work_id: i64,
) -> Result<HashMap<String, HeldMember>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT member.sha256, member.chapter_declared, member.chapter_source,
                    member.root_id, member.path
             FROM member JOIN edition ON edition.id = member.edition_id
             WHERE edition.work_id = ?1
               AND (member.chapter_declared IS NOT NULL OR member.chapter_source IS NOT NULL
                    OR member.root_id IS NOT NULL
                    OR EXISTS (SELECT 1 FROM member_root WHERE member_id = member.id))
             ORDER BY edition.id, member.ordinal",
        )
        .map_err(|e| format!("preparing held member metadata: {e}"))?;
    type Row = (String, Option<f64>, Option<String>, Option<i64>, String);
    let rows = stmt
        .query_map([work_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .map_err(|e| format!("querying held member metadata: {e}"))?
        .collect::<Result<Vec<Row>, _>>()
        .map_err(|e| format!("reading held member metadata: {e}"))?;

    let mut out: HashMap<String, HeldMember> = HashMap::new();
    for (sha256, declared, source, root_id, path) in rows {
        // First row wins the chapter columns: the earliest edition, earliest
        // ordinal within it, per the `ORDER BY` above.
        let held = out.entry(sha256).or_insert(HeldMember {
            declared,
            source,
            roots: BTreeSet::new(),
            root_by_path: HashMap::new(),
            roots_by_path: HashMap::new(),
        });
        held.root_by_path.entry(path).or_insert(root_id);
    }

    let mut stmt = conn
        .prepare(
            "SELECT member.sha256, member.path, member_root.root_id
               FROM member_root
               JOIN member ON member.id = member_root.member_id
               JOIN edition ON edition.id = member.edition_id
              WHERE edition.work_id = ?1",
        )
        .map_err(|e| format!("preparing held member directories: {e}"))?;
    let rows = stmt
        .query_map([work_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(|e| format!("querying held member directories: {e}"))?;
    for row in rows {
        let (sha256, path, root_id) =
            row.map_err(|e| format!("reading held member directories: {e}"))?;
        // Every row read here has a member the first query also read, by its
        // `EXISTS` arm, so the hash is already in `out`.
        if let Some(held) = out.get_mut(&sha256) {
            held.roots.insert(root_id);
            held.roots_by_path.entry(path).or_default().insert(root_id);
        }
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
