import { act } from 'react'
import { describe, expect, it } from 'vitest'
import { page, userEvent } from 'vitest/browser'
// **Required.** `--strip-unit` and friends live in `styles/reader.css`, which
// reaches the app through `index.css`. Without this import,
// `getPropertyValue('--strip-unit')` below returns `''`, `Number.parseFloat`
// turns that into `NaN`, and every arithmetic assertion in this file fails on
// `NaN` regardless of the guard a few lines down — that guard is not what
// protects against losing this import outright. What it protects against is
// a *valid but degenerate* `--strip-unit`, such as `0px`: that resolves to a
// real number, so every tile collapses to its border width and every
// `calc()` below would agree with itself at zero. 6 of the 7
// `*.browser.test.tsx` files in this directory import it for the same
// reason; `PageView.browser.test.tsx` does not.
import './index.css'
import { Filmstrip, type FilmstripProps } from './Filmstrip'
import type { Group } from './pairing'
import type { Position } from './resolver'
import { mount, rerender } from './testing/mount'
import {
  requestedTileEdge,
  STRIP_BREAKPOINT_PX,
  STRIP_TILE_PX,
} from './tileSize'

/**
 * What jsdom cannot answer: whether a windowed rail is the same *shape* as an
 * unwindowed one. Every assertion here is a measured box.
 *
 * The invariant is deliberately "the windowed rail is as wide as its tiles'
 * nominal widths say it should be", not "as wide as it was before O140" —
 * giving a tile a width moved that on its own, before anything was windowed.
 *
 * Nothing here loads a thumbnail: `thumbUrl` points at `/api/...`, which no
 * server answers in this environment, so every tile sits at exactly its
 * nominal floor. That is what makes the closed-form arithmetic below exact.
 * It is also why these tests cannot speak to a rail whose images have loaded,
 * where a tile may exceed its floor — the spacers only ever cover tiles that
 * have not rendered, which are the ones still at nominal.
 */
const slots: Position[] = Array.from({ length: 120 }, (_, i) => ({ page: i }))
const groups: Group[] = slots.map((_, i) => ({ slots: [i] }))

function props(overrides: Partial<FilmstripProps> = {}): FilmstripProps {
  return {
    reading: 'ltr',
    work: 'w',
    edition: 'e',
    slots,
    groups,
    currentIndex: 60,
    onNavigate: () => {},
    ...overrides,
  }
}

/**
 * The rail needs the reader's custom properties, which live on `.reader`.
 *
 * Its own function so that a `rerender` can hand React the tree `inReader`
 * mounted, with the same two wrappers around the strip — which is what keeps
 * the strip the same instance across it.
 */
function reader(ui: React.ReactNode) {
  return (
    <div className="reader" style={{ position: 'fixed', inset: 0 }}>
      <div style={{ height: 'var(--strip-height)' }}>{ui}</div>
    </div>
  )
}

function inReader(ui: React.ReactNode) {
  return mount(reader(ui))
}

const railOf = (host: HTMLElement) =>
  host.querySelector('.reader > div > div') as HTMLElement

/** The rail's geometry, read from the stylesheet rather than hardcoded. */
function geometry(rail: HTMLElement) {
  const cs = getComputedStyle(rail)
  const num = (v: string) => Number.parseFloat(v)
  const unit = num(cs.getPropertyValue('--strip-unit'))
  const border = num(cs.getPropertyValue('--strip-tile-border'))
  return {
    unit,
    border,
    gap: num(cs.columnGap),
    padding: num(cs.paddingLeft) + num(cs.paddingRight),
    nominal: unit + 2 * border,
  }
}

describe('a windowed rail has the shape of an unwindowed one', () => {
  for (const reading of ['ltr', 'rtl'] as const) {
    it(`is as wide as every tile's nominal width says, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader(<Filmstrip {...props({ reading })} />)
      const rail = railOf(host)
      const g = geometry(rail)

      // THE GUARD. Losing the `./index.css` import outright already fails
      // loudly on its own — `NaN` from `Number.parseFloat('')`, propagated
      // into every assertion below (see the import's own comment). What
      // these two lines actually catch is a *valid* `--strip-unit`, such as
      // `0px`: that resolves to a real number, collapses every tile, and
      // would otherwise let every `calc()` below agree with itself at zero.
      // `nominal > 50` rules that out, and names the cause up front instead
      // of leaving a wall of `NaN` assertions to puzzle out.
      expect(g.unit).toBeGreaterThan(0)
      expect(g.nominal).toBeGreaterThan(50)
      expect(
        rail.querySelector('button')?.getBoundingClientRect().width,
      ).toBeCloseTo(g.nominal, 0)

      // 120 one-unit tiles, whatever fraction of them is actually rendered.
      expect(rail.scrollWidth).toBeCloseTo(
        120 * g.nominal + 119 * g.gap + g.padding,
        0,
      )
    })
  }

  it('renders far fewer tiles than the rail holds', async () => {
    await page.viewport(1440, 900)
    const host = inReader(<Filmstrip {...props()} />)
    expect(host.querySelectorAll('button').length).toBeLessThan(40)
    expect(host.querySelectorAll('button').length).toBeGreaterThan(0)
  })

  // `scrollWidth` above is order-invariant — it is the same number whether
  // the two spacers are the right way round or swapped, since a swap
  // exchanges their widths rather than changing either one's magnitude. Only
  // a *positional* assertion can catch a swap, and only if it is run in both
  // directions: the review that produced this loop found the original,
  // `ltr`-only version of this test caught a swap (48 vs 47 tiles = 101px)
  // while an untested `rtl` case would not have — the same shape of miss
  // recorded as O65 elsewhere in this project, an `rtl` fixture passing a
  // mutation `ltr` caught.
  for (const reading of ['ltr', 'rtl'] as const) {
    it(`puts the current tile where an unwindowed rail would put it, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader(
        <Filmstrip {...props({ reading, currentIndex: 60 })} />,
      )
      const rail = railOf(host)
      const g = geometry(rail)
      const current = host.querySelector('[aria-current="true"]') as HTMLElement
      expect(current).not.toBeNull()

      // Recovering the tile's content-space left edge — where it would sit
      // if the rail were scrolled all the way back to its start — from its
      // viewport-space rect needs `scrollLeft` added back in. But Chromium
      // does not report `scrollLeft` on the same 0..max scale in both
      // directions here: under `row` it is the ordinary range (0 at the
      // start), while under `row-reverse` (this component's `rtl` layout,
      // even though the box's own `direction` stays `ltr` — only the
      // children's `flexDirection` reverses) content overflows to the box's
      // *left*, and Chromium reports `scrollLeft` as negative,
      // `-(scrollWidth - clientWidth)` at the start up to `0`. Folding both
      // onto the same 0..max scale up front is what lets one `offset`
      // formula serve both directions — measured empirically (a raw,
      // un-normalised `rtl` reading came out negative and off by exactly
      // `scrollWidth - clientWidth`).
      const scrollLeft =
        rail.scrollLeft >= 0
          ? rail.scrollLeft
          : rail.scrollLeft + (rail.scrollWidth - rail.clientWidth)
      const offset =
        current.getBoundingClientRect().left -
        rail.getBoundingClientRect().left +
        scrollLeft

      // Under `row`, main-start is the left edge, so tile 60 is 60 tiles
      // and 60 gaps in from it. Under `row-reverse` (`rtl`), main-start
      // moves to the RIGHT instead: tile 0 sits against the right padding
      // edge, and tile *i* is *i* tiles-and-gaps further left of that, i.e.
      // tile 60 is at the MIRRORED position, `count - 1 - 60`
      // tiles-and-gaps in from the left. Reusing the `ltr` expected value
      // here (measuring the same left edge but expecting the un-mirrored
      // position) is exactly the mistake that would leave `rtl` unable to
      // tell a correct rail from one whose two spacers were swapped.
      const paddingLeft = Number.parseFloat(getComputedStyle(rail).paddingLeft)
      const expected =
        reading === 'rtl'
          ? paddingLeft + (groups.length - 1 - 60) * (g.nominal + g.gap)
          : paddingLeft + 60 * (g.nominal + g.gap)
      expect(offset).toBeCloseTo(expected, 0)
    })
  }
})

/**
 * O134: what the strip asks the server for follows the stylesheet, at both
 * of its breakpoints, on the real `matchMedia` and the real
 * `devicePixelRatio`.
 *
 * **One `it()` per viewport, not one `it()` looping over both.**
 * `testing/mount.ts` keeps a single module-level `root`/`host` pair and tears
 * it down in `afterEach`; a second `mount()` call inside one test would
 * overwrite those before the first is ever unmounted, leaking the first
 * host into `document.body`. `describe('a windowed rail has the shape of an
 * unwindowed one')` above already follows this shape for `ltr`/`rtl` — a
 * `for` loop outside `it()`, generating one test per case — and this follows
 * it too.
 *
 * **Mounted fresh at each viewport**, because the rung is read once at mount
 * by design (`Filmstrip.tsx`'s `tileEdge`) — resizing a mounted strip is
 * deliberately not expected to move it.
 *
 * **What this cannot see at `devicePixelRatio` 1**: both branches round up
 * to the same 160, so the two viewports agreeing proves the wiring, not the
 * ladder. The ladder's own discrimination is `tileSize.test.ts`'s table,
 * where the density is an argument. What is checked here and nowhere else is
 * that the height fed to it is the one the stylesheet actually drew.
 */
describe("the strip's rung follows the stylesheet's drawn tile height", () => {
  for (const [width, height] of [
    [1440, 900],
    [390, 844],
  ] as const) {
    it(`asks for the rung its own drawn tile height calls for, ${width}x${height}`, async () => {
      await page.viewport(width, height)
      const host = inReader(<Filmstrip {...props()} />)
      const rail = railOf(host)
      const drawn = Number.parseFloat(
        getComputedStyle(rail).getPropertyValue('--strip-tile'),
      )
      expect(drawn).toBe(
        width < STRIP_BREAKPOINT_PX ? STRIP_TILE_PX.narrow : STRIP_TILE_PX.wide,
      )

      const expected = requestedTileEdge(drawn, window.devicePixelRatio)
      const images = [...rail.querySelectorAll('img')]
      expect(images.length).toBeGreaterThan(0)
      for (const img of images) {
        expect(img.getAttribute('src')).toContain(`/thumb?w=${expected}`)
      }
    })
  }
})

/**
 * O146: a page turn that lands after the reader has flicked the rail away
 * from the current tile, in real layout.
 *
 * `Filmstrip.test.tsx` holds this in jsdom ("recentres on the new current
 * tile after a page turn that lands outside the window the reader had flicked
 * to"), and what it can hold there is which element `scrollIntoView` was last
 * called on. jsdom has no layout, so that file swaps `scrollIntoView` for a
 * recorder, stubs the rail's scroll geometry and its box, and dispatches the
 * scroll event by hand. Whether the rail then *shows* the tile is not a
 * question it can ask — and the defect was that it did not: the window
 * rebuilt around the new current tile while the rail stayed scrolled to
 * wherever the flick had left it (`Filmstrip.tsx`'s centring effect, C1).
 *
 * Nothing is stubbed here. The flick is a write to the real `scrollLeft`,
 * the scroll event is the browser's own, the centring is the real
 * `scrollIntoView`, and the two things asserted at the end are the rail's
 * real scroll position and the tile's real box.
 *
 * **The turn is a `rerender`, not a second `mount`.** The same instance has
 * to see `currentIndex` change: a fresh one has no flicked window to be stale
 * against.
 */
describe('a page turn after the reader has flicked the rail', () => {
  /** Which tiles the window holds, as `groups` indices. */
  const rendered = (rail: HTMLElement) =>
    [...rail.querySelectorAll('button')].map((b) => Number(b.dataset.group))
  const currentOf = (rail: HTMLElement) =>
    rail.querySelector<HTMLElement>('[aria-current="true"]')

  // One `it()` per direction, as elsewhere in this file. Both directions,
  // because the rail's scroll range has the opposite sign under `rtl`, and
  // because the new current tile is off a different end of the rail in each
  // when the rail fails to follow it — so each of the two bounds asserted at
  // the end is the one that fails in one direction and passes in the other.
  for (const reading of ['ltr', 'rtl'] as const) {
    it(`brings the new current tile on screen, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader(
        <Filmstrip {...props({ reading, currentIndex: 60 })} />,
      )
      const rail = railOf(host)

      // Where the mount leaves things: the rail scrolled out to tile 60 —
      // the flick below is a share of that distance, so it must not be 0 —
      // and a window around it that holds the page the turn will go to and
      // does not reach the first page, which is where the flick is headed.
      // Those two are asserted here so that their opposites, after the
      // flick, are changes the flick made.
      const centred = rail.scrollLeft
      expect(centred).not.toBe(0)
      expect(currentOf(rail)?.dataset.group).toBe('60')
      expect(rendered(rail)).toContain(61)
      expect(rendered(rail)).not.toContain(0)

      // THE FLICK: most of the way back toward the first page, to a tenth of
      // the distance the mount scrolled the rail out. A share of the
      // browser's own reading rather than a pixel number, because that
      // reading is negative under `rtl` (the positional test above has the
      // scale) and this way the sign is the browser's in both directions.
      //
      // Inside an `act` held open until the frame after the scroll event:
      // the rail's scroll handler is throttled to an animation frame, and
      // this is what puts the state it sets there inside the `act`. The
      // listener here is added after the rail's own and the frame here is
      // requested after the rail's, so the rail's runs first both times.
      await act(async () => {
        rail.scrollLeft = Math.round(centred / 10)
        await new Promise((resolve) =>
          rail.addEventListener('scroll', resolve, { once: true }),
        )
        await new Promise((resolve) => requestAnimationFrame(resolve))
      })

      // THE STATE THIS TEST NAMES, or everything after it proves nothing: a
      // turn to a tile the window already holds finds that tile on the first
      // attempt. So the old current tile has to
      // have left the DOM, the window has to have followed the flick to the
      // first page, and the page the turn is about to go to has to be
      // outside it. Polled rather than read once: the window moves on a
      // frame after the scroll, and what has to hold before the turn is this
      // condition, not that some number of frames has passed.
      await expect.poll(() => currentOf(rail)).toBeNull()
      const flicked = rail.scrollLeft
      expect(flicked).not.toBe(centred)
      expect(rendered(rail)).toContain(0)
      expect(rendered(rail)).not.toContain(61)

      // THE TURN: one page forward, on the same instance.
      rerender(reader(<Filmstrip {...props({ reading, currentIndex: 61 })} />))
      expect(railOf(host)).toBe(rail)

      const current = currentOf(rail)
      expect(current?.dataset.group).toBe('61')
      const tile = (current as HTMLElement).getBoundingClientRect()
      const visible = rail.getBoundingClientRect()
      // A box with no width lies inside any box that holds its edge.
      expect(tile.width).toBeGreaterThan(0)

      // The rail moved, and the tile the reader is now on lies inside the
      // rail's own box along the axis it scrolls on — existing is not enough,
      // since the tile existed under the defect too, off screen. Soft, so
      // that one failing run reports every reading that fails.
      expect.soft(rail.scrollLeft).not.toBe(flicked)
      expect.soft(tile.left).toBeGreaterThanOrEqual(visible.left)
      expect.soft(tile.right).toBeLessThanOrEqual(visible.right)
    })
  }
})

/**
 * O138: the rail draws no scrollbar.
 *
 * What is read here is the cascade: that the rail wears the class and the
 * rule reaches it. It is not the scrollbar. O145 found that Playwright's
 * defaults launch Chromium with `--hide-scrollbars`, so in this project the
 * rail's `clientHeight` equals its `offsetHeight` with the rule or without
 * it. The scrollbar itself is read on the running app (`docs/notes.md`).
 */
describe("the rail's scrollbar", () => {
  it('is declared away on the rail itself', async () => {
    await page.viewport(1440, 900)
    const host = inReader(<Filmstrip {...props()} />)
    const rail = railOf(host)
    // The fixture overflows, which is the one state a scrollbar is drawn in.
    expect(rail.scrollWidth).toBeGreaterThan(rail.clientWidth)
    expect(getComputedStyle(rail).getPropertyValue('scrollbar-width')).toBe(
      'none',
    )
  })
})

/**
 * O138: the wheel moves the rail, which draws no scrollbar.
 *
 * `tiles.test.ts` holds which wheels the rail takes and how far each moves
 * it, and `Filmstrip.test.tsx` that the listener applies the answer. What
 * neither can ask is which way the rail *goes*: jsdom has no layout, so a
 * `scrollLeft` there is a number and not a direction. Here the wheel is a
 * real one (`userEvent.wheel` hovers the rail and turns the mouse's wheel),
 * and what is read is where two tiles stand afterwards.
 *
 * **Both directions, every case.** The rail's scroll range has the opposite
 * sign under `rtl`, so a handler that forgot the direction is right in one
 * and wrong in the other.
 */
describe('the wheel on the rail', () => {
  /** How far a tile's centre is from the rail's, along the axis it scrolls on. */
  const fromCentre = (rail: HTMLElement, group: number) => {
    const tile = rail.querySelector<HTMLElement>(`[data-group="${group}"]`)
    if (!tile) throw new Error(`tile ${group} is not rendered`)
    const t = tile.getBoundingClientRect()
    const r = rail.getBoundingClientRect()
    return Math.abs(t.left + t.width / 2 - (r.left + r.width / 2))
  }

  /**
   * The rail, with focus on its current tile.
   *
   * Vitest runs this in an iframe, and a real input sent before anything in
   * it has focus can go to the frame around it: `App.browser.test.tsx`'s
   * `readerFocused` has the case. The current tile is the rail's one stop.
   */
  function focusedRail(overrides: Partial<FilmstripProps>) {
    const host = inReader(<Filmstrip {...props(overrides)} />)
    const rail = railOf(host)
    act(() =>
      rail
        .querySelector<HTMLElement>('[aria-current="true"]')
        ?.focus({ preventScroll: true }),
    )
    return rail
  }

  /**
   * The next wheel event to reach the window. A listener here runs after the
   * rail's own, so `defaultPrevented` is what the rail decided.
   */
  const nextWheel = () =>
    new Promise<WheelEvent>((resolve) =>
      window.addEventListener('wheel', resolve, { once: true }),
    )

  /** The `scroll` the write fires, and the frame the rail's handler takes after it. */
  const twoFrames = async () => {
    await new Promise((resolve) => requestAnimationFrame(resolve))
    await new Promise((resolve) => requestAnimationFrame(resolve))
  }

  for (const reading of ['ltr', 'rtl'] as const) {
    it(`moves toward later pages on a notch down, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const rail = focusedRail({ reading, currentIndex: 60 })
      const before = rail.scrollLeft
      const later = fromCentre(rail, 70)
      const earlier = fromCentre(rail, 50)

      const seen = nextWheel()
      await act(async () => {
        await userEvent.wheel(rail, { delta: { y: 100 } })
        await seen
        await twoFrames()
      })

      expect((await seen).defaultPrevented).toBe(true)
      expect(Math.abs(rail.scrollLeft - before)).toBeCloseTo(100, 0)
      // THE DIRECTION, asked of the tiles and not of `scrollLeft`'s sign: a
      // page ten on is 100 px nearer the middle of the rail, and a page ten
      // back 100 px further from it.
      expect(fromCentre(rail, 70)).toBeCloseTo(later - 100, 0)
      expect(fromCentre(rail, 50)).toBeCloseTo(earlier + 100, 0)
    })
  }

  for (const reading of ['ltr', 'rtl'] as const) {
    it(`takes a notch at the rail's end and does not pass it on, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const rail = focusedRail({ reading, currentIndex: 119 })

      // THE STATE THIS TEST NAMES: the rail stands at its far end, so a notch
      // toward later pages has nowhere to go.
      const span = rail.scrollWidth - rail.clientWidth
      expect(span).toBeGreaterThan(0)
      expect(Math.abs(rail.scrollLeft)).toBeCloseTo(span, 0)
      const before = rail.scrollLeft

      const seen = nextWheel()
      await act(async () => {
        await userEvent.wheel(rail, { delta: { y: 100 } })
        await seen
        await twoFrames()
      })

      expect(rail.scrollLeft).toBe(before)
      expect((await seen).defaultPrevented).toBe(true)
    })
  }

  // Made events, not real ones: a real wheel with Ctrl held zooms the
  // browser's page, for this test and for every one after it. A made event
  // has no default action, so nothing moves unless the rail's listener
  // moves it.
  it('leaves Ctrl, Shift and a sideways delta alone', async () => {
    await page.viewport(1440, 900)
    const rail = focusedRail({ currentIndex: 60 })
    const before = rail.scrollLeft
    /** `false` once a listener has cancelled the event. */
    const send = (init: WheelEventInit) =>
      rail.dispatchEvent(
        new WheelEvent('wheel', { bubbles: true, cancelable: true, ...init }),
      )

    expect(send({ deltaY: 100, ctrlKey: true })).toBe(true)
    expect(send({ deltaY: 100, shiftKey: true })).toBe(true)
    expect(send({ deltaX: 100 })).toBe(true)
    expect(rail.scrollLeft).toBe(before)

    // THE CONTROL: the same event with no key held is taken, so the three
    // above reached the listener and it was the listener that declined them.
    let passed = true
    await act(async () => {
      passed = send({ deltaY: 100 })
      await twoFrames()
    })
    expect(passed).toBe(false)
    expect(rail.scrollLeft).not.toBe(before)
  })

  // The listener is on the rail itself: a made event is right here, since a
  // real wheel outside the rail would scroll the page. The target is the rail's
  // parent, the `inReader` wrapper that gives the strip its height, the
  // nearest element outside the rail.
  it('leaves a wheel outside the rail alone, and takes the same wheel on it', async () => {
    await page.viewport(1440, 900)
    const rail = focusedRail({ currentIndex: 60 })
    const outside = rail.parentElement as HTMLElement
    const before = rail.scrollLeft
    const send = (target: HTMLElement) =>
      target.dispatchEvent(
        new WheelEvent('wheel', {
          bubbles: true,
          cancelable: true,
          deltaY: 100,
        }),
      )

    expect(send(outside)).toBe(true)
    expect(rail.scrollLeft).toBe(before)

    // THE CONTROL: the same wheel, at the rail, is taken.
    let passed = true
    await act(async () => {
      passed = send(rail)
      await twoFrames()
    })
    expect(passed).toBe(false)
    expect(rail.scrollLeft).not.toBe(before)
  })

  // `leaves Ctrl, Shift and a sideways delta alone` shows the listener
  // declining a sideways delta, not that anything then moves the rail. This one is a real sideways wheel: the
  // rail's listener leaves it alone, and the browser, scrolling a box that is
  // still `overflow-x: auto`, moves the rail with it. Polled, since that scroll
  // is the browser's own and lands on its schedule rather than the test's.
  it('leaves a real sideways wheel to the browser, which moves the rail', async () => {
    await page.viewport(1440, 900)
    const rail = focusedRail({ currentIndex: 60 })
    const before = rail.scrollLeft
    const seen = nextWheel()
    await act(async () => {
      await userEvent.wheel(rail, { delta: { x: 100 } })
      await seen
      await twoFrames()
    })
    expect((await seen).defaultPrevented).toBe(false)
    await expect.poll(() => rail.scrollLeft).not.toBe(before)
  })
})
