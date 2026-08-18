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
/// `proposals::accept`'s swallow-check rests on: `derive::other_series_under_prefix`
/// asks `rel_path.starts_with(pattern)` and `accept`'s post-write
/// verification asks `substr(rel_path, 1, length(?1)) = ?1`, so all three
/// agree today and only this test says so.
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
