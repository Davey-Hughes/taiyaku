import { act } from 'react'
import { MemoryRouter } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as library from './library'
import { Search } from './Search'
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

async function settle(entry = '/search'): Promise<HTMLElement> {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <MemoryRouter initialEntries={[entry]}>
        <Search />
      </MemoryRouter>,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
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
    expect(vi.mocked(library.search)).toHaveBeenCalledWith('花嫁')
    expect(host.querySelector('.grid .title')?.textContent).toBe('五等分の花嫁')
  })

  it('runs the query in the URL on arrival, so a search can be a link', async () => {
    const host = await settle('/search?q=%E8%8A%B1%E5%AB%81')
    await act(async () => {
      vi.advanceTimersByTime(300)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(vi.mocked(library.search)).toHaveBeenCalledWith('花嫁')
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
