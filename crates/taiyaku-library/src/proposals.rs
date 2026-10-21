//! Proposals: the judgements §1's measurement says no rule should make.
//!
//! §6 and §6.1 of the scanner spec. Suffix-stripping as an edition signal
//! scored roughly 20 right to 17 wrong on a hand classification of the real
//! library — barely better than a coin flip — so nothing here merges
//! anything: [`raise`] proposes, and a person decides through [`accept`],
//! [`unaccept`] or [`dismiss`].
//!
//! # Three decisions, carried from the schema into this module
//!
//! **Dismissal persists.** [`Proposal::subject_id`] keys on the series row
//! `schema/003_scanner.sql` inserted, not on anything content-derived, and
//! [`raise`] relies on the table's expression index — `INSERT OR IGNORE`
//! rather than a check-then-insert — to make a second call a no-op over a
//! dismissal rather than a fresh proposal. See that migration's own comment on
//! `proposal_identity` for why a plain `UNIQUE` would not do this.
//!
//! **Accepting writes a rule, never structure.** [`accept`] inserts one row
//! into `rule` and records its id in `created_rule_id`; it never touches
//! `series`, `work`, `edition` or `member`. §2's claim — editing a rule
//! re-derives the whole library — survives only if derivation has exactly one
//! input, and a direct write here would be a second one. [`unaccept`] deletes
//! the rule it wrote and reopens the proposal, which is what
//! `created_rule_id` is for.
//!
//! **The vocabulary ranks; it never decides.** [`EDITION_WORDS`] is a scoring
//! input, not a filter — see its own doc comment for the counterexample §6
//! measures. The pattern-level control for dismissal is [`open`]'s
//! `min_score` threshold, not a suppression list, because no pattern covers
//! the false positives §1 found: they are unrelated proper nouns whose only
//! shared property is scoring low.
//!
//! # Within a library
//!
//! Part F1 of `docs/superpowers/specs/2026-10-05-libraries-design.md`.
//! [`raise`] pairs only series of one library, and offers a chapter-organised
//! directory the series of the library it is in. A series in no library
//! pairs only with another in none. A proposal whose two series are in
//! different libraries, or one in a library and the other in none, is refused
//! at [`accept`], and no rule accepted for another proposal answers it.
//!
//! # The four kinds, and where each comes from
//!
//! - `edition-of` — [`raise_edition_of`], scored from §6's two signals: a
//!   vocabulary hit in the suffix, and an equal volume count between the two
//!   series.
//! - `same-series` — [`raise_same_series`], §5's second use of
//!   `series_title`: a directory whose name matches any known title of a
//!   series joins that series, which is what relates 「五等分の花嫁」 to
//!   `Go-Toubun no Hanayome` since no pattern can.
//! - `attach-chapters` — [`raise_attach_chapters`], from
//!   [`crate::derive::unattached_chapter_dirs`]: a directory the rules gave a
//!   chapter and no volume, which `derive` already leaves visibly unattached
//!   rather than folding into a work (§7).
//! - `align` — named in the schema's `kind` column comment, for a later spec.
//!   Nothing in this module raises it.
//!
//! # `attach-chapters` cannot be accepted yet
//!
//! §6.1 says accepting writes a narrow `prefix` rule, and a `prefix` rule sets
//! only `edition_key`, `kind`, `language` and `series` — none of them a
//! volume, and `derive` needs one to make a work. Accepting one of these
//! today would flip the proposal to `accepted`, write a rule, and produce
//! nothing a person can see, which is worse than refusing outright. So
//! [`accept`] refuses this one kind by name, naming the command that can
//! actually answer the question — `taiyaku align`, the README's own example —
//! rather than silently doing nothing. See the module's own doc for the two
//! other shapes this could take and why they are not implemented here.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use rusqlite::{Connection, OptionalExtension};

/// The edition words, and what they are for.
///
/// **The vocabulary ranks; it never decides.** §1's roughly-20-to-17 hand
/// classification is the whole argument, and this list does not rescue it:
/// `Shingeki no Kyojin - Kui Naki Sentaku (Complete Color Edition)` matches
/// "Color Edition" and is the colour edition of a *spin-off*, so even the best
/// signal available gets that one wrong.
const EDITION_WORDS: [&str; 8] = [
    "Fan Translated",
    "Color",
    "Colour",
    "Kanzenban",
    "Perfect Edition",
    "Definitive Edition",
    "Deluxe Edition",
    "Digital Colored Comics",
];

/// Which vocabulary word a suffix matched, if any.
///
/// [`raise_edition_of`] only ever needed *whether* one did. [`preview`] needs
/// *which*, because the word is what implies the edition kind — see the
/// mapping in [`implied_edition_kind`]. Extracted rather than duplicated so
/// there is one list and one matching rule; `any` and `find(..).is_some()`
/// decide identically, which is what keeps this a refactor.
fn matched_edition_word(suffix: &str) -> Option<&'static str> {
    EDITION_WORDS
        .iter()
        .copied()
        .find(|word| suffix.contains(word))
}

/// The `edition.kind` a matched vocabulary word implies.
///
/// **This is a reading of the word, not a guess about the material.** Seven of
/// the eight words name a publisher's own edition; `Fan Translated` names the
/// other thing entirely. `language` gets no such treatment and is left unset —
/// an English-looking suffix is evidence about the *suffix*, and
/// `serve/src/comicinfo.rs` already records why deriving a language from a key
/// would be a guess presented as metadata.
fn implied_edition_kind(word: &str) -> &'static str {
    if word == "Fan Translated" {
        "fan"
    } else {
        "official"
    }
}

const KIND_EDITION_OF: &str = "edition-of";
const KIND_SAME_SERIES: &str = "same-series";
const KIND_ATTACH_CHAPTERS: &str = "attach-chapters";

const STATE_OPEN: &str = "open";
const STATE_ACCEPTED: &str = "accepted";
const STATE_DISMISSED: &str = "dismissed";

/// The narrow rule shape [`preview`] derives: a literal `rel_path` prefix.
///
/// **No longer the only shape [`accept_with`] will take.** It was, for as
/// long as the collateral guard could only reason in literal prefixes under
/// one root; [`resolves`] evaluates a candidate through
/// [`crate::rules::Engine`] instead, so every `kind` and `match_on` the
/// engine understands is checked with the semantics the engine gives it. The
/// pair is still named here because `preview` writes exactly it.
const RULE_KIND_PREFIX: &str = "prefix";
const MATCH_ON_PATH: &str = "path";

/// The `rule.id` [`series_reached_by`] gives the candidate while the engine
/// compiles it — an identifier no row in `rule` can hold, so a
/// [`crate::rules::Resolved`] whose `provenance` names it is one the
/// candidate itself decided a field for.
///
/// **Negative rather than `0`.** `rules.rs` already spends `0` on its own
/// `ROOT_FALLBACK` sentinel, the provenance a field takes from the root's
/// default column rather than from any rule; a candidate sharing that id
/// would be indistinguishable from "the root decided this" for `edition_key`,
/// `kind` and `language`. `rule.id` is an `INTEGER PRIMARY KEY` SQLite
/// assigns upwards from 1, so `-1` collides with neither.
const CANDIDATE_RULE_ID: i64 = -1;

/// One §6 signal's worth of score: a vocabulary hit in the suffix, or an
/// equal volume count between the two series. Weighted equally because
/// neither alone clears the bar §1 measured — the strong case is a suffix
/// carrying both, and a suffix carrying only one is exactly the coin flip the
/// vocabulary was built to rank rather than resolve.
const SIGNAL: f64 = 1.0;

/// What [`raise_same_series`] and [`raise_attach_chapters`] score.
///
/// Neither is a suffix-stripping guess. A `same-series` match is a known
/// title — recorded by a person or an earlier import into `series_title` —
/// matching a directory's own name exactly; an `attach-chapters` proposal
/// reports a fact `derive` already found (a chapter, no volume), not a guess
/// about what a suffix means. §6's score threshold exists to separate a real
/// `edition-of` match from an unrelated proper noun, and neither of these two
/// kinds asks that question, so both are scored above the strongest possible
/// `edition-of` match (two signals) rather than competing on the same scale.
///
/// **This is a judgement, not a measurement, and says so here rather than
/// leaving it to look like one.** §1 measured `edition-of`'s signal —
/// suffix-stripping, hand-classified against the real library — and nothing
/// else. `same-series` and `attach-chapters` have no survey behind them; the
/// number is a reasonable placement on the scale [`open`]'s threshold reads,
/// not a finding.
const CERTAIN: f64 = 2.0 * SIGNAL + 1.0;

/// The message [`accept`] refuses an `attach-chapters` proposal with.
///
/// Names the command that can actually answer the question — the README's
/// own example — rather than leaving a person to guess why nothing changed.
const ATTACH_CHAPTERS_REFUSAL: &str = "this proposal names a chapter-organised directory, and \
     accepting it would write a rule that sets no volume — derive needs one to make a work. \
     Which files are which volume is a question only alignment answers: run it by hand, e.g. \
     `taiyaku align raw-v01.cbz -- en-c001.cbz en-c002.cbz …`, the README's own example.";

/// One judgement raised for a person to accept or dismiss — never applied on
/// its own. A projection of one `proposal` row (`schema/003_scanner.sql`).
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    pub id: i64,
    /// `edition-of` | `same-series` | `attach-chapters` | `align`.
    pub kind: String,
    /// The series row this proposal is about, and what a dismissal stays
    /// keyed on across a rescan.
    pub subject_id: i64,
    /// The other series this proposal relates `subject_id` to. `None` for
    /// `attach-chapters`, which has no second series to point at.
    pub target_id: Option<i64>,
    pub score: f64,
    /// Shown to the person, because a proposal whose basis is invisible is a
    /// coin flip with a confidence score attached.
    pub reason: String,
    /// `open` | `accepted` | `dismissed`.
    pub state: String,
    /// The `rule` [`accept`] wrote, if this proposal is accepted. `None` for
    /// every other state, and also for an accepted proposal whose rule was
    /// since deleted through C4's editor — [`raise`] is what notices that and
    /// reopens the proposal.
    pub created_rule_id: Option<i64>,
}

/// Find every proposal this library's current facts support, and open the
/// ones nobody has already judged.
///
/// Idempotent: raising over unchanged facts inserts nothing a second time,
/// because `proposal_identity` (`schema/003_scanner.sql`) is what backs
/// `INSERT OR IGNORE` here — a proposal already `dismissed` or `accepted`
/// stays exactly that rather than reappearing as a new `open` row. Also
/// reopens any `accepted` proposal whose `created_rule_id` has gone `NULL`,
/// which is what a rule deleted through C4's editor leaves behind.
///
/// **What the number counts, exactly.** Proposals that are `open` now and
/// were not before this call — which is two things, not one: proposals this
/// call created, and accepted proposals it reopened because their rule has
/// gone. Both are a row appearing in somebody's inbox that was not there
/// before, which is what `library scan`'s "N proposal(s) raised" tells a
/// person to go and look at, so they are summed rather than reported apart.
/// What is *not* counted is a refresh: [`insert_proposal`] rewrites the score
/// and reason of a proposal already open, and that is the same row saying
/// something better, not a new thing to decide. If the two sources ever need
/// telling apart at the surface, that is a return type, not a second call —
/// a second `raise` returns zero for everything the first already opened.
///
/// # Errors
///
/// Returns an error if `series`, `series_title`, `work` or `rule` cannot be
/// read, if [`crate::derive::unattached_chapter_dirs`] fails for the same
/// reasons [`crate::derive::derive`] can, or if a write to `proposal` is
/// refused.
pub fn raise(conn: &Connection) -> Result<usize, String> {
    let series = all_series(conn)?;
    let mut newly_open = 0;
    newly_open += raise_edition_of(conn, &series)?;
    newly_open += raise_same_series(conn, &series)?;
    newly_open += raise_attach_chapters(conn)?;
    newly_open += reopen_orphaned_accepts(conn)?.len();
    Ok(newly_open)
}

/// What the inbox's filter tabs narrow proposals by — C3 §7's query string.
#[derive(Debug, Clone)]
pub struct Filter {
    /// `open` | `accepted` | `dismissed`; `None` is every state.
    pub state: Option<String>,
    /// `edition-of` | `same-series` | `attach-chapters` | `align`; `None` is
    /// every kind.
    pub kind: Option<String>,
    /// §6.1's score threshold — the scale it reads is `{0, 1, 2, 3}`, see
    /// [`SIGNAL`] and [`CERTAIN`], so this is a choice among four values, not
    /// a percentile.
    pub min_score: f64,
}

/// The columns every projection of a `proposal` row into [`Proposal`]
/// selects, in the order [`proposal_from_row`] reads them back. Shared by
/// [`list`] and [`one`] so a column added to one cannot go missing from the
/// other.
///
/// **Qualified to `p`, so every statement using it aliases `proposal` as
/// `p`.** [`list`] joins `series` twice and both tables carry an `id`, which
/// is ambiguous unqualified; the alternative — a second, joined-only copy of
/// this list — is the drift this constant exists to prevent.
const PROPOSAL_COLUMNS: &str = "p.id, p.kind, p.subject_id, p.target_id, p.score, p.reason, \
     p.state, p.created_rule_id";

fn proposal_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Proposal> {
    Ok(Proposal {
        id: r.get(0)?,
        kind: r.get(1)?,
        subject_id: r.get(2)?,
        target_id: r.get(3)?,
        score: r.get(4)?,
        reason: r.get(5)?,
        state: r.get(6)?,
        created_rule_id: r.get(7)?,
    })
}

/// One proposal **as a list of them is read**: the row, plus the `sort_title`
/// of each series it names.
///
/// **A separate type rather than two more fields on [`Proposal`].** A
/// collapsed inbox row has to say which series this proposal is about — C3
/// §3's whole argument for a scannable list over a triage loop is seeing that
/// 29 of 37 rows ask the same question, and rows that cannot be told apart
/// are not scannable. But [`lookup`], [`one`] and [`evidence`] read a single
/// proposal and already have the titles by other means ([`Side`] carries
/// them), so putting the titles on `Proposal` would make every one of those
/// callers carry two fields it does not use and cannot fill without a join it
/// does not need.
///
/// **Not the same thing as the leading words of [`Proposal::reason`].** Every
/// `reason` this module writes happens to open with the subject's quoted
/// title, and a row drawn from that string alone looked identifiable while
/// the window was wide. It is not a contract: [`scored_reasons`] deliberately
/// re-words the same signals *without* titles for the evidence panel, so
/// nothing stops the `format!` behind a reason changing — and a row whose
/// identity came from it would then be a kind, a strength and a truncated
/// sentence, identical to every other row of that kind.
#[derive(Debug, Clone, PartialEq)]
pub struct ListedProposal {
    pub proposal: Proposal,
    /// `series.sort_title` for [`Proposal::subject_id`]. Not optional:
    /// `subject_id` is `NOT NULL REFERENCES series(id) ON DELETE CASCADE`
    /// and `PRAGMA foreign_keys` is ON for every connection this crate hands
    /// out (`lib.rs`, and `tests.rs`'s `every_connection_enforces_foreign_keys`
    /// is what keeps it that way), so a proposal cannot outlive its subject
    /// and the inner join below cannot silently drop a row.
    pub subject_title: String,
    /// `series.sort_title` for [`Proposal::target_id`], and `None` exactly
    /// where that is — an `attach-chapters` proposal names one series only.
    pub target_title: Option<String>,
}

fn listed_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ListedProposal> {
    Ok(ListedProposal {
        proposal: proposal_from_row(r)?,
        subject_title: r.get(8)?,
        target_title: r.get(9)?,
    })
}

/// Proposals matching `filter`, highest score first, each carrying the
/// titles of the series it names — see [`ListedProposal`].
///
/// **The one query every multi-row listing of `proposal` goes through.**
/// [`open`] is this with `state` fixed to `open` and `kind` left
/// unrestricted; it stays a function of its own because its signature has
/// callers in this crate and in `taiyaku-cli` this task does not touch, not
/// because it is a different query.
///
/// **Not the one query every *read* goes through.** [`counts_by_kind`]
/// answers "how many", not "which rows", and runs its own `GROUP BY` rather
/// than counting this query's output; [`resolves`] reads the two ends of one
/// proposal with a `SELECT` of its own; and [`lookup`] reads a single row by
/// id — it is `pub`, and `taiyaku-serve`'s curation routes call it to tell an
/// absent proposal apart from a failed read. Everything else in this module
/// that wants a single proposal reaches it through [`lookup`], which [`one`]
/// wraps.
///
/// So a filter or an audit hook meant to reach every read of `proposal` has
/// to go into each statement that names the table rather than here alone —
/// and `grep 'FROM proposal'` over this file is that list, which is the one
/// worth trusting. Naming the callers instead is what earlier versions of
/// this paragraph did, and each of them went stale the next time one moved.
///
/// # Errors
///
/// Returns an error if the query against `proposal` fails.
pub fn list(conn: &Connection, filter: &Filter) -> Result<Vec<ListedProposal>, String> {
    // **The titles are joined, not fetched per row.** An inbox is dozens of
    // rows and each names one or two series; a title read per row is the N+1
    // this join exists instead of. The subject join is inner and the target
    // join is not — see [`ListedProposal`]'s own fields for why each is what
    // it is.
    let mut sql = format!(
        "SELECT {PROPOSAL_COLUMNS}, s.sort_title, t.sort_title \
         FROM proposal p \
         JOIN series s ON s.id = p.subject_id \
         LEFT JOIN series t ON t.id = p.target_id \
         WHERE p.score >= ?1"
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(filter.min_score)];
    if let Some(state) = &filter.state {
        params.push(Box::new(state.clone()));
        let _ = write!(sql, " AND p.state = ?{}", params.len());
    }
    if let Some(kind) = &filter.kind {
        params.push(Box::new(kind.clone()));
        let _ = write!(sql, " AND p.kind = ?{}", params.len());
    }
    sql.push_str(" ORDER BY p.score DESC, p.id");

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("listing proposals: {e}"))?;
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(AsRef::as_ref).collect();
    let rows = stmt
        .query_map(refs.as_slice(), listed_from_row)
        .map_err(|e| format!("listing proposals: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("listing proposals: {e}"))
}

/// How many proposals of each kind currently sit in `state` at or above
/// `min_score` — what the inbox's filter tabs count themselves by.
///
/// **`min_score` is a parameter because a count over a different set from the
/// list beside it is a wrong number, not a looser one.** [`Filter`] carries a
/// threshold and this took none, so `?state=open&min_score=9` answered
/// `{"counts":{"edition-of":1},"proposals":[]}` — measured, at the route: a
/// tab reading "1" over an empty list. `kind` is deliberately *not* a
/// parameter here: the tabs are what a kind filter is chosen from, and a
/// facet counted with its own filter applied can only ever count itself out
/// of existence.
///
/// # Errors
///
/// Returns an error if the query against `proposal` fails.
pub fn counts_by_kind(
    conn: &Connection,
    state: &str,
    min_score: f64,
) -> Result<BTreeMap<String, usize>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT kind, count(*) FROM proposal WHERE state = ?1 AND score >= ?2 GROUP BY kind",
        )
        .map_err(|e| format!("counting proposals: {e}"))?;
    let rows = stmt
        .query_map(rusqlite::params![state, min_score], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })
        .map_err(|e| format!("counting proposals: {e}"))?;
    let mut counts = BTreeMap::new();
    for row in rows {
        let (kind, n) = row.map_err(|e| format!("counting proposals: {e}"))?;
        counts.insert(kind, usize::try_from(n).unwrap_or(0));
    }
    Ok(counts)
}

/// Every proposal's state, keyed by id.
///
/// **[`crate::preview::preview`]'s snapshot type, the same idea as
/// [`crate::preview::Grouping`] applied to `proposal` instead of `member`.**
/// A derivation reaches exactly one proposal writer — [`raise`], which only
/// ever inserts a row or flips an `accepted` one back to `open`
/// ([`reopen_orphaned_accepts`]) — so a plain open-count taken before and
/// after cannot tell those two apart, and can only ever go up. Keying by id
/// and reading state back is what lets [`reopened`] tell a rule's
/// disappearance from a proposal nobody had judged yet.
pub type States = BTreeMap<i64, String>;

/// Snapshot every proposal's current state — see [`States`].
///
/// **`pub(crate)`, not `pub`.** Its only callers are [`crate::preview::preview`]
/// and this crate's own `preview/tests.rs`, the same narrowing
/// [`crate::rules::replace_all`] got last round: a function nothing outside
/// the crate calls is a `pub` surface nobody uses and nobody has decided to
/// support.
///
/// # Errors
///
/// Returns an error if the query against `proposal` fails.
pub(crate) fn states(conn: &Connection) -> Result<States, String> {
    let mut stmt = conn
        .prepare("SELECT id, state FROM proposal")
        .map_err(|e| format!("preparing the proposal-state query: {e}"))?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| format!("reading proposal states: {e}"))?;
    let mut out = States::new();
    for row in rows {
        let (id, state) = row.map_err(|e| format!("reading a proposal state: {e}"))?;
        out.insert(id, state);
    }
    Ok(out)
}

/// Which proposals were `accepted` in `before` and are `open` in `after`
/// — [`crate::preview::Impact::proposals_reopened`]'s one definition, shared
/// by [`crate::preview::preview`] and by `preview/tests.rs` for the same
/// reason `preview`'s own private `diff` function is: two ways of computing
/// what "reopened" means is how they come to disagree.
///
/// **Ascending, and that is [`States`]' own `BTreeMap` ordering rather than a
/// sort performed here** — the same contract
/// [`reopen_orphaned_accepts`] states for its own return, and the shape
/// [`crate::rules::Deleted::reopened_proposals`] already answers. Retyping
/// `States` as a `HashMap` would make this order arbitrary without changing a
/// line of this function, so the ordering is asserted in
/// `proposals/tests.rs` rather than left to the type.
///
/// **`pub(crate)`, not `pub`**, for the same reason [`states`] just above is
/// — its only callers are inside this crate.
pub(crate) fn reopened_ids(before: &States, after: &States) -> Vec<i64> {
    after
        .iter()
        .filter(|(id, state)| {
            state.as_str() == STATE_OPEN && before.get(*id).is_some_and(|was| was == STATE_ACCEPTED)
        })
        .map(|(id, _state)| *id)
        .collect()
}

/// How many proposals [`reopened_ids`] named — the count
/// [`crate::preview::Impact::proposals_reopened`] carries.
///
/// **Its `.len()`, not a second filter.** Two ways of arriving at one number
/// is how they come to disagree — the argument [`crate::preview::Impact`]'s
/// own `proposals_raised` field makes for carrying the derive report's count
/// through rather than recounting it here.
pub(crate) fn reopened(before: &States, after: &States) -> usize {
    reopened_ids(before, after).len()
}

/// Every open proposal scoring at least `min_score`, highest score first.
///
/// `min_score` is §6.1's score threshold: the weak proposals stay in the
/// table and reachable at a lower threshold rather than being hidden by a
/// suppression list, because no pattern covers the false positives §1
/// measured.
///
/// # Errors
///
/// Returns an error if the query against `proposal` fails.
pub fn open(conn: &Connection, min_score: f64) -> Result<Vec<Proposal>, String> {
    // **Drops the joined titles rather than exposing them.** [`list`] answers
    // [`ListedProposal`] because the inbox's collapsed row needs the titles;
    // this function's callers — `taiyaku-cli`'s `library` command among them
    // — print a `reason` and a score, and widening their type to hand them
    // two fields they do not read is a cost paid at every call site for one
    // caller's benefit. The join still runs, which is one indexed lookup per
    // row against `series`' primary key and not a second statement.
    Ok(list(
        conn,
        &Filter {
            state: Some(STATE_OPEN.to_string()),
            kind: None,
            min_score,
        },
    )?
    .into_iter()
    .map(|listed| listed.proposal)
    .collect())
}

/// The `rule` row an accept would write, before it is written.
///
/// §7 of C3: *"the client shows the rule, the server writes what it was
/// shown"*. A server that re-derived the rule at write time could write
/// something the preview did not say, which is the whole reason this type
/// crosses the wire in both directions.
#[derive(Debug, Clone, PartialEq)]
pub struct ProposedRule {
    /// `NULL` means every root, which is what a widened rule usually wants.
    pub root_id: Option<i64>,
    /// `prefix` for the narrow default; `regex` once widened.
    pub kind: String,
    pub pattern: String,
    /// Always `path` here. `rules.rs` also has `name` and `parent`; neither
    /// addresses "this directory's material", which is what an accept claims.
    pub match_on: String,
    pub set_series: Option<String>,
    pub set_edition_key: Option<String>,
    pub set_kind: Option<String>,
    pub set_language: Option<String>,
    pub note: String,
}

/// One proposal's facts, after the three refusals every accept makes about
/// the proposal itself — as opposed to about the rule — have been made.
struct AcceptableProposal {
    kind: String,
    /// The `target_id` column already unwrapped: a proposal carrying none is
    /// refused before this is built, so nothing downstream re-asks.
    target_id: i64,
}

/// What this module made of a request about one proposal: the thing asked
/// for, or the sentence saying why the library declines it.
///
/// **A refusal is a different outcome from a failure**, and one
/// `Result<_, String>` cannot tell them apart. [`evidence`] needed to first: it
/// answers `rule: None` for every proposal that cannot be accepted and still
/// shows the row, and it did that with `preview(conn, id).ok()` — which
/// turned a database read that *failed* into the same `None`, presenting a
/// fault as "this cannot be accepted".
///
/// **The three writes needed to next (O70).** [`accept_with`], [`unaccept`]
/// and [`dismiss`] answered their refusals and their store failures through
/// one `Result<_, String>`, and `taiyaku-serve` can only map a status onto the
/// type it is handed: a trigger failing the `INSERT INTO rule` inside an
/// accept answered **409**, a refusal about the library's state, for a store
/// that had failed. Each answers `Result<Offer<_>, String>` now.
///
/// Beside a `Result`, `Err` means a read or a write failed — or, for a
/// function reading its proposal by id, that there is no such proposal, which
/// [`lookup`] tells apart without reading the sentence. `Refused` means the
/// library declines and nothing was written, in a sentence written for the
/// person who asked.
#[derive(Debug, PartialEq)]
#[must_use = "a refusal is not an error, so dropping this drops a refusal silently"]
pub enum Offer<T> {
    /// What was asked for: the rule worked out, or the write done.
    Made(T),
    /// Why the library declines, for the person who asked. Nothing was
    /// written.
    Refused(String),
}

impl<T> Offer<T> {
    /// The refusal as an error, which is [`preview`]'s public contract and
    /// [`accept`]'s.
    fn or_refusal(self) -> Result<T, String> {
        match self {
            Self::Made(made) => Ok(made),
            Self::Refused(why) => Err(why),
        }
    }
}

/// Read proposal `id` and refuse it if an accept must not proceed.
///
/// [`accept_with`]'s way in to [`acceptable`] — see there for the refusals
/// themselves and for why they live in one place — and then to
/// [`across_libraries`].
///
/// # Errors
///
/// Returns an error if `id` names no proposal or if a read fails. Each of
/// [`acceptable`]'s refusals is [`Offer::Refused`], and so is
/// [`across_libraries`]'s.
fn acceptable_proposal(conn: &Connection, id: i64) -> Result<Offer<AcceptableProposal>, String> {
    let proposal = one(conn, id)?;
    let facts = match acceptable(&proposal) {
        Offer::Made(facts) => facts,
        Offer::Refused(why) => return Ok(Offer::Refused(why)),
    };
    if let Some(why) = across_libraries(conn, &proposal, facts.target_id)? {
        return Ok(Offer::Refused(why));
    }
    Ok(Offer::Made(facts))
}

/// Refuse `proposal` if an accept must not proceed: not `open`,
/// `attach-chapters`, or no `target_id` to write a rule against.
///
/// **One helper, three callers, deliberately.** These three refusals used to
/// live in [`accept`] alone and then, after the split, in [`preview`] alone —
/// and [`accept_with`] is a `pub fn` that C3's
/// `POST /api/library/proposals/{id}/accept` calls with an admin-supplied
/// rule, so it cannot assume its caller came through [`preview`] first.
/// Expressing the refusals twice would be two places for them to drift; this
/// is the one place they are expressed, and every caller asks it.
///
/// **Over a `Proposal` already read, rather than reading its own row.** That
/// is what lets [`evidence`] — which has one in hand — tell these three
/// refusals apart from a read that failed without expressing them a second
/// time: no read happens here, so it has no failure to report, and its
/// answer is an [`Offer`] with no `Result` around it.
///
/// **A fourth refusal needs a read, so it is not here.** It is
/// [`across_libraries`]'s, which reads each series' library.
fn acceptable(proposal: &Proposal) -> Offer<AcceptableProposal> {
    let Proposal {
        id,
        kind,
        target_id,
        state,
        ..
    } = proposal;
    let (id, kind) = (*id, kind.clone());

    // **A state guard, not just a happy-path assumption.** Without it, a
    // second `accept` on an already-accepted proposal writes a second rule
    // and overwrites `created_rule_id` with its id — `unaccept` then deletes
    // only the second, and the first stays in force deciding structure,
    // invisible to the proposal that made it, while the proposal reads
    // `open`. That is exactly what `created_rule_id` exists to prevent.
    if state != STATE_OPEN {
        return Offer::Refused(format!(
            "proposal {id} is `{state}`, not open — accept only applies to an open proposal"
        ));
    }
    if kind == KIND_ATTACH_CHAPTERS {
        return Offer::Refused(format!("proposal {id}: {ATTACH_CHAPTERS_REFUSAL}"));
    }
    let Some(target_id) = *target_id else {
        return Offer::Refused(format!(
            "proposal {id} (kind `{kind}`) has no target series to write a rule against"
        ));
    };

    Offer::Made(AcceptableProposal { kind, target_id })
}

/// The refusal for a proposal whose two series are not filed together, or
/// `None` where they are. Two series in two different libraries are refused
/// in one sentence, and a series in a library beside one in none in another.
/// Two series of one library are not refused, and neither are two in no
/// library.
fn across_libraries(
    conn: &Connection,
    proposal: &Proposal,
    target_id: i64,
) -> Result<Option<String>, String> {
    let subject = series_library(conn, proposal.subject_id)?;
    let target = series_library(conn, target_id)?;
    if subject == target {
        return Ok(None);
    }
    Ok(Some(if subject.is_none() || target.is_none() {
        format!(
            "proposal {}: one of its two series is in no library. A derive files a series under \
             the library whose directories hold it; if this one is still in none after a scan, \
             dismiss the proposal.",
            proposal.id
        )
    } else {
        format!(
            "proposal {}: its two series are in different libraries, and a rule moves \
             material only within one. Dismiss it; the next raise proposes only within a \
             library.",
            proposal.id
        )
    }))
}

/// The rule [`accept`] would write for proposal `id`, without writing it.
///
/// Every refusal [`accept`] makes before its transaction opens is made here
/// instead, so a client is never shown a rule the write will reject: the
/// state guard, the `attach-chapters` refusal by name, a missing `target_id`,
/// two series in different libraries or one of the two in none, a subject
/// whose entries share no single directory, a target renamed since the
/// proposal was raised, and a suffix with no letters or digits to make an
/// edition key from.
///
/// The first three of those are [`acceptable`]'s rather than this
/// function's: [`accept_with`] is a `pub fn` that cannot assume its caller
/// came through here, so they are written once and called three times. The
/// fourth is [`across_libraries`]'s, for the same reason.
///
/// # Errors
///
/// As [`accept`], for every condition listed above.
pub fn preview(conn: &Connection, id: i64) -> Result<ProposedRule, String> {
    let proposal = one(conn, id)?;
    // **Refused before the sweep, not after it.** [`offered_rule`] makes
    // these refusals itself — it has to, since [`evidence`] reaches it
    // by another road — but it is handed the paths rather than reading them,
    // and finding them costs a pass over every `scan_entry` row in the
    // library. A proposal that cannot be accepted at all must not pay for
    // one to be told so.
    let AcceptableProposal { target_id, .. } = acceptable(&proposal).or_refusal()?;
    if let Some(why) = across_libraries(conn, &proposal, target_id)? {
        return Err(why);
    }
    let subject_title = series_title(conn, proposal.subject_id)?;
    let subject_library = series_library(conn, proposal.subject_id)?;
    let paths = crate::derive::rel_paths_for_series(conn, subject_library, &subject_title)?;
    offered_rule(conn, &proposal, &subject_title, &paths)?.or_refusal()
}

/// [`preview`]'s answer for a proposal already read, over the entry paths its
/// subject resolves to — which the caller has already swept for.
///
/// **The sweep is the caller's because two callers hold it already.**
/// [`crate::derive::rel_paths_for_series`] compiles every rule in the
/// library, reads every root and resolves every `scan_entry` row under each,
/// and [`evidence`] needs exactly that answer for the side it is drawing
/// anyway. Reading it here as well made an expanded row pay for the subject's
/// sweep twice — measured: `material_of` for the subject, `material_of` for
/// the target, then this function's own for the subject again, three passes
/// for a route a person triggers by opening a row.
///
/// # Errors
///
/// If a read fails. A proposal this declines to offer a rule for is
/// [`Offer::Refused`], not an error — see [`Offer`].
fn offered_rule(
    conn: &Connection,
    proposal: &Proposal,
    subject_title: &str,
    subject_paths: &[(i64, String)],
) -> Result<Offer<ProposedRule>, String> {
    let id = proposal.id;
    let AcceptableProposal { kind, target_id } = match acceptable(proposal) {
        Offer::Made(acceptable) => acceptable,
        Offer::Refused(why) => return Ok(Offer::Refused(why)),
    };
    if let Some(why) = across_libraries(conn, proposal, target_id)? {
        return Ok(Offer::Refused(why));
    }

    let target_title = series_title(conn, target_id)?;
    let (pattern, root_id) = match common_directory_pattern(conn, id, subject_paths)? {
        Offer::Made(derived) => derived,
        Offer::Refused(why) => return Ok(Offer::Refused(why)),
    };

    // **`edition-of` sets the edition key too, not only the series.**
    // `set_series` alone makes the accepted directory's material extra pages
    // of the *base* edition rather than a second edition — measured: after
    // accept and re-derive, three works each with one `raw` edition holding
    // two members, base and colour concatenated into a single page sequence.
    // For a proposal whose whole claim is "this is the colour edition of
    // that", silently merging the pages is the wrong outcome and a reader
    // sees it. The key is the suffix that made the proposal an `edition-of`
    // one in the first place — the part of the subject's own title after
    // `"{target} - "` — slugified, so it reads the way `raise_edition_of`'s
    // `reason` already does: `full-color-edition` for
    // `Go-Toubun no Hanayome - Full Color Edition`. `same-series` sets no
    // edition key: it is a claim about identity, not about a second edition.
    let (set_edition_key, set_kind) = if kind == KIND_EDITION_OF {
        // **A missing prefix relationship is refused, not silently
        // absorbed.** If the target was renamed between this proposal being
        // raised and being accepted, `strip_prefix` finds nothing, and
        // falling back to `None` here would silently restore the very bug
        // this fix exists for: `set_series` alone, the colour volume's pages
        // concatenated onto the base edition's. The failure has to be
        // visible instead.
        let Some(suffix) = subject_title.strip_prefix(&format!("{target_title} - ")) else {
            return Ok(Offer::Refused(format!(
                "proposal {id}: \"{subject_title}\" no longer starts with \"{target_title} - \" \
                 — one of the two series was renamed since this proposal was raised, and \
                 accepting now would set only set_series, silently merging the colour volume's \
                 pages into the base edition. Dismiss this proposal and let the next raise \
                 propose it again against the current titles."
            )));
        };
        // **And an empty slug is refused rather than written.** A suffix
        // with no letters or digits — `"X - ???"` — slugifies to `""`, and
        // `edition.key` is `TEXT NOT NULL`: SQLite takes an empty string
        // happily, and a reader would see an edition with no name at all.
        let slug = slugify(suffix);
        if slug.is_empty() {
            return Ok(Offer::Refused(format!(
                "proposal {id}: the suffix \"{suffix}\" has no letters or digits to make an \
                 edition key from. Write the rule by hand."
            )));
        }
        let implied = matched_edition_word(suffix).map(implied_edition_kind);
        (Some(slug), implied.map(str::to_owned))
    } else {
        (None, None)
    };

    Ok(Offer::Made(ProposedRule {
        root_id: Some(root_id),
        kind: RULE_KIND_PREFIX.to_string(),
        pattern,
        match_on: MATCH_ON_PATH.to_string(),
        set_series: Some(target_title.clone()),
        set_edition_key,
        set_kind,
        // Never derived. See `implied_edition_kind`'s own comment.
        set_language: None,
        note: format!("accepted from proposal {id}: \"{subject_title}\" is \"{target_title}\""),
    }))
}

/// Accept proposal `id` by writing exactly `rule`.
///
/// **The guards are not about who chose the pattern.** `accept` derived its
/// own rule so that two measured failures could not happen — a pattern that
/// matched nothing, and a pattern that reached an unrelated series. Both are
/// properties of the *pattern*, so both are checked here regardless of where
/// it came from, and a widened pattern is checked harder rather than less —
/// or, where the guards cannot evaluate its semantics at all, refused rather
/// than waved through.
///
/// Answers [`Offer::Made`] with the written rule's id. Answers
/// [`Offer::Refused`], with nothing written, if the proposal is not `open`,
/// if it is `attach-chapters`, if it has no target series, if its two series
/// are in different libraries or one of the two is in none — what
/// [`acceptable_proposal`] refuses on — if `rule`'s `kind` or `match_on` is
/// not one `rules.rs` knows or its pattern will not compile, if `rule`
/// reaches a series no proposal it answers named — naming it — if it reaches
/// the target it is being accepted into, if `rule` matches no `scan_entry` at
/// all, if what it matches includes nothing of this proposal's own subject,
/// if nothing it matches would change series — in any of which three cases it
/// is rolled back rather than left deciding nothing — or if this proposal, or
/// one of the others the same rule answers, stopped being `open` between the
/// dry run and the write.
///
/// # Errors
///
/// If `id` names no proposal — [`lookup`] tells that apart — or if a read or
/// a write fails, the dry run's reads included. A refusal is not an error:
/// see [`Offer`] for what a caller answered while it was one.
pub fn accept_with(conn: &Connection, id: i64, rule: &ProposedRule) -> Result<Offer<i64>, String> {
    // The refusals about the *proposal* rather than the rule, made
    // here as well as in `preview` and `offered_rule`: this is a `pub fn`,
    // and C3's accept route hands it a rule out of a request body without
    // any guarantee `preview` was ever called. One helper, three callers —
    // see `acceptable`'s own doc — and `across_libraries` beside it.
    //
    // Its answer is dropped: what the rule would decide, and whose material
    // that is, is `resolves`' question now, and it reads the proposal's own
    // two ends itself.
    if let Offer::Refused(why) = acceptable_proposal(conn, id)? {
        return Ok(Offer::Refused(why));
    }

    // **"Matches something" is not "matches only the subject".** A rule
    // decides for everything it matches, not only the paths that derived it —
    // see [`series_reached_by`]'s own doc for the measured consequence of
    // skipping this and stopping at [`common_directory_pattern`] alone.
    //
    // **This one dry run replaces the two fail-closed refusals that stood
    // here**: a rule whose `kind`/`match_on` was not `prefix`/`path`, and a
    // rule with no `root_id`. Both existed because the guard underneath was
    // `derive::other_series_under_prefix`, which asks
    // `rel_path.starts_with(pattern)` under one named root — handed a regex
    // it prefix-matched the pattern's own *source text*, found nothing, and
    // reported a clean bill of health, and handed a `parent`-matched rule it
    // compared against the whole path the engine would never look at. A guard
    // that cannot evaluate a rule's semantics has to refuse it. `resolves` is
    // built on `rules::Engine`, which is the thing `derive` itself will use,
    // so it evaluates the candidate with whatever semantics its `kind`,
    // `match_on` and `root_id` actually carry — which is what earns the right
    // to stop refusing them.
    //
    // **Except that a rule it cannot evaluate is refused here, before it.**
    // `resolves` makes that check first, but answers it through the same
    // `Err(String)` as the reads that follow — see [`evaluable`] — and this
    // function's own `Err` is kept for a proposal that is not there and a
    // store that failed, neither of them a refusal. Left to `resolves`, a typo
    // in a widened pattern would reach `taiyaku-serve` as a failed store and a
    // person as a 500.
    if let Err(why) = engine_parts(rule) {
        return Ok(Offer::Refused(why));
    }
    let resolution = resolves(conn, id, rule)?;
    if !resolution.collateral.is_empty() {
        return Ok(Offer::Refused(format!(
            "proposal {id}'s rule also decides for {}, so accepting it would move that \
             material too. Narrow the pattern or write the rule by hand.",
            resolution.collateral.join(", ")
        )));
    }
    if !resolution.targets_reached.is_empty() {
        // The target is part of the CHECK — if the rule reaches it,
        // `rules.rs` applies every `set_*` column a matching rule carries, so
        // accepting would stamp `set_edition_key` onto the target's own
        // volumes too and make the base its own colour edition. It is kept
        // out of what `collateral` NAMES: it is the series being accepted
        // into, not collateral, so listing it beside genuinely unrelated
        // material misreads what it is. Hence a refusal of its own, in its
        // own words.
        //
        // **Written for however many targets there are, and it no longer
        // says "directory".** Both were true of the narrow `prefix` rule this
        // message was first written for and of nothing else: a widened rule
        // can be a cross-root regex reaching several targets at once, and the
        // message joined them into one pair of quotes and singular phrasing —
        // *…shared with "A, B, C" itself…* — while the test asserting a
        // substring stayed green.
        let named = resolution
            .targets_reached
            .iter()
            .map(|title| format!("\"{title}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let (itself, those) = if resolution.targets_reached.len() == 1 {
            ("itself", "that series'")
        } else {
            ("themselves", "those series'")
        };
        // **And the consequence names what this rule actually carries.**
        // "would set the edition key" is true of an `edition-of` accept and
        // of nothing else: [`preview`] writes `set_edition_key: None` for
        // every other kind, and the previous round rewrote this sentence
        // around it without noticing. Measured on a `same-series` proposal
        // widened to `^[^/]+/`, whose rule carries `set_series` alone — the
        // refusal named an edition key that is not there. What is true for
        // every rule is the general form: `rules.rs` applies every `set_*`
        // column a matching rule carries to everything it matches, so the
        // target's own volumes get whatever this one sets.
        let would = if rule.set_edition_key.is_some() {
            format!("set the edition key on {those} own volumes too")
        } else {
            format!("apply every column this rule sets to {those} own volumes too")
        };
        return Ok(Offer::Refused(format!(
            "proposal {id}'s rule also reaches {named} {itself} — the series it would be \
             accepted into — so accepting it would {would}. Narrow the pattern or write the \
             rule by hand."
        )));
    }

    // **I5: one transaction for the write, the verify and the state
    // change.** Three separate statements used to run outside any
    // transaction of their own: a crash or a failure between the `INSERT`
    // and the `UPDATE` left a live rule no proposal pointed at while the
    // proposal still read `open` — and a second `accept` on that same
    // proposal then wrote a *second* rule and recorded only that one, the
    // exact state `created_rule_id` exists to prevent, reached by a
    // different road. Wrapped here, a failure anywhere in this block rolls
    // the whole thing back — including the `INSERT` — via `Transaction`'s own
    // `Drop`, which is also why the "matched nothing" branch below no longer
    // deletes the rule by hand: returning without `commit` already undoes it,
    // whether what is returned is an `Err` or a refusal.
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("starting the accept transaction for proposal {id}: {e}"))?;

    // Above the seeded defaults (`003_scanner.sql`'s ordinals 1-4) and above
    // every earlier accept, so an accepted rule always applies after the
    // rules that decide a bare series name in the first place — the layering
    // model's own ordering, applied to rules this module writes rather than
    // ones a person typed.
    let ordinal: i64 = tx
        .query_row("SELECT COALESCE(MAX(ordinal), 4) + 1 FROM rule", [], |r| {
            r.get(0)
        })
        .map_err(|e| format!("choosing an ordinal for the accepted rule: {e}"))?;

    tx.execute(
        "INSERT INTO rule (ordinal, root_id, kind, pattern, match_on, set_series, \
         set_edition_key, set_kind, set_language, note) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            ordinal,
            rule.root_id,
            rule.kind,
            rule.pattern,
            rule.match_on,
            rule.set_series,
            rule.set_edition_key,
            rule.set_kind,
            rule.set_language,
            rule.note,
        ],
    )
    .map_err(|e| format!("writing the rule for proposal {id}: {e}"))?;
    let rule_id = tx.last_insert_rowid();

    // **The load-bearing half.** [`common_directory_pattern`] derives the
    // pattern from `scan_entry` through the rule set as it stands right now,
    // but a pattern correct at derivation time can still match nothing by
    // the time it is checked — and a rule that decides nothing is
    // indistinguishable from a decision that was never made. So this is
    // verified against the raw facts directly rather than trusted from how
    // the pattern was derived.
    //
    // **Counted by the engine, not by a `substr` against `rel_path`.** That
    // comparison was blind in exactly the way the collateral guard above was
    // — it read `rule.pattern` as a literal prefix of the whole path whatever
    // `kind` and `match_on` said, so a `regex` rule was "verified" against
    // its own source text and a `parent`-matched one against a string the
    // engine never looks at. It got the right answer for every pattern
    // probed only because both blindnesses failed the same way. The count
    // now comes from the same sweep the refusals above rest on, which scopes
    // itself to `root_id` (I6) because [`crate::rules::Engine::resolve`]
    // does.
    if resolution.entries_matched == 0 {
        // **No manual `DELETE` here any more.** Returning without
        // `tx.commit()` rolls the whole transaction back — the `INSERT`
        // above included — which is I5's whole point: a rule that decides
        // nothing must never be left standing, and now it structurally
        // cannot be, rather than depending on this branch remembering to
        // clean up after it.
        //
        // `pattern == "/"` is what a subject whose entries sit directly under
        // the root derives — `common_directory_pattern`'s single "directory"
        // is `""`, and no real `rel_path` ever begins with a `/` to match it.
        // Named rather than left as the generic pattern-quoting message
        // below, which reads as a coordinate rather than a reason once the
        // pattern itself is invisible in this way.
        // **Written as a single physical line, deliberately.** `cargo fmt`
        // has a real quirk on a `\`-continued string short enough to want
        // merging onto fewer lines: it does not know that Rust itself strips
        // the leading whitespace after a `\`-newline, so it bakes that
        // whitespace into the string as literal spaces instead of removing
        // it — silent, since fmt never looks inside a string's own content,
        // and unreachable to the gate since nothing there compares string
        // *values*. Caught only by reading the runtime message. One physical
        // line has nothing for that merge to do.
        let why = if rule.pattern == "/" {
            "this series' files sit directly in the root, so a `path` prefix rule cannot narrow to only them".to_string()
        } else {
            format!(
                "the rule this would write (`{}` on {}) matched no scan_entry",
                rule.pattern, rule.match_on
            )
        };
        return Ok(Offer::Refused(format!(
            "proposal {id}: {why} — refusing rather than leaving an accepted proposal whose rule decides nothing"
        )));
    }

    // **"Matches something" is not "matches any of the subject's material".**
    // Exactly that, and no more: `series_reached_by` counts an entry towards
    // `entries_matched` and then drops it from `reach.series` when the live
    // rules give it no series at all — a container directory, or the root's
    // own entry — so a rule reaching only those matches something and reaches
    // nobody's material. Measured on a shared `Publisher/` layout: a `regex`
    // of `^Publisher$` returned `entries_matched: 1` with everything else
    // empty, wrote its rule, marked the proposal `accepted`, and left every
    // `work` row identical across the following `derive` — compared row by
    // row, before and after. `^$` did the same through the root's entry. The
    // question then only comes back through `unaccept`.
    //
    // **What this does not ask is whether anything changes.** It used to say
    // it did, and it did not: `subject_reached` is `reach.series` containing
    // the subject's id, which is "an entry this rule decides a field for
    // belongs to the subject today" and nothing else. A rule that matches the
    // subject's own volume and gives it back the series it already has passes
    // this and moves nothing — the `works_moved == 0` refusal below is what
    // covers that half, and the two are separate because a rule can fail
    // either one alone. Measured on `^Kaguya-sama wa Kokurasetai - Full Color
    // Edition/` handed to the *Go-Toubun* proposal: `works_moved: 1` with
    // `subject_reached: false`, refused here and by nothing below.
    //
    // Ordered *after* the count above deliberately: a rule matching nothing
    // at all fails this too, and "matched no scan_entry" is the more specific
    // thing to say about it.
    if !resolution.subject_reached {
        return Ok(Offer::Refused(format!(
            "proposal {id}: the rule this would write matches {} scan_entry row(s), none of \
             them this proposal's own subject — accepting would take the question out of the \
             inbox without deciding it. Widen the pattern to reach the subject's own material, \
             or write the rule by hand.",
            resolution.entries_matched
        )));
    }

    // **And reaching the subject is not moving it.** `works_moved` is counted
    // from the entries whose resolved series differs from their live one, so
    // zero means no member anywhere changes series: the rule would be written,
    // the proposal would leave the inbox `accepted`, and the next `derive`
    // would produce the library it already has.
    //
    // **Measured, on the motion the form exists for.** Starting from
    // `preview`'s rule on a shared `Publisher/` layout and widening only the
    // pattern to `^Publisher/(?<series>[^/]+ - Full Color Edition)/` — a
    // `series` capture, which `rules.rs` applies *after* `set_series` and so
    // silently overrides it with the colour title the material already has —
    // gave `Resolution { collateral: [], targets_reached: [],
    // entries_matched: 1, subject_reached: true, works_moved: 0 }`,
    // `accept_with` returned `Ok`, the proposal read `accepted`, and every
    // `work` row was identical across the following `derive`, compared row by
    // row. Setting `set_series` to the subject's own title does the same
    // thing without a capture at all.
    //
    // Ordered *after* `subject_reached` for the same reason that one is
    // ordered after the count: a rule reaching only a container directory
    // fails this too, and "none of them this proposal's own subject" is the
    // more specific thing to say about it.
    if resolution.works_moved == 0 {
        return Ok(Offer::Refused(format!(
            "proposal {id}: the rule this would write matches {} scan_entry row(s) and moves \
             none of them — every one already resolves to the series this rule would give it, \
             so accepting would take the question out of the inbox without changing anything. \
             Check `set_series`, and any `series` capture in the pattern, which overrides it.",
            resolution.entries_matched
        )));
    }

    // **`AND state = 'open'`, and the row count is checked.** This project
    // expects two processes on one database file — `serve` and
    // `library import` — and `unchecked_transaction` opens a *deferred*
    // BEGIN: everything `resolves` read, this proposal's state included, was
    // read before any lock was taken. A second process accepting the same
    // proposal in that window would have its `created_rule_id` overwritten by
    // this one, and `unaccept` would then delete only the second rule while
    // the first stayed live, deciding structure nothing points at — the exact
    // state `created_rule_id` exists to prevent. The guard turns that race
    // into a rolled-back refusal.
    let marked = tx
        .execute(
            "UPDATE proposal SET state = ?1, created_rule_id = ?2 WHERE id = ?3 AND state = ?4",
            rusqlite::params![STATE_ACCEPTED, rule_id, id, STATE_OPEN],
        )
        .map_err(|e| format!("marking proposal {id} accepted: {e}"))?;
    if marked != 1 {
        return Ok(Offer::Refused(format!(
            "proposal {id} stopped being open while this accept was working out what it would \
             do — another process has judged it since. Nothing was written; read it again and \
             retry."
        )));
    }

    // **A widened rule answers every proposal it resolves, and each records
    // it.** Leaving them `open` would show a person questions the library has
    // already decided; leaving them `accepted` with a NULL `created_rule_id`
    // would make them un-un-acceptable — and `raise` would reopen them on the
    // next scan, since that is exactly what a NULL there means to it.
    //
    // **Guarded on `state = 'open'` for the same reason, and this is the
    // group the race actually costs something.** `resolution.also_resolves`
    // is read by `resolves`, outside any transaction, from `open`'s own
    // `state = 'open'` filter; the write below then landed unconditionally,
    // so a second process that accepted one of these in between had its
    // `created_rule_id` replaced by this rule's id and its own rule orphaned
    // — live, and pointed at by nothing.
    for other in &resolution.also_resolves {
        let marked = tx
            .execute(
                "UPDATE proposal SET state = ?1, created_rule_id = ?2 WHERE id = ?3 AND state = ?4",
                rusqlite::params![STATE_ACCEPTED, rule_id, other, STATE_OPEN],
            )
            .map_err(|e| format!("recording proposal {other} against rule {rule_id}: {e}"))?;
        if marked != 1 {
            return Ok(Offer::Refused(format!(
                "proposal {other}, which this rule also answers, stopped being open while this \
                 accept was working out what it would do — another process has judged it since. \
                 Nothing was written; read them again and retry."
            )));
        }
    }

    tx.commit()
        .map_err(|e| format!("committing the accept of proposal {id}: {e}"))?;

    Ok(Offer::Made(rule_id))
}

/// Accept proposal `id` with the rule [`preview`] derives for it.
///
/// The narrow default: exactly what this function did before the split, and
/// what `taiyaku library proposals --accept` still calls.
///
/// # Errors
///
/// As [`preview`], and as [`accept_with`] with its refusal returned as the
/// error — the fold [`preview`] makes of its own. That is this function's
/// contract because its one caller outside tests, `taiyaku library proposals
/// --accept`, prints either and exits 1 on it. A caller answering a refusal
/// and a failure differently calls [`accept_with`] itself, as `taiyaku-serve`
/// does.
pub fn accept(conn: &Connection, id: i64) -> Result<i64, String> {
    accept_with(conn, id, &preview(conn, id)?)?.or_refusal()
}

/// What a candidate rule would do, before it does it.
///
/// **One computation, read two ways.** [`Resolution::also_resolves`] is the
/// form's *"resolves 6 more"*; [`Resolution::collateral`] is the measured
/// refusal that stops a `Publisher/` pattern moving somebody else's volumes.
/// They are the same query because the question is the same one: which series
/// does this rule decide for, and which of those did anybody ask about.
///
/// **Every refusal [`accept_with`] makes about the rule itself is a field
/// here.** A form promising "resolves 2 more, nothing collateral" over a rule
/// the write then rejects is the exact disagreement this type exists to
/// prevent, so `targets_reached` and `entries_matched` — the accept's other
/// two refusals — are part of the same answer rather than something the write
/// discovers on its own afterwards.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolution {
    /// Other open proposals this rule would answer. Never includes `id`.
    pub also_resolves: Vec<i64>,
    /// Series this rule decides for that no proposal named. Non-empty is a
    /// refusal, not a warning.
    pub collateral: Vec<String>,
    /// Series this rule decides for that are the *target* of a proposal it
    /// answers — the series being accepted into, so not collateral, and named
    /// apart from it for that reason. Still a refusal: a rule applies every
    /// `set_*` column it carries to everything it matches, so an `edition-of`
    /// rule reaching its own target stamps the colour edition key onto the
    /// base's volumes and makes the base its own colour edition.
    pub targets_reached: Vec<String>,
    /// How many `scan_entry` rows this rule would decide at least one field
    /// for. Zero is a refusal too — a rule that decides nothing is
    /// indistinguishable from a decision nobody made.
    pub entries_matched: usize,
    /// Whether any entry this rule decides a field for **belongs to the
    /// proposal's own subject today** — the material the question is about.
    ///
    /// **That, and not "does the subject change".** The rule may give that
    /// material back the series it already has; this field is still true, and
    /// [`Self::works_moved`] is what says so. False is a refusal, and one
    /// `entries_matched` cannot make on its own: measured on a shared
    /// `Publisher/` layout, a `regex` rule of `^Publisher$` matches the
    /// container directory, counts 1 entry, names no series at all, and was
    /// accepted — writing a rule, marking the proposal `accepted`, and
    /// changing not one `work` row on the next `derive`. The question left
    /// the inbox and nothing decided it, which is exactly what the
    /// `entries_matched == 0` refusal exists to prevent, reached by a
    /// different road. `^$`, which matches the root's own entry, does the
    /// same.
    ///
    /// **It still earns its place beside a `works_moved == 0` refusal**,
    /// because a rule can move material that is not the subject's: measured,
    /// `^Kaguya-sama wa Kokurasetai - Full Color Edition/` handed to the
    /// *Go-Toubun* proposal comes back `subject_reached: false` with
    /// `works_moved: 1` and nothing in `collateral` — the Kaguya subject is
    /// allowed, because the same rule answers its proposal. Accepting would
    /// mark the Go-Toubun question answered by a rule that never looks at it.
    pub subject_reached: bool,
    /// How many `work` rows hold at least one member this rule would give a
    /// different series — for the consequence line, and a refusal at zero.
    ///
    /// **That headline is what it counts, which is not quite "how many `work`
    /// rows would change series" — what it used to claim.** `derive` regroups
    /// after the rule lands, and a work whose members span two roots can shed
    /// one member to another work while keeping its own `series_id`: measured
    /// on exactly that shape, with a root-scoped rule (the kind [`preview`]
    /// writes) matching one of the two members, `works_moved: 1` and *zero*
    /// `work` rows changed `series_id` across the following `derive` —
    /// compared row by row. The number is right about the members; the works
    /// it counts are the works those members are in now.
    ///
    /// **Counted from the members that would actually move, not from every
    /// work in every series the rule reaches.** It was the latter, and the
    /// difference is measurable: a two-volume subject under a regex matching
    /// only `v01` reported `entries_matched: 1` beside `works_moved: 2`. A
    /// later task renders this in a sentence saying what accepting will do,
    /// so an over-count is a false statement to the person deciding. The two
    /// agree exactly on the canonical group path — three colour editions of
    /// one volume each, where every work in reach is also a work that moves —
    /// which is why nothing noticed.
    ///
    /// Zero is a refusal: see [`accept_with`]'s own comment for the widened
    /// pattern that reached its subject, decided a field for it, and handed
    /// it back the title it already had.
    pub works_moved: usize,
}

/// What `rule` would resolve if accepted for proposal `id`.
///
/// Built on [`crate::rules::Engine`] rather than on a second matcher: the
/// engine is what `derive` will actually use, so a dry run that agreed with a
/// reimplementation and disagreed with the engine would be worse than none.
///
/// **What "resolves" requires, and why it is more than "reaches".** Another
/// proposal is answered only if this rule would give its subject's material
/// its own target's title. Reaching that material is not enough: a rule can
/// reach a series and move it somewhere nobody asked for, and counting that
/// as an answer would put the series into the allowed set and take it back
/// out of [`Resolution::collateral`] — the guard disarming itself on the
/// strength of a proposal it is not actually answering. It also keeps
/// `attach-chapters` out of the group, which carries no target and which
/// [`accept_with`] refuses by name; sweeping one into an accept would mark it
/// answered by a rule that cannot answer it.
///
/// **One `Err` for two different things**, and [`evaluable`] is how a caller
/// separates them: a rule this cannot evaluate and a read that failed both
/// arrive as an `Err(String)` here, so a caller answering them differently
/// asks [`evaluable`] first rather than reading the sentence.
///
/// # Errors
///
/// If `id` names no proposal, if the rule's `kind` or `match_on` is not one
/// `rules.rs` knows, if the rule is not a valid `regex` when it says it is, or
/// if the reads against `scan_entry`, `rule` or `series` fail.
pub fn resolves(conn: &Connection, id: i64, rule: &ProposedRule) -> Result<Resolution, String> {
    let (subject_id, target_id): (i64, Option<i64>) = conn
        .query_row(
            "SELECT subject_id, target_id FROM proposal WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| format!("looking up proposal {id}: {e}"))?;

    // Every series this rule would decide for, by resolving every scan entry
    // through the live rule set with the candidate appended to it.
    let reach = series_reached_by(conn, rule)?;

    // The legitimate set: this proposal's two ends, plus both ends of every
    // other open proposal the same rule answers.
    let mut also_resolves = Vec::new();
    let mut allowed: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();
    let mut targets: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();
    allowed.insert(subject_id);
    if let Some(target_id) = target_id {
        allowed.insert(target_id);
        targets.insert(target_id);
    }
    for other in open(conn, 0.0)? {
        if other.id == id || !answers(conn, &reach, &other)? {
            continue;
        }
        also_resolves.push(other.id);
        allowed.insert(other.subject_id);
        if let Some(other_target) = other.target_id {
            allowed.insert(other_target);
            targets.insert(other_target);
        }
    }

    let collateral = reach
        .series
        .iter()
        .filter(|(series_id, _)| !allowed.contains(series_id))
        .map(|(_, reached)| reached.title.clone())
        .collect();
    let targets_reached = reach
        .series
        .iter()
        .filter(|(series_id, _)| targets.contains(series_id))
        .map(|(_, reached)| reached.title.clone())
        .collect();
    // Keyed on the series the library records *now*, which is what
    // `reach.series` is keyed on: "does this rule decide anything about the
    // material this proposal is about".
    let subject_reached = reach.series.contains_key(&subject_id);
    let works_moved = works_moved(conn, &reach.moved_members)?;

    Ok(Resolution {
        also_resolves,
        collateral,
        targets_reached,
        entries_matched: reach.entries_matched,
        subject_reached,
        works_moved,
    })
}

/// Whether the rule behind `reach` answers `other` — reaches its subject's
/// material *and* gives that material `other`'s own target title.
///
/// See [`resolves`]'s own doc for why reaching alone is not enough.
///
/// **Never a proposal [`across_libraries`] refuses.** [`accept_with`] marks
/// every proposal this answers accepted as one of a group, and would refuse
/// that one asked about on its own.
fn answers(conn: &Connection, reach: &Reach, other: &Proposal) -> Result<bool, String> {
    let (Some(reached), Some(target_id)) = (reach.series.get(&other.subject_id), other.target_id)
    else {
        return Ok(false);
    };
    if across_libraries(conn, other, target_id)?.is_some() {
        return Ok(false);
    }
    let target_title = series_title(conn, target_id)?;
    Ok(reached.becomes.len() == 1 && reached.becomes.contains(&target_title))
}

/// A `set_edition_key`-safe token from `s`: lowercase, with every run of
/// non-alphanumeric characters collapsed to one `-` and none leading or
/// trailing. `"Full Color Edition"` becomes `"full-color-edition"`, the same
/// string [`raise_edition_of`]'s own `reason` already quotes, so the rule an
/// accept writes says what the proposal said.
fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut last_was_dash = true; // suppresses a leading `-`
    for c in s.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            out.push('-');
            last_was_dash = true;
        }
    }
    if out.ends_with('-') {
        out.pop();
    }
    out
}

/// The `prefix` pattern [`accept`] would write for a subject whose entries
/// resolve to `paths` — the single directory every one of them sits under,
/// plus a trailing `/` — and the single root that directory sits under (I6).
///
/// `paths` is [`crate::derive::rel_paths_for_series`]' answer for the
/// subject's title in the subject's library, swept by the caller: see
/// [`offered_rule`] for why it is not read here.
///
/// # Errors
///
/// If a root cannot be read. A subject this cannot derive a pattern for is
/// [`Offer::Refused`] naming `proposal_id`, not an error: its entries do not
/// share exactly one directory — none at all (nothing has been scanned for
/// this series), or more than one, in which case every directory found is
/// named, the refusal [`preview`]'s own doc lists as "a subject whose entries
/// share no single directory" — or that one directory is shared by more than
/// one root (two roots each holding their own directory of that name, say),
/// where there is no single root to scope the written rule to and picking one
/// arbitrarily would leave the pattern silently deciding nothing for the
/// entries under the other.
fn common_directory_pattern(
    conn: &Connection,
    proposal_id: i64,
    paths: &[(i64, String)],
) -> Result<Offer<(String, i64)>, String> {
    let dirs: std::collections::BTreeSet<&str> = paths
        .iter()
        .map(|(_, p)| crate::derive::parent_of(p))
        .collect();
    if dirs.len() != 1 {
        let listed = if dirs.is_empty() {
            "none — nothing has been scanned for this series yet".to_string()
        } else {
            dirs.into_iter().collect::<Vec<_>>().join(", ")
        };
        return Ok(Offer::Refused(format!(
            "proposal {proposal_id}'s subject spans more than one directory, so accepting it \
             cannot be expressed as one prefix rule. Its entries are under: {listed}"
        )));
    }
    let dir = dirs.into_iter().next().expect("checked len == 1 above");

    // **I6: the rule this writes is scoped to one root, or it is not scoped
    // at all.** `rule.root_id` NULL means "every root, forever" — including a
    // root added after this accept, sharing nothing with the subject beyond a
    // directory name that happens to match. [`series_reached_by`] only ever
    // looks at the roots this library holds *right now*; a NULL `root_id`
    // outlives that check.
    //
    // Which is a reason for the *narrow* rule this function derives to name
    // its root, not a refusal of every unscoped rule: `accept_with` takes one
    // now, because the sweep behind it evaluates a candidate through the same
    // engine `derive` uses and so covers every root the library holds. What
    // it cannot cover is a root nobody has added yet, and that is a decision
    // for whoever deliberately widens a rule to every root — not something
    // this narrow default should choose on their behalf.
    let root_ids: std::collections::BTreeSet<i64> = paths.iter().map(|(r, _)| *r).collect();
    if root_ids.len() != 1 {
        let listed = root_paths(conn, &root_ids)?
            .into_values()
            .collect::<Vec<_>>()
            .join(", ");
        return Ok(Offer::Refused(format!(
            "proposal {proposal_id}'s subject sits under `{dir}/` in more than one root, so \
             accepting it cannot be scoped to a single root. Its roots are: {listed}"
        )));
    }
    let root_id = root_ids.into_iter().next().expect("checked len == 1 above");

    Ok(Offer::Made((format!("{dir}/"), root_id)))
}

/// One series a candidate rule reaches, and what it would make of it.
struct Reached {
    /// The series' `sort_title`, as the library records it now.
    title: String,
    /// Every series title the candidate would give this series' material —
    /// one, for any rule worth accepting. Its own title back means the rule
    /// touches this series without moving it.
    becomes: std::collections::BTreeSet<String>,
}

/// What a candidate rule reaches: the series whose material it decides for,
/// and how many `scan_entry` rows that is.
struct Reach {
    /// Keyed by `series.id` **as the library records it now** — the series
    /// whose material the rule would take, not the one it would give it to.
    /// That is the whole of the collateral question: "whose volumes move".
    series: BTreeMap<i64, Reached>,
    entries_matched: usize,
    /// The absolute path of every entry the candidate would give a *different*
    /// series from the one it resolves to today — `member.path`'s own spelling
    /// (`root.path` joined with `rel_path`, exactly as `derive`'s `Candidate`
    /// builds it), so [`works_moved`] can turn them into the `work` rows that
    /// would actually change hands.
    ///
    /// **Per entry rather than per series, because that is the difference the
    /// number is wrong by.** A rule matching one volume of a two-volume series
    /// reaches that series, and a count taken at series granularity then
    /// reports both volumes moving.
    moved_members: std::collections::BTreeSet<String>,
}

/// The parts of a candidate rule the engine needs, or the reason it cannot be
/// evaluated at all.
///
/// **A question about the rule and nothing else** — it reads no table, so
/// every answer it gives is about the text it was handed. That is what lets
/// [`evaluable`] be offered to a caller which has to answer a bad rule and a
/// broken database differently: both go through here, so there is no second
/// copy of the check to drift out of step.
///
/// **A pattern that will not compile is an error here, not a complaint.**
/// `Engine::compile` disables such a rule and carries on, which is right for
/// a scan — one bad rule must not kill a library. It is wrong for a dry run:
/// "resolves nothing" and "your regex has a typo" would render identically in
/// the form, and a person would widen a pattern that does nothing and accept
/// it. Checked on the candidate alone, rather than by reading `compile`'s
/// complaint list back and matching the candidate's sentinel id out of it —
/// that would couple this to a message format, and this way the caller gets
/// `regex`'s own error.
fn engine_parts(
    rule: &ProposedRule,
) -> Result<(crate::rules::RuleKind, crate::rules::MatchOn), String> {
    let kind = rule
        .kind
        .parse::<crate::rules::RuleKind>()
        .map_err(|e| format!("this rule cannot be checked: {e}"))?;
    let match_on = rule
        .match_on
        .parse::<crate::rules::MatchOn>()
        .map_err(|e| format!("this rule cannot be checked: {e}"))?;
    if kind == crate::rules::RuleKind::Regex {
        regex::Regex::new(&rule.pattern)
            .map_err(|e| format!("this pattern will not compile: {e}"))?;
    }
    Ok((kind, match_on))
}

/// Whether the engine could evaluate `rule` at all, asked without reading
/// anything.
///
/// **So that a caller can tell its own bad input from this library's state.**
/// [`resolves`] answers both through one `Err(String)`: a rule it cannot
/// compile, and a read that failed underneath it. `taiyaku-serve`'s dry-run
/// route mapped that single `Err` to a 400 and so told a client to fix its
/// rule while the library's `scan_entry` table was gone — measured, at the
/// route. Asking this first leaves [`resolves`]'s own `Err` meaning a read
/// that failed, because this is the check [`series_reached_by`] itself makes
/// and it makes it before it touches the store.
///
/// # Errors
///
/// Returns an error if the rule's `kind` or `match_on` is not one `rules.rs`
/// knows, or if it claims to be a `regex` and will not compile.
pub fn evaluable(rule: &ProposedRule) -> Result<(), String> {
    engine_parts(rule).map(|_| ())
}

/// Every series `rule` would decide for, and how much it matches.
///
/// **The candidate joins the live rule set and the real engine compiles it.**
/// A dry run built on a second matcher could agree with itself and disagree
/// with `derive`, which is the one thing it must not do: this answer is what
/// [`accept_with`] refuses on and what the form promises. It is also what
/// lets the answer be right for a `regex` rule, a `name`- or `parent`-matched
/// rule and an every-root rule alike — the three shapes a literal-prefix
/// guard could only fail closed on.
///
/// **The bug this is the guard for, measured.** A colour-edition proposal
/// whose subject sat under a shared `Publisher/` directory derived the
/// pattern `Publisher/`, which was correct about the subject and also
/// matched every other series under `Publisher/`: their volumes were
/// silently re-derived into the accepted series' page sequence, and
/// `write_work`'s own doc records that member order *is* page order and every
/// alignment offset is computed from it. A rule reaching too much looks like
/// a decision, which is what makes it worse than one reaching nothing.
///
/// **Two engines, and the difference between them is the answer.** The
/// widened engine says which entries the candidate decides a field for — by
/// [`CANDIDATE_RULE_ID`] appearing in the resolution's provenance, which is
/// the engine's own record of who decided what rather than a second reading
/// of the pattern. The live engine says which series each of those entries
/// belongs to *today*, which is the series a person would see move: the one
/// of that name in the entry's own library. An entry
/// no rule gives a series to, or a series name no `series` row carries — a
/// container directory resolving to its publisher's name, say — is reached
/// but has nothing to lose, so it counts towards `entries_matched` and
/// towards no series. That gap is why [`Resolution::subject_reached`] exists:
/// a rule reaching *only* such entries matches something and decides nothing,
/// and `entries_matched` alone cannot tell [`accept_with`] so.
///
/// # Errors
///
/// If the rule's `kind` or `match_on` is not one `rules.rs` knows, if it
/// claims to be a `regex` and will not compile, or if the rules, roots or
/// scan entries cannot be read.
fn series_reached_by(conn: &Connection, rule: &ProposedRule) -> Result<Reach, String> {
    // Before the first read, so that a caller which has already asked
    // `evaluable` knows every `?` below this line is a read that failed.
    let (kind, match_on) = engine_parts(rule)?;
    let (rules, _complaints) = crate::rules::read(conn)?;

    let (live, _complaints) = crate::rules::Engine::compile(&rules);
    let mut widened_rules = rules;
    widened_rules.push(crate::rules::Rule {
        id: CANDIDATE_RULE_ID,
        // Above every existing rule, which is where `accept_with` writes it —
        // a dry run that resolved the candidate at a different ordinal than
        // the accept would use is answering a different question.
        ordinal: i64::MAX,
        enabled: true,
        root_id: rule.root_id,
        kind,
        pattern: rule.pattern.clone(),
        match_on,
        set_edition_key: rule.set_edition_key.clone(),
        set_kind: rule.set_kind.clone(),
        set_language: rule.set_language.clone(),
        set_series: rule.set_series.clone(),
        note: Some(rule.note.clone()),
    });
    let (widened, _complaints) = crate::rules::Engine::compile(&widened_rules);

    let mut reach = Reach {
        series: BTreeMap::new(),
        entries_matched: 0,
        moved_members: std::collections::BTreeSet::new(),
    };

    // **Prepared once, not once per root, and the series lookup memoised.**
    // This is the endpoint C3's rule-editing form calls on every keystroke,
    // and both of these were per-iteration round trips inside the entry loop:
    // one `SELECT rel_path` re-prepared for each root, and one
    // `series_id_by_sort_title` for each *matched entry* — a widened rule
    // matching a whole library's worth of entries paid a query per entry to
    // ask the same handful of questions over and over. Structural, not
    // speculative: the answers are identical, only the number of statements
    // is not.
    let roots = crate::scan::roots(conn)?;
    let mut stmt = conn
        .prepare("SELECT rel_path FROM scan_entry WHERE root_id = ?1 ORDER BY rel_path")
        .map_err(|e| format!("reading scan entries for the dry run: {e}"))?;
    // Keyed by the entry's library as well as the name: an entry's series is
    // the one of that name in the entry's own library.
    let mut series_ids: BTreeMap<(i64, String), Option<i64>> = BTreeMap::new();

    for root in &roots {
        let rows = stmt
            .query_map([root.id], |r| r.get::<_, String>(0))
            .map_err(|e| format!("reading scan entries for the dry run: {e}"))?;
        for row in rows {
            let rel_path = row.map_err(|e| format!("reading a scan entry: {e}"))?;
            let resolved = widened.resolve(root, &rel_path);
            if !resolved
                .provenance
                .values()
                .any(|decided_by| *decided_by == CANDIDATE_RULE_ID)
            {
                continue;
            }
            reach.entries_matched += 1;
            let Some(current) = live.resolve(root, &rel_path).series else {
                continue;
            };
            // Recorded before the series row is looked up, because "would
            // this entry's material change series" is a question about the
            // two resolutions and nothing else.
            if resolved.series.as_deref() != Some(current.as_str()) {
                reach
                    .moved_members
                    .insert(root.path.join(&rel_path).to_string_lossy().into_owned());
            }
            let asked = (root.library_id, current.clone());
            let looked_up = match series_ids.get(&asked) {
                Some(hit) => *hit,
                None => {
                    let found = series_id_by_sort_title(conn, Some(root.library_id), &current)?;
                    series_ids.insert(asked, found);
                    found
                }
            };
            let Some(series_id) = looked_up else {
                continue;
            };
            let reached = reach.series.entry(series_id).or_insert_with(|| Reached {
                title: current,
                becomes: std::collections::BTreeSet::new(),
            });
            if let Some(becomes) = resolved.series {
                reached.becomes.insert(becomes);
            }
        }
    }
    Ok(reach)
}

/// How many `work` rows hold at least one of `members` — the works whose
/// series [`Reach::moved_members`] says would change.
///
/// **One scan of `member`, not one query per path.** There is no index on
/// `member.path`, so a lookup per path would be a table scan per path; the
/// intersection is done in memory instead, on the same interactive path
/// [`series_reached_by`]'s own memo exists for.
fn works_moved(
    conn: &Connection,
    members: &std::collections::BTreeSet<String>,
) -> Result<usize, String> {
    if members.is_empty() {
        return Ok(0);
    }
    let mut stmt = conn
        .prepare("SELECT m.path, e.work_id FROM member m JOIN edition e ON e.id = m.edition_id")
        .map_err(|e| format!("counting the works this rule would move: {e}"))?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
        .map_err(|e| format!("counting the works this rule would move: {e}"))?;
    let mut works: std::collections::BTreeSet<i64> = std::collections::BTreeSet::new();
    for row in rows {
        let (path, work_id) = row.map_err(|e| format!("reading a member row: {e}"))?;
        if members.contains(&path) {
            works.insert(work_id);
        }
    }
    Ok(works.len())
}

/// The `root.path` of every id in `ids`, keyed by the id.
///
/// **Keyed rather than listed**, because [`material_of`] joins each entry's
/// directory onto *its own* root and a positional list cannot say which is
/// which. [`common_directory_pattern`]'s refusal message wants only the
/// names — named roots read better than a set of surrogate keys nobody typed
/// — and takes the values.
fn root_paths(
    conn: &Connection,
    ids: &std::collections::BTreeSet<i64>,
) -> Result<BTreeMap<i64, String>, String> {
    ids.iter()
        .map(|id| {
            conn.query_row("SELECT path FROM root WHERE id = ?1", [id], |r| r.get(0))
                .map(|path: String| (*id, path))
                .map_err(|e| format!("looking up root {id}: {e}"))
        })
        .collect()
}

/// Undo [`accept`]: delete the rule it wrote and reopen every proposal that
/// rule answered.
///
/// Answers [`Offer::Refused`], with nothing written, if that proposal has no
/// `created_rule_id` — never accepted, or already un-accepted.
///
/// # Errors
///
/// Returns an error if `id` names no proposal — [`lookup`] tells that apart —
/// or if the read, the update or the delete fails.
pub fn unaccept(conn: &Connection, id: i64) -> Result<Offer<()>, String> {
    // Through [`one`] rather than a `SELECT created_rule_id` of its own, so
    // that an `id` naming no proposal reads the same sentence here, in
    // [`dismiss`] and in [`accept_with`] — and so that [`lookup`]'s split
    // between a missing row and a failed read covers all three.
    let Some(rule_id) = one(conn, id)?.created_rule_id else {
        return Ok(Offer::Refused(format!(
            "proposal {id} has no accepted rule to remove"
        )));
    };

    // **One transaction, the same argument I5 makes about `accept_with`.**
    // Un-accepting is now two writes that mean one thing — the rule goes and
    // everything it answered comes back — and half of it is a worse state
    // than neither half: the rule deleted with the group still `accepted`
    // waits for the next `raise` to repair it, and the group reopened with
    // the rule still live shows a person questions the library is still
    // deciding.
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("starting the un-accept transaction for proposal {id}: {e}"))?;

    // **Keyed on the rule, not on the id, and run before the `DELETE`.** One
    // rule can answer many proposals — see `accept_with`'s group write — and
    // un-accepting is the undo of the accept that wrote it, so it reopens
    // everything that accept closed, not only the row the caller happened to
    // name. That has to happen while `created_rule_id` still says which rows
    // those are: `rule`'s deletion sets the column to NULL through
    // `ON DELETE SET NULL` (`schema/003_scanner.sql`), so a delete-then-update
    // keyed on it would match nothing at all. The column's `ON DELETE` half
    // is not enough on its own anyway — it never touches `state`, which is
    // `raise`'s job for a rule deleted some *other* way (C4's editor), and
    // un-accepting must not wait for the next raise to take effect.
    tx.execute(
        "UPDATE proposal SET state = ?1, created_rule_id = NULL WHERE created_rule_id = ?2",
        rusqlite::params![STATE_OPEN, rule_id],
    )
    .map_err(|e| format!("reopening the proposals rule {rule_id} answered: {e}"))?;
    tx.execute("DELETE FROM rule WHERE id = ?1", [rule_id])
        .map_err(|e| format!("removing the rule proposal {id} wrote: {e}"))?;

    tx.commit()
        .map_err(|e| format!("committing the un-accept of proposal {id}: {e}"))?;
    Ok(Offer::Made(()))
}

/// Dismiss proposal `id`. Confirms the outcome doing nothing already has —
/// the subject stays its own series — rather than changing anything, so it
/// writes no rule. §6's first argument: this state has to persist, or
/// [`raise`] re-raises the same proposal on every rescan.
///
/// **Refuses an accepted proposal rather than un-accepting it silently.**
/// Without this, dismissing an accepted proposal would leave its rule alive
/// under a `dismissed` label: the proposal drops out of `open`,
/// `reopen_orphaned_accepts` only ever looks at `state = 'accepted'` so it
/// never notices, and the rule keeps deciding structure that nothing points
/// at any more. A command whose name says it only changes a label must not
/// also delete a rule out from under a decision — [`unaccept`] is the
/// command that does that, explicitly.
///
/// **Nor dismisses a proposal that still holds a rule.** An accept is what
/// writes `created_rule_id`, so a row holding one that is not `accepted` is
/// already a contradiction, and a dismissal on top of it would hide the rule
/// under a label that says no rule was written.
///
/// **One conditional `UPDATE` in a transaction, then a read only to word a
/// refusal — [`reopen`]'s shape, for [`reopen`]'s reason.** Until O72's fix
/// round this read the state first and then wrote unconditionally, so an
/// accept committing between the two left a `dismissed` row holding a live
/// `created_rule_id`: the state this refusal exists to prevent, reached
/// through the gap in front of it. That race predates O72. Reopening is what
/// made it matter twice, since a reopened row with a live rule is one
/// [`acceptable`] lets be accepted again, beside the rule still in force.
///
/// Answers [`Offer::Refused`], with nothing written, if it is currently
/// `accepted` — un-accept it first — or still holds a rule.
///
/// # Errors
///
/// Returns an error if `id` names no proposal — [`lookup`] tells that apart —
/// or if the update, the read or the transaction fails.
pub fn dismiss(conn: &Connection, id: i64) -> Result<Offer<()>, String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("starting the dismissal of proposal {id}: {e}"))?;
    let dismissed = tx
        .execute(
            "UPDATE proposal SET state = ?1 \
              WHERE id = ?2 AND state <> ?3 AND created_rule_id IS NULL",
            rusqlite::params![STATE_DISMISSED, id, STATE_ACCEPTED],
        )
        .map_err(|e| format!("dismissing proposal {id}: {e}"))?;
    let answer = if dismissed == 1 {
        Offer::Made(())
    } else {
        // Through [`one`], for the reason [`unaccept`] reads that way.
        let proposal = one(&tx, id)?;
        Offer::Refused(match (proposal.state.as_str(), proposal.created_rule_id) {
            (STATE_ACCEPTED, Some(_)) => {
                format!("proposal {id} is accepted — un-accept it first, then dismiss")
            }
            (STATE_ACCEPTED, None) => format!(
                "proposal {id} is accepted, but the rule it wrote has gone — the next scan \
                 reopens it, and it can be dismissed then"
            ),
            (state, Some(rule)) => format!(
                "proposal {id} is `{state}` but still holds rule {rule}, which an accept wrote \
                 — un-accept it first, then dismiss"
            ),
            // The transaction took the write lock at `BEGIN` and holds it to
            // the commit below, so no other process can have changed the row
            // between the `UPDATE` and this read: not reachable today, and
            // worded for the day something reads outside that lock.
            (state, None) => format!(
                "proposal {id} changed while it was being dismissed and is `{state}` now — ask \
                 again"
            ),
        })
    };
    tx.commit()
        .map_err(|e| format!("committing the dismissal of proposal {id}: {e}"))?;
    Ok(answer)
}

/// Reopen proposal `id` after a dismissal — the undo of [`dismiss`].
///
/// Spec C3 §4: "a dismissal you disagree with later is a thing you have to be
/// able to find", and finding one is only half of disagreeing with it (O72).
/// **A state change is the whole undo**, for the reason [`dismiss`] gives for
/// being one: a dismissal wrote no rule, so nothing a scan derived from it is
/// left to withdraw. That is what [`unaccept`] cannot say about an accept
/// (O74), and why an accepted proposal is refused here rather than treated as
/// one more way back to `open`.
///
/// Once open again it is an ordinary open proposal: the next [`raise`]
/// refreshes its score and reason from the facts as they are then, and
/// `proposal_identity` keeps that pass from inserting a second row beside it.
///
/// **One conditional `UPDATE`, then a read only to word a refusal.** Reading
/// the state first and writing after would let a second process accept the
/// proposal in between and have this flip an `accepted` row with a live
/// `created_rule_id` back to `open` — the state [`dismiss`]'s own refusal
/// exists to prevent from the other side. The `WHERE` carries the state, so
/// nothing but a dismissal is ever rewritten.
///
/// **And no dismissal still holding a rule (O72's fix round).** [`dismiss`]
/// read, then wrote unconditionally, so an accept committing between the two
/// left a `dismissed` row with a live `created_rule_id`. Reopened, that row
/// would be `open` with its rule in force, and [`acceptable`] would let it be
/// accepted a second time, writing a second rule beside the first. The
/// refusal names [`unaccept`], which removes the rule and reopens the
/// proposal in one step.
///
/// **In one transaction, so the refusal is about the row the `UPDATE`
/// missed.** Every connection this crate hands out opens a transaction with
/// `BEGIN IMMEDIATE` (`Library::configure`), so the write lock is taken before
/// the `UPDATE` and held to the commit, and another process cannot change the
/// row between it and the read that words the refusal: without that, a row
/// dismissed in the gap read "is `dismissed`, not dismissed".
///
/// Answers [`Offer::Refused`], with nothing written, unless the proposal is
/// currently `dismissed` and holds no rule.
///
/// # Errors
///
/// Returns an error if `id` names no proposal — [`lookup`] tells that apart —
/// or if the update, the read or the transaction fails.
pub fn reopen(conn: &Connection, id: i64) -> Result<Offer<()>, String> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("starting the reopen of proposal {id}: {e}"))?;
    let reopened = tx
        .execute(
            "UPDATE proposal SET state = ?1 \
              WHERE id = ?2 AND state = ?3 AND created_rule_id IS NULL",
            rusqlite::params![STATE_OPEN, id, STATE_DISMISSED],
        )
        .map_err(|e| format!("reopening proposal {id}: {e}"))?;
    let answer = if reopened == 1 {
        Offer::Made(())
    } else {
        // Through [`one`], for the reason [`unaccept`] reads that way — and it
        // is what turns an `id` naming no proposal into the same `Err` as there.
        let proposal = one(&tx, id)?;
        Offer::Refused(match (proposal.state.as_str(), proposal.created_rule_id) {
            (STATE_ACCEPTED, Some(_)) => format!(
                "proposal {id} is accepted, not dismissed — un-accepting it is what reopens it"
            ),
            (STATE_ACCEPTED, None) => format!(
                "proposal {id} is accepted, but the rule it wrote has gone — the next scan \
                 reopens it"
            ),
            (state, Some(rule)) => format!(
                "proposal {id} is `{state}` but still holds rule {rule}, which an accept wrote \
                 — un-accepting it removes that rule and reopens it"
            ),
            // Not reachable while the transaction holds the write lock from
            // `BEGIN` to the commit — see this function's doc — and worded
            // for the day it does not.
            (STATE_DISMISSED, None) => {
                format!("proposal {id} changed while it was being reopened — ask again")
            }
            (state, None) => format!(
                "proposal {id} is `{state}`, not dismissed — only a dismissal can be reopened"
            ),
        })
    };
    tx.commit()
        .map_err(|e| format!("committing the reopen of proposal {id}: {e}"))?;
    Ok(answer)
}

// --- evidence ----------------------------------------------------------------

/// Volume 1 of a side, and the pages the evidence panel draws from it.
///
/// **A slug and an edition key rather than image bytes.** C1 already ships
/// `/api/works/{work}/{edition}/pages/{index}` and its `/thumb` sibling, so
/// the panel builds its URLs against routes that exist rather than this
/// module growing an image route of its own.
#[derive(Debug, Clone, PartialEq)]
pub struct FirstVolume {
    pub work_slug: String,
    /// The `work.hub` edition's key. The hub is the edition `content_key` is
    /// computed over (`schema/001_initial.sql`), so it is the one a
    /// comparison between two sides should be drawing pages from.
    pub edition_key: String,
    /// That edition's whole page sequence, summed over its members, which is
    /// the shape `/api/works/{work}/{edition}/pages/{index}` takes: a flat
    /// index that `taiyaku-serve`'s `Served::page` turns into a member and
    /// an offset within it. `0` for a hub edition holding no members.
    pub page_count: i64,
    /// Every distinct format across that edition's members — `zip`, `dir`,
    /// … — ascending, and empty where none of its members was walked from a
    /// root this library still holds.
    ///
    /// **Edition-wide, like [`Self::page_count`] above it.** Nothing
    /// constrains an edition's members to share a format — one volume held
    /// as a directory of pages *and* as a `.cbz` of the same volume is one
    /// edition with two members and two tokens — and a single member's
    /// format printed beside a page count summed over all of them would be
    /// two adjacent fields answering different questions.
    ///
    /// **Read from `scan_entry`, never from `member.format`.** See
    /// [`first_volume`]'s own comment: nothing in this repository writes
    /// that column.
    pub formats: Vec<String>,
    /// Two interior page indices into [`Self::page_count`], or fewer where
    /// the edition is too short to have two distinct ones.
    ///
    /// **Fractions of the page count, not fixed numbers.** C3 §2's argument
    /// is that a colour edition's *cover* is a different image while an
    /// interior page is the same panel layout — so the indices have to land
    /// inside every member they are asked for. The mockup's `p012`/`p047`
    /// are illustrative; hard-coding them renders a broken panel for a
    /// three-page member, which §8 forbids.
    pub interior: Vec<i64>,
}

/// One end of a proposal, as the expanded row draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct Side {
    pub series_id: i64,
    pub title: String,
    /// Every directory this side's material sits in, each joined onto the
    /// root it is under, distinct and ascending — C3 §2's "the full path on
    /// disk, verbatim". One element for the ordinary case, several where
    /// this series' entries span directories or roots, and **empty where
    /// nothing in the series' library currently resolves to this series'
    /// title**, as nothing does for a series in no library — which is not
    /// only "never scanned": an edited rule, or a series a document minted
    /// under a title no rule resolves, can leave a series with
    /// `volume_count: 1` and a real `first_volume` while nothing in the live
    /// rule set resolves to it any more.
    ///
    /// **A list, because a count is not a path.** This was one `String`, and
    /// a side spanning two directories rendered as `"{root} (2 directories)"`
    /// — prose in a field the client puts inside a `<code>` element, naming
    /// something that does not exist on disk and cannot be copied. The
    /// client's only options were to render the sentence or to parse it back
    /// apart, and re-parsing a store-side sentence is what [`Reason`]'s own
    /// doc comment refuses to make anyone do. The empty case carried two
    /// meanings as well — `""` was both "nothing scanned" and "a path that
    /// is empty" — and an empty list carries one.
    ///
    /// **Not [`common_directory_pattern`], which computes the same fact for
    /// a different purpose.** That function *errors* when a series' entries
    /// span more than one directory — correctly, since an accept then cannot
    /// be expressed as one prefix rule — but evidence is shown for *both*
    /// sides of a proposal and only the subject is ever asked to be
    /// acceptable. Built on it, this whole request would fail for a
    /// perfectly ordinary target whose volumes sit under two publisher
    /// directories. A side that spans two directories is a fact worth
    /// showing, not an error, so this goes to that function's own source —
    /// [`crate::derive::rel_paths_for_series`] — instead.
    pub directories: Vec<String>,
    /// Every `work` row against this series, whether or not it carries a
    /// volume number: [`volume_count`], the same count §6's equal-volumes
    /// signal is scored from, so the expanded row and the score beside it
    /// are reading one number.
    pub volume_count: usize,
    /// The volume numbers those works carry, ascending — the list C3 §2 asks
    /// for so that "11 against 14" is visible rather than summarised.
    /// Shorter than [`Self::volume_count`] where a work carries no number.
    pub volumes: Vec<f64>,
    /// The final component of the root this side's entries sit under, or
    /// `None` where they sit under several roots or under none.
    ///
    /// Taken from the same [`crate::derive::rel_paths_for_series`] rows
    /// [`Self::directories`] is built from rather than from `member.root_id`, so
    /// the two can never name different roots — and so a side whose material
    /// produced no `member` rows at all, a chapter-organised directory being
    /// exactly that, still says where it is.
    pub root_name: Option<String>,
    /// The lowest-numbered volume's hub edition, or `None` for a side with
    /// no `work` rows — a series holding only chapter-organised directories
    /// has none, and C3 §8 asks for that to degrade rather than break.
    pub first_volume: Option<FirstVolume>,
}

/// One signal, as the expanded row lists it.
///
/// **Structured rather than split out of [`Proposal::reason`].** That field
/// is one human sentence built for a CLI line, and a client that split it on
/// punctuation would break the moment the wording changed. The mockup shows
/// each signal on its own line with its sign, which is a different rendering
/// of the same facts rather than a reformatting of the same string.
#[derive(Debug, Clone, PartialEq)]
pub struct Reason {
    pub text: String,
    /// `+` a signal for, `-` a signal against, `?` a fact with no direction:
    /// "neither side has any known volumes yet", a target renamed out from
    /// under the proposal, and the single sentence an `attach-chapters`
    /// proposal carries — which argues neither way, there being no accept
    /// for it to argue about.
    ///
    /// **`?` is also what everything unlisted gets**, because
    /// [`scored_reasons`] reaches it through an `else` rather than through a
    /// case per kind: `align`, which nothing in this module raises, and an
    /// `edition-of` row whose `target_id` is NULL both land here. Neither has
    /// a second series to compare against, so there is nothing for a sign to
    /// be about — which is the same answer the three named cases get, for the
    /// same reason.
    pub sign: char,
}

/// Everything the expanded row shows for one proposal.
#[derive(Debug, Clone, PartialEq)]
pub struct Evidence {
    pub proposal: Proposal,
    pub subject: Side,
    /// Absent for a kind that names only a subject.
    pub target: Option<Side>,
    /// The signals behind the score, each with its sign. C3 §2.
    pub reasons: Vec<Reason>,
    /// The rule an accept would write, or `None` when this proposal cannot
    /// be accepted — every refusal [`preview`] makes, `attach-chapters` and
    /// a renamed target among them. The row still expands; only the accept
    /// control is absent.
    ///
    /// **Only a refusal, never a read that failed.** This was
    /// `preview(conn, id).ok()`, which turned a database fault inside the
    /// preview into the same `None` and so presented a failure to a person as
    /// "this cannot be accepted". [`Offer`] is what tells the two apart:
    /// a refusal lands here as `None`, a failed read fails the whole request.
    pub rule: Option<ProposedRule>,
}

/// The expanded evidence for proposal `id`.
///
/// # Errors
///
/// If `id` names no proposal, or if the reads against `series`, `work`,
/// `edition`, `member`, `root`, `rule` — the sweep behind it reads every
/// rule through [`crate::rules::Engine::load`] — or `scan_entry` fail. A
/// proposal that cannot be previewed is **not** an error: it answers with
/// `rule: None`.
pub fn evidence(conn: &Connection, id: i64) -> Result<Evidence, String> {
    let proposal = one(conn, id)?;
    let subject_title = series_title(conn, proposal.subject_id)?;
    let subject_library = series_library(conn, proposal.subject_id)?;
    let target_title = match proposal.target_id {
        Some(target_id) => Some(series_title(conn, target_id)?),
        None => None,
    };
    let target_library = match proposal.target_id {
        Some(target_id) => series_library(conn, target_id)?,
        None => None,
    };

    // **One sweep for the whole request.** Finding where a series' material
    // sits means compiling every rule in the library, reading every root and
    // resolving every `scan_entry` row under each — see
    // [`crate::derive::rel_paths_by_series`] — and this route ran three of
    // them: one per side, plus `preview`'s own for the subject a second time.
    // It is what a person triggers by expanding a row.
    //
    // Keyed by library and *title* rather than by series id, which is also
    // how `rel_paths_by_series` filters: two `series` rows of one library can
    // carry the same `sort_title`, and both then read the same entries, which
    // is what the sweep would have answered for either of them separately.
    // Each side is asked about in its own library.
    let mut asked: std::collections::BTreeSet<(Option<i64>, &str)> =
        std::collections::BTreeSet::from([(subject_library, subject_title.as_str())]);
    if let Some(title) = &target_title {
        asked.insert((target_library, title.as_str()));
    }
    let material = crate::derive::rel_paths_by_series(conn, &asked)?;
    let paths_of = |library: Option<i64>, title: &str| -> &[(i64, String)] {
        material
            .get(&(library, title.to_string()))
            .map_or(&[][..], Vec::as_slice)
    };

    let subject = side(
        conn,
        proposal.subject_id,
        subject_title.clone(),
        paths_of(subject_library, &subject_title),
    )?;
    let target = match proposal.target_id {
        Some(target_id) => {
            // `target_title` was read from this same `target_id` above, so
            // it is `Some` whenever this arm runs. Matched on
            // `target_id` alone rather than on the pair, so there is no
            // `(Some, None)` arm to make impossible and no `_` to make it
            // silent: if that invariant ever broke, this panics instead of
            // quietly dropping the target side.
            let title = target_title
                .clone()
                .expect("target_title is set whenever target_id is, above");
            Some(side(
                conn,
                target_id,
                title.clone(),
                paths_of(target_library, &title),
            )?)
        }
        None => None,
    };
    let reasons = scored_reasons(conn, &proposal)?;

    // **A refusal becomes an absent rule; a failed read fails the request.**
    // `preview` declines several kinds of proposal by design — a kind that
    // cannot be accepted at all, one already judged, a target renamed since
    // it was raised — and every one of those rows is still listed and still
    // expandable, so propagating those would mean a person could not read the
    // evidence for exactly the proposals whose evidence explains why they are
    // stuck. A read that *fails* is not one of them, and `.ok()` could not
    // tell the difference.
    let rule = match offered_rule(
        conn,
        &proposal,
        &subject_title,
        paths_of(subject_library, &subject_title),
    )? {
        Offer::Made(rule) => Some(rule),
        Offer::Refused(_) => None,
    };

    Ok(Evidence {
        proposal,
        subject,
        target,
        reasons,
        rule,
    })
}

/// Proposal `id`, or `None` if there is no such proposal.
///
/// **The read that tells "there is no such row" apart from "the read
/// failed".** [`one`] below folds the two together — an absent row becomes an
/// `Err` like any other — which is right for the callers that cannot proceed
/// either way, and wrong for a caller that has to answer differently: C3's
/// routes answered an unknown id 409 on three routes and 500 on a fourth,
/// each quoting the same sentence, because the only way to tell from the
/// outside was to match on the store's own words. This is the same split
/// [`Offer`] makes for [`preview`] and the three writes, made for a read.
///
/// # Errors
///
/// Returns an error only if the read fails. An `id` naming no proposal is
/// `Ok(None)`.
pub fn lookup(conn: &Connection, id: i64) -> Result<Option<Proposal>, String> {
    conn.query_row(
        &format!("SELECT {PROPOSAL_COLUMNS} FROM proposal p WHERE p.id = ?1"),
        [id],
        proposal_from_row,
    )
    .optional()
    .map_err(|e| format!("looking up proposal {id}: {e}"))
}

/// Proposal `id` alone, with an absent one an error.
///
/// # Errors
///
/// Returns an error naming `id` if no proposal carries it, or if the read
/// fails.
fn one(conn: &Connection, id: i64) -> Result<Proposal, String> {
    lookup(conn, id)?.ok_or_else(|| format!("looking up proposal {id}: there is no such proposal"))
}

/// One end of a proposal: its title, where its material is, what volumes it
/// holds, and the first of them.
///
/// `title` and `paths` are handed in rather than read, so that one sweep
/// answers for both sides of a proposal and for the rule beside them — see
/// [`evidence`].
fn side(
    conn: &Connection,
    series_id: i64,
    title: String,
    paths: &[(i64, String)],
) -> Result<Side, String> {
    let (directories, root_name) = material_of(conn, paths)?;
    Ok(Side {
        series_id,
        volume_count: usize::try_from(volume_count(conn, series_id)?).unwrap_or(0),
        volumes: volumes_of(conn, series_id)?,
        first_volume: first_volume(conn, series_id)?,
        title,
        directories,
        root_name,
    })
}

/// Where a series' material sits: [`Side::directories`], and
/// [`Side::root_name`] beside it.
///
/// The two are returned together because they are read from one set of rows
/// and would otherwise be two answers to "which root" — see
/// [`Side::root_name`].
///
/// **One case rather than four.** While this answered with a single string
/// it needed an arm per shape — nothing scanned, one directory under one
/// root, several under one root, several across several — and the last two
/// were sentences rather than paths. The third of them was also wrong:
/// `dirs` is a set of *relative* directories, so two roots each holding
/// their own `Publisher/` gave `dirs.len() == 1` with two roots, fell
/// through to the catch-all, and rendered `"1 directories across 2 roots"`,
/// discarding the one directory name the two had in common. Joining each
/// entry onto its own root before collecting makes all four the same case,
/// and the empty answer falls out of it rather than being written.
fn material_of(
    conn: &Connection,
    paths: &[(i64, String)],
) -> Result<(Vec<String>, Option<String>), String> {
    let root_ids: std::collections::BTreeSet<i64> = paths.iter().map(|(r, _)| *r).collect();
    let roots = root_paths(conn, &root_ids)?;

    // **`None` for material under two roots, rather than either of them.**
    // The field is one name a heading shows, and a side spanning two roots
    // has no single answer — naming the first would be a claim about where
    // the *rest* of it is that nothing checked. `directories` below is where
    // that side's several answers are.
    let root_name = if roots.len() == 1 {
        roots
            .values()
            .next()
            .and_then(|root| std::path::Path::new(root).file_name())
            .map(|name| name.to_string_lossy().into_owned())
    } else {
        None
    };

    let mut directories: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for (root_id, rel_path) in paths {
        let root = roots
            .get(root_id)
            .expect("every id here came out of `paths`, and `root_paths` read all of them");
        // `parent_of` answers `""` for an entry directly under the root,
        // which is the root's own relative path — so the two agree rather
        // than needing a case of their own.
        let dir = crate::derive::parent_of(rel_path);
        directories.insert(if dir.is_empty() {
            root.clone()
        } else {
            format!("{root}/{dir}")
        });
    }
    Ok((directories.into_iter().collect(), root_name))
}

/// The volume numbers this series' works carry, ascending — the works
/// carrying none are left out, which is why this is not
/// [`volume_count`]'s list.
fn volumes_of(conn: &Connection, series_id: i64) -> Result<Vec<f64>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT volume FROM work WHERE series_id = ?1 AND volume IS NOT NULL ORDER BY volume",
        )
        .map_err(|e| format!("listing the volumes of series {series_id}: {e}"))?;
    let rows = stmt
        .query_map([series_id], |r| r.get(0))
        .map_err(|e| format!("listing the volumes of series {series_id}: {e}"))?;
    rows.collect::<Result<Vec<f64>, _>>()
        .map_err(|e| format!("listing the volumes of series {series_id}: {e}"))
}

/// The lowest-numbered volume of `series_id` and its hub edition, or `None`
/// where the series has no `work` row, or where the work's `hub` names no
/// `edition` row.
///
/// **`volume IS NULL` sorts last, rather than SQLite's own order.** A plain
/// `ORDER BY volume` puts NULLs first, so a single one-shot beside eleven
/// numbered volumes would be shown as this side's "volume 1" while
/// [`Side::volumes`] beside it started at 1 — the panel's heading and its
/// list disagreeing about which volume the pages came from. A series whose
/// works *all* carry no number still answers, with the lowest work id among
/// them.
///
/// **A missing hub edition is `None`, not a substitute.** Falling back to
/// whichever edition the work does have would put a different edition's
/// pages under a key that says `hub`, and the whole reason to compare the
/// hub is that `content_key` is computed over it.
fn first_volume(conn: &Connection, series_id: i64) -> Result<Option<FirstVolume>, String> {
    let work: Option<(i64, String, String)> = conn
        .query_row(
            "SELECT id, slug, hub FROM work WHERE series_id = ?1 \
             ORDER BY volume IS NULL, volume, id LIMIT 1",
            [series_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(|e| format!("looking up volume 1 of series {series_id}: {e}"))?;
    let Some((work_id, work_slug, hub)) = work else {
        return Ok(None);
    };

    let edition_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM edition WHERE work_id = ?1 AND key = ?2",
            rusqlite::params![work_id, hub],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("looking up the hub edition of work {work_id}: {e}"))?;
    let Some(edition_id) = edition_id else {
        return Ok(None);
    };

    let page_count: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(pages), 0) FROM member WHERE edition_id = ?1",
            [edition_id],
            |r| r.get(0),
        )
        .map_err(|e| format!("counting the pages of edition {edition_id}: {e}"))?;
    let formats = edition_formats(conn, edition_id)?;

    Ok(Some(FirstVolume {
        work_slug,
        edition_key: hub,
        page_count,
        formats,
        interior: interior_indices(page_count),
    }))
}

/// Every distinct format `edition_id`'s members were walked as, ascending.
///
/// **`member.format` is not the source, because nothing writes it.** The
/// only `INSERT INTO member` in this repository — `import.rs`'s — lists
/// `(edition_id, ordinal, path, sha256, pages)` and no format, and every
/// `UPDATE` that reaches these rows sets only `chapter_declared`,
/// `chapter_source` or `root_id`. Measured on this module's own two-series
/// fixture:
/// `SELECT COUNT(*), COUNT(format) FROM member` answers `2, 0`, while every
/// `scan_entry` row behind those members carries a real token. Read from the
/// column, [`FirstVolume::formats`] was `None` for every edition in every
/// library — a permanently blank field a client cannot tell from an honest
/// "not available yet".
///
/// **Joined on `(root_id, rel_path)`, which is the pair a member and the
/// entry it came from share.** `derive`'s `Candidate` builds `member.path`
/// by joining `root.path` onto `scan_entry.rel_path`, so taking that prefix
/// back off is what re-finds the row — and it lands on `scan_entry`'s own
/// `UNIQUE (root_id, rel_path)` index rather than comparing every entry in
/// the root against every member. `rtrim` is for a root that is a single
/// `/`, whose join adds no separator of its own.
///
/// **A member with no root contributes nothing**, which is why this can be
/// empty for an edition that has members: an imported work's rows carry
/// `root_id` NULL by design (`schema/006_member_root.sql`), and so does any
/// member whose root has since been removed. Nothing was walked, so there is
/// no format to report, and inventing one from the path's extension would be
/// a guess where the scanner has a measurement.
fn edition_formats(conn: &Connection, edition_id: i64) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT scan_entry.format FROM member \
             JOIN root ON root.id = member.root_id \
             JOIN scan_entry ON scan_entry.root_id = member.root_id \
              AND scan_entry.rel_path = substr(member.path, length(rtrim(root.path, '/')) + 2) \
             WHERE member.edition_id = ?1 AND scan_entry.format IS NOT NULL \
             ORDER BY 1",
        )
        .map_err(|e| format!("reading the formats of edition {edition_id}: {e}"))?;
    let rows = stmt
        .query_map([edition_id], |r| r.get(0))
        .map_err(|e| format!("reading the formats of edition {edition_id}: {e}"))?;
    rows.collect::<Result<Vec<String>, _>>()
        .map_err(|e| format!("reading the formats of edition {edition_id}: {e}"))
}

/// Two interior indices inside an edition of `page_count` pages.
///
/// Deterministic, so the same two pages are compared every time; and
/// skipping 0, which the cover already shows.
///
/// Fewer than two where the two fractions land on the same page, which is
/// what three pages or fewer does — `4` is the shortest edition with two
/// distinct interiors, `[1, 2]` — and none at all below two pages, where
/// there is no page but the cover to name.
///
/// The upper half of the `clamp` never fires while both fractions are below
/// one, and stays anyway: the invariant is this function's, not a property
/// of whichever fractions someone later picks. A mutation raising it to
/// `page_count` survives the suite for exactly that reason.
fn interior_indices(page_count: i64) -> Vec<i64> {
    if page_count < 2 {
        return Vec::new();
    }
    let mut picked: Vec<i64> = [page_count / 4, page_count * 3 / 5]
        .into_iter()
        .map(|i| i.clamp(1, page_count - 1))
        .collect();
    picked.dedup();
    picked
}

/// The signals behind `proposal`'s score, each with its sign.
fn scored_reasons(conn: &Connection, proposal: &Proposal) -> Result<Vec<Reason>, String> {
    let paired = proposal
        .target_id
        .filter(|_| proposal.kind == KIND_EDITION_OF);
    let Some(target_id) = paired else {
        // **The kinds that are not scored from a pair of signals.**
        // [`CERTAIN`]'s own doc comment says the number is a placement on
        // the scale rather than a measurement, so there is no second signal
        // to list beside the first: `same-series` states one fact — a
        // directory's name is a known title of another series — which is
        // evidence *for*, and `attach-chapters` states one that argues
        // neither way, there being no accept for it to argue about.
        //
        // Every other kind — `align`, which nothing here raises — and an
        // `edition-of` row with no target to compare against take the same
        // unsigned `?`, for want of anything to say about a direction.
        //
        // The sentence `raise` recorded is carried whole rather than split,
        // which is what [`Reason`]'s own doc comment refuses.
        let sign = if proposal.kind == KIND_SAME_SERIES {
            '+'
        } else {
            '?'
        };
        return Ok(vec![Reason {
            text: proposal.reason.clone(),
            sign,
        }]);
    };

    let signals = edition_of_signals(conn, proposal.subject_id, target_id)?;
    let vocabulary = match (signals.suffix.as_deref(), signals.word) {
        (Some(suffix), Some(word)) => Reason {
            text: format!("the suffix \"{suffix}\" matches the edition word \"{word}\""),
            sign: '+',
        },
        (Some(suffix), None) => Reason {
            text: format!("the suffix \"{suffix}\" matches no edition word"),
            sign: '-',
        },
        // The rename `preview` refuses an accept for, stated as the fact it
        // is: there is no suffix to have matched anything, so neither a `+`
        // nor a `-` would be about the vocabulary at all.
        (None, _) => Reason {
            text: format!(
                "\"{}\" no longer starts with \"{} - \" — one of the two series was renamed \
                 since this proposal was raised",
                signals.subject_title, signals.target_title,
            ),
            sign: '?',
        },
    };
    let (text, sign) = signals.volume_signal();
    Ok(vec![vocabulary, Reason { text, sign }])
}

// --- raising, per kind -------------------------------------------------------

struct SeriesRow {
    id: i64,
    sort_title: String,
    /// The library the series is filed under, or `None` for a series in none.
    library_id: Option<i64>,
}

fn all_series(conn: &Connection) -> Result<Vec<SeriesRow>, String> {
    let mut stmt = conn
        .prepare("SELECT id, sort_title, library_id FROM series ORDER BY id")
        .map_err(|e| format!("listing series: {e}"))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(SeriesRow {
                id: r.get(0)?,
                sort_title: r.get(1)?,
                library_id: r.get(2)?,
            })
        })
        .map_err(|e| format!("listing series: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("listing series: {e}"))
}

fn series_title(conn: &Connection, series_id: i64) -> Result<String, String> {
    conn.query_row(
        "SELECT sort_title FROM series WHERE id = ?1",
        [series_id],
        |r| r.get(0),
    )
    .map_err(|e| format!("looking up series {series_id}: {e}"))
}

/// [`series_title`]'s inverse within `library`, and `None` where no series of
/// that library carries the name. A `library` of `None` asks among the series
/// in no library. Where several carry it, the oldest answers.
///
/// **`None` is an answer, not a failure**, for both callers.
/// [`raise_attach_chapters`] can be called before any derive has reconciled
/// the names it found, and [`series_reached_by`] resolves container
/// directories to names — a publisher's, under the seeded first-component
/// rule — that no series was ever derived from. Neither has anything to
/// attach to, which is the honest outcome rather than an error.
fn series_id_by_sort_title(
    conn: &Connection,
    library: Option<i64>,
    sort_title: &str,
) -> Result<Option<i64>, String> {
    conn.query_row(
        "SELECT id FROM series WHERE sort_title = ?1 AND library_id IS ?2 ORDER BY id LIMIT 1",
        rusqlite::params![sort_title, library],
        |r| r.get(0),
    )
    .optional()
    .map_err(|e| format!("looking up the series `{sort_title}`: {e}"))
}

/// The library series `series_id` is filed under, or `None` for a series in
/// none.
fn series_library(conn: &Connection, series_id: i64) -> Result<Option<i64>, String> {
    conn.query_row(
        "SELECT library_id FROM series WHERE id = ?1",
        [series_id],
        |r| r.get(0),
    )
    .map_err(|e| format!("looking up the library of series {series_id}: {e}"))
}

/// Every `work` row against `series_id`, whether or not it carries a volume
/// number — a series of one-shots all sharing `volume IS NULL` is as much
/// "equal" between two series as a series of ten matching volumes is.
fn volume_count(conn: &Connection, series_id: i64) -> Result<i64, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM work WHERE series_id = ?1",
        [series_id],
        |r| r.get(0),
    )
    .map_err(|e| format!("counting volumes for series {series_id}: {e}"))
}

/// Raise one proposal — `INSERT OR IGNORE`, backed by `proposal_identity` — and
/// refresh its score and reason if it was already `open`.
///
/// **Score and `reason` are not frozen at first raise.** `INSERT OR IGNORE`
/// never updates an existing row, so a proposal raised before its series had
/// any facts behind it would otherwise keep its first score and text forever
/// — `open`'s threshold hiding it even after the facts that would raise its
/// score arrive. The `UPDATE` below is scoped to `state = 'open'` so a
/// dismissed or accepted row is never rescored out from under the decision
/// already made about it — §6.1's "a dismissed or accepted row is never
/// rescored" — and its own affected-row count is not part of what this
/// function returns: refreshing a proposal a person has already seen is not
/// the same event as opening one they have not, and the caller's return
/// value means the latter.
fn insert_proposal(
    conn: &Connection,
    kind: &str,
    subject_id: i64,
    target_id: Option<i64>,
    score: f64,
    reason: &str,
) -> Result<usize, String> {
    let newly_open = conn
        .execute(
            "INSERT OR IGNORE INTO proposal (kind, subject_id, target_id, score, reason) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![kind, subject_id, target_id, score, reason],
        )
        .map_err(|e| format!("raising a {kind} proposal for series {subject_id}: {e}"))?;

    conn.execute(
        "UPDATE proposal SET score = ?1, reason = ?2 \
          WHERE kind = ?3 AND subject_id = ?4 AND ifnull(target_id, -1) = ifnull(?5, -1) \
            AND state = ?6",
        rusqlite::params![score, reason, kind, subject_id, target_id, STATE_OPEN],
    )
    .map_err(|e| format!("refreshing a {kind} proposal for series {subject_id}: {e}"))?;

    Ok(newly_open)
}

/// §6's two signals for one `edition-of` pair, and the titles they were read
/// from.
///
/// **One computation, two renderings.** [`raise_edition_of`] scores from
/// these and formats them into the single sentence a CLI line shows;
/// [`scored_reasons`] lists them one per line with a sign for the expanded
/// row. Written twice, the two implementations would drift, and the drift
/// would show as an expanded row whose signs disagreed with the sentence
/// beside them about the same pair of series.
///
/// **What this does not do is make the row agree with `proposal.score`.**
/// That number is stored when [`raise`] runs and these signals are computed
/// live, so between a derive that changes the facts and the next `raise` an
/// expanded row can honestly show two `+` beside a stored score of `1`. The
/// row agrees with the score as of the last raise, and it is
/// [`insert_proposal`]'s refresh of an `open` proposal — not anything here —
/// that closes the window.
struct EditionOfSignals {
    subject_title: String,
    target_title: String,
    /// What the subject's title carries past `"{target} - "`, or `None`
    /// where it no longer carries that prefix at all — the rename
    /// [`preview`] refuses an accept for.
    suffix: Option<String>,
    /// The vocabulary word that suffix matched, if any: §6's first signal.
    word: Option<&'static str>,
    subject_volumes: i64,
    target_volumes: i64,
    /// §6's second signal. **Absence of evidence is not agreement.** `0 == 0`
    /// is not two series agreeing about their volumes — it is two series
    /// about which the question has not been asked, and counting it as a
    /// match would inflate the score of exactly the cases with the least
    /// evidence behind them. So this requires a `work` row on both sides,
    /// not merely equal counts.
    volumes_match: bool,
}

impl EditionOfSignals {
    /// [`SIGNAL`] per signal that fired — 0, 1 or 2 for this kind.
    fn score(&self) -> f64 {
        let mut score = 0.0;
        if self.word.is_some() {
            score += SIGNAL;
        }
        if self.volumes_match {
            score += SIGNAL;
        }
        score
    }

    /// The vocabulary half of [`Proposal::reason`]'s sentence.
    ///
    /// Deliberately not the wording [`scored_reasons`] uses: that one names
    /// the suffix and the word it matched, which is a different rendering of
    /// the same signal rather than the same string reformatted.
    fn vocabulary_phrase(&self) -> &'static str {
        if self.word.is_some() {
            "the suffix matches the edition vocabulary"
        } else {
            "the suffix matches no edition word"
        }
    }

    /// The volume half of [`Proposal::reason`]'s sentence, and the sign the
    /// expanded row shows beside it.
    ///
    /// One function returning both, so the sentence and the sign cannot
    /// disagree about which of the three cases this pair is in.
    fn volume_signal(&self) -> (String, char) {
        if self.subject_volumes == 0 && self.target_volumes == 0 {
            (
                "neither side has any known volumes yet, so there is nothing to compare"
                    .to_string(),
                '?',
            )
        } else if self.volumes_match {
            (format!("both hold {} volume(s)", self.target_volumes), '+')
        } else {
            (
                format!(
                    "{} vs {} volumes",
                    self.target_volumes, self.subject_volumes
                ),
                '-',
            )
        }
    }
}

/// Read §6's signals for the pair `(subject_id, target_id)`.
///
/// [`raise_edition_of`] arrives here having already matched the same prefix
/// in memory: that is the pair filter for its loop over every ordered pair
/// of series, which must not cost four queries per non-pair. The suffix it
/// keeps is the same string this recomputes from the same two rows; the
/// *signals* are read here only.
fn edition_of_signals(
    conn: &Connection,
    subject_id: i64,
    target_id: i64,
) -> Result<EditionOfSignals, String> {
    let subject_title = series_title(conn, subject_id)?;
    let target_title = series_title(conn, target_id)?;
    let suffix = subject_title
        .strip_prefix(&format!("{target_title} - "))
        .map(str::to_owned);
    let word = suffix.as_deref().and_then(matched_edition_word);
    let subject_volumes = volume_count(conn, subject_id)?;
    let target_volumes = volume_count(conn, target_id)?;
    Ok(EditionOfSignals {
        subject_title,
        target_title,
        suffix,
        word,
        subject_volumes,
        target_volumes,
        volumes_match: target_volumes > 0 && target_volumes == subject_volumes,
    })
}

/// §6: every series whose `sort_title` is another series' title plus a
/// `" - "` suffix, scored by a vocabulary hit in the suffix and an equal
/// volume count between the two series.
fn raise_edition_of(conn: &Connection, series: &[SeriesRow]) -> Result<usize, String> {
    let mut newly_open = 0;
    for base in series {
        let prefix = format!("{} - ", base.sort_title);
        for suffixed in series {
            if suffixed.id == base.id {
                continue;
            }
            // **Within one library — spec decision 3.** An Ecchi series is never
            // offered as an edition of a Volumes one.
            if suffixed.library_id != base.library_id {
                continue;
            }
            let Some(suffix) = suffixed.sort_title.strip_prefix(prefix.as_str()) else {
                continue;
            };
            if suffix.is_empty() {
                continue;
            }
            let signals = edition_of_signals(conn, suffixed.id, base.id)?;
            let reason = format!(
                "\"{}\" may be the \"{suffix}\" edition of \"{}\" — {}; {}",
                suffixed.sort_title,
                base.sort_title,
                signals.vocabulary_phrase(),
                signals.volume_signal().0,
            );
            newly_open += insert_proposal(
                conn,
                KIND_EDITION_OF,
                suffixed.id,
                Some(base.id),
                signals.score(),
                &reason,
            )?;
        }
    }
    Ok(newly_open)
}

/// §5's second use of `series_title`: a series whose own `sort_title` equals
/// some *other* series' known title joins that series. The other series is
/// one of the candidate's own library, or one in no library for a candidate
/// in none.
fn raise_same_series(conn: &Connection, series: &[SeriesRow]) -> Result<usize, String> {
    let mut newly_open = 0;
    for candidate in series {
        // `ORDER BY series_id` before `LIMIT 1`: two different series can
        // each claim the same title (two people, or two imports, recording
        // it independently), and without an explicit order the pick is
        // whatever a b-tree scan happens to return first — unspecified and
        // liable to change across a page split that has nothing to do with
        // this data. The lowest id is arbitrary but stable, which is what a
        // proposal's identity needs to be idempotent across a rescan.
        let owner: Option<i64> = conn
            .query_row(
                "SELECT st.series_id FROM series_title st JOIN series s ON s.id = st.series_id \
                  WHERE st.title = ?1 AND st.series_id != ?2 AND s.library_id IS ?3 \
                  ORDER BY st.series_id LIMIT 1",
                rusqlite::params![candidate.sort_title, candidate.id, candidate.library_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| {
                format!(
                    "looking up known titles for `{}`: {e}",
                    candidate.sort_title
                )
            })?;
        let Some(target_id) = owner else { continue };
        let target_title = series_title(conn, target_id)?;
        let reason = format!(
            "\"{}\" is a known title of \"{target_title}\"",
            candidate.sort_title
        );
        newly_open += insert_proposal(
            conn,
            KIND_SAME_SERIES,
            candidate.id,
            Some(target_id),
            CERTAIN,
            &reason,
        )?;
    }
    Ok(newly_open)
}

/// §7, through [`crate::derive::unattached_chapter_dirs`]: one proposal per
/// series holding at least one chapter-organised directory the rules gave no
/// volume. `target_id` is `NULL` — there is no second series involved — which
/// `proposal_identity`'s `ifnull(target_id, -1)` is what makes idempotent
/// across however many such directories one series accumulates.
///
/// **The series is the one in the directory's own library.** A directory's
/// library is its root's, the directories are counted per series within a
/// library, and the name is looked up there.
fn raise_attach_chapters(conn: &Connection) -> Result<usize, String> {
    let dirs = crate::derive::unattached_chapter_dirs(conn)?;
    let library_of: BTreeMap<i64, i64> = crate::scan::roots(conn)?
        .into_iter()
        .map(|root| (root.id, root.library_id))
        .collect();
    let mut per_series: BTreeMap<(String, i64), usize> = BTreeMap::new();
    for ((root_id, _directory), series_name) in dirs {
        // A root removed since the directories were read has no library to
        // ask in.
        let Some(&library_id) = library_of.get(&root_id) else {
            continue;
        };
        *per_series.entry((series_name, library_id)).or_insert(0) += 1;
    }

    let mut newly_open = 0;
    for ((series_name, library_id), count) in per_series {
        let Some(series_id) = series_id_by_sort_title(conn, Some(library_id), &series_name)? else {
            // No series row exists for this name yet — `raise` is meant to
            // run at the end of `derive::derive`, after every named series is
            // reconciled, so this is unreached in that path. A standalone
            // call before any derive has run finds nothing to attach a
            // proposal to, which is the honest outcome rather than a
            // dangling `subject_id`.
            continue;
        };
        let reason = if count == 1 {
            format!(
                "\"{series_name}\" holds one directory organised by chapter, not volume — only \
                 alignment can say which volume it belongs to, e.g. `taiyaku align raw-v01.cbz \
                 -- en-c001.cbz en-c002.cbz …`"
            )
        } else {
            format!(
                "\"{series_name}\" holds {count} directories organised by chapter, not volume — \
                 only alignment can say which volume each belongs to, e.g. `taiyaku align \
                 raw-v01.cbz -- en-c001.cbz en-c002.cbz …`"
            )
        };
        newly_open += insert_proposal(
            conn,
            KIND_ATTACH_CHAPTERS,
            series_id,
            None,
            CERTAIN,
            &reason,
        )?;
    }
    Ok(newly_open)
}

/// Reopen any `accepted` proposal whose `created_rule_id` has gone `NULL` —
/// what a rule deleted through C4's editor leaves behind, per
/// `schema/003_scanner.sql`'s `ON DELETE SET NULL`. `unaccept` handles the
/// same repair immediately for the rule it deletes itself; this is for every
/// other way a rule this module wrote can disappear.
///
/// **`003_scanner.sql` calls this function `proposals::reconcile`, and that
/// name never existed.** It cannot be corrected there: the comment sits
/// inside the `CREATE TABLE proposal (…)` statement, and SQLite stores a
/// statement's original text verbatim in `sqlite_master` — checked, not
/// assumed — so editing it would make a database created by a later build
/// hold different schema text from one already on disk, which is exactly what
/// `schema.rs`'s "append, never edit" exists to prevent. The correction lives
/// here instead, where a reader following that comment arrives.
///
/// **Returns every id it reopened, ascending —
/// [`crate::rules::Deleted::reopened_proposals`]'s own shape, not a bare
/// count.** [`crate::rules::import_json`] calls this directly (whole-branch
/// review, Important 3): it replaces the whole rule set through
/// `replace_all`'s own `DELETE FROM rule`, which orphans exactly as many
/// `created_rule_id`s as [`crate::rules::delete`] deletes one at a time, and
/// its own HTTP route reports the ids the identical way `delete_rule`
/// already does — a route that only ever handed back a count could not.
pub(crate) fn reopen_orphaned_accepts(conn: &Connection) -> Result<Vec<i64>, String> {
    let mut stmt = conn
        .prepare("SELECT id FROM proposal WHERE state = ?1 AND created_rule_id IS NULL")
        .map_err(|e| format!("looking up accepted proposals whose rule has gone: {e}"))?;
    let reopened: Vec<i64> = stmt
        .query_map([STATE_ACCEPTED], |r| r.get(0))
        .map_err(|e| format!("looking up accepted proposals whose rule has gone: {e}"))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("reading accepted proposals whose rule has gone: {e}"))?;
    conn.execute(
        "UPDATE proposal SET state = ?1 WHERE state = ?2 AND created_rule_id IS NULL",
        rusqlite::params![STATE_OPEN, STATE_ACCEPTED],
    )
    .map_err(|e| format!("reopening accepted proposals whose rule has gone: {e}"))?;
    Ok(reopened)
}

#[cfg(test)]
mod tests;
