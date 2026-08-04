//! The binding's logic, as ordinary Rust.
//!
//! Kept apart from the `wasm_bindgen` shell in `lib.rs` so that `cargo test`
//! exercises the code that ships rather than a native stand-in for it.

use std::cell::RefCell;

use taiyaku_core::{ChapterAt, Half, Route, Sidecar, Slot, Via, route};

/// A parsed sidecar, held so a reader can ask it many questions without
/// re-parsing the document.
///
/// **It memoises the last `(from, to)` pair's `Route`.** `resolve` used to
/// build a fresh `Route` per call, which rebuilds both slot tables and, where
/// the answer composes through the hub, recomposes the run lists —
/// `taiyaku_core::Route` exists precisely so a caller can pay that once.
/// `route_cache` holds exactly one pair rather than a map keyed on every pair
/// ever asked about, because the caller this was measured against —
/// `web/src/nearest.ts`'s `nearestPaired`, scanning either side of a gap —
/// fixes `from`/`to` for the whole scan and varies only the slot. One swap
/// per keypress does not notice either way; up to `slotCount` resolves during
/// one gap scan does — see `docs/status.md`'s O14 entry for the measured
/// counts.
///
/// **What this does not cover: a caller alternating between two pairs.** A
/// different `(from, to)` replaces the memo outright — there is no second
/// slot — so `a→b`, `b→a`, `a→b`, ... on successive calls rebuilds every
/// time, the same as no cache at all. Nothing in this reader does that today:
/// a swap changes what is *shown*, not what `resolve` is asked, and
/// `nearestPaired`'s own scan holds `from`/`to` fixed throughout. A future
/// caller that interleaves pairs would need a small map instead, not a
/// bigger version of this field.
///
/// The cache needs interior mutability because `resolve` takes `&self`, not
/// `&mut self` — a reader holds one `Binding` and asks it many unrelated
/// questions through shared references (`slot_at`, `index_of`, `resolve`,
/// ...), so there is no exclusive borrow to hang a plain field update off of.
/// `RefCell` is enough, rather than something that would also survive shared
/// access from multiple threads, because **`Binding` is single-threaded by
/// construction on the JS side**: `wasm-bindgen` hands a `WasmSidecar` to
/// exactly one JS thread, and nothing in this crate spawns another to race
/// it. A `RefCell` would be the wrong tool the moment that stopped being
/// true.
#[derive(Debug, Clone)]
pub struct Binding {
    sidecar: Sidecar,
    route_cache: RefCell<Option<CachedRoute>>,
    /// How many times `resolve` actually rebuilt a `Route` rather than
    /// reusing the memo — a cache-miss counter, not anything a shipped
    /// caller reads. It exists purely so a test can pin the memo's effect:
    /// resolving many slots of one pair should build the route once, not
    /// once per slot, and a counter is the only way to observe that from
    /// outside without timing the call.
    #[cfg(test)]
    route_builds: std::cell::Cell<u32>,
}

/// One memoised `Route`, kept only for the `(from, to)` pair that produced
/// it. See `Binding`'s doc comment for what replaces this and what does not
/// benefit from it.
#[derive(Debug, Clone)]
struct CachedRoute {
    from: String,
    to: String,
    route: Route,
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
            route_cache: RefCell::new(None),
            #[cfg(test)]
            route_builds: std::cell::Cell::new(0),
        })
    }

    /// The `Route` between `from` and `to`, from the memo if it already holds
    /// this pair, or built and stored there otherwise.
    ///
    /// Returns an owned `Route` rather than a borrow through the `RefCell`,
    /// so a caller never has to juggle a `Ref` alongside a second, mutable
    /// borrow taken to update the cache on a miss. Cloning a cache hit is not
    /// the cost this exists to avoid — a `Route` is two slot tables and a run
    /// list, plain data with no lookups left to redo — building one from
    /// scratch is what reads the sidecar's editions, rebuilds both slot
    /// tables from their spreads, and, through the hub, recomposes the run
    /// list; see `Route`'s own doc comment in `route.rs`.
    fn cached_route(&self, from: &str, to: &str) -> Option<Route> {
        if let Some(cached) = self.route_cache.borrow().as_ref() {
            if cached.from == from && cached.to == to {
                return Some(cached.route.clone());
            }
        }
        #[cfg(test)]
        self.route_builds.set(self.route_builds.get() + 1);
        let built = route(&self.sidecar, from, to)?;
        *self.route_cache.borrow_mut() = Some(CachedRoute {
            from: from.to_string(),
            to: to.to_string(),
            route: built.clone(),
        });
        Some(built)
    }

    /// How many times `resolve` has actually rebuilt a `Route` since this
    /// `Binding` was created. Test-only — see the field's own doc comment for
    /// why it exists.
    #[cfg(test)]
    pub(crate) fn route_builds(&self) -> u32 {
        self.route_builds.get()
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

    /// The `(page, half)` pair at a flat slot index, or `None` for an unknown
    /// edition or an index past the end.
    ///
    /// A reader's position is a flat index — that is what `progress` stores —
    /// and `resolve` takes a pair, so something has to convert. It lives here
    /// rather than in TypeScript because the slot sequence is normative and a
    /// second derivation of it is the defect `taiyaku-read` was extracted to
    /// prevent, one language further out.
    pub fn slot_at(&self, edition: &str, index: u32) -> Option<Slot> {
        let reading = self.sidecar.work.reading;
        self.sidecar
            .editions
            .get(edition)?
            .slots(reading)
            .get(index as usize)
            .copied()
    }

    /// The flat index of a `(page, half)` pair, or `None` where this edition
    /// presents no such slot.
    ///
    /// A linear scan: the longest edition measured is 202 slots (One Piece
    /// v092's all-spread copy — `docs/status.md`'s "All-spread edition ↔
    /// ordinary" entry), and a map would have to be built per call or cached
    /// against a mutation this type does not have.
    pub fn index_of(&self, edition: &str, slot: Slot) -> Option<u32> {
        let reading = self.sidecar.work.reading;
        self.sidecar
            .editions
            .get(edition)?
            .slots(reading)
            .iter()
            .position(|candidate| *candidate == slot)
            .map(|found| found as u32)
    }

    /// Resolve one slot of `from` into `to`.
    ///
    /// `None` covers three different situations on purpose — no such edition,
    /// no path between them, and no counterpart for this slot. The shell in
    /// `lib.rs` passes that through as `undefined`; a reader distinguishing
    /// them would ask `slot_count` first.
    ///
    /// Routes through `cached_route` rather than calling `route` directly —
    /// see `Binding`'s doc comment for what that buys and what it does not.
    pub fn resolve(&self, from: &str, to: &str, page: u32, half: Option<Half>) -> Option<Resolved> {
        let r = self.cached_route(from, to)?;
        let landed = r.resolve_slot(Slot { page, half })?;
        Some(Resolved {
            page: landed.page,
            half: landed.half,
            via: via_word(&r.via).to_string(),
        })
    }

    /// Which chapter a flat slot index of `edition` falls in, or `None`.
    ///
    /// Takes an index rather than a `(page, half)` pair for the same reason
    /// `slot_at` does: that is what a caller holds, and this converts
    /// through it before asking the core. `member`/`of` stay `usize` here —
    /// `lib.rs`'s shell is where they cross into `u32`.
    pub fn chapter_at(&self, edition: &str, index: u32) -> Option<ChapterAt> {
        let slot = self.slot_at(edition, index)?;
        self.sidecar.chapter_at(edition, slot)
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

/// A two-edition document where `chapters` is assembled from three members
/// and `volume` from one — the shape a chapter-organised edition has. The
/// same document as `taiyaku_core::sidecar`'s `CHAPTERED`, so both crates'
/// tests agree on what "chapter-organised" looks like.
#[cfg(test)]
pub(crate) const CHAPTERED_FIXTURE: &str = r#"{
  "taiyaku": 1,
  "work": { "title": "T", "reading": "rtl" },
  "editions": {
    "volume": { "label": "V", "members": [{ "path": "v01.cbz", "sha256": "00", "pages": 6 }] },
    "chapters": { "label": "C", "members": [
      { "path": "c001.cbz", "sha256": "01", "pages": 2 },
      { "path": "c002.cbz", "sha256": "02", "pages": 2 },
      { "path": "c003.cbz", "sha256": "03", "pages": 2 }
    ] }
  },
  "hub": "volume",
  "alignments": [
    { "from": "volume", "to": "chapters", "by": "solver", "runs": [{ "a": "0-5", "b": "0-5" }] }
  ]
}"#;

/// Go-Toubun no Hanayome volume 1, raw against English — the same document
/// as `taiyaku_core::sidecar`'s `REAL`, duplicated rather than imported
/// because that constant is `pub(crate)` to `taiyaku-core` and this crate
/// sits on the far side of that boundary. It is real solver output, not a
/// hand-built shape: `raw-mono` is 195 pages with no spreads, `en-mono` is
/// 187 with seven, and the stored runs leave `raw-mono` slots 1 and 6 with no
/// counterpart at all — the genuine gap `docs/status.md`'s O14 entry
/// measures against, not a synthetic one.
#[cfg(test)]
pub(crate) const REAL_FIXTURE: &str = r#"{
  "taiyaku": 1,
  "work": { "title": "Go-Toubun no Hanayome", "reading": "rtl" },
  "editions": {
    "raw-mono": {
      "label": "Raw, monochrome",
      "members": [
        { "path": "Go-Toubun no Hanayome - v01.cbz", "sha256": "3f9a", "pages": 195 }
      ],
      "spreads": []
    },
    "en-mono": {
      "label": "English official, Kodansha",
      "members": [
        { "path": "Go-Toubun no Hanayome - v01 (2018).cbz", "sha256": "c104", "pages": 187 }
      ],
      "spreads": [4, 45, 46, 49, 90, 121, 152]
    }
  },
  "hub": "raw-mono",
  "alignments": [
    {
      "from": "raw-mono",
      "to": "en-mono",
      "by": "solver",
      "solver": { "hash": "dhash-16", "floor": 0.68 },
      "runs": [
        { "a": "0", "b": "0", "margin": 0.09 },
        { "a": "1" },
        { "a": "2-5", "b": "1-4R", "margin": 0.15 },
        { "a": "6" },
        { "a": "7-194", "b": "5-186", "margin": 0.17 }
      ]
    }
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

    /// `b`'s page 2 is a joined spread, so its slot sequence is
    /// 0, 1, 2R, 2L, 3, 4, 5, 6, 7 under `rtl` — nine slots for eight pages.
    /// Index 2 and index 3 are the two halves of one page, which is the whole
    /// reason a flat index cannot be a page number.
    #[test]
    fn slot_at_walks_a_spread_as_two_slots() {
        let b = Binding::from_json(FIXTURE).unwrap();
        assert_eq!(
            b.slot_at("b", 1),
            Some(Slot {
                page: 1,
                half: None
            })
        );
        assert_eq!(
            b.slot_at("b", 2),
            Some(Slot {
                page: 2,
                half: Some(Half::Right)
            })
        );
        assert_eq!(
            b.slot_at("b", 3),
            Some(Slot {
                page: 2,
                half: Some(Half::Left)
            })
        );
        assert_eq!(
            b.slot_at("b", 4),
            Some(Slot {
                page: 3,
                half: None
            })
        );
    }

    #[test]
    fn slot_at_refuses_an_index_past_the_end_and_an_unknown_edition() {
        let b = Binding::from_json(FIXTURE).unwrap();
        assert_eq!(b.slot_count("b"), Some(9));
        assert_eq!(b.slot_at("b", 9), None);
        assert_eq!(b.slot_at("nope", 0), None);
    }

    /// The two are inverses over every slot of every edition. This is the
    /// property the reader depends on: a swap converts index → pair → resolve →
    /// pair → index, and a mismatch anywhere silently moves the reader.
    #[test]
    fn index_of_inverts_slot_at_over_every_slot() {
        let b = Binding::from_json(FIXTURE).unwrap();
        for edition in ["a", "b"] {
            let count = b.slot_count(edition).unwrap();
            for index in 0..count {
                let slot = b.slot_at(edition, index).unwrap();
                assert_eq!(
                    b.index_of(edition, slot),
                    Some(index),
                    "{edition} slot {index}"
                );
            }
        }
    }

    /// The binding takes a flat index, not a `Slot` — `chapter_at` converts
    /// through `slot_at` first, the same as `resolve` converts through
    /// `route`.
    #[test]
    fn chapter_at_crosses_the_boundary_as_an_index_and_a_label() {
        let b = Binding::from_json(CHAPTERED_FIXTURE).unwrap();
        let at = b.chapter_at("volume", 0).unwrap();
        assert_eq!((at.member, at.of), (0, 3));
        assert_eq!(at.path, "c001.cbz");
        assert_eq!(b.chapter_at("volume", 999), None);
        assert_eq!(b.chapter_at("nope", 0), None);
    }

    #[test]
    fn index_of_refuses_a_slot_the_edition_does_not_present() {
        let b = Binding::from_json(FIXTURE).unwrap();
        // `a` has no spreads, so it presents no halved slot at all.
        assert_eq!(
            b.index_of(
                "a",
                Slot {
                    page: 0,
                    half: Some(Half::Right)
                }
            ),
            None
        );
        assert_eq!(
            b.index_of(
                "b",
                Slot {
                    page: 99,
                    half: None
                }
            ),
            None
        );
    }

    /// O14's own number, reproduced. `route.rs` says a reader paging a
    /// 195-page volume would pay for a route rebuild 195 times without a
    /// memo; this resolves every slot of `raw-mono` against `en-mono` — the
    /// real 195-slot edition — and checks that `route_builds` stops counting
    /// after the first. Before `resolve` routed through `cached_route`, this
    /// assertion failed with `route_builds() == 195`; it is the test that
    /// pinned the memo, not one added after the fact.
    #[test]
    fn resolving_every_slot_of_one_pair_builds_the_route_once() {
        let b = Binding::from_json(REAL_FIXTURE).expect("parses");
        let count = b.slot_count("raw-mono").expect("raw-mono exists");
        assert_eq!(count, 195, "the fixture's own claim about its size");
        for slot in 0..count {
            b.resolve("raw-mono", "en-mono", slot, None);
        }
        assert_eq!(
            b.route_builds(),
            1,
            "195 resolves of one pair must build the route once, not 195 times"
        );
    }

    /// The realistic worst case, not a synthetic one: `raw-mono` slot 1 has
    /// no counterpart in `en-mono` at all (the fixture's run `"a": "1"` with
    /// no `b` side) — the same gap `docs/status.md`'s O14 entry cites. Its
    /// neighbours, slots 0 and 2, do resolve, so `web/src/nearest.ts`'s
    /// `nearestPaired` finds both in one step each way: two resolves for the
    /// scan. Before `App.tsx`'s gap branch was changed to reuse what the scan
    /// already found, it resolved both offers a second time through
    /// `resolveOffered` — two more calls, same pair. All four are reproduced
    /// here directly against `Binding`, since `nearest.ts`'s own tests use a
    /// fake resolver and cannot observe a Rust-side rebuild.
    #[test]
    fn a_gap_scan_and_its_former_second_pass_build_the_route_once() {
        let b = Binding::from_json(REAL_FIXTURE).expect("parses");

        // The scan: nearestPaired checking slot 0 (before) and slot 2 (after)
        // from the gap at slot 1.
        assert!(b.resolve("raw-mono", "en-mono", 0, None).is_some());
        assert!(b.resolve("raw-mono", "en-mono", 2, None).is_some());
        // The second pass App.tsx no longer makes — kept here to show the
        // memo would have absorbed it even if it still did.
        assert!(b.resolve("raw-mono", "en-mono", 0, None).is_some());
        assert!(b.resolve("raw-mono", "en-mono", 2, None).is_some());

        assert_eq!(
            b.route_builds(),
            1,
            "four resolves of one pair, from a scan and its former repeat, \
             must build the route once"
        );
    }
}
