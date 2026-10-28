import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as library from '../src/library'
import { useSearchHits } from '../src/searchHits'
import { mount, rerender } from './support/mount'

vi.mock('../src/library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/library')>()),
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

/**
 * Mount the hook for an empty query and hand back what it last returned.
 * `ask(q)` re-renders the *same* instance with another query, which is what
 * the box changing `?q=` does to the Library view; `leave()` swaps it for
 * something with no hook in it, a real unmount, which is what navigating
 * away from the view does.
 */
function probe(library?: number) {
  let last: ReturnType<typeof useSearchHits> | undefined
  function Probe({ q }: { q: string }) {
    last = useSearchHits(q, library)
    return null
  }
  mount(<Probe q="" />)
  return {
    last: () => {
      if (!last) throw new Error('the hook never ran')
      return last
    },
    ask: async (q: string) => {
      await act(async () => {
        rerender(<Probe q={q} />)
      })
    },
    leave: () => rerender(<p>gone</p>),
  }
}

/** Wait out the debounce, and let the fetch settle. */
async function settle() {
  await act(async () => {
    vi.advanceTimersByTime(300)
    await Promise.resolve()
    await Promise.resolve()
  })
}

const titles = (hits: library.SearchHits | undefined) =>
  hits?.series.map((s) => s.title)

describe('useSearchHits', () => {
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

  it('asks for nothing while the query is empty', async () => {
    probe()
    // Past the debounce window: without this, a `run('')` scheduled on
    // mount would still be sitting in its timer, unfired, and this
    // assertion could not tell that apart from never having been scheduled
    // at all.
    await settle()
    expect(vi.mocked(library.search)).not.toHaveBeenCalled()
  })

  it('keeps the answer to the last query asked, not the last one to arrive', async () => {
    // The slow one is asked first and resolves last — the shape that puts a
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

    const hook = probe()
    await hook.ask('aaa')
    await settle()
    await hook.ask('bbb')
    await settle()
    await act(async () => {
      releaseSlow({
        series: [card({ title: 'first' })],
        unfiled: [],
        query: 'a',
      })
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(titles(hook.last().hits)).toEqual(['second'])
  })

  it('keeps those results when an earlier query fails after them', async () => {
    // The mirror of the case above, for the `.catch`, which needs its own
    // sequence check. Without it a *stale* rejection puts a failure over
    // results that were read successfully afterwards — and names a failure
    // for a query the box no longer holds.
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

    const hook = probe()
    await hook.ask('aaa')
    await settle()
    await hook.ask('bbb')
    await settle()
    await act(async () => {
      rejectSlow(new Error('/api/library/search: 500'))
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(hook.last().failure).toBeUndefined()
    expect(titles(hook.last().hits)).toEqual(['second'])
  })

  it('cancels a pending search if the query is emptied inside the debounce window', async () => {
    const hook = probe()
    await hook.ask('花嫁')
    // Still inside the 250ms debounce window: nothing has been sent yet.
    await hook.ask('')
    await settle()
    expect(vi.mocked(library.search)).not.toHaveBeenCalled()
  })

  it('does not answer with stale results if the query is emptied while a fetch is in flight', async () => {
    let resolveSearch: (v: library.SearchHits) => void = () => {}
    vi.mocked(library.search).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveSearch = resolve
        }),
    )
    const hook = probe()
    await hook.ask('花嫁')
    await settle()
    expect(vi.mocked(library.search)).toHaveBeenCalledTimes(1)
    await hook.ask('')
    await act(async () => {
      resolveSearch({ series: [card()], unfiled: [], query: '花嫁' })
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(hook.last().hits).toBeUndefined()
  })

  /**
   * Unmounting inside the 250 ms window drops the pending call: the view it
   * was meant for is gone, so nothing is asked for it when the timer would
   * have fired. `Library`, `Shell` and `Series` carry a `cancelled` flag for
   * the same reason.
   */
  it('does not search after the view that asked is gone', async () => {
    const hook = probe()
    await hook.ask('hana')
    // Still inside the 250ms debounce window: nothing has been sent yet.
    hook.leave()
    await settle()
    expect(vi.mocked(library.search)).not.toHaveBeenCalled()
  })
})
