//! What `main` itself does, which no unit test can reach.
//!
//! Everything below `run` is tested in place. `main` is the four lines around
//! it — take the arguments, call it, print the message, set the exit code —
//! and each of those four is a contract something outside this repository
//! depends on: `scripts/margin-survey.sh` and the collectors beside it read
//! stdout and branch on the status. A `main` that did nothing at all would
//! leave every other test in the crate green.

use std::process::Command;

/// The binary this crate builds, resolved by Cargo rather than by guessing at
/// a path under `target/`, which differs by profile and by platform.
fn taiyaku() -> Command {
    Command::new(env!("CARGO_BIN_EXE_taiyaku"))
}

/// A refusal exits non-zero and says why, prefixed so a message from this tool
/// is distinguishable from one from the shell.
#[test]
fn a_refused_command_exits_one_and_names_the_tool() {
    let run = taiyaku()
        .arg("no-such-command")
        .output()
        .expect("the binary runs");

    assert_eq!(run.status.code(), Some(1), "a refusal is a failing exit");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.starts_with("taiyaku: "),
        "the message names the tool: {stderr}"
    );
    assert!(
        stderr.contains("no-such-command"),
        "and names what it refused: {stderr}"
    );
    assert!(
        run.stdout.is_empty(),
        "a refusal writes nothing to the output stream"
    );
}

/// No arguments is the usage, and the usage is a success: someone asking what
/// this is has not made a mistake.
#[test]
fn no_arguments_prints_the_usage_to_stdout_and_succeeds() {
    let run = taiyaku().output().expect("the binary runs");

    assert_eq!(run.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        stdout.starts_with("taiyaku "),
        "the version leads the usage: {stdout}"
    );
    assert!(
        stdout.contains("usage: taiyaku align"),
        "and the commands follow it: {stdout}"
    );
}

/// `--cache` with nothing after it is a refusal rather than a silent fall
/// back to the default. The same contract every other value-taking flag in
/// this binary holds, and the reason `option_value` returns a `Result`.
///
/// Only refusal paths are exercised from here: a `serve` that parsed its
/// arguments successfully would bind a port and block forever.
#[test]
fn serve_refuses_cache_with_no_directory_after_it() {
    let run = taiyaku()
        .args(["serve", "some.json", "--cache"])
        .output()
        .expect("the binary runs");

    assert_eq!(run.status.code(), Some(1), "a refusal is a failing exit");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.starts_with("taiyaku: "),
        "the message names the tool: {stderr}"
    );
    assert!(
        stderr.contains("--cache needs a directory"),
        "and says what was missing: {stderr}"
    );
    assert!(
        run.stdout.is_empty(),
        "a refusal writes nothing to the output stream"
    );
}

/// Adding a flag to `serve`'s accepted list must not accept everything.
/// `reject_unknown_flags` is the guard, and a list edited by hand is exactly
/// where it could be widened by accident.
#[test]
fn serve_still_refuses_an_unknown_flag_and_lists_cache_among_the_real_ones() {
    let run = taiyaku()
        .args(["serve", "some.json", "--cash", "/tmp/x"])
        .output()
        .expect("the binary runs");

    assert_eq!(run.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("--cash"),
        "the refusal names what it refused: {stderr}"
    );
    assert!(
        stderr.contains("--cache"),
        "and the alphabet it listed includes the real flag: {stderr}"
    );
}

/// The usage names the flag. A flag that works but is not advertised is one
/// nobody finds, and this line is the only place `serve`'s options are
/// listed.
#[test]
fn the_usage_advertises_serve_cache() {
    let run = taiyaku().output().expect("the binary runs");

    let stdout = String::from_utf8_lossy(&run.stdout);
    let serve_line = stdout
        .lines()
        .find(|l| l.contains("taiyaku serve"))
        .expect("the usage lists serve");
    assert!(
        serve_line.contains("[--cache DIR]"),
        "serve's usage line advertises the flag: {serve_line}"
    );
}
