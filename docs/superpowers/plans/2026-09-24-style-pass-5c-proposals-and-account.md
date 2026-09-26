# Style pass ⑤c — the proposals inbox and the account screen: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Put the last two screens of the style pass on the tokens: the inbox's kind row stops mixing a one-of-N filter with an independent toggle and takes the underlined tab look (O150), the pills move to the stylesheet of the screen they draw, 91 raw `rem` values and ten raw corners become tokens, the reader's Account overlay becomes a reader dialog and `/account` becomes a settings screen, `TILE_HEIGHT_PX` is pinned against the CSS that draws it (O148), `admin.css` gets the 44 px coarse-pointer floor it has never had on any of its five screens, and the colour-literal allowance reaches `{}`.

**Architecture:** Almost all of it is `web/src/styles/admin.css` lines 29–730 (the proposals range, which holds **zero** tokens of its own) and 1589–1646 (`.account`). Four markup changes are taken deliberately and no more: the inbox's two rows drop the `chips` co-class, "Show weak proposals" becomes a checkbox outside the kind row, `Account.tsx` swaps 25 inline `style=` props for class names, and `Library.tsx` drops a co-class that is about to have no rules. `reader.css` gains three layout rules and no colour, because ③ already owns every colour this panel needs. One task reaches past the two screens the branch is named for: the coarse-pointer floor covers all five screens `admin.css` draws, which is a decision taken with the measurements in hand (spec §12.4a).

**Tech Stack:** React 19, TypeScript, plain CSS with custom properties, Vitest across three projects — `node` (`src/**/*.test.ts`), `jsdom` (`src/**/*.test.tsx`, excluding `*.browser.test.tsx`), `browser` (`src/**/*.browser.test.tsx`, real Chromium).

## Global Constraints

- **Spec:** `docs/superpowers/specs/2026-09-17-style-pass-design.md` §12. §5's checklist applies to this PR as to every other in the pass.
- **Branch:** `style-pass-5c-proposals-and-account`, off `main` at `5e09e474`. The spec's §12 is already written into the working tree on it and is committed by Task 0.
- **Colour comes from `styles/tokens.css` only.** §4 rule 2. `colourLiterals.test.ts` fails on any hex, `rgb()`/`rgba()`/`hsl()`/`hsla()` or CSS system-colour keyword added under `src/`, and its allowance only shrinks. **This PR takes `ALLOWED` to `{}`** — do not add an entry, and do not leave one.
- **No `opacity` for quiet text.** Quiet is bought with `--muted`, size and border. The one exemption is a disabled control — `.proposal-detail .compare` and `.proposals .row .dismiss:disabled` are the two that exist, and `.proposal-detail`'s own comment names them.
- **Spacing, type and radius come from tokens**: `--space-1: 4px` `--space-2: 8px` `--space-3: 12px` `--space-4: 16px` `--space-5: 24px` `--space-6: 32px` `--space-7: 48px`; `--text-h1: 2rem` `--text-h2: 1.5rem` `--text-h3: 1.25rem` `--text-body: 1rem` `--text-ui: 0.875rem` `--text-meta: 0.75rem`; `--radius-control: 2px`. Colour tokens: `--bg --surface --field --line --line-strong --text --muted --accent --on-accent --danger --success --warning`, plus the reader's `--scrim --reader-panel --reader-hair --reader-ground`.
- **Run the web gate with the thread throttle**: `NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh`. An unthrottled run has been killed for memory on this machine.
- **Verify a gate by its own exit status.** Never `| tail`, never `; echo` — both report the *pipeline's* status, and a background task's reported exit code is the compound command's. Write the run to a file and read `$?` from it.
- **Vitest reporter**: under Claude Code vitest picks an `agent` reporter that hides passing tests' console output even with `--silent=false`. Use `--reporter=default` when you need to see it, and put a control `console.error` in before believing a zero.
- **Every new test is watched failing** with its defect put back by hand before it counts as written. A test that passes against its own defect is this project's most-recorded failure mode.
- **No AI/assistant attribution** in any commit message, comment or doc.
- **Scratch files go in the session scratchpad**, never in the repo and never in `$HOME`. Screenshots for the browser pass go in `target/style-pass-5c/`, which is gitignored.

---

## What is already on the branch

The working tree on `style-pass-5c-proposals-and-account` carries **§12 of the spec, 423 lines, uncommitted**. Task 0 commits it. Nothing else has been changed.

`target/style-pass-5c/lib/library.db` is a copy of the real library made on 2026-09-24 — 138 series, 1,896 works, 1 root, 4 rules, 35 open proposals, account `local` with the password `style5c`. The original's checksum is `d5ebc0c2378650356115a8cce637e50dc5b4ab7f7cc8f388553cadd575a59e67` and must still be that afterwards. `target/style-pass-5c/drawn/` holds the option drawings §12 refers to.

---

## The measurements this plan rests on

Every figure below was taken in a Playwright-driven Chromium against that copy on 2026-09-24, and each is re-checkable.

| what | figure |
|---|---|
| raw `rem` in the proposals range's own 59 rules | **80**, against 0 space, type or radius tokens |
| raw `rem` in `.account`'s 7 rules | **11**, against 1 type token |
| `font-size` declarations in the slice | 24, over 7 sizes — 11.2, 12, 12.8, 13.6, 14.4, 15.2, 20 px |
| spacing declarations in the slice | 62, over 15 values from 0.8 px to 16 px |
| `border-radius` declarations in the slice | 11 — `4px`×7, `3px`×3, `999px`×1 |
| `Account.tsx` inline `style=` props | **25**; colour literals **10** |
| `/account`'s `opacity: 0.6` on "— last seen" | **4.18:1 on Paper** (fails AA), 5.88 on Ink |
| `--muted` | 5.25 Paper, 6.77 Ink |
| `--danger` / `--success` on the reader panel's ground `rgb(19,17,14)` | 7.99 / 9.23 |
| sideways scroll at 390 px, both screens, document and `main` | **0** — and it stays 0 |

---

## File Structure

| File | Responsibility | Tasks |
|---|---|---|
| `web/src/Proposals.tsx` | Drops `chips` from both rows; "Show weak proposals" leaves the kind row and becomes a checkbox | 1 |
| `web/src/Library.tsx` | Drops the `chips` co-class, which is about to have no rules | 1 |
| `web/src/styles/library.css` | `.chips` leaves; `.library-chips` becomes self-sufficient | 1 |
| `web/src/styles/admin.css` | The inbox's own pill and tab rules; the whole 29–730 sweep; `.account`'s card, `.field` and measure; the coarse floor | 1, 2, 3, 6, 7 |
| `web/src/ProposalRow.tsx` | `TILE_HEIGHT_PX` leaves for `tileSize.ts`; nothing else changes | 4 |
| `web/src/tileSize.ts` | Takes `TILE_HEIGHT_PX` in beside the strip's own two, so a `node` test can reach it without importing a `.tsx` | 4 |
| `web/src/testing/css.ts` | The caller count its own doc comment states goes six → eight | 3, 6 |
| `web/src/styles/admin-proposals.css.test.ts` | **New**, `node` project: pins `TILE_HEIGHT_PX` against the CSS, and the two rows' looks | 1, 4 |
| `web/src/Account.tsx` | 25 inline `style=` props become class names; `LOOKS` becomes a map of class names | 5, 6 |
| `web/src/styles/reader.css` | Three layout rules the Account panel needs and no ③ panel did; `.reader-field` joins the coarse floor | 5, 7 |
| `web/src/index.css.test.ts` | The pill/tab test is rewritten for the new shape | 1 |
| `web/src/Proposals.test.tsx` | The `chips` assertion is replaced; the checkbox's independence is asserted | 1 |
| `web/src/Account.test.tsx` | The inline-prop and `opacity` source gates; the class-name switch per placement | 5, 6, 7 |
| `web/src/Account.browser.test.tsx` | **New**, `browser` project: the panel really is a reader dialog, measured | 5 |
| `web/src/Proposals.browser.test.tsx` | No sideways scroll at 390, expanded as well as collapsed | 8 |
| `web/src/colourLiterals.test.ts` | `annotations.css` joins `EXEMPT`; `ALLOWED` becomes `{}` | 7 |
| `web/src/Config.browser.test.tsx` | The coarse floor's subjects, on the configuration screens | 7 |
| `docs/status.md` | O148 and O150 close; two new rows open; the test count | 0, 10 |

---

## Task 0: Commit the design

**Files:**
- Modify: `docs/superpowers/specs/2026-09-17-style-pass-design.md` (already edited in the working tree)

**Interfaces:**
- Consumes: nothing.
- Produces: the committed §12 every later task cites.

- [ ] **Step 1: Confirm the branch and that the only change is the spec**

```bash
cd /home/davey/projects/taiyaku
git branch --show-current
git status --short
```
Expected: `style-pass-5c-proposals-and-account`, and exactly one line — ` M docs/superpowers/specs/2026-09-17-style-pass-design.md`.

- [ ] **Step 2: Confirm the scratch library is untouched**

```bash
sha256sum /home/davey/temp/manga-test/library.db
```
Expected: `d5ebc0c2378650356115a8cce637e50dc5b4ab7f7cc8f388553cadd575a59e67`. If it differs, stop and say so — nothing in this plan writes to it, so a difference means something else did.

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/specs/2026-09-17-style-pass-design.md
git commit -m "⑤c: the design

Section 12 of the style pass spec. Six decisions, each drawn on the real
screen against a copy of the real library before it was settled: the
inbox's filter rows stop being one kind of row, the pills move to the
stylesheet of the screen they draw, the Account panel becomes a reader
dialog, /account becomes a settings screen, TILE_HEIGHT_PX gets the guard
the reader's strip has had since the reader was styled, and the
colour-literal allowance reaches empty by exempting the labeller rather
than tokenising two values that must not follow the theme.

O149 has an answer here and lands on its own branch afterwards: it is the
Rules screen, which this one never opens. Section 12.4a is a seventh
decision no open item had: admin.css has never carried a coarse-pointer
floor, so on a phone every control on all five of its screens is its
desktop height, down to 27.6px Dismiss buttons and 13x13 checkboxes."
```

---

## The sweep's one rule, stated once

Tasks 2 and 3 apply the same rule, so it is written here rather than twice.

**Nearest step, ties to the smaller.** Spacing goes to `--space-1: 4px` … `--space-5: 24px`; type to `--text-meta: 12px`, `--text-ui: 14px`, `--text-body: 16px`, `--text-h3: 20px`; every `border-radius` of `3px` or `4px` to `--radius-control: 2px`.

| raw | px | becomes (spacing) | becomes (`font-size`) |
|---|---|---|---|
| `0.05rem` | 0.8 | `--space-1` | — |
| `0.1rem` | 1.6 | `--space-1` | — |
| `0.15rem` | 2.4 | `--space-1` | — |
| `0.2rem` | 3.2 | `--space-1` | — |
| `0.25rem` | 4 | `--space-1` | — |
| `0.3rem` | 4.8 | `--space-1` | — |
| `0.35rem` | 5.6 | `--space-1` | — |
| `0.4rem` | 6.4 | `--space-2` | — |
| `0.5rem` | 8 | `--space-2` | — |
| `0.6rem` | 9.6 | `--space-2` | — |
| `0.7rem` | 11.2 | `--space-3` | **stays raw only on a micro-label** — uppercase at `0.06em` tracking (`.side-role`, `.lab`). Plain 11.2 px text is off the scale like any other: `.vol` and `.accept-form .scope` take `--text-meta` |
| `0.75rem` | 12 | `--space-3` | `--text-meta` |
| `0.8rem` | 12.8 | `--space-3` | `--text-meta` |
| `0.85rem` | 13.6 | — | `--text-ui` |
| `0.9rem` | 14.4 | `--space-4` | `--text-ui` |
| `0.95rem` | 15.2 | — | `--text-body` |
| `1rem` | 16 | `--space-4` | — |
| `1.25rem` | 20 | — | `--text-h3` |

**Five raw values stay, and each is a box rather than a gap or a step**: `grid-template-columns: 9rem …` (the kind column), `minmax(15rem, 1fr)` (the sides' fold), `width: 4.5rem` (the page tile, which Task 4 pins), `flex: 0 0 1rem` (the reason list's sign column), and `max-width: 40rem` (`.account`, which Task 6 turns into `--config-measure`). `border-radius: 999px` on `.score` stays a pill.

**⑤a's shared `.field` block (`admin.css:493-612`) is not in this sweep.** It is six screens' rule, not this screen's, and its two raw values are `gap: 0.15rem` and the `font-size: 0.7rem` §12.4 keeps as the micro-label anyway. The "80 raw `rem` in the proposals' own 59 rules" figure already excludes it — do not widen the diff to it.

**Four places where the sweep moves pixels, listed so the browser pass looks at them rather than past them:**

| rule | what moves | by |
|---|---|---|
| `.proposal-detail .fact` | `padding: 0.05rem 0` → `var(--space-1) 0` | +6.4 px per fact row, four to five rows a side |
| `.proposals .what` | `gap: 0.1rem` → `var(--space-1)` | +2.4 px between the subject and the `↳` line |
| `.proposal-states .state` | `padding: 0.35rem 0.9rem` → `var(--space-1) var(--space-3)` | pill height 37.2 → ~34, before Task 7's coarse floor |
| every `border-radius` | 3 px and 4 px → 2 px | corners tighten everywhere on these screens |

---

## Task 1: The sweep, part 1 — the header, the list and the collapsed row

`admin.css:29-212`. Fourteen rules, none of which holds a token today.

**Files:**
- Modify: `web/src/styles/admin.css:29-212`

**Interfaces:**
- Consumes: nothing. This task runs **first**, against `admin.css` exactly as `5e09e474` left it, so every line number below is the one in the file today.
- Produces: nothing new. The range's rules keep every selector they have.

- [ ] **Step 1: Rewrite the fourteen rules**

Leave every comment in place — they explain decisions, not values, and the one exception is noted in Step 2. The declarations become:

```css
.proposals h1 {
  margin: 0;
  font-size: var(--text-h3);
}

.proposals .failure {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.proposals .retry {
  padding: var(--space-1) var(--space-2);
  border: 1px solid var(--line-strong);
  border-radius: var(--radius-control);
  background: var(--surface);
  color: var(--text);
  font: inherit;
  cursor: pointer;
}

.proposal-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  gap: var(--space-1);
  container-type: inline-size;
  container-name: inbox;
}

.proposal-list li {
  min-width: 0;
  border: 1px solid var(--line-strong);
  border-radius: var(--radius-control);
}

.proposal-list li.strong {
  box-shadow: inset 3px 0 0 var(--accent);
}

.proposals .row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding-right: var(--space-2);
}

.proposals .row .disclosure {
  flex: 1;
  min-width: 0;
  display: grid;
  grid-template-columns: 9rem minmax(0, 1fr) auto;
  align-items: baseline;
  column-gap: var(--space-3);
  row-gap: var(--space-1);
  padding: var(--space-2);
  border: 0;
  background: none;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.proposals .kind {
  font-weight: 600;
}

.proposals .what {
  display: grid;
  gap: var(--space-1);
}

.proposals .subject,
.proposals .arrow {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.proposals .subject {
  font-weight: 600;
}

.proposals .arrow {
  font-size: var(--text-meta);
}

.proposals .score {
  justify-self: start;
  border: 1px solid var(--line-strong);
  border-radius: 999px;
  padding: 0 var(--space-2);
  font-size: var(--text-meta);
  line-height: 1.5;
}

.proposals li.strong .score {
  border-color: var(--accent);
  background: var(--accent);
  color: var(--on-accent);
}
```

The container query at the foot of the range holds only `grid-template-columns` and grid placement and does not change.

`inset 3px 0 0` on `.proposal-list li.strong` **stays a literal 3 px**: it is a strip's width, not a gap, and its own comment records why it is a shadow rather than a border — a border that changed width between rows would shift the `.kind` column, the one column this screen asks a person to read straight down.

`padding: var(--space-2)` on `.disclosure` replaces `0.5rem 0.6rem`: 8 and 9.6 both go to 8, so the two-value form collapses to one.

- [ ] **Step 2: Fix the one comment the sweep makes wrong**

`admin.css:28` ends the file's head comment with `Tokens, like everything else below `.library`.` — which has been false for this range since it was written, and is what this task makes true. Replace it with:

```
 * Tokens, like everything else below `.library` — which this range did not
 * hold a single one of until ⑤c. Its 59 rules carried 80 raw `rem` values
 * and zero space, type or radius tokens; every token that appeared in it
 * belonged to the `.field` block ⑤a hoisted through it.
```

- [ ] **Step 3: Run the suites that read this range**

```bash
cd web && npx vitest run --project node src/index.css.test.ts src/colourLiterals.test.ts --reporter=default
```
Expected: PASS. No colour changed, so `colourLiterals` is unmoved; it is run because a sweep that accidentally inlined one would be caught nowhere else. (`admin-proposals.css.test.ts` does not exist yet — Task 3 writes it.)

- [ ] **Step 4: Confirm the range is empty of raw values**

```bash
cd /home/davey/projects/taiyaku && python3 - <<'EOF'
import re, pathlib
src = pathlib.Path('web/src/styles/admin.css').read_text().split('\n')[28:212]
body = re.sub(r'/\*[\s\S]*?\*/', '', '\n'.join(src))
print('raw rem:', sorted(set(re.findall(r'[\d.]+rem', body))))
print('raw px :', sorted(set(re.findall(r'\d+px', body))))
EOF
```
Expected exactly: `raw rem: ['32rem', '9rem']` — the container query's breakpoint and the kind column — and `raw px : ['1px', '3px', '999px']` — the hairline borders, the strength strip, and `.score`'s pill.

- [ ] **Step 5: Commit**

```bash
git add web/src/styles/admin.css
git commit -m "⑤c: the inbox's header, list and collapsed row take tokens

Fourteen rules, nearest step with ties to the smaller. Two of them move
pixels and the browser pass is told where: .proposals .what's gap goes
1.6px to 4px, and both of this range's 4px corners go to 2px. The 3px
corners are in the evidence panel and belong to the next task.

The strength strip keeps its literal 3px. It is a strip's width rather
than a gap, and its own comment says why it is an inset shadow and not a
border: a border that changed width between rows would shift the kind
column, which is the one column this screen asks a person to read
straight down.

The head comment said this range took tokens like everything else below
.library. It never had; that sentence is now true and says what it used
to hide."
```

---

## Task 2: The sweep, part 2 — the evidence panel

`admin.css:213-730`, excluding ⑤a's shared `.field` block at `493-612`. **55 declarations change**, and the list below is exhaustive — it was generated from the file rather than read off it, so a diff with a 56th or a 54th change in it is wrong.

**Files:**
- Modify: `web/src/styles/admin.css:213-492` and `:613-730`

**Interfaces:**
- Consumes: the rule at the head of this section.
- Produces: nothing new.

- [ ] **Step 1: Apply the 55 changes**

| line | property | from | to |
|---|---|---|---|
| 214 | `padding` | `0.5rem 0.6rem` | `var(--space-2)` |
| 250 | `margin-bottom` | `0.7rem` | `var(--space-3)` |
| 251 | `font-size` | `0.9rem` | `var(--text-ui)` |
| 257 | `gap` | `1rem` | `var(--space-4)` |
| 258 | `margin-bottom` | `0.8rem` | `var(--space-3)` |
| 265 | `gap` | `0.4rem` | `var(--space-2)` |
| 283 | `font-size` | `0.95rem` | `var(--text-body)` |
| 290 | `gap` | `0.4rem` | `var(--space-2)` |
| 305 | `border-radius` | `3px` | `var(--radius-control)` |
| 313 | `font-size` | `0.75rem` | `var(--text-meta)` |
| 326 | `border-radius` | `3px` | `var(--radius-control)` |
| 330 | `margin-left` | `0.35rem` | `var(--space-1)` |
| 336 | `font-size` | `0.8rem` | `var(--text-meta)` |
| 341 | `font-size` | `0.8rem` | `var(--text-meta)` |
| 347 | `gap` | `0.75rem` | `var(--space-3)` |
| 348 | `padding` | `0.05rem 0` | `var(--space-1) 0` |
| 371 | `gap` | `0.2rem` | `var(--space-1)` |
| 376 | `border-radius` | `3px` | `var(--radius-control)` |
| 377 | `padding` | `0 0.3rem` | `0 var(--space-1)` |
| 382 | `margin` | `0 0 0.25rem` | `0 0 var(--space-1)` |
| 390 | `margin` | `0 0 0.8rem` | `0 0 var(--space-3)` |
| 393 | `gap` | `0.15rem` | `var(--space-1)` |
| 394 | `font-size` | `0.85rem` | `var(--text-ui)` |
| 401 | `gap` | `0.5rem` | `var(--space-2)` |
| 421 | `gap` | `0.5rem 0.75rem` | `var(--space-2) var(--space-3)` |
| 423 | `border-radius` | `4px` | `var(--radius-control)` |
| 424 | `padding` | `0.5rem 0.6rem` | `var(--space-2)` |
| 425 | `font-size` | `0.85rem` | `var(--text-ui)` |
| 438 | `border-radius` | `4px` | `var(--radius-control)` |
| 439 | `padding` | `0.25rem 0.7rem` | `var(--space-1) var(--space-3)` |
| 443 | `font-size` | `0.8rem` | `var(--text-meta)` |
| 448 | `font-size` | `0.75rem` | `var(--text-meta)` |
| 452 | `margin-top` | `0.7rem` | `var(--space-3)` |
| 454 | `padding` | `0.4rem 0.6rem` | `var(--space-2)` |
| 455 | `font-size` | `0.85rem` | `var(--text-ui)` |
| 460 | `font-size` | `0.85rem` | `var(--text-ui)` |
| 475 | `margin-top` | `0.7rem` | `var(--space-3)` |
| 477 | `padding-top` | `0.6rem` | `var(--space-2)` |
| 479 | `gap` | `0.5rem` | `var(--space-2)` |
| 485 | `gap` | `0.5rem 0.75rem` | `var(--space-2) var(--space-3)` |
| 630 | `gap` | `0.25rem` | `var(--space-1)` |
| 631 | `font-size` | `0.85rem` | `var(--text-ui)` |
| 643 | `padding-left` | `0.5rem` | `var(--space-2)` |
| 650 | `font-size` | `0.85rem` | `var(--text-ui)` |
| 658 | `margin` | `0.35rem 0 0` | `var(--space-1) 0 0` |
| 660 | `padding-left` | `0.5rem` | `var(--space-2)` |
| 661 | `font-size` | `0.8rem` | `var(--text-meta)` |
| 667 | `border-radius` | `4px` | `var(--radius-control)` |
| 668 | `padding` | `0.25rem 0.9rem` | `var(--space-1) var(--space-4)` |
| 672 | `font-size` | `0.85rem` | `var(--text-ui)` |
| 709 | `border-radius` | `4px` | `var(--radius-control)` |
| 710 | `padding` | `0.2rem 0.6rem` | `var(--space-1) var(--space-2)` |
| 714 | `font-size` | `0.8rem` | `var(--text-meta)` |
| 726 | `padding` | `0.35rem 0.6rem` | `var(--space-1) var(--space-2)` |
| 728 | `font-size` | `0.85rem` | `var(--text-ui)` |

Three of those are worth knowing what they are, because they are the ones a reviewer will ask about:

- **348**, `.proposal-detail .fact` — `0.05rem` is 0.8 px, below the smallest step. `--space-1` is the only honest answer and it grows each fact row by 6.4 px; there are four or five rows a side. Looked at in the browser pass, not waved through.
- **283**, `.proposal-detail .side-title` — 15.2 px goes **up** to `--text-body`, not down to `--text-ui`, because 16 is the nearer step and this is an `<h2>`.
- **305**, `.proposal-detail .pages img` — the tile's `border-radius` tightens 3 px to 2 px. Its `width: 4.5rem` does **not** move; Task 4 pins that.

`border-left: 3px solid …` on `.no-rule` (451), `.refusals li` (640) and `.the-fold` (657) keeps its literal, for the same reason `.proposal-list li.strong`'s does: it is a strip's width.

- [ ] **Step 2: Confirm the diff is exactly 55 declarations**

```bash
cd /home/davey/projects/taiyaku && git diff --stat web/src/styles/admin.css
cd /home/davey/projects/taiyaku && git diff -U0 web/src/styles/admin.css | grep -c '^+' 
```
Expected: the second figure is 56 — 55 declarations plus the `+++` header line. If it is anything else, find the extra or missing change before going on.

- [ ] **Step 3: Confirm the range is empty of raw values**

```bash
cd /home/davey/projects/taiyaku && python3 - <<'EOF'
import re, pathlib
lines = pathlib.Path('web/src/styles/admin.css').read_text().split('\n')
body = '\n'.join(lines[212:492] + lines[612:730])
body = re.sub(r'/\*[\s\S]*?\*/', '', body)
print('raw rem:', sorted(set(re.findall(r'[\d.]+rem', body))))
print('raw px :', sorted(set(re.findall(r'\d+px', body))))
EOF
```
Expected exactly: `raw rem: ['0.7rem', '15rem', '1rem', '4.5rem']` and `raw px : ['1px', '3px']`. `0.7rem` is the micro-label on `.side-role`, `.lab` and `.accept-form .scope`; `1rem` is the sign column's `flex`; `4.5rem` is the tile; `15rem` is the fold.

- [ ] **Step 4: Run the suites**

```bash
cd web && npx vitest run --project node --reporter=default
cd web && npx vitest run --project jsdom --reporter=default
```
Expected: PASS on both. This range is read by `index.css.test.ts`'s contrast and naming checks and by `admin-rules.css.test.ts` through the shared `.field` block, which this task does not touch.

- [ ] **Step 5: Watch a mutation fail**

There is no new test here, so the check is the opposite one — **does anything at all notice this range?** Put `font-size: 0.85rem` back on `.proposal-detail .reasons` (line 394) and run the whole `node` project. If nothing goes red, that is the honest state and is recorded rather than papered over: the sweep's guarantee is Task 7's source-text gate and the browser pass, not a unit test. Restore.

- [ ] **Step 6: Commit**

```bash
git add web/src/styles/admin.css
git commit -m "⑤c: the evidence panel takes tokens

55 declarations, nearest step with ties to the smaller, generated from
the file rather than read off it. ⑤a's shared .field block is inside this
range and is deliberately untouched: it is six screens' rule and its two
raw values are not this screen's to spend.

Three that a reviewer should look at rather than scan past. .fact's
vertical padding was 0.8px, below the smallest step, and goes to 4px,
which grows each fact row by 6.4px over four or five rows a side.
.side-title goes up to --text-body rather than down to --text-ui, because
16px is the nearer step to 15.2 and it is an h2. Every 3px and 4px corner
goes to 2px.

A mutation putting one font-size back went unnoticed by every test, which
is the honest state of a sweep: what guards it is the source-text gate and
the browser pass, not a unit test."
```

---

## Task 3: The filter rows stop being one kind of row

O150. Spec §12.3. Drawings: `target/style-pass-5c/drawn/o150-A-1440.png` (today) and `-B-1440.png`/`-B-390.png` (what this builds).

Three controls, three treatments: the state row keeps pills, the kind row takes the underlined tab look, and "Show weak proposals" leaves the row and becomes a checkbox.

**Files:**
- Modify: `web/src/Proposals.tsx:181-241`
- Modify: `web/src/Library.tsx:143`
- Modify: `web/src/styles/library.css:1-54` and `:490-493`
- Modify: `web/src/styles/admin.css` — insert after the `.proposals .retry` rule; extend `.preview-tabs`, `.config-nav a[aria-current="page"]`, `.root-row .checks` and `.root-row .checks label`. **Line numbers are not given here on purpose**: Tasks 1 and 2 have already moved them, and every one of these is found by its selector.
- Modify: `web/src/index.css.test.ts:585-600`
- Modify: `web/src/Proposals.test.tsx:94-98, 244-251, 327-355`
- Test: `web/src/styles/admin-proposals.css.test.ts` (**new**, `node` project)

**Interfaces:**
- Consumes: Tasks 1 and 2, which have already tokenised this range — so the new rules below are written in tokens from the start and the line numbers in **Files** have shifted by the few lines those tasks' comment edits added. Locate by selector, not by line.
- Produces: `.proposal-states` / `.proposal-tabs` / `.proposals .checks` as the three row classes Tasks 2, 7 and 8 assert against; `weakBox(host)` as `Proposals.test.tsx`'s helper in place of `weakToggle(host)`.

- [ ] **Step 1: Write the failing test**

Create `web/src/styles/admin-proposals.css.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import adminSource from './admin.css?raw'
import { rules, selectors, withoutComments } from '../testing/css'

/**
 * The proposals inbox read from its stylesheet — spec 2026-09-17 §12.
 *
 * Only facts that are *defects* if they regress live here, the rule
 * `admin-rules.css.test.ts` states for the Rules screen. How the screen looks
 * is the browser pass's job (§5 item 2).
 */

/** The bodies of every rule whose selector list *contains* `selector`. */
function bodiesFor(selector: string): string[] {
  return rules(adminSource)
    .filter(({ prelude }) =>
      prelude.split(',').some((part) => part.trim() === selector),
    )
    .map(({ body }) => body)
}

function declared(body: string, property: string): string | undefined {
  const m = body.match(new RegExp(`(?:^|;)\\s*${property}:\\s*([^;]+)`))
  return m?.[1].trim()
}

/**
 * **The two rows are drawn as two different kinds of control, and that is the
 * whole of O150.** Style pass ② found the failure this pins by shipping it:
 * the underlined tab look reached the inbox and drew "All" and "Hide weak
 * proposals" with the same accent underline side by side, so the look was
 * pulled back off the row (spec §7.2, decided 2026-09-18). ⑤c earns it back by
 * taking the independent toggle out of the row, and this is what stops the two
 * converging again.
 *
 * Asserted on the *declarations*, not on a class name: a test that only
 * checked `.proposal-tabs` exists would pass against a row with no rules at
 * all, which is exactly the state this screen was in before this PR.
 */
describe('the inbox draws a filter and a tab differently', () => {
  it('draws the state row as pills', () => {
    const [body] = bodiesFor('.proposal-states .state')
    expect(body).toBeDefined()
    expect(declared(body, 'border-radius')).toBe('999px')
    expect(declared(body, 'background')).toBe('var(--surface)')
  })

  it('draws the kind row as underlined tabs, sharing the rule with the preview strip', () => {
    const [body] = bodiesFor('.proposal-tabs button')
    expect(body).toBeDefined()
    expect(declared(body, 'background')).toBe('none')
    expect(declared(body, 'border')).toBe('0')

    const current = bodiesFor('.proposal-tabs button[aria-pressed="true"]')
    expect(current).toHaveLength(1)
    expect(declared(current[0], 'box-shadow')).toBe('inset 0 -2px 0 var(--accent)')

    // **One rule, three preludes** — §12.3. Asserted on the prelude rather
    // than on the body, because three separate rules with identical bodies
    // pass every assertion above and are exactly what this is against: they
    // can drift apart one edit later, and the app marks a chosen one-of-N
    // two different ways, so there is real pressure to split them.
    const rule = rules(adminSource).find(({ prelude }) =>
      selectors(prelude).includes('.proposal-tabs button[aria-pressed="true"]'),
    )
    expect(rule).toBeDefined()
    expect(selectors(rule!.prelude).sort()).toEqual([
      '.config-nav a[aria-current="page"]',
      '.preview-tabs button[aria-current="true"]',
      '.proposal-tabs button[aria-pressed="true"]',
    ])
  })

  it('gives the two rows no shared treatment', () => {
    // The regression ② shipped: one selector matching both rows, drawing
    // them alike. `.chips` was that selector and is gone from this app.
    // Comments are stripped first — this file's head comment tells the
    // story of `.chips` and must go on being allowed to.
    expect(withoutComments(adminSource)).not.toContain('.chips')
  })
})
```

- [ ] **Step 2: Run it and watch it fail**

```bash
cd web && npx vitest run --project node src/styles/admin-proposals.css.test.ts --reporter=default
```
Expected: FAIL on the first two — no `.proposal-states .state` or `.proposal-tabs button` rule exists in `admin.css`. The third passes already and is there for later.

- [ ] **Step 3: Move the pills out of `library.css`**

Replace `web/src/styles/library.css:1-54` — the whole `.chips` block and its four `.chips.library-chips` descendants — with a self-sufficient `.library-chips`. The four declarations it was *inheriting* rather than overriding are `display`, `flex-wrap` (from `.chips`) and `font`, `cursor` (from `.chips button`); they come back here.

```css
/* The library's own filter row, as underlined tabs — the mockup the user
 * chose (spec 2026-09-17 §7.2): the pressed one in the text colour over the
 * accent, the rest muted. "Added" (`.sort`) sits at the row's far end,
 * because it sorts where the others filter.
 *
 * **This was `.chips.library-chips` until ⑤c.** The bare `.chips` it
 * qualified held the proposals inbox's pills — two consumers, both in
 * `Proposals.tsx` — and lived here for as long as that screen was unstyled.
 * ⑤c gave the inbox its own rules in `admin.css`, so the co-class had nothing
 * left to qualify and the four declarations this rule used to inherit from it
 * are written out below. */
.library-chips {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: var(--space-1) var(--space-5);
  margin: var(--space-5) 0 var(--space-4);
}

.library-chips button {
  padding: var(--space-1) 0;
  border: 0;
  border-radius: 0;
  background: none;
  color: var(--muted);
  font: inherit;
  font-size: var(--text-ui);
  cursor: pointer;
}

.library-chips button[aria-pressed="true"] {
  color: var(--text);
  box-shadow: inset 0 -2px 0 var(--accent);
}

.library-chips .sort {
  margin-left: auto;
}
```

The pressed rule loses its `background: none`: it was there to beat `.chips button[aria-pressed="true"]`'s accent fill, and that rule no longer exists.

Then at `library.css:491`, inside the existing `@media (pointer: coarse)` block:

```css
  .library-chips button {
    min-height: 44px;
  }
```

- [ ] **Step 4: Give the inbox its own rules in `admin.css`**

First replace the stale paragraph in the file's head comment — the one beginning `**The tab row has no rules here.**` and ending `carrying no rules of its own.` — with:

```
 * **The three filter controls are three kinds of control**, and the rules for
 * them are below. Style pass ② found the failure by shipping it: the
 * underlined tab look reached this screen and drew "All" and "Hide weak
 * proposals" with the same accent underline side by side, so it was pulled
 * back to the library's own row. ⑤c earns it back by moving the independent
 * toggle out of the row (O150) — the state row keeps pills, the kind row
 * takes the tab look, and the threshold is a checkbox on a line of its own.
```

Then insert after `.proposals .retry` (which ends at line 48) and before `.proposal-list`:

```css
/* One of three states — the coarser of the screen's two one-of-N filters, so
 * it reads as "which inbox" above the kind row's "which kind". Pills rather
 * than tabs for that reason, and because two identical underlined strips
 * stacked give no hierarchy at all; drawn both ways before it was chosen
 * (spec §12.3, `drawn/o150-B-1440.png` against `-D-`).
 *
 * These four rules were `library.css`'s bare `.chips` until ⑤c, whose own
 * comment said they were this screen's and were waiting for ⑤. */
.proposal-states {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  margin: var(--space-4) 0;
}

.proposal-states .state {
  padding: var(--space-1) var(--space-3);
  border: 1px solid var(--line-strong);
  border-radius: 999px;
  background: var(--surface);
  color: var(--text);
  font: inherit;
  cursor: pointer;
}

.proposal-states .state[aria-pressed="true"] {
  border-color: var(--accent);
  background: var(--accent);
  color: var(--on-accent);
}

/* The kind row sits under the state row rather than beside it, so it needs a
 * top margin the preview strip does not; everything else it takes from the
 * shared tab rules further down this file. */
.proposal-tabs {
  margin-top: var(--space-4);
}

/* The threshold is an independent toggle and so is a checkbox — the same
 * treatment ⑤b gave the permission and grant boxes, whose labels are
 * sentences rather than field names (spec §11.3). `.proposals .checks` joins
 * their two rules below; only the bottom margin is this row's own, because
 * the list follows it directly. */
.proposals .checks {
  margin-bottom: var(--space-4);
}
```

- [ ] **Step 5: Add the two new subjects to the three shared rules**

`admin.css:747` — `.config-nav ul` is a `<ul>` of links with its own gap and margin and is **not** merged; only the current-item rule is.

`admin.css:768`, `.preview-tabs` gains `.proposal-tabs`:

```css
.preview-tabs,
.proposal-tabs {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-5);
  margin-bottom: var(--space-4);
  padding-bottom: var(--space-1);
  border-bottom: 1px solid var(--line);
}

.preview-tabs button,
.proposal-tabs button {
  padding: var(--space-1) 0;
  border: 0;
  border-radius: 0;
  background: none;
  color: var(--muted);
  font: inherit;
  font-size: var(--text-ui);
  cursor: pointer;
}
```

Delete the standalone `.config-nav a[aria-current="page"]` rule (`admin.css:763-766`) and the standalone `.preview-tabs button[aria-current="true"]` rule (`admin.css:788-791`), and write one rule in their place, directly after the two above:

```css
/* The current one of a one-of-N row, wherever the row is. Three preludes
 * because the app marks a chosen one two different ways — `aria-current` on
 * the config nav's links and the preview strip's buttons, `aria-pressed` on
 * the inbox's. That disagreement is real and is the accessibility pass's to
 * settle (spec §12.8); what must not happen meanwhile is the three drifting
 * apart visually, which one rule prevents and three copies would not. */
.config-nav a[aria-current="page"],
.preview-tabs button[aria-current="true"],
.proposal-tabs button[aria-pressed="true"] {
  color: var(--text);
  box-shadow: inset 0 -2px 0 var(--accent);
}
```

`admin.css:1287` and `:1294` each gain `.proposals .checks`:

```css
.root-row .checks,
.user-row .checks,
.proposals .checks {
  display: grid;
  gap: var(--space-2);
  margin-top: var(--space-3);
}

.root-row .checks label,
.user-row .checks label,
.proposals .checks label {
  display: flex;
  align-items: baseline;
  gap: var(--space-2);
  font-size: var(--text-ui);
}
```

- [ ] **Step 6: Change the markup**

`web/src/Library.tsx:143`:

```tsx
      <div className="library-chips">
```

`web/src/Proposals.tsx` — replace the two filter rows and their comments (lines 193–241) with:

```tsx
      {/* The three states, as C3 §4's "their own filters". A row of its own
          because a state and a kind are two filters, not two values of one —
          and pills rather than tabs because it is the coarser of the two
          (spec §12.3). */}
      <div className="proposal-states">
        {PROPOSAL_STATES.map(({ state: s, label }) => (
          <button
            key={s}
            type="button"
            className="state"
            aria-pressed={state === s}
            onClick={() => setState(s)}
          >
            {label}
          </button>
        ))}
      </div>
      {/* One-of-N, so it wears the underlined tab look `config-nav` and the
          rule preview wear (§12.3). It could not until O150: the score
          threshold used to sit in this row wearing the same pill as a kind,
          and ②'s review found the two drawing the same accent underline side
          by side. */}
      <div className="proposal-tabs">
        <button
          type="button"
          className="tab"
          aria-pressed={kind === undefined}
          onClick={() => setKind(undefined)}
        >
          All {total}
        </button>
        {tabs.map(([k, n]) => (
          <button
            key={k}
            type="button"
            className="tab"
            aria-pressed={kind === k}
            onClick={() => setKind(k)}
          >
            {kindLabel(k)} {n}
          </button>
        ))}
      </div>
      {/* **The label does not flip.** It read "Show weak proposals" or "Hide
          weak proposals" while `aria-pressed` said the same thing beside it —
          the same fact twice, disagreeing for as long as a click was in
          flight. A checkbox says its state with its box. It also makes
          `EmptyState`'s copy true: that component points at "Show weak
          proposals" by name, and used to do so while the control read
          "Hide". */}
      <div className="checks weak-check">
        <label>
          <input
            type="checkbox"
            checked={showWeak}
            onChange={(e) => {
              setShowWeak(e.target.checked)
            }}
          />
          Show weak proposals
        </label>
      </div>
```

- [ ] **Step 7: Update `Proposals.test.tsx`**

Replace the helper at lines 94–98:

```ts
/** The one control that reveals weak proposals, by class rather than by label. */
function weakBox(host: HTMLElement): HTMLInputElement {
  const box = host.querySelector<HTMLInputElement>(
    '.weak-check input[type="checkbox"]',
  )
  if (!box) throw new Error('no weak-proposals control rendered')
  return box
}
```

Every `weakToggle(` call site becomes `weakBox(`. At lines 244–251, replace the `chips` assertion:

```ts
    // The kind row is one-of-N and wears the tab look; the state row is the
    // coarser filter and keeps pills; the threshold is neither and sits
    // outside both (O150, spec §12.3). jsdom computes no styles, so what a
    // test here can hold is that the three are three different elements —
    // `admin-proposals.css.test.ts` holds that they are drawn differently.
    expect(host.querySelector('.proposal-tabs')).not.toBeNull()
    expect(host.querySelector('.proposal-tabs .weak-toggle')).toBeNull()
    expect(host.querySelector('.proposal-tabs input')).toBeNull()
    expect(host.querySelector('.weak-check input[type="checkbox"]')).not.toBeNull()
```

At lines 327–355, the label no longer flips, so the assertions read the box:

```ts
    const weak = weakBox(host)
    expect(weak.checked).toBe(false)
    // The label is stable now — asserted once, because a label that changed
    // under a state also drawn was the defect O150's answer removed.
    expect(weak.closest('label')?.textContent?.trim()).toBe(
      'Show weak proposals',
    )

    await click(weak)
    expect(lastCall()).toEqual({
      state: 'open',
      kind: 'edition-of',
      min_score: 0,
    })
    expect(weak.checked).toBe(true)

    // A real toggle, not a one-way switch a fixture that clicked only once
    // could not tell apart from this: pressing it again must go back to the
    // default threshold — and must still not touch the kind.
    await click(weak)
    expect(lastCall()).toEqual({
      state: 'open',
      kind: 'edition-of',
      min_score: 1,
    })
    expect(weak.checked).toBe(false)
```

- [ ] **Step 8: Rewrite the pill/tab test in `index.css.test.ts`**

First widen its import at line 7 — the file takes `rules, selectors, subject` today and needs one more:

```ts
import { rules, selectors, subject, withoutComments } from './testing/css'
```

`cssSource` (line 37) is `shell + library + admin`, which is the right scope: `.chips` only ever lived in two of those three.

Then replace lines 585–600 entirely:

```ts
/**
 * **Two rows of one-of-N buttons, drawn two ways, and a third control that is
 * neither.** The library's filter row and the inbox's kind row take the
 * underlined tab look; the inbox's state row keeps pills, because it is the
 * coarser filter and two identical strips stacked give no hierarchy (spec
 * §12.3). What this pins is that no single selector reaches both kinds — the
 * regression ② shipped, when `.chips` drew "All" and "Hide weak proposals"
 * with the same accent underline side by side.
 */
describe('a tab row and a pill row stay two different rows', () => {
  it('draws the library’s row and the inbox’s kind row as tabs', () => {
    for (const sel of ['.library-chips button', '.proposal-tabs button']) {
      const body = ruleBody(sel)
      expect(declared(body, 'background')).toBe('none')
      expect(declared(body, 'border-radius')).toBe('0')
    }
  })

  it('draws the inbox’s state row as pills', () => {
    const body = ruleBody('.proposal-states .state')
    expect(declared(body, 'border-radius')).toBe('999px')
    expect(declared(body, 'background')).toBe('var(--surface)')
  })

  it('has no `.chips` left anywhere', () => {
    // `.chips` was the selector that reached both kinds. Its absence is the
    // fact, not its replacement: a `.chips` written again would draw the
    // inbox's three controls alike whatever the rules below say. Comments
    // are stripped first: `admin.css` and `library.css` both explain in
    // prose what `.chips` was, and should keep being able to.
    expect(withoutComments(cssSource)).not.toContain('.chips')
  })
})
```

- [ ] **Step 9: Correct the caller count `testing/css.ts` states about itself**

`web/src/testing/css.ts`'s doc comment says **six** files call it and names them — and says why the number is written down: *"The count is stated because it is what decides whether this module's shape can change; an open item quoted 'three' from an earlier version of this sentence and was wrong by half."*

This branch adds two callers — `styles/admin-proposals.css.test.ts` here, and `Account.test.tsx` in Task 6. Update the comment to **eight** and list both. Do it here rather than in Task 6 so the count is right at every commit: add `admin-proposals.css.test.ts` now and say *seven*; Task 6 takes it to eight when it adds the second.

```
 * Shared parsing for the tests that read a stylesheet's own text rather than
 * render it. Seven call it: `../index.css.test.ts`, `../theme.test.ts`, and
 * `../styles/`'s `base.css.test.ts`, `admin-rules.css.test.ts`,
 * `admin-config.css.test.ts`, `admin-proposals.css.test.ts` and
 * `reader.css.test.ts`. **The count is stated because it is what decides
 * whether this module's shape can change**: an open item quoted "three" from
 * an earlier version of this sentence and was wrong by half.
```

- [ ] **Step 10: Run the three suites**

```bash
cd web && npx vitest run --project node src/styles/admin-proposals.css.test.ts src/index.css.test.ts --reporter=default
cd web && npx vitest run --project jsdom src/Proposals.test.tsx src/Library.test.tsx --reporter=default
```
Expected: PASS on both.

- [ ] **Step 11: Watch each new assertion fail against its own defect**

Four mutations, each applied by hand, the suite run, and the edit reverted:

1. In `admin.css`, change `.proposal-states .state`'s `border-radius: 999px` to `var(--radius-control)`. Expected: `draws the state row as pills` and `draws the inbox’s state row as pills` go red.
2. Add `.proposal-tabs` to the `.proposal-states` selector list. Expected: the pill assertions still pass *and* `draws the kind row as underlined tabs` still passes — **this mutation does not go red**, which is the point: the two rules would both match and source order decides. Record it, and add the guard it asks for:

```ts
  it('gives no selector both rows', () => {
    // The mutation that found this: adding `.proposal-tabs` to the pill
    // rule's selector list leaves every other assertion here green, because
    // both rules still exist and match. What goes wrong is that one element
    // takes both.
    for (const { prelude } of rules(adminSource)) {
      const parts = prelude.split(',').map((p) => p.trim())
      const pill = parts.some((p) => p.startsWith('.proposal-states'))
      const tab = parts.some((p) => p.startsWith('.proposal-tabs'))
      expect(pill && tab).toBe(false)
    }
  })
```
   Re-apply the mutation and confirm the new test goes red. Revert.
3. In `Proposals.tsx`, put the checkbox back inside `.proposal-tabs`. Expected: `expect(host.querySelector('.proposal-tabs input')).toBeNull()` goes red.
4. In `Proposals.tsx`, change the checkbox's `onChange` to `setShowWeak((w) => !w)` ignoring `e.target.checked`. Expected: **still green** — the two are equivalent for a click. Do not add a test for it; record in the commit that the mutation was tried and is not a defect.

- [ ] **Step 12: Commit**

```bash
git add web/src/testing/css.ts web/src/Proposals.tsx web/src/Library.tsx web/src/styles/library.css web/src/styles/admin.css web/src/styles/admin-proposals.css.test.ts web/src/index.css.test.ts web/src/Proposals.test.tsx
git commit -m "⑤c: the inbox's filter rows stop being one kind of row

O150. The kind row is one-of-N and now wears the underlined tab look the
config nav and the rule preview wear; the state row keeps pills, because
it is the coarser filter and two identical strips stacked say nothing
about which is which; and the score threshold leaves the row entirely to
become a checkbox, which is what it always was.

The pills come with it. library.css's bare .chips had exactly two
consumers, both in Proposals.tsx, and its own comment said it was holding
them until this PR. .library-chips is self-sufficient now, with the four
declarations it used to inherit written out.

One current-item rule with three preludes, because the app marks a chosen
one-of-N two different ways. Which attribute is right is the accessibility
pass's question; the three drifting apart visually is not, and one rule is
what stops it.

A mutation that adds .proposal-tabs to the pill rule's selector list left
every assertion green, so there is now a test for the thing that mutation
breaks. A second, rewriting the checkbox's onChange as a functional
toggle, is equivalent for a click and is not a defect."
```

---

## Task 4: O148 — the restated tile height, pinned

Spec §12.6. `ProposalRow.tsx`'s `TILE_HEIGHT_PX = 108` restates `admin.css`'s `.proposal-detail .pages img` in TypeScript, and nothing checks the two agree. **Setting it to `1` passes the entire suite** — verified by hand on O134's branch — because `devicePixelRatio` is 1 in jsdom and every height from 1 to 160 rounds to the same bottom rung there.

**The constant moves to `tileSize.ts`, and that is why this task is shaped the way it is.** `admin-proposals.css.test.ts` is a `node` test, and **no `node` test in this tree imports a `.tsx`** — the project is "pure TypeScript with no DOM" by its vite config's own comment, so importing `ProposalRow.tsx` to reach one number would be a first and an unnecessary one. `tileSize.ts` is pure, already holds `requestedTileEdge` and the strip's `STRIP_TILE_PX`/`STRIP_BREAKPOINT_PX`, and is already imported by `styles/reader.css.test.ts` — which is the precedent O148's row names. Putting the inbox's restatement beside the reader's puts both in one pure module with one guard each.

**Files:**
- Modify: `web/src/tileSize.ts` — `TILE_HEIGHT_PX` arrives, after `STRIP_TILE_PX`
- Modify: `web/src/ProposalRow.tsx:110-124` — the constant leaves; it is imported instead
- Test: `web/src/styles/admin-proposals.css.test.ts` (created by Task 3)

**Interfaces:**
- Consumes: Task 3's `admin-proposals.css.test.ts` and its `bodiesFor` / `declared` helpers.
- Produces: `TILE_HEIGHT_PX` exported from `web/src/tileSize.ts`.

- [ ] **Step 1: Write the failing test**

Append to `web/src/styles/admin-proposals.css.test.ts`:

```ts
import { TILE_HEIGHT_PX } from '../tileSize'

/**
 * **O148.** `TILE_HEIGHT_PX` is the box `.proposal-detail .pages img` draws,
 * restated in TypeScript so `requestedTileEdge` can pick a rung for it. The
 * reader's filmstrip carries the same restatement and has been pinned against
 * its own stylesheet since ③ (`styles/reader.css.test.ts`); the inbox got the
 * restatement without the guard, so changing the CSS left the constant
 * silently wrong and every test green. Setting it to `1` passed the whole
 * suite, because `devicePixelRatio` is 1 in jsdom and every height from 1 to
 * 160 rounds to the same bottom rung there.
 *
 * **The box has two heights and this asserts the right one.** The rule
 * carries `border: 1px solid` with no `box-sizing`, so the *content* box is
 * 72×108 and the *border* box is 74×110 — both measured in Chromium. The
 * constant is the content height, which is correct: that is the box the
 * artwork is scaled into and the number `requestedTileEdge` chooses a rung
 * for. A test that derived from the drawn box would assert 110 and fail
 * against right code; one that ignored the border would pass either way.
 */
describe('the inbox tile height follows the stylesheet', () => {
  it('derives TILE_HEIGHT_PX from the rule that draws the tile', () => {
    const [body] = bodiesFor('.proposal-detail .pages img')
    expect(body).toBeDefined()

    const width = declared(body, 'width')
    expect(width).toMatch(/^[\d.]+rem$/)
    const ratio = declared(body, 'aspect-ratio')
    expect(ratio).toBe('2 / 3')

    // The content box: `width` is a content width, and the 1px border sits
    // outside it. 4.5rem × 16 = 72; 72 × 3 ÷ 2 = 108.
    const contentWidthPx = Number.parseFloat(width!) * 16
    const [w, h] = ratio!.split('/').map((n) => Number(n.trim()))
    expect(TILE_HEIGHT_PX).toBe((contentWidthPx * h) / w)
  })

  // **One case, not two.** A second `it()` here asserted `object-fit` and
  // `display` and claimed those guard the derivation's two inputs. Neither
  // property affects the box's dimensions, and both hazards are already
  // carried above — mutating `width` to `100%` reddens the `toMatch` while
  // that second case stayed green. Deleted rather than re-commented.
})
```

This import is the whole reason the constant moves: `styles/reader.css.test.ts` already reads `STRIP_TILE_PX` and `STRIP_BREAKPOINT_PX` out of `../tileSize` in exactly this way, and following it costs nothing.

- [ ] **Step 2: Move the constant, with its comment rewritten**

**This has to happen before Step 3 runs the test**, because Step 1 writes an import from `tileSize.ts` and the constant does not live there yet. An earlier draft numbered this step fourth, and running the plan literally gave `expected undefined to be 108`. Found by ⑤c's Task 4 implementer, who reordered and said so.

Delete `TILE_HEIGHT_PX` and its comment from `web/src/ProposalRow.tsx` and import it instead:

```ts
import { requestedTileEdge, TILE_HEIGHT_PX } from './tileSize'
```

In `web/src/tileSize.ts`, after `STRIP_TILE_PX`:

```ts
/**
 * The height an *inbox* page tile is drawn at — the proposals evidence
 * panel's, not the reader's strip, whose two sizes are `STRIP_TILE_PX` above.
 * Both are restatements of a stylesheet in TypeScript and both live here so
 * that each can be pinned against the file that draws it; `STRIP_TILE_PX` has
 * been since ③ and this one was not, which is O148.
 *
 * From `styles/admin.css`'s
 * `.proposal-detail .pages img`: `width: 4.5rem` at `aspect-ratio: 2 / 3`, so
 * a 72×108 content box. A constant rather than a measurement because the box
 * is fixed on purpose — three tiles of different widths would make the two
 * sides of a proposal impossible to read across, which is the one thing the
 * row exists for.
 *
 * **The content box, not the drawn one.** That rule also carries a 1px
 * border with no `box-sizing`, so what a browser reports for the element is
 * 74×110. 108 is right: it is the box the artwork is scaled into, and the
 * number `requestedTileEdge` picks a rung for.
 *
 * At 3× this box wants 324 px and was being sent 160 (O134, closing O133's
 * "the proposals inbox's page tiles keep the 160 px tile").
 *
 * **Pinned against the stylesheet by `styles/admin-proposals.css.test.ts`**
 * (O148). It was not, and setting this to `1` passed the entire suite:
 * `devicePixelRatio` is 1 in jsdom and every height from 1 to 160 rounds to
 * the same bottom rung there. The reader's filmstrip has carried the same
 * guard since ③; this is the asymmetry that row was about.
 */
export const TILE_HEIGHT_PX = 108
```

- [ ] **Step 3: Run it and watch it pass**

```bash
cd web && npx vitest run --project node src/styles/admin-proposals.css.test.ts --reporter=default
```
Expected: PASS. It should — the constant is correct today. The test exists to keep it so, which means Step 3 is the step that matters.

- [ ] **Step 4: Watch it fail, four ways**

This is the task's real work. Each mutation applied by hand, the suite run, the edit reverted.

1. `TILE_HEIGHT_PX = 1`. Expected: RED, `expected 1 to be 108`. This is the mutation that passed the entire suite before this test existed — run the **whole** `node` and `jsdom` projects with it in place and confirm the new test is the only thing that goes red.
2. `TILE_HEIGHT_PX = 110` — the border box rather than the content box. Expected: RED. A test written against the drawn box would call this correct.
3. In `admin.css`, `width: 4.5rem` → `width: 5rem`. Expected: RED, `expected 108 to be 120`. This is the direction the row is actually about: the CSS moves and the constant does not.
4. In `admin.css`, drop `aspect-ratio: 2 / 3`. Expected: RED on `expect(ratio).toBe('2 / 3')` rather than on the arithmetic — the assertion that the derivation still has both its inputs.

- [ ] **Step 5: Run both projects**

```bash
cd web && npx vitest run --project node --reporter=default
cd web && npx vitest run --project jsdom --reporter=default
```
Expected: PASS on both.

- [ ] **Step 6: Commit**

```bash
git add web/src/tileSize.ts web/src/ProposalRow.tsx web/src/styles/admin-proposals.css.test.ts
git commit -m "⑤c: pin the inbox's tile height against the CSS that draws it

O148. TILE_HEIGHT_PX restated .proposal-detail .pages img in TypeScript
and nothing checked the two agreed, so setting it to 1 passed the entire
suite — devicePixelRatio is 1 in jsdom and every height from 1 to 160
rounds to the same rung there. The reader's filmstrip has been pinned
against its own stylesheet since the reader was styled; the inbox got the
restatement without the guard, and that asymmetry is what the row was
about.

The constant moves from ProposalRow.tsx to tileSize.ts, beside the strip's
own two. The node project holds no test that imports a .tsx and should not
gain one to reach a number; tileSize.ts is pure, already holds
requestedTileEdge, and is already read this way by reader.css.test.ts —
which is the precedent the row itself names.

The derivation is from the content box. The rule carries a 1px border
with no box-sizing, so the drawn box is 74x110 and the content box is
72x108; 108 is the right one, because it is the box the artwork is scaled
into. A test written against the drawn box would assert 110 and fail
against correct code, so that mutation is one of the four watched here."
```

---

## Task 5: The Account panel becomes a reader dialog

Spec §12.5. Drawings: `drawn/o-acct-A-today.png` (today) against `drawn/o-acct-B-reader-dialog.png`.

**Files:**
- Modify: `web/src/Account.tsx:37-124` (the `CSSProperties` objects and `LOOKS`) and `:255-429` (the render)
- Modify: `web/src/styles/reader.css` — three rules after `.reader-field`
- Modify: `web/src/colourLiterals.test.ts` — drop `'./Account.tsx': 10`
- Test: `web/src/Account.browser.test.tsx` (**new**, `browser` project)
- Test: `web/src/Account.test.tsx`

**The allowance entry goes in this commit, not Task 8's.** `ALLOWED` is compared with `toEqual`, so a file *under* its entry fails exactly as loudly as one over it. This task takes `Account.tsx` from ten literals to zero, so leaving the entry at 10 would make `colourLiterals.test.ts` red from here until Task 8 — and a knowingly-red commit is the process failure ⑤a recorded against itself and ⑤b's pre-flight split a task to avoid. Task 8 keeps the `annotations.css` half, which is a decision rather than a consequence.

**Interfaces:**
- Consumes: nothing.
- Produces: `LOOKS` as `Record<'panel' | 'page', Look>` — a map of **class names**, keyed `section`, `heading`, `label`, `field`, `wide`, `signOut`, `secret`, `failure`, `note`, `row`, `meta`, `button`. Task 6 fills the `page` half of the same map, so every key must exist in the interface even where the panel leaves it undefined.

- [ ] **Step 1: Write the failing test**

Create `web/src/Account.browser.test.tsx`:

```tsx
import { describe, expect, it } from 'vitest'
import { Account } from './Account'
import './index.css'
import { mount } from './testing/mount'

/**
 * **The account panel is one of the reader's dialogs, measured rather than
 * asserted by class name.** ③ drew Settings, Help, the note list and Credits
 * as `.reader-dialog-panel`; this one was 25 inline `style=` props drawing a
 * different dialog — 12px corners against 0, a drop shadow against none,
 * `rgba(24,24,24,0.96)` against `--reader-panel`, 16px body against 14, a
 * sans title against a serif one. ⑤c makes it the same panel.
 *
 * In the browser project because jsdom computes no styles: a class-name
 * assertion passes against a panel whose rules never loaded, which is the
 * failure this file exists to rule out.
 *
 * **Mounted inside `<div className="reader">`, which is load-bearing rather
 * than scenery.** `App` renders `<Account>` as a sibling of `<Settings>`
 * inside that element, and `reader.css` sets `color-scheme: dark` on it. The
 * same classes outside it would resolve `--text` to its *light* value over a
 * fixed dark ground. A fixture that mounted the panel bare would pass every
 * assertion below and prove nothing about the screen.
 */
function panel(): HTMLElement {
  const host = mount(
    <div className="reader">
      <Account visible onClose={() => {}} onSignedOut={() => {}} />
    </div>,
  )
  const el = host.querySelector<HTMLElement>('.reader-dialog-panel')
  if (!el) throw new Error('no .reader-dialog-panel rendered')
  return el
}

describe('the account panel', () => {
  it('is drawn as a reader dialog, not as its own', () => {
    const el = panel()
    const style = getComputedStyle(el)
    // The four the old panel differed on, read off the box rather than the
    // class list.
    expect(style.borderRadius).toBe('0px')
    expect(style.boxShadow).toBe('none')
    expect(Math.round(el.getBoundingClientRect().width)).toBe(480)
    expect(style.fontSize).toBe('14px')

    const scrim = el.closest('.reader-dialog')
    expect(scrim).not.toBeNull()
    // The reader's scrim, not a hand-mixed black. Compared against the panel
    // ③ already draws rather than against a literal, so the assertion
    // survives the token changing.
    expect(getComputedStyle(scrim!).backgroundColor).not.toBe(
      'rgba(0, 0, 0, 0.6)',
    )
  })

  it('resolves its colours dark although the document is light', () => {
    // The containment check, stated as a colour rather than as a DOM path:
    // outside `.reader` this computes the *light* `--text` over a fixed dark
    // ground, which is the failure `LOOKS` was written to avoid, inverted.
    document.documentElement.setAttribute('data-theme', 'paper')
    const el = panel()
    expect(getComputedStyle(el).color).toBe('rgb(236, 228, 212)')
    document.documentElement.removeAttribute('data-theme')
  })

  it('gives its fields the panel’s own font size, which is what puts them inside O151’s floor', () => {
    const field = panel().querySelector<HTMLInputElement>(
      'input[type="password"]',
    )
    expect(field).not.toBeNull()
    // 14px, inherited from the panel — not 16px by accident of never having
    // been given a size. The coarse-pointer floor above it is what takes it
    // back to 16 on a phone, and `base.css.test.ts` pins that pairing.
    expect(getComputedStyle(field!).fontSize).toBe('14px')
  })

  it('carries no inline style attribute anywhere', () => {
    const styled = [...panel().querySelectorAll('[style]')].map(
      (el) => `${el.tagName}: ${el.getAttribute('style')}`,
    )
    expect(styled).toEqual([])
  })
})
```

- [ ] **Step 2: Run it and watch it fail**

```bash
cd web && npx vitest run --project browser src/Account.browser.test.tsx --reporter=default
```
Expected: FAIL on the first assertion — there is no `.reader-dialog-panel`.

- [ ] **Step 3: Add the three rules `reader.css` is missing**

③'s panels are lists of settings with their controls at the right-hand end; none of them has a form, so none needed a field that spans the panel or a button that does. Insert after `.reader-field` (`reader.css:448-458`):

```css
/* A field that spans the panel, and the label above it. ③'s own panels put
 * their controls at the right-hand end of a `.reader-section-head` row —
 * none of them asks anybody to type, so this is the first panel that needs
 * a form shape. ⑤c's account panel is the first (spec §12.5). */
.reader-field-block {
  display: block;
  width: 100%;
  margin-top: var(--space-1);
}

/* An action that applies to the whole section rather than to a row in it:
 * Change password, Sign out. Full width for the same reason it is full
 * width in a native sheet — it is the section's verb, not one of its rows. */
.reader-button-wide {
  display: block;
  width: 100%;
  margin-top: var(--space-3);
}

/* The one-time API key secret. `.account .secret` is the page's copy of
 * this; the two differ only in their ground, which each stylesheet owns. */
.reader-secret {
  margin-top: var(--space-2);
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--reader-hair);
  font-family: ui-monospace, monospace;
  word-break: break-all;
}
```

- [ ] **Step 4: Replace `Account.tsx`'s style objects with class names**

Delete `overlayStyle`, `panelStyle`, `sectionStyle`, `fieldStyle`, `secretStyle` and `rowStyle` (`Account.tsx:37-89`) and the `CSSProperties` import. `LOOKS` becomes:

```tsx
/** The class each placement paints a part with. */
interface Look {
  section?: string
  /** On the `<label>`. The page uses `.field`; the panel wants none. */
  label?: string
  field?: string
  wide?: string
  signOut?: string
  secret?: string
  failure?: string
  note?: string
  row?: string
  meta?: string
  button?: string
  heading?: string
}

/**
 * **Two placements, two vocabularies, one component.**
 *
 * The panel is one of the reader's dialogs and takes ③'s classes; the page
 * is a settings screen inside `.library` and takes `admin.css`'s. This map
 * was a map of `CSSProperties` until ⑤c, and the panel's half of it was 25
 * inline `style=` props and 10 colour literals drawing a dialog that matched
 * none of the four beside it.
 *
 * **The panel's colours are the reader's and resolve dark in both themes**,
 * because `App` renders `<Account>` as a sibling of `<Settings>` inside
 * `<div className="reader">`, where `reader.css` sets `color-scheme: dark`.
 * That containment is load-bearing: the same classes *outside* `.reader`
 * would draw `--text` pale-on-paper over a fixed dark ground.
 */
const LOOKS: Record<'panel' | 'page', Look> = {
  panel: {
    section: 'reader-section',
    heading: 'reader-panel-heading',
    // No label class: a reader panel puts the word above the box as plain
    // text. `.field`'s uppercase micro-label is the settings area's idiom,
    // and the page takes it in Task 6.
    field: 'reader-field reader-field-block',
    wide: 'reader-button reader-button-wide',
    signOut: 'reader-button reader-button-wide',
    secret: 'reader-secret',
    failure: 'reader-failure',
    note: 'reader-success',
    row: 'reader-section-head reader-section-head-stacked',
    meta: 'reader-meta',
    button: 'reader-button',
  },
  page: {},
}
```

`reader-failure` and `reader-success` are two more rules, and they are colour rather than box, so they go beside the others in `reader.css`:

```css
/* A failure and a confirmation inside a panel. `--danger` and `--success`
 * resolve to their dark values here because `.reader` sets
 * `color-scheme: dark`.
 *
 * **The floor on this ground is already asserted**, by `index.css.test.ts`'s
 * "keep every Ink text colour above 4.5:1 on a panel over black" and its
 * white-artwork twin: both composite `--reader-panel` over `--reader-ground`
 * and run every one of `TEXT_TOKENS`, which includes these two. The measured
 * 7.99:1 and 9.23:1 are a record of what it draws, not the guarantee that it
 * clears AA. */
.reader-failure {
  color: var(--danger);
}

.reader-success {
  color: var(--success);
}
```

**And a third box rule, which the first draft of this task did not have.** `.reader-section-head` carries no margin, because its only user before this renders one row per section. The Account panel stacks it, so a second class carries the separation and ③'s four panels cannot move:

```css
.reader-section-head-stacked {
  margin-top: var(--space-2);
}
```

Two names rather than a margin on `.reader-section-head` itself: that class is ③'s, and the requirement was that its four existing panels render identically — which a rule they never match satisfies mechanically rather than by measurement.

- [ ] **Step 5: Replace the render's inline props**

Every `style={look.x}` becomes `className={look.x}`, and the six hard-coded objects go:

| was | becomes |
|---|---|
| `<p role="alert" style={look.failure}>` | `<p role="alert" className={look.failure}>` |
| `<p style={look.note}>` | `<p className={look.note}>` |
| `<section style={look.section}>` | `<section className={look.section}>` |
| `<h3 style={{ margin: 0, fontSize: '1em' }}>` | `<h3 className={look.heading}>` |
| `<label>` (the two password labels and the key-name one) | `<label className={look.label}>` |
| `<input style={look.field} …>` | `<input className={look.field} …>` |
| `<button type="submit" style={look.wide}>` | `<button type="submit" className={look.wide}>` |
| `<p style={{ fontSize: '0.9em', opacity: 0.8 }}>` | `<p className={look.meta}>` |
| `<form onSubmit={create} style={rowStyle}>` | `<form onSubmit={create} className={look.row}>` |
| `<div className="secret" style={look.secret}>` | `<div className={`secret ${look.secret ?? ''}`.trim()}>` |
| `<p style={{ margin: '8px 0 0', fontSize: '0.85em', opacity: 0.85 }}>` | `<p className={look.meta}>` |
| `<div key={key.id} style={rowStyle}>` | `<div key={key.id} className={look.row}>` |
| `<span style={{ opacity: 0.6 }}>` | `<span className={look.meta}>` |
| `<button type="button" onClick={…}>Revoke</button>` | `<button type="button" className={look.button} onClick={…}>Revoke</button>` |
| `<button type="submit" disabled={busy}>Create</button>` | `<button type="submit" className={look.button} disabled={busy}>Create</button>` |
| `<button style={look.signOut} …>Sign out</button>` | `<button className={look.signOut} …>Sign out</button>` |
| `<div style={overlayStyle}>` | `<div className="reader-dialog">` |
| `<div style={panelStyle} role="dialog" …>` | `<div className="reader-dialog-panel" role="dialog" …>` |
| `<div style={rowStyle}>` (the head) | `<div className="reader-panel-head">` |
| `<h2 style={{ margin: 0, fontSize: '1.1em' }}>Account</h2>` | `<h2 className="reader-panel-title">Account</h2>` |
| `<button type="button" onClick={props.onClose}>Close</button>` | `<button type="button" className="reader-button" onClick={props.onClose}>Close</button>` |

The last five are the panel branch only and take their classes literally rather than through `look`, because that branch **is** the panel — `look` exists for the parts both placements share.

The `page` placement's `look` is `{}` throughout, so every `className={look.x}` renders as `undefined` and React omits the attribute. Task 6 fills it.

- [ ] **Step 6: Run the browser test**

```bash
cd web && npx vitest run --project browser src/Account.browser.test.tsx --reporter=default
```
Expected: PASS on all three.

- [ ] **Step 7: Watch each assertion fail against its own defect**

1. In `reader.css`, give `.reader-dialog-panel` `border-radius: 12px`. Expected: RED on the radius assertion.
2. Put one `style={{ opacity: 0.6 }}` back on the session's "last seen" span. Expected: RED on `carries no inline style attribute anywhere`, naming the `SPAN`.
3. In `LOOKS.panel`, change `field` to `'reader-field'` alone — dropping `reader-field-block`. Expected: **green**, because the field's font size is unchanged. Record it: the width assertion this asks for belongs to the browser pass, where a field narrower than its panel is visible, and a unit test for it would be pinning a number the design does not fix.
4. In the fixture, drop the `<div className="reader">` wrapper. Expected: RED on `resolves its colours dark although the document is light`, and **only** on that one — which is what makes it the assertion that the containment is doing work rather than the DOM path being described twice.

- [ ] **Step 8: Update `Account.test.tsx` for the class switch**

Any assertion reading a `style` attribute on a panel element now reads `className`. Run the file and fix what it reports:

```bash
cd web && npx vitest run --project jsdom src/Account.test.tsx --reporter=default
```

- [ ] **Step 9: Commit**

```bash
git add web/src/Account.tsx web/src/styles/reader.css web/src/Account.browser.test.tsx web/src/Account.test.tsx
git commit -m "⑤c: the account panel becomes one of the reader's dialogs

It was 25 inline style props drawing a dialog that matched none of the
four beside it: 12px corners against 0, a drop shadow against none,
rgba(24,24,24,0.96) against --reader-panel, 16px body against 14, a sans
title against a serif one. It now takes ③'s classes, and LOOKS switches
class names instead of CSSProperties objects.

reader.css gains five rules and that is the honest size of it, not 'no new
CSS': three are box — a field that spans the panel, a full-width section
verb, the secret's block — and exist because no ③ panel had a form. Two
are colour, for the failure and the confirmation.

The narrowing from 560 to 480 is taken deliberately. A user-agent string
wraps one line more; a fifth panel with its own width is the per-control
figure ⑤b's measure decision exists to stop.

It also moves the panel's fields inside O151's floor rather than beside
it. They are 16px today only because panelStyle set no size at all."
```

---

## Task 6: The account page becomes a settings screen

Spec §12.5. Drawings: `drawn/acctpage-A-today.png` (today) against `drawn/acctpage-B-fields.png`.

Today `/account` is the one settings screen not drawn like the other four: hairline-ruled sections rather than cards, sentence-case labels butting against bare inputs rather than the `.field` micro-label, no measure on the prose, and the two `opacity` dims — one of which fails WCAG AA.

**Files:**
- Modify: `web/src/Account.tsx` — `LOOKS.page`
- Modify: `web/src/styles/admin.css` — the `.account` block, and the three coupled `.field` selector lists
- Test: `web/src/Account.test.tsx`

**Interfaces:**
- Consumes: Task 5's `Look` and the `className={look.x}` call sites.
- Produces: nothing.

- [ ] **Step 1: Write the failing test**

Append to `web/src/Account.test.tsx`:

```ts
import adminSource from './styles/admin.css?raw'
import { rules, selectors, withoutComments } from './testing/css'

/**
 * **The page placement holds no colour and no `opacity`, and that is the half
 * of this screen a stylesheet cannot be asked about.** `/account`'s quiet text
 * was two inline `opacity` dims, and the 0.6 one computed **4.18:1 on Paper**
 * at 16 px — under AA's 4.5 for normal text, measured in Chromium on
 * 2026-09-24. `admin.css`'s own `.proposal-detail` comment has banned opacity
 * dimming on these screens since C3, and ⑤b spent a PR emptying the last nine
 * from the stylesheet — but a `style=` prop is somewhere no rule in that
 * stylesheet can reach, which is why this assertion is over the component's
 * source rather than over the CSS.
 */
describe('Account.tsx carries no inline style', () => {
  it('has no `style=` prop and no `opacity`', async () => {
    const source = await import('./Account.tsx?raw').then((m) => m.default)
    const code = source.replace(/\/\*[\s\S]*?\*\/|\/\/[^\n]*/g, ' ')
    expect(code.match(/style=\{/g) ?? []).toEqual([])
    expect(code.match(/\bopacity\b/g) ?? []).toEqual([])
  })
})

/**
 * **The 16 px floor's three lists agree** — O152's shape. `base.css.test.ts`
 * pairs an exposed rule with its coarse companion by *exact selector text*,
 * so a subject added to two of the three lists is a floor with no companion
 * and the gate cannot say so. `.account input`'s own pair is deleted rather
 * than left beneath the new one: two rules at different specificities both
 * setting a size on the same element is the shape that produced O151.
 */
describe('the account page joins the shared field block whole', () => {
  it('appears in all three coupled selector lists and nowhere else', () => {
    const preludes = rules(withoutComments(adminSource)).map((r) => r.prelude)
    const withAccountField = preludes.filter((p) =>
      // `selectors()` rather than `split(',')`: the shared block's lists are
      // plain today, but the coarse companion sits beside rules that use
      // `:where(a, b)`, where a naive split cuts a selector in half.
      selectors(p).some((s) => s.startsWith('.account .field')),
    )
    expect(withAccountField).toHaveLength(3)
    expect(withoutComments(adminSource)).not.toMatch(/^\.account input\b/m)
  })
})
```

- [ ] **Step 2: Run it and watch it fail**

```bash
cd web && npx vitest run --project jsdom src/Account.test.tsx --reporter=default
```
Expected: FAIL on both — `Account.tsx` still has `style={` in the page branch's shared parts, and no `.account .field` exists.

- [ ] **Step 3: Fill `LOOKS.page`**

```tsx
  page: {
    section: 'account-card',
    heading: 'account-heading',
    label: 'field',
    wide: 'account-wide',
    signOut: 'account-wide',
    secret: 'secret',
    failure: 'account-failure',
    note: 'account-note',
    row: 'account-row',
    meta: 'why',
  },
```

`field` and `button` are absent on purpose: `.account .field input` draws the control, and `base.css`'s `:where(.library, .login) :where(button)` already draws a button on this screen. `secret` repeats the class the markup already carries, which is harmless and keeps the two placements symmetrical — Task 5's call site is `className={`secret ${look.secret ?? ''}`.trim()}`, so the page renders `secret secret`; collapse that call site to `className={look.secret ?? 'secret'}` instead.

- [ ] **Step 4: Rewrite the `.account` block in `admin.css`**

Replace the whole block (`.account` through `.account .secret`, and both `@media (pointer: coarse)` rules inside it) with:

```css
/* O94's account screen at `/account`: the `Account` the reader draws as an
   overlay, drawn here inside `.library`'s `<main>`. The panel's palette is
   fixed dark to sit over a page, so `Account` hands *that* placement the
   reader's own classes and this one `admin.css`'s (spec §12.5).

   ⑤c made it a settings screen rather than a page of hairlines: a card per
   section on the same measure the roots and users screens use, the `.field`
   micro-label on its three inputs, and quiet text bought with `--muted` and
   size. It was bought with `opacity` before, and the 0.6 dim on a session's
   "last seen" measured 4.18:1 on Paper at 16px — under AA. */
.account {
  --config-measure: 40rem;
}

.account > h2 {
  margin: 0;
}

.account-card {
  max-width: var(--config-measure);
  margin-top: var(--space-4);
  padding: var(--space-4);
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
}

.account-heading {
  margin: 0 0 var(--space-3);
  font-family: var(--font-title);
  font-size: var(--text-body);
  font-weight: 600;
}

.account-row {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--space-3);
  margin-top: var(--space-2);
}

.account-wide {
  margin-top: var(--space-3);
}

.account-failure {
  color: var(--danger);
}

.account-note {
  color: var(--success);
}

.account .secret {
  margin-top: var(--space-2);
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--line-strong);
  border-radius: var(--radius-control);
  font-family: ui-monospace, monospace;
  word-break: break-all;
}
```

`.account .why` joins the existing prose-measure rule (`.users .why, .backup .what-a-backup-is`) rather than getting its own:

```css
.users .why,
.account .why,
.backup .what-a-backup-is {
  max-width: 60ch;
}
```

and the quiet-text rule it needs, beside `.user-row .why, .user-row .locked`:

```css
.account .why {
  margin: 0;
  color: var(--muted);
  font-size: var(--text-meta);
}
```

- [ ] **Step 5: Add `.account .field` to all three coupled lists**

The base rule (`.proposal-detail .field, .rule-form .field, …`):

```css
.proposal-detail .field,
.rule-form .field,
.root-row .field,
.user-row .field,
.add-user .field,
.add-root .field,
.account .field {
```

The control rule (`.proposal-detail .field input, …`) gains `.account .field input`. The `@media (pointer: coarse)` companion gains `.account .field input` too — **both of the control lists, or the floor gate cannot pair them**. Note the asymmetry: the *label* subject `.account .field` goes in the base rule only. A label subject in the control lists would put an 11.2 px micro-label size on an input. Then delete `.account input` and its own `@media (pointer: coarse)` companion entirely: the new `.account .field input` pair replaces both at (0,2,1), and leaving the old (0,1,1) pair underneath is two rules setting a size on one element, which is how O151 happened.

- [ ] **Step 6: Take `testing/css.ts`'s caller count to eight**

Task 3 took it to seven. `Account.test.tsx` is the eighth — and it is the first caller outside `index.css.test.ts`, `theme.test.ts` and `styles/`, so the sentence gains a clause rather than just a number:

```
 * Shared parsing for the tests that read a stylesheet's own text rather than
 * render it. Eight call it: `../index.css.test.ts`, `../theme.test.ts`,
 * `../Account.test.tsx` — which reads `admin.css` to check that one screen's
 * three coupled selector lists agree — and `../styles/`'s `base.css.test.ts`,
 * `admin-rules.css.test.ts`, `admin-config.css.test.ts`,
 * `admin-proposals.css.test.ts` and `reader.css.test.ts`. **The count is
 * stated because it is what decides whether this module's shape can
 * change**: an open item quoted "three" from an earlier version of this
 * sentence and was wrong by half.
```

- [ ] **Step 7: Run the suites**

```bash
cd web && npx vitest run --project jsdom src/Account.test.tsx --reporter=default
cd web && npx vitest run --project node src/styles/base.css.test.ts src/index.css.test.ts --reporter=default
```
Expected: PASS. `base.css.test.ts` is the one that matters — it is the gate that would go red if the three lists disagreed.

- [ ] **Step 8: Watch each assertion fail against its own defect**

1. Put `style={{ opacity: 0.6 }}` back on the session's "last seen" span. Expected: RED on both halves of the source test — the `style=` count and the `opacity` count. Confirm it names both, not one.
2. Remove `.account .field input` from the `(pointer: coarse)` list only, leaving it in the other two. Expected: RED in `base.css.test.ts` — an exposed control with no companion. **If it is green, stop**: that is O152's second half, where a wrapped mixed list is dropped rather than checked, and this task has landed in it. Report rather than adjusting the mutation until something goes red.
3. Remove `.account .field` from the base rule only. Expected: RED on `appears in all three coupled selector lists`.
4. Change `.account-failure`'s `color` to `var(--muted)`. Expected: **green** — nothing asserts it. Record it: what covers that is `index.css.test.ts`'s floor, which computes `--danger` against `--bg`/`--surface`/`--field`, and the browser pass.

- [ ] **Step 9: Commit**

```bash
git add web/src/Account.tsx web/src/styles/admin.css web/src/Account.test.tsx web/src/testing/css.ts
git commit -m "⑤c: /account becomes a settings screen

A card per section on the same measure the roots and users screens use,
the .field micro-label on its three inputs, and prose at 60ch. It was the
one settings screen drawn as a page of hairlines.

The 0.6 opacity dim on a session's 'last seen' measured 4.18:1 on Paper at
16px, under AA for normal text. --muted measures 5.25 there, so the sweep
fixes it by doing what it was going to do anyway. admin.css has banned
opacity dimming on these screens since C3 and ⑤b emptied the last nine
from the stylesheet — but a style= prop is somewhere no rule in that
stylesheet can reach, so the assertion that replaces it reads the
component's source.

.account .field goes into all three coupled selector lists, not two: the
floor gate pairs a rule with its companion by exact selector text, and
O152 records what a partial addition does to it. The old .account input
pair is deleted rather than left underneath — two rules setting a size on
one element at different specificities is how O151 happened."
```

---

## Task 7: The 44 px floor `admin.css` never had

Spec §12.4a. **This is the one task that reaches past the two screens the branch is named for**, and that was decided with the measurements in hand.

`shell.css`, `library.css` and `reader.css` each implement §7.1's *"links and controls get 44 px rows under `(pointer: coarse)`"*. `admin.css` contains **no `(pointer: coarse)` block at all**, so on a phone every control on all five of its screens is its desktop height. Measured at a 390 px viewport against the copy of the real library on 2026-09-24:

| screen | control | height |
|---|---|---|
| inbox | the filter pills | 37.2 |
| inbox | **Dismiss**, one per row, 17 of them | **27.6** |
| `/account` | inputs · buttons | 35.6 · 34 |
| `/config/users` | inputs · selects · buttons | 31 · 32 · 34 |
| `/config/users` | **the permission checkboxes** | **13 × 13** |

**Files:**
- Modify: `web/src/styles/admin.css` — one block at the foot of the file
- Modify: `web/src/styles/reader.css` — `.reader-field` joins the existing coarse block
- Test: `web/src/styles/admin-proposals.css.test.ts`

**Interfaces:**
- Consumes: Task 3's `.proposal-states` / `.proposal-tabs` / `.proposals .checks`, Task 6's `.account .field` and `.account-wide`.
- Produces: nothing.

- [ ] **Step 1: Write the failing test**

Append to `web/src/styles/admin-proposals.css.test.ts`:

```ts
/**
 * **The coarse-pointer floor, asserted as a floor rather than as a rule that
 * exists.** A test that only checked `admin.css` now contains a `(pointer:
 * coarse)` block would pass against a block covering one selector, so this
 * asserts the *subjects*: every kind of control on the five screens
 * `admin.css` draws is named inside it.
 *
 * The heights themselves belong to the browser pass. jsdom computes none, and
 * driven Chromium is `pointer: fine` so the media query never engages — the
 * blind spot O151 hid in, and the reason §12.4a's figures were taken with a
 * fine pointer, where an absent rule makes the two identical.
 */
describe('admin.css carries a coarse-pointer floor', () => {
  /**
   * The body of the **last** `@media (pointer: coarse)` block, matched by
   * counting braces rather than by a regex.
   *
   * `admin.css` holds two such blocks when this branch starts — the shared
   * `.field` companion at 589 and `.account input`'s at 1623 — and Task 6
   * deletes the second while Task 7 adds this one, so the count is two again
   * and *which* one a test reads is the whole question. A greedy
   * `\{([\s\S]*)\}` anchored to the file's end matches from the **first**
   * block, so it would report a floor that exists whether or not this one was
   * ever written. That version of this helper was caught in review before it
   * shipped; this is the replacement.
   */
  const coarse = (() => {
    const src = withoutComments(adminSource)
    const at = src.lastIndexOf('@media (pointer: coarse)')
    if (at === -1) return ''
    const open = src.indexOf('{', at)
    let depth = 0
    for (let i = open; i < src.length; i++) {
      if (src[i] === '{') depth += 1
      else if (src[i] === '}') {
        depth -= 1
        if (depth === 0) return src.slice(open + 1, i)
      }
    }
    return ''
  })()

  it('has one, and it is the last of exactly two', () => {
    expect(coarse).not.toBe('')
    // Two: the shared `.field` block's companion, and this floor. A third
    // would mean this helper is reading something other than what it thinks,
    // which is the failure mode the brace counter above exists for.
    const blocks = withoutComments(adminSource).match(
      /@media\s*\(pointer:\s*coarse\)/g,
    )
    expect(blocks).toHaveLength(2)
  })

  for (const subject of [
    '.proposal-states .state',
    '.proposal-tabs button',
    '.proposals .checks label',
    '.proposals .row .dismiss',
    '.proposals .row .reopen',
    '.account-wide',
    '.config button',
    '.proposals button',
    '.account button',
  ]) {
    it(`names ${subject}`, () => {
      expect(coarse).toContain(subject)
    })
  }

  it('gives a checkbox a box rather than a row', () => {
    // A checkbox is not a row: 44px beside a 14px sentence draws something
    // that looks like a button. 24px is what `library.css` already gives its
    // own two, and the tappable target is the `<label>`, which takes the 44.
    expect(coarse).toMatch(/input\[type="checkbox"\][\s\S]*?(inline|block)-size:\s*24px/)
    expect(coarse).not.toMatch(/input\[type="checkbox"\][\s\S]*?min-height:\s*44px/)
  })
})
```

- [ ] **Step 2: Run it and watch it fail**

```bash
cd web && npx vitest run --project node src/styles/admin-proposals.css.test.ts --reporter=default
```
Expected: FAIL on the subject cases, and on `has one, and it is the last of exactly two` reporting **one** block rather than two — because at this point the only coarse block left in the file is the shared `.field` companion (Task 6 deleted `.account input`'s). Confirm the failure says `expected 1 to be 2`. If it says the subjects are missing but the count is fine, the brace counter is reading the `.field` block and must be fixed before going on.

- [ ] **Step 3: Write the floor**

At the very foot of `admin.css`:

```css
/* **44 px targets for a finger, which this stylesheet has never had** — spec
   §7.1, §12.4a. `shell.css`, `library.css` and `reader.css` each carry one;
   `admin.css` carried none, so on a phone every control on all five of its
   screens was its desktop height. Measured at 390 px on 2026-09-24: the
   inbox's filter pills 37.2, its Dismiss buttons 27.6, `/account`'s inputs
   35.6, `/config/users`'s selects 32 and its permission checkboxes 13×13.

   **Scoped to the three roots that hold those screens**, not to `.library`.
   Every one of them sits inside `.library`, but so do the library home,
   search, the series page and the sidebar, which `library.css` and
   `shell.css` already answer for — and a second floor over the same controls
   is the drift this pass keeps recording.

   **A checkbox is not a row.** The four on these screens take an explicit
   24 px box, the size `library.css` already gives its own two; the tappable
   target is the `<label>` beside it, which is a row and takes the 44. A
   44 px box beside a 14 px sentence draws a checkbox that looks like a
   button.

   Nothing here could be settled on a phone and did not need to be: the rule
   was *absent*, so a fine-pointer measurement is the coarse one. That is the
   opposite of O151, where a rule existed and lost. */
@media (pointer: coarse) {
  :where(.proposals, .account, .config) :where(button, a.button),
  :where(.proposals, .account, .config) .field :where(input, select, textarea),
  :where(.proposals, .account, .config) .checks label,
  .account-wide {
    min-height: 44px;
  }

  :where(.proposals, .account, .config) .checks input[type="checkbox"] {
    inline-size: 24px;
    block-size: 24px;
  }
}
```

- [ ] **Step 4: Give `.reader-field` the floor `.reader-button` has had since ③**

`reader.css`'s coarse block is `@media (pointer: coarse) { .reader-button { min-height: 44px } }`. `.reader-field` never needed one because no ③ panel had a field; Task 5's does.

```css
@media (pointer: coarse) {
  .reader-button,
  .reader-field {
    min-height: 44px;
  }
}
```

- [ ] **Step 5: Run the suites**

```bash
cd web && npx vitest run --project node --reporter=default
```
Expected: PASS, including `base.css.test.ts` — this rule sets `min-height`, not `font-size`, so the floor gate has no opinion about it. If `base.css.test.ts` goes red, something in Step 3 set a size and should not have.

- [ ] **Step 6: Watch the assertions fail against their own defects**

1. Delete `.proposals .row .dismiss` from the first selector list. Expected: RED on `names .proposals .row .dismiss`.
2. Change the checkbox rule to `min-height: 44px`. Expected: RED on **both** halves of `gives a checkbox a box rather than a row` — confirm it is both, because the negative assertion alone would pass against a rule that set neither.
3. Replace the three roots with `.library`. Expected: **green** — every assertion still passes, because `.library` contains all three. Record it, and add the guard:

```ts
  it('is scoped to the three admin roots, not to .library', () => {
    // The mutation that found this: widening the scope to `.library` leaves
    // every subject assertion green and silently puts a second floor over
    // the library home, search, the series page and the sidebar, which
    // `library.css` and `shell.css` already answer for.
    expect(coarse).not.toMatch(/:where\(\.library[,)]/)
    expect(coarse).toContain(':where(.proposals, .account, .config)')
  })
```
   Re-apply the mutation, confirm the new test goes red, revert.

- [ ] **Step 7: Commit**

```bash
git add web/src/styles/admin.css web/src/styles/reader.css web/src/styles/admin-proposals.css.test.ts
git commit -m "⑤c: admin.css gets the 44px coarse floor it never had

Spec section 7.1 settled that links and controls get 44px rows under
(pointer: coarse). shell.css, library.css and reader.css each implement
it; admin.css contained no (pointer: coarse) block at all, so on a phone
every control on all five of its screens was its desktop height. Measured
at 390 on 2026-09-24: the inbox's filter pills 37.2, its Dismiss buttons
27.6, /account's inputs 35.6, /config/users's selects 32, and its
permission checkboxes 13x13.

All five screens rather than this branch's two, which is a decision taken
with those figures in hand: a floor is one rule, and three screens would
otherwise ship knowingly short behind a row nobody was holding.

Scoped to .proposals, .account and .config rather than to .library. A
mutation widening it to .library left every assertion green while putting
a second floor over the library home, search, the series page and the
sidebar, so there is now a test for that.

A checkbox takes a 24px box, not a 44px row: the tappable target is the
label beside it, and 44px beside a 14px sentence draws a checkbox that
looks like a button.

.reader-field joins reader.css's own coarse block, which .reader-button
has been in since ③. It never needed one because no ③ panel had a field."
```

---

## Task 8: The bookkeeping — the allowance reaches empty

Spec §12.7. §5 promised "the colour-literal allowlist reaches empty" in this PR, and `ALLOWED` has two entries: `./Account.tsx: 10` and `./styles/annotations.css: 2`. Task 5 took the first to zero. This task takes the second to zero by **exempting the file, not tokenising it** — chosen by the user on 2026-09-24.

**Files:**
- Modify: `web/src/colourLiterals.test.ts:10-34`

**Interfaces:**
- Consumes: Task 5's removal of `Account.tsx`'s ten literals.
- Produces: `ALLOWED === {}`, which is the claim the PR body makes.

- [ ] **Step 1: Confirm only the labeller is left**

```bash
cd web && npx vitest run --project node src/colourLiterals.test.ts --reporter=default
```
Expected: PASS, with `ALLOWED` holding exactly `{ './styles/annotations.css': 2 }`. Task 5 removed `Account.tsx`'s entry in the commit that made it true. If `Account.tsx` appears in `ALLOWED` or in `actual`, something regressed and this task stops here.

- [ ] **Step 2: Exempt the labeller and empty the allowance**

```ts
/**
 * **Colour lives in `styles/tokens.css`**, spec 2026-09-17 §4 rule 2.
 *
 * Every non-test file under `src/` is read as text and counted for hex
 * colours, `rgb()`/`rgba()`/`hsl()`/`hsla()` and CSS system-colour keywords —
 * outside comments, and for a keyword in TypeScript only inside a string
 * literal, since `Field` is also a type name there.
 *
 * **Three files are exempt**, and each for a reason about the artwork rather
 * than about convenience:
 *
 * - `styles/tokens.css`, where colour is defined.
 * - `palette.ts`, whose note colours are drawn *on* artwork and so follow no
 *   theme.
 * - `styles/annotations.css`, the labeller's, added by ⑤c. Spec §6 puts the
 *   labeller outside the style pass, and its two literals are deliberately
 *   theme-independent for `palette.ts`'s own reason — that file's comment
 *   says `currentColor` "would follow the theme and go pale on white paper,
 *   which is the failure the palette above exists to avoid". The two are the
 *   `#fff` inner line that keeps a box's outline legible where it crosses
 *   black ink, and the `rgba(74, 36, 0, 0.5)` of the ignore hatch. Moving
 *   them into `tokens.css` would put two values that must *not* follow the
 *   theme into the file whose whole job is following it, which is why ⑤c
 *   exempted the file instead (spec §12.7).
 *
 * **The allowance only shrinks, and it is now empty.** It started at what the
 * tree held when the tokens arrived, and `toEqual` makes a file over its entry
 * fail and a file *under* its entry fail too. ⑤c was the style pass's last PR
 * and emptied it: `Account.tsx`'s ten went when its inline `style=` props
 * became class names. **Adding an entry back is a decision, not a fix** —
 * a colour that needs to exist outside `tokens.css` needs the paragraph above
 * to gain a fourth bullet saying why.
 *
 * **What this cannot see:** a colour named in words (`black`), a colour built
 * at runtime, or a CSS id selector made only of hex digits (`#add`), which it
 * would count. Nor a regular expression literal in TypeScript, which is
 * scanned as code: one holding `//` or `/*` — `/\//` ends in two slashes —
 * reads to the counter as a comment opening, and it under-counts whatever
 * that comment appears to swallow. None of these exists here today.
 */
const EXEMPT = new Set([
  './styles/tokens.css',
  './palette.ts',
  './styles/annotations.css',
])

const ALLOWED: Record<string, number> = {}
```

- [ ] **Step 3: Run it**

```bash
cd web && npx vitest run --project node src/colourLiterals.test.ts --reporter=default
```
Expected: PASS, four tests.

- [ ] **Step 4: Watch it fail against its own defect, both directions**

1. Add `color: #ff0000` to any rule in `admin.css`. Expected: RED, `actual` carrying `'./styles/admin.css': 1` against an empty `ALLOWED`. Revert.
2. Take `'./styles/annotations.css'` back out of `EXEMPT`. Expected: RED, `actual` carrying `2`. This is the assertion that the exemption is doing work rather than being decorative. Revert.
3. Put one literal back into `Account.tsx` — `failure: { color: '#ffb4a8' }` as an inline object. Expected: RED **and** Task 6's source-text test RED on the `style=` count. Two gates catch it, which is the point of having both. Revert.

- [ ] **Step 5: Commit**

```bash
git add web/src/colourLiterals.test.ts
git commit -m "⑤c: the colour-literal allowance reaches empty

Section 5 of the style pass spec promised this PR would empty it.
Account.tsx's ten went with its inline style props. annotations.css's two
are exempted rather than tokenised: section 6 puts the labeller outside
this pass, and both values are deliberately theme-independent for the
reason palette.ts is already exempt — a currentColor there would follow
the theme and go pale on white paper, which is the failure the palette
exists to avoid. Tokenising them would move two values that must not
follow the theme into the file whose whole job is following it.

The doc comment now says adding an entry back is a decision rather than a
fix, and what such a decision would have to write down."
```

---

## Task 9: The browser pass

§5 item 2, and spec §12.4a widens it: **five screens, not two**.

**Files:**
- Create: screenshots under `target/style-pass-5c/pass/` (gitignored)
- Modify: `docs/notes.md` — the pass's own section

**Interfaces:**
- Consumes: Tasks 1–8, all committed.
- Produces: the evidence `docs/status.md`'s rows and the PR body cite.

- [ ] **Step 1: Bring the app up against a copy of the real library**

```bash
cd /home/davey/projects/taiyaku
sha256sum /home/davey/temp/manga-test/library.db   # d5ebc0c2…, before
mkdir -p target/style-pass-5c/pass
cp /home/davey/temp/manga-test/library.db target/style-pass-5c/lib/library.db
printf 'style5c\nstyle5c\n' | ./target/release/taiyaku library user password local --db target/style-pass-5c/lib/library.db
./target/release/taiyaku serve --db target/style-pass-5c/lib/library.db \
  --cache "$SCRATCH/cache" --addr 127.0.0.1:8497 &
cd web && TAIYAKU_SERVE=http://127.0.0.1:8497 npx vite --port 5197 --strictPort &
```

`$SCRATCH` is this session's scratchpad directory. The cache goes there, not in the repo and not in `$HOME`.

- [ ] **Step 2: Shoot the matrix**

Five screens × two widths × two themes, plus the reader's Account panel. Twenty-one shots into `target/style-pass-5c/pass/`, named `<screen>-<width>-<theme>.png`:

| screen | route | extra state |
|---|---|---|
| inbox, collapsed | `/proposals` | — |
| inbox, expanded | `/proposals` | second row open — it has a target, so `AcceptForm` mounts |
| account page | `/account` | — |
| rules | `/config/rules` | first rule selected, Provenance tab |
| roots | `/config/roots` | — |
| users | `/config/users` | — |
| backup | `/config/backup` | — |
| account panel | `/read/akame-ga-kill----v01--2015--cbz` | `Shift+A` |

At 390×844 and 1440×900, in Paper and in Ink. Theme is set from the sidebar's Theme radios; **on a light OS, "System" is Paper** — set the radio explicitly rather than trusting the default, which is a mistake this pass has already recorded making once.

- [ ] **Step 3: Assert no sideways scroll, every screen, both widths**

```js
// in the driven page, per screen
({ docC: document.documentElement.clientWidth,
   docS: document.documentElement.scrollWidth,
   mainC: document.querySelector('main').clientWidth,
   mainS: document.querySelector('main').scrollWidth })
```
Expected: `docS === docC` and `mainS === mainC` on all eight rows, both widths. The inbox must be checked **expanded as well as collapsed** — the expanded panel is the one carrying absolute paths.

- [ ] **Step 4: Look at the four places the sweep moved pixels**

Named in "The sweep's one rule" above, and this is the step that decides whether each was right:

1. `.proposal-detail .fact` — rows grew 6.4 px each, four or five a side. Crop the facts list in both themes at both widths and look at it, rather than reading the number.
2. `.proposals .what` — the `↳` line sits 2.4 px further from the title.
3. `.proposal-states .state` — pills went 37.2 px to about 34 before Task 7's floor.
4. Corners everywhere went 3 px or 4 px to 2 px.

If any of the four reads worse than what it replaced, say so and change the mapping for that rule with the reason written down — the scale is the default, not a rule that outranks looking.

- [ ] **Step 5: Check the two things only this pass can see**

- **O150's row**, at 390 and at 1440, in both themes: the state pills, the underlined kind tabs and the checkbox are three visibly different kinds of control, and the three kind tabs are on one line at 390 where four pills wrapped to two.
- **The account panel**, over a real page: it is indistinguishable in weight from Settings opened beside it. Shoot both and put them side by side.

- [ ] **Step 6: Confirm the library is unchanged**

```bash
sha256sum /home/davey/temp/manga-test/library.db
```
Expected: `d5ebc0c2378650356115a8cce637e50dc5b4ab7f7cc8f388553cadd575a59e67`, the same figure as Step 1. Kill both servers **by PID** — this machine runs other projects' dev servers and `pkill -f vite` has hit them before.

- [ ] **Step 7: Write the pass up in `docs/notes.md`**

A `### ⑤c: the browser pass — 2026-09-…` section: what was served, the checksum before and after, the matrix, the four pixel moves with what was decided about each, and every figure this PR's body or `status.md` will quote. **Measurements go in `notes.md`, not `status.md`** — `status.md`'s own preamble promises every claim there traces to one recorded here.

- [ ] **Step 8: Commit**

```bash
git add docs/notes.md
git commit -m "⑤c: the browser pass

Five screens rather than two, because the coarse floor reaches all five.
390x844 and 1440x900, Paper and Ink, against a copy of the real library;
the original's checksum is unchanged.

Every figure this branch quotes is here."
```

---

## Task 10: The device pass, the gate, the rows and the claim check

**Files:**
- Modify: `docs/notes.md`, `docs/status.md`

**Interfaces:**
- Consumes: every task above.
- Produces: the branch, ready to open.

- [ ] **Step 1: The device pass**

The user's iPhone, iOS Safari over the tailnet, against the **production build** — `npm run build`, then `vite preview` behind `tailscale serve`, proxying `/api` to a `taiyaku serve` on a copy of the real library. Checksum before and after.

**Four questions, and the first three are the only reason this pass exists**: driven Chromium is `pointer: fine`, so nothing on the development machine engages a `(pointer: coarse)` rule.

1. **Task 7's floor, on the inbox.** Dismiss, the filter pills, the kind tabs and the weak checkbox's label are all at least 44 px to a finger.
2. **Task 7's floor, on Configuration.** The permission checkboxes on `/config/users` are 24 px boxes with 44 px label rows, not 13 × 13.
3. **Task 5 and 6's fields.** Tapping a password box on `/account` and in the reader's Account panel does **not** zoom the document. The panel's is the one that matters: Task 5 took it from 16 px by accident to 14 px plus O151's floor, and if that floor is not reaching it, this is where it shows.
4. **O150's row on a real phone**, which is the width it was designed for.

Read the user-agent before believing any of it — a result from the development machine's browser has been mistaken for a device result on this project before.

- [ ] **Step 2: The gate**

```bash
cd /home/davey/projects/taiyaku
env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 \
  scripts/check-all.sh > "$SCRATCH/gate.log" 2>&1
echo "exit $?" >> "$SCRATCH/gate.log"
tail -3 "$SCRATCH/gate.log"
```
Expected: the `all green` line, quoted verbatim into the PR body, and `exit 0` read **out of the file**. Never `| tail` and never `; echo` — both report the pipeline's status, and a background task's reported exit code is the compound command's.

- [ ] **Step 3: The web test counts, which the gate does not check**

`scripts/status-test-count.mjs` checks only the Rust figure. The `plus N in web/`'s clause has no gate behind it, and this branch adds tests to three projects.

```bash
cd web
npx vitest run --project node --reporter=default   | tail -5
npx vitest run --project jsdom --reporter=default  | tail -5
npx vitest run --project browser --reporter=default | tail -5
```
Read the three counts off those runs and write **those** into `docs/status.md`, rather than adding to the numbers already there.

- [ ] **Step 4: Close O148 and O150 — the five-part edit, twice**

For each, all five parts or the document contradicts itself:

1. the table row's checkbox, `[ ]` → `[x]`
2. the row's annotation
3. the prose bullet, rewritten to say what was done
4. the test count near the top — it appears **twice**, in "Five crates, N tests" and again two lines later in "Nine of the N are standing checks"
5. the "N are now closed — leaving M" sentence, and the "What remains … M open items" line near the bottom

Then the sweep, which is three greps and catches the drift this ritual exists for:

```bash
cd /home/davey/projects/taiyaku
grep -c '^| O[0-9]' docs/status.md                 # rows
grep -c '^| O[0-9]* | \[x\]' docs/status.md        # closed
grep -n -A1 '^What remains:' docs/status.md        # the count word wraps a line
```

Then `grep` the **bare O-number** across `docs/` and `web/src/` and read every hit. Prose outside the table describes open items in wording no sweep predicts — closing O25 once left a branch narrative claiming in the present tense that it was open, four lines above the "What remains" line.

- [ ] **Step 5: Open the three new rows**

Opening is the same shape inverted: the count word at the head of the section, the "leaving N" sentence, a table row, a prose bullet, and the "What remains" line.

| new row | what it says |
|---|---|
| "weak" | `strength()` calls any score under 2 weak while `INBOX_MIN_SCORE` hides only score 0, so on this library **13 of the 17 rows the default view shows carry a chip reading "weak"**, directly under a control offering to *show* weak proposals. 35 open: 18 at score 0, 13 at 1, 3 at 2, 1 at 3. §6 reserves what a screen says; which of the two lines is wrong is a curation judgement. Site: `curation.ts`'s `strength` and `INBOX_MIN_SCORE`. |
| `aria-pressed` vs `aria-current` | Four one-of-N **button** rows split two–two. `PreviewTabs` writes `aria-current="true"`; the library's filter row and the inbox's state and kind rows write `aria-pressed`. `ConfigNav` is a `<nav>` of links and `aria-current="page"` is right there, so it is not part of the disagreement. ⑤c's CSS carries all three preludes so the look is one look; the markup is the accessibility pass's. Site: `PreviewTabs.tsx`, `Library.tsx`, `Proposals.tsx`. |
| the other three screens' floor | **Only if Task 7 was narrowed.** If Task 7 covered all five screens as planned, there is no row — say so in the PR body instead, because a row that closes in the commit that opens it is noise. |

**Check every row has exactly six `|` characters**, counting the ones inside backticks: a `|` in a code span still ends a GFM table cell, and the forge drops cells past the header's five. Seven rows have done it before. Escape as `\|`, then render a suspect row through `POST /api/v1/markdown` with `mode: gfm` **and the real header and separator rows above it** — a fragment without a header renders nothing, which reads like a pass.

- [ ] **Step 6: The pre-merge claim check**

Separately from drafting, and after the gate is green: read every factual claim in the two closed rows, the three new ones, `docs/notes.md`'s new section and the PR body, and check each against the thing it describes. Worst case on this project was fifteen findings then eleven, four of the eleven inside the first round's own fixes.

Three shapes to look for specifically, each of which has bitten this project:

- **A figure quoted in more than one place.** Grep a corrected figure's shape across the PR body too, not only the row it was found in.
- **A claim about what a test covers.** "Watched failing" means the defect was put back by hand and the test went red — not that it was read and looked right.
- **Prefer deleting a figure to correcting it a fourth time.** On #166 four passes found 14 → 10 → 8 → 7 while the code never moved.

- [ ] **Step 7: Commit and open**

```bash
git add docs/status.md docs/notes.md
git commit -m "⑤c: close O148 and O150, open the rows this branch found

O148: the inbox's restated tile height is pinned against the CSS that
draws it. O150: the inbox's filter row stopped mixing a one-of-N filter
with an independent toggle, so it could take the tab look.

Two new rows, both words rather than styling and both deliberately not
fixed here. strength() calls any score under 2 weak while INBOX_MIN_SCORE
hides only score 0, so 13 of the 17 rows the default view shows are
chipped 'weak' under a control offering to show weak proposals. And four
one-of-N button rows split two-two between aria-pressed and aria-current.

O149 has an answer and lands on its own branch: it is the Rules screen,
which this one never opens."
```

Then open the PR through the **gitea** MCP, not `gh` — `gh` is authenticated against the wrong host here. Never hard-wrap the PR body: the body *is* the squash commit message, and the remote is squash-only.

---

## Self-Review

Run against the spec rather than from memory, and it found four things.

**1. Spec coverage.** §12.1 is measurement, no task. §12.2's seven decisions → Tasks 3 (1, 2), 5 (3), 6 (4), 4 (5), 8 (6), 7 (7). §12.3 → Task 3. §12.4 → Tasks 1 and 2. §12.4a → Task 7. §12.5 → Tasks 5 and 6. §12.6 → Task 4. §12.7 → Task 8. §12.8 is what is *not* done, and its two filed rows are Task 10 Step 5. §12.9's seven checks → Task 4, Task 3, Task 6, Task 5, Task 9, Task 6 again, Task 7. No gap.

**2. Placeholders.** None. The three steps that carry no code are Task 9's screenshots and Task 10's device pass and claim check, which are acts rather than edits and carry their matrices instead.

**3. Type consistency.** `LOOKS` is `Record<'panel' | 'page', Look>` in Task 5, and Task 6 fills the same keys — `label` was missing from the first draft of the interface and was added when Task 6 needed it, because `.field` belongs on the `<label>` and the panel wants no class there. `TILE_HEIGHT_PX` is exported from `tileSize.ts` in Task 4 and imported from there. `bodiesFor` and `declared` are defined in Task 3's new file and reused by Tasks 4 and 7. `selectors` and `withoutComments` are imported wherever they are used.

**4. What this review actually caught**, listed because three of the four would have failed at execution time rather than at review time:

- **The sweep's line numbers went stale.** Tasks 1 and 2 were written third and fourth and would have run after two tasks that insert into the same file. They are now **first**, against `admin.css` exactly as `5e09e474` left it, and every later task locates by selector. A 55-row table keyed by line number, applied to a file two commits have already shifted, is the failure this would have been.
- **Task 4 would have imported a `.tsx` into the `node` project**, to reach one number. No `node` test in this tree does that — the project is "pure TypeScript with no DOM" by its own config comment. `TILE_HEIGHT_PX` moves to `tileSize.ts` instead, which is pure, already holds `requestedTileEdge`, and is already read this way by `styles/reader.css.test.ts` — the precedent O148's row itself names.
- **Task 7's helper regex read the wrong block.** `/@media\s*\(pointer:\s*coarse\)\s*\{([\s\S]*)\}\s*$/` matches leftmost, so it would have captured from the **first** coarse block in `admin.css` to the end of the file — reporting a floor that exists whether or not the new one was ever written. Replaced with a brace counter from `lastIndexOf`, plus an assertion on how many coarse blocks the file holds, so the helper says something if it is ever reading a different one.
- **Task 3's "one rule, three preludes" assertion asserted nothing.** It looked the rule up by its own body and checked the lookup succeeded, which it always would. It now asserts the prelude's three selectors, which is the thing that can actually drift — three rules with identical bodies pass every other assertion in that file and are exactly what the decision is against.

**5. One thing outside the code.** `web/src/testing/css.ts`'s doc comment states how many files call it *and says why the number is written down* — an open item once quoted an earlier version of that sentence and was wrong by half. This branch adds two callers, so the count and its list are edited in Tasks 3 and 6 rather than left for the claim check to find.
