use std::process::Command;

use super::*;

/// Field `n` of this thread's `/proc/thread-self/stat`, numbered from 1 as
/// `proc(5)` numbers them. Counted from the name's closing parenthesis for
/// every field after it, since the name in field 2 can hold a space.
pub(in crate::fingerprint_pass) fn stat_field(n: usize) -> String {
    let stat = std::fs::read_to_string("/proc/thread-self/stat").unwrap();
    let (head, rest) = stat.rsplit_once(')').unwrap();
    let field = if n == 1 {
        head.split_whitespace().next()
    } else {
        rest.split_whitespace().nth(n - 3)
    };
    field.unwrap().to_owned()
}

/// This thread's I/O class as `ionice` prints it.
pub(in crate::fingerprint_pass) fn io_class() -> String {
    let out = Command::new("ionice")
        .arg("-p")
        .arg(stat_field(1))
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A line `proc(5)` describes, its name holding what would end a naive read.
const STAT: &str = "7 (a) b (c) S 1 7 7 0 -1 4194304 90 0 0 0 3 1 0 0 20 19 2 0 12 0 0";

#[test]
fn niceness_is_read_from_after_the_last_parenthesis_of_the_name() {
    assert_eq!(niceness_of(STAT), Some(19));
    assert_eq!(niceness_of("7 (a) S 1"), None, "a line cut short has none");
    assert_eq!(niceness_of("no name at all"), None);
}

#[test]
fn a_thread_is_lowered_only_when_it_runs_below_the_main_thread() {
    assert!(lowered_given(Some(19), Some(0)), "below the main thread");
    assert!(
        lowered_given(Some(19), Some(5)),
        "below a niced main thread"
    );
    assert!(!lowered_given(Some(0), Some(0)), "level with it");
    assert!(!lowered_given(Some(5), Some(5)), "level under `nice`");
    assert!(!lowered_given(Some(0), Some(5)), "above it");
}

#[test]
fn a_niceness_that_cannot_be_read_counts_as_lowered() {
    assert!(lowered_given(None, Some(0)));
    assert!(lowered_given(Some(0), None));
    assert!(lowered_given(None, None));
}

/// The niceness (field 19) and the id (field 1) come from the thread's own
/// `stat`, not from [`lower`]'s reading of its id, so a `lower` that acts on
/// the process id lowers another thread and this sees it.
#[test]
fn lower_renices_and_idles_this_thread_alone() {
    for tool in ["renice", "ionice"] {
        if Command::new(tool).arg("--version").output().is_err() {
            eprintln!(
                "NOT MEASURED: lower_renices_and_idles_this_thread_alone (no `{tool}` on PATH)"
            );
            return;
        }
    }
    let before = stat_field(19);
    if before == "19" || io_class() == "idle" {
        eprintln!(
            "NOT MEASURED: lower_renices_and_idles_this_thread_alone (this thread is already lowered)"
        );
        return;
    }

    let (nice, class) = std::thread::spawn(|| {
        lower().unwrap();
        (stat_field(19), io_class())
    })
    .join()
    .unwrap();

    assert_eq!(nice, "19", "the thread that called it is at niceness 19");
    assert_eq!(class, "idle", "and in the idle I/O class");
    assert_eq!(stat_field(19), before, "this thread keeps its niceness");
}
