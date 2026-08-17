//! Write a file so a crash cannot publish half of one.
//!
//! **Eight call sites**, from `rg -n 'atomic::write' crates/` on 2026-08-14
//! rather than from anyone's memory: in `taiyaku-serve`, `progress::store`,
//! `notes::store`, `truth::store`, the sidecar rewritten by a correction in
//! `routes.rs`, and **two** in `cover.rs`; in `taiyaku-cli`, `write_sidecar`
//! — behind both `align --write` and `credits --write` — and `regions.rs`.
//!
//! **This tally has now been wrong three times, and the third is the reason
//! the instruction below is phrased as a command.** It said "three" while it
//! was four, then "four" while it was five, and then sat at five through
//! `truth.rs` landing on 2026-08-11 — so it was five while it was six, for
//! three days, in a comment whose entire subject is that it keeps being
//! wrong. The move to this crate on 2026-08-14 was drafted with a corrected
//! count that was *also* wrong, because it too was written from this
//! paragraph rather than from the code.
//!
//! **So: if you touch this module, run the grep and count what comes back.**
//! Do not update this number by reading the sentence above and adding one.
//!
//! It lives in `taiyaku-read` rather than in the server because it is not a
//! server concern — a sidecar is a sidecar whoever rewrites it — and because
//! `taiyaku-library` needs it for export and must not depend on
//! `taiyaku-serve`. It was in `taiyaku-serve` until 2026-08-14 and moved down
//! rather than being copied.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// A temp path nothing else will ever pick.
///
/// `pub` rather than `pub(crate)` since 2026-08-14, when this module moved
/// down a crate. `taiyaku-serve`'s cover cache asserts a joint property of
/// this function and its own `cache_path` — that a temp name never ends where
/// the read path would find it, so an unrenamed leftover can never be served
/// as a real cover — and that test cannot move here, because this crate knows
/// nothing about covers.
///
/// A process id plus a monotonic per-process counter. Two devices writing one
/// work through one server is two requests in one process, so the counter
/// carries that case; two servers over one directory is what the pid carries.
pub fn temp_path(path: &Path, extension: &str) -> PathBuf {
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
