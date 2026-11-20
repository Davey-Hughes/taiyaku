//! What a JPEG XL decode does to what the whole process shares: the rayon pool
//! jxl-oxide renders on and `image`'s decoding hooks. Each test runs again in a
//! process of its own, because the first use of the pool fixes its threads'
//! priority for the life of the process, and a hook once registered stays.

use std::io::Cursor;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

use image::{DynamicImage, ImageReader, ImageResult};

use super::{ICC_CAP, METADATA_CAP, icc_pages, rebuilt_jpeg, register};
use crate::fingerprint_pass::priority;
use crate::test_support::{self, jxl_fixture};

/// Set in the process a test runs again in.
const ALONE: &str = "TAIYAKU_JXL_PROCESS_TEST";

/// How long a wait only a defect runs into may last: inside nextest's limit
/// for one test, so the failure names what it was waiting for.
const DEFECT: Duration = Duration::from_secs(45);

/// How many steps of niceness the process started below normal priority sits
/// at, which a lowered thread must still be able to go under.
const NICED_BY: i32 = 5;

/// Runs `scenario` in a process that has done nothing else yet.
fn alone(name: &str, scenario: impl FnOnce()) {
    again(name, None, scenario);
}

/// [`alone`], where a thread can be seen to run below the process's priority.
/// A process already near niceness 19, as a mutation sweep's is, has no lower
/// priority to put a thread at, and so nothing these tests could tell apart.
fn alone_where_lowering_shows(name: &str, scenario: impl FnOnce()) {
    if lowering_shows(name) {
        alone(name, scenario);
    }
}

fn lowering_shows(name: &str) -> bool {
    if std::env::var_os(ALONE).is_some() {
        return true;
    }
    for tool in ["renice", "ionice", "nice"] {
        if Command::new(tool).arg("--version").output().is_err() {
            eprintln!("NOT MEASURED: {name} (no `{tool}` on PATH)");
            return false;
        }
    }
    if main_niceness() + NICED_BY >= 19 {
        eprintln!("NOT MEASURED: {name} (this process is already near niceness 19)");
        return false;
    }
    true
}

/// Runs the test binary again for this test alone, through `nice` when
/// `niced_by` says so. Counting the one test that passed catches a name that
/// matches none, which libtest would report as success.
fn again(name: &str, niced_by: Option<i32>, scenario: impl FnOnce()) {
    if std::env::var_os(ALONE).is_some() {
        scenario();
        return;
    }
    let binary: PathBuf = std::env::current_exe().expect("names this test binary");
    let mut command = match niced_by {
        Some(steps) => {
            let mut nice = Command::new("nice");
            nice.arg("-n").arg(steps.to_string()).arg(&binary);
            nice
        }
        None => Command::new(&binary),
    };
    let test = format!("jxl::process_tests::{name}");
    let output = command
        .args(["--exact", &test, "--nocapture", "--test-threads=1"])
        .env(ALONE, "1")
        .output()
        .expect("runs the test binary again");
    let out = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && out.contains("1 passed"),
        "{test} in a process of its own:\n{out}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The niceness in a `stat` file: field 19, counted from the name's closing
/// parenthesis since the name can hold a space.
fn niceness_in(stat_file: &str) -> i32 {
    let stat = std::fs::read_to_string(stat_file).expect("reads the stat file");
    let (_, fields) = stat.rsplit_once(')').expect("a stat line has a name");
    let niceness = fields.split_whitespace().nth(16).expect("has field 19");
    niceness.parse().expect("niceness is a number")
}

fn my_niceness() -> i32 {
    niceness_in("/proc/thread-self/stat")
}

/// The main thread's, which `/proc/self/stat` reports for the whole process.
fn main_niceness() -> i32 {
    niceness_in("/proc/self/stat")
}

/// Lowers this thread as a fingerprint pass lowers its workers.
fn lower_this_thread() {
    priority::lower().expect("lowers the thread");
    assert!(
        my_niceness() > main_niceness(),
        "the thread runs below the process's priority"
    );
}

/// One step below the process rather than all the way to 19: any step down is
/// lowered, and a page this size would crawl at 19 beside a busy test run.
fn lower_this_thread_one_step() {
    let niceness = (main_niceness() + 1).to_string();
    let renice = Command::new("renice")
        .args(["--priority", &niceness, "-p", &thread_id()])
        .output()
        .expect("runs renice");
    assert!(renice.status.success(), "renice lowers this thread");
    assert!(my_niceness() > main_niceness(), "the thread is lowered");
}

fn on_a_lowered_thread<T: Send>(work: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                lower_this_thread();
                work()
            })
            .join()
            .expect("the lowered thread ends")
    })
}

fn decoded(fixture: &str) -> ImageResult<DynamicImage> {
    ImageReader::new(Cursor::new(jxl_fixture(fixture)))
        .with_guessed_format()
        .expect("reads from memory")
        .decode()
}

/// Every way a page reaches jxl-oxide: the decode, whose first-frame check and
/// render are separate readings of the page, and the rebuilt JPEG.
fn decode_a_page_every_way() {
    decoded("lossy.jxl").expect("decodes the page");
    assert_eq!(
        rebuilt_jpeg(jxl_fixture("recompressed.jxl")).as_deref(),
        Some(jxl_fixture("recompressed.jpg")),
        "rebuilds the JPEG the page was made from"
    );
}

/// Every thread of the shared pool, kept from taking work until released.
struct Held {
    release: Arc<Barrier>,
}

impl Held {
    fn release(self) {
        self.release.wait();
    }
}

/// Starts the shared pool from this thread and holds all of it.
fn hold_the_pool() -> Held {
    rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build_global()
        .expect("nothing has used the pool in this process");
    let workers = rayon::current_num_threads();
    let entered = Arc::new(Barrier::new(workers + 1));
    let release = Arc::new(Barrier::new(workers + 1));
    rayon::spawn_broadcast({
        let entered = Arc::clone(&entered);
        let release = Arc::clone(&release);
        move |_| {
            entered.wait();
            release.wait();
        }
    });
    entered.wait();
    Held { release }
}

/// This thread's id, as `/proc/self/task` lists it.
fn thread_id() -> String {
    let link = std::fs::read_link("/proc/thread-self").expect("reads this thread's link");
    let id = link.file_name().expect("the link ends in the thread's id");
    id.to_string_lossy().into_owned()
}

/// Whether the thread is asleep rather than running or runnable.
fn asleep(thread: &str) -> bool {
    let stat = std::fs::read_to_string(format!("/proc/self/task/{thread}/stat"))
        .expect("reads the thread's stat");
    let (_, fields) = stat.rsplit_once(')').expect("a stat line has a name");
    fields.split_whitespace().next() == Some("S")
}

/// Whether the decode came to rest waiting, and stayed there, rather than
/// finishing. A decode doing its own work never sleeps this long: it reads
/// nothing from disk and shares no lock, so its thread stays runnable until it
/// ends. Decided by what the thread does, so a slow machine cannot change it.
fn rests_without_finishing(thread: &str, finished: &Receiver<()>) -> bool {
    const SAMPLES_ASLEEP: u32 = 50;
    let deadline = Instant::now() + DEFECT;
    let mut samples = 0;
    while Instant::now() < deadline {
        if finished.try_recv().is_ok() {
            return false;
        }
        samples = if asleep(thread) { samples + 1 } else { 0 };
        if samples == SAMPLES_ASLEEP {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

/// With every pool thread held, a decode at the process's own priority cannot
/// finish, and finishes once they are let go.
fn a_decode_waits_for_the_held_pool() {
    register();
    let held = hold_the_pool();
    let (named, name) = mpsc::channel();
    let (done, finished) = mpsc::channel();
    let decoding = std::thread::spawn(move || {
        named.send(thread_id()).expect("the test is waiting");
        decoded("lossy.jxl").expect("decodes the page");
        done.send(()).expect("the test is waiting");
    });
    let thread = name.recv().expect("the decoding thread names itself");

    assert!(
        rests_without_finishing(&thread, &finished),
        "the decode finished with every pool thread held, so it did not use the pool"
    );

    held.release();
    finished
        .recv_timeout(DEFECT)
        .expect("the decode finishes once the pool is free");
    decoding.join().expect("the decoding thread ends");
}

#[test]
fn a_lowered_decode_never_starts_the_shared_pool() {
    alone_where_lowering_shows("a_lowered_decode_never_starts_the_shared_pool", || {
        register();

        on_a_lowered_thread(decode_a_page_every_way);

        assert!(
            rayon::ThreadPoolBuilder::new()
                .num_threads(1)
                .build_global()
                .is_ok(),
            "a lowered thread started the shared pool, whose threads keep its priority"
        );
    });
}

/// Every pool thread is held throughout, so all of the decode's work happened
/// on the one thread that was free to do it, at that thread's priority.
#[test]
fn a_lowered_decode_runs_on_its_own_thread_while_the_pool_is_busy() {
    alone_where_lowering_shows(
        "a_lowered_decode_runs_on_its_own_thread_while_the_pool_is_busy",
        || {
            register();
            let held = hold_the_pool();
            let (done, finished) = mpsc::channel();

            std::thread::spawn(move || {
                lower_this_thread();
                decode_a_page_every_way();
                done.send(my_niceness()).expect("the test is waiting");
            });

            let niceness = finished
                .recv_timeout(DEFECT)
                .expect("a lowered decode waited for the shared pool");
            assert_eq!(
                niceness, 19,
                "the work ran at the lowered thread's priority"
            );
            held.release();
        },
    );
}

#[test]
fn a_lowered_decode_first_leaves_the_pool_at_normal_priority() {
    alone_where_lowering_shows(
        "a_lowered_decode_first_leaves_the_pool_at_normal_priority",
        || {
            register();

            on_a_lowered_thread(decode_a_page_every_way);
            decoded("lossy.jxl").expect("a reader's decode after the pass's");

            let normal = main_niceness();
            let pool = rayon::broadcast(|_| my_niceness());
            assert!(
                pool.iter().all(|&niceness| niceness == normal),
                "the pool's threads run at {pool:?}, the process at {normal}"
            );
        },
    );
}

#[test]
fn a_normal_decode_uses_the_pool() {
    alone_where_lowering_shows(
        "a_normal_decode_uses_the_pool",
        a_decode_waits_for_the_held_pool,
    );
}

/// A server someone started under `nice` runs every thread at that niceness:
/// none of them is below the server's own priority, so serving stays parallel.
#[test]
fn a_server_started_below_normal_priority_still_uses_the_pool() {
    const NAME: &str = "a_server_started_below_normal_priority_still_uses_the_pool";
    if lowering_shows(NAME) {
        again(NAME, Some(NICED_BY), || {
            assert!(main_niceness() >= NICED_BY, "the process was started niced");
            a_decode_waits_for_the_held_pool();
        });
    }
}

#[test]
fn a_lowered_decode_draws_what_a_normal_decode_draws() {
    alone_where_lowering_shows("a_lowered_decode_draws_what_a_normal_decode_draws", || {
        register();
        for fixture in [
            "lossy.jxl",
            "lossless.jxl",
            "grey.jxl",
            "alpha.jxl",
            "animated.jxl",
            "large.jxl",
            "recompressed.jxl",
        ] {
            let normal = decoded(fixture).expect("decodes at normal priority");
            let lowered = std::thread::scope(|scope| {
                scope
                    .spawn(|| {
                        lower_this_thread_one_step();
                        decoded(fixture).expect("decodes lowered")
                    })
                    .join()
                    .expect("the lowered thread ends")
            });
            assert!(
                normal.as_bytes() == lowered.as_bytes(),
                "{fixture} draws differently on a lowered thread"
            );
        }
    });
}

/// Nothing else in this process registers the hooks, so `serve` is what makes
/// the page readable.
#[test]
fn serve_registers_the_jpeg_xl_decoder() {
    alone("serve_registers_the_jpeg_xl_decoder", || {
        assert!(
            decoded("lossy.jxl").is_err(),
            "the control: nothing has registered the decoder in this process yet"
        );
        let tmp = test_support::Fixture::new("serve-registers-jxl");
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("builds a runtime");
        runtime.spawn(crate::serve(
            test_support::library(),
            crate::Works::fixed(Vec::new()),
            crate::ServeOptions {
                addr: "127.0.0.1:0".to_owned(),
                cache: Some(tmp.join("cache")),
                ..crate::ServeOptions::default()
            },
        ));

        let deadline = Instant::now() + DEFECT;
        while decoded("lossy.jxl").is_err() {
            assert!(
                Instant::now() < deadline,
                "serve is running and still cannot read a JPEG XL page"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        runtime.shutdown_background();
    });
}

/// The size the hand-built pages declare, the most jxl-oxide fills: far above
/// what the rest of a process of its own holds at its peak, so half of it
/// tells a fill from everything else.
const ZERO_FILL: u64 = 1 << 28;

/// The process's peak resident memory so far.
fn peak_resident_bytes() -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").expect("reads this process's status");
    let line = status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .expect("the status has a VmHWM line");
    let kib: u64 = line
        .trim()
        .strip_suffix("kB")
        .expect("VmHWM is in kB")
        .trim()
        .parse()
        .expect("VmHWM is a number");
    kib * 1024
}

/// What `work` gave, and how far it raised the process's peak resident memory.
fn rise_in_peak<T>(work: impl FnOnce() -> T) -> (T, u64) {
    let before = peak_resident_bytes();
    let given = work();
    (given, peak_resident_bytes() - before)
}

/// The control, after the check's own measurement since a peak never falls:
/// jxl-oxide reading the page with nothing of taiyaku's ahead of it fills the
/// declared profile, so a smaller rise before it was the check's doing.
fn jxl_oxide_alone_fills_the_profile(page: &[u8]) {
    let (read, rise) = rise_in_peak(|| super::read_page(page, usize::MAX));
    assert!(
        read.is_err(),
        "the control: jxl-oxide read the hand-built page"
    );
    assert!(
        rise >= ZERO_FILL / 2,
        "the control: jxl-oxide alone raised peak resident memory by only {rise} bytes"
    );
}

#[test]
fn a_page_declaring_a_huge_colour_profile_is_refused_before_jxl_oxide_fills_it() {
    alone(
        "a_page_declaring_a_huge_colour_profile_is_refused_before_jxl_oxide_fills_it",
        || {
            register();
            let page = icc_pages::declaring(ZERO_FILL);

            let (decoded, rise) = rise_in_peak(|| {
                ImageReader::new(Cursor::new(&page))
                    .with_guessed_format()
                    .expect("reads from memory")
                    .decode()
            });
            assert!(
                decoded.is_err(),
                "the page declaring a huge profile decoded"
            );
            assert!(
                rise < ZERO_FILL / 2,
                "the hook let jxl-oxide fill the declared profile: peak resident memory rose by {rise} bytes"
            );

            jxl_oxide_alone_fills_the_profile(&page);
        },
    );
}

#[test]
fn a_page_declaring_a_huge_colour_profile_is_not_rebuilt_and_never_filled() {
    alone(
        "a_page_declaring_a_huge_colour_profile_is_not_rebuilt_and_never_filled",
        || {
            register();
            let page = icc_pages::in_jxlp(&icc_pages::declaring(ZERO_FILL));

            let (rebuilt, rise) = rise_in_peak(|| rebuilt_jpeg(&page));
            assert!(
                rebuilt.is_none(),
                "the page declaring a huge profile was rebuilt to {:?} bytes",
                rebuilt.as_ref().map(Vec::len)
            );
            assert!(
                rise < ZERO_FILL / 2,
                "rebuilt_jpeg let jxl-oxide fill the declared profile: peak resident memory rose by {rise} bytes"
            );

            jxl_oxide_alone_fills_the_profile(&page);
        },
    );
}

/// Within the cap, a page's profile still costs what jxl-oxide spends on it
/// once read, outside every limit a caller sets: it decodes the profile from
/// the encoded one and parses what it decoded. The cap keeps the worst of one
/// decode and parse within what the metadata boxes may inflate to.
#[test]
fn the_worst_profile_the_cap_passes_is_decoded_and_parsed_within_the_metadata_cap() {
    alone(
        "the_worst_profile_the_cap_passes_is_decoded_and_parsed_within_the_metadata_cap",
        || {
            let len = usize::try_from(ICC_CAP).expect("the cap fits in memory");
            let encoded = icc_pages::expanding_profile(len);
            assert_eq!(
                encoded.len(),
                len,
                "the fixture: the profile is as long as the cap"
            );

            let ((decoded, kept), rise) = rise_in_peak(|| {
                let profile = jxl_color::icc::decode_icc(&encoded).expect("jxl-color decodes it");
                let parsed = jxl_color::ColorEncodingWithProfile::with_icc(&profile)
                    .expect("jxl-color parses what it decoded");
                (profile.len(), parsed.icc_profile().len())
            });
            // Each tag command writes 36 bytes, and all but a few of the
            // encoded bytes are tag commands.
            assert!(
                decoded / len >= 35,
                "the fixture: {len} encoded bytes decoded to only {decoded}"
            );
            assert_eq!(
                kept, decoded,
                "the fixture: the parse kept a copy of the profile, as for one it cannot name"
            );
            assert!(
                rise < METADATA_CAP as u64,
                "a profile of {len} encoded bytes, decoded to {decoded} and parsed, raised peak resident memory by {rise} bytes"
            );
        },
    );
}
