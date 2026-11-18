//! Reducing a decoded page to a structural hash, and a file's pages to the
//! records the fingerprint pass stores.
//!
//! The one copy of the algorithm, so the CLI and the server hash a page alike.
//! The thresholds that decide a content match were set on hashes made this
//! way (`docs/notes.md`, "Edition matching by content"), so a change here
//! changes what they mean, and raises [`VERSION`].

use std::io::Cursor;

use image::GrayImage;
use taiyaku_core::PageHash;

use taiyaku_read::{MAX_DECODE_ALLOC, Page};

/// The algorithm and [`encode`]'s layout together: raised by a change to
/// either, so a row stored another way is read again.
pub const VERSION: u32 = 1;

/// Hash grid edge. 16 gives the 256 bits `PageHash` carries.
const GRID: u32 = 16;

/// At or above this encoded size a page decodes on its own: in the server one
/// at a time across the store, every fingerprint worker sharing one lock
/// (`fingerprint_pass`'s `LargePages`), and in the CLI outside its parallel
/// batch.
///
/// Archival scans exist whose pages decode to hundreds of megabytes each
/// (`docs/notes.md`, "Art books cannot be aligned, and the solver does not say
/// so"), so page size has to bound concurrency; ordinary pages decode side by side.
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
    pub fn symmetric(self) -> Self {
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

/// One decoded page as [`describe_page`] records it: its size once the border
/// is off, its hash, and its halves' hashes when it is wider than tall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PagePrint {
    pub width: u32,
    pub height: u32,
    pub hash: PageHash,
    pub halves: Option<(PageHash, PageHash)>,
}

/// The pages [`common_border`] measures: the first, then every
/// `page_count / 12`th, so every page of a file under 24. Public so a caller
/// that reads a file a page at a time measures the same pages.
pub fn border_sample(page_count: usize) -> impl Iterator<Item = usize> {
    (0..page_count).step_by((page_count / 12).max(1))
}

/// The border a file's measured pages agree on: each side's median, the upper
/// of the two middles when the count is even, evened up by
/// [`Border::symmetric`]. No measurement gives no border.
pub fn median_border(seen: &[Border]) -> Border {
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
    let seen: Vec<Border> = border_sample(pages.len())
        .filter_map(|i| decode_luma(&pages[i].bytes).ok())
        .map(|luma| flat_border(&luma))
        .collect();
    median_border(&seen)
}

/// `page` less `border`; the page whole when the border is zero or would
/// leave it no width or no height.
pub fn crop(page: &GrayImage, border: Border) -> GrayImage {
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
pub fn flat_border(page: &GrayImage) -> Border {
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

/// What the pass stores for one page: `raw` less `border`, hashed, with each
/// half hashed too when the page is wider than tall, as a joined spread is
/// matched half by half against two single pages.
pub fn describe_page(raw: &GrayImage, border: Border) -> PagePrint {
    let luma = &crop(raw, border);
    let (width, height) = luma.dimensions();
    let halves = (width > height).then(|| {
        let mid = width / 2;
        (
            fingerprint(&image::imageops::crop_imm(luma, 0, 0, mid, height).to_image()),
            fingerprint(&image::imageops::crop_imm(luma, mid, 0, mid, height).to_image()),
        )
    });
    PagePrint {
        width,
        height,
        hash: fingerprint(luma),
        halves,
    }
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

/// A record's tag: the page did not decode.
const NOT_DECODED: u8 = 0;
/// A record's tag: the page is hashed.
const HASHED: u8 = 1;
/// A record's tag: the page is hashed, and so are its halves.
const WITH_HALVES: u8 = 2;

/// A file's pages as the blob the pass stores: one record per page in
/// reading order, a tag byte, then for a hashed page its width and height as
/// `u32` and its hash as four `u64`, then for a page with halves the left
/// and right halves' hashes the same way, all little-endian.
pub fn encode(pages: &[Option<PagePrint>]) -> Vec<u8> {
    let mut blob = Vec::new();
    for page in pages {
        let Some(print) = page else {
            blob.push(NOT_DECODED);
            continue;
        };
        blob.push(if print.halves.is_some() {
            WITH_HALVES
        } else {
            HASHED
        });
        blob.extend_from_slice(&print.width.to_le_bytes());
        blob.extend_from_slice(&print.height.to_le_bytes());
        put_hash(&mut blob, print.hash);
        if let Some((left, right)) = print.halves {
            put_hash(&mut blob, left);
            put_hash(&mut blob, right);
        }
    }
    blob
}

fn put_hash(blob: &mut Vec<u8>, hash: PageHash) {
    for word in hash.0 {
        blob.extend_from_slice(&word.to_le_bytes());
    }
}

/// The pages a blob [`encode`] wrote holds.
///
/// # Errors
///
/// A tag none of the three, or a blob that ends inside a record: a short read
/// taken as fewer pages would misplace every page after it.
pub fn decode(blob: &[u8]) -> Result<Vec<Option<PagePrint>>, String> {
    let mut rest = blob;
    let mut pages = Vec::new();
    while let Some((&tag, after)) = rest.split_first() {
        rest = after;
        let page = pages.len();
        pages.push(match tag {
            NOT_DECODED => None,
            HASHED | WITH_HALVES => {
                let width = u32::from_le_bytes(take(&mut rest, page)?);
                let height = u32::from_le_bytes(take(&mut rest, page)?);
                let hash = take_hash(&mut rest, page)?;
                let halves = if tag == WITH_HALVES {
                    Some((take_hash(&mut rest, page)?, take_hash(&mut rest, page)?))
                } else {
                    None
                };
                Some(PagePrint {
                    width,
                    height,
                    hash,
                    halves,
                })
            }
            other => return Err(format!("page {page}: unknown record tag {other}")),
        });
    }
    Ok(pages)
}

fn take<const N: usize>(rest: &mut &[u8], page: usize) -> Result<[u8; N], String> {
    let (head, tail) = rest
        .split_first_chunk::<N>()
        .ok_or_else(|| format!("page {page}: the blob ends inside its record"))?;
    *rest = tail;
    Ok(*head)
}

fn take_hash(rest: &mut &[u8], page: usize) -> Result<PageHash, String> {
    let mut words = [0u64; 4];
    for word in &mut words {
        *word = u64::from_le_bytes(take(rest, page)?);
    }
    Ok(PageHash(words))
}

#[cfg(test)]
mod tests;
