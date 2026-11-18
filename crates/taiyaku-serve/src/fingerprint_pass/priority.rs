//! Lowering one thread's CPU and I/O priority.
//!
//! Through `renice` and `ionice` rather than a call: the workspace forbids
//! `unsafe_code`, and no crate in the shipped graph wraps `ioprio_set`. Both
//! are given the thread's own id, since Linux keeps niceness and I/O priority
//! per thread: the process id would lower the main thread and leave this one.

use std::process::Command;

/// Lower this thread to niceness 19 and the idle I/O class.
///
/// # Errors
///
/// When this thread's id cannot be read, or either command cannot run or
/// refuses. Each command is tried whether or not the other succeeds.
pub(crate) fn lower() -> Result<(), String> {
    let tid = thread_id()?;
    // `--priority` is the niceness itself; under POSIXLY_CORRECT util-linux
    // reads `-n` as an increment.
    let failures: Vec<String> = [
        ("renice", ["--priority", "19", "-p"]),
        ("ionice", ["-c", "3", "-p"]),
    ]
    .into_iter()
    .filter_map(|(tool, args)| run(tool, &args, &tid).err())
    .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

/// This thread's id: the last part of where `/proc/thread-self` points.
fn thread_id() -> Result<String, String> {
    let link =
        std::fs::read_link("/proc/thread-self").map_err(|e| format!("/proc/thread-self: {e}"))?;
    link.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| name.parse::<u32>().is_ok())
        .map(str::to_owned)
        .ok_or_else(|| format!("/proc/thread-self points at {}", link.display()))
}

/// Run `tool` with `args` and then `tid`, its output kept off the server's.
fn run(tool: &str, args: &[&str], tid: &str) -> Result<(), String> {
    let output = Command::new(tool)
        .args(args)
        .arg(tid)
        .output()
        .map_err(|e| format!("{tool}: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{tool}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[cfg(test)]
pub(super) mod tests;
