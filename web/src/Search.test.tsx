import { act } from 'react'
import { MemoryRouter, useLocation } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as library from './library'
import { Search } from './Search'
import { StatusContext } from './shell'
import { fields } from './testing/fields'
import { mount, rerender } from './testing/mount'

vi.mock('./library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./library')>()),
  search: vi.fn(),
}))

const card = (over: Partial<library.SeriesCard> = {}): library.SeriesCard => ({
  id: 7,
  title: '五等分の花嫁',
  works: 3,
  editions: 5,
  aligned: 1,
  added: 3,
  unread: 2,
  cover: '/api/library/series/7/cover',
  ...over,
})

/** Two libraries, in an order that is neither by id nor by name. */
const twoLibraries: library.LibraryStatus = {
  libraries: [
    { id: 2, name: 'Manga', scanned: true },
    { id: 1, name: 'Comics', scanned: true },
  ],
  restricted: false,
}

/**
 * A child that draws the router's query string, so a test can read what the
 * screen put in the URL.
 *
 * Declared inside a function for the reason `Shell.test.tsx`'s `reporter()`
 * gives: `style/useComponentExportOnlyModules` refuses an unexported
 * component at module scope, and `suspicious/noExportsInTest` refuses the
 * export.
 */
function locationProbe() {
  function Probe() {
    return <output data-testid="location">{useLocation().search}</output>
  }
  return <Probe />
}

/**
 * Mount the screen at `entry`, under the status `Shell` would publish.
 * `undefined` is what the screen reads with no shell above it, or before the
 * shell's own fetch has landed.
 */
async function settle(
  entry = '/search',
  status?: library.LibraryStatus,
): Promise<HTMLElement> {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(screenAt(entry, status))
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

/**
 * The tree `settle` mounts. A `rerender` of it with another status is the
 * shell's fetch landing under a screen already drawn: `MemoryRouter` reads
 * `initialEntries` once, so the URL stays where the screen has put it.
 */
const screenAt = (entry: string, status?: library.LibraryStatus) => (
  <MemoryRouter initialEntries={[entry]}>
    <StatusContext.Provider value={status}>
      <Search />
      {locationProbe()}
    </StatusContext.Provider>
  </MemoryRouter>
)

/** The query string the screen has put in the URL, as a plain object. */
const urlOf = (host: HTMLElement) =>
  Object.fromEntries(
    new URLSearchParams(
      host.querySelector('[data-testid="location"]')?.textContent ?? '',
    ),
  )

/** The select its label calls "Search in", or `undefined` where none is drawn. */
const scopeOf = (host: HTMLElement) =>
  fields(host.querySelectorAll('label')).find((f) => f.written === 'Search in')
    ?.control as HTMLSelectElement | undefined

/** Choose `value` in a controlled select and let the debounce and the fetch settle. */
async function choose(select: HTMLSelectElement, value: string) {
  await act(async () => {
    Object.getOwnPropertyDescriptor(
      HTMLSelectElement.prototype,
      'value',
    )?.set?.call(select, value)
    select.dispatchEvent(new Event('change', { bubbles: true }))
  })
  await act(async () => {
    vi.advanceTimersByTime(300)
    await Promise.resolve()
    await Promise.resolve()
  })
}

/** Wait out the debounce on arrival, and let the fetch settle. */
async function arrive(entry: string, status?: library.LibraryStatus) {
  const host = await settle(entry, status)
  await act(async () => {
    vi.advanceTimersByTime(300)
    await Promise.resolve()
    await Promise.resolve()
  })
  return host
}

/**
 * Set the box's value and fire the `input` event, without waiting out the
 * debounce. The native setter, not `box.value = q` directly: React installs
 * a per-instance value tracker over `value` on a controlled input, so a
 * plain assignment updates the tracker along with the DOM property, and the
 * subsequent `input` event then finds nothing changed and never calls
 * `onChange` — the same fix `Login.test.tsx`'s and `Account.test.tsx`'s own
 * `type()` helpers already use. Exported at this grain so a test can put a
 * query in mid-debounce, or clear the box, without the 300 ms advance
 * `type()` below folds in.
 */
async function setValue(host: HTMLElement, q: string) {
  const box = host.querySelector('input[type="search"]') as HTMLInputElement
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      'value',
    )?.set
    setter?.call(box, q)
    box.dispatchEvent(new Event('input', { bubbles: true }))
  })
}

/** Type into the box and let the debounce and the fetch settle. */
async function type(host: HTMLElement, q: string) {
  await setValue(host, q)
  await act(async () => {
    vi.advanceTimersByTime(300)
    await Promise.resolve()
    await Promise.resolve()
  })
}

describe('Search', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    // Call history, not just the resolved value: without this, a test's
    // `toHaveBeenCalledTimes`/`not.toHaveBeenCalled` counts every call made
    // by every test that ran earlier in this file, not just its own.
    vi.mocked(library.search).mockClear()
    vi.mocked(library.search).mockResolvedValue({
      series: [card()],
      unfiled: [],
      query: '花嫁',
    })
  })

  it('asks for nothing until something is typed', async () => {
    await settle()
    // Past the debounce window: without this, a `run('')` scheduled on
    // mount would still be sitting in its timer, unfired, and this
    // assertion could not tell that apart from never having been scheduled
    // at all.
    await act(async () => {
      vi.advanceTimersByTime(300)
      await Promise.resolve()
    })
    expect(vi.mocked(library.search)).not.toHaveBeenCalled()
  })

  it('searches what was typed, once the typing stops', async () => {
    const host = await settle()
    await type(host, '花嫁')
    expect(vi.mocked(library.search)).toHaveBeenCalledTimes(1)
    expect(vi.mocked(library.search)).toHaveBeenCalledWith('花嫁', undefined)
    expect(host.querySelector('.grid .title')?.textContent).toBe('五等分の花嫁')
  })

  it('runs the query in the URL on arrival, so a search can be a link', async () => {
    const host = await settle('/search?q=%E8%8A%B1%E5%AB%81')
    await act(async () => {
      vi.advanceTimersByTime(300)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(vi.mocked(library.search)).toHaveBeenCalledWith('花嫁', undefined)
    expect(
      (host.querySelector('input[type="search"]') as HTMLInputElement).value,
    ).toBe('花嫁')
  })

  it('names what was searched when nothing matched', async () => {
    vi.mocked(library.search).mockResolvedValue({
      series: [],
      unfiled: [],
      query: '花婿',
    })
    const host = await settle()
    await type(host, '花婿')
    expect(host.querySelector('.empty')?.textContent).toContain('花婿')
  })

  it('keeps the answer to the last query typed, not the last one to arrive', async () => {
    // The slow one is typed first and resolves last — the shape that puts a
    // stale result on screen when nothing counts the requests.
    let releaseSlow: (v: library.SearchHits) => void = () => {}
    vi.mocked(library.search)
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            releaseSlow = resolve
          }),
      )
      .mockResolvedValueOnce({
        series: [card({ title: 'second' })],
        unfiled: [],
        query: 'b',
      })

    const host = await settle()
    await type(host, 'aaa')
    await type(host, 'bbb')
    await act(async () => {
      releaseSlow({
        series: [card({ title: 'first' })],
        unfiled: [],
        query: 'a',
      })
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.grid .title')?.textContent).toBe('second')
  })

  it('keeps those results when an earlier query fails after them', async () => {
    // The mirror of the case above, for the `.catch`, which needs its own
    // sequence check and had no test. Without it a *stale* rejection puts a
    // red alert over results that were read successfully afterwards — and the
    // alert names a failure for a query the box no longer holds.
    let rejectSlow: (why: unknown) => void = () => {}
    vi.mocked(library.search)
      .mockImplementationOnce(
        () =>
          new Promise((_resolve, reject) => {
            rejectSlow = reject
          }),
      )
      .mockResolvedValueOnce({
        series: [card({ title: 'second' })],
        unfiled: [],
        query: 'b',
      })

    const host = await settle()
    await type(host, 'aaa')
    await type(host, 'bbb')
    await act(async () => {
      rejectSlow(new Error('/api/library/search: 500'))
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('[role="alert"]')).toBeNull()
    expect(host.querySelector('.grid .title')?.textContent).toBe('second')
  })

  it('cancels a pending search if the box is cleared inside the debounce window', async () => {
    const host = await settle()
    await setValue(host, '花嫁')
    // Still inside the 250ms debounce window: nothing has been sent yet.
    await setValue(host, '')
    await act(async () => {
      vi.advanceTimersByTime(300)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(vi.mocked(library.search)).not.toHaveBeenCalled()
  })

  it('does not render stale results if the box is cleared while a fetch is in flight', async () => {
    let resolveSearch: (v: library.SearchHits) => void = () => {}
    vi.mocked(library.search).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveSearch = resolve
        }),
    )
    const host = await settle()
    await type(host, '花嫁')
    expect(vi.mocked(library.search)).toHaveBeenCalledTimes(1)
    await setValue(host, '')
    await act(async () => {
      resolveSearch({ series: [card()], unfiled: [], query: '花嫁' })
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.grid')).toBeNull()
    expect(
      (host.querySelector('input[type="search"]') as HTMLInputElement).value,
    ).toBe('')
  })

  /**
   * M11 of the whole-branch review: `Search` never called `run.cancel()` on
   * teardown, unlike `Library`, `Shell` and `Series`, which all carry a
   * `cancelled` flag for the same reason. Typing, then navigating away
   * inside the 250 ms window, then letting the timer fire, called
   * `library.search` after the screen it was meant for was gone.
   *
   * `rerender` swaps this root's UI for something with no `Search` in it —
   * a real unmount, not merely leaving the box untouched — which is what a
   * navigation away from `/search` does to this component in the app.
   */
  it('does not search after the screen it was typed on is gone', async () => {
    const host = await settle()
    await setValue(host, 'hana')
    // Still inside the 250ms debounce window: nothing has been sent yet.
    rerender(
      <MemoryRouter>
        <p>gone</p>
      </MemoryRouter>,
    )
    await act(async () => {
      vi.advanceTimersByTime(300)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(vi.mocked(library.search)).not.toHaveBeenCalled()
  })

  it('says so when the search fails', async () => {
    vi.mocked(library.search).mockRejectedValue(
      new Error('/api/library/search: 500'),
    )
    const host = await settle()
    await type(host, '花嫁')
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('500')
  })

  it('opens with one h1, Search, for a screen reader alone — O163', async () => {
    const host = await settle()
    const h1s = [...host.querySelectorAll('h1')]
    expect(h1s.map((h) => [h.textContent, h.className])).toEqual([
      ['Search', 'visually-hidden'],
    ])
    expect(host.firstElementChild).toBe(h1s[0])
  })

  it('names its tab Search, and typing does not rename it — O173', async () => {
    document.title = 'stale'
    const host = await settle()
    expect(document.title).toBe('Search — taiyaku')
    await type(host, 'one')
    expect(document.title).toBe('Search — taiyaku')
  })
})

describe('Search in one library — spec F §3', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.mocked(library.search).mockClear()
    vi.mocked(library.search).mockResolvedValue({
      series: [card()],
      unfiled: [],
      query: 'Series',
    })
  })

  it('offers Every library, then each library by name in the status’s order', async () => {
    const host = await settle('/search', twoLibraries)
    const scope = scopeOf(host)
    if (!scope) throw new Error('no select labelled Search in')
    expect([...scope.options].map((o) => [o.value, o.textContent])).toEqual([
      ['', 'Every library'],
      ['2', 'Manga'],
      ['1', 'Comics'],
    ])
    expect(scope.value).toBe('')
  })

  it('draws no select where the status lists one library', async () => {
    const host = await settle('/search', {
      libraries: [{ id: 2, name: 'Manga', scanned: true }],
      restricted: false,
    })
    // The box is drawn, so the screen is there to have drawn a select.
    expect(host.querySelector('input[type="search"]')).not.toBeNull()
    expect(host.querySelector('select')).toBeNull()
  })

  it('searches the library in the URL on arrival, and the select shows it', async () => {
    const host = await arrive('/search?q=Series&library=2', twoLibraries)
    // Every call, not the last: one search, narrowed, and none before it of
    // another scope.
    expect(vi.mocked(library.search).mock.calls).toEqual([['Series', 2]])
    expect(scopeOf(host)?.value).toBe('2')
  })

  it('searches a link’s library once, when the status lands after the search has gone', async () => {
    const host = await arrive('/search?q=Series&library=2')
    expect(vi.mocked(library.search).mock.calls).toEqual([['Series', 2]])
    rerender(screenAt('/search?q=Series&library=2', twoLibraries))
    await act(async () => {
      vi.advanceTimersByTime(300)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(vi.mocked(library.search).mock.calls).toEqual([['Series', 2]])
    expect(scopeOf(host)?.value).toBe('2')
  })

  it('searches every library, and says so, for a library the status does not list', async () => {
    const host = await arrive('/search?q=Series&library=7', twoLibraries)
    expect(vi.mocked(library.search).mock.calls).toEqual([
      ['Series', undefined],
    ])
    const scope = scopeOf(host)
    if (!scope) throw new Error('no select labelled Search in')
    expect(scope.value).toBe('')
    expect(scope.selectedOptions[0]?.textContent).toBe('Every library')
    // The URL is the reader's, and is left as it was.
    expect(urlOf(host)).toEqual({ q: 'Series', library: '7' })
  })

  it('searches every library, and draws no select, for a link to the one library there is', async () => {
    const host = await arrive('/search?q=Series&library=2', {
      libraries: [{ id: 2, name: 'Manga', scanned: true }],
      restricted: false,
    })
    expect(vi.mocked(library.search).mock.calls).toEqual([
      ['Series', undefined],
    ])
    expect(host.querySelector('select')).toBeNull()
  })

  // `library.ts`'s `libraryParam` is the reading the library page and the
  // sidebar give the value too, and what it reads as no library id narrows
  // nothing, before the status lands as well as after.
  for (const raw of ['abc', '', '0', '01', '-1']) {
    const named = raw === '' ? ' (empty)' : raw
    it(`searches every library for ?library=${named}, which is no library`, async () => {
      const host = await arrive(`/search?q=Series&library=${raw}`, twoLibraries)
      expect(vi.mocked(library.search).mock.calls).toEqual([
        ['Series', undefined],
      ])
      expect(scopeOf(host)?.value).toBe('')
    })

    it(`searches every library for ?library=${named} while the status is in flight`, async () => {
      await arrive(`/search?q=Series&library=${raw}`)
      expect(vi.mocked(library.search).mock.calls).toEqual([
        ['Series', undefined],
      ])
    })
  }

  it('choosing a library searches it and puts it in the URL beside q, and Every library takes it out', async () => {
    const host = await arrive('/search?q=Series', twoLibraries)
    expect(vi.mocked(library.search)).toHaveBeenLastCalledWith(
      'Series',
      undefined,
    )
    const scope = scopeOf(host)
    if (!scope) throw new Error('no select labelled Search in')

    await choose(scope, '2')
    expect(vi.mocked(library.search)).toHaveBeenLastCalledWith('Series', 2)
    expect(urlOf(host)).toEqual({ q: 'Series', library: '2' })
    expect(scope.value).toBe('2')

    await choose(scope, '')
    expect(vi.mocked(library.search)).toHaveBeenLastCalledWith(
      'Series',
      undefined,
    )
    expect(urlOf(host)).toEqual({ q: 'Series' })
    expect(scope.value).toBe('')
    expect(vi.mocked(library.search)).toHaveBeenCalledTimes(3)
  })

  it('keeps the chosen library in the URL while the box is typed in', async () => {
    const host = await arrive('/search?q=Ser&library=2', twoLibraries)
    await type(host, 'Series')
    expect(urlOf(host)).toEqual({ q: 'Series', library: '2' })
    expect(vi.mocked(library.search)).toHaveBeenLastCalledWith('Series', 2)
    expect(scopeOf(host)?.value).toBe('2')
  })
})
