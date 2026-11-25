//! Helpers for tests in this crate and, through the `test-support` feature,
//! in crates that depend on it.
//!
//! **Why a feature rather than a copy in each crate.** O104's strace harness
//! is needed by `atomic`'s tests here and by `stamp`'s in `taiyaku-library`,
//! and a second copy of a log parser is a second place for the two to drift
//! apart — one of them silently matching nothing. `taiyaku-library` enables
//! the feature only under `[dev-dependencies]`, so it is compiled into that
//! crate's test builds and any `--all-features` build, and into no binary
//! this workspace ships. The file name is the one the coverage report and
//! `.cargo/mutants.toml` exclude.
//!
//! [`TempDir`] is here for the same reason, and for every crate with tests
//! rather than two: `taiyaku-library`, `taiyaku-serve` and `taiyaku-cli` enable
//! the feature the same way. O117 is why it exists — see its own doc.

use std::path::{Path, PathBuf};

/// One call from a strace log, reduced to what O104's ordering needs.
#[derive(Debug)]
pub enum Call {
    /// A successful write of any kind — `write`, `pwrite64`, `writev`,
    /// `copy_file_range` and the rest — to the files at these paths.
    Write(Vec<PathBuf>),
    /// An `fsync` or `fdatasync` of the file or directory at this path.
    Sync(PathBuf),
    /// A `rename`, `renameat` or `renameat2` of `from` onto `to`.
    Rename { from: PathBuf, to: PathBuf },
}

/// Every syscall that can put bytes into a file, so a write strace did not
/// report cannot hide after a sync.
const WRITES: &[&str] = &[
    "write",
    "writev",
    "pwrite64",
    "pwritev",
    "pwritev2",
    "copy_file_range",
    "sendfile",
    "splice",
    "ftruncate",
    "fallocate",
];

/// Of [`WRITES`], the ones whose arguments are all descriptors and numbers —
/// no data string — so every path in the line is one the call touched.
const DESCRIPTORS_ONLY: &[&str] = &["copy_file_range", "sendfile", "splice"];

/// Run one test of this binary again as a child under `strace`, and return
/// every successful write, sync and rename it made, in order.
///
/// The child is this same test binary, asked for exactly `test` and handed
/// `child` through the environment; the test notices the variable and does
/// only the traced work. `-y` has strace print the path behind every file
/// descriptor, so a call names the file it touched without this parser
/// tracking `openat` and `close`.
///
/// **A missing `strace` fails the test rather than skipping it**, unlike the
/// `NOT MEASURED:` shape `format/tests.rs` uses for `tar`. That test checks
/// that a fixture is well formed; this one is the only thing standing
/// between a later edit and a crash destroying a reader's file, and a skip
/// is a green run that measured nothing.
///
/// # Panics
///
/// If `strace` is not on `PATH`, if the traced child fails, or if strace
/// wrote no log.
#[must_use]
pub fn traced(test: &str, child: (&str, &std::ffi::OsStr), log: &Path) -> Vec<Call> {
    let recorded = strace(
        test,
        child,
        &format!(
            "fsync,fdatasync,rename,renameat,renameat2,{}",
            WRITES.join(",")
        ),
        log,
    );
    rejoined(&recorded)
        .iter()
        .filter_map(|line| call(line))
        .collect()
}

/// Run one test of this binary again as a child under `strace`, tracing the
/// syscalls `trace` names, and return the log as strace wrote it.
///
/// [`traced`]'s runner. Split out for O120, whose test in `taiyaku-serve`
/// asks different syscalls a different question — which thread made a
/// `stat` — and reads the log itself rather than through a second copy of
/// this invocation. Under `-f` each line begins with the id of the thread
/// that made the call.
///
/// # Panics
///
/// If `strace` is not on `PATH`, if the traced child fails, or if strace
/// wrote no log. A missing `strace` fails rather than skips, for
/// [`traced`]'s reason.
#[must_use]
pub fn strace(test: &str, child: (&str, &std::ffi::OsStr), trace: &str, log: &Path) -> String {
    let exe = std::env::current_exe().expect("the test binary's own path");
    let ran = std::process::Command::new("strace")
        .args(["-f", "-qq", "-y", "-e"])
        .arg(format!("trace={trace}"))
        .arg("-o")
        .arg(log)
        .arg(exe)
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env(child.0, child.1)
        .output()
        .expect("`strace` must be on PATH: a traced test cannot observe its syscalls without it");
    assert!(
        ran.status.success(),
        "the traced child failed: {}\n{}",
        String::from_utf8_lossy(&ran.stdout),
        String::from_utf8_lossy(&ran.stderr)
    );
    std::fs::read_to_string(log).expect("strace wrote its log")
}

/// strace's log with every interrupted call put back on one line.
///
/// Under `-f`, a call still in progress when another thread's traced call
/// starts is printed as `1234 write(3</a>, "x", 1 <unfinished ...>` and later
/// finished as `1234 <... write resumed>) = 1`. Either half on its own parses
/// as nothing — a write that silently vanished from the order.
pub(crate) fn rejoined(recorded: &str) -> Vec<String> {
    let mut pending: std::collections::HashMap<&str, String> = std::collections::HashMap::new();
    let mut lines = Vec::new();
    for line in recorded.lines() {
        let pid = line.split_whitespace().next().unwrap_or_default();
        if let Some(head) = line.strip_suffix(" <unfinished ...>") {
            pending.insert(pid, head.to_string());
        } else if let Some(at) = line.find(" resumed>") {
            let head = pending.remove(pid).unwrap_or_default();
            lines.push(format!("{head}{}", &line[at + " resumed>".len()..]));
        } else {
            lines.push(line.to_string());
        }
    }
    lines
}

/// One line of the log as a [`Call`], or nothing for a call that failed or is
/// not one of the three kinds. A call succeeded when what follows its last
/// `) = ` is a non-negative count.
fn call(line: &str) -> Option<Call> {
    let (_, result) = line.rsplit_once(") = ")?;
    result.split_whitespace().next()?.parse::<u64>().ok()?;

    let name = line.split_whitespace().nth(1)?.split('(').next()?;
    let args = &line[line.find('(')? + 1..];
    if name == "fsync" || name == "fdatasync" {
        return Some(Call::Sync(decorated(args).next()?));
    }
    if name.starts_with("rename") {
        let mut quoted = args.split('"').skip(1).step_by(2).map(PathBuf::from);
        return Some(Call::Rename {
            from: quoted.next()?,
            to: quoted.next()?,
        });
    }
    if WRITES.contains(&name) {
        // A data string can hold `<` and `>` of its own — a ComicInfo
        // document is XML — so outside the descriptor-only calls only the
        // first decoration, the destination descriptor, is trusted.
        let paths: Vec<PathBuf> = if DESCRIPTORS_ONLY.contains(&name) {
            decorated(args).collect()
        } else {
            decorated(args).take(1).collect()
        };
        return Some(Call::Write(paths));
    }
    None
}

/// The paths `-y` printed after descriptors in `args`, in order.
fn decorated(args: &str) -> impl Iterator<Item = PathBuf> + '_ {
    args.split('<').skip(1).filter_map(|part| {
        let (path, _) = part.split_once('>')?;
        Some(PathBuf::from(path))
    })
}

/// O104's ordering for a staged file renamed onto `path` inside `dir`: every
/// write to the staged file, then a sync of it, then the rename, then a sync
/// of `dir` — and nothing written to `path` after the rename.
///
/// **A sync before the last write proves nothing**: syncing the empty
/// temporary and then writing to it, or syncing a stamp's staged archive
/// before `finish` writes its central directory, are both that case. So the
/// sync must follow the last write, and at least one write must have been
/// seen at all, or an unrecognised write syscall would make the whole
/// ordering vacuous.
///
/// # Panics
///
/// Naming whichever step is missing or out of order.
pub fn assert_replaced_durably(calls: &[Call], path: &Path, dir: &Path) {
    let renamed = calls
        .iter()
        .position(|c| matches!(c, Call::Rename { to, .. } if to == path))
        .unwrap_or_else(|| panic!("nothing was renamed onto {}: {calls:?}", path.display()));
    let Call::Rename { from: staged, .. } = &calls[renamed] else {
        unreachable!()
    };
    let (before, after) = (&calls[..renamed], &calls[renamed + 1..]);

    let last_write = before.iter().rposition(|c| wrote_to(c, staged)).unwrap_or_else(|| {
        panic!(
            "no write to {} was seen before the rename, so its ordering proves nothing: {calls:?}",
            staged.display()
        )
    });
    assert!(
        before[last_write + 1..]
            .iter()
            .any(|c| matches!(c, Call::Sync(p) if p == staged)),
        "{} was renamed onto {} without being synced after its last write: {calls:?}",
        staged.display(),
        path.display()
    );
    assert!(
        !after.iter().any(|c| wrote_to(c, path)),
        "{} was written to after the rename, where no sync before it covers the bytes: {calls:?}",
        path.display()
    );
    assert!(
        after.iter().any(|c| matches!(c, Call::Sync(p) if p == dir)),
        "{} was not synced after the rename, so the rename may not survive a crash: {calls:?}",
        dir.display()
    );
}

/// Whether `call` wrote to the file at `target`.
fn wrote_to(call: &Call, target: &Path) -> bool {
    matches!(call, Call::Write(paths) if paths.iter().any(|p| p == target))
}

/// A directory of its own for one test, removed with everything in it
/// when dropped.
///
/// **O117.** A fixed name under the temp dir is shared by every run on the
/// machine. Two checkouts running their suites at once, or a `cargo test`
/// beside a `cargo mutants`, would delete and rewrite each other's files
/// mid-test and fail with `disk I/O error` or `NotFound` for a reason that was
/// never the change under test. `docs/notes.md`, "O117", has the reproduction.
///
/// The name is `taiyaku-test-{label}-{pid}-{n}`. The process id separates two
/// runs: a counter alone restarts at nought in every process, which under
/// `cargo nextest` is every test. The counter separates two directories in one
/// process, which under `cargo test` is every test in a crate. The label is
/// only for a person reading the temp dir.
///
/// A directory already at that name is removed first: only a run killed
/// before its `Drop` leaves one, and its process id has since been reused by
/// this one.
///
/// Removal failures on drop are ignored in both directions: a test that
/// passed must not be failed by a directory that will not go, and one that
/// failed must not have its message replaced — a panic during unwinding aborts
/// the process.
///
/// No `tempfile` dependency: the workspace's dependency list is short enough
/// to quote in `docs/status.md`, and this is the whole of what that crate
/// would be used for.
#[derive(Debug)]
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// A new, empty directory under the temp dir, unique to this process and
    /// this call.
    ///
    /// # Panics
    ///
    /// If `label` holds a `/`, which would leave the directories above the
    /// last one behind on drop, or if the directory cannot be created.
    #[must_use]
    pub fn new(label: &str) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        assert!(
            !label.contains('/'),
            "a TempDir label is one path component: {label}"
        );
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("taiyaku-test-{label}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path)
            .unwrap_or_else(|e| panic!("creating {}: {e}", path.display()));
        Self { path }
    }

    /// The directory itself.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// So `&dir` goes wherever a `&Path` does and `dir.join(…)` reads as it does
/// on a `PathBuf`.
impl std::ops::Deref for TempDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for TempDir {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests;

/// A reader that hands back one byte per call, however much is asked for.
///
/// Legal `Read` behaviour, and what a network filesystem does under load. No
/// file written to this host's local disk will produce it, which is why
/// `read_header` takes a `Read` at all, and why the header check in `pages.rs`
/// does too.
#[derive(Debug)]
pub struct OneByteAtATime<'a> {
    pub rest: &'a [u8],
}

impl std::io::Read for OneByteAtATime<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.rest.is_empty() || buf.is_empty() {
            return Ok(0);
        }
        buf[0] = self.rest[0];
        self.rest = &self.rest[1..];
        Ok(1)
    }
}

/// A minimal reflowable EPUB at `dir/name`: one spine document holding each
/// of `paragraphs` as a `<p>`.
pub fn epub(dir: &Path, name: &str, paragraphs: &[&str]) -> PathBuf {
    epub_with(dir, name, "", paragraphs)
}

/// [`epub`], its package's metadata holding `dc`, Dublin Core elements written
/// as `<dc:title>…</dc:title>`.
pub fn epub_with(dir: &Path, name: &str, dc: &str, paragraphs: &[&str]) -> PathBuf {
    let body: String = paragraphs.iter().map(|p| format!("<p>{p}</p>")).collect();
    let metadata =
        format!(r#"<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">{dc}</metadata>"#);
    let files = [
        ("mimetype", "application/epub+zip".to_string()),
        (
            "META-INF/container.xml",
            r#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="content.opf"/></rootfiles></container>"#
                .to_string(),
        ),
        (
            "content.opf",
            format!(
                r#"<package xmlns="http://www.idpf.org/2007/opf">{metadata}<manifest><item id="t" href="text.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="t"/></spine></package>"#
            ),
        ),
        (
            "text.xhtml",
            format!(r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>{body}</body></html>"#),
        ),
    ];
    let path = dir.join(name);
    write_zip(&path, files);
    path
}

/// A new zip at `path` holding `entries` in the order given, each with zip's
/// default options but a fixed modification time.
fn write_zip<B: AsRef<[u8]>>(
    path: &Path,
    entries: impl IntoIterator<Item = (impl Into<String>, B)>,
) {
    use std::io::Write;
    // zip's default stamps each entry with the wall clock, so two calls with
    // the same entries could write different bytes and hash apart; a test that
    // needs two members of the same bytes (a fingerprint-cache hit, a shared
    // sha256) would then pass or fail by the clock.
    let options =
        zip::write::SimpleFileOptions::default().last_modified_time(zip::DateTime::default());
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).expect("create zip"));
    for (name, bytes) in entries {
        zip.start_file(name.into(), options).expect("start entry");
        zip.write_all(bytes.as_ref()).expect("write entry");
    }
    zip.finish().expect("finish zip");
}

/// `image` encoded as PNG.
#[cfg(feature = "test-support")]
fn png(image: &image::GrayImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .expect("a gray image always encodes");
    bytes
}

/// A cbz of `count` decodable pages that fingerprint differently from each
/// other. Seeds used together must be at least `count` apart, since page `i + 1`
/// of seed `s` is page `i` of seed `s + 1`.
///
/// Not a solid fill: taiyaku-cli's `fingerprint::fingerprint` is a difference
/// hash, so every flat page hashes alike and any alignment would look perfect
/// by accident. The pattern varies along both axes, by page and by `seed`.
// Gated on the feature because `image` exists only with it.
#[cfg(feature = "test-support")]
pub fn cbz_pages(dir: &Path, name: &str, count: u32, seed: u32) -> PathBuf {
    let path = dir.join(name);
    write_zip(
        &path,
        (0..count).map(|i| {
            let level = seed.wrapping_add(i).wrapping_mul(101);
            let image = image::GrayImage::from_fn(8, 8, |x, y| {
                let v = x.wrapping_mul(37).wrapping_add(y.wrapping_mul(59)) + level;
                let byte = u8::try_from(v % 256)
                    .expect("v % 256 is in 0..256 by construction, which is exactly u8's range");
                image::Luma([byte])
            });
            (format!("{i:03}.png"), png(&image))
        }),
    );
    path
}

/// One page of [`PRINT_FIXTURE`] as taiyaku-cli's `describe_pages` records
/// it, each hash as its four words: this crate cannot name `PageHash`.
#[cfg(feature = "test-support")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinnedPrint {
    pub width: u32,
    pub height: u32,
    pub hash: [u64; 4],
    pub halves: Option<([u64; 4], [u64; 4])>,
}

/// The print fixture's art. It varies along both axes, so no line of it is
/// flat; it never reaches 255, so the bars are the only white; and it moves
/// with `seed`, so no two pages hash alike.
#[cfg(feature = "test-support")]
fn print_ink(x: u32, y: u32, seed: u32) -> u8 {
    let v = (x * 7 + y * 31 + seed * 53) % 180;
    40 + u8::try_from(v).expect("v % 180 is under 180, which fits u8")
}

/// One print-fixture page as greyscale PNG: `width` by `height`, flat 255
/// bars of `[left, right, top, bottom]` pixels, and [`print_ink`] art seeded
/// by `seed` inside them.
#[cfg(feature = "test-support")]
#[must_use]
pub fn print_page(width: u32, height: u32, bars: [u32; 4], seed: u32) -> Vec<u8> {
    let [left, right, top, bottom] = bars;
    png(&image::GrayImage::from_fn(width, height, |x, y| {
        let art = (left..width - right).contains(&x) && (top..height - bottom).contains(&y);
        image::Luma([if art { print_ink(x, y, seed) } else { 255 }])
    }))
}

/// The pages [`PRINT_FIXTURE`] pins, as greyscale PNG named `p00.png` to
/// `p25.png`, in reading order. Every page is 128 tall. Sampled at a step of
/// two, the pages' bars give a common border of 6 a side and 5 above and
/// below, and every page loses it.
#[cfg(feature = "test-support")]
#[must_use]
pub fn print_fixture_pages() -> Vec<(String, Vec<u8>)> {
    (0..26u32)
        .map(|index| {
            let (width, bars) = match index {
                // A full-bleed cover: a minimum in place of the median reads
                // its zero and cancels the crop for every page.
                0 => (96, [0; 4]),
                // Never sampled at a step of two; sampling every page moves
                // the median to 12.
                odd if odd % 2 == 1 => (96, [12; 4]),
                // Wider than tall after the crop too, so it gets halves; odd
                // after it, so a right half cut to the edge differs from one
                // cut `mid` wide.
                8 => (201, [6, 3, 2, 5]),
                // Wider than tall only before the crop, so it gets none.
                10 => (130, [6, 3, 2, 5]),
                // Uneven bars: without evening up each pair, 3 and 2 come
                // off where 6 and 5 should.
                _ => (96, [6, 3, 2, 5]),
            };
            (
                format!("p{index:02}.png"),
                print_page(width, 128, bars, index),
            )
        })
        .collect()
}

/// `dir/print-fixture.cbz`, holding [`print_fixture_pages`] in order.
#[cfg(feature = "test-support")]
pub fn print_fixture_cbz(dir: &Path) -> PathBuf {
    let path = dir.join("print-fixture.cbz");
    write_zip(&path, print_fixture_pages());
    path
}

/// `dir/print-fixture/`, holding [`print_fixture_pages`] as files.
#[cfg(feature = "test-support")]
pub fn print_fixture_folder(dir: &Path) -> PathBuf {
    let path = dir.join("print-fixture");
    std::fs::create_dir(&path).expect("create the fixture folder");
    for (name, bytes) in print_fixture_pages() {
        std::fs::write(path.join(name), bytes).expect("write a page");
    }
    path
}

/// What taiyaku-cli's `describe_pages` gives for [`print_fixture_pages`],
/// page by page. The thresholds that decide a content match were set on hashes
/// made this way (`docs/notes.md`, "Edition matching by content"), so every
/// copy of the fingerprint code must give these exactly: a value that changes
/// here changes what those thresholds mean.
#[cfg(feature = "test-support")]
pub const PRINT_FIXTURE: [PinnedPrint; 26] = [
    PinnedPrint {
        width: 84,
        height: 118,
        hash: [
            0x5543_C2AA_A835_731C,
            0x5555_570A_AAA8_3855,
            0x2155_5554_0AAA_AAA1,
            0xC739_1554_542A_2AA1,
        ],
        halves: None,
    },
    PinnedPrint {
        width: 84,
        height: 118,
        hash: [
            0x2553_350B_2E63_1863,
            0x2693_2553_24AB_2AAB,
            0x2AAB_2553_355B_2AAB,
            0x2319_231B_3543_22AB,
        ],
        halves: None,
    },
    PinnedPrint {
        width: 84,
        height: 118,
        hash: [
            0xAA1C_1555_508A_8E7B,
            0xAAA8_3855_5543_C2AA,
            0x0AAA_AAA1_5555_570A,
            0x18EF_22A8_2155_5555,
        ],
        halves: None,
    },
    PinnedPrint {
        width: 84,
        height: 118,
        hash: [
            0x2AAB_2833_314B_330F,
            0x342B_2AAB_2153_352B,
            0x2553_2AAB_2A83_2553,
            0x0C63_3CE3_2A0B_2553,
        ],
        halves: None,
    },
    PinnedPrint {
        width: 84,
        height: 118,
        hash: [
            0x5542_AAAA_8615_31CE,
            0x5555_C2AA_AAA8_1555,
            0x5555_550A_AAAA_A855,
            0xE31D_5555_4B2A_AAA9,
        ],
        halves: None,
    },
    PinnedPrint {
        width: 84,
        height: 118,
        hash: [
            0x255B_298B_2E2B_0C63,
            0x2153_355B_2AAB_2AAB,
            0x2A83_2553_342B_2AAB,
            0x318F_339B_2553_22AB,
        ],
        halves: None,
    },
    PinnedPrint {
        width: 84,
        height: 118,
        hash: [
            0xAE15_5550_58AA_A638,
            0xAAAA_1555_5542_AAAA,
            0xAAAA_A855_5555_42AA,
            0x8C63_A3A8_5955_550A,
        ],
        halves: None,
    },
    PinnedPrint {
        width: 84,
        height: 118,
        hash: [
            0x2AAB_235B_3DC3_31C7,
            0x2AAB_2AAB_2553_250B,
            0x342B_2AAB_2153_355B,
            0x4633_2E33_2A8B_2553,
        ],
        halves: None,
    },
    PinnedPrint {
        width: 189,
        height: 118,
        hash: [
            0x6666_3331_8DC8_4AB5,
            0xCCCC_6666_1111_CCCC,
            0x1111_CCCC_6666_1111,
            0x54AB_2B10_CCCC_6666,
        ],
        halves: Some((
            [
                0x2854_050A_5188_CE73,
                0x42D1_285A_850A_52A1,
                0x0685_6050_AC2A_8685,
                0x9CCE_4211_A150_9428,
            ],
            [
                0x2A5A_050A_42B1_19CC,
                0x6850_2C1A_8585_50D1,
                0x4285_2852_9C2A_8285,
                0x7399_4AC6_A150_142D,
            ],
        )),
    },
    PinnedPrint {
        width: 84,
        height: 118,
        hash: [
            0x355B_2AEB_229B_6619,
            0x2553_250B_2AAB_2853,
            0x2153_3553_2AAB_2AA3,
            0x18C7_39C3_243B_2AAB,
        ],
        halves: None,
    },
    PinnedPrint {
        width: 118,
        height: 118,
        hash: [
            0x3860_860C_C983_3664,
            0xE1C6_3871_061C_6186,
            0x871C_61C7_3871_871C,
            0x4CC8_1336_61C5_9870,
        ],
        halves: None,
    },
    PinnedPrint {
        width: 84,
        height: 118,
        hash: [
            0x2AAB_255B_1C63_18C3,
            0x2AAB_285B_355B_22AB,
            0x2C2B_2AAB_2153_350B,
            0x2319_2F3B_28C3_2553,
        ],
        halves: None,
    },
    PinnedPrint {
        width: 84,
        height: 118,
        hash: [
            0x2AAA_AA85_7154_CC63,
            0x54A8_AAAA_A554_5554,
            0x5551_D2AA_AA96_9555,
            0x1947_4718_AAAA_1A55,
        ],
        halves: None,
    },
    PinnedPrint {
        width: 84,
        height: 118,
        hash: [
            0x254B_2AAB_2B8B_631D,
            0x355B_22AB_2AAB_2553,
            0x2153_350B_2AAB_287B,
            0x0C63_3C63_260B_2AAB,
        ],
        halves: None,
    },
    PinnedPrint {
        width: 84,
        height: 118,
        hash: [
            0x9555_552A_8AA3_39CC,
            0x8595_5550_2CAA_AA85,
            0xAA96_8555_54B0_AAAA,
            0xE338_9857_5551_D2AA,
        ],
        halves: None,
    },
    PinnedPrint {
        width: 84,
        height: 118,
        hash: [
            0x2A1B_3553_2E7B_0C63,
            0x2AAB_2853_254B_22AB,
            0x2AAB_28AB_355B_270B,
            0x338D_238B_2853_351B,
        ],
        halves: None,
    },
    PinnedPrint {
        width: 84,
        height: 118,
        hash: [
            0x2AAB_A955_5556_C739,
            0x34AA_AAA5_A555_552A,
            0x54D0_AAAA_8695_5554,
            0x9C63_52AA_AAAA_8555,
        ],
        halves: None,
    },
    PinnedPrint {
        width: 84,
        height: 118,
        hash: [
            0x20CB_2A3B_39C3_318F,
            0x254B_22AB_2A1B_2553,
            0x3553_270B_2AAB_2853,
            0x0633_1C73_238B_28EB,
        ],
        halves: None,
    },
    PinnedPrint {
        width: 84,
        height: 118,
        hash: [
            0x5555_0CAA_EAA9_18C7,
            0xA155_552C_0AAA_A961,
            0xAA94_D554_54AA_AAA5,
            0x338C_8D55_5550_AAAA,
        ],
        halves: None,
    },
    PinnedPrint {
        width: 84,
        height: 118,
        hash: [
            0x2613_3543_263B_0639,
            0x2A2B_2553_214B_2AAB,
            0x2AAB_285B_2543_22AB,
            0x38C7_31CB_2553_250B,
        ],
        halves: None,
    },
    PinnedPrint {
        width: 84,
        height: 118,
        hash: [
            0xAAA9_7155_1D43_E398,
            0x2AAA_29A5_5555_0D2A,
            0x542A_AAAA_A155_5554,
            0xC671_38AE_AA84_D554,
        ],
        halves: None,
    },
    PinnedPrint {
        width: 84,
        height: 118,
        hash: [
            0x20AB_2A8B_3CC3_18C7,
            0x354B_2AAB_2E13_2553,
            0x3553_22AB_2AAB_2553,
            0x6719_273B_28AB_2A5B,
        ],
        halves: None,
    },
    PinnedPrint {
        width: 84,
        height: 118,
        hash: [
            0x554B_82AA_A31C_1C53,
            0x5555_152A_AAA9_6855,
            0xA155_5554_2AAA_2AA5,
            0x38C6_C550_542A_2AAA,
        ],
        halves: None,
    },
    PinnedPrint {
        width: 84,
        height: 118,
        hash: [
            0x2553_3463_239B_6319,
            0x2A13_2553_20AB_2A8B,
            0x2AAB_2553_254B_2AAB,
            0x0863_28C3_3553_22AB,
        ],
        halves: None,
    },
    PinnedPrint {
        width: 84,
        height: 118,
        hash: [
            0xAA68_5555_5662_738E,
            0xAAAA_2855_5555_42AA,
            0x2AAA_2AA1_5555_550A,
            0x6738_2AA2_A155_5554,
        ],
        halves: None,
    },
    PinnedPrint {
        width: 84,
        height: 118,
        hash: [
            0x2AAB_2183_2C73_0C63,
            0x20AB_2AAB_2553_355B,
            0x254B_2AAB_2A13_2553,
            0x218D_231B_2AEB_2513,
        ],
        halves: None,
    },
];

/// How a RAR fixture's manifest describes one entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RarEntryKind {
    File,
    Dir,
    /// A file whose data is encrypted, which refuses the archive it is in.
    Encrypted,
    /// A file stored as a reference to an earlier identical one (rar's `-oi`),
    /// or as a hard link to an earlier name of it (`-oh`): listed as a file,
    /// and read as its target's bytes.
    Reference,
    /// A file of no bytes, which reads as none.
    Empty,
    /// A file continued from or into another volume of a set, whose archive
    /// is refused as one volume.
    Split,
}

/// One `entry` line of a RAR fixture's manifest.
#[derive(Debug, Clone)]
pub struct RarFixtureEntry {
    pub name: String,
    pub kind: RarEntryKind,
    pub size: u64,
}

/// A RAR archive in `fixtures/rar/` and what its manifest says it holds.
#[derive(Debug, Clone)]
pub struct RarFixture {
    /// The archive's file name.
    pub file: String,
    pub path: PathBuf,
    pub format: String,
    /// In archive order.
    pub entries: Vec<RarFixtureEntry>,
    /// In reading order, with each page's sha256.
    pub pages: Vec<(String, String)>,
    /// Entries read by name that are not pages, with their sha256.
    pub others: Vec<(String, String)>,
    /// What an archive that cannot be listed is refused with.
    pub refused: Option<String>,
}

/// Every fixture in `fixtures/rar/`, by file name, from its `<file>.manifest`.
///
/// `make.sh` writes the manifests: tab-separated, one fact a line, each
/// `format <rar4|rar5>`, `entry <name> <file|dir|encrypted|reference|empty|split>
/// <size>`, `page <name> <sha256>`, `other <name> <sha256>` or
/// `refused <text>`.
///
/// # Panics
///
/// Naming the manifest and the line, on a line it cannot read.
#[must_use]
pub fn rar_fixtures() -> Vec<RarFixture> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/rar");
    let mut manifests: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "manifest"))
        .collect();
    manifests.sort();
    manifests
        .iter()
        .map(|manifest| rar_fixture(manifest))
        .collect()
}

fn rar_fixture(manifest: &Path) -> RarFixture {
    let text = std::fs::read_to_string(manifest)
        .unwrap_or_else(|e| panic!("reading {}: {e}", manifest.display()));
    let path = manifest.with_extension("");
    let mut fixture = RarFixture {
        file: path
            .file_name()
            .expect("a manifest names its archive")
            .to_string_lossy()
            .into_owned(),
        path,
        format: String::new(),
        entries: Vec::new(),
        pages: Vec::new(),
        others: Vec::new(),
        refused: None,
    };
    for (index, line) in text.lines().enumerate() {
        let malformed = || -> String { format!("{}:{}: {line:?}", manifest.display(), index + 1) };
        match line.split('\t').collect::<Vec<_>>()[..] {
            ["format", format @ ("rar4" | "rar5")] => format.clone_into(&mut fixture.format),
            ["entry", name, kind, size] => fixture.entries.push(RarFixtureEntry {
                name: name.to_owned(),
                kind: match kind {
                    "file" => RarEntryKind::File,
                    "dir" => RarEntryKind::Dir,
                    "encrypted" => RarEntryKind::Encrypted,
                    "reference" => RarEntryKind::Reference,
                    "empty" => RarEntryKind::Empty,
                    "split" => RarEntryKind::Split,
                    _ => panic!("{}: no such kind", malformed()),
                },
                size: size
                    .parse()
                    .unwrap_or_else(|_| panic!("{}: not a size", malformed())),
            }),
            ["page", name, sha] if is_sha256(sha) => {
                fixture.pages.push((name.to_owned(), sha.to_owned()));
            }
            ["other", name, sha] if is_sha256(sha) => {
                fixture.others.push((name.to_owned(), sha.to_owned()));
            }
            ["refused", text] => fixture.refused = Some(text.to_owned()),
            _ => panic!("{}: not a manifest line", malformed()),
        }
    }
    assert!(
        !fixture.format.is_empty(),
        "{}: no format line",
        manifest.display()
    );
    fixture
}

fn is_sha256(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// `bytes`' sha256 in lower-case hex, as the RAR fixtures' manifests give it.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    use std::fmt::Write as _;
    let mut hex = String::with_capacity(64);
    for b in &sha2::Sha256::digest(bytes) {
        let _ = write!(hex, "{b:02x}");
    }
    hex
}
