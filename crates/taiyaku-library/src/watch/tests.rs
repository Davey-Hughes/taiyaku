use super::*;
use std::path::Path;
use std::time::Duration;

#[test]
fn a_probe_on_an_ordinary_local_directory_sees_its_own_file() {
    // This machine's /tmp is not NFS, so the probe should succeed here. It is
    // a test of the mechanism, not of any deployment: §13 records that
    // whether it works on Unraid's shfs is unknown and cannot be measured
    // from this workstation.
    let dir = crate::tests::tempdir("probe");
    let probe = probe(&dir.path, Duration::from_secs(2));
    assert!(probe.watching, "{:?}", probe.reason);
}

#[test]
fn the_probe_removes_the_file_it_wrote() {
    // §10: the scanner does not write to the library. The probe is the one
    // exception, and it is only an exception while it cleans up after itself.
    let dir = crate::tests::tempdir("probe");
    probe(&dir.path, Duration::from_secs(2));
    let left: Vec<_> = std::fs::read_dir(&dir.path)
        .expect("read_dir")
        .filter_map(Result::ok)
        .collect();
    assert!(left.is_empty(), "the probe left {} entries", left.len());
}

/// The test above only ever runs against tmpfs with a live watcher, so
/// `seen` is always `true` when cleanup runs; the read-only and symlink
/// tests fail before `create_new` ever creates a file, so cleanup there is a
/// no-op with nothing to prove. Nothing pinned the case this exists for: a
/// probe file that *was* created and then timed out rather than being seen.
///
/// Security review's own check: moving `remove_file` inside `if seen` —
/// leaving a timed-out probe file behind — left every other test in this
/// module green. `Duration::ZERO` guarantees the timeout branch (the file is
/// written before the wait loop even starts, and the loop's first deadline
/// check is already past) without racing a real clock, closing property 1:
/// "removed on every path out ... including the timeout."
#[test]
fn the_probe_file_is_removed_even_on_an_immediate_timeout() {
    let dir = crate::tests::tempdir("probe-timeout");
    let probe = probe(&dir.path, Duration::ZERO);
    assert!(
        !probe.watching,
        "a zero timeout cannot see the event arrive"
    );
    let left: Vec<_> = std::fs::read_dir(&dir.path)
        .expect("read_dir")
        .filter_map(Result::ok)
        .collect();
    assert!(
        left.is_empty(),
        "the probe left {} entries on the timeout path",
        left.len()
    );
}

#[test]
fn a_root_that_cannot_be_written_falls_back_to_the_short_interval() {
    // **`chmod 0555` does not stop root**, which holds `CAP_DAC_OVERRIDE` —
    // and CI's `test` job runs `rust:1-bookworm` with no `user:`, doing
    // root-only work in its own steps. Reproduced under `unshare -Ur`: this
    // test FAILS as root, on a property the code gets right.
    //
    // So it measures whether the restriction actually took, and says so when
    // it did not. `taiyaku-serve/src/tests.rs`'s
    // `one_work_that_cannot_be_forgotten_does_not_stop_the_others` stages the
    // same thing the same way, for the same reason, and this follows it —
    // including printing `NOT MEASURED:` rather than passing quietly, which
    // is what `check-all.sh`'s own summary does when something is skipped.

    let dir = crate::tests::tempdir("probe");
    let mut perms = std::fs::metadata(&dir.path)
        .expect("metadata")
        .permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o555);
    std::fs::set_permissions(&dir.path, perms).expect("chmod");

    // The guard: if this process can still create a file here, the chmod
    // bought nothing and every assertion below would pass without measuring.
    let sentinel = dir.path.join(".taiyaku-permission-check");
    if std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&sentinel)
        .is_ok()
    {
        let _ = std::fs::remove_file(&sentinel);
        eprintln!(
            "NOT MEASURED: a_root_that_cannot_be_written_falls_back_to_the_short_interval (this process can write to a 0555 directory)"
        );
        return;
    }

    let probe = probe(&dir.path, Duration::from_secs(1));

    assert!(!probe.watching);
    assert!(probe.reason.is_some(), "a failure says why");
    assert_eq!(interval_for(&probe), Duration::from_secs(15 * 60));
}

#[test]
fn a_symlink_in_the_probes_place_is_refused_rather_than_followed() {
    // `fs::write` would follow this and truncate the target. The pid is
    // guessable and a media share may be writable by other accounts, so the
    // scanner's one write must not be a write to an arbitrary file.
    let dir = crate::tests::tempdir("probe-symlink");
    let victim = dir.path.join("important.json");
    std::fs::write(&victim, b"{\"keep\": true}").expect("write");
    let planted = dir
        .path
        .join(format!(".taiyaku-watch-probe-{}", std::process::id()));
    std::os::unix::fs::symlink(&victim, &planted).expect("symlink");

    let answer = probe(&dir.path, Duration::from_millis(200));

    assert!(!answer.watching);
    assert_eq!(
        std::fs::read(&victim).expect("victim survives"),
        b"{\"keep\": true}",
        "the probe followed a symlink and truncated its target"
    );
}

#[test]
fn the_interval_is_long_where_watching_works_and_short_where_it_does_not() {
    // §8: long where watching works — a safety net for events missed while
    // the server was down; short where it does not, because then it is the
    // only mechanism.
    let watching = Probe {
        watching: true,
        reason: None,
    };
    let not = Probe {
        watching: false,
        reason: Some("no event arrived".into()),
    };
    assert_eq!(interval_for(&watching), Duration::from_secs(24 * 60 * 60));
    assert_eq!(interval_for(&not), Duration::from_secs(15 * 60));
}

#[test]
fn the_report_names_what_this_root_can_actually_do() {
    // §8's exact shape. Reported per root, never assumed.
    let watching = Probe {
        watching: true,
        reason: None,
    };
    let line = report_line(
        Path::new("/srv/manga"),
        &watching,
        156,
        interval_for(&watching),
    );
    assert!(line.contains("/srv/manga"), "{line}");
    assert!(line.contains("watching 156 directories"), "{line}");
    assert!(line.contains("24h"), "{line}");

    let not = Probe {
        watching: false,
        reason: Some("no event arrived within 2s".into()),
    };
    let line = report_line(Path::new("/mnt/remote/manga"), &not, 12, interval_for(&not));
    assert!(line.contains("watch probe failed"), "{line}");
    assert!(line.contains("15m"), "{line}");
}

// --- the watch-count ceiling, not among the brief's five tests -------------
//
// §8: "a lower ceiling elsewhere must be reported... a partially-watched root
// is the worst of the three states, because it looks like the first and
// behaves like neither." `probe` only answers whether *one* event on *one*
// file arrived; it says nothing about whether a real recursive watch of every
// directory under a root fits under this host's inotify limit — that
// comparison is `report_line`'s, so it is `report_line`'s tests that have to
// pin it.
//
// This machine allows 524,288 watches (measured 2026-08-14 by the scanner
// spec, confirmed again here on 2026-08-17), which no fixture's directory
// count will ever exceed — the same shape `lib.rs` names for
// `require_fts5_with_probe`. `report_line_against` takes the ceiling as a
// parameter for exactly that reason: a test can hand in one low enough to
// reach the branch without a host that actually has a lower one.

#[test]
fn the_report_names_the_shortfall_when_the_directory_count_exceeds_the_watch_ceiling() {
    let watching = Probe {
        watching: true,
        reason: None,
    };
    let line = report_line_against(
        Path::new("/srv/manga"),
        &watching,
        200,
        interval_for(&watching),
        Some(150),
    );
    assert!(line.contains("150 of 200"), "{line}");
    assert!(
        line.contains("50"),
        "the shortfall itself, 200 - 150: {line}"
    );
    assert!(
        line.contains("max_user_watches"),
        "names where the ceiling comes from: {line}"
    );
    // The exact string, checked by running rather than assumed from reading
    // the source — `cargo fmt` is known to bake stripped indentation into a
    // `\`-continued string as literal spaces, and `.contains()` checks above
    // would not catch an extra space this rewrote in.
    assert_eq!(
        line,
        "root /srv/manga watching only 150 of 200 directories — 50 short of \
         /proc/sys/fs/inotify/max_user_watches — scanning every 24h"
    );
}

#[test]
fn the_report_stays_plain_when_the_directory_count_is_under_the_watch_ceiling() {
    let watching = Probe {
        watching: true,
        reason: None,
    };
    let line = report_line_against(
        Path::new("/srv/manga"),
        &watching,
        50,
        interval_for(&watching),
        Some(150),
    );
    assert!(line.contains("watching 50 directories"), "{line}");
    assert!(
        !line.contains("max_user_watches"),
        "no shortfall, nothing to name: {line}"
    );
}

/// An event about somebody else's file is not the answer to this
/// experiment.
///
/// `a_probe_on_an_ordinary_local_directory_sees_its_own_file` cannot see the
/// difference: its tempdir holds nothing but the probe file, so "the probe's
/// own event arrived" and "some event arrived" are the same event. Replacing
/// the predicate with `!event.paths.is_empty()` left every test in this
/// module green — and an NFS root that delivers unrelated traffic but not
/// this file would then take the 24-hour interval and miss a day of changes.
#[test]
fn only_the_probes_own_file_answers_the_question() {
    use notify::EventKind;
    use notify::event::CreateKind;

    let root = Path::new("/library");
    let probe_path = root.join(".taiyaku-watch-probe-1234");

    let mine = notify::Event::new(EventKind::Create(CreateKind::File)).add_path(probe_path.clone());
    assert!(names_the_probe(&mine, &probe_path));

    let somebody_elses = notify::Event::new(EventKind::Create(CreateKind::File))
        .add_path(root.join("Series - v01.cbz"));
    assert!(
        !names_the_probe(&somebody_elses, &probe_path),
        "an unrelated write in the same directory is not this experiment's answer"
    );

    // A rename carries two paths and either may be the probe's; the `any`
    // is what covers that, and an event carrying no path at all is not an
    // answer either.
    let renamed_onto_the_probe = notify::Event::new(EventKind::Create(CreateKind::File))
        .add_path(root.join("elsewhere"))
        .add_path(probe_path.clone());
    assert!(names_the_probe(&renamed_onto_the_probe, &probe_path));
    assert!(!names_the_probe(
        &notify::Event::new(EventKind::Create(CreateKind::File)),
        &probe_path
    ));
}

/// The boundary itself. The two tests above sit 50 either side of the
/// ceiling and neither is on it, so `>` and `>=` answer alike for both — and
/// a root with exactly as many directories as the ceiling allows reports
/// "watching only 150 of 150 directories — 0 short of …", which names a
/// shortfall of nothing.
#[test]
fn a_directory_count_exactly_at_the_ceiling_is_not_a_shortfall() {
    let watching = Probe {
        watching: true,
        reason: None,
    };
    let line = report_line_against(
        Path::new("/srv/manga"),
        &watching,
        150,
        interval_for(&watching),
        Some(150),
    );
    assert_eq!(
        line, "root /srv/manga watching 150 directories — scanning every 24h",
        "150 of a 150 ceiling fits; the shortfall branch must not fire"
    );
}

/// `probe` always supplies a `reason` when it answers `false`, so nothing
/// built the state this fallback exists for — and `""` in its place gives an
/// operator `watch probe failed () — scanning every 15m`, which names
/// neither the failure nor anything to do about it. [`Probe`] is a `pub`
/// struct with `pub` fields, so a caller outside this crate can build
/// exactly that.
#[test]
fn a_failure_with_no_reason_still_says_something() {
    let no_reason = Probe {
        watching: false,
        reason: None,
    };
    let line = report_line(
        Path::new("/mnt/remote/manga"),
        &no_reason,
        12,
        interval_for(&no_reason),
    );
    assert_eq!(
        line,
        "root /mnt/remote/manga watch probe failed (no reason given) — scanning every 15m"
    );
}

#[test]
fn report_line_reads_the_real_watch_ceiling_this_host_allows() {
    // Not a mock: this host really does expose the file, so the public
    // `report_line` — not the injectable `report_line_against` above — must
    // actually read it rather than behaving as if no ceiling exists.
    let ceiling = read_watch_ceiling();
    assert!(
        ceiling.is_some_and(|n| n > 0),
        "this host exposes /proc/sys/fs/inotify/max_user_watches: {ceiling:?}"
    );

    // The assertion above pins `read_watch_ceiling` alone — it never calls
    // `report_line`, so it could not catch `report_line` failing to use it.
    // `usize::MAX` exceeds any ceiling this host could really report, so if
    // `report_line` used `None` in place of the real value here (surviving
    // mutant: replace the `read_watch_ceiling()` call at its one call site
    // with `None`), this line would come back as the plain "watching N
    // directories" form instead of naming a shortfall, and the assertion
    // below would catch it.
    let watching = Probe {
        watching: true,
        reason: None,
    };
    let line = report_line(
        Path::new("/srv/manga"),
        &watching,
        usize::MAX,
        interval_for(&watching),
    );
    assert!(
        line.contains("max_user_watches"),
        "report_line must read the real ceiling rather than assume none exists: {line}"
    );
}
