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
    /// the other side. Capped at a quarter per side by `flat_border`.
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
/// hash grid and wrecks the comparison against a clean page.
///
/// This is not an autocrop to the inked bounding box, which moves with
/// whatever each page happens to contain; that crop adds more variance than
/// it removes.
///
/// Per-member rather than per-page, and that distinction is the whole point. A
/// letterbox is identical on every page of a volume, so removing it is stable.
/// Trimming each page independently is not: pages whose art reaches the edge
/// trim less than their neighbours and the content then sits differently in
/// each hash. That is the same failure as the ink-bounding-box autocrop,
/// arrived at from a different direction.
///
/// Sampled rather than exhaustive because a bar present on pages spread through
/// the volume is present throughout, and decoding twice costs real time.
///
/// The **median** across samples, not the minimum. A minimum is wrong: a
/// single full-bleed page — a cover, typically the first thing sampled — has no
/// border at all and drops the minimum to zero, cancelling the trim for the
/// entire member. A median survives a handful of such pages.
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
    if width == 0 || height == 0 || (border == Border::default()) {
        page.clone()
    } else {
        image::imageops::crop_imm(page, border.left, border.top, width, height).to_image()
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

    if right <= left || bottom <= top {
        Border::default()
    } else {
        Border {
            left,
            right: w - 1 - right,
            top,
            bottom: h - 1 - bottom,
        }
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
        if *size >= LARGE_PAGE_BYTES {
            alone.push(index);
        } else {
            parallel.push(index);
        }
    }
    (parallel, alone)
}

/// Decode pages, keeping memory bounded by decoding the large ones one at a time.
///
/// Results come back in page order regardless of which path each took, because
/// every later index — slots, runs, the whole alignment — is derived from it.
///
/// Detection needs the same bounded parallel decode and must not carry a
/// second copy of the large-page rule: the ceiling is `MAX_DECODE_ALLOC` and
/// the corpus holds 430 MB TIFFs, so decoding the large ones one at a time is
/// what keeps a detection run inside memory.
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
/// Decoding dominates the work and is embarrassingly parallel, so it is worth
/// spreading across cores. This is also why fingerprints are cached per
/// archive rather than recomputed per pair.
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
/// what makes the result independent of exposure and of the page's resolution.
///
/// Chosen over a DCT-based perceptual hash because at equal width the two
/// scored within noise of each other on the corpus, and this one needs
/// no transform.
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
mod tests;
