# Books tap look-up Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** In a look-up mode switched by `l` or a bars button, a single tap on a word in the book reader looks it up. Japanese finds the longest dictionary word from the tapped character, through inflected forms. Other languages take the run of letters around the tap.

**Architecture:**
- **Library:** `taiyaku-library::dictionary` gains a longest-prefix look-up.
- **Server:** `taiyaku-serve` exposes it on a new route.
- **Web:** the reader gains a mode module. The tap handler finds the word's block-text range and lights it as the DOM selection, and "Save word" saves a highlight note over that range.

**Tech Stack:** Rust (rusqlite, axum 0.8), TypeScript/React, vitest (node, jsdom, browser).

**Spec:** `docs/superpowers/specs/2026-11-17-book-tap-lookup-design.md`

## Global Constraints

- **Base:** builds on `main`, with project 2 (search and vertical Japanese, #260) merged into it.
- **Mode off:** every tap, double tap and drag behaves exactly as on `main`.
- **Notes:** only "Save word" writes a note.
- **Offsets:** UTF-16 offsets into `block.text`. Ruby `<rt>` (marked `data-aside`) is never counted.
- **Server limits:** `text` at most **16 code points**, and more is refused with 400. `length` is answered in **UTF-16 units**.
- **Prefixes:** a prefix stops before a character `ends_word` names. A prefix whose `key()` differs from its own lower case is skipped. The match starts at the tapped character, and a single character may match.
- **Japanese:** an edition is Japanese by `isJapanese` in `bookSearch.ts`: a language tag of `ja` or `jpn`, or, with no tag or `und`, a text more than half kana and kanji with some kana. A tap on such an edition always asks the `ja` dictionary.
- **Keys and labels:** `l` toggles the mode, and the button is labelled "Look up words". The mode is remembered per browser, default off.
- **Tooling:** run web tools from `web/` via `node_modules/.bin`.
- **Comments** give a reason, never a restatement. Doc comments are at most 8 lines.
- **Plan deviation from the spec:** the route is `GET /api/dictionary/{lang}/at/{text}` (`at` and the text as separate path segments), not `?text=`. axum would otherwise route the word "at" to the new handler, since `/api/dictionary/{lang}/{word}` already exists.

## Review Focus

1. **A tap on whitespace or punctuation with the mode on** does what it does today (toggles the bars). It never looks up an empty word. Test in Task 3.
2. **Two quick taps on different words, the first answer arriving last.** The panel shows the second word. A stale answer never replaces a newer look-up. Test in Task 3.
3. **A Japanese prefix holding an astral kanji or ending at 。** is cut by code point, never mid-surrogate, and stops at the punctuation. Test in Task 1.
4. **A tap with the mode on, on a word inside a highlight,** looks the word up rather than opening the note (the mode was chosen). A tap on the note's mark still opens the note. Test in Task 3.
5. **Closing the panel after the reader has drag-selected something else** leaves their selection alone. Test in Task 3.

---

### Task 1: `longest_at` in the dictionary library

**Files:** Modify `crates/taiyaku-library/src/dictionary.rs`. Test `crates/taiyaku-library/src/dictionary/tests.rs`.

**Interfaces:**
- Produces `pub fn longest_at(conn: &Connection, text: &str) -> Result<(usize, Vec<Found>), String>`. It returns the matched prefix's length in **chars**, the variation selectors and stress marks after it included, and that prefix's `lookup()`. It returns `(0, vec![])` where nothing matches. It considers at most 16 chars, and a prefix stops before a character `ends_word` names. A prefix where `key(prefix) != prefix.to_lowercase()` is skipped.
- [ ] **Step 1: Write the failing tests.** Add to `SAMPLE` an entry `{"word":"食べる","pos":"verb","lang_code":"ja","senses":[{"glosses":["to eat"]}],"forms":[{"form":"食べました","tags":["polite","past"]},{"form":"食べ","tags":["stem"]}]}` and `{"word":"は","pos":"particle","lang_code":"ja","senses":[{"glosses":["topic marker"]}]}`. Tests:
  - `longest_at(&conn, "食べました。今日")` gives `(5, [食べる via 食べました])`.
  - `longest_at(&conn, "食べ物")` gives `(2, [食べる via 食べ])`.
  - `longest_at(&conn, "はい")` gives `(1, [は])`.
  - `longest_at(&conn, "。食べる")` gives `(0, [])`, since a prefix can't start with punctuation.
  - `longest_at(&conn, "𠮷食べる")` returns `Ok` with no panic and length 0. The astral char is counted as one char.
  - `longest_at(&conn, "hablaron mucho")` gives `(8, [hablar via hablaron])`, stopping at the space.
  - More than 16 chars of input considers only the first 16.
- [ ] **Step 2: Run to verify they fail.** `cargo test -p taiyaku-library dictionary::` Expected: compile error, then FAIL.
- [ ] **Step 3: Implement** by trying prefix lengths from the longest down, reusing `lookup`.
- [ ] **Step 4: Run.** Expected: PASS. Also run `cargo clippy -p taiyaku-library --all-targets -- -D warnings`.
- [ ] **Step 5: Commit:** "Dictionary: the longest word a text begins with, found through its forms".

### Task 2: The `/at` route

**Files:** Modify `crates/taiyaku-serve/src/dictionary.rs` and `crates/taiyaku-serve/src/routes.rs` (the router near :2319, the grant posture table near :2701, the narrowing table near :3173, each beside `/api/dictionary/{lang}/{word}` and with the same posture and reason). Modify the route list in `crates/taiyaku-serve/src/auth/tests.rs` if the existing dictionary route is listed there. Test `crates/taiyaku-serve/src/dictionary/tests.rs` (check that it exists, and follow the existing route tests' fixture).

**Interfaces:**
- Consumes `longest_at` (Task 1).
- Produces `GET /api/dictionary/{lang}/at/{text}`, answering `{"length": <utf16 units>, "entries": [<same entry JSON as the existing route>]}`. It is 404 where there is no dictionary for `lang` (same message as the existing route), and 400 where `text` has more than 16 code points.
- [ ] **Step 1: Write the failing tests:**
  - `食べました。` gives 200 with length 5 and the entry `食べる`.
  - A text with an astral char before a word gives the length in UTF-16 units (2 per astral char where it is part of the match; build a fixture form that contains one).
  - A 17-code-point text gives 400.
  - An unknown language gives 404.
  - The existing `/api/dictionary/{lang}/{word}` still answers the word `at`, if a fixture has it, or at least still answers `hablar`.
- [ ] **Step 2: Run to verify they fail.** `cargo test -p taiyaku-serve dictionary::`
- [ ] **Step 3: Implement** by mirroring the existing `lookup` handler's dictionary opening and error paths.
- [ ] **Step 4: Run** `cargo test -p taiyaku-serve` and clippy. Expected: PASS, including the route-table completeness tests.
- [ ] **Step 5: Commit:** "Dictionary route: the longest word at a text's start, for a tap".

### Task 3: The look-up mode and the tap

**Files:**
- Create `web/src/modules/lookup.ts` and `web/src/wordAt.ts`.
- Modify `web/src/api.ts` (`lookUpAt`), `web/src/BookReader.tsx` (the module, a bars button, `onPointerUp`, the `lookingUp` flow and the selection light) and `web/src/styles/book.css` (the button's pressed state, if needed).
- Test `web/tests/wordAt.test.ts` and `web/tests/BookReader.test.tsx`.

**Interfaces:**
- Consumes Task 2's route.
- Produces:
  ```ts
  // api.ts
  export function lookUpAt(lang: string, text: string): Promise<{ length: number; entries: DictionaryEntry[] } | undefined> // undefined on 404
  // wordAt.ts
  export function wordAt(text: string, at: number): [number, number] | undefined // run of \p{L}\p{M} (inner ' or - kept) around at; undefined on whitespace/punctuation
  export function headAt(text: string, at: number, most = 16): string           // up to `most` code points from at
  // modules/lookup.ts
  export const TOGGLE_INTENT = 'lookup:toggle'
  export const lookupModule: ReaderModule<{ on: boolean }> // key 'l', defaults { on: false }
  ```
- [ ] **Step 1: Write the failing tests:**
  - (node) `wordAt('¿Hablaron mucho?', 3)` gives `[1, 9]`. `wordAt("l'homme est", 0)` gives `[0, 7]`. `wordAt('a  b', 1)` is undefined. `wordAt('говори́т', 2)` covers the whole word, combining accent included. `headAt('𠮷あいう', 0, 2)` gives `'𠮷あ'`.
  - (jsdom) **Mode off:** a tap on a word toggles the bars as on `main`, and no look-up request is made.
  - (jsdom) `l` toggles the mode. The bars button shows `aria-pressed`, and the pref persists across a remount.
  - (jsdom) **Mode on, English/Spanish book:** a tap on a word (offset driven by stubbing the caret-from-point helper) calls the existing `lookUpWord` with that word. The look-up panel opens, the DOM selection's text equals the word, and the bars don't toggle.
  - (jsdom) **Mode on, `ja` book:** a tap calls `lookUpAt('ja', headAt(text, at))`. With a stubbed answer `{length: 5, entries}`, the selection covers `[at, at + 5)`, and "Save word" creates a highlight note with exactly that range.
  - (jsdom) **Review Focus 1:** a tap on whitespace or punctuation with the mode on toggles the bars, with no request.
  - (jsdom) **Review Focus 2:** two taps on different words with the first answer resolved last. The panel and the selection show the second word.
  - (jsdom) **Review Focus 4:** in the mode, a tap on a highlighted word looks it up, and a tap on a `.book-note-mark` opens the note.
  - (jsdom) **Review Focus 5:** open a look-up, then drag-select other text, then close the panel. The drag selection is unchanged.
  - (jsdom) A double tap on a word in the mode closes the look-up, clears its selection, and peeks.
  - (jsdom) A press past `TAP_SLOP` or held past `TAP_HOLD` in the mode still drag-selects and opens the selection bar, with no look-up.
  - (jsdom) A tap on an interlinear line or a peek keeps its existing behaviour in the mode.
- [ ] **Step 2: Run to verify they fail.** `node_modules/.bin/vitest run --project node tests/wordAt.test.ts` and `node_modules/.bin/vitest run --project jsdom tests/BookReader.test.tsx`.
- [ ] **Step 3: Implement.**
  - The caret-to-offset helper is `document.caretPositionFromPoint` → `caretRangeFromPoint` → undefined. Its node and offset are converted to a block offset with the same `wordNodes` counting `selectionIn` uses. Export it from `bookNotes.ts` as `offsetAtPoint(text: HTMLElement, x: number, y: number): { block: number; at: number } | undefined`, so tests can stub it.
  - Track the in-flight request with a sequence number, so a stale answer is dropped.
  - Keep the selection range the tap set in a ref. Close clears the selection only if it still equals that range.
- [ ] **Step 4: Run** both files, then the full node+jsdom suites once, `tsc -b` and biome. Expected: PASS.
- [ ] **Step 5: Commit:** "Books reader: a look-up mode where a tap looks a word up, Japanese by its longest dictionary word".

### Task 4: Gate, status and a real book

**Files:** Modify `docs/status.md`.
- [ ] **Step 1: Run the full gate.** `scripts/check-all.sh > target/check-all.log 2>&1; echo "exit $?"`, in the background. Expected: exit 0. A known flaky browser test that fails must be re-run alone and reported.
- [ ] **Step 2: Real books, on a copy of the database in the scratchpad (never the user's own), with the server built from this branch, `--dictionaries <a directory of imported dictionaries>`:**
  - *Botchan*: mode on, tap 親 of 親譲り, then the first character of an inflected verb. Screenshot the panel each time.
  - A Spanish book: tap a word, then "Save word", and check the note.
- [ ] **Step 3: Update `docs/status.md`.** Add the Books entry's new paragraph, and an O-line from the next free number for each gap a reader would notice: a tap mid-word looks up from the tapped character; no Chinese or Korean segmentation. Correct the counts the gate checks.
- [ ] **Step 4: Commit:** "Books tap look-up: status".
