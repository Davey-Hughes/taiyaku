import { act } from 'react'
import { describe, expect, it, onTestFinished } from 'vitest'
import { useLineEnds } from '../src/lineEnds'
import { deferred } from './support/async'
import { mount } from './support/mount'

/**
 * Words in a row that wraps, inside a holder whose height is fixed, so that
 * only a change of its width resizes it: a change inside it that moves the
 * line ends without resizing it is seen by nothing but the hook's own wait.
 */
const WORDS = ['alpha', 'bravo', 'charlie', 'delta', 'echo', 'foxtrot']

/**
 * The row, drawn by the hook. Made here because
 * `useComponentExportOnlyModules` refuses an unexported component at module
 * scope, and `noExportsInTest` refuses the export.
 */
function drawRow() {
  function Row() {
    const holder = useLineEnds<HTMLDivElement>('span')
    return (
      <div
        ref={holder}
        className="row"
        style={{
          display: 'flex',
          flexWrap: 'wrap',
          gap: '0 8px',
          width: '600px',
          height: '300px',
          font: '16px monospace',
        }}
      >
        {WORDS.map((word) => (
          <span key={word}>{word}</span>
        ))}
      </div>
    )
  }
  return <Row />
}

/** A frame, and what the browser does after it, inside `act`. */
const frame = () =>
  act(
    () =>
      new Promise<void>((done) =>
        requestAnimationFrame(() => setTimeout(done, 20)),
      ),
  )

/** Each word, as drawn. */
const words = (host: HTMLElement) => [
  ...host.querySelectorAll<HTMLElement>('.row > span'),
]

/** Whether each word ends a line, read from the boxes. */
const endsLine = (host: HTMLElement) =>
  words(host).map((word, i, all) => {
    const next = all[i + 1]
    return (
      next === undefined ||
      next.getBoundingClientRect().top >=
        word.getBoundingClientRect().bottom - 0.5
    )
  })

/** Whether each word is marked as ending a line. */
const marked = (host: HTMLElement) =>
  words(host).map((word) => word.hasAttribute('data-ends-line'))

describe('useLineEnds marks again when the line ends move after the first draw', () => {
  it('marks the words that end a line anew when the holder is narrowed after mount, with no draw of its own', async () => {
    const host = mount(drawRow())
    await frame()
    // One line at first: only the last word ends one.
    expect(marked(host)).toEqual(endsLine(host))
    expect(endsLine(host).filter(Boolean)).toHaveLength(1)

    const row = host.querySelector('.row') as HTMLElement
    row.style.width = '160px'
    await frame()
    expect(endsLine(host).filter(Boolean).length).toBeGreaterThan(1)
    expect(marked(host)).toEqual(endsLine(host))
  })

  it('marks the words that end a line anew once the fonts arrive, when the face that arrives moves them and resizes nothing', async () => {
    const fonts = deferred<FontFaceSet>()
    Object.defineProperty(document.fonts, 'ready', {
      configurable: true,
      get: () => fonts.promise,
    })
    onTestFinished(() => {
      Reflect.deleteProperty(document.fonts, 'ready')
    })
    const host = mount(drawRow())
    await frame()
    expect(marked(host)).toEqual(endsLine(host))
    expect(endsLine(host).filter(Boolean)).toHaveLength(1)

    // The face that arrives is wider than the one drawn while waiting. The
    // holder's height and width are its own, so its box does not change.
    const row = host.querySelector('.row') as HTMLElement
    const box = row.getBoundingClientRect()
    row.style.fontSize = '32px'
    await frame()
    expect(endsLine(host).filter(Boolean).length).toBeGreaterThan(1)
    expect([row.offsetWidth, row.offsetHeight]).toEqual([box.width, box.height])
    await act(async () => {
      fonts.resolve(document.fonts)
      await fonts.promise
    })
    expect(marked(host)).toEqual(endsLine(host))
  })
})
