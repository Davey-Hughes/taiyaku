//! The read queries a browse screen needs, with the per-root permission
//! filter applied in SQL rather than after it.
//!
//! Spec C1 §5. One responsibility: reading rows for a screen. Nothing here
//! writes, and nothing here formats — `taiyaku-serve`'s `library_api` turns
//! these into JSON.
//!
//! # The filter, which is the reason this module exists rather than a handful
//! # of queries at the call sites
//!
//! Every function takes `roots: &[i64]`, the caller's granted roots, and **an
//! empty slice applies no filter at all**. That is spec C1 §5.1's rule and it
//! is counter-intuitive on purpose: `user_root` had no writer before spec C1,
//! so every account in every existing library holds an empty grant, and any
//! other reading of empty would have emptied every library the day migration
//! 6 applied.
//!
//! The filter is applied to **editions**: an edition survives if its members
//! do, and a work is listed if it keeps at least one edition. A member under
//! no root — every member of every imported work — has `root_id` NULL, and
//! `IN (…)` excludes NULL by SQL's own rules, so that case needs no branch.
//!
//! An **alignment** names two editions, so it survives only if *both* of them
//! do — both inside the grant, and both under the `?root=` narrowing whenever
//! one is in force. One clause says so — `pair_clause` — and `alignments_of`,
//! `WorkCard::aligned` and `SeriesCard::aligned` all read it, because the two
//! counts asked a weaker question until 2026-08-21 and a grid drawing `⇄ 1`
//! beside a work page listing no pair is one screen contradicting itself.
//!
//! Filtering after the query was the alternative and is wrong twice over: it
//! reads rows the caller may not see into memory before deciding, and it makes
//! every count (`works`, `editions`, `aligned`) a number computed over the
//! unfiltered set.
//!
//! # Every placeholder in these queries is anonymous `?`, never `?1`
//!
//! SQLite numbers an anonymous parameter *one higher than the largest index
//! assigned so far*, so a single `?3` earlier in a statement pushes the
//! following `?`s to 4 and 5 — and a query built by concatenating a caller's
//! fragment onto a generated `IN (?,?)` cannot know what those indices are.
//! Measured on 2026-08-20 against the real schema: `WHERE e.work_id = ?3 AND …
//! IN (?,?)` counts **five** parameters, not three, and `rusqlite` refuses the
//! call.
//!
//! Four constant statements — the three FTS id lookups and `cover_work`'s
//! `series` read — carried `?1` until 2026-08-21 and were converted rather
//! than exempted. Nothing broke while each had a single parameter, but
//! `matching_unfiled_works` is exactly where a search-by-root would one day
//! add a generated `IN (?,?)` beside that `?1`, and a rule with four standing
//! exceptions is one a later edit will mis-remember. (The hand-written
//! `INSERT`s in the `tests` submodule are fixtures, not queries, and are
//! outside this.)
//!
//! Anonymous-only means the binding order is the **text order of the
//! statement**, and that is the whole rule: each function pushes its
//! parameters in the order the fragments it splices appear in the SQL, reading
//! left to right from the `SELECT` list. It is deliberately *not* "roots
//! first, then the caller's own parameters" — that was true until 2026-08-21,
//! when `?root=` was threaded through `pair_clause` and every restricted
//! edition became roots-then-`only`, interleaving the two groups. `series` now
//! binds `(roots…, only?)` three times over — the alignment's from side, its
//! to side, then the edition join — and only then `user_id`. Every function
//! that mixes groups states its own order in a comment above the `format!`;
//! read that against the params vector directly below it.

// **Imported rather than written `std::collections::HashMap` inline, and that
// is load-bearing for the mutation harness.** `cargo-mutants` replaces a
// function body with a literal it generates — `Ok(HashMap::new())` — and emits
// the short name, so while this module spelled the path out in full every such
// mutant failed to *compile* and was reported `unviable`. O85's own two
// functions had **fourteen** whole-body mutants and all fourteen were unviable,
// which is a zero-survivor result that establishes nothing about them. Measured
// 2026-08-29, before and after this line.
use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension as _, params_from_iter};

#[cfg(test)]
thread_local! {
    /// Every statement this module issues, counted.
    ///
    /// **A thread-local counter rather than `rusqlite`'s `trace` feature**,
    /// the same choice `proposals::tests`' one-sweep check made and for the
    /// same reason: no Cargo feature has to be turned on for a test to see
    /// it, and thread-local rather than a shared `static` so a test running
    /// concurrently on another thread cannot land inside this one's
    /// reset-then-read window.
    ///
    /// **Statements, not calls to a named function.** The claim worth holding
    /// is O85's — that a listing's cost does not grow with the number of works
    /// in it — and a call counter on the per-work loop would go to zero the
    /// moment that loop is deleted, passing for the wrong reason.
    pub(crate) static STATEMENTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// `Connection::prepare`, counted in test builds.
///
/// **Every query in this module goes through this or [`query_row`]**, so the
/// count is the module's whole SQL cost rather than the part somebody
/// remembered to instrument. A new query added straight onto `conn` is
/// invisible to O85's check and silently weakens it.
fn prepare<'c>(conn: &'c Connection, sql: &str) -> rusqlite::Result<rusqlite::Statement<'c>> {
    #[cfg(test)]
    STATEMENTS.with(|c| c.set(c.get() + 1));
    conn.prepare(sql)
}

/// `Connection::query_row`, counted in test builds. See [`prepare`].
fn query_row<T, P, F>(conn: &Connection, sql: &str, params: P, f: F) -> rusqlite::Result<T>
where
    P: rusqlite::Params,
    F: FnOnce(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
{
    #[cfg(test)]
    STATEMENTS.with(|c| c.set(c.get() + 1));
    conn.query_row(sql, params, f)
}

/// One series as a grid tile.
///
/// `works`, `editions` and `aligned` are counts **after** the filter, which is
/// why they are computed in SQL: a count over the unfiltered set is the exact
/// bug that filtering in Rust would introduce.
#[derive(Debug, Clone, PartialEq)]
pub struct SeriesCard {
    pub id: i64,
    pub title: String,
    pub works: i64,
    pub editions: i64,
    /// Works with at least one alignment **whose two editions both survive
    /// the filter** — spec C1 §3's `⇄ 3` mark. A series page that lists
    /// volumes without saying which are aligned is a Komga screen with extra
    /// badges; one that counts a pair half of which this reader cannot open
    /// contradicts the work page, which lists that pair to nobody.
    pub aligned: i64,
    pub cover_work: Option<i64>,
    /// The highest `work.id` in the series — spec C1 §5.5's recency proxy.
    ///
    /// The same one `home`'s `added` list already sorts on (`w.id DESC`). A
    /// real `work.created_at` is what would replace it, and a migration for a
    /// sort order is not worth one while every row any scan has produced sorts
    /// identically under both.
    pub added: i64,
    /// Works in this series with no `progress` row for the calling reader on
    /// **any** edition.
    ///
    /// The key has been `(user_id, work_id, edition)` since migration 9, so
    /// "no row" and "no row for the edition they are in" are different
    /// questions; the design mandated the first and the `LEFT JOIN` answers
    /// it — every joined row for a work carries a non-NULL `p.work_id` as
    /// soon as the reader holds one position anywhere in it.
    ///
    /// The `Unread` chip means `unread == works`: nothing here started.
    /// `unread > 0` was the alternative and leaves a 14-volume series in the
    /// chip forever after volume 1.
    pub unread: i64,
}

/// One edition of one work, as a series page or a work page shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct EditionCard {
    pub key: String,
    pub label: String,
    pub members: i64,
    pub pages: i64,
    /// How many of `members` no longer exist on disk — spec C1 §8's empty
    /// state. See [`missing_by_edition`].
    pub missing: usize,
}

/// One work, with only the editions this caller may see.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkCard {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub volume: Option<f64>,
    /// **Only the editions this caller may see.** Never empty: a work with no
    /// surviving edition is not listed at all.
    pub editions: Vec<EditionCard>,
    /// Whether any alignment of this work has **both** its editions inside the
    /// grant — the same question `WorkDetail::alignments` answers in detail,
    /// asked as one bit. They were separate pieces of SQL and disagreed.
    pub aligned: bool,
    pub pages: i64,
    /// **The edition this caller's reader will actually open — O84.**
    ///
    /// Not the hub, and the difference is the whole item. `startup.ts` calls
    /// `reading::restore(progress, hub, resolver)`, which opens the *stored
    /// position's* edition when there is one the resolver knows and the hub
    /// otherwise; `api.ts::getProgress` takes the head of a newest-first list,
    /// because a position became per-edition in migration 9. So the edition
    /// that opens is the caller's newest position **if the grant still shows
    /// it**, and `work.hub` in every other case — a progress row naming an
    /// edition this caller cannot see is one `resolver.slotCount` answers
    /// `undefined` for, which is exactly `restore`'s hub fallback.
    ///
    /// **This is what makes `WorkCard` per-caller.** It was user-independent
    /// until O84, and `works_of` took no `user_id`; a work whose hub has lost
    /// its files while a sibling has not is readable for one reader and not
    /// for another, so no user-independent field can answer it. Withholding on
    /// the *hub* alone was the cheaper answer and drops a work out of
    /// `Library.tsx`'s Continue strip while its reader is part-way through the
    /// intact sibling — O62's over-withholding, in a narrower form.
    ///
    /// Always a key of one of `editions` while the grant shows the hub. See
    /// [`opens_by_work`].
    pub opens: String,
}

/// One edition pair's alignment, as §3's panel shows it.
///
/// **One row per pair, never a scalar on the work.** A work with three
/// editions has no single verdict, and picking one pair's would repeat §5.1's
/// `work.root_id` mistake somewhere new: a field with nothing true to hold that
/// a screen then presents as fact.
#[derive(Debug, Clone, PartialEq)]
pub struct AlignmentCard {
    pub from: String,
    pub to: String,
    pub verdict: Option<String>,
    pub slots: Option<i64>,
    pub paired: Option<i64>,
}

/// One work with its alignment detail — what `/api/library/works/{slug}`
/// answers and **what no list route carries**.
///
/// `works_of` answers a whole listing's editions in two statements since O85,
/// and an `alignments_of` per row would put back exactly the N+1 that fix
/// removed — the shape the whole-branch review closed in `home` at 111 queries
/// on the largest series, and reopened here once already. §3's panel opens one
/// work at a time, so this is the route that fills it.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkDetail {
    pub card: WorkCard,
    pub alignments: Vec<AlignmentCard>,
}

/// A grid, or a search over one: series, and the works that belong to none.
///
/// **Spec C1 §5.4.** `import` never writes `series_id`, so every hand-imported
/// work has no series and no series-joined query can see it. Both the grid and
/// search carry the same shape so one component renders either.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchResults {
    pub series: Vec<SeriesCard>,
    pub unfiled: Vec<WorkCard>,
}

/// The three lists the library screen opens on.
#[derive(Debug, Clone, PartialEq)]
pub struct Home {
    pub continuing: Vec<WorkCard>,
    pub added: Vec<WorkCard>,
    pub read: Vec<WorkCard>,
}

/// What the home lists and search return at most.
///
/// Not a parameter, because no caller has a reason to differ and a cap the
/// caller chooses is a cap the caller forgets. Spec C1 §7 declines
/// virtualisation on measured numbers — 127 series, 111 volumes in the largest
/// — so this bounds the three *recency* lists rather than the grid.
const LIST_CAP: i64 = 20;

/// The `AND` clause that restricts an edition to the caller's roots — empty
/// when the grant is empty.
///
/// Built as a string because the number of parameters varies with the grant,
/// which a prepared statement cannot express otherwise. The values are `i64`
/// read from `user_root`, never caller text, so there is nothing here to
/// inject; the placeholders are still generated rather than the values
/// interpolated, so that stays true if the source ever changes.
///
/// `rm`, not `m`: two callers below already have a `member m` in scope, and an
/// inner alias that shadows an outer one is legal SQL, correct here, and one
/// edit away from silently meaning the other thing.
fn root_clause(roots: &[i64]) -> String {
    root_clause_for("e", roots)
}

/// [`root_clause`], against an edition alias the caller names.
///
/// **The alias is a parameter because [`aligned_clause`] restricts two
/// *different* editions inside one subquery** — an alignment's two sides — and
/// the alternative is calling both of them `e`. That compiles, binds and runs:
/// an inner alias shadows the outer one, which is exactly the hazard
/// [`member_clause`]'s doc names, and here it would be invisible, since the
/// clause would still restrict *an* edition, just not always the intended one.
///
/// `alias` is a literal at every call site, never caller text — the same rule
/// the `predicate` and `order` fragments below follow.
fn root_clause_for(alias: &str, roots: &[i64]) -> String {
    if roots.is_empty() {
        return String::new();
    }
    let holes = holes(roots.len());
    format!(
        " AND EXISTS (SELECT 1 FROM member rm WHERE rm.edition_id = {alias}.id \
         AND rm.root_id IN ({holes}))"
    )
}

/// `?,?,?` for an `IN` list of `n` values.
///
/// Spelled out inline in [`root_clause_for`], [`member_clause`] and [`status`]
/// before O85 added two more `IN` lists; one place to read the rule the module
/// header states, that every placeholder here is anonymous.
fn holes(n: usize) -> String {
    std::iter::repeat_n("?", n).collect::<Vec<_>>().join(",")
}

/// The `AND` restricting a joined `member m` to the caller's roots.
///
/// **Distinct from [`root_clause`], and the whole-branch review found out
/// why.** `root_clause` decides whether an *edition* survives; this decides
/// which of its *members* are counted. Without it, the edition listing's
/// `LEFT JOIN member m` counted every member of a surviving edition — so a
/// reader granted one root saw member and page counts covering roots they
/// cannot see, and the module header's claim that counts are computed after
/// the filter was true of `SeriesCard` and false of `EditionCard`.
///
/// Reachable in the *default* configuration rather than in a corner: two roots
/// added with `library scan --add` carry no `edition_key`, both fall back to
/// `raw`, and derivation merges their members into one edition.
fn member_clause(roots: &[i64]) -> String {
    if roots.is_empty() {
        return String::new();
    }
    let holes = holes(roots.len());
    format!(" AND m.root_id IN ({holes})")
}

/// The `AND` restricting an edition to **one** root, on top of the grant.
///
/// Spec C1 §5.5's `?root=`. Intersected with [`root_clause`] rather than
/// replacing it: a root the caller does not hold yields no rows, which is the
/// same answer as a root that does not exist. Confirming which ids are real is
/// the enumeration `one_series` already answers 404 rather than 403 to avoid.
fn only_clause(only: Option<i64>) -> String {
    only_clause_for("e", only)
}

/// [`only_clause`], against an edition alias the caller names.
///
/// **The alias is a parameter for [`root_clause_for`]'s reason**, and for the
/// same clause: [`pair_clause`] narrows two *different* editions — an
/// alignment's two sides — inside one subquery, and calling both of them `e`
/// would compile, bind, run, and silently narrow whichever edition the inner
/// scope happened to resolve.
fn only_clause_for(alias: &str, only: Option<i64>) -> String {
    if only.is_none() {
        return String::new();
    }
    format!(
        " AND EXISTS (SELECT 1 FROM member om WHERE om.edition_id = {alias}.id \
         AND om.root_id = ?)"
    )
}

/// The two `AND EXISTS` halves that keep an alignment row `a` only when
/// **both** of the editions it names survive the grant.
///
/// **One definition, read by both questions the `alignment` table answers**,
/// because the whole-branch review found them disagreeing: with an `en`↔`raw`
/// pair and a grant of `en` alone, the grid drew `⇄ 1` and the work reported
/// `aligned: true`, while the same work's alignment list — which had this
/// filter and the counts did not — was empty. The same screen contradicted
/// itself, so [`aligned_clause`] and `alignments_of` now read one clause.
///
/// **`only` restricts both sides too, and until 2026-08-21 it restricted
/// neither.** `?root=` was spliced into `series`'s edition join alone, so
/// `works`, `editions` and `unread` were narrowed and the `SELECT`-list
/// `aligned` count was not: `series(&conn, &[], 1, Some(en_id))` over a
/// two-root library with one `en`↔`raw` pair answered `editions: 1` beside
/// `aligned: 1` — a tile claiming a pair while reporting the one edition that
/// can hold half of it. The same self-contradiction the grant half had, in the
/// half of the filter the grant fix did not reach.
///
/// `fe` and `te` rather than `e` twice: see [`root_clause_for`]. Contributes
/// **two** parameter groups, the from side first, each of them the grant's
/// roots followed by `only` — see the module header on why the groups
/// interleave rather than roots leading.
fn pair_clause(roots: &[i64], only: Option<i64>) -> String {
    let from = root_clause_for("fe", roots);
    let from_only = only_clause_for("fe", only);
    let to = root_clause_for("te", roots);
    let to_only = only_clause_for("te", only);
    format!(
        "EXISTS (SELECT 1 FROM edition fe \
                  WHERE fe.work_id = a.work_id AND fe.key = a.from_key{from}{from_only}) \
         AND EXISTS (SELECT 1 FROM edition te \
                  WHERE te.work_id = a.work_id AND te.key = a.to_key{to}{to_only})"
    )
}

/// Whether work `w` carries an alignment this caller may see, as an expression.
///
/// **A count after the filter, which until 2026-08-21 it was not.** Both
/// `aligned` counts read `EXISTS (SELECT 1 FROM alignment a WHERE a.work_id =
/// w.id)`, which asks nothing about roots — so `WorkCard.aligned` and
/// `SeriesCard.aligned` were computed over the *unfiltered* alignment set,
/// the exact number the module header says filtering in Rust would produce,
/// in the module whose reason for existing is that it does not. Shipped in
/// PR #81; `alignments_of` is what made the disagreement observable.
///
/// Requires `w` in scope, and contributes [`pair_clause`]'s two groups. The
/// correlation `a.work_id = w.id` is written first so those groups still bind
/// ahead of whatever the caller's own statement carries.
fn aligned_clause(roots: &[i64], only: Option<i64>) -> String {
    let pair = pair_clause(roots, only);
    format!("EXISTS (SELECT 1 FROM alignment a WHERE a.work_id = w.id AND {pair})")
}

/// Every series this caller may see, with its counts, ordered by title.
///
/// `is_primary` picks the display title; `series_title` holds several names per
/// series so the romaji, the native title and the English one all find the same
/// shelf. A series with no primary title falls back to `sort_title`, which is
/// NOT NULL — without that fallback a series whose titles were all written with
/// `is_primary = 0` would be invisible.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn series(
    conn: &Connection,
    roots: &[i64],
    user_id: i64,
    only: Option<i64>,
) -> Result<Vec<SeriesCard>, String> {
    let aligned = aligned_clause(roots, only);
    let clause = root_clause(roots);
    let narrowed = only_clause(only);
    // Placeholders bind in text order, all anonymous — three groups of
    // `(roots…, only?)` and then `user_id`:
    //
    //   1. the `aligned` count's from side (`fe`), in the `SELECT` list, which
    //      is the earliest text a parameter can appear in;
    //   2. its to side (`te`);
    //   3. the edition join's `{clause}{narrowed}`;
    //   4. `user_id`, in the progress join below.
    //
    // See the module header: `only` rides *inside* each group rather than
    // trailing all of them, which is why the old "roots first, then everything
    // else" phrasing no longer describes this. The progress join still sits
    // *after* the edition join: as a `SELECT`-list subquery its parameter
    // would bind ahead of every group above, and the filter's own parameters
    // are the only thing allowed in front of it — which is also why the
    // `aligned` count may live where it does.
    //
    // **`aligned` is an `EXISTS` rather than the `LEFT JOIN alignment a` it
    // was.** The join could carry the same restriction in its `ON`, but it
    // sits below the progress join, so its roots would bind *after* `user_id`;
    // and it multiplied every grouped row by the number of alignments, which
    // only `COUNT(DISTINCT …)` everywhere else was hiding.
    let sql = format!(
        "SELECT s.id,
                COALESCE(MAX(CASE WHEN st.is_primary = 1 THEN st.title END), s.sort_title),
                COUNT(DISTINCT w.id),
                COUNT(DISTINCT e.id),
                COUNT(DISTINCT CASE WHEN {aligned} THEN w.id END),
                s.cover_work_id,
                MAX(w.id),
                COUNT(DISTINCT CASE WHEN p.work_id IS NULL THEN w.id END)
           FROM series s
           JOIN work w ON w.series_id = s.id
           JOIN edition e ON e.work_id = w.id{clause}{narrowed}
           LEFT JOIN progress p ON p.work_id = w.id AND p.user_id = ?
           LEFT JOIN series_title st ON st.series_id = s.id
          GROUP BY s.id
          ORDER BY 2 COLLATE NOCASE"
    );
    let mut params: Vec<i64> = Vec::new();
    for _ in 0..3 {
        params.extend(roots.iter().copied());
        params.extend(only);
    }
    params.push(user_id);

    let mut stmt = prepare(conn, &sql).map_err(|e| format!("preparing the series listing: {e}"))?;
    let rows = stmt
        .query_map(params_from_iter(params), |row| {
            Ok(SeriesCard {
                id: row.get(0)?,
                title: row.get(1)?,
                works: row.get(2)?,
                editions: row.get(3)?,
                aligned: row.get(4)?,
                cover_work: row.get(5)?,
                added: row.get(6)?,
                unread: row.get(7)?,
            })
        })
        .map_err(|e| format!("listing series: {e}"))?;
    rows.collect::<rusqlite::Result<Vec<SeriesCard>>>()
        .map_err(|e| format!("reading series: {e}"))
}

/// Every edition of every work in `work_ids`, keyed by work id, in key order.
///
/// **One statement for the whole set, and one more for every edition's
/// `missing` — O85.** This was `editions_of`, called once per work by
/// [`works_of`], which then called `missing_members` once per edition: an N+1
/// nested inside an N+1, in the very module whose own comment on
/// [`WorkDetail`] cites the whole-branch review's 111-query fix in `home` as
/// the precedent not to repeat. Measured on the two-root fixture
/// `library_with_volumes` builds, before the fix: **2 + 3n statements** for an
/// n-work series — 8 at two works, 20 at six — against 4 for any n after it.
///
/// A work whose editions the grant wholly hides has **no key in the map**
/// rather than an empty vector, which is [`visible_editions`]' rule for the same
/// question and keeps `WorkCard::editions`' "never empty" contract readable at
/// the one place that fills it.
///
/// **The `IN` list is bounded by SQLite's parameter ceiling, which this trades
/// one unbounded thing for another against.** `unfiled` takes no `LIMIT`, so
/// `work_ids` is as long as the caller's set; where the old shape issued one
/// query per work and could never exceed a parameter count, this issues one
/// query with one placeholder per work. Measured on this machine's system
/// libsqlite3, 3.53.4, 2026-08-29: **at least 200,000** bound parameters bind
/// without complaint, against a library that would need that many works in a
/// single listing to reach it. Recorded rather than guarded, because a guard
/// would be code no library can execute.
///
/// Placeholders bind in text order, all anonymous: the member join's roots,
/// then the `EXISTS` clause's roots, then `work_ids`. See the module header.
fn editions_by_work(
    conn: &Connection,
    roots: &[i64],
    work_ids: &[i64],
) -> Result<HashMap<i64, Vec<EditionCard>>, String> {
    if work_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let joined = member_clause(roots);
    let clause = root_clause(roots);
    let wanted = holes(work_ids.len());
    let sql = format!(
        "SELECT e.work_id, e.id, e.key, e.label, COUNT(m.id), COALESCE(SUM(m.pages), 0)
           FROM edition e
           LEFT JOIN member m ON m.edition_id = e.id{joined}
          WHERE 1 = 1{clause} AND e.work_id IN ({wanted})
          GROUP BY e.id
          ORDER BY e.work_id, e.key"
    );
    let mut params: Vec<&dyn rusqlite::ToSql> =
        roots.iter().map(|r| r as &dyn rusqlite::ToSql).collect();
    params.extend(roots.iter().map(|r| r as &dyn rusqlite::ToSql));
    params.extend(work_ids.iter().map(|w| w as &dyn rusqlite::ToSql));

    let mut stmt =
        prepare(conn, &sql).map_err(|e| format!("preparing the edition listing: {e}"))?;
    let rows = stmt
        .query_map(params.as_slice(), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                EditionCard {
                    key: row.get(2)?,
                    label: row.get(3)?,
                    members: row.get(4)?,
                    pages: row.get(5)?,
                    missing: 0,
                },
            ))
        })
        .map_err(|e| format!("listing editions: {e}"))?
        .collect::<rusqlite::Result<Vec<(i64, i64, EditionCard)>>>()
        .map_err(|e| format!("reading editions: {e}"))?;

    let edition_ids: Vec<i64> = rows.iter().map(|(_, id, _)| *id).collect();
    let missing = missing_by_edition(conn, roots, &edition_ids)?;

    let mut by_work: HashMap<i64, Vec<EditionCard>> = HashMap::new();
    for (work_id, edition_id, card) in rows {
        by_work.entry(work_id).or_default().push(EditionCard {
            missing: missing.get(&edition_id).copied().unwrap_or(0),
            ..card
        });
    }
    Ok(by_work)
}

/// How many of each edition's members no longer exist on disk.
///
/// **O62.** The scanner deletes a `scan_entry` for a file it no longer meets
/// (`scan::forget_unseen`) while the `member` row survives, so the difference
/// between the two is the fact — no new column is needed, and C1 §5.5's
/// framing of this as "what the store should record when a file vanishes" is
/// answered *nothing further*.
///
/// **Joined on `(root_id, rel_path)`, `scan_entry`'s own `UNIQUE` index**,
/// with the computed string on the *probing* side rather than the probed
/// side — the same shape `proposals::edition_formats` already reads an
/// edition's formats back out of `scan_entry` with, for a different reason.
/// `ltrim` rather than `length(r.path) + 2`: a root stored with a trailing
/// slash makes the fixed-offset arithmetic off by one, and `ltrim` is exact
/// either way, matching how `derive` builds `member.path` as
/// `root.path.join(rel_path)`.
///
/// A member with no root — an imported work's, by design, or one whose root
/// was since removed (`ON DELETE SET NULL`) — never matches the `JOIN root`
/// at all and so is excluded from the count entirely, neither present nor
/// missing: nothing was ever walked for it, so there is no scan fact to read,
/// the same reasoning `edition_formats` states for reporting no format.
///
/// **Task 7b: takes `roots` like every other function in this module.**
/// Shipped without it, which is a grant leak `member_clause`'s own doc calls
/// out as reachable in the default configuration: two roots added the
/// ordinary way merge into one edition, and a caller granted only one of them
/// was told a `missing` count covering material under the root they cannot
/// see — a fact about material the grant hides. [`member_clause`] is the same
/// clause [`editions_by_work`]'s member join already reads for the same reason.
///
/// **O85: a set, not one edition.** This took a single `edition_id` and was
/// called once per edition of every work in a listing. **An edition with
/// nothing missing has no row here at all** — `COUNT(*)` over an empty group
/// produces none — so a caller reads its absence as zero rather than expecting
/// a key per edition, which is what the single-edition version returned.
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub fn missing_by_edition(
    conn: &Connection,
    roots: &[i64],
    edition_ids: &[i64],
) -> Result<HashMap<i64, usize>, String> {
    if edition_ids.is_empty() {
        return Ok(HashMap::new());
    }
    // Parameter order is text order (the module header, and `alignments_of`'s
    // doc): the `IN` list's `?`s precede `{members}`'s anonymous `?`s in the
    // SQL text, matching the order `params` below pushes them in.
    let members = member_clause(roots);
    let wanted = holes(edition_ids.len());
    let sql = format!(
        "SELECT m.edition_id, COUNT(*)
           FROM member m
           JOIN root r ON r.id = m.root_id
           LEFT JOIN scan_entry se
                  ON se.root_id  = m.root_id
                 AND se.rel_path = ltrim(substr(m.path, length(r.path) + 1), '/')
          WHERE m.edition_id IN ({wanted}) AND se.rel_path IS NULL{members}
          GROUP BY m.edition_id"
    );
    let mut params: Vec<i64> = edition_ids.to_vec();
    params.extend(roots.iter().copied());

    let mut stmt =
        prepare(conn, &sql).map_err(|e| format!("preparing the missing-member count: {e}"))?;
    let rows = stmt
        .query_map(params_from_iter(params), |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|e| format!("counting missing members: {e}"))?
        .collect::<rusqlite::Result<Vec<(i64, i64)>>>()
        .map_err(|e| format!("reading missing members: {e}"))?;

    Ok(rows
        .into_iter()
        .map(|(id, n)| (id, usize::try_from(n).unwrap_or(0)))
        .collect())
}

/// One work's alignments, with both sides restricted to the caller's roots.
///
/// [`pair_clause`] comes **before** `a.work_id = ?` so its parameters bind
/// first, which is this module's one parameter rule — text order. Written the
/// other way round the work id takes hole 1 and every root shifts.
///
/// That clause is shared with [`aligned_clause`] rather than written twice:
/// this list and the `aligned` flag beside it are the same question, and they
/// answered it differently for as long as they were two pieces of SQL.
///
/// **`None` for `only`, because no caller of this narrows.** `?root=` reaches
/// `series` and `unfiled`; a work page addresses one work by slug and has no
/// root to be scoped to. If one is ever given one, the narrowing belongs here
/// and in [`works_of`] alongside it, not in a second clause.
fn alignments_of(
    conn: &Connection,
    roots: &[i64],
    work_id: i64,
) -> Result<Vec<AlignmentCard>, String> {
    let pair = pair_clause(roots, None);
    let sql = format!(
        "SELECT a.from_key, a.to_key, a.verdict, a.slots, a.paired
           FROM alignment a
          WHERE {pair}
            AND a.work_id = ?
          ORDER BY a.from_key, a.to_key"
    );
    let mut params: Vec<i64> = roots.to_vec();
    params.extend(roots.iter().copied());
    params.push(work_id);

    let mut stmt =
        prepare(conn, &sql).map_err(|e| format!("preparing the alignment listing: {e}"))?;
    let rows = stmt
        .query_map(params_from_iter(params), |row| {
            Ok(AlignmentCard {
                from: row.get(0)?,
                to: row.get(1)?,
                verdict: row.get(2)?,
                slots: row.get(3)?,
                paired: row.get(4)?,
            })
        })
        .map_err(|e| format!("listing alignments of work {work_id}: {e}"))?;
    rows.collect::<rusqlite::Result<Vec<AlignmentCard>>>()
        .map_err(|e| format!("reading alignments of work {work_id}: {e}"))
}

/// The works matching `predicate`, with only the editions this caller may see.
///
/// **Two queries, not one join.** A work joined to its editions returns one row
/// per edition, and rebuilding the nesting from a flat result is where counts
/// go wrong. The work list is small — spec C1 §7 measures 111 volumes in the
/// largest series — so a second query per work is cheaper than the bug.
///
/// `predicate` and `order` are SQL fragments over `w`, never caller text: every
/// call site below passes a literal. Their parameters bind **after** the roots,
/// in text order.
///
/// **`None` for [`aligned_clause`]'s `only`.** This takes no narrowing of its
/// own: [`unfiled`] carries `?root=` in its `predicate` instead, because the
/// predicate is the only fragment between the roots and the end of the
/// statement. That is sound rather than lucky — `unfiled` selects
/// `w.series_id IS NULL`, derivation gives every scanned work a series, so the
/// works it returns are imported ones whose members carry `root_id` NULL, and
/// `om.root_id = ?` is false for NULL: with a narrowing in force the list is
/// empty and there is no `aligned` for it to have got wrong. Any future caller
/// that does narrow a *filed* work needs `only` threaded here, exactly as
/// `series` threads it.
fn works_of(
    conn: &Connection,
    roots: &[i64],
    user_id: i64,
    predicate: &str,
    extra: &[&dyn rusqlite::ToSql],
    order: &str,
) -> Result<Vec<WorkCard>, String> {
    let aligned = aligned_clause(roots, None);
    let clause = root_clause(roots);
    // Placeholders bind in text order, all anonymous: `aligned`'s two root
    // groups in the `SELECT` list, then the edition join's roots, then
    // `extra`. No `only` group interleaves here — see this function's doc.
    // See the module header.
    let sql = format!(
        "SELECT DISTINCT w.id, w.slug, w.title, w.volume, w.hub, {aligned}
           FROM work w
           JOIN edition e ON e.work_id = w.id{clause}
          WHERE {predicate}
          ORDER BY {order}"
    );
    let mut params: Vec<&dyn rusqlite::ToSql> =
        roots.iter().map(|r| r as &dyn rusqlite::ToSql).collect();
    params.extend(roots.iter().map(|r| r as &dyn rusqlite::ToSql));
    params.extend(roots.iter().map(|r| r as &dyn rusqlite::ToSql));
    params.extend_from_slice(extra);

    let mut stmt = prepare(conn, &sql).map_err(|e| format!("preparing the work listing: {e}"))?;
    let rows: Vec<(i64, String, String, Option<f64>, String, bool)> = stmt
        .query_map(params.as_slice(), |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        })
        .map_err(|e| format!("listing works: {e}"))?
        .collect::<rusqlite::Result<_>>()
        .map_err(|e| format!("reading works: {e}"))?;

    // **O85: the editions for the whole set, not one work at a time.** Two
    // statements for any number of works, where this was one per work and one
    // more per edition of each.
    let ids: Vec<i64> = rows.iter().map(|(id, ..)| *id).collect();
    let mut editions = editions_by_work(conn, roots, &ids)?;
    let positions = opens_by_work(conn, user_id, &ids)?;

    Ok(rows
        .into_iter()
        .map(|(id, slug, title, volume, hub, aligned)| {
            let editions = editions.remove(&id).unwrap_or_default();
            let pages = editions.iter().map(|e| e.pages).max().unwrap_or(0);
            // **O84.** `restore`'s rule, server-side: the stored position's
            // edition when the grant still shows it, the hub otherwise.
            let named = positions
                .get(&id)
                .filter(|edition| editions.iter().any(|e| &&e.key == edition))
                .map_or(hub, Clone::clone);
            // **And `routes::filter_sidecar_editions`' rule after it, which a
            // hidden hub makes load-bearing.** That function substitutes a hub
            // the grant hides with the first surviving edition in key order,
            // so the reader never opens the hidden one — and `opens` naming it
            // anyway would point this guard at an edition that is not on the
            // screen and never opens, which reads as "not unreadable" whatever
            // the real one has lost. `editions_by_work` returns each work's
            // editions in key order, the same order that function's
            // `BTreeMap::keys().next()` takes.
            let opens = if editions.iter().any(|e| e.key == named) {
                named
            } else {
                editions.first().map_or(named, |e| e.key.clone())
            };
            WorkCard {
                id,
                slug,
                title,
                volume,
                editions,
                aligned,
                pages,
                opens,
            }
        })
        .collect())
}

/// The edition each of `work_ids` holds this reader's **newest** position in.
///
/// **One statement for the whole set**, like [`editions_by_work`] beside it and
/// for O85's reason. A work with no position at all has no key here, and the
/// caller reads that absence as "the hub".
///
/// **Newest, and ties broken by edition key.** `api.ts::getProgress` answers
/// every edition's position newest-first and the reader resumes into the head,
/// so the newest is the one that opens. `ORDER BY … updated DESC, edition`
/// makes the head deterministic where two positions share a timestamp —
/// `progress` has second granularity and two editions touched in the same
/// second is reachable, at which point "whichever SQLite returned first" is a
/// listing that disagrees with itself between calls.
///
/// Placeholders bind in text order: `user_id`, then `work_ids`.
fn opens_by_work(
    conn: &Connection,
    user_id: i64,
    work_ids: &[i64],
) -> Result<HashMap<i64, String>, String> {
    if work_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let wanted = holes(work_ids.len());
    let sql = format!(
        "SELECT p.work_id, p.edition
           FROM progress p
          WHERE p.user_id = ? AND p.work_id IN ({wanted})
          ORDER BY p.work_id, p.updated DESC, p.edition"
    );
    let mut params: Vec<i64> = vec![user_id];
    params.extend(work_ids.iter().copied());

    let mut stmt =
        prepare(conn, &sql).map_err(|e| format!("preparing the position listing: {e}"))?;
    let rows = stmt
        .query_map(params_from_iter(params), |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("listing positions: {e}"))?
        .collect::<rusqlite::Result<Vec<(i64, String)>>>()
        .map_err(|e| format!("reading positions: {e}"))?;

    // The first row per work is its newest, the `ORDER BY` having said so.
    let mut out: HashMap<i64, String> = HashMap::new();
    for (work_id, edition) in rows {
        out.entry(work_id).or_insert(edition);
    }
    Ok(out)
}

/// One series and its works, or `None` if this caller cannot see it.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn one_series(
    conn: &Connection,
    roots: &[i64],
    user_id: i64,
    id: i64,
) -> Result<Option<(SeriesCard, Vec<WorkCard>)>, String> {
    let Some(card) = series(conn, roots, user_id, None)?
        .into_iter()
        .find(|c| c.id == id)
    else {
        return Ok(None);
    };
    let works = works_of(
        conn,
        roots,
        user_id,
        "w.series_id = ?",
        &[&id],
        "w.volume, w.id",
    )?;
    Ok(Some((card, works)))
}

/// One work by slug, or `None` if this caller cannot see it.
///
/// **`None` rather than a refusal for a work the grant hides**, which is the
/// same answer as one that does not exist — deliberately. Distinguishing them
/// tells an unauthorised caller which slugs are real.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn work(
    conn: &Connection,
    roots: &[i64],
    user_id: i64,
    slug: &str,
) -> Result<Option<WorkDetail>, String> {
    let Some(card) = works_of(conn, roots, user_id, "w.slug = ?", &[&slug], "w.id")?
        .into_iter()
        .next()
    else {
        return Ok(None);
    };
    let alignments = alignments_of(conn, roots, card.id)?;
    Ok(Some(WorkDetail { card, alignments }))
}

/// The works belonging to no series, which no series-joined query can reach.
///
/// **Spec C1 §5.4**, and measured rather than assumed: `import.rs` never writes
/// `series_id` — `taiyaku.json` has no field for it — so this is every
/// hand-imported work in the library, and before this it was invisible on the
/// one screen a reader opens on.
///
/// **A restricted account sees none of them**, and that needs no branch: an
/// imported member's `root_id` is NULL, and `IN (…)` excludes NULL. That is
/// §5.1's rule applying unchanged, not a second one.
///
/// **`only` empties this list, by that same rule rather than a new one.** The
/// grid fetches series and unfiled works together, and until 2026-08-21 it
/// passed `?root=` to the first and not the second — so
/// `/api/library/series?root=5` answered a root-scoped grid beside a
/// root-agnostic list of every imported work in the library. A member under no
/// root belongs to no root, and asking for a root is asking for what is under
/// it; [`only_clause`]'s `om.root_id = ?` is false for NULL, so the SQL says
/// that without the handler having to.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn unfiled(
    conn: &Connection,
    roots: &[i64],
    user_id: i64,
    only: Option<i64>,
) -> Result<Vec<WorkCard>, String> {
    // The narrowing rides in the predicate rather than in a parameter of its
    // own: `works_of` binds `extra` in text order after its roots, and the
    // predicate is the only fragment that sits between them and the end of
    // the statement.
    let narrowed = only_clause(only);
    let mut extra: Vec<&dyn rusqlite::ToSql> = Vec::new();
    if let Some(id) = only.as_ref() {
        extra.push(id);
    }
    works_of(
        conn,
        roots,
        user_id,
        &format!("w.series_id IS NULL{narrowed}"),
        &extra,
        "w.title COLLATE NOCASE, w.id",
    )
}

/// The shortest query FTS5's `trigram` tokeniser can answer.
///
/// Measured 2026-08-21 against SQLite 3.53.4: a trigram index over
/// `五等分の花嫁` answers `五` and `五等` with nothing, and `五等分` with the
/// row — the `*` prefix operator does not change that, because trigram has no
/// prefix to operate on. Below this length the index is not slow, it is
/// silent, which is a different failure and needs a different answer.
const TRIGRAM_MIN: usize = 3;

/// How one query is going to be asked, chosen by its length alone.
///
/// **Two paths rather than one widened one.** `LIKE '%q%'` would answer every
/// query correctly at this library's size — §7's measured 127 series — and was
/// considered; the index is kept because it is the part that still holds at
/// ten times that, and the scan is kept because it is the only thing that
/// answers the first two keystrokes at all.
enum Match {
    /// An FTS5 string literal, quotes doubled, for `MATCH`.
    Fts(String),
    /// A `LIKE` needle with `%`, `_` and `\` escaped, for `LIKE … ESCAPE '\'`.
    Like(String),
}

/// Which path a query takes, by `chars().count()` rather than `len()`.
///
/// **Bytes would be wrong in exactly the case this exists for**: `花嫁` is two
/// characters and six UTF-8 bytes, so a byte length would send it to trigram,
/// which is the silence this function is avoiding. Every Japanese query short
/// enough to need the fallback is long enough in bytes to miss it.
fn match_for(trimmed: &str) -> Match {
    if trimmed.chars().count() < TRIGRAM_MIN {
        Match::Like(escape_like(trimmed))
    } else {
        Match::Fts(format!("\"{}\"", trimmed.replace('"', "\"\"")))
    }
}

/// A `LIKE` needle in which the reader's own `%`, `_` and `\` are literals.
///
/// Measured 2026-08-21: without this, `title LIKE '%' || '%' || '%'` matches
/// every row — a one-character search that dumps the library. `ESCAPE '\'` is
/// what makes the backslash meaningful, and so what makes a typed backslash
/// need one of its own.
fn escape_like(q: &str) -> String {
    let mut out = String::with_capacity(q.len());
    for ch in q.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Search titles through FTS5 — **over both indexes, and past it when the
/// query is too short for one.**
///
/// **A query of three characters or more is quoted as an FTS5 string
/// literal**, doubling any `"`, so a reader typing `AND`, `*` or `-` gets a
/// search for those characters rather than a syntax error or an operator they
/// did not ask for. **A shorter one is a `LIKE` scan**, because migration 8's
/// `trigram` tokeniser has no term shorter than three characters to match and
/// answers such a query with silence rather than slowness — and `花嫁`, two
/// characters, is an ordinary Japanese word and the second half of a title
/// this library holds. See [`TRIGRAM_MIN`] and [`escape_like`].
///
/// `work_fts` has been populated by triggers since migration 1 with nothing
/// reading it. Searching series alone leaves an imported work unfindable by its
/// own title, which is spec C1 §5.4.
///
/// # Errors
///
/// Returns an error if a query fails.
pub fn search(
    conn: &Connection,
    roots: &[i64],
    user_id: i64,
    q: &str,
) -> Result<SearchResults, String> {
    let trimmed = q.trim();
    if trimmed.is_empty() {
        return Ok(SearchResults {
            series: Vec::new(),
            unfiled: Vec::new(),
        });
    }
    let matcher = match_for(trimmed);

    let mut matched = matching_series(conn, &matcher, trimmed)?;
    matched.extend(series_of_matching_works(conn, &matcher, trimmed)?);

    // Filtered through `series` and `unfiled` rather than joined into the FTS
    // queries, so a hit this caller may not see is dropped by exactly the same
    // code that decides every other screen — one filter, one place to read.
    let cap = usize::try_from(LIST_CAP).unwrap_or(usize::MAX);
    let series_hits = series(conn, roots, user_id, None)?
        .into_iter()
        .filter(|c| matched.contains(&c.id))
        .take(cap)
        .collect();

    let unfiled_ids = matching_unfiled_works(conn, &matcher, trimmed)?;
    let unfiled_hits = unfiled(conn, roots, user_id, None)?
        .into_iter()
        .filter(|w| unfiled_ids.contains(&w.id))
        .take(cap)
        .collect();

    Ok(SearchResults {
        series: series_hits,
        unfiled: unfiled_hits,
    })
}

/// Series ids whose own titles match.
fn matching_series(conn: &Connection, m: &Match, shown: &str) -> Result<Vec<i64>, String> {
    let (sql, needle) = match m {
        Match::Fts(q) => (
            "SELECT DISTINCT st.series_id FROM series_fts f
               JOIN series_title st ON st.rowid = f.rowid
              WHERE series_fts MATCH ?",
            q,
        ),
        Match::Like(q) => (
            "SELECT DISTINCT st.series_id FROM series_title st
              WHERE st.title LIKE '%' || ? || '%' ESCAPE '\\'",
            q,
        ),
    };
    ids(
        conn,
        sql,
        needle,
        &format!("searching series for `{shown}`"),
    )
}

/// Series ids dragged in by a *work* whose title matches — searching
/// "Hanayome" should find the shelf, not only the volume that spells it.
fn series_of_matching_works(conn: &Connection, m: &Match, shown: &str) -> Result<Vec<i64>, String> {
    let (sql, needle) = match m {
        Match::Fts(q) => (
            "SELECT DISTINCT w.series_id FROM work_fts f
               JOIN work w ON w.id = f.rowid
              WHERE work_fts MATCH ? AND w.series_id IS NOT NULL",
            q,
        ),
        Match::Like(q) => (
            "SELECT DISTINCT w.series_id FROM work w
              WHERE w.title LIKE '%' || ? || '%' ESCAPE '\\'
                AND w.series_id IS NOT NULL",
            q,
        ),
    };
    ids(conn, sql, needle, &format!("searching works for `{shown}`"))
}

/// Work ids matching, among those belonging to no series.
fn matching_unfiled_works(conn: &Connection, m: &Match, shown: &str) -> Result<Vec<i64>, String> {
    let (sql, needle) = match m {
        Match::Fts(q) => (
            "SELECT f.rowid FROM work_fts f
               JOIN work w ON w.id = f.rowid
              WHERE work_fts MATCH ? AND w.series_id IS NULL",
            q,
        ),
        Match::Like(q) => (
            "SELECT w.id FROM work w
              WHERE w.title LIKE '%' || ? || '%' ESCAPE '\\'
                AND w.series_id IS NULL",
            q,
        ),
    };
    ids(
        conn,
        sql,
        needle,
        &format!("searching unfiled works for `{shown}`"),
    )
}

/// One column of ids from a title query, which the three above differ only in.
///
/// **Unbounded, deliberately, and the cap moved to the caller.** These used to
/// `LIMIT ?2` and were only then intersected with what the grant allows — so
/// if the twenty rows SQLite happened to return were all outside the caller's
/// roots, a search that genuinely matched visible series reported nothing
/// found. That is the "filter after the query" failure the module header
/// argues against, committed by the one function that argued it. Found by the
/// whole-branch review.
///
/// Unbounded is affordable because these return ids from a query over title
/// columns, and [`search`] caps the filtered result instead. It stays
/// affordable on the `LIKE` path for the same reason §7 declines
/// virtualisation: 127 series and their titles is a scan measured in
/// microseconds, and it is only ever reached by a query of one or two
/// characters.
fn ids(conn: &Connection, sql: &str, needle: &str, doing: &str) -> Result<Vec<i64>, String> {
    let mut stmt = prepare(conn, sql).map_err(|e| format!("preparing: {doing}: {e}"))?;
    let rows = stmt
        .query_map(rusqlite::params![needle], |row| row.get(0))
        .map_err(|e| format!("{doing}: {e}"))?;
    rows.collect::<rusqlite::Result<Vec<i64>>>()
        .map_err(|e| format!("{doing}: {e}"))
}

/// The three lists the library screen opens on, for one reader.
///
/// `continuing` is progress that is not finished, `read` is progress that is
/// **and no sibling edition is still open** (see below), and `added` is the
/// newest works — each capped at [`LIST_CAP`], each filtered by the same
/// grant.
///
/// **Both orderings are `MAX(…)` over `progress`, not a scalar subquery.**
/// Since migration 009 a reader holds a position per *edition* of a work, so
/// `(SELECT updated FROM progress p WHERE p.work_id = w.id AND p.user_id = ?)`
/// is a subquery over what can now be several rows — and a scalar subquery
/// over several rows returns whichever one SQLite reaches first, with no
/// error and no warning, so the ordering was silently arbitrary among a
/// work's editions. `MAX(updated)` / `MAX(completed_at)` answer "this work's
/// most recent position", across however many editions it has one in.
///
/// **`read`'s predicate also excludes a work still open in any edition.** The
/// old `(user_id, work_id)` key could never be both "finished" and "still
/// reading" at once; the per-edition key can, when one edition is finished and
/// a sibling is not. Showing the same volume under both Continuing and Read on
/// one screen would state a contradiction to the reader, so a work counts as
/// read only once no edition is still open — a judgement call rather than a
/// measurement, made here rather than by showing the volume in both.
///
/// **`continuing` binds `user_id` twice — once for the predicate, once for the
/// ordering subquery — and `read` binds it three times**, the third being the
/// `NOT IN` clause above. `works_of` binds `extra` in text order after the
/// roots, so a miscount here is a hard refusal from `rusqlite` rather than a
/// wrong answer.
///
/// # Errors
///
/// Returns an error if a query fails.
pub fn home(conn: &Connection, roots: &[i64], user_id: i64) -> Result<Home, String> {
    let continuing = works_of(
        conn,
        roots,
        user_id,
        "w.id IN (SELECT work_id FROM progress
                   WHERE user_id = ? AND completed_at IS NULL)",
        &[&user_id, &user_id],
        "(SELECT MAX(updated) FROM progress p WHERE p.work_id = w.id AND p.user_id = ?) DESC",
    )?;
    let read = works_of(
        conn,
        roots,
        user_id,
        "w.id IN (SELECT work_id FROM progress
                   WHERE user_id = ? AND completed_at IS NOT NULL)
         AND w.id NOT IN (SELECT work_id FROM progress
                           WHERE user_id = ? AND completed_at IS NULL)",
        &[&user_id, &user_id, &user_id],
        "(SELECT MAX(completed_at) FROM progress p WHERE p.work_id = w.id AND p.user_id = ?) DESC",
    )?;
    // **`LIMIT` in the SQL, not `.take()` afterwards.** When this was written
    // `works_of` fired an edition query per row it returned, so selecting every
    // work in the library and discarding all but twenty cost one query per work
    // — 69 per `/api/library/home` on the measured corpus, and thousands on a
    // real library. Found by the whole-branch review. **O85 has since made that
    // one statement for the whole set, which removes the per-row cost but not
    // this bound's reason**: it keeps the rows read proportional to what the
    // route returns rather than to the size of the library.
    let added = works_of(conn, roots, user_id, "1 = 1", &[], "w.id DESC LIMIT 20")?;

    let cap = usize::try_from(LIST_CAP).unwrap_or(usize::MAX);
    Ok(Home {
        continuing: continuing.into_iter().take(cap).collect(),
        read: read.into_iter().take(cap).collect(),
        added: added.into_iter().take(cap).collect(),
    })
}

/// The work whose hub cover stands for this series.
///
/// **A fallback, not a lookup.** `series.cover_work_id` exists in migration 1
/// and nothing in this repository writes it — verified 2026-08-20, and still
/// true now that spec C3's metadata editing has shipped — so it is NULL for
/// every series a scan has produced, which is all of them. C1 writes it at no
/// point, and C3's `PATCH /api/library/series/{id}` has no field for it either
/// — `SeriesEdit` covers titles, reading, tags and external ids, not a cover
/// choice. Choosing a cover is still where this belongs; it remains undone.
///
/// The fallback is the lowest-`volume` work this caller may see, `w.id` as the
/// tiebreak so a series of volume-less works answers the same row every time
/// rather than whichever the b-tree returned.
///
/// A hand-set cover still has to survive the filter: a reader granted one root
/// must not be handed a cover generated from a root they cannot see.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn cover_work(
    conn: &Connection,
    roots: &[i64],
    series_id: i64,
) -> Result<Option<CoverTarget>, String> {
    // **`.optional()`, because a series id that does not exist is not an
    // error.** Without it `query_row` returns `QueryReturnedNoRows`, the
    // handler turned that into a 500, and `/series/9999/cover` answering 500
    // while `/series/9999` answers 404 is an enumeration oracle — exactly the
    // distinction `one_series` goes out of its way not to expose. Found by the
    // whole-branch review.
    let set: Option<i64> = query_row(
        conn,
        "SELECT cover_work_id FROM series WHERE id = ?",
        rusqlite::params![series_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(|e| format!("reading the cover of series {series_id}: {e}"))?
    .flatten();

    // A hand-set cover must still survive the filter — a reader granted one
    // root must not be handed a cover generated from a root they cannot see —
    // so it is probed through the same visibility query rather than trusted.
    if let Some(chosen) = set
        && let Some(target) = cover_target(conn, roots, series_id, Some(chosen))?
    {
        return Ok(Some(target));
    }
    cover_target(conn, roots, series_id, None)
}

/// Which work and which of its editions a cover comes from.
///
/// **The edition matters and an earlier version ignored it**, redirecting to
/// `work.hub`: grant `Manga` (key `en`) and not `Manga (RAW)` (key `raw`, the
/// hub) and the cover served page 0 of the edition the grant hides — with
/// `cover_work`'s own doc comment promising the opposite. Whole-branch review.
#[derive(Debug, Clone, PartialEq)]
pub struct CoverTarget {
    pub work_id: i64,
    pub slug: String,
    /// The first edition of that work this caller may see, never `work.hub`
    /// unless the two coincide.
    pub edition: String,
}

/// One query for the cover, rather than building every `WorkCard` in the
/// series to read one id off the front.
///
/// When this was written `works_of` fired an edition query per work, and the
/// grid asks for one cover per tile — so a 127-series library rendered
/// thousands of queries to answer 127 questions. Found by the whole-branch
/// review. O85 has since bounded the listing itself; asking for a cover with
/// one query rather than by building every `WorkCard` is what this function is
/// still for.
///
/// `wanted` pins a specific work (the hand-set cover); `None` takes the
/// lowest-volume visible one, with `w.id` as the tiebreak so a series of
/// volume-less works answers the same row every time.
fn cover_target(
    conn: &Connection,
    roots: &[i64],
    series_id: i64,
    wanted: Option<i64>,
) -> Result<Option<CoverTarget>, String> {
    let joined = member_clause(roots);
    let pin = if wanted.is_some() {
        " AND w.id = ?"
    } else {
        ""
    };
    let sql = format!(
        "SELECT w.id, w.slug, e.key
           FROM work w
           JOIN edition e ON e.work_id = w.id
          WHERE w.series_id = ?
            AND EXISTS (SELECT 1 FROM member m
                         WHERE m.edition_id = e.id{joined}){pin}
          ORDER BY w.volume, w.id, e.key
          LIMIT 1"
    );
    let mut params: Vec<&dyn rusqlite::ToSql> = vec![&series_id];
    params.extend(roots.iter().map(|r| r as &dyn rusqlite::ToSql));
    if let Some(id) = wanted.as_ref() {
        params.push(id);
    }
    query_row(conn, &sql, params.as_slice(), |row| {
        Ok(CoverTarget {
            work_id: row.get(0)?,
            slug: row.get(1)?,
            edition: row.get(2)?,
        })
    })
    .optional()
    .map_err(|e| format!("reading the cover of series {series_id}: {e}"))
}

/// Whether `slug`'s `edition` holds a member this caller may see.
///
/// **The page routes' grant check**, and the reason the whole-branch review
/// called the filter partly cosmetic: `/api/works/{slug}/{edition}/pages/N`
/// applied no grant at all, so a restricted reader who knew a slug could read
/// every page of every root. Browse hid the work and the bytes were served
/// anyway.
///
/// Answers `true` for an empty grant, like every other query here.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn edition_visible(
    conn: &Connection,
    roots: &[i64],
    slug: &str,
    edition: &str,
) -> Result<bool, String> {
    if roots.is_empty() {
        return Ok(true);
    }
    let joined = member_clause(roots);
    let sql = format!(
        "SELECT 1 FROM work w
           JOIN edition e ON e.work_id = w.id
          WHERE w.slug = ? AND e.key = ?
            AND EXISTS (SELECT 1 FROM member m
                         WHERE m.edition_id = e.id{joined})
          LIMIT 1"
    );
    let mut params: Vec<&dyn rusqlite::ToSql> = vec![&slug, &edition];
    params.extend(roots.iter().map(|r| r as &dyn rusqlite::ToSql));
    let found: Option<i64> = query_row(conn, &sql, params.as_slice(), |row| row.get(0))
        .optional()
        .map_err(|e| format!("checking visibility of `{slug}`/`{edition}`: {e}"))?;
    Ok(found.is_some())
}

/// Every edition key this caller may see, for every work, keyed by slug.
///
/// **What `taiyaku-serve`'s `/api/works` list and OPDS `works` feed both need
/// instead of a loop.** `browse::work` already answers "only the editions
/// this caller may see" for one work (`WorkCard::editions`'s own contract),
/// but calling it once per work to build a whole-library list would be the
/// same shape of N+1 the whole-branch review closed in `home` at 111 queries
/// on the largest series — this answers the whole library in one round trip,
/// and the caller does one `HashSet` lookup per edition after that. [`work`]
/// stays the right tool for a caller that already holds one slug —
/// `get_sidecar` and `patch_alignments` need nothing else.
///
/// This is [`root_clause`] alone — no `?root=`, no alignment, no
/// `member_clause` — because the question is only "does this edition keep a
/// member under one of these roots", which is the same question
/// [`edition_visible`] answers one edition at a time. A work with zero
/// visible editions has no key in the returned map at all, rather than an
/// entry mapping to an empty set — a caller that iterates the map's keys
/// never sees such a work named.
///
/// Answers every edition of every work for an empty grant, like every other
/// query here — though a caller checking `roots.is_empty()` first, the way
/// `routes::refuse_if_hidden` does, never reaches this call at all.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn visible_editions(
    conn: &Connection,
    roots: &[i64],
) -> Result<HashMap<String, HashSet<String>>, String> {
    let clause = root_clause(roots);
    let sql = format!(
        "SELECT DISTINCT w.slug, e.key
           FROM work w
           JOIN edition e ON e.work_id = w.id{clause}"
    );
    let params: Vec<&dyn rusqlite::ToSql> =
        roots.iter().map(|r| r as &dyn rusqlite::ToSql).collect();
    let mut stmt =
        prepare(conn, &sql).map_err(|e| format!("preparing the visible edition listing: {e}"))?;
    let rows = stmt
        .query_map(params.as_slice(), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("listing visible editions: {e}"))?;

    let mut out: HashMap<String, HashSet<String>> = HashMap::new();
    for pair in rows {
        let (slug, key) = pair.map_err(|e| format!("reading visible editions: {e}"))?;
        out.entry(slug).or_default().insert(key);
    }
    Ok(out)
}

/// One root, as §2's provisional `ROOTS` section lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct RootCard {
    pub id: i64,
    pub name: String,
}

/// What a first-run screen needs to know, and the only thing that can tell it.
///
/// **Spec C1 §5.5.** `/api/library/series` answers `{ series: [], unfiled: [] }`
/// for "no roots configured" and for "roots configured, scan not yet run"
/// alike, and §8 wants those to be different screens: the difference between
/// "you have no library" and "your library has not been read yet".
///
/// Read-only, with no writer and no screen of its own. C4 still builds roots
/// administration.
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub roots: Vec<RootCard>,
    pub scanned: bool,
}

/// The last component of a path, or the whole thing if it has none.
fn last_component(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned())
}

/// Label each root, falling back to the whole path where two would collide.
///
/// A nav item reading `Manga` twice is worse than one reading
/// `/media/a/Manga`, and a non-admin reader has no business being handed the
/// server's filesystem layout to render a sidebar that does not collide.
fn named(rows: Vec<(i64, String)>) -> Vec<RootCard> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for (_, path) in &rows {
        *counts.entry(last_component(path)).or_default() += 1;
    }
    rows.into_iter()
        .map(|(id, path)| {
            let short = last_component(&path);
            let name = if counts.get(&short).copied().unwrap_or(0) > 1 {
                path
            } else {
                short
            };
            RootCard { id, name }
        })
        .collect()
}

/// The roots this caller may see, and whether anything has been scanned.
///
/// # Errors
///
/// Returns an error if either query fails.
pub fn status(conn: &Connection, roots: &[i64]) -> Result<Status, String> {
    let clause = if roots.is_empty() {
        String::new()
    } else {
        format!(" WHERE id IN ({})", holes(roots.len()))
    };
    let sql = format!("SELECT id, path FROM root{clause} ORDER BY path");
    let mut stmt = prepare(conn, &sql).map_err(|e| format!("preparing the root listing: {e}"))?;
    let rows = stmt
        .query_map(params_from_iter(roots), |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("listing roots: {e}"))?
        .collect::<rusqlite::Result<Vec<(i64, String)>>>()
        .map_err(|e| format!("reading roots: {e}"))?;

    let scanned: bool = query_row(conn, "SELECT EXISTS (SELECT 1 FROM dir_state)", [], |row| {
        row.get(0)
    })
    .map_err(|e| format!("reading whether a scan has run: {e}"))?;

    Ok(Status {
        roots: named(rows),
        scanned,
    })
}

#[cfg(test)]
mod tests;
