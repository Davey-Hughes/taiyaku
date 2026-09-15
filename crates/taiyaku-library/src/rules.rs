//! The rule layer: an ordered, user-editable set of rules that maps a
//! `rel_path` onto a series, an edition key, a volume, a chapter and so on.
//!
//! §4 of the scanner spec. `scan.rs` writes facts and decides nothing about
//! them; this module is the layer above it that decides. Nothing here opens a
//! file — an [`Engine`] is built from rows already read, and [`Engine::resolve`]
//! takes a [`Root`] and a `rel_path` rather than a path on disk, which is what
//! makes editing a rule able to re-derive the whole library without touching
//! the filesystem: the facts `scan.rs` already wrote are all a re-derive ever
//! needs.
//!
//! # Ordered layering, per field
//!
//! Every matching rule applies, in `ordinal` order, and a later rule
//! overrides **only the fields it actually sets**. "Everything under this
//! root is raw" and "except these are the colour edition" are two rules
//! rather than four, because the second sets `edition_key` and nothing else —
//! the first rule's `language` survives it. First-match-wins was rejected
//! because every exception would have to restate the general case;
//! most-specific-wins was rejected because the engine would then be making a
//! judgement the user cannot see or reorder.
//!
//! Because the effective value of a field comes from whichever rule set it
//! last, [`Resolved::provenance`] records, per field, the id of the rule that
//! decided it — the sentinel `0` for a root's own default column, and no
//! entry at all where nothing decided the field. A later spec displays this;
//! without it the layering is unexplainable.
//!
//! # Four kinds
//!
//! - [`RuleKind::Prefix`] — the one-click case, "this whole directory is
//!   raw": `pattern` is tested as a literal prefix of whichever string
//!   `match_on` names, and only the `set_*` columns apply. No pattern to
//!   write for the general case.
//! - [`RuleKind::Regex`] — `pattern` is a compiled [`regex::Regex`], matched
//!   against the same string. The `set_*` columns apply on any match, and so
//!   do the named captures `series`, `volume`, `chapter`, `edition` and
//!   `title` where present.
//! - [`RuleKind::BuiltinVolume`] / [`RuleKind::BuiltinChapter`] — hand the
//!   `match_on` string to [`taiyaku_read::volume_number_marked`] /
//!   [`taiyaku_read::chapter_number`], rather than a second, drifting
//!   reimplementation as a regex default. **Not**
//!   [`taiyaku_read::volume_number`], which `inventory::walk` still calls
//!   unmodified: that one falls back to a bare digit run when no marker
//!   appears, right for a directory that is a volume by construction and
//!   wrong for a rule applied to every filename in a library, where a bare
//!   `c001` would otherwise read as volume 1.
//!
//! # A malformed pattern disables its own rule and nothing else
//!
//! Rules arrive from a text box (spec C4's editor) and from imported JSON, so
//! a scan of a real library must not die because one rule does not compile.
//! [`Engine::compile`] returns one complaint per pattern that would not
//! compile, rather than failing outright or logging where a caller cannot
//! control it — the caller (`library scan`, and later the API) decides how
//! loudly to say so. The rule itself compiles to nothing, matches nothing,
//! and every other rule is unaffected.
//!
//! # Compiled once, at load
//!
//! Every regex in the rule set is compiled exactly once, when the engine is
//! built. Compiling per entry would recompile the same handful of patterns
//! for every file in a library of thousands, and `Regex::new` is the
//! expensive call here by an order of magnitude next to matching against an
//! already-compiled one.
//!
//! # `compile` and `load`
//!
//! [`Engine::compile`] takes rules already loaded from SQLite; [`Engine::load`]
//! is the thin wrapper that reads the rows and calls it. The split is the same
//! one [`crate::require_fts5`] makes over [`crate::require_fts5_with_probe`],
//! and for the same reason: the branch worth pinning — a pattern that will
//! not compile, and the layering behaviour built on top of it — is the one a
//! database cannot easily be made to produce, so every test in this module
//! reaches `compile` directly and never opens a connection at all.

use std::collections::BTreeMap;
use std::path::Path;
use std::str::FromStr;

use regex::Regex;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::scan::Root;

/// One field an [`Engine`] can decide, and the key [`Resolved::provenance`]
/// tracks per field.
///
/// Declared in the order the store spec lists them (§4): series, the edition
/// key, the material kind, the language, the volume and chapter numbers, and
/// the title.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Field {
    Series,
    EditionKey,
    Kind,
    Language,
    Volume,
    Chapter,
    Title,
}

/// The sentinel [`Resolved::provenance`] records for a field decided by the
/// root's own default column rather than by any rule — distinguishable from
/// no entry at all, which means nothing decided the field.
const ROOT_FALLBACK: i64 = 0;

/// Whether `id` is [`ROOT_FALLBACK`] — the question a caller outside this
/// module asks instead of comparing against the literal `0` itself.
///
/// **`ROOT_FALLBACK` stays private; this is the only way in.**
/// `crate::preview`'s matched-files table has to tell "the root's own column
/// decided this" apart from "a rule decided this" on the way out to a wire
/// format, and the sentinel that distinguishes them here is an implementation
/// detail of a `BTreeMap<Field, i64>` — not a rule id, and not something a
/// client should ever have to know to special-case. Exporting the constant
/// itself would leave every caller writing `id == rules::ROOT_FALLBACK`, one
/// `==` away from the literal `0` reappearing at the boundary this function
/// exists to keep it off.
#[must_use]
pub fn is_root_fallback(id: i64) -> bool {
    id == ROOT_FALLBACK
}

/// The four shapes a `rule` row's `kind` column takes.
///
/// `Field` is a different enum — this one is the *kind of rule*, not a
/// property it can set — and the two are kept apart on purpose: a rule whose
/// `kind` is [`RuleKind::Regex`] can still set `Field::Kind` (the resolved
/// work's material kind) through its `set_kind` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleKind {
    Prefix,
    Regex,
    BuiltinVolume,
    BuiltinChapter,
}

impl FromStr for RuleKind {
    type Err = String;

    /// # Errors
    ///
    /// Returns an error naming `s` if it is none of the four values the
    /// `rule.kind` column holds.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "prefix" => Ok(Self::Prefix),
            "regex" => Ok(Self::Regex),
            "builtin-volume" => Ok(Self::BuiltinVolume),
            "builtin-chapter" => Ok(Self::BuiltinChapter),
            other => Err(format!("unknown rule kind {other:?}")),
        }
    }
}

impl std::fmt::Display for RuleKind {
    /// The exact inverse of [`FromStr::from_str`] above, kept beside it so
    /// the two cannot drift — [`replace_all`] writes this back to the same
    /// `rule.kind` column [`read`] parses it from.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Prefix => "prefix",
            Self::Regex => "regex",
            Self::BuiltinVolume => "builtin-volume",
            Self::BuiltinChapter => "builtin-chapter",
        })
    }
}

/// Which of a `rel_path`'s three derived strings a rule is tested against.
///
/// - [`MatchOn::Path`] — the whole `rel_path`, unmodified.
/// - [`MatchOn::Name`] — its last component (a file's name, or a directory's
///   own name if the path names a directory).
/// - [`MatchOn::Parent`] — the *name* of its immediate parent directory, not
///   the parent's own full path — `"Full Color"` for
///   `"Full Color/Series - v01.cbz"`, not `"Full Color"` prefixed by
///   anything above it. A path with no parent component (a file directly
///   under the root) matches the empty string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchOn {
    Path,
    Name,
    Parent,
}

impl FromStr for MatchOn {
    type Err = String;

    /// # Errors
    ///
    /// Returns an error naming `s` if it is none of `path`, `name` or
    /// `parent`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "path" => Ok(Self::Path),
            "name" => Ok(Self::Name),
            "parent" => Ok(Self::Parent),
            other => Err(format!("unknown match_on {other:?}")),
        }
    }
}

impl std::fmt::Display for MatchOn {
    /// The exact inverse of [`FromStr::from_str`] above, for the same reason
    /// [`RuleKind`]'s own `Display` sits beside its `FromStr`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Path => "path",
            Self::Name => "name",
            Self::Parent => "parent",
        })
    }
}

/// A projection of one `rule` row (`schema/003_scanner.sql`).
///
/// `root_id` of `None` means every root; `Some(id)` scopes the rule to one
/// root, so a second library sharing no rules with the first is possible
/// without every rule restating which root it is for.
///
/// There is no `set_volume` or `set_chapter`: a literal number is a property
/// of one file, not of a rule that matches many — those two fields only ever
/// come from a regex capture or a builtin extractor, both per-file by
/// construction.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub id: i64,
    pub ordinal: i64,
    pub enabled: bool,
    pub root_id: Option<i64>,
    pub kind: RuleKind,
    pub pattern: String,
    pub match_on: MatchOn,
    pub set_edition_key: Option<String>,
    pub set_kind: Option<String>,
    pub set_language: Option<String>,
    pub set_series: Option<String>,
    pub note: Option<String>,
}

/// What an [`Engine`] decided for one `rel_path`, and which rule decided each
/// field it holds.
///
/// `volume` and `chapter` are `f64` because [`taiyaku_read::chapter_number`]
/// already is — a chapter can be `12.5` — and a regex capture parses into the
/// same type as the builtin extractor so the two are interchangeable per
/// field rather than one being a narrower answer than the other.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub series: Option<String>,
    pub edition_key: Option<String>,
    pub kind: Option<String>,
    pub language: Option<String>,
    pub volume: Option<f64>,
    pub chapter: Option<f64>,
    pub title: Option<String>,
    pub provenance: BTreeMap<Field, i64>,
}

impl Resolved {
    fn empty() -> Self {
        Self {
            series: None,
            edition_key: None,
            kind: None,
            language: None,
            volume: None,
            chapter: None,
            title: None,
            provenance: BTreeMap::new(),
        }
    }

    /// Set one string field and record which rule decided it, overwriting
    /// whatever an earlier rule recorded — the whole of "a later rule
    /// overrides only the fields it actually sets" is this one line, called
    /// once per field a rule actually provides.
    fn set(&mut self, field: Field, rule_id: i64, value: String) {
        match field {
            Field::Series => self.series = Some(value),
            Field::EditionKey => self.edition_key = Some(value),
            Field::Kind => self.kind = Some(value),
            Field::Language => self.language = Some(value),
            Field::Title => self.title = Some(value),
            Field::Volume | Field::Chapter => {
                unreachable!("volume and chapter are set through `set_number`, not `set`")
            }
        }
        self.provenance.insert(field, rule_id);
    }

    /// **A non-finite value sets nothing at all**, exactly as a capture that
    /// does not parse sets nothing.
    ///
    /// `str::parse::<f64>()` accepts `nan`, `inf`, `infinity` and overflowing
    /// literals like `1e999`, so a capture group aimed at a filename and
    /// slightly too wide really does produce one — `^(?<volume>Nan)` off
    /// `Nanatsu no Taizai` is the case this was found through, a real series
    /// title rather than anything contrived. There is no volume or chapter a
    /// non-finite could mean, and letting one through made three separate
    /// things wrong downstream, each found by a different review pass of the
    /// O95 branch: `preview::overrides` reported a chain in which *every*
    /// rule produced `NaN` as a disagreement with itself, since `NaN != NaN`;
    /// `config_api::field_value_json` collapsed every non-finite onto the same
    /// `null` that means "this field is unset", because JSON holds neither;
    /// and the editor then drew "rule 3 overrides rule 2 on N files" above two
    /// sides both reading *unset*. One cause, refused once, here.
    ///
    /// **Guarded here rather than at `apply_captures`' two call sites**,
    /// because `RuleKind::BuiltinVolume` and `BuiltinChapter` reach this
    /// function by a different path and a guard per caller is a guard that
    /// grows a hole when somebody adds a third.
    ///
    /// **Rejected: comparing non-finites as equal in `preview::FieldValue`.**
    /// That was the first fix written for the `NaN` half and it is wrong for
    /// the other: `inf` and `NaN` are genuinely *different* resolutions, so
    /// treating every non-finite pair as one answer would suppress a real
    /// disagreement to hide a rendering problem.
    fn set_number(&mut self, field: Field, rule_id: i64, value: f64) {
        if !value.is_finite() {
            return;
        }
        match field {
            Field::Volume => self.volume = Some(value),
            Field::Chapter => self.chapter = Some(value),
            _ => unreachable!("only volume and chapter are set through `set_number`"),
        }
        self.provenance.insert(field, rule_id);
    }
}

/// One rule, with its regex compiled if it has one.
///
/// Kept as a matched pair rather than two parallel `Vec`s so the two can
/// never drift out of index alignment — see delta review notes across this
/// crate on exactly that shape of bug elsewhere.
#[derive(Debug)]
struct Compiled {
    rule: Rule,
    regex: Option<Regex>,
}

/// The compiled rule set: every enabled, in-scope rule this library holds,
/// in `ordinal` order, ready to [`Engine::resolve`] against any `rel_path`
/// with no further compilation.
#[derive(Debug)]
pub struct Engine {
    rules: Vec<Compiled>,
}

impl Engine {
    /// Compile `rules`, sorted into `ordinal` order, into an [`Engine`] — and
    /// one complaint per pattern that would not compile.
    ///
    /// **Sorted here rather than trusted from the caller.** `Engine`'s own
    /// doc promises `ordinal` order, and sorting inside `compile` is what
    /// makes that true of every `Engine` regardless of what order a caller
    /// hands `rules` in — every test in this module, and `load`'s own
    /// `ORDER BY`, already arrange rules in order before calling this, but
    /// `compile` is `pub` and nothing stops a future caller handing it rules
    /// in another order. Ties on `ordinal` break on `id`, the same tie-break
    /// `load`'s query now names explicitly, so two rules sharing an
    /// `ordinal` still resolve to one deterministic order rather than an
    /// unspecified one.
    ///
    /// **A bad pattern disables its own rule and nothing else.** Only a
    /// [`RuleKind::Regex`] rule can fail here — [`RuleKind::Prefix`] has no
    /// pattern to compile, and the two builtin kinds ignore `pattern`
    /// entirely — and a failure removes that one rule from matching while
    /// leaving every other rule, and the order between them, untouched: a
    /// rule that comes after a bad pattern still applies.
    ///
    /// Every rule's pattern is attempted regardless of `enabled`, so a
    /// disabled rule with a bad pattern is still named in the complaints —
    /// useful to an editor showing every problem rather than only the live
    /// ones — but a disabled rule never matches anything in
    /// [`Engine::resolve`], compiled or not.
    #[must_use]
    pub fn compile(rules: &[Rule]) -> (Self, Vec<String>) {
        let mut ordered: Vec<&Rule> = rules.iter().collect();
        ordered.sort_by_key(|rule| (rule.ordinal, rule.id));

        let mut compiled = Vec::with_capacity(ordered.len());
        let mut complaints = Vec::new();
        for rule in ordered {
            let regex = if rule.kind == RuleKind::Regex {
                match Regex::new(&rule.pattern) {
                    Ok(re) => Some(re),
                    Err(e) => {
                        complaints.push(format!(
                            "rule {}: pattern {:?} does not compile: {e}",
                            rule.id, rule.pattern
                        ));
                        None
                    }
                }
            } else {
                None
            };
            compiled.push(Compiled {
                rule: rule.clone(),
                regex,
            });
        }
        (Self { rules: compiled }, complaints)
    }

    /// Every rule, compiled, in `ordinal` order — and one complaint per
    /// pattern that would not compile.
    ///
    /// **A bad pattern disables its own rule and nothing else.** These arrive
    /// from a text box (spec C4's editor) and from imported JSON, so a scan
    /// of a real library must not die because one rule is malformed. The
    /// complaints are returned rather than logged, so the caller —
    /// `library scan`, and later the API — decides how loudly to say so; a
    /// rule silently doing nothing is the outcome this avoids.
    ///
    /// # Errors
    ///
    /// Returns an error only if SQLite refuses the query. A malformed
    /// pattern is a complaint, not an error.
    pub fn load(conn: &Connection) -> Result<(Self, Vec<String>), String> {
        let (rules, mut complaints) = read(conn)?;
        let (engine, compile_complaints) = Self::compile(&rules);
        complaints.extend(compile_complaints);
        Ok((engine, complaints))
    }

    /// Resolve `rel_path`, under `root`, against every rule in this engine —
    /// each that matches applies, in order, setting only the fields it
    /// provides — and fall back to `root`'s own defaults for whatever no
    /// rule decided.
    #[must_use]
    pub fn resolve(&self, root: &Root, rel_path: &str) -> Resolved {
        let mut resolved = Resolved::empty();
        for compiled in &self.rules {
            let rule = &compiled.rule;
            if !rule.enabled {
                continue;
            }
            if rule.root_id.is_some_and(|scoped_to| scoped_to != root.id) {
                continue;
            }
            let target = match_target(rule.match_on, rel_path);
            // Whether this rule matched `target` at all, per its own kind,
            // and — for `Regex` only — the captures that match produced.
            // Applying them is deferred to below the match, alongside the
            // literal `set_*` columns, so the two apply in the same order
            // `apply_set_columns` then `apply_captures` this function has
            // always used: a named capture must still be able to override a
            // literal `set_series` on the same rule, not the other way
            // round.
            let (matched, captures) = match rule.kind {
                RuleKind::Prefix => (target.starts_with(rule.pattern.as_str()), None),
                RuleKind::Regex => {
                    let Some(regex) = &compiled.regex else {
                        // Did not compile — `Engine::compile` already
                        // complained about it. Matches nothing.
                        continue;
                    };
                    match regex.captures(target) {
                        Some(caps) => (true, Some(caps)),
                        None => (false, None),
                    }
                }
                RuleKind::BuiltinVolume => match taiyaku_read::volume_number_marked(target) {
                    Some(v) => {
                        resolved.set_number(Field::Volume, rule.id, v);
                        (true, None)
                    }
                    None => (false, None),
                },
                RuleKind::BuiltinChapter => match taiyaku_read::chapter_number(target) {
                    Some(c) => {
                        resolved.set_number(Field::Chapter, rule.id, c);
                        (true, None)
                    }
                    None => (false, None),
                },
            };
            // The literal `set_*` columns first, so a `Regex` rule's own
            // named capture (applied next) still gets the final word on any
            // field both provide — see the comment above.
            if matched && kind_applies_set_columns(rule.kind) {
                apply_set_columns(rule, &mut resolved);
            }
            if let Some(caps) = captures {
                apply_captures(rule, &caps, &mut resolved);
            }
        }
        apply_root_fallback(root, &mut resolved);
        resolved
    }
}

/// Whether [`Engine::resolve`] ever applies a rule's four literal `set_*`
/// columns for a rule of this `kind` — true for [`RuleKind::Prefix`] and
/// [`RuleKind::Regex`], false for [`RuleKind::BuiltinVolume`] and
/// [`RuleKind::BuiltinChapter`], which never look at `set_series`,
/// `set_edition_key`, `set_kind` or `set_language` no matter what a row
/// happens to carry in those columns — the schema has no `CHECK` forbidding
/// a builtin rule from carrying one, and `import_json` does not reject one
/// either.
///
/// **The one place `resolve` and [`fields_settable_by`] both ask this
/// question, so the two cannot silently disagree about it.** `resolve` gates
/// its own call to [`apply_set_columns`] on this function's answer;
/// `fields_settable_by` gates whether it reports any of the four `set_*`
/// fields at all on the same call. Before this existed, `fields_settable_by`
/// checked `rule.set_*.is_some()` unconditionally, so a `BuiltinVolume` rule
/// carrying a stray `set_series` was reported as able to decide `Series` —
/// a field `resolve` can never actually set for that kind — which made
/// `crate::preview::rows_for`'s near-miss check demand `Series` be undecided
/// or root-decided before it would call a genuine `Volume` miss a near-miss
/// at all, excluding rows the tab exists to show. If `resolve` ever starts
/// or stops applying `set_*` for a kind, this function's arms must move with
/// it — one function to update rather than two lists that can drift.
fn kind_applies_set_columns(kind: RuleKind) -> bool {
    matches!(kind, RuleKind::Prefix | RuleKind::Regex)
}

/// Every rule in the database, in `ordinal, id` order, with the complaints
/// raised by rows whose `kind` or `match_on` would not parse.
///
/// Split out of [`Engine::load`] so a caller can add a rule that is not in
/// the table yet — `proposals::resolves` asks what a *candidate* rule would
/// decide, and the only honest way to answer is to put it in the set the real
/// engine compiles.
///
/// # Errors
///
/// Only if SQLite refuses the query. A row that will not parse is a
/// complaint, exactly as it was inside `load`.
pub fn read(conn: &Connection) -> Result<(Vec<Rule>, Vec<String>), String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, ordinal, enabled, root_id, kind, pattern, match_on, \
             set_edition_key, set_kind, set_language, set_series, note \
             FROM rule ORDER BY ordinal, id",
        )
        .map_err(|e| format!("loading rules: {e}"))?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)? != 0,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, Option<String>>(7)?,
                r.get::<_, Option<String>>(8)?,
                r.get::<_, Option<String>>(9)?,
                r.get::<_, Option<String>>(10)?,
                r.get::<_, Option<String>>(11)?,
            ))
        })
        .map_err(|e| format!("loading rules: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("loading rules: {e}"))?;

    let mut rules = Vec::with_capacity(rows.len());
    let mut complaints = Vec::new();
    for (
        id,
        ordinal,
        enabled,
        root_id,
        kind,
        pattern,
        match_on,
        set_ek,
        set_k,
        set_l,
        set_s,
        note,
    ) in rows
    {
        // `kind` and `match_on` are written only through validated paths
        // — C4's editor offers a fixed set, and an imported rule is
        // expected to name one of them — so a row that fails to parse
        // either is unreachable in practice. Handled as a complaint
        // rather than trusted with `.expect`, or promoted to the one
        // `Err` this function has (SQLite refusing the query): the same
        // "never die over one bad row" reasoning that governs a bad
        // pattern applies just as well to a bad `kind` string, and this
        // function's own doc says the only `Err` case is SQLite itself.
        let kind = match kind.parse::<RuleKind>() {
            Ok(k) => k,
            Err(e) => {
                complaints.push(format!("rule {id}: {e}"));
                continue;
            }
        };
        let match_on = match match_on.parse::<MatchOn>() {
            Ok(m) => m,
            Err(e) => {
                complaints.push(format!("rule {id}: {e}"));
                continue;
            }
        };
        rules.push(Rule {
            id,
            ordinal,
            enabled,
            root_id,
            kind,
            pattern,
            match_on,
            set_edition_key: set_ek,
            set_kind: set_k,
            set_language: set_l,
            set_series: set_s,
            note,
        });
    }

    Ok((rules, complaints))
}

/// The string one `match_on` value picks out of `rel_path`, per this
/// module's doc comment on [`MatchOn`].
fn match_target(match_on: MatchOn, rel_path: &str) -> &str {
    match match_on {
        MatchOn::Path => rel_path,
        MatchOn::Name => Path::new(rel_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(rel_path),
        MatchOn::Parent => Path::new(rel_path)
            .parent()
            .and_then(Path::file_name)
            .and_then(|s| s.to_str())
            .unwrap_or(""),
    }
}

/// Apply a rule's four literal `set_*` columns — the part every kind of rule
/// shares once it has matched at all.
fn apply_set_columns(rule: &Rule, resolved: &mut Resolved) {
    if let Some(v) = &rule.set_series {
        resolved.set(Field::Series, rule.id, v.clone());
    }
    if let Some(v) = &rule.set_edition_key {
        resolved.set(Field::EditionKey, rule.id, v.clone());
    }
    if let Some(v) = &rule.set_kind {
        resolved.set(Field::Kind, rule.id, v.clone());
    }
    if let Some(v) = &rule.set_language {
        resolved.set(Field::Language, rule.id, v.clone());
    }
}

/// Apply whichever of a [`RuleKind::Regex`] rule's named captures are
/// present and non-empty: `series`, `edition`, `title` as literal strings,
/// `volume` and `chapter` parsed as `f64`.
///
/// A capture that will not parse as a number sets nothing and is not an
/// error — a rule matching a name whose digits are not a number is the rule
/// being wrong about that one file, not the library being broken. String
/// captures get the same treatment for the same reason, just against
/// emptiness rather than a parse failure: `(?<series>x*)` matching zero `x`s
/// is a rule that matched but found nothing, and letting that set
/// `series = Some("")` over an earlier rule's real, directory-derived name
/// would be the layering model silently erasing a field a later rule never
/// actually decided.
fn apply_captures(rule: &Rule, caps: &regex::Captures<'_>, resolved: &mut Resolved) {
    if let Some(m) = caps.name("series").filter(|m| !m.as_str().is_empty()) {
        resolved.set(Field::Series, rule.id, m.as_str().to_string());
    }
    if let Some(m) = caps.name("edition").filter(|m| !m.as_str().is_empty()) {
        resolved.set(Field::EditionKey, rule.id, m.as_str().to_string());
    }
    if let Some(m) = caps.name("title").filter(|m| !m.as_str().is_empty()) {
        resolved.set(Field::Title, rule.id, m.as_str().to_string());
    }
    if let Some(m) = caps.name("volume") {
        if let Ok(v) = m.as_str().parse::<f64>() {
            resolved.set_number(Field::Volume, rule.id, v);
        }
    }
    if let Some(m) = caps.name("chapter") {
        if let Ok(c) = m.as_str().parse::<f64>() {
            resolved.set_number(Field::Chapter, rule.id, c);
        }
    }
}

/// Every [`Field`] `rule` could possibly decide, independent of whether it
/// actually matches any particular `rel_path` — the static half of what
/// [`Engine::resolve`] computes dynamically through [`apply_set_columns`] and
/// [`apply_captures`].
///
/// **For `crate::preview`'s near-miss definition**, which has to ask "did
/// this rule miss this entry, or did it simply have nothing to say about it
/// at all" — and answering that needs to know what the rule was even trying
/// to set, not only what it happened to set on the entries it matched.
///
/// A [`RuleKind::Regex`] pattern that fails to compile contributes only its
/// static `set_*` columns: [`Engine::compile`] already turns a bad pattern
/// into a rule that matches nothing, and there is no capture list to ask a
/// pattern that never parsed for.
///
/// **Must report only what `resolve` would actually apply for this rule's
/// `kind`, or the near-miss check built on it is wrong by construction.**
/// [`Engine::resolve`] only ever applies a rule's literal `set_*` columns for
/// [`RuleKind::Prefix`] and [`RuleKind::Regex`] — never for either builtin
/// kind — so this function gates those four checks on
/// [`kind_applies_set_columns`], the exact same predicate `resolve` gates its
/// own [`apply_set_columns`] call on. See that function's doc comment for
/// what goes wrong when the two disagree, and why they are kept as one
/// function rather than two hand-maintained lists of kinds.
#[must_use]
pub(crate) fn fields_settable_by(rule: &Rule) -> std::collections::BTreeSet<Field> {
    let mut fields = std::collections::BTreeSet::new();
    if kind_applies_set_columns(rule.kind) {
        if rule.set_series.is_some() {
            fields.insert(Field::Series);
        }
        if rule.set_edition_key.is_some() {
            fields.insert(Field::EditionKey);
        }
        if rule.set_kind.is_some() {
            fields.insert(Field::Kind);
        }
        if rule.set_language.is_some() {
            fields.insert(Field::Language);
        }
    }
    match rule.kind {
        RuleKind::Regex => {
            if let Ok(re) = Regex::new(&rule.pattern) {
                for name in re.capture_names().flatten() {
                    let field = match name {
                        "series" => Some(Field::Series),
                        "edition" => Some(Field::EditionKey),
                        "title" => Some(Field::Title),
                        "volume" => Some(Field::Volume),
                        "chapter" => Some(Field::Chapter),
                        _ => None,
                    };
                    if let Some(field) = field {
                        fields.insert(field);
                    }
                }
            }
        }
        RuleKind::BuiltinVolume => {
            fields.insert(Field::Volume);
        }
        RuleKind::BuiltinChapter => {
            fields.insert(Field::Chapter);
        }
        RuleKind::Prefix => {}
    }
    fields
}

/// Fields no rule decided fall back to `root`'s own `edition_key`,
/// `default_kind` and `default_language` columns — §3 of the store spec:
/// "what this root's material becomes by default; B's rules layer over
/// them." Recorded in `provenance` as [`ROOT_FALLBACK`], so "the root
/// decided this" is distinguishable from "nothing decided this" (no entry at
/// all, left where [`Resolved::empty`] put it).
fn apply_root_fallback(root: &Root, resolved: &mut Resolved) {
    if resolved.edition_key.is_none() {
        if let Some(v) = &root.edition_key {
            resolved.edition_key = Some(v.clone());
            resolved.provenance.insert(Field::EditionKey, ROOT_FALLBACK);
        }
    }
    if resolved.kind.is_none() {
        if let Some(v) = &root.default_kind {
            resolved.kind = Some(v.clone());
            resolved.provenance.insert(Field::Kind, ROOT_FALLBACK);
        }
    }
    if resolved.language.is_none() {
        if let Some(v) = &root.default_language {
            resolved.language = Some(v.clone());
            resolved.provenance.insert(Field::Language, ROOT_FALLBACK);
        }
    }
}

// --- single-row CRUD: create, patch, delete ------------------------------
//
// [`replace_all`], below, is what the CLI and JSON import need: hand it the
// whole rule list, get the whole list back written. C4's editor works a row
// at a time instead — add one rule, patch one field, flip `enabled`, drag an
// ordinal past its neighbour, delete one — and driving each of those through
// a read-modify-`replace_all` round trip would ship the *entire* list back
// to the database for a one-field edit, racing every other row against
// whatever else touched the table in between. The three functions below
// write only the row (for [`delete`], the rule and, where one wrote it, the
// proposal it undoes) each operation actually changes.

/// A rule not yet written — everything [`create`] needs to insert one row.
///
/// **`ordinal: None` appends past the current maximum rather than defaulting
/// to `0`.** [`read`] and [`Engine::compile`] both sort `(ordinal, id)`, so a
/// rule minted at `0` would jump to the front of the layering order the
/// moment nobody names one — silently overriding every rule already there —
/// rather than landing harmlessly at the end, which is what "I typed a new
/// rule into the editor and never dragged it anywhere" has to mean. See
/// [`create`]'s own doc comment for how that maximum is found, and for the
/// overflow hazard this crate has already measured once
/// (`crate::tests::LAST_WORD_ORDINAL`'s own doc comment) guarded against.
#[derive(Debug, Clone)]
pub struct NewRule {
    pub ordinal: Option<i64>,
    pub enabled: bool,
    pub root_id: Option<i64>,
    pub kind: RuleKind,
    pub pattern: String,
    pub match_on: MatchOn,
    pub set_edition_key: Option<String>,
    pub set_kind: Option<String>,
    pub set_language: Option<String>,
    pub set_series: Option<String>,
    pub note: Option<String>,
}

/// A change to one existing rule — everything [`update`] needs to patch a
/// row without touching the columns nobody named.
///
/// `Default`-derived, so a caller names only what it changes:
/// `RulePatch { enabled: Some(false), ..Default::default() }` flips one
/// column and leaves the other ten exactly as they were.
///
/// **`root_id`, `note` and the four `set_*` columns are `Option<Option<_>>`,
/// not `Option<_>`.** Every one of those six is nullable in
/// `schema/003_scanner.sql`, and a patch has to be able to say two different
/// things about a nullable column: "leave it" and "clear it to NULL" — the
/// outer `None` is the first, `Some(None)` the second. A single-layer
/// `Option<String>` cannot carry that distinction: there would be no way to
/// spell `Some(None)` at all, and clearing a `set_series` back to NULL — how
/// a rule stops setting a field it used to set — would become inexpressible
/// through this type. The other five columns (`ordinal`, `enabled`, `kind`,
/// `pattern`, `match_on`) are `NOT NULL` in the schema, so a caller can
/// replace their value or leave it alone but never clear it, exactly what a
/// single-layer `Option` already says.
#[derive(Debug, Clone, Default)]
pub struct RulePatch {
    pub ordinal: Option<i64>,
    pub enabled: Option<bool>,
    pub root_id: Option<Option<i64>>,
    pub kind: Option<RuleKind>,
    pub pattern: Option<String>,
    pub match_on: Option<MatchOn>,
    pub set_edition_key: Option<Option<String>>,
    pub set_kind: Option<Option<String>>,
    pub set_language: Option<Option<String>>,
    pub set_series: Option<Option<String>>,
    pub note: Option<Option<String>>,
}

/// Insert one new rule and report the id SQLite minted for it.
///
/// **Defaults `ordinal` to one past the current maximum, computed in Rust
/// rather than folded into the `INSERT` as SQL arithmetic.** A bare
/// `COALESCE(MAX(ordinal), 4) + 1` —
/// [`crate::proposals::accept_with`]'s own version of this exact
/// computation — overflows the moment any rule holds `i64::MAX`: measured
/// for real, once, by a fixture that gave a rule exactly that ordinal to
/// guarantee it always sorted last
/// (`crate::tests::LAST_WORD_ORDINAL`'s own doc comment tells the story, and
/// why that fixture no longer uses `i64::MAX` either). This function reads
/// `MAX(ordinal)` and adds one with [`i64::saturating_add`] instead of a
/// bare `+`, so a rule already sitting at `i64::MAX` gives the new one
/// `i64::MAX` too, rather than panicking (a debug build) or silently
/// wrapping to `i64::MIN` (a release one) — either of which would make
/// "append" mean "prepend." A tie at the ceiling still reads back in the
/// right order regardless: [`read`] breaks a tied `ordinal` on `id`, and
/// this function's own `INSERT` always mints a fresh id higher than
/// anything already in the table, so the newly created rule still sorts
/// after the one it tied with.
///
/// An empty table — nothing to take a maximum of at all — defaults to
/// ordinal `0`: there is no "past the maximum" to be one past, and `0` is no
/// different from any other number when it is the only rule in the table.
///
/// # Errors
///
/// Returns an error if SQLite refuses reading the current maximum ordinal or
/// refuses the insert — a foreign key violation on `root_id` among them.
pub fn create(conn: &Connection, new: &NewRule) -> Result<i64, String> {
    let ordinal = match new.ordinal {
        Some(ordinal) => ordinal,
        None => {
            let max: Option<i64> = conn
                .query_row("SELECT MAX(ordinal) FROM rule", [], |r| r.get(0))
                .map_err(|e| format!("reading the current maximum ordinal: {e}"))?;
            max.map_or(0, |m| m.saturating_add(1))
        }
    };
    conn.execute(
        "INSERT INTO rule (ordinal, enabled, root_id, kind, pattern, match_on, \
         set_edition_key, set_kind, set_language, set_series, note) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            ordinal,
            new.enabled,
            new.root_id,
            new.kind.to_string(),
            new.pattern,
            new.match_on.to_string(),
            new.set_edition_key,
            new.set_kind,
            new.set_language,
            new.set_series,
            new.note,
        ],
    )
    .map_err(|e| format!("creating a rule: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// Patch rule `id` from whichever fields of `patch` are `Some`, and report
/// whether `id` named a row at all.
///
/// **Builds one `UPDATE` naming only the patched columns** — the same
/// variable-clause shape [`crate::proposals::list`] already builds its
/// `WHERE` from for [`Filter`](crate::proposals::Filter) — a placeholder and
/// a positional parameter per field actually present in `patch`, nothing
/// written for a field left `None`. The alternative — always writing every
/// column, refilling whatever the caller didn't patch from a fresh read — is
/// the bug this exists to avoid: `RulePatch { enabled: Some(false),
/// ..Default::default() }` must flip `enabled` alone, not silently clear
/// every `set_*` column back to whatever that extra read happened to find.
///
/// **Reports whether a row existed to patch, not whether any value on it
/// actually differed.** A patch naming nothing at all (`sets` ends up empty)
/// is its own case: there is no valid `UPDATE ... SET` with an empty list,
/// so this asks for `id`'s existence directly instead of issuing a
/// statement that would be a syntax error.
///
/// # Errors
///
/// Only if SQLite refuses the read or the write — a foreign key violation on
/// a patched `root_id` among them. An `id` naming no rule is `Ok(false)`,
/// not an error, and patches nothing.
pub fn update(conn: &Connection, id: i64, patch: &RulePatch) -> Result<bool, String> {
    let mut sets: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(v) = patch.ordinal {
        params.push(Box::new(v));
        sets.push(format!("ordinal = ?{}", params.len()));
    }
    if let Some(v) = patch.enabled {
        params.push(Box::new(v));
        sets.push(format!("enabled = ?{}", params.len()));
    }
    if let Some(v) = patch.root_id {
        params.push(Box::new(v));
        sets.push(format!("root_id = ?{}", params.len()));
    }
    if let Some(v) = &patch.kind {
        params.push(Box::new(v.to_string()));
        sets.push(format!("kind = ?{}", params.len()));
    }
    if let Some(v) = &patch.pattern {
        params.push(Box::new(v.clone()));
        sets.push(format!("pattern = ?{}", params.len()));
    }
    if let Some(v) = &patch.match_on {
        params.push(Box::new(v.to_string()));
        sets.push(format!("match_on = ?{}", params.len()));
    }
    if let Some(v) = &patch.set_edition_key {
        params.push(Box::new(v.clone()));
        sets.push(format!("set_edition_key = ?{}", params.len()));
    }
    if let Some(v) = &patch.set_kind {
        params.push(Box::new(v.clone()));
        sets.push(format!("set_kind = ?{}", params.len()));
    }
    if let Some(v) = &patch.set_language {
        params.push(Box::new(v.clone()));
        sets.push(format!("set_language = ?{}", params.len()));
    }
    if let Some(v) = &patch.set_series {
        params.push(Box::new(v.clone()));
        sets.push(format!("set_series = ?{}", params.len()));
    }
    if let Some(v) = &patch.note {
        params.push(Box::new(v.clone()));
        sets.push(format!("note = ?{}", params.len()));
    }

    if sets.is_empty() {
        return conn
            .query_row("SELECT 1 FROM rule WHERE id = ?1", [id], |_| Ok(()))
            .optional()
            .map(|found| found.is_some())
            .map_err(|e| format!("checking whether rule {id} exists: {e}"));
    }

    params.push(Box::new(id));
    let sql = format!(
        "UPDATE rule SET {} WHERE id = ?{}",
        sets.join(", "),
        params.len()
    );
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(AsRef::as_ref).collect();
    let changed = conn
        .execute(&sql, refs.as_slice())
        .map_err(|e| format!("updating rule {id}: {e}"))?;
    Ok(changed == 1)
}

/// What [`delete`] did: whether `id` named a row at all, and — when that row
/// was one an accepted proposal wrote — every proposal it reopened as a
/// result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deleted {
    pub existed: bool,
    /// Every proposal [`delete`] reopened because `id` was its
    /// `created_rule_id`, ordered by `id` — empty if no proposal ever
    /// pointed at this rule (one created by hand or by import, never by an
    /// accept).
    ///
    /// **A `Vec`, not an `Option<i64>` naming one proposal — one rule
    /// answering several is the ordinary case here, not a corner.**
    /// [`crate::proposals::accept_with`]'s group write can record the *same*
    /// rule against more than one proposal at once — a widened rule
    /// answering several, its own doc comment's "resolves 6 more" — and that
    /// path is reachable through a live HTTP route
    /// (`taiyaku-serve`'s curation endpoints), not just a test. `unaccept`
    /// is keyed on the rule rather than on the one proposal id it is handed,
    /// so it always reopens every proposal that rule answered regardless of
    /// what this field reports — an `Option<i64>` version of this field
    /// reported one of seven reopened proposals and left the other six
    /// unaccounted for to whichever caller reads this struct, which is the
    /// same under-reporting `proposals_resolved` was replaced for two
    /// commits before this one.
    pub reopened_proposals: Vec<i64>,
}

/// Delete rule `id`, reopening whichever proposal(s) it answered — the other
/// end of an accept, C3 §3's `proposal.created_rule_id` closing the loop
/// C4's editor opens.
///
/// **Reuses [`crate::proposals::unaccept`] rather than shipping a second
/// version of the same state change.** `proposals::accept_with` writes a
/// rule and records its id in the accepting proposal's `created_rule_id` —
/// and, for a widened rule, in every other proposal its group write also
/// answers (`also_resolves`); `unaccept` is already the undo of exactly
/// that — one transaction that deletes the rule *and* reopens every
/// proposal `created_rule_id` pointed at it. A rule this table holds because
/// C4's editor created it by hand, rather than because some proposal was
/// accepted, has no proposal to reopen — `unaccept` is never reached for it,
/// and this function deletes the row itself instead, the one write
/// `unaccept` would otherwise also have made.
///
/// **The lookup runs before anything is deleted, and has to.**
/// `proposal.created_rule_id REFERENCES rule(id) ON DELETE SET NULL`
/// (`schema/003_scanner.sql`) clears the column the instant the row is gone,
/// so a lookup keyed on `created_rule_id = id` finds nothing once the delete
/// has already happened — `unaccept`'s own doc comment makes the identical
/// argument about reading `created_rule_id` before its own `DELETE FROM
/// rule`; this is that same ordering applied one level up, before
/// `unaccept` is even called.
///
/// **Calls `unaccept` once, with the lowest-numbered id — it reopens the
/// rest by rule id, not by the one id it is handed.** The lookup already
/// orders by `id`, and `unaccept`'s own `UPDATE ... WHERE created_rule_id =
/// ?2` (keyed on the rule, not the proposal) reopens every proposal that
/// shares this rule regardless of which one is named — so one call is
/// enough, and [`Deleted::reopened_proposals`] reports every id the lookup
/// found rather than only the one passed to `unaccept`.
///
/// # Errors
///
/// Only if SQLite refuses the lookup, the delete, or — through `unaccept` —
/// the reopen. An `id` naming no rule at all is `Ok(Deleted { existed:
/// false, reopened_proposals: vec![] })`, not an error.
pub fn delete(conn: &Connection, id: i64) -> Result<Deleted, String> {
    let mut stmt = conn
        .prepare("SELECT id FROM proposal WHERE created_rule_id = ?1 ORDER BY id")
        .map_err(|e| format!("looking up the proposals rule {id} came from: {e}"))?;
    let reopened_proposals: Vec<i64> = stmt
        .query_map([id], |r| r.get(0))
        .map_err(|e| format!("looking up the proposals rule {id} came from: {e}"))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("reading the proposals rule {id} came from: {e}"))?;

    let existed = if let Some(&first) = reopened_proposals.first() {
        // A refusal means `first` stopped carrying this rule between the
        // lookup above and this call — another connection un-accepted it —
        // and it stays this function's error, as it was while `unaccept`
        // answered a refusal through its `Err`.
        if let crate::proposals::Offer::Refused(why) = crate::proposals::unaccept(conn, first)? {
            return Err(why);
        }
        true
    } else {
        conn.execute("DELETE FROM rule WHERE id = ?1", [id])
            .map_err(|e| format!("deleting rule {id}: {e}"))?
            == 1
    };

    Ok(Deleted {
        existed,
        reopened_proposals,
    })
}

// --- export / import: rules as JSON, so a layout scheme can travel --------
//
// §4: "Rules export and import as JSON, so a layout scheme can be shared or
// moved between hosts. They live in the database... and not in a config
// file, which would be a second source of truth a UI writing rules would
// have to keep in step."

/// A `rule` row's shape in exported JSON.
///
/// **`id` is deliberately absent, and `root_id` is exported as a path for
/// the same reason.** A database's own integer row identifiers name nothing
/// once a rule set moves to a different host — [`import_json`] mints fresh
/// `rule` rows rather than trying to carry `id` across that boundary, and a
/// `root_id` carried the same way is worse than meaningless: `root.id` is
/// enforced by a foreign key on every connection (`Library::configure`),
/// so an import naming a `root_id` this database happens not to have fails
/// with an opaque constraint error, and one it happens to have but that
/// names a *different* tree silently scopes the rule to the wrong root. A
/// root's canonical filesystem path is the one thing about it that is
/// portable — [`export_json`] resolves it once per rule via a join, and
/// [`import_json`] resolves it back to whichever local `root.id` currently
/// holds that path, refusing outright if none does, rather than silently
/// widening the rule's scope to every root or handing SQLite's own
/// constraint error to a person who never typed a `root_id` at all.
///
/// Excluding `id` is also what makes the round trip in this module's
/// `tests.rs` exact: two libraries that hold the same rules by content
/// export to the same bytes regardless of which ids their own migrations
/// happened to assign.
///
/// **`#[serde(deny_unknown_fields)]` is load-bearing, not decoration.**
/// Without it, a document from before this field was renamed — one still
/// carrying `"root_id": 3` from an older build — parses anyway: serde
/// silently fills the now-missing `root_path` with `None` and drops the
/// unrecognised `root_id` on the floor, so the imported rule comes back
/// scoped to *every* root rather than refusing or erroring. That is exactly
/// the silent-widening outcome this whole redesign exists to rule out, by a
/// route that never touches SQL at all — the attribute is what makes an
/// unrecognised field a parse error instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleJson {
    ordinal: i64,
    enabled: bool,
    root_path: Option<String>,
    kind: String,
    pattern: String,
    match_on: String,
    set_edition_key: Option<String>,
    set_kind: Option<String>,
    set_language: Option<String>,
    set_series: Option<String>,
    note: Option<String>,
}

/// Every rule this library holds, as JSON — see the module doc above and §4.
///
/// Ordered by `ordinal` then `id`, the same order [`Engine::load`] reads them
/// in, so which row happens to hold which id never shows up in the document.
///
/// # Errors
///
/// Returns an error if the query against `rule` (joined to `root` for
/// `root_path`) fails.
pub fn export_json(conn: &Connection) -> Result<String, String> {
    let mut stmt = conn
        .prepare(
            "SELECT rule.ordinal, rule.enabled, root.path, rule.kind, rule.pattern, \
             rule.match_on, rule.set_edition_key, rule.set_kind, rule.set_language, \
             rule.set_series, rule.note \
             FROM rule LEFT JOIN root ON root.id = rule.root_id \
             ORDER BY rule.ordinal, rule.id",
        )
        .map_err(|e| format!("reading rules: {e}"))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(RuleJson {
                ordinal: r.get(0)?,
                enabled: r.get::<_, i64>(1)? != 0,
                root_path: r.get(2)?,
                kind: r.get(3)?,
                pattern: r.get(4)?,
                match_on: r.get(5)?,
                set_edition_key: r.get(6)?,
                set_kind: r.get(7)?,
                set_language: r.get(8)?,
                set_series: r.get(9)?,
                note: r.get(10)?,
            })
        })
        .map_err(|e| format!("reading rules: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading rules: {e}"))?;
    serde_json::to_string_pretty(&rows).map_err(|e| format!("serialising rules: {e}"))
}

/// Replace the whole rule table with `rules`, and report the id every
/// element of `rules` ended up written under, in `rules`' own order.
///
/// **The returned `Vec` is positional, not sorted by id or `ordinal` — its
/// element `i` is always the id `rules[i]` was written under**, whether that
/// row was kept at the id it already carried or freshly minted for a `rules[i]`
/// that came in as `id: 0`. [`crate::preview::preview_with_rows`] is why this
/// exists: a rule the caller has not saved yet always carries `id: 0` (`NEW`,
/// below) — indistinguishable from a second unsaved rule in the same call,
/// and from [`ROOT_FALLBACK`] the moment the rule's own root sets a default —
/// so nothing past this function may go on using `Rule::id` to name "the rule
/// the caller meant." This function is the one place every `0` in `rules`
/// becomes a real, minted, mutually distinct `rule.id`; returning them in
/// `rules`' own order is what lets a caller that already has a position in
/// `rules` — an index into the exact list it just sent — recover the id that
/// position now names, without minting a second sentinel of its own.
///
/// **Position in `rules` decides nothing about where a row reads back —
/// `rule.ordinal` does.** [`read`]
/// and [`Engine::load`] both read the table back `ORDER BY ordinal, id`, so
/// where a rule sits in the slice this function is handed has no bearing on
/// where it reads back. Where two rules tie on `ordinal`, `id` breaks it,
/// and that tie-break changed under this function's own fix: a kept row
/// keeps the low id it already had, while a freshly minted row is always
/// minted higher (SQLite mints `INTEGER PRIMARY KEY` rowids upward), so a
/// tie on `ordinal` now always sorts a kept rule ahead of a new one,
/// regardless of list position — where the old delete-and-reinsert design
/// minted every id in list order, so a tie broke in list order instead.
/// Measured: the four seeded rules plus a new rule at `ordinal: 1`, placed
/// *first* in the proposed list, reads back as `[(1,1,"^(?<series>[^/]+)/"),
/// (5,1,"brand-new"), ...]` — the kept rule first despite the new one
/// leading the list, where the old code put `brand-new` first.
///
/// **Atomic on its own**, unlike [`crate::import::replace_alignments`], the
/// precedent this copies its shape from: that function still leans on both
/// its callers already holding a transaction, but this one is `pub(crate)`
/// across more callers than `replace_alignments` has, one of them a bare
/// connection in a test (`preview/tests.rs`), so trusting every present and
/// future caller to wrap it is exactly the "correct only while nobody
/// forgets" shape this crate otherwise avoids (see, for the same reasoning
/// applied to a pragma rather than a function, [`crate::Library`]'s own
/// `configure` comment on `TransactionBehavior::Immediate`). It opens its own
/// [`crate::Nested`] scope — a real transaction where nothing is open yet, a
/// nested `SAVEPOINT` where a caller already holds one, such as
/// [`import_json`]'s own transaction or [`crate::preview::preview`]'s
/// rollback — so the delete, the updates and the inserts below are atomic in
/// both contexts, and still roll back together with an outer transaction
/// where one is open.
///
/// `Rule::id` decides whether a row keeps its identity or is freshly minted
/// — and a kept row is never deleted, only updated in place.
///
/// **`0` means "no id yet" — write a fresh one.** `Rule` doesn't carry an
/// `Option<i64>` for this: every construction site in this crate that builds
/// an unsaved rule already spells it `id: 0` ([`import_json`], below, and
/// this crate's own fixtures), and `0` is safe as the sentinel because
/// SQLite mints `INTEGER PRIMARY KEY` rowids upward from `1` — no row this
/// function ever writes under a minted id can hold `0` for real, so "this
/// rule already exists" and "this rule is new" can never collide on it.
///
/// **A kept row's identity survives because its `DELETE` never happens —
/// not because a `DELETE`-then-reinsert happens to land on the same id.**
/// The obvious-looking alternative — delete every row and reinsert every
/// rule, nonzero ids included — does not work: `proposal.created_rule_id
/// REFERENCES rule(id) ON DELETE SET NULL` (`schema/003_scanner.sql`) fires
/// the moment a row is deleted, and reinserting a different row under the
/// same id afterward does not retroactively relink it — SQLite does not
/// re-evaluate a foreign key against a row that did not exist at the moment
/// the referencing column was cleared. So this function only ever deletes
/// the rows for ids that do not appear in `rules` at all — a rule someone
/// actually removed through C4's editor — and updates every kept row's other
/// columns by `id` instead of touching its identity. That is what lets
/// [`crate::preview::preview`] and a real save tell a genuine edit from a
/// no-op one: previewing (or saving) the exact list already in the database
/// leaves every accepted proposal's `created_rule_id` exactly as it was,
/// and [`crate::proposals::raise`]'s own orphan check reopens nothing.
/// Before this, every call reset every id regardless, which reopened every
/// accepted proposal on *any* call at all, whether or not its rule actually
/// left the list — measured by previewing the library's own unchanged rule
/// list back at itself and getting `proposals_reopened: 1` for a proposal
/// nothing about the preview touched.
///
/// **A nonzero id must already name a row this table holds, or this
/// refuses.** A rule editor only ever hands back a real id for a rule that
/// already exists — a brand new one is `id: 0` — so a nonzero id naming no
/// current row is a caller bug, not a legitimate "adopt this identity"
/// request: silently inserting a fresh row under a caller-chosen id would
/// resurrect whatever that id used to mean (a proposal's own
/// `created_rule_id`, say, from before that row was genuinely deleted) under
/// content that has nothing to do with it. Two rules sharing one nonzero id
/// is refused for the same reason — not SQLite's own primary key constraint,
/// since nothing here inserts a second row under an id already present, but
/// a check this function makes before writing anything, so the whole call
/// fails atomically rather than applying one of the two and silently
/// dropping the other.
///
/// # Errors
///
/// If a rule's nonzero id names no row currently in `rule`, if two rules
/// share one nonzero id, if SQLite refuses to open its scope, refuses a
/// delete, an update or an insert, or refuses committing that scope.
pub(crate) fn replace_all(conn: &Connection, rules: &[Rule]) -> Result<Vec<i64>, String> {
    /// The `Rule::id` sentinel meaning "not written yet" — see this
    /// function's own doc comment for why `0` is safe to use for it.
    const NEW: i64 = 0;

    let scope = crate::Nested::open(conn, "rules_replace_all")
        .map_err(|e| format!("starting the rule replace scope: {e}"))?;

    // The ids this table holds right now — read before anything changes, so
    // a nonzero id in `rules` can be checked against what it could possibly
    // mean to "keep" rather than trusted outright.
    let existing_ids: std::collections::BTreeSet<i64> = {
        let mut stmt = scope
            .prepare("SELECT id FROM rule")
            .map_err(|e| format!("reading the current rule ids: {e}"))?;
        stmt.query_map([], |r| r.get::<_, i64>(0))
            .map_err(|e| format!("reading the current rule ids: {e}"))?
            .collect::<Result<_, _>>()
            .map_err(|e| format!("reading the current rule ids: {e}"))?
    };

    let mut kept_ids = std::collections::BTreeSet::new();
    for rule in rules.iter().filter(|r| r.id != NEW) {
        if !existing_ids.contains(&rule.id) {
            return Err(format!(
                "rule {}: no such rule exists to keep — a genuinely new rule must carry id 0, \
                 not a caller-chosen one",
                rule.id
            ));
        }
        if !kept_ids.insert(rule.id) {
            return Err(format!(
                "rule {}: named more than once in the same replace",
                rule.id
            ));
        }
    }

    // Only the ids genuinely dropped from `rules` are deleted — everything
    // kept is updated in place below, its row never touched by a `DELETE` at
    // all, which is the whole of what makes this function's fix work.
    for id in existing_ids.difference(&kept_ids) {
        scope
            .execute("DELETE FROM rule WHERE id = ?1", [id])
            .map_err(|e| format!("dropping rule {id}: {e}"))?;
    }

    for rule in rules.iter().filter(|r| r.id != NEW) {
        scope
            .execute(
                "UPDATE rule SET ordinal = ?1, enabled = ?2, root_id = ?3, kind = ?4, \
                 pattern = ?5, match_on = ?6, set_edition_key = ?7, set_kind = ?8, \
                 set_language = ?9, set_series = ?10, note = ?11 \
                 WHERE id = ?12",
                rusqlite::params![
                    rule.ordinal,
                    rule.enabled,
                    rule.root_id,
                    rule.kind.to_string(),
                    rule.pattern,
                    rule.match_on.to_string(),
                    rule.set_edition_key,
                    rule.set_kind,
                    rule.set_language,
                    rule.set_series,
                    rule.note,
                    rule.id,
                ],
            )
            .map_err(|e| format!("updating rule {}: {e}", rule.id))?;
    }
    // The id `rules[i]` is written under, for every `i` — a kept row's id is
    // already known (copied straight from `rules` itself, `NEW` included as
    // a placeholder); the loop below overwrites each `NEW` with the rowid
    // SQLite actually mints for it, in the same relative order those rows
    // appear in `rules`, which is exactly the order `Iterator::enumerate`
    // preserves under a `filter`.
    let mut written_ids: Vec<i64> = rules.iter().map(|rule| rule.id).collect();
    for (index, rule) in rules.iter().enumerate().filter(|(_, r)| r.id == NEW) {
        scope
            .execute(
                "INSERT INTO rule (ordinal, enabled, root_id, kind, pattern, match_on, \
                 set_edition_key, set_kind, set_language, set_series, note) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                rusqlite::params![
                    rule.ordinal,
                    rule.enabled,
                    rule.root_id,
                    rule.kind.to_string(),
                    rule.pattern,
                    rule.match_on.to_string(),
                    rule.set_edition_key,
                    rule.set_kind,
                    rule.set_language,
                    rule.set_series,
                    rule.note,
                ],
            )
            .map_err(|e| format!("writing a new rule: {e}"))?;
        written_ids[index] = scope.last_insert_rowid();
    }
    scope
        .commit()
        .map_err(|e| format!("committing the rule replace: {e}"))?;
    Ok(written_ids)
}

/// Replace every rule this library holds with the ones `json` describes.
///
/// **Replaces the rule set rather than appending to it.** An import that
/// layered a second copy of every rule over the first would be harmless the
/// moment nobody has edited a default, and a duplicated, drifting rule set
/// the moment someone has — importing is "this is now the layout scheme", not
/// "add these rules too". [`replace_all`] is what actually clears and
/// rewrites `rule`; this function's own job is turning a `RuleJson` list into
/// the `Rule`s that call needs, `root_path` resolved to this library's own
/// `root_id` along the way.
///
/// **Atomic, and in that order.** `json` is parsed before anything in `rule`
/// is touched, and every root lookup and the write after it happens inside
/// one transaction — so a document that does not parse, that names a root
/// this library does not have, that names a `kind` or `match_on` string
/// `rules.rs` does not know, or a write that fails partway through, leaves
/// the existing rule set exactly as it was rather than emptied or half
/// replaced.
///
/// **Returns every proposal this import reopened — whole-branch review,
/// Important 3.** [`replace_all`]'s own `DELETE FROM rule` orphans
/// `created_rule_id` (`ON DELETE SET NULL`) on every proposal an accepted
/// rule this import just dropped had answered, exactly as [`delete`] does
/// for one rule at a time — but nothing on this path ever called `derive`,
/// so nothing ever reached [`crate::proposals::raise`]'s own repair for that
/// (`reopen_orphaned_accepts`), and an import through `taiyaku-serve`'s
/// `POST /api/library/rules/import` route left every such proposal reading
/// `accepted` with a `created_rule_id` of `None` — an acceptance nothing any
/// longer decides — until whatever scan happened to run next. Calling the
/// same repair here, inside this function's own transaction, closes the gap
/// for every caller at once rather than only the one route that noticed it.
///
/// # Errors
///
/// Returns an error if `json` does not parse as the shape [`export_json`]
/// produces, if a rule's `root_path` matches no `root.path` in this library,
/// if a rule's `kind` or `match_on` is not one of the values [`RuleKind`] or
/// [`MatchOn`] parses — both named by ordinal, since a row that fails either
/// check has no `id` yet — or if [`replace_all`] fails.
pub fn import_json(conn: &Connection, json: &str) -> Result<Vec<i64>, String> {
    let parsed: Vec<RuleJson> =
        serde_json::from_str(json).map_err(|e| format!("parsing rule JSON: {e}"))?;
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("starting the rule import transaction: {e}"))?;

    let mut rules = Vec::with_capacity(parsed.len());
    for rule in &parsed {
        // A path resolved to *this* library's own root id, never a bare
        // number carried across from wherever the document was exported —
        // see `RuleJson`'s own doc comment for why a raw `root_id` would be
        // either an opaque FK failure or a silent scope change.
        let root_id: Option<i64> = match &rule.root_path {
            Some(path) => Some(
                tx.query_row("SELECT id FROM root WHERE path = ?1", [path], |r| r.get(0))
                    .optional()
                    .map_err(|e| format!("looking up root {path}: {e}"))?
                    .ok_or_else(|| {
                        format!(
                            "rule at ordinal {} is scoped to root {path}, which this \
                             library does not have — add that root first, or edit the \
                             rule to apply to every root before importing",
                            rule.ordinal
                        )
                    })?,
            ),
            None => None,
        };
        rules.push(Rule {
            id: 0, // The "mint a fresh id" sentinel `replace_all` reads — see its own doc comment.
            ordinal: rule.ordinal,
            enabled: rule.enabled,
            root_id,
            kind: rule
                .kind
                .parse()
                .map_err(|e| format!("rule at ordinal {}: {e}", rule.ordinal))?,
            pattern: rule.pattern.clone(),
            match_on: rule
                .match_on
                .parse()
                .map_err(|e| format!("rule at ordinal {}: {e}", rule.ordinal))?,
            set_edition_key: rule.set_edition_key.clone(),
            set_kind: rule.set_kind.clone(),
            set_language: rule.set_language.clone(),
            set_series: rule.set_series.clone(),
            note: rule.note.clone(),
        });
    }

    // The written ids `replace_all` reports are for a caller that still holds
    // a position in the list it sent — `crate::preview::preview_with_rows`,
    // not an import that has already handed every rule away.
    replace_all(&tx, &rules)?;
    let reopened = crate::proposals::reopen_orphaned_accepts(&tx)?;

    tx.commit()
        .map_err(|e| format!("committing the rule import: {e}"))?;
    Ok(reopened)
}

#[cfg(test)]
mod tests;
