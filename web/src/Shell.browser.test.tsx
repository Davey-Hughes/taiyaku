import { act } from 'react'
import {
  BrowserRouter,
  MemoryRouter,
  Route,
  Routes,
  useLocation,
} from 'react-router'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { page, userEvent } from 'vitest/browser'
import * as aboutApi from './about'
import { Account } from './Account'
import type { Me } from './api'
import { GateContext } from './gate'
import './index.css'
import * as library from './library'
import { Library } from './Library'
import { Licence } from './Licence'
import { SEARCH_BOX_ID } from './searchHits'
import { Series } from './Series'
import { Shell } from './Shell'
import { ABOUT } from './testing/about'
import { deferred } from './testing/async'
import { licenceTextsLanded } from './testing/licence'
import { focused } from './testing/focused'
import { lengthOf } from './testing/length'
import { mount } from './testing/mount'
import { mouse } from './testing/mouse'
import {
  areaOf,
  coarseRulesOn,
  fingerArea,
  near,
  overlapOf,
  pressArea,
} from './testing/pointer'
import { rootFontAt } from './testing/rootFont'

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

// An administrator's shell reads the open count; answered here, so the
// request never leaves the test.
vi.mock('./curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./curation')>()),
  listProposals: vi.fn(async () => ({
    proposals: [],
    counts: { 'edition-of': 3 },
  })),
}))

// The build's label at the sidebar's foot asks for the build; answered here,
// so the request never leaves the test.
vi.mock('./about', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./about')>()),
  about: vi.fn(),
}))

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

/** Twenty series, so the home screen is taller than a phone. */
const SERIES = Array.from({ length: 20 }, (_, i) =>
  series(i + 1, `Kaguya-sama wa Kokurasetai ${i + 1}`, 10 + i),
)

/** Six half-read volumes with long titles, more than a phone's width holds. */
const CONTINUING = Array.from({ length: 6 }, (_, i) =>
  work(
    i + 1,
    `Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunousen - Full Color Edition v0${i + 1}`,
    [edition('raw', 'Raw')],
  ),
)

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
    ...series(
      7,
      'Go-Toubun_no_Hanayome_Full_Color_Edition_Digital_Kodansha',
      3,
    ),
    titles: [
      {
        title: 'Go-Toubun_no_Hanayome_Full_Color_Edition_Digital_Kodansha',
        kind: 'romaji',
        primary: true,
      },
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
  // The search bar debounces its request by 250 ms (`searchHits.ts`); wait
  // it out with real timers.
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

/** How far the page itself scrolls sideways: zero is the spec's promise. */
const sideways = () =>
  document.documentElement.scrollWidth - document.documentElement.clientWidth

beforeEach(() => {
  localStorage.clear()
  window.scrollTo(0, 0)
  vi.mocked(aboutApi.about).mockResolvedValue(ABOUT)
  vi.mocked(library.status).mockResolvedValue({
    libraries: [{ id: 1, name: 'Volumes', scanned: true }],
    restricted: false,
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

  it('slides the open menu to the left edge above the scrim, and a tap beside it closes it', async () => {
    const host = await at('/', 390)
    const column = need(host, '#library-menu')
    const button = need<HTMLButtonElement>(
      host,
      'button[aria-controls="library-menu"]',
    )
    await act(async () => {
      button.click()
    })
    // Past the slide — the column's transition, however long it takes.
    await act(async () => {
      await Promise.all(column.getAnimations().map((a) => a.finished))
    })
    expect(column.getBoundingClientRect().left).toBe(0)
    expect(column.contains(document.elementFromPoint(100, 400))).toBe(true)
    const close = need(column, '.menu-close')
    const box = close.getBoundingClientRect()
    expect(
      document
        .elementFromPoint(box.left + box.width / 2, box.top + box.height / 2)
        ?.closest('.menu-close'),
    ).toBe(close)
    // Beside the wordmark (spec §7.1): on its row, clear of its text, read
    // from what the wordmark holds.
    const markEl = need(column, '.wordmark')
    const text = document.createRange()
    text.selectNodeContents(markEl)
    const mark = text.getBoundingClientRect()
    expect(box.left).toBeGreaterThan(mark.right)
    // On its row: the button's centre within the wordmark's height.
    const centre = box.top + box.height / 2
    expect(centre).toBeGreaterThan(mark.top)
    expect(centre).toBeLessThan(mark.bottom)
    expect(
      document.elementFromPoint(370, 400)?.classList.contains('scrim'),
    ).toBe(true)
    await act(async () => {
      await userEvent.click(need(host, '.scrim'), {
        position: { x: 370, y: 400 },
      })
    })
    expect(button.getAttribute('aria-expanded')).toBe('false')
    expect(getComputedStyle(document.documentElement).overflowY).not.toBe(
      'hidden',
    )
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
    expect(getComputedStyle(need(host, '.menu-close')).display).toBe('none')
  })
})

describe('the library home at 390 px', () => {
  it('lays out without sideways scroll, with its chips on one line', async () => {
    vi.mocked(library.home).mockResolvedValue({
      continuing: CONTINUING,
      added: [],
      read: [],
    })
    const host = await at('/', 390)
    expect(sideways()).toBe(0)

    // The strip pads itself, so its first cover's hairline is not clipped.
    const strip = need(host, '.continue ul')
    const cover = need(strip, 'img, .cover-missing')
    expect(
      cover.getBoundingClientRect().top - strip.getBoundingClientRect().top,
    ).toBeGreaterThanOrEqual(4)
    expect(
      cover.getBoundingClientRect().left - strip.getBoundingClientRect().left,
    ).toBeGreaterThanOrEqual(4)
    const card = need(strip, 'li').getBoundingClientRect()
    const heading = need(host, '.continue h2').getBoundingClientRect()
    expect(Math.abs(card.left - heading.left)).toBeLessThan(1)
    expect(card.top - heading.bottom).toBeCloseTo(12, 0)

    const tops = [...host.querySelectorAll('.library-chips button')].map(
      (b) => b.getBoundingClientRect().top,
    )
    expect(tops).toHaveLength(4)
    expect(new Set(tops).size).toBe(1)

    // "Added" sorts where the others filter, so it sits at the row's far end.
    const row = need(host, '.library-chips')
    const added = need(row, '.sort')
    expect(
      Math.abs(
        added.getBoundingClientRect().right - row.getBoundingClientRect().right,
      ),
    ).toBeLessThan(1)
  })
})

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
    localStorage.setItem(
      'taiyaku.prefs.browse',
      JSON.stringify({ layout: 'table' }),
    )
    const host = await at('/series/7', 390)
    const box = need(host, '.table-scroll')
    expect(box.scrollWidth).toBeGreaterThan(box.clientWidth)
    expect(sideways()).toBe(0)
  })

  it('lays search out without sideways scroll', async () => {
    const host = await at('/?q=go-toubun', 390)
    expect(host.querySelector('.search-hits .grid')).not.toBeNull()
    expect(sideways()).toBe(0)
  })

  it('lays search out without sideways scroll, in a long-named library chosen in the sidebar', async () => {
    // The sidebar is what narrows a search (spec §2): its library's page is
    // the scope, and the box's placeholder carries the library's whole name.
    const name =
      'Go-Toubun no Hanayome - Full Color Edition [Digital] (Kodansha)'
    vi.mocked(library.status).mockResolvedValue({
      libraries: [
        { id: 1, name: 'Volumes', scanned: true },
        { id: 2, name, scanned: true },
      ],
      restricted: false,
    })
    const host = await at('/', 390)
    await clickWithMouse(need(host, 'button.menu-button'))
    // Past the slide, so the press lands on the link and not where it was.
    const column = need(host, '#library-menu')
    await act(async () => {
      await Promise.all(column.getAnimations().map((a) => a.finished))
    })
    await clickWithMouse(need(column, 'a[href="/?library=2"]'))
    const box = need<HTMLInputElement>(host, 'input[type="search"]')
    expect(box.placeholder).toBe(`Search ${name}`)
    expect(sideways()).toBe(0)
    await act(async () => {
      await userEvent.fill(box, 'go-toubun')
    })
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 300))
    })
    expect(vi.mocked(library.search)).toHaveBeenLastCalledWith('go-toubun', 2)
    expect(host.querySelector('.search-hits .grid')).not.toBeNull()
    expect(sideways()).toBe(0)
  })
})

describe('the library chip row — O156', () => {
  for (const width of [1440, 390]) {
    it(`keeps its four buttons where they were at ${width} px`, async () => {
      const host = await at('/', width)
      const row = need(host, '.library-chips')
      const gap = Number.parseFloat(getComputedStyle(row).columnGap)
      const edges = row.getBoundingClientRect()
      const boxes = [...row.querySelectorAll('button')].map((b) =>
        b.getBoundingClientRect(),
      )
      expect(boxes).toHaveLength(4)
      const [all, multi, unread, added] = boxes
      // One line, the filters a column gap apart from the row's start, and
      // "Added" at the row's far end.
      expect(new Set(boxes.map((b) => Math.round(b.top))).size).toBe(1)
      expect(all.left).toBeCloseTo(edges.left, 0)
      expect(multi.left - all.right).toBeCloseTo(gap, 0)
      expect(unread.left - multi.right).toBeCloseTo(gap, 0)
      expect(added.right).toBeCloseTo(edges.right, 0)
    })
  }
})

describe('the skip link — O174', () => {
  for (const width of [1440, 390]) {
    it(`is the first Tab stop at ${width} px, drawn on screen, and Enter lands in main`, async () => {
      const host = await at('/', width)
      const link = need<HTMLAnchorElement>(host, '.skip-link')
      const main = need(host, 'main')
      // No fragment to start from, whichever case ran before: a link that
      // was followed rather than handled leaves `#content` in the frame's
      // URL, and following it again from there would change nothing.
      history.replaceState(
        null,
        '',
        window.location.pathname + window.location.search,
      )
      // A stop before the shell, so the first real Tab starts inside the
      // test's frame — a key pressed before anything in it has focus goes to
      // the frame around it.
      const before = document.createElement('button')
      before.textContent = 'before'
      document.body.prepend(before)
      try {
        before.focus()
        await userEvent.tab()
        expect(document.activeElement).toBe(link)
        // Out of the grid and the flow (`shell.css`), read here while it
        // has focus.
        expect(getComputedStyle(link).position).toBe('fixed')
        const box = link.getBoundingClientRect()
        expect(box.width).toBeGreaterThan(1)
        expect(box.left).toBeGreaterThanOrEqual(0)
        expect(box.top).toBeGreaterThanOrEqual(0)
        expect(box.right).toBeLessThanOrEqual(window.innerWidth)
        expect(box.bottom).toBeLessThanOrEqual(window.innerHeight)
        // Nothing drawn over it.
        expect(
          document.elementFromPoint(
            box.left + box.width / 2,
            box.top + box.height / 2,
          ),
        ).toBe(link)
        const url = window.location.href
        await act(async () => {
          await userEvent.keyboard('{Enter}')
        })
        expect(document.activeElement).toBe(main)
        expect(getComputedStyle(main).outlineStyle).toBe('none')
        expect(window.location.href).toBe(url)
        await userEvent.tab()
        // The next stop is `main`'s first, and `main` stops being focusable
        // the moment focus leaves it.
        expect(document.activeElement).toBe(
          main.querySelector('a[href], button, input, select, textarea'),
        )
        expect(main.hasAttribute('tabindex')).toBe(false)
      } finally {
        before.remove()
      }
    })
  }

  it('leaves a click on plain text in main where it was, not on main', async () => {
    // `/series/7` shows its title, a heading nobody can press.
    vi.mocked(library.oneSeries).mockResolvedValue(SERIES_PAGE)
    const host = await at('/series/7', 1440)
    const main = need(host, 'main')
    const title = need(host, 'main h1')
    await act(async () => {
      await userEvent.click(title)
    })
    // A focusable `main` would have taken this click's focus, and the next
    // Tab would have started from its top.
    expect(document.activeElement).not.toBe(main)
    expect(main.hasAttribute('tabindex')).toBe(false)
  })

  it('is not drawn until it has focus', async () => {
    const host = await at('/', 1440)
    const box = need(host, '.skip-link').getBoundingClientRect()
    expect(box.width).toBeLessThanOrEqual(1)
    expect(box.height).toBeLessThanOrEqual(1)
  })
})

/**
 * The Library view's heading row as the search box arrives. The box is drawn
 * once the status lands (`Library.tsx`'s `offered`), after the first paint,
 * so a row that grew to take it would move View and push everything under
 * it down on every cold load. A coarse pointer sizes both the box and View
 * differently (`base.css`, `shell.css`), so the row is measured under each.
 */
describe('the Library view’s heading row as the search box arrives', () => {
  // Coarse first at each width: the fine case's pointer check after it is
  // what shows the coarse rules were switched back.
  it.each([
    [1440, 'coarse'],
    [1440, 'fine'],
    [390, 'coarse'],
    [390, 'fine'],
  ] as const)(
    'stays put at %i px with a %s pointer',
    async (width, pointer) => {
      if (pointer === 'coarse') onTestFinished(coarseRulesOn())
      let land: (status: library.LibraryStatus) => void = () => {}
      vi.mocked(library.status).mockReturnValue(
        new Promise((resolve) => {
          land = resolve
        }),
      )
      // The grid never answers: with rows it would bring the box on its own.
      vi.mocked(library.grid).mockReturnValue(new Promise(() => {}))
      const host = await at('/', width)
      // THE POINTER THIS CASE NAMES, read from a coarse-pointer rule that is
      // not View's or the row's: More's 44 px floor.
      expect(getComputedStyle(need(host, '.account-button')).minHeight).toBe(
        pointer === 'coarse' ? '44px' : '0px',
      )
      const measure = () => ({
        head: need(host, '.page-head').getBoundingClientRect().height,
        view: need(host, '.view-button').getBoundingClientRect().top,
        chips: need(host, '.library-chips').getBoundingClientRect().top,
      })
      expect(host.querySelector(`#${SEARCH_BOX_ID}`)).toBeNull()
      // View is a finger's 44 px under a coarse pointer (spec 2026-09-17 §7.1).
      if (pointer === 'coarse') {
        expect(
          need(host, '.view-button').getBoundingClientRect().height,
        ).toBeGreaterThanOrEqual(44)
      }
      const before = measure()
      await act(async () => {
        land({
          libraries: [{ id: 1, name: 'Volumes', scanned: true }],
          restricted: false,
        })
      })
      need(host, `#${SEARCH_BOX_ID}`)
      expect(measure()).toEqual(before)
    },
  )
})

/**
 * The shell, signed in, on a route that matches every path: a link
 * followed inside it lands on a screen that still has the shell, so what
 * the shell does on arrival can be read. `screen` is what it draws in its
 * `<main>`, a line with nothing to press unless a case names a screen.
 */
async function shellAt(
  path: string,
  width: number,
  admin: boolean,
  screen: React.ReactNode = (
    <p className="plain">Nothing here can be pressed.</p>
  ),
): Promise<HTMLElement> {
  await page.viewport(width, 844)
  const who: Me = {
    user_id: 1,
    admin,
    can_download: false,
    can_stream: false,
    can_stamp: false,
  }
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <MemoryRouter initialEntries={[path]}>
        <GateContext.Provider value={{ who, signOut: () => {} }}>
          <Routes>
            <Route path="*" element={<Shell>{screen}</Shell>} />
          </Routes>
        </GateContext.Provider>
      </MemoryRouter>,
    )
    host.style.height = '100%'
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

/**
 * A real press and release at the middle of `el`. Each is its own `act`, so
 * whatever the press changes is drawn before the release lands, as it is
 * for a hand: a panel the press closed is gone from under the click.
 */
async function clickWithMouse(el: Element) {
  const box = el.getBoundingClientRect()
  await mouse.moveTo(box.left + box.width / 2, box.top + box.height / 2)
  await act(async () => {
    await mouse.press()
  })
  await act(async () => {
    await mouse.release()
  })
}

/** The drawer at a phone's width, opened and past its slide. */
async function openDrawer(host: HTMLElement) {
  await clickWithMouse(need(host, 'button.menu-button'))
  const column = need(host, '#library-menu')
  await act(async () => {
    await Promise.all(column.getAnimations().map((a) => a.finished))
  })
}

/** A colour token as the browser resolves it, whichever scheme is in force. */
function tokenIn(host: HTMLElement, token: string): string {
  const probe = document.createElement('span')
  probe.style.color = `var(${token})`
  host.append(probe)
  const colour = getComputedStyle(probe).color
  probe.remove()
  return colour
}

describe('More, with a real mouse and real keys — spec §1', () => {
  const panelOf = (host: HTMLElement) =>
    need(
      host,
      `[id="${need(host, 'button.account-button').getAttribute('aria-controls')}"]`,
    )

  it('opens on a click, and a second click on the button closes it', async () => {
    const host = await shellAt('/', 1440, true)
    const button = need<HTMLButtonElement>(host, 'button.account-button')
    await clickWithMouse(button)
    expect(button.getAttribute('aria-expanded')).toBe('true')
    expect(panelOf(host).checkVisibility()).toBe(true)
    // The press lands on the button, which is outside the panel and inside
    // what the panel belongs to: it must not close the panel only for the
    // click to open it again.
    await clickWithMouse(button)
    expect(button.getAttribute('aria-expanded')).toBe('false')
    expect(panelOf(host).checkVisibility()).toBe(false)
  })

  it('closes on a click outside it, on text nothing can focus', async () => {
    const host = await shellAt('/', 1440, true)
    const button = need<HTMLButtonElement>(host, 'button.account-button')
    await clickWithMouse(button)
    expect(button.getAttribute('aria-expanded')).toBe('true')
    // Plain text, so no focus moves: only the press can close it.
    await clickWithMouse(need(host, 'main .plain'))
    expect(button.getAttribute('aria-expanded')).toBe('false')
    expect(panelOf(host).checkVisibility()).toBe(false)
  })

  it('follows a click on a link inside it, closes, and marks More there', async () => {
    const host = await shellAt('/', 1440, true)
    const button = need<HTMLButtonElement>(host, 'button.account-button')
    await clickWithMouse(button)
    await clickWithMouse(need(panelOf(host), 'a[href="/config"]'))
    expect(button.getAttribute('aria-expanded')).toBe('false')
    expect(panelOf(host).checkVisibility()).toBe(false)
    expect(button.hasAttribute('data-here')).toBe(true)
    expect(getComputedStyle(need(button, '.account-name')).color).toBe(
      tokenIn(host, '--accent'),
    )
  })

  it('opens on Enter with focus left on the button, Tab enters the panel, and Escape there closes it', async () => {
    const host = await shellAt('/', 1440, true)
    const button = need<HTMLButtonElement>(host, 'button.account-button')
    // Inside the frame first: a key pressed before anything here has focus
    // goes to the frame around the test.
    await act(async () => button.focus())
    await act(async () => {
      await userEvent.keyboard('{Enter}')
    })
    expect(button.getAttribute('aria-expanded')).toBe('true')
    expect(document.activeElement).toBe(button)
    // The panel is drawn above the button and follows it in the document,
    // so the next stop is its first link.
    expect(panelOf(host).getBoundingClientRect().bottom).toBeLessThanOrEqual(
      button.getBoundingClientRect().top,
    )
    await act(async () => {
      await userEvent.keyboard('{Tab}')
    })
    expect(document.activeElement).toBe(
      need(panelOf(host), 'a[href="/proposals"]'),
    )
    await act(async () => {
      await userEvent.keyboard('{Escape}')
    })
    expect(button.getAttribute('aria-expanded')).toBe('false')
    expect(panelOf(host).checkVisibility()).toBe(false)
    expect(document.activeElement).toBe(button)
  })

  // Following a link closes the panel, which hides the link that had focus:
  // left there, the next Tab would start again from the top of the document.
  it('gives focus back to the button after Enter on a link inside it', async () => {
    const host = await shellAt('/', 1440, true)
    const button = need<HTMLButtonElement>(host, 'button.account-button')
    // Inside the frame first: a key pressed before anything here has focus
    // goes to the frame around the test.
    await act(async () => button.focus())
    await act(async () => {
      await userEvent.keyboard('{Enter}')
    })
    await act(async () => {
      await userEvent.keyboard('{Tab}')
    })
    await act(async () => {
      await userEvent.keyboard('{Tab}')
    })
    expect(document.activeElement).toBe(
      need(panelOf(host), 'a[href="/config"]'),
    )
    await act(async () => {
      await userEvent.keyboard('{Enter}')
    })
    expect(button.hasAttribute('data-here')).toBe(true)
    expect(button.getAttribute('aria-expanded')).toBe('false')
    expect(document.activeElement).toBe(button)
  })

  // Inside the phone drawer the navigation closes the drawer too, and the
  // drawer's own close sends focus to ☰, which is what opened it.
  it('leaves focus on ☰ after Enter on one of its links inside the drawer', async () => {
    const host = await shellAt('/', 390, true)
    const menu = need<HTMLButtonElement>(
      host,
      'button[aria-controls="library-menu"]',
    )
    await act(async () => menu.click())
    const button = need<HTMLButtonElement>(host, 'button.account-button')
    await act(async () => button.focus())
    await act(async () => {
      await userEvent.keyboard('{Enter}')
    })
    await act(async () => {
      await userEvent.keyboard('{Tab}')
    })
    await act(async () => {
      await userEvent.keyboard('{Tab}')
    })
    expect(document.activeElement).toBe(
      need(panelOf(host), 'a[href="/config"]'),
    )
    await act(async () => {
      await userEvent.keyboard('{Enter}')
    })
    expect(button.hasAttribute('data-here')).toBe(true)
    expect(menu.getAttribute('aria-expanded')).toBe('false')
    expect(document.activeElement).toBe(menu)
  })

  it('closes More and not the drawer on one Escape, and the drawer on the next', async () => {
    // `popover.ts`: one Escape closes the innermost thing open.
    const host = await shellAt('/', 390, true)
    const menu = need<HTMLButtonElement>(
      host,
      'button[aria-controls="library-menu"]',
    )
    await act(async () => menu.click())
    const button = need<HTMLButtonElement>(host, 'button.account-button')
    await act(async () => button.focus())
    await act(async () => {
      await userEvent.keyboard('{Enter}')
    })
    expect(button.getAttribute('aria-expanded')).toBe('true')
    await act(async () => {
      await userEvent.keyboard('{Escape}')
    })
    expect(button.getAttribute('aria-expanded')).toBe('false')
    expect(menu.getAttribute('aria-expanded')).toBe('true')
    expect(document.activeElement).toBe(button)
    await act(async () => {
      await userEvent.keyboard('{Escape}')
    })
    expect(menu.getAttribute('aria-expanded')).toBe('false')
    expect(document.activeElement).toBe(menu)
  })

  /**
   * The panel opens upward over the build's label, which stands above More
   * (the user's ruling of 2026-10-27). Read by what the browser would hit at
   * a point: a panel clipped by the foot or the column, or painted under the
   * label, fails here though its box is where it should be.
   */
  it.each([1280, 390])(
    'draws its panel whole and over the label above it, and Escape or a click outside closes it, at %i px',
    async (width) => {
      const host = await shellAt('/', width, true)
      if (width < 900) await openDrawer(host)
      const button = need<HTMLButtonElement>(host, 'button.account-button')
      const label = need(host, '.version-label a')
      await clickWithMouse(button)
      expect(button.getAttribute('aria-expanded')).toBe('true')
      const panel = panelOf(host)
      const drawn = panel.getBoundingClientRect()
      expect(drawn.bottom).toBeLessThanOrEqual(
        button.getBoundingClientRect().top,
      )
      const hits = (x: number, y: number) =>
        panel.contains(document.elementFromPoint(x, y))
      expect([
        hits(drawn.left + 1, drawn.top + 1),
        hits(drawn.right - 1, drawn.top + 1),
        hits(drawn.left + 1, drawn.bottom - 1),
        hits(drawn.right - 1, drawn.bottom - 1),
      ]).toEqual([true, true, true, true])
      // A full window has room for all of it: nothing inside to scroll.
      expect(panel.scrollHeight).toBe(panel.clientHeight)
      const under = label.getBoundingClientRect()
      expect(
        hits(under.left + under.width / 2, under.top + under.height / 2),
      ).toBe(true)

      await act(async () => {
        await userEvent.keyboard('{Escape}')
      })
      expect(button.getAttribute('aria-expanded')).toBe('false')
      expect(document.activeElement).toBe(button)

      await clickWithMouse(button)
      expect(button.getAttribute('aria-expanded')).toBe('true')
      // Theme's legend: text nothing can focus, in the column at both widths.
      await clickWithMouse(need(host, '.theme-control legend'))
      expect(button.getAttribute('aria-expanded')).toBe('false')
      expect(panelOf(host).checkVisibility()).toBe(false)
    },
  )

  /**
   * A window too short for the panel above More, as a phone held on its
   * side can be: the column clips the panel at its top, where no
   * scroll reaches. So the panel is held to the room above More and scrolls
   * inside it, using all of that room, and every entry can be brought into
   * view and pressed, while More stays clear of it. The same where the room
   * changes while it is open: a window that shrinks, as a phone turned on
   * its side does; text made larger, which raises More; or a pointer that
   * turns coarse, which raises More by growing the foot alone. A coarse
   * pointer makes the rows, and so the panel, their tallest.
   */
  it.each([
    ['opened in it', 'opened'],
    ['shrunk to while it is open', 'shrunk'],
    ['with the text made larger while it is open', 'larger text'],
    ['with the pointer turned coarse while it is open', 'coarse pointer'],
  ] as const)(
    'holds its panel inside a short window %s, every entry within reach, in the drawer with a coarse pointer',
    async (_, form) => {
      if (form !== 'coarse pointer') onTestFinished(coarseRulesOn())
      const host = await shellAt('/', 390, true)
      if (form !== 'shrunk') await page.viewport(390, 280)
      await openDrawer(host)
      const button = need<HTMLButtonElement>(host, 'button.account-button')
      await clickWithMouse(button)
      if (form === 'shrunk') {
        await act(async () => {
          await page.viewport(390, 280)
        })
      }
      if (form === 'larger text') rootFontAt('20px')
      if (form === 'coarse pointer') onTestFinished(coarseRulesOn())
      // Two frames: a change of size is answered in the frame after it.
      await aFrame()
      await aFrame()
      expect(button.getAttribute('aria-expanded')).toBe('true')
      const panel = panelOf(host)
      const drawn = panel.getBoundingClientRect()
      const more = button.getBoundingClientRect()
      // Held to the column's top, past its padding, and no lower: it uses
      // the room it has, and what does not fit scrolls.
      const column = need(host, '#library-menu')
      expect(drawn.top).toBe(
        column.getBoundingClientRect().top +
          Number.parseFloat(getComputedStyle(column).paddingTop),
      )
      expect(panel.scrollHeight).toBeGreaterThan(panel.clientHeight)
      expect(drawn.bottom).toBeLessThanOrEqual(more.top)
      expect(
        button.contains(
          document.elementFromPoint(
            more.left + more.width / 2,
            more.top + more.height / 2,
          ),
        ),
      ).toBe(true)
      const links = [...panel.querySelectorAll('a')]
      expect(links).toHaveLength(3)
      const reached = links.map((link) => {
        link.scrollIntoView({ block: 'nearest' })
        const box = link.getBoundingClientRect()
        return link.contains(
          document.elementFromPoint(
            box.left + box.width / 2,
            box.top + box.height / 2,
          ),
        )
      })
      expect(reached).toEqual([true, true, true])
      await clickWithMouse(need(panel, 'a[href="/account"]'))
      expect(button.hasAttribute('data-here')).toBe(true)
    },
  )

  /**
   * Tab and Shift+Tab scroll a short window's panel to the entry they reach,
   * and each is drawn with its whole focus ring: the panel's edges cut none
   * of it, at the foot of the list or at its head.
   */
  it('draws each entry’s whole focus ring as Tab and Shift+Tab scroll a short window’s panel to it, in the drawer with a coarse pointer', async () => {
    onTestFinished(coarseRulesOn())
    const host = await shellAt('/', 390, true)
    await page.viewport(390, 280)
    await openDrawer(host)
    const button = need<HTMLButtonElement>(host, 'button.account-button')
    // Inside the frame first: a key pressed before anything here has focus
    // goes to the frame around the test.
    await act(async () => button.focus())
    await act(async () => {
      await userEvent.keyboard('{Enter}')
    })
    const panel = panelOf(host)
    expect(panel.scrollHeight).toBeGreaterThan(panel.clientHeight)
    /** Where focus is, and whether the panel's scrollport cuts its ring. */
    const ring = () => {
      const at = document.activeElement
      if (!(at instanceof HTMLElement) || !panel.contains(at)) {
        return { at: at?.outerHTML.slice(0, 60) ?? 'nothing', cut: true }
      }
      const style = getComputedStyle(at)
      expect(style.outlineStyle).toBe('solid')
      const reach =
        Number.parseFloat(style.outlineOffset) +
        Number.parseFloat(style.outlineWidth)
      const box = at.getBoundingClientRect()
      const drawn = panel.getBoundingClientRect()
      const left = drawn.left + panel.clientLeft
      const top = drawn.top + panel.clientTop
      return {
        at: at.getAttribute('href'),
        cut:
          box.left - reach < left ||
          box.top - reach < top ||
          box.right + reach > left + panel.clientWidth ||
          box.bottom + reach > top + panel.clientHeight,
      }
    }
    const seen = []
    for (const shift of [false, false, false, true, true]) {
      await act(async () => {
        await userEvent.tab({ shift })
      })
      seen.push(ring())
    }
    expect(seen).toEqual([
      { at: '/proposals', cut: false },
      { at: '/config', cut: false },
      { at: '/account', cut: false },
      { at: '/config', cut: false },
      { at: '/proposals', cut: false },
    ])
  })

  it.each([
    ['/proposals', true],
    ['/config', true],
    ['/config/libraries', true],
    ['/account', true],
    ['/', false],
    ['/?library=1', false],
    ['/series/3', false],
  ])(
    'draws an administrator’s More in the accent on %s: %s',
    async (path, accent) => {
      const host = await shellAt(path, 1440, true)
      const name = getComputedStyle(
        need(host, 'button.account-button .account-name'),
      )
      expect(name.color === tokenIn(host, '--accent')).toBe(accent)
    },
  )

  it.each([
    ['/account', true],
    ['/', false],
    ['/proposals', true],
    ['/config', true],
  ])(
    'draws anyone else’s More in the accent on %s: %s',
    async (path, accent) => {
      const host = await shellAt(path, 1440, false)
      const name = getComputedStyle(
        need(host, 'a.account-button .account-name'),
      )
      expect(name.color === tokenIn(host, '--accent')).toBe(accent)
    },
  )
})

describe('View, with a real mouse and real keys — spec §2', () => {
  beforeEach(() => {
    vi.mocked(library.oneSeries).mockResolvedValue(SERIES_PAGE)
  })

  const panelOf = (host: HTMLElement) =>
    need(
      host,
      `[id="${need(host, 'button.view-button').getAttribute('aria-controls')}"]`,
    )

  it('opens on Enter with focus left on the button, Tab enters the panel, and Escape closes it from there or from the button', async () => {
    const host = await at('/series/7', 1440)
    const button = need<HTMLButtonElement>(host, 'button.view-button')
    // Inside the frame first: a key pressed before anything here has focus
    // goes to the frame around the test.
    await act(async () => button.focus())
    await act(async () => {
      await userEvent.keyboard('{Enter}')
    })
    expect(button.getAttribute('aria-expanded')).toBe('true')
    expect(document.activeElement).toBe(button)
    // The panel is drawn below the button and follows it in the document,
    // so the next stop is its first control, Layout.
    expect(panelOf(host).getBoundingClientRect().top).toBeGreaterThanOrEqual(
      button.getBoundingClientRect().bottom,
    )
    await act(async () => {
      await userEvent.keyboard('{Tab}')
    })
    expect(document.activeElement).toBe(need(panelOf(host), 'select'))
    await act(async () => {
      await userEvent.keyboard('{Escape}')
    })
    expect(button.getAttribute('aria-expanded')).toBe('false')
    expect(panelOf(host).checkVisibility()).toBe(false)
    expect(document.activeElement).toBe(button)
    await act(async () => {
      await userEvent.keyboard('{Enter}')
    })
    expect(button.getAttribute('aria-expanded')).toBe('true')
    await act(async () => {
      await userEvent.keyboard('{Escape}')
    })
    expect(button.getAttribute('aria-expanded')).toBe('false')
    expect(document.activeElement).toBe(button)
  })

  it('closes on a click outside it, on text nothing can focus, and not on one inside it', async () => {
    const host = await at('/series/7', 1440)
    const button = need<HTMLButtonElement>(host, 'button.view-button')
    await clickWithMouse(button)
    expect(button.getAttribute('aria-expanded')).toBe('true')
    // The panel's own padding, where no control is: inside, so it stays.
    const box = panelOf(host).getBoundingClientRect()
    await mouse.moveTo(box.left + 4, box.top + 4)
    await act(async () => {
      await mouse.press()
    })
    await act(async () => {
      await mouse.release()
    })
    expect(button.getAttribute('aria-expanded')).toBe('true')
    expect(panelOf(host).checkVisibility()).toBe(true)
    // The series title, plain text, so no focus moves: only the press can
    // close it.
    await clickWithMouse(need(host, '.page-head h1'))
    expect(button.getAttribute('aria-expanded')).toBe('false')
    expect(panelOf(host).checkVisibility()).toBe(false)
  })
})

/**
 * The build's label above More — spec §4. `VersionLabel.test.tsx` pins what
 * it says and where it is current; this is what a browser draws of that:
 * its colour, the line it holds before the answer, and a real press on it
 * inside the drawer.
 */
describe('the build’s label', () => {
  it('is followed from the drawer with a real press, which closes it as a library link does', async () => {
    const host = await shellAt('/', 390, false)
    const menu = need<HTMLButtonElement>(host, 'button.menu-button')
    await clickWithMouse(menu)
    // Past the slide, so the press lands on the link and not where it was.
    const column = need(host, '#library-menu')
    await act(async () => {
      await Promise.all(column.getAnimations().map((a) => a.finished))
    })
    const label = need<HTMLAnchorElement>(column, '.version-label a')
    expect(label.checkVisibility()).toBe(true)
    await clickWithMouse(label)
    expect(label.getAttribute('aria-current')).toBe('page')
    expect(menu.getAttribute('aria-expanded')).toBe('false')
  })

  it.each([
    ['/about', true],
    ['/about/licence', true],
    ['/', false],
    ['/account', false],
  ])('draws the label in the accent on %s: %s', async (path, accent) => {
    const host = await shellAt(path, 1440, false)
    const label = getComputedStyle(need(host, '.version-label a'))
    expect(label.color === tokenIn(host, '--accent')).toBe(accent)
  })

  it('is muted text at the 14px floor where it is not current', async () => {
    const host = await shellAt('/', 1440, false)
    const label = getComputedStyle(need(host, '.version-label a'))
    expect(label.color).toBe(tokenIn(host, '--muted'))
    expect(label.fontSize).toBe('14px')
  })

  // Coarse first at each width: the fine case's pointer check after it is
  // what shows the coarse rules were switched back.
  it.each([
    [1440, 'coarse'],
    [1440, 'fine'],
    [390, 'coarse'],
  ] as const)(
    'holds its line at %i px with a %s pointer, so the foot does not move as the answer lands',
    async (width, pointer) => {
      if (pointer === 'coarse') onTestFinished(coarseRulesOn())
      const answer = deferred<aboutApi.About>()
      vi.mocked(aboutApi.about).mockReturnValue(answer.promise)
      const host = await shellAt('/', width, false)
      // THE POINTER THIS CASE NAMES, read from a coarse-pointer rule that is
      // not the label's: More's 44 px floor.
      expect(getComputedStyle(need(host, '.account-button')).minHeight).toBe(
        pointer === 'coarse' ? '44px' : '0px',
      )
      if (width < 900) {
        await clickWithMouse(need(host, 'button.menu-button'))
        const column = need(host, '#library-menu')
        await act(async () => {
          await Promise.all(column.getAnimations().map((a) => a.finished))
        })
      }
      const measure = () => ({
        foot: need(host, '.sidebar-foot').getBoundingClientRect().height,
        more: need(host, '.account-button').getBoundingClientRect().top,
      })
      expect(host.querySelector('.version-label a')).toBeNull()
      const before = measure()
      await act(async () => {
        answer.resolve(ABOUT)
      })
      const link = need(host, '.version-label a')
      expect(measure()).toEqual(before)
      // A finger's row under a coarse pointer (spec 2026-09-17 §7.1).
      if (pointer === 'coarse') {
        expect(link.getBoundingClientRect().height).toBeGreaterThanOrEqual(44)
      }
    },
  )
})

/**
 * The sidebar's foot reads the build's label, then More, then Theme (the
 * user's ruling of 2026-10-27). A browser draws a hovered link's address
 * over the window's bottom-left corner, so Theme, which holds no link, takes
 * that corner, and the label and More stand above it. In both of More's
 * shapes: an administrator's button and anyone else's link.
 */
describe('the sidebar’s foot', () => {
  const footOf = (host: HTMLElement) => {
    const foot = need(host, '#library-menu .sidebar-foot')
    return {
      foot,
      label: need(foot, '.version-label'),
      more: need(foot, '.account-button'),
      theme: need(foot, '.theme-control'),
    }
  }

  const SHAPES = [
    [1280, true],
    [1280, false],
    [390, true],
    [390, false],
  ] as const

  it.each(SHAPES)(
    'draws the label, More, then Theme in the window’s bottom corner, at %i px (admin: %s)',
    async (width, admin) => {
      const host = await shellAt('/', width, admin)
      if (width < 900) await openDrawer(host)
      const { foot, label, more, theme } = footOf(host)
      expect(need(label, 'a').checkVisibility()).toBe(true)
      const box = (el: Element) => el.getBoundingClientRect()
      expect(box(label).bottom).toBeLessThanOrEqual(box(more).top)
      expect(box(more).bottom).toBeLessThanOrEqual(box(theme).top)
      const padding = Number.parseFloat(getComputedStyle(foot).paddingBottom)
      expect(box(theme).bottom).toBe(box(foot).bottom - padding)
      expect(box(foot).bottom).toBe(window.innerHeight)
    },
  )

  it.each(SHAPES)(
    'takes Tab from the navigation to the label, More, then Theme, at %i px (admin: %s)',
    async (width, admin) => {
      const host = await shellAt('/', width, admin)
      if (width < 900) await openDrawer(host)
      const { label, more, theme } = footOf(host)
      const stops = new Map<Element, string>([
        [need(label, 'a'), 'the label'],
        [more, 'More'],
        [need(theme, 'input:checked'), 'Theme'],
      ])
      const links = need(host, 'nav.sidebar').querySelectorAll('a[href]')
      const last = links[links.length - 1]
      if (!(last instanceof HTMLElement)) throw new Error('no sidebar link')
      // Inside the frame first: a key pressed before anything here has focus
      // goes to the frame around the test.
      await act(async () => last.focus())
      const reached: string[] = []
      for (let i = 0; i < 3; i++) {
        await userEvent.tab()
        const at = document.activeElement
        reached.push(
          (at && stops.get(at)) ?? at?.outerHTML.slice(0, 60) ?? 'nothing',
        )
      }
      expect(reached).toEqual(['the label', 'More', 'Theme'])
    },
  )
})

/**
 * **A screen with the catalog's address does not scroll the document at a
 * desktop width**, where `<main>` is what scrolls. The address's live region
 * is out of the flow, so it is placed in the box of its nearest positioned
 * ancestor, and where no ancestor is positioned that is the window's: a row
 * lower in `<main>`'s content than the window is tall then stretches the
 * document past the window. The account screen draws its row lower than
 * About does, under the password form, and the window here is shorter than
 * that, which the case checks first.
 */
describe('a screen with the catalog’s address, at a desktop width', () => {
  it('leaves the document unscrolled on the account screen in a short window', async () => {
    // The account screen's lists, answered empty.
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => Response.json([])),
    )
    onTestFinished(() => {
      vi.unstubAllGlobals()
    })
    const host = await shellAt(
      '/account',
      1280,
      false,
      <Account placement="page" onSignedOut={() => {}} />,
    )
    await page.viewport(1280, 300)
    await act(async () => {
      await new Promise((resolve) => {
        setTimeout(resolve, 0)
      })
    })
    const row = need(host, '.catalog-address')
    const main = need(host, 'main')
    expect(need(row, '[role="status"]').className).toBe('visually-hidden')
    expect(
      row.getBoundingClientRect().top -
        main.getBoundingClientRect().top +
        main.scrollTop,
    ).toBeGreaterThan(innerHeight)
    expect(document.documentElement.scrollHeight).toBeLessThanOrEqual(
      innerHeight,
    )
  })
})

/**
 * **The licence page's way up to About is a finger's 44 px under a coarse
 * pointer in the shell itself** (spec 2026-09-17 §7.1), where it sits just
 * under the phone bar. The bar is drawn over `<main>`, so where the two
 * meet the bar takes the press. Its target is its box grown about its
 * middle to 44 px (`fingerArea`), and that area shares nothing with the
 * bar's box, so none of it is under the bar; hit testing along its middle
 * row and column (`pressArea`) finds that whole area pressable.
 */
describe('the licence page’s way up, under the phone bar', () => {
  it('is a finger’s 44 px with a coarse pointer, none of it under the bar', async () => {
    onTestFinished(coarseRulesOn())
    const host = await shellAt('/about/licence', 390, false, <Licence />)
    await licenceTextsLanded()
    await document.fonts.ready
    const back = need(host, '.back a')
    const target = fingerArea(back)
    const bar = areaOf(need(host, '.phone-bar').getBoundingClientRect())
    expect(overlapOf(target, bar)).toBe(0)
    expect(pressArea(back)).toEqual(near(target))
  })
})

/**
 * The phone bar's wordmark is the way to `/` too, named as the sidebar's
 * is (`the sidebar’s wordmark`), and a finger's 44 px with a coarse
 * pointer, inside the bar and over neither ☰ nor Search beside it.
 */
describe('the phone bar’s wordmark', () => {
  it('is a link to /, named “taiyaku, Libraries”, as the sidebar’s wordmark is', async () => {
    const host = await shellAt('/config/rules', 390, true)
    const home = need(host, '.phone-bar a:has(.wordmark)')
    expect(home.getAttribute('href')).toBe('/')
    expect(
      page
        .getByRole('link', { name: 'taiyaku, Libraries', exact: true })
        .elements(),
    ).toEqual([home])
  })

  it('is a finger’s 44 px with a coarse pointer, inside the bar and over neither ☰ nor Search', async () => {
    onTestFinished(coarseRulesOn())
    const host = await shellAt('/config/rules', 390, true)
    await document.fonts.ready
    const home = need(host, '.phone-bar a:has(.wordmark)')
    const target = fingerArea(home)
    expect(pressArea(home)).toEqual(near(target))
    const bar = areaOf(need(host, '.phone-bar').getBoundingClientRect())
    expect(overlapOf(target, bar)).toBeCloseTo(target.width * target.height, 1)
    for (const beside of ['.menu-button', '.search']) {
      const drawn = areaOf(need(host, beside).getBoundingClientRect())
      expect(overlapOf(target, drawn), beside).toBe(0)
    }
  })
})

/**
 * The fragment the router holds, drawn where a test can read it. Declared
 * inside a function, for the reason `Gate.test.tsx`'s `probe` gives.
 */
function routerFragment() {
  function RouterFragment() {
    return <output data-fragment={useLocation().hash} />
  }
  return <RouterFragment />
}

/**
 * The licence page in the shell, signed in, under the router the app runs,
 * `BrowserRouter`, at this frame's own address with no fragment, which
 * `path="*"` matches and which is put back when the test ends. The texts
 * and the faces have landed, and focus is on the way up, so a click that
 * follows is not the frame's first input. `routerFragment()` draws the
 * router's fragment beside the page.
 */
async function licenceInShell(width: number): Promise<HTMLElement> {
  const before = location.href
  onTestFinished(() => {
    history.replaceState(null, '', before)
  })
  history.replaceState(null, '', `${location.pathname}${location.search}`)
  await page.viewport(width, 844)
  const who: Me = {
    user_id: 1,
    admin: false,
    can_download: false,
    can_stream: false,
    can_stamp: false,
  }
  const host = mount(
    <BrowserRouter>
      <GateContext.Provider value={{ who, signOut: () => {} }}>
        <Routes>
          <Route
            path="*"
            element={
              <Shell>
                <Licence />
                {routerFragment()}
              </Shell>
            }
          />
        </Routes>
      </GateContext.Provider>
    </BrowserRouter>,
  )
  host.style.height = '100%'
  await licenceTextsLanded()
  await document.fonts.ready
  act(() => {
    need(host, '.back a').focus()
  })
  return host
}

/** A frame, inside `act`, for whatever the router's render of a new location runs. */
async function aFrame() {
  await act(async () => {
    await new Promise((resolve) => requestAnimationFrame(resolve))
  })
}

/** A real click on the contents link to `fragment`, once its heading has focus. */
async function follow(host: HTMLElement, fragment: string) {
  const link = need(host, `nav[aria-label="Contents"] a[href="${fragment}"]`)
  await act(async () => {
    await userEvent.click(link)
  })
  await expect.poll(() => document.activeElement).toBe(need(host, fragment))
  await aFrame()
}

/**
 * Back `entries` entries, one as the browser's own button goes and more as
 * its history menu jumps, with the router's render of it held a task so it
 * runs after the browser has put back the scroll it keeps for the entry. In
 * the app that order is the scheduler's to choose; this forces the order in
 * which the browser's restore would undo the page's landing.
 */
async function back(entries = 1) {
  await act(async () => {
    const popped = new Promise((resolve) => {
      window.addEventListener('popstate', resolve, { once: true })
    })
    history.go(-entries)
    await popped
    await new Promise((resolve) => setTimeout(resolve, 0))
  })
  await aFrame()
}

/**
 * The part of the window the page is seen through: `<main>`, which scrolls
 * at a desktop width, below the phone bar, which covers the window's top at
 * a phone's width, where the document scrolls instead.
 */
function viewOf(host: HTMLElement) {
  const main = need(host, 'main').getBoundingClientRect()
  return {
    top: Math.max(
      main.top,
      need(host, '.phone-bar').getBoundingClientRect().bottom,
    ),
    bottom: Math.min(main.bottom, innerHeight),
  }
}

/** Whether all of `el` is in the view. */
function inView(host: HTMLElement, el: Element) {
  const view = viewOf(host)
  const box = el.getBoundingClientRect()
  return box.top >= view.top && box.bottom <= view.bottom
}

/** How far each of the two scrollers is scrolled: `<main>`, and the document. */
const scrolled = (host: HTMLElement) => [
  need(host, 'main').scrollTop,
  Math.round(scrollY),
]

/**
 * **A contents link followed at a phone's width lands its licence's heading
 * below the phone bar**, which covers the top of the view there: the
 * heading's top edge, the top of its section, is not under the bar, and a
 * press at the middle of the heading lands on it, so the bar is not drawn
 * over it. The router hears the fragment change, and focus is on the
 * heading a frame after it has drawn the new location. Each link is clicked
 * from wherever the last one left the view.
 */
describe('the licence page’s contents, under the phone bar', () => {
  it('land each licence’s heading below the bar, focus on it', async () => {
    const host = await licenceInShell(390)
    const landed = []
    for (const link of host.querySelectorAll('nav[aria-label="Contents"] a')) {
      await act(async () => {
        await userEvent.click(link)
      })
      const heading = need(host, link.getAttribute('href') ?? '')
      await expect.poll(() => document.activeElement).toBe(heading)
      await aFrame()
      const bar = need(host, '.phone-bar').getBoundingClientRect()
      const middle = heading.getBoundingClientRect()
      const hit = document.elementFromPoint(
        middle.left + middle.width / 2,
        middle.top + middle.height / 2,
      )
      landed.push([
        heading.textContent,
        need(host, '[data-fragment]').dataset.fragment,
        document.activeElement === heading,
        middle.top >= bar.bottom,
        hit !== null && heading.contains(hit),
      ])
    }
    expect(landed).toEqual([
      ['taiyaku', '#taiyaku', true, true, true],
      ['Shippori Mincho B1', '#shippori-mincho-b1', true, true, true],
      ['Zen Kaku Gothic New', '#zen-kaku-gothic-new', true, true, true],
      ['taiyaku marks', '#taiyaku-marks', true, true, true],
    ])
  })
})

/**
 * **Back to top, on the heading's line or at the box's foot, pressed at a
 * phone's width, lands the page's top below the phone bar**: both scrollers
 * at their tops, the title's top edge not under the bar, a press at the
 * middle of the title landing on it, and focus on it, with the fragment gone
 * from the router's address. Pressed from a licence a contents link has
 * landed, so the view starts far down the page.
 */
describe('the licence page’s Back to top, under the phone bar', () => {
  it.each([
    ['heading', ':scope > .settings-section-head'],
    ['foot', ':scope > .settings-box'],
  ] as const)(
    'at the %s, lands the page’s top below the bar, focus on its title',
    async (_, part) => {
      const host = await licenceInShell(390)
      await follow(host, '#zen-kaku-gothic-new')
      expect(scrolled(host)).not.toEqual([0, 0])
      const section = need(host, '#zen-kaku-gothic-new').closest(
        '.settings-section',
      )
      const links = [
        ...(section?.querySelector(part)?.querySelectorAll('a') ?? []),
      ].filter((each) => each.textContent?.endsWith('Back to top'))
      expect(links).toHaveLength(1)
      const link = links[0]
      if (!link) throw new Error('no Back to top')
      await act(async () => {
        await userEvent.click(link)
      })
      await aFrame()
      const title = need(host, 'h1')
      const bar = need(host, '.phone-bar').getBoundingClientRect()
      const box = title.getBoundingClientRect()
      const hit = document.elementFromPoint(
        box.left + box.width / 2,
        box.top + box.height / 2,
      )
      expect({
        fragment: need(host, '[data-fragment]').dataset.fragment,
        scrolled: scrolled(host),
        focus: document.activeElement === title,
        belowBar: box.top >= bar.bottom,
        hit: hit !== null && title.contains(hit),
      }).toEqual({
        fragment: '',
        scrolled: [0, 0],
        focus: true,
        belowBar: true,
        hit: true,
      })
    },
  )
})

/**
 * **Back puts the view where the address it goes back to says**, at a
 * desktop width, where `<main>` scrolls and the browser keeps no scroll of
 * its own for it, and at a phone's, where the document scrolls. To the
 * page's own address, with no fragment: the page's top, the contents in
 * view, focus left where the link put it. To another licence's fragment:
 * that licence, landed as following its link lands it, focus on its
 * heading.
 */
describe('the licence page’s contents, and Back', () => {
  it.each([1280, 390])(
    'at %i px, Back from a licence to the page’s own address puts the page’s top in view',
    async (width) => {
      const host = await licenceInShell(width)
      await follow(host, '#zen-kaku-gothic-new')
      expect(scrolled(host)).not.toEqual([0, 0])
      await back()
      expect({
        fragment: need(host, '[data-fragment]').dataset.fragment,
        scrolled: scrolled(host),
        contents: inView(host, need(host, 'nav[aria-label="Contents"]')),
        focus: focused(),
      }).toEqual({
        fragment: '',
        scrolled: [0, 0],
        contents: true,
        focus: 'Zen Kaku Gothic New',
      })
    },
  )

  it.each([1280, 390])(
    'at %i px, Back from one licence to another lands on that licence, focus on its heading',
    async (width) => {
      const host = await licenceInShell(width)
      await follow(host, '#shippori-mincho-b1')
      await follow(host, '#zen-kaku-gothic-new')
      await back()
      const heading = need(host, '#shippori-mincho-b1')
      const section = heading.closest('.settings-section')
      if (!section) throw new Error('no section')
      expect({
        fragment: need(host, '[data-fragment]').dataset.fragment,
        focus: focused(),
        heading: Math.round(
          heading.getBoundingClientRect().top -
            Number.parseFloat(getComputedStyle(heading).scrollMarginTop),
        ),
        section: section.getBoundingClientRect().top >= viewOf(host).top,
      }).toEqual({
        fragment: '#shippori-mincho-b1',
        focus: 'Shippori Mincho B1',
        heading: 0,
        section: true,
      })
    },
  )

  // Two entries of one address, two apart: the router hands back the
  // location it already had, so only the browser's `popstate` says the
  // history moved. The reader has scrolled on into the GPL's text first,
  // and focus is moved off the heading, so a landing has to put it back.
  it.each([1280, 390])(
    'at %i px, Back two entries from one licence’s fragment to the same fragment lands on that licence again',
    async (width) => {
      const host = await licenceInShell(width)
      await follow(host, '#taiyaku')
      await follow(host, '#shippori-mincho-b1')
      await follow(host, '#taiyaku')
      const heading = need(host, '#taiyaku')
      act(() => {
        need(host, '.back a').focus({ preventScroll: true })
        heading.closest('.settings-section')?.scrollIntoView({ block: 'end' })
      })
      expect({ focus: focused(), heading: inView(host, heading) }).toEqual({
        focus: '‹ About',
        heading: false,
      })
      await back(2)
      expect({
        fragment: need(host, '[data-fragment]').dataset.fragment,
        focus: focused(),
        heading: Math.round(
          heading.getBoundingClientRect().top -
            Number.parseFloat(getComputedStyle(heading).scrollMarginTop),
        ),
      }).toEqual({
        fragment: '#taiyaku',
        focus: 'taiyaku',
        heading: 0,
      })
    },
  )

  // Under React's own scheduler rather than `act`, which draws every update
  // it holds at once: the page's count of the history's moves is never drawn
  // beside the location the history has left, so Back lands on the licence
  // it goes to and never first on the licence it leaves. Focus is moved off
  // that licence's heading first, so a landing on it would be heard.
  it('at 1280 px, Back under React’s own scheduler lands only on the licence it goes to', async () => {
    const host = await licenceInShell(1280)
    await follow(host, '#shippori-mincho-b1')
    await follow(host, '#zen-kaku-gothic-new')
    act(() => {
      need(host, '.back a').focus()
    })
    const landed: string[] = []
    host.addEventListener('focusin', (event) => {
      if (event.target instanceof HTMLElement) landed.push(event.target.id)
    })
    globalThis.IS_REACT_ACT_ENVIRONMENT = false
    try {
      const popped = new Promise((resolve) => {
        window.addEventListener('popstate', resolve, { once: true })
      })
      history.back()
      await popped
      await new Promise((resolve) => requestAnimationFrame(resolve))
    } finally {
      globalThis.IS_REACT_ACT_ENVIRONMENT = true
    }
    expect(landed).toEqual(['shippori-mincho-b1'])
  })
})

/**
 * The sidebar's wordmark is the way to `/`, as its Libraries heading is
 * (the user's ruling of 2026-10-26): a link named for both, drawn at rest
 * as the plain wordmark it was, with the sidebar links' ring and hover.
 * Login's wordmark stays a heading's words (`Login.test.tsx`).
 */
describe('the sidebar’s wordmark', () => {
  /** The column's wordmark, and the link it is drawn in. */
  const wordmarkOf = (host: HTMLElement) => {
    const mark = need(host, '#library-menu .wordmark')
    const link = mark.closest('a')
    if (!link) throw new Error('the sidebar’s wordmark is no link')
    return { mark, link, name: need(mark, '.wordmark-name') }
  }

  it('is a link to /, where the Libraries heading goes, named “taiyaku, Libraries”', async () => {
    const host = await shellAt('/config/rules', 1440, true)
    const { link } = wordmarkOf(host)
    expect(link.getAttribute('href')).toBe('/')
    expect(link.getAttribute('href')).toBe(
      need(host, '.sidebar-heading a').getAttribute('href'),
    )
    expect(
      page
        .getByRole('link', { name: 'taiyaku, Libraries', exact: true })
        .elements(),
    ).toEqual([link])
  })

  it('goes to / on a real press', async () => {
    const host = await shellAt('/config/rules', 1440, true)
    const heading = need(host, '.sidebar-heading a')
    expect(heading.getAttribute('aria-current')).toBeNull()
    await clickWithMouse(wordmarkOf(host).link)
    expect(heading.getAttribute('aria-current')).toBe('page')
  })

  it('is drawn at rest as the plain wordmark, and takes the sidebar links’ hover and focus ring', async () => {
    const host = await shellAt('/config/rules', 1440, true)
    const { link, name } = wordmarkOf(host)
    const library = need(host, '.sidebar-libraries a')
    const column = need(host, '#library-menu')
    expect(getComputedStyle(link).color).toBe(getComputedStyle(column).color)
    expect(
      [link, name].map((el) => getComputedStyle(el).textDecorationLine),
    ).toEqual(['none', 'none'])

    const hovered = async (el: Element, drawn: Element) => {
      const box = el.getBoundingClientRect()
      await mouse.moveTo(box.left + box.width / 2, box.top + box.height / 2)
      return getComputedStyle(drawn).textDecorationLine
    }
    expect(await hovered(library, library)).toBe('underline')
    expect(await hovered(link, name)).toBe('underline')

    // Reached by Tab, so each is `:focus-visible` as a person's Tab makes it.
    const ring = async (el: Element) => {
      act(() => {
        need(host, '.skip-link').focus()
      })
      for (let i = 0; i < 8 && document.activeElement !== el; i++) {
        await userEvent.tab()
      }
      expect(document.activeElement).toBe(el)
      const style = getComputedStyle(el)
      return [
        style.outlineStyle,
        style.outlineWidth,
        style.outlineColor,
        style.outlineOffset,
      ]
    }
    const libraryRing = await ring(library)
    expect(libraryRing[0]).toBe('solid')
    expect(await ring(link)).toEqual(libraryRing)
  })

  /**
   * A finger's 44 px with a coarse pointer, as every link in the column
   * is, though the target grows and not the box (`shell.css`): drawn at
   * 44, the link would push the column's links down under it. The target
   * covers none of them, nor Close beside it in the drawer.
   */
  it.each([1440, 390])(
    'is a finger’s 44 px with a coarse pointer, drawn where it stood and over no other control, at %i px',
    async (width) => {
      onTestFinished(coarseRulesOn())
      const host = await shellAt('/config/rules', width, true)
      if (width < 900) await openDrawer(host)
      await document.fonts.ready
      const { mark, link } = wordmarkOf(host)
      const target = fingerArea(link)
      expect(pressArea(link)).toEqual(near(target))
      expect(link.getBoundingClientRect().toJSON()).toEqual(
        mark.getBoundingClientRect().toJSON(),
      )
      expect(need(host, 'nav.sidebar').getBoundingClientRect().top).toBe(
        link.getBoundingClientRect().bottom,
      )
      const others = [
        ...need(host, '#library-menu').querySelectorAll('a[href], button'),
      ].filter((other) => other !== link && other.checkVisibility())
      expect(others.length).toBeGreaterThan(1)
      expect(
        others
          .filter(
            (other) =>
              overlapOf(target, areaOf(other.getBoundingClientRect())) > 0,
          )
          .map((other) => other.outerHTML.slice(0, 60)),
      ).toEqual([])
    },
  )

  /**
   * Opening the drawer gives focus to the Libraries heading's link, the
   * first of the navigation's, and not to the wordmark's above it, which
   * leads to the same place: Close, beside the wordmark, is one Shift+Tab
   * back (spec 2026-09-17 §7.1), and the wordmark one more.
   */
  it('gives focus, as the drawer opens, to the Libraries heading’s link, Close one Shift+Tab before it and the wordmark’s link one more', async () => {
    const host = await shellAt('/config/rules', 390, true)
    await openDrawer(host)
    expect(document.activeElement).toBe(
      page.getByRole('link', { name: 'Libraries', exact: true }).element(),
    )
    await userEvent.tab({ shift: true })
    expect(document.activeElement).toBe(need(host, 'button.menu-close'))
    await userEvent.tab({ shift: true })
    expect(document.activeElement).toBe(wordmarkOf(host).link)
  })

  it.each([1440, 390])(
    'stands where the plain wordmark stood, its link as big as the wordmark, at %i px',
    async (width) => {
      const host = await shellAt('/config/rules', width, true)
      if (width < 900) await openDrawer(host)
      const { mark, link } = wordmarkOf(host)
      const column = need(host, '#library-menu').getBoundingClientRect()
      const seal = need(mark, '.seal').getBoundingClientRect()
      const inset = lengthOf(host, '--space-4')
      expect([seal.left - column.left, seal.top - column.top]).toEqual([
        inset,
        inset,
      ])
      expect(link.getBoundingClientRect().toJSON()).toEqual(
        mark.getBoundingClientRect().toJSON(),
      )
      expect(need(host, 'nav.sidebar').getBoundingClientRect().top).toBe(
        link.getBoundingClientRect().bottom,
      )
    },
  )
})
