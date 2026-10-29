import { act } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { page } from 'vitest/browser'
import '../src/index.css'
import * as aboutApi from '../src/about'
import * as library from '../src/library'
import { Library } from '../src/Library'
import { Series } from '../src/Series'
import { Shell } from '../src/Shell'
import { ABOUT } from './support/about'
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
