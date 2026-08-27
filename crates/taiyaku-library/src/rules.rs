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

    fn set_number(&mut self, field: Field, rule_id: i64, value: f64) {
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
            match rule.kind {
                RuleKind::Prefix => {
                    if target.starts_with(rule.pattern.as_str()) {
                        apply_set_columns(rule, &mut resolved);
                    }
                }
                RuleKind::Regex => {
                    let Some(regex) = &compiled.regex else {
                        // Did not compile — `Engine::compile` already
                        // complained about it. Matches nothing.
                        continue;
                    };
                    if let Some(caps) = regex.captures(target) {
                        apply_set_columns(rule, &mut resolved);
                        apply_captures(rule, &caps, &mut resolved);
                    }
                }
                RuleKind::BuiltinVolume => {
                    if let Some(v) = taiyaku_read::volume_number_marked(target) {
                        resolved.set_number(Field::Volume, rule.id, v);
                    }
                }
                RuleKind::BuiltinChapter => {
                    if let Some(c) = taiyaku_read::chapter_number(target) {
                        resolved.set_number(Field::Chapter, rule.id, c);
                    }
                }
            }
        }
        apply_root_fallback(root, &mut resolved);
        resolved
    }
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
/// enforced by a foreign key on every connection (`Library::from_connection`),
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

/// Replace every rule this library holds with the ones `json` describes.
///
/// **Replaces the rule set rather than appending to it.** An import that
/// layered a second copy of every rule over the first would be harmless the
/// moment nobody has edited a default, and a duplicated, drifting rule set
/// the moment someone has — importing is "this is now the layout scheme", not
/// "add these rules too".
///
/// **Atomic, and in that order.** `json` is parsed before anything in `rule`
/// is touched, and every delete, lookup and insert after that happens inside
/// one transaction — so a document that does not parse, that names a root
/// this library does not have, or a write that fails partway through, leaves
/// the existing rule set exactly as it was rather than emptied or half
/// replaced.
///
/// # Errors
///
/// Returns an error if `json` does not parse as the shape [`export_json`]
/// produces, if a rule's `root_path` matches no `root.path` in this library
/// — named by ordinal, since it has no `id` until it is written — or if
/// clearing or writing `rule` fails.
pub fn import_json(conn: &Connection, json: &str) -> Result<(), String> {
    let rules: Vec<RuleJson> =
        serde_json::from_str(json).map_err(|e| format!("parsing rule JSON: {e}"))?;
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("starting the rule import transaction: {e}"))?;
    tx.execute("DELETE FROM rule", [])
        .map_err(|e| format!("clearing the existing rule set: {e}"))?;
    for rule in &rules {
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
        tx.execute(
            "INSERT INTO rule (ordinal, enabled, root_id, kind, pattern, match_on, \
             set_edition_key, set_kind, set_language, set_series, note) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                rule.ordinal,
                rule.enabled,
                root_id,
                rule.kind,
                rule.pattern,
                rule.match_on,
                rule.set_edition_key,
                rule.set_kind,
                rule.set_language,
                rule.set_series,
                rule.note,
            ],
        )
        .map_err(|e| format!("writing rule: {e}"))?;
    }
    tx.commit()
        .map_err(|e| format!("committing the rule import: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests;
