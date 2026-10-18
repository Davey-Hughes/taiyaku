import { act } from 'react'
import { MemoryRouter } from 'react-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from './config'
import { FolderPage } from './FolderPage'
import * as api from './libraries'
import { deferred, flush } from './testing/async'
import { emptyDerive } from './testing/derive'
import { press } from './testing/input'
import {
  counted,
  folder,
  idle,
  manga,
  scanningIn,
  tick,
} from './testing/libraries'
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
  libraryScanStatus: vi.fn(),
  moveCost: vi.fn(),
  moveFolder: vi.fn(),
}))

/**
 * The page of the folder `at` at its own route, with each handler a mock. It
 * draws the second folder, so the first is the sibling a test scans.
 */
async function shown({
  library = manga(),
  at = 2,
}: {
  library?: api.Library
  at?: number
} = {}) {
  const root = library.folders.find((one) => one.id === at)
  if (root === undefined) throw new Error(`no folder ${at}`)
  const onChanged = vi.fn()
  const onRemoved = vi.fn()
  const onMoved = vi.fn()
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <MemoryRouter
        initialEntries={[`/config/libraries/${library.id}/folders/${root.id}`]}
      >
        <FolderPage
          library={library}
          root={root}
          others={[{ id: 8, name: 'Comics' }]}
          onChanged={onChanged}
          onRemoved={onRemoved}
          onMoved={onMoved}
        />
      </MemoryRouter>,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return { host, onChanged, onRemoved, onMoved }
}

const control = (host: HTMLElement, text: string) =>
  [...host.querySelectorAll('button')].find(
    (b) => b.textContent === text,
  ) as HTMLButtonElement

/** Scan folder, Move… and Remove…, each drawn disabled or not. */
const actionsHeld = (host: HTMLElement) =>
  ['Scan folder', 'Move…', 'Remove…'].map(
    (text) => control(host, text).disabled,
  )

const statusOf = (id: number, running: boolean): config.ScanStatus => ({
  root_id: id,
  running,
  elapsed_secs: running ? 3 : undefined,
  report: null,
  last_scan_at: null,
  last_scan_status: null,
})

/** Each folder's own status: running for the ids named, idle for the rest. */
const ownRunning = (...ids: number[]) =>
  vi
    .mocked(config.scanStatus)
    .mockImplementation((id) => Promise.resolve(statusOf(id, ids.includes(id))))

/** The folder's own Scanning… line, which `RootRow` draws from its status. */
const ownScanning = (host: HTMLElement) =>
  host.querySelector('.root-row .scanning')

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

describe('FolderPage', () => {
  it('words the folder’s button Scan folder', async () => {
    const { host } = await shown()
    expect(
      [...host.querySelectorAll('.root-row button')]
        .map((b) => b.textContent)
        .filter((text) => text?.startsWith('Scan')),
    ).toEqual(['Scan folder'])
  })

  it('tells a folder it is the last one, and no folder of two', async () => {
    vi.mocked(config.removalCost).mockResolvedValue(counted(0))
    const one = await shown({
      library: manga({ folders: [folder(1, '/media/Manga/Volumes')] }),
      at: 1,
    })
    await press(control(one.host, 'Remove…'))
    expect(one.host.querySelector('.last-folder')?.textContent).toContain(
      'the only folder of the library Manga',
    )

    const two = await shown()
    await press(control(two.host, 'Remove…'))
    expect(two.host.querySelector('.remove-root .counts')).not.toBeNull()
    expect(two.host.querySelector('.last-folder')).toBeNull()
  })

  it('hands a folder’s removal up, and not as a move', async () => {
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

  it('hands a folder’s move up, and not as a removal', async () => {
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
    expect(api.moveFolder).toHaveBeenCalledWith(2, { library: 8 })
    expect(onMoved).toHaveBeenCalledWith(moved)
    expect(onRemoved).not.toHaveBeenCalled()
  })

  it('offers the folder the other libraries', async () => {
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

describe('FolderPage, scans and removals across its library exclude each other', () => {
  it('holds the folder’s Scan folder, Move… and Remove… while a sibling is scanning, and sends nothing from them', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 1))
    const { host } = await shown()

    expect(actionsHeld(host)).toEqual([true, true, true])
    for (const text of ['Scan folder', 'Move…', 'Remove…']) {
      await press(control(host, text))
    }
    expect(config.startScan).not.toHaveBeenCalled()
    expect(config.removalCost).not.toHaveBeenCalled()
    expect(host.querySelector('.remove-root')).toBeNull()
    expect(host.querySelector('.move-root')).toBeNull()

    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    expect(actionsHeld(host)).toEqual([false, false, false])
  })

  it('holds Scan folder, Move… and Remove… while its library is being scanned, and asks again until the scan ends', async () => {
    // A scan of the library holds every folder's slot until the whole scan
    // ends, so this folder's own status says running too.
    vi.mocked(api.libraryScanStatus).mockResolvedValue(
      scanningIn(manga(), 1, 2),
    )
    ownRunning(1, 2)
    const { host } = await shown()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(1)
    expect(api.libraryScanStatus).toHaveBeenCalledWith(7)
    expect(actionsHeld(host)).toEqual([true, true, true])
    expect(ownScanning(host)).not.toBeNull()

    await tick()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(2)
    expect(actionsHeld(host)).toEqual([true, true, true])

    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    ownRunning()
    await tick()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(3)
    expect(ownScanning(host)).toBeNull()
    expect(actionsHeld(host)).toEqual([false, false, false])

    // Nothing else is scanning, so nothing more is asked.
    await tick()
    await tick()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(3)
  })

  it('leaves a scan of only this folder to the folder’s own status, and asks its library’s once', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 2))
    ownRunning(2)
    const { host } = await shown()
    expect(ownScanning(host)).not.toBeNull()
    await tick()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(1)

    // The folder's own poll sees its scan end, and that alone frees the
    // buttons: the library's status is not asked again.
    ownRunning()
    await tick()
    expect(ownScanning(host)).toBeNull()
    expect(actionsHeld(host)).toEqual([false, false, false])
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(1)
  })

  it('closes the folder’s open removal dialog when the page’s own first read finds a scan of the library under it, and asks its cost afresh once that ends', async () => {
    // The library's status and the folder's own are separate requests, and
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

  it('closes a folder’s open removal dialog when a sibling is found scanning under it, and asks its cost afresh when it is opened again', async () => {
    const late = deferred<api.LibraryScanStatus>()
    vi.mocked(api.libraryScanStatus).mockReturnValueOnce(late.promise)
    vi.mocked(config.removalCost).mockResolvedValue(counted(5))
    const { host } = await shown()
    await press(control(host, 'Remove…'))
    expect(host.querySelector('.remove-root .counts')?.textContent).toContain(
      '5 works',
    )

    await flush(() => late.resolve(scanningIn(manga(), 1)))
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

  it('keeps a folder’s removal dialog while its own DELETE is out when a sibling is found scanning, and draws the DELETE’s refusal', async () => {
    const late = deferred<api.LibraryScanStatus>()
    vi.mocked(api.libraryScanStatus).mockReturnValueOnce(late.promise)
    vi.mocked(config.removalCost).mockResolvedValue(counted(5))
    const del = deferred<config.RemovalAnswer>()
    vi.mocked(config.removeRoot).mockReturnValueOnce(del.promise)
    const { host } = await shown()
    await press(control(host, 'Remove…'))
    await press(host.querySelector('.remove-root .confirm') as Element)
    expect(host.querySelector('.remove-root .confirm')?.textContent).toBe(
      'Removing…',
    )

    await flush(() => late.resolve(scanningIn(manga(), 1)))
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

  it('closes the folder’s open Move… dialog when its first read finds a scan of the library under it', async () => {
    // As in the removal's case above: the library's status answers before
    // the folder's own, which says running too.
    const late = deferred<api.LibraryScanStatus>()
    vi.mocked(api.libraryScanStatus).mockReturnValueOnce(late.promise)
    const own = deferred<config.ScanStatus>()
    vi.mocked(config.scanStatus).mockReturnValueOnce(own.promise)
    const { host } = await shown()
    await press(control(host, 'Move…'))
    expect(host.querySelector('.move-root')).not.toBeNull()

    await flush(() => late.resolve(scanningIn(manga(), 1, 2)))
    expect(host.querySelector('.move-root')).toBeNull()
    expect(api.moveFolder).not.toHaveBeenCalled()
    await flush(() => own.resolve(statusOf(2, true)))
    expect(ownScanning(host)).not.toBeNull()
  })

  it('does not hold the folder’s settings for a sibling’s scan', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 1))
    const { host } = await shown()
    expect(control(host, 'Scan folder').disabled).toBe(true)

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

  it('stops asking when it is unmounted while another folder is scanning', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 1))
    const { host } = await shown()
    vi.mocked(api.libraryScanStatus).mockClear()
    act(() => rerender(<div />))
    expect(host.querySelector('.folder-page')).toBeNull()
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
    expect(actionsHeld(host)).toEqual([true, true, true])

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
    expect(actionsHeld(host)).toEqual([true, true, true])
    expect(host.querySelector('[role="alert"]')).toBeNull()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(3)

    await tick()
    expect(actionsHeld(host)).toEqual([false, false, false])
    // The sentence of the poll that ended the hold, as the library page's
    // poll draws its own.
    expect(
      [...host.querySelectorAll('[role="alert"]')].map((p) => p.textContent),
    ).toEqual(['the store is locked'])
    await tick()
    await tick()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(4)
  })

  it('drops the sentence of a given-up hold when the folder’s own scan is pressed', async () => {
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
    expect(host.querySelector('.folder-page > [role="alert"]')).not.toBeNull()

    await press(control(host, 'Scan folder'))
    expect(config.startScan).toHaveBeenCalledWith(2)
    expect(host.querySelector('.folder-page > [role="alert"]')).toBeNull()
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
    expect(host.querySelector('.folder-page > [role="alert"]')).not.toBeNull()

    // The same path under another id: the page asks its library again, and
    // the read fails as a first read: silently.
    vi.mocked(api.libraryScanStatus).mockClear()
    const library = manga({
      folders: [
        folder(1, '/media/Manga/Volumes'),
        folder(3, '/media/Manga/Chapters'),
      ],
    })
    await act(async () => {
      rerender(
        <MemoryRouter initialEntries={['/config/libraries/7/folders/3']}>
          <FolderPage
            library={library}
            root={library.folders[1]}
            others={[{ id: 8, name: 'Comics' }]}
            onChanged={vi.fn()}
            onRemoved={vi.fn()}
            onMoved={vi.fn()}
          />
        </MemoryRouter>,
      )
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(1)
    expect(host.querySelector('.folder-page > [role="alert"]')).toBeNull()
    expect(actionsHeld(host)).toEqual([false, false, false])
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
    expect(actionsHeld(host)).toEqual([true, true, true])
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(6)
  })

  it('holds nothing and asks no more when its first read fails', async () => {
    vi.mocked(api.libraryScanStatus).mockRejectedValue(
      new Error('/api/library/libraries/7/scan: 502 bad gateway'),
    )
    const { host } = await shown()
    expect(actionsHeld(host)).toEqual([false, false, false])
    expect(host.querySelector('[role="alert"]')).toBeNull()
    await tick()
    await tick()
    await tick()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(1)
  })
})
