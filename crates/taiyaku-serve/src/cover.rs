//! One cover per edition, for the OPDS entries — and, since M5a, one
//! filmstrip tile per page.
//!
//! Scoped deliberately at first: every OPDS entry needs a cover, so that
//! shipped alone; the filmstrip's thumbnail-per-page route waited for M5a,
//! when its access pattern — one member's cover against ~195 of its tiles —
//! was known rather than guessed. One cover per edition and 195 thumbnails
//! per volume are different problems, and solving them together on a guess
//! about the second would have been the wrong order. Now that the pattern is
//! known, both share one decoder (`thumbnail_at`) and one cache-publish path
//! (`at_path`); only the cache key and the output size differ.
//!
//! Cached by the member's `sha256` **and** by the parameters the image was
//! generated with — the long edge (`LONG_EDGE` for a cover, `THUMB_LONG_EDGE`
//! for a tile) and a format version, both folded into the cache path. A tile
//! is additionally keyed by the page's offset within its member, because a
//! member has one cover but many tiles. Every axis is content-addressed: the
//! same archive under two sidecars generates one cover, an archive that is
//! replaced generates a new one without anything having to notice, and
//! raising a long edge for a higher-DPI screen — or changing the output
//! format or resize algorithm — leaves every previously cached file sitting
//! at its old path, never read again, rather than silently served at the
//! wrong size.
//!
//! That guarantee rests on one thing this module does not check: the
//! sidecar's recorded `sha256` being accurate for the member as it sits on
//! disk. Every other route in this crate reads its file fresh on each request
//! and so self-corrects if a member is edited in place; this cache does not —
//! a `sha256` that has gone stale relative to the bytes it names will go on
//! serving the cover generated under the old contents until the cache
//! directory is cleared by hand.

use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};

use taiyaku_read::MAX_DECODE_ALLOC;

/// The long edge of a generated cover, in pixels.
///
/// Large enough for a grid on a tablet, small enough that generating one is
/// not the slow part of opening a feed.
const LONG_EDGE: u32 = 512;

/// The long edge of a filmstrip tile, in pixels.
///
/// A cover is 512 because it fills a grid cell; a tile sits in a strip along
/// the bottom edge and there are ~195 of them per volume, so it is sized for
/// the strip rather than for a grid. Folded into the cache path exactly as
/// `LONG_EDGE` is, so changing it invalidates by construction.
const THUMB_LONG_EDGE: u32 = 160;

/// Bumped whenever a change to the output format or the resize algorithm
/// would make an already-cached file the wrong output for the same input.
/// `LONG_EDGE` is folded into the cache path separately, since it is the
/// parameter most likely to move on its own (a higher-DPI target screen);
/// this covers everything else about how a cover is produced.
const FORMAT_VERSION: u32 = 1;

/// Where a cover for this member is cached.
///
/// The path itself is the cache invalidation: changing `LONG_EDGE` or bumping
/// `FORMAT_VERSION` changes every path this produces, so a file cached under
/// the old parameters is simply never looked up again rather than served as a
/// stale size or format.
pub fn cache_path(dir: &Path, sha256: &str) -> PathBuf {
    keyed_path(dir, sha256, LONG_EDGE, FORMAT_VERSION)
}

/// Where a filmstrip tile for one page of one member is cached.
///
/// Keyed on the member's `sha256` *and the page's offset within it*, because
/// a member has one cover and 195 tiles. The offset rather than the
/// edition-flat index, so a tile survives the edition gaining a member ahead
/// of this one.
pub fn thumb_cache_path(dir: &Path, sha256: &str, offset: u32) -> PathBuf {
    dir.join(format!(
        "{sha256}-p{offset}-{THUMB_LONG_EDGE}v{FORMAT_VERSION}.jpg"
    ))
}

/// `cache_path`'s logic, taking the generation parameters as arguments rather
/// than reading the module constants directly, so a test can hold `sha256`
/// fixed and vary only the parameters without needing a second copy of
/// `LONG_EDGE` to change.
fn keyed_path(dir: &Path, sha256: &str, long_edge: u32, format_version: u32) -> PathBuf {
    dir.join(format!("{sha256}-{long_edge}v{format_version}.jpg"))
}

/// Where a **reading-sized** page is cached — one per member, offset and long
/// edge.
///
/// Distinct from `thumb_cache_path` by the `r` in the name rather than by the
/// size alone: a tile and a reading page could in principle be asked for at the
/// same long edge, and two different outputs sharing one path is the one
/// failure a content-addressed cache must not have. A tile is cropped to a
/// strip's proportions in the reader; this is the whole page.
pub fn page_cache_path(dir: &Path, sha256: &str, offset: u32, long_edge: u32) -> PathBuf {
    dir.join(format!(
        "{sha256}-p{offset}-r{long_edge}v{PAGE_FORMAT_VERSION}.jpg"
    ))
}

/// `FORMAT_VERSION`'s counterpart for reading-sized pages, versioned separately
/// so a change to how a *page* is produced does not throw away every cover and
/// tile in a large library, or the reverse. Bumped when the resize filter moved
/// off `thumbnail`'s sampler — see `fit_at`.
const PAGE_FORMAT_VERSION: u32 = 2;

/// The largest long edge this server will resize to.
///
/// A ceiling on what a client can ask for, so a hand-edited or hostile `?w=`
/// cannot fill the cache directory with one entry per pixel value. `web/`'s own
/// ladder tops out well below this; the bound exists for requests that did not
/// come from it.
pub const MAX_FIT_LONG_EDGE: u32 = 4096;

/// Fit a page inside `long_edge`, or say that it already fits.
///
/// **`None` means "serve the original bytes", and it is the answer that makes
/// this safe to apply to every edition.** A page whose long edge is already
/// within the request is returned untouched rather than re-encoded: re-encoding
/// it would cost quality for no pixels gained, and on this corpus `raw-mono`
/// (685×1024) and `en-fan` (1114×1600) are frequently already smaller than what
/// a reader's viewport asks for.
///
/// The check reads the image header only — `into_dimensions` parses the SOF
/// marker and stops, where `decode` would rasterise the whole frame — so
/// deciding costs microseconds against the ~75 ms a full decode of this
/// corpus's largest pages takes. That is what lets the decision be made per
/// request without a cache entry to remember it in.
///
/// Always JPEG on the resize path, for `thumbnail_at`'s reason: one declared
/// content type rather than whatever the archive happened to hold.
pub fn fit_at(page: &[u8], long_edge: u32) -> Result<Option<Vec<u8>>, String> {
    // Two readers, and the second is not waste. `into_dimensions` consumes the
    // one it is called on, and it is what makes the cheap path cheap: it stops
    // at the frame header, so a page that already fits costs microseconds and no
    // decode at all. Only a page that genuinely needs resizing pays for a second
    // header parse, which is nothing beside the decode that follows it.
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    let mut reader = image::ImageReader::new(std::io::Cursor::new(page))
        .with_guessed_format()
        .map_err(|e| format!("fit: undecodable page: {e}"))?;
    reader.limits(limits);
    let (width, height) = reader
        .into_dimensions()
        .map_err(|e| format!("fit: unreadable dimensions: {e}"))?;
    if width.max(height) <= long_edge {
        return Ok(None);
    }

    // **Not `thumbnail`'s sampler, and the filter is the reason.** `image`
    // documents it as "a fast integer algorithm where each source pixel
    // contributes to exactly one target pixel… may give aliasing artifacts" —
    // the right trade for a 512px cover and the wrong one for the page a reader
    // is looking at, since manga is screentone and thin ink, the two things
    // that alias worst. Reducing 2147px to 768px through each differs at
    // **25.3 dB PSNR**, with the hatching visibly broken up on the fast path.
    //
    // `CatmullRom` rather than `Lanczos3`: both cost the same here (about 5 ms
    // on a 100 ms generation, measured), and Lanczos rings on high-contrast
    // edges — every panel border and every letter on the page. A little
    // softness is a better failure than haloed ink.
    let decoded = decode_limited(page, "fit")?;
    let scaled = decoded.resize(
        long_edge,
        long_edge,
        image::imageops::FilterType::CatmullRom,
    );
    Ok(Some(encode_jpeg(scaled, "fit")?))
}

/// Read a cached reading-sized page, or make one and cache it.
///
/// `None` from `fit_at` is passed straight through and **nothing is cached**:
/// the correct output for that request is the member's own bytes, which the
/// caller already has and which no cache could hold more cheaply than the
/// archive does. A per-request header parse is the price, and it is
/// microseconds.
///
/// Shares `GENERATION_LIMIT` with covers and tiles rather than taking a bound
/// of its own. All three do the same expensive thing — decode a page at up to
/// `MAX_DECODE_ALLOC` — so one budget over all of them is what actually bounds
/// peak memory; separate budgets would each be safe alone and add up to
/// something that is not.
pub fn cached_or_generated_page(
    dir: &Path,
    sha256: &str,
    offset: u32,
    long_edge: u32,
    page: impl FnOnce() -> Result<Vec<u8>, String>,
) -> Result<Option<Vec<u8>>, String> {
    let path = page_cache_path(dir, sha256, offset, long_edge);
    if let Ok(found) = std::fs::read(&path) {
        return Ok(Some(found));
    }
    let generated = {
        let _permit = GENERATION_LIMIT.acquire();
        fit_at(&page()?, long_edge)?
    };
    let Some(bytes) = generated else {
        return Ok(None);
    };
    if std::fs::create_dir_all(dir).is_ok() {
        // Discarded for `at_path`'s reason: the page was generated and the
        // reader gets it, so a cache that cannot be written is slow rather
        // than broken.
        let _ = taiyaku_read::atomic::write(&path, &bytes, "jpg");
    }
    Ok(Some(bytes))
}

/// How many cover generations — fetching a page and decoding it — may run at
/// once.
///
/// Unbounded is dangerous at `MAX_DECODE_ALLOC`'s ceiling: `notes.md`
/// measured four concurrent art-book decodes peaking at 3.0 GB, and the
/// `image` crate's 512 MB *default* is precisely what protected the aligner
/// from exactly this — a cover request raises that ceiling to 3 GiB (above)
/// with no concurrency limit of its own. A cold OPDS grid view issues one
/// cover request per edition at once, so nothing upstream bounds how many
/// decodes could otherwise overlap. Two permits caps the worst case at twice
/// a single decode's ceiling rather than however many requests a client
/// happens to fire together, while still letting one generation proceed
/// while another finishes.
const MAX_CONCURRENT_GENERATIONS: usize = 2;

/// A minimal counting semaphore, built on `Mutex` and `Condvar` so this stays
/// dependency-free.
///
/// Guards only the *generation* step (fetching a page and decoding it) — see
/// `cached_or_generated` — never the cache-hit read, which has to stay fully
/// parallel or a warm OPDS grid would queue behind a decode limit it does not
/// need.
#[derive(Debug)]
struct Semaphore {
    available: Mutex<usize>,
    released: Condvar,
}

impl Semaphore {
    const fn new(permits: usize) -> Self {
        Self {
            available: Mutex::new(permits),
            released: Condvar::new(),
        }
    }

    /// Block until a permit is free, then hold it until the returned guard
    /// drops.
    fn acquire(&self) -> SemaphoreGuard<'_> {
        let mut available = self
            .available
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while *available == 0 {
            available = self
                .released
                .wait(available)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        *available -= 1;
        SemaphoreGuard { semaphore: self }
    }
}

#[derive(Debug)]
struct SemaphoreGuard<'a> {
    semaphore: &'a Semaphore,
}

impl Drop for SemaphoreGuard<'_> {
    fn drop(&mut self) {
        let mut available = self
            .semaphore
            .available
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *available += 1;
        drop(available);
        self.semaphore.released.notify_one();
    }
}

static GENERATION_LIMIT: Semaphore = Semaphore::new(MAX_CONCURRENT_GENERATIONS);

/// Decode a page with this crate's allocation ceiling in force.
///
/// `image::load_from_memory` takes no `Limits`, so it inherits the crate's
/// 512 MB default and rejects this corpus's largest pages outright. Building an
/// `ImageReader` and setting limits explicitly — exactly as
/// `fingerprint.rs::decode_luma` does — is what raises that ceiling without
/// removing it.
///
/// `what` names the caller in the error, so a failure says whether a cover, a
/// tile or a reading page could not be read.
fn decode_limited(page: &[u8], what: &str) -> Result<image::DynamicImage, String> {
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    let mut reader = image::ImageReader::new(std::io::Cursor::new(page))
        .with_guessed_format()
        .map_err(|e| format!("{what}: undecodable page: {e}"))?;
    reader.limits(limits);
    reader
        .decode()
        .map_err(|e| format!("{what}: undecodable page: {e}"))
}

/// Encode as JPEG, whatever came in.
///
/// Always JPEG: an OPDS entry declares one type per link and a client is
/// entitled to believe it, so emitting whatever the page happened to be would
/// make that declaration a guess. The reading-page route declares the same for
/// the same reason.
fn encode_jpeg(image: image::DynamicImage, what: &str) -> Result<Vec<u8>, String> {
    let mut out = std::io::Cursor::new(Vec::new());
    image
        .into_rgb8()
        .write_to(&mut out, image::ImageFormat::Jpeg)
        .map_err(|e| format!("{what}: encoding: {e}"))?;
    Ok(out.into_inner())
}

/// Shrink a page into a cover.
///
/// Always JPEG, whatever came in: an OPDS entry declares one type per link and
/// a client is entitled to believe it, so emitting whatever the page happened
/// to be would make that declaration a guess.
pub fn thumbnail(page: &[u8]) -> Result<Vec<u8>, String> {
    thumbnail_at(page, LONG_EDGE)
}

/// `thumbnail`, with the long edge as an argument.
///
/// Split out for the filmstrip, which wants the same decode limits, the same
/// always-JPEG rule and a different size. One decoder rather than two, so a
/// change to the limits cannot reach covers and miss tiles.
pub fn thumbnail_at(page: &[u8], long_edge: u32) -> Result<Vec<u8>, String> {
    let decoded = decode_limited(page, "cover")?;
    // `thumbnail`'s own fast sampler, kept for covers and tiles: at 512px and
    // 160px its aliasing is invisible and it is the cheaper of the two. A page
    // a reader actually looks at goes through `fit_at` instead, which says why.
    encode_jpeg(decoded.thumbnail(long_edge, long_edge), "cover")
}

/// Read a cached cover, or make one and cache it.
///
/// A cache that cannot be written is not an error — the cover was generated
/// and the client should get it. Failing here would make a read-only cache
/// directory break the feed rather than merely slow it.
///
/// Important 6 (whole-branch review): generation — fetching the page and
/// decoding it — runs behind `GENERATION_LIMIT`, bounding how many decodes at
/// up to `MAX_DECODE_ALLOC` each can run at once. The cache-hit read above
/// this line is deliberately outside the permit, so a warm grid stays fully
/// parallel; only a cold one, which is what actually decodes anything, is
/// bounded.
pub fn cached_or_generated(
    dir: &Path,
    sha256: &str,
    page: impl FnOnce() -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    at_path(&cache_path(dir, sha256), dir, page, |bytes| {
        thumbnail(&bytes)
    })
}

/// Read a cached filmstrip tile, or make one and cache it.
pub fn cached_or_generated_thumb(
    dir: &Path,
    sha256: &str,
    offset: u32,
    page: impl FnOnce() -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    at_path(&thumb_cache_path(dir, sha256, offset), dir, page, |bytes| {
        thumbnail_at(&bytes, THUMB_LONG_EDGE)
    })
}

/// The shared body: cache hit outside the permit, generation inside it,
/// atomic publish via a per-attempt temp name.
///
/// Takes the decode-and-resize step as a closure — `thumbnail` for a cover,
/// `thumbnail_at(_, THUMB_LONG_EDGE)` for a tile — rather than a bare
/// `long_edge`, so each caller keeps calling the entry point named in its own
/// doc comment instead of `at_path` re-deriving which one to use from a
/// number.
///
/// Both callers get the same concurrency bound, which matters more for tiles
/// than for covers: a cold filmstrip asks for every page of a volume, and
/// unbounded decodes at `MAX_DECODE_ALLOC` each is what
/// `MAX_CONCURRENT_GENERATIONS` exists to prevent.
fn at_path(
    path: &Path,
    dir: &Path,
    page: impl FnOnce() -> Result<Vec<u8>, String>,
    generate: impl FnOnce(Vec<u8>) -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    if let Ok(found) = std::fs::read(path) {
        return Ok(found);
    }
    let generated = {
        let _permit = GENERATION_LIMIT.acquire();
        generate(page()?)?
    };
    if std::fs::create_dir_all(dir).is_ok() {
        // A cache that cannot be written is not an error — the cover was
        // already generated and the client gets it regardless — so the
        // result of the write-and-rename is discarded rather than
        // propagated. `taiyaku_read::atomic::write` is unique per attempt, so
        // two concurrent misses for the same edition — exactly what a cold
        // OPDS grid produces — each get their own file to write and rename,
        // rather than racing on one shared name, and it cleans up its own
        // temp file on a failed rename.
        let _ = taiyaku_read::atomic::write(path, &generated, "jpg");
    }
    Ok(generated)
}

#[cfg(test)]
mod tests;
