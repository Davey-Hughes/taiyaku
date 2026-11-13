//! WebAssembly binding over `taiyaku-core`.
//!
//! `binding` holds the logic as ordinary Rust and is what the tests exercise.
//! Everything here converts types across the JS boundary and decides nothing,
//! so that the behaviour under test is the behaviour that ships.

pub mod binding;

use taiyaku_core::Half;
use wasm_bindgen::prelude::*;

/// A parsed sidecar, as held by a JS caller.
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct WasmSidecar {
    inner: binding::Binding,
}

/// Where a slot landed. `half` is `"L"`, `"R"`, or absent for a whole page.
///
/// `getter_with_clone` sits on the fields that need it rather than on the
/// struct, here and on the two structs below: on the struct it also gives the
/// `u32` fields a getter that calls `.clone()` on a `Copy` value, which
/// clippy's `clone_on_copy` rejects under `-D warnings` from Rust 1.99. A
/// `Copy` field's plain getter is the same getter to JavaScript.
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct WasmSlot {
    pub page: u32,
    #[wasm_bindgen(getter_with_clone)]
    pub half: Option<String>,
    #[wasm_bindgen(getter_with_clone)]
    pub via: String,
}

/// Where a slot sits inside one edition: a page, and which half of it.
///
/// Distinct from `WasmSlot`, which additionally carries the `via` of a
/// resolution. A position is not the answer to a resolution and should not
/// claim a path it never walked.
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct WasmPosition {
    pub page: u32,
    #[wasm_bindgen(getter_with_clone)]
    pub half: Option<String>,
}

/// One page of one member: the anchor a note or a position is written with (O240).
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct WasmAt {
    #[wasm_bindgen(getter_with_clone)]
    pub sha256: String,
    pub nth: u32,
    pub page: u32,
    /// `"left"`, `"right"`, or absent for a whole page: the wire's spelling.
    #[wasm_bindgen(getter_with_clone)]
    pub half: Option<String>,
}

/// Which chapter a slot falls in: `member`'s zero-based index among `of`
/// members, and the member's path as the sidecar records it.
///
/// `member` and `of` cross as `u32`. `taiyaku_core::ChapterAt` holds them as
/// `usize`, which does not cross the JS boundary, so `chapter_at` below casts
/// them deliberately rather than leaving the conversion to whatever wasm-
/// bindgen would otherwise do with a `usize` field. The cast cannot lose
/// information in practice: it is bounded by how many files a release group
/// actually splits a volume into, the same "202 slots is the longest edition
/// measured" scale `index_of`'s doc comment cites — nowhere near `u32::MAX`.
#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct WasmChapter {
    pub member: u32,
    pub of: u32,
    #[wasm_bindgen(getter_with_clone)]
    pub path: String,
}

#[wasm_bindgen]
impl WasmSidecar {
    /// Parse a sidecar from its JSON text, throwing on malformed input.
    ///
    /// # Errors
    ///
    /// Throws (as a JS `Error`, via `JsError`) if `json` does not parse as a
    /// taiyaku sidecar document — the same condition `Binding::from_json`
    /// reports. The thrown message names what failed to read and the line
    /// and column the parser found the fault at, so a JS caller handed a
    /// malformed `taiyaku.json` sees why rather than a bare failure.
    #[wasm_bindgen(js_name = fromJson)]
    pub fn from_json(json: &str) -> Result<WasmSidecar, JsError> {
        binding::Binding::from_json(json)
            .map(|inner| WasmSidecar { inner })
            .map_err(|e| JsError::new(&e))
    }

    /// Every edition this document defines.
    pub fn editions(&self) -> Vec<String> {
        self.inner.editions()
    }

    /// How many slots an edition presents, or `undefined` if it has no such
    /// edition.
    #[wasm_bindgen(js_name = slotCount)]
    pub fn slot_count(&self, edition: &str) -> Option<u32> {
        self.inner.slot_count(edition)
    }

    /// Resolve one slot of `from` into `to`, or `undefined` when there is no
    /// counterpart, no path, or no such edition.
    ///
    /// # Errors
    ///
    /// Throws if `half` is present but is neither `"L"` nor `"R"` — the
    /// thrown message names the string it received and the two values it
    /// accepts. An unknown `from`/`to` edition, no path between them, or a
    /// slot with no counterpart is not an error: each resolves to
    /// `undefined`, as the doc above says.
    #[wasm_bindgen(js_name = resolveSlot)]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "half: &str does not compile here: #[wasm_bindgen] on this impl block \
                  expands resolveSlot's Option<T> parameter through wasm_bindgen::convert:: \
                  OptionFromWasmAbi, which is implemented for String (see wasm-bindgen \
                  0.2.126, src/convert/slices.rs) but not for &str. Tried Option<&str> \
                  directly: `cargo clippy -p taiyaku-wasm --all-targets` fails E0277, \"the \
                  trait bound `&str: wasm_bindgen::convert::OptionFromWasmAbi` is not \
                  satisfied\", on the native (non-wasm32) lib and lib-test targets — the \
                  wasm32-unknown-unknown build alone does not surface it, which is why this \
                  needs checking against --all-targets specifically."
    )]
    pub fn resolve_slot(
        &self,
        from: &str,
        to: &str,
        page: u32,
        half: Option<String>,
    ) -> Result<Option<WasmSlot>, JsError> {
        let half = half_from_str(half.as_deref()).map_err(|e| JsError::new(&e))?;
        Ok(self.inner.resolve(from, to, page, half).map(|r| WasmSlot {
            page: r.page,
            half: half_to_str(r.half).map(str::to_string),
            via: r.via,
        }))
    }

    /// The page and half at a flat slot index, or `undefined`.
    #[wasm_bindgen(js_name = slotAt)]
    pub fn slot_at(&self, edition: &str, index: u32) -> Option<WasmPosition> {
        self.inner.slot_at(edition, index).map(|slot| WasmPosition {
            page: slot.page,
            half: half_to_str(slot.half).map(str::to_string),
        })
    }

    /// The anchor of the page a flat slot index shows, or `undefined`.
    #[wasm_bindgen(js_name = memberPage)]
    pub fn member_page(&self, edition: &str, index: u32) -> Option<WasmAt> {
        self.inner.member_page(edition, index).map(|at| WasmAt {
            sha256: at.sha256,
            nth: at.nth,
            page: at.page,
            half: half_to_wire(at.half).map(str::to_string),
        })
    }

    /// The flat slot index of a page and half, or `undefined`.
    ///
    /// # Errors
    ///
    /// Throws if `half` is present but is neither `"L"` nor `"R"` — the same
    /// condition and message as `resolve_slot` (`resolveSlot` in JS). An
    /// unknown `edition`, or a `(page, half)` the edition does not present,
    /// is not an error: it answers `undefined`.
    #[wasm_bindgen(js_name = indexOf)]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "half: &str does not compile here, for the same reason as resolveSlot above: \
                  wasm_bindgen::convert::OptionFromWasmAbi is implemented for String, not \
                  &str, so #[wasm_bindgen] on this impl block fails E0277 on the native \
                  (non-wasm32) targets with Option<&str>. Verified by trying it."
    )]
    pub fn index_of(
        &self,
        edition: &str,
        page: u32,
        half: Option<String>,
    ) -> Result<Option<u32>, JsError> {
        let half = half_from_str(half.as_deref()).map_err(|e| JsError::new(&e))?;
        Ok(self
            .inner
            .index_of(edition, taiyaku_core::Slot { page, half }))
    }

    /// The edition's credit pages, as page indices — empty for an unknown
    /// edition or one with no list.
    pub fn credits(&self, edition: &str) -> Vec<u32> {
        self.inner.credits(edition)
    }

    /// Which chapter a flat slot index falls in, or `undefined`.
    #[wasm_bindgen(js_name = chapterAt)]
    pub fn chapter_at(&self, edition: &str, index: u32) -> Option<WasmChapter> {
        self.inner.chapter_at(edition, index).map(|at| {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "at.member is a member index bounded by edition.members.len() \
                          (ChapterAt::member, taiyaku-core/src/sidecar.rs) — how many files a \
                          release group split a volume into, nowhere near u32::MAX. Tried \
                          taiyaku-core::solve's page_index pattern — a named helper that \
                          panics on the impossible case — and rejected it here: this crate \
                          compiles to wasm32-unknown-unknown and is loaded by a JS caller, \
                          where a panic is a trap that poisons the whole module rather than \
                          an ordinary Rust unwind, and edition.members.len() is a JS-supplied \
                          taiyaku.json's own member-array length, not data this shell can \
                          treat as a trusted in-process invariant. A JsError would fix that, \
                          but chapter_at returns Option<WasmChapter> end to end (here and in \
                          Binding::chapter_at, binding.rs), and widening it to \
                          Result<_, JsError> for an unreachable case is an API change outside \
                          a behaviour-preserving cleanup."
            )]
            let member = at.member as u32;
            #[expect(
                clippy::cast_possible_truncation,
                reason = "at.of is edition.members.len() itself (ChapterAt::of, \
                          taiyaku-core/src/sidecar.rs) — the same bounded member count as \
                          at.member above, and the same rejection of a page_index-style \
                          helper applies: a panic here would be a wasm trap over data that \
                          traces back to a JS-supplied taiyaku.json, and chapter_at's \
                          Option<WasmChapter> signature would need to become fallible to \
                          report it any other way."
            )]
            let of = at.of as u32;
            WasmChapter {
                member,
                of,
                path: at.path,
            }
        })
    }
}

/// `"L"` / `"R"` / absent, into a half.
///
/// An unrecognised string is refused rather than read as a whole page: a typo
/// that quietly means "the first half" would show the wrong side of every
/// spread and look like an alignment bug.
fn half_from_str(half: Option<&str>) -> Result<Option<Half>, String> {
    match half {
        None => Ok(None),
        Some("L") => Ok(Some(Half::Left)),
        Some("R") => Ok(Some(Half::Right)),
        Some(other) => Err(format!("`{other}` is not a half; use `L`, `R`, or nothing")),
    }
}

/// The wire spelling of a half. `None` is a whole page.
///
/// Spelled out rather than derived from `Debug`, for the reason `via_word`
/// gives: a `Debug` rendering is a formatting detail nothing promises to keep,
/// and this one crosses a boundary.
fn half_to_str(half: Option<Half>) -> Option<&'static str> {
    match half {
        Some(Half::Left) => Some("L"),
        Some(Half::Right) => Some("R"),
        None => None,
    }
}

/// An anchor's half as the notes and progress wire spells it, the core's
/// `Half` serde spelling. It is not `half_to_str`'s `"L"`/`"R"`, which
/// `resolveSlot` and `indexOf` read back, so the web copies a `WasmAt`'s
/// fields onto the wire and never converts its half.
fn half_to_wire(half: Option<Half>) -> Option<&'static str> {
    match half {
        Some(Half::Left) => Some("left"),
        Some(Half::Right) => Some("right"),
        None => None,
    }
}

#[cfg(test)]
mod tests;
