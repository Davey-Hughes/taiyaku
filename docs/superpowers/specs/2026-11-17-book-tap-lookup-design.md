# Books: a word looked up by a tap, in a look-up mode — design

**Status: settled in brainstorming on 2026-11-17, and built. "Built differently", at the end, says where the build departs from this design.**

This is **project 3 of 4** of the book reader's gaps. It started as "tap a word to look it up" plus "known-word tracking". The user dropped known-word tracking on 2026-11-17.

**Built on project 2.** Project 2 (search and vertical Japanese, #260) merged first, so this takes its Japanese test from `isJapanese` and its paged thirds from `pageTurnedBy`, which turns them the other way in vertical text.

## Why

A learner reading in another language looks words up constantly. Today that takes three actions: select the word, press "Look up", then read the panel (`BookNotes.tsx`'s selection bar, `BookLookup.tsx`). Selecting precisely on a phone is slow, and in Japanese, with no spaces, picking out exactly one word by dragging is hard.

## Goals

1. **The mode:** a look-up mode, switched with a button in the bars or the `l` key. While it is on, a single tap on a word looks it up.
2. **Japanese:** a tap finds the longest dictionary word starting at the tapped character, through inflected forms. A tap on 食 in 食べました finds 食べる.
3. **Spanish and Russian:** the word is the run of letters around the tap.
4. **What a found word gets:** it is lit while its panel is open, and "Save word" keeps working as today: a highlight note over exactly that word, with its meaning.

## Non-goals

- Known-word tracking. The user dropped it.
- Hover look-up or a modifier-click on desktop.
- Chinese or Korean segmentation. Korean has no dictionary yet (project 4).
- Changing the existing select-then-"Look up" path. It stays.

## Anti-goals

- **Changing the reader for someone not using the mode.** With the mode off, every tap, double tap and drag behaves exactly as on `main`.
- **A light or a look-up that moves or creates notes by itself.** Only "Save word" writes a note.
- **Breaking a drag-select while the mode is on.** A press past the tap slop or the hold time still selects, as today.

## Approaches (the gesture)

1. **A look-up mode toggled from the bars or `l`: chosen** by the user on 2026-11-17. The reader is unchanged unless the mode is chosen, and it works alike on a phone and a desktop.
2. **A long press on a word.** On a phone a long press starts the native text selection used for highlights, so the two would fight.
3. **Every tap on a word looks it up.** It changes the reader for everyone, and leaves page turning by tap only on empty space.

There is one sensible way to find the word: the server matches the longest prefix against the dictionary (below). The browser's `Intl.Segmenter` splits Japanese words but knows no inflection-to-headword link, and would disagree with the dictionary's own words.

## The design

### Server: a longest-prefix look-up

- **The route:** `GET /api/dictionary/{lang}/at?text=…`. `text` is at most 16 code points (more is refused with 400), starting at the tapped character. The access rules are those of the existing `GET /api/dictionary/{lang}/{word}`: same posture in every route table, and 404 where there is no dictionary for the language.
- **The answer:** `{ "length": n, "entries": [...] }`.
  - `n` is the length in UTF-16 units of the longest prefix of `text` that is an entry key or a form.
  - `entries` is what `lookup()` gives for that prefix, in the same JSON shape the existing route sends.
  - `{ "length": 0, "entries": [] }` where nothing matches.
- **Matching:** try prefixes from the longest (up to 16 characters, never past the first whitespace or punctuation character) down to 1. The first prefix that `dictionary::lookup` (with its `key()` normalisation) finds anything for wins.
- **Library side:** `taiyaku-library::dictionary::longest_at(conn, text) -> Result<(usize /* chars */, Vec<Found>), String>`. The route converts characters to UTF-16 units.

### Web: the mode, and the tap

- **The mode:** `modules/lookup.ts`, a new reader module with a `TOGGLE_INTENT` and key `l` (free on `main`'s book reader). A bars button, labelled "Look up words", shows its state.
  - The mode is remembered for this browser, like the other reader prefs, as `prefs.on` in the module's defaults (`false`).
- **The tap, with the mode on:**
  - In `onPointerUp`, after the existing slop and hold check and before the double-tap logic, a single tap on a word acts at once and clears the pending tap state.
  - A tap on a link, a note mark, a figure or an aside keeps its own meaning.
    *Note, 2026-11-25: narrowed for a page image, a cover or full-page
    illustration drawn on a page of its own, by the owner's rulings of
    2026-11-25 (O572, the book image pages PR). Paged outside compare, a tap
    on its outer thirds turns the page, and one in the middle third shows or
    hides the bars, as on a page's empty room, opening nothing; only a tap on
    its picture, where it is set in a link, follows the link.*
  - A tap on empty space, or on whitespace or punctuation, does what it does today.
  - A double tap still peeks.
- **Finding the word:**
  - Convert the tap point to a character offset in the block's own text through `document.caretPositionFromPoint` (or `caretRangeFromPoint`) and the `wordNodes` count `selectionIn` uses, so offsets are block-text offsets.
  - Japanese edition (`Edition.language` starts with `ja`): send the next 16 characters to `/at`, and the word is `[at, at + length)`. `length` 0 means "Not in the dictionary" for the tapped character alone.
  - Any other language: the word is the maximal run of letters and marks (`\p{L}\p{M}`, with an inner apostrophe or hyphen kept) around `at`, looked up with the existing route.
- **The light:** while the look-up panel is open, the word is the DOM selection, set programmatically over its range. It looks like a selection, which the browser draws, so no change to `pieces()` is needed, and "Save word" writes `lookingUp.range` without reading the selection back. Closing the panel clears it.
  - Setting the selection must not open the selection bar. The tap path sets `lookingUp` directly, with `{block, range, word, lang}`.
- **Saving:** "Save word" is unchanged, a highlight note over `range` with the meaning.

## Settled after the builder check (my calls)

1. **Which text and language a tap uses.** Only the main text: a block's own words, in the edition being read, with that edition's language. A tap on an aside (the interlinear line, a peek, a footnote card) keeps its existing meaning. A hidden interlinear line is revealed by a tap as today, and a shown one is not looked up. This keeps "Save word" exact, since its range is always in the shown edition's block.
2. **Ruby readings are not counted.** `<rt>` carries `data-aside` on `main` (#253), so `wordNodes` and `selectionIn` already skip it. The characters sent to `/at` are taken from `block.text` at the offset, never from the DOM, so readings, page breaks and columns don't matter.
3. **Units.** The client sends at most 16 **code points** (so a surrogate pair is never cut), and the server caps and counts in code points. The route's `length` is in UTF-16 units, converted on the server, so the client adds it straight to its UTF-16 offset.
4. **Prefixes.**
   - A prefix stops before the first character `ends_word` names (see "Built differently"). ー and 々 are letters (`Lm`), so they continue it. 、。「」・ count as punctuation.
   - A prefix whose `key()` differs from its own lower case (a stress mark inside it, say) is skipped.
   - The match starts at the tapped character, as Yomitan does. Tapping inside a word looks up from there; to look up the whole word, tap its first character.
   - Single-character matches are allowed (a particle like は is a word).
5. **The button is always shown.** A look-up for a language with no dictionary says so, as today. No list of dictionaries is needed.
6. **A tap while a look-up is open** replaces it, and the selection set for the old word is cleared. Closing the panel clears the selection only if it is still the one the tap set, so a selection the reader made is left alone. A drag-select with the mode on opens the selection bar as today and closes an open look-up.
7. **Double tap.** The look-up tap is recorded as the last tap. A second tap within `DOUBLE_TAP_MS` on the text, or anywhere near the first, closes the look-up, clears its selection, and peeks, as the existing pattern undoes a tap's action before peeking. A tap on a control away from the first is that control's.
8. **Paged layout** works through the caret-from-point API, which handles columns. The 16 characters come from `block.text`, so a page boundary doesn't matter. Vertical text (project 2) is out of scope here.

## Calls made for the user (may be overruled)
- The key is `l`, and the button label is "Look up words".
- The mode is remembered per browser and off by default.
- Japanese searches at most 16 characters forward.
- Spanish and Russian words are runs of letters with inner apostrophes and hyphens.
- The light is the native selection.

## Left to the builder
- The exact caret-from-point fallback where neither API is available (jsdom). Tests drive the offset directly.
- The button's place in the bars and its icon.
- How a tap during an in-flight look-up cancels the earlier one.

## Built differently

Where this section and the design above disagree, this section is what was built.

- **The route** is `GET /api/dictionary/{lang}/at/{text}`, the text a path segment and not `?text=`. A route ending in `/at` would take the word "at" from the existing `/api/dictionary/{lang}/{word}`.
- **Paged text:** a tap in either outer third turns the page, and only the middle third looks up (`pageTurnedBy`). A word in an outer third is looked up only by selecting it (O361).
- **The prefix rule:** a prefix stops before a character `ends_word` names, one that is not alphanumeric, a diacritic (U+0300 to U+036F), a kana voicing mark or a variation selector. That is whitespace, punctuation and non-alphabetic symbols, and also other non-alphabetic marks and format characters such as a zero-width space. The reader's `beginsWord` keeps to the same rule.
- **The length** is not always the prefix's own: it takes in the variation selectors and the stress marks U+0300 and U+0301 after the word, and a prefix that another diacritic or voicing mark follows is not tried. `key()` is unchanged, so a selector or stress mark inside a word answers the shorter word before it (O362).
- **A tap on a character:** a tap counts only where it is on a character, within its box along the line and, across the line, in its glyph or the leading beside it, read along the text's own axes, so vertical text is covered, and a tap on a ruby's reading counts as its base. A tap on a character's far half counts as that character, and a tap on no character keeps its usual meaning.
- **The Japanese test** is `isJapanese`: a language tag of `ja` or `jpn`, or, with no tag or `und`, a text more than half kana and kanji with some kana. A tap on such an edition always asks the `ja` dictionary.
- **The lit word stays in sight:** where the text set in beside a docked panel, or an answer that lengthens the word, would leave the lit word out of sight, the least scroll or page turn brings it back (`bringIntoSight`). This is the one exception to the rule that opening or closing a docked panel leaves the reader's place identical.
- **The phone panel:** at phone size the look-up panel opens on the side of the screen away from the word (`seatLookup`), no taller than 45% of the screen, its body scrolling inside it. It is untested on a real iPhone (O366).
- **The select-then-Look up path changed too:** Save word after a swap keeps its note on the edition the word was looked up in. Unless both ends of the selection have already left the screen, the word looked up is kept in sight and at phone size the panel opens away from the word.
