/**
 * `usePage` (spec §4), driven through a page drawn as a settings page is:
 * its header at once, `PageLoading` in the body's place while it waits, and
 * its body only once `ready`. Each address's client call is answered by
 * hand, so the order answers land in is the test's to choose, and the wait
 * line's clock is held, so 199 ms and 200 ms are read without a race.
 */
import { act, useEffect } from 'react'
import { beforeEach, describe, expect, it } from 'vitest'
import {
  entryOf,
  IDLE_KEPT,
  invalidate,
  type Read,
  readSet,
} from '../src/reads'
import { PageHeader, PageLoading } from '../src/settingsLayout'
import { mount, rerender } from './support/mount'
import { address } from './support/reads'
import { holdTheClock, pass, WAIT_DRAWN_AFTER_MS } from './support/wait'
import { usePage } from '../src/usePage'

/** Renders of each part of the body, by its read's address. */
let renders: Map<string, number>
/** What each render of the page drew in the body's place, in order. */
let drawn: ('nothing' | 'loading' | 'body')[]

/**
 * The page under test, and a component that invalidates a read as it
 * mounts. Made in `testPage` because `useComponentExportOnlyModules`
 * refuses an unexported component at module scope, and `noExportsInTest`
 * refuses the export; made once per test, so a test that leaves the page and
 * comes back draws the same component.
 */
let TestPage: ReturnType<typeof testPage>['TestPage']
let Invalidate: ReturnType<typeof testPage>['Invalidate']

beforeEach(() => {
  // Every case, so that none can draw "Loading…" by the real clock passing
  // 200 ms between two of its steps.
  holdTheClock()
  renders = new Map()
  drawn = []
  ;({ TestPage, Invalidate } = testPage())
})

function testPage() {
  /** One read's part of the body: its answer, and its failure if any. */
  function Part({
    name,
    answer,
    failure,
  }: {
    name: string
    answer: unknown
    failure: unknown
  }) {
    renders.set(name, (renders.get(name) ?? 0) + 1)
    const answered = JSON.stringify(answer) ?? 'none'
    return (
      <p data-part={name}>
        {failure === undefined
          ? answered
          : `${answered}; failed: ${(failure as Error).message}`}
      </p>
    )
  }

  /**
   * Invalidates `read` from its mount effect, which runs after the effects
   * of a sibling drawn before it and before those of one drawn after it,
   * and after every cleanup of the same commit.
   */
  function Invalidate({ read }: { read: Read<unknown> }) {
    useEffect(() => {
      void invalidate(read)
    }, [read])
    return null
  }

  function TestPage({
    reads,
    title = 'Test page',
  }: {
    reads: readonly (Read<unknown> | undefined)[]
    title?: string
  }) {
    const page = usePage(reads)
    drawn.push(page.ready ? 'body' : page.loadingDrawn ? 'loading' : 'nothing')
    return (
      <>
        <PageHeader title={title} />
        {page.ready ? (
          <div className="page-body">
            {reads.map((r, i) => (
              <Part
                key={r?.address ?? i}
                name={r?.address ?? String(i)}
                answer={page.answers[i]}
                failure={page.failures[i]}
              />
            ))}
          </div>
        ) : (
          <PageLoading drawn={page.loadingDrawn} />
        )}
      </>
    )
  }

  return { TestPage, Invalidate }
}

/** Each element the page draws, read one by one. */
function shape(host: HTMLElement): string[] {
  return [...host.children].map((el) => {
    if (el.classList.contains('page-header')) {
      return `header: ${el.querySelector('.page-title')?.textContent}`
    }
    if (el.classList.contains('page-body')) {
      const parts = [...el.querySelectorAll<HTMLElement>('[data-part]')]
      return `body: ${parts.map((p) => `${p.dataset.part} ${p.textContent}`).join(', ')}`
    }
    if (el.getAttribute('role') === 'status') return `status: ${el.textContent}`
    return el.outerHTML
  })
}

const HEADER = 'header: Test page'

/** Lets every answer already given land, and React draw what it set. */
async function land() {
  await act(async () => {
    await new Promise((resolve) => setImmediate(resolve))
  })
}

/** A first visit to `reads`, every request answered `answers`, then left. */
async function visitAndLeave(
  reads: readonly ReturnType<typeof address<string>>[],
  answers: readonly string[],
) {
  const host = mount(<TestPage reads={reads.map((r) => r.read)} />)
  reads.forEach((r, i) => {
    r.requests[r.requests.length - 1].resolve(answers[i])
  })
  await land()
  rerender(null)
  renders.clear()
  drawn = []
  return host
}

describe('usePage', () => {
  describe('a first visit (§4.1)', () => {
    it('draws the header alone until 200 ms, Loading… at 200, and the whole body in the render the last read lands in', async () => {
      const a = address<string>('/api/a')
      const b = address<string>('/api/b')
      const host = mount(<TestPage reads={[a.read, b.read]} />)
      expect(shape(host)).toEqual([HEADER])
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(shape(host)).toEqual([HEADER])
      pass(1)
      expect(shape(host)).toEqual([HEADER, 'status: Loading…'])

      a.requests[0].resolve('one')
      await land()
      expect(shape(host)).toEqual([HEADER, 'status: Loading…'])
      expect(renders.size).toBe(0)

      b.requests[0].resolve('two')
      await land()
      expect(shape(host)).toEqual([HEADER, 'body: /api/a "one", /api/b "two"'])
      expect(Object.fromEntries(renders)).toEqual({
        '/api/a': 1,
        '/api/b': 1,
      })
    })

    it('never draws Loading… for an answer within 200 ms', async () => {
      const a = address<string>('/api/a')
      const host = mount(<TestPage reads={[a.read]} />)
      pass(WAIT_DRAWN_AFTER_MS - 1)
      a.requests[0].resolve('one')
      await land()
      pass(WAIT_DRAWN_AFTER_MS)
      expect(shape(host)).toEqual([HEADER, 'body: /api/a "one"'])
      expect(drawn).not.toContain('loading')
    })
  })

  describe('a revisit (§4.2)', () => {
    it('draws the body in its first render, re-reads each address once as one set, and updates once when the answers changed', async () => {
      const a = address<string>('/api/a')
      const b = address<string>('/api/b')
      const host = await visitAndLeave([a, b], ['one', 'two'])

      rerender(<TestPage reads={[a.read, b.read]} />)
      expect(drawn[0]).toBe('body')
      expect(shape(host)).toEqual([HEADER, 'body: /api/a "one", /api/b "two"'])
      expect(a.fetch).toHaveBeenCalledTimes(2)
      expect(b.fetch).toHaveBeenCalledTimes(2)

      pass(WAIT_DRAWN_AFTER_MS)
      a.requests[1].resolve('one, again')
      await land()
      expect(shape(host)).toEqual([HEADER, 'body: /api/a "one", /api/b "two"'])
      expect(Object.fromEntries(renders)).toEqual({
        '/api/a': 1,
        '/api/b': 1,
      })

      b.requests[1].resolve('two, again')
      await land()
      expect(shape(host)).toEqual([
        HEADER,
        'body: /api/a "one, again", /api/b "two, again"',
      ])
      expect(Object.fromEntries(renders)).toEqual({
        '/api/a': 2,
        '/api/b': 2,
      })
      expect(new Set(drawn)).toEqual(new Set(['body']))
    })

    it('does not update when the re-reads answer as before', async () => {
      const a = address<string>('/api/a')
      const b = address<string>('/api/b')
      const host = await visitAndLeave([a, b], ['one', 'two'])

      rerender(<TestPage reads={[a.read, b.read]} />)
      a.requests[1].resolve('one')
      b.requests[1].resolve('two')
      await land()
      pass(WAIT_DRAWN_AFTER_MS)
      expect(shape(host)).toEqual([HEADER, 'body: /api/a "one", /api/b "two"'])
      expect(Object.fromEntries(renders)).toEqual({
        '/api/a': 1,
        '/api/b': 1,
      })
      expect(new Set(drawn)).toEqual(new Set(['body']))
    })
  })

  describe('a stale answer (§4.3)', () => {
    it('waits as a first visit when an answer is stale on arrival', async () => {
      const a = address<string>('/api/a')
      const host = await visitAndLeave([a], ['one'])
      await invalidate(a.read)
      expect(entryOf(a.read)?.stale).toBe(true)

      rerender(<TestPage reads={[a.read]} />)
      expect(drawn[0]).toBe('nothing')
      expect(shape(host)).toEqual([HEADER])
      expect(a.fetch).toHaveBeenCalledTimes(2)
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(shape(host)).toEqual([HEADER])
      pass(1)
      expect(shape(host)).toEqual([HEADER, 'status: Loading…'])

      a.requests[1].resolve('two')
      await land()
      expect(shape(host)).toEqual([HEADER, 'body: /api/a "two"'])
    })

    it('keeps drawing an answer that goes stale while drawn until the re-read lands', async () => {
      const a = address<string>('/api/a')
      const host = mount(<TestPage reads={[a.read]} />)
      a.requests[0].resolve('one')
      await land()
      expect(shape(host)).toEqual([HEADER, 'body: /api/a "one"'])

      const reread = invalidate(a.read)
      expect(a.fetch).toHaveBeenCalledTimes(2)
      // Drawn again for a reason of its own, so the page reads the entry
      // the store has marked without telling it.
      rerender(<TestPage reads={[a.read]} title="Test page, again" />)
      expect(entryOf(a.read)?.stale).toBe(true)
      pass(WAIT_DRAWN_AFTER_MS)
      expect(shape(host)).toEqual([
        'header: Test page, again',
        'body: /api/a "one"',
      ])

      a.requests[1].resolve('two')
      await act(() => reread)
      expect(shape(host)).toEqual([
        'header: Test page, again',
        'body: /api/a "two"',
      ])
      expect(drawn).not.toContain('loading')
    })
  })

  it('waits when some are answered and some not, and re-reads the answered ones in the same set (§4.4)', async () => {
    const a = address<string>('/api/a')
    const b = address<string>('/api/b')
    const host = await visitAndLeave([a], ['one'])

    rerender(<TestPage reads={[a.read, b.read]} />)
    expect(drawn[0]).toBe('nothing')
    expect(a.fetch).toHaveBeenCalledTimes(2)
    expect(b.fetch).toHaveBeenCalledTimes(1)

    b.requests[0].resolve('two')
    await land()
    expect(shape(host)).toEqual([HEADER])

    a.requests[1].resolve('one, again')
    await land()
    expect(shape(host)).toEqual([
      HEADER,
      'body: /api/a "one, again", /api/b "two"',
    ])
    expect(Object.fromEntries(renders)).toEqual({ '/api/a': 1, '/api/b': 1 })
  })

  it('holds the body for a read not named yet, then waits for that read alone (§4.5)', async () => {
    const a = address<string>('/api/a')
    const b = address<string>('/api/b')
    const host = mount(<TestPage reads={[a.read, undefined]} />)
    a.requests[0].resolve('one')
    await land()
    expect(shape(host)).toEqual([HEADER])
    pass(WAIT_DRAWN_AFTER_MS)
    expect(shape(host)).toEqual([HEADER, 'status: Loading…'])

    rerender(<TestPage reads={[a.read, b.read]} />)
    expect(a.fetch).toHaveBeenCalledTimes(1)
    expect(b.fetch).toHaveBeenCalledTimes(1)
    expect(shape(host)).toEqual([HEADER, 'status: Loading…'])

    b.requests[0].resolve('two')
    await land()
    expect(shape(host)).toEqual([HEADER, 'body: /api/a "one", /api/b "two"'])
  })

  // A sibling's mount effect runs before the page's read and subscription,
  // finding nothing to invalidate, or after both, when the page draws the
  // address and the invalidation sends a newer request: never between them,
  // where it would drop the page's request and leave it waiting with
  // nothing out.
  it.each(['before', 'after'] as const)(
    'draws its read’s answer though a sibling drawn %s it invalidates the read as it mounts',
    async (where) => {
      const a = address<string>('/api/a')
      const invalidator = <Invalidate read={a.read} />
      const host = mount(
        <>
          {where === 'before' && invalidator}
          <TestPage reads={[a.read]} />
          {where === 'after' && invalidator}
        </>,
      )
      expect(a.fetch).toHaveBeenCalledTimes(where === 'before' ? 1 : 2)
      a.requests.forEach((request, i) => {
        request.resolve(`answer ${i}`)
      })
      await land()
      const last = a.requests.length - 1
      expect(shape(host)).toEqual([HEADER, `body: /api/a "answer ${last}"`])
    },
  )

  it('reads again, with the read newly named, one invalidated as the list changed', async () => {
    const a = address<string>('/api/a')
    const b = address<string>('/api/b')
    // `Invalidate` mounts in the commit that changes the list, before the
    // page's effects: after the cleanup of the page's subscription to its
    // old list, and before it subscribes to the new one.
    const frame = (
      reads: readonly (Read<unknown> | undefined)[],
      invalidated?: Read<unknown>,
    ) => (
      <>
        {invalidated && <Invalidate read={invalidated} />}
        <TestPage reads={reads} />
      </>
    )
    const host = mount(frame([a.read, undefined]))
    a.requests[0].resolve('one')
    await land()

    rerender(frame([a.read, b.read], a.read))
    expect(entryOf(a.read)?.stale).toBe(true)
    expect(a.fetch).toHaveBeenCalledTimes(2)
    expect(b.fetch).toHaveBeenCalledTimes(1)

    b.requests[0].resolve('two')
    await land()
    expect(shape(host)).toEqual([HEADER])
    a.requests[1].resolve('one, again')
    await land()
    expect(shape(host)).toEqual([
      HEADER,
      'body: /api/a "one, again", /api/b "two"',
    ])
  })

  it('reads again a read that leaves the list and comes back, and waits again for one that had only failed', async () => {
    const a = address<string>('/api/a')
    const host = mount(<TestPage reads={[a.read]} />)
    a.requests[0].reject(new Error('the list was refused'))
    await land()
    expect(shape(host)).toEqual([
      HEADER,
      'body: /api/a none; failed: the list was refused',
    ])

    // Hidden and shown again, as `/account`'s panel names its reads.
    rerender(<TestPage reads={[]} />)
    expect(shape(host)).toEqual([HEADER, 'body: '])
    rerender(<TestPage reads={[a.read]} />)
    expect(a.fetch).toHaveBeenCalledTimes(2)
    expect(shape(host)).toEqual([HEADER])
    a.requests[1].resolve('one')
    await land()
    expect(shape(host)).toEqual([HEADER, 'body: /api/a "one"'])
  })

  it('waits again for a read whose failure landed while it was off the list', async () => {
    const a = address<string>('/api/a')
    const host = mount(<TestPage reads={[a.read]} />)
    rerender(<TestPage reads={[]} />)
    a.requests[0].reject(new Error('the list was refused'))
    await land()

    rerender(<TestPage reads={[a.read]} />)
    expect(a.fetch).toHaveBeenCalledTimes(2)
    expect(shape(host)).toEqual([HEADER])
    a.requests[1].resolve('one')
    await land()
    expect(shape(host)).toEqual([HEADER, 'body: /api/a "one"'])
  })

  describe('a failure (§4.6)', () => {
    it('draws the body once a first visit’s reads have settled, a failure among them, with the failure handed over', async () => {
      const a = address<string>('/api/a')
      const b = address<string>('/api/b')
      const host = mount(<TestPage reads={[a.read, b.read]} />)
      a.requests[0].reject(new Error('the list was refused'))
      b.requests[0].resolve('two')
      await land()
      expect(shape(host)).toEqual([
        HEADER,
        'body: /api/a none; failed: the list was refused, /api/b "two"',
      ])
      expect(Object.fromEntries(renders)).toEqual({
        '/api/a': 1,
        '/api/b': 1,
      })
    })

    it('waits on arrival for a read that has only failed, until this visit’s read settles', async () => {
      const a = address<string>('/api/a')
      const host = mount(<TestPage reads={[a.read]} />)
      a.requests[0].reject(new Error('the list was refused'))
      await land()
      expect(shape(host)).toEqual([
        HEADER,
        'body: /api/a none; failed: the list was refused',
      ])
      rerender(null)
      drawn = []

      rerender(<TestPage reads={[a.read]} />)
      expect(drawn[0]).toBe('nothing')
      expect(shape(host)).toEqual([HEADER])
      a.requests[1].resolve('one')
      await land()
      expect(shape(host)).toEqual([HEADER, 'body: /api/a "one"'])
    })

    it('draws an answer kept from an earlier visit at once without the failure kept beside it, and hands over a failure once this visit’s read has settled', async () => {
      const a = address<string>('/api/a')
      const host = await visitAndLeave([a], ['one'])
      rerender(<TestPage reads={[a.read]} />)
      a.requests[1].reject(new Error('the server is down'))
      await land()
      rerender(null)
      drawn = []

      rerender(<TestPage reads={[a.read]} />)
      expect(drawn[0]).toBe('body')
      expect(shape(host)).toEqual([HEADER, 'body: /api/a "one"'])
      a.requests[2].reject(new Error('the server is still down'))
      await land()
      expect(shape(host)).toEqual([
        HEADER,
        'body: /api/a "one"; failed: the server is still down',
      ])
    })

    it('keeps the answer drawn beside a failed re-read', async () => {
      const a = address<string>('/api/a')
      const host = await visitAndLeave([a], ['one'])
      rerender(<TestPage reads={[a.read]} />)
      a.requests[1].reject(new Error('the server is down'))
      await land()
      expect(shape(host)).toEqual([
        HEADER,
        'body: /api/a "one"; failed: the server is down',
      ])
    })
  })

  it('is ready at once for an empty list', () => {
    const host = mount(<TestPage reads={[]} />)
    expect(drawn[0]).toBe('body')
    pass(WAIT_DRAWN_AFTER_MS)
    expect(shape(host)).toEqual([HEADER, 'body: '])
    expect(drawn).not.toContain('loading')
  })

  it('keeps drawing a kept answer that was the oldest of the idle ones when it replaces a page whose going puts one past the bound', async () => {
    // Nothing draws `arriving`, and it was read first: of the IDLE_KEPT + 1
    // idle entries once `leaving` goes, it is the oldest, the one the bound
    // would drop. The swap is one commit, whose cleanups (`leaving` let go)
    // run before its mount effects (`arriving` subscribed), after `arriving`
    // has drawn from what is kept.
    const arriving = address<string>('/api/arriving')
    const read = readSet([arriving.read])
    arriving.requests[0].resolve('kept')
    await read
    for (let i = 0; i < IDLE_KEPT - 1; i++) {
      const idle = address<string>(`/api/idle/${i}`)
      const one = readSet([idle.read])
      idle.requests[0].resolve(`idle ${i}`)
      await one
    }
    const leaving = address<string>('/api/leaving')
    const host = mount(<TestPage key="leaving" reads={[leaving.read]} />)
    leaving.requests[0].resolve('leaving')
    await land()
    expect(shape(host)).toEqual([HEADER, 'body: /api/leaving "leaving"'])
    drawn = []

    rerender(<TestPage key="arriving" reads={[arriving.read]} />)
    expect(shape(host)).toEqual([HEADER, 'body: /api/arriving "kept"'])
    await land()
    expect(shape(host)).toEqual([HEADER, 'body: /api/arriving "kept"'])
    expect(entryOf(arriving.read)?.answer).toBe('kept')
    expect(new Set(drawn)).toEqual(new Set(['body']))
  })
})
