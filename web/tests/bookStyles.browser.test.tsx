import type { CSSProperties } from 'react'
import { describe, expect, it } from 'vitest'
import { BookFootnote } from '../src/BookFootnote'
import '../src/index.css'
import { paintedBox } from '../src/notes'
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

  it('takes the browser’s double-tap zoom, and only that, from the text, paged or scrolled', () => {
    const host = mount(
      <div className="reader">
        <div className="book-text" data-paged="">
          <p>One.</p>
        </div>
        <div className="book-text">
          <p>One.</p>
        </div>
      </div>,
    )
    const [paged, scrolled] = host.querySelectorAll('.book-text')
    expect(getComputedStyle(paged).touchAction).toBe('manipulation')
    expect(getComputedStyle(scrolled).touchAction).toBe('manipulation')
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

/** An image `width` by `height` px. */
function svg(width: number, height: number): string {
  return `data:image/svg+xml,${encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}"><rect width="${width}" height="${height}"/></svg>`,
  )}`
}

/**
 * Where `image` draws its picture: contained in its own box and centred in
 * it, which this checks, and so where `paintedBox` reckons it.
 */
function drawnOf(image: HTMLImageElement): DOMRect {
  const style = getComputedStyle(image)
  expect([style.objectFit, style.objectPosition]).toEqual([
    'contain',
    '50% 50%',
  ])
  const box = image.getBoundingClientRect()
  const art = paintedBox(
    { w: image.naturalWidth, h: image.naturalHeight },
    { w: box.width, h: box.height },
  )
  return new DOMRect(box.left + art.x, box.top + art.y, art.w, art.h)
}

const middleOf = (rect: DOMRect) => ({
  x: rect.left + rect.width / 2,
  y: rect.top + rect.height / 2,
})

describe('book.css on a page image', () => {
  /**
   * A text 400 px wide and 600 high holding a page image between two
   * paragraphs: paged, scrolled, or paged and compared, where the three are
   * the first cell of a row beside a second edition's. Its image is not
   * loaded (`loaded`). `upscale` is the comic reader's upscale-to-fit
   * setting, set on the text as the book reader sets it, `linked` sets the
   * image in a link, and `vertical` sets the text as vertical Japanese.
   */
  const withPageImage = (
    as: 'paged' | 'scrolled' | 'compared',
    { upscale = false, linked = false, vertical = false } = {},
  ) => {
    const image = <img alt="cover" />
    const read = (
      <>
        <p>Before.</p>
        <figure className="book-figure" data-figure data-page-image>
          {linked ? (
            <a href="#n" data-link>
              {image}
            </a>
          ) : (
            image
          )}
        </figure>
        <p>After.</p>
      </>
    )
    return mount(
      <div className="reader" style={{ display: 'flex', height: '600px' }}>
        <div
          className="book-text"
          data-paged={as !== 'scrolled' || undefined}
          data-compared={as === 'compared' || undefined}
          data-upscale={upscale || undefined}
          data-vertical={vertical || undefined}
          lang={vertical ? 'ja' : undefined}
          style={
            {
              '--book-column-width': '20em',
              '--book-text-size': '16px',
              width: '400px',
              height: '600px',
            } as CSSProperties
          }
        >
          {as === 'compared' ? (
            <div className="book-row" data-block={0}>
              <div className="book-cell" data-column="en">
                {read}
              </div>
              <div className="book-cell" data-column="ko">
                <p>Avant.</p>
              </div>
            </div>
          ) : (
            read
          )}
        </div>
      </div>,
    )
  }

  /** `host`'s image, loaded from an image `width` by `height` px. */
  async function loaded(host: HTMLElement, width: number, height: number) {
    const image = host.querySelector('img') as HTMLImageElement
    image.src = svg(width, height)
    await image.decode()
    return image
  }

  it.each([
    ['off', false],
    ['on', true],
  ] as const)(
    'gives it a whole page of a paged text before its image loads, and loading it moves no text, with upscale-to-fit %s',
    async (_, upscale) => {
      const host = withPageImage('paged', { upscale })
      const text = host.querySelector('.book-text') as HTMLElement
      const figure = host.querySelector('figure') as HTMLElement
      const after = host.querySelectorAll('p')[1] as HTMLElement
      const style = getComputedStyle(text)
      const top =
        text.getBoundingClientRect().top + Number.parseFloat(style.paddingTop)
      const pageHigh =
        text.clientHeight -
        Number.parseFloat(style.paddingTop) -
        Number.parseFloat(style.paddingBottom)
      const box = figure.getBoundingClientRect()
      expect(box.height).toBeCloseTo(pageHigh, 0)
      expect(box.top).toBeCloseTo(top, 0)
      // Its own page: the paragraph after it starts the next page at its top.
      const next = after.getBoundingClientRect()
      expect(next.top).toBeCloseTo(top, 0)
      await loaded(host, 40, 30)
      expect(figure.getBoundingClientRect().toJSON()).toEqual(box.toJSON())
      expect(after.getBoundingClientRect().toJSON()).toEqual(next.toJSON())
    },
  )

  /**
   * Vertical, the pages run down the screen, a `clientHeight` apart, and a
   * page's width is its block size, which the figure takes whole.
   */
  it.each([
    ['off', false],
    ['on', true],
  ] as const)(
    'gives it a whole page of a paged vertical text, the next block on the page below, with upscale-to-fit %s',
    async (_, upscale) => {
      const host = withPageImage('paged', { upscale, vertical: true })
      const text = host.querySelector('.book-text') as HTMLElement
      const figure = host.querySelector('figure') as HTMLElement
      const after = host.querySelectorAll('p')[1] as HTMLElement
      await loaded(host, 40, 30)
      const style = getComputedStyle(text)
      expect(style.writingMode).toBe('vertical-rl')
      const outer = text.getBoundingClientRect()
      const page = {
        left: outer.left + Number.parseFloat(style.paddingLeft),
        right: outer.right - Number.parseFloat(style.paddingRight),
        top: outer.top + Number.parseFloat(style.paddingTop),
        bottom:
          outer.top +
          text.clientHeight -
          Number.parseFloat(style.paddingBottom),
      }
      // The page after the one "Before." is on, and "After." on the next.
      const down = text.clientHeight
      const box = figure.getBoundingClientRect()
      expect([box.left, box.right, box.top, box.bottom]).toEqual(
        [page.left, page.right, page.top + down, page.bottom + down].map(
          (edge) => expect.closeTo(edge, 0),
        ),
      )
      const next = after.getBoundingClientRect()
      expect(next.right).toBeCloseTo(page.right, 0)
      expect(next.top).toBeCloseTo(page.top + 2 * down, 0)
    },
  )

  /**
   * As the comic reader sizes a page: never larger than the page, and larger
   * than its own size only while upscale-to-fit is on. Set in a link or not,
   * and set across or down, the same.
   */
  it.each([
    ['off', 40, 30, 'its own size'],
    ['on', 40, 30, 'as large as the page lets it'],
    ['off', 2000, 1000, 'as large as the page lets it'],
    ['on', 2000, 1000, 'as large as the page lets it'],
    ['off', 1000, 4000, 'as large as the page lets it'],
    ['on', 1000, 4000, 'as large as the page lets it'],
  ] as const)(
    'with upscale-to-fit %s, draws a %i×%i image %s, centred on its page, set in a link or not, across or down',
    async (setting, width, height, size) => {
      for (const [linked, vertical] of [
        [false, false],
        [true, false],
        [false, true],
        [true, true],
      ]) {
        const host = withPageImage('paged', {
          upscale: setting === 'on',
          linked,
          vertical,
        })
        const image = await loaded(host, width, height)
        const page = (
          host.querySelector('figure') as HTMLElement
        ).getBoundingClientRect()
        const drawn = drawnOf(image)
        const scale =
          size === 'its own size'
            ? 1
            : Math.min(page.width / width, page.height / height)
        const said = `linked: ${linked}, vertical: ${vertical}`
        expect(drawn.width, said).toBeCloseTo(width * scale, 0)
        expect(drawn.height, said).toBeCloseTo(height * scale, 0)
        expect(middleOf(drawn).x, said).toBeCloseTo(middleOf(page).x, 0)
        expect(middleOf(drawn).y, said).toBeCloseTo(middleOf(page).y, 0)
        if (linked) {
          // The link is under all of the picture, so a tap on it is the link's.
          const link = (
            host.querySelector('a') as HTMLElement
          ).getBoundingClientRect()
          expect(link.left).toBeLessThanOrEqual(drawn.left + 0.5)
          expect(link.top).toBeLessThanOrEqual(drawn.top + 0.5)
          expect(link.right).toBeGreaterThanOrEqual(drawn.right - 0.5)
          expect(link.bottom).toBeGreaterThanOrEqual(drawn.bottom - 0.5)
        }
      }
    },
  )

  it('shows neither zoom nor a link’s pointer on it paged, set in a link or not, the pointer where a press follows its link, and zoom compared', () => {
    const cursor = (host: HTMLElement) =>
      getComputedStyle(host.querySelector('img') as Element).cursor
    const follows = withPageImage('paged', { linked: true, upscale: true })
    follows.querySelector('img')?.toggleAttribute('data-follows', true)
    expect([
      cursor(withPageImage('paged')),
      cursor(withPageImage('paged', { linked: true })),
      cursor(withPageImage('paged', { upscale: true })),
      cursor(follows),
      cursor(withPageImage('compared')),
    ]).toEqual(['default', 'default', 'default', 'pointer', 'zoom-in'])
  })

  it('shows no link’s pointer on the room beside its picture paged, drawn its own size in a link', async () => {
    const host = withPageImage('paged', { linked: true })
    const image = await loaded(host, 40, 30)
    const text = host.querySelector('.book-text') as HTMLElement
    const figure = host.querySelector('figure') as HTMLElement
    // Its page brought onto the screen, the one after "Before.".
    text.scrollLeft =
      figure.getBoundingClientRect().left - text.getBoundingClientRect().left
    const page = figure.getBoundingClientRect()
    const picture = drawnOf(image)
    const beside = document.elementFromPoint(
      middleOf(page).x,
      (page.top + picture.top) / 2,
    ) as Element
    // THE STATE THIS TEST NAMES: the room above the picture is the link's.
    expect([beside, picture.top - page.top > 20]).toEqual([
      host.querySelector('a'),
      true,
    ])
    expect(getComputedStyle(beside).cursor).toBe('default')
  })

  it('leaves it a figure like any other in a scrolled text', () => {
    const host = withPageImage('scrolled')
    expect(
      getComputedStyle(host.querySelector('img') as Element).objectFit,
    ).toBe('fill')
  })

  it('leaves it a figure like any other in a compared paged text, so its row is not carried onto a page of its own', () => {
    const host = withPageImage('compared')
    const text = host.querySelector('.book-text') as HTMLElement
    const figure = host.querySelector('figure') as HTMLElement
    expect(figure.getBoundingClientRect().height).toBeLessThan(
      text.clientHeight / 2,
    )
    expect(
      getComputedStyle(host.querySelector('img') as Element).objectFit,
    ).toBe('fill')
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
