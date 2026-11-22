import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { page } from 'vitest/browser'
import * as aboutApi from '../src/about'
import type { Me } from '../src/api'
import { COVER_MIN } from '../src/browseModule'
// **Required**, the same reason `Config.browser.test.tsx` gives.
import '../src/index.css'
import * as library from '../src/library'
import { SIDEBAR_LAYOUT } from '../src/phoneLayout'
import { ABOUT } from './support/about'
import { settleUntil } from './support/async'
import { byHand, clickAt, pointAt, until } from './support/byHand'
import { inProgress, seriesCard } from './support/continueRow'
import { homePage } from './support/homePage'
import { afterMotion } from './support/motion'
import { mouse } from './support/mouse'
import { pointerRulesAs } from './support/pointer'

/**
 * The Continue row's panel (O417) beside `main`'s scrollbar where a
 * scrollbar takes width, as a classic one does: this file runs only in the
 * Chromium launch that draws them (`vite.config.ts`'s `SHOWN_SCROLLBARS`).
 * A press on what scrolls the page closes nothing, so ↓ Download can be
 * reached by its scrollbar; one beside it closes the panel, as any press
 * outside does (O418). What scrolls the page keeps its scrollbar's gutter
 * while a panel can open, so a panel run past the end of a page that did
 * not scroll moves nothing (`library.css`).
 */

// The build's label at the sidebar's foot asks for the build; answered here.
vi.mock('../src/about', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/about')>()),
  about: vi.fn(),
}))

vi.mock('../src/library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/library')>()),
  status: vi.fn(),
  home: vi.fn(),
  grid: vi.fn(),
  oneSeries: vi.fn(),
  search: vi.fn(),
  work: vi.fn(),
}))

/** An administrator who may download: the panel draws all it can. */
const ADMIN: Me = {
  user_id: 1,
  admin: true,
  can_download: true,
  can_stream: true,
  can_stamp: false,
}

/** Rows enough that `main` scrolls at 1280 px. */
const SERIES = 40
const ENTRIES = 4

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(aboutApi.about).mockResolvedValue(ABOUT)
  vi.mocked(library.status).mockResolvedValue({
    libraries: [{ id: 1, name: 'Volumes', scanned: true }],
    restricted: false,
  })
  vi.mocked(library.home).mockResolvedValue({
    continuing: inProgress(ENTRIES),
    added: [],
    read: [],
  })
  vi.mocked(library.grid).mockResolvedValue({
    series: Array.from({ length: SERIES }, (_, i) => seriesCard(i + 1)),
    unfiled: [],
  })
  vi.mocked(library.work).mockImplementation(async (slug) => {
    const card = inProgress(ENTRIES).find((w) => w.slug === slug)
    if (card === undefined) throw new Error(`no card ${slug}`)
    return { ...card, series: null, alignments: [], kept_apart: [] }
  })
})

function need<T extends Element = HTMLElement>(
  scope: ParentNode,
  selector: string,
): T {
  const found = scope.querySelector<T>(selector)
  if (!found) throw new Error(`no ${selector}`)
  return found
}

const rect = (el: Element) => el.getBoundingClientRect()

/**
 * The home page at 1280 px for an administrator, its row and grid drawn,
 * and the second card's panel opened as a hand opens it: the pointer onto
 * the card, then a press on its `i`.
 */
async function opened() {
  await mouse.moveTo(0, 0)
  const host = await homePage({ width: 1280, who: ADMIN })
  await settleUntil(
    'the Continue row and the grid',
    () =>
      host.querySelectorAll('section.continue > ul > li').length === ENTRIES &&
      host.querySelectorAll('main .grid > li').length === SERIES,
    2000,
  )
  await document.fonts.ready
  const card = host.querySelectorAll<HTMLElement>(
    'section.continue > ul > li',
  )[1]
  await openOn(host, card)
  return {
    host,
    main: need(host, '.library > main'),
    i: need<HTMLButtonElement>(card, ':scope > .panel-open'),
  }
}

/**
 * Opens `card`'s panel as a hand does, the pointer onto the card and then a
 * press on its `i`, and waits for its alignment detail to be read.
 */
async function openOn(host: HTMLElement, card: HTMLElement) {
  await byHand(async () => {
    await pointAt(need(card, ':scope > a'))
    await clickAt(need(card, ':scope > .panel-open'))
    await until('the panel’s alignment detail', () => {
      const detail = host.querySelector(
        'section.continue > .tile-panel .alignment-panel',
      )
      return detail !== null && detail.textContent !== 'Reading…'
    })
  })
  return need(host, 'section.continue > .tile-panel')
}

/** A real press and release at (`x`, `y`), outside `act` (`byHand`). */
async function pressAt(x: number, y: number) {
  await byHand(async () => {
    await mouse.moveTo(x, y)
    await mouse.press()
    await mouse.release()
  })
}

describe('the Continue row’s panel beside main’s scrollbar (O417)', () => {
  // A press on a scroller's scrollbar closes nothing (`volumePanel.ts`),
  // so ↓ Download can be reached by the page area's scrollbar.
  it('keeps the panel open through a real press on main’s scrollbar (O418)', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { host, main, i } = await opened()
    const bar = main.offsetWidth - main.clientWidth
    const box = rect(main)
    const x = box.right - bar / 2
    const y = box.top + box.height * 0.9
    // The fixture tells: the page area scrolls, its scrollbar takes width,
    // and a press there lands on the page area itself.
    expect({
      scrolls: main.scrollHeight > main.clientHeight,
      bar: bar > 0,
      on: document.elementFromPoint(x, y) === main,
    }).toEqual({ scrolls: true, bar: true, on: true })

    await pressAt(x, y)
    expect(
      {
        panel: host.querySelector('section.continue > .tile-panel') !== null,
        expanded: i.getAttribute('aria-expanded'),
      },
      'the panel after the press',
    ).toEqual({ panel: true, expanded: 'true' })
  })

  // Off it, the page area's blank space closes the panel, as any press
  // outside does: the scrollbar and the space beside it are both `main`'s.
  it('closes on a real press on main’s blank space beside its scrollbar (O418)', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { host, main, i } = await opened()
    const box = rect(main)
    const x = box.left + main.clientLeft + main.clientWidth - 2
    const y = box.top + box.height * 0.9
    // The fixture tells: the page area scrolls, its scrollbar takes width,
    // and a press 2 px short of the scrollbar lands on the page area itself.
    expect({
      scrolls: main.scrollHeight > main.clientHeight,
      bar: main.offsetWidth - main.clientWidth > 0,
      on: document.elementFromPoint(x, y) === main,
    }).toEqual({ scrolls: true, bar: true, on: true })

    await pressAt(x, y)
    expect(
      {
        panel: host.querySelector('section.continue > .tile-panel') !== null,
        expanded: i.getAttribute('aria-expanded'),
      },
      'the panel after the press',
    ).toEqual({ panel: false, expanded: 'false' })
  })
})

describe('the Continue row’s panel run past the end of a short home page (O417)', () => {
  // The panel at its tallest: a verdict, a content match and the stamp
  // section drawn, so it runs past the end of a page of one grid row.
  beforeEach(() => {
    vi.mocked(library.grid).mockResolvedValue({
      series: [seriesCard(1), seriesCard(2)],
      unfiled: [],
    })
    vi.mocked(library.work).mockImplementation(async (slug) => {
      const card = inProgress(ENTRIES).find((w) => w.slug === slug)
      if (card === undefined) throw new Error(`no card ${slug}`)
      return {
        ...card,
        editions: card.editions.map((e) => ({
          ...e,
          matched: { id: 9, score: 0.97 },
        })),
        series: null,
        alignments: [
          {
            from: 'raw',
            to: 'en',
            verdict: 'confident',
            slots: 180,
            paired: 178,
          },
        ],
        kept_apart: [],
      }
    })
  })

  /** An administrator who may stamp as well: the panel draws its stamp section. */
  const STAMPER: Me = { ...ADMIN, can_stamp: true }

  // Each page width and two window heights at the least cover size, where
  // one grid row is shorter than the panel: `short`, in which the page
  // scrolls with no panel open, and `tall`, which holds the page but not
  // the panel. The sidebar is drawn at 500 px and taller.
  const WINDOW = {
    1280: { short: 540, tall: 700 },
    390: { short: 600, tall: 745 },
  } as const

  /**
   * What scrolls the page in the layout drawn: `main` beside the sidebar,
   * and in the phone layout the document, where `main` does not scroll
   * (`shell.css`). `taken` is the width its scrollbar, or the gutter kept
   * for one, takes from its content.
   */
  function scroller(main: HTMLElement) {
    if (matchMedia(SIDEBAR_LAYOUT).matches) {
      return {
        scrolls: () => main.scrollHeight > main.clientHeight,
        shown: () => rect(main).top + main.clientTop + main.clientHeight,
        to: (y: number) => {
          main.scrollTop = y
        },
        end: () => main.scrollHeight,
        taken: () => main.offsetWidth - main.clientWidth,
      }
    }
    const root = document.documentElement
    return {
      scrolls: () => root.scrollHeight > root.clientHeight,
      shown: () => root.clientHeight,
      to: (y: number) => {
        window.scrollTo(0, y)
      },
      end: () => root.scrollHeight,
      // The root's `clientWidth` leaves out a scrollbar but not a gutter kept
      // with none drawn; its box is laid in what the gutter leaves.
      taken: () => window.innerWidth - rect(root).width,
    }
  }

  /** The width a classic scrollbar takes here. */
  function scrollbarWidth(): number {
    const probe = document.createElement('div')
    probe.style.cssText =
      'position: absolute; overflow-y: scroll; width: 100px; height: 10px'
    document.body.append(probe)
    const width = probe.offsetWidth - probe.clientWidth
    probe.remove()
    return width
  }

  /** Each box's left edge and width. */
  const placesOf = (boxes: HTMLElement[]) =>
    boxes.map((box) => {
      const { left, width } = rect(box)
      return { left, width }
    })

  /**
   * The home page at `width` by `height` px, at the least cover size, its
   * row and its one grid row drawn.
   */
  async function shortPage(
    width: 1280 | 390,
    height: number,
    panel: 'button' | 'off',
  ) {
    await mouse.moveTo(0, 0)
    const host = await homePage({
      width,
      height,
      coverSize: COVER_MIN,
      who: STAMPER,
      browse: { panel },
    })
    await settleUntil(
      'the Continue row and the grid',
      () =>
        host.querySelectorAll('section.continue > ul > li').length ===
          ENTRIES && host.querySelectorAll('main .grid > li').length === 2,
      2000,
    )
    await document.fonts.ready
    const main = need(host, '.library > main')
    return {
      host,
      main,
      cards: [
        ...host.querySelectorAll<HTMLElement>('section.continue > ul > li'),
      ],
      tiles: [...main.querySelectorAll<HTMLElement>('.grid > li')],
    }
  }

  it.each([1280, 390] as const)(
    'moves no card and no grid tile as a panel runs past the end of a home page that did not scroll, and leaves as much room under the panel as under the last row, at %i px',
    async (width) => {
      onTestFinished(pointerRulesAs('mouse'))
      const { host, main, cards, tiles } = await shortPage(
        width,
        WINDOW[width].short,
        'button',
      )
      const view = scroller(main)
      // The room under the last row, read in the shorter window, where the
      // page scrolls with no panel open.
      expect(view.scrolls(), 'the page in the shorter window').toBe(true)
      view.to(view.end())
      const room0 = view.shown() - Math.max(...tiles.map((t) => rect(t).bottom))
      await page.viewport(width, WINDOW[width].tall)
      view.to(0)
      await afterMotion()
      // The fixture tells: a scrollbar here takes width, and the page does
      // not scroll in the taller window.
      expect({
        bar: scrollbarWidth() > 0,
        scrolls: view.scrolls(),
      }).toEqual({ bar: true, scrolls: false })

      const before = { cards: placesOf(cards), tiles: placesOf(tiles) }
      const panel = await openOn(host, cards[0])
      await afterMotion()
      expect(view.scrolls(), 'the page with the panel open').toBe(true)
      expect(
        { cards: placesOf(cards), tiles: placesOf(tiles) },
        'every card’s and grid tile’s left and width',
      ).toEqual(before)
      view.to(view.end())
      const room1 = view.shown() - rect(panel).bottom
      // Within 1 px: the scroll end is a whole pixel, so each room carries
      // its own rounding of the fractional end of what it measures.
      expect(room0, 'the room under the last row').toBeGreaterThan(0)
      expect(
        Math.abs(room1 - room0),
        `the room under the panel, ${room1}, against the room under the last row, ${room0}`,
      ).toBeLessThanOrEqual(1)
    },
  )

  // The gutter is kept only while a panel can open: with none, a page that
  // does not scroll draws to the window's edge.
  it.each([1280, 390] as const)(
    'keeps no gutter on a home page whose panel is off, at %i px',
    async (width) => {
      const { host, main } = await shortPage(width, WINDOW[width].tall, 'off')
      const view = scroller(main)
      // The fixture tells: no card draws an `i`, and the page does not scroll.
      expect({
        i: host.querySelector('section.continue .panel-open') === null,
        scrolls: view.scrolls(),
      }).toEqual({ i: true, scrolls: false })

      expect({
        main: getComputedStyle(main).scrollbarGutter,
        root: getComputedStyle(document.documentElement).scrollbarGutter,
        taken: view.taken(),
      }).toEqual({ main: 'auto', root: 'auto', taken: 0 })
    },
  )
})
