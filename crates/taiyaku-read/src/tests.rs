use super::*;

/// The ceiling is a number, and nothing asserted it — so every arithmetic
/// mutation of `3 * 1024 * 1024 * 1024` survived, including ones that lower
/// it below the corpus's largest page.
///
/// Written as the two facts the doc comment above claims rather than as a
/// restatement of the expression, because `assert_eq!(X, 3 * 1024 * 1024 *
/// 1024)` is the expression again and would move with any edit to it. What
/// the constant has to be true of: it must admit the largest page in the
/// corpus, and the `image` default it replaces must not.
#[test]
fn the_decode_ceiling_admits_the_largest_page_in_the_corpus() {
    // The Color Walk art books, at 10200×14039 — the largest pages the
    // corpus holds, and the reason this constant exists. `image` bounds the
    // decoded buffer, which is four bytes a pixel.
    const WIDEST: u64 = 10_200;
    const TALLEST: u64 = 14_039;
    const IMAGE_DEFAULT: u64 = 512 * 1024 * 1024;

    assert_eq!(MAX_DECODE_ALLOC, 3_221_225_472, "three GiB");

    let decoded = WIDEST * TALLEST * 4;
    assert!(
        MAX_DECODE_ALLOC > decoded,
        "must admit a {decoded}-byte decode; the ceiling is {MAX_DECODE_ALLOC}"
    );

    assert!(
        IMAGE_DEFAULT < decoded,
        "and the default this replaces must not, or raising it bought nothing"
    );
}
