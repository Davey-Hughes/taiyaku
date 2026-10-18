import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from './config'
import { emptyDerive } from './testing/derive'
import { Libraries } from './Libraries'
import * as api from './libraries'
import { deferred, flush } from './testing/async'
import { press, submit, typeInto } from './testing/input'
import { fields } from './testing/fields'
import { mount } from './testing/mount'

vi.mock('./config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./config')>()),
  removalCost: vi.fn(),
  removeRoot: vi.fn(),
  scanStatus: vi.fn(),
}))
vi.mock('./libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./libraries')>()),
  addFolder: vi.fn(),
  createLibrary: vi.fn(),
  libraryRemovalCost: vi.fn(),
  libraryScanStatus: vi.fn(),
  listLibraries: vi.fn(),
  moveCost: vi.fn(),
  moveFolder: vi.fn(),
  removeLibrary: vi.fn(),
  renameLibrary: vi.fn(),
  startLibraryScan: vi.fn(),
}))

const folder = (id: number, path: string): config.Root => ({
  id,
  path,
  edition_key: null,
  default_kind: null,
  default_language: null,
  exclude_globs: null,
  scan_interval_secs: 86400,
  keeps_documents: false,
  last_scan_at: null,
  last_scan_status: null,
})

const library = (id: number, name: string, paths: string[]): api.Library => ({
  id,
  name,
  works: 0,
  books: 0,
  folders: paths.map((p, i) => folder(id * 10 + i, p)),
})

async function shown() {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(<Libraries />)
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

const form = (host: HTMLElement) =>
  host.querySelector('form.add-library') as HTMLFormElement

async function choose(host: HTMLElement, path: string) {
  await typeInto(
    form(host).querySelector('input[name="path"]') as HTMLInputElement,
    path,
  )
  await press(form(host).querySelector('button[name="pick"]') as Element)
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [] })
  vi.mocked(api.libraryScanStatus).mockImplementation((id) =>
    Promise.resolve({
      library_id: id,
      running: false,
      elapsed_secs: null,
      folders: [],
    }),
  )
  vi.mocked(config.scanStatus).mockImplementation((id) =>
    Promise.resolve({
      root_id: id,
      running: false,
      report: null,
      last_scan_at: null,
      last_scan_status: null,
    }),
  )
})

describe('Libraries', () => {
  it('heads itself, and says so when there is no library yet', async () => {
    const host = await shown()
    expect(host.querySelector('.libraries > h2')?.textContent).toBe('Libraries')
    expect(host.querySelector('.empty')?.textContent).toContain(
      'No libraries yet',
    )
    expect(host.querySelector('.library-list')).toBeNull()
  })

  it('draws a card per library, in the order sent', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [
        library(2, 'Comics', ['/media/Comics']),
        library(1, 'Manga', ['/media/Manga/Volumes', '/media/Manga/Chapters']),
      ],
    })
    const host = await shown()
    expect(
      [...host.querySelectorAll('.library-card > h3')].map(
        (h) => h.textContent,
      ),
    ).toEqual(['Comics', 'Manga'])
    expect(host.querySelector('.empty')).toBeNull()

    const manga = host.querySelectorAll('.library-card')[1]
    const move = [
      ...(manga.querySelector('.root-row')?.querySelectorAll('button') ?? []),
    ].find((b) => b.textContent === 'Move…') as HTMLButtonElement
    await press(move)
    const options = [
      ...(
        manga.querySelector('select[name="move-target"]') as HTMLSelectElement
      ).options,
    ].map((o) => o.textContent)
    expect(options).toContain('Comics')
    expect(options).not.toContain('Manga')
  })

  it('makes a library from a name and the folders chosen, then reloads', async () => {
    const made = library(1, 'Manga', [
      '/media/Manga/Volumes',
      '/media/Manga/Chapters',
    ])
    vi.mocked(api.createLibrary).mockResolvedValue({
      library: made,
      added: [
        {
          root: made.folders[0],
          watching: true,
          watch_reason: null,
          interval_secs: 86400,
        },
        {
          root: made.folders[1],
          watching: false,
          watch_reason: 'no events',
          interval_secs: 3600,
        },
      ],
    })
    const host = await shown()
    const make = form(host).querySelector(
      'button[type="submit"]',
    ) as HTMLButtonElement
    expect(make.disabled).toBe(true)

    await typeInto(
      form(host).querySelector(
        'input[name="library-name"]',
      ) as HTMLInputElement,
      'Manga',
    )
    expect(make.disabled).toBe(true)
    await choose(host, '/media/Manga/Volumes')
    await choose(host, '/media/Manga/Chapters')
    await choose(host, '/media/Manga/Volumes')
    expect(
      [...form(host).querySelectorAll('.chosen-folders code')].map(
        (c) => c.textContent,
      ),
    ).toEqual(['/media/Manga/Volumes', '/media/Manga/Chapters'])

    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
    await submit(form(host))

    expect(api.createLibrary).toHaveBeenCalledWith('Manga', [
      '/media/Manga/Volumes',
      '/media/Manga/Chapters',
    ])
    expect(host.querySelector('.library-card > h3')?.textContent).toBe('Manga')
    expect(form(host).querySelector('.chosen-folders')).toBeNull()
    // The probe each folder was added with is kept for its row.
    expect(
      [...host.querySelectorAll('.root-row .regime dd')].map(
        (d) => d.textContent,
      ),
    ).toEqual(expect.arrayContaining(['watching', 'probe failed: no events']))
  })

  it('takes a chosen folder back off the list', async () => {
    const host = await shown()
    await choose(host, '/media/a')
    await choose(host, '/media/b')
    await press(form(host).querySelector('button[name="unchoose"]') as Element)
    expect(
      [...form(host).querySelectorAll('.chosen-folders code')].map(
        (c) => c.textContent,
      ),
    ).toEqual(['/media/b'])
  })

  it("draws a refusal's sentence and keeps what was entered", async () => {
    vi.mocked(api.createLibrary).mockRejectedValue(
      new Error(
        '/api/library/libraries: 400 {"error":"/media/typo: No such file or directory (os error 2)"}',
      ),
    )
    const host = await shown()
    await typeInto(
      form(host).querySelector(
        'input[name="library-name"]',
      ) as HTMLInputElement,
      'Manga',
    )
    await choose(host, '/media/typo')
    await submit(form(host))

    expect(form(host).querySelector('[role="alert"]')?.textContent).toBe(
      '/media/typo: No such file or directory (os error 2)',
    )
    // Beside the button that was pressed, and not the list's slot.
    expect(host.querySelector('.libraries > [role="alert"]')).toBeNull()
    expect(
      (
        form(host).querySelector(
          'input[name="library-name"]',
        ) as HTMLInputElement
      ).value,
    ).toBe('Manga')
    expect(form(host).querySelectorAll('.chosen-folders li')).toHaveLength(1)
  })

  it('keeps a create refusal when a card reloads the list', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [library(2, 'Comics', ['/media/Comics'])],
    })
    vi.mocked(api.createLibrary).mockRejectedValue(
      new Error(
        '/api/library/libraries: 400 {"error":"a library is already called `Comics`"}',
      ),
    )
    vi.mocked(api.renameLibrary).mockResolvedValue({
      id: 2,
      name: 'Comics Two',
    })
    const host = await shown()
    await typeInto(
      form(host).querySelector(
        'input[name="library-name"]',
      ) as HTMLInputElement,
      'Comics',
    )
    await choose(host, '/media/x')
    await submit(form(host))
    expect(form(host).querySelector('[role="alert"]')).not.toBeNull()

    // A card's rename reloads the list. That read must not clear the form's
    // refusal.
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [library(2, 'Comics Two', ['/media/Comics'])],
    })
    await press(host.querySelector('button[name="rename-library"]') as Element)
    await typeInto(
      host.querySelector(
        'form.rename-library input[name="library-name"]',
      ) as HTMLInputElement,
      'Comics Two',
    )
    await submit(host.querySelector('form.rename-library') as Element)

    expect(host.querySelector('.library-card > h3')?.textContent).toBe(
      'Comics Two',
    )
    expect(form(host).querySelector('[role="alert"]')?.textContent).toContain(
      'already called',
    )
  })

  it('draws the newest list when two reads answer out of order', async () => {
    const first = deferred<{ libraries: api.Library[] }>()
    vi.mocked(api.listLibraries).mockReturnValueOnce(first.promise)
    const made = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.createLibrary).mockResolvedValue({ library: made, added: [] })
    const host = await shown()
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
    await typeInto(
      form(host).querySelector(
        'input[name="library-name"]',
      ) as HTMLInputElement,
      'Manga',
    )
    await choose(host, '/media/Manga')
    await submit(form(host))
    expect(host.querySelector('.library-card > h3')?.textContent).toBe('Manga')

    await flush(() => first.resolve({ libraries: [] }))
    expect(host.querySelector('.library-card > h3')?.textContent).toBe('Manga')
  })

  it('says why when the list cannot be read', async () => {
    vi.mocked(api.listLibraries).mockRejectedValue(
      new Error('/api/library/libraries: 500 the store is locked'),
    )
    const host = await shown()
    expect(
      host.querySelector('.libraries > [role="alert"]')?.textContent,
    ).toContain('the store is locked')
  })

  it('sends one create for two submits', async () => {
    vi.mocked(api.createLibrary).mockReturnValue(new Promise(() => {}))
    const host = await shown()
    await typeInto(
      form(host).querySelector(
        'input[name="library-name"]',
      ) as HTMLInputElement,
      'Manga',
    )
    await choose(host, '/media/Manga')
    await submit(form(host))
    await submit(form(host))
    expect(api.createLibrary).toHaveBeenCalledTimes(1)
  })

  it('clears the last refusal when the next attempt is made, and empties the name once it works', async () => {
    const made = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.createLibrary)
      .mockRejectedValueOnce(
        new Error('/api/library/libraries: 400 {"error":"refused"}'),
      )
      .mockResolvedValueOnce({ library: made, added: [] })
    const host = await shown()
    const name = form(host).querySelector(
      'input[name="library-name"]',
    ) as HTMLInputElement
    await typeInto(name, 'Manga')
    await choose(host, '/media/Manga')
    await submit(form(host))
    expect(form(host).querySelector('[role="alert"]')).not.toBeNull()

    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
    await submit(form(host))
    expect(form(host).querySelector('[role="alert"]')).toBeNull()
    expect(name.value).toBe('')
  })

  it('clears a create refusal when the name is edited, a folder is added or taken off', async () => {
    vi.mocked(api.createLibrary).mockRejectedValue(
      new Error('/api/library/libraries: 400 {"error":"refused"}'),
    )
    const host = await shown()
    const name = form(host).querySelector(
      'input[name="library-name"]',
    ) as HTMLInputElement
    const refused = () => form(host).querySelector('[role="alert"]')
    await typeInto(name, 'Manga')
    await choose(host, '/media/a')
    await choose(host, '/media/b')

    await submit(form(host))
    expect(refused()).not.toBeNull()
    await typeInto(name, 'Manga 2')
    expect(refused()).toBeNull()

    await submit(form(host))
    expect(refused()).not.toBeNull()
    await choose(host, '/media/c')
    expect(refused()).toBeNull()

    await submit(form(host))
    expect(refused()).not.toBeNull()
    await press(form(host).querySelector('button[name="unchoose"]') as Element)
    expect(refused()).toBeNull()
  })

  it('draws the create refusal beside the submit button, after it', async () => {
    vi.mocked(api.createLibrary).mockRejectedValue(
      new Error('/api/library/libraries: 400 {"error":"refused"}'),
    )
    const host = await shown()
    await typeInto(
      form(host).querySelector(
        'input[name="library-name"]',
      ) as HTMLInputElement,
      'Manga',
    )
    await choose(host, '/media/a')
    await submit(form(host))
    const submitButton = form(host).querySelector('button[type="submit"]')
    expect(submitButton?.nextElementSibling?.getAttribute('role')).toBe('alert')
  })

  it('draws what the re-derive after a removal found, and clears it when a library is made', async () => {
    const manga = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [manga] })
    vi.mocked(api.libraryRemovalCost).mockResolvedValue({
      removed: { works: 0, positions: 0, notes: 0, left_with_no_library: [] },
      derive: emptyDerive,
      dry_run: true,
    })
    vi.mocked(api.removeLibrary).mockResolvedValue({
      removed: { works: 0, positions: 0, notes: 0, left_with_no_library: [] },
      derive: { ...emptyDerive, lost_positions: 4 },
      dry_run: false,
    })
    const host = await shown()
    await press(host.querySelector('button[name="remove-library"]') as Element)
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [] })
    await press(host.querySelector('.remove-library .confirm') as Element)

    expect(host.querySelector('.library-card')).toBeNull()
    const report = host.querySelector('.libraries > .removal-report')
    expect(report?.querySelector('p')?.textContent).toBe(
      'What re-deriving after removing Manga found:',
    )
    expect(report?.querySelector('.derived')).not.toBeNull()
    expect(
      report?.querySelector('[data-field="lost_positions"] dd')?.textContent,
    ).toBe('4')

    const made = library(2, 'Comics', ['/media/Comics'])
    vi.mocked(api.createLibrary).mockResolvedValue({ library: made, added: [] })
    await typeInto(
      form(host).querySelector(
        'input[name="library-name"]',
      ) as HTMLInputElement,
      'Comics',
    )
    await choose(host, '/media/Comics')
    await submit(form(host))
    expect(host.querySelector('.removal-report')).toBeNull()
  })

  /** A list of two libraries, a removal rehearsed, and the real one answered. */
  function removable() {
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [
        library(2, 'Comics', ['/media/Comics']),
        library(1, 'Manga', ['/media/Manga']),
      ],
    })
    vi.mocked(api.libraryRemovalCost).mockResolvedValue({
      removed: { works: 0, positions: 0, notes: 0, left_with_no_library: [] },
      derive: emptyDerive,
      dry_run: true,
    })
    vi.mocked(api.removeLibrary).mockResolvedValue({
      removed: { works: 0, positions: 0, notes: 0, left_with_no_library: [] },
      derive: { ...emptyDerive, lost_positions: 4 },
      dry_run: false,
    })
  }

  async function removeManga(host: HTMLElement) {
    const buttons = host.querySelectorAll('button[name="remove-library"]')
    await press(buttons[1])
    await press(host.querySelector('.remove-library .confirm') as Element)
  }

  it('moves focus to the removal report, which is named, when one is drawn', async () => {
    removable()
    const host = await shown()
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [library(2, 'Comics', ['/media/Comics'])],
    })
    await removeManga(host)
    const report = host.querySelector('.libraries > .removal-report')
    expect(report).not.toBeNull()
    expect(document.activeElement).toBe(report)
    expect(report?.getAttribute('tabindex')).toBe('-1')
    // A group, which is what makes its name one: an `aria-label` on a plain
    // `div` is not announced. Named by its opening paragraph, which it
    // would otherwise repeat when read on.
    expect(report?.getAttribute('role')).toBe('group')
    expect(report?.hasAttribute('aria-label')).toBe(false)
    const named = document.getElementById(
      report?.getAttribute('aria-labelledby') ?? '',
    )
    expect(named).toBe(report?.querySelector(':scope > p'))
    expect(named?.textContent).toBe(
      'What re-deriving after removing Manga found:',
    )
  })

  it('drops a removed library at once when the list that follows cannot be read', async () => {
    removable()
    const host = await shown()
    vi.mocked(api.listLibraries).mockRejectedValue(
      new Error('/api/library/libraries: 500 the store is locked'),
    )
    await removeManga(host)
    expect(
      [...host.querySelectorAll('.library-card > h3')].map(
        (h) => h.textContent,
      ),
    ).toEqual(['Comics'])
    expect(
      host.querySelector('.libraries > [role="alert"]')?.textContent,
    ).toContain('the store is locked')
  })

  /** Makes a library of the one folder `path`, so the list reloads. */
  async function makeOne(host: HTMLElement, name: string, path: string) {
    await typeInto(
      form(host).querySelector(
        'input[name="library-name"]',
      ) as HTMLInputElement,
      name,
    )
    await choose(host, path)
    await submit(form(host))
  }

  it('does not hand a folder the probe of a removed one that shared its id', async () => {
    const first = library(1, 'Manga', ['/srv/first'])
    vi.mocked(api.createLibrary).mockResolvedValueOnce({
      library: first,
      added: [
        {
          root: first.folders[0],
          watching: false,
          watch_reason: 'inotify limit reached',
          interval_secs: 900,
        },
      ],
    })
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [first] })
    const host = await shown()
    await makeOne(host, 'Manga', '/srv/first')
    expect(host.textContent).toContain('inotify limit reached')

    // The folder's id, freed and given to another folder by the store.
    const second = {
      ...first,
      folders: [folder(first.folders[0].id, '/srv/second')],
    }
    vi.mocked(api.createLibrary).mockResolvedValueOnce({
      library: second,
      added: [],
    })
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [second] })
    await makeOne(host, 'Manga', '/srv/second')
    expect(host.textContent).toContain('/srv/second')
    expect(host.textContent).not.toContain('inotify limit reached')
  })

  it('does not reuse a folder’s row across two folders that shared an id', async () => {
    const edition = (host: HTMLElement) =>
      (host.querySelector('input[name="edition_key"]') as HTMLInputElement)
        .value
    const first = library(1, 'Manga', ['/srv/first'])
    first.folders[0].edition_key = 'first'
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [first] })
    const host = await shown()
    expect(edition(host)).toBe('first')

    const second = library(1, 'Manga', ['/srv/second'])
    second.folders[0].id = first.folders[0].id
    second.folders[0].edition_key = 'second'
    vi.mocked(api.createLibrary).mockResolvedValueOnce({
      library: second,
      added: [],
    })
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [second] })
    await makeOne(host, 'Other', '/srv/second')
    expect(host.textContent).toContain('/srv/second')
    expect(edition(host)).toBe('second')
  })

  it('keeps a watch probe through a failed read of the list, for the read that follows', async () => {
    // The probe runs when a folder is added and never again, so it is the
    // only chance to say what the watch found; a list that could not be read
    // after the add must not take it with it.
    const first = library(1, 'Manga', ['/srv/first'])
    vi.mocked(api.createLibrary).mockResolvedValueOnce({
      library: first,
      added: [
        {
          root: first.folders[0],
          watching: false,
          watch_reason: 'inotify limit reached',
          interval_secs: 900,
        },
      ],
    })
    vi.mocked(api.listLibraries).mockRejectedValue(
      new Error('/api/library/libraries: 500 the store is locked'),
    )
    const host = await shown()
    await makeOne(host, 'Manga', '/srv/first')
    expect(
      host.querySelector('.libraries > [role="alert"]')?.textContent,
    ).toContain('the store is locked')

    // A later read answers, as the next create's would.
    vi.mocked(api.createLibrary).mockResolvedValueOnce({
      library: library(2, 'Comics', ['/srv/comics']),
      added: [],
    })
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [first] })
    await makeOne(host, 'Comics', '/srv/comics')
    expect(
      [...host.querySelectorAll('.root-row .regime dd')].map(
        (d) => d.textContent,
      ),
    ).toContain('probe failed: inotify limit reached')
  })

  it('gives each box of the make-a-library form its label’s words as its aria-label — O175', async () => {
    const host = await shown()
    const labelled = fields(form(host).querySelectorAll('label.field'))
    expect(labelled.map((f) => f.written)).toEqual(['Name', 'Folder path'])
    for (const { written, control } of labelled) {
      expect(control.getAttribute('aria-label'), written).toBe(written)
    }
  })
})

describe('Libraries, focus and the two slots', () => {
  const answerOf = (lost = 0): config.RemovalAnswer => ({
    removed: { works: 0, positions: 0, notes: 0, left_with_no_library: [] },
    derive: { ...emptyDerive, lost_positions: lost },
    dry_run: false,
  })

  function twoLibraries() {
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [
        library(2, 'Comics', ['/media/Comics']),
        library(1, 'Manga', ['/media/Manga']),
      ],
    })
    vi.mocked(api.libraryRemovalCost).mockResolvedValue({
      ...answerOf(),
      dry_run: true,
    })
    vi.mocked(api.removeLibrary).mockResolvedValue(answerOf(4))
  }

  const nameBox = (host: HTMLElement) =>
    form(host).querySelector('input[name="library-name"]') as HTMLInputElement

  it('takes focus once for a report, and leaves it in the Name box while a person types, a card rereads the list, or a library is made', async () => {
    twoLibraries()
    const host = await shown()
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [library(2, 'Comics', ['/media/Comics'])],
    })
    await press(host.querySelectorAll('button[name="remove-library"]')[1])
    await press(host.querySelector('.remove-library .confirm') as Element)
    expect(document.activeElement).toBe(host.querySelector('.removal-report'))

    // Typing.
    act(() => nameBox(host).focus())
    await typeInto(nameBox(host), 'Co')
    expect(document.activeElement).toBe(nameBox(host))

    // A card's rename rereads the list.
    vi.mocked(api.renameLibrary).mockResolvedValue({ id: 2, name: 'Comix' })
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [library(2, 'Comix', ['/media/Comics'])],
    })
    await press(host.querySelector('button[name="rename-library"]') as Element)
    await typeInto(
      host.querySelector('form.rename-library input') as HTMLInputElement,
      'Comix',
    )
    act(() => nameBox(host).focus())
    await submit(host.querySelector('form.rename-library') as Element)
    expect(document.activeElement).toBe(nameBox(host))

    // A library being made.
    vi.mocked(api.createLibrary).mockReturnValue(new Promise(() => {}))
    await choose(host, '/media/x')
    act(() => nameBox(host).focus())
    await submit(form(host))
    expect(document.activeElement).toBe(nameBox(host))
  })

  it('leaves focus where it is when a read of the list fails after a report took it', async () => {
    twoLibraries()
    const host = await shown()
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [library(2, 'Comics', ['/media/Comics'])],
    })
    await press(host.querySelectorAll('button[name="remove-library"]')[1])
    await press(host.querySelector('.remove-library .confirm') as Element)
    expect(document.activeElement).toBe(host.querySelector('.removal-report'))

    act(() => nameBox(host).focus())
    vi.mocked(api.renameLibrary).mockResolvedValue({ id: 2, name: 'Comix' })
    vi.mocked(api.listLibraries).mockRejectedValue(
      new Error('/api/library/libraries: 500 {"error":"the store is locked"}'),
    )
    await press(host.querySelector('button[name="rename-library"]') as Element)
    await typeInto(
      host.querySelector('form.rename-library input') as HTMLInputElement,
      'Comix',
    )
    act(() => nameBox(host).focus())
    await submit(host.querySelector('form.rename-library') as Element)
    expect(
      host.querySelector('.libraries > [role="alert"]')?.textContent,
    ).toContain('the store is locked')
    expect(document.activeElement).toBe(nameBox(host))
  })

  it('draws a made library after those already drawn until the next read orders it', async () => {
    const comics = library(2, 'Comics', ['/media/Comics'])
    const zeta = library(3, 'Zeta', ['/media/Zeta'])
    const made = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [comics, zeta],
    })
    vi.mocked(api.createLibrary).mockResolvedValue({ library: made, added: [] })
    const host = await shown()
    const names = () =>
      [...host.querySelectorAll('.library-card > h3')].map((h) => h.textContent)
    const slow = deferred<{ libraries: api.Library[] }>()
    vi.mocked(api.listLibraries).mockReturnValue(slow.promise)
    await typeInto(nameBox(host), 'Manga')
    await choose(host, '/media/Manga')
    await submit(form(host))
    expect(names()).toEqual(['Comics', 'Zeta', 'Manga'])

    // The server orders by name, and the read that follows says where it goes.
    await flush(() => slow.resolve({ libraries: [comics, made, zeta] }))
    expect(names()).toEqual(['Comics', 'Manga', 'Zeta'])
  })

  it('holds only the library that is scanning, and none of the others', async () => {
    const comics = library(2, 'Comics', ['/media/Comics'])
    const manga = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [comics, manga],
    })
    vi.mocked(api.startLibraryScan).mockResolvedValue({
      library_id: 2,
      status_url: '/api/library/libraries/2/scan',
    })
    const host = await shown()
    vi.mocked(api.libraryScanStatus).mockImplementation((id) =>
      Promise.resolve({
        library_id: id,
        running: id === 2,
        elapsed_secs: 1,
        folders: [],
      }),
    )
    const cards = () => [...host.querySelectorAll('.library-card')]
    const held = (card: Element) =>
      [...card.querySelectorAll('button')]
        .filter((b) => b.disabled)
        .map((b) => b.textContent)
    await press(
      cards()[0].querySelector('button[name="scan-library"]') as Element,
    )

    // Held: the library's buttons and its folder's. The Add folder button is
    // disabled in every card until a path is typed, scanning or not.
    expect(held(cards()[0])).toEqual([
      'Scan library',
      'Remove library…',
      'Scan folder',
      'Move…',
      'Remove…',
      'Add folder',
    ])
    expect(held(cards()[1])).toEqual(['Add folder'])
    expect(cards()[1].querySelector('.scanning')).toBeNull()
  })

  it('holds no create refusal while the next attempt is in flight', async () => {
    vi.mocked(api.createLibrary)
      .mockRejectedValueOnce(
        new Error('/api/library/libraries: 400 {"error":"refused"}'),
      )
      .mockReturnValueOnce(new Promise(() => {}))
    const host = await shown()
    await typeInto(nameBox(host), 'Manga')
    await choose(host, '/media/Manga')
    await submit(form(host))
    expect(form(host).querySelector('[role="alert"]')).not.toBeNull()
    await submit(form(host))
    expect(form(host).querySelector('[role="alert"]')).toBeNull()
  })

  it('drops the first of two libraries at once too, by id and not by position', async () => {
    twoLibraries()
    const host = await shown()
    vi.mocked(api.listLibraries).mockRejectedValue(
      new Error('/api/library/libraries: 500 {"error":"the store is locked"}'),
    )
    await press(host.querySelectorAll('button[name="remove-library"]')[0])
    await press(host.querySelector('.remove-library .confirm') as Element)
    expect(
      [...host.querySelectorAll('.library-card > h3')].map(
        (h) => h.textContent,
      ),
    ).toEqual(['Manga'])
  })

  it('keeps the list’s failure when the make form is edited', async () => {
    vi.mocked(api.listLibraries).mockRejectedValue(
      new Error('/api/library/libraries: 500 {"error":"the store is locked"}'),
    )
    const host = await shown()
    expect(host.querySelector('.libraries > [role="alert"]')?.textContent).toBe(
      'the store is locked',
    )
    await typeInto(nameBox(host), 'M')
    await choose(host, '/media/x')
    await press(form(host).querySelector('button[name="unchoose"]') as Element)
    expect(host.querySelector('.libraries > [role="alert"]')?.textContent).toBe(
      'the store is locked',
    )
  })

  it('takes the list’s failure away when a later read succeeds', async () => {
    vi.mocked(api.listLibraries).mockRejectedValue(
      new Error('/api/library/libraries: 500 {"error":"the store is locked"}'),
    )
    const made = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.createLibrary).mockResolvedValue({ library: made, added: [] })
    const host = await shown()
    expect(host.querySelector('.libraries > [role="alert"]')).not.toBeNull()

    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
    await typeInto(nameBox(host), 'Manga')
    await choose(host, '/media/Manga')
    await submit(form(host))
    expect(host.querySelector('.library-card > h3')?.textContent).toBe('Manga')
    expect(host.querySelector('.libraries > [role="alert"]')).toBeNull()
  })

  it('draws the list’s failure as the server’s sentence', async () => {
    vi.mocked(api.listLibraries).mockRejectedValue(
      new Error('/api/library/libraries: 500 {"error":"the store is locked"}'),
    )
    const host = await shown()
    expect(host.querySelector('.libraries > [role="alert"]')?.textContent).toBe(
      'the store is locked',
    )
  })

  it('draws no failure from a read that a newer one has superseded', async () => {
    const first = deferred<{ libraries: api.Library[] }>()
    vi.mocked(api.listLibraries).mockReturnValueOnce(first.promise)
    const made = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.createLibrary).mockResolvedValue({ library: made, added: [] })
    const host = await shown()
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
    await typeInto(nameBox(host), 'Manga')
    await choose(host, '/media/Manga')
    await submit(form(host))
    await flush(() =>
      first.reject(
        new Error('/api/library/libraries: 500 {"error":"too late"}'),
      ),
    )
    expect(host.querySelector('.libraries > [role="alert"]')).toBeNull()
    expect(host.querySelector('.library-card > h3')?.textContent).toBe('Manga')
  })

  it('keeps the watch probe of a folder added to a library, for its row', async () => {
    const manga = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [manga] })
    const extra = folder(9, '/media/Manga/Extras')
    vi.mocked(api.addFolder).mockResolvedValue({
      root: extra,
      watching: false,
      watch_reason: 'inotify limit reached',
      interval_secs: 900,
    })
    const host = await shown()
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [{ ...manga, folders: [...manga.folders, extra] }],
    })
    const picker = host.querySelector(
      '.library-card > .add-root',
    ) as HTMLElement
    await typeInto(
      picker.querySelector('input[name="path"]') as HTMLInputElement,
      '/media/Manga/Extras',
    )
    await press(picker.querySelector('button[name="pick"]') as Element)
    expect(
      [...host.querySelectorAll('.root-row .regime dd')].map(
        (d) => d.textContent,
      ),
    ).toContain('probe failed: inotify limit reached')
  })

  it('draws the library that was just made at once, when the list that follows cannot be read', async () => {
    const made = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.createLibrary).mockResolvedValue({ library: made, added: [] })
    const host = await shown()
    expect(host.querySelector('.empty')).not.toBeNull()
    vi.mocked(api.listLibraries).mockRejectedValue(
      new Error('/api/library/libraries: 500 {"error":"the store is locked"}'),
    )
    await typeInto(nameBox(host), 'Manga')
    await choose(host, '/media/Manga')
    await submit(form(host))
    expect(host.querySelector('.library-card > h3')?.textContent).toBe('Manga')
    expect(host.querySelector('.empty')).toBeNull()
    expect(host.querySelector('.libraries > [role="alert"]')?.textContent).toBe(
      'The libraries list could not be read again (the store is locked), so what is drawn may be out of date.',
    )
  })
})

describe('Libraries, a folder’s move', () => {
  it('keeps what deriving after the move found, named for the folder and the library, once the list has reloaded', async () => {
    const answer = (dry_run: boolean): api.MoveAnswer => ({
      moved: {
        library_id: 2,
        library: 'Comics',
        works_kept: 1,
        works_joined: 0,
        works_split: 0,
        series_joined: 2,
        // The rehearsal's counts differ from the move's: the kept report
        // can only have the move's.
        positions: dry_run ? 1 : 3,
        notes: 0,
        alignments_dropped: 0,
        library_removed: null,
        grants_removed: 0,
        left_with_no_library: [],
      },
      derive: { ...emptyDerive, lost_positions: dry_run ? 2 : 6 },
      dry_run,
    })
    vi.mocked(api.moveCost).mockResolvedValue(answer(true))
    vi.mocked(api.moveFolder).mockResolvedValue(answer(false))
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [
        library(1, 'Manga', ['/media/Manga/Volumes', '/media/Manga/Chapters']),
        library(2, 'Comics', ['/media/Comics']),
      ],
    })
    const host = await shown()
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [
        library(1, 'Manga', ['/media/Manga/Volumes']),
        library(2, 'Comics', ['/media/Comics', '/media/Manga/Chapters']),
      ],
    })
    const row = host.querySelectorAll('.root-row')[1]
    await press(
      [...row.querySelectorAll('button')].find(
        (b) => b.textContent === 'Move…',
      ) as Element,
    )
    await act(async () => {
      const box = row.querySelector(
        'select[name="move-target"]',
      ) as HTMLSelectElement
      Object.getOwnPropertyDescriptor(
        HTMLSelectElement.prototype,
        'value',
      )?.set?.call(box, '2')
      box.dispatchEvent(new Event('change', { bubbles: true }))
    })
    await press(row.querySelector('button[name="rehearse"]') as Element)
    await press(row.querySelector('.move-root .confirm') as Element)

    const report = host.querySelector(
      '.libraries > .removal-report',
    ) as HTMLElement
    expect(report.querySelector(':scope > p')?.textContent).toBe(
      'Moved /media/Manga/Chapters from Manga to Comics.',
    )
    // What the move did leads, in the past tense, before what deriving found.
    const did = report.querySelector('.move-counts')?.textContent ?? ''
    expect(did).toContain(
      '1 work kept its address, 0 works joined a work Comics already held, as another edition, and 0 works were split between the two libraries.',
    )
    expect(did).toContain('3 in-progress positions and 0 notes went with them.')
    expect(did).toContain('2 series already in Comics gained volumes.')
    expect(
      [...report.querySelectorAll(':scope > p')].map((p) => p.textContent),
    ).toEqual([
      'Moved /media/Manga/Chapters from Manga to Comics.',
      'What deriving after the move found:',
    ])
    expect(
      report.querySelector('[data-field="lost_positions"] dd')?.textContent,
    ).toBe('6')
    const pathsOf = (card: Element) =>
      [...card.querySelectorAll('.root-row code')].map((c) => c.textContent)
    const cards = host.querySelectorAll('.library-card')
    expect(pathsOf(cards[0])).toEqual(['/media/Manga/Volumes'])
    expect(pathsOf(cards[1])).toEqual([
      '/media/Comics',
      '/media/Manga/Chapters',
    ])
  })

  /** Moves Manga's second folder to `to` and answers the report kept. */
  async function keptReport(
    to: '2' | 'new',
    moved: Partial<api.Moved>,
  ): Promise<HTMLElement> {
    const answer = (dry_run: boolean): api.MoveAnswer => ({
      moved: { ...movedAnswer(null).moved, ...moved },
      derive: emptyDerive,
      dry_run,
    })
    vi.mocked(api.moveCost).mockResolvedValue(answer(true))
    vi.mocked(api.moveFolder).mockResolvedValue(answer(false))
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [
        library(1, 'Manga', ['/media/Manga/Volumes', '/media/Manga/Chapters']),
        library(2, 'Comics', ['/media/Comics']),
      ],
    })
    const host = await shown()
    const row = host.querySelectorAll('.root-row')[1]
    await press(
      [...row.querySelectorAll('button')].find(
        (b) => b.textContent === 'Move…',
      ) as Element,
    )
    await act(async () => {
      const box = row.querySelector(
        'select[name="move-target"]',
      ) as HTMLSelectElement
      Object.getOwnPropertyDescriptor(
        HTMLSelectElement.prototype,
        'value',
      )?.set?.call(box, to)
      box.dispatchEvent(new Event('change', { bubbles: true }))
    })
    if (to === 'new') {
      await typeInto(
        row.querySelector('input[name="new-library"]') as HTMLInputElement,
        'Extras',
      )
    }
    await press(row.querySelector('button[name="rehearse"]') as Element)
    await press(row.querySelector('.move-root .confirm') as Element)
    return host.querySelector('.libraries > .removal-report') as HTMLElement
  }

  it('leaves the join out of the kept report where the target was made by the move', async () => {
    const report = await keptReport('new', {
      library: 'Extras',
      library_id: 9,
      works_kept: 2,
    })
    expect(report.querySelector('.move-counts')?.textContent).toContain(
      '2 works kept their address and 0 works were split between the two libraries.',
    )
    expect(report.textContent).not.toContain('joined')
  })

  it('says what the move dropped and removed, in the past tense', async () => {
    const report = await keptReport('2', {
      alignments_dropped: 2,
      library_removed: 'Manga',
      grants_removed: 3,
    })
    const did = report.querySelector('.move-counts')?.textContent ?? ''
    expect(did).toContain(
      '2 alignments between editions the move separated were dropped.',
    )
    expect(did).toContain(
      'Manga had no other folder, so it was removed, with 3 grants.',
    )
  })

  /** Moves the second folder of Manga to Comics, the list failing to reload. */
  async function moveThenFailToReload(
    host: HTMLElement,
    library_removed: string | null,
  ) {
    vi.mocked(api.listLibraries).mockRejectedValue(
      new Error('/api/library/libraries: 500 {"error":"the store is locked"}'),
    )
    const row = host.querySelectorAll('.root-row')[1]
    await press(
      [...row.querySelectorAll('button')].find(
        (b) => b.textContent === 'Move…',
      ) as Element,
    )
    await act(async () => {
      const box = row.querySelector(
        'select[name="move-target"]',
      ) as HTMLSelectElement
      Object.getOwnPropertyDescriptor(
        HTMLSelectElement.prototype,
        'value',
      )?.set?.call(box, '2')
      box.dispatchEvent(new Event('change', { bubbles: true }))
    })
    await press(row.querySelector('button[name="rehearse"]') as Element)
    await press(row.querySelector('.move-root .confirm') as Element)
    return library_removed
  }

  const movedAnswer = (library_removed: string | null): api.MoveAnswer => ({
    moved: {
      library_id: 2,
      library: 'Comics',
      works_kept: 1,
      works_joined: 0,
      works_split: 0,
      series_joined: 0,
      positions: 0,
      notes: 0,
      alignments_dropped: 0,
      library_removed,
      grants_removed: 0,
      left_with_no_library: [],
    },
    derive: emptyDerive,
    dry_run: false,
  })

  it('drops a moved folder from its library at once when the list that follows cannot be read', async () => {
    vi.mocked(api.moveCost).mockResolvedValue({
      ...movedAnswer(null),
      dry_run: true,
    })
    vi.mocked(api.moveFolder).mockResolvedValue(movedAnswer(null))
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [
        library(1, 'Manga', ['/media/Manga/Volumes', '/media/Manga/Chapters']),
        library(2, 'Comics', ['/media/Comics']),
      ],
    })
    const host = await shown()
    await moveThenFailToReload(host, null)
    // Gone from Manga; not drawn in Comics, which the reload would do.
    expect(
      [...host.querySelectorAll('.root-row code')].map((c) => c.textContent),
    ).toEqual(['/media/Manga/Volumes', '/media/Comics'])
    expect(
      host.querySelector('.libraries > .removal-report > p')?.textContent,
    ).toBe('Moved /media/Manga/Chapters from Manga to Comics.')
    expect(
      host.querySelector('.libraries > [role="alert"]')?.textContent,
    ).toContain('the store is locked')
  })

  it('drops the library a move emptied at once when the list that follows cannot be read', async () => {
    vi.mocked(api.moveCost).mockResolvedValue({
      ...movedAnswer('Manga'),
      dry_run: true,
    })
    vi.mocked(api.moveFolder).mockResolvedValue(movedAnswer('Manga'))
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [
        library(1, 'Manga', ['/media/Manga/Chapters']),
        library(2, 'Comics', ['/media/Comics']),
      ],
    })
    const host = await shown()
    const row = host.querySelectorAll('.root-row')[0]
    vi.mocked(api.listLibraries).mockRejectedValue(
      new Error('/api/library/libraries: 500 {"error":"the store is locked"}'),
    )
    await press(
      [...row.querySelectorAll('button')].find(
        (b) => b.textContent === 'Move…',
      ) as Element,
    )
    await act(async () => {
      const box = row.querySelector(
        'select[name="move-target"]',
      ) as HTMLSelectElement
      Object.getOwnPropertyDescriptor(
        HTMLSelectElement.prototype,
        'value',
      )?.set?.call(box, '2')
      box.dispatchEvent(new Event('change', { bubbles: true }))
    })
    await press(row.querySelector('button[name="rehearse"]') as Element)
    await press(row.querySelector('.move-root .confirm') as Element)
    expect(
      [...host.querySelectorAll('.library-card > h3')].map(
        (h) => h.textContent,
      ),
    ).toEqual(['Comics'])
  })
})

describe('Libraries, a folder’s removal', () => {
  async function removeFolder(host: HTMLElement, row: number) {
    const rows = host.querySelectorAll('.root-row')
    const open = [...rows[row].querySelectorAll('button')].find(
      (b) => b.textContent === 'Remove…',
    ) as Element
    await press(open)
    await press(host.querySelector('.remove-root .confirm') as Element)
  }

  beforeEach(() => {
    vi.mocked(config.removalCost).mockResolvedValue({
      removed: { works: 5, positions: 2, notes: 0, left_with_no_library: [] },
      derive: emptyDerive,
      dry_run: true,
    })
    vi.mocked(config.removeRoot).mockResolvedValue({
      removed: { works: 5, positions: 2, notes: 0, left_with_no_library: [] },
      derive: { ...emptyDerive, lost_positions: 4 },
      dry_run: false,
    })
  })

  it('keeps what a folder’s re-derive found, named for the folder, once the list has reloaded', async () => {
    const two = library(1, 'Manga', [
      '/media/Manga/Volumes',
      '/media/Manga/Chapters',
    ])
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [two] })
    const host = await shown()
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [library(1, 'Manga', ['/media/Manga/Volumes'])],
    })
    await removeFolder(host, 1)

    const report = host.querySelector(
      '.libraries > .removal-report',
    ) as HTMLElement
    expect(report.querySelector(':scope > p')?.textContent).toBe(
      'What re-deriving after removing /media/Manga/Chapters found:',
    )
    expect(
      report.querySelector('[data-field="lost_positions"] dd')?.textContent,
    ).toBe('4')
    expect(document.activeElement).toBe(report)
    // Its headings sit one level beneath the screen's own.
    expect(
      [...report.querySelectorAll('.report-heading')].map((h) => h.tagName),
    ).toEqual(['H3'])
    // The library stays, with the folder that is left.
    expect(host.querySelector('.library-card > h3')?.textContent).toBe('Manga')
    expect(host.querySelectorAll('.root-row')).toHaveLength(1)
  })

  it('drops a removed folder at once when the list that follows cannot be read', async () => {
    const two = library(1, 'Manga', [
      '/media/Manga/Volumes',
      '/media/Manga/Chapters',
    ])
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [two] })
    const host = await shown()
    vi.mocked(api.listLibraries).mockRejectedValue(
      new Error('/api/library/libraries: 500 {"error":"the store is locked"}'),
    )
    await removeFolder(host, 1)

    expect(
      [...host.querySelectorAll('.root-row code')].map((c) => c.textContent),
    ).toEqual(['/media/Manga/Volumes'])
    expect(host.querySelector('.holds')?.textContent).toBe(
      '0 works in 1 folder',
    )
    expect(
      host.querySelector('.libraries > .removal-report > p')?.textContent,
    ).toBe('What re-deriving after removing /media/Manga/Chapters found:')
    expect(
      host.querySelector('.libraries > [role="alert"]')?.textContent,
    ).toContain('the store is locked')
    // What is left is live, and is now the library's last folder.
    const remove = [...host.querySelectorAll('.root-row button')].find(
      (b) => b.textContent === 'Remove…',
    ) as HTMLButtonElement
    expect(remove.disabled).toBe(false)
    await press(remove)
    expect(
      host.querySelector('.remove-root .last-folder')?.textContent,
    ).toContain('the only folder of the library Manga')
  })

  it('names the library, not the folder, when both folders of a library are removed before the list answers', async () => {
    const two = library(1, 'Manga', [
      '/media/Manga/Volumes',
      '/media/Manga/Chapters',
    ])
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [two] })
    vi.mocked(config.removeRoot)
      .mockResolvedValueOnce({
        removed: { works: 5, positions: 2, notes: 0, left_with_no_library: [] },
        derive: { ...emptyDerive, lost_positions: 4 },
        dry_run: false,
      })
      .mockResolvedValueOnce({
        removed: { works: 5, positions: 2, notes: 0, left_with_no_library: [] },
        derive: { ...emptyDerive, lost_positions: 7 },
        dry_run: false,
      })
    const host = await shown()
    const slow = deferred<{ libraries: api.Library[] }>()
    vi.mocked(api.listLibraries).mockReturnValue(slow.promise)
    await removeFolder(host, 1)
    expect(
      host.querySelector('.libraries > .removal-report > p')?.textContent,
    ).toBe('What re-deriving after removing /media/Manga/Chapters found:')

    await removeFolder(host, 0)
    const report = host.querySelector(
      '.libraries > .removal-report',
    ) as HTMLElement
    expect(report.querySelector(':scope > p')?.textContent).toBe(
      'What re-deriving after removing Manga found:',
    )
    expect(
      report.querySelector('[data-field="lost_positions"] dd')?.textContent,
    ).toBe('7')
    expect(host.querySelector('.library-card')).toBeNull()

    await flush(() => slow.resolve({ libraries: [] }))
    expect(host.querySelector('.library-card')).toBeNull()
  })

  it('drops a library at once when its last folder went and the list cannot be read, and reads the list once', async () => {
    const one = library(1, 'Manga', ['/media/Manga'])
    const comics = library(2, 'Comics', ['/media/Comics'])
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [comics, one] })
    const host = await shown()
    vi.mocked(api.listLibraries).mockRejectedValue(
      new Error('/api/library/libraries: 500 {"error":"the store is locked"}'),
    )
    vi.mocked(api.listLibraries).mockClear()
    await removeFolder(host, 1)

    expect(
      [...host.querySelectorAll('.library-card > h3')].map(
        (h) => h.textContent,
      ),
    ).toEqual(['Comics'])
    expect(
      host.querySelector('.libraries > .removal-report > p')?.textContent,
    ).toBe('What re-deriving after removing Manga found:')
    expect(api.listLibraries).toHaveBeenCalledTimes(1)
  })

  it('names the library when its last folder went and the library with it', async () => {
    const one = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [one, library(2, 'Comics', ['/media/Comics'])],
    })
    const host = await shown()
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [library(2, 'Comics', ['/media/Comics'])],
    })
    await removeFolder(host, 0)

    const report = host.querySelector(
      '.libraries > .removal-report',
    ) as HTMLElement
    expect(report.querySelector(':scope > p')?.textContent).toBe(
      'What re-deriving after removing Manga found:',
    )
    expect(
      report.querySelector('[data-field="lost_positions"] dd')?.textContent,
    ).toBe('4')
    expect(document.activeElement).toBe(report)
    expect(
      [...host.querySelectorAll('.library-card > h3')].map(
        (h) => h.textContent,
      ),
    ).toEqual(['Comics'])
  })

  it('says why Make the library is held while a path waits in the box', async () => {
    const host = await shown()
    const form = host.querySelector('form.add-library') as HTMLFormElement
    const make = form.querySelector(
      'button[type="submit"]',
    ) as HTMLButtonElement
    await typeInto(
      form.querySelector('input[name="library-name"]') as HTMLInputElement,
      'Comics',
    )
    await typeInto(
      form.querySelector('input[name="path"]') as HTMLInputElement,
      '/srv/comics',
    )

    expect(make.disabled).toBe(true)
    const why = document.getElementById(
      make.getAttribute('aria-describedby') ?? '',
    )
    expect(why?.textContent).toBe(
      'The folder in the box is not on the list yet: “Add to the list” puts it there.',
    )

    await press(form.querySelector('button[name="pick"]') as Element)
    expect(make.disabled).toBe(false)
    expect(make.getAttribute('aria-describedby')).toBeNull()
    expect(form.textContent).not.toContain('is not on the list yet')

    // Another path waiting in the box does not hold the button: a folder is
    // on the list already.
    await typeInto(
      form.querySelector('input[name="path"]') as HTMLInputElement,
      '/srv/more',
    )
    expect(make.disabled).toBe(false)
    expect(make.getAttribute('aria-describedby')).toBeNull()
    expect(form.textContent).not.toContain('is not on the list yet')
  })

  it('says nothing of a box holding only white space', async () => {
    const host = await shown()
    const form = host.querySelector('form.add-library') as HTMLFormElement
    const make = form.querySelector(
      'button[type="submit"]',
    ) as HTMLButtonElement
    await typeInto(
      form.querySelector('input[name="path"]') as HTMLInputElement,
      '   ',
    )
    expect(make.getAttribute('aria-describedby')).toBeNull()
    expect(form.textContent).not.toContain('is not on the list yet')
  })

  it('says nothing of the box while the box is empty', async () => {
    const host = await shown()
    const form = host.querySelector('form.add-library') as HTMLFormElement
    expect(form.textContent).not.toContain('is not on the list yet')
  })
})
