import { act, useState } from 'react'
import { MemoryRouter } from 'react-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from '../src/config'
import { NO_ADDRESS } from '../src/configAddress'
import { WAIT_DRAWN_AFTER_MS } from '../src/hooks'
import * as api from '../src/libraries'
import { LibraryPage } from '../src/LibraryPage'
import { FEED_IDLE_MS, type FeedJob, type ScanFeed } from '../src/scanFeed'
import { inUtc } from '../src/utc'
import { deferred, flush, land } from './support/async'
import { emptyDerive } from './support/derive'
import { press, submit, typeInto } from './support/input'
import { holdOf } from './support/held'
import { changesIn } from './support/liveRegion'
import { counted, dir, idle, manga, tick } from './support/libraries'
import { mount, rerender } from './support/mount'
import { feedJob, fingerprintPass, serveFeed } from './support/scanFeed'
import { expectRowsWired, helpOf, row, section } from './support/settingsRows'
import { cellValue } from './support/table'
import { watchDrawn } from './support/wait'

vi.mock('../src/config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/config')>()),
  scanStatus: vi.fn(),
  startScan: vi.fn(),
}))
vi.mock('../src/libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/libraries')>()),
  addDir: vi.fn(),
  libraryRemovalCost: vi.fn(),
  libraryScanStatus: vi.fn(),
  listDirs: vi.fn(),
  removeLibrary: vi.fn(),
  renameLibrary: vi.fn(),
  startLibraryScan: vi.fn(),
}))

/** Comics, library 8, of one directory of its own. */
const comics = () =>
  manga({
    id: 8,
    name: 'Comics',
    dirs: [dir(80, '/media/Comics', [{ id: 8, name: 'Comics' }])],
  })

/**
 * The page at its own route, with each handler a mock, in a list that holds
 * `others` beside it.
 */
async function shown(library = manga(), others: api.Library[] = []) {
  const onAdded = vi.fn()
  const onChanged = vi.fn()
  const onRemoved = vi.fn()
  const page = (drawn: api.Library, list = others) => (
    <MemoryRouter initialEntries={[`/config/libraries/${drawn.name}`]}>
      <LibraryPage
        library={drawn}
        libraries={[drawn, ...list]}
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
  /**
   * The same page handed another library, and the list's other libraries
   * if they changed, as a reload of the list does.
   */
  const redraw = (next: api.Library, list = others) =>
    act(async () => {
      rerender(page(next, list))
      await Promise.resolve()
    })
  return { host, onAdded, onChanged, onRemoved, redraw }
}

/**
 * The page drawn, left, and drawn again, as a return to it is: its body is
 * drawn at once from the scan status its first visit kept, and `again` is
 * the read of that status the return sends, which the test answers.
 */
async function returned() {
  await shown()
  act(() => rerender(<div />))
  const again = deferred<api.LibraryScanStatus>()
  vi.mocked(api.libraryScanStatus).mockReturnValueOnce(again.promise)
  return { ...(await shown()), again }
}

const button = (host: HTMLElement, name: string) =>
  host.querySelector(`button[name="${name}"]`) as HTMLButtonElement

/** The box that edits the name, in the header's title's place. */
const nameBox = (host: HTMLElement) =>
  host.querySelector(
    '.library-page > .page-header .page-header-text > form.rename-library > input[name="library-name"]',
  ) as HTMLInputElement

/** The rename's refusal, in the room the header keeps under its line. */
const renameRefusal = (host: HTMLElement) =>
  host.querySelector(
    '.library-page > .page-header > .page-header-answer > [role="alert"]',
  )

/** What the header draws over what is under it while the name is edited. */
const editorAnswer = (host: HTMLElement) =>
  host.querySelector('.library-page > .page-header > .page-header-answer')

/** Why the name typed would be refused, said as it is typed. */
const clashNote = (host: HTMLElement) =>
  editorAnswer(host)?.querySelector<HTMLElement>(':scope > [role="status"]')

/** Whether the name box is marked invalid, and what describes it. */
const boxMarks = (host: HTMLElement) => [
  nameBox(host).getAttribute('aria-invalid'),
  nameBox(host).getAttribute('aria-describedby'),
]

/** Past the time a write may be out before its wait is drawn. */
const waitDrawn = () =>
  act(() => new Promise((r) => setTimeout(r, WAIT_DRAWN_AFTER_MS + 50)))

/** Escape pressed on `el`, as a key goes to whatever has focus. */
const pressEscape = (el: Element) =>
  flush(() => {
    el.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }),
    )
  })

/** The page's title, beneath the box while the name is edited. */
const title = (host: HTMLElement) =>
  host.querySelector('.library-page > .page-header .page-title')

/**
 * The names and words of the header's buttons in one of its two sets: the
 * one a person can reach, or the one made inert, which is not seen either.
 */
const actions = (host: HTMLElement, set: 'shown' | 'unseen') =>
  [
    ...host.querySelectorAll(
      `.library-page > .page-header .page-actions > .page-actions-set${set === 'shown' ? ':not([inert])' : '[inert]'} button`,
    ),
  ].map((b) => [b.getAttribute('name'), b.textContent])

/** The header's line saying what the library holds. */
const holds = (host: HTMLElement) =>
  host.querySelector('.library-page > .page-header .page-description > .holds')
    ?.textContent

/** Press Add directory… and hand back what holds the picker and its refusal. */
async function openPicker(host: HTMLElement) {
  await press(button(host, 'add-dir'))
  return section(host, 'Directories').querySelector(
    ':scope > .settings-box > .add-root',
  ) as HTMLElement
}

/** The titles of the page's sections, in the order they are drawn. */
const sectionTitles = (host: HTMLElement) =>
  [
    ...host.querySelectorAll(
      '.library-page > .settings-section > .settings-section-head > .settings-section-title',
    ),
  ].map((title) => title.textContent)

/** The scan feed every test's page polls, idle until a test answers otherwise. */
let feed: ReturnType<typeof serveFeed>

/** The feed while no job runs, with its marker at `ended`. */
const idleFeed = (ended = 0): ScanFeed => ({ ended, libraries: [], jobs: [] })

/** The feed while `jobs` run, each library they hold at its job's step. */
const runningFeed = (...jobs: FeedJob[]): ScanFeed => ({
  ended: 0,
  libraries: jobs.flatMap((job) =>
    job.libraries.map(({ id }) => ({ id, step: job.step })),
  ),
  jobs,
})

/** A scan of the whole library, its two directories. */
const whole = () =>
  feedJob({
    dirs: [
      { id: 1, path: '/media/Manga/Volumes' },
      { id: 2, path: '/media/Manga/Chapters' },
    ],
  })

/** A scan of Chapters alone, as its own page starts one. */
const chapters = () =>
  feedJob({ id: 9, dirs: [{ id: 2, path: '/media/Manga/Chapters' }] })

/** Lets `ms` of the clock run, and the feed's polls sent meanwhile land. */
const clock = (ms: number) =>
  act(async () => {
    await vi.advanceTimersByTimeAsync(ms)
  })

/** The lines under the header saying what each scan holding it is doing. */
const scanLines = (host: HTMLElement) =>
  [...host.querySelectorAll('.library-page > .scanning')].map(
    (p) => p.textContent,
  )

/** The line saying the library's directories are paused, if it is drawn. */
const pausedLine = (host: HTMLElement) =>
  host.querySelector('.library-page > .paused')?.textContent

beforeEach(() => {
  vi.clearAllMocks()
  vi.useFakeTimers({ shouldAdvanceTime: true })
  feed = serveFeed(idleFeed())
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

  it('names the library, what it holds, and a row per directory', async () => {
    const { host } = await shown()
    const header = host.querySelector('.library-page > .page-header')
    expect(header?.querySelector('.page-title')?.textContent).toBe('Manga')
    expect(
      header?.querySelector('.page-description > .holds')?.textContent,
    ).toBe('12 works in 2 directories')
    expect(
      [...host.querySelectorAll('.dir-table tbody code')].map(
        (c) => c.textContent,
      ),
    ).toEqual(['/media/Manga/Volumes', '/media/Manga/Chapters'])
    expect(host.querySelector('.no-readable')).toBeNull()
  })

  it('heads itself with ‹ Libraries, its name, what it holds, and Rename… and Scan library as its actions, over Directories, Matching and the removal box', async () => {
    const { host } = await shown()
    const header = host.querySelector(
      '.library-page > .page-header',
    ) as HTMLElement
    const back = header.querySelector('.back a')
    expect([back?.textContent, back?.getAttribute('href')]).toEqual([
      '‹ Libraries',
      '/config/libraries',
    ])
    expect(header.querySelector('.page-title')?.tagName).toBe('H2')
    expect(title(host)?.hasAttribute('inert')).toBe(false)
    expect(actions(host, 'shown')).toEqual([
      ['rename-library', 'Rename…'],
      ['scan-library', 'Scan library'],
    ])
    // Save and Cancel keep the slot they share with these, out of reach.
    expect(actions(host, 'unseen')).toEqual([
      ['save-rename', 'Save'],
      ['cancel-rename', 'Cancel'],
    ])
    expect(sectionTitles(host)).toEqual(['Directories', 'Matching', 'Removal'])
    expect(
      section(host, 'Directories').querySelector(
        ':scope > .settings-box > .add-root > button[name="add-dir"]',
      )?.textContent,
    ).toBe('Add directory…')
    const removal = section(host, 'Removal')
    expect(removal.classList.contains('settings-removal')).toBe(true)
    expect(
      row(removal, 'Remove this library').querySelector(
        'button[name="remove-library"]',
      )?.textContent,
    ).toBe('Remove…')
  })

  it('opens the rename in the header’s title, nothing between the header and Directories, and the removal’s confirm inside the removal box', async () => {
    vi.mocked(api.libraryRemovalCost).mockReturnValue(new Promise(() => {}))
    const { host } = await shown()
    await press(button(host, 'rename-library'))
    const header = host.querySelector(
      '.library-page > .page-header',
    ) as HTMLElement
    expect(header.contains(nameBox(host))).toBe(true)
    expect(header.nextElementSibling).toBe(section(host, 'Directories'))
    await press(button(host, 'remove-library'))
    expect(
      section(host, 'Removal').querySelector(
        ':scope > .settings-box > .remove-library',
      ),
    ).not.toBeNull()
  })

  it('ties each row’s label and help to its control, and names the rename’s box Name — O175', async () => {
    const { host } = await shown()
    await press(button(host, 'rename-library'))
    // Every row the page draws, named, so the check is never over fewer.
    expect(
      [...host.querySelectorAll('.settings-row-label')].map(
        (label) => label.textContent,
      ),
    ).toEqual(['Match editions by page content', 'Remove this library'])
    expect(nameBox(host).getAttribute('aria-label')).toBe('Name')
    expectRowsWired(host)
  })

  it('lays a box holding the name over the title, which stays inert beneath it, selected, with focus in it, and shows Save and Cancel where Rename… and Scan library were', async () => {
    const { host } = await shown()
    await press(button(host, 'rename-library'))
    // The title holds its place, out of reach, and the box follows it.
    expect(title(host)?.textContent).toBe('Manga')
    expect(title(host)?.hasAttribute('inert')).toBe(true)
    const box = nameBox(host)
    expect(box.form?.previousElementSibling).toBe(title(host))
    expect(box.value).toBe('Manga')
    expect(document.activeElement).toBe(box)
    expect([box.selectionStart, box.selectionEnd]).toEqual([0, 'Manga'.length])
    expect(actions(host, 'shown')).toEqual([
      ['save-rename', 'Save'],
      ['cancel-rename', 'Cancel'],
    ])
    expect(actions(host, 'unseen')).toEqual([
      ['rename-library', 'Rename…'],
      ['scan-library', 'Scan library'],
    ])
    // Outside the box's form, and its submit all the same, so Enter in the
    // box presses it.
    expect(button(host, 'save-rename').form).toBe(box.form)
    expect(holds(host)).toBe('12 works in 2 directories')
  })

  it('closes the editor with Escape from the box, Save or Cancel, and with Cancel, putting focus back on Rename…, but holds both from the moment a save is sent', async () => {
    vi.mocked(api.renameLibrary).mockReturnValue(new Promise(() => {}))
    const { host } = await shown()
    const editor = () => host.querySelector('form.rename-library')
    for (const from of [
      nameBox,
      (h: HTMLElement) => button(h, 'save-rename'),
      (h: HTMLElement) => button(h, 'cancel-rename'),
    ]) {
      await press(button(host, 'rename-library'))
      await pressEscape(from(host))
      expect(editor()).toBeNull()
      expect(title(host)?.textContent).toBe('Manga')
      expect(title(host)?.hasAttribute('inert')).toBe(false)
      expect(actions(host, 'shown')[0]).toEqual(['rename-library', 'Rename…'])
      expect(document.activeElement).toBe(button(host, 'rename-library'))
    }

    await press(button(host, 'rename-library'))
    await press(button(host, 'cancel-rename'))
    expect(editor()).toBeNull()
    expect(document.activeElement).toBe(button(host, 'rename-library'))

    await press(button(host, 'rename-library'))
    await typeInto(nameBox(host), 'Comics')
    await submit(host.querySelector('form.rename-library') as Element)
    // Held before the wait is drawn: Cancel is not drawn held yet.
    expect(button(host, 'cancel-rename').disabled).toBe(false)
    await press(button(host, 'cancel-rename'))
    expect(nameBox(host)).not.toBeNull()
    for (const from of [
      nameBox,
      (h: HTMLElement) => button(h, 'save-rename'),
      (h: HTMLElement) => button(h, 'cancel-rename'),
    ]) {
      await pressEscape(from(host))
      expect(nameBox(host)).not.toBeNull()
    }
    await waitDrawn()
    expect(button(host, 'cancel-rename').disabled).toBe(true)
  })

  it('says so when it holds books and no work, and only then', async () => {
    const books = await shown(
      manga({ works: 0, books: 3, dirs: [dir(1, '/media/Books')] }),
    )
    expect(books.host.querySelector('.holds')?.textContent).toBe(
      '0 works in 1 directory',
    )
    expect(books.host.querySelector('.no-readable')?.textContent).toContain(
      'No readable files yet: its scans found 3 PDF or fixed-layout EPUB files',
    )
    const mixed = await shown(manga({ works: 2, books: 3 }))
    expect(mixed.host.querySelector('.no-readable')).toBeNull()
    const empty = await shown(manga({ works: 0, books: 0 }))
    expect(empty.host.querySelector('.no-readable')).toBeNull()
  })

  it('renames by id and reloads', async () => {
    vi.mocked(api.renameLibrary).mockResolvedValue({
      id: 7,
      name: 'Comics',
      match_by_content: true,
    })
    const { host, onChanged } = await shown()
    await press(button(host, 'rename-library'))
    const box = host.querySelector(
      'input[name="library-name"]',
    ) as HTMLInputElement
    expect(box.value).toBe('Manga')

    await typeInto(box, 'Comics')
    // Saved as a mouse saves, with focus on Save, which goes with the editor.
    const save = button(host, 'save-rename')
    act(() => save.focus())
    await press(save)

    expect(api.renameLibrary).toHaveBeenCalledWith(7, 'Comics')
    expect(onChanged).toHaveBeenCalled()
    expect(host.querySelector('form.rename-library')).toBeNull()
    // The title takes focus, and reads the name saved before the list read
    // again says it (`onChanged` here reads nothing).
    expect(document.activeElement).toBe(title(host))
    expect(title(host)?.textContent).toBe('Comics')
  })

  it('puts focus on the title when the editor closes by Escape or Cancel while the removal holds Rename…', async () => {
    vi.mocked(api.libraryRemovalCost).mockResolvedValue(counted(12))
    const { host } = await shown()
    const cancelRemoval = () =>
      press(
        [...host.querySelectorAll('.remove-library button')].find(
          (b) => b.textContent === 'Cancel',
        ) as Element,
      )
    for (const close of [
      () => pressEscape(nameBox(host)),
      () => press(button(host, 'cancel-rename')),
    ]) {
      await press(button(host, 'rename-library'))
      await press(button(host, 'remove-library'))
      await close()
      expect(host.querySelector('form.rename-library')).toBeNull()
      expect(button(host, 'rename-library').disabled).toBe(true)
      expect(document.activeElement).toBe(title(host))
      await cancelRemoval()
    }
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

    expect(renameRefusal(host)?.textContent).toBe(
      'a library is already called `Comics`',
    )
    expect(holds(host)).toBe('12 works in 2 directories')
    expect(host.querySelector('.library-page > [role="alert"]')).toBeNull()
    expect(host.querySelector('form.rename-library')).not.toBeNull()
    expect(onChanged).not.toHaveBeenCalled()
  })

  it('draws, under the header’s line and as it is typed, why a library cannot be renamed . or .., holds Save and sends nothing', async () => {
    // The server refuses both names once trimmed, since no address can hold
    // either; the page says so in the server's sentence before Save.
    const why = 'An address cannot hold “.” or “..” as a name.'
    expect(NO_ADDRESS).toBe(why)
    const { host } = await shown()
    await press(button(host, 'rename-library'))
    for (const name of ['..', ' . ']) {
      await typeInto(nameBox(host), name)
      expect(
        [...(editorAnswer(host)?.children ?? [])].map((a) => [
          a.getAttribute('role'),
          a.textContent,
        ]),
        JSON.stringify(name),
      ).toEqual([['status', why]])
      expect(button(host, 'save-rename').disabled).toBe(true)
    }
    await submit(host.querySelector('form.rename-library') as Element)
    expect(api.renameLibrary).not.toHaveBeenCalled()
  })

  it('says as it is typed that another library holds the name in another case, tied to the box, and holds Save at once until the name is free', async () => {
    const { host } = await shown(manga(), [comics()])
    await press(button(host, 'rename-library'))
    const note = clashNote(host) as HTMLElement
    // Drawn, empty, before it has anything to say, so that its words are
    // read out as they come.
    expect(note.textContent).toBe('')
    expect(boxMarks(host)).toEqual([null, null])

    await typeInto(nameBox(host), 'COMICS')
    expect(clashNote(host)).toBe(note)
    expect(note.textContent).toBe('a library is already called `Comics`')
    expect(boxMarks(host)).toEqual(['true', note.id])
    // At once: no wait is drawn for it.
    expect(button(host, 'save-rename').disabled).toBe(true)
    expect(button(host, 'save-rename').textContent).toBe('Save')
    expect(renameRefusal(host)).toBeNull()

    // A keystroke that keeps the clash changes nothing that is read out.
    const changes = changesIn(note)
    await typeInto(nameBox(host), 'comics')
    expect(changes()).toBe(0)

    await typeInto(nameBox(host), 'Comics 2')
    expect(clashNote(host)).toBe(note)
    expect(note.textContent).toBe('')
    expect(boxMarks(host)).toEqual([null, null])
    expect(button(host, 'save-rename').disabled).toBe(false)
  })

  it('sends nothing on Enter in the box while the name clashes', async () => {
    const { host } = await shown(manga(), [comics()])
    await press(button(host, 'rename-library'))
    await typeInto(nameBox(host), 'comics')
    await submit(host.querySelector('form.rename-library') as Element)
    expect(api.renameLibrary).not.toHaveBeenCalled()
    expect(host.querySelector('form.rename-library')).not.toBeNull()
  })

  it('says a taken name once when the list read after a refused rename learns of it', async () => {
    // A list gone stale: another admin took the name, and a scan's end then
    // reads the list again while the refusal is drawn.
    vi.mocked(api.renameLibrary).mockRejectedValue(
      new Error(
        '/api/library/libraries/7: 400 {"error":"a library is already called `Comics`"}',
      ),
    )
    const { host, redraw } = await shown()
    await press(button(host, 'rename-library'))
    await typeInto(nameBox(host), 'Comics')
    await submit(host.querySelector('form.rename-library') as Element)
    await redraw(manga(), [comics()])

    const said = () =>
      [...(editorAnswer(host)?.children ?? [])]
        .filter((one) => one.textContent !== '')
        .map((one) => [one.getAttribute('role'), one.textContent])
    expect(said()).toEqual([['alert', 'a library is already called `Comics`']])
    expect(boxMarks(host)).toEqual([null, null])
    expect(button(host, 'save-rename').disabled).toBe(true)

    // An edit takes the refusal away, and the list's clash is said then.
    await typeInto(nameBox(host), 'COMICS')
    expect(said()).toEqual([['status', 'a library is already called `Comics`']])
    expect(boxMarks(host)).toEqual(['true', clashNote(host)?.id])
  })

  it('says the clash a list read brings beside a rename refusal in other words, and holds Save with it', async () => {
    vi.mocked(api.renameLibrary).mockRejectedValue(
      new Error(
        '/api/library/libraries/7: 500 {"error":"renaming library 7: database is locked"}',
      ),
    )
    const { host, redraw } = await shown()
    await press(button(host, 'rename-library'))
    await typeInto(nameBox(host), 'Comics')
    await submit(host.querySelector('form.rename-library') as Element)
    await redraw(manga(), [comics()])

    expect(
      [...(editorAnswer(host)?.children ?? [])]
        .filter((one) => one.textContent !== '')
        .map((one) => [one.getAttribute('role'), one.textContent]),
    ).toEqual([
      ['alert', 'renaming library 7: database is locked'],
      ['status', 'a library is already called `Comics`'],
    ])
    expect(boxMarks(host)).toEqual(['true', clashNote(host)?.id])
    expect(button(host, 'save-rename').disabled).toBe(true)
  })

  it('moves focus from Save to the name box when a list read holds Save, so that Escape still closes the editor', async () => {
    const { host, redraw } = await shown()
    await press(button(host, 'rename-library'))
    await typeInto(nameBox(host), 'Comics')
    const save = button(host, 'save-rename')
    act(() => save.focus())
    expect(document.activeElement).toBe(save)
    // Another admin took the name; a scan's end reads the list again.
    await redraw(manga(), [comics()])
    expect(save.disabled).toBe(true)
    expect(document.activeElement).toBe(nameBox(host))
    await pressEscape(nameBox(host))
    expect(host.querySelector('form.rename-library')).toBeNull()
  })

  it('leaves focus on the editor’s Cancel when a list read holds Save', async () => {
    const { host, redraw } = await shown()
    await press(button(host, 'rename-library'))
    await typeInto(nameBox(host), 'Comics')
    const cancel = button(host, 'cancel-rename')
    act(() => cancel.focus())
    await redraw(manga(), [comics()])
    expect(button(host, 'save-rename').disabled).toBe(true)
    expect(document.activeElement).toBe(cancel)
  })

  it('moves focus from Save to the name box once the save’s wait is drawn, and keeps it there through a refusal, so that Escape still closes the editor', async () => {
    const sent = deferred<api.Patched>()
    vi.mocked(api.renameLibrary).mockReturnValue(sent.promise)
    const { host } = await shown()
    await press(button(host, 'rename-library'))
    await typeInto(nameBox(host), 'Comics')
    const save = button(host, 'save-rename')
    act(() => save.focus())
    await press(save)
    expect(document.activeElement).toBe(save)
    await waitDrawn()
    expect([save.disabled, save.textContent]).toEqual([true, 'Saving…'])
    expect(document.activeElement).toBe(nameBox(host))

    await flush(() =>
      sent.reject(
        new Error(
          '/api/library/libraries/7: 400 {"error":"a library is already called `Comics`"}',
        ),
      ),
    )
    expect(renameRefusal(host)?.textContent).toBe(
      'a library is already called `Comics`',
    )
    expect(document.activeElement).toBe(nameBox(host))
    await pressEscape(nameBox(host))
    expect(host.querySelector('form.rename-library')).toBeNull()
  })

  it('moves focus from the editor’s Cancel to the name box once the save’s wait holds it', async () => {
    const sent = deferred<api.Patched>()
    vi.mocked(api.renameLibrary).mockReturnValue(sent.promise)
    const { host } = await shown()
    await press(button(host, 'rename-library'))
    await typeInto(nameBox(host), 'Comics')
    await submit(host.querySelector('form.rename-library') as Element)
    // Tabbed to inside the wait's delay.
    const cancel = button(host, 'cancel-rename')
    act(() => cancel.focus())
    expect(document.activeElement).toBe(cancel)
    await waitDrawn()
    expect(cancel.disabled).toBe(true)
    expect(document.activeElement).toBe(nameBox(host))

    await flush(() =>
      sent.reject(
        new Error(
          '/api/library/libraries/7: 400 {"error":"a library is already called `Comics`"}',
        ),
      ),
    )
    expect(document.activeElement).toBe(nameBox(host))
    await pressEscape(nameBox(host))
    expect(host.querySelector('form.rename-library')).toBeNull()
  })

  it('puts focus on the title after a save whose wait was drawn, as after one answered at once', async () => {
    const sent = deferred<api.Patched>()
    vi.mocked(api.renameLibrary).mockReturnValue(sent.promise)
    const { host } = await shown()
    await press(button(host, 'rename-library'))
    await typeInto(nameBox(host), 'Comics')
    const save = button(host, 'save-rename')
    act(() => save.focus())
    await press(save)
    await waitDrawn()
    expect(document.activeElement).toBe(nameBox(host))

    await flush(() =>
      sent.resolve({ id: 7, name: 'Comics', match_by_content: true }),
    )
    expect(host.querySelector('form.rename-library')).toBeNull()
    expect(title(host)?.textContent).toBe('Comics')
    expect(document.activeElement).toBe(title(host))
  })

  it('takes the library’s own name, in any case, as no clash', async () => {
    vi.mocked(api.renameLibrary).mockResolvedValue({
      id: 7,
      name: 'MANGA',
      match_by_content: true,
    })
    const { host } = await shown(manga(), [comics()])
    await press(button(host, 'rename-library'))
    expect(nameBox(host).value).toBe('Manga')
    expect(clashNote(host)?.textContent).toBe('')
    await typeInto(nameBox(host), 'MANGA')
    expect(clashNote(host)?.textContent).toBe('')
    expect(boxMarks(host)).toEqual([null, null])
    expect(button(host, 'save-rename').disabled).toBe(false)
    await submit(host.querySelector('form.rename-library') as Element)
    expect(api.renameLibrary).toHaveBeenCalledWith(7, 'MANGA')
  })

  it('adds a directory to this library, hands its probe up, and reloads', async () => {
    const added: config.Added = {
      root: dir(3, '/media/Manga/Extras'),
      watching: true,
      watch_reason: null,
      interval_secs: 86400,
      also_in: [],
      schedule_kept: false,
    }
    vi.mocked(api.addDir).mockResolvedValue(added)
    const { host, onAdded, onChanged } = await shown()
    const picker = await openPicker(host)

    await typeInto(
      picker.querySelector('input[name="path"]') as HTMLInputElement,
      '/media/Manga/Extras',
    )
    await press(picker.querySelector('button[name="pick"]') as Element)

    expect(api.addDir).toHaveBeenCalledWith(7, '/media/Manga/Extras', false)
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

  it('starts one scan of the library, and draws it from the feed until it ends', async () => {
    vi.mocked(api.startLibraryScan).mockResolvedValue(started)
    const { host, onChanged } = await shown()
    await clock(0)
    feed.answer(runningFeed(whole()))

    act(() => {
      button(host, 'scan-library').click()
      button(host, 'scan-library').click()
    })
    expect(holdOf(button(host, 'scan-library'))).toBe('held')
    await clock(0)
    expect(api.startLibraryScan).toHaveBeenCalledTimes(1)
    expect(api.startLibraryScan).toHaveBeenCalledWith(7)
    expect(config.startScan).not.toHaveBeenCalled()
    expect(button(host, 'scan-library').textContent).toBe('Cancel')
    expect(holdOf(button(host, 'remove-library'))).toBe('held')
    expect(scanLines(host)).toEqual([
      'Reading files · 120 of 2,097 · directory 1 of 1',
    ])

    feed.answer(idleFeed(1))
    await clock(config.SCAN_POLL_MS)
    expect(scanLines(host)).toEqual([])
    expect([
      button(host, 'scan-library').textContent,
      holdOf(button(host, 'scan-library')),
      holdOf(button(host, 'remove-library')),
    ]).toEqual(['Scan library', 'free', 'free'])
    // The feed reads the list again, not the page.
    expect(onChanged).not.toHaveBeenCalled()
  })

  it('reads a 409 as a scan in flight: the sentence, and the feed asked at once', async () => {
    vi.mocked(api.startLibraryScan).mockRejectedValue(
      new Error(
        '/api/library/libraries/7/scan: 409 {"error":"a scan of /media/Manga/Chapters is already running"}',
      ),
    )
    const { host } = await shown()
    await clock(0)
    const polls = feed.polls()
    feed.answer(runningFeed(chapters()))
    await press(button(host, 'scan-library'))
    await clock(0)
    expect(
      host.querySelector('.library-page > [role="alert"]')?.textContent,
    ).toBe('a scan of /media/Manga/Chapters is already running')
    expect(feed.polls()).toBe(polls + 1)
    expect(button(host, 'scan-library').textContent).toBe('Cancel')
    expect(scanLines(host)).toEqual([
      'Reading files · 120 of 2,097 · directory 1 of 1',
    ])
  })

  it('opens the removal dialog, and holds the scan button while it is open, sending nothing from it', async () => {
    vi.mocked(api.libraryRemovalCost).mockReturnValue(new Promise(() => {}))
    const { host } = await shown()
    await press(button(host, 'remove-library'))
    expect(
      host.querySelector('.remove-library')?.getAttribute('aria-label'),
    ).toBe('Remove the library Manga')
    // Out of the tab order: focus is on Remove… as this hold comes on.
    expect(holdOf(button(host, 'scan-library'))).toBe('disabled')
    await press(button(host, 'scan-library'))
    expect(api.startLibraryScan).not.toHaveBeenCalled()
    expect(host.querySelector('.library-page > .scanning')).toBeNull()
  })

  it('hands up the real answer of a removal, by id and name, and does not reload itself', async () => {
    const answer = (works: number, dry_run: boolean): config.RemovalAnswer => ({
      removed: {
        works,
        positions: 0,
        notes: 0,
        positions_page_gone: 0,
        notes_page_gone: 0,
        left_with_no_library: [],
        libraries_removed: [],
        kept: [],
      },
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
    expect(onRemoved).toHaveBeenCalledWith(7, 'Manga', real)
    expect(onChanged).not.toHaveBeenCalled()
    expect(host.querySelector('.remove-library')).toBeNull()
  })

  it('holds Scan library and Remove while the scan is being started', async () => {
    vi.mocked(api.startLibraryScan).mockReturnValue(new Promise(() => {}))
    const { host } = await shown()
    await press(button(host, 'scan-library'))
    expect(holdOf(button(host, 'scan-library'))).toBe('held')
    // Out of the tab order: focus is on Scan library as this hold comes on.
    expect(holdOf(button(host, 'remove-library'))).toBe('disabled')
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
    expect(holdOf(button(host, 'scan-library'))).toBe('free')
  })

  it('sends one rename for two submits, and draws Save waiting only once the save has been out a while', async () => {
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
    const save = button(host, 'save-rename')
    expect([save.disabled, save.textContent]).toEqual([false, 'Save'])
    await waitDrawn()
    expect([save.disabled, save.textContent]).toEqual([true, 'Saving…'])
    await submit(form)
    expect(api.renameLibrary).toHaveBeenCalledTimes(1)
  })

  it('never draws Save waiting, nor Save or Cancel held, for a save answered at once', async () => {
    vi.mocked(api.renameLibrary).mockResolvedValue({
      id: 7,
      name: 'Comics',
      match_by_content: true,
    })
    const { host } = await shown()
    await press(button(host, 'rename-library'))
    await typeInto(nameBox(host), 'Comics')
    const save = button(host, 'save-rename')
    const cancel = button(host, 'cancel-rename')
    // Every state the two are drawn in, not only the last.
    const drawn: string[] = []
    const record = () =>
      drawn.push(
        `${save.textContent} ${save.disabled} ${cancel.textContent} ${cancel.disabled}`,
      )
    const watch = new MutationObserver(record)
    for (const el of [save, cancel]) {
      watch.observe(el, {
        attributes: true,
        childList: true,
        characterData: true,
        subtree: true,
      })
    }
    record()
    await submit(host.querySelector('form.rename-library') as Element)
    await waitDrawn()
    watch.disconnect()
    record()
    expect(api.renameLibrary).toHaveBeenCalledTimes(1)
    expect(host.querySelector('form.rename-library')).toBeNull()
    expect(new Set(drawn)).toEqual(new Set(['Save false Cancel false']))
  })

  it('lets a rename be saved while a scan is being started', async () => {
    vi.mocked(api.startLibraryScan).mockReturnValue(new Promise(() => {}))
    vi.mocked(api.renameLibrary).mockResolvedValue({
      id: 7,
      name: 'Comics',
      match_by_content: true,
    })
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
      also_in: [],
      schedule_kept: false,
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
})

const started: api.LibraryScanStarted = {
  library_id: 7,
  status_url: '/api/library/libraries/7/scan',
  root_ids: [1, 2],
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
  it('offers Cancel and holds Remove… while a directory is scanning from its own page, and sends nothing from Remove…', async () => {
    feed.answer(runningFeed(chapters()))
    const { host } = await shown()
    await clock(0)

    expect(button(host, 'scan-library').textContent).toBe('Cancel')
    expect(holdOf(button(host, 'scan-library'))).toBe('free')
    expect(holdOf(button(host, 'remove-library'))).toBe('held')
    await press(button(host, 'remove-library'))
    expect(api.startLibraryScan).not.toHaveBeenCalled()
    expect(host.querySelector('.remove-library')).toBeNull()
    expect(api.removeLibrary).not.toHaveBeenCalled()
  })

  it('frees the library once the directory’s scan has ended', async () => {
    feed.answer(runningFeed(chapters()))
    const { host } = await shown()
    await clock(0)
    expect(button(host, 'scan-library').textContent).toBe('Cancel')
    feed.answer(idleFeed(1))
    await clock(config.SCAN_POLL_MS)
    expect([
      button(host, 'scan-library').textContent,
      holdOf(button(host, 'scan-library')),
      holdOf(button(host, 'remove-library')),
    ]).toEqual(['Scan library', 'free', 'free'])
  })

  it('draws a Directories table: a row per directory, its path a link to the directory’s page, how often it is scanned and when it last was, and that one Off and never scanned waits for a press', async () => {
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
          { ...dir(5, '/media/Manga/Oneshots'), scan_interval_secs: 0 },
          { ...dir(6, '/media/Manga/Doujin'), scan_interval_secs: 5400 },
          { ...dir(7, '/media/Manga/Artbooks'), scan_interval_secs: 9000 },
        ],
      }),
    )
    const table = host.querySelector('table.dir-table') as HTMLElement
    expect(
      section(host, 'Directories').querySelector(
        ':scope > .settings-box > table.dir-table',
      ),
    ).toBe(table)
    expect(
      [...table.querySelectorAll('thead th')].map((th) => th.textContent),
    ).toEqual(['Path', 'Scan every', 'Last scan', 'Overlaps'])
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
        links: [['/media/Manga/Volumes', '/config/libraries/Manga/dirs/1']],
        cells: ['/media/Manga/Volumes', '24h', 'Never', ''],
      },
      {
        links: [['/media/Manga/Chapters', '/config/libraries/Manga/dirs/4']],
        cells: [
          '/media/Manga/Chapters',
          'automatic',
          '2026-10-01 10:00:00 UTC\u00a0·\u00a0failed',
          '',
        ],
      },
      {
        links: [['/media/Manga/Oneshots', '/config/libraries/Manga/dirs/5']],
        cells: [
          '/media/Manga/Oneshots',
          'off',
          'Never\u00a0·\u00a0until Scan is pressed',
          '',
        ],
      },
      {
        links: [['/media/Manga/Doujin', '/config/libraries/Manga/dirs/6']],
        cells: ['/media/Manga/Doujin', '90m', 'Never', ''],
      },
      {
        links: [['/media/Manga/Artbooks', '/config/libraries/Manga/dirs/7']],
        cells: ['/media/Manga/Artbooks', '150m', 'Never', ''],
      },
    ])
  })

  it('says under Overlaps how each directory overlaps another library’s, without naming one: Shared, Inside, Around, in that order, or nothing', async () => {
    const kids = { id: 8, name: 'Kids' }
    const around = (path: string): api.AroundDir => ({
      path,
      libraries: ['Everything'],
      shows: true,
      kept_out_by: null,
    })
    // Kept out of the outer directory's library: it still overlaps.
    const keptOut = (id: number, path: string): api.NestedDir => ({
      id,
      path,
      libraries: ['Kids'],
      shown: false,
      kept_out_by: 'exclusion',
    })
    const { host } = await shown(
      manga({
        dirs: [
          dir(1, '/media/Manga/Volumes', [{ id: 7, name: 'Manga' }, kids]),
          { ...dir(2, '/media/All/Chapters'), inside: [around('/media/All')] },
          {
            ...dir(3, '/media/Manga/Extras'),
            nested: [keptOut(30, '/media/Manga/Extras/Kids')],
          },
          {
            ...dir(4, '/media/All/Omnibus', [{ id: 7, name: 'Manga' }, kids]),
            inside: [around('/media/All')],
            nested: [keptOut(31, '/media/All/Omnibus/Kids')],
          },
          dir(5, '/srv/alone'),
        ],
      }),
    )
    // Per row, each cell read alone, so a word drawn in the wrong row is seen.
    const cells = [...host.querySelectorAll('.dir-table tbody tr')].map(
      (row) => row.querySelectorAll('td')[3],
    )
    expect(cells.map(cellValue)).toEqual([
      'Shared',
      'Inside',
      'Around',
      'Shared, Inside, Around',
      '',
    ])
    // Each word says its column's heading where the headings are not drawn;
    // a directory that overlaps nothing draws nothing, label and all.
    expect(
      cells.map((cell) => cell.querySelector('.cell-label')?.textContent),
    ).toEqual([
      'Overlaps: ',
      'Overlaps: ',
      'Overlaps: ',
      'Overlaps: ',
      undefined,
    ])
    expect(cells[4].childNodes).toHaveLength(0)
  })

  it('says Scanning… in the table for each directory being scanned, and its last scan once that ends', async () => {
    vi.mocked(api.startLibraryScan).mockResolvedValue(started)
    feed.answer(runningFeed(chapters()))
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
    await clock(0)
    expect(lastScans(host)).toEqual(['Never', 'Scanning…'])

    feed.answer(idleFeed(1))
    await clock(config.SCAN_POLL_MS)
    expect(lastScans(host)).toEqual(['Never', '2026-10-01 10:00:00 UTC'])

    // A scan of the library is a scan of every directory in it.
    feed.answer(runningFeed(whole()))
    await press(button(host, 'scan-library'))
    await clock(0)
    expect(lastScans(host)).toEqual(['Scanning…', 'Scanning…'])
  })

  it('says Scanning… and no "until Scan is pressed" for its Off directory never scanned while the scan pressed here runs (O275)', async () => {
    const offNever = manga({
      dirs: [
        { ...dir(1, '/media/Manga/Volumes'), scan_interval_secs: 0 },
        dir(2, '/media/Manga/Chapters'),
      ],
    })
    vi.mocked(api.startLibraryScan).mockResolvedValue(started)
    const { host } = await shown(offNever)
    await clock(0)
    expect(lastScans(host)).toEqual(['Never · until Scan is pressed', 'Never'])

    // The list the cells come from is not read again until the scan ends.
    feed.answer(runningFeed(whole()))
    await press(button(host, 'scan-library'))
    await clock(0)
    expect(lastScans(host)).toEqual(['Scanning…', 'Scanning…'])
    await clock(config.SCAN_POLL_MS)
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

  it('moves focus from Add directory to the path box as the add holds it, and keeps it there through a refusal', async () => {
    const sent = deferred<config.Added>()
    vi.mocked(api.addDir).mockReturnValue(sent.promise)
    const { host } = await shown()
    const picker = await openPicker(host)
    const box = picker.querySelector('input[name="path"]') as HTMLInputElement
    await typeInto(box, '/media/x')
    const pick = picker.querySelector(
      'button[name="pick"]',
    ) as HTMLButtonElement
    act(() => pick.focus())
    await press(pick)
    expect(api.addDir).toHaveBeenCalledWith(7, '/media/x', false)
    expect(pick.disabled).toBe(true)
    expect(document.activeElement).toBe(box)

    await flush(() =>
      sent.reject(
        new Error(
          '/api/library/libraries/7/dirs: 400 {"error":"/media/x: No such file or directory (os error 2)"}',
        ),
      ),
    )
    expect(picker.querySelector('[role="alert"]')?.textContent).toBe(
      '/media/x: No such file or directory (os error 2)',
    )
    expect(box.value).toBe('/media/x')
    expect(document.activeElement).toBe(box)
  })

  it('closes the picker once a directory is added, and the directory joins the table when the list is read again', async () => {
    const added: config.Added = {
      root: dir(3, '/media/Manga/Extras'),
      watching: true,
      watch_reason: null,
      interval_secs: 86400,
      also_in: [],
      schedule_kept: false,
    }
    vi.mocked(api.addDir).mockResolvedValue(added)
    const before = manga()
    const after = manga({
      dirs: [
        ...before.dirs,
        {
          ...added.root,
          libraries: [{ id: 7, name: 'Manga' }],
          nested: [],
          inside: [],
        },
      ],
    })
    // The screen's reload, which hands the page the list read again.
    function Reloading() {
      const [library, setLibrary] = useState(before)
      return (
        <MemoryRouter initialEntries={['/config/libraries/Manga']}>
          <LibraryPage
            library={library}
            libraries={[library]}
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
      ['/media/Manga/Volumes', '/config/libraries/Manga/dirs/1'],
      ['/media/Manga/Chapters', '/config/libraries/Manga/dirs/2'],
      ['/media/Manga/Extras', '/config/libraries/Manga/dirs/3'],
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
        taken_over: 0,
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
    await clock(0)
    expect(sectionTitles(host)).toEqual(['Directories', 'Matching', 'Removal'])
    feed.answer(runningFeed(whole()))
    await press(button(host, 'scan-library'))
    await clock(config.SCAN_POLL_MS)
    expect(sectionTitles(host)).toEqual(['Directories', 'Matching', 'Removal'])

    feed.answer(idleFeed(1))
    await clock(config.SCAN_POLL_MS)

    // Under the directories, its heading an h3 and each directory's path an
    // h4 beneath it.
    expect(sectionTitles(host)).toEqual([
      'Directories',
      'What the scan found',
      'Matching',
      'Removal',
    ])
    const found = section(host, 'What the scan found')
    expect(found.querySelector('h3')?.textContent).toBe('What the scan found')
    const reports = [
      ...found.querySelectorAll(':scope > .settings-box > .dir-report'),
    ]
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
          taken_over: 0,
        },
        derive: emptyDerive,
      },
      last_scan_at: '2026-10-01 10:00:00',
      last_scan_status: 'ok',
    }),
  )

/** The paths the reports of the last scan are drawn under, in order. */
const reportedPaths = (host: HTMLElement) =>
  sectionTitles(host).includes('What the scan found')
    ? [
        ...section(host, 'What the scan found').querySelectorAll(
          '.dir-report > h4',
        ),
      ].map((h) => h.textContent)
    : []

describe('LibraryPage, whose reports a scan draws', () => {
  it('draws no report for a directory that joined the library while the scan ran, though its status holds one', async () => {
    vi.mocked(api.startLibraryScan).mockResolvedValue(started)
    reporting()
    const { host, redraw } = await shown()
    await clock(0)
    feed.answer(runningFeed(whole()))
    await press(button(host, 'scan-library'))
    await clock(0)
    await redraw(
      manga({
        dirs: [
          dir(1, '/media/Manga/Volumes'),
          dir(2, '/media/Manga/Chapters'),
          dir(3, '/media/Manga/Extras'),
        ],
      }),
    )
    feed.answer(idleFeed(1))
    await clock(config.SCAN_POLL_MS)
    expect(reportedPaths(host)).toEqual([
      '/media/Manga/Volumes',
      '/media/Manga/Chapters',
    ])
  })

  it('draws only the report of the directory whose own scan was running as it was drawn', async () => {
    reporting()
    feed.answer(
      runningFeed(
        feedJob({ id: 9, dirs: [{ id: 1, path: '/media/Manga/Volumes' }] }),
      ),
    )
    const { host } = await shown()
    await clock(0)
    feed.answer(idleFeed(1))
    await clock(config.SCAN_POLL_MS)
    expect(reportedPaths(host)).toEqual(['/media/Manga/Volumes'])
  })

  it('draws the reports of the directories the feed says the scan a 409 named holds', async () => {
    vi.mocked(api.startLibraryScan).mockRejectedValue(
      new Error(
        '/api/library/libraries/7/scan: 409 {"error":"a scan of /media/Manga/Chapters is already running"}',
      ),
    )
    reporting()
    const { host } = await shown()
    await clock(0)
    feed.answer(runningFeed(chapters()))
    await press(button(host, 'scan-library'))
    await clock(0)
    feed.answer(idleFeed(1))
    await clock(config.SCAN_POLL_MS)
    expect(reportedPaths(host)).toEqual(['/media/Manga/Chapters'])
  })

  it('draws the reports of the directories each scan held, not those an earlier scan held', async () => {
    vi.mocked(api.startLibraryScan).mockResolvedValueOnce(started)
    reporting()
    const { host } = await shown()
    await clock(0)
    feed.answer(runningFeed(whole()))
    await press(button(host, 'scan-library'))
    await clock(0)
    feed.answer(idleFeed(1))
    await clock(config.SCAN_POLL_MS)
    expect(reportedPaths(host)).toHaveLength(2)

    feed.answer(runningFeed(chapters()))
    await clock(FEED_IDLE_MS)
    feed.answer(idleFeed(2))
    await clock(config.SCAN_POLL_MS)
    expect(reportedPaths(host)).toEqual(['/media/Manga/Chapters'])
  })
})

/**
 * Manga with Kids' (library 3) `/media/Manga/Volumes/Kids`, directory 5, inside
 * its Volumes, and Tiny's (library 9) `/media/Manga/Chapters/Tiny`, directory 6,
 * inside its Chapters, which Manga's patterns keep out.
 */
const nesting = () => {
  const library = manga()
  library.dirs[0].nested = [
    {
      id: 5,
      path: '/media/Manga/Volumes/Kids',
      libraries: ['Kids'],
      shown: true,
      kept_out_by: null,
    },
  ]
  library.dirs[1].nested = [
    {
      id: 6,
      path: '/media/Manga/Chapters/Tiny',
      libraries: ['Tiny'],
      shown: false,
      kept_out_by: 'exclusion',
    },
  ]
  return library
}

/** Manga's status, naming each directory its scan walks, `ids` scanning. */
const walking = (...ids: number[]): api.LibraryScanStatus => {
  const own = ids.filter((id) => id !== 5)
  return {
    library_id: 7,
    held_removals: [],
    moves_held: false,
    derive_running: false,
    // Manga's scan holds its own directories; Kids' own scan holds 5 alone.
    scans: [
      ...(own.length > 0
        ? [{ library_id: 7, library: 'Manga', root_ids: ids }]
        : []),
      ...(own.length === 0 && ids.includes(5)
        ? [{ library_id: 3, library: 'Kids', root_ids: [5] }]
        : []),
    ],
    running: ids.length > 0,
    elapsed_secs: ids.length > 0 ? 3 : null,
    dirs: [1, 2, 5].map((id) => ({ root_id: id, running: ids.includes(id) })),
  }
}

describe('LibraryPage, a directory inside one of its own', () => {
  it('draws the report of each directory its scan walks, one inside its own among them, and none for one it does not show', async () => {
    vi.mocked(api.startLibraryScan).mockResolvedValue({
      ...started,
      root_ids: [1, 2, 5],
    })
    reporting()
    const { host } = await shown(nesting())
    // Over before the first poll, so only the directories the press took
    // to be walking can be reported.
    vi.mocked(api.libraryScanStatus).mockResolvedValue(walking())
    await press(button(host, 'scan-library'))
    await tick()
    await flush()
    expect(reportedPaths(host)).toEqual([
      '/media/Manga/Volumes',
      '/media/Manga/Chapters',
      '/media/Manga/Volumes/Kids',
    ])
  })

  it('names the library whose scan of a folder inside its own is running, offers Cancel, and draws its report once that ends', async () => {
    reporting()
    feed.answer(
      runningFeed(
        feedJob({
          id: 9,
          dirs: [{ id: 5, path: '/media/Manga/Volumes/Kids' }],
          libraries: [
            { id: 3, name: 'Kids' },
            { id: 7, name: 'Manga' },
          ],
        }),
      ),
    )
    const { host } = await shown(nesting())
    await clock(0)
    expect(scanLines(host)).toEqual([
      'Reading files · 120 of 2,097 · directory 1 of 1',
      'This scan covers Kids too.',
    ])
    expect(button(host, 'scan-library').textContent).toBe('Cancel')
    // The table lists its own directories, and neither is scanning.
    expect(lastScans(host)).toEqual(['Never', 'Never'])

    feed.answer(idleFeed(1))
    await clock(config.SCAN_POLL_MS)
    expect(reportedPaths(host)).toEqual(['/media/Manga/Volumes/Kids'])
  })

  it('holds Remove… alone while its status names a removal held by a scan around one of its directories, and frees it once a later read names none', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      held_removals: [1],
      moves_held: false,
    })
    const { host } = await shown()
    expect(holdOf(button(host, 'remove-library'))).toBe('held')
    expect(holdOf(button(host, 'scan-library'))).toBe('free')
    expect(host.querySelector('.library-page > .scanning')).toBeNull()

    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    await flush()
    expect(holdOf(button(host, 'remove-library'))).toBe('free')
  })

  /** The Removal box's one row, and the help its label has. */
  const removalHelp = (host: HTMLElement) =>
    helpOf(row(section(host, 'Removal'), 'Remove this library'))?.textContent

  it('says in Remove…’s help why it is held while a scan around one of its directories holds it, and its words of today once a later read names none (O250)', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      held_removals: [1],
    })
    const { host } = await shown()
    expect(holdOf(button(host, 'remove-library'))).toBe('held')
    expect(removalHelp(host)).toBe(
      "Waits for a scan of a directory around or inside one of Manga's directories.",
    )

    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    await flush()
    expect(holdOf(button(host, 'remove-library'))).toBe('free')
    expect(removalHelp(host)).toBe('Asks first, with what it would take.')
  })

  it('describes Remove… by its row’s help, held or not (O250)', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      held_removals: [1],
    })
    const { host } = await shown()
    /** What Remove…'s `aria-describedby` names, read through the id. */
    const described = () => {
      const named = document.getElementById(
        button(host, 'remove-library').getAttribute('aria-describedby') ?? '',
      )
      expect(named).toBe(
        helpOf(row(section(host, 'Removal'), 'Remove this library')),
      )
      return named?.textContent
    }
    expect(described()).toBe(
      "Waits for a scan of a directory around or inside one of Manga's directories.",
    )
    // The row's words label nothing: the button is named by its own.
    expect(section(host, 'Removal').querySelector('label')).toBeNull()

    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    await flush()
    expect(described()).toBe('Asks first, with what it would take.')
  })

  it('keeps Remove…’s words of today while the library’s own scan holds it beside a removal held around one of its directories, and says why once only that holds it (O250)', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      held_removals: [1],
    })
    feed.answer(runningFeed(whole()))
    const { host } = await shown()
    await clock(0)
    expect(holdOf(button(host, 'remove-library'))).toBe('held')
    expect(removalHelp(host)).toBe('Asks first, with what it would take.')

    // The library's scan ends with the removal still held: the words come.
    feed.answer(idleFeed(1))
    await clock(config.SCAN_POLL_MS)
    expect(holdOf(button(host, 'remove-library'))).toBe('held')
    expect(removalHelp(host)).toBe(
      "Waits for a scan of a directory around or inside one of Manga's directories.",
    )
  })

  it('keeps Remove… held through two failed reads of a held removal, and frees it on the third, saying why', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValueOnce({
      ...idle(7),
      held_removals: [1],
    })
    const { host } = await shown()
    vi.mocked(api.libraryScanStatus).mockRejectedValue(
      new Error('/api/library/libraries/7/scan: 500 the store is locked'),
    )
    await tick()
    await flush()
    await tick()
    await flush()
    expect(holdOf(button(host, 'remove-library'))).toBe('held')
    expect(host.querySelector('.library-page > [role="alert"]')).toBeNull()

    await tick()
    await flush()
    expect(holdOf(button(host, 'remove-library'))).toBe('free')
    expect(
      host.querySelector('.library-page > [role="alert"]')?.textContent,
    ).toContain('the store is locked')
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
    await clock(0)
    feed.answer(runningFeed(chapters()))
    await press(button(host, 'scan-library'))
    await clock(config.SCAN_POLL_MS)
    expect(pageAlert(host)?.textContent).toBe(
      'a scan of /media/Manga/Chapters is already running',
    )
    expect(scanLines(host)).toHaveLength(1)

    feed.answer(idleFeed(1))
    await clock(config.SCAN_POLL_MS)
    expect(pageAlert(host)).toBeNull()
    expect(scanLines(host)).toEqual([])
    expect(holdOf(button(host, 'scan-library'))).toBe('free')
  })

  it('draws a failed scan start as the server’s sentence', async () => {
    vi.mocked(api.startLibraryScan).mockRejectedValueOnce(
      new Error('/api/library/libraries/7/scan: 500 {"error":"no disk"}'),
    )
    const { host } = await shown()
    await press(button(host, 'scan-library'))
    expect(pageAlert(host)?.textContent).toBe('no disk')
  })

  it('draws a rename refusal under the header’s line, beneath the count it leaves in place, until the name is edited or the editor is cancelled', async () => {
    vi.mocked(api.renameLibrary).mockRejectedValue(
      new Error('/api/library/libraries/7: 400 {"error":"taken"}'),
    )
    const { host } = await shown()
    await press(button(host, 'rename-library'))
    const form = () => host.querySelector('form.rename-library') as HTMLElement
    const box = nameBox(host)
    await typeInto(box, 'Comics')
    await submit(form())
    expect(renameRefusal(host)?.textContent).toBe('taken')
    expect(holds(host)).toBe('12 works in 2 directories')

    await typeInto(box, 'Comics Two')
    expect(renameRefusal(host)).toBeNull()
    expect(holds(host)).toBe('12 works in 2 directories')

    await submit(form())
    expect(renameRefusal(host)).not.toBeNull()
    await press(button(host, 'cancel-rename'))
    expect(host.querySelector('form.rename-library')).toBeNull()
    expect(holds(host)).toBe('12 works in 2 directories')
    await press(button(host, 'rename-library'))
    expect(renameRefusal(host)).toBeNull()
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
    await typeInto(nameBox(host), 'Comics')
    await submit(form())
    expect(renameRefusal(host)).not.toBeNull()
    await submit(form())
    expect(renameRefusal(host)).toBeNull()
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

  it('hands focus back to Remove… when the library’s removal confirm is cancelled', async () => {
    vi.mocked(api.libraryRemovalCost).mockResolvedValue(counted(12))
    const { host } = await shown()
    await press(button(host, 'remove-library'))
    const cancel = [...host.querySelectorAll('.remove-library button')].find(
      (b) => b.textContent === 'Cancel',
    ) as HTMLButtonElement
    act(() => cancel.focus())
    await press(cancel)
    expect(host.querySelector('.remove-library')).toBeNull()
    expect(document.activeElement).toBe(button(host, 'remove-library'))
  })

  it('closes the library’s removal dialog with Cancel, and the name’s editor with Cancel', async () => {
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
    await press(button(host, 'cancel-rename'))
    expect(host.querySelector('form.rename-library')).toBeNull()
  })
})

describe('LibraryPage, scans and removals of a library and its directories exclude each other', () => {
  it('holds the library’s removal when the feed finds a directory scanning under its open dialog, and asks its cost again when that ends', async () => {
    vi.mocked(api.libraryRemovalCost).mockResolvedValue(counted(12))
    const { host } = await shown()
    await clock(0)
    await press(button(host, 'remove-library'))
    const confirm = () =>
      host.querySelector('.remove-library .confirm') as HTMLButtonElement
    expect(holdOf(confirm())).toBe('free')

    feed.answer(runningFeed(chapters()))
    await clock(FEED_IDLE_MS)
    expect(holdOf(confirm())).toBe('held')
    confirm().click()
    await flush()
    expect(api.removeLibrary).not.toHaveBeenCalled()
    expect(api.libraryRemovalCost).toHaveBeenCalledTimes(1)

    // The scan ends; what removing would cost is asked for again, and no
    // confirm stands over the figures from before it.
    const second = deferred<config.RemovalAnswer>()
    vi.mocked(api.libraryRemovalCost).mockReturnValueOnce(second.promise)
    feed.answer(idleFeed(1))
    await clock(config.SCAN_POLL_MS)
    expect(api.libraryRemovalCost).toHaveBeenCalledTimes(2)
    expect(host.querySelector('.remove-library .counts')).toBeNull()
    expect(host.querySelector('.remove-library .confirm')).toBeNull()

    await flush(() => second.resolve(counted(9)))
    expect(
      host.querySelector('.remove-library .counts')?.textContent,
    ).toContain('9 works')
    expect(holdOf(confirm())).toBe('free')
  })

  it('holds the library’s removal confirm when the read a return sends names a removal held by a scan around it', async () => {
    vi.mocked(api.libraryRemovalCost).mockResolvedValue(counted(12))
    const { host, again: late } = await returned()
    await press(button(host, 'remove-library'))
    const confirm = () =>
      host.querySelector('.remove-library .confirm') as HTMLButtonElement
    expect(holdOf(confirm())).toBe('free')

    await flush(() => late.resolve({ ...idle(7), held_removals: [1] }))
    expect(holdOf(confirm())).toBe('held')
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

    const save = button(host, 'save-rename')
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
      '/config/libraries/Manga/dirs/1',
      '/config/libraries/Manga/dirs/2',
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
      '/config/libraries/Manga/dirs/1',
      '/config/libraries/Manga/dirs/2',
    ])
  })

  it('does not hold Rename, Save or Add directory for a scan', async () => {
    feed.answer(
      runningFeed(
        feedJob({ id: 9, dirs: [{ id: 1, path: '/media/Manga/Volumes' }] }),
      ),
    )
    vi.mocked(api.renameLibrary).mockResolvedValue({
      id: 7,
      name: 'Comics',
      match_by_content: true,
    })
    vi.mocked(api.addDir).mockResolvedValue({
      root: dir(3, '/media/Manga/Extras'),
      watching: true,
      watch_reason: null,
      interval_secs: 86400,
      also_in: [],
      schedule_kept: false,
    })
    const { host } = await shown()
    await clock(0)
    expect(scanLines(host)).toHaveLength(1)

    await press(button(host, 'rename-library'))
    await typeInto(
      host.querySelector('form.rename-library input') as HTMLInputElement,
      'Comics',
    )
    const save = button(host, 'save-rename')
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
    expect(api.addDir).toHaveBeenCalledWith(7, '/media/Manga/Extras', false)
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
    vi.mocked(api.renameLibrary).mockResolvedValue({
      id: 7,
      name: 'Comics',
      match_by_content: true,
    })
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

  // Scan library is not drawn while the name is edited, so the scan is one
  // the page took up on being drawn, and it ends under the refusal.
  it('keeps a rename’s refusal through the end of a scan of the library', async () => {
    feed.answer(runningFeed(whole()))
    vi.mocked(api.renameLibrary).mockRejectedValue(
      new Error('/api/library/libraries/7: 400 {"error":"taken"}'),
    )
    const { host } = await shown()
    await clock(0)
    await press(button(host, 'rename-library'))
    await typeInto(nameBox(host), 'Comics')
    await submit(host.querySelector('form.rename-library') as Element)
    expect(renameRefusal(host)?.textContent).toBe('taken')

    feed.answer(idleFeed(1))
    await clock(config.SCAN_POLL_MS)
    expect(scanLines(host)).toEqual([])
    expect(renameRefusal(host)?.textContent).toBe('taken')
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

describe('LibraryPage, Don’t scan yet (schedule Off) beside Add directory — O275', () => {
  const extras: config.Added = {
    root: dir(3, '/media/Manga/Extras'),
    watching: true,
    watch_reason: null,
    interval_secs: 86400,
    also_in: [],
    schedule_kept: false,
  }
  /** The add's box, found by its label's words; `undefined` while not drawn. */
  const scanOffBox = (host: HTMLElement) =>
    [...host.querySelectorAll<HTMLLabelElement>('.add-root label')].find(
      (label) => label.textContent === 'Don’t scan yet (schedule Off)',
    )?.control as HTMLInputElement | undefined
  const pathBox = (picker: HTMLElement) =>
    picker.querySelector('input[name="path"]') as HTMLInputElement
  const pick = (picker: HTMLElement) =>
    press(picker.querySelector('button[name="pick"]') as Element)

  it('draws the box only while the picker is open, and unticked as it opens', async () => {
    const { host } = await shown()
    expect(scanOffBox(host)).toBeUndefined()
    await openPicker(host)
    const box = scanOffBox(host)
    expect([box?.type, box?.name, box?.checked]).toEqual([
      'checkbox',
      'scan-off',
      false,
    ])
  })

  it.each([
    { ticked: false, sent: false },
    { ticked: true, sent: true },
  ])(
    'adds the directory with the box ticked $ticked, and asks for the schedule Off $sent',
    async ({ ticked, sent }) => {
      vi.mocked(api.addDir).mockResolvedValue(extras)
      const { host } = await shown()
      const picker = await openPicker(host)
      if (ticked) await press(scanOffBox(host) as Element)
      expect(scanOffBox(host)?.checked).toBe(ticked)
      await typeInto(pathBox(picker), '/media/Manga/Extras')
      await pick(picker)
      expect(api.addDir).toHaveBeenCalledWith(7, '/media/Manga/Extras', sent)
    },
  )

  it('draws the box unticked again when Add directory… is pressed after a ticked add', async () => {
    vi.mocked(api.addDir).mockResolvedValue(extras)
    const { host } = await shown()
    const picker = await openPicker(host)
    await press(scanOffBox(host) as Element)
    await typeInto(pathBox(picker), '/media/Manga/Extras')
    await pick(picker)
    expect(api.addDir).toHaveBeenCalledWith(7, '/media/Manga/Extras', true)
    expect(scanOffBox(host)).toBeUndefined()

    await openPicker(host)
    expect(scanOffBox(host)?.checked).toBe(false)
  })

  it('draws the box unticked again when Add directory… is pressed after Cancel', async () => {
    const { host } = await shown()
    await openPicker(host)
    await press(scanOffBox(host) as Element)
    expect(scanOffBox(host)?.checked).toBe(true)
    await press(button(host, 'cancel-add'))
    expect(scanOffBox(host)).toBeUndefined()

    await openPicker(host)
    expect(scanOffBox(host)?.checked).toBe(false)
  })

  it('keeps the box ticked through a refused add, and a tick or an untick takes the refusal away', async () => {
    vi.mocked(api.addDir).mockRejectedValue(
      new Error('/api/library/libraries/7/dirs: 400 {"error":"nope"}'),
    )
    const { host } = await shown()
    const picker = await openPicker(host)
    const refused = () => picker.querySelector('[role="alert"]')
    await press(scanOffBox(host) as Element)
    await typeInto(pathBox(picker), '/media/typo')
    await pick(picker)
    expect(api.addDir).toHaveBeenLastCalledWith(7, '/media/typo', true)
    expect(refused()?.textContent).toBe('nope')
    expect(scanOffBox(host)?.checked).toBe(true)
    expect(pathBox(picker).value).toBe('/media/typo')

    await press(scanOffBox(host) as Element)
    expect(scanOffBox(host)?.checked).toBe(false)
    expect(refused()).toBeNull()

    await pick(picker)
    expect(api.addDir).toHaveBeenLastCalledWith(7, '/media/typo', false)
    expect(refused()?.textContent).toBe('nope')
    await press(scanOffBox(host) as Element)
    expect(scanOffBox(host)?.checked).toBe(true)
    expect(refused()).toBeNull()
  })
})

/**
 * What the page draws of a scan, and whether its body is drawn at all: the
 * line saying one is running, each directory's Last scan, and how Scan
 * library and Remove… are held.
 */
const scanDrawn = () => {
  const page = document.querySelector<HTMLElement>('.library-page')
  if (page?.querySelector('.dir-table') == null) return 'no body'
  return JSON.stringify([
    page.querySelector(':scope > .scanning')?.textContent ?? null,
    lastScans(page),
    button(page, 'scan-library').textContent,
    holdOf(button(page, 'scan-library')),
    holdOf(button(page, 'remove-library')),
  ])
}

describe('LibraryPage, its scan status as it is drawn', () => {
  it('draws the line of a scan the feed shows, Scanning… and Cancel in the same render as the rest of its body', async () => {
    feed.answer(runningFeed(chapters()))
    const read = deferred<api.LibraryScanStatus>()
    vi.mocked(api.libraryScanStatus).mockReturnValueOnce(read.promise)
    const drawn = watchDrawn(scanDrawn)
    await shown()
    await clock(0)
    await flush(() => read.resolve(idle(7)))
    drawn.stop()
    expect([...drawn.seen].filter((one) => one !== 'no body')).toEqual([
      JSON.stringify([
        'Reading files · 120 of 2,097 · directory 1 of 1',
        ['Never', 'Scanning…'],
        'Cancel',
        'free',
        'held',
      ]),
    ])
  })

  it('draws its body as no scan running, and no alert, when its scan status cannot be read, and polls nothing', async () => {
    vi.mocked(api.libraryScanStatus).mockRejectedValue(
      new Error('/api/library/libraries/7/scan: 500 the store is locked'),
    )
    const { host } = await shown()
    expect(scanDrawn()).toBe(
      JSON.stringify([
        null,
        ['Never', 'Never'],
        'Scan library',
        'free',
        'free',
      ]),
    )
    expect(host.querySelector('.library-page [role="alert"]')).toBeNull()
    await tick()
    await tick()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(1)
  })

  it('draws its body at once on a return, with the scan the feed’s answer kept shows, and reads the status again', async () => {
    feed.answer(runningFeed(chapters()))
    await shown()
    await clock(0)
    act(() => rerender(<div />))
    vi.mocked(api.libraryScanStatus).mockClear()
    vi.mocked(api.libraryScanStatus).mockReturnValueOnce(new Promise(() => {}))

    const drawn = watchDrawn(scanDrawn)
    await shown()
    drawn.stop()
    expect([...drawn.seen]).toEqual([
      'no body',
      JSON.stringify([
        'Reading files · 120 of 2,097 · directory 1 of 1',
        ['Never', 'Scanning…'],
        'Cancel',
        'free',
        'held',
      ]),
    ])
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(1)
  })
})

describe('LibraryPage, its removal refused (O252)', () => {
  /** The client's error for a refusal of the dry run or of the removal. */
  const refusal = (write: boolean, status: number, body: string) =>
    new Error(
      `/api/library/libraries/7${write ? '' : '?dry_run=true'}: ${status} ${body}`,
    )
  const HELD =
    '{"error":"a scan of /media/Manga/Volumes is running; remove the library once it has finished"}'

  /** Opens Remove…'s dialog and, for the removal, presses its confirm. */
  const refuse = async (host: HTMLElement, write: boolean) => {
    await press(button(host, 'remove-library'))
    if (write) {
      await press(host.querySelector('.remove-library .confirm') as Element)
    }
    await land()
  }

  const cancelRemoval = (host: HTMLElement) =>
    press(
      [...host.querySelectorAll('.remove-library button')].find(
        (b) => b.textContent === 'Cancel',
      ) as Element,
    )

  /** What Remove…'s `aria-describedby` names, read through the id. */
  const described = (host: HTMLElement) => {
    const named = document.getElementById(
      button(host, 'remove-library').getAttribute('aria-describedby') ?? '',
    )
    expect(named).toBe(
      helpOf(row(section(host, 'Removal'), 'Remove this library')),
    )
    return named?.textContent
  }

  /** Every alert on the page that says something. */
  const alerts = (host: HTMLElement) =>
    [...host.querySelectorAll('[role="alert"]')]
      .map((a) => a.textContent)
      .filter((words) => words !== '')

  it.each([
    { at: 'its dry run', write: false },
    { at: 'the removal', write: true },
  ])(
    'reads its status again when $at is refused for a hold, so that after Cancel Remove… is held and says why until a later read names no hold',
    async ({ write }) => {
      vi.mocked(api.libraryRemovalCost).mockResolvedValue(counted(12))
      if (!write) {
        vi.mocked(api.libraryRemovalCost).mockRejectedValue(
          refusal(write, 409, HELD),
        )
      }
      vi.mocked(api.removeLibrary).mockRejectedValue(refusal(write, 409, HELD))
      const { host } = await shown()
      expect(api.libraryScanStatus).toHaveBeenCalledTimes(1)
      vi.mocked(api.libraryScanStatus).mockResolvedValue({
        ...idle(7),
        held_removals: [1],
      })

      await refuse(host, write)
      expect(api.libraryScanStatus).toHaveBeenCalledTimes(2)
      // The dialog draws the refusal, and the page no second copy of it.
      expect(alerts(host)).toEqual([
        'a scan of /media/Manga/Volumes is running; remove the library once it has finished',
      ])
      expect(host.querySelector('.remove-library .confirm')).toBeNull()

      await cancelRemoval(host)
      expect(host.querySelector('.remove-library')).toBeNull()
      expect(holdOf(button(host, 'remove-library'))).toBe('held')
      expect(described(host)).toBe(
        "Waits for a scan of a directory around or inside one of Manga's directories.",
      )

      // Polled as a hold the page reads on arrival is, until it ends.
      vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
      await tick()
      await flush()
      expect(api.libraryScanStatus).toHaveBeenCalledTimes(3)
      expect(holdOf(button(host, 'remove-library'))).toBe('free')
      expect(described(host)).toBe('Asks first, with what it would take.')
    },
  )

  it.each([
    { at: 'its dry run', write: false },
    { at: 'the removal', write: true },
  ])(
    'reads no status when $at fails for anything but a hold',
    async ({ write }) => {
      const failure = refusal(write, 500, '{"error":"the disk is full"}')
      vi.mocked(api.libraryRemovalCost).mockResolvedValue(counted(12))
      if (!write) {
        vi.mocked(api.libraryRemovalCost).mockRejectedValue(failure)
      }
      vi.mocked(api.removeLibrary).mockRejectedValue(failure)
      const { host } = await shown()

      await refuse(host, write)
      expect(alerts(host)).toEqual(['the disk is full'])
      expect(api.libraryScanStatus).toHaveBeenCalledTimes(1)

      await cancelRemoval(host)
      expect(holdOf(button(host, 'remove-library'))).toBe('free')
      expect(described(host)).toBe('Asks first, with what it would take.')
      await tick()
      expect(api.libraryScanStatus).toHaveBeenCalledTimes(1)
    },
  )

  it('draws the hold again when its dry run is refused for a hold the page gave up after failed reads, and the status read again says it in the words it last read', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValueOnce({
      ...idle(7),
      held_removals: [1],
    })
    vi.mocked(api.libraryRemovalCost).mockRejectedValue(
      refusal(false, 409, HELD),
    )
    const { host } = await shown()
    vi.mocked(api.libraryScanStatus).mockRejectedValue(
      new Error('/api/library/libraries/7/scan: 500 the store is locked'),
    )
    for (const _ of [1, 2, 3]) {
      await tick()
      await flush()
    }
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(4)
    expect(holdOf(button(host, 'remove-library'))).toBe('free')

    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      held_removals: [1],
    })
    await refuse(host, false)
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(5)
    await cancelRemoval(host)
    expect(holdOf(button(host, 'remove-library'))).toBe('held')
    expect(described(host)).toBe(
      "Waits for a scan of a directory around or inside one of Manga's directories.",
    )

    // Polled again, as any hold the page draws is, until it ends.
    await tick()
    await flush()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(6)
    expect(holdOf(button(host, 'remove-library'))).toBe('held')
    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    await flush()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(7)
    expect(holdOf(button(host, 'remove-library'))).toBe('free')
    expect(described(host)).toBe('Asks first, with what it would take.')
  })

  it('draws no hold from the status read after a refusal when a newer read, answered first, named none', async () => {
    vi.mocked(api.libraryRemovalCost).mockRejectedValue(
      refusal(false, 409, HELD),
    )
    const { host, redraw } = await shown()
    vi.mocked(api.libraryScanStatus).mockResolvedValueOnce({
      ...idle(7),
      held_removals: [1],
    })
    await refuse(host, false)
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(2)
    expect(described(host)).toBe(
      "Waits for a scan of a directory around or inside one of Manga's directories.",
    )

    // A directory added under the open dialog asks its dry run again, which
    // is refused again; the read that refusal sends is answered late.
    const late = deferred<api.LibraryScanStatus>()
    vi.mocked(api.libraryScanStatus).mockReturnValueOnce(late.promise)
    await redraw(
      manga({ dirs: [...manga().dirs, dir(3, '/media/Manga/Extras')] }),
    )
    await land()
    expect(api.libraryRemovalCost).toHaveBeenCalledTimes(2)
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(3)

    // The held poll's read, sent after it, answers first: nothing holds.
    vi.mocked(api.libraryRemovalCost).mockResolvedValue(counted(12))
    await tick()
    await flush()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(4)
    expect(described(host)).toBe('Asks first, with what it would take.')

    await flush(() => late.resolve({ ...idle(7), held_removals: [1] }))
    await land()
    await cancelRemoval(host)
    expect(holdOf(button(host, 'remove-library'))).toBe('free')
    expect(described(host)).toBe('Asks first, with what it would take.')
    await tick()
    await flush()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(4)
  })

  it('draws the hold the page gave up when the read a refusal sends names it in other words and is overtaken by a newer one that fails', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValueOnce({
      ...idle(7),
      held_removals: [1],
    })
    vi.mocked(api.libraryRemovalCost).mockRejectedValue(
      refusal(false, 409, HELD),
    )
    const { host, redraw } = await shown()
    vi.mocked(api.libraryScanStatus).mockRejectedValue(
      new Error('/api/library/libraries/7/scan: 500 the store is locked'),
    )
    for (const _ of [1, 2, 3]) {
      await tick()
      await flush()
    }
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(4)
    expect(holdOf(button(host, 'remove-library'))).toBe('free')

    // The server now holds the removal in other words than the page last read.
    const heldWider = { ...idle(7), held_removals: [1, 3] }
    vi.mocked(api.libraryScanStatus).mockResolvedValue(heldWider)
    const first = deferred<api.LibraryScanStatus>()
    const second = deferred<api.LibraryScanStatus>()
    vi.mocked(api.libraryScanStatus)
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise)
    await refuse(host, false)
    // A directory added under the open dialog asks its dry run again, which
    // is refused again, and that refusal's read overtakes the first's.
    await redraw(
      manga({ dirs: [...manga().dirs, dir(3, '/media/Manga/Extras')] }),
    )
    await land()
    expect(api.libraryRemovalCost).toHaveBeenCalledTimes(2)
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(6)

    await flush(() => first.resolve(heldWider))
    await flush(() =>
      second.reject(
        new Error('/api/library/libraries/7/scan: 500 the store is locked'),
      ),
    )
    await land()
    await cancelRemoval(host)
    expect(holdOf(button(host, 'remove-library'))).toBe('held')
    expect(described(host)).toBe(
      "Waits for a scan of a directory around or inside one of Manga's directories.",
    )

    // Polled, as any hold the page draws is, until it ends.
    await tick()
    await flush()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(7)
    expect(holdOf(button(host, 'remove-library'))).toBe('held')
    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    await flush()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(8)
    expect(holdOf(button(host, 'remove-library'))).toBe('free')
    expect(described(host)).toBe('Asks first, with what it would take.')
  })
})

describe('LibraryPage, a derive holding the lease (O51)', () => {
  const JOB_WAITS = 'Waits for a scan or another library job to finish.'
  const AROUND =
    "Waits for a scan of a directory around or inside one of Manga's directories."
  const TODAY = 'Asks first, with what it would take.'
  const deriving = { ...idle(7), derive_running: true }
  const LOCKED = new Error(
    '/api/library/libraries/7/scan: 500 the store is locked',
  )

  /** What Remove…'s `aria-describedby` names, read through the id. */
  const described = (host: HTMLElement) => {
    const named = document.getElementById(
      button(host, 'remove-library').getAttribute('aria-describedby') ?? '',
    )
    expect(named).toBe(
      helpOf(row(section(host, 'Removal'), 'Remove this library')),
    )
    return named?.textContent
  }

  it('holds Remove… alone while its first read says a derive holds the lease, says why, and polls until a read finds it free', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue(deriving)
    const { host } = await shown()
    expect(holdOf(button(host, 'remove-library'))).toBe('held')
    expect(holdOf(button(host, 'scan-library'))).toBe('free')
    expect(described(host)).toBe(JOB_WAITS)

    await tick()
    await flush()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(2)
    expect(holdOf(button(host, 'remove-library'))).toBe('held')

    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    await flush()
    expect(holdOf(button(host, 'remove-library'))).toBe('free')
    expect(described(host)).toBe(TODAY)
  })

  it('says the scan around one of its directories, the more specific, where that and a derive both hold Remove…', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...deriving,
      held_removals: [1],
    })
    const { host } = await shown()
    expect(holdOf(button(host, 'remove-library'))).toBe('held')
    expect(described(host)).toBe(AROUND)
  })

  it('holds Remove… and says why when its own scan leaves the feed and the status read again names a derive holding the lease', async () => {
    feed.answer(
      runningFeed(
        feedJob({ id: 9, dirs: [{ id: 1, path: '/media/Manga/Volumes' }] }),
      ),
    )
    const { host } = await shown()
    await clock(0)
    expect(holdOf(button(host, 'remove-library'))).toBe('held')
    expect(described(host)).toBe(TODAY)

    vi.mocked(api.libraryScanStatus).mockResolvedValue(deriving)
    feed.answer(idleFeed(1))
    await clock(config.SCAN_POLL_MS)
    expect(holdOf(button(host, 'scan-library'))).toBe('free')
    expect(holdOf(button(host, 'remove-library'))).toBe('held')
    expect(described(host)).toBe(JOB_WAITS)

    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    await flush()
    expect(holdOf(button(host, 'remove-library'))).toBe('free')
    expect(described(host)).toBe(TODAY)
  })

  it('draws the derive’s hold again when its dry run is refused for it after the page gave it up, and the status read again says it in the words it last read', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValueOnce(deriving)
    vi.mocked(api.libraryRemovalCost).mockRejectedValue(
      new Error(
        '/api/library/libraries/7?dry_run=true: 409 {"error":"a scan or another library job is running; remove the library once it has finished"}',
      ),
    )
    const { host } = await shown()
    vi.mocked(api.libraryScanStatus).mockRejectedValue(LOCKED)
    for (const _ of [1, 2, 3]) {
      await tick()
      await flush()
    }
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(4)
    expect(holdOf(button(host, 'remove-library'))).toBe('free')

    vi.mocked(api.libraryScanStatus).mockResolvedValue(deriving)
    await press(button(host, 'remove-library'))
    await land()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(5)
    await press(
      [...host.querySelectorAll('.remove-library button')].find(
        (b) => b.textContent === 'Cancel',
      ) as Element,
    )
    expect(holdOf(button(host, 'remove-library'))).toBe('held')
    expect(described(host)).toBe(JOB_WAITS)
  })
})

describe('LibraryPage, a directory another library holds too', () => {
  const MANGA = { id: 7, name: 'Manga' }
  const KIDS = { id: 3, name: 'Kids' }
  /** Kids before the add: its own directory alone. */
  const kidsAlone = () =>
    manga({ id: 3, name: 'Kids', dirs: [dir(5, '/media/Kids', [KIDS])] })
  /**
   * The add's answer for `path`, held before by the libraries named, and the
   * shared schedule a link kept where the add asked for another.
   */
  const answer = (
    path: string,
    also_in: string[],
    kept?: number | null,
  ): config.Added => ({
    root: {
      ...dir(9, path, [...(also_in.length > 0 ? [MANGA] : []), KIDS]),
      ...(kept === undefined ? {} : { scan_interval_secs: kept }),
    },
    watching: also_in.length > 0 ? null : true,
    watch_reason: null,
    interval_secs: also_in.length > 0 ? null : 86400,
    also_in,
    schedule_kept: kept !== undefined,
  })
  /** Kids once the list is read again after the add of `/m`. */
  const kidsHolding = () =>
    manga({
      id: 3,
      name: 'Kids',
      dirs: [dir(9, '/m', [MANGA, KIDS]), dir(5, '/media/Kids', [KIDS])],
    })
  async function added(path: string, also_in: string[], kept?: number | null) {
    vi.mocked(api.addDir).mockResolvedValue(answer(path, also_in, kept))
    const shownPage = await shown(kidsAlone())
    const picker = await openPicker(shownPage.host)
    await typeInto(
      picker.querySelector('input[name="path"]') as HTMLInputElement,
      path,
    )
    await press(picker.querySelector('button[name="pick"]') as Element)
    await flush()
    return shownPage
  }
  const linkLine = (host: HTMLElement) =>
    section(host, 'Directories').querySelector(
      ':scope > .settings-box > .add-root > .linked',
    )

  it('says, where the picker was, that a directory another library held is also its directory and keeps its settings, naming that library', async () => {
    const { host, onAdded } = await added('/m', ['Manga'])
    expect(onAdded).toHaveBeenCalledWith([answer('/m', ['Manga'])])
    expect(host.querySelector('.add-root .dir-picker')).toBeNull()
    const line = linkLine(host)
    expect(line?.textContent).toBe(
      '/m is also a directory of Manga. It keeps the settings it has there.',
    )
    expect(line?.getAttribute('role')).toBe('status')
    expect(line?.previousElementSibling).toBe(button(host, 'add-dir'))
  })

  it('names every library that held it, as a list is said', async () => {
    const { host } = await added('/m', ['Manga', 'Comics'])
    expect(linkLine(host)?.textContent).toBe(
      '/m is also a directory of Manga and Comics. It keeps the settings it has there.',
    )
  })

  it('says the shared schedule a link kept where the add asked for another', async () => {
    const { host } = await added('/m', ['Manga'], null)
    expect(linkLine(host)?.textContent).toBe(
      '/m is also a directory of Manga. It keeps the settings it has there, and its scan schedule stays automatic.',
    )
  })

  it('says a link kept a shared schedule of Off, so that an add that asked for a scan is not left waiting for one (O275)', async () => {
    const { host } = await added('/m', ['Manga'], 0)
    expect(linkLine(host)?.textContent).toBe(
      '/m is also a directory of Manga. It keeps the settings it has there, and its scan schedule stays off.',
    )
  })

  it('says nothing of a link for a new directory, its live region drawn empty', async () => {
    const { host, onAdded } = await added('/media/New', [])
    expect(onAdded).toHaveBeenCalledWith([answer('/media/New', [])])
    expect(host.querySelector('.add-root .dir-picker')).toBeNull()
    expect(linkLine(host)?.textContent).toBe('')
    expect(host.querySelector('.add-root')?.textContent).toBe('Add directory…')
  })

  it('fills the live region it drew empty while the picker was open, so that a screen reader reads the link out', async () => {
    vi.mocked(api.addDir).mockResolvedValue(answer('/m', ['Manga']))
    const { host } = await shown(kidsAlone())
    const region = linkLine(host) as HTMLElement
    expect(region.getAttribute('role')).toBe('status')
    expect(region.textContent).toBe('')
    const picker = await openPicker(host)
    expect(linkLine(host)).toBe(region)
    const changes = changesIn(region)
    await typeInto(
      picker.querySelector('input[name="path"]') as HTMLInputElement,
      '/m',
    )
    await press(picker.querySelector('button[name="pick"]') as Element)
    await flush()
    expect(linkLine(host)).toBe(region)
    expect(region.textContent).toContain('is also a directory of Manga')
    expect(changes()).toBeGreaterThan(0)
  })

  it('takes the line away as Add directory… opens the picker again', async () => {
    const { host } = await added('/m', ['Manga'])
    expect(linkLine(host)?.textContent).not.toBe('')
    await openPicker(host)
    expect(linkLine(host)?.textContent).toBe('')
    await press(button(host, 'cancel-add'))
    expect(linkLine(host)?.textContent).toBe('')
  })

  it('takes the line away at a click on it, which lies over what is below it', async () => {
    const { host } = await added('/m', ['Manga'])
    const line = linkLine(host) as HTMLElement
    expect(line.textContent).not.toBe('')
    await press(line)
    expect(linkLine(host)).toBe(line)
    expect(line.textContent).toBe('')
  })

  it('takes the line away at a press of any other action on the page', async () => {
    const { host } = await added('/m', ['Manga'])
    expect(linkLine(host)?.textContent).not.toBe('')
    await press(button(host, 'rename-library'))
    expect(linkLine(host)?.textContent).toBe('')
  })

  it('keeps the line through a press on words of the page that are no action', async () => {
    const { host } = await added('/m', ['Manga'])
    await press(host.querySelector('.library-page .page-title') as Element)
    expect(linkLine(host)?.textContent).not.toBe('')
  })

  it('keeps the line while the list has yet to hold the directory, and takes it away once the list holds it no longer, as after Remove from this library or a move on the directory’s page beside it', async () => {
    const { host, redraw } = await added('/m', ['Manga'])
    expect(linkLine(host)?.textContent).not.toBe('')
    await redraw(kidsHolding())
    expect(linkLine(host)?.textContent).not.toBe('')
    await redraw(kidsAlone())
    expect(linkLine(host)?.textContent).toBe('')
  })

  describe('where its line is laid, read in jsdom, which lays nothing out', () => {
    /** A box's `[top, bottom, left, right]`, as a stub draws it. */
    type At = readonly [number, number, number, number]
    const px = (length: string) => Number.parseFloat(length) || 0
    /** What the stub draws each part of the page at; zero where unset. */
    let where = new Map<Element, At>()
    /** Where the flow lays the line, before its own `top` and padding. */
    let lineAt: At = [0, 0, 0, 0]
    let observers: Array<{ answer: () => void; observed: Element[] }> = []

    /**
     * Each part drawn where `where` says, and the line where `lineAt` says,
     * moved by its own `top` and grown by its padding, as a browser draws
     * a box `position: relative`.
     */
    function drawnAt(this: Element) {
      const line = this.matches('.add-root > .linked')
      let [top, bottom, left, right] = line
        ? lineAt
        : (where.get(this) ?? [0, 0, 0, 0])
      if (line) {
        const { style } = this as HTMLElement
        top += px(style.top)
        bottom += px(style.top) + px(style.paddingTop) + px(style.paddingBottom)
      }
      return DOMRect.fromRect({
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
      })
    }

    beforeEach(() => {
      where = new Map()
      lineAt = [0, 0, 0, 0]
      observers = []
      vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(
        drawnAt,
      )
      vi.stubGlobal(
        'ResizeObserver',
        class {
          observed: Element[] = []
          answer: () => void
          constructor(callback: () => void) {
            this.answer = callback
            observers.push(this)
          }
          observe(el: Element) {
            this.observed.push(el)
          }
          unobserve() {}
          disconnect() {
            observers = observers.filter((each) => each !== this)
          }
        },
      )
      // jsdom marks no focus as seen; a key press would, here.
      const matches = Element.prototype.matches
      vi.spyOn(Element.prototype, 'matches').mockImplementation(function (
        this: Element,
        selector: string,
      ) {
        if (selector === ':focus-visible')
          return this === document.activeElement
        return matches.call(this, selector)
      })
    })

    afterEach(() => {
      vi.restoreAllMocks()
      vi.unstubAllGlobals()
    })

    /**
     * The page once `/m` is said to be Manga's too, its Directories box
     * drawn 16–374 wide: the table's head 0–40 and `/media/Kids`'s row
     * 40–100, then the add form's room 100–140 with Add directory… at
     * 16–150 in it; and the line laid beside the button, inside the room,
     * as a wider window lays it.
     */
    async function laidOut() {
      const page = await added('/m', ['Manga'])
      const box = section(page.host, 'Directories').querySelector(
        ':scope > .settings-box',
      ) as HTMLElement
      const rows = [...box.querySelectorAll('tr')]
      expect(rows).toHaveLength(2)
      where.set(rows[0], [0, 40, 16, 374])
      where.set(rows[1], [40, 100, 16, 374])
      where.set(box.querySelector('.add-root') as Element, [100, 140, 16, 374])
      const add = button(page.host, 'add-dir')
      where.set(add, [100, 140, 16, 150])
      lineAt = [100, 140, 160, 374]
      const watching = observers.filter((each) => each.observed.includes(box))
      expect(watching, 'what watches the Directories box').toHaveLength(1)
      /** The box changing size, with no render of the page. */
      const resized = () =>
        act(() => {
          for (const each of watching) each.answer()
        })
      resized()
      const line = linkLine(page.host) as HTMLElement
      expect(line.textContent).not.toBe('')
      return { ...page, box, add, line, resized }
    }

    const placeOf = (line: HTMLElement) => [
      line.style.top,
      line.style.paddingTop,
      line.style.paddingBottom,
      line.style.marginBottom,
    ]

    it('lays the line again as its box changes size, with no render: too tall for the room under Add directory…, over the rows above the room, the button in view', async () => {
      const { line, resized } = await laidOut()
      expect(placeOf(line), 'beside the button, inside the room').toEqual([
        '',
        '',
        '',
        '',
      ])
      // The phone turned: the line under the button, 50 tall. The rows
      // above have room for it at 40–100: up 100, 10 more than its words,
      // which a margin gives back to the flow.
      lineAt = [140, 190, 16, 374]
      resized()
      expect(placeOf(line), 'over the row above the room').toEqual([
        '-100px',
        '5px',
        '5px',
        '-10px',
      ])
      // Turned back: read where the flow lays it, not where it was moved
      // to, it fits beside the button again.
      lineAt = [100, 140, 160, 374]
      resized()
      expect(placeOf(line), 'beside the button again').toEqual(['', '', '', ''])
    })

    it('rings the line while the control with focus lies under it, as the box changes size and as focus moves, with no render', async () => {
      const { add, line, resized } = await laidOut()
      const ringed = () => line.hasAttribute('data-over-focus')
      // Focus came back to Add directory… as the picker closed.
      expect(document.activeElement).toBe(add)
      expect(ringed(), 'beside the button').toBe(false)
      // 120 tall under the button: no room above, so it is laid up from
      // the room's foot over the button, 0–140.
      lineAt = [140, 260, 16, 374]
      resized()
      expect(line.style.top).toBe('-140px')
      expect(ringed(), 'over the focused button').toBe(true)
      act(() => add.blur())
      expect(ringed(), 'once focus leaves the button').toBe(false)
      act(() => add.focus())
      expect(ringed(), 'once focus comes back to it').toBe(true)
    })

    it('stops watching the box and focus once the page goes', async () => {
      const adds = vi.spyOn(document, 'addEventListener')
      const removes = vi.spyOn(document, 'removeEventListener')
      const { box } = await laidOut()
      act(() => rerender(<div />))
      expect(
        observers.filter((each) => each.observed.includes(box)),
        'what still watches the box',
      ).toEqual([])
      const focusOnes = (spy: typeof adds) =>
        spy.mock.calls.filter(
          ([type]) => type === 'focusin' || type === 'focusout',
        )
      expect(focusOnes(adds).length).toBeGreaterThan(0)
      expect(
        focusOnes(adds)
          .filter(
            ([type, listener]) =>
              !focusOnes(removes).some(
                ([gone, was]) => gone === type && was === listener,
              ),
          )
          .map(([type]) => type),
        'focus listeners left on the document',
      ).toEqual([])
    })
  })
})

describe('LibraryPage, a scan the feed shows (O273)', () => {
  it('shows a scan it did not start within one idle poll', async () => {
    const { host } = await shown()
    await clock(0)
    feed.answer(runningFeed(feedJob({ dir: { at: 2, of: 5 } })))
    // The pinned idle poll, not the constant, so that a slower poll fails.
    await clock(5000)
    expect(button(host, 'scan-library').textContent).toBe('Cancel')
    expect(scanLines(host)).toEqual([
      'Reading files · 120 of 2,097 · directory 2 of 5',
    ])
    expect(host.textContent).not.toMatch(/\d+s\b/)
  })

  it('Cancel cancels each job holding the library and draws Cancelling…', async () => {
    feed.answer(
      runningFeed(
        feedJob(),
        feedJob({
          id: 8,
          dirs: [{ id: 80, path: '/media/Comics' }],
          libraries: [{ id: 8, name: 'Comics' }],
        }),
        feedJob({ id: 9, dirs: [{ id: 2, path: '/media/Manga/Chapters' }] }),
      ),
    )
    const { host } = await shown()
    await clock(0)
    expect(holdOf(button(host, 'scan-library'))).toBe('free')
    await press(button(host, 'scan-library'))
    await clock(0)
    expect(feed.cancels).toEqual([7, 9])
    expect(button(host, 'scan-library').textContent).toBe('Cancel')
    expect(holdOf(button(host, 'scan-library'))).toBe('held')
    expect(scanLines(host)).toEqual(['Cancelling…', 'Cancelling…'])
  })

  it('a cancel made elsewhere ends in the paused line and Scan library', async () => {
    feed.answer(runningFeed(feedJob()))
    const { host, redraw } = await shown()
    await clock(0)
    feed.answer(runningFeed(feedJob({ cancelling: true })))
    await clock(config.SCAN_POLL_MS)
    expect(scanLines(host)).toEqual(['Cancelling…'])
    expect(holdOf(button(host, 'scan-library'))).toBe('held')

    feed.answer(idleFeed(1))
    await clock(config.SCAN_POLL_MS)
    // The directory the job had not reached yet: paused, its last scan kept.
    await redraw(
      manga({
        dirs: [
          {
            ...dir(1, '/media/Manga/Volumes'),
            last_scan_at: '2026-11-15 10:00:00',
            last_scan_status: 'ok',
            paused_at: '2026-11-16 03:33:10',
          },
          dir(2, '/media/Manga/Chapters'),
        ],
      }),
    )
    expect(scanLines(host)).toEqual([])
    expect(pausedLine(host)).toBe(
      `Paused until Scan is pressed, since ${inUtc('2026-11-16 03:33:10')}`,
    )
    expect(button(host, 'scan-library').textContent).toBe('Scan library')
    expect(holdOf(button(host, 'scan-library'))).toBe('free')
  })

  it('names the other libraries a job covers', async () => {
    feed.answer(
      runningFeed(
        feedJob({
          libraries: [
            { id: 7, name: 'Manga' },
            { id: 8, name: 'Comics' },
          ],
        }),
      ),
    )
    const { host } = await shown()
    await clock(0)
    expect(scanLines(host)).toEqual([
      'Reading files · 120 of 2,097 · directory 1 of 1',
      'This scan covers Comics too.',
    ])
  })

  it('sends no scan-status poll while the feed shows a job', async () => {
    // A removal held around one of its directories, which a job explains.
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      held_removals: [1],
    })
    feed.answer(runningFeed(feedJob()))
    await shown()
    await clock(0)
    const before = vi.mocked(api.libraryScanStatus).mock.calls.length
    for (let i = 0; i < 5; i++) await clock(1000)
    expect(vi.mocked(api.libraryScanStatus).mock.calls.length).toBe(before)

    // The job's end reads the hold again, through the feed.
    feed.answer(idleFeed(1))
    await clock(config.SCAN_POLL_MS)
    expect(vi.mocked(api.libraryScanStatus).mock.calls.length).toBeGreaterThan(
      before,
    )
  })

  it('offers Scan library, not Cancel, while only a fingerprint pass covers it, and reads a held removal’s status again each poll’s interval', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      held_removals: [1],
    })
    // A pass is never in the libraries the feed holds.
    feed.answer({ ended: 0, libraries: [], jobs: [fingerprintPass()] })
    const { host } = await shown()
    await clock(0)
    expect(button(host, 'scan-library').textContent).toBe('Scan library')
    expect(holdOf(button(host, 'remove-library'))).toBe('held')
    const reads = () => vi.mocked(api.libraryScanStatus).mock.calls.length
    const before = reads()
    const added: number[] = []
    for (let i = 0; i < 3; i++) {
      await tick()
      await flush()
      added.push(reads() - before)
    }
    expect(added).toEqual([1, 2, 3])
  })

  it('draws no paused line while a scan of its paused directory runs, though the list still says it is paused', async () => {
    vi.mocked(api.startLibraryScan).mockResolvedValue(started)
    const paused = manga({
      dirs: [
        {
          ...dir(1, '/media/Manga/Volumes'),
          paused_at: '2026-11-16 03:33:10',
        },
        dir(2, '/media/Manga/Chapters'),
      ],
    })
    const { host } = await shown(paused)
    await clock(0)
    expect(pausedLine(host)).toBe(
      `Paused until Scan is pressed, since ${inUtc('2026-11-16 03:33:10')}`,
    )
    // The press resumes it; the list is read again only as the job ends.
    feed.answer(runningFeed(whole()))
    await press(button(host, 'scan-library'))
    await clock(0)
    expect(button(host, 'scan-library').textContent).toBe('Cancel')
    expect(pausedLine(host)).toBeUndefined()
  })

  it('draws no refusal for a cancel of a job that had ended, and reads Scan library once the feed says so', async () => {
    feed.answer(runningFeed(feedJob()))
    const { host } = await shown()
    await clock(0)
    // The job ends before the next poll, so the page still offers Cancel.
    feed.answer(idleFeed(1))
    await press(button(host, 'scan-library'))
    await clock(0)
    expect(feed.cancels).toEqual([7])
    expect(host.querySelector('.library-page > [role="alert"]')).toBeNull()
    expect(button(host, 'scan-library').textContent).toBe('Scan library')
  })

  it('reports only the directories the server says a pressed scan walks, not one the list still shows inside them', async () => {
    // The answer names its own two directories, not Kids (5), which the
    // list still shows inside Volumes.
    vi.mocked(api.startLibraryScan).mockResolvedValue(started)
    reporting()
    const { host } = await shown(nesting())
    await clock(0)
    feed.answer(idleFeed(1))
    await press(button(host, 'scan-library'))
    await clock(0)
    expect(reportedPaths(host)).toEqual([
      '/media/Manga/Volumes',
      '/media/Manga/Chapters',
    ])
    expect(
      vi.mocked(config.scanStatus).mock.calls.map(([id]) => id),
    ).not.toContain(5)
  })

  it('draws the report of a scan it started that ended before the feed saw it', async () => {
    vi.mocked(api.startLibraryScan).mockResolvedValue(started)
    reporting()
    const { host } = await shown()
    await clock(0)
    // The scan is over by the feed's next answer, which moved its marker.
    feed.answer(idleFeed(1))
    await press(button(host, 'scan-library'))
    await clock(0)
    expect(reportedPaths(host)).toEqual([
      '/media/Manga/Volumes',
      '/media/Manga/Chapters',
    ])
  })

  it('draws the report of a scan it started that ended before the feed saw it, after an earlier scan the feed saw end', async () => {
    vi.mocked(api.startLibraryScan).mockResolvedValue(started)
    reporting()
    feed.answer(runningFeed(feedJob()))
    const { host } = await shown()
    await clock(0)
    feed.answer(idleFeed(1))
    await clock(config.SCAN_POLL_MS)
    expect(reportedPaths(host)).toEqual(['/media/Manga/Volumes'])

    // The next scan is over by the feed's next answer.
    feed.answer(idleFeed(2))
    await press(button(host, 'scan-library'))
    await clock(0)
    expect(reportedPaths(host)).toEqual([
      '/media/Manga/Volumes',
      '/media/Manga/Chapters',
    ])
  })

  it('holds Scan library from the press until the feed answers, then reads Cancel', async () => {
    vi.mocked(api.startLibraryScan).mockResolvedValue(started)
    const { host } = await shown()
    await clock(0)
    const release = feed.hold()
    feed.answer(runningFeed(feedJob()))
    await press(button(host, 'scan-library'))
    await clock(0)
    expect(api.startLibraryScan).toHaveBeenCalledTimes(1)
    expect([
      button(host, 'scan-library').textContent,
      holdOf(button(host, 'scan-library')),
    ]).toEqual(['Scan library', 'held'])

    release()
    await clock(0)
    expect([
      button(host, 'scan-library').textContent,
      holdOf(button(host, 'scan-library')),
    ]).toEqual(['Cancel', 'free'])
  })
})

/** The fingerprint pass's line under the header, if it is drawn. */
const passLine = (host: HTMLElement) =>
  host.querySelector('.library-page > .pass-line > .scanning')?.textContent

/** The box that switches matching by content, in its row. */
const matchBox = (host: HTMLElement) =>
  row(
    section(host, 'Matching'),
    'Match editions by page content',
  ).querySelector('input[type="checkbox"]') as HTMLInputElement

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  })

/**
 * Stubs `PATCH /api/library/libraries/7` over the feed's stub, each answered
 * by `answer`, and hands back each one's method and body as sent.
 */
function servePatch(answer: () => Promise<Response>) {
  const before = globalThis.fetch
  const sent: { method: string | undefined; body: unknown }[] = []
  vi.stubGlobal(
    'fetch',
    vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
      if (String(input) !== '/api/library/libraries/7')
        return before(input, init)
      sent.push({
        method: init?.method,
        body: JSON.parse(String(init?.body)) as unknown,
      })
      return answer()
    }),
  )
  return sent
}

describe('LibraryPage, the fingerprint pass', () => {
  it('draws the pass’s line and Cancel on the page of a library it covers, and neither on another’s', async () => {
    // Covers Manga, 7, and not Comics, 8.
    feed.answer({ ended: 0, libraries: [], jobs: [fingerprintPass({ id: 9 })] })
    const manga = await shown()
    await clock(0)
    expect(passLine(manga.host)).toBe(
      'Fingerprinting pages · 120 of 2,097 files',
    )
    expect(button(manga.host, 'cancel-pass')?.textContent).toBe('Cancel')

    act(() => rerender(<div />))
    const other = await shown(comics())
    await clock(0)
    expect(passLine(other.host)).toBeUndefined()
    expect(button(other.host, 'cancel-pass')).toBeNull()
  })

  it('draws the pass’s line after a scan’s, and its Cancel cancels the pass alone', async () => {
    // A pass is never in the libraries the feed holds.
    feed.answer({
      ended: 0,
      libraries: [{ id: 7, step: 'resolving' }],
      jobs: [feedJob(), fingerprintPass({ id: 9 })],
    })
    const { host } = await shown()
    await clock(0)
    expect(
      [
        ...host.querySelectorAll(
          '.library-page > .scanning, .library-page > .pass-line',
        ),
      ].map((line) => line.className),
    ).toEqual(['scanning', 'pass-line'])
    expect(holdOf(button(host, 'cancel-pass'))).toBe('free')

    await press(button(host, 'cancel-pass'))
    await clock(0)
    expect(feed.cancels).toEqual([9])
    expect(passLine(host)).toBe('Cancelling…')
    expect(holdOf(button(host, 'cancel-pass'))).toBe('held')
    expect(scanLines(host)).toEqual([
      'Reading files · 120 of 2,097 · directory 1 of 1',
    ])
    expect([
      button(host, 'scan-library').textContent,
      holdOf(button(host, 'scan-library')),
    ]).toEqual(['Cancel', 'free'])
  })

  it('draws a refused Cancel as the server’s sentence under the pass’s line, and frees the Cancel again', async () => {
    feed.answer({ ended: 0, libraries: [], jobs: [fingerprintPass({ id: 9 })] })
    const { host } = await shown()
    await clock(0)
    const answer = deferred<Response>()
    const before = globalThis.fetch
    vi.stubGlobal(
      'fetch',
      vi.fn((input: RequestInfo | URL, init?: RequestInit) =>
        String(input) === '/api/library/scans/9/cancel'
          ? answer.promise
          : before(input, init),
      ),
    )

    await press(button(host, 'cancel-pass'))
    expect(holdOf(button(host, 'cancel-pass'))).toBe('held')
    answer.resolve(json({ error: 'the store refused the cancel' }, 500))
    await clock(0)
    expect(
      host.querySelector('.library-page > .pass-line + [role="alert"]')
        ?.textContent,
    ).toBe('the store refused the cancel')
    expect(holdOf(button(host, 'cancel-pass'))).toBe('free')
    expect(passLine(host)).toBe('Fingerprinting pages · 120 of 2,097 files')
  })
})

describe('LibraryPage, Match editions by page content', () => {
  it('ticks the box as the list says, under its own heading, with its line under it', async () => {
    const on = await shown(manga({ match_by_content: true }))
    expect(matchBox(on.host).checked).toBe(true)
    expect(
      helpOf(
        row(section(on.host, 'Matching'), 'Match editions by page content'),
      )?.textContent,
    ).toBe(
      "Fingerprints this library's pages in the background, so later versions can match editions by what their pages show.",
    )

    act(() => rerender(<div />))
    const off = await shown(manga({ match_by_content: false }))
    expect(matchBox(off.host).checked).toBe(false)
  })

  it('unticking sends the switch alone, holds the box as the server holds it until the answer, and then draws the answer', async () => {
    const answer = deferred<Response>()
    const sent = servePatch(() => answer.promise)
    const { host, onChanged } = await shown()
    await press(matchBox(host))
    expect(sent).toEqual([
      { method: 'PATCH', body: { match_by_content: false } },
    ])
    expect([matchBox(host).checked, holdOf(matchBox(host))]).toEqual([
      true,
      'held',
    ])
    // A second press while it is out sends nothing.
    await press(matchBox(host))
    expect(sent).toHaveLength(1)

    await flush(() =>
      answer.resolve(json({ id: 7, name: 'Manga', match_by_content: false })),
    )
    await land()
    expect([matchBox(host).checked, holdOf(matchBox(host))]).toEqual([
      false,
      'free',
    ])
    expect(onChanged).toHaveBeenCalledTimes(1)
  })

  it('draws what the server answers it holds, not what was pressed', async () => {
    // An answer that is not the press, which the server never sends: it
    // tells a box drawn from the answer from one drawn from the press.
    servePatch(() =>
      Promise.resolve(json({ id: 7, name: 'Manga', match_by_content: true })),
    )
    const { host } = await shown()
    await press(matchBox(host))
    await land()
    expect([matchBox(host).checked, holdOf(matchBox(host))]).toEqual([
      true,
      'free',
    ])
  })

  it('holds the box by disabled while the library’s removal is open, and frees it once that is cancelled', async () => {
    vi.mocked(api.libraryRemovalCost).mockResolvedValue(counted(12))
    const sent = servePatch(() =>
      Promise.resolve(json({ id: 7, name: 'Manga', match_by_content: false })),
    )
    const { host } = await shown()
    await press(button(host, 'remove-library'))
    expect(holdOf(matchBox(host))).toBe('disabled')
    await press(matchBox(host))
    expect(sent).toEqual([])

    await press(
      [...host.querySelectorAll('.remove-library button')].find(
        (b) => b.textContent === 'Cancel',
      ) as Element,
    )
    expect(holdOf(matchBox(host))).toBe('free')
  })

  it('a refusal leaves the box ticked and says why', async () => {
    servePatch(() =>
      Promise.resolve(json({ error: 'the store is locked' }, 500)),
    )
    const { host, onChanged } = await shown()
    await press(matchBox(host))
    await land()
    expect([matchBox(host).checked, holdOf(matchBox(host))]).toEqual([
      true,
      'free',
    ])
    expect(
      section(host, 'Matching').querySelector('[role="alert"]')?.textContent,
    ).toBe('the store is locked')
    expect(onChanged).not.toHaveBeenCalled()
  })
})
