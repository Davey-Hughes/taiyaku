import { act, useState } from 'react'
import { MemoryRouter } from 'react-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from './config'
import * as api from './libraries'
import { LibraryPage } from './LibraryPage'
import { deferred, flush } from './testing/async'
import { emptyDerive } from './testing/derive'
import { press, submit, typeInto } from './testing/input'
import {
  counted,
  dir,
  idle,
  manga,
  scanningIn,
  tick,
} from './testing/libraries'
import { mount, rerender } from './testing/mount'
import { cellValue } from './testing/table'

vi.mock('./config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./config')>()),
  scanStatus: vi.fn(),
  startScan: vi.fn(),
}))
vi.mock('./libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./libraries')>()),
  addDir: vi.fn(),
  libraryRemovalCost: vi.fn(),
  libraryScanStatus: vi.fn(),
  listDirs: vi.fn(),
  removeLibrary: vi.fn(),
  renameLibrary: vi.fn(),
  startLibraryScan: vi.fn(),
}))

/** The page at its own route, with each handler a mock. */
async function shown(library = manga()) {
  const onAdded = vi.fn()
  const onChanged = vi.fn()
  const onRemoved = vi.fn()
  const page = (drawn: api.Library) => (
    <MemoryRouter initialEntries={[`/config/libraries/${drawn.id}`]}>
      <LibraryPage
        library={drawn}
        onAdded={onAdded}
        onChanged={onChanged}
        onRemoved={onRemoved}
      />
    </MemoryRouter>
  )
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(page(library))
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  /** The same page handed another library, as a reload of the list does. */
  const redraw = (next: api.Library) =>
    act(async () => {
      rerender(page(next))
      await Promise.resolve()
    })
  return { host, onAdded, onChanged, onRemoved, redraw }
}

const button = (host: HTMLElement, name: string) =>
  host.querySelector(`button[name="${name}"]`) as HTMLButtonElement

/** Press Add directory… and hand back what holds the picker and its refusal. */
async function openPicker(host: HTMLElement) {
  await press(button(host, 'add-dir'))
  return host.querySelector('.library-page > .add-root') as HTMLElement
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.useFakeTimers({ shouldAdvanceTime: true })
  vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
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

afterEach(() => {
  vi.useRealTimers()
})

describe('LibraryPage', () => {
  it('words the library’s button Scan library, and leaves Scan directory to each directory’s page', async () => {
    const { host } = await shown()
    expect(button(host, 'scan-library').textContent).toBe('Scan library')
    expect(
      [...host.querySelectorAll('button')].filter((b) =>
        b.textContent?.startsWith('Scan directory'),
      ),
    ).toEqual([])
  })

  it('says a scan is running in the library, not that it is scanning the library’s directories', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      running: true,
      elapsed_secs: 9,
    })
    const { host } = await shown()
    expect(
      host.querySelector('.library-page > .scanning')?.textContent,
    ).toMatch(/^A scan is running in this library… 9s$/)
  })

  it('names the library, what it holds, and a row per directory', async () => {
    const { host } = await shown()
    expect(host.querySelector('.library-page > h2')?.textContent).toBe('Manga')
    expect(host.querySelector('.holds')?.textContent).toBe(
      '12 works in 2 directories',
    )
    expect(
      [...host.querySelectorAll('.dir-table tbody code')].map(
        (c) => c.textContent,
      ),
    ).toEqual(['/media/Manga/Volumes', '/media/Manga/Chapters'])
    expect(host.querySelector('.no-readable')).toBeNull()
  })

  it('says so when it holds books and no work, and only then', async () => {
    const books = await shown(
      manga({ works: 0, books: 3, dirs: [dir(1, '/media/Books')] }),
    )
    expect(books.host.querySelector('.holds')?.textContent).toBe(
      '0 works in 1 directory',
    )
    expect(books.host.querySelector('.no-readable')?.textContent).toContain(
      'No readable files yet: its scans found 3 EPUB or PDF files',
    )
    const mixed = await shown(manga({ works: 2, books: 3 }))
    expect(mixed.host.querySelector('.no-readable')).toBeNull()
    const empty = await shown(manga({ works: 0, books: 0 }))
    expect(empty.host.querySelector('.no-readable')).toBeNull()
  })

  it('renames by id and reloads', async () => {
    vi.mocked(api.renameLibrary).mockResolvedValue({ id: 7, name: 'Comics' })
    const { host, onChanged } = await shown()
    await press(button(host, 'rename-library'))
    const box = host.querySelector(
      'input[name="library-name"]',
    ) as HTMLInputElement
    expect(box.value).toBe('Manga')

    await typeInto(box, 'Comics')
    await submit(host.querySelector('form.rename-library') as Element)

    expect(api.renameLibrary).toHaveBeenCalledWith(7, 'Comics')
    expect(onChanged).toHaveBeenCalled()
    expect(host.querySelector('form.rename-library')).toBeNull()
  })

  it('keeps the rename open and says why when the name is taken', async () => {
    vi.mocked(api.renameLibrary).mockRejectedValue(
      new Error(
        '/api/library/libraries/7: 400 {"error":"a library is already called `Comics`"}',
      ),
    )
    const { host, onChanged } = await shown()
    await press(button(host, 'rename-library'))
    await typeInto(
      host.querySelector('input[name="library-name"]') as HTMLInputElement,
      'Comics',
    )
    await submit(host.querySelector('form.rename-library') as Element)

    expect(
      host.querySelector('form.rename-library [role="alert"]')?.textContent,
    ).toBe('a library is already called `Comics`')
    expect(host.querySelector('.library-page > [role="alert"]')).toBeNull()
    expect(host.querySelector('form.rename-library')).not.toBeNull()
    expect(onChanged).not.toHaveBeenCalled()
  })

  it('adds a directory to this library, hands its probe up, and reloads', async () => {
    const added: config.Added = {
      root: dir(3, '/media/Manga/Extras'),
      watching: true,
      watch_reason: null,
      interval_secs: 86400,
    }
    vi.mocked(api.addDir).mockResolvedValue(added)
    const { host, onAdded, onChanged } = await shown()
    const picker = await openPicker(host)

    await typeInto(
      picker.querySelector('input[name="path"]') as HTMLInputElement,
      '/media/Manga/Extras',
    )
    await press(picker.querySelector('button[name="pick"]') as Element)

    expect(api.addDir).toHaveBeenCalledWith(7, '/media/Manga/Extras')
    expect(onAdded).toHaveBeenCalledWith([added])
    expect(onChanged).toHaveBeenCalled()
  })

  it("draws a refused directory's sentence and keeps what was typed", async () => {
    vi.mocked(api.addDir).mockRejectedValue(
      new Error(
        '/api/library/libraries/7/dirs: 400 {"error":"/media/Manga/Volumes/x is inside /media/Manga/Volumes, which is already a directory of the library `Manga`"}',
      ),
    )
    const { host, onChanged } = await shown()
    const picker = await openPicker(host)
    const box = picker.querySelector('input[name="path"]') as HTMLInputElement

    await typeInto(box, '/media/Manga/Volumes/x')
    await press(picker.querySelector('button[name="pick"]') as Element)

    expect(picker.querySelector('[role="alert"]')?.textContent).toBe(
      '/media/Manga/Volumes/x is inside /media/Manga/Volumes, which is already a directory of the library `Manga`',
    )
    expect(host.querySelector('.library-page > [role="alert"]')).toBeNull()
    expect(box.value).toBe('/media/Manga/Volumes/x')
    expect(onChanged).not.toHaveBeenCalled()
  })

  it('starts one scan of the library, and polls the library’s status until it ends', async () => {
    vi.mocked(api.startLibraryScan).mockResolvedValue({
      library_id: 7,
      status_url: '/api/library/libraries/7/scan',
    })
    const { host, onChanged } = await shown()
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      running: true,
      elapsed_secs: 2.4,
    })

    act(() => {
      button(host, 'scan-library').click()
      button(host, 'scan-library').click()
    })
    await flush()
    expect(api.startLibraryScan).toHaveBeenCalledTimes(1)
    expect(api.startLibraryScan).toHaveBeenCalledWith(7)
    expect(button(host, 'scan-library').disabled).toBe(true)
    expect(button(host, 'remove-library').disabled).toBe(true)
    expect(config.startScan).not.toHaveBeenCalled()

    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(
      host.querySelector('.library-page > .scanning')?.textContent,
    ).toContain('2s')
    expect(onChanged).not.toHaveBeenCalled()

    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.library-page > .scanning')).toBeNull()
    expect(button(host, 'scan-library').disabled).toBe(false)
    expect(onChanged).toHaveBeenCalledTimes(1)
    expect(onChanged).toHaveBeenCalledWith(true)
  })

  it('takes up a scan that was running when it was drawn', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      running: true,
      elapsed_secs: 41,
    })
    const { host } = await shown()
    expect(
      host.querySelector('.library-page > .scanning')?.textContent,
    ).toContain('41s')
    expect(button(host, 'scan-library').disabled).toBe(true)
  })

  it('reads a 409 as a scan in flight: the sentence, and a poll', async () => {
    vi.mocked(api.startLibraryScan).mockRejectedValue(
      new Error(
        '/api/library/libraries/7/scan: 409 {"error":"a scan of /media/Manga/Chapters is already running"}',
      ),
    )
    const { host } = await shown()
    await press(button(host, 'scan-library'))
    expect(
      host.querySelector('.library-page > [role="alert"]')?.textContent,
    ).toBe('a scan of /media/Manga/Chapters is already running')
    expect(host.querySelector('.library-page > .scanning')).not.toBeNull()
  })

  it('opens the removal dialog, and holds the scan button while it is open', async () => {
    vi.mocked(api.libraryRemovalCost).mockReturnValue(new Promise(() => {}))
    const { host } = await shown()
    await press(button(host, 'remove-library'))
    expect(
      host.querySelector('.remove-library')?.getAttribute('aria-label'),
    ).toBe('Remove the library Manga')
    expect(button(host, 'scan-library').disabled).toBe(true)
  })

  it('hands up the real answer of a removal, by name, and does not reload itself', async () => {
    const answer = (works: number, dry_run: boolean): config.RemovalAnswer => ({
      removed: { works, positions: 0, notes: 0, left_with_no_library: [] },
      derive: emptyDerive,
      dry_run,
    })
    const real = answer(11, false)
    vi.mocked(api.libraryRemovalCost).mockResolvedValue(answer(12, true))
    vi.mocked(api.removeLibrary).mockResolvedValue(real)
    const { host, onChanged, onRemoved } = await shown()
    await press(button(host, 'remove-library'))
    await press(host.querySelector('.remove-library .confirm') as Element)

    expect(api.removeLibrary).toHaveBeenCalledWith(7)
    expect(onRemoved).toHaveBeenCalledTimes(1)
    expect(onRemoved).toHaveBeenCalledWith('Manga', real)
    expect(onChanged).not.toHaveBeenCalled()
    expect(host.querySelector('.remove-library')).toBeNull()
  })

  it('holds Scan library and Remove while the scan is being started', async () => {
    vi.mocked(api.startLibraryScan).mockReturnValue(new Promise(() => {}))
    const { host } = await shown()
    await press(button(host, 'scan-library'))
    expect(button(host, 'scan-library').disabled).toBe(true)
    expect(button(host, 'remove-library').disabled).toBe(true)
  })

  it('draws a failed scan start as its sentence and no running scan', async () => {
    vi.mocked(api.startLibraryScan).mockRejectedValue(
      new Error('/api/library/libraries/7/scan: 500 the store is locked'),
    )
    const { host } = await shown()
    await press(button(host, 'scan-library'))
    expect(
      host.querySelector('.library-page > [role="alert"]')?.textContent,
    ).toContain('the store is locked')
    expect(host.querySelector('.library-page > .scanning')).toBeNull()
    expect(button(host, 'scan-library').disabled).toBe(false)
  })

  it('sends one rename for two submits, with Save disabled meanwhile', async () => {
    vi.mocked(api.renameLibrary).mockReturnValue(new Promise(() => {}))
    const { host } = await shown()
    await press(button(host, 'rename-library'))
    await typeInto(
      host.querySelector('input[name="library-name"]') as HTMLInputElement,
      'Comics',
    )
    const form = host.querySelector('form.rename-library') as Element
    await submit(form)
    await submit(form)
    expect(api.renameLibrary).toHaveBeenCalledTimes(1)
    const save = form.querySelector(
      'button[type="submit"]',
    ) as HTMLButtonElement
    expect(save.disabled).toBe(true)
    expect(save.textContent).toBe('Saving…')
  })

  it('lets a rename be saved while a scan is being started', async () => {
    vi.mocked(api.startLibraryScan).mockReturnValue(new Promise(() => {}))
    vi.mocked(api.renameLibrary).mockResolvedValue({ id: 7, name: 'Comics' })
    const { host, onChanged } = await shown()
    await press(button(host, 'scan-library'))
    await press(button(host, 'rename-library'))
    await typeInto(
      host.querySelector('input[name="library-name"]') as HTMLInputElement,
      'Comics',
    )
    await submit(host.querySelector('form.rename-library') as Element)
    expect(api.renameLibrary).toHaveBeenCalledWith(7, 'Comics')
    expect(onChanged).toHaveBeenCalled()
  })

  it('clears an earlier refusal once a directory is added', async () => {
    const added: config.Added = {
      root: dir(3, '/media/Manga/Extras'),
      watching: true,
      watch_reason: null,
      interval_secs: 86400,
    }
    vi.mocked(api.addDir)
      .mockRejectedValueOnce(
        new Error('/api/library/libraries/7/dirs: 400 {"error":"refused"}'),
      )
      .mockResolvedValueOnce(added)
    const { host } = await shown()
    const picker = await openPicker(host)
    const box = picker.querySelector('input[name="path"]') as HTMLInputElement
    await typeInto(box, '/media/typo')
    await press(picker.querySelector('button[name="pick"]') as Element)
    expect(picker.querySelector('[role="alert"]')).not.toBeNull()

    await typeInto(box, '/media/Manga/Extras')
    await press(picker.querySelector('button[name="pick"]') as Element)
    expect(picker.querySelector('[role="alert"]')).toBeNull()
  })

  it('survives two failed polls of a running scan and stops on the third', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValueOnce({
      ...idle(7),
      running: true,
      elapsed_secs: 1,
    })
    const { host } = await shown()
    vi.mocked(api.libraryScanStatus).mockRejectedValue(
      new Error('/api/library/libraries/7/scan: 500 the store is locked'),
    )
    await tick()
    await tick()
    expect(host.querySelector('.library-page > .scanning')).not.toBeNull()
    expect(host.querySelector('.library-page > [role="alert"]')).toBeNull()

    await tick()
    expect(host.querySelector('.library-page > .scanning')).toBeNull()
    expect(
      host.querySelector('.library-page > [role="alert"]')?.textContent,
    ).toContain('the store is locked')
  })
})

const started = {
  library_id: 7,
  status_url: '/api/library/libraries/7/scan',
}

const pageAlert = (host: HTMLElement) =>
  host.querySelector('.library-page > [role="alert"]')

const held = (...buttons: HTMLButtonElement[]) => buttons.map((b) => b.disabled)

/** What each row of the Directories table says under Last scan. */
const lastScans = (host: HTMLElement) =>
  [...host.querySelectorAll('.dir-table tbody tr')].map((row) =>
    cellValue(row.querySelectorAll('td')[2]),
  )

describe('LibraryPage, a library and its directories', () => {
  it('holds the library’s Scan library and Remove while a directory is scanning from its own page, and sends nothing from them', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 2))
    const { host } = await shown()

    expect(button(host, 'scan-library').disabled).toBe(true)
    expect(button(host, 'remove-library').disabled).toBe(true)
    await press(button(host, 'scan-library'))
    await press(button(host, 'remove-library'))
    expect(api.startLibraryScan).not.toHaveBeenCalled()
    expect(host.querySelector('.remove-library')).toBeNull()
    expect(api.removeLibrary).not.toHaveBeenCalled()
  })

  it('asks for the list again, quietly, when the library’s own scan ends', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      running: true,
      elapsed_secs: 3,
    })
    const { host, onChanged } = await shown()
    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    expect(host.querySelector('.library-page > .scanning')).toBeNull()
    expect(onChanged).toHaveBeenCalledTimes(1)
    expect(onChanged).toHaveBeenCalledWith(true)
  })

  it('frees the library once the directory’s scan has ended', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 2))
    const { host } = await shown()
    expect(button(host, 'scan-library').disabled).toBe(true)
    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    expect(button(host, 'scan-library').disabled).toBe(false)
    expect(button(host, 'remove-library').disabled).toBe(false)
  })

  it('draws a Directories table: a row per directory, its path a link to the directory’s page, how often it is scanned and when it last was', async () => {
    const { host } = await shown(
      manga({
        dirs: [
          dir(1, '/media/Manga/Volumes'),
          {
            ...dir(4, '/media/Manga/Chapters'),
            scan_interval_secs: null,
            last_scan_at: '2026-10-01 10:00:00',
            last_scan_status: 'failed',
          },
        ],
      }),
    )
    const table = host.querySelector('table.dir-table') as HTMLElement
    expect(table.previousElementSibling?.textContent).toBe('Directories')
    expect(
      [...table.querySelectorAll('thead th')].map((th) => th.textContent),
    ).toEqual(['Path', 'Scan every', 'Last scan'])
    // Per row, so a value drawn in the wrong row or under the wrong column
    // is seen.
    expect(
      [...table.querySelectorAll('tbody tr')].map((row) => ({
        links: [...row.querySelectorAll('a')].map((a) => [
          a.textContent,
          a.getAttribute('href'),
        ]),
        cells: [...row.querySelectorAll('td')].map(cellValue),
      })),
    ).toEqual([
      {
        links: [['/media/Manga/Volumes', '/config/libraries/7/dirs/1']],
        cells: ['/media/Manga/Volumes', '24h', 'Never'],
      },
      {
        links: [['/media/Manga/Chapters', '/config/libraries/7/dirs/4']],
        cells: [
          '/media/Manga/Chapters',
          'not set',
          '2026-10-01 10:00:00 · failed',
        ],
      },
    ])
  })

  it('says Scanning… in the table for each directory being scanned, and its last scan once that ends', async () => {
    vi.mocked(api.startLibraryScan).mockResolvedValue(started)
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 2))
    const { host } = await shown(
      manga({
        dirs: [
          dir(1, '/media/Manga/Volumes'),
          {
            ...dir(2, '/media/Manga/Chapters'),
            last_scan_at: '2026-10-01 10:00:00',
            last_scan_status: 'ok',
          },
        ],
      }),
    )
    expect(lastScans(host)).toEqual(['Never', 'Scanning…'])

    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    expect(lastScans(host)).toEqual(['Never', '2026-10-01 10:00:00'])

    // A scan of the library is a scan of every directory in it.
    vi.mocked(api.libraryScanStatus).mockResolvedValue(
      scanningIn(manga(), 1, 2),
    )
    await press(button(host, 'scan-library'))
    expect(lastScans(host)).toEqual(['Scanning…', 'Scanning…'])
  })

  it('draws one Add directory…, and the picker only once it is pressed', async () => {
    const { host } = await shown()
    expect(host.querySelectorAll('button[name="add-dir"]')).toHaveLength(1)
    expect(button(host, 'add-dir').textContent).toBe('Add directory…')
    expect(host.querySelector('input[name="path"]')).toBeNull()

    const picker = await openPicker(host)
    expect(picker.querySelectorAll('input[name="path"]')).toHaveLength(1)
    expect(picker.querySelector('button[name="pick"]')?.textContent).toBe(
      'Add directory',
    )
    expect(button(host, 'add-dir')).toBeNull()
    expect(document.activeElement).toBe(
      picker.querySelector('input[name="path"]'),
    )

    await press(button(host, 'cancel-add'))
    expect(host.querySelector('input[name="path"]')).toBeNull()
    expect(document.activeElement).toBe(button(host, 'add-dir'))
  })

  it('closes the picker once a directory is added, and the directory joins the table when the list is read again', async () => {
    const added: config.Added = {
      root: dir(3, '/media/Manga/Extras'),
      watching: true,
      watch_reason: null,
      interval_secs: 86400,
    }
    vi.mocked(api.addDir).mockResolvedValue(added)
    const before = manga()
    const after = manga({ dirs: [...before.dirs, added.root] })
    // The screen's reload, which hands the page the list read again.
    function Reloading() {
      const [library, setLibrary] = useState(before)
      return (
        <MemoryRouter initialEntries={['/config/libraries/7']}>
          <LibraryPage
            library={library}
            onAdded={vi.fn()}
            onChanged={() => setLibrary(after)}
            onRemoved={vi.fn()}
          />
        </MemoryRouter>
      )
    }
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(<Reloading />)
      await Promise.resolve()
    })
    if (!host) throw new Error('nothing mounted')
    const picker = await openPicker(host)
    await typeInto(
      picker.querySelector('input[name="path"]') as HTMLInputElement,
      '/media/Manga/Extras',
    )
    await press(picker.querySelector('button[name="pick"]') as Element)

    expect(host.querySelector('input[name="path"]')).toBeNull()
    expect(button(host, 'add-dir')).not.toBeNull()
    expect(
      [...host.querySelectorAll('.dir-table tbody a')].map((a) => [
        a.textContent,
        a.getAttribute('href'),
      ]),
    ).toEqual([
      ['/media/Manga/Volumes', '/config/libraries/7/dirs/1'],
      ['/media/Manga/Chapters', '/config/libraries/7/dirs/2'],
      ['/media/Manga/Extras', '/config/libraries/7/dirs/3'],
    ])
  })

  it('draws each directory’s report under “What the scan found” once a scan it watched ends', async () => {
    vi.mocked(api.startLibraryScan).mockResolvedValue(started)
    const finished: config.ScanOutcome = {
      scan: {
        root: '/media/Manga/Volumes',
        dirs: 2,
        files: 5,
        added: 5,
        updated: 0,
        gone: 0,
        refused: null,
        unreadable_dirs: [],
        excluded_dirs: [],
        excluded_files: 0,
        bad_exclude_patterns: [],
        forgotten_by_exclusion: 0,
      },
      derive: emptyDerive,
    }
    // Each directory's status holds a report from the start: none is drawn
    // until a scan this page watched has ended.
    vi.mocked(config.scanStatus).mockImplementation((id) =>
      Promise.resolve({
        root_id: id,
        running: false,
        report: id === 1 ? finished : { error: 'the scan panicked' },
        last_scan_at: '2026-10-01 10:00:00',
        last_scan_status: id === 1 ? 'ok' : 'failed',
      }),
    )
    const { host } = await shown()
    await flush()
    expect(host.querySelector('.library-scan-reports')).toBeNull()
    vi.mocked(api.libraryScanStatus).mockResolvedValue(
      scanningIn(manga(), 1, 2),
    )
    await press(button(host, 'scan-library'))
    await tick()
    await flush()
    expect(host.querySelector('.library-scan-reports')).toBeNull()

    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    await flush()

    const section = host.querySelector('.library-scan-reports') as HTMLElement
    expect(section).not.toBeNull()
    expect(section.querySelector('h3')?.textContent).toBe('What the scan found')
    const reports = [...section.querySelectorAll('.dir-report')]
    // Per report, so a report drawn under the wrong directory is seen.
    expect(
      reports.map((one) => ({
        path: one.querySelector('h4')?.textContent,
        walk: one.querySelector('.scan-report .walk') !== null,
        headings: [...one.querySelectorAll('.report-heading')].map(
          (h) => h.tagName,
        ),
        alert: one.querySelector('.scan-report [role="alert"]')?.textContent,
      })),
    ).toEqual([
      {
        path: '/media/Manga/Volumes',
        walk: true,
        headings: ['H5', 'H5'],
        alert: undefined,
      },
      {
        path: '/media/Manga/Chapters',
        walk: false,
        headings: [],
        alert: 'the scan panicked',
      },
    ])
  })
})

/** Each directory's own status, holding a report that walked its directory. */
const reporting = () =>
  vi.mocked(config.scanStatus).mockImplementation((id) =>
    Promise.resolve({
      root_id: id,
      running: false,
      report: {
        scan: {
          root: `/dir/${id}`,
          dirs: 1,
          files: 1,
          added: 1,
          updated: 0,
          gone: 0,
          refused: null,
          unreadable_dirs: [],
          excluded_dirs: [],
          excluded_files: 0,
          bad_exclude_patterns: [],
          forgotten_by_exclusion: 0,
        },
        derive: emptyDerive,
      },
      last_scan_at: '2026-10-01 10:00:00',
      last_scan_status: 'ok',
    }),
  )

/** The paths the reports of the last scan are drawn under, in order. */
const reportedPaths = (host: HTMLElement) =>
  [...host.querySelectorAll('.library-scan-reports .dir-report h4')].map(
    (h) => h.textContent,
  )

describe('LibraryPage, whose reports a scan draws', () => {
  it('draws no report for a directory that joined the library while the scan ran, though its status holds one', async () => {
    vi.mocked(api.startLibraryScan).mockResolvedValue(started)
    reporting()
    const { host, redraw } = await shown()
    vi.mocked(api.libraryScanStatus).mockResolvedValue(
      scanningIn(manga(), 1, 2),
    )
    await press(button(host, 'scan-library'))
    await tick()
    await redraw(
      manga({
        dirs: [
          dir(1, '/media/Manga/Volumes'),
          dir(2, '/media/Manga/Chapters'),
          dir(3, '/media/Manga/Extras'),
        ],
      }),
    )
    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    await flush()
    expect(reportedPaths(host)).toEqual([
      '/media/Manga/Volumes',
      '/media/Manga/Chapters',
    ])
  })

  it('draws only the report of the directory whose own scan it took up on being drawn', async () => {
    reporting()
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 1))
    const { host } = await shown()
    await tick()
    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    await flush()
    expect(reportedPaths(host)).toEqual(['/media/Manga/Volumes'])
  })

  it('draws the reports of the directories a poll named scanning after a 409', async () => {
    vi.mocked(api.startLibraryScan).mockRejectedValue(
      new Error(
        '/api/library/libraries/7/scan: 409 {"error":"a scan of /media/Manga/Chapters is already running"}',
      ),
    )
    reporting()
    const { host } = await shown()
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 2))
    await press(button(host, 'scan-library'))
    await tick()
    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    await flush()
    expect(reportedPaths(host)).toEqual(['/media/Manga/Chapters'])
  })

  it('forgets the directories an earlier scan held when the next is pressed', async () => {
    vi.mocked(api.startLibraryScan).mockResolvedValueOnce(started)
    reporting()
    const { host } = await shown()
    vi.mocked(api.libraryScanStatus).mockResolvedValue(
      scanningIn(manga(), 1, 2),
    )
    await press(button(host, 'scan-library'))
    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    await flush()
    expect(reportedPaths(host)).toHaveLength(2)

    vi.mocked(api.startLibraryScan).mockRejectedValueOnce(
      new Error(
        '/api/library/libraries/7/scan: 409 {"error":"a scan of /media/Manga/Chapters is already running"}',
      ),
    )
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 2))
    await press(button(host, 'scan-library'))
    await tick()
    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    await flush()
    expect(reportedPaths(host)).toEqual(['/media/Manga/Chapters'])
  })
})

describe('LibraryPage, what it says went wrong', () => {
  it('keeps a 409’s sentence only until the scan it named has ended', async () => {
    vi.mocked(api.startLibraryScan).mockRejectedValue(
      new Error(
        '/api/library/libraries/7/scan: 409 {"error":"a scan of /media/Manga/Chapters is already running"}',
      ),
    )
    const { host } = await shown()
    await press(button(host, 'scan-library'))
    expect(pageAlert(host)?.textContent).toBe(
      'a scan of /media/Manga/Chapters is already running',
    )
    expect(host.querySelector('.library-page > .scanning')).not.toBeNull()

    await tick()
    expect(pageAlert(host)).toBeNull()
    expect(host.querySelector('.library-page > .scanning')).toBeNull()
    expect(button(host, 'scan-library').disabled).toBe(false)
  })

  it('keeps a 409’s sentence through a poll that finds the scan still running', async () => {
    vi.mocked(api.startLibraryScan).mockRejectedValue(
      new Error(
        '/api/library/libraries/7/scan: 409 {"error":"a scan of /media/Manga/Chapters is already running"}',
      ),
    )
    const { host } = await shown()
    await press(button(host, 'scan-library'))
    const named = 'a scan of /media/Manga/Chapters is already running'
    expect(pageAlert(host)?.textContent).toBe(named)

    vi.mocked(api.libraryScanStatus).mockResolvedValueOnce({
      ...idle(7),
      running: true,
      elapsed_secs: 4,
    })
    await tick()
    expect(pageAlert(host)?.textContent).toBe(named)
    expect(host.querySelector('.library-page > .scanning')).not.toBeNull()

    await tick()
    expect(pageAlert(host)).toBeNull()
    expect(host.querySelector('.library-page > .scanning')).toBeNull()
  })

  it('drops a 409’s sentence when the poll is abandoned, since nothing can keep it true', async () => {
    vi.mocked(api.startLibraryScan).mockRejectedValue(
      new Error(
        '/api/library/libraries/7/scan: 409 {"error":"a scan of /media/Manga/Chapters is already running"}',
      ),
    )
    const { host } = await shown()
    vi.mocked(api.libraryScanStatus).mockRejectedValue(
      new Error('/api/library/libraries/7/scan: 502 bad gateway'),
    )
    await press(button(host, 'scan-library'))
    expect(pageAlert(host)?.textContent).toContain('already running')
    await tick()
    await tick()
    expect(pageAlert(host)?.textContent).toContain('already running')
    await tick()
    expect(
      [...host.querySelectorAll('.library-page > [role="alert"]')].map(
        (a) => a.textContent,
      ),
    ).toEqual(['bad gateway'])
    expect(button(host, 'scan-library').disabled).toBe(false)
  })

  it('draws a failed scan start as the server’s sentence', async () => {
    vi.mocked(api.startLibraryScan).mockRejectedValueOnce(
      new Error('/api/library/libraries/7/scan: 500 {"error":"no disk"}'),
    )
    const { host } = await shown()
    await press(button(host, 'scan-library'))
    expect(pageAlert(host)?.textContent).toBe('no disk')
  })

  it('draws a rename refusal in the rename form, until the name is edited or the form is cancelled', async () => {
    vi.mocked(api.renameLibrary).mockRejectedValue(
      new Error('/api/library/libraries/7: 400 {"error":"taken"}'),
    )
    const { host } = await shown()
    await press(button(host, 'rename-library'))
    const form = () => host.querySelector('form.rename-library') as HTMLElement
    const box = form().querySelector('input') as HTMLInputElement
    await typeInto(box, 'Comics')
    await submit(form())
    expect(form().querySelector('[role="alert"]')?.textContent).toBe('taken')

    await typeInto(box, 'Comics Two')
    expect(form().querySelector('[role="alert"]')).toBeNull()

    await submit(form())
    expect(form().querySelector('[role="alert"]')).not.toBeNull()
    await press(
      [...form().querySelectorAll('button')].find(
        (b) => b.textContent === 'Cancel',
      ) as Element,
    )
    expect(host.querySelector('form.rename-library')).toBeNull()
    await press(button(host, 'rename-library'))
    expect(host.querySelector('form.rename-library [role="alert"]')).toBeNull()
  })

  it('holds no rename refusal while the next attempt is in flight', async () => {
    vi.mocked(api.renameLibrary)
      .mockRejectedValueOnce(
        new Error('/api/library/libraries/7: 400 {"error":"taken"}'),
      )
      .mockReturnValueOnce(new Promise(() => {}))
    const { host } = await shown()
    await press(button(host, 'rename-library'))
    const form = () => host.querySelector('form.rename-library') as HTMLElement
    await typeInto(form().querySelector('input') as HTMLInputElement, 'Comics')
    await submit(form())
    expect(form().querySelector('[role="alert"]')).not.toBeNull()
    await submit(form())
    expect(form().querySelector('[role="alert"]')).toBeNull()
  })

  it('draws an add refusal beside the picker, and holds none while the next attempt is in flight', async () => {
    vi.mocked(api.addDir)
      .mockRejectedValueOnce(
        new Error('/api/library/libraries/7/dirs: 400 {"error":"nope"}'),
      )
      .mockReturnValueOnce(new Promise(() => {}))
    const { host } = await shown()
    const picker = await openPicker(host)
    const box = picker.querySelector('input[name="path"]') as HTMLInputElement
    const pick = picker.querySelector('button[name="pick"]') as Element
    await typeInto(box, '/media/a')
    await press(pick)
    expect(picker.querySelector('[role="alert"]')?.textContent).toBe('nope')

    // The same refusal again has to be a new alert, so the old one is gone
    // while the second attempt is out.
    await press(pick)
    expect(picker.querySelector('[role="alert"]')).toBeNull()
  })

  it('clears an add refusal when the path is edited', async () => {
    vi.mocked(api.addDir).mockRejectedValue(
      new Error('/api/library/libraries/7/dirs: 400 {"error":"nope"}'),
    )
    const { host } = await shown()
    const picker = await openPicker(host)
    const box = picker.querySelector('input[name="path"]') as HTMLInputElement
    await typeInto(box, '/media/a')
    await press(picker.querySelector('button[name="pick"]') as Element)
    expect(picker.querySelector('[role="alert"]')).not.toBeNull()
    await typeInto(box, '/media/ab')
    expect(picker.querySelector('[role="alert"]')).toBeNull()
  })

  it('draws the poll’s failure as the server’s sentence', async () => {
    vi.mocked(api.startLibraryScan).mockResolvedValue(started)
    const { host } = await shown()
    vi.mocked(api.libraryScanStatus).mockRejectedValue(
      new Error(
        '/api/library/libraries/7/scan: 500 {"error":"the registry is unreadable"}',
      ),
    )
    await press(button(host, 'scan-library'))
    await tick()
    await tick()
    await tick()
    expect(pageAlert(host)?.textContent).toBe('the registry is unreadable')
  })

  it('counts only consecutive missed polls', async () => {
    vi.mocked(api.startLibraryScan).mockResolvedValue(started)
    const running = { ...idle(7), running: true, elapsed_secs: 1 }
    const { host } = await shown()
    const answers = ['miss', 'miss', 'ok', 'miss', 'miss', 'ok'] as const
    for (const a of answers) {
      if (a === 'miss')
        vi.mocked(api.libraryScanStatus).mockRejectedValueOnce(new Error('502'))
      else vi.mocked(api.libraryScanStatus).mockResolvedValueOnce(running)
    }
    vi.mocked(api.libraryScanStatus).mockResolvedValue(running)
    await press(button(host, 'scan-library'))
    for (let i = 0; i < answers.length; i++) await tick()
    // Four misses in six polls and never three together: still watching.
    expect(host.querySelector('.library-page > .scanning')).not.toBeNull()
    expect(pageAlert(host)).toBeNull()
  })

  it('starts the count of missed polls afresh with each scan', async () => {
    vi.mocked(api.startLibraryScan).mockResolvedValue(started)
    vi.mocked(api.libraryScanStatus).mockRejectedValue(new Error('502'))
    const { host } = await shown()
    await press(button(host, 'scan-library'))
    await tick()
    await tick()
    await tick()
    // Three misses ended that poll. A second scan whose first poll misses
    // once is still being watched.
    expect(host.querySelector('.library-page > .scanning')).toBeNull()
    await press(button(host, 'scan-library'))
    expect(host.querySelector('.library-page > .scanning')).not.toBeNull()
    await tick()
    expect(host.querySelector('.library-page > .scanning')).not.toBeNull()
  })

  it('stops polling when it is unmounted mid-scan', async () => {
    vi.mocked(api.startLibraryScan).mockResolvedValue(started)
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      running: true,
      elapsed_secs: 1,
    })
    const { host } = await shown()
    await press(button(host, 'scan-library'))
    vi.mocked(api.libraryScanStatus).mockClear()
    act(() => rerender(<div />))
    expect(host.querySelector('.library-page')).toBeNull()
    await tick()
    await tick()
    expect(api.libraryScanStatus).not.toHaveBeenCalled()
  })

  it('closes the library’s removal dialog with Cancel, and the rename form with Cancel', async () => {
    vi.mocked(api.libraryRemovalCost).mockResolvedValue(counted(12))
    const { host } = await shown()
    await press(button(host, 'remove-library'))
    await press(
      [...host.querySelectorAll('.remove-library button')].find(
        (b) => b.textContent === 'Cancel',
      ) as Element,
    )
    expect(host.querySelector('.remove-library')).toBeNull()

    await press(button(host, 'rename-library'))
    await press(
      [...host.querySelectorAll('form.rename-library button')].find(
        (b) => b.textContent === 'Cancel',
      ) as Element,
    )
    expect(host.querySelector('form.rename-library')).toBeNull()
  })
})

describe('LibraryPage, scans and removals of a library and its directories exclude each other', () => {
  it('holds the library’s removal when a directory is found scanning under its open dialog, and asks its cost again when that ends', async () => {
    const late = deferred<api.LibraryScanStatus>()
    vi.mocked(api.libraryScanStatus).mockReturnValueOnce(late.promise)
    vi.mocked(api.libraryRemovalCost).mockResolvedValue(counted(12))
    const { host } = await shown()
    await press(button(host, 'remove-library'))
    const confirm = () =>
      host.querySelector('.remove-library .confirm') as HTMLButtonElement
    expect(confirm().disabled).toBe(false)

    await flush(() => late.resolve(scanningIn(manga(), 2)))
    expect(confirm().disabled).toBe(true)
    confirm().click()
    await flush()
    expect(api.removeLibrary).not.toHaveBeenCalled()
    expect(api.libraryRemovalCost).toHaveBeenCalledTimes(1)

    // The scan ends; what removing would cost is asked for again, and no
    // confirm stands over the figures from before it.
    const second = deferred<config.RemovalAnswer>()
    vi.mocked(api.libraryRemovalCost).mockReturnValueOnce(second.promise)
    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    expect(api.libraryRemovalCost).toHaveBeenCalledTimes(2)
    expect(host.querySelector('.remove-library .counts')).toBeNull()
    expect(host.querySelector('.remove-library .confirm')).toBeNull()

    await flush(() => second.resolve(counted(9)))
    expect(
      host.querySelector('.remove-library .counts')?.textContent,
    ).toContain('9 works')
    expect(confirm().disabled).toBe(false)
  })

  it('holds the library’s removal when the page’s own first read finds a scan under its open dialog', async () => {
    const late = deferred<api.LibraryScanStatus>()
    vi.mocked(api.libraryScanStatus).mockReturnValueOnce(late.promise)
    vi.mocked(api.libraryRemovalCost).mockResolvedValue(counted(12))
    const { host } = await shown()
    await press(button(host, 'remove-library'))
    const confirm = () =>
      host.querySelector('.remove-library .confirm') as HTMLButtonElement
    expect(confirm().disabled).toBe(false)

    await flush(() =>
      late.resolve({ ...idle(7), running: true, elapsed_secs: 5 }),
    )
    expect(confirm().disabled).toBe(true)
    confirm().click()
    await flush()
    expect(api.removeLibrary).not.toHaveBeenCalled()
  })

  it('holds Rename… and Add directory while the library’s removal is being decided, and sends nothing from them', async () => {
    vi.mocked(api.libraryRemovalCost).mockResolvedValue(counted(12))
    const { host } = await shown()
    const picker = await openPicker(host)
    const box = picker.querySelector('input[name="path"]') as HTMLInputElement
    const add = picker.querySelector('button[name="pick"]') as HTMLButtonElement
    await press(button(host, 'rename-library'))
    await typeInto(
      host.querySelector('form.rename-library input') as HTMLInputElement,
      'Comics',
    )
    await typeInto(box, '/media/Manga/Extras')
    await press(button(host, 'remove-library'))

    const save = host.querySelector(
      'form.rename-library button[type="submit"]',
    ) as HTMLButtonElement
    expect(held(save, add)).toEqual([true, true])
    await press(save)
    await press(add)
    await flush(() => {
      box.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }),
      )
    })
    expect(api.renameLibrary).not.toHaveBeenCalled()
    expect(api.addDir).not.toHaveBeenCalled()

    // Cancelled, both are live again.
    await press(
      [...host.querySelectorAll('.remove-library button')].find(
        (b) => b.textContent === 'Cancel',
      ) as Element,
    )
    expect(held(save, add)).toEqual([false, false])
  })

  it('holds Rename… and Add directory while the library’s removal is on its way', async () => {
    vi.mocked(api.libraryRemovalCost).mockResolvedValue(counted(12))
    vi.mocked(api.removeLibrary).mockReturnValue(new Promise(() => {}))
    const { host } = await shown()
    await press(button(host, 'remove-library'))
    expect(button(host, 'rename-library').disabled).toBe(true)
    await press(button(host, 'rename-library'))
    expect(host.querySelector('form.rename-library')).toBeNull()

    await press(host.querySelector('.remove-library .confirm') as Element)
    expect(api.removeLibrary).toHaveBeenCalledWith(7)
    expect(button(host, 'rename-library').disabled).toBe(true)
    expect(button(host, 'add-dir').disabled).toBe(true)
    await press(button(host, 'add-dir'))
    expect(host.querySelector('input[name="path"]')).toBeNull()
    expect(api.addDir).not.toHaveBeenCalled()
  })

  it('draws the Directories table’s paths without links while the library’s removal is open or on its way, and with them once it is cancelled', async () => {
    vi.mocked(api.libraryRemovalCost).mockResolvedValue(counted(12))
    const removal = deferred<config.RemovalAnswer>()
    vi.mocked(api.removeLibrary).mockReturnValue(removal.promise)
    const { host } = await shown()
    const links = () =>
      [...host.querySelectorAll('.dir-table tbody a')].map((a) =>
        a.getAttribute('href'),
      )
    const paths = () =>
      [...host.querySelectorAll('.dir-table tbody code')].map(
        (c) => c.textContent,
      )
    expect(links()).toEqual([
      '/config/libraries/7/dirs/1',
      '/config/libraries/7/dirs/2',
    ])

    await press(button(host, 'remove-library'))
    expect(links()).toEqual([])
    expect(paths()).toEqual(['/media/Manga/Volumes', '/media/Manga/Chapters'])

    await press(host.querySelector('.remove-library .confirm') as Element)
    expect(api.removeLibrary).toHaveBeenCalledWith(7)
    expect(links()).toEqual([])
    expect(paths()).toEqual(['/media/Manga/Volumes', '/media/Manga/Chapters'])

    // Refused, the dialog stays open, and so the links stay away until it
    // is cancelled.
    await flush(() => removal.reject(new Error('500 the store is locked')))
    expect(links()).toEqual([])
    await press(
      [...host.querySelectorAll('.remove-library button')].find(
        (b) => b.textContent === 'Cancel',
      ) as Element,
    )
    expect(links()).toEqual([
      '/config/libraries/7/dirs/1',
      '/config/libraries/7/dirs/2',
    ])
  })

  it('does not hold Rename, Save or Add directory for a scan', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 1))
    vi.mocked(api.renameLibrary).mockResolvedValue({ id: 7, name: 'Comics' })
    vi.mocked(api.addDir).mockResolvedValue({
      root: dir(3, '/media/Manga/Extras'),
      watching: true,
      watch_reason: null,
      interval_secs: 86400,
    })
    const { host } = await shown()
    expect(host.querySelector('.library-page > .scanning')).not.toBeNull()

    await press(button(host, 'rename-library'))
    await typeInto(
      host.querySelector('form.rename-library input') as HTMLInputElement,
      'Comics',
    )
    const save = host.querySelector(
      'form.rename-library button[type="submit"]',
    ) as HTMLButtonElement
    expect(save.disabled).toBe(false)
    await press(save)
    expect(api.renameLibrary).toHaveBeenCalledWith(7, 'Comics')

    expect(button(host, 'add-dir').disabled).toBe(false)
    const picker = await openPicker(host)
    await typeInto(
      picker.querySelector('input[name="path"]') as HTMLInputElement,
      '/media/Manga/Extras',
    )
    await press(picker.querySelector('button[name="pick"]') as Element)
    expect(api.addDir).toHaveBeenCalledWith(7, '/media/Manga/Extras')
  })
})

describe('LibraryPage, what a refusal outlasts', () => {
  const refusedAdd = async (host: HTMLElement) => {
    vi.mocked(api.addDir).mockRejectedValue(
      new Error('/api/library/libraries/7/dirs: 400 {"error":"nope"}'),
    )
    const picker = await openPicker(host)
    await typeInto(
      picker.querySelector('input[name="path"]') as HTMLInputElement,
      '/media/typo',
    )
    await press(picker.querySelector('button[name="pick"]') as Element)
    return picker
  }

  it('keeps an add’s refusal through a scan of the library and a rename', async () => {
    vi.mocked(api.startLibraryScan).mockResolvedValue(started)
    vi.mocked(api.renameLibrary).mockResolvedValue({ id: 7, name: 'Comics' })
    const { host } = await shown()
    const picker = await refusedAdd(host)
    expect(picker.querySelector('[role="alert"]')?.textContent).toBe('nope')

    await press(button(host, 'scan-library'))
    expect(picker.querySelector('[role="alert"]')?.textContent).toBe('nope')

    await press(button(host, 'rename-library'))
    await typeInto(
      host.querySelector('form.rename-library input') as HTMLInputElement,
      'Comics',
    )
    await submit(host.querySelector('form.rename-library') as Element)
    expect(api.renameLibrary).toHaveBeenCalled()
    expect(picker.querySelector('[role="alert"]')?.textContent).toBe('nope')
  })

  it('keeps a rename’s refusal through a scan of the library', async () => {
    vi.mocked(api.startLibraryScan).mockResolvedValue(started)
    vi.mocked(api.renameLibrary).mockRejectedValue(
      new Error('/api/library/libraries/7: 400 {"error":"taken"}'),
    )
    const { host } = await shown()
    await press(button(host, 'rename-library'))
    await typeInto(
      host.querySelector('form.rename-library input') as HTMLInputElement,
      'Comics',
    )
    await submit(host.querySelector('form.rename-library') as Element)
    const alert = () =>
      host.querySelector('form.rename-library [role="alert"]')?.textContent
    expect(alert()).toBe('taken')

    await press(button(host, 'scan-library'))
    expect(api.startLibraryScan).toHaveBeenCalled()
    expect(alert()).toBe('taken')
  })

  it('clears an add’s refusal when browsing writes another path into the box', async () => {
    vi.mocked(api.listDirs).mockImplementation((path) =>
      Promise.resolve(
        path === undefined
          ? {
              path: null,
              parent: null,
              dirs: [{ name: '/media', path: '/media' }],
            }
          : { path, parent: null, dirs: [] },
      ),
    )
    const { host } = await shown()
    const picker = await refusedAdd(host)
    expect(picker.querySelector('[role="alert"]')?.textContent).toBe('nope')

    await press(picker.querySelector('button[name="browse"]') as Element)
    expect(picker.querySelector('[role="alert"]')?.textContent).toBe('nope')
    await press(picker.querySelector('.dir-list button') as Element)
    expect(
      (picker.querySelector('input[name="path"]') as HTMLInputElement).value,
    ).toBe('/media')
    expect(picker.querySelector('[role="alert"]')).toBeNull()
  })
})
