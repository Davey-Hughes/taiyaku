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

const KIND_EDITION_OF: &str = "edition-of";
const KIND_SAME_SERIES: &str = "same-series";
const KIND_ATTACH_CHAPTERS: &str = "attach-chapters";

const STATE_OPEN: &str = "open";
const STATE_ACCEPTED: &str = "accepted";
const STATE_DISMISSED: &str = "dismissed";

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
    newly_open += reopen_orphaned_accepts(conn)?;
    Ok(newly_open)
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
    let mut stmt = conn
        .prepare(
            "SELECT id, kind, subject_id, target_id, score, reason, state, created_rule_id \
             FROM proposal WHERE state = ?1 AND score >= ?2 ORDER BY score DESC, id",
        )
        .map_err(|e| format!("listing open proposals: {e}"))?;
    let rows = stmt
        .query_map(rusqlite::params![STATE_OPEN, min_score], |r| {
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
        })
        .map_err(|e| format!("listing open proposals: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("listing open proposals: {e}"))
}

/// Accept proposal `id`: write a narrow `prefix` rule matching only the
/// subject series' own material, record its id on the proposal, and mark it
/// `accepted`.
///
/// §6.1: this is the only way an accept reaches derivation, which is what
/// keeps `scan_entry` + `rule` derivation's one input. The rule sets
/// `set_series` to the target series' title — "this directory's material
/// belongs to that series" — and, for an `edition-of` accept,
/// `set_edition_key` too; see the note on that below.
///
/// **The pattern comes from `scan_entry`, never from the subject's own
/// `sort_title`.** An earlier version read the title directly, reasoning
/// that seeded rule 1 resolves a series from the first path component, so
/// the title *is* that component — false whenever a higher-ordinal rule
/// decides the series first, which is exactly what seeded rule 4 exists for
/// (a filename that repeats the series where the directory does not).
/// Measured: on a root holding `Publisher/Go-Toubun no Hanayome v01.cbz` and
/// its colour sibling, the title-derived pattern was
/// `Go-Toubun no Hanayome - Full Color Edition/`, every real path began
/// `Publisher/`, and the rule matched nothing — `accepted` forever, the
/// library untouched. That is the "accept produces nothing a person can
/// see" outcome this module refuses `attach-chapters` to avoid, reached by a
/// different road. See [`common_directory_pattern`] and
/// [`crate::derive::rel_paths_for_series`].
///
/// **And the pattern must match *only* the subject.** A pattern that merely
/// matches something is not enough either: `Publisher/`, derived correctly
/// above, matches every series a publisher directory holds. Measured: on
/// such a directory holding an unrelated third series alongside the two
/// related ones, accepting moved that third series' volumes into the
/// accepted series' page sequence, silently — `write_work`'s own doc records
/// that member order is page order and every alignment offset is computed
/// from it, so a reader-visible mix-up, not a cosmetic one. The pattern this
/// bug replaced decided nothing; this one decides too much, which is worse
/// because it looks like a decision nobody would notice was wrong. See
/// [`crate::derive::other_series_under_prefix`].
///
/// # Errors
///
/// Returns an error if `id` names no proposal, if that proposal is not
/// currently `open` (an already-accepted or -dismissed proposal is never
/// silently re-decided — un-accept first), if it names an `attach-chapters`
/// proposal — refused by name rather than silently producing no work, see
/// the module doc — if it has no `target_id` to write a rule against, if the
/// subject's entries do not share exactly one directory, if that directory
/// also holds another series' entries — naming it, rather than moving its
/// material silently — if the rule this would write is verified against
/// `scan_entry` after writing and matches nothing (written and then removed
/// rather than left deciding nothing), or if the lookups and writes against
/// `series`, `rule` or `proposal` fail.
pub fn accept(conn: &Connection, id: i64) -> Result<i64, String> {
    let (kind, subject_id, target_id, state): (String, i64, Option<i64>, String) = conn
        .query_row(
            "SELECT kind, subject_id, target_id, state FROM proposal WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(|e| format!("looking up proposal {id}: {e}"))?;

    // **A state guard, not just a happy-path assumption.** Without it, a
    // second `accept` on an already-accepted proposal writes a second rule
    // and overwrites `created_rule_id` with its id — `unaccept` then deletes
    // only the second, and the first stays in force deciding structure,
    // invisible to the proposal that made it, while the proposal reads
    // `open`. That is exactly what `created_rule_id` exists to prevent.
    if state != STATE_OPEN {
        return Err(format!(
            "proposal {id} is `{state}`, not open — accept only applies to an open proposal"
        ));
    }

    if kind == KIND_ATTACH_CHAPTERS {
        return Err(format!("proposal {id}: {ATTACH_CHAPTERS_REFUSAL}"));
    }
    let Some(target_id) = target_id else {
        return Err(format!(
            "proposal {id} (kind `{kind}`) has no target series to write a rule against"
        ));
    };

    let subject_title = series_title(conn, subject_id)?;
    let target_title = series_title(conn, target_id)?;
    let (pattern, root_id) = common_directory_pattern(conn, id, &subject_title)?;

    // **"Matches something" is not "matches only the subject".** A `prefix`
    // rule decides for everything under it, not only the paths that derived
    // it — see this function's own doc for the measured consequence of
    // skipping this and stopping at [`common_directory_pattern`] alone.
    let others = crate::derive::other_series_under_prefix(conn, &pattern, &subject_title, root_id)
        .map_err(|e| format!("checking what else `{pattern}` would decide: {e}"))?;
    if !others.is_empty() {
        // The target stays part of the CHECK above — if it shares this
        // directory, `rules.rs` applies every `set_*` column a matching
        // rule carries, so accepting would stamp `set_edition_key` onto the
        // target's own volumes too and make the base its own colour
        // edition. It is dropped from what is NAMED here only: it is the
        // series being accepted into, not collateral, so listing it beside
        // genuinely unrelated material misreads what it is.
        let collateral: Vec<&str> = others
            .iter()
            .filter(|s| **s != target_title)
            .map(String::as_str)
            .collect();
        let message = if collateral.is_empty() {
            format!(
                "proposal {id}'s directory is shared with \"{target_title}\" itself, so \
                 accepting it would set the colour edition key on {target_title}'s own volumes \
                 too. Narrow the layout or write the rule by hand."
            )
        } else {
            format!(
                "proposal {id}'s directory also holds {}, so accepting it would move that \
                 material too. Narrow the layout or write the rule by hand.",
                collateral.join(", ")
            )
        };
        return Err(message);
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
    // deletes the rule by hand: returning `Err` without `commit` already
    // undoes it.
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
    let set_edition_key = if kind == KIND_EDITION_OF {
        // **A missing prefix relationship is refused, not silently
        // absorbed.** If the target was renamed between this proposal being
        // raised and being accepted, `strip_prefix` finds nothing, and
        // falling back to `None` here would silently restore the very bug
        // this fix exists for: `set_series` alone, the colour volume's pages
        // concatenated onto the base edition's. The failure has to be
        // visible instead.
        let Some(suffix) = subject_title.strip_prefix(&format!("{target_title} - ")) else {
            return Err(format!(
                "proposal {id}: \"{subject_title}\" no longer starts with \"{target_title} - \" \
                 — one of the two series was renamed since this proposal was raised, and \
                 accepting now would set only set_series, silently merging the colour volume's \
                 pages into the base edition. Dismiss this proposal and let the next raise \
                 propose it again against the current titles."
            ));
        };
        // **And an empty slug is refused rather than written.** A suffix
        // with no letters or digits — `"X - ???"` — slugifies to `""`, and
        // `edition.key` is `TEXT NOT NULL`: SQLite takes an empty string
        // happily, and a reader would see an edition with no name at all.
        let slug = slugify(suffix);
        if slug.is_empty() {
            return Err(format!(
                "proposal {id}: the suffix \"{suffix}\" has no letters or digits to make an \
                 edition key from. Write the rule by hand."
            ));
        }
        Some(slug)
    } else {
        None
    };

    let note = format!("accepted from proposal {id}: \"{subject_title}\" is \"{target_title}\"");
    tx.execute(
        "INSERT INTO rule (ordinal, root_id, kind, pattern, match_on, set_series, \
         set_edition_key, note) VALUES (?1, ?2, 'prefix', ?3, 'path', ?4, ?5, ?6)",
        rusqlite::params![
            ordinal,
            root_id,
            pattern,
            target_title,
            set_edition_key,
            note
        ],
    )
    .map_err(|e| format!("writing the rule for proposal {id}: {e}"))?;
    let rule_id = tx.last_insert_rowid();

    // **The load-bearing half.** [`common_directory_pattern`] derives the
    // pattern from `scan_entry` through the rule set as it stands right now,
    // but a pattern correct at derivation time can still match nothing by
    // the time it is checked — and a rule that decides nothing is
    // indistinguishable from a decision that was never made. So this is
    // verified against the raw facts directly, after the write, rather than
    // trusted from how the pattern was derived.
    //
    // **Scoped to `root_id`, same reason `other_series_under_prefix` above
    // is (I6).** The rule just written only ever applies within this root;
    // a path in some other root that happens to share the prefix textually
    // would otherwise read as "matched" while deciding nothing real.
    let matched: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM scan_entry \
             WHERE root_id = ?2 AND substr(rel_path, 1, length(?1)) = ?1",
            rusqlite::params![pattern, root_id],
            |r| r.get(0),
        )
        .map_err(|e| format!("verifying the rule written for proposal {id}: {e}"))?;
    if matched == 0 {
        // **No manual `DELETE` here any more.** Returning `Err` without
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
        let why = if pattern == "/" {
            "this series' files sit directly in the root, so a `path` prefix rule cannot narrow to only them".to_string()
        } else {
            format!("the rule this would write (`{pattern}` on path) matched no scan_entry")
        };
        return Err(format!(
            "proposal {id}: {why} — refusing rather than leaving an accepted proposal whose rule decides nothing"
        ));
    }

    tx.execute(
        "UPDATE proposal SET state = ?1, created_rule_id = ?2 WHERE id = ?3",
        rusqlite::params![STATE_ACCEPTED, rule_id, id],
    )
    .map_err(|e| format!("marking proposal {id} accepted: {e}"))?;

    tx.commit()
        .map_err(|e| format!("committing the accept of proposal {id}: {e}"))?;

    Ok(rule_id)
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

/// The `prefix` pattern [`accept`] would write for `subject_title` — the
/// single directory every one of its currently-resolved
/// [`crate::derive::rel_paths_for_series`] sits under, plus a trailing `/` —
/// and the single root that directory sits under (I6).
///
/// # Errors
///
/// Returns an error naming `proposal_id` if `subject_title`'s entries do not
/// share exactly one directory: none at all (nothing has been scanned for
/// this series), or more than one, in which case every directory found is
/// named — the message [`accept`]'s own doc promises. Also returns an error
/// if that one directory is somehow shared by more than one root (two roots
/// each holding their own directory of that name, say): there is no single
/// root to scope the written rule to, and picking one arbitrarily would
/// leave the pattern silently deciding nothing for the entries under the
/// other.
fn common_directory_pattern(
    conn: &Connection,
    proposal_id: i64,
    subject_title: &str,
) -> Result<(String, i64), String> {
    let paths = crate::derive::rel_paths_for_series(conn, subject_title)?;
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
        return Err(format!(
            "proposal {proposal_id}'s subject spans more than one directory, so accepting it \
             cannot be expressed as one prefix rule. Its entries are under: {listed}"
        ));
    }
    let dir = dirs.into_iter().next().expect("checked len == 1 above");

    // **I6: the rule this writes is scoped to one root, or it is not scoped
    // at all.** `rule.root_id` NULL means "every root, forever" — including a
    // root added after this accept, sharing nothing with the subject beyond a
    // directory name that happens to match. `other_series_under_prefix` and
    // the verification below only ever look at the roots this library holds
    // *right now*; a NULL `root_id` outlives that check.
    let root_ids: std::collections::BTreeSet<i64> = paths.iter().map(|(r, _)| *r).collect();
    if root_ids.len() != 1 {
        let listed = root_paths(conn, &root_ids)?.join(", ");
        return Err(format!(
            "proposal {proposal_id}'s subject sits under `{dir}/` in more than one root, so \
             accepting it cannot be scoped to a single root. Its roots are: {listed}"
        ));
    }
    let root_id = root_ids.into_iter().next().expect("checked len == 1 above");

    Ok((format!("{dir}/"), root_id))
}

/// The `root.path` of every id in `ids`, for [`common_directory_pattern`]'s
/// own refusal message — named roots read better than a set of surrogate
/// keys nobody typed.
fn root_paths(
    conn: &Connection,
    ids: &std::collections::BTreeSet<i64>,
) -> Result<Vec<String>, String> {
    ids.iter()
        .map(|id| {
            conn.query_row("SELECT path FROM root WHERE id = ?1", [id], |r| r.get(0))
                .map_err(|e| format!("looking up root {id}: {e}"))
        })
        .collect()
}

/// Undo [`accept`]: delete the rule it wrote and reopen the proposal.
///
/// # Errors
///
/// Returns an error if `id` names no proposal, if that proposal has no
/// `created_rule_id` (never accepted, or already un-accepted), or if the
/// delete or the update fails.
pub fn unaccept(conn: &Connection, id: i64) -> Result<(), String> {
    let created_rule_id: Option<i64> = conn
        .query_row(
            "SELECT created_rule_id FROM proposal WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| format!("looking up proposal {id}: {e}"))?;
    let Some(rule_id) = created_rule_id else {
        return Err(format!("proposal {id} has no accepted rule to remove"));
    };

    conn.execute("DELETE FROM rule WHERE id = ?1", [rule_id])
        .map_err(|e| format!("removing the rule proposal {id} wrote: {e}"))?;
    // `rule`'s deletion already sets `created_rule_id` to NULL through
    // `ON DELETE SET NULL` (`schema/003_scanner.sql`), but not `state` — that
    // half is `raise`'s job on a rule deleted some *other* way (C4's editor),
    // and un-accepting must not wait for the next raise to take effect.
    conn.execute(
        "UPDATE proposal SET state = ?1, created_rule_id = NULL WHERE id = ?2",
        rusqlite::params![STATE_OPEN, id],
    )
    .map_err(|e| format!("reopening proposal {id}: {e}"))?;
    Ok(())
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
/// # Errors
///
/// Returns an error if `id` names no proposal, if it is currently
/// `accepted` — un-accept it first — or if the update fails.
pub fn dismiss(conn: &Connection, id: i64) -> Result<(), String> {
    let state: String = conn
        .query_row("SELECT state FROM proposal WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .map_err(|e| format!("looking up proposal {id}: {e}"))?;
    if state == STATE_ACCEPTED {
        return Err(format!(
            "proposal {id} is accepted — un-accept it first, then dismiss"
        ));
    }

    conn.execute(
        "UPDATE proposal SET state = ?1 WHERE id = ?2",
        rusqlite::params![STATE_DISMISSED, id],
    )
    .map_err(|e| format!("dismissing proposal {id}: {e}"))?;
    Ok(())
}

// --- raising, per kind -------------------------------------------------------

struct SeriesRow {
    id: i64,
    sort_title: String,
}

fn all_series(conn: &Connection) -> Result<Vec<SeriesRow>, String> {
    let mut stmt = conn
        .prepare("SELECT id, sort_title FROM series ORDER BY id")
        .map_err(|e| format!("listing series: {e}"))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(SeriesRow {
                id: r.get(0)?,
                sort_title: r.get(1)?,
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
            let Some(suffix) = suffixed.sort_title.strip_prefix(prefix.as_str()) else {
                continue;
            };
            if suffix.is_empty() {
                continue;
            }
            let base_volumes = volume_count(conn, base.id)?;
            let suffixed_volumes = volume_count(conn, suffixed.id)?;
            let vocab_hit = EDITION_WORDS.iter().any(|word| suffix.contains(word));
            // **Absence of evidence is not agreement.** `0 == 0` is not two
            // series agreeing about their volumes — it is two series about
            // which the question has not been asked, and counting it as a
            // match would inflate the score of exactly the cases with the
            // least evidence behind them. So the volume signal requires both
            // sides to hold at least one `work` row, not merely equal counts.
            let volumes_match = base_volumes > 0 && base_volumes == suffixed_volumes;
            let mut score = 0.0;
            if vocab_hit {
                score += SIGNAL;
            }
            if volumes_match {
                score += SIGNAL;
            }
            let volume_reason = if base_volumes == 0 && suffixed_volumes == 0 {
                "neither side has any known volumes yet, so there is nothing to compare".to_string()
            } else if volumes_match {
                format!("both hold {base_volumes} volume(s)")
            } else {
                format!("{base_volumes} vs {suffixed_volumes} volumes")
            };
            let reason = format!(
                "\"{}\" may be the \"{suffix}\" edition of \"{}\" — {}; {volume_reason}",
                suffixed.sort_title,
                base.sort_title,
                if vocab_hit {
                    "the suffix matches the edition vocabulary"
                } else {
                    "the suffix matches no edition word"
                },
            );
            newly_open += insert_proposal(
                conn,
                KIND_EDITION_OF,
                suffixed.id,
                Some(base.id),
                score,
                &reason,
            )?;
        }
    }
    Ok(newly_open)
}

/// §5's second use of `series_title`: a series whose own `sort_title` equals
/// some *other* series' known title joins that series.
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
                "SELECT series_id FROM series_title WHERE title = ?1 AND series_id != ?2 \
                 ORDER BY series_id LIMIT 1",
                rusqlite::params![candidate.sort_title, candidate.id],
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
fn raise_attach_chapters(conn: &Connection) -> Result<usize, String> {
    let dirs = crate::derive::unattached_chapter_dirs(conn)?;
    let mut per_series: BTreeMap<String, usize> = BTreeMap::new();
    for series_name in dirs.into_values() {
        *per_series.entry(series_name).or_insert(0) += 1;
    }

    let mut newly_open = 0;
    for (series_name, count) in per_series {
        let Some(series_id) = conn
            .query_row(
                "SELECT id FROM series WHERE sort_title = ?1",
                [&series_name],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| format!("looking up the series `{series_name}`: {e}"))?
        else {
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
fn reopen_orphaned_accepts(conn: &Connection) -> Result<usize, String> {
    conn.execute(
        "UPDATE proposal SET state = ?1 WHERE state = ?2 AND created_rule_id IS NULL",
        rusqlite::params![STATE_OPEN, STATE_ACCEPTED],
    )
    .map_err(|e| format!("reopening accepted proposals whose rule has gone: {e}"))
}

#[cfg(test)]
mod tests;
