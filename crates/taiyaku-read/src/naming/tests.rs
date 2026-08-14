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
