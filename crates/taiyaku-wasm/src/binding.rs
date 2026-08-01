//! The binding's logic, as ordinary Rust.
//!
//! Kept apart from the `wasm_bindgen` shell in `lib.rs` so that `cargo test`
//! exercises the code that ships rather than a native stand-in for it.

use taiyaku_core::{Half, Sidecar, Slot, Via, route};

/// A parsed sidecar, held so a reader can ask it many questions without
/// re-parsing the document.
///
/// **It does not cache the path between two editions.** `resolve` builds a
/// fresh `Route` per call, which rebuilds both slot tables and, where the
/// answer composes through the hub, recomposes the run lists —
/// `taiyaku_core::Route` exists precisely so a caller can pay that once. One
/// swap per keypress does not notice; a filmstrip resolving per tile would.
/// Tracked in `docs/status.md` rather than fixed here.
#[derive(Debug, Clone)]
pub struct Binding {
    sidecar: Sidecar,
}

/// Where a slot landed, and how the answer was found.
///
/// `via` travels with the answer because a composed correspondence is weaker
/// than a stored one — `route.rs` reports it for the same reason, and a reader
/// that cannot tell them apart has been told less than the document knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub page: u32,
    pub half: Option<Half>,
    pub via: String,
}

impl Binding {
    /// Parse a sidecar from its JSON text.
    pub fn from_json(json: &str) -> Result<Self, String> {
        Ok(Self {
            sidecar: Sidecar::from_json(json)?,
        })
    }

    /// Every edition this document defines, unordered.
    pub fn editions(&self) -> Vec<String> {
        self.sidecar.editions.keys().cloned().collect()
    }

    /// How many slots an edition presents, or `None` if it has no such edition.
    ///
    /// Slots rather than pages, because a joined spread is two of them and the
    /// slot is the addressed unit everywhere else in the format.
    pub fn slot_count(&self, edition: &str) -> Option<u32> {
        let reading = self.sidecar.work.reading;
        Some(self.sidecar.editions.get(edition)?.slots(reading).len() as u32)
    }

    /// Resolve one slot of `from` into `to`.
    ///
    /// `None` covers three different situations on purpose — no such edition,
    /// no path between them, and no counterpart for this slot. The shell in
    /// `lib.rs` passes that through as `undefined`; a reader distinguishing
    /// them would ask `slot_count` first.
    pub fn resolve(&self, from: &str, to: &str, page: u32, half: Option<Half>) -> Option<Resolved> {
        let r = route(&self.sidecar, from, to)?;
        let landed = r.resolve_slot(Slot { page, half })?;
        Some(Resolved {
            page: landed.page,
            half: landed.half,
            via: via_word(&r.via).to_string(),
        })
    }
}

/// A stable lowercase name per path, for a JS caller to branch on.
///
/// Spelled out rather than derived from `Debug`, which is a formatting detail
/// nothing promises to keep.
fn via_word(via: &Via) -> &'static str {
    match via {
        Via::Identity => "identity",
        Via::Direct => "direct",
        Via::Inverted => "inverted",
        Via::Hub(_) => "hub",
    }
}

/// Two editions, one spread, one run that ends mid-spread — the smallest
/// document that can tell a half-aware resolve from a page-aware one.
///
/// **The run sides must be equal in slots, not pages.** `b`'s page 2 is a
/// spread, so `"0-2R"` is three slots (`0`, `1`, `2R`) and the `a` side has
/// to be three as well; `taiyaku-core`'s `runs_of` refuses an unequal pair
/// and `route` then answers `None`. That is why `a` starts at 1 — which
/// leaves `a`'s slot 0 deliberately unpaired, and the agreement check below
/// is stronger for having an unpaired slot in range.
#[cfg(test)]
pub(crate) const FIXTURE: &str = r#"{
      "taiyaku": 1,
      "work": { "title": "T", "reading": "rtl" },
      "editions": {
        "a": { "label": "A", "members": [{ "path": "a.cbz", "sha256": "00", "pages": 8 }] },
        "b": { "label": "B", "members": [{ "path": "b.cbz", "sha256": "01", "pages": 8 }], "spreads": [2] }
      },
      "hub": "a",
      "alignments": [
        {
          "from": "a", "to": "b", "by": "solver",
          "runs": [{ "a": "1-3", "b": "0-2R" }, { "a": "5-7", "b": "5-7" }]
        }
      ]
    }"#;

/// Three editions in a star, so a spoke-to-spoke lookup has no stored edge and
/// must compose through the hub. Nothing here is a spread: this fixture exists
/// to reach `Via::Hub`, which two editions cannot.
#[cfg(test)]
pub(crate) const STAR_FIXTURE: &str = r#"{
  "taiyaku": 1,
  "work": { "title": "T", "reading": "rtl" },
  "editions": {
    "a": { "label": "A", "members": [{ "path": "a.cbz", "sha256": "00", "pages": 8 }] },
    "b": { "label": "B", "members": [{ "path": "b.cbz", "sha256": "01", "pages": 8 }] },
    "c": { "label": "C", "members": [{ "path": "c.cbz", "sha256": "02", "pages": 8 }] }
  },
  "hub": "a",
  "alignments": [
    { "from": "a", "to": "b", "by": "solver", "runs": [{ "a": "0-7", "b": "0-7" }] },
    { "from": "a", "to": "c", "by": "solver", "runs": [{ "a": "0-7", "b": "0-7" }] }
  ]
}"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sidecar_parses_and_lists_its_editions() {
        let b = Binding::from_json(FIXTURE).expect("parses");
        let mut names = b.editions();
        names.sort();
        assert_eq!(names, vec!["a".to_string(), "b".to_string()]);
    }

    /// Refusal names the problem. A binding that answered `None` for malformed
    /// input would make "no such work" and "this file is broken" the same
    /// answer, and the browser is the worst place to tell them apart.
    #[test]
    fn malformed_json_is_refused_with_a_message() {
        let refused = Binding::from_json("{ not json").expect_err("must not invent a document");
        assert!(
            refused.contains("not a readable taiyaku.json"),
            "the refusal says what kind of file it failed to read: {refused}"
        );
        assert!(
            refused.contains("line") && refused.contains("column"),
            "and where — serde locates the fault, and a binding that dropped \
             that would leave a browser with nothing to act on: {refused}"
        );
    }

    /// Slot count, not page count: `b` has 8 pages of which one is a joined
    /// spread, so it presents 9 slots. A reader sizing a filmstrip on pages
    /// would come up one short on every spread.
    #[test]
    fn slot_count_counts_slots_rather_than_pages() {
        let b = Binding::from_json(FIXTURE).expect("parses");
        assert_eq!(b.slot_count("a"), Some(8));
        assert_eq!(b.slot_count("b"), Some(9));
        assert_eq!(b.slot_count("nosuch"), None);
    }

    /// The half travels in and out, and `via` says how the answer was found.
    #[test]
    fn a_slot_resolves_with_its_half_and_reports_the_path() {
        let b = Binding::from_json(FIXTURE).expect("parses");
        let got = b
            .resolve("a", "b", 3, None)
            .expect("a pairs with b directly");
        assert_eq!(got.page, 2);
        assert_eq!(got.half, Some(Half::Right), "run `1-3` ends on b's `2R`");
        assert_eq!(got.via, "direct");
    }

    /// Every slot of every ordered pair, through the binding and through the
    /// core, must agree.
    ///
    /// **What this catches, and what it does not.** `Binding::resolve` calls
    /// `route` and `Route::resolve_slot` and copies the answer into
    /// `Resolved`, so both sides of this comparison reach the same core
    /// functions: a bug in spread arithmetic or run composition would be wrong
    /// identically on both sides and this would stay green. What it does catch
    /// is drift in the binding's own plumbing — a transposed `from`/`to`, a
    /// dropped half, a wrong field copied into `Resolved`. It is not a value
    /// compared with itself: the two sides parse the document separately and
    /// build their own `Route`.
    ///
    /// Exhaustive rather than sampled because the bug it *can* catch is a
    /// single slot answering differently, which sampling finds only by luck.
    #[test]
    fn the_binding_agrees_with_the_core_on_every_slot() {
        let b = Binding::from_json(FIXTURE).expect("parses");
        let s = taiyaku_core::Sidecar::from_json(FIXTURE).expect("parses");
        let reading = s.work.reading;

        let mut compared = 0;
        for from in b.editions() {
            for to in b.editions() {
                let Some(r) = taiyaku_core::route(&s, &from, &to) else {
                    continue;
                };
                for slot in s.editions[&from].slots(reading) {
                    let native = r.resolve_slot(slot);
                    let bound = b.resolve(&from, &to, slot.page, slot.half);
                    assert_eq!(
                        native.map(|n| (n.page, n.half)),
                        bound.map(|x| (x.page, x.half)),
                        "{from} -> {to}, slot {slot:?} disagrees"
                    );
                    compared += 1;
                }
            }
        }
        // 8 slots in `a` and 9 in `b` — `b`'s page 2 is a spread — across four
        // ordered pairs, all of which have a route in a two-edition star.
        // Pinned rather than merely non-zero: an edit that narrowed the loop to
        // a subset of pairs would still compare *something* and stay green
        // while quietly dropping the "every ordered pair" claim above.
        assert_eq!(
            compared, 34,
            "the fixture's geometry changed, or the loop stopped covering it"
        );
    }

    /// The paths a two-edition star can produce, named the way JS will branch
    /// on them. `Via::Hub` needs three editions, so it is not reachable from
    /// this fixture and is not asserted here.
    ///
    /// **Slot 1, not slot 0, for the `a -> b` case.** The fixture's first run
    /// starts at `a`'s slot 1, so slot 0 is unpaired and `a -> b` answers
    /// `None` for it — `via` is a property of the path, but this call can only
    /// report one when the slot actually lands somewhere.
    #[test]
    fn each_path_reports_its_own_word() {
        let b = Binding::from_json(FIXTURE).expect("parses");
        assert_eq!(
            b.resolve("a", "a", 0, None).expect("identity").via,
            "identity"
        );
        assert_eq!(b.resolve("a", "b", 1, None).expect("stored").via, "direct");
        assert_eq!(
            b.resolve("b", "a", 0, None).expect("reversed").via,
            "inverted"
        );
    }

    /// The fourth `via` word, which two editions cannot reach. `b` and `c` have
    /// no stored edge, so the answer composes through the hub — and a reader is
    /// entitled to know that, because a composed correspondence is weaker than
    /// a stored one.
    #[test]
    fn a_spoke_to_spoke_lookup_reports_the_hub() {
        let b = Binding::from_json(STAR_FIXTURE).expect("parses");
        let got = b.resolve("b", "c", 0, None).expect("composes through `a`");
        assert_eq!(got.via, "hub");
        assert_eq!(
            got.page, 0,
            "the star's runs are identity, so the page holds"
        );
    }
}
