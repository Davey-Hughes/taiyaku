# Wave 2 prediction, 2026-08-13, before any page was viewed

Three series with no raw anywhere on the NAS, so the fan edition is aligned
against **official English** rather than `raw-mono`. They live under
`corpus/incoming/fan-vs-official/` rather than as settled works: `corpus/README.md` says a
settled directory holds one volume in every edition that exists for it with
`raw-mono` as the hub, and a work with no raw cannot serve the zero-drift
baseline that makes that useful.

Three new groups, and the first test of the size rule outside the raw-hub
convention. The size rule reads no alignment at all, so the hub change cannot
affect it; it can only affect signal 1.

## The prediction

| edition | pages | median | signal 1 | size rule (area AND aspect) |
| --- | --: | --- | --- | --- |
| `dungeon-meshi-v01` | 187 | 2.83 MPx | pages 40, 65 | pages 40, 65 — **agree** |
| `frieren-v01` | 181 | 1.78 MPx | none | none — **agree** |
| `domestic-v01` | 190 | 1.13 MPx | none | **pages 2, 54, 95, 130, 166** |

**Predicted: pages 40 and 65 of Dungeon Meshi and pages 2, 54, 95, 130 and 166
of Domestic na Kanojo are scanlation credit pages; Frieren carries none.**

## The disagreement is the point

`domestic-v01` is the first edition where the two rules give different answers.
Signal 1 finds nothing — its five unpaired runs are all either longer than 1 or
not at a member's last index — while the size rule proposes five pages.

- If those five are credit pages, the size rule catches five that signal 1
  structurally cannot, and that is the largest single divergence measured.
- If they are not, the size rule takes five false positives in one edition and
  its 25/25 precision falls to 25/30 at best.

There is no reading on which both rules are right.

## Two things worth noting in advance

**Page 2 is not at a chapter end.** Every one of the 25 known credit pages so
far sits at or near a member's last index. A card at a member's *second* page
would be a position nothing has measured, and `UnpairedRun`'s own doc records
that the first-index version of signal 1 was zero for four on Go-Toubun — what
accumulates at a member's start is front matter. So page 2 is the likeliest of
the five to be a false positive.

**`domestic-v01` is low resolution at 1.13 MPx median**, which is the regime the
previous pre-registration named as the size rule's blind spot: a 0.72 MPx card
would sit above the half-median cut there. That the area test fires at all means
these five are well under 0.565 MPx, so this edition does not test that blind
spot — it is still unexercised.

## How it will be checked

Every page either rule flags, plus every chapter-final page of all three
editions, adjudicated by extraction. Chapter-finals are included regardless of
what the rules say, because that is where every known card so far has sat and it
is the only way a shared miss would surface.
