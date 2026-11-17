import { act } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { cdp, page } from 'vitest/browser'
import '../src/index.css'
import * as aboutApi from '../src/about'
import * as library from '../src/library'
import { Library } from '../src/Library'
import { Series } from '../src/Series'
import { Shell } from '../src/Shell'
import { ABOUT } from './support/about'
import { homePage } from './support/homePage'
import { mount } from './support/mount'

/**
 * **A cover that fails takes the room of one that loads** (spec C1 §8's
 * third empty state): in the library's grid, a series page's volumes and
 * the continue strip, at a desktop's width and a phone's, the box a cover is
 * drawn in is the same height whether the cover loaded or failed, and the
 * title under it sits at the same place. A cover swapped for its
 * placeholder after Back has put the scroll in place would otherwise move
 * what is in view.
 */

// The build's label at the sidebar's foot asks for the build; answered here.
vi.mock('../src/about', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/about')>()),
  about: vi.fn(),
}))

vi.mock('../src/library', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../src/library')>()
  return {
    ...actual,
    status: vi.fn(),
    home: vi.fn(),
    grid: vi.fn(),
    oneSeries: vi.fn(),
    // A data address is drawn as it is: the test's server has no cover to
    // send, a cover that loads is half of what is measured, and a cover that
    // fails as a data address sends nothing through the dev server's proxy.
    coverSources: (url: string) =>
      url.startsWith('data:')
        ? { src: url, srcSet: '' }
        : actual.coverSources(url),
  }
})

/** A cover that loads: one pixel. */
const LOADS =
  'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII='
/** A cover the browser fails to load: a data address that does not decode. */
const FAILS = 'data:image/png;base64,!'

const series = (id: number, title: string, cover: string) => ({
  id,
  title,
  works: 1,
  editions: 1,
  aligned: 0,
  added: id,
  unread: 1,
  cover,
})

const work = (id: number, title: string, cover: string) => ({
  id,
  slug: `work-${id}`,
  title,
  volume: id,
  aligned: false,
  pages: 180,
  editions: [
    { key: 'raw', label: 'Raw', members: 1, pages: 180, missing: 0, cover },
  ],
  opens: 'raw',
})

// Titles of one length, so each pair's titles take the same lines.
const PAIR = [
  ['Cover loaded', LOADS],
  ['Cover failed', FAILS],
] as const

beforeEach(() => {
  vi.mocked(aboutApi.about).mockResolvedValue(ABOUT)
  vi.mocked(library.status).mockResolvedValue({
    libraries: [{ id: 1, name: 'Volumes', scanned: true }],
    restricted: false,
  })
  vi.mocked(library.grid).mockResolvedValue({
    series: PAIR.map(([title, cover], i) => series(i + 1, title, cover)),
    unfiled: [],
  })
  vi.mocked(library.home).mockResolvedValue({
    continuing: PAIR.map(([title, cover], i) => work(i + 1, title, cover)),
    added: [],
    read: [],
  })
  vi.mocked(library.oneSeries).mockResolvedValue({
    series: { ...series(1, 'Series', LOADS), titles: [] },
    works: PAIR.map(([title, cover], i) => work(i + 1, title, cover)),
  })
})

async function at(path: string, width: number): Promise<HTMLElement> {
  await page.viewport(width, 844)
  const host = mount(
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
  host.style.height = '100%'
  return host
}

/**
 * The tile titled `title` in `list`, once its cover has loaded or failed as
 * `drawn` says: the height of the box the cover is drawn in, and the
 * title's offset from the tile's top. Not the tile's own height: a row of
 * the grid or the strip stretches its tiles to the tallest.
 */
async function measured(
  host: HTMLElement,
  list: string,
  title: string,
  drawn: 'loaded' | 'placeholder',
) {
  const tile = () =>
    [...host.querySelectorAll(`${list} > li`)].find(
      (li) => li.querySelector('.title')?.textContent === title,
    )
  await expect
    .poll(() => {
      const li = tile()
      const img = li?.querySelector('img')
      if (img) return img.complete && img.naturalWidth > 0 ? 'loaded' : 'img'
      return li?.querySelector('.cover-missing') ? 'placeholder' : 'none'
    })
    .toBe(drawn)
  await act(async () => {
    await new Promise((resolve) => requestAnimationFrame(resolve))
  })
  const li = tile()
  if (!li) throw new Error(`no tile ${title}`)
  const cover =
    li.querySelector('.cover-box') ?? li.querySelector('img, .cover-missing')
  return [
    cover?.getBoundingClientRect().height,
    (li.querySelector('.title')?.getBoundingClientRect().top ?? Number.NaN) -
      li.getBoundingClientRect().top,
  ]
}

/**
 * A series page's volume tiles draw their own title in the title face at
 * 600, in a link and in O62's `tile-unreadable` box alike. `library.css`
 * names that title by where it sits, so the metadata editor's Title field in
 * the same tile is left out (`MetadataEditor.browser.test.tsx`); this holds
 * the tiles the rule must still reach.
 */
describe('a volume tile’s own title', () => {
  it('is in the title face at 600, linked or unreadable', async () => {
    const gone = work(2, 'Files gone', LOADS)
    vi.mocked(library.oneSeries).mockResolvedValue({
      series: { ...series(1, 'Series', LOADS), titles: [] },
      works: [
        work(1, 'Readable', LOADS),
        {
          ...gone,
          editions: gone.editions.map((e) => ({ ...e, missing: 1 })),
        },
      ],
    })
    const host = await at('/series/1', 1280)
    const titleOf = (name: string) =>
      [...host.querySelectorAll('.volumes > li .title')].find(
        (el) => el.textContent === name,
      )
    await expect
      .poll(() =>
        [titleOf('Readable'), titleOf('Files gone')].includes(undefined),
      )
      .toBe(false)
    const linked = titleOf('Readable') as Element
    const unreadable = titleOf('Files gone') as Element
    const tile = linked.closest('li') as Element
    // The face named: a probe set in `--font-title`, in the tile it reads.
    const probe = document.createElement('span')
    probe.style.fontFamily = 'var(--font-title)'
    tile.append(probe)
    const titleFace = getComputedStyle(probe).fontFamily
    probe.remove()
    const face = (el: Element) => {
      const style = getComputedStyle(el)
      return { family: style.fontFamily, weight: style.fontWeight }
    }

    // The fixture tells: one tile of each kind, and the tile's own text in
    // another face and weight, which a title the rule missed would take.
    expect(linked.parentElement?.tagName).toBe('A')
    expect(
      unreadable.parentElement?.classList.contains('tile-unreadable'),
    ).toBe(true)
    expect(face(tile).family).not.toBe(titleFace)
    expect(face(tile).weight).not.toBe('600')

    expect({ linked: face(linked), unreadable: face(unreadable) }).toEqual({
      linked: { family: titleFace, weight: '600' },
      unreadable: { family: titleFace, weight: '600' },
    })
  })
})

describe.each([1280, 390])('a failed cover’s tile at %i px', (width) => {
  it.each([
    ['/', '.grid'],
    // A guard, not a watched red: the strip's image is a flex item of its
    // link, which makes it a block already, so this row holds without
    // `.continue img`'s `display: block`.
    ['/', '.continue ul'],
    ['/series/1', '.volumes'],
  ])('on %s, in %s, takes the room of a loaded cover’s', async (path, list) => {
    const host = await at(path, width)
    const loaded = await measured(host, list, 'Cover loaded', 'loaded')
    const failed = await measured(host, list, 'Cover failed', 'placeholder')
    // Within half a pixel: an image's height and a box's from one width can
    // round to different sixty-fourths of a pixel, which nothing shows.
    expect(
      failed.map(
        (value, i) =>
          Math.abs((value ?? Number.NaN) - (loaded[i] ?? Number.NaN)) <= 0.5,
      ),
      `cover box and title offset: failed ${failed}, loaded ${loaded}`,
    ).toEqual([true, true])
  })
})

/**
 * **A cover asks for the image its drawn width needs** (`library.ts`'s
 * `coverSources` and `coverSizes`): beside the sidebar a cover is the size,
 * and in the phone layout it fills its column, up to the whole row. The
 * address is a blob's with the size after a `#`, so it loads with no server;
 * a data address would not do, as Chromium counts one as held already and
 * picks the largest. A blob per case, since a size the page already holds is
 * picked the same way.
 */
describe('a cover’s image', () => {
  async function drawn(width: number, coverSize: number, height?: number) {
    const png = Uint8Array.from(
      atob(LOADS.slice(LOADS.indexOf(',') + 1)),
      (c) => c.charCodeAt(0),
    )
    const blob = URL.createObjectURL(new Blob([png], { type: 'image/png' }))
    onTestFinished(() => URL.revokeObjectURL(blob))
    const address = `${blob}#cover`
    vi.mocked(library.grid).mockResolvedValue({
      series: [series(1, 'Cover', address)],
      unfiled: [],
    })
    vi.mocked(library.home).mockResolvedValue({
      continuing: [],
      added: [],
      read: [],
    })
    const host = await homePage({ width, height, coverSize })
    const img = () => host.querySelector<HTMLImageElement>('.grid img')
    await expect
      .poll(() => (img()?.complete ? img()?.currentSrc : ''))
      .not.toBe('')
    return {
      picked: img()?.currentSrc.slice(address.length),
      width: img()?.getBoundingClientRect().width,
    }
  }

  it.each([
    [1280, 144, 144, '?w=256'],
    [1280, 240, 240, '?w=512'],
    // One column, the whole row wide.
    [390, 240, 358, '?w=768'],
  ])(
    'at %i px with a %i px cover drawn %i px wide is the %s one',
    async (width, size, drawnWidth, picked) => {
      expect(await drawn(width, size)).toEqual({ picked, width: drawnWidth })
    },
  )

  // A phone held sideways: the phone layout by its height alone, its covers
  // grown to fill their columns. The browser picks by `sizes`, not by the
  // drawn width: the least of the server's three whose `srcset` width
  // reaches the width `coverSizes` names, which no filled column passes. So
  // the picked image is also no narrower than the cover drawn, or the filled
  // cover would draw blurred.
  it('at 956×440 px with a 144 px cover filling its column is the one for the filled width', async () => {
    const { picked, width } = await drawn(956, 144, 440)
    if (width === undefined) throw new Error('no cover drawn')
    const reaching = (wide: number) =>
      [256, 512, 768].find((edge) => Math.round((edge * 2) / 3) >= wide)
    const named = reaching(2 * 144 + 32)
    // The fixture tells: the cover is wider than its size, and the width
    // `coverSizes` names in the phone layout calls for a larger image than
    // the size alone would.
    expect({
      filled: width > 144 + 1,
      larger: named !== reaching(144),
    }).toEqual({ filled: true, larger: true })
    expect(picked).toBe(`?w=${named}`)
    expect(
      Math.round((Number(picked?.slice('?w='.length)) * 2) / 3),
      'the picked image against the cover drawn',
    ).toBeGreaterThanOrEqual(width)
  })

  /**
   * The page's density, set over the DevTools session: it is the page's, not
   * this frame's, so when the case ends the density is put back, at the
   * page's size and screen as they were. The override itself stays: clearing
   * it leaves the screen at the window's own size, not the one Playwright set.
   */
  async function atDensity(density: number) {
    const { cssLayoutViewport } = await cdp().send('Page.getLayoutMetrics')
    const metrics = {
      width: cssLayoutViewport.clientWidth,
      height: cssLayoutViewport.clientHeight,
      screenWidth: screen.width,
      screenHeight: screen.height,
      mobile: false,
    }
    const was = window.devicePixelRatio
    onTestFinished(async () => {
      await cdp().send('Emulation.setDeviceMetricsOverride', {
        ...metrics,
        deviceScaleFactor: was,
      })
    })
    await cdp().send('Emulation.setDeviceMetricsOverride', {
      ...metrics,
      deviceScaleFactor: density,
    })
    expect(window.devicePixelRatio).toBe(density)
  }

  // O133's picks, which a cover the default size must keep.
  it.each([
    [1, '?w=256'],
    [2, '?w=512'],
    [3, '?w=768'],
  ])(
    'at 1280 px with a 144 px cover on a %i× screen is the %s one',
    async (density, picked) => {
      await atDensity(density)
      expect(await drawn(1280, 144)).toEqual({ picked, width: 144 })
    },
  )
})
