//! The read queries a browse screen needs, with the per-root permission
//! filter applied in SQL rather than after it.
//!
//! Spec C1 §5. One responsibility: reading rows for a screen. Nothing here
//! writes, and nothing here formats — `taiyaku-serve`'s `library_api` turns
//! these into JSON.
//!
//! A work marked missing (`work.missing_since`, O91) is left out of every
//! listing, each listing's own SQL saying so, since no directory holds a file
//! of it to open; [`work`] and [`visible_members`] still read it, so the
//! reader can answer it with that reason rather than as no work at all.
//!
//! # The filter, which is the reason this module exists rather than a handful
//! # of queries at the call sites
//!
//! Every public function but [`held_in`] takes the caller's [`Access`]:
//! `Every` applies no filter, `Only` keeps what lies under its directories, and
//! **`Only` with no directory keeps nothing** — its clause is a false literal, never
//! a clause left out, so a grant emptied by any path cannot read as every
//! library ([`Access::grant_clause`]). `held_in` reads no grant, and its doc
//! says how its caller applies one.
//!
//! The filter is applied to **editions**: an edition survives if its members
//! do, and a work is listed if it keeps at least one edition. A member is
//! under each directory it has a `member_root` row for that holds an entry of
//! its digest or one at its path not found empty (`held_by`), or whose walk
//! reaches its path ([`member_under`]), so a copy held in two libraries is
//! read through either grant (overlapping-libraries spec §3.4). A member under
//! no directory — every member of every imported work — has no row, and
//! `EXISTS` over none is false, so that case needs no branch.
//!
//! A listing narrowed to one library reads the directories that library reaches
//! and the grant allows in place of the grant, so a library's page lists what
//! an account granted only that library would see.
//!
//! An **alignment** names two editions, so it survives only if *both* of them
//! do — both inside the directories the listing reads, and both seen whole by
//! the account, since its runs and counts are in the full numbering of each
//! (spec §3.8). One clause says so — `pair_clause` — and `alignments_of`,
//! `WorkCard::aligned` and `SeriesCard::aligned` all read it, since a grid
//! drawing `⇄ 1` beside a work page listing no pair is one screen
//! contradicting itself.
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
//! So `WHERE e.work_id = ?3 AND … IN (?,?)` counts **five** parameters, not
//! three, and `rusqlite` refuses the call.
//!
//! A constant statement with a single parameter is no exception: a generated
//! `IN (?,?)` spliced into one that carried `?1` would put that `?1` and
//! anonymous holes in one statement, the mix described above, and a rule with
//! standing exceptions is one a later edit will mis-remember. (The
//! hand-written `INSERT`s in the `tests` submodule are fixtures, not queries,
//! and are outside this.)
//!
//! Anonymous-only means the binding order is the **text order of the
//! statement**, and that is the whole rule: each function pushes its
//! parameters in the order the fragments it splices appear in the SQL, reading
//! left to right from the `SELECT` list. Every group a filter contributes is
//! the grant's directories ([`Access::bound`]), and a narrowing to one library
//! swaps the grant for its share of that library (see [`library_scope`])
//! rather than adding a parameter of its own.

// **Imported rather than written `std::collections::HashMap` inline, and that
// is load-bearing for the mutation harness.** `cargo-mutants` replaces a
// function body with a literal it generates — `Ok(HashMap::new())` — and emits
// the short name, so with the path spelled out in full every such mutant would
// fail to *compile* and be reported `unviable`, which is a zero-survivor result
// that establishes nothing about the function.
use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension as _, params_from_iter};

use crate::account::Access;

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
    /// The work and edition this series' cover is drawn from, chosen as
    /// [`cover_work`] chooses it for the access the listing reads, which a
    /// `library` narrows; `None` when no edition of the series holds a member
    /// that access may see.
    pub cover: Option<CoverTarget>,
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
    /// The `sha256` of the member holding this edition's page 0, or `None` when
    /// none of its members holds a page. See [`page_zero_hash`].
    pub page_zero_sha256: Option<String>,
    /// Whether this edition is a reflowable EPUB, read as text rather than as
    /// pages. Its `pages` then counts text blocks. See [`books_among`].
    pub book: bool,
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
    /// asked as one bit.
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
    /// **This is what makes `WorkCard` per-caller.** A work whose hub has lost
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
/// removed. §3's panel opens one work at a time, so this is the route that
/// fills it.
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
/// virtualisation, so this bounds the three *recency* lists rather than
/// the grid.
const LIST_CAP: i64 = 20;

/// The `AND` clause that restricts an edition to the caller's access (see
/// [`Access::grant_clause`]).
///
/// Built as a string because the number of parameters varies with the grant,
/// which a prepared statement cannot express otherwise. The values are `i64`
/// directory ids read from the store, a grant's or a library's (see
/// [`library_scope`]), never caller text, so there is nothing here to inject;
/// the placeholders are still generated rather than the values interpolated,
/// so that stays true if the source ever changes.
///
/// `rm`, not `m`: two callers below already have a `member m` in scope, and an
/// inner alias that shadows an outer one is legal SQL, correct here, and one
/// edit away from silently meaning the other thing.
fn root_clause(access: &Access) -> String {
    root_clause_for("e", access)
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
fn root_clause_for(alias: &str, access: &Access) -> String {
    access.grant_clause(|holes| {
        format!(
            "EXISTS (SELECT 1 FROM member rm WHERE rm.edition_id = {alias}.id AND {})",
            member_under("rm", holes)
        )
    })
}

/// The `AND` keeping edition `alias` only when the caller sees every member of
/// it, so it is sent whole: [`member_clause_for`]'s rule, negated. An
/// alignment's runs, `slots` and `paired` count both its editions in full, so
/// one touching an edition the account sees narrowed tells it the hidden
/// members' size; the work document leaves those out (`View::build` in
/// `taiyaku-serve`), and [`pair_clause`] reads this to leave out the same.
/// Nothing for `Access::Every`, which sees every edition whole.
fn whole_clause_for(alias: &str, access: &Access) -> String {
    access.grant_clause(|holes| {
        format!(
            "NOT EXISTS (SELECT 1 FROM member hm WHERE hm.edition_id = {alias}.id AND NOT {})",
            member_under("hm", holes)
        )
    })
}

/// `?,?,?` for an `IN` list of `n` values.
///
/// One place to read the rule the module header states, that every placeholder
/// here is anonymous. [`Access::grant_clause`] writes its holes here too, for
/// the same reason.
pub(crate) fn holes(n: usize) -> String {
    std::iter::repeat_n("?", n).collect::<Vec<_>>().join(",")
}

/// The `AND` restricting a joined `member m` to the caller's roots.
///
/// **Distinct from [`root_clause`].** `root_clause` decides whether an
/// *edition* survives; this decides which of its *members* are counted.
/// Without it, the edition listing's `LEFT JOIN member m` would count every
/// member of a surviving edition, so a reader granted one root would see member
/// and page counts covering roots they cannot see.
///
/// Reachable in the *default* configuration rather than in a corner: two roots
/// added with `library scan --add` carry no `edition_key`, both fall back to
/// `raw`, and derivation merges their members into one edition.
fn member_clause(access: &Access) -> String {
    member_clause_for("m", access)
}

/// [`member_clause`], against a member alias the caller names, so
/// [`page_zero_hash`] reads it rather than spelling it again. `alias` is a
/// literal at every call site.
fn member_clause_for(alias: &str, access: &Access) -> String {
    access.grant_clause(|holes| member_under(alias, holes))
}

/// Whether member `alias` has a `member_root` row naming one of the `holes`
/// directories that holds it (`held_by`) or whose walk reaches its path:
/// the one rule for which members an account sees (spec §3.8), read by every
/// grant clause here, [`held_in`] and `taiyaku-serve`'s metadata lookups. A
/// removal can hand a member with no entry to a directory whose patterns keep
/// its walk off the file, outside that directory's grant (the owner's ruling
/// "Narrow the grant", O91). The entries are asked first, so the patterns are
/// read only for a row with none. `alias` is a literal.
pub fn member_under(alias: &str, holes: &str) -> String {
    format!(
        "EXISTS (SELECT 1 FROM member_root mr JOIN root r ON r.id = mr.root_id \
         WHERE mr.member_id = {alias}.id AND mr.root_id IN ({holes}) \
           AND ({} OR {WALK_REACHES}(r.path, r.exclude_globs, {alias}.path)))",
        held_by(alias, "mr")
    )
}

/// Whether the directory of `member_root` row `row` holds member `member`: an
/// entry there of its digest, or one at its path the walk did not find empty,
/// whatever its digest (the owner's "Path counts too", O91). So a file replaced
/// in place holds its work, a folder the walk found emptied does not, and a file
/// whose pages the walk did not count, as for any format but zip, does. The one
/// "held" [`member_under`] and `derive::mark_missing` read. The prefix is
/// compared by `substr` up to a `'/'`, never `LIKE` (`_` and `%` occur in
/// names). Both aliases are literals; `he` and `hr` shadow none of a caller's.
pub(crate) fn held_by(member: &str, row: &str) -> String {
    format!(
        "(EXISTS (SELECT 1 FROM scan_entry he \
                   WHERE he.root_id = {row}.root_id AND he.sha256 = {member}.sha256) \
          OR EXISTS (SELECT 1 FROM root hr JOIN scan_entry he ON he.root_id = hr.id \
                      WHERE hr.id = {row}.root_id \
                        AND substr({member}.path, 1, length(hr.path) + 1) = hr.path || '/' \
                        AND he.rel_path = substr({member}.path, length(hr.path) + 2) \
                        AND coalesce(he.pages, 1) > 0))"
    )
}

/// The SQL name of the function [`register_walk_reaches`] adds.
const WALK_REACHES: &str = "walk_reaches";

/// How many pattern texts one connection keeps compiled before it starts
/// over: a text is added per pattern edit, and a store has a few directories.
const COMPILED_PATTERNS: usize = 64;

/// Add `walk_reaches(directory, patterns, path)` to `conn`: whether the walk
/// of the directory at `directory`, excluding `patterns`, reaches a member at
/// `path`, asked as `scan::Excluder::reaches_member` asks it. A path not under
/// the directory is a copy's, whose own path the store does not keep, and is
/// answered true, so such a row counts as it always did.
///
/// **On every connection [`crate::Library`] opens**, since [`member_under`]
/// names it and a statement naming an unknown function fails to prepare.
/// Each connection compiles a pattern text once, not per row or request.
pub(crate) fn register_walk_reaches(conn: &Connection) -> rusqlite::Result<()> {
    let compiled: std::sync::Mutex<HashMap<String, crate::scan::Excluder>> =
        std::sync::Mutex::new(HashMap::new());
    conn.create_scalar_function(
        WALK_REACHES,
        3,
        rusqlite::functions::FunctionFlags::SQLITE_UTF8
            | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
        move |ctx| {
            let directory: String = ctx.get(0)?;
            let patterns: String = ctx.get::<Option<String>>(1)?.unwrap_or_default();
            let path: String = ctx.get(2)?;
            let Some(rel) = crate::reach::nested_in(
                std::path::Path::new(&path),
                std::path::Path::new(&directory),
            ) else {
                return Ok(true);
            };
            let mut compiled = compiled
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if compiled.len() >= COMPILED_PATTERNS && !compiled.contains_key(&patterns) {
                compiled.clear();
            }
            let excluder = compiled
                .entry(patterns)
                .or_insert_with_key(|text| crate::scan::Excluder::build(Some(text)));
            Ok(excluder.reaches_member(&rel))
        },
    )
}

/// One library a listing is narrowed to: its name, and the directories it
/// reaches that the caller's access allows.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryScope {
    pub id: i64,
    pub name: String,
    /// Never empty: see [`library_scope`].
    pub dirs: Vec<i64>,
}

impl LibraryScope {
    /// The access a listing narrowed to this library reads: these directories
    /// and this library alone.
    ///
    /// **The directories as they are, never reached from again.** Reach does
    /// not chain: `/m/kids/tiny` is asked of `/m` directly, so reaching from
    /// `/m/kids` once more could add a directory `/m`'s own patterns leave out.
    pub fn access(&self) -> Access {
        Access::Only {
            roots: self.dirs.clone(),
            libraries: vec![self.id],
        }
    }
}

/// The directories `library` reaches ([`crate::reach::of_libraries`]) that this
/// caller may read, with the library's name.
///
/// **`None` unless the caller was granted `library`**, so its page and name
/// reach only the accounts granted it, and **`None` when no directory is
/// left** or the library does not exist: never a scope with no directories,
/// so "nothing to list" has one answer, which the listings return early on
/// and the OPDS library feed answers with a 404.
///
/// # Errors
///
/// Returns an error if a query fails.
pub fn library_scope(
    conn: &Connection,
    access: &Access,
    library: i64,
) -> Result<Option<LibraryScope>, String> {
    if !access.grants(library) {
        return Ok(None);
    }
    let Some(name) = crate::libraries::name_of(conn, library)? else {
        return Ok(None);
    };
    let roots = crate::scan::roots(conn)?;
    let dirs: Vec<i64> = crate::reach::of_libraries(&roots, &[library])
        .into_iter()
        .filter(|id| access.allows(*id))
        .collect();
    Ok((!dirs.is_empty()).then_some(LibraryScope {
        id: library,
        name,
        dirs,
    }))
}

/// The access a listing reads: the caller's, or [`LibraryScope::access`] for
/// `library`. `None` means there is nothing to list.
fn narrowed(
    conn: &Connection,
    access: &Access,
    library: Option<i64>,
) -> Result<Option<Access>, String> {
    match library {
        None => Ok(Some(access.clone())),
        Some(id) => Ok(library_scope(conn, access, id)?.map(|scope| scope.access())),
    }
}

/// The two `AND EXISTS` halves that keep an alignment row `a` only when
/// **both** of the editions it names survive `access`'s filter and `account`
/// sees both whole ([`whole_clause_for`]).
///
/// **One definition, read by both questions the `alignment` table answers**,
/// because two definitions can disagree: with an `en`↔`raw` pair and a grant of
/// `en` alone, a count without this filter would draw `⇄ 1` and report
/// `aligned: true` while the alignment list, which has it, is empty. The same
/// screen would contradict itself, so [`aligned_clause`] and `alignments_of`
/// read one clause.
///
/// A listing narrowed to one library reaches both sides through `access`, only
/// the directories it reaches ([`library_scope`]), so a pair half of which is
/// under a directory it does not reach is counted by nobody.
/// `account` is the caller's grant, as for [`page_zero_hash`]: what it opens.
///
/// `fe` and `te`: see [`root_clause_for`]. It binds [`pair_bound`]'s values.
fn pair_clause(access: &Access, account: &Access) -> String {
    let from = format!(
        "{}{}",
        root_clause_for("fe", access),
        whole_clause_for("fe", account)
    );
    let to = format!(
        "{}{}",
        root_clause_for("te", access),
        whole_clause_for("te", account)
    );
    format!(
        "EXISTS (SELECT 1 FROM edition fe \
                  WHERE fe.work_id = a.work_id AND fe.key = a.from_key{from}) \
         AND EXISTS (SELECT 1 FROM edition te \
                  WHERE te.work_id = a.work_id AND te.key = a.to_key{to})"
    )
}

/// The values [`pair_clause`] binds, in hole order: for the from side and then
/// the to side, `access`'s directories and then `account`'s.
fn pair_bound<'a>(access: &'a Access, account: &'a Access) -> impl Iterator<Item = i64> + 'a {
    let side = || access.bound().iter().chain(account.bound()).copied();
    side().chain(side())
}

/// Whether work `w` carries an alignment this caller may see, as an expression.
///
/// **A count after the filter.** Without [`pair_clause`], `EXISTS (SELECT 1
/// FROM alignment a WHERE a.work_id = w.id)` asks nothing about roots, so it
/// would count the *unfiltered* alignment set, the exact number the module
/// header says filtering in Rust would produce.
///
/// Requires `w` in scope, and contributes [`pair_bound`]'s values. The
/// correlation `a.work_id = w.id` is written first so those values still bind
/// ahead of whatever the caller's own statement carries.
fn aligned_clause(access: &Access, account: &Access) -> String {
    let pair = pair_clause(access, account);
    format!("EXISTS (SELECT 1 FROM alignment a WHERE a.work_id = w.id AND {pair})")
}

/// The id of series `s`'s cover edition for this caller, as a scalar subquery:
/// NULL when no edition of the series holds a member the caller may see.
///
/// **One ordering for both of [`cover_work`]'s steps.** Only editions with a
/// visible member are ranked, so a hand-set cover the grant hides is not a
/// candidate; the hand-set work's editions rank first, then the lowest
/// `volume`, `w.id` as the tiebreak so volume-less works answer the same row
/// every time, then the first key. [`series`] and [`cover_target`] both read
/// this, so for one access the grid and the `307` cannot choose different
/// covers; a grid narrowed by `library` reads a narrower access, and its
/// cover can differ from the `307`'s.
///
/// Requires `s` in scope, and is placed where the caller has no `cs`, `w`, `e`
/// or `m` of its own, since it names its own. Contributes one
/// [`member_clause`] group.
///
/// **The `ORDER BY` reads its own `series cs`, never the caller's `s`.**
/// SQLite 3.40, the version CI and the image link, cannot resolve an outer
/// column in a subquery's `ORDER BY` and refuses the statement with `no such
/// column`; later versions accept it, so a local run does not show it.
fn cover_edition(access: &Access) -> String {
    let joined = member_clause(access);
    format!(
        "(SELECT e.id FROM series cs
           JOIN work w ON w.series_id = cs.id AND w.missing_since IS NULL
           JOIN edition e ON e.work_id = w.id
          WHERE cs.id = s.id
            AND EXISTS (SELECT 1 FROM member m WHERE m.edition_id = e.id{joined})
          ORDER BY (w.id IS cs.cover_work_id) DESC, w.volume, w.id, e.key
          LIMIT 1)"
    )
}

/// The `sha256` of the member holding edition `alias`'s page 0 as `account`
/// is served it, as a scalar subquery: NULL when none it sees holds a page.
///
/// The page routes narrow an edition to the members `account` sees (spec
/// §3.8) and locate page 0 in the first that holds pages, so a version read
/// past the grant would name bytes the thumbnail route does not send, which
/// it then serves without `COVER_CACHE_CONTROL`. `account` is the caller's
/// grant, never a listing's library, which the route does not know.
fn page_zero_hash(alias: &str, account: &Access) -> String {
    // `member_clause`'s rule: `Access::Every` adds no SQL, any other one
    // `Access::bound` group. `pm`, not `m`: `editions_by_work` has a
    // `member m` in scope (see `root_clause`). `alias` is always a literal.
    let seen = member_clause_for("pm", account);
    format!(
        "(SELECT pm.sha256 FROM member pm
           WHERE pm.edition_id = {alias}.id AND pm.pages > 0{seen}
           ORDER BY pm.ordinal LIMIT 1)"
    )
}

/// Every series this caller may see, with its counts, ordered by title.
///
/// `is_primary` picks the display title; `series_title` holds several names per
/// series so the romaji, the native title and the English one all find the same
/// shelf. A series with no primary title shows the first title it holds, in
/// `rowid` order — the order `librarydoc::set_series_titles` writes a list in —
/// and only then `sort_title`, which is NOT NULL, so every series has a title to
/// show. **Not `sort_title` second — O114.** That column is the scan's key and
/// a correction leaves it alone, so a list sent with no primary would show text
/// the correction may have removed.
///
/// `library` narrows the listing to the directories that library reaches and
/// the grant allows ([`library_scope`]); a library not granted, one with no
/// directory, or one that does not exist lists nothing.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn series(
    conn: &Connection,
    access: &Access,
    user_id: i64,
    library: Option<i64>,
) -> Result<Vec<SeriesCard>, String> {
    let Some(listed) = narrowed(conn, access, library)? else {
        return Ok(Vec::new());
    };
    series_cards(conn, &listed, access, user_id)
}

/// [`series`] over the directories `access` lists, each cover versioned as
/// `account` is served it (see [`page_zero_hash`]).
fn series_cards(
    conn: &Connection,
    access: &Access,
    account: &Access,
    user_id: i64,
) -> Result<Vec<SeriesCard>, String> {
    let aligned = aligned_clause(access, account);
    let clause = root_clause(access);
    let cover = cover_edition(access);
    let page = page_zero_hash("ce", account);
    // Placeholders bind in text order, all anonymous — `account`'s directory
    // list, the `aligned` count's, the listed one, `user_id`, and the listed
    // one again:
    //
    //   0. the cover's `{page}`, first in the outer `SELECT` list;
    //   1. the `aligned` count's [`pair_bound`], in the inner `SELECT` list;
    //   2. the edition join's `{clause}`;
    //   3. `user_id`, in the progress join below;
    //   4. the cover edition's `{cover}`, joined to each grouped row `g`.
    //
    // The cover is joined outside the grouping, so its subquery runs once per
    // series rather than once per joined work, edition, position and title, and
    // the `w`, `e` and `m` it names shadow none of the grouping's (see
    // `root_clause` on why a shadowed alias is avoided).
    //
    // The progress join sits *after* the edition join: as a `SELECT`-list
    // subquery its parameter would bind ahead of every group above, and the
    // filters' own parameters are the only thing allowed in front of it —
    // which is also why `aligned` and the cover may sit where they do.
    //
    // **`aligned` is an `EXISTS` rather than a `LEFT JOIN alignment a`.** The
    // join could carry the same restriction in its `ON`, but it would sit below
    // the progress join, so its roots would bind *after* `user_id`; and it
    // would multiply every grouped row by the number of alignments, which only
    // `COUNT(DISTINCT …)` everywhere else would hide.
    let sql = format!(
        "SELECT g.id, g.title, g.works, g.editions, g.aligned, s.cover_work_id, g.added,
                g.unread, cw.id, cw.slug, ce.key, {page}
           FROM (SELECT s.id AS id,
                        COALESCE(MAX(CASE WHEN st.is_primary = 1 THEN st.title END),
                                 (SELECT title FROM series_title WHERE series_id = s.id
                                   ORDER BY rowid LIMIT 1),
                                 s.sort_title) AS title,
                        COUNT(DISTINCT w.id) AS works,
                        COUNT(DISTINCT e.id) AS editions,
                        COUNT(DISTINCT CASE WHEN {aligned} THEN w.id END) AS aligned,
                        MAX(w.id) AS added,
                        COUNT(DISTINCT CASE WHEN p.work_id IS NULL THEN w.id END) AS unread
                   FROM series s
                   JOIN work w ON w.series_id = s.id AND w.missing_since IS NULL
                   JOIN edition e ON e.work_id = w.id{clause}
                   LEFT JOIN progress p ON p.work_id = w.id AND p.user_id = ?
                   LEFT JOIN series_title st ON st.series_id = s.id
                  GROUP BY s.id) g
           JOIN series s ON s.id = g.id
           LEFT JOIN edition ce ON ce.id = {cover}
           LEFT JOIN work cw ON cw.id = ce.work_id
          ORDER BY 2 COLLATE NOCASE"
    );
    let mut params: Vec<i64> = account.bound().to_vec();
    params.extend(pair_bound(access, account));
    params.extend(access.bound().iter().copied());
    params.push(user_id);
    params.extend(access.bound().iter().copied());

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
                cover: cover_at(row, 8)?,
            })
        })
        .map_err(|e| format!("listing series: {e}"))?;
    rows.collect::<rusqlite::Result<Vec<SeriesCard>>>()
        .map_err(|e| format!("reading series: {e}"))
}

/// Every edition of every work in `work_ids`, keyed by work id, in key order.
///
/// **One statement for the whole set, and one more for every edition's
/// `missing` — O85.**
///
/// A work whose editions the grant wholly hides has **no key in the map**
/// rather than an empty vector, which is [`visible_editions`]' rule for the same
/// question and keeps `WorkCard::editions`' "never empty" contract readable at
/// the one place that fills it.
///
/// **The `IN` list is bounded by SQLite's parameter ceiling, which this trades
/// one unbounded thing for another against.** `unfiled` takes no `LIMIT`, so
/// `work_ids` is as long as the caller's set, and this issues one query with
/// one placeholder per work, where a query per work could never exceed a
/// parameter count. This is left unguarded, because a guard would be code no
/// library can execute.
///
/// Placeholders bind in text order, anonymous: `account`'s roots for page 0
/// and the member join, `access`'s for the `EXISTS` clause, then `work_ids`.
fn editions_by_work(
    conn: &Connection,
    access: &Access,
    account: &Access,
    work_ids: &[i64],
) -> Result<HashMap<i64, Vec<EditionCard>>, String> {
    if work_ids.is_empty() {
        return Ok(HashMap::new());
    }
    // `access` lists the editions and `account` counts them: the reader and
    // the download serve an edition by the account's own grant, so a cell or
    // download label counting a `?library=` listing's share would promise
    // fewer pages than they give (spec §3.6, decision 15).
    let joined = member_clause(account);
    let clause = root_clause(access);
    let wanted = holes(work_ids.len());
    let page = page_zero_hash("e", account);
    let sql = format!(
        "SELECT e.work_id, e.id, e.key, e.label, COUNT(m.id), COALESCE(SUM(m.pages), 0),
                {page}
           FROM edition e
           LEFT JOIN member m ON m.edition_id = e.id{joined}
          WHERE 1 = 1{clause} AND e.work_id IN ({wanted})
          GROUP BY e.id
          ORDER BY e.work_id, e.key"
    );
    let mut params: Vec<&dyn rusqlite::ToSql> = account
        .bound()
        .iter()
        .map(|r| r as &dyn rusqlite::ToSql)
        .collect();
    params.extend(account.bound().iter().map(|r| r as &dyn rusqlite::ToSql));
    params.extend(access.bound().iter().map(|r| r as &dyn rusqlite::ToSql));
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
                    page_zero_sha256: row.get(6)?,
                    book: false,
                },
            ))
        })
        .map_err(|e| format!("listing editions: {e}"))?
        .collect::<rusqlite::Result<Vec<(i64, i64, EditionCard)>>>()
        .map_err(|e| format!("reading editions: {e}"))?;

    let edition_ids: Vec<i64> = rows.iter().map(|(_, id, _)| *id).collect();
    let missing = missing_by_edition(conn, account, &edition_ids)?;
    let books = books_among(conn, &edition_ids)?;

    let mut by_work: HashMap<i64, Vec<EditionCard>> = HashMap::new();
    for (work_id, edition_id, card) in rows {
        by_work.entry(work_id).or_default().push(EditionCard {
            missing: missing.get(&edition_id).copied().unwrap_or(0),
            book: books.contains(&edition_id),
            ..card
        });
    }
    Ok(by_work)
}

/// The editions among `edition_ids` whose members were walked as reflowable
/// EPUBs, read back from `scan_entry` as [`missing_by_edition`] reads, since
/// nothing writes `member.format`.
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub fn books_among(conn: &Connection, edition_ids: &[i64]) -> Result<HashSet<i64>, String> {
    if edition_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let sql = format!(
        "SELECT DISTINCT m.edition_id
           FROM member m
           JOIN root r ON r.id = m.root_id
           JOIN scan_entry se
             ON se.root_id  = m.root_id
            AND se.rel_path = ltrim(substr(m.path, length(r.path) + 1), '/')
          WHERE m.edition_id IN ({}) AND se.format = ?",
        holes(edition_ids.len())
    );
    let mut params: Vec<&dyn rusqlite::ToSql> = edition_ids
        .iter()
        .map(|id| id as &dyn rusqlite::ToSql)
        .collect();
    let token = taiyaku_read::Format::EpubReflow.token();
    params.push(&token);
    let mut stmt = prepare(conn, &sql).map_err(|e| format!("preparing the book listing: {e}"))?;
    stmt.query_map(params.as_slice(), |row| row.get::<_, i64>(0))
        .map_err(|e| format!("listing books: {e}"))?
        .collect::<rusqlite::Result<HashSet<i64>>>()
        .map_err(|e| format!("reading books: {e}"))
}

/// The keys of work `work_id`'s editions that are books, as [`books_among`]
/// tells them, so a server reading a work's editions tells a book as this
/// listing does, whatever its file is named.
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub fn book_keys(conn: &Connection, work_id: i64) -> Result<HashSet<String>, String> {
    let failed = |e: rusqlite::Error| format!("reading the editions of work {work_id}: {e}");
    let keyed: Vec<(i64, String)> = conn
        .prepare_cached("SELECT id, key FROM edition WHERE work_id = ?1")
        .map_err(failed)?
        .query_map([work_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(failed)?
        .collect::<rusqlite::Result<_>>()
        .map_err(failed)?;
    let ids: Vec<i64> = keyed.iter().map(|(id, _)| *id).collect();
    let books = books_among(conn, &ids)?;
    Ok(keyed
        .into_iter()
        .filter(|(id, _)| books.contains(id))
        .map(|(_, key)| key)
        .collect())
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
/// **Takes the caller's access.** Without it this would be a grant leak, which
/// `member_clause`'s own doc calls out as reachable in the default
/// configuration: two roots added the ordinary way merge into one edition, and
/// a caller granted only one of them would be told a `missing` count covering
/// material under the root they cannot see — a fact about material the grant
/// hides. [`member_clause`] is the same clause [`editions_by_work`]'s member
/// join reads for the same reason.
///
/// **O85: a set, not one edition.** **An edition with nothing missing has no
/// row here at all** — `COUNT(*)` over an empty group produces none — so a
/// caller reads its absence as zero rather than expecting a key per edition.
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub fn missing_by_edition(
    conn: &Connection,
    access: &Access,
    edition_ids: &[i64],
) -> Result<HashMap<i64, usize>, String> {
    if edition_ids.is_empty() {
        return Ok(HashMap::new());
    }
    // Parameter order is text order (the module header, and `alignments_of`'s
    // doc): the `IN` list's `?`s precede `{members}`'s anonymous `?`s in the
    // SQL text, matching the order `params` below pushes them in.
    let members = member_clause(access);
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
    params.extend(access.bound().iter().copied());

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

/// One work's alignments, with both sides restricted to the caller's roots
/// and seen whole by it.
///
/// [`pair_clause`] comes **before** `a.work_id = ?` so its parameters bind
/// first, which is this module's one parameter rule — text order. Written the
/// other way round the work id takes hole 1 and every root shifts.
///
/// That clause is shared with [`aligned_clause`] rather than written twice:
/// this list and the `aligned` flag beside it are the same question.
fn alignments_of(
    conn: &Connection,
    access: &Access,
    work_id: i64,
) -> Result<Vec<AlignmentCard>, String> {
    let pair = pair_clause(access, access);
    let sql = format!(
        "SELECT a.from_key, a.to_key, a.verdict, a.slots, a.paired
           FROM alignment a
          WHERE {pair}
            AND a.work_id = ?
          ORDER BY a.from_key, a.to_key"
    );
    let mut params: Vec<i64> = pair_bound(access, access).collect();
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
/// **Separate queries, not one join.** A work joined to its editions returns
/// one row per edition, and rebuilding the nesting from a flat result is where
/// counts go wrong.
///
/// `predicate` and `order` are SQL fragments over `w`, never caller text: every
/// call site below passes a literal. Their parameters bind **after** the roots,
/// in text order. `account` is the caller's grant, which the edition cards'
/// counts and page 0 are read through when `access` lists one library of it.
fn works_of(
    conn: &Connection,
    access: &Access,
    account: &Access,
    user_id: i64,
    predicate: &str,
    extra: &[&dyn rusqlite::ToSql],
    order: &str,
) -> Result<Vec<WorkCard>, String> {
    let aligned = aligned_clause(access, account);
    let clause = root_clause(access);
    // Placeholders bind in text order, all anonymous: `aligned`'s
    // [`pair_bound`] in the `SELECT` list, then the edition join's roots,
    // then `extra`. See the module header.
    let sql = format!(
        "SELECT DISTINCT w.id, w.slug, w.title, w.volume, w.hub, {aligned}
           FROM work w
           JOIN edition e ON e.work_id = w.id{clause}
          WHERE {predicate}
          ORDER BY {order}"
    );
    let paired: Vec<i64> = pair_bound(access, account).collect();
    let mut params: Vec<&dyn rusqlite::ToSql> =
        paired.iter().map(|r| r as &dyn rusqlite::ToSql).collect();
    params.extend(access.bound().iter().map(|r| r as &dyn rusqlite::ToSql));
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
    // statements for any number of works.
    let ids: Vec<i64> = rows.iter().map(|(id, ..)| *id).collect();
    let mut editions = editions_by_work(conn, access, account, &ids)?;
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
/// **Narrowed by `library` as [`series`] is**, card and works alike, so the
/// page a library's tile opens counts and lists what the tile counted (spec
/// §3.6): a series can hold works under several libraries' directories. Each
/// edition's files and pages are still the account's (`editions_by_work`). A
/// library outside the grant, or holding nothing of the series, answers
/// `None`, the answer for a series that does not exist.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn one_series(
    conn: &Connection,
    access: &Access,
    user_id: i64,
    id: i64,
    library: Option<i64>,
) -> Result<Option<(SeriesCard, Vec<WorkCard>)>, String> {
    let Some(listed) = narrowed(conn, access, library)? else {
        return Ok(None);
    };
    let Some(card) = series_cards(conn, &listed, access, user_id)?
        .into_iter()
        .find(|c| c.id == id)
    else {
        return Ok(None);
    };
    let works = works_of(
        conn,
        &listed,
        access,
        user_id,
        "w.series_id = ? AND w.missing_since IS NULL",
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
    access: &Access,
    user_id: i64,
    slug: &str,
) -> Result<Option<WorkDetail>, String> {
    let Some(card) = works_of(
        conn,
        access,
        access,
        user_id,
        "w.slug = ?",
        &[&slug],
        "w.id",
    )?
    .into_iter()
    .next() else {
        return Ok(None);
    };
    let alignments = alignments_of(conn, access, card.id)?;
    Ok(Some(WorkDetail { card, alignments }))
}

/// The works belonging to no series, which no series-joined query can reach.
///
/// **Spec C1 §5.4.** `import.rs` never writes `series_id` — `taiyaku.json` has
/// no field for it — so this is every hand-imported work in the library.
///
/// **A restricted account sees none of them**, and that needs no branch: an
/// imported member has no `member_root` row, so no grant's `EXISTS` finds one.
/// That is §5.1's rule applying unchanged, not a second one.
///
/// **A library narrowing leaves every imported work out of this, by that same
/// rule rather than a new one.** An imported member has no `member_root` row,
/// and the narrowed directory list is never empty ([`library_scope`]), so the
/// clause excludes every imported work: an imported work is in no library, and
/// asking for a library asks for what is in it.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn unfiled(
    conn: &Connection,
    access: &Access,
    user_id: i64,
    library: Option<i64>,
) -> Result<Vec<WorkCard>, String> {
    let Some(listed) = narrowed(conn, access, library)? else {
        return Ok(Vec::new());
    };
    unfiled_cards(conn, &listed, access, user_id)
}

/// [`unfiled`] over the directories `access` lists, as [`series_cards`] is
/// [`series`].
fn unfiled_cards(
    conn: &Connection,
    access: &Access,
    account: &Access,
    user_id: i64,
) -> Result<Vec<WorkCard>, String> {
    works_of(
        conn,
        access,
        account,
        user_id,
        "w.series_id IS NULL AND w.missing_since IS NULL",
        &[],
        "w.title COLLATE NOCASE, w.id",
    )
}

/// The shortest query FTS5's `trigram` tokeniser can answer.
///
/// A trigram index over `五等分の花嫁` answers `五` and `五等` with nothing,
/// and `五等分` with the row — the `*` prefix operator does not change that,
/// because trigram has no prefix to operate on. Below this length the index
/// is not slow, it is silent, which is a different failure and needs a
/// different answer.
const TRIGRAM_MIN: usize = 3;

/// How one query is going to be asked, chosen by its length alone.
///
/// **Two paths rather than one widened one.** `LIKE '%q%'` would answer every
/// query correctly at this library's size (§7), but the index is the part that
/// still holds at ten times that, and the scan is the only thing that answers
/// the first two keystrokes at all.
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
/// Without this, `title LIKE '%' || '%' || '%'` would match every row — a
/// one-character search that dumps the library. `ESCAPE '\'` is what makes
/// the backslash meaningful, and so what makes a typed backslash need one of
/// its own.
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
/// Searching series alone leaves an imported work unfindable by its own title,
/// which is spec C1 §5.4.
///
/// `library` narrows the search as it narrows [`series`] and [`unfiled`].
///
/// # Errors
///
/// Returns an error if a query fails.
pub fn search(
    conn: &Connection,
    access: &Access,
    user_id: i64,
    q: &str,
    library: Option<i64>,
) -> Result<SearchResults, String> {
    let trimmed = q.trim();
    if trimmed.is_empty() {
        return Ok(SearchResults {
            series: Vec::new(),
            unfiled: Vec::new(),
        });
    }
    let Some(listed) = narrowed(conn, access, library)? else {
        return Ok(SearchResults {
            series: Vec::new(),
            unfiled: Vec::new(),
        });
    };
    let matcher = match_for(trimmed);

    let mut matched = matching_series(conn, &matcher, trimmed)?;
    matched.extend(series_of_matching_works(conn, &matcher, trimmed)?);

    // Filtered through `series` and `unfiled` rather than joined into the FTS
    // queries, so a hit this caller may not see is dropped by exactly the same
    // code that decides every other screen — one filter, one place to read.
    let cap = usize::try_from(LIST_CAP).unwrap_or(usize::MAX);
    let series_hits = series_cards(conn, &listed, access, user_id)?
        .into_iter()
        .filter(|c| matched.contains(&c.id))
        .take(cap)
        .collect();

    let unfiled_ids = matching_unfiled_works(conn, &matcher, trimmed)?;
    let unfiled_hits = unfiled_cards(conn, &listed, access, user_id)?
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
              WHERE work_fts MATCH ? AND w.series_id IS NOT NULL
                AND w.missing_since IS NULL",
            q,
        ),
        Match::Like(q) => (
            "SELECT DISTINCT w.series_id FROM work w
              WHERE w.title LIKE '%' || ? || '%' ESCAPE '\\'
                AND w.series_id IS NOT NULL
                AND w.missing_since IS NULL",
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
/// **Unbounded, deliberately, and the cap is the caller's.** A `LIMIT` of
/// twenty here would apply before the grant does, so if the twenty rows SQLite
/// happened to return were all outside the caller's roots, a search that
/// genuinely matched visible series would report nothing found. That is the
/// "filter after the query" failure the module header argues against.
///
/// Unbounded is affordable because these return ids from a query over title
/// columns, and [`search`] caps the filtered result instead. It stays
/// affordable on the `LIKE` path for the same reason §7 declines
/// virtualisation, the size of the library, and that path is only ever reached
/// by a query of one or two characters.
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
/// grant, and each narrowed by `library` as [`series`] is.
///
/// **Both orderings are `MAX(…)` over `progress`.** Since migration 009 a
/// reader holds a position per *edition* of a work, so
/// `(SELECT updated FROM progress p WHERE p.work_id = w.id AND p.user_id = ?)`
/// is a subquery over what can be several rows — and a scalar subquery over
/// several rows returns whichever one SQLite reaches first, with no error and
/// no warning, so the ordering would be silently arbitrary among a work's
/// editions. `MAX(updated)` / `MAX(completed_at)` answer "this work's most
/// recent position", across however many editions it has one in.
///
/// **`read`'s predicate also excludes a work still open in any edition.** The
/// per-edition key can have a work both "finished" and "still reading" at once,
/// when one edition is finished and a sibling is not. Showing the same volume
/// under both Continuing and Read on one screen would state a contradiction to
/// the reader, so a work counts as read only once no edition is still open — a
/// judgement call rather than a measurement, made here rather than by showing
/// the volume in both.
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
pub fn home(
    conn: &Connection,
    access: &Access,
    user_id: i64,
    library: Option<i64>,
) -> Result<Home, String> {
    let Some(listed) = narrowed(conn, access, library)? else {
        return Ok(Home {
            continuing: Vec::new(),
            added: Vec::new(),
            read: Vec::new(),
        });
    };
    let continuing = works_of(
        conn,
        &listed,
        access,
        user_id,
        "w.id IN (SELECT work_id FROM progress
                   WHERE user_id = ? AND completed_at IS NULL)
         AND w.missing_since IS NULL",
        &[&user_id, &user_id],
        "(SELECT MAX(updated) FROM progress p WHERE p.work_id = w.id AND p.user_id = ?) DESC",
    )?;
    let read = works_of(
        conn,
        &listed,
        access,
        user_id,
        "w.id IN (SELECT work_id FROM progress
                   WHERE user_id = ? AND completed_at IS NOT NULL)
         AND w.id NOT IN (SELECT work_id FROM progress
                           WHERE user_id = ? AND completed_at IS NULL)
         AND w.missing_since IS NULL",
        &[&user_id, &user_id, &user_id],
        "(SELECT MAX(completed_at) FROM progress p WHERE p.work_id = w.id AND p.user_id = ?) DESC",
    )?;
    // **`LIMIT` in the SQL, not `.take()` afterwards.** It keeps the rows read
    // proportional to what the route returns rather than to the size of
    // the library.
    let added = works_of(
        conn,
        &listed,
        access,
        user_id,
        "w.missing_since IS NULL",
        &[],
        "w.id DESC LIMIT 20",
    )?;

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
/// and nothing but a test writes it, so it is NULL for every series a scan has
/// produced, which is all of them. C1 writes it at no point, and C3's
/// `PATCH /api/library/series/{id}` has no field for it either — `SeriesEdit`
/// covers titles, reading, tags and external ids, not a cover choice.
///
/// The fallback is the lowest-`volume` work this caller may see, `w.id` as the
/// tiebreak so a series of volume-less works answers the same row every time
/// rather than whichever the b-tree returned.
///
/// A hand-set cover still has to survive the filter: a reader granted one root
/// must not be handed a cover generated from a root they cannot see.
///
/// Both steps are one ordering, [`cover_edition`]'s, which [`series`] reads
/// too, so a grid row's `cover` and this one are the same edition for the
/// same access; a grid narrowed by `library` reads a narrower one.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn cover_work(
    conn: &Connection,
    access: &Access,
    series_id: i64,
) -> Result<Option<CoverTarget>, String> {
    // **`.optional()`, because a series id that does not exist is not an
    // error.** Without it `query_row` returns `QueryReturnedNoRows`, so
    // `/series/9999/cover` would answer with a failure while `/series/9999`
    // answers 404, an enumeration oracle — exactly the distinction `one_series`
    // goes out of its way not to expose.
    cover_target(conn, access, series_id)
        .optional()
        .map(Option::flatten)
        .map_err(|e| format!("reading the cover of series {series_id}: {e}"))
}

/// Which work and which of its editions a cover comes from.
///
/// **The edition matters.** Redirecting to `work.hub` would serve page 0 of an
/// edition the grant hides when the hub is outside it: grant `Manga` (key `en`)
/// and not `Manga (RAW)` (key `raw`, the hub).
#[derive(Debug, Clone, PartialEq)]
pub struct CoverTarget {
    pub work_id: i64,
    pub slug: String,
    /// The first edition of that work this caller may see, never `work.hub`
    /// unless the two coincide.
    pub edition: String,
    /// The `sha256` of the member holding that edition's page 0, or `None`
    /// when none of its members holds a page. See [`page_zero_hash`].
    pub page_zero_sha256: Option<String>,
}

/// One query for the cover, rather than building every `WorkCard` in the
/// series to read one id off the front.
///
/// One row for a series that exists, holding no cover where [`cover_edition`]
/// finds none, and no row for one that does not.
fn cover_target(
    conn: &Connection,
    access: &Access,
    series_id: i64,
) -> rusqlite::Result<Option<CoverTarget>> {
    let cover = cover_edition(access);
    let page = page_zero_hash("ce", access);
    let sql = format!(
        "SELECT cw.id, cw.slug, ce.key, {page}
           FROM series s
           LEFT JOIN edition ce ON ce.id = {cover}
           LEFT JOIN work cw ON cw.id = ce.work_id
          WHERE s.id = ?"
    );
    // `{page}`'s roots, then `{cover}`'s, in text order.
    let mut params: Vec<&dyn rusqlite::ToSql> = access
        .bound()
        .iter()
        .map(|r| r as &dyn rusqlite::ToSql)
        .collect();
    params.extend(access.bound().iter().map(|r| r as &dyn rusqlite::ToSql));
    params.push(&series_id);
    query_row(conn, &sql, params.as_slice(), |row| cover_at(row, 0))
}

/// The [`CoverTarget`] in the four columns from `at`: the work's id and slug,
/// the edition's key and [`page_zero_hash`]. `None` where the work's id is
/// NULL, as a `LEFT JOIN` to no cover edition leaves it.
fn cover_at(row: &rusqlite::Row<'_>, at: usize) -> rusqlite::Result<Option<CoverTarget>> {
    let Some(work_id) = row.get::<_, Option<i64>>(at)? else {
        return Ok(None);
    };
    Ok(Some(CoverTarget {
        work_id,
        slug: row.get(at + 1)?,
        edition: row.get(at + 2)?,
        page_zero_sha256: row.get(at + 3)?,
    }))
}

/// One member of an edition as [`visible_members`] reads it from the store.
/// The caller pairs these with the sidecar's members by position, and
/// `sha256` and `path` let it check the pairing: its sidecar may predate a
/// scan that moved another member to an ordinal, and a flag paired with the
/// wrong member would send an account a member its grant hides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeenMember {
    pub sha256: String,
    pub path: String,
    pub visible: bool,
}

/// For each edition key of the work (or only `edition`), its members in
/// ordinal order, each with whether `access` sees it. Not asked for
/// `Access::Every`.
///
/// **What narrowing a merged edition reads (spec §3.8).** `visible` is
/// [`member_clause`]'s test, so a member is visible here exactly when browse
/// counts it. Ordinal order is the order the sidecar lists an edition's
/// members in. An edition with no member has no entry.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn visible_members(
    conn: &Connection,
    access: &Access,
    slug: &str,
    edition: Option<&str>,
) -> Result<HashMap<String, Vec<SeenMember>>, String> {
    // `1` and the clause's leading ` AND …` make the flag; the clause's holes
    // come first in the text, so its roots are bound before the slug.
    let seen = member_clause(access);
    let only = if edition.is_some() {
        " AND e.key = ?"
    } else {
        ""
    };
    let sql = format!(
        "SELECT e.key, m.sha256, m.path, 1{seen}
           FROM work w
           JOIN edition e ON e.work_id = w.id
           JOIN member m ON m.edition_id = e.id
          WHERE w.slug = ?{only}
          ORDER BY e.key, m.ordinal"
    );
    let mut params: Vec<&dyn rusqlite::ToSql> = access
        .bound()
        .iter()
        .map(|r| r as &dyn rusqlite::ToSql)
        .collect();
    params.push(&slug);
    if let Some(key) = &edition {
        params.push(key);
    }
    let mut stmt =
        prepare(conn, &sql).map_err(|e| format!("preparing `{slug}`'s member listing: {e}"))?;
    let rows = stmt
        .query_map(params.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                SeenMember {
                    sha256: row.get(1)?,
                    path: row.get(2)?,
                    visible: row.get(3)?,
                },
            ))
        })
        .map_err(|e| format!("listing `{slug}`'s visible members: {e}"))?;
    let mut out: HashMap<String, Vec<SeenMember>> = HashMap::new();
    for row in rows {
        let (key, member) = row.map_err(|e| format!("reading `{slug}`'s visible members: {e}"))?;
        out.entry(key).or_default().push(member);
    }
    Ok(out)
}

/// Every edition key this caller may see, for every work, keyed by slug.
///
/// **What `taiyaku-serve`'s `/api/works` list and OPDS `works` feed both need
/// instead of a loop.** `browse::work` already answers "only the editions
/// this caller may see" for one work (`WorkCard::editions`'s own contract),
/// but calling it once per work to build a whole-library list would be an N+1
/// — this answers the whole library in one round trip, and the caller does one
/// `HashSet` lookup per edition after that. [`work`] stays the right tool for a
/// caller that already holds one slug — `get_sidecar` and `patch_alignments`
/// need nothing else.
///
/// This is [`root_clause`] alone — no library narrowing, no alignment, no
/// `member_clause` — because the question is only "does this edition keep a
/// member under one of these roots", which [`visible_members`] answers
/// member by member for one work. A work with zero
/// visible editions has no key in the returned map at all, rather than an
/// entry mapping to an empty set — a caller that iterates the map's keys
/// never sees such a work named.
///
/// Answers every edition of every work not marked missing for
/// [`Access::Every`] — though a caller checking for `Every` first, the way
/// `routes::list_works` does, never reaches this call at all.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn visible_editions(
    conn: &Connection,
    access: &Access,
) -> Result<HashMap<String, HashSet<String>>, String> {
    let clause = root_clause(access);
    let sql = format!(
        "SELECT DISTINCT w.slug, e.key
           FROM work w
           JOIN edition e ON e.work_id = w.id{clause}
          WHERE w.missing_since IS NULL"
    );
    let params: Vec<&dyn rusqlite::ToSql> = access
        .bound()
        .iter()
        .map(|r| r as &dyn rusqlite::ToSql)
        .collect();
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

/// The slugs of the works with a member under a directory `library` reaches
/// ([`crate::reach::of_libraries`]), so a work whose editions are spread is
/// named for each library. A listing narrowed to a library keeps only the
/// editions there ([`library_scope`]); this is for the one view that lists
/// each such work whole.
///
/// **It reads no grant**, so its caller intersects the answer with what
/// [`visible_editions`] allows, as `opds::library_works_by_work` does.
///
/// # Errors
///
/// Returns an error if a query fails.
pub fn held_in(conn: &Connection, library: i64) -> Result<HashSet<String>, String> {
    let dirs = crate::reach::of_libraries(&crate::scan::roots(conn)?, &[library]);
    if dirs.is_empty() {
        return Ok(HashSet::new());
    }
    let mut stmt = prepare(
        conn,
        &format!(
            "SELECT DISTINCT w.slug FROM work w
               JOIN edition e ON e.work_id = w.id
               JOIN member m ON m.edition_id = e.id
              WHERE {}",
            member_under("m", &holes(dirs.len()))
        ),
    )
    .map_err(|e| format!("preparing the works held in library {library}: {e}"))?;
    stmt.query_map(rusqlite::params_from_iter(&dirs), |row| {
        row.get::<_, String>(0)
    })
    .map_err(|e| format!("listing the works held in library {library}: {e}"))?
    .collect::<rusqlite::Result<HashSet<String>>>()
    .map_err(|e| format!("reading the works held in library {library}: {e}"))
}

/// One library as the sidebar lists it.
///
/// `scanned` is whether any of its directories has been walked, so a library page
/// with nothing in it can say "not read yet" rather than "nothing here".
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryEntry {
    pub id: i64,
    pub name: String,
    pub scanned: bool,
}

/// What a first-run screen needs to know, and the only thing that can tell it.
///
/// **Spec C1 §5.5.** `/api/library/series` answers `{ series: [], unfiled: [] }`
/// for "no roots configured" and for "roots configured, scan not yet run"
/// alike, and §8 wants those to be different screens: the difference between
/// "you have no library" and "your library has not been read yet". The answer
/// is per library: each entry says whether that library has been read.
///
/// Read-only, with no writer and no screen of its own.
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub libraries: Vec<LibraryEntry>,
    /// Whether the caller sees only the libraries it was granted. With no
    /// library listed, this tells "none is shared with this account" from
    /// "this server has no library", which are different screens.
    pub restricted: bool,
}

/// The libraries this caller may see, each with whether it has been scanned,
/// ordered by name (`COLLATE NOCASE`) and then by id.
///
/// A library is listed when it has a directory and the caller was granted it
/// ([`Access::grants`]), which for [`Access::Every`] is every library with a
/// directory. **Not every library whose directories the caller reaches**: an
/// account granted Manga reaches the directory Kids holds inside Manga's, and
/// is not listed Kids for that.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn status(conn: &Connection, access: &Access) -> Result<Status, String> {
    let mut stmt = prepare(
        conn,
        "SELECT l.id, l.name,
                EXISTS (SELECT 1 FROM dir_state d JOIN library_root dr ON dr.root_id = d.root_id
                         WHERE dr.library_id = l.id)
           FROM library l
          WHERE EXISTS (SELECT 1 FROM library_root r WHERE r.library_id = l.id)
          ORDER BY l.name COLLATE NOCASE, l.id",
    )
    .map_err(|e| format!("preparing the library listing: {e}"))?;
    let libraries = stmt
        .query_map([], |row| {
            Ok(LibraryEntry {
                id: row.get(0)?,
                name: row.get(1)?,
                scanned: row.get(2)?,
            })
        })
        .map_err(|e| format!("listing libraries: {e}"))?
        .collect::<rusqlite::Result<Vec<LibraryEntry>>>()
        .map_err(|e| format!("reading libraries: {e}"))?
        .into_iter()
        .filter(|entry| access.grants(entry.id))
        .collect();
    Ok(Status {
        libraries,
        restricted: matches!(access, Access::Only { .. }),
    })
}

#[cfg(test)]
pub(crate) mod tests;
