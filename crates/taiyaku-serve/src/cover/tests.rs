use super::*;
use std::sync::atomic::Ordering;

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

/// Content alone is not the whole cache key. `cache_path`'s real behaviour —
/// proven here via `keyed_path` with the actual module constants substituted in
/// as arguments, not a second hand-typed literal — must change when the
/// generation parameters do, or a `LONG_EDGE` bump (a higher-DPI screen) leaves
/// every cached file silently the wrong size until someone clears the cache
/// directory by hand.
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

/// Two attempts for the same key must never be handed the same temp filename
/// to write through, or a concurrent pair racing on a cold cache can interleave
/// and publish a corrupted cover forever. Also checks the leftover-file safety
/// property: a temp name must never collide with what `cache_path` itself would
/// produce, so an unrenamed temp file can never be mistaken for a real cover by
/// the read path.
///
/// Exercises `taiyaku_read::atomic::temp_path` directly.
#[test]
fn temp_paths_for_the_same_key_never_collide_or_look_like_a_cover() {
    let path = cache_path(Path::new("/tmp/covers"), "abc123");

    let one = taiyaku_read::atomic::temp_path(&path, "jpg");
    let two = taiyaku_read::atomic::temp_path(&path, "jpg");

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

/// The cache-hit path is the one every OPDS grid view relies on to make a
/// second request cheap. Bytes matching alone would not prove it — JPEG
/// encoding is deterministic, so a second *regeneration* from the same page
/// would produce identical bytes too — so the second call's generator panics
/// if it is ever invoked at all.
#[test]
fn a_cache_hit_reads_back_the_same_bytes_without_regenerating() {
    let scratch = crate::test_support::Fixture::new("cover-cache-hit");
    let dir = scratch.join("covers");

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

    assert_eq!(
        first, second,
        "a cache hit must return exactly what was cached"
    );
    assert!(
        first.starts_with(b"\xff\xd8\xff"),
        "and it must be a real jpeg"
    );
}

/// A whole JPEG at a cover's cache path in `dir`, last modified at `used`.
fn cached_cover_used(dir: &Path, used: std::time::SystemTime) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = cache_path(dir, "deadbeef");
    std::fs::write(&path, thumbnail(&real_page_bytes()).unwrap()).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .and_then(|file| file.set_modified(used))
        .unwrap();
    path
}

fn modified(path: &Path) -> std::time::SystemTime {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap()
}

/// A read through the cache whose `page` panics, so it can only be a hit.
fn hit(dir: &Path) {
    cached_or_generated(dir, "deadbeef", || {
        panic!("a cache hit must not regenerate")
    })
    .expect("a hit answers");
}

#[test]
fn a_hit_on_a_rendering_last_used_over_a_day_ago_marks_it_used_now() {
    let scratch = crate::test_support::Fixture::new("cover-touch-stale");
    let dir = scratch.join("covers");
    let a_day_and_a_minute = std::time::Duration::from_secs(24 * 60 * 60 + 60);
    let path = cached_cover_used(&dir, std::time::SystemTime::now() - a_day_and_a_minute);

    let before = std::time::SystemTime::now();
    hit(&dir);
    let after = std::time::SystemTime::now();

    let used = modified(&path);
    assert!(
        before <= used && used <= after,
        "the mtime {used:?} is not between {before:?} and {after:?}"
    );
}

/// A minute under a day ago is within a day, and an mtime ahead of now, as a
/// clock set back leaves, counts as young.
#[test]
fn a_hit_on_a_rendering_used_within_a_day_or_dated_ahead_keeps_its_mtime() {
    let (minute, day) = (
        std::time::Duration::from_secs(60),
        std::time::Duration::from_secs(24 * 60 * 60),
    );
    let now = std::time::SystemTime::now();
    for (label, used) in [
        ("a minute under a day ago", now - day + minute),
        ("a day ahead", now + day),
    ] {
        let scratch = crate::test_support::Fixture::new("cover-touch-young");
        let dir = scratch.join("covers");
        let path = cached_cover_used(&dir, used);
        let had = modified(&path);

        hit(&dir);

        assert_eq!(modified(&path), had, "a cover last used {label}");
    }
}

/// `cached_or_generated`'s doc promises a read-only cache directory degrades
/// the feed rather than breaking it. Pointed at a path that cannot be created —
/// here, a directory nested under an existing plain file — because
/// `create_dir_all` fails cleanly on that without needing permissions tricks
/// that would not be portable across CI runners.
#[test]
fn a_cover_survives_a_cache_directory_that_cannot_be_created() {
    let scratch = crate::test_support::Fixture::new("cover-blocker");
    let blocker = scratch.join("blocker-file");
    std::fs::write(&blocker, b"not a directory").unwrap();
    let dir = blocker.join("covers");

    let page = real_page_bytes();
    let result = cached_or_generated(&dir, "deadbeef", || Ok(page.clone()));

    let bytes = result.expect("an unwritable cache must not fail the request");
    assert!(
        bytes.starts_with(b"\xff\xd8\xff"),
        "the generated cover must still be real bytes, not empty ones"
    );
}

/// A directory unique to this test, so tests sharing a name for their cache key
/// cannot see each other's files — and one that removes itself when the test
/// ends. `test_support::Fixture` carries the argument for both halves.
fn tempdir_for_test() -> crate::test_support::Fixture {
    crate::test_support::Fixture::new("thumb")
}

/// A filmstrip tile is smaller than a cover, and the two never collide in
/// the cache even for page 0 of the same member — which is the one page
/// that is both.
#[test]
fn a_thumb_is_smaller_than_a_cover_and_keyed_apart() {
    let dir = std::path::Path::new("/tmp/does-not-matter");
    assert_ne!(
        cache_path(dir, "abc"),
        thumb_cache_path(dir, "abc", 0, THUMB_LONG_EDGE)
    );

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
        thumb_cache_path(dir, "abc", 0, THUMB_LONG_EDGE),
        thumb_cache_path(dir, "abc", 1, THUMB_LONG_EDGE)
    );
}

/// A tile's cache path changes with each parameter it was generated under, one
/// at a time (O106). Without the hash, page N of every member would share one
/// tile file. Without `THUMB_LONG_EDGE` or `FORMAT_VERSION`, a change to how
/// tiles are made would leave every old tile served as the new one.
///
/// The name is pinned for the page key's reason: every tile already cached is
/// named this way, and renaming them regenerates them all. It is built from
/// the constants, so changing one still passes.
#[test]
fn changing_any_tile_parameter_changes_the_tile_path() {
    let dir = Path::new("/cache");
    let current = thumb_keyed_path(dir, "abc", 7, THUMB_LONG_EDGE, FORMAT_VERSION);

    assert_eq!(
        current,
        thumb_cache_path(dir, "abc", 7, THUMB_LONG_EDGE),
        "thumb_cache_path must use the real constants, not copies of them"
    );
    assert_eq!(
        current,
        dir.join(format!("abc-p7-{THUMB_LONG_EDGE}v{FORMAT_VERSION}.jpg")),
        "the name every cached tile already has"
    );
    for (changed, other) in [
        (
            "sha256",
            thumb_keyed_path(dir, "abd", 7, THUMB_LONG_EDGE, FORMAT_VERSION),
        ),
        (
            "offset",
            thumb_keyed_path(dir, "abc", 8, THUMB_LONG_EDGE, FORMAT_VERSION),
        ),
        (
            "long edge",
            thumb_keyed_path(dir, "abc", 7, THUMB_LONG_EDGE + 1, FORMAT_VERSION),
        ),
        (
            "format version",
            thumb_keyed_path(dir, "abc", 7, THUMB_LONG_EDGE, FORMAT_VERSION + 1),
        ),
    ] {
        assert_ne!(
            current, other,
            "a changed {changed} must change the tile path"
        );
    }
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
    let first = cached_or_generated_thumb(dir.path(), "abc", 7, THUMB_LONG_EDGE, page).unwrap();
    let second = cached_or_generated_thumb(dir.path(), "abc", 7, THUMB_LONG_EDGE, || {
        calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(real_page_bytes())
    })
    .unwrap();
    assert_eq!(first, second);
    assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
}

/// O105: files at the right cache path that are not a whole JPEG, each named
/// for the test messages.
///
/// An empty file and a zero-filled one both fail both halves of the check a
/// hit must pass, so a check that tested only one half would still reject
/// them. The other four are each built to fail exactly one half:
///
/// - `truncated` keeps its start marker and loses its end marker.
/// - `cut on D9` is cut short on a `D9` byte that is not preceded by `FF`, so a
///   check reading only the last byte of the end marker accepts it.
/// - `headless` keeps its end marker and loses its start marker.
/// - `third byte` has `FF D8` but not the `FF` after it, so a check reading
///   only two bytes of the start marker accepts it.
///
/// The assertions below check those four constructions rather than trusting
/// them. A truncation that happened to end on `FF D9` would test nothing.
fn broken_cache_files() -> [(&'static str, Vec<u8>); 6] {
    let whole = thumbnail(&real_page_bytes()).unwrap();
    let truncated = whole[..whole.len() / 2].to_vec();
    let mut cut_on_d9 = truncated.clone();
    cut_on_d9.push(0xd9);
    let mut headless = whole.clone();
    headless[0] = 0;
    let mut third_byte = whole.clone();
    third_byte[2] = 0;

    assert!(truncated.starts_with(b"\xff\xd8\xff") && !truncated.ends_with(b"\xff\xd9"));
    assert!(cut_on_d9.starts_with(b"\xff\xd8\xff") && !cut_on_d9.ends_with(b"\xff\xd9"));
    assert!(cut_on_d9.ends_with(b"\xd9"));
    assert!(!headless.starts_with(b"\xff\xd8\xff") && headless.ends_with(b"\xff\xd9"));
    assert!(!third_byte.starts_with(b"\xff\xd8\xff") && third_byte.ends_with(b"\xff\xd9"));
    assert!(third_byte.starts_with(b"\xff\xd8"));

    [
        ("empty", Vec::new()),
        ("zero-filled", vec![0; whole.len()]),
        ("truncated", truncated),
        ("cut on D9", cut_on_d9),
        ("headless", headless),
        ("third byte", third_byte),
    ]
}

/// O105: a cached cover that is not a whole JPEG is regenerated and replaced.
///
/// Four assertions per case, and the last is the one a lazy fix fails: the
/// replacement must itself be a hit. Otherwise "always regenerate" would pass
/// the first three.
#[test]
fn a_broken_cached_cover_is_regenerated_and_replaced() {
    let expected = thumbnail(&real_page_bytes()).unwrap();
    for (kind, broken) in broken_cache_files() {
        let dir = tempdir_for_test();
        let path = cache_path(dir.path(), "abc");
        std::fs::write(&path, &broken).unwrap();
        let calls = std::sync::atomic::AtomicUsize::new(0);

        let served = cached_or_generated(dir.path(), "abc", || {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(real_page_bytes())
        })
        .unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 1, "{kind}: must regenerate");
        assert_eq!(served, expected, "{kind}: must serve the regenerated cover");
        assert_eq!(
            std::fs::read(&path).unwrap(),
            expected,
            "{kind}: must replace the broken file"
        );
        let again = cached_or_generated(dir.path(), "abc", || {
            panic!("{kind}: the replacement must be a hit")
        })
        .unwrap();
        assert_eq!(again, expected, "{kind}: must serve the replacement");
    }
}

/// O105, for a filmstrip tile.
#[test]
fn a_broken_cached_tile_is_regenerated_and_replaced() {
    let expected = thumbnail_at(&real_page_bytes(), THUMB_LONG_EDGE).unwrap();
    for (kind, broken) in broken_cache_files() {
        let dir = tempdir_for_test();
        let path = thumb_cache_path(dir.path(), "abc", 7, THUMB_LONG_EDGE);
        std::fs::write(&path, &broken).unwrap();
        let calls = std::sync::atomic::AtomicUsize::new(0);

        let served = cached_or_generated_thumb(dir.path(), "abc", 7, THUMB_LONG_EDGE, || {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(real_page_bytes())
        })
        .unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 1, "{kind}: must regenerate");
        assert_eq!(served, expected, "{kind}: must serve the regenerated tile");
        assert_eq!(
            std::fs::read(&path).unwrap(),
            expected,
            "{kind}: must replace the broken file"
        );
        let again = cached_or_generated_thumb(dir.path(), "abc", 7, THUMB_LONG_EDGE, || {
            panic!("{kind}: the replacement must be a hit")
        })
        .unwrap();
        assert_eq!(again, expected, "{kind}: must serve the replacement");
    }
}

/// O105, for a reading-sized page. This one does not go through `at_path`, so
/// it needs its own witness. The long edge is 8 against a 16x16 page, so the
/// page is resized and a file is cached.
#[test]
fn a_broken_cached_reading_page_is_regenerated_and_replaced() {
    let expected = resize_to_long_edge(&real_page_bytes(), 8).unwrap();
    for (kind, broken) in broken_cache_files() {
        let dir = tempdir_for_test();
        let path = page_cache_path(dir.path(), "abc", 3, 8);
        std::fs::write(&path, &broken).unwrap();
        let calls = std::sync::atomic::AtomicUsize::new(0);

        let served = cached_or_generated_page(dir.path(), "abc", 3, 8, || {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(real_page_bytes())
        })
        .unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 1, "{kind}: must regenerate");
        assert_eq!(
            served,
            Fitted::Resized(expected.clone()),
            "{kind}: must serve the regenerated page"
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            expected,
            "{kind}: must replace the broken file"
        );
        let again = cached_or_generated_page(dir.path(), "abc", 3, 8, || -> Result<Vec<u8>, _> {
            panic!("{kind}: the replacement must be a hit")
        })
        .unwrap();
        assert_eq!(
            again,
            Fitted::Resized(expected.clone()),
            "{kind}: must serve the replacement"
        );
    }
}

/// Every generation permit, held until the returned guards drop, and the lock
/// that keeps two tests from each taking half of them.
///
/// The permits are the process-wide `GENERATION_LIMIT`. Under nextest each
/// test is its own process, so no other test waits on them. Under plain
/// `cargo test`, which `.cargo/mutants.toml` uses for every sweep, the tests
/// share one process, and another test's generation waits while they are held.
///
/// **`ONE_HOLDER` is what keeps two callers from hanging each other.** The
/// permits are taken one at a time. Without the lock, two tests could each
/// take one permit and wait forever for the second, and every test needing a
/// permit would then queue behind them.
fn hold_every_permit() -> (
    std::sync::MutexGuard<'static, ()>,
    Vec<crate::limit::SemaphoreGuard<'static>>,
) {
    static ONE_HOLDER: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let one_holder = ONE_HOLDER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let permits = (0..MAX_CONCURRENT_GENERATIONS)
        .map(|_| GENERATION_LIMIT.acquire())
        .collect();
    (one_holder, permits)
}

/// Runs `request` on another thread while this thread holds every generation
/// permit, and returns what it served, or `None` if it had not finished
/// within five seconds.
///
/// A request that wrongly waits for a permit blocks past the deadline: a
/// cache hit, or a page that already fits (O111). The permits are released
/// after it, so that test fails instead of hanging.
fn served_while_every_permit_is_held<T: Send>(request: impl FnOnce() -> T + Send) -> Option<T> {
    let (_one_holder, held) = hold_every_permit();
    let (sent, received) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(move || {
            let _ = sent.send(request());
        });
        let served = received
            .recv_timeout(std::time::Duration::from_secs(5))
            .ok();
        drop(held);
        served
    })
}

/// Starts `generate` on another thread while this thread holds every
/// generation permit. Returns whether it had raised its flag after 300 ms,
/// and whether it had once the permits were released and it finished.
///
/// `generate` raises the flag at the step that must wait for a permit. For
/// a cover that is the fetch. For a reading page it is finishing, because a
/// page fetches and reads its header before it takes one (O111).
///
/// A correct generation cannot reach that step while the permits are held,
/// so the first answer is `false` however slow the machine is. Only a
/// generation that skips the permit can make it `true`, and 300 ms is ample
/// for that to show. The permits are released before the thread is joined,
/// so a correct implementation cannot hang this.
fn flagged_while_every_permit_is_held(
    generate: impl FnOnce(&std::sync::atomic::AtomicBool) + Send,
) -> (bool, bool) {
    let flag = std::sync::atomic::AtomicBool::new(false);
    let (_one_holder, held) = hold_every_permit();
    let seen = &flag;
    let early = std::thread::scope(|scope| {
        scope.spawn(move || generate(seen));
        std::thread::sleep(std::time::Duration::from_millis(300));
        let early = seen.load(Ordering::SeqCst);
        drop(held);
        early
    });
    (early, flag.load(Ordering::SeqCst))
}

/// A cache hit never waits for a generation permit. `cached`'s doc gives that
/// as the reason it checks markers rather than decoding: the hit runs outside
/// `GENERATION_LIMIT`, so a warm grid stays fully parallel while a cold one
/// queues. This one covers `at_path`, which covers and tiles share.
#[test]
fn a_cached_cover_is_served_while_every_generation_permit_is_held() {
    let dir = tempdir_for_test();
    let warm = cached_or_generated(dir.path(), "abc", || Ok(real_page_bytes())).unwrap();

    let served = served_while_every_permit_is_held(|| {
        cached_or_generated(dir.path(), "abc", || {
            panic!("a cache hit must not regenerate")
        })
        .unwrap()
    });

    assert_eq!(
        served.as_deref(),
        Some(warm.as_slice()),
        "a warm cover must not wait for a generation permit"
    );
}

/// The same, for `cached_or_generated_page`, which has its own copy of the
/// hit.
#[test]
fn a_cached_reading_page_is_served_while_every_generation_permit_is_held() {
    let dir = tempdir_for_test();
    let warm = cached_or_generated_page(dir.path(), "abc", 3, 8, || Ok(real_page_bytes())).unwrap();
    assert!(
        matches!(warm, Fitted::Resized(_)),
        "a 16x16 page resized to 8 is cached"
    );

    let served = served_while_every_permit_is_held(|| {
        cached_or_generated_page(dir.path(), "abc", 3, 8, || -> Result<Vec<u8>, _> {
            panic!("a cache hit must not regenerate")
        })
        .unwrap()
    });

    assert_eq!(
        served,
        Some(warm),
        "a warm reading page must not wait for a generation permit"
    );
}

/// A cold generation waits for a permit, which is the half of
/// `GENERATION_LIMIT` the hit tests above do not reach. The permit is what
/// bounds peak memory, and `MAX_CONCURRENT_GENERATIONS` gives the
/// measurement. This one covers `at_path`.
///
/// It does not pin how many permits there are. That number is a tuning
/// choice with its reason in its own doc comment, and this test holds however
/// many it says.
#[test]
fn a_cold_cover_waits_for_a_generation_permit() {
    let dir = tempdir_for_test();

    let (early, finally) = flagged_while_every_permit_is_held(|fetched| {
        cached_or_generated(dir.path(), "abc", || {
            fetched.store(true, Ordering::SeqCst);
            Ok(real_page_bytes())
        })
        .unwrap();
    });

    assert!(
        !early,
        "a cold cover must not fetch its page while every permit is held"
    );
    assert!(finally, "and must fetch it once a permit is free");
}

/// The same, for `cached_or_generated_page`, which takes its own permit
/// around the resize.
///
/// **What waits is the resize, not the fetch.** The fetch and the header parse
/// run outside the permit (O111), so a page that fits skips the queue. A page
/// that needs resizing fetches while the permits are held and waits before it
/// decodes, so the flag is raised once the call has returned a resized page.
/// Deleting the permit around the resize makes it finish inside the 300 ms.
#[test]
fn a_cold_reading_page_that_needs_resizing_waits_for_a_generation_permit() {
    let dir = tempdir_for_test();

    let (early, finally) = flagged_while_every_permit_is_held(|resized| {
        let served =
            cached_or_generated_page(dir.path(), "abc", 3, 8, || Ok(real_page_bytes())).unwrap();
        assert!(
            matches!(served, Fitted::Resized(_)),
            "a 16x16 page asked for at 8 is resized"
        );
        resized.store(true, Ordering::SeqCst);
    });

    assert!(
        !early,
        "a reading page must not be resized while every permit is held"
    );
    assert!(finally, "and must be once a permit is free");
}

/// O111: a page that already fits never waits for a generation permit.
///
/// Learning that it fits takes the fetch and a header parse. Neither decodes
/// anything, so neither needs the permit, which bounds decode memory.
#[test]
fn a_page_that_already_fits_is_served_while_every_generation_permit_is_held() {
    let dir = tempdir_for_test();

    let served = served_while_every_permit_is_held(|| {
        cached_or_generated_page(dir.path(), "abc", 3, 16, || Ok(real_page_bytes())).unwrap()
    });

    assert_eq!(
        served,
        Some(Fitted::Original(real_page_bytes())),
        "a 16x16 page fits a long edge of 16 and must not wait for a permit"
    );
}

/// A page is resized exactly when its long edge exceeds the request.
///
/// The permit sits between `fits_within` and `resize_to_long_edge` (O111).
/// This pins the boundary on both sides, a page equal to the request and one a
/// pixel over, through the halves and through `cached_or_generated_page`, and
/// pins the resized bytes to what `resize_to_long_edge` makes.
#[test]
fn a_page_is_resized_only_when_its_long_edge_exceeds_the_request() {
    let page = real_page_bytes();
    assert!(fits_within(&page, 16).unwrap(), "16x16 fits 16");
    assert!(!fits_within(&page, 15).unwrap(), "16x16 does not fit 15");
    let resized = resize_to_long_edge(&page, 15).unwrap();
    let decoded = image::load_from_memory(&resized).unwrap();
    assert_eq!(decoded.width().max(decoded.height()), 15);

    let dir = tempdir_for_test();
    assert_eq!(
        cached_or_generated_page(dir.path(), "abc", 3, 16, || Ok(page.clone())).unwrap(),
        Fitted::Original(page.clone()),
        "a page equal to the request is handed back"
    );
    assert_eq!(
        cached_or_generated_page(dir.path(), "abc", 3, 15, || Ok(page.clone())).unwrap(),
        Fitted::Resized(resized),
        "a page a pixel over it is resized, to what `resize_to_long_edge` makes"
    );
}

/// A second request for the same reading-sized page reads the cache rather
/// than resizing again. A hit check that turned down every file would make the
/// page cache regenerate on every request.
#[test]
fn a_cached_reading_page_is_not_regenerated() {
    let dir = tempdir_for_test();
    let first =
        cached_or_generated_page(dir.path(), "abc", 3, 8, || Ok(real_page_bytes())).unwrap();
    assert!(
        matches!(first, Fitted::Resized(_)),
        "a 16x16 page resized to 8 is cached"
    );
    let second = cached_or_generated_page(dir.path(), "abc", 3, 8, || -> Result<Vec<u8>, _> {
        panic!("a cache hit must not regenerate")
    })
    .unwrap();
    assert_eq!(second, first);
}

/// O107: a page that already fits is fetched once, handed back as it was
/// fetched, and not cached.
///
/// The page here is not a bare `Vec`, so "as it was fetched" means more than
/// the same bytes: the field beside them has to come back too. That field is
/// what lets `get_page` keep the content type. The route test
/// `a_page_that_already_fits_the_width_is_read_once` counts the reads.
#[test]
fn a_page_that_already_fits_is_fetched_once_and_handed_back() {
    #[derive(Debug, PartialEq)]
    struct Fetched {
        bytes: Vec<u8>,
        content_type: &'static str,
    }
    impl AsRef<[u8]> for Fetched {
        fn as_ref(&self) -> &[u8] {
            &self.bytes
        }
    }

    let dir = tempdir_for_test();
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let fitted = cached_or_generated_page(dir.path(), "abc", 3, 16, || {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok(Fetched {
            bytes: real_page_bytes(),
            content_type: "image/png",
        })
    })
    .unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 1, "fetched once");
    assert_eq!(
        fitted,
        Fitted::Original(Fetched {
            bytes: real_page_bytes(),
            content_type: "image/png",
        }),
        "a 16x16 page fits a long edge of 16 and comes back as it was fetched"
    );
    assert!(
        !page_cache_path(dir.path(), "abc", 3, 16).exists(),
        "and nothing is cached for it"
    );
}

/// O106: a reading page's key changes with each parameter it was rendered
/// under. `page_key_at` takes the version as an argument for `keyed_path`'s
/// reason: a test can vary it without a second copy of `PAGE_FORMAT_VERSION`.
///
/// The key is both the cache file's name and the page's `ETag`, and
/// `routes`' `a_resized_page_tag_is_the_cache_key_it_was_rendered_under`
/// holds the tag to it. A key missing a parameter gives two renderings one
/// file and one tag: without the offset, every page of a member at one width
/// is served whichever page was cached first, and validates the others' tags.
///
/// **Each parameter changes on its own.**
/// `a_reading_page_and_a_tile_never_share_a_cache_path` changes offset and
/// edge together, so it cannot show a key that lacks the offset.
///
/// **The name's shape is pinned too**, because O106 promised it would not
/// change: every page already cached is named `<sha256>-p<offset>-r<edge>v2.jpg`,
/// and a key in any other shape regenerates all of them. Swapping offset and
/// edge is the case that needs it, since the swapped key still tells every
/// page apart. The version is left out of the pin so that a bump still passes.
#[test]
fn changing_any_rendering_parameter_changes_the_page_key() {
    let dir = std::path::Path::new("/cache");
    let current = page_key_at("abc", 3, 800, PAGE_FORMAT_VERSION);

    assert_eq!(
        current,
        page_key("abc", 3, 800),
        "page_key must use the real constant, not a copy of it"
    );
    assert_eq!(
        page_cache_path(dir, "abc", 3, 800),
        dir.join(format!("{current}.jpg")),
        "the cache file is named by the key"
    );
    assert!(
        current.starts_with("abc-p3-r800v"),
        "the name every cached page already has: {current}"
    );
    for (changed, other) in [
        ("sha256", page_key_at("abd", 3, 800, PAGE_FORMAT_VERSION)),
        ("offset", page_key_at("abc", 4, 800, PAGE_FORMAT_VERSION)),
        ("long edge", page_key_at("abc", 3, 801, PAGE_FORMAT_VERSION)),
        (
            "format version",
            page_key_at("abc", 3, 800, PAGE_FORMAT_VERSION + 1),
        ),
    ] {
        assert_ne!(current, other, "a changed {changed} must change the key");
    }
}

/// A cover's version changes exactly when the `ETag` of a cover size would,
/// whatever the size: it is what lets `routes::COVER_CACHE_CONTROL` keep an
/// address for good, so an input the tags have and the version lacks would
/// keep old bytes under an address that no longer names them.
///
/// Every pair of inputs below is compared both ways at every cover size: the
/// two tags differ if and only if the two versions do. Each input changes on
/// its own, for `changing_any_rendering_parameter_changes_the_page_key`'s
/// reason. The size is not an argument, so the three sizes of one page share
/// one version.
///
/// **Hex and `-` only** over a real digest. `cover_address` encodes it all
/// the same, since an imported sidecar's hash need not be hex.
#[test]
fn a_cover_version_changes_exactly_when_a_cover_tag_does_at_every_size() {
    let sha = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
    let other_sha = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    let current = cover_version_at(sha, 0, PAGE_FORMAT_VERSION);
    assert_eq!(
        current,
        cover_version(sha, 0),
        "cover_version must use the real constant, not a copy of it"
    );
    assert!(
        current.chars().all(|c| c.is_ascii_hexdigit() || c == '-'),
        "hex and `-` only: {current}"
    );

    let inputs = [
        ("as it is", sha, 0, PAGE_FORMAT_VERSION),
        ("sha256", other_sha, 0, PAGE_FORMAT_VERSION),
        ("offset", sha, 1, PAGE_FORMAT_VERSION),
        ("format version", sha, 0, PAGE_FORMAT_VERSION + 1),
    ];
    for (a_name, a_sha, a_offset, a_format) in inputs {
        for (b_name, b_sha, b_offset, b_format) in inputs {
            let versions_differ = cover_version_at(a_sha, a_offset, a_format)
                != cover_version_at(b_sha, b_offset, b_format);
            for edge in COVER_LONG_EDGES {
                let tags_differ = page_key_at(a_sha, a_offset, edge, a_format)
                    != page_key_at(b_sha, b_offset, edge, b_format);
                assert_eq!(
                    versions_differ, tags_differ,
                    "{a_name} against {b_name} at {edge} px: the tags differ \
                     {tags_differ}, the versions differ {versions_differ}"
                );
            }
        }
    }
}

/// Two different outputs must never share one path. `page_cache_path` and
/// `thumb_cache_path` can be asked for the same sha, offset and long edge —
/// a tile cropped to a strip, and the whole page — and a content-addressed
/// cache that returned one for the other would show a reader a crop where a
/// page belongs.
///
/// The `r` in the name is what keeps them apart, and a `page_cache_path` that
/// returned `Default::default()` — the empty path, shared by every page of
/// every work — would fail this.
#[test]
fn a_reading_page_and_a_tile_never_share_a_cache_path() {
    let dir = std::path::Path::new("/cache");
    // At the tile's own long edge, so the two differ by the `r` in the
    // name rather than by the size — which is the case the doc calls out.
    let page = page_cache_path(dir, "abc", 3, THUMB_LONG_EDGE);
    let tile = thumb_cache_path(dir, "abc", 3, THUMB_LONG_EDGE);

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

/// O134: the long edge a tile is generated at and the path it is cached under
/// come from one argument. A second size is a second file, and the first is
/// left exactly as it was — which is what lets a library that has been read
/// keep every tile it has already made.
#[test]
fn a_tile_is_cached_and_generated_at_the_long_edge_it_was_asked_for() {
    let dir = tempdir_for_test();
    let page = || Ok(real_page_bytes());

    let small = cached_or_generated_thumb(dir.path(), "abc", 7, THUMB_LONG_EDGE, page).unwrap();
    let large = cached_or_generated_thumb(dir.path(), "abc", 7, 480, page).unwrap();

    assert_eq!(long_edge_of(&small), THUMB_LONG_EDGE, "the 160 px tile");
    assert_eq!(long_edge_of(&large), 480, "the 480 px tile");
    assert!(
        thumb_cache_path(dir.path(), "abc", 7, THUMB_LONG_EDGE).exists(),
        "the 160 px file is still there"
    );
    assert!(
        thumb_cache_path(dir.path(), "abc", 7, 480).exists(),
        "and the 480 px file is beside it"
    );
    assert_ne!(
        std::fs::read(thumb_cache_path(dir.path(), "abc", 7, THUMB_LONG_EDGE)).unwrap(),
        std::fs::read(thumb_cache_path(dir.path(), "abc", 7, 480)).unwrap(),
        "two sizes are two images, not one file written twice"
    );
}

/// The long edge of an encoded image, for the test above.
fn long_edge_of(bytes: &[u8]) -> u32 {
    let image = image::load_from_memory(bytes).expect("a decodable image");
    image.width().max(image.height())
}

/// `TILE_LONG_EDGES` is pinned to a literal here, deliberately, rather than
/// being read through the constant (O134).
///
/// Every other test that touches `TILE_LONG_EDGES` — `routes/tests.rs`'s
/// `the_tile_route_sizes_any_page_to_a_tile_rung` and
/// `a_download_only_account_gets_the_unsized_tile_whatever_rung_it_asks_for` —
/// gets its edges by `for edge in crate::cover::TILE_LONG_EDGES`, so each one
/// tests whatever the constant currently says rather than what it must say.
/// None of them can see its *values* change.
///
/// `web/src/tileSize.ts`'s `TILE_LADDER` is documented to hold the same three
/// numbers, and `web/tests/tileSize.test.ts` pins its half the same way this
/// test pins this side. Without this test the two could drift and every test
/// on both sides would stay green: a rung present on one side only is not
/// rejected — `routes.rs::get_thumb`'s `tile_edge` falls back to the 160 px
/// tile for a `w` it does not recognise, the same fallback `cover_edge`
/// already makes — so a screen asking for a rung this server dropped, or
/// offering one the server never made, is served a soft image instead of the
/// sharp one it asked for, and nothing goes red.
#[test]
fn tile_long_edges_is_pinned_to_its_literal() {
    assert_eq!(
        TILE_LONG_EDGES,
        [160, 320, 480],
        "web/src/tileSize.ts's TILE_LADDER must hold the same three; a rung \
         missing from either side is answered with the 160 px tile instead of \
         failing loudly"
    );
}
