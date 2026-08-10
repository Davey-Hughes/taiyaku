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
#[wasm_bindgen(getter_with_clone)]
#[derive(Debug, Clone)]
pub struct WasmSlot {
    pub page: u32,
    pub half: Option<String>,
    pub via: String,
}

/// Where a slot sits inside one edition: a page, and which half of it.
///
/// Distinct from `WasmSlot`, which additionally carries the `via` of a
/// resolution. A position is not the answer to a resolution and should not
/// claim a path it never walked.
#[wasm_bindgen(getter_with_clone)]
#[derive(Debug, Clone)]
pub struct WasmPosition {
    pub page: u32,
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
#[wasm_bindgen(getter_with_clone)]
#[derive(Debug, Clone)]
pub struct WasmChapter {
    pub member: u32,
    pub of: u32,
    pub path: String,
}

#[wasm_bindgen]
impl WasmSidecar {
    /// Parse a sidecar from its JSON text, throwing on malformed input.
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
    #[wasm_bindgen(js_name = resolveSlot)]
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

    /// The flat slot index of a page and half, or `undefined`.
    #[wasm_bindgen(js_name = indexOf)]
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
        self.inner.chapter_at(edition, index).map(|at| WasmChapter {
            member: at.member as u32,
            of: at.of as u32,
            path: at.path,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The shell converts and does not decide. This pins the one piece of
    /// judgement it does carry — how a half crosses the language boundary —
    /// because a silent disagreement about `"L"` would show a reader the wrong
    /// half of every spread.
    #[test]
    fn a_half_crosses_the_boundary_as_l_or_r() {
        assert_eq!(half_from_str(None), Ok(None));
        assert_eq!(half_from_str(Some("L")), Ok(Some(taiyaku_core::Half::Left)));
        assert_eq!(
            half_from_str(Some("R")),
            Ok(Some(taiyaku_core::Half::Right))
        );
        assert_eq!(half_to_str(Some(taiyaku_core::Half::Left)), Some("L"));
        assert_eq!(half_to_str(Some(taiyaku_core::Half::Right)), Some("R"));
        assert_eq!(half_to_str(None), None);
        assert!(
            half_from_str(Some("middle")).is_err(),
            "an unknown half is refused rather than silently read as a whole page"
        );
    }

    /// The shell forwards four positional arguments and copies three fields,
    /// and until now nothing exercised either. The agreement test in `binding`
    /// sits one layer below this and cannot see a transposed `from`/`to` or a
    /// wrong field copied into `WasmSlot`.
    ///
    /// `.ok()` rather than `.unwrap()` on the `Result`: formatting a `JsError`
    /// off-target would call into JavaScript, and the happy path never
    /// constructs one. Clippy's `ok_expect` does not know that, and its
    /// suggested `.expect()` is the very call this test declines to make.
    #[allow(clippy::ok_expect)]
    #[test]
    fn the_shell_forwards_its_arguments_and_copies_every_field() {
        let s = WasmSidecar::from_json(crate::binding::FIXTURE)
            .ok()
            .expect("the fixture parses");
        let got = s
            .resolve_slot("a", "b", 3, None)
            .ok()
            .expect("a half of `None` is accepted")
            .expect("slot 3 of `a` lands in `b`");
        assert_eq!(got.page, 2, "run `1-3` ends on b's page 2");
        assert_eq!(got.half, Some("R".to_string()), "and on its right half");
        assert_eq!(got.via, "direct", "a stored edge, walked as stored");
    }

    /// The three forwarders that carry a value straight across. `binding`'s own
    /// tests pin what each answers; nothing pinned that *this* layer asks the
    /// right question with the right argument, so all three were replaceable by
    /// a constant without a test noticing.
    ///
    /// `editions` and `credits` are asserted against exact contents rather than
    /// a length, and `credits` against a list that is neither empty nor `[0]`
    /// nor `[1]`, because those are precisely the constants a stub returns.
    #[allow(clippy::ok_expect)]
    #[test]
    fn the_value_forwarders_ask_the_binding_rather_than_answering_themselves() {
        let s = WasmSidecar::from_json(crate::binding::FIXTURE)
            .ok()
            .expect("the fixture parses");
        assert_eq!(s.editions(), vec!["a".to_string(), "b".to_string()]);
        assert_eq!(s.slot_count("a"), Some(8), "eight pages, no spreads");
        assert_eq!(
            s.slot_count("b"),
            Some(9),
            "eight pages, one of them joined, so nine slots"
        );
        assert_eq!(
            s.slot_count("no-such-edition"),
            None,
            "an unknown edition is absent, not zero"
        );

        let c = WasmSidecar::from_json(crate::binding::SIDECAR_WITH_CREDITS)
            .ok()
            .expect("the fixture parses");
        assert_eq!(c.credits("fan"), vec![3, 4]);
        assert!(
            c.credits("no-such-edition").is_empty(),
            "an unknown edition has no credit pages rather than an error"
        );
    }

    /// `slotAt` and `indexOf` are inverses across the boundary, and each adds
    /// its own conversion on the way — `slotAt` spells the half with
    /// `half_to_str`, `indexOf` reads it back with `half_from_str`. Pinning
    /// them against each other catches a stub in either direction and a half
    /// that crosses as the wrong side, which is the failure that would show a
    /// reader the opposite page of every spread.
    ///
    /// Slot 2 of `b` is deliberate: it is the first half of the joined spread,
    /// so the assertion is on an index that is neither `0` nor `1`.
    #[allow(clippy::ok_expect)]
    #[test]
    fn slot_at_and_index_of_are_inverses_across_the_boundary() {
        let s = WasmSidecar::from_json(crate::binding::FIXTURE)
            .ok()
            .expect("the fixture parses");

        let at = s.slot_at("b", 2).expect("b presents nine slots");
        assert_eq!(at.page, 2, "the joined spread");
        assert_eq!(
            at.half,
            Some("R".to_string()),
            "right half first, reading rtl"
        );
        assert_eq!(
            s.index_of("b", at.page, at.half.clone())
                .ok()
                .expect("`R` is a half"),
            Some(2),
            "and back to the index it came from"
        );

        let other = s.slot_at("b", 3).expect("the spread's second half");
        assert_eq!((other.page, other.half.as_deref()), (2, Some("L")));

        assert!(s.slot_at("b", 999).is_none(), "past the end of the edition");
        assert!(
            s.slot_at("no-such-edition", 0).is_none(),
            "an unknown edition presents no slots"
        );

        // `index_of`'s refusal of an unknown half is *not* asserted here, and
        // the reason is a hazard rather than an oversight: the error arm builds
        // a `JsError`, and constructing one on a non-wasm target panics with
        // "cannot call wasm-bindgen imported functions on non-wasm targets" —
        // not only formatting it, as the note on the test above suggests.
        // `a_half_crosses_the_boundary_as_l_or_r` pins that refusal one layer
        // down, in `half_from_str`, which is ordinary Rust and can be asked.
    }

    /// The one piece of judgement this shell adds for `chapterAt`: `usize`
    /// does not cross the boundary, so `member` and `of` convert to `u32`.
    /// `binding`'s own test pins the lookup; this pins the conversion, the
    /// same division `a_half_crosses_the_boundary_as_l_or_r` draws for a half.
    #[allow(clippy::ok_expect)]
    #[test]
    fn chapter_at_crosses_the_boundary_as_u32_fields() {
        let s = WasmSidecar::from_json(crate::binding::CHAPTERED_FIXTURE)
            .ok()
            .expect("the fixture parses");
        let at = s
            .chapter_at("volume", 0)
            .expect("volume slot 0 has a chapter");
        assert_eq!(at.member, 0);
        assert_eq!(at.of, 3);
        assert_eq!(at.path, "c001.cbz");
        assert!(
            s.chapter_at("volume", 999).is_none(),
            "past the end of the edition"
        );
    }
}
