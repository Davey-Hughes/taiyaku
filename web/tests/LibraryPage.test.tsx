import { act, useState } from 'react'
import { MemoryRouter } from 'react-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from '../src/config'
import { NO_ADDRESS } from '../src/configAddress'
import { WAIT_DRAWN_AFTER_MS } from '../src/hooks'
import * as api from '../src/libraries'
import { LibraryPage } from '../src/LibraryPage'
import { deferred, flush, land } from './support/async'
import { emptyDerive } from './support/derive'
import { press, submit, typeInto } from './support/input'
import { holdOf } from './support/held'
import { changesIn } from './support/liveRegion'
import {
  counted,
  dir,
  idle,
  manga,
  scanningIn,
  tick,
} from './support/libraries'
import { mount, rerender } from './support/mount'
import { keptGrid, keptSplitEditions } from './support/reads'
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

  it('heads itself with ‹ Libraries, its name, what it holds, and Rename… and Scan library as its actions, over Directories and the removal box', async () => {
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
    expect(sectionTitles(host)).toEqual(['Directories', 'Removal'])
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
    ).toEqual(['Remove this library'])
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
    const sent = deferred<{ id: number; name: string }>()
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
    const sent = deferred<{ id: number; name: string }>()
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
    const sent = deferred<{ id: number; name: string }>()
    vi.mocked(api.renameLibrary).mockReturnValue(sent.promise)
    const { host } = await shown()
    await press(button(host, 'rename-library'))
    await typeInto(nameBox(host), 'Comics')
    const save = button(host, 'save-rename')
    act(() => save.focus())
    await press(save)
    await waitDrawn()
    expect(document.activeElement).toBe(nameBox(host))

    await flush(() => sent.resolve({ id: 7, name: 'Comics' }))
    expect(host.querySelector('form.rename-library')).toBeNull()
    expect(title(host)?.textContent).toBe('Comics')
    expect(document.activeElement).toBe(title(host))
  })

  it('takes the library’s own name, in any case, as no clash', async () => {
    vi.mocked(api.renameLibrary).mockResolvedValue({ id: 7, name: 'MANGA' })
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
    vi.mocked(api.startLibraryScan).mockResolvedValue(started)
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
    expect(holdOf(button(host, 'scan-library'))).toBe('held')
    expect(holdOf(button(host, 'remove-library'))).toBe('held')
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
    expect(holdOf(button(host, 'scan-library'))).toBe('free')
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
    expect(holdOf(button(host, 'scan-library'))).toBe('held')
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
    vi.mocked(api.renameLibrary).mockResolvedValue({ id: 7, name: 'Comics' })
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
      also_in: [],
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
  it('holds the library’s Scan library and Remove while a directory is scanning from its own page, and sends nothing from them', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 2))
    const { host } = await shown()

    expect(holdOf(button(host, 'scan-library'))).toBe('held')
    expect(holdOf(button(host, 'remove-library'))).toBe('held')
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

  it('marks the Library views and the split editions stale when the library’s own scan ends', async () => {
    const grid = await keptGrid()
    const split = await keptSplitEditions()
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      running: true,
      elapsed_secs: 3,
    })
    const { host } = await shown()
    expect(grid.stale()).toBe(false)
    expect(split.stale()).toBe(false)
    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    expect(host.querySelector('.library-page > .scanning')).toBeNull()
    expect(grid.stale()).toBe(true)
    expect(grid.sent()).toBe(1)
    expect(split.stale()).toBe(true)
    expect(split.sent()).toBe(1)
  })

  it('frees the library once the directory’s scan has ended', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 2))
    const { host } = await shown()
    expect(holdOf(button(host, 'scan-library'))).toBe('held')
    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    expect(holdOf(button(host, 'scan-library'))).toBe('free')
    expect(holdOf(button(host, 'remove-library'))).toBe('free')
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
          'not set',
          '2026-10-01 10:00:00 UTC\u00a0·\u00a0failed',
          '',
        ],
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
    expect(lastScans(host)).toEqual(['Never', '2026-10-01 10:00:00 UTC'])

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
    expect(api.addDir).toHaveBeenCalledWith(7, '/media/x')
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
    await flush()
    expect(sectionTitles(host)).toEqual(['Directories', 'Removal'])
    vi.mocked(api.libraryScanStatus).mockResolvedValue(
      scanningIn(manga(), 1, 2),
    )
    await press(button(host, 'scan-library'))
    await tick()
    await flush()
    expect(sectionTitles(host)).toEqual(['Directories', 'Removal'])

    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    await flush()

    // Between the directories and the removal box, its heading an h3 and
    // each directory's path an h4 beneath it.
    expect(sectionTitles(host)).toEqual([
      'Directories',
      'What the scan found',
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

  it('takes the directories its scan walks from the server’s answer, not from a list that still shows one it no longer reaches', async () => {
    // The list predates a pattern edit at Volumes that keeps Kids out: it
    // still says Manga shows Kids, and the server reserved Manga's own two.
    vi.mocked(api.startLibraryScan).mockResolvedValue(started)
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
    ])
    expect(config.scanStatus).not.toHaveBeenCalledWith(5)
  })

  it('says which library scans the folder inside its own whose scan alone is running, holds Scan library, and draws its report once that ends', async () => {
    reporting()
    vi.mocked(api.libraryScanStatus).mockResolvedValue(walking(5))
    const { host } = await shown(nesting())
    const lines = [...host.querySelectorAll('.library-page > .scanning')]
    expect(lines.map((line) => line.textContent)).toEqual([
      'Kids is scanning Kids/, which Manga shows.',
    ])
    expect(holdOf(button(host, 'scan-library'))).toBe('held')
    // The table lists its own directories, and neither is scanning.
    expect(lastScans(host)).toEqual(['Never', 'Never'])

    await tick()
    vi.mocked(api.libraryScanStatus).mockResolvedValue(walking())
    await tick()
    await flush()
    expect(reportedPaths(host)).toEqual(['/media/Manga/Volumes/Kids'])
  })

  it('says a line for each folder inside its own that is scanning', async () => {
    const library = nesting()
    library.dirs[1].nested = [
      {
        id: 6,
        path: '/media/Manga/Chapters/Tiny/weekly',
        libraries: ['Tiny'],
        shown: true,
        kept_out_by: null,
      },
    ]
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...walking(5),
      dirs: [1, 2, 5, 6].map((id) => ({
        root_id: id,
        running: id === 5 || id === 6,
      })),
      scans: [
        { library_id: 3, library: 'Kids', root_ids: [5] },
        { library_id: 9, library: 'Tiny', root_ids: [6] },
      ],
    })
    const { host } = await shown(library)
    const lines = [...host.querySelectorAll('.library-page > .scanning')]
    expect(lines.map((line) => line.textContent)).toEqual([
      'Kids is scanning Kids/, which Manga shows.',
      'Tiny is scanning Tiny/weekly/, which Manga shows.',
    ])
    // The folder breaks after its slashes, never inside a name.
    expect(lines[1].querySelectorAll('code .path-break')).toHaveLength(1)
  })

  it('says one line for a scan that walks a folder inside its own and another inside that, naming the topmost folder and the library that asked for the scan', async () => {
    // Kids' scan walks its own Kids and Tiny's Kids/tiny inside it, both of
    // which Manga shows: Tiny is not scanning anything.
    const library = nesting()
    library.dirs[0].nested.push({
      id: 7,
      path: '/media/Manga/Volumes/Kids/tiny',
      libraries: ['Tiny'],
      shown: true,
      kept_out_by: null,
    })
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...walking(5),
      dirs: [1, 2, 5, 7].map((id) => ({
        root_id: id,
        running: id === 5 || id === 7,
      })),
      scans: [{ library_id: 3, library: 'Kids', root_ids: [5, 7] }],
    })
    const { host } = await shown(library)
    expect(
      [...host.querySelectorAll('.library-page > .scanning')].map(
        (line) => line.textContent,
      ),
    ).toEqual(['Kids is scanning Kids/, which Manga shows.'])
  })

  it('says a folder of one name once, where the scan walks one inside each of two of its directories', async () => {
    const library = nesting()
    library.dirs[1].nested = [
      {
        id: 8,
        path: '/media/Manga/Chapters/Kids',
        libraries: ['Kids'],
        shown: true,
        kept_out_by: null,
      },
    ]
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...walking(5),
      dirs: [1, 2, 5, 8].map((id) => ({
        root_id: id,
        running: id === 5 || id === 8,
      })),
      scans: [{ library_id: 3, library: 'Kids', root_ids: [5, 8] }],
    })
    const { host } = await shown(library)
    expect(
      [...host.querySelectorAll('.library-page > .scanning')].map(
        (line) => line.textContent,
      ),
    ).toEqual(['Kids is scanning Kids/, which Manga shows.'])
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
      ...scanningIn(manga(), 1, 2),
      held_removals: [1],
    })
    const { host } = await shown()
    expect(holdOf(button(host, 'remove-library'))).toBe('held')
    expect(removalHelp(host)).toBe('Asks first, with what it would take.')

    // The library's scan ends with the removal still held: the words come.
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      held_removals: [1],
    })
    await tick()
    await flush()
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

  it('says two folders a scan walks inside two of its directories in one line', async () => {
    const library = nesting()
    library.dirs[0].nested = [
      {
        id: 5,
        path: '/media/Manga/Volumes/a',
        libraries: ['Kids'],
        shown: true,
        kept_out_by: null,
      },
    ]
    library.dirs[1].nested = [
      {
        id: 8,
        path: '/media/Manga/Chapters/b',
        libraries: ['Kids'],
        shown: true,
        kept_out_by: null,
      },
    ]
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...walking(5),
      dirs: [1, 2, 5, 8].map((id) => ({
        root_id: id,
        running: id === 5 || id === 8,
      })),
      scans: [{ library_id: 3, library: 'Kids', root_ids: [5, 8] }],
    })
    const { host } = await shown(library)
    expect(
      [...host.querySelectorAll('.library-page > .scanning')].map(
        (line) => line.textContent,
      ),
    ).toEqual(['Kids is scanning a/ and b/, which Manga shows.'])
  })

  it('names no library for a scan none asked for, of a folder two libraries hold, in one folder or two', async () => {
    const library = nesting()
    library.dirs[0].nested = [
      {
        id: 5,
        path: '/media/Manga/Volumes/a',
        libraries: ['Kids', 'Teens'],
        shown: true,
        kept_out_by: null,
      },
    ]
    library.dirs[1].nested = [
      {
        id: 8,
        path: '/media/Manga/Chapters/b',
        libraries: ['Kids', 'Teens'],
        shown: true,
        kept_out_by: null,
      },
    ]
    const status = (...ids: number[]): api.LibraryScanStatus => ({
      ...walking(5),
      dirs: [1, 2, 5, 8].map((id) => ({
        root_id: id,
        running: ids.includes(id),
      })),
      scans: [{ library_id: null, library: null, root_ids: ids }],
    })
    vi.mocked(api.libraryScanStatus).mockResolvedValue(status(5))
    const { host } = await shown(library)
    const lines = () =>
      [...host.querySelectorAll('.library-page > .scanning')].map(
        (line) => line.textContent,
      )
    expect(lines()).toEqual(['a/ is being scanned, which Manga shows.'])

    vi.mocked(api.libraryScanStatus).mockResolvedValue(status(5, 8))
    await tick()
    await flush()
    expect(lines()).toEqual(['a/ and b/ are being scanned, which Manga shows.'])
  })

  it('says a scan is running in this library while one of its own directories is scanning, though one inside it is too', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue(walking(1, 5))
    const { host } = await shown(nesting())
    expect(host.querySelector('.library-page > .scanning')?.textContent).toBe(
      'A scan is running in this library… 3s',
    )
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
    expect(holdOf(button(host, 'scan-library'))).toBe('free')
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
  it('holds the library’s removal when a directory is found scanning under its open dialog, and asks its cost again when that ends', async () => {
    vi.mocked(api.libraryRemovalCost).mockResolvedValue(counted(12))
    // Found by the read a return sends, the first that can land under a
    // drawn body.
    const { host, again: late } = await returned()
    await press(button(host, 'remove-library'))
    const confirm = () =>
      host.querySelector('.remove-library .confirm') as HTMLButtonElement
    expect(holdOf(confirm())).toBe('free')

    await flush(() => late.resolve(scanningIn(manga(), 2)))
    expect(holdOf(confirm())).toBe('held')
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
    expect(holdOf(confirm())).toBe('free')
  })

  it('holds the library’s removal when the read a return sends finds a scan under its open dialog', async () => {
    vi.mocked(api.libraryRemovalCost).mockResolvedValue(counted(12))
    const { host, again: late } = await returned()
    await press(button(host, 'remove-library'))
    const confirm = () =>
      host.querySelector('.remove-library .confirm') as HTMLButtonElement
    expect(holdOf(confirm())).toBe('free')

    await flush(() =>
      late.resolve({ ...idle(7), running: true, elapsed_secs: 5 }),
    )
    expect(holdOf(confirm())).toBe('held')
    confirm().click()
    await flush()
    expect(api.removeLibrary).not.toHaveBeenCalled()
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
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 1))
    vi.mocked(api.renameLibrary).mockResolvedValue({ id: 7, name: 'Comics' })
    vi.mocked(api.addDir).mockResolvedValue({
      root: dir(3, '/media/Manga/Extras'),
      watching: true,
      watch_reason: null,
      interval_secs: 86400,
      also_in: [],
    })
    const { host } = await shown()
    expect(host.querySelector('.library-page > .scanning')).not.toBeNull()

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

  // Scan library is not drawn while the name is edited, so the scan is one
  // the page took up on being drawn, and it ends under the refusal.
  it('keeps a rename’s refusal through the end of a scan of the library', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      running: true,
      elapsed_secs: 3,
    })
    vi.mocked(api.renameLibrary).mockRejectedValue(
      new Error('/api/library/libraries/7: 400 {"error":"taken"}'),
    )
    const { host, onChanged } = await shown()
    await press(button(host, 'rename-library'))
    await typeInto(nameBox(host), 'Comics')
    await submit(host.querySelector('form.rename-library') as Element)
    expect(renameRefusal(host)?.textContent).toBe('taken')

    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    expect(host.querySelector('.library-page > .scanning')).toBeNull()
    expect(onChanged).toHaveBeenCalledWith(true)
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
    holdOf(button(page, 'scan-library')),
    holdOf(button(page, 'remove-library')),
  ])
}

describe('LibraryPage, its scan status as it is drawn', () => {
  it('draws a running scan’s line, Scanning… and its held buttons in the same render as the rest of its body', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 2))
    const drawn = watchDrawn(scanDrawn)
    await shown()
    drawn.stop()
    expect([...drawn.seen].filter((one) => one !== 'no body')).toEqual([
      JSON.stringify([
        'A scan is running in this library… 3s',
        ['Never', 'Scanning…'],
        'held',
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
      JSON.stringify([null, ['Never', 'Never'], 'free', 'free']),
    )
    expect(host.querySelector('.library-page [role="alert"]')).toBeNull()
    await tick()
    await tick()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(1)
  })

  it('draws its body at once on a return, with the running scan the status kept says, and reads that status again', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      running: true,
      elapsed_secs: 9,
    })
    await shown()
    act(() => rerender(<div />))
    vi.mocked(api.libraryScanStatus).mockClear()
    vi.mocked(api.libraryScanStatus).mockReturnValueOnce(new Promise(() => {}))

    const drawn = watchDrawn(scanDrawn)
    await shown()
    drawn.stop()
    expect([...drawn.seen]).toEqual([
      'no body',
      JSON.stringify([
        'A scan is running in this library… 9s',
        ['Never', 'Never'],
        'held',
        'held',
      ]),
    ])
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(1)
  })

  it('takes up a scan its read on a return finds, though the status it was drawn from said none was running', async () => {
    const { host, again } = await returned()
    expect(scanDrawn()).toBe(
      JSON.stringify([null, ['Never', 'Never'], 'free', 'free']),
    )

    await flush(() => again.resolve(scanningIn(manga(), 1)))
    expect(scanDrawn()).toBe(
      JSON.stringify([
        'A scan is running in this library… 3s',
        ['Scanning…', 'Never'],
        'held',
        'held',
      ]),
    )
    // Watched from here as any scan is: the poll finds it ended.
    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    expect(scanDrawn()).toBe(
      JSON.stringify([null, ['Never', 'Never'], 'free', 'free']),
    )
    expect(host.querySelector('.library-page > .scanning')).toBeNull()
  })

  it('sends the next poll only once the last has answered', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 1))
    const { host } = await shown()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(1)
    const out = deferred<api.LibraryScanStatus>()
    vi.mocked(api.libraryScanStatus).mockReturnValueOnce(out.promise)
    await tick()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(2)
    await tick()
    await tick()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(2)

    await flush(() => out.resolve(scanningIn(manga(), 1, 2)))
    expect(lastScans(host)).toEqual(['Scanning…', 'Scanning…'])
    await tick()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(3)
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

  it('takes up a scan of its own that the status read after a refusal finds running, as one found on arrival is', async () => {
    vi.mocked(api.libraryRemovalCost).mockResolvedValue(counted(12))
    vi.mocked(api.removeLibrary).mockRejectedValue(refusal(true, 409, HELD))
    const { host } = await shown()
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 1))

    await refuse(host, true)
    await cancelRemoval(host)
    expect(scanDrawn()).toBe(
      JSON.stringify([
        'A scan is running in this library… 3s',
        ['Scanning…', 'Never'],
        'held',
        'held',
      ]),
    )
    // Watched from here as any scan is: the poll finds it ended.
    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    expect(scanDrawn()).toBe(
      JSON.stringify([null, ['Never', 'Never'], 'free', 'free']),
    )
  })

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

describe('LibraryPage, the status read on a return', () => {
  /** The kept status: a scan running 9 s in, no directory named. */
  const kept = () => ({ ...idle(7), running: true, elapsed_secs: 9 })

  it('draws a scan the kept status said was running as ended in the render the read on a return lands, and polls it no more', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue(kept())
    const { again, onChanged } = await returned()
    const running = JSON.stringify([
      'A scan is running in this library… 9s',
      ['Never', 'Never'],
      'held',
      'held',
    ])
    expect(scanDrawn()).toBe(running)

    const drawn = watchDrawn(scanDrawn)
    await flush(() => again.resolve(idle(7)))
    drawn.stop()
    expect([...drawn.seen]).toEqual([
      running,
      JSON.stringify([null, ['Never', 'Never'], 'free', 'free']),
    ])

    // Ended: nothing polls it, and it ended before this visit, so the list
    // is not read again for it.
    vi.mocked(api.libraryScanStatus).mockClear()
    await tick()
    await tick()
    expect(api.libraryScanStatus).not.toHaveBeenCalled()
    expect(onChanged).not.toHaveBeenCalled()
  })

  it('draws no report and reads the list no more for a scan the kept status said was running and the read on a return finds over', async () => {
    reporting()
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 1))
    const { host, again, onChanged } = await returned()
    expect(lastScans(host)).toEqual(['Scanning…', 'Never'])

    await flush(() => again.resolve(idle(7)))
    await flush()
    expect(scanDrawn()).toBe(
      JSON.stringify([null, ['Never', 'Never'], 'free', 'free']),
    )
    expect(reportedPaths(host)).toEqual([])
    expect(config.scanStatus).not.toHaveBeenCalled()
    expect(onChanged).not.toHaveBeenCalled()
  })

  it('marks no Library view stale for a scan the kept status said was running and the read on a return finds over', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue(kept())
    const { again } = await returned()
    const grid = await keptGrid()
    expect(scanDrawn()).toBe(
      JSON.stringify([
        'A scan is running in this library… 9s',
        ['Never', 'Never'],
        'held',
        'held',
      ]),
    )
    await flush(() => again.resolve(idle(7)))
    await tick()
    expect(scanDrawn()).toBe(
      JSON.stringify([null, ['Never', 'Never'], 'free', 'free']),
    )
    expect(grid.stale()).toBe(false)
  })

  it('draws no report for a scan the kept status said was running when the poll’s read is the first of this visit to find it over', async () => {
    reporting()
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 1))
    const { host, onChanged } = await returned()
    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    await flush()
    await flush()
    await flush()
    expect(scanDrawn()).toBe(
      JSON.stringify([null, ['Never', 'Never'], 'free', 'free']),
    )
    expect(reportedPaths(host)).toEqual([])
    expect(config.scanStatus).not.toHaveBeenCalled()
    expect(onChanged).not.toHaveBeenCalled()
  })

  it('draws the reports of a scan the kept status said was running once a poll of this visit has found it running in the same words, then over', async () => {
    reporting()
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 1))
    // The read the return sends is never answered; the poll's first read
    // says what the kept status says, which leaves the store's entry as it
    // was.
    const { host, onChanged } = await returned()
    await tick()
    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    await flush()
    expect(reportedPaths(host)).toEqual(['/media/Manga/Volumes'])
    expect(onChanged).toHaveBeenCalledTimes(1)
    expect(onChanged).toHaveBeenCalledWith(true)
  })

  it('draws the reports of a scan the read on a return finds still running, once its poll finds it over', async () => {
    reporting()
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 1))
    const { host, again, onChanged } = await returned()
    await flush(() =>
      again.resolve({ ...scanningIn(manga(), 1), elapsed_secs: 4 }),
    )
    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    await flush()
    expect(scanDrawn()).toBe(
      JSON.stringify([null, ['Never', 'Never'], 'free', 'free']),
    )
    expect(reportedPaths(host)).toEqual(['/media/Manga/Volumes'])
    expect(onChanged).toHaveBeenCalledTimes(1)
    expect(onChanged).toHaveBeenCalledWith(true)
  })

  it('draws the reports of a scan the read on a return finds still running in the kept status’s own words, once its poll finds it over', async () => {
    reporting()
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 1))
    const { host, again, onChanged } = await returned()
    // The same words as the kept status, which leave the store's entry as
    // it was.
    await flush(() => again.resolve(scanningIn(manga(), 1)))
    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    await flush()
    expect(scanDrawn()).toBe(
      JSON.stringify([null, ['Never', 'Never'], 'free', 'free']),
    )
    expect(reportedPaths(host)).toEqual(['/media/Manga/Volumes'])
    expect(onChanged).toHaveBeenCalledTimes(1)
    expect(onChanged).toHaveBeenCalledWith(true)
  })

  it('draws no report for a scan the kept status said was running when the read on a return fails and its poll finds it over', async () => {
    reporting()
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 1))
    const { host, again, onChanged } = await returned()
    await flush(() =>
      again.reject(new Error('/api/library/libraries/7/scan: 500 locked')),
    )
    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await tick()
    await flush()
    expect(scanDrawn()).toBe(
      JSON.stringify([null, ['Never', 'Never'], 'free', 'free']),
    )
    expect(reportedPaths(host)).toEqual([])
    expect(config.scanStatus).not.toHaveBeenCalled()
    expect(onChanged).not.toHaveBeenCalled()
  })

  it('draws no report for a scan the kept status said was running when its poll’s read, sent before the read on a return answers, finds it over', async () => {
    reporting()
    vi.mocked(api.libraryScanStatus).mockResolvedValue(scanningIn(manga(), 1))
    const { host, again, onChanged } = await returned()
    const polled = deferred<api.LibraryScanStatus>()
    vi.mocked(api.libraryScanStatus).mockReturnValueOnce(polled.promise)
    await tick()
    expect(api.libraryScanStatus).toHaveBeenCalledTimes(3)
    // The store drops this answer, whatever its words: the poll's read is
    // newer.
    await flush(() => again.resolve(idle(7)))
    await flush(() => polled.resolve(idle(7)))
    await flush()
    await flush()
    expect(scanDrawn()).toBe(
      JSON.stringify([null, ['Never', 'Never'], 'free', 'free']),
    )
    expect(reportedPaths(host)).toEqual([])
    expect(config.scanStatus).not.toHaveBeenCalled()
    expect(onChanged).not.toHaveBeenCalled()
  })

  it('draws how far the scan the kept status said was running has gone once the read on a return lands', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue(kept())
    const { again } = await returned()
    await flush(() => again.resolve(scanningIn(manga(), 1)))
    expect(scanDrawn()).toBe(
      JSON.stringify([
        'A scan is running in this library… 3s',
        ['Scanning…', 'Never'],
        'held',
        'held',
      ]),
    )
  })

  it('keeps watching a scan it started when the read on a return, sent before the press, answers after it saying none was running', async () => {
    vi.mocked(api.startLibraryScan).mockResolvedValue(started)
    const { host, again, onChanged } = await returned()
    await press(button(host, 'scan-library'))
    const watching = JSON.stringify([
      'A scan is running in this library…',
      ['Scanning…', 'Scanning…'],
      'held',
      'held',
    ])
    expect(scanDrawn()).toBe(watching)

    // Whatever its words, the store drops this answer: the press marked the
    // status stale, which sent a newer read.
    await flush(() =>
      again.resolve({
        ...idle(7),
        dirs: manga().dirs.map((one) => ({ root_id: one.id, running: false })),
      }),
    )
    expect(scanDrawn()).toBe(watching)
    expect(onChanged).not.toHaveBeenCalled()
  })

  it.each([
    [
      'started',
      () =>
        Promise.resolve({ library_id: 7, status_url: '', root_ids: [1, 2] }),
    ],
    [
      'refused with a 409, which is proof of a scan',
      () =>
        Promise.reject(
          new Error(
            '/api/library/libraries/7/scan: 409 {"error":"a scan of /media/Manga/Chapters is already running"}',
          ),
        ),
    ],
  ] as const)(
    'draws as running, on a return before the first poll, a scan Scan library %s',
    async (_, answer) => {
      vi.mocked(api.startLibraryScan).mockImplementation(answer)
      const { host } = await shown()
      expect(scanDrawn()).toBe(
        JSON.stringify([null, ['Never', 'Never'], 'free', 'free']),
      )
      vi.mocked(api.libraryScanStatus).mockResolvedValue(
        scanningIn(manga(), 1, 2),
      )
      await press(button(host, 'scan-library'))
      await flush()
      act(() => rerender(<div />))
      const again = deferred<api.LibraryScanStatus>()
      vi.mocked(api.libraryScanStatus).mockReturnValueOnce(again.promise)
      const drawn = watchDrawn(scanDrawn)
      await shown()
      await flush(() => again.resolve(scanningIn(manga(), 1, 2)))
      drawn.stop()
      expect([...drawn.seen].filter((one) => one !== 'no body')).toEqual([
        JSON.stringify([
          'A scan is running in this library… 3s',
          ['Scanning…', 'Scanning…'],
          'held',
          'held',
        ]),
      ])
    },
  )
})

describe('LibraryPage, a directory another library holds too', () => {
  const MANGA = { id: 7, name: 'Manga' }
  const KIDS = { id: 3, name: 'Kids' }
  /** Kids, library 3, of `/m`, which Manga holds too, and a directory of its own. */
  const kids = () =>
    manga({
      id: 3,
      name: 'Kids',
      dirs: [dir(4, '/m', [MANGA, KIDS]), dir(5, '/media/Kids', [KIDS])],
    })
  /** Kids before the add: its own directory alone. */
  const kidsAlone = () =>
    manga({ id: 3, name: 'Kids', dirs: [dir(5, '/media/Kids', [KIDS])] })
  /** The add's answer for `path`, held before by the libraries named. */
  const answer = (path: string, also_in: string[]): config.Added => ({
    root: dir(9, path, [...(also_in.length > 0 ? [MANGA] : []), KIDS]),
    watching: also_in.length > 0 ? null : true,
    watch_reason: null,
    interval_secs: also_in.length > 0 ? null : 86400,
    also_in,
  })
  /** Kids once the list is read again after the add of `/m`. */
  const kidsHolding = () =>
    manga({
      id: 3,
      name: 'Kids',
      dirs: [dir(9, '/m', [MANGA, KIDS]), dir(5, '/media/Kids', [KIDS])],
    })
  async function added(path: string, also_in: string[]) {
    vi.mocked(api.addDir).mockResolvedValue(answer(path, also_in))
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

  it('says which library scans a directory it holds too, while that library’s scan walks it', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(3),
      running: true,
      elapsed_secs: 3,
      dirs: [
        { root_id: 4, running: true },
        { root_id: 5, running: false },
      ],
      scans: [{ library_id: 7, library: 'Manga', root_ids: [4] }],
    })
    const { host } = await shown(kids())
    expect(
      [...host.querySelectorAll('.library-page > .scanning')].map(
        (line) => line.textContent,
      ),
    ).toEqual(['Manga is scanning /m, which Kids holds too.'])
    expect(holdOf(button(host, 'scan-library'))).toBe('held')
  })

  it('says a directory it holds too is being scanned, where no library asked for the scan', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(3),
      running: true,
      elapsed_secs: 3,
      dirs: [
        { root_id: 4, running: true },
        { root_id: 5, running: false },
      ],
      scans: [{ library_id: null, library: null, root_ids: [4] }],
    })
    const { host } = await shown(kids())
    expect(
      [...host.querySelectorAll('.library-page > .scanning')].map(
        (line) => line.textContent,
      ),
    ).toEqual(['/m is being scanned, which Kids holds too.'])
  })

  it('says its own scan is running, and names no library, while its own scan walks a directory it holds too', async () => {
    // Its own scan of that directory alone, as Scan directory on its page
    // starts one: no other directory of its own is scanning to say so.
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(3),
      running: true,
      elapsed_secs: 3,
      dirs: [
        { root_id: 4, running: true },
        { root_id: 5, running: false },
      ],
      scans: [{ library_id: 3, library: 'Kids', root_ids: [4] }],
    })
    const { host } = await shown(kids())
    expect(
      [...host.querySelectorAll('.library-page > .scanning')].map(
        (line) => line.textContent,
      ),
    ).toEqual(['A scan is running in this library… 3s'])
  })

  it('says its own scan is running after the library is renamed, by its id', async () => {
    // The status names a library as it is called when read; the page's list
    // may still hold the name it had.
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(3),
      running: true,
      elapsed_secs: 3,
      dirs: [
        { root_id: 4, running: true },
        { root_id: 5, running: false },
      ],
      scans: [{ library_id: 3, library: 'Children', root_ids: [4] }],
    })
    const { host } = await shown(kids())
    expect(
      [...host.querySelectorAll('.library-page > .scanning')].map(
        (line) => line.textContent,
      ),
    ).toEqual(['A scan is running in this library… 3s'])
  })

  it('says its own scan is running where no library asked for the scan of a directory it alone holds', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(3),
      running: true,
      elapsed_secs: 3,
      dirs: [
        { root_id: 4, running: false },
        { root_id: 5, running: true },
      ],
      scans: [{ library_id: null, library: null, root_ids: [5] }],
    })
    const { host } = await shown(kids())
    expect(
      [...host.querySelectorAll('.library-page > .scanning')].map(
        (line) => line.textContent,
      ),
    ).toEqual(['A scan is running in this library… 3s'])
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
