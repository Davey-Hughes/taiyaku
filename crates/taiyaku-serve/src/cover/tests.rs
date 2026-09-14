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
/// Exercises `taiyaku_read::atomic::temp_path` directly, now that this
/// module's own `unique_temp_path` copy is gone — the property under test is
/// unchanged, only which function proves it.
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
/// cache key cannot see each other's files — and one that removes itself
/// when the test ends, which the local version of this helper did not.
/// `test_support::Fixture` carries the argument for both halves.
fn tempdir_for_test() -> crate::test_support::Fixture {
    crate::test_support::Fixture::new("taiyaku-thumb")
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
    let first = cached_or_generated_thumb(dir.path(), "abc", 7, page).unwrap();
    let second = cached_or_generated_thumb(dir.path(), "abc", 7, || {
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
/// An empty file is the case that was found. A zero-filled file is the same
/// failure a crash can leave at full length. Both fail both halves of the
/// check a hit must pass, so a check that tested only one half would still
/// reject them. The other four are each built to fail exactly one half:
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
/// Before this, a hit was any file `fs::read` could open, so an empty one was
/// served to every client, and nothing ever replaced it, because the key
/// never changes.
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
        let path = thumb_cache_path(dir.path(), "abc", 7);
        std::fs::write(&path, &broken).unwrap();
        let calls = std::sync::atomic::AtomicUsize::new(0);

        let served = cached_or_generated_thumb(dir.path(), "abc", 7, || {
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
        let again = cached_or_generated_thumb(dir.path(), "abc", 7, || {
            panic!("{kind}: the replacement must be a hit")
        })
        .unwrap();
        assert_eq!(again, expected, "{kind}: must serve the replacement");
    }
}

/// O105, for a reading-sized page. This one does not go through `at_path`,
/// so it had its own copy of the defect and needs its own witness. The long
/// edge is 8 against a 16x16 page, so `fit_at` resizes and a file is cached.
#[test]
fn a_broken_cached_reading_page_is_regenerated_and_replaced() {
    let expected = fit_at(&real_page_bytes(), 8).unwrap().unwrap();
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
            served.as_deref(),
            Some(expected.as_slice()),
            "{kind}: must serve the regenerated page"
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            expected,
            "{kind}: must replace the broken file"
        );
        let again = cached_or_generated_page(dir.path(), "abc", 3, 8, || {
            panic!("{kind}: the replacement must be a hit")
        })
        .unwrap();
        assert_eq!(
            again.as_deref(),
            Some(expected.as_slice()),
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
/// share one process, and another test's generation waits while they are
/// held.
///
/// **`ONE_HOLDER` is what keeps two callers from hanging each other.** The
/// permits are taken one at a time. Without the lock, two tests could each
/// take one permit and wait forever for the second, and every test needing a
/// permit then queued behind them: 5 of 6 plain `cargo test` runs of
/// `cover::tests` hung. The fix-round review of O105 found it, and the first
/// version of this comment said the tests "do not hang".
fn hold_every_permit() -> (
    std::sync::MutexGuard<'static, ()>,
    Vec<SemaphoreGuard<'static>>,
) {
    static ONE_HOLDER: Mutex<()> = Mutex::new(());
    let one_holder = ONE_HOLDER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let permits = (0..MAX_CONCURRENT_GENERATIONS)
        .map(|_| GENERATION_LIMIT.acquire())
        .collect();
    (one_holder, permits)
}

/// Runs `hit` on another thread while this thread holds every generation
/// permit, and returns what it served, or `None` if it had not finished
/// within five seconds.
///
/// A hit that wrongly waits for a permit blocks past the deadline. The
/// permits are released after it, so that test fails instead of hanging.
fn served_while_every_permit_is_held<T: Send>(hit: impl FnOnce() -> T + Send) -> Option<T> {
    let (_one_holder, held) = hold_every_permit();
    let (sent, received) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(move || {
            let _ = sent.send(hit());
        });
        let served = received
            .recv_timeout(std::time::Duration::from_secs(5))
            .ok();
        drop(held);
        served
    })
}

/// Starts `generate` on another thread while this thread holds every
/// generation permit. Returns whether it had fetched its page after 300 ms,
/// and whether it had once the permits were released and it finished.
///
/// A correct generation cannot fetch while the permits are held, so the
/// first answer is `false` however slow the machine is. Only a generation
/// that skips the permit can make it `true`, and 300 ms is ample for that to
/// show. The permits are released before the thread is joined, so a correct
/// implementation cannot hang this.
fn fetched_while_every_permit_is_held(
    generate: impl FnOnce(&std::sync::atomic::AtomicBool) + Send,
) -> (bool, bool) {
    let fetched = std::sync::atomic::AtomicBool::new(false);
    let (_one_holder, held) = hold_every_permit();
    let seen = &fetched;
    let early = std::thread::scope(|scope| {
        scope.spawn(move || generate(seen));
        std::thread::sleep(std::time::Duration::from_millis(300));
        let early = seen.load(Ordering::SeqCst);
        drop(held);
        early
    });
    (early, fetched.load(Ordering::SeqCst))
}

/// A cache hit never waits for a generation permit. `cached`'s doc gives
/// that as the reason it checks markers rather than decoding: the hit runs
/// outside `GENERATION_LIMIT`, so a warm grid stays fully parallel while
/// a cold one queues. Nothing tested it. Moving the hit inside the permit, in
/// `at_path` or in `cached_or_generated_page`, left every test green, which
/// the whole-branch review of O105 found. This one covers `at_path`, which
/// covers and tiles share.
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
    let warm = cached_or_generated_page(dir.path(), "abc", 3, 8, || Ok(real_page_bytes()))
        .unwrap()
        .expect("a 16x16 page resized to 8 is cached");

    let served = served_while_every_permit_is_held(|| {
        cached_or_generated_page(dir.path(), "abc", 3, 8, || {
            panic!("a cache hit must not regenerate")
        })
        .unwrap()
    });

    assert_eq!(
        served.flatten().as_deref(),
        Some(warm.as_slice()),
        "a warm reading page must not wait for a generation permit"
    );
}

/// A cold generation waits for a permit, which is the half of
/// `GENERATION_LIMIT` the hit tests above do not reach. The permit is what
/// bounds peak memory, and `MAX_CONCURRENT_GENERATIONS` gives the
/// measurement. Deleting the line that takes it, in `at_path` or in
/// `cached_or_generated_page`, left every test green, which the review of
/// O105's fix round found. This one covers `at_path`.
///
/// It does not pin how many permits there are. That number is a tuning
/// choice with its reason in its own doc comment, and this test holds however
/// many it says.
#[test]
fn a_cold_cover_waits_for_a_generation_permit() {
    let dir = tempdir_for_test();

    let (early, finally) = fetched_while_every_permit_is_held(|fetched| {
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

/// The same, for `cached_or_generated_page`, which takes its own permit.
#[test]
fn a_cold_reading_page_waits_for_a_generation_permit() {
    let dir = tempdir_for_test();

    let (early, finally) = fetched_while_every_permit_is_held(|fetched| {
        cached_or_generated_page(dir.path(), "abc", 3, 8, || {
            fetched.store(true, Ordering::SeqCst);
            Ok(real_page_bytes())
        })
        .unwrap();
    });

    assert!(
        !early,
        "a cold reading page must not fetch while every permit is held"
    );
    assert!(finally, "and must fetch once a permit is free");
}

/// A second request for the same reading-sized page reads the cache rather
/// than resizing again. Covers and tiles each had this test and pages did
/// not, so a hit check that turned down every file would have passed for
/// pages. It would make the page cache regenerate on every request.
#[test]
fn a_cached_reading_page_is_not_regenerated() {
    let dir = tempdir_for_test();
    let first = cached_or_generated_page(dir.path(), "abc", 3, 8, || Ok(real_page_bytes()))
        .unwrap()
        .expect("a 16x16 page resized to 8 is cached");
    let second = cached_or_generated_page(dir.path(), "abc", 3, 8, || {
        panic!("a cache hit must not regenerate")
    })
    .unwrap();
    assert_eq!(second.as_deref(), Some(first.as_slice()));
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
