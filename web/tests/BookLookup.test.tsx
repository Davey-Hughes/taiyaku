import { act } from 'react'
import { describe, expect, it, vi } from 'vitest'
import type { ChapterAt, CharacterCard } from '../src/book'
import { LookupPanel } from '../src/BookLookup'
import { ADA, MARA, NED } from './support/characters'
import { mount } from './support/mount'

/** The panel for `hablaron`, as a lookup stands at `looked`. */
const shown = (looked: Parameters<typeof LookupPanel>[0]['looked']) =>
  mount(
    <LookupPanel
      word="hablaron"
      looked={looked}
      onSave={vi.fn()}
      onClose={vi.fn()}
    />,
  )

describe('the look-up panel', () => {
  it('says it is looking, and what went wrong where it failed', () => {
    expect(shown({ kind: 'looking' }).textContent).toContain('Looking it up…')
    expect(shown({ kind: 'failed', why: 'Error: 500' }).textContent).toContain(
      'Error: 500',
    )
  })

  it('says when the dictionary holds no such word', () => {
    expect(shown({ kind: 'found', entries: [] }).textContent).toContain(
      'Not in the dictionary.',
    )
  })

  it('names the form a headword was found by, and no part of speech it lacks', () => {
    const host = shown({
      kind: 'found',
      entries: [
        { word: 'hablar', pos: 'verb', glosses: ['to speak'], via: 'hablaron' },
        { word: 'Hablaron', pos: '', glosses: ['a name'], via: null },
      ],
    })
    const heads = [...host.querySelectorAll('.book-lookup-entry > p')].map(
      (p) => p.textContent,
    )
    expect(heads).toEqual(['hablar verb · from hablaron', 'Hablaron '])
    expect(host.querySelectorAll('.book-lookup-entry em')).toHaveLength(1)
  })
})

describe('the cards above the entries', () => {
  /** The panel for `Reed` with one entry found and `cards` above it. */
  const carded = (cards: CharacterCard[]) =>
    mount(
      <LookupPanel
        word="Reed"
        looked={{
          kind: 'found',
          entries: [
            { word: 'reed', pos: 'noun', glosses: ['a tall grass'], via: null },
          ],
        }}
        cards={cards}
        onSave={vi.fn()}
        onClose={vi.fn()}
      />,
    )
  const cardsIn = (host: HTMLElement) => [
    ...host.querySelectorAll('section.book-lookup-character'),
  ]
  const textOf = (element: Element | null | undefined) =>
    element?.textContent ?? ''
  const keepers = (chapter: number, chapterTitle: string): ChapterAt => ({
    book: 'The Lamp Keepers',
    chapter,
    chapterTitle,
  })
  /** What the "overturned" mark says of itself to a pointer resting on it. */
  const WRONG = 'The story later showed this to be wrong'
  /** The lines of a card's list of who the character is, each as its tag and text. */
  const about = (card: Element | undefined) =>
    [...(card?.querySelector('dl.book-lookup-about')?.children ?? [])].map(
      (line) => [line.tagName, line.textContent],
    )
  const factsIn = (within: Element) => [
    ...within.querySelectorAll<HTMLElement>('li.book-lookup-fact'),
  ]
  /** The row a fact is pressed by. */
  const rowOf = (fact: Element | undefined) =>
    fact?.querySelector<HTMLButtonElement>('button.book-lookup-fact-row')
  /** What an open fact adds under its row, each line as its tag, class and text. */
  const opened = (fact: Element | undefined) =>
    [...(fact?.querySelector('.book-lookup-more')?.children ?? [])].map(
      (line) => [line.tagName, line.className, line.textContent],
    )

  it('draws each card above the dictionary’s entries, in the answer’s order', () => {
    const host = carded([NED, ADA])
    const entry = host.querySelector('.book-lookup-entry')
    if (!entry) throw new Error('no entry drawn')
    expect(
      cardsIn(host).map(
        (card) =>
          (card.compareDocumentPosition(entry) &
            Node.DOCUMENT_POSITION_FOLLOWING) !==
          0,
      ),
    ).toEqual([true, true])
    expect(
      cardsIn(host).map((card) => card.querySelector('h3')?.textContent),
    ).toEqual(['Ned Reed', 'Ada Reed'])
  })

  it('a card names its other names and where it was first met and last seen', () => {
    const [mara, ned] = cardsIn(carded([MARA, NED]))
    expect(textOf(mara?.querySelector('.book-lookup-aliases'))).toContain(
      'Mrs. Vell',
    )
    expect(ned?.querySelector('.book-lookup-aliases')).toBeNull()
    const first = textOf(mara?.querySelector('[data-met="first"]'))
    expect(first).toContain('The Lamp Keepers')
    expect(first).toMatch(/\b2\b/)
    expect(first).toContain('Dusk')
    const last = textOf(mara?.querySelector('[data-met="last"]'))
    expect(last).toContain('The Lamp Keepers')
    expect(last).toMatch(/\b3\b/)
    expect(last).toContain('Return')
  })

  it('keeps every card inside the one .book-lookup-characters', () => {
    const host = carded([NED, ADA])
    const places = [...host.querySelectorAll('.book-lookup-characters')]
    expect(places).toHaveLength(1)
    expect(cardsIn(host).map((card) => card.parentElement)).toEqual([
      places[0],
      places[0],
    ])
  })

  it('says who a character is as labels beside their values', () => {
    const [mara, ned] = cardsIn(carded([MARA, NED]))
    expect(about(mara)).toEqual([
      ['DT', 'Also called'],
      ['DD', 'Mrs. Vell'],
      ['DT', 'First met'],
      ['DD', 'The Lamp Keepers, chapter 2, Dusk'],
      ['DT', 'Last seen'],
      ['DD', 'The Lamp Keepers, chapter 3, Return'],
    ])
    expect(about(ned)).toEqual([
      ['DT', 'First met'],
      ['DD', 'The Lamp Keepers, chapter 1, The Lamp'],
      ['DT', 'Last seen'],
      ['DD', 'The Lamp Keepers, chapter 3, Return'],
    ])
  })

  it('leaves a chapter’s number out where its title carries it, and keeps it where it does not', () => {
    /** Where a card first met in chapter 4, so titled, says that was. */
    const met = (chapterTitle: string) =>
      textOf(
        cardsIn(
          carded([{ ...ADA, first: keepers(4, chapterTitle) }]),
        )[0]?.querySelector('[data-met="first"]'),
      )
    expect(met('Chapter 4 - Embers')).toBe(
      'The Lamp Keepers, Chapter 4 - Embers',
    )
    expect(met('4장 불씨')).toBe('The Lamp Keepers, 4장 불씨')
    expect(met('4')).toBe('The Lamp Keepers, 4')
    expect(met('Chapter 14 - Ash')).toBe(
      'The Lamp Keepers, chapter 4, Chapter 14 - Ash',
    )
    expect(met('The 40 Wicks')).toBe(
      'The Lamp Keepers, chapter 4, The 40 Wicks',
    )
    expect(met('')).toBe('The Lamp Keepers, chapter 4')
  })

  it('sets a character’s facts under the other character’s name, said once', () => {
    const [ned, ada] = cardsIn(carded([NED, ADA]))
    expect(
      [...(ned?.querySelectorAll('section.book-lookup-relation') ?? [])].map(
        (relation) => [
          relation.querySelector(':scope > h4')?.textContent,
          factsIn(relation).map((fact) => [
            fact.getAttribute('data-kind'),
            fact.textContent,
          ]),
        ],
      ),
    ).toEqual([
      [
        'Tom Field',
        [
          ['friend', 'they keep the lamp togetheroverturned'],
          ['mentor', 'Tom taught him the wick'],
        ],
      ],
      ['Mara Vell', [['family', 'cousins']]],
    ])
    expect(ada?.querySelector('.book-lookup-relation')).toBeNull()
  })

  it('a fact is a row to press, which opens its quote and the chapter the books say it in, and closes them', () => {
    const host = carded([NED])
    const [friend, mentor] = factsIn(host)
    expect(
      factsIn(host).map((fact) =>
        [...fact.querySelectorAll('button')].map((button) => [
          button.className,
          button.getAttribute('aria-expanded'),
        ]),
      ),
    ).toEqual([
      [['book-lookup-fact-row', 'false']],
      [['book-lookup-fact-row', 'false']],
      [['book-lookup-fact-row', 'false']],
    ])
    expect(host.querySelector('.book-lookup-more')).toBeNull()
    act(() => rowOf(mentor)?.click())
    expect(
      factsIn(host).map((fact) => rowOf(fact)?.getAttribute('aria-expanded')),
    ).toEqual(['false', 'true', 'false'])
    expect(opened(mentor)).toEqual([
      [
        'BLOCKQUOTE',
        'book-lookup-quote',
        'Tom Field showed Ned Reed the wick.',
      ],
      ['P', 'book-lookup-where', 'The Lamp Keepers, chapter 1, The Lamp'],
    ])
    expect(opened(friend)).toEqual([])
    act(() => rowOf(mentor)?.click())
    expect(host.querySelector('.book-lookup-more')).toBeNull()
    expect(rowOf(mentor)?.getAttribute('aria-expanded')).toBe('false')
  })

  it('says where a fact is first and last said when those are two chapters, or one chapter of two books', () => {
    const [cousins] = NED.relations[1]?.facts ?? []
    if (!cousins) throw new Error('no fact with Mara')
    const said = (last: ChapterAt) => {
      const host = carded([
        {
          ...ADA,
          relations: [{ with: MARA.name, facts: [{ ...cousins, last }] }],
        },
      ])
      const [fact] = factsIn(host)
      act(() => rowOf(fact)?.click())
      return opened(fact).slice(1)
    }
    expect(said(keepers(3, 'Return'))).toEqual([
      ['P', 'book-lookup-where', 'First: The Lamp Keepers, chapter 2, Dusk'],
      ['P', 'book-lookup-where', 'Last: The Lamp Keepers, chapter 3, Return'],
    ])
    expect(
      said({ book: 'The Wick Menders', chapter: 2, chapterTitle: 'Tallow' }),
    ).toEqual([
      ['P', 'book-lookup-where', 'First: The Lamp Keepers, chapter 2, Dusk'],
      ['P', 'book-lookup-where', 'Last: The Wick Menders, chapter 2, Tallow'],
    ])
  })

  it('tags an overturned fact, and open says what overturned it, with that quote and chapter', () => {
    const host = carded([NED])
    const [friend, mentor] = factsIn(host)
    expect(
      factsIn(host).map((fact) => fact.hasAttribute('data-overturned')),
    ).toEqual([true, false, false])
    expect(
      factsIn(host).map((fact) =>
        [...(rowOf(fact)?.children ?? [])].map((part) => [
          part.className,
          part.textContent,
          part.getAttribute('title'),
        ]),
      ),
    ).toEqual([
      [
        ['book-lookup-fact-note', 'they keep the lamp together', null],
        ['book-lookup-fact-tag', 'overturned', WRONG],
      ],
      [['book-lookup-fact-note', 'Tom taught him the wick', null]],
      [['book-lookup-fact-note', 'cousins', null]],
    ])
    act(() => rowOf(friend)?.click())
    act(() => rowOf(mentor)?.click())
    expect(opened(friend)).toEqual([
      [
        'BLOCKQUOTE',
        'book-lookup-quote',
        'Ned Reed and Tom Field kept it burning.',
      ],
      ['P', 'book-lookup-where', 'The Lamp Keepers, chapter 1, The Lamp'],
      ['P', 'book-lookup-overturn', 'Overturned: Tom only pretended'],
      [
        'BLOCKQUOTE',
        'book-lookup-quote',
        'Ned Reed saw that Tom Field had lied.',
      ],
      ['P', 'book-lookup-where', 'The Lamp Keepers, chapter 3, Return'],
    ])
    expect(
      [...friend.querySelectorAll('.book-lookup-overturn [title]')].map(
        (label) => [label.textContent, label.getAttribute('title')],
      ),
    ).toEqual([['Overturned:', WRONG]])
    expect(opened(mentor).map(([, name]) => name)).toEqual([
      'book-lookup-quote',
      'book-lookup-where',
    ])
  })

  it('no card and no room for one when the answer names no one', () => {
    const host = carded([])
    expect(host.querySelector('.book-lookup-entry')).not.toBeNull()
    expect(host.querySelector('.book-lookup-characters')).toBeNull()
  })
})
