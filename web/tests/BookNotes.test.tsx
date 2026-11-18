import { act } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { Book } from '../src/book'
import { NoteEditor, NoteList, SelectionBar } from '../src/BookNotes'
import type { Note } from '../src/notes'
import { mount } from './support/mount'

const book: Book = {
  title: 'T',
  hub: 'en',
  editions: {
    en: {
      label: 'English',
      language: 'en',
      blocks: [
        { text: 'One two three.', heading: false, sentences: [[0, 14]] },
        { text: 'Four five.', heading: false, sentences: [[0, 10]] },
      ],
    },
    ko: {
      label: 'Korean',
      language: 'ko',
      blocks: [{ text: '하나 둘.', heading: false, sentences: [[0, 5]] }],
    },
  },
  alignments: [],
}

const note = (over: Partial<Note>): Note => ({
  id: 'n',
  edition: 'en',
  slot: 0,
  body: '',
  created: '2026-11-15T10:00:00.000Z',
  updated: '2026-11-15T10:00:00.000Z',
  ...over,
})

describe('NoteEditor', () => {
  it('rings the usual colour for a note that names none, and quotes nothing where its block is gone', () => {
    const host = mount(
      <NoteEditor
        book={book}
        note={note({ slot: 9 })}
        onChange={vi.fn()}
        onDelete={vi.fn()}
        onClose={vi.fn()}
      />,
    )
    expect(
      host.querySelector('[aria-pressed="true"]')?.getAttribute('aria-label'),
    ).toBe('amber')
    expect(host.querySelector('.book-note-quote')).toBeNull()
  })

  it('writes nothing on leaving a box whose words did not change', () => {
    const onChange = vi.fn()
    const host = mount(
      <NoteEditor
        book={book}
        note={note({ body: 'same' })}
        onChange={onChange}
        onDelete={vi.fn()}
        onClose={vi.fn()}
      />,
    )
    const box = host.querySelector('textarea') as HTMLTextAreaElement
    act(() => box.focus())
    act(() => box.blur())
    expect(onChange).not.toHaveBeenCalled()
  })
})

describe('NoteList', () => {
  it('runs in book order: by edition, then block, then where a highlight starts', () => {
    const notes = [
      note({ id: 'k', edition: 'ko', slot: 0 }),
      note({ id: 'late', slot: 0, text: { start: 8, end: 13 } }),
      note({ id: 'b', slot: 1, body: 'said' }),
      note({ id: 'early', slot: 0, text: { start: 0, end: 3 } }),
      note({ id: 'whole', slot: 0 }),
    ]
    const onOpen = vi.fn()
    const host = mount(
      <NoteList
        book={book}
        notes={notes}
        onOpen={onOpen}
        onClose={vi.fn()}
        onExport={vi.fn()}
      />,
    )
    const rows = [...host.querySelectorAll('li button')]
    expect(rows.map((row) => row.textContent)).toEqual([
      'One',
      'One two three.',
      'three',
      'Four five.said',
      '하나 둘.',
    ])
    act(() => (rows[4] as HTMLElement).click())
    expect(onOpen).toHaveBeenCalledWith(notes[0])
  })
})

describe('SelectionBar', () => {
  afterEach(() => vi.restoreAllMocks())

  /** Where a 300 by 50 bar is set, in a 1024 by 768 window, for a selection `near`. */
  function placed(
    near: { left: number; top: number; right: number; bottom: number },
    vertical: boolean,
  ) {
    expect([window.innerWidth, window.innerHeight]).toEqual([1024, 768])
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue(
      new DOMRect(0, 0, 300, 50),
    )
    const host = mount(
      <SelectionBar
        near={near}
        vertical={vertical}
        onHighlight={() => {}}
        onNote={() => {}}
        onLookUp={() => {}}
      />,
    )
    const bar = host.querySelector<HTMLElement>('.book-selection-bar')
    expect(bar?.style.visibility).toBe('')
    return [bar?.style.left, bar?.style.top]
  }

  it('sits under a horizontal selection, or over it at the bottom', () => {
    expect(
      placed({ left: 100, top: 200, right: 160, bottom: 220 }, false),
    ).toEqual(['100px', '228px'])
    expect(
      placed({ left: 100, top: 700, right: 160, bottom: 720 }, false),
    ).toEqual(['100px', '642px'])
  })

  it('sits left of a vertical selection, or right of it at the left edge', () => {
    expect(
      placed({ left: 900, top: 80, right: 920, bottom: 700 }, true),
    ).toEqual(['592px', '80px'])
    expect(
      placed({ left: 100, top: 80, right: 120, bottom: 700 }, true),
    ).toEqual(['128px', '80px'])
  })

  it('is held inside the window', () => {
    expect(
      placed({ left: 1000, top: 760, right: 1020, bottom: 766 }, false),
    ).toEqual(['716px', '702px'])
  })
})
