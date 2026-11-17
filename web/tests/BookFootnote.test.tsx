import { act } from 'react'
import { describe, expect, it, vi } from 'vitest'
import type { Block } from '../src/book'
import { BookFootnote } from '../src/BookFootnote'
import './support/dialog'
import { mount } from './support/mount'

const note: Block = {
  text: '1. 坊ちゃん is a novel.',
  heading: false,
  sentences: [[0, 19]],
  marks: [
    { kind: 'ruby', start: 3, end: 4, reading: 'ぼっ' },
    { kind: 'italic', start: 3, end: 7 },
    { kind: 'link', start: 0, end: 2, block: 4, note: false },
  ],
}

function opened(over: Partial<Parameters<typeof BookFootnote>[0]> = {}) {
  const onGo = vi.fn()
  const onClose = vi.fn()
  const host = mount(
    <BookFootnote
      block={note}
      work="w"
      edition="ja"
      furigana
      onGo={onGo}
      onClose={onClose}
      {...over}
    />,
  )
  const dialog = host.querySelector('dialog.book-footnote') as HTMLDialogElement
  return { host, dialog, onGo, onClose }
}

/** A press and release on `on`, and the click it makes there. */
function tapOn(on: Element) {
  on.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
  on.dispatchEvent(new PointerEvent('pointerup', { bubbles: true }))
  ;(on as HTMLElement).click()
}

describe('BookFootnote', () => {
  it('opens as a dialog holding the note’s words with their marks', () => {
    const { dialog } = opened()
    expect(dialog.open).toBe(true)
    const words = dialog.querySelector('.book-footnote-text') as HTMLElement
    // Cut where the ruby's base ends, the italic drawn in two runs.
    expect([...words.querySelectorAll('i')].map((i) => i.textContent)).toEqual([
      '坊ぼっ',
      'ちゃん',
    ])
    expect(words.querySelector('ruby')?.outerHTML).toBe(
      '<ruby>坊<rt data-aside="true">ぼっ</rt></ruby>',
    )
    expect(words.querySelector('a')?.dataset.linkBlock).toBe('4')
  })

  it('draws no reading with furigana off', () => {
    const { dialog } = opened({ furigana: false })
    expect(dialog.querySelector('rt')).toBeNull()
    expect(dialog.querySelector('.book-footnote-text')?.textContent).toBe(
      note.text,
    )
  })

  it('goes to the note on its button', () => {
    const { dialog, onGo, onClose } = opened()
    const go = [...dialog.querySelectorAll('button')].find(
      (b) => b.textContent === 'Go to note',
    )
    act(() => go?.click())
    expect(onGo).toHaveBeenCalledOnce()
    expect(onClose).not.toHaveBeenCalled()
  })

  it('closes on a tap beside the note, not on it', () => {
    const { dialog, onClose } = opened()
    const words = dialog.querySelector('.book-footnote-text') as HTMLElement
    act(() => tapOn(words))
    expect(onClose).not.toHaveBeenCalled()
    // A press beside the note that made no click, and then a click on the
    // note from the keyboard.
    act(() => {
      dialog.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
      words.click()
    })
    expect(onClose).not.toHaveBeenCalled()
    act(() => tapOn(dialog))
    expect(onClose).toHaveBeenCalledOnce()
  })

  it('stays open for a click beside the note from a press that began elsewhere', () => {
    const { dialog, onClose } = opened()
    // The tap that opened it, its press on the text beneath, sends its
    // `click` here on a touch screen.
    act(() => dialog.click())
    // A press on the note let go beside it: the click goes to the dialog.
    const words = dialog.querySelector('.book-footnote-text') as HTMLElement
    act(() => {
      words.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }))
      dialog.click()
    })
    expect(onClose).not.toHaveBeenCalled()
  })

  it('closes on the browser’s close request, leaving the dialog to its owner', () => {
    const { dialog, onClose } = opened()
    const cancel = new Event('cancel', { cancelable: true })
    act(() => {
      dialog.dispatchEvent(cancel)
    })
    expect(onClose).toHaveBeenCalledOnce()
    expect(cancel.defaultPrevented).toBe(true)
  })

  it('sets itself under a reference in the top half of the screen, and over one in the bottom half', () => {
    vi.stubGlobal('innerHeight', 800)
    const card = (near: {
      left: number
      right: number
      top: number
      bottom: number
    }) =>
      opened({ near }).dialog.querySelector<HTMLElement>('.book-footnote-card')
        ?.style
    const under = card({ left: 40, right: 60, top: 100, bottom: 120 })
    expect(under?.top).toBe('124px')
    expect(under?.bottom).toBe('')
    expect(under?.getPropertyValue('--footnote-x')).toBe('50px')
    const over = card({ left: 40, right: 60, top: 600, bottom: 620 })
    expect(over?.bottom).toBe('204px')
    expect(over?.top).toBe('')
    vi.unstubAllGlobals()
  })

  it('sets itself under a reference in the bottom half that leaves it no room over it but room under it, and mid-screen where neither has room', () => {
    vi.stubGlobal('innerHeight', 800)
    const card = (top: number, bottom: number) =>
      opened({
        near: { left: 40, right: 60, top, bottom },
      }).dialog.querySelector<HTMLElement>('.book-footnote-card') as HTMLElement
    const under = card(300, 420)
    expect([under.style.top, under.style.bottom]).toEqual(['424px', ''])
    expect(under.dataset.placed).toBe('true')
    // Room both ways: the half the reference ends in still decides.
    const both = [card(370, 390), card(380, 410)].map((c) => [
      c.style.top,
      c.style.bottom,
    ])
    expect(both).toEqual([
      ['394px', ''],
      ['', '424px'],
    ])
    const neither = card(300, 500)
    expect([neither.style.top, neither.style.bottom]).toEqual(['', ''])
    expect(neither.dataset.placed).toBeUndefined()
    vi.unstubAllGlobals()
  })

  it('draws the card at its tallest inside the window it was placed by, however it is placed', () => {
    vi.stubGlobal('innerHeight', 640)
    for (const near of [
      { left: 40, right: 60, top: 100, bottom: 120 },
      { left: 40, right: 60, top: 300, bottom: 320 },
      { left: 40, right: 60, top: -100, bottom: 740 },
    ]) {
      const style = opened({ near }).dialog.querySelector<HTMLElement>(
        '.book-footnote-card',
      )?.style as CSSStyleDeclaration
      // In pixels, the window's own unit: `45vh` would read as 45 here.
      expect(style.maxHeight, JSON.stringify(near)).toMatch(/^\d+(\.\d+)?px$/)
      const tallest = Number.parseFloat(style.maxHeight)
      const [top, bottom] = [style.top, style.bottom].map(Number.parseFloat)
      const extent = style.top
        ? [top, top + tallest]
        : style.bottom
          ? [640 - bottom - tallest, 640 - bottom]
          : [(640 - tallest) / 2, (640 + tallest) / 2]
      expect(extent[0], JSON.stringify(near)).toBeGreaterThanOrEqual(0)
      expect(extent[1], JSON.stringify(near)).toBeLessThanOrEqual(640)
    }
    vi.unstubAllGlobals()
  })

  it('takes the reader’s page, which it sits outside', () => {
    expect(opened({ page: 'paper' }).dialog.dataset.page).toBe('paper')
    expect(opened().dialog.dataset.page).toBeUndefined()
  })

  it('names the note’s language', () => {
    const { dialog } = opened({ lang: 'ja' })
    expect(
      dialog.querySelector('.book-footnote-card')?.getAttribute('lang'),
    ).toBe('ja')
  })
})
