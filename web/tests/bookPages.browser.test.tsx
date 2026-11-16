import { describe, expect, it } from 'vitest'
import { kept, show, shows, topOf } from '../src/bookPages'
import '../src/index.css'
import { mount } from './support/mount'

/**
 * `bookPages.ts` against a real layout, which jsdom has none of: where a
 * page starts inside a paragraph, and the page that holds it once the text
 * is laid out again at another size.
 */

const WORDS = Array.from(
  { length: 400 },
  (_, i) => `word${i} the quick brown fox jumps over the lazy dog.`,
).join(' ')

/** A paged text, `book.css`'s columns at a fixed size, of one long paragraph between two short ones. */
function text(paged: boolean): HTMLElement {
  const host = mount(
    <div
      className="book-text"
      data-paged={paged || undefined}
      style={{
        boxSizing: 'border-box',
        width: 320,
        height: 240,
        padding: 24,
        fontSize: 16,
        lineHeight: 1.5,
        overflow: paged ? 'hidden' : 'auto',
        ...(paged ? { columnCount: 1, columnFill: 'auto', columnGap: 48 } : {}),
      }}
    >
      <p data-block={0}>
        <span data-text>A short opening.</span>
      </p>
      <p data-block={1}>
        <span data-text>
          {WORDS.slice(0, 900)}
          <mark>{WORDS.slice(900, 1200)}</mark>
          {WORDS.slice(1200)}
        </span>
      </p>
      <p data-block={2}>
        <span data-text>A short close.</span>
      </p>
    </div>,
  )
  return host.firstElementChild as HTMLElement
}

const pageOn = (el: HTMLElement) => Math.round(el.scrollLeft / el.clientWidth)

describe('bookPages, laid out', () => {
  it('finds the character a page starts on inside a paragraph', () => {
    const el = text(true)
    el.scrollTo({ left: 3 * el.clientWidth })
    const top = topOf(el)
    expect(top.block).toBe(1)
    expect(top.at).toBeGreaterThan(0)
    expect(shows(el, top)).toBe(true)
    expect(shows(el, { block: 1, at: Math.max(top.at - 1, 0) })).toBe(false)
  })

  it('keeps a page that starts inside a paragraph on screen as the text is sized up', () => {
    const el = text(true)
    el.scrollTo({ left: 4 * el.clientWidth })
    const top = topOf(el)
    el.style.fontSize = '22px'
    show(el, top.block, top.at)
    expect(shows(el, top)).toBe(true)
    // Placed by its block alone, the reader goes to the paragraph's first
    // line, pages earlier, and the page is lost.
    show(el, top.block)
    expect(shows(el, top)).toBe(false)
  })

  it('comes back to the same page on sizing the text up and down again', () => {
    const el = text(true)
    el.scrollTo({ left: 5 * el.clientWidth })
    const began = topOf(el)
    for (const size of ['20px', '24px', '18px', '16px']) {
      const anchor = kept(el, began)
      expect(anchor).toEqual(began)
      el.style.fontSize = size
      show(el, anchor.block, anchor.at)
    }
    expect(pageOn(el)).toBe(5)
    expect(topOf(el)).toEqual(began)
  })

  it('takes the top of the page once a turn has taken the anchor off it', () => {
    const el = text(true)
    el.scrollTo({ left: 2 * el.clientWidth })
    const before = topOf(el)
    el.scrollTo({ left: 6 * el.clientWidth })
    expect(kept(el, before)).toEqual(topOf(el))
    expect(kept(el, before)).not.toEqual(before)
  })

  it('keeps the line a scrolled text is at as it is sized up', () => {
    const el = text(false)
    el.scrollTo({ top: 1500 })
    const top = topOf(el)
    expect(top.at).toBeGreaterThan(0)
    el.style.fontSize = '22px'
    show(el, top.block, top.at)
    expect(shows(el, top)).toBe(true)
  })
})
