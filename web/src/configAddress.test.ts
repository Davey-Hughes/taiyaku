import { describe, expect, it } from 'vitest'
import {
  accountNameClash,
  accountPath,
  byName,
  dirPath,
  LIBRARIES,
  libraryNameClash,
  libraryPath,
  NO_ADDRESS,
  nameAt,
  nameSegment,
  USERS,
} from './configAddress'

/** One name for each thing an address has to escape, and one it must not mistake for a number. */
const NAMES = [
  'Light novels',
  'Manga/Chapters',
  'What?',
  'No. #1',
  '100%',
  'Text holding %2F',
  'Tom & Jerry',
  'Émile',
  '1984',
]

describe('a library’s address', () => {
  it.each(NAMES)('puts %s in one segment and reads it back', (name) => {
    const segment = nameSegment(name)
    expect(segment).not.toMatch(/[/?#]/)
    expect(libraryPath(name)).toBe(`${LIBRARIES}/${segment}`)
    expect(nameAt(libraryPath(name), LIBRARIES)).toEqual({ name, rest: '' })
    expect(nameAt(dirPath(name, 3), LIBRARIES)).toEqual({
      name,
      rest: '/dirs/3',
    })
  })

  it('draws a space as an address carries it', () => {
    expect(libraryPath('Light novels')).toBe('/config/libraries/Light%20novels')
  })

  it('reads a bare % typed into the address bar as written', () => {
    expect(nameAt('/config/libraries/100%', LIBRARIES)).toEqual({
      name: '100%',
      rest: '',
    })
  })

  it('reads the base in either case, as the router matches it', () => {
    expect(nameAt('/Config/Libraries/Manga', LIBRARIES)).toEqual({
      name: 'Manga',
      rest: '',
    })
  })

  it.each([
    '/config/libraries',
    '/config/libraries/',
    '/config/users/Manga',
    '/config/librariesX/Manga',
  ])('reads no name from %s', (pathname) => {
    expect(nameAt(pathname, LIBRARIES)).toBeUndefined()
  })
})

describe('an account’s address', () => {
  it('puts each name in one segment of an account’s address and reads it back', () => {
    for (const name of NAMES) {
      expect(accountPath(name), name).toBe(`/config/users/${nameSegment(name)}`)
      expect(nameAt(accountPath(name), USERS), name).toEqual({ name, rest: '' })
    }
  })
})

/** Things of `names`, numbered from 1 in the order given. */
const named = (...names: string[]) =>
  names.map((name, i) => ({ id: i + 1, name }))

describe('the library a name finds', () => {
  it('finds the exact name before one that differs in case', () => {
    expect(byName(named('manga', 'Manga'), 'Manga')?.id).toBe(2)
    expect(byName(named('Manga', 'manga'), 'manga')?.id).toBe(2)
  })

  it('folds ASCII case, as the store’s NOCASE does', () => {
    expect(byName(named('Comics', 'Manga'), 'MANGA')?.id).toBe(2)
  })

  it('leaves the case of a letter outside ASCII as it is, as NOCASE does', () => {
    expect(byName(named('Émile'), 'émile')).toBeUndefined()
    expect(byName(named('Émile', 'émile'), 'émile')?.id).toBe(2)
  })

  it('finds nothing for a name no library has', () => {
    expect(byName(named('Manga'), 'Comics')).toBeUndefined()
  })
})

describe('the account a name finds', () => {
  it('folds ASCII case only where one name alone matches, as account names are unique as written', () => {
    expect(byName(named('Sam', 'kim'), 'SAM')?.id).toBe(1)
    expect(byName(named('Sam', 'sam'), 'SAM')).toBeUndefined()
    expect(byName(named('Sam', 'sam'), 'sam')?.id).toBe(2)
    expect(byName(named('Sam', 'sam'), 'Sam')?.id).toBe(1)
  })
})

describe('a library name that clashes, as the store judges it', () => {
  const held = named('Comics', 'Manga')

  it('names the library holding the name, as it holds it, folding ASCII case', () => {
    expect(libraryNameClash('comics', held, 2)).toBe(
      'a library is already called `Comics`',
    )
    expect(libraryNameClash('COMICS', held)).toBe(
      'a library is already called `Comics`; add the directory to it instead',
    )
  })

  it('leaves the case of a letter outside ASCII as it is, as NOCASE does', () => {
    expect(libraryNameClash('émile', named('Émile'))).toBeUndefined()
    expect(libraryNameClash('Émile', named('Émile'), 2)).toBe(
      'a library is already called `Émile`',
    )
  })

  it('trims the White_Space the store trims, and no more', () => {
    expect(libraryNameClash(' \u0085Comics\u3000\t', held, 2)).toBe(
      'a library is already called `Comics`',
    )
    // U+FEFF is no White_Space, so the store keeps it, and the name is new.
    expect(libraryNameClash('\uFEFFComics', held, 2)).toBeUndefined()
  })

  it('is no clash with the library’s own name, in any case', () => {
    expect(libraryNameClash('MANGA', held, 2)).toBeUndefined()
    expect(libraryNameClash('Manga', held, 1)).toBe(
      'a library is already called `Manga`',
    )
  })

  it('refuses . and .. once trimmed, and nothing else made of dots', () => {
    for (const name of ['.', '..', ' . ', '\u00A0..\n']) {
      expect(libraryNameClash(name, held), JSON.stringify(name)).toBe(
        NO_ADDRESS,
      )
    }
    for (const name of ['...', 'a.b', '. .']) {
      expect(libraryNameClash(name, held), name).toBeUndefined()
    }
  })

  it('finds nothing for a new name', () => {
    expect(libraryNameClash('Light novels', held)).toBeUndefined()
    expect(libraryNameClash('', held)).toBeUndefined()
  })
})

describe('an account name that clashes, as the store judges it', () => {
  it('clashes only with a name exactly as written, since account names are compared byte for byte', () => {
    const accounts = named('Sam', 'kim')
    expect(accountNameClash('Sam', accounts)).toBe(
      'an account is already called `Sam`',
    )
    for (const name of ['sam', 'SAM', ' Sam', 'Sam ']) {
      expect(accountNameClash(name, accounts), JSON.stringify(name)).toBe(
        undefined,
      )
    }
  })

  it('refuses . and .. exactly, untrimmed, as the store stores the name as written', () => {
    expect(accountNameClash('.', [])).toBe(NO_ADDRESS)
    expect(accountNameClash('..', [])).toBe(NO_ADDRESS)
    expect(accountNameClash(' . ', [])).toBeUndefined()
    expect(accountNameClash('...', [])).toBeUndefined()
  })
})
