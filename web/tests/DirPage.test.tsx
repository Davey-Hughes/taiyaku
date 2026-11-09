import { act } from 'react'
import { MemoryRouter } from 'react-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from '../src/config'
import { DirPage } from '../src/DirPage'
import * as api from '../src/libraries'
import { deferred, flush } from './support/async'
import './support/dialog'
import { press } from './support/input'
import { holdOf } from './support/held'
import {
  counted,
  dir,
  idle,
  manga,
  scanningIn,
  tick,
} from './support/libraries'
import { libraryScan } from '../src/pageReads'
import { entryOf, readSet, subscribe } from '../src/reads'
import { mount, rerender } from './support/mount'
import { keptGrid } from './support/reads'
import { expectRowsWired, row, section } from './support/settingsRows'
import { pass, WAIT_DRAWN_AFTER_MS } from './support/wait'

vi.mock('../src/config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/config')>()),
  patchRoot: vi.fn(),
  removalCost: vi.fn(),
  removeRoot: vi.fn(),
  scanStatus: vi.fn(),
  startScan: vi.fn(),
}))
vi.mock('../src/libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/libraries')>()),
  libraryScanStatus: vi.fn(),
  moveCost: vi.fn(),
  moveDir: vi.fn(),
  unlinkCost: vi.fn(),
  unlinkDir: vi.fn(),
}))

/**
 * The page of the directory `at` at its own route, with each handler a mock. It
 * draws the second directory, so the first is the sibling a test scans.
 */
async function shown({
  library = manga(),
  at = 2,
}: {
  library?: api.Library
  at?: number
} = {}) {
  const root = library.dirs.find((one) => one.id === at)
  if (root === undefined) throw new Error(`no directory ${at}`)
  const onChanged = vi.fn()
  const onRemoved = vi.fn()
  const onUnlinked = vi.fn()
  const onMoved = vi.fn()
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <MemoryRouter
        initialEntries={[`/config/libraries/${library.name}/dirs/${root.id}`]}
      >
        <DirPage
          library={library}
          root={root}
          others={[{ id: 8, name: 'Comics' }]}
          onChanged={onChanged}
          onRemoved={onRemoved}
          onUnlinked={onUnlinked}
          onMoved={onMoved}
        />
      </MemoryRouter>,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return { host, onChanged, onRemoved, onUnlinked, onMoved }
}

const control = (host: HTMLElement, text: string) =>
  [...host.querySelectorAll('button')].find(
    (b) => b.textContent === text,
  ) as HTMLButtonElement

/** Scan directory, Move… and Remove…, each held, disabled or free. */
const actionsHeld = (host: HTMLElement) =>
  ['Scan directory', 'Move…', 'Remove…'].map((text) =>
    holdOf(control(host, text)),
  )

const statusOf = (id: number, running: boolean): config.ScanStatus => ({
  root_id: id,
  running,
  elapsed_secs: running ? 3 : undefined,
  report: null,
  last_scan_at: null,
  last_scan_status: null,
})

/** Each directory's own status: running for the ids named, idle for the rest. */
const ownRunning = (...ids: number[]) =>
  vi
    .mocked(config.scanStatus)
    .mockImplementation((id) => Promise.resolve(statusOf(id, ids.includes(id))))

/**
 * The directory's own Scanning… line, which `RootRow` draws from its status
 * under the page's header.
 */
const ownScanning = (host: HTMLElement) =>
  host.querySelector('.dir-page > .scanning')

/** Type into a box and leave it, as a person committing an edit does. */
async function editBox(box: Element | null, value: string) {
  const el = box as HTMLInputElement
  await act(async () => {
    el.dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
    Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      'value',
    )?.set?.call(el, value)
    el.dispatchEvent(new Event('input', { bubbles: true }))
    el.dispatchEvent(new FocusEvent('focusout', { bubbles: true }))
    await Promise.resolve()
    await Promise.resolve()
  })
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.useFakeTimers({ shouldAdvanceTime: true })
  vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
  vi.mocked(config.scanStatus).mockImplementation((id) =>
    Promise.resolve(statusOf(id, false)),
  )
  vi.mocked(config.patchRoot).mockResolvedValue(undefined)
})

afterEach(() => {
  vi.useRealTimers()
})

describe('DirPage', () => {
  it('words the directory’s button Scan directory', async () => {
    const { host } = await shown()
    expect(
      [...host.querySelectorAll('.dir-page button')]
        .map((b) => b.textContent)
        .filter((text) => text?.startsWith('Scan')),
    ).toEqual(['Scan directory'])
  })

  it('heads itself with ‹ the library, the path as its title, the facts as its line, and Scan directory and Move… as its actions', async () => {
    const { host } = await shown()
    const header = host.querySelector('.dir-page > .page-header') as HTMLElement
    const back = header.querySelector('.back a')
    expect([back?.textContent, back?.getAttribute('href')]).toEqual([
      '‹ Manga',
      '/config/libraries/Manga',
    ])
    const title = header.querySelector('.page-title')
    expect(title?.tagName).toBe('H2')
    expect(title?.querySelector('code')?.textContent).toBe(
      '/media/Manga/Chapters',
    )
    // The facts by name, each the header's line's own.
    expect(
      [...header.querySelectorAll('.page-description .regime > div')].map(
        (fact) => fact.querySelector('dt')?.textContent,
      ),
    ).toEqual(['watch', 'scan every', 'last scan'])
    // Remove… is the removal box's, not the header's.
    expect(
      [...header.querySelectorAll('.page-actions button')].map(
        (b) => b.textContent,
      ),
    ).toEqual(['Scan directory', 'Move…'])
  })

  it('draws How it is scanned, How it is read, then the removal box, whose Remove… opens its confirm inside it', async () => {
    vi.mocked(config.removalCost).mockResolvedValue(counted())
    const { host } = await shown()
    expect(
      [...host.querySelectorAll('.dir-page > .settings-section')].map(
        (one) => one.querySelector('.settings-section-title')?.textContent,
      ),
    ).toEqual(['How it is scanned', 'How it is read', 'Removal'])
    const removal = section(host, 'Removal')
    expect(removal.classList.contains('settings-removal')).toBe(true)
    const remove = row(removal, 'Remove this directory').querySelector(
      'button',
    ) as HTMLButtonElement
    expect(remove.textContent).toBe('Remove…')
    await press(remove)
    expect(
      removal.querySelector(':scope > .settings-box > .remove-root .counts'),
    ).not.toBeNull()
  })

  it('opens Move… in a dialog named for the directory, and draws nothing new between the header and the settings', async () => {
    const { host } = await shown()
    await press(control(host, 'Move…'))
    const page = host.querySelector('.dir-page') as HTMLElement
    const dialogs = [...page.querySelectorAll('dialog')]
    expect(dialogs).toHaveLength(1)
    expect(dialogs[0].open).toBe(true)
    expect(
      document.getElementById(dialogs[0].getAttribute('aria-labelledby') ?? '')
        ?.textContent,
    ).toBe('Move /media/Manga/Chapters')
    expect(
      dialogs[0].querySelector('select[name="move-target"]'),
    ).not.toBeNull()
    // The page's own parts in its own order, by what each is: the dialog is
    // drawn over them and takes no place among them.
    expect(
      [...page.children]
        .filter((part) => part.tagName !== 'DIALOG')
        .map((part) =>
          part.classList.contains('settings-section')
            ? part.querySelector('.settings-section-title')?.textContent
            : part.className,
        )
        .slice(0, 2),
    ).toEqual(['page-header', 'How it is scanned'])
  })

  it('ties each row’s label and help to its control', async () => {
    const { host } = await shown()
    // Every row the page draws, named, so the check is never over fewer.
    expect(
      [...host.querySelectorAll('.settings-row-label')].map(
        (label) => label.textContent,
      ),
    ).toEqual([
      'Scan interval',
      'Exclude globs',
      'Keeps documents under .taiyaku/',
      'Edition key',
      'Default kind',
      'Default language',
      'Remove this directory',
    ])
    expectRowsWired(host)
  })

  it('tells a directory it is the last one where its dry run says so, and no directory whose dry run empties nothing', async () => {
    vi.mocked(config.removalCost).mockResolvedValue({
      ...counted(0),
      removed: { ...counted(0).removed, libraries_removed: ['Manga'] },
    })
    const one = await shown({
      library: manga({ dirs: [dir(1, '/media/Manga/Volumes')] }),
      at: 1,
    })
    await press(control(one.host, 'Remove…'))
    expect(one.host.querySelector('.last-dir')?.textContent).toContain(
      'the only directory of the library Manga',
    )

    vi.mocked(config.removalCost).mockResolvedValue(counted(0))
    const two = await shown()
    await press(control(two.host, 'Remove…'))
    expect(two.host.querySelector('.remove-root .counts')).not.toBeNull()
    expect(two.host.querySelector('.last-dir')).toBeNull()
  })

  it('takes the libraries a removal empties from its dry run, not from the list drawn', async () => {
    // The list drawn says Manga keeps another directory, and says nothing of
    // whether Kids does: the dry run says the removal empties Kids alone.
    const both = [
      { id: 7, name: 'Manga' },
      { id: 9, name: 'Kids' },
    ]
    vi.mocked(config.removalCost).mockResolvedValue({
      ...counted(0),
      removed: { ...counted(0).removed, libraries_removed: ['Kids'] },
    })
    const shared = await shown({
      library: manga({
        dirs: [dir(1, '/media/Manga/Volumes'), dir(2, '/media/Shared', both)],
      }),
    })
    await press(control(shared.host, 'Remove…'))
    expect(shared.host.querySelector('.last-dir')?.textContent).toBe(
      'This is the only directory of the library Kids, so removing it removes the library too.',
    )

    // A library the list draws with one directory, which the dry run says
    // the removal does not empty: another directory was added since.
    vi.mocked(config.removalCost).mockResolvedValue(counted(0))
    const stale = await shown({
      library: manga({ dirs: [dir(1, '/media/Manga/Volumes')] }),
      at: 1,
    })
    await press(control(stale.host, 'Remove…'))
    expect(stale.host.querySelector('.remove-root .counts')).not.toBeNull()
    expect(stale.host.querySelector('.last-dir')).toBeNull()
  })

  it('hands a directory’s removal up, and not as a move', async () => {
    vi.mocked(config.removalCost).mockResolvedValue(counted())
    const real = { ...counted(5), dry_run: false }
    vi.mocked(config.removeRoot).mockResolvedValue(real)
    const { host, onRemoved, onMoved, onChanged } = await shown()
    await press(control(host, 'Remove…'))
    await press(host.querySelector('.remove-root .confirm') as Element)
    expect(config.removeRoot).toHaveBeenCalledWith(2)
    expect(onRemoved).toHaveBeenCalledWith(real)
    expect(onMoved).not.toHaveBeenCalled()
    expect(onChanged).toHaveBeenCalled()
  })

  it('hands a directory’s move up, and not as a removal', async () => {
    const moved: api.MoveAnswer = {
      moved: {
        library_id: 8,
        library: 'Comics',
        gains_sight: ['kim'],
        loses_sight: [],
        library_removed: null,
        grants_removed: 0,
        left_with_no_library: [],
      },
      dry_run: false,
    }
    vi.mocked(api.moveCost).mockResolvedValue({
      moved: { ...moved.moved, gains_sight: [] },
      dry_run: true,
    })
    vi.mocked(api.moveDir).mockResolvedValue(moved)
    const { host, onMoved, onRemoved } = await shown()
    await press(control(host, 'Move…'))
    await act(async () => {
      const box = host.querySelector(
        'select[name="move-target"]',
      ) as HTMLSelectElement
      Object.getOwnPropertyDescriptor(
        HTMLSelectElement.prototype,
        'value',
      )?.set?.call(box, '8')
      box.dispatchEvent(new Event('change', { bubbles: true }))
    })
    await press(host.querySelector('button[name="rehearse"]') as Element)
    await press(host.querySelector('.move-root .confirm') as Element)
    expect(api.moveDir).toHaveBeenCalledWith(2, 7, { library: 8 })
    expect(onMoved).toHaveBeenCalledWith(moved)
    expect(onRemoved).not.toHaveBeenCalled()
  })

  it('offers the directory the other libraries', async () => {
    const { host } = await shown()
    await press(control(host, 'Move…'))
    const target = host.querySelector(
      'select[name="move-target"]',
    ) as HTMLSelectElement
    expect([...target.options].map((o) => o.textContent)).toEqual([
      'Choose a library…',
      'Comics',
      'A new library…',
    ])
  })
})

/** What the page says of the directories around and inside it, in its header. */
const nesting = (host: HTMLElement) =>
  host.querySelector('.dir-page .page-header .page-description > .nesting')

/** Each line saying a directory this one sits in. */
const insideLines = (host: HTMLElement) =>
  [...(nesting(host)?.querySelectorAll(':scope > p') ?? [])].map(
    (p) => p.textContent,
  )

/** The fold of the directories inside this one. */
const fold = (host: HTMLElement) =>
  nesting(host)?.querySelector(':scope > details') as HTMLDetailsElement | null

/** Each row of the fold, cell by cell. */
const foldRows = (host: HTMLElement) =>
  [...(fold(host)?.querySelectorAll('ul > li') ?? [])].map((li) =>
    [...li.children].map((cell) => cell.textContent),
  )

describe('DirPage, one directory in two libraries', () => {
  it('draws the directory afresh under the other library: a removal open under the first is gone', async () => {
    vi.mocked(config.removalCost).mockResolvedValue(counted())
    const both = [
      { id: 7, name: 'Manga' },
      { id: 9, name: 'Kids' },
    ]
    const library = manga({
      dirs: [dir(1, '/media/Manga/Volumes'), dir(2, '/media/Shared', both)],
    })
    const { host } = await shown({ library })
    await press(
      row(section(host, 'Removal'), 'Remove this directory').querySelector(
        'button',
      ) as HTMLButtonElement,
    )
    expect(host.querySelector('.remove-root')).not.toBeNull()

    const kids: api.Library = {
      id: 9,
      name: 'Kids',
      works: 0,
      books: 0,
      dirs: [dir(2, '/media/Shared', both)],
    }
    await act(async () => {
      rerender(
        <MemoryRouter initialEntries={['/config/libraries/Kids/dirs/2']}>
          <DirPage
            library={kids}
            root={kids.dirs[0]}
            others={[]}
            onChanged={vi.fn()}
            onRemoved={vi.fn()}
            onUnlinked={vi.fn()}
            onMoved={vi.fn()}
          />
        </MemoryRouter>,
      )
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.dir-page .back a')?.textContent).toBe('‹ Kids')
    expect(host.querySelector('.remove-root')).toBeNull()
  })
})

describe('DirPage, a directory another library holds too', () => {
  const MANGA = { id: 7, name: 'Manga' }
  const KIDS = { id: 9, name: 'Kids' }
  const TEENS = { id: 11, name: 'Teens' }
  /** Manga, its second directory held by `holders` too. */
  const sharing = (...holders: { id: number; name: string }[]) =>
    manga({
      dirs: [
        dir(1, '/media/Manga/Volumes'),
        dir(2, '/media/Shared', [MANGA, ...holders]),
      ],
    })
  const unlinked = (dry_run: boolean): api.UnlinkAnswer => ({
    unlinked: {
      loses_sight: ['ana'],
      library_removed: null,
      grants_removed: 0,
      left_with_no_library: [],
    },
    dry_run,
  })
  /** The Removal box's rows: each label, help and button. */
  const removalRows = (host: HTMLElement) =>
    [
      ...section(host, 'Removal').querySelectorAll(
        ':scope > .settings-box > .settings-group > .settings-row',
      ),
    ].map((one) => [
      one.querySelector('.settings-row-label')?.textContent,
      one.querySelector('.settings-row-help')?.textContent,
      one.querySelector('button')?.textContent,
    ])
  const alsoIn = (host: HTMLElement) =>
    host.querySelector('.dir-page .page-header .page-description > .also-in')
  /** Scan directory, Move…, Remove from Manga… and Remove…. */
  const fourHeld = (host: HTMLElement) =>
    ['Scan directory', 'Move…', 'Remove from Manga…', 'Remove…'].map((text) =>
      holdOf(control(host, text)),
    )

  beforeEach(() => {
    vi.mocked(api.unlinkCost).mockResolvedValue(unlinked(true))
    vi.mocked(api.unlinkDir).mockResolvedValue(unlinked(false))
  })

  it('is as it was for a directory in one library: no line naming another, and one Removal row', async () => {
    const { host } = await shown()
    expect(alsoIn(host)).toBeNull()
    expect(removalRows(host)).toEqual([
      [
        'Remove this directory',
        'Asks first, with what it would take.',
        'Remove…',
      ],
    ])
  })

  it('says under its facts which other library holds it, linking to that library’s page of it', async () => {
    const { host } = await shown({ library: sharing(KIDS) })
    const line = alsoIn(host) as HTMLElement
    expect(line.textContent).toBe(
      'Also in Kids, which shares these settings and scans.',
    )
    expect(
      [...line.querySelectorAll('a')].map((a) => [
        a.textContent,
        a.getAttribute('href'),
      ]),
    ).toEqual([['Kids', '/config/libraries/Kids/dirs/2']])
    expect(line.previousElementSibling?.className).toBe('regime')
  })

  it('names two other libraries in one line, each a link, with the verb in the plural', async () => {
    const { host } = await shown({ library: sharing(KIDS, TEENS) })
    const line = alsoIn(host) as HTMLElement
    expect(line.textContent).toBe(
      'Also in Kids and Teens, which share these settings and scans.',
    )
    expect([...line.querySelectorAll('a')].map((a) => a.textContent)).toEqual([
      'Kids',
      'Teens',
    ])
    expect(removalRows(host)[0]?.[1]).toBe(
      'Kids and Teens keep it; nothing is deleted.',
    )
  })

  it('offers Remove from Manga beside Remove this directory, each in its own row with its own words', async () => {
    const { host } = await shown({ library: sharing(KIDS) })
    expect(removalRows(host)).toEqual([
      [
        'Remove from Manga',
        'Kids keeps it; nothing is deleted.',
        'Remove from Manga…',
      ],
      [
        'Remove this directory',
        'From every library that holds it, with what it would take.',
        'Remove…',
      ],
    ])
    expectRowsWired(host)
  })

  it('names every library holding it in Remove this directory’s confirm, and every library its dry run says it empties', async () => {
    vi.mocked(config.removalCost).mockResolvedValue({
      ...counted(5),
      removed: { ...counted(5).removed, libraries_removed: ['Kids', 'Teens'] },
    })
    const { host } = await shown({ library: sharing(KIDS, TEENS) })
    await press(control(host, 'Remove…'))
    expect(host.querySelector('.remove-root .counts')?.textContent).toBe(
      'This removes it from Manga, Kids and Teens, and 5 works, 2 in-progress positions and 0 notes.',
    )
    expect(host.querySelector('.remove-root .last-dir')?.textContent).toBe(
      'This is the only directory of the libraries Kids and Teens, so removing it removes them too.',
    )
  })

  it('holds Remove from Manga… exactly when Move… is, and Remove… on a held removal alone', async () => {
    const library = sharing(KIDS)
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(library, 1))
    const sibling = await shown({ library })
    expect(fourHeld(sibling.host)).toEqual(['free', 'held', 'held', 'free'])

    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      held_removals: [2],
    })
    const around = await shown({ library })
    expect(fourHeld(around.host)).toEqual(['free', 'free', 'free', 'held'])

    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      moves_held: true,
    })
    const locked = await shown({ library })
    expect(fourHeld(locked.host)).toEqual(['free', 'held', 'held', 'free'])
  })

  it('hands the unlink up as one, and neither as a removal nor as a move', async () => {
    const { host, onUnlinked, onRemoved, onMoved, onChanged } = await shown({
      library: sharing(KIDS),
    })
    await press(control(host, 'Remove from Manga…'))
    expect(api.unlinkCost).toHaveBeenCalledWith(7, 2)
    expect(host.querySelector('.unlink-root .counts')?.textContent).toBe(
      'Removing this directory from Manga: ana will lose sight of it.',
    )
    await press(host.querySelector('.unlink-root .confirm') as Element)
    expect(api.unlinkDir).toHaveBeenCalledWith(7, 2)
    expect(onUnlinked).toHaveBeenCalledWith(unlinked(false))
    expect(onRemoved).not.toHaveBeenCalled()
    expect(onMoved).not.toHaveBeenCalled()
    expect(onChanged).toHaveBeenCalled()
  })

  it('asks its library’s status again on a refusal for a hold it had not read, draws the hold, and takes the refusal away as the hold ends', async () => {
    const library = sharing(KIDS)
    vi.mocked(api.unlinkCost).mockRejectedValue(
      new Error(
        `/api/library/libraries/7/dirs/2?dry_run=true: 409 ${JSON.stringify({ error: 'a scan or another library job is running; remove the directory from this library once it has finished' })}`,
      ),
    )
    // The first read finds nothing holding, the second the lock held, and
    // the poll after it nothing again. By count, so that no answer queued
    // for a read that never comes is left for the next test.
    let reads = 0
    vi.mocked(api.libraryScanStatus).mockImplementation(() => {
      reads += 1
      return Promise.resolve(
        reads === 2 ? { ...idle(7), moves_held: true } : idle(7),
      )
    })
    const { host } = await shown({ library })
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(1)
    await press(control(host, 'Remove from Manga…'))
    await flush()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(2)
    expect(holdOf(control(host, 'Remove from Manga…'))).toBe('held')
    expect(host.querySelector('.callout [role="alert"]')?.textContent).toBe(
      'a scan or another library job is running; remove the directory from this library once it has finished',
    )
    await tick()
    expect(holdOf(control(host, 'Remove from Manga…'))).toBe('free')
    expect(host.querySelector('.callout')).toBeNull()
  })

  it('draws the refusal on the directory’s last library over Remove from Manga…, naming Remove this directory', async () => {
    const LAST =
      '`/media/Shared` is in no other library, so it can only be removed: use Remove this directory.'
    vi.mocked(api.unlinkCost).mockRejectedValue(
      new Error(
        `/api/library/libraries/7/dirs/2?dry_run=true: 400 ${JSON.stringify({ error: LAST })}`,
      ),
    )
    const { host } = await shown({ library: sharing(KIDS) })
    await press(control(host, 'Remove from Manga…'))
    expect(host.querySelector('.unlink-root')).toBeNull()
    const callout = row(section(host, 'Removal'), 'Remove this directory')
      .nextElementSibling as HTMLElement
    expect(callout.classList.contains('callout')).toBe(true)
    expect(callout.querySelector('[role="alert"]')?.textContent).toBe(LAST)
    expect(holdOf(control(host, 'Remove…'))).toBe('free')
  })
})

describe('DirPage, the directories around and inside it', () => {
  it('names every library holding a directory around or inside it, and says "their" of two', async () => {
    const library = manga({ name: 'Teens' })
    library.dirs[1].inside = [
      {
        path: '/media',
        libraries: ['Manga', 'Kids'],
        shows: true,
        kept_out_by: null,
      },
      {
        path: '/media/Manga',
        libraries: ['Manga', 'Kids'],
        shows: false,
        kept_out_by: 'exclusion',
      },
    ]
    library.dirs[1].nested = [
      {
        id: 5,
        path: '/media/Manga/Chapters/kids',
        libraries: ['Kids', 'Archive'],
        shown: true,
        kept_out_by: null,
      },
    ]
    const { host } = await shown({ library })
    expect(insideLines(host)).toEqual([
      'Also shown in Manga and Kids (inside /media)',
      'Kept out of Manga and Kids by their exclusions (inside /media/Manga)',
    ])
    expect(foldRows(host)).toEqual([
      ['kids/', 'Kids and Archive', 'also shown in Teens'],
    ])

    const dotted = manga({ name: 'Teens' })
    dotted.dirs[1].inside = [
      {
        path: '/media/Manga',
        libraries: ['Manga', 'Kids'],
        shows: false,
        kept_out_by: 'dot_name',
      },
    ]
    const again = await shown({ library: dotted })
    expect(insideLines(again.host)).toEqual([
      'Kept out of Manga and Kids: their scans skip names starting with a dot (inside /media/Manga)',
    ])
  })

  it('folds one directory inside it under a closed line in the singular', async () => {
    const library = manga()
    library.dirs[1].nested = [
      {
        id: 5,
        path: '/media/Manga/Chapters/kids',
        libraries: ['Kids'],
        shown: true,
        kept_out_by: null,
      },
    ]
    const { host } = await shown({ library })
    expect(fold(host)?.open).toBe(false)
    expect(fold(host)?.querySelector('summary')?.textContent).toBe(
      "1 folder inside uses its own library's rules",
    )
    expect(insideLines(host)).toEqual([])
  })

  it('folds two directories inside it under their count, a row each: its folder, its library, and whether this library shows it', async () => {
    const library = manga()
    library.dirs[1].nested = [
      {
        id: 5,
        path: '/media/Manga/Chapters/kids',
        libraries: ['Kids'],
        shown: true,
        kept_out_by: null,
      },
      {
        id: 6,
        path: '/media/Manga/Chapters/old',
        libraries: ['Archive'],
        shown: false,
        kept_out_by: 'exclusion',
      },
    ]
    const { host } = await shown({ library })
    expect(fold(host)?.open).toBe(false)
    expect(fold(host)?.querySelector('summary')?.textContent).toBe(
      "2 folders inside use their own library's rules",
    )
    expect(foldRows(host)).toEqual([
      ['kids/', 'Kids', 'also shown in Manga'],
      ['old/', 'Archive', "kept out by this directory's exclusions"],
    ])
  })

  it('says of the directory it sits in that its library shows this one too', async () => {
    const library = manga({ name: 'Kids' })
    library.dirs[1].inside = [
      {
        path: '/media/Manga',
        libraries: ['Manga'],
        shows: true,
        kept_out_by: null,
      },
    ]
    const { host } = await shown({ library })
    expect(insideLines(host)).toEqual([
      'Also shown in Manga (inside /media/Manga)',
    ])
    expect(fold(host)).toBeNull()
  })

  it('says a line for each of two directories it sits in, outermost first, the second keeping its books out', async () => {
    const library = manga({ name: 'Tiny' })
    library.dirs[1].inside = [
      {
        path: '/media/Manga',
        libraries: ['Manga'],
        shows: true,
        kept_out_by: null,
      },
      {
        path: '/media/Manga/Chapters',
        libraries: ['Kids'],
        shows: false,
        kept_out_by: 'exclusion',
      },
    ]
    const { host } = await shown({ library })
    expect(insideLines(host)).toEqual([
      'Also shown in Manga (inside /media/Manga)',
      'Kept out of Kids by its exclusions (inside /media/Manga/Chapters)',
    ])
  })

  it('says when a name starting with a dot, not an exclusion, keeps a directory out, on both sides', async () => {
    const outer = manga()
    outer.dirs[1].nested = [
      {
        id: 5,
        path: '/media/Manga/Chapters/.stash/kids',
        libraries: ['Kids'],
        shown: false,
        kept_out_by: 'dot_name',
      },
    ]
    const { host } = await shown({ library: outer })
    expect(foldRows(host)).toEqual([
      [
        '.stash/kids/',
        'Kids',
        'kept out: names starting with a dot are skipped',
      ],
    ])

    const inner = manga({ name: 'Kids' })
    inner.dirs[1].inside = [
      {
        path: '/media/Manga',
        libraries: ['Manga'],
        shows: false,
        kept_out_by: 'dot_name',
      },
    ]
    const again = await shown({ library: inner })
    expect(insideLines(again.host)).toEqual([
      'Kept out of Manga: its scans skip names starting with a dot (inside /media/Manga)',
    ])
  })

  it('draws no line, and nothing to hold one, for a directory with nothing around or inside it', async () => {
    const { host } = await shown()
    expect(host.querySelector('.dir-page .nesting')).toBeNull()
    // Its facts are still the whole of its line.
    expect(
      host.querySelector('.dir-page .page-description')?.children,
    ).toHaveLength(1)
  })
})

describe('DirPage, scans and removals across its library exclude each other', () => {
  it('holds Move… alone while a sibling is scanning, and leaves Remove… and Scan directory to send theirs, which the server takes', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 1))
    vi.mocked(config.removalCost).mockResolvedValue(counted(5))
    vi.mocked(config.startScan).mockResolvedValue({
      root_id: 2,
      status_url: '/api/library/roots/2/scan',
    })
    const { host } = await shown()

    expect(actionsHeld(host)).toEqual(['free', 'held', 'free'])
    await press(control(host, 'Move…'))
    expect(host.querySelector('.move-root')).toBeNull()

    await press(control(host, 'Remove…'))
    expect(config.removalCost).toHaveBeenCalledTimes(1)
    expect(host.querySelector('.remove-root .counts')?.textContent).toContain(
      '5 works',
    )
    const cancel = [...host.querySelectorAll('.remove-root button')].find(
      (b) => b.textContent === 'Cancel',
    ) as HTMLButtonElement
    await press(cancel)
    expect(host.querySelector('.remove-root')).toBeNull()

    await press(control(host, 'Scan directory'))
    expect(config.startScan).toHaveBeenCalledWith(2, 7)
  })

  it('holds Remove… alone while a directory inside it that its library shows is scanning, and frees it when that ends', async () => {
    // Kids' directory 5 inside Chapters: the status names it, as a scan of
    // Manga walks it, and names Chapters' removal as held.
    const library = manga()
    library.dirs[1].nested = [
      {
        id: 5,
        path: '/media/Manga/Chapters/kids',
        libraries: ['Kids'],
        shown: true,
        kept_out_by: null,
      },
    ]
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      running: true,
      elapsed_secs: 3,
      dirs: [
        { root_id: 1, running: false },
        { root_id: 2, running: false },
        { root_id: 5, running: true },
      ],
      held_removals: [2],
      moves_held: false,
      scans: [],
    })
    const { host } = await shown({ library })
    expect(actionsHeld(host)).toEqual(['free', 'free', 'held'])

    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    expect(actionsHeld(host)).toEqual(['free', 'free', 'free'])
  })

  it('holds Remove… alone while a directory it sits in is scanning, which its library’s status names only as a held removal', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      held_removals: [2],
      moves_held: false,
      scans: [],
    })
    const { host } = await shown()
    expect(actionsHeld(host)).toEqual(['free', 'free', 'held'])
    await press(control(host, 'Remove…'))
    expect(config.removalCost).not.toHaveBeenCalled()
  })

  it('holds Move… alone while the server’s job lock is held, and frees it once a read says it is not', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      moves_held: true,
    })
    const { host } = await shown()
    expect(actionsHeld(host)).toEqual(['free', 'held', 'free'])
    await press(control(host, 'Move…'))
    expect(host.querySelector('.move-root')).toBeNull()

    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    expect(actionsHeld(host)).toEqual(['free', 'free', 'free'])
  })

  it('holds nothing for a removal held on another of its library’s directories', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      held_removals: [1],
      moves_held: false,
      scans: [],
    })
    const { host } = await shown()
    expect(actionsHeld(host)).toEqual(['free', 'free', 'free'])
  })

  it('holds Scan directory, Move… and Remove… while its library is being scanned, and asks again until the scan ends', async () => {
    // A scan of the library holds every directory's slot until the whole scan
    // ends, so this directory's own status says running too.
    vi.mocked(api.libraryScanStatus).mockResolvedValue(
      scanningIn(manga(), 1, 2),
    )
    ownRunning(1, 2)
    const { host } = await shown()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(1)
    expect(api.libraryScanStatus).toHaveBeenCalledWith(7)
    expect(actionsHeld(host)).toEqual(['held', 'held', 'held'])
    expect(ownScanning(host)).not.toBeNull()

    await tick()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(2)
    expect(actionsHeld(host)).toEqual(['held', 'held', 'held'])

    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    ownRunning()
    await tick()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(3)
    expect(ownScanning(host)).toBeNull()
    expect(actionsHeld(host)).toEqual(['free', 'free', 'free'])

    // Nothing else is scanning, so nothing more is asked.
    await tick()
    await tick()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(3)
  })

  it('leaves a scan of only this directory to the directory’s own status, and asks its library’s once', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 2))
    ownRunning(2)
    const { host } = await shown()
    expect(ownScanning(host)).not.toBeNull()
    await tick()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(1)

    // The directory's own poll sees its scan end, and that alone frees the
    // buttons: the library's status is not asked again.
    ownRunning()
    await tick()
    expect(ownScanning(host)).toBeNull()
    expect(actionsHeld(host)).toEqual(['free', 'free', 'free'])
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(1)
  })

  // §9's call 9: told, a library's page drawn beside this one would read the
  // status, take the scan up and draw its report beside the row's own.
  it('marks its library’s scan status in the store stale as its own scan starts, and tells no page drawing it', async () => {
    vi.mocked(api.libraryScanStatus).mockImplementation((id) =>
      Promise.resolve(idle(id)),
    )
    await readSet([libraryScan(7), libraryScan(8)])
    const heard = vi.fn()
    const off = subscribe(libraryScan(7).address, heard)
    try {
      const { host } = await shown()
      vi.mocked(api.libraryScanStatus).mockClear()
      vi.mocked(config.startScan).mockResolvedValue({
        root_id: 2,
        status_url: '/api/library/roots/2/scan',
      })
      await press(control(host, 'Scan directory'))
      await flush()
      expect(ownScanning(host)).not.toBeNull()
      expect(entryOf(libraryScan(7))?.stale).toBe(true)
      expect(entryOf(libraryScan(8))?.stale).toBe(false)
      expect(heard).not.toHaveBeenCalled()
      expect(api.libraryScanStatus).not.toHaveBeenCalled()
    } finally {
      off()
    }
  })

  it('closes the directory’s open removal dialog when the page’s own first read finds a scan of the library under it, and asks its cost afresh once that ends', async () => {
    // The library's status and the directory's own are separate requests, and
    // the library's answers first here.
    const late = deferred<api.LibraryScanStatus>()
    vi.mocked(api.libraryScanStatus).mockReturnValueOnce(late.promise)
    const own = deferred<config.ScanStatus>()
    vi.mocked(config.scanStatus).mockReturnValueOnce(own.promise)
    vi.mocked(config.removalCost).mockResolvedValue(counted(5))
    const { host } = await shown()
    await press(control(host, 'Remove…'))
    expect(host.querySelector('.remove-root .counts')).not.toBeNull()

    await flush(() => late.resolve(scanningIn(manga(), 1, 2)))
    expect(host.querySelector('.remove-root')).toBeNull()
    expect(config.removeRoot).not.toHaveBeenCalled()
    await flush(() => own.resolve(statusOf(2, true)))
    expect(ownScanning(host)).not.toBeNull()

    vi.mocked(config.removalCost).mockResolvedValue(counted(9))
    await tick()
    await press(control(host, 'Remove…'))
    expect(config.removalCost).toHaveBeenCalledTimes(2)
    expect(host.querySelector('.remove-root .counts')?.textContent).toContain(
      '9 works',
    )
  })

  it('keeps a directory’s open removal dialog when a sibling is found scanning under it, and its confirm sends the removal', async () => {
    const late = deferred<api.LibraryScanStatus>()
    vi.mocked(api.libraryScanStatus).mockReturnValueOnce(late.promise)
    vi.mocked(config.removalCost).mockResolvedValue(counted(5))
    const { host } = await shown()
    await press(control(host, 'Remove…'))
    expect(host.querySelector('.remove-root .counts')?.textContent).toContain(
      '5 works',
    )

    await flush(() => late.resolve(scanningIn(manga(), 1)))
    expect(host.querySelector('.remove-root .counts')?.textContent).toContain(
      '5 works',
    )
    const del = deferred<config.RemovalAnswer>()
    vi.mocked(config.removeRoot).mockReturnValueOnce(del.promise)
    await press(host.querySelector('.remove-root .confirm') as Element)
    expect(config.removeRoot).toHaveBeenCalledWith(2)
    expect(config.removalCost).toHaveBeenCalledTimes(1)
  })

  it('closes a directory’s open removal dialog when its status names this directory’s removal held, and asks its cost afresh when it is opened again', async () => {
    const late = deferred<api.LibraryScanStatus>()
    vi.mocked(api.libraryScanStatus).mockReturnValueOnce(late.promise)
    vi.mocked(config.removalCost).mockResolvedValue(counted(5))
    const { host } = await shown()
    await press(control(host, 'Remove…'))
    expect(host.querySelector('.remove-root .counts')?.textContent).toContain(
      '5 works',
    )

    // A scan around this directory: no directory of the library is named
    // running, and this one's removal is held.
    await flush(() => late.resolve({ ...idle(7), held_removals: [2] }))
    expect(host.querySelector('.remove-root')).toBeNull()
    expect(config.removeRoot).not.toHaveBeenCalled()
    expect(config.removalCost).toHaveBeenCalledTimes(1)

    vi.mocked(config.removalCost).mockResolvedValue(counted(9))
    await tick()
    await press(control(host, 'Remove…'))
    expect(config.removalCost).toHaveBeenCalledTimes(2)
    expect(host.querySelector('.remove-root .counts')?.textContent).toContain(
      '9 works',
    )
  })

  it('keeps a directory’s removal dialog while its own DELETE is out when its removal is found held, and draws the DELETE’s refusal', async () => {
    const late = deferred<api.LibraryScanStatus>()
    vi.mocked(api.libraryScanStatus).mockReturnValueOnce(late.promise)
    vi.mocked(config.removalCost).mockResolvedValue(counted(5))
    const del = deferred<config.RemovalAnswer>()
    vi.mocked(config.removeRoot).mockReturnValueOnce(del.promise)
    const { host } = await shown()
    await press(control(host, 'Remove…'))
    await press(host.querySelector('.remove-root .confirm') as Element)
    pass(WAIT_DRAWN_AFTER_MS)
    expect(host.querySelector('.remove-root .confirm')?.textContent).toBe(
      'Removing…',
    )

    await flush(() => late.resolve({ ...idle(7), held_removals: [2] }))
    expect(host.querySelector('.remove-root .confirm')?.textContent).toBe(
      'Removing…',
    )
    expect(config.removeRoot).toHaveBeenCalledTimes(1)

    await flush(() =>
      del.reject(
        new Error('/api/library/roots/2: 500 {"error":"the disk is full"}'),
      ),
    )
    expect(host.querySelector('.remove-root [role="alert"]')?.textContent).toBe(
      'the disk is full',
    )
  })

  it('closes the directory’s open Move… dialog when its first read finds a scan of the library under it', async () => {
    // As in the removal's case above: the library's status answers before
    // the directory's own, which says running too.
    const late = deferred<api.LibraryScanStatus>()
    vi.mocked(api.libraryScanStatus).mockReturnValueOnce(late.promise)
    const own = deferred<config.ScanStatus>()
    vi.mocked(config.scanStatus).mockReturnValueOnce(own.promise)
    const { host } = await shown()
    await press(control(host, 'Move…'))
    expect(host.querySelector('.move-root')).not.toBeNull()

    await flush(() => late.resolve(scanningIn(manga(), 1, 2)))
    expect(host.querySelector('.move-root')).toBeNull()
    expect(api.moveDir).not.toHaveBeenCalled()
    await flush(() => own.resolve(statusOf(2, true)))
    expect(ownScanning(host)).not.toBeNull()
  })

  it('closes the directory’s open Move… dialog when its first read says the job lock is held', async () => {
    const late = deferred<api.LibraryScanStatus>()
    vi.mocked(api.libraryScanStatus).mockReturnValueOnce(late.promise)
    const { host } = await shown()
    await press(control(host, 'Move…'))
    expect(host.querySelector('.move-root')).not.toBeNull()

    await flush(() => late.resolve({ ...idle(7), moves_held: true }))
    expect(host.querySelector('.move-root')).toBeNull()
    expect(api.moveDir).not.toHaveBeenCalled()
  })

  it('does not hold the directory’s settings for a sibling’s scan', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 1))
    const { host } = await shown()
    expect(holdOf(control(host, 'Move…'))).toBe('held')

    const keepsDocuments = host.querySelector(
      'input[name="keeps_documents"]',
    ) as HTMLInputElement
    expect(keepsDocuments.disabled).toBe(false)
    await press(keepsDocuments)
    expect(config.patchRoot).toHaveBeenCalledWith(2, { keeps_documents: true })

    // The text boxes and the interval box send on blur, too.
    for (const name of [
      'edition_key',
      'default_kind',
      'default_language',
      'exclude_globs',
      'scan_interval_secs',
    ]) {
      const box = host.querySelector(`[name="${name}"]`) as HTMLInputElement
      expect(box.disabled, name).toBe(false)
    }
    await editBox(host.querySelector('input[name="edition_key"]'), 'weekly')
    expect(config.patchRoot).toHaveBeenCalledWith(2, { edition_key: 'weekly' })
    await editBox(host.querySelector('input[name="scan_interval_secs"]'), '600')
    expect(config.patchRoot).toHaveBeenCalledWith(2, {
      scan_interval_secs: 600,
    })
  })

  it('stops asking when it is unmounted while another directory is scanning', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 1))
    const { host } = await shown()
    vi.mocked(api.libraryScanStatus).mockClear()
    act(() => rerender(<div />))
    expect(host.querySelector('.dir-page')).toBeNull()
    await tick()
    await tick()
    expect(api.libraryScanStatus).not.toHaveBeenCalled()
  })

  it('drops an answer that arrives after it is unmounted, and asks no more', async () => {
    const late = deferred<api.LibraryScanStatus>()
    vi.mocked(api.libraryScanStatus).mockReturnValueOnce(late.promise)
    await shown()
    act(() => rerender(<div />))
    await flush(() => late.resolve(scanningIn(manga(), 1)))
    await tick()
    await tick()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(1)
  })

  it('keeps the hold through two failed polls, lets go on the third and says why, and asks no more', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 1))
    const { host } = await shown()
    expect(actionsHeld(host)).toEqual(['free', 'held', 'free'])

    vi.mocked(api.libraryScanStatus)
      .mockRejectedValueOnce(new Error('502'))
      .mockRejectedValueOnce(new Error('502'))
      .mockRejectedValue(
        new Error(
          '/api/library/libraries/7/scan: 500 {"error":"the store is locked"}',
        ),
      )
    await tick()
    await tick()
    expect(actionsHeld(host)).toEqual(['free', 'held', 'free'])
    expect(host.querySelector('[role="alert"]')).toBeNull()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(3)

    await tick()
    expect(actionsHeld(host)).toEqual(['free', 'free', 'free'])
    // The sentence of the poll that ended the hold, as the library page's
    // poll draws its own.
    expect(
      [...host.querySelectorAll('[role="alert"]')].map((p) => p.textContent),
    ).toEqual(['the store is locked'])
    await tick()
    await tick()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(4)
  })

  it('drops the sentence of a given-up hold when the directory’s own scan is pressed', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 1))
    vi.mocked(config.startScan).mockResolvedValue({
      root_id: 2,
      status_url: '/api/library/roots/2/scan',
    })
    const { host } = await shown()
    vi.mocked(api.libraryScanStatus).mockRejectedValue(
      new Error('/api/library/libraries/7/scan: 502 bad gateway'),
    )
    await tick()
    await tick()
    await tick()
    expect(host.querySelector('.dir-page > [role="alert"]')).not.toBeNull()

    await press(control(host, 'Scan directory'))
    expect(config.startScan).toHaveBeenCalledWith(2, 7)
    expect(host.querySelector('.dir-page > [role="alert"]')).toBeNull()
  })

  it('drops the sentence of a given-up hold when the page asks its library again, though that read fails', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 1))
    const { host } = await shown()
    vi.mocked(api.libraryScanStatus).mockRejectedValue(
      new Error('/api/library/libraries/7/scan: 502 bad gateway'),
    )
    await tick()
    await tick()
    await tick()
    expect(host.querySelector('.dir-page > [role="alert"]')).not.toBeNull()

    // The same path under another id: the page asks its library again, and
    // the read fails as a first read: silently.
    vi.mocked(api.libraryScanStatus).mockClear()
    const library = manga({
      dirs: [dir(1, '/media/Manga/Volumes'), dir(3, '/media/Manga/Chapters')],
    })
    await act(async () => {
      rerender(
        <MemoryRouter initialEntries={['/config/libraries/Manga/dirs/3']}>
          <DirPage
            library={library}
            root={library.dirs[1]}
            others={[{ id: 8, name: 'Comics' }]}
            onChanged={vi.fn()}
            onRemoved={vi.fn()}
            onUnlinked={vi.fn()}
            onMoved={vi.fn()}
          />
        </MemoryRouter>,
      )
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(1)
    expect(host.querySelector('.dir-page > [role="alert"]')).toBeNull()
    expect(actionsHeld(host)).toEqual(['free', 'free', 'free'])
  })

  it('counts only consecutive failed polls', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 1))
    const { host } = await shown()
    for (const answer of ['miss', 'miss', 'ok', 'miss', 'miss'] as const) {
      if (answer === 'miss')
        vi.mocked(api.libraryScanStatus).mockRejectedValueOnce(new Error('502'))
      else
        vi.mocked(api.libraryScanStatus).mockResolvedValueOnce(
          scanningIn(manga(), 1),
        )
    }
    for (let i = 0; i < 5; i++) await tick()
    // Four misses in five polls and never three together: still held.
    expect(actionsHeld(host)).toEqual(['free', 'held', 'free'])
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(6)
  })

  it('lets go of a held removal and a held move together on the third failed poll', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      held_removals: [2],
      moves_held: true,
    })
    const { host } = await shown()
    expect(actionsHeld(host)).toEqual(['free', 'held', 'held'])

    vi.mocked(api.libraryScanStatus).mockRejectedValue(
      new Error('/api/library/libraries/7/scan: 502 bad gateway'),
    )
    await tick()
    await tick()
    expect(actionsHeld(host)).toEqual(['free', 'held', 'held'])
    await tick()
    expect(actionsHeld(host)).toEqual(['free', 'free', 'free'])
    expect(
      host.querySelector('.dir-page > [role="alert"]')?.textContent,
    ).not.toBeUndefined()
  })

  it('lets go of a hold for a scan of its library on the third failed poll', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue(
      scanningIn(manga(), 1, 2),
    )
    const { host } = await shown()
    expect(actionsHeld(host)).toEqual(['held', 'held', 'held'])

    vi.mocked(api.libraryScanStatus).mockRejectedValue(
      new Error('/api/library/libraries/7/scan: 502 bad gateway'),
    )
    await tick()
    await tick()
    expect(actionsHeld(host)).toEqual(['held', 'held', 'held'])
    await tick()
    expect(actionsHeld(host)).toEqual(['free', 'free', 'free'])
  })

  it('holds nothing and asks no more when its first read fails', async () => {
    vi.mocked(api.libraryScanStatus).mockRejectedValue(
      new Error('/api/library/libraries/7/scan: 502 bad gateway'),
    )
    const { host } = await shown()
    expect(actionsHeld(host)).toEqual(['free', 'free', 'free'])
    expect(host.querySelector('[role="alert"]')).toBeNull()
    await tick()
    await tick()
    await tick()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(1)
  })
})

/**
 * What a scan found changes what the Library views draw. A scan of another
 * directory that this page saw running marks them stale once a poll finds
 * it over; a first read that finds none running saw no scan end, and marks
 * nothing.
 */
describe('DirPage, another directory’s scan and the Library views', () => {
  it('marks them stale when a sibling’s scan it saw running ends', async () => {
    const grid = await keptGrid()
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 1))
    const { host } = await shown()
    expect(actionsHeld(host)).toEqual(['free', 'held', 'free'])
    await tick()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(2)
    expect(grid.stale()).toBe(false)

    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    expect(actionsHeld(host)).toEqual(['free', 'free', 'free'])
    expect(grid.stale()).toBe(true)
    expect(grid.sent()).toBe(1)
  })

  it('marks none of them stale when its first read finds no other directory scanning', async () => {
    const grid = await keptGrid()
    const { host } = await shown()
    await tick()
    expect(actionsHeld(host)).toEqual(['free', 'free', 'free'])
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(1)
    expect(grid.stale()).toBe(false)
  })
})
