use super::*;

/// Fractional chapters are how bonus content is numbered, and plain
/// lexicographic comparison gets them backwards: `c005.5` precedes
/// `c005.cbz` because `5` sorts before `c`. Measured cost of the bug on the
/// corpus was a whole volume assembled out of order.
#[test]
fn a_fractional_chapter_sorts_after_its_whole_number() {
    assert_eq!(natural_cmp("c005.cbz", "c005.5.cbz"), Ordering::Less);
    assert_eq!(natural_cmp("c005.5.cbz", "c005.cbz"), Ordering::Greater);
}

/// Where the decimal point goes, pinned on the token stream rather than on
/// an ordering.
///
/// The ordering test above cannot see this. Reading the `.` of `c005.cbz`
/// as part of the number gives `Number(5.0), Text("cbz")` instead of
/// `Number(5.0), Text(".cbz")` — the same first token, so `c005.cbz`
/// against `c005.5.cbz` still compares `5.0` to `5.5` and still answers
/// `Less`. The lookahead that stops it is `chars[i + 1]`; asking
/// `chars[i - 1]` instead always sees the digit the run just ended on, so
/// every `.` would swallow. Only the tokens show it.
#[test]
fn a_decimal_point_joins_the_number_only_when_digits_follow_it() {
    assert_eq!(
        tokenize("c005.cbz"),
        vec![
            Token::Text("c".into()),
            Token::Number(5.0),
            Token::Text(".cbz".into()),
        ],
        "the extension's dot is text, not part of the chapter number"
    );
    assert_eq!(
        tokenize("c005.5.cbz"),
        vec![
            Token::Text("c".into()),
            Token::Number(5.5),
            Token::Text(".cbz".into()),
        ],
        "but a dot with digits after it is"
    );
}

/// Names whose last character is the last character of a loop.
///
/// Every `while` in `tokenize` indexes `chars[i]` behind an `i < len`
/// guard, and every one of those guards is invisible to a test whose names
/// all end in an extension: the digit runs finish in the middle of the
/// string and the bounds are never approached. These three names each end
/// exactly where one of the loops would run off the end — on a digit, on a
/// trailing `.`, and on the digits after a `.` — so a guard that admits one
/// index too many panics instead of passing.
///
/// `c005.` is not a hypothetical: a trailing dot is what a truncated
/// rename leaves behind, and it must tokenize rather than crash.
#[test]
fn a_name_ending_at_a_loop_bound_is_tokenized_rather_than_indexed_past() {
    assert_eq!(
        tokenize("page1"),
        vec![Token::Text("page".into()), Token::Number(1.0)],
        "ends on a digit: the digit run reaches the end of the string"
    );
    assert_eq!(
        tokenize("c005."),
        vec![
            Token::Text("c".into()),
            Token::Number(5.0),
            Token::Text(".".into()),
        ],
        "ends on a dot with nothing after it to look at"
    );
    assert_eq!(
        tokenize("c005.5"),
        vec![Token::Text("c".into()), Token::Number(5.5)],
        "ends on the digits after the dot"
    );

    // And the ordering still holds across all three, which is what the
    // tokens are for.
    assert_eq!(natural_cmp("page1", "page2"), Ordering::Less);
    assert_eq!(natural_cmp("page2", "page10"), Ordering::Less);
    assert_eq!(natural_cmp("c005.", "c005.5"), Ordering::Less);
}

/// `natural_cmp`'s three remaining decisions, none of which had a test.
///
/// **This is code the scanner branch moved past rather than wrote** —
/// `tokenize` and `natural_cmp` are byte-identical to what
/// `crates/taiyaku-read/src/naming.rs` held before it. The gap is
/// pre-existing; it is closed here because the mutation sweep is what found
/// it and leaving three unpinned decisions in the function that orders every
/// page of every archive would be the wrong place to draw a line.
///
/// 1. **Digits before letters.** The comment above the arms says
///    "`9.jpg` precedes `cover.jpg`" and nothing checked it. Swapped, every
///    archive with a cover page opens on the cover instead of page one —
///    visible to a reader on the first page of every such work.
/// 2. **A shorter token list sorts first.** With the final length comparison
///    returning `Equal`, `page1` and `page1a` compare equal, so a sort over
///    names sharing a prefix is unstable — `sort_by` may leave them in any
///    order, and a re-sort of the same directory may answer differently.
/// 3. **Text tokens are lower-cased.** Without it, `Page10.jpg` sorts before
///    `page9.jpg`, because `P` sorts before `p` and the comparison never
///    reaches the numbers — a page sequence split by capitalisation rather
///    than ordered by number.
#[test]
fn natural_order_puts_digits_first_breaks_prefix_ties_and_ignores_case() {
    assert_eq!(
        natural_cmp("9.jpg", "cover.jpg"),
        Ordering::Less,
        "digits before letters, so a numbered page precedes a named one"
    );
    assert_eq!(natural_cmp("cover.jpg", "9.jpg"), Ordering::Greater);

    assert_eq!(
        natural_cmp("page1", "page1a"),
        Ordering::Less,
        "identical as far as either goes, so the shorter token list decides"
    );
    assert_eq!(natural_cmp("page1a", "page1"), Ordering::Greater);

    assert_eq!(
        natural_cmp("Page10.jpg", "page9.jpg"),
        Ordering::Greater,
        "case is folded, so 9 and 10 decide rather than `P` and `p`"
    );
    assert_eq!(
        natural_cmp("Cover.jpg", "cover.jpg"),
        Ordering::Equal,
        "and two names differing only in case are one position, not two"
    );
}

#[test]
fn a_chapter_number_is_taken_from_the_marker_that_precedes_it() {
    assert_eq!(chapter_number("c001"), Some(1.0));
    assert_eq!(chapter_number("Series - c014.cbz"), Some(14.0));
    assert_eq!(chapter_number("ch. 7"), Some(7.0));
    assert_eq!(chapter_number("chapter 128"), Some(128.0));
    assert_eq!(chapter_number("第7話"), Some(7.0));
}

#[test]
fn a_chapter_number_carries_its_decimal_and_never_falls_back() {
    // `member.chapter_declared` is REAL because half-chapters exist.
    assert_eq!(chapter_number("c001.5"), Some(1.5));
    // THE decision of this task. `volume_number` falls back to the first
    // digit run, which is right for a directory that is a volume by
    // construction. A chapter parser that fell back would answer for every
    // volume file in the library — `Series - v01 (2023).cbz` would carry
    // chapter 1 — and §4's rule 3 would then set a chapter on everything.
    assert_eq!(chapter_number("Series - v01 (2023).cbz"), None);
    assert_eq!(chapter_number("069"), None);
    assert_eq!(chapter_number("[2015] ONE PIECE v077"), None);
}

#[test]
fn a_marker_must_start_a_word_rather_than_end_one() {
    // Both measured on the built function, not reasoned about: `c` and `ch`
    // are one and two characters, so without a boundary test they match the
    // tail of ordinary words. This is the no-fallback rule's hole reached by
    // another route — a name with no chapter in it getting a chapter number.
    assert_eq!(chapter_number("Scan by ABC 2019"), None);
    assert_eq!(chapter_number("which 7"), None);
    assert_eq!(chapter_number("Sonic 3"), None);
}

#[test]
fn a_marker_after_a_kanji_still_counts() {
    // The boundary test is ASCII-only for this: every kanji is alphanumeric
    // to Unicode, so `char::is_alphanumeric` would reject the one form of
    // this that is not English.
    assert_eq!(chapter_number("単行本第7話"), Some(7.0));
}

#[test]
fn a_volume_number_is_taken_from_the_marker_that_precedes_it() {
    assert_eq!(volume_number("v069"), Some(69));
    assert_eq!(volume_number("v095-colour-c0"), Some(95));
    assert_eq!(volume_number("第69巻"), Some(69));
    assert_eq!(volume_number("vol. 7"), Some(7));
    assert_eq!(volume_number("raw-v01-src-a"), Some(1));
}

/// With no marker, the first digit run is the best available guess.
#[test]
fn a_bare_number_still_groups() {
    assert_eq!(volume_number("069"), Some(69));
}

/// A trailing year does not distinguish the marker scan from a bare
/// first-digit-run rule: `077` is already the first digit run here, so
/// both agree on `77` regardless of which one runs. Kept as a check that
/// the marker still matches this shape, not as the case that justifies
/// searching for it first — see the next test for that.
#[test]
fn a_year_in_the_name_does_not_win_over_the_volume() {
    assert_eq!(volume_number("ONE PIECE v077 (2015)"), Some(77));
}

/// The case that actually distinguishes the marker scan from a bare
/// first-digit-run rule, and the reason the marker is searched for before
/// falling back: a number appearing *before* the volume marker. The
/// marker scan finds `77`; a bare first-digit-run rule would return `2015`
/// instead. Sabotage-checked: with `MARKERS` and the preceding-text check
/// removed so `volume_number` falls straight through to the first digit
/// run, this assertion fails (`2015` instead of `77`), confirming the
/// marker scan is what this test exercises.
#[test]
fn a_number_before_the_marker_does_not_win_over_the_volume() {
    assert_eq!(volume_number("[2015] ONE PIECE v077"), Some(77));
}

/// `volume_number`'s fallback is the **first** digit run, and the marker
/// scan tolerates **one** space between the marker and the digits. Two
/// separate decisions, and the tests above prove neither, because both of
/// their fixtures answer the same whichever way the code goes:
///
/// - `a_volume_number_is_taken_from_the_marker_that_precedes_it` asserts
///   `volume_number("vol. 7") == Some(7)` — and the fallback returns 7 for
///   that name anyway, so deleting the `strip_suffix(' ')` that makes the
///   marker scan see `vol.` at all leaves the assertion passing while the
///   code it names never runs. `[2015] Series vol. 7` is the same name with
///   the two answers pulled apart: the marker scan says 7, the fallback says
///   2015.
/// - `a_bare_number_still_groups` has a single digit run (`069`), so `first`
///   and `last` are the same run. `[2015] Series 077` is the distinction the
///   fallback is *defined* by, and it is exactly the shape
///   `a_number_before_the_marker_does_not_win_over_the_volume` already
///   guards for the marker path.
///
/// `volume_number` is code this branch moved from `taiyaku-cli`'s
/// `inventory.rs` rather than wrote — byte-identical — so both gaps predate
/// it. `volume_number_marked` shares `marked_number` and not this scan, so
/// each is checked on its own function.
#[test]
fn the_marker_scan_tolerates_a_space_and_the_fallback_takes_the_first_run() {
    assert_eq!(
        volume_number("[2015] Series vol. 7"),
        Some(7),
        "the marker `vol.` is one space back from the digits and still counts"
    );
    assert_eq!(
        volume_number("[2015] Series 077"),
        Some(2015),
        "no marker anywhere, so the fallback is the FIRST digit run, not the last"
    );
}

/// A candidate that reached the page threshold and then vanished from the
/// output is the silence this repository exists to avoid. When a single
/// unparsed candidate is present, it must appear in the output — a name
/// without a volume number indicates a parse failure that must be visible.
#[test]
fn a_name_with_no_digits_has_no_number_rather_than_a_wrong_one() {
    assert_eq!(volume_number("extras"), None);
}

/// `volume_number_marked` is `volume_number`'s marker scan with the fallback
/// removed, so the two agree on every whole-number case above that does not
/// rely on the fallback — but not in general: see
/// `volume_number_marked_reads_a_decimal_part_volume_number_does_not` for
/// the other place they diverge.
#[test]
fn volume_number_marked_agrees_with_volume_number_wherever_a_marker_is_present() {
    assert_eq!(volume_number_marked("v069"), Some(69.0));
    assert_eq!(volume_number_marked("v095-colour-c0"), Some(95.0));
    assert_eq!(volume_number_marked("第69巻"), Some(69.0));
    assert_eq!(volume_number_marked("vol. 7"), Some(7.0));
    assert_eq!(volume_number_marked("raw-v01-src-a"), Some(1.0));
    assert_eq!(volume_number_marked("ONE PIECE v077 (2015)"), Some(77.0));
    assert_eq!(volume_number_marked("[2015] ONE PIECE v077"), Some(77.0));
}

/// The second place `volume_number_marked` and `volume_number` diverge,
/// beside the bare-digit fallback above: `volume_number`'s own scan has no
/// decimal handling at all, so `v01.5` reads as `1` there, while
/// `marked_number`'s scan — the one `chapter_number` already carried a
/// decimal part for — reads it as `1.5`. Intended: half-volumes are as real
/// as half-chapters, and `work.volume` is `REAL` for the same reason
/// `member.chapter_declared` is.
#[test]
fn volume_number_marked_reads_a_decimal_part_volume_number_does_not() {
    assert_eq!(volume_number("v01.5"), Some(1));
    assert_eq!(volume_number_marked("v01.5"), Some(1.5));
}

/// The one place the two diverge, and the entire reason
/// `volume_number_marked` exists: with no marker anywhere in the name,
/// `volume_number` falls back to the first bare digit run — right for
/// `inventory`, which is handed a directory that is a volume by
/// construction — while `volume_number_marked` answers `None`, which is
/// right for the scanner's `builtin-volume` rule (§4), applied to every
/// filename in a library rather than to a directory known in advance to be
/// one.
#[test]
fn volume_number_marked_does_not_fall_back_to_a_bare_digit_run() {
    assert_eq!(volume_number("069"), Some(69));
    assert_eq!(volume_number_marked("069"), None);
    // The case that motivated this task: no `v`/`vol`/`第` anywhere, so a
    // bare-fallback parser reads the chapter digits as a volume.
    assert_eq!(volume_number_marked("Kaguya-sama - c001.cbz"), None);
}

#[test]
fn a_kanji_marker_needs_its_counter_and_the_counter_decides_which_number_it_is() {
    // `第` is an ordinal prefix, not a unit. Before the counter was checked,
    // both parsers answered for both forms and every Japanese-named file
    // took a volume and a chapter with the same value.
    assert_eq!(volume_number_marked("第69巻"), Some(69.0));
    assert_eq!(chapter_number("第69巻"), None);

    assert_eq!(chapter_number("第7話"), Some(7.0));
    assert_eq!(volume_number_marked("第7話"), None);

    assert_eq!(chapter_number("第12章"), Some(12.0));
    assert_eq!(volume_number_marked("第12章"), None);

    // Ambiguous without a counter: neither answers, and a `regex` rule is
    // the documented way to read a layout that writes this.
    assert_eq!(volume_number_marked("第7"), None);
    assert_eq!(chapter_number("第7"), None);
}
