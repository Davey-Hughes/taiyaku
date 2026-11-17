import { describe, expect, it } from 'vitest'
import { BookFootnote } from '../src/BookFootnote'
import '../src/index.css'
import './support/dialog'
import { mount } from './support/mount'

/**
 * `book.css` as a browser draws it: what a link, a note opened from one and
 * a paragraph under a heading's figure look like, which jsdom computes none
 * of.
 */

const note = {
  text: 'See 1 here.',
  heading: false,
  sentences: [[0, 11]] as [number, number][],
  marks: [{ kind: 'link' as const, start: 4, end: 5, block: 0, note: true }],
}

function footnote(page?: string): HTMLElement {
  const host = mount(
    <div className="reader">
      <BookFootnote
        block={note}
        work="w"
        edition="en"
        furigana
        page={page}
        onGo={() => {}}
        onClose={() => {}}
      />
    </div>,
  )
  return host.querySelector('.book-footnote-card') as HTMLElement
}

describe('book.css', () => {
  it('underlines a link in the page’s own ink, and raises a note reference with no superscript of its own', () => {
    const host = mount(
      <div className="reader">
        <div className="book-text" style={{ color: 'rgb(1, 2, 3)' }}>
          <p>
            <a href="#n" data-link data-link-note>
              1
            </a>{' '}
            <a href="#n" data-link data-link-note>
              <sup>2</sup>
            </a>{' '}
            <a href="#n" data-link>
              on
            </a>
          </p>
        </div>
      </div>,
    )
    const [bare, sup, plain] = host.querySelectorAll('a')
    const style = getComputedStyle(plain)
    expect(style.color).toBe('rgb(1, 2, 3)')
    expect(style.textDecorationLine).toBe('underline')
    expect(style.cursor).toBe('pointer')
    expect(style.verticalAlign).toBe('baseline')
    expect(getComputedStyle(bare).verticalAlign).toBe('super')
    expect(getComputedStyle(sup).verticalAlign).toBe('baseline')
  })

  it('draws a note’s card opaque, and on paper where the page is paper', () => {
    const dark = getComputedStyle(footnote())
    expect(dark.backgroundColor).toBe('rgb(21, 19, 15)')
    expect(
      getComputedStyle(footnote().querySelector('a') as Element)
        .textDecorationLine,
    ).toBe('underline')
    const paper = getComputedStyle(footnote('paper'))
    expect(paper.backgroundColor).toBe('rgb(243, 238, 227)')
    expect(paper.color).toBe('rgb(35, 30, 23)')
  })

  it('sets no indent on the paragraph under a heading’s figure', () => {
    const host = mount(
      <div className="reader">
        <div className="book-text" data-indent>
          <h2>One</h2>
          <figure className="book-figure" data-figure />
          <p>First.</p>
          <p>Second.</p>
        </div>
      </div>,
    )
    const [first, second] = host.querySelectorAll('p')
    expect(getComputedStyle(first).textIndent).toBe('0px')
    expect(getComputedStyle(second).textIndent).not.toBe('0px')
  })
})

describe('book.css on an image set in a link', () => {
  it('offers a link’s pointer on an inline image or a figure set in one, and zoom on one not', () => {
    const host = mount(
      <div className="reader">
        <div className="book-text">
          <p>
            <a href="#n" data-link>
              <img className="book-inline-image" alt="linked" />
            </a>
            <img className="book-inline-image" alt="plain" />
          </p>
          <figure className="book-figure">
            <a href="#n" data-link>
              <img alt="figure" />
            </a>
          </figure>
        </div>
      </div>,
    )
    const cursor = (alt: string) =>
      getComputedStyle(host.querySelector(`img[alt="${alt}"]`) as Element)
        .cursor
    expect(cursor('linked')).toBe('pointer')
    expect(cursor('figure')).toBe('pointer')
    expect(cursor('plain')).toBe('zoom-in')
  })
})

describe('book.css on a note card opened from a tall reference', () => {
  it('keeps the card on the screen when its reference, a figure, runs past both edges or reaches high', () => {
    const tall: typeof note = {
      text: 'A long note. '.repeat(200).trim(),
      heading: false,
      sentences: [[0, 2599]],
      marks: [],
    }
    const h = window.innerHeight
    for (const near of [
      { left: 10, right: 300, top: -200, bottom: h + 200 },
      { left: 10, right: 300, top: h * 0.2, bottom: h * 0.9 },
      { left: 10, right: 300, top: h * 0.6, bottom: h * 0.7 },
    ]) {
      const host = mount(
        <div className="reader">
          <BookFootnote
            block={tall}
            work="w"
            edition="en"
            furigana
            near={near}
            onGo={() => {}}
            onClose={() => {}}
          />
        </div>,
      )
      const card = (
        host.querySelector('.book-footnote-card') as HTMLElement
      ).getBoundingClientRect()
      expect(card.height, JSON.stringify(near)).toBeGreaterThan(h * 0.4)
      expect(card.top, JSON.stringify(near)).toBeGreaterThanOrEqual(0)
      expect(card.bottom, JSON.stringify(near)).toBeLessThanOrEqual(h)
    }
  })
})
