# Content matching: the volume-against-volume test (O340)

## Why

The owner wants raw and fan editions to join their English volumes by page content, with confident matches applied automatically. PR 1's volume rule failed on held-out because two different fan-extra chapters lined up page for page. The owner ruled on 2026-11-17 that a short new measurement comes before PR 3: a rule limited to volume-against-volume pairs, its bar frozen on the series already scored and tested on series never scored by this measurement. PR 3 compares only volumes, so this is the rule it would use.

## Goals

- Decide whether PR 3 may apply a volume match automatically, and at what bar.
- Report how many true pairs clear that bar, by kind of edition.

## Non-goals

- No product change. PR 3's matching is designed after this result.
- No chapters, packs or omnibus editions. Whether pack cuts go automatic stays the owner's decision once this result lands.
- No new score. V stays as PR 1 defined it: long-run pages over the longer side's pages.
- No use of names or the solver's verdict to rescue a pair.

## Anti-goals

- A bar chosen or adjusted after any test page has been read.
- A pass that depends on leaving a hard wrong pair out of the test.

## Approaches

1. **Extend PR 1's harness (chosen).** It already fingerprints, scores and records a frozen bar with proof of order.
2. A throwaway script. Rejected: it leaves no record that the bar came before the scores.
3. Score from the server's stored fingerprints. Rejected: it needs PR 3's matching first, and this test gates PR 3.

## Design

### The bar

The rule covers pairs whose two members are both volumes. Its bar is the highest V of any wrong volume-against-volume pair in PR 1's dev and held-out scores, plus PR 1's gap of 0.15. A preview of those scores gives 0.72 + 0.15 = 0.87. The harness computes the bar and commits it in a new bars file before this measurement fingerprints any test series. PR 1's `bars.json` and `manifest.json` do not change.

### The test series

Every series below is new to the measurement, copied from the NAS into `/home/davey/temp/manga-test` under its NAS path. The test has 157 volumes and scores all 12,246 pairs.

| Series | Editions | True pairs |
| --- | --- | --- |
| Tongari Boushi no Atelier | raw 14, English 14 | v01 to v13 |
| RuriDragon | raw 4, English 3 | v01 to v03 |
| Hunter × Hunter | black and white 36, colour 37 | v01 to v36 |
| Parasyte | black and white 8, Full Color Collection 8, Colored Council 10 | black and white against the Collection, v01 to v08 |
| Yu Yu Hakusho | black and white 19, Colored Council 4 | none |

RuriDragon's raws are RAR5, which taiyaku refuses, so the test copies are repacked as zip. Every page image was checked identical to the original's.

Tongari stays in the test with its earlier work disclosed (owner's ruling, 2026-11-18). Its raw and English v01 and v09 went through the aligner in early project work (`docs/notes.md`, "Tongari confirms the pattern on a fifth series", "A letterbox has to come off both sides equally", and the rows `tongari-v01` and `tongari-v09-letterbox`), and the rule that trims page borders was designed on v09. An early survey also measured volume 13 for gatefolds (`docs/notes.md`, "No gatefolds exist in anything measured"). V was never computed on these volumes, and the bar comes only from PR 1's scores. The pre-registration and the result both say so, and the report shows Tongari's true pairs on their own line.

### Labels

- **True pair:** the same volume number in the editions the table names, 60 in all.
- **Damaged:** Hunter × Hunter's colour v35 (48 pages against 204). It is outside pass and fail and is reported separately, which leaves 59 true pairs that count. Tongari's raw v09 is a true pair (owner's ruling, 2026-11-18): its 90 pages against 172 are joined two-page spreads, so the volume is complete.
- **Different split (owner's ruling, 2026-11-18):** every pair of a Colored Council Parasyte volume with either 8-volume edition, and of a Colored Council Yu Yu Hakusho volume with the black and white edition, 236 pairs. These are wrong pairs even where the numbers agree.
- **Wrong pair:** every other pair.

### Pass or fail

The rule passes if no wrong pair reaches the frozen bar. Any wrong pair at or above the bar goes to the owner one pair at a time, with its pages beside it. A relabel is counted and reported, and the bar never moves.

If the rule passes, PR 3 may apply volume matches at or above the bar automatically. If it fails, every PR 3 match is a suggestion.

### What the PR delivers

The harness changes, the frozen bar, the result in `docs/notes.md`, and O340 updated in `docs/status.md`.

## Left to the builder

- How the test manifest is drafted and how the new split is named.
- Where the run's evidence is kept, under `.superpowers/sdd/content-matching-o340-evidence/`.
