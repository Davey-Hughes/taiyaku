# Korean Dictionary Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Korean words in the book reader look up. Verbs and adjectives look up through the conjugation forms the kaikki extract already carries, and nouns through particles taken off the end of a word with no entry of its own.

**Architecture:** A `korean` submodule of `taiyaku-library::dictionary` adds `lookup_korean`. It tries `lookup` first, then strips particles that agree with the stem's final consonant, up to 3 deep. The existing dictionary route calls it for `ko`. A task then installs the real dictionary and measures it on Harry Potter 1 (Korean).

**Tech Stack:** Rust (rusqlite, axum). No new dependency.

**Spec:** `docs/superpowers/specs/2026-11-17-korean-dictionary-design.md`

## Global Constraints

- **Base:** built on search and vertical text (#260) and tap look-up (#261), both merged.
- **No change to other languages:** Spanish, Russian and Japanese lookups behave exactly as on `main`.
- **No new dependency.**
- **When stripping runs:** where the word as written has no entry of its own, so also where it is found only as a form of another word. Up to 3 strips deep, and the shortest strip is tried first at each step.
- **Final consonant (batchim):** read off the last syllable U+AC00–D7A3. `(code - 0xAC00) % 28 != 0` means a final consonant, and `== 8` means final ㄹ. A stem whose last char is not a Hangul syllable takes either variant.
- **Display:** a stripped result's `via` is the word as written (after `key()`), so the panel says "from 해리가".
- **Comments** give a reason, never a restatement. Doc comments are at most 8 lines.

## Review Focus

1. **A noun that ends in what looks like a particle,** such as 아이 ("child") or 나이 ("age"), is found as itself and never stripped. Test in Task 1.
2. **A word that is only a particle** (이, 가, 에서) is never stripped to an empty stem. Test in Task 1.
3. **A Latin or numeric stem with a particle** (Harry가, 3층에) still strips, since the batchim test doesn't apply. Test in Task 1.
4. **Punctuation around the word** (「해리가!」) still finds 해리. Stripping works on `key(word)`. Test in Task 1.
5. **A Spanish, Russian or Japanese word** given to the `ko` path finds nothing new, and the other languages' route answers are byte-identical to `main`. Test in Task 2.

---

### Task 1: `lookup_korean`

**Files:** Create `crates/taiyaku-library/src/dictionary/korean.rs` (with `mod korean;` and `pub use korean::lookup_korean;` in `dictionary.rs`). Test `crates/taiyaku-library/src/dictionary/korean/tests.rs`.

**Interfaces:**
- Consumes `dictionary::lookup(conn, word) -> Result<Vec<Found>, String>` and `dictionary::key`.
- Produces `pub fn lookup_korean(conn: &Connection, word: &str) -> Result<Vec<Found>, String>`.

- [ ] **Step 1: Write the failing tests.** Use a fixture built with `import` from kaikki-shaped lines for 해리 (noun), 학교 (noun), 친구 (noun), 아이 (noun, "child"), 아 (interjection), 나이 (noun, "age"), 먹다 (verb, with forms 먹었습니다 and 먹어요), 집 (noun), 층 (noun) and 길 (noun). Tests:
  - `해리가`, `해리는`, `해리를` and `해리의` each find 해리, with `via == Some("해리가")` and so on.
  - `학교에서`, `학교에서는` and `학교까지` find 학교.
  - `친구들에게는` finds 친구 (three strips: 는, 에게, 들).
  - `집은`, `집을` and `집이` find 집. `집는` finds nothing, because 는 needs no final consonant and 집 has one.
  - `길로` finds 길 (final ㄹ takes 로).
  - `아이` finds 아이 (child) with `via == None`, never 아 (Review Focus 1). `나이` finds 나이.
  - `먹었습니다` finds 먹다 through its form, exactly as `lookup` does (no stripping).
  - `이`, `가` and `에서` alone find nothing and don't panic (Review Focus 2).
  - `Harry가` with a fixture entry `harry` finds it (Review Focus 3). `3층에` finds 층 only if `key` keeps the digit; assert whatever `key` gives, and document it.
  - `「해리가!」` finds 해리 (Review Focus 4).
  - A word that finds nothing as written and after every strip gives an empty Vec.
- [ ] **Step 2: Run to verify they fail.** `cargo test -p taiyaku-library dictionary::korean` Expected: compile error, then FAIL.
- [ ] **Step 3: Implement.** Particles are a constant table of `(&str, Needs)`, where `Needs` is `Consonant`, `Vowel` (or ㄹ, for the 로-forms only), `Vowel` or `Either`, with at least the spec's list. Try them ordered so the shortest strip is tried first at each depth, and recurse up to depth 3. Set `via` on every stripped result.
- [ ] **Step 4: Run.** Expected: PASS. Run clippy `-D warnings` for the crate.
- [ ] **Step 5: Commit:** "Dictionary: a Korean word looked up with its particles taken off".

### Task 2: The route uses it for Korean

**Files:** Modify `crates/taiyaku-serve/src/dictionary.rs` (the `lookup` handler's blocking closure). Test its existing test module.

**Interfaces:** Consumes `taiyaku_library::dictionary::lookup_korean`.

- [ ] **Step 1: Write the failing tests:**
  - With a `ko` dictionary fixture in the served dictionaries directory, `GET /api/dictionary/ko/해리가` answers 200 with the entry 해리 and `via` "해리가". `GET /api/dictionary/ko-KR/해리가` answers the same, through the existing region normalisation.
  - With the `es` fixture, the existing tests' exact JSON for `hablaron` is unchanged (Review Focus 5).
- [ ] **Step 2: Run to verify they fail.** `cargo test -p taiyaku-serve dictionary::`
- [ ] **Step 3: Implement.** Branch on the normalised language: `ko` calls `lookup_korean`, and anything else calls `lookup`.
- [ ] **Step 4: Run** `cargo test -p taiyaku-serve` and clippy. Expected: PASS.
- [ ] **Step 5: Commit:** "Dictionary route: Korean words looked up through their particles".

### Task 3: Install the dictionary, measure it, gate and status

**Files:** Modify `docs/status.md`. Write a measurement note to `docs/notes.md` (append a short section in its existing style). No product code.

- [ ] **Step 1: Download** `https://kaikki.org/dictionary/Korean/kaikki.org-dictionary-Korean.jsonl` to `taiyaku-library/Dictionaries/raw/kaikki-Korean.jsonl`. Check the size is 207,649,961 bytes (or report the size seen if kaikki has updated it).
- [ ] **Step 2: Import.** `cargo run --release -p taiyaku-cli -- dictionary import ko taiyaku-library/Dictionaries/raw/kaikki-Korean.jsonl --dir taiyaku-library/Dictionaries` (check the CLI's exact usage first). Record entries, forms and skipped. Expected: `taiyaku-library/Dictionaries/ko.db` exists.
- [ ] **Step 3: Measure.** With a throwaway program or test, not committed, take the first 200 spaced words of Harry Potter 1 (Korean), `taiyaku-library/Books/Literature/J. K. Rowling/Harry Potter (Korean)/1 - 해리포터와 마법사의 돌 (Pottermore).epub`, read through `taiyaku_read::read_book`. Skip words with no Hangul. Report how many `lookup` finds, how many `lookup_korean` finds, and up to 20 misses with a guess at why each missed (an ending missing from the conjugation tables, a name, a stacked particle, and so on). Put the figures in the `docs/notes.md` section.
- [ ] **Step 4: Run the full gate.** `scripts/check-all.sh > target/check-all.log 2>&1; echo "exit $?"`, in the background with `pnpm` on PATH. Expected: exit 0. A browser test failing in a file this branch doesn't touch must be re-run alone and reported.
- [ ] **Step 5: Update `docs/status.md`.** Add the Books entry's line for the Korean dictionary, and an O-line from the next free number for each gap the measurement shows a reader would meet (verb endings beyond the tables, names, and so on). Correct the counts the gate checks.
- [ ] **Step 6: Commit** `docs/status.md` and `docs/notes.md`: "Korean dictionary: installed, measured on Harry Potter 1, status". Never commit the `.db` or `.jsonl` files (check `taiyaku-library/` is ignored or outside git).
