//! Whether page fingerprints can match manga and comics editions safely enough
//! to apply a match without a person: the measurement behind edition matching
//! by content (`docs/superpowers/specs/2026-11-15-edition-matching-by-content-design.md`).
//!
//! Each labelled pair of members is aligned by the solver `align` uses and
//! scored two ways (`features`): V, how much of both sides long runs cover,
//! and C, whether the shorter side's long runs land in one stretch of the
//! longer side about its own length. `rule` sets each class's bar the gap
//! above its highest wrong pair, and `lookup` ranks every member's nearest
//! members by page votes, which is how a scan would find a pair to score.
//!
//! **It asserts the instrument's controls, never a quality figure.** The
//! tables are read and the decisions recorded in `docs/notes.md`, the section
//! "Edition matching by content".
//!
//! - `TAIYAKU_MATCH_LIBRARY`: the library root the manifest's paths are under.
//! - `TAIYAKU_MATCH_EVIDENCE`: the directory the runs write their files into.
//! - `TAIYAKU_MATCH_NAS_ROOT`: the read-only root `nas_timing` reads, alone.
//!
//! Unset skips. Set to anything but an absolute path to a directory panics,
//! because cargo runs these tests from `crates/taiyaku-cli/`, where a relative
//! path finds nothing and the run would pass having measured nothing. Each
//! gated test runs alone, in release:
//!
//! ```sh
//! TAIYAKU_MATCH_LIBRARY=/abs/library TAIYAKU_MATCH_EVIDENCE=/abs/evidence \
//!   cargo test --release -p taiyaku-cli match_experiment::<test> -- --exact --nocapture
//! ```
//!
//! `fingerprint_and_lookup` keeps its own `logs/fingerprint.log`, whose first
//! line is its stamp, written before the first member is read. A rerun
//! replaces the log, so that line dates the first fingerprint of the
//! measured material only beside `cache hits: 0` on its members line, the
//! members any of whose fingerprints came from `$EV/fp-cache`. With hits, the
//! oldest entry under `$EV/fp-cache` dates it instead: an entry is keyed by
//! content and written only on a miss, so a rerun cannot move it earlier.
//! Cargo's output and the run's exit status go to a file of their own:
//!
//! ```sh
//! EV=/abs/evidence
//! TAIYAKU_MATCH_LIBRARY=/abs/library TAIYAKU_MATCH_EVIDENCE=$EV RAYON_NUM_THREADS=16 \
//!   cargo test --release -p taiyaku-cli match_experiment::fingerprint_and_lookup \
//!   -- --exact --nocapture >| $EV/logs/fingerprint-cargo.log 2>&1
//! echo "exit=$?" >> $EV/logs/fingerprint-cargo.log
//! ```
//!
//! `nas_timing` writes no file. It reads the compiled-in sample under its
//! root, and `/proc`, and its output is the evidence, so its test binary can
//! run in a container that mounts nothing but the library. Its stamp names
//! the commit only where git and the source are at hand, and elsewhere reads
//! `HEAD unknown`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use taiyaku_core::PageHash;

mod features;
mod held_out;
mod lookup;
mod manifest;
mod rule;
mod score;
mod sheet;
mod tests;
mod volume_test;

/// Where a run reads its members from and writes its evidence to.
#[derive(Debug)]
struct Material {
    library: PathBuf,
    evidence: PathBuf,
}

/// The two roots, or `None` when `TAIYAKU_MATCH_LIBRARY` is unset.
///
/// Unset is an absence and skips; set-but-unusable is a mistake and panics,
/// as `corpus::corpus` rules. The evidence directory is checked whenever it
/// is set, and required once the library is, since a run that writes nowhere
/// has measured nothing anyone can read.
fn material() -> Option<Material> {
    let evidence = std::env::var("TAIYAKU_MATCH_EVIDENCE").ok();
    if let Some(value) = &evidence {
        refuse_unusable("TAIYAKU_MATCH_EVIDENCE", value);
    }
    let library = std::env::var("TAIYAKU_MATCH_LIBRARY").ok()?;
    refuse_unusable("TAIYAKU_MATCH_LIBRARY", &library);
    let Some(evidence) = evidence else {
        panic!(
            "TAIYAKU_MATCH_LIBRARY is set but TAIYAKU_MATCH_EVIDENCE is not: \
             set it to an absolute path to the directory the run writes into."
        );
    };
    Some(Material {
        library: PathBuf::from(library),
        evidence: PathBuf::from(evidence),
    })
}

fn refuse_unusable(var: &str, value: &str) {
    if let Some(why) = unusable(var, value, Path::new(value).is_dir()) {
        panic!("{why}");
    }
}

/// Why `value` cannot be used for `var`, or `None` when it can.
///
/// Split out from `material` so the message can be tested without setting a
/// process-global variable under a parallel runner.
fn unusable(var: &str, value: &str, is_dir: bool) -> Option<String> {
    if Path::new(value).is_absolute() && is_dir {
        return None;
    }
    Some(format!(
        "{var}={value} is not an absolute path to an existing directory. cargo runs \
         each package's tests from that package's own directory, so a relative path \
         resolves under crates/taiyaku-cli/ and finds nothing. Pass an absolute path."
    ))
}

/// The first line of every evidence file: the test, the time and the commit,
/// so a file left from an earlier run shows itself.
fn stamp(test: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let rev = git(&["rev-parse", "--short", "HEAD"]);
    let module = ["src/match_experiment", "src/match_experiment.rs"];
    let status = git(&[&["status", "--porcelain", "--"][..], &module].concat());
    let head = head(rev, status);
    format!("# {test} {} HEAD {head}", utc(now))
}

/// `git` run in this package's directory: what it printed, or `None` when
/// it failed.
fn git(args: &[&str]) -> Option<String> {
    (std::process::Command::new("git"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
}

/// The commit a stamp names: `+dirty` when the module has uncommitted
/// changes, since then the commit is not the code that ran, and `unknown`
/// when git could not say.
fn head(rev: Option<String>, status: Option<String>) -> String {
    let rev = rev.map(|r| r.trim().to_owned()).filter(|r| !r.is_empty());
    match (rev, status) {
        (Some(hash), Some(status)) if status.trim().is_empty() => hash,
        (Some(hash), Some(_)) => format!("{hash}+dirty"),
        _ => "unknown".to_owned(),
    }
}

/// Seconds since the epoch as `YYYY-MM-DDTHH:MM:SSZ`.
///
/// The crate has no date library and this is its one use, so the civil date
/// is computed here: days are counted from 1 March of year zero so that a
/// leap day falls at the end of its year, and every quantity stays unsigned
/// because nothing dated before 1970 is ever stamped.
fn utc(secs: u64) -> String {
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}

/// Drafts the manifest from names alone into `$EV/manifest-draft.json`, and
/// prints each table directory's member count and every name it could not
/// place, to be read against `ls`.
#[test]
fn draft_manifest() {
    #[derive(serde::Serialize)]
    struct Stamped<'a> {
        stamp: String,
        #[serde(flatten)]
        manifest: &'a manifest::Manifest,
    }

    let Some(material) = material() else {
        eprintln!("TAIYAKU_MATCH_LIBRARY unset; skipping");
        return;
    };
    let draft = manifest::draft(&material.library);
    for (dir, n) in &draft.counts {
        println!("{n:5}  {dir}");
    }
    let omnibus: Vec<String> = (draft.manifest.members.iter())
        .filter(|m| manifest::is_omnibus_name(&m.path) != (m.kind == manifest::Kind::Omnibus))
        .map(|m| m.path.clone())
        .collect();
    for (what, list) in [
        ("unclassified", &draft.unclassified),
        ("skipped", &draft.skipped),
        ("not in the table", &draft.untabled),
        ("omnibus by name or by row, not both", &omnibus),
    ] {
        println!("{what}: {}", list.len());
        for line in list {
            println!("  {line}");
        }
    }
    assert!(
        draft.missing.is_empty(),
        "table directories not found: {:?}",
        draft.missing
    );
    assert!(
        !draft.manifest.members.is_empty(),
        "no members under {}",
        material.library.display()
    );

    let stamped = Stamped {
        stamp: stamp("draft_manifest"),
        manifest: &draft.manifest,
    };
    let out = material.evidence.join("manifest-draft.json");
    let json = serde_json::to_string_pretty(&stamped).expect("a manifest serialises");
    std::fs::write(&out, json + "\n").unwrap_or_else(|e| panic!("{}: {e}", out.display()));
    println!(
        "{} members to {}",
        draft.manifest.members.len(),
        out.display()
    );
}

/// The ground truth: the draft with the owner-confirmed transcription
/// applied, written to the committed `manifest.json` that
/// `the_manifest_is_consistent` checks ([`rebuild_manifest`]).
#[test]
fn write_manifest() {
    let Some(material) = material() else {
        return;
    };
    let ev = &material.evidence;
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/match_experiment/manifest.json");
    let truth = ev.join("truth/transcription.json");
    match rebuild_manifest(&ev.join("manifest-draft.json"), &truth, &out) {
        Ok(members) => println!("{members} members to {}", out.display()),
        Err(why) => panic!("{why}"),
    }
}

/// Writes `draft` with `transcription` applied to `out`, refusing while `out`
/// holds any adjudication. The owner's verdicts are kept in `out` alone, as
/// neither input carries them, so a rebuild over them would drop every one
/// and silently relabel the pairs they settle. Over a manifest with none, the
/// same two inputs write the same bytes.
fn rebuild_manifest(draft: &Path, transcription: &Path, out: &Path) -> Result<usize, String> {
    #[derive(serde::Serialize, serde::Deserialize)]
    struct Stamped {
        stamp: String,
        #[serde(flatten)]
        manifest: manifest::Manifest,
    }

    let read =
        |file: &Path| std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()));
    match std::fs::read_to_string(out) {
        Ok(text) => {
            let held: manifest::Manifest =
                serde_json::from_str(&text).map_err(|e| format!("{}: {e}", out.display()))?;
            let n = held.adjudications.len();
            if n > 0 {
                let noun = if n == 1 {
                    "adjudication"
                } else {
                    "adjudications"
                };
                return Err(format!(
                    "{}: holds {n} {noun}, the owner's verdicts, which neither the draft nor the \
                     transcription carries, so a rebuild would drop them; nothing is written",
                    out.display()
                ));
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("{}: {e}", out.display())),
    }
    let mut draft: Stamped =
        serde_json::from_str(&read(draft)?).map_err(|e| format!("the draft: {e}"))?;
    let text = read(transcription)?;
    let parsed: manifest::Transcription =
        serde_json::from_str(&text).map_err(|e| format!("the transcription: {e}"))?;
    let raw: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("the transcription: {e}"))?;
    let Some(owner) = raw["owner"]["date"].as_str() else {
        return Err(format!(
            "{}: no owner confirmation",
            transcription.display()
        ));
    };
    manifest::apply_truth(&mut draft.manifest, &parsed, manifest::RENUMBERED)?;
    draft.manifest.members.sort_by(|a, b| a.path.cmp(&b.path));

    let stamped = Stamped {
        stamp: format!(
            "# write_manifest from manifest-draft.json ({}) and truth/transcription.json ({}), \
             owner-confirmed {owner}",
            draft.stamp,
            raw["stamp"].as_str().unwrap_or("unstamped")
        ),
        manifest: draft.manifest,
    };
    let json = serde_json::to_string_pretty(&stamped).expect("a manifest serialises");
    std::fs::write(out, json + "\n").map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(stamped.manifest.members.len())
}

/// Task 3's truth: the first pages of each of `sheet::VOLUMES` for someone
/// to transcribe into `truth/transcription.json`, or, once that file exists,
/// the sheet the owner checks the transcription against.
#[test]
fn truth_sheet() {
    let Some(material) = material() else {
        return;
    };
    let truth = material.evidence.join("truth");
    let transcription = truth.join("transcription.json");
    let done = if transcription.is_file() {
        sheet::truth_render(&material.library, &truth, &transcription)
    } else {
        sheet::truth_extract(&material.library, &truth, sheet::VOLUMES, 12)
    };
    if let Err(e) = done {
        panic!("{e}");
    }
}

/// Writes `text` to `path`, creating its directory.
fn put(path: &Path, text: &str) {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    }
    std::fs::write(path, text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}

/// A run's own log: created with the stamp as its first line, then every
/// line both printed and appended, so the file opens with the stamp whatever
/// cargo printed before the test began.
#[derive(Debug)]
struct Log(std::fs::File);

impl Log {
    fn create(path: &Path, stamp: &str) -> Self {
        put(path, &format!("{stamp}\n"));
        println!("{stamp}");
        let file = (std::fs::OpenOptions::new().append(true))
            .open(path)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        Log(file)
    }

    fn line(&mut self, text: &str) {
        use std::io::Write as _;
        println!("{text}");
        writeln!(self.0, "{text}").expect("the run's log takes a line");
    }
}

/// A run's rows must be the manifest's members in its order, and at least
/// one must have printed: a run that read nothing has measured nothing.
fn check_rows(rows: &[lookup::Row], m: &manifest::Manifest) -> Result<(), String> {
    let listed: Vec<&str> = rows.iter().map(|r| r.path.as_str()).collect();
    let members: Vec<&str> = m.members.iter().map(|x| x.path.as_str()).collect();
    if listed != members {
        return Err(format!(
            "members.tsv's {} rows are not the manifest's {} members in order",
            listed.len(),
            members.len()
        ));
    }
    if rows.iter().all(|r| r.error.is_some()) {
        return Err("no member was fingerprinted".to_owned());
    }
    Ok(())
}

/// Fingerprints every member of `m` under `library` through `ev/fp-cache`
/// and writes `members.tsv`; then every member's slots vote over an index of
/// all the others, written to `leaders.json`, `lookup.txt` and
/// `dev/lookup.txt`. The log is created before the first member is read.
fn run(library: &Path, ev: &Path, m: &manifest::Manifest) -> Result<(), String> {
    let stamp = stamp("fingerprint_and_lookup");
    let mut log = Log::create(&ev.join("logs/fingerprint.log"), &stamp);
    let started = std::time::Instant::now();
    let (prints, rows) = lookup::print_all(&ev.join("fp-cache"), library, m);
    let seconds = started.elapsed().as_secs_f64();
    put(&ev.join("members.tsv"), &lookup::members_tsv(&stamp, &rows));
    let printed = prints.iter().flatten().count();
    let hits = prints.iter().flatten().filter(|p| p.hits > 0).count();
    let unreadable = rows.len() - printed;
    log.line(&format!(
        "members: {}, fingerprinted: {printed}, unreadable: {unreadable} ({}), cache hits: {hits}",
        rows.len(),
        lookup::share(unreadable, rows.len()),
    ));
    log.line(&format!("fingerprinting took {seconds:.1} s"));
    for line in rows.iter().filter_map(lookup::unreadable_line) {
        log.line(&line);
    }
    check_rows(&rows, m)?;

    let (lookups, cost) = lookup::lookup_all(&prints);
    put(
        &ev.join("leaders.json"),
        &lookup::leaders_json(&stamp, &m.members, &lookups),
    );
    put(&ev.join("lookup.txt"), &lookup::lookup_txt(&stamp, &cost));
    log.line(&format!(
        "slots indexed: {}, comparisons: {}, wall seconds: {:.3}, threads: {}",
        cost.indexed,
        cost.comparisons,
        cost.wall.as_secs_f64(),
        cost.threads
    ));
    let dev = lookup::query_report(&stamp, manifest::Split::Dev, m, &prints, &lookups);
    put(&ev.join("dev/lookup.txt"), &dev);
    for line in dev
        .lines()
        .skip_while(|line| !line.starts_with("queries: "))
    {
        log.line(line);
    }
    Ok(())
}

/// The measured library's [`run`], over `manifest.json`.
#[test]
fn fingerprint_and_lookup() {
    let Some(material) = material() else {
        eprintln!("TAIYAKU_MATCH_LIBRARY unset; skipping");
        return;
    };
    let m: manifest::Manifest =
        serde_json::from_str(include_str!("match_experiment/manifest.json"))
            .expect("manifest.json parses");
    if let Err(why) = run(&material.library, &material.evidence, &m) {
        panic!("{why}");
    }
}

/// The dev split scored ([`score::dev`]): `dev/scores.tsv`, `dev/rules.txt`
/// and `dev/adjudicate-N.json`, from `leaders.json` and `fp-cache`. Panics,
/// judging nothing, when a control fails.
#[test]
fn score_dev() {
    let Some(material) = material() else {
        eprintln!("TAIYAKU_MATCH_LIBRARY unset; skipping");
        return;
    };
    let m: manifest::Manifest =
        serde_json::from_str(include_str!("match_experiment/manifest.json"))
            .expect("manifest.json parses");
    match score::dev(&material.library, &material.evidence, &m) {
        Ok(line) => println!("{line}"),
        Err(why) => panic!("{why}"),
    }
}

/// The newest `dev/adjudicate-N.json` set out as `dev/adjudicate-N.html`
/// for the owner to mark ([`score::render`]).
#[test]
fn adjudication_sheet() {
    let Some(material) = material() else {
        eprintln!("TAIYAKU_MATCH_LIBRARY unset; skipping");
        return;
    };
    match score::render(&material.library, &material.evidence.join("dev")) {
        Ok(Some(sheet)) => println!("the sheet: {}", sheet.display()),
        Ok(None) => println!("no pair to judge"),
        Err(why) => panic!("{why}"),
    }
}

/// The held-out split scored against the bars `bars.json` froze
/// ([`held_out::run`]), once the bars compiled in are found to be the ones
/// HEAD commits and one commit the only one touching them.
/// `logs/score-held-out.log` opens with the run's stamp, that commit and the
/// count, which dates the one against the other.
#[test]
fn score_held_out() {
    let Some(material) = material() else {
        eprintln!("TAIYAKU_MATCH_LIBRARY unset; skipping");
        return;
    };
    let compiled = include_str!("match_experiment/bars.json");
    let file = "src/match_experiment/bars.json";
    let committed = git(&["show", &format!("HEAD:./{file}")]);
    if let Err(why) = held_out::frozen(compiled, committed.as_deref()) {
        panic!("{why}");
    }
    let bars = held_out::read_bars(compiled).unwrap_or_else(|e| panic!("{e}"));
    let commits = git(&["log", "--format=%H", "--", file]);
    let touched = held_out::one_freeze(commits.as_deref()).unwrap_or_else(|e| panic!("{e}"));
    let commit = git(&["log", "-1", "--format=%h %ct", "--", file]);
    let provenance = format!("{}\n{touched}", held_out::provenance(commit.as_deref()));
    let m: manifest::Manifest =
        serde_json::from_str(include_str!("match_experiment/manifest.json"))
            .expect("manifest.json parses");
    match held_out::run(
        &material.library,
        &material.evidence,
        &m,
        &bars,
        &provenance,
    ) {
        Ok(line) => println!("{line}"),
        Err(why) => panic!("{why}"),
    }
}

/// The newest `held-out/adjudicate-N.json` set out as
/// `held-out/adjudicate-N.html` for the owner to mark ([`score::render`]).
#[test]
fn held_out_adjudication_sheet() {
    let Some(material) = material() else {
        eprintln!("TAIYAKU_MATCH_LIBRARY unset; skipping");
        return;
    };
    match score::render(&material.library, &material.evidence.join("held-out")) {
        Ok(Some(sheet)) => println!("the sheet: {}", sheet.display()),
        Ok(None) => println!("no pair to judge"),
        Err(why) => panic!("{why}"),
    }
}

/// A fixed-seed xorshift, so every synthetic hash is the same on every run.
#[derive(Debug)]
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn hash(&mut self) -> PageHash {
        PageHash([self.next(), self.next(), self.next(), self.next()])
    }
}

/// The NAS sample, `class<TAB>path` a line, each path under the root
/// `nas_timing` reads. Compiled in, so the binary carries it to the NAS.
const SAMPLE: &str = include_str!("match_experiment/nas_sample.txt");

/// Large enough that thread start-up is noise beside the scan: the
/// extrapolation uses the rate, not the total.
const KERNEL_QUERIES: usize = 5_000;
const KERNEL_STORED: usize = 2_000_000;
/// Any nonzero seed: a xorshift started at zero stays at zero.
const KERNEL_SEED: u64 = 0x6e61_735f_7469_6d65;

/// Each sample line as `(class, path)`. A line with no tab panics: dropped,
/// it would shrink the base the four-fifths bar is taken over.
fn sample(text: &str) -> Vec<(&str, &str)> {
    (text.lines().filter(|line| !line.trim().is_empty()))
        .map(|line| {
            (line.split_once('\t')).unwrap_or_else(|| panic!("nas_sample.txt: no tab in {line:?}"))
        })
        .collect()
}

/// `TAIYAKU_MATCH_NAS_ROOT`'s value as the root to read, `None` when unset;
/// set but unusable panics, as `material` rules.
fn nas_root(value: Option<String>) -> Option<PathBuf> {
    let root = value?;
    refuse_unusable("TAIYAKU_MATCH_NAS_ROOT", &root);
    Some(PathBuf::from(root))
}

/// Whether `done` of `total` reaches the four fifths of the sample the run
/// must fingerprint for its rates to stand for their classes.
fn enough(done: usize, total: usize) -> bool {
    total > 0 && 5 * done >= 4 * total
}

/// `/proc/mounts` writes a space in a mount point as `\040`, and a tab,
/// newline or backslash likewise, as three octal digits.
fn unescape(field: &str) -> String {
    let mut out = String::new();
    let mut rest = field;
    while let Some(at) = rest.find('\\') {
        out.push_str(&rest[..at]);
        let code = (rest.get(at + 1..at + 4)).and_then(|o| u8::from_str_radix(o, 8).ok());
        let (text, skip) = code.map_or(('\\', 1), |byte| (char::from(byte), 4));
        out.push(text);
        rest = &rest[at + skip..];
    }
    out + rest
}

/// The `/proc/mounts` line of the filesystem `root` is on: the deepest mount
/// point holding it, and of two at one point the later, which hides the
/// earlier.
fn mount_of<'m>(mounts: &'m str, root: &Path) -> Option<&'m str> {
    let mut best: Option<(usize, &str)> = None;
    for line in mounts.lines() {
        let Some(point) = line.split(' ').nth(1).map(unescape) else {
            continue;
        };
        let depth = Path::new(&point).components().count();
        if root.starts_with(&point) && best.is_none_or(|(deepest, _)| depth >= deepest) {
            best = Some((depth, line));
        }
    }
    best.map(|(_, line)| line)
}

/// Whether a mount line's options hold `ro` itself, not inside another
/// option such as `errors=remount-ro`.
fn read_only(line: &str) -> bool {
    (line.split(' ').nth(3)).is_some_and(|options| options.split(',').any(|o| o == "ro"))
}

/// The 1-, 5- and 15-minute loads, `-` where `/proc/loadavg` cannot be read.
fn loadavg() -> String {
    std::fs::read_to_string("/proc/loadavg").map_or_else(
        |_| "-".to_owned(),
        |text| {
            text.split_whitespace()
                .take(3)
                .collect::<Vec<_>>()
                .join(" ")
        },
    )
}

/// What the run's figures depend on: the cores and threads, the load, and
/// the mount the root is on, which on the NAS must be read-only.
fn header(root: &Path) -> String {
    let parallelism = (std::thread::available_parallelism())
        .map_or_else(|e| format!("unknown ({e})"), |n| n.to_string());
    let proc = |name: &str| std::fs::read_to_string(name).unwrap_or_default();
    let canonical = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let mounts = proc("/proc/mounts");
    let mount = mount_of(&mounts, &canonical);
    format!(
        "available parallelism\t{parallelism}\nrayon threads\t{}\nloadavg\t{}\n\
         mount\t{}\nread-only\t{}",
        rayon::current_num_threads(),
        proc("/proc/loadavg").trim(),
        mount.unwrap_or("none found"),
        if mount.is_some_and(read_only) {
            "yes"
        } else {
            "no"
        },
    )
}

/// What one sample entry cost, phase by phase, summed over its paths.
#[derive(Debug, Default)]
struct Timing {
    bytes: u64,
    pages: usize,
    digest: Duration,
    read: Duration,
    describe: Duration,
}

/// Times an entry as `cached_edition` first meets each of its paths: the
/// digest is the cold read, the re-read unpacks the pages again from the
/// page cache, and the description decodes and hashes them. `None` when
/// nothing is at `full`.
fn time_entry(full: &Path) -> Result<Option<Timing>, String> {
    if !full.exists() {
        return Ok(None);
    }
    let mut t = Timing::default();
    for path in &lookup::edition_paths(full)? {
        let started = Instant::now();
        taiyaku_read::member_digest(path)?;
        let digested = Instant::now();
        let pages = taiyaku_read::read_edition(std::slice::from_ref(path))?;
        let read = Instant::now();
        t.pages += crate::fingerprint::describe_pages(&pages)?.len();
        t.describe += read.elapsed();
        t.read += read - digested;
        t.digest += digested - started;
        t.bytes += lookup::size(path)?;
    }
    Ok(Some(t))
}

const TIMING_HEADER: &str =
    "class\tpath\tbytes\tpages\tdigest ms\tre-read ms\tfingerprint ms\tloadavg after";

fn timing_line(class: &str, path: &str, t: &Timing, load: &str) -> String {
    let ms = |d: Duration| format!("{:.1}", d.as_secs_f64() * 1e3);
    let (digest, read, describe) = (ms(t.digest), ms(t.read), ms(t.describe));
    let (bytes, pages) = (t.bytes, t.pages);
    format!("{class}\t{path}\t{bytes}\t{pages}\t{digest}\t{read}\t{describe}\t{load}")
}

/// Times every entry under `root`, emitting each line as its entry finishes
/// so a long run shows where it is, and returns how many were fingerprinted.
/// A missing or unreadable entry is a line of its own, and the run goes on.
fn time_sample(root: &Path, entries: &[(&str, &str)], emit: &mut dyn FnMut(String)) -> usize {
    emit(TIMING_HEADER.to_owned());
    let mut done = 0;
    for &(class, path) in entries {
        emit(match time_entry(&root.join(path)) {
            Ok(Some(t)) => {
                done += 1;
                timing_line(class, path, &t, &loadavg())
            }
            Ok(None) => format!("MISSING {path}"),
            Err(e) => format!("UNREADABLE {path}: {}", lookup::cell(&e)),
        });
    }
    done
}

/// The vote kernel over random hashes from a fixed seed: a comparison costs
/// the same whatever the two hashes hold, so random ones time it.
fn kernel_lines(queries: usize, stored: usize) -> String {
    let mut rng = Rng(KERNEL_SEED);
    let asked: Vec<PageHash> = (0..queries).map(|_| rng.hash()).collect();
    let held: Vec<PageHash> = (0..stored).map(|_| rng.hash()).collect();
    let (comparisons, wall) = lookup::kernel(&asked, &held);
    format!(
        "kernel queries\t{queries}\nkernel stored\t{stored}\nkernel comparisons\t{comparisons}\n\
         kernel seconds\t{:.3}\nkernel comparisons per second\t{}",
        wall.as_secs_f64(),
        lookup::per_second(comparisons, wall)
    )
}

/// Times the NAS sample under `TAIYAKU_MATCH_NAS_ROOT`, then the vote
/// kernel, for the extrapolation `docs/notes.md` registers under "The cost
/// on the NAS".
#[test]
fn nas_timing() {
    let Some(root) = nas_root(std::env::var("TAIYAKU_MATCH_NAS_ROOT").ok()) else {
        eprintln!("TAIYAKU_MATCH_NAS_ROOT unset; skipping");
        return;
    };
    let entries = sample(SAMPLE);
    println!("{}\n{}", stamp("nas_timing"), header(&root));
    let done = time_sample(&root, &entries, &mut |line| println!("{line}"));
    println!("{}", kernel_lines(KERNEL_QUERIES, KERNEL_STORED));
    let total = entries.len();
    let share = lookup::share(done, total);
    println!("present and fingerprinted\t{done} of {total} ({share})");
    assert!(
        enough(done, total),
        "{done} of the sample's {total} entries were present and fingerprinted; \
         the run needs four fifths"
    );
}
