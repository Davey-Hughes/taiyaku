import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as library from '../src/library'
import { useSearchHits } from '../src/searchHits'
import { deferred, flush, land } from './support/async'
import { mount, rerender } from './support/mount'
import { watchDrawn } from './support/wait'

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
  cover: '/api/works/gotoubun-v01/raw/pages/0/thumb?v=c0ffee-0-1',
  ...over,
})

/** An answer whose one hit is titled `title`. */
const hitsOf = (title: string, query = title): library.SearchHits => ({
  series: [card({ title })],
  unfiled: [],
  query,
})

const titles = (hits: library.SearchHits | undefined) =>
  hits?.series.map((s) => s.title)

/**
 * Mount the hook for `first`, an arrival, and hand back what it last
 * returned. `ask(q)` re-renders the *same* instance with a query the box
 * typed, which is what the box changing `?q=` does to the Library view;
 * `arrive(q)` with one it did not type, as Back or a link sets `?q=` under
 * a view that stays mounted; `leave()` swaps it for something with no hook
 * in it, a real unmount, which is what navigating away from the view does.
 *
 * The probe also draws what the hook returned, so `watchDrawn` can read
 * every state committed between two steps of a test.
 */
function probe(library?: number, first = '') {
  let last: ReturnType<typeof useSearchHits> | undefined
  function Probe({ q, typed }: { q: string; typed: boolean }) {
    last = useSearchHits(q, library, typed)
    return (
      <output data-testid="hits">
        {JSON.stringify({
          q,
          hits: titles(last.hits) ?? null,
          failure: last.failure ?? null,
          waiting: last.waiting,
        })}
      </output>
    )
  }
  const host = mount(<Probe q={first} typed={false} />)
  const to = (q: string, typed: boolean) =>
    act(async () => {
      rerender(<Probe q={q} typed={typed} />)
    })
  return {
    last: () => {
      if (!last) throw new Error('the hook never ran')
      return last
    },
    /** What the probe draws now. */
    drawn: () =>
      host.querySelector('[data-testid="hits"]')?.textContent ?? 'nothing',
    ask: (q: string) => to(q, true),
    arrive: (q: string) => to(q, false),
    leave: () => rerender(<p>gone</p>),
    /** Mount the hook afresh at `q`, as a return to the view does. */
    remount: (q: string) => rerender(<Probe key="again" q={q} typed={false} />),
  }
}

/**
 * What the probe draws at `q` for hits titled `hits`, no failure and no
 * wait: the query beside the hits, so a render that draws one query's hits
 * under another is seen.
 */
const showing = (q: string, ...hits: string[]) =>
  JSON.stringify({ q, hits, failure: null, waiting: false })
/** What the probe draws at `q` with no hits and no failure. */
const none = (q: string, waiting = false) =>
  JSON.stringify({ q, hits: null, failure: null, waiting })

/** Wait out the debounce, and let the search land. */
async function settle() {
  await act(async () => {
    vi.advanceTimersByTime(300)
  })
  await land()
}

describe('useSearchHits', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    // Call history and queued answers, not just the resolved value: without
    // this, a test's `toHaveBeenCalledTimes`/`not.toHaveBeenCalled` counts
    // every call made by every test that ran earlier in this file, and an
    // answer queued `Once` that an earlier test never asked for answers this
    // one's first call.
    vi.mocked(library.search).mockReset()
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
    await land()
    expect(titles(hook.last().hits)).toEqual(['second'])
  })

  it('keeps those results when an earlier query fails after them', async () => {
    // The mirror of the case above, for a failure. A *stale* rejection must
    // not put a failure over results that were read successfully afterwards
    // — and name a failure for a query the box no longer holds.
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
    await land()
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
    await land()
    expect(hook.last().hits).toBeUndefined()
  })

  /**
   * Unmounting inside the 250 ms window drops the pending call: the view it
   * was meant for is gone, so nothing is asked for it when the timer would
   * have fired.
   */
  it('does not search after the view that asked is gone', async () => {
    const hook = probe()
    await hook.ask('hana')
    // Still inside the 250ms debounce window: nothing has been sent yet.
    hook.leave()
    await settle()
    expect(vi.mocked(library.search)).not.toHaveBeenCalled()
  })

  it('sends the query it is mounted with at once, not after the typing delay', async () => {
    const hook = probe(2, 'foo')
    await land()
    expect(vi.mocked(library.search).mock.calls).toEqual([['foo', 2]])
    expect(hook.drawn()).toBe(showing('foo', '五等分の花嫁'))
  })

  it('sends a query it arrives at under a mounted view at once, and one typed only once the typing stops', async () => {
    const hook = probe()
    await hook.arrive('foo')
    await land()
    expect(vi.mocked(library.search).mock.calls).toEqual([['foo', undefined]])
    await hook.ask('fooba')
    await act(async () => {
      vi.advanceTimersByTime(100)
    })
    await hook.ask('foobar')
    await act(async () => {
      vi.advanceTimersByTime(249)
    })
    await land()
    expect(vi.mocked(library.search).mock.calls).toEqual([['foo', undefined]])
    await act(async () => {
      vi.advanceTimersByTime(1)
    })
    await land()
    expect(vi.mocked(library.search).mock.calls).toEqual([
      ['foo', undefined],
      ['foobar', undefined],
    ])
  })

  it('draws the hits kept for a query on a return, before its re-read lands, and reads them again once', async () => {
    vi.mocked(library.search).mockResolvedValue(hitsOf('foo'))
    const hook = probe(undefined, 'foo')
    await land()
    expect(hook.drawn()).toBe(showing('foo', 'foo'))

    hook.leave()
    vi.mocked(library.search).mockClear()
    const again = deferred<library.SearchHits>()
    vi.mocked(library.search).mockReturnValueOnce(again.promise)
    const seen = watchDrawn(hook.drawn)
    hook.remount('foo')
    await land()
    seen.stop()
    expect([...seen.seen]).toEqual(['nothing', showing('foo', 'foo')])
    expect(vi.mocked(library.search).mock.calls).toEqual([['foo', undefined]])
    await flush(() => again.resolve(hitsOf('foo, read again', 'foo')))
    await land()
    expect(hook.drawn()).toBe(showing('foo', 'foo, read again'))
  })

  it('draws the hits kept for a query Back returns to under a mounted view before its re-read lands', async () => {
    vi.mocked(library.search).mockImplementation(async (q) => hitsOf(q))
    const hook = probe(undefined, 'foo')
    await land()
    await hook.arrive('bar')
    await land()
    expect(hook.drawn()).toBe(showing('bar', 'bar'))

    vi.mocked(library.search).mockClear()
    vi.mocked(library.search).mockReturnValueOnce(
      deferred<library.SearchHits>().promise,
    )
    const seen = watchDrawn(hook.drawn)
    await hook.arrive('foo')
    await land()
    seen.stop()
    expect([...seen.seen]).toEqual([
      showing('bar', 'bar'),
      showing('foo', 'foo'),
    ])
    expect(vi.mocked(library.search).mock.calls).toEqual([['foo', undefined]])
  })

  it('keeps the last query’s hits while a typed one is out, and draws the next in one step', async () => {
    vi.mocked(library.search).mockResolvedValueOnce(hitsOf('fo'))
    const hook = probe()
    await hook.ask('fo')
    await settle()
    expect(hook.drawn()).toBe(showing('fo', 'fo'))

    const foo = deferred<library.SearchHits>()
    vi.mocked(library.search).mockReturnValueOnce(foo.promise)
    const seen = watchDrawn(hook.drawn)
    await hook.ask('foo')
    await settle()
    expect(vi.mocked(library.search)).toHaveBeenLastCalledWith('foo', undefined)
    // Hits are drawn, so the hook says nothing waits for the view to draw.
    expect(hook.drawn()).toBe(showing('foo', 'fo'))
    await flush(() => foo.resolve(hitsOf('foo')))
    await land()
    seen.stop()
    expect([...seen.seen]).toEqual([
      showing('fo', 'fo'),
      showing('foo', 'fo'),
      showing('foo', 'foo'),
    ])
  })

  it('holds the hits of a query that lands inside the typing wait when the next query is sent', async () => {
    const fo = deferred<library.SearchHits>()
    const foo = deferred<library.SearchHits>()
    vi.mocked(library.search)
      .mockReturnValueOnce(fo.promise)
      .mockReturnValueOnce(foo.promise)
    const hook = probe()
    await hook.ask('fo')
    await settle()
    expect(hook.drawn()).toBe(none('fo', true))
    await hook.ask('foo')
    // `fo` lands while `foo` still waits out the typing delay.
    await flush(() => fo.resolve(hitsOf('fo')))
    await land()
    expect(hook.drawn()).toBe(showing('foo', 'fo'))
    await settle()
    expect(vi.mocked(library.search)).toHaveBeenLastCalledWith('foo', undefined)
    expect(hook.drawn()).toBe(showing('foo', 'fo'))
  })

  it('waits with nothing drawn on an arrival with nothing kept, and draws no hits of the queries before it', async () => {
    vi.mocked(library.search)
      .mockResolvedValueOnce(hitsOf('fo'))
      .mockResolvedValueOnce(hitsOf('foo'))
    const hook = probe()
    await hook.ask('fo')
    await settle()
    // A second typed query, sent with the first one's hits drawn.
    await hook.ask('foo')
    await settle()
    expect(hook.drawn()).toBe(showing('foo', 'foo'))

    const bar = deferred<library.SearchHits>()
    vi.mocked(library.search).mockReturnValueOnce(bar.promise)
    await hook.arrive('bar')
    await land()
    expect(vi.mocked(library.search)).toHaveBeenLastCalledWith('bar', undefined)
    expect(hook.drawn()).toBe(none('bar', true))
    await flush(() => bar.resolve(hitsOf('bar')))
    await land()
    expect(hook.drawn()).toBe(showing('bar', 'bar'))
  })

  it('draws no hits once the box is cleared, sends nothing for it, and draws none of them under the next query', async () => {
    vi.mocked(library.search).mockResolvedValueOnce(hitsOf('fo'))
    const hook = probe()
    await hook.ask('fo')
    await settle()
    expect(hook.drawn()).toBe(showing('fo', 'fo'))

    await hook.ask('')
    expect(hook.drawn()).toBe(none(''))
    await settle()
    expect(vi.mocked(library.search)).toHaveBeenCalledTimes(1)

    vi.mocked(library.search).mockReturnValueOnce(
      deferred<library.SearchHits>().promise,
    )
    await hook.ask('f')
    await settle()
    expect(vi.mocked(library.search)).toHaveBeenLastCalledWith('f', undefined)
    expect(hook.drawn()).toBe(none('f', true))
  })
})
