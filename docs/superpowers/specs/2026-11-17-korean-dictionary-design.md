# Books: a Korean dictionary, with particles taken off a word — design

**Status: settled in brainstorming on 2026-11-17. Built in PR #262.**

This is **project 4 of 4** of the book reader's gaps. It is built on search and vertical text (#260) and tap look-up (#261), both merged.

## Why

The reader's look-up (select a word, then "Look up") answers "this server has no dictionary for ko" in a Korean book. The test library holds all 7 Korean Harry Potter books (`Literature/J. K. Rowling/Harry Potter (Korean)`).

## What was found (2026-11-17)

- **The source:** kaikki.org's Korean extract, `https://kaikki.org/dictionary/Korean/kaikki.org-dictionary-Korean.jsonl`, is 207,649,961 bytes with 57,810 distinct words. It has the same shape as the Spanish, Russian and Japanese extracts, so `taiyaku dictionary import ko <file> --dir DIR` reads it unchanged.
- **Verbs and adjectives already work.** They carry their conjugation tables as forms, a median of 98 rows and 79 distinct forms a verb over the 3,443 verbs `ko.db` holds forms for. For example, 진정했습니다 → 진정하다 through the existing `form` table.
- **Nouns don't.** Korean writes a noun with its particles as one spaced word (해리가, 학교에서, 친구들은), and none of those are entries or forms.

## Goals

1. Korean words in the books look up: verbs and adjectives through their forms, as now, and a noun followed by particles through particle stripping.
2. Install the dictionary on this machine: download the extract, import `ko.db` into `taiyaku-library/Dictionaries/`, and keep the raw file in `raw/` like the others.

## Non-goals

- A morphological analyser.
- Verb endings absent from the conjugation tables (connectives such as -는데, -으면서 beyond what the tables list).
- Korean in tap look-up's `/at` route. Korean has spaces, so a tap takes the run of letters and decimal digits around the character tapped, which goes through the same lookup as a selection: 10년 sends 10년. A run of digits with no letter is not a word, so a tap on a bare number keeps its usual meaning. A drag over the letters alone sends only those letters, so 년 dragged out of 10년 is looked up as 년.
- Hanja-only entries.

## Anti-goals

- **A wrong answer where the plain lookup would have found the word.** Particles never come off a word with an entry of its own. A word found only as a form of another is stripped as well, and what stripping finds comes after the form's results, except for the pronouns named in the design below.
- **Any change to Spanish, Russian or Japanese lookups.**

## Approaches

1. **A particle list on the server: chosen** by the user on 2026-11-17. It is small, needs no dependency, fits `lookup`, and serves both the select path on `main` and the tap path in `book-tap-lookup`.
2. **A Korean morphological analyser** (a Rust crate with its own dictionary, tens of MB). It is exact, but heavy, and mostly redundant with the conjugation tables.

## The design

### Library: `taiyaku-library::dictionary`

- **`pub fn lookup_korean(conn: &Connection, word: &str) -> Result<Vec<Found>, String>`**:
  1. `lookup(conn, word)`. If it finds an entry under the word's own key, return what it found.
  2. Otherwise, for each particle ending in `PARTICLES`, shortest strip first: if `word` ends with it, the stem is non-empty, and the stem agrees with it (below), then `lookup(conn, stem)`, passing over entries whose headword starts or ends with a hyphen. `key` trims that hyphen, so the suffix -거라 is filed under 거라 and would otherwise answer for 거라는 in place of 거. A stem left with nothing is stripped further, as one that found nothing. The word as written (step 1) keeps its affix entries: 거라 shows -거라.
  3. Take the first stem's result that finds anything left. Each `Found` from a stripped lookup has `via` set to the word's key, so the panel says "from 해리가".
  4. A word step 1 found only as a form of another is stripped too: its form results come first and the stripped results after, each headword once, except that 그, 나, 너 or 우리 with 는 or 를 on lists the pronoun first, since 그는 is also a form of 글다.
- **Stacked particles:** stripping is repeated up to 3 times (학교에서는 → 학교에서 → 학교). The first found stem wins, and the shortest strip is tried first at each step.
- **`PARTICLES`** is a constant list, each with the stem-final condition it needs:
  - **Needs a final consonant (batchim):** 은, 이, 을, 과, 으로, 으로는, 으로서, 으로써, 이랑, 이나, 이에요, 이었다, 이라고.
  - **Needs no final consonant (or final ㄹ for 로-forms):** 는, 가, 를, 와, 로, 로는, 로서, 로써, 랑, 나, 다, 예요, 라고.
  - **Either:** 이다, 입니다, 의, 에, 에서, 에서는, 에서도, 에게, 에게서, 에게는, 한테, 한테서, 께, 께서, 도, 만, 까지, 부터, 처럼, 보다, 하고, 마다, 조차, 밖에, 들, 들은, 들이, 들을, 들의, 들에게.
  - The batchim test reads the stem's last syllable (U+AC00–D7A3): `(code - 0xAC00) % 28 != 0` means a final consonant, and `% 28 == 8` means final ㄹ.
  - The list is a starting point. The builder may add common particles, each with a test.
- **The route:** `GET /api/dictionary/{lang}/{word}` calls `lookup_korean` where `lang` (after its existing normalisation) is `ko`, and `lookup` otherwise. No route, posture or JSON change.

### Installing the dictionary (a task step, not product code)

1. Download the extract into `taiyaku-library/Dictionaries/raw/kaikki-Korean.jsonl`.
2. Run `taiyaku dictionary import ko …/raw/kaikki-Korean.jsonl --dir taiyaku-library/Dictionaries`.
3. Record the entries, forms and skipped counts.
4. Measure on the real books: sample 200 spaced words from Harry Potter 1 (Korean), excluding pure punctuation and numbers. Report how many look up with plain `lookup` and how many with `lookup_korean`, and list up to 20 misses.

## Calls made for the user (may be overruled)
- Stripping leaves a word with an entry of its own alone, and goes up to 3 deep, shortest strip first.
- Particles must agree with the stem's final consonant.
- A stripped result shows "from <the word's key>" through `via`.
- Korean doesn't go through the `/at` route.

## Left to the builder
- The exact particle list beyond the one above, each addition with a test.
- Where the Korean code sits: a `korean` submodule of `dictionary` is fine.
