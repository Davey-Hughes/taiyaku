# Style pass ② — the shell and the library: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the library usable on a phone — a drawer below 900 px, the document scrolling, no sideways scroll — and restyle the library home, the series page and search in the ink-and-paper look, without changing what any screen does.

**Architecture:** `Shell.tsx` gains a phone bar and the drawer's state; below 900 px `styles/shell.css` moves the existing sidebar column off-canvas, and `inert` on the bar and `<main>` keeps focus and taps inside it while it is open. The screens change mostly in `styles/shell.css` and `styles/library.css`, with small markup changes: one `Wordmark` component, `NavLink` in the sidebar, cover cards in the continue strip, a volume count on each series tile, and a scroll box and a `pages` class on the series table.

**Tech Stack:** React 19, React Router 8, Vite 8, Vitest 4 (projects `node`, `jsdom`, `browser`), Biome 2.5.7, pnpm 11.

**Spec:** [`docs/superpowers/specs/2026-09-17-style-pass-design.md`](../specs/2026-09-17-style-pass-design.md) — §5 ② and §7. This plan is its PR ②.

**Branch:** `style-2-shell-library`, off `c70413e`, which already carries §7 (`aab6bea`).

## Decided while planning

These came out of reading and probing the code the plan touches, on this
branch, before any task.

1. **Two browser tests move from 700 px to 460 px.** `Proposals.browser.test.tsx`
   and `AcceptForm.browser.test.tsx` mount a fake shell (`.library`, an empty
   `.sidebar-column`, `<main>`) and measure at 700 px, chosen as a narrow
   desktop. Below 900 px the sidebar leaves the flow, so at 700 px their
   column grows from 428 px (700 − 224 − 48) to 668 px (700 − 2 × 16).
   Trialled with the narrow layout added to `shell.css` and then reverted:
   Proposals' "reflows at 700px" failed — `expected 217.5625 to be greater
   than 217.5625`, the row no longer reflowing — and the other 27 of 28
   passed. With both files at 460 px, where 460 − 2 × 16 = 428 px, all 8 of
   their cases passed. Task 2 moves them.
2. **The drawer's dialog attributes are spread from a constant.** Biome's
   `useAriaPropsSupportedByRole` refuses `aria-modal={open ? true :
   undefined}` beside `role={open ? 'dialog' : undefined}` — "The ARIA
   attribute 'aria-modal' is not supported by this element", measured by a
   probe file — while `{...(menuOpen ? AS_DIALOG : {})}` passes both biome
   and `tsc`.
3. **An inline SVG with `aria-hidden="true"` passes biome's recommended
   set** (probe). The two icons are drawn that way, stroked in
   `currentColor`, which the colour-literal guard does not count.
4. **The scrim's colour is a token**: `--scrim: light-dark(#231e1759,
   #0000008c)`, the mockup's `rgba(35, 30, 23, 0.35)` and `rgba(0, 0, 0,
   0.55)` as 8-digit hex. `tokens.css` is exempt from the colour-literal
   guard, and the contrast model and the browser palette check both read
   6-digit pairs only, so neither counts it — the palette check asserts
   exactly twelve pairs. Biome accepts the 8-digit pair, `:root:has()`,
   `-webkit-line-clamp` and the two-value `transition` (probe).
5. **`<main>` gets `min-width: 0` at every width.** A grid item's automatic
   minimum is its min-content width, so one unbreakable word inside `<main>`
   would widen the `1fr` track past the viewport.
6. **The edition selector's `<select>` may shrink.** A `<select>` is as wide
   as its longest option, and a real edition label reads `Go-Toubun no
   Hanayome v03 (RAW) [c024-036].cbz + 11 more` (`Series.test.tsx` quotes
   it). Task 5's browser case holds it.
7. **The device pass goes through `tailscale serve` to vite.** `taiyaku
   serve` does not host the web UI. Vite refuses a `Host` it has not been
   told about, and `serve` compares `Origin` with `Host`, or with
   `X-Forwarded-Host` from a `--trusted-proxy`; Task 6 says what to do when
   either refuses.

## Global Constraints

- Colours come from `styles/tokens.css` only. No colour literal (hex, `rgb()`, `rgba()`, `hsl()`, `hsla()`) or CSS system-colour keyword may be added to any non-test file under `web/src/` except `styles/tokens.css` and `palette.ts`; `colourLiterals.test.ts` enforces it.
- The breakpoint is 900 px: `@media (width < 900px)` in CSS, `(width >= 900px)` in `matchMedia`.
- Spacing is `--space-1` to `--space-7` (4, 8, 12, 16, 24, 32, 48 px); sizes are `--text-h1`, `--text-h2`, `--text-h3`, `--text-body`, `--text-ui`, `--text-meta`; corners are 0 on covers and panels, `--radius-control` (2 px) on buttons and inputs, and round only on count badges.
- Touch targets are 44 px under `(pointer: coarse)`, and a tile's ↓ and `i` controls are at least 24 px everywhere.
- No change to the reader (`App.tsx`, `Chrome.tsx`, `Filmstrip.tsx`, `Settings.tsx`, the panels) beyond comment text. `settingsFields.tsx`'s `rowStyle` becomes optional and the reader keeps passing its own.
- No change to what a screen does: the same controls, routes and requests. The only new facts on screen are the continue card's cover and volume and the series tile's volume count, all from data each screen already receives.
- `index.css.test.ts`'s `ruleBody` returns the *first* rule in source order whose selector matches exactly. Keep the main rules for `.library`, `.alignment-panel`, `.volumes .panel-open`, `.editions`, `.editions td.missing`, `.editions td.pages` and `.sidebar-heading` ahead of any `@media` block that repeats the selector, and keep `background` and `color` on `.library`, `.alignment-panel` and `.volumes .panel-open` — D2 and D3 read them.
- Component tests use `mount()` from `web/src/testing/mount.ts` and `act()`; `@testing-library/react` is not a dependency.
- `pnpm -C web test` runs the `node` project only. Run a `.test.tsx` with `--project jsdom` and a `.browser.test.tsx` with `--project browser`. Under Claude Code pass `--reporter=default` when reading test output.
- Typecheck with `pnpm -C web run typecheck`, and lint with `pnpm -C web run lint`, which is `biome check` and includes formatting: if it reports formatting, run `pnpm -C web exec biome format --write <the files>` and lint again.
- Scratch output goes under `target/style-pass/two/` (gitignored), never `/tmp` or `$HOME`.
- Commit at the end of every task, with a message starting `Style pass 2:` and no attribution lines.

---

### Task 1: The phone bar, the drawer's behaviour, and one wordmark

**Files:**
- Create: `web/src/Wordmark.tsx`
- Modify: `web/src/Login.tsx`, `web/src/Shell.tsx`, `web/src/Shell.test.tsx`, `web/src/styles/shell.css`

**Interfaces:**
- Produces: `export function Wordmark()`, no props, rendering `<span class="wordmark"><span class="seal" aria-hidden="true">対訳</span>taiyaku</span>`.
- Produces, in `Shell`'s markup: `.library` carries the class `menu-open` while the drawer is open. `<header class="phone-bar">` holds `button.menu-button[aria-controls="library-menu"][aria-label="Menu"]` with `aria-expanded`, then `a.home[href="/"]` around a `Wordmark`, then `a.search[href="/search"][aria-label="Search"]`. The sidebar column is `div#library-menu.sidebar-column`, its first child a `Wordmark`, carrying `role="dialog"`, `aria-modal="true"` and `aria-label="Menu"` only while open. `button.scrim[aria-label="Close menu"]` exists only while open. `.phone-bar` and `main` carry `inert` only while open.

- [ ] **Step 1: Write the failing tests**

In `web/src/Shell.test.tsx`, directly after the `checkboxLabelled` function and before `describe('Shell', () => {`, add:

```tsx
/** ☰, found by the column it controls rather than by its icon. */
function menuButton(host: HTMLElement): HTMLButtonElement {
  const button = host.querySelector<HTMLButtonElement>(
    'button[aria-controls="library-menu"]',
  )
  if (!button) throw new Error('no menu button')
  return button
}

/** The sidebar column, by the id ☰ names. */
function menu(host: HTMLElement): HTMLElement {
  const column = host.querySelector<HTMLElement>('#library-menu')
  if (!column) throw new Error('nothing carries the id ☰ controls')
  return column
}

function openMenu(host: HTMLElement) {
  act(() => {
    menuButton(host).click()
  })
}

/**
 * A `matchMedia` for jsdom, which has none: one list whose `matches` the test
 * flips, calling its listeners the way a window resized across the
 * breakpoint would.
 */
function fakeMatchMedia() {
  const listeners = new Set<() => void>()
  const list = {
    matches: false,
    addEventListener: (_type: string, listener: () => void) => {
      listeners.add(listener)
    },
    removeEventListener: (_type: string, listener: () => void) => {
      listeners.delete(listener)
    },
  }
  const matchMedia = vi.fn((_query: string) => list)
  const widen = () => {
    list.matches = true
    for (const listener of listeners) listener()
  }
  return { matchMedia, widen }
}
```

Then, immediately before the file's last line — the `})` that closes `describe('Shell', () => {` — add:

```tsx
  /**
   * The drawer — spec 2026-09-17 §7.1. jsdom lays nothing out, so this pins
   * the state and the attributes; `Shell.browser.test.tsx` measures what a
   * browser does with them: where Tab can go, whether the page scrolls
   * behind the drawer, where the column sits.
   */
  describe('the menu below 900 px', () => {
    it('draws ☰ closed, naming the column it opens, with nothing inert', async () => {
      const host = await settle(
        <Shell>
          <p>x</p>
        </Shell>,
      )
      const button = menuButton(host)
      expect(button.getAttribute('aria-label')).toBe('Menu')
      expect(button.getAttribute('aria-expanded')).toBe('false')
      expect(menu(host).classList.contains('sidebar-column')).toBe(true)
      expect(menu(host).hasAttribute('role')).toBe(false)
      expect(menu(host).hasAttribute('aria-modal')).toBe(false)
      expect(host.querySelector('.phone-bar')?.hasAttribute('inert')).toBe(
        false,
      )
      expect(host.querySelector('main')?.hasAttribute('inert')).toBe(false)
      expect(host.querySelector('.scrim')).toBeNull()
    })

    it('opens the column as a modal dialog and makes what is behind it inert', async () => {
      const host = await settle(
        <Shell>
          <p>x</p>
        </Shell>,
      )
      openMenu(host)
      expect(menuButton(host).getAttribute('aria-expanded')).toBe('true')
      const column = menu(host)
      expect(column.getAttribute('role')).toBe('dialog')
      expect(column.getAttribute('aria-modal')).toBe('true')
      expect(column.getAttribute('aria-label')).toBe('Menu')
      expect(column.hasAttribute('inert')).toBe(false)
      expect(host.querySelector('.phone-bar')?.hasAttribute('inert')).toBe(
        true,
      )
      expect(host.querySelector('main')?.hasAttribute('inert')).toBe(true)
      expect(
        host.querySelector('.library')?.classList.contains('menu-open'),
      ).toBe(true)
    })

    it('takes focus to its first link, and gives it back to ☰ on Escape', async () => {
      const host = await settle(
        <Shell>
          <p>x</p>
        </Shell>,
      )
      openMenu(host)
      expect(document.activeElement).toBe(
        menu(host).querySelector('a[href="/"]'),
      )
      act(() => {
        document.dispatchEvent(
          new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }),
        )
      })
      expect(menuButton(host).getAttribute('aria-expanded')).toBe('false')
      expect(document.activeElement).toBe(menuButton(host))
    })

    it('closes on a tap on the page behind it', async () => {
      const host = await settle(
        <Shell>
          <p>x</p>
        </Shell>,
      )
      openMenu(host)
      const scrim = host.querySelector<HTMLButtonElement>('.scrim')
      expect(scrim?.getAttribute('aria-label')).toBe('Close menu')
      act(() => {
        scrim?.click()
      })
      expect(menuButton(host).getAttribute('aria-expanded')).toBe('false')
      expect(host.querySelector('.scrim')).toBeNull()
    })

    it('closes when a link in it is followed', async () => {
      const host = await settle(
        <Shell>
          <p>x</p>
        </Shell>,
      )
      openMenu(host)
      const search = menu(host).querySelector<HTMLAnchorElement>(
        'a[href="/search"]',
      )
      if (!search) throw new Error('no search link in the menu')
      await act(async () => {
        search.click()
      })
      expect(menuButton(host).getAttribute('aria-expanded')).toBe('false')
    })

    it('closes when the window widens past the breakpoint, where the column is the sidebar again', async () => {
      const media = fakeMatchMedia()
      vi.stubGlobal('matchMedia', media.matchMedia)
      const host = await settle(
        <Shell>
          <p>x</p>
        </Shell>,
      )
      openMenu(host)
      expect(media.matchMedia).toHaveBeenCalledWith('(width >= 900px)')
      act(() => {
        media.widen()
      })
      expect(menuButton(host).getAttribute('aria-expanded')).toBe('false')
    })

    it('draws the wordmark at the top of the column, and as the bar’s way home', async () => {
      const host = await settle(
        <Shell>
          <p>x</p>
        </Shell>,
      )
      expect(
        menu(host).firstElementChild?.classList.contains('wordmark'),
      ).toBe(true)
      expect(
        host.querySelector('.phone-bar a[href="/"] .wordmark'),
      ).not.toBeNull()
      expect(
        host
          .querySelector('.phone-bar a[href="/search"]')
          ?.getAttribute('aria-label'),
      ).toBe('Search')
    })
  })
```

- [ ] **Step 2: Run them and watch them fail**

```bash
pnpm -C web exec vitest run --project jsdom src/Shell.test.tsx --reporter=default
```

Expected: the seven new cases FAIL (`no menu button` for six of them; `firstElementChild` is the `nav` for the wordmark case); every other case passes.

- [ ] **Step 3: Write `Wordmark.tsx`**

Create `web/src/Wordmark.tsx`:

```tsx
/**
 * 対訳, *taiyaku*: the seal set vertically in the accent, then the name.
 *
 * One component for the three places it is drawn — the sign-in screen's
 * heading, the phone bar's way home and the top of the sidebar (spec
 * 2026-09-17 §7.1) — so the seal cannot drift between them. It sets no size
 * and no weight: each place's own rule does, and the sign-in screen's `<h2>`
 * gives it the heading's.
 *
 * The seal is `aria-hidden`, so what a screen reader names is *taiyaku*;
 * `Login.test.tsx` holds that for the heading.
 */
export function Wordmark() {
  return (
    <span className="wordmark">
      <span className="seal" aria-hidden="true">
        対訳
      </span>
      taiyaku
    </span>
  )
}
```

- [ ] **Step 4: Draw the sign-in screen's heading with it**

In `web/src/Login.tsx`, add after `import { messageOf } from './message'`:

```tsx
import { Wordmark } from './Wordmark'
```

Replace:

```tsx
const wordmarkStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: 10,
  margin: 0,
}

/** 対訳, *taiyaku*, set vertically in the accent like a seal. */
const sealStyle: CSSProperties = {
  writingMode: 'vertical-rl',
  padding: '4px 3px 3px',
  background: 'var(--accent)',
  color: 'var(--on-accent)',
  fontSize: 'var(--text-meta)',
  letterSpacing: '0.1em',
  lineHeight: 1,
}
```

with:

```tsx
// The wordmark draws itself (`Wordmark.tsx`, `styles/shell.css`); the
// heading around it only drops its margin.
const headingStyle: CSSProperties = {
  margin: 0,
}
```

and replace:

```tsx
        <h2 style={wordmarkStyle}>
          <span style={sealStyle} aria-hidden="true">
            対訳
          </span>
          taiyaku
        </h2>
```

with:

```tsx
        <h2 style={headingStyle}>
          <Wordmark />
        </h2>
```

- [ ] **Step 5: Give `Shell` the bar and the drawer**

In `web/src/Shell.tsx`, replace the first line of the imports,

```tsx
import { useCallback, useEffect, useState } from 'react'
```

with:

```tsx
import { useCallback, useEffect, useRef, useState } from 'react'
import { Link, useLocation } from 'react-router'
```

and add after `import { ThemeControl } from './ThemeControl'`:

```tsx
import { Wordmark } from './Wordmark'
```

Add after the `BROWSE_MODULES` constant:

```tsx
/** Spec 2026-09-17 §2's breakpoint: the sidebar at or above it, the drawer below. */
const WIDE = '(width >= 900px)'

/**
 * What makes the sidebar column a modal dialog, while — and only while — the
 * drawer is open. Spread rather than written as three conditional props:
 * biome's `useAriaPropsSupportedByRole` refuses `aria-modal` beside a `role`
 * it cannot read statically.
 */
const AS_DIALOG = {
  role: 'dialog',
  'aria-modal': true,
  'aria-label': 'Menu',
} as const
```

Add, directly after the `useEffect` that reads the open count (the one ending `}, [admin, inboxReads])`):

```tsx
  // **The drawer — spec 2026-09-17 §7.1.** Below 900 px the sidebar column is
  // off-canvas and ☰ slides it in; at or above, ☰ is not drawn, so this never
  // becomes true there. While it is open the bar and `<main>` are `inert`,
  // which is what keeps focus and taps inside the column: a browser skips an
  // inert subtree for both. A native `<dialog>` would have done the same, but
  // this column is also the permanent sidebar at 900 px and up, so its
  // content would have had to move between the two.
  const [menuOpen, setMenuOpen] = useState(false)
  const menuButton = useRef<HTMLButtonElement>(null)
  const column = useRef<HTMLDivElement>(null)
  const wasOpen = useRef(false)
  const location = useLocation()

  // Focus follows the drawer: to its first control on opening, back to ☰ on
  // closing — and only on a close, so the first render moves nothing.
  useEffect(() => {
    if (menuOpen) {
      column.current
        ?.querySelector<HTMLElement>('a[href], button, input, select')
        ?.focus()
    } else if (wasOpen.current) {
      menuButton.current?.focus()
    }
    wasOpen.current = menuOpen
  }, [menuOpen])

  // Escape closes it, and so does the window widening past the breakpoint,
  // where the column is the sidebar again and nothing would be left to close
  // it with. `matchMedia` is checked for because jsdom has none.
  useEffect(() => {
    if (!menuOpen) return
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setMenuOpen(false)
    }
    const wide =
      typeof window.matchMedia === 'function'
        ? window.matchMedia(WIDE)
        : undefined
    const onWide = () => {
      if (wide?.matches) setMenuOpen(false)
    }
    document.addEventListener('keydown', onKey)
    wide?.addEventListener('change', onWide)
    return () => {
      document.removeEventListener('keydown', onKey)
      wide?.removeEventListener('change', onWide)
    }
  }, [menuOpen])

  // Any navigation closes it: a link inside it, and the browser's own back
  // button alike.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `location` is a trigger, not a closure read — the body reads nothing from it.
  useEffect(() => {
    setMenuOpen(false)
  }, [location])

  const classes = [
    'library',
    prefs.density === 'compact' ? 'compact' : '',
    menuOpen ? 'menu-open' : '',
  ]
    .filter(Boolean)
    .join(' ')
```

Replace the whole `<div className={…library…}>` element in the `return` — from

```tsx
          <div
            className={`library ${prefs.density === 'compact' ? 'compact' : ''}`.trim()}
          >
```

through its closing `</div>` (the one after `</main>`) — with:

```tsx
          <div className={classes}>
            <header className="phone-bar" inert={menuOpen}>
              <button
                ref={menuButton}
                type="button"
                className="menu-button"
                aria-label="Menu"
                aria-expanded={menuOpen}
                aria-controls="library-menu"
                onClick={() => {
                  setMenuOpen(true)
                }}
              >
                <svg viewBox="0 0 24 24" aria-hidden="true">
                  <path d="M4 7h16M4 12h16M4 17h16" />
                </svg>
              </button>
              <Link to="/" className="home">
                <Wordmark />
              </Link>
              <Link to="/search" className="search" aria-label="Search">
                <svg viewBox="0 0 24 24" aria-hidden="true">
                  <circle cx="11" cy="11" r="6" />
                  <path d="M16 16l4 4" />
                </svg>
              </Link>
            </header>
            <div
              id="library-menu"
              ref={column}
              className="sidebar-column"
              {...(menuOpen ? AS_DIALOG : {})}
            >
              <Wordmark />
              <Sidebar
                roots={status?.roots ?? []}
                showRoots={prefs.showRoots}
                admin={admin}
                openProposals={openProposals}
              />
              <BrowseSettings prefs={prefs} onChange={change} onReset={reset} />
              <ThemeControl />
            </div>
            {menuOpen && (
              <button
                type="button"
                className="scrim"
                aria-label="Close menu"
                tabIndex={-1}
                onClick={() => {
                  setMenuOpen(false)
                }}
              />
            )}
            <main inert={menuOpen}>
              {failure !== undefined && <p role="alert">{failure}</p>}
              {children}
            </main>
          </div>
```

The scrim is a `<button>` so a tap has something to land on without a click handler on a bare `div`; `tabIndex={-1}` keeps it out of Tab's order, because Escape is the keyboard's way out.

- [ ] **Step 6: The wordmark's rules, and the bar hidden until the narrow layout exists**

Append to `web/src/styles/shell.css`:

```css

/* The wordmark (`Wordmark.tsx`): 対訳 set vertically in the accent like a
 * seal, then the name. It sets no size or weight; where it is drawn does —
 * the sign-in screen's `<h2>`, the phone bar's way home, the sidebar's top.
 * `display: flex` is what the sign-in heading itself had before this
 * component took its markup, so that screen draws as it did. */
.wordmark {
  display: flex;
  align-items: center;
  gap: 10px;
  font-family: var(--font-title);
}

.wordmark .seal {
  padding: 4px 3px 3px;
  background: var(--accent);
  color: var(--on-accent);
  font-size: var(--text-meta);
  line-height: 1;
  letter-spacing: 0.1em;
  writing-mode: vertical-rl;
}

/* The phone bar and the scrim belong to the narrow layout, which this file
 * does not draw yet; hidden at every width until it does. */
.phone-bar,
.scrim {
  display: none;
}
```

- [ ] **Step 7: Run the tests**

```bash
pnpm -C web exec vitest run --project jsdom src/Shell.test.tsx src/Login.test.tsx --reporter=default
```

Expected: PASS, every case in both files, the seven new ones included.

- [ ] **Step 8: Watch the new tests fail against their own defects**

Apply each alone, run Step 7's command, confirm the named case fails, undo, and confirm `git diff --stat` shows only this task's files:

1. `<main inert={menuOpen}>` → `<main>`. Expected: "opens the column as a modal dialog…" FAILS on `main`'s `inert`.
2. `{...(menuOpen ? AS_DIALOG : {})}` → `{...AS_DIALOG}`. Expected: "draws ☰ closed…" FAILS on `role`.
3. Delete the line `if (event.key === 'Escape') setMenuOpen(false)`. Expected: "takes focus to its first link…" FAILS on `aria-expanded`.
4. Delete the line `menuButton.current?.focus()`. Expected: "takes focus to its first link…" FAILS on `document.activeElement`.
5. Delete the whole `useEffect` that calls `setMenuOpen(false)` on `location`, with its `biome-ignore` line. Expected: "closes when a link in it is followed" FAILS.
6. Delete the line `wide?.addEventListener('change', onWide)`. Expected: "closes when the window widens…" FAILS.

- [ ] **Step 9: The jsdom and node projects, lint, typecheck, the browser project**

```bash
pnpm -C web exec vitest run --project jsdom
pnpm -C web exec vitest run --project node
pnpm -C web run lint
pnpm -C web run typecheck
pnpm -C web exec vitest run --project browser --reporter=default
```

Expected: all exit 0; the browser project reports 28 passed, since nothing in it mounts `Shell` and the bar is hidden at every width.

- [ ] **Step 10: Commit**

```bash
git add web/src/Wordmark.tsx web/src/Login.tsx web/src/Shell.tsx web/src/Shell.test.tsx web/src/styles/shell.css
git commit -m "Style pass 2: the phone bar, the drawer's behaviour, and one wordmark"
```

---

### Task 2: The narrow layout — the drawer on screen, the document scrolling

**Files:**
- Create: `web/src/Shell.browser.test.tsx`
- Modify: `web/src/styles/tokens.css`, `web/src/styles/shell.css`, `web/src/Proposals.browser.test.tsx`, `web/src/AcceptForm.browser.test.tsx`

**Interfaces:**
- Consumes: Task 1's markup — `.library.menu-open`, `.phone-bar`, `.menu-button`, `.home`, `.search`, `#library-menu.sidebar-column`, `.scrim`.
- Produces: the token `--scrim`; below 900 px, the layout spec §7.1 describes. `Shell.browser.test.tsx` exports nothing and holds an `at(path, width)` mount helper, a `need(host, selector)` lookup and a `series` builder that Tasks 4 and 5 add cases with. The `edition` and `work` builders and the `sideways()` measure arrive in Task 4, with their first use: `tsconfig`'s `noUnusedLocals` fails the typecheck on a local nothing reads. *(Amended after Task 2 shipped them early and `pnpm -C web run typecheck` failed with TS6133 on all three.)*

- [ ] **Step 1: Write the browser tests**

Create `web/src/Shell.browser.test.tsx`:

```tsx
import { act } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { page, userEvent } from 'vitest/browser'
import './index.css'
import * as library from './library'
import { Library } from './Library'
import { Search } from './Search'
import { Series } from './Series'
import { Shell } from './Shell'
import { mount } from './testing/mount'

/**
 * The shell and the library's screens at a phone's width and a desktop's —
 * spec 2026-09-17 §7.3. jsdom lays nothing out, so what `Shell.test.tsx`
 * pins as attributes is measured here as what a browser does with them:
 * where Tab can go, whether the document scrolls, whether the page scrolls
 * sideways.
 *
 * The library's requests are mocked. Covers are requested and fail, so each
 * draws `Cover.tsx`'s placeholder, which takes the same box.
 *
 * CI does not run this project (see `vite.config.ts`: no Chromium);
 * `scripts/check-all.sh` does.
 */

vi.mock('./library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./library')>()),
  status: vi.fn(),
  home: vi.fn(),
  grid: vi.fn(),
  oneSeries: vi.fn(),
  search: vi.fn(),
}))

const series = (
  id: number,
  title: string,
  works: number,
): library.SeriesCard => ({
  id,
  title,
  works,
  editions: works,
  aligned: 0,
  added: id,
  unread: works,
  cover: `/api/library/series/${id}/cover`,
})

/** Twenty series, so the home screen is taller than a phone. */
const SERIES = Array.from({ length: 20 }, (_, i) =>
  series(i + 1, `Kaguya-sama wa Kokurasetai ${i + 1}`, 10 + i),
)

/** Every screen inside the shell it lives in, at one window width. */
async function at(path: string, width: number): Promise<HTMLElement> {
  await page.viewport(width, 844)
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route
            path="/"
            element={
              <Shell>
                <Library />
              </Shell>
            }
          />
          <Route
            path="/series/:id"
            element={
              <Shell>
                <Series />
              </Shell>
            }
          />
          <Route
            path="/search"
            element={
              <Shell>
                <Search />
              </Shell>
            }
          />
        </Routes>
      </MemoryRouter>,
    )
    // The height chain `base.css` gives `#root`, which `mount()`'s host is
    // not: without it `.library`'s `height: 100%` resolves against an
    // auto-height parent and acts as `auto`, and the desktop layout would
    // grow with its content here as it never does in the app.
    host.style.height = '100%'
    await Promise.resolve()
    await Promise.resolve()
  })
  // `Search.tsx` debounces its request by 250 ms; wait it out with real timers.
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 300))
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

function need<T extends Element = HTMLElement>(
  host: ParentNode,
  selector: string,
): T {
  const found = host.querySelector<T>(selector)
  if (!found) throw new Error(`no ${selector}`)
  return found
}

beforeEach(() => {
  localStorage.clear()
  window.scrollTo(0, 0)
  vi.mocked(library.status).mockResolvedValue({
    roots: [{ id: 1, name: 'Volumes' }],
    scanned: true,
  })
  vi.mocked(library.home).mockResolvedValue({
    continuing: [],
    added: [],
    read: [],
  })
  vi.mocked(library.grid).mockResolvedValue({ series: SERIES, unfiled: [] })
})

describe('the shell at 390 px', () => {
  it('keeps the closed menu out of Tab’s reach, and Tab inside the open one', async () => {
    const host = await at('/', 390)
    const column = need(host, '#library-menu')
    const button = need<HTMLButtonElement>(
      host,
      'button[aria-controls="library-menu"]',
    )
    const stopsIn = (inside: boolean) => {
      const focused = document.activeElement
      if (!focused || focused === document.body) return []
      return column.contains(focused) === inside
        ? [focused.outerHTML.slice(0, 80)]
        : []
    }

    // Closed: twelve Tabs from ☰ walk the bar and `<main>`, never the column.
    button.focus()
    const reachedClosed: string[] = []
    for (let i = 0; i < 12; i++) {
      await userEvent.tab()
      reachedClosed.push(...stopsIn(true))
    }
    expect(reachedClosed).toEqual([])

    // Open: focus starts inside, and neither Tab nor Shift+Tab leaves the
    // column for the bar or `<main>`. Focus may leave the document for the
    // browser's own frame, which reads as `body` and is not counted.
    await act(async () => {
      button.click()
    })
    expect(column.contains(document.activeElement)).toBe(true)
    const escaped: string[] = []
    for (let i = 0; i < 30; i++) {
      await userEvent.tab()
      escaped.push(...stopsIn(false))
    }
    need<HTMLElement>(column, 'a[href]').focus()
    for (let i = 0; i < 10; i++) {
      await userEvent.tab({ shift: true })
      escaped.push(...stopsIn(false))
    }
    expect(escaped).toEqual([])
  })

  it('stops the document scrolling while the menu is open, and only then', async () => {
    const host = await at('/', 390)
    const root = document.documentElement
    expect(getComputedStyle(root).overflowY).not.toBe('hidden')
    await act(async () => {
      need<HTMLButtonElement>(
        host,
        'button[aria-controls="library-menu"]',
      ).click()
    })
    expect(getComputedStyle(root).overflowY).toBe('hidden')
    await act(async () => {
      document.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }),
      )
    })
    expect(getComputedStyle(root).overflowY).not.toBe('hidden')
  })

  // Scrolled well past one window's height: a bar that sticks inside a
  // `.library` only as tall as the window would have scrolled away by then.
  it('scrolls the document rather than <main>, with the bar pinned all the way down', async () => {
    const host = await at('/', 390)
    expect(getComputedStyle(need(host, 'main')).overflowY).toBe('visible')
    expect(document.documentElement.scrollHeight).toBeGreaterThan(2400)
    window.scrollTo(0, 2000)
    expect(window.scrollY).toBeGreaterThan(1900)
    expect(need(host, '.phone-bar').getBoundingClientRect().top).toBe(0)
  })
})

describe('the shell at 1440 px', () => {
  it('draws the sidebar in its column and no phone bar', async () => {
    const host = await at('/', 1440)
    expect(getComputedStyle(need(host, '.phone-bar')).display).toBe('none')
    const column = need(host, '#library-menu')
    expect(getComputedStyle(column).visibility).toBe('visible')
    const box = column.getBoundingClientRect()
    expect(box.left).toBe(0)
    expect(box.width).toBe(224)
    expect(getComputedStyle(need(host, 'main')).overflowY).toBe('auto')
  })
})
```

- [ ] **Step 2: Run them and watch the narrow cases fail**

```bash
pnpm -C web exec vitest run --project browser src/Shell.browser.test.tsx --reporter=default
```

Expected: the three cases under "the shell at 390 px" FAIL — Tab reaches the column's links (☰ is not drawn yet, so focus starts from the page and walks straight into the column), the root's `overflow-y` never becomes `hidden`, and `<main>` still scrolls itself (`overflow-y: auto`) — and the 1440 case passes, since it describes the desktop layout that stands.

- [ ] **Step 3: Add the scrim's token**

In `web/src/styles/tokens.css`, after the line `  --warning: light-dark(#7d5500, #e2b55c);`, add:

```css

  /* The drawer's scrim (spec 2026-09-17 §7.1): the page behind it dimmed
   * toward the ink in Paper and toward black in Ink — the mockup's
   * `rgba(35, 30, 23, 0.35)` and `rgba(0, 0, 0, 0.55)`, as 8-digit hex. It
   * carries no text, so it has no contrast floor, and the contrast model,
   * which reads 6-digit pairs, does not count it. */
  --scrim: light-dark(#231e1759, #0000008c);
```

- [ ] **Step 4: Write the narrow layout**

In `web/src/styles/shell.css`, replace:

```css
.library > main {
  overflow-y: auto;
  padding: 1rem 1.5rem;
}
```

with:

```css
/* `min-width: 0` because a grid item's automatic minimum is its min-content
 * width: one unbreakable word inside `<main>` would otherwise widen the `1fr`
 * track past the viewport. */
.library > main {
  min-width: 0;
  overflow-y: auto;
  padding: 1rem 1.5rem;
}
```

Replace the block Task 1 appended:

```css
/* The phone bar and the scrim belong to the narrow layout, which this file
 * does not draw yet; hidden at every width until it does. */
.phone-bar,
.scrim {
  display: none;
}
```

with:

```css
/* The phone bar and the scrim exist below 900 px only — the `@media` block
 * below draws them. */
.phone-bar,
.scrim {
  display: none;
}

/* Below 900 px — spec 2026-09-17 §7.1. The sidebar column becomes a drawer
 * that ☰ slides in, and the *document* scrolls rather than `<main>`: iOS
 * Safari shrinks its toolbar, and scrolls to the top on a tap on the status
 * bar, only for the document's own scroll. So `.library` takes its content's
 * height here and the bar sticks. While the drawer is open — `menu-open`,
 * set by `Shell.tsx` — the bar and `<main>` are `inert` and the root stops
 * scrolling. `Shell.browser.test.tsx` measures all three. */
@media (width < 900px) {
  .library {
    display: block;
    height: auto;
    min-height: 100%;
  }

  .phone-bar {
    position: sticky;
    top: 0;
    z-index: 10;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 52px;
    padding: 0 var(--space-2);
    border-bottom: 1px solid var(--line);
    background: var(--bg);
  }

  .phone-bar .menu-button,
  .phone-bar .search {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 44px;
    min-height: 44px;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
  }

  .phone-bar svg {
    width: 22px;
    height: 22px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.6;
    stroke-linecap: round;
  }

  .phone-bar .home {
    flex: 1;
    color: inherit;
    font-size: var(--text-h3);
    font-weight: 700;
    text-decoration: none;
  }

  .library > main {
    overflow-y: visible;
    padding: var(--space-4);
  }

  /* Off-canvas, and `visibility: hidden` as well, so neither Tab nor a screen
   * reader reaches it while closed. Visibility waits out the slide on the way
   * out and flips at once on the way in. */
  .sidebar-column {
    position: fixed;
    inset: 0 auto 0 0;
    z-index: 30;
    width: min(20rem, 85vw);
    border-right: 1px solid var(--line);
    background: var(--bg);
    overscroll-behavior: contain;
    transform: translateX(-100%);
    visibility: hidden;
    transition:
      transform 200ms ease,
      visibility 0s linear 200ms;
  }

  .library.menu-open .sidebar-column {
    transform: none;
    visibility: visible;
    transition: transform 200ms ease;
  }

  .scrim {
    display: block;
    position: fixed;
    inset: 0;
    z-index: 20;
    padding: 0;
    border: 0;
    background: var(--scrim);
  }

  :root:has(.library.menu-open) {
    overflow: hidden;
  }
}

@media (prefers-reduced-motion: reduce) {
  .sidebar-column,
  .library.menu-open .sidebar-column {
    transition: none;
  }
}
```

- [ ] **Step 5: Run the shell's browser tests**

```bash
pnpm -C web exec vitest run --project browser src/Shell.browser.test.tsx --reporter=default
```

Expected: PASS, 4 tests.

- [ ] **Step 6: Move the two fake-shell tests to 460 px, and re-measure their figures**

```bash
sed -i 's/inboxAt(700)/inboxAt(460)/g' web/src/Proposals.browser.test.tsx
sed -i 's/panelAt(700,/panelAt(460,/g' web/src/AcceptForm.browser.test.tsx
command grep -n '700' web/src/Proposals.browser.test.tsx web/src/AcceptForm.browser.test.tsx
```

Every line the `grep` prints is a test title or a comment about the old width. Rewrite each to say 460 px, and where a comment explains the width chosen, add why it moved: *"460 px since the style pass's ②: below 900 px the sidebar leaves the flow, and 460 − 2 × 16 gives `main` the 428 px that 700 − 224 − 48 gave it beside the 14 rem sidebar."*

Those comments also quote widths measured at 700 px — in `Proposals.browser.test.tsx` the `.what` column's 323 px, and in `AcceptForm.browser.test.tsx` the panel's 426 px and the pattern's 407 px. Measure them at 460 px: add, temporarily, `console.log('what', boxOf(first, '.what'))` after `const first = …` in Proposals' "reflows at 460px" case, and `console.log('form', box(host, '.accept-form').width, 'pattern', box(host, '.field.pattern input').width)` at the top of AcceptForm's "wraps the fields at 460px" case after `panelAt`. Run:

```bash
pnpm -C web exec vitest run --project browser src/Proposals.browser.test.tsx src/AcceptForm.browser.test.tsx --reporter=default 2>&1 | command grep -E 'what|form|Tests'
```

Expected: `Tests  8 passed (8)`. Record the three printed widths for Task 6's notes. Where one differs from the figure its comment quotes, write the new one and say it was measured at 460 px on this branch; where it agrees, leave the figure. Delete both `console.log` lines, and confirm with `command grep -n 'console.log' web/src/Proposals.browser.test.tsx web/src/AcceptForm.browser.test.tsx` printing nothing.

- [ ] **Step 7: Watch the new tests fail against their own defects**

Apply each alone, run Step 5's command, confirm the named case fails, undo:

1. In the narrow `.sidebar-column` rule, delete `visibility: hidden;`. Expected: "keeps the closed menu out of Tab's reach…" FAILS with the column's links in `reachedClosed`.
2. In `web/src/Shell.tsx`, `<main inert={menuOpen}>` → `<main>`. Expected: the same case FAILS with `<main>`'s links in `escaped`.
3. In `web/src/Shell.tsx`, `<header className="phone-bar" inert={menuOpen}>` → `<header className="phone-bar">`. Expected: the same case FAILS with the bar's links in `escaped` (the Shift+Tab half).
4. Delete the `:root:has(.library.menu-open)` rule. Expected: "stops the document scrolling…" FAILS.
5. In the narrow `.library` rule, delete `height: auto;` and `min-height: 100%;`. Expected: "scrolls the document rather than <main>…" FAILS on the bar's `top`: `.library` is back to the window's height, the content overflows it, and the sticky bar leaves with its container.
6. Delete the top-level `.phone-bar, .scrim { display: none; }` rule. Expected: "draws the sidebar in its column and no phone bar" FAILS.

- [ ] **Step 8: Every project, lint, typecheck**

```bash
pnpm -C web exec vitest run --project browser --reporter=default
pnpm -C web exec vitest run --project jsdom
pnpm -C web exec vitest run --project node
pnpm -C web run lint
pnpm -C web run typecheck
```

Expected: all exit 0; the browser project reports 32 passed (28 + 4).

- [ ] **Step 9: Commit**

```bash
git add web/src/Shell.browser.test.tsx web/src/styles/tokens.css web/src/styles/shell.css web/src/Proposals.browser.test.tsx web/src/AcceptForm.browser.test.tsx
git commit -m "Style pass 2: the narrow layout — the drawer on screen, the document scrolling"
```

---

### Task 3: The sidebar — current screen, headings, settings rows

**Files:**
- Modify: `web/src/Sidebar.tsx`, `web/src/Sidebar.test.tsx`, `web/src/BrowseSettings.tsx`, `web/src/BrowseSettings.test.tsx`, `web/src/settingsFields.tsx`, `web/src/styles/shell.css`, `web/src/index.css.test.ts`

**Interfaces:**
- Consumes: Task 1's `Wordmark` as the column's first child.
- Produces: the sidebar's primary links as `NavLink`s carrying `aria-current="page"` on the current screen; root links stay `Link`. *(Amended after Task 3: the plan put `end` on `/`, and its first hand mutation, dropping it, failed nothing — react-router 8.3.0's `NavLink` counts a longer path as inside `to` only where the next character is `/`, which after `/` it never is, so `end` there is a no-op and was removed.)* `FieldControl`'s `rowStyle` prop becomes optional; `BrowseSettings` passes none.

- [ ] **Step 1: Write the failing tests**

In `web/src/Sidebar.test.tsx`, add as the last case inside `describe('the sidebar', () => {`:

```tsx
  it('marks the screen it is on, the library only on the library itself, and never a root', () => {
    const on = (path: string) =>
      mount(
        <MemoryRouter initialEntries={[path]}>
          <Sidebar
            roots={[{ id: 4, name: 'Manga' }]}
            showRoots={true}
            admin={true}
          />
        </MemoryRouter>,
      )
    const current = (host: HTMLElement) =>
      [...host.querySelectorAll('[aria-current="page"]')].map((a) =>
        a.getAttribute('href'),
      )

    expect(current(on('/'))).toEqual(['/'])
    expect(current(on('/search'))).toEqual(['/search'])
    expect(current(on('/config/roots'))).toEqual(['/config'])
    expect(current(on('/series/3'))).toEqual([])
  })
```

In `web/src/BrowseSettings.test.tsx`, add as the last case inside `describe('BrowseSettings', () => {`:

```tsx
  it('draws its rows from the stylesheet, with no inline style', () => {
    const host = mount(
      <BrowseSettings
        prefs={browseModule.defaults}
        onChange={() => {}}
        onReset={() => {}}
      />,
    )
    const styled = [...host.querySelectorAll('label')].filter((label) =>
      label.hasAttribute('style'),
    )
    expect(styled).toEqual([])
  })
```

In `web/src/index.css.test.ts`, append at the end of the file:

```ts

/**
 * The sidebar's section headings — Roots, Library, Theme. They were faded with
 * `opacity: 0.65`, which has no floor anyone checks; spec 2026-09-17 §7.1
 * gives them `--muted` instead, on the page's own ground.
 */
describe('the sidebar headings are legible in both themes', () => {
  const heading = ruleBody('.sidebar-heading')

  it('are not faded with opacity', () => {
    expect(declared(heading, 'opacity')).toBeUndefined()
  })

  for (const scheme of SCHEMES) {
    it(`clear 4.5:1 on the page's ground in ${scheme}`, () => {
      const ground = resolve(
        required(ruleBody('.library'), 'background'),
        scheme,
      )
      expect(
        contrast(resolve(required(heading, 'color'), scheme), ground),
      ).toBeGreaterThanOrEqual(4.5)
    })
  }
})
```

- [ ] **Step 2: Run them and watch them fail**

```bash
pnpm -C web exec vitest run --project jsdom src/Sidebar.test.tsx src/BrowseSettings.test.tsx --reporter=default
pnpm -C web exec vitest run --project node src/index.css.test.ts --reporter=default
```

Expected: the Sidebar case FAILS (`[]` where `['/']` was expected: `Link` sets no `aria-current`); the BrowseSettings case FAILS (six labels carry `style`); the node file's three new cases FAIL (`opacity` is `0.65`; `rule sets no color`).

- [ ] **Step 3: Mark the current screen**

In `web/src/Sidebar.tsx`, replace `import { Link } from 'react-router'` with:

```tsx
import { Link, NavLink } from 'react-router'
```

Replace the whole `<ul className="sidebar-primary">…</ul>` element with:

```tsx
      <ul className="sidebar-primary">
        <li>
          <NavLink to="/">Library</NavLink>
        </li>
        <li>
          <NavLink to="/search">Search</NavLink>
        </li>
        {admin && (
          <li>
            <NavLink to="/proposals">
              Proposals
              {openProposals !== undefined && (
                <span className="count">{openProposals}</span>
              )}
            </NavLink>
          </li>
        )}
        {admin && (
          <li>
            <NavLink to="/config">Configuration</NavLink>
          </li>
        )}
        <li>
          <NavLink to="/account">Account</NavLink>
        </li>
      </ul>
```

In the file's doc comment, before the line ` * **This paragraph once described behaviour that was not true, and that is`, add:

```tsx
 * **The screen you are on is marked by `NavLink`'s `aria-current="page"`**,
 * which `shell.css` draws in the accent (spec 2026-09-17 §7.1). `/` needs no
 * `end`: React Router's `NavLink` counts a longer path as inside `to` only
 * where the next character is `/`, and after `/` it never is — read in
 * react-router 8.3.0 — so a series page marks nothing rather than the
 * library, which `Sidebar.test.tsx` holds. `/config` has no `end`, so every
 * configuration tab marks it. The root links stay plain
 * `Link`s: `NavLink` compares the path and not the query, so every
 * `/?root=N` would read as current on the library.
 *
```

- [ ] **Step 4: Take the settings rows' inline style out**

In `web/src/settingsFields.tsx`, replace:

```tsx
 * is dark on purpose; the browse screens are UI chrome and follow
 * `color-scheme`. `rowStyle` is that disagreement, passed in. Everything else —
```

with:

```tsx
 * is dark on purpose; the browse screens are UI chrome and follow
 * `color-scheme`. `rowStyle` is that disagreement: the reader's panel passes
 * its own, and the browse panel passes none and draws its rows from
 * `shell.css`. Everything else —
```

and in the props type replace `  rowStyle: CSSProperties` with:

```tsx
  rowStyle?: CSSProperties
```

In `web/src/BrowseSettings.tsx`, delete the line `import type { CSSProperties } from 'react'` and the whole `const rowStyle: CSSProperties = { … }` block, and delete the line `          rowStyle={rowStyle}`. In its doc comment, replace:

```tsx
 * `settingsFields.tsx`) and not the chrome. `Login.tsx` is why that
 * distinction is written down rather than assumed.
 */
```

with:

```tsx
 * `settingsFields.tsx`) and not the chrome. `Login.tsx` is why that
 * distinction is written down rather than assumed. Its rows are drawn by
 * `shell.css`'s `.browse-settings label`, so it hands `FieldControl` no
 * `rowStyle`.
 */
```

- [ ] **Step 5: Restyle the column**

In `web/src/styles/shell.css`, replace:

```css
.sidebar {
  padding: 1rem;
}

.browse-settings {
  padding: 0 1rem 1rem;
}

.browse-settings button {
  margin-top: 0.5rem;
  padding: 0.25rem 0.6rem;
  border: 1px solid var(--line-strong);
  border-radius: 4px;
  background: var(--surface);
  color: var(--text);
  font-size: 0.8rem;
  cursor: pointer;
}
```

with:

```css
/* The wordmark at the column's top, at both widths (spec 2026-09-17 §7.1). */
.sidebar-column > .wordmark {
  padding: var(--space-4) var(--space-4) 0;
  font-size: var(--text-h3);
  font-weight: 700;
}

.sidebar {
  padding: var(--space-4);
}

.browse-settings {
  padding: 0 var(--space-4) var(--space-4);
}

/* One row per preference: the label's text at one end, its control at the
 * other. `FieldControl` draws a `<label>` per field; the reader's own panel
 * passes it an inline row style instead, because that panel is dark on
 * purpose. */
.browse-settings label {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  padding: var(--space-1) 0;
  font-size: var(--text-ui);
}

.browse-settings button {
  margin-top: var(--space-2);
  padding: var(--space-1) var(--space-3);
  border: 1px solid var(--line-strong);
  border-radius: var(--radius-control);
  background: var(--surface);
  color: var(--text);
  font-size: var(--text-ui);
  cursor: pointer;
}
```

Replace:

```css
.sidebar-heading {
  font-size: 0.85rem;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  opacity: 0.65;
  margin: 0 0 0.5rem;
}
```

with:

```css
/* The section headings — Roots, Library, Theme — in the title face with a
 * hairline running to the column's edge, as the mockup the user chose drew
 * them (spec 2026-09-17 §7.1). `--muted` rather than the `opacity: 0.65` they
 * had: a token has a floor `index.css.test.ts` checks, and an opacity has
 * none. */
.sidebar-heading {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin: 0 0 var(--space-2);
  color: var(--muted);
  font-family: var(--font-title);
  font-size: var(--text-ui);
  font-weight: 600;
  line-height: 1.25;
}

.sidebar-heading::after {
  content: "";
  flex: 1;
  height: 1px;
  background: var(--line);
}
```

Replace:

```css
.sidebar a {
  color: var(--accent);
  text-decoration: none;
}
```

with:

```css
/* Navigation in the text colour, and the screen you are on in the accent —
 * `NavLink` marks it `aria-current="page"` (`Sidebar.tsx`). */
.sidebar a {
  color: var(--text);
  text-decoration: none;
}

.sidebar a[aria-current="page"] {
  color: var(--accent);
}
```

Replace the theme control's legend rule and the comment above it —

```css
/* The legend carries `.sidebar-heading` like the sidebar's `h2`s, but it is
 * not a heading, so `base.css`'s heading rule — the title face, 600, a 1.25
 * line — does not reach it. Without these three it drew in the UI face at 400
 * on a taller line than "Roots" above it. */
.theme-control legend {
  padding: 0;
  font-family: var(--font-title);
  font-weight: 600;
  line-height: 1.25;
}
```

— with:

```css
/* The legend carries `.sidebar-heading` like the sidebar's `h2`s, and that
 * rule now sets the face, the weight and the line itself. A legend shrinks to
 * its text by default, and the heading's hairline reaches the column's edge
 * only if the legend does. */
.theme-control legend {
  width: 100%;
  padding: 0;
}
```

Append at the end of the file:

```css

/* 44 px rows for a finger (spec 2026-09-17 §7.1). */
@media (pointer: coarse) {
  .sidebar-primary a,
  .sidebar-roots a {
    display: flex;
    align-items: center;
    min-height: 44px;
  }

  .browse-settings label,
  .browse-settings button {
    min-height: 44px;
  }

  .theme-control label {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 44px;
  }
}
```

- [ ] **Step 6: Run the tests**

```bash
pnpm -C web exec vitest run --project jsdom src/Sidebar.test.tsx src/BrowseSettings.test.tsx src/Shell.test.tsx src/Settings.test.tsx --reporter=default
pnpm -C web exec vitest run --project node src/index.css.test.ts --reporter=default
```

Expected: PASS, every case; `Settings.test.tsx` is the reader's panel, which still passes its own `rowStyle`.

- [ ] **Step 7: Watch the new tests fail against their own defects**

Apply each alone, run Step 6's commands, confirm, undo:

1. *(Removed: dropping `end` from `/` failed nothing, because `end` there is a no-op — see this task's Interfaces.)* The Library link's marking is held by mutation 2, and not marking it on a series page by the test's `/series/3` expectation.
2. The Library `NavLink` → `<Link to="/">Library</Link>`. Expected: the Sidebar case FAILS at `/` with `[]`.
3. The root links' `Link` → `NavLink`. Expected: the Sidebar case FAILS at `/` with `['/', '/?root=4']`.
4. Put `rowStyle={{ display: 'flex' }}` back on `BrowseSettings`' `FieldControl`. Expected: the BrowseSettings case FAILS.
5. Add `opacity: 0.65;` to `.sidebar-heading`. Expected: "are not faded with opacity" FAILS.
6. `.sidebar-heading`'s `color: var(--muted)` → `color: var(--line-strong)`. Expected: "clear 4.5:1 … in paper" FAILS at 3.49.

- [ ] **Step 8: Every project, lint, typecheck**

```bash
pnpm -C web exec vitest run --project jsdom
pnpm -C web exec vitest run --project node
pnpm -C web exec vitest run --project browser --reporter=default
pnpm -C web run lint
pnpm -C web run typecheck
```

Expected: all exit 0; browser 32 passed.

- [ ] **Step 9: Commit**

```bash
git add web/src/Sidebar.tsx web/src/Sidebar.test.tsx web/src/BrowseSettings.tsx web/src/BrowseSettings.test.tsx web/src/settingsFields.tsx web/src/styles/shell.css web/src/index.css.test.ts
git commit -m "Style pass 2: the sidebar marks the current screen, and its headings and rows take tokens"
```

---

### Task 4: The library home — cover cards, tabs and counts

**Files:**
- Modify: `web/src/Library.tsx`, `web/src/Library.test.tsx`, `web/src/SeriesGrid.tsx`, `web/src/styles/library.css`, `web/src/Shell.browser.test.tsx`

**Interfaces:**
- Consumes: `Cover` (`Cover.tsx`), `workCover(slug, edition)` and `unreadable(w)` (`library.ts`); Task 2's `at`, `need` and `series` in `Shell.browser.test.tsx`.
- Produces: in `Shell.browser.test.tsx`, the `edition` and `work` builders and `sideways()`, which Task 5 uses too. Each continue card is `li > (a[href="/read/<slug>"] | span.tile-unreadable)` holding a `Cover`, `span.title`, and `span.meta` "Vol. N" when `volume` is set; an unreadable card adds `span.unreadable`. Each series tile gains `span.meta` after its `span.title`. The Added button carries `class="sort"`.

- [ ] **Step 1: Write the failing tests**

In `web/src/Library.test.tsx`, add as the last cases inside `describe('the library screen', () => {`:

```tsx
  it('draws each continue card from the edition its reader will open', async () => {
    vi.mocked(library.home).mockResolvedValue({
      continuing: [
        work({
          slug: 'half-read-v02',
          opens: 'en',
          editions: [
            { key: 'raw', label: 'Raw', members: 1, pages: 180, missing: 0 },
            { key: 'en', label: 'En', members: 1, pages: 178, missing: 0 },
          ],
        }),
      ],
      added: [],
      read: [],
    })
    const host = await show()
    const strip = host.querySelector('[data-testid="continue"]')
    expect(strip?.querySelector('img')?.getAttribute('src')).toBe(
      '/api/works/half-read-v02/en/pages/0/thumb',
    )
  })

  it('says which volume a continue card is, and nothing where it has no number', async () => {
    vi.mocked(library.home).mockResolvedValue({
      continuing: [
        work({ slug: 'numbered', title: 'Numbered v03', volume: 3 }),
        work({ slug: 'special', title: 'A Special', volume: null }),
      ],
      added: [],
      read: [],
    })
    const host = await show()
    const cards = [
      ...host.querySelectorAll('[data-testid="continue"] li'),
    ]
    expect(cards.map((c) => c.querySelector('.meta')?.textContent)).toEqual([
      'Vol. 3',
      undefined,
    ])
  })

  it('keeps the cover on a continue card whose files are gone', async () => {
    vi.mocked(library.home).mockResolvedValue({
      continuing: [
        work({
          slug: 'gone-v01',
          editions: [
            { key: 'raw', label: 'Raw', members: 1, pages: 180, missing: 1 },
          ],
        }),
      ],
      added: [],
      read: [],
    })
    const host = await show()
    const strip = host.querySelector('[data-testid="continue"]')
    expect(strip?.querySelector('a')).toBeNull()
    expect(strip?.querySelector('.tile-unreadable img')).not.toBeNull()
  })

  it('says how many volumes each series holds', async () => {
    vi.mocked(library.grid).mockResolvedValue({
      series: [
        series({ id: 1, title: 'Many', works: 42 }),
        series({ id: 2, title: 'One', works: 1 }),
      ],
      unfiled: [],
    })
    const host = await show()
    expect(
      [...host.querySelectorAll('.grid .meta')].map((m) => m.textContent),
    ).toEqual(['42 volumes', '1 volume'])
  })
```

In `web/src/Shell.browser.test.tsx`, add after the `series` builder (before `SERIES` — a module-level constant built from them must come after them):

```tsx
const edition = (key: string, label: string): library.EditionCard => ({
  key,
  label,
  members: 1,
  pages: 180,
  missing: 0,
})

const work = (
  id: number,
  title: string,
  editions: library.EditionCard[],
): library.WorkCard => ({
  id,
  slug: `work-${id}`,
  title,
  volume: id,
  aligned: false,
  pages: 180,
  editions,
  opens: editions[0]?.key ?? 'raw',
})
```

after the `need` function:

```tsx
/** How far the page itself scrolls sideways: zero is the spec's promise. */
const sideways = () =>
  document.documentElement.scrollWidth - document.documentElement.clientWidth
```

and after the `SERIES` constant:

```tsx
/** Six half-read volumes with long titles, more than a phone's width holds. */
const CONTINUING = Array.from({ length: 6 }, (_, i) =>
  work(
    i + 1,
    `Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunousen - Full Color Edition v0${i + 1}`,
    [edition('raw', 'Raw')],
  ),
)
```

and add a new `describe` at the end of the file:

```tsx
describe('the library home at 390 px', () => {
  it('lays out without sideways scroll, with its chips on one line', async () => {
    vi.mocked(library.home).mockResolvedValue({
      continuing: CONTINUING,
      added: [],
      read: [],
    })
    const host = await at('/', 390)
    expect(sideways()).toBe(0)
    const tops = [...host.querySelectorAll('.chips button')].map(
      (b) => b.getBoundingClientRect().top,
    )
    expect(tops).toHaveLength(4)
    expect(new Set(tops).size).toBe(1)
  })
})
```

- [ ] **Step 2: Run them and watch them fail**

```bash
pnpm -C web exec vitest run --project jsdom src/Library.test.tsx --reporter=default
pnpm -C web exec vitest run --project browser src/Shell.browser.test.tsx --reporter=default
```

Expected: the four new Library cases FAIL (no `img` in the strip; no `.meta`); the browser case FAILS — the pill chips wrap at 390 px, so `tops` holds more than one value.

- [ ] **Step 3: Cover cards in the continue strip, and the sort chip marked**

In `web/src/Library.tsx`, add after `import { useBrowsePrefs } from './browseModule'`:

```tsx
import { Cover } from './Cover'
```

In the `from './library'` import list, add `type WorkCard,` after `unreadable,` and `workCover,` after that, so the list ends:

```tsx
  unreadable,
  type WorkCard,
  workCover,
} from './library'
```

Add after the `CHIPS` constant:

```tsx
/**
 * One card in the continue strip — spec 2026-09-17 §7.2. The cover is the
 * thumbnail of the edition this reader's reader will open (`opens`, O84),
 * not the series cover, because the strip is about a volume. What the card
 * cannot show is where the reader is in it: `progress` holds a slot, and the
 * home route sends no page — O132.
 */
function ContinueCard({ w }: { w: WorkCard }) {
  return (
    <>
      <Cover src={workCover(w.slug, w.opens)} title={w.title} />
      <span className="title">{w.title}</span>
      {w.volume !== null && <span className="meta">Vol. {w.volume}</span>}
    </>
  )
}
```

In the strip's list items, below the O62 comment (which stays), replace:

```tsx
                  <span className="tile-unreadable">
                    {w.title}{' '}
                    <span className="unreadable">
                      files missing — cannot be opened
                    </span>
                  </span>
                ) : (
                  <Link to={`/read/${w.slug}`}>{w.title}</Link>
                )}
```

with:

```tsx
                  <span className="tile-unreadable">
                    <ContinueCard w={w} />
                    <span className="unreadable">
                      files missing — cannot be opened
                    </span>
                  </span>
                ) : (
                  <Link to={`/read/${w.slug}`}>
                    <ContinueCard w={w} />
                  </Link>
                )}
```

and on the Added button, add `className="sort"` after `type="button"`:

```tsx
        <button
          type="button"
          className="sort"
          aria-pressed={sort === 'added'}
          onClick={() => setSort(sort === 'added' ? 'title' : 'added')}
        >
```

- [ ] **Step 4: A count under each series title**

In `web/src/SeriesGrid.tsx`, add after the imports:

```tsx
/** "42 volumes", "1 volume": a series' `works`, as its tile says it. */
function volumeCount(works: number): string {
  return works === 1 ? '1 volume' : `${works} volumes`
}
```

and replace `            <span className="title">{s.title}</span>` (inside the series `Link`, not the unfiled one) with:

```tsx
            <span className="title">{s.title}</span>
            <span className="meta">{volumeCount(s.works)}</span>
```

- [ ] **Step 5: Restyle the home screen**

In `web/src/styles/library.css`, replace the chips block —

```css
.chips {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  margin: 1rem 0;
}

.chips button {
  border: 1px solid var(--line-strong);
  background: var(--surface);
  color: var(--text);
  border-radius: 999px;
  padding: 0.35rem 0.9rem;
  font: inherit;
  cursor: pointer;
}

.chips button[aria-pressed="true"] {
  background: var(--accent);
  color: var(--on-accent);
  border-color: var(--accent);
}
```

— with:

```css
/* The chips as underlined tabs, as the mockup the user chose drew them (spec
 * 2026-09-17 §7.2): the pressed one in the text colour over the accent, the
 * rest muted. "Added" (`.sort`) sits at the row's far end, because it sorts
 * where the others filter. */
.chips {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: var(--space-1) var(--space-5);
  margin: var(--space-5) 0 var(--space-4);
}

.chips button {
  padding: var(--space-1) 0;
  border: 0;
  border-radius: 0;
  background: none;
  color: var(--muted);
  font-size: var(--text-ui);
  cursor: pointer;
}

.chips button[aria-pressed="true"] {
  color: var(--text);
  box-shadow: inset 0 -2px 0 var(--accent);
}

.chips .sort {
  margin-left: auto;
}
```

Replace everything from `.continue ul {` through the end of the `.continue .tile-unreadable { color: var(--text); }` rule — the four `.continue` rules and the comment between them — with:

```css
/* The continue strip — spec 2026-09-17 §7.2: a row of cover cards under a
 * heading with a hairline after it. The row scrolls sideways inside itself,
 * so the page never does. `.tile-unreadable` is the same card with no link,
 * for a work whose files have left the disk — O62, applied here for the
 * reason I1 of the whole-branch review names this strip: the reader was
 * mid-read when the files went. */
.continue h2 {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  margin: 0 0 var(--space-3);
  font-size: var(--text-h3);
}

.continue h2::after {
  content: "";
  flex: 1;
  height: 1px;
  background: var(--line);
}

.continue ul {
  list-style: none;
  margin: 0;
  padding: 0 0 var(--space-2);
  display: flex;
  gap: var(--space-3);
  overflow-x: auto;
}

.continue li {
  flex: 0 0 104px;
}

.continue a,
.continue .tile-unreadable {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  color: inherit;
  text-decoration: none;
}
```

Replace the shared cover rule —

```css
.grid img,
.grid .cover-missing,
.volumes img,
.volumes .cover-missing {
  width: 100%;
  aspect-ratio: 2 / 3;
  border: 1px solid var(--line-strong);
  border-radius: 4px;
  background: var(--surface);
}

.grid img,
.volumes img {
  object-fit: cover;
}

.grid .cover-missing,
.volumes .cover-missing {
```

— with (the placeholder rule's body below its selectors is unchanged):

```css
/* Square corners and a hairline (spec 2026-09-17 §2 and §7.2): the cover is
 * the artwork, and the rule only says where it ends. */
.grid img,
.grid .cover-missing,
.volumes img,
.volumes .cover-missing,
.continue img,
.continue .cover-missing {
  width: 100%;
  aspect-ratio: 2 / 3;
  box-shadow: 0 0 0 1px var(--line);
  background: var(--surface);
}

.grid img,
.volumes img,
.continue img {
  object-fit: cover;
}

.grid .cover-missing,
.volumes .cover-missing,
.continue .cover-missing {
```

Replace:

```css
.grid .title,
.volumes .title {
  font-size: 0.9rem;
  line-height: 1.3;
}
```

with:

```css
/* Titles in the title face; `anywhere` so a long file name breaks rather
 * than widening its column. */
.grid .title,
.volumes .title,
.continue .title {
  font-family: var(--font-title);
  font-size: var(--text-ui);
  font-weight: 600;
  line-height: 1.3;
  overflow-wrap: anywhere;
}

.continue .title {
  display: -webkit-box;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
  overflow: hidden;
}

.grid .meta,
.continue .meta {
  color: var(--muted);
  font-size: var(--text-meta);
}

/* "files missing — cannot be opened", wherever a tile says it. */
.unreadable {
  color: var(--warning);
  font-size: var(--text-meta);
}
```

Append at the end of the file:

```css

/* The empty states (`EmptyState.tsx`), which had no rules of their own: the
 * heading is `base.css`'s, the prose muted, the command in a box of its own
 * that scrolls rather than widening the page. */
.empty p {
  max-width: 40rem;
  color: var(--muted);
}

.empty pre {
  overflow-x: auto;
  padding: var(--space-3);
  background: var(--surface);
  font-size: var(--text-ui);
}

/* 44 px targets for a finger (spec 2026-09-17 §7.1). */
@media (pointer: coarse) {
  .chips button {
    min-height: 44px;
  }
}
```

- [ ] **Step 6: Run the tests**

```bash
pnpm -C web exec vitest run --project jsdom src/Library.test.tsx src/Search.test.tsx --reporter=default
pnpm -C web exec vitest run --project node src/index.css.test.ts src/colourLiterals.test.ts --reporter=default
pnpm -C web exec vitest run --project browser src/Shell.browser.test.tsx --reporter=default
```

Expected: PASS, every case. The existing continue-strip cases still pass — the unreadable card still names its work and carries `.unreadable`.

- [ ] **Step 7: Watch the new tests fail against their own defects**

Apply each alone, run the command that holds the named case, confirm, undo:

1. In `ContinueCard`, `workCover(w.slug, w.opens)` → `workCover(w.slug, w.editions[0]?.key ?? '')`. Expected: "draws each continue card from the edition its reader will open" FAILS (`raw` for `en`).
2. `{w.volume !== null && <span className="meta">Vol. {w.volume}</span>}` → `<span className="meta">Vol. {w.volume}</span>`. Expected: "says which volume…" FAILS with `'Vol. '` for the second card.
3. In the unreadable branch, delete `<ContinueCard w={w} />` and put back `{w.title}{' '}`. Expected: "keeps the cover on a continue card whose files are gone" FAILS.
4. `works === 1 ? '1 volume' : …` → `` `${works} volumes` ``. Expected: "says how many volumes…" FAILS with `'1 volumes'`.
5. In `.continue ul`, delete `overflow-x: auto;`. Expected: the browser case FAILS on `sideways()`.
6. In `.chips`, `gap: var(--space-1) var(--space-5)` → `gap: var(--space-1) 6rem`. Expected: the browser case FAILS on the chips' tops.

- [ ] **Step 8: Every project, lint, typecheck**

```bash
pnpm -C web exec vitest run --project jsdom
pnpm -C web exec vitest run --project node
pnpm -C web exec vitest run --project browser --reporter=default
pnpm -C web run lint
pnpm -C web run typecheck
```

Expected: all exit 0; browser 33 passed.

- [ ] **Step 9: Commit**

```bash
git add web/src/Library.tsx web/src/Library.test.tsx web/src/SeriesGrid.tsx web/src/styles/library.css web/src/Shell.browser.test.tsx
git commit -m "Style pass 2: the library home — cover cards, tabs and volume counts"
```

---

### Task 5: The series page and search

**Files:**
- Modify: `web/src/Series.tsx`, `web/src/Series.test.tsx`, `web/src/styles/library.css`, `web/src/index.css.test.ts`, `web/src/Shell.browser.test.tsx`

**Interfaces:**
- Consumes: Task 2's `at`, `need` and `series`, and Task 4's `edition`, `work` and `sideways()`, in `Shell.browser.test.tsx`.
- Produces: the series table inside `div.table-scroll`; each cell holding an edition's page count carries `class="pages"`; `td.missing` drawn in `--muted` with a full-strength dashed `--line-strong` border.

- [ ] **Step 1: Write the failing tests**

In `web/src/Series.test.tsx`, add as the last cases inside `describe('Series, table layout', () => {`:

```tsx
  it('sits in a box of its own that scrolls sideways, so the page never does', async () => {
    const host = await settleWith({ layout: 'table' })
    expect(host.querySelector('.table-scroll > table.editions')).not.toBeNull()
  })

  it('marks the page counts, and only them, for the figures face', async () => {
    const host = await settleWith({ layout: 'table' })
    const marked = [...host.querySelectorAll('.editions tbody tr')].map((row) =>
      [...row.querySelectorAll('td')].map((td) =>
        td.classList.contains('pages'),
      ),
    )
    // The second volume has no `en` edition: its third cell is the dash.
    expect(marked).toEqual([
      [false, true, true],
      [false, true, false],
    ])
  })
```

In `web/src/index.css.test.ts`, append at the end of the file:

```ts

/**
 * The series table's missing cell — spec 2026-09-17 §5 ② and §7.2. It was
 * `opacity: 0.55` over the text colour, 3.60:1 on Paper's ground; it is now
 * `--muted`, with its dashed border back at full `--line-strong`.
 */
describe('the series table’s missing cell is legible in both themes', () => {
  const missing = ruleBody('.editions td.missing')

  it('is not faded with opacity', () => {
    expect(declared(missing, 'opacity')).toBeUndefined()
  })

  for (const scheme of SCHEMES) {
    it(`draws its dash at 4.5:1 and its border at 3:1 in ${scheme}`, () => {
      const ground = resolve(
        required(ruleBody('.library'), 'background'),
        scheme,
      )
      const border = required(missing, 'border').match(/var\(--[a-z-]+\)/)
      if (!border) throw new Error('the missing cell’s border names no token')
      expect(
        contrast(resolve(required(missing, 'color'), scheme), ground),
      ).toBeGreaterThanOrEqual(4.5)
      expect(contrast(resolve(border[0], scheme), ground)).toBeGreaterThanOrEqual(
        3,
      )
    })
  }
})

/**
 * The figures face on the page counts only — spec 2026-09-17 §5 ②. `base.css`
 * sets it on every table inside `.library`, which drew the digits inside a
 * volume's file name heavier than the title around them; the series table
 * takes the UI face and gives the figures face back to its page-count cells.
 */
describe('the figures face reaches the series table’s page counts only', () => {
  it('sets the UI face on the table and the figures face on its page counts', () => {
    expect(declared(ruleBody('.editions'), 'font-family')).toBe(
      'var(--font-ui)',
    )
    expect(declared(ruleBody('.editions td.pages'), 'font-family')).toBe(
      'var(--font-figures)',
    )
  })

  it('is named by no other rule in the screens’ stylesheets', () => {
    const naming = rules(cssSource)
      .filter((rule) => rule.body.includes('--font-figures'))
      .map((rule) => rule.prelude)
    expect(naming).toEqual(['.editions td.pages'])
  })
})
```

In `web/src/Shell.browser.test.tsx`, add after the `CONTINUING` constant:

```tsx
/**
 * Five editions whose labels are real ones — a label is generated per volume
 * from member filenames — so the edition selector's longest option and the
 * table's columns are as wide as a real series makes them.
 */
const EDITIONS = [
  'Manga (RAW)',
  'Go-Toubun no Hanayome v03 (RAW) [c024-036].cbz + 11 more',
  'Go-Toubun no Hanayome - Fan Translated - c000.cbz + 6 more',
  'Manga',
  'Go-Toubun no Hanayome - Full Color Edition [Digital] v01.cbz',
].map((label, i) => edition(`e${i}`, label))

/** What `oneSeries` resolves to — an inline type in `library.ts`, spelled out here. */
const SERIES_PAGE: {
  series: library.SeriesDetail
  works: library.WorkCard[]
} = {
  series: {
    ...series(7, 'Go-Toubun no Hanayome', 3),
    titles: [
      { title: 'Go-Toubun no Hanayome', kind: 'romaji', primary: true },
    ],
  },
  works: [1, 2, 3].map((i) =>
    work(
      i,
      `Go-Toubun no Hanayome - v0${i} (2018) (Digital) (danke-Empire).cbz`,
      EDITIONS,
    ),
  ),
}
```

Then add a new `describe` at the end of the file:

```tsx
describe('the series page and search at 390 px', () => {
  beforeEach(() => {
    vi.mocked(library.oneSeries).mockResolvedValue(SERIES_PAGE)
    vi.mocked(library.search).mockResolvedValue({
      query: 'go-toubun',
      series: SERIES,
      unfiled: [],
    })
  })

  it('lays the series grid out without sideways scroll, edition selector and all', async () => {
    const host = await at('/series/7', 390)
    expect(host.querySelector('.volumes')).not.toBeNull()
    expect(host.querySelector('.edition-selector select')).not.toBeNull()
    expect(sideways()).toBe(0)
  })

  it('lays the series table out without sideways scroll, the table scrolling in its box', async () => {
    localStorage.setItem('taiyaku.prefs.browse', JSON.stringify({ layout: 'table' }))
    const host = await at('/series/7', 390)
    const box = need(host, '.table-scroll')
    expect(box.scrollWidth).toBeGreaterThan(box.clientWidth)
    expect(sideways()).toBe(0)
  })

  it('lays search out without sideways scroll', async () => {
    const host = await at('/search?q=go-toubun', 390)
    expect(host.querySelector('.grid')).not.toBeNull()
    expect(sideways()).toBe(0)
  })
})
```

- [ ] **Step 2: Run them and watch them fail**

```bash
pnpm -C web exec vitest run --project jsdom src/Series.test.tsx --reporter=default
pnpm -C web exec vitest run --project node src/index.css.test.ts --reporter=default
pnpm -C web exec vitest run --project browser src/Shell.browser.test.tsx --reporter=default
```

Expected: the two Series cases FAIL (no `.table-scroll`; no `pages` class); the node file's new cases FAIL (`opacity` is `0.55`; `.editions` sets no `font-family`; `no rule found for .editions td.pages`); of the three browser cases, the grid and table cases FAIL on `sideways()` — the `<select>` sits at its longest option's width, and the table has no box to scroll in — while search passes, since Task 2's layout already holds it. Search's case is watched failing in Step 7.

- [ ] **Step 3: A scroll box and a `pages` class on the table**

In `web/src/Series.tsx`'s `Table`, replace `    <table className="editions">` with:

```tsx
    <div className="table-scroll">
      <table className="editions">
```

and its closing `    </table>` with:

```tsx
      </table>
    </div>
```

(re-indent the table's body by two spaces, or let `biome format --write src/Series.tsx` do it), and in the cell for a held edition replace:

```tsx
                <td
                  key={c.key}
                  // Per edition, because `missing` is: one edition of a
```

with:

```tsx
                <td
                  key={c.key}
                  className="pages"
                  // Per edition, because `missing` is: one edition of a
```

In `Table`'s doc comment, before its closing ` */`, add:

```tsx
 *
 * **Two things for a phone** (spec 2026-09-17 §7.2): the table sits in a box
 * that scrolls sideways, so a series with several editions never makes the
 * page do so; and each page count is `pages`, the one cell `library.css`
 * draws in the figures face — the table itself takes the UI face, so the
 * digits inside a volume's file name are the title's own.
```

- [ ] **Step 4: Restyle the series page and search**

In `web/src/styles/library.css`, replace:

```css
.cover-box .aligned {
  position: absolute;
  right: 0.25rem;
  bottom: 0.25rem;
  padding: 0.1rem 0.35rem;
  border: 1px solid var(--line-strong);
  border-radius: 4px;
```

with:

```css
.cover-box .aligned {
  position: absolute;
  right: 0.25rem;
  bottom: 0.25rem;
  padding: 0.1rem 0.35rem;
  border: 1px solid var(--line-strong);
  border-radius: var(--radius-control);
```

Replace the table's rules, from

```css
/* §3's preset B. Tokens, like every other rule in this block. */
.editions {
  border-collapse: collapse;
  width: 100%;
}

.editions th,
.editions td {
  border-bottom: 1px solid var(--line-strong);
  padding: 0.45rem 0.6rem;
  text-align: left;
  font-size: 0.9rem;
}

.editions th {
  font-weight: 600;
}
```

with:

```css
/* §3's preset B, in a box that scrolls sideways (spec 2026-09-17 §7.2). The
 * table takes the UI face and gives the figures face back to its page-count
 * cells only: `base.css` puts the figures face on every table, and here that
 * drew the digits inside a volume's file name heavier than its title. Its
 * headings are `base.css`'s — muted, 500. */
.table-scroll {
  max-width: 100%;
  overflow-x: auto;
}

.editions {
  border-collapse: collapse;
  width: 100%;
  font-family: var(--font-ui);
}

.editions th,
.editions td {
  border-bottom: 1px solid var(--line);
  padding: var(--space-2) var(--space-3);
  text-align: left;
  font-size: var(--text-ui);
}

.editions td.pages {
  font-family: var(--font-figures);
  font-variant-numeric: tabular-nums;
}
```

Replace the missing cell's rule and its comment —

```css
/* A dashed border rather than a lighter colour: the cell means "this edition
 * is not here", and opacity alone reads as "loading" against a screen whose
 * covers arrive one at a time. */
.editions td.missing {
  border: 1px dashed var(--line-strong);
  color: var(--text);
  opacity: 0.55;
  text-align: center;
}
```

— with:

```css
/* A dashed border, because the cell means "this edition is not here" and a
 * fade alone reads as "loading" against a screen whose covers arrive one at a
 * time. The dash is `--muted` rather than the text colour at `opacity: 0.55`,
 * which measured 3.60:1 on Paper's ground (spec 2026-09-17 §5 ②): the token
 * clears 4.5:1 in both themes, and the border stays at full `--line-strong`.
 * `index.css.test.ts` holds both. */
.editions td.missing {
  border: 1px dashed var(--line-strong);
  color: var(--muted);
  text-align: center;
}
```

Replace:

```css
.edition-selector {
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
  margin: 0 0 1rem;
  font-size: 0.9rem;
}
```

with:

```css
/* A `<select>` is as wide as its longest option, and a real edition label is
 * a file name with a count after it — so the control may shrink, and the page
 * never widens for it. */
.edition-selector {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  max-width: 100%;
  margin: 0 0 var(--space-4);
  font-size: var(--text-ui);
}

.edition-selector select {
  min-width: 0;
  max-width: 100%;
}
```

Replace the download control's rule —

```css
.volumes a.get {
  display: inline-block;
  align-self: start;
  margin-top: 0.15rem;
  padding: 0 0.4rem;
  border: 1px solid var(--line-strong);
  border-radius: 999px;
```

— with:

```css
.volumes a.get {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  align-self: start;
  min-width: 24px;
  min-height: 24px;
  margin-top: 0.15rem;
  padding: 0 0.4rem;
  border: 1px solid var(--line-strong);
  border-radius: var(--radius-control);
```

and in the comment above it replace `**\`display: inline-block\` is load-bearing, not tidying.**` with `` **An inline display is load-bearing, not tidying** — `inline-flex` since the style pass, so its 24 px box centres the arrow.``

Replace in `.volumes .panel-open`:

```css
.volumes .panel-open {
  margin-top: 0.15rem;
  padding: 0 0.4rem;
  border: 1px solid var(--line-strong);
  border-radius: 999px;
```

with:

```css
.volumes .panel-open {
  min-width: 24px;
  min-height: 24px;
  margin-top: 0.15rem;
  padding: 0 0.4rem;
  border: 1px solid var(--line-strong);
  border-radius: var(--radius-control);
```

Replace in `.alignment-panel`:

```css
.alignment-panel {
  margin-top: 0.35rem;
  padding: 0.5rem 0.6rem;
  border: 1px solid var(--line-strong);
  border-radius: 6px;
```

with:

```css
.alignment-panel {
  margin-top: 0.35rem;
  padding: 0.5rem 0.6rem;
  border: 1px solid var(--line);
  border-radius: 0;
```

Replace the search box's rules —

```css
.search-box {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin: 0 0 1rem;
}

.search-box input {
  flex: 1;
  max-width: 24rem;
  padding: 0.35rem 0.5rem;
  border: 1px solid var(--line-strong);
  border-radius: 4px;
  background: var(--bg);
  color: var(--text);
  font: inherit;
}
```

— with:

```css
/* The field on `--field` like every input `base.css` draws — it overrode that
 * with `--bg` until the style pass — at most 24 rem wide, and the whole row on
 * a phone. Its border, corners, colour and font are `base.css`'s. */
.search-box {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin: 0 0 var(--space-4);
  font-size: var(--text-ui);
}

.search-box input {
  flex: 1;
  min-width: 0;
  max-width: 24rem;
  padding: var(--space-2) var(--space-3);
  background: var(--field);
}

/* A series title breaks rather than widening the page. */
.library > main h1 {
  overflow-wrap: anywhere;
}
```

In the `@media (pointer: coarse)` block Task 4 appended at the end of the file, add after the `.chips button` rule:

```css

  .volumes a.get,
  .volumes .panel-open {
    min-width: 44px;
    min-height: 44px;
  }
```

and append at the end of the file:

```css

@media (width < 900px) {
  .search-box input {
    max-width: none;
  }
}
```

- [ ] **Step 5: Run the tests**

```bash
pnpm -C web exec vitest run --project jsdom src/Series.test.tsx src/Search.test.tsx --reporter=default
pnpm -C web exec vitest run --project node src/index.css.test.ts src/colourLiterals.test.ts --reporter=default
pnpm -C web exec vitest run --project browser src/Shell.browser.test.tsx --reporter=default
```

Expected: PASS, every case; D2 and D3 in `index.css.test.ts` still pass, reading `.alignment-panel`'s and `.volumes .panel-open`'s unchanged `background` and `color`.

- [ ] **Step 6: Confirm the old ratio before trusting the new one**

The spec's 3.60:1 for the old cell came from outside any test. Reproduce it with the model: in the new "draws its dash…" case, temporarily replace the `color` line's check with `contrast(composite(token('text', scheme), ground, 0.55), ground)` and log it with `console.log(scheme, …)`; run the node command; expected `paper 3.6…` and `ink 5.1…`. Undo, and confirm `git diff web/src/index.css.test.ts` shows only Step 1's additions.

- [ ] **Step 7: Watch the new tests fail against their own defects**

Apply each alone, run the command that holds the named case, confirm, undo:

1. In `Series.tsx`, drop the `<div className="table-scroll">` wrapper. Expected: "sits in a box of its own…" FAILS; with `.table-scroll` gone from the page too, the browser table case FAILS on `need`.
2. Move `className="pages"` from the held cell to the Volume cell. Expected: "marks the page counts, and only them…" FAILS.
3. Put `opacity: 0.55;` back in `.editions td.missing`. Expected: "is not faded with opacity" FAILS.
4. `.editions td.missing`'s `color: var(--muted)` → `color: var(--line-strong)`. Expected: "draws its dash … in paper" FAILS at 3.49.
5. Its border's `var(--line-strong)` → `var(--line)`. Expected: "draws its dash … in paper" FAILS on the border.
6. Delete `font-family: var(--font-ui);` from `.editions`. Expected: "sets the UI face on the table…" FAILS.
7. Add `font-family: var(--font-figures);` to `.editions th, .editions td`. Expected: "is named by no other rule…" FAILS.
8. Delete `.edition-selector select { … }`. Expected: the browser grid case FAILS on `sideways()`.
9. In `.table-scroll`, delete `overflow-x: auto;`. Expected: the browser table case FAILS.
10. Add `min-width: 30rem;` to `.search-box input`. Expected: the browser search case FAILS on `sideways()`. This one is contrived, and the notes should say so: nothing this task writes is what keeps search from scrolling sideways — Task 2's layout and search's existing rules already do — so the mutation shows only that the case measures the page on that screen, not that any line here holds it. (Deleting the narrow `.library` rule would *not* fail it: at the desktop layout `<main>` has `min-width: 0` and scrolls itself, so the page never does.)

- [ ] **Step 8: Every project, lint, typecheck**

```bash
pnpm -C web exec vitest run --project jsdom
pnpm -C web exec vitest run --project node
pnpm -C web exec vitest run --project browser --reporter=default
pnpm -C web run lint
pnpm -C web run typecheck
```

Expected: all exit 0; browser 36 passed.

- [ ] **Step 9: Commit**

```bash
git add web/src/Series.tsx web/src/Series.test.tsx web/src/styles/library.css web/src/index.css.test.ts web/src/Shell.browser.test.tsx
git commit -m "Style pass 2: the series page and search — the table in its box, figures on page counts"
```

---

### Task 6: Measure it in a browser and on the phone, record it, and run the gate

Steps 3 to 6 need the running app, and Step 6 needs the user holding their iPhone. **The controller runs this task, not a subagent.**

**Files:**
- Modify: `docs/notes.md` (append a section), `docs/status.md` (test counts; O132), `docs/superpowers/specs/2026-09-17-style-pass-design.md` (§7.1's scroll-lock sentence, and anything the measurements contradict)

**Interfaces:**
- Consumes: every earlier task.
- Produces: the figures `notes.md` records, O132 in `status.md`, and a green gate.

- [ ] **Step 1: Serve the scratch copy of the real library**

This branch changes nothing under `crates/` (`git diff --stat main -- crates` prints nothing), so `target/debug/taiyaku`, built at `dbd47a3`, is current. The scratch copy from ① is at `target/style-pass/lib/library.db`, with `local` / `stylepass`. Skip whatever is already listening (`ss -ltn | command grep -E ':(8491|5291) '`); otherwise start each with the Bash tool's `run_in_background`:

```bash
exec target/debug/taiyaku serve --addr 127.0.0.1:8491 --db target/style-pass/lib/library.db --cache target/style-pass/cache --insecure-cookies --log-requests target/style-pass/two/requests.log
```

```bash
TAIYAKU_SERVE=http://127.0.0.1:8491 exec web/node_modules/.bin/vite web --host 127.0.0.1 --port 5291 --strictPort
```

If `serve` was already running without `--log-requests`, stop it by PID (`ss -ltnp 'sport = :8491'`) and start it as above: Step 6 reads that log.

- [ ] **Step 2: Give the continue strip something to show**

The scratch copy holds no reading positions, so the strip is empty. With Playwright at 1440×900, sign in at `http://127.0.0.1:5291/` as `local` / `stylepass`, then:

```js
async () => {
  const all = await (await fetch('/api/library/series')).json()
  const picks = all.series.slice(0, 4)
  const slugs = []
  for (const s of picks) {
    const one = await (await fetch(`/api/library/series/${s.id}`)).json()
    slugs.push(one.works[0].slug)
  }
  return slugs
}
```

For each slug: navigate to `/read/<slug>`, wait for the page to draw, dispatch `ArrowLeft` three times on `window` (`window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true }))`, 300 ms apart), and wait a second. Then evaluate `(await (await fetch('/api/library/home')).json()).continuing.length`. Expected: 4. If a volume turned no page (a left-to-right edition), repeat it with `ArrowRight`. These writes land in the scratch copy only.

- [ ] **Step 3: The browser pass, the whole matrix**

At 390×844 and at 1440×900, in Paper and in Ink (chosen with the sidebar's — or at 390 the drawer's — theme control), screenshot each of these to `target/style-pass/two/pass/<screen>-<theme>-<width>.png`:

1. `home` — `/`, the continue strip in view.
2. `series-grid` — `/series/6`.
3. `series-table` — `/series/6` after choosing `table` for `layout` in the library settings; choose `grid` again afterwards.
4. `search` — `/search?q=berserk`.
5. `drawer` — `/` with ☰ pressed (390 only).
6. `login` — after signing out from `/account` (390 only; set `localStorage['taiyaku.prefs.theme']` to `{"choice":"ink"}` and reload for Ink). Compare it with `target/style-pass/review-sheet-390.png`, ①'s login screens: the wordmark should sit exactly where it did.
7. `proposals` — `/proposals`. *(Added after Task 4's review: `Proposals.tsx` draws its state and kind filters in the same `.chips` row, so ② turns them from pills into underlined tabs on a screen that is ⑤'s. Record how both rows read, and anything a count inside a tab does to it.)*

8. `proposals-empty` — `/proposals` with a filter that shows nothing (Dismissed, or a kind with no open proposals), for the shared empty state (390 only).
9. `config` and `account` — `/config/roots` and `/account` (390 only): below 900 px the document scrolls, so a ⑤ screen that overflows sideways now carries the sticky bar off to the left with it. Record whether either does.

That is 32 screenshots: five screens at both widths in both themes, and the drawer, the login screen, the empty inbox, configuration and account at 390 in both. *(Items 8 and 9 added after the whole-branch review.)* Read every one. Record what each shows, and anything unreadable, clipped, still drawn in a browser-default grey, or scrolling sideways. Do not record a screen as seen unless its file was read.

- [ ] **Step 4: Measure what the screenshots cannot say**

At 390×844, on `home`, `series-grid`, `series-table` and `search`, evaluate:

```js
() => document.documentElement.scrollWidth - document.documentElement.clientWidth
```

Expected: `0` on each. On `series-table`:

```js
() => {
  const row = document.querySelector('.editions tbody tr')
  const cells = [...row.querySelectorAll('td')]
  return {
    volume: getComputedStyle(cells[0]).fontFamily,
    pages: getComputedStyle(row.querySelector('td.pages')).fontFamily,
    missing: (() => {
      const m = document.querySelector('.editions td.missing')
      return m && { color: getComputedStyle(m).color, opacity: getComputedStyle(m).opacity }
    })(),
  }
}
```

Expected: `volume` begins `"Zen Kaku Gothic New"`, `pages` begins `"taiyaku figures"`, and `missing` — if this series has a missing cell; otherwise note that it has none — has opacity `1` and colour `rgb(107, 97, 81)` in Paper, `rgb(166, 155, 135)` in Ink. On `drawer`:

```js
() => {
  const legend = document.querySelector('.theme-control legend')
  return {
    rootOverflow: getComputedStyle(document.documentElement).overflowY,
    mainInert: document.querySelector('main').inert,
    legendRule: getComputedStyle(legend, '::after').width,
  }
}
```

Expected: `rootOverflow` `hidden`, `mainInert` `true`, and `legendRule` wider than `0px` — the Theme heading's hairline reaches the column's edge like Roots' and Library's. Record every figure.

- [ ] **Step 5: Count the tests**

```bash
pnpm -C web exec vitest run --project node --reporter=default 2>&1 | command grep -E '^\s+Tests '
pnpm -C web exec vitest run --project jsdom --reporter=default 2>&1 | command grep -E '^\s+Tests '
pnpm -C web exec vitest run --project browser --reporter=default 2>&1 | command grep -E '^\s+Tests '
```

Expected: `node` 1,192 + 3 (sidebar headings) + 5 (missing cell and figures) = **1,200**; `jsdom` 984 + 7 (Shell) + 1 (Sidebar) + 1 (BrowseSettings) + 4 (Library) + 2 (Series) = **999**; `browser` 28 + 4 + 1 + 3 = **36**. If a run disagrees, the run is right: write its figure and find the difference before going on.

- [ ] **Step 6: The iPhone, over the tailnet**

Expose vite to the tailnet only (not Funnel):

```bash
tailscale serve --bg --https=443 http://127.0.0.1:5291
```

Stop vite by PID and start it again, with the Bash tool's `run_in_background`, allowing the tailnet name:

```bash
__VITE_ADDITIONAL_SERVER_ALLOWED_HOSTS=davey-endeavor.walrus-census.ts.net TAIYAKU_SERVE=http://127.0.0.1:8491 exec web/node_modules/.bin/vite web --host 127.0.0.1 --port 5291 --strictPort
```

Confirm from this machine first: `curl -s -o /dev/null -w '%{http_code}\n' https://davey-endeavor.walrus-census.ts.net/` prints `200`. If it prints `403` with vite's "Blocked request" text, the variable did not take: add `allowedHosts: ['davey-endeavor.walrus-census.ts.net']` to `server` in `web/vite.config.ts` for the pass only, and remove it before committing.

Ask the user to open `https://davey-endeavor.walrus-census.ts.net/` on their iPhone and sign in as `local` / `stylepass`. If sign-in answers "this request's Origin does not match this server", restart `serve` with `--trusted-proxy 127.0.0.1/32` added, so the `X-Forwarded-Host` vite passes on is believed, and ask again.

Then ask the user, one at a time, and record each answer:

1. Does ☰ open the drawer, and does each of a tap on the dimmed page, following a link in it, and nothing else close it?
2. With the drawer open, does dragging the dimmed page scroll the library behind it?
3. On the library, does Safari's toolbar shrink as you scroll down, and does a tap on the status bar scroll back to the top?
4. Are the drawer's links, the chips and a series tile's ↓ and `i` easy to hit with a thumb?
5. On a series in the table layout, does the table scroll sideways inside its box while the page stays put?
6. With VoiceOver on (triple-click the side button if it is set as the shortcut), open the drawer: is the Close button announced and does it close the drawer, and does VoiceOver's escape gesture (a two-finger Z) close it too? *(Added after the whole-branch review found the modal had no close control inside it; the Close button came from that.)*

Read the user-agent of the iPhone's requests before believing any answer:

```bash
command grep -o '"user_agent":"[^"]*iPhone[^"]*"' target/style-pass/two/requests.log | sort | uniq -c
```

Expected: at least one line naming `iPhone` and `Safari`. If the log carries no user-agent field, read it with `command grep -m3 iPhone target/style-pass/two/requests.log` and record what it does carry. Afterwards:

```bash
tailscale serve --https=443 off
tailscale serve status
```

Expected: `No serve config`. Restore vite to Step 1's command.

- [ ] **Step 7: Write it down**

Append to `docs/notes.md` a section headed `## Style pass ②: the shell and the library — <today's date>`, in this file's usual style (what was measured, how, the figures), holding:

1. The planning trial: the narrow layout added and reverted, 27 of 28 browser tests passing with Proposals' "reflows at 700px" failing at `expected 217.5625 to be greater than 217.5625`, and all 8 of the two files' cases passing at 460 px; and the widths at 460 px as the whole-branch review re-measured them — `.what` 323.3125 px (the same as at 700 px on `main`; Task 2's 326.625 did not reproduce in four runs), `.proposal-detail` 426 px and the form and pattern 407 px.
2. Step 4's figures: sideways scroll per screen, the two font families, the missing cell's colour and opacity, the drawer's root overflow, `inert` and the legend's hairline.
3. Step 3's observations, screen by screen and theme by theme, including the login comparison, and what is left for ③, ④ and ⑤.
4. Step 6's answers, each with the user-agent it was read against, and whether `overflow: hidden` on the root held on iOS.
5. The biome probes from planning: `aria-modal` refused beside a conditional `role` and passing as a spread; an `aria-hidden` SVG passing; the 8-digit `light-dark()` pair, `:root:has()` and `-webkit-line-clamp` passing.

In `docs/status.md`:

1. In `Six crates, 2227 tests, plus 2,176 in \`web/\`'s TypeScript — 1,192 in the \`node\`` and the two lines after it, replace `2,176`, `1,192`, `984` and `twenty-eight` with Step 5's total of node and jsdom, its `node` figure, its `jsdom` figure and its `browser` figure in words.
2. Add this row after the `| O131 |` row, as one line:

```markdown
| O132 | [ ] | **The continue strip shows no reading progress.** Each card draws a volume's cover, title and number (style pass ②), not where the reader is in it: the mockup the user chose on 2026-09-17 drew "Vol. 1 · page 8" and a bar, but `progress` holds a `slot` per edition, and nothing turns a slot into a page for `/api/library/home`. **The user wants both, chosen by a setting** (2026-09-18): the browse module's `continueStrip` checkbox becoming *Off / Covers / Progress*. What it needs: the home route sending each continuing work's page and page count in the edition it opens — resolved from the slot through that edition's spread flags and its work's reading direction, as the reader resolves it — and the card drawing them | `browse::home`, `library_api.rs`'s `/api/library/home`, `Library.tsx`'s `ContinueCard`, `browseModule.ts`'s `continueStrip` | the style pass's ② design, 2026-09-18 |
```

3. Replace `One hundred and thirty-one, each found by` with `One hundred and thirty-two, each found by`; `— leaving forty-six.**` with `— leaving forty-seven.**`; and in the `What remains:` line, `the forty-six open items` with `the forty-seven open items`. The "twenty-two of them found by a whole-branch review" after it does not move: O132 came from a design.
4. Run the sweep and check it agrees with itself: `command grep -c '^| O[0-9]' docs/status.md` prints 132, `command grep -c '^| O[0-9]* | \[x\]' docs/status.md` prints 85, and the O132 row has exactly six `|` characters outside its backticks.

In `docs/superpowers/specs/2026-09-17-style-pass-design.md` — prose wrapped at 80 columns, so match each phrase across line breaks and re-wrap what you write:

1. §7.1's `end` sentence was corrected when the fix round was decided; check it still reads true. Then replace the sentence beginning `The drawer's scroll lock is then` and ending `(§7.3) confirms it.` with what Step 6 measured: that `overflow: hidden` on the root while the drawer is open held, or did not, when dragged on the user's iPhone, naming the iOS version from the user-agent.
2. Anything else Steps 3, 4 or 6 contradicted in §7: correct it in place and say it was measured.

- [ ] **Step 8: Run the gate**

Run with the Bash tool's `run_in_background`:

```bash
env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh > target/style-pass/two/gate.log 2>&1; echo "exit $?"
```

When it finishes, read the end of `target/style-pass/two/gate.log`. Expected: `exit 0` and an `all green` line; quote that line exactly in the PR body. A failure is read in full and fixed, never summarised.

- [ ] **Step 9: Commit**

```bash
git add docs/notes.md docs/status.md docs/superpowers/specs/2026-09-17-style-pass-design.md
git commit -m "Style pass 2: measured in the browser and on the phone, recorded, O132 filed, gate green"
```

---

## After the tasks

Not tasks in this plan, but required before the PR opens (spec §5, "Every PR, before it is opened"):

1. A whole-branch review of `main..style-2-shell-library`, then a review of each fix round until one finds nothing.
2. Every hand mutation above re-run by whoever dispatched the tasks, control run first.
3. The pre-merge claim check over the PR body, `docs/status.md`, `docs/notes.md` and the spec.
4. The PR through the Gitea MCP, never `gh`. The body is the squash commit message, is not hard-wrapped, and carries no attribution.
