import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  addDir,
  createLibrary,
  libraryRemovalCost,
  libraryRemovedSentence,
  libraryScanStatus,
  listDirs,
  listLibraries,
  moveCost,
  moveDir,
  noLibrarySentence,
  removeLibrary,
  renameLibrary,
  startLibraryScan,
  unlinkCost,
  unlinkDir,
  withLibraryMade,
  withMovedDir,
  withoutDir,
} from '../src/libraries'
import type {
  AroundDir,
  Library,
  LibraryDir,
  Moved,
  NestedDir,
} from '../src/libraries'
import { dir } from './support/libraries'

const answering = (body: unknown) => {
  const fetchMock = vi.fn().mockImplementation(() =>
    Promise.resolve(
      new Response(JSON.stringify(body), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    ),
  )
  vi.stubGlobal('fetch', fetchMock)
  return fetchMock
}

/** What the first request was: its path, its method, and its body parsed. */
const sent = (fetchMock: ReturnType<typeof vi.fn>) => {
  const [path, init] = fetchMock.mock.calls[0] as [string, RequestInit?]
  return {
    path,
    method: init?.method ?? 'GET',
    body:
      typeof init?.body === 'string'
        ? (JSON.parse(init.body) as unknown)
        : undefined,
  }
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('the libraries client', () => {
  it.each([
    [
      'listLibraries',
      () => listLibraries(),
      'GET',
      '/api/library/libraries',
      undefined,
    ],
    [
      'createLibrary',
      () => createLibrary('Manga', ['/media/a', '/media/b']),
      'POST',
      '/api/library/libraries',
      { name: 'Manga', dirs: ['/media/a', '/media/b'] },
    ],
    [
      'renameLibrary',
      () => renameLibrary(7, 'Comics'),
      'PATCH',
      '/api/library/libraries/7',
      { name: 'Comics' },
    ],
    [
      'addDir',
      () => addDir(7, '/media/c'),
      'POST',
      '/api/library/libraries/7/dirs',
      { path: '/media/c' },
    ],
    [
      'libraryRemovalCost',
      () => libraryRemovalCost(7),
      'DELETE',
      '/api/library/libraries/7?dry_run=true',
      undefined,
    ],
    [
      'removeLibrary',
      () => removeLibrary(7),
      'DELETE',
      '/api/library/libraries/7',
      undefined,
    ],
    [
      'moveCost',
      () => moveCost(3, 9, { library: 7 }),
      'POST',
      '/api/library/roots/3/move?dry_run=true',
      { from: 9, library: 7 },
    ],
    [
      'moveDir',
      () => moveDir(3, 9, { new_library: 'Extras' }),
      'POST',
      '/api/library/roots/3/move',
      { from: 9, new_library: 'Extras' },
    ],
    [
      'unlinkCost',
      () => unlinkCost(7, 3),
      'DELETE',
      '/api/library/libraries/7/dirs/3?dry_run=true',
      undefined,
    ],
    [
      'unlinkDir',
      () => unlinkDir(7, 3),
      'DELETE',
      '/api/library/libraries/7/dirs/3',
      undefined,
    ],
    [
      'startLibraryScan',
      () => startLibraryScan(7),
      'POST',
      '/api/library/libraries/7/scan',
      undefined,
    ],
    [
      'libraryScanStatus',
      () => libraryScanStatus(7),
      'GET',
      '/api/library/libraries/7/scan',
      undefined,
    ],
    [
      'listDirs, at the prefixes',
      () => listDirs(),
      'GET',
      '/api/library/dirs',
      undefined,
    ],
    [
      'listDirs, at a path',
      () => listDirs('/mnt/user/media 1/Manga & more'),
      'GET',
      '/api/library/dirs?path=%2Fmnt%2Fuser%2Fmedia%201%2FManga%20%26%20more',
      undefined,
    ],
  ])(
    '%s asks the route it is named for',
    async (_name, call, method, path, body) => {
      const fetchMock = answering({})
      await call()
      expect(sent(fetchMock)).toEqual({ path, method, body })
    },
  )

  it.each([
    [
      'createLibrary',
      { name: 'Manga', dirs: ['/media/a'] },
      () => createLibrary('Manga', ['/media/a'], true),
      () => createLibrary('Manga', ['/media/a'], false),
      () => createLibrary('Manga', ['/media/a']),
    ],
    [
      'addDir',
      { path: '/media/c' },
      () => addDir(7, '/media/c', true),
      () => addDir(7, '/media/c', false),
      () => addDir(7, '/media/c'),
    ],
  ])(
    '%s asks for the schedule Off when ticked, and sends no schedule when not — O275',
    async (_name, body, ticked, unticked, leftOut) => {
      const off = answering({})
      await ticked()
      expect(sent(off).body).toEqual({ ...body, scan_interval_secs: 0 })
      for (const [said, call] of [
        ['unticked', unticked],
        ['left out', leftOut],
      ] as const) {
        const none = answering({})
        await call()
        expect(sent(none).body, said).not.toHaveProperty('scan_interval_secs')
        expect(sent(none).body, said).toEqual(body)
      }
    },
  )

  it('refuses an empty 204 where a body was expected', async () => {
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockImplementation(() =>
          Promise.resolve(new Response(null, { status: 204 })),
        ),
    )
    await expect(renameLibrary(7, 'Comics')).rejects.toThrow(
      'expected a JSON body, got an empty 204',
    )
  })
})

describe('noLibrarySentence', () => {
  it('names one account twice, and no pronoun stands for it', () => {
    expect(noLibrarySentence(['alice'], 'this library')).toBe(
      'alice can see only this library. After this, alice will see no library until an administrator grants one.',
    )
  })

  it('joins two names with "and", and says "they" after', () => {
    expect(noLibrarySentence(['alice', 'bob'], 'this library')).toBe(
      'alice and bob can see only this library. After this, they will see no library until an administrator grants one.',
    )
  })

  it('joins three names with commas and a last "and", and names the directory’s library', () => {
    expect(
      noLibrarySentence(['alice', 'bob', 'carol'], "this directory's library"),
    ).toBe(
      "alice, bob and carol can see only this directory's library. After this, they will see no library until an administrator grants one.",
    )
  })
})

/**
 * The lines other directories draw for `/m/k`, which Kids and Teens hold:
 * Manga's `/m` around it names it on a nested line, and Comics' `/m/k/s`
 * inside it on an inside line, each with both holders. Kids holds nothing
 * else, so a write taking `/m/k` out of it empties it.
 */
const NESTING = (() => {
  const COMICS = { id: 3, name: 'Comics' }
  const MANGA = { id: 7, name: 'Manga' }
  const KIDS = { id: 8, name: 'Kids' }
  const TEENS = { id: 9, name: 'Teens' }
  const nestedLine = (
    id: number,
    path: string,
    libraries: string[],
  ): NestedDir => ({ id, path, libraries, shown: true, kept_out_by: null })
  const aroundLine = (path: string, libraries: string[]): AroundDir => ({
    path,
    libraries,
    shows: true,
    kept_out_by: null,
  })
  const k = (): LibraryDir => ({
    ...dir(2, '/m/k', [KIDS, TEENS]),
    nested: [nestedLine(4, '/m/k/s', ['Comics'])],
    inside: [aroundLine('/m', ['Manga'])],
  })
  const held = (): Library[] => [
    {
      ...COMICS,
      works: 0,
      books: 0,
      dirs: [
        {
          ...dir(4, '/m/k/s', [COMICS]),
          inside: [
            aroundLine('/m', ['Manga']),
            aroundLine('/m/k', ['Kids', 'Teens']),
          ],
        },
      ],
    },
    {
      ...MANGA,
      works: 0,
      books: 0,
      dirs: [
        {
          ...dir(1, '/m', [MANGA]),
          nested: [
            nestedLine(2, '/m/k', ['Kids', 'Teens']),
            nestedLine(4, '/m/k/s', ['Comics']),
          ],
        },
      ],
    },
    { ...KIDS, works: 0, books: 0, dirs: [k()] },
    { ...TEENS, works: 0, books: 0, dirs: [dir(3, '/t', [TEENS]), k()] },
  ]
  /**
   * Each library's directories, each with its holders and its nested and
   * inside lines, each line with the names it carries, in order.
   */
  const lines = (libraries: Library[]) =>
    libraries.map((one) => [
      one.name,
      one.dirs.map((d) => [
        d.path,
        d.libraries.map((l) => l.name),
        d.nested.map((l) => [l.path, l.libraries]),
        d.inside.map((l) => [l.path, l.libraries]),
      ]),
    ])
  return { held, lines }
})()

describe('withoutDir', () => {
  const MANGA = { id: 7, name: 'Manga' }
  const KIDS = { id: 8, name: 'Kids' }
  const TEENS = { id: 9, name: 'Teens' }
  /**
   * `/m` held by Manga, Kids and Teens, Manga and Teens holding another
   * directory each, and Kids nothing else, so dropping `/m` from Kids can
   * empty it.
   */
  const held = () => [
    {
      ...MANGA,
      works: 0,
      books: 0,
      dirs: [dir(1, '/m', [MANGA, KIDS, TEENS]), dir(2, '/a', [MANGA])],
    },
    { ...KIDS, works: 0, books: 0, dirs: [dir(1, '/m', [MANGA, KIDS, TEENS])] },
    {
      ...TEENS,
      works: 0,
      books: 0,
      dirs: [dir(3, '/t', [TEENS]), dir(1, '/m', [MANGA, KIDS, TEENS])],
    },
  ]
  const paths = (libraries: ReturnType<typeof held>) =>
    libraries.map((one) => [one.name, one.dirs.map((d) => d.path)])

  it('drops an unlinked directory from its library alone', () => {
    expect(paths(withoutDir(held(), 7, '/m', 'unlinked', []))).toEqual([
      ['Manga', ['/a']],
      ['Kids', ['/m']],
      ['Teens', ['/t', '/m']],
    ])
  })

  it('drops a removed directory from every library holding it, and each library the answer says it emptied', () => {
    expect(paths(withoutDir(held(), 7, '/m', 'removed', ['Kids']))).toEqual([
      ['Manga', ['/a']],
      ['Teens', ['/t']],
    ])
  })

  it('drops the library an unlink emptied, and no other', () => {
    expect(paths(withoutDir(held(), 8, '/m', 'unlinked', ['Kids']))).toEqual([
      ['Manga', ['/m', '/a']],
      ['Teens', ['/t', '/m']],
    ])
  })

  /** Each library's directories, each with the libraries it says hold it. */
  const holders = (libraries: ReturnType<typeof held>) =>
    libraries.map((one) => [
      one.name,
      one.dirs.map((d) => [d.path, d.libraries.map((l) => l.name)]),
    ])

  it('takes the library an unlink left out of the holders every other copy of the directory names, and touches no other directory', () => {
    expect(holders(withoutDir(held(), 7, '/m', 'unlinked', []))).toEqual([
      ['Manga', [['/a', ['Manga']]]],
      ['Kids', [['/m', ['Kids', 'Teens']]]],
      [
        'Teens',
        [
          ['/t', ['Teens']],
          ['/m', ['Kids', 'Teens']],
        ],
      ],
    ])
  })

  it('takes an unlink that empties its library out of the holders every other copy names', () => {
    expect(holders(withoutDir(held(), 8, '/m', 'unlinked', ['Kids']))).toEqual([
      [
        'Manga',
        [
          ['/m', ['Manga', 'Teens']],
          ['/a', ['Manga']],
        ],
      ],
      [
        'Teens',
        [
          ['/t', ['Teens']],
          ['/m', ['Manga', 'Teens']],
        ],
      ],
    ])
  })

  it('takes the library an unlink left, which it emptied, off the lines other directories draw for the directory, on both sides', () => {
    const { held, lines } = NESTING
    expect(lines(withoutDir(held(), 8, '/m/k', 'unlinked', ['Kids']))).toEqual([
      [
        'Comics',
        [
          [
            '/m/k/s',
            ['Comics'],
            [],
            [
              ['/m', ['Manga']],
              ['/m/k', ['Teens']],
            ],
          ],
        ],
      ],
      [
        'Manga',
        [
          [
            '/m',
            ['Manga'],
            [
              ['/m/k', ['Teens']],
              ['/m/k/s', ['Comics']],
            ],
            [],
          ],
        ],
      ],
      [
        'Teens',
        [
          ['/t', ['Teens'], [], []],
          ['/m/k', ['Teens'], [['/m/k/s', ['Comics']]], [['/m', ['Manga']]]],
        ],
      ],
    ])
  })

  it('takes the library an unlink left, which it did not empty, off the lines other directories draw for the directory, and keeps the library still holding it there', () => {
    // Teens keeps `/t`, so this unlink empties no library: the names taken
    // off the lines are the library left, not the libraries emptied.
    const { held, lines } = NESTING
    expect(lines(withoutDir(held(), 9, '/m/k', 'unlinked', []))).toEqual([
      [
        'Comics',
        [
          [
            '/m/k/s',
            ['Comics'],
            [],
            [
              ['/m', ['Manga']],
              ['/m/k', ['Kids']],
            ],
          ],
        ],
      ],
      [
        'Manga',
        [
          [
            '/m',
            ['Manga'],
            [
              ['/m/k', ['Kids']],
              ['/m/k/s', ['Comics']],
            ],
            [],
          ],
        ],
      ],
      [
        'Kids',
        [['/m/k', ['Kids'], [['/m/k/s', ['Comics']]], [['/m', ['Manga']]]]],
      ],
      ['Teens', [['/t', ['Teens'], [], []]]],
    ])
  })

  it('takes the line of a removed directory away from every other directory, on both sides', () => {
    const { held, lines } = NESTING
    expect(lines(withoutDir(held(), 8, '/m/k', 'removed', ['Kids']))).toEqual([
      ['Comics', [['/m/k/s', ['Comics'], [], [['/m', ['Manga']]]]]],
      ['Manga', [['/m', ['Manga'], [['/m/k/s', ['Comics']]], []]]],
      ['Teens', [['/t', ['Teens'], [], []]]],
    ])
  })
})

describe('withMovedDir', () => {
  const MANGA = { id: 7, name: 'Manga' }
  const KIDS = { id: 8, name: 'Kids' }
  /** Below Kids by id, so the server lists it first among `/m`'s holders. */
  const COMICS = { id: 3, name: 'Comics' }
  /** `/m` held by Manga and Kids, Manga holding `/a` too; Comics holding `/c`. */
  const held = () => [
    { ...COMICS, works: 0, books: 0, dirs: [dir(4, '/c', [COMICS])] },
    {
      ...MANGA,
      works: 0,
      books: 0,
      dirs: [dir(1, '/m', [MANGA, KIDS]), dir(2, '/a', [MANGA])],
    },
    { ...KIDS, works: 0, books: 0, dirs: [dir(1, '/m', [MANGA, KIDS])] },
  ]
  const moved = (over: Partial<Moved> = {}): Moved => ({
    library_id: 3,
    library: 'Comics',
    gains_sight: [],
    loses_sight: [],
    library_removed: null,
    grants_removed: 0,
    left_with_no_library: [],
    ...over,
  })
  /** Each library's directories, each with the libraries it says hold it. */
  const holders = (libraries: ReturnType<typeof held>) =>
    libraries.map((one) => [
      one.name,
      one.dirs.map((d) => [d.path, d.libraries.map((l) => l.name)]),
    ])

  it('names the library a move went to in place of the one it left in every copy that stays, by id, and draws no row in that library, which the read that follows draws', () => {
    expect(holders(withMovedDir(held(), 7, '/m', moved()))).toEqual([
      ['Comics', [['/c', ['Comics']]]],
      ['Manga', [['/a', ['Manga']]]],
      ['Kids', [['/m', ['Comics', 'Kids']]]],
    ])
  })

  it('takes away the library a move emptied', () => {
    expect(
      withMovedDir(held(), 7, '/m', moved({ library_removed: 'Manga' })).map(
        (one) => one.name,
      ),
    ).toEqual(['Comics', 'Kids'])
  })

  it('names a library the move made among the holders, and draws the directory in no library the list does not hold', () => {
    expect(
      holders(
        withMovedDir(
          held(),
          7,
          '/m',
          moved({ library_id: 12, library: 'Extras' }),
        ),
      ),
    ).toEqual([
      ['Comics', [['/c', ['Comics']]]],
      ['Manga', [['/a', ['Manga']]]],
      ['Kids', [['/m', ['Kids', 'Extras']]]],
    ])
  })

  it('names the library a move went to in place of the one it left on the lines other directories draw for the directory, on both sides', () => {
    const { held, lines } = NESTING
    expect(
      lines(
        withMovedDir(held(), 8, '/m/k', moved({ library_removed: 'Kids' })),
      ),
    ).toEqual([
      [
        'Comics',
        [
          [
            '/m/k/s',
            ['Comics'],
            [],
            [
              ['/m', ['Manga']],
              ['/m/k', ['Comics', 'Teens']],
            ],
          ],
        ],
      ],
      [
        'Manga',
        [
          [
            '/m',
            ['Manga'],
            [
              ['/m/k', ['Comics', 'Teens']],
              ['/m/k/s', ['Comics']],
            ],
            [],
          ],
        ],
      ],
      [
        'Teens',
        [
          ['/t', ['Teens'], [], []],
          [
            '/m/k',
            ['Comics', 'Teens'],
            [['/m/k/s', ['Comics']]],
            [['/m', ['Manga']]],
          ],
        ],
      ],
    ])
  })

  it('names a library the move made last on the lines other directories draw for the directory, as its id is the largest', () => {
    const { held, lines } = NESTING
    expect(
      lines(
        withMovedDir(
          held(),
          8,
          '/m/k',
          moved({ library_id: 12, library: 'Extras', library_removed: 'Kids' }),
        ),
      ),
    ).toEqual([
      [
        'Comics',
        [
          [
            '/m/k/s',
            ['Comics'],
            [],
            [
              ['/m', ['Manga']],
              ['/m/k', ['Teens', 'Extras']],
            ],
          ],
        ],
      ],
      [
        'Manga',
        [
          [
            '/m',
            ['Manga'],
            [
              ['/m/k', ['Teens', 'Extras']],
              ['/m/k/s', ['Comics']],
            ],
            [],
          ],
        ],
      ],
      [
        'Teens',
        [
          ['/t', ['Teens'], [], []],
          [
            '/m/k',
            ['Teens', 'Extras'],
            [['/m/k/s', ['Comics']]],
            [['/m', ['Manga']]],
          ],
        ],
      ],
    ])
  })

  it('keeps a library a move made, which the list does not hold, on the lines when a second move of the directory follows', () => {
    const { held, lines } = NESTING
    const first = withMovedDir(
      held(),
      8,
      '/m/k',
      moved({ library_id: 12, library: 'Extras', library_removed: 'Kids' }),
    )
    expect(lines(withMovedDir(first, 9, '/m/k', moved()))).toEqual([
      [
        'Comics',
        [
          [
            '/m/k/s',
            ['Comics'],
            [],
            [
              ['/m', ['Manga']],
              ['/m/k', ['Comics', 'Extras']],
            ],
          ],
        ],
      ],
      [
        'Manga',
        [
          [
            '/m',
            ['Manga'],
            [
              ['/m/k', ['Comics', 'Extras']],
              ['/m/k/s', ['Comics']],
            ],
            [],
          ],
        ],
      ],
      ['Teens', [['/t', ['Teens'], [], []]]],
    ])
  })

  it('changes nothing for a move answered after a read that already draws it, where the library it left holds the directory no longer', () => {
    const { held } = NESTING
    const answer = moved({ library_removed: 'Kids' })
    const after = withMovedDir(held(), 8, '/m/k', answer)
    const teens = after.find((one) => one.name === 'Teens')
    const k = teens?.dirs.find((d) => d.path === '/m/k')
    if (k === undefined) throw new Error('the fixture holds /m/k in Teens')
    const read = after.map((one) =>
      one.name === 'Comics' ? { ...one, dirs: [...one.dirs, k] } : one,
    )
    expect(withMovedDir(read, 8, '/m/k', answer)).toEqual(read)
  })
})

describe('withLibraryMade', () => {
  it('adds the library a make linked a directory to, last, to the lines other directories draw for that directory, on both sides', () => {
    const { held, lines } = NESTING
    const extras = { id: 12, name: 'Extras' }
    const made: Library = {
      ...extras,
      works: 0,
      books: 0,
      dirs: [
        dir(2, '/m/k', [
          { id: 8, name: 'Kids' },
          { id: 9, name: 'Teens' },
          extras,
        ]),
      ],
    }
    expect(lines(withLibraryMade(held(), made))).toEqual([
      [
        'Comics',
        [
          [
            '/m/k/s',
            ['Comics'],
            [],
            [
              ['/m', ['Manga']],
              ['/m/k', ['Kids', 'Teens', 'Extras']],
            ],
          ],
        ],
      ],
      [
        'Manga',
        [
          [
            '/m',
            ['Manga'],
            [
              ['/m/k', ['Kids', 'Teens', 'Extras']],
              ['/m/k/s', ['Comics']],
            ],
            [],
          ],
        ],
      ],
      [
        'Kids',
        [
          [
            '/m/k',
            ['Kids', 'Teens', 'Extras'],
            [['/m/k/s', ['Comics']]],
            [['/m', ['Manga']]],
          ],
        ],
      ],
      [
        'Teens',
        [
          ['/t', ['Teens'], [], []],
          [
            '/m/k',
            ['Kids', 'Teens', 'Extras'],
            [['/m/k/s', ['Comics']]],
            [['/m', ['Manga']]],
          ],
        ],
      ],
      ['Extras', [['/m/k', ['Kids', 'Teens', 'Extras'], [], []]]],
    ])
  })
})

describe('libraryRemovedSentence', () => {
  it.each([
    [false, 0, 'Manga has no other directory, so it is removed.'],
    [
      false,
      2,
      'Manga has no other directory, so it is removed, with 2 grants.',
    ],
    [true, 1, 'Manga had no other directory, so it was removed, with 1 grant.'],
  ] as const)(
    'says the library a write empties, done %s, with %i grants',
    (past, grants, said) => {
      expect(
        libraryRemovedSentence(
          { library_removed: 'Manga', grants_removed: grants },
          past,
        ),
      ).toBe(said)
    },
  )

  it('says nothing where no library is emptied', () => {
    expect(
      libraryRemovedSentence(
        { library_removed: null, grants_removed: 0 },
        true,
      ),
    ).toBeUndefined()
  })
})
