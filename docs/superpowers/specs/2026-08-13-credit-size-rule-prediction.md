# Pre-registered prediction, 2026-08-13, before any page was viewed

Rule under test: a page is a credits card iff its pixel area is under half the
volume's median page area AND its aspect differs from the volume's median by
more than 15%. Scored 25/25 precision and 25/25 recall on seven editions.

## The prediction

| edition | group | area flags | aspect flags | BOTH | predicted cards |
| --- | --- | --: | --: | --: | --: |
| kaguya-v03    | Psylocke Scans | 0 | 1 | 0 | **none** |
| kaguya-v08    | /a/nonymous, Jaimini's box & Fans Scans, Psylocke Scans | 0 | 0 | 0 | **none** |
| kobayashi-v08 | TSHH | 0 | 1 | 0 | **none** |

## Why this prediction is weaker than it looks, stated in advance

Predicting nothing is cheap to satisfy. Two of the three have a prior that
makes it likely correct for reasons unrelated to the rule: Psylocke Scans
printed no card in kaguya-v01, earned three ways there, so kaguya-v03 and v08
being empty would confirm a group habit rather than the rule.

**kobayashi-v08 is the real test.** TSHH is a group nobody has measured, and it
is a different group from the Cirno9baka that printed ten cards in v01 and ten
in v03 of the same series.

## The failure mode I expect if this is wrong

All three volumes are low resolution: median page area 1.04, 1.07 and 1.00 MPx
against kobayashi-v01's 3.18 and v03's 1.80. Cirno9baka's cards run about 0.72
MPx. In a 1.00 MPx volume a 0.72 MPx card is 0.72x the median, which is ABOVE
the 0.5 cut, so the area half of the rule cannot fire no matter what the page
is. The conjunction inherits that blindness.

So if TSHH does print cards, this rule misses them, and what we will have
learned is that it fails on low-resolution releases - which is a large share of
scanlation output and would matter more than the 25/25 does.

## How it will be checked

Not by the rule. Every chapter-final page of all three editions goes into the
contact sheet regardless of what the rule says, because that is where all 25
known cards so far have sat. If a card is there and unflagged, the rule missed
it and the failure is recorded here in advance.
