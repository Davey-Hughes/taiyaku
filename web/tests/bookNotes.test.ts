import { describe, expect, it } from 'vitest'
import type { Book, Mark } from '../src/book'
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
      { sentence: 0, runs: [{ text: 'He came.', marks: [] }] },
      { runs: [{ text: ' ', marks: [] }] },
      { sentence: 1, runs: [{ text: 'He saw it.', marks: [] }] },
    ])
  })

  it('keeps a highlight across two sentences, cut where they part', () => {
    const across = note({ id: 'a', text: { start: 3, end: 12 } })
    expect(pieces(text, sentences, [across])).toEqual([
      {
        sentence: 0,
        runs: [
          { text: 'He ', marks: [] },
          { text: 'came.', note: across, marks: [] },
        ],
      },
      { runs: [{ text: ' ', note: across, marks: [] }] },
      {
        sentence: 1,
        runs: [
          { text: 'He ', note: across, marks: [] },
          { text: 'saw it.', marks: [] },
        ],
      },
    ])
  })

  it('draws each of two overlapping sentences whole', () => {
    expect(
      pieces(
        'abcd',
        [
          [0, 2],
          [0, 4],
        ],
        [],
      ),
    ).toEqual([
      { sentence: 0, runs: [{ text: 'ab', marks: [] }] },
      { sentence: 1, runs: [{ text: 'abcd', marks: [] }] },
    ])
  })

  it('is the whole text, as one run, where the block holds no sentence', () => {
    expect(pieces('  ', [], [])).toEqual([
      { runs: [{ text: '  ', marks: [] }] },
    ])
  })
})

describe('a block cut by its marks', () => {
  const italic: Mark = { kind: 'italic', start: 2, end: 5 }

  it('cuts at mark boundaries', () => {
    expect(pieces('a big dog', [], [], [italic])).toEqual([
      {
        runs: [
          { text: 'a ', marks: [] },
          { text: 'big', marks: [italic] },
          { text: ' dog', marks: [] },
        ],
      },
    ])
  })

  it('lists the marks over a run in the order they are given', () => {
    const bold: Mark = { kind: 'bold', start: 0, end: 5 }
    expect(pieces('a big dog', [], [], [italic, bold])).toEqual([
      {
        runs: [
          { text: 'a ', marks: [bold] },
          { text: 'big', marks: [italic, bold] },
          { text: ' dog', marks: [] },
        ],
      },
    ])
  })

  it('keeps both a highlight and an italic word it covers half of', () => {
    const over = note({ id: 'h', text: { start: 3, end: 9 } })
    expect(pieces('a big dog', [], [over], [italic])).toEqual([
      {
        runs: [
          { text: 'a ', marks: [] },
          { text: 'b', marks: [italic] },
          { text: 'ig', marks: [italic], note: over },
          { text: ' dog', marks: [], note: over },
        ],
      },
    ])
  })

  it('opens a ruby base cut by a sentence end only on its first run', () => {
    const ruby: Mark = { kind: 'ruby', start: 1, end: 3, reading: 'あい' }
    expect(
      pieces(
        'x愛情y',
        [
          [0, 2],
          [2, 4],
        ],
        [],
        [ruby],
      ),
    ).toEqual([
      {
        sentence: 0,
        runs: [
          { text: 'x', marks: [] },
          { text: '愛', marks: [ruby], rubyStart: true },
        ],
      },
      {
        sentence: 1,
        runs: [
          { text: '情', marks: [ruby] },
          { text: 'y', marks: [] },
        ],
      },
    ])
  })

  it('opens a ruby base cut by a highlight only on its first run, losing none of the base', () => {
    const ruby: Mark = { kind: 'ruby', start: 0, end: 3, reading: 'あいう' }
    const runs = pieces(
      '愛情心',
      [],
      [note({ text: { start: 1, end: 2 } })],
      [ruby],
    ).flatMap((p) => p.runs)
    expect(runs.map((r) => [r.text, r.rubyStart])).toEqual([
      ['愛', true],
      ['情', undefined],
      ['心', undefined],
    ])
    expect(runs.map((r) => r.text).join('')).toBe('愛情心')
  })

  it('sets a zero-width image on the run after it', () => {
    const image: Mark = { kind: 'image', start: 1, end: 1, image: 4, alt: '' }
    expect(pieces('ab', [], [], [image])).toEqual([
      {
        runs: [
          { text: 'a', marks: [] },
          { text: 'b', marks: [], images: [image] },
        ],
      },
    ])
  })

  it('sets an image at the end of the text on an empty run of its own', () => {
    const image: Mark = { kind: 'image', start: 2, end: 2, image: 4, alt: '' }
    expect(pieces('ab', [[0, 2]], [], [image])).toEqual([
      {
        sentence: 0,
        runs: [
          { text: 'ab', marks: [] },
          { text: '', marks: [], images: [image] },
        ],
      },
    ])
    expect(pieces('', [], [], [{ ...image, start: 0, end: 0 }])).toEqual([
      {
        runs: [
          { text: '', marks: [], images: [{ ...image, start: 0, end: 0 }] },
        ],
      },
    ])
  })

  it('gives what it gave before where there are no marks', () => {
    const text = 'He came. He saw it.'
    const sentences: [number, number][] = [
      [0, 8],
      [9, 19],
    ]
    const across = note({ id: 'a', text: { start: 3, end: 12 } })
    expect(pieces(text, sentences, [across], [])).toEqual(
      pieces(text, sentences, [across]),
    )
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

  it('cuts a found range out as a run of its own', () => {
    expect(pieces('a big dog', [[0, 9]], [], [], [2, 5])).toEqual([
      {
        sentence: 0,
        runs: [
          { text: 'a ', marks: [] },
          { text: 'big', marks: [], found: true },
          { text: ' dog', marks: [] },
        ],
      },
    ])
  })

  it('keeps every base character of a ruby a found range and a highlight cross, the reading opened once', () => {
    const ruby: Mark = { kind: 'ruby', start: 1, end: 4, reading: 'あいう' }
    const runs = pieces(
      'x愛情心y',
      [],
      [note({ text: { start: 2, end: 3 } })],
      [ruby],
      [0, 3],
    ).flatMap((p) => p.runs)
    expect(runs.map((r) => r.text).join('')).toBe('x愛情心y')
    expect(runs.filter((r) => r.rubyStart).map((r) => r.text)).toEqual(['愛'])
    expect(
      runs
        .filter((r) => r.found)
        .map((r) => r.text)
        .join(''),
    ).toBe('x愛情')
  })

  it('leaves the runs as they were without a found range', () => {
    expect(pieces('a big dog', [[0, 9]], [], [], undefined)).toEqual(
      pieces('a big dog', [[0, 9]], []),
    )
    expect(
      pieces('a big dog', [], []).some((p) => p.runs.some((r) => 'found' in r)),
    ).toBe(false)
  })
})

describe('a number in vertical text', () => {
  const upright = (
    text: string,
    found?: [number, number],
    notes: Note[] = [],
    marks: Mark[] = [],
  ) => pieces(text, [], notes, marks, found, true).flatMap((p) => p.runs)

  it('of one or two digits is marked upright in its run, which it does not cut', () => {
    expect(upright('第12話は7日と123日')).toEqual([
      {
        text: '第12話は7日と123日',
        marks: [],
        upright: [
          [1, 3],
          [5, 6],
        ],
      },
    ])
  })

  it('is not marked where it touches another digit, full-width or not', () => {
    for (const text of ['1234', '１２3', '3１', '12３']) {
      expect(upright(text).some((r) => r.upright)).toBe(false)
    }
    expect(upright('a1b22c')).toEqual([
      {
        text: 'a1b22c',
        marks: [],
        upright: [
          [1, 2],
          [3, 5],
        ],
      },
    ])
  })

  it('is found in the block’s text, so a run cut inside a number takes its part of it and never a number of its own', () => {
    // 123 cut as 1 and 23 by a match: 23 is not a number of two digits.
    expect(upright('第123話の5', [2, 4])).toEqual([
      { text: '第1', marks: [] },
      { text: '23', marks: [], found: true },
      { text: '話の5', marks: [], upright: [[2, 3]] },
    ])
    // 12 cut as 1 and 2: each part is the number's, so each is still marked.
    expect(upright('第12話', [2, 3])).toEqual([
      { text: '第1', marks: [], upright: [[1, 2]] },
      { text: '2', marks: [], found: true, upright: [[0, 1]] },
      { text: '話', marks: [] },
    ])
  })

  it('keeps the text, the found range and a highlight over it as they were', () => {
    const text = '昭和12年に'
    const runs = upright(text, [2, 4], [note({ text: { start: 2, end: 5 } })])
    expect(runs.map((r) => r.text).join('')).toBe(text)
    expect(runs.find((r) => r.upright)).toMatchObject({
      text: '12',
      found: true,
      note: { text: { start: 2, end: 5 } },
      upright: [[0, 2]],
    })
    expect(
      runs
        .filter((r) => r.note)
        .map((r) => r.text)
        .join(''),
    ).toBe('12年')
    expect(
      runs
        .filter((r) => r.found)
        .map((r) => r.text)
        .join(''),
    ).toBe('12')
  })

  it('is marked inside a ruby’s base or a link holding more, which stays one run', () => {
    const age: Mark = { kind: 'ruby', start: 2, end: 5, reading: 'はたち' }
    expect(upright('彼は20歳になった', undefined, [], [age])).toEqual([
      { text: '彼は', marks: [] },
      { text: '20歳', marks: [age], rubyStart: true, upright: [[0, 2]] },
      { text: 'になった', marks: [] },
    ])
    const link: Mark = { kind: 'link', start: 0, end: 3, block: 3, note: false }
    expect(upright('問12を解く', undefined, [], [link])).toEqual([
      { text: '問12', marks: [link], upright: [[1, 3]] },
      { text: 'を解く', marks: [] },
    ])
  })

  it('is marked whatever other marks are over it', () => {
    for (const kind of ['bold', 'italic', 'sup', 'sub'] as const) {
      const mark: Mark = { kind, start: 0, end: 4 }
      expect(upright('第12話を', undefined, [], [mark]), kind).toEqual([
        { text: '第12話', marks: [mark], upright: [[1, 3]] },
        { text: 'を', marks: [] },
      ])
    }
  })

  it('is left unmarked in horizontal text', () => {
    expect(pieces('第12話', [], [])).toEqual([
      { runs: [{ text: '第12話', marks: [] }] },
    ])
    expect(pieces('第12話', [], [], [], undefined, false)).toEqual(
      pieces('第12話', [], []),
    )
  })
})
