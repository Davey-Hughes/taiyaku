//! One cover per edition, for the OPDS entries — and, since M5a, one
//! filmstrip tile per page.
//!
//! A cover and a tile share one decoder (`thumbnail_at`) and one cache-publish
//! path (`at_path`); only the cache key and the output size differ.
//!
//! Cached by the member's `sha256` **and** by the parameters the image was
//! generated with — the long edge (`LONG_EDGE` for a cover, `THUMB_LONG_EDGE`
//! for a tile that did not ask for another — `TILE_LONG_EDGES` are the ones
//! it may ask for) and a format version, both folded into the cache path. A tile
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

use taiyaku_read::MAX_DECODE_ALLOC;

/// The long edge of a generated cover, in pixels.
///
/// Large enough for a grid on a tablet, small enough that generating one is
/// not the slow part of opening a feed.
const LONG_EDGE: u32 = 512;

/// The long edge of a filmstrip tile, in pixels — the size every tile was
/// served at until O134, and still the one a request that does not ask for
/// another gets.
///
/// A cover is 512 because it fills a grid cell; a tile sits in a strip along
/// the bottom edge and there are many of them per volume, so it is sized for
/// the strip rather than for a grid. Folded into the cache path exactly as
/// `LONG_EDGE` is, so changing it invalidates by construction.
pub const THUMB_LONG_EDGE: u32 = 160;

/// The long edges a filmstrip tile is offered at, one per screen density —
/// O134.
///
/// A tile is drawn 160 px tall in the strip, and 96 below the 900 px
/// breakpoint (`web/src/styles/reader.css`'s `--strip-tile`), so one, two and
/// three device pixels per CSS pixel want at most 480 — and a phone, whose
/// tile is the shorter one, wants 320 rather than the 480 an `srcSet`'s `3x`
/// descriptor would have fetched.
///
/// **The web holds the same three** (`web/src/tileSize.ts`'s `TILE_LADDER`),
/// the way `COVER_LONG_EDGES` is held on both sides. A rung added there alone
/// is a `w` this server does not recognise, answered with the 160 px tile —
/// soft rather than broken, and so invisible to any test that does not decode
/// what came back.
pub const TILE_LONG_EDGES: [u32; 3] = [THUMB_LONG_EDGE, 320, 480];

/// `TILE_LONG_EDGES` and `COVER_LONG_EDGES` must never share a value.
///
/// `routes.rs::get_thumb` tries `cover_edge` before `tile_edge`, and
/// `cover_edge` matches on page 0 alone but on *any* size in
/// `COVER_LONG_EDGES` — so a `w` that landed in both arrays would take the
/// cover branch on page 0 and never reach `tile_edge` at all. Page 0 at exactly
/// the size someone added expecting a sharper tile would be served with an
/// `ETag`, as the archive's own bytes and content type whenever page 0 already
/// fits that size, silently breaking "a tile is always JPEG" for that one page
/// while the rest of the filmstrip kept working, and every other page's `w`
/// still found `tile_edge` unaffected. Today the two are disjoint by
/// construction, not by anything that stops them from drifting apart; this
/// makes drifting apart a build failure instead of a bug report.
const _: () = {
    let mut i = 0;
    while i < TILE_LONG_EDGES.len() {
        let mut j = 0;
        while j < COVER_LONG_EDGES.len() {
            assert!(
                TILE_LONG_EDGES[i] != COVER_LONG_EDGES[j],
                "TILE_LONG_EDGES and COVER_LONG_EDGES share a value: get_thumb's cover_edge \
                 branch runs first and would take it on page 0, silently serving a cover instead \
                 of a tile"
            );
            j += 1;
        }
        i += 1;
    }
};

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

/// Where a filmstrip tile for one page of one member is cached, at
/// `long_edge`.
///
/// Keyed on the member's `sha256` *and the page's offset within it*, because
/// a member has one cover and many tiles. The offset rather than the
/// edition-flat index, so a tile survives the edition gaining a member ahead
/// of this one. The long edge is an argument rather than the constant: one
/// page has a file per size a screen has asked for (O134).
pub fn thumb_cache_path(dir: &Path, sha256: &str, offset: u32, long_edge: u32) -> PathBuf {
    thumb_keyed_path(dir, sha256, offset, long_edge, FORMAT_VERSION)
}

/// `thumb_cache_path`, with the generation parameters as arguments, for
/// `keyed_path`'s reason: a test can vary each one alone without a second copy
/// of `THUMB_LONG_EDGE` or `FORMAT_VERSION`.
fn thumb_keyed_path(
    dir: &Path,
    sha256: &str,
    offset: u32,
    long_edge: u32,
    format_version: u32,
) -> PathBuf {
    dir.join(format!(
        "{sha256}-p{offset}-{long_edge}v{format_version}.jpg"
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
    dir.join(format!("{}.jpg", page_key(sha256, offset, long_edge)))
}

/// The name of one reading-sized rendering of a page: the member, the page's
/// offset within it, the long edge and the format version.
///
/// **It is both the cache file's name and the page's `ETag`** (see
/// `routes::page_etag`), and O106 is why there is one function for both.
/// A tag that lacked a parameter the file name has would stay unchanged when
/// that parameter's bump regenerated every cached page, so a browser holding
/// an old rendering would be answered `304` once its copy went stale, and keep
/// it. A parameter added here reaches both. One that is not the long edge
/// belongs in `cover_version` too, or a cover's versioned address goes on
/// being kept for good over bytes it no longer names.
pub(crate) fn page_key(sha256: &str, offset: u32, long_edge: u32) -> String {
    page_key_at(sha256, offset, long_edge, PAGE_FORMAT_VERSION)
}

/// `page_key`, with the format version as an argument, for `keyed_path`'s
/// reason: a test can vary the version without a second copy of the constant.
fn page_key_at(sha256: &str, offset: u32, long_edge: u32, format_version: u32) -> String {
    format!("{sha256}-p{offset}-r{long_edge}v{format_version}")
}

/// What a cover's `ETag` is made of but its long edge: so it changes exactly
/// when the `ETag` of any cover size would.
///
/// Without the edge because one address per edition carries it and the client
/// adds the size, one per screen density. Its alphabet is free:
/// `cover_address` encodes it into the query string.
pub(crate) fn cover_version(sha256: &str, offset: u32) -> String {
    cover_version_at(sha256, offset, PAGE_FORMAT_VERSION)
}

/// `cover_version`, with the format version as an argument, for `page_key_at`'s
/// reason.
fn cover_version_at(sha256: &str, offset: u32, format_version: u32) -> String {
    format!("{sha256}-{offset}-{format_version}")
}

/// `FORMAT_VERSION`'s counterpart for reading-sized pages, versioned separately
/// so a change to how a *page* is produced does not throw away every cover and
/// tile in a large library, or the reverse.
const PAGE_FORMAT_VERSION: u32 = 2;

/// The largest long edge this server will resize to.
///
/// A ceiling on what a client can ask for, so a hand-edited or hostile `?w=`
/// cannot fill the cache directory with one entry per pixel value. `web/`'s own
/// ladder tops out well below this; the bound exists for requests that did not
/// come from it.
pub const MAX_FIT_LONG_EDGE: u32 = 4096;

/// The long edges a library cover is served at — O133: one per screen density,
/// for a tile drawn about 256 CSS px on its long edge at one, two and three
/// device pixels per CSS pixel. The web's `library.ts` offers the same three
/// (`COVER_LONG_EDGES` there), and the tile route's cover branch sizes page 0
/// to these and to nothing else, so a cover is never a way to read a page an
/// account may not stream.
pub const COVER_LONG_EDGES: [u32; 3] = [256, 512, 768];

/// Whether a page already fits inside `long_edge`, from its header alone.
///
/// **`true` means "serve the original bytes", and it is the answer that makes
/// resizing safe to apply to every edition.** A page whose long edge is already
/// within the request is returned untouched rather than re-encoded: re-encoding
/// it would cost quality for no pixels gained, and on this corpus `raw-mono`
/// and `en-fan` are frequently already smaller than what a reader's viewport
/// asks for.
///
/// The check reads the image header only — `into_dimensions` parses the SOF
/// marker and stops, where `decode` would rasterise the whole frame — so
/// deciding costs a small fraction of a full decode. That is what lets the
/// decision be made per request without a cache entry to remember it in.
///
/// **Apart from [`resize_to_long_edge`], and O111 is why.** The permit is taken
/// between them: after this check, and only for a page that needs the decode.
fn fits_within(page: &[u8], long_edge: u32) -> Result<bool, String> {
    // Two readers, and the second is not waste. `into_dimensions` consumes the
    // one it is called on, and it is what makes the cheap path cheap: it stops
    // at the frame header, so a page that already fits costs no decode at all.
    // Only a page that genuinely needs resizing pays for a second header
    // parse, in `resize_to_long_edge`, which is nothing beside the decode that
    // follows it.
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    let mut reader = image::ImageReader::new(std::io::Cursor::new(page))
        .with_guessed_format()
        .map_err(|e| format!("fit: undecodable page: {e}"))?;
    reader.limits(limits);
    let (width, height) = reader
        .into_dimensions()
        .map_err(|e| format!("fit: unreadable dimensions: {e}"))?;
    Ok(width.max(height) <= long_edge)
}

/// A page decoded, resized to fit inside `long_edge` and encoded as JPEG.
///
/// **Only for a page [`fits_within`] has said does not fit.** `resize` scales
/// up as readily as down, so on a page already inside `long_edge` this would
/// enlarge it.
///
/// Always JPEG, for `thumbnail_at`'s reason: one declared content type rather
/// than whatever the archive happened to hold.
fn resize_to_long_edge(page: &[u8], long_edge: u32) -> Result<Vec<u8>, String> {
    // **Not `thumbnail`'s sampler, and the filter is the reason.** `image`
    // documents it as "a fast integer algorithm where each source pixel
    // contributes to exactly one target pixel… may give aliasing artifacts" —
    // the right trade for a 512px cover and the wrong one for the page a reader
    // is looking at, since manga is screentone and thin ink, the two things
    // that alias worst. Reducing a page to 768px through the fast path visibly
    // breaks up the hatching.
    //
    // `CatmullRom` rather than `Lanczos3`: both cost the same here, and Lanczos
    // rings on high-contrast edges — every panel border and every letter on the
    // page. A little softness is a better failure than haloed ink.
    let decoded = decode_limited(page, "fit")?;
    let scaled = decoded.resize(
        long_edge,
        long_edge,
        image::imageops::FilterType::CatmullRom,
    );
    encode_jpeg(scaled, "fit")
}

/// A reading-sized page, or the page itself when it already fits.
///
/// `P` is whatever the caller's fetch returned, and **`Original` hands it back
/// untouched. O107 is why.** Handing back the fetched page lets the caller
/// serve that one read, with whatever came with its bytes. The route's content
/// type is what needs this.
#[derive(Debug, PartialEq, Eq)]
pub enum Fitted<P> {
    /// A JPEG at the requested long edge, from the cache or just made.
    Resized(Vec<u8>),
    /// The page as `page` returned it, because it is already within the
    /// requested long edge.
    Original(P),
}

/// Read a cached reading-sized page, or make one and cache it.
///
/// A page that `fits_within` says already fits comes back as `Fitted::Original`
/// and **nothing is cached**: the correct output for that request is the
/// member's own bytes, which no cache could hold more cheaply than the archive
/// does. `page` runs at most once, so a page that fits is read once.
///
/// **The permit is taken for the resize alone, and O111 is why.** A cache hit,
/// the fetch, and the header parse that decides whether the page fits all run
/// without it. Were they in it, a request for a page that fits, which decodes
/// nothing, would queue behind a cold filmstrip (`docs/notes.md`, under O107
/// and O111). The permit bounds decode memory, and only the resize decodes.
///
/// **The cost is a fetched page held while it queues.** A page that needs a
/// resize is fetched before it waits for the permit, so its bytes are in memory
/// for the wait. A full-size request already holds a fetched page with no
/// permit at all, so this is not a new shape here. `at_path` still fetches
/// inside the permit. An OPDS cover or a filmstrip tile always decodes, so
/// fetching first would not spare it the wait, and would hold its page in
/// memory through it. **A library cover comes here (O133)**, sized by the
/// tile route's `?w=` (`routes.rs::cover_edge`), so a cold grid holds a
/// fetched page per queued cover while the permits resize; that cost was
/// accepted for the page route's output — the filter chosen for line art, and
/// one cache file shared with the page route at the same size.
///
/// Shares `GENERATION_LIMIT` with covers and tiles rather than taking a bound
/// of its own. All three do the same expensive thing — decode a page at up to
/// `MAX_DECODE_ALLOC` — so one budget over all of them is what actually bounds
/// peak memory; separate budgets would each be safe alone and add up to
/// something that is not.
pub fn cached_or_generated_page<P: AsRef<[u8]>>(
    dir: &Path,
    sha256: &str,
    offset: u32,
    long_edge: u32,
    page: impl FnOnce() -> Result<P, String>,
) -> Result<Fitted<P>, String> {
    let path = page_cache_path(dir, sha256, offset, long_edge);
    if let Some(found) = cached(&path) {
        return Ok(Fitted::Resized(found));
    }
    let fetched = page()?;
    if fits_within(fetched.as_ref(), long_edge)? {
        return Ok(Fitted::Original(fetched));
    }
    let bytes = {
        let _permit = GENERATION_LIMIT.acquire();
        resize_to_long_edge(fetched.as_ref(), long_edge)?
    };
    if std::fs::create_dir_all(dir).is_ok() {
        // Discarded for `at_path`'s reason: the page was generated and the
        // reader gets it, so a cache that cannot be written is slow rather
        // than broken.
        let _ = taiyaku_read::atomic::write(&path, &bytes, "jpg");
    }
    Ok(Fitted::Resized(bytes))
}

/// The cached file at `path`, if there is one and it is a whole JPEG.
///
/// **O105: a hit is a whole JPEG, not any file `fs::read` can open.** An empty
/// file served as a cover would never be replaced, because the path is
/// content-addressed and never changes. `None` for a broken file makes the
/// caller treat it as a miss. The caller then regenerates and
/// `atomic::write` renames the new file over the broken one, so the next
/// request is a real hit.
///
/// Every file in these caches came out of `encode_jpeg`, which always starts
/// with SOI and ends with EOI, so the check rejects only the shapes a copy cut
/// off leaves: an empty file, a zero-filled one, and one cut short.
///
/// A marker check rather than a decode, because the hit is the cheap path.
/// It runs outside `GENERATION_LIMIT` so that a warm grid stays fully
/// parallel, and a decode here would cost a warm request about as much as a
/// cold one. The bytes are already in memory, so checking five of them adds
/// nothing. **Damage between the markers is not caught**: that takes a decode.
///
/// **A hit records its use (O11)** in the mtime the cache sweep evicts by: the
/// atime moves or not by how the cache is mounted. The touch follows the
/// decided hit and drops its result, so it can never turn a hit into a miss.
fn cached(path: &Path) -> Option<Vec<u8>> {
    let found = std::fs::read(path).ok()?;
    if !(found.starts_with(b"\xff\xd8\xff") && found.ends_with(b"\xff\xd9")) {
        return None;
    }
    let _ = record_use(path);
    Some(found)
}

/// How stale a rendering's recorded use may grow before a hit sets it to now.
///
/// A day, so a warm read becomes a write at most once a day per file; the
/// sweep then sees a use up to a day late.
const USE_REFRESH_AFTER: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// Sets `path`'s mtime to now when it is more than [`USE_REFRESH_AFTER`] old.
/// An mtime ahead of now, as a clock set back leaves, is young.
fn record_use(path: &Path) -> std::io::Result<()> {
    let now = std::time::SystemTime::now();
    let modified = std::fs::metadata(path)?.modified()?;
    if now
        .duration_since(modified)
        .is_ok_and(|age| age > USE_REFRESH_AFTER)
    {
        std::fs::File::options()
            .write(true)
            .open(path)?
            .set_modified(now)?;
    }
    Ok(())
}

/// How many generations may decode at once: a cover or a tile, fetching its
/// page and decoding it, or a reading page resizing one it has already fetched.
///
/// **A reading page fetches outside the permit, and covers and tiles fetch
/// inside it.** O111: a page that fits never queues; see
/// `cached_or_generated_page`.
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

/// The permits [`MAX_CONCURRENT_GENERATIONS`] describes.
///
/// Guards only the *generation* step — fetching and decoding a cover or tile in
/// `at_path`, resizing a reading page in `cached_or_generated_page` — never the
/// cache-hit read, which has to stay fully parallel or a warm OPDS grid would
/// queue behind a decode limit it does not need.
static GENERATION_LIMIT: crate::limit::Semaphore =
    crate::limit::Semaphore::new(MAX_CONCURRENT_GENERATIONS);

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
    // a reader actually looks at goes through `resize_to_long_edge` instead,
    // which says why.
    encode_jpeg(decoded.thumbnail(long_edge, long_edge), "cover")
}

/// Read a cached cover, or make one and cache it.
///
/// A cache that cannot be written is not an error — the cover was generated
/// and the client should get it. Failing here would make a read-only cache
/// directory break the feed rather than merely slow it.
///
/// Important 6: generation — fetching the page and decoding it — runs behind
/// `GENERATION_LIMIT`, bounding how many decodes at up to `MAX_DECODE_ALLOC`
/// each can run at once. The cache-hit read in `at_path` is deliberately
/// outside the permit, so a warm grid stays fully parallel; only a cold one,
/// which is what actually decodes anything, is bounded.
pub fn cached_or_generated(
    dir: &Path,
    sha256: &str,
    page: impl FnOnce() -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    at_path(&cache_path(dir, sha256), dir, page, |bytes| {
        thumbnail(&bytes)
    })
}

/// Read a cached filmstrip tile at `long_edge`, or make one and cache it.
///
/// **One `long_edge` feeds both the path and the generator**, deliberately as
/// a single argument: a file named for one size holding the pixels of another
/// is a cache poisoning no later request can detect, since the bytes decode
/// perfectly at the wrong size.
pub fn cached_or_generated_thumb(
    dir: &Path,
    sha256: &str,
    offset: u32,
    long_edge: u32,
    page: impl FnOnce() -> Result<Vec<u8>, String>,
) -> Result<Vec<u8>, String> {
    at_path(
        &thumb_cache_path(dir, sha256, offset, long_edge),
        dir,
        page,
        |bytes| thumbnail_at(&bytes, long_edge),
    )
}

/// The shared body: cache hit outside the permit, generation inside it,
/// atomic publish via a per-attempt temp name.
///
/// Takes the decode-and-resize step as a closure — `thumbnail` for a cover,
/// `thumbnail_at(_, long_edge)` for a tile — rather than a bare
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
    if let Some(found) = cached(path) {
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
