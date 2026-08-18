//! Whether this root can be watched, answered by experiment.
//!
//! **Not by filesystem type.** No fstype table can know about an Unraid user
//! share (FUSE `shfs`), a Docker bind mount or `overlayfs`, and a table of
//! special cases would be wrong for the next deployment. So the scanner writes
//! a temporary file into the root, watches for the event, and records whether
//! it arrived — one experiment that is correct for this deployment, for the
//! NFS case other people will have, and for filesystems nobody has considered.
//!
//! **This is the one place the scanner writes to the library tree** (§10). The
//! file is dot-prefixed, so `scan.rs`'s walk skips it even in the window
//! before it is removed, and it is removed on every path out of `probe`
//! including the timeout.
//!
//! **A measurement of this workstation, and not of the server.** The store
//! spec's §10 originally reasoned that a watcher delivers nothing over NFS and
//! that the interval must therefore be short. The NFS observation is correct
//! *of this workstation*, which mounts the library over nfs4; the server will
//! run **on** the NAS, where the same tree is local. The conclusion was about
//! the wrong host, and the probe replaces the prediction entirely.

use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use notify::{Event, RecursiveMode, Watcher};

/// Whether `probe` saw its own file arrive, and why not when it did not.
///
/// Two fields, deliberately no third for "partially watched" — a recursive
/// watch either sees the one event this experiment asks for or it does not,
/// and the watch-count ceiling that can make a *real* recursive watch partial
/// is a fact about `dirs`, not about this one-file probe, so it is reported
/// by [`report_line`] rather than folded in here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    pub watching: bool,
    pub reason: Option<String>,
}

/// Long where watching works: a safety net for events missed while the server
/// was down, not the mechanism.
const WATCHED_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// Short where it does not, because then it is the only mechanism.
const UNWATCHED_INTERVAL: Duration = Duration::from_secs(15 * 60);

/// Ask by experiment whether `root` can be watched, rather than guessing from
/// its filesystem type.
///
/// Writes `.taiyaku-watch-probe-<pid>` into `root` — dot-prefixed, so
/// `scan.rs`'s walk skips it even in the window before it is removed — starts
/// a `notify` recommended watcher on `root` non-recursively, and waits up to
/// `timeout` for an event naming that exact path. The file is removed on
/// every path out of this function, including the timeout, because §10 makes
/// "the scanner writes nothing to the library" the rule this is the one
/// exception to, and an exception that leaves litter behind is not one.
///
/// **A write that fails is not an error return.** A read-only root, a probe
/// path already occupied — a leftover from a crashed run, or a symlink
/// planted by another account on a shared media tree — or a watcher that
/// cannot be started, all come back `watching: false` with the OS error as
/// `reason`, because a root that cannot be probed is a root that gets
/// scanned on the short interval, not a scan that refuses to run at all.
///
/// No `#[must_use]` here, unlike [`interval_for`] and [`report_line`]:
/// `the_probe_removes_the_file_it_wrote` (this module's own test, and the
/// brief's) calls this only for the side effect and checks the directory
/// afterward rather than the `Probe` — clippy's pedantic suggestion is
/// advisory (`check-all.sh`, "The pedantic lints, and why they are
/// advisory") and this is the one call site where taking it would fight a
/// legitimate caller rather than catch a real bug.
pub fn probe(root: &Path, timeout: Duration) -> Probe {
    let probe_path = root.join(format!(".taiyaku-watch-probe-{}", std::process::id()));

    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = match notify::recommended_watcher(tx) {
        Ok(watcher) => watcher,
        Err(e) => {
            return Probe {
                watching: false,
                reason: Some(format!("starting a watcher: {e}")),
            };
        }
    };
    // **`NonRecursive`, and the choice is about cost rather than the
    // answer.** The probe writes one file directly into `root` and asks
    // whether one event about it arrives; a recursive watch would deliver
    // that same event and give the same answer, so no test in this workspace
    // can tell the two apart — `probe`'s return value is identical either
    // way. What differs is that recursive costs one inotify watch per
    // directory beneath `root` to answer a yes/no question about one file,
    // against a whole-host budget (`report_line`'s own ceiling), so on a
    // large enough root the recursive version fails to start at all and
    // reports `watching: false` about a filesystem that watches fine. That
    // is the failure this line avoids, and it is not reachable from a
    // fixture.
    if let Err(e) = watcher.watch(root, RecursiveMode::NonRecursive) {
        return Probe {
            watching: false,
            reason: Some(format!("watching {}: {e}", root.display())),
        };
    }

    // The watcher is armed before the file is written, or the create event
    // could fire before anything is listening for it.
    //
    // **`create_new`, not `fs::write`.** This maps to `O_CREAT | O_EXCL`,
    // which does not follow symlinks and fails outright if anything already
    // exists at the path. `fs::write` follows a symlink and truncates its
    // target, and the pid in the name is guessable — so on a share other
    // accounts can write to, the scanner's one and only write becomes a
    // write to any file on the host.
    //
    // A leftover from a crashed run therefore reports rather than being
    // clobbered: the root falls back to the short interval and the message
    // names the path, so a person can remove it. Silently overwriting
    // whatever is there is what this is avoiding, and a stale probe file is
    // precisely the case where "whatever is there" is least predictable.
    let probe_file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe_path);
    if let Err(e) = probe_file {
        return Probe {
            watching: false,
            reason: Some(format!(
                "creating the probe file at {}: {e}",
                probe_path.display()
            )),
        };
    }

    let deadline = Instant::now() + timeout;
    let mut seen = false;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match rx.recv_timeout(remaining) {
            Ok(Ok(event)) if names_the_probe(&event, &probe_path) => {
                seen = true;
                break;
            }
            // Some other path under `root`, or a watcher-internal error:
            // neither answers the question, so keep waiting out the deadline.
            Ok(_) => {}
            // Timed out, or every sender dropped — either way, nothing more
            // is coming.
            Err(_) => break,
        }
    }

    // Removed on every path out from here down — found, timed out, or a
    // watcher error mid-wait — because this is the one write the scanner
    // makes and it must not be the one thing that outlives the scan.
    let _ = std::fs::remove_file(&probe_path);

    if seen {
        Probe {
            watching: true,
            reason: None,
        }
    } else {
        Probe {
            watching: false,
            reason: Some(format!("no event arrived within {timeout:?}")),
        }
    }
}

/// Whether `event` is the arrival of the probe's **own** file.
///
/// **The question is "did my file arrive", not "did anything happen".** A
/// root's directory holds other things and other processes write to them, so
/// treating any event as the answer makes the experiment report success for a
/// filesystem where the probe's own event never came — and a root that then
/// takes the 24-hour interval on a watcher that delivers nothing misses a
/// day of changes rather than fifteen minutes of them, which is the one
/// outcome §8's whole design is arranged against.
///
/// Split out from the `match` guard because the loop around it cannot be
/// driven to the failing case from a fixture: `probe`'s own file is the only
/// thing that reliably arrives in a tempdir, so a test of `probe` sees
/// "correct" and "any event will do" answer identically. The same split
/// [`report_line_against`] makes below, for the same reason.
fn names_the_probe(event: &Event, probe_path: &Path) -> bool {
    event.paths.iter().any(|p| p == probe_path)
}

/// The scan interval `probe`'s answer implies: long where a watcher covers
/// the gap, short where nothing else does.
#[must_use]
pub fn interval_for(probe: &Probe) -> Duration {
    if probe.watching {
        WATCHED_INTERVAL
    } else {
        UNWATCHED_INTERVAL
    }
}

/// One line naming what this root can actually do — §8's exact shape,
/// reported per root and never assumed. The strings below are pasted in
/// rather than paraphrased, produced from exactly these two calls:
///
/// ```text
/// let watching = Probe { watching: true, reason: None };
/// report_line(Path::new("/srv/manga"), &watching, 156, interval_for(&watching))
///     => "root /srv/manga watching 156 directories — scanning every 24h"
///
/// let not = Probe { watching: false, reason: Some("no event arrived within 2s".into()) };
/// report_line(Path::new("/mnt/remote/manga"), &not, 12, interval_for(&not))
///     => "root /mnt/remote/manga watch probe failed (no event arrived within 2s) — scanning every 15m"
/// ```
///
/// **The watch-count ceiling is reported here, never silently degraded.** A
/// recursive watch costs one inotify watch per directory, and a root whose
/// directory count exceeds `/proc/sys/fs/inotify/max_user_watches` cannot
/// actually be watched in full even though `probe` succeeded — a
/// partially-watched root is the worst of the three states, since it looks
/// like a fully-watched one and behaves like neither. Where that file exists
/// and `dirs` exceeds it, the shortfall is named instead of a bare directory
/// count.
#[must_use]
pub fn report_line(root: &Path, probe: &Probe, dirs: usize, interval: Duration) -> String {
    report_line_against(root, probe, dirs, interval, read_watch_ceiling())
}

/// `report_line`, with the watch ceiling passed in rather than read from
/// `/proc`.
///
/// Split out for the same reason `require_fts5_with_probe` is split from
/// `require_fts5` in `lib.rs`: this machine allows 524,288 watches (measured
/// 2026-08-14), so no test running here can drive the real
/// `/proc/sys/fs/inotify/max_user_watches` past a directory count anyone
/// would put in a fixture. Parameterising the ceiling lets a test hand in one
/// low enough to exercise the branch without needing a host that actually has
/// a lower one.
fn report_line_against(
    root: &Path,
    probe: &Probe,
    dirs: usize,
    interval: Duration,
    ceiling: Option<usize>,
) -> String {
    let scanning = format!("scanning every {}", fmt_interval(interval));
    if !probe.watching {
        let reason = probe.reason.as_deref().unwrap_or("no reason given");
        return format!(
            "root {} watch probe failed ({reason}) — {scanning}",
            root.display()
        );
    }
    match ceiling {
        Some(ceiling) if dirs > ceiling => format!(
            "root {} watching only {ceiling} of {dirs} directories — {} short of \
             /proc/sys/fs/inotify/max_user_watches — {scanning}",
            root.display(),
            dirs - ceiling
        ),
        _ => format!(
            "root {} watching {dirs} directories — {scanning}",
            root.display()
        ),
    }
}

/// This host's inotify watch ceiling, or `None` where the file does not exist
/// — not every kernel exposes it, and that must not read as a ceiling of
/// zero.
fn read_watch_ceiling() -> Option<usize> {
    std::fs::read_to_string("/proc/sys/fs/inotify/max_user_watches")
        .ok()
        .and_then(|contents| contents.trim().parse().ok())
}

/// `24h`, `15m`, `90s` — whichever unit divides `interval` evenly, since the
/// only durations this ever actually formats are [`WATCHED_INTERVAL`] and
/// [`UNWATCHED_INTERVAL`].
fn fmt_interval(interval: Duration) -> String {
    let secs = interval.as_secs();
    if secs > 0 && secs % 3600 == 0 {
        format!("{}h", secs / 3600)
    } else if secs > 0 && secs % 60 == 0 {
        format!("{}m", secs / 60)
    } else {
        format!("{secs}s")
    }
}

#[cfg(test)]
mod tests;
