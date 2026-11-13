# Go-public lane 4: anchors on the wire (O240, O244) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A note or a position lands on the page it was written on, whatever renumbering came after the tab read it.
- O240: the web sends each note's and each position's page (`at`) beside its slot, and the server places a write by it. A 412 retry re-sends each note with the page it was read or made on, and the server keeps an untouched echo whose `at` has gone stale.
- O244's remainder: a removal counts the positions and notes on works it keeps whose page it takes, and the two Remove dialogs show that count. A note whose page is gone is drawn as such.

**Architecture:**
- Task 1: `taiyaku-core`'s `MemberPage` gets its wire form, and `taiyaku-wasm` hands it to the web (`memberPage`).
- Task 2: the library's `Note` and `Position` gain `at`, filled with the stored anchor by two new reads that serve alone calls.
- Tasks 3–4: the notes `GET` sends each note's `at`; the notes `PUT` places a note by it.
- Tasks 4b–4c: a stamp remembers each member's digest before and after (migration 22), and a save naming a stamped-away digest is forwarded to the new one before placement.
- Task 5: the same for a position, forwarded the same way.
- Tasks 6–7: the web computes `at` through the binding, sends it, echoes what it read, and draws `stale`.
- Tasks 8–9: the removal's count, in the library and the answer, then in the two dialogs.
- Task 10: the closes, after the rebase onto PR A. Then the Finish.

One migration, 22 (`crates/taiyaku-library/src/schema/022_stamp_aliases.sql`, Task 4b): the next free number on the branch, and PR A adds none. The anchor columns themselves exist since migration 21. Task 4b adds SQL beyond a plain `SELECT`, so it runs `rust:1-bookworm` (settled call 13).

**Tech Stack:** Rust (`taiyaku-core`, `taiyaku-wasm`, `taiyaku-library`, `taiyaku-serve`), TypeScript/React (`web/`), vitest projects `node`, `jsdom` and `browser`, `wasm-pack`.

**Spec:** `docs/superpowers/specs/2026-11-12-go-public-design.md`, cited as "spec §n". This plan implements §6 (lane 4) and follows §7 (overlaps) and §9 "Lane 4".
- There was no lane survey. The code was read for this plan at **main 4b9d1d01**, after lanes 1–3 merged (#240, #241, #242) and the toolchain pin (#243).
- The pre-flight is `.superpowers/sdd/deploy-evidence/plans/lane-4-preflight.md`. This revision applies its findings and the controller's rulings on them.
- Locations are by symbol; line numbers are hints at 4b9d1d01.
- The pre-deploy design (`docs/superpowers/specs/2026-11-10-pre-deploy-design.md`) is cited as "pre-deploy".

**Rulings** (binding; do not re-open):
- **The owner:**
  - 2026-11-10: the web sends anchors (pre-deploy decision 4), in a follow-up that closes O240.
  - 2026-11-12: the full fix, now, over a stopgap.
  - 2026-11-03 (overlapping-libraries decision 11): no reload on a 412; the `ETag` names the account's numbering.
  - 2026-11-12 (spec §6 decision 9; pre-deploy decision 8 deferred it here): the removal rehearsal counts the rows it would leave unresolved.
  - 2026-11-10 (pre-deploy decisions 2 and 3): a lost file's note keeps its stored slot and is `stale`; the row is never rewritten.
  - **2026-11-13, "Server keeps echoes"** (pre-flight I1, `deploy-evidence/rulings.md`): in the notes `PUT`'s case (d), a resent note equal to the held one in everything but `at` and `stale`, compared in the account's numbering, keeps the held note instead of being refused. This is `notes::validate_against`'s untouched-echo rule. A note another device really moved is still refused, and that refusal's sentence tells the reader to reload.
  - **2026-11-13, "Stamp remembers fingerprints"** (Task 4 review, Important 2; `deploy-evidence/rulings.md`): each stamp records each member's digest before and after it, and a note or position save whose `at` names an old digest is translated to the new one before placement. So every edit from a pre-stamp tab or device saves instead of being refused: a body or colour edit, a new note, a region move, and an untouched note after a stamp plus a renumbering.
- **The controller, on the pre-flight:**
  - B1: `newNote`'s and `anchorNote`'s two `App.tsx` call sites are Task 6's.
  - I2: a note on a half the layout no longer holds is `stale`, never placed (pre-deploy decision 6; `state::resolve_note`).
  - I3: Task 7 makes no fallback claim for Review Focus 1.
  - The pre-flight's flagged items 1, 3 and 5 stand as planned; item 5 says it neither closes nor narrows O271.
  - PR A's overlaps are as Global Constraints lists them.
- **The spec's own calls** (spec §6 decisions 3–8): `at` is optional on each note and on a position, in the account's numbering; the `GET`s answer it; a valid anchor wins over the slot; an `at` equal to the stored anchor keeps it, compared before any translation; the web computes anchors through `taiyaku-core`; exported documents stay anchor-free and `taiyaku_notes` stays 1; `stale` is drawn, an edit keeps the anchor and a move re-anchors it.
- **Not reversed:** overlapping-libraries decisions 7, 8 and 11; pre-deploy decisions 2, 3, 6 and 7; O3 (a position's slot is range-checked, for a body without `at`); O60 (an edition outside the grant is refused in the words of one the work does not have); O264 (only a newer document is refused); lane 3's O184 (`store` takes `Mint` and answers `NoRow`); the "Never refuse" ruling of 2026-11-10, which is the fold's and does not reach a `PUT`.

**What the plan settles** (the plan writer's calls; the controller tells the owner):
1. **One PR**, not two.
   - The count is two small tasks; the wire is the risk.
   - Two PRs would mean two whole-branch reviews, two claim checks and two edits of O244's row (a narrowing, then the close). WUD deploys each merge, so the first PR would run alone for a while either way.
   - Nothing in the count depends on the web's anchors, nor the reverse. The controller may still cut Tasks 8–9 into a second PR if the first runs long; the first PR then closes O240 and narrows nothing (O244's row already says what remains).
2. **The wire's shape.** `MemberPage` serialises as `{"sha256": "…", "nth": 0, "page": 1}`, with `deny_unknown_fields`. `"half": "left"` or `"right"` appears only for a half (core's `Half` serde, the store's `at_half` spelling). `Note.at` follows `stale`; `Position.at` follows `updated`; both are left out when absent.
3. **`WasmAt.half` uses the wire's spelling** (`"left"`/`"right"`), not `WasmPosition`'s `"L"`/`"R"`, so the web copies the object onto the wire unchanged and never converts.
4. **Which reads fill `at`.** `state::notes_anchored_in` and `state::progress_anchored_in` answer each row's stored anchor, as stored, whether or not it resolves. `notes`, `notes_in`, `progress` and `progress_in` leave `at` absent. So `library export` stays anchor-free by construction, and the many test reads of notes and positions across the library, CLI and serve suites do not change. Only serve's `notes::load` and `progress::load` call the anchored reads.
5. **`library import` ignores `at`.** It writes through `state::put_notes`/`put_progress_at`, which derive the anchor from the slot. A document carrying `at` is accepted: `Note` now has the field.
6. **`Notes::clear_stale` becomes `Notes::clear_wire_fields`**, clearing `stale` and `at`: the two fields a read adds to a note and a store never takes from one.
7. **The notes `GET`** (`View::anchor_sent`):
   - For `Access::Every`, and for an edition the account is sent whole, `at` is the stored anchor, under the hidden-digest rule below; a row with no anchor is sent no `at`, on any edition, as settled call 10 sends a position (the controller, 2026-11-13).
   - For a narrowed edition, a note that is not `stale` is sent the page its sent slot shows in the document sent, so `nth` counts the members the account is sent.
   - A `stale` note is sent its stored anchor as stored (spec decision 5), **unless, for any account but `Access::Every`, its `sha256` is that of a member of the work the account is not sent, in any edition, and of no member it is sent, on an edition sent whole as on a narrowed one** (the controller, 2026-11-13). Then it is sent no `at`. That digest is the key `View::etag_naming_numbering` hashes so a tag cannot be enumerated. The note's echo then has no `at`, is read by its slot (case e), and `put_notes_in`'s `same_anchor` keeps the stored anchor. See spec gap 8.
8. **The notes `PUT`**, per incoming note, after the `If-Match` check and before `as_stored` (`notes::placed`). Each incoming `at` is first forwarded past any stamp (`works::past_stamps`, settled call 16; Task 4c), except one equal to the stored anchor of the held note of the same id and edition, which is not forwarded (`routes::put_notes`' `held_there`; the controller, 2026-11-13). That anchor is numbered after the stamp, where a copy the stamp left out takes the stamped copy's old pair, so forwarding it would move a lost copy's note onto the stamped copy (O244). The cases below read the forwarded `at`:
   - (a) With `at` equal to the stored anchor of the held note of the same id and edition: its slot becomes the slot that held note is sent at now (`View::slot_from_full`, `Visible`). Where it is not sent, the slot is left for `as_stored` to judge, as today.
   - (b) Otherwise, where `View::slot_of_anchor` answers `SlotOf::Exact` in the account's document: its slot becomes that slot. **`SlotOf::PageOnly` does not place a note.** It is a half the layout no longer holds, and a note is never moved to a page it was not written on (pre-deploy decision 6; `state::resolve_note`). Such an `at` goes on to (d).
   - (c) An `at` on an edition the account's document lacks: left alone, so `validate_against` refuses the edition in its own words (O60).
   - (d) Any other `at`:
     - **(d1) An untouched echo is kept** (the owner's 2026-11-13 ruling). Where a held note of the same id is sent to the account, and the incoming note equals that held note as the account is sent it in every field but `at` and `stale` (`id`, `edition`, the slot `View::slot_from_full` sends it at, `region`, `color`, `body`, `created`, `updated`), the incoming note stands as the held one. `put_notes_in`'s `same_anchor` then keeps its stored anchor. A stamp is the common case: `stamp::rekey_anchors` moves every stored anchor on a stamped member to the new digest, and the tab's untouched notes still carry the old one.
     - **(d2) Otherwise refused**: `Invalid::Missing`, a 404, in the sentence below, which tells the reader to reload. This covers a page the account is not sent, a page past a sent member's end, a note made on a half now gone, and a note another device moved.
   - (e) No `at`: the slot is read as today.
   - Then `clear_wire_fields` on the incoming document and on `current` (the held one), and `as_stored`, `validate_against` and `store` run unchanged. `store` derives each new or moved note's anchor from its slot (`state::put_notes_in`), which for (a) and (b) is the page `at` named.
9. **A position's `PUT`** reads its body as `progress::Body` (serve's own type), so `Progress`, the library's write shape, built as a struct literal across the library's and serve's tests, is unchanged. `progress::place` decides; the handler acts. Its `at` is first forwarded past any stamp, by the same `works::past_stamps` the notes write calls (settled call 16), except one equal to the stored anchor of this reader's row on that edition, which is not forwarded (`routes::put_progress`; the controller, 2026-11-13), for the reason settled call 8 gives for a note:
   - (a) An `at` the account's document resolves, `Exact` or `PageOnly`, sets the slot. `PageOnly` is the page's first slot: the position half of pre-deploy decision 6, as `state::resolve_position` reads it. The slot sent is then neither read nor range-checked.
   - (b) Otherwise, an `at` equal to the stored anchor of this reader's row on that edition leaves the row as it is, and answers 204.
   - (c) Any other `at`: 404 in the sentence below.
   - (d) No `at`: today's path, O3's range check included.
   - Resolution comes before equality here, unlike a note's, because a write is what makes an edition the one a reader resumes in (`state::progress`'s `ORDER BY updated DESC`). And the library's writer derives the anchor from the slot, so a "kept" write would tie the row to the page its stored slot shows now. See spec gap 4.
10. **The progress `GET`**, through `View::anchor_sent`:
    - Where the stored anchor resolves in `View::full`, `at` is the page of the slot sent on a narrowed edition (for a hidden page's fallback, the fallback's page). On an edition sent whole, and for `Access::Every`, a resolving stored anchor is sent as stored; it differs from the slot's page only in `half`, where the stored half is gone.
    - Otherwise, `at` is the stored anchor as stored, under settled call 7's hidden-digest rule.
    - A row with no anchor sends no `at`.
11. **Lane 3's hand-off.** `at` is resolved in the handlers before `store`. `store`, `Mint` and `NoRow` are unchanged.
12. **The web.**
    - `at` comes from `Resolver.memberPage` over the tab's own sidecar, the layout the reader draws.
    - A new note takes the page shown. A region drawn or moved (`anchorNote`) re-anchors the note on the page its slot shows and clears `stale`. A body or colour edit keeps `at` and `stale`.
    - An untouched note keeps the `at` it was read with: `editsToWrite`, `mergeOver` and the history's undo already copy whole notes.
    - A progress write's `at` is taken when the slot is recorded, from the session whose slot it is: in the effect that records the turn, never in the debounced callback, which fires up to 400 ms later and can follow a resolver swap.
    - `getProgress` keeps answering `{edition, slot}`.
    - `notesJson`, the reader's own notes export, leaves out `at` and `stale` (spec decision 7; spec gap 5).
13. **One `rust:1-bookworm` run, Task 4b's.** Task 4b's migration and alias writes are the lane's only SQL beyond a plain `SELECT`. Any other task that finds it needs more stops and reports.
14. **The count** (Task 8).
    - It is taken inside `apply_removal` and `apply_library_removal`, before the deletes and again after the derive inside the transaction. So the dry run and the real removal answer the same measured figure, and the real removal's `removed` object carries a figure instead of a 0 it never measured.
    - **It neither closes nor narrows O271.** O271 is about the `derive` object's first-pass figures. The two new fields sit in `removed` and are measured in the first pass in both answers, so they add no new O271-shaped 0. Task 10 leaves O271's row alone.
    - The cost: two store-wide `resolving` reads inside the real removal's write transaction, beside the derive it already runs there.
    - "Resolves" is the read's own rule: a position's anchor resolves where `slot_of` answers anything (`resolve_position`), a note's only on `SlotOf::Exact` (`resolve_note`).
    - Rows are keyed by identity: a position by `(user_id, work_id, edition)`, a note by `(user_id, work_id, id)` (the controller, 2026-11-13). A row absent after (its work removed) is not counted, as `positions`/`notes` already count it. A row the derive's fold carries to another work is not counted under its old key. A carried note takes a new key, which nothing counts; a carried position takes the kept work's key for its edition, so where that work already held a position there, the newer of the two stands under that key and is counted where the one held there before resolved and it does not (accepted as rare).
15. **One 412 per tab at deploy.** The notes tag hashes the stored document as loaded (`notes::tagged_for`, `View::etag_naming_numbering`), and each note in it now carries `at`. So a tab open across the deploy, on a work that holds a note, meets one 412 on its next note write. It re-reads and shows "kept on top" once. A work with no notes keeps its tag. The PR body says so.
16. **A stamp's fingerprints** (the owner's 2026-11-13 ruling; Tasks 4b–4c).
    - **The table.** `stamp_alias`, one row per `(work_id, edition, was_sha256, was_nth)`, naming `(now_sha256, now_nth)`. It is keyed by work and edition, not global:
      - a digest can stand in two works, and a stamp rewrites one work's archive only;
      - `rekey_anchors`' moves are per edition, since a copy the stamp did not rewrite can change its occurrence (`nth`) when a copy before it changes digest.
      - `work_id` references `work(id) ON DELETE CASCADE`, so a work's removal or purge takes its aliases.
    - **Written where the anchors move.** The aliases are written by `stamp::remember_moves` in `stamp::record_rows`' transaction, from the same `moves` map `rekey_anchors` applies to the stored rows. A stamp that rolls back leaves no alias.
    - **A chain collapses when it is written.** A second stamp (`mid → new`) redirects every alias whose target is `mid` to `new`, so `old → new` and `mid → new` both stand, and one lookup suffices. An alias that comes back to its own source (`A → B`, then `B → A`) is deleted.
    - **What the table does not follow:**
      - an edition key a rule edit renames, whose aliases stay under the old key and never match;
      - a fold, whose superseded work's aliases go with its row.
      - Both lose only the translation for a tab older than the stamp, which then meets case (d) as before this ruling.
    - **Not exported, not imported.** `library export` writes anchor-free documents (decision 7), and an alias means nothing without the anchors it forwards to. A backup (`VACUUM INTO`) and a restore by moving the file carry the table with the rest of the store.
    - **Where the translation runs:** one function, `works::past_stamps`. The notes write calls it on each incoming `at` before `notes::placed` (so before cases (a)–(d)), and the position write calls it before `progress::place` (Task 5). Neither calls it on an `at` equal to the stored anchor of its held note or of the reader's row (settled calls 8 and 9).
      - It forwards an `at` only where `View::slot_of_anchor` answers nothing for it in the document sent, and a `stamp_alias` row names its `(sha256, nth)` on that work and edition. `page` and `half` are kept.
      - The guard is O270's window: a derive before the next scan puts the old digest back on a stamped member. A reader sent that document has an `at` naming the old digest, which resolves, and forwarding it would make it miss.
      - An alias's `nth` is the full edition's, read against the account's `at`, and the two numberings agree: a stamp rewrites a member under one directory, and derive gives every copy of its digest a file there (`works::past_stamps`).
      - Where one archive stands twice in an edition and a stamp rewrites the earlier copy but leaves the later one out (`stamp::plan` skips a member it cannot read or one held under more than one directory), the later copy takes the rewritten copy's old digest and occurrence. A pre-stamp `at` naming the rewritten copy then resolves on the later copy, so it is not forwarded and is placed on that copy's same page (accepted, as O244 accepts duplicate copies renumbering; the two copies' pages are the same bytes).
    - **Never sent.** Reads still send the current anchor (`notes_anchored_in`, `progress_anchored_in`); no alias reaches a client, so no old digest of a hidden member does.

**Spec gaps** (not resolved silently; the controller tells the owner):
1. **Locations moved since ec4eb11c, or never held.** At 4b9d1d01:
   - `get_notes` is at ~1104 (spec 1042), `put_notes` ~1180 (1118), `get_progress` ~1539 (1445), `put_progress` ~1602 (1508).
   - `scan::removal_cost` ~2010 (1969). `removal_json`'s destructuring ~1348 (1335). O244's row ~1867 (`status.md:1853`).
   - "`App.tsx:1648`" (the retry re-sends every note) is now `scheduleSave` ~1653 and `writeNotes` ~1670.
   - "`scan.rs:1128-1130`" (an unlink derives nothing) is now `Unlinked`'s doc ~1132-1134. `UnlinkRoot.tsx:12-14` is ~13-14.
   - **`library_removal_cost` is not in `libraries_api.rs:554`.** It is `scan::library_removal_cost` (~2194), which `libraries_api::delete_library` calls (~567).
   - Lane 3 changed the same writers. `notes::store` and `progress::store` take a final `Mint` and answer `Result<(), NoRow>`, `notes::store` is `pub(crate)`, and the `Failed` arms of `NoNotes` and `NoProgress` answer path-free. Lane 3's PR body hands lane 4 the choice between resolving `at` before `store` and adding a `NoRow` variant: settled call 11.
2. **Decision 6 names `slot_of` among the exports, and no web path in §6 calls it.** The server answers every slot already resolved, and a new or moved note needs only `member_page`. This plan exports `memberPage` alone. If the owner wants `slotOf` too, it is one binding method, one wasm method and one binding test in Task 1.
3. **Decision 5 names three cases. The spec covers a fourth elsewhere, and leaves a fifth.**
   - Decision 5 names an `at` that resolves, one equal to the stored anchor, and one naming no member the account is sent.
   - **A half the layout no longer holds is not a gap.** Pre-deploy decision 6 (`2026-11-10-pre-deploy-design.md:395-398`) and `state::resolve_note`'s doc (`state.rs` ~469-470) say a note there is `stale`, and a position goes to its page's first slot. A write follows the read: settled calls 8b and 9a.
   - **An `at` naming a sent member's page past that member's end is the plan's extension.** It is refused, in the same words as decision 5's refusal. An untouched note never reaches the refusal: case (a) and the owner's (d1) come first.
4. **Decision 5's order applied to a position.** "Compared with the stored anchor for equality before any translation" is right for a note: it keeps a narrowed edition's stale note round-tripping.
   - Applied to a position, an `at` equal to the stored one would skip the write. A reader who turns back to the page their row already names in edition X, after reading in Y, would then resume in Y.
   - So a position's `at` is resolved first, and equality decides only an `at` that does not resolve (settled call 9).
   - The case this gives up: one archive standing twice in a narrowed edition, with the account's `nth` and the stored `nth` naming different copies. The plan accepts it as rare; O244's row already accepts duplicate copies renumbering.
   - "No write" also skips `put_progress_in_order`'s arrival record (O272), which is harmless for a no-op.
5. **§6 "Where" omits the reader's own notes export.** `NotePanel`'s download writes `notesJson(doc)`, the server's document as sent. It already writes `"stale": true` and would write `at`. Decision 7 ("exported documents stay anchor-free") covers it, so Task 6 strips both. An older binary's `library import` would otherwise refuse the file: `Note` is `deny_unknown_fields`.
6. **The count is "the dry run's"** (§6), and this plan takes it in the step the dry run and the real removal share (settled call 14). It neither closes nor narrows O271. If the owner wants the real removal's answer left without it, the two fields become `Option` and the real removals pass `None`.
7. **Decision 5's refusal against decision 2's whole-document retry: ruled 2026-11-13, "Server keeps echoes".** Without the ruling, three things went wrong:
   - **It stuck.** A refused save leaves `notesDoc` and `notesEtag` as they were (`App.tsx`'s `writeNotes`, the `refused` arm). Every later save resends every note the tab holds (`scheduleSave` → `editsToWrite(allNotes, …)`; the retry's `mergeOver(fresh, mine)`). So one refused note would refuse every save in that tab until a reload. The refusal sentence did not say to reload. That cuts against `validate_against`'s exemption (`notes.rs` ~68-77) and against decision 11's choice of the `ETag` over a reload.
   - **A stamp reached it**, not only a stale note another device moved. `stamp::rekey_anchors` moves every row on a stamped member to the new digest. The tab's `at`s name the old digest, which no longer resolves, so each untouched note fell to the refusal. Because the tag hashes `at`, the stamp also forces the 412 that starts the retry.
   - **Undoing the delete of a stale note** reaches it too: there is no held note, and the `at` does not resolve. That one does not stick, since the undo is one save.
   - **The first ruling's effect ("Server keeps echoes").** Case (d1) keeps an untouched echo, so a stamp and a renumbered tab's untouched stale notes save. The refusal sentence now tells the reader to reload, as `NO_VALIDATOR` does (`notesWrite.ts` ~65-67).
   - **What it left, found by Task 4's review (Important 2).** After a stamp, a pre-stamp tab's own edit carries the old digest: a body or colour edit (an edit keeps the `at` it was read with), a new note, or a region move (both computed from the tab's pre-stamp sidecar). Cases (a), (b) and (d1) all fail, the save is refused, and the refusal sticks. So does an untouched note after a stamp plus a renumbering of its slot: (d1) compares the slot in the current numbering.
   - **The second ruling, "Stamp remembers fingerprints"** (settled call 16; Tasks 4b–4c): each stamp records old to new digests, and every save forwards an old digest before placement. Every case above then saves, through (a) or (b).
   - **What is still refused**, and sticks until a reload, in words that say to reload:
     - a note another device really moved;
     - a page the account is not sent, or past a member's end;
     - a note made on a half now gone;
     - an `at` naming a digest no stamp recorded, which includes a pre-stamp tab after a fold or an edition rename (settled call 16);
     - the undone delete of a stale note, which has no held note and an `at` that resolves nowhere, and the undone region marked on a stale note, whose held note now names the page shown and whose region differs (neither sticks, since the undo is one save).
   - The PR body says all of this.
8. **Decision 5 sends a stale note's stored anchor "as stored"; this plan withholds it where it names a hidden member** (settled call 7; pre-flight M7).
   - The case: a stale note or position of any account but `Access::Every` whose stored anchor names a member of the work the account is not sent, in any edition, and no member it is sent, whether its own edition is narrowed or sent whole: a half or a past-the-end page of a member hidden after a grant narrowed, or an archive since filed only under an edition the account is not sent (the controller, 2026-11-13).
   - Sent as stored, the body would carry that member's `sha256`. That digest is the key `View::etag_naming_numbering` hashes so the tag cannot be enumerated (`view.rs` ~197-201; `manga_alones_notes_tag_cannot_be_found_from_what_it_is_sent`).
   - It is rare. Withholding it costs nothing: the echo is read by its slot and keeps the stored anchor.

**New interfaces this plan names** (the plan is where they are first written):
- **Core** (`crates/taiyaku-core/src/sidecar.rs`): `MemberPage` gains `Serialize, Deserialize` and `#[serde(deny_unknown_fields)]`. Its `half` gains `#[serde(default, skip_serializing_if = "Option::is_none")]`.
- **Binding:**
  - `crates/taiyaku-wasm/src/binding.rs`: `pub fn member_page(&self, edition: &str, index: u32) -> Option<MemberPage>`, which is `Edition::member_page` in the sidecar's reading direction, `None` for an unknown edition.
  - `crates/taiyaku-wasm/src/lib.rs`:
    ```rust
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
    ```
    and `WasmSidecar::member_page(&self, edition: &str, index: u32) -> Option<WasmAt>` with `js_name = memberPage`.
- **Library** (`crates/taiyaku-library`):
  - `Note.at: Option<taiyaku_core::MemberPage>` and `Position.at: Option<taiyaku_core::MemberPage>`, each `#[serde(default, skip_serializing_if = "Option::is_none")]`.
  - `Notes::clear_wire_fields(&mut self)`, replacing `clear_stale`.
  - `state::notes_anchored_in(conn: &Connection, work_id: i64, user_id: i64, numbering: Numbering<'_>) -> Result<Notes, String>`, and `state::progress_anchored_in(…same…) -> Result<Vec<Position>, String>`.
  - Task 4b, migration 22 (`crates/taiyaku-library/src/schema/022_stamp_aliases.sql`), its comment saying why in the style of `021_anchors.sql`:
    ```sql
    CREATE TABLE stamp_alias (
      work_id    INTEGER NOT NULL REFERENCES work(id) ON DELETE CASCADE,
      edition    TEXT    NOT NULL,
      was_sha256 TEXT    NOT NULL,
      was_nth    INTEGER NOT NULL,
      now_sha256 TEXT    NOT NULL,
      now_nth    INTEGER NOT NULL,
      PRIMARY KEY (work_id, edition, was_sha256, was_nth)
    ) WITHOUT ROWID;
    ```
  - Task 4b: `pub fn stamp::remember_moves(conn: &Connection, work_id: i64, edition: &str, moves: &BTreeMap<(String, u32), (String, u32)>) -> Result<(), String>`.
    - It composes `moves` with the edition's stored aliases: each stored target `moves` names is redirected, each move is added, and an alias whose target is its source is dropped.
    - It writes the result in place of the edition's aliases.
    - `pub` so serve's tests can shape a stamp the way `record_rows` makes one.
  - Task 4b: `pub fn stamp::alias_of(conn: &Connection, work_id: i64, edition: &str, sha256: &str, nth: u32) -> Result<Option<(String, u32)>, String>`.
  - Task 8: `pub(crate) struct state::Resolving`; `pub(crate) fn state::resolving(conn: &Connection) -> Result<Resolving, String>`; `pub(crate) fn Resolving::lost_since(&self, before: &Resolving) -> (usize, usize)`, answering (positions, notes); `scan::Removal` gains `pub positions_page_gone: usize` and `pub notes_page_gone: usize`.
- **Serve** (`crates/taiyaku-serve`):
  - `View::anchor_sent(&self, edition: &str, slot: u32, stored: Option<&MemberPage>, resolves: bool) -> Option<MemberPage>`: `None` where `stored` is, on any edition; on a narrowed edition where `resolves`, the page `slot` shows in the document sent. Otherwise `stored`, cloned, unless the private `View::hides` its `sha256`: for every view but `Access::Every`, a digest some member of any edition of the full document carries and no member of the document sent carries (a digest no member carries is sent as stored).
  - `View::slot_of_anchor(&self, edition: &str, at: &MemberPage) -> Option<SlotOf>`: `Edition::slot_of` in the document sent.
  - `notes::placed(incoming: &mut Notes, current: &Notes, view: &View, work: &str) -> Result<(), Invalid>` (settled call 8).
  - Task 4c: `pub(crate) fn works::past_stamps(conn: &rusqlite::Connection, work_id: Option<i64>, view: &View, edition: &str, at: &MemberPage) -> Result<MemberPage, String>`.
    - It answers `at` itself where `view.slot_of_anchor(edition, at)` answers anything, where the work has no row, or where `stamp::alias_of` names nothing.
    - Otherwise it answers the alias's digest and occurrence, with `at`'s `page` and `half` (settled call 16).
  - `progress::Body { edition: String, slot: u32, #[serde(default)] at: Option<MemberPage> }`: `Deserialize` only, without `deny_unknown_fields`, as `Progress` has none.
  - `pub(crate) enum progress::Placed { Slot(u32), Kept, Refused }` and `pub(crate) fn progress::place(view: &View, edition: &str, at: &MemberPage, stored: Option<&MemberPage>) -> Placed` (settled call 9a–c). `Slot` is in the account's numbering.
- **Web:**
  - `web/src/resolver.ts`:
    ```ts
    /** One page of one member, as the wire carries it (O240). */
    export interface At {
      sha256: string
      nth: number
      page: number
      half?: 'left' | 'right'
    }
    ```
    and `Resolver.memberPage(edition: string, index: number): At | undefined`.
  - `web/src/notes.ts`:
    - `Note` gains `at?: At` and `stale?: boolean`.
    - `newNote` gains a last parameter `anchor: At | undefined`.
    - `anchorNote` gains a last parameter `anchorOf: (edition: string, slot: number) => At | undefined`.
  - `web/src/api.ts`: `StoredProgress` gains `at?: At`.
  - `web/src/config.ts`: `Removal` gains `positions_page_gone: number` and `notes_page_gone: number`.
  - `web/src/libraries.ts`: `export function pagesGoneSentence(positions: number, notes: number): string`.

**Sentences** (literal; tests assert them written out):
- **Notes `PUT`, settled call 8 (d2)** (404): `` note `{id}` names a page that `{edition}` of work `{work}` does not have now. Reload the page to see the notes as they are. ``
- **Progress `PUT`, settled call 9c** (404): `` this position names a page that `{edition}` of work `{work}` does not have ``.
- **`NotePanel`, editing a `stale` note,** a paragraph under the anchor line: `The page this note was written on is no longer in {edition}, so it is shown where it was stored. Marking a region moves it onto the page shown.`
- **`NotePanel`'s review list,** a `stale` note's row: its meta text ends ` · no longer on its page`.
- **The two Remove dialogs,** a paragraph (`className="pages-gone"`) after the counts, drawn only when either figure is above 0: `On works that stay, this takes the page of {parts}.`
  - `{parts}` is `count(positions, 'in-progress position', 'in-progress positions')` and `count(notes, 'note', 'notes')`, each only when above 0, joined by ` and `.
  - For example: `On works that stay, this takes the page of 2 in-progress positions and 1 note.` and `On works that stay, this takes the page of 1 note.`

**Functions this lane touches**, so the rebase onto PR A (O280) can be checked:
- `taiyaku-core`: `sidecar.rs`'s `MemberPage` only.
- `taiyaku-wasm`: `binding.rs` (`Binding::member_page`), `lib.rs` (`WasmAt`, `WasmSidecar::member_page`); `scripts/smoke-wasm.mjs`.
- `taiyaku-library`:
  - `lib.rs`: `Note`, `Position`, `Notes::clear_wire_fields`.
  - `state.rs`: the anchored reads and `Resolving`.
  - `scan.rs`: `Removal`, `apply_removal`, `apply_library_removal`, and the docs of `removal_cost`, `library_removal_cost`, `remove_root` and `remove_library`.
  - `export.rs`: the `clear_stale` call.
  - `stamp.rs`: `record_rows`, `rekey_anchors` (their callers of `remember_moves`), `remember_moves`, `alias_of`. `schema.rs`: `SCHEMA_VERSION`, `MIGRATIONS`. New `schema/022_stamp_aliases.sql`.
- `taiyaku-serve`:
  - `works.rs`: `past_stamps`.
  - `routes.rs`: `get_notes`, `put_notes`, `get_progress`, `put_progress`.
  - `notes.rs`: `load`, `as_seen`, `placed`. `progress.rs`: `load`, `Body`, `Placed`, `place`.
  - `view.rs`: `anchor_sent`, `slot_of_anchor`.
  - `config_api.rs`: `removal_json`.
  - Tests: `routes/tests.rs` (the `note` helper, and new tests at the end of `mod narrowing`), `notes/tests.rs`, `progress/tests.rs`, `config_api/tests.rs`, `libraries_api/tests.rs`.
- Web: `resolver.ts`, `notes.ts`, `api.ts`, `App.tsx` (`startNote`, `endRegion`, the progress write), `NotePanel.tsx`, `config.ts`, `libraries.ts`, `RemoveRoot.tsx`, `RemoveLibrary.tsx`, and the fixtures that build a `Resolver` or a `Removal`.

## Global Constraints

- **The worktree and the rebase.**
  - Lane 4's worktree is cut from main **4b9d1d01** now. It rebases onto PR A once PR A merges, which PR A does first.
  - `W=/home/davey/projects/taiyaku/.claude/worktrees/go-public-4` (the controller confirms the path in each dispatch).
  - `E=/home/davey/projects/taiyaku/.superpowers/sdd/deploy-evidence/lane-4`, in the MAIN checkout, so reports survive the worktree.
  - Every command runs against `$W` by absolute path. Never `cd`: it is zoxide here. Where a tool must run from a directory, write `builtin cd <dir> && …` in that one call.
  - Tasks 1–9 run on 4b9d1d01. **Task 10 and the Finish run after the rebase onto PR A**: Task 10 measures every count after it, and the Finish gate runs on the rebased head. If PR A has not merged when Task 9 ends, wait.
- **PR A** (O280 and housekeeping, plan `deploy-evidence/plans/2026-11-13-o280-housekeeping.md`) runs on its own branch. Shared files:
  - `crates/taiyaku-serve/src/routes.rs`: PR A changes `get_sidecar` and adds `with_file_names` beside `filter_sidecar_editions`. Lane 4 changes the four notes and progress handlers.
  - `crates/taiyaku-serve/src/routes/tests.rs`, `mod narrowing`:
    - PR A inserts tests after `a_narrowed_document_has_its_own_etag`, and edits `both_libraries_are_sent_raw_whole` and `manga_alones_notes_tag_cannot_be_found_from_what_it_is_sent`.
    - Lane 4's Task 2 edits the `note` helper's literal. Tasks 3–5 add their tests **at the end of `mod narrowing`**.
  - `crates/taiyaku-serve/src/view.rs`: PR A edits one doc-comment clause of `View::etag_naming_numbering` (the controller's ruling). Lane 4 adds `anchor_sent` and `slot_of_anchor`.
  - `docs/status.md`: PR A closes O280, opens O281 and rewrites the freshness sentence (~392). Lane 4's Task 10 edits O240, O244 and the counts.
  - `scripts/comment-budget.json`: both may move a share. After the rebase, rerun `check-comments.mjs` and take each value from the guard's message.
  - `scripts/check-all.sh` and the new `scripts/coverage-build-guard.sh`: PR A's coverage guard then runs in lane 4's gate. Lane 4 does not edit either.
  - PR A alone touches `session.rs` (`administers`), `docs/sidecar.md`, `docs/ci.md`, `web/src/DirPicker.tsx` and `web/src/NewLibrary.tsx`. PR A does not touch `taiyaku-core`, `taiyaku-wasm` or `web/src/resolver.ts`.
- **The worktree's set-up.** Before Task 1 the controller:
  - makes `$W/web/node_modules` a real copy, never a symlink;
  - copies `crates/taiyaku-wasm/pkg` in.
  - Never `pnpm exec`. Call `./node_modules/.bin/<tool>` directly.
  - Never set `CARGO_TARGET_DIR`. `$W` builds into `$W/target`. A fresh worktree has no `target/llvm-cov-target` from before the 1.100.0 pin, so its coverage reads true.
- **The wasm package.** From Task 1 on, `WasmSidecar` has `memberPage`. Before any web task's `tsc -b`, rebuild it: `builtin cd $W && scripts/build-wasm.sh >| $E/T<n>-wasm.log 2>&1`. `tsc` checks `resolver.ts` against the generated `.d.ts` whenever `pkg/` exists.
- **Commits.** One commit per task, its subject naming the O-number(s). Never `git commit --amend`. Before reporting, check with `git -C $W log --oneline BASE..HEAD` that each task's commit is an ancestor of HEAD.
- **Comments say why**: the reason, the invariant, what breaks if it changes. Never history or measurements; a comment may point at an O-number.
  - `scripts/check-comments.mjs` and `scripts/comment-budget.json` hold this in the gate.
  - Run `node $W/scripts/check-comments.mjs --base <the task's start head>` before committing.
  - If the gate fails on a share, follow `docs/ci.md`, "The comment guard", and say so in the report.
- **Attribution.** None anywhere: code, comments, docs, commits, PR body.
- **The gate.** Run the repo's own `scripts/check-all.sh`, never a copy. It runs from the worktree root, so the call is `builtin cd $W && env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh >| $E/<name>.log 2>&1; echo "exit $?" >| $E/<name>.exit`.
  - Wait for load < 24 (`cat /proc/loadavg`) and record it.
  - Read the `.exit` file, not the harness's exit code, and quote the `all green` line.
  - Never pipe the gate through `tail`.
- **Rust, per task:**
  - `env -u TAIYAKU_CORPUS cargo nextest run --manifest-path $W/Cargo.toml -p <crate> --test-threads=8 >| $E/T<n>-nextest-<crate>.log 2>&1`, for each crate the task touches and each crate that depends on one it changed the shape of. Read the summary lines from the log.
  - Then `cargo clippy --manifest-path $W/Cargo.toml --workspace --all-targets --all-features -- -D warnings` and `cargo fmt --manifest-path $W/Cargo.toml --all -- --check`.
  - Whole crates, never a named test.
- **Web, per task:**
  - `builtin cd $W/web && ./node_modules/.bin/vitest run --project <node|jsdom> --reporter=default >| $E/T<n>-<project>.log 2>&1`.
  - Then `./node_modules/.bin/tsc -b` and `./node_modules/.bin/biome check ./src ./tests`, the same way.
  - Whole projects, never a named file.
  - A task touching a `*.browser.test.tsx` file, any CSS, or a fixture a browser test builds (`tests/support/fixtures.ts`, `tests/support/libraries.ts`) runs the whole browser project. CI never runs it.
  - A browser-project failure with import or connection errors and no failed assertion is the headless Chromium crashing. Read `coredumpctl list` and `/proc/loadavg`, then rerun the whole project.
- **Pre-existing failures.** A failure is pre-existing only if the same command fails at BASE, not at another commit of this branch. Report it; do not fix it.
- **Watched mutations.** Every new or tightened assertion gets one:
  - Put the named defect back by hand and watch the test fail.
  - Restore, and check with `git -C $W diff` that the restore is complete.
  - Record in the report the defect, the failing test and the assertion's message.
  - A test whose fixture renders right and wrong alike is a plan failure: say so rather than ship it.
  - A case this plan labels a **pin** names no mutation: it holds behaviour that already holds. Record it as a pin.
- **The fallout rule.** When `at` starts being sent (Tasks 3 and 5), an existing assertion may compare a whole note or position, read through `notes::load`, `progress::load` or a `GET` body, with one built without `at`.
  - Keep the assertion. Compare with `at` cleared through a test helper, or state the expected `at`.
  - Never delete the assertion or loosen it to a slot comparison.
  - List each in the report.
- **Sentences are literals** in tests, never built through the helper under test.
- **Real interaction.** jsdom tests navigate outside `act`, with real events.
- **Evidence.** Logs and each task's report (`$E/T<n>-report.md`) go to `$E`, written by Bash (`cat >| … <<'EOF'`). The Write tool is refused under `.superpowers/`. Never under `target/`.
- **One migration, 22, and SQL beyond a plain `SELECT` in Task 4b alone** (settled call 13).
  - Task 4b runs the library, serve and cli crates' tests in `rust:1-bookworm` (SQLite 3.40.1), from a `git -C $W archive HEAD` export under `$W/target/bookworm/`.
  - The recipe is the memory "SQLite 3.40.1 in CI and the image": `--user`; `RUSTUP_TOOLCHAIN` as the image names it, checked with `rustup toolchain list` inside it, at the repo's pinned 1.100.0; `--test-threads=6`.
  - Log to `$E/T4b-bookworm.log`. Only the `strace` tests the memory names may fail.
  - If a later fix changes this SQL, run it again before the Finish.
  - PR A adds no migration (checked against `o280-housekeeping` at the plan's revision). If one lands before the rebase, Task 4b's number moves to the next free one, and its test and `SCHEMA_VERSION` with it.
- **Counts.** `docs/status.md`'s test counts and the closes are Task 10's, after the rebase. Tasks 1–9 leave the totals stale. The gate's `status-test-count` step only runs in the Finish.

## Review Focus

The failure modes the spec implies that no task's tests would otherwise exercise, most likely first. Each one's test is in the task named:
1. **A progress write's `at` taken in the debounced callback**, after a correction swapped the resolver, names the page the new layout shows at the old slot. Task 7 takes it in the effect, and tests it if a swap that keeps the slot can be built. Otherwise the report records it as untested.
2. **`at` not cleared before `as_stored`**, on either side, refuses every echo of a note the account is not sent. Task 4's echo case.
3. **An untouched echo refused** after a stamp, or after any change to stored anchors the tab has not seen. Every later save in that tab is then refused until a reload. Task 4's stamp case (the owner's first ruling), and Task 4c's edits after a stamp (the second).
4. **A narrowed edition's `nth`.** No route fixture repeats a digest, so the translation in `anchor_sent` and in placement would pass untranslated against `router_over_an_edition_merged_from_two_libraries`. Task 3's unit test builds a `View` over an edition with one archive twice, the first copy hidden.
5. **`PageOnly`.** No route fixture has a spread. Tasks 3–5's unit tests build one: a note on a gone half is never placed, and a position on one goes to its page's first slot.
6. **A stale note's anchor sent as the page its stored slot shows**, rather than as stored, would tie it to that page on the next echo. Tasks 2 and 4. And one naming a hidden member must not be sent (Tasks 3 and 5).
7. **Export carrying `at`**, from the CLI or from the reader's own download. Tasks 2 and 6.
8. **A removal's count of the rows of works it removes.** These are already in `positions` and `notes`. Task 8.
9. **A stamp's forwarding applied where the `at` still resolves**, in O270's window, would move a fresh reader's note off a page it is on. Task 4c's guard case. And **an alias that does not collapse** leaves a pre-stamp tab refused after a second stamp: Task 4b's chain cases.

---

### Task 1: The core's page anchor has a wire form, and the binding hands it out (O240, part 1)

Spec §6 decisions 3 and 6. Settled calls 2, 3; spec gap 2.

**Files:**
- `docs/superpowers/plans/2026-11-13-go-public-lane-4-anchors.md`: copy this plan in from `$E/../plans/`. It rides in this task's commit.
- `crates/taiyaku-core/src/sidecar.rs`: `MemberPage`'s derives and `half`'s serde attributes; its doc says it is also the wire's `at` (O240).
- `crates/taiyaku-core/src/sidecar/tests.rs`: beside `every_slot_names_its_member_page_and_that_page_names_the_slot`.
- `crates/taiyaku-wasm/src/binding.rs`: `Binding::member_page`, beside `slot_at`.
- `crates/taiyaku-wasm/src/lib.rs`: `WasmAt` and `memberPage`, beside `slotAt`. A private `half_to_wire` maps `Half` to `"left"`/`"right"`, the way `half_to_str` maps it to `"L"`/`"R"`; its doc says why the two spellings differ (settled call 3).
- `crates/taiyaku-wasm/src/binding/tests.rs`.
- `scripts/smoke-wasm.mjs`: one `check` of `sc.memberPage('b', 2)`.
  - Copy the handle's four fields into a plain object first, as the file's `slotAt` check does: `JSON.stringify` of a wasm-bindgen handle gives its pointer.
  - Then free the handle.
  - Expected: `{sha256: '01', nth: 0, page: 2, half: 'right'}`, the fixture's spread.

- [ ] **Step 1: Copy the plan in.**
- [ ] **Step 2: Write the failing tests.**
  - Core:
    - `MemberPage { sha256: "ab", nth: 1, page: 2, half: None }` serialises to exactly `{"sha256":"ab","nth":1,"page":2}`.
    - With `Some(Half::Left)`, exactly `…,"page":2,"half":"left"}`.
    - Both parse back equal.
    - `{"sha256":"ab","nth":1,"page":2,"slot":3}` is refused.
  - Binding: over a sidecar whose edition holds one archive twice (two members with one `sha256`) and a spread, `member_page` equals `Edition::member_page` for:
    - a whole page of the first copy;
    - a page of the second copy (`nth: 1`);
    - each half of the spread.
    It answers `None` past the end and for an unknown edition.
  - Run `taiyaku-core` and `taiyaku-wasm`. They fail to compile, then fail.
- [ ] **Step 3: Implement.** Green. Then `builtin cd $W && scripts/build-wasm.sh >| $E/T1-wasm.log 2>&1 && node scripts/smoke-wasm.mjs >| $E/T1-smoke.log 2>&1`.
- [ ] **Step 4: Watched mutations.**
  - `half` without `skip_serializing_if`: the whole-page JSON test fails on `"half":null`.
  - `deny_unknown_fields` dropped: the refusal test fails.
  - `Binding::member_page` answering `nth: 0` always: the second-copy case fails.
  - `half_to_wire` answering `"R"`: the smoke check fails.
- [ ] **Step 5: Run and commit.** Run nextest for `taiyaku-core` and `taiyaku-wasm`, then clippy and fmt. Commit `The core's page anchor has a wire form and the binding hands a page's anchor to the web (O240)`. Write `$E/T1-report.md`.

---

### Task 2: The library's notes and positions carry `at`, read by serve alone (O240, part 2)

Spec §6 decisions 3, 5 and 7, and "Where". Settled calls 4, 5, 6.

**Files:**
- `crates/taiyaku-library/src/lib.rs`:
  - `Note.at`, after `stale`. Its doc says it is a wire field like `stale`: the stored anchor, set by `state::notes_anchored_in` and read by serve; `notes`/`notes_in` and an export leave it out; an import ignores it.
  - `Position.at`, after `updated`, the same.
  - `Notes::clear_stale` becomes `clear_wire_fields`, clearing `at` too.
- `crates/taiyaku-library/src/state.rs`:
  - `notes_anchored_in`: `notes_in` with each note's `at` its stored anchor (`note_from_row` already reads it). `notes_in` stays as it answers today.
  - `progress_anchored_in`: the same for positions.
  - The module doc's anchors paragraph names the two.
- The three `clear_stale` callers: `export.rs` (`work`), and `routes.rs`' two in `put_notes` (renamed only; Task 4 moves the first).
- Every `Note { … }` and `Position { … }` literal across the workspace gains `at: None`. Clippy and the compiler name them. In `routes/tests.rs` that includes `mod narrowing`'s `note` helper, which PR A's tests sit near.
- Tests:
  - `crates/taiyaku-library/src/state/tests.rs`, over `crate::tests::a_volume_across_two_directories`;
  - `crates/taiyaku-library/src/export/tests.rs`.

- [ ] **Step 1: Write the failing tests.**
  - `notes_anchored_in` answers a resolving note's stored anchor, and a stale note's as stored:
    - A note on c002's second page (slot 4) is written through `put_notes`. Then `scan::remove_root` takes `a`.
    - The note answers slot 1 and `at` = c002's digest, `nth` 0, `page` 1, `half` none.
    - Then a second fixture: the same note, and `remove_root` takes `b`. It answers `stale`, slot 4 and the same `at`.
    - The case that tells stored from derived: in the second fixture, slot 4 is past c001's three slots, so `member_page` of the slot would answer nothing.
  - `progress_anchored_in`, the same two cases for a position.
  - `notes_in` and `progress_in` answer `at: None` for the same rows.
  - **Export:** a position and a note written through `put_progress`/`put_notes` (each gets an anchor); `export::work` writes `notes.json` and `progress.json`; neither file contains `"at"` or `"sha256"`, and `notes.json` contains no `"stale"`.
  - Run `taiyaku-library`. They fail to compile, then fail.
- [ ] **Step 2: Implement.** Run `taiyaku-library`, `taiyaku-serve` and `taiyaku-cli` to green: the literal changes reach all three.
- [ ] **Step 3: Watched mutations.**
  - `notes_anchored_in` answering `member_page` of the resolved slot instead of the stored anchor: the stale case fails.
  - `notes_in` filling `at`: the `None` case fails.
  - `export::work` reading through `notes_anchored_in`: the export test fails. Then the same for `progress_anchored_in`.
  - `clear_wire_fields` leaving `at`: run the serve crate. Name the test that fails, or record that none does, which Task 4 then closes with its echo case.
- [ ] **Step 4: Run and commit.** Run nextest for `taiyaku-library`, `taiyaku-serve` and `taiyaku-cli`, then clippy and fmt. Commit `A note and a position carry the page they are anchored on, read for serve and left out of an export (O240)`. Write `$E/T2-report.md`.

---

### Task 3: The notes read sends each note's `at` (O240, part 3)

Spec §6 decisions 3–5 and "Where". Settled call 7; spec gaps 3 and 8. Review Focus 4, 5 and 6.

**Files:**
- `crates/taiyaku-serve/src/notes.rs`:
  - `load` calls `state::notes_anchored_in`. Its doc says each note carries its stored anchor.
  - `as_seen` sends each note `at` = `view.anchor_sent(edition, sent slot, note.at.as_ref(), !note.stale)`.
- `crates/taiyaku-serve/src/view.rs`: `View::anchor_sent` and `View::slot_of_anchor`. The second is Task 4's caller and Task 5's; it lands here with its unit tests. PR A edits `etag_naming_numbering`'s doc in the same file; add nothing inside that function.
- Tests:
  - `crates/taiyaku-serve/src/notes/tests.rs`: the unit tests below, over hand-built `Sidecar`s (`Sidecar::from_json`, as this file's slot-count test builds one) and `View::build`, with hand-made `taiyaku_library::browse::SeenMember` flags where narrowed.
  - `crates/taiyaku-serve/src/routes/tests.rs`: new tests **at the end of `mod narrowing`**, and the fallout rule.

- [ ] **Step 1: Write the failing tests.**
  - **Unit, `a_narrowed_editions_anchor_counts_the_members_the_account_is_sent`.**
    - The fixture: edition `raw`, `ltr`, members X (2 pages), Y (2 pages), X again (2 pages, the same `sha256`), and a fourth member H (2 pages). Flags hide the first X and H. The account's document is Y then X.
    - Assert the fixture first: the stored slot 5 (the second X's page 1) is sent as slot 3.
    - `anchor_sent("raw", 3, Some(&{X, nth 1, page 1}), true)` answers `{X, nth 0, page 1}`.
    - `slot_of_anchor("raw", &{X, nth 0, page 1})` answers `Exact(3)`.
    - `slot_of_anchor("raw", &{X, nth 1, page 1})` answers `None`: the account's document holds X once. (In the full edition it is slot 5.)
    - `as_seen` of a stored document with one non-stale note at slot 5, stored `at` `{X, nth 1, page 1}`, sends slot 3 and `{X, nth 0, page 1}`.
    - **Hidden digest** (spec gap 8): `anchor_sent("raw", 3, Some(&{H, nth 0, page 9}), false)` answers `None`. `anchor_sent("raw", 3, Some(&{X, nth 0, page 9}), false)` answers the stored anchor: X is also a sent member's digest.
    - Over a `View` built with no flags (`Access::Every`), `anchor_sent` answers the stored anchor unchanged, resolving or not.
  - **Unit, `a_half_the_layout_no_longer_holds_is_found_on_its_page_only`** (Review Focus 5).
    - The fixture: one edition, one member Z of 3 pages, page 1 a spread, under `Access::Every`.
    - `slot_of_anchor` of `{Z, 0, page 1, half: left}` answers `Exact` of that half's slot.
    - `slot_of_anchor` of `{Z, 0, page 0, half: left}` (page 0 is not a spread) answers `PageOnly` of page 0's slot.
    - `slot_of_anchor` of `{Z, 0, page 1, half: none}` (page 1 is) answers `PageOnly` of page 1's first slot in reading order.
  - **Route, `the_notes_read_sends_each_notes_page`** (`router_over_an_edition_merged_from_two_libraries`):
    - The administrator holds a note on raw slot 6 (part3's second page). Its `GET` sends `at` = part3's digest (read by SQL from `member`), `nth` 0, `page` 1.
    - Manga alone, holding the same note, is sent slot 3 and the same `at`.
  - **Route, `a_stale_notes_read_sends_its_stored_anchor`.**
    - Kids alone's k1 on `raw` (a note Kids is sent), made `stale` the way `kids_alone_echoing_or_dropping_stale_on_a_note_it_is_not_sent_is_accepted` makes k3 stale (`at_sha256 = 'held-by-no-member'`).
    - The `GET` sends k1 with `"stale":true` and `at.sha256 == "held-by-no-member"`.
  - **Route, `a_stale_note_on_a_hidden_member_is_sent_no_anchor`.**
    - Manga alone holds a note on raw slot 1 (part1's second page). By SQL, its anchor becomes part2's digest, `nth` 0, `at_page` 9: part2 is hidden from Manga, and page 9 is past its end.
    - The `GET` sends the note `stale` with no `at`, and the body does not contain part2's digest.
  - Run `taiyaku-serve`. They fail.
- [ ] **Step 2: Implement.** Run `taiyaku-serve`, and apply the fallout rule to every red test.
- [ ] **Step 3: Watched mutations.**
  - `anchor_sent` answering `stored` for a narrowed edition: the unit test's `nth 0` cases fail.
  - `slot_of_anchor` resolving in `self.full` instead of `self.sidecar`: the `Exact(3)` and the `None` cases fail.
  - `slot_of_anchor` answering `PageOnly` as `None`: the half unit test fails.
  - `anchor_sent` sending a hidden digest: the hidden-digest unit case and route test fail.
  - `anchor_sent` withholding every unresolved anchor on a narrowed edition: the `X` case fails.
  - `load` calling `notes_in`: the route `at` assertions fail.
  - `as_seen` sending a stale note the page of its slot: the stale route test fails.
- [ ] **Step 4: Run and commit.** Run nextest for `taiyaku-serve`, then clippy and fmt. Commit `The notes read sends each note's page, in the account's numbering where its edition is narrowed (O240)`. Write `$E/T3-report.md`, with the fallout list.

---

### Task 4: The notes write places a note by its `at`, and keeps an untouched echo (O240, part 4)

Spec §6 decisions 1, 2, 4, 5 and "Where"; §9 "Lane 4"; the owner's 2026-11-13 ruling. Settled call 8; spec gaps 3 and 7. Review Focus 2, 3, 5 and 6.

**Files:**
- `crates/taiyaku-serve/src/notes.rs`: `placed`, and the sentence of settled call 8 (d2). Its doc gives the cases and their order:
  - equality first, because a stale note's stored `nth` cannot be translated;
  - `Exact` only, by pre-deploy decision 6;
  - the untouched echo kept, as `validate_against` exempts an unmoved anchor, so one note never refuses every later write.
- `crates/taiyaku-serve/src/routes.rs`, `put_notes`:
  - The early `incoming.clear_wire_fields()` goes.
  - Inside the guard, after the `If-Match` match, call `crate::notes::placed(&mut incoming, &current, &view, &found.id)?`.
  - Then `incoming.clear_wire_fields()` and `current.clear_wire_fields()`. Their comments say why: `as_stored` compares whole notes, and a held note's `at` and `stale` would differ from every copy sent back.
  - Then `as_stored` and the rest, unchanged. The doc says a valid `at` wins over the slot (O240).
- Tests:
  - `crates/taiyaku-serve/src/notes/tests.rs`: the `PageOnly` unit cases, over Task 3's spread fixture.
  - `crates/taiyaku-serve/src/routes/tests.rs`: new tests **at the end of `mod narrowing`**, using its helpers (`remove_kids_directory`, `reread`, `fetched`, `put`, `notes_uri`, `note`, `stored_notes`). Each case reads the stored `at_` columns by SQL where it asserts an anchor.

- [ ] **Step 1: Write the failing tests** (§9's list, the ruling, and Review Focus 2, 3 and 5):
  - **`a_note_written_with_an_old_slot_lands_on_the_page_its_at_names`.**
    - The administrator. `remove_kids_directory`, then `reread`. Then `GET` for a current tag.
    - `PUT` the document plus note `n` with slot 5 and `at` = part3's digest, `nth` 0, `page` 0.
    - In the old numbering, slot 5 was part3's first page. In the new one it is part3's fourth.
    - Answers 200. The stored note is at slot 2, anchored on part3's page 0.
  - **`a_note_anchored_on_a_page_the_account_is_not_sent_is_refused`.** Manga alone `PUT`s new note `m` with `at` = part2's digest, `nth` 0, `page` 0. It answers 404 with the sentence, written out. The store is unchanged.
  - **`an_at_past_its_members_end_is_refused`.** The administrator, new note, `at` = part1's digest, `page` 9. 404, the sentence.
  - **`an_edited_stale_note_keeps_its_stored_anchor`** (case a; spec decision 8, "an edit keeps its anchor").
    - Kids alone, its note k1 on `raw` (narrowed) made stale with `at_sha256 = 'held-by-no-member'`.
    - `GET`, change k1's `body` alone, and `PUT` it with the `at` it was read with.
    - 200. k1 has the new body, is still `stale` at its stored slot, and its four `at_` columns are unchanged.
    - The body edit is what makes this case (a)'s: an unchanged echo would also be kept by (d1).
  - **`an_echo_of_a_note_the_account_is_not_sent_carrying_an_at_is_accepted`** (Review Focus 2).
    - Kids alone, k3 on `en` as in the existing stale test, held with a real anchor.
    - `PUT` Kids' document plus k3 carrying `at` equal to k3's stored anchor (read by SQL).
    - 200, and k3 is unchanged.
  - **`a_note_sent_without_an_at_is_read_by_its_slot`.** After `remove_kids_directory` and `reread`, a note at slot 2 with no `at` is stored at slot 2, anchored on part3's page 0.
  - **`an_untouched_note_echoed_after_a_renumbering_keeps_its_page`** (spec decision 4).
    - The administrator holds n on slot 6. `GET` for the tag and body. Then `remove_kids_directory` and `reread`.
    - `PUT` with the old tag: 412.
    - Re-read, and `PUT` the new tag with n as first read: old slot 6, `at` part3 page 1.
    - Stored at slot 3, still on part3's page 1. Without `at`, slot 6 is out of the six-slot edition and refused.
  - **`untouched_notes_echoed_after_a_stamp_are_kept`** (the ruling; Review Focus 3).
    - The administrator holds n1 on slot 1 (part1) and n2 on slot 6 (part3). `GET` for the tag and body.
    - Then shape a stamp of part3. By SQL, give part3's `member` row a new `sha256` and move every `at_sha256` naming the old digest to it, as `stamp::rekey_anchors` does. Then `reread`.
    - Assert the fixture first: a fresh `GET` sends n2 at slot 6 with the new digest.
    - `PUT` with the old tag: 412, the stamp's.
    - Re-read, and `PUT` the new tag with the body as first read, n2 carrying the old digest.
    - 200. n2's slot and its four `at_` columns are the new digest's, unchanged, and nothing is `stale`.
  - **`a_stale_note_another_device_moved_is_refused_with_reload_words`** (the ruling).
    - The administrator holds n on raw slot 1, made stale with `at_sha256 = 'held-by-no-member'`. Tab 1 `GET`s.
    - Tab 2 `GET`s and `PUT`s n with a region and `at` = part1's page 1, as a region move re-anchors it.
    - Tab 1 `PUT`s its first body with the old tag (412), re-reads, and `PUT`s its first body again with the new tag.
    - 404 with the sentence, written out, `Reload the page` in it. The store holds tab 2's n.
  - **`placed`, unit, `a_note_on_a_half_the_layout_no_longer_holds_is_not_placed`** (Review Focus 5). Over Task 3's spread fixture:
    - A new note whose `at` is `{Z, 0, page 0, half: left}` is refused, `Invalid::Missing`, with the sentence.
    - A held note with that stored `at`, sent back with only its `body` changed, keeps its slot (case a).
  - Run `taiyaku-serve`. They fail.
- [ ] **Step 2: Implement.** Green. Run the whole serve crate and apply the fallout rule.
- [ ] **Step 3: Watched mutations.**
  - `placed` made a no-op, the slot preferred: the first and the untouched-echo cases fail.
  - Case (b) alone skipped: the first case fails. The untouched echo still passes through case (a), which is right.
  - `slot_of_anchor` replaced by a lookup in `view.full()`: the Manga refusal case fails, with 200.
  - A past-end `at` read by its slot (case e) instead of refused: the past-end case fails, with 200.
  - Case (a) dropped: the edited-stale case and the unit body-edit case fail, with 404. (d1) does not rescue them: the body differs.
  - (d1) dropped: the stamp case fails, with 404.
  - (d1) comparing ids alone: the moved-note case fails, with 200.
  - `PageOnly` accepted for a note: the unit new-note case fails.
  - Either `clear_wire_fields` call dropped, one at a time: the k3 case fails.
  - A note with no `at` refused: the no-`at` case fails.
- [ ] **Step 4: Run and commit.** Run nextest for `taiyaku-serve`, then clippy and fmt. Commit `A notes write places each note on the page its at names, keeps an untouched echo, and refuses a page the account is not sent with words that say to reload (O240)`. Write `$E/T4-report.md`.

---

### Task 4b: A stamp remembers the digests it moved (O240, part 4b; migration 22)

The owner's 2026-11-13 ruling "Stamp remembers fingerprints". Settled calls 13 and 16; spec gap 7. Review Focus 9.

**Files:**
- `crates/taiyaku-library/src/schema/022_stamp_aliases.sql`: the table in New interfaces. Its comment says why, in the voice of `021_anchors.sql`:
  - a stamp re-keys stored anchors, and a tab or device that read before it still names the old digest;
  - keyed by work and edition, for settled call 16's two reasons;
  - the cascade.
- `crates/taiyaku-library/src/schema.rs`: `SCHEMA_VERSION` 22, `MIGRATIONS` gains the file.
- `crates/taiyaku-library/src/stamp.rs`:
  - `remember_moves` and `alias_of`, beside `rekey_anchors`, each documented. `remember_moves` reads the edition's aliases, composes in memory, then deletes and inserts them, all on the connection it is handed.
  - `rekey_anchors` hands its `moves` to `remember_moves` on the same `tx`, after it moves the rows. So the aliases commit or roll back with `record_rows`' transaction.
  - `rekey_anchors`' doc names the aliases (O240).
- Tests:
  - `crates/taiyaku-library/src/schema/tests.rs`: `the_schema_version_is_twenty_one` becomes `…_twenty_two`. A migration 22 test is shaped like `migration_21_gives_progress_and_note_a_page_beside_the_slot`.
  - `crates/taiyaku-library/src/state/tests.rs`, beside `stamp_the_volume` (private to that file): a real stamp's aliases.
  - `crates/taiyaku-library/src/stamp/tests.rs`:
  - `remember_moves`' and `alias_of`'s unit cases.

- [ ] **Step 1: Write the failing tests.**
  - **`migration_22_gives_a_stamp_a_memory_of_the_digests_it_moved`.**
    - `migrate_with(&conn, &MIGRATIONS[..21])`, with foreign keys on as `Library::open` sets them. Seed one `work` row, as the 21 test seeds its store.
    - `migrate`: version 22. `stamp_alias` has exactly the six columns, in order.
    - An alias row inserts. A second with the same `(work_id, edition, was_sha256, was_nth)` is refused by the primary key.
    - Deleting the work deletes its alias.
  - **A real stamp records its moves** (`a_volume_across_two_directories`, `stamp_the_volume`).
    - Read c001's and c002's digests before.
    - After the stamp, `alias_of(work, "raw", c001_old, 0)` answers `(c001_new, 0)`, and the same for c002.
    - `alias_of` for a digest no stamp moved answers `None`.
  - **`remember_moves`, unit, over an in-memory store with two `work` rows seeded** (the table's foreign key needs them):
    - **A chain:** `{(A,0)→(B,0)}`, then `{(B,0)→(C,0)}`. `alias_of(A,0)` answers `(C,0)`, and `alias_of(B,0)` answers `(C,0)`.
    - **A return:** `{(A,0)→(B,0)}`, then `{(B,0)→(A,0)}`. `alias_of(A,0)` answers `None` (no self-alias), and `alias_of(B,0)` answers `(A,0)`.
    - **An occurrence that moves without its digest** (rekey's "a copy the stamp does not rewrite moves too"): `{(X,0)→(Y,0), (X,1)→(X,0)}`. Both stand.
    - **Per work and edition:** the same move recorded on work 1's `raw` answers `None` for work 2's `raw` and for work 1's `en`.
  - Run `taiyaku-library`. They fail to compile, then fail.
- [ ] **Step 2: Implement.** Run `taiyaku-library`, `taiyaku-serve` and `taiyaku-cli` to green (`taiyaku-cli`'s `detect` tests read the store too).
- [ ] **Step 3: Watched mutations.**
  - `rekey_anchors` not calling `remember_moves`: the real-stamp case fails.
  - `remember_moves` adding the new moves without redirecting stored targets: the chain case fails on `alias_of(A,0)`, answering `(B,0)`.
  - Self-aliases kept: the return case fails.
  - `alias_of` reading without `work_id`, or without `edition`: the per-work case fails.
  - The cascade dropped from the migration: the migration test's delete case fails.
  - Not testable here: `remember_moves` called after `record_rows`' commit instead of inside it. No test seam makes that transaction fail between the two. Say so in the report; the review checks it by reading.
- [ ] **Step 4: SQLite 3.40.1.** Run the library, serve and cli crates in `rust:1-bookworm` as Global Constraints says. Log to `$E/T4b-bookworm.log`.
- [ ] **Step 5: Run and commit.** Run nextest for `taiyaku-library`, `taiyaku-serve` and `taiyaku-cli`, then clippy and fmt. Commit `A stamp remembers each member's digest before and after it, so a save from before the stamp can be forwarded (O240, migration 22)`. Write `$E/T4b-report.md`.

---

### Task 4c: A note saved with a stamped-away digest is forwarded before placement (O240, part 4c)

The owner's 2026-11-13 ruling "Stamp remembers fingerprints"; Task 4's review, Important 2. Settled calls 8 and 16; spec gap 7. Review Focus 3 and 9.

**Files:**
- `crates/taiyaku-serve/src/works.rs`: `past_stamps`, documented: the guard and why (O270), and that no alias is ever sent.
- `crates/taiyaku-serve/src/routes.rs`, `put_notes`, inside the guard, before `crate::notes::placed`:
  - find the work's row (`works::find_row`);
  - replace each incoming note's `at` with `past_stamps`' answer.
  - An alias read that fails answers `NoNotes::Failed`, path-free, as the store's other failures do.
  - The comment says why it runs before `placed`: (a) compares with the stored anchor, which the stamp re-keyed.
- `crates/taiyaku-serve/src/routes/tests.rs`, at the end of `mod narrowing`:
  - A helper `stamp_part3(merged) -> (String, String)` (old and new digest). It does what `untouched_notes_echoed_after_a_stamp_are_kept` does by SQL today: a new `sha256` on part3's `member` row, and every `at_sha256` naming the old digest moved to it. It then calls `taiyaku_library::stamp::remember_moves` with that one move on `raw`, as `record_rows` would, and `reread`s.
  - That test then uses the helper, its assertions unchanged.
  - A second call stamps part3 again (`mid → new`).

- [ ] **Step 1: Write the failing tests.**
  - Every case but the untouched one: the administrator holds n2 on raw slot 6 (part3's page 1). A `GET` for the tag and body before the stamp is the pre-stamp tab. After the stamp, re-read for a current tag, then `PUT` that tag with the body named, carrying the old digest as the tab would. Each asserts the row by SQL (`row`, `on`).
  - **`a_body_edit_from_before_a_stamp_saves`:** n2's `body` changed, `at` as read. 200. The body is saved, and the row is at slot 6 on the new digest's page 1.
  - **`a_colour_edit_from_before_a_stamp_saves`:** the same, `color` changed.
  - **`a_new_note_from_before_a_stamp_saves`:** a new note `m` at slot 5 with `at` = the old digest's page 0. 200. `m` is at slot 5 on the new digest's page 0.
  - **`a_region_move_from_before_a_stamp_saves`:** n2 with a new `region` and `at` = the old digest's page 1, as `anchorNote` re-anchors from the tab's sidecar. 200. n2 has the new region, at slot 6, on the new digest's page 1.
  - **`an_untouched_note_after_a_stamp_and_a_renumbering_saves`.**
    - The renumbering is a grant, not a removal. A removal derives, and a derive before the next scan puts part3's old digest back (O270), so the old `at` would resolve and nothing would be forwarded.
    - Manga alone holds n2 on raw (full slot 6, Manga's slot 3), and its `GET` is the tab.
    - Then `stamp_part3`, and grant Manga alone's account the Kids library (`taiyaku_library::account::grant_library`, library 80). Manga's `raw` is now sent whole, and n2 is slot 6.
    - Send n2 as first read: slot 3, the old digest. 200. n2's row is at full slot 6 on the new digest's page 1.
    - Without forwarding, (a) and (b) fail on the old digest, and (d1) fails on the slot.
  - **`a_body_edit_from_before_two_stamps_saves`:** `stamp_part3` twice. The body edit carrying the first digest saves on the newest.
  - **`a_digest_no_stamp_recorded_is_still_refused`:** after `stamp_part3`, a new note with `at` = a digest no member and no alias names (`never-stamped`). 404 with the sentence, written out.
  - **`a_digest_the_member_still_shows_is_not_forwarded`** (Review Focus 9; O270's window).
    - `stamp_part3`, then put part3's `member` row back on the old digest by SQL, as a derive before the next scan would, and `reread`. The alias stays.
    - Assert the fixture first: the document sent names part3 by the old digest.
    - A new note with `at` = the old digest's page 0 saves at slot 5, anchored on the old digest.
  - Run `taiyaku-serve`. All but the refusal and the O270 case fail with 404.
- [ ] **Step 2: Implement.** Green. Run the whole serve crate: Task 4's tests pass unchanged. Its stamp test now passes through case (a), not (d1).
- [ ] **Step 3: Watched mutations.**
  - `past_stamps` answering `at` unchanged: each of the six saving cases fails, with 404.
  - `past_stamps` forwarding without the guard: the O270 case fails, with 404 (the forwarded digest names no member).
  - `alias_of` matched on `nth` alone, ignoring the digest: the never-stamped case fails, with 200.
  - The forwarding run after `placed` instead of before: the body, colour and untouched cases fail. (a) compares with the re-keyed stored anchor before anything forwards it.
  - The chain collapse removed in `remember_moves` (Task 4b's mutation, run here too): the two-stamp case fails.
- [ ] **Step 4: Run and commit.** Run nextest for `taiyaku-serve`, then clippy and fmt. Commit `A note saved with a digest a stamp moved is forwarded to the new one before it is placed, so a tab from before the stamp saves (O240)`. Write `$E/T4c-report.md`.

---

### Task 5: A position's read and write carry its `at` (O240, part 5)

Spec §6 decisions 3–5 and "Where"; §9 ("a position PUT follows the same rules"). Settled calls 9, 10 and 16; spec gaps 4 and 8. Review Focus 5 and 6.

**Files:**
- `crates/taiyaku-serve/src/progress.rs`:
  - `load` calls `state::progress_anchored_in`.
  - `Body`, with its doc: serve's wire type, so `Progress` stays the library's write shape.
  - `Placed` and `place` (settled call 9a–c), with their doc.
- `crates/taiyaku-serve/src/routes.rs`:
  - `put_progress` takes `axum::Json<crate::progress::Body>`.
  - Inside the closure, a body with `at` first forwards it past any stamp with `works::past_stamps`, the same function the notes write calls (Task 4c; settled call 16). It then reads this reader's stored position on that edition through `progress::load`, and calls `progress::place` with the forwarded `at`.
    - `Slot` goes through `view.slot_to_full` and `store` as today.
    - `Kept` answers 204 without writing.
    - `Refused` answers `NoProgress::NotFound` with the sentence.
  - A body without `at` takes today's path. The doc says a valid `at` wins over the slot, and why equality comes second.
  - `get_progress`: in `retain_mut`, each kept position's `at` is `view.anchor_sent(edition, sent slot, stored, resolves)`. `resolves` is whether `view.full().editions.get(…)` and `slot_of` find the stored anchor.
- Tests:
  - `crates/taiyaku-serve/src/progress/tests.rs`: `place`'s unit cases over Task 3's spread fixture, and the fallout.
  - `crates/taiyaku-serve/src/routes/tests.rs`: new tests **at the end of `mod narrowing`** (`put_raw_as_the_administrator`, `positions`, `progress_uri`, `put`).

- [ ] **Step 1: Write the failing tests.**
  - **Unit, `place`** over the spread fixture: `{Z, 0, page 0, half: left}` answers `Slot` of page 0's slot (the page's first slot); an `at` on no member answers `Kept` when it equals `stored` and `Refused` when it does not.
  - **`a_position_written_with_an_old_slot_lands_on_the_page_its_at_names`.** After `remove_kids_directory` and `reread`, the administrator `PUT`s `{"edition":"raw","slot":5,"at":{part3's digest, nth 0, page 0}}`. 204, and `positions` answers `("raw", 2)`.
  - **`a_position_anchored_on_a_page_the_account_is_not_sent_is_refused`.** Manga alone, `at` on part2. 404 with the sentence, and no row written.
  - **`a_page_turn_from_before_a_stamp_saves`** (settled call 16).
    - After Task 4c's `stamp_part3`, the administrator `PUT`s `{"edition":"raw","slot":6,"at":{the old digest, nth 0, page 1}}`.
    - 204. The row is at slot 6, anchored on the new digest's page 1.
  - **`a_position_naming_its_own_lost_page_is_left_as_it_is`.**
    - The administrator stands on raw slot 3 (part2's page 1). Then `remove_kids_directory` and `reread`.
    - `PUT` `{"edition":"raw","slot":3,"at":{part2's digest, nth 0, page 1}}`.
    - 204. The row's slot and four `at_` columns are as before.
  - **`a_position_sent_without_an_at_is_read_by_its_slot`.** After the same renumbering, `{"edition":"raw","slot":2}` lands on slot 2. A slot of 99 is still refused in O3's words.
  - **`the_progress_read_sends_each_positions_page`.**
    - The administrator on raw slot 6: the `GET` sends `at` = part3's page 1.
    - After `remove_kids_directory` and `reread`, a position on part2's page sends its stored anchor as stored, beside the clamped slot.
    - Manga alone, its position on raw slot 1 given part2's digest and `at_page` 9 by SQL: the `GET` sends no `at`, and the body does not contain part2's digest (spec gap 8).
  - **`a_position_turned_back_to_in_one_edition_is_the_one_resumed`** (spec gap 4).
    - The administrator writes raw slot 6 with its `at`, then `en` slot 0.
    - Then backdate both rows by SQL, raw's `updated` to `2026-01-01 00:00:00` and en's to `2026-01-02 00:00:00`. `updated` is `datetime('now')` in whole seconds, and writes in one second tie, broken by edition name, so without this the case cannot tell right from wrong.
    - Then write raw slot 6 with the same `at` again. `positions`' head is `raw`.
  - Run `taiyaku-serve`. They fail.
- [ ] **Step 2: Implement.** Green. Run the whole serve crate and apply the fallout rule.
- [ ] **Step 3: Watched mutations.**
  - `place` answering `PageOnly` as unresolved: the unit `Slot` case fails.
  - Case (a) skipped, the slot preferred: the first route case fails.
  - Resolution in `view.full()`: the Manga case fails, with 204.
  - Case (b) dropped: the lost-page case fails, with 404.
  - Case (b) written through the slot instead of left: the lost-page case fails on the `at_` columns.
  - Equality asked before resolution, leaving the row unwritten: the resume case fails.
  - `load` calling `progress_in`: the read case fails.
  - `get_progress` passing `resolves: true` always: the hidden-digest case fails.
  - `put_progress` not calling `works::past_stamps`: the stamp case fails, with 404.
- [ ] **Step 4: Run and commit.** Run nextest for `taiyaku-serve`, then clippy and fmt. Commit `A position is read with its page and written by it, and a write naming its own lost page leaves the row as it is (O240)`. Write `$E/T5-report.md`, with the fallout list.

---

### Task 6: The web's notes, progress and resolver carry `at` (O240, part 6)

Spec §6 decisions 1, 2, 6, 7 and "Where"; §9 vitest. Settled call 12; spec gap 5.

**Files:**
- First: `builtin cd $W && scripts/build-wasm.sh >| $E/T6-wasm.log 2>&1`.
- `web/src/resolver.ts`: `At`, `Resolver.memberPage`, and `wasmResolver`'s `memberPage`, which reads `sc.memberPage`'s fields into a plain `At` and frees the handle, as `slotAt` does.
- `tests/support/fixtures.ts` (`fakeResolver`) and `tests/correction.test.ts`' resolver: `memberPage`, derived from `slotAt` so that two slots never share an anchor.
- `web/src/notes.ts`:
  - `Note.at` and `Note.stale`, each documented.
  - `newNote` sets `at` from `anchor`.
  - `anchorNote` sets `at` = `anchorOf(note.edition, note.slot)` and drops `stale`.
  - `notesJson` writes the document with each note's `at` and `stale` left out. Its doc says an export stays anchor-free (spec decision 7).
- `web/src/App.tsx`, **the two call sites only** (the controller's B1 ruling), so this task passes its own `tsc -b`:
  - `startNote` passes `current.resolver.memberPage(shown.edition, shown.slot)` to `newNote`.
  - `endRegion` passes an `anchorOf` that reads `sessionRef.current?.resolver.memberPage` to `anchorNote`.
  - Their tests are Task 7's.
- `web/src/api.ts`: `StoredProgress.at`. `putProgress` sends the object as given. `getProgress` answers `{edition, slot}` as today.
- Tests: `web/tests/notes.test.ts`, `notesWrite.test.ts`, `resolver.test.ts`, `api.test.ts`.

- [ ] **Step 1: Write the failing tests.**
  - `notes.test.ts`:
    - `newNote` with an anchor puts it on the fresh note.
    - `anchorNote` on a `stale` note sets `at` to what `anchorOf` answers for that note's edition and slot, and drops `stale`. On a non-stale note, the same `at` rule holds.
    - `notesJson` of a document whose notes carry `at` and `stale: true` has neither key, and keeps every other field.
    - Pins: `changeShown` with a body change keeps `at` and `stale`; `editsToWrite` carries an edited note's `at`. Both hold before this task.
  - `notesWrite.test.ts`, `saveNotes`:
    - The first `put` refuses as stale. The re-read's notes carry `at`s of their own.
    - The retry body's notes are this reader's, each with the `at` it was given, and the re-read's other notes keep theirs. Assert per note.
  - `resolver.test.ts`, over the mocked `WasmSidecar`:
    - `memberPage` answers the mock's fields as an `At`, and frees the handle.
    - `undefined` passes through.
  - `api.test.ts`: `putProgress(work, {edition, slot, at})` sends `at` in the body. Before this task only `tsc` refuses the call: `sendProgress` stringifies what it is given.
  - Run the node project. They fail.
- [ ] **Step 2: Implement.** Green on the node and jsdom projects. `tsc -b` and `biome` clean. Run the browser project too: `fakeResolver` is a browser fixture.
- [ ] **Step 3: Watched mutations.**
  - `newNote` ignoring `anchor`: its case fails.
  - `anchorNote` keeping `stale`, or keeping the old `at`: its cases fail.
  - `mergeOver` rebuilding notes without `at`: the retry case fails.
  - `notesJson` as `JSON.stringify(doc, null, 2)`: its case fails.
  - `wasmResolver.memberPage` mapping `half` through the `L`/`R` spelling: the resolver case fails.
  - `sendProgress` building `{edition, slot}` from its argument: the api case fails.
- [ ] **Step 4: Run and commit.** Run the node and jsdom projects, `tsc -b` and `biome check`, and the browser project. Commit `The reader's notes and positions carry the page they were made on, and its notes export leaves it out (O240)`. Write `$E/T6-report.md`.

---

### Task 7: The reader sends a page's `at` and draws a note whose page is gone (O240, part 7)

Spec §6 decisions 1, 4, 6, 8 and "Where". Settled call 12. Review Focus 1.

**Files:**
- `web/src/App.tsx`, **the progress write**:
  - `writeProgress`' debounce carries a `StoredProgress`.
  - The function handed to `useProgressWrite` adds `at` from the session's resolver when it is called, before the debounce. It must be built once (`useConstant`), since `useProgressWrite` re-runs its effect when `write` changes.
  - It reads the session through `sessionRef`. `sessionRef`'s `useLatest` effect is declared before `useProgressWrite`'s, so it runs first in the same commit; the comment says so.
- `web/src/NotePanel.tsx`:
  - The edit surface draws the stale paragraph under the anchor line when `draft.stale`.
  - The review row's meta text gains the suffix for a `stale` note.
  - No CSS: the existing `reader-meta`/`reader-muted` classes.
- Tests: `web/tests/App.test.tsx` (its `ports` and `fakeResolver`), `web/tests/NotePanel.test.tsx`.

- [ ] **Step 1: Write the failing tests.**
  - `App.test.tsx`:
    - **A new note's `at` is the resolver's** (Task 6's `startNote` call site). Press `n`, type a word, let the debounce run. The note in `putNotes`' document carries `at` equal to `fakeResolver`'s `memberPage` for the slot shown. Assert that value written out from the fixture.
    - **A region drawn re-anchors** (Task 6's `endRegion` call site): a region drawn on a `stale` note sends it with the `memberPage` of its slot and no `stale`.
    - **A page turn's `at`.** Turn a page and let the debounce run. `putProgress`' second argument carries `at` equal to `memberPage` for the new slot.
    - **The `at` is the turn's** (Review Focus 1).
      - Turn a page. Before the debounce fires, replace the session's resolver with one whose `memberPage` answers differently, through the seam a correction's re-read uses (`ports.wasmResolver`, as `a landed write installs the new resolver, frees the old one, and moves the reader` does).
      - The `at` sent is the first resolver's.
      - That swap also moves the reader. If no swap can be built that keeps the slot, record Review Focus 1 as untested in the report. The two-turn case below makes no claim about it.
    - **Two turns inside one debounce** send one `PUT`, whose `at` is the last slot's.
  - `NotePanel.test.tsx`:
    - A `stale` draft draws the stale paragraph, written out, and a non-stale one does not.
    - A review entry for a `stale` note ends ` · no longer on its page`.
  - Run the jsdom project. They fail.
- [ ] **Step 2: Implement.** Green. Run the browser project too (`App.browser.test.tsx` drives the same writers).
- [ ] **Step 3: Watched mutations.**
  - `startNote` passing `undefined`: the new-note case fails.
  - `endRegion` passing an `anchorOf` that answers `undefined`: the region case fails.
  - `at` taken inside the debounced callback from `sessionRef`: the swap case fails, if it exists.
  - The progress write without `at`: the turn case fails.
  - The paragraph's condition inverted: the NotePanel case fails.
- [ ] **Step 4: Run and commit.** Run the jsdom project, `tsc -b`, `biome check`, and the browser project. Commit `The reader sends the page each note and turn is on, and draws a note whose page is gone (O240)`. Write `$E/T7-report.md`.

---

### Task 8: A removal counts the rows on kept works whose page it takes (O244, part 1)

Spec §6 decision 9 and "Count". Settled call 14; spec gap 6. Review Focus 8.

**Files:**
- `crates/taiyaku-library/src/state.rs`:
  - `Resolving`, `resolving` and `lost_since`, in the anchors section.
  - `resolving` reads every progress and note row with an anchor (`at_from_columns`), each edition's layout once (`layout_in` with `Numbering::Store`, as `layouts_in` does).
  - It records whether each row's anchor resolves by the read's own rule (settled call 14).
  - `lost_since` counts rows that resolved in `before` and stand, unresolved, in `self`.
- `crates/taiyaku-library/src/scan.rs`:
  - `Removal`'s two fields, with docs.
  - `apply_removal` and `apply_library_removal` take `state::resolving` before their deletes and again after their derive, and fill the fields. `apply_library_removal` may skip both reads when it removes no directory.
  - The docs of `removal_cost`, `library_removal_cost`, `remove_root` and `remove_library` say both answer it.
  - The two test literals of `Removal` (scan/tests ~11120, ~11227) gain the fields.
- `crates/taiyaku-serve/src/config_api.rs`: `removal_json` destructures and sends both, under the same names.
- Tests: `crates/taiyaku-library/src/scan/tests.rs`; `crates/taiyaku-serve/src/config_api/tests.rs` and `libraries_api/tests.rs` (a dry run's body).

- [ ] **Step 1: Write the failing tests.**
  - **`removal_cost` over `a_volume_across_two_directories`.** Hold, for user 1:
    - a position on c002's first page (slot 3);
    - a note on c002's second page (slot 4);
    - a note on c001's second page (slot 1);
    - a note already stale before the removal (`at_sha256 = 'held-by-no-member'`).
  - Then:
    - `removal_cost(b)`: `positions_page_gone == 1`, `notes_page_gone == 1`, `works == 0`.
    - `removal_cost(a)`: `positions_page_gone == 0` (c002's pages move down and still resolve), `notes_page_gone == 1` (c001's).
    - `remove_root(b)` answers the same as `removal_cost(b)`.
  - **Rows on a removed work are not counted.** `library_with_two_roots_and_a_reading_position`: `removal_cost(root b)` answers `positions 1, notes 1, positions_page_gone 0, notes_page_gone 0`.
    - Assert the fixture first: root b's own position and note carry anchors.
    - The fixture seeds them by SQL, and its position sits past the end of its four slots, so its `at_` columns are NULL. Write the position through `state::put_progress` at an in-range slot (0–3), and the note through `state::put_notes`, in this test. A row with no anchor is never in `Resolving`, and the "absent after counted as unresolved" mutation would then pass.
  - **`library_removal_cost`.** The volume with directory b in a library of its own.
    - Either add `a_volume_across_two_directories`' second directory with `InLibrary::Named` of another name behind a parameter, or move it with `relocate::move_root` before the cost. This is the implementer's choice, named in the report.
    - Assert the fixture first: the work survives the removal.
    - Then the same figures as `removal_cost(b)`.
  - **Serve:** a dry run's `removed` object carries `positions_page_gone` and `notes_page_gone`, for a directory (`config_api/tests.rs`) and a library (`libraries_api/tests.rs`). The figures need not be above 0: the keys are the claim.
  - Run `taiyaku-library` and `taiyaku-serve`. They fail.
- [ ] **Step 2: Implement.** Green.
- [ ] **Step 3: Watched mutations.**
  - `lost_since` counting every unresolved row in `self`: the already-stale note is counted, and the `removal_cost(b)` case fails.
  - Counting every anchored row of a kept work: the c001 note is counted, and the same case fails.
  - A row absent after counted as unresolved: the two-roots case fails.
  - A position resolving only on `Exact`: unchanged here (no spread). Say so in the report rather than claim the rule is tested.
  - The JSON key misspelt: the serve case fails.
- [ ] **Step 4: Run and commit.** Run nextest for `taiyaku-library` and `taiyaku-serve`, then clippy and fmt. Commit `A removal counts the positions and notes on works it keeps whose page it takes (O244)`. Write `$E/T8-report.md`.

---

### Task 9: The Remove dialogs say whose page a removal takes (O244, part 2)

Spec §6 "Count". The sentence above.

**Files:**
- `web/src/config.ts`: `Removal`'s two fields, each documented.
- `web/src/libraries.ts`: `pagesGoneSentence`, beside `noLibrarySentence`.
- `web/src/RemoveRoot.tsx` and `web/src/RemoveLibrary.tsx`: the `pages-gone` paragraph after the counts, when either figure is above 0.
- Every `Removal` literal in `web/tests` gains both fields at 0: `tests/support/libraries.ts`, `RemoveRoot.test.tsx`, `RemoveLibrary.test.tsx`, `RootRow.test.tsx`, `Libraries.test.tsx`, `LibraryPage.test.tsx`, `DirPage.test.tsx`, and the browser files `Config.browser.test.tsx`, `Libraries.shell.browser.test.tsx`, `SideBySide.browser.test.tsx`, `fieldNames.browser.test.tsx`. `tsc -b` names any other.
- Tests: `web/tests/libraries.test.ts`, `RemoveRoot.test.tsx`, `RemoveLibrary.test.tsx`.

- [ ] **Step 1: Write the failing tests.**
  - `pagesGoneSentence(2, 1)`, `(0, 1)` and `(1, 0)`, each written out.
  - Each dialog with a cost of `positions_page_gone: 2, notes_page_gone: 1` draws the paragraph, written out. With both 0, there is no `.pages-gone` element.
  - Run the node and jsdom projects. They fail.
- [ ] **Step 2: Implement.** Green on node and jsdom. `tsc -b` and `biome` clean. Then the browser project, since its fixtures changed.
- [ ] **Step 3: Watched mutations.**
  - The sentence drawn from `removed.positions`/`removed.notes`: the dialog cases fail.
  - The condition `>= 0`: the zero case fails.
  - `pagesGoneSentence` always naming both parts: the `(0, 1)` case fails.
- [ ] **Step 4: Run and commit.** Run node, jsdom, `tsc -b`, `biome check` and the browser project. Commit `The Remove dialogs name the positions and notes on works that stay whose page the removal takes (O244)`. Write `$E/T9-report.md`.

---

### Task 10: The closes in `docs/status.md`, after the rebase onto PR A

The controller rebases the branch onto main after PR A merges, and runs the whole workspace and the web projects green on the rebased head before this task starts. Read the Open items section, O240's row (~1863) and O244's (~1867), and their notes bullets (~4017, ~4047), as they stand after the rebase.

- [ ] **Step 1: Close O240 and O244** with the five-part edit:
  1. The row's checkbox.
  2. The row's text, rewritten to say what was done, with symbols and no line numbers:
     - **O240:**
       - The web sends `at` on each note and position, computed through the binding's `memberPage`.
       - The server places a write by it, keeps a stored anchor echoed unchanged and an untouched echo (the owner's 2026-11-13 ruling), and refuses a page the account is not sent, in words that say to reload.
       - A 412's retry sends each note with its `at`. No reload on a 412: decision 11 stands.
       - The position's validator gap is closed by `at`, not by a tag.
       - A stamp remembers each member's digest before and after it in `stamp_alias` (migration 22). A save naming a stamped-away digest is forwarded to the new one before it is placed (`works::past_stamps`), except where the old digest still resolves (O270's window). No alias is ever sent. This is the owner's 2026-11-13 ruling "Stamp remembers fingerprints".
       - Name settled calls 8 and 9's order, and spec gap 7's remaining refusals, which still stick until a reload.
     - **O244:**
       - The remainder is done: both removals answer `positions_page_gone` and `notes_page_gone`, and the two dialogs show them.
       - `stale` is drawn in the note panel.
       - *Not anchored* (`page_flag`, `run`, `truth_page`) stays as written.
       - O271 is neither closed nor narrowed.
  3. The notes bullet.
  4. The "N are now closed — leaving M" sentence (~1496). O240 and O244 go in the closed list in number order.
  5. The "What remains … M open items" line (~5253). Its count word wraps across a line break, so grep the sentence opener and read the next line.

  Then the edits outside the two rows:
  - **"What is built"** (~138-144): after "its positions and notes are kept in the full numbering", say that the reader sends each one's page with it, so a write read before a renumbering lands on its page (O240).
  - **The pre-deploy design** (`docs/superpowers/specs/2026-11-10-pre-deploy-design.md`): mark decision 4 (~391), decision 8 (~404) and the Boundaries' "web half" bullet (~554) done in go-public lane 4, as lane 3 marked decision 5.
  - **O270's row** (open): one sentence saying that in its window, a save naming the old digest is placed by it and not forwarded (settled call 16's guard). O270 stays open and is not narrowed.
  - **The `taiyaku-library` row** (~390), if it lists the store's tables: add `stamp_alias` (migration 22).
- [ ] **Step 2: Counts.** Measure them, on the rebased head; never add.
  - **The Rust total:** run `env -u TAIYAKU_CORPUS cargo nextest run --manifest-path $W/Cargo.toml --workspace --all-features --test-threads=8`, then `node $W/scripts/status-test-count.mjs $W/target/nextest/default/junit.xml $W/docs/status.md`. Set "Six crates, N tests" (~181) and "of the N are standing checks" (~258) to the figure it wants.
  - **The web counts** in the same sentence: set them from the node and jsdom runs' summary lines.
  - The browser count (~182-183): from the browser run's summary line, if the sentence states one.
  - **`scripts/comment-budget.json`:** run `node $W/scripts/check-comments.mjs`, and take any changed share from its message, per `docs/ci.md`.
- [ ] **Step 3: Sweep.**
  - Run `grep -a -c '^| O[0-9]' docs/status.md`, `grep -a -c '^| O[0-9]* | \[x\]' docs/status.md` and `grep -a -n -A1 '^What remains:' docs/status.md`. Check each against its sentence. The pre-flight expects 281 rows, 188 closed and 93 open once PR A and this lane are both in. Measure; do not copy.
  - Grep `O240` and `O244` bare across `docs/`, `web/src/`, `crates/` and `scripts/`, and read every hit.
    - Prose that speaks of either as open is marked or corrected. That includes `notes.ts`, `state.rs` and `scan.rs` docs that say the web "does not yet" send anchors, or that the rehearsal does not count.
    - A hit in an older plan or spec under `docs/superpowers/` is history and stays, except the pre-deploy design in Step 1.
- [ ] **Step 4: Commit.** Commit `O240 and O244 close in docs/status.md`. Write `$E/T10-report.md`.

---

### Finish

1. **Whole-branch review** of `BASE..HEAD` on the rebased branch, against the spec, the rulings, this plan's settled calls and its spec gaps, in three lanes:
   - (a) the server wire: Tasks 1–5, 4b and 4c. The reviewer walks settled calls 8, 9 and 16 case by case against the code, and Review Focus 2–6 and 9. That includes that `remember_moves` runs inside `record_rows`' transaction, which no test can show.
   - (b) the web: Tasks 6–7, with Review Focus 1 and 7.
   - (c) the count and the docs: Tasks 8–10, with Review Focus 8.

   One fix wave goes back to the implementers by SendMessage. After fixing a finding, grep for its shape, not its function. Review the fixes too.
2. **Final gate** on the final commit, after the rebase onto PR A, in the fresh worktree, at load < 24. Read the `.exit` file. The gate includes PR A's coverage guard, the wasm build and smoke run, the browser project and the coverage floors.
3. **The drive.** Start the server from the controller, or tell the agent never to restart it.

   **Setup:**
   - Record the test library's state: `sha256sum /home/davey/temp/manga-test/library.db`, `stat` it, and `touch` a marker in the scratch directory.
   - Build:
     - `cp` the db into a scratch directory under the session scratchpad;
     - build a release `taiyaku-cli` from `$W` and copy the binary in;
     - `builtin cd $W && scripts/build-wasm.sh`, then `node $W/web/node_modules/vite/bin/vite.js build $W/web --outDir <scratch>/dist --emptyOutDir`.
   - Check the port is free (`ss -lptn 'sport = :4404'`).
   - Serve: `<bin> serve --db <scratch>/library.db --cache <scratch>/cache --web <scratch>/dist --addr 127.0.0.1:4404 --insecure-cookies`, with its stderr in `$E/drive-server.log`.
   - `printf 'pw\npw\n' | <bin> library user password local --db …`.
   - **A scratch tree** beside the copy, never under the test library: three distinct small `.cbz` files copied (not linked) from the test library to `m/Foo v01 part1.cbz`, `m/Foo v01 part3.cbz` and `k/Foo v01 part2.cbz`.
     - As `local`, add one library holding `m` and `k`, the "Don't scan yet" box unticked, and wait for both scans.
     - Assert the fixture first: one work, `Foo v01`, whose first edition holds part1, part2 and part3 in that order.
     - If the derive does not merge them, stop and report: the drive's claims need that layout.

   **Check, with chrome-devtools at 1280 px** (390 px for the dialog's paragraph only):
   - Tab A, the reader on `Foo v01`:
     - On part2's first page, press `n` and write `on part2`.
     - On part3's second page, write `before`.
   - Tab B: Remove the directory `k` (the `RemoveRoot` dialog).
     - The dialog draws `On works that stay, this takes the page of 1 note.` Screenshot it.
     - Confirm. Wait more than `registry::REFRESH_EVERY` (two seconds), so the server has re-read the work.
   - Tab A, not reloaded:
     - On the page it shows, which is part3's second page in its own layout, write `after`. Expect the merged notice ("These notes changed on another device; yours were kept on top."): that is the 412 retry, which re-sends `on part2` (now stale) and `before` untouched. If the first `PUT` lands without a 412, record that and why; the reload check below is the claim either way.
     - Turn one page forward.
     - Read in the network panel the notes `PUT`s (412, then 200) and the progress `PUT` carrying `at`.
   - Reload tab A:
     - `before` and `after` are both on part3's second page.
     - The reader resumes on part3's third page.
     - In the review list, `on part2`'s row ends `· no longer on its page`. Opening it draws the stale paragraph.
   - Export from the panel. The downloaded file has no `"at"` and no `"stale"`.

   **Tear-down:**
   - Stop the server by PID and check the port again.
   - `find /home/davey/temp/manga-test -newer <marker>` must print nothing, and the `sha256sum` must equal the first.
   - Then delete the scratch directory.
4. **Claim check** of the closes, the spec-gap wording and the PR body. Put the PR body in pass 1, and scope later passes to the correction commit plus the PR body.
5. **The PR body** at `$E/pr-body.md`, never hard-wrapped. The title is the first line, under 255 characters: measure it.
   - It names the two rows closed, the rulings and the settled calls.
   - It states spec gaps 2–8 and what was done about each. That includes:
     - the owner's "Server keeps echoes" ruling: the reload words;
     - the owner's "Stamp remembers fingerprints" ruling: migration 22's `stamp_alias`, what a chain, a removal, a fold and an export do with it, the O270 guard, and that no alias is sent;
     - the refusals left by both, which still stick until a reload.
   - It names migration 22 and the `rust:1-bookworm` run.
   - It states the one 412 per tab at deploy, for a work that holds a note (settled call 15).
   - It says O271 is neither closed nor narrowed.
   - It names the files shared with PR A and how the rebase went.

   Ask the user before pushing.
