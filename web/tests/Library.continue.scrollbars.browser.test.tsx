import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as aboutApi from '../src/about'
// **Required**, the same reason `Config.browser.test.tsx` gives.
import '../src/index.css'
import * as library from '../src/library'
import { SIDEBAR_LAYOUT } from '../src/phoneLayout'
import { ABOUT } from './support/about'
import { settleUntil } from './support/async'
import { inProgress, offGrid, seriesCard } from './support/continueRow'
import { homePage } from './support/homePage'

/**
 * The Continue row over the grid's columns where a scrollbar takes width
 * (spec O288 item 2): this file runs only in the Chromium launch that draws
 * classic scrollbars (`vite.config.ts`'s `SHOWN_SCROLLBARS`). Beside the
 * sidebar `main` scrolls, in the phone layout the document does, and either
 * narrows the grid and the row alike.
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
}))

/** Rows enough that the page scrolls at either width. */
const SERIES = 40
const ENTRIES = 12

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
})

describe('the Continue row over the grid with a scrollbar drawn (O288 item 2)', () => {
  it.each([1280, 899])(
    'lines the cards up over the grid with a scrollbar drawn at %i px',
    async (width) => {
      const host = await homePage({ width, coverSize: 144 })
      await settleUntil(
        'the Continue row and the grid',
        () =>
          host.querySelectorAll('.continue ul > li').length === ENTRIES &&
          host.querySelectorAll('main .grid > li').length === SERIES,
        2000,
      )
      await document.fonts.ready
      const main = host.querySelector('main') as HTMLElement
      const sidebar = matchMedia(SIDEBAR_LAYOUT).matches
      const scroller = sidebar ? main : document.documentElement
      const bar = sidebar
        ? main.offsetWidth - main.clientWidth
        : window.innerWidth - document.documentElement.clientWidth
      // The fixture tells: what scrolls at this width scrolls, and its
      // scrollbar takes width.
      expect({
        scrolls: scroller.scrollHeight > scroller.clientHeight,
        bar: bar > 0,
      }).toEqual({ scrolls: true, bar: true })
      await expect.poll(() => offGrid(host), { timeout: 3000 }).toEqual([])
    },
  )
})
