import { afterEach, describe, expect, it, vi } from 'vitest'
import type { CharacterCard, CharacterName } from '../src/book'
import {
  markLookedUp,
  markMentions,
  mentionPattern,
  mentionsIn,
} from '../src/bookMentions'
import BOOK_CSS from '../src/styles/book.css?raw'
import { MARA, NED } from './support/characters'
import { rules } from './support/css'

const named = (
  text: string,
  given: string | null = null,
  surname: string | null = null,
): CharacterName => ({ text, given, surname, title: null, titleSex: null })

/** `name`'s card, as an answer that `matched` it whole by its shown name unless told. */
const card = (
  name: CharacterName,
  aliases: CharacterName[] = [],
  matched: CharacterCard['matched'] = { text: name.text, part: 'whole' },
): CharacterCard => ({ ...NED, name, aliases, matched, relations: [] })

/** What of `text` is marked while `cards` show, each mention as where it starts and its words. */
function marked(cards: CharacterCard[], text: string): [number, string][] {
  const names = mentionPattern(cards)
  if (!names) return []
  return mentionsIn(text, names).map(({ start, end }) => [
    start,
    text.slice(start, end),
  ])
}

/**
 * Ned Reed and Ada Reed, each answered for a tap on the surname alone: the
 * name matched is the whole one the part is of.
 */
const NED_BY_SURNAME = card(NED.name, [], { text: 'Ned Reed', part: 'surname' })
const ADA_BY_SURNAME = card(named('Ada Reed', 'Ada', 'Reed'), [], {
  text: 'Ada Reed',
  part: 'surname',
})
/** Tom Field, shown by a title and his surname. */
const MR_FIELD = named('Mr. Field', 'Tom', 'Field')
/** A character the books give no given name. */
const MRS_HOLT = named('Mrs. Holt', null, 'Holt')
const BORA = card(named('보라', '보라'))

describe('the names a card marks in the text', () => {
  it('marks the given name and the full name, the full name as one mention', () => {
    expect(marked([NED], 'Ned Reed lit it. Then Ned slept.')).toEqual([
      [0, 'Ned Reed'],
      [22, 'Ned'],
    ])
  })

  it('never marks a bare surname, or a title with it, which another of the family bears', () => {
    expect(marked([NED], 'Reed came. Mrs. Reed came. The Reeds came.')).toEqual(
      [],
    )
  })

  it('marks a name only as a whole word, in its own case', () => {
    expect(marked([NED], 'Nedra and McNed met ned, then Ned’s wick.')).toEqual([
      [30, 'Ned'],
    ])
  })

  it('takes a name the book breaks a line inside as one mention', () => {
    expect(marked([NED], 'Up went Ned\n  Reed.')).toEqual([[8, 'Ned\n  Reed']])
  })

  it('reads a name’s own marks as themselves, not as a pattern’s', () => {
    const tom = card(named('Tom Field', 'Tom', 'Field'), [named('Tom F.')])
    expect(marked([tom], 'Tom Fx and Tom F. went.')).toEqual([
      [0, 'Tom'],
      [11, 'Tom F.'],
    ])
  })

  it('marks another name that carries the given name, and not one that lacks it', () => {
    const mara = card(MARA.name, [
      named('Cousin Mara'),
      named('the lamp girl'),
      ...MARA.aliases,
    ])
    expect(
      marked([mara], 'Cousin Mara, the lamp girl, was Mrs. Vell by then.'),
    ).toEqual([[0, 'Cousin Mara']])
  })

  it('marks a shown name that has neither the given name nor the surname in it', () => {
    const tom = card(
      named('Old Wickhand', 'Tom', 'Field'),
      [named('Tom Field', 'Tom', 'Field')],
      { text: 'Tom Field', part: 'given' },
    )
    expect(marked([tom], 'Old Wickhand, Tom to some, was a Field.')).toEqual([
      [0, 'Old Wickhand'],
      [14, 'Tom'],
    ])
  })

  it('marks a title with the surname only where it is the shown name and the very text looked up', () => {
    const TEXT = 'Mr. Field and Tom Field and Mrs. Field.'
    expect(marked([card(MR_FIELD)], TEXT)).toEqual([
      [0, 'Mr. Field'],
      [14, 'Tom Field'],
    ])
    // Matched by a part of it, the shown name is not what was looked up.
    expect(
      marked(
        [card(MR_FIELD, [], { text: 'Mr. Field', part: 'surname' })],
        TEXT,
      ),
    ).toEqual([[14, 'Tom Field']])
    // A whole match of another of its names is not one of the shown name.
    expect(
      marked([card(MR_FIELD, [], { text: 'Tom Field', part: 'whole' })], TEXT),
    ).toEqual([[14, 'Tom Field']])
  })

  it('marks a character with no given name by its shown name and the other names that hold it', () => {
    const holt = card(MRS_HOLT, [named('old Mrs. Holt'), named('the widow')])
    expect(
      marked([holt], 'There old Mrs. Holt sat; the widow, Mrs. Holt. Holt!'),
    ).toEqual([
      [6, 'old Mrs. Holt'],
      [36, 'Mrs. Holt'],
    ])
  })

  it('marks nothing for a character shown by a surname alone, or by a common noun', () => {
    expect(mentionPattern([card(named('Holt', null, 'Holt'))])).toBeUndefined()
    expect(mentionPattern([card(named('boy'))])).toBeUndefined()
    expect(mentionPattern([])).toBeUndefined()
  })

  it('marks nothing after a tap that names several and lacks the first one’s given name', () => {
    expect(mentionPattern([NED_BY_SURNAME, ADA_BY_SURNAME])).toBeUndefined()
    // Alone, the same card says who is meant.
    expect(marked([NED_BY_SURNAME], 'Ned Reed')).toEqual([[0, 'Ned Reed']])
    // Another of his names, matched whole, with no given name in it.
    const byTitle = card(NED.name, [named('Mr. Reed', null, 'Reed')], {
      text: 'Mr. Reed',
      part: 'whole',
    })
    expect(mentionPattern([byTitle, ADA_BY_SURNAME])).toBeUndefined()
    const holts = [
      card(MRS_HOLT, [], { text: 'Mrs. Holt', part: 'surname' }),
      card(named('Mr. Holt', null, 'Holt'), [], {
        text: 'Mr. Holt',
        part: 'surname',
      }),
    ]
    expect(mentionPattern(holts)).toBeUndefined()
  })

  it('marks the first card’s names where several answer a tap that carries its given name or is its shown name', () => {
    const neds = [
      card(NED.name, [], { text: 'Ned Reed', part: 'given' }),
      card(named('Ned Holt', 'Ned', 'Holt'), [], {
        text: 'Ned Holt',
        part: 'given',
      }),
    ]
    expect(marked(neds, 'Ned Reed, not Ned Holt.')).toEqual([
      [0, 'Ned Reed'],
      [14, 'Ned'],
    ])
    const fields = [
      card(named('Mr. Field', null, 'Field')),
      card(named('Tom Field', 'Tom', 'Field'), [named('Mr. Field')], {
        text: 'Mr. Field',
        part: 'whole',
      }),
    ]
    expect(marked(fields, 'Mr. Field and Tom Field.')).toEqual([
      [0, 'Mr. Field'],
    ])
  })

  it('marks a Hangul name only where it begins a word, a particle written onto it or none', () => {
    expect(
      marked([BORA], '보라가 왔다. 눈보라가 쳤다. 그건 보라의 등불.'),
    ).toEqual([
      [0, '보라'],
      [20, '보라'],
    ])
  })

  it('marks a kana or Han name wherever it is written, straight after the word before', () => {
    expect(marked([card(named('灯子', '灯子'))], 'そこへ灯子が来た。')).toEqual(
      [[3, '灯子']],
    )
  })

  it('KNOWN WRONG: marks a conjugated Korean verb that begins like a Hangul name', () => {
    expect(marked([BORA], '그는 저쪽을 보라고 했다.')).toEqual([[7, '보라']])
  })

  it('KNOWN WRONG: misses the same character named by surname alone, or by a title and surname', () => {
    expect(marked([NED], 'Reed went up. Young Mr. Reed lit it.')).toEqual([])
  })

  it('KNOWN WRONG: misses another name of the character that lacks the given name', () => {
    expect(marked([MARA], 'Mrs. Vell came back.')).toEqual([])
  })
})

describe('the highlights the marks are painted through', () => {
  /** A browser's registry of custom highlights, each kept as the ranges it was made of. */
  function registry() {
    const marks = new Map<string, { ranges: Range[] }>()
    vi.stubGlobal('CSS', { highlights: marks })
    vi.stubGlobal(
      'Highlight',
      class {
        readonly ranges: Range[]
        constructor(...ranges: Range[]) {
          this.ranges = ranges
        }
      },
    )
    return marks
  }
  /** Stands for a range: the registry keeps what it is handed, unread. */
  const range = (name: string) => ({ name }) as unknown as Range

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('names each highlight as a ::highlight() rule of the book’s stylesheet does', () => {
    const marks = registry()
    markMentions([range('a')])
    markLookedUp(range('b'))
    const painted = rules(BOOK_CSS)
      .filter(({ body }) => body.trim() !== '')
      .flatMap(({ prelude }) =>
        [...prelude.matchAll(/::highlight\(([\w-]+)\)/g)].map((m) => m[1]),
      )
    expect([...marks.keys()].sort()).toEqual([
      'book-lookup-word',
      'book-mention',
    ])
    expect(painted.sort()).toEqual(['book-lookup-word', 'book-mention'])
  })

  it('sets the mentions in place of those marked before, and with none takes the highlight away', () => {
    const marks = registry()
    markMentions([range('a'), range('b')])
    markMentions([range('c')])
    expect(marks.get('book-mention')?.ranges).toEqual([range('c')])
    markMentions([])
    expect(marks.has('book-mention')).toBe(false)
  })

  it('marks the one word looked up, and with none takes its highlight away, the mentions left', () => {
    const marks = registry()
    markMentions([range('a')])
    markLookedUp(range('b'))
    expect(marks.get('book-lookup-word')?.ranges).toEqual([range('b')])
    markLookedUp(undefined)
    expect(marks.has('book-lookup-word')).toBe(false)
    expect(marks.get('book-mention')?.ranges).toEqual([range('a')])
  })

  it('does nothing, and throws nothing, in a browser with no custom highlights', () => {
    // Neither the registry's class,
    vi.stubGlobal('CSS', { highlights: new Map() })
    expect(() => markMentions([range('a')])).not.toThrow()
    vi.unstubAllGlobals()
    // nor `CSS`,
    vi.stubGlobal('Highlight', class {})
    expect(() => markMentions([range('a')])).not.toThrow()
    // nor a registry on it.
    vi.stubGlobal('CSS', {})
    expect(() => markLookedUp(range('b'))).not.toThrow()
  })
})
