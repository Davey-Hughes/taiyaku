# The book reader keeps the reader's place: compare with a panel, and the first search

Approved by the owner on 2026-11-18.

## Why

Four book reader defects move a reader away from the text they were reading.

1. Compared and paged, opening a docked panel can show a page that does not hold the text being read. Closing the panel brings it back.
2. The first search after opening a book goes to the book's first match, or going back its last, however far in the reader is, and saves that as their place.
3. A window resize while a book opens can replace the saved place with the start of the book, and that start is then saved.
4. Reopening a paged book can walk the saved place back, a page or more at a time, with no reading between.

## Goals

- Compared and paged, on a page turned or scrolled to, a docked panel keeps the first character of the column read on the page shown, opened and closed any number of times, and closing it restores the exact page. That includes a page whose row opens with a figure.
- Compared and paged, a book just opened shows the first words of the saved block in the column read. A docked panel keeps those words on the page shown, and closing it restores the page the book opened on.
- A window resize while a book opens never replaces the saved place.
- Paged, in every reading mode, reopening a book left with no panel set in beside the text returns to the page it was left on, and opening and closing it any number of times moves neither the page nor the first character shown. A saved place names a block, so a page no block starts on reopens on the first page of the block carried onto it, and holds there. A page holding only a figure names no block of its own, so it reopens on the first page of the block after the figure, or, where the figure ends the book or follows a block of the column read that opened earlier in its compared row, on the first page of the last block to open before it, and holds there.
- Compared and paged, on a page turned or scrolled to that a row carries over onto, an image loading keeps the first character of the column read on the page shown, in every layout.
- The first Enter or `]` after opening goes to the first match at or after the reader's place. The first Shift+Enter or `[` goes to the last match before it. Both go round the ends, as later searches do.
- While the match last gone to is on screen in the edition read, Enter, Shift+Enter, `]` and `[` step on from it. Once the reader has moved away from it, they step from the reader's place, as the first search does. A match gone to stays the one stepped on from until a hand move or a navigation, even where a figure pushed its words off the page.

## Non-goals

- Reached through a search match, a panel keeps the match in view rather than the first character of the page.
- How a panel or an image places the text is not changed on a page reached outside compare, for a docked panel opened or closed in the phone layout, where it goes over the text, or in vertical text. Compare is never vertical.
- A change of text size or window size, compared, on a page turned to, still goes back to the start of the row, as it does now. A window resize on a book just opened places the saved block again, so a block a row opens with goes back to the start of the row, onto a page a figure fills where a figure opens the row.

## Approaches

**Compare with a panel.**

- **Chosen: remember the character.** On a page turned or scrolled to that a row carries over onto, the anchor also holds the first character of the column read, and placing goes to the page holding it. Where the page holds only the other column's words, placing falls back to today's fraction of the row.
- **Added on the owner's ruling, 2026-11-18: every page, and a book just opened.** The character is taken on any compared page that holds one of the column read, after a placing as well as after a scroll, except a placing at a search match.
- Rejected: let the panel cover the text in compare. It is smaller, but a match stepped to could sit under the panel, which the search design rules out.

**First search.**

- **Chosen: use the rule later searches already use.** `stepFrom` asks for a place whenever the match given is not listed, including when no match has been gone to.
- Rejected: move the reader's place into a separate first-search path. It would be a second rule for the same question.

**Stepping after reading on (owner's ruling, 2026-11-18).**

- **Chosen: the match while it is on screen, the place once it is not.** Several matches on one page can still be walked through, and a reader who has read on steps from where they are, not from an old match.
- **Added in the build: the hold.** Going to a match holds it until a hand move or a navigation, such as the contents, a link, Back or a note, ends the hold, so a match a figure pushed off its page is still stepped on from. The text laid out again where it stands, by a text size, a setting, a window size, an image or a docked panel's set-in, keeps the hold, whatever page it then shows.
- Rejected: always step on from the last match, which took a reader who had read on back to the match next to it and saved that as their place.
- Rejected: always step from the place, which would stop on the same match again when the page starts before it.

**Reopening (owner's ruling, 2026-11-18).**

- **Chosen: save the block that reopens on this page.** Paged, the block saved is the first whose first line is on the page showing. Where none is, it is the top block, with two exceptions compared: the block of the column read carried onto the page, and on a page of a row holding none of that column, the last of that column's blocks to open before the page in that row, where one has. A match gone to, Back, a link, a note, a text size step and a window made shorter save the same block, whether or not they turned the page.
- Rejected: skip the save during a placing, or save the placed block during one. Either stops the walk on an idle open but still reopens a page early after reading.

## Left to the builder

The names of private helpers, and how the character lookup shares code with `rowWords` in `bookPages.ts`.
