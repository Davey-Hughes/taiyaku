//! The binding's logic, as ordinary Rust.
//!
//! Kept apart from the `wasm_bindgen` shell in `lib.rs` so that `cargo test`
//! exercises the code that ships rather than a native stand-in for it.

use std::cell::RefCell;

use taiyaku_core::{ChapterAt, Half, MemberPage, Route, Sidecar, Slot, Via, route};

/// A parsed sidecar, held so a reader can ask it many questions without
/// re-parsing the document.
///
/// **It memoises the last `(from, to)` pair's `Route`.** Building a `Route`
/// rebuilds both slot tables and, where the answer composes through the hub,
/// recomposes the run lists — `taiyaku_core::Route` exists precisely so a
/// caller can pay that once. `route_cache` holds exactly one pair rather than
/// a map keyed on every pair ever asked about, because `web/src/nearest.ts`'s
/// `nearestPaired`, scanning either side of a gap, fixes `from`/`to` for the
/// whole scan and varies only the slot. One swap per keypress does not notice
/// either way; up to `slotCount` resolves during one gap scan does — see
/// `docs/status.md`'s O14 entry for the measured counts.
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
/// it. A `RefCell` would be the wrong tool the moment that stopped
/// being true.
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
    ///
    /// # Errors
    ///
    /// Returns an error if `json` does not parse as a taiyaku sidecar
    /// document — see `Sidecar::from_json` in `taiyaku-core`. The message
    /// names what failed to read (`"not a readable taiyaku.json"`) and the
    /// line and column `serde_json` located the fault at, so a caller
    /// handed a malformed `taiyaku.json` learns why rather than only that.
    ///
    /// Also if the document declares a format version newer than this build
    /// reads (`Sidecar::readable_version`). The reader is served its document
    /// by `taiyaku serve`, which projects it out of rows stamped with this
    /// build's `VERSION`, so that refusal is not reachable through the app as
    /// shipped — it is reachable for anything handing this binding a file, and
    /// resolving pages through a document half of whose geometry was silently
    /// discarded is the failure it prevents.
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
        if let Some(cached) = self.route_cache.borrow().as_ref()
            && cached.from == from
            && cached.to == to
        {
            return Some(cached.route.clone());
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

    /// Every edition this document defines, in key order.
    pub fn editions(&self) -> Vec<String> {
        self.sidecar.editions.keys().cloned().collect()
    }

    /// How many slots an edition presents, or `None` if it has no such edition.
    ///
    /// Slots rather than pages, because a joined spread is two of them and the
    /// slot is the addressed unit everywhere else in the format.
    pub fn slot_count(&self, edition: &str) -> Option<u32> {
        let reading = self.sidecar.work.reading;
        let count = self.sidecar.editions.get(edition)?.slots(reading).len();
        #[expect(
            clippy::cast_possible_truncation,
            reason = "count is at most twice the edition's page count (crate::expand's \
                      flat_map in taiyaku-core/src/slot.rs emits at most two Slots per page), \
                      and a page count is bounded by what a book can physically contain, far \
                      below u32::MAX. Tried taiyaku-core::solve's page_index pattern — a named \
                      helper that panics on the impossible case — and rejected it here: this \
                      crate compiles to wasm32-unknown-unknown and is loaded by a JS caller, \
                      where a panic is a trap that poisons the whole module rather than an \
                      ordinary Rust unwind. Unlike page_index's PageHash slices, `count` \
                      traces back through a JS-supplied taiyaku.json's declared per-member \
                      `pages` field (Sidecar::total_pages, taiyaku-core/src/sidecar.rs) — data \
                      this binding cannot treat as a trusted in-process invariant the way \
                      page_index's caller can. A JsError would fix that, but slot_count's \
                      signature is Option<u32> end to end, here and in WasmSidecar::slot_count \
                      (lib.rs), and widening it to Result<_, JsError> for an unreachable case \
                      is an API change outside a behaviour-preserving cleanup."
        )]
        let count = count as u32;
        Some(count)
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

    /// The page of one member that a flat slot index shows, or `None` for an
    /// unknown edition or an index past the end: the anchor a note or a
    /// position is written with (O240), from the core so the web never
    /// derives one itself.
    pub fn member_page(&self, edition: &str, index: u32) -> Option<MemberPage> {
        let reading = self.sidecar.work.reading;
        self.sidecar
            .editions
            .get(edition)?
            .member_page(index, reading)
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
            .map(|found| {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "found is a position within edition.slots(reading), at most \
                              twice the edition's page count (crate::expand's flat_map in \
                              taiyaku-core/src/slot.rs emits at most two Slots per page), and \
                              a page count is bounded by what a book can physically contain, \
                              far below u32::MAX. Same reasoning, and the same rejection of \
                              slot_count's page_index-style helper above, applies here: this \
                              crate is wasm32-unknown-unknown and JS-loaded, so a panic traps \
                              the module rather than unwinding; `found`'s bound traces through \
                              a JS-supplied taiyaku.json's declared `pages` fields, not a \
                              trusted in-process computation; and index_of's signature is \
                              Option<u32> throughout, so a JsError would need a scope-widening \
                              API change this cleanup does not make."
                )]
                let found = found as u32;
                found
            })
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

    /// The edition's credit pages, as page indices.
    ///
    /// Pages, not slots — the field is the page-sequence kind, like `spreads`.
    /// `web/src/modules/credits.ts` converts once, where it draws.
    ///
    /// An unknown edition answers empty rather than erroring: the reader asks
    /// about whatever edition it is showing, and "no list" and "no edition" are
    /// the same non-event to a caller deciding whether to skip a page.
    pub fn credits(&self, edition: &str) -> Vec<u32> {
        self.sidecar
            .editions
            .get(edition)
            .map(|e| e.credits.clone())
            .unwrap_or_default()
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

/// One edition carrying a `credits` list, for `credits` itself. No alignment
/// is needed — this exercises a lookup on a single edition, not a route —
/// so `hub` names the edition itself and `alignments` is empty, the same
/// minimal shape `taiyaku-core`'s `credits_round_trip_present_empty_and_absent`
/// uses for the same reason.
#[cfg(test)]
pub(crate) const SIDECAR_WITH_CREDITS: &str = r#"{
  "taiyaku": 1,
  "work": { "title": "T", "reading": "rtl" },
  "editions": {
    "fan": {
      "label": "Fan",
      "members": [{ "path": "fan.cbz", "sha256": "00", "pages": 8 }],
      "credits": [3, 4]
    }
  },
  "hub": "fan",
  "alignments": []
}"#;

/// One archive (`0a`) standing twice around `0b`, whose second page is a
/// spread: rtl slots are `0a` 0-2, `0b` 0, 1R, 1L, then `0a` again 0-2.
#[cfg(test)]
pub(crate) const TWICE_FIXTURE: &str = r#"{
  "taiyaku": 1,
  "work": { "title": "T", "reading": "rtl" },
  "editions": {
    "twice": { "label": "T", "members": [
      { "path": "a1.cbz", "sha256": "0a", "pages": 3 },
      { "path": "b.cbz", "sha256": "0b", "pages": 2 },
      { "path": "a2.cbz", "sha256": "0a", "pages": 3 }
    ], "spreads": [4] }
  },
  "hub": "twice",
  "alignments": []
}"#;

/// One archive whose second page is a spread, read left to right: slots are
/// 0, 1L, 1R, 2, where `rtl` would put the right half first.
#[cfg(test)]
pub(crate) const LTR_FIXTURE: &str = r#"{
  "taiyaku": 1,
  "work": { "title": "T", "reading": "ltr" },
  "editions": {
    "ltr": { "label": "L", "members": [
      { "path": "c.cbz", "sha256": "0c", "pages": 3 }
    ], "spreads": [1] }
  },
  "hub": "ltr",
  "alignments": []
}"#;

#[cfg(test)]
mod tests;
