import { describe, expect, it } from 'vitest'
import type { Book } from '../src/book'
import {
  ankiCsv,
  excerpt,
  newNote,
  notesOn,
  pieces,
  segments,
} from '../src/bookNotes'
import type { Note } from '../src/notes'

/** `en` the hub; `ko` splits `en`'s block 1 in two. */
const book: Book = {
  title: 'T',
  hub: 'en',
  editions: {
    en: {
      label: 'English',
      language: 'en',
      blocks: [
        { text: 'CHAPTER ONE', heading: true, sentences: [[0, 11]] },
        {
          text: 'Mr. and Mrs. Dursley were proud to say.',
          heading: false,
          sentences: [[0, 39]],
        },
      ],
    },
    ko: {
      label: 'Korean',
      language: 'ko',
      blocks: [
        { text: '1장', heading: true, sentences: [[0, 2]] },
        { text: '더즐리 부부는', heading: false, sentences: [[0, 7]] },
        { text: '자랑했다.', heading: false, sentences: [[0, 5]] },
      ],
    },
  },
  alignments: [
    {
      to: 'ko',
      links: [
        [0, 1, 0, 1],
        [1, 2, 1, 3],
      ],
      sentences: [
        [0, 1, 0, 1],
        [1, 2, 1, 3],
      ],
      pins: [],
    },
  ],
}

const note = (over: Partial<Note>): Note => ({
  id: 'n',
  edition: 'en',
  slot: 1,
  body: '',
  created: '2026-11-15T10:00:00.000Z',
  updated: '2026-11-15T10:00:00.000Z',
  ...over,
})

describe('the notes an edition shows', () => {
  it('shows its own on their block, and another edition’s where a swap from it lands', () => {
    const own = note({ id: 'a', edition: 'en', slot: 1 })
    const carried = note({ id: 'b', edition: 'ko', slot: 2 })
    const elsewhere = note({ id: 'c', edition: 'raw', slot: 0 })
    const on = notesOn(book, 'en', [own, carried, elsewhere])
    expect([...on.entries()]).toEqual([
      [
        1,
        [
          { note: own, own: true },
          { note: carried, own: false },
        ],
      ],
    ])
    expect([...notesOn(book, 'ko', [own]).entries()]).toEqual([
      [1, [{ note: own, own: false }]],
    ])
  })
})

describe('a block cut by its highlights', () => {
  const text = 'Mr. and Mrs. Dursley'

  it('is one run with none', () => {
    expect(segments(text, [])).toEqual([{ text }])
  })

  it('marks each highlight’s run, the newest on top where two overlap', () => {
    const older = note({ id: 'o', text: { start: 0, end: 7 } })
    const newer = note({
      id: 'w',
      text: { start: 4, end: 12 },
      created: '2026-11-15T11:00:00.000Z',
    })
    expect(segments(text, [newer, older])).toEqual([
      { text: 'Mr. ', note: older },
      { text: 'and Mrs.', note: newer },
      { text: ' Dursley' },
    ])
  })

  it('leaves out a paragraph note, and keeps a run inside the text', () => {
    const whole = note({ id: 'p' })
    const past = note({ id: 'x', text: { start: 13, end: 99 } })
    expect(segments(text, [whole, past])).toEqual([
      { text: 'Mr. and Mrs. ' },
      { text: 'Dursley', note: past },
    ])
  })
})

describe('a block cut into its sentences', () => {
  const text = 'He came. He saw it.'
  const sentences: [number, number][] = [
    [0, 8],
    [9, 19],
  ]

  it('is each sentence, and the space between them on its own', () => {
    expect(pieces(text, sentences, [])).toEqual([
      { sentence: 0, runs: [{ text: 'He came.' }] },
      { runs: [{ text: ' ' }] },
      { sentence: 1, runs: [{ text: 'He saw it.' }] },
    ])
  })

  it('keeps a highlight across two sentences, cut where they part', () => {
    const across = note({ id: 'a', text: { start: 3, end: 12 } })
    expect(pieces(text, sentences, [across])).toEqual([
      {
        sentence: 0,
        runs: [{ text: 'He ' }, { text: 'came.', note: across }],
      },
      { runs: [{ text: ' ', note: across }] },
      {
        sentence: 1,
        runs: [{ text: 'He ', note: across }, { text: 'saw it.' }],
      },
    ])
  })

  it('is the whole text, as one run, where the block holds no sentence', () => {
    expect(pieces('  ', [], [])).toEqual([{ runs: [{ text: '  ' }] }])
  })
})

describe('a new note', () => {
  it('is a highlight with a run, and a paragraph note without', () => {
    const now = new Date('2026-11-15T12:00:00.000Z')
    const marked = newNote('ko', 2, 'rose', now, { start: 1, end: 3 })
    expect(marked).toMatchObject({
      edition: 'ko',
      slot: 2,
      text: { start: 1, end: 3 },
      color: 'rose',
      body: '',
      created: '2026-11-15T12:00:00.000Z',
      updated: '2026-11-15T12:00:00.000Z',
    })
    expect(marked.id).toMatch(/^[0-9A-Z]{26}$/)
    expect('text' in newNote('ko', 2, 'rose', now)).toBe(false)
  })
})

describe('what a note marks', () => {
  it('is its highlighted words, or its block’s opening, cut to a length', () => {
    expect(excerpt(book, note({ text: { start: 4, end: 20 } }))).toBe(
      'and Mrs. Dursley',
    )
    expect(excerpt(book, note({}), 12)).toBe('Mr. and Mrs…')
    expect(excerpt(book, note({ edition: 'gone' }))).toBe('')
  })
})

describe('words for Anki', () => {
  it('sets each highlight’s words against what its note says over its sentence', () => {
    const word = note({
      id: 'w',
      text: { start: 13, end: 20 },
      body: 'Dursley [name]: a surname',
    })
    const bare = note({ id: 'b', text: { start: 0, end: 3 } })
    const paragraph = note({ id: 'p' })
    expect(ankiCsv(book, [word, bare, paragraph]).split('\n')).toEqual([
      'Dursley,Dursley [name]: a surname<br><i>Mr. and Mrs. Dursley were proud to say.</i>',
      'Mr.,<i>Mr. and Mrs. Dursley were proud to say.</i>',
    ])
  })

  it('gives a word the sentence it was read in, not its paragraph’s last', () => {
    const two: Book = {
      ...book,
      editions: {
        ...book.editions,
        en: {
          ...book.editions.en,
          blocks: [
            book.editions.en.blocks[0],
            {
              text: 'They were proud. They were normal.',
              heading: false,
              sentences: [
                [0, 16],
                [17, 34],
              ],
            },
          ],
        },
      },
    }
    const word = note({ id: 'w', text: { start: 10, end: 15 } })
    expect(ankiCsv(two, [word])).toBe('proud,<i>They were proud.</i>')
  })

  it('quotes a field holding a comma or a quote', () => {
    const word = note({
      id: 'q',
      text: { start: 0, end: 7 },
      body: 'a "title", said before a name',
    })
    expect(ankiCsv(book, [word])).toBe(
      'Mr. and,"a ""title"", said before a name<br><i>Mr. and Mrs. Dursley were proud to say.</i>"',
    )
  })

  it('quotes a field holding a carriage return', () => {
    const word = note({ id: 'r', text: { start: 0, end: 3 }, body: 'a\rb' })
    expect(ankiCsv(book, [word])).toBe(
      'Mr.,"a\rb<br><i>Mr. and Mrs. Dursley were proud to say.</i>"',
    )
  })

  it('escapes the words, the note and the sentence as HTML, keeping its own markup', () => {
    const less: Book = {
      ...book,
      editions: {
        ...book.editions,
        en: {
          ...book.editions.en,
          blocks: [
            book.editions.en.blocks[0],
            {
              text: 'Is 1 < 2 & 3 > 2? Yes.',
              heading: false,
              sentences: [
                [0, 17],
                [18, 22],
              ],
            },
          ],
        },
      },
    }
    const word = note({ id: 'h', text: { start: 3, end: 8 }, body: 'x<y & z' })
    expect(ankiCsv(less, [word])).toBe(
      '1 &lt; 2,x&lt;y &amp; z<br><i>Is 1 &lt; 2 &amp; 3 &gt; 2?</i>',
    )
  })
})
