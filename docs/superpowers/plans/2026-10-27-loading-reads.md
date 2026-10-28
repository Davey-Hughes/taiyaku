# The loading work, first PR: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every Configuration page, `/account` and About draws its header at once and its body in one step, with one "Loading…" after 200 ms; a revisit draws the last answers at once and updates quietly in one step.

**Architecture:** A module-level store (`web/src/reads.ts`) keeps one entry per GET address, calls the address's existing client function, and applies a set of reads with one notification. A hook (`web/src/usePage.ts`) subscribes a page to its reads through `useSyncExternalStore` and says when its body may be drawn. The `Read` for each address is named once in `web/src/pageReads.ts`; screens move onto it one cluster per task.

**Tech Stack:** React + TypeScript, vitest (`node`, `jsdom` and `browser` projects), Playwright Chromium for the browser project.

**Spec:** `docs/superpowers/specs/2026-10-27-loading-reads-design.md`. Read it whole before Task 1 (it is short), and §5 again before each screen task. It is the record of the user's rulings; this plan does not repeat its reasons.

## Global Constraints

- No new dependency in `web/package.json`.
- The wait line's text is exactly `Loading…` (U+2026), drawn as `<p role="status">` in the body's place, only once `useDrawnWait` (hooks.ts, `WAIT_DRAWN_AFTER_MS` = 200) says the wait has held. No minimum time on screen.
- A failure is drawn as each screen draws it today: the same element, role and sentence. Do not reword any failure.
- The store calls the client function (`listLibraries()` and so on); it never calls `fetch` or `request` itself. Every `Read` in `pageReads.ts` wraps its client call in an arrow (`() => listLibraries()`), so a test's `vi.mock` of the client module is the function called.
- Store state is module-level, so `web/src/browser-setup.ts` (the `jsdom` and `browser` projects' setup file) empties it after every test.
- Comments say why only: no history, no measurements, no "this used to" (`CLAUDE.md`). A doc comment that describes a screen's first read or its sequence counter is rewritten to say what is now true.
- Tests: count updates and read text per element, never with a whole-page `toContain`. A test written for a defect or a rule is watched failing: on `main`'s code for a defect, under a hand-applied mutation for a rule. Record each red in the task report.
- Run vitest with `--reporter=default` (the agent reporter hides output). One file: `pnpm -C /home/davey/projects/taiyaku/web exec vitest run --project jsdom src/Backup.test.tsx --reporter=default`; a browser file: `--project browser src/X.browser.test.tsx`. At the end of each task run the `node`, `jsdom` and `browser` projects WHOLE, plus `pnpm -C /home/davey/projects/taiyaku/web typecheck` and `lint`, each into a log file under the scratchpad, reading the summary lines (never pipe a gate through `tail`).
- Shell: the Bash tool runs zsh with `noclobber` (`>|` to start a log), `grep` skips files it calls binary (`grep -a`), `cd` is zoxide (use absolute paths and `git -C`). A worktree gets a COPIED `web/node_modules`, never a symlink.
- A browser project failure with import or connection errors and no failed assertion is the headless renderer crashing: check `coredumpctl list` and rerun the whole project before suspecting the branch.

## Interfaces

Named here because this plan is the first place they exist. Implementations are the implementer's.

```ts
// web/src/reads.ts
/** One GET the store keeps: its address, and the client call that reads it. */
export interface Read<T> {
  readonly address: string
  readonly fetch: () => Promise<T>
}
export function read<T>(address: string, fetch: () => Promise<T>): Read<T>

/** What the store holds for one address. The same object until it changes. */
export interface Entry<T> {
  /** The last good answer; `undefined` until one lands. */
  readonly answer: T | undefined
  /** The latest attempt's failure, cleared by the next good answer. */
  readonly failure: unknown
  /** Marked by `invalidate`, cleared when a re-read settles. */
  readonly stale: boolean
}

export function entryOf<T>(r: Read<T>): Entry<T> | undefined
/** Per address; the count of listeners is what "drawn" means to `invalidate`. */
export function subscribe(address: string, listener: () => void): () => void
/** Reads `reads` as one set (§3.8): joins a request already out for an address, starts one otherwise; resolves once the set is applied. */
export function readSet(reads: readonly Read<unknown>[]): Promise<void>
/** Marks each stale and reads the drawn ones now as one set, always with new requests (§3.2, §3.6). Resolves once applied. */
export function invalidate(...reads: readonly Read<unknown>[]): Promise<void>
/** A poll's read: a new request applied alone; resolves with its answer, rejects with its failure. */
export function readNow<T>(r: Read<T>): Promise<T>
/** Signing in or out, and every test's end: drops every entry, and every answer still in flight. */
export function emptyReads(): void
```

```ts
// web/src/usePage.ts
export interface Page<T extends readonly unknown[]> {
  /** The body may be drawn: on arrival every read answered and none stale, or every read has settled since. Once true it stays true for this mount while the addresses are unchanged (§4.3). */
  readonly ready: boolean
  readonly answers: { readonly [K in keyof T]: T[K] | undefined }
  readonly failures: { readonly [K in keyof T]: unknown }
  /** `useDrawnWait(!ready)`. */
  readonly loadingDrawn: boolean
}
/** An `undefined` entry is an address not known yet (§4.5): the page is not ready. An empty list is ready. */
export function usePage<T extends readonly unknown[]>(
  reads: { readonly [K in keyof T]: Read<T[K]> | undefined },
): Page<T>
```

```tsx
// web/src/settingsLayout.tsx, beside PageHeader
/** The body's place while a page waits; draws nothing until `drawn`. */
export function PageLoading({ drawn }: { drawn: boolean }): JSX.Element | null
```

`web/src/pageReads.ts` holds every `Read`. Each task adds the ones it needs, exactly these names: `LIBRARIES`, `LIBRARY_STATUS`, `libraryScan(id)`, `USERS`, `BACKUPS`, `RULES`, `ROOTS`, `ACCEPTED_PROPOSALS`, `KEYS`, `SESSIONS`, `ABOUT`. Each address string is the one its client function sends, query included (check `curation.ts`'s `queryString` for `ACCEPTED_PROPOSALS`).

---

### Task 1: The store

**Files:**
- Create: `web/src/reads.ts`; `web/src/pageReads.ts` (empty of reads but its file doc, saying the arrow rule and why).
- Test: `web/src/reads.test.ts` (`node` project), with a fake client call per case whose promise the test resolves by hand.
- Modify: `web/src/browser-setup.ts` (an `afterEach` that calls `emptyReads`); `web/src/Gate.tsx` (`signOut` ~:81-83 and the sign-in's `setWho(account)` ~:69 each call `emptyReads()` first; read `ask` ~:64-79 to place it so a 401's sign-out and a fresh sign-in both empty it).

**Interfaces:** Produces everything in `reads.ts` above.

- [ ] **Step 1: Tests first**, one case per item of spec §3 that has behaviour: two `readSet` calls while one request is out send one request (item 2); `invalidate` while a request is out starts a second (item 2); an older request answering after a newer one is dropped, and one answering before it is replaced (item 3); the same `JSON.stringify` text again keeps the entry and the answer the same objects and calls no listener (item 4); a failure keeps the answer and sets `failure`, and the next good answer clears it (item 5); `invalidate` re-reads an address with a listener and only marks one without, and a later `readSet` of the marked one reads it (item 6); a set of three, answered in the order 2, 3, 1, notifies each listener once, after the third (item 8); `emptyReads` drops entries, and an answer in flight across it is not kept (item 9); `readNow` resolves with its answer, rejects with its failure, and is newest-wins with the rest.
- [ ] **Step 2: Red.** Run the file: every case fails (the module does not exist yet). Record.
- [ ] **Step 3:** Implement `reads.ts` and wire `emptyReads` into `Gate.tsx` and `browser-setup.ts`. Add a `Gate` test (in its existing test file) that a sign-out empties the store.
- [ ] **Step 4: Watched mutations**, each turning a named case red, then restored: the newest-wins check removed; the text comparison removed (always replace); a set applied answer by answer as each lands; `invalidate` joining a request already out; `emptyReads` not dropping answers in flight. Record which case went red for each.
- [ ] **Step 5:** Run the three projects whole, typecheck and lint. Commit.

### Task 2: The page hook and the wait line

**Files:**
- Create: `web/src/usePage.ts`.
- Modify: `web/src/settingsLayout.tsx` (`PageLoading` beside `PageHeader`).
- Test: `web/src/usePage.test.tsx` (`jsdom`, fake timers), driving a small test page that draws a header, `PageLoading`, and its body only when `ready`; `settingsLayout`'s existing test file for `PageLoading`.

**Interfaces:** Consumes `reads.ts` (Task 1). Produces `usePage`, `Page`, `PageLoading`.

- [ ] **Step 1: Tests first**, per spec §4: a first visit draws the header and nothing else at 199 ms, `Loading…` at 200 ms, and the whole body in the one render in which the last read settles; an answer within 200 ms never draws `Loading…`; a revisit (unmount, remount with every entry answered and fresh) draws the body in its first render, never draws `Loading…`, sends one re-read per address, and its body element updates at most once when the re-reads land changed, and not at all when unchanged (count renders of a body element with a render counter, per element); a stale entry on arrival waits as a first visit; an entry going stale while drawn keeps the body drawn until the re-read lands; some answered and some not waits, and the answered ones are re-read in the same set; an `undefined` read holds `ready` false, and once it becomes a `Read` the page waits for that read alone; a failed read on a first visit makes the page ready with the failure in `failures`; a failed re-read keeps the answer and sets the failure; an empty list is ready.
- [ ] **Step 2: Red.** Record.
- [ ] **Step 3:** Implement.
- [ ] **Step 4: Watched mutations:** a stale entry counted as answered on arrival; `ready` falling back to false when an entry goes stale while drawn; the mount's re-read issued per address instead of as one set; `loadingDrawn` without `useDrawnWait` (drawn at once). Record the case each turned red.
- [ ] **Step 5:** Three projects whole, typecheck, lint. Commit.

### Task 3: The sidebar's library status and Libraries

**Files:**
- Modify: `web/src/pageReads.ts` (`LIBRARIES`, `LIBRARY_STATUS`).
- Modify: `web/src/Shell.tsx`: the status effect (~:112-136) and `statusReads`/`refreshStatus` go; `status` and `failure` come from `usePage([LIBRARY_STATUS])`; `StatusRefreshContext.Provider` (~:260) goes; `StatusContext.Provider` (~:261) and the Sidebar's prop (~:369) stay; the failure alert in `<main>` (~:393) is unchanged. Do not draw `PageLoading` here: the Shell is not a page.
- Modify: `web/src/shell.ts`: `StatusRefreshContext` (~:69) and `useStatusRefresh` (~:72-74) go, with their docs.
- Modify: `web/src/Libraries.tsx`: `reload` (~:135-152), `latest` (~:128) and the `pathname` effect (~:154-157) give way to `usePage([LIBRARIES])` for the drawn list and `invalidate(LIBRARIES, LIBRARY_STATUS)` wherever `reload()` or `refreshStatus()` ran (the `pathname` change, `changed` ~:183-188 with its `answering` skip kept, `answered` ~:208, `made` ~:258, `libraryRenamed` ~:356-382). Its failure sentences (~:424-431, "could not be read again (…), so what is drawn may be out of date" when a list is drawn) are kept, fed from `failures`. `LibraryAt` (~:568-595) draws the `PageHeader` with the URL's name (`nameAt`) and `PageLoading` while the list is unread, then the library's page as now.
- Modify: `web/src/LibraryTable.tsx` only if its hold on `libraries !== undefined` (~:70) needs `PageLoading` beside it.
- Test: `web/src/Libraries.test.tsx` (it wraps the screen in `StatusRefreshContext.Provider` ~:107-116: replace with assertions on `library.status` calls through a mocked `./library`); `Libraries.shell.browser.test.tsx`; `Libraries.address.browser.test.tsx`; `Shell.test.tsx` (uses `useStatusRefresh` ~:138); `Shell.browser.test.tsx`.

**Interfaces:** Consumes Tasks 1–2. Produces `LIBRARIES` and `LIBRARY_STATUS` for Tasks 4 and 5.

- [ ] **Step 1: Tests first:** Libraries' first visit holds the table body and draws `PageLoading` after 200 ms; a revisit (another tab, then Libraries) draws the table at once and re-reads quietly; a library's page draws its header from the URL's name before the list lands; after a library is renamed or made, the sidebar's list and Libraries' list each update from one invalidation (one read of each address); **O222's return case**: a write sent from Libraries whose answer lands after Users has replaced it, then a return to Libraries, draws the list as the write left it without a change of page (watch it fail on `main`).
- [ ] **Step 2: Red.** Record each.
- [ ] **Step 3:** Implement. Existing tests that counted reads per mount or used `StatusRefreshContext` change to what the store does; list each changed assertion and why in the report.
- [ ] **Step 4: Watched mutations:** the `invalidate` after a rename removed (the sidebar keeps the old name); `LIBRARY_STATUS` left out of Libraries' invalidation. Record.
- [ ] **Step 5:** Three projects whole, typecheck, lint. Commit (two commits if the Shell part stands alone).

### Task 4: A library's page waits for its scan status

**Files:**
- Modify: `web/src/pageReads.ts` (`libraryScan(id)` for `/api/library/libraries/{id}/scan`).
- Modify: `web/src/Libraries.tsx`, `LibraryAt` (~:568-595): once the list has found the library, it waits on `usePage([LIBRARIES, found === undefined ? undefined : libraryScan(found.id)])`, keeps the URL-named header and `PageLoading` up while waiting, and passes the scan answer to `LibraryPage` as a prop.
- Modify: `web/src/LibraryPage.tsx`: the mount read (~:231-247) goes; the prop seeds `running`, `elapsed` and `scanning` in the first render, so the running-scan notice (~:591-596), "Scanning…" (~:645) and the held buttons (~:546, ~:709) are in the first drawn body. A failed scan read stays silent (drawn as not running). The poll (~:249-281) keeps `SCAN_POLL_MS`, `POLL_MISSES_ALLOWED` and its start and stop rules, and reads with `readNow(libraryScan(id))`. `renameLibrary` and `addDir` keep calling `onChanged`, which Task 3 made an invalidation. The roots-address reads at ~:285-309 and in `RootRow.tsx` are untouched (spec §5).
- Test: `LibraryPage.test.tsx` (it stubs `libraryScanStatus` through `vi.mock('./libraries')` ~:31, ~:160), `Libraries.test.tsx` for `LibraryAt`.

**Interfaces:** Consumes Tasks 1–3.

- [ ] **Step 1: Tests first:** with a scan running, a library's page draws the running-scan notice, "Scanning…" and the held buttons in the same render as the rest of its body (watch it fail on `main`); a failed scan read draws the body as not running and no alert; the poll still stops on `!running` and after `POLL_MISSES_ALLOWED` misses; a revisit draws at once.
- [ ] **Step 2: Red.** **Step 3:** Implement. **Step 4: Watched mutation:** the scan read left out of the page's wait (the notice lands later). **Step 5:** Three projects whole, typecheck, lint. Commit.

### Task 5: Users and an account's page

**Files:**
- Modify: `web/src/pageReads.ts` (`USERS`).
- Modify: `web/src/Users.tsx`: `reload` (~:112-124), `latest` (~:98) and the libraries read (~:143-165) give way to `usePage([USERS, LIBRARIES])`; `reload` callers (`added` ~:189, `onChanged={reload}` ~:306) invalidate `USERS`. The users failure alert (~:219) and the libraries failure sentence on an account's page (~:227-232) are kept, fed from `failures`. `AccountAt` (~:343) draws the header with the URL's name and `PageLoading` while unread.
- Modify: `web/src/AccountPage.tsx`: "The libraries list has not been read, so there is nothing to offer here." (~:752-763) is never drawn while the list is being read. If the only state left in which `libraries` is `undefined` is a failed read, which Users already says, delete the branch; otherwise say in the report what still reaches it.
- Modify: `web/src/UserTable.tsx` only if needed, as `LibraryTable` in Task 3.
- Test: `Users.test.tsx`, `Users.address.browser.test.tsx`, `AccountPage.test.tsx`, `AccountPage.browser.test.tsx`.

- [ ] **Step 1: Tests first:** with the libraries read pending, an account's page draws neither its body nor "has not been read" (watch it fail on `main`); users and libraries land in one render; Libraries then Users sends one libraries read on arrival (the re-read), not two; a revisit draws at once.
- [ ] **Step 2: Red.** **Step 3:** Implement. **Step 4: Watched mutation:** the libraries read left out of the page's wait. **Step 5:** Three projects whole, typecheck, lint. Commit.

### Task 6: Backup and Rules

**Files:**
- Modify: `web/src/pageReads.ts` (`BACKUPS`, `RULES`, `ROOTS`, `ACCEPTED_PROPOSALS`).
- Modify: `web/src/Backup.tsx`: `reload` (~:89-105) and `latest` (~:87) give way to `usePage([BACKUPS])`; `takeBackup`'s re-read (~:116) invalidates `BACKUPS`; the list section (~:154) is drawn in the first body, so it is never inserted above "How backups work" (~:188). Failure alert (~:151) kept.
- Modify: `web/src/RuleEditor.tsx`: the `Promise.all` (~:344-356) gives way to `usePage([RULES, ROOTS, ACCEPTED_PROPOSALS])`; the editor's working copies (`rules` ~:202, `roots`, `proposals`) are seeded from the answers in the render the body is first drawn, and a later answer that changes them replaces the rules only while nothing is edited (find the editor's own "edited" notion and name it in the report); `runPreview` is sent once the body is first drawn; `saveRules` (~:407-459) and `importRules` (~:589) invalidate `RULES` and, as now after a save, `ACCEPTED_PROPOSALS`. The failure "The rules were not loaded: …" (~:354, ~:653) kept.
- Modify: `web/src/PreviewTabs.tsx`: whatever makes the comment above "No preview yet" (~:222-228) true: the pane holds its wait from the first render until the first preview answers. If that comes from `RuleEditor` setting `busy` before the first draw, `PreviewTabs` may not change; the comment must be true either way.
- Test: `Backup.test.tsx`, `RuleEditor.test.tsx`, `RuleEditor.browser.test.tsx`, `PreviewTabs.test.tsx`.

- [ ] **Step 1: Tests first:** Rules never draws "No preview yet", "0 rules" or "Pick a rule to edit it" before its reads land, and never draws "No preview yet" before its first preview answers (each watched failing on `main`); Backup's list and "How backups work" arrive in one render; a revisit to Rules draws at once and a changed re-read does not replace an edited rule; saving invalidates `RULES`.
- [ ] **Step 2: Red.** **Step 3:** Implement. **Step 4: Watched mutations:** the preview's held wait removed; the "only while nothing is edited" check removed. **Step 5:** Three projects whole, typecheck, lint. Commit.

### Task 7: `/account`, the reader's account panel, About, the build label and Licence

**Files:**
- Modify: `web/src/pageReads.ts` (`KEYS`, `SESSIONS`, `ABOUT`).
- Modify: `web/src/about.ts`: the memo (`asked` ~:40, `about` ~:42-53) becomes a plain read; `forgetAbout` (~:56-58) goes, and every test call of it (about.test.ts:13; Account.test.tsx and Account.browser.test.tsx, several; testing/aboutPage.tsx:47) goes with it, since `browser-setup.ts` empties the store.
- Modify: `web/src/Account.tsx`: `refresh` (~:369-403), the `listings` ref (~:331-337), `landListing` (~:215-234) and the `about()` effect (~:440-456) give way to `usePage(visible ? [KEYS, SESSIONS, ABOUT] : [])`. The writes (`createKey` ~:476-479, `changePassword` ~:525-532, `revokeKey` ~:764-767, `revokeSession` ~:808-809) invalidate `KEYS` and `SESSIONS`. A failed list is still drawn as nothing; the catalog's failure ("The catalog’s address could not be read.", `CatalogAddress.tsx`) still is. `/account`'s header (~:846) at once; the body behind `PageLoading`. The panel draws no `PageLoading` unless it already draws a wait.
- Modify: `web/src/About.tsx` (~:48-62): `usePage([ABOUT])`; failure alert (~:56, ~:67) kept. `web/src/VersionLabel.tsx` (~:29): `usePage([ABOUT])`, drawing what it draws today before and after the answer.
- Modify: `web/src/Licence.tsx` (~:164-192): the texts, once imported, are kept in a module-level variable, so a revisit draws them in its first render; a first visit draws `PageLoading` while the chunk loads. Failure sentence (~:165, ~:292) kept.
- Test: `Account.test.tsx` and `Account.browser.test.tsx` (they stub `fetch` globally; keep that), `About.test.tsx`, `About.browser.test.tsx`, `VersionLabel.test.tsx`, `Licence.test.tsx`, `Licence.browser.test.tsx`, `about.test.ts`.

- [ ] **Step 1: Tests first:** `/account` draws no key row while sessions are unread and no session row while keys are (watch it fail on `main`); `/account`, About and the build label send one `/api/about` between them; About revisited draws at once; Licence revisited draws its texts in the first render; the panel reopened draws its lists at once.
- [ ] **Step 2: Red.** **Step 3:** Implement. **Step 4: Watched mutation:** the sessions read left out of `/account`'s wait. **Step 5:** Three projects whole, typecheck, lint. Commit.

### Task 8: The docs

**Files:**
- Modify: `docs/status.md`:
  - The Later entry "After PR 3: the loading work, in two PRs" (~:4868-4884): the first PR becomes what shipped (in the Configuration entry or its own, matching how #204 and #207 were recorded); the second stays in Later.
  - O222 (row ~:1788 and its prose): narrowed by Task 3's return case if that test went red on `main` and green here; the dropped report half stays open. The five-part edit applies only if a row closes; a narrowing changes the row's annotation and prose. Then grep the bare `O222` across `docs/` and `web/src/` and read every hit.
  - O200 (~:1766) and O209 (~:1775): read; correct a sentence only where it is no longer true.
  - The About entry's sentence on the build label's PR (~:5703-5705) gains "(#207, merged 2026-10-27)".
  - The web test counts ("plus N in `web/`'s TypeScript — X in the `node` project and Y in the `jsdom` one"): measured from whole-project runs at your commit, not added to. The Rust count is unchanged.
- Grep `docs/` and `README.md` for `forgetAbout`, `useStatusRefresh`, `StatusRefreshContext` and "read again on each change of page", and fix any sentence this branch made false.

- [ ] **Step 1:** Edit. **Step 2:** `node scripts/status-test-count.mjs` and the doc-link check as `scripts/check-all.sh` runs them. Commit.

---

## After the look (added 2026-10-27)

The user ruled that §9's old known limit 8 is to be fixed (spec §2.6, §9 call 9), and the claim check found the defects of Task 10. Both tasks build on the head after `fd908db5`.

### Task 9: A directory's own scan marks its library's scan status stale (spec §9 call 9)

**Files:**
- Modify: `web/src/reads.ts` (new `markStale`, beside `invalidate` at ~:216, and its doc)
- Modify: `web/src/RootRow.tsx` (a new optional prop beside `onScan` at :141; calls in `begin` ~:567-610 for a start and a 409, the mount read's adoption ~:519-526, the poll's end ~:634, and the poll effect's cleanup ~:647 while running)
- Modify: `web/src/DirPage.tsx` (`<RootRow` at :129 passes the mark for its `library.id`)
- Test: `web/src/reads.test.ts`; the Libraries cases that drive a directory's page from a library's, in `web/src/Libraries.test.tsx` or `web/src/Libraries.shell.browser.test.tsx` (grep `side by side`, phone width and `scanStatus` mocks there and put each case next to its nearest neighbour); `web/src/RootRow.test.tsx` and `web/src/DirPage.test.tsx` for the prop's calls

**Interfaces:**
- Produces: `export function markStale(...reads: readonly Read<unknown>[]): void` in `reads.ts`; `RootRow`'s `onScanMoved?: () => void`.
- Consumes: `libraryScan(id)` from `pageReads.ts:35`; `invalidate`'s handling of drawn and undrawn addresses (`reads.ts` ~:216-232).

`markStale`'s rule: an address with nothing kept is left alone (a first visit reads anyway). An entry is marked stale and no listener is told, so nothing is drawn again and no request is sent. Where the address is not drawn, a request already out is dropped, as `invalidate` drops it. Where it is drawn, a request already out still answers its reader. Invariant: after the mark, an arrival at the address draws no answer to a request sent before the mark. If keeping that for a drawn address with a request out needs more than recording, at the mark, which request it was, report it rather than build it.

- [ ] **Step 1: Store tests, red.** In `reads.test.ts`, beside the `invalidate` cases: nothing kept (no request, no entry); kept and not drawn (the next arrival re-reads and is not ready until it lands; an answer to a request from before the mark does not land); kept and drawn (no listener called, no request, the drawn answer unchanged; after unmount the next arrival re-reads); drawn with a request out from before the mark (its answer reaches the reader; the next arrival still re-reads). Run: red, as `markStale` does not exist.
- [ ] **Step 2: `markStale`.** Implement it with its doc (why: a write whose change no reader on screen should take up). Step 1 green.
- [ ] **Step 3: Screen tests, red on this head.** Each fails today; record the failure line:
  - (a) Phone width: a library's page with no scan kept; its directory's page; Scan starts; Back while it runs. Before its read lands the library's page draws neither a directory's last scan where `Scanning…` belongs (`LibraryPage.tsx` ~:817) nor a free Scan library; then it draws the scan running.
  - (b) Phone width: the library's kept status says running; the directory's page sees the scan end; Back. The library's page never draws `Scanning…` or a held Scan library; it draws no scan and no report (call 2).
  - (c) The directory's page left for another screen while its scan runs; the scan ends; the library's page then waits for its read.
  - (d) The directory's mount read takes up a running scan the library's kept status does not have; Back: as (a).
  - (e) Wide window, the library's page beside the directory's, through a directory's scan and its end: Scan library and Remove… stay free, the library's page sends no status read, and one report is drawn, `RootRow`'s. This passes before and after the fix; watch it red with `invalidate` hand-applied in place of `markStale` in `DirPage.tsx`, then restore.
- [ ] **Step 4: Wire it.** The `RootRow` prop and its four call sites, and `DirPage.tsx` passing `() => markStale(libraryScan(library.id))`. Step 3 green.
- [ ] **Step 5: Each call site is reached.** Hand-remove each of the four call sites in turn and see at least one case red for each; restore. Record which.
- [ ] **Step 6:** Whole `node`, `jsdom` and `browser` projects, typecheck and lint, each into a log (Global Constraints). Commit, its message one sentence in the style of `git log -3 --format=%s`.

### Task 10: The claim check's minor defects

**Files:**
- Modify: `web/src/RuleEditor.tsx` (`unread` at :239-240; the comments at ~:285 and ~:605 that cite `Libraries.tsx`'s `createFailure`, which does not exist)
- Modify: `web/src/LibraryPage.tsx` (`Watch.from` at :177, `endWatch` at :337, the adoption at ~:356-365 and what moves a watch from `kept` to `status`)
- Modify: `web/src/Backup.tsx` (:81, "See `Users.tsx`'s own note", which does not exist)
- Modify: `web/src/usePage.ts` and `web/src/usePage.test.tsx` (:335) only if Step 4 finds the order claim false
- Modify: `docs/status.md` (the web test counts)
- Test: `web/src/RuleEditor.test.tsx`, `web/src/LibraryPage.test.tsx` (or wherever the kept-running return cases live: grep `from: 'kept'`'s cases), `web/src/Account.test.tsx` (:4026)

- [ ] **Step 1: Rules' alert names the read that kept the body from being drawn.** Today `unread` is the first failure in read order, so a return whose rules read fails with an answer kept and whose roots read fails with nothing kept gives the rules read's reason. Test that case, red; then pick the failure of a read with no answer kept. The alert's sentence is unchanged (Global Constraints).
- [ ] **Step 2: A scan this visit saw running draws its reports when it ends (call 2).** Today a return whose re-read answers in the kept status's exact words (same `elapsed_secs`) leaves the store's entry as it was, so the watch stays `kept`, and when the first poll finds the scan over no report is drawn. Test that case, red; then move the watch to `status` once this visit's read has settled saying running, whether or not the entry changed. Keep the case where the first read finds the scan over drawing no report (call 2).
- [ ] **Step 3: The panel draws no Loading….** `Account.test.tsx:4026` asserts the head, and that no status on the page says `Loading…`, only after `WAIT_DRAWN_AFTER_MS`. Add, per element, that no `Loading…` status is drawn in the panel at once and after the wait while its reads are out. Watch it red by hand-making the panel draw `PageLoading`; restore.
- [ ] **Step 4: Is the read effect's order real?** `usePage.ts` says the read effect's place after `useSyncExternalStore` keeps an invalidation from dropping the read, and `usePage.test.tsx:335` is named for it. Hand-move the effect before the subscription and run that case. If it goes red, record it. If it stays green, find a case that goes red, or rewrite the comment and the test's name to say only what holds.
- [ ] **Step 5: The two dangling references.** Rewrite the `RuleEditor.tsx` and `Backup.tsx` comments to give the reason themselves (read the code), with no history.
- [ ] **Step 6:** Whole `node`, `jsdom` and `browser` projects, typecheck and lint into logs; set status.md's web test counts from those runs (the sentence "plus N in `web/`'s TypeScript — X in the `node` project and Y in the `jsdom` one"). Commit, one sentence as in Task 9.

---

## After the tasks (controller)

1. Whole-branch review, then a fix round and a review of the fixes; after fixing a finding, grep for its shape across the branch.
2. The full gate, `scripts/check-all.sh`, into a log; quote its `all green` line and exit status.
3. The user's look: a server built from the branch on a scratch copy of the test library (`127.0.0.1:4399`, `local`/`pw`), with the network throttled so `Loading…` shows: first visits and revisits on each screen of spec §5.
4. Claim check over the docs and the PR body (which records the watched reds and mutations); then the push and PR asks, each separately.
