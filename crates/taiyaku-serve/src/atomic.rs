//! Write a file so a crash cannot publish half of one.
//!
//! Five callers, counted rather than remembered: `progress::store` (O2),
//! `cover.rs`'s generated cover, `notes::store`, the sidecar rewritten by a
//! correction in `routes.rs`, and `taiyaku-cli`'s `write_sidecar`. The first
//! two carried their own copy of this and said so in a comment; a third copy
//! is where a comment asking whoever changes one to keep the others in step
//! stops being good enough.
//!
//! **This tally said "three" while it was already four, and the count was
//! raised to "four" while it was already five.** Both times the number was
//! written from memory of what this module was for rather than from the call
//! sites. `notes::store` was the one missed twice. If you add a caller, grep
//! for `atomic::write` and count what comes back.
//!
//! **O26 added a sixth command-line writer and the count did not move**, which
//! is the point of counting call sites rather than commands: `taiyaku credits
//! --write` and `taiyaku align --write` both go through one `write_sidecar`,
//! so there is one call site for the two of them. A tally of commands would
//! now be wrong.
//!
//! The last is in another crate, which is why this module is `pub`. It is not
//! a server concern that happens to be reachable — a sidecar is a sidecar
//! whoever rewrites it, and the alternative to exporting this was another copy
//! of write-and-rename in `taiyaku-cli`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// A temp path nothing else will ever pick.
///
/// A process id plus a monotonic per-process counter. Two devices writing one
/// work through one server is two requests in one process, so the counter
/// carries that case; two servers over one directory is what the pid carries.
pub(crate) fn temp_path(path: &Path, extension: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    path.with_extension(format!("{extension}.tmp.{pid}.{n}"))
}

/// Write `bytes` to `path`, atomically.
///
/// `fs::write` truncates first, and a crash in that window leaves a short file
/// where a whole one was — for progress that answers 500 instead of "nothing
/// recorded", and for a sidecar it destroys the alignment of every edition in
/// the work.
///
/// A failed write or a failed rename each leave a temp file nothing else will
/// ever collect, since its name is unique to this attempt, so both branches
/// remove it rather than accumulating one file per failure in the reader's
/// own library directory. `cover.rs`'s writer, before this module existed,
/// did this unconditionally for the same reason; both failure paths now get
/// the same treatment here.
///
/// **Both errors name `path` — the file the caller asked for — and never the
/// temporary.** That was O29: the staging branch used to name the temporary,
/// which is a filename this module invents, that no caller typed, that is
/// deleted before the error is returned, and that therefore cannot be looked
/// for. It reached people two ways. `taiyaku align --write /nowhere/out.json`
/// answered `out.json.tmp.4041762.0: No such file or directory`, and — the
/// half the item had recorded as harmless — three of the four callers in this
/// crate put the string straight into a 500 body (`routes.rs`'s
/// `Failed(message)` arm), so `notes::store`, `progress::store` and the
/// sidecar rewrite leaked it to HTTP clients too. Only `cover.rs` discards it.
///
/// The distinction between the two failures is kept, because they are not the
/// same event — a full disk or a missing directory fails the staging write,
/// while a rename fails on a destination that cannot be replaced. It is
/// carried in words rather than by swapping in a path the caller cannot use.
///
/// `pub`, not `pub(crate)`: `taiyaku-cli`'s `write_sidecar` — the one writer
/// behind both `align --write` and `credits --write` — is the caller outside
/// this crate, and it is writing the very file the paragraph above names as the
/// worst thing to lose half of.
///
/// # Errors
///
/// Returns an error if writing the temporary file beside `path` fails — a
/// full disk or a missing parent directory, most often — or if renaming that
/// temporary onto `path` fails, such as `path` naming a directory rather than
/// a file. Both messages name `path`, the file the caller asked for, and
/// never the temporary, which is deleted before either error is returned.
pub fn write(path: &Path, bytes: &[u8], extension: &str) -> Result<(), String> {
    let temporary = temp_path(path, extension);
    if let Err(e) = std::fs::write(&temporary, bytes) {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!(
            "{}: staging a temporary beside it: {e}",
            path.display()
        ));
    }
    match std::fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&temporary);
            Err(format!(
                "{}: renaming the temporary into place: {e}",
                path.display()
            ))
        }
    }
}

#[cfg(test)]
mod tests;
