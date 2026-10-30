//! Write a file so a crash cannot publish half of one.
//!
//! **Call sites**, from `rg -n 'atomic::write\(' crates/` with comment lines
//! and tests set aside, rather than from anyone's memory: in `taiyaku-serve`,
//! the sidecar rewritten by a correction in `routes.rs` and **two** in
//! `cover.rs`; in `taiyaku-library`, `hand::save` and `export`; in
//! `taiyaku-cli`, `write_sidecar` — behind both `align --write` and
//! `credits --write` — and `regions.rs`.
//!
//! **If you touch this module, run the grep and read what comes back.** Do
//! not update this list by reading the sentence above and adding to it.
//!
//! It lives in `taiyaku-read` rather than in the server because it is not a
//! server concern — a sidecar is a sidecar whoever rewrites it — and because
//! `taiyaku-library` needs it for export and must not depend on
//! `taiyaku-serve`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// A temp path nothing else will ever pick.
///
/// `pub` because `taiyaku-serve`'s cover cache asserts a joint property of
/// this function and its own `cache_path` — that a temp name never ends where
/// the read path would find it, so an unrenamed leftover can never be served
/// as a real cover — and that test cannot live in this crate, because this
/// crate knows nothing about covers.
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
/// where a whole one was — for a sidecar, that destroys the alignment of every
/// edition in the work.
///
/// A failed write or a failed rename each leave a temp file nothing else will
/// ever collect, since its name is unique to this attempt, so both branches
/// remove it rather than accumulating one file per failure in the reader's
/// own library directory.
///
/// **Every error names `path` — the file the caller asked for — and never the
/// temporary (O29).** The temporary is a filename this module invents, that no
/// caller typed, that is deleted before the error is returned, and that
/// therefore cannot be looked for.
///
/// The distinction between the two failures is kept, because they are not the
/// same event — a full disk or a missing directory fails the staging write,
/// while a rename fails on a destination that cannot be replaced. It is
/// carried in words rather than by swapping in a path the caller cannot use.
///
/// `pub`, not `pub(crate)`: `taiyaku-cli`'s `write_sidecar` — the one writer
/// behind both `align --write` and `credits --write` — is one of the callers
/// outside this crate, and it is writing the very file the first
/// paragraph names.
///
/// **The temporary is synced before the rename, and the directory after it
/// (O104).** A rename replaces the destination atomically only for bytes
/// already on disk, and without the sync the rename can be journalled first —
/// XFS, which the NAS's array disks run, does nothing to force the data out
/// ahead of it — so a crash could leave `path` naming data never written, with
/// the old file already gone. The directory sync is what makes the rename
/// itself survive the crash.
///
/// # Errors
///
/// Returns an error if writing or syncing the temporary file beside `path`
/// fails — a full disk or a missing parent directory, most often — if
/// renaming that temporary onto `path` fails, such as `path` naming a
/// directory rather than a file, or if syncing `path`'s directory after the
/// rename fails. Every message names `path`, the file the caller asked for,
/// and never the temporary, which is deleted before either of the first two
/// errors is returned and no longer exists by the third.
pub fn write(path: &Path, bytes: &[u8], extension: &str) -> Result<(), String> {
    let temporary = temp_path(path, extension);
    let staged = std::fs::File::create(&temporary).and_then(|mut file| {
        std::io::Write::write_all(&mut file, bytes)?;
        file.sync_all()
    });
    if let Err(e) = staged {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!(
            "{}: staging a temporary beside it: {e}",
            path.display()
        ));
    }
    if let Err(e) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!(
            "{}: renaming the temporary into place: {e}",
            path.display()
        ));
    }
    sync_directory_of(path).map_err(|e| {
        format!(
            "{}: syncing its directory after the rename: {e}",
            path.display()
        )
    })
}

/// Sync the directory holding `path`, so a rename into it survives a crash.
///
/// # Errors
///
/// Returns the error opening or syncing that directory.
pub fn sync_directory_of(path: &Path) -> std::io::Result<()> {
    let dir = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    std::fs::File::open(dir)?.sync_all()
}

#[cfg(test)]
mod tests;
