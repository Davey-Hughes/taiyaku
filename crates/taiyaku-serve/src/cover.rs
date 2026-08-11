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
        let _ = crate::atomic::write(&path, &bytes, "jpg");
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
        // propagated. `crate::atomic::write` is unique per attempt, so two
        // concurrent misses for the same edition — exactly what a cold OPDS
        // grid produces — each get their own file to write and rename,
        // rather than racing on one shared name, and it cleans up its own
        // temp file on a failed rename.
        let _ = crate::atomic::write(path, &generated, "jpg");
    }
    Ok(generated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A pixel channel value synthesized for a fixture image below.
    ///
    /// # Panics
    ///
    /// Panics if `v` exceeds `u8::MAX`. It cannot at either call site: `real_page_bytes`
    /// multiplies a coordinate `from_fn` bounds to `0..16` by 16, topping out at
    /// `15 * 16 == 240`, and the aspect-ratio fixture below takes `x % 256` / `y %
    /// 256`, always inside `0..256` by construction.
    fn channel(v: u32) -> u8 {
        u8::try_from(v).expect("channel value exceeds u8::MAX")
    }

    /// A real, small, decodable page. The shared `test_support::work`
    /// fixture's "pages" are ASCII strings and cannot be decoded — that is by
    /// design (see `bytes_that_are_not_an_image_fail_by_name`) — so any test
    /// that needs a cover actually produced builds its own image rather than
    /// weakening that fixture.
    fn real_page_bytes() -> Vec<u8> {
        let page = image::RgbImage::from_fn(16, 16, |x, y| {
            let r = channel(x * 16);
            let g = channel(y * 16);
            image::Rgb([r, g, 200])
        });
        let mut encoded = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(page)
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        encoded.into_inner()
    }

    /// A cover is smaller than the page it came from, and is a JPEG whatever
    /// the page was — an OPDS entry states one type and a client is entitled
    /// to believe it.
    #[test]
    fn a_cover_is_a_bounded_jpeg_whatever_the_page_was() {
        // A 900x1400 PNG, which is portrait like a manga page.
        let page = image::RgbImage::from_fn(900, 1400, |x, y| {
            let r = channel(x % 256);
            let g = channel(y % 256);
            image::Rgb([r, g, 128])
        });
        let mut encoded = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(page)
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();

        let cover = thumbnail(&encoded.into_inner()).unwrap();
        let decoded = image::load_from_memory(&cover).unwrap();

        assert!(cover.starts_with(b"\xff\xd8\xff"), "a cover is a JPEG");
        assert_eq!(decoded.height(), LONG_EDGE, "the long edge is bounded");
        assert!(decoded.width() <= LONG_EDGE, "and so is the short one");
        // The shape, not an exact pixel count: which way `thumbnail` rounds
        // 900x1400 into a 512 box is an implementation detail of `image`, and
        // pinning it would fail on a patch release without anything being
        // wrong. What must hold is that the page was not distorted.
        let before = 900.0 / 1400.0;
        let after = f64::from(decoded.width()) / f64::from(decoded.height());
        assert!(
            (before - after).abs() < 0.01,
            "aspect ratio changed: {before} -> {after}"
        );
    }

    /// Bytes that are not an image fail with a message rather than producing
    /// a zero-byte cover an OPDS client would render as a broken tile.
    #[test]
    fn bytes_that_are_not_an_image_fail_by_name() {
        let refused = thumbnail(b"not an image at all").expect_err("must not invent a cover");
        assert!(refused.contains("cover"), "must say what failed: {refused}");
    }

    /// The cache is keyed by content, so it does not need invalidating on
    /// that axis alone: a replaced archive has a different hash and
    /// therefore a different path.
    #[test]
    fn the_cache_path_is_keyed_by_the_member_hash() {
        let dir = Path::new("/tmp/covers");
        let one = cache_path(dir, "abc123");
        let two = cache_path(dir, "def456");

        assert_ne!(one, two);
        assert!(one.to_string_lossy().contains("abc123"));
        assert_eq!(one.extension().unwrap(), "jpg");
    }

    /// Important 2: content alone is not the whole cache key. `cache_path`'s
    /// real behaviour — proven here via `keyed_path` with the actual module
    /// constants substituted in as arguments, not a second hand-typed literal
    /// — must change when the generation parameters do, or a `LONG_EDGE` bump
    /// (a higher-DPI screen) leaves every cached file silently the wrong size
    /// until someone clears the cache directory by hand.
    #[test]
    fn changing_a_generation_parameter_changes_the_cache_path() {
        let dir = Path::new("/tmp/covers");
        let sha = "abc123";

        let current = cache_path(dir, sha);
        assert_eq!(
            current,
            keyed_path(dir, sha, LONG_EDGE, FORMAT_VERSION),
            "cache_path must be derived from the real constants, not a copy of them"
        );

        let bigger_edge = keyed_path(dir, sha, LONG_EDGE + 1, FORMAT_VERSION);
        let newer_format = keyed_path(dir, sha, LONG_EDGE, FORMAT_VERSION + 1);

        assert_ne!(
            current, bigger_edge,
            "a changed LONG_EDGE must change the path"
        );
        assert_ne!(
            current, newer_format,
            "a changed format version must change the path"
        );
        assert_ne!(
            bigger_edge, newer_format,
            "the two parameters must not collide with each other"
        );
    }

    /// Important 3: two attempts for the same key must never be handed the
    /// same temp filename to write through, or a concurrent pair racing on a
    /// cold cache can interleave and publish a corrupted cover forever. Also
    /// checks the leftover-file safety property: a temp name must never
    /// collide with what `cache_path` itself would produce, so an unrenamed
    /// temp file can never be mistaken for a real cover by the read path.
    ///
    /// Exercises `crate::atomic::temp_path` directly, now that this module's
    /// own `unique_temp_path` copy is gone — the property under test is
    /// unchanged, only which function proves it.
    #[test]
    fn temp_paths_for_the_same_key_never_collide_or_look_like_a_cover() {
        let path = cache_path(Path::new("/tmp/covers"), "abc123");

        let one = crate::atomic::temp_path(&path, "jpg");
        let two = crate::atomic::temp_path(&path, "jpg");

        assert_ne!(one, two, "two attempts must not share a temp name");
        assert_ne!(one, path, "must not just be the canonical path");
        assert_ne!(two, path, "must not just be the canonical path");
        // `cache_path` always ends exactly in `.jpg`; the read path in
        // `cached_or_generated` looks up nothing else. A leftover temp file
        // that ended the same way could be mistaken for a real cover.
        assert!(
            !one.to_string_lossy().ends_with(".jpg"),
            "a temp name must not end where the read path would find it: {one:?}"
        );
        assert!(
            !two.to_string_lossy().ends_with(".jpg"),
            "a temp name must not end where the read path would find it: {two:?}"
        );
    }

    /// Important 4: the cache-hit path is the one every OPDS grid view relies
    /// on to make a second request cheap. Bytes matching alone would not
    /// prove it — JPEG encoding is deterministic, so a second *regeneration*
    /// from the same page would produce identical bytes too — so the second
    /// call's generator panics if it is ever invoked at all.
    #[test]
    fn a_cache_hit_reads_back_the_same_bytes_without_regenerating() {
        let dir = std::env::temp_dir().join("taiyaku-test-cover-cache-hit");
        let _ = std::fs::remove_dir_all(&dir);

        let calls = std::sync::atomic::AtomicUsize::new(0);
        let page = real_page_bytes();

        let first = cached_or_generated(&dir, "deadbeef", || {
            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(page.clone())
        })
        .unwrap();
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "a miss must generate exactly once"
        );

        let second = cached_or_generated(&dir, "deadbeef", || {
            panic!("a cache hit must not regenerate")
        })
        .unwrap();

        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            first, second,
            "a cache hit must return exactly what was cached"
        );
        assert!(
            first.starts_with(b"\xff\xd8\xff"),
            "and it must be a real jpeg"
        );
    }

    /// Important 4: the module doc promises a read-only cache directory
    /// degrades the feed rather than breaking it. Pointed at a path that
    /// cannot be created — here, a directory nested under an existing plain
    /// file — because `create_dir_all` fails cleanly on that without needing
    /// permissions tricks that would not be portable across CI runners.
    #[test]
    fn a_cover_survives_a_cache_directory_that_cannot_be_created() {
        let blocker = std::env::temp_dir().join("taiyaku-test-cover-blocker-file");
        let _ = std::fs::remove_file(&blocker);
        let _ = std::fs::remove_dir_all(&blocker);
        std::fs::write(&blocker, b"not a directory").unwrap();
        let dir = blocker.join("covers");

        let page = real_page_bytes();
        let result = cached_or_generated(&dir, "deadbeef", || Ok(page.clone()));

        let _ = std::fs::remove_file(&blocker);
        let bytes = result.expect("an unwritable cache must not fail the request");
        assert!(
            bytes.starts_with(b"\xff\xd8\xff"),
            "the generated cover must still be real bytes, not empty ones"
        );
    }

    /// Important 6: two permits must allow two acquisitions to be held at
    /// once — proving the semaphore does not serialise generation down to
    /// one, which would defeat the point of choosing more than one permit.
    #[test]
    fn the_generation_semaphore_admits_its_configured_number_of_permits_at_once() {
        use std::sync::Barrier;
        use std::sync::atomic::AtomicUsize;

        let sem = Semaphore::new(2);
        let max_seen = AtomicUsize::new(0);
        let inside = AtomicUsize::new(0);
        // Sized to exactly the permit count: this can only complete if both
        // permits are held at the same instant.
        let both_held = Barrier::new(2);

        std::thread::scope(|scope| {
            for _ in 0..2 {
                scope.spawn(|| {
                    let _permit = sem.acquire();
                    let now = inside.fetch_add(1, Ordering::SeqCst) + 1;
                    max_seen.fetch_max(now, Ordering::SeqCst);
                    both_held.wait();
                    inside.fetch_sub(1, Ordering::SeqCst);
                });
            }
        });

        assert_eq!(
            max_seen.load(Ordering::SeqCst),
            2,
            "two permits must be usable at the same time"
        );
    }

    /// A directory unique to this test, so tests sharing a name for their
    /// cache key cannot see each other's files. No `tempfile` dependency:
    /// the workspace's dependency set is deliberately short.
    fn tempdir_for_test() -> std::path::PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("taiyaku-thumb-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A filmstrip tile is smaller than a cover, and the two never collide in
    /// the cache even for page 0 of the same member — which is the one page
    /// that is both.
    #[test]
    fn a_thumb_is_smaller_than_a_cover_and_keyed_apart() {
        let dir = std::path::Path::new("/tmp/does-not-matter");
        assert_ne!(cache_path(dir, "abc"), thumb_cache_path(dir, "abc", 0));

        let page = real_page_bytes();
        let cover = thumbnail(&page).unwrap();
        let tile = thumbnail_at(&page, THUMB_LONG_EDGE).unwrap();
        assert!(
            tile.len() < cover.len(),
            "tile {} should be smaller than cover {}",
            tile.len(),
            cover.len()
        );
    }

    /// Two pages of one member are two tiles. Keying on the member alone —
    /// which is what a cover does, because a member has one cover — would serve
    /// page 0's tile for every page in the volume.
    #[test]
    fn each_page_of_a_member_gets_its_own_tile() {
        let dir = std::path::Path::new("/tmp/does-not-matter");
        assert_ne!(
            thumb_cache_path(dir, "abc", 0),
            thumb_cache_path(dir, "abc", 1)
        );
    }

    /// A second request for the same tile reads the cache rather than decoding
    /// again. Pinned by counting generations, not by timing.
    #[test]
    fn a_cached_tile_is_not_regenerated() {
        let dir = tempdir_for_test();
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let page = || {
            calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(real_page_bytes())
        };
        let first = cached_or_generated_thumb(&dir, "abc", 7, page).unwrap();
        let second = cached_or_generated_thumb(&dir, "abc", 7, || {
            calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(real_page_bytes())
        })
        .unwrap();
        assert_eq!(first, second);
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    /// Acquiring takes a permit, and the count says so.
    ///
    /// The direct, single-threaded half of the property the test below tries
    /// to observe through two threads. It exists because that one **is** timing
    /// dependent, despite having said otherwise for as long as it existed:
    /// mutation testing flipped `*available -= 1` to `*available /= 1` — a
    /// permit count that never decreases — and caught it on two runs and
    /// missed it on a third. Nothing about the code changed between them.
    ///
    /// With the count asserted here, that mutant dies the same way every run.
    #[test]
    fn acquiring_a_permit_decrements_the_available_count() {
        let sem = Semaphore::new(2);
        assert_eq!(*sem.available.lock().unwrap(), 2, "both permits free");

        let first = sem.acquire();
        assert_eq!(*sem.available.lock().unwrap(), 1, "one taken");

        let second = sem.acquire();
        assert_eq!(*sem.available.lock().unwrap(), 0, "both taken");

        drop(second);
        assert_eq!(*sem.available.lock().unwrap(), 1, "and released on drop");
        drop(first);
        assert_eq!(*sem.available.lock().unwrap(), 2);
    }

    /// Important 6: a third acquisition against a one-permit semaphore must
    /// not succeed until the held permit is released — the property that
    /// actually bounds concurrent decodes rather than merely allowing some.
    ///
    /// **This test is timing dependent, and the comment here used to claim it
    /// was not.** The claim was that the second thread's `acquire()` can only
    /// return after `Drop` runs on the first thread's guard, which is strictly
    /// after `released.store(true, ..)` — true while `acquire` blocks, and the
    /// whole point is that a broken `acquire` does not. Once it returns early
    /// there is no ordering at all between that store and this load, so the
    /// mutant that stops permits decrementing is caught or missed by
    /// scheduling. It was observed doing both.
    ///
    /// Kept, because the blocking property is worth stating and this is the
    /// only test that states it. What makes the mutant die reliably is
    /// `acquiring_a_permit_decrements_the_available_count` above.
    #[test]
    fn a_second_acquisition_waits_until_the_held_permit_is_released() {
        use std::sync::Barrier;
        use std::sync::atomic::AtomicBool;

        let sem = Semaphore::new(1);
        let released = AtomicBool::new(false);
        let observed_after_release = AtomicBool::new(false);
        let holder_ready = Barrier::new(2);

        std::thread::scope(|scope| {
            scope.spawn(|| {
                let _permit = sem.acquire();
                holder_ready.wait();
                // The other thread is now blocked in `acquire()`. Releasing
                // happens when `_permit` drops, at the end of this closure —
                // strictly after this store, in program order.
                released.store(true, Ordering::SeqCst);
            });

            scope.spawn(|| {
                holder_ready.wait();
                let _permit = sem.acquire();
                observed_after_release.store(released.load(Ordering::SeqCst), Ordering::SeqCst);
            });
        });

        assert!(
            observed_after_release.load(Ordering::SeqCst),
            "the second acquire must not succeed before the first permit was released"
        );
    }

    /// Two different outputs must never share one path. `page_cache_path` and
    /// `thumb_cache_path` can be asked for the same sha, offset and long edge —
    /// a tile cropped to a strip, and the whole page — and a content-addressed
    /// cache that returned one for the other would show a reader a crop where a
    /// page belongs.
    ///
    /// The `r` in the name is what keeps them apart, and nothing asserted it:
    /// `page_cache_path` could return `Default::default()` — the empty path,
    /// shared by every page of every work — with the suite still green.
    #[test]
    fn a_reading_page_and_a_tile_never_share_a_cache_path() {
        let dir = std::path::Path::new("/cache");
        // At the tile's own long edge, so the two differ by the `r` in the
        // name rather than by the size — which is the case the doc calls out.
        let page = page_cache_path(dir, "abc", 3, THUMB_LONG_EDGE);
        let tile = thumb_cache_path(dir, "abc", 3);

        assert_ne!(page, tile, "same inputs, two different outputs");
        assert!(page.starts_with(dir), "under the cache directory");
        assert!(
            page.to_string_lossy().contains("abc"),
            "content-addressed: {page:?}"
        );
        assert_ne!(
            page,
            page_cache_path(dir, "abc", 4, 1600),
            "a different offset is a different page"
        );
        assert_ne!(
            page,
            page_cache_path(dir, "abc", 3, 800),
            "and a different size is a different rendering"
        );
    }
}
