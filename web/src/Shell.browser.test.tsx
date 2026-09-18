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

/** How far the page itself scrolls sideways: zero is the spec's promise. */
const sideways = () =>
  document.documentElement.scrollWidth - document.documentElement.clientWidth

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
    // Beside the wordmark (spec §7.1): on its row, clear of its text. The
    // `.wordmark` box itself spans the column, so measure what it holds.
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
    const host = await at('/search?q=go-toubun', 390)
    expect(host.querySelector('.grid')).not.toBeNull()
    expect(sideways()).toBe(0)
  })
})
