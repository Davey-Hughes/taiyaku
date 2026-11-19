//! Fingerprinting an edition's pages in parallel, the large ones one at a
//! time; the algorithm for one page is `taiyaku_serve::fingerprint`'s.

#[cfg(test)]
use std::io::Cursor;

use image::GrayImage;
use rayon::prelude::*;
use taiyaku_core::PageHash;

use taiyaku_read::Page;
use taiyaku_serve::fingerprint::describe_page;
pub use taiyaku_serve::fingerprint::{
    LARGE_PAGE_BYTES, common_border, decode_luma, decode_page, fingerprint,
};
// Only the test-only experiments name it outside the tests.
#[cfg(test)]
pub use taiyaku_serve::fingerprint::Border;
#[cfg(test)]
use taiyaku_serve::fingerprint::{crop, flat_border};

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
        crate::cache::CachedPage::from(describe_page(raw, border))
    })
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
