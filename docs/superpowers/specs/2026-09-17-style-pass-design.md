# The style pass — design

**Status: settled in brainstorming on 2026-09-17. ① merged as #162
(`c70413e`). ②'s details settled on 2026-09-18, in §7.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

The user asked on 2026-09-15 whether anything blocked a style/UI pass so they
could start *using* taiyaku. Three use-blockers were picked and landed first —
O89 (#159), O111 (#160) and O90 (#161). This is the pass itself. It settles
**what taiyaku looks like, how that look is built, and the five PRs that apply
it**. It does not change what any screen does, with one exception: touch input
for the reader, which is behaviour and gets its own PR, ④ in §5.

The mockups the decisions below were made against live in the brainstorming
session's `.superpowers/brainstorm/` directory, which is gitignored. They used
real covers and a real page from a scratch copy of the NAS library, served by
a debug build of `dbd47a3`.

## 0. What exists

Measured on `dbd47a3`, 2026-09-17.

**No design system.** `web/src/index.css` is 1,693 lines of global classes
for the library, proposals and configuration screens, built on CSS system
colours under `:root { color-scheme: light dark }` — 79 system-colour keywords
outside comments. The reader, its panels, the account overlay and the login
screen are styled instead through `CSSProperties` objects in 12 components.
Outside comments and tests there are **94 colour literals** — 11 hex and 83
`rgb()`/`rgba()` — in 14 files: 73 across 12 `.tsx` components, 15 in
`palette.ts` and 6 in `index.css`. There are about a dozen distinct font sizes
(`0.7rem`, `0.75rem`, `0.8rem`, `0.85rem`, `0.85em`, `0.9rem`, `0.9em`, `11`,
…). The only named colours are the four `--box-*` properties of the annotation
palette, and the one font is `system-ui`. *A first count, 93, came from a
pattern that also matched PR numbers such as `#115` inside comments; the
figures here strip comments first, and so must the test in §4.*

**A phone cannot use it.** At 390×844, `.library`'s grid keeps its `14rem`
(224 px) sidebar column, more than half the screen, the filter chips wrap one
per line, and the page scrolls sideways.

**The reader takes no touch input.** It listens for `keydown` on `window`,
`wheel` on the page area (zoom) and `load`; `App.tsx`'s pointer handlers are
region drawing's. Its chrome — the strip, the readout and the `Library` button
— is shown only in answer to a key: `Tab` (`keyplan.ts`'s `toggle-chrome`), or
the key that begins a correction, whose `beginCorrection` is the other caller
of `setChromeVisible`. Pages turn only by key. So on a phone, single-page
and two-page modes show one spread with no way to turn it or open the chrome.
**Scroll mode is the exception**: its pages sit in an `overflow-y: auto`
container (807 px tall over 2,249 px of content at 390×844, `touch-action:
auto`), so a touch drag should scroll it. That was read from computed style
in Chromium, not tried on a device. Even there, leaving the reader needs the
chrome.

**The tests mostly pin geometry, not colour.** 15 test files make 133 style
assertions (`.style.*`, `toHaveStyle`, `getComputedStyle`). Most of them are
width, left, height, bottom and z-index — `Annotations.test.tsx` alone has 45,
`Chrome.test.tsx` pins O65's dock. Colour assertions are about 18, in four
files: `Filmstrip.test.tsx` `borderColor` ×5; `Annotations.test.tsx`
`background` ×4, two of them `palette.ts`'s note tints; `NoteList.test.tsx`
`background` ×2, both note tabs from `palette.ts`; and
`AcceptForm.browser.test.tsx`, which compares text and background colours to
the body's and probes the system colours under each `color-scheme` — in the
browser project, which CI does not run. `index.css.test.ts` recomputes
WCAG contrast from `index.css`'s text against Chromium's measured system
colours, because jsdom cannot render it.

## 1. Decisions

Each was put to the user as options, with mockups where the choice was
visual. The middle column is what was chosen.

| Question | Chosen | Beat, and why |
| --- | --- | --- |
| What the pass delivers | **A look and a system**: tokens first, then screens one PR at a time | *Fix rough edges* — faster, but leaves 94 colour literals and a dozen sizes to drift further. *Reader only* — leaves the library, the first screen, as it is |
| Devices | **Phone too**: narrow widths are first-class, library and admin included | *Desktop*, *iPad*, *both* — each would have let the phone layout stay an afterthought |
| Visual direction | **Ink and paper**: warm paper or sumi-ink ground, mincho titles, one vermilion accent, hairline rules for boxes | *Quiet shelf* — neutral and familiar, rounded covers; *Screentone* — hard black and white, poster type, halftone, which gets heavy on configuration screens |
| Theme | **Follows the OS, with a System / Paper / Ink switch** per device | *OS only* — least code, no override; *always ink* or *always paper* — one palette to tune, the other never shown |
| Phone navigation | **A drawer**: ☰ slides in today's sidebar — nav, roots, browse settings — unchanged in content | *Bottom tabs* — one tap to anywhere, but gives up about 50 px in the mockup, has to hide in the reader, and moves browse settings into a new sheet |
| Behind the reader's page | **Pure black** (`#000`) in both themes | *Follows the theme* — paper behind a white page loses its edge; *always ink* — warm, but the letterbox stays visible on an OLED phone |
| Touch input | **Its own PR, after the reader restyle** | *Folded into the restyle* — one review mixing visual and behaviour; *desktop-only reader* — reading on a phone left to Panels; *filed for later* |
| Order after the foundation | **Library → reader → touch → admin** | *Reader → touch → library → admin* — phone reading soonest, but the library stays broken on a phone until the fourth PR; *library → admin → reader → touch* |
| How it is built | **Plain CSS**: custom properties, `light-dark()`, native nesting, `@import` partials | see below |

**Plain CSS over SCSS was measured, not preferred.** The user asked for SCSS.
Biome 2.5.7, run with this repository's `biome.jsonc` over one file saved as
both `.css` and `.scss` — an unknown property, a duplicated property and an
empty block — reported `noUnknownProperty`, `noDuplicateProperties` and
`noEmptyBlock` on the `.css`, and nothing on the `.scss` — its summary read
`Checked 1 file` for the two: **it skips `.scss` silently**. `index.css` is
linted today through `pnpm -C web run lint`, which `scripts/check-all.sh`
runs, so SCSS would have removed that check without a failure to show it.
What SCSS would have added is mostly available in CSS now: `light-dark()`
writes each colour once for both themes, nesting is native, and Vite inlines
`@import`. What CSS still lacks is a breakpoint mixin, so
`@media (width < 900px)` is repeated by hand. *SCSS + stylelint* (three new
dev dependencies, formatting still unenforced) and *SCSS without lint* were
the alternatives. CSS Modules were rejected earlier for churn: every
`className`, and every test querying by class, would change. Tokens as a TypeScript object were rejected because
inline styles cannot express media queries, `:hover` or `:focus-visible`,
and the drawer needs all three.

## 2. Tokens

Every text colour clears WCAG AA, 4.5:1, on `bg`, `surface` and `field` in its
theme, and `line-strong` — the border of an input or button — clears 3:1 on
all three. The ratios were computed with the WCAG relative-luminance formula;
the figure given is against `bg`. `line` is for decorative rules only and
carries no contrast requirement.

| Token | Paper | ratio | Ink | ratio | Use |
| --- | --- | --- | --- | --- | --- |
| `bg` | `#f3eee3` | | `#15130f` | | page ground |
| `surface` | `#e9e2d2` | | `#211e18` | | buttons, chips, table heads |
| `field` | `#fbf8f2` | | `#1b1914` | | input backgrounds |
| `line` | `#d6ccb9` | | `#35312a` | | hairline rules |
| `line-strong` | `#897d68` | 3.49 | `#746c60` | 3.58 | control borders |
| `text` | `#231e17` | 14.30 | `#ece4d4` | 14.68 | body |
| `muted` | `#6b6151` | 5.25 | `#a69b87` | 6.77 | metadata |
| `accent` | `#b0361d` | 5.35 | `#e8715a` | 6.13 | links, active, focus |
| `on-accent` | `#fbf8f2` | 5.84 on accent | `#15130f` | 6.13 on accent | text on accent |
| `danger` | `#9e1b32` | 6.83 | `#f28b9a` | 7.89 | errors |
| `success` | `#3f6b2f` | 5.41 | `#95c283` | 9.12 | confirmations |
| `warning` | `#7d5500` | 5.72 | `#e2b55c` | 9.72 | unreadable, skipped |

The lowest ratios are on `surface` rather than `bg`: Paper's `muted` 4.71,
`accent` 4.80 and `success` 4.85, and `line-strong` 3.13 in Paper and 3.21
in Ink. *Ink's `line-strong` was `#6d655a` in the mockups shown to the user,
and measured 2.90:1 on `surface` once the check covered all three grounds
rather than `bg` alone; `#746c60` replaced it.*

**The reader is fixed**: `#000` behind the page in both themes; panels are
Ink's `bg` at 92% opacity with a hairline of Ink's `text` at 14%; text,
muted and accent are Ink's. **The colours drawn on artwork are unchanged**,
because artwork does not follow a theme, as `index.css`'s comment on the box
palette says: the `--box-*` properties move into `tokens.css` as their own
group, and `palette.ts`, the note colours, stays the one module that turns a
note's colour name into pixels.

**Type.** Shippori Mincho B1 (600, 700) for titles, headings and the wordmark;
Zen Kaku Gothic New (400, 500, 700) for everything else. Both are bundled from
`@fontsource`, never fetched from a CDN at runtime. Checked against
`@fontsource@5.3.0` on jsDelivr: Shippori Mincho B1's `600.css` declares 122
`@font-face` subsets by `unicode-range` and Zen Kaku Gothic New's `400.css`
121, and the Latin subset of Mincho 600 is 32,452 bytes, so a page of Latin
text downloads only the Latin files. Measured on `web/dist`, in apparent
bytes: before the fonts landed (at `948efbc`) the build held 604,783 bytes with
one 16,306-byte CSS asset; with them (`b42c095`) it held 34,294,658, with a
713.98 kB CSS asset (312.31 kB gzip) — the JS chunks unmoved by fonts, at
191.45 kB (`main`), 10.87 kB (`label`) and 215.92 kB (the shared chunk), gzip
58.04 kB, 4.29 kB and 68.92 kB, and the wasm binary's 170.46 kB unchanged. The
growth was fonts: 570 `.woff2` files (14,841,516 bytes) and 574 `.woff`
fallbacks (18,148,448 bytes), plus 693,791 bytes of `@font-face` CSS in that
build (608 rules; the CSS asset grew 697,682 bytes, the rest this PR's own
rules), since `@fontsource` splits each weight into around 121
`unicode-range` subsets. **The `.woff` fallbacks are then stripped from the
build**, as the user decided on 2026-09-18: every browser that parses
`light-dark()` loads woff2, so a PostCSS step, `web/src/stripWoff.ts`,
removes the `format('woff')` source from each `@font-face`. After it
(`b09ac9a`) the build holds 16,005,973 bytes — no `.woff` file, the same 570
`.woff2` — with a 573.60 kB CSS asset (243.22 kB gzip), 553,346 bytes of it
`@font-face` rules; those are the current figures. A Latin-only subset was
measured at roughly 18 kB of CSS, with Japanese falling back to the device
font, against 966,336 to 1,992,116 bytes per weight for a whole
Japanese-subset file; the user chose to keep
every subset regardless, on 2026-09-17. The scale is six steps — `2rem` h1,
`1.5rem` h2, `1.25rem` h3, `1rem` body, `0.875rem` UI, `0.75rem` metadata —
replacing every ad hoc size.

**Spacing** 4, 8, 12, 16, 24, 32, 48 px. **Corners** 0 on covers and panels,
2 px on buttons and inputs, fully round only on count badges. **Focus** a 2 px
`accent` outline at 2 px offset on `:focus-visible`. **Breakpoint** 900 px:
below it the drawer, at or above it the sidebar, so a phone and a portrait
iPad get the drawer and a landscape iPad and a desktop the sidebar.

**The system colours map one to one**: `Canvas` → `bg`, `CanvasText` →
`text`, `ButtonFace` → `surface`, `ButtonBorder` → `line-strong`, `GrayText`
→ `muted`, `Highlight` and `LinkText` → `accent`, `HighlightText` →
`on-accent`, `Field` → `field`.

Read with fontTools, not assumed: Zen Kaku Gothic New's Latin subset
(`@fontsource/zen-kaku-gothic-new@5.3.0`, `latin-400`) carries only the
`ccmp`, `frac`, `vert` and `vrt2` GSUB features, no `tnum`, and its digit
advances are 0: 504, 1: 383, 2: 445, 3: 484, 4: 493, 5: 468, 6: 493, 7: 412,
8: 509, 9: 475 units — no tabular figures. Tables therefore use
`--font-figures`: a `taiyaku figures` face that maps U+0030–0039 to a local
tabular sans (`Helvetica Neue`, `Roboto`, `Noto Sans`, `DejaVu Sans`, `Arial`)
and falls back to `--font-ui` for everything else. Measured in a `<table>`
inside `.library`, after `document.fonts.ready`: `1111111111` and `8888888888`
both come to 101.8 px. Outside a table the same two strings come to 61.28 px
and 81.45 px — the control that shows Zen Kaku Gothic New's own digits are
proportional, so the fallback face is doing the work rather than the first
measurement being vacuous. The only tables in the app are the series page's,
in its table layout, and the rule preview's two; configuration → roots is
fields in cards and has none, though this paragraph once called it the
densest configuration table. On the series table, read in Paper, the figures
face also draws the digits inside a volume's file name, heavier than the
title around them, because ① puts `--font-figures` on the whole `table`. ②
puts it on the numeric columns only (§5). Whether Zen Kaku's digits read
fine in the app's other tables — the rule preview's, the actual densest
configuration tables — is still unanswered: nobody opened a preview during
this pass. PR ⑤, which restyles that screen, owns the question (§5).

## 3. The theme switch

1. **Stored** under `localStorage` key `taiyaku.prefs.theme`, through
   `prefs.ts`'s `loadPrefs`/`savePrefs` like every other per-device
   preference. Those store a JSON object, so the value is one field holding
   `system`, `paper` or `ink`, and the default is `system`.
2. **Applied** as `data-theme` on `<html>`: absent for `system`, where
   `color-scheme: light dark` follows the OS; `paper` or `ink` pin
   `color-scheme` to `light` or `dark`. Every token is written once as
   `light-dark(<paper>, <ink>)`, so both routes share one set of values, and
   native scrollbars and pickers follow `color-scheme` without help.
   `light-dark()` needs Safari 17.5 or later (iOS 17.5), and **the web UI
   requires it**: the user accepted that on 2026-09-18. Below it every colour
   drawn from a token fails — simulated in Chromium, a transparent
   background, black text, no border, and a theme switch that does nothing —
   where the system colours before ① worked. Measured on ①'s library page:
   Paper keeps the paper ground and text with the OS emulated as dark; System
   follows an emulated dark OS to the ink values and a light one to paper.
3. **Before first paint** a short inline `<script>` in `index.html`'s `<head>`
   reads the key and sets `data-theme`, so a reader who picked Ink on a
   light-mode OS never sees a paper flash. There is no Content Security Policy
   today, so an inline script runs; a proxy that later adds a strict one
   breaks exactly this, and a comment beside it says so.
4. **The control** is a System / Paper / Ink segmented control at the foot of
   the sidebar, which the drawer shows too. The login screen honours the
   stored choice and has no control.
5. **The reader** sets `color-scheme: dark` on its own root, so native
   controls inside it are dark in both themes, as they are meant to be today.
   PR ③ does this (§5); ① does not, so with Paper chosen the reader's
   Settings panel and keyboard help draw light native controls inside their
   fixed-dark panels until ③.
6. **`index.html`** gains two `<meta name="theme-color">` tags, one per
   `prefers-color-scheme`, so Safari's toolbar matches the ground — the OS's
   ground, not a pinned choice. `viewport-fit=cover` moved to PR ③: without
   the safe-area padding ③ designs, it would put the library under a phone's
   notch in landscape.

## 4. Files, and the rules for moving into them

`index.css` becomes a list of `@import`s, which Vite inlines at build time.
`main.tsx` and `label.tsx` keep importing it as they do now.

| File | Holds | Its first rule in `index.css` at `dbd47a3` |
| --- | --- | --- |
| `styles/tokens.css` | `@fontsource` imports, colour tokens, type and spacing scales, the `--box-*` palette | the palette's `:root`, line 41; the rest new |
| `styles/base.css` | the `html`/`body`/`#root` height chain; buttons, inputs, selects, tables, links, headings, focus | `:root`, line 1; the rest new |
| `styles/annotations.css` | region boxes, the labeller legend, the region preview, the label page | `.region-box`, line 64 |
| `styles/shell.css` | the library frame, sidebar, browse settings; the drawer | `.library`, line 312 |
| `styles/library.css` | chips, the continue strip, the grid, volumes, cover boxes, editions, the edition selector, the alignment panel, search | `.chips`, line 399 |
| `styles/reader.css` | the reader root and its bars | none; the reader is inline today |
| `styles/admin.css` | proposals, configuration navigation, roots, the scan report, users, backup, account | `.proposals h1`, line 791, to the end at 1,693 |

Each group is contiguous in today's file. The split commit moves whole rules,
and the comment above a rule moves with it, so a group's text starts at its
comment rather than at the line given.

1. **The split changes nothing visible.** PR ①'s first commit only moves rules
   into partials, and is checked by building `web/` on `main` and on the
   commit and comparing the emitted CSS with whitespace normalised.
2. **Colour literals only shrink.** A node test scans the non-test files under
   `src/` — `.ts`, `.tsx`, `.css` — with comments stripped, for hex, `rgb()`,
   `rgba()` and system colour keywords, and compares the count per file
   against an allowlist. Two files are exempt by design: `styles/tokens.css`,
   where colours are defined, and `palette.ts`, whose note colours are drawn
   on artwork (§2). The allowlist starts at today's figures, so it passes on
   day one; a count above its entry fails, and a count below it fails too
   until the entry is lowered, so the allowlist cannot go stale. Each PR
   lowers the entries for its screens, and ⑤ ends with the allowlist empty.
   Biome has no rule for this, and without a test the literals would grow
   back. *An empty allowlist from ① was the first version of this rule, and
   it was wrong*: it would have made ① recolour the reader and admin screens
   ahead of their own PRs.
3. **Inline styles keep their geometry.** Position, size, z-index and
   transforms stay in their `style={{}}` objects, so the tests pinning them —
   O65's dock in `Chrome.test.tsx`, `Annotations`, `Labeller`, `GroupView` —
   are not touched. Only colour and type values inside become `var(--…)`.
4. **A screen moves to classes only in its own PR.** Inline objects that are
   mostly visual — `Account`, `Help`, `Settings`, `NotePanel` — become classes
   when the PR restyling that screen does it, never in passing.
5. **The contrast test reads tokens.** `index.css.test.ts` stops modelling
   Chromium's system colours and parses the `light-dark()` pairs in
   `tokens.css` instead. It gains the §2 floors in both themes — text tokens
   at 4.5:1 and `line-strong` at 3:1, each on `bg`, `surface` and `field` —
   and **keeps its three regression checks**, recomputed against the tokens
   rather than deleted: D2 (`.alignment-panel .paired` under its opacity), D3
   (a dimmed volume tile fades its cover, not its title or its `i` button)
   and the disabled Accept. None of the three can reproduce its shipped defect
   any more — each was measured against a system colour no rule uses now — so
   the token model earns trust instead by reproducing four ratios §2 computed
   outside the test. `vite.config.ts`'s `css.include` widens from
   `/index\.css/` to every `.css` file under `src/`. `Login.theme.test.ts`,
   which already refuses any colour in `Login.tsx` but its one `light-dark()`
   pair, is the precedent for rule 2 and gives way to it in ①, when the login
   screen takes the tokens.
6. **Colour assertions move with their screen.** Each colour assertion in §0
   that pins a themed colour changes in the PR that restyles its component,
   and is watched failing with the old value put back by hand before it is
   trusted. The ones pinning `palette.ts`'s note colours do not change.

## 5. The PRs

**① Foundation.** The split (§4 rule 1). Fonts, `tokens.css`, `base.css`.
The theme preference, the pre-paint script, the control in the existing
sidebar, and the `index.html` meta changes (§3). The colour-literal test and
the token-driven contrast test. Every browser-default control on every screen
takes the tokens, and the login screen is restyled. Every one inside
`.library` and `.login`, that is: `base.css` scopes its element rules there
with `:where()`, because the reader's inline-styled chrome leaves properties
to the browser that a document-wide rule would change under ③. And all 79
system-colour keywords become tokens in ①, one to one, since rule 5 recomputes
D2, D3 and the Accept against tokens. Four rules stay document-wide and so
reach the reader and the labeller — `:root`'s font, `body`'s background and
colour, and `::selection`. Measured rather than assumed: a bare `<button>`
does not inherit `:root`'s font, because the UA stylesheet sets one on
`button`, so the reader's layout buttons keep the platform's UA default —
`font-family: Arial` in Chromium on Linux, where it was measured, and not a
promise of Arial anywhere else; anything that opts in with `font: inherit` —
the Library button, the readout — takes the UI font and shifts width a
little. Accepted 2026-09-17 because ③ redraws that chrome. Measures the fonts'
weight in `web/dist`, and strips the `.woff` fallbacks from the build (§2).

**② Shell and library.** The drawer below 900 px: a ☰ button, focus kept
inside while open, `Escape` to close, `aria-expanded` on the button, the page
behind it not scrolling. The sidebar at 900 px and above. The library home,
the continue strip, the grid and the table, the series page and search. The
sideways scroll at 390 px is gone. Two things ① leaves to it, both on the
series table: **the figures face on the numeric columns only** — ① sets
`--font-figures` on every `table`, which draws the digits inside file names
too (§2), and the user left that to ② on 2026-09-18 — and `.editions
td.missing`'s "—", which at `opacity: 0.55` measures 3.60:1 on Paper's `bg`
(5.12:1 on Ink's; it was 4.76:1 on a white `Canvas`): ② checks it against the
3:1 non-text floor, or raises it, when it restyles that table. §7 has the
details, settled with the user on 2026-09-18.

**③ Reader.** The black ground; the top bar (back to the library, title and
edition, layout, settings) and the bottom bar (strip, progress, readout) in
ink, padded for safe areas; the settings, help, notes and credits panels.
**§3 item 5 is this PR's**: the reader sets `color-scheme: dark` on its own
root, which ① does not do. Measured in ①'s browser pass with Paper chosen, the
Settings panel and the keyboard help draw light native checkboxes, number
inputs and scrollbar inside their fixed-dark panels.
**O112 sits exactly here** — with the chrome open, the strip and the readout
row cover a note list placed below the page — and this PR redraws that bar,
so its options go back to the user once the bar's geometry exists, rather
than being decided silently in either direction.

**④ Touch.** A tap on the left or right third turns the page, mirrored for a
right-to-left edition; a tap in the middle shows or hides the chrome; a
horizontal swipe turns. None of it may take a gesture from region drawing,
wheel or pinch zoom, or panning. Verified on a real phone over the tailnet as
well as in the browser test project.

**⑤ Admin.** Proposals; configuration — rules, roots, users, backup, and the
scan report, including the long path that wraps inside its cell, which
`docs/notes.md` recorded during O90 as working and "left to the style pass";
the account screen. The colour-literal allowlist reaches empty.

**Then an accessibility pass**, after ⑤ — decided by the user on
2026-09-18, when ②'s device pass reached VoiceOver: "keep the accessibility
stuff as a pass at the end". It checks every screen the pass restyled with
VoiceOver on the phone and a screen reader on the desktop, including the
drawer's Close button and whether VoiceOver's escape gesture closes it,
rather than each PR checking its own screens alone.

**Every PR, before it is opened:**

1. `scripts/check-all.sh`, with its `all green` line quoted.
2. A browser pass against a scratch copy of the real library, at 390×844 and
   1440×900, in Paper and in Ink.
3. Every new test watched failing with its defect put back by hand.
4. A whole-branch review, and a review of each fix round until one finds
   nothing.
5. The pre-merge claim check over the PR body, `docs/status.md` and
   `docs/notes.md`.
6. Anything found and not fixed filed as its own open item.

## 6. Not in this pass

- **The labeller** (`label.html`). It shares `tokens.css` through `index.css`
  and is otherwise left as it is.
- **Anything that changes what a screen does**, except ④. A defect a PR finds
  on its screens is filed, or fixed only if it blocks that PR's own browser
  pass.
- **O63** (the table layout cannot reach alignment detail). It is a decision
  already taken, and ② restyles the table without reopening it.
- **Reading progress on the continue strip** — O132, filed by ②. The user
  wants it, behind a setting (§7.2), but it needs the server to send a page
  where it now stores a slot, so it follows the pass.
- **Blurry covers** — O133, filed by ②'s device pass. Every cover in the
  library is the reader's 160 px filmstrip thumbnail; the fix is the server's,
  and the user put it in its own PR straight after ②.

## 7. PR ② in detail

Settled with the user on 2026-09-18, before planning. What §5 ② already
says is not repeated here.

**Measured first**, on `c70413e`, with the scratch copy of the NAS library
served in Chromium at 390×844. The library still draws its 14 rem sidebar
at that width, with the chips wrapping one per line, as §0 found on
`dbd47a3`. With the sidebar column hidden by an injected rule and the grid
set to one column, a series page gave two volume columns of 155.5 px, and
an open alignment panel and metadata editor fitted inside their tile: the
volume grid needs a narrower frame, not a new layout.

### 7.1 The shell

**Chosen: one sidebar, which CSS moves off-canvas below 900 px.** The
alternatives were a native `<dialog>` opened with `showModal()` — the
browser traps focus, closes on Escape and draws the backdrop — and a second
copy of the sidebar inside a drawer. The dialog lost because the same
sidebar is the permanent column at 900 px and above, so its content would
have to move between the column and the dialog at every crossing of the
breakpoint. The copy lost outright: the theme control's radios share one
`name`, and two groups would uncheck each other. `inert` gives this
approach the focus containment the dialog would have, at the cost of the
focus and Escape handling written here — forty lines or so, a guess made
before writing them.

**The phone bar**, below 900 px only: ☰, the wordmark, and a search icon,
52 px tall, a `--line` hairline beneath, pinned with `position: sticky`.
☰ is a `<button>` labelled "Menu" with `aria-expanded` and `aria-controls`
naming the sidebar column. The search icon is a link to `/search` labelled
"Search", and the wordmark is a link home. The icons are inline SVG in
`currentColor`, so they add no colour literal.

**The wordmark** — 対訳 set vertically in the accent like a seal, then
*taiyaku* — becomes one `Wordmark` component. It was `Login.tsx`'s markup and
inline style objects before ②; the sign-in screen keeps its `<h2>` around the
component, the phone bar wraps it in its home link, and the sidebar draws it
at its top, unlinked, with the Library link just below.

**The drawer**, below 900 px, is the sidebar column, sliding in from the
left at `min(20rem, 85vw)` over a scrim. While it is open:

1. the column has `role="dialog"`, `aria-modal="true"` and the label "Menu";
2. the phone bar and `<main>` carry `inert`;
3. the document does not scroll;
4. focus moves to the column's first link.

It closes on Escape, on a tap on the scrim, on its own Close button, on
any navigation, and when the viewport widens to 900 px or more (a
`matchMedia` change listener, guarded, because jsdom has none). Focus then
returns to ☰ — except on widening, where ☰ is not drawn and focus stays in
the sidebar, measured in Chromium by the pre-merge claim check.

**The Close button** — a ✕ at the drawer's top, beside the wordmark, drawn
below 900 px only — was added on 2026-09-18 after the whole-branch review
found the modal had no way out inside it: the scrim sits outside the
`aria-modal` region and ☰ is inert, so under VoiceOver following a link
could have been the only exit. It is what the W3C's dialog guidance asks
for. Focus still opens on the first link, so the button is one Shift+Tab
away rather than the first thing announced. Closed, the column is
`visibility: hidden` as well as off-screen, so neither Tab nor a screen
reader reaches it. The slide takes 200 ms, and none under
`prefers-reduced-motion`.

**Below 900 px the document scrolls, not `<main>`.** At and above it,
`.library` keeps today's frame: a full-height grid whose `<main>` scrolls
beside the sidebar. On a phone, iOS Safari shrinks its toolbar, and scrolls
to the top on a tap on the status bar, only for the document's own scroll —
neither happens inside an inner scroller, which is the belief this rests on;
only the document-scroll half was tried on a device — so below 900 px `.library` takes
its content's height and the bar sticks. The drawer's scroll lock is then
`overflow: hidden` on the root while the drawer is open, and it holds on
iOS: measured on the user's iPhone on 2026-09-18, in Safari 26.6.1 (every
request's user-agent read), a drag on the scrim left the library behind the
open drawer still, the toolbar shrank as the library scrolled, and a tap on
the status bar scrolled back to the top.

**The sidebar, at both widths**, keeps its content — navigation, roots,
the library settings, the theme control:

- The wordmark at its top.
- Navigation links in `--text`, the current screen's in `--accent` —
  `NavLink`'s `aria-current="page"`; a series page marks nothing, because
  React Router's `NavLink` counts a longer path as inside `to` only where
  the next character is `/`, which after `/` it never is. *(This said
  "with `end` on `/`"; ② found `end` there a no-op in react-router 8.3.0
  and removed it.)* Root links stay plain `Link`s: `NavLink` compares the
  path, not the query, so every `/?root=N` would read as current.
- The section headings — Roots, Library, Theme — in the title face with a
  `--line` hairline running to the edge, as in the mockup the user chose,
  and coloured `--muted` rather than today's `opacity: 0.65`.
- The library settings' rows move from `BrowseSettings.tsx`'s inline
  `rowStyle` object to a class (§4 rule 4 — this is their PR). Their
  controls stay native `select`s and checkboxes, so a phone opens its own
  picker.
- Links and controls get 44 px rows under `(pointer: coarse)`.

**Proposals, Configuration and Account** sit inside the shell, so they get
the bar and the drawer in ②. What is inside them is ⑤'s, and a sideways
scroll inside one of them at 390 px stays until ⑤.

### 7.2 The screens

**The continue strip shows covers.** The mockup the user chose on
2026-09-17 showed one work with its cover, "Vol. 1 · page 8" and a progress
bar. The page has no page to show: `progress` holds a `slot` per edition
(`schema/001_initial.sql`, migration 009), and nothing turns a slot into a
page for the home route. Shown both on 2026-09-18, **the user wants both,
chosen by a setting, and covers first**: ② ships covers, and O132 carries
the progress version and the setting that picks between them — the
`continueStrip` checkbox becoming *Off / Covers / Progress*. Each card:

- a 104 px cover, `workCover(slug, opens)`, which is the thumbnail of the
  edition the reader will open, from data `/api/library/home` already sends;
- the title in the title face, at most two lines;
- "Vol. N" in `--muted` when `volume` is set;
- no link, and "files missing — cannot be opened" in `--warning`, when
  `unreadable(w)` — the card keeps its place, as O62's chip did before ②.

The row scrolls sideways under a "Continue" heading in the title face with a
hairline after it.

**The chips become underlined tabs**, as in the chosen mockup: `--muted`
text, the pressed one `--text` over a 2 px `--accent` underline. They stay
the same `aria-pressed` buttons. "Added" moves to the row's far end, because
it sorts where the others filter. **On the library's own row only**, by a
class of its own: `Proposals.tsx` draws its state and kind filters with the
same `chips` class, and they keep their pills until ⑤ restyles that screen.
Decided by the user on 2026-09-18, after the whole-branch review found the
tab look had reached the inbox, where "All" and "Hide weak proposals"
drew the accent underline side by side.

**The grid.** Covers take square corners (§2) and a `--line` hairline
instead of a rounded `--line-strong` border. Titles take the title face.
Under each title a new line in `--muted` gives the series' `works` count —
"42 volumes", "1 volume" — which `SeriesCard` already carries. A phone
gets two columns, three in compact. Works that belong to no series keep
their title-only tiles, restyled. The empty states, which had no CSS at
all before ②, take the title face for their headings and `--muted` for their
text — the proposals inbox's too, since it draws the same `EmptyState`
component, which is accepted rather than scoped away: muted prose under a
heading is not a restyle of that screen.

**The series page.** Its title stays an `<h1>` in the title face, and a
long one wraps rather than widening the page. Volume tiles take the grid's
cover treatment. The ↓ and `i` controls take 2 px corners and are at least
24 px square, 44 px under `(pointer: coarse)`. The alignment panel takes
square corners and a hairline.

**The series table** sits in its own box that scrolls sideways, so the page
never does. §5's two items:

- **The figures face on the page-count cells only.** `.editions` sets
  `--font-ui` on the table and `--font-figures` on those cells; `base.css`'s
  rule for every other table stays, because the rule preview's tables are
  ⑤'s question (§2).
- **`td.missing` loses its `opacity: 0.55`.** The "—" becomes `--muted`,
  5.25:1 on Paper's `bg` and 6.77:1 on Ink's (§2's table), clearing the text
  floor and not only the 3:1 one; the dashed border goes back to full
  `--line-strong`, 3.49:1 and 3.58:1.

**Search.** The field is full width on a phone and 24 rem at most above
900 px, on `--field` — `.search-box input` overrode `base.css`'s `--field`
with `--bg` before ②. Hits use the grid above.

**At 390 px the page never scrolls sideways** on `/`, `/series/:id` in
either layout, or `/search`.

### 7.3 Tests and checks

On top of §5's list for every PR:

- **jsdom.** ☰'s `aria-expanded` goes false to true and its
  `aria-controls` names the column; while open the column has
  `role="dialog"` and `aria-modal`, and the bar and `<main>` are `inert`;
  Escape, a scrim tap, the Close button, a navigation and a stubbed
  media-query change each close it; focus goes to the first link on open
  and back to ☰ on close.
  The continue card's cover `src` is `workCover(slug, opens)`, its
  "Vol. N" shows, and an unreadable one has no link. The grid's count reads
  "42 volumes" and "1 volume". The table sits in its scroll box, and only
  the page-count cells carry the figures class.
- **Node, over the CSS.** `.editions td.missing` has no `opacity`, its
  colour clears 4.5:1 and its border 3:1 in both themes; the sidebar
  headings take a token rather than opacity; the figures face reaches the
  page-count cells only. `base.css.test.ts`'s scope holds. The
  colour-literal allowlist has no entry to lower — the library's files are
  already at zero — and it fails any literal ② adds.
- **The browser project** (Chromium; the gate runs it, CI does not). At
  390×844 the page does not scroll sideways on the home, series grid, series
  table and search screens; the closed drawer is out of Tab's reach, the open
  one keeps Tab inside it, and the document does not scroll behind it; the
  open drawer sits at the left edge, above the scrim, and a tap beside it
  lands on the scrim and closes it. At 1440 there is no bar, no Close button,
  and the sidebar shows. *(The open drawer's place on screen was added after
  the whole-branch review: deleting its `transform: none`, the scrim's
  `display: block` or the column's `z-index` had failed no test.)*
- **The browser pass** covers the whole matrix — 390 and 1440, Paper and
  Ink, home, series grid and table, search, and the drawer open — and reads
  every screenshot.
- **The iPhone**, over the tailnet: the drawer opening and closing, the page
  not scrolling behind it, the toolbar shrinking on scroll, and tap sizes.
  The user-agent is read before any result is believed.
- **O132 is filed** in `docs/status.md`: the continue strip's progress
  version and the setting that picks between it and covers.
