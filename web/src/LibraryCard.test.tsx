import { act } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from './config'
import * as api from './libraries'
import { LibraryCard } from './LibraryCard'
import { deferred, flush } from './testing/async'
import { emptyDerive } from './testing/derive'
import { press, submit, typeInto } from './testing/input'
import { mount, rerender } from './testing/mount'

vi.mock('./config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./config')>()),
  patchRoot: vi.fn(),
  removalCost: vi.fn(),
  removeRoot: vi.fn(),
  scanStatus: vi.fn(),
  startScan: vi.fn(),
}))
vi.mock('./libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./libraries')>()),
  addFolder: vi.fn(),
  libraryRemovalCost: vi.fn(),
  libraryScanStatus: vi.fn(),
  listFolders: vi.fn(),
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

const manga = (over: Partial<api.Library> = {}): api.Library => ({
  id: 7,
  name: 'Manga',
  works: 12,
  books: 0,
  folders: [
    folder(1, '/media/Manga/Volumes'),
    folder(2, '/media/Manga/Chapters'),
  ],
  ...over,
})

const idle = (id: number): api.LibraryScanStatus => ({
  library_id: id,
  running: false,
  elapsed_secs: null,
  folders: [],
})

async function shown(library = manga()) {
  const onAdded = vi.fn()
  const onChanged = vi.fn()
  const onRemoved = vi.fn()
  const onFolderRemoved = vi.fn()
  const onFolderMoved = vi.fn()
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <ul>
        <LibraryCard
          library={library}
          others={[{ id: 8, name: 'Comics' }]}
          probes={{}}
          onAdded={onAdded}
          onChanged={onChanged}
          onRemoved={onRemoved}
          onFolderRemoved={onFolderRemoved}
          onFolderMoved={onFolderMoved}
        />
      </ul>,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return {
    host,
    onAdded,
    onChanged,
    onRemoved,
    onFolderRemoved,
    onFolderMoved,
  }
}

const button = (host: HTMLElement, name: string) =>
  host.querySelector(`button[name="${name}"]`) as HTMLButtonElement

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

describe('LibraryCard', () => {
  it('words the library’s button Scan library and each folder’s Scan folder', async () => {
    const { host } = await shown()
    expect(button(host, 'scan-library').textContent).toBe('Scan library')
    expect(
      [0, 1].map((row) =>
        [...host.querySelectorAll('.root-row')[row].querySelectorAll('button')]
          .map((b) => b.textContent)
          .filter((text) => text?.startsWith('Scan')),
      ),
    ).toEqual([['Scan folder'], ['Scan folder']])
  })

  it('says a scan is running in the library, not that it is scanning the library’s folders', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      running: true,
      elapsed_secs: 9,
    })
    const { host } = await shown()
    expect(
      host.querySelector('.library-card > .scanning')?.textContent,
    ).toMatch(/^A scan is running in this library… 9s$/)
  })

  it('names the library, what it holds, and a row per folder', async () => {
    const { host } = await shown()
    expect(host.querySelector('.library-card > h3')?.textContent).toBe('Manga')
    expect(host.querySelector('.holds')?.textContent).toBe(
      '12 works in 2 folders',
    )
    expect(
      [...host.querySelectorAll('.root-row code')].map((c) => c.textContent),
    ).toEqual(['/media/Manga/Volumes', '/media/Manga/Chapters'])
    expect(host.querySelector('.no-readable')).toBeNull()
  })

  it('says so when it holds books and no work, and only then', async () => {
    const books = await shown(
      manga({ works: 0, books: 3, folders: [folder(1, '/media/Books')] }),
    )
    expect(books.host.querySelector('.holds')?.textContent).toBe(
      '0 works in 1 folder',
    )
    expect(books.host.querySelector('.no-readable')?.textContent).toContain(
      'No readable files yet: its scans found 3 EPUB or PDF files',
    )
    const mixed = await shown(manga({ works: 2, books: 3 }))
    expect(mixed.host.querySelector('.no-readable')).toBeNull()
    const empty = await shown(manga({ works: 0, books: 0 }))
    expect(empty.host.querySelector('.no-readable')).toBeNull()
  })

  it('tells a folder it is the last one, and no folder of two', async () => {
    vi.mocked(config.removalCost).mockResolvedValue({
      removed: { works: 0, positions: 0, notes: 0, left_with_no_library: [] },
      derive: emptyDerive,
      dry_run: true,
    })
    const removeFolder = (host: HTMLElement) =>
      [...host.querySelectorAll('.root-row button')].find(
        (b) => b.textContent === 'Remove…',
      ) as Element

    const one = await shown(
      manga({ folders: [folder(1, '/media/Manga/Volumes')] }),
    )
    await press(removeFolder(one.host))
    expect(one.host.querySelector('.last-folder')?.textContent).toContain(
      'the only folder of the library Manga',
    )

    const two = await shown()
    await press(removeFolder(two.host))
    expect(two.host.querySelector('.remove-root .counts')).not.toBeNull()
    expect(two.host.querySelector('.last-folder')).toBeNull()
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
    expect(host.querySelector('.library-card > [role="alert"]')).toBeNull()
    expect(host.querySelector('form.rename-library')).not.toBeNull()
    expect(onChanged).not.toHaveBeenCalled()
  })

  it('adds a folder to this library, hands its probe up, and reloads', async () => {
    const added: config.Added = {
      root: folder(3, '/media/Manga/Extras'),
      watching: true,
      watch_reason: null,
      interval_secs: 86400,
    }
    vi.mocked(api.addFolder).mockResolvedValue(added)
    const { host, onAdded, onChanged } = await shown()
    const picker = host.querySelector(
      '.library-card > .add-root',
    ) as HTMLElement

    await typeInto(
      picker.querySelector('input[name="path"]') as HTMLInputElement,
      '/media/Manga/Extras',
    )
    await press(picker.querySelector('button[name="pick"]') as Element)

    expect(api.addFolder).toHaveBeenCalledWith(7, '/media/Manga/Extras')
    expect(onAdded).toHaveBeenCalledWith([added])
    expect(onChanged).toHaveBeenCalled()
  })

  it("draws a refused folder's sentence and keeps what was typed", async () => {
    vi.mocked(api.addFolder).mockRejectedValue(
      new Error(
        '/api/library/libraries/7/folders: 400 {"error":"/media/Manga/Volumes/x is inside /media/Manga/Volumes, a folder of the library `Manga`"}',
      ),
    )
    const { host, onChanged } = await shown()
    const picker = host.querySelector(
      '.library-card > .add-root',
    ) as HTMLElement
    const box = picker.querySelector('input[name="path"]') as HTMLInputElement

    await typeInto(box, '/media/Manga/Volumes/x')
    await press(picker.querySelector('button[name="pick"]') as Element)

    expect(picker.querySelector('[role="alert"]')?.textContent).toBe(
      '/media/Manga/Volumes/x is inside /media/Manga/Volumes, a folder of the library `Manga`',
    )
    expect(host.querySelector('.library-card > [role="alert"]')).toBeNull()
    expect(box.value).toBe('/media/Manga/Volumes/x')
    expect(onChanged).not.toHaveBeenCalled()
  })

  it('starts one scan of the library, and its folder rows poll for themselves', async () => {
    vi.mocked(api.startLibraryScan).mockResolvedValue({
      library_id: 7,
      status_url: '/api/library/libraries/7/scan',
    })
    const { host, onChanged } = await shown()
    vi.mocked(config.scanStatus).mockClear()
    // Each folder's own status says running for as long as this test runs,
    // so no row reports a finished scan: the reload asserted at the end is
    // the card's.
    vi.mocked(config.scanStatus).mockImplementation((id) =>
      Promise.resolve({
        root_id: id,
        running: true,
        elapsed_secs: 2.4,
        report: null,
        last_scan_at: null,
        last_scan_status: null,
      }),
    )
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
      host.querySelector('.library-card > .scanning')?.textContent,
    ).toContain('2s')
    expect(
      vi
        .mocked(config.scanStatus)
        .mock.calls.map(([id]) => id)
        .sort(),
    ).toEqual([1, 2])
    expect(onChanged).not.toHaveBeenCalled()

    // The library's scan ends, and with it each folder's: a folder that went
    // on saying it was scanning would hold the card, as it should.
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
    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.library-card > .scanning')).toBeNull()
    expect(button(host, 'scan-library').disabled).toBe(false)
    // One reload from the card's poll, and one from each folder's.
    expect(onChanged).toHaveBeenCalledTimes(3)
  })

  it('takes up a scan that was running when it was drawn', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      running: true,
      elapsed_secs: 41,
    })
    const { host } = await shown()
    expect(
      host.querySelector('.library-card > .scanning')?.textContent,
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
      host.querySelector('.library-card > [role="alert"]')?.textContent,
    ).toBe('a scan of /media/Manga/Chapters is already running')
    expect(host.querySelector('.library-card > .scanning')).not.toBeNull()
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
      host.querySelector('.library-card > [role="alert"]')?.textContent,
    ).toContain('the store is locked')
    expect(host.querySelector('.library-card > .scanning')).toBeNull()
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

  it('clears an earlier refusal once a folder is added', async () => {
    const added: config.Added = {
      root: folder(3, '/media/Manga/Extras'),
      watching: true,
      watch_reason: null,
      interval_secs: 86400,
    }
    vi.mocked(api.addFolder)
      .mockRejectedValueOnce(
        new Error('/api/library/libraries/7/folders: 400 {"error":"refused"}'),
      )
      .mockResolvedValueOnce(added)
    const { host } = await shown()
    const picker = host.querySelector(
      '.library-card > .add-root',
    ) as HTMLElement
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
    const tick = () =>
      act(async () => {
        vi.advanceTimersByTime(config.SCAN_POLL_MS)
        await Promise.resolve()
        await Promise.resolve()
      })
    await tick()
    await tick()
    expect(host.querySelector('.library-card > .scanning')).not.toBeNull()
    expect(host.querySelector('.library-card > [role="alert"]')).toBeNull()

    await tick()
    expect(host.querySelector('.library-card > .scanning')).toBeNull()
    expect(
      host.querySelector('.library-card > [role="alert"]')?.textContent,
    ).toContain('the store is locked')
  })
})

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

const rowButton = (host: HTMLElement, row: number, text: string) =>
  [...host.querySelectorAll('.root-row')[row].querySelectorAll('button')].find(
    (b) => b.textContent === text,
  ) as HTMLButtonElement

const tick = () =>
  act(async () => {
    vi.advanceTimersByTime(config.SCAN_POLL_MS)
    await Promise.resolve()
    await Promise.resolve()
  })

const counted = (works = 5): config.RemovalAnswer => ({
  removed: { works, positions: 2, notes: 0, left_with_no_library: [] },
  derive: emptyDerive,
  dry_run: true,
})

const started = {
  library_id: 7,
  status_url: '/api/library/libraries/7/scan',
}

const cardAlert = (host: HTMLElement) =>
  host.querySelector('.library-card > [role="alert"]')

describe('LibraryCard, a library and its folders', () => {
  it('holds the library’s Scan library and Remove while a folder’s removal is being confirmed', async () => {
    vi.mocked(config.removalCost).mockResolvedValue(counted())
    const { host } = await shown()
    await press(rowButton(host, 0, 'Remove…'))
    expect(host.querySelector('.remove-root .confirm')).not.toBeNull()

    expect(button(host, 'scan-library').disabled).toBe(true)
    expect(button(host, 'remove-library').disabled).toBe(true)
    await press(button(host, 'scan-library'))
    await press(button(host, 'remove-library'))
    expect(api.startLibraryScan).not.toHaveBeenCalled()
    expect(host.querySelector('.remove-library')).toBeNull()
    // The dialog the person is in keeps its counts: nothing changed under it.
    expect(host.querySelector('.remove-root .counts')).not.toBeNull()
  })

  it('holds every folder’s Scan folder and Remove while the library’s removal is open, and sends nothing from them', async () => {
    vi.mocked(api.libraryRemovalCost).mockResolvedValue(counted(12))
    vi.mocked(config.startScan).mockResolvedValue({
      root_id: 1,
      status_url: '/api/library/roots/1/scan',
    } as never)
    const { host } = await shown()
    await press(button(host, 'remove-library'))
    expect(host.querySelector('.remove-library .counts')).not.toBeNull()

    for (const row of [0, 1]) {
      expect(rowButton(host, row, 'Scan folder').disabled).toBe(true)
      expect(rowButton(host, row, 'Remove…').disabled).toBe(true)
      await press(rowButton(host, row, 'Scan folder'))
      await press(rowButton(host, row, 'Remove…'))
    }
    expect(config.startScan).not.toHaveBeenCalled()
    expect(config.removalCost).not.toHaveBeenCalled()
    expect(host.querySelector('.remove-root')).toBeNull()

    // Cancelled, the rows are live again.
    await press(
      [...host.querySelectorAll('.remove-library button')].find(
        (b) => b.textContent === 'Cancel',
      ) as Element,
    )
    expect(rowButton(host, 0, 'Scan folder').disabled).toBe(false)
    expect(rowButton(host, 0, 'Remove…').disabled).toBe(false)
  })

  it('holds the library’s Scan library and Remove while a folder is scanning from its own button, and sends nothing from them', async () => {
    vi.mocked(config.startScan).mockResolvedValue({
      root_id: 2,
      status_url: '/api/library/roots/2/scan',
    } as never)
    const { host } = await shown()
    await press(rowButton(host, 1, 'Scan folder'))
    expect(
      host.querySelectorAll('.root-row')[1].querySelector('.scanning'),
    ).not.toBeNull()

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
    expect(host.querySelector('.library-card > .scanning')).toBeNull()
    expect(onChanged).toHaveBeenCalledTimes(1)
    expect(onChanged).toHaveBeenCalledWith(true)
  })

  it('frees the library once the folder’s scan has ended', async () => {
    vi.mocked(config.startScan).mockResolvedValue({
      root_id: 2,
      status_url: '/api/library/roots/2/scan',
    } as never)
    const { host } = await shown()
    await press(rowButton(host, 1, 'Scan folder'))
    expect(button(host, 'scan-library').disabled).toBe(true)
    await tick()
    expect(button(host, 'scan-library').disabled).toBe(false)
    expect(button(host, 'remove-library').disabled).toBe(false)
  })

  it('frees the library when a folder’s removal dialog is cancelled', async () => {
    vi.mocked(config.removalCost).mockResolvedValue(counted())
    const { host } = await shown()
    await press(rowButton(host, 0, 'Remove…'))
    expect(button(host, 'scan-library').disabled).toBe(true)
    await press(
      [...host.querySelectorAll('.remove-root button')].find(
        (b) => b.textContent === 'Cancel',
      ) as Element,
    )
    expect(button(host, 'scan-library').disabled).toBe(false)
  })

  it('hands a folder’s removal up by its path, and not as the library’s', async () => {
    vi.mocked(config.removalCost).mockResolvedValue(counted())
    const real = { ...counted(5), dry_run: false }
    vi.mocked(config.removeRoot).mockResolvedValue(real)
    const { host, onFolderRemoved, onRemoved, onChanged } = await shown()
    await press(rowButton(host, 1, 'Remove…'))
    await press(host.querySelector('.remove-root .confirm') as Element)
    expect(onFolderRemoved).toHaveBeenCalledWith('/media/Manga/Chapters', real)
    expect(onRemoved).not.toHaveBeenCalled()
    expect(onChanged).toHaveBeenCalled()
  })
})

describe('LibraryCard, what it says went wrong', () => {
  it('keeps a 409’s sentence only until the scan it named has ended', async () => {
    vi.mocked(api.startLibraryScan).mockRejectedValue(
      new Error(
        '/api/library/libraries/7/scan: 409 {"error":"a scan of /media/Manga/Chapters is already running"}',
      ),
    )
    const { host } = await shown()
    await press(button(host, 'scan-library'))
    expect(cardAlert(host)?.textContent).toBe(
      'a scan of /media/Manga/Chapters is already running',
    )
    expect(host.querySelector('.library-card > .scanning')).not.toBeNull()

    await tick()
    expect(cardAlert(host)).toBeNull()
    expect(host.querySelector('.library-card > .scanning')).toBeNull()
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
    expect(cardAlert(host)?.textContent).toBe(named)

    vi.mocked(api.libraryScanStatus).mockResolvedValueOnce({
      ...idle(7),
      running: true,
      elapsed_secs: 4,
    })
    await tick()
    expect(cardAlert(host)?.textContent).toBe(named)
    expect(host.querySelector('.library-card > .scanning')).not.toBeNull()

    await tick()
    expect(cardAlert(host)).toBeNull()
    expect(host.querySelector('.library-card > .scanning')).toBeNull()
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
    expect(cardAlert(host)?.textContent).toContain('already running')
    await tick()
    await tick()
    expect(cardAlert(host)?.textContent).toContain('already running')
    await tick()
    expect(
      [...host.querySelectorAll('.library-card > [role="alert"]')].map(
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
    expect(cardAlert(host)?.textContent).toBe('no disk')
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
    vi.mocked(api.addFolder)
      .mockRejectedValueOnce(
        new Error('/api/library/libraries/7/folders: 400 {"error":"nope"}'),
      )
      .mockReturnValueOnce(new Promise(() => {}))
    const { host } = await shown()
    const picker = host.querySelector(
      '.library-card > .add-root',
    ) as HTMLElement
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
    vi.mocked(api.addFolder).mockRejectedValue(
      new Error('/api/library/libraries/7/folders: 400 {"error":"nope"}'),
    )
    const { host } = await shown()
    const picker = host.querySelector(
      '.library-card > .add-root',
    ) as HTMLElement
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
    expect(cardAlert(host)?.textContent).toBe('the registry is unreadable')
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
    expect(host.querySelector('.library-card > .scanning')).not.toBeNull()
    expect(cardAlert(host)).toBeNull()
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
    expect(host.querySelector('.library-card > .scanning')).toBeNull()
    await press(button(host, 'scan-library'))
    expect(host.querySelector('.library-card > .scanning')).not.toBeNull()
    await tick()
    expect(host.querySelector('.library-card > .scanning')).not.toBeNull()
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
    act(() => rerender(<ul />))
    expect(host.querySelector('.library-card')).toBeNull()
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

describe('LibraryCard, moving a folder', () => {
  it('hands a folder’s move up by its path', async () => {
    const moved: api.MoveAnswer = {
      moved: {
        library_id: 8,
        library: 'Comics',
        works_kept: 1,
        works_joined: 0,
        works_split: 0,
        series_joined: 0,
        positions: 0,
        notes: 0,
        alignments_dropped: 0,
        library_removed: null,
        grants_removed: 0,
        left_with_no_library: [],
      },
      derive: { ...emptyDerive, lost_positions: 6 },
      dry_run: false,
    }
    vi.mocked(api.moveCost).mockResolvedValue({
      ...moved,
      derive: { ...emptyDerive, lost_positions: 2 },
      dry_run: true,
    })
    vi.mocked(api.moveFolder).mockResolvedValue(moved)
    const { host, onFolderMoved, onFolderRemoved } = await shown()
    await press(rowButton(host, 1, 'Move…'))
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
    expect(onFolderMoved).toHaveBeenCalledWith('/media/Manga/Chapters', moved)
    expect(onFolderRemoved).not.toHaveBeenCalled()
  })

  it('offers each folder the other libraries', async () => {
    const { host } = await shown()
    const row = host.querySelector('.root-row') as HTMLElement
    const move = [...row.querySelectorAll('button')].find(
      (b) => b.textContent === 'Move…',
    ) as HTMLButtonElement
    await press(move)
    const target = row.querySelector(
      'select[name="move-target"]',
    ) as HTMLSelectElement
    expect([...target.options].map((o) => o.textContent)).toEqual([
      'Choose a library…',
      'Comics',
      'A new library…',
    ])
  })
})

/** The card as the screen draws it, for a test that changes its library. */
const cardOf = (library: api.Library) => (
  <ul>
    <LibraryCard
      library={library}
      others={[{ id: 8, name: 'Comics' }]}
      probes={{}}
      onAdded={vi.fn()}
      onChanged={vi.fn()}
      onRemoved={vi.fn()}
      onFolderRemoved={vi.fn()}
      onFolderMoved={vi.fn()}
    />
  </ul>
)

const statusOf = (id: number, running: boolean): config.ScanStatus => ({
  root_id: id,
  running,
  elapsed_secs: running ? 3 : undefined,
  report: null,
  last_scan_at: null,
  last_scan_status: null,
})

/** Each folder's own status, running for the ids named and idle for the rest. */
const runningRows = (...ids: number[]) =>
  vi
    .mocked(config.scanStatus)
    .mockImplementation((id) => Promise.resolve(statusOf(id, ids.includes(id))))

const held = (...buttons: HTMLButtonElement[]) => buttons.map((b) => b.disabled)

describe('LibraryCard, scans and removals of a library and its folders exclude each other', () => {
  it('holds a folder’s Scan folder and Remove… while a sibling is scanning, and sends nothing from them', async () => {
    vi.mocked(config.startScan).mockResolvedValue({
      root_id: 1,
      status_url: '/api/library/roots/1/scan',
    } as never)
    const { host } = await shown()
    runningRows(1)
    await press(rowButton(host, 0, 'Scan folder'))

    expect(
      held(rowButton(host, 1, 'Scan folder'), rowButton(host, 1, 'Remove…')),
    ).toEqual([true, true])
    await press(rowButton(host, 1, 'Scan folder'))
    await press(rowButton(host, 1, 'Remove…'))
    expect(config.startScan).toHaveBeenCalledTimes(1)
    expect(config.startScan).toHaveBeenCalledWith(1)
    expect(config.removalCost).not.toHaveBeenCalled()
    expect(host.querySelector('.remove-root')).toBeNull()

    runningRows()
    await tick()
    expect(
      held(rowButton(host, 1, 'Scan folder'), rowButton(host, 1, 'Remove…')),
    ).toEqual([false, false])
  })

  it('holds a folder’s Scan folder and Remove… while a sibling’s removal is being confirmed, and leaves that dialog live', async () => {
    vi.mocked(config.removalCost).mockResolvedValue(counted())
    const { host } = await shown()
    await press(rowButton(host, 0, 'Remove…'))

    expect(
      held(rowButton(host, 1, 'Scan folder'), rowButton(host, 1, 'Remove…')),
    ).toEqual([true, true])
    await press(rowButton(host, 1, 'Scan folder'))
    await press(rowButton(host, 1, 'Remove…'))
    expect(config.startScan).not.toHaveBeenCalled()
    expect(config.removalCost).toHaveBeenCalledTimes(1)
    expect(config.removalCost).toHaveBeenCalledWith(1)
    expect(host.querySelectorAll('.remove-root')).toHaveLength(1)
    // The dialog the person is in is not held by the sibling's being held.
    const confirm = host.querySelector('.remove-root .confirm')
    expect((confirm as HTMLButtonElement).disabled).toBe(false)
    expect(host.querySelector('.remove-root .counts')).not.toBeNull()

    await press(
      [...host.querySelectorAll('.remove-root button')].find(
        (b) => b.textContent === 'Cancel',
      ) as Element,
    )
    expect(
      held(rowButton(host, 1, 'Scan folder'), rowButton(host, 1, 'Remove…')),
    ).toEqual([false, false])
  })

  it('closes a folder’s open removal dialog when a sibling is found scanning under it, and asks its cost afresh when it is opened again', async () => {
    const late = deferred<config.ScanStatus>()
    vi.mocked(config.scanStatus).mockImplementation((id) =>
      id === 2 ? late.promise : Promise.resolve(statusOf(id, false)),
    )
    vi.mocked(config.removalCost).mockResolvedValue(counted(5))
    const { host } = await shown()
    await press(rowButton(host, 0, 'Remove…'))
    expect(host.querySelector('.remove-root .counts')?.textContent).toContain(
      '5 works',
    )

    await flush(() => late.resolve(statusOf(2, true)))
    expect(host.querySelector('.remove-root')).toBeNull()
    expect(config.removeRoot).not.toHaveBeenCalled()
    expect(config.removalCost).toHaveBeenCalledTimes(1)

    vi.mocked(config.removalCost).mockResolvedValue(counted(9))
    runningRows()
    await tick()
    await press(rowButton(host, 0, 'Remove…'))
    expect(config.removalCost).toHaveBeenCalledTimes(2)
    expect(host.querySelector('.remove-root .counts')?.textContent).toContain(
      '9 works',
    )
  })

  it('keeps a folder’s removal dialog while its own DELETE is out when a sibling is found scanning, and draws the DELETE’s refusal', async () => {
    const late = deferred<config.ScanStatus>()
    vi.mocked(config.scanStatus).mockImplementation((id) =>
      id === 2 ? late.promise : Promise.resolve(statusOf(id, false)),
    )
    vi.mocked(config.removalCost).mockResolvedValue(counted(5))
    const del = deferred<config.RemovalAnswer>()
    vi.mocked(config.removeRoot).mockReturnValueOnce(del.promise)
    const { host } = await shown()
    await press(rowButton(host, 0, 'Remove…'))
    await press(host.querySelector('.remove-root .confirm') as Element)
    expect(host.querySelector('.remove-root .confirm')?.textContent).toBe(
      'Removing…',
    )

    await flush(() => late.resolve(statusOf(2, true)))
    expect(host.querySelector('.remove-root .confirm')?.textContent).toBe(
      'Removing…',
    )

    // The sibling's scan ends with the DELETE still out: the row is still
    // busy, so the card stays held and nothing else is sent.
    runningRows()
    await tick()
    expect(button(host, 'scan-library').disabled).toBe(true)
    await press(button(host, 'scan-library'))
    expect(api.startLibraryScan).not.toHaveBeenCalled()
    expect(config.removeRoot).toHaveBeenCalledTimes(1)

    await flush(() =>
      del.reject(
        new Error('/api/library/roots/1: 500 {"error":"the disk is full"}'),
      ),
    )
    expect(host.querySelector('.remove-root [role="alert"]')?.textContent).toBe(
      'the disk is full',
    )
  })

  it('closes a folder’s open removal dialog when the card’s own first read finds a scan under it', async () => {
    const late = deferred<api.LibraryScanStatus>()
    vi.mocked(api.libraryScanStatus).mockReturnValueOnce(late.promise)
    vi.mocked(config.removalCost).mockResolvedValue(counted(5))
    const { host } = await shown()
    await press(rowButton(host, 0, 'Remove…'))
    expect(host.querySelector('.remove-root .counts')).not.toBeNull()

    await flush(() =>
      late.resolve({ ...idle(7), running: true, elapsed_secs: 5 }),
    )
    expect(host.querySelector('.remove-root')).toBeNull()
    expect(config.removeRoot).not.toHaveBeenCalled()

    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    vi.mocked(config.removalCost).mockResolvedValue(counted(9))
    await tick()
    await press(rowButton(host, 0, 'Remove…'))
    expect(config.removalCost).toHaveBeenCalledTimes(2)
    expect(host.querySelector('.remove-root .counts')?.textContent).toContain(
      '9 works',
    )
  })

  it('holds every folder’s Scan folder and Remove… while a scan the card found at mount is running', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      running: true,
      elapsed_secs: 9,
    })
    const { host } = await shown()
    expect(
      host.querySelector('.library-card > .scanning')?.textContent,
    ).toContain('A scan is running in this library')
    for (const row of [0, 1]) {
      expect(
        held(
          rowButton(host, row, 'Scan folder'),
          rowButton(host, row, 'Remove…'),
        ),
      ).toEqual([true, true])
      await press(rowButton(host, row, 'Scan folder'))
      await press(rowButton(host, row, 'Remove…'))
    }
    expect(config.startScan).not.toHaveBeenCalled()
    expect(config.removalCost).not.toHaveBeenCalled()
    expect(config.removeRoot).not.toHaveBeenCalled()
    expect(host.querySelector('.remove-root')).toBeNull()
  })

  it('holds every folder’s Scan folder and Remove… once a 409 has told the card a scan is running', async () => {
    vi.mocked(api.startLibraryScan).mockRejectedValue(
      new Error(
        '/api/library/libraries/7/scan: 409 {"error":"a scan of this library is already running"}',
      ),
    )
    const { host } = await shown()
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      running: true,
      elapsed_secs: 9,
    })
    await press(button(host, 'scan-library'))
    expect(cardAlert(host)?.textContent).toBe(
      'a scan of this library is already running',
    )
    for (const row of [0, 1]) {
      expect(
        held(
          rowButton(host, row, 'Scan folder'),
          rowButton(host, row, 'Remove…'),
        ),
      ).toEqual([true, true])
      await press(rowButton(host, row, 'Remove…'))
    }
    expect(config.removalCost).not.toHaveBeenCalled()
    expect(config.removeRoot).not.toHaveBeenCalled()

    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    expect(
      held(rowButton(host, 0, 'Scan folder'), rowButton(host, 0, 'Remove…')),
    ).toEqual([false, false])
  })

  it('holds every folder’s Scan folder and Remove… while the card’s own scan is being started', async () => {
    const start = deferred<never>()
    vi.mocked(api.startLibraryScan).mockReturnValue(start.promise)
    const { host } = await shown()
    await press(button(host, 'scan-library'))
    for (const row of [0, 1]) {
      expect(
        held(
          rowButton(host, row, 'Scan folder'),
          rowButton(host, row, 'Remove…'),
        ),
      ).toEqual([true, true])
      await press(rowButton(host, row, 'Scan folder'))
      await press(rowButton(host, row, 'Remove…'))
    }
    expect(config.startScan).not.toHaveBeenCalled()
    expect(config.removalCost).not.toHaveBeenCalled()
    expect(host.querySelector('.remove-root')).toBeNull()
    expect(api.startLibraryScan).toHaveBeenCalledTimes(1)
  })

  it('holds the card and the sibling while a folder’s own scan is being started', async () => {
    const start = deferred<never>()
    vi.mocked(config.startScan).mockReturnValue(start.promise)
    const { host } = await shown()
    await press(rowButton(host, 1, 'Scan folder'))
    expect(
      held(button(host, 'scan-library'), button(host, 'remove-library')),
    ).toEqual([true, true])
    expect(
      held(rowButton(host, 0, 'Scan folder'), rowButton(host, 0, 'Remove…')),
    ).toEqual([true, true])
    await press(button(host, 'scan-library'))
    expect(api.startLibraryScan).not.toHaveBeenCalled()
  })

  it('frees the card when a folder that was scanning is no longer drawn', async () => {
    vi.mocked(config.startScan).mockResolvedValue({
      root_id: 2,
      status_url: '/api/library/roots/2/scan',
    } as never)
    const { host } = await shown()
    runningRows(2)
    await press(rowButton(host, 1, 'Scan folder'))
    expect(button(host, 'scan-library').disabled).toBe(true)

    await act(async () => {
      rerender(cardOf(manga({ folders: [folder(1, '/media/Manga/Volumes')] })))
      await Promise.resolve()
    })
    expect(host.querySelectorAll('.root-row')).toHaveLength(1)
    expect(button(host, 'scan-library').disabled).toBe(false)
    expect(button(host, 'remove-library').disabled).toBe(false)
  })

  it('frees the card when a scanning folder’s id changes under its path and the scan ends', async () => {
    vi.mocked(config.startScan).mockResolvedValue({
      root_id: 2,
      status_url: '/api/library/roots/2/scan',
    } as never)
    const { host } = await shown()
    runningRows(2)
    await press(rowButton(host, 1, 'Scan folder'))
    await act(async () => {
      rerender(
        cardOf(
          manga({
            folders: [
              folder(1, '/media/Manga/Volumes'),
              folder(9, '/media/Manga/Chapters'),
            ],
          }),
        ),
      )
      await Promise.resolve()
    })
    runningRows(9)
    expect(button(host, 'scan-library').disabled).toBe(true)

    runningRows()
    await tick()
    await tick()
    expect(button(host, 'scan-library').disabled).toBe(false)
    expect(button(host, 'remove-library').disabled).toBe(false)
    expect(rowButton(host, 0, 'Scan folder').disabled).toBe(false)
  })

  it('holds the library’s removal when a folder is found scanning under its open dialog, and asks its cost again when that ends', async () => {
    const late = deferred<config.ScanStatus>()
    vi.mocked(config.scanStatus).mockImplementation((id) =>
      id === 2 ? late.promise : Promise.resolve(statusOf(id, false)),
    )
    vi.mocked(api.libraryRemovalCost).mockResolvedValue(counted(12))
    const { host } = await shown()
    await press(button(host, 'remove-library'))
    const confirm = () =>
      host.querySelector('.remove-library .confirm') as HTMLButtonElement
    expect(confirm().disabled).toBe(false)

    await flush(() => late.resolve(statusOf(2, true)))
    expect(confirm().disabled).toBe(true)
    confirm().click()
    await flush()
    expect(api.removeLibrary).not.toHaveBeenCalled()
    expect(api.libraryRemovalCost).toHaveBeenCalledTimes(1)

    // The scan ends; what removing would cost is asked for again, and no
    // confirm stands over the figures from before it.
    const second = deferred<config.RemovalAnswer>()
    vi.mocked(api.libraryRemovalCost).mockReturnValueOnce(second.promise)
    runningRows()
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

  it('holds the library’s removal when the card’s own first read finds a scan under its open dialog', async () => {
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

  it('holds Rename… and Add folder while the library’s removal is being decided, and sends nothing from them', async () => {
    vi.mocked(api.libraryRemovalCost).mockResolvedValue(counted(12))
    const { host } = await shown()
    const picker = host.querySelector(
      '.library-card > .add-root',
    ) as HTMLElement
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
    expect(api.addFolder).not.toHaveBeenCalled()

    // Cancelled, both are live again.
    await press(
      [...host.querySelectorAll('.remove-library button')].find(
        (b) => b.textContent === 'Cancel',
      ) as Element,
    )
    expect(held(save, add)).toEqual([false, false])
  })

  it('holds Rename… and Add folder while the library’s removal is on its way', async () => {
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
    const picker = host.querySelector(
      '.library-card > .add-root',
    ) as HTMLElement
    await typeInto(
      picker.querySelector('input[name="path"]') as HTMLInputElement,
      '/media/Manga/Extras',
    )
    const add = picker.querySelector('button[name="pick"]') as HTMLButtonElement
    expect(add.disabled).toBe(true)
    await press(add)
    expect(api.addFolder).not.toHaveBeenCalled()
  })

  it('does not hold a folder’s settings, Rename, Save or Add folder for a scan', async () => {
    vi.mocked(config.startScan).mockResolvedValue({
      root_id: 1,
      status_url: '/api/library/roots/1/scan',
    } as never)
    vi.mocked(config.patchRoot).mockResolvedValue(undefined)
    vi.mocked(api.renameLibrary).mockResolvedValue({ id: 7, name: 'Comics' })
    vi.mocked(api.addFolder).mockResolvedValue({
      root: folder(3, '/media/Manga/Extras'),
      watching: true,
      watch_reason: null,
      interval_secs: 86400,
    })
    const { host } = await shown()
    runningRows(1)
    await press(rowButton(host, 0, 'Scan folder'))
    expect(
      host.querySelectorAll('.root-row')[0].querySelector('.scanning'),
    ).not.toBeNull()

    const tick1 = host
      .querySelectorAll('.root-row')[1]
      .querySelector('input[name="keeps_documents"]') as HTMLInputElement
    expect(tick1.disabled).toBe(false)
    await press(tick1)
    expect(config.patchRoot).toHaveBeenCalledWith(2, { keeps_documents: true })

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

    // The sibling's text boxes and its interval box send on blur, too.
    const siblingRow = host.querySelectorAll('.root-row')[1]
    for (const name of [
      'edition_key',
      'default_kind',
      'default_language',
      'exclude_globs',
      'scan_interval_secs',
    ]) {
      const box = siblingRow.querySelector(
        `[name="${name}"]`,
      ) as HTMLInputElement
      expect(box.disabled, name).toBe(false)
    }
    await editBox(
      siblingRow.querySelector('input[name="edition_key"]'),
      'weekly',
    )
    expect(config.patchRoot).toHaveBeenCalledWith(2, { edition_key: 'weekly' })
    await editBox(
      siblingRow.querySelector('input[name="scan_interval_secs"]'),
      '600',
    )
    expect(config.patchRoot).toHaveBeenCalledWith(2, {
      scan_interval_secs: 600,
    })

    const picker = host.querySelector(
      '.library-card > .add-root',
    ) as HTMLElement
    await typeInto(
      picker.querySelector('input[name="path"]') as HTMLInputElement,
      '/media/Manga/Extras',
    )
    await press(picker.querySelector('button[name="pick"]') as Element)
    expect(api.addFolder).toHaveBeenCalledWith(7, '/media/Manga/Extras')
  })
})

describe('LibraryCard, what a refusal outlasts', () => {
  const refusedAdd = async (host: HTMLElement) => {
    vi.mocked(api.addFolder).mockRejectedValue(
      new Error('/api/library/libraries/7/folders: 400 {"error":"nope"}'),
    )
    const picker = host.querySelector(
      '.library-card > .add-root',
    ) as HTMLElement
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
    vi.mocked(api.listFolders).mockImplementation((path) =>
      Promise.resolve(
        path === undefined
          ? {
              path: null,
              parent: null,
              folders: [{ name: '/media', path: '/media' }],
            }
          : { path, parent: null, folders: [] },
      ),
    )
    const { host } = await shown()
    const picker = await refusedAdd(host)
    expect(picker.querySelector('[role="alert"]')?.textContent).toBe('nope')

    await press(picker.querySelector('button[name="browse"]') as Element)
    expect(picker.querySelector('[role="alert"]')?.textContent).toBe('nope')
    await press(picker.querySelector('.folder-list button') as Element)
    expect(
      (picker.querySelector('input[name="path"]') as HTMLInputElement).value,
    ).toBe('/media')
    expect(picker.querySelector('[role="alert"]')).toBeNull()
  })
})
