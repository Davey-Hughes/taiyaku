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
    /// `/`-separated, as the archive names it, and decoded as UTF-8. A RAR4
    /// name with no Unicode form, stored in a legacy code page, is read as
    /// code page 437 where its bytes are not UTF-8, as a zip's is, so the page
    /// keeps its place under a name that may be garbled; one whose bytes
    /// happen to be valid UTF-8 reads as UTF-8. A legacy name whose `\` may be
    /// half of a character instead of a separator refuses the whole archive,
    /// as does a name its header declares UTF-8 that is not, which libarchive
    /// leaves with no name: a RAR5's, or a RAR4's under its Unicode flag.
    pub(crate) name: String,
    /// The uncompressed size the header declares. A hard link's or file
    /// reference's is its target's, whose bytes it reads.
    pub(crate) size: u64,
    /// A regular file: directories and symbolic links are not. Hard links and
    /// file references are, and read their target's bytes.
    pub(crate) readable: bool,
    /// The CRC32 the header declares for the bytes the entry reads, which a
    /// whole read of them meets: a hard link's or file reference's is its
    /// target's. None where it declares none, as a RAR5 made with `rar -htb`
    /// declares a BLAKE2 hash in its place, which the backend holds a read to
    /// instead, and for a directory or a symbolic link.
    pub(crate) crc: Option<u32>,
    /// For a hard link or file reference, the archive position of the earlier
    /// entry whose bytes it reads. An archive holding one with no such entry
    /// is refused where it is listed.
    pub(crate) target: Option<usize>,
}

/// What [`Backend::visit`] hands each decoded entry to, with its name.
pub(crate) type Visitor<'a> = dyn FnMut(&str, &[u8]) -> Result<(), String> + 'a;

/// A way of reading RAR. Each call opens the file itself, so calls on several
/// threads share nothing. Errors start with the path: `"{path}: not a
/// readable RAR archive: {cause}"` when it will not open, list or walk, and
/// `"{path}: {name}: {cause}"` when one entry will not read, as when its
/// bytes differ from its declared size or checksum or none arrive.
pub(crate) trait Backend: Sync + std::fmt::Debug {
    /// The backend's name, as the tests give it and as its
    /// [`signature`](Backend::signature) begins.
    fn name(&self) -> &'static str;
    /// The backend's part of the reader signature, so that a build reading RAR
    /// another way counts every RAR again.
    fn signature(&self) -> String;
    /// Every entry, in archive order. An archive of a kind no read could give
    /// whole is refused here, where it is counted: one volume of a multi-volume
    /// set, or one holding an encrypted entry, a RAR4 entry compressed in a way
    /// the backend does not decode, a hard link or file reference with no earlier
    /// target or of another size than its target, or a file it lists but reads no
    /// bytes of.
    fn entries(&self, path: &Path) -> Result<Vec<Entry>, String>;
    /// The bytes of the first regular file named `name`, or `"{path}: {name}:
    /// no such entry"`. A directory or link of that name holds no bytes to read.
    fn read(&self, path: &Path, name: &str) -> Result<Vec<u8>, String>;
    /// Lists the archive as [`Backend::entries`] does, hands the listing to
    /// `pick`, and reads the regular file at the position `pick` returns, all
    /// through one open of the file: a file renamed over `path` between the
    /// listing and the read cannot hand over another archive's bytes for a
    /// position this listing gave. An `Err` from `pick` is returned unchanged.
    fn read_listed(
        &self,
        path: &Path,
        pick: &mut dyn FnMut(&[Entry]) -> Result<usize, String>,
    ) -> Result<Vec<u8>, String>;
    /// Decodes, in archive order, each readable entry `wanted` accepts and
    /// hands it to `f`. A hard link or file reference hands over its target's
    /// bytes, kept from the target's own decode, which happens for it even
    /// where `wanted` passes the target over. The rest are skipped, undecoded
    /// unless the archive is solid. An `Err` from `f` stops the visit and is
    /// returned unchanged.
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
