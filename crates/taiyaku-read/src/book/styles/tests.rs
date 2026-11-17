use super::*;

fn italic(css: &str, tag: &str, classes: &str) -> Option<bool> {
    Styles::parse(css).of(tag, classes, None).italic
}

fn bold(css: &str, tag: &str, classes: &str) -> Option<bool> {
    Styles::parse(css).of(tag, classes, None).bold
}

#[test]
fn a_class_sets_italic() {
    assert_eq!(
        italic(".it { font-style: italic }", "span", "x it"),
        Some(true)
    );
}

#[test]
fn a_tag_and_class_rule_applies_to_that_tag_only() {
    let css = "span.b { font-weight: 700 }";
    assert_eq!(bold(css, "span", "b"), Some(true));
    assert_eq!(bold(css, "p", "b"), None);
}

#[test]
fn weights_from_600_up_are_bold_and_lighter_ones_are_not() {
    for (value, expected) in [
        ("bolder", true),
        ("bold", true),
        ("600", true),
        ("400", false),
        ("normal", false),
        ("500", false),
    ] {
        let css = format!(".w {{ font-weight: {value} }}");
        assert_eq!(bold(&css, "span", "w"), Some(expected), "{value}");
    }
}

#[test]
fn normal_style_is_a_false_not_nothing() {
    assert_eq!(
        italic(".n { font-style: normal }", "span", "n"),
        Some(false)
    );
}

#[test]
fn selectors_it_cannot_place_are_ignored() {
    for css in [
        "p .it { font-style: italic }",
        "[class=it] { font-style: italic }",
        ".it:first-child { font-style: italic }",
        ".a.b { font-style: italic }",
        "p > .it { font-style: italic }",
    ] {
        assert_eq!(
            Styles::parse(css).of("span", "it a b", None),
            Style::default(),
            "{css}"
        );
    }
}

#[test]
fn a_selector_list_applies_to_each_of_its_selectors() {
    let css = ".a, p .z, .b { font-style: italic }";
    assert_eq!(italic(css, "span", "a"), Some(true));
    assert_eq!(italic(css, "span", "b"), Some(true));
    assert_eq!(italic(css, "span", "z"), None);
}

#[test]
fn comments_important_and_case_are_handled() {
    let css = "/* x */ .it { /* y */ FONT-STYLE: ITALIC !important }";
    assert_eq!(italic(css, "span", "it"), Some(true));
}

#[test]
fn media_blocks_are_ignored_and_the_rules_after_them_still_read() {
    let css =
        "@media print { .it { font-style: italic } } @import url(x.css); .b { font-weight: bold }";
    assert_eq!(italic(css, "span", "it"), None);
    assert_eq!(bold(css, "span", "b"), Some(true));
}

#[test]
fn a_later_declaration_wins() {
    let css = ".it { font-style: italic } .it { font-style: normal }";
    assert_eq!(italic(css, "span", "it"), Some(false));
}

#[test]
fn among_matching_classes_the_later_rule_wins_not_the_later_class() {
    let css = ".a { font-style: italic } .b { font-style: normal }";
    assert_eq!(italic(css, "span", "b a"), Some(false));
    assert_eq!(italic(css, "span", "a b"), Some(false));
}

#[test]
fn source_order_holds_across_extended_sheets() {
    let mut styles = Styles::parse(".a { font-style: italic }");
    styles.extend(Styles::parse(".b { font-style: normal }"));
    assert_eq!(styles.of("span", "b a", None).italic, Some(false));
}

#[test]
fn a_tag_rule_outranks_a_class_rule_and_the_style_attribute_both() {
    let styles = Styles::parse("span.it { font-style: normal } .it { font-style: italic }");
    assert_eq!(styles.of("span", "it", None).italic, Some(false));
    assert_eq!(
        styles.of("span", "it", Some("font-style: italic")).italic,
        Some(true)
    );
}

#[test]
fn a_style_attribute_is_read() {
    let styles = Styles::default();
    assert_eq!(
        styles
            .of("span", "", Some("font-style: italic; color: red"))
            .italic,
        Some(true)
    );
}

#[test]
fn extend_lays_a_later_sheet_over_an_earlier_one() {
    let mut styles = Styles::parse(".it { font-style: italic }");
    styles.extend(Styles::parse(
        ".it { font-style: normal } .b { font-weight: bold }",
    ));
    assert_eq!(styles.of("span", "it", None).italic, Some(false));
    assert_eq!(styles.of("span", "b", None).bold, Some(true));
}

#[test]
fn garbage_parses_to_nothing() {
    let rules = |styles: Styles| -> usize {
        styles
            .sheets
            .iter()
            .flat_map(|sheet| sheet.by_class.values())
            .map(Vec::len)
            .sum()
    };
    assert_eq!(rules(Styles::parse("}{;;: {")), 0);
    assert_eq!(rules(Styles::parse("/* unterminated")), 0);
    assert!(rules(Styles::parse(".a { font-style: italic")) <= 1);
}
