use std::path::PathBuf;

use super::*;
use crate::Library;
use crate::scan::Root;

/// One rule row, with the `set_` columns filled in by the builder methods.
fn rule(ordinal: i64, kind: &str, pattern: &str, match_on: &str) -> Rule {
    Rule {
        id: ordinal,
        ordinal,
        enabled: true,
        root_id: None,
        kind: kind.parse().expect("a known rule kind"),
        pattern: pattern.to_string(),
        match_on: match_on.parse().expect("a known match_on"),
        set_edition_key: None,
        set_kind: None,
        set_language: None,
        set_series: None,
        note: None,
    }
}

impl Rule {
    #[cfg(test)]
    fn sets_edition_key(mut self, key: &str) -> Self {
        self.set_edition_key = Some(key.to_string());
        self
    }
    #[cfg(test)]
    fn sets_language(mut self, language: &str) -> Self {
        self.set_language = Some(language.to_string());
        self
    }
}

/// An engine over rules that are all expected to compile.
fn engine_of(rules: &[Rule]) -> Engine {
    let (engine, complaints) = Engine::compile(rules);
    assert!(complaints.is_empty(), "{complaints:?}");
    engine
}

/// A root with nothing configured on it, so the rules alone decide.
fn root() -> Root {
    Root {
        id: 1,
        path: PathBuf::from("/library"),
        edition_key: None,
        default_kind: None,
        default_language: None,
        scan_interval_secs: None,
        keeps_documents: false,
    }
}

/// §4's four seeded defaults, as the engine sees them after `load`.
fn defaults() -> Vec<Rule> {
    vec![
        rule(1, "regex", "^(?<series>[^/]+)/", "path"),
        rule(2, "builtin-volume", "", "name"),
        rule(3, "builtin-chapter", "", "name"),
        rule(4, "regex", "^(?<series>.+?)[ _-]+v", "name"),
    ]
}

#[test]
fn a_later_rule_overrides_only_the_fields_it_sets() {
    // §4's headline case: "everything under this root is raw" and "except
    // these are the colour edition" are TWO rules, not four. The second sets
    // `edition_key` and says nothing about language, so the first's language
    // survives.
    let engine = engine_of(&[
        rule(1, "prefix", "", "path")
            .sets_edition_key("raw")
            .sets_language("ja"),
        rule(2, "prefix", "Full Color", "parent").sets_edition_key("colour"),
    ]);

    let plain = engine.resolve(&root(), "Series/Series - v01.cbz");
    assert_eq!(plain.edition_key.as_deref(), Some("raw"));
    assert_eq!(plain.language.as_deref(), Some("ja"));

    let colour = engine.resolve(&root(), "Full Color/Series - v01.cbz");
    assert_eq!(colour.edition_key.as_deref(), Some("colour"));
    assert_eq!(
        colour.language.as_deref(),
        Some("ja"),
        "the later rule set no language, so the earlier one still decides it"
    );
    assert_eq!(colour.provenance[&Field::EditionKey], 2);
    assert_eq!(colour.provenance[&Field::Language], 1);
}

#[test]
fn the_four_defaults_read_a_real_library_path() {
    let engine = engine_of(&defaults());
    let resolved = engine.resolve(
        &root(),
        "Go-Toubun no Hanayome/Go-Toubun no Hanayome - v01 (2019).cbz",
    );

    // Rule 1 takes the directory; rule 4 layers the filename capture over it.
    assert_eq!(resolved.series.as_deref(), Some("Go-Toubun no Hanayome"));
    assert_eq!(resolved.provenance[&Field::Series], 4);
    assert_eq!(resolved.volume, Some(1.0));
    assert_eq!(
        resolved.chapter, None,
        "no chapter marker, and `chapter_number` does not fall back"
    );
}

#[test]
fn a_chapter_file_gets_a_chapter_and_no_volume() {
    let engine = engine_of(&defaults());
    let resolved = engine.resolve(
        &root(),
        "Kaguya-sama - Fan Translated/Kaguya-sama - c001.cbz",
    );
    assert_eq!(resolved.chapter, Some(1.0));
    assert_eq!(resolved.volume, None);
}

/// Nothing in this module resolves a Japanese path through the engine at
/// all otherwise — `巻` counts volumes, `話`/`章` count chapters, and before
/// `marked_number`'s `after` check this file's own `第69巻.cbz` would have
/// taken a chapter number too.
#[test]
fn a_japanese_volume_gets_a_volume_and_not_also_a_chapter() {
    let engine = engine_of(&defaults());
    let resolved = engine.resolve(&root(), "ワンピース/第69巻.cbz");
    assert_eq!(resolved.volume, Some(69.0));
    assert_eq!(resolved.chapter, None, "巻 counts volumes, not chapters");
}

#[test]
fn a_pattern_that_will_not_compile_is_reported_and_disables_only_itself() {
    // A bad pattern reaches this engine from a text box in C4's editor. It
    // must not take a scan of 156 series down with it.
    let (engine, complaints) = Engine::compile(&[
        rule(1, "regex", "^(?<series>[^/]+)/", "path"),
        rule(2, "regex", "(unclosed", "name"),
    ]);
    assert_eq!(complaints.len(), 1);
    assert!(complaints[0].contains("rule 2"), "{}", complaints[0]);
    let resolved = engine.resolve(&root(), "Series/Series - v01.cbz");
    assert_eq!(resolved.series.as_deref(), Some("Series"));
}

/// Both tests above put the surviving rule *before* the bad pattern, which
/// an implementation that stopped compiling — or stopped resolving — at the
/// first failure would also pass. This one puts a rule *after* the bad
/// pattern instead, which is the direction that actually distinguishes
/// "disables only itself" from "disables everything from here on". Found by
/// review as a test-strength gap rather than a behaviour one: the reviewer
/// probed the shipped code against this exact shape and it was already
/// correct.
#[test]
fn a_bad_pattern_does_not_disable_the_rules_that_come_after_it() {
    let (engine, complaints) = Engine::compile(&[
        rule(1, "regex", "^(?<series>[^/]+)/", "path"),
        rule(2, "regex", "(unclosed", "name"),
        rule(3, "builtin-volume", "", "name"),
    ]);
    assert_eq!(complaints.len(), 1, "{complaints:?}");
    let resolved = engine.resolve(&root(), "Series/Series - v01.cbz");
    assert_eq!(resolved.series.as_deref(), Some("Series"));
    assert_eq!(resolved.volume, Some(1.0));
}

// --- everything below is beyond the brief's four tests: `load`, root
// scoping, disabled rules, the root fallback for `kind`/`language`, and the
// two `FromStr` refusals were all implemented but left unexercised by the
// four above, which is a real coverage gap rather than a hypothetical one —
// the workspace's aggregate coverage gate caught it. Each test here would
// fail if the behaviour it names broke. ---

/// `load`'s own SQL — the query, the column order, `ORDER BY ordinal, id` —
/// over the real rows migration 3 seeds, rather than the hand-built
/// `Vec<Rule>` every test above uses. Also pins that `defaults()` (this
/// file's own fixture) is not quietly out of step with what the migration
/// actually inserts: the `provenance[&Field::Series]` assertion below only
/// holds if rule 4 loaded too, and resolving a chapter path only comes back
/// `Some` if rule 3 did — a version of this test that stopped at
/// `series`/`volume` would still pass with only rules 1 and 2 seeded, since
/// `chapter` being `None` there proves nothing: it is `None` whether or not
/// rule 3 exists at all.
#[test]
fn load_reads_the_seeded_defaults_from_a_real_migration() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let (engine, complaints) = Engine::load(&conn).expect("load");
    assert!(complaints.is_empty(), "{complaints:?}");
    let resolved = engine.resolve(
        &root(),
        "Go-Toubun no Hanayome/Go-Toubun no Hanayome - v01 (2019).cbz",
    );
    assert_eq!(resolved.series.as_deref(), Some("Go-Toubun no Hanayome"));
    assert_eq!(
        resolved.provenance[&Field::Series],
        4,
        "rule 4's filename capture, not just rule 1's directory"
    );
    assert_eq!(resolved.volume, Some(1.0));

    let chapter_resolved = engine.resolve(
        &root(),
        "Kaguya-sama - Fan Translated/Kaguya-sama - c001.cbz",
    );
    assert_eq!(chapter_resolved.chapter, Some(1.0));
    assert_eq!(chapter_resolved.provenance[&Field::Chapter], 3);
}

/// The three ways one row can fail to become a [`Rule`] at all — a pattern
/// that will not compile (caught by [`Engine::compile`]), and a `kind` or
/// `match_on` string outside the fixed vocabulary (caught by `load` itself,
/// before a `Rule` exists to hand to `compile`) — each report their own
/// complaint naming their own row, and none of them stops the one good row
/// from working.
#[test]
fn load_reports_a_bad_pattern_and_an_unrecognised_kind_or_match_on_and_skips_only_those_rows() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute("DELETE FROM rule", [])
        .expect("clear the seeded defaults");
    conn.execute(
        "INSERT INTO rule (id, ordinal, kind, pattern, match_on) VALUES
         (1, 1, 'regex', '^(?<series>[^/]+)/', 'path'),
         (2, 2, 'regex', '(unclosed', 'name'),
         (3, 3, 'bogus-kind', '', 'name'),
         (4, 4, 'regex', '^(?<series>.+)$', 'bogus-match-on')",
        [],
    )
    .expect("insert");

    let (engine, complaints) = Engine::load(&conn).expect("load");
    assert_eq!(complaints.len(), 3, "{complaints:?}");
    assert!(
        complaints.iter().any(|c| c.contains("rule 2")),
        "{complaints:?}"
    );
    assert!(
        complaints.iter().any(|c| c.contains("rule 3")),
        "{complaints:?}"
    );
    assert!(
        complaints.iter().any(|c| c.contains("rule 4")),
        "{complaints:?}"
    );

    let resolved = engine.resolve(&root(), "Series/Series - v01.cbz");
    assert_eq!(resolved.series.as_deref(), Some("Series"));
}

/// `root_id` of `None` means every root; `Some(id)` scopes a rule to one.
#[test]
fn a_rule_scoped_to_one_root_does_not_apply_to_another() {
    let mut scoped = rule(1, "prefix", "", "path");
    scoped.root_id = Some(1);
    scoped.set_series = Some("Scoped".to_string());
    let engine = engine_of(&[scoped]);

    let root_one = root();
    let mut root_two = root();
    root_two.id = 2;

    assert_eq!(
        engine
            .resolve(&root_one, "Series/Series - v01.cbz")
            .series
            .as_deref(),
        Some("Scoped")
    );
    assert_eq!(
        engine.resolve(&root_two, "Series/Series - v01.cbz").series,
        None,
        "the rule is scoped to root 1 and this is root 2"
    );
}

/// A path with no parent component (a file directly under the root) matches
/// on the empty string rather than panicking or falling back to the whole
/// path.
#[test]
fn a_root_level_file_matches_on_an_empty_parent_name() {
    let mut catches_everything = rule(1, "prefix", "", "parent");
    catches_everything.set_series = Some("Everything".to_string());
    let mut only_inside_a_directory = rule(2, "prefix", "Series", "parent");
    only_inside_a_directory.set_series = Some("Nested".to_string());
    let engine = engine_of(&[catches_everything, only_inside_a_directory]);

    let resolved = engine.resolve(&root(), "notes.pdf");
    assert_eq!(
        resolved.series.as_deref(),
        Some("Everything"),
        "an empty-pattern prefix rule still matches the empty parent name"
    );
    assert_eq!(
        resolved.provenance[&Field::Series],
        1,
        "the second rule's non-empty pattern does not match an empty parent"
    );
}

/// [`MatchOn::Parent`] is the parent directory's **own name**, not the
/// parent's full path and not the whole `rel_path` — the distinction
/// `MatchOn`'s doc comment states and which nothing tested.
///
/// The test above cannot see it: its fixture is `notes.pdf`, where the whole
/// path and the parent name are one component and the empty string
/// respectively, and an implementation that handed back `rel_path` untouched
/// passes it — the empty pattern matches everything and `"Series"` matches
/// neither. Both directions are asserted here because each catches the other
/// half: a `parent` rule must fire for a file whose parent is deeper than the
/// root, and must **not** fire for a file that merely lives somewhere under a
/// directory of that name. The second is the one with consequences — a
/// `parent`-scoped rule deciding for every file whose whole path begins with
/// the pattern is a rule silently applied to a subtree instead of a
/// directory.
#[test]
fn a_parent_rule_matches_the_parents_own_name_and_not_the_whole_path() {
    let mut colour = rule(1, "prefix", "Full Color", "parent");
    colour.set_series = Some("Colour".to_string());
    let engine = engine_of(&[colour]);

    assert_eq!(
        engine
            .resolve(&root(), "Series/Full Color/Series - v01.cbz")
            .series
            .as_deref(),
        Some("Colour"),
        "the parent directory is named `Full Color`, whatever sits above it"
    );
    assert_eq!(
        engine
            .resolve(&root(), "Full Color/Chapters/Series - v01.cbz")
            .series,
        None,
        "the parent here is `Chapters`; a `parent` rule must not reach a whole subtree"
    );
}

/// A [`RuleKind::Prefix`] pattern is a **prefix**, per its own doc comment,
/// and nothing tested that it is not a substring.
///
/// `a_later_rule_overrides_only_the_fields_it_sets` above is the closest
/// thing, and it cannot see this: its pattern is `"Full Color"` against a
/// parent that is exactly `"Full Color"`, where prefix and substring agree.
/// The cost of the difference is not cosmetic — a one-click "this directory
/// is the colour edition" rule whose pattern is `Color` would capture every
/// series with `Color` anywhere in its path — and it is the predicate
/// `proposals::accept`'s swallow-check rests on. It used to rest on two
/// restatements of it as well — `derive::other_series_under_prefix` asking
/// `rel_path.starts_with(pattern)`, and `accept`'s post-write verification
/// asking `substr(rel_path, 1, length(?1)) = ?1` — and this test was what
/// said all three agreed. Both restatements are gone: `proposals::resolves`
/// asks this engine what a candidate rule would decide, so the predicate
/// below is now the only one there is, and this test is the only thing
/// pinning it.
#[test]
fn a_prefix_rule_matches_a_prefix_and_not_a_substring() {
    let mut mid_path = rule(1, "prefix", "Color", "path");
    mid_path.set_series = Some("Should never appear".to_string());
    let engine = engine_of(&[mid_path]);

    assert_eq!(
        engine
            .resolve(&root(), "Full Color/Series - v01.cbz")
            .series,
        None,
        "`Color` appears mid-path; a prefix rule must not match there"
    );
    assert_eq!(
        engine
            .resolve(&root(), "Color Edition/Series - v01.cbz")
            .series
            .as_deref(),
        Some("Should never appear"),
        "and the same rule does match where the path really does start with it"
    );
}

/// [`Engine::compile`]'s own doc promises `ordinal` order **regardless of
/// what order a caller hands rules in**, and nothing tested it: every test
/// in this file builds its `Vec<Rule>` already ordered, and `load`'s query
/// orders the rows before `compile` ever sees them. So deleting the sort
/// changed no test's output, and neither did dropping `id` from its key.
///
/// Both halves matter for the same reason: layering is what makes "everything
/// under this root is raw" and "except these are colour" two rules rather
/// than four, and layering in the wrong order is an exception that stops
/// overriding the general case.
#[test]
fn compile_sorts_by_ordinal_regardless_of_the_order_it_is_handed() {
    let mut second = rule(1, "prefix", "", "path");
    second.ordinal = 2;
    second.set_series = Some("Second".to_string());
    let mut first = rule(2, "prefix", "", "path");
    first.ordinal = 1;
    first.set_series = Some("First".to_string());

    // Handed in reverse ordinal order, which is what `compile` being `pub`
    // makes possible and what its doc says it copes with.
    let engine = engine_of(&[second, first]);
    let resolved = engine.resolve(&root(), "Series/Series - v01.cbz");
    assert_eq!(
        resolved.series.as_deref(),
        Some("Second"),
        "ordinal 2 applies after ordinal 1 and therefore decides"
    );
    assert_eq!(resolved.provenance[&Field::Series], 1);
}

/// The `id` half of `compile`'s `(ordinal, id)` key, which the sort above
/// cannot see: `sort_by_key` is stable, so with `ordinal` alone two rules
/// sharing an ordinal keep whatever order the caller handed them in —
/// unspecified, and liable to flip a library's structure with no input
/// change.
#[test]
fn two_rules_sharing_an_ordinal_break_the_tie_on_id() {
    let mut higher_id = rule(2, "prefix", "", "path");
    higher_id.ordinal = 1;
    higher_id.set_series = Some("Higher id".to_string());
    let mut lower_id = rule(1, "prefix", "", "path");
    lower_id.ordinal = 1;
    lower_id.set_series = Some("Lower id".to_string());

    let engine = engine_of(&[higher_id, lower_id]);
    let resolved = engine.resolve(&root(), "Series/Series - v01.cbz");
    assert_eq!(
        resolved.series.as_deref(),
        Some("Higher id"),
        "the tie breaks on id, so rule 2 applies last and decides"
    );
    assert_eq!(resolved.provenance[&Field::Series], 2);
}

/// `load`'s `ORDER BY ordinal, id`, which is **not** what puts the engine in
/// order — [`Engine::compile`] re-sorts, which is exactly why reversing this
/// query changed no test's answer and the two masked each other. With
/// `compile`'s own sort now pinned above, the query's one remaining
/// observable effect is the order rows are read in, and complaints are what
/// shows it: `load` pushes one per row it cannot turn into a [`Rule`], as it
/// meets them.
///
/// Both bad rows are bad *kinds* rather than bad patterns on purpose — a
/// pattern complaint comes from `compile`, after every row has been read, and
/// would be ordered by `compile`'s sort rather than by this query.
#[test]
fn load_reads_rows_in_ordinal_order_and_complains_in_that_order() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute("DELETE FROM rule", [])
        .expect("clear the seeded defaults");
    conn.execute(
        "INSERT INTO rule (id, ordinal, kind, pattern, match_on) VALUES
         (1, 1, 'regex', '^(?<series>[^/]+)/', 'path'),
         (2, 2, 'bogus-kind', '', 'name'),
         (3, 3, 'another-bogus-kind', '', 'name')",
        [],
    )
    .expect("insert");

    let (_, complaints) = Engine::load(&conn).expect("load");
    assert_eq!(complaints.len(), 2, "{complaints:?}");
    assert!(complaints[0].starts_with("rule 2:"), "{complaints:?}");
    assert!(complaints[1].starts_with("rule 3:"), "{complaints:?}");
}

/// A disabled rule never matches — but [`Engine::compile`] still attempts
/// its pattern, so a bad one is still named in the complaints rather than
/// going quiet the moment someone unchecks it in the editor.
#[test]
fn a_disabled_rule_never_matches_but_a_bad_pattern_in_it_is_still_reported() {
    let mut off = rule(1, "prefix", "", "path");
    off.enabled = false;
    off.set_series = Some("Should never appear".to_string());
    let engine = engine_of(&[off]);
    assert_eq!(
        engine.resolve(&root(), "Series/Series - v01.cbz").series,
        None
    );

    let mut off_and_broken = rule(1, "regex", "(unclosed", "path");
    off_and_broken.enabled = false;
    let (_, complaints) = Engine::compile(&[off_and_broken]);
    assert_eq!(complaints.len(), 1, "{complaints:?}");
}

/// Fields no rule decides fall back to the root's own `edition_key`,
/// `default_kind` and `default_language` columns, recorded as the sentinel
/// rule id `0` — distinguishable from `series`, which has no root column to
/// fall back to and so is left with no provenance entry at all.
#[test]
fn fields_no_rule_decides_fall_back_to_the_roots_own_defaults() {
    let mut generous_root = root();
    generous_root.edition_key = Some("official".to_string());
    generous_root.default_kind = Some("manga".to_string());
    generous_root.default_language = Some("ja".to_string());

    let engine = engine_of(&[]);
    let resolved = engine.resolve(&generous_root, "Series/Series - v01.cbz");

    assert_eq!(resolved.edition_key.as_deref(), Some("official"));
    assert_eq!(resolved.kind.as_deref(), Some("manga"));
    assert_eq!(resolved.language.as_deref(), Some("ja"));
    assert_eq!(resolved.provenance[&Field::EditionKey], 0);
    assert_eq!(resolved.provenance[&Field::Kind], 0);
    assert_eq!(resolved.provenance[&Field::Language], 0);
    assert_eq!(
        resolved.series, None,
        "no rule set it and the root has no series default column to fall back to"
    );
    assert!(!resolved.provenance.contains_key(&Field::Series));
}

/// The other half of the fallback, and the half nothing tested: it applies to
/// fields **no rule decided**, and must leave the ones a rule did decide
/// alone.
///
/// No test had both a root default and a rule setting the same field, so
/// removing the `is_none()` guard on any of the three arms left the whole
/// suite green — and a root's own `edition_key` overwriting the one a rule
/// explicitly chose is two editions of one work silently merged into one,
/// which is the outcome `apply_set_columns` exists to make possible in the
/// first place. All three arms are covered here, because they are three
/// copies of one shape and a test for one of them proves nothing about the
/// other two.
#[test]
fn a_roots_default_does_not_overwrite_a_field_a_rule_decided() {
    let mut generous_root = root();
    generous_root.edition_key = Some("official".to_string());
    generous_root.default_kind = Some("manga".to_string());
    generous_root.default_language = Some("ja".to_string());

    let mut decides_everything = rule(1, "prefix", "", "path");
    decides_everything.set_edition_key = Some("colour".to_string());
    decides_everything.set_kind = Some("artbook".to_string());
    decides_everything.set_language = Some("en".to_string());

    let engine = engine_of(&[decides_everything]);
    let resolved = engine.resolve(&generous_root, "Series/Series - v01.cbz");

    assert_eq!(resolved.edition_key.as_deref(), Some("colour"));
    assert_eq!(resolved.kind.as_deref(), Some("artbook"));
    assert_eq!(resolved.language.as_deref(), Some("en"));
    assert_eq!(
        resolved.provenance[&Field::EditionKey],
        1,
        "the rule decided it, so the rule is what provenance names — not the root's sentinel 0"
    );
    assert_eq!(resolved.provenance[&Field::Kind], 1);
    assert_eq!(resolved.provenance[&Field::Language], 1);
}

/// **Important 1's reproduction.** `fields_settable_by` used to check its
/// four `set_*` columns unconditionally, before the kind-gated `match`
/// underneath them — so a `BuiltinVolume` row carrying a stray `set_series`
/// (nothing stops one: the schema has no `CHECK` on that column, and
/// `import_json` does not reject one either) was reported as able to decide
/// `Field::Series`, a field [`Engine::resolve`] can never actually set for
/// that kind — its own `match rule.kind` only ever calls `apply_set_columns`
/// for `Prefix` and `Regex`. Fixed by gating those four checks on
/// [`kind_applies_set_columns`], the identical predicate `resolve` now gates
/// its own `apply_set_columns` call on, so the two cannot drift apart again.
#[test]
fn fields_settable_by_ignores_a_stray_set_column_a_builtin_rules_kind_can_never_apply() {
    let mut stray_volume_rule = rule(1, "builtin-volume", "", "name");
    stray_volume_rule.set_series = Some("stray".to_string());

    let fields = fields_settable_by(&stray_volume_rule);

    assert_eq!(
        fields,
        std::collections::BTreeSet::from([Field::Volume]),
        "a BuiltinVolume rule can only ever decide Volume — its stray set_series column must \
         not be reported as settable, or `preview::rows_for`'s near-miss check would demand \
         Series be undecided or root-decided before calling a genuine Volume miss a near-miss \
         at all: {fields:?}"
    );

    // The other builtin kind, same defect, same fix.
    let mut stray_chapter_rule = rule(1, "builtin-chapter", "", "name");
    stray_chapter_rule.set_kind = Some("stray".to_string());

    let fields = fields_settable_by(&stray_chapter_rule);

    assert_eq!(
        fields,
        std::collections::BTreeSet::from([Field::Chapter]),
        "a BuiltinChapter rule can only ever decide Chapter — its stray set_kind column must \
         not be reported as settable: {fields:?}"
    );
}

/// A `regex` rule's `edition` and `title` captures, and the rule that a
/// capture which will not parse as a number sets nothing rather than
/// erroring.
#[test]
fn a_regex_rule_sets_edition_and_title_captures_and_ignores_a_capture_that_will_not_parse() {
    let engine = engine_of(&[rule(
        1,
        "regex",
        r"^(?<edition>[a-z]+)-(?<title>[^-]+)-(?<volume>[a-z]+)\.cbz$",
        "path",
    )]);
    let resolved = engine.resolve(&root(), "raw-My Great Title-abc.cbz");
    assert_eq!(resolved.edition_key.as_deref(), Some("raw"));
    assert_eq!(resolved.title.as_deref(), Some("My Great Title"));
    assert_eq!(resolved.provenance[&Field::EditionKey], 1);
    assert_eq!(resolved.provenance[&Field::Title], 1);
    assert_eq!(
        resolved.volume, None,
        "the volume capture matched `abc`, which does not parse as a number"
    );
    assert!(!resolved.provenance.contains_key(&Field::Volume));
}

/// **`fields_settable_by`'s own `RuleKind::Regex` arm names each of the five
/// recognised captures by hand — `apply_captures`'s own five, one call to
/// `resolved.set`/`set_number` apiece — and nothing above ever calls
/// `fields_settable_by` itself with a pattern naming `series`, `volume` or
/// `chapter`.** The test just above drives `edition`/`title` only, and only
/// through `Engine::resolve`, which runs `apply_captures` directly and never
/// touches this function at all — the two are independent implementations of
/// the same five names, one dynamic and one static, and nothing before this
/// test compared them. Deleting any one arm of `fields_settable_by`'s own
/// `match name { .. }` — `"series"` through `"chapter"` — left the whole
/// suite green: the field it named would then never be reported settable,
/// which is what lets `preview::rows_for`'s near-miss check demand that field
/// be undecided (or root-decided) before it will call a genuine miss a
/// near-miss at all, for a rule that could never have decided it in the
/// first place.
#[test]
fn fields_settable_by_names_every_capture_a_regex_rule_recognises() {
    let names_every_field = rule(
        1,
        "regex",
        r"(?<series>a)(?<edition>b)(?<title>c)(?<volume>d)(?<chapter>e)",
        "name",
    );

    let fields = fields_settable_by(&names_every_field);

    assert_eq!(
        fields,
        std::collections::BTreeSet::from([
            Field::Series,
            Field::EditionKey,
            Field::Title,
            Field::Volume,
            Field::Chapter,
        ]),
        "a regex rule naming all five recognised captures must report all five as settable: \
         {fields:?}"
    );
}

#[test]
fn rule_kind_and_match_on_refuse_a_string_outside_their_fixed_vocabulary() {
    assert!("bogus".parse::<RuleKind>().is_err());
    assert!("nowhere".parse::<MatchOn>().is_err());
}

/// A numeric capture that will not parse sets nothing rather than erroring —
/// pinned above. String captures need the same guard against a different
/// failure: `(?<series>x*)` matches successfully and captures the empty
/// string whenever the pattern matches at all, and letting that set
/// `series = Some("")` would erase an earlier rule's real,
/// directory-derived name rather than leaving it alone.
#[test]
fn an_empty_string_capture_does_not_overwrite_an_earlier_rules_value() {
    let mut names_the_series = rule(1, "prefix", "", "path");
    names_the_series.set_series = Some("Real Series".to_string());
    let matches_but_captures_nothing = rule(2, "regex", r"^(?<series>x*)Series", "name");

    let engine = engine_of(&[names_the_series, matches_but_captures_nothing]);
    let resolved = engine.resolve(&root(), "Series/Series - v01.cbz");
    assert_eq!(resolved.series.as_deref(), Some("Real Series"));
    assert_eq!(
        resolved.provenance[&Field::Series],
        1,
        "rule 2 matched but its capture was empty, so it never took over provenance"
    );
}

// -- export_json / import_json ----------------------------------------------

/// §4: "Rules export and import as JSON, so a layout scheme can be shared or
/// moved between hosts." Two independently-migrated libraries seed the same
/// four defaults with different row ids — SQLite hands out ids in insertion
/// order regardless, but nothing here should rely on that — so this is the
/// property that actually matters: importing one library's export into
/// another and exporting *that* produces the identical document, `id` and
/// all, because `id` is not part of it.
#[test]
fn rules_export_and_import_round_trip() {
    let first = Library::open_in_memory().expect("open");
    let json = export_json(&first.conn()).expect("export");

    let second = Library::open_in_memory().expect("open");
    import_json(&second.conn(), &json).expect("import");

    assert_eq!(export_json(&second.conn()).expect("export"), json);
}

/// A fresh library's export is exactly the four seeded defaults from
/// `schema/003_scanner.sql` — pinned so the round trip above is proven
/// against real content and not against two empty tables agreeing with
/// nothing.
#[test]
fn a_fresh_librarys_export_names_the_four_seeded_defaults() {
    let library = Library::open_in_memory().expect("open");
    let json = export_json(&library.conn()).expect("export");
    for pattern in [
        r#""kind": "regex""#,
        r#""kind": "builtin-volume""#,
        r#""kind": "builtin-chapter""#,
        r#""pattern": "^(?<series>[^/]+)/""#,
    ] {
        assert!(json.contains(pattern), "{pattern} missing from {json}");
    }
    assert!(
        !json.contains("\"id\""),
        "the row id is this database's own and must not appear: {json}"
    );
}

/// [`export_json`]'s own `ORDER BY rule.ordinal, rule.id`, which the round
/// trip above cannot see: it compares one export against another, so a
/// reversal is symmetric and both sides agree on it. The consequence of
/// missing it is not cosmetic — an exported layout scheme reimports with its
/// layering inverted, so every exception a person wrote stops overriding the
/// general rule it was written against.
///
/// Parsed rather than pattern-matched on the text, so this pins the order of
/// the *rules* rather than the order of some substring that happens to
/// appear in them.
#[test]
fn an_export_is_ordered_by_ordinal_so_a_reimport_layers_the_same_way() {
    let library = Library::open_in_memory().expect("open");
    let json = export_json(&library.conn()).expect("export");
    let exported: Vec<RuleJson> = serde_json::from_str(&json).expect("the export parses back");

    let ordinals: Vec<i64> = exported.iter().map(|rule| rule.ordinal).collect();
    assert_eq!(
        ordinals,
        vec![1, 2, 3, 4],
        "the four seeded defaults, ascending — a descending export reimports inverted"
    );
}

/// **Replaces the rule set rather than appending to it.** An import that
/// doubled the four seeded defaults would layer each one over itself, which
/// is harmless today and is exactly the kind of thing that stops being
/// harmless once someone edits a default — see this function's own doc
/// comment.
#[test]
fn importing_the_same_json_twice_does_not_double_the_rule_set() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let json = export_json(&conn).expect("export");
    let before: i64 = conn
        .query_row("SELECT count(*) FROM rule", [], |r| r.get(0))
        .expect("count");

    import_json(&conn, &json).expect("first import");
    import_json(&conn, &json).expect("second import");

    let after: i64 = conn
        .query_row("SELECT count(*) FROM rule", [], |r| r.get(0))
        .expect("count");
    assert_eq!(
        before, after,
        "importing the same rule set twice must replace, not append"
    );
}

/// A document that does not even parse must not touch what is already
/// there — the "atomic, and in that order" half of `import_json`'s own doc
/// comment: `json` is parsed before `rule` is ever cleared.
#[test]
fn a_malformed_import_leaves_the_existing_rule_set_untouched() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let before: i64 = conn
        .query_row("SELECT count(*) FROM rule", [], |r| r.get(0))
        .expect("count");

    let err = import_json(&conn, "not json").unwrap_err();
    assert!(err.contains("parsing"), "{err}");

    let after: i64 = conn
        .query_row("SELECT count(*) FROM rule", [], |r| r.get(0))
        .expect("count");
    assert_eq!(
        before, after,
        "a document that does not parse must leave the existing rules alone"
    );
}

/// **The hazard `#[serde(deny_unknown_fields)]` on `RuleJson` exists to
/// close.** Without it, a document from before the `root_id` → `root_path`
/// rename — this exact shape, produced by an older build — parsed anyway:
/// serde filled the now-missing `root_path` with `None` and silently
/// dropped the unrecognised `root_id`, so a rule scoped to one root came
/// back scoped to every root. This must now refuse, not import unscoped.
#[test]
fn importing_a_stale_document_with_root_id_refuses_rather_than_widening_scope() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let before = export_json(&conn).expect("export");

    let json = r#"[
        {"ordinal": 1, "enabled": true, "root_id": 3, "kind": "prefix",
         "pattern": "", "match_on": "path", "set_edition_key": null,
         "set_kind": null, "set_language": null,
         "set_series": "Scoped Series", "note": null}
    ]"#;

    let err = import_json(&conn, json).unwrap_err();
    assert!(err.contains("parsing"), "{err}");

    assert_eq!(
        export_json(&conn).expect("export"),
        before,
        "a stale root_id document must not touch the existing rule set at all"
    );
}

/// An edited rule set actually takes effect: importing a single narrow
/// `prefix` rule replaces the four defaults with just that one, and the
/// engine `Engine::load` builds from it resolves accordingly — the point of
/// export/import existing at all, proven end to end rather than only at the
/// JSON layer.
#[test]
fn an_imported_rule_set_is_what_engine_load_builds_from() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let json = r#"[
        {"ordinal": 1, "enabled": true, "root_path": null, "kind": "prefix",
         "pattern": "", "match_on": "path", "set_edition_key": null,
         "set_kind": null, "set_language": null,
         "set_series": "Everything Is One Series", "note": null}
    ]"#;

    import_json(&conn, json).expect("import");

    let (engine, complaints) = Engine::load(&conn).expect("load");
    assert!(complaints.is_empty(), "{complaints:?}");
    let resolved = engine.resolve(&root(), "Anything/At/All.cbz");
    assert_eq!(
        resolved.series.as_deref(),
        Some("Everything Is One Series"),
        "the imported rule, not the four defaults it replaced, must be what resolves"
    );
}

/// A rule scoped to a root round-trips onto **whichever local row currently
/// holds that path**, never a bare `root_id` carried across from wherever it
/// was exported — see `RuleJson`'s own doc comment. A fresh `Library::open_in_memory`
/// has no rows in `root` at all, so this adds one with a real, canonicalisable
/// path — `add_root` requires one to exist — the same way a real caller would.
#[test]
fn a_root_scoped_rule_resolves_onto_the_local_root_at_the_same_path() {
    let dir = crate::tests::tempdir("rules-root-scoped");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let added = crate::scan::add_root(&conn, &dir.path).expect("adds a root");

    let json = format!(
        r#"[
            {{"ordinal": 1, "enabled": true, "root_path": {:?}, "kind": "prefix",
              "pattern": "", "match_on": "path", "set_edition_key": null,
              "set_kind": null, "set_language": null,
              "set_series": "Scoped Series", "note": null}}
        ]"#,
        added.path.display()
    );

    import_json(&conn, &json).expect("import");

    let stored_root_id: Option<i64> = conn
        .query_row("SELECT root_id FROM rule", [], |r| r.get(0))
        .expect("reads the written rule");
    assert_eq!(
        stored_root_id,
        Some(added.id),
        "the path resolved to this library's own root id, not a number carried across"
    );

    // And the export of that row names the path again, not the id — the
    // whole point of the round trip.
    let exported = export_json(&conn).expect("export");
    assert!(
        exported.contains(&added.path.display().to_string()),
        "{exported}"
    );
    assert!(
        !exported.contains("root_id"),
        "the field is root_path, never root_id: {exported}"
    );
}

/// The refusal half: a rule scoped to a root this library does not have must
/// not silently widen to every root, and must not hand a person SQLite's own
/// foreign-key error either. `import_json`'s own "atomic, and in that order"
/// doc comment is what this pins — the existing rule set survives untouched.
#[test]
fn importing_a_rule_scoped_to_an_unknown_root_refuses_and_leaves_the_rule_set_untouched() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let before = export_json(&conn).expect("export");

    let json = r#"[
        {"ordinal": 1, "enabled": true, "root_path": "/nowhere/this/library/has",
         "kind": "prefix", "pattern": "", "match_on": "path",
         "set_edition_key": null, "set_kind": null, "set_language": null,
         "set_series": "Scoped Series", "note": null}
    ]"#;

    let err = import_json(&conn, json).unwrap_err();
    assert!(err.contains("/nowhere/this/library/has"), "{err}");
    assert!(err.contains("ordinal 1"), "{err}");

    assert_eq!(
        export_json(&conn).expect("export"),
        before,
        "a rule naming an unknown root must not touch the existing rule set at all"
    );
}

/// **Minor 5.** `replace_all`'s own doc claims atomicity — "the delete and
/// the inserts are atomic ... roll back together with an outer transaction
/// where one is open" — and nothing had tested it before this: every test
/// above reaches `replace_all` only through `import_json`, which refuses a
/// rule scoped to an unknown root *before* ever calling it (a root-path
/// lookup, not a SQL failure), so the property this test pins was
/// unexercised by anything in this file. Reverting `replace_all`'s `Nested`
/// scope to bare `conn.execute` calls with no commit leaves every other test
/// in this suite green, because none of them ever makes an insert partway
/// through the list fail.
///
/// **`root_id: Some(9999)` is the failure.** No such root exists, and every
/// connection this crate hands out enforces foreign keys
/// (`every_connection_enforces_foreign_keys`, `lib.rs`), so the `INSERT` for
/// that one new rule refuses. Where it sits in the proposed list does not
/// matter: `replace_all`'s three-pass structure runs every `DELETE`, then
/// every `UPDATE`, then every `INSERT`, so the one insert this test makes
/// always runs last regardless of list position — placing it "in the
/// middle" was only ever a fact about the `Vec`, never about execution
/// order.
///
/// **The list also genuinely drops the last seeded rule**, not only adds the
/// bad one — `replace_all` no longer deletes every row on every call (that
/// was the defect this same round's other fix removed), so a proposed list
/// that only *adds* a rule never issues a real `DELETE` at all, and a probe
/// confirmed reverting `Nested::open`'s wrapping still left this test green
/// in that shape: every write it makes is either a harmless same-value
/// `UPDATE` or the one `INSERT` that fails, so there is nothing for a broken
/// rollback to leave half-applied. Dropping a rule for real gives the
/// `DELETE` something to do — without the atomic scope, that `DELETE` and
/// every `UPDATE`, both of which run ahead of the `INSERT` pass, would
/// already have landed by the time the bad `INSERT` fails, leaving `rule`
/// short one seeded row and without the new one either — neither the old
/// set nor the new one.
///
/// The bare-connection case: nothing else is open, so `replace_all`'s own
/// `Nested` scope takes the real-transaction branch.
#[test]
fn replace_all_leaves_the_rule_set_untouched_when_an_insert_mid_list_is_refused() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let original = read(&conn).expect("reading the seeded rules").0;
    assert!(
        original.len() >= 2,
        "the seeded defaults, so there is a real middle to insert into"
    );

    // Drops the last seeded rule for real, and adds a doomed new one in the
    // middle of what remains — see the doc comment above for why both
    // matter to what this test actually proves.
    let mut proposed = original[..original.len() - 1].to_vec();
    proposed.insert(
        proposed.len() / 2,
        Rule {
            id: 0,
            root_id: Some(9999),
            ..rule(50, "prefix", "", "path")
        },
    );

    let err = replace_all(&conn, &proposed).expect_err("root 9999 does not exist");
    assert!(err.contains("FOREIGN KEY"), "{err}");

    assert_eq!(
        read(&conn).expect("reading after the refusal").0,
        original,
        "a refused write partway through the list must leave `rule` byte-identical to what it \
         held before — not the seeded rows minus the one this call meant to drop, and not a \
         half-applied mix of the kept rows and the new one that landed before the bad one"
    );
}

/// The other branch of the same property: called with an outer transaction
/// already open — exactly how [`crate::preview::preview`] calls it, nested
/// inside its own rollback — `replace_all`'s scope takes the `SAVEPOINT` arm
/// instead, and a mid-list refusal there must roll back no further than this
/// call's own writes, leaving both the outer transaction usable and the rule
/// set as this call found it.
///
/// Drops the last seeded rule for real, same as the bare-connection test
/// above and for the same reason: with nothing genuinely deleted, every
/// write this call makes ahead of the bad `INSERT` is a same-value `UPDATE`,
/// which leaves nothing for a broken `SAVEPOINT` rollback to prove itself
/// against.
#[test]
fn replace_all_leaves_the_rule_set_untouched_when_an_insert_mid_list_is_refused_inside_a_transaction()
 {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let original = read(&conn).expect("reading the seeded rules").0;

    let mut proposed = original[..original.len() - 1].to_vec();
    proposed.insert(
        proposed.len() / 2,
        Rule {
            id: 0,
            root_id: Some(9999),
            ..rule(50, "prefix", "", "path")
        },
    );

    let outer = conn.unchecked_transaction().expect(
        "opening an outer transaction, so replace_all's own scope takes the SAVEPOINT \
                 branch rather than the real-transaction one",
    );
    let err = replace_all(&outer, &proposed).expect_err("root 9999 does not exist");
    assert!(err.contains("FOREIGN KEY"), "{err}");
    outer
        .query_row("SELECT 1", [], |r| r.get::<_, i64>(0))
        .expect("the outer transaction must still be open and usable after the nested refusal");
    outer
        .commit()
        .expect("committing the outer transaction, which held no write of its own");

    assert_eq!(
        read(&conn).expect("reading after the refusal").0,
        original,
        "a refused write partway through the list must leave `rule` byte-identical to what it \
         held before, even nested inside a transaction that itself went on to commit"
    );
}

/// **A kept row's own `UPDATE` actually has to run.** Every existing test
/// that checks a kept rule's columns after a replace proposes that rule back
/// *unchanged* — `replace_all_preserves_the_ids_it_is_given_and_mints_only_for_the_rest`
/// included, just below — so `kept[&1] == &rule_1` there is satisfied whether
/// or not the `UPDATE … WHERE id = ?12` loop runs at all: the row already
/// held those values before this call. Filtering that loop on `r.id == NEW`
/// instead of `r.id != NEW` — new rules only, kept rules never — still
/// passes every one of them, because a no-op `UPDATE` on a row already
/// holding the proposed values is indistinguishable from a skipped one. This
/// proposes a *changed* field for a kept row and reads it back, which the
/// no-op case cannot satisfy by accident.
#[test]
fn replace_all_writes_a_kept_rules_changed_field_not_only_its_unchanged_id() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let seeded = read(&conn).expect("reading the seeded rules").0;

    let mut changed = seeded.clone();
    changed[0].pattern = "changed-by-this-test".to_string();

    replace_all(&conn, &changed).expect("replace_all over a list with one changed field");

    let after = read(&conn).expect("reading after the replace").0;
    let kept = after
        .iter()
        .find(|r| r.id == seeded[0].id)
        .expect("the kept row still exists under its own id");
    assert_eq!(
        kept.pattern, "changed-by-this-test",
        "a kept row's own UPDATE must actually run — the row continuing to exist under the same \
         id is not proof its columns were rewritten: {kept:?}"
    );
}

/// The id contract itself, over a real save rather than a hand-built
/// `States` map: a rule carrying a real id keeps exactly that id, a rule
/// carrying `0` is minted a fresh one, and a rule dropped from the list —
/// present before, absent from `rules` — loses its row entirely.
///
/// **Also the exact interleaving [`replace_all`]'s own doc comment argues
/// about**, though for a different reason than the old two-pass `INSERT`
/// design it once had: two kept rules (`1` and `4`, the seeded first and
/// last) sit either side of a brand new one in the proposed list, so a
/// version of this function that minted ids by list position rather than by
/// `id == 0` would still pass a version of this test that put every new rule
/// at the end. Seeded rule `2` is dropped entirely, to also pin the third
/// outcome — gone, not merely un-kept.
#[test]
fn replace_all_preserves_the_ids_it_is_given_and_mints_only_for_the_rest() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let seeded = read(&conn).expect("reading the seeded rules").0;
    assert_eq!(
        seeded.iter().map(|r| r.id).collect::<Vec<_>>(),
        vec![1, 2, 3, 4],
        "the four seeded defaults, by id — the fixture this test's own reasoning depends on"
    );

    let rule_1 = seeded[0].clone();
    let rule_4 = seeded[3].clone();
    let new_rule = Rule {
        id: 0,
        ..rule(50, "prefix", "brand-new", "path")
    };
    let proposed = vec![rule_1.clone(), new_rule, rule_4.clone()];

    replace_all(&conn, &proposed).expect("replace_all over a valid list");

    let after = read(&conn).expect("reading after the replace").0;
    assert_eq!(
        after.iter().map(|r| r.id).collect::<Vec<_>>(),
        vec![1, 4, 5],
        "rule 1 and rule 4 must keep exactly the ids they came in with, the new rule must be \
         minted the next id after the highest one this call was told to keep, and rule 2, \
         dropped from the list entirely, must be gone rather than merely unmatched"
    );

    let kept: std::collections::BTreeMap<i64, &Rule> = after.iter().map(|r| (r.id, r)).collect();
    assert_eq!(
        kept[&1], &rule_1,
        "rule 1's own columns must be exactly what was proposed for it, not merely its id"
    );
    assert_eq!(
        kept[&4], &rule_4,
        "rule 4's own columns must be exactly what was proposed for it, not merely its id"
    );
    assert_eq!(
        kept[&5].pattern, "brand-new",
        "the minted rule must hold the new rule's own content"
    );
}

/// A kept row's identity is not just its final id matching — it must never
/// have been deleted at all, or `ON DELETE SET NULL` already fired and
/// re-inserting under the same id cannot undo it. Exercised through
/// `proposal.created_rule_id` directly, one level below
/// `preview::tests::a_preview_of_the_librarys_own_unchanged_rules_reports_no_reopens`,
/// which pins the same property through [`crate::preview::preview`]'s own
/// `Impact` instead.
#[test]
fn replace_all_over_an_unchanged_rule_never_nulls_a_proposals_created_rule_id() {
    use crate::scan;

    let dir = crate::tests::tempdir("replace-all-keeps-created-rule-id");
    scan::tests::write_archive(
        &dir.path
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01.cbz"),
        3,
    );
    scan::tests::write_archive(
        &dir.path
            .join("Go-Toubun no Hanayome - Full Color Edition")
            .join("Go-Toubun no Hanayome - Full Color Edition - v01.cbz"),
        3,
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &dir.path).expect("adding the root");
    scan::scan_root(&conn, &root).expect("scanning the root");
    crate::derive::derive(&conn).expect("the first derive, which raises the edition-of proposal");
    let proposal = crate::proposals::open(&conn, 0.0)
        .expect("reading open proposals")
        .into_iter()
        .find(|p| p.kind == "edition-of")
        .expect("the fixture's two titles raise exactly one edition-of proposal");
    crate::proposals::accept(&conn, proposal.id).expect("accepting the proposal");

    let created_rule_id: Option<i64> = conn
        .query_row(
            "SELECT created_rule_id FROM proposal WHERE id = ?1",
            [proposal.id],
            |r| r.get(0),
        )
        .expect("reading the proposal's created_rule_id");
    assert!(
        created_rule_id.is_some(),
        "accept must have recorded the rule it wrote"
    );

    // Exactly what a save through C4's editor sends when nobody touched
    // anything: the library's own current rule list, read back and handed
    // straight to `replace_all`.
    let current = read(&conn).expect("reading the current rules").0;
    replace_all(&conn, &current).expect("replace_all over an unchanged list");

    let created_rule_id_after: Option<i64> = conn
        .query_row(
            "SELECT created_rule_id FROM proposal WHERE id = ?1",
            [proposal.id],
            |r| r.get(0),
        )
        .expect("reading created_rule_id after the no-op replace");
    assert_eq!(
        created_rule_id_after, created_rule_id,
        "replacing the rule set with the exact list already in the database must not clear the \
         accepted proposal's created_rule_id — its rule's row must never have been deleted at \
         all"
    );
    assert_eq!(
        crate::proposals::states(&conn).expect("reading proposal states")[&proposal.id],
        "accepted",
        "and the proposal itself must still read accepted, not reopened"
    );
}

/// **Whole-branch review, Important 3.** `import_json` replaces the whole
/// rule set through `replace_all`'s own `DELETE FROM rule`, which orphans
/// `created_rule_id` (`ON DELETE SET NULL`) on any proposal an accepted,
/// now-dropped rule had answered — and nothing on this path ever calls
/// `derive`, so nothing ever reached `proposals::raise`'s own repair for
/// that. Before this fix, an import of a document that simply predates an
/// accept left that proposal reading `accepted` with `created_rule_id`
/// `NULL` — an acceptance nothing any longer decided — until whatever scan
/// happened to run next. `import_json` now runs the identical repair
/// `delete_rule` already does, inside its own transaction, and reports the
/// ids it reopened rather than silently dropping them.
#[test]
fn importing_a_rule_set_that_drops_an_accepted_rule_reopens_its_proposal() {
    use crate::scan;

    let dir = crate::tests::tempdir("import-json-reopens-orphaned-accept");
    scan::tests::write_archive(
        &dir.path
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01.cbz"),
        3,
    );
    scan::tests::write_archive(
        &dir.path
            .join("Go-Toubun no Hanayome - Full Color Edition")
            .join("Go-Toubun no Hanayome - Full Color Edition - v01.cbz"),
        3,
    );
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &dir.path).expect("adding the root");
    scan::scan_root(&conn, &root).expect("scanning the root");
    crate::derive::derive(&conn).expect("the first derive, which raises the edition-of proposal");

    // The rule set exactly as it stood before the accept below — the
    // document `import_json` will be handed to reproduce "importing this
    // now drops the rule the accept minted."
    let pre_accept_json = export_json(&conn).expect("exporting the pre-accept rule set");

    let proposal = crate::proposals::open(&conn, 0.0)
        .expect("reading open proposals")
        .into_iter()
        .find(|p| p.kind == "edition-of")
        .expect("the fixture's two titles raise exactly one edition-of proposal");
    crate::proposals::accept(&conn, proposal.id).expect("accepting the proposal");
    assert_eq!(
        crate::proposals::states(&conn).expect("reading proposal states")[&proposal.id],
        "accepted",
        "the accept itself must have landed, or this test proves nothing"
    );

    let reopened = import_json(&conn, &pre_accept_json).expect("importing the pre-accept rule set");

    assert_eq!(
        reopened,
        vec![proposal.id],
        "import_json must report the exact proposal its own import just reopened"
    );
    assert_eq!(
        crate::proposals::states(&conn).expect("reading proposal states")[&proposal.id],
        "open",
        "the proposal must read open again, not stranded as accepted with nothing deciding it"
    );
    let created_rule_id: Option<i64> = conn
        .query_row(
            "SELECT created_rule_id FROM proposal WHERE id = ?1",
            [proposal.id],
            |r| r.get(0),
        )
        .expect("reading the proposal's created_rule_id");
    assert_eq!(
        created_rule_id, None,
        "the rule the accept minted is genuinely gone — reimporting an older document must not \
         resurrect it under the old id"
    );
}

/// **Important 1, first refusal.** A nonzero id naming no row this table
/// currently holds must be refused — a caller bug, per this function's own
/// doc comment, not a legitimate "adopt this identity" request. Reverting
/// the guard loop above to a bare `kept_ids.insert(rule.id);` leaves both
/// this test and the one below unwritten territory: nothing before them
/// ever handed `replace_all` an id it did not itself mint or read back, so
/// the whole crate's suite stayed at 613 passed, 0 failed with the guard
/// gone entirely. Measured with it gone: the four seeded rules plus
/// `Rule { id: 77, .. }` returns `Ok(())`, and the table still holds exactly
/// the four seeded rows — the `UPDATE ... WHERE id = 77` this function
/// issues for it matches no row, its rows-affected count silently
/// discarded, so the caller's rule is dropped on the floor rather than
/// refused. Nothing reachable through `preview` or `import_json` today ever
/// exercises this; C4's save path handing back an id from a stale editor is
/// exactly what it exists to catch.
#[test]
fn replace_all_refuses_a_nonzero_id_that_names_no_existing_row() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let original = read(&conn).expect("reading the seeded rules").0;

    let mut proposed = original.clone();
    proposed.push(Rule {
        id: 77,
        ..rule(50, "prefix", "", "path")
    });

    let err = replace_all(&conn, &proposed).expect_err("id 77 names no existing rule");
    assert!(err.contains('7'), "{err}");
    assert!(err.contains("no such rule exists to keep"), "{err}");

    assert_eq!(
        read(&conn).expect("reading after the refusal").0,
        original,
        "a refused replace must leave `rule` exactly as it found it — not silently applying \
         every rule that was fine and dropping only the one naming an unknown id"
    );
}

/// **Important 1, second refusal.** Two rules sharing one nonzero id must be
/// refused atomically, not resolved by silent last-write-wins — exactly the
/// outcome this function's own doc comment says it refuses. With the guard
/// loop reverted to a bare `kept_ids.insert(rule.id);`, the four seeded
/// rules plus a second copy of rule 1 — same id, `pattern: "second-copy"` —
/// returns `Ok(())`, the table still holds exactly four rows, and rule 1's
/// pattern now reads `"second-copy"`, silently overwritten by whichever of
/// the two copies its `UPDATE` happened to run last.
#[test]
fn replace_all_refuses_two_rules_sharing_one_nonzero_id() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let original = read(&conn).expect("reading the seeded rules").0;
    let rule_1 = original[0].clone();
    assert_eq!(
        rule_1.id, 1,
        "the fixture this test's own reasoning depends on"
    );

    let mut proposed = original.clone();
    proposed.push(Rule {
        pattern: "second-copy".to_string(),
        ..rule_1.clone()
    });

    let err = replace_all(&conn, &proposed).expect_err("rule 1 is named twice");
    assert!(err.contains('1'), "{err}");
    assert!(err.contains("named more than once"), "{err}");

    assert_eq!(
        read(&conn).expect("reading after the refusal").0,
        original,
        "a refused replace must leave `rule` exactly as it found it — rule 1's pattern must not \
         have become \"second-copy\", the silent last-write-wins this guard exists to prevent"
    );
}

// --- `create`, `update`, `delete`: task 7's single-row CRUD ---------------

/// A rule with every column that can hold a non-default value set to one —
/// `root_id`, `set_kind` and `set_language` included.
///
/// **Review found this exact gap.** `library_with_two_competing_rules`'s
/// pair, which every `update` test before this one reused, leaves `root_id`,
/// `set_kind` and `set_language` all `None` — so a version of `update` whose
/// guard on any of those three is unconditional (always writing
/// `patch.<field>.unwrap_or(None)` whether `patch` named it or not) writes
/// `None` over `None`, indistinguishable from correct. A fixture rule has to
/// hold a real value on each of them before a test can tell "left alone"
/// from "silently rewritten" apart.
fn fully_specified_rule(conn: &Connection) -> Rule {
    let dir = crate::tests::tempdir("rules-fully-specified");
    let root = crate::scan::add_root(conn, &dir.path).expect("adding a root for root_id");
    let id = create(
        conn,
        &NewRule {
            ordinal: Some(10),
            enabled: true,
            root_id: Some(root.id),
            kind: RuleKind::Regex,
            pattern: "^(?<series>.+)/".to_string(),
            match_on: MatchOn::Path,
            set_edition_key: Some("raw".to_string()),
            set_kind: Some("manga".to_string()),
            set_language: Some("ja".to_string()),
            set_series: Some("Series".to_string()),
            note: Some("every column set, for the patch-leaves-it-alone tests".to_string()),
        },
    )
    .expect("creating the fully specified fixture rule");
    read(conn)
        .expect("reading the fixture rule back")
        .0
        .into_iter()
        .find(|r| r.id == id)
        .expect("the just-created rule")
}

/// **Important 2.** The reviewer mutation-tested `update` rather than
/// reading it: making `root_id`'s guard (`rules.rs`) unconditional —
/// `patch.root_id.unwrap_or(None)`, always pushed instead of only when
/// `patch.root_id` is `Some` — left all 643 tests in this crate green,
/// because no fixture any `update` test used ever gave a rule a non-`None`
/// `root_id` to lose. Reverting `set_kind`'s guard the same way survives for
/// the identical reason, and `set_language` has the same shape.
/// [`fully_specified_rule`] gives a rule real values on all three, so a
/// patch naming a different field has something to actually preserve.
///
/// Making the `root_id`, `set_kind` or `set_language` guard unconditional —
/// each on its own — must turn this test red: the field goes back to `NULL`
/// even though the patch never named it.
#[test]
fn updating_one_field_leaves_root_id_set_kind_and_set_language_alone() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let before = fully_specified_rule(&conn);

    update(
        &conn,
        before.id,
        &RulePatch {
            pattern: Some("changed/".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    let after = read(&conn)
        .unwrap()
        .0
        .into_iter()
        .find(|r| r.id == before.id)
        .unwrap();
    assert_eq!(after.pattern, "changed/", "the one field the patch named");
    assert_eq!(
        after.root_id, before.root_id,
        "root_id must survive an unrelated patch"
    );
    assert_eq!(
        after.set_kind, before.set_kind,
        "set_kind must survive an unrelated patch"
    );
    assert_eq!(
        after.set_language, before.set_language,
        "set_language must survive an unrelated patch"
    );
    // The remaining columns, to the same standard — review called these
    // "weaker versions of the same gap", covered before only indirectly.
    assert_eq!(after.kind, before.kind);
    assert_eq!(after.match_on, before.match_on);
    assert_eq!(after.note, before.note);
    assert_eq!(after.ordinal, before.ordinal);
    assert_eq!(after.set_edition_key, before.set_edition_key);
    assert_eq!(after.set_series, before.set_series);
    assert_eq!(after.enabled, before.enabled);
}

/// The `Option<Option<T>>` trap, exercised on a second nullable column.
/// `clearing_a_nullable_column_sets_it_to_null_rather_than_leaving_it_alone`,
/// below, only ever clears `set_series` — a `String` column — and a version
/// of `update` that always writes `Some(Some(_))`-shaped values reads
/// identically to a correct one on that one field alone. `root_id` is an
/// `i64` column instead, so this confirms the same `Option<Option<_>>`
/// plumbing clears a non-string nullable column too, and leaves an unrelated
/// one alone while doing it.
#[test]
fn clearing_root_id_sets_it_to_null_and_leaves_set_kind_alone() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let before = fully_specified_rule(&conn);
    assert!(
        before.root_id.is_some(),
        "the fixture must give root_id a real value to clear"
    );

    update(
        &conn,
        before.id,
        &RulePatch {
            root_id: Some(None),
            ..Default::default()
        },
    )
    .unwrap();

    let after = read(&conn)
        .unwrap()
        .0
        .into_iter()
        .find(|r| r.id == before.id)
        .unwrap();
    assert_eq!(after.root_id, None);
    assert_eq!(
        after.set_kind, before.set_kind,
        "a patch clearing root_id must leave a different nullable column, set_kind, alone"
    );
}

/// C4 §1: a rule written by an accepted proposal is marked as such, and
/// deleting it reopens the proposal it came from — the other end of C3 §3's
/// `proposal.created_rule_id`. Without this the two screens each hold half of
/// a reversible action and neither can complete it.
///
/// `list`'s items are [`crate::proposals::ListedProposal`], which wraps
/// `.proposal` rather than exposing `state`/`created_rule_id` at the top
/// level — the plan this test started from assumed a flatter shape, and
/// `crate::proposals::Filter` has no `Default` impl either, so the filter
/// is spelled out by hand below.
#[test]
fn deleting_a_proposal_written_rule_reopens_its_proposal() {
    let (fixture, _base_rules) = crate::tests::library_with_an_accepted_proposal();
    let conn = fixture.conn();
    let accepted = crate::proposals::list(
        &conn,
        &crate::proposals::Filter {
            state: Some("accepted".to_string()),
            kind: None,
            min_score: 0.0,
        },
    )
    .unwrap()
    .into_iter()
    .map(|listed| listed.proposal)
    .next()
    .expect("the fixture accepted one proposal");
    let rule_id = accepted.created_rule_id.expect("acceptance wrote a rule");

    let deleted = delete(&conn, rule_id).unwrap();

    assert!(deleted.existed);
    assert_eq!(deleted.reopened_proposals, vec![accepted.id]);
    let after = crate::proposals::lookup(&conn, accepted.id)
        .unwrap()
        .unwrap();
    assert_eq!(after.state, "open");
}

/// A rule nothing ever accepted has no proposal to reopen — `delete` must
/// still remove it, and must report `reopened_proposals: vec![]` rather
/// than inventing one.
#[test]
fn deleting_a_rule_no_proposal_ever_wrote_removes_it_without_reopening_anything() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let id = read(&conn).unwrap().0[0].id;

    let deleted = delete(&conn, id).unwrap();

    assert!(deleted.existed);
    assert_eq!(deleted.reopened_proposals, Vec::<i64>::new());
    assert!(
        read(&conn).unwrap().0.iter().all(|r| r.id != id),
        "the deleted rule must actually be gone"
    );
}

/// An id naming no rule at all is `existed: false`, not an error — the same
/// "report it rather than pretend" contract [`update`] makes for a patch.
#[test]
fn deleting_an_absent_rule_reports_it_rather_than_erroring() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();

    let deleted = delete(&conn, 9_999).unwrap();

    assert!(!deleted.existed);
    assert_eq!(deleted.reopened_proposals, Vec::<i64>::new());
}

/// **Important 1.** `proposals::accept_with` can write one rule that answers
/// several proposals at once — a widened rule, its own doc comment's
/// "resolves 6 more" — and `unaccept` reopens every one of them, keyed on
/// the rule rather than on any single proposal id. `delete` has to report
/// all of them, not just the lowest-numbered one: a caller told
/// `reopened_proposals: [5]` when the rule actually answered `[5, 9]` would
/// tell a person deleting a widened rule that they reopened one question
/// when they reopened two.
///
/// Reverting `delete` to report only the first id it finds — the
/// `Option<i64>` this field used to be — leaves this test red: the second
/// proposal's id is missing from `deleted.reopened_proposals`, and the
/// second proposal itself is still `accepted` afterward rather than `open`.
#[test]
fn deleting_a_widened_rule_reports_and_reopens_every_proposal_it_answered() {
    let dir = crate::tests::tempdir("rules-widened-accept");
    crate::scan::tests::write_archive(
        &dir.path
            .join("Go-Toubun no Hanayome")
            .join("Go-Toubun no Hanayome - v01.cbz"),
        3,
    );
    crate::scan::tests::write_archive(
        &dir.path
            .join("Go-Toubun no Hanayome - Full Color Edition")
            .join("Go-Toubun no Hanayome - Full Color Edition - v01.cbz"),
        3,
    );
    crate::scan::tests::write_archive(
        &dir.path
            .join("Kaguya-sama wa Kokurasetai")
            .join("Kaguya-sama wa Kokurasetai - v01.cbz"),
        3,
    );
    crate::scan::tests::write_archive(
        &dir.path
            .join("Kaguya-sama wa Kokurasetai - Full Color Edition")
            .join("Kaguya-sama wa Kokurasetai - Full Color Edition - v01.cbz"),
        3,
    );

    let db = Library::open_in_memory().expect("open");
    let conn = db.conn();
    let root = crate::scan::add_root(&conn, &dir.path).expect("adding the root");
    crate::scan::scan_root(&conn, &root).expect("scanning the root");
    crate::derive::derive(&conn).expect("the first derive, which raises both edition-of proposals");

    let open = crate::proposals::open(&conn, 0.0)
        .expect("reading open proposals")
        .into_iter()
        .filter(|p| p.kind == "edition-of")
        .collect::<Vec<_>>();
    assert_eq!(
        open.len(),
        2,
        "the fixture's two titles each raise one edition-of proposal: {open:?}"
    );
    let first = &open[0];
    let mut rule = crate::proposals::preview(&conn, first.id).expect("preview");
    rule.kind = "regex".to_string();
    rule.pattern = "^(?<series>.+) - Full Color Edition/".to_string();
    rule.root_id = None;

    let rule_id =
        crate::proposals::accept_with(&conn, first.id, &rule).expect("the widened accept");
    let accepted_ids: Vec<i64> = conn
        .prepare("SELECT id FROM proposal WHERE created_rule_id = ?1 ORDER BY id")
        .unwrap()
        .query_map([rule_id], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        accepted_ids.len(),
        2,
        "both edition-of proposals point at the one widened rule"
    );

    let deleted = delete(&conn, rule_id).unwrap();

    assert!(deleted.existed);
    assert_eq!(
        deleted.reopened_proposals, accepted_ids,
        "every proposal the widened rule answered, not only the first"
    );
    for id in accepted_ids {
        let after = crate::proposals::lookup(&conn, id).unwrap().unwrap();
        assert_eq!(after.state, "open", "proposal {id} must be reopened too");
    }
}

/// Reordering is a patch of `ordinal`, and it changes what derivation
/// produces — per-field layering asserted, not assumed.
#[test]
fn swapping_two_rules_ordinals_changes_the_derived_structure() {
    let fixture = crate::tests::library_with_two_competing_rules();
    let conn = fixture.conn();
    let rules = read(&conn).unwrap().0;
    let before = crate::preview::grouping(&conn).unwrap();

    update(
        &conn,
        rules[0].id,
        &RulePatch {
            ordinal: Some(rules[1].ordinal),
            ..Default::default()
        },
    )
    .unwrap();
    update(
        &conn,
        rules[1].id,
        &RulePatch {
            ordinal: Some(rules[0].ordinal),
            ..Default::default()
        },
    )
    .unwrap();
    crate::derive::derive(&conn).unwrap();

    assert_ne!(crate::preview::grouping(&conn).unwrap(), before);
}

/// A patch of one field leaves the others alone.
#[test]
fn a_patch_changes_only_the_fields_it_names() {
    let fixture = crate::tests::library_with_two_competing_rules();
    let conn = fixture.conn();
    let before = read(&conn).unwrap().0[0].clone();

    update(
        &conn,
        before.id,
        &RulePatch {
            enabled: Some(false),
            ..Default::default()
        },
    )
    .unwrap();

    let after = read(&conn)
        .unwrap()
        .0
        .into_iter()
        .find(|r| r.id == before.id)
        .unwrap();
    assert!(!after.enabled);
    assert_eq!(after.pattern, before.pattern);
    assert_eq!(after.set_series, before.set_series);
}

/// The trap `RulePatch`'s nullable columns exist to name:
/// `Some(Some(value))` sets a nullable column, leaving the patch field
/// altogether `None` leaves it alone, and `Some(None)` clears it back to
/// NULL — the only way a rule stops setting a field it used to. A test that
/// only ever patches with `Some(Some(_))` cannot tell that apart from a
/// version of `update` that writes every column regardless of what `patch`
/// actually named, since "set it" and "always rewrite it" look identical on
/// a column already holding the value being set.
#[test]
fn clearing_a_nullable_column_sets_it_to_null_rather_than_leaving_it_alone() {
    let fixture = crate::tests::library_with_two_competing_rules();
    let conn = fixture.conn();
    let rules = read(&conn).unwrap().0;
    let with_series = rules
        .iter()
        .find(|r| r.set_series.is_some())
        .expect("the fixture's first rule sets series");

    update(
        &conn,
        with_series.id,
        &RulePatch {
            set_series: Some(None),
            ..Default::default()
        },
    )
    .unwrap();

    let after = read(&conn)
        .unwrap()
        .0
        .into_iter()
        .find(|r| r.id == with_series.id)
        .unwrap();
    assert_eq!(after.set_series, None);
    assert_eq!(
        after.set_edition_key, with_series.set_edition_key,
        "a patch clearing one nullable column must leave a different nullable column, \
         set_edition_key, exactly where it found it"
    );
}

/// Patching a rule that does not exist is `false`, not an error and not a
/// silently created row.
#[test]
fn patching_an_absent_rule_reports_it_rather_than_creating_one() {
    let fixture = crate::tests::library_with_two_competing_rules();
    let conn = fixture.conn();
    let before = read(&conn).unwrap().0.len();

    let changed = update(
        &conn,
        9_999,
        &RulePatch {
            enabled: Some(false),
            ..Default::default()
        },
    )
    .unwrap();

    assert!(!changed);
    assert_eq!(read(&conn).unwrap().0.len(), before);
}

/// A patch naming nothing at all still has to answer whether `id` exists —
/// `update`'s own empty-`SET`-list special case, pinned directly rather than
/// left to whichever branch happens to run.
#[test]
fn an_entirely_empty_patch_reports_whether_the_row_exists_without_changing_anything() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let before = read(&conn).unwrap().0;
    let id = before[0].id;

    let changed = update(&conn, id, &RulePatch::default()).unwrap();

    assert!(changed);
    assert_eq!(read(&conn).unwrap().0, before);
}

/// `create`'s own headline decision, pinned directly: a rule with no
/// `ordinal` named appends past whatever the table already holds rather
/// than landing at the front of the layering order.
#[test]
fn creating_without_an_ordinal_appends_past_the_current_maximum() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let max_before = read(&conn)
        .unwrap()
        .0
        .iter()
        .map(|r| r.ordinal)
        .max()
        .expect("the seeded defaults give this a real maximum to be one past");

    let id = create(
        &conn,
        &NewRule {
            ordinal: None,
            enabled: true,
            root_id: None,
            kind: RuleKind::Prefix,
            pattern: "Anything/".to_string(),
            match_on: MatchOn::Path,
            set_edition_key: None,
            set_kind: None,
            set_language: None,
            set_series: None,
            note: None,
        },
    )
    .unwrap();

    let created = read(&conn)
        .unwrap()
        .0
        .into_iter()
        .find(|r| r.id == id)
        .unwrap();
    assert!(
        created.ordinal > max_before,
        "a new rule with no ordinal named must sort after every rule already in the table, not \
         at ordinal 0 — got ordinal {}, max before was {max_before}",
        created.ordinal
    );
}

/// **Minor 3.** `create`'s own doc comment names the hazard: a bare
/// `MAX(ordinal) + 1` overflows the moment a rule already sits at
/// `i64::MAX` — exactly what `crate::proposals::accept_with`'s own
/// `COALESCE(MAX(ordinal), 4) + 1` does, measured for real once
/// (`crate::tests::LAST_WORD_ORDINAL`'s own doc comment tells that story).
/// `create` guards the identical computation with `i64::saturating_add`
/// rather than a bare `+`, so a rule already at `i64::MAX` gives the next
/// one `i64::MAX` too.
///
/// Reverting `create`'s `m.saturating_add(1)` to a bare `m + 1` turns this
/// test red: a debug build panics on the overflow, and a release build
/// wraps to `i64::MIN`, sorting the new rule first instead of last.
#[test]
fn creating_past_the_maximum_ordinal_saturates_rather_than_overflowing() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    create(
        &conn,
        &NewRule {
            ordinal: Some(i64::MAX),
            enabled: true,
            root_id: None,
            kind: RuleKind::Prefix,
            pattern: "Ceiling/".to_string(),
            match_on: MatchOn::Path,
            set_edition_key: None,
            set_kind: None,
            set_language: None,
            set_series: None,
            note: None,
        },
    )
    .unwrap();

    let id = create(
        &conn,
        &NewRule {
            ordinal: None,
            enabled: true,
            root_id: None,
            kind: RuleKind::Prefix,
            pattern: "PastTheCeiling/".to_string(),
            match_on: MatchOn::Path,
            set_edition_key: None,
            set_kind: None,
            set_language: None,
            set_series: None,
            note: None,
        },
    )
    .unwrap();

    let created = read(&conn)
        .unwrap()
        .0
        .into_iter()
        .find(|r| r.id == id)
        .unwrap();
    assert_eq!(
        created.ordinal,
        i64::MAX,
        "one past i64::MAX saturates at i64::MAX rather than panicking or wrapping"
    );
}

/// The rest of what `create` writes, over a rule that names every field —
/// `creating_without_an_ordinal_appends_past_the_current_maximum` already
/// covers the default; this is the content round trip.
#[test]
fn created_rule_reads_back_with_the_content_it_was_given() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();

    let id = create(
        &conn,
        &NewRule {
            ordinal: Some(42),
            enabled: false,
            root_id: None,
            kind: RuleKind::Regex,
            pattern: "^(?<series>[^/]+)/".to_string(),
            match_on: MatchOn::Path,
            set_edition_key: Some("raw".to_string()),
            set_kind: None,
            set_language: None,
            set_series: None,
            note: Some("a hand-typed rule".to_string()),
        },
    )
    .unwrap();

    let created = read(&conn)
        .unwrap()
        .0
        .into_iter()
        .find(|r| r.id == id)
        .unwrap();
    assert_eq!(created.ordinal, 42);
    assert!(!created.enabled);
    assert_eq!(created.kind, RuleKind::Regex);
    assert_eq!(created.pattern, "^(?<series>[^/]+)/");
    assert_eq!(created.set_edition_key, Some("raw".to_string()));
    assert_eq!(created.note, Some("a hand-typed rule".to_string()));
}
