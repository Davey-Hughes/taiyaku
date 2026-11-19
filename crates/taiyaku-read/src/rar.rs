//! RAR archives, read through a backend. The page rules belong above this
//! interface, in `pages.rs`, so that they hold for every backend alike.

#[cfg(test)]
use std::cell::Cell;
use std::path::Path;

#[cfg(feature = "rar-libarchive")]
mod libarchive;
#[cfg(test)]
mod tests;

/// One entry of an archive, as its header gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    /// UTF-8 and `/`-separated, as the archive names it. A name the backend
    /// cannot decode as UTF-8, as a RAR4 name in a legacy code page may be,
    /// refuses the whole archive as not a readable RAR archive, where a zip
    /// keeps the page under a garbled name: the RAR4 row in `docs/status.md`.
    pub(crate) name: String,
    /// The uncompressed size the header declares, or 0 where the backend is
    /// given none: libarchive gives none for a hard link or file reference.
    pub(crate) size: u64,
    /// A regular file: directories and symbolic links are not. Hard links and
    /// file references are, though they hold no data and their reads fail, and
    /// so is an encrypted entry, since encryption is not detected (O381).
    pub(crate) readable: bool,
}

/// What [`Backend::visit`] hands each decoded entry to, with its name.
pub(crate) type Visitor<'a> = dyn FnMut(&str, &[u8]) -> Result<(), String> + 'a;

/// A way of reading RAR. Each call opens the file itself, so calls on several
/// threads share nothing. Errors start with the path: `"{path}: not a
/// readable RAR archive: {cause}"` when it will not open, list or walk, and
/// `"{path}: {name}: {cause}"` when one entry will not read, as when its
/// bytes differ from its declared size or checksum or none arrive.
pub(crate) trait Backend: Sync + std::fmt::Debug {
    #[cfg_attr(not(test), expect(dead_code, reason = "only a test names its backend"))]
    fn name(&self) -> &'static str;
    /// Every entry, in archive order.
    fn entries(&self, path: &Path) -> Result<Vec<Entry>, String>;
    /// The bytes of the first entry named `name`, or `"{path}: {name}: no
    /// such entry"`.
    fn read(&self, path: &Path, name: &str) -> Result<Vec<u8>, String>;
    /// Decodes, in archive order, each readable entry `wanted` accepts and
    /// hands it to `f`. The rest are skipped, undecoded unless the archive is
    /// solid. An `Err` from `f` stops the visit and is returned unchanged.
    fn visit(
        &self,
        path: &Path,
        wanted: &dyn Fn(&str) -> bool,
        f: &mut Visitor<'_>,
    ) -> Result<(), String>;
}

/// Every compiled backend, in the order `build.rs` lists their features.
pub(crate) static BACKENDS: &[&dyn Backend] = &[
    #[cfg(feature = "rar-libarchive")]
    &libarchive::Libarchive,
];

/// The backend page reads go through: the first compiled one, or in a test
/// the one `with_backend` chose on this thread.
pub(crate) fn backend() -> &'static dyn Backend {
    #[cfg(test)]
    if let Some(chosen) = CHOSEN.with(Cell::get) {
        return chosen;
    }
    BACKENDS[0]
}

#[cfg(test)]
thread_local! {
    static CHOSEN: Cell<Option<&'static dyn Backend>> = const { Cell::new(None) };
}

/// Runs `f` with [`backend`] answering `chosen` on this thread, so one test
/// can hold each backend to the same fixtures. The choice ends with the call,
/// even when `f` panics.
#[cfg(test)]
pub(crate) fn with_backend<T>(chosen: &'static dyn Backend, f: impl FnOnce() -> T) -> T {
    struct Restore(Option<&'static dyn Backend>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CHOSEN.with(|cell| cell.set(self.0));
        }
    }
    let _restore = Restore(CHOSEN.with(|cell| cell.replace(Some(chosen))));
    f()
}
