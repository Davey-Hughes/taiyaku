import { act, StrictMode, useEffect } from 'react'
import { describe, expect, it, vi } from 'vitest'
import {
  useConstant,
  useDecodeQueue,
  useDrawn,
  useDrawnSrcs,
  useDrawnWait,
  useFlushOnHide,
  useLatest,
  usePrefsMap,
  useProgressWrite,
  useReadingClock,
  useScrollToCurrent,
  useWindowWidth,
  WAIT_DRAWN_AFTER_MS,
} from './hooks'
import type { Mode } from './pairing'
import type { ReaderModule } from './module'
import { mount, rerender } from './testing/mount'

/**
 * `hooks.ts`, mounted.
 *
 * This file is the condition on which `hooks.ts` exists at all. The
 * extraction map's Rule 2 refuses custom hooks because a hook's branches can
 * only be reached by mounting something, so moving logic into one relabels
 * uncovered statements rather than covering them — an objection that only
 * holds while nobody mounts them. Every hook in that file is mounted below,
 * in `jsdom`, which is a project CI runs.
 *
 * Deliberately not `renderHook` from a testing library: this repository's
 * rule is that a dev-only dependency needs a stated reason, and a probe
 * component that pushes each render's value into an array is four lines and
 * shows exactly what it does.
 */

/**
 * Mount a hook and hand back every value it has returned, newest last.
 *
 * `render(props)` re-renders the *same* instance rather than mounting a
 * fresh one, which is the whole point for a hook whose subject is what
 * survives a render — a second `mount()` would remount and reset it.
 */
function probe<P, T>(use: (props: P) => T, initial: NoInfer<P>) {
  const seen: T[] = []
  function Probe({ props }: { props: P }) {
    seen.push(use(props))
    return null
  }
  mount(<Probe props={initial} />)
  return {
    seen,
    /** The most recent value the hook returned. */
    last: () => seen[seen.length - 1] as T,
    render: (props: NoInfer<P>) => rerender(<Probe props={props} />),
  }
}

describe('useLatest', () => {
  it('hands a closure frozen at subscription the current value', () => {
    // The shape `App.tsx`'s keydown listener has: subscribed once, never
    // re-subscribed, and reading state through the ref rather than through
    // its own closure.
    const read: (string | undefined)[] = []
    function Frozen({ value }: { value: string }) {
      const ref = useLatest(value)
      // biome-ignore lint/correctness/useExhaustiveDependencies: subscribing once is the subject of this test — the ref is how the frozen closure reaches the current value.
      useEffect(() => {
        const onPing = () => read.push(ref.current)
        window.addEventListener('ping', onPing)
        return () => window.removeEventListener('ping', onPing)
      }, [])
      return null
    }
    mount(<Frozen value="first" />)
    act(() => void window.dispatchEvent(new Event('ping')))
    rerender(<Frozen value="second" />)
    act(() => void window.dispatchEvent(new Event('ping')))
    expect(read).toEqual(['first', 'second'])
  })

  it('starts at the value of the render that created it', () => {
    // No window, however brief, where the ref disagrees with its own first
    // render: a listener that fires before any effect has run still reads
    // something true. Read *during* render, because by the time a test can
    // look from outside the effect has already run and any seed would do.
    const duringRender: (number | undefined)[] = []
    function Seed({ value }: { value: number }) {
      duringRender.push(useLatest(value).current)
      return null
    }
    mount(<Seed value={7} />)
    expect(duringRender[0]).toBe(7)
  })

  it('keeps one ref object across renders', () => {
    const p = probe((value: number) => useLatest(value), 1)
    const first = p.last()
    p.render(2)
    expect(p.last()).toBe(first)
  })
})

describe('useDrawn', () => {
  it('is false during the first render, true once drawn, and false once gone, StrictMode or not', () => {
    for (const strict of [false, true]) {
      let held: { readonly current: boolean } | undefined
      const during: boolean[] = []
      function Drawn() {
        held = useDrawn()
        during.push(held.current)
        return null
      }
      mount(
        strict ? (
          <StrictMode>
            <Drawn />
          </StrictMode>
        ) : (
          <Drawn />
        ),
      )
      expect(during[0]).toBe(false)
      expect(held?.current).toBe(true)
      rerender(<div />)
      expect(held?.current).toBe(false)
    }
  })
})

describe('useDrawnWait', () => {
  const waits = () =>
    probe(({ waiting }: { waiting: boolean }) => useDrawnWait(waiting), {
      waiting: false,
    })

  it('draws a wait once it has held for its delay, and not from the render it ends', () => {
    vi.useFakeTimers()
    try {
      const p = waits()
      p.render({ waiting: true })
      act(() => vi.advanceTimersByTime(WAIT_DRAWN_AFTER_MS - 1))
      expect(p.seen).not.toContain(true)
      act(() => vi.advanceTimersByTime(1))
      expect(p.last()).toBe(true)

      const ended = p.seen.length
      p.render({ waiting: false })
      expect(p.seen.slice(ended)).not.toContain(true)
      expect(p.seen.length).toBeGreaterThan(ended)
    } finally {
      vi.useRealTimers()
    }
  })

  it('draws no wait that ends before its delay, and counts the next one from its own start', () => {
    vi.useFakeTimers()
    try {
      const p = waits()
      p.render({ waiting: true })
      act(() => vi.advanceTimersByTime(WAIT_DRAWN_AFTER_MS - 1))
      p.render({ waiting: false })
      p.render({ waiting: true })
      act(() => vi.advanceTimersByTime(WAIT_DRAWN_AFTER_MS - 1))
      expect(p.seen).not.toContain(true)
      act(() => vi.advanceTimersByTime(1))
      expect(p.last()).toBe(true)
    } finally {
      vi.useRealTimers()
    }
  })

  it('counts a wait that follows a drawn one from its own start', () => {
    vi.useFakeTimers()
    try {
      const p = waits()
      p.render({ waiting: true })
      act(() => vi.advanceTimersByTime(WAIT_DRAWN_AFTER_MS))
      expect(p.last()).toBe(true)
      p.render({ waiting: false })

      const next = p.seen.length
      p.render({ waiting: true })
      act(() => vi.advanceTimersByTime(WAIT_DRAWN_AFTER_MS - 1))
      expect(p.seen.slice(next)).not.toContain(true)
      act(() => vi.advanceTimersByTime(1))
      expect(p.last()).toBe(true)
    } finally {
      vi.useRealTimers()
    }
  })
})

describe('useConstant', () => {
  it('builds once and keeps the same value across renders', () => {
    const create = vi.fn(() => ({ built: true }))
    const p = probe(() => useConstant(create), undefined)
    const built = p.last()
    p.render(undefined)
    p.render(undefined)
    expect(p.last()).toBe(built)
    expect(create).toHaveBeenCalledTimes(1)
  })

  it('does not evaluate its argument on later renders', () => {
    // The defect this hook exists to prevent: `useRef(build())` evaluates
    // `build()` on every render and throws all but the first away, which for
    // a debounce means an unarmed timer allocated per render.
    let evaluations = 0
    const p = probe(
      () =>
        useConstant(() => {
          evaluations += 1
          return evaluations
        }),
      undefined,
    )
    p.render(undefined)
    p.render(undefined)
    expect(evaluations).toBe(1)
    expect(p.last()).toBe(1)
  })
})

describe('useDecodeQueue', () => {
  /** A job that never settles, so the queue's occupancy can be read. */
  const forever = () => new Promise<void>(() => undefined)

  it('caps concurrency at the value it was given', () => {
    const p = probe((max: number) => useDecodeQueue(max), 1)
    const queue = p.last()
    void queue.run(forever)
    void queue.run(forever)
    // One running, one waiting for the slot it holds.
    expect(queue.pending()).toBe(1)
  })

  it('rebuilds when the cap changes, so a setting takes effect live', () => {
    const p = probe((max: number) => useDecodeQueue(max), 1)
    const first = p.last()
    p.render(4)
    const second = p.last()
    expect(second).not.toBe(first)
    void second.run(forever)
    void second.run(forever)
    expect(second.pending()).toBe(0)
  })

  it('keeps one queue while the cap is unchanged', () => {
    const p = probe((max: number) => useDecodeQueue(max), 2)
    const settled = p.last()
    p.render(2)
    expect(p.last()).toBe(settled)
  })
})

describe('useDrawnSrcs', () => {
  it('records what each cell reports', () => {
    const p = probe(() => useDrawnSrcs(), undefined)
    act(() => p.last()[1]('first', '/a.png'))
    act(() => p.last()[1]('second', '/b.png'))
    expect(p.last()[0]).toEqual({ first: '/a.png', second: '/b.png' })
  })

  it('returns the same record for a report that changes nothing', () => {
    // Every `PageView` reports on every present, including the ones that
    // present what they were already drawing. A fresh object for each would
    // re-render the whole page area for no visible change.
    const p = probe(() => useDrawnSrcs(), undefined)
    act(() => p.last()[1]('first', '/a.png'))
    const record = p.last()[0]
    act(() => p.last()[1]('first', '/a.png'))
    expect(p.last()[0]).toBe(record)
  })

  it('is the record, not the render count, that this pins', () => {
    // Asserting "no re-render" here would fail, and the reason is React's
    // rather than this hook's: an updater returning the state it was given
    // still runs the component once more before bailing out, so a repeat
    // report costs exactly one render whether the object is fresh or not.
    // What the same object buys is everything downstream of that render —
    // nothing keyed on this record's identity sees a change.
    const p = probe(() => useDrawnSrcs(), undefined)
    act(() => p.last()[1]('second', '/b.png'))
    const before = p.seen.length
    act(() => p.last()[1]('second', '/b.png'))
    expect(p.seen.length).toBe(before + 1)
  })
})

describe('usePrefsMap', () => {
  const modules: ReaderModule<unknown>[] = [
    { id: 'alpha', label: 'Alpha', defaults: { on: true } },
    { id: 'beta', label: 'Beta', defaults: { size: 3 } },
  ]

  it('loads every module once and persists the whole map on mount', () => {
    const load = vi.fn((id: string, defaults: object) =>
      id === 'beta' ? { size: 9 } : defaults,
    )
    const save = vi.fn()
    const p = probe(() => usePrefsMap(modules, load, save), undefined)
    expect(p.last()[0]).toEqual({ alpha: { on: true }, beta: { size: 9 } })
    expect(load).toHaveBeenCalledTimes(2)
    expect(save.mock.calls).toEqual([
      ['alpha', { on: true }],
      ['beta', { size: 9 }],
    ])
  })

  it('persists every slice when any one of them changes', () => {
    // Not a diff for the id that moved: a whole-map write cannot drift from
    // which slice actually changed the way a hand-maintained one could.
    const save = vi.fn()
    const p = probe(
      () => usePrefsMap(modules, (_id, defaults) => defaults, save),
      undefined,
    )
    save.mockClear()
    act(() => p.last()[1]((map) => ({ ...map, alpha: { on: false } })))
    expect(save.mock.calls).toEqual([
      ['alpha', { on: false }],
      ['beta', { size: 3 }],
    ])
  })

  it('does not reload or re-persist on a render that changed nothing', () => {
    const load = vi.fn((_id: string, defaults: object) => defaults)
    const save = vi.fn()
    const p = probe(() => usePrefsMap(modules, load, save), undefined)
    save.mockClear()
    p.render(undefined)
    expect(load).toHaveBeenCalledTimes(2)
    expect(save).not.toHaveBeenCalled()
  })
})

describe('useWindowWidth', () => {
  it('reads the window at mount and follows a resize', () => {
    const p = probe(() => useWindowWidth(1280), undefined)
    expect(p.last()).toBe(window.innerWidth)
    act(() => {
      Object.defineProperty(window, 'innerWidth', {
        value: 640,
        configurable: true,
      })
      window.dispatchEvent(new Event('resize'))
    })
    expect(p.last()).toBe(640)
  })

  it('stops listening when it goes away', () => {
    // Nothing observable happens when a leaked `resize` handler sets state on
    // an unmounted root — React 19 discards it in silence — so the detector
    // has to be the removal itself.
    const removed: string[] = []
    // Bound before the spy replaces it, so the real removal still happens and
    // this test leaves no listener behind of its own.
    const real = window.removeEventListener.bind(window)
    const spy = vi
      .spyOn(window, 'removeEventListener')
      .mockImplementation((type, listener, options) => {
        removed.push(type)
        real(type, listener, options)
      })
    try {
      probe(() => useWindowWidth(1280), undefined)
      rerender(null)
      expect(removed).toContain('resize')
    } finally {
      spy.mockRestore()
    }
  })
})

describe('useFlushOnHide', () => {
  const flushable = () => ({ flush: vi.fn() })
  /** jsdom reports `visible` by default; this is the only way to say otherwise. */
  function visibility(state: DocumentVisibilityState) {
    Object.defineProperty(document, 'visibilityState', {
      value: state,
      configurable: true,
    })
  }

  it('sends both pending writes when the tab is hidden', () => {
    const position = flushable()
    const notes = flushable()
    probe(() => useFlushOnHide(position, notes), undefined)
    visibility('hidden')
    act(() => void document.dispatchEvent(new Event('visibilitychange')))
    expect(position.flush).toHaveBeenCalledTimes(1)
    expect(notes.flush).toHaveBeenCalledTimes(1)
  })

  it('sends nothing when the tab merely becomes visible again', () => {
    const position = flushable()
    const notes = flushable()
    probe(() => useFlushOnHide(position, notes), undefined)
    visibility('visible')
    act(() => void document.dispatchEvent(new Event('visibilitychange')))
    expect(position.flush).not.toHaveBeenCalled()
    expect(notes.flush).not.toHaveBeenCalled()
  })

  it('sends on pagehide too, for the close visibilitychange does not cover', () => {
    // Some browsers route a direct close of a background tab through
    // `pagehide` with no preceding `visibilitychange`, which is why both are
    // subscribed rather than either alone.
    const position = flushable()
    const notes = flushable()
    probe(() => useFlushOnHide(position, notes), undefined)
    visibility('visible')
    act(() => void window.dispatchEvent(new Event('pagehide')))
    expect(position.flush).toHaveBeenCalledTimes(1)
    expect(notes.flush).toHaveBeenCalledTimes(1)
  })

  it('sends both pending writes when the component itself goes away', () => {
    // The unmount is the third way the reader leaves, and until 2026-08-09 it
    // was the one nothing covered: the two debounces `App.tsx` hands in were
    // left armed, so a `putProgress` or a `putNotes` could fire up to 700 ms
    // after the component that scheduled it had gone. Flushed rather than
    // cancelled — see this hook's own doc for the argument at each call site.
    const position = flushable()
    const notes = flushable()
    probe(() => useFlushOnHide(position, notes), undefined)
    // Nothing has hidden anything: the component simply stopped existing.
    visibility('visible')
    rerender(null)
    expect(position.flush).toHaveBeenCalledTimes(1)
    expect(notes.flush).toHaveBeenCalledTimes(1)
  })

  it('stops listening when it goes away', () => {
    // Both listeners are on `document`/`window` rather than on React's tree,
    // so a missing cleanup really would keep flushing after the reader had
    // gone — which makes this a detector rather than a description.
    const position = flushable()
    const notes = flushable()
    probe(() => useFlushOnHide(position, notes), undefined)
    rerender(null)
    // The unmount itself flushes once, on purpose (above). What this is about
    // is the events *after* it, so the count starts again from there.
    position.flush.mockClear()
    notes.flush.mockClear()
    visibility('hidden')
    act(() => void document.dispatchEvent(new Event('visibilitychange')))
    act(() => void window.dispatchEvent(new Event('pagehide')))
    expect(position.flush).not.toHaveBeenCalled()
    expect(notes.flush).not.toHaveBeenCalled()
  })
})

describe('useProgressWrite', () => {
  interface At {
    work?: string
    edition?: string
    slot?: number
  }
  const run = (write: (work: string, at: object) => void, initial: At) =>
    probe(
      (at: At) => useProgressWrite(at.work, at.edition, at.slot, write),
      initial,
    )

  it('writes nothing before a work has opened', () => {
    const write = vi.fn()
    run(write, {})
    expect(write).not.toHaveBeenCalled()
  })

  it('skips the restoring load, then writes every move after it', () => {
    // The one run this skips, and the reason it exists: a `PUT` of exactly
    // what `getProgress` returned, on every load.
    const write = vi.fn()
    const p = run(write, { work: 'w', edition: 'ja', slot: 3 })
    expect(write).not.toHaveBeenCalled()
    p.render({ work: 'w', edition: 'ja', slot: 4 })
    p.render({ work: 'w', edition: 'ja', slot: 5 })
    expect(write.mock.calls).toEqual([
      ['w', { edition: 'ja', slot: 4 }],
      ['w', { edition: 'ja', slot: 5 }],
    ])
  })

  it('skips exactly one run, not every run until things settle', () => {
    // A second skip would silently lose a real page turn, so the guard is
    // "the first" rather than "while restoring".
    const write = vi.fn()
    const p = run(write, {})
    p.render({ work: 'w', edition: 'ja', slot: 0 })
    p.render({ work: 'w', edition: 'ja', slot: 1 })
    expect(write.mock.calls).toEqual([['w', { edition: 'ja', slot: 1 }]])
  })

  it('does not write for a render that moved nothing', () => {
    const write = vi.fn()
    const p = run(write, { work: 'w', edition: 'ja', slot: 3 })
    p.render({ work: 'w', edition: 'ja', slot: 4 })
    write.mockClear()
    p.render({ work: 'w', edition: 'ja', slot: 4 })
    expect(write).not.toHaveBeenCalled()
  })
})

describe('useReadingClock', () => {
  interface At {
    edition?: string
    slot?: number
  }
  const run = (initial: At) =>
    probe((at: At) => useReadingClock(at.edition, at.slot, 30_000), initial)

  it('records nothing before a work has opened', () => {
    // `App` mounts before the sidecar answers, and the mount itself is not a
    // turn any more than the load that follows it is.
    const p = run({})
    expect(p.last().turns.current).toEqual([])
  })

  it('records no turn for the load that opened the work', () => {
    // Measured, not assumed: counting the load as a turn made the first
    // estimate this project ever rendered `~6m left`, derived entirely from
    // load latency before a key had been pressed.
    const p = run({ edition: 'ja', slot: 12 })
    expect(p.last().turns.current).toEqual([])
  })

  it('records a turn for every move after that', () => {
    const p = run({ edition: 'ja', slot: 12 })
    p.render({ edition: 'ja', slot: 13 })
    p.render({ edition: 'ja', slot: 14 })
    expect(p.last().turns.current.length).toBe(2)
  })

  it('records nothing for a render that moved nothing', () => {
    const p = run({ edition: 'ja', slot: 12 })
    p.render({ edition: 'ja', slot: 13 })
    p.render({ edition: 'ja', slot: 13 })
    expect(p.last().turns.current.length).toBe(1)
  })

  it('fixes the session start at mount and keeps it', () => {
    const p = run({ edition: 'ja', slot: 12 })
    const started = p.last().startedAt
    p.render({ edition: 'ja', slot: 13 })
    expect(p.last().startedAt).toBe(started)
  })

  it('re-renders on the tick, so an estimate ticks without a page turn', () => {
    vi.useFakeTimers()
    try {
      const p = probe(
        () => useReadingClock('ja', 12, 30_000),
        undefined as undefined,
      )
      const before = p.seen.length
      act(() => void vi.advanceTimersByTime(90_000))
      expect(p.seen.length).toBeGreaterThan(before)
    } finally {
      vi.useRealTimers()
    }
  })
})

describe('useScrollToCurrent', () => {
  interface At {
    mode: Mode
    slot?: number
  }
  function run(initial: At) {
    const scrolled: unknown[] = []
    const p = probe((at: At) => {
      const ref = useScrollToCurrent(at.mode, 'ja', at.slot, 'ja')
      // A real element, so the optional chain is exercised rather than
      // short-circuited: a hook that only ever saw `null` would pass this
      // file while never scrolling anything.
      const el = document.createElement('div')
      el.scrollIntoView = (arg?: unknown) => void scrolled.push(arg)
      ref.current = el
      return ref
    }, initial)
    return { ...p, scrolled }
  }

  it('scrolls to the current group whenever the position moves', () => {
    const p = run({ mode: 'scroll', slot: 0 })
    p.render({ mode: 'scroll', slot: 150 })
    expect(p.scrolled).toEqual([{ block: 'start' }, { block: 'start' }])
  })

  it('does nothing in the modes that mount only the current group', () => {
    const p = run({ mode: 'two', slot: 0 })
    p.render({ mode: 'two', slot: 150 })
    expect(p.scrolled).toEqual([])
  })

  it('scrolls on the change into scroll mode itself', () => {
    // Entering scroll mode at slot 150 shows slot 0 without this.
    const p = run({ mode: 'single', slot: 150 })
    p.render({ mode: 'scroll', slot: 150 })
    expect(p.scrolled).toEqual([{ block: 'start' }])
  })
})
