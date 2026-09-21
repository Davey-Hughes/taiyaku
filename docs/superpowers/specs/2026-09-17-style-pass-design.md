# The style pass — design

**Status: settled in brainstorming on 2026-09-17. ① merged as #162
(`c70413e`), ② as #163 (`053ad8c`). ②'s details settled on 2026-09-18, in
§7; ③'s on 2026-09-19, in §8.**

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
than being decided silently in either direction. §8 has the details, O112's
answer included, settled with the user on 2026-09-19. *(O112 closed on
2026-09-19, with that answer built and measured — see its entry in
`docs/status.md`.)*

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

## 8. PR ③ in detail

Settled with the user on 2026-09-19, before planning, against mockups drawn
at 390×844 with a real page and real strip tiles from the scratch copy of the
NAS library (in the brainstorming session's gitignored directory). What §5 ③
already says is not repeated here.

**What exists**, read from `main` at `dbb68e4` for this section:

- The `single`, `two` and `scroll` buttons sit in flow above the page at all
  times — `App.tsx`'s toolbar row, outside `Chrome`. Measured in the
  browser project on `dbb68e4`, in Chromium with the fonts ① bundles: the
  row is 37 px tall and the page area below it 863 px at 1440×900 (987 at
  768×1024), so the row costs 37 px of every height-bound page.
  There is no layout key: the Settings panel's Layout section is the only
  other way to the same preference (`modules/layout.ts`).
- `Chrome` is the strip, floating 16 px in from each side at 176 px tall
  with rounded corners and a shadow, the readout row docked to the bottom
  edge beneath it, and `Library` docked in the band above the strip (O65).
- Help, Credits and the notes panel dock at `top: 16, right: 16`; the note
  list beside the page at `top: 8, right: 8` of the page area. Settings is a
  centred dialog that only its own key, `s`, closes: `keyPlan`'s `Escape`
  branch has no arm for it, and it has no Close button.
- The reader does not know the work's title or its editions' labels. It
  reads the sidecar through `SidecarShape`, which keeps `work.reading`,
  `hub`, `alignments` and the editions' keys, and not `work.title` or any
  edition's `label`.

**One mechanism, measured before it was chosen.** The tokens are
`light-dark()` pairs on unregistered custom properties, so a `light-dark()`
should be resolved where a property uses the token, against that element's
`color-scheme`, not where the token is defined. Measured with a page holding
the token rules, one paragraph outside a `color-scheme: dark` box and one
inside it, in Chromium 151 and Firefox 153, under Paper pinned and System,
with the OS emulated light and dark: the inside paragraph computed Ink's
text, `rgb(236, 228, 212)`, in all four combinations, and a `color-mix()`
of the `bg` pair at 92% computed Ink's; the outside paragraph followed the
theme. WebKit is not installed here. Safari is checked on the iPhone in ④'s
device pass, the first time a phone can open anything the reader draws in
the tokens: until ④ a phone cannot show the chrome.

### 8.1 Decisions

| Question | Chosen | Beat, and why |
| --- | --- | --- |
| Does the top bar come and go? | **With the bottom bar**: both are the chrome, drawn over the page, shown and hidden together | *The top bar stays*, today's shape restyled: layout, settings and the way out always one click away, but a 52 px band on screen at all times, taken off every height-bound page — most pages on a desktop, and every page on a phone held sideways (in portrait a page is width-bound, so there the band costs only the look) |
| How tall is the strip on a phone? | **96 px tiles below 900 px**, 160 at and above it | *160 everywhere*: one rule, and the size a manual pass asked for so a page already read is recognised. On the mockup's geometry the two bars cover about 27% of a 390×844 screen at 96 and 35% at 160, with about five tiles in view against three; *computed from the mockup, not measured in the app* |
| Where does a panel sit when the top bar shows? | **Always below the bar's line**, and above the bottom bar's, whether or not the chrome is shown | *At the top, stepping down while the bar shows*: uses the top 52 px while reading, but the panel jumps on every `Tab`, under a hand reaching for one of its rows. *Opening a panel hides the chrome*: a behaviour change, and a reader opening Help to find a key would lose the strip they were looking at |
| O112: the note list under the page, while the chrome is shown | **Hidden**, keeping its space | *Lifted onto the bottom bar*: stays readable and clickable, but with the list's 28% and both bars up to about 55% of a phone is covered, and while correcting the list and the hover preview would share the band above the strip. *Making room* and *floating it over the strip* are ruled out again for O112's reasons: the first resizes the page on every `Tab`, the second hides the tiles |

The first answer costs a pointer reader a step: changing layout takes `Tab`
first, or the Settings panel. It returns the toolbar's 37 px to the page.
The last one costs reading or opening a note while the strip is up; the
readout can still say how many there are, with its notes line switched on. What O112 named as a
cost of floating the list above the strip — the band where O65 docked
`Library` — is gone either way, because `Library` moves into the top bar.

### 8.2 The frame

- **A `.reader` class on the reader's root**, styled in
  `styles/reader.css`, the partial §4 reserved for it. The root's inline
  geometry — the flex column, `height: 100%`, `position: relative` —
  stays inline (§4 rule 3).
- **`color-scheme: dark` on `.reader`** (§3 item 5). By the measurement
  above, every token resolves to its Ink value inside the reader in both
  themes, so the bars and panels use the ordinary tokens — `--text`,
  `--muted`, `--accent`, `--line` — and there is no second palette. Native
  checkboxes, number inputs and scrollbars inside it are dark.
- **Three tokens of the reader's own**, in `tokens.css`: `--reader-ground`
  `#000`, `--reader-panel` Ink's `bg` at 92%, `--reader-hair` Ink's `text`
  at 14% — §2's values, not new ones. Over black, `--reader-panel` computes
  to about `rgb(19, 17, 14)`, on which Ink's text measures 14.86:1, muted
  6.85, accent 6.21 and warning 9.84; over white artwork, where a panel can
  also sit, it computes to about `rgb(40, 38, 34)`, and the four measure
  11.97, 5.52, 5.00 and 7.92 — every one above 4.5.
- **Black behind the reader.** `:root:has(.reader-route, .reader)` and its
  `body` arm take `--reader-ground` too, so overscroll and whatever shows
  under Safari's toolbar are black rather than paper. `.reader-route` is the
  box `Routes.tsx` wraps the reader in, so the ground is black from the first
  frame: `App` draws nothing until its load answers, and its own `.reader`
  root is not there until then. The user's iPhone confirmed it — a work opens
  black, with no flash of paper.
- **The page area keeps `scroll-padding-top` while the chrome is shown**, the
  top bar's height plus the top safe area, so a page scroll mode lands on
  stops below the bar rather than under it. It moves the scroll target only;
  the page area's own size is unchanged, which is what lets the bars overlay
  without resizing it. (A page already parked at its top when `Tab` arrives
  stays where it is — `useScrollToCurrent` is not keyed on the chrome — and
  scroll mode does not follow a page taken from the strip at all, which is
  O136 and predates this PR.)
- **`viewport-fit=cover`** in `index.html` (§3 item 6), with
  `env(safe-area-inset-*)` padding on the reader's two bars and panel
  docking, and on the library's shell: the phone bar, the drawer column and
  the sides of the frame. The library would otherwise sit under the notch in
  landscape, which is why §3 moved this to ③. *Built on `.library` itself
  rather than on `<main>` as this line first said: at and above 900 px that
  moves the sidebar off a landscape phone's notch too, and below it the
  sticky phone bar is inset with the frame, so its hairline stops short of
  both screen edges. The device pass found the library clear of the notch,
  the rounded corners and the home indicator in both orientations.*

### 8.3 The top bar

- **Left to right**: `‹ Library`; the title block; the layout control —
  three `aria-pressed` buttons drawn as icons in `currentColor`, keeping
  today's accessible names, because the words would leave the title about
  70 px at 390 px wide (*a guess from the mockup's type sizes*); and ⚙,
  which opens Settings. 52 px tall plus the top safe area, on
  `--reader-panel`, a `--reader-hair` rule beneath it. Each control is at
  least 44 px square under `(pointer: coarse)`, as ② made the sidebar's
  rows.
- **The title block**: the work's title in the title face and, under it in
  `--muted`, `Vol. N · <label>` for the edition on screen — the one Space
  moves to. Both come from `/api/library/works/{slug}`, fetched once beside
  the load; any signed-in user may call it (`routes.rs` lists it
  `Filters`). It carries the library's title and each edition's label,
  with metadata edits applied, which the sidecar's `work.title` and
  `editions.*.label` do not: those are the file on disk, and an edit is
  written to the database (`title_hand`, `label_hand`). The cost is one
  small request per open. Until it answers, or if it fails, the bar shows
  the edition's key alone.
- **Shown with the bottom bar, fading with it.** While correcting it is not
  drawn and `CorrectionBanner` takes its place at the top, which keeps O65's
  rule that `Library` stands down during a correction; `L` still leaves.
- **This reverses O65's dock.** `Library` leaves the band above the strip.
  O65 declined "moving it into the toolbar row" because the button would
  have lost `Chrome`'s fade and its hide-while-correcting rule; the toolbar
  is now inside `Chrome`, so it keeps both. O65's two pins —
  `Chrome.test.tsx`'s declared geometry and `App.browser.test.tsx`'s
  `elementFromPoint` — are rewritten for the new place, not deleted.
- **Settings gains a Close button.** ⚙ is a pointer's way in, so the
  dialog needs a pointer's way out; today only `s` closes it.
- **`Tab` reaches no top-bar control at all.** `keyPlan` answers `Tab`
  with `toggle-chrome` and does not prevent its focus move, so one `Tab`
  shows the bars and the next hides them. **Measured in Chromium at
  1440×900 with real keys, on this branch and on `main` alike**: the first
  `Tab` focuses the strip tile after the current one, not any top-bar
  control; `Space` then activates that tile and turns a page rather than
  swapping editions; a second `Tab` returns focus to `<body>`. Every
  control has another keyboard route — `L`, `s`, and Layout inside
  Settings — so nothing is out of reach; filed as O135 for the
  accessibility pass (§5) rather than fixed here, together with Settings
  closing on neither `Escape` nor `s` once ⚙ has opened it.

### 8.4 The bottom bar

- **One full-width bar docked to the bottom edge**, replacing the floating
  strip and the separate readout band: the strip, a 2 px progress line, the
  readout row, then the bottom safe area; `--reader-panel`, a
  `--reader-hair` rule on top.
- **Tiles 160 px at and above 900 px, 96 below**: a strip 176 or 112 px
  tall with the rail's padding and the tiles' borders. One custom property,
  set by the breakpoint, sizes it, and the hover preview stands on the same
  property, so it stays just above the strip at both widths. On a 3× phone
  a tile shows the 160 px thumbnail stretched 1.8× at 96, against 3× at
  160; still soft either way, which is filed below as O134. *Those are the
  slot's figures, not the drawn tile's, wherever a scrollbar takes space:
  the rail is `overflow-x: auto`, a real volume's tiles always overflow it,
  and a classic scrollbar costs 15 px — measured in Chromium on the running
  app, a tile image is 145 px inside the 176 px strip and 81 inside the 112.
  It predates this PR and does not happen where scrollbars overlay, as on
  iOS; filed as O138.*
- **The progress line** is `--line`, filled in `--accent` to
  `(slot + 1) / total` — the fraction `positionLine` already turns into the
  readout's percentage, so the two agree except where the slot count has gone
  backwards, which `progress` clamps at 1 and `positionLine` does not.
- **The readout row** keeps `readoutRow`'s text, order and ellipsis;
  centred, `--muted`, the 0.75rem step.
- **The strip's colours**: the current tile's ring from `#fff` to
  `--accent`; the correcting counterpart's dashed ring from `#f2b134` to
  `--warning` (Ink's `#e2b55c`); a split spread's R/L badge on
  `--reader-panel` in `--text`. `Filmstrip.test.tsx`'s two *coloured*
  `borderColor` assertions change — its three `transparent` ones have nothing
  to tokenise — each watched failing with the old value put back (§4 rule 6).

### 8.5 Panels, the note list and the rest

- **Docking.** Help, Credits, the notes panel and the note list beside the
  page sit at the top right, 16 px below the top bar's line — the top safe
  area, 52 and 16 — and stop 16 px above the bottom bar's top edge, whether
  or not the chrome is shown. A long one scrolls inside itself. At phone
  width a panel is the screen less 32 px, as today.
- **The look** (§2): square corners, `--reader-panel`, a 1 px
  `--reader-hair` border, no shadow; headings in the title face, rows
  divided by hairlines. Settings stays a centred dialog in the same look.
  The panels' own buttons — Close, Export, Reset and the rest — take a
  reader button style from `reader.css`, since `base.css`'s element rules
  reach only `.library` and `.login` (§5 ①); until now they were the
  browser's own.
  Each panel's visual inline styles become classes in `reader.css` — §4
  rule 4, this being their PR — while position and size stay inline where
  a test pins them.
- **The note list under the page** is `visibility: hidden` while the chrome
  is shown — mounted, and keeping its space, so the page area does not
  change on `Tab` — and takes the bottom safe area while it is not. O112
  closes.
- **The rest**: `CorrectionBanner` an ink panel in the top bar's place;
  `Gap`'s message in `--text` on the black; `Borrowed`'s gold mark,
  `rgba(180, 140, 60, 0.9)`, to `--warning`, with Ink's `bg` on it at
  9.72:1; the notes popover on the page in the panel look.
- **The borrowed page's mark docks below the top bar's line**, by §8.1's
  rule for panels, and takes the left safe area with it. Added after the
  whole-branch review measured the top bar covering it: at 390×844 with the
  chrome shown the mark occupied 0–26 px under a bar of 0–52, and
  `elementFromPoint` at its centre answered the bar — and that mark is the
  pointer's only way into the offers and back out. The user chose docking it
  over leaving it covered, on 2026-09-19.
- **Colours drawn on artwork keep their values** (§2) and become named
  tokens in `tokens.css`'s box group, so no component holds a literal: the
  note overlay's outlines and drawing fill, and the dark digits on the note
  list's coloured number chips.
- **The allowlist** loses its entries for `Annotations.tsx`, `Borrowed.tsx`,
  `Chrome.tsx`, `CorrectionBanner.tsx`, `CreditsPanel.tsx`,
  `Filmstrip.tsx`, `Help.tsx`, `NoteList.tsx`, `NotePanel.tsx` and
  `Settings.tsx`. Two remain: `Account.tsx`, which is ⑤'s screen, and
  `styles/annotations.css`, whose two literals — a selected region box's
  white inner line and the ignore hatch — are the labeller's, not the
  reader's. §4 rule 2 wants ⑤ to end with an empty list, and §6 leaves the
  labeller out of the pass; the two cannot both hold, and ⑤ settles which
  gives way.

### 8.6 Tests and checks

On top of §5's list for every PR:

- **jsdom.** The top bar renders only while the chrome is shown, and not
  while correcting, when the banner does; `Library` calls `onLeave`, the
  layout buttons carry `aria-pressed`, ⚙ opens Settings and Settings' Close
  closes it. The title and `Vol. N · label` come from the work's detail,
  fall back to the edition's key, and follow Space. The progress fill is
  `(slot + 1) / total`. The note list under the page stays mounted and is
  `visibility: hidden` while the chrome is shown.
- **Node, over the CSS.** `.reader` declares `color-scheme: dark`; Ink's
  text, muted and accent clear 4.5:1 on `--reader-panel` over black and
  over white; the tile height follows the 900 px breakpoint; `index.html`
  carries `viewport-fit=cover`; the allowlist is lowered.
- **The browser project** (Chromium; the gate runs it, CI does not). With
  Paper pinned, the bar's text computes to Ink's `--text` — the check that
  the mechanism measured above holds in the app and not only in a probe. At 390×844 and
  1440×900: the bars meet the edges, tiles measure 96 and 160, a panel sits
  between the two bars, and the page area is the same size with the chrome
  shown and hidden. O65's `elementFromPoint` check, rewritten for `Library`
  in the top bar; O112's, with the chrome hidden every note row takes a
  click at its centre and with it shown the list is hidden.
- **The browser pass** against the scratch copy of the library, at 390 and
  1440, in Paper and in Ink — where the reader should look the same, which
  is itself a check — in single, two and scroll, with a panel open, while
  correcting, and with notes under the page. *Run on 2026-09-19
  (`docs/notes.md`, "Style pass ③"). Paper and Ink came out **identical to
  the pixel** at 1440×900, captured back to back in one state, and at
  390×844 differed only in the readout's ticking clock. Correcting, a gap
  and a borrowed page could not be reached: every work in that library is
  single-edition and unaligned, and the repository's corpus, added as a
  second root, produced proposals rather than multi-edition works —
  accepting one is ⑤'s screen. Their geometry is pinned in jsdom, and ④'s
  device pass is where they are first seen.*
- **The iPhone**, over the tailnet, the user-agent read first: the safe
  areas in portrait and landscape on the library and on the reader while
  reading, and what Safari's toolbar shows over the reader. The bars' safe
  areas, their 44 px tap targets under `(pointer: coarse)`, and Safari's
  resolution of the tokens inside `.reader` move to ④'s device pass, since
  a phone cannot open the chrome before ④.
- **Filed** in `docs/status.md`: O112, **closed**; and five opened —
  **O134**, the strip soft on a 3× phone, because its tiles are the 160 px
  thumbnail and sharper ones are the server's to send, as O133 did for
  covers; **O135**, for the accessibility pass, the keyboard reaching no
  top-bar control and Settings closing on neither `Escape` nor `s`;
  **O136**, scroll mode not following a page taken from the strip, measured
  on `main` too; **O137**, the note editor having no exit a finger can
  reach, which ④'s touch design owns; and **O138**, the rail's scrollbar
  spending 15 px of every tile.

## 9. PR ④ in detail

Settled with the user on 2026-09-20, before planning. What §5 ④ already says
is not repeated here.

**What exists**, read from `main` at `333b3ab` for this section:

- **The chrome has exactly two ways to appear**, and `setChromeVisible` has
  exactly two call sites: `keyPlan`'s `Tab`, which `App.tsx` answers with
  `toggle-chrome`, and the correction path, which shows it because the
  filmstrip is where a counterpart is picked. A finger can reach neither.
  That is why ③'s device pass saw the reader's ground and nothing the reader
  draws on it.
- **Every press on the page area already means something.** `beginRegion`
  asks `notes.ts`'s `pressTakesPan` first — true when `zoomable` (the module
  is on, which `zoom.ts` defaults to, *and* something is drawn), the press is
  not on a control, and either the editor is closed or Shift is held — and
  pans. What is left goes to `gestureFor`, which answers `ignore` for every
  press made with the editor closed. So today a press with the editor closed
  pans, and a tap is a pan that went nowhere: `panStep` hands the *same* rect
  back at fit-whole, where `panBy`'s own doc calls a drag "a no-op instead of
  a jitter".
- **`Chrome` is a sibling of the page area, not a child.** A press on either
  bar never reaches `beginRegion`, so nothing below has to exclude the bars.
- **The wheel is subscribed by hand**, `{ passive: false }`, on the page area,
  and serves the trackpad pinch too because a pinch arrives as a `wheel` with
  `ctrlKey` set. This PR does not touch it.
- **`index.html`'s viewport meta** is `width=device-width, initial-scale=1.0,
  viewport-fit=cover`. Nothing sets `user-scalable=no` and nothing sets
  `touch-action`, so on a phone the browser's own pinch zoom and its
  double-tap zoom are live over the whole document.
- **`stepFor`** (`modules/direction.ts`) is the one place that maps a physical
  arrow to a slot step, with three consumers: `App.tsx`'s `step` intent,
  `Filmstrip`'s rail, and `modules/help.ts`.
- **`NotePanel`'s own doc offers two exits** — `Escape`, or a click on the
  background around the page — and `gestureFor`'s rule 1 is what carries the
  second.

**What this does not reopen.** `2026-07-30-reader-design.md` settled on
2026-08-01 that the reader is desktop-first, and that a sheet, a picker and
touch ergonomics belong to a native iOS reader "instead of being retrofitted
onto a keyboard-first layout". ④ is three gestures on that layout and one
button on a panel. It adds no sheet, no picker, and no way to reach anything
that a key could not already reach.

### 9.1 Decisions

| Question | Chosen | Beat, and why |
| --- | --- | --- |
| Which pointers answer the gestures? | **Every pointer, a mouse included** | *Touch and pen only*, gated on `pointerType`: nothing on a desk would change, and the browser project would need touch emulation to see any of it — but it costs one-handed reading at a desk, and makes one rule behave two ways on the two devices one reader uses. *Touch and pen, plus a mouse below 900 px* loses for making the same click mean two things on one machine depending on window width |
| With a panel open, what does a tap do? | **Closes the panel, and turns nothing** | *Turns as usual, the panel stays*: keeps panel state out of the gesture rule, but leaves every docked panel with no pointer exit on a phone, and turns a page the reader cannot see. *Closes it and turns*: nothing is spent, but one tap doing two things is the rule nobody can predict — a mis-tap while reading the credits would dismiss them and lose the place |
| Zoomed in, what does a horizontal drag do? | **Pans. The swipe lives only at fit-whole** | *A swipe always turns and a pan starts at the edge* (the photo-album rule): familiar, but it makes the first frames of every pan ambiguous and needs the drag to accumulate before committing to either. *A swipe always turns and a pan takes two fingers*: unambiguous per finger, but two fingers are the browser's pinch, it would need `touch-action: none` over the page, and it leaves a mouse — now in scope — with no pan at all |
| In scroll mode, what does a tap do? | **Toggles the chrome, anywhere on the page** | *The thirds turn here too*: one map for both modes, but it lands straight on O136, where a position move does not move the view — and fixing that is a behaviour change §6 puts outside this pass. *Nothing at all*: the smallest change, and it leaves a phone in scroll mode with no way to the chrome, which is the hole ④ exists to close |
| O137: the note editor's exit for a finger | **Both**: a Close button on the editor, and the letterbox tap kept and measured | *The button alone*: certain, but leaves the exit a reader finds by accident unreliable and unmeasured. *The letterbox alone*: no new control, but where a page fills the width there is nothing left to aim at |

The first answer is what makes the rest testable without touch emulation: a
mouse click is a press with the same shape, so the browser project can drive
the thirds, the panel dismissal and the swipe in Chromium, and only what is
touch-only — the double tap, the pinch, a thumb's idea of 10 px — waits for
the device pass. It also puts a new way to turn a page under every desk
reader's mouse, which is the cost: a click on the page that used to do
nothing now turns. The fourth answer means the same tap means different
things in the two modes, accepted because scrolling is already the turn in
scroll mode.

### 9.2 The rule

**`web/src/gestures.ts`**, a pure module with a test beside it, in the shape
`gestureFor` established: the rule lives where it can be tested, and
`App.tsx` reads the DOM for it and carries out the answer. Named for
gestures rather than for touch, because a mouse is in scope.

A press on the page area is classified **on `pointerup`**, against where and
when it went down:

1. **It began on a control** — `isInteractivePress`, ancestor-aware, the
   same predicate `pressTakesPan` uses. Not a gesture. Unchanged.
2. **It moved more than `TAP_SLOP` or lasted longer than `TAP_HOLD`.** Not a
   tap: it was a pan, a draw, a move or a swipe, and each of those is decided
   exactly as it is today.
3. **Otherwise it is a tap**, resolved in this order:
   - a. **The editor is open** (`panelMode === 'edit'`): region drawing keeps
     the press, and no tap gesture fires. §5 ④ forbids taking a gesture from
     region drawing, and this is where that is honoured.
   - b. **A panel is open** — Help, Settings, credits, or the note review
     panel: close **every** one that is open, and consume the tap. Nothing
     turns, nothing toggles. One tap returns to the page whatever is over it,
     so no order of precedence between panels has to be invented.

     **Settings needs a second route, and ④'s browser pass is what found
     it.** The three `docked-panel` panels sit in a corner, so a tap beside
     one reaches the page area and this rule answers it — measured at both
     sizes. Settings is a `reader-dialog`, whose scrim is the **whole
     viewport**: `[0, 0, 1440, 900]` and `[0, 0, 390, 844]`, so every press
     lands inside the dialog and the page area never hears it. Settings
     therefore dismisses on a press that goes down *and* comes up on its own
     scrim — the conventional modal behaviour, in `Settings.tsx` where a
     reader looks for it, spending the press on dismissal exactly as this
     rule does. A press that begins on a control inside the panel and
     releases over the scrim is not a dismissal.
   - c. **Scroll mode**: toggle the chrome, wherever the tap landed.
   - d. **Paged mode**: the page area's width in thirds. The left third
     turns `by: -1` and the right third `by: 1` — physical, before
     `stepFor` — and the middle third toggles the chrome.

**The swipe.** At fit-whole (`isWhole(view)`), in paged mode, with no editor
and no panel open: a drag whose horizontal travel is at least `SWIPE_MIN` and
at least twice its vertical travel turns one page. A drag that fails either
test is what it is today — a pan, which at fit-whole moves nothing.

**The constants**, all four in `gestures.ts` so they change in one place:
`TAP_SLOP` 10 CSS px, `TAP_HOLD` 500 ms, `SWIPE_MIN` 40 px, and the axis
ratio 2. **These are chosen, not measured.** The device pass is where a thumb
says whether 10 px is too tight to hold still for and whether 40 px is far
enough to be deliberate; the figures it lands on are written back here.

**Mirroring is not new code, and it gains no new consumer.** A tap on the
right third and a leftward swipe are the same physical intent as pressing `→`
— `by: 1` — and `gestures.ts` answers in exactly those physical terms. The
turn is then handed to `App.tsx`'s `readerIntent`, which already puts every
`step` through `stepFor` before applying it. So `stepFor` keeps its three
consumers, the tap inherits the mapping rather than repeating it, and a
right-to-left work needs nothing written twice.

**The chrome does not hide itself.** A turn made with the bars up leaves
them up, as a keyed turn does; only a tap on the middle third, or `Tab`,
takes them down. Nothing in ④ is on a timer.

**The thirds are of the page area, not of a cell.** In two-page mode the
middle third spans the seam, so a tap at the seam toggles the chrome rather
than turning either half — one geometry, whatever is drawn under it.

**The thirds do not require a drawn page**, unlike the pan. On a gap or a
borrowed page nothing is drawn and `zoomable` is false, and a reader who
wants away from one still taps the outer third to leave. `Gap`'s own buttons
are excluded by rule 1, like any other control.

**While correcting, the page area's tap is unchanged.** The correction is
picked in the chrome — the banner in the top bar's place, the candidates in
the strip — so a tap on the page below it turns or toggles as it otherwise
would, and `Gap`'s and `Borrowed`'s own buttons stay excluded by rule 1.

**A press that already did something is consumed.** `gestureFor`'s `close`
fires on `pointerdown`, so without this the `pointerup` of the tap that shut
the note editor would fall through to 3c or 3d and turn a page as well —
exactly the one-tap-two-things the panel answer rules out. `beginRegion`
marks the press; the `pointerup` classification skips a marked one.

**What ④ does not take**, each for its own reason:

- **Pinch** stays the browser's. No `touch-action: none` anywhere.
- **The wheel**, including the trackpad pinch it carries, is untouched.
- **A vertical drag in scroll mode** is the browser's scrolling: `swipe`
  refuses it on its first guard, before any arithmetic — scroll mode has no
  swipe at all — and rule 2 keeps it from being a tap. (The axis test is not
  what refuses it; that test only decides a drag already `SWIPE_MIN` or more
  across the page.)
- **Panning** keeps every press it has today. The swipe is carved out of a
  press the pan already discards: at fit-whole it moves nothing, and above
  fit-whole the swipe does not fire — every drag stays the pan's. **The tap
  is not carved out of the pan at all**: it does not read `atWhole`, so a tap
  still turns above fit-whole — settled with the user on 2026-09-20: "Zoomed
  in, the thirds still turn on a tap, so no page is unreachable." *(An
  earlier version of this said "above fit-whole neither of them fires",
  which was wrong about the tap — I5 of ④'s whole-branch review, 2026-09-20.)*
- **Region drawing, moving and finishing** keep theirs by rule 3a.
- **A long press** is not a tap by rule 2, so the selection callout iOS puts
  up over an image turns no page.

The one thing added to the page area's styling is **`touch-action:
manipulation`**, which removes the browser's double-tap zoom and nothing
else. Without it a double tap on a phone would turn two pages *and* zoom the
document; with it, two taps turn two pages, which is what they say.

### 9.3 O137

Both exits, because they cover different failures.

- **A Close button on the editor**, at least 44 px, matching the Close ③ gave
  the Settings dialog and the one the review panel has always had. This is
  the exit that is always there, and the one a reader can see.

  **It reverses a decision recorded in `NotePanel.tsx` itself**, which today
  carries a comment arguing against exactly this button: "No Save, and no
  Done either… `Escape`, or a click on the background around the page, is
  what closes this. Buttons for both would be asking the reader to tell the
  reader something it can see." That reasoning holds for a keyboard and a
  mouse and fails on a phone, where both of the exits it names are missing —
  which is O137. The comment is rewritten rather than left contradicting the
  code, and the panel's status line, which reads `saved · esc or click away
  when done`, stops naming two exits a finger does not have.
- **The letterbox tap had to be built, not measured.** This section first
  said it "stays `gestureFor`'s rule 1, unchanged in code", and put a
  *computed* band of roughly 143 px against it. ④'s browser pass measured
  instead, and found the exit had never existed: the page `<img>`'s box
  **equals** the page area at every size tested, and `object-fit: contain`
  letterboxes the picture *inside that element*. So a press in the margin is
  still a press on an `HTMLImageElement`, `onAnImage` is true of every press
  in the reader, and rule 1 — looking away finishes the note — could not
  fire. `NotePanel`'s own doc had promised that exit since the editor
  shipped.
  **What ④ changes** is the predicate at the call site: a press is on the
  page when it is on an image, and, *when that image is the draft's own*,
  its point also lies inside the draft's painted box; an image that is not
  the draft's own is left to rule 2, unaffected by where the press falls
  within it. `pointInSlot` is already unclamped for exactly this — its doc
  says "a point in the letterbox reads as outside `0..1`" — so the rule list
  in `gestureFor` is untouched, and **the only press whose meaning changes is
  one in the draft's own letterbox, which rule 1 now reaches**. **Measured**
  in Chromium against a copy of the real library: at 1440×900 a 900×1350 page
  paints at `[420, 0, 600, 900]`, leaving 420 px of margin either side; at
  390×844 a 683×1024 page paints at `[0, 130, 390, 585]`, leaving 130 px
  above and below. The Close button still covers the case where a fit leaves
  no margin at all.

  **In two-page mode this exit is one-sided.** `App.tsx`'s `beginRegion`
  judges a press against the draft's own painted box only; a press in the
  *facing* cell's letterbox is on an image that is not the draft's own, so
  rule 2 answers `ignore` for the whole of it regardless of where it falls.
  A press in the margin around the draft's own half of a spread finishes the
  note; the same press in the margin around the facing half does not, though
  the two margins look identical on screen. Deliberate, and matches `main`
  — judging a facing-cell press against the draft's own painted box would be
  the wrong frame of reference, not a finer-grained answer (C1 of the whole-
  branch review) — and `App.tsx`'s own comment on `beginRegion` records the
  reasoning where the code reads it.

`Escape` and the keyboard route are untouched. The note *list* is not a panel
for rule 3b's purposes — it is placed with the page, and O112 already decided
what happens to it when the chrome comes up.

### 9.4 The device pass, and what only it can see

The iPhone over the tailnet, with the user-agent read from the request log
before any answer is believed, in both orientations, in Paper and in Ink:

- **Every gesture above**, including one that must *not* happen: a pan that
  ends where it started must not turn, and a swipe must not fire while the
  reader is zoomed in.
- **Pinch still zooms the document**, and a double tap turns two pages
  without zooming it.
- **The bars, on a phone, with the chrome shown** — the first time either is
  on a phone at all. Their 44 px targets under `(pointer: coarse)`, and their
  safe areas against the notch and the home indicator.
- **Safari's own resolution of the reader's `light-dark()` tokens** inside
  `.reader`. ③ measured this in Chromium 151 and Firefox 153 only; WebKit is
  not installed here.
- **The note editor's two exits**, and the letterbox band's real size.

**Correcting, a gap and a borrowed page on a screen.** ③ left these pinned in
jsdom because the scratch library holds no multi-edition work and the screen
that accepts a proposal is ⑤'s. ④ **hand-seeds a second edition into the
scratch copy's database** rather than wait: it is our copy, and it is the
only way these three are seen before ⑤. If the seed does not hold up, they
pass to ⑤ and ④ files a row saying so.

**Two rows go to the user rather than being decided here**, because both are
questions about what a screen should look like and this is the PR that puts
the strip in front of a thumb:

- **O134** — the strip's tiles are the 160 px thumbnail, drawn 96 px tall
  below 900 px, so they are stretched on a 3× screen. Does the softness
  bother the user on the phone? A sharper tile is the server's to send, as
  O133's was for covers.
- **O138** — the rail's scrollbar takes 15 px off every tile where scrollbars
  take space, which is not iOS. Hide it and lose the affordance, grow the
  strip on every platform to pay for one, or leave the 1:1 claim wrong on a
  desktop.

**O135 stays the accessibility pass's**, narrowed rather than closed: rule 3b
gives Settings the pointer exit it lacks, and the keyboard half of that row
is untouched. **O136 stays open**, and is the reason scroll mode gets no
turning tap.

### 9.5 Tests and checks

- **`gestures.test.ts`** over the pure rule: the slop and hold boundaries
  either side, each of 3a–3d, both thirds and the middle, the swipe's
  distance and axis tests, and the consumed press. Every mirrored case runs
  **both** `ltr` and `rtl` — O65 was an `rtl` fixture passing a mutation that
  `ltr` caught, and one direction is not a test of a mapping.
- **jsdom component tests** drive `pointerdown`/`pointerup` on the page area
  and assert the turn, the toggle and the dismissal. The thirds arithmetic
  takes a box as an argument so jsdom can feed one directly; where a
  component test needs the page area's own width it stubs
  `getBoundingClientRect`, because jsdom lays nothing out.
- **The browser project** drives real presses in Chromium — the thirds, the
  panel dismissal, the swipe, and the pan that must not turn. It can do this
  with a mouse because of §9.1's first answer. Whether it can also emulate
  touch, through `browser.instances[].context`, is a **probe for the plan**,
  not an assumption: if it can, the double tap goes here; if it cannot, the
  double tap is the device pass's alone.
- **Every new test is watched failing** with its defect put back by hand —
  the mirrored ones with `stepFor` bypassed, the consumed press with the mark
  removed, the slop with the comparison flipped. A test that passes against
  its own defect is this repository's most-recorded failure.
- The five checks §5 requires of every PR in this pass, in full.
