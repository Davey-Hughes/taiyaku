import { act, Fragment } from 'react'
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  onTestFinished,
} from 'vitest'
import { useLineEnds } from '../src/lineEnds'
import { deferred } from './support/async'
import { mount, rerender } from './support/mount'

// jsdom lays nothing out, so each word's box comes from `lines` below, and the
// observer and the fonts are ones the test settles itself. Where a real window
// breaks a line is the browser project's to show (`lineEnds.browser.test.tsx`).

/** The line each word sits on, from 0; a word with none is not laid out. */
let lines = new Map<string, number>()

const LINE_HEIGHT = 20
/**
 * Each line starts this far above the end of the one before it, as sub-pixel
 * rounding can leave two lines, so that the hook's half-pixel allowance is
 * what reads it as a break.
 */
const OVERLAP = 0.4

function boxOf(word: string | undefined): DOMRect {
  const line = word === undefined ? undefined : lines.get(word)
  if (line === undefined) return { top: 0, bottom: 0, height: 0 } as DOMRect
  const top = line * (LINE_HEIGHT - OVERLAP)
  return { top, bottom: top + LINE_HEIGHT, height: LINE_HEIGHT } as DOMRect
}

class StubResizeObserver {
  static made: StubResizeObserver[] = []
  readonly report: () => void
  observed: Element[] = []
  disconnected = false
  constructor(report: () => void) {
    this.report = report
    StubResizeObserver.made.push(this)
  }
  observe(target: Element) {
    this.observed.push(target)
  }
  unobserve() {}
  disconnect() {
    this.disconnected = true
    this.observed = []
  }
}

const measure = Element.prototype.getBoundingClientRect
const held = globalThis.ResizeObserver
beforeEach(() => {
  lines = new Map()
  Element.prototype.getBoundingClientRect = function (this: Element) {
    return boxOf((this as HTMLElement).dataset?.word)
  }
  StubResizeObserver.made = []
  globalThis.ResizeObserver =
    StubResizeObserver as unknown as typeof ResizeObserver
})
afterEach(() => {
  Element.prototype.getBoundingClientRect = measure
  globalThis.ResizeObserver = held
})

/** Puts each word on the line given; a word left out is not laid out. */
const layOut = (at: Record<string, number>) => {
  lines = new Map(Object.entries(at))
}

/** Three lines of two words; the `<i>` after bravo sits on bravo's line. */
const THREE_LINES = {
  alpha: 0,
  bravo: 0,
  between: 0,
  charlie: 1,
  delta: 1,
  echo: 2,
  foxtrot: 2,
}

const WORDS = ['alpha', 'bravo', 'charlie', 'delta', 'echo', 'foxtrot']

/**
 * The row of words, each `span` marked by the hook, with an `<i>` after bravo:
 * a sibling the hook does not mark, which it steps over to find the word after
 * bravo. Made here because `useComponentExportOnlyModules` refuses an
 * unexported component at module scope, and `noExportsInTest` refuses the
 * export; handed back as a component so that a rerender keeps its instance.
 */
function rowComponent() {
  function Row({ words = WORDS }: { words?: readonly string[] }) {
    const holder = useLineEnds<HTMLDivElement>('span')
    return (
      <div ref={holder} className="row">
        {words.map((word) => (
          <Fragment key={word}>
            <span data-word={word}>{word}</span>
            {word === 'bravo' && <i data-word="between" />}
          </Fragment>
        ))}
      </div>
    )
  }
  return Row
}

/** Mounts the row and hands back its holder, and the component to redraw it. */
function mountRow() {
  const Row = rowComponent()
  const host = mount(<Row />)
  return { Row, row: host.querySelector('.row') as HTMLElement }
}

/** Each word of the row, and whether it is marked as ending a line. */
const marks = (row: Element) =>
  [...row.querySelectorAll<HTMLElement>('span')].map((word) => [
    word.dataset.word,
    word.hasAttribute('data-ends-line'),
  ])

/** What `marks` reads for `THREE_LINES`: the second word of each line. */
const THREE_LINES_MARKED = [
  ['alpha', false],
  ['bravo', true],
  ['charlie', false],
  ['delta', true],
  ['echo', false],
  ['foxtrot', true],
]

/** `target` was resized: each observer watching it reports. */
function resized(target: Element) {
  act(() => {
    for (const observer of StubResizeObserver.made) {
      if (observer.observed.includes(target)) observer.report()
    }
  })
}

/** A `document.fonts` whose `ready` the test settles, removed after it. */
function fontsPending() {
  const ready = deferred<FontFaceSet>()
  Object.defineProperty(document, 'fonts', {
    configurable: true,
    value: { ready: ready.promise },
  })
  onTestFinished(() => {
    Reflect.deleteProperty(document, 'fonts')
  })
  return {
    arrive: () =>
      act(async () => {
        ready.resolve(document.fonts)
        await ready.promise
      }),
  }
}

describe('useLineEnds', () => {
  it('marks a word whose next word starts on a line below it, and the last word, and no other', () => {
    layOut(THREE_LINES)
    const { row } = mountRow()
    expect(marks(row)).toEqual(THREE_LINES_MARKED)
  })

  it('marks nothing in a row that is not laid out, not even its last word', () => {
    const { row } = mountRow()
    expect(marks(row)).toEqual(WORDS.map((word) => [word, false]))
  })

  it('marks again on each draw of its owner, unmarking a word that no longer ends a line', () => {
    layOut(THREE_LINES)
    const { Row, row } = mountRow()
    expect(marks(row)).toEqual(THREE_LINES_MARKED)

    layOut({ ...THREE_LINES, charlie: 0, golf: 3 })
    rerender(<Row words={[...WORDS, 'golf']} />)
    expect(marks(row)).toEqual([
      ['alpha', false],
      ['bravo', false],
      ['charlie', true],
      ['delta', true],
      ['echo', false],
      ['foxtrot', true],
      ['golf', true],
    ])
  })

  it('marks again when the holder is resized, with no draw of its own', () => {
    layOut(THREE_LINES)
    const { row } = mountRow()
    expect(marks(row)).toEqual(THREE_LINES_MARKED)

    layOut({ ...THREE_LINES, alpha: 0, bravo: 1, between: 1, charlie: 1 })
    resized(row)
    expect(marks(row)).toEqual([
      ['alpha', true],
      ['bravo', false],
      ['charlie', false],
      ['delta', true],
      ['echo', false],
      ['foxtrot', true],
    ])
  })

  it('marks again once the fonts arrive, when they move the line ends and resize nothing', async () => {
    const fonts = fontsPending()
    layOut(THREE_LINES)
    const { row } = mountRow()
    expect(marks(row)).toEqual(THREE_LINES_MARKED)

    layOut({ ...THREE_LINES, echo: 1 })
    await fonts.arrive()
    expect(marks(row)).toEqual([
      ['alpha', false],
      ['bravo', true],
      ['charlie', false],
      ['delta', false],
      ['echo', true],
      ['foxtrot', true],
    ])
  })

  it('disconnects its observer when its owner unmounts', () => {
    const { row } = mountRow()
    const [observer] = StubResizeObserver.made
    expect(observer?.observed).toEqual([row])

    rerender(null)
    expect(observer?.disconnected).toBe(true)
  })

  it('marks nothing when the fonts arrive after its owner unmounts', async () => {
    const fonts = fontsPending()
    layOut(THREE_LINES)
    const { row } = mountRow()
    rerender(null)

    layOut(Object.fromEntries(WORDS.map((word, line) => [word, line])))
    await fonts.arrive()
    expect(marks(row)).toEqual(THREE_LINES_MARKED)
  })

  describe('without a ResizeObserver', () => {
    beforeEach(() => {
      globalThis.ResizeObserver = undefined as unknown as typeof ResizeObserver
    })

    it('still marks on each draw', () => {
      layOut(THREE_LINES)
      const { row } = mountRow()
      expect(marks(row)).toEqual(THREE_LINES_MARKED)
    })

    it('marks nothing when the fonts arrive after its owner unmounts', async () => {
      const fonts = fontsPending()
      layOut(THREE_LINES)
      const { row } = mountRow()
      rerender(null)

      layOut(Object.fromEntries(WORDS.map((word, line) => [word, line])))
      await fonts.arrive()
      expect(marks(row)).toEqual(THREE_LINES_MARKED)
    })
  })

  it('without document.fonts, still marks on each draw and on a resize', () => {
    expect('fonts' in document).toBe(false)
    layOut(THREE_LINES)
    const { row } = mountRow()
    expect(marks(row)).toEqual(THREE_LINES_MARKED)

    layOut({ ...THREE_LINES, echo: 1 })
    resized(row)
    expect(marks(row)).toEqual([
      ['alpha', false],
      ['bravo', true],
      ['charlie', false],
      ['delta', false],
      ['echo', true],
      ['foxtrot', true],
    ])
  })
})
