import { act } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as backupApi from '../src/backup'
import { Config } from '../src/Config'
import { invalidateLibraryViews } from '../src/pageReads'
import { SplitEditions } from '../src/SplitEditions'
import * as splitApi from '../src/splitEditions'
import { deferred, flush, land } from './support/async'
import { press } from './support/input'
import { mount } from './support/mount'

vi.mock('../src/splitEditions', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/splitEditions')>()),
  listSplitEditions: vi.fn(),
}))
vi.mock('../src/backup', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/backup')>()),
  listBackups: vi.fn(),
}))

/**
 * Two editions as the route sends them, by title. Foo's Kids corner holds
 * its middle file and Manga the other two, so a line that swapped a count
 * for its total, or a link that took the other library's name, reads
 * differently. Bar's Shared holds both its files, Bar 01 under a directory
 * of each library.
 */
const TWO: splitApi.SplitEdition[] = [
  {
    work: 'bar',
    title: 'Bar',
    edition: 'en',
    pages: 10,
    members: [
      {
        path: 'Bar 01.cbz',
        pages: 4,
        directories: [
          { root: 3, path: '/m', libraries: [1] },
          { root: 7, path: '/s', libraries: [3] },
        ],
      },
      {
        path: 'Bar 02.cbz',
        pages: 6,
        directories: [{ root: 7, path: '/s', libraries: [3] }],
      },
    ],
    libraries: [
      { id: 1, name: 'Manga', members: 1, pages: 4 },
      { id: 3, name: 'Shared', members: 2, pages: 10 },
    ],
  },
  {
    work: 'foo',
    title: 'Foo',
    edition: 'raw',
    pages: 9,
    members: [
      {
        path: 'Foo v01 part1.cbz',
        pages: 1,
        directories: [{ root: 3, path: '/m', libraries: [1] }],
      },
      {
        path: 'Foo v01 part2.cbz',
        pages: 3,
        directories: [{ root: 5, path: '/k', libraries: [2] }],
      },
      {
        path: 'Foo v01 part3.cbz',
        pages: 5,
        directories: [{ root: 3, path: '/m', libraries: [1] }],
      },
    ],
    libraries: [
      { id: 2, name: 'Kids corner', members: 1, pages: 3 },
      { id: 1, name: 'Manga', members: 2, pages: 6 },
    ],
  },
]

async function settle() {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <MemoryRouter>
        <SplitEditions />
      </MemoryRouter>,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  await flush()
  return host
}

/** The editions listed, one `li` each. */
const listed = (host: HTMLElement) => [
  ...host.querySelectorAll(
    '.split-editions > .settings-section > .settings-box > .settings-group > li',
  ),
]

/** One edition's row as a person reads it, part by part. */
const read = (li: Element) => ({
  title: li.querySelector(':scope > .split-edition-title')?.textContent,
  sees: [...li.querySelectorAll(':scope > .split-edition-sees > li')].map(
    (line) => line.textContent,
  ),
  files: [...li.querySelectorAll(':scope > .split-edition-files > li')].map(
    (file) => ({
      file: file.querySelector(':scope > .split-edition-file')?.textContent,
      dirs: [
        ...file.querySelectorAll(':scope > .split-edition-dirs > li > a'),
      ].map((a) => [a.textContent, a.getAttribute('href')]),
    }),
  ),
})

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(splitApi.listSplitEditions).mockResolvedValue({ editions: [] })
  vi.mocked(backupApi.listBackups).mockResolvedValue({ backups: [] })
})

describe('SplitEditions', () => {
  it('heads itself Split editions, with what a split is as its line', async () => {
    const host = await settle()
    const header = host.querySelector(
      '.split-editions > .page-header',
    ) as HTMLElement
    expect(header.querySelector('.page-title')?.tagName).toBe('H2')
    expect(header.querySelector('.page-title')?.textContent).toBe(
      'Split editions',
    )
    expect(header.querySelector('.page-description')?.textContent).toBe(
      'An edition is split when a library shows some of its files and not all of them. An account granted only some of those libraries sees only their files, with the pages renumbered. An account granted every library listed sees every file a directory holds, and an account that sees every library, as an administrator does, sees the whole edition.',
    )
  })

  it('lists each edition with a line per library, in the order sent, and says every file where a library holds them all', async () => {
    vi.mocked(splitApi.listSplitEditions).mockResolvedValue({ editions: TWO })
    const host = await settle()
    const rows = listed(host).map(read)
    expect(rows.map(({ title, sees }) => ({ title, sees }))).toEqual([
      {
        title: 'Bar\u00a0·\u00a0en',
        sees: [
          'Manga sees 1 of 2 files, 4 of 10 pages',
          'Shared sees every file',
        ],
      },
      {
        title: 'Foo\u00a0·\u00a0raw',
        sees: [
          'Kids corner sees 1 of 3 files, 3 of 9 pages',
          'Manga sees 2 of 3 files, 6 of 9 pages',
        ],
      },
    ])
  })

  it('lists each file with its pages, and links each directory holding it to that directory’s page in its own library', async () => {
    vi.mocked(splitApi.listSplitEditions).mockResolvedValue({ editions: TWO })
    const host = await settle()
    expect(listed(host).map((li) => read(li).files)).toEqual([
      [
        {
          file: 'Bar 01.cbz\u00a0·\u00a04 pages',
          dirs: [
            ['/m', '/config/libraries/Manga/dirs/3'],
            ['/s', '/config/libraries/Shared/dirs/7'],
          ],
        },
        {
          file: 'Bar 02.cbz\u00a0·\u00a06 pages',
          dirs: [['/s', '/config/libraries/Shared/dirs/7']],
        },
      ],
      [
        {
          file: 'Foo v01 part1.cbz\u00a0·\u00a01 page',
          dirs: [['/m', '/config/libraries/Manga/dirs/3']],
        },
        {
          file: 'Foo v01 part2.cbz\u00a0·\u00a03 pages',
          dirs: [['/k', '/config/libraries/Kids%20corner/dirs/5']],
        },
        {
          file: 'Foo v01 part3.cbz\u00a0·\u00a05 pages',
          dirs: [['/m', '/config/libraries/Manga/dirs/3']],
        },
      ],
    ])
  })

  it('draws a directory whose library the edition does not list as its path alone, with no link', async () => {
    const [bar] = TWO
    vi.mocked(splitApi.listSplitEditions).mockResolvedValue({
      editions: [{ ...bar, libraries: bar.libraries.slice(0, 1) }],
    })
    const host = await settle()
    const dirs = [
      ...listed(host)[0].querySelectorAll(
        ':scope > .split-edition-files > li > .split-edition-dirs > li',
      ),
    ].map((li) => [li.textContent, li.querySelector('a')?.getAttribute('href')])
    expect(dirs).toEqual([
      ['/m', '/config/libraries/Manga/dirs/3'],
      ['/s', undefined],
      ['/s', undefined],
    ])
  })

  it('links a directory several libraries hold once, under the first library the edition lists that holds it', async () => {
    const [bar] = TWO
    const held = (libraries: number[]) => ({
      ...bar,
      members: [
        {
          path: 'Bar 01.cbz',
          pages: 4,
          directories: [{ root: 3, path: '/m', libraries }],
        },
      ],
    })
    const link = async (edition: splitApi.SplitEdition) => {
      vi.mocked(splitApi.listSplitEditions).mockResolvedValue({
        editions: [edition],
      })
      const host = await settle()
      return [
        ...listed(host)[0].querySelectorAll(
          ':scope > .split-edition-files > li > .split-edition-dirs > li',
        ),
      ].map((li) => [
        li.textContent,
        [...li.querySelectorAll('a')].map((a) => a.getAttribute('href')),
      ])
    }
    // Manga (1) and Shared (3) both listed: Manga's page.
    expect(await link(held([1, 3]))).toEqual([
      ['/m', ['/config/libraries/Manga/dirs/3']],
    ])
    // Kids corner (2) is not the edition's: Shared's page.
    expect(await link(held([2, 3]))).toEqual([
      ['/m', ['/config/libraries/Shared/dirs/3']],
    ])
    // The server sends an edition's libraries by name, so Anime (4) comes
    // before Manga (1) there: the edition's order, not the ids', picks
    // Anime (spec decision 29).
    expect(
      await link({
        ...held([1, 4]),
        libraries: [
          { id: 4, name: 'Anime', members: 1, pages: 4 },
          ...bar.libraries,
        ],
      }),
    ).toEqual([['/m', ['/config/libraries/Anime/dirs/3']]])
    // Neither Kids corner (2) nor library 5 is the edition's: its path
    // alone, with no link.
    expect(await link(held([2, 5]))).toEqual([['/m', []]])
  })

  it('says how to split one under the list, and draws no empty line beside it', async () => {
    vi.mocked(splitApi.listSplitEditions).mockResolvedValue({ editions: TWO })
    const host = await settle()
    const root = host.querySelector('.split-editions') as HTMLElement
    expect(root.querySelector('.empty')).toBeNull()
    const box = root.querySelector(
      ':scope > .settings-section > .settings-box',
    ) as HTMLElement
    expect([...box.children].map((child) => child.className)).toEqual([
      'settings-group',
    ])
    const after = root.querySelector(':scope > .settings-section + p')
    expect(after?.className).toBe('split-edition-how')
    expect(after?.textContent).toBe(
      'To split one, give one of its directories its own edition key (in the directory’s settings, or with a rule), then scan that directory.',
    )
  })

  it('says no edition is split when there are none, and nothing else in the box or under it', async () => {
    const host = await settle()
    const root = host.querySelector('.split-editions') as HTMLElement
    const box = root.querySelector(
      ':scope > .settings-section > .settings-box',
    ) as HTMLElement
    expect([...box.children].map((child) => child.className)).toEqual(['empty'])
    expect(box.textContent).toBe('No edition is split between libraries.')
    expect(root.querySelector('.split-edition-how')).toBeNull()
  })

  it('draws a refusal as the server’s sentence, and no body', async () => {
    vi.mocked(splitApi.listSplitEditions).mockRejectedValue(
      new Error(
        '/api/library/split-editions: 403 only an administrator may do this',
      ),
    )
    const host = await settle()
    await flush()
    const root = host.querySelector('.split-editions') as HTMLElement
    expect(
      [...root.querySelectorAll(':scope > [role="alert"]')].map(
        (p) => p.textContent,
      ),
    ).toEqual([
      '/api/library/split-editions: 403 only an administrator may do this',
    ])
    expect(root.querySelector(':scope > .settings-section')).toBeNull()
    expect(root.querySelector('.split-edition-how')).toBeNull()
  })
})

describe('Configuration’s Split editions tab', () => {
  async function at(entry: string) {
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <MemoryRouter initialEntries={[entry]}>
          <Routes>
            <Route path="/config/*" element={<Config />} />
          </Routes>
        </MemoryRouter>,
      )
      await Promise.resolve()
      await Promise.resolve()
    })
    if (!host) throw new Error('nothing mounted')
    await flush()
    return host
  }

  const tab = (host: HTMLElement, label: string) => {
    const link = [...host.querySelectorAll('.config-nav a')].find(
      (a) => a.textContent === label,
    )
    if (link === undefined) throw new Error(`no tab ${label}`)
    return link
  }

  it('is the last tab, at /config/split-editions, and draws the screen there', async () => {
    const host = await at('/config/split-editions')
    const links = [...host.querySelectorAll('.config-nav a')]
    expect(
      [links.at(-1)].map((a) => [
        a?.textContent,
        a?.getAttribute('href'),
        a?.getAttribute('aria-current'),
      ]),
    ).toEqual([['Split editions', '/config/split-editions', 'page']])
    expect(
      host.querySelector('.config > .split-editions .page-title')?.textContent,
    ).toBe('Split editions')
  })

  it('asks the server again each time the tab is opened', async () => {
    vi.mocked(splitApi.listSplitEditions).mockResolvedValue({ editions: TWO })
    const host = await at('/config/split-editions')
    expect(splitApi.listSplitEditions).toHaveBeenCalledTimes(1)

    await press(tab(host, 'Backup'))
    expect(host.querySelector('.split-editions')).toBeNull()
    expect(host.querySelector('.config > .backup')).not.toBeNull()

    await press(tab(host, 'Split editions'))
    await flush()
    expect(host.querySelector('.split-editions')).not.toBeNull()
    expect(splitApi.listSplitEditions).toHaveBeenCalledTimes(2)
  })

  it('waits on a return after a scan for the new list, rather than drawing the one kept from before it', async () => {
    vi.mocked(splitApi.listSplitEditions).mockResolvedValue({ editions: TWO })
    const host = await at('/config/split-editions')
    expect(listed(host)).toHaveLength(2)
    await press(tab(host, 'Backup'))
    // What a scan's end does (`LibraryPage`, `RootRow`, `DirPage`).
    invalidateLibraryViews()
    const again = deferred<{ editions: splitApi.SplitEdition[] }>()
    vi.mocked(splitApi.listSplitEditions).mockReturnValue(again.promise)

    await press(tab(host, 'Split editions'))
    const root = host.querySelector('.config > .split-editions') as HTMLElement
    expect(root.querySelector(':scope > .settings-section')).toBeNull()
    expect(listed(host)).toHaveLength(0)

    again.resolve({ editions: TWO.slice(1) })
    await land()
    expect(listed(host).map((li) => read(li).title)).toEqual([
      'Foo\u00a0·\u00a0raw',
    ])
  })
})
