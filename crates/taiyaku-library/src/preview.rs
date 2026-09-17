//! What a proposed rule list would do, answered by doing it and rolling back.
//!
//! **The real `derive`, not a second evaluator.** Spec C4 §7's first test is
//! that a preview and a real save produce the same structure, and the whole
//! rule editor is a promise that the preview is true. Running the same
//! function makes that true by construction; a preview pass of its own makes
//! it true only for as long as a test keeps two implementations in step. See
//! `docs/superpowers/specs/2026-09-02-library-configuration-build-design.md`,
//! §2.
//!
//! **It writes nothing.** The transaction rolls back, and the derivation runs
//! under [`crate::derive::Documents::Skip`] so nothing under `<root>/.taiyaku/`
//! is written, renamed or removed — a rollback would not undo those.

use rusqlite::Connection;

/// Where one member sits: which series, work and edition hold it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    pub series: String,
    pub work: String,
    pub edition: String,
}

/// Every member's placement, keyed by `member.path`.
///
/// **`member.path` is the identity that survives a derivation.** Row ids do
/// not: a derivation rewrites `work`, `edition` and `member` wholesale, so a
/// before-and-after diff keyed on ids would report every row as new.
pub type Grouping = std::collections::BTreeMap<String, Placement>;

/// Every member's current placement.
///
/// # Errors
///
/// If SQLite refuses the query.
pub fn grouping(conn: &Connection) -> Result<Grouping, String> {
    let mut stmt = conn
        .prepare(
            // **`LEFT JOIN`, and the id as the fallback.** `series_title`
            // holds several names per series and `is_primary` picks the
            // display one — but nothing guarantees a series has a primary
            // title, and an inner join would silently drop every series that
            // does not, making a diff read as an orphaning that never
            // happened.
            "SELECT m.path, s.id, st.title, w.slug, e.key
               FROM member m
               JOIN edition e ON e.id = m.edition_id
               JOIN work    w ON w.id = e.work_id
               JOIN series  s ON s.id = w.series_id
               LEFT JOIN series_title st
                      ON st.series_id = s.id AND st.is_primary = 1
              ORDER BY m.path",
        )
        .map_err(|e| format!("preparing the grouping query: {e}"))?;
    let rows = stmt
        .query_map([], |r| {
            let series_id: i64 = r.get(1)?;
            Ok((
                r.get::<_, String>(0)?,
                Placement {
                    series: r
                        .get::<_, Option<String>>(2)?
                        .unwrap_or_else(|| format!("series {series_id}")),
                    work: r.get(3)?,
                    edition: r.get(4)?,
                },
            ))
        })
        .map_err(|e| format!("reading the grouping: {e}"))?;
    let mut out = Grouping::new();
    for row in rows {
        let (path, placement) = row.map_err(|e| format!("reading a placement: {e}"))?;
        out.insert(path, placement);
    }
    Ok(out)
}

/// What changes between the library as it stands and the library the proposed
/// rules would derive.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Impact {
    pub series_before: usize,
    pub series_after: usize,
    /// `series_before - series_after`, floored at zero. Merging is the only
    /// way rules reduce the count, so the difference names it directly.
    pub series_merged: usize,
    /// Members whose series changed.
    pub works_rehomed: usize,
    /// Members present before and absent after — a rule that swallowed them.
    pub works_orphaned: usize,
    /// Proposals that were `accepted` before this derivation and are `open`
    /// after it — [`crate::proposals::raise`]'s own reopen step flipping one
    /// back because the rule it created dropped out of the proposed list.
    /// This is the warning that matters: it fires exactly when the list being
    /// previewed would put a question back in front of someone who already
    /// answered it.
    ///
    /// **Not `proposals_resolved`, which this field replaces.** A derivation
    /// reaches exactly one proposal writer — [`crate::proposals::raise`],
    /// which only ever inserts a fresh `open` row or reopens an orphaned
    /// `accepted` one — so nothing a derive does can ever move a proposal
    /// *out* of `open`. `proposals_resolved` was
    /// `open_before.saturating_sub(open_after)`, and `open_after` can never
    /// be less than `open_before`: the subtraction was always `0`, structural
    /// rather than merely usually so, and it floored the one signal a
    /// derivation can actually produce — a reopen — right along with it.
    /// `proposals_reopened` reports that signal directly instead of a
    /// difference that can only ever read zero.
    pub proposals_reopened: usize,
    /// New proposals this derivation's own `raise` call opened —
    /// [`crate::derive::DeriveReport::raised`], carried straight through from
    /// the report [`preview`]'s own `derive_with` call already returns rather
    /// than counted a second time here. Two ways of arriving at one number is
    /// how they end up disagreeing.
    pub proposals_raised: usize,
}

/// The one definition of what each [`Impact`] field means, over two
/// [`Grouping`] snapshots and the two proposal counts either side of whatever
/// produced them.
///
/// **Shared by [`preview`] and by `preview/tests.rs`.** The whole claim this
/// module exists to make is that a preview's [`Impact`] equals the one a real
/// save produces —
/// `a_preview_and_a_real_save_produce_the_same_structure` — and that claim
/// only means something if the test computes the "real save" side of the
/// comparison through this same function rather than re-deriving its own
/// notion of what the numbers mean. A test that recomputed the diff its own
/// way could agree with [`preview`] by construction and still be wrong about
/// what a rehomed work is.
fn diff(
    before: &Grouping,
    after: &Grouping,
    proposals_reopened: usize,
    proposals_raised: usize,
) -> Impact {
    let series_before: std::collections::BTreeSet<&str> =
        before.values().map(|p| p.series.as_str()).collect();
    let series_after: std::collections::BTreeSet<&str> =
        after.values().map(|p| p.series.as_str()).collect();

    Impact {
        series_before: series_before.len(),
        series_after: series_after.len(),
        series_merged: series_before.len().saturating_sub(series_after.len()),
        works_rehomed: after
            .iter()
            .filter(|(path, now)| {
                before
                    .get(*path)
                    .is_some_and(|was| was.series != now.series)
            })
            .count(),
        works_orphaned: before
            .keys()
            .filter(|path| !after.contains_key(*path))
            .count(),
        proposals_reopened,
        proposals_raised,
    }
}

/// Which rule decided one [`Row`]'s field — the rule's own position in
/// `proposed` (the list a caller previewed), or the root's own default
/// column.
///
/// **An index into `proposed`, not a rule's database id — a second boundary
/// the sentinel above is kept off of, for a reason a rule id cannot avoid.**
/// "The rule currently open in the editor" is not always a rule with an id
/// at all: [`crate::rules::replace_all`]'s own `id: 0` names a rule the
/// caller has proposed but never saved — the live-typing case, and the
/// ordinary shape of "add a rule" before the save button is ever pressed —
/// and `0` is *also* `rules::ROOT_FALLBACK` — the root-fallback sentinel
/// [`decided`] exists to translate, private to `rules.rs` and reachable from
/// here only through [`crate::rules::is_root_fallback`]. An id-keyed
/// `Decided` would silently attribute a root-decided field to a brand-new,
/// unsaved rule the moment its root sets any default at all, and could never
/// tell two unsaved rules in the same proposed list apart — both carry
/// `id: 0`, so nothing about either rule's own id distinguishes them.
/// `proposed` is always the exact list the caller just sent, so a position in
/// it is unambiguous and needs no sentinel regardless of how many of its
/// rules are unsaved. No id ever crosses this boundary at all:
/// [`preview_with_rows`] maps the real ids [`crate::rules::replace_all`]
/// minted back to `proposed`'s own indices before a single [`Decided`] is
/// ever built — a stronger version of the same discipline that already keeps
/// that sentinel private behind [`crate::rules::is_root_fallback`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decided {
    Rule(usize),
    RootDefault,
}

/// One `scan_entry` a rule's scope reaches, with what changed for it and
/// which rule — or the root's own default — decided each field.
///
/// **`before`/`after` are keyed on [`Grouping`]'s own key** — the member's
/// absolute path, built the same way [`crate::derive::derive_with`] itself
/// builds it (`root.path.join(rel_path)`). Not every row has one: a container
/// directory or a chapter directory never becomes a member, and carries
/// `None` on both sides rather than a placement nothing ever wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub rel_path: String,
    pub root_id: i64,
    pub before: Option<Placement>,
    pub after: Option<Placement>,
    /// Whether the rule this table is for decided at least one field of this
    /// entry — [`crate::rules::Resolved::provenance`] naming its id anywhere.
    /// A `false` row is a near-miss, not merely absent from the match — see
    /// [`rows_for`] for the definition.
    pub matched: bool,
    pub decided_by: std::collections::BTreeMap<crate::rules::Field, Decided>,
}

/// The matched-files table for one rule: at most `limit` rows, and whether
/// more existed than fit.
///
/// **Capped, and the cap is stated rather than silently applied** — the same
/// discipline `inventory` holds over its own listings. A real library holds
/// 2 044 scan entries (`docs/notes.md`, "What a rule preview costs on a real
/// library"), and a table that quietly stopped at `limit` rows would read to
/// whoever is looking at it as though it were the whole list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Rows {
    pub rows: Vec<Row>,
    pub truncated: bool,
}

/// Map one field's provenance id — a real, minted `rule.id`, or
/// `rules::ROOT_FALLBACK` — to the boundary type this module exposes,
/// translating a minted id back to its position in `proposed` via
/// `id_to_index`. See [`Decided`]'s own doc comment for why a position and
/// not the id itself.
///
/// **Every non-root id reaching this function is a key in `id_to_index`, by
/// construction.** [`rows_for`]'s own `Engine` is compiled from a copy of
/// `proposed` whose ids are the ones [`crate::rules::replace_all`] actually
/// wrote — never `proposed`'s own, possibly-`0` ids — so every id
/// [`crate::rules::Engine::resolve`] can record in `provenance` is either
/// `rules::ROOT_FALLBACK` or one `id_to_index` was built from.
fn decided(rule_id: i64, id_to_index: &std::collections::BTreeMap<i64, usize>) -> Decided {
    if crate::rules::is_root_fallback(rule_id) {
        Decided::RootDefault
    } else {
        Decided::Rule(
            *id_to_index
                .get(&rule_id)
                .expect("provenance only ever names an id rows_for's own Engine was compiled from"),
        )
    }
}

/// Every `scan_entry.rel_path` under one root, in path order — the one
/// column [`crate::rules::Engine::resolve`] needs. [`rows_for`] does not care
/// about a `scan_entry`'s format or page count the way
/// [`crate::derive::derive_with`] does — only about what the rules decide for
/// its path.
fn rel_paths(conn: &Connection, root_id: i64) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT rel_path FROM scan_entry WHERE root_id = ?1 ORDER BY rel_path")
        .map_err(|e| format!("reading scan entries for root {root_id}: {e}"))?;
    let rows = stmt
        .query_map([root_id], |r| r.get::<_, String>(0))
        .map_err(|e| format!("reading scan entries for root {root_id}: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading scan entries for root {root_id}: {e}"))
}

/// The matched-files table for `proposed[index]`, walked over every
/// `scan_entry` its scope reaches — its own `root_id`, or every root when
/// that is `None`.
///
/// **A row is kept when it is a match or a near-miss, never otherwise.** C4
/// §1: a regex's *failure* is the thing you cannot see by reading it — its
/// own example is `Yu Yu Hakusho - Colored (Colored Council)` failing to
/// match a colour-edition regex. The spec leaves the definition open; this is
/// the one this crate implements:
///
/// - **matched** — [`crate::rules::Resolved::provenance`] names this rule's
///   own written id (`written_ids[index]`) for at least one field of this
///   entry.
/// - **near-miss** — it does not, *and* every field this rule's own
///   definition could set ([`crate::rules::fields_settable_by`]) is, on this
///   entry, either undecided or decided by the root's own default
///   ([`crate::rules::is_root_fallback`]). A field this rule sets that some
///   *other* rule decided is not a near-miss — something else already caught
///   it, so this rule missing it is not the failure worth showing.
///
/// Matches sort before near-misses; `rel_path` order (then `root_id`, for a
/// path two roots happen to share) within each. At most `limit` rows;
/// [`Rows::truncated`] says whether the real count ran past it.
///
/// `written_ids` is [`crate::rules::replace_all`]'s own return value for this
/// same call's `replace_all(&tx, proposed)` — element `i` the id `proposed[i]`
/// was actually written under. See [`Decided`]'s own doc comment for why this
/// function never resolves against `proposed`'s own (possibly-`0`, possibly
/// duplicated) ids directly.
///
/// # Errors
///
/// If `index` is out of range for `proposed`, or if a `scan_entry` or `root`
/// query fails.
fn rows_for(
    conn: &Connection,
    proposed: &[crate::rules::Rule],
    written_ids: &[i64],
    index: usize,
    before: &Grouping,
    after: &Grouping,
    limit: usize,
) -> Result<Rows, String> {
    let rule = proposed.get(index).ok_or_else(|| {
        format!(
            "selected index {index} is out of range for the proposed list, which holds {} \
             rules",
            proposed.len()
        )
    })?;
    let fields_set = crate::rules::fields_settable_by(rule);

    // A copy of `proposed` with every id replaced by the one
    // `crate::rules::replace_all` actually wrote for it — never `proposed`'s
    // own, possibly-`0` ids. Without this, an `Engine` built straight from
    // `proposed` would record `0` in `provenance` for *every* unsaved rule —
    // indistinguishable from `crate::rules::ROOT_FALLBACK`, and from any
    // other unsaved rule in the same list. See `Decided`'s own doc comment.
    let with_written_ids: Vec<crate::rules::Rule> = proposed
        .iter()
        .zip(written_ids)
        .map(|(r, &id)| crate::rules::Rule { id, ..r.clone() })
        .collect();
    let id_to_index: std::collections::BTreeMap<i64, usize> = written_ids
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();
    let rule_id = written_ids[index];

    let (engine, _complaints) = crate::rules::Engine::compile(&with_written_ids);
    let roots = crate::scan::roots(conn)?;

    let mut matched = Vec::new();
    let mut near_misses = Vec::new();
    for root in &roots {
        if rule.root_id.is_some_and(|scoped| scoped != root.id) {
            continue;
        }
        for rel_path in rel_paths(conn, root.id)? {
            let resolved = engine.resolve(root, &rel_path);
            let decided_here = resolved.provenance.values().any(|&id| id == rule_id);
            // **A rule that can set nothing near-misses nothing**, and the
            // emptiness has to be tested before the `all`, which is vacuously
            // true over an empty set. `fields_settable_by`'s own doc comment
            // frames the question this answers — "did this rule miss this
            // entry, or did it simply have nothing to say about it at all" —
            // and without this guard the second case was answered as the
            // first, for every entry in the library at once.
            //
            // **`blankRule()` is exactly that rule, so this was the ordinary
            // case rather than a degenerate one.** The editor's "+ Add" mints
            // a `regex` rule with an empty pattern and no `set_*` columns, and
            // selects it immediately: nothing is settable, so every
            // `scan_entry` in the library read as a near-miss and the Matched
            // files tab opened on 500 rows and a truncation notice. Found by
            // the whole-branch review of C4 PR B.
            let is_near_miss = !decided_here
                && !fields_set.is_empty()
                && fields_set
                    .iter()
                    .all(|field| match resolved.provenance.get(field) {
                        None => true,
                        Some(&id) => crate::rules::is_root_fallback(id),
                    });
            if !decided_here && !is_near_miss {
                continue;
            }
            let key = root.path.join(&rel_path).to_string_lossy().into_owned();
            let row = Row {
                root_id: root.id,
                before: before.get(&key).cloned(),
                after: after.get(&key).cloned(),
                matched: decided_here,
                decided_by: resolved
                    .provenance
                    .iter()
                    .map(|(field, &id)| (*field, decided(id, &id_to_index)))
                    .collect(),
                rel_path,
            };
            if decided_here {
                matched.push(row);
            } else {
                near_misses.push(row);
            }
        }
    }
    let by_path = |a: &Row, b: &Row| a.rel_path.cmp(&b.rel_path).then(a.root_id.cmp(&b.root_id));
    matched.sort_by(by_path);
    near_misses.sort_by(by_path);

    let mut rows = matched;
    rows.append(&mut near_misses);
    let truncated = rows.len() > limit;
    rows.truncate(limit);
    Ok(Rows { rows, truncated })
}

/// What replacing the rule set with `proposed` and re-deriving would change,
/// found by actually doing it — inside a transaction this function rolls
/// back before returning.
///
/// **Both snapshots are taken inside this transaction, after the `BEGIN
/// IMMEDIATE` takes the write lock — not before it.** [`crate::Library::working`]
/// hands a job this long a *second* connection to the same database
/// precisely so a scan can run while this one derives; if `before` were read
/// on the caller's connection before `BEGIN`, a write that lands on that
/// second connection between the snapshot and the `BEGIN` would make `before`
/// describe a state that never coexisted with `after` — a phantom rehome or
/// orphan on the one screen whose whole promise is that it is true.
///
/// **The write lock this holds is the one [`crate::Library::working`] exists to avoid
/// making every request wait for.** A real derive is atomic per work, not
/// across the library (see [`crate::derive::derive_with`]'s own doc) —
/// exactly so a half-derived library still has most of its works current.
/// This function trades that away on purpose: it wraps one `derive_with`
/// pass over the *whole* library in a single `BEGIN IMMEDIATE` so the whole
/// thing can roll back atomically, which means the write lock stays taken —
/// blocking any other writer, including a second preview — for as long as
/// one full derivation over the whole library takes, not per work. See
/// [`crate::Nested`]'s own doc for the matching cost on the transactions
/// nested inside this one.
///
/// # Errors
///
/// If SQLite refuses the transaction, if [`crate::rules::replace_all`] or
/// [`crate::derive::derive_with`] fails, or if the rollback itself is
/// refused.
pub fn preview(conn: &Connection, proposed: &[crate::rules::Rule]) -> Result<Impact, String> {
    preview_with_rows(conn, proposed, None, 0).map(|(impact, _rows)| impact)
}

/// [`preview`], extended with C4 §1's other two tabs: the files a selected
/// rule matched or near-missed, and what decided each field.
///
/// **One transaction, not two.** A [`Row`]'s `after` only exists inside the
/// rolled-back derivation this function already runs to answer [`Impact`] —
/// a standalone row-building function would have to open a second
/// transaction and derive again. Measured at 281 ms for one derivation over a
/// real library (`docs/notes.md`, "What a rule preview costs on a real
/// library"), and the HTTP route this feeds sends one request carrying both
/// the proposed list and which rule is selected — paying that cost twice
/// would double the wait for data this one pass already holds.
///
/// `selected` is an index into `proposed` — "the rule currently open in the
/// editor" is a position in the exact list the caller just sent, never a
/// rule id; see [`Decided`]'s own doc comment for why a rule id cannot serve
/// this role at all, unsaved or not. `None` when no rule is open in the
/// editor — the route omits it in exactly that case — and then no
/// row-building work happens at all: no `scan_entry` walk, no second
/// [`crate::rules::Engine`], nothing past the [`Impact`] this function
/// computes either way.
///
/// # Errors
///
/// Everything [`preview`] can return, plus — only when `selected` is
/// `Some` — an error naming the index and `proposed`'s own length if that
/// index is out of range for `proposed`.
pub fn preview_with_rows(
    conn: &Connection,
    proposed: &[crate::rules::Rule],
    selected: Option<usize>,
    limit: usize,
) -> Result<(Impact, Rows), String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("starting the preview transaction: {e}"))?;
    let before = grouping(&tx)?;
    let proposals_before = crate::proposals::states(&tx)?;

    // `written_ids[i]` is the real, minted id `proposed[i]` now holds in the
    // `rule` table this transaction just wrote — the mapping `rows_for`
    // needs to turn a database id back into a position in `proposed` without
    // ever trusting `proposed`'s own, possibly-`0`, possibly-repeated ids.
    let written_ids = crate::rules::replace_all(&tx, proposed)?;
    let report = crate::derive::derive_with(&tx, crate::derive::Documents::Skip)?;
    let after = grouping(&tx)?;
    let proposals_after = crate::proposals::states(&tx)?;

    let rows = match selected {
        Some(index) => rows_for(&tx, proposed, &written_ids, index, &before, &after, limit)?,
        None => Rows::default(),
    };

    // Explicit, not `drop(tx)`: a rollback that happens because nobody
    // committed is a rollback nobody can find when reading this later.
    tx.rollback()
        .map_err(|e| format!("rolling the preview back: {e}"))?;

    Ok((
        diff(
            &before,
            &after,
            crate::proposals::reopened(&proposals_before, &proposals_after),
            report.raised,
        ),
        rows,
    ))
}

/// What a save did — [`apply`]'s answer.
///
/// **The `Impact` is the same shape [`preview`] answers, on purpose.** Spec
/// C4 §4.2: the screen compares the two, and a save whose impact differs from
/// the preview it was made from is drawn as a warning rather than swallowed.
/// Nothing should change between them on a single-writer library; the screen's
/// entire promise is that the preview is true, and the cheapest place to
/// notice it is false is the save that just disproved it.
///
/// **`PartialEq`, not `Eq`.** [`crate::rules::Rule`] itself derives only
/// `PartialEq` — nothing about its fields forbids `Eq`, but nothing here
/// needs it either, since every comparison this module or its tests make is
/// `assert_eq!`/`assert_ne!`, both of which only ever require `PartialEq`.
#[derive(Debug, Clone, PartialEq)]
pub struct Applied {
    pub impact: Impact,
    /// Every proposal this derivation put back in front of someone, ascending
    /// — [`crate::proposals::reopened_ids`]. The ids, not the count, because
    /// spec C4 §1 asks the screen to *state* them after a save;
    /// `impact.proposals_reopened` is the same list's length.
    pub reopened_proposals: Vec<i64>,
    /// The rule table as this save left it, read back on the same connection
    /// once the save has committed — so a rule that arrived as `id: 0` comes
    /// back carrying the id it was actually written under and the editor
    /// needs no second request to learn it.
    ///
    /// **After the commit, not inside the transaction — whole-branch review,
    /// I6.** [`apply`]'s own doc comment carries the full argument; the short
    /// version is that this read (and [`Self::complaints`]'s) answers nothing
    /// [`Impact`] needs, so it has no business sitting between the derive
    /// landing and the commit that makes it durable.
    pub rules: Vec<crate::rules::Rule>,
    /// [`crate::rules::read`]'s complaints plus
    /// [`crate::rules::Engine::compile`]'s, the same pair `rules::import_json`'s
    /// route reports. A pattern that will not compile is dropped silently by
    /// the engine every derivation loads, so this is the only place a save can
    /// say so.
    pub complaints: Vec<String>,
    /// Everything [`crate::derive::derive_with`]'s `Documents::Reconcile` pass
    /// reported about the user's own filesystem — whole-branch review, I5.
    ///
    /// **The only `derive` caller that used to report nothing about
    /// `<root>/.taiyaku/`, on the only route in this product that writes
    /// there.** `Impact` describes the database's own structure and nothing
    /// past it; `repaired_documents`, `unwritable_documents`,
    /// `orphan_documents`, `declined_documents`, `stale_documents` and the
    /// fold counts all live on [`crate::derive::DeriveReport`] instead, so a
    /// save that could not write a hand document — a read-only root,
    /// [`crate::hand::save`]'s own `persisted: false` path — used to answer
    /// `200` with a rehome count and nothing else.
    ///
    /// **Carried whole rather than copied field by field.** `taiyaku-serve`
    /// already owns a renderer for every field of [`crate::derive::DeriveReport`]
    /// — `config_api::derive_report_json`, the one the scan route uses — and
    /// carrying the struct itself is what lets a save reuse it verbatim
    /// instead of this crate growing a second, partial one that could drift
    /// from it the moment a field is added to either.
    pub derive_report: crate::derive::DeriveReport,
}

/// Replace the rule table with `proposed` and derive over it, committed.
///
/// **[`preview_with_rows`] without the rollback, and with
/// [`crate::derive::Documents::Reconcile`] in place of `Skip`** — spec C4 §4.1.
/// A real derivation reconciles hand documents under `<root>/.taiyaku/`; a
/// preview must not, because a rollback would not undo a file it wrote. That
/// one enum value is the whole difference in what reaches the user's tree.
///
/// **Why this function exists rather than the sequence inlined in the route.**
/// [`crate::rules::replace_all`] is `pub(crate)`, which is correct — nothing
/// outside this crate should be able to rewrite the rule table without
/// deriving. Widening it to `pub` for one caller would export exactly the half
/// of the operation that is dangerous alone: a rule set replaced with nothing
/// derived over it is a library whose structure silently disagrees with the
/// rules that are supposed to have produced it, until somebody scans.
///
/// **One `BEGIN IMMEDIATE` over the whole library, the same trade
/// [`preview_with_rows`] makes and for the same reason.** A plain
/// [`crate::derive::derive`] is atomic per work rather than across the
/// library, so a half-derived library still has most of its works current.
/// This is not that: a save either lands whole or does not land, because a
/// rule set half-applied is a state nothing in this product knows how to
/// describe. The cost is the write lock, held for one full derivation —
/// measured at a median 281 ms on 130 series and 1858 works — which is why
/// the route takes [`crate::Library::working`] rather than the shared
/// connection.
///
/// **Nothing fallible sits between `derive_with` returning and the commit
/// below — whole-branch review, I6.** This module's own doc comment states
/// why [`preview_with_rows`] runs `Documents::Skip`: a rollback cannot undo a
/// file. `Documents::Reconcile` is exactly what this function runs instead,
/// so every `?` that used to sit between the derive landing and the commit
/// was a way for the database to roll back while the real files
/// `hand::reconcile`'s gap repair and `hand::rename_for_rekey` had already
/// written or renamed stayed exactly where the derive put them — a hand
/// document renamed to a `content_key` the database never adopted, reported
/// as an orphan on the next pass, someone's corrections detached from their
/// work.
///
/// Two of the reads that used to sit in that window stay there anyway:
/// `after`'s own [`grouping`] and [`crate::proposals::states`] genuinely have
/// to run on *this* transaction, because they describe what *this*
/// derivation did, not what the table happens to hold once something else
/// has had a chance to write to it — a read taken after the commit could be
/// answering a question this save was never asked. Both are cheap SELECTs on
/// a transaction already open, so keeping them costs nothing this restructuring
/// was trying to save. [`Impact`] and the reopened proposal ids are computed
/// from them before the commit for the same reason.
///
/// What actually moved is [`crate::rules::read`] and
/// [`crate::rules::Engine::compile`] — used only to populate
/// [`Applied::rules`] and [`Applied::complaints`], which answer nothing
/// [`Impact`] needs and have no business deciding whether this transaction is
/// allowed to land. Both now run after `tx.commit()`, on `conn` rather than
/// the consumed `tx` — reading the rule table back once it is durable rather
/// than while it is still provisional.
///
/// **The residue this leaves, because there is nowhere left to move it.** The
/// commit itself can still be refused, and a refusal there rolls the database
/// back with the files already written or renamed regardless — the one
/// window this restructuring narrows to a single statement rather than
/// closes. Nothing short of writing those files inside the same durability
/// boundary as the row that names them would close it the rest of the way,
/// and SQLite has no such boundary to offer.
///
/// # Errors
///
/// If SQLite refuses the transaction, if [`crate::rules::replace_all`] refuses
/// `proposed` (a nonzero id naming no row this table holds — the stale-editor
/// case — or one id named twice), if [`crate::derive::derive_with`] fails, if
/// the commit itself is refused, or if reading the rule table back afterward
/// fails.
pub fn apply(conn: &Connection, proposed: &[crate::rules::Rule]) -> Result<Applied, String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("starting the save transaction: {e}"))?;
    let before = grouping(&tx)?;
    let proposals_before = crate::proposals::states(&tx)?;

    crate::rules::replace_all(&tx, proposed)?;
    let report = crate::derive::derive_with(&tx, crate::derive::Documents::Reconcile)?;
    let after = grouping(&tx)?;
    let proposals_after = crate::proposals::states(&tx)?;

    let reopened_proposals = crate::proposals::reopened_ids(&proposals_before, &proposals_after);
    let impact = diff(&before, &after, reopened_proposals.len(), report.raised);

    // Explicit, for the reason `preview_with_rows`'s own `rollback` is
    // explicit: a commit that happens because nobody rolled back is a commit
    // nobody can find when reading this later.
    tx.commit()
        .map_err(|e| format!("committing the save: {e}"))?;

    // After the commit, on `conn` rather than the now-consumed `tx` — see
    // this function's own doc comment for why: neither answers a question
    // `impact` above needed, so neither has business sitting between the
    // derive landing and the commit that makes it durable.
    let (rules, mut complaints) = crate::rules::read(conn)?;
    let (_engine, compile_complaints) = crate::rules::Engine::compile(&rules);
    complaints.extend(compile_complaints);

    Ok(Applied {
        impact,
        reopened_proposals,
        rules,
        complaints,
        derive_report: report,
    })
}

// --- override detection ----------------------------------------------------

/// One field where a *later* enabled rule replaces an *earlier* one's
/// genuinely different answer, for at least one `scan_entry`.
///
/// **This is an observation, and the screen that draws it says so — O95.**
/// It was called a `Conflict` until 2026-09-05, and the word was wrong in the
/// one case the shipped defaults produce on nearly every library: migration
/// 3's rule 1 reads a series from the directory and its rule 4 reads it from
/// the filename, rule 4 wins, and **that is what folds a colour edition into
/// its main series** — rule 4's own seeded note says so. *(Since O89,
/// 2026-09-16: only where the directory does not qualify the filename's
/// series. A directory that extends it is reconciled back to rule 1's value
/// and is no longer an override. See `docs/status.md`, O89.)* Naming it a
/// conflict made the editor offer a reorder as the fix, and the reorder is
/// the one thing that breaks the fold. The detection below is unchanged and
/// correct; only the noun moved. See `docs/status.md`, O95.
///
/// **C4 §1's own worked example, and the reason this exists.** A rule sets
/// `edition_key = "raw"` for everything under `Manga (RAW)/`; a later rule
/// sets `edition_key = "en-color"` for anything ending "Full Color". Both
/// match `Manga (RAW)/Go-Toubun no Hanayome - Full Color`; the later wins;
/// a Japanese colour edition is labelled English. That is correct behaviour
/// — every rule did exactly what it says — producing a wrong answer, and
/// [`crate::rules::Engine::resolve`] cannot show it: `Resolved::provenance`
/// keeps only the *winner* per field, by construction, so a loser leaves no
/// trace for anything downstream of `resolve` to find. [`overrides`] is the
/// separate pass that looks for the loser instead of only ever seeing the
/// winner. Whether a given override is that mistake or the seeded fold above
/// is a question about **intent**, and intent is not a column — which is why
/// this struct reports both and judges neither.
///
/// **`first` and `wins` are indices into the `proposed` slice [`overrides`]
/// was called with — never a rule's database id.** The same boundary
/// [`Decided`] holds, for the same reason: a rule still being typed into
/// C4's editor carries `id: 0`, indistinguishable from a second unsaved rule
/// in the same call and from [`crate::rules::is_root_fallback`]'s own
/// sentinel, so nothing past this struct's construction may name a rule by
/// its `Rule::id`. See [`Decided`]'s own doc comment for the fuller
/// argument; [`overrides`]'s own doc comment covers the mechanism this
/// function uses to stay off that id entirely, including while it is
/// running, not merely at the boundary.
///
/// **`first_value` and `wins_value` are the values for `rel_path`, not for
/// every one of `examples`.** Which earlier rules actually ran, and so what
/// they left behind, is a per-`scan_entry` fact — the same one `others`
/// takes a maximum over below — so two entries sharing one
/// `(field, first, wins)` triple can hold different values. The pair kept
/// here is the one belonging to the entry `rel_path` names, captured in the
/// same step that captures the path, so a row's example file and its two
/// values can never describe different files.
///
/// **They exist because the row is now a judgement someone has to make.**
/// Naming the two rules says *that* they disagree; only the values say what
/// about, and `Death Note (Colored Council)` → `Death Note` reads instantly
/// as a deliberate normalisation where two regexes and a path do not.
///
/// *(Since O89, 2026-09-16: this example predates the reconciliation.
/// `Death Note (Colored Council)` qualifies the filename's captured
/// `Death Note`, so `rules::Engine::resolve` now restores the directory's
/// own value and this pairing is no longer reported as an override at all —
/// `docs/status.md`, O89.)*
///
/// **An override is about the *value*, not merely about who touched the
/// field.** Two enabled rules setting the same field to the *same* value are
/// not reported — the seeded defaults do exactly this on purpose: migration
/// 3's rule 1 captures a series from the parent directory, and its rule 4
/// captures the same series again from the filename "where it repeats it",
/// deliberately layering a confirmation over the default rather than
/// disagreeing with it. A detector that fired on any two rules touching one
/// field would flag that pairing on every ordinarily-named file in every
/// library this crate has a fixture for — noise on the layering model's own
/// worked example of itself working. `first` names the *most recent* earlier
/// rule whose value actually differs from what `wins` produces — not
/// necessarily the very first rule to touch the field at all, since an
/// intervening rule may have agreed with it before `wins` came along and
/// disagreed; "most recent disagreement" is the one the two on-screen
/// options (reorder, or edit the winner) are actually about.
///
/// **A disabled rule can never be `first` or `wins`.**
/// [`crate::rules::Engine::resolve`] skips a disabled rule outright — it
/// never reaches `apply_set_columns` or `apply_captures` for one — so it
/// never changes any field's value or provenance no matter how many enabled
/// rules move around it. The C4 editor shows disabled rules in the same
/// list as enabled ones; this struct is never built from one regardless.
///
/// **A rule cannot override itself.** [`overrides`] walks each rule at
/// most once per entry, so `first` and `wins` are always two distinct
/// indices whenever an [`Override`] exists at all. Two *different* rules
/// sharing one `ordinal` still resolve to a single order —
/// [`crate::rules::Engine::compile`]'s own tie-break, id ascending, which
/// this function reproduces rather than leaving to chance — so the lower-id
/// rule of such a pair is always processed first and can only ever be
/// `first`, never `wins`, against the higher-id one.
///
/// **`others` counts the disagreements `first` does not name, when three or
/// more enabled rules disagree on one field.** Decided by the owner
/// 2026-09-03, after a review found this chain-of-three-or-more case had
/// never been put to them. Rules setting one field to `"v1"`, `"v2"`, then
/// `"v3"` (all three distinct) produce one row: `first` names `"v2"`'s rule
/// — the most recent disagreement, per the paragraph above — `wins` names
/// `"v3"`'s, and `others` is `1`, for the `"v1"`-versus-`"v3"` disagreement
/// that row does not otherwise mention. It is real and it stays silent
/// until someone fixes the reported row and previews again; `others` is
/// what tells them there is more to find rather than letting the row imply
/// it is the whole story. In the ordinary two-rule case `others` is `0` and
/// the UI shows nothing extra. Rejected: leaving it silent, which
/// under-reports with nothing on screen saying so (the status quo this
/// decision replaces); and one row per disagreeing pair, N-choose-2 rows
/// for a case that is already rare and unchanged for the common two-rule
/// one. See [`overrides`]'s own doc comment for how the count is taken when
/// two entries share one `(field, first, wins)` triple but disagree on how
/// many other rules were involved.
///
/// **No `Eq`, deliberately.** [`FieldValue::Number`] holds an `f64`, so this
/// derives `PartialEq` alone. Nothing needs the stronger bound: this type is
/// never a map key or a set member, and [`overrides`]'s own grouping keys on
/// the `(Field, usize, usize)` triple rather than on the struct.
#[derive(Debug, Clone, PartialEq)]
pub struct Override {
    pub field: crate::rules::Field,
    pub rel_path: String,
    pub first: usize,
    pub wins: usize,
    pub first_value: FieldValue,
    pub wins_value: FieldValue,
    pub others: usize,
    pub examples: usize,
}

/// One field's value at one point in a resolution — this rather than the bare
/// `i64` [`crate::rules::Resolved::provenance`] records for it, because that
/// id is exactly the thing [`overrides`] must not compare: two rules that
/// both set a field to the *same* string are not an override (see
/// [`Override`]'s own doc comment), and telling that apart from "nothing
/// changed" needs the actual value, not who most recently wrote it.
///
/// **Public, and typed rather than stringified**, since 2026-09-05: an
/// [`Override`] carries two of these out to the editor, and a `volume` is a
/// number all the way to the wire the way [`Placement`]'s own fields are.
/// `None` in either arm is a field the resolution left unset — which is a
/// real half of a disagreement, not an absence of one, since a later rule
/// clearing what an earlier rule set is exactly the case a reader needs to
/// see.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    Text(Option<String>),
    Number(Option<f64>),
}

/// The value [`FieldValue`] holds for `field` in `resolved` — the one place
/// this module reaches into each of [`crate::rules::Resolved`]'s seven typed
/// fields by name, so every other function here can compare two values
/// generically instead of matching on [`crate::rules::Field`] itself.
fn field_value(resolved: &crate::rules::Resolved, field: crate::rules::Field) -> FieldValue {
    match field {
        crate::rules::Field::Series => FieldValue::Text(resolved.series.clone()),
        crate::rules::Field::EditionKey => FieldValue::Text(resolved.edition_key.clone()),
        crate::rules::Field::Kind => FieldValue::Text(resolved.kind.clone()),
        crate::rules::Field::Language => FieldValue::Text(resolved.language.clone()),
        crate::rules::Field::Title => FieldValue::Text(resolved.title.clone()),
        crate::rules::Field::Volume => FieldValue::Number(resolved.volume),
        crate::rules::Field::Chapter => FieldValue::Number(resolved.chapter),
    }
}

/// One [`Override`] under construction, while [`overrides`] is still walking
/// the entries that share its `(field, first, wins)` key.
///
/// **A struct rather than the tuple this was**, which clippy's own
/// `type_complexity` refused once the two values joined it — and the names are
/// worth more than the brevity here anyway, since `examples` and `others` are
/// both counts and `first_value` and `wins_value` are both values, so a
/// positional read of either pair is one transposition away from a row that
/// says something untrue.
struct PartialOverride {
    /// The entry this row will name, and the entry both values below belong
    /// to. First encountered wins, and nothing overwrites it.
    rel_path: String,
    examples: usize,
    others: usize,
    first_value: FieldValue,
    wins_value: FieldValue,
}

/// Every field where two or more of `proposed`'s enabled rules would decide
/// a `scan_entry` differently — see [`Override`]'s own doc comment for
/// exactly what "differently" excludes, for why `first`/`wins` are
/// positions in `proposed` rather than rule ids, and for why the noun is
/// "override" rather than "conflict".
///
/// **No transaction, and no derivation.** Unlike [`preview`] and
/// [`preview_with_rows`], this reads `scan_entry` and the rule set as they
/// stand — it never calls [`crate::rules::replace_all`] or
/// [`fn@crate::derive::derive_with`], so it takes no write lock and pays
/// none of a derivation's own cost (281 ms on a real library — `docs/notes.md`,
/// "What a rule preview costs on a real library"). That is also why it
/// cannot lean on a minted database id the way [`rows_for`] does through
/// `written_ids`: see the next paragraph.
///
/// **How this stays off `Rule::id` without ever writing one.** `proposed`
/// may hold rules with `id: 0` — unsaved, indistinguishable from each other
/// and from [`crate::rules::is_root_fallback`]'s own sentinel — so this
/// function first relabels a private copy of `proposed`, giving every `id: 0`
/// rule a synthetic id far above any real one (`i64::MAX` downward, ordered
/// by the rule's own position in `proposed`, matching where
/// [`crate::rules::replace_all`] would actually mint it: always higher than
/// any kept row, regardless of list position). A saved rule keeps its real
/// id, so two saved rules sharing an `ordinal` still tie-break the way
/// [`crate::rules::Engine::compile`] genuinely orders them. This relabelling
/// is never written anywhere and never returned — every [`Override`] this
/// function produces names a position in `proposed`, translated back through
/// the same index this relabelling was built from, not the relabelled id
/// itself.
///
/// **The mechanism: one `Engine` per prefix of the sorted rule list.**
/// [`crate::rules::Engine::resolve`] reports only the *winning* rule per
/// field — by construction, since that is the whole of what "a later rule
/// overrides only the fields it actually sets" means. To find every rule
/// that set a field, not only the winner, this compiles one `Engine` per
/// rule count from zero (root defaults alone) up to every rule
/// (`order.len() + 1` engines for `order.len()` rules) and resolves each
/// `scan_entry` against every one of them. Comparing consecutive engines'
/// `provenance` for one entry finds exactly which rule, if any, changed a
/// field at that step — and since a rule's own match/capture logic never
/// depends on any other rule, the only thing that can differ between
/// `engines[k]` and `engines[k + 1]` is `engines[k + 1]`'s one extra rule.
///
/// *(Not quite, since O89. Adding a capturing rule can also trigger
/// [`crate::rules::Engine::resolve`]'s reconciliation, which restores an
/// earlier directory rule's series. Where a user series rule sits between
/// that directory rule and the capture, over a qualifying directory, the
/// chain step is credited to the capturing rule with a value it never
/// produced — O128, `docs/status.md`.)*
///
/// [`crate::rules::Engine::compile`] has no incremental form, so each of these engines is
/// compiled from scratch — a rule already compiled into `engines[5]` is
/// compiled again into `engines[6]`, `engines[7]`, and so on.
///
/// **Cost: quadratic in rules, linear in files** — C4 §11's own estimate,
/// unmeasured until now and still unmeasured at any real scale: this
/// crate's seeded rule set is four rules, and nobody has run this against
/// seventy. Building `engines` compiles `0 + 1 + … + order.len()` rules in
/// total, quadratic in the rule count for the reason above; resolving every
/// `scan_entry` against every engine is `O(rules × files)`, linear in files
/// for a fixed rule count. `fields_settable` is the one thing in this
/// function computed per rule rather than per `(rule, scan_entry)` pair —
/// `O(rules)`, once, before the roots loop below — which is what keeps this
/// estimate true rather than silently short by an extra `O(rules × files)`
/// worth of `Regex::new` calls, the expensive one by an order of magnitude
/// per `rules.rs`'s own module doc. Whether either shape is affordable on a
/// library of thousands rather than a handful is exactly the gap C4 §11
/// flags and this function does nothing to close. Carrying the two values
/// out on each row costs nothing on top: they were already computed, to tell
/// a disagreement from an agreement, and were previously thrown away.
///
/// # Errors
///
/// If listing roots or reading `scan_entry` fails.
pub fn overrides(
    conn: &Connection,
    proposed: &[crate::rules::Rule],
) -> Result<Vec<Override>, String> {
    // Every proposed rule, relabelled so its `id` can never collide with
    // `crate::rules::is_root_fallback`'s own sentinel or with another
    // proposed rule — see this function's own doc comment. `order` is the
    // resolution order `crate::rules::Engine::compile` itself sorts into:
    // `(ordinal, id)` ascending, ties broken on the relabelled id exactly as
    // `compile` breaks them on the real one.
    let relabelled: Vec<crate::rules::Rule> = proposed
        .iter()
        .enumerate()
        .map(|(index, rule)| {
            if rule.id == 0 {
                crate::rules::Rule {
                    id: i64::MAX - (proposed.len() as i64) + index as i64,
                    ..rule.clone()
                }
            } else {
                rule.clone()
            }
        })
        .collect();
    let mut order: Vec<usize> = (0..relabelled.len()).collect();
    order.sort_by_key(|&i| (relabelled[i].ordinal, relabelled[i].id));

    // `engines[0]` holds no rules at all — root defaults only — and
    // `engines[k]` holds `order`'s first `k` rules, so `engines[k + 1]` is
    // always exactly `engines[k]` plus `order[k]`'s own rule.
    let engines: Vec<crate::rules::Engine> = (0..=order.len())
        .map(|k| {
            let subset: Vec<crate::rules::Rule> =
                order[..k].iter().map(|&i| relabelled[i].clone()).collect();
            crate::rules::Engine::compile(&subset).0
        })
        .collect();

    // Every field each of `relabelled`'s rules could possibly decide,
    // computed once per rule rather than once per `(rule, scan_entry)` pair.
    // `crate::rules::fields_settable_by`'s result depends only on `rule` —
    // never on which entry is being resolved — so computing it inside the
    // loop below would recompile the same handful of regexes for every file
    // in the library, exactly the cost `rules.rs`'s own module doc calls
    // `Regex::new` out for "by an order of magnitude" next to matching
    // against an already-compiled one. Indexed the same way `relabelled`
    // itself is, so `fields_settable[rule_index]` below never needs its own
    // lookup table.
    let fields_settable: Vec<std::collections::BTreeSet<crate::rules::Field>> = relabelled
        .iter()
        .map(crate::rules::fields_settable_by)
        .collect();

    let roots = crate::scan::roots(conn)?;
    // Keyed on the triple the row is *about*; the value carries the one entry
    // the row names, its two values, and the counts taken across every entry
    // that shares the key.
    let mut grouped: std::collections::BTreeMap<
        (crate::rules::Field, usize, usize),
        PartialOverride,
    > = std::collections::BTreeMap::new();

    for root in &roots {
        for rel_path in rel_paths(conn, root.id)? {
            let mut prev = engines[0].resolve(root, &rel_path);
            // Per field, every rule that changed it for this entry, in
            // resolution order, each with the value it left behind —
            // [`FieldValue`], not the raw provenance id, since telling a
            // genuine disagreement apart from two rules agreeing needs the
            // value itself.
            let mut chains: std::collections::BTreeMap<
                crate::rules::Field,
                Vec<(usize, FieldValue)>,
            > = std::collections::BTreeMap::new();
            for (k, &rule_index) in order.iter().enumerate() {
                let cur = engines[k + 1].resolve(root, &rel_path);
                // Restricted to the fields this one rule could possibly
                // decide — the hoisted `fields_settable`, never this
                // function reading `set_edition_key` and the rest itself —
                // so a field some earlier or later rule owns is never
                // compared on this rule's account at all.
                for &field in &fields_settable[rule_index] {
                    if cur.provenance.get(&field) != prev.provenance.get(&field) {
                        chains
                            .entry(field)
                            .or_default()
                            .push((rule_index, field_value(&cur, field)));
                    }
                }
                prev = cur;
            }

            for (field, chain) in chains {
                let Some(((wins_index, wins_value), earlier)) = chain.split_last() else {
                    continue;
                };
                // Every earlier rule whose value for this field actually
                // differs from the winner's, most recent first, each still
                // carrying that value — see `Override`'s own doc comment for
                // why "disagreement" rather than "touched the field" is the
                // bar, for what `others` means below, and for why the pair
                // of values kept is the one belonging to this entry.
                let disagreeing: Vec<(usize, &FieldValue)> = earlier
                    .iter()
                    .rev()
                    .filter(|(_, value)| value != wins_value)
                    .map(|(rule_index, value)| (*rule_index, value))
                    .collect();
                let Some(&(first_index, first_value)) = disagreeing.first() else {
                    continue;
                };
                // `first` (above) already names the most recent
                // disagreement; `others` counts every earlier one besides
                // it — `0` in the ordinary two-rule case, and the count a
                // chain of three or more disagreeing rules would otherwise
                // leave unreported past the single row `first`/`wins` name.
                let others = disagreeing.len() - 1;
                let entry = grouped
                    .entry((field, first_index, *wins_index))
                    .or_insert_with(|| PartialOverride {
                        rel_path: rel_path.clone(),
                        examples: 0,
                        others,
                        first_value: first_value.clone(),
                        wins_value: wins_value.clone(),
                    });
                entry.examples += 1;
                // Two entries can share one `(field, first, wins)` triple
                // yet disagree on how many *other* rules were involved —
                // which earlier rules actually ran, and so which of them
                // disagreed, is a per-`scan_entry` fact, not a per-triple
                // one. Taking the maximum means the row never claims fewer
                // disagreements than some entry it covers actually has —
                // the same under-reporting `others` exists to rule out in
                // the first place. The two *values* are not maximised and
                // not overwritten for the same reason inverted: they are
                // the named entry's own, and a row whose example file and
                // values came from different entries would be a worse lie
                // than an under-count.
                entry.others = entry.others.max(others);
            }
        }
    }

    Ok(grouped
        .into_iter()
        .map(|((field, first, wins), grouped)| Override {
            field,
            rel_path: grouped.rel_path,
            first,
            wins,
            first_value: grouped.first_value,
            wins_value: grouped.wins_value,
            others: grouped.others,
            examples: grouped.examples,
        })
        .collect())
}

#[cfg(test)]
mod tests;
