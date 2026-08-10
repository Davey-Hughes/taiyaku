//! Reducing a decoded page to a structural hash.

use std::io::Cursor;

use image::GrayImage;
use rayon::prelude::*;
use taiyaku_core::PageHash;

use taiyaku_read::{MAX_DECODE_ALLOC, Page};

/// Hash grid edge. 16 gives the 256 bits `PageHash` carries.
const GRID: u32 = 16;

/// Above this encoded size a page is decoded on its own rather than alongside
/// eleven others.
///
/// Archival scans exist: the Color Walk artbooks are 430 MB per page at
/// 10200×14039, which decodes to about the same again. Twelve of those at once
/// is ten gigabytes, so page size has to bound concurrency. Ordinary manga pages
/// run from 100 KB to 3 MB and stay fully parallel.
pub const LARGE_PAGE_BYTES: usize = 32 * 1024 * 1024;

/// Decode an encoded page to greyscale.
pub fn decode_luma(bytes: &[u8]) -> Result<GrayImage, String> {
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    reader.limits(limits);
    Ok(reader.decode().map_err(|e| e.to_string())?.to_luma8())
}

/// A border to remove from every page of a member, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Border {
    pub left: u32,
    pub right: u32,
    pub top: u32,
    pub bottom: u32,
}

impl Border {
    /// The same border with each opposing pair evened up to the larger of the two.
    ///
    /// A letterbox is centred by construction — the padding exists to fit an image
    /// of one aspect into a frame of another — so an honest measurement of it is
    /// symmetric. An asymmetric result means one side was mismeasured, and the
    /// cheapest thing that goes wrong is a single-column scan seam near an edge
    /// truncating that side's scan. Trusting it moves the centre of the page, which
    /// for a joined spread moves the line the two pages are split along, and that
    /// costs more than the bars did.
    ///
    /// The larger rather than the smaller: the smaller is the mismeasurement. The
    /// exposure is a page with a genuine one-sided margin, where this over-crops
    /// the other side. Capped at a quarter per side by `flat_border`, and measured
    /// across five series without regression.
    fn symmetric(self) -> Self {
        let horizontal = self.left.max(self.right);
        let vertical = self.top.max(self.bottom);
        Border {
            left: horizontal,
            right: horizontal,
            top: vertical,
            bottom: vertical,
        }
    }
}

/// The flat border to take off every page of a member.
///
/// Some scans are letterboxed: Tongari volume 9 is spreads padded into 1920×1080,
/// twenty per cent of its width being flat white bars. Splitting such a page down
/// the middle leaves each half a fifth blank, which shifts the content within the
/// hash grid and wrecks the comparison against a clean page. Removing the bars is
/// worth 98 runs against 7 on that volume, and 139 paired slots against 176.
///
/// This is not the autocrop that was tried and rejected earlier. That one cropped
/// to the inked bounding box, which moves with whatever each page happens to
/// contain, and added more variance than it removed.
///
/// Per-member rather than per-page, and that distinction is the whole point. A
/// letterbox is identical on every page of a volume, so removing it is stable.
/// Trimming each page independently is not: it was measured, and it degraded
/// Kaguya from 8 runs to 15 while barely helping the padded volume it was for,
/// because pages whose art reaches the edge trim less than their neighbours and
/// the content then sits differently in each hash. That is the same failure as
/// the ink-bounding-box autocrop, arrived at from a different direction.
///
/// Sampled rather than exhaustive because a bar present on pages spread through
/// the volume is present throughout, and decoding twice costs real time.
///
/// The **median** across samples, not the minimum. A minimum was tried and is
/// wrong: a single full-bleed page — a cover, typically the first thing sampled —
/// has no border at all and drops the minimum to zero, cancelling the trim for
/// the entire member. A median survives a handful of such pages.
pub fn common_border(pages: &[Page]) -> Border {
    let step = (pages.len() / 12).max(1);
    let mut seen: Vec<Border> = Vec::new();
    for page in pages.iter().step_by(step) {
        if let Ok(luma) = decode_luma(&page.bytes) {
            seen.push(flat_border(&luma));
        }
    }
    if seen.is_empty() {
        return Border::default();
    }
    let median_of = |mut v: Vec<u32>| {
        v.sort_unstable();
        v[v.len() / 2]
    };
    Border {
        left: median_of(seen.iter().map(|b| b.left).collect()),
        right: median_of(seen.iter().map(|b| b.right).collect()),
        top: median_of(seen.iter().map(|b| b.top).collect()),
        bottom: median_of(seen.iter().map(|b| b.bottom).collect()),
    }
    .symmetric()
}

fn crop(page: &GrayImage, border: Border) -> GrayImage {
    let (w, h) = page.dimensions();
    let width = w.saturating_sub(border.left + border.right);
    let height = h.saturating_sub(border.top + border.bottom);
    match width == 0 || height == 0 || (border == Border::default()) {
        true => page.clone(),
        false => image::imageops::crop_imm(page, border.left, border.top, width, height).to_image(),
    }
}

/// How many flat lines sit at each edge of one page.
fn flat_border(page: &GrayImage) -> Border {
    /// How much a line may vary and still count as flat.
    const FLAT: u8 = 2;
    /// Never take more than this fraction from one side.
    const MAX_SIDE: u32 = 4;

    let (w, h) = page.dimensions();
    if w < 8 || h < 8 {
        return Border::default();
    }

    let flat_column = |x: u32| {
        let first = page.get_pixel(x, 0).0[0];
        (0..h).all(|y| page.get_pixel(x, y).0[0].abs_diff(first) <= FLAT)
    };
    let flat_row = |y: u32| {
        let first = page.get_pixel(0, y).0[0];
        (0..w).all(|x| page.get_pixel(x, y).0[0].abs_diff(first) <= FLAT)
    };

    let mut left = 0;
    while left < w / MAX_SIDE && flat_column(left) {
        left += 1;
    }
    let mut right = w - 1;
    while right > w - 1 - w / MAX_SIDE && flat_column(right) {
        right -= 1;
    }
    let mut top = 0;
    while top < h / MAX_SIDE && flat_row(top) {
        top += 1;
    }
    let mut bottom = h - 1;
    while bottom > h - 1 - h / MAX_SIDE && flat_row(bottom) {
        bottom -= 1;
    }

    match right <= left || bottom <= top {
        true => Border::default(),
        false => Border {
            left,
            right: w - 1 - right,
            top,
            bottom: h - 1 - bottom,
        },
    }
}

/// Split pages into those safe to decode in parallel and those that are not.
///
/// Returns indices, because the caller has to put the results back in page
/// order and losing that ordering would misalign the whole edition.
pub fn partition_by_size(sizes: &[usize]) -> (Vec<usize>, Vec<usize>) {
    let mut parallel = Vec::new();
    let mut alone = Vec::new();
    for (index, size) in sizes.iter().enumerate() {
        match *size >= LARGE_PAGE_BYTES {
            true => alone.push(index),
            false => parallel.push(index),
        }
    }
    (parallel, alone)
}

/// Decode pages, keeping memory bounded by decoding the large ones one at a time.
///
/// Results come back in page order regardless of which path each took, because
/// every later index — slots, runs, the whole alignment — is derived from it.
///
/// Widened from private for `detect`, which needs the same bounded parallel
/// decode and must not carry a second copy of the large-page rule: the ceiling
/// is `MAX_DECODE_ALLOC` and the corpus holds 430 MB TIFFs, so decoding the
/// large ones one at a time is what keeps a detection run inside memory.
pub(crate) fn decode_all<T, F>(pages: &[Page], describe: F) -> Result<Vec<T>, String>
where
    T: Send,
    F: Fn(&Page, &GrayImage) -> T + Send + Sync,
{
    let sizes: Vec<usize> = pages.iter().map(|p| p.bytes.len()).collect();
    let (parallel, alone) = partition_by_size(&sizes);

    let mut done: Vec<Option<T>> = (0..pages.len()).map(|_| None).collect();

    let small: Result<Vec<(usize, T)>, String> = parallel
        .par_iter()
        .map(|&i| {
            let page = &pages[i];
            let luma = decode_luma(&page.bytes).map_err(|e| format!("{}: {e}", page.name))?;
            Ok((i, describe(page, &luma)))
        })
        .collect();
    for (i, value) in small? {
        done[i] = Some(value);
    }

    for &i in &alone {
        let page = &pages[i];
        let luma = decode_luma(&page.bytes).map_err(|e| format!("{}: {e}", page.name))?;
        done[i] = Some(describe(page, &luma));
    }

    done.into_iter()
        .enumerate()
        .map(|(i, value)| value.ok_or_else(|| format!("page {i} was never decoded")))
        .collect()
}

/// Fingerprint every page of an edition.
///
/// Decoding dominates by three orders of magnitude — around 12 ms per page at
/// 2147x3056 against roughly 100 us for the entire similarity matrix — and it
/// is embarrassingly parallel, so it is worth spreading across cores. This is
/// also why fingerprints are cached per archive rather than recomputed per pair.
pub fn fingerprint_edition(pages: &[Page]) -> Result<Vec<PageHash>, String> {
    decode_all(pages, |_, luma| fingerprint(luma))
}

/// Decode every page once and record everything the cache needs.
///
/// Half hashes are computed for any landscape page whether or not it turns out
/// to be a spread, because the spread rule is an edition-level judgement that
/// may change while the pixels do not.
pub fn describe_pages(pages: &[Page]) -> Result<Vec<crate::cache::CachedPage>, String> {
    let border = common_border(pages);
    decode_all(pages, move |_, raw| {
        let luma = &crop(raw, border);
        let (width, height) = luma.dimensions();
        let halves = (width > height).then(|| {
            let mid = width / 2;
            (
                fingerprint(&image::imageops::crop_imm(luma, 0, 0, mid, height).to_image()),
                fingerprint(&image::imageops::crop_imm(luma, mid, 0, mid, height).to_image()),
            )
        });
        crate::cache::CachedPage {
            width,
            height,
            hash: fingerprint(luma),
            halves,
        }
    })
}

/// Reduce a page to its difference hash.
///
/// Resize to a `(GRID+1) x GRID` grid and record, for each cell, whether it is
/// brighter than the cell to its right. Only the comparisons are kept, which is
/// what makes the result independent of exposure and of the page's resolution —
/// the corpus spans 685x1024 to 2147x3056 across editions of one volume.
///
/// Chosen over a DCT-based perceptual hash by measurement, not preference: at
/// equal width the two scored within noise of each other on the corpus, and this
/// one needs no transform.
pub fn fingerprint(page: &GrayImage) -> PageHash {
    let small =
        image::imageops::resize(page, GRID + 1, GRID, image::imageops::FilterType::Lanczos3);

    let mut words = [0u64; 4];
    let mut bit = 0usize;
    for y in 0..GRID {
        for x in 0..GRID {
            if small.get_pixel(x, y).0[0] > small.get_pixel(x + 1, y).0[0] {
                words[bit / 64] |= 1 << (bit % 64);
            }
            bit += 1;
        }
    }
    PageHash(words)
}

/// Bits differing between two dhashes, 0 to 256.
///
/// `taiyaku_core::similarity` measures the same underlying quantity but
/// normalises it to a `0.0..=1.0` fraction of agreement, which is what
/// `solve`'s calibration wants. Callers that need the raw count — `credits`
/// reports a Hamming distance a person reads directly, such as "distance 2"
/// or "distance 99" — get it from here rather than reversing that fraction
/// back out of a float.
pub(crate) fn hamming(a: PageHash, b: PageHash) -> u32 {
    a.0.iter()
        .zip(b.0.iter())
        .map(|(x, y)| (x ^ y).count_ones())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ink that varies both across the page and down it, so that no single line
    /// of it reads as flat. A fixture whose content is uniform down a column
    /// cannot tell a bar from artwork, which is the way these tests fail
    /// silently rather than loudly. Never reaches 255, so white stays white.
    fn ink(x: u32, y: u32) -> u8 {
        40 + ((x as u16 * 7 + y as u16 * 31) % 180) as u8
    }

    /// A `Page` carrying `image` as encoded PNG, which is what the entry points
    /// take: `common_border` and `describe_pages` both decode before measuring.
    fn encoded(name: &str, image: &GrayImage) -> Page {
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::ImageLuma8(image.clone())
            .write_to(&mut out, image::ImageFormat::Png)
            .expect("a greyscale image encodes as PNG");
        Page {
            name: name.into(),
            bytes: out.into_inner(),
        }
    }

    /// A flat bar comes off; a margin with anything in it stays. The second half
    /// is what separates this from the autocrop that was rejected: that one
    /// cropped to the ink and so moved with the content, and this must not.
    #[test]
    fn a_uniform_bar_is_trimmed_and_a_varying_margin_is_not() {
        // 40 wide: 8 columns of flat white, 24 of content, 8 flat again.
        let letterboxed = GrayImage::from_fn(40, 20, |x, y| {
            let inside = (8..32).contains(&x);
            // Content must vary down the page as well as across it, or every
            // column is vertically flat and the trim cannot tell them apart.
            image::Luma([if inside { ink(x, y) } else { 255 }])
        });
        let trimmed = crop(&letterboxed, flat_border(&letterboxed));
        assert_eq!(trimmed.dimensions(), (24, 20), "flat bars should come off");

        // Same shape, but the left margin carries a mark. Nothing may be cropped,
        // because the margin is part of the artwork.
        let inked_margin = GrayImage::from_fn(40, 20, |x, y| {
            let inside = (8..32).contains(&x);
            let mark = x < 8 && y == 10;
            image::Luma([if mark {
                0
            } else if inside {
                ink(x, y)
            } else {
                255
            }])
        });
        // The right bar is still flat and may go; the inked left one may not.
        let kept = crop(&inked_margin, flat_border(&inked_margin));
        assert_eq!(kept.dimensions(), (32, 20));
        assert_eq!(
            kept.get_pixel(0, 10).0[0],
            0,
            "the inked left margin must survive"
        );

        // An entirely blank page must survive rather than collapse to nothing.
        let blank = GrayImage::from_fn(30, 20, |_, _| image::Luma([255]));
        let (w, h) = crop(&blank, flat_border(&blank)).dimensions();
        assert!(w > 0 && h > 0, "a blank page must not vanish");
    }

    /// The reason a letterbox trim must be symmetric, which is the whole point of
    /// doing it at all.
    ///
    /// Trimming a padded spread is only useful if the two pages inside it still
    /// split down the middle afterwards. Detection alone does not give that. On
    /// Tongari volume 9 the right bar reads as its true 197 pixels but the left
    /// reads 8, because a one-column scan seam at x=8 stops the left scan dead.
    /// Cropping those unequal amounts leaves the content off-centre, the halves
    /// get cut in the wrong place, and the alignment goes from 88 runs to 98 —
    /// worse than not trimming. Taking 197 from both sides is what took it to 3.
    #[test]
    fn a_letterbox_comes_off_both_sides_equally_so_a_spread_still_splits_in_half() {
        // A padded spread, 400 wide: 40 flat columns, 320 of content, 40 flat.
        // The seam at x=8 is the real artifact — one column of scanner noise. It
        // has to vary *down* the column to register, because a column is judged
        // flat against its own top pixel rather than against white, so a uniform
        // off-white column would read as just another bar.
        let padded = GrayImage::from_fn(400, 100, |x, y| {
            let inside = (40..360).contains(&x);
            image::Luma([if inside {
                ink(x, y)
            } else if x == 8 {
                230 + (y % 20) as u8
            } else {
                255
            }])
        });

        // Detection on its own is lopsided: the seam truncates the left scan.
        let found = flat_border(&padded);
        assert_eq!(found.left, 8, "the seam stops the left scan early");
        assert_eq!(found.right, 40, "the right bar is measured correctly");

        // Symmetrising takes the larger of each opposing pair.
        let even = found.symmetric();
        assert_eq!((even.left, even.right), (40, 40));
        let vertical = found.top.max(found.bottom);
        assert_eq!((even.top, even.bottom), (vertical, vertical));

        // Which is the property that matters: the midpoint of the cropped page
        // still falls where the two pages of the spread meet.
        let trimmed = crop(&padded, even);
        assert_eq!(trimmed.dimensions(), (320, 100));
        assert_eq!(
            trimmed.width() / 2,
            160,
            "the split must land between the two pages"
        );

        // An already-even border is left exactly as it is.
        let square = Border {
            left: 5,
            right: 5,
            top: 2,
            bottom: 2,
        };
        assert_eq!(square.symmetric(), square);
    }

    /// A page too small to measure is left alone, and eight pixels is where
    /// that starts. Below it the quarter-per-side cap is one pixel or zero and
    /// the scan reduces to noise; the guard exists so a thumbnail or a spacer
    /// image cannot report a border and have a quarter of itself removed.
    ///
    /// Both sides of the threshold have to be asserted. Seven alone cannot see
    /// the difference between `< 8` and `<= 8`, and eight alone cannot see the
    /// difference between `< 8` and `== 8` — and each axis needs its own case,
    /// because either one being small is enough to stop the measurement.
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

    /// No side gives up more than a quarter of the page, on any of the four.
    ///
    /// The cap is what separates this from the autocrop that was rejected: a
    /// page whose margins are genuinely wide — a single panel floating in
    /// white — would otherwise be trimmed to the panel, which is exactly the
    /// content-following crop that added variance rather than removing it.
    ///
    /// Every one of the four scans has its own bound written out separately,
    /// and each is a small piece of arithmetic on `w` or `h` that no test
    /// reached: the existing letterbox fixtures all have bars narrower than the
    /// cap, so the caps never bound and every operator in them was free.
    #[test]
    fn no_side_gives_up_more_than_a_quarter_of_the_page() {
        // 40x40 with a 10x10 panel in the middle: fifteen flat lines on every
        // side, and a cap of ten. What comes off is the cap, not the fifteen.
        let framed = GrayImage::from_fn(40, 40, |x, y| {
            let panel = (15..25).contains(&x) && (15..25).contains(&y);
            image::Luma([if panel { ink(x, y) } else { 255 }])
        });

        assert_eq!(
            flat_border(&framed),
            Border {
                left: 10,
                right: 10,
                top: 10,
                bottom: 10,
            },
            "fifteen flat lines a side, but a quarter is the most that may go"
        );

        // And the page that survives it is half of each axis, not the panel.
        assert_eq!(crop(&framed, flat_border(&framed)).dimensions(), (20, 20));
    }

    /// A letterbox across the top and bottom is measured the same way as one
    /// down the sides. Nothing asserted the horizontal scan before: every
    /// fixture in this module pads left and right, because that is the shape
    /// the corpus has, and so `flat_row`'s own tolerance was never read.
    #[test]
    fn a_letterbox_above_and_below_comes_off_the_same_way_as_one_at_the_sides() {
        // 60x40: eight flat rows at each end, content spanning the full width
        // between them, so nothing may come off the sides.
        let banded = GrayImage::from_fn(60, 40, |x, y| {
            let inside = (8..32).contains(&y);
            image::Luma([if inside { ink(x, y) } else { 255 }])
        });

        assert_eq!(
            flat_border(&banded),
            Border {
                left: 0,
                right: 0,
                top: 8,
                bottom: 8,
            },
        );
        assert_eq!(crop(&banded, flat_border(&banded)).dimensions(), (60, 24));
    }

    /// `crop` removes a border from both axes, and refuses one that would leave
    /// nothing behind.
    ///
    /// The vertical pair is what no other test here exercises, and a page of
    /// zero width or height is worse than an uncropped one: every stage after
    /// this resizes it to the hash grid.
    #[test]
    fn crop_takes_the_vertical_border_off_and_refuses_one_that_would_leave_nothing() {
        let page = GrayImage::from_fn(40, 40, |x, y| image::Luma([ink(x, y)]));

        // Three and two rather than a matching pair: an even border cannot tell
        // `top + bottom` from `top - bottom`, and both read as five.
        let vertical = Border {
            left: 0,
            right: 0,
            top: 3,
            bottom: 2,
        };
        let trimmed = crop(&page, vertical);
        assert_eq!(trimmed.dimensions(), (40, 35));
        assert_eq!(
            trimmed.get_pixel(0, 0).0[0],
            page.get_pixel(0, 3).0[0],
            "the crop starts below the top border, not at the top of the page"
        );

        // A border that would swallow an axis leaves the page whole, and each
        // axis has to be tried on its own: a guard that required *both* to
        // collapse would still return a zero-width page for one of them.
        for swallowing in [
            Border {
                left: 40,
                right: 0,
                top: 0,
                bottom: 0,
            },
            Border {
                left: 0,
                right: 0,
                top: 40,
                bottom: 0,
            },
        ] {
            assert_eq!(
                crop(&page, swallowing).dimensions(),
                (40, 40),
                "{swallowing:?} leaves nothing, so nothing is taken"
            );
        }

        // An absent border is not a crop at all.
        assert_eq!(crop(&page, Border::default()).dimensions(), (40, 40));
    }

    /// Page order is what the whole alignment is indexed by, so a partition that
    /// loses or reorders indices would misalign an entire edition while looking
    /// perfectly healthy. Both halves must come back in ascending order and
    /// together account for every page exactly once.
    #[test]
    fn partitioning_by_size_keeps_every_page_exactly_once_and_in_order() {
        let small = 1024;
        let huge = LARGE_PAGE_BYTES + 1;
        let (parallel, alone) = partition_by_size(&[small, huge, small, huge, small]);

        assert_eq!(parallel, vec![0, 2, 4]);
        assert_eq!(alone, vec![1, 3]);

        // Exactly at the threshold counts as large: the bound is what protects
        // memory, so it should err towards caution.
        let (p, a) = partition_by_size(&[LARGE_PAGE_BYTES]);
        assert!(p.is_empty());
        assert_eq!(a, vec![0]);

        assert_eq!(partition_by_size(&[]), (vec![], vec![]));
    }

    /// The threshold is a memory bound, and the test has to be written in
    /// page sizes rather than in the constant's own name.
    ///
    /// `partition_by_size` above says everything about the rule except how big
    /// the pages it sorts actually are, because it is written in terms of
    /// `LARGE_PAGE_BYTES` and so agrees with the constant whatever it says.
    /// The two ways `32 * 1024 * 1024` can be got wrong leave 1 MB and 33 KB —
    /// both of which send every ordinary manga page down the one-at-a-time
    /// path, costing only wall-clock, which nothing fails on.
    #[test]
    fn the_large_page_rule_keeps_ordinary_pages_parallel_and_archival_scans_alone() {
        // Three megabytes is the top of the ordinary range; a Color Walk page
        // is 430 MB, and twelve of those at once is the ten gigabytes the rule
        // exists to prevent.
        let (parallel, alone) = partition_by_size(&[3 * 1024 * 1024, 430 * 1024 * 1024]);
        assert_eq!(parallel, vec![0], "a 3 MB page decodes alongside others");
        assert_eq!(alone, vec![1], "a 430 MB page decodes on its own");

        // And the bound itself: twelve of these at once is 384 MB decoded.
        assert_eq!(LARGE_PAGE_BYTES, 32 * 1024 * 1024);
    }

    /// A page with enough structure to survive being resized to the hash grid.
    fn textured_page() -> GrayImage {
        let mut x = 0x2545_F491_4F6C_DD1Du64;
        GrayImage::from_fn(200, 280, |_, _| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            // 60..=179, so brightening cannot saturate and destroy the very
            // comparisons this is meant to preserve.
            image::Luma([60 + (x % 120) as u8])
        })
    }

    /// The point of a difference hash is that it encodes relationships between
    /// regions rather than their absolute values, so the same artwork scanned
    /// at a different exposure still hashes the same. Two editions never share
    /// a gamma curve, so without this the whole approach fails.
    #[test]
    fn brightening_a_page_does_not_change_its_hash() {
        let page = textured_page();
        let brighter = GrayImage::from_fn(page.width(), page.height(), |x, y| {
            image::Luma([page.get_pixel(x, y).0[0].saturating_add(20)])
        });

        assert_eq!(fingerprint(&page), fingerprint(&brighter));
    }

    /// The member's letterbox is a median over sampled pages, and each of the
    /// three decisions in that sentence is load-bearing.
    ///
    /// The **median** is why a full-bleed cover cannot cancel the trim for the
    /// whole volume — it reports no border, it is page zero, and page zero is
    /// always sampled. A minimum was tried and does exactly that.
    ///
    /// The **sampling step** decides which pages get a vote, and getting it
    /// wrong is invisible: the function still returns a border, still of a
    /// plausible size, measured from the wrong pages.
    #[test]
    fn a_letterbox_is_the_median_of_sampled_pages_so_a_full_bleed_cover_cannot_cancel_it() {
        // Twenty-four pages, so the step is two and the twelve even ones are
        // sampled. Page zero is a full-bleed cover with no bars at all, the
        // other even pages carry ten pixels a side, and the odd ones carry
        // twenty. The sample is therefore eleven tens and one zero, median ten.
        // Every page would be that plus twelve twenties, median twenty; page
        // zero alone would be nothing.
        let pages: Vec<Page> = (0..24u32)
            .map(|index| {
                let bar = match index {
                    0 => 0,
                    even if even % 2 == 0 => 10,
                    _ => 20,
                };
                let image = GrayImage::from_fn(100, 20, |x, y| {
                    let inside = (bar..100 - bar).contains(&x);
                    image::Luma([if inside { ink(x, y) } else { 255 }])
                });
                encoded(&format!("p{index:02}.png"), &image)
            })
            .collect();

        assert_eq!(
            common_border(&pages),
            Border {
                left: 10,
                right: 10,
                top: 0,
                bottom: 0,
            },
        );
    }

    /// Half hashes exist so a scanned spread can be matched against the two
    /// single pages another edition holds, which fixes both the rule and the
    /// split: wider than tall, cut down the middle.
    ///
    /// A square page is not a spread. Neither is a portrait one. And a split
    /// anywhere but the middle produces two hashes that match nothing, while
    /// still being two hashes of the right type in the right place.
    #[test]
    fn only_a_page_wider_than_it_is_tall_gets_halves_and_they_split_at_its_middle() {
        // The two halves carry the same texture transposed, so each is dense in
        // both directions — no flat line, and so no border to confuse the
        // dimensions — while still being structurally different from each
        // other. Asserted below rather than assumed.
        let landscape = GrayImage::from_fn(120, 60, |x, y| {
            image::Luma([if x < 60 { ink(x, y) } else { ink(y, x) }])
        });
        let portrait = GrayImage::from_fn(60, 120, |x, y| image::Luma([ink(x, y)]));
        let square = GrayImage::from_fn(80, 80, |x, y| image::Luma([ink(x, y)]));

        let pages = [
            encoded("wide.png", &landscape),
            encoded("tall.png", &portrait),
            encoded("square.png", &square),
        ];
        let described = describe_pages(&pages).expect("all three decode");

        assert_eq!((described[0].width, described[0].height), (120, 60));
        assert_eq!(described[0].hash, fingerprint(&landscape));

        let left = image::imageops::crop_imm(&landscape, 0, 0, 60, 60).to_image();
        let right = image::imageops::crop_imm(&landscape, 60, 0, 60, 60).to_image();
        assert_ne!(
            fingerprint(&left),
            fingerprint(&right),
            "the halves must hash differently or nothing below this can fail"
        );
        assert_ne!(
            fingerprint(&left),
            fingerprint(&landscape),
            "a half must hash differently from the whole, for the same reason"
        );
        assert_eq!(
            described[0].halves,
            Some((fingerprint(&left), fingerprint(&right))),
            "the split is the midpoint, and the halves are in page order"
        );

        assert!(
            described[1].halves.is_none(),
            "a portrait page holds one page"
        );
        assert!(
            described[2].halves.is_none(),
            "a square page is not a spread"
        );
    }

    /// Distance is over the bits that *differ*, which is the whole content of
    /// the word: `credits` prints this number for a person to read as "distance
    /// 2" or "distance 99", and an or in place of the xor turns it into a count
    /// of set bits — a number that rises with how dark two pages are rather
    /// than with how unlike they are, and never reaches zero for a match.
    #[test]
    fn hamming_counts_the_bits_that_differ_and_nothing_else() {
        let hash = PageHash([u64::MAX, 0, 0xAAAA_AAAA_AAAA_AAAA, 1]);

        assert_eq!(hamming(hash, hash), 0, "a page is distance 0 from itself");
        assert_eq!(
            hamming(hash, PageHash([0; 4])),
            97,
            "64 bits, then none, then every other one, then one"
        );
        assert_eq!(
            hamming(hash, PageHash([0; 4])),
            hamming(PageHash([0; 4]), hash),
            "distance does not depend on which page is named first"
        );
    }
}
