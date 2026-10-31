# Code history moved out of comments

Text moved verbatim out of code comments when they were trimmed to their why. Each entry names the file and the line of code the comment sat above.

## `web/tests/App.browser.test.tsx`, in `describe('O112: the note list under the page, in a browser'`, above `ports.getNotes.mockResolvedValue({`

Measured in Chromium: at eight notes the
list stood 227 px tall at every size in `SIZES`, and at 1440×900 —
where the bar is 211 px, from the 176 px strip, the tallest of the
three — no row cleared its top edge at all, so "no row takes a click
meant for them" could not fail there: every row was already
unreachable for a reason this test was not checking. (At 768×1024 and
390×844, where the bar is 147 px, two rows already cleared it, so
those sizes were already testing the click half.) Nine notes (254 px)
clears the bar everywhere: one row at 1440×900, three at 768×1024 and
three at 390×844.

## `web/tests/App.browser.test.tsx`, above `const PAGES = 6`

This fixture was
first written `rtl`, and a mutation docking the button over the strip
passed every test below; read `ltr`, the same mutation reports `page 1`
onward as covered.

## `web/tests/App.test.tsx`, above `it('signing out unmounts the reader and leaves no write armed behind it', async () => {`

`App` used to answer
sign-out with `forgetTheReader`, a callback clearing thirteen pieces of
state by hand and cancelling two debounced writers; it grew to that because
each of three consecutive whole-branch reviews found more it had missed —
the second one field, unsaved note bodies drawn to the next account and
then `PUT` under their session; the third three at once, among them a
pending correction whose Confirm wrote the previous reader's pairing into
the shared sidecar.

## `web/tests/Filmstrip.test.tsx`, above `describe('re-anchoring on the rendered window running out', () => {`

a count of tiles the focus has drifted (a drift-based rule was replaced: it
assumed the viewport straddles the focus tile, when the focus is the rail's
leading edge and the viewport extends forward from it, which cost a 321 px
gap at 1440 px), and not on the first/last *rendered tile's* box either —
an earlier version of this fix tried that, checking both ends of the DOM
order against both ends of the rail on the reasoning that checking both
made it agnostic to reading direction. It did not: the rail is
`row-reverse` under `rtl`, so the last tile in the DOM sits at the left of
the screen, not the right, and checking both ends of the DOM order makes
one of the two checks true on nearly every frame instead. Measured driving
a 1.2 s flick at 390×844 under 4× CPU throttle: 71 re-anchors across 182
frames, worse than the 65 the drift-based rule produced and worse than the
24 the spacer rule below achieves.

## `web/tests/Labeller.browser.test.tsx`, above `describe('a resting box is visible without being touched', () => {`

The first version of this
styling was a 1px hairline at `opacity: 0.15`, which a labelling session
reported as "impossible to see" — invisible over halftone artwork rather
than merely quiet.

## `web/tests/Proposals.browser.test.tsx`, above `const first = listed[0]`

Measured: the list is 828px and the title column 506.6px of it, with the
web fonts loaded.

## `web/tests/Proposals.browser.test.tsx`, above `expect(boxOf(first, '.what')).toBeGreaterThan(300)`

measured
326.6px once the web fonts have loaded

## `web/tests/RuleList.test.tsx`, above `it('draws no cost warning at exactly forty rules', () => {`

(10 extra rules 47
ms, 40 -> 401 ms, 80 -> 1.34 s, 160 -> 5.01 s, in a debug build on a
one-work fixture)

## `web/tests/Settings.test.tsx`, in `it('does not close when a touch release retargets to the scrim over the panel'`, above `const onClose = vi.fn()`

Measured in Chromium at 390×844 with `hasTouch: true,
isMobile: true`: an 8 px sweep,

## `web/tests/RuleForm.browser.test.tsx`, above `host.style.width = '777px'`

777px is what `.editor-right` measured at a 1440px window
when this was found.

## `web/tests/pairing.test.ts`, above `expect(groups(bSlots, 'two', 'rtl', [3, 5])).toEqual([`

(The review's suggested [3, 6] was tried first: 6 turns out to land
on a position the walk never revisits as a "current" index once the
spread has put it back on even parity, so it has no effect at all —
groups(bSlots, 'two', 'rtl', [3, 6]) equals the no-breaks case
exactly, which would pass even if breaks were entirely unwired. 5 is
used instead so the test actually proves the mechanism does
something.)
