# Wave 3 prediction, 2026-08-13, before any page was viewed

`solo-leveling-v01` — fan chapters c001-c003 of a Korean webtoon against Yen
Press's paginated volume 1. Ingested as the deliberate stress case: the design
expected full colour to break the colour corroborator, which was already dead.
What it actually stresses is something else.

## Two things already measured, before any adjudication

**The alignment is REFUSED.** 1 run, **0 whole pairings**, 321 unmatched slots —
`taiyaku align` correctly refuses, because the fan edition is 61 vertical scroll
strips and the official is 314 print pages. A strip carries what the print
edition spreads over several pages, so there is no page correspondence to find.

**This is therefore the first edition on which signal 1 cannot work at all.** It
reads unpaired runs of an alignment, and there is no usable alignment. Not a
miss — an absence of the input it needs.

## The prediction

**The size rule finds nothing: zero cards.**

Area flags **0 of 61 pages**. On a webtoon the strips are tall, so the median
page area is 2.39 MPx, and the two landscape formats that recur at every
chapter boundary — 1778x1000 at each chapter's first page, 1575x1050 at each
chapter's last — sit at 0.74x and 0.69x of that median. Both are above the 0.5x
cut, so the area half cannot fire. Aspect flags 27 of 61, including all six of
those boundary pages, but the conjunction needs both.

## What would falsify it, and I expect it to

Pages 20, 39 and 60 are each their chapter's last page, each exactly 1575x1050,
identical across all three chapters. That is the signature every confirmed card
in this corpus has: one size, repeated at a chapter boundary. **If those three
are credit cards, the size rule misses all three and its 32/32 becomes 32/35.**

If so, this is the blind spot the wave-1 pre-registration named — "a 0.72 MPx
card sits above the half-median cut" — arriving from the opposite direction. It
was described there as a low-resolution failure. It is really a **relative**
one: what matters is the card's size against the median page, and a format with
unusually *large* pages defeats the rule exactly as a format with unusually
small ones does. Tall scroll strips make the median large.

Pages 0, 21 and 40 are each their chapter's first page at 1778x1000, and are
more likely title banners than credits.

## How it will be checked

Every chapter-first and chapter-last page, plus a sample of the aspect-only
flags, adjudicated by extraction. Signal 1 contributes nothing here.
