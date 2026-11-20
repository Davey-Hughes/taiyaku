//! An edition as one `STORE` zip whose length is known before the first byte.
//!
//! **Why the writer is here rather than `zip::ZipWriter`.** Spec C2 §13.1's
//! whole argument for `STORE` is that the output's size is a deterministic
//! function of its entry names and sizes, so `Content-Length` is exact and
//! `Range` therefore works — and a `Write + Seek` writer cannot tell you either
//! of those before it has written the file. This module is the arithmetic that
//! can: `Plan::len` is the answer, `Plan::local_header` and `Plan::trailer` are
//! the bytes around the payloads, and `Plan::payload_span` is what lets a
//! resume find its way in.
//!
//! **`STORE` and not `DEFLATE`**, because the pages are already JPEGs. That is
//! §13.1's reasoning and it holds; what does *not* hold is the sentence beside
//! it calling generation "a copy": an entry that is `DEFLATE` in its archive
//! is inflated to produce this output. The count of such entries is recorded in
//! `docs/superpowers/specs/2026-08-23-library-download-design.md` §1 and, with
//! its reproduction, in `docs/notes.md`, "Four measurements for spec C2's
//! download". The consequence worth knowing is
//! that a generated CBZ is slightly **larger** than the archive it came from.
//!
//! No data descriptors: every crc is known before its local header, because a
//! zip member's comes free from its central directory and a RAR member's from
//! its entry headers, and a directory member's, or a RAR entry's that declares
//! a BLAKE2 hash and no CRC32, is hashed in
//! `taiyaku_read::member_page_entries`. Spec §4 records
//! why that beat the streaming bit — this is the one artefact taiyaku produces
//! that leaves the server, and it fails on a device where nothing reports back.
//!
//! **`Plan::zip64` and `Plan::payload_span` are the two methods unreachable
//! outside `cbz::tests`, and each is suppressed on its own, not by the
//! module.** A `#![cfg_attr(not(test), expect(dead_code, …))]` covering the
//! whole module would stay satisfied as long as *any* item here is dead, so
//! it would never go unfulfilled and never come off. `zip64()` reports the
//! boundary `Plan::of`'s arithmetic decides; `payload_span()` is the map a
//! true `Range` seek would walk (`docs/status.md`, O66). Both wait on a
//! caller `download.rs::generated` does not have, so each carries its own
//! `#[cfg_attr(not(test), expect(dead_code, reason = …))]` — scoped to
//! `not(test)`, since `cbz::tests` is what makes each reachable at all —
//! `#[expect]`, not `#[allow]`, and each comes off the day its own caller
//! lands rather than the day any part of this module gets used.

/// A fixed DOS timestamp: 1980-01-01 00:00:00, the epoch of the field.
///
/// Constant rather than "now" so that two downloads of an unchanged edition are
/// byte-identical. A mutable clock here would also make every test in this
/// module's `tests.rs` depend on when it ran.
const DOS_DATE: u16 = 0x0021;
const DOS_TIME: u16 = 0x0000;

/// Above this, a size or offset cannot be stated in a classic record.
const U32_MAX: u64 = 0xFFFF_FFFF;

const LOCAL_HEADER_FIXED: u64 = 30;
const CENTRAL_HEADER_FIXED: u64 = 46;
const END_RECORD: u64 = 22;
/// Header id + size + uncompressed + compressed, on a local header.
const ZIP64_LOCAL_EXTRA: u64 = 4 + 8 + 8;
/// The same plus the local header's offset, on a central-directory record.
const ZIP64_CENTRAL_EXTRA: u64 = 4 + 8 + 8 + 8;
const ZIP64_END_RECORD: u64 = 56;
const ZIP64_LOCATOR: u64 = 20;

/// One entry as the plan knows it: what it is called, how long it is, and the
/// crc32 of exactly those bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlannedEntry {
    pub(crate) name: String,
    pub(crate) size: u64,
    pub(crate) crc32: u32,
}

/// A complete output layout, computed before anything is sent.
#[derive(Debug, Clone)]
pub(crate) struct Plan {
    entries: Vec<PlannedEntry>,
    /// Absolute offset of each entry's **local header**, parallel to `entries`.
    offsets: Vec<u64>,
    /// Where the first central-directory record starts.
    directory_at: u64,
    /// The total, which is what `Content-Length` states.
    total: u64,
    zip64: bool,
}

impl Plan {
    /// Lay out `entries` in the order given.
    ///
    /// The order **is** the reading order; this module does not sort. Deciding
    /// which page comes next is `docs/sidecar.md`'s normative rule and
    /// `download.rs` applies it, because a second sort here would be a second
    /// derivation of it.
    pub(crate) fn of(entries: Vec<PlannedEntry>) -> Self {
        // Two passes, because whether a classic record can hold this file
        // decides the size of every record in it. A single pass would have to
        // guess and re-do.
        let classic = Self::lay_out(&entries, false);
        // **Two conditions, and only two, because the rest are dominated.**
        // `lay_out` returns `(offsets, directory_at, directory_at + directory
        // + tail)`, and `tail` is never smaller than `END_RECORD`, so `total`
        // is strictly greater than `directory_at`. That makes both of these
        // checks unreachable:
        //
        // - **a per-entry `size > U32_MAX` check**, because `directory_at` is
        //   the running sum over every entry of `LOCAL_HEADER_FIXED + name +
        //   extra + size`, so one oversized entry has already carried it past
        //   the ceiling; and
        // - **a `directory_at > U32_MAX` check**, because `total` exceeds
        //   `directory_at` by at least 22 bytes and is tested first.
        //
        // Dead logic that looks like a safety net is worse than no net: it
        // invites the next reader to trust it. **If `lay_out`'s formula ever
        // changes so that `total` no longer dominates `directory_at`, this is
        // the invariant that breaks** — disabling the size trigger fails
        // `a_total_past_four_gibibytes_takes_the_zip64_branch`,
        // `one_entry_past_four_gibibytes_takes_it_too` and
        // `a_zip64_payload_span_accounts_for_the_extra_field`.
        let needs_zip64 = classic.2 > U32_MAX || entries.len() > usize::from(u16::MAX);
        let (offsets, directory_at, total) = if needs_zip64 {
            Self::lay_out(&entries, true)
        } else {
            classic
        };
        Self {
            entries,
            offsets,
            directory_at,
            total,
            zip64: needs_zip64,
        }
    }

    /// `(local header offsets, where the directory starts, total length)`.
    fn lay_out(entries: &[PlannedEntry], zip64: bool) -> (Vec<u64>, u64, u64) {
        let local_extra = if zip64 { ZIP64_LOCAL_EXTRA } else { 0 };
        let central_extra = if zip64 { ZIP64_CENTRAL_EXTRA } else { 0 };
        let mut offsets = Vec::with_capacity(entries.len());
        let mut at = 0u64;
        for entry in entries {
            offsets.push(at);
            at += LOCAL_HEADER_FIXED + entry.name.len() as u64 + local_extra + entry.size;
        }
        let directory_at = at;
        let directory: u64 = entries
            .iter()
            .map(|e| CENTRAL_HEADER_FIXED + e.name.len() as u64 + central_extra)
            .sum();
        let tail = if zip64 {
            ZIP64_END_RECORD + ZIP64_LOCATOR + END_RECORD
        } else {
            END_RECORD
        };
        (offsets, directory_at, directory_at + directory + tail)
    }

    pub(crate) fn len(&self) -> u64 {
        self.total
    }

    pub(crate) fn entries(&self) -> &[PlannedEntry] {
        &self.entries
    }

    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "read only by cbz::tests until a caller needs the \
                       zip64 boundary Task 3's arithmetic decided — \
                       download.rs does not ask yet"
        )
    )]
    pub(crate) fn zip64(&self) -> bool {
        self.zip64
    }

    /// The absolute `[start, end)` of entry `index`'s payload in the output.
    ///
    /// # Panics
    ///
    /// Panics if `index` is not an entry of this plan, which is a caller bug:
    /// every caller iterates `entries()`.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "read only by cbz::tests until a true Range seek \
                       walks it — download.rs::generated reads and \
                       discards a resumed prefix instead (docs/status.md, \
                       O66), so nothing calls this yet"
        )
    )]
    pub(crate) fn payload_span(&self, index: usize) -> (u64, u64) {
        let entry = &self.entries[index];
        let local_extra = if self.zip64 { ZIP64_LOCAL_EXTRA } else { 0 };
        let start =
            self.offsets[index] + LOCAL_HEADER_FIXED + entry.name.len() as u64 + local_extra;
        (start, start + entry.size)
    }

    /// Entry `index`'s local file header.
    ///
    /// # Panics
    ///
    /// Panics if `index` is not an entry of this plan — see `payload_span`.
    pub(crate) fn local_header(&self, index: usize) -> Vec<u8> {
        let entry = &self.entries[index];
        let mut out = Vec::with_capacity(64 + entry.name.len());
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&if self.zip64 { 45u16 } else { 20u16 }.to_le_bytes());
        // No flags. The names this module is given are ASCII by construction —
        // `download.rs` numbers them — so the UTF-8 bit would state something
        // true and buy nothing, and a zero here is the most conservative
        // header a reader can be handed.
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // STORE
        out.extend_from_slice(&DOS_TIME.to_le_bytes());
        out.extend_from_slice(&DOS_DATE.to_le_bytes());
        out.extend_from_slice(&entry.crc32.to_le_bytes());
        let stated = if self.zip64 { U32_MAX } else { entry.size };
        // Compressed and uncompressed are the same number: this is STORE.
        out.extend_from_slice(&(stated as u32).to_le_bytes());
        out.extend_from_slice(&(stated as u32).to_le_bytes());
        // Bounded by the caller, not by this line: `download.rs` numbers every
        // entry itself (`0001.jpg`, and `ComicInfo.xml`), so a name here is a
        // few bytes and cannot approach `u16::MAX`. Said because the two casts
        // above state their bound, and an unbounded name would state a length
        // the header does not have, while `len()` and `payload_span` stayed
        // right, so the file would be wrong in exactly the place nothing
        // here checks.
        out.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
        out.extend_from_slice(
            &(if self.zip64 {
                ZIP64_LOCAL_EXTRA as u16
            } else {
                0
            })
            .to_le_bytes(),
        );
        out.extend_from_slice(entry.name.as_bytes());
        if self.zip64 {
            out.extend_from_slice(&0x0001u16.to_le_bytes());
            out.extend_from_slice(&16u16.to_le_bytes());
            out.extend_from_slice(&entry.size.to_le_bytes());
            out.extend_from_slice(&entry.size.to_le_bytes());
        }
        out
    }

    /// Every central-directory record, then the end record(s).
    pub(crate) fn trailer(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity((self.total - self.directory_at) as usize);
        for (index, entry) in self.entries.iter().enumerate() {
            out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            // Made by: 0x031E — Unix, zip 3.0. Needed: as the local header.
            out.extend_from_slice(&0x031Eu16.to_le_bytes());
            out.extend_from_slice(&if self.zip64 { 45u16 } else { 20u16 }.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // flags
            out.extend_from_slice(&0u16.to_le_bytes()); // STORE
            out.extend_from_slice(&DOS_TIME.to_le_bytes());
            out.extend_from_slice(&DOS_DATE.to_le_bytes());
            out.extend_from_slice(&entry.crc32.to_le_bytes());
            let stated = if self.zip64 { U32_MAX } else { entry.size };
            out.extend_from_slice(&(stated as u32).to_le_bytes());
            out.extend_from_slice(&(stated as u32).to_le_bytes());
            // Bounded by the caller — see `local_header`.
            out.extend_from_slice(&(entry.name.len() as u16).to_le_bytes());
            out.extend_from_slice(
                &(if self.zip64 {
                    ZIP64_CENTRAL_EXTRA as u16
                } else {
                    0
                })
                .to_le_bytes(),
            );
            out.extend_from_slice(&0u16.to_le_bytes()); // comment length
            out.extend_from_slice(&0u16.to_le_bytes()); // disk number
            out.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
            // 0o644 in the high half, the DOS archive bit clear.
            out.extend_from_slice(&0x81A4_0000u32.to_le_bytes());
            let offset = if self.zip64 {
                U32_MAX
            } else {
                self.offsets[index]
            };
            out.extend_from_slice(&(offset as u32).to_le_bytes());
            out.extend_from_slice(entry.name.as_bytes());
            if self.zip64 {
                out.extend_from_slice(&0x0001u16.to_le_bytes());
                out.extend_from_slice(&24u16.to_le_bytes());
                out.extend_from_slice(&entry.size.to_le_bytes());
                out.extend_from_slice(&entry.size.to_le_bytes());
                out.extend_from_slice(&self.offsets[index].to_le_bytes());
            }
        }

        let directory_size = self.total
            - self.directory_at
            - if self.zip64 {
                ZIP64_END_RECORD + ZIP64_LOCATOR + END_RECORD
            } else {
                END_RECORD
            };
        let count = self.entries.len() as u64;

        if self.zip64 {
            let zip64_end_at = self.directory_at + directory_size;
            out.extend_from_slice(&0x0606_4b50u32.to_le_bytes());
            out.extend_from_slice(&(ZIP64_END_RECORD - 12).to_le_bytes()); // size of the rest
            out.extend_from_slice(&0x031Eu16.to_le_bytes());
            out.extend_from_slice(&45u16.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes()); // this disk
            out.extend_from_slice(&0u32.to_le_bytes()); // disk with the directory
            out.extend_from_slice(&count.to_le_bytes());
            out.extend_from_slice(&count.to_le_bytes());
            out.extend_from_slice(&directory_size.to_le_bytes());
            out.extend_from_slice(&self.directory_at.to_le_bytes());

            out.extend_from_slice(&0x0706_4b50u32.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes()); // disk with the zip64 end record
            out.extend_from_slice(&zip64_end_at.to_le_bytes());
            out.extend_from_slice(&1u32.to_le_bytes()); // total disks
        }

        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // this disk
        out.extend_from_slice(&0u16.to_le_bytes()); // disk with the directory
        let stated_count = if self.zip64 { u16::MAX } else { count as u16 };
        out.extend_from_slice(&stated_count.to_le_bytes());
        out.extend_from_slice(&stated_count.to_le_bytes());
        let stated_size = if self.zip64 { U32_MAX } else { directory_size };
        out.extend_from_slice(&(stated_size as u32).to_le_bytes());
        let stated_at = if self.zip64 {
            U32_MAX
        } else {
            self.directory_at
        };
        out.extend_from_slice(&(stated_at as u32).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // comment length
        out
    }
}

/// The part of one run of output bytes that falls inside `[from, upto)`.
///
/// **This is the whole of spec §5's resume.** A generated download does not
/// seek: it walks the same headers, payloads and trailer it would have sent
/// from the start, keeps a running absolute offset, and passes every run
/// through here. Runs before the window come back empty and cost nothing but
/// the read that produced them; the run the window starts in is trimmed at the
/// front; everything after `upto` is empty again, so a bounded range stops
/// producing rather than running to the end.
///
/// What that leaves on the table, deliberately: the *read* of a payload before
/// the window still happens, so resuming at 300 MB inflates the first 300 MB
/// and discards it. The better version resolves an offset straight to
/// `(entry, offset within entry)` — `Plan::payload_span` is already the map it
/// needs — and is deferred behind a measurement, because the contract does not
/// change when it lands. Spec §5 and §12.
///
/// `region_start` is where `bytes` begins in the output. `upto` is exclusive.
pub(crate) fn clipped(bytes: &[u8], region_start: u64, from: u64, upto: u64) -> &[u8] {
    let region_end = region_start + bytes.len() as u64;
    if region_end <= from || region_start >= upto || from >= upto {
        return &[];
    }
    let start = from.saturating_sub(region_start) as usize;
    let end = (upto.min(region_end) - region_start) as usize;
    &bytes[start..end]
}

#[cfg(test)]
mod tests;
