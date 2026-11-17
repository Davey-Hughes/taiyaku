import { act, useRef } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { columnsIn, spreadGap } from '../src/continueRow'
import { useReseat } from '../src/reseat'
import {
  RESEAT_EASING,
  RESEAT_MS,
  rowLaid,
  scrollPutBack,
} from '../src/reseatTiming'
import { mount, rerender } from './support/mount'

/**
 * `useReseat` reads the layout and animates, neither of which jsdom does,
 * so the browser proves the motion and the coverage run never reaches it.
 * These hand the hook what a browser would: the tokens, the grid's rows,
 * each box, a `ResizeObserver` and `Element.animate` the test answers, and
 * a frame clock the test runs.
 */

type Observer = {
  observed: Set<Element>
  disconnected: boolean
  answer: (width: number, height: number) => void
}

type Started = {
  /** The tile whose cover moves. */
  tile: Element
  /** The element the move runs on. */
  moved: Element
  from: string
  options: KeyframeAnimationOptions
  animation: Animation & { progress: number; finish: () => void }
}

let observers: Observer[]
let frames: Map<number, FrameRequestCallback>
let started: Started[]
let tokens: Record<string, string>
/** The grid's resolved rows, as `grid-template-rows` computes. */
let rows: string
/** Where the list's box is drawn in the view. */
let box: { left: number; top: number }
/** The size the list's box is laid at. */
let size: { width: number; height: number }
/** The document's scroll, which a scroll the browser anchors moves. */
let scrollTop: number
/** The columns and pitch the page lays the covers at, for their boxes. */
let page: { columns: number; pitch: number }
/** How far the page draws each cover from where its column puts it. */
let off: number
let reduce: boolean

/**
 * Lists of `count` covers, `lists` of them, each attached while `drawn`.
 * Made in a function because `useComponentExportOnlyModules` refuses an
 * unexported component at module scope, and `noExportsInTest` refuses the
 * export.
 */
function listsOf() {
  function List({
    drawn,
    count,
    bare,
  }: {
    drawn: boolean
    count: number
    bare: boolean
  }) {
    const list = useRef<HTMLUListElement>(null)
    useReseat(list, drawn)
    return (
      <ul ref={list}>
        {Array.from({ length: count }, (_, i) => (
          <li key={i}>{bare || <span />}</li>
        ))}
      </ul>
    )
  }
  function Lists({
    lists = 1,
    drawn = true,
    count = 14,
    bare = false,
  }: {
    lists?: number
    drawn?: boolean
    count?: number
    /** Tiles with nothing in them. */
    bare?: boolean
  }) {
    return Array.from({ length: lists }, (_, i) => (
      <List key={i} drawn={drawn} count={count} bare={bare} />
    ))
  }
  return Lists
}

const Lists = listsOf()

/**
 * Lays `count` Smooth covers across `width`, 250 px rows 16 px apart, and
 * answers the observer as the browser does after that change of size.
 */
function laidAt(width: number, count = 14) {
  const track = 144
  const gap = 16
  const columns = columnsIn(width, track, gap)
  page = { columns, pitch: track + spreadGap(width, track, gap) }
  const many = Math.ceil(count / columns)
  rows = Array.from({ length: many }, () => '250px').join(' ')
  act(() => {
    size = { width, height: many * 266 - 16 }
    for (const each of observers) each.answer(size.width, size.height)
  })
}

/** Runs the callbacks requested for the next frame. */
const frame = () =>
  act(() => {
    const due = [...frames.values()]
    frames.clear()
    for (const run of due) run(0)
  })

const listIn = (host: HTMLElement) => host.querySelector('ul') as HTMLElement

/** The numbers in a keyframe's transform, in order. */
const numbersIn = (transform = '') =>
  (transform.match(/-?[\d.]+/g) ?? []).map(Number)

/** Each started move's first keyframe, by the index of its cover. */
const moves = () =>
  new Map(
    started.map(({ tile, from }) => [
      [...(tile.parentElement?.children ?? [])].indexOf(tile),
      from,
    ]),
  )

beforeEach(() => {
  observers = []
  frames = new Map()
  started = []
  tokens = { '--tile-min': '144px', '--tile-gap': '16px', '--tile-fill': '0' }
  rows = ''
  box = { left: 40, top: 100 }
  size = { width: 0, height: 0 }
  scrollTop = 0
  Object.defineProperty(document.documentElement, 'scrollTop', {
    configurable: true,
    get: () => scrollTop,
  })
  page = { columns: 1, pitch: 0 }
  off = 0
  reduce = false
  vi.spyOn(window, 'getComputedStyle').mockImplementation(
    () =>
      ({
        getPropertyValue: (name: string) => tokens[name] ?? '',
        get gridTemplateRows() {
          return rows
        },
      }) as unknown as CSSStyleDeclaration,
  )
  vi.spyOn(
    HTMLUListElement.prototype,
    'getBoundingClientRect',
  ).mockImplementation(() => ({ ...box, ...size }) as DOMRect)
  // A tile's content is drawn where its tile is laid.
  for (const each of [HTMLLIElement, HTMLSpanElement]) {
    vi.spyOn(each.prototype, 'getBoundingClientRect').mockImplementation(
      function (this: HTMLElement) {
        const tile = this.closest('li')
        const index = [...(tile?.parentElement?.children ?? [])].indexOf(
          tile as Element,
        )
        return {
          left: box.left + (index % page.columns) * page.pitch + off,
          top: box.top + Math.floor(index / page.columns) * 266,
        } as DOMRect
      },
    )
  }
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: query === '(prefers-reduced-motion: reduce)' && reduce,
  }))
  Object.defineProperty(Element.prototype, 'animate', {
    configurable: true,
    value(
      this: Element,
      keyframes: Keyframe[],
      options: KeyframeAnimationOptions,
    ) {
      const listeners: Record<string, (() => void)[]> = {
        finish: [],
        cancel: [],
      }
      const send = (type: string) => {
        for (const each of listeners[type]) each()
      }
      const animation = {
        startTime: null,
        progress: 0,
        effect: {
          getComputedTiming: () => ({ progress: animation.progress }),
        },
        addEventListener: (type: string, listener: () => void) => {
          listeners[type].push(listener)
        },
        cancel: () => send('cancel'),
        finish: () => send('finish'),
      }
      started.push({
        tile: this.closest('li') ?? this,
        moved: this,
        from: String(keyframes[0].transform),
        options,
        animation: animation as unknown as Started['animation'],
      })
      return animation
    },
  })
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observed = new Set<Element>()
      disconnected = false
      answer: Observer['answer']
      constructor(callback: ResizeObserverCallback) {
        this.answer = (width, height) => {
          const entries = [...this.observed].map((target) => ({
            target,
            contentRect: { width, height },
          }))
          if (entries.length === 0) return
          callback(
            entries as unknown as ResizeObserverEntry[],
            this as unknown as ResizeObserver,
          )
        }
        observers.push(this)
      }
      observe(el: Element) {
        this.observed.add(el)
      }
      unobserve(el: Element) {
        this.observed.delete(el)
      }
      disconnect() {
        this.observed.clear()
        this.disconnected = true
      }
    },
  )
  let next = 0
  vi.stubGlobal('requestAnimationFrame', (run: FrameRequestCallback) => {
    next += 1
    frames.set(next, run)
    return next
  })
  vi.stubGlobal('cancelAnimationFrame', (id: number) => {
    frames.delete(id)
  })
})

afterEach(() => {
  // Unmounted first: the hook's last cleanup stops what the lists share,
  // which must not outlive the stubs.
  act(() => {
    rerender(<Lists lists={0} />)
  })
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  Reflect.deleteProperty(Element.prototype, 'animate')
  Reflect.deleteProperty(document.documentElement, 'scrollTop')
})

describe('useReseat: what runs, and for how long', () => {
  it('watches every list with one observer, which stops with the last', () => {
    const host = mount(<Lists lists={2} />)
    const [one, two] = host.querySelectorAll('ul')
    expect(observers).toHaveLength(1)
    expect([...observers[0].observed]).toEqual([one, two])
    rerender(<Lists lists={1} />)
    expect([...observers[0].observed]).toEqual([one])
    expect(observers[0].disconnected).toBe(false)
    rerender(<Lists lists={0} />)
    expect(observers[0].disconnected).toBe(true)
    // THE CONTROL: a list drawn again starts a new one.
    rerender(<Lists lists={1} />)
    expect(observers).toHaveLength(2)
    expect(observers[1].disconnected).toBe(false)
  })

  it('adds its window listeners with the first list and removes them with the last', () => {
    const added = vi.spyOn(window, 'addEventListener')
    const removed = vi.spyOn(window, 'removeEventListener')
    const of = (spy: typeof added, type: string) =>
      spy.mock.calls.filter(([each]) => each === type).map(([, run]) => run)
    mount(<Lists lists={2} />)
    expect([of(added, 'resize').length, of(added, 'scroll').length]).toEqual([
      1, 1,
    ])
    rerender(<Lists lists={1} />)
    expect([of(removed, 'resize'), of(removed, 'scroll')]).toEqual([[], []])
    rerender(<Lists lists={0} />)
    expect(of(removed, 'resize')).toEqual(of(added, 'resize'))
    expect(of(removed, 'scroll')).toEqual(of(added, 'scroll'))
    // The capture flag must match, or the scroll listener stays.
    expect(
      removed.mock.calls.find(([type]) => type === 'scroll')?.[2],
    ).toMatchObject({ capture: true })
  })

  it('reads at a row laid each list it has seen, and nothing once the last has gone', () => {
    mount(<Lists />)
    const read = vi.mocked(HTMLUListElement.prototype.getBoundingClientRect)
    rowLaid()
    expect(read.mock.calls.length).toBe(0)
    laidAt(1008)
    rowLaid()
    expect(read.mock.calls.length).toBe(2)
    rerender(<Lists lists={0} />)
    rowLaid()
    expect(read.mock.calls.length).toBe(2)
    // Drawn again, one read a row laid: the first list's listener went
    // with it.
    rerender(<Lists lists={1} />)
    laidAt(1008)
    rowLaid()
    expect(read.mock.calls.length).toBe(4)
  })

  it('observes nothing and listens to nothing while the list is not drawn', () => {
    const added = vi.spyOn(window, 'addEventListener')
    mount(<Lists drawn={false} />)
    expect(observers).toHaveLength(0)
    expect(added).not.toHaveBeenCalled()
    // THE CONTROL: drawn, it is watched.
    rerender(<Lists drawn />)
    expect(observers).toHaveLength(1)
  })
})

describe('useReseat: the covers at a column change', () => {
  it('glides each cover near the view from where it was drawn to its new box', () => {
    const ul = listIn(mount(<Lists />))
    laidAt(1008)
    expect(started).toHaveLength(0)
    laidAt(900)
    const from = moves()
    // Six columns 172.8 px apart become five 189 apart: the sixth cover
    // ends the first row of six and starts the second of five. The first
    // stays where it was and does not move.
    expect(from.get(5)).toBe('translate(864px, -266px) scale(1)')
    const [dx, dy] = numbersIn(from.get(1))
    expect([dx, dy]).toEqual([expect.closeTo(172.8 - 189, 9), 0])
    expect(from.has(0)).toBe(false)
    expect(from.size).toBe(13)
    expect(started[0].options).toEqual({
      duration: RESEAT_MS,
      easing: RESEAT_EASING,
    })
    expect(typeof started[0].animation.startTime).toBe('number')
    expect(ul.hasAttribute('data-reseating')).toBe(true)
    for (const each of started) each.animation.finish()
    expect(ul.hasAttribute('data-reseating')).toBe(false)
  })

  it('moves each tile’s content, never the tile, which the browser holds the page by', () => {
    mount(<Lists />)
    laidAt(1008)
    laidAt(900)
    expect(started.length).toBeGreaterThan(0)
    expect(
      started.flatMap(({ tile, moved }) =>
        moved === tile.firstElementChild ? [] : [moved.tagName],
      ),
    ).toEqual([])
  })

  it('starts a cover still gliding from where that glide draws it', () => {
    mount(<Lists />)
    laidAt(1008)
    laidAt(900)
    for (const each of started) each.animation.progress = 0.5
    const first = started.length
    laidAt(700)
    // Halfway from 864 px right and 266 up, the sixth cover is drawn at
    // (472, 233); in four columns its box is at (225.33, 366).
    const from = new Map(
      started
        .slice(first)
        .map(({ tile, from }) => [
          [...(tile.parentElement?.children ?? [])].indexOf(tile),
          from,
        ]),
    )
    const [dx, dy] = numbersIn(from.get(5))
    expect(dx).toBeCloseTo(472 - (40 + 144 + 124 / 3), 9)
    expect(dy).toBeCloseTo(-133, 9)
  })

  it('glides nothing where the width changes and the column count holds', () => {
    mount(<Lists />)
    laidAt(1008)
    laidAt(1000)
    laidAt(1000)
    expect(started).toHaveLength(0)
    // THE CONTROL: a column fewer, and they glide.
    laidAt(900)
    expect(started.length).toBeGreaterThan(0)
  })

  it('starts each cover where the view drew it, through a scroll the browser anchors in the same frame', () => {
    mount(<Lists />)
    laidAt(1008)
    // The change scrolls the page 266 px with no scroll event seen, as
    // scroll anchoring does to keep the sixth cover, now opening the
    // second row, where it was drawn: at 100 px in the view.
    scrollTop = 266
    box = { left: 40, top: -166 }
    laidAt(900)
    expect(moves().get(5)).toBe('translate(864px, 0px) scale(1)')
  })

  it('reads where each list is drawn when the scroller swap puts the scroll back, in a resized frame too', () => {
    mount(<Lists />)
    const read = vi.mocked(HTMLUListElement.prototype.getBoundingClientRect)
    laidAt(1008)
    window.dispatchEvent(new Event('resize'))
    box = { left: 40, top: 160 }
    scrollPutBack()
    laidAt(900)
    expect(moves().get(5)).toBe('translate(864px, -266px) scale(1)')
    // Gone with the last list: one read a put-back once drawn again.
    rerender(<Lists lists={0} />)
    const reads = read.mock.calls.length
    scrollPutBack()
    expect(read.mock.calls.length).toBe(reads)
    rerender(<Lists lists={1} />)
    laidAt(1008)
    scrollPutBack()
    expect(read.mock.calls.length).toBe(reads + 2)
  })

  it('reads where the list is drawn again at a scroll or a row laid above it', () => {
    mount(<Lists />)
    laidAt(1008)
    box = { left: 40, top: 160 }
    window.dispatchEvent(new Event('scroll'))
    laidAt(900)
    expect(moves().get(5)).toBe('translate(864px, -266px) scale(1)')
    for (const each of started) each.animation.finish()
    started = []
    box = { left: 40, top: 220 }
    rowLaid()
    laidAt(1008)
    expect(moves().get(5)).toBe('translate(-864px, 266px) scale(1)')
  })

  it('reads nothing at a scroll where the list is laid at a size its observer has not run for', () => {
    mount(<Lists />)
    laidAt(1008)
    // Laid at five columns' size early, and scrolled there, as a layout
    // forced before the observers and the scroll it anchors are.
    size = { width: 900, height: 3 * 266 - 16 + 1 }
    box = { left: 40, top: 160 }
    window.dispatchEvent(new Event('scroll'))
    laidAt(900)
    // From where the old layout drew it, 60 px above where it is now.
    expect(moves().get(5)).toBe('translate(864px, -326px) scale(1)')
  })

  it('reads nothing at a scroll elsewhere, or in a frame the window resized in', () => {
    const host = mount(
      <>
        <div data-testid="elsewhere" />
        <Lists />
      </>,
    )
    laidAt(1008)
    box = { left: 40, top: 160 }
    host.querySelector('div')?.dispatchEvent(new Event('scroll'))
    window.dispatchEvent(new Event('resize'))
    window.dispatchEvent(new Event('resize'))
    window.dispatchEvent(new Event('scroll'))
    // Both left the place as last read, 60 px above where it is now.
    laidAt(900)
    expect(moves().get(5)).toBe('translate(864px, -326px) scale(1)')
    // THE CONTROL: once that frame is drawn, a scroll reads it again.
    for (const each of started) each.animation.finish()
    started = []
    window.dispatchEvent(new Event('resize'))
    frame()
    box = { left: 40, top: 220 }
    document.dispatchEvent(new Event('scroll'))
    laidAt(1008)
    expect(moves().get(5)).toBe('translate(-864px, 266px) scale(1)')
  })
})

describe('useReseat: the covers snap', () => {
  it('where a tile has nothing in it to move', () => {
    const warned = vi.spyOn(console, 'warn').mockImplementation(() => {})
    const ul = listIn(mount(<Lists bare />))
    laidAt(1008)
    laidAt(900)
    expect(started).toHaveLength(0)
    expect(warned).not.toHaveBeenCalled()
    expect(ul.hasAttribute('data-reseating')).toBe(false)
  })

  it('where the number of covers changes, a cover still moving included', () => {
    const ul = listIn(mount(<Lists />))
    laidAt(1008)
    laidAt(900)
    expect(ul.hasAttribute('data-reseating')).toBe(true)
    const first = started.length
    rerender(<Lists count={15} />)
    laidAt(1008, 15)
    expect(started.length).toBe(first)
    expect(ul.hasAttribute('data-reseating')).toBe(false)
  })

  it('where the number of covers changes and the columns hold, a cover still moving included', () => {
    const ul = listIn(mount(<Lists />))
    laidAt(1008)
    laidAt(900)
    expect(ul.hasAttribute('data-reseating')).toBe(true)
    const first = started.length
    // Five columns at 900 px either way; a fourth row.
    rerender(<Lists count={16} />)
    laidAt(900, 16)
    expect(started.length).toBe(first)
    expect(ul.hasAttribute('data-reseating')).toBe(false)
  })

  it('across the phone layout’s edge', () => {
    mount(<Lists />)
    laidAt(1008)
    tokens['--tile-fill'] = '1'
    laidAt(900)
    expect(started).toHaveLength(0)
  })

  it('where the system asks for less motion', () => {
    mount(<Lists />)
    laidAt(1008)
    reduce = true
    laidAt(900)
    expect(started).toHaveLength(0)
  })

  it('every one, where a cover is not drawn where the arithmetic puts it', () => {
    const warned = vi.spyOn(console, 'warn').mockImplementation(() => {})
    mount(<Lists />)
    laidAt(1008)
    off = 1.6
    laidAt(900)
    expect(started).toHaveLength(0)
    expect(warned).toHaveBeenCalledTimes(1)
    // THE CONTROL: within 1.5 px, they glide.
    off = 1.5
    laidAt(1008)
    expect(started.length).toBeGreaterThan(0)
  })

  it('where the grid did not lay the rows the arithmetic counts', () => {
    mount(<Lists />)
    laidAt(1008)
    const height = 3 * 266 - 16
    // Five columns lay three rows; the grid says two, then three again.
    page = { columns: 5, pitch: 189 }
    rows = '250px 250px'
    act(() => observers[0].answer(900, height - 1))
    rows = '250px 250px 250px'
    act(() => observers[0].answer(900, height))
    expect(started).toHaveLength(0)
  })

  it('where the list stops being drawn mid-move', () => {
    const ul = listIn(mount(<Lists />))
    laidAt(1008)
    laidAt(900)
    expect(ul.hasAttribute('data-reseating')).toBe(true)
    rerender(<Lists drawn={false} />)
    expect(ul.hasAttribute('data-reseating')).toBe(false)
  })
})
