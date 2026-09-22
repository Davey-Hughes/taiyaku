import { describe, expect, it } from 'vitest'
import { page } from 'vitest/browser'
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
import { mount } from './testing/mount'
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

/** The rail needs the reader's custom properties, which live on `.reader`. */
function inReader(ui: React.ReactNode) {
  return mount(
    <div className="reader" style={{ position: 'fixed', inset: 0 }}>
      <div style={{ height: 'var(--strip-height)' }}>{ui}</div>
    </div>,
  )
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
