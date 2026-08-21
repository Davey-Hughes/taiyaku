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
//! Filtering after the query was the alternative and is wrong twice over: it
//! reads rows the caller may not see into memory before deciding, and it makes
//! every count (`works`, `editions`, `aligned`) a number computed over the
//! unfiltered set.
//!
//! # Every placeholder here is anonymous `?`, never `?1`
//!
//! SQLite numbers an anonymous parameter *one higher than the largest index
//! assigned so far*, so a single `?3` earlier in a statement pushes the
//! following `?`s to 4 and 5 — and a query built by concatenating a caller's
//! fragment onto a generated `IN (?,?)` cannot know what those indices are.
//! Measured on 2026-08-20 against the real schema: `WHERE e.work_id = ?3 AND …
//! IN (?,?)` counts **five** parameters, not three, and `rusqlite` refuses the
//! call. Anonymous-only means the binding order is the text order, which is
//! the one rule every function below follows: **roots first, then the caller's
//! own parameters.**

use rusqlite::{Connection, OptionalExtension as _, params_from_iter};

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
    /// Works with at least one alignment — spec C1 §3's `⇄ 3` mark. A series
    /// page that lists volumes without saying which are aligned is a Komga
    /// screen with extra badges.
    pub aligned: i64,
    pub cover_work: Option<i64>,
}

/// One edition of one work, as a series page or a work page shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct EditionCard {
    pub key: String,
    pub label: String,
    pub members: i64,
    pub pages: i64,
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
    pub aligned: bool,
    pub pages: i64,
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
    if roots.is_empty() {
        return String::new();
    }
    let holes = std::iter::repeat_n("?", roots.len())
        .collect::<Vec<_>>()
        .join(",");
    format!(
        " AND EXISTS (SELECT 1 FROM member rm WHERE rm.edition_id = e.id \
         AND rm.root_id IN ({holes}))"
    )
}

/// The `AND` restricting a joined `member m` to the caller's roots.
///
/// **Distinct from [`root_clause`], and the whole-branch review found out
/// why.** `root_clause` decides whether an *edition* survives; this decides
/// which of its *members* are counted. Without it, `editions_of`'s
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
    let holes = std::iter::repeat_n("?", roots.len())
        .collect::<Vec<_>>()
        .join(",");
    format!(" AND m.root_id IN ({holes})")
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
pub fn series(conn: &Connection, roots: &[i64]) -> Result<Vec<SeriesCard>, String> {
    let clause = root_clause(roots);
    let sql = format!(
        "SELECT s.id,
                COALESCE(MAX(CASE WHEN st.is_primary = 1 THEN st.title END), s.sort_title),
                COUNT(DISTINCT w.id),
                COUNT(DISTINCT e.id),
                COUNT(DISTINCT CASE WHEN a.id IS NOT NULL THEN w.id END),
                s.cover_work_id
           FROM series s
           JOIN work w ON w.series_id = s.id
           JOIN edition e ON e.work_id = w.id{clause}
           LEFT JOIN series_title st ON st.series_id = s.id
           LEFT JOIN alignment a ON a.work_id = w.id
          GROUP BY s.id
          ORDER BY 2 COLLATE NOCASE"
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("preparing the series listing: {e}"))?;
    let rows = stmt
        .query_map(params_from_iter(roots), |row| {
            Ok(SeriesCard {
                id: row.get(0)?,
                title: row.get(1)?,
                works: row.get(2)?,
                editions: row.get(3)?,
                aligned: row.get(4)?,
                cover_work: row.get(5)?,
            })
        })
        .map_err(|e| format!("listing series: {e}"))?;
    rows.collect::<rusqlite::Result<Vec<SeriesCard>>>()
        .map_err(|e| format!("reading series: {e}"))
}

/// One work's surviving editions, in key order.
fn editions_of(conn: &Connection, roots: &[i64], work_id: i64) -> Result<Vec<EditionCard>, String> {
    let joined = member_clause(roots);
    let clause = root_clause(roots);
    // Placeholders bind in text order, all anonymous: the join's roots, then
    // the EXISTS clause's roots, then `work_id`. See the module header.
    let sql = format!(
        "SELECT e.key, e.label, COUNT(m.id), COALESCE(SUM(m.pages), 0)
           FROM edition e
           LEFT JOIN member m ON m.edition_id = e.id{joined}
          WHERE 1 = 1{clause} AND e.work_id = ?
          GROUP BY e.id
          ORDER BY e.key"
    );
    let mut params: Vec<&dyn rusqlite::ToSql> =
        roots.iter().map(|r| r as &dyn rusqlite::ToSql).collect();
    params.extend(roots.iter().map(|r| r as &dyn rusqlite::ToSql));
    params.push(&work_id);

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("preparing the edition listing: {e}"))?;
    let rows = stmt
        .query_map(params.as_slice(), |row| {
            Ok(EditionCard {
                key: row.get(0)?,
                label: row.get(1)?,
                members: row.get(2)?,
                pages: row.get(3)?,
            })
        })
        .map_err(|e| format!("listing editions of work {work_id}: {e}"))?;
    rows.collect::<rusqlite::Result<Vec<EditionCard>>>()
        .map_err(|e| format!("reading editions of work {work_id}: {e}"))
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
fn works_of(
    conn: &Connection,
    roots: &[i64],
    predicate: &str,
    extra: &[&dyn rusqlite::ToSql],
    order: &str,
) -> Result<Vec<WorkCard>, String> {
    let clause = root_clause(roots);
    let sql = format!(
        "SELECT DISTINCT w.id, w.slug, w.title, w.volume,
                EXISTS (SELECT 1 FROM alignment a WHERE a.work_id = w.id)
           FROM work w
           JOIN edition e ON e.work_id = w.id{clause}
          WHERE {predicate}
          ORDER BY {order}"
    );
    let mut params: Vec<&dyn rusqlite::ToSql> =
        roots.iter().map(|r| r as &dyn rusqlite::ToSql).collect();
    params.extend_from_slice(extra);

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("preparing the work listing: {e}"))?;
    let rows: Vec<(i64, String, String, Option<f64>, bool)> = stmt
        .query_map(params.as_slice(), |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .map_err(|e| format!("listing works: {e}"))?
        .collect::<rusqlite::Result<_>>()
        .map_err(|e| format!("reading works: {e}"))?;

    rows.into_iter()
        .map(|(id, slug, title, volume, aligned)| {
            let editions = editions_of(conn, roots, id)?;
            let pages = editions.iter().map(|e| e.pages).max().unwrap_or(0);
            Ok(WorkCard {
                id,
                slug,
                title,
                volume,
                editions,
                aligned,
                pages,
            })
        })
        .collect()
}

/// One series and its works, or `None` if this caller cannot see it.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn one_series(
    conn: &Connection,
    roots: &[i64],
    id: i64,
) -> Result<Option<(SeriesCard, Vec<WorkCard>)>, String> {
    let Some(card) = series(conn, roots)?.into_iter().find(|c| c.id == id) else {
        return Ok(None);
    };
    let works = works_of(conn, roots, "w.series_id = ?", &[&id], "w.volume, w.id")?;
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
pub fn work(conn: &Connection, roots: &[i64], slug: &str) -> Result<Option<WorkCard>, String> {
    Ok(works_of(conn, roots, "w.slug = ?", &[&slug], "w.id")?
        .into_iter()
        .next())
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
/// # Errors
///
/// Returns an error if the query fails.
pub fn unfiled(conn: &Connection, roots: &[i64]) -> Result<Vec<WorkCard>, String> {
    works_of(
        conn,
        roots,
        "w.series_id IS NULL",
        &[],
        "w.title COLLATE NOCASE, w.id",
    )
}

/// Search titles through FTS5 — **over both indexes**.
///
/// **The query is quoted as an FTS5 string literal**, doubling any `"`, so a
/// reader typing `AND`, `*` or `-` gets a search for those characters rather
/// than a syntax error or an operator they did not ask for. Spec C1 §13 notes
/// that FTS5's default tokeniser does not segment CJK, so 「五等分の花嫁」 may
/// match as one token or not at all; §8's "name what was searched" empty state
/// exists partly for that, and a `trigram` tokeniser is where this would go
/// next, measured against real titles first.
///
/// `work_fts` has been populated by triggers since migration 1 with nothing
/// reading it. Searching series alone leaves an imported work unfindable by its
/// own title, which is spec C1 §5.4.
///
/// # Errors
///
/// Returns an error if a query fails.
pub fn search(conn: &Connection, roots: &[i64], q: &str) -> Result<SearchResults, String> {
    let trimmed = q.trim();
    if trimmed.is_empty() {
        return Ok(SearchResults {
            series: Vec::new(),
            unfiled: Vec::new(),
        });
    }
    let quoted = format!("\"{}\"", trimmed.replace('"', "\"\""));

    let mut matched = matching_series(conn, &quoted, trimmed)?;
    matched.extend(series_of_matching_works(conn, &quoted, trimmed)?);

    // Filtered through `series` and `unfiled` rather than joined into the FTS
    // queries, so a hit this caller may not see is dropped by exactly the same
    // code that decides every other screen — one filter, one place to read.
    let cap = usize::try_from(LIST_CAP).unwrap_or(usize::MAX);
    let series_hits = series(conn, roots)?
        .into_iter()
        .filter(|c| matched.contains(&c.id))
        .take(cap)
        .collect();

    let unfiled_ids = matching_unfiled_works(conn, &quoted, trimmed)?;
    let unfiled_hits = unfiled(conn, roots)?
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
fn matching_series(conn: &Connection, quoted: &str, shown: &str) -> Result<Vec<i64>, String> {
    ids(
        conn,
        "SELECT DISTINCT st.series_id FROM series_fts f
           JOIN series_title st ON st.rowid = f.rowid
          WHERE series_fts MATCH ?1",
        quoted,
        &format!("searching series for `{shown}`"),
    )
}

/// Series ids dragged in by a *work* whose title matches — searching
/// "Hanayome" should find the shelf, not only the volume that spells it.
fn series_of_matching_works(
    conn: &Connection,
    quoted: &str,
    shown: &str,
) -> Result<Vec<i64>, String> {
    ids(
        conn,
        "SELECT DISTINCT w.series_id FROM work_fts f
           JOIN work w ON w.id = f.rowid
          WHERE work_fts MATCH ?1 AND w.series_id IS NOT NULL",
        quoted,
        &format!("searching works for `{shown}`"),
    )
}

/// Work ids matching, among those belonging to no series.
fn matching_unfiled_works(
    conn: &Connection,
    quoted: &str,
    shown: &str,
) -> Result<Vec<i64>, String> {
    ids(
        conn,
        "SELECT f.rowid FROM work_fts f
           JOIN work w ON w.id = f.rowid
          WHERE work_fts MATCH ?1 AND w.series_id IS NULL",
        quoted,
        &format!("searching unfiled works for `{shown}`"),
    )
}

/// One column of ids from an FTS query, which the three above differ only in.
///
/// **Unbounded, deliberately, and the cap moved to the caller.** These used to
/// `LIMIT ?2` and were only then intersected with what the grant allows — so
/// if the twenty rows SQLite happened to return were all outside the caller's
/// roots, a search that genuinely matched visible series reported nothing
/// found. That is the "filter after the query" failure the module header
/// argues against, committed by the one function that argued it. Found by the
/// whole-branch review.
///
/// Unbounded is affordable because these return ids from an FTS match over
/// title columns, and [`search`] caps the filtered result instead.
fn ids(conn: &Connection, sql: &str, quoted: &str, doing: &str) -> Result<Vec<i64>, String> {
    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| format!("preparing: {doing}: {e}"))?;
    let rows = stmt
        .query_map(rusqlite::params![quoted], |row| row.get(0))
        .map_err(|e| format!("{doing}: {e}"))?;
    rows.collect::<rusqlite::Result<Vec<i64>>>()
        .map_err(|e| format!("{doing}: {e}"))
}

/// The three lists the library screen opens on, for one reader.
///
/// `continuing` is progress that is not finished, `read` is progress that is,
/// and `added` is the newest works — each capped at [`LIST_CAP`], each filtered
/// by the same grant.
///
/// **`user_id` binds twice** in the first two: once for the predicate and once
/// for the ordering subquery, and `works_of` binds `extra` in text order after
/// the roots. One binding is a hard refusal from `rusqlite` rather than a wrong
/// answer.
///
/// # Errors
///
/// Returns an error if a query fails.
pub fn home(conn: &Connection, roots: &[i64], user_id: i64) -> Result<Home, String> {
    let continuing = works_of(
        conn,
        roots,
        "w.id IN (SELECT work_id FROM progress
                   WHERE user_id = ? AND completed_at IS NULL)",
        &[&user_id, &user_id],
        "(SELECT updated FROM progress p WHERE p.work_id = w.id AND p.user_id = ?) DESC",
    )?;
    let read = works_of(
        conn,
        roots,
        "w.id IN (SELECT work_id FROM progress
                   WHERE user_id = ? AND completed_at IS NOT NULL)",
        &[&user_id, &user_id],
        "(SELECT completed_at FROM progress p WHERE p.work_id = w.id AND p.user_id = ?) DESC",
    )?;
    // **`LIMIT` in the SQL, not `.take()` afterwards.** `works_of` fires an
    // `editions_of` query per row it returns, so selecting every work in the
    // library and discarding all but twenty cost one query per work — 69 per
    // `/api/library/home` on the measured corpus, and thousands on a real
    // library. Found by the whole-branch review.
    let added = works_of(conn, roots, "1 = 1", &[], "w.id DESC LIMIT 20")?;

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
/// and nothing in this repository writes it — verified 2026-08-20 — so it is
/// NULL for every series a scan has produced, which is all of them. C1 writes
/// it at no point; C3's metadata editing is its writer, which is where choosing
/// a cover belongs.
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
    let set: Option<i64> = conn
        .query_row(
            "SELECT cover_work_id FROM series WHERE id = ?1",
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
/// `works_of` fires an `editions_of` query per work, and the grid asks for one
/// cover per tile — so a 127-series library rendered thousands of queries to
/// answer 127 questions. Found by the whole-branch review.
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
    conn.query_row(&sql, params.as_slice(), |row| {
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
    let found: Option<i64> = conn
        .query_row(&sql, params.as_slice(), |row| row.get(0))
        .optional()
        .map_err(|e| format!("checking visibility of `{slug}`/`{edition}`: {e}"))?;
    Ok(found.is_some())
}

#[cfg(test)]
mod tests;
