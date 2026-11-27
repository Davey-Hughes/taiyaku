import type {
  ChapterAt,
  CharacterCard,
  CharacterName,
  Characters,
} from '../../src/book'

/*
 * Cards after the invented books the server's tests store
 * (`taiyaku-library`'s `test_support`).
 */

const keepers = (chapter: number, chapterTitle: string): ChapterAt => ({
  book: 'The Lamp Keepers',
  chapter,
  chapterTitle,
})

const named = (
  text: string,
  given: string | null,
  surname: string | null,
  title: string | null = null,
  titleSex: CharacterName['titleSex'] = null,
): CharacterName => ({ text, given, surname, title, titleSex })

const NED_REED = named('Ned Reed', 'Ned', 'Reed')
const TOM_FIELD = named('Tom Field', 'Tom', 'Field')
const MARA_VELL = named('Mara Vell', 'Mara', 'Vell')

/** Cousins, from the second chapter. */
const COUSINS = {
  kind: 'family',
  from: null,
  note: 'cousins',
  quote: 'Ned Reed knew his cousin at once.',
  first: keepers(2, 'Dusk'),
  last: keepers(2, 'Dusk'),
  overturned: null,
}

/** Two facts with Tom, the first taken back in the third chapter; a third with Mara. */
export const NED: CharacterCard = {
  name: NED_REED,
  aliases: [],
  matched: { text: 'Ned Reed', part: 'whole' },
  first: keepers(1, 'The Lamp'),
  last: keepers(3, 'Return'),
  relations: [
    {
      with: TOM_FIELD,
      facts: [
        {
          kind: 'friend',
          from: null,
          note: 'they keep the lamp together',
          quote: 'Ned Reed and Tom Field kept it burning.',
          first: keepers(1, 'The Lamp'),
          last: keepers(1, 'The Lamp'),
          overturned: {
            note: 'Tom only pretended',
            quote: 'Ned Reed saw that Tom Field had lied.',
            at: keepers(3, 'Return'),
          },
        },
        {
          kind: 'mentor',
          from: 'with',
          note: 'Tom taught him the wick',
          quote: 'Tom Field showed Ned Reed the wick.',
          first: keepers(1, 'The Lamp'),
          last: keepers(1, 'The Lamp'),
          overturned: null,
        },
      ],
    },
    { with: MARA_VELL, facts: [COUSINS] },
  ],
}

/** Met and last seen in the first chapter, with no relation. */
export const ADA: CharacterCard = {
  name: named('Ada Reed', 'Ada', 'Reed'),
  aliases: [],
  matched: { text: 'Ada Reed', part: 'whole' },
  first: keepers(1, 'The Lamp'),
  last: keepers(1, 'The Lamp'),
  relations: [],
}

/** Met in the second chapter, seen again in the third, also called Mrs. Vell. */
export const MARA: CharacterCard = {
  name: MARA_VELL,
  aliases: [named('Mrs. Vell', null, 'Vell', 'Mrs.', 'female')],
  matched: { text: 'Mara Vell', part: 'whole' },
  first: keepers(2, 'Dusk'),
  last: keepers(3, 'Return'),
  relations: [{ with: NED_REED, facts: [COUSINS] }],
}

/** The characters route's answer naming `cards`, for a book it holds. */
export const naming = (...cards: CharacterCard[]): Characters => ({
  known: true,
  scope: null,
  narrowed: null,
  characters: cards,
})
