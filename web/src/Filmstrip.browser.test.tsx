import { act } from 'react'
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  type MockInstance,
  onTestFinished,
  vi,
} from 'vitest'
import { cdp, page, userEvent } from 'vitest/browser'
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
import { mouse } from './testing/mouse'
import {
  requestedTileEdge,
  STRIP_BREAKPOINT_PX,
  STRIP_TILE_PX,
} from './tileSize'
import { type GlideSample, releaseSpeed } from './tiles'

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

/**
 * O216: a mouse drags the rail.
 *
 * `Filmstrip.test.tsx` holds what the rail's listeners do with each event, on
 * events it makes. What it cannot ask is what a real mouse sends and does:
 * which element hears a move once the pointer has left the strip, where the
 * click after a release lands, where a press puts focus, and whether the
 * browser sets out on a drag of its own. Here the mouse is a real one, and
 * what is read is where a tile stands afterwards, which page was turned to
 * and what has focus.
 *
 * **Both directions, every case.** A tile that has followed the pointer has
 * moved the way the pointer moved, whichever way the rail runs, while the
 * rail's `scrollLeft` is negative in one direction and not in the other.
 */
describe('a mouse dragging the rail', () => {
  const { moveTo, press, release, along } = mouse

  // A case that fails with the page selected would leave it selected for the
  // next. `testing/mouse.ts` lets the button go for the same reason.
  afterEach(() => {
    document.getSelection()?.removeAllRanges()
  })

  /**
   * The strip, with focus on its current tile, where a reader who has come to
   * the strip by `Tab` has it, and the pages its tiles were clicked through
   * to. A case that is about where focus was before a press puts it there
   * itself.
   *
   * `inReader`'s tree, with a button under the strip: something outside the
   * rail that can hold focus, as a button of the bars does in the reader.
   *
   * With `glides` off unless a case asks for it. A real mouse is let go
   * within a frame of its last move, which is a pointer still moving, and the
   * rail would be carried on from where each of the drag's cases reads it.
   * The cases that let it be are the last block of this one.
   */
  function strip(reading: 'ltr' | 'rtl', glides = false) {
    const turned: number[] = []
    const host = mount(
      <div className="reader" style={{ position: 'fixed', inset: 0 }}>
        <div style={{ height: 'var(--strip-height)' }}>
          <Filmstrip
            {...props({
              reading,
              currentIndex: 60,
              onNavigate: (slot) => turned.push(slot),
              glides,
            })}
          />
        </div>
        <button type="button">elsewhere</button>
      </div>,
    )
    const rail = railOf(host)
    const current = rail.querySelector<HTMLElement>('[aria-current="true"]')
    const elsewhere = host.querySelector<HTMLElement>('.reader > button')
    if (!current || !elsewhere) throw new Error('the fixture is not as built')
    act(() => current.focus({ preventScroll: true }))
    // Where each click landed, heard on its way down to it, and which of them
    // came back up past the rail. The host is where React listens too.
    const landed: EventTarget[] = []
    const passed: EventTarget[] = []
    host.addEventListener('click', (e) => landed.push(e.target as Node), true)
    host.addEventListener('click', (e) => passed.push(e.target as Node))
    return { host, rail, turned, landed, passed, current, elsewhere }
  }

  const tileOf = (rail: HTMLElement, group: number) => {
    const tile = rail.querySelector<HTMLElement>(`[data-group="${group}"]`)
    if (!tile) throw new Error(`tile ${group} is not rendered`)
    return tile
  }
  const middleOf = (el: HTMLElement) => {
    const box = el.getBoundingClientRect()
    return {
      x: Math.round(box.left + box.width / 2),
      y: Math.round(box.top + box.height / 2),
    }
  }
  const leftOf = (el: HTMLElement) => el.getBoundingClientRect().left
  /**
   * One to one, to the pixel either side: the frame is drawn scaled, so the
   * pointer's place in it is not a whole number of pixels.
   */
  const expectMoved = (moved: number, by: number) => {
    expect(moved).toBeGreaterThanOrEqual(by - 1)
    expect(moved).toBeLessThanOrEqual(by + 1)
  }
  /** The `scroll` a drag's write fires, and the frame the rail's handler takes after it. */
  const twoFrames = async () => {
    await new Promise((resolve) => requestAnimationFrame(resolve))
    await new Promise((resolve) => requestAnimationFrame(resolve))
  }
  /**
   * `count` frames, each in an `act` of its own: a render that one frame
   * asks for is made before the next, as it is in the app, and the strip's
   * window keeps up with a rail that goes on moving.
   */
  const frames = async (count: number) => {
    for (let frame = 0; frame < count; frame++) {
      await act(async () => {
        await new Promise((resolve) => requestAnimationFrame(resolve))
      })
    }
  }
  /**
   * How much of the rail's own box a spacer lies across, in pixels: a
   * stretch of the strip with no tiles in it.
   */
  const bare = (rail: HTMLElement) => {
    const box = rail.getBoundingClientRect()
    let across = 0
    for (const spacer of rail.querySelectorAll(
      '[data-testid="strip-spacer"]',
    )) {
      const s = spacer.getBoundingClientRect()
      across += Math.max(
        0,
        Math.min(s.right, box.right) - Math.max(s.left, box.left),
      )
    }
    return across
  }
  /** The tile nearest the middle of the rail's box, as the rail stands. */
  const tileInTheMiddle = (rail: HTMLElement) => {
    const box = rail.getBoundingClientRect()
    const off = (tile: HTMLElement) => {
      const t = tile.getBoundingClientRect()
      return Math.abs(t.left + t.width / 2 - (box.left + box.width / 2))
    }
    return [...rail.querySelectorAll<HTMLElement>('button')].reduce(
      (nearest, tile) => (off(tile) < off(nearest) ? tile : nearest),
    )
  }
  /**
   * Which side of a page's tile the next page's stands on: `1` for the
   * right, `-1` for the left. Asked of two tiles' boxes as the strip mounts,
   * so no case decides for itself what `rtl` means.
   */
  const laterSide = (rail: HTMLElement) =>
    Math.sign(leftOf(tileOf(rail, 61)) - leftOf(tileOf(rail, 60)))

  for (const reading of ['ltr', 'rtl'] as const) {
    it(`follows the pointer one to one, both ways, and turns no page, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const { rail, turned, landed, passed } = strip(reading)
      const tile = tileOf(rail, 62)
      const { x, y } = middleOf(tile)
      const stood = leftOf(tile)
      const resting = getComputedStyle(rail).cursor
      expect(resting).not.toBe('grabbing')

      await act(async () => {
        await moveTo(x, y)
        await press()
        await along(x, y, 0, 120)
        await twoFrames()
      })
      // ON THE WAY, the button still down: the tile that was pressed has gone
      // where the pointer went.
      expectMoved(leftOf(tile) - stood, 120)
      expect(getComputedStyle(rail).cursor).toBe('grabbing')

      await act(async () => {
        await along(x, y, 120, -60)
        await release()
        await twoFrames()
      })
      // Back past where it was pressed, and it stays where it was let go.
      expectMoved(leftOf(tile) - stood, -60)
      expect(getComputedStyle(rail).cursor).toBe(resting)
      expect(turned).toEqual([])
      // THE CLICK the release made. The pointer was captured, so Chromium
      // sends it to the rail and not to the tile under the pointer; the rail
      // swallows it wherever it lands, and nothing above the rail hears it.
      expect(landed).toEqual([rail])
      expect(passed).toEqual([])

      // The next click is the reader's own, and turns the page.
      const next = middleOf(tileOf(rail, 61))
      await act(async () => {
        await moveTo(next.x, next.y)
        await press()
        await release()
        await twoFrames()
      })
      expect(turned).toEqual([61])
    })

    it(`keeps following a pointer that has left the strip, and is let go out there, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const { rail, turned, landed, passed } = strip(reading)
      const tile = tileOf(rail, 62)
      const { x, y } = middleOf(tile)
      const stood = leftOf(tile)
      const resting = getComputedStyle(rail).cursor
      const outside = rail.getBoundingClientRect().bottom + 200

      await act(async () => {
        await moveTo(x, y)
        await press()
        await along(x, y, 0, 60)
        await moveTo(x + 60, outside)
        await along(x, outside, 60, 150)
        await twoFrames()
      })
      // THE STATE THIS TEST NAMES: the pointer is not over the rail.
      expect(rail.contains(document.elementFromPoint(x + 150, outside))).toBe(
        false,
      )
      expectMoved(leftOf(tile) - stood, 150)

      await act(async () => {
        await along(x, outside, 150, 90)
        await release()
        await twoFrames()
        // The pointer comes back over the strip and along it, no button down.
        await moveTo(x + 90, y)
        await along(x, y, 90, 210)
        await twoFrames()
      })
      // It followed back to where it was let go, and has stayed there.
      expectMoved(leftOf(tile) - stood, 90)
      expect(getComputedStyle(rail).cursor).toBe(resting)
      expect(turned).toEqual([])
      expect(landed).toEqual([rail])
      expect(passed).toEqual([])
    })

    // A click is not the drag's to touch. Focus goes to the tile that was
    // clicked and stays there, as it does with no drag in the strip at all.
    it(`takes a press that goes nowhere, and one that travels less than the slop, as clicks, and leaves focus on the tile, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const { rail, turned, elsewhere } = strip(reading)
      const before = rail.scrollLeft
      const still = middleOf(tileOf(rail, 62))
      const short = middleOf(tileOf(rail, 58))
      act(() => elsewhere.focus())

      await act(async () => {
        await moveTo(still.x, still.y)
        await press()
        await release()
        await twoFrames()
      })
      expect(turned).toEqual([62])
      expect(document.activeElement).toBe(tileOf(rail, 62))

      act(() => elsewhere.focus())
      await act(async () => {
        await moveTo(short.x, short.y)
        await press()
        await moveTo(short.x + 3, short.y)
        await release()
        await twoFrames()
      })
      expect(turned).toEqual([62, 58])
      expect(document.activeElement).toBe(tileOf(rail, 58))
      expect(rail.scrollLeft).toBe(before)
      expect(getComputedStyle(rail).cursor).not.toBe('grabbing')
    })

    // The slop is along the rail. A hand that presses a tile and slips most
    // of the way down it has travelled far, and is clicking that tile.
    it(`takes a press carried across the rail, and hardly along it, as a click, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const { rail, turned } = strip(reading)
      const before = rail.scrollLeft
      const tile = tileOf(rail, 62)
      const box = tile.getBoundingClientRect()
      const x = Math.round(box.left + box.width / 2)
      const from = Math.round(box.top + 20)
      const to = Math.round(box.bottom - 20)
      // THE STATE THIS TEST NAMES: the travel across is many times the slop.
      expect(to - from).toBeGreaterThan(100)

      await act(async () => {
        await moveTo(x, from)
        await press()
        await moveTo(x + 2, (from + to) / 2)
        await moveTo(x + 3, to)
        await twoFrames()
      })
      expect(getComputedStyle(rail).cursor).not.toBe('grabbing')
      await act(async () => {
        await release()
        await twoFrames()
      })
      expect(turned).toEqual([62])
      expect(rail.scrollLeft).toBe(before)
    })

    // A press that lands on no tile: in the gap between two, and in the
    // rail's own padding above them. Either drags the rail.
    it(`drags from a press between two tiles, and from one in the rail's padding, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const { rail, turned } = strip(reading)
      const tile = tileOf(rail, 62)
      const { y } = middleOf(tile)
      const stood = leftOf(tile)
      const styles = getComputedStyle(rail)
      const between =
        tile.getBoundingClientRect().right +
        Number.parseFloat(styles.columnGap) / 2
      // THE STATE THIS TEST NAMES: what is under the press is the rail.
      expect(document.elementFromPoint(between, y)).toBe(rail)

      await act(async () => {
        await moveTo(between, y)
        await press()
        await along(between, y, 0, 90)
        await twoFrames()
      })
      expectMoved(leftOf(tile) - stood, 90)
      expect(getComputedStyle(rail).cursor).toBe('grabbing')
      await act(async () => {
        await release()
        await twoFrames()
      })

      const above =
        rail.getBoundingClientRect().top +
        Number.parseFloat(styles.paddingTop) / 2
      const { x } = middleOf(tile)
      expect(document.elementFromPoint(x, above)).toBe(rail)
      await act(async () => {
        await moveTo(x, above)
        await press()
        await along(x, above, 0, -90)
        await release()
        await twoFrames()
      })
      // Back where it stood, and neither press turned a page.
      expectMoved(leftOf(tile) - stood, 0)
      expect(turned).toEqual([])
    })

    // Before a press is a drag nothing has captured the pointer, so it can
    // leave the strip with the button down, and the button can come up out
    // there, on something that is not the rail's. The press ends there all
    // the same.
    it(`ends a press that left the strip and was let go outside it, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const { rail, turned } = strip(reading)
      const tile = tileOf(rail, 62)
      const { x, y } = middleOf(tile)
      const stood = leftOf(tile)
      const resting = getComputedStyle(rail).cursor
      const outside = rail.getBoundingClientRect().bottom + 200

      // Out across the rail, with less than the slop of travel along it, and
      // let go out there. Then another press, made out there and carried in
      // over the strip: it reaches the rail as moves with the button down and
      // no press on the rail before them. It is carried on away from where
      // the first press was made, so that a rail still following that press
      // would stand well off where it was.
      await act(async () => {
        await moveTo(x, y)
        await press()
        await moveTo(x + 2, outside)
        await release()
        await moveTo(x + 122, outside)
        await press()
        await moveTo(x + 122, y)
        await along(x + 122, y, 0, 120)
        await twoFrames()
      })
      expect(leftOf(tile) - stood).toBe(0)
      expect(getComputedStyle(rail).cursor).toBe(resting)
      await act(async () => {
        await release()
        await twoFrames()
      })
      expect(turned).toEqual([])

      // The same first press again, and this time the pointer comes back
      // over the strip and along it with no button down.
      await act(async () => {
        await moveTo(x, y)
        await press()
        await moveTo(x + 2, outside)
        await release()
        await moveTo(x + 2, y)
        await along(x + 2, y, 0, 120)
        await twoFrames()
      })
      expect(leftOf(tile) - stood).toBe(0)
      expect(getComputedStyle(rail).cursor).toBe(resting)
      expect(turned).toEqual([])

      // THE CONTROL: the strip is as it was, and a click on the tile that was
      // pressed first turns the page to it.
      await act(async () => {
        await moveTo(x, y)
        await press()
        await release()
        await twoFrames()
      })
      expect(turned).toEqual([62])
    })

    // A hand that presses a tile near the strip's edge, slips a few pixels
    // off the strip and comes back is still pressing: its leaving the rail
    // ends nothing. Carried along the rail from there it drags, and a drag
    // turns no page.
    it(`drags for a press that slips off the strip's edge and back before the slop, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const { rail, turned } = strip(reading)
      const tile = tileOf(rail, 62)
      const box = tile.getBoundingClientRect()
      const x = Math.round(box.left + box.width / 2)
      const y = Math.floor(box.bottom - 2)
      const stood = leftOf(tile)
      const off = rail.getBoundingClientRect().bottom + 3
      // THE STATE THIS TEST NAMES: the press is on the tile, and the place
      // the pointer slips to is not the rail's.
      expect(tile.contains(document.elementFromPoint(x, y))).toBe(true)
      expect(rail.contains(document.elementFromPoint(x + 1, off))).toBe(false)

      await act(async () => {
        await moveTo(x, y)
        await press()
        await moveTo(x + 1, off)
        await moveTo(x + 2, y)
        await along(x, y, 2, 42)
        await twoFrames()
      })
      expectMoved(leftOf(tile) - stood, 42)
      expect(getComputedStyle(rail).cursor).toBe('grabbing')

      await act(async () => {
        await release()
        await twoFrames()
      })
      expect(turned).toEqual([])
    })

    // A mouse focuses the tile it presses, and Space and Enter press a tile
    // that has focus, which turns the page to it. Once the press is a drag,
    // the focus it put on that tile is back with whatever had it before.
    it(`hands focus back when a press becomes a drag, and Space and Enter then turn no page, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const { rail, turned, elsewhere } = strip(reading)
      const tile = tileOf(rail, 62)
      const { x, y } = middleOf(tile)
      const stood = leftOf(tile)
      act(() => elsewhere.focus())

      await act(async () => {
        await moveTo(x, y)
        await press()
        await twoFrames()
      })
      // THE STATE THIS TEST NAMES: the press has put focus on the tile.
      expect(document.activeElement).toBe(tile)

      await act(async () => {
        await along(x, y, 0, 120)
        await twoFrames()
      })
      // Handed back with the button still down, and there after the release.
      expect(document.activeElement).toBe(elsewhere)
      await act(async () => {
        await release()
        await twoFrames()
      })
      expectMoved(leftOf(tile) - stood, 120)
      expect(document.activeElement).toBe(elsewhere)

      await act(async () => {
        await userEvent.keyboard(' ')
        await userEvent.keyboard('{Enter}')
        await twoFrames()
      })
      expect(turned).toEqual([])

      // THE CONTROL: each key does press a tile that has focus. The tile has
      // followed the pointer, so a click where the pointer is lands on it.
      await act(async () => {
        await press()
        await release()
        await twoFrames()
      })
      expect(document.activeElement).toBe(tile)
      expect(turned).toEqual([62])
      await act(async () => {
        await userEvent.keyboard(' ')
        await twoFrames()
      })
      expect(turned).toEqual([62, 62])
      await act(async () => {
        await userEvent.keyboard('{Enter}')
        await twoFrames()
      })
      expect(turned).toEqual([62, 62, 62])
    })

    // Two more places focus can be before a press. Nowhere, which the
    // document's body stands for and which takes no focus back: the tile is
    // left without it. And on the rail's own stop, the current tile, where a
    // reader who came to the strip by `Tab` has it: it goes back there.
    it(`leaves the dragged tile without focus where nothing had it, and hands it back to the rail's own stop where that had it, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const { rail, turned, current } = strip(reading)
      const tile = tileOf(rail, 62)
      const { x, y } = middleOf(tile)
      act(() => current.blur())
      expect(document.activeElement).toBe(document.body)

      await act(async () => {
        await moveTo(x, y)
        await press()
        await twoFrames()
      })
      expect(document.activeElement).toBe(tile)
      await act(async () => {
        await along(x, y, 0, 60)
        await release()
        await twoFrames()
      })
      expect(document.activeElement).toBe(document.body)
      await act(async () => {
        await userEvent.keyboard(' ')
        await userEvent.keyboard('{Enter}')
        await twoFrames()
      })
      expect(turned).toEqual([])

      act(() => current.focus({ preventScroll: true }))
      await act(async () => {
        await press()
        await twoFrames()
      })
      expect(document.activeElement).toBe(tile)
      await act(async () => {
        await along(x, y, 60, 120)
        await release()
        await twoFrames()
      })
      expect(document.activeElement).toBe(current)
      // The keys are the stop's again, as they were before the drag: they
      // press the current tile, which is the page the reader is on.
      await act(async () => {
        await userEvent.keyboard(' ')
        await twoFrames()
      })
      expect(turned).toEqual([60])
    })

    // And on the very tile that is pressed. A click on a tile leaves focus on
    // it, and a drag begun on that tile afterwards moves focus nowhere, so
    // there is nothing to hand back and the tile keeps it.
    it(`leaves focus on a tile that had it before it was pressed and dragged, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const { rail, turned } = strip(reading)
      const tile = tileOf(rail, 62)
      const { x, y } = middleOf(tile)
      const stood = leftOf(tile)

      await act(async () => {
        await moveTo(x, y)
        await press()
        await release()
        await twoFrames()
      })
      // THE STATE THIS TEST NAMES: the tile has focus before the drag's press.
      expect(document.activeElement).toBe(tile)
      expect(turned).toEqual([62])

      await act(async () => {
        await press()
        await along(x, y, 0, 120)
        await release()
        await twoFrames()
      })
      expectMoved(leftOf(tile) - stood, 120)
      expect(document.activeElement).toBe(tile)
      expect(turned).toEqual([62])
    })

    // A thumbnail takes no pointer events, so a press on one is not the start
    // of the browser's drag of an image. A selection that takes in the tiles
    // is something the browser does drag, and its drag cancels the pointer
    // the rail is following.
    it(`is not taken from the reader by the browser's drag of a selection, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const { host, rail } = strip(reading)
      const tile = tileOf(rail, 62)
      const { x, y } = middleOf(tile)
      const stood = leftOf(tile)
      // Heard on the way down, before the rail has had its say.
      const heard: string[] = []
      host.addEventListener('dragstart', (e) => heard.push(e.type), true)
      host.addEventListener('pointercancel', (e) => heard.push(e.type), true)

      // THE STATE THIS TEST NAMES: the tile that is pressed is selected.
      act(() => document.getSelection()?.selectAllChildren(document.body))
      expect(document.getSelection()?.containsNode(tile, true)).toBe(true)

      await act(async () => {
        await moveTo(x, y)
        await press()
        await along(x, y, 0, 120)
        await release()
        await twoFrames()
      })

      // THE CONTROL is the `dragstart`: the browser did set out on a drag of
      // its own. And it did not then take the pointer.
      expect(heard).toEqual(['dragstart'])
      expectMoved(leftOf(tile) - stood, 120)
    })
  }

  /**
   * O216, the glide: a drag let go while the pointer is moving carries the
   * rail on.
   *
   * `tiles.test.ts` holds the arithmetic and `Filmstrip.test.tsx` what the
   * rail does with it, on events and frames those cases make themselves.
   * Here the mouse, the events and the frames are the browser's.
   *
   * **What a case's outcome owes the clock: nothing, by its inputs.** A
   * pointer's speed is the time between its moves, and whether it had rested
   * is the time between its last move and its release. Sent as they come,
   * those would be how long this machine took between two sends. So each
   * input of a flick carries a time the case gives it (`mouse.clock`), and
   * the case says what speed that makes before it leans on it.
   *
   * What a glide then does takes real frames, and no case reads the rail
   * after a wait of so many milliseconds. Each waits for the rail to ask for
   * no more frames, or for something to be so, and gives up after seconds.
   *
   * Several cases press, or bring focus to the rail, while a glide is under
   * way, and what a press is then depends on how fast the glide is still
   * going. None takes that from how long it has been: each reads the rail's
   * speed off the rail's own movement over the frames just past, and says
   * it before it acts. A machine that stalls between that and the act can
   * only have the glide slower by then, or over.
   */
  describe('let go while the pointer is moving', () => {
    /** Every frame anything on the page asks for, the case's own among them. */
    let asked: MockInstance<typeof requestAnimationFrame>
    /** Every frame anything on the page gives back: a press gives back the one a glide had asked for. */
    let givenBack: MockInstance<typeof cancelAnimationFrame>
    beforeEach(() => {
      asked = vi.spyOn(window, 'requestAnimationFrame')
      givenBack = vi.spyOn(window, 'cancelAnimationFrame')
    })
    afterEach(() => {
      asked.mockRestore()
      givenBack.mockRestore()
    })

    /**
     * How fast the rail is going, in px a millisecond: how far it moves over
     * `count` frames, by the times those frames are stamped with. A frame to
     * an `act`, as `frames` has it.
     */
    async function speedOfTheRail(
      rail: HTMLElement,
      count: number,
    ): Promise<number> {
      const read = async () => {
        let stamped = Number.NaN
        await act(async () => {
          stamped = await new Promise<number>((resolve) =>
            requestAnimationFrame(resolve),
          )
        })
        return { at: stamped, left: rail.scrollLeft }
      }
      const from = await read()
      let to = from
      for (let frame = 0; frame < count; frame++) to = await read()
      return Math.abs(to.left - from.left) / (to.at - from.at)
    }

    /**
     * Waits until the rail, by its own movement over six frames, is going at
     * under 0.06 px a millisecond: well under the 0.1 that is the line, and
     * it only goes slower from there. Gives up after ten seconds, which fails
     * the case.
     */
    async function untilSlow(rail: HTMLElement) {
      const deadline = performance.now() + 10_000
      while ((await speedOfTheRail(rail, 6)) >= 0.06) {
        if (performance.now() > deadline) {
          throw new Error('the rail has not slowed after ten seconds')
        }
      }
    }

    /**
     * The tile the next press on the page lands on, by the press's own
     * event, heard on its way down: `null` where it lands on no tile, and
     * nothing until there has been a press.
     */
    function nextPressed(): { tile?: HTMLElement | null } {
      const heard: { tile?: HTMLElement | null } = {}
      window.addEventListener(
        'pointerdown',
        (event) => {
          heard.tile = (event.target as Element).closest('button')
        },
        { capture: true, once: true },
      )
      return heard
    }

    /**
     * Lets one frame pass, and answers whether anything but this asked for a
     * frame meanwhile. A glide asks for one in every frame it runs, and the
     * rail's scroll listener for one after every scroll.
     */
    async function aFrameWasAskedFor(): Promise<boolean> {
      const before = asked.mock.calls.length
      await frames(1)
      return asked.mock.calls.length - before > 1
    }

    /**
     * Waits for the rail to be at rest, which is two frames running with
     * nothing but this asking for one, and answers how many frames that
     * took. Gives up after ten seconds, which fails the case.
     */
    async function untilAtRest(): Promise<number> {
      const deadline = performance.now() + 10_000
      let waited = 0
      let quiet = 0
      while (quiet < 2) {
        if (performance.now() > deadline) {
          throw new Error('the rail still asks for frames after ten seconds')
        }
        quiet = (await aFrameWasAskedFor()) ? 0 : quiet + 1
        waited += 1
      }
      return waited
    }

    /**
     * A flick from (`x`, `y`): a press, `moves` moves of `step` px along the
     * rail and a release, each move stamped 15 ms after the one before and
     * the release `rest` ms after the last move. With no `rest` it is let go
     * as it moves.
     *
     * The speed is read over the 100 ms before the last move, from the press
     * on. Eight moves of 15 px end 105 ms after the first of them, so it is
     * read from the second: 90 px in 90 ms. Six end 90 ms after the press and
     * are read from it: 90 px in 90 ms. Either way a pointer going at
     * `step` / 15 px a millisecond.
     */
    async function flick(
      x: number,
      y: number,
      step: number,
      moves: number,
      rest = 0,
    ) {
      await act(async () => {
        await moveTo(x, y)
        mouse.clock(0)
        await press()
        for (let move = 1; move <= moves; move++) {
          mouse.clock(15 * move)
          await moveTo(x + step * move, y)
        }
        mouse.clock(15 * moves + rest)
        await release()
        mouse.clock()
      })
    }

    /**
     * What the window hears of a drag of `rail`: when it was pressed, each
     * move made with the button down, as the place and time `releaseSpeed`
     * reads, and at the release its time and where the rail stood. The rail's
     * own listener for a release is on the window too and was put there
     * first, so by then a glide has been begun and has written nothing.
     */
    function listenIn(rail: HTMLElement) {
      const trail: GlideSample[] = []
      const pressed = { at: Number.NaN }
      const letGo = { at: Number.NaN, scrollLeft: Number.NaN }
      const onDown = (event: PointerEvent) => {
        trail.length = 0
        pressed.at = event.timeStamp
      }
      const onMove = (event: PointerEvent) => {
        if (event.buttons & 1) {
          trail.push({ at: event.timeStamp, x: event.clientX })
        }
      }
      const onUp = (event: PointerEvent) => {
        letGo.at = event.timeStamp
        letGo.scrollLeft = rail.scrollLeft
      }
      window.addEventListener('pointerdown', onDown, true)
      window.addEventListener('pointermove', onMove, true)
      window.addEventListener('pointerup', onUp, true)
      onTestFinished(() => {
        window.removeEventListener('pointerdown', onDown, true)
        window.removeEventListener('pointermove', onMove, true)
        window.removeEventListener('pointerup', onUp, true)
      })
      return { trail, pressed, letGo }
    }

    for (const reading of ['ltr', 'rtl'] as const) {
      it(`carries the rail 325 px on from a pointer going at 1 px a millisecond, the way it was going, and comes to rest, ${reading}`, async () => {
        await page.viewport(1440, 900)
        const { rail, turned, landed, passed, current } = strip(reading, true)
        const { trail, letGo } = listenIn(rail)
        const tile = tileOf(rail, 62)
        const { x, y } = middleOf(tile)
        const stood = leftOf(tile)
        const mounted = rail.scrollLeft
        const resting = getComputedStyle(rail).cursor
        expect(resting).not.toBe('grabbing')

        await flick(x, y, 15, 8)
        // THE STATE THIS TEST NAMES: the browser gave the page the flick the
        // case sent, a pointer going right at 1 px a millisecond as it was
        // let go.
        expect(releaseSpeed(trail, letGo.at)).toBeCloseTo(1, 1)
        // Where the drag had taken the tile by then.
        const dragged = mounted - letGo.scrollLeft
        expectMoved(dragged, 120)
        // The drag has ended as a drag, with the glide still to run: the
        // cursor is put back, and the focus the press put on the tile is back
        // where the drag handed it.
        expect(getComputedStyle(rail).cursor).toBe(resting)
        expect(document.activeElement).toBe(current)

        await untilAtRest()

        // On to the right, as the pointer was going, by 1 px a millisecond
        // times 325 ms: to the two pixels either side that the events'
        // times, the rail's whole pixels and the frame's scale leave.
        const on = leftOf(tile) - stood - dragged
        expect(on).toBeGreaterThanOrEqual(325 - 2)
        expect(on).toBeLessThanOrEqual(325 + 2)
        // And at rest: it stands there.
        const rest = rail.scrollLeft
        await frames(5)
        expect(rail.scrollLeft).toBe(rest)
        // The release's click is a drag's, swallowed at the rail.
        expect(turned).toEqual([])
        expect(landed).toEqual([rail])
        expect(passed).toEqual([])
      })

      // Ten milliseconds either side of the 60 that make a rest, and not on
      // them: an event's time reaches the page rounded, here to a tenth of a
      // millisecond, so a release sent 60 ms after a move can be heard a
      // hair under 60. The line itself is `tiles.test.ts`'s.
      it(`leaves the rail where it was let go by a pointer that had rested, ${reading}`, async () => {
        await page.viewport(1440, 900)
        const { rail, turned } = strip(reading, true)
        const { trail, letGo } = listenIn(rail)
        const tile = tileOf(rail, 62)
        const { x, y } = middleOf(tile)
        const stood = leftOf(tile)

        await flick(x, y, 15, 8, 70)
        // THE STATE THIS TEST NAMES: a pointer that was going at 1 px a
        // millisecond, let go 70 ms after its last move.
        const last = trail[trail.length - 1]
        expect(releaseSpeed(trail, last.at)).toBeCloseTo(1, 1)
        expect(letGo.at - last.at).toBeCloseTo(70, 0)

        await frames(10)
        expect(rail.scrollLeft).toBe(letGo.scrollLeft)
        expectMoved(leftOf(tile) - stood, 120)
        expect(await aFrameWasAskedFor()).toBe(false)
        expect(turned).toEqual([])

        // THE CONTROL: the same flick let go 50 ms after its last move is
        // carried on.
        const again = middleOf(tile)
        await flick(again.x, again.y, 15, 8, 50)
        expect(letGo.at - trail[trail.length - 1].at).toBeCloseTo(50, 0)
        const from = letGo.scrollLeft
        await expect
          .poll(() => Math.abs(rail.scrollLeft - from), { timeout: 5000 })
          .toBeGreaterThan(30)
      })

      // The system's setting is asked of the browser's own `matchMedia`, by
      // a query the browser has to know. The DevTools session has it answer
      // as it does where the system is set to reduce motion.
      it(`carries nothing on where the system asks for reduced motion, ${reading}`, async () => {
        await page.viewport(1440, 900)
        const asks = (value: string) =>
          cdp().send('Emulation.setEmulatedMedia', {
            features: [{ name: 'prefers-reduced-motion', value }],
          })
        const reduced = () =>
          window.matchMedia('(prefers-reduced-motion: reduce)').matches
        onTestFinished(async () => {
          await asks('')
        })
        await asks('reduce')
        // THE STATE THIS TEST NAMES: this page is told the system asks.
        expect(reduced()).toBe(true)
        const { rail } = strip(reading, true)
        const { trail, letGo } = listenIn(rail)
        const tile = tileOf(rail, 62)
        const { x, y } = middleOf(tile)

        await flick(x, y, 15, 8)
        expect(releaseSpeed(trail, letGo.at)).toBeCloseTo(1, 1)
        await frames(10)
        expect(rail.scrollLeft).toBe(letGo.scrollLeft)
        expect(await aFrameWasAskedFor()).toBe(false)

        // THE CONTROL: the same flick once the system does not ask.
        await asks('no-preference')
        expect(reduced()).toBe(false)
        const again = middleOf(tile)
        await flick(again.x, again.y, 15, 8)
        const from = letGo.scrollLeft
        await expect
          .poll(() => Math.abs(rail.scrollLeft - from), { timeout: 5000 })
          .toBeGreaterThan(30)
      })

      // The tile under a press that stops a glide still carrying the rail is
      // one the glide carried there. The press is the reader stopping the
      // strip.
      it(`stops at a press while it is still carrying the rail, which turns no page and leaves focus where it was, ${reading}`, async () => {
        await page.viewport(1440, 900)
        const { rail, turned, landed, passed, current } = strip(reading, true)
        const tile = tileOf(rail, 62)
        const { x, y } = middleOf(tile)

        await flick(x, y, 15, 8)
        await frames(4)
        // THE STATE THIS TEST NAMES: a glide carrying the rail, by the rail's
        // own movement, at over three times the 0.1 px a millisecond under
        // which a press is a click; and focus where the flick's own drag
        // handed it back.
        expect(await speedOfTheRail(rail, 2)).toBeGreaterThan(0.3)
        expect(document.activeElement).toBe(current)

        // On a tile, and 30 px ahead of its middle the way the rail is
        // carrying it, which is to the right: the tile is still under the
        // pointer when the press lands, a few frames on. Which tile that was
        // is the press's own event's to say.
        const ahead = middleOf(tileInTheMiddle(rail))
        const heard = nextPressed()
        await act(async () => {
          await moveTo(ahead.x + 30, ahead.y)
          await press()
        })
        const pressed = heard.tile
        if (!pressed) throw new Error('the press landed on no tile')
        expect(pressed).not.toBe(current)
        const stoppedAt = leftOf(pressed)
        await frames(2)
        expect(document.activeElement).toBe(pressed)
        await act(async () => {
          await release()
        })
        await frames(10)

        expect(leftOf(pressed)).toBe(stoppedAt)
        expect(await aFrameWasAskedFor()).toBe(false)
        expect(turned).toEqual([])
        expect(document.activeElement).toBe(current)
        // The flick's click came to the rail, which had the pointer, and
        // this one to the tile. Neither came back up past the rail.
        expect(landed.map((on) => (on as Element).closest('button'))).toEqual([
          null,
          pressed,
        ])
        expect(passed).toEqual([])

        // THE CONTROL: the same press and release with the rail at rest is a
        // click on that tile.
        await act(async () => {
          await press()
          await release()
          await twoFrames()
        })
        expect(turned).toEqual([Number(pressed.dataset.group)])
      })

      // A glide runs on until it is half a pixel from rest, and for most of
      // that time the strip has stopped to the eye. A press on a tile then
      // is the reader choosing the tile: it stops the glide, as any press
      // does, and it is a click.
      it(`takes a press as the click it is once the glide has slowed under 0.1 px a millisecond, ${reading}`, async () => {
        await page.viewport(1440, 900)
        const { rail, turned, passed, current } = strip(reading, true)
        const tile = tileOf(rail, 62)
        const { x, y } = middleOf(tile)

        await flick(x, y, 15, 8)
        await untilSlow(rail)
        // THE STATE THIS TEST NAMES: and the glide is still under way. It
        // asks for a frame in each frame it runs.
        expect(await aFrameWasAskedFor()).toBe(true)

        const pressed = tileInTheMiddle(rail)
        expect(pressed).not.toBe(current)
        const at = middleOf(pressed)
        const before = givenBack.mock.calls.length
        await act(async () => {
          await moveTo(at.x, at.y)
          await press()
        })
        // The press gave back the frame the glide had asked for: there was a
        // glide for it to stop when it landed, however long it took to land.
        expect(givenBack.mock.calls.length).toBe(before + 1)
        const stoppedAt = leftOf(pressed)
        await act(async () => {
          await release()
          await twoFrames()
        })
        await frames(5)

        // It stopped the glide, and it is a click on the tile: the page is
        // turned to, the click went on up past the rail, and focus is the
        // tile's to keep.
        expect(leftOf(pressed)).toBe(stoppedAt)
        expect(await aFrameWasAskedFor()).toBe(false)
        expect(turned).toEqual([Number(pressed.dataset.group)])
        expect(passed.map((on) => (on as Element).closest('button'))).toEqual([
          pressed,
        ])
        expect(document.activeElement).toBe(pressed)
      })

      // A press stops a glide whether or not it moves focus, and a tile
      // taking focus stops one by itself. So this press is in the rail's own
      // padding, above the tiles, where it lands on no tile and moves focus
      // to none.
      it(`stops at a press on the rail where no tile is, fast or slow, ${reading}`, async () => {
        await page.viewport(1440, 900)
        const { rail, turned } = strip(reading, true)
        const { x, y } = middleOf(tileOf(rail, 62))

        await flick(x, y, 15, 8)
        await frames(4)
        // THE STATE THIS TEST NAMES: a glide carrying the rail, by the rail's
        // own movement.
        expect(await speedOfTheRail(rail, 2)).toBeGreaterThan(0.3)

        const above =
          rail.getBoundingClientRect().top +
          Number.parseFloat(getComputedStyle(rail).paddingTop) / 2
        const before = givenBack.mock.calls.length
        const heard = nextPressed()
        await act(async () => {
          await moveTo(x, above)
          await press()
        })
        // The press landed on no tile, and gave back the glide's frame.
        expect(heard.tile).toBeNull()
        expect(givenBack.mock.calls.length).toBe(before + 1)
        const stoppedAt = rail.scrollLeft
        await frames(10)

        expect(rail.scrollLeft).toBe(stoppedAt)
        expect(await aFrameWasAskedFor()).toBe(false)
        await act(async () => {
          await release()
          await twoFrames()
        })
        expect(turned).toEqual([])

        // And a glide that has slowed under 0.1 px a millisecond stops at a
        // press there as well: the speed decides what the press is, and not
        // whether it stops the glide.
        const again = middleOf(tileInTheMiddle(rail))
        await flick(again.x, again.y, 15, 8)
        await untilSlow(rail)
        expect(await aFrameWasAskedFor()).toBe(true)
        const beforeTheSlow = givenBack.mock.calls.length
        const heardSlow = nextPressed()
        await act(async () => {
          await moveTo(again.x, above)
          await press()
        })
        expect(heardSlow.tile).toBeNull()
        expect(givenBack.mock.calls.length).toBe(beforeTheSlow + 1)
        const slowStoppedAt = rail.scrollLeft
        await frames(10)
        expect(rail.scrollLeft).toBe(slowStoppedAt)
        expect(await aFrameWasAskedFor()).toBe(false)
        await act(async () => {
          await release()
          await twoFrames()
        })
        expect(turned).toEqual([])
      })

      // A press that creeps to the 5 px that make a drag and is let go at
      // once: a pixel every 40 ms, and the last of its six 2 ms after the
      // one before. Read from the press on it is going at 3 px in 82 ms,
      // which carries nothing. Its last two moves by themselves are a pixel
      // in 2 ms.
      it(`carries nothing on from a press that creeps past 5 px and is let go, though its last two moves come 2 ms apart, ${reading}`, async () => {
        await page.viewport(1440, 900)
        const { rail, turned } = strip(reading, true)
        const { trail, letGo } = listenIn(rail)
        const { x, y } = middleOf(tileOf(rail, 62))
        const mounted = rail.scrollLeft

        await act(async () => {
          await moveTo(x, y)
          mouse.clock(0)
          await press()
          for (const [at, px] of [
            [40, 1],
            [80, 2],
            [120, 3],
            [160, 4],
            [200, 5],
            [202, 6],
          ]) {
            mouse.clock(at)
            await moveTo(x + px, y)
          }
          await release()
          mouse.clock()
        })
        // THE STATE THIS TEST NAMES: the press did become a drag, and its
        // last two moves were heard as the case sent them, which read by
        // themselves is 0.5 px a millisecond.
        expect(letGo.scrollLeft).not.toBe(mounted)
        expect(trail).toHaveLength(6)
        expect(releaseSpeed(trail.slice(-2), letGo.at)).toBeCloseTo(0.5, 1)

        await frames(10)
        expect(rail.scrollLeft).toBe(letGo.scrollLeft)
        expect(await aFrameWasAskedFor()).toBe(false)
        expect(turned).toEqual([])
      })

      // A flick shorter than the 100 ms the speed is read over is read
      // whole, from its press: 45 px in 30 ms is 1.5 px a millisecond, and
      // 325 ms of that is 487.5 px.
      it(`carries the rail on from a quick flick let go 30 ms after its press, ${reading}`, async () => {
        await page.viewport(1440, 900)
        const { rail } = strip(reading, true)
        const { pressed, letGo } = listenIn(rail)
        const { x, y } = middleOf(tileOf(rail, 62))

        await act(async () => {
          await moveTo(x, y)
          mouse.clock(0)
          await press()
          for (const move of [1, 2, 3]) {
            mouse.clock(10 * move)
            await moveTo(x + 15 * move, y)
          }
          await release()
          mouse.clock()
        })
        // THE STATE THIS TEST NAMES: let go 30 ms after the press, as the
        // page heard them.
        expect(letGo.at - pressed.at).toBeCloseTo(30, 0)

        await untilAtRest()

        const on = Math.abs(rail.scrollLeft - letGo.scrollLeft)
        expect(on).toBeGreaterThanOrEqual(487.5 - 3)
        expect(on).toBeLessThanOrEqual(487.5 + 3)
      })

      // `Tab` brings focus to the rail's one stop, the tile of the page
      // being read, and the browser scrolls that tile into view. A glide's
      // next frame would write the rail back to where the glide had it
      // going, and the tile out of sight again. Focus starts on the button
      // after the strip, so it is `Tab` with Shift that goes to the rail.
      it(`stops when Tab brings focus to the rail, and leaves the rail where the browser scrolled it, ${reading}`, async () => {
        await page.viewport(1440, 900)
        const { rail, current, elsewhere } = strip(reading, true)
        act(() => elsewhere.focus())
        const { x, y } = middleOf(tileOf(rail, 62))

        // Toward later pages, at 4 px a millisecond.
        await flick(x, y, -laterSide(rail) * 60, 6)
        await frames(4)
        // THE STATE THIS TEST NAMES: a glide carrying the rail, by the rail's
        // own movement; and focus outside the strip, on the button after it.
        expect(await speedOfTheRail(rail, 2)).toBeGreaterThan(0.3)
        expect(document.activeElement).toBe(elsewhere)

        await act(async () => {
          await userEvent.keyboard('{Shift>}{Tab}{/Shift}')
        })
        expect(document.activeElement).toBe(current)
        const stoppedAt = rail.scrollLeft
        await frames(10)

        expect(rail.scrollLeft).toBe(stoppedAt)
        expect(await aFrameWasAskedFor()).toBe(false)
        expect(document.activeElement).toBe(current)
        // And the tile that took focus is on show, inside the rail's box.
        const tile = current.getBoundingClientRect()
        const box = rail.getBoundingClientRect()
        expect(tile.left).toBeGreaterThanOrEqual(box.left)
        expect(tile.right).toBeLessThanOrEqual(box.right)
      })

      // Each end of the rail. The start of the volume is where `scrollLeft` is
      // 0 in either reading direction. Its end is as far the other way as the
      // rail goes, which is over 0 one way round and under it the other.
      for (const which of ['start', 'end'] as const) {
        it(`stops at the ${which} of the volume where it is carried to it, ${reading}`, async () => {
          await page.viewport(1440, 900)
          const { rail } = strip(reading, true)
          const { trail, letGo } = listenIn(rail)
          const later = laterSide(rail)
          const end =
            which === 'start'
              ? 0
              : later * (rail.scrollWidth - rail.clientWidth)
          // 150 px short of that end.
          await act(async () => {
            rail.scrollLeft = end + (which === 'start' ? later : -later) * 150
            await twoFrames()
          })
          await frames(3)
          expect(Math.abs(rail.scrollLeft - end)).toBe(150)

          // Toward that end, which is the content going away from it: to the
          // side the later pages lie on for the start, and the earlier for
          // the end.
          const { x, y } = middleOf(tileInTheMiddle(rail))
          await flick(x, y, (which === 'start' ? later : -later) * 15, 6)
          // THE STATE THIS TEST NAMES: let go 60 px short of the end, by a
          // pointer going at 1 px a millisecond, which carries a rail 325.
          expectMoved(Math.abs(letGo.scrollLeft - end), 60)
          expect(Math.abs(releaseSpeed(trail, letGo.at))).toBeCloseTo(1, 1)

          const waited = await untilAtRest()

          expect(rail.scrollLeft).toBe(end)
          // And the glide ended there. Left to come to rest by itself it asks
          // for frames for two seconds, well over a hundred of them.
          expect(waited).toBeLessThan(40)
        })
      }

      // A tile that takes focus has the strip's window moved to it, for a
      // reader who reaches a tile by `Tab`. Focus handed back is no such
      // reaching. Here it goes back to the tile of the page being read, which
      // the glide has carried the rail on from: a window moved to that tile
      // ends short of what the rail shows, and with the glide stopped no
      // scroll follows to move it again.
      it(`leaves no stretch of the strip bare when a press that stopped a glide hands focus back to a tile the rail has left behind, ${reading}`, async () => {
        await page.viewport(1440, 900)
        const { rail, turned, current } = strip(reading, true)
        const mounted = rail.scrollLeft
        const { x, y } = middleOf(current)
        // Toward later pages, and hard: the pointer goes to the side the
        // earlier pages lie on at 4 px a millisecond.
        const earlier = -laterSide(rail)
        await flick(x, y, earlier * 60, 6)
        // Until the glide has carried the rail 1,350 px on from where the
        // strip mounted. Ten seconds at most.
        const deadline = performance.now() + 10_000
        while (Math.abs(rail.scrollLeft - mounted) < 1350) {
          if (performance.now() > deadline) {
            throw new Error('the rail has not got there after ten seconds')
          }
          await frames(1)
        }
        // THE STATE THIS TEST NAMES: a glide still carrying the rail, by the
        // rail's own movement, at over three times the 0.1 px a millisecond
        // under which a press hands no focus back; the rail well on from the
        // tile of the page being read, which it still holds and which has
        // focus; and the strip full of tiles.
        expect(await speedOfTheRail(rail, 2)).toBeGreaterThan(0.3)
        expect(document.activeElement).toBe(current)
        expect(bare(rail)).toBe(0)

        // On a tile, and 40 px ahead of its middle the way the rail is
        // carrying it, so that the tile is under the pointer when the press
        // lands. The press's own event says which tile it was.
        const ahead = middleOf(tileInTheMiddle(rail))
        const heard = nextPressed()
        await act(async () => {
          await moveTo(ahead.x + earlier * 40, ahead.y)
          await press()
          await twoFrames()
        })
        const pressed = heard.tile
        if (!pressed) throw new Error('the press landed on no tile')
        expect(document.activeElement).toBe(pressed)
        await act(async () => {
          await release()
          await twoFrames()
        })
        await frames(5)

        expect(document.activeElement).toBe(current)
        expect(bare(rail)).toBe(0)
        expect(turned).toEqual([])
      })
    }
  })
})
