import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  blockAt,
  columns,
  type Block,
  type Book,
  CHARACTERS_BEFORE_UNITS,
  charactersBefore,
  charactersText,
  excerpt,
  excerptsIn,
  type Mark,
  pinnedBlocks,
  pinRequest,
  contents,
  counterpart,
  editionOrder,
  firstSentences,
  getCharacters,
  lineAt,
  entryAt,
  nextEdition,
  rows,
  sentenceAt,
  sentenceMap,
  sentenceSpan,
  sentenceExcerpts,
  sentencesIn,
  sentenceTexts,
  span,
  starts,
  through,
} from '../src/book'
import { naming } from './support/characters'

/**
 * `en` the hub; `ko` splits `en`'s block 1 in two; `fr` lacks `en`'s block 2;
 * neither has `en`'s last block, 4.
 */
const book: Book = {
  title: 'T',
  hub: 'en',
  editions: {
    en: { label: 'English', language: null, blocks: [] },
    ko: { label: 'Korean', language: null, blocks: [] },
    fr: { label: 'French', language: null, blocks: [] },
  },
  alignments: [
    {
      to: 'ko',
      pins: [],
      links: [
        [0, 1, 0, 1],
        [1, 2, 1, 3],
        [2, 3, 3, 4],
        [3, 4, 4, 5],
        [4, 5, 5, 5],
      ],
      sentences: [
        [0, 1, 0, 1],
        [1, 2, 1, 3],
        [2, 3, 3, 4],
        [3, 4, 4, 5],
        [4, 5, 5, 5],
      ],
    },
    {
      to: 'fr',
      pins: [],
      links: [
        [0, 1, 0, 1],
        [1, 2, 1, 2],
        [2, 3, 2, 2],
        [3, 4, 2, 3],
        [4, 5, 3, 3],
      ],
      sentences: [
        [0, 1, 0, 1],
        [1, 2, 1, 2],
        [2, 3, 2, 2],
        [3, 4, 2, 3],
        [4, 5, 3, 3],
      ],
    },
  ],
}

describe('a swap', () => {
  it('cycles from the hub through the others in key order', () => {
    expect(editionOrder(book)).toEqual(['en', 'fr', 'ko'])
    expect(nextEdition(book, 'en')).toBe('fr')
    expect(nextEdition(book, 'ko')).toBe('en')
  })

  it('lands on the first block of the counterpart', () => {
    expect(counterpart(book, 'en', 'ko', 1)).toBe(1)
    expect(counterpart(book, 'en', 'ko', 2)).toBe(3)
    expect(counterpart(book, 'ko', 'en', 2)).toBe(1)
    expect(counterpart(book, 'en', 'en', 3)).toBe(3)
  })

  it('goes through the hub between two other editions', () => {
    expect(counterpart(book, 'ko', 'fr', 3)).toBe(2)
    expect(counterpart(book, 'fr', 'ko', 1)).toBe(1)
  })

  it('lands a block one edition lacks on the next it has, or its last', () => {
    expect(counterpart(book, 'en', 'fr', 2)).toBe(2)
    expect(counterpart(book, 'en', 'fr', 4)).toBe(2)
    expect(counterpart(book, 'en', 'ko', 4)).toBe(4)
  })
})

describe('the span a block maps to', () => {
  it('is every block of the other edition that shows it', () => {
    expect(span(book, 'en', 'ko', 1)).toEqual([1, 3])
    expect(span(book, 'ko', 'en', 2)).toEqual([1, 2])
    expect(span(book, 'en', 'en', 3)).toEqual([3, 4])
  })

  it('goes through the hub between two other editions', () => {
    expect(span(book, 'ko', 'fr', 2)).toEqual([1, 2])
    expect(span(book, 'fr', 'ko', 2)).toEqual([4, 5])
  })

  it('is empty where the other edition has nothing for it', () => {
    expect(span(book, 'en', 'ko', 4)).toEqual([0, 0])
    expect(span(book, 'ko', 'fr', 3)).toEqual([0, 0])
  })
})

describe('two editions row by row', () => {
  const five = {
    label: 'English',
    language: null,
    blocks: Array(5).fill({ text: 'x', heading: false, sentences: [[0, 1]] }),
  }
  const withHub: Book = { ...book, editions: { ...book.editions, en: five } }

  it('pairs the hub with another, a split block beside its whole', () => {
    expect(rows(withHub, 'en', 'ko')).toEqual([
      [0, 1, 0, 1],
      [1, 2, 1, 3],
      [2, 3, 3, 4],
      [3, 4, 4, 5],
      [4, 5, 5, 5],
    ])
    expect(rows(withHub, 'ko', 'en')[1]).toEqual([1, 3, 1, 2])
  })

  it('sets three side by side, each row closing where all of them meet the hub', () => {
    expect(columns(withHub, ['en', 'ko', 'fr'])).toEqual([
      [
        [0, 1],
        [0, 1],
        [0, 1],
      ],
      [
        [1, 2],
        [1, 3],
        [1, 2],
      ],
      [
        [2, 3],
        [3, 4],
        [2, 2],
      ],
      [
        [3, 4],
        [4, 5],
        [2, 3],
      ],
      [
        [4, 5],
        [5, 5],
        [3, 3],
      ],
    ])
  })

  it('pairs two other editions through the hub, a row closing where both meet it', () => {
    expect(rows(book, 'ko', 'fr')).toEqual([
      [0, 1, 0, 1],
      [1, 3, 1, 2],
      [3, 4, 2, 2],
      [4, 5, 2, 3],
    ])
  })

  it('gives a block with no hub counterpart a row of its own', () => {
    const extra: Book = {
      ...withHub,
      alignments: [
        {
          to: 'ko',
          pins: [],
          links: [
            [0, 1, 0, 1],
            [1, 1, 1, 3],
            [1, 5, 3, 4],
          ],
          sentences: [
            [0, 1, 0, 1],
            [1, 1, 1, 3],
            [1, 5, 3, 4],
          ],
        },
      ],
    }
    expect(rows(extra, 'en', 'ko')).toEqual([
      [0, 1, 0, 1],
      [1, 1, 1, 3],
      [1, 5, 3, 4],
    ])
  })
})

/** A block of one sentence, or of none where it holds no text. */
const block = (text: string, heading: boolean): Block => ({
  text,
  heading,
  sentences: text ? [[0, text.length]] : [],
})
const heading = (text: string) => block(text, true)
const para = (text: string) => block(text, false)

describe("a book's contents", () => {
  const blocks = [
    heading('CONTENTS'),
    para('x'),
    heading('CHAPTER ONE'),
    heading('THE BOY WHO LIVED'),
    para('Mr. and Mrs. Dursley'),
    heading('CHAPTER TWO'),
  ]

  it('takes each run of headings as one entry, at its first block', () => {
    expect(contents(blocks)).toEqual([
      { block: 0, lines: ['CONTENTS'] },
      { block: 2, lines: ['CHAPTER ONE', 'THE BOY WHO LIVED'] },
      { block: 5, lines: ['CHAPTER TWO'] },
    ])
  })

  it('finds the entry a block is in, and none before the first', () => {
    const entries = contents([para('x'), ...blocks])
    expect([0, 1, 2, 3, 4, 5, 6].map((b) => entryAt(entries, b))).toEqual([
      -1, 0, 0, 1, 1, 1, 2,
    ])
  })
})

describe('how far through', () => {
  const at = starts([para('ab'), para(''), para('cdef'), para('gh')])

  it('counts characters, the length last', () => {
    expect(at).toEqual([0, 2, 2, 6, 8])
  })

  it('measures a block from the start of the book, or of a stretch of it', () => {
    expect(through(at, 0)).toBe(0)
    expect(through(at, 3)).toBe(0.75)
    expect(through(at, 4)).toBe(1)
    expect(through(at, 3, 2, 4)).toBe(2 / 3)
    expect(through(at, 2, 2)).toBe(0)
  })

  it('is at the start of a stretch with no text', () => {
    expect(through(at, 1, 1, 2)).toBe(0)
    expect(through(starts([]), 0)).toBe(0)
  })
})

describe('a pin', () => {
  const pinned: Book = {
    ...book,
    alignments: book.alignments.map((a) =>
      a.to === 'ko' ? { ...a, pins: [[3, 4]] } : a,
    ),
  }

  it('asks to join a hub block to another edition’s, whichever side the hub is on', () => {
    expect(pinRequest(book, 'en', 'ko', 2, 3)).toEqual({
      edition: 'ko',
      hub: 'en',
      hub_block: 2,
      block: 3,
    })
    expect(pinRequest(book, 'ko', 'en', 3, 2)).toEqual({
      edition: 'ko',
      hub: 'en',
      hub_block: 2,
      block: 3,
    })
  })

  it('unpins by the hub block, found from either side', () => {
    expect(pinRequest(pinned, 'en', 'ko', 3, null)).toEqual({
      edition: 'ko',
      hub: 'en',
      hub_block: 3,
      block: null,
    })
    expect(pinRequest(pinned, 'ko', 'en', 4, null)).toEqual({
      edition: 'ko',
      hub: 'en',
      hub_block: 3,
      block: null,
    })
    expect(pinRequest(pinned, 'ko', 'en', 2, null)).toBeUndefined()
  })

  it('is none between two editions neither of which is the hub', () => {
    expect(pinRequest(book, 'ko', 'fr', 1, 1)).toBeUndefined()
    expect(pinnedBlocks(pinned, 'ko', 'fr')).toEqual(new Set())
  })

  it('marks the left blocks pinned links start at', () => {
    expect(pinnedBlocks(pinned, 'en', 'ko')).toEqual(new Set([3]))
    expect(pinnedBlocks(pinned, 'ko', 'en')).toEqual(new Set([4]))
  })
})

describe('the block a fraction of the way through', () => {
  const at = starts([para('ab'), para(''), para('cdef'), para('gh')])

  it('is the block holding that character', () => {
    expect(
      [0, 0.2, 0.25, 0.5, 0.74, 0.75, 1].map((f) => blockAt(at, f)),
    ).toEqual([0, 0, 2, 2, 2, 3, 3])
  })

  it('stays within the book, and is the first of none', () => {
    expect(blockAt(at, -1)).toBe(0)
    expect(blockAt(at, 2)).toBe(3)
    expect(blockAt(starts([]), 0.5)).toBe(0)
  })
})

describe('sentences', () => {
  // `en` is the hub; its first block's two sentences are ko's three, and its
  // second sentence has nothing in fr.
  const sentenced = (texts: string[][]): Block[] =>
    texts.map((ss) => {
      let at = 0
      return {
        text: ss.join(' '),
        heading: false,
        sentences: ss.map((t) => {
          const range: [number, number] = [at, at + t.length]
          at += t.length + 1
          return range
        }),
      }
    })
  const edition = (texts: string[][]) => ({
    label: '',
    language: null,
    blocks: sentenced(texts),
  })
  const book: Book = {
    title: 'T',
    hub: 'en',
    editions: {
      en: edition([['One.', 'Two.'], ['Three.']]),
      ko: edition([['하나.', '둘', '이요.'], ['셋.']]),
      fr: edition([['Un.'], ['Trois.']]),
    },
    alignments: [
      {
        to: 'ko',
        pins: [],
        links: [
          [0, 1, 0, 1],
          [1, 2, 1, 2],
        ],
        sentences: [
          [0, 1, 0, 2],
          [1, 2, 2, 3],
          [2, 3, 3, 4],
        ],
      },
      {
        to: 'fr',
        pins: [],
        links: [
          [0, 1, 0, 1],
          [1, 2, 1, 2],
        ],
        sentences: [
          [0, 1, 0, 1],
          [1, 2, 1, 1],
          [2, 3, 1, 2],
        ],
      },
    ],
  }

  it('are numbered through the edition, block by block', () => {
    expect(firstSentences(book.editions.ko.blocks)).toEqual([0, 3, 4])
  })

  it('map to the other side of their links, to and from the hub', () => {
    expect(sentenceSpan(book, 'en', 'ko', 0)).toEqual([0, 2])
    expect(sentenceSpan(book, 'ko', 'en', 1)).toEqual([0, 1])
    expect(sentenceSpan(book, 'en', 'en', 1)).toEqual([1, 2])
  })

  it('map all at once as one at a time does, between every two editions', () => {
    const keys = ['en', 'ko', 'fr']
    for (const from of keys) {
      for (const to of keys) {
        const all = sentenceMap(book, from, to)
        expect(all.length).toBe(
          firstSentences(book.editions[from].blocks).at(-1),
        )
        all.forEach((range, s) => {
          expect([from, to, s, range]).toEqual([
            from,
            to,
            s,
            sentenceSpan(book, from, to, s),
          ])
        })
      }
    }
  })

  it('is found by the character it holds, the first where the block starts mid-way', () => {
    const blocks = book.editions.en.blocks
    expect(sentenceAt(blocks, 0, 0)).toBe(0)
    expect(sentenceAt(blocks, 0, 4)).toBe(1)
    expect(sentenceAt(blocks, 0, 99)).toBe(0)
    expect(sentenceAt(blocks, 1, 0)).toBe(2)
  })

  it('are listed through an edition, block by block', () => {
    expect(sentenceTexts(book.editions.ko)).toEqual([
      '하나.',
      '둘',
      '이요.',
      '셋.',
    ])
  })

  it('are listed with their marks, the same text', () => {
    expect(sentenceExcerpts(book.editions.ko).map((x) => x.text)).toEqual(
      sentenceTexts(book.editions.ko),
    )
    expect(excerptsIn(book.editions.ko, [2, 4]).map((x) => x.text)).toEqual(
      sentencesIn(book.editions.ko, [2, 4]),
    )
  })

  it('excerpt clips and shifts marks', () => {
    const block: Block = {
      text: 'One two. Three',
      heading: false,
      sentences: [
        [0, 8],
        [9, 14],
      ],
      marks: [{ kind: 'italic', start: 4, end: 11 }],
    }
    expect(excerpt(block, [9, 14])).toEqual({
      text: 'Three',
      marks: [{ kind: 'italic', start: 0, end: 2 }],
    })
    expect(excerpt(block, [0, 8]).marks).toEqual([
      { kind: 'italic', start: 4, end: 8 },
    ])
  })

  it('excerpt keeps a zero-width mark from its start up to its end, and at the block’s end', () => {
    const image: Mark = { kind: 'image', start: 5, end: 5, image: 0, alt: '' }
    const block: Block = {
      text: 'abcdefghij',
      heading: false,
      sentences: [],
      marks: [image],
    }
    expect(excerpt(block, [5, 8]).marks).toEqual([
      { ...image, start: 0, end: 0 },
    ])
    expect(excerpt(block, [2, 5]).marks).toEqual([])
    expect(excerpt(block, [6, 8]).marks).toEqual([])
    const last: Mark = { ...image, start: 10, end: 10 }
    expect(excerpt({ ...block, marks: [last] }, [6, 10]).marks).toEqual([
      { ...last, start: 4, end: 4 },
    ])
  })

  it('sets an image on the boundary of abutting sentences once, on the second', () => {
    const image: Mark = { kind: 'image', start: 5, end: 5, image: 0, alt: '' }
    const block: Block = {
      text: 'abcdefghij',
      heading: false,
      sentences: [
        [0, 5],
        [5, 10],
      ],
      marks: [image],
    }
    const [first, second] = sentenceExcerpts({
      label: 'x',
      language: null,
      blocks: [block],
    })
    expect(first.marks).toEqual([])
    expect(second.marks).toEqual([{ ...image, start: 0, end: 0 }])
  })

  it('read as their blocks hold them, across a block’s end', () => {
    expect(sentencesIn(book.editions.ko, [2, 4])).toEqual(['이요.', '셋.'])
    expect(sentencesIn(book.editions.ko, [1, 1])).toEqual([])
  })

  it('go through the hub between two other editions, empty where one has nothing', () => {
    expect(sentenceSpan(book, 'ko', 'fr', 3)).toEqual([1, 2])
    expect(sentenceSpan(book, 'ko', 'fr', 2)).toEqual([0, 0])
  })
})

describe('a line', () => {
  const verse = (line: number | null, text = 'a line'): Block => ({
    text,
    heading: false,
    sentences: [[0, text.length]],
    line,
  })
  const head: Block = { text: 'BOOK TWO', heading: true, sentences: [[0, 8]] }
  const blocks = [
    verse(null),
    verse(5),
    verse(null),
    verse(null),
    head,
    verse(null),
    verse(10),
  ]

  it('is the printed number, or counted on from the last one, a heading no line', () => {
    expect([1, 2, 3, 5, 6].map((b) => lineAt(blocks, b))).toEqual([
      5, 6, 7, 8, 10,
    ])
  })

  it('is none before the first number, or where the book prints none', () => {
    expect(lineAt(blocks, 0)).toBeUndefined()
    expect(lineAt([verse(null)], 0)).toBeUndefined()
  })
})

describe('charactersText', () => {
  it('cuts at 128 units and never inside a pair', () => {
    expect(charactersText('a'.repeat(200))).toBe('a'.repeat(128))
    expect(charactersText(`${'a'.repeat(127)}😀${'b'.repeat(10)}`)).toBe(
      'a'.repeat(127),
    )
    // Exactly 128 units, the 128th the second half of a pair.
    expect(charactersText(`${'a'.repeat(126)}😀`)).toBe(`${'a'.repeat(126)}😀`)
    expect(charactersText('')).toBe('')
  })
})

describe('charactersBefore', () => {
  /** Twenty five-unit words, a word starting at every multiple of 5. */
  const WORDS = 'lamp '.repeat(20)

  it('is everything before the tap in a block shorter than the bound, its closing space kept', () => {
    expect(charactersBefore('Then Ned Reed kept the lamp.', 9)).toBe(
      'Then Ned ',
    )
    expect(charactersBefore('Ned Reed kept the lamp.', 0)).toBe('')
  })

  it('starts at a word, never at the white space before one', () => {
    expect(charactersBefore('  Then Ned', 7)).toBe('Then ')
    // 64 units back from the tap is the second of two spaces.
    expect(charactersBefore(`ab  ${'c'.repeat(62)} Reed`, 67)).toBe(
      `${'c'.repeat(62)} `,
    )
  })

  it('takes the whole bound where it begins at a word, and not a unit more', () => {
    const before = charactersBefore(`xx ${'lampa '.repeat(10)}abc Reed`, 67)
    expect(before).toBe(`${'lampa '.repeat(10)}abc `)
    expect(before).toHaveLength(CHARACTERS_BEFORE_UNITS)
    // A word starts 65 units back, and the next 58.
    expect(charactersBefore(` q${'lampa '.repeat(10)}abc Reed`, 66)).toBe(
      `${'lampa '.repeat(9)}abc `,
    )
  })

  it('drops the word the bound falls inside, and begins at the next', () => {
    // 64 units back from the tap is the `a` of a `lamp`.
    expect(charactersBefore(`${WORDS}Reed`, 100)).toBe('lamp '.repeat(12))
    // And here the space before one, which is no word's start either.
    expect(charactersBefore(`xxx ${'lamp '.repeat(12)}ab Reed`, 67)).toBe(
      `${'lamp '.repeat(12)}ab `,
    )
  })

  it('is empty past the bound in a text with no white space, and all of it within', () => {
    expect(charactersBefore('灯子はそこで火を見ていた。', 5)).toBe('灯子はそこ')
    expect(charactersBefore(`${'あ'.repeat(100)}灯子`, 100)).toBe('')
    expect(charactersBefore(`${'あ'.repeat(64)}灯子`, 64)).toBe('あ'.repeat(64))
  })

  it('never cuts inside a pair, which encodeURIComponent would refuse', () => {
    // 64 units back from the tap is the second half of 😀.
    const spaced = `a😀 ${'lampa '.repeat(10)}t Reed`
    expect(spaced.charCodeAt(spaced.indexOf('Reed') - 64)).toBe(0xde00)
    expect(charactersBefore(spaced, spaced.indexOf('Reed'))).toBe(
      `${'lampa '.repeat(10)}t `,
    )
    const unspaced = `${'😀'.repeat(32)}あ灯子`
    expect(unspaced.charCodeAt(65 - 64)).toBe(0xde00)
    expect(charactersBefore(unspaced, 65)).toBe('')
  })
})

describe('getCharacters', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('sends the words before the tap last in the query, and none where there are none', async () => {
    const fetch = vi.fn(async () => Response.json(naming()))
    vi.stubGlobal('fetch', fetch)
    await getCharacters('w', 'en', {
      block: 3,
      text: 'Reed kept',
      before: 'Then Ned ',
      earlier: true,
    })
    await getCharacters('w', 'en', {
      block: 3,
      text: 'Ned Reed',
      before: '',
      earlier: true,
    })
    expect(fetch.mock.calls).toEqual([
      [
        '/api/works/w/en/book/characters?block=3&text=Reed%20kept&earlier=1&before=Then%20Ned%20',
      ],
      ['/api/works/w/en/book/characters?block=3&text=Ned%20Reed&earlier=1'],
    ])
  })

  it('asks the edition’s route with block, text and earlier', async () => {
    const fetch = vi.fn(async () =>
      Response.json({
        known: false,
        scope: null,
        narrowed: null,
        characters: [],
      }),
    )
    vi.stubGlobal('fetch', fetch)
    await getCharacters('w x', 'en', {
      block: 3,
      text: 'Ned Reed',
      earlier: false,
    })
    expect(fetch).toHaveBeenCalledWith(
      '/api/works/w%20x/en/book/characters?block=3&text=Ned%20Reed&earlier=0',
    )
  })
})
