import { describe, expect, it } from 'vitest'
import type { Mark } from '../src/book'
import { Runs } from '../src/BookRuns'
import type { Segment } from '../src/bookNotes'
import { mount } from './support/mount'

/** `runs` drawn as the reader draws a block's words, inside a paragraph. */
function drawn(runs: Segment[], furigana = true): HTMLElement {
  const host = mount(
    <p>
      <Runs runs={runs} work="w" edition="ja" furigana={furigana} />
    </p>,
  )
  return host.querySelector('p') as HTMLElement
}

/** Each child of `el`, a text node as its text and an element as its tag and text. */
const children = (el: Element) =>
  [...el.childNodes].map((node) =>
    node instanceof Element
      ? `${node.tagName.toLowerCase()}${node.className ? `.${node.className}` : ''}:${node.textContent}`
      : node.textContent,
  )

describe('a run’s numbers set upright', () => {
  it('are each a span of their own, the words around them drawn as they were', () => {
    const p = drawn([
      {
        text: '12日と7',
        marks: [],
        upright: [
          [0, 2],
          [4, 5],
        ],
      },
      { text: 'は第3話の', marks: [], upright: [[2, 3]] },
    ])
    expect(children(p)).toEqual([
      'span.book-upright:12',
      '日と',
      'span.book-upright:7',
      'は第',
      'span.book-upright:3',
      '話の',
    ])
  })

  it('sit inside a ruby’s base, a link and bold, each still one element', () => {
    const ruby: Mark = { kind: 'ruby', start: 0, end: 2, reading: 'はたち' }
    const link: Mark = { kind: 'link', start: 2, end: 5, block: 0, note: false }
    const bold: Mark = { kind: 'bold', start: 2, end: 5 }
    const p = drawn([
      { text: '20歳', marks: [ruby], rubyStart: true, upright: [[0, 2]] },
      { text: '第12話', marks: [link, bold], upright: [[1, 3]] },
    ])
    const [base] = [...p.querySelectorAll('ruby')]
    expect(p.querySelectorAll('ruby')).toHaveLength(1)
    expect(children(base)).toEqual(['span.book-upright:20', '歳', 'rt:はたち'])
    expect(p.querySelectorAll('a')).toHaveLength(1)
    const strong = p.querySelector('a > b') as HTMLElement
    expect(children(strong)).toEqual(['第', 'span.book-upright:12', '話'])
  })

  it('are set upright with furigana off, the base drawn without its ruby', () => {
    const ruby: Mark = { kind: 'ruby', start: 0, end: 2, reading: 'はたち' }
    const p = drawn(
      [{ text: '20歳', marks: [ruby], rubyStart: true, upright: [[0, 2]] }],
      false,
    )
    expect(children(p)).toEqual(['span.book-upright:20', '歳'])
  })

  it('leave a run with none its words alone', () => {
    expect(children(drawn([{ text: '第12話', marks: [] }]))).toEqual(['第12話'])
  })
})
