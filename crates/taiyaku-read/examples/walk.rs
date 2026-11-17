//! A walk-only bench: how long a scan takes to list a library's files and
//! count its archives' pages, with no server and no store.
//!
//! ```text
//! cargo run --release -p taiyaku-read --example walk -- walk <dir> [--stride N --offset K]
//! cargo run --release -p taiyaku-read --example walk -- guard <dir>
//! ```
//!
//! `walk` times the scan's list and resolve steps over the top-level
//! directories of `<dir>` whose sorted index `i % N == K`, and prints one line.
//! `guard` counts the page entries a listing would drop because `by_index`
//! cannot read their local header, and any other way the listing differs from
//! the central directory's page entries.
//!
//! Every file is opened read-only and nothing is written, so both run against
//! a real library as it stands. Only the crate's public listing functions and
//! `zip` are used, so one source times the walk before and after a change to
//! how pages are listed.

use std::collections::BTreeSet;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use taiyaku_read::{Format, is_page_name, member_page_names, member_pages, sniff};

const USAGE: &str = "usage: walk walk <dir> [--stride N --offset K] | walk guard <dir>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let outcome = match args.as_slice() {
        [mode, dir, rest @ ..] if mode == "walk" => {
            stride_of(rest).and_then(|(stride, offset)| walk(Path::new(dir), stride, offset))
        }
        [mode, dir] if mode == "guard" => guard(Path::new(dir)),
        _ => Err(USAGE.to_owned()),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("walk: {message}");
            ExitCode::FAILURE
        }
    }
}

fn stride_of(rest: &[String]) -> Result<(usize, usize), String> {
    let (mut stride, mut offset) = (1, 0);
    let mut flags = rest.iter();
    while let Some(flag) = flags.next() {
        let value = flags.next().ok_or_else(|| USAGE.to_owned())?;
        let n = value.parse().map_err(|e| format!("{flag} {value}: {e}"))?;
        match flag.as_str() {
            "--stride" => stride = n,
            "--offset" => offset = n,
            _ => return Err(USAGE.to_owned()),
        }
    }
    if offset >= stride {
        return Err(format!("--offset {offset} must be below --stride {stride}"));
    }
    Ok((stride, offset))
}

fn walk(dir: &Path, stride: usize, offset: usize) -> Result<(), String> {
    let listing = Instant::now();
    let mut series = Vec::new();
    for path in children(dir)? {
        if is_dir(&path)? {
            series.push(path);
        }
    }
    let mut files = Vec::new();
    for path in series.iter().skip(offset).step_by(stride) {
        list_files(path, &mut files)?;
    }
    let list_secs = listing.elapsed();

    let resolving = Instant::now();
    let mut pages = 0u64;
    let mut per_archive = Vec::new();
    for path in &files {
        let start = Instant::now();
        // `scan::format_of` then `pages_of`: only a zip has its pages counted.
        if matches!(sniff(path), Ok(Format::Zip)) {
            if let Ok(n) = member_pages(path) {
                pages += u64::from(n);
            }
            per_archive.push(start.elapsed());
        }
    }
    let resolve_secs = resolving.elapsed();

    per_archive.sort_unstable();
    let mean = u32::try_from(per_archive.len())
        .ok()
        .and_then(|n| per_archive.iter().sum::<Duration>().checked_div(n))
        .unwrap_or_default();
    println!(
        "files={} archives={} pages={pages} list_secs={:.2} resolve_secs={:.2} per_archive_ms mean={:.2} p50={:.2} p90={:.2} max={:.2}",
        files.len(),
        per_archive.len(),
        list_secs.as_secs_f64(),
        resolve_secs.as_secs_f64(),
        ms(mean),
        ms(rank(&per_archive, 50)),
        ms(rank(&per_archive, 90)),
        ms(rank(&per_archive, 100)),
    );
    Ok(())
}

fn guard(dir: &Path) -> Result<(), String> {
    let mut files = Vec::new();
    list_files(dir, &mut files)?;
    let (mut archives, mut pages, mut dropped, mut differ, mut unopened) = (0, 0, 0, 0, 0);
    for path in &files {
        if !matches!(sniff(path), Ok(Format::Zip)) {
            continue;
        }
        let opened = File::open(path)
            .map_err(|e| e.to_string())
            .and_then(|file| zip::ZipArchive::new(file).map_err(|e| e.to_string()));
        let mut zip = match opened {
            Ok(zip) => zip,
            Err(e) => {
                unopened += 1;
                println!("unopened {}: {e}", path.display());
                continue;
            }
        };
        archives += 1;
        let mut readable = BTreeSet::new();
        for i in 0..zip.len() {
            let Some(name) = page_name(&zip, i) else {
                continue;
            };
            pages += 1;
            match zip.by_index(i) {
                Ok(_) => {
                    readable.insert(name);
                }
                Err(e) => {
                    dropped += 1;
                    println!("dropped_by_local_header {}: {name}: {e}", path.display());
                }
            }
        }
        match member_page_names(path) {
            Ok(names) => {
                let listed: BTreeSet<String> = names.into_iter().collect();
                if listed != readable {
                    differ += 1;
                    println!(
                        "lists_differ_otherwise {}: listed only {:?}, readable only {:?}",
                        path.display(),
                        listed.difference(&readable).take(3).collect::<Vec<_>>(),
                        readable.difference(&listed).take(3).collect::<Vec<_>>(),
                    );
                }
            }
            Err(e) => {
                differ += 1;
                println!("lists_differ_otherwise {}: {e}", path.display());
            }
        }
    }
    println!(
        "files={} archives={archives} pages={pages} dropped_by_local_header={dropped} lists_differ_otherwise={differ} unopened={unopened}",
        files.len(),
    );
    Ok(())
}

/// The name of entry `i` if it is a page, read from the central directory
/// alone. The test is `page_names_from`'s, so a difference between this set
/// and `member_page_names` is the listing's doing and not this bench's.
fn page_name(zip: &zip::ZipArchive<File>, i: usize) -> Option<String> {
    let entry = zip.by_index_data(i).ok()?;
    if !entry.is_file() {
        return None;
    }
    let name = entry.name().ok()?.into_owned();
    if name.starts_with("__MACOSX") || name.rsplit('/').next()?.starts_with('.') {
        return None;
    }
    is_page_name(&name).then_some(name)
}

/// Every regular file beneath `dir`, found as the scan's list step finds
/// them: one `read_dir` per directory and a stat of each entry, with
/// symlinks skipped.
fn list_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for path in children(dir)? {
        let meta =
            std::fs::symlink_metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if meta.is_dir() {
            list_files(&path, files)?;
        } else if meta.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

fn children(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .and_then(|entries| entries.map(|entry| entry.map(|e| e.path())).collect())
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    paths.sort();
    Ok(paths)
}

fn is_dir(path: &Path) -> Result<bool, String> {
    std::fs::symlink_metadata(path)
        .map(|meta| meta.is_dir())
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// The nearest-rank percentile of an ascending list, zero for an empty one.
fn rank(sorted: &[Duration], percent: usize) -> Duration {
    let k = (sorted.len() * percent).div_ceil(100);
    k.checked_sub(1)
        .and_then(|i| sorted.get(i))
        .copied()
        .unwrap_or_default()
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}
