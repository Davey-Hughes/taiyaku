//! Reducing a decoded page to a structural hash.

use std::io::Cursor;

use image::GrayImage;
use rayon::prelude::*;
use taiyaku_core::PageHash;

use crate::pages::Page;

/// Hash grid edge. 16 gives the 256 bits `PageHash` carries.
const GRID: u32 = 16;

/// Decode an encoded page to greyscale.
pub fn decode_luma(bytes: &[u8]) -> Result<GrayImage, String> {
    Ok(image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?
        .to_luma8())
}

/// Fingerprint every page of an edition.
///
/// Decoding dominates by three orders of magnitude — around 12 ms per page at
/// 2147x3056 against roughly 100 us for the entire similarity matrix — and it
/// is embarrassingly parallel, so it is worth spreading across cores. This is
/// also why fingerprints are cached per archive rather than recomputed per pair.
pub fn fingerprint_edition(pages: &[Page]) -> Result<Vec<PageHash>, String> {
    pages
        .par_iter()
        .map(|page| {
            let luma = decode_luma(&page.bytes).map_err(|e| format!("{}: {e}", page.name))?;
            Ok(fingerprint(&luma))
        })
        .collect()
}

/// Fingerprint an edition, cutting joined spreads into two hashable halves.
///
/// A landscape page contributes two hashes instead of one, so the alignment can
/// pair each half with the page the other edition split it into. The solver
/// stays strictly one-to-one; only the sequence it is given changes.
pub fn fingerprint_edition_split(
    pages: &[Page],
    direction: crate::spread::Direction,
) -> Result<(Vec<PageHash>, Vec<crate::spread::Slot>), String> {
    let per_page: Result<Vec<(Vec<PageHash>, bool)>, String> = pages
        .par_iter()
        .map(|page| {
            let luma = decode_luma(&page.bytes).map_err(|e| format!("{}: {e}", page.name))?;
            let (w, h) = luma.dimensions();
            if w > h {
                let mid = w / 2;
                let left = image::imageops::crop_imm(&luma, 0, 0, mid, h).to_image();
                let right = image::imageops::crop_imm(&luma, mid, 0, mid, h).to_image();
                // Hashes go in reading order to match the slots.
                let halves = match direction {
                    crate::spread::Direction::RightToLeft => {
                        vec![fingerprint(&right), fingerprint(&left)]
                    }
                    crate::spread::Direction::LeftToRight => {
                        vec![fingerprint(&left), fingerprint(&right)]
                    }
                };
                Ok((halves, true))
            } else {
                Ok((vec![fingerprint(&luma)], false))
            }
        })
        .collect();

    let per_page = per_page?;
    let landscape: Vec<bool> = per_page.iter().map(|(_, wide)| *wide).collect();
    let hashes: Vec<PageHash> = per_page.into_iter().flat_map(|(h, _)| h).collect();
    Ok((hashes, crate::spread::expand(&landscape, direction)))
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
