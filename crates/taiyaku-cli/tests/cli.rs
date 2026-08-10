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
