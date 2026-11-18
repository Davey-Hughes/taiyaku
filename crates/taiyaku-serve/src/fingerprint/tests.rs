use taiyaku_read::test_support::{PRINT_FIXTURE, print_fixture_pages, print_page};

use super::*;

/// The print fixture's pages, as `taiyaku_read` hands a file's pages over.
fn fixture_pages() -> Vec<Page> {
    print_fixture_pages()
        .into_iter()
        .map(|(name, bytes)| Page { name, bytes })
        .collect()
}

/// Print-fixture pages, 96 by 128, page `i` carrying `bars(i)`.
fn print_pages(count: u32, bars: impl Fn(u32) -> [u32; 4]) -> Vec<Page> {
    (0..count)
        .map(|i| Page {
            name: format!("p{i:02}.png"),
            bytes: print_page(96, 128, bars(i), i),
        })
        .collect()
}

/// `got` against `expected`, one assertion a side.
fn assert_border(got: Border, expected: Border) {
    assert_eq!(got.left, expected.left, "left of {got:?}");
    assert_eq!(got.right, expected.right, "right of {got:?}");
    assert_eq!(got.top, expected.top, "top of {got:?}");
    assert_eq!(got.bottom, expected.bottom, "bottom of {got:?}");
}

/// Every page of the print fixture, described with the border its sample
/// gives, is the record `PRINT_FIXTURE` pins for it, field by field. The
/// fixture's doc says what each page separates; cargo-mutants mutating this
/// crate runs only this crate's tests, so this is the copy of the pin that
/// guards the code here.
#[test]
fn the_moved_code_describes_the_print_fixture_as_pinned() {
    let pages = fixture_pages();
    assert_eq!(pages.len(), PRINT_FIXTURE.len(), "one record per page");
    let border = common_border(&pages);
    assert_border(
        border,
        Border {
            left: 6,
            right: 6,
            top: 5,
            bottom: 5,
        },
    );
    for (i, (page, pinned)) in pages.iter().zip(&PRINT_FIXTURE).enumerate() {
        let raw = decode_luma(&page.bytes).unwrap_or_else(|e| panic!("page {i} decodes: {e}"));
        let got = describe_page(&raw, border);
        assert_eq!(got.width, pinned.width, "page {i}'s width");
        assert_eq!(got.height, pinned.height, "page {i}'s height");
        assert_eq!(got.hash.0, pinned.hash, "page {i}'s hash");
        assert_eq!(
            got.halves.map(|(left, right)| (left.0, right.0)),
            pinned.halves,
            "page {i}'s halves"
        );
    }
}

/// 26 pages step by two; 25 do too, where a step of the count over thirteen
/// gives one and the count over twelve rounded up gives three; under 24 pages
/// every page is sampled, and a step of zero would panic.
#[test]
fn border_sample_steps_by_a_twelfth_of_the_count_from_the_first() {
    let evens = [0, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22, 24];
    assert_eq!(border_sample(26).collect::<Vec<_>>(), evens, "26 pages");
    assert_eq!(border_sample(25).collect::<Vec<_>>(), evens, "25 pages");
    assert_eq!(
        border_sample(11).collect::<Vec<_>>(),
        (0..11).collect::<Vec<_>>(),
        "11 pages"
    );
    assert_eq!(border_sample(0).count(), 0, "no pages");
}

/// Twelve pages, each measured, and twelve is even, so the median has two
/// middle values to choose from. Half the pages carry bars of 4 a side and 10
/// above and below, half 8 and 14, so the upper middle, the lower middle and
/// their average give three different borders. The CLI pins the same pages
/// through its `common_border`.
#[test]
fn median_border_takes_the_upper_middle_of_an_even_sample() {
    let pages = print_pages(12, |i| {
        if i % 2 == 0 {
            [4, 4, 10, 10]
        } else {
            [8, 8, 14, 14]
        }
    });
    let seen: Vec<Border> = pages
        .iter()
        .map(|page| flat_border(&decode_luma(&page.bytes).expect("a print page decodes")))
        .collect();
    assert_border(
        median_border(&seen),
        Border {
            left: 8,
            right: 8,
            top: 14,
            bottom: 14,
        },
    );
}

/// A file none of whose sampled pages decoded has no border to take off.
#[test]
fn median_border_of_no_measurement_is_no_border() {
    assert_border(median_border(&[]), Border::default());
}

/// Twenty-five pages, where a step of the page count over twelve rounded
/// down (2), over thirteen (1) and over twelve rounded up (3) sample three
/// different sets. Pages `6k` have no bars, the other even pages 6, pages
/// `6k + 3` 10 and the rest 2, so the three steps give medians of 6, 2 and
/// 0. The CLI pins the same pages.
#[test]
fn common_border_samples_at_a_step_of_the_count_over_twelve_rounded_down() {
    let pages = print_pages(25, |i| match i % 6 {
        0 => [0; 4],
        2 | 4 => [6; 4],
        3 => [10; 4],
        _ => [2; 4],
    });
    assert_border(
        common_border(&pages),
        Border {
            left: 6,
            right: 6,
            top: 6,
            bottom: 6,
        },
    );
}

/// A portrait page. Every word differs from every other here and in
/// [`landscape`], so a field read into its neighbour's place cannot read back
/// equal.
fn portrait() -> PagePrint {
    PagePrint {
        width: 1200,
        height: 1800,
        hash: PageHash([
            0x0123_4567_89AB_CDEF,
            0x1122_3344_5566_7788,
            0x99AA_BBCC_DDEE_FF00,
            0x0F1E_2D3C_4B5A_6978,
        ]),
        halves: None,
    }
}

/// A landscape page with its halves.
fn landscape() -> PagePrint {
    PagePrint {
        width: 2400,
        height: 1700,
        hash: PageHash([
            0xF0E1_D2C3_B4A5_9687,
            0x8877_6655_4433_2211,
            0x00FF_EEDD_CCBB_AA99,
            0x7869_5A4B_3C2D_1E0F,
        ]),
        halves: Some((
            PageHash([
                0x1357_9BDF_0246_8ACE,
                0x2468_ACE0_1357_9BDF,
                0x3579_BDF1_2468_ACE0,
                0x468A_CE02_3579_BDF1,
            ]),
            PageHash([
                0x579B_DF13_468A_CE02,
                0x68AC_E024_579B_DF13,
                0x79BD_F135_68AC_E024,
                0x8ACE_0246_79BD_F135,
            ]),
        )),
    }
}

/// A page not decoded, one hashed and one hashed with halves come back in
/// their places, each record the length its tag says.
#[test]
fn a_blob_reads_back_page_for_page() {
    let pages = [None, Some(portrait()), Some(landscape())];
    let blob = encode(&pages);
    assert_eq!(
        blob.len(),
        1 + 41 + 105,
        "a tag; a tag, two sizes and a hash; and two more hashes"
    );
    assert_eq!(blob[0], 0, "the first record's tag");
    assert_eq!(blob[1], 1, "the second record's tag");
    assert_eq!(blob[42], 2, "the third record's tag");
    assert_eq!(blob[2..6], 1200u32.to_le_bytes(), "a width, little-endian");
    assert_eq!(
        blob[6..10],
        1800u32.to_le_bytes(),
        "a height after its width"
    );
    assert_eq!(
        blob[10..18],
        0x0123_4567_89AB_CDEFu64.to_le_bytes(),
        "a hash's first word, little-endian"
    );
    assert_eq!(
        blob[83..91],
        0x1357_9BDF_0246_8ACEu64.to_le_bytes(),
        "the left half's first word, after the whole page's hash"
    );

    let back = decode(&blob).expect("the blob reads back");
    assert_eq!(back.len(), pages.len(), "one page per record");
    for (i, (got, put)) in back.iter().zip(&pages).enumerate() {
        assert_eq!(got, put, "page {i}");
    }
}

/// A blob that ends inside its last record, and one whose first tag is none
/// of the three, are each refused rather than read as fewer pages.
#[test]
fn a_cut_or_unknown_blob_is_refused() {
    let blob = encode(&[None, Some(portrait()), Some(landscape())]);
    assert!(
        decode(&blob[..blob.len() - 1]).is_err(),
        "a blob less its last byte"
    );
    let mut unknown = blob.clone();
    unknown[0] = 3;
    assert!(decode(&unknown).is_err(), "a first tag of 3");
}

/// A border that would swallow an axis leaves the page whole, each axis tried
/// on its own: a guard that needed both to collapse would hand back a page of
/// no width, or of no height, for the other.
#[test]
fn a_border_that_swallows_one_axis_leaves_the_page_whole() {
    let page = GrayImage::from_pixel(40, 30, image::Luma([128]));
    for swallowing in [
        Border {
            left: 25,
            right: 15,
            top: 0,
            bottom: 0,
        },
        Border {
            left: 0,
            right: 0,
            top: 20,
            bottom: 10,
        },
    ] {
        assert_eq!(
            crop(&page, swallowing).dimensions(),
            (40, 30),
            "{swallowing:?} leaves nothing, so nothing is taken"
        );
    }
}

/// A page too small to measure is left alone, and eight pixels is where
/// that starts. Below it the quarter-per-side cap is one pixel or zero and
/// the scan reduces to noise; the guard exists so a thumbnail or a spacer
/// image cannot report a border and have a quarter of itself removed.
///
/// Both sides of the threshold have to be asserted. Seven alone cannot see
/// the difference between `< 8` and `<= 8`, and eight alone cannot see the
/// guard deleted — and each axis needs its own case, because either one being
/// small is enough to stop the measurement. The CLI pins the same pages.
#[test]
fn a_page_under_eight_pixels_on_either_axis_reports_no_border() {
    let blank = |w, h| GrayImage::from_fn(w, h, |_, _| image::Luma([255u8]));

    // Uniformly white, so every line is flat and the scan would run to its
    // cap on all four sides if it ran at all.
    assert_eq!(flat_border(&blank(7, 20)), Border::default(), "7 wide");
    assert_eq!(flat_border(&blank(20, 7)), Border::default(), "7 tall");

    // At eight it measures, and what it finds is the cap on each side.
    assert_eq!(
        flat_border(&blank(8, 20)),
        Border {
            left: 2,
            right: 2,
            top: 5,
            bottom: 5,
        },
        "8 wide is measured, a quarter per side"
    );
    assert_eq!(
        flat_border(&blank(20, 8)),
        Border {
            left: 5,
            right: 5,
            top: 2,
            bottom: 2,
        },
        "8 tall is measured, a quarter per side"
    );
}
