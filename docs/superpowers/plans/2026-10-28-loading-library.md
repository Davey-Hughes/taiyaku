# The loading work, second PR: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Back to the Library view, a series page or a search draws what was left at once, with covers from the browser's cache, at the scroll, chip, sort and edition left behind; the reader's exits return to the last Library view the same way.

**Architecture:** The server builds every cover's address with a version naming its bytes, and the thumbnail route marks a matching version `immutable`. The three views read through the first PR's store (`reads.ts`, `usePage`), which gains a bound, a prefix `invalidate` and reads that wait for the last progress save. Chip, sort and edition move into the URL, and a small scroll keeper saves the scroller's position per history entry and restores it once the body is drawn.

**Tech Stack:** Rust (axum, rusqlite), React + TypeScript, vitest (`node`, `jsdom`, `browser` projects), Playwright Chromium.

**Spec:** `docs/superpowers/specs/2026-10-28-loading-library-design.md`. Read it whole before your task (it is short); it is the record of the user's rulings, and this plan does not repeat its reasons. The first PR's spec, `2026-10-27-loading-reads-design.md` §3–§4, says what the store and `usePage` already do.

## Global Constraints

- No new dependency in `web/package.json` or any `Cargo.toml`.
- Web tests live in `web/tests/`, mirroring `web/src` (helpers in `web/tests/support/`); a test under `web/src` fails `tests/srcHoldsNoTests.test.ts`.
- Comments say why only: no history, no measurements, no "this used to" (`CLAUDE.md`). A doc comment describing a view's own fetch, `cancelled` flag or sequence counter is rewritten to say what is now true.
- The wait line is `PageLoading` (`web/src/settingsLayout.tsx` ~:130) fed by `usePage`'s `loadingDrawn`; its text is exactly `Loading…`. A failure is drawn as each view draws it today: same element, role and sentence.
- Every `Read` in `pageReads.ts` wraps its client call in an arrow, so a test's `vi.mock('../src/library')` is the function called.
- Store and keeper state is module-level: `web/tests/support/browser-setup.ts` empties both after every `jsdom` and `browser` test, and `Gate.tsx` empties both on sign-in and sign-out.
- SQL placeholders are anonymous and bind in text order (`browse.rs` header ~:39-64); a fragment shared by two queries binds in the order each query places it.
- Tests: assert per element, never a whole-page `toContain`. A test written for a defect is watched failing on `main`'s code; one written for a rule is watched failing under a hand-applied mutation. Record each red in the task report.
- Vitest with `--reporter=default`. One file: `pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project jsdom tests/Library.test.tsx --reporter=default`. At the end of each web task run the `node`, `jsdom` and `browser` projects WHOLE, plus `typecheck` and `lint`, each into a log under the scratchpad, reading the summary lines (never pipe a gate through `tail`).
- Rust: `NEXTEST_TEST_THREADS=8` / `--test-threads=8`. At the end of each Rust task: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `env -u TAIYAKU_CORPUS cargo test --workspace --all-features -- --test-threads=8`, each into a log. A task that adds SQL also runs the affected crates' tests in `rust:1-bookworm` (SQLite 3.40.1; recipe in the memory "SQLite 3.40.1 in CI and the image").
- Shell: the Bash tool runs zsh with `noclobber` (`>|` to start a log), `grep` skips files it calls binary (`grep -a`), `cd` is zoxide (absolute paths, `git -C`). A worktree gets a COPIED `web/node_modules`, never a symlink, and its own `CARGO_TARGET_DIR`.
- A browser project failure with import or connection errors and no failed assertion is the headless renderer crashing: check `coredumpctl list` and rerun the whole project before suspecting the branch.

## Interfaces

Named here because this plan is the first place they exist. Implementations are the implementer's.

```rust
// crates/taiyaku-serve/src/cover.rs, beside `page_key`
/// What a cover's `ETag` is made of but its long edge: so it changes exactly
/// when the `ETag` of any cover size would. Hex and `-` only.
pub(crate) fn cover_version(sha256: &str, offset: u32) -> String

// crates/taiyaku-serve/src/routes.rs, beside `PAGE_CACHE_CONTROL`
pub(crate) const COVER_CACHE_CONTROL: &str = "private, max-age=31536000, immutable";

// crates/taiyaku-serve/src/library_api.rs
/// `/api/works/{work}/{edition}/pages/0/thumb?v=…`, its segments encoded as
/// the web client's `workCover` encoded them.
fn cover_address(work: &str, edition: &str, sha256: &str) -> String
```

```ts
// web/src/library.ts
SeriesCard.cover: string | null   // the server's address; null: no visible cover
EditionCard.cover: string | null  // its page 0's address; null: no page

// web/src/reads.ts
/** Entries kept that nothing draws, past which the least recently drawn goes. */
export const IDLE_KEPT = 50
/** `invalidate` for every kept address equal to a prefix or continuing it with `?` or `/`. */
export function invalidatePrefix(...prefixes: readonly string[]): void

// web/src/api.ts
/** Settles once every progress save sent before the call has settled; never rejects. */
export function progressSaved(): Promise<void>

// web/src/pageReads.ts
export const gridRead: (library: number | undefined) => Read<Grid>
export const homeRead: (library: number | undefined) => Read<HomeLists>
export const seriesRead: (id: number) => Read<SeriesDetail>
export const searchRead: (q: string, library: number | undefined) => Read<Awaited<ReturnType<typeof search>>>
export const LIBRARY_VIEWS: readonly string[] // '/api/library/series', '/api/library/home', '/api/library/search'
export function invalidateLibraryViews(): void // invalidatePrefix(...LIBRARY_VIEWS)

// web/src/scrollKeeper.ts
/** The navigation-state key `onLeave` sets; merged beside the search box's tag, never replacing it. */
export const RESTORE_SCROLL = 'restoreScroll'
/** Restores the kept scroll once `ready` on a Back/Forward arrival or a RESTORE_SCROLL one; saves it while drawn. `library` marks the Library view, whose address and scroll `lastLibraryAddress` holds. */
export function useKeptScroll(ready: boolean, options?: { library?: boolean }): void
export function lastLibraryAddress(): string | undefined
export function forgetScroll(): void
```

If the library parameter's type in `library.ts`'s `grid`/`home`/`search` is not `number | undefined`, the factories take that type instead; say so in the report.

## Order

Task 1 → Task 2 (Rust) can run beside Task 4 (web store): no file overlaps. Task 3 needs Task 2's answer shape. Tasks 5, 6, 7 and 8 need Tasks 3 and 4; Task 6 follows Task 5 (both edit `Library.tsx`). Task 9 needs Tasks 5 and 7. Task 10 follows the live and device checks.

---

### Task 1: The thumbnail route honours `?v=`

**Files:**
- Modify: `crates/taiyaku-serve/src/cover.rs` (`cover_version` beside `page_key` ~:177; `PAGE_FORMAT_VERSION` ~:191 stays private).
- Modify: `crates/taiyaku-serve/src/routes.rs`: `COVER_CACHE_CONTROL` beside `PAGE_CACHE_CONTROL` (~:220); `serve_page` (~:913) takes `version: Option<&str>` and, after `locate_member`, uses `COVER_CACHE_CONTROL` when `version == Some(&cover_version(sha256, offset))`, else `PAGE_CACHE_CONTROL`, on the `304` (~:938) and the `200` (~:983) alike; `get_thumb` (~:1064) passes `first_query_value(&query, "v")` only in its `cover_edge` branch (~:1078-1080); `get_page` (~:902) passes `None`.
- Test: `crates/taiyaku-serve/src/routes/tests.rs`, beside `a_sized_cover_answers_its_own_validator_with_a_304` (~:4965), with `state_with_real_pages` (`test_support.rs` ~:474) and `fetch` (~:1891).

**Interfaces:** Produces `cover_version`, `COVER_CACHE_CONTROL`.

- [ ] **Step 1: Tests first:** a cover size with the matching `v` answers `200` with `COVER_CACHE_CONTROL` and its `ETag`, and a matching `If-None-Match` with it answers `304` with `COVER_CACHE_CONTROL` (RFC 9111 §4.3.4, O1's lesson); a mismatched `v` answers `200` with `PAGE_CACHE_CONTROL` and the current bytes; no `v` answers as today; a `v` on an unsized thumb, on a size outside `COVER_LONG_EDGES`, on page 1 and on `get_page` is ignored (`PAGE_CACHE_CONTROL`, or that route's own headers); a download-only account (`without_can_stream` ~:4755) gets its covers at every cover size with a matching `v`, immutable, and still no pages (O133); `cover_version` changes with the sha, the offset and the format version, and never with the edge (a unit test in `cover.rs`'s tests, beside `page_key`'s).
- [ ] **Step 2: Red.** Record.
- [ ] **Step 3:** Implement. `every_route_serving_a_member_s_bytes_makes_the_client_revalidate` (~:2203) stays true for every address without `v`; say so in its doc if it claims more.
- [ ] **Step 4: Watched mutations:** compare `v` ignoring the offset; `COVER_CACHE_CONTROL` on the `200` only (the `304` keeps `PAGE_CACHE_CONTROL`); honour `v` outside `cover_edge`; `immutable` on a mismatch. Record the case each turned red.
- [ ] **Step 5:** fmt, clippy, workspace tests. Commit.

### Task 2: The answers carry each cover's address

**Files:**
- Modify: `crates/taiyaku-library/src/browse.rs`:
  - One SQL fragment, from one `fn`, choosing a series' cover edition: among editions with a member the caller may see (`member_clause`), ordered `(w.id IS s.cover_work_id) DESC, w.volume, w.id, e.key`, first only. `cover_target` (~:1371-1407, which `cover_work` ~:1311-1341 calls) and `series()` (~:459-532, SQL ~:489-508) both use it; in `series()` as a CTE with `ROW_NUMBER() OVER (PARTITION BY w.series_id …)` or a correlated subquery, binding its access group where the text places it. `cover_work`'s two steps (hand-set if visible, else the order) give the same answer as the one ordering; keep the function's contract and its `.optional()` doc.
  - Page 0's member hash: `(SELECT m.sha256 FROM member m WHERE m.edition_id = e.id AND m.pages > 0 ORDER BY m.ordinal LIMIT 1)`, with no grant filter (spec §4.2). `SeriesCard` (~:127-159) gains the cover's work slug, edition key and that hash (`CoverTarget` ~:1350-1356 holds the first two; extend it or add a field), or none; `EditionCard` (~:163-171) gains its page 0's hash, or none, from `editions_by_work` (~:562-620).
- Modify: `crates/taiyaku-serve/src/library_api.rs`: `cover_address` (Interfaces); `series_json` (~:90-101) sends `"cover"` as `cover_address(...)` or `null` in place of the `307` address (~:99); `work_json`'s editions (~:79-85) each gain `"cover"`. The `307` route itself (`cover` ~:334-374) is unchanged. Read `web/src/library.ts`'s `workCover` (~:182) for how the client encoded the segments.
- Test: `crates/taiyaku-library/src/browse/tests.rs` (fixtures `two_root_library` ~:13, `two_volume_library` ~:199, `merged_edition_library` ~:751, `library_with_volumes` ~:2080); `crates/taiyaku-serve/src/library_api/tests.rs` (the assertion on the answer's `cover` = the `307` address, ~:271, changes to the new address, while the tests of the `307` route itself, ~:53, :185, :224, :318, :384, :1325, stay; list each in the report); `crates/taiyaku-serve/src/routes/tests.rs` or `library_api/tests.rs` with `router_over_two_scanned_volumes_of_one_series` (`test_support.rs` ~:1638), whose files and hashes are real.

**Interfaces:** Consumes `cover_version` (Task 1). Produces the answer shape Task 3 consumes: a series row's `cover` is an address or `null`; each edition card's `cover` is an address or `null`.

- [ ] **Step 1: Tests first:** over fixtures with a hand-set cover, a hand-set cover hidden by a grant, a lower volume added after, and several editions of one work, the grid's chosen work and edition equal what `cover_work` gives the `307` for the same caller (assert both, per series); a series whose works the caller cannot see has a `null` cover; an edition whose first member has `pages = 0` takes its hash from the next member; over the scanned fixture, the address in a grid row, in a series page's own row, in a search hit and in an edition card, fetched with `w=256`, answers `200` with `COVER_CACHE_CONTROL` (the end-to-end agreement); a merged edition's page 0 under a root the caller is not granted still gives the address the route serves (spec §4.2).
- [ ] **Step 2: Red.** Record.
- [ ] **Step 3:** Implement.
- [ ] **Step 4: Watched mutations** (cargo-mutants makes no mutant for a string literal, a `json!` key or SQL text, so these are by hand): drop the `(w.id IS s.cover_work_id) DESC` term; order by `m.ordinal` without `m.pages > 0`; apply `member_clause` to the page-0 member; send the `307` address again for one of the four answers. Record the case each turned red.
- [ ] **Step 5:** fmt, clippy, workspace tests; the two crates' tests in `rust:1-bookworm`. Commit.

### Task 3: The web client draws the answers' cover addresses

**Files:**
- Modify: `web/src/library.ts` (`SeriesCard.cover` ~:73 and `EditionCard` ~:17-28 per Interfaces; `workCover` ~:182-184 goes; `coverSources` ~:202-210 keeps adding `w` after an existing `?`).
- Modify: `web/src/SeriesGrid.tsx` (~:32: a `null` cover draws the `Cover` component's missing box with no request; read `web/src/Cover.tsx` for how it draws one), `web/src/Series.tsx` (`VolumeCover` ~:326 draws the shown edition's `cover`), `web/src/Library.tsx` (`ContinueCard` ~:78 draws `w.opens`'s edition's `cover`).
- Test: `web/tests/library.test.ts` (or wherever `coverSources` is tested), `web/tests/Series.test.tsx`, `web/tests/Library.test.tsx`; their fixtures' `cover` values become the new shape.

**Interfaces:** Consumes Task 2's shape.

- [ ] **Step 1: Tests first:** `coverSources` on an address with `?v=` gives `…?v=…&w=256` and the 2x/3x `srcSet`; a series tile with `cover: null` draws the missing box and its `img` is never in the DOM; a volume tile draws the chosen edition's address and changes with the edition; a Continue card draws its `opens` edition's address.
- [ ] **Step 2: Red.** Record.
- [ ] **Step 3:** Implement; grep `web/src` for `workCover` and `/cover` to leave no caller.
- [ ] **Step 4: Watched mutations:** a volume tile drawing `editions[0]`'s address whatever is chosen; `coverSources` adding `?w=` after an address that has `?`. Record.
- [ ] **Step 5:** Three projects whole, typecheck, lint. Commit.

### Task 4: The store's additions

**Files:**
- Modify: `web/src/reads.ts`:
  - A slot keeps the last `Read` that started it (in `start` ~:132-147), so a prefix can re-read a drawn address it knows only by key.
  - `IDLE_KEPT` and the bound: a slot moves to the end of `slots` (delete, then set) when its last listener goes (~:239-244), and after that and after each apply (`land` ~:214-218, `readNow` ~:320) the oldest slots that are neither drawn (`drawn()` ~:117-119) nor holding a request not yet applied are dropped until at most `IDLE_KEPT` undrawn slots remain. An answer for a dropped slot is already ignored (~:157-158).
  - `invalidatePrefix`, following `invalidate` (~:263-281) over the matching keys of `slots`.
- Modify: `web/src/api.ts`: `putProgress` (~:490-501) records its request while in flight; `progressSaved` per Interfaces.
- Modify: `web/src/pageReads.ts`: the four factories (as `libraryScan` ~:35-36 is written), each `async () => { await progressSaved(); return grid(library) }` and so on; `LIBRARY_VIEWS`; `invalidateLibraryViews`. `/api/library/series` covers the grid and a series page; no store address is a cover's.
- Test: `web/tests/reads.test.ts`, `web/tests/api.test.ts` (or the file testing `putProgress`), a `pageReads` test; helper `web/tests/support/reads.ts` (`address()` ~:10-18).

**Interfaces:** Produces `IDLE_KEPT`, `invalidatePrefix`, `progressSaved`, the factories, `LIBRARY_VIEWS`, `invalidateLibraryViews`.

- [ ] **Step 1: Tests first:** with `IDLE_KEPT + 1` addresses read and none drawn, the least recently drawn is dropped and a later read of it sends a request; a drawn address is never dropped however many idle ones there are, and is not counted; an address whose request is out is not dropped before it lands; `invalidatePrefix('/api/library/series')` re-reads `/api/library/series?library=2` and `/api/library/series/17` when drawn, marks them stale when not, and leaves `/api/library/seriesX` and `/api/library/home` alone; `progressSaved` settles only after a save sent before it settles, also when the save fails, and at once with none in flight; a factory's client call starts only after `progressSaved` settles.
- [ ] **Step 2: Red.** Record.
- [ ] **Step 3:** Implement.
- [ ] **Step 4: Watched mutations:** drop by insertion order without the move on unsubscribe; count drawn slots against the bound; match a prefix with bare `startsWith`; `progressSaved` resolving before a failed save settles; a factory calling the client before awaiting. Record.
- [ ] **Step 5:** Three projects whole, typecheck, lint. Commit.

### Task 5: The Library view on the store, with `show` and `sort` in the URL

**Files:**
- Modify: `web/src/Library.tsx`: the fetch effect (~:208-223), `loaded` (~:132) and the `current` filter (~:141) give way to `usePage([gridRead(library), homeRead(library)])`; `PageLoading` in the grid's place; the retry after a failure becomes `invalidate` of the two reads (the view has no retry control at the base, so this has nothing to change: spec §6.1); `chip` and `sort` (~:133-134, controls ~:289-308) read from and write to `?show=` and `?sort=` with `replace`, keeping every other parameter and the navigation state the search box set (`SearchBar.tsx` ~:68-81 shows how it writes; O223); an unknown value reads as the default.
- Modify: `web/src/Sidebar.tsx` (~:52-57): drawn beside the Library view, each library's link carries the current `show` and `sort`; the heading's `/` link (~:40-47) carries none.
- Test: `web/tests/Library.test.tsx` (:619 and :625 keep passing unchanged; :426, :445 and :528-640 change only where the store makes the request count or timing different: list each changed assertion and why); a `Sidebar` test.

**Interfaces:** Consumes Tasks 3–4.

- [ ] **Step 1: Tests first:** a first visit draws the body in one render after both reads and `Loading…` at 200 ms, not at 199; a remount with both kept draws the grid and the Continue strip in its first render, with the same cover `src` per tile, and sends one re-read of each; a switch to a library kept draws it at once, and to one not kept draws nothing of the old library; `?show=unread&sort=added` draws that chip and sort on arrival, a press writes them with `replace` (history length unchanged) and keeps `?q=`, `?library=` and the box's navigation state, and `?show=bogus` reads as All; a library link drawn beside `/?show=multi` goes to `/?library=N&show=multi`.
- [ ] **Step 2: Red.** Record.
- [ ] **Step 3:** Implement.
- [ ] **Step 4: Watched mutations:** the chip written with `push`; the write dropping `?q=`; the library link without `show`; the grid drawn before `home` lands. Record.
- [ ] **Step 5:** Three projects whole, typecheck, lint. Commit.

### Task 6: Search on the store

**Files:**
- Modify: `web/src/searchHits.ts` (`useSearchHits`, debounce ~:45-61, sequence ~:48-56, empty query ~:77-89): reads `searchRead(q, library)` through the store. On arrival (the hook's first `q`, or a `?q=` changed by Back or a link, which `SearchBar.tsx` ~:32-59 tells apart by its `searchBox` tag in the navigation state) it reads at once; a typed change waits 250 ms. While the current address is not ready it keeps returning the hits of the last address that was, so typing never blanks the hits; a page's arrival with none kept is a first visit (`Loading…` after 200 ms). Its doc says this is where a view bends `usePage`'s first-visit rule.
- Modify: `web/src/Library.tsx` only where it passes `offered ? q : ''` (~:180): an arrival's read stays behind `offered` (O221).
- Test: `web/tests/searchHits.test.tsx` (:88, :122, :164, :191 keep their meaning), `web/tests/Library.test.tsx` (:1083, :1113, :1140, :1188, :1382, :1409).

**Interfaces:** Consumes Task 4's `searchRead`.

- [ ] **Step 1: Tests first:** Back to `/?q=foo` with its hits kept draws them in the first render and re-reads once; an arrival with none kept sends the query at 0 ms, not 250; typing `fo` then `foo` keeps `fo`'s hits drawn until `foo`'s land, then draws `foo`'s in one render; no query is sent while the box is not offered, on arrival included; a cleared box draws no hits and sends nothing.
- [ ] **Step 2: Red.** Record.
- [ ] **Step 3:** Implement.
- [ ] **Step 4: Watched mutations:** an arrival read sent through the debounce; the held hits dropped when the address changes; the arrival read sent before `offered`. Record.
- [ ] **Step 5:** Three projects whole, typecheck, lint. Commit.

### Task 7: A series page on the store, with `edition` in the URL

**Files:**
- Modify: `web/src/Series.tsx`: the fetch effect (~:471-498) and `loaded`/`failure` (~:391-393) give way to `usePage([seriesRead(id)])` (header at once, as today; `PageLoading` in the body's place); `chosen` (~:398, reset ~:479, `<select>` ~:513-529) reads from and writes to `?edition=` with `replace` (an edition this series does not have, or none, reads as all); the title edit (`onPatch={patchWork}` ~:683) becomes a wrapper whose success calls `invalidateLibraryViews()`, so the drawn page reads again at once. `open` and `detail` (~:400-469) stay as they are (spec §9).
- Test: `web/tests/Series.test.tsx` (:141, :215, :879, :1296, :1306 keep their meaning; list each changed assertion and why).

**Interfaces:** Consumes Tasks 3–4.

- [ ] **Step 1: Tests first:** a remount with the series kept draws its volumes in the first render and re-reads once; a move to another series not kept draws none of the old one, and `Loading…` at 200 ms; `?edition=en` selects it on arrival, a change writes it with `replace`, and `?edition=en` on a series without `en` reads as all; a title saved reads the series again once and marks the grid stale (a later `gridRead` sends a request).
- [ ] **Step 2: Red.** Record.
- [ ] **Step 3:** Implement.
- [ ] **Step 4: Watched mutations:** the edition kept when the series lacks it; the title wrapper invalidating before the save answers; the old series drawn while the next loads. Record.
- [ ] **Step 5:** Three projects whole, typecheck, lint. Commit.

### Task 8: The writes that change the views mark them

**Files** (each success path calls `invalidateLibraryViews()`; read each site before placing it):
- `web/src/LibraryPage.tsx`: the `ended` effect (~:472-474), so a scan this page saw end marks the views; a scan ended from a kept status deliberately reads nothing (first spec §9 call 2) and marks nothing.
- `web/src/RootRow.tsx`: the poll's "over" branch (~:647-661).
- `web/src/RuleEditor.tsx`: the save's success (~:544, ~:547). The import's success (~:689) derives nothing (~:690-699): leave it, and say so in the report.
- `web/src/AcceptForm.tsx`: the accept's success (~:264-267).
- `web/src/Libraries.tsx`: `made` (~:284-310), `libraryRemoved` (~:312-323), `dirRemoved` (~:331-359), `dirMoved` (~:366-405), `libraryRenamed` (~:418-447). Not inside `reread` (~:179-182), which also runs on every change of page.
- `web/src/AccountPage.tsx`: `save`'s success (~:485-493) when the account is the caller's own.
- Then grep `web/src` for every client write (`sendJson`, and each `post*`, `patch*`, `put*`, `delete*` function in the client modules) and decide for each whether it changes what the grid, home, a series page or search draw; list every write and the decision in the report, with the reason for each "no".
- Test: each screen's existing test file under `web/tests/`, one case per site: after the write's answer, a kept grid address is stale (its next read sends a request); and one case that a change of page in Libraries marks nothing.

**Interfaces:** Consumes Task 4's `invalidateLibraryViews`.

- [ ] **Step 1: Tests first**, one per site above.
- [ ] **Step 2: Red.** Record.
- [ ] **Step 3:** Implement.
- [ ] **Step 4: Watched mutations:** delete each call in turn and record the case that went red; put the call inside `reread` and watch the change-of-page case go red.
- [ ] **Step 5:** Three projects whole, typecheck, lint. Commit.

### Task 9: The scroll keeper and the reader's exits

**Files:**
- Create: `web/src/scrollKeeper.ts` (Interfaces). The scroller is `document.getElementById('content')` (`Shell.tsx` ~:368's `<main>`) when its computed `overflow-y` is `auto` or `scroll`, else `document.scrollingElement`. Positions are kept by `location.key`, saved on scroll (once per animation frame at most) and on unmount; restored once per arrival, in a layout effect, the first render `ready` is true on a `POP` arrival (`useNavigationType`) or a `PUSH` whose state holds `RESTORE_SCROLL`, clamped. A `REPLACE` (a chip, sort, edition or `?q=` write, each of which gives the location a new key) never restores, and saving goes on under the new key. The Library view's hook updates the held address on every location it draws, so a chip changed after arriving is what the reader's exits return to. While a hook is mounted `history.scrollRestoration` is `manual`, and on unmount it is put back to the value found at mount.
- Modify: `web/src/Library.tsx` and `web/src/Series.tsx`: `useKeptScroll(ready, { library: true })` and `useKeptScroll(ready)`.
- Modify: `web/src/Routes.tsx` (~:45): `onLeave` navigates to `lastLibraryAddress() ?? '/'` with `RESTORE_SCROLL` in the state. This changes all three exits: `TopBar.tsx` ~:90, the key at `App.tsx` ~:894, and `ReaderFailed.tsx` ~:108.
- Modify: `web/src/Licence.tsx` (~:231-236): put back the value found at mount, not `auto`.
- Modify: `web/src/Gate.tsx` (~:73, ~:89) and `web/tests/support/browser-setup.ts` (~:37-39): `forgetScroll()` beside `emptyReads()`.
- Test: `web/tests/scrollKeeper.test.tsx` (jsdom: saving per key, the clamp, restoring once, `POP` versus `PUSH` versus `RESTORE_SCROLL`, `lastLibraryAddress`, the value put back); `web/tests/scrollKeeper.browser.test.tsx` (real scrolling with `BrowserRouter`, as `Shell.browser.test.tsx`'s `licenceInShell` ~:1462-1500, `back()` ~:1526 and `scrolled()` ~:1562-1565 do, `it.each([1280, 390])`).

**Interfaces:** Consumes Tasks 5 and 7. Produces the keeper.

- [ ] **Step 0: Measure `main` first.** In the browser project, at 390 px and 1280 px, record where Back lands on `/config/libraries` and `/account` after scrolling them on `main`'s code (a throwaway test run in a worktree of `main`, not committed). The browser test for "every other page keeps its Back behaviour" pins exactly those results.
- [ ] **Step 1: Tests first** (browser, both widths): scroll the Library view, open a series, Back: the scroller's offset is restored; the same for a series page and the reader's Library button; open the Library view by the sidebar's heading, or switch library by its link: offset 0; after a restore, pressing a chip does not move the offset back, and the reader's exits then return to the address with that chip; a body that draws only after a wait (entry stale) restores when it draws; a re-read landing after the restore leaves the offset alone; Licence, then the Library view, leaves `history.scrollRestoration` `manual` while drawn; the config pages' Back lands as Step 0 recorded; at 1280 px it is `<main>` that moves and at 390 px the document.
- [ ] **Step 2: Red.** Record.
- [ ] **Step 3:** Implement.
- [ ] **Step 4: Watched mutations:** restore on `PUSH`; restore on every render `ready` is true (not once); hard-code the document as the scroller; `Licence.tsx` back to `auto`; `onLeave` without `RESTORE_SCROLL`. Record.
- [ ] **Step 5:** Three projects whole, typecheck, lint. Commit.

### Task 10: The docs

Runs after the live check and the device check (below), whose results it records.

**Files:**
- Modify: `docs/status.md`: the Later entry "The loading work, in two PRs" (~:4880-4916) says what the second PR shipped, naming reading progress as the second exception to "each write marks stale what it changes", and its sentence on the web tests' move gains "(#211)"; O224 (~:1801) closes with the five-part edit (memory "Open items ritual"), its evidence the device check's log lines.
- Modify: `docs/notes.md`: a section beside O133's (~:21582) on the cover version (spec §8), saying how it answers O1's reason for declining `immutable` (~:6462-6492, status.md ~:2058-2097); the deploy notes' `Cache-Control` list (~:24452-24459) gains `COVER_CACHE_CONTROL`.
- Modify: the spec only to add a §10 "Settled while building, and known limits", if the build settled any.

- [ ] **Step 1:** Write. Every figure is a claim the claim check verifies: write few.
- [ ] **Step 2:** The doc-link check (`scripts/check-all.sh`'s docs job) and the whole gate. Commit.

## After the tasks (the controller)

- Whole-branch review, then a review of its fixes; `scripts/mutants.sh --since main` for the Rust (commit fixes before re-sweeping).
- **Live check:** a server built from the branch over a scratch copy of the test library (memory "UI work: prototype on a live server", 127.0.0.1:4399); in Chromium with its network log: Back to the grid sends no cover request and no `307`; the reader's Library button returns to the scroll; a scan's end, then the Library view, waits for its read.
- **Device check (O224's evidence):** the user's iPhone over the tailnet; read the user agent (`Version/`) first; Back from a series page and from the reader to the grid; the server's log shows no `/cover` and no `/thumb` request for that Back. Record the log lines, not a summary.
- Claim check of the docs and the PR body; full gate; push and PR asked separately.
